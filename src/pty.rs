//! A child process on a pseudo-terminal (portable-pty, as in v1).

use std::ffi::c_int;
use std::io::{self, ErrorKind, Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};

use crate::terminal::{CellPx, TermDims};

/// What a shell's environment gets on top of the one fuxx inherited: variables to drop, then
/// variables to set.
#[derive(Debug)]
pub struct ShellEnv {
    pub set: Vec<(String, String)>,
    pub remove: Vec<&'static str>,
}

/// `program` (its path or name, then its arguments) run directly, not through a shell, so no
/// argument is ever split or expanded; `None` (or an empty one) is the user's login shell
/// (`$SHELL`, run as `-zsh` etc.). Started in `cwd` or `$HOME`, with `env` applied.
pub fn command(program: Option<&[String]>, cwd: Option<&Path>, env: &ShellEnv) -> CommandBuilder {
    let mut cmd = match program {
        Some([program, args @ ..]) => {
            let mut cmd = CommandBuilder::new(program);
            cmd.args(args);
            cmd
        }
        _ => CommandBuilder::new_default_prog(),
    };
    match cwd {
        Some(dir) => cmd.cwd(dir),
        None => {
            if let Some(home) = std::env::var_os("HOME") {
                cmd.cwd(home);
            }
        }
    }
    // Always set these: an app launched from Finder has no TERM, one launched from another
    // terminal inherits *that* terminal's TERM. `alacritty` terminfo isn't on stock macOS.
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");
    apply_env(&mut cmd, env);
    cmd
}

