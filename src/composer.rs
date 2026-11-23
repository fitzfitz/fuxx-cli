//! The agent tab's message box: a multi-line text input (M11aa). Modelled on `name_input.rs`
//! (IME through `EntityInputHandler`, UTF-16 offsets at the edge, UTF-8 inside), but multi-line:
//! Enter sends, ⇧Enter is a newline, Esc stops the agent, and losing focus does not cancel.
//! Long lines soft-wrap at the box's width (measured in its font's widest cell, `m`); ↑/↓ move
//! by visual row. It grows to `MAX_ROWS` rows, then scrolls to keep the caret in view (only the
//! rows on screen are shaped); the wheel scrolls it too. A row still wider than the box (wide
//! characters) scrolls sideways with the caret.
//!
//! The editing itself is `EditBuffer`, pure and unit-tested; `Composer` is a thin layer of
//! actions, IME and painting over it.

use std::ops::Range;
use std::time::Instant;

use alacritty_terminal::vte::ansi::CursorShape;

use gpui::{
    App, AvailableSpace, Bounds, ClipboardItem, Context, CursorStyle, DispatchPhase, ElementId,
    ElementInputHandler, Entity, EntityInputHandler, EventEmitter, FocusHandle, Focusable,
    GlobalElementId, Hitbox, HitboxBehavior, InspectorElementId, LayoutId, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point, ScrollWheelEvent, ShapedLine,
    SharedString, Style, Subscription, TextAlign, TextRun, UTF16Selection, UnderlineStyle, Window,
    actions, div, fill, point, prelude::*, px, relative, size,
};

use crate::appearance::{Appearance, CursorFun, CursorShapeSetting};
use crate::cursor_look::{self, Frames, PaintShape};
use crate::grid::{CellView, CursorView, GridSnapshot};
use crate::name_input::{next_char, prev_char, utf8_offset, utf16_offset, word_end, word_start};
use crate::status::Status;
use crate::terminal_view::CellMetrics;
use crate::terminal_view::hsla;
use crate::terminal_view::{CursorMotion, CursorSettings, breath_redraw, breath_sleep};

/// Rows shown before the box scrolls.
const MAX_ROWS: usize = 8;

// Bound in main.rs with the "Composer" key context, so these keys never reach the terminal.
actions!(
    composer,
    [
        Send,
        Newline,
        Stop,
        Backspace,
        Clear,
        Delete,
        DeleteWordLeft,
        DeleteToLineEnd,
        Copy,
        Cut,
        Paste,
        SelectAll,
        ShowCharacterPalette,
        MoveLeft,
        MoveRight,
        MoveUp,
        MoveDown,
        WordLeft,
        WordRight,
        LineStart,
        LineEnd,
        SelectLeft,
        SelectRight,
        SelectUp,
        SelectDown,
        SelectWordLeft,
        SelectWordRight,
        SelectToStart,
        SelectToEnd,
        Complete,
        TakeSuggestion
    ]
);

pub enum ComposerEvent {
    /// Enter: the text as typed (it may be empty: the view decides what that means). The box
    /// keeps its text; the receiver clears it with `set_text`.
    Send(String),
    /// Escape.
    Stop,
    /// An `@` was typed at the start of a word.
    AtTyped,
    /// Tab while a list is open over the box: complete with the selected row.
    Complete,
    /// Paste with an image (and no text) on the clipboard: the box makes it an attachment.
    PasteImage(gpui::Image),
    /// A click on an atom's pill (a pasted image's `[Image #1]`, a path pill): its text, to
    /// open.
    OpenAtom(String),
    /// A right-click, ⌃-click or ⇧F10 (batch 3, chore 4): fuxx's menu at window point `at`;
    /// `has_sel`: something is selected in the box (Cut and Copy apply).
    ContextMenu { at: Point<Pixels>, has_sel: bool },
}

/// A character that goes on a word or a path (`src/main.rs`, `~/code`).
fn joins(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '-' | '/' | '.' | '~' | '@')
}

/// Whether `text[r]` stands on its own: no word or path character goes on from either edge
/// (where its own edge character is one). A `.` after it ends a sentence unless more follows.
fn on_word_edges(text: &str, r: Range<usize>) -> bool {
    let piece = &text[r.clone()];
    let (Some(first), Some(last)) = (piece.chars().next(), piece.chars().next_back()) else {
        return false;
    };
    let before = text[..r.start].chars().next_back();
    let mut after = text[r.end..].chars();
    let next = after.next();
    let goes_on_after = match next {
        Some('.') => after.next().is_some_and(joins),
        Some(c) => joins(c),
        None => false,
    };
    let left = !(joins(first) && before.is_some_and(joins));
    let right = !(joins(last) && goes_on_after);
    left && right
}

/// The pure editing core: text, a caret and an anchor (the selection is between them), all byte
/// offsets that always sit on char boundaries.
pub struct EditBuffer {
    text: String,
    cursor: usize,
    anchor: usize,
    /// The column ↑/↓ keep across rows shorter than it (in chars); reset by any other move.
    goal: Option<usize>,
    /// Lines wrap at this many characters (the box's width in its font's widest cell); none:
    /// a row is a whole line.
    wrap: Option<usize>,
    /// Pieces of text that edit as one (a pasted image's `[Image #1]`, drawn as a pill): the
    /// caret never stops inside one, and ⌫ / ⌦ take it whole.
    atoms: Vec<String>,
}

impl EditBuffer {
    /// `text` with the caret at its end.
    pub fn new(text: &str) -> Self {
        Self {
            text: text.to_string(),
            cursor: text.len(),
            anchor: text.len(),
            goal: None,
            wrap: None,
            atoms: Vec::new(),
        }
    }

    /// The pieces that edit as one (see `atoms`).
    pub fn set_atoms(&mut self, atoms: Vec<String>) {
        self.atoms = atoms;
    }

    /// Where the atoms are in the text, in order, none overlapping: each only on word edges
    /// (batch 3, chore 5: a path pill's `src/main.rs` is not inside `code/src/main.rs` or
    /// `src/main.rs.bak`); of two starting together, the longer.
    pub fn atom_ranges(&self) -> Vec<Range<usize>> {
        let text = self.text.as_str();
        let mut found: Vec<Range<usize>> = self
            .atoms
            .iter()
            .filter(|a| !a.is_empty())
            .flat_map(|a| {
                text.match_indices(a.as_str())
                    .map(|(i, m)| i..i + m.len())
                    .filter(|r| on_word_edges(text, r.clone()))
                    .collect::<Vec<_>>()
            })
            .collect();
        found.sort_by_key(|r| (r.start, std::cmp::Reverse(r.end)));
        let mut out: Vec<Range<usize>> = Vec::new();
        for r in found {
            if out.last().is_none_or(|l| r.start >= l.end) {
                out.push(r);
            }
        }
        out
    }

    /// The atom `i` is strictly inside, if any.
    pub fn atom_around(&self, i: usize) -> Option<Range<usize>> {
        self.atom_ranges()
            .into_iter()
            .find(|r| r.start < i && i < r.end)
    }

