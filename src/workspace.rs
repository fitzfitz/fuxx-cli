//! The root view: a sidebar listing every session (with its status ring) and the layout of
//! panes holding the active session. Owns session lifecycle, switching, splits, and background
//! notifications.

use std::cell::Cell;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use futures::channel::mpsc::UnboundedReceiver;
use gpui::{
    AnyElement, Bounds, ClickEvent, Context, DispatchPhase, Entity, EntityId, Focusable, Hsla,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PathPromptOptions, Pixels, Point,
    SharedString, Subscription, Task, Window, actions, canvas, div, prelude::*, px,
};

use crate::api_events::{Event, WaitTable};
use crate::api_server::{self, Conn};
use crate::appearance::Appearance;
use crate::groups::{self, Row};
use crate::name_input::{NameInput, NameInputEvent};
use crate::notifier;
use crate::panes::{self, PaneColors, Panes, SelectPane, StartDrag};
use crate::presets::Preset;
use crate::restore::{self, Launch, RestoreMode, SavedSession, SavedState};
use crate::session_list::SessionList;
use crate::settings;
use crate::split_tree::{self, Axis, Dir, Node, Rect};
use crate::status::Status;
use crate::terminal_view::{TerminalEvent, TerminalView, hsla};
use crate::theme::UiColors;

actions!(
    workspace,
    [
        NewSession,
        NewSessionInFolder,
        CloseSession,
        NextSession,
        PrevSession,
        SelectSession1,
        SelectSession2,
        SelectSession3,
        SelectSession4,
        SelectSession5,
        SelectSession6,
        SelectSession7,
        SelectSession8,
        SelectSession9,
        LaunchPreset1,
        LaunchPreset2,
        LaunchPreset3,
        LaunchPreset4,
        LaunchPreset5,
        LaunchPreset6,
        LaunchPreset7,
        LaunchPreset8,
        LaunchPreset9,
        RenameSession,
        MoveSessionUp,
        MoveSessionDown,
        SplitRight,
        SplitDown,
        FocusPaneLeft,
        FocusPaneRight,
        FocusPaneUp,
        FocusPaneDown,
        PrevPane,
        NextPane,
        EqualizePanes,
    ]
);

const SIDEBAR_WIDTH: f32 = 220.0;

pub struct Session {
    /// Scripting id: shown to scripts as `s<id>`.
    pub id: u64,
    pub view: Entity<TerminalView>,
    pub status: Status,
    pub title: Option<SharedString>,
    /// The name the user gave it (none: the automatic name).
    pub name: Option<String>,
    /// The folder this session's group is for (none: no group).
    pub project: Option<PathBuf>,
    pub preset: Option<Preset>,
    _subscription: Subscription,
}

impl Session {
    /// Your name, else the program's own title, else the preset's name, else 🖕.
    pub fn label(&self) -> SharedString {
        groups::auto_name(
            self.name.as_deref(),
            self.title.as_deref(),
            self.preset.as_ref().map(|p| p.name.as_str()),
        )
        .into()
    }
}

/// What a sidebar row carries while it's dragged: the session (by its view), its group and its
/// name.
struct DraggedSession {
    view: EntityId,
    project: Option<PathBuf>,
    label: SharedString,
}

/// The little label that follows the pointer during a drag.
struct DragPreview {
    label: SharedString,
}

impl Render for DragPreview {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ui = cx.global::<Appearance>().theme.ui();
        div()
            .px_2()
            .py_1()
            .rounded_md()
            .text_sm()
            .bg(hsla(ui.row_active))
            .text_color(hsla(ui.text))
            .child(self.label.clone())
    }
}

pub struct Workspace {
    pub(crate) sessions: SessionList<Session>,
    /// The split layouts, by each session's view. Every session is in exactly one; a session
    /// never split is a layout of its own.
    layouts: Vec<Node<EntityId>>,
    /// Where the layout on screen is drawn, in window pixels; written each frame by a canvas.
    pane_area: Rc<Cell<Option<Bounds<Pixels>>>>,
    /// The line being dragged: a pane of its layout (by view) and the path to its split.
    dragging: Option<(EntityId, Vec<bool>)>,
    /// The projects whose groups are closed in the sidebar.
    collapsed: Vec<PathBuf>,
    /// The rename box, and the session (by its view) it renames.
    pub(crate) renaming: Option<(EntityId, Entity<NameInput>)>,
    /// True while the saved sessions are being reopened, so nothing is saved halfway (and no
    /// events are sent).
    pub(crate) restoring: bool,
    /// The id the next session gets (`s1`, `s2`, ...); never reused.
    pub(crate) next_id: u64,
    window_active: bool,
    /// Answers scripting requests (see `workspace_api`); none when scripting is off.
    _requests: Option<Task<()>>,
    /// Connections that asked for events (`events.subscribe`).
    pub(crate) subscribers: Vec<Conn>,
    /// Pending `session.wait`s.
    pub(crate) waits: WaitTable<Conn>,
    /// Whether the 200 ms wait check is running (only while waits are pending).
    pub(crate) waits_ticking: bool,
    /// The active session as of the last `focus.changed`.
    pub(crate) focused: Option<u64>,
    _activation: Subscription,
    /// Repaints the sidebar when the appearance changes.
    _appearance: Subscription,
}

