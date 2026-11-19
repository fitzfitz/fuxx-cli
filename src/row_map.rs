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
    /// Diff side by side: an old (line, cell0) on the left and a new one on the right (none:
    /// that half is empty, facing a longer line or a line only the other side has).
    Pair {
        old: Option<(usize, usize)>,
        new: Option<(usize, usize)>,
    },
    /// M11ag: row `k` of note `note`, under its line's last row (a pair's).
    Note { note: usize, k: usize },
}

/// M11ag: a note's place in the rows: under doc line `under` (a file line; in Diff, a diff
/// row), `rows` rows tall. A list of them is sorted by `under`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NoteSpan {
    pub under: usize,
    pub rows: usize,
    pub note: usize,
}

/// M11ah: a side-by-side half: the old file on the left, the new one on the right.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Half {
    Old,
    New,
}

impl Row {
    /// The doc line a row shows (a pair's first: the old side's when both have one); none: a
    /// title's, a folded stretch's or (M11ag) a note's row (`RowMap::line_of` gives its line).
    pub fn line(self) -> Option<usize> {
        match self {
            Row::Line { line, .. } => Some(line),
            Row::Pair { old, new } => match (old, new) {
                (Some((o, _)), Some((n, _))) => Some(o.min(n)),
                (Some((l, _)), None) | (None, Some((l, _))) => Some(l),
                (None, None) => None,
            },
            Row::Title { .. } | Row::Unchanged { .. } | Row::Note { .. } => None,
        }
    }

    /// The (line, first cell) a row shows on `half` (a line's row: on both).
    pub fn half(self, half: Half) -> Option<(usize, usize)> {
        match (self, half) {
            (Row::Line { line, cell0 }, _) => Some((line, cell0)),
            (Row::Pair { old, .. }, Half::Old) => old,
            (Row::Pair { new, .. }, Half::New) => new,
            _ => None,
        }
    }
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
        /// Each run of hidden lines and the row drawn for it, in order: a file's fold (its
        /// folded line's first row), a Diff's folded stretch or closed card (its `⋯` row, its
        /// title).
        held: Arc<[(Range<usize>, usize)]>,
        /// Diff mode's cards (none: a file's).
        diff: Option<Arc<DiffRows>>,
        /// M11ag: each note with rows, in row order.
        notes: Arc<[NoteRows]>,
    },
}

/// M11ag: where a note's rows sit, and the line they are under.
#[derive(Clone, Debug)]
struct NoteRows {
    note: usize,
    under: usize,
    rows: Range<usize>,
}

/// M11ag: the notes under line `line` (`notes` sorted by `under`).
fn notes_under(notes: &[NoteSpan], line: usize) -> &[NoteSpan] {
    let from = notes.partition_point(|n| n.under < line);
    let to = notes.partition_point(|n| n.under <= line);
    &notes[from..to.max(from)]
}

/// M11ag: the rows of the notes under line `line`, after `rows` (each one with rows
/// `placed`); how many were added.
fn add_notes(
    rows: &mut Vec<Row>,
    placed: &mut Vec<NoteRows>,
    notes: &[NoteSpan],
    line: usize,
) -> usize {
    let start = rows.len();
    for n in notes_under(notes, line).iter().filter(|n| n.rows > 0) {
        placed.push(NoteRows {
            note: n.note,
            under: n.under,
            rows: rows.len()..rows.len() + n.rows,
        });
        rows.extend((0..n.rows).map(|k| Row::Note { note: n.note, k }));
    }
    rows.len() - start
}

