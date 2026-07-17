---
tags: [workflow, risks]
---

# 52 - Known Risks and Gotchas

Things likely to trip up **fuxx** (the terminal app), recorded so they don't surprise you (or an AI).

## Technical

- **GPUI is pre-1.0.** Consumed as a **pinned git dependency** from the Zed repo; breaking changes
  happen between revs. Pin the rev and bump deliberately. See [[30 - Tech Stack]].
- **Heavy first build.** GPUI pulls a large dependency tree — the first `cargo build` is slow.
- **GUI can't be auto-verified.** The AI can't see the window — terminal rendering, input, and rings
  are verified by *you* (build & run, screenshots). Design each task so its success check is something
  observable, and say so.
- **Rendering the `alacritty_terminal` grid is the hard part** ([[41 - Milestone 1 - One Terminal Pane]])
  — glyph shaping, colors, perf. Fallback if it's too costly: `gpui-ghostty` (needs the Zig toolchain).
- **macOS notifications:** use **`osascript`**, not `notify-rust` (its deprecated `NSUserNotification`
  backend silently no-ops on modern macOS). Memory: `macos-notify-rust-broken`.
- **Crate / API drift:** verify versions and APIs via **Context7** or the source repo; don't trust
  remembered numbers.

## Process

- **Scope creep toward cmux-parity / a general IDE.** The core is a terminal + multi-agent rings;
  split panes, browser, SSH, and a daemon are deferred ([[12 - Scope]]). Don't pull them into the core.
- **Over-planning stalls solo projects.** One milestone at a time; don't spec far-future phases in
  detail early.
- **Lost progress** between sessions if [[60 - Progress Log]] isn't updated.
- **Accepting code you don't understand** — see [[51 - Delegation Principles]].

## Carried over from v1

The v1 wrapper's technical risks (TTY-vs-pipe, OSC-variant parsing) are archived in
[[90-Archive-v1-CLI-Wrapper]]. The OSC-9 detection reused in
[[42 - Milestone 2 - Agent State Detection]] still cares about byte-level parsing across read chunks.

## Related

- [[50 - Agentic Workflow]] · [[40 - Roadmap]] · [[30 - Tech Stack]]
