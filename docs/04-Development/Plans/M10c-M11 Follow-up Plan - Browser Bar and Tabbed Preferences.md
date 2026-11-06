---
tags: [development, plan, milestone]
milestone: M10c/M11 follow-up
status: planned
updated: 2026-11-06
---

# M10c/M11 Follow-up Plan — Browser top bar, address bar, keep-data setting, tabbed Preferences

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

| At a glance | |
|---|---|
| **Spec** | The 2026-11-06 amendments in [[52 - Milestone 10c - Agent Browser]] and [[53 - Milestone 11 - UI-UX Uplift]] |
| **Status** | ⬜ Planned (2026-11-06) |
| **Tasks done** | 4 / 8 (3d moved to M11b) |
| **Decisions used** | Yours (2026-11-06): build now, before the merge; the address bar has its own setting; bar off keeps the buttons; keep-data Off by default and turning it off deletes saved data; Preferences sections General / Appearance / Sessions / Browser. |
| **Needs your eyes** | 👀 Task 4 (batched with M10b, M10c, M11) |

**Goal:** A Safari-like compact top bar on fuxx's browser windows with a working address bar, two Browser settings,
and Preferences rebuilt as vertical tabs.

**Architecture:** Browser chrome is native AppKit through objc2 (`NSToolbar` in the unified-compact style, `NSButton`s
with SF Symbols, an `NSTextField` address field, a thin progress view), added in `browser.rs` with small `define_class!`
delegate/target objects owned by `Browser`, as M10c did for its delegates. The data-store choice lives in `browser.rs`
(one non-persistent `WKWebsiteDataStore` per run, or the default persistent one). Settings parse in `appearance.rs`.
The Preferences rebuild is GPUI in `preferences.rs`.

**Tech Stack:** No new crates. objc2-app-kit / objc2-web-kit / objc2-foundation features as needed (verify names in
the registry source).

## Global Constraints

- Setting keys: `browser-address-bar = on|off` (default `on`), `browser-keep-data = on|off` (default `off`). Junk
  values warn like the other settings and keep the default.
- Keep-data Off: one `WKWebsiteDataStore::nonPersistentDataStore()` created on first use per run, shared by every
  browser window opened while Off. On: `WKWebsiteDataStore::defaultDataStore()`. A window keeps the store it was
  opened with. On → Off: remove all website data of all types from the default store (`removeDataOfTypes:
  allWebsiteDataTypes modifiedSince: distantPast`), once, without blocking the UI.
- Address field: Enter → `browser_url::normalize_url` → load, or show the error in the field (e.g. as its tooltip and
  a red-tinted text for 2 s) without loading; Escape → the current URL; ⌘L focuses and selects it; the field shows the
  current URL as pages load (unless the user is editing it).
- Back / forward enabled only when the page can go back / forward; reload becomes stop while loading. Loading line:
  2 px accent, width = estimated progress, hidden when idle. Placeholder: "Enter an address" (no search engine).
- Window appearance follows the fuxx theme: dark themes → darkAqua, light → aqua; updates on theme change.
- The address-bar setting applies to open browser windows at once; keep-data applies to windows opened after it.
- Preferences: vertical section list on the left (General, Appearance, Sessions, Browser); only the active section's
  controls on the right; one fixed window size (the largest section); last-used section remembered for the run.
  General = Animations + a "Show the tips card again" button (resets `hint_dismissed`); Appearance = Theme, Font, Size,
  Ligatures + live preview; Sessions = Restore sessions, Keep running after quit; Browser = Show address bar, Keep
  history and data, with the note "Turning this off deletes saved browser data. Applies to browser windows opened after
  the change."
- Agents (`fuxx browser …`, MCP) unchanged; the M10c probe keeps passing; every existing test passes; M11 contrast
  floors and tokens apply.

## Review Focus

1. **Keep-data Off still leaves data on disk** (the default store used by mistake) → the probe shows an Off window's
   store is non-persistent and shared within the run, and an On window's store is the default one (Task 1).
2. **The address field loads `javascript:` / `file:`** → it goes through `normalize_url` only (Task 2 test + probe).
3. **The toolbar breaks ⌘W / ⌘Z / editing keys or the "agent opens don't take the keyboard" rule** → M10c's
   ShortcutWindow rules still hold; the field takes focus only on ⌘L or a click (Task 2).
