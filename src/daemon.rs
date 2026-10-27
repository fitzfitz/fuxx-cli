//! M10a: `fuxx daemon`, the background service that owns the sessions (study §2.3). No GPUI.
//!
//! One process, plain threads:
//! - an accept thread on `fuxxd.sock` (M9's rules, from `private_socket`: 0700 folder, 0600
//!   socket, same-user peers only, at most 64 connections, a live socket never replaced);
//! - per connection, a reader thread that decodes frames and handles them in order, and a
//!   writer thread fed by a bounded queue. Each session the client attaches gets a forwarding
//!   thread that moves frames from that session's own bounded subscriber queue to the writer
//!   queue. Sessions never wait for a client: a slow one overflows its subscriber queue and gets
//!   `Resync` (`daemon_session`), and only its own threads wait on the writer;
//! - one tick thread: every 50 ms it ticks each session (status, stuck synchronized updates),
//!   forgets sessions whose `Exited` has gone out, and exits the daemon after `IDLE_EXIT` with
//!   no sessions and no connections.
//!
//! Session ids `s<N>` count up from 1 and are never reused while the daemon lives (study
//! §2.3.7 trap 1). A session's environment is the one its `Create` carries (trap 2). The last
//! `SetTheme` is kept for new sessions and for replies while no GUI is attached (trap 3).
//! Resize authority belongs to one UI client at a time: the latest to say `Hello` or `Attach`,
//! like `tmux attach -d`; scripts never resize (trap 5).
//!
//! Control comes only from the socket, never from PTY output (§2.3.8). Logging goes to stderr,
//! which is `~/Library/Logs/fuxx/daemon.log` when the GUI spawns the daemon.

use std::collections::{BTreeMap, HashMap};
use std::io::{self, ErrorKind, Read, Write};
use std::os::unix::net::UnixStream;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, Weak};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use alacritty_terminal::vte::ansi::Rgb;
use serde_json::{Map, Value, json};

use crate::daemon_session::{self, CreateSpec, DaemonSession, FrameReceiver, QUEUE_CAPACITY};
use crate::grid::TextScope;
use crate::mux_proto::{
    self, DaemonInfo, Decoder, Frame, Hello, HelloOk, IncompatibleInfo, PROTO_MAX, PROTO_MIN,
    ProtoError, Role, SessionInfo, StopReply,
};
use crate::private_socket::{self, Slot, SocketGuard};
use crate::terminal::{CellPx, TermDims};
use crate::theme::{self, Theme};

/// With no sessions and no clients for this long, the daemon exits.
pub const IDLE_EXIT: Duration = Duration::from_secs(60);

/// How often sessions are ticked.
const TICK: Duration = Duration::from_millis(50);

/// Live connections at most; more are accepted and closed at once.
const MAX_CONNECTIONS: usize = 64;

/// Encoded frames a connection may have waiting for its writer; senders then wait.
const OUT_QUEUE: usize = 256;

/// How long a reply may take to reach the socket before the connection is closed anyway.
const FLUSH: Duration = Duration::from_secs(1);

const VERSION: &str = env!("CARGO_PKG_VERSION");

macro_rules! log {
    ($($arg:tt)*) => {
        eprintln!("[fuxx daemon] {}", format_args!($($arg)*))
    };
}

/// `~/Library/Application Support/fuxx/fuxxd.sock`: beside M9's `fuxx.sock`, not instead of it.
pub fn socket_path(home: &Path) -> PathBuf {
    home.join("Library/Application Support/fuxx/fuxxd.sock")
}

/// `~/Library/Logs/fuxx/daemon.log`: the spawned daemon's stdout and stderr.
#[allow(dead_code)] // used by the GUI's spawn from M10a Task 5
pub fn log_path(home: &Path) -> PathBuf {
    home.join("Library/Logs/fuxx/daemon.log")
}

/// The daemon socket: `FUXX_DAEMON_SOCKET` when set and non-empty, else the one under home.
pub fn client_socket_path(env_socket: Option<&str>, home: Option<&Path>) -> Option<PathBuf> {
    match env_socket.filter(|s| !s.is_empty()) {
        Some(socket) => Some(PathBuf::from(socket)),
        None => home.map(socket_path),
    }
}

fn color_hex(c: Rgb) -> Value {
    json!(format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b))
}

/// A theme as `SetTheme`'s palette: `#rrggbb` strings; the optional colours may be `null`.
#[allow(dead_code)] // used by the GUI client from M10a Task 5
pub fn theme_to_wire(theme: &Theme) -> Value {
    let optional = |c: Option<Rgb>| c.map_or(Value::Null, color_hex);
    json!({
        "background": color_hex(theme.background),
        "foreground": color_hex(theme.foreground),
        "cursor": optional(theme.cursor),
        "cursor_text": optional(theme.cursor_text),
        "selection_background": optional(theme.selection_background),
        "selection_foreground": optional(theme.selection_foreground),
        "palette": theme.palette.iter().map(|&c| color_hex(c)).collect::<Vec<_>>(),
    })
}

/// `SetTheme`'s palette as a theme. Absent keys keep the default theme's value; a colour that
/// doesn't parse, or a palette without exactly 16 colours, refuses the whole thing.
pub fn theme_from_wire(palette: &Value) -> Option<Theme> {
    let empty = Map::new();
    let map = palette.as_object().unwrap_or(&empty);
    let color = |v: &Value| v.as_str().and_then(theme::parse_color);
    let mut t = Theme::default();
    for (key, slot) in [
        ("background", &mut t.background),
        ("foreground", &mut t.foreground),
    ] {
        if let Some(v) = map.get(key) {
            *slot = color(v)?;
        }
    }
    for (key, slot) in [
        ("cursor", &mut t.cursor),
        ("cursor_text", &mut t.cursor_text),
        ("selection_background", &mut t.selection_background),
        ("selection_foreground", &mut t.selection_foreground),
    ] {
        match map.get(key) {
            None | Some(Value::Null) => {}
            Some(v) => *slot = Some(color(v)?),
        }
    }
    if let Some(v) = map.get("palette") {
        let colors = v.as_array()?;
        if colors.len() != t.palette.len() {
            return None;
        }
        for (slot, v) in t.palette.iter_mut().zip(colors) {
            *slot = color(v)?;
        }
    }
    palette.is_object().then_some(t)
}

