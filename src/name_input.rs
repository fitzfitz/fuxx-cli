//! The inline rename box in the sidebar (and every other one-line box in fuxx): one line of
//! text with a caret you can move (←/→, ⌥ by word, ⌘ to an end, ⇧ to select, a click or a
//! drag), since 2026-11-10 (your call; it used to stay at the end).
//! Typed text, ⌥ accents, the emoji picker and IME composition all arrive through
//! `EntityInputHandler` (macOS speaks UTF-16 offsets; the text is kept as UTF-8).

use std::ops::Range;

use gpui::{
    App, Bounds, Context, ElementId, ElementInputHandler, Entity, EntityInputHandler, EventEmitter,
    FocusHandle, Focusable, GlobalElementId, InspectorElementId, LayoutId, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point, ShapedLine, Style, Subscription,
    TextAlign, TextRun, UTF16Selection, UnderlineStyle, Window, actions, div, fill, point,
    prelude::*, px, relative, size,
};

use crate::appearance::Appearance;
use crate::terminal_view::hsla;

// Bound in main.rs with the "NameInput" key context, so these keys never reach the terminal.
actions!(
    name_input,
    [
        Backspace,
        Clear,
        Confirm,
        Cancel,
        Paste,
        ShowCharacterPalette,
        Delete,
        MoveLeft,
        MoveRight,
        WordLeft,
        WordRight,
        LineStart,
        LineEnd,
        SelectLeft,
        SelectRight,
        SelectWordLeft,
        SelectWordRight,
        SelectToStart,
        SelectToEnd,
        SelectAll
    ]
);

pub enum NameInputEvent {
    /// Enter: the trimmed text (empty means "back to the automatic name").
    Confirm(String),
    /// Escape, or the box lost focus.
    Cancel,
}

pub struct NameInput {
    focus_handle: FocusHandle,
    text: String,
    /// The IME's composition in progress, as a byte range of `text`.
    marked: Option<Range<usize>>,
    /// The selection, as a byte range of `text`; empty: the caret alone. While the IME
    /// composes, its own caret inside the marked text.
    selected: Range<usize>,
    /// The caret is at the selection's start (it was extended leftwards).
    reversed: bool,
    /// A mouse drag is selecting.
    dragging: bool,
    /// From the last paint, for the IME's candidate window and the caret.
    last_line: Option<ShapedLine>,
    last_origin: Option<Point<Pixels>>,
    last_bounds: Option<Bounds<Pixels>>,
    /// Drawn without its own edge and fill (the box it sits in has them: a browser tab's
    /// address field).
    bare: bool,
    _blur: Subscription,
}

impl EventEmitter<NameInputEvent> for NameInput {}

impl Focusable for NameInput {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl NameInput {
    pub fn new(text: String, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        let blur = cx.on_blur(&focus_handle, window, |_, _, cx| {
            cx.emit(NameInputEvent::Cancel)
        });
        let end = text.len();
        Self {
            focus_handle,
            text,
            marked: None,
            selected: end..end,
            reversed: false,
            dragging: false,
            last_line: None,
            last_origin: None,
            last_bounds: None,
            bare: false,
            _blur: blur,
        }
    }

    /// Without its own edge and fill: the box it is put in draws them.
    pub fn bare(&mut self) {
        self.bare = true;
    }

