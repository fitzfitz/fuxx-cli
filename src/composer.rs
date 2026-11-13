#![allow(dead_code)] // consumers arrive in M11aa Tasks 7–11; remove when wired
//! The agent tab's message box: a multi-line text input (M11aa). Modelled on `name_input.rs`
//! (IME through `EntityInputHandler`, UTF-16 offsets at the edge, UTF-8 inside), but multi-line:
//! Enter sends, ⇧Enter is a newline, Esc stops the agent, and losing focus does not cancel.
//! It grows to `MAX_ROWS` rows, then scrolls to keep the caret in view. Lines are not
//! soft-wrapped: a line wider than the box scrolls sideways with the caret.
//!
//! The editing itself is `EditBuffer`, pure and unit-tested; `Composer` is a thin layer of
//! actions, IME and painting over it.

use std::ops::Range;

use gpui::{
    App, Bounds, ClipboardItem, Context, ElementId, ElementInputHandler, Entity,
    EntityInputHandler, EventEmitter, FocusHandle, Focusable, GlobalElementId, InspectorElementId,
    LayoutId, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point, ShapedLine,
    SharedString, Style, TextAlign, TextRun, UTF16Selection, UnderlineStyle, Window, actions, div,
    fill, point, prelude::*, px, relative, size,
};

use crate::appearance::Appearance;
use crate::name_input::{next_char, prev_char, utf8_offset, utf16_offset, word_end, word_start};
use crate::terminal_view::hsla;

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
        SelectToEnd
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
}

/// The pure editing core: text, a caret and an anchor (the selection is between them), all byte
/// offsets that always sit on char boundaries.
pub struct EditBuffer {
    text: String,
    cursor: usize,
    anchor: usize,
    /// The column ↑/↓ keep across lines shorter than it (in chars); reset by any other move.
    goal: Option<usize>,
}

