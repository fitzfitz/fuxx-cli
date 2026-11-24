---
tags: [moc, architecture]
---

# 20 - Architecture Overview

How fuxx is built, high level.

## The whole thing

```
   +------------------------------------------------------------------------+
   |                               GPUI app                                 |
   |  Application -> Window -> Workspace (root view)                        |
   |                                                                        |
   |  +-----------+  +-------------------------------------+  +-----------+ |
   |  | Sidebar   |  | Split panes, each with a tab bar:   |  | Right     | |
   |  | workspaces|  |  terminal tab  (TerminalView)       |  | sidebar   | |
   |  | + status  |  |  agent tab     (AgentTab, a chat)   |  | Files     | |
   |  | badges    |  |  browser tab   (WebKit page)        |  | Changes   | |
   |  |           |  |  file tab      (read-only viewer)   |  | Activity  | |
   |  +-----------+  +-------------------------------------+  +-----------+ |
   |  title area · status bar (status, branch, ports, usage) · overlays     |
   +------------------------------------------------------------------------+
     terminal tab:  portable-pty (shell/agent) <-> alacritty_terminal (grid + VT parser)
                    -> TerminalView paints the grid; detector + status -> the marks
     agent tab:     agent_process (claude / agy in stream-json) -> providers/<agent>.rs
                    translator -> Event -> Conversation -> chat rows, lanes, activity
     outside:       private socket <- `fuxx` command / `fuxx mcp`; optional `fuxx daemon`
                    owns sessions after quit; SSH runs local `ssh`; plugins; updater
```

## The pieces

1. **App shell — GPUI.** Window, the `Workspace` root view, sidebar, panes, tab bars, overlays (⌘K palette,
   menus, Preferences), input routing. The window chrome (sidebar, tab bars, status bar, title area) is drawn as
   cached parts that redraw only when their own data changes (M11p). See [[30 - Tech Stack]].
2. **Layout model.** Workspaces → split panes → tabs (`layout_model`, `split_tree`), saved and restored
   (`restore`). A tab is a terminal, an agent tab, a browser page or a file.
3. **Terminal core — `alacritty_terminal`.** Owns the screen grid + VT/escape parser for each session.
   Fed bytes from the PTY by a reader and a parser thread; we read its grid to render.
4. **PTY — `portable-pty`.** Spawns the shell/agent per session and pipes bytes to/from the terminal
   core. (Reused from v1.)
5. **TerminalView (we build).** A GPUI view that renders one session's grid (glyphs, colors, cursor) and routes
   keyboard/mouse input back to the PTY. fuxx's zsh integration adds OSC 133 marks for command blocks, the
   chat-style card view (M11ab) and "which command finished" (M11q).
6. **Agent-state detection (we build).** Watches each terminal's output (OSC 9/99/777, BEL, quiet time, screen
   patterns) → working / waiting / done / idle → the status marks, banners and ⇧⌘U.
7. **Agent tabs (we build, M11aa onward).** fuxx runs Claude Code or Antigravity as a child process in its
   stream-json mode (`agent_process`). Each agent's wire format lives in its own provider file
   (`src/providers/claude.rs`, `src/providers/agy.rs`, with `activity.rs` submodules); generic code only speaks
   fuxx's own `Event` language (`providers/mod.rs`). The `Conversation` folds events into chat items, subagent
   lanes and a provider-neutral `Activity` per tool call, which the step model, file tracks and turn ledger draw.
8. **Notifications.** fuxx's own banners from the app bundle through UserNotifications (M11c; a click shows the
   terminal that raised it), falling back to `osascript`. Never `notify-rust` (v1 learned it silently fails — see
   [[90-Archive-v1-CLI-Wrapper]] / memory `macos-notify-rust-broken`).
9. **Scripting and services.** A private same-user socket answers the `fuxx` command and `fuxx mcp` (M9); an
   opt-in `fuxx daemon` keeps sessions running after quit (M10a); SSH sessions run your local `ssh` (M10b); a
   WebKit view per browser tab, drivable by agents (M10c, M11b).
10. **Around the panes.** The right sidebar (Files, Changes with staging, commit, branch bar and Graph, Activity),
   read-only file tabs (syntect colours, live changes, Markdown reader with Mermaid), plugins installed from GitHub
   (M11y–M11z, M11au), the updater (M11e).

The module-by-module map is `STRUCTURE.md` at the repo root.

## Key decisions

- **Rust, not Swift** — GPUI + alacritty_terminal instead of libghostty/Swift. See
  [[70 - Terminal App Pivot — Vision and Stack]].
- **We render the terminal ourselves** over `alacritty_terminal` (pure Rust; no Zig/Ghostty vendoring).
  The make-or-break milestone proved this in [[41 - Milestone 1 - One Terminal Pane]].
- **Agent tabs speak each agent's own machine-readable mode** (stream-json), one provider file per agent, so
  agent quirks stay in `src/providers/<agent>.rs` and the chat is provider-neutral
  ([[Proposal - Chat View and Agent Tabs]], [[79 - Milestone 11aa - Agent Tabs]]). An ACP client is planned later
  ([[40 - Roadmap]]).

## Related

- [[30 - Tech Stack]] · [[40 - Roadmap]]
