//! The scripting socket, server side: a private Unix socket in fuxx's Application Support
//! folder that only the same macOS user can use.
//!
//! Plain threads, no GPUI: one accept thread, and per connection a reader thread (lines →
//! `Incoming`, forwarded to the UI) and a writer thread fed by a bounded queue, so a client that
//! never reads can't block whoever replies to it.

use std::ffi::c_int;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{SyncSender, sync_channel};
use std::time::Duration;

use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};

use crate::api::{self, ApiError, Request};

/// Lines a connection may have waiting to be written before it counts as stuck and is closed.
const QUEUE: usize = 256;

/// The longest request line a client may send (bytes, without the newline).
const MAX_LINE: u64 = 16 << 20;

/// Live connections at most; more are accepted and closed at once.
const MAX_CONNECTIONS: usize = 64;

// libSystem; `uid_t` is u32 on macOS.
unsafe extern "C" {
    fn getpeereid(fd: c_int, uid: *mut u32, gid: *mut u32) -> c_int;
    fn getuid() -> u32;
}

/// `~/Library/Application Support/fuxx/fuxx.sock`.
pub fn socket_path(home: &Path) -> PathBuf {
    home.join("Library/Application Support/fuxx/fuxx.sock")
}

/// A handle to one client connection; clones share it.
#[derive(Clone)]
pub struct Conn {
    queue: SyncSender<String>,
    open: Arc<AtomicBool>,
    stream: Arc<UnixStream>,
}

impl Conn {
    /// Queue one line (the newline is added) for this connection's writer. False once the
    /// connection is gone or its queue is full; the connection is then closed.
    pub fn send_line(&self, line: &str) -> bool {
        if !self.is_open() {
            return false;
        }
        if self.queue.try_send(line.to_string()).is_ok() {
            return true;
        }
        self.close();
        false
    }

    #[allow(dead_code)] // used by waits and subscribers (M9 Task 6)
    pub fn is_open(&self) -> bool {
        self.open.load(Ordering::Acquire)
    }

    /// Mark closed and shut the socket, which also wakes a reader or writer blocked on it.
    fn close(&self) {
        self.open.store(false, Ordering::Release);
        let _ = self.stream.shutdown(std::net::Shutdown::Both);
    }
}

/// One request from a client, with the connection to answer it on.
pub struct Incoming {
    pub request: Request,
    pub conn: Conn,
}

/// Keeps the server's socket file; removing it on drop (the threads stop on their own).
pub struct ServerGuard {
    path: PathBuf,
    stop: Arc<AtomicBool>,
    /// (device, inode) of the socket we bound, so drop never removes another instance's.
    file_id: (u64, u64),
}

/// A file's (device, inode), without following a symlink.
fn file_id(path: &Path) -> Option<(u64, u64)> {
    std::fs::symlink_metadata(path)
        .ok()
        .map(|m| (m.dev(), m.ino()))
}

