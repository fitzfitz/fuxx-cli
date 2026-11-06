//! Agent-state heuristics: is a session working, waiting for you, or done?
//!
//! Signals: output timing, terminal notifications (OSC 9 / 99 / 777 and BEL), and the screen's
//! last lines.
//! Pure — the caller passes the time in — so every rule is unit-tested. The constants are a
//! first guess; tune them against real agents during the M2 manual check.

use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// Nothing to report yet (fresh session, or only startup/echo output so far).
    Idle,
    /// Producing output.
    Working,
    /// Went quiet with a question on screen (or after a notification with a question).
    Waiting,
    /// Went quiet (or sent a notification) with no question on screen.
    Done,
}

impl Status {
    /// The lowercase name used on the scripting API.
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Idle => "idle",
            Status::Working => "working",
            Status::Waiting => "waiting",
            Status::Done => "done",
        }
    }

    pub fn parse(s: &str) -> Option<Status> {
        match s {
            "idle" => Some(Status::Idle),
            "working" => Some(Status::Working),
            "waiting" => Some(Status::Waiting),
            "done" => Some(Status::Done),
            _ => None,
        }
    }
}

/// Output this soon after a keystroke is the echo of typing, not work.
pub const ECHO_WINDOW: Duration = Duration::from_millis(150);
/// A working session that has been silent this long has settled.
pub const IDLE_AFTER: Duration = Duration::from_secs(3);
/// Plain output this soon after a resize is the program redrawing for its new size (it got
/// SIGWINCH), not work. Every resize renews it, so a drag keeps it open.
pub const RESIZE_WINDOW: Duration = Duration::from_millis(500);
/// How many non-blank screen lines (from the bottom) to search for a question.
pub const TAIL_LINES: usize = 8;
/// Lowercased substrings meaning "the program is asking you something".
pub const WAITING_PATTERNS: &[&str] = &[
    "(y/n)",
    "[y/n]",
    "(yes/no)",
    "[yes/no]",
    "do you want to",
    "would you like to",
    "press enter to continue",
    "waiting for input",
    "waiting for your input",
];

pub fn looks_like_waiting(screen_tail: &str) -> bool {
    waiting_match(screen_tail).is_some()
}

/// The last line of `screen_tail` holding a waiting pattern (the newest question on screen).
fn waiting_match(screen_tail: &str) -> Option<&str> {
    screen_tail.lines().rev().find(|line| asks(line))
}

/// Whether `line` holds a waiting pattern.
fn asks(line: &str) -> bool {
    let line = line.to_lowercase();
    WAITING_PATTERNS.iter().any(|p| line.contains(p))
}

/// The longest question line kept; the renderer ellipsises it to fit.
const QUESTION_MAX_CHARS: usize = 120;

/// Words that mark a prompt for a secret; its line is never shown in the sidebar.
const SECRET_WORDS: &[&str] = &["password", "passphrase", "token"];

/// The screen line that made a session look Waiting, for the sidebar's question: without the
/// box it may be drawn in (agents like Claude Code frame their prompts with box-drawing
/// characters), tabs as spaces, trimmed, without control characters, at most 120 chars. A line
/// that is only the pattern ("(y/n)") gets the line above it in front, which holds the question.
/// A prompt for a secret gives none.
pub fn waiting_line(tail: &str) -> Option<String> {
    let lines: Vec<&str> = tail.lines().collect();
    let at = lines.iter().rposition(|line| asks(line))?;
    let mut line = clean_line(lines[at]);
    let lower = line.to_lowercase();
    let bare = WAITING_PATTERNS
        .iter()
        .any(|p| lower.trim_end_matches([':', '?', ' ']) == *p);
    if bare {
        let above = lines[..at]
            .iter()
            .rev()
            .map(|l| clean_line(l))
            .find(|l| !l.is_empty());
        if let Some(above) = above {
            line = format!("{above} {line}");
        }
    }
    let lower = line.to_lowercase();
    if SECRET_WORDS.iter().any(|w| lower.contains(w)) {
        return None;
    }
    Some(line.chars().take(QUESTION_MAX_CHARS).collect())
}

/// A screen line as the question shows it: tabs as spaces, no control characters, and no box
/// border or spaces at either end.
fn clean_line(line: &str) -> String {
    let line = crate::cli::plain_text(&line.replace('\t', " "));
    line.trim_matches(|c: char| c.is_whitespace() || ('\u{2500}'..='\u{257f}').contains(&c))
        .to_string()
}

