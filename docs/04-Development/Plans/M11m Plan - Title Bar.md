---
tags: [development, plan, milestone]
milestone: M11m
status: planned
updated: 2026-11-09
---

# M11m Plan — Title bar

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

| At a glance | |
|---|---|
| **Design (the spec)** | [[64 - Milestone 11m - Title Bar]] |
| **Status** | 🟨 Designed and planned 2026-11-09; not built |
| **Tasks done** | 0 / 6 (Task 5, the release, is folded into the one release after M11n) |
| **▶ Pick up here** | Task 1. |
| **Decisions used** | Yours (2026-11-09, confirmed "Yes, like this"): open — the sidebar icon at the right edge of the sidebar's band, "the Fuxx?" centred there, 🔔 ⚙ stay in the footer; collapsed — title hidden, workspace name centred in the window's band, **[🔔] [⚙] [sidebar]** at the far right. Yours: "own milestone after M11l"; the one combined release after M11n. Everything else: AI's calls in note 64. |
| **Needs your eyes** | 👀 the 17-item on-screen check in note 64, after the combined release |

**Goal:** The top band becomes a title bar: open, the sidebar's band holds "the Fuxx?" in its middle and the sidebar
button at its right edge; collapsed, the window's band holds the workspace name in the window's middle and
[🔔] [⚙] [sidebar] at the far right, with the popover dropping down from that 🔔.

**Architecture:** Pure geometry first, in the two files that already own it: `sidebar_toggle.rs` (where the sidebar
button goes, the lights' clearance) and `title_area.rs` (the title, the band buttons, the name's inset, both
popover anchors). Open state: the sidebar's `render_top_band` gets a label holding the title and the button, so both
ride the sliding sidebar (Task 1). Collapsed state: the band buttons and the name are drawn at the window's root,
absolutely placed in window coordinates, so they sit over the pane band and the right sidebar's band alike (Task 2).
The popover picks its anchor from `sidebar.hidden` (Task 3).

**Tech Stack:** GPUI at the pinned rev (`1d2a4b3f…`), only APIs already used in these files (`div` absolute
placement, `group` / `group_hover`, `tooltip`, `occlude`, `on_mouse_down` + `stop_propagation`, `anchored` with
`Anchor::TopRight` / `BottomLeft` and `snap_to_window_with_margin`, `window.is_fullscreen()`,
`window.viewport_size()`). **No new crates, no new GPUI API**, so no Context7 lookup is needed; if a builder reaches
for an API not already in `src/`, check it in the pinned GPUI source first.

**Spec:** [[64 - Milestone 11m - Title Bar]]

## Global Constraints

- **No new crates, settings, key bindings, menu items, palette rows or Help rows.** ⌘\\, View → Toggle Sidebar and
  ⌘, unchanged. The macOS window title (`window_title`, "N · name") unchanged.
- **Copy (exact):** title `the Fuxx?`; tooltips `Notifications` (band 🔔, as the footer's), `Preferences (⌘,)`
  (band ⚙ **and** footer ⚙, `PREFERENCES_TIP`), the sidebar button's `Hide sidebar (⌘\)` / `Show sidebar (⌘\)`
  unchanged.
- **Values:** `TOP_BAND` 38, `HIT` 24, `HIT_RADIUS` 6, button y = `(TOP_BAND − HIT) / 2` = 7. `BAND_EDGE` 8.0 (so
  `BAND_EDGE + HIT/2 == ROW_INSET + FOOTER_BUTTON/2`), `BAND_GAP` 4.0. `LIGHTS_CLEAR` 80.0 windowed / 13.0 full
  screen (replace `X_WINDOWED` / `X_FULL_SCREEN`). `TITLE_ROOM` 64.0, `TITLE_GAP` 8.0. `NAME_GAP` 12.0, `NAME_MIN`
  60.0 (kept). Popover: `POPOVER_DROP` 4, `EDGE_MARGIN` 8, `POPOVER_MIN` 228, `POPOVER_WIDTH` 340, `LIST_MAX` 420
  (kept).
- **Geometry rules:** `open_button_x(w) = w − BAND_EDGE − HIT`; `band_buttons(W) = [W − 88, W − 60, W − 32]`
  (🔔, ⚙, sidebar; left edges); title shown iff `w/2 − TITLE_ROOM/2 ≥ lights_clear(fs)` and
  `w/2 + TITLE_ROOM/2 + TITLE_GAP ≤ open_button_x(w)`; `name_inset(fs) = max(lights_clear(fs), BAND_EDGE + 3·HIT +
  2·BAND_GAP) + NAME_GAP` (= 100); name shown iff `W − 2·name_inset ≥ NAME_MIN`.
- **Look:** title `text::NAME`, semibold, `text_dim`, never truncated. Name as today (`text::NAME`, semibold, `text`,
  truncated), centred on the **window**. Band buttons: the sidebar button's look (`text_dim` icon, `text` +
  `panel_hover` fill on hover, lit = `panel_hover` fill + `text` icon); 🔔 / ⚙ glyphs via `footer_icon` (16 pt);
  unseen dot via `unseen_dot`. **No new colours, no new animation.**
