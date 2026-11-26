//! The subagents tab and its popover (chore 19, note 93): `🤖 4/6` in the corner tab beside the
//! shells and plan tabs, and over the chat a row per subagent — its mark (the arc while it
//! runs), its name and what it is doing; one that needs you in red. A row opens that
//! subagent's view (one that needs you, with the keys on its question). The words are
//! `agent_subagents`'.

use gpui::{
    AnyElement, Context, FontWeight, InteractiveElement, IntoElement, MouseButton, ParentElement,
    SharedString, StatefulInteractiveElement, Styled, div, prelude::FluentBuilder, px,
};

use super::{AgentTab, Colors};
use crate::agent_box::BoxMenu;
use crate::agent_box::menus::MenuPlace;
use crate::agent_conversation::{Lane, LaneStatus};
use crate::agent_lanes::View;
use crate::agent_subagents::{TabTone, header, order, row_words, tab_tone, tab_words};
use crate::appearance::Appearance;
use crate::terminal_view::hsla;

/// A row's height, and how many show before the list scrolls.
const ROW_H: f32 = 26.0;
const MAX_ROWS: usize = 10;

impl AgentTab {
    /// The subagents tab's pill (`🤖 1/2`): peach while one runs, green once all finished, red
    /// while one needs you; none before a subagent started.
    pub(crate) fn subagents_tab(&self, c: &Colors, cx: &mut Context<Self>) -> Option<AnyElement> {
        let lanes = self.conversation.lanes();
        if lanes.is_empty() {
            return None;
        }
        let ink = match tab_tone(lanes) {
            TabTone::Running => c.todo_active,
            TabTone::Done => c.tool_ok,
            TabTone::NeedsYou => c.failed_panel,
        };
        Some(self.corner_pill(
            "agent-subagents-tab",
            "subagents-tab",
            crate::icons::Icon::Bot,
            tab_words(lanes),
            ink,
            BoxMenu::Subagents,
            "Subagents  \u{2325}\u{2318}[ \u{2325}\u{2318}]",
            |this, _, cx| this.toggle_subagents(cx),
            c,
            cx,
        ))
    }

    /// The tab's click: the popover opens, or closes.
    pub(crate) fn toggle_subagents(&mut self, cx: &mut Context<Self>) {
        if matches!(self.menu, Some(BoxMenu::Subagents)) {
            self.close_menu(cx);
        } else if !self.conversation.lanes().is_empty() {
            self.open_menu(BoxMenu::Subagents, cx);
        }
    }

    /// A row's click: the popover closes and subagent `id`'s view opens; one waiting on you
    /// with the keys on its question (it shows in every view).
    fn open_subagent(&mut self, id: &str, cx: &mut Context<Self>) {
        let waiting = self
            .conversation
            .lane(id)
            .is_some_and(|l| l.status == LaneStatus::Waiting);
        self.close_menu(cx);
        self.show_view(View::Lane(id.to_string()), cx);
        if waiting {
            self.land_on_waiting(cx);
        }
    }

    /// The popover (pointing at the tab, as the plan's): "1 of 2 done · 1 running", a row per
    /// subagent, and "Click a subagent to open it".
    pub(crate) fn subagents_popover(&self, c: &Colors, cx: &mut Context<Self>) -> AnyElement {
        let ui = cx.global::<Appearance>().theme.ui();
        let lanes = self.conversation.lanes();
        let rows: Vec<AnyElement> = order(lanes)
            .into_iter()
            .map(|i| self.subagent_row(i, &lanes[i], c, cx))
            .collect();
        let quiet = |words: SharedString, top: f32, bottom: f32| {
            div()
                .px(px(8.))
                .pt(px(top))
                .pb(px(bottom))
                .text_size(px(11.5))
                .text_color(hsla(ui.text_dim))
                .child(words)
        };
        // Many subagents scroll inside the popover (it never runs off the window).
        let list = div()
            .id("subagent-rows")
            .max_h(px(MAX_ROWS as f32 * ROW_H))
            .overflow_y_scroll()
            .children(rows);
        let content = div()
            .flex()
            .flex_col()
            .text_size(px(12.5))
            .child(quiet(header(lanes).into(), 6., 2.))
            .child(self.marked("subagent-rows", list))
            .child(quiet("Click a subagent to open it".into(), 6., 6.));
        let content = self
            .marked("subagents-popover", div().child(content))
            .into_any_element();
        self.menu_frame_at(
            Some(content),
            None,
            crate::agent_box::PLAN_WIDTH,
            MenuPlace::Right(crate::card_view::EDGE),
            cx,
        )
    }

    /// One subagent's row: its mark, its name, what it is doing (or how it ended) and its time;
    /// "Needs you" in red while it waits on you. Finished, it dims.
    fn subagent_row(
        &self,
        i: usize,
        lane: &Lane,
        c: &Colors,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let ui = cx.global::<Appearance>().theme.ui();
        let waiting = lane.status == LaneStatus::Waiting;
        let (words, _) = self.lane_words(lane);
        let words = row_words(&words, &lane.status);
        let name_ink = if lane.status.live() {
            ui.text
        } else {
            ui.text_dim
        };
        let words_el = div()
            .flex_none()
            .font_features(crate::sidebar::tabular())
            .text_size(px(11.))
            .text_color(hsla(if waiting { c.failed_panel } else { ui.text_dim }))
            .when(waiting, |d| d.font_weight(FontWeight::MEDIUM))
            .child(SharedString::from(words));
        let words_el = if waiting {
            self.marked("subagent-needs-you", words_el)
        } else {
            words_el
        };
        let id = lane.id.clone();
        let row = div()
            .id(("subagent-row", i))
            .flex()
            .items_center()
            .gap(px(8.))
            .h(px(ROW_H))
            .px(px(8.))
            .rounded(px(6.))
            .cursor_pointer()
            .hover(move |s| s.bg(hsla(ui.row_hover)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.open_subagent(&id, cx);
                }),
            )
            .child(
                div()
                    .flex_none()
                    .w(px(14.))
                    .flex()
                    .justify_center()
                    .child(self.lane_mark(lane, c)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .truncate()
                    .text_color(hsla(name_ink))
                    .child(SharedString::from(lane.label.clone())),
            )
            .child(words_el);
        self.marked_row(lane.label.clone(), row).into_any_element()
    }
}
