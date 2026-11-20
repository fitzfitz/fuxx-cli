//! M11ag Part 2: the agent's reason drawn under its edit. Each placed note is shaped once in the
//! UI font, wrapped at the code's visible width (`NoteShapes`, a layout cache made again only for
//! another notes generation, width or theme), and takes rows in the row map: two folded, all of
//! them open (`note_spans`). Its rows show a bar in the agent's colour, the prefix
//! (`claude · 10:42 — `) in the dim text colour, then the reason as its raw characters (no
//! Markdown), and "more" / "less" at the end of the last row shown when there is more than two.
//! Nothing here changes while drawing: the shapes and the spans are made in the canvas's
//! prepaint, with the row map, only when their key changes (M11p).

use super::*;
use crate::row_map::NoteSpan;
use gpui::{ShapedLine, WrappedLine};
use std::collections::HashSet;

/// A folded note's rows (at most): all of them when opened.
pub(super) const FOLDED_ROWS: usize = 2;
/// The words that open and fold a note longer than `FOLDED_ROWS`.
pub(super) const MORE: &str = "more";
pub(super) const LESS: &str = "less";

/// A note's first words: who and when, then a dash (`claude · 10:42 — `).
pub fn note_prefix(note: &Note) -> String {
    format!("{} \u{b7} {} \u{2014} ", note.agent.name, note.clock)
}

/// The pill's words for the notes placed, one name per note (`[]`: no pill): "1 note from
/// claude", "2 notes from claude", and "2 notes" when both agents have some.
pub fn notes_pill_words(names: &[&str]) -> Option<String> {
    let first = names.first()?;
    let n = names.len();
    let s = if n == 1 { "" } else { "s" };
    Some(if names.iter().all(|name| name == first) {
        format!("{n} note{s} from {first}")
    } else {
        format!("{n} notes")
    })
}

/// Plan-time call 17: the note "next" goes to: the first whose first row starts at or below the
/// view's bottom edge (`note_tops`: each note's top, in document order), else the first. None
/// with no notes.
pub fn next_note(note_tops: &[f32], view_bottom: f32) -> Option<usize> {
    let below = note_tops.iter().position(|&top| top >= view_bottom);
    below.or(if note_tops.is_empty() { None } else { Some(0) })
}

/// A note as shaped: its wrapped lines, and its rows when open (`open_rows`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shaped {
    pub lines: usize,
    pub open: usize,
}

/// An open note's rows (fix round 1): its `lines`, and one more when its last line, ending
/// `last_end` points into the note's `room`, runs into the box "less" (`label` wide, with
/// `code::NOTE_MORE_GAP` before it) takes at the end of the last row: "less" then sits alone on
/// that extra row, so it never covers a word. A note of `FOLDED_ROWS` lines or fewer has no
/// "less".
pub fn open_rows(lines: usize, last_end: f32, room: f32, label: f32) -> usize {
    if lines > FOLDED_ROWS && last_end > room - label - code::NOTE_MORE_GAP {
        lines + 1
    } else {
        lines
    }
}

/// A note's rows: at most `FOLDED_ROWS` of its lines, or its rows when `open`.
pub fn note_rows(s: Shaped, open: bool) -> usize {
    if open {
        s.open
    } else {
        s.lines.min(FOLDED_ROWS)
    }
}

/// Each placed note's span in the rows (sorted by the row it is under): `shaped[i]` is note
/// `i` as shaped, its rows folded or `open` (by note id); `under_row` gives the doc line a
/// note's file line is drawn under (none: the note isn't shown).
pub fn note_spans(
    placed: &[Placed],
    shaped: &[Shaped],
    open: &HashSet<u64>,
    under_row: impl Fn(usize) -> Option<usize>,
) -> Vec<NoteSpan> {
    let mut spans: Vec<NoteSpan> = (placed.iter().enumerate())
        .filter_map(|(i, p)| {
            let n = *shaped.get(i)?;
            Some(NoteSpan {
                under: under_row(p.under)?,
                rows: note_rows(n, open.contains(&p.note.id)),
                note: i,
            })
        })
        .collect();
    spans.sort_by_key(|s| s.under);
    spans
}

