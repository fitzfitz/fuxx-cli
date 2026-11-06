---
tags: [development, input]
---

> [!note] Input for [[40 - Roadmap]] **M11b — Workspaces, pane tabs and the in-pane browser**, from your cmux
> screenshots and answers (2026-11-06). Nothing here is planned yet: M11b gets its own spec and plan after the current
> follow-up ([[M10c-M11 Follow-up Plan - Browser Bar and Tabbed Preferences]]) and your batched on-screen check.

# M11b input: the cmux layout you're aiming for

## Your answers (2026-11-06)

- **Scope: all of it, including the browser inside a pane.**
- **When: a new milestone after the current follow-up** (finish the tab-like rows, your batched checks, merge
  M10b–M11, then plan M11b). The follow-up's Task 3d ("tabs hold terminals") is folded into M11b.
- **Row details to copy:** git branch + folder line; a "Needs input" badge; pinned workspaces; an accent-filled
  selected row.

## What the screenshots show

1. **Titlebar, top-left:** traffic lights, then a sidebar toggle, a 🔔 notifications button and a **+** (new
   workspace). A "?" help button at the sidebar's bottom-left.
2. **Sidebar = workspaces.** Each row: the name in bold; a second line `branch • ~/folder`; when an agent waits, a
   status line ("Claude is waiting for your input") and a 🔔 **Needs input** badge. No status circles, no numbers.
   The selected row is filled with the accent colour (white text). Some workspaces are **pinned** (📌, kept at the
   top). No folder group headers.
3. **The content area's top shows the workspace name** (with an icon).
4. **Three levels:** workspace (sidebar row) → split **panes** → **tabs inside each pane**. Each pane has its own small
   tab bar (icon + title + ×, active tab highlighted) and, on its right, buttons: new terminal tab, new browser tab,
   split right, split down.
5. **The browser is a tab inside a pane:** back / forward / reload, an address field with a suggestions dropdown from
   history (pages and titles) ending in "Search Google for …", and toolbar tools (Import, screenshot, theme, devtools).

## What fuxx has today (after M11 + the follow-up)

- Sidebar rows = sessions (a status mark, name, question line, age; tab-like × / middle-click / right-click menu /
  colour from the follow-up), folder and SSH host group headers, the accent bar on the selected row.
- Splits show several sessions side by side, one terminal per pane, with 22 px pane headers.
- The browser is a separate window per session with a native top bar and address field (no history suggestions).
- The sidebar button beside the traffic lights (follow-up Task 3b).

## The gaps M11b closes

- **Model:** workspaces → panes → pane tabs (terminal or browser); ⌘T semantics, close-last rules, restore of the new
  shape (old files load), scripting ids and `fuxx ls` showing workspace / pane / tab.
- **Sidebar:** rich workspace rows (bold name, branch • folder, status line, Needs-input badge), pinning, accent-filled
  selection; what happens to group headers (cmux has none — to decide in M11b's brainstorm).
- **Titlebar:** 🔔 (a notifications list) and + beside the sidebar button; the workspace name in the content's top.
- **Pane tab bars** with new-terminal / new-browser / split buttons.
- **In-pane browser** as a pane tab, with an address field and history suggestions (needs a visited-pages list, which
  only exists with "Keep history and data" On) and a search fallback (which search engine — to decide).

## Risks to plan for

- **In-pane web view over GPUI** ([[Input - M10 Feasibility Study]] §4.2): native views sit above GPUI's Metal
  content, so fuxx overlays (rename box, ⌘P, menus, the host picker) vanish behind the page unless the web view is
  hidden or moved while they're open; bounds must follow layout changes without jitter; keyboard focus and ⌘C/⌘V/⌘W must
  go to the right side; rounded corners and clipping don't apply to the native view. A spike first.
- **Model change touches everything:** sessions, restore, the background service (M10a), scripting (M9), SSH (M10b),
  agents' browser commands (M10c) — each needs a migration path.
- **Git branch per workspace** means reading the terminal's working folder and running/parsing git safely and cheaply.
