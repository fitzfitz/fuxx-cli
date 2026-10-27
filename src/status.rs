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
    let tail = screen_tail.to_lowercase();
    WAITING_PATTERNS.iter().any(|p| tail.contains(p))
}

pub struct StatusTracker {
    status: Status,
    last_input: Option<Instant>,
    last_output: Option<Instant>,
    /// A notification (or a BEL while working) arrived since the last keystroke: settle on the
    /// next tick.
    attention: bool,
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

    /// The session produced output; `notifications` = OSC 9 / 99 / 777 notifications seen in it.
    /// Returns the new status when it changed.
    pub fn on_output(&mut self, now: Instant, notifications: usize) -> Option<Status> {
        if notifications > 0 {
            self.attention = true;
            self.last_output = Some(now);
            self.dirty = true;
            return None;
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
}