    /// The text as typed so far (with any IME composition in progress).
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Put `text` in the box, the caret at its end (the ⌘K palette drops a `>` it no longer
    /// means).
    pub fn set_text(&mut self, text: String) {
        self.text = text;
        self.caret_to_end();
    }

    /// Select the whole text: typing (or pasting) replaces it, Backspace erases it.
    pub fn select_all(&mut self) {
        self.marked = None;
        self.reversed = false;
        self.selected = 0..self.text.len();
    }

    /// Where the caret is: the selection's moving end.
    fn caret(&self) -> usize {
        if self.reversed {
            self.selected.start
        } else {
            self.selected.end
        }
    }

    /// The caret to byte `to`: alone, or (`extend`) taking the selection with it from its
    /// fixed end.
    fn move_to(&mut self, to: usize, extend: bool) {
        self.marked = None;
        if extend {
            let anchor = if self.reversed {
                self.selected.end
            } else {
                self.selected.start
            };
            self.reversed = to < anchor;
            self.selected = anchor.min(to)..anchor.max(to);
        } else {
            self.reversed = false;
            self.selected = to..to;
        }
    }

    /// A caret key: `to` from the caret where the caret goes. Without ⇧ a selection collapses
    /// to its own edge on that side (`left`) instead.
    fn step(
        &mut self,
        to: fn(&str, usize) -> usize,
        left: bool,
        extend: bool,
        cx: &mut Context<Self>,
    ) {
        let selection = self.selection();
        let at = if !extend && !selection.is_empty() {
            if left { selection.start } else { selection.end }
        } else {
            to(&self.text, self.caret())
        };
        self.move_to(at, extend);
        cx.notify();
    }

    fn move_left(&mut self, _: &MoveLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.step(prev_char, true, false, cx);
    }
    fn move_right(&mut self, _: &MoveRight, _: &mut Window, cx: &mut Context<Self>) {
        self.step(next_char, false, false, cx);
    }
    fn word_left(&mut self, _: &WordLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.step(word_start, true, false, cx);
    }
    fn word_right(&mut self, _: &WordRight, _: &mut Window, cx: &mut Context<Self>) {
        self.step(word_end, false, false, cx);
    }
    fn line_start(&mut self, _: &LineStart, _: &mut Window, cx: &mut Context<Self>) {
        self.step(|_, _| 0, true, false, cx);
    }
    fn line_end(&mut self, _: &LineEnd, _: &mut Window, cx: &mut Context<Self>) {
        self.step(|s, _| s.len(), false, false, cx);
    }
    fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.step(prev_char, true, true, cx);
    }
    fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        self.step(next_char, false, true, cx);
    }
    fn select_word_left(&mut self, _: &SelectWordLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.step(word_start, true, true, cx);
    }
    fn select_word_right(&mut self, _: &SelectWordRight, _: &mut Window, cx: &mut Context<Self>) {
        self.step(word_end, false, true, cx);
    }
    fn select_to_start(&mut self, _: &SelectToStart, _: &mut Window, cx: &mut Context<Self>) {
        self.step(|_, _| 0, true, true, cx);
    }
    fn select_to_end(&mut self, _: &SelectToEnd, _: &mut Window, cx: &mut Context<Self>) {
        self.step(|s, _| s.len(), false, true, cx);
    }
    fn select_all_action(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.select_all();
        cx.notify();
    }

    /// The byte under window point `at` on the line as last painted (the nearest boundary).
    fn index_at(&self, at: Point<Pixels>) -> Option<usize> {
        let x = at.x - self.last_origin?.x;
        Some(
            self.last_line
                .as_ref()?
                .closest_index_for_x(x)
                .min(self.text.len()),
        )
    }

    /// A click puts the caret there (⇧: selects to there); a double-click selects it all; a
    /// drag selects.
    fn mouse_down(&mut self, e: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        // Keep clicks in the box: the sidebar row under it would take focus (and so cancel).
        cx.stop_propagation();
        window.focus(&self.focus_handle, cx);
        if e.click_count >= 2 {
            self.select_all();
        } else if let Some(at) = self.index_at(e.position) {
            self.move_to(at, e.modifiers.shift);
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
            self.move_to(at, true);
            cx.notify();
        }
    }

