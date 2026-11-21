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
    AnyView, App, Bounds, BoxShadow, ClipboardItem, Context, Corners, CursorStyle, DispatchPhase,
    Entity, EventEmitter, FocusHandle, Focusable, Font, FontStyle, FontWeight, HitboxBehavior,
    Hsla, KeyDownEvent, Modifiers, ModifiersChangedEvent, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, Pixels, Rgba, ScrollWheelEvent, SharedString, Subscription, Task,
    TextAlign, TextRun, TouchPhase, UnderlineStyle, Window, actions, canvas, div, fill, point,
    prelude::*, px, size,
};

use crate::appearance::{Appearance, CursorFun, CursorShapeSetting};
use crate::block_view::{
    self, BAR_END_GAP, BAR_RADIUS, BlockDrag, CHIP_GAP, CHIP_HIDE_MS, CHIP_PAD, CHIP_RADIUS,
    GUTTER_BAR, GUTTER_BAR_ACTIVE, GutterHit, Jump, Refusal,
};
use crate::blocks::{Block, Outcome};
use crate::command_name::{Change, CommandMark, CommandRun};
use crate::cursor_look::{self, Frames, Glide, PaintShape, Spark};
use crate::daemon_client::DaemonClient;
use crate::grid::{self, GridSnapshot, TextScope};
use crate::keys;
use crate::links::Link;
use crate::mouse::{self, Button, Mods, MouseAction, ScrollAccumulator};
use crate::pty;
use crate::sidebar_toggle::Tip;
use crate::status::{Status, StatusTracker, TAIL_LINES};
use crate::terminal::{CellPx, TermDims, TermEvent, TerminalCore};
use crate::theme::{Theme, UiColors};
use crate::ui_tokens::size as ui_size;

// Bound in main.rs with the "Terminal" key context. GPUI runs key bindings before `on_key_down`,
// so these win over the byte mapping. The command-block actions (M11l): ⇧⌘↑ / ⇧⌘↓ / ⇧⌘C; Copy
// Command, Copy Command and Output and Rerun Command have no keys (the palette, the chip).
actions!(
    terminal,
    [
        Copy,
        Paste,
        ScrollPageUp,
        ScrollPageDown,
        PrevCommand,
        NextCommand,
        CopyCommandOutput,
        CopyCommand,
        CopyCommandAndOutput,
        RerunCommand
    ]
);

/// GPUI's default line height (1.618 x size) is too airy for a terminal.
const LINE_HEIGHT_FACTOR: f32 = 1.3;
/// How often to run housekeeping (flush stuck synchronized updates, re-evaluate status).
const TICK: Duration = Duration::from_millis(50);
/// The glow around the cursor: its blur and the share of the cursor's colour it takes.
const GLOW_BLUR: f32 = 6.0;
const GLOW_ALPHA: f32 = 0.30;
/// The bar's width and the underline's height; a spark's size.
const BAR: f32 = cursor_look::BAR;
/// The shell box's placeholder.
pub const PLACEHOLDER: &str = "Type a command";
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
    /// The program reported another working folder (OSC 7: a `cd`).
    FolderMoved,
    /// What runs at the shell prompt changed (OSC 133 marks): only its short form is in it.
    Command(Change),
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

/// What the last paint drew of the command blocks (M11l), for the hover, the chip and gutter
/// presses: the bars' hit areas, the blocks they belong to, and where the gutter and the grid
/// are (window points). Empty with `command-blocks` off and on the alternate screen.
#[derive(Default)]
struct GutterPaint {
    /// The bars drawn; each `id_index` points into `blocks`.
    hits: Vec<GutterHit>,
    /// The frame's blocks (taken from its snapshot), the top of each one's command row, where
    /// its chip opens, and the column of a right prompt on that row (`block_view::right_run`).
    blocks: Vec<grid::BlockView>,
    cmd_tops: Vec<Pixels>,
    runs: Vec<Option<usize>>,
    /// A cell's width and a row's height.
    cell: (Pixels, Pixels),
    /// The gutter strip: from the pane's left edge to the grid's.
    strip: (Pixels, Pixels),
    pane: Bounds<Pixels>,
    grid: Bounds<Pixels>,
}

impl GutterPaint {
    /// The block whose bar is under window position `pos`, if any.
    fn hit(&self, pos: gpui::Point<Pixels>) -> Option<&grid::BlockView> {
        if pos.x < self.strip.0 || pos.x >= self.strip.1 {
            return None;
        }
        let hit = block_view::hit_at(&self.hits, f32::from(pos.y))?;
        self.blocks.get(hit.id_index)
    }

    /// Block `id`, if its bar was drawn, its command row's top and that row's right prompt.
    fn drawn(&self, id: &str) -> Option<(&grid::BlockView, Pixels, Option<usize>)> {
        self.hits
            .iter()
            .map(|h| h.id_index)
            .find(|&i| self.blocks[i].id == id)
            .map(|i| (&self.blocks[i], self.cmd_tops[i], self.runs[i]))
    }
}

/// Where the chip's three buttons were laid out, which are enabled, and whether Rerun is (UI
/// screenshots).
#[cfg(feature = "ui-shots")]
pub type ChipButtonsShot = ([Option<Bounds<Pixels>>; 3], [bool; 3], Result<(), Refusal>);

/// The chip as `render` lays it out: what it says and where it goes, relative to the pane.
struct Chip {
    /// The block it is for.
    id: String,
    /// Whether Rerun would type its command now, or why not.
    rerun: Result<(), Refusal>,
    outcome: Option<(String, Rgb)>,
    cmd: Option<String>,
    /// What dragging its command carries (M11aa Task 11): none without a command.
    drag: Option<BlockDrag>,
    /// Whether the block has output (Copy output is disabled without).
    has_output: bool,
    buttons: [&'static str; 3],
    left: Pixels,
    top: Pixels,
    /// One terminal row.
    height: Pixels,
    max_width: Pixels,
}

/// The space inside a terminal's padding (16 px sides, 12 px top and bottom; 12 and 10 in a
/// split). The grid is fitted into it (`GridLayout::fit`); the padding is painted with the
/// background.
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

/// Whether a terminal is card-capable after `mark` (M11ab ruling 3, final review #2): a known
/// answer stands (a shell this window spawned); otherwise a real mark says yes, and a snapshot
/// (`Unknown`) says yes only when the grid already holds a tag (`tagged`).
fn integration_after(
    mark: &CommandMark,
    now: Option<bool>,
    tagged: impl FnOnce() -> bool,
) -> Option<bool> {
    match (now, mark) {
        (Some(known), _) => Some(known),
        (None, CommandMark::Unknown) => tagged().then_some(true),
        (None, _) => Some(true),
    }
}

/// The terminal's size for a pane of `bounds` (M11ab): in card view the live box's inside at
/// its tallest (`card_view::card_dims`), otherwise the cells that fit inside the padding.
fn pane_dims(
    cards: bool,
    bounds: Bounds<Pixels>,
    split: bool,
    cell_width: Pixels,
    line_height: Pixels,
) -> TermDims {
    if cards {
        crate::card_view::card_dims(
            bounds.size,
            split,
            f32::from(cell_width),
            f32::from(line_height),
        )
    } else {
        GridLayout::fit(inset(bounds, split), cell_width, line_height).dims
    }
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
    /// `TerminalCore::cwd_changes` as last heard: output that moves it is a `cd`.
    cwd_seen: u64,
    focus_handle: FocusHandle,
    tracker: StatusTracker,
    /// The command running at the shell prompt now (M11q), from the OSC 133 marks.
    run: CommandRun,
    /// Whether its zsh started with fuxx's marks (M11ab: it may show cards): known for a shell
    /// this window spawned (from its env); for a background-service session, unknown until its
    /// first mark, which makes it true (note 88, ruling 3).
    integration: Option<bool>,
    /// Marks seen so far (M11ab: the card cache reads the segments again when it moves).
    marks_seen: u64,
    /// The card view's state (M11ab), whether the last frame drew cards, and its line height
    /// (the wheel scrolls the column in points).
    cards: Rc<RefCell<crate::card_view::CardsState>>,
    cards_on: Rc<Cell<bool>>,
    card_line_h: Rc<Cell<f32>>,
    /// The cell width and line height last measured (pointing into cards; the scenes check
    /// the box's inside).
    card_cell: Rc<Cell<(f32, f32)>>,
    /// The region a selection started in (`card_view::Region::key`): it stays in that card.
    card_region: Option<String>,
    /// This terminal's folder's branch, pushed by the workspace (the composer's context row).
    branch: Option<String>,
    /// Its folder, as the workspace found it (a shell's report, else the process's own): the
    /// composer's context row (M11ab chores).
    folder: Option<std::path::PathBuf>,
    /// A one-second redraw is pending for the running chip's time.
    tick_pending: bool,
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
    /// Command blocks (M11l): what the last paint drew in the gutter, written by the canvas.
    gutter: Rc<RefCell<GutterPaint>>,
    /// The block whose gutter the pointer is over (or was, until `chip_hide` runs out): its
    /// bar is wide and its chip open.
    hovered_block: Option<String>,
    /// The block picked (M11l Task 5: ⇧⌘↑ / ⇧⌘↓ or a click on its bar): its bar runs its
    /// whole height in the accent. Cleared by typing and by a press in the grid.
    chosen_block: Option<String>,
    /// Where the chip was last laid out (written as it prepaints), for the hover.
    chip_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    /// Where its three buttons were laid out (UI screenshots press them).
    #[cfg(feature = "ui-shots")]
    chip_button_bounds: Rc<Cell<[Option<Bounds<Pixels>>; 3]>>,
    /// Where its command's words (the block's drag handle) were laid out.
    #[cfg(feature = "ui-shots")]
    chip_command_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    /// Every byte sent as input (UI screenshots read what Rerun sent).
    #[cfg(feature = "ui-shots")]
    sent: Vec<u8>,
    /// Hides the chip `CHIP_HIDE_MS` after the pointer left the gutter and the chip; dropped
    /// (cancelled) when it comes back.
    chip_hide: Option<Task<()>>,
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
        let integrated = crate::shell_integration::integrated_in(env);
        Ok(cx.new(|cx| {
            let mut view = Self::new(core, rx, cx);
            view.integration = Some(integrated);
            view
        }))
    }

    /// Whether its zsh started with fuxx's marks (M11ab's card view needs them).
    pub fn integrated(&self) -> bool {
        self.integration == Some(true)
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
        // The program is gone: what ran is not known any more.
        self.command_marked(CommandMark::Unknown, cx);
    }

    /// Whether this pane is one of several in a split (less padding). Read when the view renders,
    /// which for a pane on screen is in the same frame as the workspace render that sets it.
    /// True when it changed (the workspace then draws the pane afresh, past its cache).
    pub fn set_split(&self, split: bool) -> bool {
        self.split.replace(split) != split
    }

    /// The scenes' `cat` stands in for a zsh with fuxx's marks (M11ab's card view needs them),
    /// and a mark was seen (the card cache reads the segments again).
    #[cfg(feature = "ui-shots")]
    pub fn shot_integrated(&mut self) {
        self.integration = Some(true);
        self.marks_seen += 1;
    }

    /// The card view's last frame: whether it drew cards, and the terminal's size then.
    /// Last frame's rows to point into: (key, x, y, bottom, live).
    #[cfg(feature = "ui-shots")]
    pub fn shot_card_regions(&self) -> Vec<(String, f32, f32, f32, bool)> {
        self.cards
            .borrow()
            .regions
            .iter()
            .map(|r| (r.key.clone(), r.x, r.y, r.bottom, r.live))
            .collect()
    }

    /// The column's scroll from its bottom, and a way to set it (the scenes scroll up).
    #[cfg(feature = "ui-shots")]
    pub fn shot_card_scroll(&self) -> f32 {
        self.cards.borrow().scroll
    }

    #[cfg(feature = "ui-shots")]
    pub fn shot_scroll_cards(&mut self, by: f32) {
        let mut cards = self.cards.borrow_mut();
        cards.scroll = (cards.scroll + by).clamp(0.0, cards.max_scroll);
    }

    #[cfg(feature = "ui-shots")]
    pub fn shot_typed_row(&self) -> String {
        self.cards.borrow().typed_row.clone()
    }

    /// The card view's placeholder as drawn and its left edge (none: not drawn).
    #[cfg(feature = "ui-shots")]
    pub fn shot_hint(&self) -> Option<(String, f32)> {
        self.cards.borrow().hint.clone()
    }

    /// The composer's pill row as laid out last frame, its width (UI screenshots).
    #[cfg(feature = "ui-shots")]
    pub fn shot_pills_w(&self) -> f32 {
        self.cards.borrow().pills_w
    }

    #[cfg(feature = "ui-shots")]
    pub fn shot_card_count(&self) -> usize {
        self.cards.borrow().hits.len()
    }

