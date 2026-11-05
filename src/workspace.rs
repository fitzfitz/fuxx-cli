//! The root view: a sidebar listing every session (with its status ring) and the layout of
//! panes holding the active session. Owns session lifecycle, switching, splits, and background
//! notifications.
//!
//! With persistence on (M10a), the background service runs the sessions: ids come from it,
//! opening creates there, closing ends there, and quitting only detaches. A lost connection is
//! not an exit: the sessions stay, marked, and a line says so.
//!
//! An SSH session (M10b) runs `ssh` itself as the pane's program and is grouped by its host.
//! When ssh says the connection failed (exit 255) the pane holds its screen with a reconnect
//! line; Enter (or, with tmux, a timer) reconnects it in place.

use std::cell::Cell;
use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use futures::channel::mpsc::UnboundedReceiver;
use gpui::{
    AnyElement, Bounds, ClickEvent, Context, DispatchPhase, Entity, EntityId, Focusable, Hsla,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PathPromptOptions, Pixels, Point,
    SharedString, Subscription, Task, Window, actions, canvas, div, prelude::*, px, relative,
};

use crate::adopt::{self, Probe};
use crate::api_events::{Event, WaitTable};
use crate::api_server::{self, Conn};
use crate::appearance::Appearance;
use crate::daemon_client::{self, DaemonClient};
use crate::groups::{self, GroupKey, Row};
use crate::mux_proto::{self, Create};
use crate::name_input::{NameInput, NameInputEvent};
use crate::notifier;
use crate::panes::{self, PaneColors, Panes, SelectPane, StartDrag};
use crate::presets::Preset;
use crate::pty::ShellEnv;
use crate::restore::{self, Launch, RestoreMode, SavedSession, SavedState};
use crate::session_list::SessionList;
use crate::settings;
use crate::split_tree::{self, Axis, Dir, Node, Rect};
use crate::ssh_target::SshTarget;
use crate::status::Status;
use crate::terminal_view::{TerminalEvent, TerminalView, hsla};
use crate::theme::UiColors;
use crate::workspace_daemon::{Service, UNANSWERED_NOTICE, open_refusal};
use crate::workspace_ssh::{Disconnected, ExitAction, Place, on_exit, ssh_program, ssh_refusal};

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
    /// A line under the name, set by a script (not restored; the background service keeps it
    /// across reopens).
    pub note: Option<String>,
    /// A bar under the row: percent and an optional label, set by a script (as `note`).
    pub progress: Option<(u8, Option<String>)>,
    /// The background service's connection was lost: the view shows the last screen it had
    /// and nothing reaches the session.
    pub lost: bool,
    /// The host an SSH session connects to (M10b); none for a local one.
    pub ssh: Option<SshTarget>,
    /// SSH: the connection failed and the pane is held for a reconnect.
    pub(crate) disconnected: Option<Disconnected>,
    /// SSH: automatic reconnects in a row so far (each waits longer).
    pub(crate) retries: u32,
    /// When this session (this connection, for SSH) opened.
    pub(crate) opened_at: Instant,
    pub(crate) _subscription: Subscription,
}

/// Where a new session's pane goes, by view (so it holds while the list changes): a layout of
/// its own, split from a pane, or in that pane's place.
#[derive(Clone, Copy)]
pub(crate) enum At {
    New,
    Split(Axis, EntityId),
    Replace(EntityId),
}

/// Runs once a session has opened (with its id) or failed to (`None`).
pub(crate) type Opened =
    Box<dyn FnOnce(&mut Workspace, Option<u64>, &mut Window, &mut Context<Workspace>)>;