    fn mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.dragging = false;
    }

    /// The selected text's range; empty with the caret alone (the IME's own selection inside
    /// its marked text isn't one: then the caret is at the marked text's end).
    fn selection(&self) -> Range<usize> {
        match &self.marked {
            Some(marked) => marked.end..marked.end,
            None => self.selected.clone(),
        }
    }

    fn caret_to_end(&mut self) {
        self.move_to(self.text.len(), false);
    }

    /// `text` in place of byte range `range`, the caret after it.
    fn put(&mut self, range: Range<usize>, text: &str) {
        let at = range.start;
        self.text.replace_range(range, text);
        self.move_to(at + text.len(), false);
    }

    fn backspace(&mut self, _: &Backspace, _: &mut Window, cx: &mut Context<Self>) {
        let erased = erase(&self.text, &self.selection(), prev_char);
        self.put(erased, "");
        cx.notify();
    }

    /// ⌦: the selection, else the character after the caret.
    fn delete(&mut self, _: &Delete, _: &mut Window, cx: &mut Context<Self>) {
        let erased = erase(&self.text, &self.selection(), next_char);
        self.put(erased, "");
        cx.notify();
    }

    /// ⌘⌫: the selection, else everything before the caret.
    fn clear(&mut self, _: &Clear, _: &mut Window, cx: &mut Context<Self>) {
        let erased = erase(&self.text, &self.selection(), |_, _| 0);
        self.put(erased, "");
        cx.notify();
    }

    fn confirm(&mut self, _: &Confirm, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(NameInputEvent::Confirm(self.text.trim().to_string()));
    }

    fn cancel(&mut self, _: &Cancel, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(NameInputEvent::Cancel);
    }

    /// The clipboard's text, one line of it, in place of the selection (or at the end).
    fn paste(&mut self, _: &Paste, _: &mut Window, cx: &mut Context<Self>) {
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return;
        };
        let range = self.edit_range(None);
        self.put(range, &printable(&text));
        cx.notify();
    }

    fn show_character_palette(
        &mut self,
        _: &ShowCharacterPalette,
        window: &mut Window,
        _: &mut Context<Self>,
    ) {
        window.show_character_palette();
    }

    fn range_from_utf16(&self, range: &Range<usize>) -> Range<usize> {
        let start = utf8_offset(&self.text, range.start);
        start..utf8_offset(&self.text, range.end).max(start)
    }

    fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        utf16_offset(&self.text, range.start)..utf16_offset(&self.text, range.end)
    }

    /// The byte range an IME edit replaces: the one given, else the marked text, else the caret.
    fn edit_range(&self, range_utf16: Option<Range<usize>>) -> Range<usize> {
        range_utf16
            .map(|r| self.range_from_utf16(&r))
            .or(self.marked.clone())
            .unwrap_or(self.selected.clone())
    }

    /// Where byte `index` of the text was painted.
    fn x_for(&self, index: usize) -> Option<Pixels> {
        Some(self.last_origin?.x + self.last_line.as_ref()?.x_for_index(index))
    }
}

/// The byte offset of UTF-16 offset `utf16` in `s` (clamped to the end).
fn utf8_offset(s: &str, utf16: usize) -> usize {
    let mut count = 0;
    for (i, c) in s.char_indices() {
        if count >= utf16 {
            return i;
        }
        count += c.len_utf16();
    }
    s.len()
}

/// The UTF-16 offset of byte offset `utf8` in `s`.
fn utf16_offset(s: &str, utf8: usize) -> usize {
    s.char_indices()
        .take_while(|(i, _)| *i < utf8)
        .map(|(_, c)| c.len_utf16())
        .sum()
}

/// What an erasing key takes from `text`: the selection if there is one, else from the caret
/// (`selected`'s place) to where `to` goes from it (one character back, forward, or to the start).
fn erase(text: &str, selected: &Range<usize>, to: fn(&str, usize) -> usize) -> Range<usize> {
    if !selected.is_empty() {
        return selected.clone();
    }
    let (caret, other) = (selected.start, to(text, selected.start));
    caret.min(other)..caret.max(other)
}

/// The character boundary before byte `i` of `s` (the start stays put).
fn prev_char(s: &str, i: usize) -> usize {
    s[..i].char_indices().next_back().map_or(0, |(j, _)| j)
}

/// The character boundary after byte `i` of `s` (the end stays put).
fn next_char(s: &str, i: usize) -> usize {
    s[i..].chars().next().map_or(i, |c| i + c.len_utf8())
}

/// Part of a word, for ⌥←/⌥→.
fn in_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// ⌥←: back over spaces and punctuation, then to the start of the word before them.
fn word_start(s: &str, i: usize) -> usize {
    let before = |j: usize| s[..j].chars().next_back();
    let mut j = i;
    while before(j).is_some_and(|c| !in_word(c)) {
        j = prev_char(s, j);
    }
    while before(j).is_some_and(in_word) {
        j = prev_char(s, j);
    }
    j
}

