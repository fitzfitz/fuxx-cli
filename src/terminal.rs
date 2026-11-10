//! One live terminal: an `alacritty_terminal::Term` holding its screen, fed by one of two
//! backends. No GPUI here: the view talks to it through `TerminalCore` (cheap to clone, every
//! field is an `Arc`) and an event channel.
//! - Local: a PTY child of this process, its output parsed by a parser thread.
//! - Daemon (M10a, study §2.3.2): a replica of a session the background daemon owns. It starts
//!   from the daemon's `Snapshot` and is fed its numbered `Output` and `Resized` frames, so it
//!   ends up in the daemon's state. A gap in the numbers, or a `Resync`, re-attaches for a fresh
//!   snapshot. Input and resizes go to the daemon through a per-terminal pump thread, so the UI
//!   never waits on the socket. The daemon answers terminal queries and works out status, title
//!   and exit; the replica's own query, title and bell events are dropped.
//!
//! Either way the bytes go through `blocks::advance`, which writes the command-block tags of a
//! shell's OSC 133 marks into the grid (M11l), as the background service does for its own Term.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use alacritty_terminal::event::{Event, EventListener, WindowSize};
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Boundary, Line, Point, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::{Config, Term, TermMode};
use alacritty_terminal::vte::ansi::{CursorShape, CursorStyle, Processor, Rgb, StdSyncHandler};
use futures::channel::mpsc::UnboundedSender;
use portable_pty::CommandBuilder;

use crate::blocks::{self, Block, Tagger};
use crate::colors;
use crate::daemon_client::{DaemonClient, SessionStream};
use crate::detector::{CwdReport, CwdReports, OscDetector};
use crate::grid::{self, GridSnapshot};
use crate::links::{Link, LinkFinder};
use crate::mux_proto::{Frame, SetMeta};
use crate::pty::{self, Pty};
use crate::shell_marks::MarkScanner;
use crate::status::Status;
use crate::theme::Theme;

/// Every terminal's default cursor style: "no program has chosen a shape". Programs can't ask
/// for a hollow block (`ESC [ n q` gives block, underline or bar), so a Term reporting this
/// shape has not been told one (or was reset with `ESC [ 0 q`), and the window draws the
/// `cursor-style` setting in its place (M11f). A program's `ESC [ ? 12 l` (curses' "cursor
/// normal") can clear its `blinking`; the shape still says "not chosen".
pub const CURSOR_UNSET: CursorStyle = CursorStyle {
    shape: CursorShape::HollowBlock,
    blinking: true,
};

/// The config every Term fuxx makes starts from, the window's and the background service's
/// alike (so a snapshot rebuilds the same): alacritty's defaults with `CURSOR_UNSET` as the
/// default cursor style.
pub fn term_config() -> Config {
    Config {
        default_cursor_style: CURSOR_UNSET,
        ..Config::default()
    }
}

/// Terminal size in cells. Implements alacritty's `Dimensions` so `Term` can use it directly.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TermDims {
    pub cols: usize,
    pub rows: usize,
}

/// Size of one cell in whole pixels, reported to the PTY and to `CSI 14 t`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CellPx {
    pub width: u16,
    pub height: u16,
}

impl Dimensions for TermDims {
    fn total_lines(&self) -> usize {
        self.rows // Term::new/resize only read screen_lines + columns
    }
    fn screen_lines(&self) -> usize {
        self.rows
    }
    fn columns(&self) -> usize {
        self.cols
    }
}

/// The reply a terminal query wants written back to the child, if `event` is one: `PtyWrite`
/// (DA, DSR/CPR, ...) as is, `ColorRequest` (OSC 4/10/11/12) with `color(index)`, and
/// `TextAreaSizeRequest` (`CSI 14 t`) with `size`. Without a reply, programs that ask (shells,
/// vim, Claude Code) hang waiting.
pub fn query_reply(
    event: &Event,
    color: impl FnOnce(usize) -> Rgb,
    size: WindowSize,
) -> Option<String> {
    match event {
        Event::PtyWrite(text) => Some(text.clone()),
        Event::ColorRequest(index, format) => Some(format(color(*index))),
        Event::TextAreaSizeRequest(format) => Some(format(size)),
        _ => None,
    }
}

/// What `CSI 14 t` reports for a terminal of `dims` cells of `cell` pixels.
pub fn window_size(dims: TermDims, cell: CellPx) -> WindowSize {
    WindowSize {
        num_lines: dims.rows as u16,
        num_cols: dims.cols as u16,
        cell_width: cell.width,
        cell_height: cell.height,
    }
}

/// Everything the UI needs to hear about, in order.
#[derive(Debug)]
pub enum TermEvent {
    /// Something the `Term` raised while parsing (query replies, title, bell, ...).
    Term(Event),
    /// New output was parsed into the screen: redraw. `notifications` = OSC 9 / 99 / 777
    /// notifications in it.
    Output { notifications: usize },
    /// The child closed the PTY (it exited). `code`: its exit code, `None` when it couldn't be
    /// read (or a version-1 daemon, which doesn't send it). A child killed by a signal reads as
    /// 1 (portable-pty's rule).
    Exited { code: Option<u32> },
    /// Daemon backend: the daemon's status for this session changed.
    Status(Status),
    /// Daemon backend: the connection to the daemon is gone. Not an exit: the session may
    /// still run there.
    DaemonLost,
}

/// Receives `Term` events. It runs *inside* `Processor::advance` while the Term lock is held,
/// so it must never lock the Term: it only forwards into the channel.
#[derive(Clone)]
pub struct Listener {
    events: UnboundedSender<TermEvent>,
    /// A daemon replica's: the daemon answers queries and reports title and bell, so the
    /// replica's own are dropped (answering them here would answer twice).
    replica: bool,
}

impl EventListener for Listener {
    fn send_event(&self, event: Event) {
        let daemons = matches!(
            event,
            Event::PtyWrite(_)
                | Event::ColorRequest(..)
                | Event::TextAreaSizeRequest(_)
                | Event::Title(_)
                | Event::ResetTitle
                | Event::Bell
        );
        if !(self.replica && daemons) {
            let _ = self.events.unbounded_send(TermEvent::Term(event));
        }
    }
}

/// The parser lives next to the Term under one lock so a stuck synchronized update can be
/// flushed from outside the parser thread (see `flush_stale_sync`).
struct Shared {
    term: Term<Listener>,
    processor: Processor<StdSyncHandler>,
    /// When the program first printed (not a snapshot, not `show_local`'s text).
    first_output: Option<Instant>,
    /// The screen clears so far: the cursor doesn't glide over a cleared screen (M11f).
    clears: ClearCounter,
    /// Writes the command-block tags (M11l); fresh with each snapshot a replica takes.
    tagger: Tagger,
    /// Finds the OSC 133 marks in the output, also when one is split over two reads.
    marks: MarkScanner,
}

/// Counts the screen clears in a terminal's output (`ESC [ 2 J`, `ESC [ 3 J`, `ESC c`), also
/// when one is split over two reads. A fresh screen from a snapshot counts too (`bump`).
#[derive(Default)]
struct ClearCounter {
    count: u64,
    /// How much of a clear the bytes so far end with: 1 after ESC, 2 after `ESC [`, 3 after
    /// `ESC [ 2` or `ESC [ 3`.
    state: u8,
}

impl ClearCounter {
    fn feed(&mut self, bytes: &[u8]) {
        let mut i = 0;
        while i < bytes.len() {
            if self.at_rest() {
                match memchr::memchr(0x1b, &bytes[i..]) {
                    Some(skip) => i += skip,
                    None => break,
                }
            }
            let b = bytes[i];
            i += 1;
            self.state = match (self.state, b) {
                (_, 0x1b) => 1,
                (1, b'[') => 2,
                (2, b'2' | b'3') => 3,
                (1, b'c') | (3, b'J') => {
                    self.count += 1;
                    0
                }
                _ => 0,
            };
        }
    }

    /// Nothing of a clear partly matched: a byte that is not ESC changes nothing.
    fn at_rest(&self) -> bool {
        self.state == 0
    }

    fn bump(&mut self) {
        self.count += 1;
        self.state = 0;
    }
}

/// Decides which batches tell the UI about themselves (M11n). The UI redraws the whole screen
/// on any `Output`, so while one it hasn't handled yet is on its way, another plain one adds
/// nothing: the redraw it asks for reads the Term after the batches parsed so far.
/// - A batch with notifications always sends its own `Output` with its own count: the agent
///   state and the notification ring hear every one.
/// - A plain batch sends `Output { notifications: 0 }` only if no plain one is pending, and
///   makes it pending until the UI says it has heard it (`seen`).
///
/// Why no frame is ever left stale: the view calls `seen` when it starts handling an `Output`,
/// then renders, taking the Term lock. A batch parsed (lock released) before that render is in
/// it. A batch whose `after_batch` found `pending` set ran before `seen` cleared it, so its
/// parse finished before the render's lock. A batch whose `after_batch` runs after `seen`
/// finds `pending` clear and sends.
#[derive(Default)]
pub(crate) struct OutputGate {
    pending: AtomicBool,
}

impl OutputGate {
    /// The event to send for a batch just parsed with `notifications` in it, if any.
    pub(crate) fn after_batch(&self, notifications: usize) -> Option<TermEvent> {
        if notifications > 0 {
            return Some(TermEvent::Output { notifications });
        }
        // Mark it pending; send only if it wasn't.
        (!self.pending.swap(true, Ordering::AcqRel))
            .then_some(TermEvent::Output { notifications: 0 })
    }

    /// The UI has heard the last `Output`: the next batch sends one.
    pub(crate) fn seen(&self) {
        self.pending.store(false, Ordering::Release);
    }
}

/// A local terminal's work for each batch of the child's output, done on the PTY's parser
/// thread, which owns it (so the scanners, the Term and the event channel see the batches in
/// read order).
struct LocalFeed {
    shared: Arc<FairMutex<Shared>>,
    // alacritty_terminal ignores notification and folder OSCs, so byte scanners watch the raw
    // stream.
    detector: OscDetector,
    reports: CwdReports,
    /// The working folder the program last reported, for `TerminalCore::reported_cwd`.
    cwd: Arc<Mutex<Option<CwdReport>>>,
    gate: Arc<OutputGate>,
    events: UnboundedSender<TermEvent>,
}

impl LocalFeed {
    fn new(
        shared: Arc<FairMutex<Shared>>,
        cwd: Arc<Mutex<Option<CwdReport>>>,
        gate: Arc<OutputGate>,
        events: UnboundedSender<TermEvent>,
    ) -> Self {
        Self {
            shared,
            detector: OscDetector::new(),
            reports: CwdReports::new(),
            cwd,
            gate,
            events,
        }
    }

    /// Scan `bytes`, parse them under one lock, then (unlocked) tell the UI if the gate says
    /// so. The Term events the parse raised went out first, from inside it.
    fn batch(&mut self, bytes: &[u8]) {
        let notifications = self.detector.feed(bytes);
        if let Some(dir) = self.reports.feed(bytes) {
            *self.cwd.lock().unwrap() = Some(dir);
        }
        {
            let mut guard = self.shared.lock();
            let Shared {
                term,
                processor,
                first_output,
                clears,
                tagger,
                marks,
            } = &mut *guard;
            first_output.get_or_insert_with(Instant::now);
            blocks::advance(processor, term, tagger, marks, bytes);
            clears.feed(bytes);
        } // lock released before waking the UI
        if let Some(event) = self.gate.after_batch(notifications) {
            let _ = self.events.unbounded_send(event);
        }
    }
}

/// Where a terminal's bytes come from and go to.
#[derive(Clone)]
enum Backend {
    /// A PTY child of this process.
    Local {
        pty: Arc<Mutex<Pty>>,
        /// The child's process id, read once it started.
        pid: Option<u32>,
        /// Bytes for the child, written by the writer thread so a child that stops reading
        /// can't block the UI.
        input: Sender<Vec<u8>>,
        /// Shared with the parser thread: which batches send an `Output`.
        gate: Arc<OutputGate>,
    },
    /// A replica of a daemon session.
    Daemon(Arc<DaemonLink>),
}

/// The daemon session a replica mirrors. Dropped with the last `TerminalCore` clone, it
/// detaches: the session keeps running in the daemon (closing it is `DaemonClient::close`).
struct DaemonLink {
    id: u64,
    /// The connection it goes through (to tell an old connection's views from the current).
    client: Arc<DaemonClient>,
    /// To the pump thread.
    commands: PumpSender,
    /// The first snapshot was asked for (an adopted session waits until it is shown).
    attached: AtomicBool,
    /// The first snapshot has landed: the replica's screen is the session's.
    drawn: Arc<AtomicBool>,
}

impl DaemonLink {
    fn send(&self, command: Command) {
        self.commands.send(command);
    }
}

/// The pump's queue, counted, so a quit can wait until it has gone out.
#[derive(Clone)]
struct PumpSender {
    tx: Sender<Command>,
    queued: Arc<AtomicUsize>,
}

impl PumpSender {
    fn send(&self, command: Command) {
        self.queued.fetch_add(1, Ordering::AcqRel);
        if self.tx.send(command).is_err() {
            self.queued.fetch_sub(1, Ordering::AcqRel);
        }
    }
}

impl Drop for DaemonLink {
    fn drop(&mut self) {
        self.send(Command::Detach);
    }
}

/// How many commands a core's pump still has to hand to the connection.
#[derive(Clone)]
pub struct PumpQueue(Arc<AtomicUsize>);

impl PumpQueue {
    pub fn is_empty(&self) -> bool {
        self.0.load(Ordering::Acquire) == 0
    }
}

/// What a replica's side asks of its daemon session, in order. The pump thread does it, so a
/// daemon that is slow to read never holds up the UI (as Local's unbounded input channel).
enum Command {
    /// Ask for a (fresh) snapshot.
    Attach,
    Write(Vec<u8>),
    Input(Vec<u8>),
    Resize(TermDims, CellPx),
    SetTheme(Theme),
    SetMeta(SetMeta),
    /// End the session (`close`).
    Close,
    /// The last `TerminalCore` clone is gone: stop the stream; the pump ends.
    Detach,
}