impl Session {
    /// Its sidebar group: its host (SSH), else its folder.
    pub fn group(&self) -> Option<GroupKey> {
        groups::key(
            self.ssh.as_ref().map(|t| t.host.as_str()),
            self.project.as_deref(),
        )
    }

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
    group: Option<GroupKey>,
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
    pub(crate) layouts: Vec<Node<EntityId>>,
    /// Where the layout on screen is drawn, in window pixels; written each frame by a canvas.
    pane_area: Rc<Cell<Option<Bounds<Pixels>>>>,
    /// The line being dragged: a pane of its layout (by view) and the path to its split.
    pub(crate) dragging: Option<(EntityId, Vec<bool>)>,
    /// The groups closed in the sidebar.
    pub(crate) collapsed: Vec<GroupKey>,
    /// The rename box, and the session (by its view) it renames.
    pub(crate) renaming: Option<(EntityId, Entity<NameInput>)>,
    /// True while the saved sessions are being reopened, so nothing is saved halfway (and no
    /// events are sent).
    pub(crate) restoring: bool,
    /// Reopening with the background service failed (its List, or the service itself): the
    /// sessions file is kept as it is, for the next launch, instead of saving this run over it.
    pub(crate) hold_saving: bool,
    /// The id the next session gets (`s1`, `s2`, ...); never reused.
    pub(crate) next_id: u64,
    window_active: bool,
    /// Answers scripting requests (see `workspace_api`); none when scripting is off.
    _requests: Option<Task<()>>,
    /// The socket this fuxx serves; none when it isn't serving (scripting off, or another fuxx
    /// has it). Decides the shells' scripting variables.
    socket: Option<PathBuf>,
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
    /// Persistence asked for at startup (`persistent-sessions = on`).
    pub(crate) persistent: bool,
    pub(crate) service: Service,
    /// A line at the foot of the sidebar about the background service.
    pub(crate) notice: Option<SharedString>,
    /// How many sessions the Quit item last said keep running.
    pub(crate) quit_count: usize,
    /// A quit that first ends sessions (or waits for replies) is under way: other quits wait
    /// for it.
    pub(crate) quitting: bool,
    /// On quitting, lets what the views queued for the background service go out first.
    _flush_on_quit: Subscription,
    /// Requests that read a screen not arrived yet from the background service (`deferred`).
    pub(crate) deferred: Vec<crate::workspace_api::Deferred>,
    /// Whether the deferred requests' check is running.
    pub(crate) deferred_ticking: bool,
}

impl Workspace {
    /// `scripting`: the socket this fuxx serves and its requests, answered for as long as the
    /// workspace lives.
    /// `persistent`: sessions run in the background service (connected off the UI thread before
    /// the first one opens).
    pub fn new(
        scripting: Option<(PathBuf, UnboundedReceiver<api_server::Incoming>)>,
        persistent: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let activation = cx.observe_window_activation(window, |this, window, cx| {
            this.window_active = window.is_window_active();
            cx.notify();
        });
        let appearance = cx.observe_global::<Appearance>(|_, cx| cx.notify());
        let (socket, requests) = scripting.unzip();
        let mut this = Self {
            sessions: SessionList::new(),
            layouts: Vec::new(),
            pane_area: Rc::new(Cell::new(None)),
            dragging: None,
            collapsed: Vec::new(),
            renaming: None,
            restoring: true,
            hold_saving: false,
            next_id: 1,
            window_active: window.is_window_active(),
            _requests: requests.map(|rx| Self::serve_requests(rx, window, cx)),
            socket,
            subscribers: Vec::new(),
            waits: WaitTable::new(),
            waits_ticking: false,
            focused: None,
            _activation: activation,
            _appearance: appearance,
            persistent,
            service: if persistent {
                Service::Starting
            } else {
                Service::Off
            },
            notice: None,
            quit_count: 0,
            quitting: false,
            _flush_on_quit: cx.on_app_quit(Self::flush_on_quit),
            deferred: Vec::new(),
            deferred_ticking: false,
        };
        // While a quit ends sessions first, the red button waits for it.
        let me = cx.weak_entity();
        window.on_window_should_close(cx, move |_, cx| {
            !me.upgrade().is_some_and(|this| this.read(cx).quitting)
        });
        if persistent {
            this.connect_service(window, cx);
        } else {
            let probe = this.probe_leftover_service();
            if this.restore(&probe, window, cx) && probe == Probe::NoAnswer {
                this.notice = Some(UNANSWERED_NOTICE.into());
            }
        }
        this
    }

    /// Without the background service (persistence off, or it couldn't be reached): reopen
    /// the saved sessions per the `session-restore` mode, or start one plain session. Saved
    /// sessions the service still runs, or may run (`probe`), aren't started a second time, and
    /// the sessions file is kept for the rest of the run (`adopt::without_service`); true when
    /// that held some back. Saving starts once done. (With the service: `reopen`.)
    pub(crate) fn restore(
        &mut self,
        probe: &Probe,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let (saved, held) = match read_saved() {
            Some(saved) => {
                let (saved, held) = adopt::without_service(saved, probe);
                (Some(saved), held)
            }
            None => (None, false),
        };
        if held {
            eprintln!(
                "[fuxx] restore: saved sessions the background service may still run weren't started again; keeping the sessions file"
            );
            self.hold_saving = true;
        }
        let Some((state, launches)) = self.saved_launches(saved, cx) else {
            self.open_first(window, cx);
            return held;
        };
        self.open_restored(launches, state, window, cx);
        held
    }

