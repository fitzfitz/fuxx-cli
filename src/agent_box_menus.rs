//! The agent box's menus: the folder picker and its panels, the model menu, and the frame and
//! rows they share (moved out of `agent_box.rs`, which keeps the pure part and the box itself).

use super::*;
use crate::hover_delay::{HoverDelay, Part};
use crate::usage_view::{heading, quiet};
use alacritty_terminal::vte::ansi::Rgb;
use gpui::{Animation, AnimationExt, FontWeight};

/// The folder picker's tile (the 2026-11-25 chores, mockup A) and its two-line row.
const FOLDER_TILE: f32 = 26.0;
const FOLDER_ROW_H: f32 = 42.0;

/// The turn ledger's popover's list, at most this tall (it scrolls past it).
const LEDGER_LIST_MAX: f32 = 360.0;

/// Where a menu's panel hangs against its anchor (the chip it is a child of): its left edge
/// `dx` from the anchor's left, or its right edge `dx` from the anchor's right (positive: further
/// right).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum MenuPlace {
    Left(f32),
    Right(f32),
}

impl AgentTab {
    /// The open menu's panel (`content` in it), hung above its chip (a child of it) and opening
    /// upward, in the tab menu's look; `side` (This Computer's folders) beside it. A click
    /// outside them closes it.
    pub(super) fn menu_frame(
        &self,
        content: Option<AnyElement>,
        side: Option<AnyElement>,
        width: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.menu_frame_at(content, side, width, MenuPlace::Left(0.), cx)
    }

    /// `menu_frame`, placed by `place` against its anchor; the window's edge still pushes it
    /// back in.
    pub(crate) fn menu_frame_at(
        &self,
        content: Option<AnyElement>,
        side: Option<AnyElement>,
        width: f32,
        place: MenuPlace,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (corner, dx) = match place {
            MenuPlace::Left(dx) => (Anchor::BottomLeft, dx),
            MenuPlace::Right(dx) => (Anchor::BottomRight, dx),
        };
        let ui = cx.global::<Appearance>().theme.ui();
        // No content: only the panel beside it (the usage button's).
        let dark = cx.global::<Appearance>().theme.is_dark();
        // The lead designer's "quiet glass" (2026-11-22): it rises 4 pt as it fades in.
        let panel = content.map(|content| {
            menu_surface(&ui, dark, width)
                .id("agent-box-menu")
                .occlude()
                .child(menu_inside(&ui).child(content))
                .with_animation(
                    "agent-box-menu-open",
                    Animation::new(std::time::Duration::from_millis(120))
                        .with_easing(crate::agent_top::ease_out_cubic),
                    |d, t| d.opacity(t).mt(px(4. * (1. - t))),
                )
        });
        div()
            .absolute()
            .top_0()
            .map(|d| match place {
                MenuPlace::Left(_) => d.left_0(),
                MenuPlace::Right(_) => d.right_0(),
            })
            .child(deferred(
                anchored()
                    .anchor(corner)
                    .offset(point(px(dx), px(-MENU_GAP)))
                    .snap_to_window_with_margin(px(MENU_EDGE))
                    .child(
                        // A press outside both panels closes it (one in the panel beside the
                        // menu is a pick, not "outside").
                        div()
                            .flex()
                            .items_end()
                            .gap(px(MENU_GAP))
                            .on_mouse_down_out(cx.listener(
                                |this, event: &MouseDownEvent, _, cx| {
                                    // The model menu's effort panel hangs beside its row, not
                                    // in here: a press on it is a pick.
                                    if this
                                        .effort_panel_at
                                        .get()
                                        .is_some_and(|b| b.contains(&event.position))
                                    {
                                        return;
                                    }
                                    if matches!(
                                        this.menu,
                                        Some(
                                            BoxMenu::Folder
                                                | BoxMenu::Model { .. }
                                                | BoxMenu::Usage
                                                | BoxMenu::Clock
                                                | BoxMenu::Plan
                                                | BoxMenu::Shells
                                                | BoxMenu::Subagents
                                                | BoxMenu::Mode
                                                | BoxMenu::Attach
                                                | BoxMenu::Ledger
                                        )
                                    ) {
                                        this.chip_closed_at = Some(event.position);
                                        this.chip_closed_menu = this.menu.clone();
                                    }
                                    this.close_menu(cx)
                                },
                            ))
                            .children(panel.map(|panel| self.marked("menu", div().child(panel))))
                            .children(side),
                    ),
            ))
            .into_any_element()
    }

    /// The folder chip's click: the picker opens over it, or (open) closes.
    pub(super) fn folder_clicked(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if matches!(self.menu, Some(BoxMenu::Folder)) {
            self.close_menu(cx);
        } else {
            self.open_folder_picker(window, cx);
        }
    }

    /// Open the folder picker (§2), its search taking the keyboard. The first time in an app
    /// run it reads the folders the agent knows and the agent's version (whether it can move a
    /// conversation; a failed read is tried again); a scene tab reads neither.
    pub fn open_folder_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Fixed once you have spoken (your words: "when we already make a prompt, disabled the
        // change project/folder").
        if !super::folder_changeable(self.provider.is_some(), self.spoken()) {
            return;
        }
        let Some(provider) = self.provider else {
            return;
        };
        if !self.no_process {
            crate::agent_catalog::load_known_folders(provider, cx);
        }
        let search = cx.new(|cx| NameInput::new(String::new(), window, cx));
        let subscriptions = vec![
            cx.observe(&search, |this, _, cx| this.refresh_picker(cx)),
            cx.subscribe_in(&search, window, Self::on_search),
        ];
        search.read(cx).focus_handle(cx).focus(window, cx);
        self.open_menu(BoxMenu::Folder, cx);
        self.picker = Some(FolderPicker {
            search,
            rows: Vec::new(),
            computer: None,
            selected: None,
            checks: FolderChecks::default(),
            _check: None,
            _subscriptions: subscriptions,
        });
        self.refresh_picker(cx);
    }