    #[cfg(feature = "ui-shots")]
    pub fn shot_cards(&self) -> (bool, crate::terminal::TermDims, (f32, f32)) {
        (self.cards_on.get(), self.core.dims(), self.card_cell.get())
    }

    /// Write `text` into the screen as if the program had printed it (UI screenshots).
    #[cfg(feature = "ui-shots")]
    pub fn show_text(&self, text: &str) {
        self.core.show_local(text);
    }

    /// The status tracker found `status`: passed on as `on_term_event` passes its answers
    /// (`TerminalEvent::StatusChanged` to the workspace), for scenes with no agent to make one.
    #[cfg(feature = "ui-shots")]
    pub fn shot_status(&self, status: Status, cx: &mut Context<Self>) {
        self.status_changed(status, cx);
    }

    /// `text` as the program's output, as the app takes it from the bytes on: the screen, then
    /// each command mark as `on_term_event` takes it (UI screenshots).
    #[cfg(feature = "ui-shots")]
    pub fn shot_output(&mut self, text: &str, cx: &mut Context<Self>) {
        for mark in self.core.show_local(text) {
            self.on_term_event(TermEvent::Command(mark), cx);
        }
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

    /// The block whose gutter the pointer is over (or was, until the chip goes).
    #[cfg(feature = "ui-shots")]
    pub fn hovered_block(&self) -> Option<&str> {
        self.hovered_block.as_deref()
    }

    /// The open chip: its label, buttons and where it was laid out.
    #[cfg(feature = "ui-shots")]
    pub fn chip_shot(&self) -> Option<(String, [&'static str; 3], Bounds<Pixels>)> {
        let chip = self.chip(&self.core.theme().ui())?;
        let gutter = self.gutter.borrow();
        let (block, _, _) = gutter.drawn(self.hovered_block.as_deref()?)?;
        let label = block_view::chip_label(block.outcome, block.cmd.as_deref());
        Some((label, chip.buttons, self.chip_bounds.get()?))
    }

    /// Send `bytes` to the program as a query reply would go (not typed: no snap, no hide).
    #[cfg(feature = "ui-shots")]
    pub fn write_to_program(&self, bytes: &[u8]) {
        self.core.write(bytes);
    }

    /// Where the grid was last painted (its cells, without the padding).
    #[cfg(feature = "ui-shots")]
    pub fn grid_bounds(&self) -> Option<Bounds<Pixels>> {
        let l = self.layout.get()?;
        Some(Bounds::new(
            l.origin,
            size(
                l.cell_width * l.dims.cols as f32,
                l.line_height * l.dims.rows as f32,
            ),
        ))
    }

    /// The selected text, if any.
    #[cfg(feature = "ui-shots")]
    pub fn selection_text(&self) -> Option<String> {
        self.core.selection_text()
    }

    /// Every block's id, oldest first.
    #[cfg(feature = "ui-shots")]
    pub fn block_ids(&self) -> Vec<String> {
        self.core.blocks().into_iter().map(|b| b.id).collect()
    }

    /// Its terminal (what a dragged block carries).
    #[cfg(feature = "ui-shots")]
    pub fn shot_core(&self) -> TerminalCore {
        self.core.clone()
    }

    /// Every block, oldest first.
    #[cfg(feature = "ui-shots")]
    pub fn shot_blocks(&self) -> Vec<Block> {
        self.core.blocks()
    }

    /// Lines the view is scrolled up from the live bottom.
    #[cfg(feature = "ui-shots")]
    pub fn display_offset(&self) -> usize {
        self.core.display_offset()
    }

    /// The open chip's buttons: where each was laid out, which are enabled, and whether
    /// Rerun is (or why not).
    #[cfg(feature = "ui-shots")]
    pub fn chip_buttons_shot(&self) -> Option<ChipButtonsShot> {
        let chip = self.chip(&self.core.theme().ui())?;
        let enabled = block_view::chip_enabled(chip.cmd.is_some(), chip.has_output, chip.rerun);
        Some((self.chip_button_bounds.get(), enabled, chip.rerun))
    }

    /// Where the open chip's command (the block's drag handle) was laid out.
    #[cfg(feature = "ui-shots")]
    pub fn chip_command_shot(&self) -> Option<Bounds<Pixels>> {
        self.chip(&self.core.theme().ui())?.cmd?;
        self.chip_command_bounds.get()
    }

    /// The bytes sent as input since the last call.
    #[cfg(feature = "ui-shots")]
    pub fn take_sent(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.sent)
    }

    /// The redraws the cursor asked for at its last paint.
    #[cfg(feature = "ui-shots")]
    pub fn cursor_frames(&self) -> Frames {
        self.motion.borrow().frames
    }

    /// Make the cursor's last change `ago` old, as if nothing had touched it since (the breath's
    /// rest, M11n, without waiting five minutes).
    #[cfg(feature = "ui-shots")]
    pub fn age_breath(&self, ago: Duration) {
        self.motion.borrow_mut().changed = Instant::now()
            .checked_sub(ago)
            .expect("the clock goes back that far");
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

    /// The block picked with ⇧⌘↑ / ⇧⌘↓ or a click on its bar (M11l Task 5), by id.
    #[cfg(feature = "ui-shots")]
    pub fn chosen_block(&self) -> Option<&str> {
        self.chosen_block.as_deref()
    }

    /// Pick block `id` (or none): its bar runs its whole height in the accent.
    pub fn set_chosen_block(&mut self, id: Option<String>, cx: &mut Context<Self>) {
        if self.chosen_block != id {
            self.chosen_block = id;
            cx.notify();
        }
    }

    /// Your own key or paste (not a script's, nor Rerun's paste): the chip closes and the picked
    /// block is dropped (M11l §4).
    fn user_typed(&mut self) {
        self.hide_chip();
        self.chosen_block = None;
        // In card view the column goes back to its bottom, where the typing shows (M11ab).
        self.cards.borrow_mut().typed();
    }

    /// Close the chip now (typing, a press or a scroll in the grid). True if it was open.
    fn hide_chip(&mut self) -> bool {
        self.chip_hide = None;
        self.hovered_block.take().is_some()
    }

    /// Follow the pointer over the gutter: over a block's bar, open its chip; over the chip,
    /// keep it; anywhere else, close it `CHIP_HIDE_MS` later. `can_hover`: the pointer is over
    /// this pane with no button down.
    fn follow_gutter(&mut self, pos: gpui::Point<Pixels>, can_hover: bool, cx: &mut Context<Self>) {
        let over = match can_hover && cx.global::<Appearance>().command_blocks {
            true => self.gutter.borrow().hit(pos).map(|b| b.id.clone()),
            false => None,
        };
        let on_chip = self.hovered_block.is_some()
            && self.chip_bounds.get().is_some_and(|b| b.contains(&pos));
        if let Some(id) = over {
            self.chip_hide = None;
            if self.hovered_block.as_ref() != Some(&id) {
                self.hovered_block = Some(id);
                cx.notify();
            }
        } else if on_chip {
            self.chip_hide = None;
        } else if self.hovered_block.is_some() && self.chip_hide.is_none() {
            self.chip_hide = Some(cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(Duration::from_millis(CHIP_HIDE_MS))
                    .await;
                let _ = this.update(cx, |this, cx| {
                    if this.hide_chip() {
                        cx.notify();
                    }
                });
            }));
        }
    }

    /// The hovered block's chip, from the last paint's gutter: none when the block isn't drawn
    /// (scrolled off, the alternate screen, `command-blocks` off).
    fn chip(&self, ui: &UiColors) -> Option<Chip> {
        let id = self.hovered_block.as_deref()?;
        let gutter = self.gutter.borrow();
        let (block, cmd_top, run) = gutter.drawn(id)?;
        let (pane, grid, (cell_width, height)) = (gutter.pane, gutter.grid, gutter.cell);
        let left = grid.left() + px(CHIP_GAP);
        let top = block_view::chip_top(
            f32::from(cmd_top),
            f32::from(grid.top()),
            f32::from(grid.bottom()),
            f32::from(height),
        );
        // Up to a cell before a right prompt on the command row (the grid's edge without one),
        // never narrower than its least (over the prompt then), never past the grid.
        let limit = block_view::chip_limit(
            f32::from(grid.left()),
            f32::from(grid.right()),
            f32::from(cell_width),
            run,
        );
        let room = limit - f32::from(left);
        let rerun = block_view::rerun(self.core.rerun_facts(id), block.cmd.as_deref()).map(|_| ());
        Some(Chip {
            id: id.to_string(),
            rerun,
            outcome: block_view::chip_outcome(block.outcome)
                .map(|o| (o, block_view::outcome_ink(block.outcome, ui))),
            cmd: block.cmd.as_deref().map(block_view::chip_command),
            drag: block.cmd.clone().map(|command| BlockDrag {
                id: id.to_string(),
                command: Some(command),
                outcome: block.outcome,
                core: self.core.clone(),
            }),
            has_output: block.output.is_some(),
            buttons: block_view::chip_buttons(f32::from(grid.size.width), room),
            left: left - pane.left(),
            top: px(top) - pane.top(),
            height,
            max_width: px(block_view::chip_max_width(
                room,
                f32::from(grid.right() - left),
            )),
        })
    }

    /// The status of this terminal's session, for the cursor's colour; redraws on a change.
    /// Its folder's branch (M11ab: a chip above the live box); the workspace pushes it.
    pub fn set_place(
        &mut self,
        folder: Option<std::path::PathBuf>,
        branch: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if self.branch != branch || self.folder != folder {
            self.branch = branch;
            self.folder = folder;
            cx.notify();
        }
    }

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