    /// `i` out of an atom: to its start (`left`), its end, or (`None`) the nearer edge.
    fn out_of_atom(&self, i: usize, left: Option<bool>) -> usize {
        match self.atom_around(i) {
            Some(r) => match left {
                Some(true) => r.start,
                Some(false) => r.end,
                None if i - r.start <= r.end - i => r.start,
                None => r.end,
            },
            None => i,
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// The selected bytes; empty with the caret alone.
    pub fn selection(&self) -> Range<usize> {
        self.cursor.min(self.anchor)..self.cursor.max(self.anchor)
    }

    /// The caret is at the selection's start (it was extended leftwards).
    pub fn reversed(&self) -> bool {
        self.cursor < self.anchor
    }

    /// `i` pulled back onto a char boundary inside the text.
    fn snap(&self, i: usize) -> usize {
        let mut i = i.min(self.text.len());
        while !self.text.is_char_boundary(i) {
            i -= 1;
        }
        i
    }

    /// The caret alone at `at`.
    #[cfg(test)]
    pub fn set_cursor(&mut self, at: usize) {
        self.move_to(at, false);
    }

    /// The caret to `at`: alone, or (`extend`) taking the selection with it from the anchor.
    pub fn move_to(&mut self, at: usize, extend: bool) {
        self.cursor = self.out_of_atom(self.snap(at), None);
        if !extend {
            self.anchor = self.cursor;
        }
        self.goal = None;
    }

    /// An explicit selection (the IME's), `anchor` fixed and the caret at `cursor`.
    pub fn select_range(&mut self, anchor: usize, cursor: usize) {
        self.anchor = self.snap(anchor);
        self.cursor = self.snap(cursor);
        self.goal = None;
    }

    pub fn select_all(&mut self) {
        self.anchor = 0;
        self.cursor = self.text.len();
        self.goal = None;
    }

    /// Replace everything; the caret goes to the end (the wrap stays).
    pub fn set_text(&mut self, text: &str) {
        *self = Self {
            wrap: self.wrap,
            atoms: std::mem::take(&mut self.atoms),
            ..Self::new(text)
        };
    }

    /// `s` in place of byte range `range`, the caret after it.
    pub fn replace(&mut self, range: Range<usize>, s: &str) {
        let start = self.snap(range.start);
        let end = self.snap(range.end).max(start);
        self.text.replace_range(start..end, s);
        self.move_to(start + s.len(), false);
    }

    /// `s` in place of the selection (or at the caret).
    pub fn insert(&mut self, s: &str) {
        self.replace(self.selection(), s);
    }

    pub fn newline(&mut self) {
        self.insert("\n");
    }

    /// The selection, else the character before the caret.
    pub fn backspace(&mut self) {
        let sel = self.selection();
        if sel.is_empty() {
            // An atom just before the caret goes whole.
            let from = match self
                .atom_ranges()
                .into_iter()
                .find(|r| r.end == self.cursor)
            {
                Some(r) => r.start,
                None => prev_char(&self.text, self.cursor),
            };
            self.replace(from..self.cursor, "");
        } else {
            self.replace(sel, "");
        }
    }

    /// ⌦: the selection, else the character after the caret.
    pub fn delete(&mut self) {
        let sel = self.selection();
        if sel.is_empty() {
            let to = match self
                .atom_ranges()
                .into_iter()
                .find(|r| r.start == self.cursor)
            {
                Some(r) => r.end,
                None => next_char(&self.text, self.cursor),
            };
            self.replace(self.cursor..to, "");
        } else {
            self.replace(sel, "");
        }
    }

    /// ⌘⌫: the selection, else back to the start of the line (or the newline before, at it).
    pub fn delete_to_line_start(&mut self) {
        let sel = self.selection();
        if !sel.is_empty() {
            return self.replace(sel, "");
        }
        let start = self.line_start_of(self.cursor);
        let from = if start == self.cursor {
            prev_char(&self.text, start)
        } else {
            start
        };
        self.replace(from..self.cursor, "");
    }

    /// ⌥⌫ / ⌃W: the selection, else back over the spaces and the word before the caret (a line
    /// break counts as a space, as in zsh).
    pub fn delete_word_left(&mut self) {
        let sel = self.selection();
        if !sel.is_empty() {
            return self.replace(sel, "");
        }
        self.replace(word_start(&self.text, self.cursor)..self.cursor, "");
    }

    /// ⌃K: the selection, else on to the end of the line (the newline after, at it).
    pub fn delete_to_line_end(&mut self) {
        let sel = self.selection();
        if !sel.is_empty() {
            return self.replace(sel, "");
        }
        let end = self.line_end_of(self.cursor);
        let to = if end == self.cursor {
            next_char(&self.text, end)
        } else {
            end
        };
        self.replace(self.cursor..to, "");
    }

    /// A horizontal move: without ⇧ a selection collapses to its edge on that side instead.
    fn step(&mut self, to: fn(&str, usize) -> usize, left: bool, extend: bool) {
        let sel = self.selection();
        let at = if !extend && !sel.is_empty() {
            if left { sel.start } else { sel.end }
        } else {
            // Over an atom in one step, the way it went.
            self.out_of_atom(to(&self.text, self.cursor), Some(left))
        };
        self.move_to(at, extend);
    }

    pub fn left(&mut self, extend: bool) {
        self.step(prev_char, true, extend);
    }
    pub fn right(&mut self, extend: bool) {
        self.step(next_char, false, extend);
    }
    pub fn word_left(&mut self, extend: bool) {
        self.step(word_start, true, extend);
    }
    pub fn word_right(&mut self, extend: bool) {
        self.step(word_end, false, extend);
    }
    /// ⌘←: to the start of the line (the line the selection starts on, a selection
    /// collapsing; ⇧ extends from the caret).
    pub fn line_start(&mut self, extend: bool) {
        let from = if extend {
            self.cursor
        } else {
            self.selection().start
        };
        self.move_to(self.line_start_of(from), extend);
    }
    /// ⌘→: to the end of the line (the one the selection ends on).
    pub fn line_end(&mut self, extend: bool) {
        let from = if extend {
            self.cursor
        } else {
            self.selection().end
        };
        self.move_to(self.line_end_of(from), extend);
    }

    /// One row up, to the same column (the wider one remembered over short rows); on the first
    /// row, to the start.
    pub fn up(&mut self) {
        self.vertical(true, false);
    }
    pub fn down(&mut self) {
        self.vertical(false, false);
    }
    pub fn select_up(&mut self) {
        self.vertical(true, true);
    }
    pub fn select_down(&mut self) {
        self.vertical(false, true);
    }

    /// One visual row up or down (a wrapped line is several), to the goal column.
    fn vertical(&mut self, up: bool, extend: bool) {
        let rows = self.visual_rows();
        let r = row_at(&rows, self.cursor);
        let goal = self
            .goal
            .unwrap_or_else(|| self.text[rows[r].start..self.cursor].chars().count());
        let at = match (up, r) {
            (true, 0) => 0,
            (true, r) => self.at_column(&rows[r - 1], goal),
            (false, r) if r + 1 >= rows.len() => self.text.len(),
            (false, r) => self.at_column(&rows[r + 1], goal),
        };
        self.move_to(at, extend);
        self.goal = Some(goal);
    }

    /// The byte `column` chars into visual row `row` (its end if it is shorter; a row that
    /// wraps on keeps the caret before its last character, so the caret stays on it).
    fn at_column(&self, row: &Range<usize>, column: usize) -> usize {
        let at = self.text[row.clone()]
            .char_indices()
            .nth(column)
            .map_or(row.end, |(i, _)| row.start + i);
        let wraps_on = row.end < self.text.len() && !self.text[row.end..].starts_with('\n');
        if wraps_on && at == row.end && row.end > row.start {
            prev_char(&self.text, row.end)
        } else {
            at
        }
    }

    fn line_start_of(&self, i: usize) -> usize {
        self.text[..i].rfind('\n').map_or(0, |j| j + 1)
    }

    fn line_end_of(&self, i: usize) -> usize {
        self.text[i..].find('\n').map_or(self.text.len(), |j| i + j)
    }

    /// Wrap lines at `columns` characters (none, or 0: no wrapping).
    pub fn set_wrap(&mut self, columns: Option<usize>) {
        self.wrap = columns.filter(|c| *c > 0);
    }

    /// Each visual row's byte range: the lines, each wrapped at the wrap width after its last
    /// space that fits (a word longer than the row breaks inside it). A row that wraps keeps
    /// its trailing space.
    pub fn visual_rows(&self) -> Vec<Range<usize>> {
        let lines = self.row_ranges();
        let Some(cols) = self.wrap else {
            return lines;
        };
        let mut out = Vec::new();
        for line in lines {
            let mut start = line.start;
            loop {
                let rest = &self.text[start..line.end];
                let Some((cut, at_cut)) = rest.char_indices().nth(cols) else {
                    out.push(start..line.end);
                    break;
                };
                let brk = if at_cut.is_whitespace() {
                    cut + at_cut.len_utf8()
                } else {
                    rest[..cut]
                        .char_indices()
                        .rfind(|(_, c)| c.is_whitespace())
                        .map_or(cut, |(i, c)| i + c.len_utf8())
                };
                out.push(start..start + brk);
                start += brk;
            }
        }
        out
    }

    /// The caret is on the first visual row (an empty text has only that one).
    pub fn on_first_row(&self) -> bool {
        self.visual_row_of(self.cursor) == 0
    }

    /// The caret is on the last visual row.
    pub fn on_last_row(&self) -> bool {
        self.visual_row_of(self.cursor) + 1 >= self.visual_row_count()
    }

    pub fn visual_row_count(&self) -> usize {
        self.visual_rows().len()
    }

    /// The visual row byte `i` is on (on a wrap, the row it starts).
    pub fn visual_row_of(&self, i: usize) -> usize {
        row_at(&self.visual_rows(), self.snap(i))
    }

    /// How many lines the text has (an empty text is one).
    #[cfg(test)]
    pub fn rows(&self) -> usize {
        self.text.matches('\n').count() + 1
    }

    /// The line byte `i` is on.
    #[cfg(test)]
    pub fn row_of(&self, i: usize) -> usize {
        self.text[..self.snap(i)].matches('\n').count()
    }

    /// Each row's byte range (without its newline).
    pub fn row_ranges(&self) -> Vec<Range<usize>> {
        let mut at = 0;
        self.text
            .split('\n')
            .map(|line| {
                let range = at..at + line.len();
                at = range.end + 1;
                range
            })
            .collect()
    }

    /// Byte `i` is an `@` at the start of a word (the text start, or after whitespace).
    pub fn at_word_start(&self, i: usize) -> bool {
        self.text.get(i..).is_some_and(|rest| rest.starts_with('@'))
            && self.text[..i]
                .chars()
                .next_back()
                .is_none_or(char::is_whitespace)
    }
}

/// The row of `rows` (in order, covering the text) byte `i` is on: the last one starting at or
/// before it.
fn row_at(rows: &[Range<usize>], i: usize) -> usize {
    rows.iter().rposition(|r| r.start <= i).unwrap_or(0)
}

/// Where row `row` sits in a box showing `shown` rows from `first`: None above or below it.
fn shown_row(row: usize, first: usize, shown: usize) -> Option<usize> {
    let n = row.checked_sub(first)?;
    (n < shown).then_some(n)
}

/// Typed or pasted text as the box keeps it: newlines are `\n`, other control characters go.
fn clean(text: &str) -> String {
    text.replace("\r\n", "\n")
        .replace('\r', "\n")
        .chars()
        .filter(|c| *c == '\n' || !c.is_control())
        .collect()
}

pub struct Composer {
    focus_handle: FocusHandle,
    buf: EditBuffer,
    placeholder: SharedString,
    /// The agent's guess at your next message, drawn faint in place of the placeholder while
    /// the box is empty, "⇥" after it; Tab takes it (`TakeSuggestion`). Typing hides it,
    /// emptying the box shows it again; only its owner clears it (`set_ghost`).
    ghost: Option<SharedString>,
    /// The IME's composition in progress, as a byte range of the text.
    marked: Option<Range<usize>>,
    /// A mouse drag is selecting.
    dragging: bool,
    /// The first row shown (the box scrolls to keep the caret in view).
    first_row: usize,
    /// The caret's row is kept in view; off after a wheel scroll until the caret or the text
    /// next changes.
    follow_caret: bool,
    /// The part of a wheel scroll short of a whole row, carried to the next.
    wheel_rest: Pixels,
    /// From the last paint, for the IME's candidate window and the mouse: the rows shown
    /// (from `first_row`), shaped, where the text's left edge and the box's top were, and the
    /// row height.
    last_lines: Vec<ShapedLine>,
    last_origin: Option<Point<Pixels>>,
    last_bounds: Option<Bounds<Pixels>>,
    last_line_height: Pixels,
    /// Drawn with no edge, fill or padding of its own: a box around it draws them (the agent
    /// box, M11ak).
    bare: bool,
    /// From the last paint: the cell's width and height, the caret's shape and box, and the
    /// placeholder as drawn and where (scenes read them).
    last_cell: (Pixels, Pixels),
    last_caret: Option<(PaintShape, Bounds<Pixels>)>,
    last_placeholder: Option<(String, Bounds<Pixels>)>,
    /// The ghost as last drawn (without its "⇥"), and where.
    last_ghost: Option<(String, Bounds<Pixels>)>,
    /// From the last paint: the atoms' pills as drawn (scenes read them), and the atom whose
    /// pill the pointer is on (its fill deepens; a move that changes it redraws).
    last_pills: Vec<Bounds<Pixels>>,
    hovered_atom: Option<usize>,
    /// Scenes' stand-in for "the window is the key window" (none: ask the window).
    key_window: Option<bool>,
    /// The caret is the terminal's cursor (the designer; your words: "it doesn't glow and smooth
    /// like terminal tab"): its glide, trail, sparks and breath, as a pane's.
    motion: CursorMotion,
    /// The breathing timer runs.
    breathing: bool,
    /// A changed cursor setting redraws the box.
    _appearance: Subscription,
}

impl EventEmitter<ComposerEvent> for Composer {}

impl Focusable for Composer {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

/// A caret action: `$f` on the buffer, the IME's composition (if any) dropped.
macro_rules! steps {
    ($($name:ident: $action:ident => $f:expr;)*) => {
        $(fn $name(&mut self, _: &$action, _: &mut Window, cx: &mut Context<Self>) {
            self.step($f, cx);
        })*
    };
}

impl Composer {
    pub fn new(placeholder: SharedString, _window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            buf: EditBuffer::new(""),
            placeholder,
            ghost: None,
            marked: None,
            dragging: false,
            first_row: 0,
            follow_caret: true,
            wheel_rest: px(0.),
            last_lines: Vec::new(),
            last_origin: None,
            last_bounds: None,
            last_line_height: px(0.),
            last_pills: Vec::new(),
            hovered_atom: None,
            bare: false,
            last_cell: (px(0.), px(0.)),
            last_caret: None,
            last_placeholder: None,
            last_ghost: None,
            key_window: None,
            motion: CursorMotion::new(Instant::now()),
            breathing: false,
            _appearance: cx.observe_global::<Appearance>(|_, cx| cx.notify()),
        }
    }

    /// Draw it `bare` (no edge, fill or padding: the box around it draws them) or framed.
    pub fn set_bare(&mut self, bare: bool, cx: &mut Context<Self>) {
        if self.bare != bare {
            self.bare = bare;
            cx.notify();
        }
    }

    pub fn text(&self) -> &str {
        self.buf.text()
    }

    pub fn set_text(&mut self, text: String, cx: &mut Context<Self>) {
        self.buf.set_text(&text);
        self.marked = None;
        self.follow_caret = true;
        cx.notify();
    }

    /// `text` at the caret (in place of the selection).
    pub fn insert(&mut self, text: &str, cx: &mut Context<Self>) {
        self.buf.insert(&clean(text));
        self.marked = None;
        self.follow_caret = true;
        cx.notify();
    }

    /// The caret's byte offset.
    /// The pieces that edit as one and draw as pills (the tab's pasted images' `[Image #N]`).
    pub fn set_atoms(&mut self, atoms: Vec<String>, cx: &mut Context<Self>) {
        self.buf.set_atoms(atoms);
        cx.notify();
    }

    pub fn cursor(&self) -> usize {
        self.buf.cursor()
    }

    /// `text` in place of byte range `range`, the caret after it.
    pub fn replace(&mut self, range: Range<usize>, text: &str, cx: &mut Context<Self>) {
        self.buf.replace(range, &clean(text));
        self.marked = None;
        self.follow_caret = true;
        cx.notify();
    }

    /// Whether its window is the key window, as the box draws it (scenes can say so).
    pub fn is_key_window(&self, window: &Window) -> bool {
        self.key_window.unwrap_or_else(|| window.is_window_active())
    }

    #[cfg(feature = "ui-shots")]
    pub fn placeholder(&self) -> &SharedString {
        &self.placeholder
    }

    /// The rows as wrapped, the first shown, and where the box was drawn.
    #[cfg(feature = "ui-shots")]
    pub fn shot_rows(&self) -> (usize, usize, Option<Bounds<Pixels>>) {
        (
            self.buf.visual_row_count(),
            self.first_row,
            self.last_bounds,
        )
    }

    /// Typed text sends sparks off the caret, as in a pane (Fun = Sparks, motion on).
    fn spark(&mut self, typed: &str, cx: &App) {
        let a = cx.global::<Appearance>();
        if a.cursor_fun != CursorFun::Sparks
            || !a.animations
            || cx.reduce_motion()
            || typed.is_empty()
            || typed.chars().any(char::is_control)
        {
            return;
        }
        let (cw, lh) = self.last_cell;
        self.motion
            .spark(f32::from(cw), f32::from(lh), Instant::now());
    }

    /// The breathing timer, as a pane's (`TerminalView::breathe`): it sleeps through the hold,
    /// then redraws at the breath's frames while the last paint asked for them and the box has
    /// focus in the key window; it stops when any of that ends or the breath rests.
    fn breathe(&mut self, window: &Window, cx: &mut Context<Self>) {
        if self.breathing {
            return;
        }
        self.breathing = true;
        let mut wait = breath_sleep(self.motion.changed.elapsed());
        cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor().timer(wait).await;
                let next = this
                    .update_in(cx, |this, window, cx| {
                        let key = this.key_window.unwrap_or_else(|| window.is_window_active());
                        let go = this.motion.frames == Frames::Breath
                            && this.focus_handle.is_focused(window)
                            && key;
                        let since = this.motion.changed.elapsed();
                        if !go {
                            this.breathing = false;
                            return None;
                        }
                        if breath_redraw(since) {
                            cx.notify();
                            if cursor_look::breath_rests(since - cursor_look::HOLD) {
                                this.breathing = false;
                                return None;
                            }
                        }
                        Some(breath_sleep(since))
                    })
                    .ok()
                    .flatten();
                match next {
                    Some(next) => wait = next,
                    None => break,
                }
            }
        })
        .detach();
    }

    /// The redraws the caret's last paint asked for.
    #[cfg(feature = "ui-shots")]
    pub fn shot_frames(&self) -> Frames {
        self.motion.frames
    }

    /// The cell's width and height as last drawn.
    #[cfg(feature = "ui-shots")]
    pub fn shot_cell(&self) -> (Pixels, Pixels) {
        self.last_cell
    }

    /// The caret's shape and box as last drawn (none: not drawn).
    /// The atoms' pills as last drawn, in window coordinates.
    #[cfg(feature = "ui-shots")]
    pub fn shot_pills(&self) -> Vec<Bounds<Pixels>> {
        self.last_pills.clone()
    }

    #[cfg(feature = "ui-shots")]
    pub fn shot_caret(&self) -> Option<(PaintShape, Bounds<Pixels>)> {
        self.last_caret
    }

    /// The placeholder as last drawn, and where (none: not drawn).
    #[cfg(feature = "ui-shots")]
    pub fn shot_placeholder(&self) -> Option<(String, Bounds<Pixels>)> {
        self.last_placeholder.clone()
    }

    /// Draw as if the window were the key window, or not.
    #[cfg(feature = "ui-shots")]
    pub fn shot_be_key(&mut self) {
        self.key_window = Some(true);
    }
    #[cfg(feature = "ui-shots")]
    pub fn shot_not_key(&mut self) {
        self.key_window = Some(false);
    }

    /// The ghost as last drawn (none: not drawn).
    #[cfg(feature = "ui-shots")]
    pub fn shot_ghost(&self) -> Option<(String, Bounds<Pixels>)> {
        self.last_ghost.clone()
    }

    /// A new ghost, or none (redrawn only when it changes).
    pub fn set_ghost(&mut self, ghost: Option<SharedString>, cx: &mut Context<Self>) {
        if self.ghost != ghost {
            self.ghost = ghost;
            cx.notify();
        }
    }

    /// The caret is on the first visual row, or the last (prompt history's ↑ and ↓).
    pub fn on_first_row(&self) -> bool {
        self.buf.on_first_row()
    }

    pub fn on_last_row(&self) -> bool {
        self.buf.on_last_row()
    }

    pub fn has_selection(&self) -> bool {
        !self.buf.selection().is_empty()
    }

    /// The IME is composing text (its marked text is in the box).
    pub fn is_composing(&self) -> bool {
        self.marked.is_some()
    }

    /// One visual row up or down, as ↑ and ↓ do.
    pub fn move_up(&mut self, cx: &mut Context<Self>) {
        self.step(EditBuffer::up, cx);
    }

    pub fn move_down(&mut self, cx: &mut Context<Self>) {
        self.step(EditBuffer::down, cx);
    }

    /// Tab: the ghost fills the empty box, the caret at its end. With no ghost or text in the
    /// box, Tab goes on to its other bindings (the `@` and `/` lists complete).
    fn take_suggestion(&mut self, _: &TakeSuggestion, _: &mut Window, cx: &mut Context<Self>) {
        match self.ghost.clone().filter(|_| self.buf.text().is_empty()) {
            Some(ghost) => self.set_text(ghost.to_string(), cx),
            None => cx.propagate(),
        }
    }

    /// A new placeholder (redrawn only when it is new).
    pub fn set_placeholder(&mut self, p: SharedString, cx: &mut Context<Self>) {
        if self.placeholder != p {
            self.placeholder = p;
            cx.notify();
        }
    }

    fn step(&mut self, f: impl FnOnce(&mut EditBuffer), cx: &mut Context<Self>) {
        self.marked = None;
        f(&mut self.buf);
        self.motion.changed = Instant::now();
        self.follow_caret = true;
        cx.notify();
    }

    /// The wheel scrolls the rows (a whole row at a time; the rest carries over), leaving the
    /// caret where it is.
    fn scroll(&mut self, e: &ScrollWheelEvent, _: &mut Window, cx: &mut Context<Self>) {
        let height = self.last_line_height;
        if height <= px(0.) {
            return;
        }
        let rows = self.buf.visual_row_count();
        let shown = rows.min(MAX_ROWS);
        if rows <= shown {
            return;
        }
        cx.stop_propagation();
        self.wheel_rest += e.delta.pixel_delta(height).y;
        let whole = (self.wheel_rest / height).trunc();
        self.wheel_rest -= height * whole;
        let first = (self.first_row as f32 - whole).clamp(0., (rows - shown) as f32) as usize;
        if first != self.first_row {
            self.first_row = first;
            self.follow_caret = false;
            cx.notify();
        }
    }

    steps! {
        backspace: Backspace => |b| b.backspace();
        delete: Delete => |b| b.delete();
        clear: Clear => |b| b.delete_to_line_start();
        delete_word_left: DeleteWordLeft => |b| b.delete_word_left();
        delete_to_line_end: DeleteToLineEnd => |b| b.delete_to_line_end();
        newline: Newline => |b| b.newline();
        move_left: MoveLeft => |b| b.left(false);
        move_right: MoveRight => |b| b.right(false);
        up_pressed: MoveUp => |b| b.up();
        down_pressed: MoveDown => |b| b.down();
        word_left: WordLeft => |b| b.word_left(false);
        word_right: WordRight => |b| b.word_right(false);
        line_start: LineStart => |b| b.line_start(false);
        line_end: LineEnd => |b| b.line_end(false);
        select_left: SelectLeft => |b| b.left(true);
        select_right: SelectRight => |b| b.right(true);
        select_up: SelectUp => |b| b.select_up();
        select_down: SelectDown => |b| b.select_down();
        select_word_left: SelectWordLeft => |b| b.word_left(true);
        select_word_right: SelectWordRight => |b| b.word_right(true);
        select_to_start: SelectToStart => |b| b.line_start(true);
        select_to_end: SelectToEnd => |b| b.line_end(true);
    }

    fn send(&mut self, _: &Send, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(ComposerEvent::Send(self.buf.text().to_string()));
    }

    fn stop(&mut self, _: &Stop, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(ComposerEvent::Stop);
    }

    fn complete(&mut self, _: &Complete, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(ComposerEvent::Complete);
    }

    fn copy_selection(&self, cx: &mut Context<Self>) {
        let sel = self.buf.selection();
        if !sel.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(self.buf.text()[sel].to_string()));
        }
    }

    /// ⌘C: the box's selection; with none, the chat's (the agent tab takes it on).
    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if self.buf.selection().is_empty() {
            cx.propagate();
            return;
        }
        self.copy_selection(cx);
    }

    /// ⌘A: all you typed; with nothing typed, the whole chat (the agent tab takes it on).
    fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        if self.buf.text().is_empty() {
            cx.propagate();
            return;
        }
        self.step(|b| b.select_all(), cx);
    }

    /// The text selected in the box, if any.
    pub fn selected_text(&self) -> Option<String> {
        let sel = self.buf.selection();
        (!sel.is_empty()).then(|| self.buf.text()[sel].to_string())
    }

    /// Drop the box's selection, the caret where it is (a selection began in the chat).
    pub fn collapse_selection(&mut self, cx: &mut Context<Self>) {
        if !self.buf.selection().is_empty() {
            let at = self.buf.cursor();
            self.buf.move_to(at, false);
            cx.notify();
        }
    }

    fn cut(&mut self, _: &Cut, _: &mut Window, cx: &mut Context<Self>) {
        self.cut_selection(cx);
    }

    fn cut_selection(&mut self, cx: &mut Context<Self>) {
        self.copy_selection(cx);
        if !self.buf.selection().is_empty() {
            self.step(|b| b.backspace(), cx);
        }
    }

    /// The right-click menu's items (batch 3, chore 4), as their keys do in the box.
    pub fn menu_cut(&mut self, cx: &mut Context<Self>) {
        self.cut_selection(cx);
    }

    pub fn menu_copy(&mut self, cx: &mut Context<Self>) {
        self.copy_selection(cx);
    }

    pub fn menu_paste(&mut self, cx: &mut Context<Self>) {
        self.paste_clipboard(cx);
    }

    pub fn menu_select_all(&mut self, cx: &mut Context<Self>) {
        self.step(|b| b.select_all(), cx);
    }

    /// Ask for fuxx's menu at `at` (the selection stays).
    fn open_menu(&mut self, at: Point<Pixels>, cx: &mut Context<Self>) {
        cx.emit(ComposerEvent::ContextMenu {
            at,
            has_sel: self.has_selection(),
        });
    }

    /// ⇧F10: the menu under the caret as last drawn (else near the box's top-left).
    fn show_menu(
        &mut self,
        _: &crate::workspace_menu::ShowMenu,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let at = match (self.last_caret, self.last_bounds) {
            (Some((_, caret)), _) => caret.bottom_left() + point(px(0.), px(4.)),
            (None, bounds) => crate::workspace_menu::keyboard_at(bounds),
        };
        self.open_menu(at, cx);
    }

    /// A right mouse-down (or ⌃-click): the keyboard comes to the box, the menu opens.
    fn right_down(&mut self, e: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        window.focus(&self.focus_handle, cx);
        self.open_menu(e.position, cx);
    }

    fn paste(&mut self, _: &Paste, _: &mut Window, cx: &mut Context<Self>) {
        self.paste_clipboard(cx);
    }

    fn paste_clipboard(&mut self, cx: &mut Context<Self>) {
        let Some(item) = cx.read_from_clipboard() else {
            return;
        };
        if let Some(text) = item.text() {
            self.insert(&text, cx);
        } else if let Some(image) = item.into_entries().find_map(|entry| match entry {
            gpui::ClipboardEntry::Image(image) => Some(image),
            _ => None,
        }) {
            cx.emit(ComposerEvent::PasteImage(image));
        }
    }

    fn show_character_palette(
        &mut self,
        _: &ShowCharacterPalette,
        window: &mut Window,
        _: &mut Context<Self>,
    ) {
        window.show_character_palette();
    }

    /// The byte under window point `at` on the rows as last painted (the nearest boundary).
    fn index_at(&self, at: Point<Pixels>) -> Option<usize> {
        let (origin, bounds) = (self.last_origin?, self.last_bounds?);
        if self.last_line_height <= px(0.) {
            return None;
        }
        let below = ((at.y - bounds.top()) / self.last_line_height)
            .floor()
            .max(0.) as usize;
        let below = below.min(self.last_lines.len().checked_sub(1)?);
        let range = self.buf.visual_rows().get(self.first_row + below)?.clone();
        let x = at.x - origin.x;
        let col = self.last_lines[below]
            .closest_index_for_x(x)
            .min(range.len());
        Some(range.start + col)
    }

    /// A click puts the caret there (⇧: selects to there); a double-click selects it all; a
    /// drag selects.
    fn mouse_down(&mut self, e: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        // ⌃-click is a right-click on a Mac.
        if e.modifiers.control {
            self.right_down(e, window, cx);
            return;
        }
        cx.stop_propagation();
        window.focus(&self.focus_handle, cx);
        self.marked = None;
        // A click on a pill opens what it stands for (a pasted image), the caret staying put.
        if e.click_count == 1
            && !e.modifiers.shift
            && let Some(at) = self.index_at(e.position)
            && let Some(atom) = self.buf.atom_around(at)
        {
            let name = self.buf.text()[atom].to_string();
            cx.emit(ComposerEvent::OpenAtom(name));
            cx.notify();
            return;
        }
        if e.click_count >= 2 {
            self.buf.select_all();
        } else if let Some(at) = self.index_at(e.position) {
            self.buf.move_to(at, e.modifiers.shift);
            self.dragging = true;
        }
        cx.notify();
    }

    fn mouse_move(&mut self, e: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if !self.dragging || e.pressed_button != Some(MouseButton::Left) {
            self.dragging = false;
            return;
        }
        if let Some(at) = self.index_at(e.position) {
            self.buf.move_to(at, true);
            cx.notify();
        }
    }

    fn mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.dragging = false;
    }

    fn range_from_utf16(&self, range: &Range<usize>) -> Range<usize> {
        let text = self.buf.text();
        let start = utf8_offset(text, range.start);
        start..utf8_offset(text, range.end).max(start)
    }

    fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        let text = self.buf.text();
        utf16_offset(text, range.start)..utf16_offset(text, range.end)
    }

    /// The byte range an IME edit replaces: the one given, else the marked text, else the
    /// selection.
    fn edit_range(&self, range_utf16: Option<Range<usize>>) -> Range<usize> {
        range_utf16
            .map(|r| self.range_from_utf16(&r))
            .or(self.marked.clone())
            .unwrap_or_else(|| self.buf.selection())
    }

    /// Where byte `index` was painted: the left of its glyph and the top of its row, when that
    /// row is on screen.
    fn point_for(&self, index: usize) -> Option<Point<Pixels>> {
        let origin = self.last_origin?;
        let bounds = self.last_bounds?;
        let row = self.buf.visual_row_of(index);
        let shown = shown_row(row, self.first_row, self.last_lines.len())?;
        let line = self.last_lines.get(shown)?;
        let start = self.buf.visual_rows().get(row)?.start;
        Some(point(
            origin.x + line.x_for_index(index - start),
            bounds.top() + self.last_line_height * shown as f32,
        ))
    }
}

