//! The text body's screen rows (M11ah): what each row shows and where it sits, for File and Diff
//! mode. Painting, clicks, selection, scrolling, the scrollbar, Find's reveal, change bars and
//! Follow all go through it; the text, line numbers, Find and Copy stay on the real lines. With
//! nothing hidden, no Wrap and no cards it is the identity (row r is line r), kept without a
//! list. A layout cache: the file tab builds it in its canvas's prepaint only when its `MapKey`
//! changes (as `Wrapped` was). Pure: no GPUI.

use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;

use crate::git_status::LineChange;
use crate::layout_model::FileMode;

/// One screen row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Row {
    /// A part of a doc line, from cell `cell0` (0: the line's first row).
    Line { line: usize, cell0: usize },
    /// Diff: a card's title row.
    Title { card: usize },
    /// Diff: a folded unchanged stretch.
    Unchanged { stretch: usize },
    /// Diff side by side: an old (line, cell0) on the left and a new one on the right.
    #[allow(dead_code)] // built by `RowMap::diff` with pairs (Task 11)
    Pair {
        old: Option<(usize, usize)>,
        new: Option<(usize, usize)>,
    },
}

/// What a map was built for (rebuilt when any differs): mode, doc version, columns, folds'
/// generation, the Diff layout's generation, side by side. (The row height is checked apart:
/// `RowMap::row_h`.)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapKey {
    pub mode: FileMode,
    pub version: u64,
    pub columns: Option<usize>,
    pub folds: u64,
    pub layout: u64,
    pub side: bool,
}

/// The screen rows of a text body: each row, each row's top in points, each line's first row.
#[derive(Clone, Debug)]
pub struct RowMap {
    rows: Rows,
    /// Wrap's cell count per row (none: Wrap off).
    columns: Option<usize>,
    /// A line's row height, in points.
    row_h: f32,
}

#[derive(Clone, Debug)]
enum Rows {
    /// Row r is line r's whole, `row_h` tall: nothing is listed.
    Identity { lines: usize },
    /// Every row listed.
    Listed {
        rows: Arc<[Row]>,
        /// Each row's top, then the height (`rows.len() + 1` long).
        tops: Arc<[f32]>,
        /// Each line's first row (none: hidden).
        first: Arc<[Option<usize>]>,
        /// Diff mode's cards and folded rows (none: a file's).
        diff: Option<Arc<DiffRows>>,
    },
}

/// Where a Diff map's cards and folds sit.
#[derive(Clone, Debug, Default)]
struct DiffRows {
    /// Each card's rows: its title's, to one past its last.
    cards: Vec<Range<usize>>,
    /// Each run of hidden lines (a folded stretch, a closed card) and the row drawn for it, in
    /// order.
    held: Vec<(Range<usize>, usize)>,
}

/// Each row's top for rows `heights` tall, then the height: summed in f64 (exact for any real
/// row count), each top the f32 nearest it, as the identity's `row_h × r` is.
fn tops_of(heights: impl Iterator<Item = f32>) -> Arc<[f32]> {
    let mut tops = Vec::new();
    let mut y = 0.0_f64;
    for h in heights {
        tops.push(y as f32);
        y += f64::from(h);
    }
    tops.push(y as f32);
    tops.into()
}

/// Line `line`'s rows, `width` cells wide, cut at `columns` (none: one row).
fn line_rows(line: usize, width: usize, columns: Option<usize>) -> impl Iterator<Item = Row> {
    let (count, step) = match columns {
        Some(c) => (width.div_ceil(c).max(1), c),
        None => (1, 0),
    };
    (0..count).map(move |k| Row::Line {
        line,
        cell0: k * step,
    })
}

