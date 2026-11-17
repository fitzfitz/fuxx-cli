//! The agent box's menus: the folder picker and its panels, the model menu, and the frame and
//! rows they share (moved out of `agent_box.rs`, which keeps the pure part and the box itself).

use super::*;

impl AgentTab {
    /// The open menu's panel (`content` in it), hung above its chip (a child of it) and opening
    /// upward, in the tab menu's look; `side` (This Computer's folders) beside it. A click
    /// outside them closes it.
    pub(super) fn menu_frame(
        &self,
        content: Option<AnyElement>,
        side: Option<AnyElement>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let ui = cx.global::<Appearance>().theme.ui();
        let panel = crate::overlay_panel::shell(&ui, MENU_WIDTH)
            .id("agent-box-menu")
            .p(px(4.))
            .gap(px(0.))
            .occlude()
            .children(content);
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
                                                | BoxMenu::Move(_)
                                                | BoxMenu::Model { .. }
                                                | BoxMenu::Mode
                                                | BoxMenu::Attach
                                        )
                                    ) {
                                        this.chip_closed_at = Some(event.position);
                                    }
                                    this.close_menu(cx)
                                },
                            ))
                            .child(self.marked("menu", div().child(panel)))
                            .children(side),
                    ),
            ))
            .into_any_element()
    }

    /// The folder chip's click: the picker opens over it, or (open) closes.
    pub(super) fn folder_clicked(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if matches!(self.menu, Some(BoxMenu::Folder | BoxMenu::Move(_))) {
            self.close_menu(cx);
        } else {
            self.open_folder_picker(window, cx);
        }
    }

    /// Open the folder picker (§2), its search taking the keyboard. The first time in an app
    /// run it reads the folders the agent knows and the agent's version (whether it can move a
    /// conversation); a scene tab reads neither.
    pub fn open_folder_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(provider) = self.provider else {
            return;
        };
        if !self.no_process {
            crate::agent_catalog::load_known_folders(provider, cx);
            crate::agent_catalog::load_version(provider, cx);
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

    /// Whether `folder` is there to start in. A scene tab reads no disk: its made-up folders are.
    fn folder_exists(&self, folder: &Path) -> bool {
        self.no_process || folder.is_dir()
    }

    /// Make the open picker's rows again: for what is typed, from the folders fuxx knows
    /// (`KnownFolders`) and the agent's own (once read); This Computer's too while it is open.
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
    /// agent starts there (`choose`); after it, the picker asks: Move this conversation, or a
    /// New conversation there.
    fn choose_folder(&mut self, folder: PathBuf, cx: &mut Context<Self>) {
        if folder == self.start.cwd {
            self.close_menu(cx);
        } else if self.spoken() {
            self.open_menu(BoxMenu::Move(folder), cx);
        } else {
            self.close_menu(cx);
            self.choose(Change::Folder(folder), cx);
        }
    }

    /// The answer to Move this conversation (`move_it`) or New conversation: the agent
    /// restarts on this conversation in the folder (after the turn, if one runs), or a new
    /// agent tab opens there.
    pub fn move_choice(&mut self, move_it: bool, cx: &mut Context<Self>) {
        let Some(BoxMenu::Move(folder)) = self.menu.clone() else {
            return;
        };
        self.close_menu(cx);
        if !move_it {
            cx.emit(AgentTabEvent::NewConversationIn(folder));
        } else if self.can_move(cx) {
            self.choose(Change::Folder(folder), cx);
        }
    }

    /// Whether the agent can carry this conversation to another folder: by a restart (never
    /// live), and its provider says so for the version it read.
    fn can_move(&self, cx: &App) -> bool {
        let Some(provider) = self.provider else {
            return false;
        };
        let version =
            cx.try_global::<Catalogs>()
                .and_then(|c| match c.version(provider.info().name) {
                    Some(Lookup::Ready(version)) => version.clone(),
                    _ => None,
                });
        provider.applies(ChangeKind::Folder) == Applies::Restart
            && provider.can_move(version.as_deref())
    }

    /// The folder menu's panel content, and This Computer's list beside it while open: the
    /// picker (search, rows), or after the first message the Move / New conversation choice.
    pub(super) fn folder_menu(&self, cx: &mut Context<Self>) -> (AnyElement, Option<AnyElement>) {
        let ui = cx.global::<Appearance>().theme.ui();
        if let Some(BoxMenu::Move(folder)) = &self.menu {
            let home = crate::workspace::home_dir();
            let mut rows = vec![heading(
                crate::sidebar::home_relative(folder, home.as_deref()),
                &ui,
            )];
            if self.can_move(cx) {
                rows.push(self.menu_row(
                    ("box-move", 0),
                    MOVE_HERE.into(),
                    false,
                    false,
                    &ui,
                    cx,
                    |this, window, cx| {
                        this.move_choice(true, cx);
                        this.keyboard_back(window, cx);
                    },
                ));
            }
            rows.push(self.menu_row(
                ("box-move", 1),
                NEW_CONVERSATION.into(),
                false,
                false,
                &ui,
                cx,
                |this, window, cx| {
                    this.move_choice(false, cx);
                    this.keyboard_back(window, cx);
                },
            ));
            return (
                div().flex().flex_col().children(rows).into_any_element(),
                None,
            );
        }
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
    pub(super) fn catalog_lookup(&self, cx: &App) -> Lookup<Catalog> {
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
    pub(super) fn model_clicked(&mut self, cx: &mut Context<Self>) {
        if matches!(self.menu, Some(BoxMenu::Model { .. })) {
            self.close_menu(cx);
            return;
        }
        if let Some(provider) = self.provider
            && !self.no_process
        {
            crate::agent_catalog::load_catalog(provider, self.start.cwd.clone(), cx);
        }
        self.open_menu(
            BoxMenu::Model {
                efforts_of: None,
                usage: false,
            },
            cx,
        );
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
        let (efforts_of, usage_open) = match &self.menu {
            Some(BoxMenu::Model { efforts_of, usage }) => (*efforts_of, *usage),
            _ => (None, false),
        };
        let mut top = Vec::new();
        let mut list = Vec::new();
        let mut bottom = Vec::new();
        for (i, row) in model_rows(&lookup, &shown, applies).iter().enumerate() {
            match row {
                ModelRow::Heading => top.push(heading(MODEL_HEADING.into(), &ui)),
                ModelRow::Note(note) => list.push(quiet(*note, &ui)),
                ModelRow::Model { .. } => {
                    list.push(self.model_row(i, row, efforts_of, &ui, cx));
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
                ModelRow::Divider => bottom.push(
                    div()
                        .h(px(1.))
                        .mx(px(4.))
                        .my(px(4.))
                        .bg(hsla(ui.panel_border))
                        .into_any_element(),
                ),
                ModelRow::Usage => bottom.push(self.menu_row(
                    ("box-model", i),
                    VIEW_USAGE.into(),
                    false,
                    false,
                    &ui,
                    cx,
                    |this, _, cx| this.open_usage(cx),
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
            .children(bottom)
            .into_any_element();
        let side = match (efforts_of, &lookup) {
            _ if usage_open => Some(self.usage_panel(&ui, cx)),
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
    fn mode_now<'a>(&'a self, cx: &'a App) -> Option<&'a str> {
        let provider = self.provider?;
        current_mode(
            self.start.mode.as_deref(),
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
        div().flex().flex_col().children(items).into_any_element()
    }

    /// One mode of the menu: ✓ when in force, its name, and its one-line description under it.
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
        let hover = ui.panel_hover;
        let row = div()
            .id(("box-mode", i))
            .px(px(8.))
            .py(px(4.))
            .flex()
            .items_start()
            .gap(px(6.))
            .rounded(px(ROW_RADIUS))
            .cursor_pointer()
            .hover(move |s| s.bg(hsla(hover)))
            .child(
                div()
                    .flex_none()
                    .w(px(12.))
                    .text_color(hsla(ui.accent))
                    .child(if current { "\u{2713}" } else { "" }),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(div().child(label))
                    .child(
                        div()
                            .text_size(px(text::META))
                            .text_color(hsla(ui.text_dim))
                            .child(about),
                    ),
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                this.choose_mode(id, cx);
                this.keyboard_back(window, cx);
            }));
        self.marked_row(label.to_string(), row).into_any_element()
    }

    /// `View Usage ▸`: the usage panel opens beside the model menu, and the agent's usage is
    /// read now unless a reading under a minute old is there (or on its way). Never read while
    /// drawing. A scene tab only marks it loading: it runs no process.
    fn open_usage(&mut self, cx: &mut Context<Self>) {
        self.open_menu(
            BoxMenu::Model {
                efforts_of: None,
                usage: true,
            },
            cx,
        );
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
        let found = catalogs.and_then(|c| c.usage_last(agent));
        let limits = catalogs.map_or(&[][..], |c| c.limits(agent));
        let available = provider
            .usage_probe()
            .is_none()
            .then(|| provider.info().label);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        let offset = crate::cli::local_utc_offset(now as u64 * 1000);
        usage_rows(found, limits, available, now, offset)
    }

    /// The View Usage panel beside the model menu (§5): per group its name, each limit's name,
    /// `N% left` with a ring and `Resets …`, what the agent said of it underneath.
    fn usage_panel(&self, ui: &UiColors, cx: &App) -> AnyElement {
        let items: Vec<AnyElement> = self
            .usage_rows_now(cx)
            .into_iter()
            .map(|row| match row {
                UsageRow::Heading(name) => heading(name, ui),
                UsageRow::Quiet(text) => quiet(text, ui),
                UsageRow::Text(text) => div()
                    .px(px(8.))
                    .py(px(4.))
                    .text_color(hsla(ui.text_dim))
                    .child(SharedString::from(text))
                    .into_any_element(),
                UsageRow::Limit {
                    name,
                    left_pct,
                    resets,
                    note,
                } => self.limit_row(name, left_pct, resets, note, ui),
            })
            .collect();
        let panel = crate::overlay_panel::shell(ui, USAGE_WIDTH)
            .p(px(4.))
            .gap(px(0.))
            .occlude()
            .children(items);
        self.marked("usage", div().child(panel)).into_any_element()
    }

    /// One limit of the usage panel: its name, `N% left` and its ring (when the reading says),
    /// `Resets …` under it, and what the agent said (close to, or at, its limit).
    fn limit_row(
        &self,
        name: String,
        left_pct: Option<u8>,
        resets: Option<String>,
        note: Option<String>,
        ui: &UiColors,
    ) -> AnyElement {
        let small = |t: String, color| {
            div()
                .text_size(px(text::META))
                .text_color(hsla(color))
                .child(SharedString::from(t))
        };
        let (dim, track, arc) = (ui.text_dim, ui.panel_border, ui.accent);
        let top = div()
            .flex()
            .items_center()
            .gap(px(8.))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(SharedString::from(name.clone())),
            )
            .children(left_pct.map(|left| {
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .text_color(hsla(dim))
                    .child(SharedString::from(format!("{left}% left")))
                    .child(ring(left, track, arc))
            }));
        let row = div()
            .px(px(8.))
            .py(px(4.))
            .flex()
            .flex_col()
            .child(top)
            .children(resets.map(|t| small(t, ui.text_faint)))
            .children(note.map(|t| small(t, ui.ring_waiting)));
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
        let (ix, hover) = (*ix, ui.panel_hover);
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
                    .flex_none()
                    .w(px(12.))
                    .text_color(hsla(ui.accent))
                    .child(if *current { "\u{2713}" } else { "" }),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(SharedString::from(label.clone())),
            )
            .children(effort.as_deref().map(|level| {
                div()
                    .flex_none()
                    .text_color(hsla(ui.text_dim))
                    .child(SharedString::from(effort_word(level)))
            }))
            .on_click(cx.listener(move |this, _, window, cx| this.pick_model(ix, window, cx)));
        let more = div()
            .id(("box-model-more", i))
            .flex_none()
            .h_full()
            .px(px(8.))
            .flex()
            .items_center()
            .text_color(hsla(ui.text_dim))
            .cursor_pointer()
            .child("\u{25b8}")
            .on_click(cx.listener(move |this, _, _, cx| {
                this.open_menu(
                    BoxMenu::Model {
                        efforts_of: Some(ix),
                        usage: false,
                    },
                    cx,
                )
            }));
        let row = div()
            .id(("box-model", i))
            .h(px(ROW_H))
            .flex()
            .items_center()
            .rounded(px(ROW_RADIUS))
            .hover(move |s| s.bg(hsla(hover)))
            .when(efforts_of == Some(ix), move |d| d.bg(hsla(hover)))
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
                    .flex_none()
                    .w(px(12.))
                    .text_color(hsla(ui.accent))
                    .child(if check { "\u{2713}" } else { "" }),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis_start()
                    .child(SharedString::from(label.clone())),
            )
            .when(more, |d| {
                d.child(
                    div()
                        .flex_none()
                        .text_color(hsla(ui.text_dim))
                        .child("\u{25b8}"),
                )
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

/// A menu's section heading.
fn heading(text: String, ui: &UiColors) -> AnyElement {
    div()
        .px(px(8.))
        .pt(px(6.))
        .pb(px(2.))
        .text_size(px(text::META))
        .text_color(hsla(ui.text_faint))
        .overflow_hidden()
        .whitespace_nowrap()
        .text_ellipsis_start()
        .child(SharedString::from(text))
        .into_any_element()
}

/// A quiet line in a menu's list (no row to pick).
fn quiet(text: impl Into<SharedString>, ui: &UiColors) -> AnyElement {
    div()
        .h(px(ROW_H))
        .px(px(26.))
        .flex()
        .items_center()
        .text_color(hsla(ui.text_dim))
        .child(text.into())
        .into_any_element()
}

/// A small ring showing `left` percent: a quiet track, and over it an arc (from the top,
/// clockwise) as long as what is left.
fn ring(left: u8, track: Rgb, arc: Rgb) -> impl IntoElement {
    gpui::canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let size = f32::from(bounds.size.width.min(bounds.size.height));
            let r = (size - RING_STROKE) / 2.0;
            let (cx, cy) = (
                f32::from(bounds.origin.x) + size / 2.0,
                f32::from(bounds.origin.y) + size / 2.0,
            );
            let at = |angle: f32| point(px(cx + r * angle.sin()), px(cy - r * angle.cos()));
            let radii = point(px(r), px(r));
            let tau = std::f32::consts::TAU;
            let circle = |color: Rgb, fraction: f32| {
                let mut path = gpui::PathBuilder::stroke(px(RING_STROKE));
                path.move_to(at(0.0));
                if fraction >= 0.999 {
                    path.arc_to(radii, px(0.0), false, true, at(tau / 2.0));
                    path.arc_to(radii, px(0.0), false, true, at(0.0));
                } else {
                    let end = tau * fraction;
                    path.arc_to(radii, px(0.0), fraction > 0.5, true, at(end));
                }
                (path, color)
            };
            let fraction = f32::from(left.min(100)) / 100.0;
            let mut paths = vec![circle(track, 1.0)];
            if fraction > 0.0 {
                paths.push(circle(arc, fraction));
            }
            for (path, color) in paths {
                if let Ok(path) = path.build() {
                    window.paint_path(path, hsla(color));
                }
            }
        },
    )
    .flex_none()
    .size(px(RING_SIZE))
}
