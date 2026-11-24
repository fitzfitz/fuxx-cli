//! A subagent's row in the chat (the subagent row chore, 2026-11-24, the Lead UI/UX
//! Designer's spec; your words: "Make it different if its a subagent so that I knew its a
//! subagent running"): the state's mark, an outlined robot (`Icon::Bot`), its name, its task and
//! what it is doing, in the tool group's shell (`tool_shell`). The robot also heads the pill
//! strip's subagents and a lane's header (`robot`).

use super::*;
use crate::agent_conversation::{Lane, LaneStatus};
use crate::agent_lanes::{Tone, View};
use rows::{right_words, status_slot, tool_chevron};

/// The robot: a 14 pt glyph in a 14 pt box.
pub(super) const BOT: f32 = 14.0;
/// The gap between a row's robot and its name.
const BOT_GAP: f32 = 6.0;

/// The robot in `lane_ink` while `live`, else in `off` (it never animates).
pub(super) fn robot(live: bool, c: &Colors, off: Rgb) -> gpui::Div {
    let ink = if live { c.lane_ink } else { off };
    div()
        .flex_none()
        .size(px(BOT))
        .flex()
        .items_center()
        .justify_center()
        .child(icon(Icon::Bot, px(BOT), hsla(ink)))
}

impl AgentTab {
    /// Subagent `lane`'s row (item `ix`, its tool call): `[mark] [robot] name task … words ›`,
    /// tinted while it waits on you; a click opens its lane, and its tooltip says what it is.
    pub(super) fn lane_row(&self, k: usize, ix: usize, lane: &Lane, look: &Look) -> AnyElement {
        use crate::agent_chat::summary_shown;
        let c = &look.c;
        let waiting = lane.status == LaneStatus::Waiting;
        let (fill, hover, name_ink, task_ink, quiet, lit) = if waiting {
            (
                c.ask_fill,
                c.ask_cursor,
                c.ask_text,
                c.ask_dim,
                c.ask_dim,
                c.ask_text,
            )
        } else {
            (
                c.panel,
                c.row_hover,
                c.panel_text,
                c.panel_dim,
                c.panel_dim,
                c.panel_text,
            )
        };
        let (words, tone) = self.lane_words(lane);
        let words_ink = match tone {
            Tone::Dim => quiet,
            Tone::Ask => c.ask_head,
            Tone::Failed => c.failed_panel,
        };
        let tip = self.lane_tip(lane);
        let task = lane.task.lines().next().unwrap_or("").to_string();
        let view = look.view.clone();
        let to = View::Lane(lane.id.clone());
        let row = self
            .tool_shell(k, ix, fill, Some(hover), look)
            .child(self.marked("lane-row-mark", status_slot(self.lane_mark(lane, c))))
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(BOT_GAP))
                    .child(self.marked(
                        parts_of(&lane.status).1,
                        robot(lane.status.live(), c, c.panel_dim),
                    ))
                    .child(
                        self.marked(
                            "lane-row-name",
                            div()
                                .flex_none()
                                .text_color(hsla(name_ink))
                                .font_weight(FontWeight::MEDIUM)
                                .child(SharedString::from(lane.label.clone())),
                        ),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .truncate()
                    .text_color(hsla(task_ink))
                    .when(summary_shown(&lane.label, &task), |d| {
                        d.child(SharedString::from(task.clone()))
                    }),
            )
            .child(right_words(words, words_ink))
            .child(tool_chevron(true, quiet, lit))
            .tooltip(move |_, cx| -> gpui::AnyView {
                cx.new(|_| crate::sidebar_toggle::Tip(tip.clone().into()))
                    .into()
            })
            .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                cx.stop_propagation();
                let to = to.clone();
                view.update(cx, |this, cx| this.show_view(to, cx)).ok();
            });
        self.marked(parts_of(&lane.status).0, row)
            .into_any_element()
    }

    /// Subagent `lane`'s row's right-side words and their tone (`agent_lanes::lane_row_words`).
    pub(crate) fn lane_words(&self, lane: &Lane) -> (String, Tone) {
        crate::agent_lanes::lane_row_words(
            &lane.status,
            lane.last_tool.as_deref(),
            self.conversation.lane_steps(&lane.id),
            self.lane_elapsed(&lane.id),
            self.bar_width.get() < lanes_view::NARROW,
        )
    }

    /// Subagent `lane`'s row's tooltip (`agent_lanes::lane_row_tip`; the provider says whether
    /// its kind names anything).
    pub(crate) fn lane_tip(&self, lane: &Lane) -> String {
        let relays = self.provider.is_some_and(|p| p.relays_to_subagents());
        let kind = Some(lane.kind.as_str())
            .filter(|k| !self.provider.is_some_and(|p| p.generic_lane_kind(k)));
        crate::agent_lanes::lane_row_tip(lane, self.lane_elapsed(&lane.id), relays, kind)
    }
}

/// Scenes: a lane row's parts (the row, its robot) by its state, one of each per scene.
fn parts_of(status: &LaneStatus) -> (&'static str, &'static str) {
    match status {
        LaneStatus::Working => ("lane-row-working", "lane-bot-working"),
        LaneStatus::Waiting => ("lane-row-waiting", "lane-bot-waiting"),
        LaneStatus::Done => ("lane-row-done", "lane-bot-done"),
        LaneStatus::Failed(_) => ("lane-row-failed", "lane-bot-failed"),
        LaneStatus::Stopped => ("lane-row-stopped", "lane-bot-stopped"),
        LaneStatus::Ended => ("lane-row-ended", "lane-bot-ended"),
    }
}