impl EntityInputHandler for Composer {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range_utf16);
        actual_range.replace(self.range_to_utf16(&range));
        Some(self.buf.text()[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.range_to_utf16(&self.buf.selection()),
            reversed: self.buf.reversed(),
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked.as_ref().map(|r| self.range_to_utf16(r))
    }

    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        // The composition stays as typed; the caret after it.
        if let Some(marked) = self.marked.take() {
            self.buf.move_to(marked.end, false);
        }
        cx.notify();
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = self.edit_range(range_utf16);
        let start = range.start;
        let new_text = clean(new_text);
        self.buf.replace(range, &new_text);
        self.marked = None;
        self.follow_caret = true;
        self.motion.changed = Instant::now();
        self.spark(&new_text, cx);
        if new_text == "@" && self.buf.at_word_start(start) {
            cx.emit(ComposerEvent::AtTyped);
        }
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = self.edit_range(range_utf16);
        let new_text = clean(new_text);
        let start = range.start;
        self.buf.replace(range, &new_text);
        self.follow_caret = true;
        let end = start + new_text.len();
        self.marked = (!new_text.is_empty()).then_some(start..end);
        // The IME's caret is relative to the marked text, in UTF-16.
        match new_selected_range_utf16 {
            Some(r) => self.buf.select_range(
                start + utf8_offset(&new_text, r.start),
                start + utf8_offset(&new_text, r.end),
            ),
            None => self.buf.select_range(end, end),
        }
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        _: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let range = self.range_from_utf16(&range_utf16);
        let from = self.point_for(range.start)?;
        // The end counts only on the same row; across rows the candidate window sits at the start.
        let to_x = match self.point_for(range.end) {
            Some(to) if to.y == from.y => to.x,
            _ => from.x,
        };
        Some(Bounds::from_corners(
            from,
            point(to_x, from.y + self.last_line_height),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        self.last_bounds?.localize(&point)?;
        let index = self.index_at(point)?;
        Some(utf16_offset(self.buf.text(), index))
    }
}

impl Render for Composer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ui = cx.global::<Appearance>().theme.ui();
        // Bare, the box around it draws the edge.
        let edge = (!self.bare).then(|| {
            if self.focus_handle.is_focused(window) {
                ui.focus_ring
            } else {
                ui.text_dim
            }
        });
        div()
            .w_full()
            .overflow_hidden()
            .when_some(edge, |d, edge| {
                d.px_2()
                    .py_1()
                    .rounded_sm()
                    .border_1()
                    .border_color(hsla(edge))
                    .bg(hsla(ui.sidebar_background))
            })
            .text_color(hsla(ui.text))
            .key_context("Composer")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::send))
            .on_action(cx.listener(Self::newline))
            .on_action(cx.listener(Self::stop))
            .on_action(cx.listener(Self::complete))
            .on_action(cx.listener(Self::take_suggestion))
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::clear))
            .on_action(cx.listener(Self::delete_word_left))
            .on_action(cx.listener(Self::delete_to_line_end))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::show_character_palette))
            .on_action(cx.listener(Self::show_menu))
            .on_action(cx.listener(Self::move_left))
            .on_action(cx.listener(Self::move_right))
            .on_action(cx.listener(Self::up_pressed))
            .on_action(cx.listener(Self::down_pressed))
            .on_action(cx.listener(Self::word_left))
            .on_action(cx.listener(Self::word_right))
            .on_action(cx.listener(Self::line_start))
            .on_action(cx.listener(Self::line_end))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::select_up))
            .on_action(cx.listener(Self::select_down))
            .on_action(cx.listener(Self::select_word_left))
            .on_action(cx.listener(Self::select_word_right))
            .on_action(cx.listener(Self::select_to_start))
            .on_action(cx.listener(Self::select_to_end))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::mouse_down))
            .on_mouse_down(MouseButton::Right, cx.listener(Self::right_down))
            .on_mouse_move(cx.listener(Self::mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::mouse_up))
            .on_scroll_wheel(cx.listener(Self::scroll))
            .child(ComposerText { input: cx.entity() })
    }
}

