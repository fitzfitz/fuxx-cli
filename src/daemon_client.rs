//! M10a: the GUI's side of the daemon socket (study §2.3.2, §2.3.5). No GPUI.
//!
//! `DaemonClient::connect_or_spawn` connects to `fuxxd.sock`, starting `fuxx daemon run`
//! detached when nothing answers. Once connected:
//! - one reader thread decodes frames. Session frames (`Snapshot`, `Output`, `Resized`,
//!   `Status`, `Title`, `Bell`, `Exited`, `Resync`) go to that session's stream (`attach`).
//!   Replies (`Sessions`, `Created`, `Text`, `Error`) go to the waiting request: the daemon
//!   answers in the order it was asked, so requests queue in send order. The reader never
//!   waits on anything but the socket;
//! - one writer thread drains a bounded queue of encoded frames onto the socket. Senders wait
//!   while it is full (the daemon pushes back on input a child doesn't read).

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::ffi::c_int;
use std::io::{self, ErrorKind, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, SyncSender};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use crate::daemon;
use crate::detach_probe;
use crate::mux_proto::{
    self, Create, Decoder, Frame, Hello, HelloOk, PROTO_MAX, PROTO_MIN, Role, SessionInfo, SetMeta,
};
use crate::pty::ShellEnv;
use crate::terminal::{CellPx, TermDims};
use crate::theme::Theme;

/// How long `connect_or_spawn` keeps trying after starting the daemon.
pub const SPAWN_WAIT: Duration = Duration::from_secs(2);

/// Whether the daemon is started with the private "disclaim responsibility" spawn attribute.
/// M10a Task 0's probe decides; plain `setsid` until then (ledger ruling).
const DISCLAIM: bool = false;

/// How long the daemon has to answer `Hello`.
const HANDSHAKE: Duration = Duration::from_secs(5);

/// How long a request (`List`, `Create`, `ReadText`) waits for its reply.
const REQUEST: Duration = Duration::from_secs(10);

/// Encoded frames that may wait for the writer; senders then wait.
const OUT_QUEUE: usize = 256;

/// Input is sent in frames of at most this many bytes.
const INPUT_CHUNK: usize = 64 * 1024;

/// Between connect attempts while the daemon starts.
const RETRY: Duration = Duration::from_millis(25);
/// Daemons started per `connect_or_spawn` at most (a new one only after the last one exited).
const SPAWNS: usize = 3;

const VERSION: &str = env!("CARGO_PKG_VERSION");

macro_rules! log {
    ($($arg:tt)*) => {
        eprintln!("[fuxx] daemon client: {}", format_args!($($arg)*))
    };
}

/// One frame of a session's stream: what `attach` hands back.
pub type SessionStream = Receiver<Frame>;

/// The running daemon speaks no protocol version this fuxx does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Incompatible {
    /// The daemon's `proto_min..=proto_max`.
    pub daemon: (u32, u32),
}

impl std::fmt::Display for Incompatible {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "the running fuxx daemon speaks protocol {}..={} and this fuxx {PROTO_MIN}..={PROTO_MAX}; \
             end it with `fuxx daemon stop` (its sessions end too), then try again",
            self.daemon.0, self.daemon.1
        )
    }
}

impl std::error::Error for Incompatible {}