4. **objc2 delegate/target lifetimes** outlive their window or are freed early → owned by `Browser`, cleared in `Drop`
   like M10c's delegates (Task 2).
5. **Preferences tabs lose a setting or change its file format** → every control writes exactly as before (Task 3).

---

### Task 1: Settings and the data store — ⬜

- `appearance.rs`: `browser_address_bar: bool`, `browser_keep_data: bool` with `*_from_entries` parsers + tests;
  `prefs.rs` value helpers.
- `browser.rs`: `fn data_store(keep: bool, mtm) -> Retained<WKWebsiteDataStore>` (the per-run non-persistent store kept
  for the run); `Browser::open` takes the choice; `pub fn forget_saved_data(mtm)` (async remove from the default store,
  one log line when done).
- Workspace: passes the current setting when opening a browser; on an `Appearance` change from keep-data On → Off,
  calls `forget_saved_data`.
- Probe: two keep=off browsers share one store whose `isPersistent` is false; a keep=on browser's store is the default
  one (`isPersistent` true). The probe never wipes the real default store.
- [ ] Tests RED → GREEN; probe output in the report.

### Task 2: The top bar and address field — ⬜

- Unified-compact `NSToolbar`; items: back, forward, reload/stop (SF Symbols `chevron.left`, `chevron.right`,
  `arrow.clockwise` / `xmark`) and the flexible address field; the progress line under the toolbar; title shown when
  the address field is hidden.
- Button states and field text refresh on the existing 500 ms browser timer and right after navigation calls.
- Probe: the toolbar exists with its items; Enter with `127.0.0.1:<port>/x` loads it; `javascript:alert(1)` doesn't;
  back/forward states after two navigations; ⌘W / ⌘Z / font keys behave as before.
- [ ] Tests on the pure parts (button-state decision, field text) RED → GREEN; probe output in the report.

### Task 2b: The fuxx mark in the bar — ⬜ (added 2026-11-06, your request)

