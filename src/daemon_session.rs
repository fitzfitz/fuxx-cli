//! M10a: one terminal session owned by the background daemon (study §2.3.2–§2.3.4). No GPUI.
//!
//! A session owns the child's PTY, the authoritative `Term` and its parser, the notification
//! detector and the status tracker. Everything that changes the Term happens under one lock,
//! together with numbering it and queueing it to subscribers (the ordering invariant, §2.3.3):
//! - the reader thread: `Term.advance`, `seq += 1`, queue `Output{seq}`;
//! - `resize`: `Term.resize`, `seq += 1`, queue `Resized{seq}`, then resize the PTY;
//! - `attach`: encode a snapshot at the current `seq` and register the subscriber.
//!
//! So a snapshot at seq N plus the frames from N+1 rebuild the Term with no gap or overlap.
//! `Status`, `Title`, `Bell` and `Exited` go into the same queues, unnumbered.
//!
//! Query replies (DA, DSR/CPR, OSC 4/10/11/12 colours, `CSI 14 t`) are answered here, with no
//! client needed: colours from the last `set_theme` (the default theme until then), the text
//! area size from the last cell px, with `terminal::query_reply`, the rule the GUI's Local
//! backend uses too (as it uses `pty::spawn_reader` / `spawn_writer`).

use std::collections::{BTreeMap, VecDeque};
use std::ffi::OsString;
use std::io;
use std::path::PathBuf;
use std::sync::mpsc::{self, RecvError, RecvTimeoutError, Sender, TryRecvError};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use alacritty_terminal::event::{Event, EventListener};
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::{Config, Term};
use alacritty_terminal::vte::ansi::{Processor, StdSyncHandler};
use portable_pty::CommandBuilder;

use crate::colors;
use crate::detector::OscDetector;
use crate::grid::{self, TextScope};
use crate::mux_proto::{self, Dims, Frame, MAX_FRAME, SessionInfo, SetMeta, Snapshot};
use crate::pty::{self, Pty};
use crate::status::{Status, StatusTracker, TAIL_LINES};
use crate::terminal::{CellPx, TermDims, query_reply, window_size};
use crate::theme::Theme;
use crate::vt_snapshot;

/// Frames a subscriber may have queued before it is cut off with `Resync`.
#[allow(dead_code)] // used by the daemon from M10a Task 4
pub const QUEUE_CAPACITY: usize = 1024;
/// Room left in a `Snapshot` frame for its binary header and JSON metadata (title excluded).
const SNAPSHOT_HEADROOM: usize = 64 * 1024;
/// Largest terminal a session takes, so even a snapshot with no history fits in a frame.
pub const MAX_COLS: usize = 1000;
pub const MAX_ROWS: usize = 500;
/// Longest unfinished escape sequence carried into a snapshot.
const TAIL_LIMIT: usize = 4096;

/// `dims` within `MAX_COLS` x `MAX_ROWS`.
fn clamp(dims: TermDims) -> TermDims {
    TermDims {
        cols: dims.cols.min(MAX_COLS),
        rows: dims.rows.min(MAX_ROWS),
    }
}

/// Where the output stream is, as the parser (vte) sees it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Seq {
    #[default]
    Ground,
    /// After ESC.
    Escape,
    /// After ESC and an intermediate byte (`ESC ( B`).
    EscIntermediate,
    /// After `ESC [`.
    Csi,
    /// After `ESC ]`, until BEL or ESC.
    Osc,
    /// DCS, SOS, PM or APC: until ESC.
    Str,
}

/// The raw bytes since the output stream was last between sequences: an unfinished escape
/// sequence or UTF-8 character. The parser has taken them in but not acted on them yet, so a
/// snapshot carries them at its end and a new replica's parser ends up in the same state.
///
/// Control characters inside ESC/CSI sequences are left out: the parser runs them straight
/// away, so they are already in the snapshot. An unfinished sequence longer than `TAIL_LIMIT`
/// is dropped and the scanner starts over (a replica attached during it shows the rest as text).
#[derive(Default)]
struct SequenceTail {
    seq: Seq,
    /// Continuation bytes still expected for a UTF-8 character.
    utf8_left: u8,
    tail: Vec<u8>,
}

impl SequenceTail {
    fn feed(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.byte(b);
        }
    }

    fn bytes(&self) -> &[u8] {
        &self.tail
    }

    fn byte(&mut self, b: u8) {
        match b {
            // CAN and SUB abort any sequence.
            0x18 | 0x1a => return self.reset(),
            // ESC starts a new one (and ends an OSC or DCS).
            0x1b => {
                self.reset();
                return self.enter(Seq::Escape, b);
            }
            _ => {}
        }
        match self.seq {
            Seq::Ground => self.utf8(b),
            // Executed (C0) or ignored (DEL) on the spot: not replayed.
            Seq::Escape | Seq::EscIntermediate | Seq::Csi if b < 0x20 || b == 0x7f => {}
            Seq::Escape => match b {
                b'[' => self.enter(Seq::Csi, b),
                b']' => self.enter(Seq::Osc, b),
                b'P' | b'X' | b'^' | b'_' => self.enter(Seq::Str, b),
                0x20..=0x2f => self.enter(Seq::EscIntermediate, b),
                0x30..=0x7e => self.reset(), // dispatched
                _ => self.tail.push(b),
            },
            Seq::EscIntermediate => match b {
                0x30..=0x7e => self.reset(),
                _ => self.tail.push(b),
            },
            Seq::Csi => match b {
                0x40..=0x7e => self.reset(), // the final byte
                _ => self.tail.push(b),
            },
            Seq::Osc if b == 0x07 => self.reset(),
            Seq::Osc | Seq::Str => self.tail.push(b),
        }
        if self.tail.len() > TAIL_LIMIT {
            self.reset();
        }
    }

    fn utf8(&mut self, b: u8) {
        if self.utf8_left > 0 {
            if (0x80..=0xbf).contains(&b) {
                self.tail.push(b);
                self.utf8_left -= 1;
                if self.utf8_left == 0 {
                    self.tail.clear();
                }
                return;
            }
            self.reset(); // broken: the parser already gave up on it; `b` starts afresh
        }
        self.utf8_left = match b {
            0xc2..=0xdf => 1,
            0xe0..=0xef => 2,
            0xf0..=0xf4 => 3,
            _ => return,
        };
        self.tail.push(b);
    }

    fn enter(&mut self, seq: Seq, b: u8) {
        self.seq = seq;
        self.tail.push(b);
    }

    fn reset(&mut self) {
        self.seq = Seq::Ground;
        self.utf8_left = 0;
        self.tail.clear();
    }
}