/// The `Incompatible` inside `e`, if that is why connecting failed.
#[allow(dead_code)] // tests; the GUI shows the error's own text
pub fn incompatible(e: &io::Error) -> Option<&Incompatible> {
    e.get_ref()?.downcast_ref()
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The reply a request waits for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Expect {
    Sessions,
    Created,
    Text(u64),
}

impl Expect {
    /// Whether `frame` is this request's reply. An `Error` answers a `Create` (no id) or a
    /// `ReadText` (its session's id).
    ///
    /// Limitation: the daemon also sends id-less `Error`s for a bad `SetTheme` palette or a
    /// second `Hello`, and those would be taken as the pending `Create`'s reply. This client
    /// never sends either (`set_theme` always sends a valid palette).
    fn answered_by(self, frame: &Frame) -> bool {
        match (self, frame) {
            (Expect::Sessions, Frame::Sessions(_)) | (Expect::Created, Frame::Created { .. }) => {
                true
            }
            (Expect::Text(id), Frame::Text(t)) => t.id == id,
            (Expect::Created, Frame::Error { id: None, .. }) => true,
            (Expect::Text(id), Frame::Error { id: Some(e), .. }) => *e == id,
            _ => false,
        }
    }
}

struct Pending {
    expect: Expect,
    reply: Sender<Frame>,
}

/// What the reader thread routes to.
#[derive(Default)]
struct Routes {
    /// Per attached session, its stream and the token `route` gave it.
    sessions: HashMap<u64, (u64, Sender<Frame>)>,
    /// The next token.
    next_token: u64,
    /// Requests waiting for a reply, in the order they were sent.
    pending: VecDeque<Pending>,
    /// The connection is gone: nothing more will be routed.
    closed: bool,
}

/// The GUI's connection to the daemon. Share it (`Arc`); dropping the last one closes it
/// (after the writer has sent what was queued, e.g. the `Detach`es of closing views).
pub struct DaemonClient {
    out: SyncSender<Vec<u8>>,
    routes: Arc<Mutex<Routes>>,
    /// Held while queueing a request and its `pending` entry, so both are in the same order.
    requests: Mutex<()>,
    stream: Arc<UnixStream>,
    hello: HelloOk,
}

impl DaemonClient {
    /// Connect to the daemon at `socket`; when nothing answers, start `exe daemon run` detached
    /// and keep trying for `SPAWN_WAIT`.
    pub fn connect_or_spawn(socket: &Path, exe: &Path) -> io::Result<Arc<Self>> {
        let home = std::env::var_os("HOME")
            .filter(|h| !h.is_empty())
            .map(PathBuf::from)
            .ok_or_else(|| io::Error::new(ErrorKind::NotFound, "$HOME is unset"))?;
        Self::connect_or_spawn_with(socket, exe, &daemon::log_path(&home), SPAWN_WAIT)
    }

    /// `connect_or_spawn` with the log file and the wait spelled out (tests).
    pub fn connect_or_spawn_with(
        socket: &Path,
        exe: &Path,
        log: &Path,
        wait: Duration,
    ) -> io::Result<Arc<Self>> {
        match Self::connect(socket) {
            Err(e) if respawnable(&e) => {}
            done => return done,
        }
        let deadline = Instant::now() + wait;
        let mut spawns = 0;
        let mut started: Option<Arc<AtomicBool>> = None;
        loop {
            // Again only once the last one has exited: one started while an old daemon was
            // still exiting found the socket taken and quit. One still starting is waited for.
            if spawns < SPAWNS
                && started
                    .as_ref()
                    .is_none_or(|exited| exited.load(Ordering::Acquire))
            {
                started = Some(spawn(exe, socket, log)?);
                spawns += 1;
            }
            std::thread::sleep(RETRY);
            match Self::connect(socket) {
                Err(e) if respawnable(&e) && Instant::now() < deadline => {}
                done => return done,
            }
        }
    }

    /// Connect to a daemon already listening at `socket` and say `Hello` as the UI.
    pub fn connect(socket: &Path) -> io::Result<Arc<Self>> {
        let mut stream = UnixStream::connect(socket)?;
        let (hello, decoder) = handshake(&mut stream)?;
        let stream = Arc::new(stream);
        let routes = Arc::new(Mutex::new(Routes::default()));
        let (out, frames) = mpsc::sync_channel::<Vec<u8>>(OUT_QUEUE);
        let writing = stream.clone();
        std::thread::Builder::new()
            .name("fuxx-client-write".into())
            .spawn(move || write_loop(&writing, &frames))?;
        let (reading, routing) = (stream.clone(), routes.clone());
        std::thread::Builder::new()
            .name("fuxx-client-read".into())
            .spawn(move || read_loop(&reading, decoder, &routing))?;
        Ok(Arc::new(Self {
            out,
            routes,
            requests: Mutex::new(()),
            stream,
            hello,
        }))
    }

    /// The daemon's process id.
    pub fn pid(&self) -> u32 {
        self.hello.pid
    }

    /// Every session the daemon has.
    pub fn list(&self) -> io::Result<Vec<SessionInfo>> {
        match self.request(&Frame::List, Expect::Sessions)? {
            Frame::Sessions(sessions) => Ok(sessions),
            other => Err(unexpected(&other)),
        }
    }

    /// `create` on its own thread, for callers that must not wait (the UI thread): the id
    /// arrives on the returned channel (`.await` it).
    pub fn create_in_background(
        self: &Arc<Self>,
        create: Create,
    ) -> futures::channel::oneshot::Receiver<io::Result<u64>> {
        let (done, id) = futures::channel::oneshot::channel();
        let client = self.clone();
        let spawned = std::thread::Builder::new()
            .name("fuxx-daemon-create".into())
            .spawn(move || {
                let _ = done.send(client.create(create));
            });
        if let Err(e) = spawned {
            log!("no thread to create a session: {e}");
        }
        id
    }

    /// Start a session; its id. Waits for the daemon: not on the UI thread
    /// (`create_in_background`).
    pub fn create(&self, create: Create) -> io::Result<u64> {
        match self.request(&Frame::Create(create), Expect::Created)? {
            Frame::Created { id } => Ok(id),
            other => Err(unexpected(&other)),
        }
    }

    /// Start receiving session `id`'s frames: a `Snapshot`, then everything after it. Asking
    /// again (after a `Resync` or a gap) brings a new `Snapshot` on the same stream. One stream
    /// per session: attaching again replaces it.
    #[allow(dead_code)] // tests; views use `route` + `reattach`
    pub fn attach(&self, id: u64) -> (u64, SessionStream) {
        let route = self.route(id);
        self.reattach(id);
        route
    }

    /// Register session `id`'s stream without asking for it yet (no socket I/O, so fine on
    /// the UI thread); `reattach` asks. Returns the stream's token for `detach`.
    pub fn route(&self, id: u64) -> (u64, SessionStream) {
        let (tx, rx) = mpsc::channel();
        let mut routes = lock(&self.routes);
        routes.next_token += 1;
        let token = routes.next_token;
        if !routes.closed {
            routes.sessions.insert(id, (token, tx));
        }
        (token, rx)
    }

    /// Ask for a fresh `Snapshot` on session `id`'s existing stream.
    pub fn reattach(&self, id: u64) {
        self.send(&Frame::Attach { id });
    }

    /// Stop session `id`'s stream `token`; the session keeps running. A stale token (the
    /// stream was replaced by a later `attach`) does nothing.
    pub fn detach(&self, id: u64, token: u64) {
        {
            let mut routes = lock(&self.routes);
            match routes.sessions.get(&id) {
                Some((current, _)) if *current == token => {
                    routes.sessions.remove(&id);
                }
                _ => return,
            }
        }
        self.send(&Frame::Detach { id });
    }

    /// Bytes for session `id`'s program (keystrokes, paste, mouse reports).
    pub fn input(&self, id: u64, bytes: &[u8]) {
        for chunk in bytes.chunks(INPUT_CHUNK) {
            self.send(&Frame::Input {
                id,
                bytes: chunk.to_vec(),
            });
        }
    }

    /// Bytes for session `id`'s program that aren't typing (mouse reports): the daemon's
    /// status tracker doesn't count them.
    pub fn write(&self, id: u64, bytes: &[u8]) {
        for chunk in bytes.chunks(INPUT_CHUNK) {
            self.send(&Frame::Write {
                id,
                bytes: chunk.to_vec(),
            });
        }
    }

    pub fn resize(&self, id: u64, dims: TermDims, cell: CellPx) {
        let cells = |n: usize| u16::try_from(n).unwrap_or(u16::MAX);
        self.send(&Frame::Resize(mux_proto::Resize {
            id,
            cols: cells(dims.cols),
            rows: cells(dims.rows),
            cell_w: cell.width,
            cell_h: cell.height,
        }));
    }

    /// The colours the daemon answers OSC 4/10/11/12 queries with, for every session.
    pub fn set_theme(&self, theme: &Theme) {
        self.send(&Frame::SetTheme {
            palette: daemon::theme_to_wire(theme),
        });
    }

    #[allow(dead_code)] // for scripting while the window is closed (a later step)
    pub fn set_meta(&self, meta: SetMeta) {
        self.send(&Frame::SetMeta(meta));
    }

    /// End session `id` (its stream then gets `Exited`).
    pub fn close(&self, id: u64) {
        self.send(&Frame::Close { id });
    }

    /// The daemon's text of session `id`: `screen`, `all` or `last:N`.
    #[allow(dead_code)] // for scripting while the window is closed (a later step)
    pub fn read_text(&self, id: u64, scope: &str) -> io::Result<String> {
        let read = mux_proto::ReadText {
            id,
            scope: scope.into(),
        };
        match self.request(&Frame::ReadText(read), Expect::Text(id))? {
            Frame::Text(t) => Ok(t.text),
            other => Err(unexpected(&other)),
        }
    }

    /// Queue `frame` for the writer, waiting while the queue is full. False once the
    /// connection is gone.
    fn send(&self, frame: &Frame) -> bool {
        self.out.send(mux_proto::encode(frame)).is_ok()
    }

    /// Send `frame` and wait for its reply (an `Error` reply is returned as `Err`).
    fn request(&self, frame: &Frame, expect: Expect) -> io::Result<Frame> {
        let (reply, answer) = mpsc::channel();
        {
            let _order = lock(&self.requests);
            {
                let mut routes = lock(&self.routes);
                if routes.closed {
                    return Err(gone());
                }
                routes.pending.push_back(Pending { expect, reply });
            }
            if !self.send(frame) {
                return Err(gone());
            }
        }
        match answer.recv_timeout(REQUEST) {
            Ok(Frame::Error { message, .. }) => Err(io::Error::other(message)),
            Ok(frame) => Ok(frame),
            Err(mpsc::RecvTimeoutError::Timeout) => Err(io::Error::new(
                ErrorKind::TimedOut,
                "the fuxx daemon didn't answer",
            )),
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(gone()),
        }
    }
}

impl Drop for DaemonClient {
    fn drop(&mut self) {
        // The writer ends once `out` is dropped, after writing what is queued, and shuts the
        // socket; the reader then sees EOF. Shut reading now so the reader doesn't wait for it.
        let _ = self.stream.shutdown(std::net::Shutdown::Read);
    }
}

fn gone() -> io::Error {
    io::Error::new(ErrorKind::BrokenPipe, "lost the fuxx daemon")
}

fn unexpected(frame: &Frame) -> io::Error {
    io::Error::other(format!("unexpected reply from the fuxx daemon: {frame:?}"))
}

/// Say `Hello` and read the answer. EOF here means the daemon is going away (respawnable).
fn handshake(stream: &mut UnixStream) -> io::Result<(HelloOk, Decoder)> {
    // macOS refuses socket options (EINVAL) once the peer has hung up: that is EOF too.
    let hung_up = |e: io::Error| match e.kind() {
        ErrorKind::InvalidInput => io::Error::new(
            ErrorKind::UnexpectedEof,
            "the fuxx daemon closed the connection during the handshake",
        ),
        _ => e,
    };
    stream.set_read_timeout(Some(HANDSHAKE)).map_err(hung_up)?;
    let hello = Frame::Hello(Hello {
        proto_min: PROTO_MIN,
        proto_max: PROTO_MAX,
        app_version: VERSION.into(),
        role: Role::Ui,
    });
    stream.write_all(&mux_proto::encode(&hello))?;
    let deadline = Instant::now() + HANDSHAKE;
    let mut decoder = Decoder::new();
    let mut buf = [0u8; 4096];
    loop {
        if let Some(frame) = decoder.next() {
            return match frame.map_err(io::Error::other)? {
                Frame::HelloOk(ok) => {
                    stream.set_read_timeout(None).map_err(hung_up)?;
                    Ok((ok, decoder))
                }
                Frame::Incompatible(i) => Err(io::Error::new(
                    ErrorKind::Unsupported,
                    Incompatible {
                        daemon: (i.proto_min, i.proto_max),
                    },
                )),
                other => Err(io::Error::other(format!(
                    "unexpected handshake reply from the fuxx daemon: {other:?}"
                ))),
            };
        }
        let timed_out =
            || io::Error::new(ErrorKind::TimedOut, "the fuxx daemon didn't answer Hello");
        if Instant::now() >= deadline {
            return Err(timed_out());
        }
        match stream.read(&mut buf) {
            Ok(0) => return Err(hung_up(io::Error::from(ErrorKind::InvalidInput))),
            Ok(n) => decoder.push(&buf[..n]),
            Err(e) if e.kind() == ErrorKind::Interrupted => {}
            Err(e) if e.kind() == ErrorKind::WouldBlock => return Err(timed_out()),
            Err(e) => return Err(e),
        }
    }
}

/// Write queued frames until every sender is gone or the socket fails, then shut the socket.
fn write_loop(stream: &UnixStream, frames: &Receiver<Vec<u8>>) {
    for bytes in frames {
        if (&*stream).write_all(&bytes).is_err() {
            break;
        }
    }
    let _ = stream.shutdown(std::net::Shutdown::Both);
}

/// Decode frames and route them until the socket ends. Never waits on anything else: every
/// route is an unbounded channel.
fn read_loop(stream: &UnixStream, mut decoder: Decoder, routes: &Mutex<Routes>) {
    let mut buf = vec![0u8; 64 * 1024];
    'read: loop {
        while let Some(result) = decoder.next() {
            match result {
                Ok(frame) => route(routes, frame),
                Err(e @ mux_proto::ProtoError::BadLength(_)) => {
                    log!("{e}; closing the connection");
                    break 'read;
                }
                Err(e) => log!("skipped a frame: {e}"),
            }
        }
        match (&*stream).read(&mut buf) {
            Ok(0) => break,
            Ok(n) => decoder.push(&buf[..n]),
            Err(e) if e.kind() == ErrorKind::Interrupted => {}
            Err(_) => break,
        }
    }
    // Every stream and waiting request ends: their receivers see the disconnect.
    let mut routes = lock(routes);
    routes.closed = true;
    routes.sessions.clear();
    routes.pending.clear();
    drop(routes);
    let _ = stream.shutdown(std::net::Shutdown::Both);
}

