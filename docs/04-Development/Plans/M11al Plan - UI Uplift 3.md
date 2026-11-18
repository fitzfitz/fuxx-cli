---
tags: [development, plan, milestone]
milestone: M11al
status: planned
updated: 2026-11-18
---

# M11al Plan — UI Uplift 3: one floating card, roomier sections

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

| At a glance | |
|---|---|
| **Design (the spec)** | [[84 - Milestone 11al - UI Uplift 3]] |
| **Status** | 📝 planned 2026-11-18 on branch `m11al-ui-uplift` (from `cc3b2b8`); building, subagent-driven |
| **Tasks done** | 1 / 13 (Task 0, baselines) |
| **Baselines** | Task 0, on `cc3b2b8`: `cargo test --bin fuxx` 1,944 passed, 1 ignored; clippy default 14 (bin) / 27 (test), `ui-shots` 16 / 29; `cargo fmt --check` clean; full `scripts/ui-shots.sh` 191 passed, 0 failed, 144.1 s |
| **▶ Pick up here** | Task 1 |
| **Decisions used** | Yours: option 1 (one card), tab style E, "looks right, do it right after M11ak", "go ahead with M11al". The design's AI's calls and "Calls on the drafter's open questions". Plan-time calls below (AI's). |
| **Needs your eyes** | 👀 the on-screen check in note 84, after Task 12 |

**Goal:** Every pane and the right panel sit in one floating card beside an attached sidebar on the darkest
background, with minimal tabs and roomier ("comfortable") sizes everywhere — same features, same positions.

**Architecture:** Colours stay derived in `Theme::ui_uncached` (`src/theme.rs`); new surfaces (`raised`, `side`,
`side_hover`, `side_selected`, `tab_active`) join its contrast lists and `drawn_pairs`. Sizes are edited in place
(design approach C): the card's numbers in `ui_tokens::size`, each file's local constants local. The Workspace
draws one card element (`panes::card` reworked: fill, `CARD_RADIUS` 14, edge, a soft shadow) around the pane
layout and, when shown, a 1 px hairline and the right panel; panes inside stop being cards and splits become
hairlines. The cached chrome parts (M11p) keep their definite sizes from the tokens.

**Tech Stack:** Rust 2024, GPUI (pinned git rev, unchanged). No new crates. Per-corner radius (`rounded_tr` /
`rounded_br`) and `BoxShadow` are GPUI `Styled` APIs; check Context7 (or the pinned GPUI source) before using any
GPUI API this repo doesn't already use.

**Spec:** [[84 - Milestone 11al - UI Uplift 3]] (binding, incl. "Calls on the drafter's open questions").
Mockups (git-ignored, for the look only): `.superpowers/brainstorm/34352-1794905233/content/combined.html`.

## Plan-time calls (AI's, made while reading the code)

1. **Waiting rows keep "Needs input" / the question.** The design's "Calls" line says a waiting agent's line says
   "needs you"; §2.4 says Waiting's precedence and words are unchanged, and "No feature changes" holds. §2.4 wins:
   the words stay as today.
2. **`text_dim` / `text_faint` are pinned by test** to today's values in Catppuccin Mocha and Catppuccin Latte
   (hex recorded from master in Task 1's test, test-only). In other themes they may move only where a new surface's
   floor forces it; the test checks floors there, not equality.
3. **New tokens' formulas:** `raised = mix(canvas, card, 0.5)`; `side = mix(card, canvas, 0.33)`;
   `side_hover = mix(side, fg, 0.06)`; `side_selected` = the `row_selected` tint formula applied to `side`;
   `tab_active` = the accent over `card` at 10 % (dark) / 8 % (light), lowered in 2 % steps while `text`,
   `text_dim`, `text_faint` (4.5:1) or the tab marks (3:1) would fall below their floors. `drop_tab` re-bases on
   `card` (`wash(card, 0.20, …)`); `tab_bar`, `tab_hover`, `TAB_BAR_MIN`, `TAB_BAR_LIGHT_CAP`,
   `TAB_BAR_LOW_MIN` and their tests retire in Task 5 once no pane bar reads them.
4. **The card's shadow escapes by moving the clip:** the pane column's `overflow_hidden` goes; the root row clips
   (the window edge), so the shadow falls into the 8 px under the card and over the status bar, which has no fill.
5. **Split grab strip on a 1 px hairline:** `grab_shift(ratio, line, grab) = -line * ratio - (grab - line) / 2`
   (the strip's left edge from `relative(ratio)`), `accent_offset(grab) = (grab - DIVIDER_ACTIVE) / 2`. `GRAB` stays
   6.
6. **Line two's *who*:** `Who::Agent { glyph, name }` from the provider (`ProviderInfo`), `Who::Program(name)` from
   `procs::ProgramState::Running(name)`, else `Who::Shell(name)`: the file name of the program the terminal was
   started with (`$SHELL` when it ran the default shell), else `"shell"`. *Where*: the last path part of `here`
   (else `folder`); an SSH row: its host. Rendered `"<who> · <where>"` (`·` is U+00B7 with spaces), or `"<who>"`
   alone when there is no where.
7. **Scenes derive sizes from tokens:** every rewritten check reads `SIDEBAR_DEFAULT_WIDTH`, `CARD_INSET`,
   `STATUS_BAR`, `TAB_BAR`, `TOP_BAND`, etc., instead of `240` / `6` / `22` / `30`. `ui_shots::GAP` goes with
   `CARD_GAP`.
8. **The status bar's text is 12 px** (§7's number), not `text::META` (12.5 after §11): a local constant in
   `status_bar.rs`.
9. **Task 0 (baselines) was run by the coordinator** before the plan was committed (numbers above). Idle CPU needs
   a real window: it is an item of your on-screen check, against master.

## Global Constraints

- Branch `m11al-ui-uplift`, main checkout. **Never touch `.worktrees/`** (M11ah and M11ai live there). Commit per
  task; no merge, no push in this milestone's build.
- **Gate per task:** `cargo test --bin fuxx && cargo clippy --all-targets && cargo clippy --all-targets --features
  ui-shots && cargo fmt --check` (no new warnings over Task 0's baselines), then `scripts/ui-shots.sh --changed
  master`, and the full `scripts/ui-shots.sh` before the commit of any task that touches drawing. Re-run a failing
  scene alone once before treating it as real. Reference fingerprints are stale on this Mac ("looks different" is
  reported, never a failure) — **never `--update-refs`**.
- **Existing scenes whose expected pixels/positions change are rewritten deliberately**, with the reason in the
  task report — never deleted or loosened to make a run pass. Each risk an old check covered stays covered. Must
  pass unchanged: `chrome-breath-skips-parts`, `chrome-notify-redraws-parts`, `chrome-pixels-same`,
  `chrome-kill-switch`, `chrome-working-marks`, the counters scenes.
- **M11p:** nothing visible changes inside any `render`; no new `timer(`, `request_animation_frame` or `spawn` loop
  outside the existing slides. The sidebar, status bar, tab bars and title area stay cached parts at definite sizes
  from the tokens.
- **Tokens only, no hex** in drawing code; every new surface is in `ui_uncached`'s surface / text-surface lists and
  in `drawn_pairs`. Light and dark both checked (`Themes::Both` for every new scene).
- **No feature changes:** every click, key, menu, drag, tooltip and setting does what it does today. The one content
  change is the sidebar's line two (§2.4).
- **`src/file_tab.rs`: only the outer bar's few lines, in Task 11.** Nothing inside the file tab is redesigned.
- **No font size or line height change for the terminal.** Overlays (palette, menus, popovers, Welcome,
  Preferences) keep their look.
- New ui-shots scenes: `Themes::Both`, `Kind::Visual`, in the area file's `SCENES` and in `registry::ORDER` beside
  their neighbours (after `cards-corners` for the card scenes). Made-up data only (`~/code/my-app`, `api`).
- **Public repo:** no private names, no absolute `/Users/` paths, no time-zone names; grep each commit's added lines
  for `/Users/`. Commits stage files by path; no AI attribution lines.
- **Shell safety:** never put backticks or `$(...)` inside double-quoted shell strings; single-quote grep patterns;
  never run the app by the bare name `fuxx` (the user's real app) — use `cargo run` / the built binary's path.
- Exact copy: line two `"✳ Claude · api"`, `"›_ zsh · my-app"`, `"› npm · my-app"` (`·` U+00B7 with spaces); group
  headings upper-cased at draw time; the app title `the Fuxx?`, `New Fuxxpace`, `⌘N`, `Jump to a workspace…`, `⌘P`
  unchanged.

## Review Focus

1. **Resizing the right panel with the sidebar shown or hidden, and in a narrow window:** the edge stays under the
   pointer (`width_for_edge` with 8 px right inset, 0 or 8 left, 1 px hairline); the panes never go under
   `PANE_COLUMN_MIN`. Pinned in Task 3 (`width_for_edge_puts_the_hairline_under_the_pointer`,
   `drawn_width_keeps_the_panes_their_minimum`).
2. **An overflowing tab bar** (12 tabs in a narrow split pane): tabs shrink to 100, then scroll; `＋ ⋯ ▥` stay
   pinned and visible; separators never double up at the scroll edge. Pinned in Task 5 (scene `tabs-minimal`, its
   overflow check).
3. **A long name, program and folder in a 180-wide sidebar:** name and line two ellipsize, the age drops, nothing
   overlaps the badge, the ⌘ hint or ×. Pinned in Task 8 (scene `sidebar-comfortable`, its narrow variant).
4. **Dragging a split's line at extreme ratios** (0.1 / 0.9, row and column): the 6 px strip stays centred on the
   hairline. Pinned in Task 2 (`the_grab_strip_is_centred_on_the_hairline_at_any_ratio`, scene `card-split`).
5. **Dragging a tab into another bar:** the gap opens where it will land with the new widths, separators and 12 px
   left inset; the drop tint (`drop_tab`) reads on the card. Pinned in Task 5 (`tab-drag` rewritten,
   `drop_tab_reads_on_the_card`).

---

### Task 0: Baselines (done by the coordinator, before this plan's commit)

- [x] **Step 1:** gate and full `scripts/ui-shots.sh` on `cc3b2b8`; numbers in "At a glance".

### Task 1: Tokens and colours

**Files:**
- Modify: `src/theme.rs` (`UiColors` fields, `ui_uncached`, `drawn_pairs`, tests), `src/ui_tokens.rs` (new sizes
  below, text sizes), any compile-led construction of `UiColors` (ui-shots token board)

**Interfaces:**
- Produces (`UiColors`): `raised`, `side`, `side_hover`, `side_selected`, `tab_active: Rgb`; `sidebar_background`
  darker (it is the canvas); `card_shadow_alpha` 0.45 dark / 0.12 light.
- Produces (`ui_tokens::size`, added, not yet drawn with): `CARD_INSET = 8.0`, `CARD_SHADOW_Y = 8.0`,
  `CARD_SHADOW_BLUR = 28.0`, `HAIRLINE = 1.0`. `CARD_RADIUS` stays 8 until Task 2. (`ui_tokens::text`): `UI = 13.5`,
  `KEYS = 11.5` added; `NAME`, `META`, `GROUP_HEADER` change in Task 8 with the rows that draw them.

- [ ] **Step 1: Write the failing tests** in `theme.rs` (every built-in theme and the default):
  `the_canvas_is_darkest_and_side_between` (luminance: `sidebar_background` < `side` < `card`, light and dark);
  `card_border_is_quiet_on_canvas_card_and_side` (≥ `QUIET_LINE_MIN`); `text_reads_on_the_new_surfaces`
  (`text`, `text_dim`, `text_faint` ≥ `TEXT_MIN` on `raised`, `side`, `side_hover`, `side_selected`, `tab_active`);
  `marks_read_on_the_new_surfaces` (`ring_working`, `ring_waiting`, `ring_done`, `accent` ≥ `MARK_MIN` on
  `tab_active`, `side`, `card_lift`); `tab_active_keeps_the_accent_hue` (as `row_selected`'s test);
  `dim_and_faint_text_keep_todays_values` (Mocha and Latte, the hex read from master); the shadow alpha test moves
  to 0.45 / 0.12.
- [ ] **Step 2: Run** `cargo test --bin fuxx theme::` — FAIL (fields missing).
- [ ] **Step 3: Implement** (§1, §11): canvas `mix(bg, black, 0.42)` dark / `mix(bg, fg, 0.08)` light; a local
  `base` = today's formula for `text_dim` / `text_faint` / `selection`; the formulas of plan-time call 3; add the new
  surfaces to `surfaces` / `text_surfaces` and to `drawn_pairs`. If a light theme's canvas step breaks a floor, take
  the largest step ≤ 0.08 that passes and say so.
- [ ] **Step 4: Run** — PASS; the gate; `scripts/ui-shots.sh --changed master`; full `scripts/ui-shots.sh` (the
  canvas got darker: rewrite any check that hard-codes a canvas step, report which).
- [ ] **Step 5: Commit** `M11al Task 1: tokens and colours`.

### Task 2: The floating card and splits as hairlines

**Files:**
- Modify: `src/panes.rs` (`card`, `render_node`, `grab_shift`, `accent_offset`, tests), `src/workspace.rs` (the root
  row, the card wrapper, `render_drag_strip` at `CARD_INSET`, the column's clip), `src/ui_tokens.rs`
  (`CARD_RADIUS` 14; `CARD_GAP` and `FOCUS_RING` go when unused), `src/terminal_view.rs` (corner radius follows),
  `src/agent_tab.rs` (only if its page paints a fill to the bottom: round it at `CARD_RADIUS`, §3.4),
  `src/ui_shots/window.rs`, `src/ui_shots/panes.rs`, `src/ui_shots/mod.rs` (`GAP` → `CARD_INSET`)

**Interfaces:**
- Produces: `panes::card(colors: &PaneColors) -> Div` (no ring argument; fill `card`, `CARD_RADIUS`, 1 px
  `card_border` over the content, shadow `0, CARD_SHADOW_Y, CARD_SHADOW_BLUR` at `card_shadow_alpha`, clipped);
  `grab_shift(ratio: f32, line: f32, grab: f32) -> f32`; `accent_offset(grab: f32) -> f32`;
  `render_layout(tree, panes)` draws leaves as plain flex columns (bar, then view, unfocused veil) and splits with a
  `HAIRLINE` `card_border` line. `PaneColors` loses `focus`.
- Consumes: Task 1's `CARD_INSET`, `CARD_SHADOW_Y`, `CARD_SHADOW_BLUR`, `HAIRLINE`, `card_shadow_alpha`.

- [ ] **Step 1: Write the failing tests** (`panes.rs`): `the_grab_strip_is_centred_on_the_hairline_at_any_ratio`
  (for `w` in {400, 961}, ratio in {0.1, 0.5, 0.9}: the line sits at `ratio * (w - 1)`; the strip
  `[ratio * w + grab_shift(..), + 6]` has its middle at the line's middle ± 0.01); `the_accent_line_is_centred_in_the_strip`.
- [ ] **Step 2: Write the failing scenes** (`window.rs`, after `cards-corners` in `ORDER`):
  `card` — one pane, sidebar shown: the card's left edge at the sidebar's right (no canvas between), its top at
  `CARD_INSET`, right at window − `CARD_INSET`, bottom at status bar − `CARD_INSET`; canvas 2 px outside each
  rounded corner diagonally; a pixel 4 px under the card's bottom middle darker than the canvas (the shadow);
  `card-split` (2 × 2 split): a 1 px `card_border` hairline between panes and no canvas there; the veil on the three
  unfocused panes; no accent ring; hovered line: the 2 px quiet line in its middle; `card-sidebar-hidden`: the card
  starts 8 px from the window's left, under the `TOP_BAND`.
  Rewrite `cards`, `cards-split`, `cards-corners` to the one-card rules (keep: nothing square past a corner, the
  first tab clear of the curve, the grid blue up to the rounded corner).
- [ ] **Step 3: Run** `scripts/ui-shots.sh card` — FAIL.
- [ ] **Step 4: Implement** §3.1–3.5, §3.7: the wrapper card at `CARD_INSET` (flush left against the sidebar; 8 px
  from the window's left with it hidden), the column clip moved (plan-time call 4), the strip above it
  `CARD_INSET` tall, focus ring gone, veil kept. The right panel stays outside the card for now (Task 3), 8 px right
  of it. Check the agent / file / browser tab bodies' bottom corners at radius 14 (§3.4).
- [ ] **Step 5: Run** — PASS; the gate; full `scripts/ui-shots.sh`; rewrite every scene broken by the new insets,
  radius or ring (`splits`, `window`, `panels`, …), listing each with its reason.
- [ ] **Step 6: Commit** `M11al Task 2: one floating card, splits as hairlines`.

### Task 3: The right panel inside the card

**Files:**
- Modify: `src/right_sidebar.rs` (`drawn_width`, `width_for_edge`, `right_sidebar_layout`, `render_right_sidebar`,
  the resize strip), `src/workspace.rs` (the card's inner row: panes, hairline, right panel; the slide clipped by
  the card), `src/ui_shots/right_sidebar.rs`, `src/ui_shots/window.rs`

**Interfaces:**
- Produces: `drawn_width(width: f32, window_width: f32, left: f32) -> f32` =
  `width.min((window_width - left - PANE_COLUMN_MIN - left_inset - CARD_INSET - HAIRLINE).max(0.0))` with
  `left_inset` = 0 when `left > 0`, else `CARD_INSET` (pass it explicitly if cleaner: say which);
  `width_for_edge(window_width: f32, x: f32) -> f32` = `clamp(Some(window_width - CARD_INSET - x - HAIRLINE / 2))`.
  The right panel fills with `side`, rounds its top-right and bottom-right corners at `CARD_RADIUS - HAIRLINE`.
- Consumes: Task 1's `side`; Task 2's card wrapper.

- [ ] **Step 1: Write the failing tests:** `width_for_edge_puts_the_hairline_under_the_pointer` (the drawn hairline's
  x for the returned width equals `x` ± 0.5, sidebar 0 and 290); `drawn_width_keeps_the_panes_their_minimum`
  (window 800, left 290 → panes ≥ 200; window 520 → 0).
- [ ] **Step 2: Write the failing scenes:** `card` gains the hairline between the panes and the right panel and the
  panel's `side` fill; `card-right-hidden` (the terminal owns the card's right corners: canvas outside, card inside);
  `card-narrow` (a window where `drawn_width` is 0: no panel, no hairline); `card-slide` (mid-slide: the panel cut at
  the card's right edge, nothing of it past the card or on the canvas).
- [ ] **Step 3: Run** — FAIL. **Step 4: Implement** §3.2, §6.1, §6.7 (header height unchanged until Task 5/9).
- [ ] **Step 5: Run** — PASS; the gate; full `scripts/ui-shots.sh`; rewrite `right-sidebar`, `files`,
  `right-panels`, `changes*`, `activity`, `branch*` position checks (derive from tokens), listing each.
- [ ] **Step 6: Commit** `M11al Task 3: the right panel inside the card`.

### Task 4: Status bar

**Files:**
- Modify: `src/status_bar.rs` (`PAD` 16, text `text::META`-sized 12, `MARK` 10, no fill), `src/ui_tokens.rs`
  (`STATUS_BAR` 26), `src/ui_shots/status_bar.rs`

- [ ] **Step 1: Write the failing scene check** in `status-bar`: the bar's middle pixel equals the canvas (no fill),
  height `STATUS_BAR`; a pixel just under the card in the bar is darker than the canvas (the shadow falls over it).
- [ ] **Step 2: Run** — FAIL. **Step 3: Implement** §7 (contents and fitting order unchanged; text 12 px as a local
  constant, plan-time call 8).
- [ ] **Step 4: Run** — PASS; the gate; full `scripts/ui-shots.sh`; rewrite `status-bar*`, `update-button`.
- [ ] **Step 5: Commit** `M11al Task 4: the status bar on the canvas`.

### Task 5: Minimal tabs (E)

**Files:**
- Modify: `src/tab_bar.rs` (`look`, the bar, tabs, icon tile, status dot, separators, buttons, drag chip, the
  constants below), `src/ui_tokens.rs` (`TAB_BAR` 52, `TABS_LEFT` 12, `TAB_TOP_GAP` goes), `src/theme.rs` (retire
  `tab_bar` / `tab_hover` / `TAB_BAR_*` and their tests; `drop_tab` on `card`), `src/mark_view.rs` (an 8 px size if
  the mark views need it), `src/workspace.rs` (the cached bar's height), `src/ui_shots/panes.rs`,
  `src/ui_shots/chrome*.rs`, `src/ui_shots/window.rs`

**Interfaces:**
- Produces: `look(ui: &UiColors, status: Status, active: bool, pane_focused: bool) -> Look` — active & focused:
  `tab_active`; active & unfocused: `card_hover`; inactive: no fill, hover `card_hover`, text `text_faint`;
  inactive waiting: `waiting_tint` / `waiting_hover`; active text `text`. `TabIcon::Mark` goes (the mark no longer
  replaces the icon). Constants: `TAB` 34, `TAB_RADIUS` 8, `TAB_PAD` 12, part gap 8, separator 1 × 16 with 4 px each
  side, `TAB_MIN_WIDTH` 100, `TAB_MAX_WIDTH` 220, `TILE` 20 / radius 6 / glyph 11.5, `STATUS_DOT` 8, `CLOSE` 16,
  `BUTTON` 28, `ICON` 16, `BUTTON_GAP` 4, `BUTTONS_PAD` 10.

- [ ] **Step 1: Write the failing tests:** `look_active_focused_is_tab_active`, `look_active_unfocused_is_card_hover`,
  `look_inactive_has_no_fill_and_faint_text`, `look_waiting_inactive_keeps_its_tint`; `drop_tab_reads_on_the_card`
  (theme: text and tab marks at their floors on `drop_tab`).
- [ ] **Step 2: Write the failing scenes:** `tabs-minimal` (after `tab-bars` in `ORDER`): tile on every tab with its
  glyph colour (agent: provider colour; terminal / browser: `accent`; file: `text_dim`); the status dot after the
  title (working arc, waiting dot, done ✓ in `text_dim`, idle none); separators; the active tab's `tab_active`
  wash; the preview tab italic; × on hover; an inactive waiting tab's tint; the bar's bottom `card_border` line; the
  bar has no fill (card colour between tabs); **overflow:** 12 tabs in a narrow split pane — each ≥ 100 wide, the
  row scrolls, `＋ ⋯ ▥` inside the pane and visible. `card-split` gains: `tab_active` only on the focused pane's
  active tab, `card_hover` on the others' active tabs.
- [ ] **Step 3: Run** — FAIL. **Step 4: Implement** §4.1–4.7 (Done in tabs is the muted ✓; the right panel's header
  height follows `TAB_BAR` already, so it stays level).
- [ ] **Step 5: Run** — PASS; the gate; full `scripts/ui-shots.sh`; rewrite `tab-bars*`, `tab-drag`, `tabs`,
  `splits`, `workspaces`, `browser-tab`, `chrome-bars-and-edges` and any agent-tab scene reading the bar, listing
  each.
- [ ] **Step 6: Commit** `M11al Task 5: minimal tabs`.

### Task 6: Terminal padding and the agent box

**Files:**
- Modify: `src/ui_tokens.rs` (`TERMINAL_PADDING` (28, 22), `SPLIT_TERMINAL_PADDING` (20, 14)), `src/agent_box.rs`
  (the §8 table), `src/agent_tab.rs` (the conversation's side room = `AREA_X` 28, if it is a local number),
  `src/ui_shots/terminal.rs`, `src/ui_shots/agent.rs`, `src/ui_shots/agent_box.rs`

- [ ] **Step 1: Write the failing scene** `agent-box-card` (after the other `agent-box-*` in `ORDER`): the box's left
  and right edges 28 px in from the card's inner edges; the conversation's text column starts at the same x as the
  box's.
- [ ] **Step 2: Run** — FAIL. **Step 3: Implement** §5, §8 (`AREA_X` 28, `AREA_TOP`/`AREA_BOTTOM` 8/20,
  `PLACE_GAP` 8, `BOX_RADIUS` 14, `BOX_X`/`BOX_Y`/`BOX_GAP` 14/12/10, `CHIP_H`/`CHIP_X`/`CHIP_RADIUS`/`CHIP_GAP`
  28/10/8/6, `BUTTON` 28, `MENU_WIDTH` 260, `ROW_H`/`ROW_RADIUS` 32/8). Command blocks' gutter keeps its offset
  from the grid.
- [ ] **Step 4: Run** — PASS; the gate; full `scripts/ui-shots.sh`; rewrite `cursor`, `command-blocks*`,
  `agent-tab-*`, `agent-box-*` position checks, listing each.
- [ ] **Step 5: Commit** `M11al Task 6: terminal padding and the agent box`.

### Task 7: The sidebar's band, search and footer

**Files:**
- Modify: `src/ui_tokens.rs` (`TOP_BAND` 52, `TRAFFIC_LIGHTS` (18, 20), `SIDEBAR_DEFAULT_WIDTH` 290), `src/main.rs`
  (only if the lights' placement needs more than the token), `src/sidebar.rs` (search §2.2, footer §2.6, `FOOTER`
  62, `SEARCH` 38), `src/title_area.rs` (band title 13 px semibold `text_dim`, sidebar button glyph 15,
  `FOOTER_BUTTON` 34, gap 12), `src/sidebar_toggle.rs` if it reads the band, `src/ui_shots/window.rs`,
  `src/ui_shots/sidebar.rs`, `src/ui_shots/welcome.rs`, `src/ui_shots/overlays.rs`

- [ ] **Step 1: Write the failing scene checks** in `sidebar` (or a new `sidebar-band` scene beside it): the
  search box 38 tall at 4 / 14 / 10 margins, `raised` fill, `card_border` edge; the footer 62 tall with its top
  line, the "New Fuxxpace ⌘N" button 38 tall reaching the 🔔; a new window's sidebar 290 wide.
- [ ] **Step 2: Run** — FAIL. **Step 3: Implement** §2 (no edge line on the sidebar's right), §2.1, §2.2, §2.6,
  §2.7.
- [ ] **Step 4: Run** — PASS; the gate; full `scripts/ui-shots.sh`; rewrite `window`, `title-area`, `title-bar`,
  `sidebar-button`, `welcome*`, `overlays`, `panels` (band 52, anchors) and every check hard-coding 240, listing
  each.
- [ ] **Step 5: Commit** `M11al Task 7: the sidebar's band, search and footer`.

### Task 8: Sidebar rows, headings and line two

**Files:**
- Modify: `src/sidebar.rs` (`WorkspaceFacts` gains `who`, `ssh_host`; `WorkspaceRow` gains `who`; `LineTwo`;
  `line_two`; `place_lines`; `row_height`; row, badge, age, selected bar and group-heading drawing; constants
  below), `src/ui_tokens.rs` (`SIDEBAR_ROW` 56, `SIDEBAR_ROW_TWO` 56, `SIDEBAR_ROW_THREE` goes, `GROUP_HEADER` 36,
  `text::NAME` 14, `META` 12.5, `GROUP_HEADER` 12), the facts' builder (`src/workspace.rs` /
  `src/workspace_agent.rs`, wherever `WorkspaceFacts { .. }` is filled), `src/ui_shots/sidebar.rs`

**Interfaces:**
- Produces: `#[derive(Clone, Debug, PartialEq)] pub enum Who { Agent { glyph: &'static str, name: &'static str },
  Program(String), Shell(String) }` with `fn label(&self) -> String` (`"✳ Claude"`, `"› npm"`, `"›_ zsh"`);
  `LineTwo::WhoWhere { who: Who, place: Option<String> }` replaces `Place`, `Here` and `State`;
  `pub fn who_where(who: &Who, place: Option<&str>) -> String`. Constants: `ROW_INSET` 10, `ROW_PAD` 12, row radius
  12, row margin 2, `AVATAR` 34 / radius 10 / letters 12 bold, `AVATAR_GAP` 12, `BADGE` 10 in a 2.5 ring set 3 px
  past the corner, `AGE_MIN_ROW` 240, accent bar 3 px inset 12, heading 16 above / 8 below / 20 sides; `TILE`,
  `CARD_PAD`, the group `CARD_RADIUS` go.

- [ ] **Step 1: Write the failing tests:** `line_two_says_who_and_where` (agent → `"✳ Claude · api"`; program →
  `"› npm · my-app"`; shell → `"›_ zsh · my-app"`; no folder → `"›_ zsh"`; SSH → host as where; in and out of a
  folder group); `line_two_keeps_not_live_waiting_note_and_title_first` (the old precedence test, updated);
  `every_row_is_56`; `place_lines_follow_who_where`. Update the existing `LineTwo::Place` / `Here` / `State` tests
  to the new rule (each listed in the report).
- [ ] **Step 2: Write the failing scene** `sidebar-comfortable` (after `sidebar-rows` in `ORDER`): 56 px rows; the
  badge ring in the row's fill (plain, hover, selected); ages top-right; the 3 px accent bar at the left edge;
  quiet uppercase headings without a card; a closed group's status only; who · where lines; **narrow variant**
  (sidebar 180, a long name / program / folder): ellipsized, no age, nothing overlapping.
- [ ] **Step 3: Run** — FAIL. **Step 4: Implement** §2.3–2.5 (the hover dot, ⌘ hint, × and Open stay, sized to the
  row; Chore 2 meanings unchanged).
- [ ] **Step 5: Run** — PASS; the gate; full `scripts/ui-shots.sh`; rewrite `sidebar`, `sidebar-rows`,
  `sidebar-quiet`, `sidebar-selection`, `sidebar-marks`, `agent-sidebar-waiting`, `chrome-sidebar-scroll`, listing
  each.
- [ ] **Step 6: Commit** `M11al Task 8: sidebar rows, headings and who · where`.

### Task 9: Right panel — switch, repo card, section headings

**Files:**
- Modify: `src/right_sidebar.rs` (the header: segmented switch, 14 px from the sides, `canvas` track, `card_lift`
  active segment; the Files root header as the repo card; section headings), `src/changes_panel.rs` (its header as
  the repo card; STAGED / CHANGES headings with count pills), `src/graph_panel.rs`, `src/activity_panel.rs` (their
  headings), `src/ui_shots/right_sidebar.rs`, `src/ui_shots/changes.rs`, `src/ui_shots/branch.rs`

- [ ] **Step 1: Write the failing scene** `right-panel-comfortable` (after `right-panels` in `ORDER`), first half:
  the header `TAB_BAR` tall with its bottom `card_border` line at the same y as the pane tab bar's line (one line
  across the card); the track's `canvas` fill and the active segment's `card_lift`; the repo card 4 / 14 / 6 margin,
  `card_hover` fill, name and meta; a section heading uppercase with its `card_lift` count pill.
- [ ] **Step 2: Run** — FAIL. **Step 3: Implement** §6.2–6.4 (same content, clicks, open/closed behaviour; the header
  padding outside the track drags the window when at the top, not in full screen).
- [ ] **Step 4: Run** — PASS; the gate; full `scripts/ui-shots.sh`; rewrite the affected `right-sidebar`, `files*`,
  `changes*`, `activity`, `graph*` checks, listing each.
- [ ] **Step 5: Commit** `M11al Task 9: right panel switch, repo card and headings`.

### Task 10: Right panel — rows, filter, branch footer

**Files:**
- Modify: `src/right_sidebar.rs` (`ROW` 32, `PILL_INSET` 10, `PILL_PAD` 12, `PILL_RADIUS` 9, names 13.5, folders
  12, `FILTER` 34 / radius 10 with a fill on `side` that passes the floors), `src/changes_panel.rs` (rows; branch
  footer: `BAR_PAD_X` 16, `BAR_PAD_Y` 14, `BAR_BUTTON` 34, radius 9, `card_border` edge, 15 px icons `text_dim`,
  "↑1 ahead of origin" 12.5 `text_faint`), `src/activity_panel.rs` (rows), row hover / selected on `side_hover` /
  `side_selected`, `src/ui_shots/right_sidebar.rs`, `src/ui_shots/changes.rs`, `src/ui_shots/branch.rs`

- [ ] **Step 1: Write the failing checks** in `right-panel-comfortable`, second half: 32 px rows; a hovered row's
  `side_hover`, a selected row's `side_selected`; the filter field 34 tall; the footer's 34 px ⟳ ⇡ buttons with
  their `card_border` edges; the indent guides still drawn on `side`.
- [ ] **Step 2: Run** — FAIL. **Step 3: Implement** §6.5, §6.6 (same actions, tooltips, error line).
- [ ] **Step 4: Run** — PASS; the gate; full `scripts/ui-shots.sh`; rewrite `files-follow`, `changes*`,
  `branch*`, listing each.
- [ ] **Step 5: Commit** `M11al Task 10: right panel rows, filter and footer`.

### Task 11: The file tab's and browser tab's outer bar (last: M11ah overlap)

**Files:**
- Modify: `src/file_tab.rs` (only the toolbar's fill `panel` → `card`, its bottom line `panel_border` →
  `card_border`, side padding → 16; height stays `FILE_TOOLBAR` 42), `src/browser_tab.rs` (`BAR`'s fill and line,
  the same), `src/ui_shots/file_tab.rs`, `src/ui_shots/chrome_parts.rs`

- [ ] **Step 1: Write the failing checks** in `file-tab` and `browser-tab`: the bar's fill equals the card, its
  bottom line `card_border`, the title starts 16 px in.
- [ ] **Step 2: Run** — FAIL. **Step 3: Implement** §9 (find bar unchanged). Keep the `file_tab.rs` diff to these
  lines (`git diff --stat` in the report).
- [ ] **Step 4: Run** — PASS; the gate; full `scripts/ui-shots.sh`; rewrite `file-tab*`, `file-live-*`,
  `file-follow-*`, `chrome-file-tab-toolbar`, listing each.
- [ ] **Step 5: Commit** `M11al Task 11: file and browser tab outer bars`.

### Task 12: Clean-up, docs and hand-off

**Files:**
- Modify: anything left unused (`grep` for `CARD_GAP`, `TAB_TOP_GAP`, `FOCUS_RING`, `SIDEBAR_ROW_THREE`, `tab_bar`,
  `tab_hover`, the sidebar's `TILE` / `CARD_PAD`, `#[allow(dead_code)]` on the new tokens); note 84 ("What was
  built", "Calls made while building", Limits, Final numbers, every rewritten scene, the on-screen check, status),
  this plan's table and status, `docs/04-Development/40 - Roadmap.md`, `docs/06-Progress-Log/60 - Progress Log.md`,
  module docs of `panes.rs`, `tab_bar.rs`, `right_sidebar.rs`, `sidebar.rs` (the old "6 pt gap" / "cards" words)

- [ ] **Step 1: M11p grep:** `git diff master...HEAD` for added `timer(`, `request_animation_frame`, `spawn(` —
  none outside the existing slides (list what's found).
- [ ] **Step 2: Write note 84's on-screen check** (~12 items, run with `FUXX_DAEMON_SOCKET=/tmp/fuxx-dev.sock cargo
  run`, debug only): Mocha and Latte; one pane; a 2 × 2 split; sidebar hidden; right panel hidden; both hidden; full
  screen; a narrow window; an agent tab with the box; a file tab; a waiting and a working tab; idle CPU (Activity
  Monitor, 60 s, branch vs master, nothing open).
- [ ] **Step 3: Full gate** and full `scripts/ui-shots.sh`; numbers into note 84 and the Progress Log; grep the
  branch's added lines for `/Users/` and private names.
- [ ] **Step 4:** Status everywhere: "🟨 built on branch m11al-ui-uplift, waiting for your on-screen check; not
  merged".
- [ ] **Step 5: Commit** `M11al Task 12: docs and hand-off`.
