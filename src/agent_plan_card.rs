//! The Plan card (the 2026-11-25 chores: "We haven't handled agent 'Plan Mode' draft. We cannot
//! read it"; mockup A, refined on the fuxx-look mockup): the plan an agent asks you to approve,
//! rendered as the chat renders replies, folded when long, with its three answers; one quiet line
//! once answered; and, while its buttons are scrolled out of view, a slim bar over the box.

use gpui::{
    AnyElement, Context, FontWeight, InteractiveElement, IntoElement, ListState, ParentElement,
    SharedString, StatefulInteractiveElement, Styled, canvas, div, prelude::FluentBuilder, px,
};

use super::{AgentTab, Look, Refocus, button, on_tab};
use crate::agent_chat::chat;
use crate::agent_conversation::Answer;
use crate::agent_markdown::{Md, Span};
use crate::icons::{Icon, icon};
use crate::providers::PlanDraft;
use crate::terminal_view::hsla;

/// What Enter in the box does while a plan waits: empty, the highlighted answer (auto-accept,
/// Claude Code's own first choice) unless you are writing a note (`noting`), then nothing; text
/// keeps planning with it.
pub fn plan_submit(text: &str, noting: bool) -> Option<Answer> {
    let note = text.trim();
    match (note.is_empty(), noting) {
        (false, _) => Some(Answer::Denied(Some(note.to_string()))),
        (true, false) => Some(Answer::AlwaysAllowed),
        (true, true) => None,
    }
}

/// A plan's parts, for its outline chips: its `##` headings' words.
pub fn plan_outline(blocks: &[Md]) -> Vec<String> {
    blocks
        .iter()
        .filter_map(|b| match b {
            Md::Heading(2, spans) => Some(words(spans)),
            _ => None,
        })
        .collect()
}

/// How many steps a plan has: its top-level numbered items.
pub fn plan_steps(blocks: &[Md]) -> usize {
    blocks
        .iter()
        .filter(|b| matches!(b, Md::Numbered(..)))
        .count()
}

/// Whether a plan is long enough to fold: more than `FOLD_LINES` lines with words.
pub fn plan_is_long(markdown: &str) -> bool {
    markdown.lines().filter(|l| !l.trim().is_empty()).count() > FOLD_LINES
}

/// Spans' words, as read.
fn words(spans: &[Span]) -> String {
    spans
        .iter()
        .map(|s| match s {
            Span::Text(t) | Span::Code(t) | Span::Bold(t) | Span::Italic(t) => t.as_str(),
            Span::Link { text, .. } => text.as_str(),
            Span::Image { alt, .. } => alt.as_str(),
            #[allow(unreachable_patterns)]
            _ => "",
        })
        .collect()
}

/// Lines a plan shows folded.
pub const FOLD_LINES: usize = 12;

/// A folded plan's height: `FOLD_LINES` lines of the chat's text.
const FOLD_H: f32 = FOLD_LINES as f32 * chat::BODY * chat::BODY_LINE;