/// The session a daemon-to-client frame is about, for the ones that go to a session's stream.
fn session_of(frame: &Frame) -> Option<u64> {
    match frame {
        Frame::Snapshot(s) => Some(s.id),
        Frame::Resized(r) => Some(r.id),
        Frame::Output { id, .. }
        | Frame::Status { id, .. }
        | Frame::Title { id, .. }
        | Frame::Bell { id }
        | Frame::Exited { id }
        | Frame::Resync { id } => Some(*id),
        _ => None,
    }
}

fn route(routes: &Mutex<Routes>, frame: Frame) {
    let mut routes = lock(routes);
    if let Some(id) = session_of(&frame) {
        let exited = matches!(frame, Frame::Exited { .. });
        if let Some((_, stream)) = routes.sessions.get(&id)
            && (stream.send(frame).is_err() || exited)
        {
            // Nothing more comes after Exited; a dropped receiver wants nothing more.
            routes.sessions.remove(&id);
        }
        return;
    }
    match frame {
        Frame::Sessions(_) | Frame::Created { .. } | Frame::Text(_) | Frame::Error { .. } => {
            if routes
                .pending
                .front()
                .is_some_and(|p| p.expect.answered_by(&frame))
            {
                let pending = routes.pending.pop_front().expect("checked");
                let _ = pending.reply.send(frame); // the requester may have timed out
            } else if let Frame::Error { id, message } = frame {
                log!("daemon error (session {id:?}): {message}");
            } else {
                log!("a reply nobody asked for: {frame:?}");
            }
        }
        // `DaemonInfo`, `StopReply` (never asked here), and kinds from a newer daemon.
        _ => {}
    }
}

