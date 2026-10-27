//! Private Unix sockets, shared by the scripting socket (M9, `api_server`) and the background
//! daemon (M10a, `daemon`): a 0600 socket in a 0700 folder, a `getpeereid` same-user check on
//! every connection, a connection cap, and a guard that removes the socket file only if it is
//! still ours. No GPUI.

use std::ffi::c_int;
use std::io;
use std::os::fd::AsRawFd;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

// libSystem; `uid_t` is u32 on macOS.
unsafe extern "C" {
    fn getpeereid(fd: c_int, uid: *mut u32, gid: *mut u32) -> c_int;
    fn getuid() -> u32;
}

/// Create the private folder, clear a stale socket, bind, and make the socket 0600.
/// `AddrInUse` when something already listens at `path`.
pub fn bind(path: &Path) -> io::Result<UnixListener> {
    if let Some(dir) = path.parent() {
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)?;
        // `mode` only applies to folders it creates; an existing one is made private too.
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    }
    let explain = |e: io::Error| {
        if e.kind() == io::ErrorKind::InvalidInput {
            io::Error::new(
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
        Err(e) if e.kind() == io::ErrorKind::AddrInUse => {
            if UnixStream::connect(path).is_ok() {
                return Err(io::Error::new(
                    io::ErrorKind::AddrInUse,
                    format!("another fuxx is listening on {}", path.display()),
                ));
            }
            std::fs::remove_file(path)?;
            UnixListener::bind(path).map_err(explain)?
        }
        Err(e) => return Err(explain(e)),
    };
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    Ok(listener)
}

/// Keeps a server's socket file; on drop it raises `stop` and removes the file (the threads
/// stop on their own).
pub struct SocketGuard {
    path: PathBuf,
    stop: Arc<AtomicBool>,
    /// (device, inode) of the socket we bound, so drop never removes another instance's.
    file_id: (u64, u64),
}

impl SocketGuard {
    /// Guard the socket just bound at `path`; `stop` tells its accept loop to end.
    pub fn new(path: &Path, stop: Arc<AtomicBool>) -> Self {
        SocketGuard {
            path: path.to_path_buf(),
            stop,
            file_id: file_id(path).unwrap_or_default(),
        }
    }
}

/// A file's (device, inode), without following a symlink.
fn file_id(path: &Path) -> Option<(u64, u64)> {
    std::fs::symlink_metadata(path)
        .ok()
        .map(|m| (m.dev(), m.ino()))
}

impl Drop for SocketGuard {
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

/// One live connection, counted until dropped.
pub struct Slot(Arc<AtomicUsize>);

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

/// Accept connections until `stop` is raised or `keep_going` says no. Over `max_connections`
/// live ones, a new connection is closed at once (a runaway script can't use up threads); one
/// from another user is dropped unanswered. Each accepted one goes to `serve` with its `Slot`,
/// which counts it in `live` until dropped. `label` names the server in the log.
pub fn accept_loop(
    listener: UnixListener,
    stop: Arc<AtomicBool>,
    live: Arc<AtomicUsize>,
    max_connections: usize,
    label: &str,
    keep_going: impl Fn() -> bool,
    mut serve: impl FnMut(UnixStream, Slot),
) {
    let mut warned = false;
    for stream in listener.incoming() {
        if stop.load(Ordering::Acquire) || !keep_going() {
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
        if live.load(Ordering::Acquire) >= max_connections {
            if !warned {
                eprintln!("[fuxx] {label}: {max_connections} connections open; refusing more");
                warned = true;
            }
            continue;
        }
        if peer_uid(&stream).is_some_and(same_user) {
            live.fetch_add(1, Ordering::AcqRel);
            serve(stream, Slot(live.clone()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peer_check_accepts_only_the_same_user() {
        assert!(same_user(unsafe { getuid() }));
        assert!(!same_user(unsafe { getuid() } + 1));
    }
}
