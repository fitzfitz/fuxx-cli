---
tags: [development, milestone]
status: done
---

# 44 - Milestone 4 - Notification Rings

**Status:: ✅ Done (2026-10-26). Rings and background notifications confirmed on screen, incl. real `claude` runs.**  ·  **First genuinely useful version.**
**Plan:** [[M4 Plan - Notification Rings]] (step-by-step, compile-verified, with status per task)

## Goal

Surface each session's state (from [[42 - Milestone 2 - Agent State Detection]]) as a **ring / status
badge** in the sidebar, and fire **desktop notifications** for background sessions that change state —
the product's payoff.

## Rough scope / checklist

- [x] Render a per-session ring/badge in the sidebar (working / done / waiting), updating live.
- [x] Fire a desktop notification when a **non-active** session becomes done / waiting (focus-aware —
      don't nag about the session you're looking at).
- [x] ~~Clicking a notification switches to that session.~~ **Moved to M12 (D5, decided 2026-10-26).**
      Not possible with `osascript` (clicking its banner opens Script Editor); a real `.app` can receive
      the click, as Ghostty and iTerm2 do. See [[00 - Plan Tracker (Start Here)]].
- [x] Notifications via `osascript` (macOS; v1 finding — see memory `macos-notify-rust-broken`).

## Success check

Run agents in background sessions; when one finishes or needs input, its ring updates **and** a
desktop notification fires. The active session doesn't nag. (Clicking it to jump to the session: M12, per D5.)

## Related

- [[40 - Roadmap]] (Phase 2 polish follows) · [[11 - Features]] (section C)