    /// Start the breathing timer unless it runs. It sleeps through the hold (`breath_sleep`),
    /// then wakes only for the breath's frames (its level crossings, trough and peak) and
    /// redraws at each, while the last paint asked for it (`Frames::Breath`) and the terminal
    /// is focused in the key window. It stops, without a redraw, as soon as any of that ends,
    /// and by itself once the breath rests (after drawing that peak); the next paint that wants
    /// the breath again (any change ends the rest) starts it anew.
    fn breathe(&mut self, window: &Window, cx: &mut Context<Self>) {
        if self.breathing {
            return;
        }
        self.breathing = true;
        let mut wait = breath_sleep(self.motion.borrow().changed.elapsed());
        cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor().timer(wait).await;
                let next = this
                    .update_in(cx, |this, window, cx| {
                        let motion = this.motion.borrow();
                        let go = motion.frames == Frames::Breath
                            && this.focus_handle.is_focused(window)
                            && this.key_window(window);
                        let since = motion.changed.elapsed();
                        drop(motion);
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
        let (cw, lh) = (f32::from(layout.cell_width), f32::from(layout.line_height));
        self.motion.borrow_mut().spark(cw, lh, Instant::now());
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
        self.paste_with(text, bracketed, cx);
    }

    /// Paste text, bracketed exactly when `bracketed` (the mode as the caller read it).
    pub fn paste_with(&mut self, text: &str, bracketed: bool, cx: &mut Context<Self>) {
        self.send_paste(&keys::paste_bytes(text, bracketed), cx);
    }

    /// Send pasted bytes as input (a paste by script or Rerun: the chip and the picked block
    /// stay).
    fn send_paste(&mut self, bytes: &[u8], cx: &mut Context<Self>) {
        self.input_bytes(bytes, cx);
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
            cwd_seen: core.cwd_changes(),
            core,
            focus_handle,
            tracker: StatusTracker::new(),
            run: CommandRun::default(),
            integration: None,
            marks_seen: 0,
            cards: Rc::default(),
            cards_on: Rc::default(),
            card_line_h: Rc::default(),
            card_cell: Rc::default(),
            card_region: None,
            branch: None,
            folder: None,
            tick_pending: false,
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
            gutter: Rc::new(RefCell::new(GutterPaint::default())),
            hovered_block: None,
            chosen_block: None,
            chip_bounds: Rc::new(Cell::new(None)),
            #[cfg(feature = "ui-shots")]
            chip_button_bounds: Rc::new(Cell::new([None; 3])),
            #[cfg(feature = "ui-shots")]
            chip_command_bounds: Rc::new(Cell::new(None)),
            #[cfg(feature = "ui-shots")]
            sent: Vec::new(),
            chip_hide: None,
            #[cfg(feature = "ui-shots")]
            shot_key_window: Cell::new(false),
            _events: events,
            _ticker: ticker,
            _appearance: appearance,
        }
    }

    /// A mark changed what runs: tell the status tracker and the owner, and redraw (the chrome
    /// shows the name). The order the owner hears them in matters: a start's status change (a
    /// Done seen, as on a keystroke) comes before the command; an end's `Command` comes before
    /// its Done, so the owner settles the status knowing which command ended, and the tracker's
    /// own Done then changes nothing.
    fn command_marked(&mut self, mark: CommandMark, cx: &mut Context<Self>) {
        // A background-service session shows it was started with the marks (ruling 3): a real
        // mark, or at its snapshot a tag already in its grid (final review #2).
        let core = self.core.clone();
        self.integration = integration_after(&mark, self.integration, || {
            core.read_term(|term, _| !crate::blocks::segments(term).is_empty())
        });
        self.marks_seen += 1;
        let now = Instant::now();
        let tracks = self.tracks_status();
        match self.run.on_mark(mark, now) {
            Some(change @ Change::Started(_)) => {
                if tracks && let Some(status) = self.tracker.on_command_start(now) {
                    self.status_changed(status, cx);
                }
                cx.emit(TerminalEvent::Command(change));
            }
            Some(Change::Ended(finished)) => {
                let settled = tracks
                    .then(|| self.tracker.on_command_end(now, finished.ran))
                    .flatten();
                cx.emit(TerminalEvent::Command(Change::Ended(finished)));
                if let Some(status) = settled {
                    self.status_changed(status, cx);
                }
            }
            Some(change @ Change::Cleared) => cx.emit(TerminalEvent::Command(change)),
            None => {}
        }
        cx.notify();
    }

    fn on_term_event(&mut self, event: TermEvent, cx: &mut Context<Self>) {
        match event {
            TermEvent::Output { notifications } => {
                // First, before anything renders: tell the parser thread this `Output` has been
                // heard. A batch it finishes before this is in the frame `cx.notify` asks for
                // below (the render takes the Term lock after it); one it finishes after sends
                // a new `Output` (M11n's gate), so the screen is never left an older frame.
                self.core.output_seen();
                // A `cd` (the folder report changed): the sidebar looks it up now.
                let moved = self.core.cwd_changes();
                if moved != self.cwd_seen {
                    self.cwd_seen = moved;
                    cx.emit(TerminalEvent::FolderMoved);
                }
                // The program wrote: the breath holds.
                self.motion.borrow_mut().changed = Instant::now();
                self.hear_of_resize();
                if self.tracks_status()
                    && let Some(status) = self.tracker.on_output(Instant::now(), notifications)
                {
                    self.status_changed(status, cx);
                }
                // Link spans are absolute grid lines that shift as output scrolls; the next
                // mouse move recomputes the hover. (The chip closes in paint, only if the
                // output moved its block.)
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
            TermEvent::Command(mark) => self.command_marked(mark, cx),
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
        // A press (not a held key's repeats) of a key with a sound plays it (M11s).
        let pressed = !event.is_held;
        if let Some(bytes) =
            keys::mac_shortcut(&keystroke.key, m.platform, m.alt, m.control, m.shift)
        {
            if pressed {
                crate::sounds::press(bytes, cx);
            }
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
            if pressed {
                crate::sounds::press(&bytes, cx);
            }
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
        self.user_typed();
        self.input_bytes(bytes, cx);
        cx.stop_propagation();
    }

    /// Input from the user or a script: write to the pty and tell the tracker it was typing.
    /// Held (disconnected): dropped, there is no program to take it.
    fn input_bytes(&mut self, bytes: &[u8], cx: &mut Context<Self>) {
        if self.disconnected {
            return;
        }
        #[cfg(feature = "ui-shots")]
        self.sent.extend_from_slice(bytes);
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
    fn report(&mut self, bytes: &[u8]) {
        if !self.disconnected {
            // (The scenes read the mouse reports with what was typed.)
            #[cfg(feature = "ui-shots")]
            self.sent.extend_from_slice(bytes);
            self.core.write(bytes);
        }
    }

    /// Grid point, cell side and viewport col/row under a window position, or `None` before the
    /// first paint. Always in range: `grid_point` clamps to the painted dims and the live offset.
    fn point_at(&self, pos: gpui::Point<Pixels>) -> Option<(Point, Side, usize, usize)> {
        if self.cards_on.get() {
            return self.card_point_at(pos).map(|(point, side, _)| {
                let row = point.line.0.max(0) as usize;
                (point, side, point.column.0, row)
            });
        }
        let layout = self.layout.get()?;
        let display_offset = self.core.display_offset();
        let (point, side) = layout.grid_point(pos, display_offset);
        let row = (point.line.0 + display_offset as i32) as usize;
        Some((point, side, point.column.0, row))
    }

    /// In card view (M11ab): the grid point under `pos` in the rows painted last frame, the
    /// selection's own region while selecting (so it stays in one card), and whether that
    /// region is the live box.
    fn card_point_at(&self, pos: gpui::Point<Pixels>) -> Option<(Point, Side, bool)> {
        let cards = self.cards.borrow();
        let (x, y) = (f32::from(pos.x), f32::from(pos.y));
        // Held while a selection or the program's mouse press lasts: a drag or a release over a
        // gap or the chips still lands in the region it started in (final review #1).
        let held = self.selecting || self.mouse_reported;
        let region = match (&self.card_region, held) {
            (Some(key), true) => cards.regions.iter().find(|r| &r.key == key),
            _ => cards.regions.iter().find(|r| y >= r.y && y < r.bottom),
        }?;
        let cell = self.card_cell.get();
        let cols = self.core.dims().cols;
        let (point, side) =
            crate::card_view::card_point((x, y), (region.x, region.y), region.top, cell, cols);
        // Kept to the region's own rows.
        let rows = ((region.bottom - region.y) / cell.1).round().max(1.0) as i32;
        let line = point.line.max(region.top).min(region.top + (rows - 1));
        Some((Point::new(line, point.column), side, region.live))
    }

    fn on_scroll_wheel(
        &mut self,
        e: &ScrollWheelEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // M11ab: in card view the wheel scrolls the column (points; up is positive), unless the
        // program in the live box asked for the wheel and the pointer is over the box.
        let over_live = self.cards_on.get()
            && self
                .card_point_at(e.position)
                .is_some_and(|(_, _, live)| live);
        if self.cards_on.get()
            && !(over_live && mouse::wants_mouse(self.core.mode(), e.modifiers.shift))
        {
            let line_h = self.card_line_h.get();
            if line_h > 0.0 {
                let delta = f32::from(e.delta.pixel_delta(px(line_h)).y);
                let mut cards = self.cards.borrow_mut();
                cards.scroll = (cards.scroll + delta).clamp(0.0, cards.max_scroll);
                cx.notify();
            }
            return;
        }
        let line_height = match self.layout.get() {
            Some(layout) => f32::from(layout.line_height),
            None if self.cards_on.get() => self.card_line_h.get(),
            None => return,
        };
        if line_height <= 0.0 {
            return;
        }
        if e.touch_phase == TouchPhase::Started {
            self.scroll.reset();
        }
        let delta = f32::from(e.delta.pixel_delta(px(line_height)).y);
        let lines = self.scroll.push(delta, line_height);
        if lines == 0 {
            return;
        }
        // The blocks move (or the program has the wheel): the chip closes.
        self.hide_chip();
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
        // A press on a block's bar picks that block (no scroll), and never reaches the
        // selection or the program.
        if cx.global::<Appearance>().command_blocks {
            let hit = self.gutter.borrow().hit(e.position).map(|b| b.id.clone());
            if let Some(id) = hit {
                self.set_chosen_block(Some(id), cx);
                return;
            }
        }
        // A press in the grid closes the chip and drops the picked block.
        if self.hide_chip() | self.chosen_block.take().is_some() {
            cx.notify();
        }
        let Some((point, side, col, row)) = self.point_at(e.position) else {
            return;
        };
        // In card view a selection stays in the region it starts in, and only the live box
        // reports the mouse to its program (M11ab).
        let card = self
            .cards_on
            .get()
            .then(|| self.card_point_at(e.position))
            .flatten();
        let card_live = card.is_none_or(|(_, _, live)| live);
        if card.is_some() {
            let (x, y) = (f32::from(e.position.x), f32::from(e.position.y));
            self.card_region = self
                .cards
                .borrow()
                .regions
                .iter()
                .find(|r| y >= r.y && y < r.bottom && x >= r.x - 1.0)
                .map(|r| r.key.clone());
        }
        let m = &e.modifiers;
        if e.button == MouseButton::Left && m.platform {
            self.pressed_link = self.core.link_at(point);
            if self.pressed_link.is_some() {
                return;
            }
        }
        let mode = self.core.mode();
        if mouse::wants_mouse(mode, m.shift) && card_live {
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
        self.follow_gutter(e.position, over_pane && e.pressed_button.is_none(), cx);
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
        self.card_region = None;
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
        self.user_typed();
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
        // In card view the column scrolls, by a view's height (final review #6).
        if self.cards_on.get() {
            let mut cards = self.cards.borrow_mut();
            let up = matches!(scroll, Scroll::PageUp);
            cards.scroll = crate::card_view::page(cards.scroll, cards.view, cards.max_scroll, up);
            drop(cards);
            cx.notify();
            return;
        }
        self.core.scroll(scroll);
        cx.notify();
    }

    /// The command-block actions do something: `command-blocks` is on and the alternate screen
    /// isn't up (M11l §4).
    fn blocks_active(&self, cx: &App) -> bool {
        cx.global::<Appearance>().command_blocks && !self.core.mode().contains(TermMode::ALT_SCREEN)
    }

    /// The block an action applies to (M11l §4): the chosen one, else the newest finished
    /// one. A chosen block that is gone is dropped, and nothing is done. Reads every block:
    /// on a keypress only.
    /// ⇧⌘↑ / ⇧⌘↓ in card view (M11ab, as M11l's jump): the card before or after the chosen
    /// one (none chosen: up picks the newest), scrolled into view; past the newest, none chosen
    /// and the column at its bottom.
    fn jump_card(&mut self, up: bool, cx: &mut Context<Self>) {
        let mut cards = self.cards.borrow_mut();
        let chosen = self
            .chosen_block
            .as_deref()
            .and_then(|id| cards.order.iter().position(|(c, ..)| c == id));
        let next = match (chosen, up) {
            (None, true) => cards.order.len().checked_sub(1),
            (None, false) => return,
            (Some(i), true) => i.checked_sub(1).or(Some(i)),
            (Some(i), false) if i + 1 < cards.order.len() => Some(i + 1),
            (Some(_), false) => None,
        };
        match next {
            Some(i) => {
                let (id, top, height) = cards.order[i].clone();
                cards.scroll =
                    crate::card_view::reveal(top, height, cards.total, cards.view, cards.scroll);
                drop(cards);
                self.chosen_block = Some(id);
            }
            None => {
                cards.typed();
                drop(cards);
                self.chosen_block = None;
            }
        }
        cx.notify();
    }

    /// The composer's context row (rework), over its outer box from the last frame: the running
    /// command (peach), the folder and the branch with the app's own icons, no ▾s (chores). The
    /// folder is a plain label (opening Finder from it was out of place, your call); a click on
    /// the branch opens the branch switcher. None when the box is out of view.
    fn context_row(&self, ui: &UiColors, cx: &mut Context<Self>) -> Option<gpui::AnyElement> {
        use crate::card_view::{CTX_ROW, EDGE, PILL_GAP, PILL_H, PILL_MAX, TAB_PAD};
        use crate::icons::{Icon, icon};
        let (composer, (ox, oy), view) = {
            let cards = self.cards.borrow();
            (cards.composer, cards.origin, cards.view_rect)
        };
        let (x, y, w) = composer?;
        if y < view.1 || y + CTX_ROW > view.1 + view.3 {
            return None;
        }
        let dim = hsla(ui.text_dim);
        // A pill (the designer's details): 22 tall, 8 in, round, on `composer_inner`.
        let pill = || {
            div()
                .flex()
                .flex_none()
                .items_center()
                .gap(px(5.0))
                .h(px(PILL_H))
                .px(px(8.0))
                .rounded(px(PILL_H / 2.0))
                .bg(hsla(ui.composer_inner))
        };
        // The running command first: its dot the composer's one colour.
        let running = self.run.started().map(|at| {
            let short = self.run.running().unwrap_or(crate::command_name::NO_SHORT);
            let words = crate::card_view::running_words(short, at.elapsed());
            let words = words.trim_start_matches("\u{25cf} ").to_string();
            pill()
                .child(
                    div()
                        .flex_none()
                        .size(px(6.0))
                        .rounded_full()
                        .bg(hsla(ui.ring_working)),
                )
                .child(words)
        });
        let folder_item = self.folder.clone().and_then(|path| {
            let name = path.file_name()?.to_string_lossy().to_string();
            // Its whole path on hover, as the app writes folders (`~/…`; chores: "on folder
            // information, give tooltip"). Its name gives way first when the row is too wide.
            let home = crate::workspace::home_dir();
            let tip = crate::sidebar::home_relative(&path, home.as_deref());
            Some(
                pill()
                    .flex_shrink(1.0)
                    .min_w(px(0.0))
                    .max_w(px(PILL_MAX))
                    .id("composer-folder")
                    .tooltip(move |_, cx| -> AnyView { cx.new(|_| Tip(tip.clone().into())).into() })
                    .child(icon(Icon::Folder, px(13.0), dim))
                    .child(div().min_w(px(0.0)).truncate().child(name)),
            )
        });
        let branch_item = self.branch.clone().map(|branch| {
            let hover = hsla(ui.composer_pill_hover);
            let ink = hsla(ui.text);
            // Its whole name on hover (it ends in "…" past `PILL_MAX`).
            let tip = branch.clone();
            pill()
                .flex_shrink(1.0)
                .min_w(px(0.0))
                .max_w(px(PILL_MAX))
                .id("composer-branch")
                .cursor_pointer()
                .hover(move |s| s.bg(hover).text_color(ink))
                .tooltip(move |_, cx| -> AnyView { cx.new(|_| Tip(tip.clone().into())).into() })
                .child(icon(Icon::Branch, px(13.0), dim))
                .child(div().min_w(px(0.0)).truncate().child(branch))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|_, _: &MouseDownEvent, window, cx| {
                        cx.stop_propagation();
                        window.dispatch_action(Box::new(crate::workspace::SwitchBranch), cx);
                    }),
                )
        });
        // The row's laid-out width, for the tab painted round it next frame (`pills_w`).
        let cards = self.cards.clone();
        let measure = canvas(
            move |bounds, window, _| {
                let width = f32::from(bounds.size.width);
                let mut state = cards.borrow_mut();
                if (state.pills_w - width).abs() > 0.5 {
                    state.pills_w = width;
                    window.request_animation_frame();
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .top_0()
        .left_0()
        .size_full();
        let inset = EDGE + TAB_PAD;
        Some(
            div()
                .absolute()
                .left(px(x - ox + inset))
                .top(px(y - oy + TAB_PAD))
                .max_w(px((w - 48.0 - 2.0 * inset).max(0.0)))
                .h(px(PILL_H))
                .flex()
                .items_center()
                .gap(px(PILL_GAP))
                .overflow_hidden()
                .text_size(px(12.0))
                .text_color(dim)
                .children(running)
                .children(folder_item)
                .children(branch_item)
                .child(measure)
                .into_any_element(),
        )
    }

    /// A card's Rerun (M11ab): M11l's Rerun (types it, never runs it), and the column back at
    /// its bottom, where it types (final review).
    pub fn rerun_card(&mut self, id: &str, cx: &mut Context<Self>) {
        self.rerun_block(id, cx);
        self.cards.borrow_mut().typed();
        cx.notify();
    }

    /// A card's elements over its painted self (M11ab), from the last frame's geometry: Copy
    /// and Rerun at its header's right on hover, its command line as M11aa's drag handle onto
    /// an agent tab, its fold row a button.
    fn card_overlay(
        &self,
        hit: crate::card_view::CardHit,
        (ox, oy): (f32, f32),
        ui: &UiColors,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        use crate::card_view::HEADER_H;
        let (x, y, w, h) = hit.card;
        let group = SharedString::from(format!("card-{}", hit.id));
        let button = |label: &'static str, key: &str| {
            div()
                .id(SharedString::from(format!("card-{label}-{key}")))
                .px(px(6.0))
                .rounded(px(5.0))
                .text_size(px(crate::ui_tokens::text::META))
                .text_color(hsla(ui.text_dim))
                .cursor_pointer()
                .hover(|s| s.bg(hsla(ui.panel_hover)))
                .child(label)
        };
        let (copy_id, rerun_id) = (hit.id.clone(), hit.id.clone());
        let actions = div()
            .absolute()
            .top_0()
            .right_0()
            .h(px(HEADER_H))
            .flex()
            .items_center()
            .gap(px(4.0))
            .invisible()
            .group_hover(group.clone(), |s| s.visible())
            .child(button("Copy", &hit.id).on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    this.copy_block(copy_id.clone(), false, cx);
                }),
            ))
            .child(button("Rerun", &hit.id).on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    this.rerun_card(&rerun_id, cx);
                }),
            ));
        let (cx0, cy0, cw, ch) = hit.cmd;
        let drag = BlockDrag {
            id: hit.id.clone(),
            command: hit.command.clone(),
            outcome: hit.outcome,
            core: self.core.clone(),
        };
        let words: SharedString = drag_words(&drag).into();
        let handle = div()
            .id(SharedString::from(format!("card-cmd-{}", hit.id)))
            .absolute()
            .left(px(cx0 - x))
            .top(px(cy0 - y))
            .w(px(cw))
            .h(px(ch))
            .on_drag(drag, move |_, _, window, cx| {
                crate::tab_bar::hold_closed_hand(window, cx);
                let words = words.clone();
                cx.new(|_| BlockDragChip { words })
            });
        let fold = hit.fold.map(|(fx, fy, fw, fh)| {
            let id = hit.id.clone();
            div()
                .id(SharedString::from(format!("card-fold-{}", hit.id)))
                .absolute()
                .left(px(fx - x))
                .top(px(fy - y))
                .w(px(fw))
                .h(px(fh))
                .cursor_pointer()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        let mut cards = this.cards.borrow_mut();
                        if !cards.unfolded.remove(&id) {
                            cards.unfolded.insert(id.clone());
                        }
                        drop(cards);
                        cx.notify();
                    }),
                )
        });
        div()
            .absolute()
            .left(px(x - ox))
            .top(px(y - oy))
            .w(px(w))
            .h(px(h))
            .group(group)
            .child(actions)
            .child(handle)
            .children(fold)
            .into_any_element()
    }

    fn action_block(&mut self, cx: &mut Context<Self>) -> Option<Block> {
        let blocks = self.core.blocks();
        let (chosen, gone) = chosen_index(&blocks, self.chosen_block.as_deref());
        if gone {
            self.set_chosen_block(None, cx);
            return None;
        }
        let newest_done = blocks.iter().rposition(|b| b.outcome != Outcome::Running);
        let i = block_view::target(blocks.len(), chosen, newest_done)?;
        blocks.into_iter().nth(i)
    }

    /// ⇧⌘↑ / ⇧⌘↓: pick the previous / next block, scrolling its first prompt row to the top
    /// unless it is all in view; past the newest, back to the bottom with nothing picked.
    fn jump_to(&mut self, up: bool, cx: &mut Context<Self>) {
        if !self.blocks_active(cx) {
            return;
        }
        if self.cards_on.get() {
            self.jump_card(up, cx);
            return;
        }
        let (blocks, (top, bottom)) = self.core.blocks_and_view();
        let rows: Vec<_> = blocks
            .iter()
            .map(|b| {
                let last = b.output.map_or(b.cmd_row, |(_, last)| last.max(b.cmd_row));
                (b.prompt_top, b.prompt_top >= top && last <= bottom)
            })
            .collect();
        // A chosen block that is gone is dropped, and counts as none.
        let (chosen, gone) = chosen_index(&blocks, self.chosen_block.as_deref());
        if gone {
            self.set_chosen_block(None, cx);
        }
        match block_view::jump(&rows, chosen, up) {
            Jump::To(i) => {
                let (prompt_top, in_view) = rows[i];
                if !in_view {
                    self.core.scroll_to_line(prompt_top);
                }
                self.chosen_block = Some(blocks[i].id.clone());
            }
            Jump::Bottom => {
                self.chosen_block = None;
                self.core.scroll(Scroll::Bottom);
            }
            Jump::Stay => return,
        }
        cx.notify();
    }

    fn prev_command(&mut self, _: &PrevCommand, _window: &mut Window, cx: &mut Context<Self>) {
        self.jump_to(true, cx);
    }

    fn next_command(&mut self, _: &NextCommand, _window: &mut Window, cx: &mut Context<Self>) {
        self.jump_to(false, cx);
    }

    fn copy_command_output(
        &mut self,
        _: &CopyCommandOutput,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.blocks_active(cx)
            && let Some(block) = self.action_block(cx)
        {
            self.copy_block(block.id, false, cx);
        }
    }

    fn copy_command(&mut self, _: &CopyCommand, _window: &mut Window, cx: &mut Context<Self>) {
        if self.blocks_active(cx)
            && let Some(block) = self.action_block(cx)
            && let Some(cmd) = block.cmd
        {
            cx.write_to_clipboard(ClipboardItem::new_string(cmd));
        }
    }

    fn copy_command_and_output(
        &mut self,
        _: &CopyCommandAndOutput,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.blocks_active(cx)
            && let Some(block) = self.action_block(cx)
            && block.cmd.is_some()
        {
            self.copy_block(block.id, true, cx);
        }
    }

    fn rerun_command(&mut self, _: &RerunCommand, _window: &mut Window, cx: &mut Context<Self>) {
        if self.blocks_active(cx)
            && let Some(block) = self.action_block(cx)
        {
            self.rerun_block(&block.id, cx);
        }
    }

    /// Copy block `id`'s output (`with_command`: `$ <command>`, a newline, then it) to the
    /// clipboard. The text is read off the UI thread (the Term lock held only for the read),
    /// then written; nothing when the block is gone (or, with the command, has none), or when
    /// there is no text (no output): the clipboard keeps what it had.
    fn copy_block(&self, id: String, with_command: bool, cx: &mut Context<Self>) {
        let core = self.core.clone();
        let text = cx.background_spawn(async move {
            let output = core.block_output(&id)?;
            match with_command {
                true => Some(block_view::with_command(&core.block_cmd(&id)?, &output)),
                false => Some(output),
            }
        });
        cx.spawn(async move |_, cx| {
            if let Some(text) = block_view::clipboard_text(text.await) {
                cx.update(|cx| cx.write_to_clipboard(ClipboardItem::new_string(text)));
            }
        })
        .detach();
    }

    /// Rerun block `id`: type its command at the prompt, never pressing Return, when every
    /// guard allows it (`block_view::rerun_bytes`); otherwise nothing. Pasted like a script's
    /// paste (bracketed when the shell asked), so the chip and the picked block stay.
    /// Type `cd -- '<folder>'` and Return, if the shell is at an empty prompt (`cd_bytes`);
    /// false when it isn't (nothing is typed).
    pub fn cd_to(&mut self, folder: &std::path::Path, cx: &mut Context<Self>) -> bool {
        let Some(folder) = folder.to_str() else {
            return false;
        };
        match block_view::cd_bytes(folder, self.core.prompt_facts()) {
            Ok(bytes) => {
                self.send_paste(&bytes, cx);
                true
            }
            Err(_) => false,
        }
    }

    fn rerun_block(&mut self, id: &str, cx: &mut Context<Self>) {
        let (facts, cmd) = self.core.rerun_facts_and_cmd(id);
        // Bracketed exactly as the facts that allowed it said: a `?2004l` arriving since can't
        // turn a newline into a Return.
        if let Ok(bytes) = block_view::rerun_bytes(facts, cmd.as_deref()) {
            self.send_paste(&bytes, cx);
        }
    }

    /// A chip button for block `id`: Copy command, Copy output or Rerun (the chip stays, the
    /// picked block is kept).
    fn chip_button(&mut self, button: usize, id: &str, cx: &mut Context<Self>) {
        if !self.blocks_active(cx) {
            return;
        }
        match button {
            0 => {
                if let Some(cmd) = self.core.block_cmd(id) {
                    cx.write_to_clipboard(ClipboardItem::new_string(cmd));
                }
            }
            1 => self.copy_block(id.to_string(), false, cx),
            _ => self.rerun_block(id, cx),
        }
    }
}

