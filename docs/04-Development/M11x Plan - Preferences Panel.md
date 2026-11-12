# M11x Preferences Panel Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Preferences opens as a rounded panel over the dimmed fuxx window, each section a page of grouped cards, plus a Banner sound that plays with every banner fuxx posts (the window's and, after quit, the background service's).

**Architecture:** `PreferencesView` stops being a window's root view and becomes a workspace overlay (`Overlay::Preferences`, like the ⌘K palette), drawn over a scrim like the Welcome card. The pages are laid out from one table (section → page title, lede, groups → rows), which the ⌘K palette's settings list reads too. Banner sound is a third `SoundKey`; the window plays it through the existing `Sounds` global, the background service through `afplay` on the file the settings name.

**Tech Stack:** Rust, GPUI (pinned rev, unchanged), `NSSound` (existing), macOS `afplay` (built in). No new crate.

**Spec:** [[75 - Milestone 11x - Preferences Panel]]

## Global Constraints

- No new library (note 75, "How").
- The settings code is unchanged: every click still writes one line of the settings file through `preferences::apply` and reloads (`prefs`, `settings`, `appearance`).
- Panel 880 × 580 pt, at least 24 pt from each window edge; corners 22 pt; nav 200 pt; page title 26 pt semibold, lede 14 pt faint, group name 15 pt semibold, card corners 14 pt, row name 14 pt, explanation 12.5 pt faint.
- Copy (page ledes, group names, row explanations) exactly as note 75's "The pages".
- Banner sound keys: `sound-banner`, `sound-banner-on` (on by default). Plays only when fuxx posts a banner.
- Colours from `theme.ui()` only (Latte gives a light panel); no hard-coded colours.
- No commit until you say so; work on branch `m11x-preferences-panel`.

## Review Focus

1. **Esc in a sound field** puts back the saved link and keeps the panel open; Esc with no field focused closes the panel. (Task 4 test `escape_in_a_field_keeps_the_panel`, ui-shot step.)
2. **⌘W while the panel is open** closes the panel and never the terminal behind it. (Task 6 keys scene: session count unchanged.)
3. **A browser page in a pane** (a native view drawn over GPUI) hides while the panel is open. (`Overlay::Preferences` in `Overlay::ALL`; the `overlays` scene opens it.)
4. **A window smaller than the panel:** the panel shrinks to the window less 24 pt each side, the nav stays, the page scrolls. (Task 4 `panel_size` tests; Task 6 `preferences-small` scene.)
5. **Banner sound in the background service** with no settings file, a blank or switched-off link, or a web link not downloaded yet: silent, no error. (Task 2 `banner_file` tests.)

---

### Task 0: Branch

- [ ] `git checkout -b m11x-preferences-panel` (from master `badf6a4` or later).

### Task 1: Banner sound in the window

**Files:** Modify `src/sounds.rs`, `src/appearance.rs`, `src/workspace.rs` (~2151), `src/workspace_api.rs` (~394).

**Interfaces:**
- Produces: `SoundKey::Banner`; `SoundKey::ALL: [SoundKey; 3]` (Enter, CtrlC, Banner); `Appearance::sound_links: [String; 3]`, `sound_on: [bool; 3]`; `sounds::banner(cx: &App)` — plays Banner's sound when it's switched on.

- [ ] **Step 1: Failing tests** in `appearance.rs`: `sound_on_from_entries(&[])` → `[true, true, true]`; `sound_on_from_entries(&[e("sound-banner-on","off")])` → `[true, true, false]`; `sound_links_from_entries(&[e("sound-banner","~/c.wav")])[2] == "~/c.wav"`. In `sounds.rs`: `SoundKey::Banner.key() == "sound-banner"`, `.switch_key() == "sound-banner-on"`, `.label() == "Banner sound"`, and `key_for_bytes` never returns `Banner`.
- [ ] **Step 2:** `cargo test sound` → FAIL (no `Banner`).
- [ ] **Step 3:** Add the variant, widen the arrays (use `SoundKey::ALL.len()`), add the third `Sounds` slot, add `pub fn banner(cx: &App)` (`if sound_on[Banner] { play(Banner, cx) }`). Call `sounds::banner(cx)` beside each `notifier::fire` in `workspace.rs` (inside `if worth_a_banner`) and `workspace_api.rs`, on the UI thread, before the background spawn.
- [ ] **Step 4:** `cargo test` → PASS.

### Task 2: Banner sound in the background service

**Files:** Modify `src/sounds.rs`, `src/daemon.rs` (~777).

**Interfaces:**
- Produces: `sounds::banner_file(entries: &[settings::Entry], home: &Path, exists: &dyn Fn(&Path) -> bool) -> Option<PathBuf>`; `sounds::afplay_args(file: &Path) -> Vec<OsString>`; `sounds::play_banner_detached()` (reads the settings file via `settings::settings_path` + `settings::parse`, then spawns `afplay`; never waits, never logs a missing file).

