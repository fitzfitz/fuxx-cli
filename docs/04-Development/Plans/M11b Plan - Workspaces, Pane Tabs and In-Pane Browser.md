---
tags: [development, plan, milestone]
milestone: M11b
status: planned
updated: 2026-11-06
---

# M11b Plan — Workspaces, pane tabs and the in-pane browser

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

| At a glance | |
|---|---|
| **Milestone note** | [[54 - Milestone 11b - Workspaces, Pane Tabs and In-Pane Browser]] · input: [[M11b Input - Workspaces, Pane Tabs and In-Pane Browser]] |
| **Status** | ⬜ Planned (2026-11-06) |
| **Tasks done** | 0 / 10 |
| **▶ Pick up here** | Task 0 |
| **Decisions used** | Yours (2026-11-06): the full cmux layout incl. the in-pane browser; row details (branch • folder, Needs-input badge, pinning, accent-filled selection); group headers stay; workspaces only in the sidebar; closing a 2+-terminal workspace asks; "just continue". Everything else: AI's call (spec). |
| **Needs your eyes** | 👀 Task 8 (batched with M10b, M10c, M11 and the follow-up) |

**Goal:** Workspaces → panes → pane tabs (terminal | browser), with the browser drawn inside a pane, cmux-style sidebar
rows, and title-area 🔔 / +.