/// Start `exe daemon run` detached (its own session, output to `log`) for `socket`, and reap
/// it when it exits so it never lingers as a zombie of this process. The flag turns true then.
fn spawn(exe: &Path, socket: &Path, log: &Path) -> io::Result<Arc<AtomicBool>> {
    let env = [("FUXX_DAEMON_SOCKET", socket.as_os_str())];
    let (pid, note) = detach_probe::spawn_detached(exe, &["daemon", "run"], log, &env, DISCLAIM)
        .map_err(io::Error::other)?;
    if let Some(note) = note {
        log!("{note}");
    }
    log!("started the fuxx daemon (pid {pid})");
    let exited = Arc::new(AtomicBool::new(false));
    let reaped = exited.clone();
    let _ = std::thread::Builder::new()
        .name("fuxx-daemon-reap".into())
        .spawn(move || {
            let mut status: c_int = 0;
            // SAFETY: `pid` is our own child; `status` is a valid out pointer.
            while unsafe { waitpid(pid, &mut status, 0) } == -1
                && io::Error::last_os_error().kind() == ErrorKind::Interrupted
            {}
            reaped.store(true, Ordering::Release);
        });
    Ok(exited)
}

// libSystem.
unsafe extern "C" {
    fn waitpid(pid: c_int, status: *mut c_int, options: c_int) -> c_int;
}