/// Diff mode (plan-time call 16): the diff row each note's file line is drawn on, the row
/// whose new line number is it (`under + 1`, 1-based); a note under a line the diff doesn't
/// show (a removed-only neighbourhood) has none. One pass over the rows.
pub fn under_rows(rows: &[DiffRow], placed: &[Placed]) -> HashMap<usize, usize> {
    let wanted: HashSet<usize> = placed.iter().map(|p| p.under + 1).collect();
    (rows.iter().enumerate())
        .filter_map(|(r, row)| {
            let new = row.new.filter(|n| wanted.contains(n))?;
            Some((new - 1, r))
        })
        .collect()
}

/// The spans of `placed`'s rows (`counts`: each one's wrapped lines) in `mode`'s rows: a file's
/// under their lines; a diff's (`rows`) under the rows of their new lines (`under_rows`); a
/// diff with no rows: none.
pub(super) fn spans_for(
    placed: &[Placed],
    counts: &[Shaped],
    open: &HashSet<u64>,
    mode: FileMode,
    rows: Option<&[DiffRow]>,
) -> Vec<NoteSpan> {
    match (mode, rows) {
        (FileMode::Plain, _) => note_spans(placed, counts, open, Some),
        (FileMode::Diff, Some(rows)) => {
            let at = under_rows(rows, placed);
            note_spans(placed, counts, open, |under| at.get(&under).copied())
        }
        (FileMode::Diff, None) => Vec::new(),
    }
}

/// Where a note's text starts, from the body's left edge: the code's text column (side by side:
/// the old half's; the note runs on under the new half).
pub(super) fn note_left(cell: f32, gutter: Gutter, side: bool) -> f32 {
    if side {
        code::CARD_INSET + side_text_left(cell, gutter)
    } else {
        columns(cell, gutter).text_left
    }
}

/// The width a note wraps at in a body `width` points wide, its text starting `left` points in:
/// to the body's right edge (a diff's: inside its cards), less `RIGHT_PAD`, as the code's text
/// (with or without Wrap; side by side, across both halves).
pub(super) fn note_room(width: f32, left: f32, mode: FileMode) -> f32 {
    let inset = match mode {
        FileMode::Diff => code::CARD_INSET,
        FileMode::Plain => 0.0,
    };
    width - inset - RIGHT_PAD - left
}

/// An agent's colour as its marks have it: its provider's, held to `MARK_MIN` on `bg`; an agent
/// without one grey.
pub(super) fn agent_ink(a: &AgentMark, ui: &UiColors, bg: Rgb) -> Rgb {
    match a.rgb {
        Some(c) => {
            let c = Rgb {
                r: (c >> 16) as u8,
                g: (c >> 8) as u8,
                b: c as u8,
            };
            ensure_contrast(c, bg, MARK_MIN)
        }
        None => ui.tab(TabColor::Grey),
    }
}

/// Each placed note's text as shaped for its rows (a layout cache: made again only for another
/// notes generation, width or theme), and the words that open and fold one.
pub(super) struct NoteShapes {
    pub key: ShapeKey,
    /// Note `i`'s prefix and reason, wrapped at the key's width (none: it didn't shape).
    pub lines: Vec<Option<WrappedLine>>,
    /// Note `i`'s rows when open (`open_rows`; 0: it didn't shape).
    pub opens: Vec<usize>,
    pub more: ShapedLine,
    pub less: ShapedLine,
}

/// What the notes were shaped for: the notes' generation, the width (whole points), the theme.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct ShapeKey {
    pub notes: u64,
    pub width: u32,
    pub theme: Theme,
}

impl NoteShapes {
    /// Each note's wrapped lines and its rows open (0: it didn't shape).
    pub fn counts(&self) -> Vec<Shaped> {
        (self.lines.iter().zip(&self.opens))
            .map(|(l, &open)| Shaped {
                lines: l.as_ref().map_or(0, lines_of),
                open,
            })
            .collect()
    }

    /// Note `i`'s wrapped line count (none: no such note, or it didn't shape).
    fn count(&self, i: usize) -> Option<usize> {
        self.lines.get(i)?.as_ref().map(lines_of)
    }
}

