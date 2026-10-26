---
tags: [development, milestone]
status: done
---

# 43 - Milestone 3 - Many Sessions

**Status:: ✅ Done (2026-10-26). Three+ sessions, sidebar, switching and background running confirmed on screen.**
D6 shortcuts: **cmd-t** new · **cmd-w** close (no confirmation) · **cmd-1…9** jump · **cmd-{ / cmd-}** previous/next · **cmd-q** quit.
**Plan:** [[M3 Plan - Many Sessions]] (step-by-step, compile-verified, with status per task)

## Goal

Manage **multiple sessions** in one window: a session manager holding N `(pty, term, status)`, a
**sidebar** listing them, and quick-switch of which one is shown in the main pane.

## Rough scope / checklist

- [x] Session manager: create / destroy / list sessions, each its own PTY + `Term` (from M1).
- [x] Sidebar view (GPUI) listing sessions; click to switch the active pane.
- [x] Route input/rendering to the active session; keep background sessions running + updating.
- [x] New-session action (spawns a shell; agent presets come in Phase 2).

- [x] *(Plan change 2026-10-26)* PTY writes on a per-session writer thread, so one program that stops
      reading input can't freeze the app. Shortcuts: D6 (recommended set).

- [x] *(Found during the on-screen check)* **Working** is now reported: since M2 the status log
      only ever showed Done / Waiting / Idle. Fixed in `StatusTracker::on_output` (`6973016`).

## Success check

Open 3 sessions, switch between them via the sidebar; each keeps running independently; the active one
renders and takes input.

## Related

- [[44 - Milestone 4 - Notification Rings]] · [[11 - Features]] (section B)
