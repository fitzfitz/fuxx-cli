//! The scripting socket, server side: a private Unix socket in fuxx's Application Support
//! folder that only the same macOS user can use.
//!
//! Plain threads, no GPUI: one accept thread, and per connection a reader thread (lines →
//! `Incoming`, forwarded to the UI) and a writer thread fed by a bounded queue, so a client that
//! never reads can't block whoever replies to it.

use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{SyncSender, sync_channel};
use std::time::Duration;

use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};

use crate::api::{self, ApiError, Request};
use crate::private_socket::{self, Slot};

/// Lines a connection may have waiting to be written before it counts as stuck and is closed.
const QUEUE: usize = 256;

/// The longest request line a client may send (bytes, without the newline).
const MAX_LINE: u64 = 16 << 20;

/// Live connections at most; more are accepted and closed at once.
const MAX_CONNECTIONS: usize = 64;

/// `~/Library/Application Support/fuxx/fuxx.sock`.
pub fn socket_path(home: &Path) -> PathBuf {
    home.join("Library/Application Support/fuxx/fuxx.sock")
}

/// A handle to one client connection; clones share it.
#[derive(Clone)]
pub struct Conn {
    queue: SyncSender<String>,
    /// Lines queued but not yet written.
    pending: Arc<AtomicUsize>,
    open: Arc<AtomicBool>,
    /// The reader saw the client's end close (or stopped reading).
    hung_up: Arc<AtomicBool>,
    stream: Arc<UnixStream>,
}

impl Conn {
    /// Queue one line (the newline is added) for this connection's writer. False once the
    /// connection is gone or its queue is full; the connection is then closed.
    pub fn send_line(&self, line: &str) -> bool {
        if !self.is_open() {
            return false;
        }
        // Counted before queueing, so the writer never takes it below zero.
        self.pending.fetch_add(1, Ordering::AcqRel);
        if self.queue.try_send(line.to_string()).is_ok() {
            return true;
        }
        self.pending.fetch_sub(1, Ordering::AcqRel);
        self.close();
        false
    }

    pub fn is_open(&self) -> bool {
        self.open.load(Ordering::Acquire)
    }

    /// The client is gone: it closed its end (or fuxx stopped reading it), or the connection is
    /// closed. A client that only shuts its sending side still gets replies to what it sent, but
    /// for a long wait or an event stream it counts as gone.
    pub fn peer_gone(&self) -> bool {
        self.hung_up.load(Ordering::Acquire) || !self.is_open()
    }

    /// Wait (blocking, up to `timeout`) until every queued line has been written. False if
    /// lines are still unwritten: the time ran out or the connection is gone.
    pub fn flushed(&self, timeout: Duration) -> bool {
        let deadline = std::time::Instant::now() + timeout;
        loop {
            if self.pending.load(Ordering::Acquire) == 0 {
                return true;
            }
            if !self.is_open() || std::time::Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
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
pub type ServerGuard = private_socket::SocketGuard;

/// Marks a connection's client as gone when its reader thread ends.
struct HangUp(Arc<AtomicBool>);

impl Drop for HangUp {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
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
    let listener = private_socket::bind(path)?;
    let (tx, rx) = unbounded();
    let stop = Arc::new(AtomicBool::new(false));
    let guard = ServerGuard::new(path, stop.clone());
    std::thread::Builder::new()
        .name("fuxx-api-accept".into())
        .spawn(move || {
            let live = Arc::new(AtomicUsize::new(0));
            let open = tx.clone();
            private_socket::accept_loop(
                listener,
                stop,
                live,
                max_connections,
                "scripting",
                move || !open.is_closed(),
                |stream, slot| serve(stream, tx.clone(), slot),
            );
        })?;
    Ok((guard, rx))
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
        pending: Arc::new(AtomicUsize::new(0)),
        open: Arc::new(AtomicBool::new(true)),
        hung_up: Arc::new(AtomicBool::new(false)),
        stream: Arc::new(stream),
    };

    // Ends once every `Conn` is gone and the queue is drained, or on a failed write.
    let (open, socket, pending) = (conn.open.clone(), conn.stream.clone(), conn.pending.clone());
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
                pending.fetch_sub(1, Ordering::AcqRel);
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
            // However reading ends, nothing more comes from this client.
            let _hung_up = HangUp(conn.hung_up.clone());
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
    fn flushed_returns_once_queued_lines_are_written() {
        let path = temp_dir("flush").join("f.sock");
        let (_g, rx) = start(&path).unwrap();
        let mut rx = futures::executor::block_on_stream(rx);
        let mut s = std::os::unix::net::UnixStream::connect(&path).unwrap();
        use std::io::{Read, Write};
        s.write_all(b"{\"method\":\"system.ping\"}\n").unwrap();
        let conn = rx.next().unwrap().conn;
        assert!(conn.flushed(Duration::from_millis(500))); // nothing queued
        // Small enough for the socket's buffer (about 8 KiB on macOS), so nothing waits on us.
        let line = "x".repeat(1000);
        for _ in 0..4 {
            assert!(conn.send_line(&line));
        }
        assert!(conn.flushed(Duration::from_secs(2)));
        // Written means already in the socket: readable without waiting.
        s.set_nonblocking(true).unwrap();
        let mut got = Vec::new();
        let mut buf = [0u8; 8192];
        while let Ok(n) = s.read(&mut buf) {
            if n == 0 {
                break;
            }
            got.extend_from_slice(&buf[..n]);
        }
        assert_eq!(got.len(), 4 * 1001);
    }

    #[test]
    fn peer_gone_flips_when_the_client_hangs_up() {
        let path = temp_dir("gone").join("f.sock");
        let (_g, rx) = start(&path).unwrap();
        let mut rx = futures::executor::block_on_stream(rx);
        let mut s = UnixStream::connect(&path).unwrap();
        use std::io::Write;
        s.write_all(b"{\"method\":\"session.wait\"}\n").unwrap();
        let conn = rx.next().unwrap().conn;
        assert!(!conn.peer_gone());
        drop(s);
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while !conn.peer_gone() && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(conn.peer_gone());
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