fn lines_of(line: &WrappedLine) -> usize {
    line.wrap_boundaries().len() + 1
}

/// Where a wrapped text's last line ends, from its start: the text's width less where its last
/// wrap boundary's glyph (the last line's first) sits in the unwrapped line.
fn last_end(line: &WrappedLine) -> f32 {
    let layout = &line.unwrapped_layout;
    let start = (line.wrap_boundaries().last())
        .and_then(|b| layout.runs.get(b.run_ix)?.glyphs.get(b.glyph_ix))
        .map_or(px(0.0), |g| g.position.x);
    f32::from(layout.width - start)
}

/// The notes shaped for `key` (the cache's when it was made for it): each note's prefix in the
/// dim text colour, then its reason in the text colour, in the UI font (`text::UI`), wrapped at
/// the key's width; and "more" / "less" in the dim colour.
pub(super) fn shapes_for(
    cache: &RefCell<Option<Rc<NoteShapes>>>,
    key: ShapeKey,
    placed: &[Placed],
    window: &mut Window,
) -> Rc<NoteShapes> {
    if let Some(shapes) = cache.borrow().as_ref().filter(|s| s.key == key) {
        return shapes.clone();
    }
    let (ui, bg) = (key.theme.ui(), key.theme.background);
    let dim = ensure_contrast(ui.text_dim, bg, TEXT_MIN);
    let font = window.text_style().font();
    let run = |len: usize, ink: Rgb| TextRun {
        len,
        font: font.clone(),
        color: hsla(ink),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let size = px(text::UI);
    let lines: Vec<Option<WrappedLine>> = (placed.iter())
        .map(|p| {
            let prefix = note_prefix(&p.note);
            let runs = [run(prefix.len(), dim), run(p.note.reason.len(), ui.text)];
            let words = SharedString::from(format!("{prefix}{}", p.note.reason));
            let width = Some(px(key.width as f32));
            let shaped = window
                .text_system()
                .shape_text(words, size, &runs, width, None);
            shaped.ok().and_then(|lines| lines.into_iter().next())
        })
        .collect();
    let word = |w: &'static str, window: &mut Window| {
        let run = run(w.len(), dim);
        window
            .text_system()
            .shape_line(SharedString::from(w), size, &[run], None)
    };
    let (more, less) = (word(MORE, window), word(LESS, window));
    let label = f32::from(less.width());
    let opens = (lines.iter())
        .map(|l: &Option<WrappedLine>| {
            l.as_ref().map_or(0, |l| {
                open_rows(lines_of(l), last_end(l), key.width as f32, label)
            })
        })
        .collect();
    let shapes = Rc::new(NoteShapes {
        key,
        lines,
        opens,
        more,
        less,
    });
    *cache.borrow_mut() = Some(shapes.clone());
    shapes
}

/// A note's bar, `h` tall from `top` (window points): `code::NOTE_BAR` wide, centred in the
/// gutter's marks column (a file with marks), else in the change bars' (a file's) or the signs'
/// (a diff's; side by side, the old half's).
pub(super) fn bar_bounds(g: &Geom, top: Pixels, h: Pixels, mode: FileMode) -> Bounds<Pixels> {
    let (left, w) = match (mode, g.marks_left) {
        (FileMode::Diff, _) => (g.bar_left, g.cell_width),
        (FileMode::Plain, Some(marks)) => (marks, px(MARK_COLUMN)),
        (FileMode::Plain, None) => (g.bar_left, px(BAR_WIDTH)),
    };
    let x = g.bounds.left() + left + (w - px(code::NOTE_BAR)) / 2.0;
    Bounds::new(point(x, top), size(px(code::NOTE_BAR), h))
}

/// Where "more" / "less" (`w` wide) sits on a note's last row shown (its top `y`, window
/// points): ending at the end of the note's room (`room` points from the text column).
pub(super) fn more_box(g: &Geom, y: Pixels, w: Pixels, room: u32) -> Bounds<Pixels> {
    let right = g.bounds.left() + g.text_left + px(room as f32);
    Bounds::new(point(right - w, y), size(w, g.line_height))
}

