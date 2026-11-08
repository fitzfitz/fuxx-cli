//! The GPUI view for one terminal: paints a `GridSnapshot`, turns keystrokes into PTY bytes,
//! tracks the agent status, and reports what happened to whoever owns it (the workspace).
//!
//! On the Daemon backend (M10a) the daemon tracks the status and answers terminal queries, so
//! the view takes `Status` from the backend and runs no tracker of its own; its owner sees the
//! same `TerminalEvent`s either way.

use std::cell::{Cell, RefCell};
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use alacritty_terminal::event::Event;
use alacritty_terminal::grid::Scroll;
use alacritty_terminal::index::{Point, Side};
use alacritty_terminal::selection::SelectionType;
use alacritty_terminal::term::TermMode;
use alacritty_terminal::vte::ansi::{self, Rgb};
use futures::StreamExt;
use futures::channel::mpsc::UnboundedReceiver;
use gpui::{
    App, Bounds, BoxShadow, ClipboardItem, Context, Corners, CursorStyle, DispatchPhase, Entity,
    EventEmitter, FocusHandle, Focusable, Font, FontStyle, FontWeight, HitboxBehavior, Hsla,
    KeyDownEvent, Modifiers, ModifiersChangedEvent, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, Pixels, Rgba, ScrollWheelEvent, SharedString, Subscription, Task, TextAlign,
    TextRun, TouchPhase, UnderlineStyle, Window, actions, canvas, div, fill, point, prelude::*, px,
    size,
};

use crate::appearance::{Appearance, CursorFun, CursorShapeSetting};
use crate::cursor_look::{self, Frames, Glide, PaintShape, Spark};
use crate::daemon_client::DaemonClient;
use crate::grid::{self, GridSnapshot, TextScope};
use crate::keys;
use crate::links::Link;
use crate::mouse::{self, Button, Mods, MouseAction, ScrollAccumulator};
use crate::pty;
use crate::status::{Status, StatusTracker, TAIL_LINES};
use crate::terminal::{CellPx, TermDims, TermEvent, TerminalCore};
use crate::theme::Theme;
use crate::ui_tokens::size as ui_size;

// Bound in main.rs with the "Terminal" key context. GPUI runs key bindings before `on_key_down`,
// so these win over the byte mapping.
actions!(terminal, [Copy, Paste, ScrollPageUp, ScrollPageDown]);

/// GPUI's default line height (1.618 x size) is too airy for a terminal.
const LINE_HEIGHT_FACTOR: f32 = 1.3;
/// How often to run housekeeping (flush stuck synchronized updates, re-evaluate status).
const TICK: Duration = Duration::from_millis(50);
/// The breathing glow's redraw interval: at most ~30 a second (M11f).
const BREATH_FRAME: Duration = Duration::from_millis(33);
/// The glow around the cursor: its blur and the share of the cursor's colour it takes.
const GLOW_BLUR: f32 = 6.0;
const GLOW_ALPHA: f32 = 0.30;
/// The bar's width and the underline's height; a spark's size.
const BAR: f32 = 2.0;
const SPARK: f32 = 2.0;

/// What a terminal tells its owner.
pub enum TerminalEvent {
    /// The detected agent status changed (M2 heuristics).
    StatusChanged(Status),
    /// The program set (Some) or reset (None) the title via OSC 0/2.
    TitleChanged(Option<SharedString>),
    /// The program exited (`code`: its exit code, if known); the session should close, or
    /// with SSH's "connection failed" (255) hold for a reconnect (M10b).
    Exited { code: Option<u32> },
    /// Enter was pressed in a held (disconnected) pane: reconnect it.
    Reconnect,
    /// Daemon backend: the connection to the daemon is gone. Not an exit: the session may
    /// still run there, so it stays.
    DaemonLost,
}

/// Where the grid was last painted, for turning mouse positions into grid points.
#[derive(Clone, Copy)]
struct GridLayout {
    origin: gpui::Point<Pixels>,
    cell_width: Pixels,
    line_height: Pixels,
    dims: TermDims,
}

impl GridLayout {
    /// The grid that fits `area` (see `inset`): as many whole cells as fit, with the pixels left
    /// over shared evenly on either side.
    fn fit(area: Bounds<Pixels>, cell_width: Pixels, line_height: Pixels) -> Self {
        let dims = dims_for(area, cell_width, line_height);
        let spare_x = (area.size.width - cell_width * dims.cols as f32).max(px(0.0));
        let spare_y = (area.size.height - line_height * dims.rows as f32).max(px(0.0));
        Self {
            origin: area.origin + point(spare_x / 2.0, spare_y / 2.0),
            cell_width,
            line_height,
            dims,
        }
    }

    /// The grid point and cell side under a window position; `display_offset` is the live
    /// scrollback offset. Always in range: clamped to `dims`.
    fn grid_point(&self, pos: gpui::Point<Pixels>, display_offset: usize) -> (Point, Side) {
        let rel = pos - self.origin;
        mouse::grid_point(
            f32::from(rel.x),
            f32::from(rel.y),
            f32::from(self.cell_width),
            f32::from(self.line_height),
            self.dims,
            display_offset,
        )
    }
}

/// The space inside a terminal's padding (10 px sides, 8 px top and bottom; 8 and 6 in a split).
/// The grid is fitted into it (`GridLayout::fit`); the padding is painted with the background.
pub fn inset(bounds: Bounds<Pixels>, split: bool) -> Bounds<Pixels> {
    let (x, y) = if split {
        ui_size::SPLIT_TERMINAL_PADDING
    } else {
        ui_size::TERMINAL_PADDING
    };
    let (x, y) = (px(x), px(y));
    Bounds::new(
        bounds.origin + point(x, y),
        size(
            (bounds.size.width - x * 2.0).max(px(0.0)),
            (bounds.size.height - y * 2.0).max(px(0.0)),
        ),
    )
}

/// How many whole cells fit in `area` (at least 2 columns and 1 row).
fn dims_for(area: Bounds<Pixels>, cell_width: Pixels, line_height: Pixels) -> TermDims {
    TermDims {
        cols: ((area.size.width / cell_width).floor() as usize).max(2),
        rows: ((area.size.height / line_height).floor() as usize).max(1),
    }
}

