---
tags: [moc, product, vision]
---

# 10 - What Is fuxx

The product / vision layer.

## What it is

fuxx is a **native, GPU-accelerated terminal application** whose reason to exist is **running AI
coding agents in parallel**. It's a real terminal (renders a full VT screen, runs any program), plus
a layer built for the agent workflow.

## The problem

Running several agents (Claude Code, OpenCode, Codex, …) at once means juggling many terminal windows
/ tabs and constantly checking "is this one done? is that one waiting for input?" A plain terminal
notifies at best per-window; you lose the big picture.

## The differentiator (vs a plain terminal)

Ghostty already shows OSC 9 notifications, so "ping when done" is *not* the product. fuxx's value is
**multi-agent orchestration**:

- One window, **many agent sessions**, each in a pane.
- A per-session **notification "ring" / status** — working / done / waiting-for-input — visible at a
  glance across all sessions.
- Quick-switch between sessions; bring the one that needs you to the front.

This is the "interesting 20%" of cmux, built as the terminal *you* own.

## Who it's for

Initially the author (you), on macOS, running multiple AI agents. A legitimate, focused first user.

## Relationship to cmux

Same core idea (a terminal built for AI agents, with notification rings). cmux is Swift/AppKit +
libghostty. fuxx is **Rust** (GPUI + alacritty_terminal) — chosen to keep the Rust investment and stay
cross-platform-capable. See [[70 - Terminal App Pivot — Vision and Stack]].

## Related

- [[12 - Scope]] · [[20 - Architecture Overview]] · [[40 - Roadmap]]
