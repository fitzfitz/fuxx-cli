---
tags: [development, plan, milestone]
milestone: M10c/M11 follow-up
status: built
updated: 2026-11-06
---

# M10c/M11 Follow-up Plan — Browser top bar, address bar, keep-data setting, tabbed Preferences

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

| At a glance | |
|---|---|
| **Spec** | The 2026-11-06 amendments in [[52 - Milestone 10c - Agent Browser]] and [[53 - Milestone 11 - UI-UX Uplift]] |
| **Status** | 🟨 Built and reviewed (2026-11-06); on-screen check pending (577 tests) |
| **Tasks done** | 7 / 9 built and reviewed (Task 3d moved to M11b; Task 4 is your check) |
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

### Task 1: Settings and the data store — ✅

- `appearance.rs`: `browser_address_bar: bool`, `browser_keep_data: bool` with `*_from_entries` parsers + tests;
  `prefs.rs` value helpers.
- `browser.rs`: `fn data_store(keep: bool, mtm) -> Retained<WKWebsiteDataStore>` (the per-run non-persistent store kept
  for the run); `Browser::open` takes the choice; `pub fn forget_saved_data(mtm)` (async remove from the default store,
  one log line when done).
- Workspace: passes the current setting when opening a browser; on an `Appearance` change from keep-data On → Off,
  calls `forget_saved_data`.
- Probe: two keep=off browsers share one store whose `isPersistent` is false; a keep=on browser's store is the default
  one (`isPersistent` true). The probe never wipes the real default store.
- [x] Tests RED → GREEN; probe output in the report.

### Task 2: The top bar and address field — ✅

- Unified-compact `NSToolbar`; items: back, forward, reload/stop (SF Symbols `chevron.left`, `chevron.right`,
  `arrow.clockwise` / `xmark`) and the flexible address field; the progress line under the toolbar; title shown when
  the address field is hidden.
- Button states and field text refresh on the existing 500 ms browser timer and right after navigation calls.
- Probe: the toolbar exists with its items; Enter with `127.0.0.1:<port>/x` loads it; `javascript:alert(1)` doesn't;
  back/forward states after two navigations; ⌘W / ⌘Z / font keys behave as before.
- [x] Tests on the pure parts (button-state decision, field text) RED → GREEN; probe output in the report.

### Task 2b: The fuxx mark in the bar — ✅ (added 2026-11-06, your request)