/// What a session is started with (`mux_proto::Create`, in the daemon's types).
#[allow(dead_code)] // used by the daemon from M10a Task 4
#[derive(Debug, Clone)]
pub struct CreateSpec {
    /// The program and its arguments; `None` runs the login shell from `env`'s `SHELL`.
    pub program: Option<Vec<String>>,
    /// Starting folder; `None` = `env`'s `HOME`.
    pub cwd: Option<PathBuf>,
    /// Typed into the child right after it starts, with Enter.
    pub startup: Option<String>,
    /// The child's whole environment (the GUI's, §2.3.7): the daemon's own is never used.
    pub env: BTreeMap<String, String>,
    pub dims: TermDims,
    pub cell: CellPx,
    pub name: Option<String>,
    pub project: Option<String>,
    pub preset: Option<String>,
}

impl From<mux_proto::Create> for CreateSpec {
    fn from(c: mux_proto::Create) -> Self {
        CreateSpec {
            program: None,
            cwd: c.cwd.map(PathBuf::from),
            startup: c.startup,
            env: c.env,
            dims: TermDims {
                cols: c.dims.cols.into(),
                rows: c.dims.rows.into(),
            },
            cell: CellPx {
                width: c.cell_px.w,
                height: c.cell_px.h,
            },
            name: c.name,
            project: c.project,
            preset: c.preset,
        }
    }
}

impl CreateSpec {
    fn command(&self) -> CommandBuilder {
        let mut cmd = match &self.program {
            Some(argv) => CommandBuilder::from_argv(argv.iter().map(OsString::from).collect()),
            None => CommandBuilder::new_default_prog(),
        };
        // portable-pty starts from a copy of our own environment: replace it with the GUI's.
        cmd.env_clear();
        for (key, value) in &self.env {
            cmd.env(key, value);
        }
        // As `pty::shell_command`: our TERM, whatever the GUI was started from.
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        match (&self.cwd, self.env.get("HOME")) {
            (Some(dir), _) => cmd.cwd(dir),
            (None, Some(home)) => cmd.cwd(home),
            (None, None) => {}
        }
        cmd
    }
}

/// The sending half of one client's queue for one session: bounded, and never blocks.
pub struct Subscriber {
    id: u64,
    queue: Arc<Queue>,
}

/// The receiving half: the client sends the attach snapshot first, then these frames in order.
/// It ends (`Disconnected`) once the session lets go: after `detach`, after a `Resync`, or when
/// the session is dropped.
pub struct FrameReceiver {
    queue: Arc<Queue>,
}

struct Queue {
    capacity: usize,
    inner: Mutex<QueueInner>,
    ready: Condvar,
}

#[derive(Default)]
struct QueueInner {
    frames: VecDeque<Frame>,
    /// The session let go of this subscriber: nothing more will come.
    closed: bool,
    /// The receiver was dropped: the session should let go.
    receiver_gone: bool,
}

impl Queue {
    fn lock(&self) -> MutexGuard<'_, QueueInner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// A subscriber queue holding at most `capacity` frames, identified by `id` (unique per
/// session; attaching again with the same id replaces the old one).
#[allow(dead_code)] // used by the daemon from M10a Task 4
pub fn subscriber(id: u64, capacity: usize) -> (Subscriber, FrameReceiver) {
    let queue = Arc::new(Queue {
        capacity: capacity.max(1),
        inner: Mutex::default(),
        ready: Condvar::new(),
    });
    (
        Subscriber {
            id,
            queue: queue.clone(),
        },
        FrameReceiver { queue },
    )
}

impl Subscriber {
    #[allow(dead_code)] // used by the daemon from M10a Task 4
    pub fn id(&self) -> u64 {
        self.id
    }

