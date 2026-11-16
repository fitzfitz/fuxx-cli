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
//! `launch_plan_for`, `agent_row_label` and `after_classic` are pure and tested.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

use gpui::{
    App, Context, Div, Entity, FontWeight, PromptLevel, SharedString, Window, div, prelude::*, px,
};

use crate::activity::ActivityLog;
use crate::agent_tab::{AgentTab, AgentTabEvent};
use crate::layout_model::{PaneId, TabId, TabKind, WorkspaceId};
use crate::presets::{self, Preset};
use crate::procs::ProgramState;
use crate::providers::{self, Start};
use crate::restore::{Launch, SavedSession};
use crate::split_tree::Axis;
use crate::status::Status;
use crate::terminal_view::hsla;
use crate::ui_tokens::text;
use crate::workspace::{At, Opened, Workspace};

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
    /// Idle / when it was made, until Task 9 writes them.
    pub status: Status,
    pub since: Instant,
    #[allow(dead_code)] // M11aa Task 9 (the Done pop)
    pub pop: Option<(u64, Instant)>,
    #[allow(dead_code)] // M11aa Task 9 (the row's question)
    pub question: Option<String>,
    #[allow(dead_code)] // M11aa Task 9 (the Activity panel)
    pub activity: ActivityLog,
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
        let view = cx.new(|cx| match provider {
            // The scenes never start an agent: the tab is fed decoded events (R12).
            #[cfg(feature = "ui-shots")]
            Some(provider) if offline => AgentTab::for_shots(provider, start, window, cx),
            _ => {
                let _ = offline;
                AgentTab::new(provider, name.clone(), start, window, cx)
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
        if self.all_gone() {
            let conns = self.subscribers.clone();
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
                    let kind = TabKind::Agent {
                        provider,
                        cwd,
                        conversation: Some(id.clone()),
                        mode,
                        title,
                    };
                    self.model.set_kind(tab, kind);
                    self.save();
                }
            }
            AgentTabEvent::Close => self.close_agent_tab(tab, false, window, cx),
            AgentTabEvent::OpenClassic(_) => self.open_classic(tab, window, cx),
            AgentTabEvent::ResumeAsked(text) => {
                let running = self.classic_running(tab);
                if let Some(slot) = self.agents.get(&tab) {
                    let text = text.clone();
                    slot.view
                        .update(cx, |view, cx| view.resume_after_classic(text, running, cx));
                }
            }
            // M11aa Task 9: the chrome's status and question.
            AgentTabEvent::Status { .. } | AgentTabEvent::Question(_) => {}
        }
    }

    /// The terminal "Open in classic view" opened for agent tab `tab`, while it still runs the
    /// agent (by the programs' 3 s lookup: a `claude` quit just now may still read running): its
    /// name.
    fn classic_running(&self, tab: TabId) -> Option<SharedString> {
        let id = *self.classic_of.get(&tab)?;
        let TabKind::Agent { provider, .. } = &self.model.tab(tab)?.kind else {
            return None;
        };
        self.index_of(id)?;
        match self.programs.state(id) {
            ProgramState::Running(name) if name == *provider => Some(self.terminal_label(id)),
            _ => None,
        }
    }

    /// "Open in classic view" (R13): while it works or waits for you, a native sheet asks first.
    /// Then the tab's own process stops (it reads Done, no banner) and a terminal tab of the
    /// same pane runs the agent's own UI on the conversation (`claude --resume <id>`).
    pub(crate) fn open_classic(&mut self, tab: TabId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(TabKind::Agent {
            conversation: Some(_),
            ..
        }) = self.model.tab(tab).map(|t| &t.kind)
        else {
            return;
        };
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

    /// `open_classic` once asked (or not busy).
    fn open_classic_now(&mut self, tab: TabId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(TabKind::Agent {
            provider,
            cwd,
            conversation: Some(id),
            ..
        }) = self.model.tab(tab).map(|t| t.kind.clone())
        else {
            return;
        };
        let (Some(p), Some((_, pane))) = (providers::by_name(&provider), self.model.locate(tab))
        else {
            return;
        };
        if let Some(slot) = self.agents.get(&tab) {
            slot.view.update(cx, |view, cx| view.pause_for_classic(cx));
        }
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
            if let Some(id) = opened {
                this.classic_of.insert(tab, id);
            }
        });
        self.open_session_then(launch, At::Tab(pane), Some(then), window, cx);
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

/// The agent tabs' processes (quitting ends them: `flush_on_quit`).
pub(crate) fn processes(
    agents: &HashMap<TabId, AgentSlot>,
    cx: &App,
) -> Vec<std::sync::Arc<crate::agent_process::AgentProcess>> {
    agents
        .values()
        .filter_map(|slot| slot.view.read(cx).process())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn an_unknown_provider_looks_grey_with_its_name() {
        assert_eq!(provider_look("gone"), ("gone".into(), "?", 0x6b6b6b));
        let (label, badge, _) = provider_look("claude");
        assert_eq!((label.as_ref(), badge), ("Claude", "CC"));
    }
}