impl RowMap {
    /// A file's rows: each shown line (`hidden` ranges left out) cut at `columns` cells when
    /// wrapping (a line takes at least one row, an empty one too; no room at all is one column).
    pub fn file(
        widths: &[usize],
        columns: Option<usize>,
        hidden: &[Range<usize>],
        row_h: f32,
    ) -> RowMap {
        let lines = widths.len();
        let columns = columns.map(|c| c.max(1));
        if columns.is_none() && hidden.iter().all(|h| h.start >= h.end.min(lines)) {
            return RowMap {
                rows: Rows::Identity { lines },
                columns,
                row_h,
            };
        }
        let mut shown = vec![true; lines];
        for h in hidden {
            if let Some(part) = shown.get_mut(h.start.min(lines)..h.end.min(lines)) {
                part.fill(false);
            }
        }
        let mut rows = Vec::with_capacity(lines);
        let mut first = vec![None; lines];
        for (line, &width) in widths.iter().enumerate() {
            if !shown[line] {
                continue;
            }
            first[line] = Some(rows.len());
            rows.extend(line_rows(line, width, columns));
        }
        let tops = tops_of(std::iter::repeat_n(row_h, rows.len()));
        RowMap {
            rows: Rows::Listed {
                rows: rows.into(),
                tops,
                first: first.into(),
                diff: None,
            },
            columns,
            row_h,
        }
    }

    /// How many rows.
    pub fn len(&self) -> usize {
        match &self.rows {
            Rows::Identity { lines } => *lines,
            Rows::Listed { rows, .. } => rows.len(),
        }
    }

    /// Row `r` (none past the last).
    pub fn row(&self, r: usize) -> Option<Row> {
        match &self.rows {
            Rows::Identity { lines } => (r < *lines).then_some(Row::Line { line: r, cell0: 0 }),
            Rows::Listed { rows, .. } => rows.get(r).copied(),
        }
    }

    /// Row `r`'s top, in points down the text; past the last row: the height.
    pub fn top(&self, r: usize) -> f32 {
        match &self.rows {
            Rows::Identity { lines } => self.row_h * r.min(*lines) as f32,
            Rows::Listed { tops, .. } => tops[r.min(tops.len() - 1)],
        }
    }

    /// The rows' height, in points.
    pub fn height(&self) -> f32 {
        self.top(self.len())
    }

    /// Wrap's cells per row (none: Wrap off).
    pub fn columns(&self) -> Option<usize> {
        self.columns
    }

    /// The row height a line's row was given (the map is made again for another).
    pub fn row_h(&self) -> f32 {
        self.row_h
    }

    /// The row drawn at `y` points down the text (above the first: the first; below the last:
    /// the last).
    pub fn row_at(&self, y: f32) -> usize {
        let last = self.len().saturating_sub(1);
        let r = match &self.rows {
            Rows::Identity { .. } => (y / self.row_h).floor().max(0.0) as usize,
            Rows::Listed { tops, .. } => tops[..tops.len() - 1]
                .partition_point(|&t| t <= y)
                .saturating_sub(1),
        };
        r.min(last)
    }

    /// The rows drawn (wholly or partly) between `y.start` and `y.end` points down the text.
    pub fn rows_in(&self, y: Range<f32>) -> Range<usize> {
        let len = self.len();
        let end = match &self.rows {
            Rows::Identity { .. } => ((y.end / self.row_h).ceil().max(0.0) as usize).min(len),
            Rows::Listed { tops, .. } => tops[..len].partition_point(|&t| t < y.end),
        };
        let start = if y.start >= self.height() {
            len
        } else {
            self.row_at(y.start)
        };
        start.min(end)..end
    }

    /// The hidden lines right after shown line `line` (a fold's: empty when none, and for a
    /// hidden line or one past the end). Read while drawing a folded line's row: a few rows'
    /// look, never a scan of the lines.
    pub fn hidden_after(&self, line: usize) -> Range<usize> {
        let none = line + 1..line + 1;
        let Rows::Listed { rows, first, .. } = &self.rows else {
            return none;
        };
        let Some(Some(mut r)) = first.get(line).copied() else {
            return none;
        };
        // Past the line's own rows (Wrap: a few), to the next shown line.
        while matches!(rows.get(r), Some(Row::Line { line: l, .. }) if *l == line) {
            r += 1;
        }
        match rows.get(r) {
            Some(Row::Line { line: next, .. }) => line + 1..*next,
            Some(_) => none,
            None => line + 1..first.len(),
        }
    }

