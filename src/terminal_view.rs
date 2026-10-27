//! The GPUI view for one terminal: paints a `GridSnapshot`, turns keystrokes into PTY bytes,
//! tracks the agent status, and reports what happened to whoever owns it (the workspace).

use std::cell::Cell;
use std::path::Path;
use std::rc::Rc;
use std::time::{Duration, Instant};

use alacritty_terminal::event::Event;
use alacritty_terminal::grid::Scroll;
use alacritty_terminal::index::{Point, Side};
use alacritty_terminal::selection::SelectionType;
use alacritty_terminal::term::TermMode;
use alacritty_terminal::vte::ansi::{CursorShape, Rgb};
use futures::StreamExt;
use futures::channel::mpsc::UnboundedReceiver;
use gpui::{
    App, Bounds, ClipboardItem, Context, CursorStyle, DispatchPhase, Entity, EventEmitter,
    FocusHandle, Focusable, Font, FontStyle, FontWeight, HitboxBehavior, Hsla, KeyDownEvent,
    Modifiers, ModifiersChangedEvent, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    Pixels, Rgba, ScrollWheelEvent, SharedString, Subscription, Task, TextAlign, TextRun,
    TouchPhase, UnderlineStyle, Window, actions, canvas, div, fill, point, prelude::*, px, size,
};

use crate::appearance::Appearance;
use crate::grid::{self, GridSnapshot};
use crate::keys;
use crate::links::Link;
use crate::mouse::{self, Button, Mods, MouseAction, ScrollAccumulator};
use crate::pty;
use crate::status::{Status, StatusTracker, TAIL_LINES};
use crate::terminal::{CellPx, TermDims, TermEvent, TerminalCore};
use crate::theme::Theme;

// Bound in main.rs with the "Terminal" key context. GPUI runs key bindings before `on_key_down`,
// so these win over the byte mapping.
actions!(terminal, [Copy, Paste, ScrollPageUp, ScrollPageDown]);

/// GPUI's default line height (1.618 x size) is too airy for a terminal.
const LINE_HEIGHT_FACTOR: f32 = 1.3;
/// How often to run housekeeping (flush stuck synchronized updates, re-evaluate status).
const TICK: Duration = Duration::from_millis(50);

/// What a terminal tells its owner.
pub enum TerminalEvent {
    /// The detected agent status changed (M2 heuristics).
    StatusChanged(Status),
    /// The program set (Some) or reset (None) the title via OSC 0/2.
    TitleChanged(Option<SharedString>),
    /// The shell exited; the session should close.
    Exited,
}

/// Where the grid was last painted, for turning mouse positions into grid points.
#[derive(Clone, Copy)]
struct GridLayout {
    origin: gpui::Point<Pixels>,
    cell_width: Pixels,
    line_height: Pixels,
    dims: TermDims,
}

pub struct TerminalView {
    core: TerminalCore,
    focus_handle: FocusHandle,
    tracker: StatusTracker,
    /// Written by the canvas prepaint; `None` until the first paint (mouse events are ignored).
    layout: Rc<Cell<Option<GridLayout>>>,
    /// Fractional wheel lines carried between scroll events.
    scroll: ScrollAccumulator,
    /// The link under the pointer while cmd is held (underlined, hand cursor).
    hovered: Option<Link>,
    /// The link a cmd-click went down on; opened if the button comes up on the same link.
    pressed_link: Option<Link>,
    /// A left press started a local selection; drags extend it until the button comes up.
    selecting: bool,
    /// A press was reported to the program, so drags and the release are reported too.
    mouse_reported: bool,
    /// Dropping a GPUI `Task` cancels it, so these live as long as the view.
    _events: Task<()>,
    _ticker: Task<()>,
    /// Follows the appearance global so a theme change repaints this session.
    _appearance: Subscription,
}

impl EventEmitter<TerminalEvent> for TerminalView {}