impl Workspace {
    /// `requests`: the scripting socket's requests, answered for as long as the workspace lives.
    pub fn new(
        requests: Option<UnboundedReceiver<api_server::Incoming>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let activation = cx.observe_window_activation(window, |this, window, cx| {
            this.window_active = window.is_window_active();
            cx.notify();
        });
        let appearance = cx.observe_global::<Appearance>(|_, cx| cx.notify());
        let mut this = Self {
            sessions: SessionList::new(),
            layouts: Vec::new(),
            pane_area: Rc::new(Cell::new(None)),
            dragging: None,
            collapsed: Vec::new(),
            renaming: None,
            restoring: true,
            next_id: 1,
            window_active: window.is_window_active(),
            _requests: requests.map(|rx| Self::serve_requests(rx, window, cx)),
            subscribers: Vec::new(),
            waits: WaitTable::new(),
            waits_ticking: false,
            focused: None,
            _activation: activation,
            _appearance: appearance,
        };
        this.restore(window, cx);
        this.restoring = false;
        this.save();
        this
    }

    /// Reopen the saved sessions per the `session-restore` mode, or start one plain session.
    fn restore(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mode = cx.global::<Appearance>().restore;
        if mode != RestoreMode::Off
            && let Some(home) = home_dir()
        {
            let path = restore::state_path(&home);
            match std::fs::read_to_string(&path) {
                Ok(text) => match restore::decode(&text) {
                    Some(state) if !state.sessions.is_empty() => {
                        let presets = cx.global::<Appearance>().presets.clone();
                        for launch in restore::launches(&state, mode, &presets) {
                            self.open_session(launch, None, window, cx);
                        }
                        if !self.sessions.is_empty() {
                            self.restore_layouts(state.layouts, state.sessions.len());
                            self.collapsed = state.collapsed;
                            let last = self.sessions.items().len() - 1;
                            let active = state.active.unwrap_or(0).min(last);
                            // The normal route, so the active session's group opens.
                            self.sessions.select(active);
                            self.activate(window, cx);
                            return;
                        }
                        eprintln!(
                            "[fuxx] restore: no saved session could start, opening a new one"
                        );
                    }
                    Some(_) => {}
                    None => eprintln!(
                        "[fuxx] restore: {} is not a valid sessions file, starting fresh",
                        path.display()
                    ),
                },
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                Err(err) => eprintln!(
                    "[fuxx] restore: could not read {}: {err}, starting fresh",
                    path.display()
                ),
            }
        }
        self.open_session(Launch::default(), None, window, cx);
    }

    /// The layouts as session indices, for the file. A tree with a pane that isn't a session
    /// (never, by the layout invariant) is left out rather than saved wrong.
    fn saved_layouts(&self) -> Vec<Node<usize>> {
        let index_of = |id: EntityId| self.sessions.position(|s| s.view.entity_id() == id);
        self.layouts
            .iter()
            .filter(|t| t.leaves().into_iter().all(|id| index_of(id).is_some()))
            .map(|t| t.map(&|id| index_of(id).unwrap_or(0)))
            .collect()
    }

    /// Put the saved split layouts back on the restored sessions. Saved indices follow the saved
    /// order, so if a session didn't start they no longer line up and every session stays alone.
    fn restore_layouts(&mut self, saved: Vec<Node<usize>>, saved_count: usize) {
        let views: Vec<EntityId> = self
            .sessions
            .items()
            .iter()
            .map(|s| s.view.entity_id())
            .collect();
        if views.len() != saved_count {
            eprintln!("[fuxx] restore: a session didn't start, so split layouts weren't restored");
            return;
        }
        let (trees, changed) = split_tree::sanitize(saved, views.len());
        if changed {
            eprintln!(
                "[fuxx] restore: some split layouts didn't match the saved sessions; those sessions open on their own"
            );
        }
        self.layouts = trees.iter().map(|t| t.map(&|i| views[i])).collect();
    }

    /// Write the session list to the sessions file (not while restoring). One log line on failure.
    pub(crate) fn save(&self) {
        if self.restoring {
            return;
        }
        let Some(home) = home_dir() else { return };
        let state = SavedState {
            version: restore::VERSION,
            active: self.sessions.active_index(),
            collapsed: self.collapsed.clone(),
            layouts: self.saved_layouts(),
            sessions: self
                .sessions
                .items()
                .iter()
                .map(|s| SavedSession {
                    name: s.name.clone(),
                    project: s.project.clone(),
                    preset: s.preset.clone(),
                })
                .collect(),
        };
        let path = restore::state_path(&home);
        if let Err(err) = settings::write_atomic(&path, &restore::encode(&state)) {
            eprintln!("[fuxx] restore: could not save {}: {err}", path.display());
        }
    }