/// `ReadText`'s scope: `screen`, `all`, or `last:N` (the last N non-blank lines).
fn text_scope(scope: &str) -> Option<TextScope> {
    match scope {
        "screen" => Some(TextScope::Screen),
        "all" => Some(TextScope::All),
        _ => scope
            .strip_prefix("last:")?
            .parse()
            .ok()
            .map(TextScope::Last),
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// What every thread of the daemon shares.
struct Shared {
    sessions: Mutex<BTreeMap<u64, Arc<DaemonSession>>>,
    /// The next session's number; never goes back.
    next_id: AtomicU64,
    next_conn: AtomicU64,
    /// The connection allowed to resize (0: none).
    resize_owner: AtomicU64,
    /// The last `SetTheme`.
    theme: Mutex<Option<Theme>>,
    /// Open connections, counted by `private_socket::accept_loop`.
    live: Arc<AtomicUsize>,
    /// Every open connection's socket, to close them when the daemon exits.
    conns: Mutex<HashMap<u64, Arc<UnixStream>>>,
    /// Asked to exit (`Stop`, or the owner of `Running` let go).
    quit: AtomicBool,
    started: Instant,
}

impl Shared {
    fn session(&self, id: u64) -> Option<Arc<DaemonSession>> {
        lock(&self.sessions).get(&id).cloned()
    }

    fn all(&self) -> Vec<Arc<DaemonSession>> {
        lock(&self.sessions).values().cloned().collect()
    }
}

/// A daemon serving in this process. Dropping it stops the daemon and waits for it.
pub struct Running {
    tick: Option<JoinHandle<()>>,
    shared: Arc<Shared>,
}

impl Running {
    /// Block until the daemon exits (idle, or `Stop`).
    pub fn wait(mut self) {
        if let Some(tick) = self.tick.take() {
            let _ = tick.join();
        }
    }

    #[allow(dead_code)] // tests; the in-process daemon of M10a Task 5's tests
    pub fn is_finished(&self) -> bool {
        self.tick.as_ref().is_none_or(JoinHandle::is_finished)
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        self.shared.quit.store(true, Ordering::Release);
        if let Some(tick) = self.tick.take() {
            let _ = tick.join();
        }
    }
}

/// Bind `path` and serve until stopped or idle for `idle_exit`. `AddrInUse` when a daemon
/// already listens there.
pub fn start(path: &Path, idle_exit: Duration) -> io::Result<Running> {
    start_with(path, idle_exit, true)
}

/// `start`; with `reap` false, ended sessions stay listed (tests only).
fn start_with(path: &Path, idle_exit: Duration, reap: bool) -> io::Result<Running> {
    let listener = private_socket::bind(path)?;
    let stop = Arc::new(AtomicBool::new(false));
    let guard = SocketGuard::new(path, stop.clone());
    let shared = Arc::new(Shared {
        sessions: Mutex::default(),
        next_id: AtomicU64::new(1),
        next_conn: AtomicU64::new(1),
        resize_owner: AtomicU64::new(0),
        theme: Mutex::default(),
        live: Arc::new(AtomicUsize::new(0)),
        conns: Mutex::default(),
        quit: AtomicBool::new(false),
        started: Instant::now(),
    });
    let accepting = shared.clone();
    std::thread::Builder::new()
        .name("fuxx-daemon-accept".into())
        .spawn(move || {
            let live = accepting.live.clone();
            private_socket::accept_loop(
                listener,
                stop,
                live,
                MAX_CONNECTIONS,
                "daemon",
                || true,
                |stream, slot| serve(stream, slot, accepting.clone()),
            );
        })?;
    let ticking = shared.clone();
    let tick = std::thread::Builder::new()
        .name("fuxx-daemon-tick".into())
        .spawn(move || tick_loop(&ticking, guard, idle_exit, reap))?;
    Ok(Running {
        tick: Some(tick),
        shared,
    })
}

/// Every `TICK` until asked to quit or idle: tick the sessions, forget the ones that ended.
/// Then end every session and connection and remove the socket.
fn tick_loop(shared: &Shared, guard: SocketGuard, idle_exit: Duration, reap: bool) {
    let mut idle_since: Option<Instant> = None;
    while !shared.quit.load(Ordering::Acquire) {
        let now = Instant::now();
        let sessions = shared.all();
        for session in &sessions {
            // The returned Status frames are already queued to subscribers.
            if catch_unwind(AssertUnwindSafe(|| session.tick(now))).is_err() {
                log!("s{}: tick panicked", session.id());
            }
        }
        // A session's `alive` turns false together with queueing `Exited`, so forgetting it now
        // loses nothing: its subscribers still drain their queues.
        let ended: Vec<u64> = sessions
            .iter()
            .filter(|s| reap && !s.info().alive)
            .map(|s| s.id())
            .collect();
        drop(sessions);
        if !ended.is_empty() {
            let removed: Vec<_> = {
                let mut table = lock(&shared.sessions);
                ended.iter().filter_map(|id| table.remove(id)).collect()
            };
            for session in removed {
                log!("s{} ended", session.id());
            }
        }
        let idle = lock(&shared.sessions).is_empty() && shared.live.load(Ordering::Acquire) == 0;
        match (idle, idle_since) {
            (false, _) => idle_since = None,
            (true, None) => idle_since = Some(now),
            (true, Some(since)) if now.duration_since(since) >= idle_exit => {
                log!("idle for {idle_exit:?} with no sessions and no clients; exiting");
                break;
            }
            (true, Some(_)) => {}
        }
        std::thread::sleep(TICK);
    }
    // Dropping a session ends its child.
    let sessions = std::mem::take(&mut *lock(&shared.sessions));
    std::thread::scope(|scope| {
        for (_, session) in sessions {
            scope.spawn(move || drop(session));
        }
    });
    drop(guard);
    for (_, stream) in lock(&shared.conns).drain() {
        let _ = stream.shutdown(std::net::Shutdown::Both);
    }
    log!("stopped");
}

/// The sending side of a connection's writer queue.
#[derive(Clone)]
struct Out {
    queue: SyncSender<Vec<u8>>,
    /// Frames queued but not yet written.
    pending: Arc<AtomicUsize>,
    /// The writer has stopped (the client went away).
    closed: Arc<AtomicBool>,
}

impl Out {
    /// Queue `frame`, waiting while the queue is full. False once the connection is gone.
    fn send(&self, frame: &Frame) -> bool {
        self.pending.fetch_add(1, Ordering::AcqRel);
        if self.queue.send(mux_proto::encode(frame)).is_ok() {
            return true;
        }
        self.pending.fetch_sub(1, Ordering::AcqRel);
        false
    }

    /// Wait up to `timeout` for everything queued to be written.
    fn flushed(&self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        while self.pending.load(Ordering::Acquire) > 0 {
            if self.closed.load(Ordering::Acquire) || Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        true
    }
}

/// Start the writer and reader threads for one accepted connection.
fn serve(stream: UnixStream, slot: Slot, shared: Arc<Shared>) {
    let id = shared.next_conn.fetch_add(1, Ordering::AcqRel);
    let stream = Arc::new(stream);
    let (queue, frames) = sync_channel::<Vec<u8>>(OUT_QUEUE);
    let out = Out {
        queue,
        pending: Arc::new(AtomicUsize::new(0)),
        closed: Arc::new(AtomicBool::new(false)),
    };
    let (socket, pending, closed) = (stream.clone(), out.pending.clone(), out.closed.clone());
    let writer = std::thread::Builder::new()
        .name("fuxx-daemon-write".into())
        .spawn(move || write_loop(&socket, &frames, &pending, &closed, slot));
    if writer.is_err() {
        return;
    }
    lock(&shared.conns).insert(id, stream.clone());
    let spawned = std::thread::Builder::new()
        .name("fuxx-daemon-read".into())
        .spawn(move || {
            let mut conn = Connection {
                id,
                shared,
                stream,
                out,
                role: None,
                attached: HashMap::new(),
            };
            if catch_unwind(AssertUnwindSafe(|| conn.run())).is_err() {
                log!("connection {id}: handler panicked; closing it");
            }
            // `conn` drops here, panic or not: detach, give up authority, close.
        });
    if spawned.is_err() {
        log!("connection {id}: no thread for it");
    }
}

/// Write queued frames until every sender is gone or the socket fails. `slot` counts the
/// connection as open until then.
fn write_loop(
    socket: &UnixStream,
    frames: &Receiver<Vec<u8>>,
    pending: &AtomicUsize,
    closed: &AtomicBool,
    _slot: Slot,
) {
    for bytes in frames {
        if (&*socket).write_all(&bytes).is_err() {
            closed.store(true, Ordering::Release);
            let _ = socket.shutdown(std::net::Shutdown::Both);
            return;
        }
        pending.fetch_sub(1, Ordering::AcqRel);
    }
    closed.store(true, Ordering::Release);
}

/// Move one session's frames to the connection's writer until the session lets go of this
/// subscriber or the connection is gone. Returns whether `Exited` went out.
fn forward(frames: &FrameReceiver, out: &Out) -> bool {
    let mut exited = false;
    while let Ok(frame) = frames.recv() {
        exited |= matches!(frame, Frame::Exited { .. });
        if !out.send(&frame) {
            break;
        }
    }
    exited
}

/// One client connection, handled on its reader thread.
struct Connection {
    id: u64,
    shared: Arc<Shared>,
    stream: Arc<UnixStream>,
    out: Out,
    /// Set by `Hello`.
    role: Option<Role>,
    /// The forwarding thread per attached session.
    attached: HashMap<u64, (Weak<DaemonSession>, JoinHandle<bool>)>,
}

impl Connection {
    fn run(&mut self) {
        let mut decoder = Decoder::new();
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            let n = match (&*self.stream).read(&mut buf) {
                Ok(0) => return,
                Ok(n) => n,
                Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                Err(_) => return,
            };
            decoder.push(&buf[..n]);
            while let Some(result) = decoder.next() {
                let keep = match result {
                    Ok(frame) => self.handle(frame),
                    Err(e @ ProtoError::BadLength(_)) => {
                        log!("connection {}: {e}; closing it", self.id);
                        false
                    }
                    Err(e) => self.error(None, e.to_string()) && self.role.is_some(),
                };
                if !keep {
                    return;
                }
            }
        }
    }

    fn send(&self, frame: &Frame) -> bool {
        self.out.send(frame)
    }

    fn error(&self, id: Option<u64>, message: impl Into<String>) -> bool {
        self.send(&Frame::Error {
            id,
            message: message.into(),
        })
    }

    /// Handle one frame; false closes the connection.
    fn handle(&mut self, frame: Frame) -> bool {
        let Some(role) = self.role else {
            return match frame {
                Frame::Hello(hello) => self.hello(&hello),
                _ => {
                    self.error(None, "the first frame must be Hello");
                    false
                }
            };
        };
        match frame {
            Frame::Hello(_) => self.error(None, "Hello was already sent"),
            Frame::List => {
                let sessions = self.shared.all().iter().map(|s| s.info()).collect();
                self.send(&Frame::Sessions(sessions))
            }
            Frame::Create(create) => self.create(create),
            Frame::Attach { id } => self.attach(role, id),
            Frame::Detach { id } => {
                self.detach(id);
                true
            }
            Frame::Input { id, bytes } => {
                if let Some(session) = self.shared.session(id) {
                    session.input(&bytes);
                }
                true
            }
            Frame::Resize(r) => {
                let owner = self.shared.resize_owner.load(Ordering::Acquire);
                if role == Role::Ui
                    && owner == self.id
                    && let Some(session) = self.shared.session(r.id)
                {
                    let dims = TermDims {
                        cols: r.cols.into(),
                        rows: r.rows.into(),
                    };
                    let cell = CellPx {
                        width: r.cell_w,
                        height: r.cell_h,
                    };
                    session.resize(dims, cell);
                }
                true
            }
            Frame::SetTheme { palette } => match theme_from_wire(&palette) {
                Some(theme) => {
                    *lock(&self.shared.theme) = Some(theme);
                    for session in self.shared.all() {
                        session.set_theme(theme);
                    }
                    true
                }
                None => self.error(None, "SetTheme: not a palette"),
            },
            Frame::SetMeta(meta) => {
                if let Some(session) = self.shared.session(meta.id) {
                    session.set_meta(&meta);
                }
                true
            }
            Frame::Close { id } => {
                if let Some(session) = self.shared.session(id) {
                    session.close();
                }
                true
            }
            Frame::ReadText(r) => {
                let Some(scope) = text_scope(&r.scope) else {
                    return self.error(Some(r.id), format!("unknown scope {:?}", r.scope));
                };
                match self.shared.session(r.id) {
                    Some(session) => self.send(&Frame::Text(mux_proto::Text {
                        id: r.id,
                        text: session.text(scope),
                    })),
                    None => self.error(Some(r.id), format!("no session s{}", r.id)),
                }
            }
            Frame::Info => self.send(&Frame::DaemonInfo(DaemonInfo {
                pid: std::process::id(),
                version: VERSION.into(),
                proto: PROTO_MAX,
                uptime_secs: self.shared.started.elapsed().as_secs(),
                sessions: lock(&self.shared.sessions).len() as u64,
            })),
            Frame::Stop { force } => self.stop(force),
            // Daemon-to-client kinds, and kinds from a newer client: ignored.
            _ => true,
        }
    }

    fn hello(&mut self, hello: &Hello) -> bool {
        match mux_proto::negotiate(hello.proto_min, hello.proto_max) {
            Ok(proto) => {
                self.role = Some(hello.role);
                if hello.role == Role::Ui {
                    self.shared.resize_owner.store(self.id, Ordering::Release);
                }
                self.send(&Frame::HelloOk(HelloOk {
                    proto,
                    daemon_version: VERSION.into(),
                    pid: std::process::id(),
                }))
            }
            Err(e) => {
                log!("connection {}: {e}", self.id);
                self.send(&Frame::Incompatible(IncompatibleInfo {
                    proto_min: PROTO_MIN,
                    proto_max: PROTO_MAX,
                }));
                false
            }
        }
    }

    fn create(&mut self, create: mux_proto::Create) -> bool {
        // Numbered before spawning, so a failed spawn still never reuses its number.
        let id = self.shared.next_id.fetch_add(1, Ordering::AcqRel);
        // `program: None` is the login shell from the client's env; the env is used as sent.
        let spec = CreateSpec::from(create);
        match DaemonSession::spawn(id, spec) {
            Ok(session) => {
                let session = Arc::new(session);
                {
                    // Read the theme after inserting, under the table lock: a `SetTheme` racing
                    // this either sees the new session or stored its theme before we read it.
                    let mut table = lock(&self.shared.sessions);
                    table.insert(id, session.clone());
                    if let Some(theme) = *lock(&self.shared.theme) {
                        session.set_theme(theme);
                    }
                }
                log!("s{id} created");
                self.send(&Frame::Created { id })
            }
            Err(e) => self.error(None, format!("couldn't start a session: {e}")),
        }
    }

    /// Snapshot, then the session's frames. A UI client attaching takes resize authority.
    fn attach(&mut self, role: Role, id: u64) -> bool {
        if role == Role::Ui {
            self.shared.resize_owner.store(self.id, Ordering::Release);
        }
        let Some(session) = self.shared.session(id) else {
            return self.send(&Frame::Exited { id });
        };
        // Attaching again: let the old stream finish first, so it can't follow the new snapshot.
        self.detach(id);
        self.attached.retain(|_, (_, thread)| !thread.is_finished());
        let (sub, frames) = daemon_session::subscriber(self.id, QUEUE_CAPACITY);
        let snapshot = session.attach(sub);
        if !self.send(&Frame::Snapshot(snapshot)) {
            session.detach(self.id);
            return false;
        }
        let out = self.out.clone();
        let thread = std::thread::Builder::new()
            .name("fuxx-daemon-forward".into())
            .spawn(move || forward(&frames, &out));
        match thread {
            Ok(thread) => {
                self.attached.insert(id, (Arc::downgrade(&session), thread));
            }
            Err(e) => {
                session.detach(self.id);
                return self.error(Some(id), format!("couldn't attach: {e}"));
            }
        }
        // A child that ended before we subscribed sent its `Exited` to nobody: stop the stream
        // (it has nothing more to come) and send it, unless the stream just did.
        if !session.info().alive {
            let exited = self.detach(id);
            if !exited {
                return self.send(&Frame::Exited { id });
            }
        }
        true
    }

    /// Stop the session's stream to this client, after what it already queued. Returns
    /// whether that stream sent `Exited`.
    fn detach(&mut self, id: u64) -> bool {
        let Some((session, thread)) = self.attached.remove(&id) else {
            return false;
        };
        if let Some(session) = session.upgrade() {
            session.detach(self.id);
        }
        thread.join().unwrap_or(false)
    }

    /// Refuse while sessions live, unless `force`: then hang each one up. Then exit.
    fn stop(&mut self, force: bool) -> bool {
        let sessions = self.shared.all();
        let infos: Vec<SessionInfo> = sessions.iter().map(|s| s.info()).collect();
        if !infos.is_empty() && !force {
            return self.send(&Frame::StopReply(StopReply {
                stopping: false,
                sessions: infos,
            }));
        }
        // `close` is the hang-up: SIGHUP to the child's process group, as closing a terminal
        // window does, then a kill after a short grace.
        std::thread::scope(|scope| {
            for session in &sessions {
                scope.spawn(|| session.close());
            }
        });
        log!(
            "stop requested ({} session(s) hung up); exiting",
            sessions.len()
        );
        drop(sessions);
        self.send(&Frame::StopReply(StopReply {
            stopping: true,
            sessions: infos,
        }));
        self.out.flushed(FLUSH);
        self.shared.quit.store(true, Ordering::Release);
        true
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        for (_, (session, _thread)) in self.attached.drain() {
            // Not joined: a forwarder may be waiting on a client that stopped reading; it ends
            // once the socket is shut below.
            if let Some(session) = session.upgrade() {
                session.detach(self.id);
            }
        }
        let _ = self.shared.resize_owner.compare_exchange(
            self.id,
            0,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
        lock(&self.shared.conns).remove(&self.id);
        // Let a last reply (Incompatible, an error) reach the client before closing.
        self.out.flushed(FLUSH);
        let _ = self.stream.shutdown(std::net::Shutdown::Both);
    }
}

/// Why a daemon command failed.
#[derive(Debug)]
pub enum DaemonError {
    /// Nothing answers at the socket. Exit code 3.
    NotRunning(String),
    /// It answered, but not as expected.
    Failed(String),
}

impl std::fmt::Display for DaemonError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotRunning(m) | Self::Failed(m) => f.write_str(m),
        }
    }
}

