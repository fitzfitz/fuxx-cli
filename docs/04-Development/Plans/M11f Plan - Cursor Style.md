---
tags: [development, plan, milestone]
milestone: M11f
status: built
updated: 2026-11-08
---

# M11f Plan — Cursor style (plus M11e's package icon)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

| At a glance | |
|---|---|
| **Milestone note** | [[58 - Milestone 11f - Cursor Style]] (the spec); the icon: [[57 - Milestone 11e - Update Available]] "Amendment" |
| **Status** | 🟨 Tasks 1–3 built and reviewed 2026-11-08 (868 tests); merge, release and on-screen check next |
| **Tasks done** | 4 / 6 (Tasks 1–4); v0.3.2 released; Task 5's on-screen part pending |
| **▶ Pick up here** | Task 5: update to v0.3.2 with the button, then note 58's checklist |
| **Decisions used** | Yours (2026-11-08): a package icon and the target version on the Update Available button; the cursor mix "Bar + glide + breathing glow + colour following the agent, with Trail and Sparks as Fun" ("ya pakai cara mu"). Details: AI's call, in note 58. |
| **Needs your eyes** | 👀 Task 5 (release v0.3.2 on your word, then on screen) |

**Goal:** a package icon on the Update Available button, and a bar cursor that glides, breathes, follows the agent's
status colour, with optional trail and sparks, all set in Preferences and still under motion settings.

**Architecture:** Every terminal (local or in the background service) starts with the marker style
`CursorStyle { shape: HollowBlock, blinking: true }` as its default. `HollowBlock` can't be requested by a program,
so it means "no program has chosen", and the window draws the user's `cursor-style` in its place. A new pure module
`cursor_look.rs` holds every rule: shape, colour, breathing opacity, glide position, trail and sparks. `TerminalView`
keeps a small `CursorMotion` state (last cell, glide start, last change, sparks), paints from it, and drives frames:
`request_animation_frame` during a glide, trail or sparks, and a 33 ms timer for breathing.

**Tech Stack:** GPUI at the pinned rev, `alacritty_terminal` 0.26 (`term::Config::default_cursor_style`,
`CursorShape`, `CursorStyle`), the existing status colours (`UiColors.ring_*`) and motion settings
(`App::reduce_motion`, `Appearance.animations`).

**Spec:** [[58 - Milestone 11f - Cursor Style]]

## Global Constraints

- **No new crates.** No change to the scripting protocol or the background service's wire format.
- Shape sizes: bar **2 px** wide at the cell's left edge, full height; underline **2 px**; hollow **1 px** outline;
  block = the inverted cell (as today). An unfocused terminal: a 1 px outline in the theme's cursor colour, still.
- Colours: Working `ring_working`, Waiting `ring_waiting`, Done `ring_done`, Idle the theme's cursor colour
  (`theme.cursor.unwrap_or(theme.foreground)`). A block cursor in a status colour draws its letter in the theme's
  background colour.
- Breathing: opacity **1.0 → 0.35 → 1.0** over **1.6 s** (cosine ease), glow **6 px** blur at **30%** of the
  cursor's colour. It holds at 1.0 for **0.6 s** after the cursor or its text last changed. It redraws at most every
  **33 ms**, only for the focused terminal of the key window. It applies only when the effective style is blinking.
- Glide **90 ms** ease-out (cubic). No glide for a jump of more than **20 rows** or a screen switch (alternate screen,
  clear, resize).
- Trail: **5** copies, shrinking and fading, gone **150 ms** after the glide ends; only for glides longer than 1 cell.
  Sparks: **6** dots of **2 px**, flying **6–14 px**, fading over **350 ms**, only on keys the user types that send
  text.
- With `Appearance.animations` off or `App::reduce_motion`: no glide, breathing, trail or sparks (still and solid);
  the colour still follows the status.
- Settings lines (only non-defaults written): `cursor-style = underline|hollow|block` (default `bar`),
  `cursor-glide = off`, `cursor-glow = off`, `cursor-color = theme` (default `status`), `cursor-fun = trail|sparks`
  (default `off`). Preferences → Appearance → "Cursor": Style (Bar/Underline/Hollow/Block), Glide, Breathing glow,
  Colour follows the agent, Fun (Off/Trail/Sparks), and the note
  `Programs that set their own cursor (vim, for example) still choose their shape.`
- Icon: a line-drawn package (an isometric box with a tape line) in a **16 px** box, white, **6 px** left of the text,
  the pair centred in the button. The button text is `Update Available · v{version}` (the ` · v{version}` part in
  white at **75%** opacity); while downloading, `Downloading v{version}…`. When the button is too narrow, the version
  part is dropped first; the icon and `Update Available` always show.
- Subagents never `cargo run` the app. Allowed: `cargo test`, `cargo clippy --all-targets` (both feature sets),
  `cargo fmt --check`, `cargo run --features ui-shots -- __ui-shots [filter]`, `scripts/bundle-mac.sh --app-only`.

## Review Focus

