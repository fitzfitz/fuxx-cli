//! The agent tab's subagents (M11ar): following a subagent's transcript (agy writes each
//! subagent's steps to its own file), the pill strip that switches the chat between Main and
//! each subagent (a lane), and the keys that step through them.

use super::*;
use crate::agent_conversation::{Lane, LaneStatus};
use crate::agent_lanes::View;

/// The strip's height (the Lead UI/UX Designer's spec).
const STRIP_H: f32 = 32.0;
/// A tab narrower than this cuts labels shorter.
const NARROW: f32 = 480.0;

/// How often a followed transcript is read.
const FOLLOW_EVERY: Duration = Duration::from_millis(500);

impl AgentTab {
    /// `Event::Follow`: read `path`'s new lines as lane `lane`'s, every `FOLLOW_EVERY`, off the
    /// UI thread, through the translator. It lives as long as the process (a woken subagent
    /// writes again after its report); a file that shrinks ends it.
    pub(super) fn follow(&mut self, lane: String, path: PathBuf, cx: &mut Context<Self>) {
        let task = cx.spawn({
            let lane = lane.clone();
            async move |this, cx| {
                let mut tail = crate::lane_follow::Tail::default();
                loop {
                    cx.background_executor().timer(FOLLOW_EVERY).await;
                    let (at, file) = (tail.offset, path.clone());
                    let read = cx
                        .background_executor()
                        .spawn(async move {
                            crate::lane_follow::read_new(&file, at, crate::lane_follow::MAX_READ)
                        })
                        .await;
                    let Ok(bytes) = read else { break };
                    let lines = tail.feed(&bytes);
                    if lines.is_empty() {
                        continue;
                    }
                    let alive = this
                        .update(cx, |this, cx| this.on_followed(&lane, lines, cx))
                        .is_ok();
                    if !alive {
                        break;
                    }
                }
            }
        });
        self.followers.insert(lane, task);
    }

    /// A followed file's new lines for `lane`, through the translator, as events.
    pub(super) fn on_followed(&mut self, lane: &str, lines: Vec<String>, cx: &mut Context<Self>) {
        let Some(translator) = self.translator.as_mut() else {
            return;
        };
        let events: Vec<Event> = lines
            .iter()
            .flat_map(|l| translator.decode_followed(lane, l))
            .collect();
        if !events.is_empty() {
            self.on_events(events, cx);
        }
    }

    /// Show `view` (Main or a lane): its own list, kept as it was.
    pub(crate) fn show_view(&mut self, view: View, cx: &mut Context<Self>) {
        self.lanes_menu = false;
        if self.view != view {
            self.view = view;
            self.sync_rows();
        }
        cx.notify();
    }

    pub(super) fn prev_lane(&mut self, _: &PrevLane, _: &mut Window, cx: &mut Context<Self>) {
        let to = crate::agent_lanes::step(&self.view, self.conversation.lanes(), -1);
        self.show_view(to, cx);
    }

    pub(super) fn next_lane(&mut self, _: &NextLane, _: &mut Window, cx: &mut Context<Self>) {
        let to = crate::agent_lanes::step(&self.view, self.conversation.lanes(), 1);
        self.show_view(to, cx);
    }

    /// A mark for each live lane (Working or Waiting) and Main's; a lane that went takes its
    /// view back to Main.
    pub(super) fn sync_lane_marks(&mut self, cx: &mut Context<Self>) {
        let lanes = self.conversation.lanes();
        if let View::Lane(id) = &self.view
            && !lanes.iter().any(|l| &l.id == id)
        {
            self.view = View::Main;
        }
        let live: Vec<(String, Status)> = lanes
            .iter()
            .filter_map(|l| match l.status {
                LaneStatus::Working => Some((l.id.clone(), Status::Working)),
                LaneStatus::Waiting => Some((l.id.clone(), Status::Waiting)),
                _ => None,
            })
            .collect();
        let empty = lanes.is_empty();
        self.lane_marks
            .retain(|id, _| live.iter().any(|(l, _)| l == id));
        for (id, status) in live {
            let clock = self.clock.clone();
            let mark = self
                .lane_marks
                .entry(id)
                .or_insert_with(|| cx.new(|cx| MarkView::new(&clock, status, px(MARK), BLACK, cx)));
            mark.update(cx, |m, _| m.set(status, px(MARK), BLACK));
        }
        if empty {
            self.main_mark = None;
        } else if self.main_mark.is_none() {
            let clock = self.clock.clone();
            let status = self.status();
            self.main_mark = Some(cx.new(|cx| MarkView::new(&clock, status, px(MARK), BLACK, cx)));
        }
    }

