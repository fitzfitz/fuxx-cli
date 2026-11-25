//! The agent's question card in the chat (batch 3, chore 8; your words: "we don't have open
//! question popup yet"): Claude's AskUserQuestion as a card of options, one question at a time,
//! keyboard first, the message box its "Other". `agent_ask` decides what each key does; this
//! draws it (the lead designer's spec), routes the keys (key context `AgentAsk`) and sends the
//! answer.
//!
//! The user's UX bar (2026-11-23: "make it easy to use and easy to access, feels like
//! required"): the card docks over the chat's bottom while its row is below (the latest-prompt
//! strip's check: a cached row and the list's last layout, no scan), says the agent is waiting,
//! takes the cursor to a "(Recommended)" option, ends with a review of every answer, and leaves
//! a summary that opens to the full questions. It fades in once (120 ms, none with reduce
//! motion), the box rim gets one lap in the waiting colour, and nothing moves after.

use super::*;
use crate::agent_ask::{
    AskKey, AskUi, CardLayout, DockKind, Effect, answer_message, asked_line, asks_line, card_width,
    chip_of, dock_kind, dock_shows, header_of, layout_for, preview_lines, recommended, shown_label,
    strip_covers_card, summary_spans,
};
use crate::agent_conversation::Reply;
use crate::providers::{QAnswer, Question};
use crate::ui_tokens::ask as size;
use gpui::{HighlightStyle, ScrollHandle, StyledText, canvas};

/// The preview well's text size.
const WELL_TEXT: f32 = 12.5;
/// The chat viewport's room the strip needs beside the dock (the designer: under it, the strip
/// hides).
const STRIP_ROOM: f32 = 96.0;

/// Scenes: what the open card shows and whether it has the keys.
#[cfg(feature = "ui-shots")]
#[derive(Clone, Debug, PartialEq)]
pub struct ShotAsk {
    pub step: usize,
    pub cursor: usize,
    pub picks: Vec<Vec<usize>>,
    pub focus: bool,
    pub keys_on: bool,
    pub docked: bool,
    pub dock: Option<DockKind>,
    pub review: bool,
    pub armed: bool,
    pub anyway: bool,
    /// The light shows (the tab focused, the window the key window).
    pub lit: bool,
}

/// Where a card is drawn: in its chat row, or docked over the chat's bottom.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Place {
    Chat,
    Dock(DockKind),
}

/// The open card's state in a tab.
#[derive(Default)]
pub(crate) struct AskState {
    /// The oldest open ask, when it is a question (a permission before it goes first).
    pub(crate) ui: Option<AskUi>,
    /// The card takes the keys (while the box is empty); Tab or typing gives them to the box,
    /// ↑ or Esc in the empty box back.
    pub(crate) focus: bool,
    /// The box's text as the Other row last showed it (its row is laid out again on a change).
    mirrored: String,
    /// The light shows on the cursor row: the card has the keys and the tab the keyboard.
    lit: bool,
    /// The answered summaries opened, by question id.
    open: HashSet<String>,
    /// The last draw's dock: its kind and height, while shown.
    pub(crate) dock: Option<(DockKind, f32)>,
    /// The Waiting marks of the card's head, in the chat and docked.
    marks: Option<(Entity<MarkView>, Entity<MarkView>)>,
    /// The step last shown, when it changed (the body's cross-fade) and how many times.
    step: usize,
    step_at: Option<Instant>,
    steps: u64,
    /// The compact dock's option list (it scrolls, the lit row kept in view).
    dock_scroll: ScrollHandle,
    /// Disarms Skip's "Esc again" when its time is up.
    disarm: Option<Task<()>>,
    /// Whether the card was below its place after the last layout: what a draw goes by while
    /// its row isn't measured (a question just come is measured as it lays out: no flash).
    after: Rc<Cell<bool>>,
    /// The chat's width the card was last laid out for (its layout follows the card's width:
    /// the design review's side-by-side previews and narrow rules).
    width: Rc<Cell<f32>>,
    /// The oldest open ask last seen: when another becomes it, its row is laid out again (a
    /// queued line becomes its card).
    oldest: Option<usize>,
}

/// Whether the latest-prompt strip would cover the open ask's row `row` (the design review: the
/// "waiting" line is never covered), from the list as it was last laid out.
pub(super) fn ask_under_strip(list: &ListState, row: Option<usize>, cover: f32) -> bool {
    let view = list.viewport_bounds();
    if view.size.height <= px(0.) {
        return false;
    }
    let bounds = row
        .and_then(|r| list.bounds_for_item(r))
        .map(|b| (f32::from(b.top()), f32::from(b.bottom())));
    strip_covers_card(bounds, f32::from(view.top()), cover)
}

impl AgentTab {
    /// The question the box and Enter answer: the oldest open ask, when it is one.
    fn ask_target(&self) -> Option<(usize, &str, &[Question])> {
        let ix = self.conversation.oldest_ask()?;
        match &self.conversation.items()[ix] {
            Item::Question {
                id,
                questions,
                reply: None,
                ..
            } => Some((ix, id.as_str(), questions.as_slice())),
            _ => None,
        }
    }

    /// The card follows the conversation (after every change): a new oldest question gets a
    /// fresh card with the keys, its Waiting marks and one lap of the box rim; none, no card.
    pub(super) fn sync_ask(&mut self, cx: &mut Context<Self>) {
        // Another ask is the oldest now: its row (a queued line until now) is laid out again.
        let oldest = self.conversation.oldest_ask();
        if oldest != self.ask.oldest {
            self.ask.oldest = oldest;
            if let Some(ix) = oldest {
                self.relayout_item(ix);
            }
        }
        let target = self
            .ask_target()
            .map(|(_, id, questions)| (id.to_string(), questions.to_vec()));
        let Some((id, questions)) = target else {
            self.ask.ui = None;
            self.ask.marks = None;
            self.ask.disarm = None;
            return;
        };
        if self.ask.ui.as_ref().is_some_and(|ui| ui.id == id) {
            return;
        }
        self.ask.ui = Some(AskUi::new(id, questions, Instant::now()));
        self.ask.focus = true;
        self.ask.step = 0;
        self.ask.step_at = None;
        // A question the agent didn't wait on (2026-11-24): its Waiting mark doesn't breathe
        // (a clock of its own that never ticks holds it still).
        let clock = if self.conversation.questions_by_message() {
            cx.new(|_| MarkClock {
                phase: crate::status_mark::STILL_PHASE,
            })
        } else {
            self.clock.clone()
        };
        let mut mark = || cx.new(|cx| MarkView::new(&clock, Status::Waiting, px(MARK), BLACK, cx));
        self.ask.marks = Some((mark(), mark()));
        self.relayout_ask();
        // The designer: the box rim's one lap, in the waiting colour.
        self.shown(cx);
    }

    /// A question waits for you here (the box and Enter answer it).
    pub(crate) fn asking(&self) -> bool {
        self.ask.ui.is_some()
    }

    /// Since when the question here has waited (⇧⌘A goes to the oldest).
    pub fn waiting_ask_since(&self) -> Option<Instant> {
        self.ask.ui.as_ref().map(AskUi::shown_at)
    }

    /// The card's keys are on: it waits, has the keys, and the box is empty (nothing else over
    /// the box).
    pub(super) fn ask_keys_on(&self, cx: &App) -> bool {
        self.ask.ui.is_some()
            && self.ask.focus
            && self.modal.is_none()
            && self.image_view.is_none()
            && self.menu.is_none()
            && self.rw.pick.is_none()
            && self.composer.read(cx).text().is_empty()
    }