/// Where a Diff map's cards sit.
#[derive(Clone, Debug, Default)]
struct DiffRows {
    /// Each card's rows: its title's, to one past its last.
    cards: Vec<Range<usize>>,
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

/// M11ah: a side-by-side pair's rows: each half's line cut at `columns` (none: one row), the
/// pair as tall as its taller half, the shorter half padded with none.
fn pair_rows(
    (old, new): (Option<usize>, Option<usize>),
    widths: &[usize],
    columns: Option<usize>,
) -> impl Iterator<Item = Row> {
    let count = |l: Option<usize>| match (l, columns) {
        (None, _) => 0,
        (Some(_), None) => 1,
        (Some(l), Some(c)) => widths[l].div_ceil(c).max(1),
    };
    let (o, n) = (count(old), count(new));
    let step = columns.unwrap_or(0);
    let at = move |l: Option<usize>, k: usize, upto: usize| {
        l.filter(|_| k < upto).map(|l| (l, k * step))
    };
    (0..o.max(n).max(1)).map(move |k| Row::Pair {
        old: at(old, k, o),
        new: at(new, k, n),
    })
}

impl RowMap {
    /// A file's rows: each shown line (`hidden` ranges left out) cut at `columns` cells when
    /// wrapping (a line takes at least one row, an empty one too; no room at all is one column),
    /// then (M11ag) the rows of the `notes` under it.
    pub fn file(
        widths: &[usize],
        columns: Option<usize>,
        hidden: &[Range<usize>],
        notes: &[NoteSpan],
        row_h: f32,
    ) -> RowMap {
        let lines = widths.len();
        let columns = columns.map(|c| c.max(1));
        if columns.is_none()
            && notes.is_empty()
            && hidden.iter().all(|h| h.start >= h.end.min(lines))
        {
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
        // Each run of hidden lines, held by the shown line before it (its fold's line).
        let mut held: Vec<(Range<usize>, usize)> = Vec::new();
        let mut placed = Vec::new();
        let mut last_shown = None;
        for (line, &width) in widths.iter().enumerate() {
            if !shown[line] {
                match held.last_mut() {
                    Some((run, _)) if run.end == line => run.end = line + 1,
                    _ => held.extend(last_shown.map(|r| (line..line + 1, r))),
                }
                continue;
            }
            first[line] = Some(rows.len());
            last_shown = Some(rows.len());
            rows.extend(line_rows(line, width, columns));
            add_notes(&mut rows, &mut placed, notes, line);
        }
        let tops = tops_of(std::iter::repeat_n(row_h, rows.len()));
        RowMap {
            rows: Rows::Listed {
                rows: rows.into(),
                tops,
                first: first.into(),
                held: held.into(),
                diff: None,
                notes: placed.into(),
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
        // Past the line's own rows (Wrap: a few) and its notes' (M11ag), to the next shown line.
        while matches!(rows.get(r), Some(Row::Line { line: l, .. }) if *l == line) {
            r += 1;
        }
        while matches!(rows.get(r), Some(Row::Note { .. })) {
            r += 1;
        }
        match rows.get(r) {
            Some(Row::Line { line: next, .. }) => line + 1..*next,
            Some(_) => none,
            None => line + 1..first.len(),
        }
    }

    /// M11ag: note `note`'s rows (none: it has none, under a hidden line or none shown).
    pub fn note_rows(&self, note: usize) -> Option<Range<usize>> {
        (self.notes().iter())
            .find(|n| n.note == note)
            .map(|n| n.rows.clone())
    }

    /// M11ag: the doc line row `r` belongs to: a note row's, the line it is under; else the
    /// line it shows (`Row::line`).
    pub fn line_of(&self, r: usize) -> Option<usize> {
        match self.row(r)? {
            Row::Note { .. } => {
                let notes = self.notes();
                let k = notes.partition_point(|n| n.rows.end <= r);
                notes
                    .get(k)
                    .filter(|n| n.rows.contains(&r))
                    .map(|n| n.under)
            }
            row => row.line(),
        }
    }

    fn notes(&self) -> &[NoteRows] {
        match &self.rows {
            Rows::Listed { notes, .. } => notes,
            Rows::Identity { .. } => &[],
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
    pub pairs: Option<&'a [CardPairs]>,
}

impl RowMap {
    /// Diff mode's rows (M11ah): the rows outside the cards as lines (side by side, `pairs` per
    /// card given: every row a `Row::Pair`, a card's rows paired as `pairs` says), a folded stretch one `⋯`
    /// row (unless `shown`), each card a title row `code::CARD_TITLE` tall (`code::CARD_GAP`
    /// further down for every card but the first) and its lines (none when `closed`); lines cut
    /// at `columns` cells when wrapping; (M11ag) the rows of the `notes` under a shown line
    /// after its rows (side by side: after the pair's).
    pub fn diff(
        widths: &[usize],
        columns: Option<usize>,
        layout: &DiffLayout,
        notes: &[NoteSpan],
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
        let mut held: Vec<(Range<usize>, usize)> = Vec::new();
        let mut placed = Vec::new();
        let (mut card, mut stretch) = (0, 0);
        let mut line = 0;
        let push = |rows: &mut Vec<Row>, heights: &mut Vec<f32>, row: Row, h: f32| {
            rows.push(row);
            heights.push(h);
        };
        // M11ag: the notes under each of a row's lines (a pair's: both, once each), row_h tall.
        let mut notes_after =
            |rows: &mut Vec<Row>,
             heights: &mut Vec<f32>,
             (o, n): (Option<usize>, Option<usize>)| {
                for l in [o, n.filter(|&n| Some(n) != o)].into_iter().flatten() {
                    let added = add_notes(rows, &mut placed, notes, l);
                    heights.extend(std::iter::repeat_n(row_h, added));
                }
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
                    held.push((line..end, title));
                } else if let Some(pairs) = layout.pairs.and_then(|p| p.get(card)) {
                    // Side by side: the card's rows paired.
                    for &(o, n) in pairs {
                        for l in [o, n].into_iter().flatten() {
                            first[l] = Some(rows.len());
                        }
                        for row in pair_rows((o, n), widths, columns) {
                            push(&mut rows, &mut heights, row, row_h);
                        }
                        notes_after(&mut rows, &mut heights, (o, n));
                    }
                } else {
                    for l in line..end {
                        first[l] = Some(rows.len());
                        for row in line_rows(l, widths[l], columns) {
                            push(&mut rows, &mut heights, row, row_h);
                        }
                        notes_after(&mut rows, &mut heights, (Some(l), None));
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
                    held.push((line..end, rows.len()));
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
            if layout.pairs.is_some() {
                // Side by side: an unchanged line on both halves.
                for row in pair_rows((Some(line), Some(line)), widths, columns) {
                    push(&mut rows, &mut heights, row, row_h);
                }
            } else {
                for row in line_rows(line, widths[line], columns) {
                    push(&mut rows, &mut heights, row, row_h);
                }
            }
            notes_after(&mut rows, &mut heights, (Some(line), None));
            line += 1;
        }
        RowMap {
            rows: Rows::Listed {
                rows: rows.into(),
                tops: tops_of(heights.into_iter()),
                first: first.into(),
                held: held.into(),
                diff: Some(Arc::new(diff)),
                notes: placed.into(),
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

    /// Each run of hidden lines and the row drawn for it, in order (none: nothing hidden).
    pub fn held(&self) -> &[(Range<usize>, usize)] {
        match &self.rows {
            Rows::Listed { held, .. } => held,
            Rows::Identity { .. } => &[],
        }
    }

    /// The row line `line` is drawn on: its first, or (hidden) its fold's: a file's folded line,
    /// a Diff's `⋯` row of its stretch or the title of its closed card.
    pub fn row_holding(&self, line: usize) -> Option<usize> {
        self.first_row(line).or_else(|| {
            let held = self.held();
            let k = held.partition_point(|(lines, _)| lines.end <= line);
            held.get(k)
                .filter(|(l, _)| l.contains(&line))
                .map(|(_, r)| *r)
        })
    }

    /// The lines a folded row `r` holds (a `⋯` row's stretch, a closed card's title's; a file's
    /// folded line's hidden lines).
    pub fn lines_held_at(&self, r: usize) -> Option<Range<usize>> {
        let held = self.held();
        let k = held.partition_point(|(_, row)| *row < r);
        held.get(k)
            .filter(|(_, row)| *row == r)
            .map(|(l, _)| l.clone())
    }

    /// Side by side: the line of `half` nearest row `r` (a press on an empty half takes it):
    /// the nearest above (true: at its end), else the nearest below (false: at its start),
    /// looking only through the paired rows around `r` (M11ag: past note rows; never past a
    /// title or a `⋯` row).
    pub fn nearest_on(&self, r: usize, half: Half) -> Option<(usize, bool)> {
        let on = |k: usize| match self.row(k)? {
            row @ Row::Pair { .. } => Some(row.half(half)),
            // M11ag: a note's rows are looked past.
            Row::Note { .. } => Some(None),
            _ => None,
        };
        on(r)?;
        let above = (0..=r)
            .rev()
            .map_while(on)
            .find_map(|h| h.map(|(line, _)| (line, true)));
        above.or_else(|| {
            (r + 1..self.len())
                .map_while(on)
                .find_map(|h| h.map(|(line, _)| (line, false)))
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
        let map = RowMap::file(&[3, 10, 0, 5], None, &[], &[], 10.0);
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
        let empty = RowMap::file(&[1, 1], None, &[1..1], &[], 10.0);
        assert_eq!(listed(&empty), [(0, 0), (1, 0)]);
        // A fractional row height: the same tops as the row count times it.
        let odd = RowMap::file(&[1; 50], None, &[], &[], 18.000_002);
        assert_eq!(odd.top(37), 18.000_002 * 37.0);
    }

    /// M11x part 2's `visual_rows` / `first_rows` cases, now the map's.
    #[test]
    fn wrapped_lines_take_their_rows() {
        let map = RowMap::file(&[3, 10, 0], Some(4), &[], &[], 10.0);
        assert_eq!(listed(&map), [(0, 0), (1, 0), (1, 4), (1, 8), (2, 0)]);
        // Each line's first row, and after the last line the row count.
        let first: Vec<_> = (0..=3).map(|i| map.first_row(i)).collect();
        assert_eq!(first, [Some(0), Some(1), Some(4), Some(5)]);
        assert_eq!(map.columns(), Some(4));
        assert_eq!((map.top(2), map.top(4), map.height()), (20.0, 40.0, 50.0));
        // Without Wrap, a row a line.
        let plain = RowMap::file(&[3, 10], None, &[], &[], 10.0);
        assert_eq!(listed(&plain), [(0, 0), (1, 0)]);
        assert_eq!(
            (plain.first_row(0), plain.first_row(1), plain.first_row(2)),
            (Some(0), Some(1), Some(2))
        );
        // Exactly full: no empty row after it.
        assert_eq!(
            listed(&RowMap::file(&[8], Some(4), &[], &[], 10.0)),
            [(0, 0), (0, 4)]
        );
        // No room at all is one column.
        let narrow = RowMap::file(&[2], Some(0), &[], &[], 10.0);
        assert_eq!(listed(&narrow), [(0, 0), (0, 1)]);
        assert_eq!(narrow.columns(), Some(1));
        // Listed tops are the identity's: the row count times a fractional row height.
        let odd = RowMap::file(&[1; 50], Some(4), &[], &[], 18.000_002);
        assert_eq!(odd.top(37), 18.000_002 * 37.0);
        assert_eq!(odd.height(), 18.000_002 * 50.0);
    }

    /// Final review: a file's hidden runs are held by the folded line before them (its first
    /// row with Wrap), touching runs as one; a hidden line's row is its fold's.
    #[test]
    fn a_files_hidden_lines_are_held_by_their_fold() {
        let map = RowMap::file(
            &[1, 9, 1, 1, 1, 1, 1, 1],
            Some(4),
            &[2..4, 4..5, 6..8],
            &[],
            10.0,
        );
        // Rows: line 0, line 1 (3 rows: 1..4), line 5 (row 4).
        assert_eq!(map.held(), [(2..5, 1), (6..8, 4)]);
        assert_eq!(map.row_holding(3), Some(1), "hidden: its fold's first row");
        assert_eq!(map.row_holding(7), Some(4));
        assert_eq!(map.row_holding(5), Some(4), "shown: its own");
        assert_eq!(map.lines_held_at(1), Some(2..5));
        assert_eq!(map.lines_held_at(2), None);
        assert!(
            RowMap::file(&[1; 4], None, &[], &[], 10.0)
                .held()
                .is_empty()
        );
        assert!(
            RowMap::file(&[1; 4], Some(4), &[], &[], 10.0)
                .held()
                .is_empty()
        );
    }

    #[test]
    fn hidden_lines_have_no_row() {
        let map = RowMap::file(&[1; 10], None, &[3..7], &[], 10.0);
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
        let map = RowMap::file(&[1; 10], None, &[8..20, 3..5, 4..7, 6..2], &[], 10.0);
        assert_eq!(listed(&map), [(0, 0), (1, 0), (2, 0), (7, 0)]);
        assert_eq!(map.first_row(10), Some(4));
        // With Wrap: a shown line keeps all its rows.
        let wrapped = RowMap::file(&[2, 9, 9, 2], Some(4), &[1..2], &[], 10.0);
        assert_eq!(listed(&wrapped), [(0, 0), (2, 0), (2, 4), (2, 8), (3, 0)]);
        assert_eq!(wrapped.first_row(3), Some(4));
    }

    /// Review Focus 5: a press below hidden lines, with Wrap on, lands on the row drawn there.
    #[test]
    fn row_at_finds_the_row_drawn_at_y_with_hidden_lines_and_titles() {
        // Rows: 0 | 1 1 1 | (2, 3 hidden) 4 | 5 5.
        let map = RowMap::file(&[2, 10, 2, 2, 2, 8], Some(4), &[2..4], &[], 10.0);
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
        let plain = RowMap::file(&[1; 5], None, &[], &[], 10.0);
        assert_eq!(
            [-3.0, 0.0, 9.9, 10.0, 35.0, 49.9, 50.0, 400.0].map(|y| plain.row_at(y)),
            [0, 0, 0, 1, 3, 4, 4, 4]
        );
    }

    #[test]
    fn rows_in_covers_partly_shown_rows() {
        let plain = RowMap::file(&[1; 10], None, &[], &[], 10.0);
        let listed = RowMap::file(&[1; 10], Some(4), &[], &[], 10.0);
        for map in [&plain, &listed] {
            assert_eq!(map.rows_in(15.0..35.0), 1..4);
            assert_eq!(map.rows_in(10.0..30.0), 1..3, "edges: no extra row");
            assert_eq!(map.rows_in(-5.0..5.0), 0..1);
            assert_eq!(map.rows_in(95.0..200.0), 9..10);
            assert_eq!(map.rows_in(100.0..150.0), 10..10, "past the end: none");
        }
        // Hidden lines: the rows shown, not the lines.
        let folded = RowMap::file(&[1; 10], None, &[2..8], &[], 10.0);
        assert_eq!(folded.rows_in(0.0..100.0), 0..4);
    }

    #[test]
    fn a_shown_line_knows_the_lines_hidden_after_it() {
        let map = RowMap::file(&[1; 10], None, &[3..7, 8..10], &[], 10.0);
        assert_eq!(map.hidden_after(2), 3..7);
        assert_eq!(map.hidden_after(7), 8..10, "to the end");
        assert_eq!(map.hidden_after(0), 1..1);
        assert!(map.hidden_after(4).is_empty(), "a hidden line");
        assert!(map.hidden_after(30).is_empty(), "past the end");
        // With Wrap: after all of the line's rows.
        let wrapped = RowMap::file(&[9, 2, 2, 2], Some(4), &[1..3], &[], 10.0);
        assert_eq!(wrapped.hidden_after(0), 1..3);
        assert!(wrapped.hidden_after(3).is_empty());
        // The identity hides nothing.
        let plain = RowMap::file(&[1; 4], None, &[], &[], 10.0);
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
        let map = RowMap::diff(&widths, None, &layout, &[], 10.0);
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
        assert_eq!(
            RowMap::file(&[1], None, &[], &[], 10.0).cards_in(0..1),
            0..0
        );
        assert!(map.hidden_after(14).is_empty(), "no fold pill in a diff");
        // A shown stretch is its lines; a closed card its title only.
        let layout = DiffLayout {
            grouped: &g,
            shown: &[false, true, false],
            closed: &[true, false],
            pairs: None,
        };
        let map = RowMap::diff(&widths, None, &layout, &[], 10.0);
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
        assert_eq!(map.lines_held_at(1), Some(8..15), "the closed card's lines");
        assert_eq!(map.lines_held_at(0), Some(0..8), "the stretch's lines");
        assert_eq!(map.lines_held_at(2), None, "a line's row");
        assert_eq!(map.card_rows(0), Some(1..2));
        // With Wrap, a line takes its rows inside a card.
        let mut wide = widths.clone();
        wide[8] = 9;
        let map = RowMap::diff(&wide, Some(4), &layout_all(&g), &[], 10.0);
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
        let map = RowMap::diff(&widths, Some(4), &layout_all(&g), &[], 10.0);
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

    /// M11ah Task 11: side by side, each card's rows paired (`diff_cards::pair`): a removed and
    /// an added line level, each half cut at the half's columns, the pair as tall as its
    /// taller half (the shorter padded with none); rows outside the cards are a line on both
    /// halves; both lines of a pair start on its first row.
    #[test]
    fn pairs_take_the_taller_half_when_wrapped() {
        use crate::diff_view::{DiffRow, RowKind};
        let row = |kind, old, new| DiffRow { kind, old, new };
        // s s - + + s : a card over all six (too few to fold anything).
        let rows = vec![
            row(RowKind::Same, Some(1), Some(1)),
            row(RowKind::Same, Some(2), Some(2)),
            row(RowKind::Removed, Some(3), None),
            row(RowKind::Added, None, Some(3)),
            row(RowKind::Added, None, Some(4)),
            row(RowKind::Same, Some(4), Some(5)),
        ];
        let g = crate::diff_cards::group(&rows);
        assert_eq!(g.cards.len(), 1);
        let pairs: Vec<CardPairs> = g
            .cards
            .iter()
            .map(|c| crate::diff_cards::pair(&rows, c.rows.clone()))
            .collect();
        let layout = DiffLayout {
            grouped: &g,
            shown: &[],
            closed: &[false],
            pairs: Some(&pairs),
        };
        // The removed line 9 cells (3 rows at 4 columns), the first added 2 (1 row).
        let widths = [1, 1, 9, 2, 5, 1];
        let map = RowMap::diff(&widths, Some(4), &layout, &[], 10.0);
        let got: Vec<Row> = (0..map.len()).filter_map(|r| map.row(r)).collect();
        let both = |l: usize| Row::Pair {
            old: Some((l, 0)),
            new: Some((l, 0)),
        };
        assert_eq!(
            got,
            [
                Row::Title { card: 0 },
                both(0),
                both(1),
                Row::Pair {
                    old: Some((2, 0)),
                    new: Some((3, 0))
                },
                Row::Pair {
                    old: Some((2, 4)),
                    new: None
                },
                Row::Pair {
                    old: Some((2, 8)),
                    new: None
                },
                // The second added line (5 cells: 2 rows) faces an empty half.
                Row::Pair {
                    old: None,
                    new: Some((4, 0))
                },
                Row::Pair {
                    old: None,
                    new: Some((4, 4))
                },
                both(5),
            ]
        );
        assert_eq!(map.first_row(2), Some(3), "the removed line's first row");
        assert_eq!(map.first_row(3), Some(3), "level with it, the added one");
        assert_eq!(map.first_row(4), Some(6));
        assert_eq!(map.row_holding(5), Some(8));
        assert_eq!(map.card_rows(0), Some(0..9));
        assert_eq!(got[4].line(), Some(2));
        assert_eq!(got[6].line(), Some(4));
        assert_eq!(got[3].half(Half::New), Some((3, 0)));
        assert_eq!(got[4].half(Half::New), None);
        assert_eq!(Row::Title { card: 0 }.line(), None);
        // Without Wrap a pair is one row; a stretch outside the cards is a line on both halves.
        let map = RowMap::diff(&widths, None, &layout, &[], 10.0);
        assert_eq!(map.len(), 1 + 6 - 1);
        assert_eq!(
            map.row(3),
            Some(Row::Pair {
                old: Some((2, 0)),
                new: Some((3, 0))
            })
        );
        let g2 = two_cards();
        let pairs2: Vec<CardPairs> = g2
            .cards
            .iter()
            .map(|c| c.rows.clone().map(|l| (Some(l), Some(l))).collect())
            .collect();
        let layout = DiffLayout {
            grouped: &g2,
            shown: &[true, false, false],
            closed: &[false, false],
            pairs: Some(&pairs2),
        };
        let map = RowMap::diff(&[1; 38], None, &layout, &[], 10.0);
        assert_eq!(
            map.row(0),
            Some(both(0)),
            "a shown stretch's line, both halves"
        );
    }

    /// M11ah Task 11: a press on an empty half takes that side's nearest line: above (at its
    /// end), else below (at its start), never past a title.
    #[test]
    fn an_empty_half_takes_the_nearest_line_of_its_side() {
        use crate::diff_view::{DiffRow, RowKind};
        let row = |kind, old, new| DiffRow { kind, old, new };
        let rows = vec![
            row(RowKind::Same, Some(1), Some(1)),
            row(RowKind::Same, Some(2), Some(2)),
            row(RowKind::Removed, Some(3), None),
            row(RowKind::Added, None, Some(3)),
            row(RowKind::Added, None, Some(4)),
            row(RowKind::Same, Some(4), Some(5)),
        ];
        let g = crate::diff_cards::group(&rows);
        let pairs: Vec<CardPairs> = (g.cards.iter())
            .map(|c| crate::diff_cards::pair(&rows, c.rows.clone()))
            .collect();
        let layout = DiffLayout {
            grouped: &g,
            shown: &[],
            closed: &[false],
            pairs: Some(&pairs),
        };
        let widths = [1, 1, 9, 2, 5, 1];
        // Title | s0 | s1 | 2·3 | 2·_ | 2·_ | _·4 | _·4 | s5
        let map = RowMap::diff(&widths, Some(4), &layout, &[], 10.0);
        assert_eq!(map.nearest_on(4, Half::New), Some((3, true)));
        assert_eq!(map.nearest_on(6, Half::Old), Some((2, true)));
        assert_eq!(map.nearest_on(3, Half::Old), Some((2, true)), "its own");
        // A card starting with an added line: no old line above it (the title), the one below.
        let rows = vec![
            row(RowKind::Added, None, Some(1)),
            row(RowKind::Same, Some(1), Some(2)),
        ];
        let g = crate::diff_cards::group(&rows);
        let pairs: Vec<CardPairs> = (g.cards.iter())
            .map(|c| crate::diff_cards::pair(&rows, c.rows.clone()))
            .collect();
        let layout = DiffLayout {
            grouped: &g,
            shown: &[],
            closed: &[false],
            pairs: Some(&pairs),
        };
        let map = RowMap::diff(&[1, 1], None, &layout, &[], 10.0);
        assert_eq!(map.row(0), Some(Row::Title { card: 0 }));
        assert_eq!(map.nearest_on(1, Half::Old), Some((1, false)));
        assert_eq!(map.nearest_on(0, Half::Old), None, "a title has no half");
    }

    fn note(under: usize, rows: usize, note: usize) -> NoteSpan {
        NoteSpan { under, rows, note }
    }

    /// M11ag: a note's rows follow its line's, each a row tall; the lines below move down.
    #[test]
    fn note_rows_follow_their_line() {
        let map = RowMap::file(&[1; 10], None, &[], &[note(4, 2, 0)], 10.0);
        assert_eq!(map.len(), 12);
        assert_eq!(map.row(4), line(4, 0));
        assert_eq!(map.row(5), Some(Row::Note { note: 0, k: 0 }));
        assert_eq!(map.row(6), Some(Row::Note { note: 0, k: 1 }));
        assert_eq!(map.row(7), line(5, 0));
        assert_eq!(map.first_row(5), Some(7));
        assert_eq!(map.first_row(10), Some(12), "past the end: the row count");
        assert_eq!(map.top(7), 7.0 * 10.0);
        assert_eq!(map.height(), 120.0);
        assert_eq!(map.note_rows(0), Some(5..7));
        assert_eq!(map.note_rows(1), None, "no such note");
        // A note row shows no line and no half; its line is the one it is under.
        let row = map.row(5).unwrap();
        assert_eq!((row.line(), row.half(Half::Old)), (None, None));
        assert_eq!(
            [4, 5, 6, 7].map(|r| map.line_of(r)),
            [Some(4), Some(4), Some(4), Some(5)]
        );
        assert_eq!(map.line_of(12), None, "past the last row");
        assert!(map.hidden_after(4).is_empty(), "not folded");
        // Two notes, in order, under two lines.
        let map = RowMap::file(&[1; 4], None, &[], &[note(0, 1, 3), note(2, 1, 7)], 10.0);
        let rows: Vec<_> = (0..map.len()).filter_map(|r| map.row(r)).collect();
        assert_eq!(
            rows,
            [
                Row::Line { line: 0, cell0: 0 },
                Row::Note { note: 3, k: 0 },
                Row::Line { line: 1, cell0: 0 },
                Row::Line { line: 2, cell0: 0 },
                Row::Note { note: 7, k: 0 },
                Row::Line { line: 3, cell0: 0 },
            ]
        );
        assert_eq!(
            (map.note_rows(3), map.note_rows(7)),
            (Some(1..2), Some(4..5))
        );
        assert_eq!(map.line_of(4), Some(2));
    }

    /// M11ag: a note under a hidden line (a fold's) or past the last line has no rows.
    #[test]
    fn a_note_under_a_hidden_line_has_no_rows() {
        let map = RowMap::file(
            &[1; 10],
            None,
            &[3..7],
            &[note(5, 2, 0), note(10, 1, 1)],
            10.0,
        );
        assert_eq!(map.len(), 6);
        assert_eq!(map.note_rows(0), None);
        assert_eq!(map.note_rows(1), None);
        assert!((0..map.len()).all(|r| !matches!(map.row(r), Some(Row::Note { .. }))));
        assert_eq!(map.held(), [(3..7, 2)]);
        // Wrap off and nothing hidden, but a note: every row listed all the same.
        let map = RowMap::file(&[1; 3], None, &[], &[note(1, 1, 0)], 10.0);
        assert_eq!(map.len(), 4);
        assert_eq!(map.row(2), Some(Row::Note { note: 0, k: 0 }));
    }

    /// M11ag: a note under a folded line follows its row; the fold still knows its hidden
    /// lines (the pill), stepping over the note's rows.
    #[test]
    fn a_note_under_a_fold_line_follows_it_and_hidden_after_skips_it() {
        let map = RowMap::file(&[1; 10], None, &[3..7], &[note(2, 2, 0)], 10.0);
        // 0 | 1 | 2 | note | note | 7 | 8 | 9
        assert_eq!(map.len(), 8);
        assert_eq!(map.note_rows(0), Some(3..5));
        assert_eq!(map.hidden_after(2), 3..7);
        assert_eq!(map.held(), [(3..7, 2)]);
        assert_eq!(map.row_holding(4), Some(2), "a hidden line: its fold's row");
        assert_eq!(map.lines_held_at(2), Some(3..7));
        assert_eq!(map.lines_held_at(3), None, "a note row holds nothing");
        assert_eq!(map.first_row(7), Some(5));
        // A fold to the end with a note under it.
        let map = RowMap::file(&[1; 10], None, &[8..10], &[note(7, 1, 0)], 10.0);
        assert_eq!(map.hidden_after(7), 8..10);
        assert_eq!(map.note_rows(0), Some(8..9));
    }

    /// M11ag: with Wrap, a note follows its line's last row.
    #[test]
    fn note_rows_follow_a_wrapped_line() {
        let map = RowMap::file(&[1, 9, 1], Some(4), &[], &[note(1, 1, 0)], 10.0);
        let rows: Vec<_> = (0..map.len()).filter_map(|r| map.row(r)).collect();
        assert_eq!(
            rows,
            [
                Row::Line { line: 0, cell0: 0 },
                Row::Line { line: 1, cell0: 0 },
                Row::Line { line: 1, cell0: 4 },
                Row::Line { line: 1, cell0: 8 },
                Row::Note { note: 0, k: 0 },
                Row::Line { line: 2, cell0: 0 },
            ]
        );
        assert_eq!(map.first_row(2), Some(5));
        assert_eq!(map.line_of(4), Some(1));
        assert!(map.hidden_after(1).is_empty());
    }

    /// M11ag: Diff mode: a note follows its row unified, the whole pair side by side (inside
    /// the card); none in a folded stretch or a closed card; a press on an empty half looks
    /// past the note's rows.
    #[test]
    fn diff_note_rows_follow_the_row_and_the_pair() {
        use crate::diff_view::{DiffRow, RowKind};
        let row = |kind, old, new| DiffRow { kind, old, new };
        let rows = vec![
            row(RowKind::Same, Some(1), Some(1)),
            row(RowKind::Same, Some(2), Some(2)),
            row(RowKind::Removed, Some(3), None),
            row(RowKind::Added, None, Some(3)),
            row(RowKind::Added, None, Some(4)),
            row(RowKind::Same, Some(4), Some(5)),
        ];
        let g = crate::diff_cards::group(&rows);
        let pairs: Vec<CardPairs> = (g.cards.iter())
            .map(|c| crate::diff_cards::pair(&rows, c.rows.clone()))
            .collect();
        let layout = |pairs| DiffLayout {
            grouped: &g,
            shown: &[],
            closed: &[false],
            pairs,
        };
        let widths = [1, 1, 9, 2, 5, 1];
        // Unified: title | 0 | 1 | 2 | 3 | note | 4 | 5.
        let map = RowMap::diff(&widths, None, &layout(None), &[note(3, 1, 0)], 10.0);
        assert_eq!(map.len(), 8);
        assert_eq!(map.row(4), line(3, 0));
        assert_eq!(map.row(5), Some(Row::Note { note: 0, k: 0 }));
        assert_eq!(map.row(6), line(4, 0));
        assert_eq!(map.card_rows(0), Some(0..8), "inside its card");
        assert_eq!(map.line_of(5), Some(3));
        // Side by side: title | 0 | 1 | 2·3 | 2·_ | 2·_ | note | _·4 | _·4 | 5.
        let map = RowMap::diff(
            &widths,
            Some(4),
            &layout(Some(&pairs)),
            &[note(3, 1, 0)],
            10.0,
        );
        assert_eq!(map.note_rows(0), Some(6..7));
        assert_eq!(
            map.row(7),
            Some(Row::Pair {
                old: None,
                new: Some((4, 0))
            })
        );
        assert_eq!(map.line_of(6), Some(3));
        assert_eq!(map.first_row(4), Some(7));
        assert_eq!(
            map.nearest_on(7, Half::Old),
            Some((2, true)),
            "past the note"
        );
        assert_eq!(
            map.nearest_on(6, Half::New),
            Some((3, true)),
            "the note's line"
        );
        // A note in a folded stretch or a closed card: no rows.
        let g = two_cards();
        let widths = vec![1; 38];
        let notes = [note(3, 1, 0), note(9, 1, 1), note(24, 1, 2)];
        let layout = DiffLayout {
            grouped: &g,
            shown: &[false, false, false],
            closed: &[true, false],
            pairs: None,
        };
        let map = RowMap::diff(&widths, None, &layout, &notes, 10.0);
        assert_eq!((map.note_rows(0), map.note_rows(1)), (None, None));
        let at = map.first_row(24).unwrap() + 1;
        assert_eq!(map.note_rows(2), Some(at..at + 1));
        assert_eq!(map.card_at(at), Some(1));
    }

    /// M11ag: presses and the rows in sight count note rows.
    #[test]
    fn row_at_and_rows_in_count_note_rows() {
        let map = RowMap::file(&[1; 10], None, &[], &[note(4, 2, 0)], 10.0);
        assert_eq!(
            [45.0, 55.0, 65.0, 70.0].map(|y| map.row_at(y)),
            [4, 5, 6, 7]
        );
        assert_eq!(map.row(map.row_at(75.0)), line(5, 0));
        assert_eq!(map.rows_in(45.0..75.0), 4..8);
        assert_eq!(map.row_at(1000.0), 11);
    }

    /// M11ag: Follow and reveals land on lines: each line's first row, its holding row and the
    /// fold's lines are the same with notes.
    #[test]
    fn first_row_is_unchanged_by_notes() {
        let map = RowMap::file(&[1, 9, 1, 1, 1], Some(4), &[3..4], &[note(1, 2, 0)], 10.0);
        for l in [0, 1, 2, 4] {
            assert_eq!(map.row(map.first_row(l).unwrap()), line(l, 0), "line {l}");
            assert_eq!(map.row_holding(l), map.first_row(l));
        }
        assert_eq!(map.row_holding(3), map.first_row(2), "hidden: its fold's");
        assert_eq!(map.lines_held_at(map.first_row(2).unwrap()), Some(3..4));
    }
}