- A small 🖕 mark (fuxx's logo, your choice) as the last toolbar item, to the right of the address field, drawn as a
  crisp image (emoji rendered at the toolbar's icon size, ~18 pt, vertically centred), not clickable. It stays when the
  address field is hidden (to the right of the buttons/title). Same in light and dark bars.
- [x] Probe: the toolbar's last item is the mark, with or without the address field.

### Task 3: Preferences as vertical tabs — ✅

- Left: a 160 px section list (selected = selection token + 2 px accent bar, like sidebar rows); right: the active
  section's form; window size = the largest section's; ui-shots per section in Mocha and Latte.
- Move every existing control without changing what it writes; add the two Browser switches and the General button.
- [x] Tests: `sections_list_every_control_once`, `browser_values`, `window_fits_the_largest_section`; screenshots looked at.

### Task 3b: Sidebar button beside the window buttons — ✅ (added 2026-11-06, your request)

- A sidebar-outline icon (16 px, drawn with paths, `text_dim`; hover → `text` on a rounded `panel_hover`) in the top
  band just right of the traffic lights (x ≈ 80, vertically centred on them; x = 13 in full screen), in the same spot
  whether the sidebar is shown or hidden. Click = the ⌘\ toggle (never starts a window drag); tooltip
  "Hide sidebar (⌘\)" / "Show sidebar (⌘\)".
- With motion on, the sidebar's width eases out/in over ~160 ms (ease-out); with Animations off or Reduce motion,
  instant. The terminal re-fits once at the end (no PTY resize storm during the slide).
- [x] Tests: the icon's position rule (normal / full screen), the width curve; ui-shots: band with the sidebar shown
  and hidden, hover, Mocha + Latte.

### Task 3c: Sidebar rows as tabs — close, middle-click, right-click menu, tab colour — ✅ (added 2026-11-06, your request)

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
- [x] Tests: the menu's actions on a pure session list (close others / to the right / which indices), colour
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

### Task 4: On screen — 👀 pending (batched)

The bar in light and dark themes; typing an address; Escape; ⌘L; back / forward / reload / stop; address bar off →
buttons only; keep-data Off → log in somewhere, quit fuxx, reopen → logged out; On → still logged in after a restart;
On → Off → saved data gone; each Preferences section and control still works.

The full numbered list is in "On-screen checklist (pending)" below.

### Task 5: Docs — ✅

- [x] Notes 52 / 53 amendment status; this plan + verification log; Progress Log; README (browser bar, settings).

## Verification log

Built on branch `m11b-browser-bar-prefs` from `m11-ui-ux` (fb94b14), one subagent per task with a separate reviewer. The
on-screen checks are yours and are **not** done, so nothing here is marked ✅ for the look. Off-screen pictures were looked
at for every UI task, but the glass toolbar items (buttons, field text) don't render off-screen, so their look is on your list.

| Task | Commits | Tests | Review |
|---|---|---|---|
| 1 Settings and data store | 2935e45 | 550 passing; RED unit tests + probe; the probe's `data_store` line shows Off = non-persistent and shared, On = the default store | Approved (sonnet), clean |
| 2 Top bar and address field | 0839ef9, fix round 25e9567 | 557 passing; RED for each fix; probe ok (toolbar items, Enter loads, `javascript:` refused, keys) | Approved (opus) with a ruling: pages must keep following macOS light/dark; fix round 1 addressed 3 of 3 |
| 2b fuxx mark | e10071d | 558 passing; RED unit + probe (mark is the last toolbar item, with or without the field) | Approved (sonnet), scoped with fix round 1, clean |
| 3 Preferences as tabs | b010aa3, f806edc | 561 passing; RED 4 | Approved (sonnet), clean |
| 3b Sidebar button | abf0ddb | 564 passing; RED 3 | Approved (sonnet), clean |
| 3c Rows as tabs | a4bf889, fix round ecfa75d | 574, then 576 passing; RED 13, then RED 1 | Approved (sonnet); fix round addressed 4 of 4 (haiku re-review) |
| Final whole-branch | 583e773 | 577 passing; `fmt --check` clean; ui-shots all scenes ok; probe ok | Ready after fixes (opus; Critical 0, Important 1, Minor 4) → fix pass 583e773 → re-review: all addressed (sonnet) |

Rulings made during the build (by effect, from the ledger):

- Task 2: pages keep following macOS light/dark (`prefers-color-scheme`, as before); only the bar and window chrome
  follow the fuxx theme. The page keeps the keyboard from the start; the field takes it only on ⌘L or a click.
- Task 3b: full screen keeps an empty 38 px band so the button doesn't sit on the first row.
- Task 3c: × shows on hover only (the active row too), so the focused session's mark stays visible; Close Tabs to the
  Right closes only the rows shown below the clicked one.
- Task 3b/3c: a re-toggled sidebar slide starts where the last one got to (Ruling U).
- Final: keep-data Off must not leave saved data behind. A flag in the sessions file (`browser_data_kept`) is set when a
  window opens with keep-data On; with the setting Off, launch and the last On window closing wipe the saved data; the
  flag clears only when WebKit reports the deletion done. The tab menu closes when the keyboard moves to a terminal.

## Deferred minors

Left on purpose (none blocks the merge); each is a candidate for a later polish pass.

- Task 1: the wipe's log line says "deleted" even if WebKit's removal failed (WebKit has no error API there).
- Task 2: a field Enter during an agent's open makes the agent's open report the person's page (document it);
  `set_look` re-applies on every Appearance change; `state()` is read twice per tick; the probe has duplicated
  PNG-writing code.
- Task 2/2b: occlusion lag right after a window shows (the first tick can be slow, up to 500 ms); the emoji is centred by
  its line box; the mark may land in the overflow menu as "fuxx" in a very narrow window.
- Task 3: the disabled tips button looks almost live ("Shown" label, unchanged border); the tips lookup runs once when
  Preferences opens; stale old `preferences-*.png` files in `target/ui-shots`.
- Task 3b: a blank strip beside the narrowed panes for up to 160 ms while the sidebar shows (one re-fit at the start);
  the Latte hover fill on the sidebar button is nearly invisible (the `panel_hover` token value).
- Task 3c: the bulk-close sheet and its Cancel are untested (native prompt); the menu has no ↑↓/Enter keyboard
  navigation; `Theme::ui()` fits the 11 tab colours on every call (not cached).
- Final: `clippy --all-targets --all-features -D warnings` has 26 errors that were there before this work; a wipe whose
  WebKit completion never runs is retried every 500 ms while a browser is open (no backoff; overlap-guarded).
- **Known limitation (parked residual N1):** while the background service is lost or its List failed (M10a's
  `hold_saving` is active), opening a keep-data-On browser can't save the `browser_data_kept` flag, so a later switch to
  Off might not wipe that data. It needs a lost service and keep-data On in the same run; no second fix wave.

## On-screen checklist (pending)

Not run yet. Batched with M10b, M10c and M11 (your choice). Run `fuxx` from the `.app` so macOS treats it as an app.

1. Run fuxx from the .app opened in Finder (not from a terminal).
2. Open a browser (⇧⌘B) in a dark theme (Mocha): the bar is dark, the traffic lights sit inline, the three buttons form a
   capsule, the address field fills the rest. Switch to a light theme (Latte) with it open: the bar turns light at once.
3. The 🖕 mark sits right of the address field, is not clickable, and is yellow while the window is active (grey when inactive).
4. Type an address and press Enter: it loads and the keyboard goes to the page. The thin accent loading line grows under
   the bar during a slow load and vanishes when done.
5. Type `javascript:alert(1)` and press Enter: the text turns red for about 2 s, the tooltip says why, nothing loads.
6. Press Escape while typing: the current address comes back. Press ⌘L: the field is focused and fully selected.
   Opening a browser (⇧⌘B, or an agent's `browser open`) never puts the keyboard in the field.
7. Back and forward are enabled only when they can go; reload turns into ✕ while loading and stops the load.
8. Set macOS and fuxx to opposite looks (macOS dark + a light fuxx theme, then the reverse): the web page follows macOS
   light/dark while the bar follows the fuxx theme.
9. Preferences → Browser → Show address bar Off: open browser windows lose the field at once, keep the buttons and show
   the page title; On brings it back.
10. Keep history and data Off (the default): log in somewhere, quit fuxx, reopen: you are logged out. Switch it On, log in,
    quit, reopen: still logged in.
11. On, then Off: saved data is gone (relaunch with On: logged out) and the log shows `[fuxx] browser: saved data deleted`.
    With a browser window still open when you switch to Off, that window stays logged in; once it closes, a later On is logged out.
12. Hand-edit the config to remove `browser-keep-data = on` after an On session, relaunch, then turn On: logged out
    (the launch wipe).
13. Preferences opens on General; every section (General, Appearance, Sessions, Browser) shows only its controls and every
    control works at once; ↑/↓ moves through the sections; close with ⌘W and reopen with ⌘,: it opens on the last section.
14. General → "Show the tips card again": dismiss the sidebar tips card with ×, the button turns live; click it, the card
    returns, the button greys with "Shown", and the card is still there after a restart.
15. The sidebar button sits just right of the green light (the same spot with the sidebar shown or hidden); click toggles
    like ⌘\ and never drags the window; the tooltip reads "Hide sidebar (⌘\)" / "Show sidebar (⌘\)"; the slide is smooth
    with Animations on and instant with it off or Reduce motion.
16. Full screen: the button sits at the left edge in an empty band, not on the first row; typing still goes to the terminal
    after clicking the button.
17. Sidebar rows: hover shows × in the mark's place (the active row keeps its mark until hovered); × closes the session;
    middle-click closes without selecting or dragging.
18. Right-click a row: the menu opens at the pointer and stays inside the window; Close Tab, Close Other Tabs, Close Tabs
    to the Right (only the rows shown below), Show All Tabs (opens ⌘P) and Rename Tab… all work; disabled items look
    disabled; Escape or a click outside closes it.
19. Close Other Tabs and Close Tabs to the Right ask in a native sheet ("Close N sessions?", plus the busy line when agents
    are working); Return = Close, Cancel and Escape change nothing.
20. Tab Color: pick one: the dot shows on the row, in ⌘P and on the pane header, and survives a restart (also with
    persistent sessions on); "none" clears it. Rows 1–9 show "⌘1"…"⌘9" on the right and no number on the left.
21. With the right-click menu open, press ⌘1–9, ⌘T or ⇧⌘U: the menu closes, and Escape then reaches the terminal.
22. Press ⌘\ twice quickly mid-slide: the slide turns back smoothly from where it was.