1. **A program that sets its own cursor** (vim's insert bar, `ESC [ 2 q` steady block), then resets it with
   `ESC [ 0 q`. Expected: the program's shape while set; steady means no breathing; after the reset, the user's
   setting again. (Task 2: `effective_style` tests over the marker, each DECSCUSR shape and blinking.)
2. **Heavy output** (`yes`, a build log) moving the cursor every frame. Expected: no queue of glides, the cursor drawn
   near its true place, breathing held at full, no runaway CPU. (Task 2: `glide_from_now` restarts from the drawn
   position; Task 3: the ui-shot drives 50 moves in one frame and checks a single active glide.)
3. **fuxx in the background or the terminal unfocused.** Expected: no frames requested at all (no breathing timer),
   and a still outline. (Task 3: a `needs_frames` test over focused, key window and animations.)
4. **Restoring a session from the background service, or an older service** whose terminals started with the old
   default (steady Block). Expected: no crash; such a terminal shows a steady block until its program resets the
   cursor; new terminals get the marker. (Task 1: restore/snapshot tests keep a marker or a plain style intact.)
5. **Light themes and the status colours.** Expected: the bar is visible on every built-in theme's background, and a
   status-coloured block keeps its letter readable. (Task 3: ui-shots in Catppuccin Mocha and Latte for each shape and
   status.)

---

### Task 1: Settings, the "no program has chosen" marker, Preferences, and the package icon — ✅

**Files:**
- Modify: `src/appearance.rs`, `src/prefs.rs`, `src/preferences.rs` (a "Cursor" group in Appearance);
  `src/daemon_session.rs` (every `Term::new` gets the marker as `default_cursor_style`); `src/vt_snapshot.rs` (the
  marker writes no DECSCUSR, as HollowBlock already doesn't); `src/tab_bar.rs` (`Glyph::Package`); `src/sidebar.rs`
  (the icon in the button); `src/ui_shots.rs`.

**Interfaces:**
- Produces: `pub enum CursorShapeSetting { Bar, Underline, Hollow, Block }`; `pub enum CursorFun { Off, Trail, Sparks }`;
  the `Appearance` fields `cursor_style: CursorShapeSetting`, `cursor_glide: bool`, `cursor_glow: bool`,
  `cursor_follows_status: bool`, `cursor_fun: CursorFun`, parsed with warnings like `browser_keep_data_from_entries`;
  `prefs::cursor_*_values` in the same style; `pub const CURSOR_UNSET: CursorStyle = CursorStyle { shape: CursorShape::HollowBlock, blinking: true }`
  (in `daemon_session.rs` or a shared spot both the daemon and the window import).

- [x] **Step 1: failing tests**
  - Appearance parsing: defaults (bar, on, on, status, off); each non-default value; junk → the default plus a warning.
  - `prefs::cursor_style_values(Bar) == []`, `(Hollow) == ["hollow"]`; the same pattern for the others.
  - A new `Term` from `daemon_session` reports `cursor_style() == CURSOR_UNSET`, and after feeding `ESC [ 6 q` →
    Beam steady, after `ESC [ 0 q` → `CURSOR_UNSET` again.
  - The vt snapshot of a terminal with the marker restores to a terminal with the marker.
  - The help-menu-style test for the Preferences Appearance section lists the Cursor rows in order.
  - ui-shot `update-button` (both themes): the button now has the package glyph left of the text; assert the glyph's
    box is 16 px and the gap is 6 px; the text reads `Update Available · v0.3.2` for a 0.3.2 release; Downloading
    reads `Downloading v0.3.2…`; at the narrowest sidebar width the version part is gone and `Update Available`
    still shows in full.
  - A pure `button_label(state, width_fits_version: bool)` (or similar) test for the three texts.
- [x] **Step 2: run** → FAIL.  **Step 3: implement.**  **Step 4: run** `cargo test`, clippy (both), fmt,
  `__ui-shots preferences` and `update-button` → ok. Look at the PNGs.
- [x] **Step 5: commit** `feat(m11f): cursor settings and the "no program has chosen" marker; M11e: a package icon on the update button`.

### Task 2: The cursor's rules (pure) — ✅

**Files:** Create `src/cursor_look.rs`; Modify `src/main.rs` (`mod`).

**Interfaces:**
- Produces:
  - `pub fn effective_style(term_style: CursorStyle, setting: CursorShapeSetting) -> CursorStyle` (the marker → the
    setting's shape, blinking; anything else → unchanged).
  - `pub fn cursor_color(status: Status, follows: bool, ui: &UiColors, theme: &Theme) -> Rgb`.
  - `pub fn breath_alpha(since_change: Duration, clock: Duration) -> f32` (1.0 while `since_change < 600 ms`, else the
    1.6 s cosine between 1.0 and 0.35).
  - `pub struct Glide { from: (f32, f32), to: (usize, usize), start: Instant }` with
    `pub fn position(&self, now: Instant) -> (f32, f32)` (in cells, 90 ms cubic ease-out) and
    `pub fn glide_from_now(current: Option<&Glide>, drawn: (f32, f32), to: (usize, usize), rows_moved: usize, screen_changed: bool, now: Instant) -> Option<Glide>`
    (None = jump).
  - `pub fn trail(glide: &Glide, now: Instant) -> Vec<(f32, f32, f32 /*scale*/, f32 /*alpha*/)>` (5 items while
    active, empty after 150 ms past the end, empty for ≤ 1 cell).
  - `pub struct Spark { origin: (f32, f32), angle: f32, reach: f32, start: Instant }` and
    `pub fn sparks_for_key(seed: u64, origin: (f32, f32), now: Instant) -> [Spark; 6]` (deterministic from `seed`;
    angles spread around the circle; reach in 6–14 px) plus `pub fn spark_at(s: &Spark, now: Instant) -> Option<((f32, f32), f32)>`
    (position and alpha; None after 350 ms).
  - `pub fn needs_frames(focused: bool, key_window: bool, motion: bool, glowing: bool, animating: bool) -> Frames`
    with `pub enum Frames { None, Timer33ms, EveryFrame }`.
- [x] **Step 1: failing tests**, one or more per function:
  - `effective_style` over the marker × each setting and over Beam/Underline/Block × blinking.
  - `cursor_color` for each status with `follows` true and false.
  - `breath_alpha`: 1.0 at 0.3 s since a change; 0.35 at the trough; 1.0 at the period.
  - `Glide::position`: at 0, 45 and 90 ms.
  - `glide_from_now`: a 25-row jump → None; a screen change → None; mid-glide → starts from the drawn point.
  - `trail` counts.
  - `sparks_for_key`: deterministic, 6 distinct angles, reach in range; `spark_at` is None after 350 ms.
  - `needs_frames`: unfocused or non-key → None; motion off → None; glowing only → Timer33ms; animating →
    EveryFrame.
- [x] **Step 2: run** → FAIL.  **Step 3: implement.**  **Step 4: run** `cargo test`, clippy (both), fmt → ok.
- [x] **Step 5: commit** `feat(m11f): the cursor's rules — shape, colour, breathing, glide, trail, sparks (pure)`.

### Task 3: Drawing and driving the cursor — ✅

**Files:** Modify `src/terminal_view.rs` (`CursorMotion`, painting, frames, sparks on typed keys, the status passed
in), `src/workspace.rs` (giving each `TerminalView` its session's status when it changes), `src/ui_shots.rs` (scene
`cursor`).

**Interfaces:**
- Consumes: Task 1's settings and `CURSOR_UNSET`; every Task 2 function.
- Produces: `TerminalView::set_status(&mut self, status: Status, cx)` (redraws only on a change); `CursorMotion`
  (private): last cell, last screen, the current `Glide`, last change, live sparks, and a spark seed counter.
- Painting order: the trail (under), the glow (a shadowed quad), the cursor shape at `Glide::position` (in cells →
  px) with `breath_alpha`, then the sparks.
- Frames: per `needs_frames`. Use `window.request_animation_frame()` for EveryFrame, and a 33 ms
  `cx.background_executor().timer` loop for Timer33ms. The loop stops when `needs_frames` is None (unfocused, the
  window not key, motion off).
- [x] **Step 1: failing tests (ui-shot `cursor`, Mocha and Latte)**:
  - Each shape (bar, underline, hollow, block) with the setting and no program style; assert the bar's quad is 2 px
    wide at the cell's left edge.
  - A Working terminal draws the cursor in `ring_working`; Idle draws it in the theme's cursor colour.
  - With a program's `ESC [ 2 q`: a steady block, no glow quad.
  - An unfocused pane: a 1 px outline, and `needs_frames == None`.
  - 50 cursor moves in one frame leave one glide.
  - With `reduce_motion`: no glow, no trail, no sparks, and the cursor exactly on its cell.
- [x] **Step 2: run** `__ui-shots cursor` → FAIL.  **Step 3: implement.**  **Step 4: run** `cargo test`, clippy
  (both), fmt, the full `__ui-shots`. Look at the PNGs.
- [x] **Step 5: commit** `feat(m11f): a bar cursor that glides, breathes and follows the agent; trail and sparks`.

### Task 4: Merge and push (your word) — ✅ (2026-11-08)

After the whole-branch review: merge to `master` and push it, both on your word.

### Task 5: Release v0.3.2 and the on-screen check — 👀 (your word, then you)

- [x] On your word: version 0.3.2, commit, push, tag `v0.3.2`, and watch the Release run. (2026-11-08, green)
- [ ] You: fuxx 0.3.1 shows **Update Available**, now with the package icon (note 57's amendment). Update and
  Restart brings 0.3.2. This also covers note 57's items 5–8.
- [ ] Note 58's checklist, items 1–10.

### Task 6: Docs

- [ ] Note 58 (status, anything changed while building). Note 57 (the icon amendment's result). This plan (ticks).
  `40 - Roadmap.md`. The Plan Tracker. `60 - Progress Log.md`. `STRUCTURE.md` (`cursor_look.rs`). `README.md` (a line
  on the cursor and its settings).
- [ ] Commit `docs(m11f): …`.
