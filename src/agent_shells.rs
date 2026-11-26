//! An agent's background shells (chore 16, note 93): what the shells tab (`▣ 1/3`), its popover
//! and the chat's "running in the background" row read. Pure and GPUI-free; the tab and the
//! popover are `agent_shells_view`.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::providers::ShellEnd;

/// A finished shell's row leaves the popover this long after it ended (the tab still counts it
/// until your next message).
pub const DROP_AFTER: Duration = Duration::from_secs(120);

/// How much of an output file's end is read for its last line.
pub const TAIL_BYTES: u64 = 4096;

/// The longest last line a row keeps, in characters (the row truncates it to its width).
const LINE_MAX: usize = 200;

/// One background shell.
#[derive(Clone, Debug, PartialEq)]
pub struct Shell {
    /// Its task id (what ■ stops).
    pub id: String,
    /// The tool call that started it (its chat row).
    pub call: String,
    pub description: String,
    pub command: String,
    /// The file its output goes to, when the agent said.
    pub output: Option<PathBuf>,
    pub started: Instant,
    pub ended: Option<(ShellEnd, Instant)>,
    /// Its output's last line, as last read.
    pub last_line: Option<String>,
}

impl Shell {
    pub fn running(&self) -> bool {
        self.ended.is_none()
    }

    /// How long it ran (so far, while it runs).
    pub fn elapsed(&self, now: Instant) -> Duration {
        let to = self.ended.map_or(now, |(_, at)| at);
        to.saturating_duration_since(self.started)
    }
}

/// The tab's colour: peach while any runs, green once all are done, red after a failure you
/// haven't opened the popover on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Running,
    Done,
    Failed,
}

/// This conversation's shells since your last message (and any still running from before).
#[derive(Clone, Debug, Default)]
pub struct Shells {
    list: Vec<Shell>,
    /// A shell failed since you last opened the popover.
    failure_unseen: bool,
}

impl Shells {
    pub fn start(&mut self, shell: Shell) {
        match self.list.iter_mut().find(|s| s.id == shell.id) {
            Some(old) => *old = shell,
            None => self.list.push(shell),
        }
    }

    /// Shell `id` ended `end` at `now` (`output`: its file, when said; else the start's stays).
    /// The shell, when it was running (for the banner).
    pub fn end(
        &mut self,
        id: &str,
        end: ShellEnd,
        output: Option<PathBuf>,
        now: Instant,
    ) -> Option<&Shell> {
        let shell = self.list.iter_mut().find(|s| s.id == id && s.running())?;
        shell.ended = Some((end, now));
        if output.is_some() {
            shell.output = output;
        }
        self.failure_unseen |= end == ShellEnd::Failed;
        Some(shell)
    }

    /// Your message: the finished shells go (the running ones stay).
    pub fn you_sent(&mut self) {
        self.list.retain(Shell::running);
        self.failure_unseen = false;
    }

