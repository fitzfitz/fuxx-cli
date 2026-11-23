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
/// How many stderr lines are sent one by one (`ErrLine`); a chatty agent's later ones only
/// reach the tail.
const ERR_LINES: usize = 64;
/// The longest stderr line sent; a longer one is cut.
const ERR_LINE_MAX: usize = 1024;
/// How long a process group gets after SIGTERM before SIGKILL.
pub(crate) const TERM_GRACE: Duration = Duration::from_secs(2);

unsafe extern "C" {
    fn setsid() -> c_int;
    fn kill(pid: c_int, sig: c_int) -> c_int;
}

const SIGTERM: c_int = 15;
const SIGKILL: c_int = 9;

/// Every signal `signal_group` sent (tests assert on it).
#[cfg(test)]
static SENT: std::sync::Mutex<Vec<(i32, c_int)>> = std::sync::Mutex::new(Vec::new());

/// Signal process group `pgid`. The one place signals leave this module.
fn signal_group(pgid: i32, sig: c_int) {
    #[cfg(test)]
    SENT.lock().unwrap().push((pgid, sig));
    // SAFETY: a plain signal to a group we created with setsid, only called while its leader
    // is unreaped or (for the final sweep) just reaped, so the id can't have been reused.
    unsafe { kill(-pgid, sig) };
}

/// SIGTERM the group, then SIGKILL it after `TERM_GRACE` unless the leader is reaped by then.
fn terminate_group(pgid: i32, done: Arc<AtomicBool>) {
    signal_group(pgid, SIGTERM);
    escalate_later(pgid, done);
}

/// SIGKILL the group after `TERM_GRACE` unless the leader is reaped by then (on a thread).
fn escalate_later(pgid: i32, done: Arc<AtomicBool>) {
    let _ = std::thread::Builder::new()
        .name("fuxx-agent-killer".into())
        .spawn(move || {
            let until = Instant::now() + TERM_GRACE;
            while Instant::now() < until {
                if done.load(Ordering::SeqCst) {
                    return;
                }
                std::thread::sleep(Duration::from_millis(25));
            }
            if !done.load(Ordering::SeqCst) {
                signal_group(pgid, SIGKILL);
            }
        });
}

/// After `end_all`'s SIGKILL, how long it waits for those children to be reaped.
const KILL_WAIT: Duration = Duration::from_secs(1);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProcessEvent {
    /// One stdout line (without its newline).
    Line(String),
    /// The child was reaped (its exit code is known); sent at most once, right after the wait
    /// and before the output is drained, so a straggler holding stdout open delays `Exited` but
    /// not this. Lines may still follow it. Not sent when the child was never started (killed
    /// before spawn: only `Exited { code: None }` comes) or on `SpawnFailed`.
    Ended { code: Option<i32> },
    /// The process ended and stdout was fully read; sent exactly once, last, after every `Line`
    /// and any `Ended` (not after `SpawnFailed`).
    Exited {
        code: Option<i32>,
        stderr_tail: String,
    },
    /// The process couldn't be started.
    SpawnFailed(String),
    /// One stderr line (without its newline), as it comes: the first `ERR_LINES` of them, each
    /// cut to `ERR_LINE_MAX` bytes (agy warns there that it refused a mode; the tail still
    /// comes with `Exited`).
    ErrLine(String),
}

pub struct AgentProcess {
    ids: Ids,
    writer: mpsc::Sender<String>,
}

/// What it takes to end a process: shared by its `AgentProcess`, its own thread and the list
/// of every process started (`STARTED`), which outlives a closed tab's handle (quitting ends
/// those still dying: `end_every_started`).
#[derive(Clone)]
struct Ids {
    /// The child's pid (also its process group id); 0 until spawned.
    pid: Arc<AtomicI32>,
    /// Set by `kill`, so a child spawned after it is killed at once.
    killed: Arc<AtomicBool>,
    /// Set once the child is reaped (or was never started); `kill` then signals nothing.
    done: Arc<AtomicBool>,
}

impl Ids {
    /// Signal its group with `sig` while its child runs (spawned, not reaped yet): the group
    /// id, if it was signalled. The one check `kill` and `end_all` share.
    fn signal_running(&self, sig: c_int) -> Option<i32> {
        if self.done.load(Ordering::SeqCst) {
            return None;
        }
        let pgid = self.pid.load(Ordering::SeqCst);
        (pgid > 0).then(|| {
            signal_group(pgid, sig);
            pgid
        })
    }