    /// Before a draw: whether the light shows (the card has the keys and the box the
    /// keyboard); its row is laid out again when that changes.
    pub(super) fn ask_frame(&mut self, window: &Window, cx: &App) {
        if self.ask.ui.is_none() {
            return;
        }
        // No light while the window isn't the key window (the design review; the picks stay).
        let composer = self.composer.read(cx);
        let lit = self.ask_keys_on(cx)
            && composer.focus_handle(cx).is_focused(window)
            && composer.is_key_window(window);
        let view_w = f32::from(self.pane().list.viewport_bounds().size.width);
        let resized = view_w > 0. && view_w != self.ask.width.get();
        if resized {
            self.ask.width.set(view_w);
        }
        if lit != self.ask.lit || resized {
            self.ask.lit = lit;
            self.relayout_ask();
        }
    }

    /// The open card's layout for the chat's width as last laid out (before the first layout:
    /// the widest column).
    fn ask_layout(&self) -> CardLayout {
        let view_w = self.ask.width.get();
        layout_for(if view_w > 0. {
            card_width(view_w)
        } else {
            card_width(f32::MAX)
        })
    }

    /// The oldest open ask's row in the shown view (a question or a permission).
    pub(super) fn ask_row(&self) -> Option<usize> {
        let ix = self.conversation.oldest_ask()?;
        let pane = self.pane();
        pane.cache
            .order()
            .binary_search(&ix)
            .ok()
            .map(|k| k + pane.head as usize)
    }

    /// "⟨Label⟩ asks: ⟨its first question⟩ (+N more)" while a question is the oldest ask (a
    /// subagent's says its label).
    pub(super) fn ask_question_line(&self) -> Option<String> {
        let (ix, _, questions) = self.ask_target()?;
        if self.conversation.questions_by_message() {
            return Some(asked_line(self.who(), questions));
        }
        let label = self.asker_label(ix);
        Some(asks_line(&label, questions))
    }

    /// The agent's short name ("agy"), as a question it didn't wait on says it (and its
    /// notification's title).
    pub(crate) fn who(&self) -> &str {
        self.provider.map_or(self.label(), |p| p.info().who.1)
    }

    /// Who asks question `ix`: its lane's subagent type (its task after it, when that says
    /// more; the design review), else the agent's label.
    fn asker(&self, ix: usize) -> (String, Option<String>) {
        match &self.conversation.items()[ix] {
            Item::Question { by: Some(b), .. } => match self.conversation.lane(b) {
                Some(l) => crate::agent_ask::asker(&l.kind, &l.label),
                None => (self.label().to_string(), None),
            },
            _ => (self.label().to_string(), None),
        }
    }

    fn asker_label(&self, ix: usize) -> String {
        self.asker(ix).0
    }

    /// The box's placeholder while it is the card's Other.
    pub(super) fn ask_placeholder(&self) -> Option<String> {
        self.ask.ui.as_ref().map(AskUi::placeholder)
    }

    /// The question waits: the keys go to its card (a click on the tab, ⇧⌘A, a banner),
    /// without scrolling (below the view, it is docked).
    pub(crate) fn land_on_ask(&mut self, cx: &mut Context<Self>) {
        if self.ask_target().is_none() {
            return;
        }
        self.ask.focus = true;
        self.refocus = Some(Refocus::Box);
        cx.notify();
    }

    /// Its banner clicked (the opens fixed, 2026-11-23): the oldest ask's card takes the keys —
    /// a question's as `land_on_ask` does, a permission's brought into sight with the box (its
    /// Enter allows, its words deny) taking the keyboard.
    pub(crate) fn land_on_waiting(&mut self, cx: &mut Context<Self>) {
        let Some(ix) = self.conversation.oldest_ask() else {
            return;
        };
        if matches!(self.conversation.items()[ix], Item::Permission { .. }) {
            let pane = self.pane();
            if let Ok(k) = pane.cache.order().binary_search(&ix) {
                pane.list.scroll_to_reveal_item(k + pane.head as usize);
            }
            self.refocus = Some(Refocus::Box);
            cx.notify();
        } else {
            self.land_on_ask(cx);
        }
    }

    /// The latest-prompt strip clicked while a question waits: its row brought into sight and
    /// the keys on its card.
    pub(super) fn reveal_ask(&mut self, cx: &mut Context<Self>) {
        if let Some((ix, _, _)) = self.ask_target() {
            let pane = self.pane();
            if let Ok(k) = pane.cache.order().binary_search(&ix) {
                pane.list.scroll_to_reveal_item(k + pane.head as usize);
            }
        }
        self.land_on_ask(cx);
    }

    /// The dock's "Show in chat": the chat glides to its end (the strip's glide; a jump with
    /// motion reduced), following it again.
    fn show_in_chat(&mut self, cx: &mut Context<Self>) {
        self.glide_to_end(cx);
        self.land_on_ask(cx);
    }

    /// Scenes: the dock's "Show in chat" clicked.
    #[cfg(feature = "ui-shots")]
    pub fn shot_show_in_chat(&mut self, cx: &mut Context<Self>) {
        self.show_in_chat(cx);
    }

    /// The question's row is laid out again in every view (its card changed).
    fn relayout_ask(&mut self) {
        if let Some(ix) = self.ask_target().map(|(ix, _, _)| ix) {
            self.relayout_item(ix);
        }
    }

    /// Item `ix`'s row laid out again in every view.
    fn relayout_item(&self, ix: usize) {
        for pane in self.panes.values() {
            if let Ok(k) = pane.cache.order().binary_search(&ix) {
                let k = k + pane.head as usize;
                if k < pane.list.item_count() {
                    pane.list.splice(k..k + 1, 1);
                }
            }
        }
    }

    /// The box changed: typing gives it the keys, and the Other row follows its text.
    pub(super) fn ask_box_changed(&mut self, cx: &mut Context<Self>) {
        if self.ask.ui.is_none() {
            return;
        }
        let text = self.composer.read(cx).text().to_string();
        if text == self.ask.mirrored {
            return;
        }
        if !text.is_empty() {
            self.ask.focus = false;
        }
        self.ask.mirrored = text;
        self.relayout_ask();
        cx.notify();
    }

    /// ↑ or Esc in the empty box while a question waits: the keys go back to its card. True
    /// when it took the key (Esc never stops the agent while it waits on you).
    pub(super) fn ask_take_back(&mut self, cx: &mut Context<Self>) -> bool {
        if self.ask.ui.is_none() {
            return false;
        }
        if self.composer.read(cx).text().is_empty() {
            self.ask.focus = true;
            cx.notify();
        }
        true
    }

    /// Enter (or the send circle) in the box while a question waits: its text is Other for the
    /// question shown (empty: the card's Enter; within the guard: nothing). The box is cleared
    /// once its text is used, and the keys go back to the card.
    pub(super) fn ask_submit(&mut self, text: &str, cx: &mut Context<Self>) {
        let Some(ui) = &mut self.ask.ui else {
            return;
        };
        let effect = ui.box_enter(text, Instant::now());
        let used = !text.trim().is_empty() && !matches!(effect, Effect::None);
        if used {
            self.ask.focus = true;
            self.composer
                .update(cx, |c, cx| c.set_text(String::new(), cx));
        }
        self.apply_ask(effect, cx);
    }

    fn ask_key(&mut self, key: AskKey, cx: &mut Context<Self>) {
        let Some(ui) = &mut self.ask.ui else {
            return;
        };
        let effect = ui.key(key, Instant::now());
        self.apply_ask(effect, cx);
    }

