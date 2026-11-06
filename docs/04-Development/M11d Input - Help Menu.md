---
tags: [development, input]
---

> [!note] Input for [[40 - Roadmap]] **M11d — Help menu and shortcut list**, from your request (2026-11-06). Not
> designed yet: M11d gets its own spec and plan after [[55 - Milestone 11c - Native Notifications]].

# M11d input: a Help menu

## Your request (2026-11-06)

- "Add a help menu" in the menu bar, as Ghostty has (after Window). One item is **Shortcuts**: a list of all fuxx's
  shortcuts **with a search**.
- Asked what else belongs in Help, you chose all four suggestions:
  1. **fuxx command reference** — a searchable panel for the `fuxx` commands agents and scripts use (`fuxx ls`,
     `notify`, `new`, `browser …`) and the MCP setup, built from the same text `fuxx help` prints.
  2. **Notifications status** — whether macOS lets fuxx send its own banners (M11c), with a button to System
     Settings → Notifications → fuxx when they're off.
  3. **Show the welcome tips again** — brings back the sidebar's first-run hint card.
  4. **Report an issue / logs** — "Report an Issue…" opens a new GitHub issue for fuxx; "Show Logs" opens the folder
     with fuxx's and the background service's logs.

## What was suggested with it (AI's, to settle in M11d's design)

- macOS adds its own search box to any menu named "Help" (it finds menu commands) — free with the menu.
- The shortcut list as a panel over the window like ⌘P (not a separate window), searchable by name or key, grouped
  (workspaces, tabs, panes, terminal, browser pages, app), opened from Help or a shortcut such as ⌘/.
- Built from the real key-binding table, so it can't drift; it includes the page-only keys (⌘R, ⌘[ …).

## To find out in the design

- Where fuxx's logs live today (the background service logs; the window prints to stderr) — "Show Logs" may need a
  log file first.
- The GitHub repository fuxx's issues should go to, and whether it's public.