/// The atoms among `atoms` written in brackets (`[Image #1]`): only theirs are drawn clear; a
/// path pill (batch 3, chore 5) shows every character.
fn bracketed(text: &str, atoms: &[Range<usize>]) -> Vec<Range<usize>> {
    atoms
        .iter()
        .filter(|r| {
            text.get((*r).clone())
                .is_some_and(|t| t.len() >= 2 && t.starts_with('[') && t.ends_with(']'))
        })
        .cloned()
        .collect()
}

/// `runs` (of the row `row`) with each atom's first and last character (its brackets) drawn
/// clear, so the atom reads as its name on a pill.
fn hide_brackets(runs: Vec<TextRun>, row: &Range<usize>, atoms: &[Range<usize>]) -> Vec<TextRun> {
    // The row-relative bytes to hide: each bracket is one byte (`[` / `]`).
    let hidden: Vec<usize> = atoms
        .iter()
        .flat_map(|a| [a.start, a.end - 1])
        .filter(|&i| row.start <= i && i < row.end)
        .map(|i| i - row.start)
        .collect();
    if hidden.is_empty() {
        return runs;
    }
    let mut out = Vec::new();
    let mut at = 0;
    for run in runs {
        let end = at + run.len;
        let mut from = at;
        let mut cuts: Vec<usize> = hidden
            .iter()
            .flat_map(|&h| [h, h + 1])
            .filter(|&c| at < c && c < end)
            .collect();
        cuts.sort_unstable();
        cuts.dedup();
        cuts.push(end);
        for to in cuts {
            let clear = hidden.contains(&from);
            out.push(TextRun {
                len: to - from,
                color: if clear {
                    gpui::Hsla { a: 0., ..run.color }
                } else {
                    run.color
                },
                ..run.clone()
            });
            from = to;
        }
        at = end;
    }
    out.retain(|r| r.len > 0);
    out
}