- **Clicks:** the title and the name take no clicks (no listeners, no `occlude`), so the band under them drags and
  double-click zooms; every band button `occlude`s and stops the left press. Full screen: nothing drags (as today).
- **Every toggle closes the popover** and gives the keyboard back to the focused terminal.
- Every visual change has ui-shots in **Catppuccin Mocha and Latte**; new scenes are named `title-bar*`. Ink
  positions are read over **every device pixel** of the box (the `ink_box` way of the existing `title-area` footer
  check), so 1x and 2x agree; centres within ±0.5 pt, ±1 pt for text.
- Existing scenes keep passing except the assertions this plan names as replaced. A failure anywhere else is a bug to
  debug (superpowers:systematic-debugging), not a check to loosen.
- Subagents never `cargo run` the app. Allowed: `cargo test`, `cargo clippy --all-targets` and
  `cargo clippy --all-targets --features ui-shots`, `cargo fmt --check`,
  `cargo run --features ui-shots -- __ui-shots [filter]`.

## Review Focus

1. **The band stops dragging or zooming where text is drawn.** The title (open) or the name (collapsed, now drawn at
   the window's root over two bands) picks up a listener or `occlude` and swallows the press. Expected: a press and
   move on the text drags the window; only the buttons are dead to drags. (Task 1: `title-bar` behaviour
   `a press on the title arms a window drag`; Task 2: `a press on the name arms a window drag` and `a press on each
   band button doesn't`.)
2. **The popover outlives its 🔔.** Open from the footer, then ⌘\\ shows the band; open from the band, then ⌘\\
   shows the sidebar. Expected: closed both ways, no backdrop left eating clicks, the terminal has the keyboard.
   (Task 3: `every toggle closes the popover`.)
3. **The right sidebar open while collapsed.** The band buttons drawn under the right sidebar's band (z-order) or
   clipped by the pane column's `overflow_hidden`, or the name centred on the pane column. Expected: the three icons
   at `band_buttons(W)`, the name's ink centred on `W/2`. (Task 2: scene `title-bar-collapsed-right`.)
4. **Mid-slide.** A hide slide 60 ms in: the band buttons hidden under the sliding sidebar, or the sliding sidebar's
   button not drawn. Expected: the three band icons' ink at their slots above everything; the sliding sidebar partly
   out. (Task 2: scene `title-bar-sliding`.)
5. **Sidebar widths around the title's threshold.** Resizing across ~224 pt: the title overlaps the lights' area or
   the button, or flickers between widths. Expected: shown iff the rule holds, its box always clear of both.
   (Task 1: `the_title_never_meets_the_lights_or_the_button` over every width 180..=400 in 0.5 pt steps, both
   full-screen states; scene `title-bar-open-threshold` at exactly the threshold width.)

---

### Task 1: The open state — the button at the sidebar's right edge, the title in the middle

**Files:** Modify `src/sidebar_toggle.rs` (geometry, the button takes its x), `src/title_area.rs` (title geometry and
`render_app_title`), `src/sidebar.rs` (the sidebar band's label; the footer ⚙'s tooltip), `src/workspace.rs` (the
root button only while hidden, at its old place until Task 2), `src/ui_shots.rs` (the `ink_box` helper, scenes, the
replaced assertions).

**Interfaces — Produces:**
- `sidebar_toggle.rs`: `pub(crate) const BAND_EDGE: f32 = 8.0;` `pub(crate) const BUTTON_Y: f32 = (TOP_BAND − HIT)
  / 2.0;` `pub fn lights_clear(full_screen: bool) -> f32` (80.0 / 13.0); `pub fn open_button_x(sidebar_width: f32)
  -> f32`; `Workspace::render_sidebar_button(&self, ui: UiColors, x: f32, cx: &mut Context<Self>) -> AnyElement`
  (absolute at `(x, BUTTON_Y)` in its parent; look, hover, tooltip, press-stop and click unchanged). `button_origin`
  is removed; until Task 2 the collapsed root button is placed at `lights_clear(window.is_fullscreen())`.
- `title_area.rs`: `pub(crate) const TITLE: &str = "the Fuxx?";` `pub(crate) const TITLE_ROOM: f32 = 64.0;`
  `const TITLE_GAP: f32 = 8.0;` `pub(crate) const PREFERENCES_TIP: &str = "Preferences (⌘,)";`
  `pub fn shows_title(sidebar_width: f32, full_screen: bool) -> bool`;
  `Workspace::render_app_title(&self, ui: UiColors, window: &Window) -> Option<AnyElement>` (absolute over the
  sidebar band, `justify_center` on its full width, no listeners; `None` when `!shows_title`).
- `sidebar.rs` `render_sidebar`: `render_top_band("sidebar-band", Some(label), …)` where `label` holds
  `render_app_title` and `render_sidebar_button(ui, open_button_x(width), cx)`. Drawn whenever the sidebar is
  drawn (sliding out too).
- `ui_shots.rs`: `fn ink_box(shot: &Shot, dims: Size<Pixels>, area: (f32, f32, f32, f32), bg: [u8; 3], ink: [u8; 3])
  -> Option<(f32, f32, f32, f32)>` — `(x0, x1, y0, y1)` in points of every device pixel inside `area` (x, y, w, h)
  at least half-way from `bg` to `ink` (squared distance × 4 ≥ full), `None` when none. The `title-area` footer
  check is moved onto it.

- [ ] **Step 1: failing unit tests**
  - (sidebar_toggle) `the_open_button_sits_at_the_sidebars_right_edge_on_the_gears_centre`: for `w` in
    `[180.0, 240.0, 260.0, 340.0, 400.0]`: `open_button_x(w) + HIT + BAND_EDGE == w`;
    `open_button_x(w) + HIT / 2.0 == footer_buttons(w)[2] + FOOTER_BUTTON / 2.0`; `BUTTON_Y + HIT / 2.0 ==
    TOP_BAND / 2.0`; `lights_clear(false) == 80.0`, `lights_clear(true) == 13.0`. (Replaces
    `the_button_sits_right_of_the_traffic_lights_and_moves_to_the_edge_in_full_screen`.)
  - (title_area) `the_title_shows_where_it_fits`: `shows_title(240.0, false)`; `!shows_title(180.0, false)`;
    `shows_title(180.0, true)`; with `t = 2.0 * (80.0 + TITLE_ROOM / 2.0)`: `shows_title(t, false)` and
    `!shows_title(t - 0.5, false)`; `TITLE == "the Fuxx?"`.
  - (title_area) `the_title_never_meets_the_lights_or_the_button`: for both `fs`, every `w` from 180.0 to 400.0 in
    0.5 steps where `shows_title(w, fs)`: `w/2 − TITLE_ROOM/2 ≥ lights_clear(fs)` and `w/2 + TITLE_ROOM/2 + TITLE_GAP
    ≤ open_button_x(w)`; and `shows_title` is monotonic in `w` (never true then false as `w` grows).
- [ ] **Step 2: failing ui-shots** — family `title-bar` (add `("title-bar", title_bar_scenes)` to `SCENES`), over
  `switcher_workspace`, 760×200 unless noted, sidebar 240, Mocha and Latte:
  - `title-bar-open`: `ink_box` of the band (x 0..240, y 0..38) against `sidebar_background` toward `text_dim`
    split at `open_button_x(240)`: the title's box (left part) is ≤ `TITLE_ROOM` wide and ≥ 30 wide, its centre
    within ±1 of 120, its middle within ±1 of 19, its left ≥ 80; the button's box (24×24 at `open_button_x`) is
    centred within ±0.5 of `open_button_x(240) + 12` and of the footer gear's ink centre. No ink in the band from x 0
    to 80 (shots have no traffic lights, and the button's old place starts at 80).
  - `title-bar-open-narrow`: sidebar 180: no title ink between 80 and `open_button_x(180)`; the button's ink
    centred at `open_button_x(180) + 12` (±0.5).
  - `title-bar-open-threshold`: sidebar at `2 · (80 + TITLE_ROOM/2)` (rounded up to the half point): the title is
    drawn and its ink box lies inside `[80, open_button_x(w) − TITLE_GAP]`.
  - Behaviour (Mocha, 960×600, sidebar 240): a press at (120, 19) on the title → `band_pressed`; release. A press on
    the button (`open_button_x(240) + 12`, 19) → `!band_pressed`; a click there → `sidebar.hidden`. ⌘\\ → shown
    again.
  - Replaced assertions: `sidebar-button` loses "same place in both states" (shown: the icon's edge is at
    `open_button_x + 4.75`; hidden: at `lights_clear(false) + 4.75` until Task 2) and its hover margin point moves
    with the button; `title-area` loses "the band is bare where the 🔔 and + were" for the open state (the band now
    holds the title) and keeps the hidden-state check; its "press where the buttons were drags" stays (that point is
    now on the title). Every use of the removed `button_origin(false)` in `ui_shots.rs` becomes
    `(lights_clear(false), BUTTON_Y)` for the collapsed state and `(open_button_x(width), BUTTON_Y)` for the open one.
- [ ] **Step 3: run** `cargo test sidebar_toggle title_area` and `cargo run --features ui-shots -- __ui-shots
  title-bar` → FAIL (undefined / assertions).
- [ ] **Step 4: implement** per Interfaces. The footer ⚙ gets `.tooltip(… Tip(PREFERENCES_TIP.into()) …)`. If the
  drawn title's ink is wider than 64 pt in the shot font, set `TITLE_ROOM` to `ceil(ink) + 4` (still ≤ 80, so the
  default 240 pt sidebar shows it) and update the note's 224 pt figure; the tests are written in terms of the
  constant.
- [ ] **Step 5: run** `cargo test`, `cargo clippy --all-targets`, `cargo clippy --all-targets --features ui-shots`,
  `cargo fmt --check`, `cargo run --features ui-shots -- __ui-shots title-bar`, then `… __ui-shots sidebar-button`
  and `… __ui-shots title-area` → green.
- [ ] **Step 6: commit** `feat(m11m): the sidebar button at the sidebar band's right edge and "the Fuxx?" in its middle`.

### Task 2: The collapsed state — the name centred on the window, [🔔] [⚙] [sidebar] at the far right

**Files:** Modify `src/title_area.rs` (band geometry, `render_band_buttons`, `render_workspace_name` placement),
`src/workspace.rs` (root children; the pane band no longer takes the name), `src/ui_shots.rs` (scenes, replaced
assertions).

**Interfaces — Consumes:** Task 1 (`BAND_EDGE`, `BUTTON_Y`, `lights_clear`, `render_sidebar_button(ui, x, cx)`,
`PREFERENCES_TIP`, `ink_box`). **Produces:**
- `title_area.rs`: `pub(crate) const BAND_GAP: f32 = 4.0;` `pub(crate) const BAND_GROUP: f32 = BAND_EDGE + 3.0 * HIT
  + 2.0 * BAND_GAP;` (88) `pub fn band_buttons(window_width: f32) -> [f32; 3]` (🔔, ⚙, sidebar left edges);
  `pub fn name_inset(full_screen: bool) -> f32` (new rule, = 100); `pub fn shows_name(width: f32, full_screen: bool)
  -> bool` (rule unchanged, new inset); `pub(crate) const BAND_BELL_GROUP: &str = "band-notifications";`
  `pub(crate) const BAND_PREFERENCES_GROUP: &str = "band-preferences";`
  `Workspace::render_band_buttons(&self, ui: UiColors, window: &Window, cx: &mut Context<Self>) ->
  Option<AnyElement>` (`None` unless `sidebar.hidden`; element ids `band-notifications`, `band-preferences`,
  `sidebar-toggle`; the 🔔 opens `open_notifications`, the ⚙ calls `preferences::open_or_activate`, the sidebar
  button is `render_sidebar_button(ui, band_buttons(W)[2], cx)`).
  `render_workspace_name` keeps its signature; it is placed at the window's root (absolute, top 0, height
  `TOP_BAND`, `left`/`right` = `name_inset`), no listeners.
