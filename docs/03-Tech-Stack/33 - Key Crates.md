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

## notify-rust — desktop notifications

Fires native OS notifications. This is the whole output side of
[[26 - Notification Dispatch]]. On macOS it produces a standard banner.

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
