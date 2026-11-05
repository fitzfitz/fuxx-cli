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
/// refuses connections, so ssh starts a fresh one. One that doesn't answer is hung, and its
/// socket may still accept (into the kernel's queue), so this host's own socket is removed
/// too. Blocks, so call it off the UI thread.
pub fn ensure_healthy_with(program: &OsStr, host: &str, dir: &Path, timeout: Duration) {
    ensure_healthy_capped(program, host, dir, timeout, EXIT_CAP, RESOLVE_CAP)
}

/// `ensure_healthy_with` with the caps of the follow-up `-O exit` and `-G` calls given, so a
/// test can allow them time that a loaded machine needs.
fn ensure_healthy_capped(
    program: &OsStr,
    host: &str,
    dir: &Path,
    timeout: Duration,
    exit_cap: Duration,
    resolve_cap: Duration,
) {
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
    let check = run("check", timeout);
    if check.as_ref().is_some_and(|(status, _)| status.success()) {
        return;
    }
    let _ = run("exit", exit_cap);
    if check.is_none() {
        // Hung: where its socket is, as ssh expands `%C` for this host.
        let mut command = Command::new(program);
        command
            .arg("-G")
            .arg("-o")
            .arg(format!("ControlPath={}", control_path.to_string_lossy()))
            .args(["--", host]);
        let socket = crate::ssh_hosts::run_capped(&mut command, resolve_cap)
            .filter(|(status, _)| status.success())
            .and_then(|(_, text)| host_socket(&text, dir));
        if let Some(socket) = socket
            && std::fs::symlink_metadata(&socket).is_ok_and(|m| m.file_type().is_socket())
        {
            let _ = std::fs::remove_file(&socket);
        }
    }
    remove_dead_sockets(dir);
}

/// The `controlpath` in `ssh -G` output, if it is a file right in `dir` (ours); none for
/// `none` or anywhere else.
fn host_socket(text: &str, dir: &Path) -> Option<PathBuf> {
    let path = text.lines().find_map(|line| {
        let (key, value) = line.split_once(' ')?;
        (key == "controlpath").then(|| PathBuf::from(value.trim()))
    })?;
    // Not `..` (no file name) nor anywhere else.
    path.file_name()?;
    (path.parent() == Some(dir)).then_some(path)
}

/// How long `ssh -G` gets to name a hung connection's socket (it answered quickly before the
/// check: `ssh_program` reads the host's settings first).
const RESOLVE_CAP: Duration = Duration::from_secs(2);

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
    fn a_hung_master_s_socket_is_removed() {
        let base = temp("hung-master");
        let dir = base.join("ctl");
        std::fs::create_dir(&dir).unwrap();
        // The hung master's socket still accepts (into its backlog), as does another host's.
        let mine = dir.join("c9cfb6d4");
        let _hung = UnixListener::bind(&mine).unwrap();
        let other = dir.join("0a1b2c3d");
        let _other = UnixListener::bind(&other).unwrap();
        let log = base.join("calls");
        let fake = script(
            &base,
            "ssh",
            &format!(
                "echo \"$@\" >> '{}'\n\
                 case \"$*\" in\n\
                 *'-O check'*) sleep 30;;\n\
                 *'-G'*) printf 'user me\\ncontrolpath {}\\ncontrolpersist no\\n';;\n\
                 esac\nexit 0",
                log.display(),
                mine.display()
            ),
        );
        // Only the check is meant to time out (the fake sleeps 30 s). The `-O exit` and `-G`
        // calls finish, but under a full parallel run a fresh script can take seconds to start,
        // so they get ENOUGH rather than their production caps.
        ensure_healthy_capped(
            fake.as_os_str(),
            "devbox",
            &dir,
            Duration::from_secs(2),
            ENOUGH,
            ENOUGH,
        );
        assert!(!mine.exists(), "the hung master's socket is gone");
        assert!(other.exists(), "another host's stays");
        let calls = std::fs::read_to_string(&log).unwrap();
        assert!(calls.contains("-O exit -- devbox"), "{calls}");
        assert!(
            calls.contains(&format!("-G -o ControlPath={}/%C -- devbox", dir.display())),
            "{calls}"
        );
    }

    #[test]
    fn the_control_path_is_read_from_ssh_g() {
        let dir = Path::new("/tmp/fuxx-501");
        let text = "user me\ncontrolpath /tmp/fuxx-501/c9cf\ncontrolpersist no\n";
        assert_eq!(
            host_socket(text, dir),
            Some(PathBuf::from("/tmp/fuxx-501/c9cf"))
        );
        // None set, or one outside our folder: nothing to remove.
        assert_eq!(host_socket("controlpath none\n", dir), None);
        assert_eq!(host_socket("controlpath /etc/passwd\n", dir), None);
        assert_eq!(host_socket("controlpath /tmp/fuxx-501/../x\n", dir), None);
        assert_eq!(host_socket("user me\n", dir), None);
    }

    #[test]
    fn a_hung_check_is_capped() {
        let base = temp("hung");
        let dir = base.join("ctl");
        std::fs::create_dir(&dir).unwrap();
        let fake = script(&base, "ssh", "sleep 30");
        let start = Instant::now();
        ensure_healthy_with(fake.as_os_str(), "devbox", &dir, Duration::from_millis(500));
        // The check (0.5 s), the exit (1 s cap) and, hung, `ssh -G` for its socket (2 s cap),
        // with some room.
        assert!(start.elapsed() < Duration::from_millis(5500));
    }
}