- `workspace.rs` `render`: root children in this order after the status bar: `render_workspace_name` (when hidden),
  `render_band_buttons`, then the existing overlays (so the Welcome scrim and popovers stay above). The
  `pane-band`'s label becomes `None`; the old root `button` goes.

- [ ] **Step 1: failing unit tests** (title_area)
  - `the_band_buttons_line_up_bell_gear_sidebar_at_the_windows_right`: `band_buttons(960.0) == [872.0, 900.0,
    928.0]`; for `W` in `[240.0, 360.0, 760.0, 1600.0]`: `b[2] + HIT + BAND_EDGE == W`, `b[1] + HIT + BAND_GAP ==
    b[2]`, `b[0] + HIT + BAND_GAP == b[1]`, `W − b[0] == BAND_GROUP`; `BAND_EDGE + HIT / 2.0 == ROW_INSET +
    FOOTER_BUTTON / 2.0`.
  - `the_name_keeps_clear_of_the_lights_and_the_band_buttons` (replaces `the_name_keeps_clear_of_the_sidebar_button`):
    `name_inset(false) == 100.0`, `name_inset(true) == 100.0`; for both: `name_inset(fs) ≥ lights_clear(fs) +
    NAME_GAP` and `name_inset(fs) ≥ BAND_GROUP + NAME_GAP`.
  - `a_window_too_narrow_for_the_name_leaves_it_out` (replaces `a_band_too_narrow_for_the_name_leaves_it_out`):
    `shows_name(260.0, false)`, `!shows_name(259.5, false)`, the same with `true`.
