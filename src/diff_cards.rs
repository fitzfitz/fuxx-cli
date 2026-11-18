//! Diff mode's cards (M11ah): the changed rows grouped with `CONTEXT` unchanged lines around
//! them, two runs whose context would touch sharing a card; the unchanged stretches between
//! (and before the first, after the last) folded from `FOLD_MIN` lines; each card's title.
//! Worked out off the UI thread with the diff. Pure: no GPUI.

use std::collections::HashMap;
use std::ops::Range;
use std::time::{Duration, Instant};

use similar::{ChangeTag, InlineChangeMode, InlineChangeOptions, TextDiff};

use crate::code_outline::{Outline, part_words};
use crate::diff_view::{DiffRow, RowKind};
use crate::file_tab::{LONG_LINE, thousands};

/// The unchanged lines a card keeps above and below its changes (git's usual context).
pub const CONTEXT: usize = 3;
/// An unchanged stretch folds from this many lines (a shorter one is just shown).
pub const FOLD_MIN: usize = 8;

const TOP_LEVEL: &str = "Top level";
const NEW_FILE: &str = "New file";
const DELETED_FILE: &str = "Deleted file";

/// Which changes a card holds: its lane mark's colour (green, red, blue).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CardKind {
    Added,
    Removed,
    Both,
}

/// A diff that is the whole file: every row added, or every row removed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Whole {
    New,
    Deleted,
}

/// A card: its rows (the changes with their context), the changed rows' span (first to last
/// change), how many rows it adds and removes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Card {
    pub rows: Range<usize>,
    pub changed: Range<usize>,
    pub added: usize,
    pub removed: usize,
    pub kind: CardKind,
}

/// A run of `FOLD_MIN` or more unchanged rows outside the cards, folded to one row, and the
/// new-file line it starts on (1-based; a shown one is kept over a reload by it).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stretch {
    pub rows: Range<usize>,
    pub new_start: Option<usize>,
}

/// A diff's rows grouped: its cards and its foldable stretches, in order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Grouped {
    pub cards: Vec<Card>,
    pub stretches: Vec<Stretch>,
    pub whole: Option<Whole>,
}

fn is_change(row: &DiffRow) -> bool {
    matches!(row.kind, RowKind::Added | RowKind::Removed)
}

/// `rows` grouped into cards (each run of changed rows with `CONTEXT` unchanged rows around it,
/// runs whose context would touch in one card) and the stretches of `FOLD_MIN` or more rows
/// left between them. A whole new or deleted file is one card with nothing folded.
pub fn group(rows: &[DiffRow]) -> Grouped {
    let whole = if rows.is_empty() {
        None
    } else if rows.iter().all(|r| r.kind == RowKind::Added) {
        Some(Whole::New)
    } else if rows.iter().all(|r| r.kind == RowKind::Removed) {
        Some(Whole::Deleted)
    } else {
        None
    };
    let mut cards: Vec<Card> = Vec::new();
    let mut i = 0;
    while i < rows.len() {
        if !is_change(&rows[i]) {
            i += 1;
            continue;
        }
        let start = i;
        while i < rows.len() && is_change(&rows[i]) {
            i += 1;
        }
        let context = start.saturating_sub(CONTEXT)..(i + CONTEXT).min(rows.len());
        match cards.last_mut() {
            Some(card) if context.start <= card.rows.end => {
                card.rows.end = context.end;
                card.changed.end = i;
            }
            _ => cards.push(Card {
                rows: context,
                changed: start..i,
                added: 0,
                removed: 0,
                kind: CardKind::Both,
            }),
        }
    }
    for card in &mut cards {
        let changed = &rows[card.changed.clone()];
        card.added = changed.iter().filter(|r| r.kind == RowKind::Added).count();
        card.removed = changed
            .iter()
            .filter(|r| r.kind == RowKind::Removed)
            .count();
        card.kind = match (card.added, card.removed) {
            (_, 0) => CardKind::Added,
            (0, _) => CardKind::Removed,
            _ => CardKind::Both,
        };
    }
    // The rows outside the cards, folded where long enough.
    let mut stretches = Vec::new();
    let mut from = 0;
    let ends = cards
        .iter()
        .map(|c| c.rows.clone())
        .chain(std::iter::once(rows.len()..rows.len()));
    for card in ends {
        let gap = from..card.start;
        if gap.len() >= FOLD_MIN {
            stretches.push(Stretch {
                new_start: rows[gap.clone()].iter().find_map(|r| r.new),
                rows: gap,
            });
        }
        from = card.end;
    }
    Grouped {
        cards,
        stretches,
        whole,
    }
}

