//! The Plan card (the 2026-11-25 chores: "We haven't handled agent 'Plan Mode' draft. We cannot
//! read it"; mockup A, refined on the fuxx-look mockup): the plan an agent asks you to approve,
//! rendered as the chat renders replies, folded when long, with its three answers; one quiet line
//! once answered; and, while its buttons are scrolled out of view, a slim pill over the box
//! (chore 23: smaller and tighter).

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

/// The card's padding (chore 23: "don't make too much spaces").
const PAD: f32 = 12.0;

impl AgentTab {
    /// Plan `ix`'s card (`id`: its request's), drawn from row `k` (chore 23: the chat's text
    /// size, 12 pt in, one header line): waiting, its answers in one row (or, while you write a
    /// note, what Enter does); answered or expired, one line — folded even if it was open —
    /// that opens it again to read only.
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
        let label = look.label.clone();
        let blocks = self.blocks_at(k, ix).unwrap_or_default();
        let steps = plan_steps(&blocks);
        let toggle = {
            let id = id.to_string();
            on_tab(&look.view, move |this, cx| {
                this.toggle_plan_card(&id, ix, cx)
            })
        };
        let meta = crate::ui_tokens::text::META;
        // Answered, and not opened again: one line.
        let reopened = self.plan_reopened.contains(id);
        if let (Some(answer), false) = (answer, reopened) {
            let approved = answer.allows();
            let el = div()
                .id(SharedString::from(format!("plan-answered-{ix}")))
                .w_full()
                .px(px(PAD))
                .py(px(7.))
                .rounded(px(chat::R_GROUP))
                .border_1()
                .border_color(hsla(c.panel_border))
                .bg(hsla(c.panel))
                .flex()
                .items_center()
                .gap(px(8.))
                .text_size(px(meta))
                .cursor_pointer()
                .hover(|d| d.bg(hsla(c.panel_hover)))
                .on_click(toggle)
                .child(icon(Icon::Menu, px(13.), hsla(c.accent)))
                .child(
                    // A long note is cut, never past the card (the review's Minor 6).
                    self.marked("plan-answer-line", div())
                        .flex_shrink(1.0)
                        .min_w(px(0.))
                        .truncate()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(hsla(if approved { c.tool_ok } else { c.dim }))
                        .child(SharedString::from(super::plan_answer_line(answer))),
                )
                .child(div().flex_1())
                .child(
                    self.marked("plan-show", div())
                        .flex_none()
                        .text_color(hsla(c.dim))
                        .child("Show plan \u{203a}"),
                );
            return self.marked("plan-answered", el).into_any_element();
        }
        let waiting = answer.is_none();
        // Opened again after its answer, it reads only: whole, no answers under it.
        let open = !waiting || self.plan_open.contains(id);
        // Only the oldest waiting ask answers (the review's Important 3): a later plan waits.
        let live = waiting && plan_live(self.waiting_plan(), ix);
        let long = plan_is_long(&plan.markdown);
        let folded = long && !open;
        let outline = plan_outline(&blocks);
        let link = |words: &'static str, id: String| {
            div()
                .id(SharedString::from(id))
                .flex_none()
                .text_size(px(meta))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(hsla(c.accent))
                .cursor_pointer()
                .child(words)
        };
        let dot = || div().flex_none().text_color(hsla(c.dim)).child("\u{b7}");
        // The header, one line: ☰ "<agent>'s plan" · the file (↗ opens it) · its steps, and
        // Show less while open.
        let file = plan.file.clone().map(|path| {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let view = look.view.clone();
            div()
                .id(SharedString::from(format!("plan-file-{ix}")))
                .flex_shrink(1.)
                .min_w(px(0.))
                .flex()
                .items_center()
                .gap(px(4.))
                .text_color(hsla(c.dim))
                .cursor_pointer()
                .hover(|d| d.text_color(hsla(c.text)))
                .child(div().truncate().child(SharedString::from(name)))
                .child(icon(Icon::OpenExternal, px(11.), hsla(c.dim)))
                .on_click(move |_, _, cx| {
                    view.update(cx, |_, cx| {
                        cx.emit(super::AgentTabEvent::OpenFile(path.clone()))
                    })
                    .ok();
                })
        });
        let steps_el = (steps > 0).then(|| {
            self.marked(
                "plan-header-steps",
                div()
                    .flex_none()
                    .text_color(hsla(c.dim))
                    .child(SharedString::from(match steps {
                        1 => "1 step".to_string(),
                        n => format!("{n} steps"),
                    })),
            )
        });
        let header = self.marked(
            "plan-header",
            div()
                .h(px(24.))
                .flex()
                .items_center()
                .gap(px(6.))
                .px(px(PAD))
                .mt(px(8.))
                .text_size(px(meta))
                .child(icon(Icon::Menu, px(13.), hsla(c.accent)))
                .child(
                    div()
                        .flex_none()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(hsla(c.accent))
                        .child(SharedString::from(format!("{label}'s plan"))),
                )
                .when(file.is_some(), |d| d.child(dot()))
                .children(file)
                .when(steps_el.is_some(), |d| d.child(dot()))
                .children(steps_el)
                .child(div().flex_1())
                // Opened again after its answer, Show less (back to its line) whatever its
                // length.
                .when((long && open) || reopened, |d| {
                    d.child(
                        self.marked(
                            "plan-less-top",
                            link("Show less \u{25b4}", format!("plan-less-top-{ix}"))
                                .on_click(toggle_clone(&look.view, id, ix)),
                        ),
                    )
                }),
        );
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
                .px(px(PAD))
                .pt(px(6.))
                .children(parts.into_iter().map(|(i, part)| {
                    let at = i as f32 / total as f32;
                    div()
                        .id(SharedString::from(format!("plan-part-{ix}-{i}")))
                        .px(px(8.))
                        .py(px(1.))
                        .rounded_full()
                        .bg(hsla(c.chip_fill))
                        .text_size(px(meta))
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
        let body = self.marked(
            "plan-body",
            div()
                .relative()
                .px(px(PAD))
                .pt(px(4.))
                .pb(px(if waiting { 0. } else { PAD }))
                .text_color(hsla(c.text))
                .child(column)
                .when(folded, |d| {
                    d.max_h(px(FOLD_H)).overflow_hidden().child(
                        div()
                            .absolute()
                            .left_0()
                            .right_0()
                            .bottom_0()
                            .h(px(48.))
                            .bg(gpui::linear_gradient(
                                180.,
                                gpui::linear_color_stop(gpui::Hsla { a: 0., ..page_fill }, 0.),
                                gpui::linear_color_stop(page_fill, 1.),
                            )),
                    )
                }),
        );
        let more = (waiting && long).then(|| {
            div().px(px(PAD)).pt(px(4.)).child(
                self.marked(
                    "plan-more",
                    link(
                        if open {
                            "Show less \u{25b4}"
                        } else {
                            "Show all \u{25be}"
                        },
                        format!("plan-more-{ix}"),
                    )
                    .on_click(toggle),
                ),
            )
        });
        // The footer, while it waits: the answers (aside while you write a note).
        let noting = live && self.plan_noting(cx_free_text(self));
        let footer = waiting.then(|| {
            let footer = div()
                .mt(px(8.))
                .px(px(PAD))
                .py(px(PAD - 2.))
                .border_t_1()
                .border_color(hsla(c.panel_border))
                .text_size(px(meta))
                .text_color(hsla(c.dim))
                .map(|d| {
                    if noting {
                        d.child(SharedString::from(format!(
                            "Your note goes back to {label}, and it keeps planning. Esc returns to the buttons."
                        )))
                    } else if !live {
                        d.child("Next: answer the request before it first.")
                    } else {
                        d.child(self.marked("plan-buttons", self.plan_buttons(ix, look)))
                    }
                });
            if live {
                self.plan_footer_probe(footer)
            } else {
                footer
            }
        });
        let card = div()
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
            .children(chips)
            .child(body)
            .children(more)
            .children(footer);
        self.marked("plan-card", card).into_any_element()
    }

    /// The three answers in one row of 24 pt buttons (chore 23), for the card's footer; they
    /// wrap in a narrow pane rather than run past the card.
    fn plan_buttons(&self, ix: usize, look: &Look) -> gpui::Div {
        let id = "plan";
        let c = &look.c;
        let slim = |b: gpui::Stateful<gpui::Div>| b.h(px(24.)).px(px(10.)).flex_none();
        let approve = slim(button(
            SharedString::from(format!("{id}-approve-auto")),
            "Approve \u{b7} auto-accept  \u{23ce}",
            true,
            c,
        ))
        .on_click(on_tab(&look.view, move |this, cx| {
            this.answer_plan(ix, Answer::AlwaysAllowed, cx);
        }));
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(6.))
            .text_size(px(crate::ui_tokens::text::META))
            .child(self.marked("plan-approve-auto", approve))
            // Chore 26: approve and put the session in the provider's plan mode (Claude: Auto).
            .when_some(self.plan_into(), |d, (mode, label)| {
                let answer = Answer::ApprovedInto {
                    mode: mode.to_string(),
                    label: label.to_string(),
                };
                d.child(
                    self.marked(
                        "plan-approve-into",
                        slim(button(
                            SharedString::from(format!("{id}-approve-into")),
                            SharedString::from(format!("Approve \u{b7} {label} mode")),
                            false,
                            c,
                        ))
                        .on_click(on_tab(&look.view, move |this, cx| {
                            this.answer_plan(ix, answer.clone(), cx);
                        })),
                    ),
                )
            })
            .child(
                slim(button(
                    SharedString::from(format!("{id}-approve-each")),
                    "Approve",
                    false,
                    c,
                ))
                .on_click(on_tab(&look.view, move |this, cx| {
                    this.answer_plan(ix, Answer::Allowed, cx);
                })),
            )
            .child(
                self.marked("plan-keep-planning", div())
                    .id(SharedString::from(format!("{id}-keep-planning")))
                    .h(px(24.))
                    .px(px(8.))
                    .flex()
                    .items_center()
                    .rounded(px(6.))
                    .text_color(hsla(c.dim))
                    .cursor_pointer()
                    .hover(|d| d.text_color(hsla(c.text)))
                    .child("Keep planning")
                    .on_click(on_tab(&look.view, |this, cx| this.keep_planning(cx))),
            )
    }

    /// The mode a plan can be approved into, and its picker label: the provider's
    /// `plan_into_mode`, when it is one of its modes (chore 26).
    pub(super) fn plan_into(&self) -> Option<(&'static str, &'static str)> {
        let provider = self.provider?;
        let mode = provider.plan_into_mode()?;
        let choice = provider.info().modes.iter().find(|m| m.id == mode)?;
        Some((choice.id, choice.label))
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
            self.relayout_row(ix);
        }
    }

    /// Item `ix`'s row laid out again in every view showing it (its height changed).
    pub(super) fn relayout_row(&self, ix: usize) {
        for pane in self.panes.values() {
            if let Ok(k) = pane.cache.order().binary_search(&ix) {
                let k = k + pane.head as usize;
                if k < pane.list.item_count() {
                    pane.list.splice(k..k + 1, 1);
                }
            }
        }
    }

    /// An answered permission's line clicked: its long command opens whole, or folds to its
    /// first line (chore 23).
    pub(super) fn toggle_command(&mut self, id: &str, ix: usize, cx: &mut Context<Self>) {
        if !self.command_open.remove(id) {
            self.command_open.insert(id.to_string());
        }
        self.relayout_row(ix);
        cx.notify();
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
        // A waiting plan opens whole or folds; an answered one opens again to read, or goes
        // back to its line (chore 23).
        let answered = matches!(
            &self.conversation.items()[ix],
            crate::agent_conversation::Item::Permission {
                answer: Some(_),
                ..
            }
        );
        let set = if answered {
            &mut self.plan_reopened
        } else {
            &mut self.plan_open
        };
        let folding = set.remove(id);
        if !folding {
            set.insert(id.to_string());
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
            // Pinned to the footer's corner: left to its own place, it sat under the buttons
            // once the footer stopped being a flex column (chore 23), and the pill showed with
            // the buttons in view.
            .absolute()
            .top_0()
            .left_0()
            .size_full(),
        )
    }

    /// The slim answer pill pinned over the box while the waiting plan's footer is out of the
    /// chat's view (the 2026-11-25 chores, "loved the idea"; chore 23: a pill, not a bar).
    pub(super) fn plan_bar(&mut self, look: &Look, cx: &mut Context<Self>) -> Option<AnyElement> {
        let ix = self.waiting_plan()?;
        if self.plan_noting(self.box_text_nonempty.get()) {
            return None;
        }
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
        // Chore 23: one slim pill on the box's top edge, not the big floating bar:
        // ☰ Plan waiting · Approve ⏎ · Keep planning.
        let c = &look.c;
        let action = |id: &'static str, words: &'static str, ink| {
            div()
                .id(id)
                .flex_none()
                .h(px(22.))
                .px(px(8.))
                .flex()
                .items_center()
                .rounded_full()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(hsla(ink))
                .cursor_pointer()
                .hover(move |d| d.bg(hsla(c.panel_hover)))
                .child(words)
        };
        let dot = || div().flex_none().text_color(hsla(c.dim)).child("\u{b7}");
        let pill = div()
            .h(px(28.))
            .pl(px(10.))
            .pr(px(3.))
            .flex()
            .items_center()
            .gap(px(4.))
            .rounded_full()
            .border_1()
            .border_color(hsla(c.accent))
            .bg(hsla(c.panel))
            .text_size(px(crate::ui_tokens::text::META))
            .child(icon(Icon::Menu, px(13.), hsla(c.accent)))
            .child(
                div()
                    .flex_none()
                    .pl(px(2.))
                    .text_color(hsla(c.text))
                    .child("Plan waiting"),
            )
            .child(dot())
            .child(
                action("plan-bar-approve-auto", "Approve \u{23ce}", c.accent)
                    .on_click(on_tab(&look.view, move |this, cx| {
                        this.answer_plan(ix, Answer::AlwaysAllowed, cx)
                    })),
            )
            .child(dot())
            .child(
                action("plan-bar-keep-planning", "Keep planning", c.dim)
                    .on_click(on_tab(&look.view, |this, cx| this.keep_planning(cx))),
            );
        Some(
            layer
                .block_mouse_except_scroll()
                .child(
                    div()
                        .w_full()
                        .pb(px(6.))
                        .flex()
                        .justify_center()
                        .child(self.marked("plan-pill", pill)),
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