pub struct TerminalView {
    core: TerminalCore,
    focus_handle: FocusHandle,
    tracker: StatusTracker,
    /// When the canvas last sent the pty a new size; the tracker hears of it before the next
    /// output, input or tick, so the redraw that follows isn't taken for work.
    resized_at: Rc<Cell<Option<Instant>>>,
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
    /// Whether this pane shares the screen with others (a split), which takes less padding. Set
    /// by the workspace as it lays the panes out.
    split: Cell<bool>,
    /// Held after an SSH disconnect (M10b): the program is gone, so input goes nowhere and Enter
    /// asks for a reconnect; the status no longer changes.
    disconnected: bool,
    /// The session's status, as the workspace has it (`set_status`): the cursor's colour.
    status: Status,
    /// The cursor's glide, breath and sparks between paints; the canvas updates it as it paints.
    motion: Rc<RefCell<CursorMotion>>,
    /// The breathing timer is running (`breathe`).
    breathing: bool,
    /// UI screenshots: count the window as the key one (an off-screen window never is).
    #[cfg(feature = "ui-shots")]
    shot_key_window: Cell<bool>,
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
    /// Start `program` (argv, run directly), or a login shell when `None`, in `cwd` (else home),
    /// and wrap it in a new view entity (not focused: the workspace focuses the active pane).
    /// `startup` is typed into the shell right after it starts, with Enter (ignored with a
    /// `program`); `env` is applied to the environment.
    pub fn spawn(
        cx: &mut App,
        program: Option<Vec<String>>,
        cwd: Option<&Path>,
        startup: Option<&str>,
        env: &pty::ShellEnv,
    ) -> Result<Entity<Self>, String> {
        let (tx, rx) = futures::channel::mpsc::unbounded();
        // 80x24 until the first paint measures the real size and resizes.
        let theme = cx.global::<Appearance>().theme;
        let core = TerminalCore::spawn(
            pty::command(program.as_deref(), cwd, env),
            TermDims { cols: 80, rows: 24 },
            tx,
            theme,
        )?;
        if let Some(cmd) = startup.filter(|_| program.is_none()) {
            // The shell reads this as typed-ahead input once it is up.
            core.write(format!("{cmd}\r").as_bytes());
        }
        Ok(cx.new(|cx| Self::new(core, rx, cx)))
    }

    /// A view of daemon session `id` (Daemon backend). It never waits on the daemon. For a new
    /// session, get the id with `DaemonClient::create_in_background` first (its `startup` is
    /// typed in by the daemon); for adoption (Task 7), from `list`.
    pub fn attach_daemon(
        cx: &mut App,
        client: Arc<DaemonClient>,
        id: u64,
    ) -> Result<Entity<Self>, String> {
        let (tx, rx) = futures::channel::mpsc::unbounded();
        let theme = cx.global::<Appearance>().theme;
        let core = TerminalCore::attach_daemon(client, id, tx, theme).map_err(|e| e.to_string())?;
        Ok(cx.new(|cx| Self::new(core, rx, cx)))
    }

    /// `attach_daemon` without the session's screen yet: it comes on `attach` (reopening fuxx,
    /// M10a; only the sessions shown attach at once).
    pub fn adopt_daemon(
        cx: &mut App,
        client: Arc<DaemonClient>,
        id: u64,
    ) -> Result<Entity<Self>, String> {
        let (tx, rx) = futures::channel::mpsc::unbounded();
        let theme = cx.global::<Appearance>().theme;
        let core = TerminalCore::adopt_daemon(client, id, tx, theme).map_err(|e| e.to_string())?;
        Ok(cx.new(|cx| Self::new(core, rx, cx)))
    }

    /// Fetch an adopted session's screen; true if this call asked for it (once per view).
    pub fn attach(&self) -> bool {
        self.core.attach()
    }

    /// End the session in the daemon (Daemon backend; its `Exited` follows, and input it hasn't
    /// delivered yet is discarded). Local: nothing, dropping the view ends the child.
    pub fn close(&self) {
        self.core.close();
    }

    /// Whether the screen can be read (a daemon session's first snapshot has landed).
    pub fn is_drawn(&self) -> bool {
        self.core.is_drawn()
    }

    /// The Daemon backend's queue to the daemon (to let it go out before quitting).
    pub fn pump_queue(&self) -> Option<crate::terminal::PumpQueue> {
        self.core.pump_queue()
    }

    /// Tell the daemon the session's name, note and progress (Daemon backend; Local: nothing).
    pub fn set_meta(&self, meta: crate::mux_proto::SetMeta) {
        self.core.set_meta(meta);
    }

    /// Whether this view's session goes through `client` (that very connection).
    pub fn uses_client(&self, client: &Arc<DaemonClient>) -> bool {
        self.core.uses_client(client)
    }

    /// The daemon session this view shows; `None` on the Local backend.
    pub fn daemon_id(&self) -> Option<u64> {
        self.core.daemon_id()
    }

    /// Whether this view works out the status itself (Local, still connected). On the Daemon
    /// backend the daemon does, and its `Status` arrives as `TermEvent::Status`.
    fn tracks_status(&self) -> bool {
        self.core.daemon_id().is_none() && !self.disconnected
    }

    /// Hold the pane after an SSH disconnect: write `text` (the screen reset and reconnect
    /// line) into the screen only, and from now on drop input; Enter emits `Reconnect`.
    pub fn hold(&mut self, text: &str, cx: &mut Context<Self>) {
        self.disconnected = true;
        self.core.show_local(text);
        cx.notify();
    }

    /// Whether this pane is one of several in a split (less padding). Read when the view renders,
    /// which for a pane on screen is in the same frame as the workspace render that sets it.
    /// True when it changed (the workspace then draws the pane afresh, past its cache).
    pub fn set_split(&self, split: bool) -> bool {
        self.split.replace(split) != split
    }

    /// Write `text` into the screen as if the program had printed it (UI screenshots).
    #[cfg(feature = "ui-shots")]
    pub fn show_text(&self, text: &str) {
        self.core.show_local(text);
    }

    /// Count this terminal's window as the key window (an off-screen window never is).
    #[cfg(feature = "ui-shots")]
    pub fn pretend_key_window(&self) {
        self.shot_key_window.set(true);
    }

