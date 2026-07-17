# Milestone 3 — Fire a Notification (Design Spec)

**Date:** 2026-07-17
**Milestone:** `docs/04-Development/43 - Milestone 3 - Fire a Notification.md`
**Builds on:** Milestone 2 (OSC 9 detection in the output pump).
**Status:** implemented; v1 complete.
**This completes v1.**

> **Revision (2026-07-17, during implementation):** the notification backend changed from
> `notify-rust` to **`osascript`**. `notify-rust`'s macOS backend (`mac-notification-sys`) uses
> the deprecated `NSUserNotification` API, which does **not** display for an unbundled CLI binary
> on modern macOS (verified 26.5.2) — `show()` returns `Ok` but no banner appears, and no
> bundle-id attribution fixes it. `src/notifier.rs` now shells out to `osascript`
> (`display notification`), which works without an `.app` bundle. The `notify-rust` dependency
> was dropped; `notifier::fire(summary, body)`'s signature is unchanged, so §4's wiring and the
> rest of this design still hold. See [[52 - Known Risks and Gotchas]] and [[33 - Key Crates]].

## 1. Goal

When Milestone 2 detects an OSC 9 sequence, show a **real native macOS desktop notification**
instead of the throwaway `NOTIFICATION DETECTED` line M2 wrote to stderr. After this, running an
agent through fuxx and having it finish produces an actual banner — a complete, useful tool.

## 2. Scope

**In scope (M3):**
- Add `notify-rust` and a small, decoupled `notifier` module that fires a desktop notification.
- On OSC 9 detection, fire the notification (replacing the M2 stderr line).
- A temporary "prove the crate + permissions work" startup notification during development,
  removed once detection wiring lands.

**Out of scope — v1 stops here (per the milestone note "Stop adding features here"):**
- Notification throttling/debouncing.
- Extracting the OSC payload for the notification body (still deferred from M2) — the body is
  generic.
- OSC 99 / 777 variants; ConEmu disambiguation.
- Configuration, sound, the agent's name, icons.
- Homebrew packaging (that is Milestone 4).

## 3. Dependency (verified crates.io/docs.rs 2026-07-17; Context7 unreachable)

- **`notify-rust = "4.18.0"`** — cross-platform desktop notifications; already planned in
  `docs/03-Tech-Stack/33 - Key Crates.md`.
- **macOS notes** (confirmed on docs.rs): `Notification::new().summary(..).body(..).show()` is the
  synchronous call and works on macOS via the `mac-notification-sys` backend. `icon()`, `appname()`,
  and `hint()` are no-ops/unsupported on macOS — **do not use them**; summary + body only.
- macOS shows the banner attributed to the running terminal app; the OS may require notification
  permission for that terminal. The startup proof step (§5) exists to surface a permission problem
  early.

## 4. Architecture

Decoupled per `docs/02-Architecture/26 - Notification Dispatch.md`: detection decides *that*
something happened; dispatch decides *how the user is told*.

### `src/notifier.rs` (new) — dispatch

```rust
use notify_rust::Notification;

/// Fire a desktop notification. Best-effort: a failure to display must never crash the wrapper,
/// so the Result is intentionally discarded.
pub fn fire(summary: &str, body: &str) {
    let _ = Notification::new().summary(summary).body(body).show();
}
```

### `src/main.rs` (edit) — wire detection → dispatch

In the output pump's `Ok(n)` branch, the M2 line:
```rust
if detector.feed(&buf[..n]) > 0 {
    eprintln!("NOTIFICATION DETECTED");
}
```
becomes:
```rust
if detector.feed(&buf[..n]) > 0 {
    notifier::fire("fuxx-cli", "Your agent sent a notification");
}
```
Add `mod notifier;` and `use ...` as needed. The passthrough write above it stays unchanged; the
detector is unchanged.

## 5. The "prove it works" step (staged, then removed)

`notify-rust` + macOS permissions are the kind of thing that silently fails, so we prove it before
relying on it:
- **During implementation (Task 1):** temporarily call `notifier::fire("fuxx-cli", "notifier test")`
  once at the start of `run()`. Launch fuxx, confirm a banner appears (grant permission if macOS
  prompts). This validates the crate and permissions in isolation.
- **Then (Task 2):** remove that startup call and wire the notification to detection instead. fuxx
  must **not** fire a notification on every launch.

## 6. Testing — and the tradeoff

Firing a real banner is an OS effect that cannot be asserted in an automated test, and any test that
ran fuxx on an OSC 9 input would fire a real banner on every `cargo test`. Therefore:

- **Removed:** the two Milestone 2 integration tests that asserted the `NOTIFICATION DETECTED`
  **stderr** string (`detects_osc9_notification_on_stderr` and `no_detection_without_a_sequence`) —
  that stderr behavior is intentionally gone. (Approved during brainstorming.)
- **Kept (regression):** the `OscDetector` unit tests (full detection-logic coverage) and the
  wrapper integration tests (`echo` passthrough, exit-code propagation, no-args, unknown-command).
- **Manual verification** (the milestone's success check):
  1. Startup proof (Task 1): a banner appears when fuxx launches.
  2. Detection → banner (Task 2): `cargo run -- printf '\033]9;done\007'` shows a macOS banner; and
     the real end-to-end check — run an agent through fuxx and see a banner when it finishes.

`cargo test` stays clean and fires no banners, because no remaining test exercises the notifier.

## 7. What you'll learn (Rust)

Adding and using a third-party crate, keeping modules decoupled (detector vs notifier vs wiring),
best-effort error handling (`let _ = ...`), and end-to-end wiring of a feature.

## 8. After this

v1 is complete and useful. The only remaining roadmap item is
`docs/04-Development/44 - Milestone 4 - Ship via Homebrew.md` (distribution), which is optional and
separate. Do not add more features to the core tool.