    fn reaped(&self) -> bool {
        self.done.load(Ordering::SeqCst)
    }
}

/// Every process this app started and hasn't seen reaped yet (pruned as more start).
static STARTED: std::sync::Mutex<Vec<Ids>> = std::sync::Mutex::new(Vec::new());

/// Start `argv` in `cwd` (with `PATH` = `path_env` if given, and `env` set too); never blocks.
/// Events arrive on `events`.
pub fn spawn(
    argv: &[String],
    cwd: &Path,
    path_env: Option<&str>,
    env: &[(String, String)],
    events: UnboundedSender<ProcessEvent>,
) -> AgentProcess {
    let pid = Arc::new(AtomicI32::new(0));
    let killed = Arc::new(AtomicBool::new(false));
    let done = Arc::new(AtomicBool::new(false));
    let (writer, lines) = mpsc::channel::<String>();
    let process = AgentProcess {
        ids: Ids {
            pid: pid.clone(),
            killed: killed.clone(),
            done: done.clone(),
        },
        writer,
    };
    if let Ok(mut started) = STARTED.lock() {
        started.retain(|ids| !ids.reaped());
        started.push(process.ids.clone());
    }
    let argv = argv.to_vec();
    let cwd = cwd.to_path_buf();
    let env: Vec<(String, String)> = path_env
        .map(|path| ("PATH".to_string(), path.to_string()))
        .into_iter()
        .chain(env.iter().cloned())
        .collect();
    let started = std::thread::Builder::new()
        .name("fuxx-agent".into())
        .spawn({
            let events = events.clone();
            move || run(argv, cwd, env, events, lines, pid, killed, done)
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

    /// The child's pid (its process group's id), once it is running.
    pub fn pid(&self) -> Option<i32> {
        Some(self.ids.pid.load(Ordering::SeqCst)).filter(|pid| *pid > 0)
    }

    /// Whether its child has been reaped (or never started): it signals nothing any more.
    pub fn reaped(&self) -> bool {
        self.ids.reaped()
    }

    /// SIGTERM the process group, SIGKILL it after `TERM_GRACE`; the child is reaped by its
    /// own thread, which then sends `Exited`.
    pub fn kill(&self) {
        if self.ids.killed.swap(true, Ordering::SeqCst) {
            return; // already asked
        }
        if let Some(pgid) = self.ids.signal_running(SIGTERM) {
            escalate_later(pgid, self.ids.done.clone());
        } // else reaped (its group id may belong to someone else now), or not spawned yet:
        // the spawning thread sees `killed`
    }
}

/// End every process in `processes`: SIGTERM each group, wait up to `grace` for each child to
/// be reaped, then SIGKILL the groups of those that weren't (and wait a moment for those to be
/// reaped). Blocking (quitting calls it on the UI thread on purpose: GPUI gives a quit task
/// only 200 ms). A process already reaped is not signalled (its group id may be reused); it
/// waits on the reap, not on `Exited` (a straggler holding stdout can hold that back).
#[cfg(test)] // quitting uses `end_every_started`, which also ends closed tabs' processes
pub fn end_all(processes: &[Arc<AgentProcess>], grace: Duration) {
    let ids: Vec<Ids> = processes.iter().map(|p| p.ids.clone()).collect();
    end_ids(&ids, grace);
}

/// The pids of every process this app started that isn't reaped yet (each leads its own
/// process group): fuxx's own agents, never a classic view's (R13).
pub fn started_pids() -> Vec<u32> {
    STARTED
        .lock()
        .map(|started| {
            started
                .iter()
                .filter(|i| !i.reaped())
                .map(|i| i.pid.load(Ordering::SeqCst))
                .filter(|pid| *pid > 0)
                .map(|pid| pid as u32)
                .collect()
        })
        .unwrap_or_default()
}

/// `end_all` for every process this app started that isn't reaped yet: the open tabs', and
/// those of tabs closed or paused moments ago still waiting for their SIGKILL (quitting).
pub fn end_every_started(grace: Duration) {
    let ids: Vec<Ids> = STARTED
        .lock()
        .map(|started| started.iter().filter(|i| !i.reaped()).cloned().collect())
        .unwrap_or_default();
    end_ids(&ids, grace);
}

fn end_ids(processes: &[Ids], grace: Duration) {
    for p in processes {
        p.killed.store(true, Ordering::SeqCst);
        p.signal_running(SIGTERM);
    }
    let all_reaped = |until: Instant| loop {
        if processes.iter().all(Ids::reaped) {
            return true;
        }
        if Instant::now() >= until {
            return false;
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    if all_reaped(Instant::now() + grace) {
        return;
    }
    for p in processes {
        p.signal_running(SIGKILL);
    }
    all_reaped(Instant::now() + KILL_WAIT);
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
    env: Vec<(String, String)>,
    events: UnboundedSender<ProcessEvent>,
    lines: mpsc::Receiver<String>,
    pid: Arc<AtomicI32>,
    killed: Arc<AtomicBool>,
    done: Arc<AtomicBool>,
) {
    use std::os::unix::process::CommandExt;
    if killed.load(Ordering::SeqCst) {
        done.store(true, Ordering::SeqCst);
        let _ = events.unbounded_send(ProcessEvent::Exited {
            code: None,
            stderr_tail: String::new(),
        });
        return;
    }
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
    command.envs(env);
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
        terminate_group(child.id() as i32, done.clone());
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
    let err_reader = child.stderr.take().map(|stderr| {
        let events = events.clone();
        std::thread::Builder::new()
            .name("fuxx-agent-stderr".into())
            .spawn(move || read_tail(stderr, &events))
    });
    let status = child.wait(); // reaps
    let was_killed = killed.load(Ordering::SeqCst);
    // From here `kill` must not signal the group id again.
    done.store(true, Ordering::SeqCst);
    let code = status.ok().and_then(|status| status.code());
    let _ = events.unbounded_send(ProcessEvent::Ended { code });
    if was_killed {
        // Stragglers of the group could hold the pipes open; the leader is gone, not the group.
        signal_group(child.id() as i32, SIGKILL);
    }
    if let Some(Ok(reader)) = reader {
        let _ = reader.join();
    }
    let stderr_tail = err_reader
        .and_then(Result::ok)
        .and_then(|handle| handle.join().ok())
        .unwrap_or_default();
    let _ = events.unbounded_send(ProcessEvent::Exited { code, stderr_tail });
}

/// The last `STDERR_TAIL` bytes of `stderr`, as text; its first `ERR_LINES` lines are sent as
/// they come (`ErrLine`).
fn read_tail(mut stderr: impl Read, events: &UnboundedSender<ProcessEvent>) -> String {
    let mut tail: Vec<u8> = Vec::new();
    let mut buf = [0u8; 4096];
    let mut line: Vec<u8> = Vec::new();
    let mut sent = 0;
    loop {
        match stderr.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                for &b in &buf[..n] {
                    if sent >= ERR_LINES {
                        break;
                    }
                    if b == b'\n' {
                        let text = String::from_utf8_lossy(&line);
                        let _ = events
                            .unbounded_send(ProcessEvent::ErrLine(text.trim_end().to_string()));
                        line.clear();
                        sent += 1;
                    } else if line.len() < ERR_LINE_MAX {
                        line.push(b);
                    }
                }
                tail.extend_from_slice(&buf[..n]);
                if tail.len() > STDERR_TAIL {
                    tail.drain(..tail.len() - STDERR_TAIL);
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::Interrupted => {}
            Err(_) => break,
        }
    }
    String::from_utf8_lossy(&tail).into_owned()
}

/// Send each line of `stdout`; one over `MAX_LINE` is skipped (never buffered past the cap)
/// and replaced by `OVERLONG_MARKER`.
fn read_lines(stdout: impl Read, events: &UnboundedSender<ProcessEvent>) {
    let mut reader = std::io::BufReader::with_capacity(64 * 1024, stdout);
    let mut line: Vec<u8> = Vec::new();
    let mut overlong = false;
    loop {
        let (consumed, newline) = match reader.fill_buf() {
            Ok([]) => break,
            Err(err) if err.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => break,
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
        (spawn(argv, Path::new("/"), None, &[], tx), rx)
    }

    /// The next event other than `Ended`, or panic after 5 s.
    fn next(rx: &mut UnboundedReceiver<ProcessEvent>) -> ProcessEvent {
        loop {
            match next_raw(rx) {
                ProcessEvent::Ended { .. } | ProcessEvent::ErrLine(_) => {}
                event => return event,
            }
        }
    }

    /// The next event, `Ended` included, or panic after 5 s.
    fn next_raw(rx: &mut UnboundedReceiver<ProcessEvent>) -> ProcessEvent {
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

    /// The provider's environment (prompt suggestions' switch) reaches the agent's process.
    #[test]
    fn extra_env_reaches_the_process() {
        let (tx, mut rx) = unbounded();
        let env = vec![("FUXX_TEST_SUGGEST".to_string(), "1".to_string())];
        let _process = spawn(
            &argv("echo \"[$FUXX_TEST_SUGGEST]\""),
            Path::new("/"),
            None,
            &env,
            tx,
        );
        assert_eq!(next(&mut rx), line("[1]"));
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

    /// 2026-11-24: stderr lines come as they are written (agy's refused mode), before the
    /// end, and still in the tail.
    #[test]
    fn stderr_lines_come_one_by_one() {
        let (_process, mut rx) = start(&argv("echo warn one >&2; echo warn two >&2; exit 0"));
        let mut lines = Vec::new();
        let tail = loop {
            match next_raw(&mut rx) {
                ProcessEvent::ErrLine(l) => lines.push(l),
                ProcessEvent::Exited { stderr_tail, .. } => break stderr_tail,
                _ => {}
            }
        };
        assert_eq!(lines, ["warn one", "warn two"]);
        assert!(tail.contains("warn two"), "{tail:?}");
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
    fn ended_comes_once_before_exited_and_nothing_follows() {
        let (_process, mut rx) = start(&argv("echo out; exit 4"));
        let mut events = Vec::new();
        loop {
            let event = next_raw(&mut rx);
            let exited = matches!(event, ProcessEvent::Exited { .. });
            events.push(event);
            if exited {
                break;
            }
        }
        let ended: Vec<usize> = (0..events.len())
            .filter(|i| matches!(events[*i], ProcessEvent::Ended { .. }))
            .collect();
        assert_eq!(ended.len(), 1, "{events:?}");
        assert_eq!(events[ended[0]], ProcessEvent::Ended { code: Some(4) });
        assert!(matches!(
            events.last(),
            Some(ProcessEvent::Exited { code: Some(4), .. })
        ));
        assert!(events.contains(&line("out")));
        std::thread::sleep(Duration::from_millis(100));
        assert!(matches!(
            rx.try_recv(),
            Err(TryRecvError::Empty | TryRecvError::Closed)
        ));
    }

    #[test]
    fn a_straggler_delays_exited_but_not_ended() {
        // The shell exits at once; the backgrounded sleep keeps stdout open for ~1 s.
        let (_process, mut rx) = start(&argv("sleep 1 & exit 0"));
        let began = Instant::now();
        assert_eq!(next_raw(&mut rx), ProcessEvent::Ended { code: Some(0) });
        assert!(began.elapsed() < Duration::from_millis(800));
        assert!(matches!(next_raw(&mut rx), ProcessEvent::Exited { .. }));
    }

    #[test]
    fn missing_program_is_spawn_failed() {
        let (_process, mut rx) = start(&["/nonexistent/agent".to_string()]);
        assert!(matches!(next(&mut rx), ProcessEvent::SpawnFailed(_)));
    }

    /// Reads `data` after failing once with EINTR.
    struct InterruptedOnce<'a> {
        data: &'a [u8],
        interrupted: bool,
    }

    impl Read for InterruptedOnce<'_> {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            if !self.interrupted {
                self.interrupted = true;
                return Err(std::io::ErrorKind::Interrupted.into());
            }
            self.data.read(buf)
        }
    }

    #[test]
    fn eintr_does_not_end_reading() {
        let (tx, mut rx) = unbounded();
        read_lines(
            InterruptedOnce {
                data: b"a\nb\n",
                interrupted: false,
            },
            &tx,
        );
        assert_eq!(next(&mut rx), line("a"));
        assert_eq!(next(&mut rx), line("b"));
        let tail = read_tail(
            InterruptedOnce {
                data: b"oops",
                interrupted: false,
            },
            &tx,
        );
        assert_eq!(tail, "oops");
    }

    fn group_gone(pgid: i32, within: Duration) -> bool {
        let deadline = Instant::now() + within;
        loop {
            // SAFETY: signal 0 only probes whether the group exists.
            if unsafe { kill(-pgid, 0) } == -1
                && std::io::Error::last_os_error().raw_os_error() == Some(3)
            {
                return true; // ESRCH
            }
            if Instant::now() > deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
    }

    fn sent_to(pgid: i32) -> usize {
        SENT.lock()
            .unwrap()
            .iter()
            .filter(|(p, _)| *p == pgid)
            .count()
    }

    #[test]
    fn kill_after_exit_signals_nothing() {
        let (process, mut rx) = start(&argv("exit 0"));
        assert!(matches!(next(&mut rx), ProcessEvent::Exited { .. }));
        let pgid = process.pid().unwrap();
        process.kill();
        drop(process);
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(sent_to(pgid), 0, "signalled a reaped group");
    }

    #[test]
    fn kill_escalates_when_term_is_ignored() {
        let (process, mut rx) = start(&argv("trap '' TERM; sleep 100"));
        let deadline = Instant::now() + Duration::from_secs(5);
        while process.pid().is_none() {
            assert!(Instant::now() < deadline, "never spawned");
            std::thread::sleep(Duration::from_millis(5));
        }
        std::thread::sleep(Duration::from_millis(300)); // let the trap be set
        let pgid = process.pid().unwrap();
        process.kill();
        assert!(matches!(next(&mut rx), ProcessEvent::Exited { .. }));
        assert!(group_gone(pgid, Duration::from_secs(3)));
    }

    #[test]
    fn kill_right_after_spawn_escalates() {
        let (process, mut rx) = start(&argv("trap '' TERM; sleep 100"));
        process.kill(); // races the child's start
        assert!(matches!(next(&mut rx), ProcessEvent::Exited { .. }));
        if let Some(pgid) = process.pid() {
            assert!(group_gone(pgid, Duration::from_secs(3)));
        }
    }

    #[test]
    fn end_all_ends_a_term_ignoring_agent() {
        let spawned = |p: &AgentProcess| {
            let deadline = Instant::now() + Duration::from_secs(5);
            while p.pid().is_none() {
                assert!(Instant::now() < deadline, "never spawned");
                std::thread::sleep(Duration::from_millis(5));
            }
        };
        let (a, mut rx_a) = start(&argv("trap '' TERM; sleep 100"));
        let (b, mut rx_b) = start(&argv("trap '' TERM; sleep 100"));
        spawned(&a);
        spawned(&b);
        std::thread::sleep(Duration::from_millis(300)); // let the traps be set
        let (pa, pb) = (a.pid().unwrap(), b.pid().unwrap());
        let (a, b) = (Arc::new(a), Arc::new(b));
        let began = Instant::now();
        end_all(&[a.clone(), b.clone()], Duration::from_secs(2));
        assert!(
            began.elapsed() < Duration::from_secs(3),
            "{:?}",
            began.elapsed()
        );
        // `end_all` waited for the reaps; the groups' last members (SIGKILLed with them) are
        // gone a moment later.
        for pgid in [pa, pb] {
            assert!(
                group_gone(pgid, Duration::from_millis(500)),
                "group {pgid} still alive"
            );
        }
        assert!(matches!(next(&mut rx_a), ProcessEvent::Exited { .. }));
        assert!(matches!(next(&mut rx_b), ProcessEvent::Exited { .. }));
    }

    #[test]
    fn quitting_ends_a_closed_tabs_agent_still_dying() {
        let (process, mut rx) = start(&argv("trap '' TERM; sleep 100"));
        let deadline = Instant::now() + Duration::from_secs(5);
        while process.pid().is_none() {
            assert!(Instant::now() < deadline, "never spawned");
            std::thread::sleep(Duration::from_millis(5));
        }
        std::thread::sleep(Duration::from_millis(300)); // let the trap be set
        let pgid = process.pid().unwrap();
        // The tab closed: its handle is gone; the killer thread would SIGKILL after 2 s.
        drop(process);
        let ids: Vec<Ids> = STARTED
            .lock()
            .unwrap()
            .iter()
            .filter(|i| i.pid.load(Ordering::SeqCst) == pgid)
            .cloned()
            .collect();
        assert_eq!(ids.len(), 1, "still listed as started");
        let began = Instant::now();
        end_ids(&ids, Duration::from_millis(200));
        assert!(
            began.elapsed() < Duration::from_millis(1500),
            "{:?}",
            began.elapsed()
        );
        assert!(
            group_gone(pgid, Duration::from_millis(500)),
            "ended at quit"
        );
        assert!(matches!(next(&mut rx), ProcessEvent::Exited { .. }));
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