    /// Where cell (`col`, `row`) was last painted.
    #[cfg(feature = "ui-shots")]
    pub fn cell_bounds(&self, col: usize, row: usize) -> Option<Bounds<Pixels>> {
        let l = self.layout.get()?;
        Some(Bounds::new(
            point(
                l.origin.x + l.cell_width * col as f32,
                l.origin.y + l.line_height * row as f32,
            ),
            size(l.cell_width, l.line_height),
        ))
    }

    /// The redraws the cursor asked for at its last paint.
    #[cfg(feature = "ui-shots")]
    pub fn cursor_frames(&self) -> Frames {
        self.motion.borrow().frames
    }

    /// The cursor's glide, if one was set going.
    #[cfg(feature = "ui-shots")]
    pub fn cursor_glide(&self) -> Option<Glide> {
        self.motion.borrow().glide
    }

    /// The sparks flying now.
    #[cfg(feature = "ui-shots")]
    pub fn live_sparks(&self) -> usize {
        self.motion.borrow().sparks.len()
    }

    /// How many keystrokes have sparked.
    #[cfg(feature = "ui-shots")]
    pub fn spark_bursts(&self) -> u64 {
        self.motion.borrow().seed
    }

    /// The status of this terminal's session, for the cursor's colour; redraws on a change.
    pub fn set_status(&mut self, status: Status, cx: &mut Context<Self>) {
        if self.status != status {
            self.status = status;
            cx.notify();
        }
    }

    /// Whether this terminal's window is the key one (the breathing glow runs only there).
    fn key_window(&self, window: &Window) -> bool {
        #[cfg(feature = "ui-shots")]
        if self.shot_key_window.get() {
            return true;
        }
        window.is_window_active()
    }

