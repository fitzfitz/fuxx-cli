---
tags: [tech, reference, dependencies]
---

# 33 - Key Crates

The external Rust libraries ("crates") fuxx-cli depends on. Verify current versions on
crates.io before adding — versions below are illustrative, not pinned.

## clap — argument parsing

Handles turning `fuxx claude --foo` into structured data, plus `--help` and errors for
free. The de-facto standard Rust CLI parser. Powers the arg layer in
[[21 - The Wrapper Process Model]].

## Desktop notifications — `osascript`, not `notify-rust`

The output side of [[26 - Notification Dispatch]]. **Originally planned to use the
`notify-rust` crate, but dropped in Milestone 3.** `notify-rust`'s macOS backend
(`mac-notification-sys`) relies on the **deprecated `NSUserNotification` API, which does not
display notifications for an unbundled CLI binary on modern macOS** (verified on 26.5.2):
`show()` returns `Ok` but no banner ever appears, and no bundle-id attribution fixes it.

Instead, `src/notifier.rs` shells out to **`osascript`** (`display notification … with title …`),
which goes through an entitled system component and works without shipping fuxx as a signed
`.app` bundle. No crate dependency; uses `std::process::Command`. macOS-only, matching v1 scope.
See [[52 - Known Risks and Gotchas]]. A proper app identity (instead of "Script Editor") would
require an `.app` bundle — that's [[44 - Milestone 4 - Ship via Homebrew|Milestone 4]]+ territory.

## crossterm — raw-mode control for our own terminal

`portable-pty` manages the child's terminal; our own terminal still needs raw mode so keystrokes
forward to the child instantly instead of being line-buffered/echoed by our shell. `crossterm`'s
`enable_raw_mode` / `disable_raw_mode` handle this. Added in Milestone 1. Also the likely home for
the deferred window-resize handling.

## Process handling — std first, PTY later

- Start with the standard library's `std::process::Command` to spawn the agent and read
  its output. Enough to prove the concept.
- If the agent misbehaves because it isn't talking to a real terminal, add a PTY crate
  (e.g. `portable-pty`) so the child still sees a "real" terminal while fuxx-cli reads
  the stream. This is the known subtlety in [[21 - The Wrapper Process Model]].

## (Future) ratatui — terminal UI

Only if the [[24 - Future - Daemon and Dashboard|dashboard]] is ever built. Not v1.

## Related

- [[32 - Cargo and Project Layout]]
- [[52 - Known Risks and Gotchas]]