/// The marked text's underline, as macOS expects, cut to the row `range`.
fn runs_for(range: &Range<usize>, marked: Option<&Range<usize>>, base: &TextRun) -> Vec<TextRun> {
    let len = range.len();
    let with = |len| TextRun {
        len,
        ..base.clone()
    };
    let Some(m) = marked else {
        return vec![with(len)];
    };
    let from = m.start.clamp(range.start, range.end) - range.start;
    let to = m.end.clamp(range.start, range.end) - range.start;
    // An empty row still shapes, with one empty run.
    let runs: Vec<TextRun> = [
        with(from),
        TextRun {
            underline: Some(UnderlineStyle {
                color: Some(base.color),
                thickness: px(1.0),
                wavy: false,
            }),
            ..with(to - from)
        },
        with(len - to),
    ]
    .into_iter()
    .filter(|r| r.len > 0)
    .collect();
    if runs.is_empty() { vec![with(0)] } else { runs }
}

/// An atom's pill: its fill, mixed this far from the box's toward the text; more under the
/// pointer.
const PILL: f32 = 0.12;
const PILL_HOVER: f32 = 0.20;

/// Paints the rows and caret, and registers the box as the window's text input while focused.
struct ComposerText {
    input: Entity<Composer>,
}

impl IntoElement for ComposerText {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

/// What prepaint hands paint: the visual rows, the first shown, and the shown rows shaped.
pub struct Shaped {
    rows: Vec<Range<usize>>,
    first: usize,
    lines: Vec<ShapedLine>,
    /// The text's left edge (a caret row wider than the box scrolls left).
    left: Pixels,
    /// The shown atoms' pills: which atom, where, and its hitbox (the pointing hand).
    pills: Vec<(usize, Bounds<Pixels>, Hitbox)>,
}

impl Element for ComposerText {
    type RequestLayoutState = ();
    type PrepaintState = Shaped;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    /// As tall as the wrapped text (at most `MAX_ROWS` rows): the width comes from layout, and
    /// sets the wrap (in cells of the font's `m`, its widest letter).
    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        // The terminal's font, size and cell (the designer: the agent box types as the shell's).
        let m = CellMetrics::measure(cx.global::<Appearance>(), window);
        let (cell, line_height) = (m.cell_width, m.line_height);
        let input = self.input.clone();
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        let id = window.request_measured_layout(style, move |known, available, _, cx| {
            let width = known.width.or(match available.width {
                AvailableSpace::Definite(w) => Some(w),
                _ => None,
            });
            let rows = input.update(cx, |input, _| {
                if let Some(width) = width {
                    input.buf.set_wrap(Some(wrap_columns(width, cell)));
                }
                input.buf.visual_row_count()
            });
            size(
                width.unwrap_or(px(0.)),
                line_height * rows.min(MAX_ROWS) as f32,
            )
        });
        (id, ())
    }

