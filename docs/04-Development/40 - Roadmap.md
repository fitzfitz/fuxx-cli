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
  **Merged into `master` and pushed on 2026-11-06, at your request** (M10b, M10c, M11, the follow-up, M11b); their on-screen checks are still to come.
- **M10c/M11 follow-up — browser bar, keep-data setting, tabbed Preferences, sidebar button, tab-like rows** — 🟨 built and reviewed 2026-11-06, on-screen check pending (batched with M10b, M10c, M11): [[M10c-M11 Follow-up Plan - Browser Bar and Tabbed Preferences]] (amendments in [[52 - Milestone 10c - Agent Browser]] and [[53 - Milestone 11 - UI-UX Uplift]]; your requests, 2026-11-06). Next: your batched check → merge M10b / M10c / M11 / follow-up / M11b → M12.
- **M11b — Workspaces, pane tabs and the in-pane browser** — 🟨 built and reviewed 2026-11-06, on-screen check pending (batched with M10b, M10c, M11 and the follow-up): [[54 - Milestone 11b - Workspaces, Pane Tabs and In-Pane Browser]], plan [[M11b Plan - Workspaces, Pane Tabs and In-Pane Browser]] *(added 2026-11-06 on request)*. The cmux layout: workspaces in the sidebar (rich rows: branch • folder, Needs-input badge, pinning, accent-filled selection), split panes each with their own tab bar, terminal or browser tabs (the browser inside a pane with history suggestions), the 🔔 list and + in the sidebar footer, drag tabs anywhere, ⌘T new tab / ⌘N new workspace, page shortcuts. Your numbered on-screen checklist is in note 54. Input: [[M11b Input - Workspaces, Pane Tabs and In-Pane Browser]]. Next: your batched check → merge → M12.
- **M11c — Native notifications** — 🟨 built and reviewed 2026-11-06, on-screen check pending: [[55 - Milestone 11c - Native Notifications]], plan [[M11c Plan - Native Notifications]]. Banners come from fuxx (titled fuxx, its own entry in System Settings) and **clicking one shows the terminal that raised it** — moved here from M12 after a test on your Mac showed the ad-hoc signed app can do both (no paid Apple account needed). Falls back to today's `osascript` banners when macOS says no.
- **M11d — Help menu and shortcut list** — 🟨 built and reviewed 2026-11-08 (merged into `master` locally 2026-11-08 at your request, not pushed), on-screen check pending; planned 2026-11-07 (your "plan it" / "go"): spec [[56 - Milestone 11d - Help Menu]], plan [[M11d Plan - Help Menu]], input [[M11d Input - Help Menu]]. A Help menu (after Window, with macOS's search): **Keyboard Shortcuts** (⌘/, a searchable panel built from the real key bindings), a **fuxx Command Reference** (built from `fuxx help`'s text, plus the MCP setup line), **Notifications Status…**, **Show Welcome Tips**, **Report an Issue… / Show Logs** (the window gets its own `fuxx.log`). Next: your on-screen check (17 items in note 56).
- **M11e — Update Available** — 🟨 built, merged, pushed and released as **v0.3.0** (2026-11-08); the on-screen check of an update (v0.3.1) is pending; planned 2026-11-08 (added by you): spec [[57 - Milestone 11e - Update Available]], plan [[M11e Plan - Update Available]]. GitHub Releases built by GitHub Actions on a `v*` tag (first: v0.3.0); fuxx checks GitHub daily and shows a blue **Update Available** button above the sidebar footer; one click downloads, checks, swaps and reopens fuxx, with no "Open Anyway" and no Developer ID. First step: a spike on your Mac (Task 0).
- **M11f — Cursor style** — 🟨 built, merged, pushed and released as **v0.3.2** (2026-11-08, run 42439235155 green); on-screen check pending; planned 2026-11-08 (added by you): spec [[58 - Milestone 11f - Cursor Style]], plan [[M11f Plan - Cursor Style]]. A bar cursor by default that glides, breathes instead of blinking, and takes the colour of the agent's status; Trail and Sparks as optional Fun; Preferences → Appearance → Cursor. The same plan adds the package icon to M11e's Update Available button. Ships as v0.3.2.
- **M11g — Right sidebar, Files, file tabs** — 🟨 built, merged, pushed and released as **v0.4.0** (2026-11-08, run 42469390248 green); on-screen check pending (14 items in note 59); designed 2026-11-08 (added by you): [[59 - Milestones 11g-11h - Right Sidebar and File Tabs]]. One right sidebar per window (⌥⌘\\, View menu, an icon on every pane's tab bar), following the focused terminal; Files (repo tree, git markers, filter); read-only file tabs (syntect + two-face colours from the fuxx theme, line numbers, change bars vs HEAD, live reload, ⌘F, copy, Open in Editor — opens as text, never runs).
- **M11i — UI uplift 2** — 🟨 built, reviewed, merged and pushed 2026-11-08 (released in v0.5.0 (2026-11-10)); on-screen check pending (22 items in note 60); designed and planned 2026-11-08 (every detail AI's call; you skipped the design questions): [[60 - Milestone 11i - UI Uplift 2]], plan [[M11i Plan - UI Uplift 2]] (requested 2026-11-08, your "I love all 1-11"; **before M11h**, so Changes and Activity are built into the new look). Smart names (reverted by you 2026-11-09: names are yours, else the program's title, else ╭∩╮, as before); compact 22/32 pt rows with a tinted selection and a 2 pt accent bar; Files icons, indent guides, a root header and collapse all; panes and the right sidebar as rounded cards with 6 pt gaps and a focus ring; 16/12 pt terminal padding; a status bar (status, branch, folder, listening ports, the update button moved there); a tab bar with +, ⋯ and the right-sidebar toggle. Released in v0.5.0 (2026-11-10).
- **M11h — Changes, diff view, Activity** — 🟨 built, reviewed, merged and pushed 2026-11-09 (released in v0.5.0 (2026-11-10)); on-screen check pending (14 items in note 59); plan [[M11h Plan - Changes, Diff View, Activity]]. The right sidebar's Files · Changes · Activity tabs; Changes (files changed since the last commit, M/A/D, +/− counts); a diff mode for file tabs (old and new lines in one column, File | Diff toggle, change-bar clicks); Activity (each terminal's timeline, its waiting question, notes and progress; in memory, last 200).
- **M11j — Command palette (⌘K)** — 🟨 built, reviewed, merged and pushed 2026-11-09 (released in v0.5.0 (2026-11-10)); on-screen check pending; requested 2026-11-08; designed and planned 2026-11-09 (every detail AI's call): [[61 - Milestone 11j - Command Palette]], plan [[M11j Plan - Command Palette]]. One search for actions, workspaces, files and settings; replaces the ⌘P switcher (⌘P now opens the palette on its Workspaces scope). After M11h.
- **M11k — First-run screen** — 🟨 built, reviewed, merged and pushed 2026-11-09 (released in v0.5.0 (2026-11-10)); on-screen check pending (18 items in note 62); [[62 - Milestone 11k - First-Run Screen]], plan [[M11k Plan - First-Run Screen]]. A Welcome card on the very first launch: pick a folder, start an agent (presets checked on your PATH), three shortcuts (⌘K, ⌘N, ⇧⌘U); Help → Welcome to fuxx brings it back; "Show Welcome Tips" renamed "Show Tips Card".
- **M11l — Command blocks** — 🟨 built, reviewed, merged and pushed 2026-11-09 (1251 tests; released in v0.5.0 (2026-11-10)); on-screen check pending (22 items in note 63): [[63 - Milestone 11l - Command Blocks]], plan [[M11l Plan - Command Blocks]]. Requested 2026-11-08: Warp-style blocks per command and its output (collapse, copy, rerun). OSC 133 marks from fuxx's own zsh integration (injected through `ZDOTDIR`, your dotfiles untouched; works with powerlevel10k; off with `command-blocks = off`), a bar in the left padding per command (red when it failed), a hairline between commands, a one-row hover chip (it leaves a right prompt visible) with Copy command / Copy output / Rerun (types the command, never presses Return), ⇧⌘↑ / ⇧⌘↓ to jump, ⇧⌘C to copy output. Also: a darker tab bar (your request, 2026-11-09). **Collapse is not in M11l** (AI's call: alacritty can't fold rows; see note 63). bash and fish aren't injected; any shell that sends OSC 133 itself gets blocks.
- **M11m — Title bar** — 🟨 built, reviewed, merged and pushed 2026-11-09 (released in v0.5.0 (2026-11-10); on-screen check pending, 20 items in note 64): [[64 - Milestone 11m - Title Bar]], plan [[M11m Plan - Title Bar]]. Requested 2026-11-09 by you. With the sidebar open: the sidebar icon moves to the right edge of the sidebar's top band and the app title **"the Fuxx?"** is centred there; 🔔 and ⚙ stay in the footer. With the sidebar collapsed: the title is hidden, the workspace name is centred in the window's top band, and **[🔔] [⚙] [sidebar]** sit at the far right.
- **M11n — Performance** — 🟨 built, reviewed, merged and pushed 2026-11-10 (1289 tests; every target met); released in v0.5.0 (2026-11-10); on-screen check pending (11 items in note 65): [[65 - Milestone 11n - Performance]], plan [[M11n Plan - Performance]]. Requested 2026-11-09 by you, after M11m: fuxx idled at about 6 % CPU (the cursor's breathing glow redraws the whole window 20×/s) against Ghostty's 0.2 %, and drained plain output about 2.7× slower. Measured in one session against the code before M11n: idle with the glow breathing 6.99 → 5.35 % (0.765×; target ≤ 0.8×), **0.93 % after five minutes untouched** (the breath rests), glow off unchanged (0.74 %), `cat` of 100 MiB 0.931 → 0.481 s (**1.42×** Ghostty, was 2.7–2.9×), 200k coloured lines **1.01×** Ghostty (about 1.07× allowing for unparsed output at return), launch and memory unchanged. What changed: the byte scanners jump over plain text, a parser thread takes the PTY's reads in 64 KiB batches with one UI wake per batch, `Theme::ui()` once per theme, and the breath drawn at level crossings (22 frames per breath instead of 32). Found for the chores round: the breath still costs about 5.3 % because each frame re-renders the window chrome; caching the chrome could save more.
- **The one combined release (M11i–M11n)** is **v0.5.0, released 2026-11-10 (run 42662570303)**: tag `v0.5.0` on `7f85fad`, asset `fuxx-0.5.0.zip`, https://github.com/fitzfitz/fuxx-cli/releases/tag/v0.5.0 (your rule, 2026-11-09: "push the release after performance milestone"). ⬜ Next: your combined on-screen check (notes 59–65), then a chores round planned **with** you ("after release, let's plan another chore tasks"); not yet planned.
  - Chores candidates (noted, not yet planned or in Scope):
    - ✅ **Branch and folder on two lines** in the sidebar row — done 2026-11-10 on your word ("bisa ga kalau branch dan folder jadi 2 baris? hilangkan baris branch jika bukan git"; "A langsung kerjakan di master dan langsung push"). Note 60, "After release". Not yet released.
    - **Branch switcher → M11o** (below): designed with you and approved 2026-11-10 ("oke setuju, lanjut tulis plan-nya").
    - ✅ **No grey line above prompts** — done 2026-11-10 on your word ("ada garis pemisah abu, jelek" → "ya hapus"): the M11l hairline above each prompt is removed; the gutter bars stay. Note 63, "After release". Not yet released.
    - **Shell choice in Preferences** — requested by you 2026-11-10 ("catat pilihan shell di preferences untuk chores"). Today fuxx always starts your account's login shell (`/bin/sh` if that is missing; `src/pty.rs` `command`), with no setting to pick another; the workarounds are `chsh` or an `agent-preset = bash: exec bash -l` line.
    - **Say which command finished** — requested by you 2026-11-10 ("npm/cargo/pnpm/dan lain-lain untuk jalankan proses dev, test, build. Saya mau titlenya berubah juga… jika saya selesai jalankan dan dapat notifikasi, saya tidak tahu itu apa"). Today a finished command's notification reads only `╭∩╮` · "Worked for 3s", though the zsh integration (M11l) already sends the command line (`133;C;cmdline_url`) and the exit code (`133;D`). AI's proposal, not yet agreed: (1) while a command runs, its name shows as a temporary name — yours → program's title → **the running command** → preset → ╭∩╮ — and goes back at the prompt; (2) the notification and Activity name the command and its result (`pnpm build` · Done in 3m 12s; `cargo test` · Failed (exit 101) after 40s, marked as a failure); (3) notifications show only the program and its subcommand, never the other arguments or leading `VAR=…`, since command lines can hold tokens and Notification Center keeps them. zsh only until the integration covers bash and fish.
- **M11o — Branch Switcher** — 🟨 built 2026-11-10 on branch `feat/m11o-branch-switcher` (6/6 tasks, 1319 tests; not merged yet); waiting for your on-screen check (12 items in note 66). Designed and approved 2026-11-10 (your call: a modal; conflicts disabled with "conflict x file(s)"; a confirmation every time; ahead/behind; a background fetch on open and a Fetch button; a typed name from origin, "not found" otherwise). Planned 2026-11-10 (6 tasks): [[66 - Milestone 11o - Branch Switcher]] ("M11o as built" and the checklist), plan [[M11o Plan - Branch Switcher]].
- **M12 — Ship as a real `.app`:** (after M11d; not planned yet) take M4b's bundle and sign/notarize it with a Developer ID (no more "Open Anyway"). *The notification identity and click-to-session moved to M11c on 2026-11-06.* Universal (Apple Silicon + Intel) per **D7**, decided 2026-10-26.
- **M13 — Homebrew cask** for install. (Later: Windows support.)

---

## The rule of one milestone at a time

Finish and verify each milestone (its own success check passes on the real machine) before starting
the next. Don't pull Phase 2+ features into the core — guard with [[12 - Scope]].

## Related

- [[11 - Features]] · [[20 - Architecture Overview]] · [[50 - Agentic Workflow]]
