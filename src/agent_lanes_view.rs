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

impl AgentTab {
    /// `Event::Follow`: read `path`'s new lines as lane `lane`'s (`agent_lanes::follow_every`:
    /// often while it works, rarely once done), off the UI thread, through the translator. It lives as long as the process (a woken subagent
    /// writes again after its report); a file that shrinks ends it.
    pub(super) fn follow(&mut self, lane: String, path: PathBuf, cx: &mut Context<Self>) {
        let task = cx.spawn({
            let lane = lane.clone();
            async move |this, cx| {
                let mut tail = crate::lane_follow::Tail::default();
                loop {
                    let Ok(every) = this.update(cx, |this, _| {
                        crate::agent_lanes::follow_every(this.conversation.lane(&lane))
                    }) else {
                        break;
                    };
                    cx.background_executor().timer(every).await;
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
            self.update_placeholder(cx);
        }
        cx.notify();
    }

    /// Stop lane `id` alone (its agent's own stop: `Action::StopSubagent`).
    pub(crate) fn stop_lane(&mut self, id: &str, cx: &mut Context<Self>) {
        self.write(&Action::StopSubagent {
            lane: id.to_string(),
        });
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
        let back_to_main =
            matches!(&self.view, View::Lane(id) if !lanes.iter().any(|l| &l.id == id));
        // Lists of lanes that went (`/clear`, a new conversation) go too.
        self.panes.retain(|view, _| match view {
            View::Main => true,
            View::Lane(id) => lanes.iter().any(|l| &l.id == id),
        });
        let live: Vec<(String, Status)> = lanes
            .iter()
            .filter_map(|l| match l.status {
                LaneStatus::Working => Some((l.id.clone(), Status::Working)),
                LaneStatus::Waiting => Some((l.id.clone(), Status::Waiting)),
                _ => None,
            })
            .collect();
        let empty = lanes.is_empty();
        crate::agent_lanes::stamp_ends(&mut self.lane_times, lanes, Instant::now());
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
        if back_to_main {
            // Its box talks to Main again, and says so.
            self.view = View::Main;
            self.update_placeholder(cx);
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
        // Main, then the lanes newest first: strip place `n` (1…) is lane `order[n - 1]`.
        let order = crate::agent_lanes::shown_order(lanes);
        let mut widths = vec![pill_w("Main")];
        widths.extend(order.iter().map(|&i| pill_w(&labels[i])));
        let avail = (width - 2.0 * AREA_X).min(crate::ui_tokens::chat::COLUMN_MAX);
        let active = match &self.view {
            View::Main => 0,
            View::Lane(id) => order
                .iter()
                .position(|&i| &lanes[i].id == id)
                .map_or(0, |p| p + 1),
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
            let lane = &lanes[order[n - 1]];
            pills.push(pill(
                n,
                self.lane_mark(lane, &c),
                labels[order[n - 1]].clone(),
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
                        let lane = &lanes[order[n - 1]];
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
                                    .child(SharedString::from(labels[order[n - 1]].clone())),
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

    /// The shown lane, if the view is one.
    pub(crate) fn shown_lane(&self) -> Option<&Lane> {
        match &self.view {
            View::Lane(id) => self.conversation.lane(id),
            View::Main => None,
        }
    }

    /// Back to Main, its list brought to the lane's row ("Back to Main ›").
    pub(crate) fn back_to_main(&mut self, lane: &str, cx: &mut Context<Self>) {
        self.show_view(View::Main, cx);
        let items = self.conversation.items();
        let pane = self.pane();
        if let Some(k) = pane
            .order
            .iter()
            .position(|&i| matches!(&items[i], Item::Tool { id, .. } if id == lane))
        {
            pane.list.scroll_to_reveal_item(k);
        }
    }

    /// A lane's header (the Lead UI/UX Designer's spec), over its list: its name, its kind and
    /// model, and on the right its time, tokens and tools as far as the agent says (a narrow tab
    /// keeps the time).
    pub(super) fn lane_header(&self, look: &Look, _cx: &mut Context<Self>) -> Option<AnyElement> {
        let lane = self.shown_lane()?;
        let c = look.c;
        let narrow = self.bar_width.get() < NARROW;
        let kind = [Some(lane.kind.clone()), lane.model.clone()]
            .into_iter()
            .flatten()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" · ");
        let stats = [
            self.lane_elapsed(&lane.id).map(crate::agent_lanes::clock),
            lane.tokens
                .filter(|_| !narrow)
                .map(|t| format!("{:.1}k tok", t as f32 / 1000.0)),
            lane.tools.filter(|_| !narrow).map(|n| match n {
                1 => "1 tool".to_string(),
                n => format!("{n} tools"),
            }),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" · ");
        Some(
            div()
                .flex_none()
                .w_full()
                .px(px(AREA_X))
                .pb(px(4.))
                .flex()
                .justify_center()
                .child(
                    self.marked(
                        "lane-header",
                        div()
                            .w_full()
                            .max_w(px(crate::ui_tokens::chat::COLUMN_MAX))
                            .h(px(crate::card_view::PILL_H))
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .text_size(px(text::META))
                            .child(
                                div()
                                    .flex_none()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(hsla(c.text))
                                    .child(SharedString::from(lane.label.clone())),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.))
                                    .truncate()
                                    .text_color(hsla(c.dim))
                                    .child(SharedString::from(kind)),
                            )
                            .child(
                                div()
                                    .flex_none()
                                    .font_features(crate::sidebar::tabular())
                                    .text_color(hsla(c.panel_dim))
                                    .child(SharedString::from(stats)),
                            )
                            .when(crate::agent_lanes::shows_stop(lane), |d| {
                                let view = look.view.clone();
                                let id = lane.id.clone();
                                d.child(self.marked(
                                    "lane-stop",
                                    div().child(button("lane-stop", "Stop", false, &c).on_click(
                                        on_tab(&view, move |this, cx| this.stop_lane(&id, cx)),
                                    )),
                                ))
                            }),
                    ),
                )
                .into_any_element(),
        )
    }

    /// A lane's first row: what Main asked it, as your bubble is drawn, captioned "from Main".
    pub(super) fn lane_head_row(&self, look: &Look) -> AnyElement {
        let Some(lane) = self.shown_lane() else {
            return div().into_any_element();
        };
        let c = look.c;
        let task = lane.task.trim().to_string();
        div()
            .w_full()
            .px(px(AREA_X))
            .pt(px(crate::ui_tokens::chat::FIRST_TOP))
            .flex()
            .flex_col()
            .items_center()
            .child(
                self.marked(
                    "lane-task",
                    div()
                        .w_full()
                        .max_w(px(crate::ui_tokens::chat::COLUMN_MAX))
                        .flex()
                        .flex_col()
                        .items_end()
                        .gap(px(4.))
                        .child(
                            div()
                                .text_size(px(text::META))
                                .text_color(hsla(c.dim))
                                .child("from Main"),
                        )
                        // Lit a moment when the strip jumps to it (chore after M11ar).
                        .child(
                            self.flashed(
                                div()
                                    .max_w(px(520.))
                                    .px(px(14.))
                                    .py(px(9.))
                                    .rounded(px(crate::ui_tokens::chat::R_BUBBLE))
                                    .rounded_tr(px(crate::ui_tokens::chat::R_TAIL))
                                    .bg(hsla(c.you_fill))
                                    .border_1()
                                    .border_color(hsla(c.you_edge))
                                    .text_size(px(crate::ui_tokens::chat::BODY))
                                    .line_height(relative(crate::ui_tokens::chat::BODY_LINE))
                                    .text_color(hsla(c.you_text))
                                    .child(SharedString::from(task)),
                                0,
                                &c,
                            ),
                        ),
                ),
            )
            .into_any_element()
    }

    /// A lane's last row once it ended: "✓ Done in 1:42 · Back to Main ›", "✕ Failed · why",
    /// "Stopped".
    pub(super) fn lane_end_row(&self, look: &Look) -> AnyElement {
        let Some(lane) = self.shown_lane() else {
            return div().into_any_element();
        };
        let c = look.c;
        let (word, ink): (String, Rgb) = match &lane.status {
            // "✓ Done in 1:42 · 18.2k tokens", each part when known.
            LaneStatus::Done => (
                [
                    Some(match self.lane_elapsed(&lane.id) {
                        Some(d) => format!("\u{2713} Done in {}", crate::agent_lanes::clock(d)),
                        None => "\u{2713} Done".into(),
                    }),
                    lane.tokens
                        .map(|t| format!("{:.1}k tokens", t as f32 / 1000.0)),
                ]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" \u{b7} "),
                c.tool_ok,
            ),
            LaneStatus::Failed(why) => (format!("\u{2715} Failed \u{b7} {why}"), c.failed_panel),
            _ => ("Stopped".into(), c.panel_dim),
        };
        let view = look.view.clone();
        let id = lane.id.clone();
        div()
            .w_full()
            .px(px(AREA_X))
            .pt(px(crate::ui_tokens::chat::GAP_NOTICE))
            .pb(px(crate::ui_tokens::chat::LAST_BOTTOM))
            .flex()
            .flex_col()
            .items_center()
            .child(
                self.marked(
                    "lane-end",
                    div()
                        .w_full()
                        .max_w(px(crate::ui_tokens::chat::COLUMN_MAX))
                        .pl(px(
                            crate::ui_tokens::chat::AVATAR + crate::ui_tokens::chat::AVATAR_GAP
                        ))
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .text_size(px(text::META))
                        .child(div().text_color(hsla(ink)).child(SharedString::from(word)))
                        .child(div().text_color(hsla(c.panel_dim)).child("\u{b7}"))
                        .child(
                            div()
                                .id("lane-back")
                                .cursor_pointer()
                                .text_color(hsla(c.dim))
                                .hover(move |s| s.text_color(hsla(c.text)))
                                .child("Back to Main \u{203a}")
                                .on_click(on_tab(&view, move |this, cx| {
                                    this.back_to_main(&id, cx)
                                })),
                        ),
                ),
            )
            .into_any_element()
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

    /// Scenes: whether `part` has been drawn (since `shot_forget`).
    #[cfg(feature = "ui-shots")]
    pub fn shot_part_now(&self, part: &str) -> bool {
        self.shot_part(part).is_some()
    }

    /// Scenes: forget where `parts` were drawn (the next frame says again).
    #[cfg(feature = "ui-shots")]
    pub fn shot_forget(&self, parts: &[&str]) {
        self.parts.borrow_mut().retain(|k, _| !parts.contains(k));
    }

    /// Scenes: a followed file's lines for `lane`, into the same path the follower takes.
    #[cfg(feature = "ui-shots")]
    pub fn shot_followed(&mut self, lane: &str, lines: Vec<String>, cx: &mut Context<Self>) {
        self.on_followed(lane, lines, cx);
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
