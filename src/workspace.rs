//! The root view: a sidebar listing every session (with its status ring) and the active
//! session's terminal. Owns session lifecycle, switching, and background notifications.

use std::path::{Path, PathBuf};

use gpui::{
    ClickEvent, Context, Entity, Focusable, PathPromptOptions, SharedString, Subscription, Window,
    actions, div, prelude::*, px,
};

use crate::appearance::Appearance;
use crate::groups;
use crate::notifier;
use crate::presets::Preset;
use crate::restore::{Launch, SavedSession};
use crate::session_list::SessionList;
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
    ]
);

const SIDEBAR_WIDTH: f32 = 220.0;

pub struct Session {
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

pub struct Workspace {
    sessions: SessionList<Session>,
    window_active: bool,
    _activation: Subscription,
    /// Repaints the sidebar when the appearance changes.
    _appearance: Subscription,
}

impl Workspace {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let activation = cx.observe_window_activation(window, |this, window, cx| {
            this.window_active = window.is_window_active();
            cx.notify();
        });
        let appearance = cx.observe_global::<Appearance>(|_, cx| cx.notify());
        let mut this = Self {
            sessions: SessionList::new(),
            window_active: window.is_window_active(),
            _activation: activation,
            _appearance: appearance,
        };
        this.open_session(Launch::default(), window, cx);
        this
    }

    /// Open a session for `launch` next to its group and make it active.
    fn open_session(&mut self, launch: Launch, window: &mut Window, cx: &mut Context<Self>) {
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
        let view = match TerminalView::spawn(window, cx, cwd, startup) {
            Ok(view) => view,
            Err(err) => {
                eprintln!("fuxx: could not start a shell: {err}");
                return;
            }
        };
        let subscription = cx.subscribe_in(&view, window, Self::on_terminal_event);
        let projects: Vec<Option<&Path>> = self
            .sessions
            .items()
            .iter()
            .map(|s| s.project.as_deref())
            .collect();
        let index = groups::insert_index(&projects, saved.project.as_deref());
        self.sessions.insert(
            index,
            Session {
                view,
                status: Status::Idle,
                title: None,
                name: saved.name,
                project: saved.project,
                preset: saved.preset,
                _subscription: subscription,
            },
        );
        self.focus_active(window, cx);
        cx.notify();
    }

    /// The active session's folder, so a new session joins its group.
    fn active_project(&self) -> Option<PathBuf> {
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
            let launch = Launch {
                saved: SavedSession {
                    project: Some(folder),
                    ..Default::default()
                },
                run_preset: false,
            };
            this.update_in(cx, |this, window, cx| this.open_session(launch, window, cx))
                .ok();
        })
        .detach();
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
                if notifier::should_notify(old, *new, is_active, window_active) {
                    if let Some(body) = notifier::message(*new) {
                        let summary = format!("fuxx — {}", session.label());
                        cx.background_spawn(async move { notifier::fire(&summary, body) })
                            .detach();
                    }
                }
                cx.notify();
            }
            TerminalEvent::TitleChanged(title) => {
                if let Some(session) = self.sessions.get_mut(index) {
                    session.title = title.clone();
                }
                cx.notify();
            }
            TerminalEvent::Exited => self.close_at(index, window, cx),
        }
    }

    fn close_at(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        // Dropping the Session drops its view -> TerminalCore -> Pty, which kills the child.
        self.sessions.remove(index);
        if self.sessions.is_empty() {
            window.remove_window(); // last session gone: close the window (and quit)
            return;
        }
        self.focus_active(window, cx);
        cx.notify();
    }

    fn select(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.sessions.select(index) {
            self.focus_active(window, cx);
            cx.notify();
        }
    }

    fn focus_active(&self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(session) = self.sessions.active() {
            session.view.focus_handle(cx).focus(window, cx);
        }
    }

    fn render_sidebar(&self, ui: UiColors, cx: &mut Context<Self>) -> impl IntoElement {
        let active = self.sessions.active_index();
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
            .children(
                self.sessions
                    .items()
                    .iter()
                    .enumerate()
                    .map(|(i, session)| {
                        let is_active = active == Some(i);
                        div()
                            .id(("session", i))
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .px_2()
                            .py_1()
                            .rounded_md()
                            .text_sm()
                            .text_color(hsla(if is_active { ui.text } else { ui.text_dim }))
                            .when(is_active, |row| row.bg(hsla(ui.row_active)))
                            .hover(|style| style.bg(hsla(ui.row_hover)))
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                                this.select(i, window, cx)
                            }))
                            .child(ring(session.status, ui))
                            .child(format!("{}  {}", i + 1, session.label()))
                    }),
            )
    }
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
                this.focus_active(window, cx);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &PrevSession, window, cx| {
                this.sessions.select_prev();
                this.focus_active(window, cx);
                cx.notify();
            }))
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
            // Only the active terminal is rendered; background sessions keep running (their
            // reader threads and tickers live in their views, which the list keeps alive).
            .child(
                div()
                    .flex_1()
                    .h_full()
                    .children(self.sessions.active().map(|s| s.view.clone())),
            )
    }
}
