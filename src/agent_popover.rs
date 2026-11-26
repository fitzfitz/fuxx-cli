//! One look for the popovers over the agent box (chore 36, note 93: "the fonts the gaps, the
//! sizes on the subagent, shell, tasks popover is cooler"): the subagents, shells and plan
//! popovers over the corner tabs, and the status line's changed files and checks. A small dim
//! header with its count on the right, compact 26 pt rows with a 14 pt mark cell, mono only for
//! commands and paths, and the same status marks.

use alacritty_terminal::vte::ansi::Rgb;
use gpui::{AnyElement, Context, Div, IntoElement, ParentElement, SharedString, Styled, div, px};

use super::{AgentTab, Colors};
use crate::agent_box::menus::MenuPlace;
use crate::terminal_view::hsla;
use crate::theme::UiColors;

/// A row's height, its room left and right, the gap between its cells and its corners.
pub(crate) const ROW_H: f32 = 26.0;
pub(crate) const PAD_X: f32 = 8.0;
const GAP: f32 = 8.0;
const RADIUS: f32 = 6.0;
/// The mark cell's width, and the room a row under a folder is set in by.
const MARK_W: f32 = 14.0;
pub(crate) const INDENT: f32 = MARK_W + GAP;
/// Text sizes: a row's words, a command's or a path's (mono reads larger), the header and the
/// footer, and a row's tail (a time, a count, an outcome).
const TEXT: f32 = 12.5;
const MONO: f32 = 11.5;
const SMALL: f32 = 11.5;
const TAIL: f32 = 11.0;

/// A row's status mark.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Mark {
    Done,
    Failed,
    /// Ended by you or the agent (a stopped shell).
    Stopped,
    Building,
    Next,
    /// Ran, and nothing says how it went (a check piped through `tail`).
    NoVerdict,
}

impl Mark {
    pub(crate) fn glyph(self) -> &'static str {
        match self {
            Mark::Done => "\u{2713}",
            Mark::Failed | Mark::Stopped => "\u{2715}",
            Mark::Building => "\u{25d0}",
            Mark::Next => "\u{25cb}",
            Mark::NoVerdict => "\u{2013}",
        }
    }

    fn ink(self, c: &Colors, ui: &UiColors) -> Rgb {
        match self {
            Mark::Done => c.tool_ok,
            Mark::Failed => c.failed_panel,
            Mark::Building => c.todo_active,
            Mark::Stopped | Mark::Next | Mark::NoVerdict => ui.text_dim,
        }
    }
}

/// `mark`'s glyph in its ink.
pub(crate) fn mark(m: Mark, c: &Colors, ui: &UiColors) -> AnyElement {
    div()
        .text_size(px(12.))
        .text_color(hsla(m.ink(c, ui)))
        .child(m.glyph())
        .into_any_element()
}

/// The mark's cell at a row's left: `MARK_W` wide, its mark centred.
pub(crate) fn mark_cell(el: impl IntoElement) -> Div {
    div()
        .flex_none()
        .w(px(MARK_W))
        .flex()
        .justify_center()
        .child(el)
}

/// A row: `ROW_H` high, its cells in a `line`.
pub(crate) fn row() -> Div {
    line().h(px(ROW_H))
}

/// A row's cells in a line, as high as they are (a shell's first line, with two under it).
pub(crate) fn line() -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(GAP))
        .px(px(PAD_X))
        .rounded(px(RADIUS))
}

/// A row's words, taking the room left and cut with "…".
pub(crate) fn title(text: impl Into<SharedString>, ink: Rgb) -> Div {
    div()
        .flex_1()
        .min_w(px(0.))
        .truncate()
        .text_color(hsla(ink))
        .child(text.into())
}

/// `title` for a command or a path: in the terminal's font.
pub(crate) fn mono(text: impl Into<SharedString>, ink: Rgb, family: SharedString) -> Div {
    title(text, ink).font_family(family).text_size(px(MONO))
}

/// A row's tail: a time, a count or an outcome, never squeezed.
pub(crate) fn tail(text: impl Into<SharedString>, ink: Rgb) -> Div {
    div()
        .flex_none()
        .font_features(crate::sidebar::tabular())
        .text_size(px(TAIL))
        .text_color(hsla(ink))
        .child(text.into())
}

/// The header: `title` on the left, `count` on the right, small and dim.
pub(crate) fn header(title: &str, count: Option<AnyElement>, ui: &UiColors) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(GAP))
        .px(px(PAD_X))
        .pt(px(6.))
        .pb(px(2.))
        .text_size(px(SMALL))
        .text_color(hsla(ui.text_dim))
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .truncate()
                .child(title.to_string()),
        )
        .children(count.map(|count| {
            div()
                .flex_none()
                .font_features(crate::sidebar::tabular())
                .child(count)
        }))
}

/// A header's count in words.
pub(crate) fn count(text: impl Into<SharedString>) -> AnyElement {
    div().child(text.into()).into_any_element()
}

/// A quiet line (the footer's "Click a subagent to open it", an empty list's words).
pub(crate) fn quiet(words: impl Into<SharedString>, ui: &UiColors) -> Div {
    div()
        .px(px(PAD_X))
        .py(px(6.))
        .text_size(px(SMALL))
        .text_color(hsla(ui.text_dim))
        .child(words.into())
}

impl AgentTab {
    /// A popover in the one look: `content` (a header, rows, a footer) in a column, probed as
    /// `part`, hung by `place` in the menus' frame.
    pub(crate) fn popover_frame(
        &self,
        part: &'static str,
        content: Vec<AnyElement>,
        width: f32,
        place: MenuPlace,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let column = div()
            .flex()
            .flex_col()
            .text_size(px(TEXT))
            .children(content);
        let content = self.marked(part, div().child(column)).into_any_element();
        self.menu_frame_at(Some(content), None, width, place, cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_marks_are_the_corner_popovers() {
        assert_eq!(Mark::Done.glyph(), "\u{2713}");
        assert_eq!(Mark::Failed.glyph(), "\u{2715}");
        assert_eq!(Mark::Stopped.glyph(), "\u{2715}");
        assert_eq!(Mark::Building.glyph(), "\u{25d0}");
        assert_eq!(Mark::Next.glyph(), "\u{25cb}");
        assert_eq!(Mark::NoVerdict.glyph(), "\u{2013}");
    }

    /// The five popovers draw through this module: none hangs its own frame or sets its own row
    /// height (chore 19's deferred "the subagents and shells popovers repeat one frame shape").
    #[test]
    fn the_five_popovers_share_one_look() {
        let plan = include_str!("agent_box_menus.rs");
        let plan = &plan[plan
            .find("fn plan_popover")
            .expect("plan_popover in agent_box_menus.rs")..];
        let plan = &plan[..plan.find("\n    }\n").expect("its end")];
        for (name, text) in [
            (
                "agent_subagents_view.rs",
                include_str!("agent_subagents_view.rs"),
            ),
            ("agent_shells_view.rs", include_str!("agent_shells_view.rs")),
            ("agent_ledger_view.rs", include_str!("agent_ledger_view.rs")),
            ("plan_popover", plan),
        ] {
            for own in ["menu_frame_at(", "h(px(26.", "h(px(ROW_H", ".h(px(24."] {
                assert!(!text.contains(own), "{name} draws its own {own}");
            }
        }
    }
}
