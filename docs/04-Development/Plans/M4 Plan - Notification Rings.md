---
tags: [development, plan, milestone]
milestone: M4
status: done
updated: 2026-10-26
---

# M4 Plan — Notification Rings (first genuinely useful version)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

| At a glance | |
|---|---|
| **Milestone note** | [[44 - Milestone 4 - Notification Rings]] |
| **Status** | ✅ Done 2026-10-26 (commits `5af9f85`, `a0990fc` + docs on `m4-notification-rings`) |
| **Tasks done** | 4 / 4 |
| **▶ Pick up here** | M4 complete → M4b (tester build), plan to be written |
| **Decisions used** | ✅ **D5** = notification click → session moves to M11 (decided 2026-10-26) · ✅ **D4** = Idle ring |
| **Needs your eyes** | ✅ Task 3 confirmed by you on 2026-10-26 (all checks, incl. real `claude` runs) |

**Goal:** Every session row in the sidebar shows a live **status ring**, and when a session you're
**not looking at** finishes or needs input, a **desktop notification** fires. This is the payoff:
start agents, walk away, get pulled back exactly when needed.

**Architecture:**

```
 TerminalView.status_changed ──emit──► TerminalEvent::StatusChanged(new)
                                              │
 Workspace::on_terminal_event ──► session.status = new ──► sidebar ring repaints
                              └─► notifier::should_notify(old, new, is_active, window_active)?
                                        └─ yes ─► background thread: notifier::fire(title, body)  (osascript)
 Workspace.window_active ◄── cx.observe_window_activation
```

**Ring legend:** ◯ grey hollow = **Idle** · 🟠 amber = **Working** · 🔴 red = **Waiting** (needs you) · 🟢 green = **Done**.

**"Looking at it"** = the session is active **and** the fuxx window is focused. Only that case is
silent. If fuxx is in the background, even the active session notifies you.

**Tech Stack:** No new dependencies. Notifications use `osascript` ([[52 - Known Risks and Gotchas]]: `notify-rust` silently fails on modern macOS).

**Spec:** [[44 - Milestone 4 - Notification Rings]] · [[11 - Features]] (section C)

## Global Constraints