/// A blocking client, for `fuxx daemon status|stop` (and tests).
pub struct Client {
    stream: UnixStream,
    decoder: Decoder,
}

impl Client {
    /// Connect and say `Hello` as `role`.
    pub fn connect(path: &Path, role: Role) -> Result<(Client, HelloOk), DaemonError> {
        let stream = UnixStream::connect(path).map_err(|e| {
            DaemonError::NotRunning(format!(
                "the fuxx daemon isn't running ({}: {e})",
                path.display()
            ))
        })?;
        let mut client = Client {
            stream,
            decoder: Decoder::new(),
        };
        let failed = |e: io::Error| DaemonError::Failed(format!("daemon handshake: {e}"));
        client
            .send(&Frame::Hello(Hello {
                proto_min: PROTO_MIN,
                proto_max: PROTO_MAX,
                app_version: VERSION.into(),
                role,
            }))
            .map_err(failed)?;
        match client.recv(Duration::from_secs(5)).map_err(failed)? {
            Frame::HelloOk(ok) => Ok((client, ok)),
            Frame::Incompatible(i) => Err(DaemonError::Failed(format!(
                "the running daemon speaks protocol {}..={}, this fuxx {PROTO_MIN}..={PROTO_MAX}",
                i.proto_min, i.proto_max
            ))),
            other => Err(DaemonError::Failed(format!(
                "unexpected handshake reply {other:?}"
            ))),
        }
    }