/// Send `commands` to daemon session `id` (stream `token`) until `Detach`; `queued` counts
/// those not handed to the connection yet.
fn pump(
    commands: &std::sync::mpsc::Receiver<Command>,
    queued: &AtomicUsize,
    client: &DaemonClient,
    id: u64,
    token: u64,
) {
    let done = || queued.fetch_sub(1, Ordering::AcqRel);
    for command in commands {
        let detach = matches!(command, Command::Detach);
        match command {
            Command::Attach => client.reattach(id),
            Command::Write(bytes) => client.write(id, &bytes),
            Command::Input(bytes) => client.input(id, &bytes),
            Command::Resize(dims, cell) => client.resize(id, dims, cell),
            Command::SetTheme(theme) => client.set_theme(&theme),
            Command::SetMeta(meta) => client.set_meta(meta),
            Command::Close => client.close(id),
            Command::Detach => {}
        }
        done();
        if detach {
            break;
        }
    }
    client.detach(id, token);
    // Anything after the `Detach` (a replica's late re-attach) is moot.
    while commands.try_recv().is_ok() {
        done();
    }
}

#[derive(Clone)]
pub struct TerminalCore {
    shared: Arc<FairMutex<Shared>>,
    backend: Backend,
    /// The size last asked for (Daemon: the replica takes it when the daemon's `Resized`
    /// arrives).
    dims: Arc<Mutex<(TermDims, CellPx)>>,
    /// Built once: compiling the URL regex is not free.
    links: Arc<Mutex<LinkFinder>>,
    /// Shared by every clone, so a theme change reaches them all.
    theme: Arc<Mutex<Theme>>,
    /// The working folder the program last reported (OSC 7) and the host it named, kept by
    /// the thread that reads its output.
    cwd: Arc<Mutex<Option<CwdReport>>>,
}

impl TerminalCore {
    pub fn spawn(
        cmd: CommandBuilder,
        dims: TermDims,
        events: UnboundedSender<TermEvent>,
        theme: Theme,
    ) -> Result<Self, String> {
        let (pty, reader, writer) = Pty::spawn(cmd, dims)?;
        let listener = Listener {
            events: events.clone(),
            replica: false,
        };
        let term = Term::new(term_config(), &dims, listener);
        let shared = Arc::new(FairMutex::new(Shared {
            term,
            processor: Processor::new(),
            first_output: None,
            clears: ClearCounter::default(),
            tagger: Tagger::new(),
            marks: MarkScanner::new(),
        }));

        let eof_events = events.clone();
        let exit = pty.exit_waiter();
        let cwd = Arc::new(Mutex::new(None));
        let gate = Arc::new(OutputGate::default());
        let mut feed = LocalFeed::new(shared.clone(), cwd.clone(), gate.clone(), events);
        pty::spawn_reader(
            reader,
            move |bytes| feed.batch(bytes),
            // After the last batch's `Output`, on the same thread.
            move || {
                let code = exit.exit_code();
                let _ = eof_events.unbounded_send(TermEvent::Exited { code });
            },
        )
        .map_err(|e| e.to_string())?;

        // The writer thread owns the PTY writer for the whole session. It ends when every
        // `TerminalCore` clone is dropped (the channel closes) or the child goes away.
        let input = pty::spawn_writer(writer).map_err(|e| e.to_string())?;

        Ok(Self {
            shared,
            backend: Backend::Local {
                pid: pty.pid(),
                pty: Arc::new(Mutex::new(pty)),
                input,
                gate,
            },
            dims: Arc::new(Mutex::new((dims, CellPx::default()))),
            links: Arc::new(Mutex::new(LinkFinder::new())),
            theme: Arc::new(Mutex::new(theme)),
            cwd,
        })
    }

    /// A replica of daemon session `id`: attaches, then mirrors the daemon's Term (Daemon
    /// backend).
    pub fn attach_daemon(
        client: Arc<DaemonClient>,
        id: u64,
        events: UnboundedSender<TermEvent>,
        theme: Theme,
    ) -> std::io::Result<Self> {
        // No size asked for yet, so the pane's first `resize` always goes to the daemon. The
        // replica starts at 80x24 until the snapshot brings the session's size.
        let asked = (TermDims { cols: 0, rows: 0 }, CellPx::default());
        Self::replica(client, id, asked, events, theme, true)
    }

    /// `attach_daemon` that doesn't ask for the snapshot yet (`attach` does): reopening fuxx
    /// with many sessions only fetches the screens shown. Input already reaches the session.
    pub fn adopt_daemon(
        client: Arc<DaemonClient>,
        id: u64,
        events: UnboundedSender<TermEvent>,
        theme: Theme,
    ) -> std::io::Result<Self> {
        let asked = (TermDims { cols: 0, rows: 0 }, CellPx::default());
        Self::replica(client, id, asked, events, theme, false)
    }

    /// Ask for an adopted session's first snapshot; true if this call asked (once per core).
    /// Local, or already attached: nothing.
    pub fn attach(&self) -> bool {
        match &self.backend {
            Backend::Daemon(link) if !link.attached.swap(true, Ordering::AcqRel) => {
                link.send(Command::Attach);
                true
            }
            _ => false,
        }
    }

    /// The Daemon backend: a replica Term, its feeding thread and its pump thread. No socket
    /// I/O here, so it is fine on the UI thread. `attach`: ask for the snapshot now.
    fn replica(
        client: Arc<DaemonClient>,
        id: u64,
        asked: (TermDims, CellPx),
        events: UnboundedSender<TermEvent>,
        theme: Theme,
        attach: bool,
    ) -> std::io::Result<Self> {
        let listener = Listener {
            events: events.clone(),
            replica: true,
        };
        // The daemon's Term has the same `term_config()` (and so the same history and cursor).
        let start = match asked.0 {
            TermDims { cols: 0, .. } | TermDims { rows: 0, .. } => TermDims { cols: 80, rows: 24 },
            dims => dims,
        };
        let term = Term::new(term_config(), &start, listener.clone());
        let shared = Arc::new(FairMutex::new(Shared {
            term,
            processor: Processor::new(),
            first_output: None,
            clears: ClearCounter::default(),
            tagger: Tagger::new(),
            marks: MarkScanner::new(),
        }));
        let (token, stream) = client.route(id);
        let (tx, pending) = std::sync::mpsc::channel();
        let commands = PumpSender {
            tx,
            queued: Arc::new(AtomicUsize::new(0)),
        };
        let drawn = Arc::new(AtomicBool::new(false));
        let link = Arc::new(DaemonLink {
            id,
            client: client.clone(),
            commands: commands.clone(),
            attached: AtomicBool::new(attach),
            drawn: drawn.clone(),
        });
        // So OSC 4/10/11/12 replies match what this window shows; then the first snapshot.
        link.send(Command::SetTheme(theme));
        if attach {
            link.send(Command::Attach);
        }
        let pumping = client.clone();
        let queued = commands.queued.clone();
        std::thread::Builder::new()
            .name(format!("fuxx-pump-s{id}"))
            .spawn(move || pump(&pending, &queued, &pumping, id, token))?;
        let cwd = Arc::new(Mutex::new(None));
        let replica = Replica {
            id,
            listener,
            events,
            commands,
            drawn,
            next: None,
            title: None,
            status: None,
            exit_code: None,
            reports: CwdReports::new(),
            cwd: cwd.clone(),
        };
        let feeding = shared.clone();
        std::thread::Builder::new()
            .name(format!("fuxx-replica-s{id}"))
            .spawn(move || replica.run(&stream, &feeding, &client))?;
        Ok(Self {
            shared,
            backend: Backend::Daemon(link),
            dims: Arc::new(Mutex::new(asked)),
            links: Arc::new(Mutex::new(LinkFinder::new())),
            theme: Arc::new(Mutex::new(theme)),
            cwd,
        })
    }

    /// Ask the daemon for a new session (`create`), then attach to it. Waits for the daemon,
    /// so tests only: the UI uses `DaemonClient::create_in_background`, then `attach_daemon`.
    #[cfg(test)]
    pub fn create_daemon(
        client: Arc<DaemonClient>,
        create: crate::mux_proto::Create,
        events: UnboundedSender<TermEvent>,
        theme: Theme,
    ) -> std::io::Result<Self> {
        let dims = TermDims {
            cols: create.dims.cols.into(),
            rows: create.dims.rows.into(),
        };
        let cell = CellPx {
            width: create.cell_px.w,
            height: create.cell_px.h,
        };
        let id = client.create(create)?;
        Self::replica(client, id, (dims, cell), events, theme, true)
    }

    /// Daemon backend: end the session in the daemon (its `Exited` follows). It is sent after
    /// the input already queued here, but the daemon discards input it hasn't delivered to the
    /// program yet. Local: nothing (dropping the last clone ends the child).
    pub fn close(&self) {
        if let Backend::Daemon(link) = &self.backend {
            link.send(Command::Close);
        }
    }

    /// Whether the screen is there to read: always for Local; for the Daemon backend once the
    /// first snapshot has landed (an adopted session's comes on `attach`).
    pub fn is_drawn(&self) -> bool {
        match &self.backend {
            Backend::Local { .. } => true,
            Backend::Daemon(link) => link.drawn.load(Ordering::Acquire),
        }
    }

    /// The UI has started handling an `Output`: the next batch parsed sends one again (see
    /// `OutputGate`). Call it before the render that `Output` asks for. A Daemon replica sends
    /// one `Output` per frame and has no gate: nothing.
    pub fn output_seen(&self) {
        if let Backend::Local { gate, .. } = &self.backend {
            gate.seen();
        }
    }

    /// The Daemon backend's queue to the daemon, to wait until what was sent has gone out.
    pub fn pump_queue(&self) -> Option<PumpQueue> {
        match &self.backend {
            Backend::Local { .. } => None,
            Backend::Daemon(link) => Some(PumpQueue(link.commands.queued.clone())),
        }
    }

    /// Tell the daemon the session's sidebar facts (Daemon backend; in order with the rest).
    pub fn set_meta(&self, meta: SetMeta) {
        if let Backend::Daemon(link) = &self.backend {
            link.send(Command::SetMeta(meta));
        }
    }

    /// Whether this mirrors a session through `client` (that very connection).
    pub fn uses_client(&self, client: &Arc<DaemonClient>) -> bool {
        match &self.backend {
            Backend::Local { .. } => false,
            Backend::Daemon(link) => Arc::ptr_eq(&link.client, client),
        }
    }

    /// The daemon session this mirrors; `None` for a Local terminal.
    pub fn daemon_id(&self) -> Option<u64> {
        match &self.backend {
            Backend::Local { .. } => None,
            Backend::Daemon(link) => Some(link.id),
        }
    }

    /// The allowed-scheme link under `point`, if any. The point is clamped to the grid under
    /// the same lock, so one computed from a stale display offset can't index out of range.
    pub fn link_at(&self, point: Point) -> Option<Link> {
        let term = &self.shared.lock().term;
        let point = point.grid_clamp(term, Boundary::Grid);
        self.links.lock().unwrap().find(term, point)
    }

    /// Queue bytes for the child (query replies, mouse reports). Never blocks. On Daemon these
    /// don't count as typing; `input` does.
    pub fn write(&self, bytes: &[u8]) {
        match &self.backend {
            Backend::Local { input, .. } => {
                let _ = input.send(bytes.to_vec());
            }
            Backend::Daemon(link) => link.send(Command::Write(bytes.to_vec())),
        }
    }

    /// The working folder the program last reported (OSC 7), if it ever did.
    pub fn reported_cwd(&self) -> Option<CwdReport> {
        self.cwd.lock().unwrap().clone()
    }

    /// Local: the child's process id. Daemon: none (the background service runs it).
    pub fn pid(&self) -> Option<u32> {
        match &self.backend {
            Backend::Local { pid, .. } => *pid,
            Backend::Daemon(_) => None,
        }
    }

    /// When the program first printed: its own bytes (a daemon session's after the first
    /// snapshot), not text from `show_local`. `None` until then.
    pub fn first_output(&self) -> Option<Instant> {
        self.shared.lock().first_output
    }

    /// Parse `text` into the screen as if the program had printed it; nothing reaches the
    /// program (an SSH session's reconnect line, M10b). The caller redraws.
    pub fn show_local(&self, text: &str) {
        let mut guard = self.shared.lock();
        let Shared {
            term,
            processor,
            clears,
            tagger,
            marks,
            ..
        } = &mut *guard;
        // Tagged like the program's output, so screenshots can draw command blocks.
        blocks::advance(processor, term, tagger, marks, text.as_bytes());
        clears.feed(text.as_bytes());
    }

    /// User input: snap the view back to the live bottom, drop any selection, then send the
    /// bytes. Query replies use `write` instead so they never move the view.
    pub fn input(&self, bytes: &[u8]) {
        {
            let mut guard = self.shared.lock();
            guard.term.scroll_display(Scroll::Bottom);
            guard.term.selection = None;
        }
        match &self.backend {
            Backend::Local { .. } => self.write(bytes),
            // Typing: the daemon's status tracker counts it (mouse reports go by `write`).
            Backend::Daemon(link) => link.send(Command::Input(bytes.to_vec())),
        }
    }

    /// Begin a selection of kind `ty` anchored at `point` (replaces any existing one).
    /// The point is clamped to the grid under the same lock.
    pub fn start_selection(&self, ty: SelectionType, point: Point, side: Side) {
        let term = &mut self.shared.lock().term;
        let point = point.grid_clamp(term, Boundary::Grid);
        term.selection = Some(Selection::new(ty, point, side));
    }

    /// Move the selection's moving end; with no selection, starts a simple one here.
    /// The point is clamped to the grid under the same lock.
    pub fn extend_selection(&self, point: Point, side: Side) {
        let term = &mut self.shared.lock().term;
        let point = point.grid_clamp(term, Boundary::Grid);
        match term.selection.as_mut() {
            Some(sel) => sel.update(point, side),
            None => term.selection = Some(Selection::new(SelectionType::Simple, point, side)),
        }
    }

    /// The selected text, or `None` when nothing is selected.
    pub fn selection_text(&self) -> Option<String> {
        self.shared.lock().term.selection_to_string()
    }

    /// Scroll the view through history (positive `Delta` = up).
    pub fn scroll(&self, scroll: Scroll) {
        self.shared.lock().term.scroll_display(scroll);
    }

    /// Lines the view is scrolled up from the live bottom (0 = live).
    pub fn display_offset(&self) -> usize {
        self.shared.lock().term.grid().display_offset()
    }