/// How long ago, for a sidebar row: "12s", "4m", "2h", "3d" (rounded down).
pub fn age_label(d: Duration) -> String {
    let secs = d.as_secs();
    match secs {
        0..60 => format!("{secs}s"),
        60..3600 => format!("{}m", secs / 60),
        3600..86400 => format!("{}h", secs / 3600),
        _ => format!("{}d", secs / 86400),
    }
}

pub struct StatusTracker {
    status: Status,
    last_input: Option<Instant>,
    last_output: Option<Instant>,
    /// A notification (or a BEL while working) arrived since the last keystroke: settle on the
    /// next tick.
    attention: bool,
    /// Until when plain output is a redraw for a resize and changes nothing.
    resize_until: Option<Instant>,
    /// New output (or a BEL) arrived since the screen was last read.
    dirty: bool,
}

impl StatusTracker {
    pub fn new() -> Self {
        Self {
            status: Status::Idle,
            last_input: None,
            last_output: None,
            attention: false,
            resize_until: None,
            dirty: false,
        }
    }

    #[cfg(test)]
    pub fn status(&self) -> Status {
        self.status
    }

    /// The user typed into this session. Typing into a settled (Waiting/Done) session means
    /// you've seen it, so it goes back to Idle. Returns the new status when it changed.
    pub fn on_input(&mut self, now: Instant) -> Option<Status> {
        self.last_input = Some(now);
        self.attention = false;
        if matches!(self.status, Status::Waiting | Status::Done) {
            self.status = Status::Idle;
            return Some(Status::Idle);
        }
        None
    }

    /// A new size was sent to the program. Plain output for the next `RESIZE_WINDOW` is its
    /// redraw: no Working, no settling, no new notification. Each call renews the window.
    pub fn on_resize(&mut self, now: Instant) {
        self.resize_until = Some(now + RESIZE_WINDOW);
    }

    /// The session produced output; `notifications` = OSC 9 / 99 / 777 notifications seen in it.
    /// Returns the new status when it changed.
    pub fn on_output(&mut self, now: Instant, notifications: usize) -> Option<Status> {
        if notifications > 0 {
            self.attention = true;
            self.last_output = Some(now);
            self.dirty = true;
            return None;
        }
        if self.resize_until.is_some_and(|until| now < until) {
            return None; // the redraw for a resize
        }
        let Some(input) = self.last_input else {
            return None; // never typed in: shell startup / prompt noise
        };
        if now.duration_since(input) < ECHO_WINDOW {
            return None; // echo of typing
        }
        self.last_output = Some(now);
        self.dirty = true;
        if !self.attention && self.status != Status::Working {
            self.status = Status::Working;
            return Some(Status::Working);
        }
        None
    }

    /// The program rang the bell (a real BEL, reported by the parser). Claude Code uses it to say
    /// "look at me" in Terminal.app. It only counts while Working, so a shell's beep at a failed
    /// tab completion doesn't mark a session Done.
    pub fn on_bell(&mut self, now: Instant) {
        if self.status == Status::Working {
            self.attention = true;
            self.last_output = Some(now);
            self.dirty = true;
        }
    }

