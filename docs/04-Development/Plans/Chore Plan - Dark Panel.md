# Dark Panel Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans (the user's standing build mode) to
> implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** the floating panel (panes, terminal) is the darker surface and the window (canvas, sidebar) the lighter
one, in every theme, with everything derived following.

**Architecture:** one switch where a theme is settled: `theme::resolve` ends with `Theme::with_dark_panel`, which
moves the window colour (the theme's `chrome-canvas`, else the derived one) into `background` and the theme's own
background into `chrome.canvas`. Everything reading `background` or `ui()` follows with no other edits; only the
composer's inner box changes source.

**Tech Stack:** Rust, GPUI; `src/theme.rs`.

**Spec:** [[88 - Milestone 11ab - Chat-Style Shell Tabs]] "Chore design: the panel darker, the window lighter".

## Global Constraints

- Built-in canvases (the new panels): Mocha `#11111b`, Latte `#dce0e8`, Nord `#242933`, Dracula `#191a21`,
  Gruvbox `#1d2021`, TokyoNight `#15161e`.
- Derived panel for a theme without `chrome-canvas`: dark `mix(bg, black, 0.42)`, light `mix(bg, fg, 0.08)`.
- ANSI palette, cursor and selection entries unchanged. Text and marks at their existing floors (`TEXT_MIN`,
  `MARK_MIN`) in every theme.
- Gate: `cargo test --bin fuxx`, both clippy runs at 14/27 and 16/29, `cargo fmt --check`, full `scripts/ui-shots.sh`
  (6 known display-dependent failures).

## Review Focus

1. A config `background = …` line over a theme: it is switched too (it is the theme's background as written).
2. The app with no `theme` line (`Theme::default()` through `resolve`): switched like the rest.
3. The screenshot harness builds themes with `apply`, not `resolve`: it must switch too or the pictures lie.
4. `with_dark_panel` applied twice would switch back: only `resolve` (and the harness) call it.
5. Program-painted backgrounds (palette colours) on the darker panel: unchanged colours, judged on screen.

---

### Task 1: The switch

**Files:** Modify `src/theme.rs` (`resolve`, `ui_uncached`'s canvas, `every_theme`'s default, `composer_inner`,
tests); `src/ui_shots/harness.rs` (`shot_theme`).

**Interfaces:** Produces `pub fn with_dark_panel(self) -> Theme` and `fn window_canvas(&self) -> Rgb` (the
pinned-or-derived canvas, today's expression lifted out of `ui_uncached`).

- [ ] **Step 1: failing tests** in `theme.rs`:
  - `the_panel_is_darker_than_the_window`: for every theme, `luminance(theme.background) < luminance(ui.sidebar_background)`.
  - `mocha_and_latte_switch_their_two_colours`: Mocha `background == #11111b`, `ui.sidebar_background == #1e1e2e`;
    Latte `#dce0e8` / `#eff1f5`.
  - `a_theme_without_a_canvas_derives_its_panel`: `resolve` of `background = #202020` (no theme) gives
    `background == mix(#202020, black, 0.42)` and `ui.sidebar_background == #202020`.
  - `the_composer_inner_box_is_the_panel` (replaces `…_is_the_sidebar_colour`): `ui.composer_inner == theme.background`.
- [ ] **Step 2:** run, expect FAIL (no switch yet).
- [ ] **Step 3:** implement `window_canvas`, `with_dark_panel`, call it at `resolve`'s end; `every_theme`'s default via
  `resolve(&[], true, …)`; `shot_theme` calls `with_dark_panel`; `composer_inner: card`; comments say the new rule.
- [ ] **Step 4:** full suite; pinned text values (`dim_and_faint_text_keep_todays_values`) move: update them to the
  new values only if every floor test passes (ledger a ruling).
- [ ] **Step 5:** commit.

### Task 2: Look, gate, docs

- [ ] Full gate; `scripts/ui-shots.sh shell-cards sidebar --png --all-themes` and look in Mocha and Latte.
- [ ] Note 88 "as built" lines and Progress Log entry; commit; show the user the pictures. Merge and push on their word.