/// Where the block `chosen` is in `blocks`, and whether it was chosen but is gone.
fn chosen_index(blocks: &[Block], chosen: Option<&str>) -> (Option<usize>, bool) {
    match chosen {
        None => (None, false),
        Some(id) => match blocks.iter().position(|b| b.id == id) {
            Some(i) => (Some(i), false),
            None => (None, true),
        },
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
        let ui = theme.ui();
        let blocks_on = appearance.command_blocks;
        // M11ab: what card view needs that's known here; the width is known at prepaint.
        let shell_cards = appearance.shell_cards && blocks_on;
        let integrated = self.integrated();
        let alt_screen = self.core.mode().contains(TermMode::ALT_SCREEN);
        let marks_seen = self.marks_seen;
        let utc_offset = crate::cli::local_utc_offset(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_millis() as u64),
        );
        let cards_state = self.cards.clone();
        let cards_on = self.cards_on.clone();
        let card_line_h = self.card_line_h.clone();
        let card_cell = self.card_cell.clone();
        let card_core = self.core.clone();
        let was_cards = self.cards_on.get();
        let chosen_card = self.chosen_block.clone();
        // The running chip's time moves once a second (only while one runs in card view).
        if was_cards && self.run.started().is_some() && !self.tick_pending {
            self.tick_pending = true;
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                let _ = this.update(cx, |this, cx| {
                    this.tick_pending = false;
                    cx.notify();
                });
            })
            .detach();
        }
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
                &ui,
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
        let gutter = self.gutter.clone();
        let chosen = self.chosen_block.clone().filter(|_| blocks_on);
        let hovered_block = self.hovered_block.clone().filter(|_| blocks_on);
        let chip = if blocks_on && !was_cards {
            self.chip(&ui)
        } else {
            None
        };
        if chip.is_none() {
            self.chip_bounds.set(None);
        }
        let chip = chip.map(|chip| {
            chip_element(
                chip,
                &ui,
                ChipSlots {
                    chip: self.chip_bounds.clone(),
                    #[cfg(feature = "ui-shots")]
                    buttons: self.chip_button_bounds.clone(),
                    #[cfg(feature = "ui-shots")]
                    command: self.chip_command_bounds.clone(),
                },
                cx,
            )
        });

        let ctx_row = if was_cards {
            self.context_row(&ui, cx)
        } else {
            None
        };
        let overlays: Vec<gpui::AnyElement> = if was_cards {
            let (hits, origin, view) = {
                let cards = self.cards.borrow();
                (cards.hits.clone(), cards.origin, cards.view_rect)
            };
            // Cut to the column's view (final review #5): nothing over the padding or the bar.
            hits.iter()
                .filter_map(|hit| crate::card_view::clip_hit(hit, view))
                .map(|hit| self.card_overlay(hit, origin, &ui, cx))
                .collect()
        } else {
            Vec::new()
        };

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
            .on_action(cx.listener(Self::prev_command))
            .on_action(cx.listener(Self::next_command))
            .on_action(cx.listener(Self::copy_command_output))
            .on_action(cx.listener(Self::copy_command))
            .on_action(cx.listener(Self::copy_command_and_output))
            .on_action(cx.listener(Self::rerun_command))
            .child(
                canvas(
                    // prepaint: measure cells, fit the terminal inside the padding of the space
                    // we were given, remember the layout for hit-testing. The hitbox is the
                    // whole pane: a click in the padding lands on the nearest cell.
                    move |bounds, window, cx| {
                        let metrics = CellMetrics::measure(cx.global::<Appearance>(), window);
                        let cards = crate::card_view::card_mode(
                            shell_cards,
                            integrated,
                            alt_screen,
                            f32::from(bounds.size.width),
                        );
                        cards_on.set(cards);
                        card_line_h.set(f32::from(metrics.line_height));
                        card_cell.set((
                            f32::from(metrics.cell_width),
                            f32::from(metrics.line_height),
                        ));
                        let grid = GridLayout::fit(
                            inset(bounds, split),
                            metrics.cell_width,
                            metrics.line_height,
                        );
                        let dims = pane_dims(
                            cards,
                            bounds,
                            split,
                            metrics.cell_width,
                            metrics.line_height,
                        );
                        let sent = resize_core.resize(
                            dims,
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
                        // In card view the grid isn't laid out over the pane (the cards and the
                        // live box place their rows).
                        layout.set((!cards).then_some(grid));
                        let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
                        (metrics, grid.origin, hitbox, cards)
                    },
                    // paint: draw the current screen, set the pointer, follow the mouse.
                    move |bounds, (metrics, origin, hitbox, cards), window, cx| {
                        if cards {
                            paint_cards(
                                CardPaint {
                                    core: &card_core,
                                    state: &cards_state,
                                    marks: marks_seen,
                                    utc_offset,
                                    split,
                                    theme: &theme,
                                    ui: &ui,
                                    chosen: chosen_card.clone(),
                                },
                                &metrics,
                                bounds,
                                &cursor,
                                &mut motion.borrow_mut(),
                                window,
                                cx,
                            );
                            let frames = motion.borrow().frames;
                            match frames {
                                Frames::EveryFrame => window.request_animation_frame(),
                                Frames::Breath => {
                                    view.update(cx, |this, cx| this.breathe(window, cx))
                                }
                                Frames::None => {}
                            }
                            window.set_cursor_style(CursorStyle::IBeam, &hitbox);
                            // The card view drags, selects and releases as today's view does
                            // (final review #1).
                            follow_mouse(&view, &hitbox, window);
                            return;
                        }
                        // With command blocks on, the blocks in view come with the screen,
                        // under the same lock (the viewport and 8 rows below, plus the chosen
                        // block); nothing is read for them otherwise.
                        let mut snap = match blocks_on {
                            true => paint_core.snapshot_with_blocks(chosen.as_deref()),
                            false => paint_core.snapshot(),
                        };
                        if let Some(link) = &hovered {
                            let offset = paint_core.display_offset();
                            grid::underline_cells(&mut snap, link.start, link.end, offset);
                        }
                        let look =
                            cursor.look(&snap, &theme, &mut motion.borrow_mut(), Instant::now());
                        let blocks = std::mem::take(&mut snap.blocks);
                        // The chip stops before a right prompt: each command row's cells, from
                        // this frame's screen (one row per block in view).
                        let runs = blocks
                            .iter()
                            .map(|b| {
                                let row = usize::try_from(b.cmd_row).ok()?;
                                block_view::right_run(snap.lines.get(row)?, |c| {
                                    (c.ch == ' ' || c.ch == '\0') && !c.spacer
                                })
                            })
                            .collect();
                        let marks = BlockMarks {
                            blocks,
                            runs,
                            dims: (snap.cols, snap.rows),
                            split,
                            hovered: hovered_block.as_deref(),
                        };
                        let mut painted = None;
                        paint_grid(
                            snap,
                            &theme,
                            &metrics,
                            bounds,
                            origin,
                            look,
                            |window| {
                                painted = Some(marks.paint(&ui, &metrics, bounds, origin, window))
                            },
                            window,
                            cx,
                        );
                        let painted = painted.unwrap_or_default();
                        // The chip stays while output leaves its block where it was; once
                        // the block moves (output scrolled it, a clear, a reflow) or isn't
                        // drawn any more, it closes. Compared by where the command row is
                        // drawn: the scrollback cap can rotate lines with no other sign. A right
                        // prompt that came or went on its row only lays the chip out again.
                        let (moved, run_changed) = match hovered_block.as_deref() {
                            Some(id) => {
                                let before = gutter.borrow().drawn(id).map(|(_, t, r)| (t, r));
                                let after = painted.drawn(id).map(|(_, t, r)| (t, r));
                                (before.map(|b| b.0) != after.map(|a| a.0), before != after)
                            }
                            None => (false, false),
                        };
                        *gutter.borrow_mut() = painted;
                        if moved {
                            view.update(cx, |this, cx| {
                                if this.hide_chip() {
                                    cx.notify();
                                }
                            });
                        } else if run_changed {
                            view.update(cx, |_, cx| cx.notify());
                        }
                        // The next redraw the cursor needs (none when still).
                        let frames = motion.borrow().frames;
                        match frames {
                            Frames::EveryFrame => window.request_animation_frame(),
                            Frames::Breath => view.update(cx, |this, cx| this.breathe(window, cx)),
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
                        follow_mouse(&view, &hitbox, window);
                    },
                )
                .size_full(),
            )
            .children(chip)
            .children(overlays)
            .children(ctx_row)
    }
}

