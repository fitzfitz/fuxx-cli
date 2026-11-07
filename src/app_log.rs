// The window's own log (M11d): `~/Library/Logs/fuxx/fuxx.log`, written only when fuxx runs from
// fuxx.app with no terminal for its error output, so what it prints isn't lost.

use std::ffi::c_int;
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::os::fd::AsRawFd;
use std::path::Path;

use crate::{daemon, native_notify};

unsafe extern "C" {
    fn isatty(fd: c_int) -> c_int;
    fn dup2(oldfd: c_int, newfd: c_int) -> c_int;
}

/// A `fuxx.log` over this many bytes (1 MiB) is started fresh at launch.
pub const LOG_CAP: u64 = 1_048_576;

/// Whether the window's output goes to `fuxx.log`: from the app bundle, with stderr not a terminal.
pub fn should_redirect(bundled: bool, stderr_is_tty: bool) -> bool {
    bundled && !stderr_is_tty
}

/// How to open `fuxx.log`, given the length of the one already there.
#[derive(Debug, PartialEq, Eq)]
pub enum OpenMode {
    /// Keep the file and add to its end.
    Append,
    /// Start the file over.
    Fresh,
}

/// Append, unless the existing file is over `LOG_CAP`.
pub fn open_mode(existing_len: Option<u64>) -> OpenMode {
    match existing_len {
        Some(len) if len > LOG_CAP => OpenMode::Fresh,
        _ => OpenMode::Append,
    }
}

/// Open `path` per `open_mode` and write this run's first line, `fuxx <version> started (pid N)`.
pub fn open_log(path: &Path) -> io::Result<File> {
    let existing = std::fs::metadata(path).ok().map(|m| m.len());
    let mut options = OpenOptions::new();
    options.create(true);
    match open_mode(existing) {
        OpenMode::Append => options.append(true),
        OpenMode::Fresh => options.write(true).truncate(true),
    };
    let mut file = options.open(path)?;
    writeln!(
        file,
        "fuxx {} started (pid {})",
        env!("CARGO_PKG_VERSION"),
        std::process::id()
    )?;
    Ok(file)
}

/// Send the window's stdout and stderr to `fuxx.log` when `should_redirect` says so. Any failure
/// leaves them alone, silently: there is nowhere to say it.
pub fn start(home: &Path) {
    // SAFETY: `isatty` only reads the descriptor's kind.
    let stderr_is_tty = unsafe { isatty(2) } == 1;
    if !should_redirect(native_notify::is_bundled(), stderr_is_tty) {
        return;
    }
    let dir = daemon::logs_dir(home);
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let Ok(file) = open_log(&dir.join("fuxx.log")) else {
        return;
    };
    // SAFETY: `file` is open; `dup2` copies it onto fds 1 and 2, which then own their copies.
    unsafe {
        dup2(file.as_raw_fd(), 1);
        dup2(file.as_raw_fd(), 2);
    }
}

#[cfg(test)]
mod tests {
    use super::{LOG_CAP, OpenMode, open_log, open_mode, should_redirect};
    use std::fs;

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("fuxx-app-log-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn first_line() -> String {
        format!(
            "fuxx {} started (pid {})\n",
            env!("CARGO_PKG_VERSION"),
            std::process::id()
        )
    }

    #[test]
    fn only_a_bundle_without_a_terminal_redirects() {
        assert!(should_redirect(true, false));
        assert!(!should_redirect(true, true));
        assert!(!should_redirect(false, false));
        assert!(!should_redirect(false, true));
    }

    #[test]
    fn a_log_over_the_cap_starts_fresh() {
        assert_eq!(open_mode(None), OpenMode::Append);
        assert_eq!(open_mode(Some(0)), OpenMode::Append);
        assert_eq!(open_mode(Some(LOG_CAP)), OpenMode::Append);
        assert_eq!(open_mode(Some(LOG_CAP + 1)), OpenMode::Fresh);
    }

    #[test]
    fn a_new_log_holds_just_the_first_line() {
        let dir = temp_dir("new");
        let path = dir.join("fuxx.log");
        drop(open_log(&path).unwrap());
        assert_eq!(fs::read_to_string(&path).unwrap(), first_line());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_small_log_keeps_its_contents() {
        let dir = temp_dir("small");
        let path = dir.join("fuxx.log");
        fs::write(&path, "earlier\n").unwrap();
        drop(open_log(&path).unwrap());
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            format!("earlier\n{}", first_line())
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_log_over_the_cap_is_started_over() {
        let dir = temp_dir("big");
        let path = dir.join("fuxx.log");
        fs::write(&path, vec![b'x'; LOG_CAP as usize + 1]).unwrap();
        drop(open_log(&path).unwrap());
        assert_eq!(fs::read_to_string(&path).unwrap(), first_line());
        let _ = fs::remove_dir_all(&dir);
    }
}