    /// Line `line`'s first row (none: hidden). Past the last line: the row count (the edge
    /// after the last row, where lines deleted at the end are marked).
    pub fn first_row(&self, line: usize) -> Option<usize> {
        match &self.rows {
            Rows::Identity { lines } => Some(line.min(*lines)),
            Rows::Listed { rows, first, .. } => match first.get(line) {
                Some(row) => *row,
                None => Some(rows.len()),
            },
        }
    }
}

/// A card's rows side by side (Task 11): each an old and a new row, either may be none.
pub type CardPairs = Vec<(Option<usize>, Option<usize>)>;

/// Diff mode's layout (M11ah): the cards and stretches, which stretches are shown and which
/// cards closed, and (side by side, Task 11) each card's rows paired.
pub struct DiffLayout<'a> {
    pub grouped: &'a crate::diff_cards::Grouped,
    /// Per stretch: shown whole (else one `⋯` row).
    pub shown: &'a [bool],
    /// Per card: closed to its title.
    pub closed: &'a [bool],
    #[allow(dead_code)] // side by side (M11ah Task 11)
    pub pairs: Option<&'a [CardPairs]>,
}

impl RowMap {
    /// Diff mode's rows (M11ah): the rows outside the cards as lines, a folded stretch one `⋯`
    /// row (unless `shown`), each card a title row `code::CARD_TITLE` tall (`code::CARD_GAP`
    /// further down for every card but the first) and its lines (none when `closed`); lines cut
    /// at `columns` cells when wrapping.
    pub fn diff(
        widths: &[usize],
        columns: Option<usize>,
        layout: &DiffLayout,
        row_h: f32,
    ) -> RowMap {
        use crate::ui_tokens::code::{CARD_GAP, CARD_TITLE};
        let columns = columns.map(|c| c.max(1));
        let lines = widths.len();
        let g = layout.grouped;
        let mut rows: Vec<Row> = Vec::with_capacity(lines + g.cards.len());
        let mut heights: Vec<f32> = Vec::with_capacity(lines + g.cards.len());
        let mut first = vec![None; lines];
        let mut diff = DiffRows::default();
        let (mut card, mut stretch) = (0, 0);
        let mut line = 0;
        let push = |rows: &mut Vec<Row>, heights: &mut Vec<f32>, row: Row, h: f32| {
            rows.push(row);
            heights.push(h);
        };
        while line < lines {
            if let Some(c) = g.cards.get(card).filter(|c| c.rows.start == line) {
                if card > 0
                    && let Some(h) = heights.last_mut()
                {
                    // The gap above it: the row before it reaches down to its title.
                    *h += CARD_GAP;
                }
                let title = rows.len();
                push(&mut rows, &mut heights, Row::Title { card }, CARD_TITLE);
                let end = c.rows.end.min(lines);
                if layout.closed.get(card).copied().unwrap_or(false) {
                    diff.held.push((line..end, title));
                } else {
                    for l in line..end {
                        first[l] = Some(rows.len());
                        for row in line_rows(l, widths[l], columns) {
                            push(&mut rows, &mut heights, row, row_h);
                        }
                    }
                }
                diff.cards.push(title..rows.len());
                (line, card) = (end, card + 1);
                continue;
            }
            if let Some(s) = g.stretches.get(stretch).filter(|s| s.rows.start == line) {
                stretch += 1;
                if !layout.shown.get(stretch - 1).copied().unwrap_or(false) {
                    let end = s.rows.end.min(lines);
                    diff.held.push((line..end, rows.len()));
                    push(
                        &mut rows,
                        &mut heights,
                        Row::Unchanged {
                            stretch: stretch - 1,
                        },
                        row_h,
                    );
                    line = end;
                    continue;
                }
            }
            first[line] = Some(rows.len());
            for row in line_rows(line, widths[line], columns) {
                push(&mut rows, &mut heights, row, row_h);
            }
            line += 1;
        }
        RowMap {
            rows: Rows::Listed {
                rows: rows.into(),
                tops: tops_of(heights.into_iter()),
                first: first.into(),
                diff: Some(Arc::new(diff)),
            },
            columns,
            row_h,
        }
    }

    fn diff_rows(&self) -> Option<&DiffRows> {
        match &self.rows {
            Rows::Listed { diff, .. } => diff.as_deref(),
            Rows::Identity { .. } => None,
        }
    }

