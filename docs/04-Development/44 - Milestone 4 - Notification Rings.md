---
tags: [development, milestone]
status: not-started
---

# 44 - Milestone 4 - Notification Rings

**Status:: ⬜ Not started**  ·  **First genuinely useful version.**

## Goal

Surface each session's state (from [[42 - Milestone 2 - Agent State Detection]]) as a **ring / status
badge** in the sidebar, and fire **desktop notifications** for background sessions that change state —
the product's payoff.

## Rough scope / checklist

- [ ] Render a per-session ring/badge in the sidebar (working / done / waiting), updating live.
- [ ] Fire a desktop notification when a **non-active** session becomes done / waiting (focus-aware —
      don't nag about the session you're looking at).
- [ ] Clicking a notification switches to that session.
- [ ] Notifications via `osascript` (macOS; v1 finding — see memory `macos-notify-rust-broken`).

## Success check

Run agents in background sessions; when one finishes or needs input, its ring updates **and** a
desktop notification fires; clicking it jumps to that session. The active session doesn't nag.

## Related

- [[40 - Roadmap]] (Phase 2 polish follows) · [[11 - Features]] (section C)