/// The environment for a new daemon session: ours with `shell` applied (the daemon never uses
/// its own, study §2.3.7 trap 2).
pub fn create_env(shell: &ShellEnv) -> BTreeMap<String, String> {
    let mut env: BTreeMap<String, String> = std::env::vars_os()
        .map(|(k, v)| {
            (
                k.to_string_lossy().into_owned(),
                v.to_string_lossy().into_owned(),
            )
        })
        .collect();
    for key in &shell.remove {
        env.remove(*key);
    }
    for (key, value) in &shell.set {
        env.insert(key.clone(), value.clone());
    }
    env
}

/// Whether a failed connect means "no live daemon here, start one": nothing at the path, a
/// stale socket file, or a daemon that hung up during the handshake (it was exiting).
fn respawnable(e: &io::Error) -> bool {
    matches!(
        e.kind(),
        ErrorKind::NotFound
            | ErrorKind::ConnectionRefused
            | ErrorKind::UnexpectedEof
            | ErrorKind::ConnectionReset
            | ErrorKind::BrokenPipe
    )
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::daemon::{self, Running};
    use crate::mux_proto::{CellPx as WireCell, Decoder, Dims, encode};
    use std::io::{Read, Write};
    use std::os::unix::net::UnixListener;
    use std::path::PathBuf;
    use std::time::Instant;

    /// A fresh socket path in its own temp folder.
    pub(crate) fn temp_sock(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("fuxxc-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("d.sock")
    }

    /// An in-process daemon at a temp socket.
    pub(crate) fn daemon(name: &str) -> (Running, PathBuf) {
        let path = temp_sock(name);
        (daemon::start(&path, daemon::IDLE_EXIT).unwrap(), path)
    }

    /// `Create` for `/bin/sh` with a `$ ` prompt: quick and the same everywhere.
    pub(crate) fn sh_create(cols: u16, rows: u16) -> Create {
        let mut env: BTreeMap<String, String> = std::env::vars().collect();
        env.insert("SHELL".into(), "/bin/sh".into());
        env.insert("PS1".into(), "$ ".into());
        Create {
            cwd: None,
            startup: None,
            env,
            dims: Dims { cols, rows },
            cell_px: WireCell { w: 8, h: 16 },
            name: None,
            project: None,
            preset: None,
        }
    }

    /// Frames from `stream` until `stop` matches; panics after 5 s.
    fn recv_until(stream: &SessionStream, mut stop: impl FnMut(&Frame) -> bool) -> Vec<Frame> {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut frames = Vec::new();
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match stream.recv_timeout(left) {
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

    #[test]
    fn create_list_attach_read_and_close() {
        let (_d, path) = daemon("basic");
        let client = DaemonClient::connect(&path).unwrap();
        let id = client.create(sh_create(40, 5)).unwrap();
        assert_eq!(id, 1);
        assert_eq!(
            client
                .list()
                .unwrap()
                .iter()
                .map(|s| s.id)
                .collect::<Vec<_>>(),
            [1]
        );
        let (_, stream) = client.attach(id);
        let first = stream.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(
            matches!(first, Frame::Snapshot(ref s) if s.id == id),
            "{first:?}"
        );
        client.input(id, b"echo $((6*7))X\r");
        recv_until(
            &stream,
            |f| matches!(f, Frame::Output { bytes, .. } if String::from_utf8_lossy(bytes).contains("42X")),
        );
        let deadline = Instant::now() + Duration::from_secs(5);
        while !client.read_text(id, "screen").unwrap().contains("42X") {
            assert!(Instant::now() < deadline, "ReadText never showed it");
            std::thread::sleep(Duration::from_millis(20));
        }
        client.close(id);
        recv_until(&stream, |f| matches!(f, Frame::Exited { id: 1 }));
        // The route ends with Exited.
        assert!(stream.recv_timeout(Duration::from_secs(5)).is_err());
    }

    #[test]
    fn a_failed_request_keeps_later_replies_in_step() {
        let (_d, path) = daemon("errors");
        let client = DaemonClient::connect(&path).unwrap();
        // Zero size: the daemon refuses with an id-less Error.
        assert!(client.create(sh_create(0, 0)).is_err());
        assert!(client.read_text(99, "screen").is_err()); // no such session
        let id = client.create(sh_create(40, 5)).unwrap();
        assert!(client.read_text(id, "nonsense").is_err()); // bad scope
        assert_eq!(client.list().unwrap()[0].id, id);
        assert!(client.read_text(id, "screen").is_ok());
    }

    #[test]
    fn requests_from_many_threads_get_their_own_replies() {
        let (_d, path) = daemon("threads");
        let client = DaemonClient::connect(&path).unwrap();
        let a = client.create(sh_create(40, 5)).unwrap();
        let b = client.create(sh_create(40, 5)).unwrap();
        for (id, word) in [(a, "alpha"), (b, "bravo")] {
            client.input(id, format!("echo {word}{word}\r").as_bytes());
        }
        std::thread::scope(|s| {
            for _ in 0..4 {
                s.spawn(|| {
                    for _ in 0..20 {
                        assert_eq!(client.list().unwrap().len(), 2);
                        let deadline = Instant::now() + Duration::from_secs(5);
                        loop {
                            let ta = client.read_text(a, "screen").unwrap();
                            let tb = client.read_text(b, "screen").unwrap();
                            assert!(!ta.contains("bravobravo") && !tb.contains("alphaalpha"));
                            if ta.contains("alphaalpha") && tb.contains("bravobravo") {
                                break;
                            }
                            assert!(Instant::now() < deadline);
                        }
                    }
                });
            }
        });
    }

    /// A listener that answers the first connection's Hello with `reply`.
    fn fake_daemon(path: &Path, reply: Option<Frame>) -> std::thread::JoinHandle<()> {
        let listener = UnixListener::bind(path).unwrap();
        std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let mut buf = [0u8; 4096];
            let mut decoder = Decoder::new();
            while decoder.next().is_none() {
                let n = s.read(&mut buf).unwrap();
                decoder.push(&buf[..n]);
            }
            if let Some(reply) = reply {
                s.write_all(&encode(&reply)).unwrap();
            }
            // Dropping `s` closes it: with no reply that is EOF in the handshake.
        })
    }

    #[test]
    fn an_incompatible_daemon_is_a_clear_error() {
        let path = temp_sock("incompat");
        let fake = fake_daemon(
            &path,
            Some(Frame::Incompatible(crate::mux_proto::IncompatibleInfo {
                proto_min: 7,
                proto_max: 9,
            })),
        );
        let err = DaemonClient::connect(&path).err().expect("must fail");
        fake.join().unwrap();
        assert_eq!(
            incompatible(&err),
            Some(&Incompatible { daemon: (7, 9) }),
            "{err:?}"
        );
        assert!(!respawnable(&err));
        let text = err.to_string();
        assert!(
            text.contains("7..=9") && text.contains("fuxx daemon stop"),
            "{text}"
        );
    }

    #[test]
    fn eof_in_the_handshake_means_start_a_daemon() {
        let path = temp_sock("eof");
        let fake = fake_daemon(&path, None);
        let err = DaemonClient::connect(&path).err().expect("must fail");
        fake.join().unwrap();
        assert!(respawnable(&err), "{err:?}");
        let missing = DaemonClient::connect(&temp_sock("missing")).err().unwrap();
        assert!(respawnable(&missing), "{missing:?}");
        std::fs::remove_file(&path).unwrap();
        std::os::unix::net::UnixListener::bind(&path)
            .map(drop)
            .unwrap(); // stale socket file
        let refused = DaemonClient::connect(&path).err().unwrap();
        assert!(respawnable(&refused), "{refused:?}");
    }

    #[test]
    fn nothing_listening_starts_the_daemon_detached_then_connects() {
        let path = temp_sock("spawn");
        let dir = path.parent().unwrap().to_path_buf();
        let log = dir.join("logs/daemon.log"); // the folder doesn't exist yet
        let marker = dir.join("started");
        // Stands in for `fuxx`: records how it was started; the test then starts the daemon.
        let exe = dir.join("fake-fuxx");
        std::fs::write(
            &exe,
            format!(
                "#!/bin/sh\necho \"args=$*\"\necho \"to stderr\" >&2\n\
                 echo \"$$ $(ps -o pgid= -p $$) $FUXX_DAEMON_SOCKET\" > '{0}.$$'\n\
                 mv '{0}.$$' '{0}'\n",
                marker.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&exe, std::os::unix::fs::PermissionsExt::from_mode(0o755))
            .unwrap();
        let starter = {
            let (marker, path) = (marker.clone(), path.clone());
            std::thread::spawn(move || {
                let deadline = Instant::now() + SPAWN_WAIT;
                while !marker.exists() {
                    assert!(Instant::now() < deadline, "the daemon was never started");
                    std::thread::sleep(Duration::from_millis(10));
                }
                daemon::start(&path, daemon::IDLE_EXIT).unwrap()
            })
        };
        let client = DaemonClient::connect_or_spawn_with(&path, &exe, &log, SPAWN_WAIT).unwrap();
        let _running = starter.join().unwrap();
        assert!(client.list().unwrap().is_empty());
        let started = std::fs::read_to_string(&marker).unwrap();
        let fields: Vec<&str> = started.split_whitespace().collect();
        assert_eq!(fields[0], fields[1], "not its own process group: {started}");
        assert_eq!(fields[2], path.to_str().unwrap());
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let logged = std::fs::read_to_string(&log).unwrap_or_default();
            if logged.contains("args=daemon run") && logged.contains("to stderr") {
                break;
            }
            assert!(Instant::now() < deadline, "log: {logged:?}");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn a_running_daemon_is_used_as_is() {
        let (_d, path) = daemon("running");
        let exe = Path::new("/nonexistent/fuxx"); // would fail if it were started
        let log = path.parent().unwrap().join("daemon.log");
        let client = DaemonClient::connect_or_spawn_with(&path, exe, &log, SPAWN_WAIT).unwrap();
        assert!(client.list().unwrap().is_empty());
        assert!(!log.exists());
    }

    #[test]
    fn a_stale_detach_leaves_a_newer_stream_alone() {
        let (_d, path) = daemon("tokens");
        let client = DaemonClient::connect(&path).unwrap();
        let id = client.create(sh_create(40, 5)).unwrap();
        let (first, _old) = client.attach(id);
        let (second, stream) = client.attach(id);
        assert_ne!(first, second);
        client.detach(id, first); // e.g. the first view going away late
        recv_until(&stream, |f| matches!(f, Frame::Snapshot(_)));
        client.input(id, b"echo $((6*7))T\r");
        recv_until(
            &stream,
            |f| matches!(f, Frame::Output { bytes, .. } if String::from_utf8_lossy(bytes).contains("42T")),
        );
    }

    #[test]
    fn create_in_background_hands_back_the_id() {
        let (_d, path) = daemon("bgcreate");
        let client = DaemonClient::connect(&path).unwrap();
        let id = futures::executor::block_on(client.create_in_background(sh_create(40, 5)));
        assert_eq!(id.unwrap().unwrap(), 1);
        let failed = futures::executor::block_on(client.create_in_background(sh_create(0, 0)));
        assert!(failed.unwrap().is_err());
    }

    #[test]
    fn create_env_applies_the_shell_env_to_ours() {
        let env = create_env(&ShellEnv {
            set: vec![("FUXX_TEST_SET".into(), "1".into())],
            remove: vec!["HOME"],
        });
        assert_eq!(env.get("FUXX_TEST_SET").map(String::as_str), Some("1"));
        assert!(!env.contains_key("HOME"));
        assert_eq!(env.get("PATH"), std::env::var("PATH").ok().as_ref());
    }

    #[test]
    fn dropping_the_client_ends_its_streams() {
        let (_d, path) = daemon("drop");
        let client = DaemonClient::connect(&path).unwrap();
        let id = client.create(sh_create(40, 5)).unwrap();
        let (_, stream) = client.attach(id);
        recv_until(&stream, |f| matches!(f, Frame::Snapshot(_)));
        let other = client.clone();
        drop(client);
        assert!(other.list().is_ok(), "a clone keeps it open");
        drop(other);
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match stream.recv_timeout(Duration::from_millis(50)) {
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                _ => assert!(Instant::now() < deadline, "stream never ended"),
            }
        }
    }
}
