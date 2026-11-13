#![allow(dead_code)] // consumers arrive in M11aa Tasks 7–11; remove when wired

//! Runs an agent as a piped child process (its own session and process group) and streams its
//! stdout lines as `ProcessEvent`s. GPUI-free: plain threads and a futures channel.

use futures::channel::mpsc::UnboundedSender;
use std::ffi::c_int;
use std::io::{BufRead, Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

/// The longest stdout line kept; a longer one is dropped and replaced by `OVERLONG_MARKER`.
pub const MAX_LINE: usize = 8 * 1024 * 1024;
/// Sent in place of a line over `MAX_LINE`; every translator ignores this type.
const OVERLONG_MARKER: &str = r#"{"type":"fuxx_overlong"}"#;
/// How much of the end of stderr is kept for `Exited`.
const STDERR_TAIL: usize = 4 * 1024;
/// How long a process group gets after SIGTERM before SIGKILL.
const TERM_GRACE: Duration = Duration::from_secs(2);

unsafe extern "C" {
    fn setsid() -> c_int;
    fn kill(pid: c_int, sig: c_int) -> c_int;
}

const SIGTERM: c_int = 15;
const SIGKILL: c_int = 9;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProcessEvent {
    /// One stdout line (without its newline).
    Line(String),
    /// The process ended and stdout was fully read; sent exactly once, after every `Line`.
    Exited {
        code: Option<i32>,
        stderr_tail: String,
    },
    /// The process couldn't be started.
    SpawnFailed(String),
}

pub struct AgentProcess {
    /// The child's pid (also its process group id); 0 until spawned.
    pid: Arc<AtomicI32>,
    /// Set by `kill`, so a child spawned after it is killed at once.
    killed: Arc<AtomicBool>,
    /// Set once the child is reaped and its output is read.
    done: Arc<AtomicBool>,
    writer: mpsc::Sender<String>,
}

/// Start `argv` in `cwd` (with `PATH` = `path_env` if given); never blocks. Events arrive on
/// `events`.
pub fn spawn(
    argv: &[String],
    cwd: &Path,
    path_env: Option<&str>,
    events: UnboundedSender<ProcessEvent>,
) -> AgentProcess {
    let pid = Arc::new(AtomicI32::new(0));
    let killed = Arc::new(AtomicBool::new(false));
    let done = Arc::new(AtomicBool::new(false));
    let (writer, lines) = mpsc::channel::<String>();
    let process = AgentProcess {
        pid: pid.clone(),
        killed: killed.clone(),
        done: done.clone(),
        writer,
    };
    let argv = argv.to_vec();
    let cwd = cwd.to_path_buf();
    let path_env = path_env.map(str::to_string);
    let started = std::thread::Builder::new()
        .name("fuxx-agent".into())
        .spawn({
            let events = events.clone();
            move || run(argv, cwd, path_env, events, lines, pid, killed, done)
        });
    if let Err(err) = started {
        let _ = events.unbounded_send(ProcessEvent::SpawnFailed(err.to_string()));
    }
    process
}

impl AgentProcess {
    /// Queue `line` for the agent's stdin (a newline is added).
    pub fn send(&self, line: String) {
        let _ = self.writer.send(line);
    }

    /// The child's pid, once it is running.
    pub fn pid(&self) -> Option<i32> {
        Some(self.pid.load(Ordering::SeqCst)).filter(|pid| *pid > 0)
    }

    /// SIGTERM the process group, SIGKILL it after `TERM_GRACE`; the child is reaped by its
    /// own thread, which then sends `Exited`.
    pub fn kill(&self) {
        if self.killed.swap(true, Ordering::SeqCst) {
            return;
        }
        let pid = self.pid.load(Ordering::SeqCst);
        if pid <= 0 {
            return; // not spawned yet: the spawning thread sees `killed`
        }
        // SAFETY: plain signal to the group we created with setsid.
        unsafe { kill(-pid, SIGTERM) };
        let done = self.done.clone();
        std::thread::spawn(move || {
            let until = Instant::now() + TERM_GRACE;
            while Instant::now() < until {
                if done.load(Ordering::SeqCst) {
                    return;
                }
                std::thread::sleep(Duration::from_millis(25));
            }
            if !done.load(Ordering::SeqCst) {
                // SAFETY: as above; `done` unset means the group leader isn't reaped yet.
                unsafe { kill(-pid, SIGKILL) };
            }
        });
    }
}

impl Drop for AgentProcess {
    fn drop(&mut self) {
        self.kill();
    }
}

/// `command.spawn()`, tried again a few times on ETXTBSY ("text file busy").
fn spawn_retrying(command: &mut Command) -> std::io::Result<std::process::Child> {
    const ETXTBSY: i32 = 26;
    for _ in 0..5 {
        match command.spawn() {
            Err(err) if err.raw_os_error() == Some(ETXTBSY) => {
                std::thread::sleep(Duration::from_millis(20));
            }
            spawned => return spawned,
        }
    }
    command.spawn()
}

#[allow(clippy::too_many_arguments)]
fn run(
    argv: Vec<String>,
    cwd: std::path::PathBuf,
    path_env: Option<String>,
    events: UnboundedSender<ProcessEvent>,
    lines: mpsc::Receiver<String>,
    pid: Arc<AtomicI32>,
    killed: Arc<AtomicBool>,
    done: Arc<AtomicBool>,
) {
    use std::os::unix::process::CommandExt;
    let Some((program, args)) = argv.split_first() else {
        let _ = events.unbounded_send(ProcessEvent::SpawnFailed("empty command".into()));
        done.store(true, Ordering::SeqCst);
        return;
    };
    let mut command = Command::new(program);
    command
        .args(args)
        .current_dir(&cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(path) = &path_env {
        command.env("PATH", path);
    }
    // SAFETY: setsid is async-signal-safe; it makes the child its own process group's leader.
    unsafe {
        command.pre_exec(|| {
            setsid();
            Ok(())
        });
    }
    let mut child = match spawn_retrying(&mut command) {
        Ok(child) => child,
        Err(err) => {
            let _ = events.unbounded_send(ProcessEvent::SpawnFailed(format!("{program}: {err}")));
            done.store(true, Ordering::SeqCst);
            return;
        }
    };
    pid.store(child.id() as i32, Ordering::SeqCst);
    if killed.load(Ordering::SeqCst) {
        // SAFETY: signal to the group just created.
        unsafe { kill(-(child.id() as i32), SIGTERM) };
    }
    if let Some(mut stdin) = child.stdin.take() {
        let _ = std::thread::Builder::new()
            .name("fuxx-agent-writer".into())
            .spawn(move || {
                for line in lines {
                    if stdin.write_all(line.as_bytes()).is_err()
                        || stdin.write_all(b"\n").is_err()
                        || stdin.flush().is_err()
                    {
                        break;
                    }
                }
            });
    }
    let reader = child.stdout.take().map(|stdout| {
        let events = events.clone();
        std::thread::Builder::new()
            .name("fuxx-agent-reader".into())
            .spawn(move || read_lines(stdout, &events))
    });
    let err_reader = child.stderr.take().map(|mut stderr| {
        std::thread::Builder::new()
            .name("fuxx-agent-stderr".into())
            .spawn(move || {
                let mut tail: Vec<u8> = Vec::new();
                let mut buf = [0u8; 4096];
                while let Ok(n) = stderr.read(&mut buf) {
                    if n == 0 {
                        break;
                    }
                    tail.extend_from_slice(&buf[..n]);
                    if tail.len() > STDERR_TAIL {
                        tail.drain(..tail.len() - STDERR_TAIL);
                    }
                }
                String::from_utf8_lossy(&tail).into_owned()
            })
    });
    let status = child.wait(); // reaps
    if killed.load(Ordering::SeqCst) {
        // Stragglers of the group could hold the pipes open; the leader is gone, not the group.
        // SAFETY: the group id can't be reused while it has members.
        unsafe { kill(-(child.id() as i32), SIGKILL) };
    }
    if let Some(Ok(reader)) = reader {
        let _ = reader.join();
    }
    let stderr_tail = err_reader
        .and_then(Result::ok)
        .and_then(|handle| handle.join().ok())
        .unwrap_or_default();
    done.store(true, Ordering::SeqCst);
    let code = status.ok().and_then(|status| status.code());
    let _ = events.unbounded_send(ProcessEvent::Exited { code, stderr_tail });
}

/// Send each line of `stdout`; one over `MAX_LINE` is skipped (never buffered past the cap)
/// and replaced by `OVERLONG_MARKER`.
fn read_lines(stdout: impl Read, events: &UnboundedSender<ProcessEvent>) {
    let mut reader = std::io::BufReader::with_capacity(64 * 1024, stdout);
    let mut line: Vec<u8> = Vec::new();
    let mut overlong = false;
    loop {
        let (consumed, newline) = match reader.fill_buf() {
            Ok([]) | Err(_) => break,
            Ok(buf) => match buf.iter().position(|b| *b == b'\n') {
                Some(at) => (at + 1, Some(at)),
                None => (buf.len(), None),
            },
        };
        let buf = reader.buffer();
        let chunk = &buf[..newline.unwrap_or(consumed)];
        if !overlong {
            if line.len() + chunk.len() > MAX_LINE {
                overlong = true;
                line = Vec::new();
            } else {
                line.extend_from_slice(chunk);
            }
        }
        reader.consume(consumed);
        if newline.is_some() {
            send_line(&mut line, &mut overlong, events);
        }
    }
    if overlong || !line.is_empty() {
        send_line(&mut line, &mut overlong, events); // final line without a newline
    }
}

fn send_line(line: &mut Vec<u8>, overlong: &mut bool, events: &UnboundedSender<ProcessEvent>) {
    let text = if std::mem::take(overlong) {
        OVERLONG_MARKER.to_string()
    } else {
        if line.last() == Some(&b'\r') {
            line.pop();
        }
        String::from_utf8_lossy(line).into_owned()
    };
    line.clear();
    let _ = events.unbounded_send(ProcessEvent::Line(text));
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::channel::mpsc::{TryRecvError, UnboundedReceiver, unbounded};

    fn argv(script: &str) -> Vec<String> {
        vec!["/bin/sh".into(), "-c".into(), script.into()]
    }

    fn start(argv: &[String]) -> (AgentProcess, UnboundedReceiver<ProcessEvent>) {
        let (tx, rx) = unbounded();
        (spawn(argv, Path::new("/"), None, tx), rx)
    }

    /// The next event, or panic after 5 s.
    fn next(rx: &mut UnboundedReceiver<ProcessEvent>) -> ProcessEvent {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match rx.try_recv() {
                Ok(event) => return event,
                Err(TryRecvError::Closed) => panic!("event channel closed"),
                Err(TryRecvError::Empty) if Instant::now() > deadline => {
                    panic!("timed out waiting for an event")
                }
                Err(TryRecvError::Empty) => std::thread::sleep(Duration::from_millis(5)),
            }
        }
    }

    fn line(text: &str) -> ProcessEvent {
        ProcessEvent::Line(text.to_string())
    }

    #[test]
    fn echoes_lines_back() {
        let (process, mut rx) = start(&argv("cat"));
        process.send("a".into());
        process.send("b".into());
        assert_eq!(next(&mut rx), line("a"));
        assert_eq!(next(&mut rx), line("b"));
    }

    #[test]
    fn lines_survive_split_reads() {
        let (_process, mut rx) = start(&argv(r#"printf '{"x":'; sleep 0.2; printf '1}\n'"#));
        assert_eq!(next(&mut rx), line(r#"{"x":1}"#));
        assert!(matches!(next(&mut rx), ProcessEvent::Exited { .. }));
    }

    #[test]
    fn overlong_line_dropped() {
        let (_process, mut rx) = start(&argv(
            "head -c 9437184 /dev/zero | tr '\\0' a; echo; echo ok",
        ));
        assert_eq!(next(&mut rx), line(OVERLONG_MARKER));
        assert_eq!(next(&mut rx), line("ok"));
        assert!(matches!(
            next(&mut rx),
            ProcessEvent::Exited { code: Some(0), .. }
        ));
    }

    #[test]
    fn exit_code_and_stderr_reported() {
        let (_process, mut rx) = start(&argv("echo out; echo nope >&2; exit 3"));
        assert_eq!(next(&mut rx), line("out"));
        match next(&mut rx) {
            ProcessEvent::Exited { code, stderr_tail } => {
                assert_eq!(code, Some(3));
                assert!(stderr_tail.contains("nope"), "{stderr_tail:?}");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn missing_program_is_spawn_failed() {
        let (_process, mut rx) = start(&["/nonexistent/agent".to_string()]);
        assert!(matches!(next(&mut rx), ProcessEvent::SpawnFailed(_)));
    }

    #[test]
    fn kill_ends_process_group() {
        let (process, mut rx) = start(&argv("sleep 100 & sleep 100"));
        let deadline = Instant::now() + Duration::from_secs(5);
        while process.pid().is_none() {
            assert!(Instant::now() < deadline, "never spawned");
            std::thread::sleep(Duration::from_millis(5));
        }
        std::thread::sleep(Duration::from_millis(300)); // let sh fork its sleeps
        let pgid = process.pid().unwrap();
        process.kill();
        assert!(matches!(next(&mut rx), ProcessEvent::Exited { .. }));
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            // SAFETY: signal 0 only probes whether the group exists.
            let gone = unsafe { kill(-pgid, 0) } == -1
                && std::io::Error::last_os_error().raw_os_error() == Some(3); // ESRCH
            if gone {
                break;
            }
            assert!(Instant::now() < deadline, "process group still alive");
            std::thread::sleep(Duration::from_millis(25));
        }
    }
}