    /// The last `n` non-blank lines of the live screen, for the status heuristics.
    pub fn tail_text(&self, n: usize) -> String {
        grid::screen_tail_text(&self.shared.lock().term, n)
    }

    /// The terminal's text for a scripting read.
    pub fn text(&self, scope: grid::TextScope) -> String {
        grid::text_lines(&self.shared.lock().term, scope)
    }

    /// Resize screen + PTY. No-op when unchanged, so it's cheap to call every frame.
    /// Daemon: asks the daemon, and the replica resizes when the daemon's `Resized` arrives, at
    /// the same point in the output as the daemon's Term. Returns whether a new size was sent.
    pub fn resize(&self, dims: TermDims, cell: CellPx) -> bool {
        if dims.cols == 0 || dims.rows == 0 {
            return false;
        }
        // Held throughout, so racing resizes apply in the order they were stored.
        let mut current = self.dims.lock().unwrap();
        if *current == (dims, cell) {
            return false;
        }
        *current = (dims, cell);
        match &self.backend {
            Backend::Local { pty, .. } => {
                self.shared.lock().term.resize(dims);
                if let Ok(pty) = pty.lock() {
                    let _ = pty.resize(dims, cell);
                }
            }
            Backend::Daemon(link) => link.send(Command::Resize(dims, cell)),
        }
        true
    }

    pub fn dims(&self) -> TermDims {
        self.dims.lock().unwrap().0
    }

    /// Cell counts and cell pixel size from the last resize, for `CSI 14 t` replies.
    pub fn window_size(&self) -> WindowSize {
        let (dims, cell) = *self.dims.lock().unwrap();
        window_size(dims, cell)
    }

    /// The reply a terminal query (`PtyWrite`, `ColorRequest`, `TextAreaSizeRequest`) wants
    /// written back to the child; `None` for any other event.
    pub fn query_reply(&self, event: &Event) -> Option<String> {
        query_reply(event, |index| self.color(index), self.window_size())
    }

    pub fn mode(&self) -> TermMode {
        *self.shared.lock().term.mode()
    }

    pub fn theme(&self) -> Theme {
        *self.theme.lock().unwrap()
    }

    pub fn set_theme(&self, theme: Theme) {
        *self.theme.lock().unwrap() = theme;
        if let Backend::Daemon(link) = &self.backend {
            link.send(Command::SetTheme(theme));
        }
    }

    pub fn snapshot(&self) -> GridSnapshot {
        // Copy the theme out first so its lock is not held while the terminal is locked.
        let theme = self.theme();
        // Under one lock, so the clears match the screen.
        let shared = self.shared.lock();
        GridSnapshot {
            clears: shared.clears.count,
            ..grid::snapshot(&shared.term, &theme)
        }
    }

    /// `snapshot` with the command blocks to draw (M11l), under the same lock: the viewport
    /// and 8 rows below it, plus the `chosen` block wherever it is in scrollback.
    pub fn snapshot_with_blocks(&self, chosen: Option<&str>) -> GridSnapshot {
        let theme = self.theme();
        let shared = self.shared.lock();
        let blocks = blocks::visible(&shared.term, chosen);
        GridSnapshot {
            clears: shared.clears.count,
            blocks,
            ..grid::snapshot(&shared.term, &theme)
        }
    }

    /// The top row of the prompt shown now, if it is tagged, counted from the top of the
    /// scrollback (stable while it scrolls). For tests: the view no longer draws anything there.
    #[cfg(test)]
    pub fn live_prompt_from_top(&self) -> Option<i64> {
        let shared = self.shared.lock();
        let history = shared.term.grid().history_size() as i64;
        blocks::live_prompt(&shared.term).map(|line| i64::from(line.0) + history)
    }

    /// Every command block in the grid, oldest first (none on the alternate screen). Reads the
    /// whole scrollback under the Term lock: on a keypress or off the UI thread only, never per
    /// frame (`snapshot_with_blocks` is the per-frame read).
    pub fn blocks(&self) -> Vec<Block> {
        blocks::list(&self.shared.lock().term)
    }

    /// Block `id`'s output text; none when it is gone. Finds it with a column-0 scan, then
    /// reads its rows: call it off the UI thread.
    pub fn block_output(&self, id: &str) -> Option<String> {
        blocks::output_text(&self.shared.lock().term, id)
    }

    /// Block `id`'s command line, if it is still in the grid and was recorded (one column-0
    /// scan; only the matching tag is decoded).
    pub fn block_cmd(&self, id: &str) -> Option<String> {
        blocks::block_cmd(&self.shared.lock().term, id)
    }

    /// A prompt is shown and where typing starts at it is known (false after a reattach until
    /// the next prompt). (The view reads it through `rerun_facts`.)
    #[cfg(test)]
    pub fn prompt_ready(&self) -> bool {
        self.shared.lock().tagger.prompt_ready()
    }

    /// The cursor is where typing started at the shown prompt: nothing has been typed there.
    #[cfg(test)]
    pub fn cursor_at_input_start(&self) -> bool {
        let shared = self.shared.lock();
        shared.tagger.input_start() == Some(shared.term.grid().cursor.point)
    }

    /// What Rerun of block `id` depends on, under one lock: whether this terminal's tagger
    /// wrote the block in this run, the prompt, the cursor and the modes. Cheap (no scan).
    /// Whether the shell is at an empty prompt (and its paste mode), for typing a command
    /// into it (a folder's `cd` from Files): `rerun_facts` without a block.
    pub fn prompt_facts(&self) -> blocks::RerunFacts {
        Self::facts(&self.shared.lock(), "")
    }

    pub fn rerun_facts(&self, id: &str) -> blocks::RerunFacts {
        Self::facts(&self.shared.lock(), id)
    }

    /// `rerun_facts` with block `id`'s command, under the same lock (one column-0 scan: on a
    /// click or a keypress only).
    pub fn rerun_facts_and_cmd(&self, id: &str) -> (blocks::RerunFacts, Option<String>) {
        let shared = self.shared.lock();
        let facts = Self::facts(&shared, id);
        (facts, blocks::block_cmd(&shared.term, id))
    }

    fn facts(shared: &Shared, id: &str) -> blocks::RerunFacts {
        let mode = *shared.term.mode();
        blocks::RerunFacts {
            ours: shared.tagger.issued(id),
            prompt_ready: shared.tagger.prompt_ready(),
            at_input_start: shared.tagger.input_start() == Some(shared.term.grid().cursor.point),
            bracketed: mode.contains(TermMode::BRACKETED_PASTE),
            alt_screen: mode.contains(TermMode::ALT_SCREEN),
        }
    }

    /// Every block (as `blocks`) and the grid lines at the top and bottom of the view, under
    /// one lock: on a keypress only.
    pub fn blocks_and_view(&self) -> (Vec<Block>, (Line, Line)) {
        let shared = self.shared.lock();
        let grid = shared.term.grid();
        let top = Line(-(grid.display_offset() as i32));
        let bottom = top + (grid.screen_lines() as i32 - 1);
        (blocks::list(&shared.term), (top, bottom))
    }

    /// Scroll the view so grid line `line` is at its top, as far as the history allows.
    pub fn scroll_to_line(&self, line: Line) {
        let term = &mut self.shared.lock().term;
        let history = i32::try_from(term.grid().history_size()).unwrap_or(i32::MAX);
        let target = line.0.saturating_neg().clamp(0, history);
        let now = i32::try_from(term.grid().display_offset()).unwrap_or(i32::MAX);
        term.scroll_display(Scroll::Delta(target - now));
    }

    /// Color for an OSC 4/10/11/12 query reply.
    pub fn color(&self, index: usize) -> Rgb {
        let theme = self.theme();
        colors::resolve_index(index, self.shared.lock().term.colors(), &theme)
    }

    /// Synchronized updates (DEC mode 2026, used by Claude Code and most TUIs) buffer output
    /// until an end marker. If the marker never comes, the parser holds the frame until its
    /// deadline (~150 ms) *and* the next byte arrives. Called on a timer to flush it on time.
    /// Returns true if something was flushed (redraw needed).
    pub fn flush_stale_sync(&self) -> bool {
        let mut guard = self.shared.lock();
        let Shared {
            term, processor, ..
        } = &mut *guard;
        match processor.sync_timeout().sync_timeout() {
            Some(deadline) if Instant::now() >= deadline => {
                processor.stop_sync(term);
                true
            }
            _ => false,
        }
    }
}

/// What the replica thread does after a frame.
#[derive(Debug, PartialEq, Eq)]
enum Step {
    Continue,
    /// Out of step with the daemon: ask for a fresh snapshot.
    Reattach,
    /// A snapshot was applied: ask for the status.
    Snapshotted,
    /// The session ended.
    End,
}

/// Feeds a Daemon backend's replica Term from its session's frames, on its own thread.
struct Replica {
    id: u64,
    listener: Listener,
    events: UnboundedSender<TermEvent>,
    /// To the pump, for re-attaching.
    commands: PumpSender,
    /// Set once the first snapshot is in the Term.
    drawn: Arc<AtomicBool>,
    /// The seq the next `Output` or `Resized` must carry; `None` while waiting for a snapshot.
    next: Option<u64>,
    title: Option<String>,
    status: Option<Status>,
    /// From `ExitCode`, for the `Exited` that follows.
    exit_code: Option<u32>,
    /// The working-folder reports in the session's output (a snapshot has none), and the last.
    reports: CwdReports,
    cwd: Arc<Mutex<Option<CwdReport>>>,
}

impl Replica {
    fn run(mut self, stream: &SessionStream, shared: &FairMutex<Shared>, client: &DaemonClient) {
        while let Ok(frame) = stream.recv() {
            match self.apply(frame, shared) {
                Step::Continue => {}
                // Through the pump, so it stays in order with a `Detach`.
                Step::Reattach => {
                    // Out of step: where typing starts at the prompt is no longer known (so no
                    // rerun) until the next prompt after the new snapshot.
                    let mut guard = shared.lock();
                    guard.tagger = Tagger::new();
                    guard.marks = MarkScanner::new();
                    drop(guard);
                    self.commands.send(Command::Attach);
                }
                // Status frames from before the snapshot (or dropped with an overflowed
                // stream) never arrive: ask. Asked after the snapshot came, so the answer is at
                // least as new as it, and the frames after it apply on top.
                Step::Snapshotted => {
                    self.drawn.store(true, Ordering::Release);
                    self.refresh_status(client);
                }
                Step::End => return,
            }
        }
        // The stream ended without `Exited`: detached (no one is listening any more) or the
        // connection to the daemon is gone. Not an exit: the session may still run there.
        self.emit(TermEvent::DaemonLost);
    }

    fn apply(&mut self, frame: Frame, shared: &FairMutex<Shared>) -> Step {
        match frame {
            Frame::Snapshot(snap) => {
                let dims = wire_dims(snap.dims.cols, snap.dims.rows);
                {
                    // A fresh Term and parser: an old parser could be inside a synchronized
                    // update (DEC 2026) or an escape sequence, and would swallow the snapshot.
                    let mut guard = shared.lock();
                    guard.term = Term::new(term_config(), &dims, self.listener.clone());
                    guard.processor = Processor::new();
                    guard.clears.bump();
                    // The snapshot carries the service's tags; what the shell is doing between
                    // marks is unknown until its next prompt (so no rerun until then). An
                    // unfinished mark at the snapshot's end is finished by the next output.
                    guard.tagger = Tagger::new();
                    guard.marks = MarkScanner::new();
                    let Shared {
                        term,
                        processor,
                        tagger,
                        marks,
                        ..
                    } = &mut *guard;
                    blocks::advance(processor, term, tagger, marks, &snap.vt_bytes);
                }
                self.next = Some(snap.seq + 1);
                self.set_title(snap.title);
                self.emit(TermEvent::Output { notifications: 0 });
                return Step::Snapshotted;
            }
            Frame::Output {
                seq,
                notifications,
                bytes,
                ..
            } => {
                if let Err(step) = self.take_seq(seq) {
                    return step;
                }
                {
                    let mut guard = shared.lock();
                    let Shared {
                        term,
                        processor,
                        first_output,
                        clears,
                        tagger,
                        marks,
                    } = &mut *guard;
                    first_output.get_or_insert_with(Instant::now);
                    // The same bytes, split the same way, as the service's Term: tagged alike.
                    blocks::advance(processor, term, tagger, marks, &bytes);
                    clears.feed(&bytes);
                }
                if let Some(dir) = self.reports.feed(&bytes) {
                    *self.cwd.lock().unwrap() = Some(dir);
                }
                let notifications = usize::try_from(notifications).unwrap_or(usize::MAX);
                self.emit(TermEvent::Output { notifications });
            }
            Frame::Resized(r) => {
                if let Err(step) = self.take_seq(r.seq) {
                    return step;
                }
                shared.lock().term.resize(wire_dims(r.cols, r.rows));
                self.emit(TermEvent::Term(Event::Wakeup));
            }
            // Protocol 2: comes right before `Exited`, even while waiting for a snapshot.
            Frame::ExitCode { code, .. } => self.exit_code = code,
            Frame::Exited { .. } => {
                self.emit(TermEvent::Exited {
                    code: self.exit_code,
                });
                return Step::End;
            }
            // Until the snapshot, the rest belongs to the old stream: the snapshot brings the
            // title and `refresh_status` the status.
            _ if self.next.is_none() => {}
            Frame::Resync { .. } => {
                self.next = None;
                return Step::Reattach;
            }
            Frame::Status { status, .. } => {
                if let Some(status) = Status::parse(&status) {
                    self.set_status(status);
                }
            }
            Frame::Title { title, .. } => self.set_title(title),
            // `Bell` (the daemon's status tracker has it), and anything newer.
            _ => {}
        }
        Step::Continue
    }

    /// Accept numbered frame `seq` if it is the next one. Otherwise: while waiting for a
    /// snapshot, drop it; after a gap, re-attach.
    fn take_seq(&mut self, seq: u64) -> Result<(), Step> {
        match self.next {
            Some(next) if next == seq => {
                self.next = Some(next + 1);
                Ok(())
            }
            Some(_) => {
                self.next = None;
                Err(Step::Reattach)
            }
            None => Err(Step::Continue),
        }
    }

    fn refresh_status(&mut self, client: &DaemonClient) {
        match client.list() {
            Ok(sessions) => {
                let status = sessions
                    .iter()
                    .find(|s| s.id == self.id)
                    .and_then(|s| Status::parse(&s.status));
                if let Some(status) = status {
                    self.set_status(status);
                }
            }
            Err(e) => eprintln!("[fuxx] s{}: no status from the daemon: {e}", self.id),
        }
    }

