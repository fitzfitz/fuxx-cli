//! A child process on a pseudo-terminal (portable-pty, as in v1).

use std::ffi::c_int;
use std::io::{self, ErrorKind, Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};

use crate::terminal::{CellPx, TermDims};

/// What a shell's environment gets on top of the one fuxx inherited (or `base`): variables to
/// drop, then variables to set.
#[derive(Debug)]
pub struct ShellEnv {
    /// Start from this environment instead of fuxx's own (an SSH session: your login shell's).
    pub base: Option<Vec<(String, String)>>,
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
    if let Some(base) = &env.base {
        cmd.env_clear();
        for (key, value) in base {
            cmd.env(key, value);
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

    /// The child's process id (its working folder is a row's folder, M11b).
    pub fn pid(&self) -> Option<u32> {
        lock(&self.child).process_id()
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

/// The most the parser thread takes from the inbox at once, so the Term lock (which the UI
/// takes every frame) is never held for more than one batch's parse.
pub const BATCH_MAX: usize = 64 * 1024;
/// The reader waits while this much is read and not yet taken: backpressure, never loss. The
/// child then blocks on a full PTY, as it would in any terminal that parses slower than it
/// prints.
pub const INBOX_CAP: usize = 1 << 20;
/// The buffer capacity a drained inbox keeps, so an idle terminal holds no flood-sized buffer.
pub const IDLE_KEEP: usize = 64 * 1024;

/// What the reader thread has read and the parser thread hasn't taken yet.
#[derive(Default)]
struct Queue {
    bytes: Vec<u8>,
    /// `bytes[read..]` is still to take.
    read: usize,
    /// The reader is done (EOF or a read error): nothing more will be pushed.
    closed: bool,
    /// The parser side is gone: nothing more will be taken.
    gone: bool,
}

impl Queue {
    fn queued(&self) -> usize {
        self.bytes.len() - self.read
    }
}

/// The hand-off between a PTY's reader thread and its parser thread: the reader pushes what it
/// reads, in order; the parser takes it in batches of at most `BATCH_MAX`. Neither side holds
/// the lock while it reads or parses, only to copy bytes in or out.
pub(crate) struct Inbox {
    queue: Mutex<Queue>,
    /// Signalled when bytes arrive or the reader closes (the parser waits on it).
    bytes: Condvar,
    /// Signalled when bytes are taken or the parser goes (the reader waits on it).
    room: Condvar,
}

impl Inbox {
    pub(crate) fn new() -> Self {
        Self {
            queue: Mutex::new(Queue::default()),
            bytes: Condvar::new(),
            room: Condvar::new(),
        }
    }

    /// Append `bytes`, first waiting while `INBOX_CAP` or more is queued. `false` once the
    /// parser side is gone (the bytes then go nowhere: stop reading).
    pub(crate) fn push(&self, bytes: &[u8]) -> bool {
        let mut q = lock(&self.queue);
        while !q.gone && q.queued() >= INBOX_CAP {
            q = self.room.wait(q).unwrap_or_else(PoisonError::into_inner);
        }
        if q.gone {
            return false;
        }
        q.bytes.extend_from_slice(bytes);
        drop(q);
        self.bytes.notify_one();
        true
    }

    /// The reader is done: the parser takes what is left, then hears EOF.
    pub(crate) fn close(&self) {
        lock(&self.queue).closed = true;
        self.bytes.notify_all();
    }

    /// The parser side is gone: a waiting or later `push` returns `false`.
    pub(crate) fn parser_gone(&self) {
        lock(&self.queue).gone = true;
        self.room.notify_all();
    }

    /// Replace `into`'s contents with the next batch: waits for bytes (or EOF), then moves up
    /// to `BATCH_MAX` of them, oldest first. `false` = EOF and nothing left (`into` empty).
    pub(crate) fn take(&self, into: &mut Vec<u8>) -> bool {
        into.clear();
        let mut q = lock(&self.queue);
        while q.queued() == 0 {
            if q.closed {
                return false;
            }
            q = self.bytes.wait(q).unwrap_or_else(PoisonError::into_inner);
        }
        let end = q.read + q.queued().min(BATCH_MAX);
        into.extend_from_slice(&q.bytes[q.read..end]);
        q.read = end;
        if q.queued() == 0 {
            q.bytes.clear();
            q.read = 0;
            q.bytes.shrink_to(IDLE_KEEP);
        } else if q.read >= q.queued() {
            // Drop the taken front once it outweighs the rest, so each byte is moved at most
            // a constant number of times on average.
            let read = q.read;
            q.bytes.drain(..read);
            q.read = 0;
        }
        drop(q);
        self.room.notify_one();
        true
    }

    #[cfg(test)]
    fn capacity(&self) -> usize {
        lock(&self.queue).bytes.capacity()
    }
}

/// Closes the inbox when the reader thread ends, however it ends.
struct ReaderEnd(Arc<Inbox>);

impl Drop for ReaderEnd {
    fn drop(&mut self) {
        self.0.close();
    }
}

/// Tells the reader the parser is gone when the parser thread ends, however it ends.
struct ParserEnd(Arc<Inbox>);

impl Drop for ParserEnd {
    fn drop(&mut self) {
        self.0.parser_gone();
    }
}

/// Reads the child's output on one thread and parses it on another. `fuxx-pty-reader` only
/// reads and pushes into an `Inbox` (waiting while `INBOX_CAP` is queued); `fuxx-pty-parser`
/// owns `on_output` and gives it each batch of up to `BATCH_MAX` bytes, in read order, then
/// runs `on_eof` once the child has closed the PTY (or reading failed) and the last batch is
/// done. No lock is held while `on_output` runs.
pub fn spawn_reader(
    mut reader: Box<dyn Read + Send>,
    mut on_output: impl FnMut(&[u8]) + Send + 'static,
    on_eof: impl FnOnce() + Send + 'static,
) -> io::Result<()> {
    let inbox = Arc::new(Inbox::new());
    let pushing = ReaderEnd(inbox.clone());
    std::thread::Builder::new()
        .name("fuxx-pty-reader".into())
        .spawn(move || {
            let mut buf = [0u8; 8192];
            loop {
                match reader.read(&mut buf) {
                    // EOF. portable-pty already maps macOS's EIO-at-exit to Ok(0).
                    Ok(0) => break,
                    Ok(n) => {
                        if !pushing.0.push(&buf[..n]) {
                            break; // no one is parsing any more
                        }
                    }
                    Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                    Err(_) => break,
                }
            }
            // `pushing` drops here: the inbox closes and the parser finishes up.
        })?;
    // If this fails, `ParserEnd` drops with the closure and the reader stops at its next push.
    let taking = ParserEnd(inbox);
    std::thread::Builder::new()
        .name("fuxx-pty-parser".into())
        .spawn(move || {
            let mut batch = Vec::with_capacity(BATCH_MAX);
            while taking.0.take(&mut batch) {
                on_output(&batch);
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
                base: None,
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
                base: None,
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
    fn a_base_environment_replaces_fuxx_s_own() {
        let base = [
            ("PATH", "/login/bin:/usr/bin"),
            ("SSH_AUTH_SOCK", "/agent"),
            ("TERM", "dumb"),
            ("FUXX_SESSION", "s9"),
        ];
        let env = ShellEnv {
            base: Some(base.map(|(k, v)| (k.to_string(), v.to_string())).to_vec()),
            set: vec![("FUXX_SOCKET".into(), "/ours.sock".into())],
            remove: vec!["FUXX_SESSION"],
        };
        let cmd = command(Some(&["ssh".to_string()]), None, &env);
        let var = |k: &str| cmd.get_env(k).and_then(|v| v.to_str()).map(String::from);
        assert_eq!(var("PATH").as_deref(), Some("/login/bin:/usr/bin"));
        assert_eq!(var("SSH_AUTH_SOCK").as_deref(), Some("/agent"));
        // fuxx's own variables on top.
        assert_eq!(var("TERM").as_deref(), Some("xterm-256color"));
        assert_eq!(var("COLORTERM").as_deref(), Some("truecolor"));
        assert_eq!(var("FUXX_SOCKET").as_deref(), Some("/ours.sock"));
        assert_eq!(var("FUXX_SESSION"), None);
        // Nothing of fuxx's own environment that the base doesn't have.
        assert!(std::env::var_os("HOME").is_some());
        assert_eq!(var("HOME"), None);
    }

    /// splitmix64, so the inbox test's piece sizes and pauses are the same every run.
    fn splitmix(state: &mut u64) -> u64 {
        *state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = *state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    #[test]
    fn the_inbox_keeps_order_and_blocks_when_full() {
        // One thread pushes 3 MiB in random-sized pieces while another takes with random pauses.
        let mut state = 7;
        let input: Vec<u8> = (0..3 << 20).map(|_| splitmix(&mut state) as u8).collect();
        let inbox = Arc::new(Inbox::new());
        let pusher = {
            let inbox = inbox.clone();
            let input = input.clone();
            std::thread::spawn(move || {
                let mut state = 11;
                let mut at = 0;
                while at < input.len() {
                    let n = (1 + splitmix(&mut state) % 20_000) as usize;
                    let end = (at + n).min(input.len());
                    assert!(inbox.push(&input[at..end]));
                    at = end;
                }
                inbox.close();
            })
        };
        let mut state = 13;
        let mut got = Vec::new();
        let mut batch = Vec::new();
        while inbox.take(&mut batch) {
            assert!(
                !batch.is_empty() && batch.len() <= BATCH_MAX,
                "{}",
                batch.len()
            );
            got.extend_from_slice(&batch);
            if splitmix(&mut state).is_multiple_of(8) {
                std::thread::sleep(Duration::from_millis(splitmix(&mut state) % 3));
            }
        }
        pusher.join().unwrap();
        assert!(
            got == input,
            "the bytes taken are not the bytes pushed, in order"
        );

        // With no taker, a push past INBOX_CAP waits, and returns once 64 KiB is taken.
        let inbox = Arc::new(Inbox::new());
        for _ in 0..INBOX_CAP / 8192 {
            assert!(inbox.push(&[b'x'; 8192]));
        }
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let pusher = {
            let inbox = inbox.clone();
            std::thread::spawn(move || {
                let pushed = inbox.push(b"more");
                let _ = done_tx.send(pushed);
            })
        };
        assert!(
            done_rx.recv_timeout(Duration::from_millis(200)).is_err(),
            "a push past INBOX_CAP didn't wait"
        );
        assert!(inbox.take(&mut batch));
        assert_eq!(batch.len(), BATCH_MAX);
        assert_eq!(done_rx.recv_timeout(Duration::from_secs(2)), Ok(true));
        pusher.join().unwrap();
    }

    #[test]
    fn eof_comes_after_the_last_batch() {
        let inbox = Inbox::new();
        assert!(inbox.push(b"last words"));
        inbox.close();
        let mut batch = Vec::new();
        assert!(inbox.take(&mut batch));
        assert_eq!(batch, b"last words");
        assert!(!inbox.take(&mut batch));
        assert!(batch.is_empty());
    }

    #[test]
    fn a_push_fails_once_the_parser_is_gone() {
        let inbox = Arc::new(Inbox::new());
        for _ in 0..INBOX_CAP / 8192 {
            assert!(inbox.push(&[b'x'; 8192]));
        }
        // A reader waiting for room hears that no one will ever take.
        let waiting = {
            let inbox = inbox.clone();
            std::thread::spawn(move || inbox.push(b"more"))
        };
        std::thread::sleep(Duration::from_millis(50));
        inbox.parser_gone();
        assert!(!waiting.join().unwrap());
        assert!(!inbox.push(b"x"));
    }

    #[test]
    fn a_drained_inbox_keeps_at_most_idle_keep() {
        let inbox = Inbox::new();
        for _ in 0..INBOX_CAP / 8192 {
            assert!(inbox.push(&[b'x'; 8192]));
        }
        let mut batch = Vec::new();
        let mut taken = 0;
        while taken < INBOX_CAP {
            assert!(inbox.take(&mut batch));
            taken += batch.len();
        }
        assert!(inbox.capacity() <= IDLE_KEEP, "{}", inbox.capacity());
        assert!(batch.capacity() <= IDLE_KEEP.max(BATCH_MAX));
    }

    /// Sets its flag when dropped: the thread that owns it has finished.
    struct DropFlag(Arc<std::sync::atomic::AtomicBool>);

    impl Drop for DropFlag {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    /// A PTY reader that says when the reader thread lets go of it.
    struct FlaggedReader {
        inner: Box<dyn Read + Send>,
        _done: DropFlag,
    }

    impl Read for FlaggedReader {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            self.inner.read(buf)
        }
    }

    #[test]
    fn both_threads_end_when_the_pty_goes_mid_flood() {
        use std::sync::atomic::AtomicBool;
        let (pty, reader, _writer) = Pty::spawn(
            {
                let mut cmd = CommandBuilder::new("/bin/sh");
                cmd.args(["-c", "yes fuxx-flood"]);
                cmd
            },
            TermDims { cols: 80, rows: 24 },
        )
        .unwrap();
        let reader_done = Arc::new(AtomicBool::new(false));
        let parser_done = Arc::new(AtomicBool::new(false));
        let eof = Arc::new(AtomicBool::new(false));
        let seen = Arc::new(AtomicUsize::new(0));
        let flag = DropFlag(parser_done.clone());
        let counted = seen.clone();
        let ended = eof.clone();
        spawn_reader(
            Box::new(FlaggedReader {
                inner: reader,
                _done: DropFlag(reader_done.clone()),
            }),
            move |bytes| {
                let _keep = &flag;
                counted.fetch_add(bytes.len(), Ordering::SeqCst);
            },
            move || ended.store(true, Ordering::SeqCst),
        )
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while seen.load(Ordering::SeqCst) < 1 << 20 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(seen.load(Ordering::SeqCst) >= 1 << 20, "no flood");
        drop(pty); // the pane closed: the child is killed
        let deadline = Instant::now() + Duration::from_secs(1);
        while !(reader_done.load(Ordering::SeqCst) && parser_done.load(Ordering::SeqCst))
            && Instant::now() < deadline
        {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(
            reader_done.load(Ordering::SeqCst),
            "the reader thread is still running"
        );
        assert!(
            parser_done.load(Ordering::SeqCst),
            "the parser thread is still running"
        );
        assert!(eof.load(Ordering::SeqCst), "on_eof never ran");
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