    /// The agent's process is gone: what still runs is stopped (no banner).
    pub fn gone(&mut self, now: Instant) {
        for shell in self.list.iter_mut().filter(|s| s.running()) {
            shell.ended = Some((ShellEnd::Stopped, now));
        }
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    /// (done, all): every ending counts as done.
    pub fn counts(&self) -> (usize, usize) {
        let done = self.list.iter().filter(|s| !s.running()).count();
        (done, self.list.len())
    }

    pub fn tone(&self) -> Tone {
        if self.failure_unseen {
            Tone::Failed
        } else if self.list.iter().any(Shell::running) {
            Tone::Running
        } else {
            Tone::Done
        }
    }

    /// The popover was opened: a failure is seen.
    pub fn seen(&mut self) {
        self.failure_unseen = false;
    }

    /// The popover's rows: the running ones, then the finished ones under `DROP_AFTER`, each in
    /// start order.
    pub fn rows(&self, now: Instant) -> Vec<&Shell> {
        let running = self.list.iter().filter(|s| s.running());
        let recent = self.list.iter().filter(|s| {
            s.ended
                .is_some_and(|(_, at)| now.saturating_duration_since(at) < DROP_AFTER)
        });
        running.chain(recent).collect()
    }

    /// The popover's header: "1 of 3 done · 2 running" (and "· 1 failed").
    pub fn header(&self) -> String {
        let (done, all) = self.counts();
        let running = all - done;
        let failed = self
            .list
            .iter()
            .filter(|s| matches!(s.ended, Some((ShellEnd::Failed, _))))
            .count();
        let mut words = format!("{done} of {all} done");
        if running > 0 {
            words.push_str(&format!(" \u{b7} {running} running"));
        }
        if failed > 0 {
            words.push_str(&format!(" \u{b7} {failed} failed"));
        }
        words
    }

    /// The running shell started by tool call `call` (its chat row).
    pub fn running_for_call(&self, call: &str) -> Option<&Shell> {
        self.list.iter().find(|s| s.call == call && s.running())
    }

    pub fn get(&self, id: &str) -> Option<&Shell> {
        self.list.iter().find(|s| s.id == id)
    }

    pub fn set_last_line(&mut self, id: &str, line: Option<String>) {
        if let Some(shell) = self.list.iter_mut().find(|s| s.id == id) {
            shell.last_line = line;
        }
    }

    /// The rows' output files, to read their last lines from.
    pub fn row_files(&self, now: Instant) -> Vec<(String, PathBuf)> {
        self.rows(now)
            .into_iter()
            .filter_map(|s| Some((s.id.clone(), s.output.clone()?)))
            .collect()
    }

    /// Every shell (scenes date them back).
    #[cfg(feature = "ui-shots")]
    pub fn list_mut(&mut self) -> &mut Vec<Shell> {
        &mut self.list
    }
}

/// The tab's words: "1/3", "3/3 ✓" all done, "1/2 ✕" after a failure.
pub fn tab_words(shells: &Shells) -> String {
    let (done, all) = shells.counts();
    match shells.tone() {
        Tone::Failed => format!("{done}/{all} \u{2715}"),
        Tone::Done => format!("{done}/{all} \u{2713}"),
        Tone::Running => format!("{done}/{all}"),
    }
}

/// A Bash row's right-hand words while its shell runs.
pub fn row_words(elapsed: Duration) -> String {
    format!(
        "running in the background \u{b7} {}",
        crate::agent_lanes::clock(elapsed)
    )
}

/// The banner's text when a shell ends: "Build · done in 6:40" / "Build · failed".
pub fn banner_words(shell: &Shell) -> String {
    let how = match shell.ended {
        Some((ShellEnd::Failed, _)) => "failed".to_string(),
        Some((ShellEnd::Stopped, _)) => "stopped".to_string(),
        Some((ShellEnd::Done, at)) => format!(
            "done in {}",
            crate::agent_lanes::clock(at.saturating_duration_since(shell.started))
        ),
        None => "running".to_string(),
    };
    format!("{} \u{b7} {how}", shell.description)
}

/// An output's last line from its file's end: the last line with words, after any `\r`
/// rewrite (a progress bar) and without terminal colours.
pub fn last_line(tail: &str) -> Option<String> {
    tail.lines().rev().find_map(|line| {
        let shown = line.rsplit('\r').next().unwrap_or(line);
        let plain = strip_colours(shown);
        let words = plain.trim();
        (!words.is_empty()).then(|| words.chars().take(LINE_MAX).collect())
    })
}

/// `text` without its terminal escape sequences (`ESC [ … letter`, and a lone `ESC x`).
fn strip_colours(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            out.push(c);
            continue;
        }
        if chars.next() == Some('[') {
            for c in chars.by_ref() {
                if c.is_ascii_alphabetic() || c == '~' {
                    break;
                }
            }
        }
    }
    out
}

