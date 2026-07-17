---
tags: [moc, architecture]
---

# 20 - Architecture Overview

How fuxx is built, high level.

## The whole thing

```
   +--------------------------------------------------+
   |                    GPUI app                      |
   |  Application -> Window -> root view              |
   |                                                  |
   |  +-----------+   +----------------------------+  |
   |  | Sidebar   |   |  Active session pane       |  |
   |  | sessions  |   |  (TerminalView)            |  |
   |  | + rings   |   |                            |  |
   |  +-----------+   +----------------------------+  |
   +--------------------------------------------------+
        |  each session:
        v
   portable-pty (child shell/agent)  <-->  alacritty_terminal (grid + VT parser)
                                                 |
                                    TerminalView renders the grid (GPUI)
                                                 |
                                   agent-state detector -> ring status
```

## The pieces

1. **App shell — GPUI.** Window, layout, sidebar, panes, input routing, the rings UI. GPUI is Zed's
   GPU-accelerated Rust UI framework. See [[30 - Tech Stack]].
2. **Terminal core — `alacritty_terminal`.** Owns the screen grid + VT/escape parser for each session.
   Fed bytes from the PTY; we read its grid to render.
3. **PTY — `portable-pty`.** Spawns the shell/agent per session and pipes bytes to/from the terminal
   core. (Reused from v1.)
4. **TerminalView (we build).** A GPUI view that renders one session's `alacritty_terminal` grid
   (glyphs, colors, cursor) and routes keyboard/mouse input back to the PTY.
5. **Session manager (we build).** Holds N sessions `(pty, term, status)`; the sidebar lists them.
6. **Agent-state detector (we build).** Watches each session's output for signals (the OSC-9 detection
   from v1, plus heuristics for "waiting for input") → drives each session's **ring**.
7. **Notifications.** Fire when a background session changes state. (v1 learned: use `osascript` on
   macOS — see [[90-Archive-v1-CLI-Wrapper]] / memory `macos-notify-rust-broken`.)

## Key decisions

- **Rust, not Swift** — GPUI + alacritty_terminal instead of libghostty/Swift. See
  [[70 - Terminal App Pivot — Vision and Stack]].
- **We render the terminal ourselves** over `alacritty_terminal` (pure Rust; no Zig/Ghostty vendoring).
  The make-or-break milestone is proving this in [[41 - Milestone 1 - One Terminal Pane]].

## Related

- [[30 - Tech Stack]] · [[40 - Roadmap]]
