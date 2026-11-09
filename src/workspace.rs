//! The root view: a sidebar listing every workspace (with its most urgent terminal's status
//! mark) and the focused workspace's layout of panes, each showing its active tab (M11b; the
//! rules are `layout_model`'s). Owns session lifecycle, switching, splits, and background
//! notifications. A session is one terminal (`sN`); every session is shown by exactly one tab.
//!
//! With persistence on (M10a), the background service runs the sessions: ids come from it,
//! opening creates there, closing ends there, and quitting only detaches. A lost connection is
//! not an exit: the sessions stay, marked, and a line says so.
//!
//! An SSH session (M10b) runs `ssh` itself as the pane's program and is grouped by its host.
//! When ssh says the connection failed (exit 255) the pane holds its screen with a reconnect
//! line; Enter (or, with tmux, a timer) reconnects it in place.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashMap};
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use futures::StreamExt;
use futures::channel::mpsc::UnboundedReceiver;
use gpui::{
    AnyElement, App, Bounds, ClickEvent, Context, DispatchPhase, Div, Entity, EntityId,
    FocusHandle, Focusable, KeyDownEvent, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, PathPromptOptions, Pixels, Point, ScrollHandle, SharedString, Stateful,
    StyleRefinement, Subscription, Task, Window, WindowControlArea, actions, canvas, div,
    prelude::*, px,
};

use crate::adopt::{self, Probe};
use crate::api_events::{Event, WaitTable};
use crate::api_server::{self, Conn};
use crate::appearance::Appearance;
use crate::browser_tab::BrowserTab;
use crate::daemon_client::{self, DaemonClient};
use crate::drag_gap::Slides;
use crate::file_tab::FileTab;
use crate::git_branch::{self, Places};
use crate::groups::{self, GroupKey};
use crate::help_panel::HelpPanel;
use crate::host_picker::HostPicker;
use crate::layout_model::{self, Model, PaneId, TabDrop, TabId, TabKind, WorkspaceId};
use crate::mux_proto::{self, Create};
use crate::name_input::{NameInput, NameInputEvent};
use crate::native_notify::{self, PendingShow, Terminal};
use crate::notification_list::{self, NotificationList};
use crate::notifier;
use crate::palette::Palette;
use crate::palette_items::{Recent, Scope};
use crate::pane_actions::PaneActionsMenu;
use crate::pane_tab_menu::PaneTabMenu;
use crate::panes::{self, HoverDivider, PaneColors, Panes, SelectPane, StartDrag};
use crate::presets::Preset;
use crate::procs::{self, Foreground, Proc, ProgramState, Stable};
use crate::pty::ShellEnv;
use crate::reduce_motion::{SystemReduceMotion, WindowVisible};
use crate::restore::{self, Launch, RestoreMode, SavedSession, SavedState};
use crate::right_sidebar::RightSidebarState;
use crate::session_list::SessionList;
use crate::settings;
use crate::sidebar::{self, RowGrab, SidebarRow, SidebarState};
use crate::sidebar_toggle::{self, Slide};
use crate::split_tree::{Axis, Dir, Node, Rect};
use crate::ssh_target::SshTarget;
use crate::status::{self, Status};
use crate::status_bar::Ports;
use crate::status_mark;
use crate::tab_bar::TabGrab;
use crate::terminal_view::{TerminalEvent, TerminalView, hsla};
use crate::theme::UiColors;
use crate::ui_tokens::size::{CARD_GAP, TOP_BAND};
use crate::welcome::Welcome;
use crate::welcome_items::Origin;
use crate::workspace_browser::{AddressBox, Browsers};
use crate::workspace_daemon::{Service, UNANSWERED_NOTICE, open_refusal};
use crate::workspace_ssh::{
    Disconnected, ExitAction, Place, checked_launches, on_exit, ssh_program, ssh_refusal,
};

actions!(
    workspace,
    [
        NewWorkspace,
        NewTab,
        NewSessionInFolder,
        NewSshSession,
        OpenBrowser,
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
        ToggleSidebar,
        ToggleRightSidebar,
        NextNeedingYou,
        OpenSwitcher,
        OpenPalette,
        NextTab,
        PrevTab,
    ]
);

/// How long after a session turns Done its mark is drawn as a pop: the pop, plus a margin for the
/// frame it starts on. After that the mark is drawn plain, so the pop never plays twice.
pub(crate) const POP_SHOWN: std::time::Duration = std::time::Duration::from_millis(450);

/// How often what runs in the terminals (`program_subjects`) is looked up again (one `ps`), at
/// most.
pub(crate) const PROGRAMS_REFRESH: Duration = Duration::from_secs(3);

/// How long after a `ps` that failed (or ran past its cap) the next one runs.
pub(crate) const PROGRAMS_RETRY: Duration = Duration::from_secs(30);

/// The wait before the next `ps` after one that went `ok` (`PROGRAMS_REFRESH`) or failed
/// (`PROGRAMS_RETRY`), from when it began.
pub(crate) fn programs_wait(ok: bool) -> Duration {
    if ok { PROGRAMS_REFRESH } else { PROGRAMS_RETRY }
}

/// What runs in each terminal (M11i), for the status bar's word and ports and an idle row's dot
/// (never its name): read from one `ps` table (`procs::ps_table`, on the background executor)
/// at most every `PROGRAMS_REFRESH`, and kept steady (`Stable`). A terminal never looked up is
/// `Unknown`, not at its prompt.
#[derive(Default)]
pub(crate) struct Programs {
    by_terminal: HashMap<u64, Stable>,
    /// The last table read (the status bar's ports lookup finds a shell's processes in it).
    table: Option<Rc<Vec<Proc>>>,
    /// Nothing is looked up (the UI screenshots set what they show).
    pub frozen: bool,
    /// When `ps` last ran.
    last_run: Option<Instant>,
    /// The last `ps` failed (or ran past its cap): the next waits `PROGRAMS_RETRY`.
    failed: bool,
}

impl Programs {
    /// What terminal `id` is shown running.
    pub fn state(&self, id: u64) -> ProgramState {
        self.by_terminal
            .get(&id)
            .map_or(ProgramState::Unknown, |s| s.shown().clone())
    }

    /// The last `ps` table read; none before the first or after a failed one.
    pub fn table(&self) -> Option<Rc<Vec<Proc>>> {
        self.table.clone()
    }

    /// Show terminal `id` running `state` (the screenshots; nothing is looked up there).
    #[cfg(feature = "ui-shots")]
    pub fn set_for_shots(&mut self, id: u64, state: ProgramState) {
        let mut stable = Stable::new();
        stable.offer(state);
        self.by_terminal.insert(id, stable);
    }

    /// A `ps` table (none: `ps` failed), read for `shells` (each terminal and its shell's pid:
    /// every local one, `ps -A` being system-wide; any other is kept as it was, and only a
    /// closed one is forgotten, by `retain`); true when a terminal's shown program changed. A
    /// failed `ps` keeps the last good answers (and the next waits `PROGRAMS_RETRY`); only the
    /// table goes.
    pub fn take(&mut self, table: Option<Vec<Proc>>, shells: &[(u64, u32)]) -> bool {
        self.failed = table.is_none();
        let Some(table) = table else {
            self.table = None;
            return false;
        };
        let mut changed = false;
        for &(id, shell) in shells {
            let seen = match procs::foreground(&table, shell) {
                Foreground::Prompt => ProgramState::Prompt,
                Foreground::Program(args) => {
                    let name = procs::program_name(&args);
                    if name.is_empty() {
                        ProgramState::Unknown
                    } else {
                        ProgramState::Running(name)
                    }
                }
                Foreground::Unknown => ProgramState::Unknown,
            };
            changed |= self.by_terminal.entry(id).or_default().offer(seen);
        }
        self.table = Some(Rc::new(table));
        changed
    }

    /// How long until `ps` may run again (`programs_wait` after the last run began: 3 s, 30 s
    /// after a failed one), from `now`.
    pub fn wait(&self, now: Instant) -> Duration {
        self.last_run.map_or(Duration::ZERO, |at| {
            programs_wait(!self.failed).saturating_sub(now.saturating_duration_since(at))
        })
    }

    /// Forget terminals that are gone (by their ids still open).
    pub fn retain(&mut self, open: impl Fn(u64) -> bool) {
        self.by_terminal.retain(|id, _| open(*id));
    }
}

impl Workspace {
    /// Session `i`'s terminal and its shell's pid, if its program can be looked up: a local
    /// one. Not an SSH one (its programs run elsewhere) nor a background-service one (fuxx
    /// doesn't have its shell's pid), nor a lost one.
    fn program_shell(&self, i: usize, cx: &App) -> Option<(u64, u32)> {
        let s = self.sessions.items().get(i)?;
        if s.ssh.is_some() || s.lost {
            return None;
        }
        Some((s.id, s.view.read(cx).working_folder().1?))
    }

    /// The sessions (by place in the list) whose program shows, each once
    /// (`git_branch::subjects`): each row in view's named terminal (an idle row's dot or ring
    /// is its; none with the sidebar hidden), then the terminal the status bar follows (its
    /// status word, and its ports only while a program runs).
    fn program_subjects(&self) -> Vec<usize> {
        let rows = self.rows_in_view(Instant::now());
        let named = rows.iter().map(|row| match row {
            SidebarRow::Workspace(w) => self.named_terminal(w.workspace),
            SidebarRow::Header(_) => None,
        });
        git_branch::subjects(named, self.right_sidebar.followed)
            .into_iter()
            .filter_map(|id| self.index_of(id))
            .collect()
    }

    /// The local ones of `program_subjects` with their shell's pid (`program_shell`): while
    /// there are some, the programs' lookup runs.
    fn program_shells(&self, cx: &App) -> Vec<(u64, u32)> {
        self.program_subjects()
            .into_iter()
            .filter_map(|i| self.program_shell(i, cx))
            .collect()
    }

    /// Every local terminal with its shell's pid (`program_shell`): what each table is read
    /// for (`ps -A` is system-wide, so it costs nothing more), so a terminal out of view keeps
    /// its program and shows it at once when it's followed or its row comes into view.
    fn all_program_shells(&self, cx: &App) -> Vec<(u64, u32)> {
        (0..self.sessions.items().len())
            .filter_map(|i| self.program_shell(i, cx))
            .collect()
    }

    /// Whether the programs' lookup should run: the window can be seen (not hidden, minimised
    /// or covered), it isn't frozen, and a local terminal's program shows (`program_shells`).
    fn programs_wanted(&self, cx: &App) -> bool {
        let seen = cx.try_global::<WindowVisible>().is_none_or(|w| w.visible);
        seen && !self.programs.frozen && !self.program_shells(cx).is_empty()
    }

    /// Start the programs' lookup unless it's running or nothing is wanted (`programs_wanted`):
    /// one `ps` on the background executor (capped at 2 s), at most every `PROGRAMS_REFRESH` (also
    /// across restarts), its answer read for every local terminal (`all_program_shells`). It
    /// stops once nothing is wanted, so a hidden or covered window is never woken; `render`
    /// calls it, so showing the window again starts it.
    pub(crate) fn tick_programs(&mut self, cx: &mut Context<Self>) {
        if self.programs_ticking || !self.programs_wanted(cx) {
            return;
        }
        self.programs_ticking = true;
        cx.spawn(async move |this, cx| {
            loop {
                let Ok(wait) = this.update(cx, |this, _| this.programs.wait(Instant::now())) else {
                    break;
                };
                if !wait.is_zero() {
                    cx.background_executor().timer(wait).await;
                }
                let wanted = this
                    .update(cx, |this, cx| {
                        this.programs_ticking = this.programs_wanted(cx);
                        if this.programs_ticking {
                            this.programs.last_run = Some(Instant::now());
                        }
                        this.programs_ticking
                    })
                    .unwrap_or(false);
                if !wanted {
                    break;
                }
                let table = cx
                    .background_executor()
                    .spawn(async { procs::ps_table() })
                    .await;
                let took = this.update(cx, |this, cx| {
                    let shells = this.all_program_shells(cx);
                    if this.programs.take(table, &shells) {
                        cx.notify();
                    }
                });
                if took.is_err() {
                    break;
                }
            }
        })
        .detach();
    }
}

impl Workspace {
    /// The followed terminal's folder, the freshest known (`palette_files::choose_folder`: what
    /// it last reported, as the right sidebar reads it, else where the places last found it,
    /// else the one it started in), and whether it is an SSH one; none with no terminal
    /// followed or no folder known.
    pub(crate) fn palette_folder(&self, cx: &App) -> Option<(PathBuf, bool)> {
        let id = self.right_sidebar.followed?;
        let s = self.sessions.items().iter().find(|s| s.id == id)?;
        let (reported, _) = self.reported_folder(id, cx)?;
        crate::palette_files::choose_folder(
            reported,
            self.places.folder(id),
            s.project.as_deref(),
            s.ssh.is_some(),
        )
    }

    /// List the followed terminal's files when they are due: one listing at a time, on the
    /// background executor, then the cache is filled and the window redrawn; with the palette
    /// still open, whatever is due next is listed then (a listing asked for while this one ran
    /// was turned away). Nothing is listed while frozen (the screenshots give the listings).
    pub(crate) fn list_palette_files(&mut self, cx: &mut Context<Self>) {
        if self.palette_files.frozen {
            return;
        }
        let Some((folder, false)) = self.palette_folder(cx) else {
            return;
        };
        if !self.palette_files.due(&folder, Instant::now()) {
            return;
        }
        self.palette_files.started(&folder);
        cx.spawn(async move |this, cx| {
            let asked = folder.clone();
            let answer = cx
                .background_executor()
                .spawn(async move { crate::palette_files::list_files(&asked) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.palette_files.finished(&folder, answer, Instant::now());
                if this.palette.is_some() {
                    this.list_palette_files(cx);
                }
                cx.notify();
            });
        })
        .detach();
    }
}

impl Workspace {
    /// The followed terminal (`right_sidebar.followed`) if its ports can be looked up, with its
    /// shell's pid: a local one (not SSH, not a background service's, not lost) with a program
    /// running in it.
    fn ports_subject(&self, cx: &App) -> Option<(u64, u32)> {
        let id = self.right_sidebar.followed?;
        let s = self.sessions.items().iter().find(|s| s.id == id)?;
        if s.ssh.is_some() || s.lost {
            return None;
        }
        if !matches!(self.programs.state(id), ProgramState::Running(_)) {
            return None;
        }
        Some((id, s.view.read(cx).working_folder().1?))
    }