impl Focusable for TerminalView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl TerminalView {
    /// Start a login shell (in `cwd`, else home) and wrap it in a new view entity (focused).
    /// `startup` is typed into the shell right after it starts, with Enter.
    pub fn spawn(
        window: &mut Window,
        cx: &mut App,
        cwd: Option<&Path>,
        startup: Option<&str>,
    ) -> Result<Entity<Self>, String> {
        let (tx, rx) = futures::channel::mpsc::unbounded();
        // 80x24 until the first paint measures the real size and resizes.
        let theme = cx.global::<Appearance>().theme;
        let core = TerminalCore::spawn(
            pty::shell_command(cwd),
            TermDims { cols: 80, rows: 24 },
            tx,
            theme,
        )?;
        if let Some(cmd) = startup {
            // The shell reads this as typed-ahead input once it is up.
            core.write(format!("{cmd}\r").as_bytes());
        }
        Ok(cx.new(|cx| Self::new(core, rx, window, cx)))
    }

    fn new(
        core: TerminalCore,
        mut rx: UnboundedReceiver<TermEvent>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        // Drain terminal events on the UI thread, in order.
        let events = cx.spawn(async move |this, cx| {
            while let Some(event) = rx.next().await {
                if this
                    .update(cx, |this, cx| this.on_term_event(event, cx))
                    .is_err()
                {
                    break; // view was dropped
                }
            }
        });

        let ticker = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(TICK).await;
                if this.update(cx, |this, cx| this.on_tick(cx)).is_err() {
                    break;
                }
            }
        });

        let appearance = cx.observe_global::<Appearance>(|this, cx| {
            let theme = cx.global::<Appearance>().theme;
            this.core.set_theme(theme);
            cx.notify();
        });

        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);
        Self {
            core,
            focus_handle,
            tracker: StatusTracker::new(),
            layout: Rc::new(Cell::new(None)),
            scroll: ScrollAccumulator::default(),
            hovered: None,
            pressed_link: None,
            selecting: false,
            mouse_reported: false,
            _events: events,
            _ticker: ticker,
            _appearance: appearance,
        }
    }

    fn on_term_event(&mut self, event: TermEvent, cx: &mut Context<Self>) {
        match event {
            TermEvent::Output { notifications } => {
                if let Some(status) = self.tracker.on_output(Instant::now(), notifications) {
                    self.status_changed(status, cx);
                }
                // Link spans are absolute grid lines that shift as output scrolls; the next
                // mouse move recomputes the hover.
                self.hovered = None;
                cx.notify();
            }
            TermEvent::Term(Event::Wakeup) => cx.notify(),
            // A real BEL (not an OSC terminator). Must come before the `TermEvent::Term(_)` catch-all.
            TermEvent::Term(Event::Bell) => self.tracker.on_bell(Instant::now()),
            // Replies to terminal queries (cursor position, device attributes, ...) MUST go back
            // to the child, or programs that ask (shells, vim, Claude Code) hang waiting.
            TermEvent::Term(Event::PtyWrite(text)) => self.core.write(text.as_bytes()),
            TermEvent::Term(Event::ColorRequest(index, format)) => {
                let reply = format(self.core.color(index));
                self.core.write(reply.as_bytes());
            }
            TermEvent::Term(Event::TextAreaSizeRequest(format)) => {
                let reply = format(self.core.window_size());
                self.core.write(reply.as_bytes());
            }
            TermEvent::Term(Event::Title(title)) => {
                cx.emit(TerminalEvent::TitleChanged(Some(title.into())))
            }
            TermEvent::Term(Event::ResetTitle) => cx.emit(TerminalEvent::TitleChanged(None)),
            TermEvent::Exited => cx.emit(TerminalEvent::Exited),
            TermEvent::Term(_) => {}
        }
    }

    fn on_tick(&mut self, cx: &mut Context<Self>) {
        if self.core.flush_stale_sync() {
            cx.notify();
        }
        let core = &self.core;
        if let Some(status) = self
            .tracker
            .tick(Instant::now(), || core.tail_text(TAIL_LINES))
        {
            self.status_changed(status, cx);
        }
    }

    fn status_changed(&self, status: Status, cx: &mut Context<Self>) {
        eprintln!("[fuxx] status -> {status:?}"); // debug log, used by the M2 manual check
        cx.emit(TerminalEvent::StatusChanged(status));
        cx.notify();
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;
        let m = &keystroke.modifiers;
        if let Some(bytes) =
            keys::mac_shortcut(&keystroke.key, m.platform, m.alt, m.control, m.shift)
        {
            self.send_key_bytes(bytes, cx);
            return;
        }
        if m.platform {
            return; // cmd-shortcuts belong to the app (cmd-q, cmd-t, ...)
        }
        // Option on a *character* key already composed a character into key_char (option-s = ß);
        // only named keys (arrows, backspace, enter, ...) get Alt-as-Meta treatment.
        let named_key = keystroke.key.chars().count() > 1;
        let app_cursor = self.core.mode().contains(TermMode::APP_CURSOR);
        if let Some(bytes) = keys::key_to_bytes(
            &keystroke.key,
            keystroke.key_char.as_deref(),
            m.control,
            m.alt && named_key,
            m.shift,
            app_cursor,
        ) {
            self.send_key_bytes(&bytes, cx);
        }
    }

    /// What a keystroke does once it has bytes: write to the pty, tell the tracker, swallow the event.
    fn send_key_bytes(&mut self, bytes: &[u8], cx: &mut Context<Self>) {
        self.core.input(bytes);
        if let Some(status) = self.tracker.on_input(Instant::now()) {
            self.status_changed(status, cx);
        }
        cx.stop_propagation();
    }

    /// Grid point, cell side and viewport col/row under a window position, or `None` before the
    /// first paint. Always in range: `grid_point` clamps to the painted dims and the live offset.
    fn point_at(&self, pos: gpui::Point<Pixels>) -> Option<(Point, Side, usize, usize)> {
        let layout = self.layout.get()?;
        let display_offset = self.core.display_offset();
        let rel = pos - layout.origin;
        let (point, side) = mouse::grid_point(
            f32::from(rel.x),
            f32::from(rel.y),
            f32::from(layout.cell_width),
            f32::from(layout.line_height),
            layout.dims,
            display_offset,
        );
        let row = (point.line.0 + display_offset as i32) as usize;
        Some((point, side, point.column.0, row))
    }

    fn on_scroll_wheel(
        &mut self,
        e: &ScrollWheelEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(layout) = self.layout.get() else {
            return;
        };
        let line_height = f32::from(layout.line_height);
        if line_height <= 0.0 {
            return;
        }
        if e.touch_phase == TouchPhase::Started {
            self.scroll.reset();
        }
        let delta = f32::from(e.delta.pixel_delta(layout.line_height).y);
        let lines = self.scroll.push(delta, line_height);
        if lines == 0 {
            return;
        }
        let mode = self.core.mode();
        let shift = e.modifiers.shift;
        if mouse::wants_mouse(mode, shift) {
            let Some((_, _, col, row)) = self.point_at(e.position) else {
                return;
            };
            let wheel = if lines > 0 {
                Button::WheelUp
            } else {
                Button::WheelDown
            };
            for _ in 0..lines.unsigned_abs() {
                if let Some(bytes) = mouse::report(
                    wheel,
                    MouseAction::Press,
                    mods(&e.modifiers),
                    col,
                    row,
                    mode,
                ) {
                    self.core.write(&bytes);
                }
            }
        } else if mode.contains(TermMode::ALT_SCREEN | TermMode::ALTERNATE_SCROLL) && !shift {
            self.core.write(&mouse::alt_scroll(lines));
        } else {
            self.core.scroll(Scroll::Delta(lines));
        }
        cx.notify();
    }

    fn on_mouse_down(&mut self, e: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.focus_handle.focus(window, cx);
        let Some((point, side, col, row)) = self.point_at(e.position) else {
            return;
        };
        let m = &e.modifiers;
        if e.button == MouseButton::Left && m.platform {
            self.pressed_link = self.core.link_at(point);
            if self.pressed_link.is_some() {
                return;
            }
        }
        let mode = self.core.mode();
        if mouse::wants_mouse(mode, m.shift) {
            if let Some(button) = button(e.button)
                && let Some(bytes) =
                    mouse::report(button, MouseAction::Press, mods(m), col, row, mode)
            {
                self.core.write(&bytes);
                self.mouse_reported = true;
            }
        } else if e.button == MouseButton::Left {
            // With mouse reporting on, shift is the "select locally" bypass, so a shift-press
            // starts a selection there instead of extending one (there's usually none to extend).
            if m.shift && !mode.intersects(TermMode::MOUSE_MODE) {
                self.core.extend_selection(point, side);
            } else {
                let ty = match e.click_count {
                    1 => SelectionType::Simple,
                    2 => SelectionType::Semantic,
                    _ => SelectionType::Lines,
                };
                // A plain click leaves an empty Simple selection, which selects nothing.
                self.core.start_selection(ty, point, side);
            }
            self.selecting = true;
        }
        cx.notify();
    }

    /// Window-wide (registered in paint), so a drag that leaves the pane keeps selecting.
    fn on_mouse_move(&mut self, e: &MouseMoveEvent, over_pane: bool, cx: &mut Context<Self>) {
        let Some((point, side, col, row)) = self.point_at(e.position) else {
            return;
        };
        if self.selecting && e.pressed_button == Some(MouseButton::Left) {
            self.core.extend_selection(point, side);
            cx.notify();
        } else if self.mouse_reported {
            let mode = self.core.mode();
            if mouse::wants_mouse(mode, e.modifiers.shift)
                && let Some(button) = e.pressed_button.and_then(button)
                && let Some(bytes) = mouse::report(
                    button,
                    MouseAction::Drag,
                    mods(&e.modifiers),
                    col,
                    row,
                    mode,
                )
            {
                self.core.write(&bytes);
            }
        }
        let hovered = if e.modifiers.platform && over_pane {
            self.core.link_at(point)
        } else {
            None
        };
        if hovered != self.hovered {
            self.hovered = hovered;
            cx.notify();
        }
    }

    /// Window-wide (registered in paint), so a release outside the pane still ends a drag.
    fn on_mouse_up(&mut self, e: &MouseUpEvent, over_pane: bool, cx: &mut Context<Self>) {
        let at = self.point_at(e.position);
        if let Some(link) = self.pressed_link.take()
            && over_pane
            && let Some((point, ..)) = at
            // Compare by url: a link's grid lines shift when new output scrolls the screen.
            && self.core.link_at(point).is_some_and(|under| under.url == link.url)
        {
            // `link_at` only returns http/https/mailto. Opened by the OS, never through a shell.
            cx.open_url(&link.url);
        }
        if self.mouse_reported {
            self.mouse_reported = false;
            if let Some((_, _, col, row)) = at
                && let Some(button) = button(e.button)
                && let Some(bytes) = mouse::report(
                    button,
                    MouseAction::Release,
                    mods(&e.modifiers),
                    col,
                    row,
                    self.core.mode(),
                )
            {
                self.core.write(&bytes);
            }
        }
        self.selecting = false;
    }

    fn on_modifiers_changed(
        &mut self,
        e: &ModifiersChangedEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !e.modifiers.platform && self.hovered.is_some() {
            self.hovered = None;
            cx.notify();
        }
    }

    /// cmd-c: copy the selection; it stays selected.
    fn copy(&mut self, _: &Copy, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = self.core.selection_text() {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
    }

    /// cmd-v: paste as user input (snaps to the bottom, clears the selection, counts as typing).
    fn paste(&mut self, _: &Paste, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return;
        };
        let bracketed = self.core.mode().contains(TermMode::BRACKETED_PASTE);
        self.core.input(&keys::paste_bytes(&text, bracketed));
        if let Some(status) = self.tracker.on_input(Instant::now()) {
            self.status_changed(status, cx);
        }
        cx.notify();
    }

    fn scroll_page_up(&mut self, _: &ScrollPageUp, _window: &mut Window, cx: &mut Context<Self>) {
        self.scroll_page(Scroll::PageUp, cx);
    }

    fn scroll_page_down(
        &mut self,
        _: &ScrollPageDown,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.scroll_page(Scroll::PageDown, cx);
    }

    /// Full-screen programs have no scrollback: let the key through to them instead.
    fn scroll_page(&mut self, scroll: Scroll, cx: &mut Context<Self>) {
        if self.core.mode().contains(TermMode::ALT_SCREEN) {
            cx.propagate();
            return;
        }
        self.core.scroll(scroll);
        cx.notify();
    }
}

