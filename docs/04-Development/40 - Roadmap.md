---
tags: [moc, development, roadmap]
---

# 40 - Roadmap

The full path to a daily-usable multi-agent terminal, in **phases**. One milestone at a time; verify
each before the next; record it in [[60 - Progress Log]]. Features referenced here are catalogued in
[[11 - Features]]. Far-phase milestones are directional — they get detailed notes when we reach them.

> [!tip] Step-by-step plans
> M0–M4b each have a detailed, compile-verified implementation plan with per-task status.
> **Start at [[00 - Plan Tracker (Start Here)]]**: current position, next action, open decisions.

## Status legend

`⬜ Not started` · `🟨 In progress` · `✅ Done` · `⛔ Blocked`

---

## Phase 0 — Foundation (prove the stack)

- [[40a - Milestone 0 - Scaffold and GPUI Window]] — ✅ — repurpose the crate; a blank GPUI window launches
- [[41 - Milestone 1 - One Terminal Pane]] — ✅ — `alacritty_terminal` + `portable-pty` rendered in a GPUI view, running the shell. **Make-or-break.**

## Phase 1 — Core multi-agent terminal (the first genuinely useful version)

- [[42 - Milestone 2 - Agent State Detection]] — ✅ — detect working / done / waiting per session (reuse v1's OSC-9 work)
- [[43 - Milestone 3 - Many Sessions]] — ✅ — session manager + sidebar; quick-switch
- [[44 - Milestone 4 - Notification Rings]] — ✅ — per-session status rings + background notifications. **← usable product.**
- [[44b - Milestone 4b - Tester Build]] — ✅ — wrap the release binary in an **unsigned, universal (Apple Silicon + Intel, D7) `.app`**, ship it in a versioned **DMG** (drag to Applications) so testers can try the MVP. *Added 2026-10-26 on request.* No signing/notarization (that stays M11), so testers clear Gatekeeper once per version (System Settings → Privacy & Security → Open Anyway). Opened from Finder, detection relies on the quiet timer and screen patterns (D8). Plan: [[M4b Plan - Tester Build]].

## Phase 2 — Daily driver (polish enough to live in it)

- [[45 - Milestone 5 - Terminal UX]] — ✅ — scrollback, selection + copy/paste, mouse, clickable URLs, robust resize. Plan: [[M5 Plan - Terminal UX]].
- [[46 - Milestone 6 - Fonts and Config]] — ✅ — font family/size/ligatures and colors/themes from **fuxx's own settings file** (`~/.config/fuxx/config`; changed 2026-10-27 from reading Ghostty's config); **prompt icons** for everyone via a bundled Symbols Nerd Font. Plan: [[M6 Plan - Fonts and Config]].
- **M6b — Settings window** — ⬜ — *added 2026-10-27 on request.* **File → Preferences…** and **cmd-,** open a settings window: pick the theme (built-in + your themes folder), font, size and ligatures, and override theme colors; changes are saved to `~/.config/fuxx/config` and applied without a restart. Its detailed plan is written when M6 is done. Open decision **D9** (how colors are entered) — see [[00 - Plan Tracker (Start Here)]].
- **M7 — Session ergonomics:** project groups (sessions grouped by project folder, rolled-up ring), rename/reorder sessions, agent presets (one-tap `claude`/`opencode`), session restore.

## Phase 3 — Power features (cmux parity — big, deferred)

- **M8 — Split panes** (multiple sessions visible at once).
- **M9 — Socket/scripting API** per session (cmux-style programmability).
- **M10 — cmux extras:** in-app browser · SSH workspaces · remote/tmux daemon. Each is its own project.

## Phase 4 — Distribution

- **M11 — Ship as a real `.app`:** take M4b's bundle and sign/notarize it → fixes the notification identity (vs "Script Editor"), and **clicking a notification switches to that session** (moved from M4 by D5, 2026-10-26). Universal (Apple Silicon + Intel) per **D7**, decided 2026-10-26.
- **M12 — Homebrew cask** for install. (Later: Windows support.)

---

## The rule of one milestone at a time

Finish and verify each milestone (its own success check passes on the real machine) before starting
the next. Don't pull Phase 2+ features into the core — guard with [[12 - Scope]].

## Related

- [[11 - Features]] · [[20 - Architecture Overview]] · [[50 - Agentic Workflow]]