    /// Start the breathing timer unless it runs: a redraw every 33 ms while the last paint
    /// asked for it (`Frames::Timer33ms`) and the terminal is focused in the key window, except
    /// while the breath holds (`breath_redraw`). It stops, without a redraw, as soon as any of
    /// that ends.
    fn breathe(&mut self, window: &Window, cx: &mut Context<Self>) {
        if self.breathing {
            return;
        }
        self.breathing = true;
        cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor().timer(BREATH_FRAME).await;
                let go = this
                    .update_in(cx, |this, window, cx| {
                        let motion = this.motion.borrow();
                        let go = motion.frames == Frames::Timer33ms
                            && this.focus_handle.is_focused(window)
                            && this.key_window(window);
                        let redraw = breath_redraw(motion.changed.elapsed());
                        drop(motion);
                        if go && redraw {
                            cx.notify();
                        } else if !go {
                            this.breathing = false;
                        }
                        go
                    })
                    .unwrap_or(false);
                if !go {
                    break;
                }
            }
        })
        .detach();
    }

    /// A keystroke that types text (not Enter, arrows or ctrl-keys) sends sparks off the
    /// cursor, with Fun = Sparks and motion on.
    fn spark(&mut self, bytes: &[u8], cx: &App) {
        let a = cx.global::<Appearance>();
        if a.cursor_fun != CursorFun::Sparks
            || !a.animations
            || cx.reduce_motion()
            || self.disconnected
            || !is_typed_text(bytes)
        {
            return;
        }
        let Some(layout) = self.layout.get() else {
            return;
        };
        let mut m = self.motion.borrow_mut();
        let Some(((x, y), shape)) = m.drawn else {
            return;
        };
        let (cw, lh) = (f32::from(layout.cell_width), f32::from(layout.line_height));
        let ox = x * cw
            + match shape {
                PaintShape::Bar => BAR / 2.0,
                _ => cw / 2.0,
            };
        let oy = y * lh
            + match shape {
                PaintShape::Underline => lh - BAR / 2.0,
                _ => lh / 2.0,
            };
        m.seed += 1;
        let seed = m.seed;
        m.sparks
            .extend(cursor_look::sparks_for_key(seed, (ox, oy), Instant::now()));
    }

    /// When the program first printed (`TerminalCore::first_output`).
    pub fn first_output(&self) -> Option<Instant> {
        self.core.first_output()
    }

    /// Where the program works: the folder it last reported (OSC 7), and its process id (a
    /// local terminal's; see `TerminalCore`).
    pub fn working_folder(&self) -> (Option<crate::detector::CwdReport>, Option<u32>) {
        (self.core.reported_cwd(), self.core.pid())
    }

    /// The terminal's text (see `grid::TextScope`).
    pub fn text(&self, scope: TextScope) -> String {
        self.core.text(scope)
    }

    /// The last `n` non-blank lines of the live screen (what the status heuristics read).
    pub fn tail_text(&self, n: usize) -> String {
        self.core.tail_text(n)
    }

    /// Type bytes as if from the keyboard: snaps to the bottom and counts as typing.
    pub fn type_bytes(&mut self, bytes: &[u8], cx: &mut Context<Self>) {
        self.input_bytes(bytes, cx);
    }

    /// Paste text, bracketed when the program asked for it.
    pub fn paste(&mut self, text: &str, cx: &mut Context<Self>) {
        let bracketed = self.core.mode().contains(TermMode::BRACKETED_PASTE);
        self.input_bytes(&keys::paste_bytes(text, bracketed), cx);
        cx.notify();
    }

    /// Whether the program wants application-cursor arrow keys.
    pub fn app_cursor(&self) -> bool {
        self.core.mode().contains(TermMode::APP_CURSOR)
    }

    fn new(
        core: TerminalCore,
        mut rx: UnboundedReceiver<TermEvent>,
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
        Self {
            core,
            focus_handle,
            tracker: StatusTracker::new(),
            resized_at: Rc::new(Cell::new(None)),
            layout: Rc::new(Cell::new(None)),
            scroll: ScrollAccumulator::default(),
            hovered: None,
            pressed_link: None,
            selecting: false,
            mouse_reported: false,
            split: Cell::new(false),
            disconnected: false,
            status: Status::Idle,
            motion: Rc::new(RefCell::new(CursorMotion::new(Instant::now()))),
            breathing: false,
            #[cfg(feature = "ui-shots")]
            shot_key_window: Cell::new(false),
            _events: events,
            _ticker: ticker,
            _appearance: appearance,
        }
    }

    fn on_term_event(&mut self, event: TermEvent, cx: &mut Context<Self>) {
        match event {
            TermEvent::Output { notifications } => {
                // The program wrote: the breath holds.
                self.motion.borrow_mut().changed = Instant::now();
                self.hear_of_resize();
                if self.tracks_status()
                    && let Some(status) = self.tracker.on_output(Instant::now(), notifications)
                {
                    self.status_changed(status, cx);
                }
                // Link spans are absolute grid lines that shift as output scrolls; the next
                // mouse move recomputes the hover.
                self.hovered = None;
                cx.notify();
            }
            TermEvent::Term(Event::Wakeup) => cx.notify(),
            // A real BEL (not an OSC terminator). Must come before the `TermEvent::Term(_)` catch-all.
            // (A Daemon replica never reports one: the daemon's tracker has it.)
            TermEvent::Term(Event::Bell) => self.tracker.on_bell(Instant::now()),
            // Replies to terminal queries (cursor position, device attributes, ...) MUST go back
            // to the child, or programs that ask (shells, vim, Claude Code) hang waiting. (A
            // Daemon replica never reports one: the daemon answers.)
            TermEvent::Term(
                event @ (Event::PtyWrite(_)
                | Event::ColorRequest(..)
                | Event::TextAreaSizeRequest(_)),
            ) => {
                if let Some(reply) = self.core.query_reply(&event) {
                    self.core.write(reply.as_bytes());
                }
            }
            TermEvent::Term(Event::Title(title)) => {
                cx.emit(TerminalEvent::TitleChanged(Some(title.into())))
            }
            TermEvent::Term(Event::ResetTitle) => cx.emit(TerminalEvent::TitleChanged(None)),
            TermEvent::Exited { code } => cx.emit(TerminalEvent::Exited { code }),
            TermEvent::Status(status) => self.status_changed(status, cx),
            TermEvent::DaemonLost => cx.emit(TerminalEvent::DaemonLost),
            TermEvent::Term(_) => {}
        }
    }

    fn on_tick(&mut self, cx: &mut Context<Self>) {
        if self.core.flush_stale_sync() {
            cx.notify();
        }
        if !self.tracks_status() {
            return;
        }
        self.hear_of_resize();
        let core = &self.core;
        if let Some(status) = self
            .tracker
            .tick(Instant::now(), || core.tail_text(TAIL_LINES))
        {
            self.status_changed(status, cx);
        }
    }

    /// Pass a resize the canvas made on to the tracker (a no-op when there was none).
    fn hear_of_resize(&mut self) {
        if let Some(at) = self.resized_at.take() {
            self.tracker.on_resize(at);
        }
    }

    fn status_changed(&self, status: Status, cx: &mut Context<Self>) {
        eprintln!("[fuxx] status -> {status:?}"); // debug log, used by the M2 manual check
        cx.emit(TerminalEvent::StatusChanged(status));
        cx.notify();
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
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
        let key_char = keystroke
            .key_char
            .as_deref()
            .map(|t| keys::with_capslock(t, window.capslock().on));
        if let Some(bytes) = keys::key_to_bytes(
            &keystroke.key,
            key_char.as_deref(),
            m.control,
            m.alt && named_key,
            m.shift,
            app_cursor,
        ) {
            self.spark(&bytes, cx);
            self.send_key_bytes(&bytes, cx);
        }
    }

    /// What a keystroke does once it has bytes: write to the pty, tell the tracker, swallow the
    /// event. Held (disconnected): Enter asks for a reconnect, anything else is dropped.
    fn send_key_bytes(&mut self, bytes: &[u8], cx: &mut Context<Self>) {
        if self.disconnected && bytes == b"\r" {
            cx.emit(TerminalEvent::Reconnect);
        }
        self.input_bytes(bytes, cx);
        cx.stop_propagation();
    }

    /// Input from the user or a script: write to the pty and tell the tracker it was typing.
    /// Held (disconnected): dropped, there is no program to take it.
    fn input_bytes(&mut self, bytes: &[u8], cx: &mut Context<Self>) {
        if self.disconnected {
            return;
        }
        self.core.input(bytes);
        // You typed: the breath holds.
        self.motion.borrow_mut().changed = Instant::now();
        self.hear_of_resize();
        if self.tracks_status()
            && let Some(status) = self.tracker.on_input(Instant::now())
        {
            self.status_changed(status, cx);
        }
        // Input snaps the view to the bottom and drops the selection: repaint even if the
        // program echoes nothing (the pane is cached, see `Workspace::render_panes`).
        cx.notify();
    }

    /// A mouse report (or wheel as arrows) for the program; dropped once disconnected.
    fn report(&self, bytes: &[u8]) {
        if !self.disconnected {
            self.core.write(bytes);
        }
    }

    /// Grid point, cell side and viewport col/row under a window position, or `None` before the
    /// first paint. Always in range: `grid_point` clamps to the painted dims and the live offset.
    fn point_at(&self, pos: gpui::Point<Pixels>) -> Option<(Point, Side, usize, usize)> {
        let layout = self.layout.get()?;
        let display_offset = self.core.display_offset();
        let (point, side) = layout.grid_point(pos, display_offset);
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
                    self.report(&bytes);
                }
            }
        } else if mode.contains(TermMode::ALT_SCREEN | TermMode::ALTERNATE_SCROLL) && !shift {
            self.report(&mouse::alt_scroll(lines));
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
                self.report(&bytes);
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
                self.report(&bytes);
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
                self.report(&bytes);
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
    fn paste_clipboard(&mut self, _: &Paste, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return;
        };
        self.paste(&text, cx);
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
        let key_window = self.key_window(window);
        let appearance = cx.global::<Appearance>();
        let theme = self.core.theme();
        let cursor = CursorSettings {
            setting: appearance.cursor_style,
            focused,
            key_window,
            motion: appearance.animations && !cx.reduce_motion(),
            glide: appearance.cursor_glide,
            glow: appearance.cursor_glow,
            fun: appearance.cursor_fun,
            color: cursor_look::cursor_color(
                self.status,
                appearance.cursor_follows_status,
                &theme.ui(),
                &theme,
            ),
        };
        let motion = self.motion.clone();
        let resize_core = self.core.clone();
        let resized_at = self.resized_at.clone();
        let paint_core = self.core.clone();
        let layout = self.layout.clone();
        let hovered = self.hovered.clone();
        let split = self.split.get();
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
            .on_action(cx.listener(Self::paste_clipboard))
            .on_action(cx.listener(Self::scroll_page_up))
            .on_action(cx.listener(Self::scroll_page_down))
            .child(
                canvas(
                    // prepaint: measure cells, fit the terminal inside the padding of the space
                    // we were given, remember the layout for hit-testing. The hitbox is the
                    // whole pane: a click in the padding lands on the nearest cell.
                    move |bounds, window, cx| {
                        let metrics = CellMetrics::measure(cx.global::<Appearance>(), window);
                        let grid = GridLayout::fit(
                            inset(bounds, split),
                            metrics.cell_width,
                            metrics.line_height,
                        );
                        let sent = resize_core.resize(
                            grid.dims,
                            CellPx {
                                width: f32::from(metrics.cell_width).round() as u16,
                                height: f32::from(metrics.line_height).round() as u16,
                            },
                        );
                        if sent {
                            resized_at.set(Some(Instant::now()));
                        }
                        let grid = GridLayout {
                            dims: resize_core.dims(),
                            ..grid
                        };
                        layout.set(Some(grid));
                        let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
                        (metrics, grid.origin, hitbox)
                    },
                    // paint: draw the current screen, set the pointer, follow the mouse.
                    move |bounds, (metrics, origin, hitbox), window, cx| {
                        let mut snap = paint_core.snapshot();
                        if let Some(link) = &hovered {
                            let offset = paint_core.display_offset();
                            grid::underline_cells(&mut snap, link.start, link.end, offset);
                        }
                        let look =
                            cursor.look(&snap, &theme, &mut motion.borrow_mut(), Instant::now());
                        paint_grid(snap, &theme, &metrics, bounds, origin, look, window, cx);
                        // The next redraw the cursor needs (none when still).
                        let frames = motion.borrow().frames;
                        match frames {
                            Frames::EveryFrame => window.request_animation_frame(),
                            Frames::Timer33ms => {
                                view.update(cx, |this, cx| this.breathe(window, cx))
                            }
                            Frames::None => {}
                        }

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
}

/// Paint a fixed screen over `bounds` the way a pane draws its own: the configured font and
/// size, the pane's padding, and `theme`'s colours. `screen` makes it for the grid that fits
/// (see `grid::fixed_screen`). The Preferences preview.
pub fn paint_fixed_screen(
    theme: &Theme,
    bounds: Bounds<Pixels>,
    window: &mut Window,
    cx: &mut App,
    screen: impl FnOnce(TermDims) -> GridSnapshot,
) {
    let metrics = CellMetrics::measure(cx.global::<Appearance>(), window);
    let grid = GridLayout::fit(
        inset(bounds, false),
        metrics.cell_width,
        metrics.line_height,
    );
    let snap = screen(grid.dims);
    let look = snap
        .cursor
        .filter(|c| c.row < snap.rows && c.col < snap.cols)
        .map(|c| CursorLook::outline(c, cursor_cells(&snap.lines[c.row], c.col), theme));
    paint_grid(snap, theme, &metrics, bounds, grid.origin, look, window, cx);
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

/// What the canvas knows of the cursor before it paints (read in `render`).
#[derive(Clone, Copy)]
struct CursorSettings {
    setting: CursorShapeSetting,
    focused: bool,
    key_window: bool,
    /// The `animations` setting on and macOS Reduce motion off.
    motion: bool,
    glide: bool,
    glow: bool,
    fun: CursorFun,
    /// `cursor_look::cursor_color` for the session's status.
    color: Rgb,
}

impl CursorSettings {
    /// The cursor to paint over `snap` at `now`, with `motion` brought up to date (where it
    /// is, its glide, its sparks, the frames it needs next). `None` when there's no cursor.
    fn look(
        &self,
        snap: &GridSnapshot,
        theme: &Theme,
        motion: &mut CursorMotion,
        now: Instant,
    ) -> Option<CursorLook> {
        let live = self.focused && self.key_window;
        let moving = live && self.motion;
        let c = snap
            .cursor
            .filter(|c| c.row < snap.rows && c.col < snap.cols);
        let style = c.map(|c| {
            cursor_look::effective_style(
                ansi::CursorStyle {
                    shape: c.shape,
                    blinking: c.blinking,
                },
                self.setting,
            )
        });
        let cell = c
            .filter(|_| style.is_some_and(|s| s.shape != PaintShape::Hidden))
            .map(|c| (c.col, c.row));
        let at = motion.follow(cell, Screen::of(snap), moving && self.glide, now);
        if !moving {
            motion.sparks.clear();
        }
        motion
            .sparks
            .retain(|s| cursor_look::spark_at(s, now).is_some());
        let trail = match motion.glide {
            Some(g) if moving && self.fun == CursorFun::Trail => cursor_look::trail(&g, now),
            _ => Vec::new(),
        };
        let animating = motion.gliding(now) || !trail.is_empty() || !motion.sparks.is_empty();
        if !animating {
            motion.glide = None;
        }
        let glowing = self.glow && style.is_some_and(|s| s.blinking) && at.is_some();
        motion.frames = cursor_look::needs_frames(
            self.focused,
            self.key_window,
            self.motion,
            glowing,
            animating,
        );
        let (Some(c), Some(style), Some(at)) = (c, style, at) else {
            motion.drawn = None;
            return None;
        };
        let cells = cursor_cells(&snap.lines[c.row], c.col);
        if !live {
            motion.drawn = None;
            return Some(CursorLook::outline(c, cells, theme));
        }
        motion.drawn = Some((at, style.shape));
        let glow = glowing && self.motion;
        let alpha = if glow {
            let since = now.saturating_duration_since(motion.changed);
            cursor_look::breath_alpha(since, since.saturating_sub(cursor_look::HOLD))
        } else {
            1.0
        };
        let own = theme.cursor.unwrap_or(theme.foreground);
        Some(CursorLook {
            cell: (c.col, c.row),
            cells,
            at,
            shape: style.shape,
            color: self.color,
            text: if self.color == own {
                theme.cursor_text.unwrap_or(theme.background)
            } else {
                theme.background
            },
            alpha,
            glow,
            trail,
            sparks: motion
                .sparks
                .iter()
                .filter_map(|s| cursor_look::spark_at(s, now))
                .collect(),
        })
    }
}

/// The cursor as `paint_grid` draws it.
struct CursorLook {
    /// The cell it is on (a block's letter there takes `text`).
    cell: (usize, usize),
    /// How many cells wide it is: 2 on a wide (CJK) character (`cursor_cells`).
    cells: usize,
    /// Where it is drawn, in cells (fractional mid-glide).
    at: (f32, f32),
    /// Never `Hidden`.
    shape: PaintShape,
    color: Rgb,
    /// The letter's colour under a full block.
    text: Rgb,
    /// The breath: the cursor's opacity.
    alpha: f32,
    glow: bool,
    /// `(column, row, scale, alpha)` copies under the cursor (`cursor_look::trail`).
    trail: Vec<(f32, f32, f32, f32)>,
    /// Each spark's position (pixels from the grid's origin) and alpha.
    sparks: Vec<((f32, f32), f32)>,
}

impl CursorLook {
    /// The still 1 px outline in the theme's cursor colour of a terminal that isn't focused
    /// in the key window.
    fn outline(c: grid::CursorView, cells: usize, theme: &Theme) -> Self {
        Self {
            cell: (c.col, c.row),
            cells,
            at: (c.col as f32, c.row as f32),
            shape: PaintShape::Hollow,
            color: theme.cursor.unwrap_or(theme.foreground),
            text: theme.background,
            alpha: 1.0,
            glow: false,
            trail: Vec::new(),
            sparks: Vec::new(),
        }
    }

    /// The shape's box with its cell's top-left at `at`, scaled about its middle by `scale`
    /// (the trail's copies): a 2 px bar at the left edge, a 2 px underline at the bottom, the
    /// whole cell for a block or hollow (both halves of a wide character).
    fn bounds(&self, at: gpui::Point<Pixels>, m: &CellMetrics, scale: f32) -> Bounds<Pixels> {
        let (cw, lh) = (m.cell_width * self.cells as f32, m.line_height);
        match self.shape {
            PaintShape::Bar => Bounds::new(
                point(at.x, at.y + lh * (1.0 - scale) / 2.0),
                size(px(BAR), lh * scale),
            ),
            PaintShape::Underline => Bounds::new(
                point(at.x + cw * (1.0 - scale) / 2.0, at.y + lh - px(BAR)),
                size(cw * scale, px(BAR)),
            ),
            _ => Bounds::new(
                point(
                    at.x + cw * (1.0 - scale) / 2.0,
                    at.y + lh * (1.0 - scale) / 2.0,
                ),
                size(cw * scale, lh * scale),
            ),
        }
    }

    /// Paint the shape in `bounds` at opacity `alpha`.
    fn paint(&self, bounds: Bounds<Pixels>, alpha: f32, window: &mut Window) {
        let color = hsla(self.color).opacity(alpha);
        match self.shape {
            PaintShape::Hollow => {
                window.paint_quad(gpui::outline(bounds, color, gpui::BorderStyle::Solid))
            }
            _ => window.paint_quad(fill(bounds, color)),
        }
    }
}

/// `a` moved toward `b` by `t` (0 to 1).
fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let m = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t.clamp(0.0, 1.0)).round() as u8;
    Rgb {
        r: m(a.r, b.r),
        g: m(a.g, b.g),
        b: m(a.b, b.b),
    }
}

/// Whether a keystroke's bytes are text you typed (they spark): printable characters only,
/// not Enter, Tab, Backspace, escape sequences or ctrl-keys.
fn is_typed_text(bytes: &[u8]) -> bool {
    std::str::from_utf8(bytes).is_ok_and(|t| !t.is_empty() && !t.chars().any(char::is_control))
}

/// The screen the cursor is on, as its glide tells screens apart: a move onto another one
/// (the alternate screen, the view scrolled back, a resize, a clear) appears at once.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Screen {
    alt: bool,
    offset: usize,
    cols: usize,
    rows: usize,
    clears: u64,
}

impl Screen {
    fn of(snap: &GridSnapshot) -> Self {
        Self {
            alt: snap.alt_screen,
            offset: snap.display_offset,
            cols: snap.cols,
            rows: snap.rows,
            clears: snap.clears,
        }
    }
}

/// How many cells the cursor at `col` of `line` covers: 2 on a wide character's left half
/// (the next cell its spacer), else 1.
fn cursor_cells(line: &[grid::CellView], col: usize) -> usize {
    match (line.get(col), line.get(col + 1)) {
        (Some(c), Some(next)) if c.wide && next.spacer => 2,
        _ => 1,
    }
}

/// Whether a breathing tick redraws: not while the breath holds at full strength after a
/// change (`cursor_look::HOLD`; the cursor looks the same), so steady output or typing costs
/// no idle repaints.
fn breath_redraw(since_change: Duration) -> bool {
    since_change >= cursor_look::HOLD
}

/// The cursor's motion between paints (M11f): the cell and screen it was last painted on, its
/// glide, when it last changed (the breath holds after), the sparks in flight and how many
/// keystrokes sparked (each burst's seed). Shared by the view and its canvas.
struct CursorMotion {
    /// The cell it was last painted on; `None` while hidden.
    cell: Option<(usize, usize)>,
    /// The screen of that cell.
    screen: Screen,
    glide: Option<Glide>,
    /// When the cursor last moved, the program wrote or you typed.
    changed: Instant,
    sparks: Vec<Spark>,
    seed: u64,
    /// Where it was last drawn (cells) and its shape, while live: where sparks start.
    drawn: Option<((f32, f32), PaintShape)>,
    /// The redraws its last paint asked for.
    frames: Frames,
}

impl CursorMotion {
    fn new(now: Instant) -> Self {
        Self {
            cell: None,
            screen: Screen::default(),
            glide: None,
            changed: now,
            sparks: Vec::new(),
            seed: 0,
            drawn: None,
            frames: Frames::None,
        }
    }

    /// The cursor is on `cell` (`None`: hidden) of `screen` at `now`: start a glide from where
    /// it is drawn when it moved and `glide` allows (`cursor_look::glide_from_now`; the rows
    /// moved counted between cells), and say where to draw it, in cells.
    fn follow(
        &mut self,
        cell: Option<(usize, usize)>,
        screen: Screen,
        glide: bool,
        now: Instant,
    ) -> Option<(f32, f32)> {
        let screen_changed = std::mem::replace(&mut self.screen, screen) != screen;
        if !glide {
            self.glide = None;
        }
        let Some(to) = cell else {
            self.cell = None;
            self.glide = None;
            return None;
        };
        match self.cell {
            Some(from) if from != to || screen_changed => {
                self.changed = now;
                if glide {
                    let drawn = self.position(from, now);
                    self.glide = cursor_look::glide_from_now(
                        self.glide.as_ref(),
                        drawn,
                        to,
                        from.1.abs_diff(to.1),
                        screen_changed,
                        now,
                    );
                }
            }
            None => self.changed = now,
            Some(_) => {}
        }
        self.cell = Some(to);
        Some(self.position(to, now))
    }

    /// Where the cursor headed for (or on) `cell` is drawn at `now`.
    fn position(&self, cell: (usize, usize), now: Instant) -> (f32, f32) {
        self.glide
            .filter(|g| g.to == cell)
            .map_or((cell.0 as f32, cell.1 as f32), |g| g.position(now))
    }

    /// Whether a glide is still on its way at `now`.
    fn gliding(&self, now: Instant) -> bool {
        self.glide
            .is_some_and(|g| g.position(now) != (g.to.0 as f32, g.to.1 as f32))
    }
}

/// Paint `snap` with its first cell at `origin`, over `bounds` (the whole pane, padding
/// included) filled with the theme background, and `cursor` over it: its trail and glow under
/// the text, a block under its letter (which takes the block's text colour), any other shape
/// over the text, then the sparks.
#[allow(clippy::too_many_arguments)]
fn paint_grid(
    mut snap: GridSnapshot,
    theme: &Theme,
    m: &CellMetrics,
    bounds: Bounds<Pixels>,
    origin: gpui::Point<Pixels>,
    cursor: Option<CursorLook>,
    window: &mut Window,
    cx: &mut App,
) {
    window.paint_quad(fill(bounds, hsla(theme.background)));

    let at_cells =
        |(x, y): (f32, f32)| point(origin.x + m.cell_width * x, origin.y + m.line_height * y);
    let cell_origin = |row: usize, col: usize| at_cells((col as f32, row as f32));

    // A block on its cell: the letter there turns the block's text colour, as much as the
    // block shows through its breath.
    if let Some(c) = &cursor
        && c.shape == PaintShape::Block
        && c.at == (c.cell.0 as f32, c.cell.1 as f32)
        && let Some(cell) = snap
            .lines
            .get_mut(c.cell.1)
            .and_then(|l| l.get_mut(c.cell.0))
    {
        cell.fg = mix(cell.fg, c.text, c.alpha);
    }

    for (row, line) in snap.lines.iter().enumerate() {
        for rect in grid::bg_rects(line, theme.background) {
            let size = size(m.cell_width * rect.cells as f32, m.line_height);
            window.paint_quad(fill(
                Bounds::new(cell_origin(row, rect.col), size),
                hsla(rect.color),
            ));
        }
    }

    if let Some(c) = &cursor {
        for &(x, y, scale, alpha) in &c.trail {
            c.paint(
                c.bounds(at_cells((x, y)), m, scale),
                alpha * c.alpha,
                window,
            );
        }
        let shape = c.bounds(at_cells(c.at), m, 1.0);
        if c.glow {
            window.paint_drop_shadows(
                shape,
                Corners::default(),
                &[BoxShadow {
                    color: hsla(c.color).opacity(GLOW_ALPHA * c.alpha),
                    offset: point(px(0.0), px(0.0)),
                    blur_radius: px(GLOW_BLUR),
                    spread_radius: px(0.0),
                    inset: false,
                }],
            );
        }
        if c.shape == PaintShape::Block {
            c.paint(shape, c.alpha, window);
        }
    }

    for (row, line) in snap.lines.iter().enumerate() {
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

    if let Some(c) = &cursor {
        if c.shape != PaintShape::Block {
            c.paint(c.bounds(at_cells(c.at), m, 1.0), c.alpha, window);
        }
        let dot = px(SPARK);
        for &((x, y), alpha) in &c.sparks {
            window.paint_quad(fill(
                Bounds::new(
                    point(origin.x + px(x) - dot / 2.0, origin.y + px(y) - dot / 2.0),
                    size(dot, dot),
                ),
                hsla(c.color).opacity(alpha),
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use gpui::{Bounds, point, px, size};

    use super::{
        CursorMotion, GridLayout, Screen, breath_redraw, cursor_cells, inset, is_typed_text,
    };
    use crate::grid::CellView;
    use crate::theme::Theme;

    /// The main screen of an 80x24 terminal, not scrolled back, never cleared.
    const MAIN: Screen = Screen {
        alt: false,
        offset: 0,
        cols: 80,
        rows: 24,
        clears: 0,
    };

    #[test]
    fn first_sight_appears_at_once() {
        let t0 = Instant::now();
        let mut m = CursorMotion::new(t0);
        assert_eq!(m.follow(Some((2, 1)), MAIN, true, t0), Some((2.0, 1.0)));
        assert!(m.glide.is_none());
    }

    #[test]
    fn a_move_glides_from_the_old_cell() {
        let t0 = Instant::now();
        let mut m = CursorMotion::new(t0);
        m.follow(Some((2, 1)), MAIN, true, t0);
        let t1 = t0 + Duration::from_millis(500);
        // However many moves came between two paints, the paint sees only the last cell.
        assert_eq!(m.follow(Some((12, 1)), MAIN, true, t1), Some((2.0, 1.0)));
        let g = m.glide.expect("a glide");
        assert_eq!((g.from, g.to, g.start), ((2.0, 1.0), (12, 1), t1));
        assert_eq!(m.changed, t1);
        assert!(m.gliding(t1));
        // A retarget mid-glide starts where the cursor is drawn.
        let t2 = t1 + Duration::from_millis(45);
        let drawn = g.position(t2);
        m.follow(Some((3, 4)), MAIN, true, t2);
        let g2 = m.glide.expect("a new glide");
        assert_eq!((g2.from, g2.to), (drawn, (3, 4)));
        // Over once its time is up.
        assert!(!m.gliding(t2 + Duration::from_millis(90)));
        assert_eq!(
            m.follow(Some((3, 4)), MAIN, true, t2 + Duration::from_millis(90)),
            Some((3.0, 4.0))
        );
    }

    #[test]
    fn no_glide_when_off_far_on_another_screen_or_after_hiding() {
        let t0 = Instant::now();
        let mut m = CursorMotion::new(t0);
        m.follow(Some((2, 1)), MAIN, true, t0);
        assert_eq!(m.follow(Some((9, 1)), MAIN, false, t0), Some((9.0, 1.0)));
        assert!(m.glide.is_none());
        // 21 rows, counted from the cells.
        assert_eq!(m.follow(Some((9, 22)), MAIN, true, t0), Some((9.0, 22.0)));
        assert!(m.glide.is_none());
        // The alternate screen, or the view scrolled back.
        let alt = Screen { alt: true, ..MAIN };
        assert_eq!(m.follow(Some((0, 0)), alt, true, t0), Some((0.0, 0.0)));
        assert!(m.glide.is_none());
        let back = Screen { offset: 3, ..alt };
        assert_eq!(m.follow(Some((0, 3)), back, true, t0), Some((0.0, 3.0)));
        assert!(m.glide.is_none());
        // Hidden, then back elsewhere.
        assert_eq!(m.follow(None, back, true, t0), None);
        assert_eq!(m.follow(Some((5, 5)), back, true, t0), Some((5.0, 5.0)));
        assert!(m.glide.is_none());
    }

    #[test]
    fn no_glide_after_a_clear_or_a_resize_but_a_line_feed_glides() {
        let t0 = Instant::now();
        let mut m = CursorMotion::new(t0);
        m.follow(Some((2, 10)), MAIN, true, t0);
        // `clear` from a prompt 10 rows down: the cursor is home at once.
        let cleared = Screen { clears: 1, ..MAIN };
        assert_eq!(m.follow(Some((0, 0)), cleared, true, t0), Some((0.0, 0.0)));
        assert!(m.glide.is_none());
        // A resize reflows the cursor's row: no glide either, frame after frame of a drag.
        m.follow(Some((0, 5)), cleared, true, t0);
        let narrow = Screen {
            cols: 60,
            ..cleared
        };
        assert_eq!(m.follow(Some((0, 6)), narrow, true, t0), Some((0.0, 6.0)));
        assert!(m.glide.is_none());
        let shorter = Screen { rows: 20, ..narrow };
        assert_eq!(m.follow(Some((0, 4)), shorter, true, t0), Some((0.0, 4.0)));
        assert!(m.glide.is_none());
        // A plain line feed on the same screen still glides.
        assert_eq!(m.follow(Some((0, 5)), shorter, true, t0), Some((0.0, 4.0)));
        assert_eq!(m.glide.map(|g| g.to), Some((0, 5)));
    }

    #[test]
    fn the_breath_redraws_only_once_its_hold_is_over() {
        assert!(!breath_redraw(Duration::ZERO));
        assert!(!breath_redraw(Duration::from_millis(599)));
        assert!(breath_redraw(Duration::from_millis(600)));
        assert!(breath_redraw(Duration::from_secs(5)));
    }

    #[test]
    fn a_block_on_a_wide_character_covers_both_halves() {
        let theme = Theme::default();
        let blank = CellView::blank(&theme);
        let wide = CellView {
            ch: '漢',
            wide: true,
            ..blank
        };
        let spacer = CellView {
            ch: ' ',
            spacer: true,
            ..blank
        };
        let line = [blank, wide, spacer, blank];
        assert_eq!(cursor_cells(&line, 1), 2);
        assert_eq!(cursor_cells(&line, 0), 1);
        assert_eq!(cursor_cells(&line, 3), 1);
        // A wide character cut at the edge, or out of range: one cell.
        assert_eq!(cursor_cells(&[blank, wide], 1), 1);
        assert_eq!(cursor_cells(&line, 9), 1);
    }

    #[test]
    fn only_typed_text_sparks() {
        for typed in [&b"a"[..], "ß".as_bytes(), b" ", b"abc"] {
            assert!(is_typed_text(typed), "{typed:?}");
        }
        for not in [
            &b""[..],
            b"\r",
            b"\t",
            b"\x1b[D",
            b"\x03",
            b"\x7f",
            b"\x1bb",
        ] {
            assert!(!is_typed_text(not), "{not:?}");
        }
    }

    #[test]
    fn inset_balances_leftover() {
        let bounds = Bounds::new(point(px(0.0), px(0.0)), size(px(400.0), px(300.0)));
        // 10 px sides, 8 px top and bottom; 8 and 6 in a split.
        assert_eq!(
            inset(bounds, false),
            Bounds::new(point(px(10.0), px(8.0)), size(px(380.0), px(284.0)))
        );
        assert_eq!(
            inset(bounds, true),
            Bounds::new(point(px(8.0), px(6.0)), size(px(384.0), px(288.0)))
        );
        // 9 x 17 cells: 42 columns use 378 of 380 px, 16 rows 272 of 284. What's left over is
        // shared evenly, so the grid sits 1 px further in on each side, 6 px top and bottom.
        let grid = GridLayout::fit(inset(bounds, false), px(9.0), px(17.0));
        assert_eq!((grid.dims.cols, grid.dims.rows), (42, 16));
        assert_eq!(grid.origin, point(px(11.0), px(14.0)));
        let right = bounds.size.width - (grid.origin.x + grid.cell_width * 42.0);
        let bottom = bounds.size.height - (grid.origin.y + grid.line_height * 16.0);
        assert_eq!((right, bottom), (px(11.0), px(14.0)));
    }

    #[test]
    fn hit_testing_uses_the_inset_origin() {
        // A pane right of a 220 px sidebar: 580 x 384 inside the padding, 8 x 17 cells, so
        // 72 x 22 cells with 4 and 10 px left over; the grid starts at (232, 13).
        let bounds = Bounds::new(point(px(220.0), px(0.0)), size(px(600.0), px(400.0)));
        let grid = GridLayout::fit(inset(bounds, false), px(8.0), px(17.0));
        let at = |x: f32, y: f32| {
            let (p, _) = grid.grid_point(point(px(x), px(y)), 0);
            (p.column.0, p.line.0)
        };
        // Just inside cell (3, 2).
        assert_eq!(at(232.0 + 3.0 * 8.0 + 1.0, 13.0 + 2.0 * 17.0 + 1.0), (3, 2));
        // In the padding, left of and above the grid: the first cell, not the second.
        assert_eq!(at(229.0, 10.0), (0, 0));
        // The last cell's far edge.
        assert_eq!(
            at(232.0 + 72.0 * 8.0 - 1.0, 13.0 + 22.0 * 17.0 - 1.0),
            (71, 21)
        );
    }
}
