//! M11ab's card view of a shell tab (note 88): finished commands as message cards painted from
//! their grid lines, the live terminal in a framed box at the bottom. Plain data here; the
//! terminal view paints and hit-tests with it.

/// Below this pane width a tab keeps today's terminal (the card's panel needs room).
pub const CARD_MIN_WIDTH: f32 = 260.0;

/// Whether a tab shows cards: the setting on, its zsh started with fuxx's marks, not on the
/// alternate screen (a full-screen program takes the whole pane), and wide enough.
pub fn card_mode(shell_cards: bool, integrated: bool, alt_screen: bool, pane_width: f32) -> bool {
    shell_cards && integrated && !alt_screen && pane_width >= CARD_MIN_WIDTH
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn card_mode_needs_all_four() {
        assert!(card_mode(true, true, false, 800.0));
        assert!(!card_mode(false, true, false, 800.0), "the setting off");
        assert!(
            !card_mode(true, false, false, 800.0),
            "a shell without the marks"
        );
        assert!(!card_mode(true, true, true, 800.0), "the alternate screen");
        assert!(!card_mode(true, true, false, 259.0), "too narrow");
        assert!(card_mode(true, true, false, CARD_MIN_WIDTH));
    }
}
