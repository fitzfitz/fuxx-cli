---
tags: [development, plan, milestone]
milestone: M2
status: done
updated: 2026-10-26
---

# M2 Plan — Agent State Detection

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

| At a glance | |
|---|---|
| **Milestone note** | [[42 - Milestone 2 - Agent State Detection]] |
| **Status** | ✅ Done 2026-10-26 (commits `a2da2e2`…`1843452` + docs on `m2-agent-state`) |
| **Tasks done** | 7 / 7 |
| **▶ Pick up here** | M2 complete → [[M3 Plan - Many Sessions]] |
| **Decisions used** | ✅ **D4** = a 4th status `Idle` (used as recommended; plan confirmed by you 2026-10-26) (see [[00 - Plan Tracker (Start Here)]]) · ✅ **Plan change 2026-10-26:** also detect OSC 99, OSC 777 and BEL (your option b; the shell's environment stays inherited, and no Claude Code setting is documented) |
| **Needs your eyes** | ✅ Task 6 confirmed by you on 2026-10-26 (all checks, incl. a real `claude` run) |

**Goal:** Each terminal knows whether its program is **Working**, **Waiting** for you, or **Done**
(plus **Idle** when there's nothing to report), and logs every change so you can watch it.

**Architecture:** A pure `StatusTracker` (time is passed in, so every rule is unit-tested) is fed
three signals:
1. **Output timing.** Output means Working. Output within 150 ms of a keystroke is just the echo of your typing.
2. **Notifications.** Agents say "look at me" with a terminal notification, and which one depends on
   the terminal they think they're in. Claude Code picks from the `TERM_PROGRAM` the shell inherited
   from whatever launched fuxx: OSC 9 `ESC ]9;…` (iTerm2), OSC 99 `ESC ]99;…` (kitty), OSC 777
   `ESC ]777;notify;…` (Ghostty) or a plain BEL (Terminal.app). `alacritty_terminal` ignores the
   three OSCs, so v1's byte scanner (`OscDetector`, restored from git and extended) watches the raw
   stream in the reader thread. It skips `ESC ]9;4;…`, the progress bar Claude Code sends *while
   working* under Ghostty ≥ 1.2 and iTerm2 ≥ 3.6.6. A real BEL comes from the parser as
   `Event::Bell`; it only counts while the session is Working, so a shell's tab-completion beep isn't
   read as "done".
3. **The bottom of the screen.** When a session goes quiet (3 s) or sends a notification, a question
   pattern near the bottom (`(y/n)`, `do you want to`…) means **Waiting**; otherwise **Done**.

Typing into a Waiting/Done session means you've seen it, so it returns to **Idle**.

```
 reader thread: bytes ─► OscDetector.feed ─► TermEvent::Output { notifications } ─► tracker.on_output
 parser BEL ─► TermEvent::Term(Event::Bell) ──────────────────────────────────────► tracker.on_bell
 keystroke ───────────────────────────────────────────────────────────────► tracker.on_input
 50 ms ticker ─► tracker.tick(now, || screen tail text) ─► Some(new status) ─► log "[fuxx] status -> …"
```

**Tech Stack:** No new dependencies.

**Spec:** [[42 - Milestone 2 - Agent State Detection]] · [[11 - Features]] (section C) · v1's OSC work in [[90-Archive-v1-CLI-Wrapper]]

## Global Constraints

- Everything from the M0/M1 plans (pinned GPUI, zero warnings from our code, you verify GUI).
- **No new dependencies.**
- Notification detection (OSC 9 except `9;4;`, OSC 99, OSC 777 `notify`) runs on the **raw PTY bytes**, before/alongside the parser (alacritty ignores these OSCs). It must survive a sequence split across two reads. BEL comes from the parser (`Event::Bell`), never from scanning bytes: a raw `0x07` is also the terminator of every OSC, including window titles.
- The shell's environment is inherited unchanged (no stripping of `TERM_PROGRAM` etc.), and no agent setting is documented as a workaround. Both were considered and kept out on 2026-10-26.
- The heuristic constants are **first guesses, tuned in Task 6**: `ECHO_WINDOW = 150 ms`, `IDLE_AFTER = 3 s`, `TAIL_LINES = 8`, `WAITING_PATTERNS` list.
- The status is only **logged** in M2 (`[fuxx] status -> Working` on stderr). Rings come in M4. This is the milestone note's "debug indicator or log".
- Output before the first keystroke (shell startup, prompt) never counts as work.

## Review Focus

Failure modes worth a test or a deliberate check, most likely first:

1. **Your own typing marks the session busy** → test `echo_of_typing_is_not_work` (Task 2).
2. **A fresh shell shows a status before anything happened** → test `starts_idle_and_ignores_startup_output` (Task 2).
3. **After you answer a prompt, it stays "Waiting"** → test `typing_into_a_settled_session_acknowledges_it` (Task 2).
4. **Claude Code permission prompts read as Done instead of Waiting** → test `quiet_session_with_a_question_is_waiting` (Task 2), plus a real check in Task 6. Add the exact wording you see to `WAITING_PATTERNS` if it's missed.
5. **An agent thinking silently for >3 s flips to Done too early** → Task 6 check. Raise `IDLE_AFTER` if it happens.
6. **Claude Code's progress bar (`ESC ]9;4;…`) reads as Done while it's still working** → tests `ignores_osc_9_progress_bar`, `ignores_progress_bar_split_across_chunks` (Task 1), plus the real check in Task 6 (you launch from Ghostty 1.3.1, which turns the progress bar on).
7. **A shell's beep (failed tab completion) marks a fresh session Done** → test `bell_when_not_working_is_ignored` (Task 2).

## Files

| File | Change | Responsibility | Task |
|---|---|---|---|
| `src/detector.rs` | Create (restore v1, extend) | `OscDetector`: count OSC 9 / 99 / 777 notifications across chunks, skipping the `9;4;` progress bar | 1 |
| `src/status.rs` | Create | `Status` + `StatusTracker` heuristics | 2 |
| `src/grid.rs` | Modify | `GridSnapshot::tail_text(n)` | 3 |
| `src/terminal.rs` | Modify | Reader thread counts notifications → `TermEvent::Output { notifications }` | 4 |
| `src/terminal_view.rs` | Modify | Own a `StatusTracker`; feed it output, BEL and keystrokes; log changes | 5 |
| `src/main.rs` | Modify | `mod detector; mod status;` | 1, 2 |

---

### Task 1: Restore v1's `OscDetector` and extend it to every notification — ✅

**Files:** Create `src/detector.rs` · Modify `src/main.rs` (add `mod detector;`)

**Interfaces:** Produces `detector::OscDetector::{new() -> OscDetector, feed(&mut self, &[u8]) -> usize}`: the number of notifications whose intro completed in this chunk. It counts OSC 9 `ESC ]9;` (but not the `ESC ]9;4;` progress bar), OSC 99 `ESC ]99;` and OSC 777 `ESC ]777;notify;`, and carries partial matches across calls. An OSC 9 is counted one or two bytes after its intro, once it's clear it isn't a progress bar.

- [x] **Step 1: Restore v1's detector from git** (it was deleted in M0; commit `eb3eaad` still has it), and add `mod detector;` to `src/main.rs`:

```bash
git show eb3eaad:src/detector.rs > src/detector.rs
```

- [x] **Step 2: Run:** `cargo test detector`. Expected: `test result: ok. 6 passed` (v1's tests; a dead-code warning is expected until Task 4). This is the baseline.

- [x] **Step 3: Write the failing tests.** In the test module, **delete** `does_not_detect_osc_99` (OSC 99 is now a notification) and add these 7 tests at the end of the module:

```rust
    #[test]
    fn detects_kitty_osc_99() {
        let mut d = OscDetector::new();
        // Claude Code's kitty notification: title, body, then "done + focus".
        let kitty = b"\x1b]99;i=1:d=0:p=title;Claude Code\x1b\\\x1b]99;i=1:p=body;Done\x1b\\\x1b]99;i=1:d=1:a=focus;\x1b\\";
        assert_eq!(d.feed(kitty), 3);
    }

    #[test]
    fn detects_ghostty_osc_777_notify() {
        let mut d = OscDetector::new();
        assert_eq!(d.feed(b"\x1b]777;notify;Claude Code;Done\x07"), 1);
    }

    #[test]
    fn ignores_other_osc_777() {
        let mut d = OscDetector::new();
        assert_eq!(d.feed(b"\x1b]777;preexec\x07"), 0);
    }

    #[test]
    fn ignores_osc_9_progress_bar() {
        let mut d = OscDetector::new();
        assert_eq!(d.feed(b"\x1b]9;4;3;\x07\x1b]9;4;1;50\x07\x1b]9;4;0;\x07"), 0);
    }

    #[test]
    fn ignores_progress_bar_split_across_chunks() {
        let mut d = OscDetector::new();
        assert_eq!(d.feed(b"\x1b]9;"), 0);
        assert_eq!(d.feed(b"4"), 0);
        assert_eq!(d.feed(b";1;50\x07"), 0);
    }

    #[test]
    fn osc_9_message_starting_with_4_still_counts() {
        let mut d = OscDetector::new();
        assert_eq!(d.feed(b"\x1b]9;42 tests passed\x07"), 1);
    }

    #[test]
    fn osc_9_is_counted_in_the_chunk_after_its_intro() {
        let mut d = OscDetector::new();
        assert_eq!(d.feed(b"\x1b]9;"), 0);
        assert_eq!(d.feed(b"done\x07"), 1);
    }
```

- [x] **Step 4: Run:** `cargo test detector`. Expected: `test result: FAILED. 7 passed; 5 failed`. The failures are `detects_kitty_osc_99`, `detects_ghostty_osc_777_notify`, `ignores_osc_9_progress_bar`, `ignores_progress_bar_split_across_chunks` and `osc_9_is_counted_in_the_chunk_after_its_intro`.

- [x] **Step 5: Implement.** Replace everything **above** the test module with:

```rust
//! Spots the "look at me" notifications agents send, in the raw PTY bytes. `alacritty_terminal`
//! ignores these OSC sequences, so they never reach the parser's events.
//!
//! Counted (each by its intro, so a sequence split across reads is still found):
//! - OSC 9 `ESC ]9;` (iTerm2 style), except `ESC ]9;4;`, which is a progress bar, not a notification
//! - OSC 99 `ESC ]99;` (kitty)
//! - OSC 777 `ESC ]777;notify;` (Ghostty / urxvt)

/// Each intro starts with ESC and contains no other ESC, so a failed partial match can only
/// restart at an ESC.
const OSC_9: &[u8] = b"\x1b]9;";
const OSC_99: &[u8] = b"\x1b]99;";
const OSC_777_NOTIFY: &[u8] = b"\x1b]777;notify;";
/// What follows `ESC ]9;` in a ConEmu-style progress bar (Claude Code sends these while working).
const PROGRESS: &[u8] = b"4;";

/// Matches one intro across calls.
struct Intro {
    target: &'static [u8],
    /// How many leading bytes of `target` have matched so far.
    matched: usize,
}

impl Intro {
    fn new(target: &'static [u8]) -> Self {
        Intro { target, matched: 0 }
    }

    /// Feeds one byte; true when the whole intro has just matched.
    fn step(&mut self, b: u8) -> bool {
        if b == self.target[self.matched] {
            self.matched += 1;
            if self.matched == self.target.len() {
                self.matched = 0; // ready for the next sequence
                return true;
            }
        } else {
            // The byte that broke the match may itself be ESC: begin a fresh match from it,
            // so e.g. "ESC ESC ]9;" is still detected.
            self.matched = usize::from(b == self.target[0]);
        }
        false
    }
}

pub struct OscDetector {
    osc9: Intro,
    osc99: Intro,
    osc777: Intro,
    /// Just after an OSC 9 intro: how many bytes of `PROGRESS` have followed it.
    after_osc9: Option<usize>,
}

impl OscDetector {
    pub fn new() -> Self {
        OscDetector {
            osc9: Intro::new(OSC_9),
            osc99: Intro::new(OSC_99),
            osc777: Intro::new(OSC_777_NOTIFY),
            after_osc9: None,
        }
    }

    /// Number of notifications whose intro completed in `bytes`. An OSC 9 is counted one or two
    /// bytes after its intro, once it's clear it isn't a progress bar.
    pub fn feed(&mut self, bytes: &[u8]) -> usize {
        let mut detections = 0;
        for &b in bytes {
            if let Some(seen) = self.after_osc9 {
                if b != PROGRESS[seen] {
                    detections += 1; // a real OSC 9 notification
                    self.after_osc9 = None;
                } else if seen + 1 == PROGRESS.len() {
                    self.after_osc9 = None; // `ESC ]9;4;`: a progress bar
                } else {
                    self.after_osc9 = Some(seen + 1);
                }
            }
            if self.osc9.step(b) {
                self.after_osc9 = Some(0);
            }
            if self.osc99.step(b) {
                detections += 1;
            }
            if self.osc777.step(b) {
                detections += 1;
            }
        }
        detections
    }
}
```