    /// Open the saved sessions in order, then put their layouts back (`restored`). SSH ones
    /// first read their ssh settings off the UI thread, so then all of it happens once that's
    /// done.
    fn open_restored(
        &mut self,
        launches: Vec<Launch>,
        state: SavedState,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let targets: Vec<Option<SshTarget>> =
            launches.iter().map(|l| l.saved.ssh.clone()).collect();
        if targets.iter().all(Option::is_none) {
            for launch in launches {
                self.open_session(launch, None, window, cx);
            }
            self.restored(state, window, cx);
            return;
        }
        let programs = cx.background_spawn(async move {
            targets
                .iter()
                .map(|t| t.as_ref().map(|t| ssh_program(t, None)))
                .collect::<Vec<_>>()
        });
        cx.spawn_in(window, async move |this, cx| {
            let programs = programs.await;
            this.update_in(cx, |this, window, cx| {
                for (launch, program) in launches.into_iter().zip(programs) {
                    this.open_now(launch, At::New, program, None, window, cx);
                }
                this.restored(state, window, cx);
            })
            .ok();
        })
        .detach();
    }

    /// `saved`'s launches per the `session-restore` mode; `None` means start one plain session.
    fn saved_launches(
        &self,
        saved: Option<SavedState>,
        cx: &Context<Self>,
    ) -> Option<(SavedState, Vec<Launch>)> {
        let mode = cx.global::<Appearance>().restore;
        if mode == RestoreMode::Off {
            return None;
        }
        let state = saved.filter(|s| !s.sessions.is_empty())?;
        let launches = restore::launches(&state, mode, &cx.global::<Appearance>().presets);
        Some((state, launches))
    }

    /// After the saved sessions were opened: their layouts, groups and active session; or one
    /// plain session when none could start.
    fn restored(&mut self, state: SavedState, window: &mut Window, cx: &mut Context<Self>) {
        if self.sessions.is_empty() {
            eprintln!("[fuxx] restore: no saved session could start, opening a new one");
            self.open_first(window, cx);
            return;
        }
        self.restore_layouts(state.layouts, state.sessions.len());
        self.collapsed = groups::collapsed_keys(&state.collapsed, &state.collapsed_hosts);
        let last = self.sessions.items().len() - 1;
        let active = state.active.unwrap_or(0).min(last);
        // The normal route, so the active session's group opens.
        self.sessions.select(active);
        self.activate(window, cx);
        self.finish_restoring(cx);
    }

    /// One plain session, then saving starts.
    pub(crate) fn open_first(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let done: Opened = Box::new(|this, _, _, cx| this.finish_restoring(cx));
        // Part of the restore, so not refused as other opens are meanwhile.
        self.open_now(Launch::default(), At::New, None, Some(done), window, cx);
    }