/// Paint note `i`'s rows (in `map`, as `f` and `g` lay them out): its bar, then its text from
/// the code's text column, one wrapped line a row (the row height as line height), cut at its
/// last row shown; past two lines, "more" (folded) or "less" (open) at the end of that row, on
/// the background so no word runs under it (open, a last line running into it gave "less" a row
/// of its own: `open_rows`).
pub(super) fn paint_note(
    f: &Frame,
    g: &Geom,
    map: &RowMap,
    shapes: &NoteShapes,
    i: usize,
    window: &mut Window,
    cx: &mut App,
) {
    let (Some(rows), Some(placed), Some(Some(line))) =
        (map.note_rows(i), f.notes.get(i), shapes.lines.get(i))
    else {
        return;
    };
    let (ui, bg) = (f.theme.ui(), f.theme.background);
    let lh = g.line_height;
    let top = g.bounds.top() + px(map.top(rows.start)) - g.scroll.y;
    let h = lh * rows.len() as f32;
    let bar = bar_bounds(g, top, h, f.mode);
    window.paint_quad(fill(bar, hsla(agent_ink(&placed.note.agent, &ui, bg))));
    let left = g.bounds.left() + g.text_left;
    let clip = Bounds::from_corners(point(left, top), point(g.bounds.right(), top + h));
    window.with_content_mask(Some(ContentMask { bounds: clip }), |window| {
        let _ = line.paint(point(left, top), lh, TextAlign::Left, None, window, cx);
        if lines_of(line) <= FOLDED_ROWS {
            return;
        }
        let word = if f.notes_open.contains(&placed.note.id) {
            &shapes.less
        } else {
            &shapes.more
        };
        let y = top + lh * (rows.len() - 1) as f32;
        let at = more_box(g, y, word.width(), shapes.key.width);
        let ground = Bounds::from_corners(
            point(at.left() - px(code::NOTE_MORE_GAP), y),
            point(at.right(), y + lh),
        );
        window.paint_quad(fill(ground, hsla(bg)));
        let _ = word.paint(at.origin, lh, TextAlign::Left, None, window, cx);
    });
}

impl FileTab {
    /// The notes' spans in the rows of the mode shown, as the last drawing shaped them (none
    /// shaped for these notes yet: none; the next drawing shapes them and makes the rows).
    pub(super) fn note_spans_now(&self) -> Vec<NoteSpan> {
        let shapes = self.note_shapes.borrow();
        let Some(s) = (shapes.as_ref())
            .filter(|s| s.key.notes == self.notes_gen)
            .filter(|_| !self.notes_hidden)
        else {
            return Vec::new();
        };
        let rows = match &self.diff.state {
            DiffState::Rows(d) => Some(d.rows.as_slice()),
            _ => None,
        };
        spans_for(&self.notes, &s.counts(), &self.notes_open, self.mode, rows)
    }

    /// Note `i`'s "more" / "less" as last drawn (none: two lines or fewer, no rows, or not
    /// shaped for these notes).
    fn note_more_at(&self, i: usize) -> Option<Bounds<Pixels>> {
        let g = self.geom.get()?;
        let shapes = self.note_shapes.borrow();
        let s = shapes.as_ref().filter(|s| s.key.notes == self.notes_gen)?;
        if s.count(i)? <= FOLDED_ROWS {
            return None;
        }
        let rows = self.rows.borrow();
        let (_, map) = rows.as_ref()?;
        let r = map.note_rows(i)?;
        let word = if self.notes_open.contains(&self.notes.get(i)?.note.id) {
            &s.less
        } else {
            &s.more
        };
        let y = g.bounds.top() + px(map.top(r.start)) - g.scroll.y
            + g.line_height * (r.len() - 1) as f32;
        Some(more_box(&g, y, word.width(), s.key.width))
    }

    /// The note whose "more" / "less" is under window point `p` (in the text, as last drawn).
    pub(super) fn note_toggle_under(&self, p: Point<Pixels>) -> Option<usize> {
        let g = self.geom.get()?;
        if !g.bounds.contains(&p) {
            return None;
        }
        (0..self.notes.len()).find(|&i| self.note_more_at(i).is_some_and(|b| b.contains(&p)))
    }

