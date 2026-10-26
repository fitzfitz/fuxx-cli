---
tags: [development, milestone]
status: not-started
---

# 43 - Milestone 3 - Many Sessions

**Status:: ⬜ Not started**
**Plan:** [[M3 Plan - Many Sessions]] (step-by-step, compile-verified, with status per task)

## Goal

Manage **multiple sessions** in one window: a session manager holding N `(pty, term, status)`, a
**sidebar** listing them, and quick-switch of which one is shown in the main pane.

## Rough scope / checklist

- [ ] Session manager: create / destroy / list sessions, each its own PTY + `Term` (from M1).
- [ ] Sidebar view (GPUI) listing sessions; click to switch the active pane.
- [ ] Route input/rendering to the active session; keep background sessions running + updating.
- [ ] New-session action (spawns a shell; agent presets come in Phase 2).

- [ ] *(Plan change 2026-10-26)* PTY writes on a per-session writer thread, so one program that stops
      reading input can't freeze the app. Shortcuts: D6 (recommended set).

## Success check

Open 3 sessions, switch between them via the sidebar; each keeps running independently; the active one
renders and takes input.

## Related

- [[44 - Milestone 4 - Notification Rings]] · [[11 - Features]] (section B)