- [ ] **Step 2: failing ui-shots** (family `title-bar`, Mocha and Latte, `sidebar.hidden = true`, 760×200 unless
  noted). Icons read with `ink_box` over each 24×24 slot at `(band_buttons(W)[k], BUTTON_Y)` against
  `sidebar_background` toward `text_dim`; the name toward `text` over the band (y 0..38):
  - `title-bar-collapsed` (all seen): three ink boxes, each centred within ±0.5 of its slot's centre, their vertical
    centres within ±0.5 of each other and of 19; no accent pixel at the 🔔's dot centre `(b[0] + HIT − DOT_INSET −
    DOT/2, BUTTON_Y + DOT_INSET + DOT/2)`; the name's ink centred within ±1 of `W/2` and inside `[100, W − 100]`;
    no `text_dim`-ward ink in the band from x 0 to 100 (the old button place is bare, no title anywhere).
  - `title-bar-collapsed-hover`: the pointer on the ⚙ slot: the slot's corner pixel `(b[1] + 1.5, 19)` is
    `panel_hover`; the 🔔 slot's is `sidebar_background`.
  - `title-bar-unseen`: notifications unseen: the dot centre pixel is `accent`; the ring pixel `(dot.x + DOT/2 − 1,
    dot.y)` is `sidebar_background`.
  - `title-bar-collapsed-narrow`: 360×200, the long name from `title-area` (`LONG_NAME`): the name's ink inside
    `[100, 260]` and > 20 inked pixels; the three icons at their slots.
  - `title-bar-collapsed-tiny`: 240×200: no name ink anywhere in the band left of `b[0]`; the three icons at their
    slots.
  - `title-bar-collapsed-right` (Review Focus 3): 960×300, the right sidebar shown (default width): the three icons
    at `band_buttons(960)`; the name's ink centred within ±1 of 480.
  - `title-bar-sliding` (Review Focus 4): a hide slide 60 ms in (`Slide { showing: false, since: now − 60 ms, from:
    1.0 }`), 760×200: the three icons at their slots; the pixel at (30, 80) is the sidebar's (partly in) and the one
    at (150, 80) is not (partly out), as in `sidebar-button-sliding`.
  - Behaviour (Mocha, 960×600, hidden): a press at (480, 19) on the name → `band_pressed`, release; a press on each
    band slot's centre → `!band_pressed` (release off the button so the ⚙ opens no Preferences window); a click on
    the 🔔 slot → `notifications_popover.is_some()` and unseen 0, Escape closes it; a click on the sidebar slot →
    `!sidebar.hidden`.
  - Replaced assertions: `sidebar-button-hidden` now finds the icon at `band_buttons(360)[2] + 4.75`;
    `title-area`'s hidden checks use the new `name_inset` (the name's room); its "nothing right of the name's room"
    becomes "nothing from `W − name_inset` to `band_buttons(W)[0]`" (the buttons are there now); its "bare from the
    sidebar button to the name" sweep starts at x 0 (the band's left is bare up to the name's inset).
- [ ] **Step 3: run** `cargo test title_area` and `cargo run --features ui-shots -- __ui-shots title-bar` → FAIL.
- [ ] **Step 4: implement** per Interfaces. The band 🔔 is lit (`panel_hover` fill, `footer_icon(…, true)`) while
  `notifications_popover.is_some()`, with `unseen_dot(ui)` while unseen; tooltips `Notifications` and
  `PREFERENCES_TIP`.
- [ ] **Step 5: run** `cargo test`, both clippy runs, `cargo fmt --check`, `__ui-shots title-bar`, `__ui-shots
  sidebar-button`, `__ui-shots title-area`, then the full `cargo run --features ui-shots -- __ui-shots` → green.
- [ ] **Step 6: commit** `feat(m11m): collapsed, the workspace name centred on the window and [🔔] [⚙] [sidebar] at the far right`.

### Task 3: The popover from the band, and every toggle closes it

**Files:** Modify `src/title_area.rs` (band anchor, list room, entry top; `render_notifications` picks the anchor),
`src/workspace.rs` (`toggle_sidebar`), `src/ui_shots.rs` (scenes).

**Interfaces — Consumes:** Task 2 (`band_buttons`, `BAND_EDGE`). **Produces** (title_area):
- `pub(crate) fn band_popover_width(window_width: f32) -> f32` = `(W − 2·EDGE_MARGIN).clamp(POPOVER_MIN,
  POPOVER_WIDTH)`.
- `pub(crate) fn band_popover_anchor(window_width: f32) -> (f32, f32)` — the panel's **top-right** corner:
  `(W − BAND_EDGE, TOP_BAND + POPOVER_DROP)`.
- `pub(crate) fn band_list_room(viewport_height: f32) -> f32` = `(H − TOP_BAND − POPOVER_DROP − PANEL_CHROME −
  STATUS_BAR − EDGE_MARGIN).clamp(0.0, LIST_MAX)`.
- `#[cfg_attr(not(feature = "ui-shots"), allow(dead_code))] pub(crate) fn band_entry_top(i: usize, viewport_height:
  f32) -> f32` = `TOP_BAND + POPOVER_DROP + 1.0 + PADDING + HEADER + PADDING + i·(ENTRY + ENTRY_GAP)`.
- `render_notifications`: with `sidebar.hidden`, `anchored().anchor(Anchor::TopRight).position(band_popover_anchor)`,
  width `band_popover_width`, list `max_h(band_list_room)`; else as today.
- `toggle_sidebar`: `let popover = self.notifications_popover.take().is_some();` on every toggle (not only hiding);
  the keyboard returns to the focused terminal when it was open.

- [ ] **Step 1: failing unit tests** (title_area)
  - `the_band_popover_drops_from_the_band_inside_the_window`: for `W` in `[360.0, 760.0, 960.0, 1600.0]`: `(r, t) =
    band_popover_anchor(W)`; `r == W − BAND_EDGE`; `t == TOP_BAND + POPOVER_DROP`; `w = band_popover_width(W)` in
    `POPOVER_MIN..=POPOVER_WIDTH`; `r − w ≥ EDGE_MARGIN`; `r − w ≤ band_buttons(W)[0]` (it spans the 🔔).
    `band_popover_width(1600.0) == POPOVER_WIDTH`.
  - `the_band_popovers_list_stops_above_the_status_bar`: for `H` in `[300.0, 480.0, 600.0, 1000.0]`: `t +
    PANEL_CHROME + band_list_room(H) ≤ H − STATUS_BAR − EDGE_MARGIN`; `band_list_room(H) ≤ LIST_MAX`;
    `band_list_room(100.0) == 0.0`.
  - `a_band_entrys_place_is_counted_down_from_the_band`: `band_entry_top(0, 600.0) == TOP_BAND + POPOVER_DROP + 1.0
    + PADDING + HEADER + PADDING`; `band_entry_top(1, 600.0) == band_entry_top(0, 600.0) + ENTRY + ENTRY_GAP`.
- [ ] **Step 2: failing ui-shots** (family `title-bar`, Mocha and Latte, hidden, notifications from
  `shot_notifications`):
  - `title-bar-popover`: 960×480, `open_notifications`: with `(r, t) = band_popover_anchor(960)` and `w =
    band_popover_width(960)`: pixels `(r − 3, t + 20)` and `(r − w + 3, t + 20)` are `panel`; `(r + 3, t + 20)` is
    not; `(r − 20, t − 6)` (in the band) is not `panel` (it starts under the band); the band 🔔's most inked pixel in its column is
    nearer `text` than `text_dim` (lit) and its slot's corner `(b[0] + 1.5, 19)` is `panel_hover`; unseen 0; an
    overlay is open.
  - `title-bar-popover-narrow`: 360×480: the panel's leftmost `panel` pixel on row `t + 20` is ≥ `EDGE_MARGIN` and
    its rightmost ≤ `360 − BAND_EDGE`.
  - Behaviour (Mocha, 960×600, hidden): open from the band; hover entry 0 at `(r − w + 60, band_entry_top(0, 600) +
    20)` → the fill pixel at `(r − w + 12, band_entry_top(0, 600) + 2)` is `panel_hover`; click it → popover closed,
    `agent` active with the keyboard, `sidebar.hidden` still true. Entry 3 (closed terminal) does nothing.
  - Behaviour, **every toggle closes the popover** (Review Focus 2): hidden, open from the band, `toggle_sidebar` →
    popover `None`, sidebar shown, the active terminal's view is focused, `overlays(cx).any()` false; then open from
    the footer, `toggle_sidebar` → the same. (The existing `title-area` "hiding the sidebar closes the popover" stays.)
- [ ] **Step 3: run** `cargo test title_area` and `__ui-shots title-bar` → FAIL.
- [ ] **Step 4: implement** per Interfaces.
- [ ] **Step 5: run** `cargo test`, both clippy runs, `cargo fmt --check`, the full `cargo run --features ui-shots --
  __ui-shots` → green.
- [ ] **Step 6: commit** `feat(m11m): the notifications popover drops down from the band's 🔔; every sidebar toggle closes it`.