/// ⌥→: on over spaces and punctuation, then to the end of the word after them.
fn word_end(s: &str, i: usize) -> usize {
    let after = |j: usize| s[j..].chars().next();
    let mut j = i;
    while after(j).is_some_and(|c| !in_word(c)) {
        j = next_char(s, j);
    }
    while after(j).is_some_and(in_word) {
        j = next_char(s, j);
    }
    j
}

/// A name is one line: control characters (tab, newline) are dropped.
fn printable(text: &str) -> String {
    text.chars().filter(|c| !c.is_control()).collect()
}

impl EntityInputHandler for NameInput {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range_utf16);
        actual_range.replace(self.range_to_utf16(&range));
        Some(self.text[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.range_to_utf16(&self.selected),
            reversed: self.reversed,
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked.as_ref().map(|r| self.range_to_utf16(r))
    }

    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        // The composition stays as typed; the caret after it.
        let end = self.selection().end;
        self.move_to(end, false);
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
        self.put(range, &printable(new_text));
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
        let new_text = printable(new_text);
        let start = range.start;
        self.text.replace_range(range, &new_text);
        let end = start + new_text.len();
        self.marked = (!new_text.is_empty()).then_some(start..end);
        self.reversed = false;
        // The IME's caret is relative to the marked text, in UTF-16.
        self.selected = match new_selected_range_utf16 {
            Some(r) => {
                start + utf8_offset(&new_text, r.start)..start + utf8_offset(&new_text, r.end)
            }
            None => end..end,
        };
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        element_bounds: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let range = self.range_from_utf16(&range_utf16);
        Some(Bounds::from_corners(
            point(self.x_for(range.start)?, element_bounds.top()),
            point(self.x_for(range.end)?, element_bounds.bottom()),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        self.last_bounds?.localize(&point)?;
        let x = point.x - self.last_origin?.x;
        let index = self
            .last_line
            .as_ref()?
            .index_for_x(x)
            .unwrap_or(self.text.len());
        Some(utf16_offset(&self.text, index))
    }
}

impl Render for NameInput {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ui = cx.global::<Appearance>().theme.ui();
        // Focused, the edge is the focus colour: softer than the dim text on a light theme, and
        // still a 3:1 line.
        let edge = if self.focus_handle.is_focused(window) {
            ui.focus_ring
        } else {
            ui.text_dim
        };
        div()
            .flex_1()
            .overflow_hidden()
            // Its own text colour with its own fill: the row under it may be filled with
            // anything (the selected row's accent, with its ink).
            .when(!self.bare, |d| {
                d.px_1()
                    .rounded_sm()
                    .border_1()
                    .border_color(hsla(edge))
                    .bg(hsla(ui.sidebar_background))
                    .text_color(hsla(ui.text))
            })
            .key_context("NameInput")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::clear))
            .on_action(cx.listener(Self::confirm))
            .on_action(cx.listener(Self::cancel))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::show_character_palette))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::move_left))
            .on_action(cx.listener(Self::move_right))
            .on_action(cx.listener(Self::word_left))
            .on_action(cx.listener(Self::word_right))
            .on_action(cx.listener(Self::line_start))
            .on_action(cx.listener(Self::line_end))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::select_word_left))
            .on_action(cx.listener(Self::select_word_right))
            .on_action(cx.listener(Self::select_to_start))
            .on_action(cx.listener(Self::select_to_end))
            .on_action(cx.listener(Self::select_all_action))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::mouse_down))
            .on_mouse_move(cx.listener(Self::mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::mouse_up))
            .child(NameText { input: cx.entity() })
    }
}

/// Paints the text and caret, and registers the box as the window's text input while focused.
struct NameText {
    input: Entity<NameInput>,
}