    pub fn send(&mut self, frame: &Frame) -> io::Result<()> {
        self.stream.write_all(&mux_proto::encode(frame))
    }

    /// The next frame, waiting up to `timeout`.
    pub fn recv(&mut self, timeout: Duration) -> io::Result<Frame> {
        let deadline = Instant::now() + timeout;
        let mut buf = [0u8; 64 * 1024];
        loop {
            if let Some(result) = self.decoder.next() {
                return result.map_err(io::Error::other);
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(io::Error::new(
                    ErrorKind::TimedOut,
                    "no reply from the daemon",
                ));
            }
            self.stream.set_read_timeout(Some(left))?;
            match self.stream.read(&mut buf) {
                Ok(0) => {
                    return Err(io::Error::new(
                        ErrorKind::UnexpectedEof,
                        "the daemon closed the connection",
                    ));
                }
                Ok(n) => self.decoder.push(&buf[..n]),
                Err(e) if e.kind() == ErrorKind::Interrupted => {}
                Err(e) if e.kind() == ErrorKind::WouldBlock => {
                    return Err(io::Error::new(
                        ErrorKind::TimedOut,
                        "no reply from the daemon",
                    ));
                }
                Err(e) => return Err(e),
            }
        }
    }

    /// Send `frame` and return the first reply `pick` accepts (other frames are skipped).
    fn request<T>(
        &mut self,
        frame: &Frame,
        mut pick: impl FnMut(Frame) -> Option<T>,
    ) -> Result<T, DaemonError> {
        let lost = |e: io::Error| DaemonError::Failed(format!("lost the daemon: {e}"));
        self.send(frame).map_err(lost)?;
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match self.recv(left).map_err(lost)? {
                Frame::Error { message, .. } => return Err(DaemonError::Failed(message)),
                frame => {
                    if let Some(found) = pick(frame) {
                        return Ok(found);
                    }
                }
            }
        }
    }
}