    /// Saving starts, and the Quit item says how many sessions keep running.
    pub(crate) fn finish_restoring(&mut self, cx: &mut Context<Self>) {
        self.restoring = false;
        self.save();
        self.sync_quit_title(cx);
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

    /// Whether the background service runs the sessions (their ids are its).
    fn daemon_backed(&self) -> bool {
        !matches!(self.service, Service::Off)
    }

    /// Write the session list to the sessions file (not while restoring). One log line on failure.
    pub(crate) fn save(&self) {
        if self.restoring || self.hold_saving {
            return;
        }
        let Some(home) = home_dir() else { return };
        let mut collapsed = Vec::new();
        let mut collapsed_hosts = Vec::new();
        for key in &self.collapsed {
            match key {
                GroupKey::Folder(dir) => collapsed.push(dir.clone()),
                GroupKey::Host(host) => collapsed_hosts.push(host.clone()),
            }
        }
        let state = SavedState {
            version: restore::VERSION,
            active: self.sessions.active_index(),
            collapsed,
            collapsed_hosts,
            layouts: self.saved_layouts(),
            daemon_instance: self.service.instance().filter(|_| self.daemon_backed()),
            sessions: self
                .sessions
                .items()
                .iter()
                .map(|s| SavedSession {
                    name: s.name.clone(),
                    project: s.project.clone(),
                    preset: s.preset.clone(),
                    daemon_id: self.daemon_backed().then_some(s.id),
                    ssh: s.ssh.clone(),
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
        self.open_session_then(launch, split, None, window, cx);
    }

    /// `open_session`, then `then` with the new session's id (`None`: it didn't start). With the
    /// background service that is once the service has created it; otherwise right away.
    /// Refused (`open_refusal`) while the service is starting, lost, or reopening sessions.
    pub(crate) fn open_session_then(
        &mut self,
        launch: Launch,
        split: Option<Axis>,
        then: Option<Opened>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(why) = open_refusal(&self.service, self.restoring) {
            eprintln!("[fuxx] session: not started, {why}");
            if let Some(then) = then {
                then(self, None, window, cx);
            }
            return;
        }
        // The pane it splits is the one active now, not when the service answers.
        let at = match split.zip(self.sessions.active()) {
            Some((axis, active)) => At::Split(axis, active.view.entity_id()),
            None => At::New,
        };
        self.open_now(launch, at, None, then, window, cx);
    }

    /// `open_session_then` without the refusal (the restore's own opens), placed `at`.
    /// `program`: run it instead of a login shell (an SSH session's `ssh`).
    pub(crate) fn open_now(
        &mut self,
        launch: Launch,
        at: At,
        program: Option<Vec<String>>,
        then: Option<Opened>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let opened = match &self.service {
            Service::Off => self.open_local(launch, at, program, window, cx),
            Service::On(client) => {
                let client = client.clone();
                self.open_in_daemon(client, launch, at, program, then, window, cx);
                return;
            }
            Service::Starting => {
                eprintln!("[fuxx] session: not started, the background service is starting");
                None
            }
            Service::Lost { .. } => {
                eprintln!("[fuxx] session: not started, the background service was lost");
                None
            }
        };
        if let Some(then) = then {
            then(self, opened, window, cx);
        }
    }

    /// A session this process runs (a PTY child); its id.
    fn open_local(
        &mut self,
        launch: Launch,
        at: At,
        program: Option<Vec<String>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<u64> {
        let Launch { saved, run_preset } = launch;
        let (cwd, startup) = start_in(&saved, run_preset);
        let session_id = self.next_id;
        self.next_id += 1;
        let env = scripting_env(
            self.socket.as_deref(),
            Some(session_id),
            exe_dir().as_deref(),
            std::env::var_os("PATH").as_deref(),
        );
        let spawned = TerminalView::spawn(
            window,
            cx,
            program,
            cwd.as_deref(),
            startup.as_deref(),
            &env,
        );
        let view = match spawned {
            Ok(view) => view,
            Err(err) => {
                eprintln!("fuxx: could not start a shell: {err}");
                return None;
            }
        };
        // Not placed (a reconnect whose pane closed meanwhile): dropping the view ends it.
        self.insert_session(session_id, view, saved, at, window, cx)
            .then_some(session_id)
    }

    /// A session the background service runs: created there off the UI thread, then shown.
    fn open_in_daemon(
        &mut self,
        client: Arc<DaemonClient>,
        launch: Launch,
        at: At,
        program: Option<Vec<String>>,
        then: Option<Opened>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ssh = program.is_some();
        let create = self.create_for(&launch, program);
        let pending = client.create_in_background(create);
        cx.spawn_in(window, async move |this, cx| {
            let answer = pending.await;
            let refused = ssh.then(|| ssh_refusal(&answer)).flatten();
            let id = created(answer);
            this.update_in(cx, |this, window, cx| {
                if let Some(why) = refused {
                    this.ssh_failed(&why, cx);
                }
                let id = id
                    .and_then(|id| this.attach_session(&client, id, launch.saved, at, window, cx));
                if let Some(then) = then {
                    then(this, id, window, cx);
                }
            })
            .ok();
        })
        .detach();
    }

    /// The `Create` for `launch`: the scripting variables as `open_local` gives them, except
    /// `FUXX_SESSION`, which the service sets (it knows the id). `program`: run instead of a
    /// login shell.
    pub(crate) fn create_for(&self, launch: &Launch, program: Option<Vec<String>>) -> Create {
        let (cwd, startup) = start_in(&launch.saved, launch.run_preset);
        let env = scripting_env(
            self.socket.as_deref(),
            None,
            exe_dir().as_deref(),
            std::env::var_os("PATH").as_deref(),
        );
        Create {
            program,
            ..daemon_create(
                cwd.as_deref(),
                startup.as_deref(),
                daemon_client::create_env(&env),
                &launch.saved,
            )
        }
    }

    /// Show background-service session `id` (as `open_session` places one); its id. If it
    /// can't be shown it is ended, so it doesn't run unseen.
    fn attach_session(
        &mut self,
        client: &Arc<DaemonClient>,
        id: u64,
        saved: SavedSession,
        at: At,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<u64> {
        let placed = match TerminalView::attach_daemon(window, cx, client.clone(), id) {
            Ok(view) => self.insert_session(id, view, saved, at, window, cx),
            Err(err) => {
                eprintln!("[fuxx] session: could not show s{id}: {err}");
                false
            }
        };
        if !placed {
            let client = client.clone();
            cx.background_spawn(async move { client.close(id) })
                .detach();
        }
        placed.then_some(id)
    }

    /// Put a new session's view in the sidebar (next to its group) and the layouts (its own,
    /// split from a pane, or in a pane's place), and make it active. False if the pane it was
    /// to replace is gone.
    fn insert_session(
        &mut self,
        session_id: u64,
        view: Entity<TerminalView>,
        saved: SavedSession,
        at: At,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let split_at = match at {
            At::Replace(old) => {
                return self.replace_session(old, session_id, view, saved, window, cx);
            }
            At::Split(axis, pane) => Some((axis, pane)),
            At::New => None,
        };
        let subscription = cx.subscribe_in(&view, window, Self::on_terminal_event);
        let id = view.entity_id();
        let split_done = split_at.is_some_and(|(axis, active)| {
            self.layouts
                .iter_mut()
                .find(|t| t.contains(active))
                .is_some_and(|t| t.split(active, id, axis))
        });
        if !split_done {
            self.layouts.push(Node::Leaf(id));
        }
        let keys = self.groups();
        let key = groups::key(
            saved.ssh.as_ref().map(|t| t.host.as_str()),
            saved.project.as_deref(),
        );
        let index = groups::insert_index(&key_refs(&keys), key.as_ref());
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
                note: None,
                progress: None,
                lost: false,
                ssh: saved.ssh,
                disconnected: None,
                retries: 0,
                opened_at: Instant::now(),
                _subscription: subscription,
            },
        );
        self.emit(&Event::Opened, Some(index));
        self.activate(window, cx);
        true
    }

    /// Each session's group, in sidebar order.
    fn groups(&self) -> Vec<Option<GroupKey>> {
        self.sessions.items().iter().map(Session::group).collect()
    }

    /// The active session's folder, so a new session joins its group.
    pub(crate) fn active_project(&self) -> Option<PathBuf> {
        self.sessions.active().and_then(|s| s.project.clone())
    }

    fn new_session(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Another session to the same host, with the same option.
        if let Some((_, target)) = self.active_ssh() {
            self.open_ssh(target, Place::New, window, cx);
            return;
        }
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

    /// The group header's +: a plain session in a folder's group, or another session to a
    /// host (with the tmux option of the group's first session to it).
    fn add_to_group(&mut self, key: &GroupKey, window: &mut Window, cx: &mut Context<Self>) {
        match key {
            GroupKey::Folder(project) => self.open_in(project.clone(), window, cx),
            GroupKey::Host(host) => {
                let target = self
                    .sessions
                    .items()
                    .iter()
                    .find_map(|s| s.ssh.clone().filter(|t| t.host == *host))
                    .unwrap_or_else(|| SshTarget {
                        host: host.clone(),
                        tmux: false,
                    });
                self.open_ssh(target, Place::New, window, cx);
            }
        }
    }

    /// A plain session in `project`'s group.
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

    /// A plain session in the active session's group, as a new pane beside or under it; or
    /// with an SSH session active, another session to its host.
    fn split(&mut self, axis: Axis, window: &mut Window, cx: &mut Context<Self>) {
        if let Some((of, target)) = self.active_ssh() {
            self.open_ssh(target, Place::Split { of, axis }, window, cx);
            return;
        }
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

    pub(crate) fn on_terminal_event(
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
        match event {
            // A held pane's ring stays done.
            TerminalEvent::StatusChanged(new) => {
                if self.sessions.items()[index].disconnected.is_none() {
                    self.set_status(index, *new, cx);
                }
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
            TerminalEvent::Exited { code } => {
                let session = &self.sessions.items()[index];
                if session.disconnected.is_some() {
                    return; // already held
                }
                match on_exit(session.ssh.as_ref(), *code) {
                    // Ended by itself (or by a close already under way): nothing more to end.
                    ExitAction::Close => self.remove_at(index, None, window, cx),
                    ExitAction::Hold => self.hold(index, window, cx),
                }
            }
            TerminalEvent::Reconnect => self.reconnect_ssh(index, false, window, cx),
            TerminalEvent::DaemonLost => self.daemon_lost(index, window, cx),
        }
    }

    /// Session `index`'s status is now `new`: scripts hear of a change, and a notification
    /// follows the M4 rules (not while you look at it).
    pub(crate) fn set_status(&mut self, index: usize, new: Status, cx: &mut Context<Self>) {
        let is_active = self.sessions.active_index() == Some(index);
        let window_active = self.window_active;
        let Some(session) = self.sessions.get_mut(index) else {
            return;
        };
        let old = session.status;
        session.status = new;
        let label = session.label();
        if old != new {
            let change = Event::StatusChanged { from: old, to: new };
            self.emit(&change, Some(index));
        }
        if notifier::should_notify(old, new, is_active, window_active) {
            if let Some(body) = notifier::message(new) {
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

    /// Close session `index` (cmd-w, the API): end it, then take it away. A background-service
    /// session is ended there; a Local one by dropping its view.
    pub(crate) fn close_at(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        // A lost one: no live service has it, and a new one may reuse its number.
        // A held SSH one: its session already ended.
        let ending = self
            .sessions
            .items()
            .get(index)
            .filter(|s| !s.lost && s.disconnected.is_none())
            .and_then(|s| {
                let view = s.view.read(cx);
                view.close();
                view.daemon_id()
            });
        self.remove_at(index, ending, window, cx);
    }

    /// Take session `index` away: from the sidebar and its layout, then the window if it was the
    /// last one. `ending`: the service session `close_at` just ended, which quitting waits for.
    fn remove_at(
        &mut self,
        index: usize,
        ending: Option<u64>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
        // Dropping the Session drops its view -> TerminalCore -> Pty, which kills the child
        // (Local), or detaches (background service; `close_at` already ended it there).
        self.sessions.remove(index);
        if self.sessions.is_empty() {
            self.save(); // an empty list, so the next launch starts fresh
            // Last session gone: close the window (and quit), once wait replies and the closed
            // event have reached their clients.
            replied.extend(self.subscribers.iter().cloned());
            let ending = self.service.client().zip(ending);
            self.quit_after_flush(replied, ending, window, cx);
            return;
        }
        // A group whose last session closed is gone; forget that it was closed.
        let keys = self.groups();
        self.collapsed.retain(|c| keys.contains(&Some(c.clone())));
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
        if let Some(key) = self.sessions.active().and_then(Session::group) {
            self.collapsed.retain(|c| *c != key);
        }
        // A divider drag belongs to the layout it started in; drop it if another is shown now.
        if let Some((pane, _)) = &self.dragging
            && !self.shown_layout().is_some_and(|t| t.contains(*pane))
        {
            self.dragging = None;
        }
        self.focus_active(window, cx);
        self.save();
        self.sync_quit_title(cx);
        cx.notify();
    }

    fn toggle_group(&mut self, key: &GroupKey, cx: &mut Context<Self>) {
        match self.collapsed.iter().position(|c| c == key) {
            Some(i) => {
                self.collapsed.remove(i);
            }
            None => self.collapsed.push(key.clone()),
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
        if let Some(key) = session.group() {
            self.collapsed.retain(|c| *c != key);
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
                    self.push_meta(i, cx);
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
        if groups::same_group(&key_refs(&self.groups()), from, to)
            && self.sessions.move_item(from, to)
        {
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
        if groups::same_group(&key_refs(&self.groups()), from, to)
            && self.sessions.move_item(from, to)
        {
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
        // Shown, so an adopted session fetches its screen now (once; reopening, M10a).
        for id in tree.leaves() {
            if let Some(view) = view(id) {
                view.read(cx).attach();
            }
        }
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
        let keys = self.groups();
        let keys = key_refs(&keys);
        let all: Vec<&GroupKey> = keys.iter().flatten().copied().collect();
        let rows: Vec<AnyElement> = groups::rows(&keys, &self.collapsed)
            .into_iter()
            .map(|row| match row {
                Row::Header {
                    key,
                    first,
                    len,
                    collapsed,
                } => {
                    let label = groups::group_label(&key, &all);
                    self.render_header(key, label, first..first + len, collapsed, ui, cx)
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
            .when_some(self.notice.clone(), |sidebar, notice| {
                sidebar.child(
                    div()
                        .mt_auto()
                        .px_2()
                        .py_1()
                        .rounded_md()
                        .text_xs()
                        .bg(hsla(ui.row_active))
                        .text_color(hsla(ui.text))
                        .child(notice),
                )
            })
    }

    /// A group's header: arrow, name, the group's most urgent ring, and a + for a new session.
    fn render_header(
        &self,
        key: GroupKey,
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
        let add = key.clone();
        div()
            .id(("group", group_id(&key)))
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
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.toggle_group(&key, cx)))
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
                        this.add_to_group(&add, window, cx)
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
        let group = session.group();
        let dragged = DraggedSession {
            view,
            group: group.clone(),
            label: session.label(),
        };
        let name = format!("{}  {}", index + 1, session.label());
        let sub = if session.lost {
            Some("not live".to_string())
        } else {
            sub_line(session.progress.as_ref(), session.note.as_deref())
        };
        let percent = session.progress.as_ref().map(|(p, _)| *p);
        div()
            .id(("session", view))
            .relative()
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
                    .is_some_and(|d| d.group == group)
            })
            .drag_over::<DraggedSession>(move |style, _, _, _| style.bg(hsla(ui.row_active)))
            .on_drop(cx.listener(move |this, dragged: &DraggedSession, _, cx| {
                this.drop_session(dragged.view, view, cx)
            }))
            .child(ring(session.status, ui))
            .map(|row| match rename_box {
                Some(input) => row.child(format!("{}", index + 1)).child(input),
                None => match sub {
                    None => row.child(name),
                    // The note or label sits under the name; both are cut with "…".
                    Some(sub) => row.child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .child(name),
                            )
                            .child(
                                div()
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .text_xs()
                                    .text_color(hsla(ui.text_dim))
                                    .child(sub),
                            ),
                    ),
                },
            })
            // The progress bar: along the bottom edge, as wide as the percent.
            .when_some(percent, |row, percent| {
                row.child(
                    div()
                        .absolute()
                        .bottom_0()
                        .left_0()
                        .h(px(2.0))
                        .w(relative(f32::from(percent) / 100.0))
                        .bg(hsla(ui.ring_working)),
                )
            })
    }
}

/// The line under a session's name: `label · note`, or whichever of them there is.
fn sub_line(progress: Option<&(u8, Option<String>)>, note: Option<&str>) -> Option<String> {
    let label = progress.and_then(|(_, label)| label.as_deref());
    match (label, note) {
        (Some(l), Some(n)) => Some(format!("{l} · {n}")),
        (Some(t), None) | (None, Some(t)) => Some(t.to_string()),
        (None, None) => None,
    }
}

/// A header's element id: stable while its sessions are reordered or closed.
fn group_id(key: &GroupKey) -> u64 {
    let mut hasher = DefaultHasher::new();
    key.hash(&mut hasher);
    hasher.finish()
}

/// `groups()` as the `groups` functions take it.
fn key_refs(keys: &[Option<GroupKey>]) -> Vec<Option<&GroupKey>> {
    keys.iter().map(Option::as_ref).collect()
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

/// The scripting variables for a shell. While this fuxx serves `socket`: the socket, the
/// session id, and the `fuxx` program's folder (`exe_dir`) at the end of `path`. Otherwise both
/// variables are removed, so one inherited from another fuxx can't make `fuxx` in this shell
/// drive that fuxx's sessions. `id: None` (a daemon session): the daemon sets `FUXX_SESSION`,
/// since only it knows the id.
fn scripting_env(
    socket: Option<&Path>,
    id: Option<u64>,
    exe_dir: Option<&Path>,
    path: Option<&OsStr>,
) -> ShellEnv {
    let Some(socket) = socket else {
        return ShellEnv {
            set: Vec::new(),
            remove: vec!["FUXX_SOCKET", "FUXX_SESSION"],
        };
    };
    let mut set = vec![("FUXX_SOCKET".to_string(), socket.display().to_string())];
    if let Some(id) = id {
        set.push(("FUXX_SESSION".to_string(), format!("s{id}")));
    }
    if let Some(dir) = exe_dir {
        let mut path = path.map(OsStr::to_os_string).unwrap_or_default();
        if !path.is_empty() {
            path.push(":");
        }
        path.push(dir);
        set.push(("PATH".to_string(), path.to_string_lossy().into_owned()));
    }
    ShellEnv {
        set,
        remove: Vec::new(),
    }
}

/// The background service's `Create` for a session: as `TerminalView::spawn` would start it
/// (80x24 until the first paint measures the pane), with its sidebar facts.
fn daemon_create(
    cwd: Option<&Path>,
    startup: Option<&str>,
    env: BTreeMap<String, String>,
    saved: &SavedSession,
) -> Create {
    Create {
        cwd: cwd.map(|dir| dir.display().to_string()),
        startup: startup.map(str::to_string),
        env,
        dims: mux_proto::Dims { cols: 80, rows: 24 },
        cell_px: mux_proto::CellPx { w: 0, h: 0 },
        name: saved.name.clone(),
        project: saved.project.as_ref().map(|p| p.display().to_string()),
        preset: saved.preset.as_ref().map(|p| p.name.clone()),
        program: None,
    }
}

/// Where a session for `saved` starts (its project's folder, if it's still there) and what is
/// typed into it first (its preset's command, when `run_preset`).
fn start_in(saved: &SavedSession, run_preset: bool) -> (Option<PathBuf>, Option<String>) {
    let cwd = match saved.project.as_deref() {
        Some(dir) if dir.is_dir() => Some(dir.to_path_buf()),
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
        .map(|p| p.command.clone());
    (cwd, startup)
}

/// The folder of the running `fuxx` program (put on the shells' `PATH`).
fn exe_dir() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
}

/// A `create_in_background` answer as the new id; failures are logged.
pub(crate) fn created(
    answer: Result<std::io::Result<u64>, futures::channel::oneshot::Canceled>,
) -> Option<u64> {
    match answer {
        Ok(Ok(id)) => Some(id),
        Ok(Err(err)) => {
            eprintln!("[fuxx] session: the background service didn't start it: {err}");
            None
        }
        Err(_) => {
            eprintln!("[fuxx] session: the background service didn't answer");
            None
        }
    }
}

/// The sessions file, if there is a valid one (logged when it can't be read or isn't valid).
pub(crate) fn read_saved() -> Option<SavedState> {
    let path = restore::state_path(&home_dir()?);
    match std::fs::read_to_string(&path) {
        Ok(text) => {
            let state = restore::decode(&text);
            if state.is_none() {
                eprintln!(
                    "[fuxx] restore: {} is not a valid sessions file, starting fresh",
                    path.display()
                );
            }
            state
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => None,
        Err(err) => {
            eprintln!(
                "[fuxx] restore: could not read {}: {err}, starting fresh",
                path.display()
            );
            None
        }
    }
}

/// `$HOME`; unset or empty counts as absent.
pub(crate) fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::{daemon_create, scripting_env, sub_line};
    use crate::presets::Preset;
    use crate::pty::apply_env;
    use crate::restore::SavedSession;
    use portable_pty::CommandBuilder;
    use std::collections::BTreeMap;
    use std::ffi::OsStr;
    use std::path::Path;

    #[test]
    fn scripting_variables_follow_whether_this_fuxx_serves_the_socket() {
        let ours = Path::new("/me/Library/Application Support/fuxx/fuxx.sock");
        let exe = Path::new("/apps/fuxx.app/Contents/MacOS");
        for serving in [true, false] {
            // Inherited: as if this fuxx was started from a session of another fuxx.
            for inherited in [true, false] {
                let mut cmd = CommandBuilder::new("sh");
                cmd.env_clear();
                cmd.env("PATH", "/usr/bin");
                if inherited {
                    cmd.env("FUXX_SOCKET", "/other/fuxx.sock");
                    cmd.env("FUXX_SESSION", "s2");
                }
                let env = scripting_env(
                    serving.then_some(ours),
                    Some(7),
                    Some(exe),
                    Some(OsStr::new("/usr/bin")),
                );
                apply_env(&mut cmd, &env);
                let var = |k: &str| cmd.get_env(k).map(|v| v.to_string_lossy().into_owned());
                let case = format!("serving {serving}, inherited {inherited}");
                if serving {
                    assert_eq!(var("FUXX_SOCKET").as_deref(), ours.to_str(), "{case}");
                    assert_eq!(var("FUXX_SESSION").as_deref(), Some("s7"), "{case}");
                    assert_eq!(
                        var("PATH").as_deref(),
                        Some("/usr/bin:/apps/fuxx.app/Contents/MacOS"),
                        "{case}"
                    );
                } else {
                    assert_eq!(var("FUXX_SOCKET"), None, "{case}");
                    assert_eq!(var("FUXX_SESSION"), None, "{case}");
                    assert_eq!(var("PATH").as_deref(), Some("/usr/bin"), "{case}");
                }
            }
        }
    }

    #[test]
    fn a_daemon_session_gets_its_id_from_the_daemon_not_the_window() {
        let ours = Path::new("/me/fuxx.sock");
        let exe = Path::new("/apps/fuxx.app/Contents/MacOS");
        let env = scripting_env(Some(ours), None, Some(exe), Some(OsStr::new("/usr/bin")));
        let keys: Vec<&str> = env.set.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(keys, ["FUXX_SOCKET", "PATH"]);
        assert!(env.remove.is_empty());
        // Not serving: both still removed (the daemon then drops FUXX_SESSION too).
        let off = scripting_env(None, None, Some(exe), None);
        assert!(off.set.is_empty());
        assert_eq!(off.remove, ["FUXX_SOCKET", "FUXX_SESSION"]);
    }

    #[test]
    fn a_daemon_create_carries_the_session_as_spawn_would_start_it() {
        let saved = SavedSession {
            name: Some("api".into()),
            project: Some("/x/api".into()),
            preset: Some(Preset {
                name: "claude".into(),
                command: "claude --resume".into(),
            }),
            daemon_id: None,
            ssh: None,
        };
        let env = BTreeMap::from([("A".to_string(), "1".to_string())]);
        let c = daemon_create(
            Some(Path::new("/x/api")),
            Some("claude --resume"),
            env.clone(),
            &saved,
        );
        assert_eq!(c.cwd.as_deref(), Some("/x/api"));
        assert_eq!(c.startup.as_deref(), Some("claude --resume"));
        assert_eq!(c.env, env);
        assert_eq!((c.dims.cols, c.dims.rows), (80, 24));
        assert_eq!(c.name.as_deref(), Some("api"));
        assert_eq!(c.project.as_deref(), Some("/x/api"));
        assert_eq!(c.preset.as_deref(), Some("claude"));
        // A plain session in home: nothing but the env.
        let plain = daemon_create(None, None, BTreeMap::new(), &SavedSession::default());
        assert_eq!(
            (
                plain.cwd,
                plain.startup,
                plain.name,
                plain.project,
                plain.preset
            ),
            (None, None, None, None, None)
        );
    }

    #[test]
    fn the_line_under_the_name_joins_label_and_note() {
        let bar = |l: Option<&str>| (40, l.map(String::from));
        assert_eq!(sub_line(None, None), None);
        assert_eq!(sub_line(Some(&bar(None)), None), None);
        assert_eq!(sub_line(None, Some("n")), Some("n".into()));
        assert_eq!(sub_line(Some(&bar(Some("l"))), None), Some("l".into()));
        assert_eq!(
            sub_line(Some(&bar(Some("l"))), Some("n")),
            Some("l · n".into())
        );
    }
}