fn mods(m: &Modifiers) -> Mods {
    Mods {
        shift: m.shift,
        alt: m.alt,
        ctrl: m.control,
    }
}

fn button(b: MouseButton) -> Option<Button> {
    match b {
        MouseButton::Left => Some(Button::Left),
        MouseButton::Middle => Some(Button::Middle),
        MouseButton::Right => Some(Button::Right),
        _ => None,
    }
}

impl Render for TerminalView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let focused = self.focus_handle.is_focused(window);
        let resize_core = self.core.clone();
        let paint_core = self.core.clone();
        let theme = self.core.theme();
        let layout = self.layout.clone();
        let hovered = self.hovered.clone();
        let view = cx.entity();

        div()
            .size_full()
            .track_focus(&self.focus_handle)
            .key_context("Terminal")
            .on_key_down(cx.listener(Self::on_key_down))
            .on_scroll_wheel(cx.listener(Self::on_scroll_wheel))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_down(MouseButton::Middle, cx.listener(Self::on_mouse_down))
            .on_mouse_down(MouseButton::Right, cx.listener(Self::on_mouse_down))
            .on_modifiers_changed(cx.listener(Self::on_modifiers_changed))
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::scroll_page_up))
            .on_action(cx.listener(Self::scroll_page_down))
            .child(
                canvas(
                    // prepaint: measure cells, fit the terminal to the space we were given,
                    // remember the layout for hit-testing.
                    move |bounds, window, cx| {
                        let metrics = CellMetrics::measure(cx.global::<Appearance>(), window);
                        resize_core.resize(
                            metrics.dims_for(bounds),
                            CellPx {
                                width: f32::from(metrics.cell_width).round() as u16,
                                height: f32::from(metrics.line_height).round() as u16,
                            },
                        );
                        layout.set(Some(GridLayout {
                            origin: bounds.origin,
                            cell_width: metrics.cell_width,
                            line_height: metrics.line_height,
                            dims: resize_core.dims(),
                        }));
                        let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
                        (metrics, hitbox)
                    },
                    // paint: draw the current screen, set the pointer, follow the mouse.
                    move |bounds, (metrics, hitbox), window, cx| {
                        let mut snap = paint_core.snapshot();
                        if let Some(link) = &hovered {
                            let offset = paint_core.display_offset();
                            grid::underline_cells(&mut snap, link.start, link.end, offset);
                        }
                        paint_grid(snap, &theme, &metrics, bounds, focused, window, cx);

                        let style = if hovered.is_some() && window.modifiers().platform {
                            CursorStyle::PointingHand
                        } else {
                            CursorStyle::IBeam
                        };
                        window.set_cursor_style(style, &hitbox);

                        // Move/up anywhere in the window, so a drag that leaves the pane keeps
                        // going. Paint-phase registrations last for one frame.
                        window.on_mouse_event({
                            let view = view.clone();
                            let hitbox = hitbox.clone();
                            move |e: &MouseMoveEvent, phase, window, cx| {
                                if phase == DispatchPhase::Bubble {
                                    let over_pane = hitbox.is_hovered(window);
                                    view.update(cx, |this, cx| {
                                        this.on_mouse_move(e, over_pane, cx)
                                    });
                                }
                            }
                        });
                        window.on_mouse_event(move |e: &MouseUpEvent, phase, window, cx| {
                            if phase == DispatchPhase::Bubble {
                                let over_pane = hitbox.is_hovered(window);
                                view.update(cx, |this, cx| this.on_mouse_up(e, over_pane, cx));
                            }
                        });
                    },
                )
                .size_full(),
            )
    }
}

