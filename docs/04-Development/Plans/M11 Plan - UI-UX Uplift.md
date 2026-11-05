---
tags: [development, plan, milestone]
milestone: M11
status: built
updated: 2026-11-06
---

# M11 Plan — UI/UX Uplift (fun, but professional)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [x]`) syntax for tracking.

| At a glance | |
|---|---|
| **Milestone note** | [[53 - Milestone 11 - UI-UX Uplift]] (the spec) · evidence: [[M11 Input - UI-UX Review]] |
| **Status** | 🟨 Built and reviewed (2026-11-06); on-screen check pending |
| **Tasks done** | 10 / 11 (Task 9 is your on-screen check, batched with M10b and M10c) |
| **▶ Pick up here** | Task 9, when you run the batched checks ([[53 - Milestone 11 - UI-UX Uplift]], "On-screen checklist") |
| **Decisions used** | Yours: "fun and not boring but keep it professional"; animations can be turned off; autonomous run. The rest is the AI's call (spec §"Answers to the review's open questions"). |
| **Needs your eyes** | 👀 Task 9 (batched with M10b and M10c) |

**Goal:** Ship the spec's window, sidebar, status marks, splits, navigation, preferences and light-theme work, with every
visual change backed by off-screen screenshots that reviewers (and the controller) look at.

**Architecture:** First a screenshot harness (GPUI's `VisualTestAppContext` at the pinned rev, behind a fuxx cargo
feature `ui-shots`), so each task renders its scenes in a dark and a light theme to PNGs. Then design tokens and
contrast in `theme.rs` + a new `ui_tokens.rs`; then the pieces, each as pure view-model code (unit-tested) plus a thin
render layer (screenshot-checked). Motion runs only from one Workspace ticker (10 fps) while something visible
animates and `animations` is on.

**Tech Stack:** No new crates. The pinned GPUI's `test-support` features (on `gpui` and `gpui_platform`) only under the
new `ui-shots` feature, never in normal builds. objc2 (already a dependency) for the Reduce-motion setting.

**Spec:** [[53 - Milestone 11 - UI-UX Uplift]] · [[12 - Scope]]

## Global Constraints

- Contrast floors (WCAG 2.x relative luminance, sRGB-linearised): text ≥ 4.5:1 on its background; status marks,
  focus/accent lines ≥ 3:1; plain 1 px dividers and panel borders are held to a quiet 1.4:1 (amended). Enforced by a test over every built-in theme and the default theme.
- New setting `animations = on|off` (default `on`). Motion also stops when macOS "Reduce motion" is on. With motion
  off: no spinning, pulsing, popping or animated transitions; marks are drawn in their still form.
- The only continuous redraw: one Workspace timer at 100 ms (10 fps) running only while animations are on and a
  Working or Waiting mark is on screen. One-shot animations (Done pop, ≤ 400 ms; chevron rotate, 120 ms) use GPUI
  `with_animation`.
- Sizes: sidebar rows 28 px (any row with a second line grows to 42 px: note, progress, "not live" or question; amended); group header 24 px; pane header 22 px;
  top band 38 px; traffic lights at (13, 12); terminal padding 10 px sides / 8 px top-bottom (8 / 6 in splits) with the
  leftover split evenly; sidebar width 180–400 px, default 240; status mark box 12 px; dividers 1 px, 2 px accent on
  hover/drag; unfocused veil 0.4.
- Text sizes: 13 px names, 11 px numbers/meta (tabular figures), 11 px group headers in small caps (no letter spacing in GPUI; amended).
- Shortcuts (check each is free first): ⇧⌘U jump to what needs you, ⌘P switcher, ⌘\ toggle sidebar, ⌘= / ⌘− / ⌘0
  font size, ⌘H hide, ⌘M minimize. Existing bindings unchanged.
- Window: integrated titlebar (transparent, app-owned drag on the top band, double-click zooms); default size 1280 × 800.
- Copy (exact): waiting-row question = the matched screen line, trimmed, one line, ellipsised; notification summary
  `#<n> <group or name> — <message>`; window title `fuxx — #<n> <name>`.
- UI state (sidebar width, sidebar hidden, first-run hint dismissed) is saved in `sessions.json`'s `SavedState`
  (additive fields), not in the config file.
- Every UI task attaches `ui-shots` PNGs (Catppuccin Mocha and Catppuccin Latte) of its scenes to its report.
- Behaviour of terminals, sessions, scripting, SSH and browsers is unchanged; existing tests pass (test changes only
  where a colour/size assertion is deliberately replaced).

