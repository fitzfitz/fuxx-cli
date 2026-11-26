//! Parts the waiting cards share (chores 23, 25 and 30, note 93): the slim 24 pt answer button
//! and the fold link (its words and a Lucide chevron, never an arrow glyph), so the plan,
//! permission and question cards look one way.

use gpui::{
    Div, ElementId, Hsla, InteractiveElement, ParentElement, SharedString, Stateful, Styled, div,
    px,
};

use super::{AgentTab, Colors, button};
use crate::icons::{Icon, icon};

/// The answer buttons' height (chore 23).
pub const SLIM_H: f32 = 24.0;

/// A card's answer: `button` at `SLIM_H`, 10 pt in, never squeezed (a row of them wraps).
pub(super) fn slim_button(
    id: impl Into<ElementId>,
    words: impl Into<SharedString>,
    primary: bool,
    c: &Colors,
) -> Stateful<Div> {
    button(id, words, primary, c)
        .h(px(SLIM_H))
        .px(px(10.))
        .flex_none()
}

impl AgentTab {
    /// A fold or open link: its `words`, then `chevron` at the words' `size` in their `ink`
    /// (chore 30: Lucide's chevrons, not ▾ ▴ ›). Its chevron is the scenes' `fold-chevron`.
    pub(super) fn fold_link(
        &self,
        id: impl Into<ElementId>,
        words: impl Into<SharedString>,
        chevron: Icon,
        ink: Hsla,
        size: f32,
    ) -> Stateful<Div> {
        // Its own width, also in a column (which would stretch it across the card).
        div()
            .id(id)
            .flex_none()
            .self_start()
            .flex()
            .items_center()
            .gap(px(3.))
            .text_size(px(size))
            .text_color(ink)
            .cursor_pointer()
            .child(words.into())
            .child(self.marked(
                "fold-chevron",
                div().flex_none().child(icon(chevron, px(size), ink)),
            ))
    }
}

#[cfg(test)]
mod tests {
    /// Chore 30: no card draws its fold or open arrow as a text glyph (the keyboard legends and
    /// the answers sent to the agent keep theirs, and live elsewhere).
    #[test]
    fn the_cards_draw_no_arrow_glyphs() {
        let glyphs = [
            "\u{25be}", "\u{25b4}", "\u{203a}", "\u{25b8}", "\u{25b2}", "\u{25bc}", "\u{25b6}",
            "\u{25c2}", "\u{2039}", "\u{25b6}", "\u{25bd}", "\u{25b3}",
        ];
        let literal = [
            '\u{25be}', '\u{25b4}', '\u{203a}', '\u{25b8}', '\u{25b2}', '\u{25bc}',
        ];
        for (name, text) in [
            ("agent_plan_card.rs", include_str!("agent_plan_card.rs")),
            ("card_view.rs", include_str!("card_view.rs")),
            (
                "agent_permission_card.rs",
                include_str!("agent_permission_card.rs"),
            ),
        ] {
            for g in glyphs {
                assert!(!text.contains(g), "{name}: {g}");
            }
            for g in literal {
                assert!(!text.contains(g), "{name}: {g}");
            }
        }
    }
}
