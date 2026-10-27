//! The inline rename box in the sidebar: one line of text with the caret always at the end.
//! Typed text, ⌥ accents, the emoji picker and IME composition all arrive through
//! `EntityInputHandler` (macOS speaks UTF-16 offsets; the text is kept as UTF-8).

use std::ops::Range;

use gpui::{
    App, Bounds, Context, ElementId, ElementInputHandler, Entity, EntityInputHandler, EventEmitter,
    FocusHandle, Focusable, GlobalElementId, InspectorElementId, LayoutId, MouseButton,
    MouseDownEvent, Pixels, Point, ShapedLine, Style, Subscription, TextAlign, TextRun,
    UTF16Selection, UnderlineStyle, Window, actions, div, fill, point, prelude::*, px, relative,
    size,
};

use crate::appearance::Appearance;
use crate::terminal_view::hsla;

// Bound in main.rs with the "NameInput" key context, so these keys never reach the terminal.
actions!(
    name_input,
    [Backspace, Clear, Confirm, Cancel, ShowCharacterPalette]
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
    /// The end of `text`, except while the IME places its own caret inside the marked text.
    selected: Range<usize>,
    /// From the last paint, for the IME's candidate window and the caret.
    last_line: Option<ShapedLine>,
    last_origin: Option<Point<Pixels>>,
    last_bounds: Option<Bounds<Pixels>>,
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
            last_line: None,
            last_origin: None,
            last_bounds: None,
            _blur: blur,
        }
    }

    fn caret_to_end(&mut self) {
        self.marked = None;
        self.selected = self.text.len()..self.text.len();
    }

    fn backspace(&mut self, _: &Backspace, _: &mut Window, cx: &mut Context<Self>) {
        self.text.pop();
        self.caret_to_end();
        cx.notify();
    }

    fn clear(&mut self, _: &Clear, _: &mut Window, cx: &mut Context<Self>) {
        self.text.clear();
        self.caret_to_end();
        cx.notify();
    }

    fn confirm(&mut self, _: &Confirm, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(NameInputEvent::Confirm(self.text.trim().to_string()));
    }

    fn cancel(&mut self, _: &Cancel, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(NameInputEvent::Cancel);
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
            reversed: false,
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked.as_ref().map(|r| self.range_to_utf16(r))
    }

    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.caret_to_end();
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
        self.text.replace_range(range, &printable(new_text));
        self.caret_to_end();
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
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ui = cx.global::<Appearance>().theme.ui();
        div()
            .flex_1()
            .overflow_hidden()
            .px_1()
            .rounded_sm()
            .border_1()
            .border_color(hsla(ui.text_dim))
            .bg(hsla(ui.sidebar_background))
            .key_context("NameInput")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::clear))
            .on_action(cx.listener(Self::confirm))
            .on_action(cx.listener(Self::cancel))
            .on_action(cx.listener(Self::show_character_palette))
            // Keep clicks in the box: the sidebar row under it would take focus (and so cancel).
            .on_mouse_down(MouseButton::Left, |_: &MouseDownEvent, _, cx| {
                cx.stop_propagation()
            })
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
        let (focus_handle, caret_index) = {
            let input = self.input.read(cx);
            (input.focus_handle.clone(), input.selected.end)
        };
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.input.clone()),
            cx,
        );
        // A name wider than the box scrolls left so its end (and the caret) stays visible.
        let caret_width = px(2.);
        let overflow = (line.width() + caret_width - bounds.size.width).max(px(0.));
        let origin = point(bounds.left() - overflow, bounds.top());
        let line_height = window.line_height();
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
