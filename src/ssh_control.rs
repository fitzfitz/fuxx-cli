//! M10b: the private folder for shared ssh connections (`/tmp/fuxx-<uid>`) and the health
//! check of a shared connection before it is reused. No GPUI.

use std::ffi::OsStr;
use std::io;
use std::os::unix::fs::{DirBuilderExt, FileTypeExt, MetadataExt, PermissionsExt};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

/// `base/fuxx-<uid>`, created 0700. Refuses (Err) anything that isn't a real folder owned by
/// `uid` with mode 0700, such as a symlink or a folder someone else made first.
pub fn control_dir_in(base: &Path, uid: u32) -> io::Result<PathBuf> {
    let dir = base.join(format!("fuxx-{uid}"));
    match std::fs::DirBuilder::new().mode(0o700).create(&dir) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e),
    }
    // Look at the entry itself, not what a symlink points to.
    let meta = std::fs::symlink_metadata(&dir)?;
    let refuse = |why: &str| {
        Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("{} {why}", dir.display()),
        ))
    };
    if !meta.is_dir() {
        return refuse("is not a real folder");
    }
    if meta.uid() != uid {
        return refuse("belongs to another user");
    }
    if meta.permissions().mode() & 0o777 != 0o700 {
        return refuse("is not private (mode 0700)");
    }
    Ok(dir)
}

/// The shared-connection folder under `/tmp`. Logs one line and returns `None` if it can't be
/// used; ssh is then run without the reuse options.
pub fn control_dir() -> Option<PathBuf> {
    let uid = crate::private_socket::current_uid();
    match control_dir_in(Path::new("/tmp"), uid) {
        Ok(dir) => Some(dir),
        Err(err) => {
            eprintln!("[fuxx] ssh: not sharing connections: {err}");
            None
        }
    }
}

/// `ensure_healthy_with` using the `ssh` on the PATH.
pub fn ensure_healthy(host: &str, dir: &Path, timeout: Duration) {
    ensure_healthy_with(OsStr::new("ssh"), host, dir, timeout)
}

/// Ask the shared connection for `host` if it is alive (`program -O check`). If not, or it
/// doesn't answer within `timeout`, tell it to exit and remove any socket file in `dir` that
/// refuses connections, so ssh starts a fresh one. Blocks, so call it off the UI thread.
pub fn ensure_healthy_with(program: &OsStr, host: &str, dir: &Path, timeout: Duration) {
    let mut control_path = dir.as_os_str().to_owned();
    control_path.push("/%C");
    let run = |operation: &str, cap: Duration| {
        let mut command = Command::new(program);
        command
            .arg("-o")
            .arg(format!("ControlPath={}", control_path.to_string_lossy()))
            .args(["-O", operation, "--", host]);
        crate::ssh_hosts::run_capped(&mut command, cap)
    };
    if run("check", timeout).is_some_and(|(status, _)| status.success()) {
        return;
    }
    let _ = run("exit", EXIT_CAP);
    remove_dead_sockets(dir);
}

/// How long `-O exit` gets before it is given up on.
const EXIT_CAP: Duration = Duration::from_secs(1);

/// Remove the socket files in `dir` that refuse a connection. Sockets that accept, and files
/// that aren't sockets, stay.
fn remove_dead_sockets(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let is_socket = entry.file_type().is_ok_and(|t| t.is_socket());
        if !is_socket {
            continue;
        }
        let dead = matches!(
            UnixStream::connect(entry.path()),
            Err(e) if matches!(
                e.kind(),
                io::ErrorKind::ConnectionRefused | io::ErrorKind::NotFound
            )
        );
        if dead {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::net::UnixListener;
    use std::time::Instant;

    fn temp(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("fuxx-ssh-ctl-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn script(dir: &Path, name: &str, body: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    fn uid() -> u32 {
        crate::private_socket::current_uid()
    }

    #[test]
    fn control_dir_is_private() {
        let base = temp("private");
        let dir = control_dir_in(&base, uid()).unwrap();
        assert_eq!(dir, base.join(format!("fuxx-{}", uid())));
        let meta = std::fs::metadata(&dir).unwrap();
        assert!(meta.is_dir());
        assert_eq!(meta.permissions().mode() & 0o777, 0o700);
        // A second call finds it fine.
        assert_eq!(control_dir_in(&base, uid()).unwrap(), dir);
    }

    #[test]
    fn control_dir_refuses_symlink_and_wrong_mode() {
        let base = temp("refuse");
        let real = base.join("elsewhere");
        std::fs::create_dir(&real).unwrap();
        std::fs::set_permissions(&real, std::fs::Permissions::from_mode(0o700)).unwrap();
        std::os::unix::fs::symlink(&real, base.join(format!("fuxx-{}", uid()))).unwrap();
        assert!(control_dir_in(&base, uid()).is_err());

        let base = temp("mode");
        let dir = base.join(format!("fuxx-{}", uid()));
        std::fs::create_dir(&dir).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(control_dir_in(&base, uid()).is_err());

        // Owned by someone else: ask for a uid that isn't ours, so the folder (made by us) is
        // another owner's as far as the check can tell.
        let base = temp("owner");
        assert!(control_dir_in(&base, uid() + 1).is_err());
    }

    #[test]
    fn a_failed_check_removes_the_stale_master() {
        let base = temp("stale");
        let dir = base.join("ctl");
        std::fs::create_dir(&dir).unwrap();
        // A socket file nobody listens on: bind, then drop the listener (the file stays).
        let dead = dir.join("dead");
        drop(UnixListener::bind(&dead).unwrap());
        // One that still accepts, and a plain file.
        let live = dir.join("live");
        let _listener = UnixListener::bind(&live).unwrap();
        let plain = dir.join("notes.txt");
        std::fs::write(&plain, "keep").unwrap();

        let log = base.join("calls");
        let fake = script(
            &base,
            "ssh",
            &format!(
                "echo \"$@\" >> '{}'\ncase \"$*\" in *'-O check'*) exit 255;; esac\nexit 0",
                log.display()
            ),
        );
        ensure_healthy_with(fake.as_os_str(), "devbox", &dir, ENOUGH);

        assert!(!dead.exists());
        assert!(live.exists());
        assert!(plain.exists());
        let calls = std::fs::read_to_string(&log).unwrap();
        assert!(calls.contains(&format!(
            "-o ControlPath={}/%C -O check -- devbox",
            dir.display()
        )));
        assert!(calls.contains("-O exit -- devbox"));
    }

    /// Time for a fake ssh that does finish: under a full parallel test run, a freshly written
    /// script can take seconds to start (seen at > 2 s in `ssh_hosts`).
    const ENOUGH: Duration = Duration::from_secs(10);

    #[test]
    fn a_healthy_check_changes_nothing() {
        let base = temp("healthy");
        let dir = base.join("ctl");
        std::fs::create_dir(&dir).unwrap();
        let dead = dir.join("dead");
        drop(UnixListener::bind(&dead).unwrap());
        let fake = script(&base, "ssh", "exit 0");
        ensure_healthy_with(fake.as_os_str(), "devbox", &dir, ENOUGH);
        assert!(dead.exists());
    }

    #[test]
    fn a_hung_check_is_capped() {
        let base = temp("hung");
        let dir = base.join("ctl");
        std::fs::create_dir(&dir).unwrap();
        let fake = script(&base, "ssh", "sleep 30");
        let start = Instant::now();
        ensure_healthy_with(fake.as_os_str(), "devbox", &dir, Duration::from_millis(500));
        // The check (0.5 s) plus the exit (1 s cap), with some room.
        assert!(start.elapsed() < Duration::from_millis(3500));
    }
}