impl EditBuffer {
    /// `text` with the caret at its end.
    pub fn new(text: &str) -> Self {
        Self {
            text: text.to_string(),
            cursor: text.len(),
            anchor: text.len(),
            goal: None,
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn anchor(&self) -> usize {
        self.anchor
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
    pub fn set_cursor(&mut self, at: usize) {
        self.move_to(at, false);
    }

    /// The caret to `at`: alone, or (`extend`) taking the selection with it from the anchor.
    pub fn move_to(&mut self, at: usize, extend: bool) {
        self.cursor = self.snap(at);
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

    /// Replace everything; the caret goes to the end.
    pub fn set_text(&mut self, text: &str) {
        *self = Self::new(text);
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
            self.replace(prev_char(&self.text, self.cursor)..self.cursor, "");
        } else {
            self.replace(sel, "");
        }
    }

    /// ⌦: the selection, else the character after the caret.
    pub fn delete(&mut self) {
        let sel = self.selection();
        if sel.is_empty() {
            self.replace(self.cursor..next_char(&self.text, self.cursor), "");
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

    /// A horizontal move: without ⇧ a selection collapses to its edge on that side instead.
    fn step(&mut self, to: fn(&str, usize) -> usize, left: bool, extend: bool) {
        let sel = self.selection();
        let at = if !extend && !sel.is_empty() {
            if left { sel.start } else { sel.end }
        } else {
            to(&self.text, self.cursor)
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
    pub fn line_start(&mut self, extend: bool) {
        self.step(|s, i| s[..i].rfind('\n').map_or(0, |j| j + 1), true, extend);
    }
    pub fn line_end(&mut self, extend: bool) {
        self.step(
            |s, i| s[i..].find('\n').map_or(s.len(), |j| i + j),
            false,
            extend,
        );
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

    fn vertical(&mut self, up: bool, extend: bool) {
        let here = self.line_start_of(self.cursor);
        let goal = self
            .goal
            .unwrap_or_else(|| self.text[here..self.cursor].chars().count());
        let at = if up {
            if here == 0 {
                0
            } else {
                let start = self.line_start_of(here - 1);
                self.at_column(start, here - 1, goal)
            }
        } else {
            let end = self.line_end_of(self.cursor);
            if end == self.text.len() {
                end
            } else {
                let start = end + 1;
                self.at_column(start, self.line_end_of(start), goal)
            }
        };
        self.move_to(at, extend);
        self.goal = Some(goal);
    }

    /// The byte `column` chars into the line `start..end` (its end if it is shorter).
    fn at_column(&self, start: usize, end: usize, column: usize) -> usize {
        self.text[start..end]
            .char_indices()
            .nth(column)
            .map_or(end, |(i, _)| start + i)
    }

    fn line_start_of(&self, i: usize) -> usize {
        self.text[..i].rfind('\n').map_or(0, |j| j + 1)
    }

    fn line_end_of(&self, i: usize) -> usize {
        self.text[i..].find('\n').map_or(self.text.len(), |j| i + j)
    }

    /// How many rows the text takes (an empty text is one).
    pub fn rows(&self) -> usize {
        self.text.matches('\n').count() + 1
    }

    /// The row byte `i` is on.
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
    /// The IME's composition in progress, as a byte range of the text.
    marked: Option<Range<usize>>,
    /// A mouse drag is selecting.
    dragging: bool,
    /// The first row shown (the box scrolls to keep the caret in view).
    first_row: usize,
    /// From the last paint, for the IME's candidate window and the mouse: every row shaped,
    /// where the text's left edge and the box's top were, and the row height.
    last_lines: Vec<ShapedLine>,
    last_origin: Option<Point<Pixels>>,
    last_bounds: Option<Bounds<Pixels>>,
    last_line_height: Pixels,
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
            marked: None,
            dragging: false,
            first_row: 0,
            last_lines: Vec::new(),
            last_origin: None,
            last_bounds: None,
            last_line_height: px(0.),
        }
    }

    pub fn text(&self) -> &str {
        self.buf.text()
    }

    pub fn set_text(&mut self, text: String, cx: &mut Context<Self>) {
        self.buf.set_text(&text);
        self.marked = None;
        cx.notify();
    }

    /// `text` at the caret (in place of the selection).
    pub fn insert(&mut self, text: &str, cx: &mut Context<Self>) {
        self.buf.insert(&clean(text));
        self.marked = None;
        cx.notify();
    }

    pub fn set_placeholder(&mut self, p: SharedString, cx: &mut Context<Self>) {
        self.placeholder = p;
        cx.notify();
    }

    fn step(&mut self, f: impl FnOnce(&mut EditBuffer), cx: &mut Context<Self>) {
        self.marked = None;
        f(&mut self.buf);
        cx.notify();
    }

    steps! {
        backspace: Backspace => |b| b.backspace();
        delete: Delete => |b| b.delete();
        clear: Clear => |b| b.delete_to_line_start();
        newline: Newline => |b| b.newline();
        select_all: SelectAll => |b| b.select_all();
        move_left: MoveLeft => |b| b.left(false);
        move_right: MoveRight => |b| b.right(false);
        move_up: MoveUp => |b| b.up();
        move_down: MoveDown => |b| b.down();
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

    fn copy_selection(&self, cx: &mut Context<Self>) {
        let sel = self.buf.selection();
        if !sel.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(self.buf.text()[sel].to_string()));
        }
    }

    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        self.copy_selection(cx);
    }

    fn cut(&mut self, _: &Cut, _: &mut Window, cx: &mut Context<Self>) {
        self.copy_selection(cx);
        if !self.buf.selection().is_empty() {
            self.step(|b| b.backspace(), cx);
        }
    }

    fn paste(&mut self, _: &Paste, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.insert(&text, cx);
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
        let row = (self.first_row + below).min(self.last_lines.len().checked_sub(1)?);
        let range = self.buf.row_ranges().get(row)?.clone();
        let x = at.x - origin.x;
        let col = self.last_lines[row].closest_index_for_x(x).min(range.len());
        Some(range.start + col)
    }

    /// A click puts the caret there (⇧: selects to there); a double-click selects it all; a
    /// drag selects.
    fn mouse_down(&mut self, e: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        window.focus(&self.focus_handle, cx);
        self.marked = None;
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
        let row = self.buf.row_of(index);
        let line = self.last_lines.get(row)?;
        let start = self.buf.row_ranges().get(row)?.start;
        let shown = row.checked_sub(self.first_row)?;
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
        let edge = if self.focus_handle.is_focused(window) {
            ui.focus_ring
        } else {
            ui.text_dim
        };
        let empty = self.buf.text().is_empty();
        let placeholder = self.placeholder.clone();
        div()
            .w_full()
            .overflow_hidden()
            .px_2()
            .py_1()
            .rounded_sm()
            .border_1()
            .border_color(hsla(edge))
            .bg(hsla(ui.sidebar_background))
            .text_color(hsla(ui.text))
            .key_context("Composer")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::send))
            .on_action(cx.listener(Self::newline))
            .on_action(cx.listener(Self::stop))
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::clear))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::show_character_palette))
            .on_action(cx.listener(Self::move_left))
            .on_action(cx.listener(Self::move_right))
            .on_action(cx.listener(Self::move_up))
            .on_action(cx.listener(Self::move_down))
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
            .on_mouse_move(cx.listener(Self::mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::mouse_up))
            .child(
                div()
                    .relative()
                    .child(ComposerText { input: cx.entity() })
                    .when(empty, |d| {
                        d.child(
                            div()
                                .absolute()
                                .top_0()
                                .left_0()
                                .text_color(hsla(ui.text_dim))
                                .child(placeholder),
                        )
                    }),
            )
    }
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