    /// How long lane `id` has run (stopped at its end); none when unknown (a resumed lane).
    pub(crate) fn lane_elapsed(&self, id: &str) -> Option<Duration> {
        let (start, end) = match self.lane_times.get(id) {
            Some(&(start, end)) => (Some(start), end),
            None => (None, None),
        };
        crate::agent_lanes::elapsed(start, end, Instant::now())
    }

    /// A lane's status mark: the live arc, a done check, a failed dot, a stopped ring.
    pub(crate) fn lane_mark(&self, lane: &Lane, c: &Colors) -> AnyElement {
        let dot = |fill: Rgb, hollow: bool| {
            div()
                .size(px(crate::ui_tokens::chat::TOOL_DOT))
                .rounded_full()
                .when(!hollow, |d| d.bg(hsla(fill)))
                .when(hollow, |d| d.border_1().border_color(hsla(fill)))
                .into_any_element()
        };
        let mark = match &lane.status {
            LaneStatus::Working | LaneStatus::Waiting => match self.lane_marks.get(&lane.id) {
                Some(mark) => mark.clone().into_any_element(),
                None => dot(c.todo_active, false),
            },
            LaneStatus::Done => {
                crate::status_mark::done_check(c.tool_ok, px(MARK)).into_any_element()
            }
            LaneStatus::Failed(_) => dot(c.failed_panel, false),
            LaneStatus::Stopped => dot(c.panel_dim, true),
        };
        mark_slot(mark)
    }

    /// Main's pill's mark: the agent's own status.
    fn main_pill_mark(&self, c: &Colors, cx: &mut Context<Self>) -> AnyElement {
        let status = self.status();
        let mark = match (&self.main_mark, status) {
            (Some(mark), Status::Working | Status::Waiting) => {
                mark.update(cx, |m, _| m.set(status, px(MARK), BLACK));
                mark.clone().into_any_element()
            }
            (_, Status::Done) => {
                crate::status_mark::done_check(c.tool_ok, px(MARK)).into_any_element()
            }
            _ => div()
                .size(px(crate::ui_tokens::chat::TOOL_DOT))
                .rounded_full()
                .border_1()
                .border_color(hsla(c.panel_dim))
                .into_any_element(),
        };
        mark_slot(mark)
    }