    /// Note `i` opened to all its rows, or folded to two: its rows are made again (the
    /// drawing keeps the line at the top where it is).
    pub(super) fn toggle_note(&mut self, i: usize, cx: &mut Context<Self>) {
        let Some(id) = self.notes.get(i).map(|p| p.note.id) else {
            return;
        };
        if !self.notes_open.remove(&id) {
            self.notes_open.insert(id);
        }
        self.note_rows_gen += 1;
        cx.notify();
    }
}

/// The eye's tooltip: what a click does.
pub(super) fn eye_tip(hidden: bool) -> &'static str {
    if hidden { "Show notes" } else { "Hide notes" }
}

impl FileTab {
    /// The pill's words and whether the eye has the notes hidden (none: no pill): shown in File
    /// and Diff modes, not in Preview, for the notes placed in the text, shown or hidden.
    fn notes_pill_state(&self) -> Option<(String, bool)> {
        if self.preview_shown() {
            return None;
        }
        let names: Vec<&str> = self
            .notes
            .iter()
            .map(|p| p.note.agent.name.as_ref())
            .collect();
        Some((notes_pill_words(&names)?, self.notes_hidden))
    }

    /// The notes pill beside Follow (M11ag Part 2): "2 notes from claude", a click scrolls to
    /// the next note below the view; and the eye after it, hiding or showing every note row in
    /// this tab.
    pub(super) fn notes_pill(&self, ui: UiColors, cx: &mut Context<Self>) -> Option<AnyElement> {
        self.notes_at.set([None; 2]);
        let (words, hidden) = self.notes_pill_state()?;
        let (pill_at, eye_at) = (self.notes_at.clone(), self.notes_at.clone());
        let tip = eye_tip(hidden);
        let kind = if hidden { Glyph::EyeOff } else { Glyph::Eye };
        let ink = ensure_contrast(ui.text_dim, ui.card, TEXT_MIN);
        let part = |id: &'static str| {
            div()
                .id(id)
                .relative()
                .flex()
                .flex_none()
                .items_center()
                .py(px(2.0))
                .rounded(px(999.0))
                .cursor_pointer()
                .hover(move |s| s.bg(hsla(ui.panel_hover)))
        };
        let place = |at: Rc<Cell<[Option<Bounds<Pixels>>; 2]>>, k: usize| {
            canvas(
                move |b, _, _| {
                    let mut both = at.get();
                    both[k] = Some(b);
                    at.set(both);
                },
                |_, _, _, _| {},
            )
            .absolute()
            .top_0()
            .left_0()
            .size_full()
        };
        Some(
            div()
                .flex()
                .flex_none()
                .flex_row()
                .items_center()
                .text_size(px(text::META))
                .text_color(hsla(ink))
                .child(
                    part("file-notes")
                        .px(px(8.0))
                        .whitespace_nowrap()
                        .child(SharedString::from(words))
                        .child(place(pill_at, 0))
                        .on_click(
                            cx.listener(|this, _: &ClickEvent, _, cx| this.next_note_click(cx)),
                        ),
                )
                .child(
                    part("file-notes-eye")
                        .px(px(5.0))
                        .child(glyph(kind, ink))
                        .child(place(eye_at, 1))
                        .on_click(
                            cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_notes_hidden(cx)),
                        )
                        .tooltip(move |_, cx| -> AnyView { cx.new(|_| Tip(tip.into())).into() }),
                )
                .into_any_element(),
        )
    }

    /// The eye: every note row of this tab hidden or shown again (the rows are made again).
    pub(super) fn toggle_notes_hidden(&mut self, cx: &mut Context<Self>) {
        self.notes_hidden = !self.notes_hidden;
        self.note_rows_gen += 1;
        cx.notify();
    }

    /// Each note's index and the row it is under (a file's: its line; a diff's: the row of its
    /// new line; a note under a line the diff doesn't show has none).
    fn note_unders(&self) -> Vec<(usize, usize)> {
        match (self.mode, &self.diff.state) {
            (FileMode::Plain, _) => (self.notes.iter().enumerate())
                .map(|(i, p)| (i, p.under))
                .collect(),
            (FileMode::Diff, DiffState::Rows(d)) => {
                let at = under_rows(&d.rows, &self.notes);
                (self.notes.iter().enumerate())
                    .filter_map(|(i, p)| Some((i, *at.get(&p.under)?)))
                    .collect()
            }
            (FileMode::Diff, _) => Vec::new(),
        }
    }

    /// Each shown note's top (as drawn; hidden by the eye, just under its line) and its line's
    /// row in `map`, by top. A note under a fold has no row and is left out.
    fn note_tops(&self, map: &RowMap) -> Vec<(f32, usize)> {
        let mut notes: Vec<(f32, usize)> = (self.note_unders().into_iter())
            .filter_map(|(i, under)| {
                let row = map.first_row(under)?;
                let top =
                    (map.note_rows(i)).map_or(map.top(row) + map.row_h(), |r| map.top(r.start));
                Some((top, row))
            })
            .collect();
        notes.sort_by(|a, b| a.0.total_cmp(&b.0));
        notes
    }

    /// A click on the pill: the next note below the view (`next_note`; wrapping to the first)
    /// has its under-line scrolled to the top as Find's reveal does (`reveal_top`). A note under
    /// a fold has no row and is passed over; when every note is under one (File mode), the
    /// first note's folds open, as a find step's do, and the click goes there. Hidden notes are
    /// found by their lines. It is your scroll: Follow ends.
    pub(super) fn next_note_click(&mut self, cx: &mut Context<Self>) {
        let (Some(g), Some(mut map)) = (self.geom.get(), self.map_now(cx)) else {
            return;
        };
        let mut notes = self.note_tops(&map);
        if notes.is_empty()
            && self.mode == FileMode::Plain
            && let Some(line) = self.notes.first().map(|p| p.under)
            && self.open_folds_at(line)
        {
            cx.notify();
            let Some(fresh) = self.fresh_map(cx) else {
                return;
            };
            map = fresh;
            notes = self.note_tops(&map);
        }
        let tops: Vec<f32> = notes.iter().map(|n| n.0).collect();
        let bottom = f32::from(g.scroll.y + g.bounds.size.height);
        let Some(k) = next_note(&tops, bottom) else {
            return;
        };
        let row = notes[k].1;
        let mode = self.mode;
        let y = match mode {
            FileMode::Diff => diff_reveal_y(&map, row),
            FileMode::Plain => map.top(reveal_top(row, |r| self.sticky_at_row(&map, r))),
        };
        let x = self.mode_scroll_mut(mode).x;
        self.set_scroll(point(x, px(y)), cx);
    }
}