    fn set_status(&mut self, status: Status) {
        if self.status != Some(status) {
            self.status = Some(status);
            self.emit(TermEvent::Status(status));
        }
    }

    fn set_title(&mut self, title: Option<String>) {
        if self.title != title {
            self.title.clone_from(&title);
            self.emit(TermEvent::Term(match title {
                Some(title) => Event::Title(title),
                None => Event::ResetTitle,
            }));
        }
    }

    fn emit(&self, event: TermEvent) {
        let _ = self.events.unbounded_send(event);
    }
}

/// A wire size as cells, at least 1x1.
fn wire_dims(cols: u16, rows: u16) -> TermDims {
    TermDims {
        cols: usize::from(cols).max(1),
        rows: usize::from(rows).max(1),
    }
}

/// The verbatim byte-at-a-time `feed` from before M11n: the oracle the fast one is proven against.
#[cfg(test)]
mod clear_bytewise {
    use super::*;

    impl ClearCounter {
        pub(super) fn feed_bytewise(&mut self, bytes: &[u8]) {
            for &b in bytes {
                self.state = match (self.state, b) {
                    (_, 0x1b) => 1,
                    (1, b'[') => 2,
                    (2, b'2' | b'3') => 3,
                    (1, b'c') | (3, b'J') => {
                        self.count += 1;
                        0
                    }
                    _ => 0,
                };
            }
        }
    }
}

#[cfg(test)]
mod clear_equivalence {
    use super::*;
    use crate::detector::fixtures::*;

    #[test]
    fn fast_scanners_match_bytewise_at_every_split() {
        let _ = ClearCounter::default().at_rest();
        for seed in 1..=200u64 {
            let stream = random_stream(seed);
            let mut slow = ClearCounter::default();
            for c in stream.chunks(1) {
                slow.feed_bytewise(c);
            }
            let mut fast = ClearCounter::default();
            for c in random_chunks(seed, &stream) {
                fast.feed(c);
            }
            assert_eq!(
                (fast.count, fast.state),
                (slow.count, slow.state),
                "seed {seed}"
            );
        }
    }

