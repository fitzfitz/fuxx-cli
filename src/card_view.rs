//! M11ab's card view of a shell tab (note 88): finished commands as message cards painted from
//! their grid lines, the live terminal in a framed box at the bottom. Plain data here; the
//! terminal view paints and hit-tests with it.

use alacritty_terminal::index::Line;
use gpui::{Pixels, Size};

use crate::activity::clock_hm;
use crate::blocks::{Block, Outcome, Segment};
use crate::command_name::card_duration;
use crate::terminal::TermDims;
use crate::ui_tokens::size as ui_size;

/// Below this pane width a tab keeps today's terminal (the card's panel needs room).
pub const CARD_MIN_WIDTH: f32 = 260.0;

/// Whether a tab shows cards: the setting on, its zsh started with fuxx's marks, not on the
/// alternate screen (a full-screen program takes the whole pane), and wide enough.
pub fn card_mode(shell_cards: bool, integrated: bool, alt_screen: bool, pane_width: f32) -> bool {
    shell_cards && integrated && !alt_screen && pane_width >= CARD_MIN_WIDTH
}

/// One item of the column, top to bottom. Rows are grid lines (negative in scrollback),
/// inclusive.
#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    /// Rows above the first tagged command (a login message, a session from an older fuxx).
    Earlier { top: Line, bottom: Line },
    /// A finished command.
    Card(Card),
    /// Rows after a card that belong to no command (a background job printing later).
    Loose { top: Line, bottom: Line },
    /// The live box: the prompt shown now, or the command running now.
    Live { top: Line, bottom: Line },
}

/// A finished command as its card shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct Card {
    pub id: String,
    pub header: Header,
    /// The command the shell reported; none when it was too long (then `cmd_rows` show it).
    pub cmd: Option<String>,
    /// The command's rows as printed.
    pub cmd_rows: (Line, Line),
    pub output: Option<(Line, Line)>,
    pub failed: bool,
}

/// A card's header words: when it started, how long it ran, how it ended.
#[derive(Clone, Debug, PartialEq)]
pub struct Header {
    pub start: Option<String>,
    pub took: Option<String>,
    pub exit: Option<i32>,
}

impl Header {
    /// `10:42 · 3.2s · exit 1`: the parts it has, the exit only when not 0.
    pub fn words(&self) -> String {
        let exit = self.exit.filter(|&x| x != 0).map(|x| format!("exit {x}"));
        [self.start.clone(), self.took.clone(), exit]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" · ")
    }
}

/// A segment's first and last rows.
fn extent(segment: &Segment) -> (Line, Line) {
    match segment {
        Segment::Prompt { top, bottom, .. } => (*top, *bottom),
        Segment::Command(b) => {
            let last = b.output.map_or(b.cmd_row, |(_, end)| end.max(b.cmd_row));
            (b.prompt_top, last)
        }
    }
}

fn card(b: &Block, utc_offset: i64) -> Card {
    let cmd_end = b
        .output
        .map_or(b.cmd_row, |(start, _)| (start - 1i32).max(b.cmd_row));
    Card {
        id: b.id.clone(),
        header: Header {
            start: b.t0.map(|t| clock_hm(t as i64, utc_offset)),
            took: b.ms.map(card_duration),
            exit: match b.outcome {
                Outcome::Ok => Some(0),
                Outcome::Failed(x) => Some(x),
                Outcome::Unknown | Outcome::Running => None,
            },
        },
        cmd: b.cmd.clone(),
        cmd_rows: (b.cmd_row, cmd_end),
        output: b.output,
        failed: matches!(b.outcome, Outcome::Failed(_)),
    }
}