    /// The first row shown (the caret's row kept in view unless the wheel moved it), and only
    /// the shown rows shaped.
    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let input = self.input.read(cx);
        let rows = input.buf.visual_rows();
        let shown = rows.len().min(MAX_ROWS);
        let caret_row = row_at(&rows, input.buf.cursor());
        let mut first = input.first_row.min(rows.len() - shown);
        if input.follow_caret {
            if caret_row < first {
                first = caret_row;
            } else if caret_row >= first + shown {
                first = caret_row + 1 - shown;
            }
        }
        let m = CellMetrics::measure(cx.global::<Appearance>(), window);
        let base = TextRun {
            len: 0,
            font: m.font.clone(),
            color: window.text_style().color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let font_size = m.font_size;
        let text = input.buf.text();
        let atoms = input.buf.atom_ranges();
        let hidden = bracketed(text, &atoms);
        let lines = rows[first..first + shown]
            .iter()
            .map(|range| {
                let runs = hide_brackets(
                    runs_for(range, input.marked.as_ref(), &base),
                    range,
                    &hidden,
                );
                window.text_system().shape_line(
                    text[range.clone()].to_string().into(),
                    font_size,
                    &runs,
                    None,
                )
            })
            .collect::<Vec<ShapedLine>>();
        // A caret row wider than the box (wide characters) scrolls left so the caret shows.
        let caret_width = m.cell_width;
        let left = match shown_row(caret_row, first, lines.len()) {
            Some(n) => {
                let line = &lines[n];
                let col = input.buf.cursor() - rows[caret_row].start;
                let overflow = (line.width() + caret_width - bounds.size.width).max(px(0.));
                let to_caret = line.x_for_index(col) + caret_width - bounds.size.width;
                bounds.left() - to_caret.clamp(px(0.), overflow)
            }
            None => bounds.left(),
        };
        // Each shown atom's pill, worked out once: painted, and a hitbox for the pointing hand.
        let line_height = m.line_height;
        let mut pills = Vec::new();
        for (n, line) in lines.iter().enumerate() {
            let row = &rows[first + n];
            let top = bounds.top() + line_height * n as f32;
            for span in atom_spans(row, &atoms) {
                let at = row.start + span.start;
                let Some(atom) = atoms.iter().position(|a| a.start <= at && at < a.end) else {
                    continue;
                };
                let rect = Bounds::from_corners(
                    point(left + line.x_for_index(span.start) + px(1.), top + px(2.)),
                    point(
                        left + line.x_for_index(span.end) - px(1.),
                        top + line_height - px(2.),
                    ),
                );
                pills.push((
                    atom,
                    rect,
                    window.insert_hitbox(rect, HitboxBehavior::Normal),
                ));
            }
        }
        Shaped {
            rows,
            first,
            lines,
            left,
            pills,
        }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        shaped: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let appearance = cx.global::<Appearance>();
        let m = CellMetrics::measure(appearance, window);
        let (cell_w, line_height) = (m.cell_width, m.line_height);
        let theme = appearance.theme;
        let ui = theme.ui();
        let settings = CursorSettings {
            setting: appearance.cursor_style,
            focused: false,
            key_window: false,
            motion: appearance.animations && !cx.reduce_motion(),
            glide: appearance.cursor_glide,
            glow: appearance.cursor_glow,
            fun: appearance.cursor_fun,
            // The box's typing is idle: the theme's cursor, or the idle status colour.
            color: cursor_look::cursor_color(
                Status::Idle,
                appearance.cursor_follows_status,
                &ui,
                &theme,
            ),
        };
        let (focus_handle, caret, selection, key_window, placeholder) = {
            let input = self.input.read(cx);
            (
                input.focus_handle.clone(),
                input.buf.cursor(),
                input.buf.selection(),
                input.key_window,
                // Empty: the agent's guess if there is one (faint, "⇥" after it), else the
                // placeholder (dim).
                input.buf.text().is_empty().then(|| match &input.ghost {
                    Some(ghost) => (ghost.clone(), true),
                    None => (input.placeholder.clone(), false),
                }),
            )
        };
        let key_window = key_window.unwrap_or_else(|| window.is_window_active());
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.input.clone()),
            cx,
        );
        let (rows, first, lines) = (&shaped.rows, shaped.first, &shaped.lines);
        let caret_row = row_at(rows, caret);
        let caret_shown = shown_row(caret_row, first, lines.len());
        let left = shaped.left;
        // The pills: a pointing hand over each (this request is newer than the box's I-beam,
        // so it wins), the one under the pointer lit, and a move that changes which redraws.
        let hovered = shaped
            .pills
            .iter()
            .find(|(_, _, hitbox)| hitbox.is_hovered(window))
            .map(|(atom, _, _)| *atom);
        for (_, _, hitbox) in &shaped.pills {
            window.set_cursor_style(CursorStyle::PointingHand, hitbox);
        }
        if !shaped.pills.is_empty() {
            let hitboxes: Vec<(usize, Hitbox)> = shaped
                .pills
                .iter()
                .map(|(atom, _, hitbox)| (*atom, hitbox.clone()))
                .collect();
            let input = self.input.clone();
            window.on_mouse_event(move |_: &MouseMoveEvent, phase, window, cx| {
                if phase != DispatchPhase::Bubble {
                    return;
                }
                let now = hitboxes
                    .iter()
                    .find(|(_, hitbox)| hitbox.is_hovered(window))
                    .map(|(atom, _)| *atom);
                input.update(cx, |input, cx| {
                    if input.hovered_atom != now {
                        input.hovered_atom = now;
                        cx.notify();
                    }
                });
            });
        }
        let shade = ui.selection;
        // The caret: the terminal's cursor over the shown rows, as cells from `origin` (the
        // shape, glide, trail, glow and sparks a pane would draw; "unset" by any program, so
        // the cursor setting's shape).
        let origin = point(left, bounds.top());
        // Empty with a beam caret, the placeholder (or the guess) starts just right of the beam
        // (`PLACEHOLDER_IN`), focused or not, so the words never move; any other shape drawn
        // there (the hollow cell without focus) moves with them, onto their first letter.
        let words_in = if placeholder.is_some() && settings.setting == CursorShapeSetting::Bar {
            px(cursor_look::PLACEHOLDER_IN)
        } else {
            px(0.)
        };
        let look = {
            let text = self.input.read(cx).buf.text();
            let blank = CellView::blank(&theme);
            let cells: Vec<Vec<CellView>> = (0..lines.len())
                .map(|n| {
                    text[rows[first + n].clone()]
                        .chars()
                        .map(|ch| CellView { ch, ..blank })
                        .collect()
                })
                .collect();
            let cols = cells.iter().map(Vec::len).max().unwrap_or(0) + 1;
            let cursor = caret_shown.map(|n| CursorView {
                row: n,
                col: text[rows[caret_row].start..caret].chars().count(),
                shape: CursorShape::HollowBlock,
                blinking: true,
            });
            let snap = GridSnapshot {
                cols: cols.max(wrap_columns(bounds.size.width, cell_w) + 1),
                rows: lines.len(),
                lines: cells,
                cursor,
                alt_screen: false,
                // A scroll of the box is another screen: the caret jumps, it doesn't glide.
                display_offset: first,
                clears: 0,
                blocks: Vec::new(),
            };
            let live = CursorSettings {
                focused: focus_handle.is_focused(window),
                key_window,
                ..settings
            };
            self.input.update(cx, |input, _| {
                live.look(&snap, &theme, &mut input.motion, Instant::now())
            })
        };
        // Where the cursor's cells start: shifted with the words, except a beam (before them).
        let cursor_origin = |c: &crate::terminal_view::CursorLook| {
            if c.shape == PaintShape::Bar {
                origin
            } else {
                origin + point(words_in, px(0.))
            }
        };
        if let Some(c) = &look {
            crate::terminal_view::paint_cursor_under(c, cursor_origin(c), &m, window);
        }
        for (n, line) in lines.iter().enumerate() {
            let range = &rows[first + n];
            let top = bounds.top() + line_height * n as f32;
            // A selection is shaded behind the text; past a row's end it runs a little on.
            if !selection.is_empty() && selection.start <= range.end && selection.end >= range.start
            {
                let s = selection.start.max(range.start) - range.start;
                let e = selection.end.min(range.end) - range.start;
                let tail = if selection.end > range.end {
                    px(4.)
                } else {
                    px(0.)
                };
                if e > s || tail > px(0.) {
                    window.paint_quad(fill(
                        Bounds::from_corners(
                            point(left + line.x_for_index(s), top),
                            point(left + line.x_for_index(e) + tail, top + line_height),
                        ),
                        hsla(shade),
                    ));
                }
            }
            // An atom is a pill behind its text (its brackets drawn clear: the pill's padding),
            // deeper while the pointer is on it.
            for (atom, rect, _) in &shaped.pills {
                if rect.top() < top || rect.top() >= top + line_height {
                    continue;
                }
                let deep = if hovered == Some(*atom) {
                    PILL_HOVER
                } else {
                    PILL
                };
                window.paint_quad(
                    fill(
                        *rect,
                        hsla(crate::theme::mix(ui.composer_inner, theme.foreground, deep)),
                    )
                    .corner_radii(px(4.)),
                );
            }
            line.paint(
                point(left, top),
                line_height,
                TextAlign::Left,
                None,
                window,
                cx,
            )
            .ok();
        }
        // The placeholder (the shell box's style): from the caret's own cell, no gap (your
        // words, 2026-11-22: a beam right beside its first letter, an underline or a block on
        // it), dim, "…" when it doesn't fit.
        let mut drawn_placeholder = None;
        let mut drawn_ghost = None;
        let empty_ink = match &placeholder {
            Some((_, true)) => hsla(ui.text_faint),
            _ => hsla(ui.text_dim),
        };
        if let Some((words, ghost)) = placeholder {
            let cols = (bounds.size.width / cell_w).floor() as usize;
            // The ghost on one row, with room for its " ⇥".
            let words = if ghost {
                let one_row = words.split_whitespace().collect::<Vec<_>>().join(" ");
                let fit = crate::card_view::fit_words(&one_row, cols.saturating_sub(2));
                if fit.is_empty() {
                    fit
                } else {
                    format!("{fit} \u{21E5}")
                }
            } else {
                crate::card_view::fit_words(&words, cols)
            };
            let at = point(bounds.left() + words_in, bounds.top());
            if !words.is_empty() {
                let run = TextRun {
                    len: words.len(),
                    font: m.font.clone(),
                    color: empty_ink,
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                };
                let shaped = window.text_system().shape_line(
                    words.clone().into(),
                    m.font_size,
                    &[run],
                    None,
                );
                shaped
                    .paint(at, line_height, TextAlign::Left, None, window, cx)
                    .ok();
                let drawn = (
                    words.clone(),
                    Bounds::new(at, size(shaped.width, line_height)),
                );
                if ghost {
                    let text = words.trim_end_matches('\u{21E5}').trim_end().to_string();
                    drawn_ghost = Some((text, drawn.1));
                }
                drawn_placeholder = Some(drawn);
            }
        }
        // A block on its cell: the letter under it in the cursor's text colour, as much as the
        // block shows through its breath; then the other shapes and the sparks over the text.
        let mut drawn_caret = None;
        if let Some(c) = &look {
            let origin = cursor_origin(c);
            let (col, n) = c.cell;
            let text = self.input.read(cx).buf.text();
            let row = &rows[first + n];
            // The letter under it: what you typed, else (nothing typed) the placeholder's first.
            let (under, ink) = match text[row.clone()].chars().nth(col) {
                Some(ch) => (Some(ch), window.text_style().color),
                None if text.is_empty() => (
                    drawn_placeholder
                        .as_ref()
                        .and_then(|(words, _): &(String, _)| words.chars().next()),
                    empty_ink,
                ),
                None => (None, window.text_style().color),
            };
            if c.shape == PaintShape::Block
                && c.at == (col as f32, n as f32)
                && let Some(ch) = under.filter(|ch| !ch.is_whitespace())
            {
                let run = TextRun {
                    len: ch.len_utf8(),
                    font: m.font.clone(),
                    color: ink.blend(hsla(c.text).opacity(c.alpha)),
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                };
                let shaped = window.text_system().shape_line(
                    ch.to_string().into(),
                    m.font_size,
                    &[run],
                    None,
                );
                let at = point(
                    origin.x + cell_w * col as f32,
                    origin.y + line_height * n as f32,
                );
                shaped
                    .paint(at, line_height, TextAlign::Left, None, window, cx)
                    .ok();
            }
            crate::terminal_view::paint_cursor_over(c, origin, &m, window);
            let at = point(origin.x + cell_w * c.at.0, origin.y + line_height * c.at.1);
            drawn_caret = Some((c.shape, c.bounds(at, &m, 1.0)));
        }
        let frames = self.input.read(cx).motion.frames;
        match frames {
            Frames::EveryFrame => window.request_animation_frame(),
            Frames::Breath => self.input.update(cx, |input, cx| input.breathe(window, cx)),
            Frames::None => {}
        }
        let lines = std::mem::take(&mut shaped.lines);
        let pills = shaped.pills.iter().map(|(_, rect, _)| *rect).collect();
        self.input.update(cx, |input, _| {
            input.first_row = first;
            input.last_lines = lines;
            input.last_origin = Some(point(left, bounds.top()));
            input.last_bounds = Some(bounds);
            input.last_line_height = line_height;
            input.last_cell = (cell_w, line_height);
            input.last_caret = drawn_caret;
            // The ghost is not the placeholder: scenes that read the placeholder see none.
            input.last_placeholder = drawn_placeholder.filter(|_| drawn_ghost.is_none());
            input.last_ghost = drawn_ghost;
            input.hovered_atom = hovered;
            input.last_pills = pills;
        });
    }
}