- [x] **Step 6: Run:** `cargo test detector`. Expected: `test result: ok. 12 passed`.
- [x] **Step 7: Commit** *(when you say so)*: `git add src/detector.rs src/main.rs && git commit -m "feat(m2): notification detector (OSC 9/99/777, skips the 9;4 progress bar)"`

### Task 2: `status.rs` — the heuristics — ✅

**Files:** Create `src/status.rs` · Modify `src/main.rs` (add `mod status;`)

**Interfaces:**
- Produces:
  - `pub enum Status { Idle, Working, Waiting, Done }` (`Copy + Eq + Debug`).
  - Constants `ECHO_WINDOW`, `IDLE_AFTER`, `TAIL_LINES`, `WAITING_PATTERNS`; `pub fn looks_like_waiting(&str) -> bool`.
  - `StatusTracker::new()`, `on_input(&mut self, Instant) -> Option<Status>`, `on_output(&mut self, Instant, notifications: usize)`, `on_bell(&mut self, Instant)`,
    `tick(&mut self, Instant, screen_tail: impl FnOnce() -> String) -> Option<Status>` (returns the new status only when it changed; `screen_tail` is called only when settling),
    `status()` (test-only).

> **The rules in one table.**
>
> | Event | Effect |
> |---|---|
> | Output, never typed yet | ignored (startup noise) |
> | Output < 150 ms after a keystroke | ignored (echo) |
> | Other output | → **Working** (unless a notification is pending) |
> | Notification (OSC 9 / 99 / 777) | mark "attention"; settle on the next tick |
> | BEL while **Working** | mark "attention"; settle on the next tick |
> | BEL otherwise | ignored (a shell's beep, e.g. failed tab completion) |
> | Tick: attention, or Working and quiet ≥ 3 s | → **Waiting** if a question pattern is in the last 8 lines, else **Done** |
> | Keystroke while Waiting/Done | → **Idle** (acknowledged); clears attention |

- [x] **Step 1: Write the failing tests.** Create `src/status.rs` with only:

```rust
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
    fn quiet_working_session_settles_to_done() {
        let t0 = Instant::now();
        let mut t = typed_at(t0);
        t.on_output(t0 + ms(400), 0);
        assert_eq!(t.tick(t0 + ms(1000), || "$ ".into()), None); // not quiet long enough
        assert_eq!(t.tick(t0 + ms(400) + IDLE_AFTER, || "$ ".into()), Some(Status::Done));
        assert_eq!(t.tick(t0 + ms(9000), || "$ ".into()), None); // reports only changes
    }

    #[test]
    fn quiet_session_with_a_question_is_waiting() {
        let t0 = Instant::now();
        let mut t = typed_at(t0);
        t.on_output(t0 + ms(400), 0);
        let tail = || "Edit src/main.rs\nDo you want to proceed?\n❯ 1. Yes".to_string();
        assert_eq!(t.tick(t0 + ms(400) + IDLE_AFTER, tail), Some(Status::Waiting));
    }

    #[test]
    fn notification_settles_on_the_next_tick_without_waiting_for_idle() {
        let t0 = Instant::now();
        let mut t = typed_at(t0);
        t.on_output(t0 + ms(400), 0);
        t.on_output(t0 + ms(500), 1);
        assert_eq!(t.tick(t0 + ms(550), || "all done".into()), Some(Status::Done));
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
        assert_eq!(t.tick(t0 + ms(550), || "all done".into()), Some(Status::Done));
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
}
```

…and add `mod status;` to `src/main.rs`.

- [x] **Step 2: Run:** `cargo test status`. Expected: compile errors (`StatusTracker`, `Status`, `IDLE_AFTER` not found).

- [x] **Step 3: Implement.** Put this above the test module:

```rust
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
}

impl StatusTracker {
    pub fn new() -> Self {
        Self { status: Status::Idle, last_input: None, last_output: None, attention: false }
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
    pub fn on_output(&mut self, now: Instant, notifications: usize) {
        if notifications > 0 {
            self.attention = true;
            self.last_output = Some(now);
            return;
        }
        let Some(input) = self.last_input else {
            return; // never typed in: shell startup / prompt noise
        };
        if now.duration_since(input) < ECHO_WINDOW {
            return; // echo of typing
        }
        self.last_output = Some(now);
        if !self.attention {
            self.status = Status::Working;
        }
    }

    /// The program rang the bell (a real BEL, reported by the parser). Claude Code uses it to say
    /// "look at me" in Terminal.app. It only counts while Working, so a shell's beep at a failed
    /// tab completion doesn't mark a session Done.
    pub fn on_bell(&mut self, now: Instant) {
        if self.status == Status::Working {
            self.attention = true;
            self.last_output = Some(now);
        }
    }

    /// Call periodically. `screen_tail` is only evaluated when the session settles.
    /// Returns the new status when it changed.
    pub fn tick(&mut self, now: Instant, screen_tail: impl FnOnce() -> String) -> Option<Status> {
        let quiet = self.last_output.is_some_and(|t| now.duration_since(t) >= IDLE_AFTER);
        let settling = self.attention || (self.status == Status::Working && quiet);
        if !settling {
            return None;
        }
        let next = if looks_like_waiting(&screen_tail()) { Status::Waiting } else { Status::Done };
        if next == self.status {
            return None;
        }
        self.status = next;
        Some(next)
    }
}
```

- [x] **Step 4: Run:** `cargo test status`. Expected: `test result: ok. 11 passed`.
- [x] **Step 5: Commit** *(when you say so)*: `git add src/status.rs src/main.rs && git commit -m "feat(m2): agent status heuristics with tests"`

### Task 3: `GridSnapshot::tail_text` — ✅

**Files:** Modify `src/grid.rs`

**Interfaces:** Produces `GridSnapshot::tail_text(&self, n: usize) -> String`: the last `n` non-blank screen lines, top to bottom, trailing spaces trimmed, joined with `\n`.

- [x] **Step 1: Write the failing test.** In `src/grid.rs`'s test module, add (just before `wide_glyph_spacer_is_skipped`):

```rust
    #[test]
    fn tail_text_returns_last_non_blank_lines() {
        let snap = snapshot(&term_with(b"one\r\n\r\ntwo  "));
        assert_eq!(snap.tail_text(8), "one\ntwo");
        assert_eq!(snap.tail_text(1), "two");
    }
```

- [x] **Step 2: Run:** `cargo test tail_text`. Expected: compile error `no method named \`tail_text\``.

- [x] **Step 3: Implement.** In `src/grid.rs`, directly after the `GridSnapshot` struct, add:

```rust
impl GridSnapshot {
    /// The last `n` non-blank screen lines (top to bottom), trailing spaces trimmed. Used by the
    /// status heuristics to look for a question near the bottom of the screen.
    pub fn tail_text(&self, n: usize) -> String {
        let mut tail: Vec<String> = self
            .lines
            .iter()
            .rev()
            .map(|line| line.iter().map(|c| c.ch).collect::<String>().trim_end().to_string())
            .filter(|text| !text.is_empty())
            .take(n)
            .collect();
        tail.reverse();
        tail.join("\n")
    }
}
```

- [x] **Step 4: Run:** `cargo test grid`. Expected: `test result: ok. 8 passed` (7 in `grid.rs`, plus M1's `resize_changes_the_grid_and_ignores_zero`, which the filter also matches).
- [x] **Step 5: Commit** *(when you say so)*: `git add src/grid.rs && git commit -m "feat(m2): screen tail text for status heuristics"`

### Task 4: Count notifications in the reader thread — ✅

**Files:** Modify `src/terminal.rs`

**Interfaces:** Changes `TermEvent::Output` → `TermEvent::Output { notifications: usize }` (notifications whose intro completed in that chunk, from Task 1's `OscDetector::feed`). Task 5 consumes it.

- [x] **Step 1: Write the failing test, and update the existing one.** In `src/terminal.rs`'s test module:
  - In `app_cursor_mode_is_visible_to_the_ui`, change `matches!(e, TermEvent::Output)` to `matches!(e, TermEvent::Output { .. })`.
  - Add a new test at the end of the module:

```rust
    #[test]
    fn notifications_are_counted() {
        let (tx, mut rx) = unbounded();
        let _core = TerminalCore::spawn(sh("printf '\\033]777;notify;fuxx;done\\007'; sleep 1"), TermDims { cols: 40, rows: 5 }, tx).unwrap();
        assert!(wait_for(&mut rx, |e| matches!(e, TermEvent::Output { notifications: 1 })));
    }
```

- [x] **Step 2: Run:** `cargo test terminal`. Expected: compile error (`TermEvent::Output` has no field `notifications`).

- [x] **Step 3: Implement.** Three edits in `src/terminal.rs`:

  (a) Add the import next to the other `crate::` imports:
```rust
use crate::detector::OscDetector;
```
  (b) Change the variant:
```rust
    /// New output was parsed into the screen: redraw. `notifications` = OSC 9 / 99 / 777
    /// notifications in it.
    Output { notifications: usize },
```
  (c) In the reader thread, create the detector before the loop and feed every chunk. The thread body becomes:
```rust
            .spawn(move || {
                let mut buf = [0u8; 8192];
                // alacritty_terminal ignores notification OSCs, so a byte scanner watches the raw stream.
                let mut detector = OscDetector::new();
                loop {
                    match reader.read(&mut buf) {
                        // EOF. portable-pty already maps macOS's EIO-at-exit to Ok(0).
                        Ok(0) => break,
                        Ok(n) => {
                            let notifications = detector.feed(&buf[..n]);
                            {
                                let mut guard = reader_shared.lock();
                                let Shared { term, processor } = &mut *guard;
                                processor.advance(term, &buf[..n]);
                            } // lock released before waking the UI
                            let _ = events.unbounded_send(TermEvent::Output { notifications });
                        }
                        Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                        Err(_) => break,
                    }
                }
                let _ = events.unbounded_send(TermEvent::Exited);
            })
```

- [x] **Step 4: Keep the crate compiling.** `cargo test` builds the whole crate, and
  `terminal_view.rs` still matches the old unit variant. In `on_term_event`, change
  `TermEvent::Output | TermEvent::Term(Event::Wakeup) => cx.notify(),` to
  `TermEvent::Output { .. } | TermEvent::Term(Event::Wakeup) => cx.notify(),` (Task 5 replaces this arm properly).
- [x] **Step 5: Run:** `cargo test terminal`. Expected: `test result: ok. 6 passed`.
- [x] **Step 6: Commit** *(when you say so)*: `git add src/terminal.rs src/terminal_view.rs && git commit -m "feat(m2): count terminal notifications in the PTY stream"`

### Task 5: Wire the tracker into `TerminalView` — ✅

**Files:** Modify `src/terminal_view.rs` · Modify `src/main.rs` (header comment)

**Interfaces:** Consumes `StatusTracker`/`Status`/`TAIL_LINES` and `on_bell` (Task 2), `tail_text` (Task 3), `TermEvent::Output { notifications }` (Task 4), and the parser's existing `TermEvent::Term(Event::Bell)`. Produces `TerminalView::status_changed(&self, Status, cx)`, the single place status changes are reported (M4 extends it).

- [x] **Step 1: Imports.** Change `use std::time::Duration;` to `use std::time::{Duration, Instant};`, and add:
```rust
use crate::status::{Status, StatusTracker, TAIL_LINES};
```
- [x] **Step 2: Field + constructor.** Add `tracker: StatusTracker,` to the struct after `title`, and initialize it in `new`:
```rust
        Self { core, focus_handle, title: None, tracker: StatusTracker::new(), _events: events, _ticker: ticker }
```
- [x] **Step 3: Tick + report.** Replace `on_tick` with these two methods:
```rust
    fn on_tick(&mut self, cx: &mut Context<Self>) {
        if self.core.flush_stale_sync() {
            cx.notify();
        }
        let core = &self.core;
        if let Some(status) = self.tracker.tick(Instant::now(), || core.snapshot().tail_text(TAIL_LINES)) {
            self.status_changed(status, cx);
        }
    }

    fn status_changed(&self, status: Status, cx: &mut Context<Self>) {
        eprintln!("[fuxx] status -> {status:?}"); // debug log, used by the M2 manual check
        cx.notify();
    }
```
- [x] **Step 4: Output.** In `on_term_event`, replace the arm `TermEvent::Output { .. } | TermEvent::Term(Event::Wakeup) => cx.notify(),` (from Task 4 Step 4) with:
```rust
            TermEvent::Output { notifications } => {
                self.tracker.on_output(Instant::now(), notifications);
                cx.notify();
            }
            TermEvent::Term(Event::Wakeup) => cx.notify(),
            // A real BEL (not an OSC terminator). Must come before the `TermEvent::Term(_)` catch-all.
            TermEvent::Term(Event::Bell) => self.tracker.on_bell(Instant::now()),
```
- [x] **Step 5: Input.** In `on_key_down`, right after `self.core.write(&bytes);`, add:
```rust
            if let Some(status) = self.tracker.on_input(Instant::now()) {
                self.status_changed(status, cx);
            }
```
- [x] **Step 6:** Change `main.rs`'s second header line to `// Milestone 2: one terminal whose agent status (working / waiting / done) is detected.`
- [x] **Step 7: Build with zero warnings:** `cargo build 2>&1 | grep -E "^(warning|error)" | grep -v "future version"`. Expected: **no output**.
- [x] **Step 8: All tests:** `cargo test`. Expected: `test result: ok. 46 passed`.
- [x] **Step 9: Commit** *(when you say so)*: `git add src/terminal_view.rs src/main.rs && git commit -m "feat(m2): detect and log agent status per terminal"`

### Task 6: Watch it work — 👀 ✅

Run `cargo run` **from another terminal**, so you can watch its stderr log while you use the fuxx window.

- [x] **Fresh window:** no `status ->` lines appear (stays Idle).
- [x] **Typing** `echo hi` + enter: no status line (echo + instant output aren't work).
- [x] **Working → Done:** run `bash -c 'for i in $(seq 1 15); do echo working $i; sleep 0.2; done'`. You should see `status -> Working`, then about 3 s after the last line, `status -> Done`.
- [x] **Working → Waiting:** run `bash -c 'for i in 1 2 3; do echo step $i; sleep 0.5; done; printf "Overwrite config? (y/n) "; read x'`. You should see `Working`, then about 3 s later, `Waiting`.
- [x] **Acknowledge:** answer `y` + enter. You should see `status -> Idle`.
- [x] **Notifications:** run each of these (type a key first, so the session isn't fresh). Each should show `Working`, then `Done` **immediately** (no 3 s wait):
  - OSC 9: `bash -c 'sleep 1; echo busy; sleep 1; printf "\033]9;done\007"'`
  - OSC 99: `bash -c 'sleep 1; echo busy; sleep 1; printf "\033]99;i=1:p=body;done\033\\"'`
  - OSC 777: `bash -c 'sleep 1; echo busy; sleep 1; printf "\033]777;notify;fuxx;done\007"'`
  - BEL: `bash -c 'sleep 1; echo busy; sleep 1; printf "\a"'`
- [x] **Progress bar isn't "done":** run `bash -c 'for i in 1 2 3 4 5; do printf "\033]9;4;1;${i}0\007"; echo step $i; sleep 0.5; done'`. You should see `Working` and **no** `Done` until about 3 s after the last step.
- [x] **Shell beep isn't "done":** in a fresh window, type `zzzq` then **tab** (zsh beeps: no completion). No `status ->` line should appear.
- [x] **Real agent:** run `claude` and give it a task. (It picks its notification type from the terminal you launched fuxx from; from Ghostty that's OSC 777, with the progress bar on.) Watch `Working` while it streams, then `Done` when it finishes, or `Waiting` when it asks permission. Note anything misread. If a prompt's wording isn't caught, add it (lowercased) to `WAITING_PATTERNS` with a test, and record the tuning in the Verification log.

### Task 7: Update the docs — ✅

- [x] [[42 - Milestone 2 - Agent State Detection]]: tick the checklist; **Status:: ✅ Done**; note D4 (Idle) and any tuned constants.
- [x] [[40 - Roadmap]]: M2 → ✅.
- [x] [[00 - Plan Tracker (Start Here)]]: M2 → ✅ 7/7; **Where we are** → M3 Task 1.
- [x] [[60 - Progress Log]]: new entry.
- [x] Commit docs *(when you say so)*.

## Verification log

| Date | Check | Result | Evidence |
|---|---|---|---|
| 2026-10-12 | Plan code compiles at pinned rev | ✅ 0 warnings | AI scratch build |
| 2026-10-12 | Plan unit tests | ✅ 38 passed | AI scratch `cargo test` |
| 2026-10-26 | Plan change (OSC 99/777, BEL, `9;4;` skip): Tasks 1–5 applied from this plan's text to a scratch worktree of `master` (`4a7d951`) | ✅ 0 warnings from our code | AI scratch build |
| 2026-10-26 | Same worktree: Task 1 baseline 6 passed → new tests 7 passed / 5 failed (the 5 named) → 12 passed; status 11; grid 8; terminal 6 | ✅ | AI scratch `cargo test` |
| 2026-10-26 | Same worktree: full `cargo test` | ✅ 46 passed | AI scratch `cargo test` |
| 2026-10-26 | Repo build + `cargo test` on `1843452` | ✅ 0 warnings from our code, 46 passed | AI build/test output |
| 2026-10-26 | Task 6 scenarios (incl. OSC 9/99/777, BEL, `9;4;` progress, tab beep, real `claude` from Ghostty) | ✅ | 👀 confirmed by you |
| 2026-10-26 | Constants tuned? | No: `ECHO_WINDOW` 150 ms, `IDLE_AFTER` 3 s, `TAIL_LINES` 8, `WAITING_PATTERNS` unchanged | 👀 nothing misread |

## Known limitations

| Limitation | Notes |
|---|---|
| Heuristic, not exact | Agents don't announce "waiting" consistently. Patterns and timings get tuned as real use reveals misses. |
| Status only logged | Rings in the sidebar are M4. |
| Notification payload text ignored | Only its presence matters (OSC 9 / 99 / 777). |
| Which notification an agent sends depends on the launching terminal | The shell inherits `TERM_PROGRAM` etc. from whatever launched fuxx (kept that way on 2026-10-26). Claude Code sends nothing when it doesn't recognize that terminal (for example fuxx launched from Finder later), and then only the 3 s quiet timer and the screen tail apply. |
| BEL only counts while Working | A program that beeps for attention *before* producing any output isn't caught. |

## Related

[[00 - Plan Tracker (Start Here)]] · [[M1 Plan - One Terminal Pane]] · [[M3 Plan - Many Sessions]]