/// The last `TAIL_BYTES` of file `path` (blocking: run it off the UI thread).
pub fn read_tail(path: &Path) -> Option<String> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = std::fs::File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    file.seek(SeekFrom::Start(len.saturating_sub(TAIL_BYTES)))
        .ok()?;
    let mut bytes = Vec::new();
    file.take(TAIL_BYTES).read_to_end(&mut bytes).ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shell(id: &str, at: Instant) -> Shell {
        Shell {
            id: id.into(),
            call: format!("call-{id}"),
            description: format!("Run {id}"),
            command: "cargo test".into(),
            output: Some(PathBuf::from(format!("/tmp/fx/{id}.output"))),
            started: at,
            ended: None,
            last_line: None,
        }
    }

    fn three(now: Instant) -> Shells {
        let mut s = Shells::default();
        for id in ["a", "b", "c"] {
            s.start(shell(id, now));
        }
        s
    }

    #[test]
    fn counts_and_tone_follow_the_shells() {
        let t0 = Instant::now();
        let mut s = three(t0);
        assert_eq!((s.counts(), s.tone()), ((0, 3), Tone::Running));
        assert_eq!(tab_words(&s), "0/3");
        let ended = s.end("a", ShellEnd::Done, None, t0 + Duration::from_secs(400));
        assert_eq!(ended.map(|e| e.id.as_str()), Some("a"));
        assert_eq!(s.header(), "1 of 3 done \u{b7} 2 running");
        s.end("b", ShellEnd::Stopped, None, t0);
        s.end("c", ShellEnd::Done, None, t0);
        assert_eq!((s.counts(), s.tone()), ((3, 3), Tone::Done));
        assert_eq!(tab_words(&s), "3/3 \u{2713}");
        assert_eq!(s.header(), "3 of 3 done");
        // An end told twice (or of no shell) is no news.
        assert!(s.end("a", ShellEnd::Done, None, t0).is_none());
        assert!(s.end("zz", ShellEnd::Done, None, t0).is_none());
    }

    #[test]
    fn your_message_clears_the_finished_and_keeps_the_running() {
        let t0 = Instant::now();
        let mut s = three(t0);
        s.end("a", ShellEnd::Done, None, t0);
        s.you_sent();
        assert_eq!(s.counts(), (0, 2), "a dev server started before stays");
        s.end("b", ShellEnd::Done, None, t0);
        s.end("c", ShellEnd::Done, None, t0);
        s.you_sent();
        assert!(s.is_empty(), "the tab goes with your next message");
    }

    #[test]
    fn a_failure_is_red_until_the_popover_opens() {
        let t0 = Instant::now();
        let mut s = three(t0);
        s.end("a", ShellEnd::Failed, None, t0);
        assert_eq!(s.tone(), Tone::Failed);
        assert_eq!(tab_words(&s), "1/3 \u{2715}");
        assert_eq!(s.header(), "1 of 3 done \u{b7} 2 running \u{b7} 1 failed");
        s.seen();
        assert_eq!(s.tone(), Tone::Running);
        assert_eq!(tab_words(&s), "1/3");
        // A stop is no failure.
        s.end("b", ShellEnd::Stopped, None, t0);
        assert_eq!(s.tone(), Tone::Running);
    }

    #[test]
    fn finished_rows_drop_off_after_two_minutes() {
        let t0 = Instant::now();
        let mut s = three(t0);
        s.end("a", ShellEnd::Done, None, t0 + Duration::from_secs(10));
        let ids = |rows: Vec<&Shell>| rows.iter().map(|r| r.id.clone()).collect::<Vec<_>>();
        assert_eq!(ids(s.rows(t0 + Duration::from_secs(20))), ["b", "c", "a"]);
        assert_eq!(ids(s.rows(t0 + Duration::from_secs(131))), ["b", "c"]);
        assert_eq!(s.counts(), (1, 3), "the tab still counts it");
        assert_eq!(s.row_files(t0 + Duration::from_secs(131)).len(), 2);
    }

    #[test]
    fn the_end_keeps_the_starts_file_when_it_names_none() {
        let t0 = Instant::now();
        let mut s = three(t0);
        s.end("a", ShellEnd::Done, None, t0);
        assert_eq!(
            s.get("a").and_then(|a| a.output.clone()),
            Some(PathBuf::from("/tmp/fx/a.output"))
        );
        s.end("b", ShellEnd::Done, Some(PathBuf::from("/tmp/fx/new")), t0);
        assert_eq!(
            s.get("b").and_then(|b| b.output.clone()),
            Some(PathBuf::from("/tmp/fx/new"))
        );
    }

    #[test]
    fn the_process_gone_stops_what_runs() {
        let t0 = Instant::now();
        let mut s = three(t0);
        s.end("a", ShellEnd::Failed, None, t0);
        s.seen();
        s.gone(t0);
        assert_eq!(s.counts(), (3, 3));
        assert!(matches!(
            s.get("b").and_then(|b| b.ended),
            Some((ShellEnd::Stopped, _))
        ));
        assert_eq!(s.tone(), Tone::Done);
    }

    #[test]
    fn the_chat_row_finds_a_running_shell_by_its_call() {
        let t0 = Instant::now();
        let mut s = three(t0);
        assert_eq!(
            s.running_for_call("call-a").map(|a| a.id.as_str()),
            Some("a")
        );
        s.end("a", ShellEnd::Done, None, t0);
        assert!(s.running_for_call("call-a").is_none());
        assert_eq!(
            row_words(Duration::from_secs(252)),
            "running in the background \u{b7} 4:12"
        );
    }

    #[test]
    fn banner_words_say_how_it_ended() {
        let t0 = Instant::now();
        let mut s = three(t0);
        s.end("a", ShellEnd::Done, None, t0 + Duration::from_secs(400));
        s.end("b", ShellEnd::Failed, None, t0 + Duration::from_secs(5));
        s.end("c", ShellEnd::Stopped, None, t0 + Duration::from_secs(5));
        assert_eq!(
            banner_words(s.get("a").unwrap()),
            "Run a \u{b7} done in 6:40"
        );
        assert_eq!(banner_words(s.get("b").unwrap()), "Run b \u{b7} failed");
        assert_eq!(banner_words(s.get("c").unwrap()), "Run c \u{b7} stopped");
    }

    #[test]
    fn the_last_line_is_the_last_with_words() {
        assert_eq!(
            last_line("step 1\nstep 2\n\n  \n").as_deref(),
            Some("step 2")
        );
        assert_eq!(
            last_line("Compiling 10%\rCompiling 55%\rCompiling 90%\n").as_deref(),
            Some("Compiling 90%")
        );
        assert_eq!(
            last_line("\u{1b}[1m\u{1b}[32m   Finished\u{1b}[0m test profile\n").as_deref(),
            Some("Finished test profile")
        );
        assert_eq!(last_line(""), None);
        assert_eq!(last_line("\n\n"), None);
        let long = "x".repeat(500);
        assert_eq!(last_line(&long).map(|l| l.chars().count()), Some(LINE_MAX));
    }

    #[test]
    fn read_tail_reads_the_files_end() {
        let dir = crate::test_temp::dir("agent-shells-tail");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("b1.output");
        let mut text = "old line\n".repeat(1000);
        text.push_str("step 3\n");
        std::fs::write(&path, &text).unwrap();
        let tail = read_tail(&path).unwrap();
        assert!(tail.len() <= TAIL_BYTES as usize);
        assert_eq!(last_line(&tail).as_deref(), Some("step 3"));
        assert!(read_tail(&dir.join("none")).is_none());
        std::fs::remove_dir_all(&dir).ok();
    }
}
