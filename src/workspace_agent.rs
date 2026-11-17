//! Agent tabs in the workspace (M11aa Task 8): the views (`AgentTab`) and the Workspace's mirror
//! of each (`AgentSlot`, R1), launching a provider preset as a shell and an agent tab side by
//! side (R6), the pane `···` menu's New ⟨Label⟩ Tab, closing (asking when busy), Rename, and
//! "Open in classic view" (R13: the tab's own process pauses; it comes back with `--resume`
//! only when you next send from it, and never while the classic terminal still runs it).
//!
//! The chrome rules it keeps (M11p, the plan's Global Constraints):
//! - `ensure_agent_tabs` runs in `render` only as `ensure_file_tabs` does: it makes views and
//!   inserts slots; a new slot is Idle, which no part draws differently.
//! - What the chrome shows of an agent tab (its title, badge, status) is the model's or the
//!   slot's; the slot is written only in `on_agent_event` (outside drawing), which notifies.
//!
//! Task 9: an agent tab's status reaches the chrome as a terminal's does (`set_agent_status`:
//! its tab's mark, its row, ages, the 🔔 list, sound and banner, Activity); a row is about the
//! most urgent of its terminals and agent tabs (`row_winner`, R3).
//!
//! `launch_plan_for`, `agent_row_label`, `after_classic` and `row_winner` are pure and tested.

use std::path::PathBuf;
use std::time::{Instant, SystemTime};

use gpui::{
    App, Context, Div, Entity, FontWeight, PromptLevel, SharedString, Window, div, prelude::*, px,
};

use crate::activity::ActivityLog;
use crate::agent_tab::{AgentTab, AgentTabEvent};
use crate::api_events::Event;
use crate::block_view::{self, BlockDrag};
use crate::groups;
use crate::layout_model::{PaneId, TabId, TabKind, WorkspaceId};
use crate::notification_list;
use crate::notifier;
use crate::presets::{self, Preset};
use crate::procs::{self, Foreground, Proc};
use crate::providers::{self, Provider, Start};
use crate::restore::{Launch, SavedSession};
use crate::split_tree::Axis;
use crate::status::Status;
use crate::terminal_view::hsla;
use crate::ui_tokens::text;
use crate::workspace::{At, Opened, Workspace};

/// Past `TERM_GRACE`, how much longer "Open in classic view" waits for the tab's process to be
/// reaped before it gives up (the classic view doesn't open: never two writers).
const REAP_WAIT: std::time::Duration = std::time::Duration::from_secs(1);

/// Where a send from a tab whose classic view is opening says the conversation runs.
const OPENING: &str = "its classic view, which is opening";

/// What "Open in classic view" says when it is ignored because a send's holder check runs.
pub const CHECKING_NOTICE: &str = "Checking the conversation — try again in a moment.";

/// An unknown provider's badge and its colour (a restored tab whose agent fuxx no longer has).
const UNKNOWN_BADGE: &str = "?";
const UNKNOWN_RGB: u32 = 0x6b6b6b;

/// The Workspace's mirror of one agent tab (R1): the view, and what the chrome shows of it.
/// Chrome code reads this (and the model), never the view.
pub(crate) struct AgentSlot {
    pub view: Entity<AgentTab>,
    /// `aN`, from `Workspace::next_agent_id` (starts at 1).
    pub id: u64,
    /// The provider's label; an unknown provider: its name.
    pub label: SharedString,
    pub badge: &'static str,
    pub badge_rgb: u32,
    /// Its provider is known: it takes a dropped command block (an error tab has nothing to
    /// send one to).
    pub attachable: bool,
    /// Its status as the tab last told it (`set_agent_status`), and since when.
    pub status: Status,
    pub since: Instant,
    /// The Done pop on its tab's mark: its number and when (as a session's).
    pub pop: Option<(u64, Instant)>,
    /// While a permission waits: "⟨Label⟩ wants to run: ⟨what⟩" (its row's line, its banner).
    pub question: Option<String>,
    /// Its status changes, for the Activity panel while it is focused.
    pub activity: ActivityLog,
}

/// What a workspace's sidebar row is about (R3): its terminal rollup's session (its place in the
/// session list), or one of its agent tabs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowWinner {
    Terminal(usize),
    Agent(TabId),
}

/// The most urgent of a workspace's terminal rollup and its agent tabs (`groups::urgency`); a tie
/// goes to the terminal, then to the first agent tab in reading order. None: neither.
pub fn row_winner(
    terminal: Option<(usize, Status)>,
    agents: &[(TabId, Status)],
) -> Option<RowWinner> {
    let mut best = terminal.map(|(i, status)| (RowWinner::Terminal(i), status));
    for &(tab, status) in agents {
        if best.is_none_or(|(_, b)| groups::urgency(status) > groups::urgency(b)) {
            best = Some((RowWinner::Agent(tab), status));
        }
    }
    best.map(|(winner, _)| winner)
}

/// How a preset is launched.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LaunchPlan {
    /// A terminal running the preset's command, as before M11aa.
    Classic,
    /// A plain shell, and the agent tab of this provider (its name) beside it (spec §1.4).
    ShellThenAgent(&'static str),
}

/// `ShellThenAgent` iff fuxx has a provider by the preset's name.
pub fn launch_plan_for(preset: &Preset) -> LaunchPlan {
    match presets::provider_for(preset) {
        Some(provider) => LaunchPlan::ShellThenAgent(provider.info().name),
        None => LaunchPlan::Classic,
    }
}

/// R11: the label a workspace's row takes from its first agent tab (`first_agent`, the
/// provider's label) when its named terminal has no name of its own (`terminal_named`: a name
/// you gave it, or a preset's).
pub fn agent_row_label(first_agent: Option<&str>, terminal_named: bool) -> Option<&str> {
    first_agent.filter(|_| !terminal_named)
}

/// What sending from an agent tab does after "Open in classic view" (R13).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AfterClassic {
    Send,
    Respawn,
    Warn,
}

/// What sending from an agent tab does: not paused → Send; paused and the classic terminal still
/// runs the agent → Warn (a Notice "⟨Label⟩ is still running this conversation in ⟨terminal⟩ —
/// quit it there first", the text stays in the composer); paused and it doesn't → Respawn with
/// `--resume`, then send.
pub fn after_classic(paused: bool, classic_running: bool) -> AfterClassic {
    match (paused, classic_running) {
        (false, _) => AfterClassic::Send,
        (true, true) => AfterClassic::Warn,
        (true, false) => AfterClassic::Respawn,
    }
}

/// Who else has a conversation open, as one `ps` table shows it (R13).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Holder {
    /// No other process runs it: the tab may start its own.
    Free,
    /// Another process runs it (or the classic terminal is still starting it).
    Held,
    /// `ps` failed: unknown, so the tab never starts a second writer on it.
    Unchecked,
}

/// How long after "Open in classic view" its terminal counts as running the agent whatever
/// `ps` says (the provider's typed resume command may not have started yet).
pub const CLASSIC_START: std::time::Duration = std::time::Duration::from_secs(5);

/// The tab's classic view, as the holder check knows it (R13).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Classic {
    /// None open (or its terminal has gone).
    Closed,
    /// Opening, or opened less than `CLASSIC_START` ago: its agent may not run yet, so it
    /// holds the conversation whatever `ps` says.
    Starting,
    /// Open in a terminal of this Mac whose shell (this pid) fuxx knows: it holds the
    /// conversation while that program is in front of the shell (whatever its arguments).
    Shell(u32),
    /// Open in a terminal whose shell fuxx can't see (a background-service one). What runs
    /// there may not show the conversation in its arguments (typed without it, or hidden: see
    /// the provider's `conversation_in_args`), so any live process of that program may be it,
    /// except one naming another conversation and fuxx's own agents (the groups they lead).
    Blind(Vec<u32>),
}

