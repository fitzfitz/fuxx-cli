//! The text body's screen rows (M11ah): what each row shows and where it sits, for File and Diff
//! mode. Painting, clicks, selection, scrolling, the scrollbar, Find's reveal, change bars and
//! Follow all go through it; the text, line numbers, Find and Copy stay on the real lines. With
//! nothing hidden, no Wrap and no cards it is the identity (row r is line r), kept without a
//! list. A layout cache: the file tab builds it in its canvas's prepaint only when its `MapKey`
//! changes (as `Wrapped` was). Pure: no GPUI.

use std::ops::Range;
use std::sync::Arc;

use crate::layout_model::FileMode;

/// One screen row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Row {
    /// A part of a doc line, from cell `cell0` (0: the line's first row).
    Line { line: usize, cell0: usize },
    /// Diff: a card's title row.
    #[allow(dead_code)] // built by `RowMap::diff` (Task 9)
    Title { card: usize },
    /// Diff: a folded unchanged stretch.
    #[allow(dead_code)] // built by `RowMap::diff` (Task 9)
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
    },
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
            let (count, step) = match columns {
                Some(c) => (width.div_ceil(c).max(1), c),
                None => (1, 0),
            };
            rows.extend((0..count).map(|k| Row::Line {
                line,
                cell0: k * step,
            }));
        }
        // Summed in f64 (exact for any real row count): each top is the f32 nearest it, as
        // the identity's `row_h × r` is.
        let mut tops = Vec::with_capacity(rows.len() + 1);
        let mut y = 0.0_f64;
        for _ in 0..=rows.len() {
            tops.push(y as f32);
            y += f64::from(row_h);
        }
        RowMap {
            rows: Rows::Listed {
                rows: rows.into(),
                tops: tops.into(),
                first: first.into(),
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
}
