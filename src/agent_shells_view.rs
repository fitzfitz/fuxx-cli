//! The shells tab and its popover (chore 16, note 93): `▣ 1/3` beside the plan tab, and over
//! the chat each background shell with its time, command, last output line and ■. The state
//! is `agent_shells`; the last lines are read off the UI thread while the popover is open.

use std::time::{Duration, Instant};

use gpui::{
    AnyElement, AppContext, Context, FontWeight, InteractiveElement, IntoElement, MouseButton,
    ParentElement, StatefulInteractiveElement, Styled, div, prelude::FluentBuilder, px,
};

use super::popover::{self, Mark};
use super::{AgentTab, AgentTabEvent, BLACK, Colors, MARK};
use crate::agent_box::BoxMenu;
use crate::agent_box::menus::MenuPlace;
use crate::agent_shells::{Shell, Tone, banner_words, last_line, read_tail, tab_words};
use crate::appearance::Appearance;
use crate::mark_view::MarkView;
use crate::providers::{Action, ShellEnd};
use crate::status::Status;
use crate::terminal_view::hsla;

/// The popover's width (a row carries a command and an output line).
pub(crate) const SHELLS_WIDTH: f32 = 400.0;

/// How often the open popover reads its shells' last lines.
const POLL_EVERY: Duration = Duration::from_secs(1);

impl AgentTab {
    /// The shells tab's pill (`▣ 1/3`): peach while one runs, its green icon alone once all are
    /// done (chore 34), red after a failure you haven't looked at; none before a shell started.
    pub(crate) fn shells_tab(&self, c: &Colors, cx: &mut Context<Self>) -> Option<AnyElement> {
        let shells = self.conversation.shells();
        if shells.is_empty() {
            return None;
        }
        let ink = match shells.tone() {
            Tone::Running => c.todo_active,
            Tone::Done => c.tool_ok,
            Tone::Failed => c.failed_panel,
        };
        // All done: the count moves into the tip (chore 34).
        let tip = match shells.tone() {
            Tone::Done => crate::corner_tabs::done_tip(shells.counts().1, "shell", "shells"),
            _ => "Background shells".to_string(),
        };
        Some(
            self.corner_pill(
                "agent-shells-tab",
                "shells-tab",
                crate::icons::Icon::Terminal,
                tab_words(shells),
                ink,
                BoxMenu::Shells,
                tip,
                |this, _, cx| this.toggle_shells(cx),
                c,
                cx,
            )
            .into_any_element(),
        )
    }

    /// Whether a background shell still runs (its time turns each second).
    pub(crate) fn shells_running(&self) -> bool {
        self.conversation
            .shells()
            .rows(Instant::now())
            .iter()
            .any(|s| s.running())
    }

    /// The shell `event` ends, when it is one still running (an end told twice is no news).
    pub(crate) fn shell_ending(&self, event: &crate::providers::Event) -> Option<String> {
        match event {
            crate::providers::Event::ShellEnded { id, .. } => self
                .conversation
                .shells()
                .get(id)
                .is_some_and(Shell::running)
                .then(|| id.clone()),
            _ => None,
        }
    }

    /// Shell `id` just ended: the Workspace hears it (for the banner).
    pub(crate) fn tell_shell_end(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(shell) = self.conversation.shells().get(id) else {
            return;
        };
        let failed = matches!(shell.ended, Some((ShellEnd::Failed, _)));
        cx.emit(AgentTabEvent::ShellEnded {
            words: banner_words(shell),
            failed,
        });
    }

    /// The tab's click: the popover opens (a failure is seen; its last lines are read from now
    /// on), or closes.
    pub(crate) fn toggle_shells(&mut self, cx: &mut Context<Self>) {
        if matches!(self.menu, Some(BoxMenu::Shells)) {
            self.close_menu(cx);
        } else if !self.conversation.shells().is_empty() {
            self.conversation.shells_mut().seen();
            self.open_menu(BoxMenu::Shells, cx);
            self.poll_shell_lines(cx);
        }
    }

    /// While the popover is open, each `POLL_EVERY`: the shown rows' output files' ends read on
    /// the background executor, their last lines kept (a file that can't be read keeps the line
    /// it had).
    fn poll_shell_lines(&mut self, cx: &mut Context<Self>) {
        self.shell_poll = Some(cx.spawn(async move |this, cx| {
            loop {
                let files = this
                    .update(cx, |this, _| {
                        matches!(this.menu, Some(BoxMenu::Shells))
                            .then(|| this.conversation.shells().row_files(Instant::now()))
                    })
                    .ok()
                    .flatten();
                let Some(files) = files else {
                    break;
                };
                let lines: Vec<(String, String)> = cx
                    .background_spawn(async move {
                        files
                            .into_iter()
                            .filter_map(|(id, path)| Some((id, last_line(&read_tail(&path)?)?)))
                            .collect()
                    })
                    .await;
                let kept = this.update(cx, |this, cx| {
                    let shells = this.conversation.shells_mut();
                    let mut changed = false;
                    for (id, line) in lines {
                        if shells.get(&id).and_then(|s| s.last_line.as_deref()) != Some(&line) {
                            shells.set_last_line(&id, Some(line));
                            changed = true;
                        }
                    }
                    if changed {
                        cx.notify();
                    }
                });
                if kept.is_err() {
                    break;
                }
                cx.background_executor().timer(POLL_EVERY).await;
            }
        }));
    }