/// Monospace cell geometry for the terminal font.
struct CellMetrics {
    font: Font,
    font_size: Pixels,
    cell_width: Pixels,
    line_height: Pixels,
}

impl CellMetrics {
    /// The configured font and size; bold/italic runs derive from `font`, keeping its fallbacks
    /// and features.
    fn measure(appearance: &Appearance, window: &Window) -> Self {
        let font = appearance.font.clone();
        let font_size = px(appearance.font_size);
        let text_system = window.text_system();
        let font_id = text_system.resolve_font(&font);
        let cell_width = text_system
            .em_advance(font_id, font_size)
            .unwrap_or(font_size * 0.6)
            .max(px(1.0));
        let line_height = (font_size * LINE_HEIGHT_FACTOR).round().max(px(1.0));
        Self {
            font,
            font_size,
            cell_width,
            line_height,
        }
    }

    fn dims_for(&self, bounds: Bounds<Pixels>) -> TermDims {
        TermDims {
            cols: ((bounds.size.width / self.cell_width).floor() as usize).max(2),
            rows: ((bounds.size.height / self.line_height).floor() as usize).max(1),
        }
    }
}

pub fn hsla(c: Rgb) -> Hsla {
    Rgba {
        r: c.r as f32 / 255.0,
        g: c.g as f32 / 255.0,
        b: c.b as f32 / 255.0,
        a: 1.0,
    }
    .into()
}