    /// Queue `frame`. False when this subscriber is done: its receiver is gone, or the queue was
    /// full, in which case the backlog was dropped and replaced by `Resync{session}` (the client
    /// re-attaches for a fresh snapshot, so memory stays bounded and a flood skips frames).
    fn push(&self, frame: Frame, session: u64) -> bool {
        let mut q = self.queue.lock();
        if q.receiver_gone {
            return false;
        }
        let keep = q.frames.len() < self.queue.capacity;
        if keep {
            q.frames.push_back(frame);
        } else {
            q.frames.clear();
            q.frames.push_back(Frame::Resync { id: session });
        }
        self.queue.ready.notify_all();
        keep
    }
}

impl Drop for Subscriber {
    fn drop(&mut self) {
        self.queue.lock().closed = true;
        self.queue.ready.notify_all();
    }
}

#[allow(dead_code)] // used by the daemon from M10a Task 4
impl FrameReceiver {
    pub fn recv(&self) -> Result<Frame, RecvError> {
        let mut q = self.queue.lock();
        loop {
            if let Some(frame) = q.frames.pop_front() {
                return Ok(frame);
            }
            if q.closed {
                return Err(RecvError);
            }
            q = self
                .queue
                .ready
                .wait(q)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }

    pub fn recv_timeout(&self, timeout: Duration) -> Result<Frame, RecvTimeoutError> {
        let Some(deadline) = Instant::now().checked_add(timeout) else {
            return self.recv().map_err(|_| RecvTimeoutError::Disconnected);
        };
        let mut q = self.queue.lock();
        loop {
            if let Some(frame) = q.frames.pop_front() {
                return Ok(frame);
            }
            if q.closed {
                return Err(RecvTimeoutError::Disconnected);
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(RecvTimeoutError::Timeout);
            }
            q = self
                .queue
                .ready
                .wait_timeout(q, left)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
    }

    pub fn try_recv(&self) -> Result<Frame, TryRecvError> {
        let mut q = self.queue.lock();
        match q.frames.pop_front() {
            Some(frame) => Ok(frame),
            None if q.closed => Err(TryRecvError::Disconnected),
            None => Err(TryRecvError::Empty),
        }
    }
}

impl Drop for FrameReceiver {
    fn drop(&mut self) {
        self.queue.lock().receiver_gone = true;
    }
}

/// Collects the Term's events. It runs inside `Processor::advance` with the session lock held,
/// so it only queues; the lock holder handles them right after (`State::handle_events`).
struct Listener(Sender<Event>);

impl EventListener for Listener {
    fn send_event(&self, event: Event) {
        let _ = self.0.send(event);
    }
}

/// Metadata kept for `List` and set by `SetMeta`.
#[derive(Default)]
struct Meta {
    name: Option<String>,
    project: Option<String>,
    preset: Option<String>,
    note: Option<String>,
    progress: Option<mux_proto::Progress>,
}

/// Everything behind the session lock.
struct State {
    id: u64,
    term: Term<Listener>,
    processor: Processor<StdSyncHandler>,
    /// What the Term raised (titles, bells, queries), drained by `handle_events`.
    events: mpsc::Receiver<Event>,
    /// Number of the last Output or Resized frame.
    seq: u64,
    subscribers: Vec<Subscriber>,
    tracker: StatusTracker,
    status: Status,
    title: Option<String>,
    theme: Theme,
    dims: TermDims,
    cell: CellPx,
    meta: Meta,
    alive: bool,
    /// Unix seconds.
    created_at: u64,
    /// `None` once closed; dropping it kills the child.
    pty: Option<Pty>,
    /// To the writer thread: client input and query replies. `None` once closed.
    to_child: Option<Sender<Vec<u8>>>,
    /// Bytes of an unfinished sequence, appended to snapshots.
    tail: SequenceTail,
}

impl State {
    /// Queue `frame` to every subscriber, letting go of those that are gone or overflowed.
    fn broadcast(&mut self, frame: Frame) {
        let id = self.id;
        self.subscribers.retain(|s| s.push(frame.clone(), id));
    }

    /// Output from the child: parse it, number it, queue it, then act on what it raised.
    fn output(&mut self, bytes: &[u8], notifications: usize, now: Instant) {
        if !self.alive {
            return; // closed: what the dying child still wrote goes nowhere
        }
        self.processor.advance(&mut self.term, bytes);
        self.tail.feed(bytes);
        self.seq += 1;
        self.broadcast(Frame::Output {
            id: self.id,
            seq: self.seq,
            notifications: u32::try_from(notifications).unwrap_or(u32::MAX),
            bytes: bytes.to_vec(),
        });
        self.handle_events(now);
        if let Some(status) = self.tracker.on_output(now, notifications) {
            self.set_status(status);
        }
    }

    /// Titles and bells go out as frames; queries are answered straight back to the child.
    fn handle_events(&mut self, now: Instant) {
        while let Ok(event) = self.events.try_recv() {
            match event {
                Event::Title(title) => {
                    self.title = Some(title);
                    self.broadcast_title();
                }
                Event::ResetTitle => {
                    self.title = None;
                    self.broadcast_title();
                }
                Event::Bell => {
                    self.tracker.on_bell(now);
                    self.broadcast(Frame::Bell { id: self.id });
                }
                event => {
                    let size = window_size(self.dims, self.cell);
                    let (overrides, theme) = (self.term.colors(), &self.theme);
                    let color = |index| colors::resolve_index(index, overrides, theme);
                    if let Some(reply) = query_reply(&event, color, size) {
                        if let Some(to_child) = &self.to_child {
                            let _ = to_child.send(reply.into_bytes());
                        }
                    }
                }
            }
        }
    }

    fn broadcast_title(&mut self) {
        let frame = Frame::Title {
            id: self.id,
            title: self.title.clone(),
        };
        self.broadcast(frame);
    }

    fn set_status(&mut self, status: Status) -> Frame {
        self.status = status;
        let frame = Frame::Status {
            id: self.id,
            status: status.as_str().into(),
        };
        self.broadcast(frame.clone());
        frame
    }

    /// Apply a synchronized update (DEC 2026) the parser is still buffering. Before a snapshot
    /// this is required: the buffered bytes were already sent as Output, so a snapshot without
    /// them would leave a new replica missing the start of the update.
    fn flush_sync(&mut self, now: Instant) {
        self.processor.stop_sync(&mut self.term);
        self.handle_events(now);
    }

    /// Not alive any more, and `Exited` sent: once, whether the child exited or was closed.
    fn mark_exited(&mut self) {
        if self.alive {
            self.alive = false;
            let frame = Frame::Exited { id: self.id };
            self.broadcast(frame);
        }
    }

    fn sync_deadline(&self) -> Option<Instant> {
        self.processor.sync_timeout().sync_timeout()
    }
}

fn wire_dims(dims: TermDims) -> Dims {
    Dims {
        cols: u16::try_from(dims.cols).unwrap_or(u16::MAX),
        rows: u16::try_from(dims.rows).unwrap_or(u16::MAX),
    }
}

/// One session. Dropping it kills the child, like `close`.
#[allow(dead_code)] // used by the daemon from M10a Task 4
pub struct DaemonSession {
    id: u64,
    state: Arc<FairMutex<State>>,
}

#[allow(dead_code)] // used by the daemon from M10a Task 4
impl DaemonSession {
    /// Start `spec`'s program on a new PTY, as session `id`.
    pub fn spawn(id: u64, spec: CreateSpec) -> io::Result<Self> {
        if spec.dims.cols == 0 || spec.dims.rows == 0 {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "zero size"));
        }
        let dims = clamp(spec.dims);
        let (pty, reader, writer) = Pty::spawn(spec.command(), dims).map_err(io::Error::other)?;
        if spec.cell != CellPx::default() {
            let _ = pty.resize(dims, spec.cell);
        }
        let to_child = pty::spawn_writer(writer)?;
        if let Some(startup) = &spec.startup {
            // The program reads this as typed-ahead input once it is up.
            let _ = to_child.send(format!("{startup}\r").into_bytes());
        }
        let (events_tx, events) = mpsc::channel();
        let created_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let state = Arc::new(FairMutex::new(State {
            id,
            term: Term::new(Config::default(), &dims, Listener(events_tx)),
            processor: Processor::new(),
            events,
            seq: 0,
            subscribers: Vec::new(),
            tracker: StatusTracker::new(),
            status: Status::Idle,
            title: None,
            theme: Theme::default(),
            dims,
            cell: spec.cell,
            meta: Meta {
                name: spec.name,
                project: spec.project,
                preset: spec.preset,
                note: None,
                progress: None,
            },
            alive: true,
            created_at,
            pty: Some(pty),
            to_child: Some(to_child),
            tail: SequenceTail::default(),
        }));

        let reader_state = state.clone();
        let eof_state = state.clone();
        // alacritty_terminal ignores notification OSCs, so a byte scanner watches the raw stream.
        let mut detector = OscDetector::new();
        pty::spawn_reader(
            reader,
            move |bytes| {
                let notifications = detector.feed(bytes);
                reader_state
                    .lock()
                    .output(bytes, notifications, Instant::now());
            },
            move || eof_state.lock().mark_exited(),
        )?;
        Ok(Self { id, state })
    }

    pub fn id(&self) -> u64 {
        self.id
    }

