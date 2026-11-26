//! The subagents tab and its popover (chore 19, note 93): `🤖 4/6` in the corner tab beside the
//! shells and plan tabs, and over the chat a row per subagent — its mark (the arc while it
//! runs), its name and what it is doing; one that needs you in red. A row opens that
//! subagent's view (one that needs you, with the keys on its question). The words are
//! `agent_subagents`'.

use gpui::{
    AnyElement, Context, FontWeight, InteractiveElement, IntoElement, MouseButton, ParentElement,
    StatefulInteractiveElement, Styled, div, prelude::FluentBuilder, px,
};

use super::{AgentTab, Colors, popover};
use crate::agent_box::BoxMenu;
use crate::agent_box::menus::MenuPlace;
use crate::agent_conversation::{Lane, LaneStatus};
use crate::agent_lanes::View;
use crate::agent_subagents::{TabTone, header, order, row_words, tab_tone, tab_words};
use crate::appearance::Appearance;
use crate::terminal_view::hsla;

/// How many rows show before the list scrolls.
const MAX_ROWS: usize = 10;

impl AgentTab {
    /// The subagents tab's pill (`🤖 1/2`): peach while one runs, its green icon alone once all
    /// finished (chore 34), red while one needs you; none before a subagent started.
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
        // All finished: the count moves into the tip (chore 34).
        let tip = match tab_tone(lanes) {
            TabTone::Done => crate::corner_tabs::done_tip(lanes.len(), "subagent", "subagents"),
            _ => "Subagents".to_string(),
        };
        Some(
            self.corner_pill(
                "agent-subagents-tab",
                "subagents-tab",
                crate::icons::Icon::Bot,
                tab_words(lanes),
                ink,
                BoxMenu::Subagents,
                tip,
                |this, _, cx| this.toggle_subagents(cx),
                c,
                cx,
            )
            // Chore 31: it spins only while the chat shows no running mark of its own.
            .when_some(self.tab_spinner(), |d, mark| d.child(mark))
            .into_any_element(),
        )
    }

    /// The subagents tab's spinner, a running subagent's arc (`tab_spins`), or none.
    fn tab_spinner(&self) -> Option<AnyElement> {
        let lanes = self.conversation.lanes();
        if !crate::agent_subagents::tab_spins(lanes, self.chat_mark_shown()) {
            return None;
        }
        let lane = lanes.iter().find(|l| l.status == LaneStatus::Working)?;
        let mark = self.lane_marks.get(&lane.id)?.clone();
        Some(
            self.marked("subagents-tab-arc", div().flex_none().child(mark))
                .into_any_element(),
        )
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

    /// The popover (pointing at the tab, as the plan's), in the popovers' one look (chore 36):
    /// "Subagents" and "1 of 2 done · 1 running", a row per subagent, and "Click a subagent to
    /// open it".
    pub(crate) fn subagents_popover(&self, c: &Colors, cx: &mut Context<Self>) -> AnyElement {
        let ui = cx.global::<Appearance>().theme.ui();
        let lanes = self.conversation.lanes();
        let rows: Vec<AnyElement> = order(lanes)
            .into_iter()
            .map(|i| self.subagent_row(i, &lanes[i], c, cx))
            .collect();
        // Many subagents scroll inside the popover (it never runs off the window).
        let list = div()
            .id("subagent-rows")
            .max_h(px(MAX_ROWS as f32 * popover::ROW_H))
            .overflow_y_scroll()
            .children(rows);
        let content = vec![
            popover::header("Subagents", Some(popover::count(header(lanes))), &ui)
                .into_any_element(),
            self.marked("subagent-rows", list).into_any_element(),
            popover::quiet("Click a subagent to open it", &ui).into_any_element(),
        ];
        self.popover_frame(
            "subagents-popover",
            content,
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
        let words_el = popover::tail(words, if waiting { c.failed_panel } else { ui.text_dim })
            .when(waiting, |d| d.font_weight(FontWeight::MEDIUM));
        let words_el = if waiting {
            self.marked("subagent-needs-you", words_el)
        } else {
            words_el
        };
        let id = lane.id.clone();
        let row = popover::row()
            .id(("subagent-row", i))
            .cursor_pointer()
            .hover(move |s| s.bg(hsla(ui.row_hover)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.open_subagent(&id, cx);
                }),
            )
            .child(popover::mark_cell(self.lane_mark(lane, c)))
            .child(popover::title(lane.label.clone(), name_ink))
            .child(words_el);
        self.marked_row(lane.label.clone(), row).into_any_element()
    }
}