impl Drop for ServerGuard {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        // Only if the file is still ours: another fuxx may have replaced it since.
        if file_id(&self.path) == Some(self.file_id) {
            // Wake the accept thread so it sees `stop` and lets go of the listener.
            let _ = UnixStream::connect(&self.path);
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

/// One live connection, counted until its writer thread ends.
struct Slot(Arc<AtomicUsize>);

impl Drop for Slot {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

/// Whether a peer with this uid may connect: only the user fuxx runs as.
fn same_user(peer_uid: u32) -> bool {
    // SAFETY: getuid takes no arguments and always succeeds.
    peer_uid == unsafe { getuid() }
}

/// The uid of the process on the other end, if the system can tell.
fn peer_uid(stream: &UnixStream) -> Option<u32> {
    let (mut uid, mut gid) = (0u32, 0u32);
    // SAFETY: the fd is a live socket owned by `stream` for the whole call, and both
    // out-pointers point at local u32s (macOS uid_t / gid_t).
    let rc = unsafe { getpeereid(stream.as_raw_fd(), &mut uid, &mut gid) };
    (rc == 0).then_some(uid)
}

/// Create the private folder, clear a stale socket, bind, and start accepting. Requests arrive
/// on the returned receiver. `AddrInUse` when something already listens at `path`.
pub fn start(path: &Path) -> std::io::Result<(ServerGuard, UnboundedReceiver<Incoming>)> {
    start_with(path, MAX_CONNECTIONS)
}

/// `start`, with the connection cap as a parameter (tests use a small one).
fn start_with(
    path: &Path,
    max_connections: usize,
) -> std::io::Result<(ServerGuard, UnboundedReceiver<Incoming>)> {
    if let Some(dir) = path.parent() {
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)?;
        // `mode` only applies to folders it creates; an existing one is made private too.
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    }
    let explain = |e: std::io::Error| {
        if e.kind() == std::io::ErrorKind::InvalidInput {
            std::io::Error::new(
                e.kind(),
                format!(
                    "socket path too long ({} bytes; macOS allows about 104): {}",
                    path.as_os_str().len(),
                    path.display()
                ),
            )
        } else {
            e
        }
    };
    // Bind first: only a taken path needs a look. A listener that accepts is in use, even if
    // it's too busy to answer, so a live fuxx never loses its socket. Only a file nobody
    // listens on (refused, or not a socket) is stale: removed, then bound once more.
    let listener = match UnixListener::bind(path) {
        Ok(listener) => listener,
        Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
            if UnixStream::connect(path).is_ok() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::AddrInUse,
                    format!("another fuxx is listening on {}", path.display()),
                ));
            }
            std::fs::remove_file(path)?;
            UnixListener::bind(path).map_err(explain)?
        }
        Err(e) => return Err(explain(e)),
    };
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;

    let (tx, rx) = unbounded();
    let stop = Arc::new(AtomicBool::new(false));
    let guard = ServerGuard {
        path: path.to_path_buf(),
        stop: stop.clone(),
        file_id: file_id(path).unwrap_or_default(),
    };
    std::thread::Builder::new()
        .name("fuxx-api-accept".into())
        .spawn(move || accept_loop(listener, tx, stop, max_connections))?;
    Ok((guard, rx))
}

fn accept_loop(
    listener: UnixListener,
    tx: UnboundedSender<Incoming>,
    stop: Arc<AtomicBool>,
    max_connections: usize,
) {
    let live = Arc::new(AtomicUsize::new(0));
    let mut warned = false;
    for stream in listener.incoming() {
        if stop.load(Ordering::Acquire) || tx.is_closed() {
            return;
        }
        let stream = match stream {
            Ok(s) => s,
            Err(_) => {
                // e.g. out of file descriptors: don't spin.
                std::thread::sleep(Duration::from_millis(50));
                continue;
            }
        };
        // Over the cap: closed at once (a runaway script can't use up fuxx's threads).
        if live.load(Ordering::Acquire) >= max_connections {
            if !warned {
                eprintln!("[fuxx] scripting: {max_connections} connections open; refusing more");
                warned = true;
            }
            continue;
        }
        // Anyone else is dropped unanswered.
        if peer_uid(&stream).is_some_and(same_user) {
            live.fetch_add(1, Ordering::AcqRel);
            serve(stream, tx.clone(), Slot(live.clone()));
        }
    }
}