/// Ask the daemon at `path` about itself.
pub fn info(path: &Path) -> Result<DaemonInfo, DaemonError> {
    let (mut client, _) = Client::connect(path, Role::Script)?;
    client.request(&Frame::Info, |f| match f {
        Frame::DaemonInfo(info) => Some(info),
        _ => None,
    })
}

/// Ask the daemon at `path` to exit (see `Connection::stop`).
pub fn stop(path: &Path, force: bool) -> Result<StopReply, DaemonError> {
    let (mut client, _) = Client::connect(path, Role::Script)?;
    client.request(&Frame::Stop { force }, |f| match f {
        Frame::StopReply(reply) => Some(reply),
        _ => None,
    })
}

/// `s3 (name)`, or `s3` when it has no name.
fn label(info: &SessionInfo) -> String {
    if info.name.is_empty() {
        format!("s{}", info.id)
    } else {
        format!("s{} ({})", info.id, info.name)
    }
}

/// `2h 3m 4s`, leading zero units dropped.
fn duration_text(secs: u64) -> String {
    let (h, m, s) = (secs / 3600, secs / 60 % 60, secs % 60);
    match (h, m) {
        (0, 0) => format!("{s}s"),
        (0, _) => format!("{m}m {s}s"),
        _ => format!("{h}h {m}m {s}s"),
    }
}