- Everything from the M0–M3 plans. **No new dependencies.**
- Notifications go **only** through `osascript` (v1's `notifier::fire`), run **off the UI thread** (it blocks ~100 ms).
- Session titles come from the programs (OSC 0/2) and are **untrusted**: escape them for AppleScript (`applescript_escape`).
- Notify only on a **change** into Done/Waiting, never for Working/Idle, never twice for the same state.
- Banners are attributed to **"Script Editor"** until the app is bundled (M11). That's expected.
- **D5:** "click a notification → jump to the session" is **not built in M4** (osascript can't deliver clicks back). It's recorded for M11.

## Review Focus

1. **Nagging about the session you're looking at** → test `never_nags_about_the_session_you_are_looking_at` (Task 1).
2. **Missing a finished agent because fuxx was in the background** → test `active_session_notifies_when_the_window_is_in_the_background` (Task 1).
3. **A malicious or odd title breaks or injects into the AppleScript** → test `escape_neutralises_quotes_backslashes_and_control_chars` (Task 1).
4. **Duplicate notifications for the same state** → test `ignores_working_idle_and_repeats` (Task 1).
5. **The UI freezing while a notification fires** → `fire` runs via `cx.background_spawn` (Task 2); Task 3 checks typing stays smooth.

## Files

| File | Change | Responsibility | Task |
|---|---|---|---|
| `src/notifier.rs` | Create (from v1 + new) | `fire` (osascript), `applescript_escape`, `should_notify`, `message` | 1 |
| `src/terminal_view.rs` | Modify | Emit `TerminalEvent::StatusChanged` | 2 |
| `src/workspace.rs` | Replace | Per-session status, rings, window-activation tracking, notifications | 2 |
| `src/main.rs` | Modify | `mod notifier;` + header | 1, 2 |

---

### Task 0: Decide D5 — ✅

- [x] *(Decided 2026-10-26: moved to M11, after comparing how Ghostty, iTerm2, Warp and Zed do it.)* In [[00 - Plan Tracker (Start Here)]] → **D5**. Recommended: move "clicking a notification
  switches to that session" to **M11** (a real `.app` can receive notification clicks). If you
  choose otherwise, this plan needs a new task *before* building. Don't improvise it.

### Task 1: `notifier.rs` — ✅

**Files:** Create `src/notifier.rs` · Modify `src/main.rs` (add `mod notifier;`)

**Interfaces:** Produces `fire(summary: &str, body: &str)`, `applescript_escape(&str) -> String`,
`should_notify(old: Status, new: Status, is_active_session: bool, window_active: bool) -> bool`,
`message(Status) -> Option<&'static str>` (`"Finished"` / `"Needs your input"`).

> **What's new vs v1.** v1's `fire` escaped inline and assumed fixed text. Titles are now
> program-controlled, so escaping is its own tested function and drops control characters. The
> focus-aware rule is a pure function, so the "don't nag" promise is unit-tested.

- [x] **Step 1: Write the failing tests.** Create `src/notifier.rs` with only:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notifies_for_background_sessions_that_finish_or_wait() {
        assert!(should_notify(Status::Working, Status::Done, false, true));
        assert!(should_notify(Status::Working, Status::Waiting, false, true));
    }

    #[test]
    fn never_nags_about_the_session_you_are_looking_at() {
        assert!(!should_notify(Status::Working, Status::Done, true, true));
    }

    #[test]
    fn active_session_notifies_when_the_window_is_in_the_background() {
        assert!(should_notify(Status::Working, Status::Done, true, false));
    }

    #[test]
    fn ignores_working_idle_and_repeats() {
        assert!(!should_notify(Status::Idle, Status::Working, false, false));
        assert!(!should_notify(Status::Done, Status::Done, false, false));
    }

    #[test]
    fn escape_neutralises_quotes_backslashes_and_control_chars() {
        assert_eq!(applescript_escape(r#"a"b\c"#), r#"a\"b\\c"#);
        assert_eq!(applescript_escape("x\ny\u{7}"), "xy");
    }

    #[test]
    fn messages() {
        assert_eq!(message(Status::Done), Some("Finished"));
        assert_eq!(message(Status::Waiting), Some("Needs your input"));
        assert_eq!(message(Status::Working), None);
    }
}
```

…and add `mod notifier;` to `src/main.rs`.

- [x] **Step 2: Run:** `cargo test notifier`. Expected: compile errors (`should_notify`, `message`, `applescript_escape` not found).
- [x] **Step 3: Implement.** Put this above the test module:

```rust
use std::process::Command;

use crate::status::Status;

/// Fire a native macOS desktop notification via `osascript`.
///
/// Best-effort: the command's result is intentionally discarded so a notification failure can
/// never crash or interrupt the app. It blocks for ~100 ms, so call it off the UI thread.
///
/// Why `osascript` and not a notifications crate: on modern macOS (tested on 26.x), the
/// `NSUserNotification` API that `notify-rust`/`mac-notification-sys` rely on is deprecated and
/// does not display notifications for an unbundled binary — `show()` succeeds but nothing
/// appears. `osascript`'s `display notification` goes through an entitled system component and
/// works without shipping fuxx as a signed `.app` bundle.
pub fn fire(summary: &str, body: &str) {
    let script = format!(
        "display notification \"{}\" with title \"{}\"",
        applescript_escape(body),
        applescript_escape(summary),
    );
    let _ = Command::new("osascript").arg("-e").arg(script).status();
}

/// Make text safe inside an AppleScript double-quoted string. Session titles are set by the
/// programs running in them (OSC 0/2), so this is untrusted input: escape `\` and `"`, and drop
/// control characters.
pub fn applescript_escape(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_control())
        .collect::<String>()
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
}

/// Focus-aware rule: notify when a session *changes* to Done or Waiting, unless you are looking
/// at it (it's the active session AND the fuxx window is focused).
pub fn should_notify(old: Status, new: Status, is_active_session: bool, window_active: bool) -> bool {
    let noteworthy = matches!(new, Status::Done | Status::Waiting);
    let looking_at_it = is_active_session && window_active;
    noteworthy && old != new && !looking_at_it
}

/// Notification body for a status, if that status is worth one.
pub fn message(status: Status) -> Option<&'static str> {
    match status {
        Status::Done => Some("Finished"),
        Status::Waiting => Some("Needs your input"),
        Status::Idle | Status::Working => None,
    }
}
```

- [x] **Step 4: Run:** `cargo test notifier`. Expected: `test result: ok. 6 passed` (dead-code warnings until Task 2).
- [x] **Step 5: Commit** *(when you say so)*: `git add src/notifier.rs src/main.rs && git commit -m "feat(m4): focus-aware osascript notifier with tests"`

### Task 2: Status events → rings + notifications — ✅

**Files:** Modify `src/terminal_view.rs` · Replace `src/workspace.rs` · Modify `src/main.rs`

**Interfaces:**
- Produces: `TerminalEvent::StatusChanged(Status)`; `Session.status: Status`; `Workspace.window_active: bool`; `ring(Status) -> impl IntoElement`.

> These three edits land together. Adding the `StatusChanged` variant makes the M3 workspace's
> `match` non-exhaustive until the new workspace handles it.

- [x] **Step 1: Emit status changes.** In `src/terminal_view.rs`:
  - Add the variant at the top of `pub enum TerminalEvent`:
```rust
    StatusChanged(Status),
```
  - In `status_changed`, emit before notifying:
```rust
    fn status_changed(&self, status: Status, cx: &mut Context<Self>) {
        eprintln!("[fuxx] status -> {status:?}"); // debug log, used by the M2 manual check
        cx.emit(TerminalEvent::StatusChanged(status));
        cx.notify();
    }
```

- [x] **Step 2: Replace `src/workspace.rs` with:**

```rust
//! The root view: a sidebar listing every session (with its status ring) and the active
//! session's terminal. Owns session lifecycle, switching, and background notifications.

use gpui::{
    ClickEvent, Context, Entity, Focusable, SharedString, Subscription, Window, actions, div,
    prelude::*, px, rgb,
};

use crate::colors;
use crate::notifier;
use crate::session_list::SessionList;
use crate::status::Status;
use crate::terminal_view::{TerminalEvent, TerminalView, hsla};

actions!(
    workspace,
    [
        NewSession,
        CloseSession,
        NextSession,
        PrevSession,
        SelectSession1,
        SelectSession2,
        SelectSession3,
        SelectSession4,
        SelectSession5,
        SelectSession6,
        SelectSession7,
        SelectSession8,
        SelectSession9,
    ]
);

const SIDEBAR_WIDTH: f32 = 220.0;

pub struct Session {
    pub id: u64,
    pub view: Entity<TerminalView>,
    pub status: Status,
    pub title: Option<SharedString>,
    _subscription: Subscription,
}

impl Session {
    pub fn label(&self) -> SharedString {
        self.title.clone().unwrap_or_else(|| format!("Shell {}", self.id).into())
    }
}

pub struct Workspace {
    sessions: SessionList<Session>,
    next_id: u64,
    window_active: bool,
    _activation: Subscription,
}

impl Workspace {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let activation = cx.observe_window_activation(window, |this, window, cx| {
            this.window_active = window.is_window_active();
            cx.notify();
        });
        let mut this = Self {
            sessions: SessionList::new(),
            next_id: 1,
            window_active: window.is_window_active(),
            _activation: activation,
        };
        this.add_session(window, cx);
        this
    }

    fn add_session(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let view = match TerminalView::spawn(window, cx) {
            Ok(view) => view,
            Err(err) => {
                eprintln!("fuxx: could not start a shell: {err}");
                return;
            }
        };
        let subscription = cx.subscribe_in(&view, window, Self::on_terminal_event);
        let id = self.next_id;
        self.next_id += 1;
        self.sessions.push(Session { id, view, status: Status::Idle, title: None, _subscription: subscription });
        self.focus_active(window, cx);
        cx.notify();
    }

    fn on_terminal_event(
        &mut self,
        view: &Entity<TerminalView>,
        event: &TerminalEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self.sessions.position(|s| s.view.entity_id() == view.entity_id()) else {
            return;
        };
        let is_active = self.sessions.active_index() == Some(index);
        match event {
            TerminalEvent::StatusChanged(new) => {
                let window_active = self.window_active;
                let Some(session) = self.sessions.get_mut(index) else { return };
                let old = session.status;
                session.status = *new;
                if notifier::should_notify(old, *new, is_active, window_active) {
                    if let Some(body) = notifier::message(*new) {
                        let summary = format!("fuxx — {}", session.label());
                        cx.background_spawn(async move { notifier::fire(&summary, body) }).detach();
                    }
                }
                cx.notify();
            }
            TerminalEvent::TitleChanged(title) => {
                if let Some(session) = self.sessions.get_mut(index) {
                    session.title = title.clone();
                }
                cx.notify();
            }
            TerminalEvent::Exited => self.close_at(index, window, cx),
        }
    }

    fn close_at(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        // Dropping the Session drops its view -> TerminalCore -> Pty, which kills the child.
        self.sessions.remove(index);
        if self.sessions.is_empty() {
            window.remove_window(); // last session gone: close the window (and quit)
            return;
        }
        self.focus_active(window, cx);
        cx.notify();
    }

    fn select(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.sessions.select(index) {
            self.focus_active(window, cx);
            cx.notify();
        }
    }

    fn focus_active(&self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(session) = self.sessions.active() {
            session.view.focus_handle(cx).focus(window, cx);
        }
    }

    fn render_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let active = self.sessions.active_index();
        div()
            .flex()
            .flex_col()
            .w(px(SIDEBAR_WIDTH))
            .h_full()
            .p_1()
            .gap_1()
            .bg(rgb(0x111111))
            .border_r_1()
            .border_color(rgb(0x2a2a2a))
            .children(self.sessions.items().iter().enumerate().map(|(i, session)| {
                let is_active = active == Some(i);
                div()
                    .id(("session", i))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .text_sm()
                    .text_color(rgb(if is_active { 0xf0f0f0 } else { 0x9a9a9a }))
                    .when(is_active, |row| row.bg(rgb(0x2a2a2a)))
                    .hover(|style| style.bg(rgb(0x222222)))
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.select(i, window, cx)))
                    .child(ring(session.status))
                    .child(format!("{}  {}", i + 1, session.label()))
            }))
    }
}