    /// Open a session for `launch` next to its group and make it active: a layout of its own, or
    /// with `split`, a new pane beside (Row) or under (Column) the active one.
    pub(crate) fn open_session(
        &mut self,
        launch: Launch,
        split: Option<Axis>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Launch { saved, run_preset } = launch;
        let cwd: Option<&Path> = match saved.project.as_deref() {
            Some(dir) if dir.is_dir() => Some(dir),
            Some(dir) => {
                eprintln!(
                    "[fuxx] session: folder {} is gone, starting in home",
                    dir.display()
                );
                None
            }
            None => None,
        };
        let startup = saved
            .preset
            .as_ref()
            .filter(|_| run_preset)
            .map(|p| p.command.as_str());
        let session_id = self.next_id;
        self.next_id += 1;
        let env = if cx.global::<Appearance>().scripting {
            scripting_env(session_id)
        } else {
            Vec::new()
        };
        let view = match TerminalView::spawn(window, cx, cwd, startup, &env) {
            Ok(view) => view,
            Err(err) => {
                eprintln!("fuxx: could not start a shell: {err}");
                return;
            }
        };
        let subscription = cx.subscribe_in(&view, window, Self::on_terminal_event);
        let id = view.entity_id();
        let active = self.sessions.active().map(|s| s.view.entity_id());
        let split_done = split.zip(active).is_some_and(|(axis, active)| {
            self.layouts
                .iter_mut()
                .find(|t| t.contains(active))
                .is_some_and(|t| t.split(active, id, axis))
        });
        if !split_done {
            self.layouts.push(Node::Leaf(id));
        }
        let index = groups::insert_index(&self.projects(), saved.project.as_deref());
        let index = self.sessions.insert(
            index,
            Session {
                id: session_id,
                view,
                status: Status::Idle,
                title: None,
                name: saved.name,
                project: saved.project,
                preset: saved.preset,
                _subscription: subscription,
            },
        );
        self.emit(&Event::Opened, Some(index));
        self.activate(window, cx);
    }

    /// Each session's project, in sidebar order.
    fn projects(&self) -> Vec<Option<&Path>> {
        self.sessions
            .items()
            .iter()
            .map(|s| s.project.as_deref())
            .collect()
    }

    /// The active session's folder, so a new session joins its group.
    pub(crate) fn active_project(&self) -> Option<PathBuf> {
        self.sessions.active().and_then(|s| s.project.clone())
    }

    fn new_session(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let saved = SavedSession {
            project: self.active_project(),
            ..Default::default()
        };
        self.open_session(
            Launch {
                saved,
                run_preset: false,
            },
            None,
            window,
            cx,
        );
    }