    /// Snapshot the Term and subscribe `sub`, under one lock: the snapshot is at `seq` N and
    /// `sub` gets every frame from N+1. Send the snapshot before anything from the receiver.
    /// Over `MAX_FRAME`, the oldest history lines are left out of the snapshot.
    pub fn attach(&self, sub: Subscriber) -> Snapshot {
        let mut s = self.state.lock();
        if s.sync_deadline().is_some() {
            s.flush_sync(Instant::now());
        }
        let title_room = 6 * s.title.as_ref().map_or(0, String::len); // JSON escapes
        let budget = MAX_FRAME.saturating_sub(SNAPSHOT_HEADROOM + title_room);
        let mut vt_bytes = bounded_snapshot(&mut s.term, budget);
        vt_bytes.extend_from_slice(s.tail.bytes());
        let snapshot = Snapshot {
            id: self.id,
            seq: s.seq,
            dims: wire_dims(s.dims),
            title: s.title.clone(),
            vt_bytes,
        };
        s.subscribers.retain(|old| old.id != sub.id);
        s.subscribers.push(sub);
        snapshot
    }

    /// Stop sending to subscriber `sub_id` (its receiver then ends).
    pub fn detach(&self, sub_id: u64) {
        self.state.lock().subscribers.retain(|s| s.id != sub_id);
    }

    /// Bytes from a client (keystrokes, paste, scripted send): counts as typing. Query replies
    /// never come this way; the session writes those itself.
    pub fn input(&self, bytes: &[u8]) {
        let mut s = self.state.lock();
        if let Some(status) = s.tracker.on_input(Instant::now()) {
            s.set_status(status);
        }
        if let Some(to_child) = &s.to_child {
            let _ = to_child.send(bytes.to_vec());
        }
    }

    /// Resize the Term and the PTY. A change of cell count is numbered into the output stream
    /// as `Resized`, before the child can redraw for it, so replicas reflow at the same byte.
    /// Zero sizes and no-change calls are ignored; sizes over `MAX_COLS` x `MAX_ROWS` are
    /// clamped.
    pub fn resize(&self, dims: TermDims, cell: CellPx) {
        if dims.cols == 0 || dims.rows == 0 {
            return;
        }
        let dims = clamp(dims);
        let mut s = self.state.lock();
        if (s.dims, s.cell) == (dims, cell) {
            return;
        }
        if s.dims != dims {
            s.dims = dims;
            s.term.resize(dims);
            s.seq += 1;
            let frame = Frame::Resized(mux_proto::Resized {
                id: self.id,
                seq: s.seq,
                cols: wire_dims(dims).cols,
                rows: wire_dims(dims).rows,
            });
            s.broadcast(frame);
        }
        s.cell = cell;
        if let Some(pty) = &s.pty {
            let _ = pty.resize(dims, cell);
        }
    }

    /// The colours OSC 4/10/11/12 replies use from now on.
    pub fn set_theme(&self, theme: Theme) {
        self.state.lock().theme = theme;
    }

    /// Apply `SetMeta`: absent fields stay, `null` clears, a value sets.
    pub fn set_meta(&self, meta: &SetMeta) {
        let m = &mut self.state.lock().meta;
        if let Some(v) = &meta.name {
            m.name = v.clone();
        }
        if let Some(v) = &meta.project {
            m.project = v.clone();
        }
        if let Some(v) = &meta.preset {
            m.preset = v.clone();
        }
        if let Some(v) = &meta.note {
            m.note = v.clone();
        }
        if let Some(v) = &meta.progress {
            m.progress = v.clone();
        }
    }

    /// End the session now: `Exited` goes out (once, even if the child already exited), the
    /// writer thread's queue closes, and the child's whole process group gets SIGHUP, as when
    /// a terminal window closes. Then the child itself is killed (SIGHUP, then SIGKILL after a
    /// grace period). When it exits, macOS revokes the terminal from every process still
    /// holding it, so the reader sees EOF and ends even if a job ignored SIGHUP.
    pub fn close(&self) {
        let pty = {
            let mut s = self.state.lock();
            s.mark_exited();
            s.to_child = None;
            s.pty.take()
        };
        if let Some(pty) = pty {
            pty.hang_up();
            drop(pty); // outside the lock: the kill can wait up to ~200 ms
        }
    }

    /// Housekeeping, every 50 ms: flush a synchronized update stuck past its deadline, then
    /// re-evaluate the status. Returns the `Status` frames this queued to subscribers, so the
    /// daemon can act on them too (e.g. notify while no UI is attached).
    pub fn tick(&self, now: Instant) -> Vec<Frame> {
        let mut s = self.state.lock();
        if s.sync_deadline().is_some_and(|deadline| now >= deadline) {
            s.flush_sync(now);
        }
        let State { tracker, term, .. } = &mut *s;
        let changed = tracker.tick(now, || grid::screen_tail_text(term, TAIL_LINES));
        changed
            .map(|status| s.set_status(status))
            .into_iter()
            .collect()
    }

    /// The session's entry for `List`. `name` is empty until one is set.
    pub fn info(&self) -> SessionInfo {
        let s = self.state.lock();
        SessionInfo {
            id: self.id,
            name: s.meta.name.clone().unwrap_or_default(),
            project: s.meta.project.clone(),
            preset: s.meta.preset.clone(),
            title: s.title.clone(),
            status: s.status.as_str().into(),
            note: s.meta.note.clone(),
            progress: s.meta.progress.clone(),
            dims: wire_dims(s.dims),
            alive: s.alive,
            created_at: s.created_at,
        }
    }