/// The atoms on visual row `row`, as byte ranges from the row's start, in order: an atom split
/// over rows is a span on each; atoms off the row (or only touching its edges) aren't there.
fn atom_spans(row: &Range<usize>, atoms: &[Range<usize>]) -> Vec<Range<usize>> {
    atoms
        .iter()
        .filter(|a| a.end > row.start && a.start < row.end)
        .map(|a| a.start.max(row.start) - row.start..a.end.min(row.end) - row.start)
        .collect()
}

/// How many cells of width `cell` fit in `width` (at least one), less a cell for the caret.
fn wrap_columns(width: Pixels, cell: Pixels) -> usize {
    if cell <= px(0.) {
        return 1;
    }
    (((width - cell) / cell).floor() as usize).max(1)
}

#[cfg(test)]
mod tests {
    use super::{EditBuffer, atom_spans, shown_row};

    #[test]
    fn atom_spans_are_the_atoms_on_a_row_row_relative() {
        // Rows 0..10 and 10..20; an atom 6..14 splits over both.
        let atom = 6..14;
        let atoms = std::slice::from_ref(&atom);
        assert_eq!(atom_spans(&(0..10), atoms), vec![6..10]);
        assert_eq!(atom_spans(&(10..20), atoms), vec![0..4]);
        // Atoms before, after or only touching the row's edges aren't on it.
        assert!(atom_spans(&(20..30), &[0..20, 30..40]).is_empty());
        // Two atoms on one row: both, in order.
        assert_eq!(atom_spans(&(10..30), &[12..15, 20..26]), vec![2..5, 10..16]);
    }

    /// Prompt history (a chore after M11ar): ↑ recalls only from the first visual row, ↓ only
    /// from the last, so a wrapped or multi-line prompt walks its rows first.
    #[test]
    fn first_and_last_row_follow_wrapping() {
        let mut b = EditBuffer::new("");
        assert!(b.on_first_row() && b.on_last_row(), "an empty box is both");
        // "aaaa bbbb cccc" wraps in 3 at 5 columns: "aaaa ", "bbbb ", "cccc".
        b.set_text("aaaa bbbb cccc\nzz");
        b.set_wrap(Some(5));
        b.set_cursor(3);
        assert!(b.on_first_row());
        assert!(!b.on_last_row());
        b.set_cursor(7);
        assert!(!b.on_first_row(), "the second visual row of the first line");
        assert!(!b.on_last_row());
        b.set_cursor(12);
        assert!(
            !b.on_last_row(),
            "the first line's last row is not the text's"
        );
        b.set_cursor(16);
        assert!(b.on_last_row());
        // Unwrapped, the first line is one row.
        b.set_wrap(None);
        b.set_cursor(12);
        assert!(b.on_first_row());
        b.set_cursor(b.text().len());
        assert!(b.on_last_row());
    }

    #[test]
    fn up_and_down_move_by_visual_row() {
        // "aaaa bbbb cccc" wraps in 3 at 5 columns: "aaaa ", "bbbb ", "cccc".
        let mut b = EditBuffer::new("aaaa bbbb cccc\nzz");
        b.set_wrap(Some(5));
        assert_eq!(b.visual_rows(), vec![0..5, 5..10, 10..14, 15..17]);
        b.set_cursor(1);
        b.down();
        assert_eq!(b.cursor(), 6, "the second visual row, same column");
        b.down();
        assert_eq!(b.cursor(), 11, "the third, still the first line");
        b.down();
        assert_eq!(b.cursor(), 16, "then the next line");
        b.up();
        b.up();
        assert_eq!(b.cursor(), 6);
        // Unwrapped, down goes straight to the next line.
        b.set_wrap(None);
        b.set_cursor(1);
        b.down();
        assert_eq!(b.cursor(), 16);
    }

    #[test]
    fn a_word_longer_than_the_row_breaks_inside_it() {
        let mut b = EditBuffer::new("abcdefghij kl");
        b.set_wrap(Some(4));
        assert_eq!(b.visual_rows(), vec![0..4, 4..8, 8..11, 11..13]);
        assert_eq!(b.visual_rows().len(), b.visual_row_count());
        // A caret on a wrap boundary sits at the start of the next row.
        assert_eq!(b.visual_row_of(4), 1);
        assert_eq!(b.visual_row_of(13), 3, "the text's end, on the last row");
        // Down from the end of a wrapping row stays on the row below, before its wrap.
        b.set_cursor(3);
        b.down();
        assert_eq!(b.cursor(), 7);
        let empty = EditBuffer::new("");
        assert_eq!(empty.visual_rows(), vec![0..0]);
    }

    #[test]
    fn command_arrows_with_a_selection_go_to_the_line_ends() {
        let mut b = EditBuffer::new("one two\nthree four");
        b.set_cursor(9);
        b.right(true);
        b.right(true);
        assert_eq!(b.selection(), 9..11);
        b.line_start(false);
        assert_eq!(b.cursor(), 8, "the line's start, not the selection's");
        b.set_cursor(1);
        b.right(true);
        b.line_end(false);
        assert_eq!(b.cursor(), 7, "the line's end, not the selection's");
    }

    #[test]
    fn a_row_below_the_box_has_no_place() {
        assert_eq!(shown_row(3, 2, 8), Some(1));
        assert_eq!(shown_row(1, 2, 8), None, "above it");
        assert_eq!(shown_row(10, 2, 8), None, "below it");
        assert_eq!(shown_row(9, 2, 8), Some(7));
    }