    /// Ask for a folder, then open a session in it. Cancel does nothing.
    fn new_session_in_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Open".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let folder = match picked.await {
                Ok(Ok(Some(paths))) => paths.into_iter().next(),
                Ok(Ok(None)) | Err(_) => None,
                Ok(Err(err)) => {
                    eprintln!("[fuxx] session: folder picker failed: {err}");
                    None
                }
            };
            let Some(folder) = folder else { return };
            this.update_in(cx, |this, window, cx| this.open_in(folder, window, cx))
                .ok();
        })
        .detach();
    }

    /// A plain session in `project`'s group (the group header's +).
    fn open_in(&mut self, project: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let launch = Launch {
            saved: SavedSession {
                project: Some(project),
                ..Default::default()
            },
            run_preset: false,
        };
        self.open_session(launch, None, window, cx);
    }

    /// A plain session in the active session's group, as a new pane beside or under it.
    fn split(&mut self, axis: Axis, window: &mut Window, cx: &mut Context<Self>) {
        let saved = SavedSession {
            project: self.active_project(),
            ..Default::default()
        };
        let launch = Launch {
            saved,
            run_preset: false,
        };
        self.open_session(launch, Some(axis), window, cx);
    }

    /// Preset `n` (0-based) of the settings, in the active session's group; absent is ignored.
    fn launch_preset(&mut self, n: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(preset) = cx.global::<Appearance>().presets.get(n).cloned() else {
            return;
        };
        let saved = SavedSession {
            project: self.active_project(),
            preset: Some(preset),
            ..Default::default()
        };
        self.open_session(
            Launch {
                saved,
                run_preset: true,
            },
            None,
            window,
            cx,
        );
    }

    fn on_terminal_event(
        &mut self,
        view: &Entity<TerminalView>,
        event: &TerminalEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self
            .sessions
            .position(|s| s.view.entity_id() == view.entity_id())
        else {
            return;
        };
        let is_active = self.sessions.active_index() == Some(index);
        match event {
            TerminalEvent::StatusChanged(new) => {
                let window_active = self.window_active;
                let Some(session) = self.sessions.get_mut(index) else {
                    return;
                };
                let old = session.status;
                session.status = *new;
                let label = session.label();
                if old != *new {
                    let change = Event::StatusChanged {
                        from: old,
                        to: *new,
                    };
                    self.emit(&change, Some(index));
                }
                if notifier::should_notify(old, *new, is_active, window_active) {
                    if let Some(body) = notifier::message(*new) {
                        let summary = format!("fuxx — {label}");
                        let posted = Event::Notification {
                            title: &summary,
                            body,
                        };
                        self.emit(&posted, Some(index));
                        cx.background_spawn(async move { notifier::fire(&summary, body) })
                            .detach();
                    }
                }
                self.check_waits(&[], cx);
                cx.notify();
            }
            TerminalEvent::TitleChanged(title) => {
                let changed = self
                    .sessions
                    .get_mut(index)
                    .filter(|s| s.title != *title)
                    .map(|s| s.title = title.clone())
                    .is_some();
                if changed {
                    self.emit(
                        &Event::TitleChanged {
                            title: title.as_deref(),
                        },
                        Some(index),
                    );
                }
                cx.notify();
            }
            TerminalEvent::Exited => self.close_at(index, window, cx),
        }
    }

    pub(crate) fn close_at(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some((id, session_id)) = self
            .sessions
            .items()
            .get(index)
            .map(|s| (s.view.entity_id(), s.id))
        else {
            return;
        };
        // While its row number and name still stand.
        self.emit(&Event::Closed, Some(index));
        // Waits get a last look at its screen and status (PASS printed just before the shell
        // exited still counts), then exit / change waits are met.
        let mut replied = self.check_waits(&[session_id], cx);
        let was_active = self.sessions.active_index() == Some(index);
        self.dragging = None; // the layout changes under a drag
        // The pane's neighbour takes its space, and the focus if it had it.
        let mut next = None;
        if let Some(pos) = self.layouts.iter().position(|t| t.contains(id)) {
            let tree = self.layouts.remove(pos);
            if was_active {
                next = tree.heir(id);
            }
            if let Some(rest) = tree.remove(id) {
                self.layouts.insert(pos, rest);
            }
        }
        // Dropping the Session drops its view -> TerminalCore -> Pty, which kills the child.
        self.sessions.remove(index);
        if self.sessions.is_empty() {
            self.save(); // an empty list, so the next launch starts fresh
            // Last session gone: close the window (and quit), once wait replies and the closed
            // event have reached their clients.
            replied.extend(self.subscribers.iter().cloned());
            self.quit_after_flush(replied, window, cx);
            return;
        }
        // A group whose last session closed is gone; forget that it was closed.
        let sessions = self.sessions.items();
        self.collapsed
            .retain(|c| sessions.iter().any(|s| s.project.as_ref() == Some(c)));
        if let Some(i) = next.and_then(|n| self.sessions.position(|s| s.view.entity_id() == n)) {
            self.sessions.select(i);
        }
        self.activate(window, cx);
    }

    pub(crate) fn select(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.sessions.select(index) {
            self.activate(window, cx);
        }
    }

    /// A click in a pane, or a move to it: make its session the active one (if it isn't already).
    fn select_view(&mut self, view: EntityId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self.sessions.position(|s| s.view.entity_id() == view) else {
            return;
        };
        if self.sessions.active_index() != Some(index) {
            self.select(index, window, cx);
        }
    }

    /// The layout holding the active session (the one on screen).
    pub(crate) fn shown_layout(&self) -> Option<&Node<EntityId>> {
        let active = self.sessions.active()?.view.entity_id();
        self.layouts.iter().find(|t| t.contains(active))
    }

    /// Move the focus to the pane `pick` finds from the focused one, in the layout on screen.
    fn focus_pane(
        &mut self,
        pick: impl FnOnce(&Node<EntityId>, EntityId) -> Option<EntityId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(active) = self.sessions.active().map(|s| s.view.entity_id()) else {
            return;
        };
        if let Some(next) = self.shown_layout().and_then(|t| pick(t, active)) {
            self.select_view(next, window, cx);
        }
    }

    /// Give every pane of the layout on screen an equal share.
    fn equalize_panes(&mut self, cx: &mut Context<Self>) {
        if let Some(active) = self.sessions.active().map(|s| s.view.entity_id()) {
            self.equalize_layout(active, cx);
        }
    }

    /// Give every pane of the layout holding `pane` an equal share.
    pub(crate) fn equalize_layout(&mut self, pane: EntityId, cx: &mut Context<Self>) {
        if let Some(tree) = self.layouts.iter_mut().find(|t| t.contains(pane)) {
            tree.equalize();
            self.save();
            cx.notify();
        }
    }

    /// A line of the layout on screen was pressed: follow the mouse until it's released.
    fn start_drag(&mut self, path: Vec<bool>, cx: &mut Context<Self>) {
        if let Some(active) = self.sessions.active() {
            self.dragging = Some((active.view.entity_id(), path));
            cx.notify(); // the next frame listens for the moves
        }
    }

    /// The dragged line follows the mouse: its split's ratio is where `position` falls in it.
    fn drag_to(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        let (Some((pane, path)), Some(bounds)) = (&self.dragging, self.pane_area.get()) else {
            return;
        };
        let Some(tree) = self.layouts.iter_mut().find(|t| t.contains(*pane)) else {
            return;
        };
        let area = Rect {
            x: f32::from(bounds.origin.x),
            y: f32::from(bounds.origin.y),
            w: f32::from(bounds.size.width),
            h: f32::from(bounds.size.height),
        };
        let Some((axis, split)) = tree.split_rect(path, area) else {
            return;
        };
        let ratio = match axis {
            Axis::Row => (f32::from(position.x) - split.x) / split.w,
            Axis::Column => (f32::from(position.y) - split.y) / split.h,
        };
        if tree.set_ratio(path, ratio) {
            cx.notify();
        }
    }

    fn end_drag(&mut self, cx: &mut Context<Self>) {
        if self.dragging.take().is_some() {
            self.save(); // once per drag, not on every move
            cx.notify();
        }
    }

    /// After the active session changed (by any route): open its group and focus it.
    pub(crate) fn activate(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.note_focus();
        if let Some(project) = self.sessions.active().and_then(|s| s.project.clone()) {
            self.collapsed.retain(|c| *c != project);
        }
        // A divider drag belongs to the layout it started in; drop it if another is shown now.
        if let Some((pane, _)) = &self.dragging
            && !self.shown_layout().is_some_and(|t| t.contains(*pane))
        {
            self.dragging = None;
        }
        self.focus_active(window, cx);
        self.save();
        cx.notify();
    }

    fn toggle_group(&mut self, project: &Path, cx: &mut Context<Self>) {
        match self.collapsed.iter().position(|c| c == project) {
            Some(i) => {
                self.collapsed.remove(i);
            }
            None => self.collapsed.push(project.to_path_buf()),
        }
        self.save();
        cx.notify();
    }

    /// Open the rename box on session `index` (opening its group) and focus it.
    fn start_rename(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(session) = self.sessions.items().get(index) else {
            return;
        };
        let view = session.view.entity_id();
        let text = session.label().to_string();
        if let Some(project) = session.project.clone() {
            self.collapsed.retain(|c| *c != project);
        }
        let input = cx.new(|cx| NameInput::new(text, window, cx));
        cx.subscribe_in(&input, window, Self::on_name_event)
            .detach();
        input.focus_handle(cx).focus(window, cx);
        self.renaming = Some((view, input));
        self.save(); // opening the group may have changed what's collapsed
        cx.notify();
    }

    fn on_name_event(
        &mut self,
        input: &Entity<NameInput>,
        event: &NameInputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // A box already closed (or replaced) can still report losing focus: ignore it.
        let Some((view, _)) = self.renaming.as_ref().filter(|(_, i)| i == input) else {
            return;
        };
        let view = *view;
        self.renaming = None;
        if let NameInputEvent::Confirm(text) = event
            && let Some(i) = self.sessions.position(|s| s.view.entity_id() == view)
            && let Some(session) = self.sessions.get_mut(i)
        {
            // Enter on an untouched automatic name keeps it automatic (it still follows the title).
            if session.name.is_some() || *text != session.label().as_ref() {
                let name = (!text.is_empty()).then(|| text.clone());
                if session.name != name {
                    session.name = name;
                    self.emit_renamed(i);
                }
            }
            self.save();
        }
        self.focus_active(window, cx);
        cx.notify();
    }

    /// Drop `dragged` on `target`'s row: it takes that place, inside the same group only.
    fn drop_session(&mut self, dragged: EntityId, target: EntityId, cx: &mut Context<Self>) {
        let find = |id: EntityId| self.sessions.position(|s| s.view.entity_id() == id);
        let (Some(from), Some(to)) = (find(dragged), find(target)) else {
            return;
        };
        if groups::same_group(&self.projects(), from, to) && self.sessions.move_item(from, to) {
            self.save();
            cx.notify();
        }
    }

    /// Swap the active session with its neighbour above (or below), inside its group only.
    fn move_active(&mut self, up: bool, cx: &mut Context<Self>) {
        let Some(from) = self.sessions.active_index() else {
            return;
        };
        let Some(to) = (if up {
            from.checked_sub(1)
        } else {
            Some(from + 1)
        }) else {
            return;
        };
        if groups::same_group(&self.projects(), from, to) && self.sessions.move_item(from, to) {
            self.save();
            cx.notify();
        }
    }

    pub(crate) fn focus_active(&self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(session) = self.sessions.active() {
            session.view.focus_handle(cx).focus(window, cx);
        }
    }

    /// The layout holding the active session, its unfocused panes dimmed.
    fn render_panes(&self, colors: PaneColors, cx: &mut Context<Self>) -> Option<AnyElement> {
        let active = self.sessions.active()?.view.entity_id();
        let tree = self.shown_layout()?;
        let view = |id: EntityId| {
            self.sessions
                .items()
                .iter()
                .find(|s| s.view.entity_id() == id)
                .map(|s| s.view.clone())
        };
        let this = cx.entity().downgrade();
        let select: SelectPane = Rc::new(move |id, window, cx| {
            this.update(cx, |this, cx| this.select_view(id, window, cx))
                .ok();
        });
        let this = cx.entity().downgrade();
        let start_drag: StartDrag = Rc::new(move |path, _, cx| {
            this.update(cx, |this, cx| this.start_drag(path, cx)).ok();
        });
        let panes = Panes {
            active,
            view: &view,
            select,
            start_drag,
            colors,
        };
        Some(panes::render_layout(tree, &panes))
    }

    /// An invisible layer over the pane area: it records the area's bounds for divider drags
    /// and, while a line is dragged, follows the mouse anywhere in the window until release.
    fn render_pane_area(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let area = self.pane_area.clone();
        let dragging = self.dragging.is_some();
        let this = cx.entity().downgrade();
        canvas(
            move |bounds, _, _| area.set(Some(bounds)),
            move |_, _, window, _| {
                if !dragging {
                    return;
                }
                // Window-wide, not tied to a hitbox, so a fast move off the line keeps dragging.
                let moved = this.clone();
                window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
                    if phase != DispatchPhase::Capture {
                        return;
                    }
                    moved
                        .update(cx, |this, cx| {
                            // Released where no mouse-up reached us (e.g. outside the window).
                            if event.dragging() {
                                this.drag_to(event.position, cx)
                            } else {
                                this.end_drag(cx)
                            }
                        })
                        .ok();
                });
                let released = this.clone();
                window.on_mouse_event(move |event: &MouseUpEvent, phase, _, cx| {
                    if phase == DispatchPhase::Capture && event.button == MouseButton::Left {
                        released.update(cx, |this, cx| this.end_drag(cx)).ok();
                    }
                });
            },
        )
        .absolute()
        .size_full()
    }

    fn render_sidebar(&self, ui: UiColors, cx: &mut Context<Self>) -> impl IntoElement {
        let projects = self.projects();
        let all: Vec<&Path> = projects.iter().flatten().copied().collect();
        let rows: Vec<AnyElement> = groups::rows(&projects, &self.collapsed)
            .into_iter()
            .map(|row| match row {
                Row::Header {
                    project,
                    first,
                    len,
                    collapsed,
                } => {
                    let label = groups::group_label(&project, &all);
                    self.render_header(project, label, first..first + len, collapsed, ui, cx)
                        .into_any_element()
                }
                Row::Session { index, grouped } => self
                    .render_session_row(index, grouped, ui, cx)
                    .into_any_element(),
            })
            .collect();
        div()
            .flex()
            .flex_col()
            .w(px(SIDEBAR_WIDTH))
            .h_full()
            .p_1()
            .gap_1()
            .bg(hsla(ui.sidebar_background))
            .border_r_1()
            .border_color(hsla(ui.sidebar_border))
            // A click in the sidebar gives focus back to the terminal, which cancels a rename
            // (the rename box keeps its own clicks).
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _: &MouseDownEvent, window, cx| this.focus_active(window, cx)),
            )
            .children(rows)
    }

    /// A project's header: arrow, name, the group's most urgent ring, and a + for a new session.
    fn render_header(
        &self,
        project: PathBuf,
        label: String,
        sessions: std::ops::Range<usize>,
        collapsed: bool,
        ui: UiColors,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let status = groups::rolled_up(
            self.sessions.items()[sessions.clone()]
                .iter()
                .map(|s| s.status),
        );
        let add = project.clone();
        div()
            .id(("group", group_key(&project)))
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .px_2()
            .py_1()
            .rounded_md()
            .text_sm()
            .text_color(hsla(ui.text_dim))
            .hover(|style| style.bg(hsla(ui.row_hover)))
            .cursor_pointer()
            .on_click(
                cx.listener(move |this, _: &ClickEvent, _, cx| this.toggle_group(&project, cx)),
            )
            .child(if collapsed { "\u{25b8}" } else { "\u{25be}" })
            .child(div().flex_1().overflow_hidden().child(label))
            .child(ring(status, ui))
            .child(
                div()
                    .id("group-add")
                    .px_1()
                    .rounded_sm()
                    .hover(|style| style.bg(hsla(ui.row_active)).text_color(hsla(ui.text)))
                    .child("+")
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        cx.stop_propagation(); // not also a click on the header
                        this.open_in(add.clone(), window, cx)
                    })),
            )
    }

    /// A session: ring, number and name (or the rename box). Click selects, double-click
    /// renames; it can be dragged onto another row of its group.
    fn render_session_row(
        &self,
        index: usize,
        grouped: bool,
        ui: UiColors,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let session = &self.sessions.items()[index];
        let is_active = self.sessions.active_index() == Some(index);
        let view = session.view.entity_id();
        // In the layout on screen, but not the focused pane: a faint highlight.
        let on_screen = !is_active && self.shown_layout().is_some_and(|t| t.contains(view));
        let rename_box = self
            .renaming
            .as_ref()
            .filter(|(v, _)| *v == view)
            .map(|(_, input)| input.clone());
        let dragged = DraggedSession {
            view,
            project: session.project.clone(),
            label: session.label(),
        };
        let project = session.project.clone();
        div()
            .id(("session", view))
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .px_2()
            .py_1()
            .when(grouped, |row| row.ml_4())
            .rounded_md()
            .text_sm()
            .text_color(hsla(if is_active { ui.text } else { ui.text_dim }))
            .when(is_active, |row| row.bg(hsla(ui.row_active)))
            .when(on_screen, |row| row.bg(hsla(ui.row_hover)))
            .hover(|style| style.bg(hsla(ui.row_hover)))
            .cursor_pointer()
            .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                this.select(index, window, cx);
                if event.click_count() == 2 {
                    this.start_rename(index, window, cx);
                }
            }))
            .on_drag(dragged, |dragged, _, _, cx| {
                cx.new(|_| DragPreview {
                    label: dragged.label.clone(),
                })
            })
            // Only rows of the dragged session's group accept it (and tint); the drop re-checks.
            .can_drop(move |dragged, _, _| {
                dragged
                    .downcast_ref::<DraggedSession>()
                    .is_some_and(|d| d.project == project)
            })
            .drag_over::<DraggedSession>(move |style, _, _, _| style.bg(hsla(ui.row_active)))
            .on_drop(cx.listener(move |this, dragged: &DraggedSession, _, cx| {
                this.drop_session(dragged.view, view, cx)
            }))
            .child(ring(session.status, ui))
            .map(|row| match rename_box {
                Some(input) => row.child(format!("{}", index + 1)).child(input),
                None => row.child(format!("{}  {}", index + 1, session.label())),
            })
    }
}

