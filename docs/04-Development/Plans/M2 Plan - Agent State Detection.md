---
tags: [development, plan, milestone]
milestone: M2
status: not-started
updated: 2026-10-12
---

# M2 Plan — Agent State Detection

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

| At a glance | |
|---|---|
| **Milestone note** | [[42 - Milestone 2 - Agent State Detection]] |
| **Status** | ⬜ Not started (waits for M1 ✅) |
| **Tasks done** | 0 / 7 |
| **▶ Pick up here** | **Task 1, Step 1** |
| **Decisions used** | ❓ **D4** = a 4th status `Idle` (see [[00 - Plan Tracker (Start Here)]]) |
| **Needs your eyes** | 👀 Task 6 (watch the status log while scripts and a real agent run) |

**Goal:** Each terminal knows whether its program is **Working**, **Waiting** for you, or **Done**
(plus **Idle** when there's nothing to report), and logs every change so you can watch it.

**Architecture:** A pure `StatusTracker` (time is passed in, so every rule is unit-tested) is fed
three signals:
1. **Output timing.** Output means Working. Output within 150 ms of a keystroke is just the echo of your typing.
2. **OSC 9.** Agents send `ESC ]9;…` to say "look at me". `alacritty_terminal` ignores OSC 9,
   so v1's byte scanner (`OscDetector`, restored from git) watches the raw stream in the reader thread.
3. **The bottom of the screen.** When a session goes quiet (3 s) or sends OSC 9, a question
   pattern near the bottom (`(y/n)`, `do you want to`…) means **Waiting**; otherwise **Done**.

Typing into a Waiting/Done session means you've seen it, so it returns to **Idle**.

```
 reader thread: bytes ─► OscDetector.feed ─► TermEvent::Output { osc9 } ─► tracker.on_output
 keystroke ───────────────────────────────────────────────────────────────► tracker.on_input
 50 ms ticker ─► tracker.tick(now, || screen tail text) ─► Some(new status) ─► log "[fuxx] status -> …"
```

**Tech Stack:** No new dependencies.

**Spec:** [[42 - Milestone 2 - Agent State Detection]] · [[11 - Features]] (section C) · v1's OSC work in [[90-Archive-v1-CLI-Wrapper]]

## Global Constraints

- Everything from the M0/M1 plans (pinned GPUI, zero warnings from our code, you verify GUI).
- **No new dependencies.**
- OSC 9 detection runs on the **raw PTY bytes**, before/alongside the parser (alacritty ignores OSC 9). It must survive a sequence split across two reads (v1's detector already does).
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

## Files

| File | Change | Responsibility | Task |
|---|---|---|---|
| `src/detector.rs` | Create (restore v1) | `OscDetector`: count `ESC ]9;` across chunks | 1 |
| `src/status.rs` | Create | `Status` + `StatusTracker` heuristics | 2 |
| `src/grid.rs` | Modify | `GridSnapshot::tail_text(n)` | 3 |
| `src/terminal.rs` | Modify | Reader thread counts OSC 9 → `TermEvent::Output { osc9 }` | 4 |
| `src/terminal_view.rs` | Modify | Own a `StatusTracker`; feed it; log changes | 5 |
| `src/main.rs` | Modify | `mod detector; mod status;` | 1, 2 |

---

### Task 1: Restore v1's `OscDetector` — ⬜

**Files:** Create `src/detector.rs` · Modify `src/main.rs` (add `mod detector;`)

**Interfaces:** Produces `detector::OscDetector::{new() -> OscDetector, feed(&mut self, &[u8]) -> usize}` (number of complete `ESC ]9;` intros seen; carries partial matches across calls; ignores OSC 99).

- [ ] **Step 1: Restore it from git** (it was deleted in M0; commit `eb3eaad` still has it):

```bash
git show eb3eaad:src/detector.rs > src/detector.rs
```
It must match this exactly (v1 code, unchanged):

```rust
/// OSC 9 desktop-notification intro: ESC ] 9 ;
const TARGET: [u8; 4] = [0x1b, 0x5d, 0x39, 0x3b];

pub struct OscDetector {
    /// How many leading bytes of TARGET have matched so far (0..TARGET.len()).
    matched: usize,
}

impl OscDetector {
    pub fn new() -> Self {
        OscDetector { matched: 0 }
    }

    pub fn feed(&mut self, bytes: &[u8]) -> usize {
        let mut detections = 0;
        for &b in bytes {
            if b == TARGET[self.matched] {
                self.matched += 1;
                if self.matched == TARGET.len() {
                    detections += 1;
                    self.matched = 0; // ready for the next sequence
                }
            } else if b == TARGET[0] {
                // The byte that broke the match is itself ESC: begin a fresh match from it,
                // so e.g. "ESC ESC ]9;" is still detected.
                self.matched = 1;
            } else {
                self.matched = 0;
            }
        }
        detections
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_full_intro_in_one_chunk() {
        let mut d = OscDetector::new();
        assert_eq!(d.feed(b"\x1b]9;hello\x07"), 1);
    }

    #[test]
    fn detects_intro_split_across_chunks() {
        let mut d = OscDetector::new();
        assert_eq!(d.feed(b"some output\x1b"), 0);
        assert_eq!(d.feed(b"]9;done\x07"), 1);
    }

    #[test]
    fn does_not_detect_osc_99() {
        let mut d = OscDetector::new();
        assert_eq!(d.feed(b"\x1b]99;progress\x07"), 0);
    }

    #[test]
    fn ignores_plain_text() {
        let mut d = OscDetector::new();
        assert_eq!(d.feed(b"hello world\n"), 0);
    }

    #[test]
    fn counts_two_intros_in_one_chunk() {
        let mut d = OscDetector::new();
        assert_eq!(d.feed(b"\x1b]9;a\x07\x1b]9;b\x07"), 2);
    }

    #[test]
    fn restarts_on_repeated_esc() {
        let mut d = OscDetector::new();
        // A stray ESC before the real intro must not swallow the match.
        assert_eq!(d.feed(b"\x1b\x1b]9;x\x07"), 1);
    }
}
```

- [ ] **Step 2:** Add `mod detector;` to `src/main.rs`.
- [ ] **Step 3: Run:** `cargo test detector`. Expected: `test result: ok. 6 passed` (v1's tests; a dead-code warning is expected until Task 4).
- [ ] **Step 4: Commit** *(when you say so)*: `git add src/detector.rs src/main.rs && git commit -m "feat(m2): restore v1 OSC 9 detector"`

### Task 2: `status.rs` — the heuristics — ⬜

**Files:** Create `src/status.rs` · Modify `src/main.rs` (add `mod status;`)

**Interfaces:**
- Produces:
  - `pub enum Status { Idle, Working, Waiting, Done }` (`Copy + Eq + Debug`).
  - Constants `ECHO_WINDOW`, `IDLE_AFTER`, `TAIL_LINES`, `WAITING_PATTERNS`; `pub fn looks_like_waiting(&str) -> bool`.
  - `StatusTracker::new()`, `on_input(&mut self, Instant) -> Option<Status>`, `on_output(&mut self, Instant, osc9: usize)`,
    `tick(&mut self, Instant, screen_tail: impl FnOnce() -> String) -> Option<Status>` (returns the new status only when it changed; `screen_tail` is called only when settling),
    `status()` (test-only).

> **The rules in one table.**
>
> | Event | Effect |
> |---|---|
> | Output, never typed yet | ignored (startup noise) |
> | Output < 150 ms after a keystroke | ignored (echo) |
> | Other output | → **Working** (unless an OSC 9 is pending) |
> | OSC 9 | mark "attention"; settle on the next tick |
> | Tick: attention, or Working and quiet ≥ 3 s | → **Waiting** if a question pattern is in the last 8 lines, else **Done** |
> | Keystroke while Waiting/Done | → **Idle** (acknowledged); clears attention |

- [ ] **Step 1: Write the failing tests.** Create `src/status.rs` with only:

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
    fn osc9_settles_on_the_next_tick_without_waiting_for_idle() {
        let t0 = Instant::now();
        let mut t = typed_at(t0);
        t.on_output(t0 + ms(400), 0);
        t.on_output(t0 + ms(500), 1);
        assert_eq!(t.tick(t0 + ms(550), || "all done".into()), Some(Status::Done));
    }

    #[test]
    fn output_after_osc9_does_not_flip_back_to_working_until_input() {
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
    fn waiting_patterns_are_case_insensitive() {
        assert!(looks_like_waiting("Overwrite file? (Y/n)"));
        assert!(looks_like_waiting("DO YOU WANT TO continue"));
        assert!(!looks_like_waiting("Compiling fuxx v0.1.0"));
    }
}
```

…and add `mod status;` to `src/main.rs`.

- [ ] **Step 2: Run:** `cargo test status`. Expected: compile errors (`StatusTracker`, `Status`, `IDLE_AFTER` not found).

- [ ] **Step 3: Implement.** Put this above the test module:

```rust
//! Agent-state heuristics: is a session working, waiting for you, or done?
//!
//! Signals: output timing, OSC 9 notifications (v1's detector), and the screen's last lines.
//! Pure — the caller passes the time in — so every rule is unit-tested. The constants are a
//! first guess; tune them against real agents during the M2 manual check.

use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// Nothing to report yet (fresh session, or only startup/echo output so far).
    Idle,
    /// Producing output.
    Working,
    /// Went quiet with a question on screen (or after OSC 9 with a question).
    Waiting,
    /// Went quiet (or sent OSC 9) with no question on screen.
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
    /// An OSC 9 arrived since the last keystroke: settle on the next tick.
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

    /// The session produced output; `osc9` = OSC 9 notifications seen in it.
    pub fn on_output(&mut self, now: Instant, osc9: usize) {
        if osc9 > 0 {
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

- [ ] **Step 4: Run:** `cargo test status`. Expected: `test result: ok. 9 passed`.
- [ ] **Step 5: Commit** *(when you say so)*: `git add src/status.rs src/main.rs && git commit -m "feat(m2): agent status heuristics with tests"`

### Task 3: `GridSnapshot::tail_text` — ⬜

**Files:** Modify `src/grid.rs`

**Interfaces:** Produces `GridSnapshot::tail_text(&self, n: usize) -> String`: the last `n` non-blank screen lines, top to bottom, trailing spaces trimmed, joined with `\n`.

- [ ] **Step 1: Write the failing test.** In `src/grid.rs`'s test module, add (just before `wide_glyph_spacer_is_skipped`):

```rust
    #[test]
    fn tail_text_returns_last_non_blank_lines() {
        let snap = snapshot(&term_with(b"one\r\n\r\ntwo  "));
        assert_eq!(snap.tail_text(8), "one\ntwo");
        assert_eq!(snap.tail_text(1), "two");
    }
```

- [ ] **Step 2: Run:** `cargo test tail_text`. Expected: compile error `no method named \`tail_text\``.

- [ ] **Step 3: Implement.** In `src/grid.rs`, directly after the `GridSnapshot` struct, add:

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

- [ ] **Step 4: Run:** `cargo test grid`. Expected: `test result: ok. 7 passed`.
- [ ] **Step 5: Commit** *(when you say so)*: `git add src/grid.rs && git commit -m "feat(m2): screen tail text for status heuristics"`

### Task 4: Count OSC 9 in the reader thread — ⬜

**Files:** Modify `src/terminal.rs`

**Interfaces:** Changes `TermEvent::Output` → `TermEvent::Output { osc9: usize }` (OSC 9 intros in that chunk). Task 5 consumes it.

- [ ] **Step 1: Write the failing test, and update the existing one.** In `src/terminal.rs`'s test module:
  - In `app_cursor_mode_is_visible_to_the_ui`, change `matches!(e, TermEvent::Output)` to `matches!(e, TermEvent::Output { .. })`.
  - Add a new test at the end of the module:

```rust
    #[test]
    fn osc9_notifications_are_counted() {
        let (tx, mut rx) = unbounded();
        let _core = TerminalCore::spawn(sh("printf '\\033]9;done\\007'; sleep 1"), TermDims { cols: 40, rows: 5 }, tx).unwrap();
        assert!(wait_for(&mut rx, |e| matches!(e, TermEvent::Output { osc9: 1 })));
    }
```

- [ ] **Step 2: Run:** `cargo test terminal`. Expected: compile error (`TermEvent::Output` has no field `osc9`).

- [ ] **Step 3: Implement.** Three edits in `src/terminal.rs`:

  (a) Add the import next to the other `crate::` imports:
```rust
use crate::detector::OscDetector;
```
  (b) Change the variant:
```rust
    /// New output was parsed into the screen: redraw. `osc9` = OSC 9 notifications in it.
    Output { osc9: usize },
```
  (c) In the reader thread, create the detector before the loop and feed every chunk. The thread body becomes:
```rust
            .spawn(move || {
                let mut buf = [0u8; 8192];
                // alacritty_terminal ignores OSC 9, so v1's byte scanner watches the raw stream.
                let mut detector = OscDetector::new();
                loop {
                    match reader.read(&mut buf) {
                        // EOF. portable-pty already maps macOS's EIO-at-exit to Ok(0).
                        Ok(0) => break,
                        Ok(n) => {
                            let osc9 = detector.feed(&buf[..n]);
                            {
                                let mut guard = reader_shared.lock();
                                let Shared { term, processor } = &mut *guard;
                                processor.advance(term, &buf[..n]);
                            } // lock released before waking the UI
                            let _ = events.unbounded_send(TermEvent::Output { osc9 });
                        }
                        Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                        Err(_) => break,
                    }
                }
                let _ = events.unbounded_send(TermEvent::Exited);
            })
```

- [ ] **Step 4: Keep the crate compiling.** `cargo test` builds the whole crate, and
  `terminal_view.rs` still matches the old unit variant. In `on_term_event`, change
  `TermEvent::Output | TermEvent::Term(Event::Wakeup) => cx.notify(),` to
  `TermEvent::Output { .. } | TermEvent::Term(Event::Wakeup) => cx.notify(),` (Task 5 replaces this arm properly).
- [ ] **Step 5: Run:** `cargo test terminal`. Expected: `test result: ok. 6 passed`.
- [ ] **Step 6: Commit** *(when you say so)*: `git add src/terminal.rs src/terminal_view.rs && git commit -m "feat(m2): count OSC 9 notifications in the PTY stream"`

### Task 5: Wire the tracker into `TerminalView` — ⬜

**Files:** Modify `src/terminal_view.rs` · Modify `src/main.rs` (header comment)

**Interfaces:** Consumes `StatusTracker`/`Status`/`TAIL_LINES` (Task 2), `tail_text` (Task 3), `TermEvent::Output { osc9 }` (Task 4). Produces `TerminalView::status_changed(&self, Status, cx)`, the single place status changes are reported (M4 extends it).

- [ ] **Step 1: Imports.** Change `use std::time::Duration;` to `use std::time::{Duration, Instant};`, and add:
```rust
use crate::status::{Status, StatusTracker, TAIL_LINES};
```
- [ ] **Step 2: Field + constructor.** Add `tracker: StatusTracker,` to the struct after `title`, and initialize it in `new`:
```rust
        Self { core, focus_handle, title: None, tracker: StatusTracker::new(), _events: events, _ticker: ticker }
```
- [ ] **Step 3: Tick + report.** Replace `on_tick` with these two methods:
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
- [ ] **Step 4: Output.** In `on_term_event`, replace the arm `TermEvent::Output { .. } | TermEvent::Term(Event::Wakeup) => cx.notify(),` (from Task 4 Step 4) with:
```rust
            TermEvent::Output { osc9 } => {
                self.tracker.on_output(Instant::now(), osc9);
                cx.notify();
            }
            TermEvent::Term(Event::Wakeup) => cx.notify(),
```
- [ ] **Step 5: Input.** In `on_key_down`, right after `self.core.write(&bytes);`, add:
```rust
            if let Some(status) = self.tracker.on_input(Instant::now()) {
                self.status_changed(status, cx);
            }
```
- [ ] **Step 6:** Change `main.rs`'s second header line to `// Milestone 2: one terminal whose agent status (working / waiting / done) is detected.`
- [ ] **Step 7: Build with zero warnings:** `cargo build 2>&1 | grep -E "^(warning|error)" | grep -v "future version"`. Expected: **no output**.
- [ ] **Step 8: All tests:** `cargo test`. Expected: `test result: ok. 38 passed`.
- [ ] **Step 9: Commit** *(when you say so)*: `git add src/terminal_view.rs src/main.rs && git commit -m "feat(m2): detect and log agent status per terminal"`

### Task 6: Watch it work — 👀 ⬜

Run `cargo run` **from another terminal**, so you can watch its stderr log while you use the fuxx window.

- [ ] **Fresh window:** no `status ->` lines appear (stays Idle).
- [ ] **Typing** `echo hi` + enter: no status line (echo + instant output aren't work).
- [ ] **Working → Done:** run `bash -c 'for i in $(seq 1 15); do echo working $i; sleep 0.2; done'`. You should see `status -> Working`, then about 3 s after the last line, `status -> Done`.
- [ ] **Working → Waiting:** run `bash -c 'for i in 1 2 3; do echo step $i; sleep 0.5; done; printf "Overwrite config? (y/n) "; read x'`. You should see `Working`, then about 3 s later, `Waiting`.
- [ ] **Acknowledge:** answer `y` + enter. You should see `status -> Idle`.
- [ ] **OSC 9:** run `bash -c 'sleep 1; echo busy; sleep 1; printf "\033]9;done\007"'`. You should see `Working`, then `Done` **immediately** (no 3 s wait).
- [ ] **Real agent:** run `claude` and give it a task. Watch `Working` while it streams, then `Done` when it finishes, or `Waiting` when it asks permission. Note anything misread. If a prompt's wording isn't caught, add it (lowercased) to `WAITING_PATTERNS` with a test, and record the tuning in the Verification log.

### Task 7: Update the docs — ⬜

- [ ] [[42 - Milestone 2 - Agent State Detection]]: tick the checklist; **Status:: ✅ Done**; note D4 (Idle) and any tuned constants.
- [ ] [[40 - Roadmap]]: M2 → ✅.
- [ ] [[00 - Plan Tracker (Start Here)]]: M2 → ✅ 7/7; **Where we are** → M3 Task 1.
- [ ] [[60 - Progress Log]]: new entry.
- [ ] Commit docs *(when you say so)*.

## Verification log

| Date | Check | Result | Evidence |
|---|---|---|---|
| 2026-10-12 | Plan code compiles at pinned rev | ✅ 0 warnings | AI scratch build |
| 2026-10-12 | Plan unit tests | ✅ 38 passed | AI scratch `cargo test` |
| | Task 6 scenarios | | 👀 stderr log |
| | Constants tuned? | | |

## Known limitations

| Limitation | Notes |
|---|---|
| Heuristic, not exact | Agents don't announce "waiting" consistently. Patterns and timings get tuned as real use reveals misses. |
| Status only logged | Rings in the sidebar are M4. |
| OSC 9 payload text ignored | Only its presence matters. Payload / OSC 99 / 777 remain out of scope (same as v1). |

## Related

[[00 - Plan Tracker (Start Here)]] · [[M1 Plan - One Terminal Pane]] · [[M3 Plan - Many Sessions]]
