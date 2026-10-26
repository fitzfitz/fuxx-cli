//! The GPUI view for one terminal: paints a `GridSnapshot`, turns keystrokes into PTY bytes,
//! tracks the agent status, and reports what happened to whoever owns it (the workspace).

use std::time::{Duration, Instant};

use alacritty_terminal::event::{Event, WindowSize};
use alacritty_terminal::term::TermMode;
use alacritty_terminal::vte::ansi::{CursorShape, Rgb};
use futures::StreamExt;
use futures::channel::mpsc::UnboundedReceiver;
use gpui::{
    App, Bounds, Context, Entity, EventEmitter, FocusHandle, Focusable, Font, FontStyle,
    FontWeight, Hsla, KeyDownEvent, Pixels, Rgba, SharedString, Task, TextAlign, TextRun,
    UnderlineStyle, Window, canvas, div, fill, font, point, prelude::*, px, size,
};

use crate::colors;
use crate::grid::{self, GridSnapshot};
use crate::keys;
use crate::pty;
use crate::status::{Status, StatusTracker, TAIL_LINES};
use crate::terminal::{TermDims, TermEvent, TerminalCore};

const FONT_FAMILY: &str = "Menlo";
const FONT_SIZE: f32 = 14.0;
/// GPUI's default line height (1.618 x size) is too airy for a terminal.
const LINE_HEIGHT_FACTOR: f32 = 1.3;
/// How often to run housekeeping (flush stuck synchronized updates, re-evaluate status).
const TICK: Duration = Duration::from_millis(50);

/// What a terminal tells its owner.
pub enum TerminalEvent {
    /// The program set (Some) or reset (None) the title via OSC 0/2.
    TitleChanged(Option<SharedString>),
    /// The shell exited; the session should close.
    Exited,
}

pub struct TerminalView {
    core: TerminalCore,
    focus_handle: FocusHandle,
    tracker: StatusTracker,
    /// Dropping a GPUI `Task` cancels it, so these live as long as the view.
    _events: Task<()>,
    _ticker: Task<()>,
}

impl EventEmitter<TerminalEvent> for TerminalView {}

impl Focusable for TerminalView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl TerminalView {
    /// Start a login shell and wrap it in a new view entity (focused).
    pub fn spawn(window: &mut Window, cx: &mut App) -> Result<Entity<Self>, String> {
        let (tx, rx) = futures::channel::mpsc::unbounded();
        // 80x24 until the first paint measures the real size and resizes.
        let core = TerminalCore::spawn(
            pty::shell_command(None),
            TermDims { cols: 80, rows: 24 },
            tx,
        )?;
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

        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);
        Self {
            core,
            focus_handle,
            tracker: StatusTracker::new(),
            _events: events,
            _ticker: ticker,
        }
    }

    fn on_term_event(&mut self, event: TermEvent, cx: &mut Context<Self>) {
        match event {
            TermEvent::Output { notifications } => {
                self.tracker.on_output(Instant::now(), notifications);
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
                let dims = self.core.dims();
                // Pixel cell size isn't tracked; cell counts are what programs actually use.
                let reply = format(WindowSize {
                    num_lines: dims.rows as u16,
                    num_cols: dims.cols as u16,
                    cell_width: 0,
                    cell_height: 0,
                });
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
            .tick(Instant::now(), || core.snapshot().tail_text(TAIL_LINES))
        {
            self.status_changed(status, cx);
        }
    }

    fn status_changed(&self, status: Status, cx: &mut Context<Self>) {
        eprintln!("[fuxx] status -> {status:?}"); // debug log, used by the M2 manual check
        cx.notify();
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;
        let m = &keystroke.modifiers;
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
            self.core.write(&bytes);
            if let Some(status) = self.tracker.on_input(Instant::now()) {
                self.status_changed(status, cx);
            }
            cx.stop_propagation();
        }
    }
}

impl Render for TerminalView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let focused = self.focus_handle.is_focused(window);
        let resize_core = self.core.clone();
        let paint_core = self.core.clone();

        div()
            .size_full()
            .track_focus(&self.focus_handle)
            .key_context("Terminal")
            .on_key_down(cx.listener(Self::on_key_down))
            .child(
                canvas(
                    // prepaint: measure cells, fit the terminal to the space we were given.
                    move |bounds, window, _cx| {
                        let metrics = CellMetrics::measure(window);
                        resize_core.resize(metrics.dims_for(bounds));
                        metrics
                    },
                    // paint: draw the current screen.
                    move |bounds, metrics, window, cx| {
                        paint_grid(paint_core.snapshot(), &metrics, bounds, focused, window, cx);
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
    fn measure(window: &Window) -> Self {
        let font = font(FONT_FAMILY);
        let font_size = px(FONT_SIZE);
        let text_system = window.text_system();
        let font_id = text_system.resolve_font(&font);
        let cell_width = text_system
            .em_advance(font_id, font_size)
            .unwrap_or(px(FONT_SIZE * 0.6));
        let line_height = (font_size * LINE_HEIGHT_FACTOR).round();
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
    m: &CellMetrics,
    bounds: Bounds<Pixels>,
    focused: bool,
    window: &mut Window,
    cx: &mut App,
) {
    window.paint_quad(fill(bounds, hsla(colors::BACKGROUND)));

    // A focused block cursor is drawn by inverting its cell, so the glyph stays readable.
    let mut outline_cursor = None;
    if let Some(c) = snap.cursor {
        if c.row < snap.rows && c.col < snap.cols {
            if focused && c.shape == CursorShape::Block {
                let cell = &mut snap.lines[c.row][c.col];
                cell.fg = colors::BACKGROUND;
                cell.bg = colors::FOREGROUND;
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
        for rect in grid::bg_rects(line, colors::BACKGROUND) {
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
        let color = hsla(colors::FOREGROUND);
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
