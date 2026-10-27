//! The scripting socket, server side: a private Unix socket in fuxx's Application Support
//! folder that only the same macOS user can use.
//!
//! Plain threads, no GPUI: one accept thread, and per connection a reader thread (lines →
//! `Incoming`, forwarded to the UI) and a writer thread fed by a bounded queue, so a client that
//! never reads can't block whoever replies to it.

use std::ffi::c_int;
use std::io::{BufRead, BufReader, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{SyncSender, sync_channel};
use std::time::Duration;

use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};

use crate::api::{self, ApiError, Request};
use crate::api_client;

/// Lines a connection may have waiting to be written before it counts as stuck and is closed.
const QUEUE: usize = 256;

/// How long a leftover socket gets to answer a ping before it counts as stale.
const PING_TIMEOUT: Duration = Duration::from_millis(300);

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
}

impl Drop for ServerGuard {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        // Wake the accept thread so it sees `stop` and lets go of the listener.
        let _ = UnixStream::connect(&self.path);
        let _ = std::fs::remove_file(&self.path);
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
/// on the returned receiver. `AddrInUse` when another fuxx already answers at `path`.
pub fn start(path: &Path) -> std::io::Result<(ServerGuard, UnboundedReceiver<Incoming>)> {
    if let Some(dir) = path.parent() {
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)?;
        // `mode` only applies to folders it creates; an existing one is made private too.
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    }
    if std::fs::symlink_metadata(path).is_ok() {
        if api_client::answers(path, PING_TIMEOUT) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::AddrInUse,
                format!("another fuxx is listening on {}", path.display()),
            ));
        }
        std::fs::remove_file(path)?;
    }
    let listener = UnixListener::bind(path).map_err(|e| {
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
    })?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;

    let (tx, rx) = unbounded();
    let stop = Arc::new(AtomicBool::new(false));
    let guard = ServerGuard {
        path: path.to_path_buf(),
        stop: stop.clone(),
    };
    std::thread::Builder::new()
        .name("fuxx-api-accept".into())
        .spawn(move || accept_loop(listener, tx, stop))?;
    Ok((guard, rx))
}

fn accept_loop(listener: UnixListener, tx: UnboundedSender<Incoming>, stop: Arc<AtomicBool>) {
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
        // Anyone else is dropped unanswered.
        if peer_uid(&stream).is_some_and(same_user) {
            serve(stream, tx.clone());
        }
    }
}

/// Start the writer and reader threads for one accepted connection.
fn serve(stream: UnixStream, tx: UnboundedSender<Incoming>) {
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
            for line in BufReader::new(read_half).lines() {
                let Ok(line) = line else { return };
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                match serde_json::from_str::<Request>(line) {
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
