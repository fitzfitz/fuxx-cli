//! The open permission card (chore 25, note 93: "redesign this"): the tool's own description as
//! its title with the tool as a small tag, the command once (folded after `PERMISSION_LINES`),
//! the raw input only for a tool with no readable form (behind "Show input"), and the answers
//! in one row of slim buttons, a thin accent edge and the shield for attention.

use gpui::{
    AnyElement, Context, FontWeight, InteractiveElement, IntoElement, ParentElement, SharedString,
    StatefulInteractiveElement, Styled, div, prelude::FluentBuilder, px,
};

use super::card_parts::slim_button;
use super::{AgentTab, DETAIL_LINES, Look, on_tab};
use crate::agent_chat::chat;
use crate::agent_conversation::Answer;
use crate::icons::{Icon, icon};
use crate::terminal_view::hsla;
use crate::ui_tokens::text;

/// The card's padding (the plan card's, chore 23).
const PAD: f32 = 12.0;

/// What the open card shows: the request's own fields (`Item::Permission`'s).
pub(super) struct Ask<'a> {
    pub ix: usize,
    pub id: &'a str,
    pub tool: &'a str,
    pub what: &'a str,
    pub detail: &'a str,
    pub about: Option<&'a str>,
    pub always: Option<&'a str>,
    pub by: Option<&'a str>,
}