**Architecture:** A pure layout model (`layout_model.rs`) owns workspaces, panes and tabs and every rule (new, split,
close-last, roll-up, ⌘N, migration); the GPUI `Workspace` renders it and keeps terminal views (`TerminalView`, unchanged)
and browser tabs (a native web view positioned over a GPUI placeholder, `pane_browser.rs`) keyed by tab id. Existing
features (M7 restore, M9 scripting, M10a service, M10b SSH, M10c agents, M11 look, the follow-up's rows) are rewired onto
the model, not rewritten. A spike proves the in-pane web view first.

**Tech Stack:** No new crates (objc2 / objc2-app-kit / objc2-web-kit already present; GPUI at the pinned rev).

## Global Constraints

- Model: a workspace has ≥ 1 pane; a pane has ≥ 1 tab; closing a pane's last tab closes the pane; closing a workspace's
  last pane closes the workspace; terminal ids `sN` never change for the life of a terminal.
- New workspace = one pane, one terminal tab. Split = a new pane with one terminal tab beside the focused pane.
- Workspace ring = its most urgent terminal (Waiting > Done > Working > Idle); its question = that terminal's.
- Closing a workspace with 2+ terminals asks first (native confirm, the follow-up's copy pattern); single tab / pane
  closes don't ask; bulk closes keep always asking.
- Sidebar row: status mark (left), **bold name**, line 2 `branch • ~/folder` (branch only in a git repo), and while
  waiting a status line + a 🔔 "Needs input" pill; selected row filled with `accent` (text on it ≥ 4.5:1); pinned
  workspaces first with 📌; group headers stay for the rest; ×-on-hover, middle-click, the right-click menu (+ Pin /
  Unpin), colour dot and ⌘1–9 act on workspaces.
- Pane tab bar (replaces pane headers): 26 px; tab = icon + title + (terminal) mark + × on hover; active tab
  highlighted; right-side buttons: new terminal tab, new browser tab, split right, split down. ⌃Tab / ⌃⇧Tab cycle tabs
  in the focused pane. The bar shows even for a single pane (cmux does).
- In-pane browser: a native `WKWebView` positioned over its GPUI placeholder (bottom-left origin and backing scale
  handled), hidden whenever (a) its tab isn't the active tab of a visible pane, (b) any fuxx overlay/menu/popover is
  open, (c) a drag (divider, sidebar, tab) is in progress; keyboard focus moves to the page only on a click in it or
  after a navigation the person started; ⌘W / ⌘T / ⌘D / ⌘1–9 / ⌘P / ⇧⌘U still reach fuxx while the page has focus.
- Browser bar (GPUI): back, forward, reload/stop, address field; suggestions from a visited-pages list that exists only
  with "Keep history and data" On (`browser-history.json`, deleted by the same wipe); last suggestion "Search Google for
  “…”" (`https://www.google.com/search?q=` + URL-encoded text); the http/https rule for typed addresses.
- Agents: `browser.*` target the agent's workspace's browser tab (reuse; else split the agent's pane right with a new
  browser tab). "Pop Out" moves a browser tab into the M10c separate window.
- Git branch: from the workspace's first terminal's working folder (OSC 7 if reported, else the process's cwd via
  `proc_pidinfo`), `git rev-parse --abbrev-ref HEAD` run off the UI thread with a 2 s cap, refreshed at most every 5 s per
  visible workspace, cached.
- Title area: 🔔 (popover listing this run's notifications, newest first, max 50; click jumps; a dot while unseen) and
  + (new workspace) right of the sidebar button; the selected workspace's name at the content area's top.
- Saved state: version bump with a loader for old files (each old layout → a workspace; each leaf → a pane with one
  terminal tab); workspaces keep name, colour, pinned, layout; tabs keep kind + (browser) URL.
- Scripting: `session.list` adds `workspace`, `pane`, `kind`; `fuxx new` = workspace, `--split` = pane, `--tab --of sN` =
  terminal tab in that pane.
- Performance and battery rules from M11 hold; contrast floors hold; every existing test passes or is deliberately
  updated with a ledgered ruling.

## Review Focus

1. **A fuxx overlay disappears behind the in-pane page** (rename box, ⌘P, right-click menu, host picker, address
   suggestions, Preferences sheet) → the hide rule (Tasks 0, 4).
2. **Keys go to the wrong side** (⌘W closes the page's tab vs the terminal; typing lands in the page after switching
   tabs) → focus rules (Tasks 0, 4).
3. **Terminals or agents lost in the model change** (a terminal with no tab, a tab with a dead terminal, restore or the
   background service putting terminals in the wrong workspace) → model invariants + migration tests (Tasks 1, 2).
4. **Scripts break** (`sN` ids, `fuxx new --split`, `fuxx ls` shape) → compatibility tests (Task 2).
5. **Git polling wakes the Mac or blocks the UI** → off-thread, capped, visible-only (Task 5).

---

### Task 0: Spike — a web view inside a pane — ⬜

- Hidden `fuxx __pane-browser-probe`: a GPUI window (off-screen position, never shown to the person), a placeholder
  element at known bounds, a `WKWebView` added as a subview of GPUI's content view and kept over the placeholder through
  a bounds change; checks the native frame equals the placeholder's (flipped, scaled); hides it when an overlay flag is
  set; loads a page and evaluates JS; confirms ⌘W reaches a GPUI action while the page is first responder (or records
  what's needed). Pure `pub fn native_frame(bounds, window_height, scale) -> NSRect` with tests.
- [ ] Findings in the report decide Task 4's approach; a blocker is reported as BLOCKED, not worked around.

### Task 1: The layout model — ⬜

- `src/layout_model.rs`: `WorkspaceId`, `PaneId`, `TabId`; `TabKind { Terminal(u64 /*sN*/), Browser }`;
  `Model { workspaces: Vec<Workspace{id, name: Option<String>, color, pinned, layout: Node<PaneId>}>, panes, tabs }` with
  ops `new_workspace`, `split(pane, dir) -> PaneId`, `add_tab(pane, kind)`, `close_tab -> Closed{tab, pane?, workspace?}`,
  `close_workspace`, `move_tab(tab, pane, index)`, `select`, `rollup(statuses)`, `shortcut(workspace) -> Option<u8>`,
  ordering (pinned first), serde + `from_old(SavedState v_old)`.
- [ ] Tests: invariants after every op (property-style over random op sequences), close-last cascades, roll-up order,
  migration of recorded old files (incl. M8 layouts and lone sessions), pinned ordering, ⌘N mapping.

### Task 2: The workspace on the model — ⬜

- `Workspace` renders `Model`: sidebar = workspaces; the focused workspace's layout = panes; each pane shows its active
  tab (terminal views keyed by `sN`); ⌘T / split / close / ⌘1–9 / drag-reorder / ⇧⌘U / ⌘P / notifications / titles act per
  the spec; save/restore (version bump + old-file loader); the background service's adoption places terminals in their
  saved tabs (M10a), SSH reconnect replaces a terminal in its tab (M10b); `session.list` + `fuxx new` changes (M9).
- For now a pane shows only its active tab without a tab bar (Task 3 adds it); browser tabs don't exist yet (Task 4).
- [ ] Tests: compatibility of scripting replies; restore of an old file; adoption into tabs; the confirm rule.
  ui-shots: a workspace with 3 panes.

### Task 3: Pane tab bars — ⬜

- Tab bar per pane per Global Constraints; new terminal tab button and ⌃Tab cycling; × closes a tab; right-click a tab:
  Close Tab, Close Other Tabs in Pane, Rename Tab… (AI's call); split buttons.
- [ ] Tests on the pure tab-bar view-model; ui-shots: bars in Mocha + Latte, hover, many tabs (scroll/truncate).

### Task 4: The browser in a pane — ⬜

- `pane_browser.rs` per Task 0's findings; the GPUI browser bar + suggestions popover (history only with keep-data On);
  the hide rule tied to every overlay (one central "an overlay is open" signal from `close_overlays`/Overlay); focus
  rules; agents' `browser.*` retargeted to browser tabs; "Pop Out" to the M10c window; browser tabs saved with URL.
- [ ] Tests: hide-rule decision, suggestion ranking + search URL, agent targeting; probe extended; ui-shots of the bar
  and suggestions (the page itself is native and won't appear in shots).

### Task 4b: Resizing doesn't make a terminal look busy — ⬜ (added 2026-11-06, your bug report)

- Cause: a resize sends the shell a redraw signal (SIGWINCH); the redrawn prompt arrives as output > 150 ms after the
  last keystroke, so `StatusTracker::on_output` marks the terminal Working, then it settles to Done (and may notify).
- `StatusTracker::on_resize(now)` opens a redraw window (`RESIZE_WINDOW` = 500 ms, renewed by every resize while a
  drag goes on); output inside it changes nothing (no Working, no Done, no notification, no `last_output` bump). Called
  from both resize paths (in-app terminals in `terminal_view.rs`, background-service terminals in `daemon_session.rs`).
- [ ] Tests (first): idle + resize + output → still Idle; Waiting + resize + redraw → still Waiting, no repeat
  notification; output after the window → Working as before; a resize during a live drag keeps extending the window;
  attention (OSC notifications) inside the window still counts.

### Task 4c: Drag the window by an empty tab bar — ⬜ (added 2026-11-06, your request)

- The empty part of a pane tab bar whose top touches the window's top edge drags the window (double-click zooms, as
  the titlebar band already does); tabs and the bar's buttons keep their clicks; tab bars lower down (under a
  horizontal split) don't drag.
- [ ] Tests: the pure "is this bar at the window top" rule over layouts (single pane, side-by-side, stacked, nested);
  on-screen check added to Task 8.

### Task 4d: ⌘T = new tab, ⌘N = new workspace — ⬜ (added 2026-11-06, your request)

- **⌘T** opens a new terminal tab in the focused pane (as the tab bar's new-terminal button does), also when the
  focused tab is a browser tab; with no workspace open it makes a new workspace. **⌘N** (unbound until now) makes a new
  workspace (what ⌘T did). Menus show both ("New Workspace ⌘N", "New Tab ⌘T"); the sidebar footer's "+ New", the
  title-area +, hints, tooltips and Help text follow; Preferences / scripting unchanged.
- [ ] Tests: the binding table (⌘T → new tab action, ⌘N → new workspace action); new tab lands in the focused pane at the
  end of its bar (as the bar's new-terminal button) and is selected; ⌘T with no workspace makes one; menu titles.

### Task 4e: Browser shortcuts inside a page — ⬜ (added 2026-11-06, your answer to review #14)

- While an in-pane page has the keyboard: **⌘R** reloads the page, **⌘[ / ⌘]** go back / forward, **⌘= / ⌘- / ⌘0**
  zoom the page in / out / reset (WKWebView `pageZoom`, per tab, kept for the run). Everywhere else these keys keep
  fuxx's meaning (rename, previous / next pane, terminal font size). The fuxx keys listed in Global Constraints (⌘W / ⌘T
  / ⌘D / ⌘1–9 / ⌘P / ⇧⌘U, ⌃Tab) still reach fuxx from the page.
- [ ] Tests: the key routing decision (page has keyboard × key → page action or fuxx action), zoom steps and clamps;
  probe: ⌘R / ⌘[ in a focused page act on the page, not fuxx.

### Task 5: Sidebar rows, pinning, git branch — ⬜

- Rich rows, accent-filled selection, 📌 pinned section + Pin/Unpin in the menu, Needs-input pill; `git_branch.rs`
  (cwd + branch, off-thread, cached, visible-only).
- [ ] Tests: row view-model, pinned ordering, branch parsing (`HEAD` detached → short sha), the refresh rule; ui-shots.

### Task 6: Title area — ⬜

- 🔔 notifications popover (this run's list, newest first, max 50, unseen dot, click jumps) and + (new workspace); the
  workspace name at the content top.
- [ ] Tests: notification list model; ui-shots.

### Task 7: Drag tabs anywhere — ⬜ (required since 2026-11-06, your request; was optional)

- Drag a tab to reorder it within its pane (an insertion line shows where it lands); into another pane's tab bar (same
  or another split); or onto a sidebar workspace row (the tab moves into that workspace's focused pane, at the end, and
  is selected). A pane or workspace emptied by the move closes through the model's cascade. The in-pane page hides
  while a tab drag is under way (Global Constraints, hide rule (c)). Esc or dropping elsewhere cancels.
- [ ] Tests: the pure drop-target rule (insertion index from the pointer's x over tab bounds; bar vs sidebar row vs
  nowhere), `move_tab` across panes and workspaces with cascades (model invariants hold), the terminal keeps its `sN`
  and keeps running; ui-shots of the insertion line and a highlighted drop row.

### Task 6b: 🔔 and + in the sidebar footer — ⬜ (added 2026-11-06, your request; after Task 7)

- The 🔔 notifications button moves from the title area to the sidebar footer, beside the Preferences gear; its popover
  opens upward from there (same list, dot and jump). The footer's "+ New" becomes an icon-only **+** (same menu, a
  tooltip "New…"). The title-area + is removed; the band keeps the sidebar button. With the sidebar hidden, the 🔔 is
  hidden too (as the gear is).
- [ ] Tests: footer layout/hit rule (pure where possible); the popover still in `Overlay`; ui-shots of the footer and the
  upward popover in Mocha + Latte; the band without 🔔/+.

### Task 8: On screen — 👀 ⬜ (batched)

The spec's success check plus Review Focus 1–2 by hand.

### Task 9: Docs — ⬜

## Verification log

_(filled in as tasks complete)_
