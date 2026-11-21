//! The agent box's menus: the folder picker and its panels, the model menu, and the frame and
//! rows they share (moved out of `agent_box.rs`, which keeps the pure part and the box itself).

use super::*;
use crate::usage_view::{heading, quiet, top_row};

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
        let ui = cx.global::<Appearance>().theme.ui();
        // No content: only the panel beside it (the usage button's).
        let panel = content.map(|content| {
            crate::overlay_panel::shell(&ui, width)
                .id("agent-box-menu")
                .p(px(4.))
                .gap(px(0.))
                .occlude()
                .child(content)
        });
        div()
            .absolute()
            .top_0()
            .left_0()
            .child(deferred(
                anchored()
                    .anchor(Anchor::BottomLeft)
                    .offset(point(px(0.), px(-MENU_GAP)))
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
                                    if matches!(
                                        this.menu,
                                        Some(
                                            BoxMenu::Folder
                                                | BoxMenu::Model { .. }
                                                | BoxMenu::Usage
                                                | BoxMenu::Mode
                                                | BoxMenu::Attach
                                        )
                                    ) {
                                        this.chip_closed_at = Some(event.position);
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
                let first = self.picker.as_ref().and_then(|p| {
                    p.rows
                        .iter()
                        .find(|r| matches!(r, PickerRow::Folder { .. }))
                        .cloned()
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
        let rows: Vec<AnyElement> = picker
            .rows
            .iter()
            .enumerate()
            .map(|(i, row)| self.picker_row(("box-folder", i), row, &ui, cx))
            .collect();
        let list = div()
            .id("box-folder-rows")
            .flex()
            .flex_col()
            .max_h(px(MENU_LIST_MAX))
            .overflow_y_scroll()
            .children(rows);
        let menu = div()
            .flex()
            .flex_col()
            .gap(px(4.))
            .child(field)
            .child(list)
            .into_any_element();
        let side = picker.computer.as_ref().map(|(rows, loading)| {
            let mut items: Vec<AnyElement> = rows
                .iter()
                .enumerate()
                .map(|(i, row)| self.picker_row((COMPUTER_ROWS, i), row, &ui, cx))
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

    /// One of the picker's rows, drawn; its click picks it.
    fn picker_row(
        &self,
        id: (&'static str, usize),
        row: &PickerRow,
        ui: &UiColors,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (label, check, more) = match row {
            PickerRow::Heading(h) => return heading((*h).into(), ui),
            PickerRow::Folder { shown, current, .. } => (shown.clone(), *current, false),
            PickerRow::ThisComputer => (THIS_COMPUTER.to_string(), false, true),
            PickerRow::NewProject => (NEW_PROJECT.to_string(), false, false),
            PickerRow::NoProject => (NO_PROJECT.to_string(), false, false),
        };
        let row = row.clone();
        self.menu_row(id, label, check, more, ui, cx, move |this, window, cx| {
            this.pick_row(row.clone(), cx);
            this.keyboard_back(window, cx);
        })
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
    pub(super) fn usage_clicked(&mut self, cx: &mut Context<Self>) {
        if matches!(self.menu, Some(BoxMenu::Usage)) {
            self.close_menu(cx);
        } else {
            self.open_usage(cx);
        }
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

    /// The model menu's panel content, and the open model's effort list beside it.
    pub(super) fn model_menu(&self, cx: &mut Context<Self>) -> (AnyElement, Option<AnyElement>) {
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
        let mut top = Vec::new();
        let mut list = Vec::new();
        for (i, row) in model_rows(&lookup, &shown, applies).iter().enumerate() {
            match row {
                ModelRow::Heading => top.push(heading(MODEL_HEADING.into(), &ui)),
                ModelRow::Note(note) => list.push(quiet(*note, &ui)),
                ModelRow::Model { label, .. } => {
                    list.push(self.model_row(i, row, efforts_of, &ui, cx));
                    // A line under "Default" (the designer's details).
                    if label.starts_with("Default") {
                        list.push(divider(&ui));
                    }
                }
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
        let menu = div()
            .flex()
            .flex_col()
            .children(top)
            .child(
                div()
                    .id("box-model-rows")
                    .flex()
                    .flex_col()
                    .max_h(px(MENU_LIST_MAX))
                    .overflow_y_scroll()
                    .children(list),
            )
            .into_any_element();
        let side = match (efforts_of, &lookup) {
            (Some(ix), Lookup::Ready(catalog)) => {
                let items: Vec<AnyElement> = effort_rows(catalog, ix, &shown)
                    .into_iter()
                    .enumerate()
                    .map(|(i, (level, current))| {
                        self.menu_row(
                            (EFFORT_ROWS, i),
                            effort_word(&level),
                            current,
                            false,
                            &ui,
                            cx,
                            move |this, window, cx| this.pick_effort(ix, level.clone(), window, cx),
                        )
                    })
                    .collect();
                let panel = crate::overlay_panel::shell(&ui, EFFORT_WIDTH)
                    .p(px(4.))
                    .gap(px(0.))
                    .occlude()
                    .children(items);
                Some(self.marked("effort", div().child(panel)).into_any_element())
            }
            _ => None,
        };
        (menu, side)
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
        let agent = provider.info().name;
        let catalogs = cx.try_global::<Catalogs>();
        // The last reading, however old: the panel never swaps numbers for `Checking…`; a
        // reopening reads again.
        // M11ap: the live usage (the last good reading, freshened by what the agent said since),
        // else what the reading is doing (checking, failed).
        let live = catalogs
            .and_then(|c| c.live_usage(agent))
            .map(Lookup::Ready);
        let found = live
            .as_ref()
            .or_else(|| catalogs.and_then(|c| c.usage_last(agent)));
        let limits = catalogs.map_or(&[][..], |c| c.limits(agent));
        let available = provider
            .usage_probe()
            .is_none()
            .then(|| provider.info().label);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        let offset = crate::cli::local_utc_offset(now as u64 * 1000);
        let mut rows = usage_rows(found, limits, available, now, offset);
        // Kept numbers whose refresh failed say so, and how old they are (the review's 5).
        let failed = matches!(
            catalogs.and_then(|c| c.usage_last(agent)),
            Some(Lookup::Failed(_))
        );
        let ago = catalogs.and_then(|c| c.usage_ready_at(agent)).map(|at| {
            std::time::Instant::now()
                .saturating_duration_since(at)
                .as_secs()
        });
        if live.is_some()
            && let Some(note) = crate::agent_catalog::stale_note(failed, ago)
        {
            rows.push(UsageRow::Quiet(note));
        }
        rows
    }

    /// The usage panel over its button (§5): per group its name, each limit's name,
    /// `N% left` with a ring and `Resets …`, what the agent said of it underneath.
    pub(super) fn usage_panel(&self, ui: &UiColors, cx: &App) -> AnyElement {
        // A "Top …" list once (the agent's text repeats them for the week: the session's
        // first), as a heading and up to three one-line rows (the designer's details).
        let mut tops_seen: Vec<String> = Vec::new();
        let mut items: Vec<AnyElement> = Vec::new();
        for row in self.usage_rows_now(cx) {
            match row {
                UsageRow::Heading(name) => items.push(heading(name, ui)),
                UsageRow::Quiet(text) => items.push(quiet(text, ui)),
                UsageRow::Text(text) => match top_list(&text) {
                    Some((title, list)) => {
                        if tops_seen.contains(&title) {
                            continue;
                        }
                        tops_seen.push(title.clone());
                        items.push(heading(title, ui));
                        items.extend(list.into_iter().map(|(name, pct)| top_row(name, pct, ui)));
                    }
                    None => items.push(
                        div()
                            .px(px(8.))
                            .py(px(4.))
                            .text_color(hsla(ui.text_dim))
                            .child(SharedString::from(text))
                            .into_any_element(),
                    ),
                },
                UsageRow::Limit {
                    name,
                    used_pct,
                    resets,
                    note,
                } => items.push(self.limit_row(name, used_pct, resets, note, ui, cx)),
            }
        }
        let panel = crate::overlay_panel::shell(ui, USAGE_WIDTH)
            .p(px(4.))
            .gap(px(0.))
            .occlude()
            .child(
                div()
                    .id("agent-usage")
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .max_h(px(USAGE_MAX_H))
                    .overflow_y_scroll()
                    .children(items),
            );
        self.marked("usage", div().child(panel)).into_any_element()
    }

    /// One limit of the usage panel (`usage_view::limit_row`), marked for the scenes.
    fn limit_row(
        &self,
        name: String,
        used_pct: Option<u8>,
        resets: Option<String>,
        note: Option<String>,
        ui: &UiColors,
        cx: &App,
    ) -> AnyElement {
        let ink = crate::usage_view::UsageInk::of(&cx.global::<Appearance>().theme);
        let row = crate::usage_view::limit_row(name.clone(), used_pct, resets, note, ui, &ink);
        self.marked_row(format!("Usage \u{25b8} {name}"), row)
            .into_any_element()
    }

    /// One model of the menu: ✓ when current, its name, its effort in grey (the current one's),
    /// and `▸` (when it has efforts), which opens its effort list beside the menu. A click on
    /// the name picks the model.
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
        let (ix, hover) = (*ix, ui.panel_hover);
        let group = SharedString::from(format!("box-model-row-{i}"));
        // "Default (recommended)" reads "Default" with "recommended" quiet beside it.
        let (shown, aside) = match label.strip_suffix(" (recommended)") {
            Some(base) => (base.to_string(), Some("recommended")),
            None => (label.clone(), None),
        };
        let name = div()
            .id(("box-model-pick", i))
            .flex_1()
            .min_w_0()
            .h_full()
            .pl(px(8.))
            .flex()
            .items_center()
            .gap(px(6.))
            .cursor_pointer()
            .child(
                div()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(SharedString::from(shown)),
            )
            .children(aside.map(|a| div().flex_none().text_color(hsla(ui.text_dim)).child(a)))
            .child(div().flex_1())
            .children(effort.as_deref().map(|level| {
                div()
                    .flex_none()
                    .text_color(hsla(ui.text_dim))
                    .child(SharedString::from(effort_word(level)))
            }))
            .child(div().flex_none().w(px(14.)).when(*current, |d| {
                d.child(icon(Icon::Check, px(14.), hsla(ui.text)))
            }))
            .on_click(cx.listener(move |this, _, window, cx| this.pick_model(ix, window, cx)));
        let more = div()
            .id(("box-model-more", i))
            .flex_none()
            .h_full()
            .px(px(6.))
            .flex()
            .items_center()
            .cursor_pointer()
            .child(icon(Icon::ChevronRight, px(14.), hsla(ui.text_dim)))
            // Only seen on the hovered row and the current one (the designer's details); still
            // there to press on the others.
            .when(!*current, |d| {
                d.opacity(0.).group_hover(group.clone(), |s| s.opacity(1.))
            })
            .on_click(cx.listener(move |this, _, _, cx| {
                this.open_menu(
                    BoxMenu::Model {
                        efforts_of: Some(ix),
                    },
                    cx,
                )
            }));
        let row = div()
            .id(("box-model", i))
            .group(group.clone())
            .h(px(ROW_H))
            .flex()
            .items_center()
            .rounded(px(ROW_RADIUS))
            .hover(move |s| s.bg(hsla(hover)))
            .when(efforts_of == Some(ix), move |d| d.bg(hsla(hover)))
            // Your words (2026-11-22): hovering a model shows its efforts beside it (none, for
            // a model without), no click.
            .on_hover({
                let shows = has_efforts.then_some(ix);
                cx.listener(move |this, hovered: &bool, _, cx| {
                    if *hovered
                        && matches!(this.menu, Some(BoxMenu::Model { efforts_of }) if efforts_of != shows)
                    {
                        this.menu = Some(BoxMenu::Model { efforts_of: shows });
                        cx.notify();
                    }
                })
            })
            .child(name)
            .when(*has_efforts, |d| d.child(more));
        self.marked_row(label.clone(), row).into_any_element()
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
        let hover = ui.panel_hover;
        let row = div()
            .id(id)
            .h(px(ROW_H))
            .px(px(8.))
            .flex()
            .items_center()
            .gap(px(6.))
            .rounded(px(ROW_RADIUS))
            .cursor_pointer()
            .hover(move |s| s.bg(hsla(hover)))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis_start()
                    .child(SharedString::from(label.clone())),
            )
            // The check on the right, in the text colour (the designer's details).
            .when(check, |d| {
                d.child(crate::icons::icon(
                    crate::icons::Icon::Check,
                    px(14.),
                    hsla(ui.text),
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
        // A row beside the menu (This Computer's, an effort) is probed apart from the menu's own.
        let probe = match id.0 {
            COMPUTER_ROWS => format!("{THIS_COMPUTER} \u{25b8} {label}"),
            EFFORT_ROWS => format!("Effort \u{25b8} {label}"),
            _ => label,
        };
        self.marked_row(probe, row).into_any_element()
    }

    /// `el`, and under `ui-shots` a probe that keeps where it was laid out by its `label`.
    fn marked_row<E: ParentElement>(&self, label: String, el: E) -> E {
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

/// A 1 px line between a menu's parts, 4 above and below.
fn divider(ui: &UiColors) -> AnyElement {
    div()
        .h(px(1.))
        .mx(px(4.))
        .my(px(4.))
        .bg(hsla(ui.panel_border))
        .into_any_element()
}