    #[test]
    fn fast_scanners_match_bytewise_split_at_every_offset() {
        let stream = every_sequence();
        let mut slow = ClearCounter::default();
        slow.feed_bytewise(&stream);
        assert!(slow.count >= 3);
        for cut in 0..=stream.len() {
            let (a, b) = stream.split_at(cut);
            let mut fast = ClearCounter::default();
            fast.feed(a);
            fast.feed(b);
            assert_eq!(
                (fast.count, fast.state),
                (slow.count, slow.state),
                "cut {cut}"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pty::BATCH_MAX;
    use alacritty_terminal::index::{Column, Line, Point, Side};
    use alacritty_terminal::selection::SelectionType;
    use futures::channel::mpsc::{UnboundedReceiver, unbounded};
    use std::time::Duration;

    /// Poll the channel until `pred` matches or 5 s pass. These tests never acknowledge an
    /// `Output` (`output_seen`), so the gate sends one plain `Output` and the rest of the
    /// output arrives quietly: while the channel is quiet, `pred` is asked again (with a
    /// `Wakeup`), so a predicate that reads the screen still sees it change.
    fn wait_for(
        rx: &mut UnboundedReceiver<TermEvent>,
        mut pred: impl FnMut(&TermEvent) -> bool,
    ) -> bool {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            match rx.try_recv() {
                Ok(ev) if pred(&ev) => return true,
                Ok(_) => {}
                Err(_) => {
                    if pred(&TermEvent::Term(Event::Wakeup)) {
                        return true;
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
        }
        false
    }

    /// The flood of Review Focus 1: 300000 lines (about 2 MB), a notification, then `END`.
    const FLOOD: &str = "seq 1 300000; printf '\\033]9;done\\007'; echo END";

    /// Run `FLOOD` without ever acknowledging an `Output`; every event until `Exited` (30 s).
    fn flood_events() -> (TerminalCore, Vec<TermEvent>) {
        let (tx, mut rx) = unbounded();
        let core = TerminalCore::spawn(
            sh(FLOOD),
            TermDims { cols: 40, rows: 10 },
            tx,
            Theme::default(),
        )
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(30);
        let mut events = Vec::new();
        while Instant::now() < deadline {
            match rx.try_recv() {
                Ok(e) => {
                    let exited = matches!(e, TermEvent::Exited { .. });
                    events.push(e);
                    if exited {
                        break;
                    }
                }
                Err(_) => std::thread::sleep(Duration::from_millis(5)),
            }
        }
        // Nothing after `Exited`.
        std::thread::sleep(Duration::from_millis(100));
        while let Ok(e) = rx.try_recv() {
            events.push(e);
        }
        (core, events)
    }

    #[test]
    fn every_byte_of_a_flood_reaches_the_screen() {
        let (core, events) = flood_events();
        assert_eq!(core.tail_text(3), "299999\n300000\nEND");
        // One plain `Output` for the whole unacknowledged flood (one per ~1 KiB read before
        // M11n), the notification's own, then `Exited`, last.
        let shape: Vec<String> = events.iter().map(|e| format!("{e:?}")).collect();
        assert_eq!(events.len(), 3, "{shape:?}");
        assert!(
            matches!(events[0], TermEvent::Output { notifications: 0 }),
            "{shape:?}"
        );
        assert!(
            matches!(events[1], TermEvent::Output { notifications: 1 }),
            "{shape:?}"
        );
        assert!(matches!(events[2], TermEvent::Exited { .. }), "{shape:?}");
    }

    #[test]
    fn an_agent_still_settles_after_a_batched_flood() {
        use crate::status::{IDLE_AFTER, StatusTracker};
        let (_core, events) = flood_events();
        let outputs: Vec<usize> = events
            .iter()
            .filter_map(|e| match e {
                TermEvent::Output { notifications } => Some(*notifications),
                _ => None,
            })
            .collect();
        assert!(!outputs.is_empty());
        let t0 = Instant::now();
        let at = |i: usize| t0 + Duration::from_millis(500 + 10 * i as u64);
        let prompt = || "% ".to_string();

        // The plain batches: Working, then Done once quiet for IDLE_AFTER.
        let mut plain = StatusTracker::new();
        plain.on_input(t0); // typed first, outside the echo window
        let mut last = t0;
        for (i, _) in outputs.iter().enumerate().filter(|(_, n)| **n == 0) {
            last = at(i);
            plain.on_output(last, 0);
        }
        assert_eq!(plain.status(), Status::Working);
        assert_eq!(
            plain.tick(last + IDLE_AFTER - Duration::from_millis(1), prompt),
            None
        );
        assert_eq!(plain.tick(last + IDLE_AFTER, prompt), Some(Status::Done));

        // Every gate event, the notification's included: settles on the next tick, as today.
        let mut noted = StatusTracker::new();
        noted.on_input(t0);
        for (i, n) in outputs.iter().enumerate() {
            last = at(i);
            noted.on_output(last, *n);
        }
        assert_eq!(noted.status(), Status::Working);
        assert_eq!(
            noted.tick(last + Duration::from_millis(1), prompt),
            Some(Status::Done)
        );
    }

    #[test]
    fn a_notification_batch_always_sends_its_own_output() {
        let gate = OutputGate::default();
        assert!(matches!(
            gate.after_batch(0),
            Some(TermEvent::Output { notifications: 0 })
        ));
        assert!(gate.after_batch(0).is_none());
        assert!(matches!(
            gate.after_batch(2),
            Some(TermEvent::Output { notifications: 2 })
        ));
        // Over a seeded run of batches, never acknowledged: every notification is delivered.
        let gate = OutputGate::default();
        let mut state = 5;
        let (mut fed, mut delivered) = (0, 0);
        for _ in 0..10_000 {
            let n = match crate::detector::fixtures::splitmix(&mut state) % 10 {
                0 => 1,
                1 => 3,
                _ => 0,
            };
            fed += n;
            if let Some(TermEvent::Output { notifications }) = gate.after_batch(n) {
                assert!(n == 0 || notifications == n);
                delivered += notifications;
            }
        }
        assert!(fed > 0);
        assert_eq!(delivered, fed);
    }

    #[test]
    fn the_batch_after_an_acknowledged_output_always_sends() {
        let gate = OutputGate::default();
        assert!(gate.after_batch(0).is_some());
        gate.seen();
        assert!(gate.after_batch(0).is_some());
        assert!(gate.after_batch(0).is_none());
    }

    #[test]
    fn output_seen_lets_the_next_batch_send() {
        let (tx, mut rx) = unbounded();
        let core = TerminalCore::spawn(
            sh("printf one; sleep 0.3; printf two; sleep 0.3; printf three; sleep 5"),
            TermDims { cols: 40, rows: 5 },
            tx,
            Theme::default(),
        )
        .unwrap();
        // As the view does: acknowledge each `Output` as it is handled.
        let mut outputs = 0;
        let deadline = Instant::now() + Duration::from_secs(5);
        while outputs < 3 && Instant::now() < deadline {
            match rx.try_recv() {
                Ok(TermEvent::Output { .. }) => {
                    core.output_seen();
                    outputs += 1;
                }
                Ok(_) => {}
                Err(_) => std::thread::sleep(Duration::from_millis(5)),
            }
        }
        assert_eq!(outputs, 3);
        assert!(screen_text(&core).contains("onetwothree"));
    }

    #[test]
    fn marks_survive_batch_boundaries() {
        // Three prompts and commands (zsh integration's OSC 133 A/B/C/D), the second with
        // enough output for several full batches.
        let mut stream = Vec::new();
        let mut marks = Vec::new();
        let mut mark = |stream: &mut Vec<u8>, m: &str| {
            marks.push(stream.len());
            stream.extend_from_slice(format!("\x1b]133;{m}\x07").as_bytes());
        };
        for (i, cmd) in ["ls", "seq -w 1 1500", "true"].iter().enumerate() {
            mark(&mut stream, "A");
            stream.extend_from_slice(b"% ");
            mark(&mut stream, "B");
            stream.extend_from_slice(format!("{cmd}\r\n").as_bytes());
            mark(
                &mut stream,
                &format!("C;cmdline_url={}", cmd.replace(' ', "%20")),
            );
            // Wide lines, so the output fills batches without scrolling the first block out
            // of the history.
            let lines = if i == 1 { 1500 } else { 2 };
            for n in 1..=lines {
                stream.extend_from_slice(format!("{n:0>150}\r\n").as_bytes());
            }
            mark(&mut stream, "D;0");
        }
        mark(&mut stream, "A");
        stream.extend_from_slice(b"% ");
        mark(&mut stream, "B");
        assert!(stream.len() > 3 * BATCH_MAX);
        // Cut inside every mark (3 bytes in), and nowhere more than BATCH_MAX apart.
        let mut cuts: Vec<usize> = marks.iter().map(|m| m + 3).collect();
        cuts.push(stream.len());
        let mut pieces = Vec::new();
        let mut from = 0;
        for cut in cuts {
            while cut - from > BATCH_MAX {
                pieces.push(&stream[from..from + BATCH_MAX]);
                from += BATCH_MAX;
            }
            pieces.push(&stream[from..cut]);
            from = cut;
        }
        assert!(pieces.iter().any(|p| p.len() == BATCH_MAX));
        assert!(pieces.iter().all(|p| p.len() <= BATCH_MAX));

        let dims = TermDims {
            cols: 160,
            rows: 10,
        };
        let (tx, _rx) = unbounded();
        let batched = TerminalCore::spawn(sh("sleep 5"), dims, tx, Theme::default()).unwrap();
        let (tx, _rx) = unbounded();
        let mut feed = LocalFeed::new(
            batched.shared.clone(),
            Arc::new(Mutex::new(None)),
            Arc::new(OutputGate::default()),
            tx,
        );
        for piece in &pieces {
            feed.batch(piece);
        }
        let (tx, _rx) = unbounded();
        let whole = TerminalCore::spawn(sh("sleep 5"), dims, tx, Theme::default()).unwrap();
        whole.show_local(std::str::from_utf8(&stream).unwrap());

        // The taggers' ids are seeded per terminal: compare everything else.
        let read = |core: &TerminalCore| {
            core.blocks()
                .into_iter()
                .map(|b| {
                    let out = core.block_output(&b.id);
                    (b.outcome, b.prompt_top, b.cmd_row, b.output, b.cmd, out)
                })
                .collect::<Vec<_>>()
        };
        let (got, want) = (read(&batched), read(&whole));
        assert_eq!(want.len(), 3, "{want:?}");
        assert_eq!(got, want);
        assert_eq!(batched.prompt_ready(), whole.prompt_ready());
    }

    #[test]
    fn dropping_a_terminal_mid_flood_ends_its_threads() {
        let (tx, mut rx) = unbounded();
        let core = TerminalCore::spawn(
            sh("yes fuxx-flood"),
            TermDims { cols: 40, rows: 5 },
            tx,
            Theme::default(),
        )
        .unwrap();
        assert!(wait_for(&mut rx, |e| matches!(e, TermEvent::Output { .. })));
        std::thread::sleep(Duration::from_millis(100)); // mid-flood
        drop(core); // the pane closed
        // The parser thread sends `Exited` after the last batch, then lets go of the last
        // sender (the Term's listener with it): the channel closes.
        let deadline = Instant::now() + Duration::from_secs(1);
        let mut exited = false;
        let closed = loop {
            match rx.try_recv() {
                Ok(TermEvent::Exited { .. }) => exited = true,
                Ok(_) => {}
                Err(futures::channel::mpsc::TryRecvError::Closed) => break true,
                Err(_) if Instant::now() >= deadline => break false,
                Err(_) => std::thread::sleep(Duration::from_millis(5)),
            }
        };
        assert!(exited, "no Exited");
        assert!(
            closed,
            "the parser thread still holds the terminal after 1 s"
        );
    }

    fn sh(script: &str) -> CommandBuilder {
        let mut cmd = CommandBuilder::new("/bin/sh");
        cmd.args(["-c", script]);
        cmd
    }

    fn screen_text(core: &TerminalCore) -> String {
        core.snapshot()
            .lines
            .iter()
            .map(|l| l.iter().map(|c| c.ch).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn scrolling_moves_the_view_and_input_snaps_back() {
        let (tx, mut rx) = unbounded();
        let core = TerminalCore::spawn(
            sh("for i in $(seq 1 40); do echo line$i; done; sleep 2"),
            TermDims { cols: 20, rows: 5 },
            tx,
            Theme::default(),
        )
        .unwrap();
        assert!(wait_for(&mut rx, |_| screen_text(&core).contains("line40")));
        core.scroll(Scroll::Delta(5));
        assert_eq!(core.display_offset(), 5);
        core.write(b"x"); // a query reply must not move the view
        assert_eq!(core.display_offset(), 5);
        core.input(b"x"); // typing does
        assert_eq!(core.display_offset(), 0);
    }

    #[test]
    fn child_output_reaches_the_screen_and_exit_is_reported() {
        let (tx, mut rx) = unbounded();
        let core = TerminalCore::spawn(
            sh("printf 'hello fuxx'"),
            TermDims { cols: 40, rows: 5 },
            tx,
            Theme::default(),
        )
        .unwrap();
        assert!(wait_for(&mut rx, |e| matches!(e, TermEvent::Exited { .. })));
        assert!(screen_text(&core).contains("hello fuxx"));
    }

    /// The code of the first `Exited`, if one comes within 5 s.
    fn exit_code(rx: &mut UnboundedReceiver<TermEvent>) -> Option<Option<u32>> {
        let mut code = None;
        wait_for(rx, |e| match e {
            TermEvent::Exited { code: c } => {
                code = Some(*c);
                true
            }
            _ => false,
        });
        code
    }

    fn argv(words: &[&str]) -> Vec<String> {
        words.iter().map(|w| w.to_string()).collect()
    }

    #[test]
    fn pty_runs_argv_without_a_shell() {
        let env = pty::ShellEnv {
            base: None,
            set: Vec::new(),
            remove: Vec::new(),
        };
        let cmd = pty::command(Some(&argv(&["/bin/sh", "-c", "exit 255"])), None, &env);
        assert_eq!(
            cmd.get_env("TERM").and_then(|v| v.to_str()),
            Some("xterm-256color")
        );
        let (tx, mut rx) = unbounded();
        let _core =
            TerminalCore::spawn(cmd, TermDims { cols: 40, rows: 5 }, tx, Theme::default()).unwrap();
        assert_eq!(exit_code(&mut rx), Some(Some(255)));
        // One argument as given: a shell would have run `a` and `b`.
        let (tx, mut rx) = unbounded();
        let core = TerminalCore::spawn(
            pty::command(Some(&argv(&["/bin/echo", "a;b"])), None, &env),
            TermDims { cols: 40, rows: 5 },
            tx,
            Theme::default(),
        )
        .unwrap();
        assert_eq!(exit_code(&mut rx), Some(Some(0)));
        assert!(screen_text(&core).contains("a;b"), "{}", screen_text(&core));
        // Killed by a signal: portable-pty reports 1.
        let (tx, mut rx) = unbounded();
        let _core = TerminalCore::spawn(
            pty::command(Some(&argv(&["/bin/sh", "-c", "kill -9 $$"])), None, &env),
            TermDims { cols: 40, rows: 5 },
            tx,
            Theme::default(),
        )
        .unwrap();
        assert_eq!(exit_code(&mut rx), Some(Some(1)));
    }

    #[test]
    fn first_output_is_when_the_program_first_printed() {
        let (tx, mut rx) = unbounded();
        let core = TerminalCore::spawn(
            sh("sleep 1; echo hi; sleep 5"),
            TermDims { cols: 40, rows: 5 },
            tx,
            Theme::default(),
        )
        .unwrap();
        // Text fuxx writes into the screen itself is not the program's.
        core.show_local("Connecting\r\n");
        assert_eq!(core.first_output(), None);
        assert!(wait_for(&mut rx, |_| screen_text(&core).contains("hi")));
        let first = core.first_output().expect("printed");
        assert!(first.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn clears_are_counted_across_chunks() {
        let mut c = ClearCounter::default();
        c.feed(b"a\x1b[2Jb");
        assert_eq!(c.count, 1);
        // Split over reads, and the other two kinds.
        c.feed(b"x\x1b[");
        c.feed(b"3");
        c.feed(b"J\x1bc");
        assert_eq!(c.count, 3);
        // Not a clear: erase below, erase line, a cursor move, a stray ESC before a clear.
        c.feed(b"\x1b[J\x1b[K\x1b[2;3H\x1b[2K");
        assert_eq!(c.count, 3);
        c.feed(b"\x1b\x1b[2J");
        assert_eq!(c.count, 4);
    }

    #[test]
    fn a_local_program_clearing_the_screen_is_counted() {
        let (tx, mut rx) = unbounded();
        let core = TerminalCore::spawn(
            sh("printf 'one\\n'; printf '\\033[2J\\033[Hdone'; sleep 5"),
            TermDims { cols: 40, rows: 5 },
            tx,
            Theme::default(),
        )
        .unwrap();
        assert!(wait_for(&mut rx, |_| screen_text(&core).contains("done")));
        assert_eq!(core.snapshot().clears, 1);
    }

    #[test]
    fn show_local_writes_to_the_screen_only() {
        // The child prints the first line it reads: had the text reached it, that would be it.
        let (tx, mut rx) = unbounded();
        let core = TerminalCore::spawn(
            sh("stty -echo; echo ready; IFS= read -r l; printf 'got[%s]' \"$l\"; sleep 5"),
            TermDims { cols: 40, rows: 5 },
            tx,
            Theme::default(),
        )
        .unwrap();
        assert!(wait_for(&mut rx, |_| screen_text(&core).contains("ready")));
        core.show_local("\r\nDisconnected from devbox\r\n");
        assert!(screen_text(&core).contains("Disconnected from devbox"));
        core.input(b"typed\r");
        assert!(wait_for(&mut rx, |_| screen_text(&core).contains("got[")));
        let text = screen_text(&core);
        assert!(text.contains("got[typed]"), "{text}");
    }

    /// One command run at a prompt, then the next prompt, as a shell with OSC 133 marks
    /// prints it (`printf` format: `\n` becomes `\r\n` on the way through the PTY).
    const ONE_COMMAND: &str = r"\033]133;A\007$ \033]133;B\007ls\n\033]133;C;cmdline_url=ls\007ok\n\033]133;D;0\007\033]133;A\007$ \033]133;B\007";

    fn assert_one_ls_block(core: &TerminalCore) {
        let blocks = core.blocks();
        assert_eq!(blocks.len(), 1, "{blocks:?}");
        assert_eq!(blocks[0].cmd.as_deref(), Some("ls"));
        assert_eq!(blocks[0].outcome, blocks::Outcome::Ok);
        assert_eq!(core.block_output(&blocks[0].id).as_deref(), Some("ok"));
        assert_eq!(core.block_cmd(&blocks[0].id).as_deref(), Some("ls"));
        assert!(core.prompt_ready());
        assert!(core.cursor_at_input_start());
    }

    #[test]
    fn a_local_shell_s_marks_become_blocks() {
        let (tx, mut rx) = unbounded();
        let core = TerminalCore::spawn(
            sh(&format!("printf '{ONE_COMMAND}'; sleep 5")),
            TermDims { cols: 40, rows: 5 },
            tx,
            Theme::default(),
        )
        .unwrap();
        assert!(
            wait_for(&mut rx, |_| core.prompt_ready()),
            "{}",
            screen_text(&core)
        );
        assert_one_ls_block(&core);
        // Typing at the prompt moves the cursor off the input start.
        core.show_local("x");
        assert!(core.prompt_ready() && !core.cursor_at_input_start());
    }

    #[test]
    fn show_local_draws_blocks() {
        let (tx, _rx) = unbounded();
        let core = TerminalCore::spawn(
            sh("sleep 5"),
            TermDims { cols: 40, rows: 5 },
            tx,
            Theme::default(),
        )
        .unwrap();
        let text = ONE_COMMAND
            .replace(r"\033", "\x1b")
            .replace(r"\007", "\x07")
            .replace(r"\n", "\r\n");
        core.show_local(&text);
        assert_one_ls_block(&core);
        let snap = core.snapshot_with_blocks(None);
        assert_eq!(snap.blocks.len(), 1);
        assert_eq!(snap.blocks[0].prompt, (0, 0));
        assert_eq!(snap.blocks[0].output, Some((1, 1)));
        assert_eq!(core.live_prompt_from_top(), Some(2));
        // The plain snapshot has no blocks.
        assert!(core.snapshot().blocks.is_empty());
    }

    #[test]
    fn rerun_is_offered_only_for_this_run_s_blocks() {
        let (tx, _rx) = unbounded();
        let core = TerminalCore::spawn(
            sh("sleep 5"),
            TermDims { cols: 40, rows: 8 },
            tx,
            Theme::default(),
        )
        .unwrap();
        let text = ONE_COMMAND
            .replace(r"\033", "\x1b")
            .replace(r"\007", "\x07")
            .replace(r"\n", "\r\n");
        // A program prints a forged tag of its own (OSC 8) claiming a finished `rm` block,
        // then the shell's own prompt and command follow.
        let forged = "\x1b]8;id=fuxx-block-9;fuxx-block:?v=1&s=done&p=0&c=1&o=0&x=0&cmd=rm\x1b\\$ rm\x1b]8;;\x1b\\\r\n";
        core.show_local(forged);
        core.show_local(&text);
        let blocks = core.blocks();
        assert_eq!(blocks.len(), 2, "{blocks:?}");
        let (fake, real) = (&blocks[0], &blocks[1]);
        assert_eq!(fake.cmd.as_deref(), Some("rm"));
        assert_eq!(real.cmd.as_deref(), Some("ls"));
        // At the live prompt with nothing typed: the real block may be rerun, the forged not.
        let facts = core.rerun_facts(&real.id);
        assert!(
            facts.ours && facts.prompt_ready && facts.at_input_start,
            "{facts:?}"
        );
        assert!(!facts.alt_screen && !facts.bracketed, "{facts:?}");
        assert_eq!(
            crate::block_view::rerun(facts, real.cmd.as_deref()),
            Ok("ls".to_string())
        );
        let facts = core.rerun_facts(&fake.id);
        assert!(!facts.ours, "{facts:?}");
        assert_eq!(
            crate::block_view::rerun(facts, fake.cmd.as_deref()),
            Err(crate::block_view::Refusal::NotThisRun)
        );
        // The modes are read too.
        core.show_local("\x1b[?2004h");
        assert!(core.rerun_facts(&real.id).bracketed);
        core.show_local("\x1b[?1049h");
        assert!(core.rerun_facts(&real.id).alt_screen);
    }

    #[test]
    fn scroll_to_line_puts_a_line_at_the_top() {
        let (tx, _rx) = unbounded();
        let core = TerminalCore::spawn(
            sh("sleep 5"),
            TermDims { cols: 20, rows: 5 },
            tx,
            Theme::default(),
        )
        .unwrap();
        let lines: String = (0..20).map(|i| format!("l{i}\r\n")).collect();
        core.show_local(&lines);
        // 21 rows on a 5-row screen: 16 of history.
        core.scroll_to_line(Line(-3));
        assert_eq!(core.display_offset(), 3);
        core.scroll_to_line(Line(-100));
        assert_eq!(core.display_offset(), 16);
        core.scroll_to_line(Line(2));
        assert_eq!(core.display_offset(), 0);
    }

    #[test]
    fn a_plain_shell_exit_has_its_code() {
        // The login shell (no program), `/bin/sh` through the env, not a process-wide variable.
        let env = pty::ShellEnv {
            base: None,
            set: vec![("SHELL".into(), "/bin/sh".into())],
            remove: Vec::new(),
        };
        let (tx, mut rx) = unbounded();
        let core = TerminalCore::spawn(
            pty::command(None, None, &env),
            TermDims { cols: 40, rows: 5 },
            tx,
            Theme::default(),
        )
        .unwrap();
        core.write(b"exit 3\r");
        assert_eq!(exit_code(&mut rx), Some(Some(3)));
    }

    #[test]
    fn cursor_position_query_produces_a_pty_write_reply() {
        let (tx, mut rx) = unbounded();
        let _core = TerminalCore::spawn(
            sh("printf '\\033[6n'; sleep 1"),
            TermDims { cols: 40, rows: 5 },
            tx,
            Theme::default(),
        )
        .unwrap();
        assert!(wait_for(
            &mut rx,
            |e| matches!(e, TermEvent::Term(Event::PtyWrite(s)) if s == "\x1b[1;1R")
        ));
    }

    #[test]
    fn written_bytes_reach_the_child() {
        let (tx, mut rx) = unbounded();
        let core = TerminalCore::spawn(
            sh("read line; printf \"got:$line\""),
            TermDims { cols: 40, rows: 5 },
            tx,
            Theme::default(),
        )
        .unwrap();
        core.write(b"ping\r");
        assert!(wait_for(&mut rx, |e| matches!(e, TermEvent::Exited { .. })));
        assert!(screen_text(&core).contains("got:ping"));
    }

    #[test]
    fn resize_changes_the_grid_and_ignores_zero() {
        let (tx, _rx) = unbounded();
        let core = TerminalCore::spawn(
            sh("sleep 1"),
            TermDims { cols: 40, rows: 5 },
            tx,
            Theme::default(),
        )
        .unwrap();
        core.resize(TermDims { cols: 0, rows: 0 }, CellPx::default());
        assert_eq!(core.dims(), TermDims { cols: 40, rows: 5 });
        core.resize(TermDims { cols: 60, rows: 10 }, CellPx::default());
        let snap = core.snapshot();
        assert_eq!((snap.cols, snap.rows), (60, 10));
    }

    #[test]
    fn csi_14t_reports_real_pixel_size() {
        let (tx, mut rx) = unbounded();
        let core = TerminalCore::spawn(
            sh("sleep 1; printf '\\033[14t'; sleep 1"),
            TermDims { cols: 80, rows: 24 },
            tx,
            Theme::default(),
        )
        .unwrap();
        core.resize(
            TermDims { cols: 80, rows: 24 },
            CellPx {
                width: 8,
                height: 17,
            },
        );
        let mut reply = None;
        assert!(wait_for(&mut rx, |e| match e {
            TermEvent::Term(Event::TextAreaSizeRequest(format)) => {
                reply = Some(format(core.window_size()));
                true
            }
            _ => false,
        }));
        assert_eq!(reply.as_deref(), Some("\x1b[4;408;640t"));
    }

    #[test]
    fn query_reply_answers_the_three_query_kinds() {
        let size = WindowSize {
            num_lines: 24,
            num_cols: 80,
            cell_width: 8,
            cell_height: 17,
        };
        let red = Rgb { r: 255, g: 0, b: 0 };
        let pty = Event::PtyWrite("\x1b[1;1R".into());
        assert_eq!(
            query_reply(&pty, |_| unreachable!(), size).as_deref(),
            Some("\x1b[1;1R")
        );
        let color = Event::ColorRequest(11, Arc::new(|c: Rgb| format!("{}:{}:{}", c.r, c.g, c.b)));
        let mut asked = None;
        let reply = query_reply(
            &color,
            |i| {
                asked = Some(i);
                red
            },
            size,
        );
        assert_eq!((reply.as_deref(), asked), (Some("255:0:0"), Some(11)));
        let area = Event::TextAreaSizeRequest(Arc::new(|w: WindowSize| {
            format!(
                "{}x{}",
                w.num_cols * w.cell_width,
                w.num_lines * w.cell_height
            )
        }));
        assert_eq!(
            query_reply(&area, |_| red, size).as_deref(),
            Some("640x408")
        );
        assert_eq!(query_reply(&Event::Bell, |_| red, size), None);
        assert_eq!(query_reply(&Event::Title("t".into()), |_| red, size), None);
    }

    #[test]
    fn colour_queries_are_answered_from_the_theme() {
        let (tx, mut rx) = unbounded();
        let mut theme = Theme::default();
        theme.background = crate::theme::parse_color("#123456").unwrap();
        let core = TerminalCore::spawn(
            sh("printf '\\033]11;?\\007'; sleep 1"),
            TermDims { cols: 40, rows: 5 },
            tx,
            theme,
        )
        .unwrap();
        let mut reply = None;
        assert!(wait_for(&mut rx, |e| match e {
            TermEvent::Term(event @ Event::ColorRequest(..)) => {
                reply = core.query_reply(event);
                true
            }
            _ => false,
        }));
        assert_eq!(reply.as_deref(), Some("\x1b]11;rgb:1212/3434/5656\x07"));
        let cpr = Event::PtyWrite("\x1b[1;1R".into());
        assert_eq!(core.query_reply(&cpr).as_deref(), Some("\x1b[1;1R"));
        assert_eq!(core.query_reply(&Event::Bell), None);
    }

    #[test]
    fn extend_without_a_selection_starts_one() {
        let (tx, mut rx) = unbounded();
        let core = TerminalCore::spawn(
            sh("printf 'hello world'; sleep 2"),
            TermDims { cols: 20, rows: 3 },
            tx,
            Theme::default(),
        )
        .unwrap();
        assert!(wait_for(&mut rx, |_| screen_text(&core).contains("world")));
        core.extend_selection(Point::new(Line(0), Column(0)), Side::Left);
        core.extend_selection(Point::new(Line(0), Column(4)), Side::Right);
        assert_eq!(core.selection_text().as_deref(), Some("hello"));
    }

    #[test]
    fn width_change_clears_selection() {
        let (tx, mut rx) = unbounded();
        let core = TerminalCore::spawn(
            sh("printf 'hello'; sleep 2"),
            TermDims { cols: 20, rows: 3 },
            tx,
            Theme::default(),
        )
        .unwrap();
        assert!(wait_for(&mut rx, |_| screen_text(&core).contains("hello")));
        core.start_selection(
            SelectionType::Simple,
            Point::new(Line(0), Column(0)),
            Side::Left,
        );
        core.extend_selection(Point::new(Line(0), Column(4)), Side::Right);
        assert_eq!(core.selection_text().as_deref(), Some("hello"));
        core.resize(
            TermDims { cols: 15, rows: 3 },
            CellPx {
                width: 8,
                height: 17,
            },
        );
        assert_eq!(core.selection_text(), None);
    }

    #[test]
    fn the_terminal_config_starts_every_term_with_the_marker() {
        use alacritty_terminal::vte::ansi::{CursorShape, CursorStyle};
        assert_eq!(
            CURSOR_UNSET,
            CursorStyle {
                shape: CursorShape::HollowBlock,
                blinking: true
            }
        );
        let config = term_config();
        assert_eq!(config.default_cursor_style, CURSOR_UNSET);
        // Everything else is alacritty's default (the same history as before).
        assert_eq!(
            config.scrolling_history,
            Config::default().scrolling_history
        );
    }

    #[test]
    fn a_local_terminal_has_the_marker_until_a_program_sets_a_shape() {
        use alacritty_terminal::vte::ansi::CursorShape;
        let (tx, mut rx) = unbounded();
        let core = TerminalCore::spawn(
            sh("printf 'a'; sleep 0.5; printf '\\033[6 q'; sleep 1"),
            TermDims { cols: 40, rows: 5 },
            tx,
            Theme::default(),
        )
        .unwrap();
        assert!(wait_for(&mut rx, |_| screen_text(&core).contains('a')));
        let shape = || core.snapshot().cursor.map(|c| c.shape);
        assert_eq!(shape(), Some(CursorShape::HollowBlock));
        assert!(wait_for(&mut rx, |_| shape() == Some(CursorShape::Beam)));
    }

    #[test]
    fn app_cursor_mode_is_visible_to_the_ui() {
        let (tx, mut rx) = unbounded();
        let core = TerminalCore::spawn(
            sh("printf '\\033[?1h'; sleep 1"),
            TermDims { cols: 40, rows: 5 },
            tx,
            Theme::default(),
        )
        .unwrap();
        assert!(wait_for(&mut rx, |e| matches!(e, TermEvent::Output { .. })));
        assert!(core.mode().contains(TermMode::APP_CURSOR));
    }

    #[test]
    fn notifications_are_counted() {
        let (tx, mut rx) = unbounded();
        let _core = TerminalCore::spawn(
            sh("printf '\\033]777;notify;fuxx;done\\007'; sleep 1"),
            TermDims { cols: 40, rows: 5 },
            tx,
            Theme::default(),
        )
        .unwrap();
        assert!(wait_for(&mut rx, |e| matches!(
            e,
            TermEvent::Output { notifications: 1 }
        )));
    }

    #[test]
    fn write_never_blocks_when_the_child_stops_reading() {
        let (tx, _rx) = unbounded();
        // Raw mode + not reading stdin: the tty's input queue fills and a blocking write would hang.
        let core = TerminalCore::spawn(
            sh("stty raw; sleep 5"),
            TermDims { cols: 40, rows: 5 },
            tx,
            Theme::default(),
        )
        .unwrap();
        std::thread::sleep(Duration::from_millis(300)); // let `stty raw` run
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            for _ in 0..64 {
                core.write(&[b'x'; 1024]);
            }
            let _ = done_tx.send(());
        });
        assert!(
            done_rx.recv_timeout(Duration::from_secs(2)).is_ok(),
            "write blocked"
        );
    }

    #[test]
    fn selection_text_word_and_input_clears_it() {
        let (tx, mut rx) = unbounded();
        let core = TerminalCore::spawn(
            sh("printf 'hello world'; sleep 2"),
            TermDims { cols: 20, rows: 3 },
            tx,
            Theme::default(),
        )
        .unwrap();
        assert!(wait_for(&mut rx, |_| screen_text(&core).contains("world")));
        core.start_selection(
            SelectionType::Simple,
            Point::new(Line(0), Column(0)),
            Side::Left,
        );
        core.extend_selection(Point::new(Line(0), Column(4)), Side::Right);
        assert_eq!(core.selection_text().as_deref(), Some("hello"));
        core.start_selection(
            SelectionType::Semantic,
            Point::new(Line(0), Column(7)),
            Side::Left,
        );
        assert_eq!(core.selection_text().as_deref(), Some("world"));
        core.input(b"x");
        assert_eq!(core.selection_text(), None);
    }

    #[test]
    fn out_of_range_points_are_clamped_not_panicking() {
        let (tx, mut rx) = unbounded();
        let core = TerminalCore::spawn(
            sh("printf 'hello world'; sleep 2"),
            TermDims { cols: 20, rows: 3 },
            tx,
            Theme::default(),
        )
        .unwrap();
        assert!(wait_for(&mut rx, |_| screen_text(&core).contains("world")));
        let far_up = Point::new(Line(-500), Column(999));
        let far_down = Point::new(Line(500), Column(0));
        // A point from a stale display offset (e.g. scrollback cleared in between) must not panic.
        assert_eq!(core.link_at(far_up), None);
        assert_eq!(core.link_at(far_down), None);
        core.start_selection(SelectionType::Simple, far_up, Side::Left);
        core.extend_selection(far_down, Side::Right);
        let _ = core.selection_text();
    }

    #[test]
    fn set_theme_changes_snapshot_colors() {
        let (tx, mut rx) = unbounded();
        let core = TerminalCore::spawn(
            sh("printf 'x'; sleep 1"),
            TermDims { cols: 10, rows: 2 },
            tx,
            Theme::default(),
        )
        .unwrap();
        assert!(wait_for(&mut rx, |_| screen_text(&core).contains('x')));
        let clone = core.clone();
        let mut t = Theme::default();
        t.foreground = crate::theme::parse_color("#123456").unwrap();
        clone.set_theme(t);
        assert_eq!(core.snapshot().lines[0][0].fg, t.foreground); // every clone sees it
        assert_eq!(core.theme(), t);
    }
}

/// The Daemon backend, against an in-process daemon (or a scripted fake one).
#[cfg(test)]
mod daemon_tests {
    use super::*;
    use crate::daemon_client::tests::{daemon, sh_create, temp_sock};
    use crate::mux_proto::{self, Decoder, Dims, Frame, encode};
    use futures::channel::mpsc::{UnboundedReceiver, unbounded};
    use std::io::{Read, Write};
    use std::os::unix::net::UnixListener;
    use std::path::Path;
    use std::time::Duration;

    fn eventually(mut cond: impl FnMut() -> bool) -> bool {
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            if cond() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        cond()
    }

    /// Every event that arrived in the next `wait`.
    fn drain(rx: &mut UnboundedReceiver<TermEvent>, wait: Duration) -> Vec<TermEvent> {
        let deadline = Instant::now() + wait;
        let mut events = Vec::new();
        while Instant::now() < deadline {
            match rx.try_recv() {
                Ok(e) => events.push(e),
                Err(_) => std::thread::sleep(Duration::from_millis(10)),
            }
        }
        events
    }

    /// Wait (10 s) for an event `pred` accepts.
    fn wait_event(
        rx: &mut UnboundedReceiver<TermEvent>,
        mut pred: impl FnMut(&TermEvent) -> bool,
    ) -> bool {
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            match rx.try_recv() {
                Ok(e) if pred(&e) => return true,
                Ok(_) => {}
                Err(_) => std::thread::sleep(Duration::from_millis(10)),
            }
        }
        false
    }

    fn screen_lines(core: &TerminalCore) -> Vec<String> {
        core.text(grid::TextScope::Screen)
            .lines()
            .map(|l| l.trim_end().to_string())
            .collect()
    }

    #[test]
    fn a_daemon_session_s_exit_code_reaches_the_replica() {
        // Not reaped: a session that ends before the replica attaches is still there to say so.
        let path = temp_sock("t-exitcode");
        let _d = crate::daemon::start_unreaped(&path).unwrap();
        let client = DaemonClient::connect(&path).unwrap();
        let (tx, mut rx) = unbounded();
        let program = ["/bin/sh", "-c", "sleep 0.3; exit 255"];
        let create = mux_proto::Create {
            program: Some(program.iter().map(|w| w.to_string()).collect()),
            ..sh_create(40, 5)
        };
        let _core = TerminalCore::create_daemon(client, create, tx, Theme::default()).unwrap();
        let mut code = None;
        assert!(wait_event(&mut rx, |e| match e {
            TermEvent::Exited { code: c } => {
                code = Some(*c);
                true
            }
            _ => false,
        }));
        assert_eq!(code, Some(Some(255)));
    }

    #[test]
    fn a_daemon_replica_has_the_marker_and_follows_a_programs_shape() {
        use alacritty_terminal::vte::ansi::CursorShape;
        let (_d, path) = daemon("t-marker");
        let client = DaemonClient::connect(&path).unwrap();
        let (tx, _rx) = unbounded();
        let core =
            TerminalCore::create_daemon(client, sh_create(40, 5), tx, Theme::default()).unwrap();
        let shape = || core.snapshot().cursor.map(|c| c.shape);
        core.input(b"echo READY\r");
        assert!(eventually(|| screen_lines(&core)
            .iter()
            .any(|l| l == "READY")));
        assert_eq!(shape(), Some(CursorShape::HollowBlock));
        // A steady block (`ESC [ 2 q`) is a program's choice, not the marker.
        core.input(b"printf '\\033[2 q'\r");
        assert!(eventually(|| shape() == Some(CursorShape::Block)));
        core.input(b"printf '\\033[0 q'\r");
        assert!(eventually(|| shape() == Some(CursorShape::HollowBlock)));
    }

    #[test]
    fn typed_echo_shows_on_the_replica() {
        let (_d, path) = daemon("t-echo");
        let client = DaemonClient::connect(&path).unwrap();
        let (tx, _rx) = unbounded();
        let core =
            TerminalCore::create_daemon(client, sh_create(40, 5), tx, Theme::default()).unwrap();
        assert!(core.daemon_id().is_some());
        core.input(b"echo hi\r");
        assert!(
            eventually(|| screen_lines(&core).iter().any(|l| l == "hi")),
            "{:?}",
            screen_lines(&core)
        );
    }

    #[test]
    fn attaching_to_a_busy_session_matches_the_daemon() {
        let (_d, path) = daemon("t-busy");
        let client = DaemonClient::connect(&path).unwrap();
        let id = client.create(sh_create(50, 8)).unwrap();
        // DO''NE: the echoed command line doesn't match a line that is just DONE.
        client.input(
            id,
            b"i=0; while [ $i -lt 20000 ]; do i=$((i+1)); echo line$i; done; echo DO''NE\r",
        );
        assert!(eventually(|| client
            .read_text(id, "screen")
            .unwrap()
            .contains("line")));
        let (tx, _rx) = unbounded();
        let core = TerminalCore::attach_daemon(client.clone(), id, tx, Theme::default()).unwrap();
        let busy = client.read_text(id, "screen").unwrap();
        assert!(!busy.lines().any(|l| l == "DONE"), "attached too late");
        assert!(eventually(|| screen_lines(&core)
            .iter()
            .any(|l| l == "DONE")));
        assert!(
            eventually(
                || core.text(grid::TextScope::All) == client.read_text(id, "all").unwrap()
                    && core.text(grid::TextScope::Screen)
                        == client.read_text(id, "screen").unwrap()
            ),
            "replica:\n{}\ndaemon:\n{}",
            core.text(grid::TextScope::Screen),
            client.read_text(id, "screen").unwrap()
        );
    }

    #[test]
    fn an_adopted_session_is_drawn_only_once_attached() {
        let (_d, path) = daemon("t-adopt");
        let client = DaemonClient::connect(&path).unwrap();
        let id = client.create(sh_create(40, 5)).unwrap();
        // MA''RK: the echoed command line doesn't match a line that is just MARK.
        client.input(id, b"echo MA''RK\r");
        assert!(eventually(|| client
            .read_text(id, "screen")
            .unwrap()
            .lines()
            .any(|l| l.trim_end() == "MARK")));
        let (tx, _rx) = unbounded();
        let core = TerminalCore::adopt_daemon(client.clone(), id, tx, Theme::default()).unwrap();
        assert_eq!(core.daemon_id(), Some(id));
        std::thread::sleep(Duration::from_millis(300));
        assert!(!core.is_drawn(), "nothing to read yet");
        assert!(
            !screen_lines(&core).iter().any(|l| l == "MARK"),
            "no snapshot before attach"
        );
        assert!(core.attach(), "the first attach asks");
        assert!(!core.attach(), "later ones don't");
        assert!(eventually(|| core.is_drawn()));
        assert!(screen_lines(&core).iter().any(|l| l == "MARK"));
        // One attached from the start never asks again.
        let (tx, _rx) = unbounded();
        let attached = TerminalCore::attach_daemon(client, id, tx, Theme::default()).unwrap();
        assert!(!attached.attach());
    }

    #[test]
    fn metadata_set_through_the_core_reaches_the_daemon_in_order() {
        use crate::mux_proto::{Progress, SetMeta};
        let (_d, path) = daemon("t-meta");
        let client = DaemonClient::connect(&path).unwrap();
        let (tx, _rx) = unbounded();
        let core =
            TerminalCore::create_daemon(client.clone(), sh_create(40, 5), tx, Theme::default())
                .unwrap();
        let id = core.daemon_id().unwrap();
        let meta = |n: u8| SetMeta {
            id,
            name: Some(Some(format!("n{n}"))),
            project: None,
            preset: None,
            note: Some(None),
            progress: Some(Some(Progress {
                percent: n,
                label: None,
            })),
        };
        for n in 1..=50 {
            core.set_meta(meta(n));
        }
        assert!(eventually(|| {
            let info = client.list().unwrap().remove(0);
            info.name == "n50" && info.progress.map(|p| p.percent) == Some(50)
        }));
        // Once the pump's queue is empty, a List (after it on the one connection) sees the
        // last one: what quitting waits for.
        let queue = core.pump_queue().unwrap();
        for round in 0..20u8 {
            for n in 0..200u8 {
                core.set_meta(SetMeta {
                    name: Some(Some(format!("r{round}-{n}"))),
                    ..meta(n % 100)
                });
            }
            assert!(eventually(|| queue.is_empty()));
            assert_eq!(client.list().unwrap()[0].name, format!("r{round}-199"));
        }
    }

    #[test]
    fn resize_round_trips_through_the_daemon() {
        let (_d, path) = daemon("t-resize");
        let client = DaemonClient::connect(&path).unwrap();
        let (tx, _rx) = unbounded();
        let core =
            TerminalCore::create_daemon(client.clone(), sh_create(40, 5), tx, Theme::default())
                .unwrap();
        let id = core.daemon_id().unwrap();
        assert!(eventually(|| core.snapshot().cols == 40));
        core.resize(
            TermDims { cols: 60, rows: 10 },
            CellPx {
                width: 8,
                height: 16,
            },
        );
        assert!(eventually(|| {
            let snap = core.snapshot();
            (snap.cols, snap.rows) == (60, 10)
        }));
        let daemon_dims = client.list().unwrap()[0].dims;
        assert_eq!(daemon_dims, Dims { cols: 60, rows: 10 });
        assert_eq!(core.dims(), TermDims { cols: 60, rows: 10 });
        // The replica reflowed at the same point as the daemon.
        core.input(b"echo after-resize\r");
        assert!(eventually(|| core.text(grid::TextScope::All)
            == client.read_text(id, "all").unwrap()
            && core.text(grid::TextScope::Screen).contains("after-resize")));
    }

    #[test]
    fn status_title_and_exit_come_from_the_daemon_and_queries_are_not_answered_here() {
        let (_d, path) = daemon("t-events");
        let client = DaemonClient::connect(&path).unwrap();
        let (tx, mut rx) = unbounded();
        let core =
            TerminalCore::create_daemon(client, sh_create(40, 5), tx, Theme::default()).unwrap();
        core.input(
            b"printf '\\033]2;mytitle\\007\\033[6n\\033]11;?\\007\\033[14t'; sleep 1; echo x\r",
        );
        let mut seen = Vec::new();
        assert!(wait_event(&mut rx, |e| {
            let done = matches!(e, TermEvent::Status(Status::Working));
            seen.push(format!("{e:?}"));
            done
        }));
        let replica_queries = |e: &String| {
            e.contains("PtyWrite") || e.contains("ColorRequest") || e.contains("TextAreaSize")
        };
        assert!(
            seen.iter().any(|e| e.contains("Title(mytitle)")),
            "{seen:?}"
        );
        assert!(!seen.iter().any(replica_queries), "{seen:?}");
        // The daemon's answers to the queries were typed ahead into the shell: clear them.
        std::thread::sleep(Duration::from_millis(1500));
        core.input(b"\x03");
        std::thread::sleep(Duration::from_millis(300));
        core.input(b"exit\r");
        let rest: Vec<String> = drain(&mut rx, Duration::from_secs(2))
            .iter()
            .map(|e| format!("{e:?}"))
            .collect();
        assert!(rest.iter().any(|e| e.starts_with("Exited")), "{rest:?}");
        assert!(!rest.iter().any(replica_queries), "{rest:?}");
    }

    #[test]
    fn a_mouse_report_is_not_typing() {
        let (_d, path) = daemon("t-mouse");
        let client = DaemonClient::connect(&path).unwrap();
        let (tx, mut rx) = unbounded();
        let core =
            TerminalCore::create_daemon(client, sh_create(40, 5), tx, Theme::default()).unwrap();
        // `write` is how the view sends mouse reports and alternate-scroll arrows.
        core.write(b"sleep 0.3; echo MOUSE\r");
        assert!(eventually(|| screen_lines(&core)
            .iter()
            .any(|l| l == "MOUSE")));
        let seen: Vec<String> = drain(&mut rx, Duration::from_millis(800))
            .iter()
            .map(|e| format!("{e:?}"))
            .collect();
        assert!(!seen.iter().any(|e| e == "Status(Working)"), "{seen:?}");
        core.input(b"sleep 0.3; echo KEY\r");
        assert!(wait_event(&mut rx, |e| matches!(
            e,
            TermEvent::Status(Status::Working)
        )));
    }

    #[test]
    fn the_ui_never_waits_on_a_daemon_that_stops_reading() {
        let path = temp_sock("t-noread");
        let listener = UnixListener::bind(&path).unwrap();
        // Answers Hello, then never reads again (and lets go after a while).
        let fake = std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let mut decoder = Decoder::new();
            let mut buf = [0u8; 4096];
            while decoder.next().is_none() {
                let n = s.read(&mut buf).unwrap();
                decoder.push(&buf[..n]);
            }
            let ok = Frame::HelloOk(mux_proto::HelloOk {
                proto: 1,
                daemon_version: "test".into(),
                pid: 1,
                instance: None,
            });
            s.write_all(&encode(&ok)).unwrap();
            std::thread::sleep(Duration::from_secs(4));
        });
        let client = DaemonClient::connect(&path).unwrap();
        let (tx, _rx) = unbounded();
        let core = TerminalCore::attach_daemon(client, 7, tx, Theme::default()).unwrap();
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            for i in 0..64 {
                core.write(&[b'x'; 1 << 20]);
                core.input(b"y");
                core.resize(
                    TermDims {
                        cols: 50 + i,
                        rows: 20,
                    },
                    CellPx::default(),
                );
                core.set_theme(Theme::default());
            }
            drop(core);
            let _ = done_tx.send(());
        });
        assert!(
            done_rx.recv_timeout(Duration::from_secs(2)).is_ok(),
            "the UI side blocked"
        );
        fake.join().unwrap();
    }

    #[test]
    fn the_first_resize_after_attaching_is_always_sent() {
        let (_d, path) = daemon("t-firstresize");
        let client = DaemonClient::connect(&path).unwrap();
        let id = client.create(sh_create(100, 30)).unwrap();
        let (tx, _rx) = unbounded();
        let core = TerminalCore::attach_daemon(client.clone(), id, tx, Theme::default()).unwrap();
        // Whatever the replica started as, the pane's size goes to the daemon.
        core.resize(TermDims { cols: 80, rows: 24 }, CellPx::default());
        assert!(eventually(
            || client.list().unwrap()[0].dims == Dims { cols: 80, rows: 24 }
        ));
        assert!(eventually(|| {
            let snap = core.snapshot();
            (snap.cols, snap.rows) == (80, 24)
        }));
    }

    #[test]
    fn dropping_the_view_side_detaches_without_ending_the_session() {
        let (_d, path) = daemon("t-detach");
        let client = DaemonClient::connect(&path).unwrap();
        let (tx, _rx) = unbounded();
        let core =
            TerminalCore::create_daemon(client.clone(), sh_create(40, 5), tx, Theme::default())
                .unwrap();
        let id = core.daemon_id().unwrap();
        let clone = core.clone();
        drop(core);
        drop(clone);
        std::thread::sleep(Duration::from_millis(200));
        let sessions = client.list().unwrap();
        assert!(sessions.iter().any(|s| s.id == id && s.alive));
        // And it can be attached again, with what it showed.
        client.input(id, b"echo still-here\r");
        let (tx, _rx) = unbounded();
        let again = TerminalCore::attach_daemon(client, id, tx, Theme::default()).unwrap();
        assert!(eventually(|| screen_lines(&again)
            .iter()
            .any(|l| l == "still-here")));
    }

    /// A scripted daemon for one connection: answers Hello and List, and for the nth Attach
    /// sends `script[n]`. Returns how many Attach frames it saw.
    fn scripted_daemon(path: &Path, script: Vec<Vec<Frame>>) -> std::thread::JoinHandle<usize> {
        scripted_daemon_with_input(path, script, Vec::new())
    }

    /// `scripted_daemon` that also sends `on_input[n]` for the nth Input frame.
    fn scripted_daemon_with_input(
        path: &Path,
        script: Vec<Vec<Frame>>,
        on_input: Vec<Vec<Frame>>,
    ) -> std::thread::JoinHandle<usize> {
        let listener = UnixListener::bind(path).unwrap();
        std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
            let mut decoder = Decoder::new();
            let mut buf = [0u8; 4096];
            let mut attaches = 0;
            let mut inputs = 0;
            loop {
                let n = match s.read(&mut buf) {
                    Ok(0) | Err(_) => return attaches,
                    Ok(n) => n,
                };
                decoder.push(&buf[..n]);
                while let Some(frame) = decoder.next() {
                    let reply = match frame.unwrap() {
                        Frame::Hello(_) => vec![Frame::HelloOk(mux_proto::HelloOk {
                            proto: 1,
                            daemon_version: "test".into(),
                            pid: 1,
                            instance: None,
                        })],
                        Frame::List => vec![Frame::Sessions(vec![mux_proto::SessionInfo {
                            id: 7,
                            name: String::new(),
                            project: None,
                            preset: None,
                            title: None,
                            status: "waiting".into(),
                            note: None,
                            progress: None,
                            dims: Dims { cols: 20, rows: 3 },
                            alive: true,
                            created_at: 0,
                        }])],
                        Frame::Attach { id: 7 } => {
                            attaches += 1;
                            script.get(attaches - 1).cloned().unwrap_or_default()
                        }
                        Frame::Input { id: 7, .. } => {
                            inputs += 1;
                            on_input.get(inputs - 1).cloned().unwrap_or_default()
                        }
                        _ => vec![],
                    };
                    for f in reply {
                        s.write_all(&encode(&f)).unwrap();
                    }
                }
            }
        })
    }

