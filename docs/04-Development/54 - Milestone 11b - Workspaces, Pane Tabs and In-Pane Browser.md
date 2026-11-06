---
tags: [development, milestone]
status: built
---

# 54 - Milestone 11b - Workspaces, Pane Tabs and In-Pane Browser

**Status:: 🟨 Built and reviewed 2026-11-06; on-screen check pending (batched with M10b, M10c, M11 and the follow-up).**  ·  **Phase 4: the cmux layout.**
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
- **Drags feel like picking the tab up** (yours, 2026-11-06, Task 7b — being built): the pointer is a closed hand, the
  dragged tab stays where you grabbed it, and instead of an insertion line the other tabs slide aside to open a gap
  where it will land (as in Ghostty). Sidebar rows do the same within their run.
- **The empty part of a tab bar at the window's top drags the window** (yours, 2026-11-06); double-click zooms.
- **⌘T opens a new terminal tab in the focused pane; ⌘N makes a new workspace** (yours, 2026-11-06; replaces "⌘T =
  new workspace" above).
- **Inside a web page, ⌘R reloads, ⌘[ / ⌘] go back / forward, ⌘= / ⌘- / ⌘0 zoom the page** (yours, 2026-11-06);
  elsewhere they keep fuxx's meaning.
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

- (Moved by Task 6b, 2026-11-06: the 🔔 sits in the sidebar footer beside the gear; the footer's "+ New" is an icon-only +; no + in the title area.) Right of the traffic lights and the sidebar button: a 🔔 **notifications** button (a list of recent notifications;
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

## What was built

Built on branch `m11b-workspaces` (from `m11b-browser-bar-prefs`), one agent per task with a separate reviewer, then a
whole-branch review. 715 tests pass. Nothing here is marked done: the look and feel is only proven on your screen.
Plan, verification log and deferred minors: [[M11b Plan - Workspaces, Pane Tabs and In-Pane Browser]].

- **Workspaces.** The sidebar lists workspaces. A workspace is a split layout of panes. Each pane has a tab bar of
  terminal and browser tabs. The model is one pure file (`layout_model.rs`); old `sessions.json` files load into it.
- **Sidebar rows.** Bold name, `branch • ~/folder` underneath, a 🔔 Needs input pill with the question when an agent
  waits, an accent-filled selected row, and pinned workspaces at the top with a 📌. Rows are 42 px (56 px when waiting).
- **Tab bars.** 26 px, one per pane. Terminal and browser tabs, × on hover, four buttons on the right (new terminal tab,
  new browser tab, split right, split down), ⌃Tab / ⌃⇧Tab to cycle, a right-click menu (Close Tab, Close Other Tabs in
  Pane, Rename Tab…, Pop Out for pages).
- **The browser in a pane.** A real web view sits exactly over the tab's area, with a fuxx bar above it: back, forward,
  reload/stop, an address field with history suggestions and a last "Search Google for …" row. The page hides whenever a
  fuxx menu, box or drag is in front of it. Agents' `fuxx browser …` now drive a browser tab in their own workspace.
- **Footer.** The 🔔 notifications list sits in the sidebar footer beside the Preferences gear; the footer's + opens the
  new-workspace menu. The workspace name shows at the top of the content only while the sidebar is hidden.
- **Saved and restored.** Workspaces, panes, tabs and browser addresses come back after a restart, also with the
  background service.

### Your later additions (2026-11-06)

All of these are already listed under "Panes and their tabs" above; here is where they landed.

- **Resize doesn't make a terminal look busy** (Task 4b): output in the 500 ms after a resize doesn't change the status.
- **Empty tab bar drags the window** (Task 4c): only bars touching the top edge; tabs and buttons keep their clicks.
- **⌘T = new terminal tab, ⌘N = new workspace** (Task 4d). The menus say "New Workspace in Folder…", "New SSH
  Workspace…", "New Agent Workspace".
- **Page shortcuts** (Task 4e): ⌘R, ⌘[ / ⌘], ⌘= / ⌘- / ⌘0 act on the page.
- **Drag tabs anywhere** (Task 7): reorder, into another pane, or onto a sidebar workspace row.
- **🔔 and + in the sidebar footer** (Task 6b): the title area keeps only the sidebar button.

### Rulings you should confirm

Every ruling is listed in [[Rulings - M10b M10c M11 (autonomous run)]]. These are the ones that change what you see:

1. **The workspace name shows at the top only while the sidebar is hidden.** With the sidebar shown, the accent-filled
   row already names it, and a band would cost about two rows of every pane. If you want it always, it is about ten lines.
2. **Page shortcuts act whenever a browser tab has fuxx's focus**, including its address field (not only while you are
   typing in the page). On a browser tab you mean the page. In the address field, ⌘R / ⌘[ / ⌘] close the field first. A
   disabled key (for example ⌘[ with nothing to go back to) does nothing; it does not fall back to fuxx's meaning.
3. **The 🔔 is hidden with the sidebar**, like the gear. While the sidebar is hidden a new notification dot isn't visible.
4. **A tab drop at its own place selects the tab** (a shaky click counts as a drag in GPUI). Esc cancels tab drags only.
5. **Agents drive only tabs in a pane**, never popped-out windows. Pop Out moves the live page (no reload).
6. **⌘T always opens a terminal tab**, even from a browser tab, at the end of the bar.
7. **Pages-only workspaces come back after a restart** whatever the "Restore sessions" setting says, when the background
   service is running (as browser tabs beside live terminals already did).
8. **Selected-row marks are drawn in the row's text colour** (the status colours are too faint on the accent fill); the
   shapes still tell the states apart.

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

## On-screen checklist (pending)

Not run yet. Batched with M10b, M10c, M11 and the follow-up (your choice). Items 1–2 apply to everything.

**Workspaces and the sidebar**

1. Run fuxx from the .app opened in Finder (not from a terminal) for all checks below.
2. ⌘N makes a workspace with one pane and one terminal tab; its row appears and is selected. Split it three ways (⌘D,
   ⇧⌘D). ⌘W in a pane closes only that tab; a pane's last tab closes the pane; the workspace's last pane closes the workspace.
3. In a git folder a new row shows the bold name and `branch • ~/folder` within about a second. `cd` to another repo, or
   `git checkout -b x`: the row follows within 5 s. `cd /tmp`: the branch goes. A detached HEAD shows a short sha.
4. An SSH workspace's row shows the host and no branch. After `ssh host` in a local workspace the row keeps showing the
   local folder, during the session and after it.
5. With persistent sessions on, a terminal shows the folder it started in; after a `cd` it follows only if the shell
   sends OSC 7.
6. When an agent waits, its row shows the question and the 🔔 Needs input pill. The row's ring is the most urgent of its
   terminals. ⇧⌘U lands on that pane. ⌘P lists the workspace with the question and Enter focuses that pane.
7. The selected row is filled with the accent; its text, mark and × read well in Mocha, Latte and a couple of other
   themes. Rename it (double-click or ⌘R): the typed text and caret read clearly.
8. Right-click a row → Pin Workspace: it jumps to the top with 📌, out of its group, and ⌘1 selects it. Drag it among
   pinned rows; it can't be dropped among unpinned ones. Unpin puts it at the end of its own group without reordering
   the groups. It stays pinned after a restart.
9. × or middle-click on a 3-terminal row asks "Close workspace with 3 terminals?" (with a busy line if agents work); a
   one-terminal row closes at once. The right-click menu says Close Workspace, Rename Workspace…, Move Workspace Up/Down,
   Workspace Color, and acts on the workspace.
10. Drag a row within its group; ⌘1–⌘9 select rows; ⇧⌘] / ⇧⌘[ cycle workspaces; the window title reads `fuxx — #n name`.
11. A workspace holding only a browser tab still has a row (named from the page title) and is in ⌘P.
12. At the narrowest sidebar (180 px) a waiting row's question is mostly cut ("Do…"); the full text is in ⌘P and the
    notification. Is that fine?
13. Battery: with the window hidden or minimised, or the sidebar hidden (⌘\), no `git` processes start. While visible,
    at most one `git rev-parse` per folder every 5 s. Without the Command Line Tools no install dialog appears.

**Tab bars and dragging**

14. Every pane has a bar, a lone pane too. ⌃Tab / ⌃⇧Tab cycle the focused pane's tabs, also while Claude Code or vim is
    running and while a browser tab is focused.
15. Hover a tab: × appears; click it; middle-click another. Double-click a tab, rename it, press Enter: the tab and
    `fuxx ls` show the new name.
16. Right-click a tab: Close Tab, Close Other Tabs in Pane (always asks), Rename Tab…; Escape and a click away close the
    menu. A browser tab's menu also shows Pop Out.
17. The four buttons: tooltips; new terminal tab; new browser tab; split right; split down.
18. Many tabs in a narrow pane: the row scrolls with the trackpad, and ⌃Tab brings an off-screen tab into view.
19. A tab whose agent works or waits shows its mark animating in the bar, and the waiting tab is tinted.
20. Empty tab bar drags the window: one pane with the sidebar shown, and again with it hidden (under the top band).
    Double-click there zooms or minimises per your macOS setting. Side-by-side panes: both bars drag. Stacked panes: only
    the upper bar drags. Nested layouts: only bars touching the top. Full screen: nothing moves.
21. Clicks stay clicks: a plain click on empty bar space focuses that pane; tabs, × and the four buttons never move the
    window. With tabs filling the row there is no empty part to drag. Press the band, leave the window, release
    elsewhere, hover back: the window must not move.
22. Drag a tab a little within its bar: the line follows between tabs (and reads as a gap marker next to the active
    tab), the chip hangs below the pointer, the tab dims. Drop: it lands there, is shown, and typing goes to it. Drag in
    a scrolled bar: the line still lands between the right tabs.
23. Click tabs quickly and a little shakily: every click shows its tab, even a tiny drag that ends on it.
24. Drag a tab into another pane's bar, including the empty end part, in a window whose top bars drag the window: the
    window must not move.
25. Drag a tab onto another workspace's row: the row lights up. After the drop that workspace shows the tab selected at
    the end of its focused pane. The terminal keeps running (an agent mid-output), keeps its scrollback and its mark,
    and `fuxx ls --json` shows its new workspace and pane. Dropping on its own workspace's row moves it to the end of
    the focused pane.
26. Drag a workspace's only tab onto another row: the emptied workspace closes with no question.
27. Esc mid-drag: nothing moves. Release over a terminal: nothing moves. (With a page holding the keyboard, Esc may not
    cancel; a release elsewhere still does.) Drag a file in from Finder after a cancelled tab drag: no tab stays dimmed.
28. With a page showing, drag any tab over the page: the page hides during the drag and comes back after. Drag the
    browser tab itself into another pane: the page keeps its state (no reload).
29. Quit and relaunch after moving tabs: the layout comes back as you left it.

**The browser in a pane (overlays hiding the page, keys, Pop Out)**

30. Open a browser tab on a real page (`localhost:3000` or https://example.com). Open each of these over it; the page
    must vanish while each is open and come back when it closes: ⌘P; the rename box (double-click a row, and Rename Tab…
    on a terminal tab in the same pane); a row's and a tab's right-click menu; the footer's + menu; the 🔔 popover;
    ⇧⌘S (host picker); ⇧⌘B (address box); the address field's suggestions.
31. ⌘\ (sidebar slide), dragging a divider, dragging the sidebar's edge, dragging a row: the page hides, then comes back
    exactly in place. Preferences (⌘,) is its own window and simply sits above.
32. Switch workspaces (⌘1/⌘2) and tabs (⌃Tab): the page never shows over another workspace or tab.
33. Click in the page's text field and type: letters go to the page; ⌘C, ⌘V, ⌘A, ⌘Z act in the page. With the page
    typing: ⌘W closes the browser tab, not a terminal; ⌘T, ⌘N, ⌘D, ⌘1–9, ⌘P, ⇧⌘U and ⌃Tab all act in fuxx; ⌘L opens the
    address field.
34. Click a terminal pane and type: letters go to the terminal. Switch back to the browser tab and type: nothing reaches
    the page until you click in it. Type an address and press Enter: the page loads and takes the keyboard at once.
35. Type words (`rust closures`): a Google search opens. Type `javascript:alert(1)`: a red reason shows in the field.
36. Over the page the cursor is an I-beam on text and a hand on links, with no flicker to an arrow. With the page
    holding the keyboard, the sidebar, tab × buttons and divider cursors still work and nothing hovers twice or jitters.
37. The page is sharp on a Retina display and on an external display. Divider-drag and window-resize: the page doesn't
    lag the pane visibly and reappears exactly placed.
38. Turn "Keep history and data" On, visit pages, then type their host: suggestions show. Turn it Off: no history rows,
    and `~/Library/Application Support/fuxx/browser-history.json` is deleted once no tab or window made with it On is
    left open.
39. Pop Out (right-click the browser tab): the same page, scroll position and login appear in a browser window and the
    tab closes. A zoomed page keeps its zoom. Pop out the only tab of the only workspace, then close that window: fuxx
    quits within about 0.5 s instead of staying with an empty window.
40. Close a workspace that has terminals and browser tabs: all go. Close the last terminal while a browser tab remains:
    fuxx stays open.

**Title area and the 🔔 footer**

41. The footer shows the 🔔 beside the Preferences gear and an icon-only +. Tooltips read "Notifications" and "New…".
    The + menu says New Workspace ⌘N, New Workspace in Folder… ⌘O and the rest; its New Workspace makes a workspace.
42. Let an agent in a background workspace finish or ask something: a dot appears on the 🔔 (also when macOS showed no
    banner). Open the popover: the entry is first and the dot goes.
43. Click an entry: that terminal's workspace, pane and tab show and typing goes there. Close a terminal and reopen the
    popover: its entries are greyed, say "closed", and do nothing when clicked.
44. In a fuxx terminal run `fuxx notify --title Build "done"`: it is listed and clicking it shows that terminal. From a
    Terminal.app shell it is listed and not clickable.
45. The popover opens upward. At the default sidebar width it sits over the sidebar under the bell and gear, its right
    edge at the gear's; at the narrowest width it overhangs like the + menu. The bell is lit and filled while it shows,
    and its outline brightens on hover. Escape or a click outside closes it and typing returns to the terminal.
46. With the 🔔 popover open press ⇧⌘U (and separately ⌘1, ⌘T): it closes, Escape afterwards does nothing to the
    terminal's agent, and a page in view reappears at once. Same with the + menu open. With the + menu open press ⌘P:
    the switcher opens and its first click works.
47. The band keeps the sidebar button and nothing else: drag the window by the band where the 🔔 and + used to be, in
    full screen too.
48. Hide the sidebar (⌘\): the 🔔 hides with it; an open popover closes and typing returns to the terminal. The
    workspace name shows centred in the band above the panes; narrow the window and it cuts with "…" and vanishes when
    there is no room. In full screen the buttons move to the left edge and the name stays centred. With the sidebar
    shown there is no name over the content. Light and dark themes: the name, dot and popover text read clearly.

**Resize**

49. Idle shell: drag the window's edge. The mark stays Idle (no spin, no Done, no notification).
50. An agent waiting on a question: resize. It stays Waiting and gives no repeat notification.
51. An agent working: resize. It stays Working and settles to Done or Waiting about 3 s after its real output stops.
    Typing after a resize still shows Working. (A slow SSH or heavy-TUI redraw can outlast the 500 ms window.)

**Keys: ⌘T / ⌘N and the page shortcuts**

52. ⌘T in a terminal pane, in a browser tab, and in the second of two panes: a new terminal tab at the end of that
    pane's bar, selected, and typing works. ⌘N makes a new workspace. With every workspace closed (if you can reach
    that), ⌘T makes a workspace. With a page focused (click in it), ⌘T and ⌘N still reach fuxx.
53. File menu: New Workspace ⌘N, New Tab ⌘T, New Workspace in Folder…, New SSH Workspace…, New Agent Workspace ▸.
54. In a page, ⌘R reloads it and the workspace is not renamed; after following a link ⌘[ / ⌘] go back / forward and the
    pane focus doesn't move.
55. In a page, ⌘= ⌘= ⌘-: the page zooms 110 % → 125 % → 110 %; ⌘0 returns to 100 %. The terminal font size doesn't change.
56. Click a terminal and press ⌘R / ⌘[ / ⌘=: rename, previous pane and bigger font, as before.
57. Address field open: ⌘R closes it and reloads; ⌘[ goes back; ⌘= zooms the page and leaves the field open. Select a
    browser tab with ⌃Tab without clicking its page, then ⌘R: the page reloads. (Known: the File and View menus still
    show ⌘R / ⌘= as rename / bigger while a browser tab is focused.)

**Restore**

58. Quit and reopen (persistence off, then on): workspaces, panes, ratios, tabs, focus and each browser tab's last
    address come back. A focused browser tab is focused again.
59. Close every terminal leaving a browser tab, ⌘Q, relaunch: the browser workspace comes back at its address.
60. With the background service running, its terminals land back in their saved tabs. SSH: drop the connection and
    reconnect (Enter): the terminal comes back in the same pane.
61. If you have an old (v1) `sessions.json`, it opens as one workspace per old layout.

**Agents' browser commands**

62. From an agent's terminal run `fuxx browser open localhost:3000`: a browser tab appears in a new pane right of its
    terminal, and your focus and typing stay where they were. A second `open` reuses that tab. `fuxx browser url`,
    `snapshot` and `close` act on it. With no tab they say `no browser for sN (fuxx browser open URL first)`. (A tab not
    on screen is throttled; a screenshot of it may come back blank.)
63. With a browser tab focused and typing in the page, run `fuxx new --background --run "echo hi"` from a terminal: the
    page keeps the keyboard.
64. `fuxx ls --json` shows `workspace`, `pane` and `kind`. `fuxx new --tab --of sN` adds a terminal tab to sN's pane;
    `fuxx new --split right --of sN` makes a pane beside it.

## Known limitations

- Page tooltips from fuxx's bar are drawn under the page (GPUI tooltips can't go over a native view).
- A browser tab that isn't on screen is throttled by WebKit; an agent's screenshot of it can be blank or stale.
- The unfocused-pane veil doesn't dim a page.
- Every restored browser tab loads at launch (not only when first shown).
- The fixed 500 ms resize window can be outlasted by a slow SSH or heavy-TUI redraw.
- The File and View menus still show ⌘R / ⌘= as rename / bigger while a browser tab is focused.
- The full lists of deferred minors are in the plan's "Deferred minors".

## Related

- [[40 - Roadmap]] · [[M11b Input - Workspaces, Pane Tabs and In-Pane Browser]] · [[48 - Milestone 8 - Split Panes]] ·
  [[52 - Milestone 10c - Agent Browser]] · [[53 - Milestone 11 - UI-UX Uplift]] · [[Input - M10 Feasibility Study]] §4