/// A card's title row: the block its first change is in, its counts, and (not for a whole
/// file) the new file's lines it shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CardTitle {
    pub name: String,
    pub added: usize,
    pub removed: usize,
    pub lines: Option<(usize, usize)>,
}

impl CardTitle {
    /// What goes between the name and the counts: " · " for a whole file ("New file · +120"),
    /// else a wider space.
    pub fn separator(&self) -> &'static str {
        if self.lines.is_none() {
            " \u{b7} "
        } else {
            "  "
        }
    }

    /// The added count (`+12`; none when nothing is added).
    pub fn added_words(&self) -> Option<String> {
        (self.added > 0).then(|| format!("+{}", thousands(self.added)))
    }

    /// The removed count (`−3`; none when nothing is removed).
    pub fn removed_words(&self) -> Option<String> {
        (self.removed > 0).then(|| format!("\u{2212}{}", thousands(self.removed)))
    }

    /// The counts together: `+12 −3`.
    #[cfg(test)]
    pub fn counts_words(&self) -> String {
        [self.added_words(), self.removed_words()]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// `lines 40–58` (none for a whole file).
    pub fn lines_words(&self) -> Option<String> {
        self.lines
            .map(|(a, b)| format!("lines {}\u{2013}{}", thousands(a), thousands(b)))
    }
}

/// The name of the block holding `line` (0-based) of a side: its opener's words, "Top level"
/// outside any block (or with no outline).
fn block_name(side: Option<(&Outline, &[&str])>, line: usize, markdown: bool) -> String {
    side.and_then(|(outline, lines)| {
        let block = outline.titled(line)?;
        let start = outline.blocks[block].start;
        Some(part_words(lines.get(start)?, markdown))
    })
    .unwrap_or_else(|| TOP_LEVEL.to_string())
}

/// The innermost named block of the card's first change (new side; an only-removing card: old side),
/// "Top level" outside any; a whole new or deleted file: "New file" / "Deleted file".
pub fn title(
    card: &Card,
    rows: &[DiffRow],
    whole: Option<Whole>,
    new: Option<(&Outline, &[&str])>,
    old: Option<(&Outline, &[&str])>,
    markdown: bool,
) -> CardTitle {
    let (added, removed) = (card.added, card.removed);
    let name = match whole {
        Some(Whole::New) => {
            return CardTitle {
                name: NEW_FILE.to_string(),
                added,
                removed,
                lines: None,
            };
        }
        Some(Whole::Deleted) => {
            return CardTitle {
                name: DELETED_FILE.to_string(),
                added,
                removed,
                lines: None,
            };
        }
        None => {
            let changed = &rows[card.changed.clone()];
            match changed
                .iter()
                .find_map(|r| r.new.filter(|_| r.kind == RowKind::Added))
            {
                Some(n) => block_name(new, n - 1, markdown),
                None => match changed.iter().find_map(|r| r.old) {
                    Some(n) => block_name(old, n - 1, markdown),
                    None => TOP_LEVEL.to_string(),
                },
            }
        }
    };
    let shown = &rows[card.rows.clone()];
    let first = shown.iter().find_map(|r| r.new);
    let last = shown.iter().rev().find_map(|r| r.new);
    CardTitle {
        name,
        added,
        removed,
        lines: first.zip(last),
    }
}

/// A folded stretch's row: `⋯ 36 unchanged lines — click to show`.
pub fn unchanged_words(n: usize) -> String {
    format!(
        "\u{22ef} {} unchanged line{} \u{2014} click to show",
        thousands(n),
        if n == 1 { "" } else { "s" }
    )
}

/// Word marks give up after this long (the whole diff's, not each pair's).
pub const WORD_LIMIT: Duration = Duration::from_millis(50);