/// Start the writer and reader threads for one accepted connection. `slot` is released when
/// the writer ends, i.e. when the connection is really gone.
fn serve(stream: UnixStream, tx: UnboundedSender<Incoming>, slot: Slot) {
    let Ok(read_half) = stream.try_clone() else {
        return;
    };
    let (queue, lines) = sync_channel::<String>(QUEUE);
    let conn = Conn {
        queue,
        open: Arc::new(AtomicBool::new(true)),
        stream: Arc::new(stream),
    };

    // Ends once every `Conn` is gone and the queue is drained, or on a failed write.
    let (open, socket) = (conn.open.clone(), conn.stream.clone());
    let writer = std::thread::Builder::new()
        .name("fuxx-api-write".into())
        .spawn(move || {
            let _slot = slot;
            for mut line in lines {
                line.push('\n');
                if (&*socket).write_all(line.as_bytes()).is_err() {
                    open.store(false, Ordering::Release);
                    let _ = socket.shutdown(std::net::Shutdown::Both);
                    return;
                }
            }
        });
    if writer.is_err() {
        return;
    }

    let _ = std::thread::Builder::new()
        .name("fuxx-api-read".into())
        .spawn(move || {
            let mut reader = BufReader::new(read_half);
            let mut buf = Vec::new();
            loop {
                buf.clear();
                // Bounded: a line longer than MAX_LINE stops at MAX_LINE + 1 bytes.
                match (&mut reader).take(MAX_LINE + 1).read_until(b'\n', &mut buf) {
                    Ok(0) | Err(_) => return,
                    Ok(_) => {}
                }
                if buf.last() != Some(&b'\n') && buf.len() as u64 > MAX_LINE {
                    let error = ApiError::bad_request("request line too long");
                    conn.send_line(&api::reply(None, Err(error)));
                    // Read no more; the writer still sends the reply, then the socket closes.
                    let _ = conn.stream.shutdown(std::net::Shutdown::Read);
                    return;
                }
                let line = buf.trim_ascii();
                if line.is_empty() {
                    continue;
                }
                // Not UTF-8 or not JSON alike: `bad_request`, and the connection goes on.
                match serde_json::from_slice::<Request>(line) {
                    Ok(request) => {
                        let incoming = Incoming {
                            request,
                            conn: conn.clone(),
                        };
                        // Nobody is handling requests any more: stop quietly.
                        if tx.unbounded_send(incoming).is_err() {
                            conn.close();
                            return;
                        }
                    }
                    Err(e) => {
                        let error = ApiError::bad_request(format!("not a request: {e}"));
                        conn.send_line(&api::reply(None, Err(error)));
                    }
                }
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{self, Request};
    use crate::api_client;

    /// A fresh, empty folder under the system temp dir, with the default (non-private) mode.
    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("fuxx-sock-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Answer every request with a pong, on a thread, until the server goes away.
    fn pong_forever(rx: futures::channel::mpsc::UnboundedReceiver<Incoming>) {
        std::thread::spawn(move || {
            for incoming in futures::executor::block_on_stream(rx) {
                let id = incoming.request.id;
                incoming
                    .conn
                    .send_line(&api::reply(id, Ok(serde_json::json!({"pong": true}))));
            }
        });
    }

    #[test]
    fn bind_makes_a_private_socket_and_replaces_a_stale_one() {
        let dir = temp_dir("bind");
        let path = dir.join("f.sock");
        std::fs::write(&path, b"stale").unwrap(); // a leftover file, nobody listening
        let (_guard, _rx) = start(&path).unwrap();
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777,
            0o700
        );
    }

    #[test]
    fn a_second_server_on_a_live_socket_is_refused() {
        let path = temp_dir("live").join("f.sock");
        let (_g, rx) = start(&path).unwrap();
        pong_forever(rx);
        assert_eq!(
            start(&path).err().unwrap().kind(),
            std::io::ErrorKind::AddrInUse
        );
    }

    #[test]
    fn a_listener_that_never_replies_is_in_use_and_kept() {
        let path = temp_dir("busy").join("f.sock");
        // Accepts connections (the kernel backlog) but never answers, like a busy UI.
        let _busy = std::os::unix::net::UnixListener::bind(&path).unwrap();
        assert_eq!(
            start(&path).err().unwrap().kind(),
            std::io::ErrorKind::AddrInUse
        );
        assert!(path.exists());
    }

    /// Write `bytes` on a new connection and read one reply line.
    fn send_raw(path: &Path, bytes: &[u8]) -> (UnixStream, String) {
        use std::io::{BufRead, Write};
        let mut s = UnixStream::connect(path).unwrap();
        s.write_all(bytes).unwrap();
        let mut line = String::new();
        std::io::BufReader::new(&s).read_line(&mut line).unwrap();
        (s, line)
    }

    #[test]
    fn an_over_long_line_is_refused_and_closed() {
        let path = temp_dir("long").join("f.sock");
        let (_g, rx) = start(&path).unwrap();
        pong_forever(rx);
        let (s, line) = send_raw(&path, &vec![b'x'; MAX_LINE as usize + 1]);
        assert!(
            line.contains("\"bad_request\"") && line.contains("too long"),
            "{line}"
        );
        // Then the connection ends.
        let mut rest = String::new();
        use std::io::Read;
        assert_eq!((&s).read_to_string(&mut rest).unwrap_or(0), 0);
    }

    #[test]
    fn a_non_utf8_line_is_a_bad_request_and_the_connection_goes_on() {
        let path = temp_dir("utf8").join("f.sock");
        let (_g, rx) = start(&path).unwrap();
        pong_forever(rx);
        let (s, line) = send_raw(&path, b"\xff\xfe{\n");
        assert!(line.contains("\"bad_request\""), "{line}");
        use std::io::{BufRead, Write};
        (&s).write_all(b"{\"id\":2,\"method\":\"system.ping\"}\n")
            .unwrap();
        let mut line = String::new();
        std::io::BufReader::new(&s).read_line(&mut line).unwrap();
        assert!(line.contains("pong"), "{line}");
    }

    #[test]
    fn connections_over_the_cap_are_closed() {
        let path = temp_dir("cap").join("f.sock");
        let (_g, rx) = start_with(&path, 1).unwrap();
        pong_forever(rx);
        let (_first, line) = send_raw(&path, b"{\"id\":1,\"method\":\"system.ping\"}\n");
        assert!(line.contains("pong"), "{line}");
        // The first is still open, so the second is accepted and closed unanswered.
        let mut second = UnixStream::connect(&path).unwrap();
        use std::io::{Read, Write};
        let _ = second.write_all(b"{\"id\":2,\"method\":\"system.ping\"}\n");
        let mut rest = String::new();
        assert_eq!(second.read_to_string(&mut rest).unwrap_or(0), 0);
    }

    #[test]
    fn drop_leaves_another_instances_socket_alone() {
        let path = temp_dir("mine").join("f.sock");
        let (guard, _rx) = start(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        std::fs::write(&path, b"someone else's").unwrap();
        drop(guard);
        assert_eq!(std::fs::read(&path).unwrap(), b"someone else's");
    }

    #[test]
    fn round_trip_and_bad_json() {
        let path = temp_dir("rt").join("f.sock");
        let (_g, rx) = start(&path).unwrap();
        pong_forever(rx);
        let req = Request {
            id: Some(5),
            method: "system.ping".into(),
            params: serde_json::Value::Null,
        };
        assert_eq!(
            api_client::call(&path, &req).unwrap(),
            serde_json::json!({"pong": true})
        );
        let mut s = std::os::unix::net::UnixStream::connect(&path).unwrap();
        use std::io::{BufRead, Write};
        s.write_all(b"not json\n").unwrap();
        let mut line = String::new();
        std::io::BufReader::new(&s).read_line(&mut line).unwrap();
        assert!(line.contains("\"bad_request\""));
    }

    #[test]
    fn peer_check_accepts_only_the_same_user() {
        assert!(same_user(unsafe { getuid() }));
        assert!(!same_user(unsafe { getuid() } + 1));
    }

    #[test]
    fn unreachable_socket_is_exit_3() {
        let err = api_client::call(
            &temp_dir("none").join("x.sock"),
            &Request {
                id: None,
                method: "system.ping".into(),
                params: serde_json::Value::Null,
            },
        )
        .unwrap_err();
        assert!(matches!(err, api_client::ClientError::Unreachable(_)));
    }
}