impl IntoElement for NameText {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for NameText {
    type RequestLayoutState = ();
    type PrepaintState = Option<ShapedLine>;

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
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = window.line_height().into();
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
        let run = TextRun {
            len: input.text.len(),
            font: style.font(),
            color: style.color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        // The IME's marked text is underlined, as macOS expects.
        let runs: Vec<TextRun> = match input.marked.clone() {
            Some(marked) => [
                TextRun {
                    len: marked.start,
                    ..run.clone()
                },
                TextRun {
                    len: marked.len(),
                    underline: Some(UnderlineStyle {
                        color: Some(run.color),
                        thickness: px(1.0),
                        wavy: false,
                    }),
                    ..run.clone()
                },
                TextRun {
                    len: input.text.len() - marked.end,
                    ..run
                },
            ]
            .into_iter()
            .filter(|r| r.len > 0)
            .collect(),
            None => vec![run],
        };
        let font_size = style.font_size.to_pixels(window.rem_size());
        Some(
            window
                .text_system()
                .shape_line(input.text.clone().into(), font_size, &runs, None),
        )
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        line: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let Some(line) = line.take() else { return };
        let (focus_handle, caret_index, selection) = {
            let input = self.input.read(cx);
            (input.focus_handle.clone(), input.caret(), input.selection())
        };
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.input.clone()),
            cx,
        );
        // A name wider than the box scrolls left so the caret stays visible (at the end: the
        // end; nearer the start, less).
        let caret_width = px(2.);
        let overflow = (line.width() + caret_width - bounds.size.width).max(px(0.));
        let to_caret = line.x_for_index(caret_index) + caret_width - bounds.size.width;
        let origin = point(
            bounds.left() - to_caret.clamp(px(0.), overflow),
            bounds.top(),
        );
        let line_height = window.line_height();
        // A selection is shaded behind the text.
        if !selection.is_empty() {
            let left = origin.x + line.x_for_index(selection.start);
            let right = origin.x + line.x_for_index(selection.end);
            let shade = cx.global::<Appearance>().theme.ui().selection;
            window.paint_quad(fill(
                Bounds::from_corners(point(left, bounds.top()), point(right, bounds.bottom())),
                hsla(shade),
            ));
        }
        line.paint(origin, line_height, TextAlign::Left, None, window, cx)
            .ok();
        if focus_handle.is_focused(window) {
            let x = origin.x + line.x_for_index(caret_index);
            let color = window.text_style().color;
            window.paint_quad(fill(
                Bounds::new(
                    point(x, bounds.top()),
                    size(caret_width, bounds.size.height),
                ),
                color,
            ));
        }
        self.input.update(cx, |input, _| {
            input.last_line = Some(line);
            input.last_origin = Some(origin);
            input.last_bounds = Some(bounds);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::{erase, next_char, prev_char, word_end, word_start};

    #[test]
    fn backspace_erases_the_selection_or_the_character_before_the_caret() {
        let text = "localhost:3000";
        assert_eq!(erase(text, &(14..14), prev_char), 13..14, "at the end");
        assert_eq!(
            erase(text, &(5..5), prev_char),
            4..5,
            "mid-name: the one before it"
        );
        assert_eq!(erase(text, &(0..13), prev_char), 0..13, "the selection");
        assert_eq!(
            erase(text, &(0..0), prev_char),
            0..0,
            "nothing before the caret"
        );
        assert_eq!(erase("héé", &(5..5), prev_char), 3..5, "a whole character");
    }

    #[test]
    fn delete_erases_after_the_caret_and_cmd_backspace_everything_before_it() {
        let text = "api server";
        assert_eq!(erase(text, &(3..3), next_char), 3..4);
        assert_eq!(
            erase(text, &(10..10), next_char),
            10..10,
            "nothing after the end"
        );
        assert_eq!(erase(text, &(4..4), |_, _| 0), 0..4);
        assert_eq!(erase(text, &(1..6), |_, _| 0), 1..6, "the selection");
    }

    #[test]
    fn the_caret_moves_by_whole_characters() {
        let text = "héé";
        assert_eq!(next_char(text, 0), 1);
        assert_eq!(next_char(text, 1), 3);
        assert_eq!(next_char(text, 5), 5, "the end stays");
        assert_eq!(prev_char(text, 5), 3);
        assert_eq!(prev_char(text, 0), 0, "the start stays");
    }

    #[test]
    fn option_arrows_move_by_words() {
        let text = "my-api  server_2";
        assert_eq!(word_start(text, text.len()), 8, "back to server_2");
        assert_eq!(word_start(text, 8), 3, "over the spaces to api");
        assert_eq!(word_start(text, 3), 0);
        assert_eq!(word_end(text, 0), 2, "my");
        assert_eq!(word_end(text, 2), 6, "over - to the end of api");
        assert_eq!(word_end(text, 6), text.len());
    }
}
