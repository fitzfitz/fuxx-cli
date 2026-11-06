---
tags: [development, milestone]
status: planned
---

# 54 - Milestone 11b - Workspaces, Pane Tabs and In-Pane Browser

**Status:: ⬜ Planned (spec written 2026-11-06).**  ·  **Phase 4: the cmux layout.**
**Plan:** [[M11b Plan - Workspaces, Pane Tabs and In-Pane Browser]] · **Input:** [[M11b Input - Workspaces, Pane Tabs and In-Pane Browser]]

## Goal

fuxx organised the way cmux is: the sidebar lists **workspaces**; a workspace is a split layout of **panes**; each pane
has its own **tab bar** of terminal and **browser** tabs. The browser lives inside a pane. Sidebar rows say at a glance
where each workspace is (git branch and folder) and which one needs you.

## Your choices (2026-11-06)

- All of the cmux layout, including the browser inside a pane, as its own milestone after the follow-up.
- Rows: git branch + folder line, a "Needs input" badge, pinned workspaces, an accent-filled selected row.
- From the earlier tab discussion: group headers stay with workspaces inside them; the sidebar lists workspaces only;
  closing a workspace with 2+ terminals asks first.
- "Just continue" — build it now; your on-screen checks come once it's done. **Everything else is the AI's call**, marked.

## How it behaves

### Workspaces (the sidebar)

- ⌘T (and the sidebar footer's "+ New", the title-area **+**, presets, New SSH Session, `fuxx new`) makes a **new
  workspace** with one pane holding one terminal tab.
- A row shows: the name in **bold**; a second line `branch • ~/folder` (the branch only inside a git repo); while one of
  its terminals waits, a status line with that terminal's question and a 🔔 **Needs input** badge. The selected row is
  filled with the accent colour (text on it ≥ 4.5:1). The status mark stays at the row's left (AI's call: the rings are
  fuxx's core signal); ×, middle-click, the right-click menu, tab colour and ⌘N carry over from the follow-up, now acting
  on workspaces.
- **Pinned workspaces** (right-click → Pin / Unpin) sit at the top of the sidebar above the groups, with a 📌.
- Group headers (folder / SSH host) stay; a workspace belongs to its first terminal's folder or host.
- A workspace's ring = its most urgent terminal (Waiting > Done > Working > Idle).
- Closing a workspace (×, middle-click, Close) ends all its tabs; it asks first when it has 2+ terminals.

### Panes and their tabs

- A workspace is a split layout (M8) of panes. Each pane has a **tab bar** (replacing M11's pane headers): each tab has
  an icon (terminal / globe), its title, its status mark for terminals, and × on hover; the active tab is highlighted.
  On the bar's right: **new terminal tab**, **new browser tab**, **split right**, **split down**.
- ⌘D / ⇧⌘D split the focused pane (the new pane gets one terminal tab). ⌘W closes the focused tab; closing a pane's last
  tab closes the pane; closing a workspace's last pane closes the workspace.
- Switching tabs: click; ⌃Tab / ⌃⇧Tab within the focused pane (AI's call).
- **Drag tabs anywhere** (yours, 2026-11-06): reorder within a pane, drop into another pane's tab bar, or drop onto a
  sidebar workspace to move the tab there. A pane or workspace left empty closes.
- **The empty part of a tab bar at the window's top drags the window** (yours, 2026-11-06); double-click zooms.
- **Resizing doesn't make a terminal look busy** (your bug report, 2026-11-06): a shell redrawing its prompt after a
  resize no longer counts as work.

### The browser in a pane

- A browser tab shows a GPUI-drawn bar (back, forward, reload/stop, address field) above the page. The address field
  suggests from history (when "Keep history and data" is On) and ends with **"Search Google for …"** (AI's call, as in
  your screenshot); Enter opens the address or searches.
- The page is a native web view placed exactly over the tab's content area. While a fuxx overlay is open (rename box,
  ⌘P, menus, the host picker, the address suggestions…) or the tab isn't visible, the native view is hidden or clipped
  so nothing disappears behind it.
- Agents: `fuxx browser …` / MCP now drive a **browser tab in the agent's workspace** — `open` reuses the workspace's
  browser tab or creates one by splitting the agent's pane to the right (AI's call). The separate browser window from
  M10c stays as "Pop Out" in a browser tab's right-click menu (AI's call: it reuses that work).
- Keep-data / address-bar settings and the http/https rule apply as in the follow-up.

### Title area

- Right of the traffic lights and the sidebar button: a 🔔 **notifications** button (a list of recent notifications;
  click one to jump to that terminal; a dot when there are unseen ones) and a **+** (new workspace).
- The content area's top shows the selected workspace's name.

### Scripting

- Terminal ids `sN` stay. `fuxx ls` / `session.list` add `workspace` and `pane` (1-based positions) and `tab` kind.
  `fuxx new` = new workspace; `fuxx new --split right|down --of sN` = new pane beside that terminal; `fuxx new --tab --of
  sN` = new terminal tab in that terminal's pane (AI's call).

### Saved and restored

- Workspaces (name, colour, pinned, layout), panes and their tabs (terminal / browser URL) are saved and restored; old
  `sessions.json` files load (each old layout becomes a workspace whose panes hold one terminal tab each). Background-
  service (M10a) terminals land back in their tabs. Browser tabs reopen their last URL (AI's call).

## Not in M11b

- Moving a tab to a new window (multi-window), the browser's Import / screenshot / devtools toolbar extras, a
  notifications history beyond this run.

## Success check

1. ⌘T: a new workspace with one terminal; split right; in the right pane add a browser tab and open localhost:3000.
2. The sidebar row shows the bold name, `branch • folder`, and when an agent waits, its question and "Needs input".
3. Pin a workspace: it moves to the top with 📌.
4. Open ⌘P / the rename box over the browser tab: nothing hides behind the page.
5. Close a pane's last tab → the pane goes; close the workspace's last pane → the workspace goes.
6. Quit and reopen: workspaces, panes, tabs (incl. the browser URL) come back.
7. An agent's `fuxx browser open localhost:3000` opens a browser tab beside its terminal.

## Related

- [[40 - Roadmap]] · [[M11b Input - Workspaces, Pane Tabs and In-Pane Browser]] · [[48 - Milestone 8 - Split Panes]] ·
  [[52 - Milestone 10c - Agent Browser]] · [[53 - Milestone 11 - UI-UX Uplift]] · [[Input - M10 Feasibility Study]] §4