/// Whether conversation `id` of `provider` is held by another process in `table` (`None`: `ps`
/// failed → `Unchecked`): a live process of that program with the id among its arguments
/// (`classic_argv` always puts it there; or as the provider reads its arguments,
/// `conversation_in_args`, e.g. a flag's `=` form), not of the tab's own process groups (`own`: the
/// groups of the processes it started, each its own group's leader); or the tab's classic view
/// (`classic`) may run it.
pub fn conversation_holder(
    table: Option<&[Proc]>,
    provider: &dyn Provider,
    id: &str,
    own: &[u32],
    classic: &Classic,
) -> Holder {
    let Some(table) = table else {
        return Holder::Unchecked;
    };
    let not_in = |p: &Proc, groups: &[u32]| !groups.contains(&p.pgid) && !groups.contains(&p.pid);
    let program = provider.program();
    let runs_it =
        |p: &&Proc| !p.is_zombie() && not_in(p, own) && procs::program_name(&p.args) == program;
    let with_id = table.iter().filter(runs_it).any(|p| {
        p.args.split_whitespace().any(|word| word == id)
            || provider.conversation_in_args(&p.args).as_deref() == Some(id)
    });
    let in_classic = match classic {
        Classic::Closed => false,
        Classic::Starting => true,
        Classic::Shell(shell) => matches!(procs::foreground(table, *shell),
            Foreground::Program(args) if procs::program_name(&args) == program),
        Classic::Blind(agents) => table.iter().filter(runs_it).any(|p| {
            not_in(p, agents)
                && provider
                    .conversation_in_args(&p.args)
                    .is_none_or(|other| other == id)
        }),
    };
    if with_id || in_classic {
        Holder::Held
    } else {
        Holder::Free
    }
}

/// A terminal of this run, as the classic check sees it: the background-service session it is
/// (None: a local one) and its shell's pid, when fuxx knows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SeenTerminal {
    pub service: Option<u64>,
    pub shell: Option<u32>,
}

/// The tab's classic view as the holder check counts on it (R13), and the terminal of this run
/// that shows it (to name in a notice, to show again). `entry`: its `classic_of` note (the
/// terminal's id, and whether it opened moments ago); `saved`: the background-service session
/// saved with the tab; `seen`: a terminal of this run by id; `agents`: fuxx's own agents.
/// - Its terminal of this run opened moments ago → `Starting`.
/// - A saved session this run doesn't see as that service session (not in its list: the service
///   couldn't be reached, or adopting it failed; or only a local terminal of the same number)
///   → `Blind`, naming none: it may still run there.
/// - Else its terminal of this run (a saved id names only the service's session): its shell →
///   `Shell`; no shell fuxx knows → `Blind`; none (closed) → `Closed`.
pub fn classic_state(
    entry: Option<(u64, bool)>,
    saved: Option<u64>,
    seen: impl Fn(u64) -> Option<SeenTerminal>,
    agents: Vec<u32>,
) -> (Classic, Option<u64>) {
    // This run's terminal `id`, never a local one that only shares the saved session's number.
    let found = |id: u64| seen(id).filter(|t| saved != Some(id) || t.service == Some(id));
    if let Some((id, true)) = entry
        && found(id).is_some()
    {
        return (Classic::Starting, Some(id));
    }
    if let Some(session) = saved
        && found(session).is_none()
    {
        return (Classic::Blind(agents), None);
    }
    let Some((id, terminal)) = entry
        .map(|(id, _)| id)
        .or(saved)
        .and_then(|id| Some((id, found(id)?)))
    else {
        return (Classic::Closed, None);
    };
    let classic = match terminal.shell {
        Some(shell) => Classic::Shell(shell),
        None => Classic::Blind(agents),
    };
    (classic, Some(id))
}

/// How a restored agent tab learns, off the UI thread, which classic view its first check
/// counts on (`restored_classic`, asking the background service).
pub type ClassicProbe = Box<dyn FnOnce() -> Classic + Send>;

/// The classic view a restored agent tab's first check counts on (R13): its classic view's
/// background-service session (`saved`), while the service still has it running (`alive`: the
/// service's live session ids; None: it couldn't be asked, so it may) → `Blind` (fuxx's own
/// agents: `agents`); none saved, or that session gone → `Closed`.
pub fn restored_classic(saved: Option<u64>, alive: Option<&[u64]>, agents: Vec<u32>) -> Classic {
    match (saved, alive) {
        (None, _) => Classic::Closed,
        (Some(session), Some(alive)) if !alive.contains(&session) => Classic::Closed,
        (Some(_), _) => Classic::Blind(agents),
    }
}

/// Whether a send's holder check, answered after a wait, is stale: since it began, the tab's
/// classic view started opening (`opening`), or one opened (`before` → `now`, its
/// `classic_of` entry). Stale, it counts as held (its `ps` and its classic view are old).
pub fn classic_changed(
    opening: bool,
    before: Option<(u64, Instant)>,
    now: Option<(u64, Instant)>,
) -> bool {
    opening || before != now
}

/// What "Open in classic view" does after a look at the tab's own processes: open it once all
/// are reaped; keep waiting while one runs; past `deadline` with one still running, don't open
/// (the tab stays paused and says so): never two writers of one conversation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReapWait {
    Open,
    Wait,
    Refuse,
}

/// `ReapWait` from whether every process of the tab is reaped and whether the deadline passed.
pub fn reap_wait(reaped: bool, deadline_passed: bool) -> ReapWait {
    match (reaped, deadline_passed) {
        (true, _) => ReapWait::Open,
        (false, false) => ReapWait::Wait,
        (false, true) => ReapWait::Refuse,
    }
}

/// Whether a resumed tab starts paused instead of starting its process (it is held elsewhere,
/// or that couldn't be checked).
pub fn starts_paused(resuming: bool, holder: &Holder) -> bool {
    resuming && *holder != Holder::Free
}

/// The notice a paused tab shows when it won't start (`terminal`: the classic view's name).
pub fn held_notice(label: &str, holder: &Holder, terminal: Option<&str>) -> String {
    match (holder, terminal) {
        (Holder::Unchecked, _) => format!(
            "Couldn't check whether {label} still runs this conversation elsewhere — send again to retry"
        ),
        (_, Some(terminal)) => format!(
            "{label} is still running this conversation in {terminal} — quit it there first"
        ),
        (_, None) => format!(
            "{label} is still running this conversation in another terminal — quit it there first"
        ),
    }
}

/// The notice of a tab whose own process didn't stop in time for its classic view, which
/// didn't open then (it stays paused).
pub fn not_stopped_notice(label: &str) -> String {
    format!("{label} didn't stop in time, so the classic view didn't open — try again")
}

/// A provider's badge: `size` square, rounded, `rgb` filled, its letters in white.
pub(crate) fn agent_badge(badge: &'static str, rgb: u32, size: f32) -> Div {
    div()
        .flex_none()
        .size(px(size))
        .rounded(px(4.0))
        .bg(hsla(alacritty_terminal::vte::ansi::Rgb {
            r: (rgb >> 16) as u8,
            g: (rgb >> 8) as u8,
            b: rgb as u8,
        }))
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(text::META))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(gpui::white())
        .child(badge)
}

/// A provider's label, badge and colour by its name (an unknown one: its name, "?", grey).
pub(crate) fn provider_look(name: &str) -> (SharedString, &'static str, u32) {
    match providers::by_name(name) {
        Some(p) => {
            let info = p.info();
            (info.label.into(), info.badge, info.badge_rgb)
        }
        None => (name.to_string().into(), UNKNOWN_BADGE, UNKNOWN_RGB),
    }
}

