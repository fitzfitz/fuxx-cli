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
- [[44b - Milestone 4b - Tester Build]] — ✅ — wrap the release binary in an **unsigned, universal (Apple Silicon + Intel, D7) `.app`**, ship it in a versioned **DMG** (drag to Applications) so testers can try the MVP. *Added 2026-10-26 on request.* No signing/notarization (that stays M12), so testers clear Gatekeeper once per version (System Settings → Privacy & Security → Open Anyway). Opened from Finder, detection relies on the quiet timer and screen patterns (D8). Plan: [[M4b Plan - Tester Build]].

## Phase 2 — Daily driver (polish enough to live in it)

- [[45 - Milestone 5 - Terminal UX]] — ✅ — scrollback, selection + copy/paste, mouse, clickable URLs, robust resize. Plan: [[M5 Plan - Terminal UX]].
- [[46 - Milestone 6 - Fonts and Config]] — ✅ — font family/size/ligatures and colors/themes from **fuxx's own settings file** (`~/.config/fuxx/config`; changed 2026-10-27 from reading Ghostty's config); **prompt icons** for everyone via a bundled Symbols Nerd Font. Plan: [[M6 Plan - Fonts and Config]].
- [[46b - Milestone 6b - Settings Window]] — ✅ — *added 2026-10-27 on request.* **File → Preferences…** and **cmd-,** open a settings window: pick the theme (built-in + your themes folder), font, size and ligatures (JetBrains Mono now ships with fuxx); changes are saved to `~/.config/fuxx/config` and applied without a restart. Plan: [[M6b Plan - Settings Window]]. **D9** decided 2026-10-27: colors are picked from the theme list only (no hex typing).
- [[47 - Milestone 7 - Session Ergonomics]] — ✅ — project groups (sessions grouped by project folder, rolled-up ring), rename/reorder sessions, agent presets (one-tap `claude`/`opencode`/`codex` + your own), session restore (configurable). Plan: [[M7 Plan - Session Ergonomics]].

> [!note] Order changed 2026-10-27, on request
> You asked to build **M7 and M8 straight away, while you check M6b on screen**, and to confirm all
> three milestones' on-screen checks afterwards. So M6b, M7 and M8 are each built and reviewed in turn, but
> their 👀 checks are done together at the end. If a check fails, the fix goes into the milestone it belongs to.

- **M8 — Split panes** (multiple sessions visible at once). *Moved into scope and up to Phase 2 on
  2026-10-27, on request:* nested splits like Ghostty/iTerm. — ✅ — Note: [[48 - Milestone 8 - Split Panes]]. Plan: [[M8 Plan - Split Panes]].

## Phase 3 — Power features (cmux parity — big, deferred)

- [[49 - Milestone 9 - Scripting API]] — ✅ — socket + `fuxx` command, agent-aware status/question/answer/wait, event stream, sidebar note/progress, and `fuxx mcp` (goes beyond cmux / iTerm2 / Warp, on request 2026-10-27). Plan: [[M9 Plan - Scripting API]].
- **M10 — cmux extras**, split into three (2026-10-27, your go; evidence: [[Input - M10 Feasibility Study]]):
  - [[50 - Milestone 10a - Persistent Sessions]] — ✅ — a background `fuxx daemon` owns the sessions; the window
    reattaches. **Opt-in** (your choice): by default quitting still ends sessions. Notifications keep coming while
    the window is closed (your choice). Scripting with the window closed comes later (your choice).
  - [[51 - Milestone 10b - SSH Workspaces]] — 🟨 built and reviewed 2026-11-06, on-screen check pending (batched with M10c and M11): [[M10b Plan - SSH Workspaces]]. A session runs local `ssh` (host picker from `~/.ssh/config`, connection reuse,
    reconnect). A remote fuxx daemon is deferred.
  - [[52 - Milestone 10c - Agent Browser]] — 🟨 built and reviewed 2026-11-06, on-screen check pending (batched with M11): [[M10c Plan - Agent Browser]]. Your choice: agents automating it, like cmux; a separate browser window per session (AI's call). A WebKit window per session, driven by `fuxx browser …` and MCP tools.

## Phase 4 — Polish and distribution

> [!note] Renumbered 2026-10-27, on request
> A new **M11 — UI/UX uplift** comes before shipping. The signed `.app` moved from M11 to **M12**, and Homebrew
> from M12 to **M13**. Older notes and log entries written before this date use the old numbers.

- **M11 — UI/UX uplift** — 🟨 built and reviewed 2026-11-06, on-screen check pending (batched with M10b and M10c): [[M11 Plan - UI-UX Uplift]], spec [[53 - Milestone 11 - UI-UX Uplift]] *(added 2026-10-27 on request)*. Integrated titlebar and padding, a sidebar that reads at a glance (question under a waiting row), shaped status marks with motion you can turn off (Animations setting + macOS Reduce motion), split pane headers, ⇧⌘U and ⌘P, redesigned Preferences, light themes with a contrast floor.
  Input: [[M11 Input - UI-UX Review]] (honest review + ranked plan, 2026-10-27). Kept after M9 and M10 on purpose (your call, 2026-10-27): those may change the layout, so polish comes last.
  **After your batched on-screen check** (M10b, M10c, M11): merge M10b / M10c / M11 / the follow-up into `master`, then M11b.
- **M10c/M11 follow-up — browser bar, keep-data setting, tabbed Preferences, sidebar button, tab-like rows** — 🟨 built and reviewed 2026-11-06, on-screen check pending (batched with M10b, M10c, M11): [[M10c-M11 Follow-up Plan - Browser Bar and Tabbed Preferences]] (amendments in [[52 - Milestone 10c - Agent Browser]] and [[53 - Milestone 11 - UI-UX Uplift]]; your requests, 2026-11-06). Next: your batched check → merge M10b / M10c / M11 / follow-up → M11b.
- [[54 - Milestone 11b - Workspaces, Pane Tabs and In-Pane Browser]] (added 2026-11-06 on request; ▶ in progress, planned 2026-11-06: [[M11b Plan - Workspaces, Pane Tabs and In-Pane Browser]]; you asked to build it before your batched check): the cmux layout — workspaces in the sidebar (rich rows: branch • folder, Needs-input badge, pinning, accent-filled selection), split panes each with their own tab bar, terminal or browser tabs (the browser inside a pane with history suggestions), titlebar 🔔 and +. Input: [[M11b Input - Workspaces, Pane Tabs and In-Pane Browser]].
- **M12 — Ship as a real `.app`:** (after M11b; not planned yet) take M4b's bundle and sign/notarize it → fixes the notification identity (vs "Script Editor"), and **clicking a notification switches to that session** (moved from M4 by D5, 2026-10-26). Universal (Apple Silicon + Intel) per **D7**, decided 2026-10-26.
- **M13 — Homebrew cask** for install. (Later: Windows support.)

---

## The rule of one milestone at a time

Finish and verify each milestone (its own success check passes on the real machine) before starting
the next. Don't pull Phase 2+ features into the core — guard with [[12 - Scope]].

## Related

- [[11 - Features]] · [[20 - Architecture Overview]] · [[50 - Agentic Workflow]]