/// Where the chip records its layout: its own bounds (for the hover) and, for the UI
/// screenshots, its buttons'.
struct ChipSlots {
    chip: Rc<Cell<Option<Bounds<Pixels>>>>,
    #[cfg(feature = "ui-shots")]
    buttons: Rc<Cell<[Option<Bounds<Pixels>>; 3]>>,
    /// The command's words (the drag handle).
    #[cfg(feature = "ui-shots")]
    command: Rc<Cell<Option<Bounds<Pixels>>>>,
}

/// What the chip that follows a dragged block says: its outcome and its command's first line,
/// as the block's own chip does.
fn drag_words(drag: &BlockDrag) -> String {
    block_view::chip_label(drag.outcome, drag.command.as_deref())
}

/// The block that follows the pointer while its command is dragged: a small panel with its
/// outcome and command.
struct BlockDragChip {
    words: SharedString,
}

impl Render for BlockDragChip {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ui = cx.global::<Appearance>().theme.ui();
        div()
            .px(px(CHIP_PAD))
            .py(px(3.0))
            .rounded(px(CHIP_RADIUS))
            .opacity(0.95)
            .shadow_md()
            .bg(hsla(ui.panel))
            .border_1()
            .border_color(hsla(ui.panel_border))
            .text_color(hsla(ui.text))
            .text_size(px(crate::ui_tokens::text::META))
            .whitespace_nowrap()
            .child(self.words.clone())
    }
}

/// The chip (M11l): an absolutely placed panel over the grid, 4 pt right of the gutter, exactly
/// on the command row (one row tall, no shadow, so the rows above and below show whole): the
/// outcome and the command on the left, three buttons on the right (Copy command, Copy
/// output, Rerun; a press runs one for the chip's block). A disabled
/// button is dimmed and does nothing; a disabled Rerun's tooltip says why. It blocks presses
/// from the grid under it (so they never select or reach the program) but not the wheel.
fn chip_element(
    chip: Chip,
    ui: &UiColors,
    slots: ChipSlots,
    cx: &mut Context<TerminalView>,
) -> gpui::Div {
    let (has_cmd, has_output) = (chip.cmd.is_some(), chip.has_output);
    let bounds = slots.chip;
    // The words fit the row (smaller under a small terminal font), on the row's inside.
    let words = block_view::chip_text(f32::from(chip.height));
    let mut label = div()
        .flex()
        .flex_row()
        .flex_shrink(1.0)
        .min_w(px(0.0))
        .overflow_hidden()
        .gap(px(4.0));
    let had_outcome = chip.outcome.is_some();
    if let Some((outcome, ink)) = chip.outcome {
        label = label.child(div().flex_none().text_color(hsla(ink)).child(outcome));
    }
    if let Some(cmd) = chip.cmd {
        // The dot goes with the command, so a narrow chip cuts both together.
        let cmd = match had_outcome {
            true => format!("· {cmd}"),
            false => cmd,
        };
        let text = div()
            .id("chip-command")
            .flex_shrink(1.0)
            .min_w(px(0.0))
            .truncate()
            .text_color(hsla(ui.text))
            .child(cmd);
        #[cfg(feature = "ui-shots")]
        let text = {
            let slot = slots.command.clone();
            text.child(
                canvas(move |b, _, _| slot.set(Some(b)), |_, _, _, _| {})
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full(),
            )
        };
        // The command is the block's handle (M11aa Task 11): dragged onto an agent tab (its
        // body, or its tab in a bar), the block is attached to your next message there. A chip
        // of the block follows the pointer.
        let text = match chip.drag {
            Some(drag) => {
                let words: SharedString = drag_words(&drag).into();
                text.on_drag(drag, move |_, _, window, cx| {
                    crate::tab_bar::hold_closed_hand(window, cx);
                    let words = words.clone();
                    cx.new(|_| BlockDragChip { words })
                })
                .into_any_element()
            }
            None => text.into_any_element(),
        };
        label = label.child(text);
    }
    let (id, rerun) = (chip.id, chip.rerun);
    let buttons = chip.buttons.into_iter().enumerate().map(|(i, name)| {
        // Copy command needs a recorded command, Copy output some output; Rerun every guard
        // of `block_view::rerun`.
        let enabled = block_view::chip_enabled(has_cmd, has_output, rerun)[i];
        let button = div()
            .id(("chip-button", i))
            .flex()
            .items_center()
            .h(px(words.button))
            .px(px(5.0))
            .rounded(px(4.0))
            .text_color(hsla(if enabled {
                ui.text_dim
            } else {
                block_view::disabled_ink(ui)
            }))
            .child(name);
        #[cfg(feature = "ui-shots")]
        let button = {
            let slots = slots.buttons.clone();
            button.child(
                canvas(
                    move |b, _, _| {
                        let mut all = slots.get();
                        all[i] = Some(b);
                        slots.set(all);
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
        };
        match (enabled, rerun) {
            (true, _) => {
                let id = id.clone();
                button
                    .hover(|style| style.bg(hsla(ui.panel_hover)))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            this.chip_button(i, &id, cx);
                        }),
                    )
            }
            (false, Err(why)) if i == 2 => {
                let tip = block_view::refusal_tip(why);
                button.tooltip(move |_, cx| -> AnyView { cx.new(|_| Tip(tip.into())).into() })
            }
            (false, _) => button,
        }
    });
    let panel = div()
        .max_w(chip.max_width)
        .h(chip.height)
        .flex()
        .flex_row()
        .items_center()
        .gap(px(8.0))
        .px(px(CHIP_PAD))
        .rounded(px(CHIP_RADIUS))
        .bg(hsla(ui.panel))
        .border_1()
        .border_color(hsla(ui.panel_border))
        // Nothing drawn past the row, whatever the font.
        .overflow_hidden()
        .text_size(px(words.size))
        .line_height(px(words.line))
        .whitespace_nowrap()
        .child(label)
        .child(
            div()
                .flex()
                .flex_row()
                .flex_none()
                .gap(px(2.0))
                .children(buttons),
        );
    // The panel in a bare box of its size, so the box's bounds are the panel's.
    div()
        .absolute()
        .left(chip.left)
        .top(chip.top)
        .block_mouse_except_scroll()
        .child(
            canvas(move |b, _, _| bounds.set(Some(b)), |_, _, _, _| {})
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
        )
        .child(panel)
}

