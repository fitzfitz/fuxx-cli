---
tags: [moc, development, plans, tracker]
updated: 2026-10-26
---

# 00 - Plan Tracker (Start Here)

**Open this first every session.** It shows where the build stands, the one next action, what's
blocked, and which decisions are waiting on you. Each milestone has a step-by-step plan, linked
below. Every task carries a status indicator, and every step is a checkbox.

> [!tip] Picking up in 60 seconds
> 1. Read **Where we are** below.
> 2. Open the current milestone's plan and jump to its **▶ Pick up here** line.
> 3. Do the next unchecked step. Tick it off. When a task is done, flip its status emoji.
> 4. End of session: update this tracker's **Where we are** + [[60 - Progress Log]].

## Where we are

| | |
|---|---|
| **Current milestone** | **M4 — Notification Rings** |
| **Status** | 🟨 In progress on branch `m4-notification-rings`. M3 ✅ done 2026-10-26 (merged into `master` and pushed). |
| **▶ Next action** | [[M4 Plan - Notification Rings]] → Task 1. |
| **Blocked on** | Nothing. D5 decided (click → session moves to M11). |
| **Last updated** | 2026-10-26 |

## Legend

| Indicator | Meaning |
|---|---|
| ⬜ | Not started |
| 🟨 | In progress |
| ✅ | Done and verified (tests pass, plus your on-screen check for GUI) |
| ⛔ | Blocked (the blocker is written next to it) |
| ❓ | Waiting on a decision from you (see **Open decisions**) |
| 👀 | Needs *your* eyes (GUI check the AI can't do) |

## Milestones (Phase 0 → Phase 1)

| # | Milestone | Plan | Status | Tasks | Success check (short) |
|---|---|---|---|---|---|
| M0 | Scaffold & GPUI window | [[M0 Plan - Scaffold and GPUI Window]] | ✅ | 4/4 | A window opens; closing it quits |
| M1 | One terminal pane (**make-or-break**) | [[M1 Plan - One Terminal Pane]] | ✅ | 7/7 | Usable shell: `ls --color`, `vim`, resize |
| M2 | Agent state detection | [[M2 Plan - Agent State Detection]] | ✅ | 7/7 | Status log shows working → done / waiting |
| M3 | Many sessions + sidebar | [[M3 Plan - Many Sessions]] | ✅ | 6/6 | 3 sessions, switch, all keep running |
| M4 | Notification rings (**usable product**) | [[M4 Plan - Notification Rings]] | 🟨 | 0/4 | Rings update; background sessions notify |
| M4b | Tester build (unsigned `.app` in a DMG) — *added 2026-10-26* | written when M4 is done | ⬜ ❓D7 ❓D8 | — | A tester drags fuxx to Applications and opens it |

Phase 2+ (M5–M12) stays a one-line roadmap entry on purpose: it gets detailed plans when we reach
it ([[40 - Roadmap]], and "over-planning stalls solo projects" in [[52 - Known Risks and Gotchas]]).

## Open decisions (❓ — yours to make)

Each plan uses the **recommended** option. Say if you want the other one, and the plan gets
updated before anyone builds it.

| ID | Decision | Recommended | Alternative | Affects |
|---|---|---|---|---|
| **D1** ✅ | How to build GPUI without full Xcode (**decided 2026-10-25: `runtime_shaders`; launch confirmed 2026-10-26**) | **Enable GPUI's `runtime_shaders` feature**: Metal shaders compile when the app starts, so no build-time `xcrun metal`. *Built 2026-10-25; launch confirmed on screen 2026-10-26.* | Install full Xcode (~10+ GB) and `xcode-select` it | M0 |
| **D2** ✅ | (**decided 2026-10-26: GPUI's own text system**) How to draw terminal glyphs ([[30 - Tech Stack]] left it open: "`cosmic-text`/`glyphon` or GPUI text") | **GPUI's own text system** (`shape_line` + `paint`). This is what Zed's terminal does, and it needs no extra crate. | `cosmic-text` + `glyphon` | M1 |
| **D3** ✅ | (**decided 2026-10-26: `futures` crate**) Channel from the PTY reader thread to the UI | **`futures` crate** (`futures::channel::mpsc`). It's what Zed uses, and it's already in `Cargo.lock` through GPUI, so there's no extra download. It's a new *direct* dependency, though. | `std::sync::mpsc` + a polling timer (no new dep, but wakes the CPU constantly) | M1 |
| **D4** ✅ | (**decided 2026-10-26: Idle**, the recommended option, used by the confirmed M2 plan) A 4th status, **Idle** (grey hollow ring), for a fresh session that hasn't done anything yet | **Yes**. Without it, every new shell would show "Done" | Only working / waiting / done | M2, M4 |
| **D5** ✅ | (**decided 2026-10-26: move to M11**, the recommended option, after comparing Ghostty / iTerm2 / Warp / Zed) "Clicking a notification switches to that session" ([[44 - Milestone 4 - Notification Rings]]) **can't work with `osascript`**: clicking its banner opens Script Editor, not fuxx | **Move that one item to M11** (the real `.app` bundle can receive notification clicks) | Add `terminal-notifier` (new external tool, outside the plan), or drop the item | M4 |
| **D6** ✅ | (**decided 2026-10-26: the recommended set**) Keyboard shortcuts (the vault doesn't specify any) | **cmd-t** new session · **cmd-w** close session (kills its shell, no confirmation) · **cmd-1…9** jump · **cmd-{ / cmd-}** previous/next · **cmd-q** quit | Any other set | M0, M3 |
| **D8** | How agent-state detection works when fuxx is opened from Finder/Dock (the M4b `.app`). Launched that way the shell inherits no terminal identity, so Claude Code sends **no** notifications; only the 3 s quiet timer and the screen's question patterns apply | **Not chosen yet.** Decide when M4b gets its detailed plan | Accept the weaker detection for testers · or revisit the 2026-10-26 "keep the inherited environment" choice for app launches | M4b, M11 |
| **D7** | Which CPUs the shipped `.app` runs on. The vault names OSes (macOS first) but no CPU; today `cargo build` makes an Apple Silicon-only binary (`aarch64-apple-darwin`) | **Not chosen yet.** Decide when M11 gets its detailed plan | **Apple Silicon only** (what builds today) · or **universal** (Apple Silicon + Intel `x86_64`; whether GPUI works on Intel Macs at our pinned rev is unverified) | M11, M12 |

## How these plans were made (so you can trust the code)

- APIs were read from the **exact pinned GPUI commit** (`1d2a4b3f7f19…`) and from the real
  `alacritty_terminal 0.26.0` / `portable-pty 0.9.0` sources. Not from memory. The condensed
  facts are in [[31 - API Fact Sheet (pinned rev)]].
- **Every code block in M0–M4 was compiled and tested** on this machine on 2026-10-12, in a
  scratch crate outside the repo, stage by stage:

  | Stage | Build | Unit tests |
  |---|---|---|
  | M0 | ✅ 0 warnings | n/a (GUI only) |
  | M1 | ✅ 0 warnings | 21 pass |
  | M2 | ✅ 0 warnings | 38 pass → **47** after the 2026-10-26 plan changes (built and run) |
  | M3 | ✅ 0 warnings | 44 pass → **54** after the 2026-10-26 plan changes (re-verified) → **55** in the repo (Working fix) |
  | M4 | ✅ 0 warnings | 50 pass → 61 expected (edited, not re-compiled) |

  (The only warning is an upstream one inside GPUI's own dependencies, `block` and
  `proc-macro-error2`. It isn't ours, and it can be ignored.)
- **What was *not* verified:** nothing was launched. Whether the window, rendering, keyboard,
  rings and notifications behave right on screen is checked by you (👀) in each plan's manual
  verification task. That's the project rule: *the AI can't see the window.*

## Session ritual

**Start:** this note → current plan's **▶ Pick up here** → [[60 - Progress Log]] (latest entry).
**During:** one task at a time; tick steps; run every command and compare with *Expected*.
**End:** flip task/milestone emojis here and in the plan; update **Where we are**; add a
[[60 - Progress Log]] entry; on milestone completion, also flip [[40 - Roadmap]] and the milestone note.

## Related

[[40 - Roadmap]] · [[11 - Features]] · [[12 - Scope]] · [[20 - Architecture Overview]] ·
[[30 - Tech Stack]] · [[31 - API Fact Sheet (pinned rev)]] · [[54 - Session Startup Checklist]]