    /// Option `option` of the question shown, clicked.
    fn ask_click(&mut self, option: usize, cx: &mut Context<Self>) {
        let Some(ui) = &mut self.ask.ui else {
            return;
        };
        let effect = ui.click(option, Instant::now());
        self.ask.focus = true;
        self.apply_ask(effect, cx);
    }

    /// Step `step` shown (a step chip), the keys on the card.
    fn ask_go_to(&mut self, step: usize, cx: &mut Context<Self>) {
        if let Some(ui) = &mut self.ask.ui {
            ui.go_to(step);
            self.ask.focus = true;
            self.apply_ask(Effect::Moved, cx);
        }
    }

    /// A review row clicked: its question, Enter coming back to the review.
    fn ask_open_from_review(&mut self, q: usize, cx: &mut Context<Self>) {
        if let Some(ui) = &mut self.ask.ui {
            ui.open_from_review(q);
            self.ask.focus = true;
            self.apply_ask(Effect::Moved, cx);
        }
    }

    /// The Other row clicked: the keys go to the box.
    fn ask_other(&mut self, cx: &mut Context<Self>) {
        self.apply_ask(Effect::ToBox(None), cx);
    }

    /// The card's primary button: as Enter (the box's text, if any, as Other).
    fn ask_enter_pressed(&mut self, cx: &mut Context<Self>) {
        let text = self.composer.read(cx).text().to_string();
        self.ask_submit(&text, cx);
    }

    fn apply_ask(&mut self, effect: Effect, cx: &mut Context<Self>) {
        let Some(ui) = self.ask.ui.as_ref() else {
            return;
        };
        let (id, step, cursor, armed) = (
            ui.id.clone(),
            ui.step,
            ui.cursor,
            ui.skip_armed(Instant::now()),
        );
        // A question the agent didn't wait on (agy, 2026-11-24): your picks go as an ordinary
        // message, through the normal send (your bubble shows; mid-turn it queues); Skip only
        // closes the card.
        if self.conversation.questions_by_message() {
            match effect {
                Effect::Send(answers) => {
                    let Some(text) = answer_message(&ui.questions, &answers) else {
                        return;
                    };
                    self.conversation.reply(&id, Reply::Answered(answers));
                    self.send(text, cx);
                    return;
                }
                Effect::Skip => {
                    self.conversation.reply(&id, Reply::Skipped);
                    self.after_change(true, cx);
                    return;
                }
                _ => {}
            }
        }
        match effect {
            Effect::None => {}
            Effect::Moved => {
                if step != self.ask.step {
                    self.ask.step = step;
                    self.ask.step_at = Some(Instant::now());
                    self.ask.steps += 1;
                }
                self.ask.dock_scroll.scroll_to_item(cursor);
                // "Esc again to skip" goes back to "Skip" when its time is up.
                self.ask.disarm = armed.then(|| {
                    cx.spawn(async move |this, cx| {
                        cx.background_executor()
                            .timer(crate::agent_ask::SKIP_ARM)
                            .await;
                        this.update(cx, |this, cx| {
                            this.relayout_ask();
                            cx.notify();
                        })
                        .ok();
                    })
                });
                self.relayout_ask();
                self.update_placeholder(cx);
                cx.notify();
            }
            Effect::ToBox(typed) => {
                self.ask.focus = false;
                if let Some(c) = typed {
                    self.composer
                        .update(cx, |b, cx| b.insert(&c.to_string(), cx));
                }
                self.refocus = Some(Refocus::Box);
                self.relayout_ask();
                cx.notify();
            }
            Effect::Send(answers) => {
                self.write(&Action::Answer {
                    id: id.clone(),
                    answers: answers.clone(),
                });
                self.suggestion = None;
                self.conversation.reply(&id, Reply::Answered(answers));
                self.after_change(true, cx);
            }
            Effect::Skip => {
                self.write(&Action::Deny {
                    id: id.clone(),
                    reason: None,
                });
                self.conversation.reply(&id, Reply::Skipped);
                self.after_change(true, cx);
            }
        }
    }

    // The card's keys (bound in main.rs to "AgentAsk > Composer").
    pub(super) fn ask_up(&mut self, _: &AskUp, _: &mut Window, cx: &mut Context<Self>) {
        self.ask_key(AskKey::Up, cx);
    }
    pub(super) fn ask_down(&mut self, _: &AskDown, _: &mut Window, cx: &mut Context<Self>) {
        self.ask_key(AskKey::Down, cx);
    }
    pub(super) fn ask_prev(&mut self, _: &AskPrev, _: &mut Window, cx: &mut Context<Self>) {
        self.ask_key(AskKey::Prev, cx);
    }
    pub(super) fn ask_next(&mut self, _: &AskNext, _: &mut Window, cx: &mut Context<Self>) {
        self.ask_key(AskKey::Next, cx);
    }
    pub(super) fn ask_space(&mut self, _: &AskSpace, _: &mut Window, cx: &mut Context<Self>) {
        self.ask_key(AskKey::Space, cx);
    }
    pub(super) fn ask_enter(&mut self, _: &AskEnter, _: &mut Window, cx: &mut Context<Self>) {
        self.ask_key(AskKey::Enter, cx);
    }
    pub(super) fn ask_to_box(&mut self, _: &AskToBox, _: &mut Window, cx: &mut Context<Self>) {
        self.ask_key(AskKey::Tab, cx);
    }
    pub(super) fn ask_skip(&mut self, _: &AskSkip, _: &mut Window, cx: &mut Context<Self>) {
        self.ask_key(AskKey::Esc, cx);
    }
    pub(super) fn ask_pick(&mut self, digit: usize, cx: &mut Context<Self>) {
        self.ask_key(AskKey::Digit(digit), cx);
    }

    /// Item `ix`, a question: its card while it is the one the box answers, a queued line while
    /// an older ask goes first, else its summary.
    pub(super) fn question_row(
        &self,
        ix: usize,
        id: &str,
        questions: &[Question],
        reply: Option<&Reply>,
        look: &Look,
    ) -> AnyElement {
        match (reply, self.ask.ui.as_ref().filter(|ui| ui.id == id)) {
            (Some(reply), _) => self.ask_summary(id, questions, reply, look),
            (None, Some(ui)) => self.ask_card(ix, ui, Place::Chat, look),
            (None, None) => self.ask_queued(ix, questions, look),
        }
    }

    /// The younger of two open asks (the designer): one quiet line, not interactive.
    fn ask_queued(&self, ix: usize, questions: &[Question], look: &Look) -> AnyElement {
        let line = format!(
            "Next \u{b7} {}",
            asks_line(&self.asker_label(ix), questions)
        );
        self.queued_line(ix, line, look)
    }

