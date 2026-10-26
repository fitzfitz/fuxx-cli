---
tags: [moc, product, features]
---

# 11 - Features

The full feature catalog — the north star. **Phased**, not a build-now list; the current work is
whatever the [[40 - Roadmap]] points at. Priorities: `P0` core / `P1` daily-driver / `P2` power /
`P3` later. Grounded in cmux's real feature set + our multi-agent vision ([[10 - What Is fuxx]]).

## A. Terminal core (it must be a real terminal)

- `P0` Render a full VT screen via `alacritty_terminal` — text, truecolor, styles, cursor.
- `P0` Run any program; keyboard input; window + PTY resize (SIGWINCH).
- `P0` Scrollback + scroll.
- `P1` Selection, copy/paste; mouse support.
- `P1` Clickable URL / path detection (open on click).
- `P1` Font config (family/size/ligatures) via `cosmic-text`, including prompt icons (Nerd Font symbols such as powerlevel10k's, `?` boxes today). *(M6; noted 2026-10-26.)*
- `P2` Search in scrollback; theming via **fuxx's own settings file** and built-in themes. *(M6. Changed 2026-10-27:
  originally "read Ghostty's config" like cmux; you chose fuxx's own file so it doesn't depend on Ghostty.)*

## B. Multi-agent orchestration (the differentiator)

- `P0` Multiple concurrent sessions, each a shell/agent on its own PTY.
- `P0` **Session sidebar / vertical tab list**; quick-switch; bring one to front.
- `P1` Per-session name/label; rename; reorder.
- `P1` **Project groups** — sessions grouped under their project (a named folder) in the sidebar.
  You assign the project when creating a session (fuxx doesn't follow a shell's `cd`). Each group
  header shows its most urgent ring (waiting > done > working > idle); collapse/expand; "new session
  in this project". Not included: per-project settings, nested groups, auto-detecting projects.
  Added 2026-10-12 (user request).
- `P1` Agent presets — launch `claude` / `opencode` / `codex` with one action, per working dir.
- `P2` Session restore across app restarts (layout + working dirs).

## C. Notification rings & agent state (the payoff)

- `P0` Per-session **state detection**: working / done / **waiting-for-input** (reuse v1's OSC-9
  detection, extended to OSC 99 / OSC 777 / BEL on 2026-10-26, + heuristics on the output stream).
- `P0` Visual **ring / status badge** per session in the sidebar; at-a-glance across all sessions.
- `P1` Desktop notifications for **background** sessions (focus-aware — don't nag about the active one).
- `P1` Jump-to-session from a notification. *(Moved to M11 by D5, 2026-10-26: needs the real `.app`.)*
- `P2` Notification history / activity feed.

## D. Layout

- `P1` Tabs (or the sidebar list is the primary switcher).
- `P2` **Split panes** (cmux uses a custom split package) — multiple sessions visible at once.

## E. Programmability & integration (cmux-like)

- `P2` A **socket API** per session (cmux exposes `CMUX_SOCKET_PATH` per pane) for scripting.
- `P3` Scriptable automation of sessions/layout.

## F. cmux-parity extras (explicitly later)

- `P3` In-app scriptable browser.
- `P3` SSH workspaces.
- `P3` Remote / tmux-compatible daemon.

## G. Distribution & platform

- `P1` Cross-platform-capable (macOS first; Linux via the Rust stack).
- `P2` **Ship as a real `.app`** (bundle + signing/notarization) → fixes the notification identity
  (v1's `osascript` banner shows as "Script Editor" until then — see memory `macos-notify-rust-broken`).
- `P2` Homebrew cask for install.
- `P3` Windows support.

## Related

- [[40 - Roadmap]] (how these get built, in order) · [[12 - Scope]] (what's deliberately out) ·
  [[20 - Architecture Overview]]