impl Element for ComposerText {
    type RequestLayoutState = ();
    type PrepaintState = Vec<ShapedLine>;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let rows = self.input.read(cx).buf.rows().min(MAX_ROWS);
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = (window.line_height() * rows as f32).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let input = self.input.read(cx);
        let style = window.text_style();
        let base = TextRun {
            len: 0,
            font: style.font(),
            color: style.color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let font_size = style.font_size.to_pixels(window.rem_size());
        let text = input.buf.text();
        input
            .buf
            .row_ranges()
            .iter()
            .map(|range| {
                let runs = runs_for(range, input.marked.as_ref(), &base);
                window.text_system().shape_line(
                    text[range.clone()].to_string().into(),
                    font_size,
                    &runs,
                    None,
                )
            })
            .collect()
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        lines: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let line_height = window.line_height();
        let (focus_handle, buf_rows, caret, selection, first_row_was) = {
            let input = self.input.read(cx);
            (
                input.focus_handle.clone(),
                input.buf.row_ranges(),
                input.buf.cursor(),
                input.buf.selection(),
                input.first_row,
            )
        };
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.input.clone()),
            cx,
        );
        // Scroll just enough to keep the caret's row in view.
        let rows = buf_rows.len();
        let shown = rows.min(MAX_ROWS);
        let caret_row = buf_rows
            .iter()
            .position(|r| caret <= r.end)
            .unwrap_or(rows - 1);
        let mut first = first_row_was.min(rows - shown);
        if caret_row < first {
            first = caret_row;
        } else if caret_row >= first + shown {
            first = caret_row + 1 - shown;
        }
        // A caret line wider than the box scrolls left so the caret stays visible.
        let caret_width = px(2.);
        let caret_col = caret - buf_rows[caret_row].start;
        let caret_line = &lines[caret_row];
        let overflow = (caret_line.width() + caret_width - bounds.size.width).max(px(0.));
        let to_caret = caret_line.x_for_index(caret_col) + caret_width - bounds.size.width;
        let left = bounds.left() - to_caret.clamp(px(0.), overflow);
        let shade = cx.global::<Appearance>().theme.ui().selection;
        for (n, row) in (first..first + shown).enumerate() {
            let (range, line) = (&buf_rows[row], &lines[row]);
            let top = bounds.top() + line_height * n as f32;
            // A selection is shaded behind the text; across a newline it runs a little past it.
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
        if focus_handle.is_focused(window) {
            let x = left + caret_line.x_for_index(caret_col);
            let y = bounds.top() + line_height * (caret_row - first) as f32;
            let color = window.text_style().color;
            window.paint_quad(fill(
                Bounds::new(point(x, y), size(caret_width, line_height)),
                color,
            ));
        }
        let lines = std::mem::take(lines);
        self.input.update(cx, |input, _| {
            input.first_row = first;
            input.last_lines = lines;
            input.last_origin = Some(point(left, bounds.top()));
            input.last_bounds = Some(bounds);
            input.last_line_height = line_height;
        });
    }
}

#[cfg(test)]
mod tests {
    use super::EditBuffer;

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