    /// The row line `line` is drawn on: its first, or (hidden in a Diff) its fold's: the `⋯`
    /// row of its stretch, the title of its closed card. None: hidden in a file's fold.
    pub fn row_holding(&self, line: usize) -> Option<usize> {
        self.first_row(line).or_else(|| {
            let held = &self.diff_rows()?.held;
            let k = held.partition_point(|(lines, _)| lines.end <= line);
            held.get(k)
                .filter(|(l, _)| l.contains(&line))
                .map(|(_, r)| *r)
        })
    }

    /// Diff: card `card`'s title row.
    pub fn title_row(&self, card: usize) -> Option<usize> {
        self.card_rows(card).map(|r| r.start)
    }

    /// Diff: card `card`'s rows, its title's first.
    pub fn card_rows(&self, card: usize) -> Option<Range<usize>> {
        self.diff_rows()?.cards.get(card).cloned()
    }

    /// Diff: the cards with a row among `rows`.
    pub fn cards_in(&self, rows: Range<usize>) -> Range<usize> {
        let Some(d) = self.diff_rows() else {
            return 0..0;
        };
        let from = d.cards.partition_point(|c| c.end <= rows.start);
        let to = d.cards.partition_point(|c| c.start < rows.end);
        from..to.max(from)
    }

    /// Diff: the card row `r` is in (its title or one of its lines).
    pub fn card_at(&self, r: usize) -> Option<usize> {
        let cards = &self.diff_rows()?.cards;
        let k = cards.partition_point(|c| c.end <= r);
        cards.get(k).filter(|c| c.contains(&r)).map(|_| k)
    }
}

/// M11ah: each run of changed lines' first line (an edge where lines were deleted counts as a
/// change at the line after it, held to the last line; one touching a run joins it), in order.
/// The places ⌥↑ / ⌥↓ stop at.
pub fn change_starts(
    by_line: &HashMap<usize, LineChange>,
    deleted: &[usize],
    lines: usize,
) -> Vec<usize> {
    let Some(last) = lines.checked_sub(1) else {
        return Vec::new();
    };
    let mut starts: Vec<usize> = by_line
        .keys()
        .copied()
        .filter(|&l| l <= last && !(l > 0 && by_line.contains_key(&(l - 1))))
        .collect();
    for &edge in deleted {
        let at = edge.min(last);
        let touches = by_line.contains_key(&at) || (at > 0 && by_line.contains_key(&(at - 1)));
        if !touches {
            starts.push(at);
        }
    }
    starts.sort_unstable();
    starts.dedup();
    starts
}

/// M11ah: the change to jump to: from the current line `from` the first start strictly after
/// (forward) or before it; with no current line, from the view's `top`: forward, the first at
/// or after it; back, the last before it. None at the ends (no wrap-around).
pub fn step_change(
    starts: &[usize],
    from: Option<usize>,
    top: usize,
    forward: bool,
) -> Option<usize> {
    match (forward, from) {
        (true, Some(cur)) => starts.iter().copied().find(|&s| s > cur),
        (true, None) => starts.iter().copied().find(|&s| s >= top),
        (false, from) => starts
            .iter()
            .rev()
            .copied()
            .find(|&s| s < from.unwrap_or(top)),
    }
}

#[cfg(test)]
#[allow(clippy::single_range_in_vec_init)] // `hidden` is a list of ranges: one hidden run
mod tests {
    use super::*;

    fn line(line: usize, cell0: usize) -> Option<Row> {
        Some(Row::Line { line, cell0 })
    }

    /// Each row as (line, its first cell).
    fn listed(map: &RowMap) -> Vec<(usize, usize)> {
        (0..map.len())
            .map(|r| match map.row(r) {
                Some(Row::Line { line, cell0 }) => (line, cell0),
                other => panic!("row {r}: {other:?}"),
            })
            .collect()
    }

