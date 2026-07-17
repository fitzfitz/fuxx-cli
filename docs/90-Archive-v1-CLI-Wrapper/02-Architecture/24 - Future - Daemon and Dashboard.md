---
tags: [architecture, future, v2]
---

# 24 - Future - Daemon and Dashboard

Explicitly-parked ideas. Not for v1. Recorded so they don't get lost or pull focus.

## Possible v2+ directions

- **Background daemon** tracking many agents at once, not just one wrapped command.
- **Status dashboard / TUI** showing every running agent and its state (this is the
  closest fuxx-cli would get to the cmux "vertical tabs" feeling). Rust's `ratatui`
  crate would be the tool here.
- **Richer notification content** — which agent, which project, what it's waiting for.
- **Cross-platform** support (Linux notifications, then maybe Windows).

## Why parked

Each of these multiplies complexity. v1 must ship first — a single wrapped agent that
notifies reliably. See [[12 - Scope Boundaries]] and [[40 - Development Roadmap]].

## Related

- [[20 - Architecture Overview]]
- [[40 - Development Roadmap]]