/// The status ring: hollow grey = idle, amber = working, red = needs you, green = done.
fn ring(status: Status) -> impl IntoElement {
    let dot = div().flex_none().size(px(10.0)).rounded_full();
    match status {
        Status::Idle => dot.border_1().border_color(rgb(0x6b6b6b)),
        Status::Working => dot.bg(rgb(0xf4bf75)),
        Status::Waiting => dot.bg(rgb(0xc55555)),
        Status::Done => dot.bg(rgb(0x90a959)),
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let title = self
            .sessions
            .active()
            .map(|s| format!("fuxx — {}", s.label()))
            .unwrap_or_else(|| "fuxx".into());
        window.set_window_title(&title);

        div()
            .flex()
            .flex_row()
            .size_full()
            .bg(hsla(colors::BACKGROUND))
            .key_context("Workspace")
            .on_action(cx.listener(|this, _: &NewSession, window, cx| this.add_session(window, cx)))
            .on_action(cx.listener(|this, _: &CloseSession, window, cx| {
                if let Some(i) = this.sessions.active_index() {
                    this.close_at(i, window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &NextSession, window, cx| {
                this.sessions.select_next();
                this.focus_active(window, cx);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &PrevSession, window, cx| {
                this.sessions.select_prev();
                this.focus_active(window, cx);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &SelectSession1, w, cx| this.select(0, w, cx)))
            .on_action(cx.listener(|this, _: &SelectSession2, w, cx| this.select(1, w, cx)))
            .on_action(cx.listener(|this, _: &SelectSession3, w, cx| this.select(2, w, cx)))
            .on_action(cx.listener(|this, _: &SelectSession4, w, cx| this.select(3, w, cx)))
            .on_action(cx.listener(|this, _: &SelectSession5, w, cx| this.select(4, w, cx)))
            .on_action(cx.listener(|this, _: &SelectSession6, w, cx| this.select(5, w, cx)))
            .on_action(cx.listener(|this, _: &SelectSession7, w, cx| this.select(6, w, cx)))
            .on_action(cx.listener(|this, _: &SelectSession8, w, cx| this.select(7, w, cx)))
            .on_action(cx.listener(|this, _: &SelectSession9, w, cx| this.select(8, w, cx)))
            .child(self.render_sidebar(cx))
            // Only the active terminal is rendered; background sessions keep running (their
            // reader threads and tickers live in their views, which the list keeps alive).
            .child(div().flex_1().h_full().children(self.sessions.active().map(|s| s.view.clone())))
    }
}
```

- [x] **Step 3:** In `src/main.rs`, make sure `mod notifier;` is present (Task 1), and change the second header line to
  `// Many sessions in one window, each with a live status ring and background notifications.`
- [x] **Step 4: Build with zero warnings:** `cargo build 2>&1 | grep -E "^(warning|error)" | grep -v "future version"`. Expected: **no output**.
- [x] **Step 5: Tests:** `cargo test`. Expected: `test result: ok. 61 passed`.
- [x] **Step 6: Commit** *(when you say so)*: `git add src/terminal_view.rs src/workspace.rs src/main.rs && git commit -m "feat(m4): status rings in the sidebar and background notifications"`

### Task 3: The payoff, for real — 👀 ✅

`cargo run`, then:

- [x] Every row has a **grey hollow ring** at first (Idle).
- [x] Session 1: run `bash -c 'for i in $(seq 1 15); do echo working $i; sleep 0.2; done'` and **stay on it**. The ring goes 🟠, then 🟢 about 3 s after it ends. **No notification** (you're looking at it).
- [x] Same command, then **immediately cmd-t** to another session. When it finishes, session 1's ring turns 🟢 **and** a banner **"fuxx — Shell 1 · Finished"** appears.
- [x] Session 1: `bash -c 'sleep 2; printf "Proceed? (y/n) "; read x'`, then switch away. Expect 🔴 plus a banner **"Needs your input"**. Switch back and type `y` → the ring goes grey (acknowledged).
- [x] Start the loop on the active session, then **click another app** so fuxx is in the background. When it finishes you **do** get a banner.
- [x] Real agents: `claude` in two sessions with different tasks; work in a third. You get pulled back exactly when each finishes or asks.
- [x] While a banner fires, typing in the active session stays smooth (no hitch).
- [x] *(Expected, not a bug:)* banners say **Script Editor**, and clicking one opens Script Editor (D5 → M11).

### Task 4: Update the docs (this completes Phase 1) — ✅

- [x] [[44 - Milestone 4 - Notification Rings]]: tick the checklist (the click item → "moved to M11 per D5"); **Status:: ✅ Done**.
- [x] [[40 - Roadmap]]: M4 → ✅, and add "notification click → session" under **M11**.
- [x] [[00 - Plan Tracker (Start Here)]]: M4 → ✅ 4/4; **Where we are** → "Phase 1 complete. Next: plan M5 (Phase 2)." *(Superseded: M4b, the tester build added 2026-10-26, comes next.)*
- [x] [[60 - Progress Log]]: new entry ("first genuinely useful version").
- [x] Commit docs *(when you say so)*.

## Verification log

| Date | Check | Result | Evidence |
|---|---|---|---|
| 2026-10-12 | Plan code compiles at pinned rev | ✅ 0 warnings | AI scratch build |
| 2026-10-12 | Plan unit tests | ✅ 50 passed | AI scratch `cargo test` |
| 2026-10-26 | Expected count 50 → 60 after the M2 plan changes (9 more M2 tests) and M3's writer-thread test | ⚠️ edited, not re-compiled | Re-verify when M4 starts |
| 2026-10-26 | Expected count 60 → 61: M3's Working fix added `output_reports_working_once`, and `StatusTracker::on_output` now returns `Option<Status>` (check M4's call sites against it) | ⚠️ edited, not re-compiled | Re-verify when M4 starts |
| 2026-10-26 | Plan text vs the merged M3 code (`8dc9b31`): the replacement `workspace.rs` is the M3 file plus the M4 additions (other differences are formatting only) | ✅ nothing from M3 dropped | AI diff |
| 2026-10-26 | Task 1 in the repo (`5af9f85`): RED (compile errors) → GREEN, notifier 6 passed | ✅ 61 passed; 4 dead-code warnings until Task 2, as planned | AI build/test output |
| 2026-10-26 | Task 2 in the repo (`a0990fc`): plan code compiled unchanged | ✅ 0 warnings from our code, 61 passed | AI build/test output (re-run by the controller) |
| 2026-10-26 | Task 3 checklist (rings, focus-aware banners, Waiting → grey on input, background window notifies, real `claude`, smooth typing) | ✅ | 👀 confirmed by you |

## Known limitations

| Limitation | Lands in |
|---|---|
| Clicking a notification doesn't jump to the session; banner says "Script Editor" | M11 `.app` bundle (D5) |
| Rings don't animate (Working is a solid amber dot) | Polish, if wanted |
| No notification history / activity feed | P2 in [[11 - Features]] |
| A green/red ring stays lit after you switch to that session; it goes grey once you type (the ring shows the detected state, not "unseen") | By design (M2 detection); revisit if it bites |

## Related

[[00 - Plan Tracker (Start Here)]] · [[M3 Plan - Many Sessions]] · [[40 - Roadmap]]