- [ ] **Step 1: Failing tests** (`banner_file`): no entries → `None`; `sound-banner ~/a.wav` → `Some(home/a.wav)`; with `sound-banner-on off` → `None`; a web link whose `cache_file(cache_dir(home), url)` `exists` → that path; the same link not existing → `None`; blank link → `None`; the last `sound-banner` line wins. `afplay_args("/x.wav") == ["/x.wav"]`.
- [ ] **Step 2:** `cargo test banner_file` → FAIL.
- [ ] **Step 3:** Implement; in `daemon.rs`'s spawned banner thread, after `notifier::fire`, call `sounds::play_banner_detached()`. The service only posts while no window is attached (`daemon_should_notify`), so a banner's sound plays on one side only.
- [ ] **Step 4:** `cargo test` → PASS.

### Task 3: The pages as a table

**Files:** Modify `src/preferences.rs`.

**Interfaces:**
- Produces: `struct Group { title: &'static str, rows: &'static [Control] }`; `Section::lede(self) -> &'static str`; `Section::groups(self) -> &'static [Group]`; `Control::explanation(self) -> &'static str`; `Control::Sound(SoundKey::Banner)` in Sound's "Notifications" group. `Control::CursorHeading` is removed (Cursor is a group). `palette_settings()` walks `groups()`. Palette names: Banner sound → `"Banner sound"`, keywords `["sound-banner", "notification", "banner"]`.

- [ ] **Step 1: Failing tests:** `pages_follow_note_75` asserts every section's lede, its groups' titles in order, each group's rows in order and every row's explanation, copied exactly from note 75 (General: Interface [Animations, Status Bar, Tips card], Updates [Check for updates]; Appearance: Theme [Theme], Text [Font, Size, Ligatures], Cursor [Style, Glide, Breathing glow, Colour follows the agent, Fun]; Sessions: Quitting and reopening [Restore sessions, Keep running after quit], Terminal [Command blocks]; Browser: Browsing [Show address bar, Keep history and data]; Sound: Keys [Enter, Ctrl + C], Notifications [Banner sound]). Update `sections_list_every_control_once` and `palette_settings_cover_every_control` to walk groups; add a palette test that "Banner sound" opens on Sound.
- [ ] **Step 2:** `cargo test preferences` → FAIL.
- [ ] **Step 3:** Implement; drop `controls()`, `note()`, the height and label-height functions and the constants only they used once Task 5 no longer needs them.
- [ ] **Step 4:** `cargo test` → PASS.

### Task 4: The panel over the window

**Files:** Modify `src/preferences.rs`, `src/workspace.rs`, `src/main.rs` (~862), `src/palette.rs` (~1175), `src/sidebar.rs` (~2557), `src/title_area.rs` (~592).

**Interfaces:**
- Produces: `Overlay::Preferences`; `Workspace::preferences: Option<Entity<PreferencesView>>`; `Workspace::toggle_preferences(&mut self, window, cx)`; `Workspace::open_preferences_at(&mut self, section: Section, window, cx)`; `PreferencesView::new(workspace: WeakEntity<Workspace>, window, cx)` emitting `gpui::DismissEvent`; `preferences::panel_size(window_w: f32, window_h: f32) -> (f32, f32)`; app-level `preferences::open_or_activate(cx)` / `open_at(section, cx)` keep their names and now route to the active (else first) workspace window.
- Removes: `PreferencesWindow`, `window_size`, the `WindowOptions` code.