/// The column for a grid whose first row is `first`, its cursor on `cursor`'s row and its last
/// non-blank row `last_text` (note 88 §1, Settled 2, 3 and 5): the shown prompt or running
/// command (the last segment) and everything after it is the live box; every command before it
/// is a card; prompts before it with no command are hidden; rows between segments are loose,
/// rows before the first are earlier. With no segment, every row is the live box.
pub fn column(
    segments: &[Segment],
    first: Line,
    cursor: Line,
    last_text: Line,
    utc_offset: i64,
) -> Vec<Item> {
    let end = cursor.max(last_text);
    let Some(last) = segments.last() else {
        return vec![Item::Live {
            top: first,
            bottom: end.max(first),
        }];
    };
    // The last segment is live unless it is a finished command (its prompt not shown yet).
    let live_last = !matches!(last, Segment::Command(b) if b.outcome != Outcome::Running);
    let (before, live_top) = if live_last {
        (&segments[..segments.len() - 1], extent(last).0)
    } else {
        (segments, extent(last).1 + 1i32)
    };
    let mut items = Vec::new();
    let mut next = first;
    for segment in before {
        let (top, bottom) = extent(segment);
        if top > next {
            let gap = (next, top - 1i32);
            items.push(if items.is_empty() {
                Item::Earlier {
                    top: gap.0,
                    bottom: gap.1,
                }
            } else {
                Item::Loose {
                    top: gap.0,
                    bottom: gap.1,
                }
            });
        }
        if let Segment::Command(b) = segment {
            items.push(Item::Card(card(b, utc_offset)));
        }
        next = next.max(bottom + 1i32);
    }
    if live_top > next {
        let gap = (next, live_top - 1i32);
        items.push(if items.is_empty() {
            Item::Earlier {
                top: gap.0,
                bottom: gap.1,
            }
        } else {
            Item::Loose {
                top: gap.0,
                bottom: gap.1,
            }
        });
    }
    items.push(Item::Live {
        top: live_top,
        bottom: end.max(live_top),
    });
    items
}

/// Outputs longer than this many lines are folded (note 88 §2) …
pub const FOLD_OVER: usize = 20;
/// … to their last this many.
pub const FOLD_KEEP: usize = 12;

/// The rows of an output a card paints, and how many above them it folds away.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shown {
    pub hidden: usize,
    pub top: Line,
    pub bottom: Line,
}

pub fn shown(output: (Line, Line), unfolded: bool) -> Shown {
    let (top, bottom) = output;
    let lines = (bottom.0 - top.0 + 1).max(0) as usize;
    if unfolded || lines <= FOLD_OVER {
        return Shown {
            hidden: 0,
            top,
            bottom,
        };
    }
    Shown {
        hidden: lines - FOLD_KEEP,
        top: bottom - (FOLD_KEEP as i32 - 1),
        bottom,
    }
}

/// The chips row above the live box (folder, branch, the running command), points.
pub const CHIPS_ROW: f32 = 28.0;
/// The live box's (and a card panel's) inner padding and edge, points.
pub const BOX_PAD: f32 = 10.0;
pub const BOX_EDGE: f32 = 1.0;