    /// The pill strip (the Lead UI/UX Designer's spec): under the top bar, in the centred
    /// column, once a subagent exists — Main first, then each lane in start order; the shown one
    /// filled; one waiting on you tinted; the ones that don't fit behind "+N".
    pub(super) fn lane_strip(&self, look: &Look, cx: &mut Context<Self>) -> Option<AnyElement> {
        let lanes = self.conversation.lanes();
        if lanes.is_empty() {
            return None;
        }
        let c = look.c;
        let width = self.bar_width.get();
        let max = if width < NARROW { 12 } else { 20 };
        let labels = crate::agent_lanes::pill_labels(lanes, max);
        // Pill widths, estimated: a mark, the label in the meta size, the padding.
        let pill_w = |label: &str| MARK + 5.0 + label.chars().count() as f32 * 6.6 + 16.0;
        let mut widths = vec![pill_w("Main")];
        widths.extend(labels.iter().map(|l| pill_w(l)));
        let avail = (width - 2.0 * AREA_X).min(crate::ui_tokens::chat::COLUMN_MAX);
        let active = match &self.view {
            View::Main => 0,
            View::Lane(id) => lanes.iter().position(|l| &l.id == id).map_or(0, |i| i + 1),
        };
        let (shown, hidden) = crate::agent_lanes::fit(&widths, avail, active);
        let neutral = mix(c.page, c.text, 0.06);
        let view = look.view.clone();
        let pill =
            |n: usize, mark: AnyElement, label: String, to: View, waiting: bool, tip: String| {
                let on = n == active;
                let el = div()
                    .id(("lane-pill", n))
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(px(5.))
                    .h(px(crate::card_view::PILL_H))
                    .px(px(8.))
                    .rounded(px(6.))
                    .text_size(px(text::META))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(hsla(if on { c.text } else { c.dim }))
                    .when(waiting, |d| d.bg(hsla(c.ask_fill)))
                    .when(on && !waiting, |d| d.bg(hsla(neutral)))
                    .when(!on, |d| {
                        d.cursor_pointer().hover(move |s| s.bg(hsla(c.panel_hover)))
                    })
                    .on_click(on_tab(&view, move |this, cx| {
                        this.show_view(to.clone(), cx)
                    }))
                    .child(mark)
                    .child(SharedString::from(label));
                if tip.is_empty() {
                    el.into_any_element()
                } else {
                    el.tooltip(move |_, cx| -> gpui::AnyView {
                        cx.new(|_| crate::sidebar_toggle::Tip(tip.clone().into()))
                            .into()
                    })
                    .into_any_element()
                }
            };
        let mut pills: Vec<AnyElement> = Vec::new();
        for &n in &shown {
            if n == 0 {
                let mark = self.main_pill_mark(&c, cx);
                pills.push(pill(
                    0,
                    mark,
                    "Main".into(),
                    View::Main,
                    false,
                    String::new(),
                ));
                continue;
            }
            let lane = &lanes[n - 1];
            pills.push(pill(
                n,
                self.lane_mark(lane, &c),
                labels[n - 1].clone(),
                View::Lane(lane.id.clone()),
                lane.status == LaneStatus::Waiting,
                self.lane_tip(lane),
            ));
        }
        let more = (hidden > 0).then(|| {
            let menu = self.lanes_menu.then(|| {
                let rows: Vec<AnyElement> = (1..widths.len())
                    .filter(|n| !shown.contains(n))
                    .map(|n| {
                        let lane = &lanes[n - 1];
                        let to = View::Lane(lane.id.clone());
                        let time = self.lane_elapsed(&lane.id).map(crate::agent_lanes::clock);
                        div()
                            .id(("lane-menu-row", n))
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .px(px(8.))
                            .h(px(26.))
                            .rounded(px(6.))
                            .cursor_pointer()
                            .hover(move |s| s.bg(hsla(c.panel_hover)))
                            .on_click(on_tab(&view, move |this, cx| {
                                this.show_view(to.clone(), cx)
                            }))
                            .child(self.lane_mark(lane, &c))
                            .child(
                                div()
                                    .flex_1()
                                    .child(SharedString::from(labels[n - 1].clone())),
                            )
                            .children(time.map(|t| {
                                div()
                                    .text_color(hsla(c.panel_dim))
                                    .child(SharedString::from(t))
                            }))
                            .into_any_element()
                    })
                    .collect();
                gpui::deferred(
                    gpui::anchored().snap_to_window_with_margin(px(8.)).child(
                        div()
                            .mt(px(4.))
                            .min_w(px(200.))
                            .p(px(4.))
                            .rounded(px(8.))
                            .bg(hsla(c.panel))
                            .border_1()
                            .border_color(hsla(c.panel_border))
                            .text_size(px(text::META))
                            .text_color(hsla(c.panel_text))
                            .occlude()
                            .child(self.marked("lanes-menu", div().children(rows))),
                    ),
                )
            });
            div()
                .flex_none()
                .child(
                    div()
                        .id("lane-more")
                        .flex()
                        .items_center()
                        .h(px(crate::card_view::PILL_H))
                        .px(px(8.))
                        .rounded(px(6.))
                        .text_size(px(text::META))
                        .text_color(hsla(c.dim))
                        .cursor_pointer()
                        .hover(move |s| s.bg(hsla(c.panel_hover)))
                        .on_click(on_tab(&view, |this, cx| {
                            this.lanes_menu = !this.lanes_menu;
                            cx.notify();
                        }))
                        .child(SharedString::from(format!("+{hidden}"))),
                )
                .children(menu)
        });
        let more = more.map(|m| self.marked("more", m));
        Some(
            div()
                .flex_none()
                .w_full()
                .h(px(STRIP_H))
                .px(px(AREA_X))
                .flex()
                .justify_center()
                .child(
                    self.marked(
                        "strip",
                        div()
                            .w_full()
                            .max_w(px(crate::ui_tokens::chat::COLUMN_MAX))
                            .flex()
                            .items_center()
                            .gap(px(crate::agent_lanes::PILL_GAP))
                            .children(pills)
                            .children(more),
                    ),
                )
                .into_any_element(),
        )
    }

    /// Scenes: the lanes, and a lane's step count.
    #[cfg(feature = "ui-shots")]
    pub fn shot_lanes(&self) -> &[Lane] {
        self.conversation.lanes()
    }

    #[cfg(feature = "ui-shots")]
    pub fn shot_lane_steps(&self, id: &str) -> usize {
        self.conversation.lane_steps(id)
    }

    /// Scenes: open or close the "+N" list.
    #[cfg(feature = "ui-shots")]
    pub fn shot_toggle_lanes_menu(&mut self, cx: &mut Context<Self>) {
        self.lanes_menu = !self.lanes_menu;
        cx.notify();
    }

    /// A lane pill's tooltip: its kind, its task's first line, its time.
    fn lane_tip(&self, lane: &Lane) -> String {
        let time = self.lane_elapsed(&lane.id).map(crate::agent_lanes::clock);
        [
            Some(lane.kind.clone()),
            lane.task.lines().next().map(str::to_string),
            time,
        ]
        .into_iter()
        .flatten()
        .filter(|s| !s.trim().is_empty())
        .collect::<Vec<_>>()
        .join(" · ")
    }
}

/// A mark in the tool row's 12 pt slot.
fn mark_slot(mark: AnyElement) -> AnyElement {
    div()
        .flex_none()
        .size(px(MARK))
        .flex()
        .items_center()
        .justify_center()
        .child(mark)
        .into_any_element()
}