fn paint_grid(
    mut snap: GridSnapshot,
    theme: &Theme,
    m: &CellMetrics,
    bounds: Bounds<Pixels>,
    focused: bool,
    window: &mut Window,
    cx: &mut App,
) {
    window.paint_quad(fill(bounds, hsla(theme.background)));

    // A focused block cursor is drawn by inverting its cell, so the glyph stays readable.
    let mut outline_cursor = None;
    if let Some(c) = snap.cursor {
        if c.row < snap.rows && c.col < snap.cols {
            if focused && c.shape == CursorShape::Block {
                let cell = &mut snap.lines[c.row][c.col];
                cell.fg = theme.cursor_text.unwrap_or(theme.background);
                cell.bg = theme.cursor.unwrap_or(theme.foreground);
            } else {
                outline_cursor = Some(c);
            }
        }
    }

    let cell_origin = |row: usize, col: usize| {
        point(
            bounds.origin.x + m.cell_width * col as f32,
            bounds.origin.y + m.line_height * row as f32,
        )
    };

    for (row, line) in snap.lines.iter().enumerate() {
        for rect in grid::bg_rects(line, theme.background) {
            let size = size(m.cell_width * rect.cells as f32, m.line_height);
            window.paint_quad(fill(
                Bounds::new(cell_origin(row, rect.col), size),
                hsla(rect.color),
            ));
        }
        for span in grid::text_spans(line) {
            let color = hsla(span.fg);
            let run = TextRun {
                len: span.text.len(),
                font: Font {
                    weight: if span.bold {
                        FontWeight::BOLD
                    } else {
                        FontWeight::NORMAL
                    },
                    style: if span.italic {
                        FontStyle::Italic
                    } else {
                        FontStyle::Normal
                    },
                    ..m.font.clone()
                },
                color,
                background_color: None,
                underline: span.underline.then(|| UnderlineStyle {
                    color: Some(color),
                    thickness: px(1.0),
                    wavy: false,
                }),
                strikethrough: None,
            };
            // force_width = one cell per glyph keeps the text locked to the grid.
            let shaped = window.text_system().shape_line(
                SharedString::from(span.text),
                m.font_size,
                &[run],
                Some(m.cell_width),
            );
            let _ = shaped.paint(
                cell_origin(row, span.col),
                m.line_height,
                TextAlign::Left,
                None,
                window,
                cx,
            );
        }
    }

    // Unfocused block = hollow box; beam = thin bar; underline = thin bottom bar.
    if let Some(c) = outline_cursor {
        let origin = cell_origin(c.row, c.col);
        let color = hsla(theme.cursor.unwrap_or(theme.foreground));
        let full = Bounds::new(origin, size(m.cell_width, m.line_height));
        match c.shape {
            CursorShape::Beam => window.paint_quad(fill(
                Bounds::new(origin, size(px(2.0), m.line_height)),
                color,
            )),
            CursorShape::Underline => window.paint_quad(fill(
                Bounds::new(
                    point(origin.x, origin.y + m.line_height - px(2.0)),
                    size(m.cell_width, px(2.0)),
                ),
                color,
            )),
            _ => window.paint_quad(gpui::outline(full, color, gpui::BorderStyle::Solid)),
        }
    }
}