/// What `fuxx daemon status` prints.
fn render_info(info: &DaemonInfo) -> String {
    format!(
        "fuxx daemon is running\n  pid       {}\n  version   {}\n  protocol  {}\n  uptime    {}\n  sessions  {}",
        info.pid,
        info.version,
        info.proto,
        duration_text(info.uptime_secs),
        info.sessions
    )
}

/// `fuxx daemon run`: serve in the foreground until stopped or idle. Exit code.
pub fn run_command(path: &Path) -> i32 {
    match start(path, IDLE_EXIT) {
        Ok(running) => {
            log!(
                "listening on {} (pid {}, version {VERSION}, protocol {PROTO_MIN}..={PROTO_MAX})",
                path.display(),
                std::process::id()
            );
            running.wait();
            0
        }
        Err(e) => {
            eprintln!("fuxx daemon: {e}");
            1
        }
    }
}

/// `fuxx daemon status`. Exit code: 0 running, 3 not running.
pub fn status_command(path: &Path) -> i32 {
    match info(path) {
        Ok(info) => {
            println!("{}", render_info(&info));
            0
        }
        Err(e @ DaemonError::NotRunning(_)) => {
            eprintln!("fuxx: {e}");
            3
        }
        Err(e) => {
            eprintln!("fuxx: {e}");
            1
        }
    }
}