    #[test]
    fn with_nothing_hidden_and_no_wrap_it_is_the_identity() {
        let map = RowMap::file(&[3, 10, 0, 5], None, &[], 10.0);
        assert_eq!(map.len(), 4);
        for r in 0..4 {
            assert_eq!(map.row(r), line(r, 0));
            assert_eq!(map.top(r), r as f32 * 10.0);
            assert_eq!(map.first_row(r), Some(r));
        }
        assert_eq!(map.row(4), None);
        assert_eq!(map.top(4), 40.0);
        assert_eq!(map.top(9), 40.0, "past the end: the height");
        assert_eq!(map.height(), 40.0);
        assert_eq!(map.columns(), None);
        // After the last line: the row count (lines deleted at the end).
        assert_eq!(map.first_row(4), Some(4));
        // Empty ranges hide nothing.
        let empty = RowMap::file(&[1, 1], None, &[1..1], 10.0);
        assert_eq!(listed(&empty), [(0, 0), (1, 0)]);
        // A fractional row height: the same tops as the row count times it.
        let odd = RowMap::file(&[1; 50], None, &[], 18.000_002);
        assert_eq!(odd.top(37), 18.000_002 * 37.0);
    }

    /// M11x part 2's `visual_rows` / `first_rows` cases, now the map's.
    #[test]
    fn wrapped_lines_take_their_rows() {
        let map = RowMap::file(&[3, 10, 0], Some(4), &[], 10.0);
        assert_eq!(listed(&map), [(0, 0), (1, 0), (1, 4), (1, 8), (2, 0)]);
        // Each line's first row, and after the last line the row count.
        let first: Vec<_> = (0..=3).map(|i| map.first_row(i)).collect();
        assert_eq!(first, [Some(0), Some(1), Some(4), Some(5)]);
        assert_eq!(map.columns(), Some(4));
        assert_eq!((map.top(2), map.top(4), map.height()), (20.0, 40.0, 50.0));
        // Without Wrap, a row a line.
        let plain = RowMap::file(&[3, 10], None, &[], 10.0);
        assert_eq!(listed(&plain), [(0, 0), (1, 0)]);
        assert_eq!(
            (plain.first_row(0), plain.first_row(1), plain.first_row(2)),
            (Some(0), Some(1), Some(2))
        );
        // Exactly full: no empty row after it.
        assert_eq!(
            listed(&RowMap::file(&[8], Some(4), &[], 10.0)),
            [(0, 0), (0, 4)]
        );
        // No room at all is one column.
        let narrow = RowMap::file(&[2], Some(0), &[], 10.0);
        assert_eq!(listed(&narrow), [(0, 0), (0, 1)]);
        assert_eq!(narrow.columns(), Some(1));
        // Listed tops are the identity's: the row count times a fractional row height.
        let odd = RowMap::file(&[1; 50], Some(4), &[], 18.000_002);
        assert_eq!(odd.top(37), 18.000_002 * 37.0);
        assert_eq!(odd.height(), 18.000_002 * 50.0);
    }

    #[test]
    fn hidden_lines_have_no_row() {
        let map = RowMap::file(&[1; 10], None, &[3..7], 10.0);
        assert_eq!(map.len(), 6);
        assert_eq!(map.first_row(2), Some(2));
        assert_eq!(map.first_row(3), None);
        assert_eq!(map.first_row(5), None);
        assert_eq!(map.first_row(6), None);
        assert_eq!(map.first_row(7), Some(3));
        assert_eq!(map.row(3), line(7, 0));
        assert_eq!(map.first_row(10), Some(6));
        assert_eq!(map.height(), 60.0);
        // Out of order, overlapping and running past the end: each line hidden once.
        #[allow(clippy::reversed_empty_ranges)]
        let map = RowMap::file(&[1; 10], None, &[8..20, 3..5, 4..7, 6..2], 10.0);
        assert_eq!(listed(&map), [(0, 0), (1, 0), (2, 0), (7, 0)]);
        assert_eq!(map.first_row(10), Some(4));
        // With Wrap: a shown line keeps all its rows.
        let wrapped = RowMap::file(&[2, 9, 9, 2], Some(4), &[1..2], 10.0);
        assert_eq!(listed(&wrapped), [(0, 0), (2, 0), (2, 4), (2, 8), (3, 0)]);
        assert_eq!(wrapped.first_row(3), Some(4));
    }