/// The command blocks a frame draws (M11l), taken from its snapshot: in viewport rows.
struct BlockMarks<'a> {
    blocks: Vec<grid::BlockView>,
    /// Each block's right prompt on its command row (`block_view::right_run`), when the row
    /// is in view.
    runs: Vec<Option<usize>>,
    /// The grid's columns and rows.
    dims: (usize, usize),
    split: bool,
    hovered: Option<&'a str>,
}

impl BlockMarks<'_> {
    /// Paint the bars in the left padding, and say where they are. `bounds` is the pane, `origin` the grid's first cell.
    fn paint(
        self,
        ui: &UiColors,
        m: &CellMetrics,
        bounds: Bounds<Pixels>,
        origin: gpui::Point<Pixels>,
        window: &mut Window,
    ) -> GutterPaint {
        let (cols, rows) = self.dims;
        let grid = Bounds::new(
            origin,
            size(m.cell_width * cols as f32, m.line_height * rows as f32),
        );
        let row_y = |r: i32| origin.y + m.line_height * r as f32;
        let centre = origin.x - px(block_view::gutter_x(self.split));
        let mut painted = GutterPaint {
            strip: (bounds.left(), origin.x),
            pane: bounds,
            grid,
            runs: self.runs,
            cell: (m.cell_width, m.line_height),
            ..GutterPaint::default()
        };
        for (i, b) in self.blocks.iter().enumerate() {
            painted.cmd_tops.push(row_y(b.cmd_row));
            let Some((top, bottom)) = block_view::bar_rows(b, rows) else {
                continue;
            };
            let active = b.chosen || self.hovered == Some(b.id.as_str());
            let width = px(if active {
                GUTTER_BAR_ACTIVE
            } else {
                GUTTER_BAR
            });
            let color = match b.chosen {
                true => ui.accent,
                false => block_view::bar_color(b.outcome, ui),
            };
            let (y0, y1) = (row_y(top), row_y(bottom + 1));
            let bar = Bounds::new(
                point(centre - width / 2.0, y0 + px(BAR_END_GAP)),
                size(width, (y1 - y0 - px(2.0 * BAR_END_GAP)).max(px(0.0))),
            );
            window.paint_quad(fill(bar, hsla(color)).corner_radii(px(BAR_RADIUS)));
            painted.hits.push(GutterHit {
                id_index: i,
                top: f32::from(y0),
                bottom: f32::from(y1),
            });
        }
        painted.blocks = self.blocks;
        painted
    }
}

/// Monospace cell geometry for the terminal font (also the file tabs' text, M11g).
pub(crate) struct CellMetrics {
    pub(crate) font: Font,
    pub(crate) font_size: Pixels,
    pub(crate) cell_width: Pixels,
    pub(crate) line_height: Pixels,
}