    fn snap(seq: u64, text: &str, title: Option<&str>) -> Frame {
        Frame::Snapshot(mux_proto::Snapshot {
            id: 7,
            seq,
            dims: Dims { cols: 20, rows: 3 },
            title: title.map(String::from),
            vt_bytes: text.as_bytes().to_vec(),
        })
    }

    fn out(seq: u64, text: &str) -> Frame {
        Frame::Output {
            id: 7,
            seq,
            notifications: 0,
            bytes: text.as_bytes().to_vec(),
        }
    }

    #[test]
    fn a_seq_gap_or_resync_reattaches_onto_a_fresh_replica() {
        let path = temp_sock("t-gap");
        let fake = scripted_daemon(
            &path,
            vec![
                // Starts a synchronized update that never ends, then skips seq 2.
                vec![
                    snap(0, "one", Some("T1")),
                    out(1, " two\x1b[?2026hhidden"),
                    out(3, "BAD"),
                ],
                // A fresh Term and parser: no "one two", and not stuck in the sync update.
                vec![
                    snap(10, "fresh", None),
                    out(11, "!"),
                    Frame::Resync { id: 7 },
                ],
                vec![
                    snap(20, "third", None),
                    Frame::Resized(mux_proto::Resized {
                        id: 7,
                        seq: 21,
                        cols: 30,
                        rows: 4,
                    }),
                    out(22, "?"),
                    Frame::Title {
                        id: 7,
                        title: Some("T2".into()),
                    },
                    Frame::Status {
                        id: 7,
                        status: "done".into(),
                    },
                    Frame::Exited { id: 7 },
                ],
            ],
        );
        let client = DaemonClient::connect(&path).unwrap();
        let (tx, mut rx) = unbounded();
        let core = TerminalCore::attach_daemon(client, 7, tx, Theme::default()).unwrap();
        let mut seen = Vec::new();
        assert!(wait_event(&mut rx, |e| {
            seen.push(format!("{e:?}"));
            matches!(e, TermEvent::Exited { .. })
        }));
        assert_eq!(screen_lines(&core)[0], "third?");
        let snap = core.snapshot();
        assert_eq!((snap.cols, snap.rows), (30, 4));
        let wanted = [
            "Term(Title(T1))",
            "Status(Waiting)",
            "Term(ResetTitle)",
            "Term(Title(T2))",
            "Status(Done)",
            "Exited { code: None }",
        ];
        let got: Vec<&String> = seen
            .iter()
            .filter(|e| wanted.contains(&e.as_str()))
            .collect();
        assert_eq!(got, wanted, "{seen:?}");
        drop(core);
        assert_eq!(fake.join().unwrap(), 3);
    }