/// portable-pty starts from a copy of fuxx's own environment, so dropping a variable takes an
/// explicit remove.
pub(crate) fn apply_env(cmd: &mut CommandBuilder, env: &ShellEnv) {
    for key in &env.remove {
        cmd.env_remove(key);
    }
    for (key, value) in &env.set {
        cmd.env(key, value);
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

// libSystem.
unsafe extern "C" {
    fn killpg(pgrp: c_int, sig: c_int) -> c_int;
}
const SIGHUP: c_int = 1;

pub fn pty_size(dims: TermDims, cell: CellPx) -> PtySize {
    PtySize {
        rows: dims.rows as u16,
        cols: dims.cols as u16,
        pixel_width: (dims.cols as u16).saturating_mul(cell.width),
        pixel_height: (dims.rows as u16).saturating_mul(cell.height),
    }
}

type SharedChild = Arc<Mutex<Box<dyn Child + Send + Sync>>>;

pub struct Pty {
    master: Box<dyn MasterPty + Send>,
    /// Shared with the reader thread's `ExitWaiter`, which reaps it.
    child: SharedChild,
}

/// How long `ExitWaiter::exit_code` waits for the child to be reapable after EOF, and how
/// often it looks.
const EXIT_WAIT: Duration = Duration::from_secs(1);
const EXIT_POLL: Duration = Duration::from_millis(5);

/// Reads the child's exit code once its output has ended (on the reader thread).
pub struct ExitWaiter(SharedChild);

impl ExitWaiter {
    /// The child's exit code: it closes the PTY as it exits, so this normally answers at once.
    /// Waits up to `EXIT_WAIT` (a child that closed its output but runs on), then `None`;
    /// also `None` if the status can't be read. A child killed by a signal reads as 1
    /// (portable-pty's rule). Reaps the child; the lock is held only to look.
    pub fn exit_code(&self) -> Option<u32> {
        let deadline = Instant::now() + EXIT_WAIT;
        loop {
            match lock(&self.0).try_wait() {
                Ok(Some(status)) => return Some(status.exit_code()),
                Ok(None) if Instant::now() < deadline => {}
                _ => return None,
            }
            std::thread::sleep(EXIT_POLL);
        }
    }
}

impl Pty {
    /// Spawn `cmd`; returns the handle plus the output reader and the input writer, each for a
    /// dedicated thread. Keep the writer for the whole session: dropping portable-pty's writer
    /// sends EOF (^D) to the child.
    pub fn spawn(
        cmd: CommandBuilder,
        dims: TermDims,
    ) -> Result<(Pty, Box<dyn Read + Send>, Box<dyn Write + Send>), String> {
        let pair = native_pty_system()
            .openpty(pty_size(dims, CellPx::default()))
            .map_err(|e| e.to_string())?;
        let child = pair.slave.spawn_command(cmd).map_err(|e| e.to_string())?;
        // Drop our copy of the slave: the reader only reaches EOF once every slave handle closes.
        drop(pair.slave);
        let reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
        let writer = pair.master.take_writer().map_err(|e| e.to_string())?;
        Ok((
            Pty {
                master: pair.master,
                child: Arc::new(Mutex::new(child)),
            },
            reader,
            writer,
        ))
    }

    /// For the reader thread: the child's exit code after EOF.
    pub fn exit_waiter(&self) -> ExitWaiter {
        ExitWaiter(self.child.clone())
    }

    /// SIGHUP the child's whole process group, as closing a terminal window does, so its
    /// background jobs go too. The child leads its own group (portable-pty runs it with setsid).
    pub fn hang_up(&self) {
        if let Some(pid) = lock(&self.child)
            .process_id()
            .and_then(|p| c_int::try_from(p).ok())
        {
            // SAFETY: plain syscall on a pid we spawned. Unreaped, a zombie keeps its pid.
            // Reaped (`ExitWaiter`), the pid isn't reused while the group it leads still has
            // members (POSIX), so jobs left behind still get this. With none left it fails
            // (ESRCH) unless a new process took the pid *and* made its own group of it since.
            unsafe { killpg(pid, SIGHUP) };
        }
    }

    /// Tell the child its new size (it receives SIGWINCH).
    pub fn resize(&self, dims: TermDims, cell: CellPx) -> Result<(), String> {
        self.master
            .resize(pty_size(dims, cell))
            .map_err(|e| e.to_string())
    }
}

/// Reads the child's output on a dedicated thread: `on_output` gets each chunk as read, then
/// `on_eof` runs once the child has closed the PTY (or reading fails).
pub fn spawn_reader(
    mut reader: Box<dyn Read + Send>,
    mut on_output: impl FnMut(&[u8]) + Send + 'static,
    on_eof: impl FnOnce() + Send + 'static,
) -> io::Result<()> {
    std::thread::Builder::new()
        .name("fuxx-pty-reader".into())
        .spawn(move || {
            let mut buf = [0u8; 8192];
            loop {
                match reader.read(&mut buf) {
                    // EOF. portable-pty already maps macOS's EIO-at-exit to Ok(0).
                    Ok(0) => break,
                    Ok(n) => on_output(&buf[..n]),
                    Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                    Err(_) => break,
                }
            }
            on_eof();
        })
        .map(drop)
}

/// Starts the thread that owns the PTY writer for the whole session and returns its queue, so
/// a child that stops reading can't block whoever sends. The thread ends when every sender is
/// dropped or the child goes away.
pub fn spawn_writer(writer: Box<dyn Write + Send>) -> io::Result<Sender<Vec<u8>>> {
    spawn_counted_writer(writer, Arc::new(AtomicUsize::new(0)))
}

/// `spawn_writer`, taking `pending` down by each chunk's length once it is written (or
/// dropped because the child went away). The sender adds to it before sending.
pub fn spawn_counted_writer(
    mut writer: Box<dyn Write + Send>,
    pending: Arc<AtomicUsize>,
) -> io::Result<Sender<Vec<u8>>> {
    let (input, queued) = std::sync::mpsc::channel::<Vec<u8>>();
    std::thread::Builder::new()
        .name("fuxx-pty-writer".into())
        .spawn(move || {
            for bytes in &queued {
                let written = writer.write_all(&bytes).and_then(|()| writer.flush());
                pending.fetch_sub(bytes.len(), Ordering::Relaxed);
                if written.is_err() {
                    break;
                }
            }
            // Whatever is still queued will never be written.
            for bytes in queued.try_iter() {
                pending.fetch_sub(bytes.len(), Ordering::Relaxed);
            }
        })?;
    Ok(input)
}

impl Drop for Pty {
    fn drop(&mut self) {
        // Closing a session must not leave its shell/agent running. One already reaped is
        // gone, and its pid may belong to someone else by now: leave it.
        let mut child = lock(&self.child);
        if !matches!(child.try_wait(), Ok(Some(_))) {
            let _ = child.kill();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_env_removes_then_sets() {
        let mut cmd = CommandBuilder::new("sh");
        cmd.env_clear();
        cmd.env("GONE", "inherited");
        cmd.env("KEPT", "inherited");
        cmd.env("SET", "inherited");
        apply_env(
            &mut cmd,
            &ShellEnv {
                set: vec![("SET".into(), "ours".into())],
                remove: vec!["GONE", "SET"],
            },
        );
        assert_eq!(cmd.get_env("GONE"), None);
        assert_eq!(
            cmd.get_env("KEPT").and_then(|v| v.to_str()),
            Some("inherited")
        );
        assert_eq!(cmd.get_env("SET").and_then(|v| v.to_str()), Some("ours"));
        // The real command applies it too.
        let cmd = command(
            None,
            None,
            &ShellEnv {
                set: Vec::new(),
                remove: vec!["FUXX_SESSION"],
            },
        );
        assert_eq!(cmd.get_env("FUXX_SESSION"), None);
        assert_eq!(
            cmd.get_env("TERM").and_then(|v| v.to_str()),
            Some("xterm-256color")
        );
    }

    #[test]
    fn pty_size_carries_pixels() {
        let s = pty_size(
            TermDims { cols: 80, rows: 24 },
            CellPx {
                width: 8,
                height: 17,
            },
        );
        assert_eq!(
            (s.cols, s.rows, s.pixel_width, s.pixel_height),
            (80, 24, 640, 408)
        );
    }
}