    #[test]
    fn newline_then_up_keeps_column() {
        let mut b = EditBuffer::new("abcd");
        b.newline();
        b.insert("xy");
        assert_eq!(b.text(), "abcd\nxy");
        b.up();
        assert_eq!(b.cursor(), 2, "column 2 of the first line");
        b.down();
        assert_eq!(b.cursor(), 7, "back to column 2 of the second");
    }

    #[test]
    fn up_and_down_remember_the_wider_column_over_a_short_line() {
        let mut b = EditBuffer::new("abcd\nx\nefgh");
        b.up();
        assert_eq!(b.cursor(), 6, "the short line's end");
        b.up();
        assert_eq!(b.cursor(), 4, "column 4 again");
        b.down();
        b.down();
        assert_eq!(b.cursor(), 11);
    }

    #[test]
    fn up_on_the_first_line_goes_to_the_start_and_down_on_the_last_to_the_end() {
        let mut b = EditBuffer::new("ab\ncd");
        b.up();
        b.up();
        assert_eq!(b.cursor(), 0);
        b.down();
        b.down();
        assert_eq!(b.cursor(), 5);
    }

    #[test]
    fn an_atom_edits_as_one() {
        let mut b = EditBuffer::new("see [Image #1] here");
        b.set_atoms(vec!["[Image #1]".into()]);
        assert_eq!(b.atom_ranges(), vec![4..14]);
        // ← from after it jumps to its start; → from its start to its end.
        b.set_cursor(14);
        b.left(false);
        assert_eq!(b.cursor(), 4);
        b.right(false);
        assert_eq!(b.cursor(), 14);
        // A click inside lands on the nearer edge.
        b.move_to(6, false);
        assert_eq!(b.cursor(), 4);
        b.move_to(12, false);
        assert_eq!(b.cursor(), 14);
        // ⌫ after it takes it whole; ⌦ before it too.
        b.backspace();
        assert_eq!(b.text(), "see  here");
        let mut b = EditBuffer::new("[Image #2]!");
        b.set_atoms(vec!["[Image #2]".into()]);
        b.set_cursor(0);
        b.delete();
        assert_eq!(b.text(), "!");
        // Text that isn't an atom edits as before.
        let mut b = EditBuffer::new("[Image #3]");
        b.backspace();
        assert_eq!(b.text(), "[Image #3");
    }

    #[test]
    fn an_atom_matches_only_on_word_edges() {
        // A path pill's text inside a longer path, or a folder's inside its file's: not it.
        let mut b = EditBuffer::new("see src/main.rs, not code/src/main.rs or src/main.rs.bak");
        b.set_atoms(vec!["src/main.rs".into()]);
        assert_eq!(b.atom_ranges(), vec![4..15]);
        let mut b = EditBuffer::new("src/ and src/main.rs");
        b.set_atoms(vec!["src/".into()]);
        assert_eq!(b.atom_ranges(), vec![0..4]);
        // At a sentence's end it still is; an image's marker beside a word too.
        let mut b = EditBuffer::new("open src/main.rs. x[Image #1]y");
        b.set_atoms(vec!["src/main.rs".into(), "[Image #1]".into()]);
        assert_eq!(b.atom_ranges(), vec![5..16, 19..29]);
        // Where two start together, the longer wins.
        let mut b = EditBuffer::new("a src/ b");
        b.set_atoms(vec!["src".into(), "src/".into()]);
        assert_eq!(b.atom_ranges(), vec![2..6]);
    }

    #[test]
    fn only_a_bracketed_atom_hides_its_brackets() {
        // An image's marker draws as its name on the pill; a path pill (batch 3, chore 5) keeps
        // every character.
        let text = "see [Image #1] and src/main.rs";
        let atoms = [4..14, 19..30];
        assert_eq!(super::bracketed(text, &atoms), vec![4..14]);
    }

    #[test]
    fn atoms_survive_set_text() {
        let mut b = EditBuffer::new("");
        b.set_atoms(vec!["[Image #1]".into()]);
        b.set_text("a [Image #1]");
        assert_eq!(b.atom_ranges(), vec![2..12]);
    }

    #[test]
    fn backspace_joins_lines() {
        let mut b = EditBuffer::new("ab\ncd");
        b.set_cursor(3);
        b.backspace();
        assert_eq!(b.text(), "abcd");
        assert_eq!(b.cursor(), 2);
    }

    #[test]
    fn rows_counts_newlines() {
        assert_eq!(EditBuffer::new("a\nb\nc").rows(), 3);
        assert_eq!(EditBuffer::new("").rows(), 1);
        assert_eq!(EditBuffer::new("a\n").rows(), 2);
    }

    #[test]
    fn insert_replaces_selection() {
        let mut b = EditBuffer::new("hello world");
        b.set_cursor(0);
        b.right(true);
        b.right(true);
        b.right(true);
        b.right(true);
        b.right(true);
        assert_eq!(b.selection(), 0..5);
        b.insert("bye");
        assert_eq!(b.text(), "bye world");
        assert_eq!(b.cursor(), 3);
        assert!(b.selection().is_empty());
        b.select_all();
        b.insert("x");
        assert_eq!(b.text(), "x");
    }

    #[test]
    fn multibyte_text_stays_on_char_boundaries() {
        let mut b = EditBuffer::new("");
        b.insert("é");
        b.insert("😀");
        b.newline();
        b.insert("é😀b");
        b.left(false);
        assert!(b.text().is_char_boundary(b.cursor()));
        b.up();
        assert!(b.text().is_char_boundary(b.cursor()));
        assert_eq!(b.cursor(), "é😀".len(), "column 2 chars into line 0");
        b.down();
        assert!(b.text().is_char_boundary(b.cursor()));
        b.backspace();
        assert!(b.text().is_char_boundary(b.cursor()));
        assert_eq!(b.text(), "é😀\néb");
        b.set_cursor(1);
        assert_eq!(b.cursor(), 0, "a byte inside é snaps back to a boundary");
    }

    #[test]
    fn line_moves_and_selection_collapse() {
        let mut b = EditBuffer::new("ab\ncd");
        b.line_start(false);
        assert_eq!(b.cursor(), 3);
        b.up();
        b.line_end(false);
        assert_eq!(b.cursor(), 2);
        b.select_all();
        b.left(false);
        assert_eq!(b.cursor(), 0, "a selection collapses to its start");
        b.select_all();
        b.right(false);
        assert_eq!(b.cursor(), 5, "or its end");
    }

    #[test]
    fn delete_to_line_start_and_forward_delete() {
        let mut b = EditBuffer::new("ab\ncd");
        b.delete_to_line_start();
        assert_eq!(b.text(), "ab\n");
        b.delete_to_line_start();
        assert_eq!(b.text(), "ab", "at a line start it takes the newline");
        b.set_cursor(0);
        b.delete();
        assert_eq!(b.text(), "b");
        let mut m = EditBuffer::new("é😀");
        m.set_cursor(0);
        m.delete();
        m.delete();
        assert_eq!(m.text(), "");
    }

    #[test]
    fn delete_word_left_takes_the_word_before_the_caret() {
        let mut b = EditBuffer::new("git commit -m  ");
        b.delete_word_left();
        assert_eq!(
            b.text(),
            "git commit -",
            "the spaces and the word before them"
        );
        b.delete_word_left();
        assert_eq!(b.text(), "git ");
        let mut s = EditBuffer::new("one two");
        s.select_range(0, 3);
        s.delete_word_left();
        assert_eq!(s.text(), " two", "a selection goes first");
        let mut l = EditBuffer::new("a\nb");
        l.set_cursor(2);
        l.delete_word_left();
        assert_eq!(l.text(), "b", "across the line break, like zsh");
        let mut e = EditBuffer::new("é😀 x");
        e.set_cursor(6);
        e.delete_word_left();
        assert_eq!(e.text(), " x");
    }

    #[test]
    fn delete_to_line_end_kills_the_rest_of_the_line() {
        let mut b = EditBuffer::new("ab\ncd");
        b.set_cursor(1);
        b.delete_to_line_end();
        assert_eq!(b.text(), "a\ncd");
        b.delete_to_line_end();
        assert_eq!(b.text(), "acd", "at a line end it takes the newline");
        b.set_cursor(b.text().len());
        b.delete_to_line_end();
        assert_eq!(b.text(), "acd", "nothing after the caret: nothing");
        let mut s = EditBuffer::new("abc");
        s.select_range(0, 1);
        s.delete_to_line_end();
        assert_eq!(s.text(), "bc", "a selection goes first");
    }

    #[test]
    fn word_moves_cross_lines() {
        let mut b = EditBuffer::new("one two\nthree");
        b.word_left(false);
        assert_eq!(b.cursor(), 8);
        b.word_left(false);
        assert_eq!(b.cursor(), 4);
        b.word_right(true);
        assert_eq!(b.selection(), 4..7);
    }

    #[test]
    fn at_word_start_only_after_whitespace_or_at_the_start() {
        let b = EditBuffer::new("@a x@y\n@z me@");
        assert!(b.at_word_start(0));
        assert!(!b.at_word_start(4 + 1), "mid-word x@y");
        assert!(b.at_word_start(7), "after a newline");
        assert!(!b.at_word_start(b.text().len() - 1), "me@");
        assert!(!b.at_word_start(1), "not an @");
    }

    #[test]
    fn row_ranges_and_row_of() {
        let b = EditBuffer::new("é\n\nxy");
        assert_eq!(b.row_ranges(), vec![0..2, 3..3, 4..6]);
        assert_eq!(b.row_of(3), 1);
        assert_eq!(b.row_of(6), 2);
    }
}