### Task 4: Merge and push (pre-authorized, autonomous program)

- [ ] Full gate on the branch: `cargo test`, `cargo clippy --all-targets`, `cargo clippy --all-targets --features
  ui-shots`, `cargo fmt --check`, the full `cargo run --features ui-shots -- __ui-shots` green; a whole-branch review
  against note 64 and this plan's Review Focus; then merge `m11m-title-bar` into `master` and push.

### Task 5: Release — folded into the one combined release after M11n

- [ ] ~~Version bump, `chore(release): …`, tag, watch the Release run~~ Skipped by ruling: the one combined release
  (M11i–M11n) ships after M11n (your rule, 2026-11-09: "push the release after performance milestone").
- [ ] You: the 17-item on-screen checklist in note 64 (after the combined release).

### Task 6: Docs

- [ ] Note 64: status, an "M11m as built" section (what the build settled or changed, e.g. a measured `TITLE_ROOM`),
  the checklist's state. This plan (ticks, header). The roadmap (`40 - Roadmap.md`). The Plan Tracker. The Progress
  Log. `STRUCTURE.md` (`sidebar_toggle.rs`: "the sidebar button at the sidebar band's right edge, or at the band's
  far right when collapsed"; `title_area.rs`: "the app title, the band's 🔔 ⚙ and sidebar buttons when collapsed,
  the workspace name, the popover from the footer or the band"). `README.md` (line 89's "A sidebar button beside the
  window buttons" and the footer line: the title bar, the band's 🔔 / ⚙ when collapsed). Note 56 (Help): no change,
  no shortcut changed.
- [ ] Commit `docs(m11m): …`.