    /// Call periodically. `screen_tail` is evaluated only when the session is settling and there
    /// is something new to read: a settled session is not read again until new output or a BEL
    /// arrives. Returns the new status when it changed.
    pub fn tick(&mut self, now: Instant, screen_tail: impl FnOnce() -> String) -> Option<Status> {
        let quiet = self
            .last_output
            .is_some_and(|t| now.duration_since(t) >= IDLE_AFTER);
        let settling = self.attention || (self.status == Status::Working && quiet);
        if !settling {
            return None;
        }
        if self.attention && !self.dirty && matches!(self.status, Status::Waiting | Status::Done) {
            return None;
        }
        self.dirty = false;
        let next = if looks_like_waiting(&screen_tail()) {
            Status::Waiting
        } else {
            Status::Done
        };
        if next == self.status {
            return None;
        }
        self.status = next;
        Some(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    /// A tracker where the user typed at `t0` (so output counts once past the echo window).
    fn typed_at(t0: Instant) -> StatusTracker {
        let mut t = StatusTracker::new();
        t.on_input(t0);
        t
    }

    #[test]
    fn waiting_line_returns_the_matching_line() {
        let tail = "building…\n  Do you want to overwrite foo.txt? (y/n)  \nmore";
        assert_eq!(
            waiting_line(tail).as_deref(),
            Some("Do you want to overwrite foo.txt? (y/n)")
        );
        assert_eq!(waiting_line("all quiet\nnothing here"), None);
        // Control characters go; the line is capped at 120 chars.
        assert_eq!(
            waiting_line("\x1b]0;x\x07Proceed? [y/n]\x1b[2J").as_deref(),
            Some("]0;xProceed? [y/n][2J")
        );
        let long = format!("{} (y/n)", "a".repeat(200));
        assert_eq!(waiting_line(&long).unwrap().chars().count(), 120);
    }

    #[test]
    fn secret_prompts_give_no_line() {
        for tail in [
            "Enter your Password (y/n)",
            "Passphrase for key: do you want to continue",
            "paste the API TOKEN? [y/n]",
        ] {
            assert_eq!(waiting_line(tail), None, "{tail}");
        }
    }

    #[test]
    fn waiting_line_trims_box_borders_and_tabs() {
        // Claude Code draws its prompt inside a box.
        let tail = "╭──────────────╮\n│ Do you want to make this edit to auth.rs? │\n│ ❯ 1. Yes │\n╰──────────────╯";
        assert_eq!(
            waiting_line(tail).as_deref(),
            Some("Do you want to make this edit to auth.rs?")
        );
        // A tab is a space; box characters inside the line stay.
        assert_eq!(
            waiting_line("┃\tProceed?\t(y/n) ┃").as_deref(),
            Some("Proceed? (y/n)")
        );
        assert_eq!(waiting_line("a ─ b (y/n)").as_deref(), Some("a ─ b (y/n)"));
    }

    #[test]
    fn waiting_line_prefixes_the_previous_line_when_only_the_pattern() {
        assert_eq!(
            waiting_line("Overwrite foo.txt?\n  (y/n)  ").as_deref(),
            Some("Overwrite foo.txt? (y/n)")
        );
        // Blank lines and bare box borders in between are skipped.
        assert_eq!(
            waiting_line("│ Apply the patch? │\n│              │\n│ [Y/n] │").as_deref(),
            Some("Apply the patch? [Y/n]")
        );
        // Nothing above it: the pattern alone.
        assert_eq!(waiting_line("(yes/no)").as_deref(), Some("(yes/no)"));
    }

    #[test]
    fn waiting_line_last_matching_line_wins() {
        let tail = "Do you want to edit a.rs? (y/n)\nok\nDo you want to edit b.rs? (y/n)\n";
        assert_eq!(
            waiting_line(tail).as_deref(),
            Some("Do you want to edit b.rs? (y/n)")
        );
    }

    #[test]
    fn waiting_line_a_secret_on_the_newest_line_gives_none() {
        // An older, harmless question doesn't stand in for the newest one.
        let tail = "Do you want to continue? (y/n)\nEnter your password (y/n)";
        assert_eq!(waiting_line(tail), None);
        // Nor does a secret in the line the bare pattern borrows.
        assert_eq!(waiting_line("Paste your token\n(y/n)"), None);
    }

    #[test]
    fn age_labels() {
        let s = Duration::from_secs;
        assert_eq!(age_label(s(0)), "0s");
        assert_eq!(age_label(s(12)), "12s");
        assert_eq!(age_label(s(59)), "59s");
        assert_eq!(age_label(s(60)), "1m");
        assert_eq!(age_label(s(4 * 60 + 59)), "4m");
        assert_eq!(age_label(s(59 * 60 + 59)), "59m");
        assert_eq!(age_label(s(3600)), "1h");
        assert_eq!(age_label(s(2 * 3600 + 1799)), "2h");
        assert_eq!(age_label(s(24 * 3600)), "1d");
        assert_eq!(age_label(s(3 * 86400 + 5)), "3d");
    }

    #[test]
    fn starts_idle_and_ignores_startup_output() {
        let t0 = Instant::now();
        let mut t = StatusTracker::new();
        t.on_output(t0, 0);
        assert_eq!(t.status(), Status::Idle);
        assert_eq!(t.tick(t0 + ms(5000), || String::new()), None);
    }

    #[test]
    fn echo_of_typing_is_not_work() {
        let t0 = Instant::now();
        let mut t = typed_at(t0);
        t.on_output(t0 + ms(20), 0);
        assert_eq!(t.status(), Status::Idle);
    }

    #[test]
    fn output_after_the_echo_window_is_work() {
        let t0 = Instant::now();
        let mut t = typed_at(t0);
        t.on_output(t0 + ms(400), 0);
        assert_eq!(t.status(), Status::Working);
    }

    #[test]
    fn output_reports_working_once() {
        let t0 = Instant::now();
        let mut t = StatusTracker::new();
        assert_eq!(t.on_output(t0, 0), None); // startup noise
        t.on_input(t0 + ms(100));
        assert_eq!(t.on_output(t0 + ms(120), 0), None); // echo of typing
        assert_eq!(t.on_output(t0 + ms(400), 0), Some(Status::Working));
        assert_eq!(t.on_output(t0 + ms(600), 0), None); // already working: no change to report
        assert_eq!(t.on_output(t0 + ms(700), 1), None); // a notification marks attention, not a change
    }

    #[test]
    fn quiet_working_session_settles_to_done() {
        let t0 = Instant::now();
        let mut t = typed_at(t0);
        t.on_output(t0 + ms(400), 0);
        assert_eq!(t.tick(t0 + ms(1000), || "$ ".into()), None); // not quiet long enough
        assert_eq!(
            t.tick(t0 + ms(400) + IDLE_AFTER, || "$ ".into()),
            Some(Status::Done)
        );
        assert_eq!(t.tick(t0 + ms(9000), || "$ ".into()), None); // reports only changes
    }

    #[test]
    fn quiet_session_with_a_question_is_waiting() {
        let t0 = Instant::now();
        let mut t = typed_at(t0);
        t.on_output(t0 + ms(400), 0);
        let tail = || "Edit src/main.rs\nDo you want to proceed?\n❯ 1. Yes".to_string();
        assert_eq!(
            t.tick(t0 + ms(400) + IDLE_AFTER, tail),
            Some(Status::Waiting)
        );
    }

    #[test]
    fn notification_settles_on_the_next_tick_without_waiting_for_idle() {
        let t0 = Instant::now();
        let mut t = typed_at(t0);
        t.on_output(t0 + ms(400), 0);
        t.on_output(t0 + ms(500), 1);
        assert_eq!(
            t.tick(t0 + ms(550), || "all done".into()),
            Some(Status::Done)
        );
    }

    #[test]
    fn output_after_a_notification_does_not_flip_back_to_working_until_input() {
        let t0 = Instant::now();
        let mut t = typed_at(t0);
        t.on_output(t0 + ms(400), 1);
        t.tick(t0 + ms(450), || String::new());
        t.on_output(t0 + ms(600), 0);
        assert_eq!(t.status(), Status::Done);
        t.on_input(t0 + ms(2000));
        t.on_output(t0 + ms(2500), 0);
        assert_eq!(t.status(), Status::Working);
    }

    #[test]
    fn typing_into_a_settled_session_acknowledges_it() {
        let t0 = Instant::now();
        let mut t = typed_at(t0);
        t.on_output(t0 + ms(400), 0);
        t.tick(t0 + ms(400) + IDLE_AFTER, || "Continue? (y/n)".into());
        assert_eq!(t.status(), Status::Waiting);
        assert_eq!(t.on_input(t0 + ms(9000)), Some(Status::Idle)); // you answered
        assert_eq!(t.on_input(t0 + ms(9100)), None); // already idle: no change to report
    }

    #[test]
    fn bell_while_working_settles_on_the_next_tick() {
        let t0 = Instant::now();
        let mut t = typed_at(t0);
        t.on_output(t0 + ms(400), 0);
        t.on_bell(t0 + ms(500));
        assert_eq!(
            t.tick(t0 + ms(550), || "all done".into()),
            Some(Status::Done)
        );
    }

    #[test]
    fn bell_when_not_working_is_ignored() {
        let t0 = Instant::now();
        let mut t = StatusTracker::new();
        t.on_bell(t0); // before any typing
        t.on_input(t0 + ms(100));
        t.on_bell(t0 + ms(120)); // the shell beeps at a failed tab completion
        assert_eq!(t.tick(t0 + ms(5000), || "$ ".into()), None);
        assert_eq!(t.status(), Status::Idle);
    }

    #[test]
    fn waiting_patterns_are_case_insensitive() {
        assert!(looks_like_waiting("Overwrite file? (Y/n)"));
        assert!(looks_like_waiting("DO YOU WANT TO continue"));
        assert!(!looks_like_waiting("Compiling fuxx v0.1.0"));
    }

    #[test]
    fn settled_session_does_not_reread_the_screen_until_new_output() {
        let t0 = Instant::now();
        let mut t = typed_at(t0);
        t.on_output(t0 + ms(400), 1);
        assert_eq!(
            t.tick(t0 + ms(450), || "all done".into()),
            Some(Status::Done)
        );
        // Nothing new arrived: the screen must not be read again.
        assert_eq!(
            t.tick(t0 + ms(500), || -> String {
                panic!("screen read without new output")
            }),
            None
        );
        // New output arrives while settled: re-read once (and it now asks a question).
        t.on_output(t0 + ms(600), 0);
        assert_eq!(
            t.tick(t0 + ms(650), || "Continue? (y/n)".into()),
            Some(Status::Waiting)
        );
    }

    #[test]
    fn redraw_after_a_resize_does_not_make_an_idle_session_work() {
        let t0 = Instant::now();
        let mut t = typed_at(t0);
        t.on_resize(t0 + ms(1000));
        assert_eq!(t.on_output(t0 + ms(1100), 0), None);
        assert_eq!(t.status(), Status::Idle);
        // Nothing to settle either: the redraw left no trace for the idle timer.
        assert_eq!(
            t.tick(t0 + ms(1100) + IDLE_AFTER, || -> String {
                panic!("screen read for a redraw")
            }),
            None
        );
    }

    #[test]
    fn redraw_after_a_resize_leaves_a_waiting_session_alone() {
        let t0 = Instant::now();
        let mut t = typed_at(t0);
        t.on_output(t0 + ms(400), 0);
        let ask = || "Continue? (y/n)".to_string();
        assert_eq!(
            t.tick(t0 + ms(400) + IDLE_AFTER, ask),
            Some(Status::Waiting)
        );
        t.on_resize(t0 + ms(5000));
        assert_eq!(t.on_output(t0 + ms(5100), 0), None);
        // The redraw doesn't make the screen get read again (so no repeat notification).
        assert_eq!(
            t.tick(t0 + ms(5200), || -> String {
                panic!("screen read for a redraw")
            }),
            None
        );
        assert_eq!(t.status(), Status::Waiting);
    }

    #[test]
    fn output_after_the_resize_window_is_work_as_before() {
        let t0 = Instant::now();
        let mut t = typed_at(t0);
        t.on_resize(t0 + ms(1000));
        assert_eq!(
            t.on_output(t0 + ms(1000) + RESIZE_WINDOW, 0),
            Some(Status::Working)
        );
    }

    #[test]
    fn a_resize_during_a_drag_keeps_extending_the_window() {
        let t0 = Instant::now();
        let mut t = typed_at(t0);
        for i in 0..5 {
            t.on_resize(t0 + ms(1000 + i * 300));
        }
        // 1000 + 4 * 300 = 2200 was the last resize: 2600 is still inside, 2700 is out.
        assert_eq!(t.on_output(t0 + ms(2600), 0), None);
        assert_eq!(t.status(), Status::Idle);
        assert_eq!(t.on_output(t0 + ms(2700), 0), Some(Status::Working));
    }

    #[test]
    fn a_notification_inside_the_resize_window_still_counts() {
        let t0 = Instant::now();
        let mut t = typed_at(t0);
        t.on_output(t0 + ms(400), 0);
        t.on_resize(t0 + ms(1000));
        t.on_output(t0 + ms(1100), 1);
        assert_eq!(
            t.tick(t0 + ms(1150), || "all done".into()),
            Some(Status::Done)
        );
    }

    #[test]
    fn a_working_session_stays_working_through_a_resize_and_settles_after() {
        let t0 = Instant::now();
        let mut t = typed_at(t0);
        t.on_output(t0 + ms(400), 0);
        assert_eq!(t.status(), Status::Working);
        t.on_resize(t0 + ms(1000));
        assert_eq!(t.on_output(t0 + ms(1100), 0), None);
        assert_eq!(t.status(), Status::Working);
        // The redraw didn't restart the idle timer: quiet counts from the last real output.
        assert_eq!(
            t.tick(t0 + ms(400) + IDLE_AFTER, || "$ ".into()),
            Some(Status::Done)
        );
    }

    #[test]
    fn a_working_session_keeps_working_on_real_output_after_a_resize() {
        let t0 = Instant::now();
        let mut t = typed_at(t0);
        t.on_output(t0 + ms(400), 0);
        t.on_resize(t0 + ms(1000));
        t.on_output(t0 + ms(1600), 0); // real output past the window renews the idle timer
        assert_eq!(t.tick(t0 + ms(1600) + ms(2900), || "$ ".into()), None);
        assert_eq!(
            t.tick(t0 + ms(1600) + IDLE_AFTER, || "$ ".into()),
            Some(Status::Done)
        );
    }
}