## Review Focus

1. **Light themes stay unreadable somewhere** → the contrast test covers every token pair actually drawn (Task 1), and
   Latte screenshots for every scene (all UI tasks).
2. **The spinner keeps the GPU busy** (redraws when nothing animates, or at 60 Hz) → a test on the ticker's run
   condition; the ticker stops when no Working/Waiting mark is visible or animations are off (Task 4).
3. **Clicks land in the wrong place after padding / titlebar changes** (selection, links, mouse reporting, divider
   drags) → tests on the inset geometry and hit-testing (Task 2).
4. **The waiting question shows something private or garbage** (a password prompt, escape sequences) → only the
   matched line, control characters stripped, capped length, secret-looking prompts suppressed (Task 3).
5. **Keyboard focus gets lost** after the switcher, rename, sidebar toggle or a pane-header click → focus returns to
   the active terminal; on-screen checks + focus-path code review (Tasks 6, 7).

---

## Files

- Create: `src/ui_tokens.rs`, `src/ui_shots.rs` (feature `ui-shots`), `src/status_mark.rs`, `src/sidebar.rs`,
  `src/switcher.rs`, `src/pane_header.rs`, `src/reduce_motion.rs`.
- Modify: `Cargo.toml`, `src/theme.rs`, `src/main.rs`, `src/workspace.rs`, `src/terminal_view.rs`, `src/panes.rs`,
  `src/status.rs`, `src/terminal.rs`, `src/name_input.rs`, `src/notifier.rs`, `src/preferences.rs`, `src/prefs.rs`,
  `src/appearance.rs`, `src/restore.rs`, `src/groups.rs`.

### Task 0: Screenshot harness — ✅

- [x] Cargo feature `ui-shots = ["gpui/test-support", "gpui_platform/test-support"]` (verify the feature names in the
  pinned source). `src/ui_shots.rs` (compiled only with the feature): `pub fn shoot(name: &str, theme: &Theme, size:
  Size<Pixels>, build: impl FnOnce(&mut Window, &mut App) -> AnyView) -> PathBuf` using
  `VisualTestAppContext::new(gpui_platform::current_platform(false))`, `open_offscreen_window`, `capture_screenshot`,
  writing `target/ui-shots/<name>-<theme-slug>.png`.
- [x] An ignored test `ui_shots_smoke` that renders a simple themed panel in Mocha and Latte and asserts the PNGs exist,
  have the right size and are not one flat colour. Run: `cargo test --features ui-shots -- --ignored ui_shots`. (As built: scenes run from a hidden `__ui-shots` entry, `cargo run --features ui-shots -- __ui-shots [filter]`, because AppKit needs the main thread.)
- [x] Subagents may run this (off-screen windows, exits by itself). Normal `cargo test` is unaffected.
- [x] Commit `test(m11): off-screen UI screenshots behind the ui-shots feature`.

### Task 1: Design tokens and contrast — ✅

- `ui_tokens.rs`: `pub fn contrast(a: Rgb, b: Rgb) -> f32` (WCAG), `pub fn ensure_contrast(fg: Rgb, bg: Rgb, min: f32)
  -> Rgb` (moves fg toward black or white along its own hue until it meets `min`), sizes and text-size constants from
  Global Constraints.
- `UiColors` gains `accent`, `text_faint`, `divider`, `waiting_tint`, `done_tint`, `focus_ring`, `panel`,
  `panel_border`, `shadow`; existing fields kept. `Theme::ui()` derives them (accent = the theme's blue, palette[4],
  pushed to ≥ 3:1 on the sidebar; status colours from their palette slots pushed to ≥ 3:1; text ≥ 4.5:1).
- [x] Tests: `every_builtin_theme_meets_the_floors` (all drawn pairs listed in the test), `ensure_contrast_keeps_hue`,
  `light_themes_get_light_chrome`; update `ui_colors_follow_light_and_dark_themes` (status colours may now differ from
  raw slots).
- [x] ui-shots: a token swatch board per theme (all 6 built-ins + default).
- [x] Commit `feat(m11): design tokens with a contrast floor`.

### Task 2: Window chrome and terminal padding — ✅