    /// The search box: Return picks the first folder listed; Esc (while it has the keyboard:
    /// losing it to a click on a row is no cancel) closes the picker.
    fn on_search(
        &mut self,
        search: &Entity<NameInput>,
        event: &NameInputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            NameInputEvent::Confirm(_) => {
                // The row ↑/↓ moved to, else the first folder.
                let first = self.picker.as_ref().and_then(|p| {
                    p.selected.and_then(|i| p.rows.get(i)).cloned().or_else(|| {
                        p.rows
                            .iter()
                            .find(|r| matches!(r, PickerRow::Folder { .. }))
                            .cloned()
                    })
                });
                if let Some(row) = first {
                    self.pick_row(row, cx);
                    self.keyboard_back(window, cx);
                }
            }
            NameInputEvent::Cancel => {
                if search.read(cx).focus_handle(cx).is_focused(window) {
                    self.close_menu(cx);
                    self.keyboard_back(window, cx);
                }
            }
        }
    }

    /// The keyboard goes back to the box's text once the picker has closed.
    fn keyboard_back(&self, window: &mut Window, cx: &mut Context<Self>) {
        if self.menu.is_none() {
            self.composer.read(cx).focus_handle(cx).focus(window, cx);
        }
    }

    /// Whether `folder` is there to start in, as the open picker's check found it (never read
    /// here: this runs on each keystroke). A scene tab reads no disk: its made-up folders are.
    fn folder_exists(&self, folder: &Path) -> bool {
        self.no_process
            || self
                .picker
                .as_ref()
                .is_some_and(|p| p.checks.exists(folder))
    }

    /// Check off the UI thread whether those of `folders` not checked yet are there; the picker's
    /// rows are made again when the answers are in (R10).
    fn check_folders(&mut self, folders: &[PathBuf], cx: &mut Context<Self>) {
        if self.no_process {
            return;
        }
        let Some(picker) = self.picker.as_mut() else {
            return;
        };
        let Some(asked) = picker.checks.begin(folders) else {
            return;
        };
        let job = cx.background_spawn(async move { FolderChecks::check(asked) });
        picker._check = Some(cx.spawn(async move |this, cx| {
            let found = job.await;
            this.update(cx, |this, cx| {
                if let Some(picker) = this.picker.as_mut() {
                    picker.checks.done(found);
                    this.refresh_picker(cx);
                }
            })
            .ok();
        }));
    }

    /// Make the open picker's rows again: for what is typed, from the folders fuxx knows
    /// (`KnownFolders`) and the agent's own (once read); This Computer's too while it is open.
    /// Only folders found there are listed (their check runs off the UI thread).
    pub(crate) fn refresh_picker(&mut self, cx: &mut Context<Self>) {
        let Some(picker) = &self.picker else {
            return;
        };
        let query = picker.search.read(cx).text().to_string();
        let open_beside = picker.computer.is_some();
        let known = cx.try_global::<KnownFolders>().cloned().unwrap_or_default();
        let agents = self.provider.and_then(|p| {
            cx.try_global::<Catalogs>()?
                .known_folders(p.info().name)
                .cloned()
        });
        let loading = matches!(agents, Some(Lookup::Loading));
        let agents = match agents {
            Some(Lookup::Ready(list)) => list,
            _ => Vec::new(),
        };
        let computer = computer_folders(&known.projects, &agents);
        let all: Vec<PathBuf> = known.recent.iter().chain(&computer).cloned().collect();
        self.check_folders(&all, cx);
        let home = crate::workspace::home_dir();
        let cwd = &self.start.cwd;
        let rows = picker_rows(
            &query,
            &known.recent,
            &computer,
            cwd,
            home.as_deref(),
            |f| self.folder_exists(f),
        );
        let beside = open_beside.then(|| {
            let rows = computer
                .iter()
                .filter(|f| self.folder_exists(f))
                .map(|f| folder_row(f, cwd, home.as_deref()))
                .collect();
            (rows, loading)
        });
        if let Some(picker) = self.picker.as_mut() {
            picker.rows = rows;
            picker.selected = None;
            picker.computer = beside;
        }
        // Rows no longer listed leave no probe behind.
        #[cfg(feature = "ui-shots")]
        self.menu_rows.borrow_mut().clear();
        cx.notify();
    }

    /// A row of the picker chosen (its click; Return in the search for the first folder).
    pub fn pick_row(&mut self, row: PickerRow, cx: &mut Context<Self>) {
        match row {
            PickerRow::Folder { path, .. } => self.choose_folder(path, cx),
            PickerRow::ThisComputer => {
                if let Some(picker) = self.picker.as_mut() {
                    picker.computer = match picker.computer {
                        Some(_) => None,
                        None => Some((Vec::new(), false)),
                    };
                }
                self.refresh_picker(cx);
            }
            PickerRow::NewProject => {
                self.close_menu(cx);
                // The folder panel, "New Folder" in it: the folder chosen or made is used.
                let picked = cx.prompt_for_paths(crate::workspace::folder_panel());
                cx.spawn(async move |this, cx| {
                    let folder = match picked.await {
                        Ok(Ok(Some(paths))) => paths.into_iter().next(),
                        Ok(Err(err)) => {
                            eprintln!("[fuxx] agent box: folder panel failed: {err}");
                            None
                        }
                        _ => None,
                    };
                    if let Some(folder) = folder {
                        this.update(cx, |this, cx| this.choose_folder(folder, cx))
                            .ok();
                    }
                })
                .detach();
            }
            PickerRow::NoProject => match crate::workspace::home_dir() {
                Some(home) => self.choose_folder(home, cx),
                None => self.close_menu(cx),
            },
            PickerRow::Heading(_) => {}
        }
    }

    /// `folder` chosen: the folder it is in already does nothing; before the first message the
    /// agent starts there (`choose`). After it the folder is fixed (the picker doesn't open).
    fn choose_folder(&mut self, folder: PathBuf, cx: &mut Context<Self>) {
        self.close_menu(cx);
        if folder != self.start.cwd && !self.spoken() {
            self.choose(Change::Folder(folder), cx);
        }
    }

    /// The folder menu's panel content, and This Computer's list beside it while open: the
    /// picker (search, rows).
    pub(super) fn folder_menu(&self, cx: &mut Context<Self>) -> (AnyElement, Option<AnyElement>) {
        let ui = cx.global::<Appearance>().theme.ui();
        let Some(picker) = &self.picker else {
            return (div().into_any_element(), None);
        };
        let empty = picker.search.read(cx).text().is_empty();
        let field = crate::overlay_panel::filter_box(&ui, &picker.search, empty, SEARCH);
        let selected = picker.selected;
        let rows: Vec<AnyElement> = picker
            .rows
            .iter()
            .enumerate()
            .map(|(i, row)| self.picker_row(("box-folder", i), row, selected == Some(i), &ui, cx))
            .collect();
        let list = div()
            .id("box-folder-rows")
            .flex()
            .flex_col()
            .max_h(px(MENU_LIST_MAX))
            .overflow_y_scroll()
            .children(rows);
        // What the keys do (the 2026-11-25 chores, mockup A).
        let footer = div()
            .flex()
            .gap(px(14.))
            .mt(px(2.))
            .pt(px(6.))
            .px(px(10.))
            .pb(px(2.))
            .border_t_1()
            .border_color(hsla(ui.divider))
            .text_size(px(11.))
            .text_color(hsla(ui.text_dim))
            .child("\u{2191}\u{2193} move")
            .child("\u{21b5} open")
            .child("esc close");
        let menu = div()
            .key_context("FolderPicker")
            .on_action(cx.listener(|this, _: &crate::agent_tab::FolderUp, _, cx| {
                this.picker_move(false, cx)
            }))
            .on_action(
                cx.listener(|this, _: &crate::agent_tab::FolderDown, _, cx| {
                    this.picker_move(true, cx)
                }),
            )
            .on_action(
                cx.listener(|this, _: &crate::agent_tab::FolderPick1, w, cx| {
                    this.pick_recent(1, w, cx)
                }),
            )
            .on_action(
                cx.listener(|this, _: &crate::agent_tab::FolderPick2, w, cx| {
                    this.pick_recent(2, w, cx)
                }),
            )
            .on_action(
                cx.listener(|this, _: &crate::agent_tab::FolderPick3, w, cx| {
                    this.pick_recent(3, w, cx)
                }),
            )
            .on_action(
                cx.listener(|this, _: &crate::agent_tab::FolderPick4, w, cx| {
                    this.pick_recent(4, w, cx)
                }),
            )
            .on_action(
                cx.listener(|this, _: &crate::agent_tab::FolderPick5, w, cx| {
                    this.pick_recent(5, w, cx)
                }),
            )
            .on_action(
                cx.listener(|this, _: &crate::agent_tab::FolderPick6, w, cx| {
                    this.pick_recent(6, w, cx)
                }),
            )
            .on_action(
                cx.listener(|this, _: &crate::agent_tab::FolderPick7, w, cx| {
                    this.pick_recent(7, w, cx)
                }),
            )
            .on_action(
                cx.listener(|this, _: &crate::agent_tab::FolderPick8, w, cx| {
                    this.pick_recent(8, w, cx)
                }),
            )
            .flex()
            .flex_col()
            .gap(px(4.))
            .child(field)
            .child(list)
            .child(footer)
            .into_any_element();
        let side = picker.computer.as_ref().map(|(rows, loading)| {
            let mut items: Vec<AnyElement> = rows
                .iter()
                .enumerate()
                .map(|(i, row)| self.picker_row((COMPUTER_ROWS, i), row, false, &ui, cx))
                .collect();
            if *loading {
                items.push(quiet("Loading\u{2026}", &ui));
            } else if items.is_empty() {
                items.push(quiet("No folders", &ui));
            }
            let panel = crate::overlay_panel::shell(&ui, MENU_WIDTH)
                .p(px(4.))
                .gap(px(0.))
                .occlude()
                .child(heading(THIS_COMPUTER.into(), &ui))
                .child(
                    div()
                        .id("box-computer-rows")
                        .flex()
                        .flex_col()
                        .max_h(px(MENU_LIST_MAX))
                        .overflow_y_scroll()
                        .children(items),
                );
            self.marked("computer", div().child(panel))
                .into_any_element()
        });
        (menu, side)
    }

    /// ↑/↓ in the picker's search: the row it moves to lights (`folder_picker::moved`).
    fn picker_move(&mut self, down: bool, cx: &mut Context<Self>) {
        if let Some(picker) = self.picker.as_mut() {
            picker.selected = crate::folder_picker::moved(&picker.rows, picker.selected, down);
            cx.notify();
        }
    }

    /// ⌘`n` in the picker's search: Recent's row `n` opens.
    fn pick_recent(&mut self, n: u8, window: &mut Window, cx: &mut Context<Self>) {
        let row = self.picker.as_ref().and_then(|p| {
            p.rows
                .iter()
                .find(|r| matches!(r, PickerRow::Folder { shortcut: Some(k), .. } if *k == n))
                .cloned()
        });
        if let Some(row) = row {
            self.pick_row(row, cx);
            self.keyboard_back(window, cx);
        }
    }

    /// One of the picker's rows, drawn (mockup A, the 2026-11-25 chores): a folder as a tile, its
    /// name over its place, ✓ or ⌘N on the right; an action with its icon. Its click picks it;
    /// `selected`: the row ↑/↓ moved to.
    fn picker_row(
        &self,
        id: (&'static str, usize),
        row: &PickerRow,
        selected: bool,
        ui: &UiColors,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        use crate::icons::{Icon, icon};
        let (glyph, label, more) = match row {
            PickerRow::Heading(h) => return heading((*h).into(), ui),
            PickerRow::Folder {
                shown,
                name,
                place,
                current,
                shortcut,
                ..
            } => {
                let home = name == "Home" && place == "~";
                return self.picker_folder_row(
                    id,
                    row.clone(),
                    (shown.clone(), name.clone(), place.clone()),
                    (*current, *shortcut, home, selected),
                    ui,
                    cx,
                );
            }
            PickerRow::ThisComputer => (Icon::FolderOpen, THIS_COMPUTER, true),
            PickerRow::NewProject => (Icon::Plus, NEW_PROJECT, false),
            PickerRow::NoProject => (Icon::Circle, NO_PROJECT, false),
        };
        let hover = ui.composer_pill_hover;
        let pick = row.clone();
        let el = div()
            .id(id)
            .relative()
            .h(px(ROW_H))
            .pl(px(10.))
            .pr(px(6.))
            .flex()
            .items_center()
            .gap(px(10.))
            .rounded(px(ROW_RADIUS))
            .cursor_pointer()
            .hover(move |s| s.bg(hsla(hover)))
            .when(selected, |d| d.bg(hsla(hover)))
            .child(div().w(px(FOLDER_TILE)).flex().justify_center().child(icon(
                glyph,
                px(14.),
                hsla(ui.text_dim),
            )))
            .child(div().flex_1().child(SharedString::from(label)))
            .when(more, |d| {
                d.child(icon(Icon::ChevronRight, px(14.), hsla(ui.text_dim)))
            })
            .on_click(cx.listener(move |this, _, window, cx| {
                this.pick_row(pick.clone(), cx);
                this.keyboard_back(window, cx);
            }));
        let probe = match id.0 {
            COMPUTER_ROWS => format!("{THIS_COMPUTER} \u{25b8} {label}"),
            _ => label.to_string(),
        };
        self.marked_row(probe, el).into_any_element()
    }

    /// A folder's row (mockup A): a tile (the accent's tint for the current one), its name in
    /// semibold over its place, ✓ when current else its ⌘N.
    fn picker_folder_row(
        &self,
        id: (&'static str, usize),
        row: PickerRow,
        (shown, name, place): (String, String, String),
        (current, shortcut, home, selected): (bool, Option<u8>, bool, bool),
        ui: &UiColors,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        use crate::icons::{Icon, icon};
        let hover = ui.composer_pill_hover;
        let tile = div()
            .flex_none()
            .size(px(FOLDER_TILE))
            .rounded(px(7.))
            .flex()
            .items_center()
            .justify_center()
            .bg(hsla(if current { ui.menu_current } else { hover }))
            .child(icon(
                if home { Icon::House } else { Icon::Folder },
                px(14.),
                hsla(if current { ui.accent } else { ui.text_dim }),
            ));
        let words = div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .child(
                div()
                    .truncate()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(SharedString::from(name)),
            )
            .when(!place.is_empty(), |d| {
                d.child(
                    div()
                        .truncate()
                        .text_size(px(11.5))
                        .text_color(hsla(ui.text_dim))
                        .child(SharedString::from(place)),
                )
            });
        let right = if current {
            Some(icon(Icon::Check, px(14.), hsla(ui.accent)).into_any_element())
        } else {
            shortcut.map(|n| {
                div()
                    .flex_none()
                    .px(px(4.))
                    .rounded(px(4.))
                    .border_1()
                    .border_color(hsla(ui.divider))
                    .text_size(px(11.))
                    .text_color(hsla(ui.text_dim))
                    .child(SharedString::from(format!("\u{2318}{n}")))
                    .into_any_element()
            })
        };
        let el = div()
            .id(id)
            .relative()
            .h(px(FOLDER_ROW_H))
            .pl(px(8.))
            .pr(px(8.))
            .flex()
            .items_center()
            .gap(px(10.))
            .rounded(px(ROW_RADIUS))
            .cursor_pointer()
            .hover(move |s| s.bg(hsla(hover)))
            .when(selected, |d| d.bg(hsla(hover)))
            .when(current, |d| {
                d.bg(hsla(ui.menu_current)).child(current_bar(ui))
            })
            .child(tile)
            .child(words)
            .children(right)
            .on_click(cx.listener(move |this, _, window, cx| {
                this.pick_row(row.clone(), cx);
                this.keyboard_back(window, cx);
            }));
        // Probed by its shown path, as before (the scenes find rows by it).
        let probe = match id.0 {
            COMPUTER_ROWS => format!("{THIS_COMPUTER} \u{25b8} {shown}"),
            _ => shown,
        };
        self.marked_row(probe, el).into_any_element()
    }

    /// The catalog lookup the model menu reads: what is known for the agent and folder (not yet
    /// asked: still loading).
    pub(crate) fn catalog_lookup(&self, cx: &App) -> Lookup<Catalog> {
        let found = self
            .provider
            .zip(cx.try_global::<Catalogs>())
            .and_then(|(p, catalogs)| catalogs.catalog(p.info().name, &self.start.cwd));
        found.cloned().unwrap_or(Lookup::Loading)
    }

    /// The start flags the model menu shows as current: the model the agent runs (named by the
    /// chip when none was chosen), with a model change waiting for the turn laid over it (so an
    /// effort picked meanwhile belongs to the model you picked, and composes with it).
    pub(crate) fn shown_start(&self, cx: &App) -> Start {
        let mut shown = self.start.clone();
        let Some(provider) = self.provider else {
            return shown;
        };
        if let Some(Change::Model { id, effort }) = self.waiting_model() {
            let (model, effort) = self.model_flags(provider, id, effort.as_deref(), cx);
            shown.model = Some(model);
            if effort.is_some() {
                shown.effort = effort;
            } else if self.model_choice(provider, id, cx).efforts.is_empty() {
                // A model with no efforts has none to show (not the last model's).
                shown.effort = None;
            }
        } else if shown.model.is_none()
            && let Some(catalog) = self.catalog(cx)
        {
            let label = chip_model_label(
                Some(catalog),
                &self.start,
                self.conversation.model(),
                provider,
            );
            shown.model = catalog
                .models
                .iter()
                .find(|m| m.label == label)
                .map(|m| m.id.clone());
        }
        shown
    }

    /// The model chip's click: the model menu opens over it, or (open) closes. Opening asks for
    /// the model list again when it could not be read (or was never asked).
    /// The usage button's click: its panel opens over it (read now unless fresh), or closes.
    /// The pointer arrived on (or left) the usage gauge or its panel: the panel opens after a
    /// rest (`hover_delay::OPEN_AFTER`) and closes once the pointer has left both.
    pub(super) fn usage_hovered(&mut self, part: Part, on: bool, cx: &mut Context<Self>) {
        self.hover_panel(
            |this| &mut this.usage_hover,
            part,
            on,
            |this, open, cx| {
                if open {
                    this.open_usage(cx);
                } else if matches!(this.menu, Some(BoxMenu::Usage)) {
                    this.close_menu(cx);
                }
            },
            cx,
        );
    }

    /// The plan's tasks (the 2026-11-24 chores), over the burger tab: each ✓ done (with its
    /// commit), ◐ being built or ○ next (the subagents have their own tab since chore 19).
    pub(super) fn plan_popover(
        &self,
        c: &crate::agent_tab::Colors,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        use crate::plan_progress::{TaskState, counts};
        let ui = cx.global::<Appearance>().theme.ui();
        let (done, all) = counts(&self.plan);
        let row =
            |mark: &'static str, mark_ink: Rgb, title: String, tail: Option<String>, ink: Rgb| {
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .h(px(26.))
                    .px(px(8.))
                    .child(
                        div()
                            .flex_none()
                            .w(px(14.))
                            .text_color(hsla(mark_ink))
                            .child(mark),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .truncate()
                            .text_color(hsla(ink))
                            .child(SharedString::from(title)),
                    )
                    .children(tail.map(|t| {
                        div()
                            .flex_none()
                            .font_features(crate::sidebar::tabular())
                            .text_size(px(11.))
                            .text_color(hsla(ui.text_dim))
                            .child(SharedString::from(t))
                    }))
            };
        // Each probed by its title (the scenes read the popover's rows).
        let tasks = self.plan.iter().map(|t| {
            let el = match &t.state {
                TaskState::Done { commit } => row(
                    "\u{2713}",
                    c.tool_ok,
                    t.title.clone(),
                    commit.clone(),
                    ui.text,
                ),
                TaskState::Building => {
                    row("\u{25d0}", c.todo_active, t.title.clone(), None, ui.text)
                }
                TaskState::Next => row("\u{25cb}", ui.text_dim, t.title.clone(), None, ui.text_dim),
            };
            self.marked_row(t.title.clone(), el)
        });
        let heading = |words: String| {
            div()
                .px(px(8.))
                .pt(px(6.))
                .pb(px(2.))
                .text_size(px(11.5))
                .text_color(hsla(ui.text_dim))
                .child(SharedString::from(words))
        };
        let content = div()
            .flex()
            .flex_col()
            .text_size(px(12.5))
            .child(heading(format!("Plan \u{b7} {done} of {all}")))
            .children(tasks)
            .into_any_element();
        let content = self
            .marked("plan-popover", div().child(content))
            .into_any_element();
        // Over the chat, its right edge on the box's (the 2026-11-25 chores: "should be left
        // aligned"): hung from the corner tabs' right edge (`corner_tabs`).
        self.menu_frame_at(
            Some(content),
            None,
            crate::agent_box::PLAN_WIDTH,
            MenuPlace::Right(crate::card_view::EDGE),
            cx,
        )
    }

    /// The clock's cost and context panel: as the gauge's (`usage_hovered`).
    pub(super) fn clock_hovered(&mut self, part: Part, on: bool, cx: &mut Context<Self>) {
        self.hover_panel(
            |this| &mut this.clock_hover,
            part,
            on,
            |this, open, cx| {
                if open {
                    this.open_menu(BoxMenu::Clock, cx);
                } else if matches!(this.menu, Some(BoxMenu::Clock)) {
                    this.close_menu(cx);
                }
            },
            cx,
        );
    }

    /// One hover-opened panel's timers (the gauge's, the clock's): `state` picks its
    /// `HoverDelay`; `changed` opens (true) or closes (false) the panel when a timer settles it.
    pub(super) fn hover_panel(
        &mut self,
        state: fn(&mut Self) -> &mut HoverDelay,
        part: Part,
        on: bool,
        changed: fn(&mut Self, bool, &mut Context<Self>),
        cx: &mut Context<Self>,
    ) {
        let hover = state(self);
        let wait = if on {
            hover
                .enter(part)
                .map(|t| (t, crate::hover_delay::OPEN_AFTER))
        } else {
            hover
                .leave(part)
                .map(|t| (t, crate::hover_delay::CLOSE_GRACE))
        };
        let Some((ticket, wait)) = wait else {
            return;
        };
        // The timer starts now, as the pointer moves, not when the task first runs.
        let timer = cx.background_executor().timer(wait);
        cx.spawn(async move |this, cx| {
            timer.await;
            this.update(cx, |this, cx| {
                let hover = state(this);
                if hover.settle(ticket) {
                    let open = hover.is_open();
                    changed(this, open, cx);
                }
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn model_clicked(&mut self, cx: &mut Context<Self>) {
        if matches!(self.menu, Some(BoxMenu::Model { .. })) {
            self.close_menu(cx);
            return;
        }
        self.open_model_menu(None, cx);
    }

    /// The model menu opens (with the efforts of model `efforts_of` beside it), in place of any
    /// other. Asks for the model list again when it could not be read (or was never asked).
    pub(crate) fn open_model_menu(&mut self, efforts_of: Option<usize>, cx: &mut Context<Self>) {
        if let Some(provider) = self.provider
            && !self.no_process
        {
            crate::agent_catalog::load_catalog(provider, self.start.cwd.clone(), cx);
        }
        self.open_menu(BoxMenu::Model { efforts_of }, cx);
    }

    /// Where the model the agent runs is in the model list, when it has efforts to choose
    /// from (`/effort` opens them): None while the list isn't read.
    pub(crate) fn running_model_place(&self, cx: &App) -> Option<usize> {
        let Lookup::Ready(catalog) = self.catalog_lookup(cx) else {
            return None;
        };
        let shown = self.shown_start(cx);
        let (at, _) = choice_of(&catalog, shown.model.as_deref()?, shown.effort.as_deref())?;
        (!catalog.models.get(at)?.efforts.is_empty()).then_some(at)
    }

    /// The model at `ix` of the open list chosen, with the effort it keeps or needs
    /// (`effort_on_switch`): the menu closes, the change is asked for (`choose`), nothing is sent.
    fn pick_model(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Lookup::Ready(catalog) = self.catalog_lookup(cx) else {
            return;
        };
        let Some(model) = catalog.models.get(ix) else {
            return;
        };
        let current = chip_effort(Some(&catalog), &self.shown_start(cx));
        let change = Change::Model {
            id: model.id.clone(),
            effort: effort_on_switch(model, current.as_deref()),
        };
        self.close_menu(cx);
        self.choose(change, cx);
        self.keyboard_back(window, cx);
    }

    /// `level` of the model at `ix` chosen (as for that model's row, with this effort).
    fn pick_effort(
        &mut self,
        ix: usize,
        level: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Lookup::Ready(catalog) = self.catalog_lookup(cx) else {
            return;
        };
        let Some(model) = catalog.models.get(ix) else {
            return;
        };
        let change = Change::Model {
            id: model.id.clone(),
            effort: Some(level),
        };
        self.close_menu(cx);
        self.choose(change, cx);
        self.keyboard_back(window, cx);
    }

    /// The model menu's panel content (the lead designer's second pass, 2026-11-22, "one list,
    /// one light"): "Model", then Default, a divider, each family's newest (and the current
    /// model, wherever it is: `model_groups`), a divider and "Older models" (closed at first)
    /// with the rest. The open model's efforts hang beside its row (`model_row`).
    pub(super) fn model_menu(&self, cx: &mut Context<Self>) -> AnyElement {
        let ui = cx.global::<Appearance>().theme.ui();
        let lookup = self.catalog_lookup(cx);
        let shown = self.shown_start(cx);
        let applies = self
            .provider
            .map_or(Applies::Live, |p| p.applies(ChangeKind::Model));
        let efforts_of = match &self.menu {
            Some(BoxMenu::Model { efforts_of }) => *efforts_of,
            _ => None,
        };
        // Set again as the effort panel is drawn.
        self.effort_panel_at.set(None);
        let rows = model_rows(&lookup, &shown, applies);
        // The models other than Default, by row: grouped when long (`model_groups`).
        let models: Vec<(usize, &str, bool)> = rows
            .iter()
            .enumerate()
            .filter_map(|(i, r)| match r {
                ModelRow::Model { label, current, .. } if !label.starts_with("Default") => {
                    Some((i, label.as_str(), *current))
                }
                _ => None,
            })
            .collect();
        let labels: Vec<&str> = models.iter().map(|m| m.1).collect();
        let current = models.iter().position(|m| m.2);
        let grouped = model_groups(&labels, current).map(|(top, older)| {
            let row = |ks: Vec<usize>| ks.into_iter().map(|k| models[k].0).collect::<Vec<_>>();
            (row(top), row(older))
        });
        let older_open = self.models_older_open.unwrap_or(false);
        let mut header = None;
        let mut list = Vec::new();
        let mut placed = false;
        for (i, row) in rows.iter().enumerate() {
            match row {
                ModelRow::Heading => header = Some(model_heading(&ui)),
                ModelRow::Note(note) => list.push(quiet(*note, &ui)),
                ModelRow::Model { label, .. } if label.starts_with("Default") => {
                    list.push(self.model_row(i, row, efforts_of, &ui, cx));
                    list.push(menu_rule(&ui));
                }
                ModelRow::Model { .. } => match &grouped {
                    None => list.push(self.model_row(i, row, efforts_of, &ui, cx)),
                    Some(_) if placed => {}
                    Some((top, older)) => {
                        placed = true;
                        for &r in top {
                            list.push(self.model_row(r, &rows[r], efforts_of, &ui, cx));
                        }
                        list.push(menu_rule(&ui));
                        list.push(self.older_row(older.len(), older_open, &ui, cx));
                        if older_open {
                            for &r in older {
                                list.push(self.model_row(r, &rows[r], efforts_of, &ui, cx));
                            }
                        }
                    }
                },
                ModelRow::NewConversation => list.push(self.menu_row(
                    ("box-model", i),
                    NEW_CONVERSATION.into(),
                    false,
                    false,
                    &ui,
                    cx,
                    |this, window, cx| {
                        this.close_menu(cx);
                        cx.emit(AgentTabEvent::NewConversationIn(this.start.cwd.clone()));
                        this.keyboard_back(window, cx);
                    },
                )),
            }
        }
        div()
            .flex()
            .flex_col()
            .text_size(px(crate::ui_tokens::text::MENU))
            .children(header)
            .child(
                div()
                    .id("box-model-rows")
                    .flex()
                    .flex_col()
                    .max_h(px(MODEL_LIST_MAX))
                    .overflow_y_scroll()
                    // A scroll closes the efforts: never beside a row scrolled away.
                    .on_scroll_wheel(cx.listener(|this, _: &gpui::ScrollWheelEvent, _, cx| {
                        if let Some(BoxMenu::Model { efforts_of }) = &mut this.menu
                            && efforts_of.is_some()
                        {
                            *efforts_of = None;
                            this.effort_switch = None;
                            cx.notify();
                        }
                    }))
                    .children(list),
            )
            .into_any_element()
    }

    /// The pointer rests on a model row (`shows`: its place when it has efforts): with no
    /// efforts open, its own open at once; with another's open, they swap after
    /// `EFFORT_SWITCH_DELAY` (on the way to the panel, the pointer may cross a row); back on the
    /// open one, nothing swaps.
    fn model_hovered(&mut self, shows: Option<usize>, cx: &mut Context<Self>) {
        let Some(BoxMenu::Model { efforts_of }) = &self.menu else {
            return;
        };
        let open = *efforts_of;
        if open == shows {
            self.effort_switch = None;
            return;
        }
        if open.is_none() {
            self.effort_switch = None;
            self.menu = Some(BoxMenu::Model { efforts_of: shows });
            cx.notify();
            return;
        }
        self.effort_switch = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(EFFORT_SWITCH_DELAY).await;
            this.update(cx, |this, cx| {
                if matches!(this.menu, Some(BoxMenu::Model { .. })) {
                    this.menu = Some(BoxMenu::Model { efforts_of: shows });
                    this.effort_switch = None;
                    cx.notify();
                }
            })
            .ok();
        }));
    }

    /// The efforts of the model at `ix`, beside its row (the lead designer's second pass): the
    /// panel's left 4 past the menu's right edge, its first level at the row's top; kept in the
    /// window (it moves up near the bottom). A 4 pt bridge leads to it from the menu. The
    /// running model's level ticked; another model's, the level a pick of it would use, ticked
    /// quieter.
    fn effort_panel(&self, ix: usize, ui: &UiColors, cx: &mut Context<Self>) -> AnyElement {
        let Lookup::Ready(catalog) = self.catalog_lookup(cx) else {
            return div().into_any_element();
        };
        let shown = self.shown_start(cx);
        let levels = effort_rows(&catalog, ix, &shown);
        let running = levels.iter().any(|(_, current)| *current);
        let would = catalog
            .models
            .get(ix)
            .filter(|_| !running)
            .and_then(|m| effort_on_switch(m, shown.effort.as_deref()));
        let items: Vec<AnyElement> = levels
            .into_iter()
            .enumerate()
            .map(|(i, (level, current))| {
                let check = if current {
                    Some(ui.menu_check)
                } else if would.as_deref() == Some(level.as_str()) {
                    Some(ui.text_dim)
                } else {
                    None
                };
                self.effort_row(i, ix, level, check, ui, cx)
            })
            .collect();
        let dark = cx.global::<Appearance>().theme.is_dark();
        let panel = menu_surface(ui, dark, EFFORT_WIDTH)
            .id("agent-box-effort")
            .child(
                menu_inside(ui)
                    .text_size(px(crate::ui_tokens::text::MENU))
                    .children(items),
            )
            .with_animation(
                "agent-box-effort-open",
                Animation::new(std::time::Duration::from_millis(90)),
                |d, t| d.opacity(t),
            );
        let at = self.effort_panel_at.clone();
        let bridge = div()
            .id("agent-box-effort-bridge")
            .relative()
            .pl(px(MENU_GAP))
            .occlude()
            .on_hover(cx.listener(|this, hovered: &bool, _, _| {
                if *hovered {
                    this.effort_switch = None;
                }
            }))
            .child(
                gpui::canvas(move |b, _, _| at.set(Some(b)), |_, _, _, _| {})
                    .absolute()
                    .size_full(),
            )
            .child(self.marked("effort", div().child(panel)));
        // From the row's top left (the menu's 1 pt rim and 4 pt inside in from its edge).
        div()
            .absolute()
            .top_0()
            .left_0()
            .child(
                deferred(
                    anchored()
                        .offset(point(px(MODEL_MENU_WIDTH - 5.), px(-5.)))
                        .snap_to_window_with_margin(px(MENU_EDGE))
                        .child(bridge),
                )
                .with_priority(1),
            )
            .into_any_element()
    }

    /// One level of the effort panel: the check column (`check`'s colour), its word.
    fn effort_row(
        &self,
        i: usize,
        ix: usize,
        level: String,
        check: Option<Rgb>,
        ui: &UiColors,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let hover = ui.composer_pill_hover;
        let word = effort_word(&level);
        let row = div()
            .id((EFFORT_ROWS, i))
            .relative()
            .h(px(MODEL_ROW_H))
            .pl(px(6.))
            .pr(px(8.))
            .flex()
            .items_center()
            .gap(px(6.))
            .rounded(px(MODEL_ROW_RADIUS))
            .cursor_pointer()
            .hover(move |s| s.bg(hsla(hover)))
            .child(check_col(check))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(SharedString::from(word.clone())),
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                this.pick_effort(ix, level.clone(), window, cx)
            }));
        self.marked_row(format!("Effort \u{25b8} {word}"), row)
            .into_any_element()
    }

    /// The mode in force: the chosen one, else the one the catalog says the agent runs, else
    /// the provider's default (as the chip says it).
    pub(super) fn mode_now<'a>(&'a self, cx: &'a App) -> Option<&'a str> {
        let provider = self.provider?;
        current_mode(
            self.mode_chosen(),
            self.catalog(cx).and_then(|c| c.mode.as_deref()),
            provider.info().default_mode,
        )
    }

    /// The mode menu's rows now.
    pub(super) fn mode_rows_now(&self, cx: &App) -> Vec<ModeRow> {
        let Some(provider) = self.provider else {
            return Vec::new();
        };
        mode_rows(
            provider.info().modes,
            self.mode_now(cx),
            provider.applies(ChangeKind::Mode),
        )
    }

    /// The mode chip's click: the mode menu opens over it, or (open) closes.
    pub(super) fn mode_clicked(&mut self, cx: &mut Context<Self>) {
        if matches!(self.menu, Some(BoxMenu::Mode)) {
            self.close_menu(cx);
        } else {
            self.open_menu(BoxMenu::Mode, cx);
        }
    }

    /// `+`'s click: the attach menu opens over it, or (open) closes.
    pub(super) fn attach_clicked(&mut self, cx: &mut Context<Self>) {
        if matches!(self.menu, Some(BoxMenu::Attach)) {
            self.close_menu(cx);
        } else {
            self.open_menu(BoxMenu::Attach, cx);
        }
    }

    /// The attach menu's rows (§8): `File…` and `Image…`.
    pub(super) fn attach_menu(&self, cx: &mut Context<Self>) -> AnyElement {
        let ui = cx.global::<Appearance>().theme.ui();
        let rows = [(FILE_ROW, false), (IMAGE_ROW, true)]
            .into_iter()
            .enumerate()
            .map(|(i, (label, images))| {
                let hover = ui.panel_hover;
                let row = div()
                    .id(("box-attach", i))
                    .h(px(ROW_H))
                    .px(px(8.))
                    .flex()
                    .items_center()
                    .rounded(px(ROW_RADIUS))
                    .cursor_pointer()
                    .hover(move |s| s.bg(hsla(hover)))
                    .child(label)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.pick_attachments(images, cx);
                        this.keyboard_back(window, cx);
                    }));
                self.marked_row(label.to_string(), row).into_any_element()
            });
        div().flex().flex_col().children(rows).into_any_element()
    }

    /// The turn ledger's popover (the agent activity chore): the files the turn changed (a
    /// click opens one's diff tab), those scripts touched, those it only read, its checks and,
    /// apart, its planned failures. Empty sections are left out.
    pub(crate) fn ledger_menu(&self, cx: &mut Context<Self>) -> AnyElement {
        use crate::agent_activity::{Activity, Check};
        let ui = cx.global::<Appearance>().theme.ui();
        let c = Colors::of(&cx.global::<Appearance>().theme);
        let Some(ledger) = self.ledger.as_ref() else {
            return div().into_any_element();
        };
        let cwd = self.start.cwd.clone();
        let shown = move |p: &Path| {
            p.strip_prefix(&cwd)
                .unwrap_or(p)
                .to_string_lossy()
                .into_owned()
        };
        let mut out: Vec<AnyElement> = Vec::new();
        // The 2026-11-25 chores (mockup A): a total, then the files under their folders (each
        // folder folds), an A/M/D badge and +/− on each; paths a script only named, and
        // folders, never show (`changed_files`).
        use crate::changed_files::{FileStatus, changed_groups, kept_paths, plain_groups, total};
        let facts = &self.ledger_facts;
        let ink_of = |status: FileStatus| match status {
            FileStatus::Added => c.tool_ok,
            FileStatus::Modified => c.meter_caution,
            FileStatus::Deleted => c.failed,
            FileStatus::Unchanged => ui.text_dim,
        };
        // The part its chip asked for (chore 22): `⧉ N changed` the files, `✓ checks` the
        // checks; both when neither did.
        use crate::agent_status_line::ChipKind;
        let show_files = self.ledger_part != Some(ChipKind::Checks);
        let show_checks = self.ledger_part != Some(ChipKind::Changed);
        let groups = if show_files {
            changed_groups(&ledger.changed, facts, &self.start.cwd)
        } else {
            Vec::new()
        };
        if !groups.is_empty() {
            let (files, plus, minus) = total(&groups);
            out.push(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .px(px(8.))
                    .pt(px(4.))
                    .pb(px(2.))
                    .font_features(crate::sidebar::tabular())
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(SharedString::from(
                                crate::git_stage::files(files) + " changed",
                            )),
                    )
                    .when(plus > 0, |d| {
                        d.child(
                            div()
                                .text_color(hsla(c.tool_ok))
                                .child(SharedString::from(format!("+{plus}"))),
                        )
                    })
                    .when(minus > 0, |d| {
                        d.child(
                            div()
                                .text_color(hsla(c.failed))
                                .child(SharedString::from(format!("\u{2212}{minus}"))),
                        )
                    })
                    .into_any_element(),
            );
        }
        let mut n: usize = 0;
        let mut section = |title: Option<&str>,
                           groups: Vec<crate::changed_files::Group>,
                           id: &'static str,
                           out: &mut Vec<AnyElement>,
                           cx: &mut Context<Self>| {
            if groups.is_empty() {
                return;
            }
            if let Some(title) = title {
                out.push(heading(title.into(), &ui));
            }
            for group in groups {
                let folded = self.ledger_folded.contains(&group.folder);
                if !group.folder.is_empty() {
                    let key = group.folder.clone();
                    let hover = ui.panel_hover;
                    n += 1;
                    out.push(
                        div()
                            .id((id, 10_000 + n))
                            .h(px(24.))
                            .px(px(6.))
                            .flex()
                            .items_center()
                            .gap(px(4.))
                            .rounded(px(ROW_RADIUS))
                            .cursor_pointer()
                            .hover(move |s| s.bg(hsla(hover)))
                            .text_size(px(11.5))
                            .text_color(hsla(ui.text_dim))
                            .child(crate::icons::icon(
                                if folded {
                                    crate::icons::Icon::ChevronRight
                                } else {
                                    crate::icons::Icon::ChevronDown
                                },
                                px(12.),
                                hsla(ui.text_dim),
                            ))
                            .child(
                                div()
                                    .min_w(px(0.))
                                    .truncate()
                                    .child(SharedString::from(group.folder.clone())),
                            )
                            .child(
                                div()
                                    .flex_none()
                                    .text_color(hsla(ui.text_faint))
                                    .child(SharedString::from(group.rows.len().to_string())),
                            )
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if !this.ledger_folded.remove(&key) {
                                    this.ledger_folded.insert(key.clone());
                                }
                                cx.notify();
                            }))
                            .into_any_element(),
                    );
                }
                if folded {
                    continue;
                }
                for file in group.rows {
                    n += 1;
                    let ink = ink_of(file.status);
                    let letter = match file.status {
                        FileStatus::Added => "A",
                        FileStatus::Modified => "M",
                        FileStatus::Deleted => "D",
                        FileStatus::Unchanged => "",
                    };
                    // A file only read has no diff: its click opens it.
                    let unchanged = file.status == FileStatus::Unchanged;
                    let counts = div()
                        .flex_none()
                        .flex()
                        .gap(px(6.))
                        .font_features(crate::sidebar::tabular())
                        .when_some(file.plus.filter(|p| *p > 0), |d, p| {
                            d.child(
                                div()
                                    .text_color(hsla(c.tool_ok))
                                    .child(SharedString::from(format!("+{p}"))),
                            )
                        })
                        .when_some(file.minus.filter(|m| *m > 0), |d, m| {
                            d.child(
                                div()
                                    .text_color(hsla(c.failed))
                                    .child(SharedString::from(format!("\u{2212}{m}"))),
                            )
                        });
                    let (path, hover) = (file.path.clone(), ui.panel_hover);
                    let label = shown(&file.path);
                    let el = div()
                        .id((id, n))
                        .h(px(ROW_H - 4.))
                        .pl(px(if group.folder.is_empty() { 8. } else { 22. }))
                        .pr(px(8.))
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .rounded(px(ROW_RADIUS))
                        .cursor_pointer()
                        .hover(move |s| s.bg(hsla(hover)))
                        .child(
                            div()
                                .flex_none()
                                .size(px(16.))
                                .rounded(px(4.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .when(!unchanged, |d| {
                                    d.bg(hsla(crate::theme::mix(ink, c.panel, 0.82)))
                                })
                                .text_size(px(11.))
                                .font_weight(FontWeight::BOLD)
                                .text_color(hsla(ink))
                                .child(letter),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.))
                                .truncate()
                                .text_color(hsla(ui.text))
                                .child(SharedString::from(file.name)),
                        )
                        .child(counts)
                        .tooltip(move |_, cx| {
                            cx.new(|_| {
                                crate::sidebar_toggle::Tip(
                                    if unchanged {
                                        "Open the file"
                                    } else {
                                        "Open the diff (\u{2325}-click: the file)"
                                    }
                                    .into(),
                                )
                            })
                            .into()
                        })
                        .on_click(cx.listener(move |this, event: &gpui::ClickEvent, _, cx| {
                            this.close_menu(cx);
                            if unchanged || event.modifiers().alt {
                                cx.emit(AgentTabEvent::OpenFile(path.clone()));
                            } else {
                                cx.emit(AgentTabEvent::OpenDiff(path.clone()));
                            }
                        }));
                    out.push(self.marked_row(label, el).into_any_element());
                }
            }
        };
        section(None, groups, "ledger-changed", &mut out, cx);
        // Touched by scripts, and read: only files (or files gone), grouped the same way, with
        // no counts; nothing until git and the disk have answered.
        if show_files {
            section(
                Some("Touched by scripts"),
                plain_groups(&kept_paths(&ledger.scripted, facts), facts, &self.start.cwd),
                "ledger-scripted",
                &mut out,
                cx,
            );
            section(
                Some("Read"),
                plain_groups(&kept_paths(&ledger.read, facts), facts, &self.start.cwd),
                "ledger-read",
                &mut out,
                cx,
            );
        }
        // The checks, one row per scope: its command, its runs' glyphs and its latest outcome
        // (red only while that run still fails for real); "P/T passed" beside the heading.
        if show_checks && !ledger.scopes.is_empty() {
            out.push(
                div()
                    .flex()
                    .items_center()
                    .child(div().flex_1().child(heading("Checks".into(), &ui)))
                    .child(
                        div()
                            .px(px(8.))
                            .text_size(px(crate::ui_tokens::text::META))
                            .text_color(hsla(ui.text_dim))
                            .font_features(crate::sidebar::tabular())
                            .child(SharedString::from(format!(
                                "{}/{} passed",
                                ledger.passed, ledger.total
                            ))),
                    )
                    .into_any_element(),
            );
        }
        for (n, scope) in ledger.scopes.iter().enumerate().filter(|_| show_checks) {
            let Some((latest, planned)) = scope.runs.last() else {
                continue;
            };
            let a = Activity {
                check: Some(Check {
                    what: scope.what,
                    scope: scope.scope.clone(),
                    outcome: latest.clone(),
                }),
                planned_failure: *planned,
                ..Default::default()
            };
            let words = crate::agent_step_words::outcome_phrase(&a).unwrap_or_else(|| "ran".into());
            let strip = scope.runs.iter().map(|(outcome, planned)| {
                use crate::agent_steps::{Glyph, check_glyph};
                let glyph = check_glyph(outcome, *planned);
                let ink = match glyph {
                    Glyph::Passed => c.bar_ok,
                    Glyph::Failed | Glyph::Planned => c.bar_failed,
                    _ => c.bar_dim,
                };
                match glyph {
                    Glyph::Check => div()
                        .flex()
                        .items_center()
                        .child(crate::agent_tab::tracks::hollow_ring(ink, 8.)),
                    _ => div()
                        .text_color(hsla(ink))
                        .child(crate::agent_tab::tracks::glyph_text(glyph)),
                }
            });
            let outcome_ink = if scope.failing() {
                c.bar_failed
            } else {
                ui.text_dim
            };
            out.push(
                // Probed by its command (the scenes read the popover's rows).
                self.marked_row(scope.scope.clone(), div())
                    .id(("ledger-check", n))
                    .h(px(ROW_H))
                    .px(px(8.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .rounded(px(ROW_RADIUS))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .truncate()
                            .text_color(hsla(ui.text))
                            .child(SharedString::from(scope.scope.clone())),
                    )
                    .child(
                        div()
                            .flex_none()
                            .flex()
                            .gap(px(2.))
                            .text_size(px(crate::ui_tokens::text::META))
                            .children(strip),
                    )
                    .child(
                        div()
                            .flex_none()
                            .text_color(hsla(outcome_ink))
                            .font_features(crate::sidebar::tabular())
                            .child(SharedString::from(words)),
                    )
                    .into_any_element(),
            );
        }
        div()
            .id("agent-ledger-list")
            .flex()
            .flex_col()
            .max_h(px(LEDGER_LIST_MAX))
            .overflow_y_scroll()
            .children(out)
            .into_any_element()
    }

    /// `File…` (any files) or `Image…` (`images`) chosen: the menu closes and the macOS file
    /// panel opens; what it picks becomes chips (`attach_files`, `attach_images`). Cancelled:
    /// nothing. Nothing is sent.
    fn pick_attachments(&mut self, images: bool, cx: &mut Context<Self>) {
        self.close_menu(cx);
        let picked = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Attach".into()),
        });
        cx.spawn(async move |this, cx| {
            let paths = match picked.await {
                Ok(Ok(Some(paths))) => paths,
                Ok(Err(err)) => {
                    eprintln!("[fuxx] agent box: file panel failed: {err}");
                    return;
                }
                _ => return,
            };
            this.update(cx, |this, cx| {
                if images {
                    this.attach_images(paths, cx)
                } else {
                    this.attach_files(paths, cx)
                }
            })
            .ok();
        })
        .detach();
    }

    /// The mode `id` chosen: the menu closes and the change is asked for (`choose`: live where
    /// the agent can, else a restart on the same conversation); nothing is sent.
    pub(crate) fn choose_mode(&mut self, id: &str, cx: &mut Context<Self>) {
        self.close_menu(cx);
        self.choose(Change::Mode(id.to_string()), cx);
    }

    /// ⇧Tab: the next mode after the one in force (`next_mode`), or after the one asked and not
    /// yet answered (pressed twice quickly), chosen as the menu would.
    pub(crate) fn cycle_mode(
        &mut self,
        _: &crate::agent_tab::CycleMode,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(provider) = self.provider else {
            return;
        };
        if let Some(id) = crate::agent_box::next_mode(
            provider.info().modes,
            self.mode_asked().or(self.mode_now(cx)),
        ) {
            self.choose_mode(id, cx);
        }
    }

    /// The mode menu's panel content (§6): each mode's name, ✓ on the one in force, and what
    /// it does in grey under it.
    pub(super) fn mode_menu(&self, cx: &mut Context<Self>) -> AnyElement {
        let ui = cx.global::<Appearance>().theme.ui();
        let mut items = Vec::new();
        for (i, row) in self.mode_rows_now(cx).iter().enumerate() {
            match row {
                ModeRow::Heading => items.push(heading(MODE_HEADING.into(), &ui)),
                ModeRow::Note(note) => items.push(quiet(*note, &ui)),
                ModeRow::Mode {
                    id,
                    label,
                    about,
                    current,
                } => items.push(self.mode_row(i, id, label, about, *current, &ui, cx)),
                ModeRow::NewConversation => items.push(self.menu_row(
                    ("box-mode", i),
                    NEW_CONVERSATION.into(),
                    false,
                    false,
                    &ui,
                    cx,
                    |this, window, cx| {
                        this.close_menu(cx);
                        cx.emit(AgentTabEvent::NewConversationIn(this.start.cwd.clone()));
                        this.keyboard_back(window, cx);
                    },
                )),
            }
        }
        div()
            .flex()
            .flex_col()
            .gap(px(2.))
            .children(items)
            .into_any_element()
    }

    /// One mode of the menu (the designer's details): its icon in a 32 pt column, its name and
    /// its description (two lines at most) under it, a check on the right when in force.
    #[allow(clippy::too_many_arguments)]
    fn mode_row(
        &self,
        i: usize,
        id: &'static str,
        label: &'static str,
        about: &'static str,
        current: bool,
        ui: &UiColors,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        use crate::icons::{Icon, icon};
        let hover = ui.panel_hover;
        let row = div()
            .id(("box-mode", i))
            .p(px(8.))
            .flex()
            .items_start()
            .rounded(px(6.))
            .cursor_pointer()
            .hover(move |s| s.bg(hsla(hover)))
            .child(div().flex_none().w(px(32.)).pt(px(1.)).child(icon(
                mode_icon(id),
                px(16.),
                hsla(ui.text_dim),
            )))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(div().text_size(px(13.)).child(label))
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(hsla(ui.text_dim))
                            .line_clamp(2)
                            .child(about),
                    ),
            )
            .child(
                div()
                    .flex_none()
                    .w(px(14.))
                    .ml(px(8.))
                    .pt(px(1.))
                    .when(current, |d| {
                        d.child(icon(Icon::Check, px(14.), hsla(ui.text)))
                    }),
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                this.choose_mode(id, cx);
                this.keyboard_back(window, cx);
            }));
        self.marked_row(label.to_string(), row).into_any_element()
    }

    /// The usage panel opens over its button (and `/usage`), and the agent's usage is
    /// read now unless a reading under a minute old is there (or on its way). Never read while
    /// drawing. A scene tab only marks it loading: it runs no process.
    pub(crate) fn open_usage(&mut self, cx: &mut Context<Self>) {
        self.open_menu(BoxMenu::Usage, cx);
        let Some(provider) = self.provider.filter(|p| p.usage_probe().is_some()) else {
            return;
        };
        if self.no_process {
            let agent = provider.info().name;
            cx.update_default_global(|c: &mut Catalogs, _| {
                c.begin_usage(agent, std::time::Instant::now())
            });
        } else {
            crate::agent_catalog::load_usage(provider, self.start.cwd.clone(), cx);
        }
    }

    /// The View Usage lines for now: the agent's reading and what it said of its limits.
    pub(crate) fn usage_rows_now(&self, cx: &App) -> Vec<UsageRow> {
        let Some(provider) = self.provider else {
            return Vec::new();
        };
        let available = provider
            .usage_probe()
            .is_none()
            .then(|| provider.info().label);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        let offset = crate::cli::local_utc_offset(now as u64 * 1000);
        crate::usage_view::agent_rows(
            provider.info().name,
            available,
            cx.try_global::<Catalogs>(),
            now,
            offset,
        )
    }

    /// The usage panel over its button (§5): per group its name, each limit's name,
    /// `N% left` with a ring and `Resets …`, what the agent said of it underneath.
    pub(super) fn usage_panel(&self, ui: &UiColors, cx: &App) -> AnyElement {
        // The status bar's agent detail draws the same (`usage_view::limits_panel`).
        let ink = crate::usage_view::UsageInk::of(&cx.global::<Appearance>().theme);
        let items =
            crate::usage_view::limit_items(self.usage_rows_now(cx), ui, &ink, |name, row| {
                self.marked_row(format!("Usage \u{25b8} {name}"), row)
                    .into_any_element()
            });
        let panel = crate::usage_view::limits_panel("agent-usage", None, items, ui).occlude();
        self.marked("usage", div().child(panel)).into_any_element()
    }

    /// One model of the menu (the lead designer's second pass, 2026-11-22): the check column
    /// (✓ on the current one), the family (medium on the current one) and its version quiet,
    /// "recommended" or the current effort quiet on the right, and a › on a model with efforts.
    /// One fill: under the pointer, or while its efforts are open (they hang beside it). A
    /// click on its name picks it; resting on it opens its efforts (`model_hovered`).
    fn model_row(
        &self,
        i: usize,
        row: &ModelRow,
        efforts_of: Option<usize>,
        ui: &UiColors,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let ModelRow::Model {
            ix,
            label,
            effort,
            current,
            has_efforts,
        } = row
        else {
            return div().into_any_element();
        };
        use crate::icons::{Icon, icon};
        let (ix, hover, current) = (*ix, ui.composer_pill_hover, *current);
        let open = efforts_of == Some(ix);
        // "Default (recommended)" reads "Default" with "recommended" quiet beside it.
        let (shown, aside) = match label.strip_suffix(" (recommended)") {
            Some(base) => (base.to_string(), Some("recommended".to_string())),
            None => (label.clone(), None),
        };
        let (family, version) = crate::agent_box::split_label(&shown);
        let (family, version) = (family.to_string(), version.map(str::to_string));
        let small = |words: String| {
            div()
                .flex_none()
                .text_size(px(crate::ui_tokens::text::MENU_META))
                .text_color(hsla(ui.text_dim))
                .child(SharedString::from(words))
        };
        let name = div()
            .id(("box-model-pick", i))
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .items_center()
            .gap(px(6.))
            .cursor_pointer()
            .child(check_col(current.then_some(ui.menu_check)))
            .child(
                div()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .child(
                        div()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .when(current, |d| d.font_weight(FontWeight::MEDIUM))
                            .child(SharedString::from(family)),
                    )
                    .children(version.map(|v| {
                        div()
                            .flex_none()
                            .ml(px(4.))
                            .text_color(hsla(ui.text_dim))
                            .child(SharedString::from(v))
                    })),
            )
            .children(aside.map(small))
            .child(div().flex_1())
            .children(effort.as_deref().map(|level| small(effort_word(level))))
            .on_click(cx.listener(move |this, _, window, cx| this.pick_model(ix, window, cx)));
        let more = div()
            .id(("box-model-more", i))
            .flex_none()
            .w(px(CHEVRON_COL))
            .h_full()
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .child(icon(Icon::ChevronRight, px(12.), hsla(ui.text_dim)))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.effort_switch = None;
                this.open_menu(
                    BoxMenu::Model {
                        efforts_of: Some(ix),
                    },
                    cx,
                )
            }));
        let shows = has_efforts.then_some(ix);
        let row = div()
            .id(("box-model", i))
            .relative()
            .h(px(MODEL_ROW_H))
            .pl(px(6.))
            .pr(px(8.))
            .flex()
            .items_center()
            .gap(px(6.))
            .rounded(px(MODEL_ROW_RADIUS))
            .hover(move |s| s.bg(hsla(hover)))
            .when(open, move |d| d.bg(hsla(hover)))
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                if *hovered {
                    this.model_hovered(shows, cx);
                }
            }))
            .child(name)
            .when(*has_efforts, |d| d.child(more))
            .when(open, |d| d.child(self.effort_panel(ix, ui, cx)));
        self.marked_row(label.clone(), row).into_any_element()
    }

    /// The "Older models  N ›" row (the lead designer's second pass): opens or closes the older
    /// models in place; closed each time the menu opens.
    fn older_row(
        &self,
        count: usize,
        open: bool,
        ui: &UiColors,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        use crate::icons::{Icon, icon};
        let hover = ui.composer_pill_hover;
        let row = div()
            .id("box-model-older")
            .relative()
            .h(px(MODEL_ROW_H))
            .pl(px(6.))
            .pr(px(8.))
            .rounded(px(MODEL_ROW_RADIUS))
            .flex()
            .items_center()
            .gap(px(6.))
            .cursor_pointer()
            .hover(move |s| s.bg(hsla(hover)))
            .text_color(hsla(ui.text_dim))
            .child(check_col(None))
            .child(div().flex_1().child("Older models"))
            .child(
                div()
                    .text_size(px(crate::ui_tokens::text::MENU_META))
                    .child(SharedString::from(count.to_string())),
            )
            .child(
                div()
                    .flex_none()
                    .w(px(CHEVRON_COL))
                    .flex()
                    .justify_center()
                    .child(icon(
                        if open {
                            Icon::ChevronDown
                        } else {
                            Icon::ChevronRight
                        },
                        px(12.),
                        hsla(ui.text_dim),
                    )),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.models_older_open = Some(!open);
                cx.notify();
            }));
        self.marked_row("Older".to_string(), row).into_any_element()
    }

    /// A menu row: a ✓ when `check`, its words (cut at the start when long: a path keeps its
    /// folder's name), `▸` when `more` opens beside it; `pick` on its click.
    #[allow(clippy::too_many_arguments)]
    fn menu_row(
        &self,
        id: (&'static str, usize),
        label: String,
        check: bool,
        more: bool,
        ui: &UiColors,
        cx: &mut Context<Self>,
        pick: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) -> AnyElement {
        let hover = ui.composer_pill_hover;
        let row = div()
            .id(id)
            .relative()
            .h(px(ROW_H))
            .pl(px(10.))
            .pr(px(6.))
            .flex()
            .items_center()
            .gap(px(6.))
            .rounded(px(ROW_RADIUS))
            .cursor_pointer()
            .hover(move |s| s.bg(hsla(hover)))
            // The current one (the lead designer's spec): the accent's tint and bar.
            .when(check, |d| {
                d.bg(hsla(ui.menu_current)).child(current_bar(ui))
            })
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis_start()
                    .child(SharedString::from(label.clone())),
            )
            // The check on the right, in the accent (the lead designer's spec, 2026-11-22).
            .when(check, |d| {
                d.child(crate::icons::icon(
                    crate::icons::Icon::Check,
                    px(14.),
                    hsla(ui.accent),
                ))
            })
            .when(more, |d| {
                d.child(crate::icons::icon(
                    crate::icons::Icon::ChevronRight,
                    px(14.),
                    hsla(ui.text_dim),
                ))
            })
            .on_click(cx.listener(move |this, _, window, cx| pick(this, window, cx)));
        // A row beside the menu (This Computer's) is probed apart from the menu's own (the
        // efforts' are marked by `effort_row`).
        let probe = match id.0 {
            COMPUTER_ROWS => format!("{THIS_COMPUTER} \u{25b8} {label}"),
            _ => label,
        };
        self.marked_row(probe, row).into_any_element()
    }

    /// `el`, and under `ui-shots` a probe that keeps where it was laid out by its `label`.
    pub(crate) fn marked_row<E: ParentElement>(&self, label: String, el: E) -> E {
        #[cfg(feature = "ui-shots")]
        {
            let rows = self.menu_rows.clone();
            el.child(
                gpui::canvas(
                    move |bounds, _, _| {
                        rows.borrow_mut().insert(label.clone(), bounds);
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
        }
        #[cfg(not(feature = "ui-shots"))]
        {
            let _ = label;
            el
        }
    }
}

/// A box menu's surface (the lead designer's spec, 2026-11-22, "quiet glass"): the menu's fill
/// in a 1 pt rim lit from above (the chat box's edge), radius 13 / 12, resting on two shadows.
fn menu_surface(ui: &UiColors, dark: bool, width: f32) -> Div {
    let shadow = |y: f32, blur: f32, a: f32| gpui::BoxShadow {
        color: gpui::Hsla { a, ..gpui::black() },
        offset: point(px(0.), px(y)),
        blur_radius: px(blur),
        spread_radius: px(0.),
        inset: false,
    };
    div()
        .w(px(width))
        .max_w_full()
        .p(px(1.))
        .rounded(px(13.))
        .bg(gpui::linear_gradient(
            180.,
            gpui::linear_color_stop(hsla(ui.composer_glow_top), 0.),
            gpui::linear_color_stop(hsla(ui.composer_glow_bottom), 0.6),
        ))
        .shadow(vec![
            shadow(1., 2., if dark { 0.30 } else { 0.06 }),
            shadow(12., 32., if dark { 0.40 } else { 0.12 }),
        ])
        .text_size(px(crate::ui_tokens::text::NAME))
        .text_color(hsla(ui.text))
}

/// The surface's inside: the menu's fill, 4 in.
fn menu_inside(ui: &UiColors) -> Div {
    div()
        .w_full()
        .rounded(px(12.))
        .bg(hsla(ui.composer_inner))
        .p(px(4.))
        .flex()
        .flex_col()
}

/// The current row's mark (the lead designer's spec): a 2 × 14 accent bar at its left.
fn current_bar(ui: &UiColors) -> Div {
    div()
        .absolute()
        .left(px(3.))
        .top(px((ROW_H - 14.) / 2.))
        .w(px(2.))
        .h(px(14.))
        .rounded(px(1.))
        .bg(hsla(ui.accent))
}

/// A model menu row's check column (16 wide): a ✓ in `ink`, or empty.
fn check_col(ink: Option<Rgb>) -> Div {
    div()
        .flex_none()
        .w(px(CHECK_COL))
        .flex()
        .items_center()
        .justify_center()
        .children(ink.map(|ink| crate::icons::icon(crate::icons::Icon::Check, px(12.), hsla(ink))))
}

/// The model menu's divider: a 1 pt `menu_rule` line, 8 in, 4 above and below.
fn menu_rule(ui: &UiColors) -> AnyElement {
    div()
        .h(px(1.))
        .mx(px(8.))
        .my(px(4.))
        .bg(hsla(ui.menu_rule))
        .into_any_element()
}

/// The model menu's heading: "Model", 24 tall, small and quiet.
fn model_heading(ui: &UiColors) -> AnyElement {
    div()
        .h(px(24.))
        .px(px(8.))
        .flex()
        .items_center()
        .text_size(px(crate::ui_tokens::text::MENU_META))
        .text_color(hsla(ui.text_dim))
        .child(MODEL_HEADING)
        .into_any_element()
}