/// A header's element id: stable while its sessions are reordered or closed.
fn group_key(project: &Path) -> u64 {
    let mut hasher = DefaultHasher::new();
    project.hash(&mut hasher);
    hasher.finish()
}

/// The status ring: hollow = idle, yellow = working, red = needs you, green = done (the
/// theme's palette colors).
fn ring(status: Status, ui: UiColors) -> impl IntoElement {
    let dot = div().flex_none().size(px(10.0)).rounded_full();
    match status {
        Status::Idle => dot.border_1().border_color(hsla(ui.ring_idle)),
        Status::Working => dot.bg(hsla(ui.ring_working)),
        Status::Waiting => dot.bg(hsla(ui.ring_waiting)),
        Status::Done => dot.bg(hsla(ui.ring_done)),
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let title = self
            .sessions
            .active()
            .map(|s| format!("fuxx — {}", s.label()))
            .unwrap_or_else(|| "fuxx".into());
        window.set_window_title(&title);
        let theme = cx.global::<Appearance>().theme;
        let ui = theme.ui();
        let colors = PaneColors {
            dim: Hsla {
                a: 0.3,
                ..hsla(theme.background)
            },
            divider: hsla(ui.sidebar_border),
        };

        div()
            .flex()
            .flex_row()
            .size_full()
            .bg(hsla(theme.background))
            .key_context("Workspace")
            .on_action(cx.listener(|this, _: &NewSession, window, cx| this.new_session(window, cx)))
            .on_action(cx.listener(|this, _: &NewSessionInFolder, window, cx| {
                this.new_session_in_folder(window, cx)
            }))
            .on_action(cx.listener(|this, _: &CloseSession, window, cx| {
                if let Some(i) = this.sessions.active_index() {
                    this.close_at(i, window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &NextSession, window, cx| {
                this.sessions.select_next();
                this.activate(window, cx);
            }))
            .on_action(cx.listener(|this, _: &PrevSession, window, cx| {
                this.sessions.select_prev();
                this.activate(window, cx);
            }))
            .on_action(cx.listener(|this, _: &RenameSession, window, cx| {
                if let Some(i) = this.sessions.active_index() {
                    this.start_rename(i, window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &MoveSessionUp, _, cx| this.move_active(true, cx)))
            .on_action(cx.listener(|this, _: &MoveSessionDown, _, cx| this.move_active(false, cx)))
            .on_action(cx.listener(|this, _: &SplitRight, w, cx| this.split(Axis::Row, w, cx)))
            .on_action(cx.listener(|this, _: &SplitDown, w, cx| this.split(Axis::Column, w, cx)))
            .on_action(cx.listener(|this, _: &FocusPaneLeft, w, cx| {
                this.focus_pane(|t, a| t.neighbor(a, Dir::Left), w, cx)
            }))
            .on_action(cx.listener(|this, _: &FocusPaneRight, w, cx| {
                this.focus_pane(|t, a| t.neighbor(a, Dir::Right), w, cx)
            }))
            .on_action(cx.listener(|this, _: &FocusPaneUp, w, cx| {
                this.focus_pane(|t, a| t.neighbor(a, Dir::Up), w, cx)
            }))
            .on_action(cx.listener(|this, _: &FocusPaneDown, w, cx| {
                this.focus_pane(|t, a| t.neighbor(a, Dir::Down), w, cx)
            }))
            .on_action(cx.listener(|this, _: &PrevPane, w, cx| {
                this.focus_pane(|t, a| t.cycle(a, false), w, cx)
            }))
            .on_action(cx.listener(|this, _: &NextPane, w, cx| {
                this.focus_pane(|t, a| t.cycle(a, true), w, cx)
            }))
            .on_action(cx.listener(|this, _: &EqualizePanes, _, cx| this.equalize_panes(cx)))
            .on_action(cx.listener(|this, _: &SelectSession1, w, cx| this.select(0, w, cx)))
            .on_action(cx.listener(|this, _: &SelectSession2, w, cx| this.select(1, w, cx)))
            .on_action(cx.listener(|this, _: &SelectSession3, w, cx| this.select(2, w, cx)))
            .on_action(cx.listener(|this, _: &SelectSession4, w, cx| this.select(3, w, cx)))
            .on_action(cx.listener(|this, _: &SelectSession5, w, cx| this.select(4, w, cx)))
            .on_action(cx.listener(|this, _: &SelectSession6, w, cx| this.select(5, w, cx)))
            .on_action(cx.listener(|this, _: &SelectSession7, w, cx| this.select(6, w, cx)))
            .on_action(cx.listener(|this, _: &SelectSession8, w, cx| this.select(7, w, cx)))
            .on_action(cx.listener(|this, _: &SelectSession9, w, cx| this.select(8, w, cx)))
            .on_action(cx.listener(|this, _: &LaunchPreset1, w, cx| this.launch_preset(0, w, cx)))
            .on_action(cx.listener(|this, _: &LaunchPreset2, w, cx| this.launch_preset(1, w, cx)))
            .on_action(cx.listener(|this, _: &LaunchPreset3, w, cx| this.launch_preset(2, w, cx)))
            .on_action(cx.listener(|this, _: &LaunchPreset4, w, cx| this.launch_preset(3, w, cx)))
            .on_action(cx.listener(|this, _: &LaunchPreset5, w, cx| this.launch_preset(4, w, cx)))
            .on_action(cx.listener(|this, _: &LaunchPreset6, w, cx| this.launch_preset(5, w, cx)))
            .on_action(cx.listener(|this, _: &LaunchPreset7, w, cx| this.launch_preset(6, w, cx)))
            .on_action(cx.listener(|this, _: &LaunchPreset8, w, cx| this.launch_preset(7, w, cx)))
            .on_action(cx.listener(|this, _: &LaunchPreset9, w, cx| this.launch_preset(8, w, cx)))
            .child(self.render_sidebar(ui, cx))
            // Only the active session's layout is rendered; other sessions keep running (their
            // reader threads and tickers live in their views, which the list keeps alive).
            .child(
                div()
                    .relative()
                    .flex_1()
                    .h_full()
                    .overflow_hidden()
                    .child(self.render_pane_area(cx))
                    .children(self.render_panes(colors, cx)),
            )
    }
}

/// The scripting variables for a shell: the socket, the session id, and `fuxx` on the PATH.
fn scripting_env(id: u64) -> Vec<(String, String)> {
    let mut env = Vec::new();
    // Without a socket path the session id means nothing, so set neither.
    if let Some(home) = home_dir() {
        env.push((
            "FUXX_SOCKET".to_string(),
            api_server::socket_path(&home).display().to_string(),
        ));
        env.push(("FUXX_SESSION".to_string(), format!("s{id}")));
    }
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
    {
        let mut path = std::env::var_os("PATH").unwrap_or_default();
        if !path.is_empty() {
            path.push(":");
        }
        path.push(dir);
        env.push(("PATH".to_string(), path.to_string_lossy().into_owned()));
    }
    env
}

/// `$HOME`; unset or empty counts as absent.
fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}