    /// A queued ask's line (a question or a permission): one quiet line, "Answer the one above
    /// first".
    pub(super) fn queued_line(&self, ix: usize, line: String, look: &Look) -> AnyElement {
        let c = &look.c;
        let el = div()
            .id(SharedString::from(format!("ask-queued-{ix}")))
            .w_full()
            .h(px(size::QUEUED_H))
            .px(px(size::PAD_X))
            .py(px(10.))
            .flex()
            .items_center()
            .rounded(px(crate::agent_chat::chat::R_CARD))
            .border_1()
            .border_color(hsla(c.ask_key_edge))
            .bg(hsla(c.ask_fill))
            .text_size(px(text::META))
            .text_color(hsla(c.ask_dim))
            .tooltip(|_, cx| {
                cx.new(|_| crate::sidebar_toggle::Tip("Answer the one above first".into()))
                    .into()
            })
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .truncate()
                    .child(SharedString::from(line)),
            );
        self.marked("ask-queued", el).into_any_element()
    }

    /// The card docked over the chat's bottom while its row sits below its place (the
    /// designer's rule; the strip's after-layout check). An overlay: nothing moves.
    pub(super) fn ask_dock(&mut self, look: &Look, cx: &mut Context<Self>) -> Option<AnyElement> {
        let Some((ix, _, _)) = self.ask_target() else {
            self.ask.dock = None;
            return None;
        };
        let pane = self.pane();
        let row = pane
            .cache
            .order()
            .binary_search(&ix)
            .ok()
            .map(|k| k + pane.head as usize);
        let list = pane.list.clone();
        let decide = move |list: &ListState, unmeasured: bool| -> Option<(DockKind, f32)> {
            let view = list.viewport_bounds();
            let laid_out = view.size.height > px(0.);
            let bounds = row.and_then(|r| list.bounds_for_item(r));
            let below = unmeasured && row.is_some_and(|r| r >= list.logical_scroll_top().item_ix);
            let (kind, h) = dock_kind(
                bounds.map(|b| f32::from(b.size.height)),
                f32::from(view.size.height),
            );
            dock_shows(
                bounds.map(|b| f32::from(b.top())),
                below,
                f32::from(view.bottom()),
                h,
                laid_out,
            )
            .then_some((kind, h))
        };
        let after = self.ask.after.clone();
        let width = self.ask.width.clone();
        let dock = decide(&list, after.get());
        self.ask.dock = dock;
        let tab = cx.entity_id();
        let check = canvas(
            move |_, window, _| {
                let now = decide(&list, true);
                after.set(now.is_some());
                // A new width lays the card out again (`ask_frame`), in the next frame.
                let w = f32::from(list.viewport_bounds().size.width);
                let resized = w > 0. && w != width.get();
                if resized || now.map(|d| d.0) != dock.map(|d| d.0) {
                    window.on_next_frame(move |_, cx| cx.notify(tab));
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_0();
        let layer = div().absolute().left_0().right_0().bottom_0().child(check);
        let Some((kind, _)) = dock else {
            return Some(layer.into_any_element());
        };
        let ui = self.ask.ui.as_ref()?;
        let card = self.ask_card(ix, ui, Place::Dock(kind), look);
        let c = &look.c;
        let page = hsla(c.page);
        let band = div()
            .w_full()
            .h(px(size::DOCK_BAND))
            .bg(gpui::linear_gradient(
                180.,
                gpui::linear_color_stop(gpui::Hsla { a: 0., ..page }, 0.),
                gpui::linear_color_stop(page, 1.),
            ));
        let column = div()
            .w_full()
            .max_w(px(crate::agent_chat::chat::COLUMN_MAX))
            .flex()
            .flex_row()
            .gap(px(crate::agent_chat::chat::AVATAR_GAP))
            .child(
                div()
                    .flex_none()
                    .w(px(crate::agent_chat::chat::AVATAR))
                    .pt(px(size::PAD_Y))
                    .child(self.avatar(look)),
            )
            .child(div().flex_1().min_w(px(0.)).child(card));
        Some(
            layer
                .block_mouse_except_scroll()
                .child(band)
                .child(
                    div()
                        .w_full()
                        .bg(page)
                        .px(px(AREA_X))
                        .pb(px(crate::agent_chat::chat::LAST_BOTTOM))
                        .flex()
                        .justify_center()
                        .child(column),
                )
                .into_any_element(),
        )
    }

    /// Whether the strip makes room for the dock (the designer: a short chat shows the dock).
    pub(super) fn ask_hides_strip(&self, view_h: f32, strip_cover: f32) -> bool {
        self.ask
            .dock
            .is_some_and(|(_, h)| view_h < strip_cover + h + STRIP_ROOM)
    }

    /// The open card (the designer's states A–F): who waits, the steps, the question shown
    /// with its options and Other (or the review), a preview well, and the buttons.
    fn ask_card(&self, ix: usize, ui: &AskUi, place: Place, look: &Look) -> AnyElement {
        let c = &look.c;
        let docked = place != Place::Chat;
        let tag = if docked { "dock" } else { "row" };
        let (label, task) = self.asker(ix);
        let by = match &self.conversation.items()[ix] {
            Item::Question { by, .. } => by.clone(),
            _ => None,
        };
        let layout = self.ask_layout();
        let (dim, ink) = (c.ask_dim, c.ask_text);
        let link = move |id: &str| {
            div()
                .id(SharedString::from(format!("{id}-{tag}")))
                .flex_none()
                .text_size(px(text::KEYS))
                .font_weight(FontWeight::MEDIUM)
                .text_color(hsla(dim))
                .cursor_pointer()
                .hover(move |s| s.text_color(hsla(ink)))
        };
        let mark =
            self.ask.marks.as_ref().map(|(row, dock)| {
                if docked { dock.clone() } else { row.clone() }.into_any_element()
            });
        // A lane's ask seen in Main: a way to its lane.
        let open_lane = by
            .filter(|_| self.view == crate::agent_lanes::View::Main)
            .map(|lane| {
                let name = self
                    .conversation
                    .lane(&lane)
                    .map_or_else(|| label.clone(), |l| l.label.clone());
                let tip: SharedString = format!("Show {name}'s view").into();
                // "Open lane": the head says the task already (the designer's second look).
                link("ask-open-lane")
                    .child("Open lane")
                    .tooltip(move |_, cx| {
                        cx.new(|_| crate::sidebar_toggle::Tip(tip.clone())).into()
                    })
                    .on_click(on_tab(&look.view, move |this, cx| {
                        this.show_view(crate::agent_lanes::View::Lane(lane.clone()), cx)
                    }))
            });
        let head = div()
            .h(px(size::HEAD_H))
            .flex()
            .items_center()
            .gap(px(8.))
            .child(
                div()
                    .flex_none()
                    .size(px(14.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .children(mark),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .truncate()
                    .text_size(px(text::META))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(hsla(c.ask_head))
                    .child({
                        // A lane's task after who waits, quiet (the design review). One the
                        // agent didn't wait on says so (2026-11-24).
                        let waits = if self.conversation.questions_by_message() {
                            format!(
                                "{} asked \u{2014} it couldn't wait for your answer",
                                self.who()
                            )
                        } else {
                            format!("{label} is waiting for your answer")
                        };
                        let at = waits.len();
                        let whole = match &task {
                            Some(t) => format!("{waits} \u{b7} {t}"),
                            None => waits,
                        };
                        let runs = (whole.len() > at)
                            .then(|| {
                                (
                                    at..whole.len(),
                                    HighlightStyle {
                                        color: Some(hsla(c.ask_dim)),
                                        font_weight: Some(FontWeight::NORMAL),
                                        ..Default::default()
                                    },
                                )
                            })
                            .into_iter()
                            .collect::<Vec<_>>();
                        StyledText::new(whole).with_highlights(runs)
                    }),
            )
            .children(open_lane)
            .when(docked, |d| {
                d.child(
                    link("ask-show-in-chat")
                        .child("Show in chat")
                        .on_click(on_tab(&look.view, |this, cx| this.show_in_chat(cx))),
                )
            });
        let card = div()
            .w_full()
            .px(px(size::PAD_X))
            .py(px(size::PAD_Y))
            .rounded(px(crate::agent_chat::chat::R_CARD))
            .border_1()
            .border_color(hsla(c.ask_edge))
            .bg(hsla(c.ask_fill))
            .flex()
            .flex_col()
            .gap(px(size::GAP))
            .child(head);
        let card = if place == Place::Dock(DockKind::Tiny) {
            card
        } else {
            let compact = place == Place::Dock(DockKind::Compact);
            let body = match ui.current() {
                Some(q) => self.ask_question(ui, q, compact, layout, tag, look),
                None => self.ask_review(ui, tag, look),
            };
            // The body cross-fades on a step change; the head, steps and footer stay put.
            let body = match self.ask.step_at {
                Some(at)
                    if crate::agent_top::animating(
                        Some(at),
                        look.now,
                        Duration::from_millis(size::STEP_FADE_MS),
                    ) =>
                {
                    div()
                        .child(body)
                        .with_animation(
                            (
                                SharedString::from(format!("ask-step-fade-{tag}")),
                                self.ask.steps as usize,
                            ),
                            Animation::new(Duration::from_millis(size::STEP_FADE_MS)),
                            |d, t| d.opacity(t),
                        )
                        .into_any_element()
                }
                _ => body,
            };
            // Why the agent went on, and where your picks go (its own words; not in the
            // compact dock).
            let note = self
                .provider
                .filter(|_| self.conversation.questions_by_message() && !compact)
                .and_then(|p| p.question_note());
            card.when(ui.questions.len() >= 2, |d| {
                d.child(self.ask_steps(ui, layout, tag, look))
            })
            .child(body)
            .child(self.ask_footer(ui, layout, tag, look))
            .children(note.map(|n| {
                div()
                    .text_size(px(text::META))
                    .text_color(hsla(c.ask_dim))
                    .child(n)
            }))
        };
        let shadow = |y: f32, blur: f32, a: f32| gpui::BoxShadow {
            color: gpui::Hsla { a, ..gpui::black() },
            offset: gpui::point(px(0.), px(y)),
            blur_radius: px(blur),
            spread_radius: px(0.),
            inset: false,
        };
        let dark = c.page_is_dark;
        let card = card.when(docked, |d| {
            d.shadow(vec![
                shadow(-1., 2., if dark { 0.30 } else { 0.06 }),
                shadow(-6., 16., if dark { 0.30 } else { 0.10 }),
            ])
        });
        let card = self.marked(if docked { "ask-dock" } else { "ask-card" }, card);
        // It fades in once as it arrives (never while scrolling); none with reduce motion
        // (GPUI draws an animation still then).
        if crate::agent_top::animating(
            Some(ui.shown_at()),
            look.now,
            Duration::from_millis(size::FADE_MS),
        ) {
            card.with_animation(
                SharedString::from(format!("ask-fade-{tag}-{}", ui.id)),
                Animation::new(Duration::from_millis(size::FADE_MS))
                    .with_easing(crate::agent_top::ease_out_cubic),
                |d, t| d.opacity(t),
            )
            .into_any_element()
        } else {
            card.into_any_element()
        }
    }

    /// The steps row (two or more questions): each question's chip and the review's, and
    /// "2 of 3".
    fn ask_steps(
        &self,
        ui: &AskUi,
        layout: CardLayout,
        tag: &'static str,
        look: &Look,
    ) -> AnyElement {
        let c = &look.c;
        let n = ui.questions.len();
        // A narrow card: only the current chip (the design review: none cut mid-word).
        let shown = (0..ui.steps()).filter(|&s| !layout.narrow_steps || s == ui.step);
        let chips = shown.map(|s| {
            let (header, whole) = match ui.questions.get(s) {
                Some(q) => (chip_of(q, s), header_of(q, s)),
                None => ("Review".to_string(), "Review".to_string()),
            };
            let current = s == ui.step;
            let done = s < n && ui.answered(s);
            let light = c.ask_cursor;
            let tip: SharedString = format!("Go to {whole}").into();
            div()
                .id((SharedString::from(format!("ask-step-{tag}")), s))
                .flex_none()
                .h(px(size::STEP_H))
                .px(px(8.))
                .flex()
                .items_center()
                .gap(px(4.))
                .rounded(px(6.))
                .text_size(px(text::KEYS))
                .font_weight(FontWeight::MEDIUM)
                .text_color(hsla(if current { c.ask_text } else { c.ask_dim }))
                .when(current, |d| d.bg(hsla(light)))
                .when(!current, |d| {
                    d.cursor_pointer()
                        .hover(move |st| st.bg(hsla(light)))
                        .tooltip(move |_, cx| {
                            cx.new(|_| crate::sidebar_toggle::Tip(tip.clone())).into()
                        })
                        .on_click(on_tab(&look.view, move |this, cx| this.ask_go_to(s, cx)))
                })
                .when(done && !current, |d| {
                    d.child(icon(Icon::Check, px(10.), hsla(c.ask_mark)))
                })
                .child(SharedString::from(header))
        });
        // On the review: how many are answered, standing out while some aren't.
        let (count, short) = ui.counter();
        div()
            .h(px(size::STEP_H))
            .flex()
            .items_center()
            .gap(px(4.))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .flex()
                    .gap(px(4.))
                    .overflow_hidden()
                    .children(chips),
            )
            .child(
                div()
                    .flex_none()
                    .text_size(px(text::KEYS))
                    .text_color(hsla(if short { c.ask_head } else { c.ask_dim }))
                    .child(SharedString::from(count)),
            )
            .into_any_element()
    }

    /// One digit mark (the designer: the radio or checkbox and the shortcut at once): round
    /// for a single-select, square for a multi, filled when chosen.
    fn digit_mark(&self, n: usize, multi: bool, filled: bool, c: &Colors) -> AnyElement {
        let digit = if n <= 9 { n.to_string() } else { String::new() };
        div()
            .flex_none()
            .size(px(size::MARK))
            .rounded(px(if multi { 4. } else { size::MARK / 2. }))
            .flex()
            .items_center()
            .justify_center()
            .text_size(px(text::KEYS))
            .font_weight(FontWeight::MEDIUM)
            .when(filled, |d| {
                d.bg(hsla(c.ask_mark)).text_color(hsla(c.ask_mark_ink))
            })
            .when(!filled, |d| {
                d.border_1()
                    .border_color(hsla(c.ask_key_edge))
                    .text_color(hsla(c.ask_dim))
            })
            .child(SharedString::from(digit))
            .into_any_element()
    }

    /// The question shown: its words, its options, Other, and (single-select) the preview
    /// well.
    fn ask_question(
        &self,
        ui: &AskUi,
        q: &Question,
        compact: bool,
        layout: CardLayout,
        tag: &'static str,
        look: &Look,
    ) -> AnyElement {
        let c = &look.c;
        let n = q.options.len();
        let picks = &ui.picks[ui.step];
        // Other mode: the box has text (the designer's state F).
        let typed = self.ask.mirrored.trim().to_string();
        let typing = !typed.is_empty();
        let other_own = ui.other[ui.step].clone();
        let lit = |i: usize| {
            if typing {
                i == n
            } else {
                self.ask.lit && ui.cursor == i
            }
        };
        let filled = |i: usize| {
            if q.multi {
                picks.contains(&i)
            } else if typing || other_own.is_some() {
                false
            } else if !picks.is_empty() {
                picks.first() == Some(&i)
            } else {
                ui.cursor == i
            }
        };
        let words = div()
            .text_size(px(text::NAME))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(hsla(c.ask_text))
            .line_height(relative(1.45))
            .when(compact, |d| d.line_clamp(2))
            .child(SharedString::from(q.text.clone()));
        let light = c.ask_cursor;
        let row = |i: usize| {
            div()
                .id((SharedString::from(format!("ask-option-{tag}")), i))
                .w_full()
                .min_h(px(size::ROW_MIN))
                .px(px(size::ROW_PAD_X))
                .py(px(size::ROW_PAD_Y))
                .flex()
                .items_start()
                .gap(px(size::MARK_GAP))
                .rounded(px(size::ROW_R))
                .when(lit(i), |d| d.bg(hsla(light)))
                .cursor_pointer()
                .hover(move |s| s.bg(hsla(light)))
        };
        let options = q.options.iter().enumerate().map(|(i, o)| {
            let tag_line = recommended(&o.label).is_some().then(|| {
                div()
                    .flex_none()
                    .text_size(px(text::KEYS))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(hsla(c.ask_tag))
                    .child("Recommended")
            });
            let label = div()
                .flex()
                .items_center()
                .gap(px(size::TAG_GAP))
                .child(
                    div()
                        .text_size(px(text::UI))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(hsla(c.ask_text))
                        .child(SharedString::from(shown_label(&o.label).to_string())),
                )
                .children(tag_line);
            let about = (!compact && !o.description.is_empty()).then(|| {
                div()
                    .text_size(px(text::META))
                    .text_color(hsla(c.ask_dim))
                    .child(SharedString::from(o.description.clone()))
            });
            let tip: Option<SharedString> =
                (compact && !o.description.is_empty()).then(|| o.description.clone().into());
            row(i)
                .child(
                    div()
                        .pt(px(1.))
                        .child(self.digit_mark(i + 1, q.multi, filled(i), c)),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .flex()
                        .flex_col()
                        .gap(px(1.))
                        .child(label)
                        .children(about),
                )
                .when_some(tip, |d, tip| {
                    d.tooltip(move |_, cx| {
                        cx.new(|_| crate::sidebar_toggle::Tip(tip.clone())).into()
                    })
                })
                .on_click(on_tab(&look.view, move |this, cx| this.ask_click(i, cx)))
                .into_any_element()
        });
        // Other: the box's text as you type it, else what you gave, else "Other…".
        let shown_other = if typing {
            Some(typed)
        } else {
            other_own.clone()
        };
        let other_filled = shown_other.is_some() && (q.multi || picks.is_empty());
        let other_words = match &shown_other {
            Some(t) => div()
                .flex_1()
                .min_w(px(0.))
                .truncate()
                .text_size(px(text::UI))
                .text_color(hsla(c.ask_text))
                .child(SharedString::from(t.clone())),
            None => div()
                .flex_1()
                .text_size(px(text::UI))
                .text_color(hsla(c.ask_dim))
                .child("Other\u{2026}"),
        };
        let tab_hint = (lit(n) && !typing).then(|| {
            div()
                .flex_none()
                .text_size(px(text::KEYS))
                .text_color(hsla(c.ask_dim))
                .child("\u{21e5} to type")
        });
        let other = row(n)
            .child(
                div()
                    .pt(px(1.))
                    .child(self.digit_mark(n + 1, q.multi, other_filled, c)),
            )
            .child(other_words)
            .children(tab_hint)
            .on_click(on_tab(&look.view, |this, cx| this.ask_other(cx)))
            .into_any_element();
        let list = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(size::ROW_GAP))
            .children(options)
            .child(other);
        // The compact dock's options scroll inside it, the lit row kept in view.
        let list = if compact {
            div()
                .id(SharedString::from(format!("ask-options-{tag}")))
                .w_full()
                .max_h(px(size::DOCK_MAX * 0.4))
                .overflow_y_scroll()
                .track_scroll(&self.ask.dock_scroll)
                .child(list)
                .into_any_element()
        } else {
            list.into_any_element()
        };
        // The preview of the lit row (single-select), in a well as tall as the question's
        // tallest (capped), so the card never jumps as the light moves.
        let lines = if compact { 0 } else { preview_lines(q) };
        let preview = (lines > 0).then(|| {
            // Other lit (the box has text): no option's preview.
            let shown = (!typing)
                .then(|| q.options.get(ui.cursor).and_then(|o| o.preview.clone()))
                .flatten();
            let line_h = WELL_TEXT * size::WELL_LINE;
            let well = div()
                .h(px(lines as f32 * line_h + 2. * size::WELL_PAD_Y))
                .px(px(size::WELL_PAD_X))
                .py(px(size::WELL_PAD_Y))
                .rounded(px(crate::agent_chat::chat::R_GROUP));
            // Beside the options on a wide card (the well takes the rest), else under them.
            let well = if layout.side_by_side {
                well.flex_1().min_w(px(0.))
            } else {
                well.w_full()
            };
            match shown {
                Some(p) => {
                    let fill = hsla(c.ask_well);
                    // A line running past the padding fades out at it (the designer's second
                    // look: the clip keeps the right padding, the cut softened).
                    let fade =
                        crate::agent_ask::preview_overflows(&p, layout.well_text_w).then(|| {
                            div().absolute().top_0().bottom_0().right_0().w(px(24.)).bg(
                                gpui::linear_gradient(
                                    90.,
                                    gpui::linear_color_stop(gpui::Hsla { a: 0., ..fill }, 0.),
                                    gpui::linear_color_stop(fill, 1.),
                                ),
                            )
                        });
                    well.bg(fill)
                        .font_family(look.mono.clone())
                        .text_size(px(WELL_TEXT))
                        .line_height(px(line_h))
                        .text_color(hsla(c.ask_well_text))
                        .child(
                            // Code lines run on and clip inside the padding: a wrapped line
                            // would push the last out of the well's fixed height.
                            div()
                                .relative()
                                .size_full()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .child(SharedString::from(
                                    p.lines()
                                        .take(crate::agent_ask::PREVIEW_MAX_LINES)
                                        .collect::<Vec<_>>()
                                        .join("\n"),
                                ))
                                .children(fade),
                        )
                }
                // Nothing to show: an outline, not a dead slab (the design review).
                None => well
                    .border_1()
                    .border_color(hsla(c.ask_key_edge))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(text::META))
                    .text_color(hsla(c.ask_dim))
                    .child("No preview"),
            }
        });
        // The question, and under a multi-select's "Choose any that apply" 2 pt below it.
        let head = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(2.))
            .child(words)
            .when(q.multi, |d| {
                d.child(
                    div()
                        .text_size(px(text::META))
                        .text_color(hsla(c.ask_dim))
                        .child("Choose any that apply"),
                )
            });
        let options = match preview {
            Some(well) if layout.side_by_side => div()
                .w_full()
                .flex()
                .items_start()
                .gap(px(size::SIDE_GAP))
                .child(div().flex_none().w(px(layout.options_w)).child(list))
                .child(well)
                .into_any_element(),
            Some(well) => div()
                .w_full()
                .flex()
                .flex_col()
                .gap(px(8.))
                .child(list)
                .child(well)
                .into_any_element(),
            None => list,
        };
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(head)
            .child(options)
            .into_any_element()
    }

    /// The review (two or more questions): each header and its answer; a click or Space opens
    /// it.
    fn ask_review(&self, ui: &AskUi, tag: &'static str, look: &Look) -> AnyElement {
        let c = &look.c;
        let light = c.ask_cursor;
        let rows = ui
            .review()
            .into_iter()
            .enumerate()
            .map(|(i, (header, answer))| {
                let lit = self.ask.lit && ui.cursor == i;
                let change = div()
                    .flex_none()
                    .text_size(px(text::KEYS))
                    .text_color(hsla(c.ask_dim))
                    .child("Change");
                let group = SharedString::from(format!("ask-review-row-{tag}-{i}"));
                div()
                    .id((SharedString::from(format!("ask-review-{tag}")), i))
                    .group(group.clone())
                    .w_full()
                    .h(px(size::REVIEW_ROW))
                    .px(px(size::ROW_PAD_X))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .rounded(px(size::ROW_R))
                    .when(lit, |d| d.bg(hsla(light)))
                    .cursor_pointer()
                    .hover(move |s| s.bg(hsla(light)))
                    .on_click(on_tab(&look.view, move |this, cx| {
                        this.ask_open_from_review(i, cx)
                    }))
                    .child(
                        div()
                            .flex_none()
                            .w(px(size::REVIEW_KEY_W))
                            .truncate()
                            .text_size(px(text::META))
                            .text_color(hsla(c.ask_dim))
                            .child(SharedString::from(header)),
                    )
                    .child(match answer {
                        Some(a) => div()
                            .flex_1()
                            .min_w(px(0.))
                            .truncate()
                            .text_size(px(text::UI))
                            .text_color(hsla(c.ask_text))
                            .child(SharedString::from(a)),
                        None => div()
                            .flex_1()
                            .text_size(px(text::UI))
                            .text_color(hsla(c.ask_head))
                            .child("Not answered"),
                    })
                    .child(if lit {
                        change.into_any_element()
                    } else {
                        div()
                            .flex_none()
                            .invisible()
                            .group_hover(group, |s| s.visible())
                            .child(change)
                            .into_any_element()
                    })
            });
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(size::ROW_GAP))
            .children(rows)
            .into_any_element()
    }

    /// The card's buttons: the primary (Answer / Next / Review / Send answers / Send anyway),
    /// Skip (or "Esc again to skip"), and a key hint.
    fn ask_footer(
        &self,
        ui: &AskUi,
        layout: CardLayout,
        tag: &'static str,
        look: &Look,
    ) -> AnyElement {
        let c = &look.c;
        let n = ui.questions.len();
        // A question the agent didn't wait on: the answers go back as a message (2026-11-24).
        let by_message = self.conversation.questions_by_message();
        let send = if by_message {
            "Send as message  \u{23ce}"
        } else {
            "Send answers  \u{23ce}"
        };
        let (primary, hint) = match ui.current() {
            None if ui.send_anyway() => ("Send anyway  \u{23ce}", "space or click to change"),
            None => (send, "space or click to change"),
            Some(q) => {
                let primary = if n == 1 && by_message {
                    send
                } else if n == 1 {
                    "Answer  \u{23ce}"
                } else if ui.step + 1 == n {
                    "Review  \u{23ce}"
                } else {
                    "Next  \u{23ce}"
                };
                let hint = match (q.multi, n > 1) {
                    (true, true) => "space toggles \u{b7} \u{2190} \u{2192} questions",
                    (true, false) => "space toggles",
                    (false, true) => "\u{2190} \u{2192} questions",
                    (false, false) => "\u{2191}\u{2193} move \u{b7} \u{21e5} to type",
                };
                (primary, hint)
            }
        };
        // A multi-select with nothing checked and the box empty: the primary waits. So does a
        // review with nothing answered when the answers go as a message (there is none).
        let nothing =
            by_message && ui.on_review() && answer_message(&ui.questions, &ui.answers()).is_none();
        let waiting = (ui.current().is_some_and(|q| q.multi)
            && !ui.answered(ui.step)
            && self.ask.mirrored.trim().is_empty())
            || nothing;
        let primary = button(
            SharedString::from(format!("ask-primary-{tag}")),
            primary,
            !waiting,
            c,
        )
        .when(waiting, |d| {
            let tip = if nothing {
                "Answer at least one question first"
            } else {
                "Check at least one, or type an answer below"
            };
            d.text_color(hsla(c.ask_dim))
                .tooltip(move |_, cx| cx.new(|_| crate::sidebar_toggle::Tip(tip.into())).into())
        })
        .on_click(on_tab(&look.view, |this, cx| this.ask_enter_pressed(cx)));
        let armed = ui.skip_armed(look.now);
        let skip = button(
            SharedString::from(format!("ask-skip-{tag}")),
            if armed { "Esc again to skip" } else { "Skip" },
            false,
            c,
        )
        // Its key quieter than its word.
        .when(!armed, |d| {
            d.gap(px(8.))
                .child(div().text_color(hsla(c.ask_dim)).child("esc"))
        })
        .when(armed, |d| d.border_color(hsla(c.ask_edge)))
        .on_click(on_tab(&look.view, |this, cx| {
            this.apply_ask(Effect::Skip, cx)
        }));
        div()
            .flex()
            .items_center()
            .gap(px(6.))
            .text_size(px(text::META + 0.5))
            .child(primary)
            .child(skip)
            // No hint on a narrow card (the design review).
            .when(layout.hint, |d| {
                d.child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .flex()
                        .justify_end()
                        .truncate()
                        .text_size(px(text::KEYS))
                        .text_color(hsla(c.ask_dim))
                        .child(hint),
                )
            })
            .into_any_element()
    }

    /// An answered, skipped or expired question (the designer's states H–J): one quiet 22 pt
    /// line that opens to the full questions and answers, read-only.
    fn ask_summary(
        &self,
        id: &str,
        questions: &[Question],
        reply: &Reply,
        look: &Look,
    ) -> AnyElement {
        let c = &look.c;
        let open = self.ask.open.contains(id);
        let (head, rest, answers) = summary_spans(questions, reply);
        let glyph = match reply {
            Reply::Answered(_) => icon(Icon::Check, px(12.), hsla(c.ask_done)).into_any_element(),
            // In a 12 pt box, so "Skipped" lines up with "Answered" and "Not answered".
            Reply::Skipped => div()
                .flex_none()
                .size(px(12.))
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(12.))
                .text_color(hsla(c.dim))
                .child("\u{2298}")
                .into_any_element(),
            Reply::Expired => icon(Icon::Circle, px(12.), hsla(c.dim)).into_any_element(),
        };
        // Answered here: the headers dim, the answers in the text colour.
        let runs: Vec<_> = answers
            .into_iter()
            .map(|r| {
                (
                    r,
                    HighlightStyle {
                        color: Some(hsla(c.text)),
                        ..Default::default()
                    },
                )
            })
            .collect();
        let line_text = StyledText::new(rest.clone()).with_highlights(runs);
        let key = id.to_string();
        let tip = match (open, reply) {
            (true, _) => "Hide the questions".to_string(),
            (false, Reply::Expired) => self.expired_words(look),
            (false, _) => "Show the questions".to_string(),
        };
        let hover_fill = c.card_hover;
        let ink = c.text;
        let line = div()
            .id(SharedString::from(format!("ask-summary-{id}")))
            .w_full()
            .h(px(size::SUMMARY_H))
            .ml(px(-6.))
            .mr(px(-6.))
            .px(px(6.))
            .rounded(px(6.))
            .flex()
            .items_center()
            .gap(px(6.))
            .cursor_pointer()
            .text_size(px(text::META))
            .text_color(hsla(c.dim))
            .hover(move |s| s.bg(hsla(hover_fill)).text_color(hsla(ink)))
            .tooltip(move |_, cx| {
                cx.new(|_| crate::sidebar_toggle::Tip(tip.clone().into()))
                    .into()
            })
            .on_click(on_tab(&look.view, move |this, cx| {
                this.toggle_summary(&key, cx)
            }))
            .child(glyph)
            .child(
                div()
                    .flex_none()
                    .font_weight(FontWeight::MEDIUM)
                    .child(SharedString::from(format!("{head} \u{b7}"))),
            )
            .child(div().flex_1().min_w(px(0.)).truncate().child(line_text))
            .child(icon(
                if open {
                    Icon::ChevronDown
                } else {
                    Icon::ChevronRight
                },
                px(12.),
                hsla(c.dim),
            ));
        let full = open.then(|| self.ask_full(questions, reply, look));
        div()
            .w_full()
            .flex()
            .flex_col()
            .child(self.marked("ask-summary", line))
            .children(full)
            .into_any_element()
    }

    /// A closed card opened: every question, its options with what you chose checked, your
    /// own words, the chosen preview; a skipped or expired one says so at the end.
    fn ask_full(&self, questions: &[Question], reply: &Reply, look: &Look) -> AnyElement {
        let c = &look.c;
        let answers: &[QAnswer] = match reply {
            Reply::Answered(a) => a,
            _ => &[],
        };
        let row = |chosen: bool, words: String| {
            div()
                .h(px(24.))
                .flex()
                .items_center()
                .gap(px(8.))
                .text_size(px(text::UI))
                .child(if chosen {
                    icon(Icon::Check, px(12.), hsla(c.tool_ok)).into_any_element()
                } else {
                    div().flex_none().size(px(12.)).into_any_element()
                })
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .truncate()
                        .when(chosen, |d| {
                            d.font_weight(FontWeight::MEDIUM)
                                .text_color(hsla(c.panel_text))
                        })
                        .when(!chosen, |d| d.text_color(hsla(c.panel_dim)))
                        .child(SharedString::from(words)),
                )
        };
        let blocks = questions.iter().enumerate().map(|(i, q)| {
            let a = answers.get(i);
            let chosen = |label: &str| a.is_some_and(|a| a.labels.iter().any(|l| l == label));
            let other = a
                .and_then(|a| a.other.clone())
                .filter(|o| !o.trim().is_empty());
            let preview = (!q.multi)
                .then(|| {
                    q.options
                        .iter()
                        .find(|o| chosen(&o.label))
                        .and_then(|o| o.preview.clone())
                })
                .flatten()
                // Under its option's words (20 pt in), on the page with an edge: `code_fill`
                // was lost on the panel (the design review).
                .map(|p| {
                    div()
                        .mt(px(4.))
                        .ml(px(20.))
                        .px(px(size::WELL_PAD_X))
                        .py(px(size::WELL_PAD_Y))
                        .rounded(px(crate::agent_chat::chat::R_GROUP))
                        .bg(hsla(c.page))
                        .border_1()
                        .border_color(hsla(c.panel_border))
                        .font_family(look.mono.clone())
                        .text_size(px(WELL_TEXT))
                        .text_color(hsla(c.text))
                        .child(SharedString::from(
                            p.lines().take(6).collect::<Vec<_>>().join("\n"),
                        ))
                });
            div()
                .flex()
                .flex_col()
                .gap(px(2.))
                // No header: the question's own words say it (not twice).
                .when(!q.header.trim().is_empty(), |d| {
                    d.child(
                        div()
                            .text_size(px(text::KEYS))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(hsla(c.panel_dim))
                            .child(SharedString::from(header_of(q, i))),
                    )
                })
                .child(
                    div()
                        .text_size(px(text::UI))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(hsla(c.panel_text))
                        .child(SharedString::from(q.text.clone())),
                )
                .children(
                    q.options
                        .iter()
                        .map(|o| row(chosen(&o.label), shown_label(&o.label).to_string())),
                )
                .children(other.map(|o| row(true, format!("Other: \u{201c}{o}\u{201d}"))))
                .children(preview)
        });
        let footer = match reply {
            Reply::Skipped => Some("You skipped these questions.".to_string()),
            Reply::Expired => Some(self.expired_words(look)),
            Reply::Answered(_) => None,
        };
        div()
            .mt(px(8.))
            .w_full()
            .px(px(size::PAD_X))
            .py(px(size::PAD_Y))
            .rounded(px(crate::agent_chat::chat::R_CARD))
            .border_1()
            .border_color(hsla(c.panel_border))
            .bg(hsla(c.panel))
            .flex()
            .flex_col()
            .gap(px(14.))
            .children(blocks)
            .children(footer.map(|f| {
                div()
                    .text_size(px(text::META))
                    .text_color(hsla(c.panel_dim))
                    .child(SharedString::from(f))
            }))
            .into_any_element()
    }

    /// Why a question closed unanswered: the agent stopped waiting, or (one it didn't wait on)
    /// you sent another message instead.
    fn expired_words(&self, look: &Look) -> String {
        if self.conversation.questions_by_message() {
            "You sent another message instead.".to_string()
        } else {
            format!("{} stopped waiting before you answered.", look.label)
        }
    }

    /// Scenes: the open card's state.
    #[cfg(feature = "ui-shots")]
    pub fn shot_ask(&self, cx: &App) -> Option<ShotAsk> {
        let ui = self.ask.ui.as_ref()?;
        Some(ShotAsk {
            step: ui.step,
            cursor: ui.cursor,
            picks: ui.picks.clone(),
            focus: self.ask.focus,
            keys_on: self.ask_keys_on(cx),
            docked: self.ask.dock.is_some(),
            dock: self.ask.dock.map(|d| d.0),
            review: ui.on_review(),
            armed: ui.skip_armed(Instant::now()),
            anyway: ui.send_anyway(),
            lit: self.ask.lit,
        })
    }

    /// Scenes: a review row clicked.
    #[cfg(feature = "ui-shots")]
    pub fn shot_ask_review_row(&mut self, q: usize, cx: &mut Context<Self>) {
        self.ask_open_from_review(q, cx);
    }

    /// Scenes: a step chip clicked.
    #[cfg(feature = "ui-shots")]
    pub fn shot_ask_go_to(&mut self, step: usize, cx: &mut Context<Self>) {
        self.ask_go_to(step, cx);
    }

    /// Scenes: whether question `id`'s summary is open.
    #[cfg(feature = "ui-shots")]
    pub fn shot_summary_open(&self, id: &str) -> bool {
        self.ask.open.contains(id)
    }

    /// An answered summary opened or closed.
    ///
    /// The design review: the clicked line stays where it was. While the chat follows its end,
    /// a last row that grows would push its line up under the strip; so following stops and the
    /// view is scrolled to keep the row's top at its old height (following resumes, as ever,
    /// once you scroll back to the end).
    fn toggle_summary(&mut self, id: &str, cx: &mut Context<Self>) {
        if !self.ask.open.remove(id) {
            self.ask.open.insert(id.to_string());
        }
        let Some(ix) = self
            .conversation
            .items()
            .iter()
            .position(|i| matches!(i, Item::Question { id: q, .. } if q == id))
        else {
            cx.notify();
            return;
        };
        let pane = self.pane();
        let list = pane.list.clone();
        let kept = pane
            .cache
            .order()
            .binary_search(&ix)
            .ok()
            .map(|k| k + pane.head as usize)
            .and_then(|row| {
                let top = list.bounds_for_item(row)?.top();
                Some((row, f32::from(top - list.viewport_bounds().top())))
            });
        self.relayout_item(ix);
        if let Some((row, y)) = kept {
            keep_row_at(&list, row, y);
        }
        cx.notify();
    }
}

/// The list scrolled so row `row`'s top sits `y` below the view's top (it stops following the
/// end); the rows before it are as they were laid out.
pub(super) fn keep_row_at(list: &ListState, row: usize, y: f32) {
    if y <= 0. {
        list.scroll_to(gpui::ListOffset {
            item_ix: row,
            offset_in_item: px(-y),
        });
    } else {
        list.scroll_to(gpui::ListOffset {
            item_ix: row,
            offset_in_item: px(0.),
        });
        list.scroll_by(px(-y));
    }
}