    /// The authoritative Term's text, for `ReadText`.
    pub fn text(&self, scope: TextScope) -> String {
        grid::text_lines(&self.state.lock().term, scope)
    }
}

impl Drop for DaemonSession {
    fn drop(&mut self) {
        self.close();
    }
}

/// The snapshot bytes, halving the history kept until they fit in `budget`. If even the
/// screen alone doesn't fit (only possible with hostile output, e.g. thousands of combining
/// marks per cell: the clamp keeps any ordinary screen well under a frame), the snapshot is
/// just a reset: the replica starts blank and fills in as the program redraws, which beats a
/// frame the client would reject.
fn bounded_snapshot<L: EventListener>(term: &mut Term<L>, budget: usize) -> Vec<u8> {
    let mut history = vt_snapshot::history_lines(term);
    loop {
        let bytes = vt_snapshot::encode_with_history(term, history);
        if bytes.len() <= budget {
            return bytes;
        }
        if history == 0 {
            return b"\x1bc".to_vec();
        }
        history /= 2;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alacritty_terminal::event::VoidListener;
    use alacritty_terminal::grid::Dimensions;
    use std::path::Path;

    const ID: u64 = 7;

    fn sh_spec(script: &str, cols: usize, rows: usize) -> CreateSpec {
        CreateSpec {
            program: Some(vec!["/bin/sh".into(), "-c".into(), script.into()]),
            env: std::env::vars().collect(),
            cwd: None,
            startup: None,
            dims: TermDims { cols, rows },
            cell: CellPx::default(),
            name: None,
            project: None,
            preset: None,
        }
    }

    fn spawn(script: &str) -> DaemonSession {
        DaemonSession::spawn(ID, sh_spec(script, 40, 6)).unwrap()
    }

    /// Frames until (and including) the first one matching `stop`; panics after 5 s.
    fn recv_until(rx: &FrameReceiver, mut stop: impl FnMut(&Frame) -> bool) -> Vec<Frame> {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut frames = Vec::new();
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match rx.recv_timeout(left) {
                Ok(f) => {
                    let done = stop(&f);
                    frames.push(f);
                    if done {
                        return frames;
                    }
                }
                Err(e) => panic!("{e:?} after {} frames: {frames:?}", frames.len()),
            }
        }
    }

    fn until_exited(rx: &FrameReceiver) -> Vec<Frame> {
        recv_until(rx, |f| matches!(f, Frame::Exited { .. }))
    }

    fn output_text(frames: &[Frame]) -> String {
        frames
            .iter()
            .filter_map(|f| match f {
                Frame::Output { bytes, .. } => Some(String::from_utf8_lossy(bytes).into_owned()),
                _ => None,
            })
            .collect()
    }

    /// The seq of every numbered frame (Output and Resized), in arrival order.
    fn seqs(frames: &[Frame]) -> Vec<u64> {
        frames
            .iter()
            .filter_map(|f| match f {
                Frame::Output { seq, .. } => Some(*seq),
                Frame::Resized(r) => Some(r.seq),
                _ => None,
            })
            .collect()
    }

    /// A GUI-side copy of the Term, built from a snapshot and then fed frames.
    struct Replica {
        term: Term<VoidListener>,
        processor: Processor<StdSyncHandler>,
    }

    impl Replica {
        fn new(snap: &Snapshot) -> Self {
            let dims = TermDims {
                cols: snap.dims.cols as usize,
                rows: snap.dims.rows as usize,
            };
            let mut r = Replica {
                term: Term::new(Config::default(), &dims, VoidListener),
                processor: Processor::new(),
            };
            r.processor.advance(&mut r.term, &snap.vt_bytes);
            r
        }

        fn apply(&mut self, frame: &Frame) {
            match frame {
                Frame::Output { bytes, .. } => self.processor.advance(&mut self.term, bytes),
                Frame::Resized(r) => self.term.resize(TermDims {
                    cols: r.cols as usize,
                    rows: r.rows as usize,
                }),
                _ => {}
            }
        }

        fn encoded(&mut self) -> Vec<u8> {
            vt_snapshot::encode(&mut self.term)
        }
    }

    fn authoritative(s: &DaemonSession) -> Vec<u8> {
        vt_snapshot::encode(&mut s.state.lock().term)
    }

    fn assert_same_screen(replica: &mut Replica, s: &DaemonSession) {
        let (a, b) = (replica.encoded(), authoritative(s));
        assert!(
            a == b,
            "replica differs:\nreplica: {:?}\ndaemon:  {:?}",
            String::from_utf8_lossy(&a),
            String::from_utf8_lossy(&b)
        );
    }

    /// Output fed straight into the session's output path, as the reader would.
    fn feed(s: &DaemonSession, bytes: &[u8]) {
        s.state.lock().output(bytes, 0, Instant::now());
    }

    /// Frames until the output so far contains `text`.
    fn until_output(rx: &FrameReceiver, text: &str) -> Vec<Frame> {
        let mut seen = String::new();
        recv_until(rx, |f| {
            seen.push_str(&output_text(std::slice::from_ref(f)));
            seen.contains(text)
        })
    }

    fn tail_of(chunks: &[&[u8]]) -> Vec<u8> {
        let mut t = SequenceTail::default();
        chunks.iter().for_each(|c| t.feed(c));
        t.bytes().to_vec()
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("fuxx-ds-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn read(dir: &Path, file: &str) -> String {
        String::from_utf8_lossy(&std::fs::read(dir.join(file)).unwrap_or_default()).into_owned()
    }

    // --- pure pieces ---

    #[test]
    fn a_full_queue_drops_its_backlog_for_resync() {
        let (sub, rx) = subscriber(1, 3);
        for seq in 1..=3 {
            assert!(sub.push(Frame::Bell { id: seq }, ID));
        }
        assert!(
            !sub.push(Frame::Bell { id: 4 }, ID),
            "a full queue cuts it off"
        );
        drop(sub); // the session lets go of it
        assert_eq!(rx.try_recv(), Ok(Frame::Resync { id: ID }));
        assert_eq!(rx.try_recv(), Err(TryRecvError::Disconnected));
    }

    #[test]
    fn a_queue_delivers_in_order_and_notices_a_dropped_receiver() {
        let (sub, rx) = subscriber(1, 8);
        assert!(sub.push(Frame::Bell { id: 1 }, ID));
        assert!(sub.push(Frame::Bell { id: 2 }, ID));
        assert_eq!(rx.recv(), Ok(Frame::Bell { id: 1 }));
        assert_eq!(
            rx.recv_timeout(Duration::from_millis(10)),
            Ok(Frame::Bell { id: 2 })
        );
        assert_eq!(
            rx.recv_timeout(Duration::from_millis(10)),
            Err(RecvTimeoutError::Timeout)
        );
        drop(rx);
        assert!(!sub.push(Frame::Bell { id: 3 }, ID), "receiver gone");
    }

    #[test]
    fn the_tail_is_an_unfinished_sequence_only() {
        let cases: [(&[&[u8]], &[u8]); 18] = [
            (&[b"plain text\r\n"], b""),
            (&[b"ab\x1b[3"], b"\x1b[3"),
            (&[b"\x1b[", b"3"], b"\x1b[3"),
            (&[b"\x1b[31m"], b""),
            (&[b"\x1b[?1049h"], b""),
            (&[b"\x1b[3\n"], b"\x1b[3"), // the LF already ran: not replayed
            (&[b"\x1b[3\x18x"], b""),    // CAN aborts the sequence
            (&[b"\x1b("], b"\x1b("),
            (&[b"\x1b(B"], b""),
            (&[b"\x1b", b"\x1b["], b"\x1b["),
            (&[b"ab\xc3"], b"\xc3"),
            (&[b"\xe2\x94", b"\x80"], b""),
            (&[b"\xe2\x94x"], b""), // broken UTF-8: the parser already gave up on it
            (&[b"\x1b]2;ti"], b"\x1b]2;ti"),
            (&[b"\x1b]2;title\x07"], b""),
            (&[b"\x1b]2;title\x1b"], b"\x1b"),
            (&[b"\x1b]2;title\x1b\\"], b""),
            (&[b"\x1bP1$qm"], b"\x1bP1$qm"),
        ];
        for (chunks, tail) in cases {
            assert_eq!(
                tail_of(chunks),
                tail,
                "{:?}",
                chunks.concat().escape_ascii().to_string()
            );
        }
    }

    #[test]
    fn an_overlong_sequence_is_dropped_from_the_tail() {
        let mut t = SequenceTail::default();
        t.feed(b"\x1b]52;c;");
        t.feed(&[b'A'; TAIL_LIMIT]);
        assert_eq!(t.bytes(), b"");
        t.feed(b"\x07\x1b[1");
        assert_eq!(t.bytes(), b"\x1b[1", "the scanner starts over");
    }

    #[test]
    fn create_frame_becomes_a_spec() {
        let spec = CreateSpec::from(mux_proto::Create {
            cwd: Some("/tmp".into()),
            startup: Some("claude".into()),
            env: BTreeMap::from([("A".into(), "1".into())]),
            dims: Dims {
                cols: 100,
                rows: 30,
            },
            cell_px: mux_proto::CellPx { w: 8, h: 17 },
            name: Some("n".into()),
            project: Some("p".into()),
            preset: Some("agent".into()),
        });
        assert_eq!(spec.program, None);
        assert_eq!(spec.cwd, Some(PathBuf::from("/tmp")));
        assert_eq!(spec.startup.as_deref(), Some("claude"));
        assert_eq!(spec.env.get("A").map(String::as_str), Some("1"));
        assert_eq!(
            spec.dims,
            TermDims {
                cols: 100,
                rows: 30
            }
        );
        assert_eq!(
            spec.cell,
            CellPx {
                width: 8,
                height: 17
            }
        );
        assert_eq!(
            (spec.name, spec.project, spec.preset),
            (Some("n".into()), Some("p".into()), Some("agent".into()))
        );
    }

    #[test]
    fn an_oversized_snapshot_drops_the_oldest_history() {
        let dims = TermDims { cols: 20, rows: 4 };
        let mut term = Term::new(Config::default(), &dims, VoidListener);
        let mut processor: Processor<StdSyncHandler> = Processor::new();
        for i in 0..500 {
            processor.advance(&mut term, format!("line {i}\r\n").as_bytes());
        }
        let full = vt_snapshot::encode(&mut term);
        assert_eq!(bounded_snapshot(&mut term, usize::MAX), full);
        let budget = full.len() / 4;
        let bytes = bounded_snapshot(&mut term, budget);
        assert!(bytes.len() <= budget, "{} > {budget}", bytes.len());
        let mut rebuilt = Term::new(Config::default(), &dims, VoidListener);
        Processor::<StdSyncHandler>::new().advance(&mut rebuilt, &bytes);
        assert!(rebuilt.grid().history_size() < term.grid().history_size());
        assert!(rebuilt.grid().history_size() > 0, "kept what fits");
        let screen = |t: &Term<VoidListener>| grid::text_lines(t, TextScope::Screen);
        assert_eq!(screen(&rebuilt), screen(&term));
        assert!(grid::text_lines(&rebuilt, TextScope::All).contains("line 499"));
    }

    // --- with a real child ---

    /// Attach between `before` and `after`: the replica must match once `after` has arrived.
    fn attach_between(before: &[u8], after: &[u8]) {
        let s = spawn("sleep 30");
        feed(&s, b"start ");
        feed(&s, before);
        let (sub, rx) = subscriber(1, QUEUE_CAPACITY);
        let snap = s.attach(sub);
        feed(&s, after);
        let mut replica = Replica::new(&snap);
        while let Ok(f) = rx.try_recv() {
            replica.apply(&f);
        }
        assert_same_screen(&mut replica, &s);
    }

    #[test]
    fn attach_mid_csi_carries_the_unfinished_bytes() {
        attach_between(b"\x1b[3", b"1mX");
    }

    #[test]
    fn attach_mid_utf8_carries_the_unfinished_bytes() {
        attach_between(b"\xc3", b"\xa9 done");
    }

    #[test]
    fn attach_mid_alt_screen_switch_carries_the_unfinished_bytes() {
        attach_between(b"\x1b[?10", b"49halt screen");
    }

    #[test]
    fn attach_mid_osc_title_carries_the_unfinished_bytes() {
        attach_between(b"\x1b]2;hel", b"lo\x07after");
    }

    #[test]
    fn attach_mid_csi_does_not_replay_a_control_that_already_ran() {
        attach_between(b"\x1b[3\n", b"1mX");
    }

    #[test]
    fn close_ends_everything_even_while_a_job_holds_the_terminal() {
        // The job ignores SIGHUP, leaves the session and keeps the terminal open.
        let s = spawn(
            r#"(trap "" HUP; exec perl -MPOSIX -e 'POSIX::setsid(); sleep 8') & echo started; sleep 30"#,
        );
        let (sub, rx) = subscriber(1, QUEUE_CAPACITY);
        s.attach(sub);
        until_output(&rx, "started");
        s.close();
        // Sent by close itself, not by the reader's EOF later.
        assert_eq!(rx.try_recv(), Ok(Frame::Exited { id: ID }));
        assert!(!s.info().alive);
        assert!(
            s.state.lock().to_child.is_none(),
            "the writer's queue is closed"
        );
        let deadline = Instant::now() + Duration::from_secs(2);
        while Arc::strong_count(&s.state) > 1 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(Arc::strong_count(&s.state), 1, "the reader thread ended");
        s.close();
        assert_eq!(rx.try_recv(), Err(TryRecvError::Empty), "Exited only once");
    }

    #[test]
    fn exited_is_sent_once_when_closed_after_exit() {
        let s = spawn("read x");
        let (sub, rx) = subscriber(1, QUEUE_CAPACITY);
        s.attach(sub);
        s.input(b"\r");
        until_exited(&rx);
        s.close();
        assert_eq!(rx.try_recv(), Err(TryRecvError::Empty));
    }

    #[test]
    fn sizes_are_clamped() {
        let s = DaemonSession::spawn(ID, sh_spec("sleep 30", 5000, 2000)).unwrap();
        let max = Dims {
            cols: MAX_COLS as u16,
            rows: MAX_ROWS as u16,
        };
        assert_eq!(s.info().dims, max);
        s.resize(TermDims { cols: 40, rows: 6 }, CellPx::default());
        let (sub, rx) = subscriber(1, QUEUE_CAPACITY);
        s.attach(sub);
        s.resize(
            TermDims {
                cols: 3000,
                rows: 9,
            },
            CellPx::default(),
        );
        let frames = recv_until(&rx, |f| matches!(f, Frame::Resized(_)));
        assert!(matches!(
            frames.last(),
            Some(Frame::Resized(r)) if (r.cols, r.rows) == (max.cols, 9)
        ));
        assert_eq!(s.state.lock().term.columns(), MAX_COLS);
    }

    #[test]
    fn a_full_size_screen_with_a_colour_per_cell_is_sent_whole() {
        let dims = TermDims {
            cols: MAX_COLS,
            rows: MAX_ROWS,
        };
        let mut term = Term::new(Config::default(), &dims, VoidListener);
        let mut bytes = Vec::new();
        for i in 0..MAX_COLS * MAX_ROWS {
            let (r, g, b) = (i % 251, i % 241, i % 239);
            bytes.extend_from_slice(format!("\x1b[38;2;{r};{g};{b}m\u{e9}").as_bytes());
        }
        Processor::<StdSyncHandler>::new().advance(&mut term, &bytes);
        let budget = MAX_FRAME - SNAPSHOT_HEADROOM;
        let snap = bounded_snapshot(&mut term, budget);
        assert!(snap.len() <= budget, "{}", snap.len());
        let mut rebuilt = Term::new(Config::default(), &dims, VoidListener);
        Processor::<StdSyncHandler>::new().advance(&mut rebuilt, &snap);
        let screen = |t: &Term<VoidListener>| grid::text_lines(t, TextScope::Screen);
        assert_eq!(screen(&rebuilt), screen(&term));
    }

    #[test]
    fn a_screen_too_big_for_any_frame_falls_back_to_a_blank_snapshot() {
        let dims = TermDims { cols: 20, rows: 4 };
        let mut term = Term::new(Config::default(), &dims, VoidListener);
        // Combining marks are unbounded per cell, so no size limit alone can cap a screen.
        let marks = "e".to_string() + &"\u{301}".repeat(500);
        Processor::<StdSyncHandler>::new().advance(&mut term, marks.as_bytes());
        assert_eq!(bounded_snapshot(&mut term, 600), b"\x1bc");
    }

    #[test]
    fn output_frames_are_numbered_in_order() {
        let s = spawn("read go; for i in $(seq 1 30); do echo line$i; done");
        let (sub, rx) = subscriber(1, QUEUE_CAPACITY);
        let snap = s.attach(sub);
        s.input(b"\r");
        let frames = until_exited(&rx);
        let seqs = seqs(&frames);
        assert!(seqs.len() > 1, "{frames:?}");
        let expected: Vec<u64> = (snap.seq + 1..=snap.seq + seqs.len() as u64).collect();
        assert_eq!(seqs, expected);
        assert!(output_text(&frames).contains("line30"));
        assert!(s.text(TextScope::Screen).contains("line30"));
        assert!(!s.info().alive);
    }

    #[test]
    fn attach_mid_stream_gives_a_snapshot_then_every_later_frame() {
        let s = spawn(
            r"burst() { for i in $(seq $1 $2); do printf '\033[3%dm' $((i % 8)); seq 1 50; done; }
              burst 1 20; echo half; read go; burst 21 40",
        );
        let (first, first_rx) = subscriber(1, QUEUE_CAPACITY);
        let first_snap = s.attach(first);
        let mut first_frames = until_output(&first_rx, "half");
        let (late, late_rx) = subscriber(2, QUEUE_CAPACITY);
        let late_snap = s.attach(late);
        assert!(late_snap.seq > first_snap.seq, "attached mid-stream");
        s.input(b"\r");

        let late_frames = until_exited(&late_rx);
        let late_seqs = seqs(&late_frames);
        assert!(!late_seqs.is_empty(), "the stream went on after attach");
        assert_eq!(late_seqs[0], late_snap.seq + 1, "no gap, no overlap");
        let mut late_replica = Replica::new(&late_snap);
        late_frames.iter().for_each(|f| late_replica.apply(f));
        assert_same_screen(&mut late_replica, &s);

        let mut first_replica = Replica::new(&first_snap);
        first_frames.extend(until_exited(&first_rx));
        first_frames.iter().for_each(|f| first_replica.apply(f));
        assert_same_screen(&mut first_replica, &s);
    }

    #[test]
    fn attach_during_a_synchronized_update_misses_nothing() {
        let s =
            spawn(r"stty -echo; printf '\033[?2026hpart1'; read go; printf ' part2\033[?2026l'");
        let (first, first_rx) = subscriber(1, QUEUE_CAPACITY);
        s.attach(first);
        recv_until(&first_rx, |f| matches!(f, Frame::Output { .. }));
        let (late, late_rx) = subscriber(2, QUEUE_CAPACITY);
        let snap = s.attach(late);
        s.input(b"\r");
        let mut replica = Replica::new(&snap);
        until_exited(&late_rx).iter().for_each(|f| replica.apply(f));
        assert_same_screen(&mut replica, &s);
        assert!(s.text(TextScope::Screen).contains("part1 part2"));
    }

    #[test]
    fn a_cursor_position_query_is_answered_with_no_client_attached() {
        let dir = temp_dir("cpr");
        let mut spec = sh_spec(
            r#"stty raw -echo min 0 time 30; printf '\033[6n'; dd bs=64 count=1 2>/dev/null > "$OUT/cpr""#,
            40,
            6,
        );
        spec.env.insert("OUT".into(), dir.display().to_string());
        let s = DaemonSession::spawn(ID, spec).unwrap();
        let (sub, rx) = subscriber(1, QUEUE_CAPACITY);
        s.attach(sub); // only to see the exit: nothing is answered through it
        until_exited(&rx);
        assert_eq!(read(&dir, "cpr"), "\x1b[1;1R");
    }

    #[test]
    fn colour_attribute_and_size_queries_are_answered_from_the_last_theme_and_cell() {
        let dir = temp_dir("queries");
        let mut spec = sh_spec(
            r#"read go; stty raw -echo min 0 time 30
               q() { printf "$1"; dd bs=64 count=1 2>/dev/null > "$OUT/$2"; }
               q '\033[c' da1; q '\033]11;?\007' bg; q '\033[14t' px"#,
            80,
            24,
        );
        spec.env.insert("OUT".into(), dir.display().to_string());
        spec.cell = CellPx {
            width: 8,
            height: 17,
        };
        let s = DaemonSession::spawn(ID, spec).unwrap();
        let mut theme = Theme::default();
        theme.background = crate::theme::parse_color("#123456").unwrap();
        s.set_theme(theme);
        let (sub, rx) = subscriber(1, QUEUE_CAPACITY);
        s.attach(sub);
        s.input(b"\r"); // only now does the child ask
        until_exited(&rx);
        assert_eq!(read(&dir, "da1"), "\x1b[?6c");
        assert_eq!(read(&dir, "bg"), "\x1b]11;rgb:1212/3434/5656\x07");
        assert_eq!(read(&dir, "px"), "\x1b[4;408;640t");
    }

    #[test]
    fn resize_is_numbered_into_the_output_stream() {
        let s = spawn(
            "stty -echo; printf 'a long line that wraps past forty columns, then some'; read go; stty size",
        );
        let (sub, rx) = subscriber(1, QUEUE_CAPACITY);
        let snap = s.attach(sub);
        let mut frames = until_output(&rx, "then some");
        let cell = CellPx {
            width: 8,
            height: 17,
        };
        s.resize(TermDims { cols: 30, rows: 9 }, cell);
        s.resize(TermDims { cols: 30, rows: 9 }, cell); // unchanged: no frame
        s.resize(TermDims { cols: 0, rows: 0 }, cell); // ignored
        s.input(b"\r");
        frames.extend(until_exited(&rx));

        let seqs = seqs(&frames);
        let expected: Vec<u64> = (snap.seq + 1..=snap.seq + seqs.len() as u64).collect();
        assert_eq!(seqs, expected, "Resized takes its own seq");
        let resized: Vec<_> = frames
            .iter()
            .filter_map(|f| match f {
                Frame::Resized(r) => Some(r.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(resized.len(), 1, "{frames:?}");
        assert_eq!(
            (resized[0].id, resized[0].cols, resized[0].rows),
            (ID, 30, 9)
        );
        let resized_at = frames
            .iter()
            .position(|f| matches!(f, Frame::Resized(_)))
            .unwrap();
        assert!(
            output_text(&frames[resized_at..]).contains("9 30"),
            "the child saw the new size after the Resized frame"
        );
        let mut replica = Replica::new(&snap);
        frames.iter().for_each(|f| replica.apply(f));
        assert_same_screen(&mut replica, &s);
        assert_eq!(s.info().dims, Dims { cols: 30, rows: 9 });
    }

    #[test]
    fn an_overflowing_subscriber_gets_resync() {
        let s = spawn("sleep 30");
        let (slow, slow_rx) = subscriber(1, 4);
        let (watch, watch_rx) = subscriber(2, QUEUE_CAPACITY);
        s.attach(slow);
        s.attach(watch);
        for i in 1..=20 {
            feed(&s, format!("{i}\r\n").as_bytes());
        }
        assert_eq!(
            seqs(&std::iter::from_fn(|| watch_rx.try_recv().ok()).collect::<Vec<_>>()).len(),
            20
        );
        assert_eq!(slow_rx.try_recv(), Ok(Frame::Resync { id: ID }));
        assert_eq!(slow_rx.try_recv(), Err(TryRecvError::Disconnected));
        // Re-attaching gets a fresh snapshot.
        let (again, _rx) = subscriber(1, 4);
        assert!(String::from_utf8_lossy(&s.attach(again).vt_bytes).contains("20"));
    }

    #[test]
    fn status_goes_working_then_settles_once_quiet() {
        let s = spawn("stty -echo; sleep 30");
        let (sub, rx) = subscriber(1, QUEUE_CAPACITY);
        s.attach(sub);
        s.input(b"go");
        // Output well past the echo window, as the reader would report it.
        let out_at = Instant::now() + Duration::from_millis(500);
        s.state.lock().output(b"busy\r\n", 0, out_at);
        recv_until(
            &rx,
            |f| matches!(f, Frame::Status { id: ID, status } if status == "working"),
        );
        assert_eq!(s.info().status, "working");
        assert!(s.tick(out_at).is_empty(), "not quiet yet");
        let later = out_at + crate::status::IDLE_AFTER;
        let done = Frame::Status {
            id: ID,
            status: "done".into(),
        };
        assert_eq!(s.tick(later), vec![done.clone()]);
        recv_until(&rx, |f| *f == done);
        assert_eq!(s.info().status, "done");
    }

    #[test]
    fn title_bell_and_exit_go_out_as_frames() {
        let s = spawn(r"stty -echo; read go; printf '\033]2;hello\007'; printf '\007'; read go");
        let (sub, rx) = subscriber(1, QUEUE_CAPACITY);
        s.attach(sub);
        s.input(b"\r");
        let frames = recv_until(&rx, |f| matches!(f, Frame::Bell { .. }));
        assert!(frames.contains(&Frame::Title {
            id: ID,
            title: Some("hello".into())
        }));
        let (again, _rx) = subscriber(2, QUEUE_CAPACITY);
        assert_eq!(s.attach(again).title.as_deref(), Some("hello"));
        assert_eq!(s.info().title.as_deref(), Some("hello"));
        s.input(b"\r");
        until_exited(&rx);
    }

    #[test]
    fn startup_env_and_metadata() {
        let mut spec = sh_spec(
            r#"read line; printf "got:$line term:$TERM t:$FUXX_T user:[$USER]""#,
            60,
            4,
        );
        spec.env = BTreeMap::from([
            ("PATH".into(), "/usr/bin:/bin".into()),
            ("FUXX_T".into(), "yes".into()),
        ]);
        spec.startup = Some("hi".into());
        spec.name = Some("agent".into());
        let s = DaemonSession::spawn(ID, spec).unwrap();
        let (sub, rx) = subscriber(1, QUEUE_CAPACITY);
        s.attach(sub);
        until_exited(&rx);
        let text = s.text(TextScope::Screen);
        assert!(
            text.contains("got:hi term:xterm-256color t:yes user:[]"),
            "{text}"
        );
        let info = s.info();
        assert_eq!((info.id, info.name.as_str()), (ID, "agent"));
        s.set_meta(&SetMeta {
            id: ID,
            name: None,
            project: Some(Some("p".into())),
            preset: None,
            note: Some(Some("n".into())),
            progress: None,
        });
        s.set_meta(&SetMeta {
            id: ID,
            name: None,
            project: None,
            preset: None,
            note: Some(None),
            progress: None,
        });
        let info = s.info();
        assert_eq!(info.name, "agent");
        assert_eq!(info.project.as_deref(), Some("p"));
        assert_eq!(info.note, None);
    }

    #[test]
    fn close_kills_the_child() {
        let s = spawn("sleep 30");
        let (sub, rx) = subscriber(1, QUEUE_CAPACITY);
        s.attach(sub);
        s.close();
        until_exited(&rx);
        assert!(!s.info().alive);
    }
}