    /// Review Focus 5: a press below hidden lines, with Wrap on, lands on the row drawn there.
    #[test]
    fn row_at_finds_the_row_drawn_at_y_with_hidden_lines_and_titles() {
        // Rows: 0 | 1 1 1 | (2, 3 hidden) 4 | 5 5.
        let map = RowMap::file(&[2, 10, 2, 2, 2, 8], Some(4), &[2..4], 10.0);
        assert_eq!(
            listed(&map),
            [(0, 0), (1, 0), (1, 4), (1, 8), (4, 0), (5, 0), (5, 4)]
        );
        let at = |y: f32| map.row(map.row_at(y));
        assert_eq!(at(45.0), line(4, 0), "below the hidden lines: line 4");
        assert_eq!(at(40.0), line(4, 0), "its top edge");
        assert_eq!(at(39.9), line(1, 8));
        assert_eq!(at(25.0), line(1, 4));
        assert_eq!(at(55.0), line(5, 0));
        // Above the text: the first row; below it: the last.
        assert_eq!(map.row_at(-5.0), 0);
        assert_eq!(map.row_at(0.0), 0);
        assert_eq!(map.row_at(1000.0), 6);
        // The identity agrees.
        let plain = RowMap::file(&[1; 5], None, &[], 10.0);
        assert_eq!(
            [-3.0, 0.0, 9.9, 10.0, 35.0, 49.9, 50.0, 400.0].map(|y| plain.row_at(y)),
            [0, 0, 0, 1, 3, 4, 4, 4]
        );
    }

    #[test]
    fn rows_in_covers_partly_shown_rows() {
        let plain = RowMap::file(&[1; 10], None, &[], 10.0);
        let listed = RowMap::file(&[1; 10], Some(4), &[], 10.0);
        for map in [&plain, &listed] {
            assert_eq!(map.rows_in(15.0..35.0), 1..4);
            assert_eq!(map.rows_in(10.0..30.0), 1..3, "edges: no extra row");
            assert_eq!(map.rows_in(-5.0..5.0), 0..1);
            assert_eq!(map.rows_in(95.0..200.0), 9..10);
            assert_eq!(map.rows_in(100.0..150.0), 10..10, "past the end: none");
        }
        // Hidden lines: the rows shown, not the lines.
        let folded = RowMap::file(&[1; 10], None, &[2..8], 10.0);
        assert_eq!(folded.rows_in(0.0..100.0), 0..4);
    }

    #[test]
    fn a_shown_line_knows_the_lines_hidden_after_it() {
        let map = RowMap::file(&[1; 10], None, &[3..7, 8..10], 10.0);
        assert_eq!(map.hidden_after(2), 3..7);
        assert_eq!(map.hidden_after(7), 8..10, "to the end");
        assert_eq!(map.hidden_after(0), 1..1);
        assert!(map.hidden_after(4).is_empty(), "a hidden line");
        assert!(map.hidden_after(30).is_empty(), "past the end");
        // With Wrap: after all of the line's rows.
        let wrapped = RowMap::file(&[9, 2, 2, 2], Some(4), &[1..3], 10.0);
        assert_eq!(wrapped.hidden_after(0), 1..3);
        assert!(wrapped.hidden_after(3).is_empty());
        // The identity hides nothing.
        let plain = RowMap::file(&[1; 4], None, &[], 10.0);
        assert!(plain.hidden_after(1).is_empty());
    }

    #[test]
    fn change_starts_are_runs() {
        let mut by_line = HashMap::new();
        for l in [3, 4, 5, 9] {
            by_line.insert(l, LineChange::Changed);
        }
        assert_eq!(change_starts(&by_line, &[12], 20), [3, 9, 12]);
        // An edge at the end is held to the last line; one touching a run joins it.
        assert_eq!(change_starts(&by_line, &[20], 20), [3, 9, 19]);
        assert_eq!(change_starts(&by_line, &[6, 9], 20), [3, 9]);
        assert_eq!(change_starts(&by_line, &[3], 20), [3, 9]);
        assert!(change_starts(&by_line, &[], 0).is_empty());
    }

    #[test]
    fn step_change_moves_from_the_current_line() {
        let starts = [3, 9, 12];
        assert_eq!(step_change(&starts, Some(3), 0, true), Some(9));
        assert_eq!(step_change(&starts, Some(5), 0, true), Some(9));
        assert_eq!(step_change(&starts, Some(9), 0, false), Some(3));
        assert_eq!(step_change(&starts, Some(10), 99, false), Some(9));
    }