#[cfg(feature = "ui-shots")]
impl FileTab {
    fn note_index(&self, id: u64) -> Option<usize> {
        self.notes.iter().position(|p| p.note.id == id)
    }

    /// The notes' rows as last drawn: each note's id and its rows (none drawn: not listed).
    pub fn note_rows_drawn(&self) -> Vec<(u64, Range<usize>)> {
        let rows = self.rows.borrow();
        let Some((_, map)) = rows.as_ref() else {
            return Vec::new();
        };
        (self.notes.iter().enumerate())
            .filter_map(|(i, p)| Some((p.note.id, map.note_rows(i)?)))
            .collect()
    }

    /// Where note `id`'s "more" / "less" is drawn (none: it has two rows or fewer, or none).
    pub fn note_more_bounds(&self, id: u64) -> Option<Bounds<Pixels>> {
        self.note_more_at(self.note_index(id)?)
    }

    /// Where note `id`'s bar is drawn (none: no rows).
    pub fn note_bar_bounds(&self, id: u64) -> Option<Bounds<Pixels>> {
        let i = self.note_index(id)?;
        let g = self.geom.get()?;
        let rows = self.rows.borrow();
        let (_, map) = rows.as_ref()?;
        let r = map.note_rows(i)?;
        let top = g.bounds.top() + px(map.top(r.start)) - g.scroll.y;
        Some(bar_bounds(
            &g,
            top,
            g.line_height * r.len() as f32,
            self.mode,
        ))
    }