impl Workspace {
    /// Every agent tab of the model has its view and slot (a restored one, one just split in).
    /// A new view for the focused tab takes the keyboard. Render-safe (as `ensure_file_tabs`):
    /// it only makes views and inserts slots; a new slot is Idle.
    pub(crate) fn ensure_agent_tabs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let missing: Vec<(TabId, TabKind)> = self
            .model
            .workspaces()
            .iter()
            .flat_map(|w| &w.panes)
            .flat_map(|p| &p.tabs)
            .filter(|t| !self.agents.contains_key(&t.id))
            .filter(|t| matches!(t.kind, TabKind::Agent { .. }))
            .map(|t| (t.id, t.kind.clone()))
            .collect();
        let focused = self.model.focused_tab();
        let mut focus_it = false;
        for (tab, kind) in missing {
            self.make_agent_tab(tab, &kind, window, cx);
            focus_it |= Some(tab) == focused;
        }
        if focus_it {
            self.focus_active(window, cx);
        }
    }

    /// The view and slot for agent tab `tab` (of `kind`), its events heard by `on_agent_event`.
    fn make_agent_tab(
        &mut self,
        tab: TabId,
        kind: &TabKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let TabKind::Agent { provider: name, .. } = kind else {
            return;
        };
        let provider = providers::by_name(name);
        let start = self.agent_start(kind, cx).unwrap_or_else(|| Start {
            cwd: PathBuf::from("/"),
            resume: None,
            mode: None,
            extra_args: Vec::new(),
        });
        let name = name.clone();
        #[cfg(feature = "ui-shots")]
        let offline = self.agents_offline;
        #[cfg(not(feature = "ui-shots"))]
        let offline = false;
        // Its classic view in a background-service terminal, from before a restart (R13): its
        // checks count on that terminal (its first one, at boot, asks the service whether it
        // still runs).
        let saved = self.model.agent_classic(tab);
        if let Some(session) = saved {
            let opened = Instant::now()
                .checked_sub(CLASSIC_START)
                .unwrap_or_else(Instant::now);
            self.classic_of.entry(tab).or_insert((session, opened));
        }
        let client = self.service.client();
        let classic: ClassicProbe = Box::new(move || {
            let alive = saved.and(client).and_then(|c| c.list().ok()).map(|list| {
                list.iter()
                    .filter(|s| s.alive)
                    .map(|s| s.id)
                    .collect::<Vec<_>>()
            });
            restored_classic(
                saved,
                alive.as_deref(),
                crate::agent_process::started_pids(),
            )
        });
        let view = cx.new(|cx| match provider {
            // The scenes never start an agent: the tab is fed decoded events (R12).
            #[cfg(feature = "ui-shots")]
            Some(provider) if offline => AgentTab::for_shots(provider, start, window, cx),
            _ => {
                let _ = offline;
                AgentTab::new(provider, name.clone(), start, classic, window, cx)
            }
        });
        cx.subscribe_in(&view, window, move |this, _, event, window, cx| {
            this.on_agent_event(tab, event, window, cx)
        })
        .detach();
        let (label, badge, badge_rgb) = provider_look(&name);
        let id = self.next_agent_id;
        self.next_agent_id += 1;
        self.agents.insert(
            tab,
            AgentSlot {
                view,
                id,
                label,
                badge,
                badge_rgb,
                attachable: provider.is_some(),
                status: Status::Idle,
                since: Instant::now(),
                pop: None,
                question: None,
                activity: ActivityLog::default(),
            },
        );
    }

    /// How agent tab `kind` starts: in its folder, resuming its conversation, in its mode, with
    /// the extra arguments of today's preset of its provider's name (none without one).
    pub(crate) fn agent_start(&self, kind: &TabKind, cx: &App) -> Option<Start> {
        let TabKind::Agent {
            provider,
            cwd,
            conversation,
            mode,
            ..
        } = kind
        else {
            return None;
        };
        let extra_args = cx
            .global::<crate::appearance::Appearance>()
            .presets
            .iter()
            .find(|p| p.name == *provider)
            .map(presets::extra_args)
            .unwrap_or_default();
        Some(Start {
            cwd: cwd.clone(),
            resume: conversation.clone(),
            mode: mode.clone(),
            extra_args,
        })
    }

    /// The agent tab pane `pane` shows, if it shows one.
    pub(crate) fn pane_agent_tab(&self, pane: PaneId) -> Option<Entity<AgentTab>> {
        self.agents
            .get(&self.model.pane(pane)?.active)
            .map(|slot| slot.view.clone())
    }

    /// The agent tab the focused pane shows, if it shows one.
    pub(crate) fn focused_agent_tab(&self) -> Option<(TabId, Entity<AgentTab>)> {
        let tab = self.model.focused_tab()?;
        Some((tab, self.agents.get(&tab)?.view.clone()))
    }

    /// A new agent tab of `provider` at the end of `pane`'s bar (its ⋯ menu, ⌘K), in the folder
    /// of the terminal the pane shows (else the active one's, else your home). Its view is made
    /// on the next frame (`ensure_agent_tabs`), and takes the keyboard there.
    pub(crate) fn new_agent_tab(
        &mut self,
        pane: PaneId,
        provider: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let _ = window;
        let project = self
            .pane_session(pane)
            .and_then(|s| s.project.clone())
            .or_else(|| self.active_project());
        let kind = self.new_agent_kind(provider, project);
        if self.model.add_tab(pane, kind).is_none() {
            return;
        }
        self.sync();
        self.save();
        cx.notify();
    }

    /// The agent tab beside terminal `sN` (`terminal`), split to its right (a provider preset's
    /// launch, R6), in the terminal's folder.
    pub(crate) fn agent_beside(
        &mut self,
        terminal: u64,
        provider: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let _ = window;
        let Some((_, pane)) = self
            .model
            .terminal_tab(terminal)
            .and_then(|tab| self.model.locate(tab))
        else {
            return;
        };
        let project = self
            .sessions
            .items()
            .iter()
            .find(|s| s.id == terminal)
            .and_then(|s| s.project.clone());
        let kind = self.new_agent_kind(provider, project);
        if self.model.split(pane, Axis::Row, kind).is_none() {
            return;
        }
        self.sync();
        self.save();
        cx.notify();
    }

    /// A fresh agent tab of `provider` in `project` (none: your home), in its default mode.
    fn new_agent_kind(&self, provider: &'static str, project: Option<PathBuf>) -> TabKind {
        let cwd = project
            .or_else(|| self.places.home.clone())
            .or_else(crate::workspace::home_dir)
            .unwrap_or_else(|| PathBuf::from("/"));
        let mode = providers::by_name(provider)
            .and_then(|p| p.info().default_mode)
            .map(str::to_string);
        TabKind::Agent {
            provider: provider.to_string(),
            cwd,
            conversation: None,
            mode,
            title: None,
        }
    }

    /// Agent tab `tab`'s title: yours or its first message's, else its provider's label.
    pub(crate) fn agent_title(&self, tab: TabId) -> SharedString {
        match self.model.tab(tab).map(|t| &t.kind) {
            Some(TabKind::Agent {
                title: Some(title), ..
            }) => title.clone().into(),
            Some(TabKind::Agent { provider, .. }) => provider_look(provider).0,
            _ => SharedString::default(),
        }
    }

    /// Whether agent tab `tab` is in a turn or waits for you (closing it asks first). Read from
    /// the view: a decision, not something the chrome draws.
    pub(crate) fn agent_busy(&self, tab: TabId, cx: &App) -> bool {
        self.agents.get(&tab).is_some_and(|slot| {
            matches!(
                slot.view.read(cx).status(),
                Status::Working | Status::Waiting
            )
        })
    }

    /// Close agent tab `tab` (its ×, ⌘W, its menu, a workspace closing). `ask`: while it works
    /// or waits for you, a native sheet asks first (Cancel keeps it). Closing ends its process
    /// (no respawn), then it goes like a browser tab: with nothing left, the window.
    pub(crate) fn close_agent_tab(
        &mut self,
        tab: TabId,
        ask: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if ask && self.agent_busy(tab, cx) {
            let title = self.agent_title(tab);
            let label = self.agents[&tab].label.clone();
            let waiting = self.agents[&tab].view.read(cx).status() == Status::Waiting;
            let detail = if waiting {
                format!("{label} is waiting for you.")
            } else {
                format!("{label} is still working.")
            };
            let answer = window.prompt(
                PromptLevel::Warning,
                &format!("Close {title}?"),
                Some(&detail),
                &["Close", "Cancel"],
                cx,
            );
            cx.spawn_in(window, async move |this, cx| {
                if answer.await != Ok(0) {
                    return;
                }
                this.update_in(cx, |this, window, cx| {
                    this.close_agent_tab(tab, false, window, cx)
                })
                .ok();
            })
            .detach();
            return;
        }
        if let Some(slot) = self.agents.get(&tab) {
            slot.view.update(cx, |view, _| view.end());
        }
        if self.model.close_tab(tab).is_none() {
            return;
        }
        if let Some(menu) = &self.pane_tab_menu
            && menu.tab == tab
        {
            self.pane_tab_menu = None;
        }
        self.sync();
        self.save();
        // Its waits end now (`sync` noted it closed), their replies before a quit.
        let mut conns = self.check_waits(&[], cx);
        if self.all_gone() {
            conns.extend(self.subscribers.iter().cloned());
            self.quit_after_flush(conns, None, window, cx);
            return;
        }
        self.activate(window, cx);
    }

    /// What agent tab `tab` told the Workspace. Each change the chrome shows is saved into the
    /// model (or the slot) here, outside drawing, and notifies.
    fn on_agent_event(
        &mut self,
        tab: TabId,
        event: &AgentTabEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            AgentTabEvent::Title(title) => {
                if let Some(TabKind::Agent {
                    provider,
                    cwd,
                    conversation,
                    mode,
                    title: None,
                }) = self.model.tab(tab).map(|t| t.kind.clone())
                {
                    let kind = TabKind::Agent {
                        provider,
                        cwd,
                        conversation,
                        mode,
                        title: Some(title.clone()),
                    };
                    self.model.set_kind(tab, kind);
                    self.save();
                    cx.notify();
                }
            }
            AgentTabEvent::Conversation(id) => {
                if let Some(TabKind::Agent {
                    provider,
                    cwd,
                    mode,
                    title,
                    ..
                }) = self.model.tab(tab).map(|t| t.kind.clone())
                {
                    // The mode its process runs in (chosen on the chip before the first
                    // message, call 6) is saved with the conversation it runs.
                    let running = self
                        .agents
                        .get(&tab)
                        .and_then(|slot| slot.view.read(cx).mode().map(str::to_string));
                    let kind = TabKind::Agent {
                        provider,
                        cwd,
                        conversation: Some(id.clone()),
                        mode: running.or(mode),
                        title,
                    };
                    self.model.set_kind(tab, kind);
                    self.save();
                }
            }
            AgentTabEvent::Close => self.close_agent_tab(tab, false, window, cx),
            AgentTabEvent::OpenClassic(_) => self.open_classic(tab, window, cx),
            AgentTabEvent::ResumeAsked(text) => {
                self.check_holder_then_resume(tab, text.clone(), window, cx)
            }
            // Before a `Status` (which notifies); a new question while it already waits (a
            // second permission) has no `Status` after it, so it notifies itself.
            AgentTabEvent::Question(question) => {
                if let Some(slot) = self.agents.get_mut(&tab) {
                    slot.question = question.clone();
                    if slot.status == Status::Waiting {
                        cx.notify();
                    }
                }
            }
            AgentTabEvent::Status { status, live } => {
                self.set_agent_status(tab, *status, *live, cx)
            }
            // Spec §1.5: an error or a non-zero exit is an Activity note too.
            AgentTabEvent::Failed(why) => {
                if let Some(slot) = self.agents.get_mut(&tab) {
                    slot.activity.happened(why, SystemTime::now());
                    cx.notify();
                }
            }
            AgentTabEvent::BlockDropped(drag) => self.attach_block(tab, drag, cx),
        }
    }

    /// Command block `drag` dropped on agent tab `tab` (its body or its tab in a bar, spec
    /// §1.7): the block's output is read off the UI thread (the Term lock held only for the
    /// read, as Copy output does), made into "command + exit code + output" text
    /// (`block_view::attachment_text`, with the block's outcome when dropped), and attached to the tab's next message, shown as a chip
    /// under its composer. It only waits there: nothing is sent, and a paused tab stays paused
    /// (R13: only your send resumes it, through the holder check). Nothing when the block has
    /// left the grid by then, or the tab is gone.
    pub(crate) fn attach_block(&mut self, tab: TabId, drag: &BlockDrag, cx: &mut Context<Self>) {
        let Some(view) = self.agents.get(&tab).map(|slot| slot.view.downgrade()) else {
            return;
        };
        let BlockDrag {
            id, command, core, ..
        } = drag.clone();
        // Its outcome as it is now (a block that finished during the drag has its exit code);
        // the command is the one the chip showed.
        let text = cx.background_spawn(async move {
            let (output, outcome) = core.block_output_and_outcome(&id)?;
            Some(block_view::attachment_text(
                command.as_deref(),
                outcome,
                &output,
            ))
        });
        cx.spawn(async move |_, cx| {
            if let Some(text) = text.await {
                let _ = view.update(cx, |t, cx| t.attach(text, cx));
            }
        })
        .detach();
    }

    /// For agent tab `tab` what `set_status` does for a terminal: its status and since when;
    /// Done pops once, as it arrives, if its row is on screen; an Activity entry; and a
    /// notification by the M4 rules (not while you look at it): a 🔔 list entry (about no
    /// terminal) and, unless a short run, the sound and a banner (call 3: no click target,
    /// `banner_about(None)`; its subtitle names the provider, its body says the question or
    /// what the tab was doing, its title). `live: false` (history replayed, the pause before
    /// "Open in classic view") is not news: no pop, no Activity entry, no notification.
    pub(crate) fn set_agent_status(
        &mut self,
        tab: TabId,
        new: Status,
        live: bool,
        cx: &mut Context<Self>,
    ) {
        let looking = self.model.focused_tab() == Some(tab);
        let window_active = self.window_active;
        let ws = self.model.locate(tab).map(|(ws, _)| ws);
        let pops = live && !self.restoring && ws.is_some_and(|ws| self.workspace_row_on_screen(ws));
        let doing = match self.model.tab(tab).map(|t| &t.kind) {
            Some(TabKind::Agent { title, .. }) => title.clone(),
            _ => None,
        };
        let Some(slot) = self.agents.get_mut(&tab) else {
            return;
        };
        let old = slot.status;
        // How long it worked, when a Working stretch ends here (for a Done notification).
        let worked = (old == Status::Working && new != old).then(|| slot.since.elapsed());
        // The log's `lasted` is read before `since` is reset.
        let lasted = slot.since.elapsed();
        slot.status = new;
        if old != new {
            slot.since = Instant::now();
        }
        if new != Status::Done {
            slot.pop = None;
        } else if old != Status::Done && pops {
            self.pops += 1;
            slot.pop = Some((self.pops, Instant::now()));
        }
        let question = slot.question.clone().filter(|_| new == Status::Waiting);
        if live && old != new {
            slot.activity
                .status(new, lasted, question.clone(), SystemTime::now());
        }
        let agent = slot.label.to_string();
        // As `set_status` for a terminal: every change is a `status.changed` event (M11aa
        // Task 10); a notification only when it is news.
        if old != new {
            self.emit_agent(&Event::StatusChanged { from: old, to: new }, tab);
        }
        if live && notifier::should_notify(old, new, looking, window_active) {
            self.notify_agent(
                tab,
                ws,
                &agent,
                new,
                question.as_deref(),
                worked,
                doing.as_deref(),
                cx,
            );
        }
        self.check_waits(&[], cx);
        cx.notify();
    }

    /// Agent tab `tab`'s notification (`set_agent_status`): as a terminal's, in workspace `ws`,
    /// by `agent` (its provider's label), and a `notification` event about the tab.
    #[allow(clippy::too_many_arguments)]
    fn notify_agent(
        &mut self,
        tab: TabId,
        ws: Option<WorkspaceId>,
        agent: &str,
        new: Status,
        question: Option<&str>,
        worked: Option<std::time::Duration>,
        doing: Option<&str>,
        cx: &mut Context<Self>,
    ) {
        let Some((message, body)) =
            notifier::message(new).zip(notifier::body(new, question, worked, doing))
        else {
            return;
        };
        let name = ws.map_or_else(
            || agent.to_string(),
            |ws| self.workspace_label(ws).to_string(),
        );
        let group = ws.and_then(|ws| self.group_label(ws));
        let number = ws.and_then(|ws| self.workspace_number(ws)).unwrap_or(0);
        let summary = notifier::banner_title(&name, message);
        let subtitle = notifier::banner_subtitle(number, group.as_deref(), &name, Some(agent));
        let posted = Event::Notification {
            title: &summary,
            body: &body,
        };
        self.emit_agent(&posted, tab);
        self.note_notification(notification_list::Entry {
            terminal: None,
            title: name,
            message: body.clone(),
            status: Some(new),
            at: Instant::now(),
        });
        // A short run (an agent's pause) is in the 🔔 list but makes no banner.
        if !notifier::worth_a_banner(new, worked) {
            return;
        }
        #[cfg(feature = "ui-shots")]
        if let Some(kept) = &mut self.shot_banners {
            kept.push((summary, subtitle, body));
            return;
        }
        let about = self.banner_about(None);
        // M11x: the banner's sound, with the banner (never for the 🔔 list alone).
        crate::sounds::banner(cx);
        cx.background_spawn(async move { notifier::fire(&summary, &subtitle, &body, about) })
            .detach();
    }

    /// The agent tabs of the shown panes, in layout then bar order: each tab, its `aN` and its
    /// status (their marks, R2).
    pub(crate) fn tab_agents(&self) -> Vec<(TabId, u64, Status)> {
        let Some(tree) = self.shown_layout() else {
            return Vec::new();
        };
        tree.leaves()
            .into_iter()
            .filter_map(|pane| self.model.pane(pane))
            .flat_map(|p| &p.tabs)
            .filter_map(|t| {
                let slot = self.agents.get(&t.id)?;
                Some((t.id, slot.id, slot.status))
            })
            .collect()
    }

    /// A message from paused agent tab `tab` (R13): one fresh `ps` (on the background
    /// executor, capped at 2 s) says whether another process holds its conversation
    /// (`conversation_holder`), then the tab sends (resuming) or warns and keeps `text`. While
    /// its classic view is opening, it is held without a look; while a check runs, another
    /// send (Enter again) is ignored.
    fn check_holder_then_resume(
        &mut self,
        tab: TabId,
        text: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(TabKind::Agent {
            provider,
            conversation,
            ..
        }) = self.model.tab(tab).map(|t| t.kind.clone())
        else {
            return;
        };
        let (Some(view), Some(provider)) = (
            self.agents.get(&tab).map(|slot| slot.view.clone()),
            providers::by_name(&provider),
        ) else {
            return;
        };
        if self.classic_opening.contains(&tab) {
            view.update(cx, |view, cx| {
                view.resume_after_classic(text, Holder::Held, Some(OPENING.into()), cx)
            });
            return;
        }
        if !self.resume_checks.insert(tab) {
            return; // its first send's check still runs
        }
        let id = conversation.or_else(|| view.read(cx).resume_id());
        // The classic view as the check counts on it, and its terminal's name.
        let (classic, terminal) = self.classic_seen(tab, cx);
        let terminal = terminal.map(|sid| self.terminal_label(sid));
        let own = view.read(cx).own_pids();
        let before = self.classic_of.get(&tab).copied();
        self.with_ps(window, cx, move |this, table, _, cx| {
            this.resume_checks.remove(&tab);
            // Its classic view started opening (or opened) meanwhile: this look is stale.
            if classic_changed(
                this.classic_opening.contains(&tab),
                before,
                this.classic_of.get(&tab).copied(),
            ) {
                view.update(cx, |view, cx| {
                    view.resume_after_classic(text, Holder::Held, Some(OPENING.into()), cx)
                });
                return;
            }
            let holder = match &id {
                Some(id) => conversation_holder(table.as_deref(), provider, id, &own, &classic),
                None => Holder::Free, // nothing to resume: nothing to hold
            };
            view.update(cx, |view, cx| {
                view.resume_after_classic(text, holder, terminal, cx)
            });
        });
    }

    /// "Open in classic view" (R13): while it works or waits for you, a native sheet asks first.
    /// Then the tab's own process stops (it reads Done, no banner) and a terminal tab of the
    /// same pane runs the agent's own UI on the conversation (the provider's resume command,
    /// `classic_argv`).
    pub(crate) fn open_classic(&mut self, tab: TabId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(TabKind::Agent {
            conversation: Some(_),
            ..
        }) = self.model.tab(tab).map(|t| &t.kind)
        else {
            return;
        };
        // Its classic view is opening: never a second one. A send's check runs: it may
        // resume the tab, so no classic view opens beside it (the tab says so).
        if self.classic_opening.contains(&tab) || self.still_checking(tab, cx) {
            return;
        }
        // Its classic view is open already: that terminal is shown, never a second one.
        if let Some(index) = self
            .classic_seen(tab, cx)
            .1
            .and_then(|sid| self.index_of(sid))
        {
            self.select(index, window, cx);
            return;
        }
        if !self.agent_busy(tab, cx) {
            self.open_classic_now(tab, window, cx);
            return;
        }
        let title = self.agent_title(tab);
        let label = self.agents[&tab].label.clone();
        let answer = window.prompt(
            PromptLevel::Warning,
            &format!("Open {title} in classic view?"),
            Some(&format!("{label}'s turn stops.")),
            &["Open", "Cancel"],
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            if answer.await != Ok(0) {
                return;
            }
            this.update_in(cx, |this, window, cx| {
                this.open_classic_now(tab, window, cx)
            })
            .ok();
        })
        .detach();
    }

    /// `open_classic` once asked (or not busy): the tab pauses, then one fresh `ps` checks that
    /// nothing else holds the conversation (a classic view left from before a restart, the
    /// provider's resume command typed elsewhere): held (or unchecked), the tab says so and no
    /// second one opens. Free, the classic view opens once the tab's own process is reaped. From the
    /// pause until then, the tab's classic view is opening (`classic_opening`).
    fn open_classic_now(&mut self, tab: TabId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(TabKind::Agent {
            provider,
            conversation: Some(id),
            ..
        }) = self.model.tab(tab).map(|t| t.kind.clone())
        else {
            return;
        };
        let Some(provider) = providers::by_name(&provider) else {
            return;
        };
        if self.still_checking(tab, cx) || !self.classic_opening.insert(tab) {
            return; // a send's check runs, or opening already (a sheet answered twice)
        }
        // A classic view that may still run it (from before a restart, in a terminal this run
        // can't see): counted on, so no second one opens beside it.
        let (classic, _) = self.classic_seen(tab, cx);
        let view = self.agents.get(&tab).map(|slot| slot.view.clone());
        if let Some(view) = &view {
            view.update(cx, |view, cx| view.pause_for_classic(cx));
        }
        let own = view
            .as_ref()
            .map(|v| v.read(cx).own_pids())
            .unwrap_or_default();
        self.with_ps(window, cx, move |this, table, window, cx| {
            let holder = conversation_holder(table.as_deref(), provider, &id, &own, &classic);
            if holder != Holder::Free {
                this.classic_opening.remove(&tab);
                if let Some(view) = &view {
                    view.update(cx, |view, cx| view.held(&holder, None, cx));
                }
                return;
            }
            this.open_classic_view(tab, view, window, cx);
        });
    }

    /// The classic view's terminal tab in agent tab `tab`'s pane, running the agent's own UI on
    /// its conversation, once the tab's own process (`view`'s) is reaped (bounded: its SIGKILL
    /// comes after `TERM_GRACE`), never beside it: one still running past `REAP_WAIT` more,
    /// none opens and the tab says so. Either way the tab's classic view is no longer opening.
    fn open_classic_view(
        &mut self,
        tab: TabId,
        view: Option<Entity<AgentTab>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(TabKind::Agent {
            provider,
            cwd,
            conversation: Some(id),
            ..
        }) = self.model.tab(tab).map(|t| t.kind.clone())
        else {
            self.classic_opening.remove(&tab);
            return;
        };
        let (Some(p), Some((_, pane))) = (providers::by_name(&provider), self.model.locate(tab))
        else {
            self.classic_opening.remove(&tab);
            return;
        };
        let project = (self.places.home.as_ref() != Some(&cwd)).then_some(cwd);
        let launch = Launch {
            saved: SavedSession {
                project,
                preset: Some(Preset {
                    name: provider.clone(),
                    command: p.classic_argv(&id).join(" "),
                }),
                ..Default::default()
            },
            run_preset: true,
        };
        let then: Opened = Box::new(move |this, opened, _, _| {
            this.classic_opening.remove(&tab);
            if let Some(id) = opened {
                this.classic_of.insert(tab, (id, Instant::now()));
                // A background-service terminal outlives fuxx: noted with the tab, so a
                // restart still counts on it (a local one ends with fuxx: not noted).
                let session = this.daemon_backed().then_some(id);
                if this.model.set_agent_classic(tab, session) {
                    this.save();
                }
            }
        });
        let reaped = |view: &Option<Entity<AgentTab>>, cx: &App| {
            view.as_ref().is_none_or(|v| v.read(cx).all_reaped())
        };
        if reaped(&view, cx) {
            self.open_session_then(launch, At::Tab(pane), Some(then), window, cx);
            return;
        }
        let until = Instant::now() + crate::agent_process::TERM_GRACE + REAP_WAIT;
        cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(50))
                    .await;
                let Ok(reaped) = this.update(cx, |_, cx| reaped(&view, cx)) else {
                    return; // the workspace has gone
                };
                match reap_wait(reaped, Instant::now() >= until) {
                    ReapWait::Wait => continue,
                    ReapWait::Open => break,
                    ReapWait::Refuse => {
                        this.update(cx, |this, cx| {
                            this.classic_opening.remove(&tab);
                            if let Some(view) = &view {
                                view.update(cx, |view, cx| view.classic_not_opened(cx));
                            }
                        })
                        .ok();
                        return;
                    }
                }
            }
            this.update_in(cx, |this, window, cx| {
                this.open_session_then(launch, At::Tab(pane), Some(then), window, cx)
            })
            .ok();
        })
        .detach();
    }

    /// Agent tab `tab`'s classic view as a holder check counts on it, and the terminal of this
    /// run that shows it (`classic_state` over its `classic_of` note, the session saved with
    /// the tab, and this run's terminals).
    pub(crate) fn classic_seen(&self, tab: TabId, cx: &App) -> (Classic, Option<u64>) {
        let entry = self
            .classic_of
            .get(&tab)
            .map(|&(sid, opened)| (sid, opened.elapsed() < CLASSIC_START));
        let seen = |id: u64| {
            let s = self.sessions.items().iter().find(|s| s.id == id)?;
            let view = s.view.read(cx);
            Some(SeenTerminal {
                service: view.daemon_id(),
                shell: (s.ssh.is_none() && !s.lost)
                    .then(|| view.working_folder().1)
                    .flatten(),
            })
        };
        classic_state(
            entry,
            self.model.agent_classic(tab),
            seen,
            crate::agent_process::started_pids(),
        )
    }

    /// Whether a send's holder check runs for agent tab `tab`; if so, the tab says to try again
    /// (an "Open in classic view" it ignores).
    fn still_checking(&self, tab: TabId, cx: &mut App) -> bool {
        if !self.resume_checks.contains(&tab) {
            return false;
        }
        if let Some(slot) = self.agents.get(&tab) {
            slot.view
                .update(cx, |view, cx| view.notice(CHECKING_NOTICE.into(), cx));
        }
        true
    }

    /// This Mac's process table, read once on the background executor (`ps`, capped at 2 s;
    /// `None`: it failed), then `then` (the scenes give their own table: `shot_ps`).
    fn with_ps(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
        then: impl FnOnce(&mut Self, Option<Vec<Proc>>, &mut Window, &mut Context<Self>) + 'static,
    ) {
        #[cfg(feature = "ui-shots")]
        let shot = self.shot_ps.clone();
        #[cfg(not(feature = "ui-shots"))]
        let shot: Option<Option<Vec<Proc>>> = None;
        cx.spawn_in(window, async move |this, cx| {
            let table = match shot {
                Some(table) => table,
                None => {
                    cx.background_executor()
                        .spawn(async { procs::ps_table() })
                        .await
                }
            };
            this.update_in(cx, |this, window, cx| then(this, table, window, cx))
                .ok();
        })
        .detach();
    }

    /// Session `sN` (`session`, a background-service one when `service`) is closing: an agent
    /// tab whose classic view it was forgets it (R13), so a later check no longer counts on it
    /// (a reused id after the service restarts would hold the tab for nothing). Only the
    /// service's own session clears a saved one (a local terminal may share its number).
    pub(crate) fn classic_terminal_closed(&mut self, session: u64, service: bool) {
        self.classic_of.retain(|_, &mut (sid, _)| sid != session);
        if service && self.model.forget_agent_classics_in(session) {
            self.save();
        }
    }

    /// Whether workspace `ws` runs on another host (any terminal of it is an SSH session's):
    /// agent tabs are this Mac's only (spec §1.6).
    pub(crate) fn workspace_is_ssh(&self, ws: WorkspaceId) -> bool {
        let terminals = self.model.terminals(ws);
        self.sessions
            .items()
            .iter()
            .any(|s| terminals.contains(&s.id) && s.ssh.is_some())
    }

    /// Open `launch` `at` a place as `launch_plan_for` says: a provider preset opens a plain
    /// shell (never re-typed on restore) and, once it has opened (after `then`), its agent tab
    /// beside it; any other launch as before.
    pub(crate) fn open_planned(
        &mut self,
        mut launch: Launch,
        at: At,
        then: Option<Opened>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let plan = launch
            .saved
            .preset
            .as_ref()
            .map_or(LaunchPlan::Classic, launch_plan_for);
        let LaunchPlan::ShellThenAgent(provider) = plan else {
            self.open_session_then(launch, at, then, window, cx);
            return;
        };
        launch.saved.preset = None;
        launch.run_preset = false;
        let then: Opened = Box::new(move |this, opened, window, cx| {
            if let Some(then) = then {
                then(this, opened, window, cx);
            }
            if let Some(id) = opened {
                this.agent_beside(id, provider, window, cx);
            }
        });
        self.open_session_then(launch, at, Some(then), window, cx);
    }
}