    /// Whether the ports can be looked up: the window can be seen and they aren't frozen (the
    /// screenshots).
    fn ports_seen(&self, cx: &App) -> bool {
        let seen = cx.try_global::<WindowVisible>().is_none_or(|w| w.visible);
        seen && !self.ports.frozen
    }

    /// Whether the ports' refresh should run (`ports_seen`, and a subject to look up).
    fn ports_wanted(&self, cx: &App) -> bool {
        self.ports_seen(cx) && self.ports_subject(cx).is_some()
    }

    /// Look the followed terminal's listening ports up where due (`Ports::due`: at once for a
    /// newly followed terminal, else every `PORTS_REFRESH`, 30 s after a failure): its shell's
    /// descendants in the programs' last `ps` table, then one `lsof` on the background executor
    /// (capped at 2 s). With no program running there any more, its ports go. True when what
    /// the bar shows changed now (an answer redraws when it comes). `render` calls it on every
    /// frame, the refresh (`tick_ports`) between frames.
    pub(crate) fn refresh_ports(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.ports_seen(cx) {
            return false;
        }
        let Some((id, shell)) = self.ports_subject(cx) else {
            return self.ports.clear();
        };
        if !self.ports.due(id, Instant::now()) {
            return false;
        }
        let Some(table) = self.programs.table() else {
            return false;
        };
        let pids = procs::descendants(&table, shell);
        self.ports.ask(id);
        cx.spawn(async move |this, cx| {
            let found = cx
                .background_executor()
                .spawn(async move { procs::listening_ports(&pids) })
                .await;
            this.update(cx, |this, cx| {
                let now = Instant::now();
                let changed = this.ports.take(id, found, now);
                // Another terminal followed meanwhile: the redraw asks it at once.
                let followed = this.ports_subject(cx).map(|(followed, _)| followed);
                if this.ports.redraw_after_answer(changed, followed, now) {
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
        false
    }

    /// Start the ports' refresh unless it's running or nothing is wanted (`ports_wanted`): it
    /// wakes when the next lookup is due (`Ports::sleep`: `PORTS_REFRESH` after the last
    /// answer, never sooner than a second) and stops once nothing is wanted, so a hidden
    /// window, or a terminal at its prompt, is never woken for it. `render` calls it, so a
    /// program starting (its name found) or the window shown again starts it.
    pub(crate) fn tick_ports(&mut self, cx: &mut Context<Self>) {
        if self.ports.ticking || !self.ports_wanted(cx) {
            return;
        }
        self.ports.ticking = true;
        cx.spawn(async move |this, cx| {
            loop {
                let wait = this
                    .update(cx, |this, cx| {
                        // (With no program running any more, this lets its ports go.)
                        if this.refresh_ports(cx) {
                            cx.notify();
                        }
                        this.ports.ticking = this.ports_wanted(cx);
                        this.ports.ticking.then(|| this.ports.sleep(Instant::now()))
                    })
                    .ok()
                    .flatten();
                let Some(wait) = wait else {
                    break;
                };
                cx.background_executor().timer(wait).await;
            }
        })
        .detach();
    }
}

pub struct Session {
    /// Scripting id: shown to scripts as `s<id>`.
    pub id: u64,
    pub view: Entity<TerminalView>,
    pub status: Status,
    /// When `status` last changed (a session adopted or restored starts now).
    pub status_since: Instant,
    /// Its mark's Done pop: a number no other pop has (the animation's key) and when it began.
    /// Set when the session turns Done with its row on screen; it plays once.
    pub(crate) pop: Option<(u64, Instant)>,
    /// While Waiting: the screen line that asked (`status::waiting_line`); none otherwise.
    pub question: Option<String>,
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
    /// Its status changes, notes and progress, for the Activity panel (in memory, not saved).
    pub activity: crate::activity::ActivityLog,
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
    /// SSH: this connection is a reconnect of a held session (its drop may be the same one).
    pub(crate) reconnected: bool,
    pub(crate) _subscription: Subscription,
}

/// `Overlay` and `Overlay::ALL` from one list, so no overlay can be left out of `ALL` (the
/// `overlays` UI scene opens each to check it hides the in-pane pages).
macro_rules! overlays {
    ($($variant:ident),* $(,)?) => {
        /// The boxes over the window that take the keyboard; only one is open at a time.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub(crate) enum Overlay {
            $($variant,)*
        }

        impl Overlay {
            /// Every overlay.
            #[cfg_attr(not(feature = "ui-shots"), allow(dead_code))]
            pub(crate) const ALL: &[Overlay] = &[$(Overlay::$variant,)*];
        }
    };
}

overlays!(
    Rename,
    HostPicker,
    AddressBox,
    Palette,
    TabMenu,
    PaneTabMenu,
    PaneActions,
    Notifications,
    Help,
    Welcome,
);

/// What the rename box names: a workspace (its sidebar row) or a terminal (its pane tab).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Renaming {
    Workspace(WorkspaceId),
    Terminal(u64),
}

/// Where a new session goes: a workspace of its own, a new pane split from a pane, a new tab
/// at the end of a pane's, or in the place of a session (by its view: an SSH reconnect).
#[derive(Clone, Copy)]
pub(crate) enum At {
    New,
    Split(Axis, PaneId),
    Tab(PaneId),
    Replace(EntityId),
}

/// Runs once a session has opened (with its id) or failed to (`None`).
pub(crate) type Opened =
    Box<dyn FnOnce(&mut Workspace, Option<u64>, &mut Window, &mut Context<Workspace>)>;

/// Whether a mouse move with `pressed` held starts the window moving (from a pressed band).
fn starts_window_move(pressed: Option<MouseButton>) -> bool {
    pressed == Some(MouseButton::Left)
}

impl Session {
    /// Its sidebar group: its host (SSH), else its folder.
    pub fn group(&self) -> Option<GroupKey> {
        groups::key(
            self.ssh.as_ref().map(|t| t.host.as_str()),
            self.project.as_deref(),
        )
    }

    /// Your name, else the program's own title, else the preset's name, else ╭∩╮.
    pub fn label(&self) -> SharedString {
        groups::auto_name(
            self.name.as_deref(),
            self.title.as_deref(),
            self.preset.as_ref().map(|p| p.name.as_str()),
        )
        .into()
    }
}

pub struct Workspace {
    /// The terminals, in sidebar order (workspaces in display order, then each one's terminals
    /// in reading order: `sync`); the active one is the focused tab's.
    pub(crate) sessions: SessionList<Session>,
    /// The workspaces, their panes and tabs (M11b). Every session is shown by exactly one tab.
    pub(crate) model: Model,
    /// Where the layout on screen is drawn, in window pixels; written each frame by a canvas.
    pane_area: Rc<Cell<Option<Bounds<Pixels>>>>,
    /// The line being dragged: its workspace and the path to its split.
    pub(crate) dragging: Option<(WorkspaceId, Vec<bool>)>,
    /// The divider the pointer is over, like `dragging`.
    divider_hover: Option<(WorkspaceId, Vec<bool>)>,
    /// The groups closed in the sidebar.
    pub(crate) collapsed: Vec<GroupKey>,
    /// The rename box, and what it renames.
    pub(crate) renaming: Option<(Renaming, Entity<NameInput>)>,
    /// A pane tab's right-click menu, while it is open.
    pub(crate) pane_tab_menu: Option<PaneTabMenu>,
    /// A pane bar's ⋯ menu, while it is open (M11i).
    pub(crate) pane_actions: Option<PaneActionsMenu>,
    /// Each pane's tab row: its scroll, and the tab last scrolled into view, if any (a pane whose
    /// shown tab changes scrolls to it). Panes gone are dropped as the bars are drawn.
    pub(crate) tab_scroll: RefCell<HashMap<PaneId, (ScrollHandle, Option<TabId>)>>,
    /// The tab being dragged and where it was taken, noted as its drag starts (M11b Task 7);
    /// see `dragged_tab`.
    pub(crate) tab_drag: Rc<Cell<Option<TabGrab>>>,
    /// While a tab is dragged: where dropping it would move it (a bar's gap, a sidebar row);
    /// none over anything else, or where it would land where it is.
    pub(crate) tab_drop: Option<TabDrop>,
    /// While a tab or a row is dragged, the others' slides out of its way (M11b Task 7b).
    pub(crate) drag_slides: RefCell<Slides>,
    /// The SSH host picker, while it is open (M10b).
    pub(crate) host_picker: Option<Entity<HostPicker>>,
    /// The ⌘K command palette (⌘P opens it on Workspaces), while it is open (M11j).
    pub(crate) palette: Option<Entity<Palette>>,
    /// What the palette picked last, newest first (kept while it's closed; never saved).
    pub(crate) palette_recent: Recent,
    /// The Help panel (Keyboard Shortcuts or the Command Reference), while it is open (M11d).
    pub(crate) help: Option<Entity<HelpPanel>>,
    /// The Welcome card, while it is open (M11k).
    pub(crate) welcome: Option<Entity<Welcome>>,
    /// The card's agent detection never runs (the UI screenshots set the answers themselves).
    pub(crate) agents_frozen: bool,
    /// The browser tabs' views (M11b), by tab: one for each browser tab of the model (`sync`
    /// drops those whose tab has gone, and with them their pages).
    pub(crate) browser_tabs: HashMap<TabId, Entity<BrowserTab>>,
    /// The file tabs' views (M11g), by tab: one for each file tab of the model (`sync` drops
    /// those whose tab has gone).
    pub(crate) file_tabs: HashMap<TabId, Entity<FileTab>>,
    /// Each browser tab's title as its page last said (for the tab bar and a workspace's name).
    pub(crate) browser_titles: HashMap<TabId, String>,
    /// Browser tabs just closed whose pages still have to leave the window.
    pub(crate) closed_browser_tabs: Vec<Entity<BrowserTab>>,
    /// Counts the browser tabs' uses (`BrowserTab::last_used`).
    pub(crate) browser_clock: u64,
    /// When this workspace was made (an age for things with no status of their own).
    pub(crate) started: Instant,
    /// The browser tab focused as of the last frame (it counts as used when it becomes so).
    pub(crate) focused_browser: Option<TabId>,
    /// Browser tabs popped out into windows of their own (M10c's), by a number of their own.
    pub(crate) browsers: Browsers,
    pub(crate) next_popped: u64,
    /// Whether the 500 ms check of those windows is running (only while one is open).
    pub(crate) browsers_ticking: bool,
    /// The browser's address box, while it is open (M10c).
    pub(crate) address_box: Option<Entity<AddressBox>>,
    /// The saved browser data may hold something (a window was opened with `browser-keep-data`
    /// on): kept in the sessions file until it has been deleted.
    pub(crate) browser_data_kept: bool,
    /// The Welcome card has been shown (M11k): kept in the sessions file.
    pub(crate) welcomed: bool,
    /// Deleting the saved browser data is under way.
    pub(crate) browser_wiping: bool,
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
    /// Hears clicks on fuxx's banners (`listen_for_clicks`).
    _clicks: Option<Task<()>>,
    /// Hears clicks on the background service's banners, which it passes on (`listen_for_shows`).
    _shows: Option<Task<()>>,
    /// A banner click held while the saved sessions reopen, shown when they have.
    pending_show: PendingShow,
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
    /// Repaints the sidebar when the appearance changes (and deletes the browser's saved data
    /// when `browser-keep-data` is turned off).
    _appearance: Subscription,
    /// Repaint (and re-check the marks' ticker) when macOS Reduce motion changes, or the window
    /// is hidden or shown.
    _reduce_motion: Subscription,
    _window_visible: Subscription,
    /// Browser tabs' pages follow macOS's light or dark when it changes.
    _system_appearance: Subscription,
    /// The status marks' shared clock (see `status_mark`), advanced by their ticker.
    pub(crate) phase: f32,
    /// Whether the marks' 100 ms ticker is running (only while a mark on screen moves).
    marks_ticking: bool,
    /// The last Done pop's number.
    pops: u64,
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
    /// The sidebar: its width, whether it's hidden (⌘\; the pane area then has the top band
    /// itself), its scroll, its hint card and menu.
    pub(crate) sidebar: SidebarState,
    /// The top band was pressed and the mouse hasn't moved since: the next move drags the window.
    pub(crate) band_pressed: bool,
    /// The sidebar sliding in or out (`sidebar_toggle`); none when it isn't moving.
    pub(crate) slide: Option<Slide>,
    /// The right sidebar (M11g): open or closed (⌥⌘\), its width, its slide, the terminal it
    /// follows and that terminal's folder (`right_sidebar`).
    pub(crate) right_sidebar: RightSidebarState,
    /// Where each workspace is (its folder and git branch), for its row (`git_branch`).
    pub(crate) places: Places,
    /// Whether the places' 5 s refresh is running (only while a row can be seen).
    pub(crate) places_ticking: bool,
    /// The palette's file listings (`palette_files`).
    pub(crate) palette_files: crate::palette_files::PaletteFiles,
    /// What runs in each terminal, for its name (`Programs`).
    pub(crate) programs: Programs,
    /// Whether the programs' 3 s lookup is running (only while a name it gives can be seen).
    pub(crate) programs_ticking: bool,
    /// The followed terminal's listening ports, for the status bar (`status_bar::Ports`).
    pub(crate) ports: Ports,
    /// The name the rename box was opened with: Enter on it unchanged keeps the name automatic.
    pub(crate) renamed_from: String,
    /// This run's notifications, for the title area's 🔔 (`title_area`).
    pub(crate) notifications: NotificationList,
    /// The 🔔's popover, while it is open: the focus it holds (so Escape closes it).
    pub(crate) notifications_popover: Option<FocusHandle>,
}

impl Workspace {
    /// `scripting`: the socket this fuxx serves and its requests, answered for as long as the
    /// workspace lives.
    /// `clicks`: clicks on fuxx's banners, each the terminal it is about and the run that
    /// numbered it (`native_notify::install`).
    /// `persistent`: sessions run in the background service (connected off the UI thread before
    /// the first one opens).
    pub fn new(
        scripting: Option<(PathBuf, UnboundedReceiver<api_server::Incoming>)>,
        clicks: UnboundedReceiver<Option<Terminal>>,
        persistent: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut this = Self::blank(scripting, persistent, window, cx);
        this.listen_for_clicks(clicks, window, cx);
        // Browser data saved by an earlier run, if the setting is off now, goes at once.
        this.browser_data_kept = read_saved_quietly().is_some_and(|s| s.browser_data_kept);
        this.welcomed = !restore::welcome_due(read_saved_quietly().as_ref());
        this.wipe_saved_data_if_due(cx);
        if let Some(saved) = read_saved_quietly() {
            this.sidebar.load(&saved);
            this.right_sidebar.load(&saved);
        }
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

    /// A workspace with no sessions, still restoring (so nothing is saved yet).
    fn blank(
        scripting: Option<(PathBuf, UnboundedReceiver<api_server::Incoming>)>,
        persistent: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let activation = cx.observe_window_activation(window, |this, window, cx| {
            this.window_active = window.is_window_active();
            cx.notify();
        });
        let appearance = cx.observe_global::<Appearance>(|this, cx| {
            this.browser_settings_changed(cx);
            cx.notify();
        });
        let reduce_motion = cx.observe_global::<SystemReduceMotion>(|_, cx| cx.notify());
        let window_visible = cx.observe_global::<WindowVisible>(|_, cx| cx.notify());
        let system_appearance = cx.observe_window_appearance(window, |this, _, cx| {
            for view in this.browser_tabs.values() {
                view.read(cx).follow_appearance();
            }
        });
        let (socket, requests) = scripting.unzip();
        Self {
            sessions: SessionList::new(),
            model: Model::new(),
            pane_area: Rc::new(Cell::new(None)),
            dragging: None,
            divider_hover: None,
            collapsed: Vec::new(),
            renaming: None,
            pane_tab_menu: None,
            pane_actions: None,
            tab_scroll: RefCell::new(HashMap::new()),
            tab_drag: Rc::new(Cell::new(None)),
            tab_drop: None,
            drag_slides: RefCell::default(),
            host_picker: None,
            palette: None,
            palette_recent: Recent::default(),
            help: None,
            welcome: None,
            agents_frozen: false,
            browser_tabs: HashMap::new(),
            file_tabs: HashMap::new(),
            browser_titles: HashMap::new(),
            closed_browser_tabs: Vec::new(),
            browser_clock: 0,
            focused_browser: None,
            started: Instant::now(),
            browsers: Browsers::new(),
            next_popped: 1,
            browsers_ticking: false,
            address_box: None,
            browser_data_kept: false,
            welcomed: true,
            browser_wiping: false,
            restoring: true,
            hold_saving: false,
            next_id: 1,
            window_active: window.is_window_active(),
            _requests: requests.map(|rx| Self::serve_requests(rx, window, cx)),
            _clicks: None,
            _shows: None,
            pending_show: PendingShow::default(),
            socket,
            subscribers: Vec::new(),
            waits: WaitTable::new(),
            waits_ticking: false,
            focused: None,
            _activation: activation,
            _appearance: appearance,
            _reduce_motion: reduce_motion,
            _window_visible: window_visible,
            _system_appearance: system_appearance,
            phase: 0.0,
            marks_ticking: false,
            pops: 0,
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
            sidebar: SidebarState::new(),
            band_pressed: false,
            slide: None,
            right_sidebar: RightSidebarState::new(),
            places: Places::new(home_dir(), crate::git_branch::this_hostname()),
            places_ticking: false,
            palette_files: Default::default(),
            programs: Programs::default(),
            programs_ticking: false,
            ports: Ports::default(),
            renamed_from: String::new(),
            notifications: NotificationList::default(),
            notifications_popover: None,
        }
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

    /// Open the saved sessions in order (each with its place in the file's `sessions`), then
    /// put their workspaces back (`restored`). SSH ones first read their ssh settings off the
    /// UI thread, so then all of it happens once that's done.
    fn open_restored(
        &mut self,
        launches: Vec<(usize, Launch)>,
        state: SavedState,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // A saved host is checked as a typed one is: a refused one isn't started.
        let (launches, refused) = checked_launches(launches);
        for why in refused {
            self.ssh_failed(&why, cx);
        }
        let targets: Vec<Option<SshTarget>> =
            launches.iter().map(|(_, l)| l.saved.ssh.clone()).collect();
        let open_all = move |this: &mut Self,
                             programs: Vec<Option<Vec<String>>>,
                             window: &mut Window,
                             cx: &mut Context<Self>| {
            // Without the background service (the only way here) each opens at once, so its
            // terminal is known when `open_now` returns.
            let terminals = Rc::new(std::cell::RefCell::new(vec![None; state.sessions.len()]));
            for ((i, launch), program) in launches.into_iter().zip(programs) {
                let terminals = terminals.clone();
                let opened: Opened = Box::new(move |_, id, _, _| {
                    if let Some(slot) = terminals.borrow_mut().get_mut(i) {
                        *slot = id;
                    }
                });
                this.open_now(launch, At::New, program, Some(opened), window, cx);
            }
            let terminals = terminals.take();
            this.restored(state, &terminals, window, cx);
        };
        if targets.iter().all(Option::is_none) {
            let programs = vec![None; targets.len()];
            open_all(self, programs, window, cx);
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
            this.update_in(cx, |this, window, cx| open_all(this, programs, window, cx))
                .ok();
        })
        .detach();
    }

    /// `saved`'s launches per the `session-restore` mode, each with its place in the file's
    /// `sessions`; `None` means start one plain session.
    fn saved_launches(
        &self,
        saved: Option<SavedState>,
        cx: &Context<Self>,
    ) -> Option<(SavedState, Vec<(usize, Launch)>)> {
        let mode = cx.global::<Appearance>().restore;
        if mode == RestoreMode::Off {
            return None;
        }
        let state = saved.filter(restore::has_anything)?;
        // Not off: one per saved session, in order.
        let launches = restore::launches(&state, mode, &cx.global::<Appearance>().presets)
            .into_iter()
            .enumerate()
            .collect();
        Some((state, launches))
    }

    /// After the saved sessions were opened (`terminals`: per saved session, the terminal it
    /// opened as): their workspaces, groups and focus; or one plain session when nothing came
    /// back (no session could start and no workspace holds a page).
    fn restored(
        &mut self,
        state: SavedState,
        terminals: &[Option<u64>],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let model = Model::from_state(&state, terminals);
        if !restore::shows_anything(&model, !self.sessions.is_empty()) {
            eprintln!("[fuxx] restore: no saved session could start, opening a new one");
            self.open_first(window, cx);
            return;
        }
        if terminals.iter().any(Option::is_none) {
            eprintln!(
                "[fuxx] restore: a session didn't start; its neighbours take its place in its workspace"
            );
        }
        self.set_model(model);
        self.collapsed = groups::collapsed_keys(&state.collapsed, &state.collapsed_hosts);
        // The normal route, so the focused workspace's group opens.
        self.activate(window, cx);
        self.finish_restoring(window, cx);
    }

    /// Take `model` (a restore's): every session it doesn't show (never, by how it is built)
    /// gets a workspace of its own, so none runs unseen.
    pub(crate) fn set_model(&mut self, model: Model) {
        self.model = model;
        self.model.reconcile(&self.session_ids());
        self.sync();
    }

    /// Every session's id (sN), in list order.
    fn session_ids(&self) -> Vec<u64> {
        self.sessions.items().iter().map(|s| s.id).collect()
    }

    /// One plain session, then saving starts; on a first run (nothing says the Welcome card was
    /// shown) the card opens over it once it has.
    pub(crate) fn open_first(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let done: Opened = Box::new(|this, opened, window, cx| {
            this.finish_restoring(window, cx);
            if let Some(over) = opened.filter(|_| !this.welcomed) {
                this.open_welcome(Origin::FirstRun { over }, window, cx);
            }
        });
        // Part of the restore, so not refused as other opens are meanwhile.
        self.open_now(Launch::default(), At::New, None, Some(done), window, cx);
    }

    /// Saving starts, the Quit item says how many sessions keep running, and a banner click
    /// that came meanwhile is shown.
    pub(crate) fn finish_restoring(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.restoring = false;
        self.save();
        self.sync_quit_title(cx);
        if let Some(terminal) = self.pending_show.restored() {
            self.show_clicked(terminal, window, cx);
        }
    }

    /// Hear clicks on fuxx's banners for as long as the workspace lives (clicks that came before
    /// it existed are waiting in `clicks`).
    pub(crate) fn listen_for_clicks(
        &mut self,
        clicks: UnboundedReceiver<Option<Terminal>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self._clicks = Some(Self::hear(clicks, window, cx));
    }

    /// Hear the clicks on the background service's banners that it passes on (`Show`, from
    /// `DaemonClient::shows`), each a session numbered by that service (`run`, its instance), for
    /// as long as the workspace lives or the connection lasts. They are handled as clicks on the
    /// window's own banners, so one that comes while the saved sessions reopen (it opened fuxx)
    /// is shown once they have.
    pub(crate) fn listen_for_shows(
        &mut self,
        shows: UnboundedReceiver<u64>,
        run: Option<u64>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let clicks = shows.map(move |id| run.map(|run| Terminal { run, id }));
        self._shows = Some(Self::hear(clicks, window, cx));
    }

    /// Hand each click from `clicks` to `banner_clicked`, until it ends or the workspace goes.
    fn hear(
        mut clicks: impl futures::Stream<Item = Option<Terminal>> + Unpin + 'static,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<()> {
        cx.spawn_in(window, async move |this, cx| {
            while let Some(terminal) = clicks.next().await {
                let heard = this.update_in(cx, |this, window, cx| {
                    this.banner_clicked(terminal, window, cx)
                });
                if heard.is_err() {
                    break;
                }
            }
        })
    }

    /// A click on one of fuxx's banners: fuxx comes forward and terminal `sN` (if the banner
    /// names one) is shown, now or, while the saved sessions reopen, once they have.
    fn banner_clicked(
        &mut self,
        terminal: Option<Terminal>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.activate(true);
        if let Some(terminal) = self.pending_show.click(terminal, self.restoring) {
            self.show_clicked(terminal, window, cx);
        }
    }

    /// Show a clicked banner's terminal as the 🔔 list does (nothing more if it has closed
    /// since); one about no terminal, or about one numbered by another run (today's `sN` is
    /// another terminal), only brought fuxx forward.
    fn show_clicked(
        &mut self,
        terminal: Option<Terminal>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let run = self.numbering_run();
        if let Some(id) = terminal.and_then(|t| t.in_run(run)) {
            self.jump_to_notification(id, window, cx);
        }
    }

    /// The run that numbers the sessions now (`workspace_daemon::numbering_run`).
    fn numbering_run(&self) -> Option<u64> {
        crate::workspace_daemon::numbering_run(&self.service, native_notify::this_run())
    }

    /// What a banner about session `terminal` names: it and the run that numbered it. None when
    /// that run isn't known (a click on it then only brings fuxx forward).
    pub(crate) fn banner_about(&self, terminal: Option<u64>) -> Option<Terminal> {
        let run = self.numbering_run()?;
        terminal.map(|id| Terminal { run, id })
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
        let state = self.saved_state();
        let path = restore::state_path(&home);
        if let Err(err) = settings::write_atomic(&path, &restore::encode(&state)) {
            eprintln!("[fuxx] restore: could not save {}: {err}", path.display());
        }
    }

    /// What `save` writes to the sessions file.
    pub(crate) fn saved_state(&self) -> SavedState {
        let mut collapsed = Vec::new();
        let mut collapsed_hosts = Vec::new();
        for key in &self.collapsed {
            match key {
                GroupKey::Folder(dir) => collapsed.push(dir.clone()),
                GroupKey::Host(host) => collapsed_hosts.push(host.clone()),
            }
        }
        let (workspaces, active_workspace) = self.model.to_saved(|id| self.index_of(id));
        SavedState {
            version: restore::VERSION,
            active: self.sessions.active_index(),
            collapsed,
            collapsed_hosts,
            layouts: Vec::new(),
            workspaces,
            active_workspace,
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
                    // Colours are the workspaces' now (M11b).
                    color: None,
                })
                .collect(),
            sidebar_width: Some(self.sidebar.width),
            sidebar_hidden: self.sidebar.hidden,
            hint_dismissed: self.sidebar.hint_dismissed,
            browser_data_kept: self.browser_data_kept,
            welcomed: self.welcomed,
            right_sidebar_hidden: self.right_sidebar.hidden,
            right_sidebar_width: Some(self.right_sidebar.width),
            right_sidebar_panel: Some(self.right_sidebar.panel.key().to_string()),
        }
    }

    /// Open a session for `launch` and make it active: a workspace of its own (next to its
    /// group), or with `split`, a new pane beside (Row) or under (Column) the focused one.
    pub(crate) fn open_session(
        &mut self,
        launch: Launch,
        split: Option<Axis>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // The pane it splits is the one focused now, not when the service answers.
        let at = match split.zip(self.model.focused_pane()) {
            Some((axis, pane)) => At::Split(axis, pane),
            None => At::New,
        };
        self.open_session_then(launch, at, None, window, cx);
    }

    /// Open a session for `launch` `at` a place, then `then` with the new session's id (`None`:
    /// it didn't start). With the background service that is once the service has created it;
    /// otherwise right away. Refused (`open_refusal`) while the service is starting, lost, or
    /// reopening sessions.
    pub(crate) fn open_session_then(
        &mut self,
        launch: Launch,
        at: At,
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
        let env = session_env(
            self.socket.as_deref(),
            Some(session_id),
            exe_dir().as_deref(),
            ssh_login_env(&saved),
            program.is_none(),
            zsh_integration(cx),
        );
        let spawned = TerminalView::spawn(cx, program, cwd.as_deref(), startup.as_deref(), &env);
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
        let ssh = launch.saved.ssh.is_some();
        let create = self.create_for(&launch, program, cx);
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

    /// The `Create` for `launch`: the environment as `open_local` gives it, except
    /// `FUXX_SESSION`, which the service sets (it knows the id). `program`: run instead of a
    /// login shell.
    pub(crate) fn create_for(
        &self,
        launch: &Launch,
        program: Option<Vec<String>>,
        cx: &App,
    ) -> Create {
        let (cwd, startup) = start_in(&launch.saved, launch.run_preset);
        let env = self.create_env(&launch.saved, program.is_none(), cx);
        Create {
            program,
            ..daemon_create(cwd.as_deref(), startup.as_deref(), env, &launch.saved)
        }
    }

    /// The environment a background-service session for `saved` starts with; `login_shell`:
    /// it runs your login shell (no `program`), which may get the zsh integration.
    pub(crate) fn create_env(
        &self,
        saved: &SavedSession,
        login_shell: bool,
        cx: &App,
    ) -> BTreeMap<String, String> {
        daemon_client::create_env(&session_env(
            self.socket.as_deref(),
            None,
            exe_dir().as_deref(),
            ssh_login_env(saved),
            login_shell,
            zsh_integration(cx),
        ))
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
        let placed = match TerminalView::attach_daemon(cx, client.clone(), id) {
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

    /// Put a new session's view in a tab: of a workspace of its own (next to its group), of a
    /// pane split from a pane, at the end of a pane's tabs, or in a session's place; and make it
    /// active. A pane gone meanwhile: a workspace of its own. False if the session it was to
    /// replace is gone.
    fn insert_session(
        &mut self,
        session_id: u64,
        view: Entity<TerminalView>,
        saved: SavedSession,
        at: At,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let kind = TabKind::Terminal(session_id);
        let placed = match at {
            At::Replace(old) => {
                return self.replace_session(old, session_id, view, saved, window, cx);
            }
            At::Split(axis, pane) => self.model.split(pane, axis, kind.clone()),
            At::Tab(pane) => self.model.add_tab(pane, kind.clone()),
            At::New => None,
        };
        if placed.is_none() {
            let keys = self.sidebar_groups();
            let key = groups::key(
                saved.ssh.as_ref().map(|t| t.host.as_str()),
                saved.project.as_deref(),
            );
            let at = groups::insert_index(&key_refs(&keys), key.as_ref());
            if self.model.new_workspace(at, kind).is_none() {
                eprintln!("[fuxx] session: s{session_id} is already shown");
                return false;
            }
        }
        // A split puts a new divider at the hovered path's place, with no enter event.
        self.divider_hover = None;
        let subscription = cx.subscribe_in(&view, window, Self::on_terminal_event);
        let end = self.sessions.items().len();
        self.sessions.insert(
            end,
            Session {
                id: session_id,
                view,
                status: Status::Idle,
                status_since: Instant::now(),
                pop: None,
                question: None,
                title: None,
                name: saved.name,
                project: saved.project,
                preset: saved.preset,
                note: None,
                progress: None,
                activity: Default::default(),
                lost: false,
                ssh: saved.ssh,
                disconnected: None,
                retries: 0,
                opened_at: Instant::now(),
                reconnected: false,
                _subscription: subscription,
            },
        );
        self.sync();
        self.emit(&Event::Opened, self.index_of(session_id));
        self.activate(window, cx);
        true
    }

    /// After the model changed: the workspaces' groups kept together (the sidebar draws each
    /// under one header), the sessions in sidebar order, and the focused tab's the active one.
    pub(crate) fn sync(&mut self) {
        // Every session has exactly one terminal tab and every terminal tab a session: each op
        // keeps it, so a mismatch is a bug. Said, caught in debug builds, and repaired.
        let repaired = self.model.reconcile(&self.session_ids());
        if !repaired.is_empty() {
            eprintln!(
                "[fuxx] layout: tabs and sessions were out of step; {}",
                repaired.join("; ")
            );
            debug_assert!(repaired.is_empty(), "tabs and sessions out of step");
        }
        // A ⋯ menu whose pane went goes with it.
        if let Some(menu) = &self.pane_actions
            && self.model.pane(menu.pane).is_none()
        {
            self.pane_actions = None;
        }
        let ids = self.session_ids();
        self.places.retain(|id| ids.contains(&id));
        self.programs.retain(|id| ids.contains(&id));
        let sessions = self.sessions.items();
        let group_of = |id: u64| {
            sessions
                .iter()
                .find(|s| s.id == id)
                .and_then(Session::group)
        };
        let model = &self.model;
        let first = |w: &layout_model::Workspace| model.terminals(w.id).first().copied();
        let groups: Vec<(WorkspaceId, Option<GroupKey>)> = model
            .workspaces()
            .iter()
            .map(|w| (w.id, first(w).and_then(group_of)))
            .collect();
        let group = |w: &layout_model::Workspace| {
            groups
                .iter()
                .find(|(id, _)| *id == w.id)
                .and_then(|(_, key)| key.clone())
        };
        self.model.group_together(group);
        let order: Vec<u64> = self
            .model
            .display_order()
            .into_iter()
            .flat_map(|w| self.model.terminals(w))
            .collect();
        self.sessions.sort_by_key(|s| {
            order
                .iter()
                .position(|&id| id == s.id)
                .unwrap_or(usize::MAX)
        });
        // A browser tab focused: no terminal is the active one (⌘W, a new session's folder and
        // the scripts' "focused" go by the focused tab).
        match self.focused_terminal().and_then(|id| self.index_of(id)) {
            Some(i) => {
                self.sessions.select(i);
            }
            None if self.model.focused_tab().is_some() => self.sessions.deselect(),
            None => {}
        }
        // A browser tab gone (closed, its pane or workspace closed): its page goes with its view.
        let model = &self.model;
        let is_browser = |tab: &TabId| {
            model
                .tab(*tab)
                .is_some_and(|t| matches!(t.kind, TabKind::Browser { .. }))
        };
        let gone: Vec<TabId> = self
            .browser_tabs
            .keys()
            .copied()
            .filter(|tab| !is_browser(tab))
            .collect();
        for tab in gone {
            if let Some(view) = self.browser_tabs.remove(&tab) {
                // Its page leaves the window at once (`close_browser_pages`).
                self.closed_browser_tabs.push(view);
            }
        }
        self.browser_titles.retain(|tab, _| is_browser(tab));
        // A file tab gone: its view goes.
        let model = &self.model;
        self.file_tabs.retain(|tab, _| {
            model
                .tab(*tab)
                .is_some_and(|t| matches!(t.kind, TabKind::File { .. }))
        });
        // Every op keeps the model's rules; a break is a bug, said where it can be found.
        if let Err(broken) = self.model.check() {
            eprintln!("[fuxx] layout: {broken}");
        }
    }

    /// The terminal the focused tab shows.
    pub(crate) fn focused_terminal(&self) -> Option<u64> {
        self.terminal_of(self.model.focused_tab()?)
    }

    /// The terminal a tab shows (none for a browser or file tab).
    pub(crate) fn terminal_of(&self, tab: layout_model::TabId) -> Option<u64> {
        match self.model.tab(tab)?.kind {
            TabKind::Terminal(id) => Some(id),
            TabKind::Browser { .. } | TabKind::File { .. } => None,
        }
    }

    /// Session `sN`'s place in the list.
    pub(crate) fn index_of(&self, id: u64) -> Option<usize> {
        self.sessions.position(|s| s.id == id)
    }

    /// The pane showing session `index`.
    pub(crate) fn pane_of(&self, index: usize) -> Option<PaneId> {
        let tab = self
            .model
            .terminal_tab(self.sessions.items().get(index)?.id)?;
        self.model.locate(tab).map(|(_, pane)| pane)
    }

    /// The workspace showing session `index`.
    pub(crate) fn workspace_of(&self, index: usize) -> Option<WorkspaceId> {
        let tab = self
            .model
            .terminal_tab(self.sessions.items().get(index)?.id)?;
        self.model.locate(tab).map(|(ws, _)| ws)
    }

    /// The session pane `pane` shows (its active tab's terminal).
    pub(crate) fn pane_session(&self, pane: PaneId) -> Option<&Session> {
        let id = self.terminal_of(self.model.pane(pane)?.active)?;
        self.sessions.items().iter().find(|s| s.id == id)
    }

    /// A workspace's sidebar group: its first terminal's (its host, else its folder).
    pub(crate) fn workspace_group(&self, ws: WorkspaceId) -> Option<GroupKey> {
        let first = *self.model.terminals(ws).first()?;
        self.sessions
            .items()
            .iter()
            .find(|s| s.id == first)
            .and_then(Session::group)
    }

    /// Each workspace's group, in sidebar order (pinned ones not moved up).
    pub(crate) fn sidebar_groups(&self) -> Vec<Option<GroupKey>> {
        self.model
            .workspaces()
            .iter()
            .map(|w| self.workspace_group(w.id))
            .collect()
    }

    /// The run of rows a workspace's row moves within (a drag, Move Up / Down): the pinned
    /// rows (true) together, else its group's.
    pub(crate) fn sidebar_run(&self, ws: WorkspaceId) -> (bool, Option<GroupKey>) {
        if self.model.workspace(ws).is_some_and(|w| w.pinned) {
            (true, None)
        } else {
            (false, self.workspace_group(ws))
        }
    }

    /// A workspace's group as the sidebar labels it; none when it has no group.
    pub(crate) fn group_label(&self, ws: WorkspaceId) -> Option<String> {
        let key = self.workspace_group(ws)?;
        let keys = self.sidebar_groups();
        let all: Vec<&GroupKey> = keys.iter().flatten().collect();
        Some(groups::group_label(&key, &all))
    }

    /// The terminal a workspace is named after: its focused pane's, else its first.
    pub(crate) fn named_terminal(&self, ws: WorkspaceId) -> Option<u64> {
        let w = self.model.workspace(ws)?;
        self.pane_session(w.focused)
            .map(|s| s.id)
            .or_else(|| self.model.terminals(ws).first().copied())
    }

    /// Terminal `id`'s name as fuxx shows it: its `Session::label` (yours, else the program's
    /// own title, else its preset's name, else ╭∩╮), as the scripting API names it.
    pub(crate) fn terminal_label(&self, id: u64) -> SharedString {
        self.sessions
            .items()
            .iter()
            .find(|s| s.id == id)
            .map_or_else(|| groups::DEFAULT_NAME.into(), Session::label)
    }

    /// A row's tooltip (`sidebar::row_tip`): its name and, under it, the program's own title
    /// when it says more, then a waiting row's whole question (its most urgent terminal's).
    pub(crate) fn workspace_tip(&self, ws: WorkspaceId) -> Option<String> {
        let label = self.workspace_label(ws);
        let title = self
            .named_terminal(ws)
            .and_then(|id| self.sessions.items().iter().find(|s| s.id == id))
            .and_then(|s| s.title.as_deref());
        let question = self
            .rollup_session(ws)
            .map(|i| &self.sessions.items()[i])
            .filter(|s| s.status == Status::Waiting)
            .and_then(|s| s.question.as_deref());
        sidebar::row_tip(&label, title, question)
    }

    /// A workspace's name: yours, else its focused pane's terminal's (`Session::label`), else
    /// its first terminal's, else the shown page's title or file's name, else "Browser".
    pub(crate) fn workspace_label(&self, ws: WorkspaceId) -> SharedString {
        let Some(w) = self.model.workspace(ws) else {
            return groups::DEFAULT_NAME.into();
        };
        if let Some(name) = w.name.as_deref().filter(|n| !n.trim().is_empty()) {
            return name.to_string().into();
        }
        self.named_terminal(ws)
            .and_then(|id| self.sessions.items().iter().find(|s| s.id == id))
            .map(Session::label)
            .or_else(|| {
                let tab = self.model.pane(w.focused)?.active;
                Some(self.browser_titles.get(&tab)?.clone().into())
            })
            .or_else(|| {
                // A workspace of file tabs only: the shown file's name.
                let tab = self.model.pane(w.focused)?.active;
                let TabKind::File { path, .. } = &self.model.tab(tab)?.kind else {
                    return None;
                };
                Some(path.file_name()?.to_string_lossy().into_owned().into())
            })
            .unwrap_or_else(|| sidebar::BROWSER_NAME.into())
    }

    /// A workspace's place in the sidebar, from 1 (what ⌘1–⌘9 and the window title show).
    pub(crate) fn workspace_number(&self, ws: WorkspaceId) -> Option<usize> {
        self.model
            .display_order()
            .iter()
            .position(|&w| w == ws)
            .map(|at| at + 1)
    }

    /// The session a workspace's ring is (`Model::rollup`): its most urgent terminal's index.
    pub(crate) fn rollup_session(&self, ws: WorkspaceId) -> Option<usize> {
        let items = self.sessions.items();
        let status = |id: u64| {
            items
                .iter()
                .find(|s| s.id == id)
                .map_or(Status::Idle, |s| s.status)
        };
        let (id, _) = self.model.rollup(ws, status)?;
        self.index_of(id)
    }

    /// The active session's folder, so a new session joins its group; with a browser tab
    /// focused, its workspace's group's (its first terminal's).
    pub(crate) fn active_project(&self) -> Option<PathBuf> {
        if let Some(session) = self.sessions.active() {
            return session.project.clone();
        }
        let ws = self.model.focused_workspace()?;
        let first = *self.model.terminals(ws).first()?;
        let i = self.index_of(first)?;
        self.sessions.items()[i].project.clone()
    }

    pub(crate) fn new_session(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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

    /// ⌘T: a new terminal tab at the end of the focused pane's bar, as its new-terminal button
    /// makes one (also when the focused tab is a browser). With no pane (no workspace open) it
    /// is a new workspace.
    pub(crate) fn new_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.model.focused_pane() {
            Some(pane) => self.new_terminal_tab(pane, window, cx),
            None => self.new_session(window, cx),
        }
    }

    /// Ask for a folder, then open a session in it. Cancel does nothing.
    pub(crate) fn new_session_in_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let picked = cx.prompt_for_paths(folder_panel());
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
    pub(crate) fn add_to_group(
        &mut self,
        key: &GroupKey,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
                        tmux_name: None,
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
    pub(crate) fn split(&mut self, axis: Axis, window: &mut Window, cx: &mut Context<Self>) {
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
    pub(crate) fn launch_preset(&mut self, n: usize, window: &mut Window, cx: &mut Context<Self>) {
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
                    self.set_status(index, *new, true, cx);
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
                match on_exit(session.ssh.as_ref(), *code, session.opened_at.elapsed()) {
                    // Ended by itself (or by a close already under way): nothing more to end.
                    ExitAction::Close => self.remove_at(index, None, window, cx),
                    ExitAction::Hold(held) => self.hold(index, held, window, cx),
                }
            }
            TerminalEvent::Reconnect => self.reconnect_ssh(index, false, window, cx),
            TerminalEvent::DaemonLost => self.daemon_lost(index, window, cx),
        }
    }

    /// Session `index`'s status is now `new`: scripts hear of a change, and with `notify` a
    /// notification follows the M4 rules (not while you look at it).
    pub(crate) fn set_status(
        &mut self,
        index: usize,
        new: Status,
        notify: bool,
        cx: &mut Context<Self>,
    ) {
        let is_active = self.sessions.active_index() == Some(index);
        let window_active = self.window_active;
        let pops = !self.restoring && self.row_on_screen(index);
        let Some(session) = self.sessions.get_mut(index) else {
            return;
        };
        let old = session.status;
        // How long it worked, when a Working stretch ends here (for a Done notification).
        let worked = (old == Status::Working && new != old).then(|| session.status_since.elapsed());
        session.status = new;
        // The cursor takes the status colour (M11f).
        session.view.update(cx, |view, cx| view.set_status(new, cx));
        // The log's `lasted` is read before `status_since` is reset.
        let lasted = session.status_since.elapsed();
        if old != new {
            session.status_since = Instant::now();
        }
        // Done pops once, as it arrives, if you can see it.
        if new != Status::Done {
            session.pop = None;
        } else if old != Status::Done && pops {
            self.pops += 1;
            session.pop = Some((self.pops, Instant::now()));
        }
        session.question = match new {
            Status::Waiting => {
                status::waiting_line(&session.view.read(cx).tail_text(status::TAIL_LINES))
            }
            _ => None,
        };
        let id = session.id;
        let question = session.question.clone();
        if old != new {
            session
                .activity
                .status(new, lasted, question.clone(), SystemTime::now());
        }
        let terminal = Some(id);
        let label = self.terminal_label(id);
        if old != new {
            let change = Event::StatusChanged { from: old, to: new };
            self.emit(&change, Some(index));
        }
        if notify && notifier::should_notify(old, new, is_active, window_active) {
            if let Some((message, body)) =
                notifier::message(new).zip(notifier::body(new, question.as_deref(), worked))
            {
                // The group says more than the name (often ╭∩╮) among many workspaces; the number
                // is its workspace's (what ⌘1–⌘9 take you to).
                let ws = self.workspace_of(index);
                let who = ws
                    .and_then(|ws| self.group_label(ws))
                    .unwrap_or_else(|| label.to_string());
                let number = ws.and_then(|ws| self.workspace_number(ws)).unwrap_or(0);
                let summary = notification_summary(number, &who, message);
                let posted = Event::Notification {
                    title: &summary,
                    body: &body,
                };
                self.emit(&posted, Some(index));
                self.note_notification(notification_list::Entry {
                    terminal: Some(self.sessions.items()[index].id),
                    title: ws.map_or_else(
                        || label.to_string(),
                        |ws| self.workspace_label(ws).to_string(),
                    ),
                    message: body.clone(),
                    status: Some(new),
                    at: Instant::now(),
                });
                let about = self.banner_about(terminal);
                cx.background_spawn(async move { notifier::fire(&summary, &body, about) })
                    .detach();
            }
        }
        self.check_waits(&[], cx);
        cx.notify();
    }

    /// A script set (or cleared) session `index`'s note: keep it, log it, tell the background
    /// service. (Notes restored or adopted from the service don't come through here.)
    pub(crate) fn set_note(&mut self, index: usize, note: Option<String>, cx: &mut Context<Self>) {
        let Some(session) = self.sessions.get_mut(index) else {
            return;
        };
        session.activity.note(note.as_deref(), SystemTime::now());
        session.note = note;
        self.push_meta(index, cx);
        cx.notify();
    }

    /// A script set (or cleared) session `index`'s progress (as `set_note`).
    pub(crate) fn set_progress(
        &mut self,
        index: usize,
        progress: Option<(u8, Option<String>)>,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = self.sessions.get_mut(index) else {
            return;
        };
        session
            .activity
            .progress(progress.clone(), SystemTime::now());
        session.progress = progress;
        self.push_meta(index, cx);
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
        let Some(session_id) = self.sessions.items().get(index).map(|s| s.id) else {
            return;
        };
        // While its row number and name still stand.
        self.emit(&Event::Closed, Some(index));
        // Waits get a last look at its screen and status (PASS printed just before the shell
        // exited still counts), then exit / change waits are met.
        let mut replied = self.check_waits(&[session_id], cx);
        self.dragging = None; // the layout changes under a drag
        self.divider_hover = None; // and a divider may be gone, with no leave event
        // Its tab goes: the pane's next tab shows, or the pane's neighbour takes its space (and
        // the focus if it had it), or the workspace goes (the next one gets the focus).
        if let Some(tab) = self.model.terminal_tab(session_id) {
            self.model.close_tab(tab);
        }
        // Dropping the Session drops its view -> TerminalCore -> Pty, which kills the child
        // (Local), or detaches (background service; `close_at` already ended it there).
        self.sessions.remove(index);
        // Browser tabs or popped-out windows may still be there; with nothing left at all the
        // window goes.
        if self.all_gone() {
            self.save(); // an empty list, so the next launch starts fresh
            // Last session gone: close the window (and quit), once wait replies and the closed
            // event have reached their clients.
            replied.extend(self.subscribers.iter().cloned());
            let ending = self.service.client().zip(ending);
            self.quit_after_flush(replied, ending, window, cx);
            return;
        }
        // A workspace whose last session closed is gone: its menu, rename box and the group
        // it alone was in (forget that it was closed).
        if let Some(menu) = &self.sidebar.tab_menu
            && self.model.workspace(menu.workspace).is_none()
        {
            self.sidebar.tab_menu = None;
        }
        let renamed_gone = match self.renaming.as_ref().map(|(r, _)| *r) {
            Some(Renaming::Workspace(ws)) => self.model.workspace(ws).is_none(),
            Some(Renaming::Terminal(id)) => id == session_id,
            None => false,
        };
        if renamed_gone {
            self.renaming = None;
        }
        // And a tab menu on its tab.
        if let Some(menu) = &self.pane_tab_menu
            && self.model.tab(menu.tab).is_none()
        {
            self.pane_tab_menu = None;
        }
        self.sync();
        let keys = self.sidebar_groups();
        self.collapsed.retain(|c| keys.contains(&Some(c.clone())));
        // An open palette with the keyboard keeps it (a script closed this terminal under it):
        // focused again before the frame, it never sees itself lose the focus, and its list
        // follows on the next draw (M11j). An open Welcome card keeps it through
        // `focus_active` (M11k).
        let keeps = self
            .palette
            .as_ref()
            .map(|p| p.focus_handle(cx))
            .filter(|f| f.contains_focused(window, cx));
        self.activate(window, cx);
        if let Some(keeps) = keeps {
            keeps.focus(window, cx);
        }
    }

    /// Show session `index` (its tab, pane and workspace) and make it the active one.
    pub(crate) fn select(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(tab) = self
            .sessions
            .items()
            .get(index)
            .and_then(|s| self.model.terminal_tab(s.id))
        else {
            return;
        };
        if self.model.select_tab(tab) {
            self.sync();
            self.activate(window, cx);
        }
    }

    /// Show a workspace, on the pane it last had focused (⌘1–⌘9, its sidebar row).
    pub(crate) fn select_workspace(
        &mut self,
        ws: WorkspaceId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.model.focus_workspace(ws) {
            self.sync();
            self.activate(window, cx);
        }
    }

    /// ⌘`digit`: the workspace in that place in the sidebar.
    fn select_shortcut(&mut self, digit: u8, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(ws) = self.model.by_shortcut(digit) {
            self.select_workspace(ws, window, cx);
        }
    }

    /// ⇧⌘] / ⇧⌘[: the next (or previous) workspace in the sidebar, wrapping.
    fn cycle_workspace(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        let order = self.model.display_order();
        let Some(at) = self
            .model
            .focused_workspace()
            .and_then(|f| order.iter().position(|&w| w == f))
        else {
            return;
        };
        let n = order.len();
        let next = if forward {
            (at + 1) % n
        } else {
            (at + n - 1) % n
        };
        self.select_workspace(order[next], window, cx);
    }

    /// A click in a pane, or a move to it: give it the focus (if it hasn't already).
    fn select_pane(&mut self, pane: PaneId, window: &mut Window, cx: &mut Context<Self>) {
        if self.model.focused_pane() != Some(pane) {
            if self.model.focus_pane(pane) {
                self.sync();
                self.activate(window, cx);
            }
        } else {
            // A click on the focused pane takes the keyboard back from a rename, address or
            // picker box.
            self.focus_active(window, cx);
        }
    }

    /// The layout of the focused workspace (the one on screen).
    pub(crate) fn shown_layout(&self) -> Option<&Node<PaneId>> {
        let ws = self.model.focused_workspace()?;
        self.model.workspace(ws).map(|w| &w.layout)
    }

    /// The terminals on screen: each pane's shown one in the focused workspace.
    pub(crate) fn on_screen(&self) -> Vec<u64> {
        let Some(tree) = self.shown_layout() else {
            return Vec::new();
        };
        tree.leaves()
            .into_iter()
            .filter_map(|p| self.pane_session(p).map(|s| s.id))
            .collect()
    }

    /// Move the focus to the pane `pick` finds from the focused one, in the layout on screen.
    fn focus_pane(
        &mut self,
        pick: impl FnOnce(&Node<PaneId>, PaneId) -> Option<PaneId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(focused) = self.model.focused_pane() else {
            return;
        };
        if let Some(next) = self.shown_layout().and_then(|t| pick(t, focused)) {
            self.select_pane(next, window, cx);
        }
    }

    /// Give every pane of the layout on screen an equal share.
    fn equalize_panes(&mut self, cx: &mut Context<Self>) {
        if let Some(ws) = self.model.focused_workspace() {
            self.equalize_layout(ws, cx);
        }
    }

    /// Give every pane of a workspace an equal share.
    pub(crate) fn equalize_layout(&mut self, ws: WorkspaceId, cx: &mut Context<Self>) {
        if self.model.equalize(ws) {
            self.save();
            cx.notify();
        }
    }

    /// A line of the layout on screen was pressed: follow the mouse until it's released.
    fn start_drag(&mut self, path: Vec<bool>, cx: &mut Context<Self>) {
        if let Some(ws) = self.model.focused_workspace() {
            self.dragging = Some((ws, path));
            cx.notify(); // the next frame listens for the moves
        }
    }

    /// The dragged line follows the mouse: its split's ratio is where `position` falls in it.
    fn drag_to(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        let (Some((ws, path)), Some(bounds)) = (&self.dragging, self.pane_area.get()) else {
            return;
        };
        let Some(tree) = self.model.workspace(*ws).map(|w| &w.layout) else {
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
        let (ws, path) = (*ws, path.clone());
        if self.model.set_ratio(ws, &path, ratio) {
            cx.notify();
        }
    }

    /// The pointer entered (`over`) or left the line of the split at `path` in the layout on
    /// screen: it is drawn as the accent line while the pointer is on it.
    fn hover_divider(&mut self, path: Vec<bool>, over: bool, cx: &mut Context<Self>) {
        let Some(ws) = self.model.focused_workspace() else {
            return;
        };
        let now = Some((ws, path));
        let next = if over {
            now
        } else if self.divider_hover == now {
            None
        } else {
            return; // already moved on to another line
        };
        if self.divider_hover != next {
            self.divider_hover = next;
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
        self.close_menus();
        self.note_active(cx);
        self.focus_active(window, cx);
    }

    /// The keyboard is going to the focused tab, so the menus and the 🔔's popover close: the
    /// tab menus and the popover hold it (left open, Escape would go to the terminal), and every
    /// one of them, the + menu too, hides the pages while it's open. The boxes (rename, address,
    /// picker, palette) end on losing the keyboard themselves.
    pub(crate) fn close_menus(&mut self) {
        self.sidebar.tab_menu = None;
        self.pane_tab_menu = None;
        self.pane_actions = None;
        self.notifications_popover = None;
        self.sidebar.new_menu = false;
    }

    /// `activate` without moving the focus (an SSH session reconnected by its timer, M10b).
    pub(crate) fn note_active(&mut self, cx: &mut Context<Self>) {
        self.note_focus();
        self.sidebar.reveal_active = true;
        if let Some(key) = self
            .model
            .focused_workspace()
            .and_then(|ws| self.workspace_group(ws))
        {
            self.collapsed.retain(|c| *c != key);
        }
        // A divider drag belongs to the workspace it started in; drop it if another is shown.
        if let Some((ws, _)) = &self.dragging
            && self.model.focused_workspace() != Some(*ws)
        {
            self.dragging = None;
        }
        self.save();
        self.sync_quit_title(cx);
        cx.notify();
    }

    /// Close every box over the window but `keep`, before `keep` takes the keyboard, and the +
    /// menu (it holds no keyboard, so it isn't an `Overlay`, but it mustn't stay over the box).
    /// Each box's late "lost focus" then finds it already closed and leaves the focus alone
    /// (their event handlers ignore a box that isn't the current one).
    pub(crate) fn close_overlays(&mut self, keep: Overlay) {
        self.sidebar.new_menu = false;
        if keep != Overlay::Rename {
            self.renaming = None;
        }
        if keep != Overlay::HostPicker {
            self.host_picker = None;
        }
        if keep != Overlay::AddressBox {
            self.address_box = None;
        }
        if keep != Overlay::Palette {
            self.palette = None;
        }
        if keep != Overlay::TabMenu {
            self.sidebar.tab_menu = None;
        }
        if keep != Overlay::PaneTabMenu {
            self.pane_tab_menu = None;
        }
        if keep != Overlay::PaneActions {
            self.pane_actions = None;
        }
        if keep != Overlay::Notifications {
            self.notifications_popover = None;
        }
        if keep != Overlay::Help {
            self.help = None;
        }
        if keep != Overlay::Welcome {
            self.welcome = None;
        }
    }

    /// Open the rename box on a workspace's row (opening its group) and focus it.
    pub(crate) fn start_rename(
        &mut self,
        ws: WorkspaceId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.model.workspace(ws).is_none() {
            return;
        }
        let text = self.workspace_label(ws).to_string();
        self.renamed_from = text.clone();
        if let Some(key) = self.workspace_group(ws) {
            self.collapsed.retain(|c| *c != key);
        }
        self.close_overlays(Overlay::Rename);
        // The old name selected, so typing replaces it.
        let input = cx.new(|cx| {
            let mut input = NameInput::new(text, window, cx);
            input.select_all();
            input
        });
        cx.subscribe_in(&input, window, Self::on_name_event)
            .detach();
        input.focus_handle(cx).focus(window, cx);
        self.renaming = Some((Renaming::Workspace(ws), input));
        self.save(); // opening the group may have changed what's collapsed
        cx.notify();
    }

    /// Rename Tab…: the rename box in the place of terminal `id`'s tab title, focused. It names
    /// the terminal (as `session.rename`), not its workspace.
    pub(crate) fn start_tab_rename(
        &mut self,
        id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.index_of(id).is_none() {
            return;
        }
        let text = self.terminal_label(id).to_string();
        self.renamed_from = text.clone();
        self.close_overlays(Overlay::Rename);
        let input = cx.new(|cx| {
            let mut input = NameInput::new(text, window, cx);
            input.select_all();
            input
        });
        cx.subscribe_in(&input, window, Self::on_name_event)
            .detach();
        input.focus_handle(cx).focus(window, cx);
        self.renaming = Some((Renaming::Terminal(id), input));
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
        let Some((renaming, _)) = self.renaming.as_ref().filter(|(_, i)| i == input) else {
            return;
        };
        let renaming = *renaming;
        self.renaming = None;
        // What the box was opened with: confirmed unchanged, an automatic name stays automatic
        // (and keeps following the program's title).
        let opened_with = std::mem::take(&mut self.renamed_from);
        if let NameInputEvent::Confirm(text) = event {
            match renaming {
                Renaming::Workspace(ws) => {
                    if let Some(named) = self.model.workspace(ws).map(|w| w.name.is_some()) {
                        // Enter on an untouched automatic name keeps it automatic (it still
                        // follows the focused terminal's name).
                        if named || *text != opened_with {
                            let name = (!text.is_empty()).then(|| text.clone());
                            self.model.set_name(ws, name);
                        }
                        self.save();
                    }
                }
                Renaming::Terminal(id) => {
                    // As `session.rename`; an untouched automatic name stays automatic too.
                    if let Some(i) = self.index_of(id) {
                        let session = &self.sessions.items()[i];
                        if session.name.is_some() || *text != opened_with {
                            self.rename_at(i, text, cx);
                        }
                    }
                }
            }
        }
        self.focus_active(window, cx);
        cx.notify();
    }

    /// Drop row `dragged` on the sidebar's list: it goes where its gap was (a slot of its run,
    /// `sidebar_run`). With its gap at its own place, it stays.
    pub(crate) fn drop_workspace(&mut self, dragged: WorkspaceId, cx: &mut Context<Self>) {
        let slot = self.sidebar.drop_slot.take();
        cx.notify(); // the gap closes
        let Some(slot) = slot else {
            return;
        };
        let run = self.sidebar_run(dragged);
        let in_run: Vec<bool> = self
            .model
            .display_order()
            .iter()
            .map(|w| self.sidebar_run(*w) == run)
            .collect();
        if let Some(to) = sidebar::run_drop_index(&in_run, slot)
            && self.model.move_workspace_display(dragged, to)
        {
            self.sync();
            self.save();
        }
    }

    /// The tab being dragged, while one is (a row's drag clears it as it starts, and GPUI ends
    /// every drag on release).
    pub(crate) fn dragged_tab(&self, cx: &App) -> Option<TabId> {
        self.tab_grab(cx).map(|grab| grab.tab)
    }

    /// The sidebar row being dragged, while one is (a tab's drag clears it as it starts; with no
    /// drag at all, `render` does).
    pub(crate) fn dragged_row(&self, cx: &App) -> Option<WorkspaceId> {
        self.row_grab(cx).map(|grab| grab.workspace)
    }

    /// The sidebar row being dragged and where it was taken, while one is.
    pub(crate) fn row_grab(&self, cx: &App) -> Option<RowGrab> {
        self.sidebar
            .drag_source
            .get()
            .filter(|_| cx.has_active_drag())
    }

    /// The tab being dragged and where it was taken, while one is.
    pub(crate) fn tab_grab(&self, cx: &App) -> Option<TabGrab> {
        self.tab_drag.get().filter(|_| cx.has_active_drag())
    }

    /// Dragged tab `tab` dropped on a bar or row: it goes where the line (or highlight) was, if
    /// `here` (the place under the pointer) says it was this target's; with none, it stays.
    /// Whether it moved.
    pub(crate) fn drop_tab(
        &mut self,
        tab: TabId,
        here: impl FnOnce(TabDrop) -> bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.tab_drag.set(None);
        let at = self.tab_drop.take().filter(|at| here(*at));
        cx.notify(); // the line or highlight goes
        let Some((pane, index)) = at.and_then(|at| self.model.drop_place(tab, at)) else {
            return false;
        };
        self.move_tab(tab, pane, index, window, cx);
        true
    }

    /// Esc during a tab drag: it ends where it started, nothing moved.
    fn cancel_tab_drag(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.dragged_tab(cx).is_none() {
            return false;
        }
        cx.stop_active_drag(window);
        self.tab_drag.set(None);
        self.tab_drop = None;
        cx.notify();
        true
    }

    /// Move `tab` to `index` in `pane`'s bar (a drop): it is shown and takes the keyboard, and
    /// the view follows it to its workspace. Its terminal or page goes as it is (the same view:
    /// the same `sN`, program, screen and status). A pane or workspace it leaves empty closes,
    /// without asking: nothing ends.
    pub(crate) fn move_tab(
        &mut self,
        tab: TabId,
        pane: PaneId,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(closed) = self.model.move_tab(tab, pane, index) else {
            return;
        };
        self.dragging = None; // the layouts change under a divider drag
        self.divider_hover = None; // and a divider may be gone, with no leave event
        // A workspace that closed: its rename box goes (`activate` closes the menus).
        if let Some((Renaming::Workspace(ws), _)) = &self.renaming
            && closed.workspaces.contains(ws)
        {
            self.renaming = None;
        }
        self.sync();
        let keys = self.sidebar_groups();
        self.collapsed.retain(|c| keys.contains(&Some(c.clone())));
        self.activate(window, cx); // saves
    }

    /// Swap the focused workspace with its neighbour above (or below), inside its run only
    /// (`sidebar_run`).
    fn move_active(&mut self, up: bool, cx: &mut Context<Self>) {
        let order = self.model.display_order();
        let Some(from) = self
            .model
            .focused_workspace()
            .and_then(|f| order.iter().position(|&w| w == f))
        else {
            return;
        };
        let Some(to) = (if up {
            from.checked_sub(1)
        } else {
            Some(from + 1)
        }) else {
            return;
        };
        let Some(&neighbour) = order.get(to) else {
            return;
        };
        if self.sidebar_run(order[from]) == self.sidebar_run(neighbour)
            && self.model.move_workspace_display(order[from], to)
        {
            self.sync();
            self.save();
            cx.notify();
        }
    }

    /// The keyboard to the focused tab: its terminal, a file tab, or a browser tab (GPUI's
    /// focus only: its page takes AppKit's keyboard only on a click in it or a navigation the
    /// person started). While the Welcome card is open it keeps the keyboard instead: terminals
    /// opened or selected meanwhile (⌘N, ⌘T, a click in the top band, a script) show behind it
    /// (M11k). What closes the card clears `welcome` before handing the keyboard on.
    pub(crate) fn focus_active(&self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(card) = &self.welcome {
            card.focus_handle(cx).focus(window, cx);
            return;
        }
        if let Some(tab) = self.focused_browser_tab() {
            tab.read(cx).focus_handle(cx).focus(window, cx);
            return;
        }
        if let Some(tab) = self.focused_file_tab() {
            tab.read(cx).focus_handle(cx).focus(window, cx);
            return;
        }
        if let Some(session) = self.sessions.active() {
            session.view.focus_handle(cx).focus(window, cx);
        }
    }

    /// The focused workspace's layout, each pane showing its active tab, the unfocused panes
    /// dimmed.
    fn render_panes(
        &self,
        colors: PaneColors,
        ui: UiColors,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let active = self.model.focused_pane()?;
        let ws = self.model.focused_workspace()?;
        let tree = self.shown_layout()?;
        let find = |pane: PaneId| self.pane_session(pane).map(|s| s.view.clone());
        // Shown, so an adopted session fetches its screen now (once; reopening, M10a). Panes
        // in a split take less padding.
        let split = matches!(tree, Node::Split { .. });
        let mut resplit = Vec::new();
        for id in tree.leaves() {
            if let Some(view) = find(id) {
                view.read(cx).attach();
                if view.read(cx).set_split(split) {
                    resplit.push(id);
                }
            }
        }
        // Cached: a terminal or a file tab repaints only when it says it changed (or its bounds
        // do), not each time the workspace redraws (the marks' ticker, the sidebar). A terminal
        // whose padding just changed is drawn afresh this frame: it can't notify itself
        // mid-frame.
        let view = |id: PaneId| {
            if let Some(browser) = self.pane_browser_tab(id) {
                return Some(browser.into_any_element());
            }
            if let Some(file) = self.pane_file_tab(id) {
                return Some(
                    file.cached(StyleRefinement::default().size_full())
                        .into_any_element(),
                );
            }
            find(id).map(|view| {
                if resplit.contains(&id) {
                    view.into_any_element()
                } else {
                    view.cached(StyleRefinement::default().size_full())
                        .into_any_element()
                }
            })
        };
        let this = cx.entity().downgrade();
        let select: SelectPane = Rc::new(move |id, window, cx| {
            this.update(cx, |this, cx| this.select_pane(id, window, cx))
                .ok();
        });
        let this = cx.entity().downgrade();
        let start_drag: StartDrag = Rc::new(move |path, _, cx| {
            this.update(cx, |this, cx| this.start_drag(path, cx)).ok();
        });
        let this = cx.entity().downgrade();
        let hover: HoverDivider = Rc::new(move |path, over, _, cx| {
            this.update(cx, |this, cx| this.hover_divider(path, over, cx))
                .ok();
        });
        // Each pane's tab bar (outside the cached terminal, so a mark or a hover redraws only
        // the bar), drawn first: it needs the listeners `cx` makes.
        // The bars at the window's top edge also drag the window (not in full screen, which has
        // no title bar to move).
        let top = if window.is_fullscreen() {
            Vec::new()
        } else {
            tree.top_leaves()
        };
        let bars: RefCell<HashMap<PaneId, AnyElement>> = RefCell::new(
            tree.leaves()
                .into_iter()
                .filter_map(|id| Some((id, self.render_tab_bar(id, ui, top.contains(&id), cx)?)))
                .collect(),
        );
        self.tab_scroll
            .borrow_mut()
            .retain(|pane, _| self.model.pane(*pane).is_some());
        let bar = |id: PaneId| bars.borrow_mut().remove(&id);
        // A hover or drag belongs to the workspace it started in.
        let panes = Panes {
            active,
            view: &view,
            bar: &bar,
            select,
            start_drag,
            hover,
            hovered: held_in(&self.divider_hover, ws),
            dragging: held_in(&self.dragging, ws),
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

    /// `area` made a place to drag the window by: pressing it and moving drags the window, a
    /// double-click does what the macOS setting "Double-click a window's title bar to" says.
    /// As Zed's own title bar does, the move starts on the first mouse move after a press, so a
    /// plain click (or double-click) stays a click. Shared by the top band and the pane tab
    /// bars' empty parts, so they can't drift apart.
    pub(crate) fn drag_window_by(
        &self,
        area: Stateful<Div>,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        area.window_control_area(WindowControlArea::Drag)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _: &MouseDownEvent, _, _| this.band_pressed = true),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _: &MouseUpEvent, _, _| this.band_pressed = false),
            )
            .on_mouse_down_out(
                cx.listener(|this, _: &MouseDownEvent, _, _| this.band_pressed = false),
            )
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, _| {
                // A press whose release was missed must not make a later plain hover move the
                // window.
                if this.band_pressed && starts_window_move(event.pressed_button) {
                    this.band_pressed = false;
                    window.start_window_move();
                } else if event.pressed_button != Some(MouseButton::Left) {
                    this.band_pressed = false;
                }
            }))
            .on_click(|event: &ClickEvent, window, _| {
                if event.click_count() == 2 {
                    window.titlebar_double_click();
                }
            })
    }

    /// The window's top band, the traffic lights' place: pressing it and moving drags the
    /// window, and a double-click does what the macOS setting "Double-click a window's title bar
    /// to" says. In full screen, which has no traffic lights, it is a strip that moves nothing.
    /// `label`, if any, is drawn in the band: the open sidebar's title and sidebar button
    /// (`render_sidebar`, M11m). With the sidebar hidden the bands have none: the workspace's
    /// name and the band's buttons are drawn at the window's root over them (`title_area`).
    pub(crate) fn render_top_band(
        &self,
        id: &'static str,
        label: Option<AnyElement>,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if window.is_fullscreen() {
            return Some(
                div()
                    .id(id)
                    .relative()
                    .flex_none()
                    .w_full()
                    .h(px(TOP_BAND))
                    .children(label)
                    .into_any_element(),
            );
        }
        let band = self
            .drag_window_by(div().id(id), cx)
            .relative()
            .flex_none()
            .w_full()
            .h(px(TOP_BAND))
            .children(label);
        Some(band.into_any_element())
    }

    /// The 6 pt canvas strip above the cards (M11i): pressing it and moving drags the window,
    /// as the top band does (not in full screen, where it is only a gap).
    pub(crate) fn render_drag_strip(
        &self,
        id: &'static str,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let strip = div().id(id).flex_none().w_full().h(px(CARD_GAP));
        if window.is_fullscreen() {
            strip.into_any_element()
        } else {
            self.drag_window_by(strip, cx).into_any_element()
        }
    }

    /// ⌘\: hide or show the sidebar. Hiding it during a rename gives the keyboard back to the
    /// terminal, which ends the rename (as clicking away from it does).
    pub(crate) fn toggle_sidebar(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.sidebar.hidden = !self.sidebar.hidden;
        // With motion on, the sidebar slides; `render` runs the frames.
        // A slide already under way turns back from where it has got to.
        let current = self.slide;
        self.slide = (!cx.reduce_motion()).then(|| Slide::start(!self.sidebar.hidden, current));
        // The + menu hangs off the sidebar's footer, and a tab menu off a row; hidden, they can't
        // stay open. The 🔔's popover hangs off the footer's 🔔 or the band's (M11m), and every
        // toggle takes the one it hung from away, so it closes either way.
        self.sidebar.new_menu = false;
        let popover = self.notifications_popover.take().is_some();
        let menu = self.sidebar.tab_menu.take().is_some();
        let renaming_row = matches!(self.renaming, Some((Renaming::Workspace(_), _)));
        if self.sidebar.hidden && (renaming_row || menu) || popover {
            self.focus_active(window, cx);
        }
        self.save();
        cx.notify();
    }

    /// Whether the marks' ticker should run now (`status_mark::should_tick`). Without the
    /// macOS globals (UI screenshots), Reduce motion is off and the window counts as visible.
    fn marks_move(&self, cx: &App) -> bool {
        let animations = cx.global::<Appearance>().animations;
        let reduce = cx.try_global::<SystemReduceMotion>().is_some_and(|r| r.on);
        let seen = cx.try_global::<WindowVisible>().is_none_or(|w| w.visible);
        // The palette's workspace rows too, while it's open.
        let mut on_screen = self.visible_statuses();
        on_screen.extend(self.palette_shown(cx).into_iter().map(|(status, _)| status));
        status_mark::should_tick(animations, reduce, seen, &on_screen)
    }

    /// Start the marks' 100 ms ticker unless it's running or nothing on screen moves; it stops,
    /// without a redraw, once nothing does. `render` calls it, so everything that redraws the
    /// sidebar (a status, a group closed, the sidebar hidden, the `animations` setting, Reduce
    /// motion, the window shown again) re-checks it.
    fn tick_marks(&mut self, cx: &mut Context<Self>) {
        if self.marks_ticking || !self.marks_move(cx) {
            return;
        }
        self.marks_ticking = true;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(status_mark::TICK).await;
                let moving = this
                    .update(cx, |this, cx| {
                        this.marks_ticking = this.marks_move(cx);
                        if this.marks_ticking {
                            this.phase = status_mark::advance(this.phase);
                            cx.notify();
                        }
                        this.marks_ticking
                    })
                    .unwrap_or(false);
                if !moving {
                    break;
                }
            }
        })
        .detach();
    }
}

/// The split path of a hover or drag (`held`) if it is in workspace `ws`.
fn held_in(held: &Option<(WorkspaceId, Vec<bool>)>, ws: WorkspaceId) -> Option<&[bool]> {
    held.as_ref()
        .filter(|(w, _)| *w == ws)
        .map(|(_, path)| path.as_slice())
}

/// `groups()` as the `groups` functions take it.
fn key_refs(keys: &[Option<GroupKey>]) -> Vec<Option<&GroupKey>> {
    keys.iter().map(Option::as_ref).collect()
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // No drag: a tab or row drag GPUI ended without a drop (released elsewhere) leaves
        // nothing behind, so a later drag of something else (a file from Finder) isn't taken for
        // it (a row taken for dragged would show empty). The slides go with the drag: the layout
        // now matches its gap.
        if !cx.has_active_drag() {
            self.tab_drag.set(None);
            self.sidebar.drag_source.set(None);
            self.tab_drop = None;
            self.sidebar.drop_slot = None;
            self.drag_slides.borrow_mut().clear();
        }
        self.ensure_browser_tabs(window, cx);
        self.ensure_file_tabs(window, cx);
        // Only the file tabs on screen watch their files.
        self.show_file_tabs(cx);
        self.tick_marks(cx);
        self.refresh_ages(self.marks_ticking, cx);
        // Before the rows are drawn, so a folder found now shows in this frame; at most once a
        // second (each check builds the rows).
        if self.places.check_due(Instant::now()) {
            self.refresh_places(cx);
            self.tick_places(cx);
            self.tick_programs(cx);
        }
        // The terminal the right sidebar follows, and its folder looked up where due.
        self.refresh_right_sidebar(cx);
        self.tick_right_sidebar(cx);
        // Its listening ports, for the status bar (after the line above: it follows the same
        // terminal).
        self.refresh_ports(cx);
        self.tick_ports(cx);
        let title = self
            .model
            .focused_workspace()
            .and_then(|ws| {
                Some(window_title(
                    self.workspace_number(ws)?,
                    &self.workspace_label(ws),
                ))
            })
            .unwrap_or_else(|| "fuxx".into());
        window.set_window_title(&title);
        let theme = cx.global::<Appearance>().theme;
        let ui = theme.ui();
        let colors = PaneColors::of(&ui);
        // The slide in progress: how much of the sidebar shows. It ends on the first frame past
        // its time; until then every frame asks for the next.
        self.slide = self
            .slide
            .filter(|s| s.since.elapsed() < sidebar_toggle::SLIDE);
        let slide = self.slide.map(|s| s.shown());
        if slide.is_some() {
            window.request_animation_frame();
        }
        // The top band is the sidebar's top; with the sidebar hidden, the pane area's. Sliding,
        // the sidebar is laid over the pane column (which already has its final width, so the
        // terminals re-fit once), not beside it.
        let sidebar_node =
            (!self.sidebar.hidden || slide.is_some()).then(|| self.render_sidebar(ui, window, cx));
        let sidebar_width = self.sidebar.width;
        let (sidebar, sliding_sidebar) = match (sidebar_node, slide) {
            (Some(node), Some(shown)) => (
                None,
                Some(
                    div()
                        .absolute()
                        .top_0()
                        .left(px(-(1.0 - shown) * sidebar_width))
                        .h_full()
                        .child(node),
                ),
            ),
            (node, _) => (node, None),
        };
        // The right sidebar the same way, from the right.
        let (right_sidebar, sliding_right_sidebar) = self.right_sidebar_layout(ui, window, cx);
        let right_sliding_in = sliding_right_sidebar.is_some() && !self.right_sidebar.hidden;
        let right_width = self.right_sidebar_drawn_width(window);
        // With the sidebar shown its button is in its band (`render_sidebar`); hidden, the
        // window's band is the title bar (M11m): the workspace's name centred on the window and
        // [🔔] [⚙] [sidebar] at its far right, at the root so they sit over the panes' band and
        // the right sidebar's alike (and over a sidebar sliding out). The name waits for the
        // hide slide to end, so it never paints over the sliding sidebar's title and rows; it
        // shows at once when the sidebar has left.
        let name = if self.sidebar.hidden && slide.is_none() {
            self.render_workspace_name(ui, window)
        } else {
            None
        };
        let band_buttons = self.render_band_buttons(ui, window, cx);
        let notifications = self.render_notifications(ui, window, cx);
        let new_menu = self.render_new_menu(ui, cx);
        let welcome = self
            .welcome
            .clone()
            .map(|card| self.welcome_layer(card, ui, cx));
        let tab_menu = self.render_tab_menu(ui, cx);
        let pane_tab_menu = self.render_pane_tab_menu(ui, cx);
        let pane_actions = self.render_pane_actions(ui, cx);
        self.refresh_palette(cx);
        // Last, once every overlay of this frame is known: which pages show.
        self.update_pages(window, cx);
        // Above the cards: with the sidebar hidden, the window's top band (the name over it is
        // drawn at the root); else the 6 pt canvas strip, which drags the window too (M11i).
        let pane_band = if self.sidebar.hidden {
            self.render_top_band("pane-band", None, window, cx)
        } else {
            Some(self.render_drag_strip("pane-strip", window, cx))
        };
        let panes = self.render_panes(colors, ui, window, cx);
        let status_bar = self.render_status_bar(ui, window, cx);
        // Tabs or rows sliding out of a drag's way (the bars and rows were just drawn): every
        // frame until they're there.
        if self.drag_slides.borrow().moving(Instant::now()) {
            window.request_animation_frame();
        }

        div()
            .flex()
            .flex_col()
            .size_full()
            // The canvas the cards float on (M11i): the left sidebar's colour.
            .bg(hsla(ui.sidebar_background))
            .key_context("Workspace")
            // Esc during a tab drag cancels it, before the terminal (or a box) takes the key.
            .capture_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape" && this.cancel_tab_drag(window, cx) {
                    cx.stop_propagation();
                }
            }))
            .on_action(
                cx.listener(|this, _: &NewWorkspace, window, cx| this.new_session(window, cx)),
            )
            .on_action(cx.listener(|this, _: &NewTab, window, cx| this.new_tab(window, cx)))
            .on_action(cx.listener(|this, _: &NewSessionInFolder, window, cx| {
                this.new_session_in_folder(window, cx)
            }))
            .on_action(
                cx.listener(|this, _: &NewSshSession, window, cx| {
                    this.open_host_picker(window, cx)
                }),
            )
            .on_action(
                cx.listener(|this, _: &OpenBrowser, window, cx| this.open_address_box(window, cx)),
            )
            .on_action(cx.listener(|this, _: &CloseSession, window, cx| {
                if let Some(tab) = this.model.focused_tab() {
                    this.close_tab(tab, window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &NextSession, window, cx| {
                this.cycle_workspace(true, window, cx)
            }))
            .on_action(cx.listener(|this, _: &PrevSession, window, cx| {
                this.cycle_workspace(false, window, cx)
            }))
            .on_action(cx.listener(|this, _: &RenameSession, window, cx| {
                if let Some(ws) = this.model.focused_workspace() {
                    this.start_rename(ws, window, cx);
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
            .on_action(cx.listener(|this, _: &ToggleSidebar, w, cx| this.toggle_sidebar(w, cx)))
            .on_action(
                cx.listener(|this, _: &ToggleRightSidebar, w, cx| this.toggle_right_sidebar(w, cx)),
            )
            .on_action(
                cx.listener(|this, _: &NextNeedingYou, w, cx| this.jump_to_needing_you(w, cx)),
            )
            .on_action(
                cx.listener(|this, _: &OpenPalette, w, cx| this.open_palette(Scope::All, w, cx)),
            )
            .on_action(cx.listener(|this, _: &OpenSwitcher, w, cx| {
                this.open_palette(Scope::Workspaces, w, cx)
            }))
            .on_action(cx.listener(|this, _: &NextTab, w, cx| this.cycle_tab(true, w, cx)))
            .on_action(cx.listener(|this, _: &PrevTab, w, cx| this.cycle_tab(false, w, cx)))
            .on_action(
                cx.listener(|this, _: &SelectSession1, w, cx| this.select_shortcut(1, w, cx)),
            )
            .on_action(
                cx.listener(|this, _: &SelectSession2, w, cx| this.select_shortcut(2, w, cx)),
            )
            .on_action(
                cx.listener(|this, _: &SelectSession3, w, cx| this.select_shortcut(3, w, cx)),
            )
            .on_action(
                cx.listener(|this, _: &SelectSession4, w, cx| this.select_shortcut(4, w, cx)),
            )
            .on_action(
                cx.listener(|this, _: &SelectSession5, w, cx| this.select_shortcut(5, w, cx)),
            )
            .on_action(
                cx.listener(|this, _: &SelectSession6, w, cx| this.select_shortcut(6, w, cx)),
            )
            .on_action(
                cx.listener(|this, _: &SelectSession7, w, cx| this.select_shortcut(7, w, cx)),
            )
            .on_action(
                cx.listener(|this, _: &SelectSession8, w, cx| this.select_shortcut(8, w, cx)),
            )
            .on_action(
                cx.listener(|this, _: &SelectSession9, w, cx| this.select_shortcut(9, w, cx)),
            )
            .on_action(cx.listener(|this, _: &LaunchPreset1, w, cx| this.launch_preset(0, w, cx)))
            .on_action(cx.listener(|this, _: &LaunchPreset2, w, cx| this.launch_preset(1, w, cx)))
            .on_action(cx.listener(|this, _: &LaunchPreset3, w, cx| this.launch_preset(2, w, cx)))
            .on_action(cx.listener(|this, _: &LaunchPreset4, w, cx| this.launch_preset(3, w, cx)))
            .on_action(cx.listener(|this, _: &LaunchPreset5, w, cx| this.launch_preset(4, w, cx)))
            .on_action(cx.listener(|this, _: &LaunchPreset6, w, cx| this.launch_preset(5, w, cx)))
            .on_action(cx.listener(|this, _: &LaunchPreset7, w, cx| this.launch_preset(6, w, cx)))
            .on_action(cx.listener(|this, _: &LaunchPreset8, w, cx| this.launch_preset(7, w, cx)))
            .on_action(cx.listener(|this, _: &LaunchPreset9, w, cx| this.launch_preset(8, w, cx)))
            // The sidebars and the cards in a row, the status bar under them (M11i).
            .child(
                div()
                    .relative()
                    .flex()
                    .flex_row()
                    .flex_1()
                    .min_h(px(0.0))
                    .w_full()
                    .when_some(sidebar, |row, sidebar| row.child(sidebar))
                    // Only the focused workspace's layout is rendered; other sessions keep running (their
                    // reader threads and tickers live in their views, which the list keeps alive).
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            // Not wider than its share: a pane's tab row scrolls rather than widen it.
                            .min_w(px(0.0))
                            .h_full()
                            // Clipped here, not round the cards, so their shadows show in the gaps.
                            .overflow_hidden()
                            .when(slide.is_some() && !self.sidebar.hidden, |column| {
                                column.ml(px(sidebar_width))
                            })
                            // The right sidebar's card and the gap at its right (M11i).
                            .when(right_sliding_in, |column| {
                                column.mr(px(right_width + CARD_GAP))
                            })
                            .children(pane_band)
                            // The cards, 6 pt from the sidebar (or the window's left edge), from the
                            // right sidebar's card (or the window's right edge) and from the bottom.
                            .child(
                                div()
                                    .flex_1()
                                    .min_h(px(0.0))
                                    .w_full()
                                    .pl(px(CARD_GAP))
                                    .pr(px(CARD_GAP))
                                    .pb(px(CARD_GAP))
                                    .child(
                                        div()
                                            .relative()
                                            .size_full()
                                            .child(self.render_pane_area(cx))
                                            .children(panes),
                                    ),
                            ),
                    )
                    .children(right_sidebar)
                    .children(sliding_sidebar)
                    .children(sliding_right_sidebar),
            )
            .child(status_bar)
            // The collapsed title bar's name and buttons, over the bands (below the overlays, so
            // the Welcome scrim and the popovers stay above them).
            .children(name)
            .children(band_buttons)
            // The host picker, the palette, the Help panel and the address box: over
            // everything, centred near the top of the window.
            .when_some(self.host_picker.clone(), |root, picker| {
                root.child(top_panel(picker))
            })
            .when_some(self.palette.clone(), |root, palette| {
                root.child(top_panel(palette))
            })
            .when_some(self.help.clone(), |root, help| root.child(top_panel(help)))
            .when_some(self.address_box.clone(), |root, address| {
                root.child(top_panel(address))
            })
            // The Welcome card over its scrim, below the top band.
            .children(welcome)
            // The sidebar's + menu and a row's tab menu, over everything, with their
            // click-away backdrops.
            .children(new_menu)
            .children(tab_menu)
            .children(pane_tab_menu)
            .children(pane_actions)
            .children(notifications)
    }
}

/// Off-screen workspaces for the UI screenshots (`ui_shots`).
#[cfg(feature = "ui-shots")]
impl Workspace {
    /// A workspace that serves no scripting, runs no background service, and reopens and saves
    /// nothing.
    pub(crate) fn for_shots(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut this = Self::blank(None, false, window, cx);
        this.restoring = false;
        this.hold_saving = true;
        // The rows show the folders the scenes give (under a made-up home), and no branch
        // unless a scene sets one: nothing is looked up on this Mac.
        this.places.frozen = true;
        this.places.home = Some(PathBuf::from("/Users/me"));
        this.programs.frozen = true;
        this.ports.frozen = true;
        this.palette_files.frozen = true;
        this.agents_frozen = true;
        this
    }

    /// Open a session named `name` in `project`'s group showing `text`, behind it a `cat` that
    /// prints nothing, so the screen is the same every time. `split`: split from the active pane.
    pub(crate) fn open_shot_session(
        &mut self,
        name: &str,
        project: &Path,
        text: &str,
        split: Option<Axis>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let at = match (split, self.model.focused_pane()) {
            (Some(axis), Some(pane)) => At::Split(axis, pane),
            _ => At::New,
        };
        self.open_shot_at(Some(name), Some(project), text, at, window, cx);
    }

    /// `open_shot_session` for a terminal with no name of yours (a new one), in a workspace of
    /// its own; `project`: none for one that knows no folder (it shows the default).
    pub(crate) fn open_unnamed_shot_session(
        &mut self,
        project: Option<&Path>,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> u64 {
        self.open_shot_at(None, project, text, At::New, window, cx)
    }

    /// `open_shot_session` as a new tab at the end of the focused pane's.
    pub(crate) fn open_shot_tab(
        &mut self,
        name: &str,
        project: &Path,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let at = self.model.focused_pane().map_or(At::New, At::Tab);
        self.open_shot_at(Some(name), Some(project), text, at, window, cx);
    }

    /// Open a session at `at`, named `name` (none: automatic) in `project`'s group (none: no
    /// folder), showing `text` (see `open_shot_session`); its id.
    pub(crate) fn open_shot_at(
        &mut self,
        name: Option<&str>,
        project: Option<&Path>,
        text: &str,
        at: At,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> u64 {
        let quiet = ShellEnv {
            base: None,
            set: Vec::new(),
            remove: Vec::new(),
        };
        let view = TerminalView::spawn(cx, Some(vec!["/bin/cat".into()]), None, None, &quiet)
            .expect("start cat");
        view.read(cx).show_text(text);
        let saved = SavedSession {
            name: name.map(Into::into),
            project: project.map(Into::into),
            ..SavedSession::default()
        };
        let id = self.next_id;
        self.next_id += 1;
        self.insert_session(id, view, saved, at, window, cx);
        id
    }
}

/// A notification's title: `#<n> <who> — <message>`, `n` being the session's workspace's place
/// in the sidebar and `who` its group (else the session's name).
pub(crate) fn notification_summary(number: usize, who: &str, message: &str) -> String {
    format!("#{number} {who} — {message}")
}

/// The window's title for the focused workspace: `fuxx — #<n> <name>`.
pub(crate) fn window_title(number: usize, name: &str) -> String {
    format!("fuxx — #{number} {name}")
}

/// `panel` over the window, centred near its top.
fn top_panel(panel: impl IntoElement) -> impl IntoElement {
    div()
        .absolute()
        .top(px(48.0))
        .left_0()
        .right_0()
        .px_4()
        .flex()
        .justify_center()
        .child(panel)
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
            base: None,
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
        base: None,
        set,
        remove: Vec::new(),
    }
}

/// A session's environment: `scripting_env` on fuxx's own, or with `login` (an SSH session's,
/// M10b: your login shell's) on that, its `PATH` included. For your login shell
/// (`login_shell`: no `program`), also what `shell_integration::env_for` adds for `$SHELL` with
/// `integration`, the installed scripts' folder while `command-blocks` is on (M11l).
fn session_env(
    socket: Option<&Path>,
    id: Option<u64>,
    exe_dir: Option<&Path>,
    login: Option<&[(String, String)]>,
    login_shell: bool,
    integration: Option<&Path>,
) -> ShellEnv {
    let path = match login {
        Some(login) => login
            .iter()
            .find(|(key, _)| key == "PATH")
            .map(|(_, path)| OsString::from(path)),
        None => std::env::var_os("PATH"),
    };
    let mut env = ShellEnv {
        base: login.map(<[_]>::to_vec),
        ..scripting_env(socket, id, exe_dir, path.as_deref())
    };
    if login_shell {
        // The shell `pty::command` starts for no program; `$SHELL` unset: no integration.
        let shell = std::env::var_os("SHELL").unwrap_or_default();
        let zdotdir = std::env::var_os("ZDOTDIR");
        crate::shell_integration::add_login_shell_env(
            &mut env,
            &shell,
            zdotdir.as_deref(),
            integration,
        );
    }
    env
}

/// The installed zsh integration's folder while `command-blocks` is on and its scripts are
/// still there (`shell_integration::usable`); none otherwise (read when a terminal is
/// spawned, so the setting applies to new terminals only).
fn zsh_integration(cx: &App) -> Option<&'static Path> {
    cx.global::<Appearance>()
        .command_blocks
        .then(crate::shell_integration::installed)
        .flatten()
        .filter(|dir| crate::shell_integration::usable(dir))
}

/// An SSH session's base environment: the login shell's, once read (`ssh_program` reads it
/// before any SSH session opens). `None` for a local session, or if it couldn't be read.
fn ssh_login_env(saved: &SavedSession) -> Option<&'static [(String, String)]> {
    saved
        .ssh
        .as_ref()
        .and_then(|_| crate::workspace_ssh::login_env_read())
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

/// The panel that asks for one folder (⌘O's, and the Welcome card's Choose…).
pub(crate) fn folder_panel() -> PathPromptOptions {
    PathPromptOptions {
        files: false,
        directories: true,
        multiple: false,
        prompt: Some("Open".into()),
    }
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

/// The sessions file, if there is a valid one, without a word otherwise (the restore says it).
fn read_saved_quietly() -> Option<SavedState> {
    let text = std::fs::read_to_string(restore::state_path(&home_dir()?)).ok()?;
    restore::decode(&text)
}

/// `$HOME`; unset or empty counts as absent.
pub(crate) fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::{
        PROGRAMS_REFRESH, PROGRAMS_RETRY, Programs, daemon_create, notification_summary,
        programs_wait, scripting_env, session_env, starts_window_move, window_title,
    };
    use crate::presets::Preset;
    use crate::procs::{ProgramState, parse_ps};
    use crate::pty::apply_env;
    use crate::restore::SavedSession;
    use gpui::MouseButton;
    use portable_pty::CommandBuilder;
    use std::collections::BTreeMap;
    use std::ffi::OsStr;
    use std::path::Path;
    use std::time::{Duration, Instant};

    #[test]
    fn programs_are_read_from_the_ps_table_and_kept_steady() {
        // s1: zsh at its prompt; s2: zsh running claude (a node package); s3: zsh running make.
        let ps = "\
            100 1 100 100 Ss -zsh\n\
            200 1 200 210 Ss -zsh\n\
            210 200 210 210 S+ node /opt/homebrew/lib/node_modules/@anthropic-ai/claude-code/cli.js\n\
            300 1 300 310 Ss -zsh\n\
            310 300 310 310 S+ make all\n";
        let shells = [(1, 100), (2, 200), (3, 300), (4, 999)];
        let mut programs = Programs::default();
        assert_eq!(programs.state(1), ProgramState::Unknown, "never looked up");
        assert!(programs.take(Some(parse_ps(ps)), &shells));
        assert_eq!(programs.state(1), ProgramState::Prompt);
        assert_eq!(programs.state(2), ProgramState::Running("claude".into()));
        assert_eq!(programs.state(3), ProgramState::Running("make".into()));
        // A shell not in the table, and one never asked about.
        assert_eq!(programs.state(4), ProgramState::Unknown);
        assert_eq!(programs.state(5), ProgramState::Unknown);
        assert!(programs.table().is_some_and(|t| t.len() == 5));
        // make ends: one answer isn't enough, two are.
        let after = "100 1 100 100 Ss -zsh\n300 1 300 300 Ss -zsh\n";
        assert!(!programs.take(Some(parse_ps(after)), &shells[2..3]));
        assert_eq!(programs.state(3), ProgramState::Running("make".into()));
        assert!(programs.take(Some(parse_ps(after)), &shells[2..3]));
        assert_eq!(programs.state(3), ProgramState::Prompt);
        // Terminals not asked about are kept as they were, never forgotten for it (the caller
        // asks about every local terminal, `ps -A` being system-wide): s2 still runs claude.
        assert_eq!(
            programs.state(2),
            ProgramState::Running("claude".into()),
            "s2 not asked about: kept"
        );
        assert_eq!(programs.state(1), ProgramState::Prompt, "s1 too");
        // Each one answers from the table, its name on screen or not: claude ends in s2 (the
        // prompt after an agent is taken at once).
        let s2_done = "100 1 100 100 Ss -zsh\n200 1 200 200 Ss -zsh\n300 1 300 300 Ss -zsh\n";
        assert!(programs.take(Some(parse_ps(s2_done)), &shells));
        assert_eq!(programs.state(2), ProgramState::Prompt);
        assert_eq!(programs.state(3), ProgramState::Prompt);
        // ps failing: the last good names are kept (not wiped to their folders); the table
        // goes (the ports lookup waits for a good one).
        assert!(!programs.take(None, &shells), "nothing shown changed");
        assert_eq!(programs.state(1), ProgramState::Prompt);
        assert_eq!(programs.state(2), ProgramState::Prompt);
        assert!(programs.table().is_none());
        // Gone terminals are forgotten.
        programs.take(Some(parse_ps(ps)), &shells);
        programs.retain(|id| id == 1);
        assert_eq!(programs.state(2), ProgramState::Unknown);
        assert_eq!(programs.state(1), ProgramState::Prompt);
    }

    #[test]
    fn ps_runs_at_most_every_3_s() {
        let now = Instant::now();
        let mut programs = Programs::default();
        assert_eq!(programs.wait(now), Duration::ZERO, "never run: at once");
        programs.last_run = Some(now);
        assert_eq!(programs.wait(now), PROGRAMS_REFRESH);
        assert_eq!(
            programs.wait(now + Duration::from_secs(1)),
            Duration::from_secs(2)
        );
        assert_eq!(programs.wait(now + Duration::from_secs(5)), Duration::ZERO);
        assert_eq!(PROGRAMS_REFRESH, Duration::from_secs(3));
    }

    #[test]
    fn a_failed_ps_waits_30_s_and_a_good_one_3_s() {
        assert_eq!(programs_wait(true), PROGRAMS_REFRESH);
        assert_eq!(programs_wait(false), Duration::from_secs(30));
        assert_eq!(programs_wait(false), PROGRAMS_RETRY);
        let now = Instant::now();
        let s = Duration::from_secs;
        let mut programs = Programs {
            last_run: Some(now),
            ..Default::default()
        };
        // It fails (or runs past its cap): the next try 30 s after it began.
        programs.take(None, &[(1, 100)]);
        assert_eq!(programs.wait(now), s(30));
        assert_eq!(programs.wait(now + s(10)), s(20));
        assert_eq!(programs.wait(now + s(31)), Duration::ZERO);
        // It works again: back to every 3 s.
        programs.last_run = Some(now + s(31));
        programs.take(Some(parse_ps("100 1 100 100 Ss -zsh\n")), &[(1, 100)]);
        assert_eq!(programs.wait(now + s(31)), PROGRAMS_REFRESH);
    }

    #[test]
    fn only_a_held_left_button_starts_a_window_move() {
        assert!(starts_window_move(Some(MouseButton::Left)));
        assert!(!starts_window_move(None));
        assert!(!starts_window_move(Some(MouseButton::Right)));
    }

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
    fn an_ssh_session_starts_from_the_login_environment() {
        let ours = Path::new("/me/fuxx.sock");
        let exe = Path::new("/apps/fuxx.app/Contents/MacOS");
        let login = [
            ("PATH".to_string(), "/opt/homebrew/bin:/usr/bin".to_string()),
            ("SSH_AUTH_SOCK".to_string(), "/agent".to_string()),
        ];
        let env = session_env(Some(ours), Some(4), Some(exe), Some(&login), false, None);
        assert_eq!(env.base.as_deref(), Some(&login[..]));
        let var = |k: &str| {
            env.set
                .iter()
                .find(|(key, _)| key == k)
                .map(|(_, v)| v.as_str())
        };
        // The login PATH, with fuxx's folder at the end.
        assert_eq!(
            var("PATH"),
            Some("/opt/homebrew/bin:/usr/bin:/apps/fuxx.app/Contents/MacOS")
        );
        assert_eq!(var("FUXX_SESSION"), Some("s4"));
        // Without it (a local session, or none read): fuxx's own, as before.
        let own = session_env(Some(ours), Some(4), Some(exe), None, false, None);
        assert!(own.base.is_none());
        let path = std::env::var("PATH").unwrap_or_default();
        let expected = if path.is_empty() {
            exe.display().to_string()
        } else {
            format!("{path}:{}", exe.display())
        };
        assert_eq!(
            own.set.iter().find(|(k, _)| k == "PATH").map(|(_, v)| v),
            Some(&expected)
        );
    }

    #[test]
    fn session_env_injects_only_a_login_shell() {
        let ours = Path::new("/me/fuxx.sock");
        let i = Some(Path::new("/i"));
        let keys = |env: &crate::pty::ShellEnv| -> Vec<String> {
            env.set.iter().map(|(k, _)| k.clone()).collect()
        };
        // Not a login shell (SSH's `ssh`, a program): nothing added.
        let program = session_env(Some(ours), Some(4), None, None, false, i);
        assert!(!keys(&program).iter().any(|k| k.contains("ZDOTDIR")));
        // No integration (off, or not installed): nothing added either.
        let off = session_env(Some(ours), Some(4), None, None, true, None);
        assert_eq!(off.set, program.set);
        // A login shell: what `env_for` gives for `$SHELL`, after the rest; nothing removed.
        let login = session_env(Some(ours), Some(4), None, None, true, i);
        let shell = std::env::var_os("SHELL").unwrap_or_default();
        let zdotdir = std::env::var_os("ZDOTDIR");
        let added =
            crate::shell_integration::env_for(Path::new(&shell), zdotdir.as_deref(), true, i);
        assert_eq!(login.set[..program.set.len()], program.set[..]);
        assert_eq!(login.set[program.set.len()..], added[..]);
        // Only a stray saved ZDOTDIR is removed, and only when you have none.
        let mut removed = program.remove.clone();
        if !added.is_empty() && zdotdir.is_none() {
            removed.push("FUXX_ZSH_ZDOTDIR");
        }
        assert_eq!(login.remove, removed);
        if Path::new(&shell).file_name() == Some(OsStr::new("zsh")) {
            assert!(keys(&login).contains(&"ZDOTDIR".to_string()));
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
            color: None,
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
    fn notification_and_title_text() {
        assert_eq!(
            notification_summary(3, "api", "Needs your input"),
            "#3 api \u{2014} Needs your input"
        );
        assert_eq!(
            notification_summary(12, "\u{1f595}", "Finished"),
            "#12 \u{1f595} \u{2014} Finished"
        );
        assert_eq!(window_title(3, "claude"), "fuxx \u{2014} #3 claude");
    }
}