    #[test]
    fn step_change_from_the_top_without_one() {
        let starts = [3, 9, 12];
        assert_eq!(
            step_change(&starts, None, 9, true),
            Some(9),
            "at or after the top"
        );
        assert_eq!(step_change(&starts, None, 4, true), Some(9));
        assert_eq!(
            step_change(&starts, None, 9, false),
            Some(3),
            "before the top"
        );
        assert_eq!(step_change(&starts, None, 0, true), Some(3));
    }

    #[test]
    fn step_change_stops_at_the_ends() {
        let starts = [3, 9, 12];
        assert_eq!(step_change(&starts, Some(12), 0, true), None);
        assert_eq!(step_change(&starts, Some(3), 0, false), None);
        assert_eq!(step_change(&starts, None, 13, true), None);
        assert_eq!(step_change(&starts, None, 3, false), None);
        assert_eq!(step_change(&[], None, 0, true), None);
    }

    /// The diff of `diff_cards`' tests: rows from letters (`s` unchanged, `a` added).
    fn diff_rows(kinds: &str) -> Vec<crate::diff_view::DiffRow> {
        use crate::diff_view::{DiffRow, RowKind};
        let mut n = 0;
        kinds
            .chars()
            .map(|c| {
                n += 1;
                match c {
                    'a' => DiffRow {
                        kind: RowKind::Added,
                        old: None,
                        new: Some(n),
                    },
                    _ => DiffRow {
                        kind: RowKind::Same,
                        old: Some(n),
                        new: Some(n),
                    },
                }
            })
            .collect()
    }

    /// 11 unchanged, an added row, 14 unchanged, an added row, 11 unchanged: a stretch (0..8),
    /// card 0 (8..15), a stretch (15..23), card 1 (23..30), a stretch (30..38).
    fn two_cards() -> crate::diff_cards::Grouped {
        let rows = diff_rows(&format!(
            "{}a{}a{}",
            "s".repeat(11),
            "s".repeat(14),
            "s".repeat(11)
        ));
        let g = crate::diff_cards::group(&rows);
        assert_eq!(g.cards.len(), 2);
        assert_eq!(g.stretches.len(), 3);
        g
    }