impl AgentTab {
    /// Plan `ix`'s card (`id`: its request's), drawn from row `k`: waiting, its buttons (or,
    /// while you write a note, what Enter does); answered or expired, one line, opening for
    /// reading.
    pub(super) fn plan_card(
        &self,
        k: usize,
        ix: usize,
        id: &str,
        plan: &PlanDraft,
        answer: Option<&Answer>,
        look: &Look,
    ) -> AnyElement {
        let c = &look.c;
        let open = self.plan_open.contains(id);
        let label = look.label.clone();
        let blocks = self.blocks_at(k, ix).unwrap_or_default();
        let toggle = {
            let id = id.to_string();
            on_tab(&look.view, move |this, cx| {
                this.toggle_plan_card(&id, ix, cx)
            })
        };
        // Answered, and not opened again: one line.
        if let (Some(answer), false) = (answer, open) {
            let approved = matches!(answer, Answer::Allowed | Answer::AlwaysAllowed);
            let steps = plan_steps(&blocks);
            return div()
                .id(SharedString::from(format!("plan-answered-{ix}")))
                .w_full()
                .px(px(14.))
                .py(px(9.))
                .rounded(px(chat::R_GROUP))
                .border_1()
                .border_color(hsla(c.panel_border))
                .bg(hsla(c.panel))
                .flex()
                .items_center()
                .gap(px(10.))
                .cursor_pointer()
                .hover(|d| d.bg(hsla(c.panel_hover)))
                .on_click(toggle)
                .child(icon(Icon::Menu, px(14.), hsla(c.accent)))
                .child(
                    div()
                        .min_w(px(0.))
                        .truncate()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(hsla(c.text))
                        .child(SharedString::from(plan.title.clone())),
                )
                .child(
                    div()
                        .flex_none()
                        .text_size(px(crate::ui_tokens::text::META))
                        .text_color(hsla(c.dim))
                        .child(SharedString::from(if steps > 0 {
                            format!("{steps} steps \u{b7} \u{25b8} show")
                        } else {
                            "\u{25b8} show".to_string()
                        })),
                )
                .child(div().flex_1())
                .child(
                    // A long note is cut, never past the card (the review's Minor 6).
                    self.marked("plan-answer-line", div())
                        .flex_shrink(1.0)
                        .min_w(px(0.))
                        .truncate()
                        .text_size(px(crate::ui_tokens::text::META))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(hsla(if approved { c.tool_ok } else { c.dim }))
                        .child(SharedString::from(super::plan_answer_line(answer))),
                )
                .into_any_element();
        }
        let waiting = answer.is_none();
        // Only the oldest waiting ask answers (the review's Important 3): a later plan waits.
        let live = waiting && plan_live(self.waiting_plan(), ix);
        let long = plan_is_long(&plan.markdown);
        let folded = long && !open;
        let outline = plan_outline(&blocks);
        let show = |words: &'static str, id: String| {
            div()
                .id(SharedString::from(id))
                .text_size(px(crate::ui_tokens::text::META))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(hsla(c.accent))
                .cursor_pointer()
                .child(words)
        };
        // The header: ☰ "<agent>'s plan", the file (↗ opens it), Show less while open.
        let file = plan.file.clone().map(|path| {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let view = look.view.clone();
            div()
                .id(SharedString::from(format!("plan-file-{ix}")))
                .flex_none()
                .flex()
                .items_center()
                .gap(px(5.))
                .px(px(8.))
                .py(px(2.))
                .rounded(px(6.))
                .bg(hsla(c.chip_fill))
                .text_size(px(crate::ui_tokens::text::META))
                .text_color(hsla(c.dim))
                .cursor_pointer()
                .hover(|d| d.text_color(hsla(c.text)))
                .child(SharedString::from(name))
                .child(icon(Icon::OpenExternal, px(12.), hsla(c.dim)))
                .on_click(move |_, _, cx| {
                    view.update(cx, |_, cx| {
                        cx.emit(super::AgentTabEvent::OpenFile(path.clone()))
                    })
                    .ok();
                })
        });
        let header = div()
            .flex()
            .items_center()
            .gap(px(10.))
            .px(px(16.))
            .pt(px(12.))
            .child(icon(Icon::Menu, px(14.), hsla(c.accent)))
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(hsla(c.accent))
                    .child(SharedString::from(format!("{label}'s plan"))),
            )
            .child(div().flex_1())
            .children(file)
            .when(long && open, |d| {
                d.child(
                    show("Show less \u{25b4}", format!("plan-less-top-{ix}"))
                        .on_click(toggle_clone(&look.view, id, ix)),
                )
            });
        let title = div()
            .px(px(16.))
            .pt(px(4.))
            .text_size(px(16.))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(hsla(c.text))
            .child(SharedString::from(plan.title.clone()));
        // Its parts, on a long plan opened: a click scrolls to one (where it falls in the plan).
        let chips = (open && outline.len() >= 3).then(|| {
            let parts: Vec<(usize, String)> = blocks
                .iter()
                .enumerate()
                .filter_map(|(i, b)| match b {
                    Md::Heading(2, spans) => Some((i, words(spans))),
                    _ => None,
                })
                .collect();
            let total = blocks.len().max(1);
            div()
                .flex()
                .flex_wrap()
                .gap(px(6.))
                .px(px(16.))
                .pt(px(8.))
                .children(parts.into_iter().map(|(i, part)| {
                    let at = i as f32 / total as f32;
                    div()
                        .id(SharedString::from(format!("plan-part-{ix}-{i}")))
                        .px(px(9.))
                        .py(px(2.))
                        .rounded_full()
                        .bg(hsla(c.chip_fill))
                        .text_size(px(crate::ui_tokens::text::META))
                        .text_color(hsla(c.dim))
                        .cursor_pointer()
                        .hover(|d| d.text_color(hsla(c.text)))
                        .on_click(on_tab(&look.view, move |this, cx| {
                            this.plan_jump(ix, at, cx)
                        }))
                        .child(SharedString::from(part))
                }))
        });
        let column = crate::md_preview::column(&blocks, ix, &look.draw);
        let page_fill = hsla(c.panel);
        let body = div()
            .relative()
            .px(px(16.))
            .pt(px(4.))
            .text_color(hsla(c.text))
            .child(column)
            .when(folded, |d| {
                d.max_h(px(FOLD_H)).overflow_hidden().child(
                    div()
                        .absolute()
                        .left_0()
                        .right_0()
                        .bottom_0()
                        .h(px(64.))
                        .bg(gpui::linear_gradient(
                            180.,
                            gpui::linear_color_stop(gpui::Hsla { a: 0., ..page_fill }, 0.),
                            gpui::linear_color_stop(page_fill, 1.),
                        )),
                )
            });
        let more = long.then(|| {
            div().px(px(16.)).pt(px(6.)).child(
                show(
                    if open {
                        "Show less \u{25b4}"
                    } else {
                        "Show the whole plan \u{25be}"
                    },
                    format!("plan-more-{ix}"),
                )
                .on_click(toggle),
            )
        });
        // The footer: the answers while it waits (aside while you write a note), else how it
        // went.
        let noting = live && self.plan_noting(cx_free_text(self));
        let footer = div()
            .mt(px(10.))
            .px(px(16.))
            .pt(px(12.))
            .pb(px(12.))
            .border_t_1()
            .border_color(hsla(c.panel_border))
            .flex()
            .flex_col()
            .gap(px(8.))
            .map(|d| match answer {
                Some(answer) => d.child(
                    div()
                        .text_size(px(crate::ui_tokens::text::META))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(hsla(c.dim))
                        .child(SharedString::from(super::plan_answer_line(answer))),
                ),
                None if noting => d.child(
                    div()
                        .text_size(px(crate::ui_tokens::text::META))
                        .text_color(hsla(c.dim))
                        .child(SharedString::from(format!(
                            "Your note goes back to {label}, and it keeps planning. Esc returns to the buttons."
                        ))),
                ),
                None if !live => d.child(
                    div()
                        .text_size(px(crate::ui_tokens::text::META))
                        .text_color(hsla(c.dim))
                        .child("Next: answer the request before it first."),
                ),
                None => d
                    .child(self.plan_buttons("plan", ix, look))
                    .child(
                        div()
                            .text_size(px(crate::ui_tokens::text::META))
                            .text_color(hsla(c.dim))
                            .child(SharedString::from(format!(
                                "Or type in the box below to tell {label} what to change in the plan."
                            ))),
                    ),
            });
        let footer = if live {
            self.plan_footer_probe(footer)
        } else {
            footer
        };
        div()
            .id(SharedString::from(format!("plan-card-{ix}")))
            .w_full()
            .rounded(px(chat::R_CARD))
            .border(px(if waiting { 1.5 } else { 1. }))
            .border_color(hsla(if waiting { c.accent } else { c.panel_border }))
            .bg(hsla(c.panel))
            .overflow_hidden()
            .flex()
            .flex_col()
            .child(header)
            .child(title)
            .children(chips)
            .child(body)
            .children(more)
            .child(footer)
            .into_any_element()
    }

    /// The three answers, for the card's footer and the pinned bar (`id` keeps them apart).
    fn plan_buttons(&self, id: &'static str, ix: usize, look: &Look) -> gpui::Div {
        let c = &look.c;
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(6.))
            .text_size(px(crate::ui_tokens::text::META + 0.5))
            .child(
                button(
                    SharedString::from(format!("{id}-approve-auto")),
                    "Approve \u{b7} auto-accept edits  \u{23ce}",
                    true,
                    c,
                )
                .on_click(on_tab(&look.view, move |this, cx| {
                    this.answer_plan(ix, Answer::AlwaysAllowed, cx);
                })),
            )
            .child(
                button(
                    SharedString::from(format!("{id}-approve-each")),
                    "Approve \u{b7} ask per edit",
                    false,
                    c,
                )
                .on_click(on_tab(&look.view, move |this, cx| {
                    this.answer_plan(ix, Answer::Allowed, cx);
                })),
            )
            .child(
                div()
                    .id(SharedString::from(format!("{id}-keep-planning")))
                    .h(px(28.))
                    .px(px(10.))
                    .flex()
                    .items_center()
                    .rounded(px(6.))
                    .text_color(hsla(c.dim))
                    .cursor_pointer()
                    .hover(|d| d.text_color(hsla(c.text)))
                    .child("Keep planning\u{2026}")
                    .on_click(on_tab(&look.view, |this, cx| this.keep_planning(cx))),
            )
    }

    /// Whether you are writing a note for a waiting plan: Keep planning was pressed, or the box
    /// has text (its buttons step aside, so Enter sends the note and can't approve).
    pub(super) fn plan_noting(&self, box_has_text: bool) -> bool {
        plan_noting_now(self.waiting_plan().is_some(), self.plan_note, box_has_text)
    }

    /// Plan `ix`'s answer, only while it is the oldest ask (the buttons' `answer` goes to that
    /// one: the review's Important 3).
    fn answer_plan(&mut self, ix: usize, answer: Answer, cx: &mut Context<Self>) {
        if self.waiting_plan() == Some(ix) {
            self.answer(answer, cx).ok();
        }
    }

    /// Keep planning…: the box takes the keyboard for your note.
    fn keep_planning(&mut self, cx: &mut Context<Self>) {
        self.plan_note = true;
        self.refocus = Some(Refocus::Box);
        self.relayout_plan();
        cx.notify();
    }

    /// The waiting plan's row laid out again (its footer changed).
    pub(super) fn relayout_plan(&self) {
        if let Some(ix) = self.waiting_plan() {
            for pane in self.panes.values() {
                if let Ok(k) = pane.cache.order().binary_search(&ix) {
                    let k = k + pane.head as usize;
                    if k < pane.list.item_count() {
                        pane.list.splice(k..k + 1, 1);
                    }
                }
            }
        }
    }

    /// The oldest ask, when it is a plan still waiting.
    pub(super) fn waiting_plan(&self) -> Option<usize> {
        let ix = self.conversation.oldest_ask()?;
        matches!(
            &self.conversation.items()[ix],
            crate::agent_conversation::Item::Permission {
                plan: Some(_),
                answer: None,
                ..
            }
        )
        .then_some(ix)
    }

    /// Show the whole plan / Show less: card `id` (item `ix`) opens or folds. Opening keeps
    /// its top where it was (you read on down; its buttons go below the view and the pinned
    /// bar takes them); folding brings its top back into view.
    fn toggle_plan_card(&mut self, id: &str, ix: usize, cx: &mut Context<Self>) {
        let folding = self.plan_open.remove(id);
        if !folding {
            self.plan_open.insert(id.to_string());
        }
        for pane in self.panes.values() {
            if let Ok(k) = pane.cache.order().binary_search(&ix) {
                let k = k + pane.head as usize;
                if k < pane.list.item_count() {
                    let list = pane.list.clone();
                    let top = list
                        .bounds_for_item(k)
                        .map(|b| f32::from(b.top() - list.viewport_bounds().top()));
                    list.splice(k..k + 1, 1);
                    if let (false, Some(y)) = (folding, top) {
                        super::ask_view::keep_row_at(&list, k, y);
                    } else {
                        list.scroll_to(gpui::ListOffset {
                            item_ix: k,
                            offset_in_item: px(0.),
                        });
                    }
                }
            }
        }
        cx.notify();
    }

    /// An outline chip: the chat scrolls to that part, `at` of the way down the plan's row.
    fn plan_jump(&mut self, ix: usize, at: f32, cx: &mut Context<Self>) {
        let pane = self.pane();
        if let Ok(k) = pane.cache.order().binary_search(&ix) {
            let k = k + pane.head as usize;
            let list = pane.list.clone();
            let h = list.bounds_for_item(k).map_or(px(0.), |b| b.size.height);
            list.scroll_to(gpui::ListOffset {
                item_ix: k,
                offset_in_item: h * at,
            });
            cx.notify();
        }
    }

    /// The footer, its painted place noted (the pinned bar shows while it is out of view).
    fn plan_footer_probe(&self, footer: gpui::Div) -> gpui::Div {
        let (seen, frame) = (self.plan_footer.clone(), self.plan_frame.get());
        footer.child(
            canvas(
                move |bounds, _, _| seen.set(Some((bounds, frame))),
                |_, _, _, _| {},
            )
            .absolute()
            .size_full(),
        )
    }

    /// The slim answer bar pinned over the box while the waiting plan's footer is out of the
    /// chat's view (the 2026-11-25 chores, "loved the idea"): ☰, its title, the three answers.
    pub(super) fn plan_bar(&mut self, look: &Look, cx: &mut Context<Self>) -> Option<AnyElement> {
        let ix = self.waiting_plan()?;
        if self.plan_noting(self.box_text_nonempty.get()) {
            return None;
        }
        let title = match &self.conversation.items()[ix] {
            crate::agent_conversation::Item::Permission {
                plan: Some(plan), ..
            } => plan.title.clone(),
            _ => return None,
        };
        let list: ListState = self.pane().list.clone();
        let footer = self.plan_footer.clone();
        // Painted in the frame just drawn, inside the view (the review's Important 5): the
        // last frame's place now, this frame's once the list has laid out (it comes first).
        let frame = self.plan_frame.get();
        let out = move |list: &ListState, painted: u64| -> bool {
            let view = list.viewport_bounds();
            view.size.height > px(0.) && footer_out(footer.get(), painted, view)
        };
        let shown = out(&list, frame.saturating_sub(1));
        let tab = cx.entity_id();
        let check = canvas(
            move |_, window, _| {
                if out(&list, frame) != shown {
                    window.on_next_frame(move |_, cx| cx.notify(tab));
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_0();
        let layer = div().absolute().left_0().right_0().bottom_0().child(check);
        if !shown {
            return Some(layer.into_any_element());
        }
        let c = &look.c;
        let bar = div()
            .w_full()
            .max_w(px(chat::COLUMN_MAX))
            .flex()
            .items_center()
            .gap(px(10.))
            .px(px(12.))
            .py(px(8.))
            .rounded(px(chat::R_GROUP))
            .border(px(1.5))
            .border_color(hsla(c.accent))
            .bg(hsla(c.panel))
            .child(icon(Icon::Menu, px(14.), hsla(c.accent)))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .truncate()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(hsla(c.text))
                    .child(SharedString::from(title)),
            )
            .child(self.plan_buttons("plan-bar", ix, look));
        Some(
            layer
                .block_mouse_except_scroll()
                .child(
                    div()
                        .w_full()
                        .px(px(super::AREA_X))
                        .pb(px(8.))
                        .flex()
                        .justify_center()
                        .child(bar),
                )
                .into_any_element(),
        )
    }
}

/// The scenes' hooks.
#[cfg(feature = "ui-shots")]
impl AgentTab {
    /// Open the waiting plan's card whole (Show the whole plan), its end in view.
    pub fn shot_open_plan(&mut self, cx: &mut Context<Self>) {
        let Some(ix) = self.waiting_plan() else {
            return;
        };
        let id = match &self.conversation.items()[ix] {
            crate::agent_conversation::Item::Permission { id, .. } => id.clone(),
            _ => return,
        };
        self.toggle_plan_card(&id, ix, cx);
    }

    /// Whether the pinned answer bar shows (its footer out of view).
    pub fn shot_plan_bar(&self) -> bool {
        let list = &self.pane().list;
        let view = list.viewport_bounds();
        self.waiting_plan().is_some()
            && footer_out(self.plan_footer.get(), self.plan_frame.get(), view)
    }
}

/// Where the waiting plan's footer was painted, and in which frame (the pinned bar's cue).
pub type FooterSeen = std::rc::Rc<std::cell::Cell<Option<(gpui::Bounds<gpui::Pixels>, u64)>>>;

/// Whether you are writing a note for a plan: one waits (`waiting`), and Keep planning... was
/// pressed (`note`) or the box has text (the review's Important 1 and 2: a note left from a plan
/// that went counts for nothing).
pub fn plan_noting_now(waiting: bool, note: bool, box_has_text: bool) -> bool {
    waiting && (note || box_has_text)
}

/// Whether plan `ix`'s card answers: it is the oldest waiting ask.
pub fn plan_live(oldest_waiting_plan: Option<usize>, ix: usize) -> bool {
    oldest_waiting_plan == Some(ix)
}

/// Whether the footer is out of the chat's `view`: not painted in frame `frame`, or painted
/// outside it.
pub fn footer_out(
    seen: Option<(gpui::Bounds<gpui::Pixels>, u64)>,
    frame: u64,
    view: gpui::Bounds<gpui::Pixels>,
) -> bool {
    match seen {
        Some((f, at)) if at == frame => f.bottom() > view.bottom() || f.top() < view.top(),
        _ => true,
    }
}

/// A click that opens or folds card `id`.
fn toggle_clone(
    view: &gpui::WeakEntity<AgentTab>,
    id: &str,
    ix: usize,
) -> impl Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static {
    let id = id.to_string();
    on_tab(view, move |this, cx| this.toggle_plan_card(&id, ix, cx))
}

/// Whether the box has text (the card reads it as you writing a note).
fn cx_free_text(tab: &AgentTab) -> bool {
    tab.box_text_nonempty.get()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_markdown::parse;

    /// The review's Important 1 and 2: a note is being written only while a plan waits (a
    /// Keep planning… left over from an expired plan counts for nothing), and typing counts.
    #[test]
    fn noting_needs_a_waiting_plan() {
        assert!(plan_noting_now(true, true, false), "Keep planning pressed");
        assert!(plan_noting_now(true, false, true), "typing");
        assert!(!plan_noting_now(true, false, false));
        assert!(
            !plan_noting_now(false, true, true),
            "no plan waits: a stale note is nothing"
        );
    }

    /// The review's Important 3: only the oldest waiting ask's card answers (its buttons send to
    /// that request); a later plan waits its turn.
    #[test]
    fn only_the_oldest_waiting_plan_is_live() {
        assert!(plan_live(Some(4), 4));
        assert!(!plan_live(Some(2), 4));
        assert!(!plan_live(None, 4));
    }

    /// The review's Important 5: the footer counts as in view only when it was painted in the
    /// frame just drawn, inside the chat's view.
    #[test]
    fn a_footer_not_painted_last_frame_is_out_of_view() {
        use gpui::{Bounds, point, px, size};
        let view = Bounds::new(point(px(0.), px(0.)), size(px(500.), px(400.)));
        let inside = Bounds::new(point(px(0.), px(300.)), size(px(500.), px(40.)));
        let below = Bounds::new(point(px(0.), px(390.)), size(px(500.), px(40.)));
        assert!(!footer_out(Some((inside, 7)), 7, view));
        assert!(
            footer_out(Some((inside, 6)), 7, view),
            "painted an older frame"
        );
        assert!(footer_out(Some((below, 7)), 7, view));
        assert!(footer_out(None, 7, view));
    }

    #[test]
    fn submit_on_a_plan() {
        assert_eq!(plan_submit("", false), Some(Answer::AlwaysAllowed));
        assert_eq!(plan_submit("  ", false), Some(Answer::AlwaysAllowed));
        assert_eq!(
            plan_submit("", true),
            None,
            "writing a note: Enter can't approve"
        );
        assert_eq!(
            plan_submit(" split step 2 ", false),
            Some(Answer::Denied(Some("split step 2".into())))
        );
    }

    #[test]
    fn the_outline_lists_level_two_headings() {
        let md = "# Retry\n\n## Context\nx\n\n## Steps\n1. a\n\n### Detail\n\n## Verification\n- t";
        assert_eq!(
            plan_outline(&parse(md)),
            ["Context", "Steps", "Verification"]
        );
    }

    #[test]
    fn steps_are_the_top_numbered_items() {
        let md = "# P\n\n1. a\n2. b\n   1. nested\n3. c\n\n- bullet";
        assert_eq!(plan_steps(&parse(md)), 3);
    }

    #[test]
    fn a_long_plan_folds() {
        let short = "# P\n\n1. a\n2. b";
        let long: String = (0..13).map(|i| format!("line {i}\n\n")).collect();
        assert!(!plan_is_long(short));
        assert!(plan_is_long(&long));
    }

    #[test]
    fn span_words_join() {
        assert_eq!(
            words(&[Span::Text("a ".into()), Span::Code("b".into())]),
            "a b"
        );
    }
}