    /// Note `id`'s text as shaped (its prefix and reason) and its wrapped line count.
    pub fn note_shaped(&self, id: u64) -> Option<(String, usize)> {
        let i = self.note_index(id)?;
        let shapes = self.note_shapes.borrow();
        let s = shapes.as_ref().filter(|s| s.key.notes == self.notes_gen)?;
        let line = s.lines.get(i)?.as_ref()?;
        Some((line.text.to_string(), lines_of(line)))
    }

    /// Note `id`'s last line as shaped: the note's line count, where its last line ends from
    /// the text column, and the room it wraps in (points).
    pub fn note_last_line(&self, id: u64) -> Option<(usize, f32, f32)> {
        let i = self.note_index(id)?;
        let shapes = self.note_shapes.borrow();
        let s = shapes.as_ref().filter(|s| s.key.notes == self.notes_gen)?;
        let line = s.lines.get(i)?.as_ref()?;
        Some((lines_of(line), last_end(line), s.key.width as f32))
    }

    /// The notes pill as it stands: its words and whether the eye has the notes hidden (none: no
    /// pill).
    pub fn notes_pill_for_shots(&self) -> Option<(String, bool)> {
        self.notes_pill_state()
    }

    /// Where the notes pill and its eye were drawn (none: not drawn).
    pub fn notes_pill_bounds(&self) -> Option<Bounds<Pixels>> {
        self.notes_at.get()[0]
    }

    pub fn notes_eye_bounds(&self) -> Option<Bounds<Pixels>> {
        self.notes_at.get()[1]
    }