/// The terminal's size in card view (note 88 §4, Settled 4): the live box's inside at its
/// tallest, the pane less its padding (`terminal_view::inset`), the chips row, and the box's
/// edge and padding on each side.
pub fn card_dims(pane: Size<Pixels>, split: bool, cell_w: f32, line_h: f32) -> TermDims {
    let (pad_x, pad_y) = if split {
        ui_size::SPLIT_TERMINAL_PADDING
    } else {
        ui_size::TERMINAL_PADDING
    };
    let frame = 2.0 * (BOX_EDGE + BOX_PAD);
    let w = f32::from(pane.width) - 2.0 * pad_x - frame;
    let h = f32::from(pane.height) - 2.0 * pad_y - CHIPS_ROW - frame;
    TermDims {
        cols: ((w / cell_w).floor().max(0.0) as usize).max(2),
        rows: ((h / line_h).floor().max(0.0) as usize).max(1),
    }
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

    use crate::blocks::{Block, Outcome, Segment};
    use alacritty_terminal::index::Line;

    /// A command's segment: its prompt on `top`, its command row the next, output rows `out`.
    fn command(id: &str, top: i32, out: Option<(i32, i32)>, outcome: Outcome) -> Segment {
        Segment::Command(Block {
            id: id.into(),
            outcome,
            prompt_top: Line(top),
            cmd_row: Line(top + 1),
            output: out.map(|(a, b)| (Line(a), Line(b))),
            cmd: Some(id.into()),
            t0: None,
            ms: None,
        })
    }

    fn prompt(id: &str, top: i32, bottom: i32) -> Segment {
        Segment::Prompt {
            id: id.into(),
            top: Line(top),
            bottom: Line(bottom),
        }
    }

    fn ids(items: &[Item]) -> Vec<String> {
        items
            .iter()
            .map(|i| match i {
                Item::Earlier { top, bottom } => format!("earlier {}..{}", top.0, bottom.0),
                Item::Card(c) => format!("card {}{}", c.id, if c.failed { " failed" } else { "" }),
                Item::Loose { top, bottom } => format!("loose {}..{}", top.0, bottom.0),
                Item::Live { top, bottom } => format!("live {}..{}", top.0, bottom.0),
            })
            .collect()
    }

    #[test]
    fn cards_for_done_commands_and_empty_prompts_hidden() {
        let segments = [
            command("ls", 0, Some((2, 3)), Outcome::Ok),
            prompt("empty", 4, 5),
            command("npm test", 6, Some((8, 9)), Outcome::Failed(1)),
            prompt("live", 10, 10),
        ];
        let items = column(&segments, Line(0), Line(10), Line(10), 0);
        assert_eq!(
            ids(&items),
            ["card ls", "card npm test failed", "live 10..10"]
        );
    }

    #[test]
    fn no_tags_is_one_live_box() {
        let items = column(&[], Line(-20), Line(3), Line(5), 0);
        assert_eq!(ids(&items), ["live -20..5"]);
    }

    #[test]
    fn a_running_command_is_in_the_live_box() {
        let segments = [
            command("ls", 0, Some((2, 2)), Outcome::Ok),
            command("npm run dev", 3, Some((5, 40)), Outcome::Running),
        ];
        let items = column(&segments, Line(0), Line(40), Line(40), 0);
        assert_eq!(ids(&items), ["card ls", "live 3..40"]);
    }

    #[test]
    fn rows_after_a_card_are_loose() {
        let segments = [
            command("ls", 0, Some((2, 2)), Outcome::Ok),
            prompt("live", 6, 6),
        ];
        let items = column(&segments, Line(0), Line(6), Line(6), 0);
        assert_eq!(ids(&items), ["card ls", "loose 3..5", "live 6..6"]);
    }

    #[test]
    fn rows_before_the_first_tag_are_earlier() {
        let segments = [prompt("live", 4, 4)];
        let items = column(&segments, Line(-2), Line(4), Line(4), 0);
        assert_eq!(ids(&items), ["earlier -2..3", "live 4..4"]);
    }

    #[test]
    fn the_live_box_ends_at_the_cursor_or_the_last_text() {
        let segments = [prompt("live", 4, 23)];
        // A menu under the prompt: text below the cursor.
        assert_eq!(
            ids(&column(&segments, Line(4), Line(4), Line(7), 0)),
            ["live 4..7"]
        );
        // Nothing under the cursor: it ends on the cursor's row.
        assert_eq!(
            ids(&column(&segments, Line(4), Line(5), Line(4), 0)),
            ["live 4..5"]
        );
    }

    #[test]
    fn header_words() {
        let full = Header {
            start: Some("10:42".into()),
            took: Some("3.2s".into()),
            exit: Some(1),
        };
        assert_eq!(full.words(), "10:42 · 3.2s · exit 1");
        let ok = Header {
            exit: Some(0),
            ..full.clone()
        };
        assert_eq!(ok.words(), "10:42 · 3.2s");
        let none = Header {
            start: None,
            took: None,
            exit: None,
        };
        assert_eq!(none.words(), "");
    }

    #[test]
    fn a_card_takes_its_times_from_its_tag() {
        let Segment::Command(mut b) = command("make", 0, Some((2, 2)), Outcome::Ok) else {
            unreachable!()
        };
        // 10:42:05 UTC, shown at UTC+7.
        b.t0 = Some(1_795_000_000 - 1_795_000_000 % 86_400 + 3 * 3600 + 42 * 60 + 5);
        b.ms = Some(3_200);
        let items = column(&[Segment::Command(b)], Line(0), Line(3), Line(3), 7 * 3600);
        let Item::Card(card) = &items[0] else {
            panic!("a card")
        };
        assert_eq!(card.header.words(), "10:42 · 3.2s");
    }

    #[test]
    fn folding_keeps_the_last_twelve() {
        let folded = shown((Line(0), Line(24)), false);
        assert_eq!(
            (folded.hidden, folded.top, folded.bottom),
            (13, Line(13), Line(24))
        );
        let short = shown((Line(0), Line(19)), false);
        assert_eq!((short.hidden, short.top), (0, Line(0)));
        let open = shown((Line(0), Line(24)), true);
        assert_eq!((open.hidden, open.top), (0, Line(0)));
    }

    #[test]
    fn card_dims_are_the_box_inside() {
        let pane = gpui::size(gpui::px(1000.0), gpui::px(700.0));
        let side = 2.0 * (BOX_EDGE + BOX_PAD);
        let (w, h) = (
            1000.0 - 2.0 * 28.0 - side,
            700.0 - 2.0 * 22.0 - CHIPS_ROW - side,
        );
        let dims = card_dims(pane, false, 8.0, 17.0);
        assert_eq!(
            (dims.cols, dims.rows),
            ((w / 8.0) as usize, (h / 17.0) as usize)
        );
        assert_eq!((dims.cols, dims.rows), (115, 35));
    }
}