    #[test]
    fn diff_rows_have_titles_and_folded_stretches() {
        use crate::ui_tokens::code::{CARD_GAP, CARD_TITLE};
        let g = two_cards();
        let widths = vec![1; 38];
        let layout = DiffLayout {
            grouped: &g,
            shown: &[false, false, false],
            closed: &[false, false],
            pairs: None,
        };
        let map = RowMap::diff(&widths, None, &layout, 10.0);
        // ⋯ | title 0 | 8..15 | ⋯ | title 1 | 23..30 | ⋯
        let rows: Vec<Row> = (0..map.len()).filter_map(|r| map.row(r)).collect();
        assert_eq!(rows.len(), 1 + 1 + 7 + 1 + 1 + 7 + 1);
        assert_eq!(rows[0], Row::Unchanged { stretch: 0 });
        assert_eq!(rows[1], Row::Title { card: 0 });
        assert_eq!(rows[2], Row::Line { line: 8, cell0: 0 });
        assert_eq!(rows[9], Row::Unchanged { stretch: 1 });
        assert_eq!(rows[10], Row::Title { card: 1 });
        assert_eq!(rows[18], Row::Unchanged { stretch: 2 });
        // A stretch is one row; the first card's title CARD_TITLE tall, right under it; the
        // next card's CARD_GAP further down.
        assert_eq!(map.top(1), 10.0);
        assert_eq!(map.top(2), 10.0 + CARD_TITLE);
        assert_eq!(map.top(10) - map.top(9), 10.0 + CARD_GAP);
        assert_eq!(map.top(11) - map.top(10), CARD_TITLE);
        assert_eq!(map.height(), 17.0 * 10.0 + 2.0 * CARD_TITLE + CARD_GAP);
        assert_eq!(map.first_row(8), Some(2));
        assert_eq!(map.first_row(3), None, "folded");
        assert_eq!(map.row_holding(3), Some(0), "its stretch's row");
        assert_eq!(map.row_holding(8), Some(2));
        assert_eq!(
            (map.title_row(1), map.card_rows(1)),
            (Some(10), Some(10..18))
        );
        assert_eq!(map.card_at(12), Some(1));
        assert_eq!(map.card_at(9), None);
        assert_eq!(map.cards_in(0..1), 0..0);
        assert_eq!(map.cards_in(0..11), 0..2);
        assert_eq!(map.cards_in(9..10), 1..1, "between them: none");
        assert_eq!(map.cards_in(5..12), 0..2);
        assert_eq!(RowMap::file(&[1], None, &[], 10.0).cards_in(0..1), 0..0);
        assert!(map.hidden_after(14).is_empty(), "no fold pill in a diff");
        // A shown stretch is its lines; a closed card its title only.
        let layout = DiffLayout {
            grouped: &g,
            shown: &[false, true, false],
            closed: &[true, false],
            pairs: None,
        };
        let map = RowMap::diff(&widths, None, &layout, 10.0);
        let rows: Vec<Row> = (0..map.len()).filter_map(|r| map.row(r)).collect();
        assert_eq!(
            &rows[..4],
            [
                Row::Unchanged { stretch: 0 },
                Row::Title { card: 0 },
                Row::Line { line: 15, cell0: 0 },
                Row::Line { line: 16, cell0: 0 },
            ]
        );
        assert_eq!(rows.len(), 1 + 1 + 8 + 1 + 7 + 1);
        assert_eq!(map.first_row(9), None, "inside the closed card");
        assert_eq!(map.row_holding(9), Some(1), "its title's row");
        assert_eq!(map.card_rows(0), Some(1..2));
        // With Wrap, a line takes its rows inside a card.
        let mut wide = widths.clone();
        wide[8] = 9;
        let map = RowMap::diff(&wide, Some(4), &layout_all(&g), 10.0);
        assert_eq!(map.row(2), Some(Row::Line { line: 8, cell0: 0 }));
        assert_eq!(map.row(4), Some(Row::Line { line: 8, cell0: 8 }));
        assert_eq!(map.columns(), Some(4));
    }

    fn layout_all(g: &crate::diff_cards::Grouped) -> DiffLayout<'_> {
        DiffLayout {
            grouped: g,
            shown: &[false, false, false],
            closed: &[false, false],
            pairs: None,
        }
    }

    /// Review Focus 5 in Diff mode: a press below titles and a folded stretch, Wrap on, lands
    /// on the row drawn there.
    #[test]
    fn row_at_finds_rows_below_titles_and_stretches() {
        use crate::ui_tokens::code::{CARD_GAP, CARD_TITLE};
        let g = two_cards();
        let mut widths = vec![1; 38];
        widths[24] = 9; // three rows at 4 columns
        let map = RowMap::diff(&widths, Some(4), &layout_all(&g), 10.0);
        let at = |y: f32| map.row(map.row_at(y));
        assert_eq!(at(5.0), Some(Row::Unchanged { stretch: 0 }));
        assert_eq!(at(10.0 + CARD_TITLE - 0.1), Some(Row::Title { card: 0 }));
        assert_eq!(at(10.0 + CARD_TITLE), Some(Row::Line { line: 8, cell0: 0 }));
        // Card 1: its title after the ⋯ row and the gap, then line 23, then 24's three rows.
        let title1 = 10.0 + CARD_TITLE + 70.0 + 10.0 + CARD_GAP;
        assert_eq!(map.top(map.title_row(1).unwrap()), title1);
        assert_eq!(
            at(title1 - 1.0),
            Some(Row::Unchanged { stretch: 1 }),
            "the gap"
        );
        assert_eq!(
            at(title1 + CARD_TITLE + 5.0),
            Some(Row::Line { line: 23, cell0: 0 })
        );
        assert_eq!(
            at(title1 + CARD_TITLE + 25.0),
            Some(Row::Line { line: 24, cell0: 4 })
        );
        assert_eq!(
            at(title1 + CARD_TITLE + 45.0),
            Some(Row::Line { line: 25, cell0: 0 })
        );
        assert_eq!(map.rows_in(0.0..10.0 + CARD_TITLE), 0..2);
    }
}