/// `fuxx daemon stop [--force]`. Exit code: 0 stopped, 1 refused, 3 not running.
pub fn stop_command(path: &Path, force: bool) -> i32 {
    match stop(path, force) {
        Ok(reply) if reply.stopping => {
            // Wait (briefly) until it is really gone, so a following `status` agrees.
            let deadline = Instant::now() + Duration::from_secs(5);
            while UnixStream::connect(path).is_ok() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(50));
            }
            match reply.sessions.len() {
                0 => println!("fuxx daemon stopped"),
                n => println!("fuxx daemon stopped ({n} session(s) hung up)"),
            }
            0
        }
        Ok(reply) => {
            let names: Vec<String> = reply.sessions.iter().map(label).collect();
            eprintln!(
                "fuxx: the daemon still has {} live session(s): {}\n(close them first, or use `fuxx daemon stop --force` to hang them up)",
                names.len(),
                names.join(", ")
            );
            1
        }
        Err(e @ DaemonError::NotRunning(_)) => {
            eprintln!("fuxx: {e}");
            3
        }
        Err(e) => {
            eprintln!("fuxx: {e}");
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mux_proto::{
        CellPx, Create, Dims, Hello, IncompatibleInfo, PROTO_MAX, PROTO_MIN, ReadText, Resize,
        SessionInfo, encode,
    };
    use std::io::{Read, Write};
    use std::time::Instant;

    /// A fresh socket path in its own temp folder.
    fn temp_sock(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("fuxxd-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("d.sock")
    }

    fn daemon(name: &str) -> (Running, PathBuf) {
        let path = temp_sock(name);
        (start(&path, IDLE_EXIT).unwrap(), path)
    }

    fn connect(path: &Path, role: Role) -> Client {
        Client::connect(path, role).unwrap().0
    }

    /// The test's environment with `/bin/sh` as the login shell: quick and the same everywhere.
    fn sh_env() -> BTreeMap<String, String> {
        let mut env: BTreeMap<String, String> = std::env::vars().collect();
        env.insert("SHELL".into(), "/bin/sh".into());
        env.insert("PS1".into(), "$ ".into());
        env
    }

    fn create_frame(name: &str) -> Frame {
        Frame::Create(Create {
            cwd: None,
            startup: None,
            env: sh_env(),
            dims: Dims { cols: 80, rows: 24 },
            cell_px: CellPx { w: 8, h: 16 },
            name: Some(name.into()),
            project: None,
            preset: None,
        })
    }

    /// Frames until (and including) the first matching `stop`; panics after 5 s.
    fn recv_until(c: &mut Client, mut stop: impl FnMut(&Frame) -> bool) -> Vec<Frame> {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut frames = Vec::new();
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match c.recv(left.max(Duration::from_millis(1))) {
                Ok(f) => {
                    let done = stop(&f);
                    frames.push(f);
                    if done {
                        return frames;
                    }
                }
                Err(e) => panic!("{e} after {frames:?}"),
            }
        }
    }

    fn create(c: &mut Client, name: &str) -> u64 {
        c.send(&create_frame(name)).unwrap();
        match recv_until(c, |f| {
            matches!(f, Frame::Created { .. } | Frame::Error { .. })
        })
        .pop()
        .unwrap()
        {
            Frame::Created { id } => id,
            other => panic!("{other:?}"),
        }
    }

    fn list(c: &mut Client) -> Vec<SessionInfo> {
        c.send(&Frame::List).unwrap();
        match recv_until(c, |f| matches!(f, Frame::Sessions(_))).pop() {
            Some(Frame::Sessions(s)) => s,
            other => panic!("{other:?}"),
        }
    }

    fn ids(sessions: &[SessionInfo]) -> Vec<u64> {
        sessions.iter().map(|s| s.id).collect()
    }

    /// Poll `cond` every 20 ms for up to 5 s.
    fn eventually(mut cond: impl FnMut() -> bool) -> bool {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if cond() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        cond()
    }

    #[test]
    fn handshake_ok_names_this_process() {
        let (_d, path) = daemon("hello");
        let (_c, ok) = Client::connect(&path, Role::Ui).unwrap();
        assert_eq!(ok.proto, PROTO_MAX);
        assert_eq!(ok.pid, std::process::id());
        assert_eq!(ok.daemon_version, env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn handshake_incompatible_replies_then_closes() {
        let (_d, path) = daemon("incompat");
        let mut s = UnixStream::connect(&path).unwrap();
        let hello = Frame::Hello(Hello {
            proto_min: PROTO_MAX + 5,
            proto_max: PROTO_MAX + 9,
            app_version: "9".into(),
            role: Role::Ui,
        });
        s.write_all(&encode(&hello)).unwrap();
        s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let mut got = Vec::new();
        s.read_to_end(&mut got).unwrap(); // ends: the daemon closed it
        let mut d = Decoder::new();
        d.push(&got);
        assert_eq!(
            d.next(),
            Some(Ok(Frame::Incompatible(IncompatibleInfo {
                proto_min: PROTO_MIN,
                proto_max: PROTO_MAX,
            })))
        );
        assert_eq!(d.next(), None);
    }

    #[test]
    fn a_first_frame_other_than_hello_is_an_error_and_closes() {
        let (_d, path) = daemon("nohello");
        let mut s = UnixStream::connect(&path).unwrap();
        s.write_all(&encode(&Frame::List)).unwrap();
        s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let mut got = Vec::new();
        s.read_to_end(&mut got).unwrap();
        let mut d = Decoder::new();
        d.push(&got);
        assert!(matches!(d.next(), Some(Ok(Frame::Error { id: None, .. }))));
    }

    #[test]
    fn create_list_close_and_ids_are_never_reused() {
        let (_d, path) = daemon("crud");
        let mut c = connect(&path, Role::Ui);
        assert_eq!(list(&mut c), vec![]);
        let a = create(&mut c, "one");
        let b = create(&mut c, "two");
        assert_eq!((a, b), (1, 2));
        let sessions = list(&mut c);
        assert_eq!(ids(&sessions), vec![1, 2]);
        assert_eq!(sessions[0].name, "one");
        assert!(sessions.iter().all(|s| s.alive));
        c.send(&Frame::Close { id: a }).unwrap();
        assert!(eventually(|| ids(&list(&mut c)) == vec![2]));
        // A new session gets a new number, never 1 again.
        assert_eq!(create(&mut c, "three"), 3);
    }

    #[test]
    fn attach_gets_snapshot_then_output_of_input() {
        let (_d, path) = daemon("attach");
        let mut c = connect(&path, Role::Ui);
        let id = create(&mut c, "a");
        c.send(&Frame::Attach { id }).unwrap();
        let first = recv_until(&mut c, |_| true).pop().unwrap();
        let Frame::Snapshot(snap) = first else {
            panic!("{first:?}")
        };
        assert_eq!((snap.id, snap.dims), (id, Dims { cols: 80, rows: 24 }));
        // The echo of the typed line has `$((6*7))`, so only the output has `42X`.
        c.send(&Frame::Input {
            id,
            bytes: b"echo $((6*7))X\r".to_vec(),
        })
        .unwrap();
        let mut text = String::new();
        let frames = recv_until(&mut c, |f| {
            if let Frame::Output { bytes, .. } = f {
                text.push_str(&String::from_utf8_lossy(bytes));
            }
            text.contains("42X")
        });
        // Numbered from just after the snapshot, with no gap.
        let seqs: Vec<u64> = frames
            .iter()
            .filter_map(|f| match f {
                Frame::Output { seq, .. } => Some(*seq),
                _ => None,
            })
            .collect();
        let want: Vec<u64> = (snap.seq + 1..snap.seq + 1 + seqs.len() as u64).collect();
        assert_eq!(seqs, want);
        // ReadText sees it too.
        c.send(&Frame::ReadText(ReadText {
            id,
            scope: "screen".into(),
        }))
        .unwrap();
        let text = match recv_until(&mut c, |f| matches!(f, Frame::Text(_))).pop() {
            Some(Frame::Text(t)) => t.text,
            other => panic!("{other:?}"),
        };
        assert!(text.contains("42X"), "{text}");
    }

    #[test]
    fn attaching_to_an_ended_session_still_listed_says_exited() {
        let path = temp_sock("deadattach");
        let _d = start_with(&path, IDLE_EXIT, false).unwrap();
        let mut c = connect(&path, Role::Ui);
        let id = create(&mut c, "a");
        c.send(&Frame::Input {
            id,
            bytes: b"exit\r".to_vec(),
        })
        .unwrap();
        assert!(eventually(|| !list(&mut c)[0].alive));
        // Its Exited went out before this client subscribed.
        let mut late = connect(&path, Role::Script);
        late.send(&Frame::Attach { id }).unwrap();
        let frames = recv_until(&mut late, |f| matches!(f, Frame::Exited { .. }));
        assert!(matches!(frames[0], Frame::Snapshot(_)), "{frames:?}");
        let exits = frames
            .iter()
            .filter(|f| matches!(f, Frame::Exited { .. }))
            .count();
        assert_eq!(exits, 1);
    }

    #[test]
    fn exited_is_delivered_then_the_session_is_forgotten() {
        let (_d, path) = daemon("exited");
        let mut c = connect(&path, Role::Ui);
        let id = create(&mut c, "a");
        c.send(&Frame::Attach { id }).unwrap();
        c.send(&Frame::Input {
            id,
            bytes: b"exit\r".to_vec(),
        })
        .unwrap();
        recv_until(&mut c, |f| matches!(f, Frame::Exited { id: i } if *i == id));
        assert!(eventually(|| list(&mut c).is_empty()));
        // Attaching to it now says it's gone.
        c.send(&Frame::Attach { id }).unwrap();
        let last = recv_until(&mut c, |f| {
            matches!(f, Frame::Exited { .. } | Frame::Snapshot(_))
        });
        assert_eq!(last.last(), Some(&Frame::Exited { id }));
    }

    fn resize(id: u64, cols: u16) -> Frame {
        Frame::Resize(Resize {
            id,
            cols,
            rows: 24,
            cell_w: 8,
            cell_h: 16,
        })
    }

    fn cols(c: &mut Client) -> u16 {
        list(c)[0].dims.cols
    }

    #[test]
    fn only_the_latest_ui_client_resizes() {
        let (_d, path) = daemon("authority");
        let mut ui = connect(&path, Role::Ui);
        let id = create(&mut ui, "a");
        ui.send(&Frame::Attach { id }).unwrap();
        let mut script = connect(&path, Role::Script);
        script.send(&resize(id, 100)).unwrap();
        assert_eq!(cols(&mut script), 80); // ignored: scripts never resize
        ui.send(&resize(id, 90)).unwrap();
        assert!(eventually(|| cols(&mut script) == 90));
        // A new UI takes over, like `tmux attach -d`.
        let mut ui2 = connect(&path, Role::Ui);
        ui.send(&resize(id, 70)).unwrap();
        assert_eq!(cols(&mut ui), 90);
        ui2.send(&resize(id, 60)).unwrap();
        assert_eq!(cols(&mut ui2), 60);
    }

    #[test]
    fn idle_exit_after_no_sessions_and_no_clients() {
        let path = temp_sock("idle");
        let d = start(&path, Duration::from_millis(200)).unwrap();
        // A client keeps it alive.
        let c = connect(&path, Role::Script);
        std::thread::sleep(Duration::from_millis(500));
        assert!(!d.is_finished());
        drop(c);
        assert!(eventually(|| d.is_finished()));
        assert!(!path.exists(), "the socket is removed");
    }

    #[test]
    fn a_session_keeps_it_alive_without_clients() {
        let path = temp_sock("idlesess");
        let d = start(&path, Duration::from_millis(200)).unwrap();
        let mut c = connect(&path, Role::Ui);
        create(&mut c, "a");
        drop(c);
        std::thread::sleep(Duration::from_millis(600));
        assert!(!d.is_finished());
    }

    #[test]
    fn stop_is_refused_while_sessions_live_then_forced() {
        let (d, path) = daemon("stop");
        let mut c = connect(&path, Role::Ui);
        let id = create(&mut c, "agent");
        let refused = stop(&path, false).unwrap();
        assert!(!refused.stopping);
        assert_eq!(ids(&refused.sessions), vec![id]);
        assert_eq!(refused.sessions[0].name, "agent");
        assert!(!d.is_finished());
        c.send(&Frame::Attach { id }).unwrap();
        let forced = stop(&path, true).unwrap();
        assert!(forced.stopping);
        // The session was hung up, then the daemon went away.
        recv_until(&mut c, |f| matches!(f, Frame::Exited { .. }));
        assert!(eventually(|| d.is_finished()));
        assert!(!path.exists());
    }

    #[test]
    fn stop_with_no_sessions_just_stops() {
        let (d, path) = daemon("stopempty");
        assert!(stop(&path, false).unwrap().stopping);
        assert!(eventually(|| d.is_finished()));
    }

    #[test]
    fn info_reports_the_daemon() {
        let (_d, path) = daemon("info");
        let mut c = connect(&path, Role::Ui);
        create(&mut c, "a");
        let i = info(&path).unwrap();
        assert_eq!(
            (i.pid, i.version.as_str(), i.proto, i.sessions),
            (std::process::id(), env!("CARGO_PKG_VERSION"), PROTO_MAX, 1)
        );
    }

    #[test]
    fn nothing_listening_is_not_running() {
        let path = temp_sock("none");
        assert!(matches!(info(&path), Err(DaemonError::NotRunning(_))));
        assert!(matches!(stop(&path, true), Err(DaemonError::NotRunning(_))));
    }

    #[test]
    fn socket_paths() {
        let home = Path::new("/Users/me");
        assert_eq!(
            socket_path(home),
            PathBuf::from("/Users/me/Library/Application Support/fuxx/fuxxd.sock")
        );
        assert_eq!(
            client_socket_path(Some("/tmp/x.sock"), Some(home)),
            Some(PathBuf::from("/tmp/x.sock"))
        );
        assert_eq!(
            client_socket_path(Some(""), Some(home)),
            Some(socket_path(home))
        );
        assert_eq!(client_socket_path(None, None), None);
    }

    #[test]
    fn theme_round_trips_and_bad_colours_are_refused() {
        let mut theme = Theme::default();
        theme.cursor = Some(theme.palette[3]);
        theme.palette[1] = theme.foreground;
        assert_eq!(theme_from_wire(&theme_to_wire(&theme)), Some(theme));
        assert_eq!(
            theme_from_wire(&serde_json::json!({"background": "nope"})),
            None
        );
        assert_eq!(
            theme_from_wire(&serde_json::json!({"palette": ["#000000"]})),
            None,
            "the palette has 16 colours"
        );
        // Absent keys keep the default.
        assert_eq!(
            theme_from_wire(&serde_json::json!({})),
            Some(Theme::default())
        );
    }

    #[test]
    fn text_scopes() {
        assert_eq!(text_scope("screen"), Some(TextScope::Screen));
        assert_eq!(text_scope("all"), Some(TextScope::All));
        assert_eq!(text_scope("last:5"), Some(TextScope::Last(5)));
        assert_eq!(text_scope("last:x"), None);
        assert_eq!(text_scope("lines"), None);
    }
}