impl AgentTab {
    /// The open permission card (chore 25): title and tag, the command once (folded), the raw
    /// input only behind "Show input", and the answers in one row.
    pub(super) fn permission_card(&self, ask: Ask<'_>, look: &Look) -> AnyElement {
        let c = &look.c;
        let meta = text::META;
        let Ask { ix, id, .. } = ask;
        // A subagent's request names it (M11ar): "Reader wants to run Bash", or its tag.
        let label: SharedString = match ask.by.and_then(|b| self.conversation.lane(b)) {
            Some(lane) => lane.label.clone().into(),
            None => look.label.clone(),
        };
        let (title, tag) = match ask.about {
            Some(about) => {
                let tag = match ask.by {
                    Some(_) => format!("{label} \u{b7} {}", ask.tool),
                    None => ask.tool.to_string(),
                };
                (about.to_string(), Some(tag))
            }
            None => (format!("{label} wants to run {}", ask.tool), None),
        };
        let header = div()
            .h(px(20.))
            .flex()
            .items_center()
            .gap(px(6.))
            .child(icon(Icon::Shield, px(14.), hsla(c.accent)))
            .child(
                self.marked("permission-title", div())
                    .flex_shrink(1.)
                    .min_w(px(0.))
                    .truncate()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(hsla(c.text))
                    .child(SharedString::from(title)),
            )
            .children(tag.map(|tag| {
                self.marked("permission-tag", div())
                    .flex_none()
                    .px(px(6.))
                    .rounded(px(chat::R_CHIP))
                    .bg(hsla(c.chip_fill))
                    .text_size(px(meta))
                    .text_color(hsla(c.dim))
                    .child(SharedString::from(tag))
            }));
        // The command once, in mono at the chat's small size, folded after its first lines.
        let open = self.command_open.contains(id);
        let (shown, hidden) = fold_lines(ask.what, PERMISSION_LINES);
        let long = hidden > 0;
        let command = (!ask.what.trim().is_empty()).then(|| {
            self.marked("permission-command", div())
                .w_full()
                .px(px(10.))
                .py(px(8.))
                .rounded(px(chat::R_GROUP))
                .bg(hsla(c.code_fill))
                .font_family(look.mono.clone())
                .text_size(px(meta + 0.5))
                .text_color(hsla(c.code_text))
                .child(SharedString::from(if open {
                    ask.what.trim_end().to_string()
                } else {
                    shown
                }))
        });
        let toggle = {
            let id = id.to_string();
            on_tab(&look.view, move |this, cx| this.toggle_command(&id, ix, cx))
        };
        let more = long.then(|| {
            let (words, chevron) = if open {
                ("Show less".to_string(), Icon::ChevronUp)
            } else {
                (
                    format!(
                        "\u{2026} {hidden} more {} \u{b7} Show all",
                        if hidden == 1 { "line" } else { "lines" }
                    ),
                    Icon::ChevronDown,
                )
            };
            self.marked(
                "permission-more",
                self.fold_link(
                    SharedString::from(format!("permission-more-{ix}")),
                    words,
                    chevron,
                    hsla(c.accent),
                    meta,
                ),
            )
            .on_click(toggle)
        });
        // The raw input, only for a tool with no readable form, behind "Show input".
        let input_open = self.input_open.contains(id);
        let input = (!ask.detail.trim().is_empty()).then(|| {
            let toggle = {
                let id = id.to_string();
                on_tab(&look.view, move |this, cx| this.toggle_input(&id, ix, cx))
            };
            let mut lines: Vec<&str> = ask.detail.lines().take(DETAIL_LINES).collect();
            if ask.detail.lines().count() > DETAIL_LINES {
                lines.push("\u{2026}");
            }
            div()
                .flex()
                .flex_col()
                .gap(px(4.))
                .child(
                    self.marked(
                        "permission-input",
                        self.fold_link(
                            SharedString::from(format!("permission-input-{ix}")),
                            if input_open {
                                "Hide input"
                            } else {
                                "Show input"
                            },
                            if input_open {
                                Icon::ChevronUp
                            } else {
                                Icon::ChevronRight
                            },
                            hsla(c.dim),
                            meta,
                        ),
                    )
                    .on_click(toggle),
                )
                .when(input_open, |d| {
                    d.child(
                        self.marked("permission-input-text", div())
                            .font_family(look.mono.clone())
                            .text_size(px(meta))
                            .text_color(hsla(c.dim))
                            .child(SharedString::from(lines.join("\n"))),
                    )
                })
        });
        let answer = |answer: Answer| {
            on_tab(&look.view, move |this, cx| {
                this.answer(answer.clone(), cx).ok();
            })
        };
        let buttons = self
            .marked("permission-buttons", div())
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(6.))
            .text_size(px(meta))
            .child(
                self.marked(
                    "permission-allow",
                    slim_button("agent-allow", "Allow  \u{23ce}", true, c),
                )
                .on_click(answer(Answer::Allowed)),
            )
            .when_some(ask.always.map(str::to_string), |d, always| {
                d.child(
                    slim_button("agent-always", always, false, c)
                        .on_click(answer(Answer::AlwaysAllowed)),
                )
            })
            .child(
                slim_button("agent-deny", "Deny", false, c).on_click(answer(Answer::Denied(None))),
            )
            .child(
                self.marked("permission-hint", div())
                    .flex_none()
                    .pl(px(2.))
                    .text_color(hsla(c.dim))
                    .child("or type to redirect"),
            );
        let card = self
            .marked("permission-card", div())
            .id(SharedString::from(format!("permission-card-{ix}")))
            .w_full()
            .px(px(PAD))
            .py(px(PAD - 2.))
            .rounded(px(chat::R_CARD))
            .border(px(1.5))
            .border_color(hsla(c.accent))
            .bg(hsla(c.panel))
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(header)
            .children(command)
            .children(more)
            .children(input)
            .child(buttons);
        card.into_any_element()
    }

    /// "Show input" / "Hide input" on permission `id` (item `ix`, chore 25).
    pub(super) fn toggle_input(&mut self, id: &str, ix: usize, cx: &mut Context<Self>) {
        if !self.input_open.remove(id) {
            self.input_open.insert(id.to_string());
        }
        self.relayout_row(ix);
        cx.notify();
    }
}

/// Lines of a command the card shows before "… N more lines".
pub const PERMISSION_LINES: usize = 6;

/// `text`'s first `keep` lines, and how many more it has (a single long line is never cut:
/// it wraps).
pub fn fold_lines(text: &str, keep: usize) -> (String, usize) {
    let lines: Vec<&str> = text.lines().collect();
    let shown = lines.len().min(keep);
    (lines[..shown].join("\n"), lines.len() - shown)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_long_command_folds_after_its_first_lines() {
        assert_eq!(
            fold_lines("a\nb\nc\nd\ne\nf\ng\nh", 6),
            ("a\nb\nc\nd\ne\nf".to_string(), 2)
        );
        assert_eq!(fold_lines("cargo test", 6), ("cargo test".to_string(), 0));
        let long = "x".repeat(400);
        assert_eq!(
            fold_lines(&long, 6),
            (long.clone(), 0),
            "one line: wrapped, not cut"
        );
        // Exactly `keep` lines (a trailing newline isn't a line): nothing hidden.
        assert_eq!(
            fold_lines("1\n2\n3\n4\n5\n6\n", 6),
            ("1\n2\n3\n4\n5\n6".to_string(), 0)
        );
    }
}