/// A card's rows side by side: a Same row with itself; each run of Removed rows then Added rows zipped in
/// order, the shorter side padded with None.
pub fn pair(rows: &[DiffRow], range: Range<usize>) -> Vec<(Option<usize>, Option<usize>)> {
    let mut out = Vec::new();
    let mut i = range.start;
    while i < range.end {
        if !is_change(&rows[i]) {
            out.push((Some(i), Some(i)));
            i += 1;
            continue;
        }
        let (mut removed, mut added) = (Vec::new(), Vec::new());
        while i < range.end && is_change(&rows[i]) {
            if rows[i].kind == RowKind::Removed {
                removed.push(i);
            } else {
                added.push(i);
            }
            i += 1;
        }
        for k in 0..removed.len().max(added.len()) {
            out.push((removed.get(k).copied(), added.get(k).copied()));
        }
    }
    out
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// The emphasised byte ranges of one side of a diff: `parts` are the inline change's (emphasised,
/// text) pieces in order, each range grown to the whole word it touches (the diff is by characters,
/// so `old` -> `fresh` is not cut mid-word), touching ranges merged.
fn grown_ranges<'a>(line: &str, parts: impl Iterator<Item = (bool, &'a str)>) -> Vec<Range<usize>> {
    let mut out: Vec<Range<usize>> = Vec::new();
    let mut at = 0;
    for (emphasised, text) in parts {
        let end = at + text.len();
        if emphasised && !text.is_empty() {
            let (mut from, mut to) = (at, end);
            if text.chars().next().is_some_and(is_word_char) {
                from = line[..from]
                    .char_indices()
                    .rev()
                    .take_while(|(_, c)| is_word_char(*c))
                    .last()
                    .map_or(from, |(i, _)| i);
            }
            if text.chars().next_back().is_some_and(is_word_char) {
                to += line[to..]
                    .char_indices()
                    .take_while(|(_, c)| is_word_char(*c))
                    .last()
                    .map_or(0, |(i, c)| i + c.len_utf8());
            }
            match out.last_mut() {
                Some(last) if from <= last.end => last.end = last.end.max(to),
                _ => out.push(from..to),
            }
        }
        at = end;
    }
    out
}