    /// Every fold opened (as Unfold All does).
    pub fn unfold_all_for_shots(&mut self, cx: &mut Context<Self>) {
        self.set_folds(BTreeSet::new(), cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit_ledger::agent_mark_for;

    fn placed(id: u64, under: usize) -> Placed {
        Placed {
            note: Note {
                id,
                agent: agent_mark_for("claude"),
                clock: "10:42".into(),
                reason: "why".into(),
                reason_key: format!("a1:{id}").into(),
                anchor: Arc::new(vec!["x".into()]),
                hint: under,
            },
            under,
        }
    }

    fn span(under: usize, rows: usize, note: usize) -> NoteSpan {
        NoteSpan { under, rows, note }
    }

    /// `lines` wrapped lines whose last one stays clear of "less" (open: as many rows).
    fn n(lines: usize) -> Shaped {
        Shaped { lines, open: lines }
    }

    #[test]
    fn note_spans_fold_to_two_rows_and_open_to_all() {
        let notes = [placed(7, 4), placed(9, 10)];
        let none = HashSet::new();
        // Five lines folded: two rows; one line: one row (no "more").
        let spans = note_spans(&notes, &[n(5), n(1)], &none, Some);
        assert_eq!(spans, [span(4, 2, 0), span(10, 1, 1)]);
        // Opened (by id): all five.
        let open = HashSet::from([7]);
        let spans = note_spans(&notes, &[n(5), n(1)], &open, Some);
        assert_eq!(spans, [span(4, 5, 0), span(10, 1, 1)]);
        // A note opened that has one line: still one.
        let open = HashSet::from([7, 9]);
        assert_eq!(
            note_spans(&notes, &[n(5), n(1)], &open, Some)[1],
            span(10, 1, 1)
        );
        assert_eq!(
            (
                note_rows(n(5), false),
                note_rows(n(5), true),
                note_rows(n(1), false)
            ),
            (2, 5, 1)
        );
        assert_eq!((note_rows(n(2), false), note_rows(n(0), true)), (2, 0));
        // Fix round 1: its last line under "less": open, one row more; folded, still two.
        let under_less = Shaped { lines: 5, open: 6 };
        let spans = note_spans(&notes, &[under_less, n(1)], &HashSet::from([7]), Some);
        assert_eq!(spans[0], span(4, 6, 0));
        assert_eq!(note_rows(under_less, false), 2);
        // A note not shaped yet: no span.
        assert_eq!(note_spans(&notes, &[n(3)], &none, Some), [span(4, 2, 0)]);
        // Sorted by the row they are under, whatever order they came in.
        let swapped = [placed(9, 10), placed(7, 4)];
        let spans = note_spans(&swapped, &[n(1), n(5)], &none, Some);
        assert_eq!(spans, [span(4, 2, 1), span(10, 1, 0)]);
    }

    #[test]
    fn note_spans_in_diff_follow_the_new_line() {
        let row = |kind, old, new| DiffRow { kind, old, new };
        // Old line 43 removed, new 43 and 44 added; new line 45 unchanged (old 44).
        let rows = [
            row(RowKind::Same, Some(42), Some(42)),
            row(RowKind::Removed, Some(43), None),
            row(RowKind::Added, None, Some(43)),
            row(RowKind::Added, None, Some(44)),
            row(RowKind::Same, Some(44), Some(45)),
        ];
        // 0-based file line 43 is new line 44: row 3; 44 is new 45: row 4.
        let notes = [placed(1, 43), placed(2, 44), placed(3, 90)];
        let at = under_rows(&rows, &notes);
        assert_eq!(at.get(&43), Some(&3));
        assert_eq!(at.get(&44), Some(&4));
        assert_eq!(at.get(&90), None, "a line the diff doesn't show");
        let shaped = [n(1), n(3), n(1)];
        let spans = note_spans(&notes, &shaped, &HashSet::new(), |u| at.get(&u).copied());
        assert_eq!(spans, [span(3, 1, 0), span(4, 2, 1)], "none: no span");
        // A removed-only neighbourhood: the old line's row has no new number.
        let removed = [row(RowKind::Removed, Some(44), None)];
        assert!(under_rows(&removed, &[placed(1, 43)]).is_empty());
    }

    /// Fix round 1: "less" sits at the end of an open note's last row, `NOTE_MORE_GAP` plus its
    /// width; a last line that runs into that box gets a row of its own for "less".
    #[test]
    fn less_takes_a_row_of_its_own_when_the_last_line_runs_under_it() {
        let gap = code::NOTE_MORE_GAP;
        let (room, label) = (600.0, 26.0);
        let edge = room - label - gap;
        // Clear of the box: as many rows as lines.
        assert_eq!(open_rows(5, edge - 1.0, room, label), 5);
        assert_eq!(open_rows(5, edge, room, label), 5, "touching is clear");
        // Into the box, or a full last line: one row more.
        assert_eq!(open_rows(5, edge + 0.5, room, label), 6);
        assert_eq!(open_rows(3, room, room, label), 4);
        // Two lines or fewer have no "less": never more rows.
        assert_eq!(open_rows(2, room, room, label), 2);
        assert_eq!(open_rows(1, room, room, label), 1);
    }

    #[test]
    fn notes_pill_words_one_two_both() {
        assert_eq!(notes_pill_words(&[]), None);
        assert_eq!(
            notes_pill_words(&["claude"]).as_deref(),
            Some("1 note from claude")
        );
        assert_eq!(
            notes_pill_words(&["claude", "claude"]).as_deref(),
            Some("2 notes from claude")
        );
        assert_eq!(
            notes_pill_words(&["agy", "agy", "agy"]).as_deref(),
            Some("3 notes from agy")
        );
        assert_eq!(
            notes_pill_words(&["claude", "agy"]).as_deref(),
            Some("2 notes")
        );
    }

    #[test]
    fn next_note_below_the_view_else_the_first() {
        assert_eq!(next_note(&[], 100.0), None);
        // The first note whose top is at or below the view's bottom.
        assert_eq!(next_note(&[10.0, 90.0, 150.0, 400.0], 100.0), Some(2));
        assert_eq!(next_note(&[10.0, 100.0, 150.0], 100.0), Some(1));
        // None below: wrap to the first.
        assert_eq!(next_note(&[10.0, 90.0], 100.0), Some(0));
        assert_eq!(next_note(&[50.0], 100.0), Some(0));
    }

    #[test]
    fn note_prefix_is_agent_clock_dash() {
        let p = placed(1, 0);
        assert_eq!(note_prefix(&p.note), "claude \u{b7} 10:42 \u{2014} ");
        let mut agy = p.note.clone();
        agy.agent = agent_mark_for("agy");
        agy.clock = "09:05".into();
        assert_eq!(note_prefix(&agy), "agy \u{b7} 09:05 \u{2014} ");
    }
}