- [ ] **Step 1: Failing tests:** `panel_size(1400, 900) == (880, 580)`; `panel_size(700, 500) == (652, 452)`; `panel_size(30, 20)` never negative (floor at 0). `escape_in_a_field_keeps_the_panel`: the pure key rule `dismiss_on_escape(field_focused: bool) -> bool` is `false` when a field has focus, `true` otherwise.
- [ ] **Step 2:** `cargo test panel_size escape_in` → FAIL.
- [ ] **Step 3:** Implement. Opening: `close_overlays(Overlay::Preferences)`, create the view, subscribe to its `DismissEvent` (→ `preferences = None`, `focus_active`), focus it. ⌘, (OpenPreferences) toggles; `open_at` opens or switches section. Dismiss on ✕, Esc (per `dismiss_on_escape`), a mouse-down on the scrim, `CloseSession` (⌘W, handled in the view's key context and stopped there). Draw: a scrim over the whole window in `ui.shadow` at the Welcome card's `SCRIM_ALPHA`, `.occlude()`, the panel centred at `panel_size`, after the other overlays in `Workspace::render`. The tips card's workspace is the one passed in (no window search). The section shown is still the run's last (`LastSection`).
- [ ] **Step 4:** `cargo test` and `cargo clippy` → PASS / no new warnings.

### Task 5: The new look

**Files:** Modify `src/preferences.rs` (split rendering helpers into `src/preferences_ui.rs` if it passes ~2000 lines).

**Interfaces:** consumes Task 3's table and Task 4's view.

- [ ] **Step 1:** The nav (200 pt, `ui.sidebar_background`, hairline right): "Settings" faint 12 pt, section rows 32 pt with 9 pt corners and a soft fill on the current one, `fuxx <CARGO_PKG_VERSION>` faint at the foot.
- [ ] **Step 2:** The page: an `overflow_y_scroll` column (back to top on a section change) with title, lede, then each group's name over a card (`ui.card` fill, `ui.divider` border and hairlines between rows); a row = name + explanation left, control right; the ✕ (28 pt) pinned at the panel's top right.
- [ ] **Step 3:** Controls: a switch (38 × 22 pt, knob 16 pt, accent fill when on) for every `OnOff` and Ligatures; segmented buttons for Restore, Cursor style, Cursor fun; the Size stepper; the Show again pill; sound rows = switch, link field, ▶, problem line under.
- [ ] **Step 4:** Appearance: the preview across the top (the existing `PreviewScreen` / `paint_fixed_screen`, full page width, caption "Preview — changes as you pick."); theme tiles four to a row (the theme's background, a text-colour line, five swatch colours, the name; the current one outlined in the accent with ✓); the Font dropdown (a pill showing the current font; its list drawn above the page with `deferred`, at most 8 rows visible then scrolling, each name in its own face, Default first; closes on a pick, Esc or a click elsewhere).
- [ ] **Step 5:** `cargo test`, `cargo clippy`, `cargo fmt --check` → PASS.

### Task 6: Screenshots

**Files:** Modify `src/ui_shots.rs`.

- [ ] **Step 1:** Rewrite `preferences_scenes` as workspace scenes with the panel open: each section in Mocha and Latte, asserting the scrim's dim outside the panel, the panel's background inside it, the nav's current-row fill, a switch's accent fill (on) and grey (off), the preview's area on Appearance, the Banner sound row on Sound.
- [ ] **Step 2:** `preferences-small` (window 760 × 520): the panel's edges 24 pt in; the page scrolled to its foot shows the last row.
- [ ] **Step 3:** Keys scene: Esc closes; ⌘W closes and the session count is unchanged; typing while open leaves the terminal's text unchanged.
- [ ] **Step 4:** Update `palette-settings` (a setting opens the panel on its section) and `overlays` (opens `Overlay::Preferences`).
- [ ] **Step 5:** Build with `--features ui-shots`, copy the binary, run all scenes → all pass (title-bar as on master).

### Task 8: Wrap in the file tab (added 2026-11-12 on your ask: "beside File | Diff, on the left, add 1 more toggle called Wrap. 1 button but it shows like toggle button contained if wrap true, button outlined when wrap is false")

**Files:** Modify `src/file_tab.rs`, `src/appearance.rs` (the `file-wrap` key).

**Interfaces:**
- Produces: `Appearance::file_wrap: bool` (`file-wrap`, off by default); `file_tab::visual_rows(widths: &[usize], columns: usize, wrap: bool) -> Vec<(usize, usize)>` — each screen row as (line, first cell); without wrap one row per line.

- [ ] **Step 1: Failing tests:** `visual_rows(&[3, 10, 0], 4, true) == [(0,0),(1,0),(1,4),(1,8),(2,0)]`; `visual_rows(&[3,10], 4, false) == [(0,0),(1,0)]`; `columns == 0` treated as 1; a wide (2-cell) character never split across rows (rows break before it); `file-wrap on` parses to true, missing → false.
- [ ] **Step 2:** `cargo test visual_rows` → FAIL.
- [ ] **Step 3:** The button: `Wrap`, left of the File | Diff pair, 24 pt tall like them; filled (the pair's current-option fill) when on, outlined (1 pt `ui.divider`) when off; a click writes `file-wrap` through `preferences::apply`. With wrap on: no horizontal scroll; rows from `visual_rows` at the text area's column count; the number, change bar and sign on a line's first row only; selection, Find's reveal and the diff's rows map through (line, cell) ↔ row.
- [ ] **Step 4:** `cargo test` → PASS; ui-shot `file-wrap` (a long line wraps; the button filled) and `file-nowrap` (button outlined).

### Task 7: Docs and final review

- [ ] Note 75 status and "As built"; Roadmap line; Progress Log entry (and snapshot); a fresh whole-branch review on the most capable model; fix Critical/Important with tests; then your on-screen check (note 75's seven items) and commit on your word.