    /// A screen as the background service would send it: `bytes` fed to a 20 x 3 Term
    /// through `blocks::advance`, then encoded.
    fn tagged_snap(seq: u64, bytes: &[u8]) -> Frame {
        let mut term = Term::new(
            term_config(),
            &TermDims { cols: 20, rows: 3 },
            alacritty_terminal::event::VoidListener,
        );
        blocks::advance(
            &mut Processor::new(),
            &mut term,
            &mut Tagger::new(),
            &mut MarkScanner::new(),
            bytes,
        );
        Frame::Snapshot(mux_proto::Snapshot {
            id: 7,
            seq,
            dims: Dims { cols: 20, rows: 3 },
            title: None,
            vt_bytes: crate::vt_snapshot::encode(&mut term),
        })
    }

    const A: &str = "\x1b]133;A\x07";
    const B: &str = "\x1b]133;B\x07";

    #[test]
    fn a_reattached_replica_is_not_prompt_ready() {
        let path = temp_sock("t-ready");
        // The session sits at a prompt after one command.
        let at_prompt = |cmd: &str| {
            format!(
                "{A}$ {B}{cmd}\r\n\x1b]133;C;cmdline_url={cmd}\x07ok\r\n\x1b]133;D;0\x07{A}$ {B}"
            )
        };
        let new_prompt = format!("\r\n{A}$ {B}");
        let fake = scripted_daemon_with_input(
            &path,
            vec![
                // A new prompt (the replica's tagger knows it), then a gap: re-attach.
                vec![
                    tagged_snap(0, at_prompt("ls").as_bytes()),
                    out(1, &format!("\r\nseen{new_prompt}")),
                    out(3, "gap"),
                ],
                // The re-attach's snapshot waits for the first input; the third attach (after
                // the `Resync`) gets nothing.
                vec![],
            ],
            vec![
                vec![tagged_snap(10, at_prompt("pwd").as_bytes())],
                // Typing: the shell draws a new prompt.
                vec![out(11, &new_prompt)],
                vec![Frame::Resync { id: 7 }],
            ],
        );
        let client = DaemonClient::connect(&path).unwrap();
        let (tx, _rx) = unbounded();
        let core = TerminalCore::attach_daemon(client, 7, tx, Theme::default()).unwrap();
        let cmds = || -> Vec<Option<String>> { core.blocks().into_iter().map(|b| b.cmd).collect() };
        // The prompt before the gap was known; from the gap on it is not, snapshot or not.
        assert!(eventually(|| core
            .text(grid::TextScope::All)
            .contains("seen")));
        assert!(
            eventually(|| !core.prompt_ready()),
            "still ready after the gap"
        );
        assert!(!core.cursor_at_input_start());
        core.input(b"x");
        assert!(
            eventually(|| cmds() == [Some("pwd".to_string())]),
            "{:?}",
            cmds()
        );
        // The snapshot carries the block and the prompt's tag...
        assert!(core.live_prompt_from_top().is_some());
        // ...but not where typing starts at that prompt: unknown until the next one.
        assert!(!core.prompt_ready());
        assert!(!core.cursor_at_input_start());
        core.input(b"\r");
        assert!(eventually(|| core.prompt_ready()));
        assert!(core.cursor_at_input_start());
        // A `Resync` forgets it too.
        core.input(b"y");
        assert!(
            eventually(|| !core.prompt_ready()),
            "still ready after Resync"
        );
        drop(core);
        assert_eq!(fake.join().unwrap(), 3);
    }