- A small 🖕 mark (fuxx's logo, your choice) as the last toolbar item, to the right of the address field, drawn as a
  crisp image (emoji rendered at the toolbar's icon size, ~18 pt, vertically centred), not clickable. It stays when the
  address field is hidden (to the right of the buttons/title). Same in light and dark bars.
- [ ] Probe: the toolbar's last item is the mark, with or without the address field.

### Task 3: Preferences as vertical tabs — ⬜

- Left: a 160 px section list (selected = selection token + 2 px accent bar, like sidebar rows); right: the active
  section's form; window size = the largest section's; ui-shots per section in Mocha and Latte.
- Move every existing control without changing what it writes; add the two Browser switches and the General button.
- [ ] Tests: `sections_list_every_control_once`, `browser_values`, `window_fits_the_largest_section`; screenshots looked at.

### Task 3b: Sidebar button beside the window buttons — ⬜ (added 2026-11-06, your request)

- A sidebar-outline icon (16 px, drawn with paths, `text_dim`; hover → `text` on a rounded `panel_hover`) in the top
  band just right of the traffic lights (x ≈ 80, vertically centred on them; x = 13 in full screen), in the same spot
  whether the sidebar is shown or hidden. Click = the ⌘\ toggle (never starts a window drag); tooltip
  "Hide sidebar (⌘\)" / "Show sidebar (⌘\)".
- With motion on, the sidebar's width eases out/in over ~160 ms (ease-out); with Animations off or Reduce motion,
  instant. The terminal re-fits once at the end (no PTY resize storm during the slide).
- [ ] Tests: the icon's position rule (normal / full screen), the width curve; ui-shots: band with the sidebar shown
  and hidden, hover, Mocha + Latte.

### Task 3c: Sidebar rows as tabs — close, middle-click, right-click menu, tab colour — ⬜ (added 2026-11-06, your request)

- Row look (your sample): on hover the status mark's spot shows a × (click = close, like ⌘W); the name; on the right a
  colour dot (when a colour is set) and the shortcut "⌘1"…"⌘9" for the first nine sessions (no number column on the
  left any more). The age moves to the row's second line beside the question/note when space is short (AI's call:
  keep it right of the name when the row is ≥ 220 px wide, else drop it).
- Middle-click a row = close it.
- Right-click a row = a context menu (an overlay in the M11 panel style, one overlay at a time): Close Tab; Close
  Other Tabs; Close Tabs to the Right (= the rows below it); Show All Tabs (opens the ⌘P switcher); separator; Rename
  Tab…; Tab Color: a "none" swatch + 11 colour swatches (blue, purple, magenta, red, pink, orange, yellow, green, teal,
  cyan, grey — AI's call on the exact palette, contrast ≥ 3:1 on the sidebar). "Move Tab to New Window" is left out
  (your choice; multi-window is a later milestone).
- Bulk closes always ask first (your choice): a native confirm "Close N sessions?" (+ "M agents are still working or
  waiting." when any are), Close / Cancel.
- Tab colour is saved with the session (`SavedSession.color`, additive) and restored; also shown as the dot in the
  ⌘P switcher row and on the pane header. Scripting: `session.list` gains `color` when set (AI's call).
- [ ] Tests: the menu's actions on a pure session list (close others / to the right / which indices), colour
  round-trip + old files load, the shortcut label for rows 1–9 and none after; ui-shots: rows with colours and ⌘N,
  hover with ×, the context menu, Mocha + Latte.

### Task 3d: Tabs hold terminals — ➡️ moved to M11b (2026-11-06, your choice: build the full cmux layout as its own milestone; see [[M11b Input - Workspaces, Pane Tabs and In-Pane Browser]])

~~Original task text kept below for the record.~~

Today every terminal is its own sidebar row and a split shows several rows side by side. From now on:

- **A tab = a split layout of one or more terminals.** The sidebar lists tabs only (your choice); the right side shows
  the selected tab's terminals with their pane headers.
- ⌘T / "+ New" / presets / New SSH Session / `fuxx new` (without `--split`) make a **new tab with one terminal**. ⌘D /
  ⇧⌘D / `fuxx new --split` add a terminal **to the current tab**. Closing a terminal (⌘W, `exit`, pane close) removes it
  from its tab; closing a tab's last terminal closes the tab.
- **Group headers stay** (your choice): a tab belongs to the folder / SSH host of its first terminal; headers collapse
  and show the rolled-up ring as today.
- **A tab's ring** = the most urgent of its terminals (Waiting > Done > Working > Idle); its question line = that
  waiting terminal's question; its age = that terminal's. Its name = the focused terminal's name unless the tab was
  renamed (Rename Tab… names the tab; the pane rename box still names a terminal) — AI's call.
- **Closing a whole tab** (×, middle-click, Close Tab): asks first only when it has 2+ terminals ("Close tab with 3
  terminals?" + "M agents are still working or waiting." when M > 0) — your choice. Close Other Tabs / Close Tabs to
  the Right keep always asking (Task 3c).
- ⌘1…⌘9, the "⌘N" hint, drag-to-reorder, Tab Color, the right-click menu and the ⌘P switcher work on **tabs**; ⇧⌘U jumps
  to the waiting **terminal** (selects its tab, focuses its pane). Notifications name the terminal and its tab.
- **Saved and restored:** tabs (layout, name, colour) and their terminals; old `sessions.json` files load (each saved
  layout becomes a tab; any session not in another's layout becomes a one-terminal tab). With the background service
  (M10a) adopted terminals land back in their tabs.
- **Scripting:** terminal ids `sN` stay as they are; `session.list` / `fuxx ls` add each terminal's tab (`"tab": N`,
  the tab's 1-based position) — AI's call; `fuxx new --split … --of sN` adds to that terminal's tab.
- [ ] Tests on a pure tab model (new / split / close-last / move / rollup / restore of old files / ⌘N mapping); ui-shots:
  a sidebar of tabs with a 3-terminal tab selected (one waiting), Mocha + Latte.

### Task 4: On screen — 👀 ⬜ (batched)

The bar in light and dark themes; typing an address; Escape; ⌘L; back / forward / reload / stop; address bar off →
buttons only; keep-data Off → log in somewhere, quit fuxx, reopen → logged out; On → still logged in after a restart;
On → Off → saved data gone; each Preferences section and control still works.

### Task 5: Docs — ⬜

- [ ] Notes 52 / 53 amendment status; this plan + verification log; Progress Log; README (browser bar, settings).

## Verification log

_(filled in as tasks complete)_