- Integrated titlebar (`TitlebarOptions { appears_transparent: true, traffic_light_position: (13, 12) }`,
  `app_owns_titlebar_drag`, the 38 px top band starts a window move; double-click zooms per macOS setting). Default
  1280 × 800. The sidebar's top 38 px is the band; the terminal area also has a 38 px band when the sidebar is hidden.
- Terminal padding: one `pub fn inset(bounds, split: bool) -> Bounds` used for `dims_for`, the grid origin
  (hit-testing) and painting; leftover pixels split evenly.
- Menus: View (Toggle Sidebar ⌘\, Bigger ⌘=, Smaller ⌘−, Actual Size ⌘0 — write `font-size` like Preferences does),
  Window (Minimize ⌘M, Zoom), fuxx menu Hide ⌘H.
- [x] Tests: `inset_balances_leftover`, `hit_testing_uses_the_inset_origin`, `font_size_steps_write_the_setting`.
- [x] ui-shots: the whole window (real Workspace with one plain session) in both themes.
- [x] Commit `feat(m11): integrated titlebar, padding and full menus`.

### Task 3: Status data for the UI — ✅

- `status::waiting_line(tail: &str) -> Option<String>` — the line that matched a waiting pattern; trimmed, control
  characters removed, ≤ 120 chars; a line containing "password", "passphrase" or "token" (case-insensitive) gives
  `None`.