/// The scenes' hooks (`ui_shots`).
#[cfg(feature = "ui-shots")]
impl Workspace {
    /// Agent tab `tab`'s status dated `ago` back, so its age label turns soon (as the scenes'
    /// `minutes_old` dates a terminal's). Nothing in the app dates a status back, so it notifies
    /// itself.
    pub(crate) fn shot_agent_since(
        &mut self,
        tab: TabId,
        ago: std::time::Duration,
        cx: &mut Context<Self>,
    ) {
        if let Some(slot) = self.agents.get_mut(&tab) {
            slot.since = Instant::now() - ago;
            cx.notify();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::claude::CLAUDE;

    /// Another conversation's session id (a UUID, as Claude's are).
    const OTHER: &str = "0b3c4a5e-1f2d-4e6a-9b8c-7d6e5f4a3b2c";

    #[test]
    fn a_native_claude_resume_with_equals_holds_it() {
        const ID: &str = "5f1e2d3c-4b5a-4968-8776-655443322110";
        let row = |args: &str| ps(&format!("  700     1   700   700 S+   {args}\n"));
        for args in [
            format!("claude --resume={ID}"),
            format!("claude --session-id={ID}"),
        ] {
            assert_eq!(
                conversation_holder(Some(&row(&args)), &CLAUDE, ID, &[], &Classic::Closed),
                Holder::Held,
                "{args}"
            );
        }
        // Another conversation's, and the tab's own: free.
        let other = row(&format!("claude --resume={OTHER}"));
        assert_eq!(
            conversation_holder(Some(&other), &CLAUDE, ID, &[], &Classic::Closed),
            Holder::Free
        );
        let own = row(&format!("claude --resume={ID}"));
        assert_eq!(
            conversation_holder(Some(&own), &CLAUDE, ID, &[700], &Classic::Closed),
            Holder::Free
        );
    }

    /// R13 holds for a provider other than Claude: an `agy` (a native binary, its arguments as
    /// given) opened by `classic_argv` (`agy --conversation <id>`), typed with `=`, or run by
    /// path, holds the conversation; the tab's own `agy -p= … --conversation <id>`, another
    /// conversation's, and a `claude` naming the same id don't.
    #[test]
    fn a_classic_agy_holds_its_conversation() {
        use crate::providers::agy::AGY;
        const ID: &str = "6a1c234d-7469-40ef-b093-c74ad8006c28";
        let classic = AGY.classic_argv(ID).join(" ");
        let table = ps(&format!(
            "  500     1   500   600 Ss   -zsh\n  600   500   600   600 S+   {classic}\n  \
             900     1   900     0 Ss   agy -p= --input-format stream-json --output-format \
             stream-json --mode accept-edits --conversation {ID}\n"
        ));
        assert_eq!(
            conversation_holder(Some(&table), &AGY, ID, &[900], &Classic::Closed),
            Holder::Held
        );
        assert_eq!(
            conversation_holder(Some(&table), &AGY, ID, &[900], &Classic::Shell(500)),
            Holder::Held
        );
        let row = |args: &str| ps(&format!("  700     1   700   700 S+   {args}\n"));
        for (args, holder) in [
            (format!("agy --conversation={ID}"), Holder::Held),
            (
                format!("/Users/me/.local/bin/agy --mode plan --conversation {ID}"),
                Holder::Held,
            ),
            (format!("agy --conversation {OTHER}"), Holder::Free),
            (format!("claude --resume {ID}"), Holder::Free),
        ] {
            assert_eq!(
                conversation_holder(Some(&row(&args)), &AGY, ID, &[], &Classic::Closed),
                holder,
                "{args}"
            );
        }
        // Only the tab's own process left: free.
        let own = ps(&format!(
            "  900     1   900     0 Ss   agy -p= --mode auto --conversation {ID}\n"
        ));
        assert_eq!(
            conversation_holder(Some(&own), &AGY, ID, &[900], &Classic::Closed),
            Holder::Free
        );
        // A background-service classic view: an `agy -c` there may be it; another id is not.
        let blind = Classic::Blind(vec![900]);
        for (args, holder) in [
            ("agy -c".to_string(), Holder::Held),
            (format!("agy --conversation {OTHER}"), Holder::Free),
        ] {
            assert_eq!(
                conversation_holder(Some(&row(&args)), &AGY, ID, &[900], &blind),
                holder,
                "{args}"
            );
        }
    }

    #[test]
    fn a_saved_service_classic_view_this_run_cant_see_stays_blind() {
        let agents = || vec![900];
        let blind = Classic::Blind(vec![900]);
        let nothing = |_: u64| None;
        let service7 = |id: u64| {
            (id == 7).then_some(SeenTerminal {
                service: Some(7),
                shell: None,
            })
        };
        // A local terminal of this run that only shares the saved session's number.
        let local7 = |id: u64| {
            (id == 7).then_some(SeenTerminal {
                service: None,
                shell: Some(500),
            })
        };
        let local3 = |id: u64| {
            (id == 3).then_some(SeenTerminal {
                service: None,
                shell: Some(500),
            })
        };
        // Saved, but not in this run's list (the service couldn't be reached, or adopting it
        // failed): its classic terminal may still run the agent → blind, never closed.
        assert_eq!(
            classic_state(Some((7, false)), Some(7), nothing, agents()),
            (blind.clone(), None)
        );
        assert_eq!(
            classic_state(None, Some(7), nothing, agents()),
            (blind.clone(), None)
        );
        // A local session whose id equals the saved one is not it: blind, not its shell.
        assert_eq!(
            classic_state(Some((7, false)), Some(7), local7, agents()),
            (blind.clone(), None)
        );
        assert_eq!(classic_state(None, Some(7), local7, agents()).0, blind);
        // The service's own session 7: blind (no shell pid), and it's the terminal to name.
        assert_eq!(
            classic_state(Some((7, false)), Some(7), service7, agents()),
            (blind.clone(), Some(7))
        );
        // A local classic view of this run (never saved): its shell; gone, closed.
        assert_eq!(
            classic_state(Some((3, false)), None, local3, agents()),
            (Classic::Shell(500), Some(3))
        );
        assert_eq!(
            classic_state(Some((3, false)), None, nothing, agents()),
            (Classic::Closed, None)
        );
        assert_eq!(
            classic_state(None, None, local3, agents()),
            (Classic::Closed, None)
        );
        // Opened moments ago: starting, whatever `ps` will say; its terminal already gone,
        // closed.
        assert_eq!(
            classic_state(Some((3, true)), None, local3, agents()),
            (Classic::Starting, Some(3))
        );
        assert_eq!(
            classic_state(Some((3, true)), None, nothing, agents()),
            (Classic::Closed, None)
        );
        // So an npm `claude` left in that unseen service terminal holds the conversation.
        let t = ps(NPM);
        let (classic, _) = classic_state(Some((7, false)), Some(7), local7, vec![900, 950]);
        assert_eq!(
            conversation_holder(Some(&t), &CLAUDE, "c1", &[900], &classic),
            Holder::Held
        );
    }

    #[test]
    fn launch_plan_follows_the_provider() {
        let p = |n: &str, c: &str| Preset {
            name: n.into(),
            command: c.into(),
        };
        assert_eq!(
            launch_plan_for(&p("claude", "claude")),
            LaunchPlan::ShellThenAgent("claude")
        );
        assert_eq!(
            launch_plan_for(&p("claude", "claude --model sonnet")),
            LaunchPlan::ShellThenAgent("claude")
        );
        assert_eq!(launch_plan_for(&p("codex", "codex")), LaunchPlan::Classic);
    }

    #[test]
    fn an_unnamed_shell_row_takes_the_agent_label() {
        assert_eq!(agent_row_label(Some("Claude"), false), Some("Claude"));
        assert_eq!(agent_row_label(Some("Claude"), true), None);
        assert_eq!(agent_row_label(None, false), None);
    }

    #[test]
    fn sending_after_classic_view_never_makes_a_second_writer() {
        assert_eq!(after_classic(false, false), AfterClassic::Send);
        assert_eq!(after_classic(false, true), AfterClassic::Send);
        assert_eq!(after_classic(true, true), AfterClassic::Warn);
        assert_eq!(after_classic(true, false), AfterClassic::Respawn);
    }

    fn ps(text: &str) -> Vec<Proc> {
        procs::parse_ps(text)
    }

    /// `ps` as it would read with a native (`claude` binary) classic terminal's
    /// `claude --resume c1`: its shell 500 (a local terminal, at no prompt: `claude` has it) and
    /// that `claude`; another conversation's `claude`; the tab's own `claude -p … --resume c1`
    /// (group 900, which it leads) and a tool its agent runs (in that group).
    const TABLE: &str = "\
  500     1   500   600 Ss   -zsh
  600   500   600   600 S+   claude --resume c1
  700     1   700   700 S+   claude --resume c2
  900     1   900     0 Ss   claude -p --input-format stream-json --resume c1
  901   900   900     0 S    /bin/zsh -c npm test
";

    /// The same with npm's `claude` (node): it sets `process.title = "claude"`, which writes
    /// over its arguments, so `ps` shows each one as `claude` alone, `--resume c1` gone. The
    /// classic terminal's (600), another conversation's run natively elsewhere (700), the
    /// tab's own (900, with a tool in its group) and another agent tab's (950).
    const NPM: &str = "\
  500     1   500   600 Ss   -zsh
  600   500   600   600 S+   claude
  700     1   700   700 S+   claude --resume 0b3c4a5e-1f2d-4e6a-9b8c-7d6e5f4a3b2c
  900     1   900     0 Ss   claude
  901   900   900     0 S    /bin/zsh -c npm test
  950     1   950     0 Ss   claude
";

    #[test]
    fn an_npm_claude_in_a_blind_classic_view_holds_it() {
        let t = ps(NPM);
        // A local classic terminal: its shell shows `claude` in front.
        assert_eq!(
            conversation_holder(Some(&t), &CLAUDE, "c1", &[900], &Classic::Shell(500)),
            Holder::Held
        );
        // A background-service one (no shell pid): its `claude` lost the id; it may be it.
        let agents = Classic::Blind(vec![900, 950]);
        assert_eq!(
            conversation_holder(Some(&t), &CLAUDE, "c1", &[900], &agents),
            Holder::Held
        );
        // Quit there: fuxx's own agents and another conversation's `claude` hold nothing.
        let quit = ps(&NPM.replace("  600   500   600   600 S+   claude\n", ""));
        assert_eq!(
            conversation_holder(Some(&quit), &CLAUDE, "c1", &[900], &agents),
            Holder::Free
        );
        // `claude -c` or a bare `claude --resume` there may be it too; another session id is not.
        for (args, holder) in [
            ("claude -c", Holder::Held),
            ("claude --resume", Holder::Held),
            // A prompt as `ps` joins it, and the picker's search: no session id there.
            ("claude -c fix the grep -r foo call", Holder::Held),
            ("claude --resume searchterm", Holder::Held),
            ("claude --resume=searchterm", Holder::Held),
            ("claude -r OTHER", Holder::Free),
            ("claude --resume=OTHER", Holder::Free),
            ("claude --session-id OTHER", Holder::Free),
        ] {
            let args = args.replace("OTHER", OTHER);
            let t = ps(&format!("  600   500   600   600 S+   {args}\n"));
            assert_eq!(
                conversation_holder(Some(&t), &CLAUDE, "c1", &[], &agents),
                holder,
                "{args}"
            );
        }
    }

    #[test]
    fn a_restored_tab_counts_on_its_classic_view_while_its_session_lives() {
        // An older file (no classic view noted), or a local one (never noted): closed.
        assert_eq!(
            restored_classic(None, Some(&[7]), vec![900]),
            Classic::Closed
        );
        assert_eq!(restored_classic(None, None, vec![]), Classic::Closed);
        // Its session still there: blind (an npm `claude` there hides its arguments).
        assert_eq!(
            restored_classic(Some(7), Some(&[3, 7]), vec![900]),
            Classic::Blind(vec![900])
        );
        // Gone: closed.
        assert_eq!(
            restored_classic(Some(7), Some(&[3]), vec![900]),
            Classic::Closed
        );
        // The service couldn't be asked: it may be there.
        assert_eq!(
            restored_classic(Some(7), None, vec![900]),
            Classic::Blind(vec![900])
        );
        // So an npm `claude` left running there holds the restored tab's conversation.
        let t = ps(NPM);
        let classic = restored_classic(Some(7), Some(&[7]), vec![]);
        assert_eq!(
            conversation_holder(Some(&t), &CLAUDE, "c1", &[], &classic),
            Holder::Held
        );
    }

    #[test]
    fn a_late_check_sees_a_classic_view_opened_meanwhile() {
        let at = Instant::now();
        let later = at + std::time::Duration::from_secs(1);
        assert!(!classic_changed(false, None, None));
        assert!(!classic_changed(false, Some((3, at)), Some((3, at))));
        // It started opening, or it opened, or another one did: stale.
        assert!(classic_changed(true, None, None));
        assert!(classic_changed(true, Some((3, at)), Some((3, at))));
        assert!(classic_changed(false, None, Some((4, later))));
        assert!(classic_changed(false, Some((3, at)), Some((4, later))));
    }

    #[test]
    fn the_classic_view_never_opens_beside_a_live_process() {
        assert_eq!(reap_wait(true, false), ReapWait::Open);
        assert_eq!(reap_wait(true, true), ReapWait::Open);
        assert_eq!(reap_wait(false, false), ReapWait::Wait);
        assert_eq!(reap_wait(false, true), ReapWait::Refuse);
    }

    #[test]
    fn a_classic_claude_holds_its_conversation() {
        let t = ps(TABLE);
        assert_eq!(
            conversation_holder(Some(&t), &CLAUDE, "c1", &[900], &Classic::Shell(500)),
            Holder::Held
        );
        // A daemon session's terminal: fuxx has no shell pid; the process itself says it.
        assert_eq!(
            conversation_holder(Some(&t), &CLAUDE, "c1", &[900], &Classic::Closed),
            Holder::Held
        );
        assert_eq!(
            conversation_holder(Some(&t), &CLAUDE, "c2", &[], &Classic::Closed),
            Holder::Held
        );
    }

    #[test]
    fn the_tabs_own_process_group_is_not_a_holder() {
        let t = ps(&TABLE.replace("  600   500   600   600 S+   claude --resume c1\n", ""));
        assert_eq!(
            conversation_holder(Some(&t), &CLAUDE, "c1", &[900], &Classic::Closed),
            Holder::Free
        );
        // Without excluding it, its own process would hold it.
        assert_eq!(
            conversation_holder(Some(&t), &CLAUDE, "c1", &[], &Classic::Closed),
            Holder::Held
        );
        // A zombie holds nothing; nor does another program naming the id.
        let z = ps("  600   500   600   600 Z+   claude --resume c1\n  610 1 610 0 S vim c1\n");
        assert_eq!(
            conversation_holder(Some(&z), &CLAUDE, "c1", &[], &Classic::Closed),
            Holder::Free
        );
    }

    #[test]
    fn a_claude_typed_in_the_classic_terminal_holds_it() {
        // `claude -c` (no id in its arguments) in front of the classic terminal's shell.
        let t = ps("  500     1   500   600 Ss   -zsh\n  600   500   600   600 S+   claude -c\n");
        assert_eq!(
            conversation_holder(Some(&t), &CLAUDE, "c1", &[], &Classic::Shell(500)),
            Holder::Held
        );
        // At its prompt: free.
        let t = ps("  500     1   500   500 Ss+  -zsh\n");
        assert_eq!(
            conversation_holder(Some(&t), &CLAUDE, "c1", &[], &Classic::Shell(500)),
            Holder::Free
        );
        // Opened moments ago: its `claude` may not have started yet.
        assert_eq!(
            conversation_holder(Some(&t), &CLAUDE, "c1", &[], &Classic::Starting),
            Holder::Held
        );
    }

    #[test]
    fn a_failed_ps_never_lets_the_tab_respawn() {
        let holder = conversation_holder(None, &CLAUDE, "c1", &[], &Classic::Closed);
        assert_eq!(holder, Holder::Unchecked);
        assert_eq!(
            after_classic(true, holder != Holder::Free),
            AfterClassic::Warn
        );
        assert!(held_notice("Claude", &holder, None).starts_with("Couldn't check"));
    }

    #[test]
    fn a_resumed_tab_held_elsewhere_starts_paused() {
        assert!(starts_paused(true, &Holder::Held));
        assert!(starts_paused(true, &Holder::Unchecked));
        assert!(!starts_paused(true, &Holder::Free));
        assert!(!starts_paused(false, &Holder::Free));
    }

    #[test]
    fn waiting_agent_beats_idle_shell() {
        assert_eq!(
            row_winner(Some((0, Status::Idle)), &[(TabId(7), Status::Waiting)]),
            Some(RowWinner::Agent(TabId(7)))
        );
    }

    #[test]
    fn a_tie_goes_to_the_terminal() {
        assert_eq!(
            row_winner(Some((0, Status::Done)), &[(TabId(7), Status::Done)]),
            Some(RowWinner::Terminal(0))
        );
        // Between agent tabs, to the first in reading order.
        assert_eq!(
            row_winner(
                Some((0, Status::Idle)),
                &[(TabId(4), Status::Working), (TabId(2), Status::Working)]
            ),
            Some(RowWinner::Agent(TabId(4)))
        );
    }

    #[test]
    fn no_agents_is_the_terminal() {
        assert_eq!(
            row_winner(Some((2, Status::Working)), &[]),
            Some(RowWinner::Terminal(2))
        );
        assert_eq!(row_winner(None, &[]), None);
    }

    #[test]
    fn an_agent_only_workspace_has_an_agent_subject() {
        assert_eq!(
            row_winner(None, &[(TabId(3), Status::Idle)]),
            Some(RowWinner::Agent(TabId(3)))
        );
    }

    #[test]
    fn an_unknown_provider_looks_grey_with_its_name() {
        assert_eq!(provider_look("gone"), ("gone".into(), "?", 0x6b6b6b));
        let (label, badge, _) = provider_look("claude");
        assert_eq!((label.as_ref(), badge), ("Claude", "CC"));
    }
}