    /// ■ on shell `id`: the agent stops it (its end comes as any other).
    pub(crate) fn stop_shell(&mut self, id: &str, cx: &mut Context<Self>) {
        self.write(&Action::StopShell { id: id.to_string() });
        cx.notify();
    }

    /// A row's click: its output file in a file tab (which follows the file as it grows).
    fn open_shell_output(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(path) = self
            .conversation
            .shells()
            .get(id)
            .and_then(|s| s.output.clone())
        else {
            return;
        };
        self.close_menu(cx);
        cx.emit(AgentTabEvent::OpenFile(path));
    }

    /// A Working arc for each running shell's popover row; none for the others.
    pub(crate) fn sync_shell_marks(&mut self, cx: &mut Context<Self>) {
        let running: Vec<String> = self
            .conversation
            .shells()
            .rows(Instant::now())
            .iter()
            .filter(|s| s.running())
            .map(|s| s.id.clone())
            .collect();
        self.shell_marks.retain(|id, _| running.contains(id));
        for id in running {
            let clock = self.clock.clone();
            self.shell_marks.entry(id).or_insert_with(|| {
                cx.new(|cx| MarkView::new(&clock, Status::Working, px(MARK), BLACK, cx))
            });
        }
    }

    /// The popover (pointing at the tab, as the plan's), in the popovers' one look (chore 36):
    /// "Shells" and "1 of 3 done · 2 running", a row per shell, and "Click a shell to follow
    /// its output".
    pub(crate) fn shells_popover(&self, c: &Colors, cx: &mut Context<Self>) -> AnyElement {
        let ui = cx.global::<Appearance>().theme.ui();
        let now = Instant::now();
        let shells = self.conversation.shells();
        let rows: Vec<AnyElement> = shells
            .rows(now)
            .into_iter()
            .enumerate()
            .map(|(i, shell)| self.shell_row(i, shell, now, c, cx))
            .collect();
        let mut content = vec![
            popover::header("Shells", Some(popover::count(shells.header())), &ui)
                .into_any_element(),
        ];
        if rows.is_empty() {
            content.push(popover::quiet("All finished", &ui).into_any_element());
        }
        content.extend(rows);
        content.push(popover::quiet("Click a shell to follow its output", &ui).into_any_element());
        // Hung inside the corner tab's edge (`corner_tabs`): out by that edge to the box's.
        self.popover_frame(
            "shells-popover",
            content,
            SHELLS_WIDTH,
            MenuPlace::Right(crate::card_view::EDGE),
            cx,
        )
    }

    /// One shell's row: its mark, description and time, ■ while it runs; under them its
    /// command and its last output line. Finished, it dims.
    fn shell_row(
        &self,
        i: usize,
        shell: &Shell,
        now: Instant,
        c: &Colors,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let ui = cx.global::<Appearance>().theme.ui();
        let mono = cx.global::<Appearance>().font.family.clone();
        let running = shell.running();
        let (title_ink, dim) = if running {
            (ui.text, ui.text_dim)
        } else {
            (ui.text_dim, ui.text_dim)
        };
        let mark = match (shell.ended, self.shell_marks.get(&shell.id)) {
            (None, Some(arc)) => arc.clone().into_any_element(),
            (None, None) => popover::mark(Mark::Building, c, &ui),
            (Some((ShellEnd::Done, _)), _) => popover::mark(Mark::Done, c, &ui),
            (Some((ShellEnd::Failed, _)), _) => popover::mark(Mark::Failed, c, &ui),
            (Some((ShellEnd::Stopped, _)), _) => popover::mark(Mark::Stopped, c, &ui),
        };
        let small = |text: String| {
            popover::mono(text, dim, mono.clone())
                .pl(px(popover::PAD_X + popover::INDENT))
                .pr(px(popover::PAD_X))
                .text_size(px(11.))
        };
        let id = shell.id.clone();
        let stop = running.then(|| {
            let id = id.clone();
            let el = div()
                .id(("shell-stop", i))
                .flex_none()
                .size(px(18.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(4.))
                .cursor_pointer()
                .hover(move |s| s.bg(hsla(ui.row_hover)))
                .child(div().size(px(8.)).rounded(px(1.5)).bg(hsla(ui.text_dim)))
                .tooltip(|_, cx| {
                    cx.new(|_| crate::sidebar_toggle::Tip("Stop this shell".into()))
                        .into()
                })
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.stop_shell(&id, cx);
                    }),
                );
            if i == 0 {
                self.marked("shell-stop", el).into_any_element()
            } else {
                el.into_any_element()
            }
        });
        let openable = shell.output.is_some();
        // Two lines under the first (its command, its last output line): the row's own height.
        let row = div()
            .id(("shell-row", i))
            .flex()
            .flex_col()
            .gap(px(2.))
            .py(px(5.))
            .rounded(px(6.))
            .when(openable, |d| {
                d.cursor_pointer()
                    .hover(move |s| s.bg(hsla(ui.row_hover)))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.open_shell_output(&id, cx);
                        }),
                    )
            })
            .child(
                popover::line()
                    .child(popover::mark_cell(mark))
                    .child(
                        popover::title(shell.description.clone(), title_ink)
                            .font_weight(FontWeight::MEDIUM),
                    )
                    .child(popover::tail(
                        crate::agent_lanes::clock(shell.elapsed(now)),
                        dim,
                    ))
                    .children(stop),
            )
            .child(small(shell.command.clone()))
            .children(shell.last_line.clone().map(small));
        if i == 0 {
            self.marked("shell-row", row).into_any_element()
        } else {
            row.into_any_element()
        }
    }
}