/// The changed words of each paired Removed / Added row (doc row -> byte ranges in its line), each
/// pair diffed alone by characters (`min_ratio(0.5)`: a pair sharing less than half is left
/// unmarked) and the changes grown to whole words; a pair with a line over `LONG_LINE` cells
/// skipped. None: gave up (`limit` over, whole diff's).
pub fn word_marks(
    rows: &[DiffRow],
    lines: &[&str],
    widths: &[usize],
    cards: &[Card],
    limit: Duration,
) -> Option<HashMap<usize, Vec<Range<usize>>>> {
    let deadline = Instant::now() + limit;
    let mut options = InlineChangeOptions::new();
    options.mode(InlineChangeMode::Chars).min_ratio(0.5);
    let mut marks = HashMap::new();
    for card in cards {
        for (r, a) in pair(rows, card.rows.clone()) {
            let (Some(r), Some(a)) = (r, a) else {
                continue;
            };
            if rows[r].kind != RowKind::Removed || rows[a].kind != RowKind::Added {
                continue;
            }
            if Instant::now() >= deadline {
                return None;
            }
            let (old, new) = (lines[r], lines[a]);
            if widths[r] > LONG_LINE || widths[a] > LONG_LINE {
                continue;
            }
            let diff = TextDiff::configure()
                .newline_terminated(false)
                .diff_lines(old, new);
            let (mut old_parts, mut new_parts) = (Vec::new(), Vec::new());
            for op in diff.ops() {
                for change in
                    diff.iter_inline_changes_with_options_deadline(op, options, Some(deadline))
                {
                    let side = match change.tag() {
                        ChangeTag::Delete => &mut old_parts,
                        ChangeTag::Insert => &mut new_parts,
                        ChangeTag::Equal => continue,
                    };
                    side.extend(change.values().iter().copied());
                }
            }
            for (row, line, parts) in [(r, old, old_parts), (a, new, new_parts)] {
                let ranges = grown_ranges(line, parts.into_iter());
                if !ranges.is_empty() {
                    marks.insert(row, ranges);
                }
            }
        }
    }
    (Instant::now() < deadline).then_some(marks)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code_outline::{LIMIT, outline};

    /// Rows from letters: `s` unchanged, `a` added, `r` removed, numbered as git would.
    fn rows_of(kinds: &str) -> Vec<DiffRow> {
        let (mut old, mut new) = (1, 1);
        kinds
            .chars()
            .map(|c| {
                let row = match c {
                    's' => DiffRow {
                        kind: RowKind::Same,
                        old: Some(old),
                        new: Some(new),
                    },
                    'a' => DiffRow {
                        kind: RowKind::Added,
                        old: None,
                        new: Some(new),
                    },
                    'r' => DiffRow {
                        kind: RowKind::Removed,
                        old: Some(old),
                        new: None,
                    },
                    _ => panic!("{c}"),
                };
                if row.old.is_some() {
                    old += 1;
                }
                if row.new.is_some() {
                    new += 1;
                }
                row
            })
            .collect()
    }

    fn s(n: usize) -> String {
        "s".repeat(n)
    }

    #[test]
    fn a_change_gets_three_lines_of_context() {
        // 10 unchanged, a removed and an added row, 10 unchanged.
        let rows = rows_of(&format!("{}ra{}", s(10), s(10)));
        let g = group(&rows);
        assert_eq!(g.cards.len(), 1);
        let card = &g.cards[0];
        assert_eq!(card.rows, 7..15);
        assert_eq!(card.changed, 10..12);
        assert_eq!(
            (card.added, card.removed, card.kind),
            (1, 1, CardKind::Both)
        );
        // At the file's ends the context is cut short.
        let g = group(&rows_of("as"));
        assert_eq!(g.cards[0].rows, 0..2);
        assert_eq!(g.cards[0].kind, CardKind::Added);
        assert_eq!(group(&rows_of("ssr")).cards[0].kind, CardKind::Removed);
    }

    #[test]
    fn runs_whose_context_touches_share_a_card() {
        // Gap 6: the contexts (3 + 3) meet: one card.
        let g = group(&rows_of(&format!("a{}a", s(6))));
        assert_eq!(g.cards.len(), 1);
        assert_eq!(g.cards[0].changed, 0..8);
        assert_eq!((g.cards[0].added, g.cards[0].removed), (2, 0));
        // Gap 7: two cards with one plain row between (not folded: under 8).
        let g = group(&rows_of(&format!("a{}r", s(7))));
        assert_eq!(g.cards.len(), 2);
        assert_eq!(g.cards[0].rows, 0..4);
        assert_eq!(g.cards[1].rows, 5..9);
        assert!(g.stretches.is_empty());
    }

    #[test]
    fn stretches_fold_from_eight_lines() {
        // 7 + 3 + 3 = 13 between the changes: a 7-row gap, shown.
        let g = group(&rows_of(&format!("a{}a", s(13))));
        assert_eq!(g.cards.len(), 2);
        assert!(g.stretches.is_empty());
        // 8 rows left: one stretch.
        let g = group(&rows_of(&format!("a{}a", s(14))));
        assert_eq!(
            g.stretches,
            [Stretch {
                rows: 4..12,
                new_start: Some(5)
            }]
        );
        // Before the first card and after the last too.
        let g = group(&rows_of(&format!("{}a{}", s(11), s(11))));
        assert_eq!(
            g.stretches,
            [
                Stretch {
                    rows: 0..8,
                    new_start: Some(1)
                },
                Stretch {
                    rows: 15..23,
                    new_start: Some(16)
                },
            ]
        );
        // 10 before: 7 left, shown.
        assert!(group(&rows_of(&format!("{}a", s(10)))).stretches.is_empty());
    }

    #[test]
    fn a_new_or_deleted_file_is_one_card_with_nothing_folded() {
        let g = group(&rows_of(&"a".repeat(40)));
        assert_eq!(g.whole, Some(Whole::New));
        assert_eq!(g.cards.len(), 1);
        assert_eq!(g.cards[0].rows, 0..40);
        assert_eq!(g.cards[0].added, 40);
        assert!(g.stretches.is_empty());
        let g = group(&rows_of(&"r".repeat(20)));
        assert_eq!(g.whole, Some(Whole::Deleted));
        assert_eq!((g.cards.len(), g.cards[0].removed), (1, 20));
        assert_eq!(group(&rows_of("sa")).whole, None);
        assert_eq!(group(&[]), Grouped::default());
    }

    /// A side's lines and its outline, named on `named`.
    fn side<'a>(lines: &[&'a str], named: &[usize]) -> (Outline, Vec<&'a str>) {
        let mut o = outline(lines, false, LIMIT).expect("an outline");
        o.name(named);
        (o, lines.to_vec())
    }

    #[test]
    fn titles_name_the_innermost_named_block() {
        // New lines 1..=60: `impl Session {` (named) on line 30, `fn refresh() {` (named) on
        // 35 holding an `if` block; the change's added rows on lines 43..=54.
        let mut new_lines = vec!["x"; 60];
        new_lines[29] = "impl Session {";
        new_lines[34] = "    fn refresh(&mut self) {";
        for l in &mut new_lines[35..58] {
            *l = "        if token.expired() {";
        }
        new_lines[35] = "        if token.expired() {";
        for l in &mut new_lines[36..57] {
            *l = "            go();";
        }
        new_lines[57] = "        }";
        new_lines[58] = "    }";
        new_lines[59] = "}";
        let (new_o, new_l) = side(&new_lines, &[29, 34]);
        // 42 unchanged, a removed row, 12 added (new lines 43..=54), one unchanged, 2 removed,
        // 4 unchanged: one card from new line 40 (context) to 58.
        let rows = rows_of(&format!("{}r{}srr{}", s(42), "a".repeat(12), s(4)));
        let g = group(&rows);
        let t = title(
            &g.cards[0],
            &rows,
            g.whole,
            Some((&new_o, &new_l)),
            None,
            false,
        );
        assert_eq!(t.name, "fn refresh(&mut self)");
        assert_eq!(t.counts_words(), "+12 \u{2212}3");
        assert_eq!(t.lines_words().as_deref(), Some("lines 40\u{2013}58"));
        assert_eq!(t.separator(), "  ");
    }

    #[test]
    fn a_removal_only_titles_from_the_old_side() {
        let old_lines = ["fn keep() {", "    a();", "    b();", "}", "x"];
        let (old_o, old_l) = side(&old_lines, &[0]);
        let rows = rows_of("ssrss");
        let g = group(&rows);
        let t = title(
            &g.cards[0],
            &rows,
            g.whole,
            None,
            Some((&old_o, &old_l)),
            false,
        );
        assert_eq!(t.name, "fn keep()");
        assert_eq!(t.counts_words(), "\u{2212}1");
        assert_eq!(t.lines, Some((1, 4)));
    }

    #[test]
    fn outside_any_block_reads_top_level() {
        let new_lines = ["use a;", "use b;", "fn x() {", "    y();", "}"];
        let (new_o, new_l) = side(&new_lines, &[2]);
        let rows = rows_of("sassss");
        let g = group(&rows);
        let t = title(
            &g.cards[0],
            &rows,
            g.whole,
            Some((&new_o, &new_l)),
            None,
            false,
        );
        assert_eq!(t.name, "Top level");
        // No outline at all: the same.
        let t = title(&g.cards[0], &rows, g.whole, None, None, false);
        assert_eq!(t.name, "Top level");
    }

    #[test]
    fn whole_file_titles() {
        let rows = rows_of(&"a".repeat(120));
        let g = group(&rows);
        let t = title(&g.cards[0], &rows, g.whole, None, None, false);
        let words = format!("{}{}{}", t.name, t.separator(), t.counts_words());
        assert_eq!(words, "New file \u{b7} +120");
        assert_eq!(t.lines_words(), None);
        let rows = rows_of(&"r".repeat(80));
        let g = group(&rows);
        let t = title(&g.cards[0], &rows, g.whole, None, None, false);
        let words = format!("{}{}{}", t.name, t.separator(), t.counts_words());
        assert_eq!(words, "Deleted file \u{b7} \u{2212}80");
        // Over 999: with thousands.
        let rows = rows_of(&"a".repeat(1200));
        let g = group(&rows);
        let t = title(&g.cards[0], &rows, g.whole, None, None, false);
        assert_eq!(t.counts_words(), "+1,200");
    }

    #[test]
    fn unchanged_words_say_how_many() {
        assert_eq!(
            unchanged_words(36),
            "\u{22ef} 36 unchanged lines \u{2014} click to show"
        );
        assert_eq!(
            unchanged_words(1234),
            "\u{22ef} 1,234 unchanged lines \u{2014} click to show"
        );
    }

    /// Rows for `kinds` and a line per row from `texts` (one per row, in order).
    fn marks_of(
        kinds: &str,
        texts: &[&str],
        limit: Duration,
    ) -> Option<HashMap<usize, Vec<Range<usize>>>> {
        let rows = rows_of(kinds);
        let g = group(&rows);
        let widths: Vec<usize> = texts.iter().map(|t| t.chars().count()).collect();
        word_marks(&rows, texts, &widths, &g.cards, limit)
    }

    fn marked<'a>(
        text: &'a str,
        marks: &HashMap<usize, Vec<Range<usize>>>,
        row: usize,
    ) -> Vec<&'a str> {
        marks
            .get(&row)
            .map(|v| v.iter().map(|r| &text[r.clone()]).collect())
            .unwrap_or_default()
    }

    #[test]
    fn pairs_uneven_runs_pad_the_shorter_side() {
        let rows = rows_of("srrra");
        assert_eq!(
            pair(&rows, 0..5),
            vec![
                (Some(0), Some(0)),
                (Some(1), Some(4)),
                (Some(2), None),
                (Some(3), None)
            ]
        );
        // More added than removed: padded the other way; a later run pairs on its own.
        let rows = rows_of("raasra");
        assert_eq!(
            pair(&rows, 0..6),
            vec![
                (Some(0), Some(1)),
                (None, Some(2)),
                (Some(3), Some(3)),
                (Some(4), Some(5))
            ]
        );
    }

    #[test]
    fn only_the_changed_word_is_marked() {
        let (old, new) = ("let token = old();", "let token = fresh();");
        let marks = marked_pair(old, new);
        assert_eq!(marked(old, &marks, 0), vec!["old"]);
        assert_eq!(marked(new, &marks, 1), vec!["fresh"]);
        assert_eq!(marks[&0].len(), 1);
        assert_eq!(marks[&1].len(), 1);
    }

    fn marked_pair(old: &str, new: &str) -> HashMap<usize, Vec<Range<usize>>> {
        marks_of("ra", &[old, new], WORD_LIMIT).expect("in time")
    }

    #[test]
    fn lines_sharing_under_half_their_words_get_no_marks() {
        let marks = marked_pair("fn refresh(&self) -> Token {", "return cache.lookup(key);");
        assert!(marks.is_empty(), "{marks:?}");
    }

    #[test]
    fn word_marks_sit_on_char_boundaries() {
        let (old, new) = ("let caf\u{e9} = 1;", "let caf\u{e9}s = 1;");
        let marks = marked_pair(old, new);
        for (row, text) in [(0, old), (1, new)] {
            for r in marks.get(&row).into_iter().flatten() {
                assert!(
                    text.is_char_boundary(r.start) && text.is_char_boundary(r.end),
                    "{r:?}"
                );
                assert!(r.start < r.end);
            }
        }
        // A pure insertion marks the word it grew; the old side has nothing to mark.
        assert_eq!(marked(new, &marks, 1), vec!["cafés"]);
        assert!(!marks.contains_key(&0));
        let (old, new) = ("caf\u{e9} = 1;", "caf\u{e8} = 1;");
        let marks = marked_pair(old, new);
        assert_eq!(marked(old, &marks, 0), vec!["caf\u{e9}"]);
        assert_eq!(marked(new, &marks, 1), vec!["caf\u{e8}"]);
        // A tab-indented line: the marks are byte ranges, tab untouched.
        let (old, new) = ("\t\tcall(old, 1);", "\t\tcall(fresh, 1);");
        let marks = marked_pair(old, new);
        assert_eq!(marked(old, &marks, 0), vec!["old"]);
        assert_eq!(marked(new, &marks, 1), vec!["fresh"]);
    }

    #[test]
    fn long_lines_are_not_paired() {
        let long = format!("let x = {};", "a".repeat(crate::file_tab::LONG_LINE + 10));
        let new = format!("let y = {};", "a".repeat(crate::file_tab::LONG_LINE + 10));
        let rows = rows_of("ra");
        let g = group(&rows);
        let texts = [long.as_str(), new.as_str()];
        let widths: Vec<usize> = texts.iter().map(|t| t.chars().count()).collect();
        let marks = word_marks(&rows, &texts, &widths, &g.cards, WORD_LIMIT).unwrap();
        assert!(marks.is_empty());
    }

    #[test]
    fn gave_up_gives_no_marks() {
        let texts = ["let token = old();", "let token = fresh();"];
        assert_eq!(marks_of("ra", &texts, Duration::ZERO), None);
    }
}