- Sessions get `status_since: Instant` (set in `set_status`; adopted/restored sessions start now) and `question:
  Option<String>` (computed GUI-side from the session's screen tail when it turns Waiting, cleared otherwise).
- `pub fn age_label(d: Duration) -> String` ("12s", "4m", "2h", "3d").
- [x] Tests: `waiting_line_returns_the_matching_line`, `secret_prompts_give_no_line`, `age_labels`.
- [x] Commit `feat(m11): the waiting question and time in state`.

### Task 4: Status marks and motion — ✅

- `status_mark.rs`: `pub fn mark(status: Status, ui: &UiColors, phase: f32, popping: Option<f32>, size: Pixels) ->
  impl IntoElement` drawing with `canvas` + `PathBuilder`: Idle hollow ring; Working arc (270°) rotated by `phase`;
  Waiting solid dot + halo (alpha from `phase`); Done dot + check, scaled by the pop curve when `popping`.
- `appearance.rs`: `animations` setting (`animations_from_entries`, default on, warnings like the others).
  `reduce_motion.rs`: `pub fn system_reduce_motion() -> bool` (NSWorkspace
  `accessibilityDisplayShouldReduceMotion`, read at start and refreshed every 5 s or on its change notification).
- Workspace ticker: `pub fn should_tick(animations: bool, reduce: bool, visible: &[Status]) -> bool`; a 100 ms timer
  advancing `phase` and notifying only while `should_tick`. Done pop: 350 ms one-shot when a visible session turns Done.
- [x] Tests: `should_tick_only_when_something_moves`, `animations_setting_parses`, `pop_curve_ends_at_rest`.
- [x] ui-shots: each mark in each status at 3 phases, both themes.
- [x] Commit `feat(m11): shaped status marks with motion you can turn off`.

### Task 5: The sidebar — ✅

- `sidebar.rs`: a pure view-model `SidebarRow` (header or session: number, label, status, age, question, active,
  on_screen, grouped, progress, note) built from the Workspace, and the render functions moved out of `workspace.rs`.
- Row spec, active fill + 2 px accent bar, on-screen pip, waiting tint + question line, age on the right, headers
  (count, hover-only "+", chevron rotate, rolled-up mark only when collapsed), `overflow_y_scroll` + keep the active
  row in view, resizable (drag the right edge, 180–400, saved), ⌘\ hides it (saved), footer ("+ New" menu with
  presets; gear → Preferences), first-run hint card (saved dismissal), drag polish (lifted preview, 40 % source,
  insertion line).
- `SavedState` gains `sidebar_width: Option<f32>`, `sidebar_hidden: bool`, `hint_dismissed: bool` (serde defaults).
- [x] Tests: `rows_build_from_sessions` (active/on-screen/waiting/question/age), `width_clamps`,
  `ui_state_round_trips_and_old_files_load`, `headers_count_and_roll_up_only_when_collapsed`.
- [x] ui-shots: sidebar with 8 sessions in 2 groups (one waiting with a question, one working, one done, one idle,
  one collapsed group), plus the hint card, both themes; and an overflowing list.
- [x] Commit `feat(m11): a sidebar that reads at a glance` (may be 2–3 commits).

### Task 6: Splits — ✅

- `pane_header.rs`: 22 px header per leaf when a session has more than one pane: mark, number, name, age; focused =
  accent underline; waiting = tint; clicking it focuses that pane.
- Dividers: 1 px `divider`, 2 px accent on hover and while dragging; veil 0.4.
- [x] Tests: header model (`header_shows_only_when_split`), divider state.
- [x] ui-shots: a 3-pane split, focused / waiting variants, both themes.
- [x] Commit `feat(m11): pane headers and quieter dividers`.

### Task 7: Getting around and smaller fixes — ✅

- ⇧⌘U: `pub fn next_needing_you(sessions, current) -> Option<index>` (longest-waiting Waiting first, then Done, cycling).
- ⌘P switcher (`switcher.rs`, the host-picker overlay pattern): filter box, rows sorted by `pub fn urgency_order`
  (Waiting by age, Done, Working, Idle; then number), each with mark, number, name, group, question; Enter jumps.
- Rename selects the old name. Notification summary and window title per Global Constraints.
- [x] Tests: `next_needing_you_cycles`, `urgency_order`, `switcher_filters`, `notification_and_title_text`.
- [x] ui-shots: the switcher with 8 sessions, both themes.
- [x] Commit `feat(m11): jump to what needs you, a session switcher, and smaller fixes`.

### Task 8: Preferences — ✅

- Two-column form (labels left, controls right), sections Appearance (Theme swatches, Font list shown in each font,
  Size, Ligatures, **Animations On/Off**), Sessions (Restore, Keep running after quit), and a live preview pane painting
  a canned terminal snapshot with `paint_grid` in the chosen theme and font. Window height computed, not hand-summed.
- [x] Tests: `animations_values`, `swatch_colours_come_from_the_theme`, a layout-height test.
- [x] ui-shots: Preferences in both themes.
- [x] Commit `feat(m11): preferences with swatches, a live preview and the animations switch`.

### Task 9: On screen — 👀 pending (batched with M10b and M10c)

The spec's success check, plus: dragging the window by its top band; double-click zoom; full screen; the sidebar
resize and ⌘\; the hint card; ⌘=/⌘−/⌘0; Reduce motion in System Settings stops the motion; battery: Activity Monitor
shows fuxx idle (≈0 % CPU) when nothing is working.

### Task 10: Update the docs — ✅

- [x] Note 53 (built; on-screen pending); this plan + verification log + deferred minors; roadmap; tracker; Progress Log;
  STRUCTURE.md; README (the new shortcuts; how to make screenshots with `cargo run --features ui-shots -- __ui-shots`).

Every task was built test-first (RED shown before GREEN) and reviewed by a fresh reviewer; test counts are the full
`cargo test` total after the task. The branch started from 1164ede (M10c, 501 tests). `cargo fmt --check` clean.

| Task | Commits | Tests | Review |
|---|---|---|---|
| 0 screenshot harness | `54780eb..d559c32` | 501 | Approved, clean. Scenes run from a hidden `__ui-shots` entry; hand-written PNG encoder (no new crate). |
| 1 tokens and contrast | `d559c32..42c8b44` | 507 | Approved, clean. Plain dividers held to 1.4:1; waiting colour to 4.5:1. |
| 2 window chrome and padding | `42c8b44..3e8b3c5` | 511 | Approved, clean. |
| 3 status data | `3e8b3c5..7618ba6` | 514 | Approved, clean. |
| 4 marks and motion | `7618ba6..40d0650` | 518 | Needs fixes (ticker ran while the window was hidden; every tick repainted the terminals). Fix round `42932ed` (+ `195a35f`, lock file); scoped re-review by opus: 3 addressed, 0 open. |
| 5 sidebar | `195a35f..516eeba` (+ `ef78bc2`, the waiting question without its box) | 530 | Approved, clean. |
| 6 splits | `516eeba..c4045fe` | 534 | Needs fixes (a header click on the active pane didn't return the keyboard). Fix round `48166f4`; scoped re-review: 2 addressed, 0 open. |
| 7 getting around | `48166f4..5870777` | 538 | Needs fixes (opening ⌘P over rename or another box closed the new one; notification body repeated the summary; rename selection barely visible in Latte; switcher look). Fix round `c3903d0`; 540 tests; scoped re-review: 4 addressed, 0 open. |
| 8 Preferences | `c3903d0..980c01e` | 544 | Needs fixes (the preview was cut off at 16 pt and above). Fix round `c59f135`; 546 tests; scoped re-review: 2 addressed, 0 open. |
| Final review | `1164ede..c59f135` | 546 | Ready to merge (0 Critical, 0 Important, 7 Minor). Three minors re-graded to a polish pass. |
| Polish pass | `1a4ccd7` (text on hover surfaces at 4.5:1), `e82f8ae` (host picker and address box share ⌘P's panel look), `67ced90` (browser windows swallow the font-size keys) | 546 | Scoped re-review: all addressed. Final review clean. |
| 9 On screen | | | Pending: batched with M10b and M10c. |
| 10 Docs | the docs commit | 546 | This commit. |

Rulings taken along the way are summarised in the milestone note, "Decisions made along the way".

## Deferred minors

Minor review findings left on purpose. Copied here from the (git-ignored) working ledger so they are not lost.

- **Task 0:** the smoke test renders each theme twice; the PNG encoder doesn't guard a zero-height shot.
- **Task 1:** tints aren't asserted distinct from the sidebar; `text_dim` / `text_faint` sit at about 4.50 with no margin and look alike on Dracula, Nord, Gruvbox and Latte; a `ui_tokens` ↔ `theme` module cycle (mix); `Theme::ui()` isn't cached; `readable()` may return a failing colour for an odd user theme (undocumented).
- **Task 2:** `check_window` asserts only right and top padding; key strings such as `cmd--` aren't exercised by a test; `band_pressed` isn't cleared on a release outside the window; nothing marks the drag band when the sidebar is hidden.
- **Task 3:** an adopted, already-Waiting session shows no question until its next change; `.take(120)` counts chars, not graphemes.
- **Task 4:** the main window is found by diffing the window list (could also match class or title); a second `watch_window` would leave the old observer; an unfocused split pane keeps a pointing-hand cursor after ⌘ is released until the mouse moves; a Reduce-motion change only notifies the Workspace (Preferences picks it up on its next redraw).
- **Task 5:** `refresh_ages` / `rows_in_view` hidden gating has no unit test; `sidebar.rs` is large (about 1,000 lines of drawing); the drag preview hides the end of the target row's insertion line.
- **Task 6:** the accent divider's position is pixel-checked only at ratio 0.5 (the maths was verified for all ratios in the final review).
- **Task 7:** the ticker counts every filtered switcher row, not only scrolled-in ones (bounded by the open switcher); the selection falls back to the plain sidebar colour for a pathological user theme; the late-Cancel guard is proven by reading and the on-screen check (the harness doesn't fire blur); the host picker wasn't in the first overlays scene (added in the polish pass); the daemon's own notification text (background service) is unchanged, since it doesn't know GUI numbers or groups.
- **Task 8:** `choice_list` has too many arguments and `font_row` has unused params; an empty area sits under the Preview caption; ligature samples are cropped below 13 columns and the first `ls` entry below 3 columns.
- **Final review:** `done_tint` is never drawn; the Done check doesn't match a hovered row's fill; "3" vs "#3" numbering between the sidebar and the switcher / headers; Preferences' selection fill and system titlebar differ from the main window; `Theme::ui()` is called per render and per switcher row.
- **Polish pass:** ⇧⌘− and numpad zoom keys aren't swallowed in browser windows (unbound, harmless).
- **Parked residual (no second fix wave):** the address-box error text (`ring_waiting`) sits on `panel`, where it is only guaranteed 3:1 on dark themes (4.5:1 is guaranteed on the sidebar). Later: add `ring_waiting` on `panel` / `panel_hover` at 4.5:1 to `drawn_pairs`.
- **Docs note:** the notification title format changed (`#<n> <group or name> — <message>`), and the `fuxx events` notification carries the new title and body.
- **Whole-branch watch item:** if `watch_window` ever matched a hidden helper window, marks would never animate (or never stop): the on-screen check (minimise, then Activity Monitor) will show it.

## Known limitations

- Hover peek, attention queue, Dock badge and overview grid are later ideas.
- Screenshots come from GPUI's off-screen renderer; the real window can differ slightly (fonts, scale) — the
  on-screen check is still the authority.
- Browser windows ignore Window › Minimize / Zoom and ⌘M (their own buttons work).

## Related

- [[53 - Milestone 11 - UI-UX Uplift]] · [[M11 Input - UI-UX Review]] · [[40 - Roadmap]] · [[00 - Plan Tracker (Start Here)]]