impl CellMetrics {
    /// The configured font and size; bold/italic runs derive from `font`, keeping its fallbacks
    /// and features.
    pub(crate) fn measure(appearance: &Appearance, window: &Window) -> Self {
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
    paint_grid(
        snap,
        theme,
        &metrics,
        bounds,
        grid.origin,
        look,
        |_| {},
        window,
        cx,
    );
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
pub(crate) struct CursorSettings {
    pub(crate) setting: CursorShapeSetting,
    pub(crate) focused: bool,
    pub(crate) key_window: bool,
    /// The `animations` setting on and macOS Reduce motion off.
    pub(crate) motion: bool,
    pub(crate) glide: bool,
    pub(crate) glow: bool,
    pub(crate) fun: CursorFun,
    /// `cursor_look::cursor_color` for the session's status.
    pub(crate) color: Rgb,
}

impl CursorSettings {
    /// The cursor to paint over `snap` at `now`, with `motion` brought up to date (where it
    /// is, its glide, its sparks, the frames it needs next). `None` when there's no cursor.
    pub(crate) fn look(
        &self,
        snap: &GridSnapshot,
        theme: &Theme,
        motion: &mut CursorMotion,
        now: Instant,
    ) -> Option<CursorLook> {
        let live = self.focused && self.key_window;
        motion.note_live(live, now);
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
        motion.note_glowing(glowing && self.motion, now);
        // At rest (five minutes with no change, M11n) the glow stays, solid, with no frames.
        let since = now.saturating_duration_since(motion.changed);
        let resting = cursor_look::breath_rests(since.saturating_sub(cursor_look::HOLD));
        motion.frames = cursor_look::needs_frames(
            self.focused,
            self.key_window,
            self.motion,
            glowing && !resting,
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
pub(crate) struct CursorLook {
    /// The cell it is on (a block's letter there takes `text`).
    pub(crate) cell: (usize, usize),
    /// How many cells wide it is: 2 on a wide (CJK) character (`cursor_cells`).
    cells: usize,
    /// Where it is drawn, in cells (fractional mid-glide).
    pub(crate) at: (f32, f32),
    /// Never `Hidden`.
    pub(crate) shape: PaintShape,
    color: Rgb,
    /// The letter's colour under a full block.
    pub(crate) text: Rgb,
    /// The breath: the cursor's opacity.
    pub(crate) alpha: f32,
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
    pub(crate) fn bounds(
        &self,
        at: gpui::Point<Pixels>,
        m: &CellMetrics,
        scale: f32,
    ) -> Bounds<Pixels> {
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
pub(crate) fn breath_redraw(since_change: Duration) -> bool {
    since_change >= cursor_look::HOLD
}

/// How long the breathing timer sleeps, `since_change` after the cursor last changed: to the
/// end of the hold while it lasts (it draws nothing new before then), else to the breath's next
/// frame (`cursor_look::breath_wait`, on the breath's clock, which starts as the hold ends).
pub(crate) fn breath_sleep(since_change: Duration) -> Duration {
    match cursor_look::HOLD.checked_sub(since_change) {
        Some(left) if !left.is_zero() => left,
        _ => cursor_look::breath_wait(since_change - cursor_look::HOLD),
    }
}

/// The cursor's motion between paints (M11f): the cell and screen it was last painted on, its
/// glide, when it last changed (the breath holds after), the sparks in flight and how many
/// keystrokes sparked (each burst's seed). Shared by the view and its canvas.
pub(crate) struct CursorMotion {
    /// The cell it was last painted on; `None` while hidden.
    cell: Option<(usize, usize)>,
    /// The screen of that cell.
    screen: Screen,
    glide: Option<Glide>,
    /// When the cursor last moved, the program wrote or you typed.
    pub(crate) changed: Instant,
    sparks: Vec<Spark>,
    seed: u64,
    /// Where it was last drawn (cells) and its shape, while live: where sparks start.
    drawn: Option<((f32, f32), PaintShape)>,
    /// The redraws its last paint asked for.
    pub(crate) frames: Frames,
    /// Whether the last paint was focused in the key window.
    was_live: bool,
    /// Whether the last paint's glow could breathe (glow on, a blinking cursor, motion on).
    was_glowing: bool,
}

impl CursorMotion {
    pub(crate) fn new(now: Instant) -> Self {
        Self {
            cell: None,
            screen: Screen::default(),
            glide: None,
            changed: now,
            sparks: Vec::new(),
            seed: 0,
            drawn: None,
            frames: Frames::None,
            was_live: false,
            was_glowing: false,
        }
    }

    /// A burst of sparks off where the cursor was last drawn (cells `cw` by `lh`), for a
    /// typed key; nothing while it isn't drawn.
    pub(crate) fn spark(&mut self, cw: f32, lh: f32, now: Instant) {
        let Some(((x, y), shape)) = self.drawn else {
            return;
        };
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
        self.seed += 1;
        let seed = self.seed;
        self.sparks
            .extend(cursor_look::sparks_for_key(seed, (ox, oy), now));
    }

    /// Focus or the key window came back: the cursor returns solid, not mid-breath.
    fn note_live(&mut self, live: bool, now: Instant) {
        if live && !self.was_live {
            self.changed = now;
        }
        self.was_live = live;
    }

    /// The breath may run again (the glow, Animations or Reduce motion turned back on): it
    /// starts afresh, solid at first, rather than staying at rest until the next change.
    fn note_glowing(&mut self, glowing: bool, now: Instant) {
        if glowing && !self.was_glowing {
            self.changed = now;
        }
        self.was_glowing = glowing;
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

/// Follow the mouse's moves and releases anywhere in the window this frame (a drag that leaves
/// the pane keeps going; paint-phase registrations last one frame), for both views.
fn follow_mouse(view: &Entity<TerminalView>, hitbox: &gpui::Hitbox, window: &mut Window) {
    window.on_mouse_event({
        let view = view.clone();
        let hitbox = hitbox.clone();
        move |e: &MouseMoveEvent, phase, window, cx| {
            if phase == DispatchPhase::Bubble {
                let over_pane = hitbox.is_hovered(window);
                view.update(cx, |this, cx| this.on_mouse_move(e, over_pane, cx));
            }
        }
    });
    let view = view.clone();
    let hitbox = hitbox.clone();
    window.on_mouse_event(move |e: &MouseUpEvent, phase, window, cx| {
        if phase == DispatchPhase::Bubble {
            let over_pane = hitbox.is_hovered(window);
            view.update(cx, |this, cx| this.on_mouse_up(e, over_pane, cx));
        }
    });
}

/// What `paint_cards` reads from (M11ab).
struct CardPaint<'a> {
    core: &'a TerminalCore,
    state: &'a RefCell<crate::card_view::CardsState>,
    marks: u64,
    utc_offset: i64,
    split: bool,
    theme: &'a Theme,
    ui: &'a UiColors,
    chosen: Option<String>,
}

/// Paint a tab in card view (note 88): the column of earlier rows, cards and the live box,
/// bottom-anchored in the pane's padding, only the rows in view read (under one lock).
#[allow(clippy::too_many_arguments)]
fn paint_cards(
    p: CardPaint,
    m: &CellMetrics,
    bounds: Bounds<Pixels>,
    cursor: &CursorSettings,
    motion: &mut CursorMotion,
    window: &mut Window,
    cx: &mut App,
) {
    use crate::card_view::{
        CTX_ROW, CacheKey, CardHit, Draw, EDGE, HEADER_H, INNER_PAD_X, OUTER_INSET, Region,
        SURF_PAD_TOP, SURF_PAD_X, TAB_RISE,
    };
    let corner = px(ui_size::CARD_RADIUS);
    window.paint_quad(
        fill(bounds, hsla(p.theme.background)).corner_radii(Corners {
            bottom_left: corner,
            bottom_right: corner,
            ..Corners::default()
        }),
    );
    // The column keeps the terminal at its bottom: no scrolled-back screen under the cards.
    if p.core.display_offset() > 0 {
        p.core.scroll(Scroll::Bottom);
    }
    let content = inset(bounds, p.split);
    let line_h = f32::from(m.line_height);
    let cell_w = f32::from(m.cell_width);
    let dims = p.core.dims();
    let (frame, clears) = p.core.read_term(|term, clears| {
        let shape = term.renderable_content().cursor.shape;
        let shape = (shape != alacritty_terminal::vte::ansi::CursorShape::Hidden)
            .then(|| (shape, term.cursor_style().blinking));
        let key = CacheKey {
            marks: p.marks,
            clears,
            cols: dims.cols,
            rows: dims.rows,
            alt: false,
        };
        let frame = crate::card_view::frame(
            term,
            &mut p.state.borrow_mut(),
            key,
            p.theme,
            line_h,
            (f32::from(content.top()), f32::from(content.size.height)),
            p.utc_offset,
            shape,
        );
        (frame, clears)
    });
    let left = content.left();
    let width = content.size.width;
    let w = f32::from(width);
    let ui_font = window.text_style().font();
    let meta = px(crate::ui_tokens::text::META);
    // What the pointer can reach this frame: rows to select in, cards' hit areas.
    let mut regions: Vec<Region> = Vec::new();
    let mut hits: Vec<CardHit> = Vec::new();
    let mut composer = None;
    let region = |key: String, rows: &crate::card_view::Rows, x: f32, live: bool| Region {
        key,
        x,
        y: rows.y,
        bottom: rows.y + rows.lines.len() as f32 * line_h,
        top: rows.top,
        live,
    };
    // Cards' text sits inside their surface; the rail in the pane's left padding.
    let text_x = f32::from(left) + SURF_PAD_X;
    let rail_x = f32::from(left) - 14.0;
    let dot_dy = SURF_PAD_TOP + HEADER_H / 2.0;
    let blank = grid::CellView::blank(p.theme);
    // The rail lives left of the content: the mask takes the padding in.
    let mask = Bounds::from_corners(
        point(bounds.left(), content.top()),
        point(content.right(), content.bottom()),
    );
    let draws = frame.draws;
    window.with_content_mask(Some(gpui::ContentMask { bounds: mask }), |window| {
        for (i, draw) in draws.iter().enumerate() {
            match draw {
                Draw::Plain(rows) => {
                    regions.push(region(format!("rows:{}", rows.top.0), rows, text_x, false));
                    let at = point(px(text_x), px(rows.y));
                    paint_lines(&rows.lines, p.theme, m, at, window, cx);
                }
                Draw::Card {
                    id,
                    y,
                    height,
                    outcome,
                    header,
                    cmd,
                    panel,
                    failed,
                } => {
                    let (y, height) = (*y, *height);
                    let chosen = p.chosen.as_deref() == Some(id.as_str());
                    // The rail down to the next card's dot.
                    if let Some(Draw::Card { y: next, .. }) = draws.get(i + 1) {
                        window.paint_quad(fill(
                            Bounds::from_corners(
                                point(px(rail_x - 0.5), px(y + dot_dy)),
                                point(px(rail_x + 0.5), px(next + dot_dy)),
                            ),
                            hsla(p.ui.card_border),
                        ));
                    }
                    // The panel with a hairline edge (the designer's call); the chosen card's edge
                    // in the accent.
                    window.paint_quad(gpui::quad(
                        Bounds::new(point(left, px(y)), size(width, px(height))),
                        px(8.0),
                        hsla(p.ui.card_surface),
                        px(EDGE),
                        hsla(if chosen { p.ui.accent } else { p.ui.card_edge }),
                        gpui::BorderStyle::Solid,
                    ));
                    // Its dot: green done, red failed, quiet when the exit wasn't reported; a ring
                    // in the accent when chosen.
                    let dot = match outcome {
                        _ if *failed => p.ui.block_failed,
                        crate::blocks::Outcome::Ok => p.ui.ring_done,
                        _ => p.ui.text_faint,
                    };
                    if chosen {
                        window.paint_quad(gpui::quad(
                            Bounds::new(
                                point(px(rail_x - 7.0), px(y + dot_dy - 7.0)),
                                size(px(14.0), px(14.0)),
                            ),
                            px(7.0),
                            hsla(p.theme.background),
                            px(1.5),
                            hsla(p.ui.accent),
                            gpui::BorderStyle::Solid,
                        ));
                    }
                    window.paint_quad(
                        fill(
                            Bounds::new(
                                point(px(rail_x - 4.0), px(y + dot_dy - 4.0)),
                                size(px(8.0), px(8.0)),
                            ),
                            hsla(dot),
                        )
                        .corner_radii(px(4.0)),
                    );
                    // The header: when · how long · exit N (the exit in the failed colour).
                    let all = header.words();
                    let exit = header
                        .exit
                        .filter(|&x| x != 0)
                        .map_or(0, |x| format!("exit {x}").len());
                    paint_words(
                        &all,
                        &[(all.len() - exit, p.ui.text_dim), (exit, p.ui.block_failed)],
                        ui_font.clone(),
                        meta,
                        point(px(text_x), px(y + SURF_PAD_TOP)),
                        px(HEADER_H),
                        window,
                        cx,
                    );
                    let cmd_y = y + SURF_PAD_TOP + HEADER_H;
                    let cmd_lines = match cmd {
                        Ok(c) => c.lines().count().max(1),
                        Err(rows) => rows.lines.len().max(1),
                    };
                    match cmd {
                        Ok(cmd) => {
                            for (n, line) in cmd.lines().enumerate() {
                                let mark = if n == 0 { "\u{276f} " } else { "  " };
                                let text = format!("{mark}{line}");
                                paint_words(
                                    &text,
                                    &[
                                        (mark.len(), p.ui.text_faint),
                                        (line.len(), p.theme.foreground),
                                    ],
                                    m.font.clone(),
                                    m.font_size,
                                    point(px(text_x), px(cmd_y + line_h * n as f32)),
                                    m.line_height,
                                    window,
                                    cx,
                                );
                            }
                        }
                        Err(rows) => {
                            let at = point(px(text_x), px(rows.y));
                            paint_lines(&rows.lines, p.theme, m, at, window, cx);
                        }
                    }
                    if let Some(panel) = panel {
                        if let Some(fold) = &panel.fold {
                            paint_words(
                                fold,
                                &[(fold.len(), p.ui.text_dim)],
                                ui_font.clone(),
                                meta,
                                point(px(text_x), px(panel.y)),
                                m.line_height,
                                window,
                                cx,
                            );
                        }
                        regions.push(region(id.clone(), &panel.rows, text_x, false));
                        let at = point(px(text_x), px(panel.rows.y));
                        paint_lines(&panel.rows.lines, p.theme, m, at, window, cx);
                    }
                    hits.push(CardHit {
                        id: id.clone(),
                        card: (f32::from(left), y, w, height),
                        cmd: (f32::from(left), cmd_y, w, cmd_lines as f32 * line_h),
                        fold: panel.as_ref().and_then(|panel| {
                            panel
                                .fold
                                .as_ref()
                                .map(|_| (text_x, panel.y, w - 2.0 * SURF_PAD_X, line_h))
                        }),
                        command: cmd.as_ref().ok().cloned(),
                        outcome: *outcome,
                    });
                }
                Draw::Live {
                    y,
                    height,
                    rows,
                    cursor: at,
                    crop,
                } => {
                    let (y, height) = (*y, *height);
                    // The composer, a folder tab (the designer's details): the box's top edge
                    // `TAB_RISE` down, the pills (elements over it, `context_row`) in a tab
                    // rising from its left; then, inset, the inner box with its lightest edge
                    // along the top.
                    let (outer_fill, edge_ink) =
                        (hsla(p.ui.composer_outer), hsla(p.ui.composer_edge));
                    let body_top = y + TAB_RISE;
                    let body = Bounds::new(
                        point(left, px(body_top)),
                        size(width, px(height - TAB_RISE)),
                    );
                    let tab = crate::card_view::composer_tab(p.state.borrow().pills_w, w);
                    let (r_tab, r_body, r_step) = (px(14.0), px(15.0), px(8.0));
                    window.paint_quad(gpui::quad(
                        body,
                        Corners {
                            top_left: if tab.is_some() { px(0.0) } else { r_body },
                            top_right: r_body,
                            bottom_left: r_body,
                            bottom_right: r_body,
                        },
                        outer_fill,
                        px(EDGE),
                        edge_ink,
                        gpui::BorderStyle::Solid,
                    ));
                    if let Some(tw) = tab {
                        let tab_right = left + px(tw);
                        // The tab: rounded at the top, open at the bottom, its sides down past
                        // the body's edge.
                        window.paint_quad(gpui::quad(
                            Bounds::new(point(left, px(y)), size(px(tw), px(TAB_RISE + EDGE))),
                            Corners {
                                top_left: r_tab,
                                top_right: r_tab,
                                ..Corners::default()
                            },
                            outer_fill,
                            gpui::Edges {
                                top: px(EDGE),
                                left: px(EDGE),
                                right: px(EDGE),
                                bottom: px(0.0),
                            },
                            edge_ink,
                            gpui::BorderStyle::Solid,
                        ));
                        // No seam: the body's top edge under the tab filled over.
                        window.paint_quad(fill(
                            Bounds::new(
                                point(left + px(EDGE), px(body_top)),
                                size(px(tw - 2.0 * EDGE), px(EDGE)),
                            ),
                            outer_fill,
                        ));
                        // The concave corner where the tab steps down: the box's fill under a
                        // panel-coloured quad whose rounded corner (its left and bottom edges
                        // drawn) carves the curve.
                        window.paint_quad(fill(
                            Bounds::from_corners(
                                point(tab_right - px(EDGE), px(body_top - 8.0)),
                                point(tab_right + px(8.0), px(body_top + EDGE)),
                            ),
                            outer_fill,
                        ));
                        window.paint_quad(gpui::quad(
                            Bounds::new(
                                point(tab_right - px(EDGE), px(body_top + EDGE - 16.0)),
                                size(px(16.0), px(16.0)),
                            ),
                            Corners {
                                bottom_left: r_step,
                                ..Corners::default()
                            },
                            hsla(p.theme.background),
                            gpui::Edges {
                                left: px(EDGE),
                                bottom: px(EDGE),
                                ..gpui::Edges::default()
                            },
                            edge_ink,
                            gpui::BorderStyle::Solid,
                        ));
                    }
                    composer = Some((f32::from(left), y, w));
                    let side = EDGE + OUTER_INSET;
                    let inner = Bounds::new(
                        point(left + px(side), px(y + EDGE + CTX_ROW)),
                        size(
                            width - px(2.0 * side),
                            px(height - EDGE - CTX_ROW - OUTER_INSET - EDGE),
                        ),
                    );
                    // No edge: its fill on the outer box sets it apart (the designer).
                    window
                        .paint_quad(fill(inner, hsla(p.ui.composer_inner)).corner_radii(px(12.0)));
                    let origin_x = f32::from(inner.left()) + INNER_PAD_X;
                    // Only what you type: the first row from where the typing starts to the
                    // right-hand prompt, after a `❯` (rework); the rows under it as they are.
                    let mut lines = rows.lines.clone();
                    let mut cur = *at;
                    let mut hint = false;
                    match (crop, lines.first()) {
                        (Some(c), Some(first)) => {
                            let end = c.cut.unwrap_or(first.len()).min(first.len());
                            let from = c.from.min(end);
                            let typed = &first[from..end];
                            let empty = typed.iter().all(|cell| cell.ch == ' ' || cell.ch == '\0');
                            let mut row = vec![
                                grid::CellView {
                                    ch: '\u{276f}',
                                    fg: p.ui.accent,
                                    ..blank
                                },
                                blank,
                            ];
                            row.extend_from_slice(typed);
                            row.resize(dims.cols.max(row.len()), blank);
                            #[cfg(feature = "ui-shots")]
                            {
                                p.state.borrow_mut().typed_row = row
                                    .iter()
                                    .map(|c| c.ch)
                                    .collect::<String>()
                                    .trim_end()
                                    .to_string();
                            }
                            lines[0] = row;
                            if let Some(cv) = cur.as_mut().filter(|cv| cv.row == 0) {
                                cv.col = cv.col.saturating_sub(from) + 2;
                                hint = empty;
                            }
                            // The rows under it hang under the typing, not under `❯` (your
                            // words: "the second line is supposed to align with the first
                            // line, the icon below should be empty").
                            let indent = crate::card_view::INDENT_COLS;
                            for row in lines.iter_mut().skip(1) {
                                row.splice(0..0, std::iter::repeat_n(blank, indent));
                            }
                            if let Some(cv) = cur.as_mut().filter(|cv| cv.row > 0) {
                                cv.col += indent;
                            }
                            regions.push(Region {
                                key: "live0".into(),
                                x: origin_x + 2.0 * cell_w - from as f32 * cell_w,
                                y: rows.y,
                                bottom: rows.y + line_h,
                                top: rows.top,
                                live: true,
                            });
                            if rows.lines.len() > 1 {
                                regions.push(Region {
                                    key: "live".into(),
                                    x: origin_x + crate::card_view::INDENT_COLS as f32 * cell_w,
                                    y: rows.y + line_h,
                                    bottom: rows.y + rows.lines.len() as f32 * line_h,
                                    top: rows.top + 1i32,
                                    live: true,
                                });
                            }
                        }
                        _ => regions.push(region("live".into(), rows, origin_x, true)),
                    }
                    let origin = point(px(origin_x), px(rows.y));
                    let snap = GridSnapshot {
                        // (Its typing rows may run `INDENT_COLS` past the terminal's.)
                        cols: lines.iter().map(Vec::len).max().unwrap_or(0).max(dims.cols),
                        rows: lines.len(),
                        lines,
                        cursor: cur,
                        alt_screen: false,
                        display_offset: 0,
                        clears,
                        blocks: Vec::new(),
                    };
                    let look = cursor.look(&snap, p.theme, motion, Instant::now());
                    // (No fill: the inner box shows through the cells' default background.)
                    let none = Bounds::new(origin, size(px(0.0), px(0.0)));
                    paint_grid(snap, p.theme, m, none, origin, look, |_| {}, window, cx);
                    #[cfg(feature = "ui-shots")]
                    {
                        p.state.borrow_mut().hint = None;
                    }
                    if hint {
                        // The agent box's style (the designer): the terminal's font and size,
                        // dim, one cell after the cursor's, "…" when it doesn't fit.
                        let from = origin_x + 3.0 * cell_w;
                        let room = f32::from(inner.right()) - INNER_PAD_X - from;
                        let cols = (room / cell_w).floor().max(0.0) as usize;
                        let words = crate::card_view::fit_words(PLACEHOLDER, cols);
                        paint_words(
                            &words,
                            &[(words.len(), p.ui.text_dim)],
                            m.font.clone(),
                            m.font_size,
                            point(px(from), px(rows.y)),
                            m.line_height,
                            window,
                            cx,
                        );
                        #[cfg(feature = "ui-shots")]
                        {
                            p.state.borrow_mut().hint = Some((words.clone(), from));
                        }
                    }
                }
            }
        }
    });
    let mut state = p.state.borrow_mut();
    state.regions = regions;
    state.hits = hits;
    state.composer = composer;
    state.origin = (f32::from(bounds.origin.x), f32::from(bounds.origin.y));
    state.view_rect = (
        f32::from(content.left()),
        f32::from(content.top()),
        f32::from(content.size.width),
        f32::from(content.size.height),
    );
}

/// `text` in `font` at `size`, its first `runs` bytes each in their colour, from `at` on a line
/// `line_h` tall.
#[allow(clippy::too_many_arguments)]
fn paint_words(
    text: &str,
    runs: &[(usize, Rgb)],
    font: Font,
    size: Pixels,
    at: gpui::Point<Pixels>,
    line_h: Pixels,
    window: &mut Window,
    cx: &mut App,
) {
    if text.is_empty() {
        return;
    }
    let runs: Vec<TextRun> = runs
        .iter()
        .filter(|(len, _)| *len > 0)
        .map(|&(len, color)| TextRun {
            len,
            font: font.clone(),
            color: hsla(color),
            background_color: None,
            underline: None,
            strikethrough: None,
        })
        .collect();
    let shaped =
        window
            .text_system()
            .shape_line(SharedString::from(text.to_string()), size, &runs, None);
    let _ = shaped.paint(at, line_h, TextAlign::Left, None, window, cx);
}

/// M11ab: `lines` (rows of cells, top first) with their first cell at `origin`: the cells'
/// backgrounds, then their text, with nothing under them filled and no cursor (a card's
/// output; `paint_grid` paints the two halves around its cursor).
fn paint_lines(
    lines: &[Vec<grid::CellView>],
    theme: &Theme,
    m: &CellMetrics,
    origin: gpui::Point<Pixels>,
    window: &mut Window,
    cx: &mut App,
) {
    paint_line_backgrounds(lines, theme, m, origin, window);
    paint_line_text(lines, m, origin, window, cx);
}

/// The cells' backgrounds that differ from the theme's (`grid::bg_rects`).
fn paint_line_backgrounds(
    lines: &[Vec<grid::CellView>],
    theme: &Theme,
    m: &CellMetrics,
    origin: gpui::Point<Pixels>,
    window: &mut Window,
) {
    for (row, line) in lines.iter().enumerate() {
        for rect in grid::bg_rects(line, theme.background) {
            let at = point(
                origin.x + m.cell_width * rect.col as f32,
                origin.y + m.line_height * row as f32,
            );
            let size = size(m.cell_width * rect.cells as f32, m.line_height);
            window.paint_quad(fill(Bounds::new(at, size), hsla(rect.color)));
        }
    }
}

/// The cells' text, shaped one cell per glyph (`grid::text_spans`).
fn paint_line_text(
    lines: &[Vec<grid::CellView>],
    m: &CellMetrics,
    origin: gpui::Point<Pixels>,
    window: &mut Window,
    cx: &mut App,
) {
    let cell_origin = |row: usize, col: usize| {
        point(
            origin.x + m.cell_width * col as f32,
            origin.y + m.line_height * row as f32,
        )
    };
    for (row, line) in lines.iter().enumerate() {
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
}

/// Paint `snap` with its first cell at `origin`, over `bounds` (the whole pane, padding
/// included) filled with the theme background, and `cursor` over it: its trail and glow under
/// the text, a block under its letter (which takes the block's text colour), any other shape
/// over the text, then the sparks. `under_text` paints over the cells' backgrounds, before the
/// cursor and the text.
#[allow(clippy::too_many_arguments)]
fn paint_grid(
    mut snap: GridSnapshot,
    theme: &Theme,
    m: &CellMetrics,
    bounds: Bounds<Pixels>,
    origin: gpui::Point<Pixels>,
    cursor: Option<CursorLook>,
    under_text: impl FnOnce(&mut Window),
    window: &mut Window,
    cx: &mut App,
) {
    // Its bottom corners rounded with the card's (M11i; `CARD_RADIUS`, 14 since M11al): the
    // padding keeps the cells out of them, so nothing square shows past the card's edge. A
    // pane away from the card's corners rounds too, onto the card's own colour (§3.4).
    let corner = px(ui_size::CARD_RADIUS);
    window.paint_quad(fill(bounds, hsla(theme.background)).corner_radii(Corners {
        bottom_left: corner,
        bottom_right: corner,
        ..Corners::default()
    }));

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

    paint_line_backgrounds(&snap.lines, theme, m, origin, window);
    // Over the cells' backgrounds, under the cursor and the text (M11l's command blocks).
    under_text(window);

    if let Some(c) = &cursor {
        paint_cursor_under(c, origin, m, window);
    }

    paint_line_text(&snap.lines, m, origin, window, cx);

    if let Some(c) = &cursor {
        paint_cursor_over(c, origin, m, window);
    }
}

/// The cursor's parts under the text, on cells from `origin`: its trail, its glow and a block.
pub(crate) fn paint_cursor_under(
    c: &CursorLook,
    origin: gpui::Point<Pixels>,
    m: &CellMetrics,
    window: &mut Window,
) {
    let at_cells =
        |(x, y): (f32, f32)| point(origin.x + m.cell_width * x, origin.y + m.line_height * y);
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

/// The cursor's parts over the text: any shape but a block, and its sparks.
pub(crate) fn paint_cursor_over(
    c: &CursorLook,
    origin: gpui::Point<Pixels>,
    m: &CellMetrics,
    window: &mut Window,
) {
    let at_cells =
        |(x, y): (f32, f32)| point(origin.x + m.cell_width * x, origin.y + m.line_height * y);
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

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use gpui::{Bounds, point, px, size};

    use super::{
        CursorMotion, GridLayout, Screen, breath_redraw, breath_sleep, cursor_cells, inset,
        is_typed_text,
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
    fn the_cursor_returns_solid_when_focus_comes_back() {
        let t0 = Instant::now();
        let mut m = CursorMotion::new(t0);
        let t1 = t0 + Duration::from_secs(5);
        m.note_live(true, t1);
        assert_eq!(m.changed, t1);
        // Staying live leaves the breath alone; losing and regaining focus restarts it.
        let t2 = t1 + Duration::from_secs(5);
        m.note_live(true, t2);
        assert_eq!(m.changed, t1);
        m.note_live(false, t2);
        let t3 = t2 + Duration::from_secs(1);
        m.note_live(true, t3);
        assert_eq!(m.changed, t3);
    }

    #[test]
    fn the_breath_restarts_when_it_may_run_again() {
        let t0 = Instant::now();
        let mut m = CursorMotion::new(t0);
        let t1 = t0 + Duration::from_secs(400);
        m.note_glowing(true, t1);
        assert_eq!(m.changed, t1);
        // Still glowing leaves the breath alone (it may rest); off then on restarts it.
        let t2 = t1 + Duration::from_secs(400);
        m.note_glowing(true, t2);
        assert_eq!(m.changed, t1);
        m.note_glowing(false, t2);
        assert_eq!(m.changed, t1);
        let t3 = t2 + Duration::from_secs(1);
        m.note_glowing(true, t3);
        assert_eq!(m.changed, t3);
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
    fn the_breath_sleeps_through_its_hold() {
        use crate::cursor_look::{BREATH_MIN, HOLD, breath_wait};
        let ms = Duration::from_millis;
        // While the breath holds, one sleep to its end, not a wake-up every 50 ms.
        assert_eq!(breath_sleep(Duration::ZERO), HOLD);
        assert_eq!(breath_sleep(ms(100)), ms(500));
        assert_eq!(breath_sleep(ms(599)), ms(1));
        // Past it, to the breath's next frame (the first level, 162 ms from the peak).
        assert_eq!(breath_sleep(HOLD), breath_wait(Duration::ZERO));
        assert!(breath_sleep(HOLD) > ms(160) && breath_sleep(HOLD) < ms(165));
        assert_eq!(breath_sleep(HOLD + ms(5)), breath_wait(ms(5)));
        assert!(breath_sleep(ms(5000)) >= BREATH_MIN);
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

    /// Final review #2: a background-service session turns card-capable only on a real mark,
    /// or at its snapshot (`Unknown`) when its grid already holds a tag; a bash session never.
    #[test]
    fn only_a_real_mark_or_a_tag_makes_a_session_card_capable() {
        use super::integration_after;
        use crate::command_name::CommandMark;
        assert_eq!(
            integration_after(&CommandMark::Unknown, None, || false),
            None
        );
        assert_eq!(
            integration_after(&CommandMark::Unknown, None, || true),
            Some(true)
        );
        assert_eq!(
            integration_after(&CommandMark::Prompt, None, || false),
            Some(true)
        );
        assert_eq!(
            integration_after(&CommandMark::Prompt, Some(false), || true),
            Some(false),
            "a spawned shell's answer stands"
        );
    }

    /// M11ab: in card view the terminal takes the live box's inside (`card_dims`); otherwise
    /// today's fit inside the padding.
    #[test]
    fn card_mode_resizes_to_the_box() {
        let bounds = Bounds::new(point(px(0.0), px(0.0)), size(px(1000.0), px(700.0)));
        for split in [false, true] {
            assert_eq!(
                super::pane_dims(true, bounds, split, px(8.0), px(17.0)),
                crate::card_view::card_dims(bounds.size, split, 8.0, 17.0)
            );
            assert_eq!(
                super::pane_dims(false, bounds, split, px(8.0), px(17.0)),
                GridLayout::fit(inset(bounds, split), px(8.0), px(17.0)).dims
            );
        }
    }

    #[test]
    fn inset_balances_leftover() {
        let bounds = Bounds::new(point(px(0.0), px(0.0)), size(px(400.0), px(300.0)));
        // 28 px sides, 22 px top and bottom; 20 and 14 in a split.
        assert_eq!(
            inset(bounds, false),
            Bounds::new(point(px(28.0), px(22.0)), size(px(344.0), px(256.0)))
        );
        assert_eq!(
            inset(bounds, true),
            Bounds::new(point(px(20.0), px(14.0)), size(px(360.0), px(272.0)))
        );
        // 9 x 17 cells: 38 columns use 342 of 344 px, 15 rows 255 of 256. What's left over is
        // shared evenly, so the grid sits 1 px further in on each side, half a px top and bottom.
        let grid = GridLayout::fit(inset(bounds, false), px(9.0), px(17.0));
        assert_eq!((grid.dims.cols, grid.dims.rows), (38, 15));
        assert_eq!(grid.origin, point(px(29.0), px(22.5)));
        let right = bounds.size.width - (grid.origin.x + grid.cell_width * 38.0);
        let bottom = bounds.size.height - (grid.origin.y + grid.line_height * 15.0);
        assert_eq!((right, bottom), (px(29.0), px(22.5)));
    }

    #[test]
    fn hit_testing_uses_the_inset_origin() {
        // A pane right of a 220 px sidebar: 544 x 356 inside the padding, 8 x 17 cells, so
        // 68 x 20 cells with 0 and 16 px left over; the grid starts at (248, 30).
        let bounds = Bounds::new(point(px(220.0), px(0.0)), size(px(600.0), px(400.0)));
        let grid = GridLayout::fit(inset(bounds, false), px(8.0), px(17.0));
        assert_eq!((grid.dims.cols, grid.dims.rows), (68, 20));
        assert_eq!(grid.origin, point(px(248.0), px(30.0)));
        let at = |x: f32, y: f32| {
            let (p, _) = grid.grid_point(point(px(x), px(y)), 0);
            (p.column.0, p.line.0)
        };
        // Just inside cell (3, 2).
        assert_eq!(at(248.0 + 3.0 * 8.0 + 1.0, 30.0 + 2.0 * 17.0 + 1.0), (3, 2));
        // In the padding, left of and above the grid: the first cell, not the second.
        assert_eq!(at(235.0, 10.0), (0, 0));
        // The last cell's far edge.
        assert_eq!(
            at(248.0 + 68.0 * 8.0 - 1.0, 30.0 + 20.0 * 17.0 - 1.0),
            (67, 19)
        );
        // In a split the padding is 20 and 14: 560 x 372 inside, 70 x 21 cells with 0 and 15 px
        // left over; the grid starts at (240, 21.5).
        let grid = GridLayout::fit(inset(bounds, true), px(8.0), px(17.0));
        assert_eq!((grid.dims.cols, grid.dims.rows), (70, 21));
        assert_eq!(grid.origin, point(px(240.0), px(21.5)));
    }
}
