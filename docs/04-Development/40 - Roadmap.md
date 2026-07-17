---
tags: [moc, development, roadmap]
---

# 40 - Roadmap

The full path to a daily-usable multi-agent terminal, in **phases**. One milestone at a time; verify
each before the next; record it in [[60 - Progress Log]]. Features referenced here are catalogued in
[[11 - Features]]. Far-phase milestones are directional — they get detailed notes when we reach them.

## Status legend

`⬜ Not started` · `🟨 In progress` · `✅ Done` · `⛔ Blocked`

---

## Phase 0 — Foundation (prove the stack)

- [[40a - Milestone 0 - Scaffold and GPUI Window]] — ⬜ — repurpose the crate; a blank GPUI window launches
- [[41 - Milestone 1 - One Terminal Pane]] — ⬜ — `alacritty_terminal` + `portable-pty` rendered in a GPUI view, running the shell. **Make-or-break.**

## Phase 1 — Core multi-agent terminal (the first genuinely useful version)

- [[42 - Milestone 2 - Agent State Detection]] — ⬜ — detect working / done / waiting per session (reuse v1's OSC-9 work)
- [[43 - Milestone 3 - Many Sessions]] — ⬜ — session manager + sidebar; quick-switch
- [[44 - Milestone 4 - Notification Rings]] — ⬜ — per-session status rings + background notifications. **← usable product.**

## Phase 2 — Daily driver (polish enough to live in it)

- **M5 — Terminal UX:** scrollback, selection + copy/paste, mouse, clickable URLs, robust resize.
- **M6 — Fonts & config:** font family/size/ligatures; read Ghostty's config (theme/colors).
- **M7 — Session ergonomics:** rename/reorder sessions, agent presets (one-tap `claude`/`opencode`), session restore.

## Phase 3 — Power features (cmux parity — big, deferred)

- **M8 — Split panes** (multiple sessions visible at once).
- **M9 — Socket/scripting API** per session (cmux-style programmability).
- **M10 — cmux extras:** in-app browser · SSH workspaces · remote/tmux daemon. Each is its own project.

## Phase 4 — Distribution

- **M11 — Ship as a real `.app`:** bundle + sign/notarize → fixes the notification identity (vs "Script Editor").
- **M12 — Homebrew cask** for install. (Later: Windows support.)

---

## The rule of one milestone at a time

Finish and verify each milestone (its own success check passes on the real machine) before starting
the next. Don't pull Phase 2+ features into the core — guard with [[12 - Scope]].

## Related

- [[11 - Features]] · [[20 - Architecture Overview]] · [[50 - Agentic Workflow]]
