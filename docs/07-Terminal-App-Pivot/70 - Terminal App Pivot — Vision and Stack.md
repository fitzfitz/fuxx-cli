---
tags: [pivot, terminal-app, planning]
---

# 70 - Terminal App Pivot — Vision & Stack

Recorded 2026-07-17. After shipping **fuxx-cli v1** (the Rust CLI wrapper, Milestones 0–3, on
`origin/master`), the project pivots to a **new, separate product**: a native terminal app in the
spirit of **cmux**, focused on running AI coding agents. This deliberately reverses the old
[[12 - Scope Boundaries]] / [[23 - Why Not libghostty]] (those governed the wrapper project, now done).

## Vision

A daily-usable terminal for running AI coding agents. The differentiator over a plain terminal
(Ghostty already shows OSC 9 notifications) is **multi-agent orchestration**: one window managing
many agent sessions, a **notification "ring" per session**, at-a-glance status
(working / done / waiting-for-input), quick-switch between them.

## Decision: build it in **Rust**

Considered two paths:
- **Swift + libghostty** (the literal cmux stack: SwiftUI/AppKit + GhosttyKit/Termini). Best terminal
  rendering, but Swift-only, macOS-only, and abandons the Rust investment. **Not chosen.**
- **Rust** — chosen, to keep the Rust investment and stay cross-platform-capable. The Rust terminal
  ecosystem is mature enough (2026): `alacritty_terminal` for emulation, and GPUI + `alacritty_terminal`
  is a proven combo for AI-agent terminals (tty7, Paneflow, zTerm).

## Chosen stack

- **App shell / UI:** **GPUI** (Zed's GPU-accelerated Rust UI framework) — for the session sidebar,
  panes, and notification rings. *Risk: pre-1.0, breaking changes; likely a git dep from the Zed repo.*
- **Terminal emulation:** **`alacritty_terminal`** (grid + VT parser, standalone library).
- **PTY:** **`portable-pty`** — reused from v1.
- **Rendering to vet:** **`gpui-ghostty`** — embeds a Ghostty-powered terminal in a GPUI app; could
  give libghostty-grade rendering from Rust. Verify maturity before relying on it.

## Proposed roadmap (start tiny, one buildable milestone at a time)

1. **M0 — Toolchain + hello window:** Rust already installed (from v1). Scaffold the app; get a blank
   **GPUI** window to launch. (Verify how to depend on GPUI — likely git dep from Zed.)
2. **M1 — One terminal pane:** `alacritty_terminal` + `portable-pty` running the shell, rendered in a
   GPUI view (or via `gpui-ghostty`). The make-or-break "does the core work" proof.
3. **M2 — One agent session:** run an agent in the pane; surface its state (reuse v1's OSC-detection).
4. **M3 — Many sessions:** sidebar/tab list; switch between panes.
5. **M4 — Notification rings:** per-session working/done/waiting status in the sidebar + notifications
   — the actual differentiator.
6. **Later (deferred):** split panes, read Ghostty's config, session restore, in-app browser, SSH.

## Open questions (for M0 brainstorming)

- **Repo layout:** new repo, or a workspace crate/subfolder alongside the shipped v1 CLI?
- **GPUI dependency:** exact way to consume it (git rev pinning, given pre-1.0).
- **`gpui-ghostty` vs hand-rolled rendering** over `alacritty_terminal` — vet before M1.

## Working model (changed from v1)

More of a Rust-native, testable project than the Swift path would have been — but GUI rendering still
needs **your** eyes (build & run, screenshots), like v1's `vim`/banner checks.

## Related

- Memory: `pivot-to-rust-terminal-app`, `fuxx-cli-v1-shipped`
- Legacy scope this reverses: [[12 - Scope Boundaries]] · [[23 - Why Not libghostty]]