    #[test]
    fn a_reattached_replica_does_not_rerun_older_blocks() {
        let path = temp_sock("t-rerun");
        let block = |cmd: &str| {
            format!("{A}$ {B}{cmd}\r\n\x1b]133;C;cmdline_url={cmd}\x07ok\r\n\x1b]133;D;0\x07")
        };
        let fake = scripted_daemon_with_input(
            &path,
            vec![
                // The service's screen: `ls` (its tag). Then `pwd` runs while the window
                // watches (the window's own tag).
                vec![
                    tagged_snap(0, block("ls").as_bytes()),
                    out(1, &format!("{}{A}$ {B}", block("pwd"))),
                ],
                // The re-attach's snapshot: both blocks, as the service tagged them, at a
                // prompt.
                vec![tagged_snap(
                    10,
                    format!("{}{}{A}$ {B}", block("ls"), block("pwd")).as_bytes(),
                )],
            ],
            // The first input: a gap, so the window re-attaches.
            vec![vec![out(3, "gap")]],
        );
        let client = DaemonClient::connect(&path).unwrap();
        let (tx, _rx) = unbounded();
        let core = TerminalCore::attach_daemon(client, 7, tx, Theme::default()).unwrap();
        let cmds = || -> Vec<Option<String>> { core.blocks().into_iter().map(|b| b.cmd).collect() };
        let both = vec![Some("ls".to_string()), Some("pwd".to_string())];
        assert!(eventually(|| cmds() == both), "{:?}", cmds());
        // `ls` came in the snapshot (no rerun); `pwd` was tagged here (rerun).
        let blocks = core.blocks();
        assert!(!core.rerun_facts(&blocks[0].id).ours, "the snapshot's ls");
        assert!(core.rerun_facts(&blocks[1].id).ours, "this run's pwd");
        assert!(core.prompt_ready());
        // A gap: re-attached, both blocks come back from the new snapshot, neither ours.
        core.input(b"x");
        assert!(eventually(|| core.snapshot().clears >= 2 && cmds() == both));
        for b in core.blocks() {
            assert!(!core.rerun_facts(&b.id).ours, "{b:?}");
        }
        drop(core);
        assert_eq!(fake.join().unwrap(), 2);
    }

    #[test]
    fn the_service_and_replica_tag_alike() {
        let (_d, path) = daemon("t-alike");
        let client = DaemonClient::connect(&path).unwrap();
        // Prints once the replica has its first snapshot (the file appears): it tags the
        // output itself.
        let go = temp_sock("t-alike-go").with_file_name("go");
        let script = r#"while [ ! -e "$GO" ]; do sleep 0.02; done; printf '\033]133;A\007dir\n$ \033]133;B\007ls\n\033]133;C;cmdline_url=ls\007a\nb\n\033]133;D;0\007\033]133;A\007$ \033]133;B\007false\n\033]133;C;cmdline_url=false\007\033]133;D;2\007\033]133;A\007$ \033]133;B\007'; sleep 20"#;
        let mut create = mux_proto::Create {
            program: Some(vec!["/bin/sh".into(), "-c".into(), script.into()]),
            ..sh_create(40, 8)
        };
        create.env.insert("GO".into(), go.display().to_string());
        let (tx, _rx) = unbounded();
        let replica =
            TerminalCore::create_daemon(client.clone(), create, tx, Theme::default()).unwrap();
        assert!(eventually(|| replica.is_drawn()));
        std::fs::write(&go, b"").unwrap();
        assert!(eventually(
            || replica.prompt_ready() && replica.blocks().len() == 2
        ));
        // The service's tags, as its snapshot brings them to a window that attaches now.
        let id = replica.daemon_id().unwrap();
        let (tx, _rx) = unbounded();
        let service = TerminalCore::attach_daemon(client, id, tx, Theme::default()).unwrap();
        assert!(eventually(
            || service.is_drawn() && service.blocks().len() == 2
        ));
        let (mine, theirs) = (replica.blocks(), service.blocks());
        // Each tagged on its own (ids differ), alike in everything else.
        assert!(mine.iter().zip(&theirs).all(|(m, t)| m.id != t.id));
        let unnamed = |blocks: Vec<Block>| -> Vec<Block> {
            blocks
                .into_iter()
                .map(|b| Block {
                    id: String::new(),
                    ..b
                })
                .collect()
        };
        assert_eq!(unnamed(mine.clone()), unnamed(theirs));
        assert_eq!(mine[0].outcome, blocks::Outcome::Ok);
        assert_eq!(mine[0].cmd.as_deref(), Some("ls"));
        assert_eq!(mine[0].prompt_top + 1, mine[0].cmd_row, "a two-line prompt");
        assert_eq!(mine[1].outcome, blocks::Outcome::Failed(2));
        assert_eq!(replica.block_output(&mine[0].id).as_deref(), Some("a\nb"));
        // The window's fresh tagger: the next prompt is not known yet.
        assert!(!service.prompt_ready());
    }

    #[test]
    fn the_replica_counts_clears_and_snapshots() {
        let path = temp_sock("t-clears");
        let fake = scripted_daemon(
            &path,
            vec![vec![
                snap(0, "a", None),
                out(1, "\x1b[2J"),
                out(2, "x\x1b["),
                out(3, "3Jdone"),
            ]],
        );
        let client = DaemonClient::connect(&path).unwrap();
        let (tx, _rx) = unbounded();
        let core = TerminalCore::attach_daemon(client, 7, tx, Theme::default()).unwrap();
        assert!(eventually(|| core
            .text(grid::TextScope::All)
            .contains("done")));
        // The snapshot (a fresh screen) and both clears.
        assert_eq!(core.snapshot().clears, 3);
        drop(core);
        fake.join().unwrap();
    }

    #[test]
    fn the_replica_drops_frames_until_the_new_snapshot() {
        let path = temp_sock("t-stale");
        let fake = scripted_daemon(
            &path,
            vec![
                vec![
                    snap(0, "a", None),
                    out(2, "GAP"),
                    out(3, "STALE"),
                    // Old news: the new snapshot and `List` say otherwise.
                    Frame::Title {
                        id: 7,
                        title: Some("OLD".into()),
                    },
                    Frame::Status {
                        id: 7,
                        status: "working".into(),
                    },
                ],
                vec![snap(5, "b", None), out(6, "c")],
            ],
        );
        let client = DaemonClient::connect(&path).unwrap();
        let (tx, mut rx) = unbounded();
        let core = TerminalCore::attach_daemon(client, 7, tx, Theme::default()).unwrap();
        assert!(eventually(|| screen_lines(&core)
            .first()
            .is_some_and(|l| l == "bc")));
        assert!(!core.text(grid::TextScope::All).contains("STALE"));
        let seen: Vec<String> = drain(&mut rx, Duration::from_millis(300))
            .iter()
            .map(|e| format!("{e:?}"))
            .collect();
        assert!(seen.contains(&"Status(Waiting)".to_string()), "{seen:?}");
        assert!(
            !seen
                .iter()
                .any(|e| e.contains("OLD") || e == "Status(Working)"),
            "{seen:?}"
        );
        drop(core);
        assert_eq!(fake.join().unwrap(), 2);
    }

    #[test]
    fn losing_the_daemon_is_not_an_exit() {
        let path = temp_sock("t-lost");
        let listener = UnixListener::bind(&path).unwrap();
        // Answers Hello and the first Attach (a snapshot), then hangs up: the daemon is gone.
        let fake = std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let mut decoder = Decoder::new();
            let mut buf = [0u8; 4096];
            loop {
                let n = s.read(&mut buf).unwrap();
                decoder.push(&buf[..n]);
                while let Some(frame) = decoder.next() {
                    match frame.unwrap() {
                        Frame::Hello(_) => {
                            let ok = Frame::HelloOk(mux_proto::HelloOk {
                                proto: 1,
                                daemon_version: "test".into(),
                                pid: 1,
                                instance: None,
                            });
                            s.write_all(&encode(&ok)).unwrap();
                        }
                        Frame::Attach { id: 7 } => {
                            s.write_all(&encode(&snap(0, "still here", None))).unwrap();
                            return;
                        }
                        _ => {}
                    }
                }
            }
        });
        let client = DaemonClient::connect(&path).unwrap();
        let (tx, mut rx) = unbounded();
        let core = TerminalCore::attach_daemon(client, 7, tx, Theme::default()).unwrap();
        fake.join().unwrap();
        let mut seen = Vec::new();
        assert!(
            wait_event(&mut rx, |e| {
                seen.push(format!("{e:?}"));
                matches!(e, TermEvent::DaemonLost)
            }),
            "{seen:?}"
        );
        assert!(!seen.iter().any(|e| e.starts_with("Exited")), "{seen:?}");
        // The screen it had stays.
        assert_eq!(screen_lines(&core)[0], "still here");
    }

    #[test]
    fn a_core_knows_which_connection_it_uses() {
        let (_d, path) = daemon("t-client");
        let client = DaemonClient::connect(&path).unwrap();
        let other = DaemonClient::connect(&path).unwrap();
        let id = client.create(sh_create(40, 5)).unwrap();
        let (tx, _rx) = unbounded();
        let core = TerminalCore::attach_daemon(client.clone(), id, tx, Theme::default()).unwrap();
        assert!(core.uses_client(&client));
        assert!(!core.uses_client(&other));
    }

    #[test]
    fn closing_through_the_core_ends_the_daemon_session() {
        let (_d, path) = daemon("t-close");
        let client = DaemonClient::connect(&path).unwrap();
        let (tx, mut rx) = unbounded();
        let core =
            TerminalCore::create_daemon(client.clone(), sh_create(40, 5), tx, Theme::default())
                .unwrap();
        let id = core.daemon_id().unwrap();
        core.close();
        assert!(wait_event(&mut rx, |e| matches!(
            e,
            TermEvent::Exited { .. }
        )));
        assert!(eventually(|| !client
            .list()
            .unwrap()
            .iter()
            .any(|s| s.id == id)));
    }
}
