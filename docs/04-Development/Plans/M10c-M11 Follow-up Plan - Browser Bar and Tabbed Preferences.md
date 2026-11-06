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
| **Tasks done** | 1 / 6 |
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

### Task 4: On screen — 👀 ⬜ (batched)

The bar in light and dark themes; typing an address; Escape; ⌘L; back / forward / reload / stop; address bar off →
buttons only; keep-data Off → log in somewhere, quit fuxx, reopen → logged out; On → still logged in after a restart;
On → Off → saved data gone; each Preferences section and control still works.

### Task 5: Docs — ⬜

- [ ] Notes 52 / 53 amendment status; this plan + verification log; Progress Log; README (browser bar, settings).

## Verification log

_(filled in as tasks complete)_
