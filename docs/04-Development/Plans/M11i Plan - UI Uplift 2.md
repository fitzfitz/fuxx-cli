---
tags: [development, plan, milestone]
milestone: M11i
status: planned
updated: 2026-11-08
---

# M11i Plan — UI uplift 2

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

| At a glance | |
|---|---|
| **Design (the spec)** | [[60 - Milestone 11i - UI Uplift 2]] (incl. "Settled before planning") |
| **Status** | 🟦 Planned 2026-11-08; not started |
| **Tasks done** | 0 / 10 |
| **▶ Pick up here** | Task 1: tokens and terminal padding |
| **Decisions used** | Yours (2026-11-08): the eight items, in M11i, before M11h. Everything else: AI's calls in note 60 (you skipped the design questions). |
| **Needs your eyes** | 👀 Tasks 8–9 (merge/push/release on your word, then the 20-item on-screen check in note 60) |

**Goal:** smart names for terminals and workspaces, compact sidebar rows with a quiet selection, the panes and the
right sidebar as rounded cards with gaps, more terminal padding, a status bar, a three-button tab bar with a ⋯ menu,
and a Files tree with icons, guides and a root header.

**Architecture:** Pure cores first, GPUI drawing second. `ui_tokens` and `Theme::ui()` gain the sizes and colours
(Task 1). Two new pure modules, `procs.rs` (parse `ps` and `lsof` output, the foreground job, descendants) and
`smart_name.rs` (program names, the naming rules, the stable-name helper), feed a `Programs` store on the
Workspace beside `Places`, refreshed off the UI thread by a visible-only ticker (Task 2). Then the drawing tasks:
sidebar rows (3), cards (4), tab bar (5), Files (6), status bar (7). Everything slow (`ps`, `lsof`, git) runs on
the background executor with the existing capped runner (`git_branch::run_capped_outcome`).

**Tech Stack:** GPUI at the pinned rev (`1d2a4b3f…`; `PaintQuad::corner_radii`, `BoxShadow`; content masks are
rectangles only); `/bin/ps` and `/usr/sbin/lsof` as processes. **No new crates.**

**Spec:** [[60 - Milestone 11i - UI Uplift 2]]

## Global Constraints

- **No new crates** (`Cargo.toml` `[dependencies]` unchanged). No new settings, key bindings or menu items.
- **Nothing blocks the UI thread:** `ps`, `lsof` and git run on the background executor, each capped at **2 s**
  (`run_capped_outcome`), absolute paths `/bin/ps`, `/usr/sbin/lsof`.
- **Lookups:** names every **3 s** (`NAMES_REFRESH`), ports every **5 s** (`PORTS_REFRESH`), both only while the
  window can be seen (`WindowVisible`) and something needs them; `lsof` failing → next try in **30 s**. The
  screenshots freeze both (`frozen`).
- **Naming rules** (note 60 §1, in order): user's name → `program · folder` → `folder` at a prompt → SSH `host`
  / `host · folder` → unknown program: `preset · folder` else `folder` → OSC title → `╭∩╮`
  (`groups::DEFAULT_NAME`). Separator `" · "`. Folder: last component, `~` for home, `/` for root. Program name
  ≤ **24** chars then `…`. Stable rule: a change needs the same answer **twice in a row**; a known agent, and the
  prompt after a known agent, are taken at once.
- **Known agents:** `claude` (`@anthropic-ai/claude-code`), `codex` (`@openai/codex`), `opencode`, `aider`,
  `gemini` (`@google/gemini-cli`), `goose`, `amp`. **Interpreters:** `node`, `python`, `python3`, `python3.N`,
  `ruby`, `perl`, `php`, `bun`, `deno`; shells with a script: `sh`, `bash`, `zsh`, `fish`. **Subcommand tools:**
  `npm`, `pnpm`, `yarn`, `bun`, `npx`, `cargo`, `git`, `go`, `make`, `just`, `docker`, `uv`, `mise` (+1 word; after
  `run`/`exec`, +2).
- **The scripting API keeps `Session::label()`** (name → title → preset → ╭∩╮) for `session.list` and target
  resolution. The UI uses `Workspace::terminal_label`.
- **Sizes (`ui_tokens::size`):** `CARD_GAP = 6.0`, `CARD_RADIUS = 8.0`, `FOCUS_RING = 1.0`, `STATUS_BAR = 22.0`,
  `TAB_BAR = 30.0`, `TAB_TOP_GAP = 4.0`, `SIDEBAR_ROW = 22.0`, `SIDEBAR_ROW_TWO = 32.0` (replaces `WAITING_ROW`),
  `SELECTION_BAR = 2.0`, `TERMINAL_PADDING = (16.0, 12.0)`, `SPLIT_TERMINAL_PADDING = (12.0, 10.0)`.
  `UNFOCUSED_VEIL` stays 0.4.
- **Colours (`UiColors`, dark / light):** `card` = background; `card_border` = mix(bg, fg, 0.10 / 0.12) at
  `QUIET_LINE_MIN` on `[sidebar_background, bg]`; `card_shadow_alpha: f32` = 0.30 / 0.08; `row_selected` =
  mix(sidebar, accent, 0.18 / 0.12) stepped down 0.02 until `contrast(text, it) ≥ TEXT_MIN`; `card_hover` =
  mix(bg, fg, 0.06); `card_selected` = mix(bg, accent, 0.18 / 0.12) with the same step rule; `tab_bar` =
  mix(bg, sidebar, 0.5); `tab_hover` = mix(tab_bar, fg, 0.06); `guide` = mix(bg, fg, 0.12) at `QUIET_LINE_MIN` on
  bg. The new surfaces join the `surfaces` list behind `text` / `text_dim` / `text_faint`.
- **Rows:** line 1 **17 pt** (name 13 pt semibold), line 2 **13 pt** (11 pt), 2 pt apart, `ROW_INSET` 6, `ROW_PAD`
  8, radius 6. Line 2 precedence: `not live` → `Needs input · question` (ring_waiting / text_dim) → note → `branch •
  folder`. Idle mark: filled 6 pt dot (`text_dim`) while a program runs, hollow 6 pt ring (`text_faint`) otherwise.
  Shortcut on hover only. Selection: `row_selected` + a 2 pt `accent` bar inset 5 pt top and bottom, radius 1; text
  and marks keep their own colours.
- **Cards:** panes and the right sidebar; fill `card`, radius 8, 1 pt `card_border` (focused pane in a split:
  `accent`), shadow (0, 1) blur 6. Left sidebar on the canvas, no right border. Gaps 6 pt everywhere (window
  top/right/left-when-hidden, sidebar → card, card ↔ card, card → status bar). Split divider = the 6 pt gap, 2 pt
  accent line in its middle on hover/drag. Terminal and file views paint their background with 8 pt **bottom**
  corner radii; the veil is rounded the same. Browser pages stay square.
- **Status bar:** 22 pt, full width, on the canvas, 11 pt text, 14 pt between items, 10 pt end padding. Left:
  mark (10 pt) + `Working · 4m` / `Needs input · 2m` (ring_waiting) / `Done · 1m` / `Running` / `Ready` / `Idle`;
  branch glyph + branch (SSH: `ssh host`); folder (text_faint, start-cut). Right: port chips (`:3000`, 16 pt, pad 6,
  radius 4, `row_hover` / `row_active`), max 3 then `+N`; the update pill (16 pt, radius 8, `UPDATE_FILL` /
  `UPDATE_HOVER`, white 11 pt medium, 12 pt package glyph, gap 6). Drop order: update version → `N ports` → branch
  to 8 chars → folder start-cut → status word → folder → branch.
- **Tab bar:** 30 pt in `tab_bar`; tabs 4 pt from the top, top radius 6, pads 8/4, gap 6, widths 90–200; active =
  `card`, `text`; inactive no fill, `text_dim`, hover `tab_hover`; waiting inactive `waiting_tint`; no top accent
  line. Buttons: `+` (New terminal tab (⌘T)), `⋯`, right sidebar (as M11g), 20 pt, 2 pt apart, 4 pt pad. ⋯ menu:
  200 pt wide, 26 pt items: "New Terminal Tab ⌘T", "New Browser Tab", "Split Right ⌘D", "Split Down ⇧⌘D".
- **Files:** root header 40 pt (line 1 20 pt: ▾/▸ + name 13 pt semibold + collapse-all button; line 2 14 pt: path
  11 pt `text_faint` start-cut); `INDENT` 14; icon 12 pt + 6 pt gap; guides 1 pt `guide` at each ancestor level's
  chevron centre; type tints per the note's table (ANSI 1–6, else `text_dim`), pushed to `MARK_MIN` on the card.
- **Animations / Reduce motion:** the status bar's mark animates only when the sidebar's would; nothing new slides.
- Every visual change has ui-shots in **Catppuccin Mocha and Latte**. Existing tests keep passing; scenes whose
  pixels or geometry change are updated in the task that changes them (named per task).
- Subagents never `cargo run` the app. Allowed: `cargo test`, `cargo clippy --all-targets` (both feature sets),
  `cargo fmt --check`, `cargo run --features ui-shots -- __ui-shots [filter]`, `scripts/bundle-mac.sh --app-only`.

## Review Focus

1. **Content in rounded corners.** A full-screen program painting a coloured background (`htop`, vim with a
   theme), a selection reaching the last row, a file tab scrolled to its end. Expected: the card's bottom corners
   stay round (only browser pages are square). (Task 4: ui-shot `cards-corners` fills the grid with a blue
   background and asserts the pixel 2 pt inside each bottom corner is the canvas colour, in Mocha and Latte.)
2. **Odd `ps` output and slow or missing tools.** Arguments with spaces or `\012`, `tpgid` 0, a zombie job leader
   gone between runs, `ps` exiting non-zero, `lsof` missing, the 2 s cap. Expected: names fall back by the rules
   (folder), no chips, no UI stall, no log spam. (Task 2: `parse_ps` / `foreground` cases; Task 7: `parse_lsof`
   cases and the 30 s back-off.)
3. **Name flicker.** A shell loop running short commands (`for f in *; do wc $f; done`), `make` spawning many
   children, an agent exiting to the prompt. Expected: steady names; `claude` appears at once. (Task 2: `Stable`
   tests for both paths.)
4. **Narrow windows and long text.** 720 pt window, 180 pt sidebar, a 40-character branch, a deep folder, 6 ports,
   an update pending. Expected: the status bar drops pieces in the stated order, nothing overlaps, the update pill
   stays whole; rows and tabs truncate. (Task 7: `fit_status_bar` drop-order tests + ui-shot `status-bar-narrow`;
   Task 3: ui-shot `sidebar-rows-narrow` updated.)
5. **Every theme, light or dark, and user themes.** Expected: `text` on `row_selected` / `card_selected` ≥ 4.5:1,
   `card_border` and `guide` ≥ 1.4:1, file-type tints ≥ 3:1 on the card. (Task 1: a test over every built-in
   theme; Task 6: tints over every built-in theme.)

---

### Task 1: Tokens, colours and terminal padding

**Files:** Modify `src/ui_tokens.rs` (`size`), `src/theme.rs` (`UiColors`, `Theme::ui`), `src/terminal_view.rs`
(`inset` doc comment; tests `inset_balances_leftover`, `hit_testing_uses_the_inset_origin`), `src/ui_shots.rs`
(`tokens` scene lists the new colours). `SIDEBAR_ROW` / `SIDEBAR_ROW_TWO` (replacing `WAITING_ROW`) and `TAB_BAR`
change in the tasks that draw them (3 and 5), so this task moves no row or bar.

**Interfaces — Produces:** the constants and `UiColors` fields in Global Constraints (`card`, `card_border`,
`card_shadow_alpha: f32`, `row_selected`, `card_hover`, `card_selected`, `tab_bar`, `tab_hover`, `guide`).

- [ ] **Step 1: failing tests:**
  - `new_surfaces_keep_text_readable_on_every_theme`: for every built-in theme, `contrast(ui.text, s) ≥ 4.5` and
    `contrast(ui.text_dim, s) ≥ 4.5` for `s` in `row_selected`, `card_selected`, `card_hover`, `tab_bar`,
    `tab_hover`;
  - `card_edges_and_guides_are_quiet_but_visible`: `contrast(card_border, sidebar_background) ≥ 1.4`,
    `contrast(card_border, background) ≥ 1.4`, `contrast(guide, background) ≥ 1.4`, each `< 3.0` on Mocha;
  - `selection_is_a_tint_of_the_accent`: on Mocha, `row_selected != sidebar_background` and its hue is within 15° of
    `accent`'s; `card_shadow_alpha` is 0.30 on Mocha and 0.08 on Latte;
  - `inset_balances_leftover` and `hit_testing_uses_the_inset_origin` updated to 16/12 and 12/10.
- [ ] **Step 2: run** `cargo test` → FAIL. **Step 3: implement.** **Step 4: run** `cargo test`, clippy (both), fmt,
  `__ui-shots tokens` and `__ui-shots window` (the window scenes change by the padding; update their checks).
- [ ] **Step 5: commit** `feat(m11i): design tokens for cards, selection and tab bar; more terminal padding`.

### Task 2: Smart names (pure core, lookups, wiring)

**Files:** Create `src/procs.rs`, `src/smart_name.rs`; Modify `src/main.rs` (`mod`s), `src/workspace.rs`
(`Programs` field, `terminal_label`, `workspace_label`, the rename comparison in `on_name_event`, the ticker
`tick_programs`, called from `render` beside `tick_places`), `src/git_branch.rs` (`refresh_places` looks up every
named-on-screen terminal: rows' named terminals, the shown workspace's terminal tabs, `right_sidebar.followed`),
`src/tab_bar.rs` (`TerminalFacts.label` from `terminal_label`; `tip` adds the OSC title on a second line when it
differs), `src/sidebar.rs` (`sidebar_rows` labels; row tooltip), `src/switcher.rs`, `src/title_area.rs`, the
window's banners' label call site in `src/workspace.rs` (`label = session.label()` at the notification), and
`src/ui_shots.rs` (scene `smart-names`; update `default-name`). The window's own banners' label (the
`let label = session.label()` near `workspace.rs:1540`, the status-change banner) and the tab rename box's prefill
(near `workspace.rs:1955`) move to `terminal_label` too.

**Interfaces — Produces:**
- `procs.rs`: `pub struct Proc { pub pid: u32, pub ppid: u32, pub pgid: u32, pub tpgid: u32, pub args: String }`;
  `pub fn parse_ps(stdout: &str) -> Vec<Proc>` (skips malformed lines); `pub fn foreground(procs: &[Proc], shell:
  u32) -> Foreground` with `pub enum Foreground { Prompt, Program(String /*args*/), Unknown }` (Unknown: the shell
  isn't in the table); `pub fn descendants(procs: &[Proc], root: u32) -> Vec<u32>` (excludes `root`);
  `pub fn ps_table() -> Option<Vec<Proc>>` (runs `/bin/ps -A -o pid=,ppid=,pgid=,tpgid=,args=`, capped 2 s;
  blocking, call off-thread).
- `smart_name.rs`: `pub fn program_name(args: &str) -> String`; `pub fn is_known_agent(name: &str) -> bool`;
  `pub fn folder_name(folder: &Path, home: Option<&Path>) -> String`;
  `pub struct NameFacts<'a> { pub user: Option<&'a str>, pub program: ProgramState, pub folder: Option<&'a Path>,
  pub home: Option<&'a Path>, pub ssh_host: Option<&'a str>, pub preset: Option<&'a str>, pub title: Option<&'a
  str> }` with `pub enum ProgramState { Running(String), Prompt, Unknown }`;
  `pub fn smart_name(f: &NameFacts) -> String`;
  `pub struct Stable { shown: ProgramState, pending: Option<(ProgramState, u8)> }` with
  `pub fn new() -> Stable`, `pub fn offer(&mut self, seen: ProgramState) -> bool` (true when `shown` changed) and
  `pub fn shown(&self) -> &ProgramState`.
- `workspace.rs`: `pub(crate) struct Programs { by_terminal: HashMap<u64, Stable>, table: Option<Rc<Vec<Proc>>>,
  pub frozen: bool, .. }` with `pub fn state(&self, id: u64) -> ProgramState`, `pub fn table(&self) ->
  Option<Rc<Vec<Proc>>>` and (ui-shots) `pub fn set_for_shots(&mut self, id: u64, state: ProgramState)`;
  `pub(crate) fn terminal_label(&self, id: u64) -> SharedString`; `pub(crate) const NAMES_REFRESH: Duration = 3 s`.

- [ ] **Step 1: failing tests:**
  - `parse_ps`: the five-column lines from this Mac (`39856 39855 39856 39882 -/bin/zsh`, `39882 39856 39882 39882
    claude`), args with spaces and `\012`, a short line and a non-numeric pid skipped, empty input;
  - `foreground`: shell pgid == tpgid → `Prompt`; tpgid 0 → `Prompt`; tpgid = another group → `Program(args of the
    leader)`; leader gone but a member of the group present → that member's args; shell missing → `Unknown`;
  - `descendants`: a three-level tree, a cycle-free result without the root, siblings outside the tree excluded;
  - `program_name` table: `claude` → `claude`; `node /opt/homebrew/lib/node_modules/@anthropic-ai/claude-code/cli.js`
    → `claude`; `node /usr/local/bin/npm run dev` → `npm run dev`; `pnpm dev` → `pnpm dev`; `cargo test --release`
    → `cargo test`; `/usr/bin/vim src/main.rs` → `vim`; `python3 -m http.server 8000` → `python3`;
    `python3 manage.py runserver` → `manage`; `bash ./build.sh` → `build`; `bash` → `bash`; `-zsh` → `zsh`;
    `codex --full-auto` → `codex`; a 40-char name → 24 chars ending `…`; `foo\012bar` → `foo`;
  - `smart_name`: each rule in order (user wins over a running program; `claude · fuxx-cli`; `fuxx-cli` at a
    prompt; `~` at home; SSH `host` then `host · src`; Unknown with preset → `claude · fuxx-cli`, without → folder;
    nothing → title; nothing at all → `╭∩╮`); a blank user name counts as none;
  - `Stable`: Prompt → `Running("make")` once → not shown, twice → shown; Prompt → `Running("claude")` → shown at
    once; `Running("claude")` → Prompt → shown at once; alternating A/B/A/B never changes it;
  - `terminal_label` uses the smart name, while `Session::label()` (API) is unchanged (an existing `workspace_api`
    test still passes);
  - Enter on an untouched smart name keeps the terminal and the workspace automatic.
  - ui-shot `smart-names` (Mocha, Latte), with `Programs::set_for_shots` and fixed folders: tabs and rows read
    `claude · fuxx-cli`, `npm run dev · invitee`, `fuxx-cli`, `~`, a renamed `api` tab; the SSH `devbox · src` tab;
    a browser tab's page title. `default-name` updated: ╭∩╮ only for a terminal with no folder.
- [ ] **Step 2: run** → FAIL. **Step 3: implement** (`ps_table` via `run_capped_outcome(Command::new("/bin/ps"),
  None, GIT_CAP)`; the ticker as `tick_places`: a `programs_ticking` flag, stops when nothing is wanted).
  **Step 4: run** `cargo test`, clippy (both), fmt, full `__ui-shots` (update the scenes whose labels change:
  `sidebar`, `tab-bars`, `switcher`, `title-area`, `banner-click`).
- [ ] **Step 5: commit** `feat(m11i): smart names — what runs, or the folder; ╭∩╮ only as a last resort`.

### Task 3: Compact rows and the subtle selection

**Files:** Modify `src/sidebar.rs` (`row_height`, `render_workspace_row`, `close_slot` (the idle mark), `on_fill`
and `needs_input_pill` removed, `Sub` drawing, `NAME_LINE` 17, `SUB_LINE` 13, the shortcut's `group_hover`),
`src/status_mark.rs` (an idle variant: `pub fn idle_mark(running: bool, ui: &UiColors) -> impl IntoElement`, 6 pt
dot or ring), `src/ui_shots.rs` (`sidebar`, `sidebar-rows`, `sidebar-marks` updated; new scene
`sidebar-selection`).

**Interfaces — Consumes:** Task 1 tokens; Task 2 `Programs::state` (running → filled dot).
**Produces:** `size::SIDEBAR_ROW = 22.0` and `size::SIDEBAR_ROW_TWO = 32.0` (`WAITING_ROW` removed);
`pub fn row_height(row: &WorkspaceRow) -> f32` = `SIDEBAR_ROW_TWO` when `line_two(row).is_some()`,
else `SIDEBAR_ROW`; `pub enum LineTwo { NotLive, Waiting(Option<String>), Note(String), Place }` and
`pub fn line_two(row: &WorkspaceRow) -> Option<LineTwo>`; `WorkspaceRow.running: bool`.

- [ ] **Step 1: failing tests:**
  - `a_row_is_22_with_one_line_and_32_with_two`: no place, no sub → 22; place only → 32; waiting with place and
    question → 32 (was 56); lost → 32;
  - `line_two_prefers_not_live_then_waiting_then_note_then_place`;
  - the existing `a_row_grows_a_line…` test rewritten to the new heights; `the_first_nine_rows_show_their_shortcut`
    still passes (the model keeps `shortcut_label`).
  - ui-shots (Mocha, Latte): `sidebar` / `sidebar-rows` / `sidebar-rows-narrow` (180 pt) at the new heights;
    `sidebar-selection`: the selected row's pixel at x = row left + 1 is `accent`, at the row's centre is
    `row_selected`, its name pixels are `text`-coloured (not `on_accent`); a waiting selected row keeps its waiting
    mark colour; hover on another row shows the shortcut and `row_hover`; an idle row with a running program shows
    the dot, at a prompt the ring.
- [ ] **Step 2: run** → FAIL. **Step 3: implement.** **Step 4: run** `cargo test`, clippy (both), fmt,
  `__ui-shots sidebar` and the full set.
- [ ] **Step 5: commit** `feat(m11i): compact two-line sidebar rows, a tinted selection with an accent bar`.

### Task 4: Floating panels

**Files:** Modify `src/workspace.rs` (`render`: canvas background, 6 pt margins, the drag strip above the cards;
`PaneColors` gains `card`, `card_border`, `focus`, `shadow_alpha`), `src/panes.rs` (`render_node`: a leaf is a card;
a split's line is `CARD_GAP` wide with no fill; the accent offset scaled by the gap; the veil `rounded_b`),
`src/sidebar.rs` (`render_sidebar`: no `border_r`), `src/right_sidebar.rs` (`render_right_sidebar`: a card; its
resize strip in the gap; `drawn_width` / `PANE_COLUMN_MIN` count the gaps), `src/terminal_view.rs` (`paint_grid`
background with bottom corner radii `CARD_RADIUS`), `src/file_tab.rs` (its background the same; 12 pt top room),
`src/ui_shots.rs` (new `cards`, `cards-split`, `cards-corners`; update `splits`, `right-sidebar`, `workspaces`,
`cached-panes`, `window`, `sidebar-button`).

**Interfaces — Produces:** `pub fn card(colors: &PaneColors, focused_ring: bool) -> Div` in `panes.rs` (fill,
radius, edge, shadow); `pub fn accent_offset(ratio: f32, gap: f32) -> f32` (pure: where the 2 pt line sits in the
grab strip); `pub fn drawn_width(width, window_width, left)` keeps its signature, now reserving `PANE_COLUMN_MIN +
3 * CARD_GAP`.

- [ ] **Step 1: failing tests:**
  - `accent_offset` at ratio 0.5 centres the 2 pt line in a 6 pt gap, at 0.25 and 0.75 stays inside the gap;
  - `a_narrow_window_keeps_the_panes_200_wide` updated for the gaps;
  - `divider_active` tests unchanged.
  - ui-shots (Mocha, Latte): `cards`: the pixel at (sidebar width + 3, 100) is the canvas, at (sidebar width + 6 +
    20, 100) is the card; the right sidebar open is a card with a 6 pt gap on both sides; `cards-split`: two cards
    with a 6 pt canvas gap; the focused one's edge pixel is `accent`, the other's is `card_border`; a hovered gap
    shows the 2 pt accent line; `cards-corners` (Review Focus 1): a terminal filled with `ESC[44m` + clear, the
    pixel 2 pt inside each bottom corner is the canvas; a lone pane has no `accent` edge pixel.
- [ ] **Step 2: run** → FAIL. **Step 3: implement.** **Step 4: run** `cargo test`, clippy (both), fmt, full
  `__ui-shots`.
- [ ] **Step 5: commit** `feat(m11i): panes and the right sidebar as rounded cards with gaps and a focus ring`.

### Task 5: The cleaner tab bar and the ⋯ menu

**Files:** Modify `src/tab_bar.rs` (`look`, `render_tab`, `render_tab_bar`, `BarButton`, `render_button`,
`Glyph::{More, Plus (shared with the footer)}`), create `src/pane_actions.rs` (the menu: model + drawing, modelled on
`pane_tab_menu.rs`), `src/workspace.rs` (`overlays!` gains `PaneActions`; `pane_actions: Option<PaneActionsMenu>`;
render), `src/main.rs` (`mod`), `src/ui_shots.rs` (update `tab-bars*`, `tab-drag`, `overlays`; new
`tab-bars-more`).

**Interfaces — Produces:** `BarButton { NewTerminal, More, RightSidebar }` with `ALL: [BarButton; 3]` and
tooltips `"New terminal tab (⌘T)"`, `"More"`, `"Toggle right sidebar (⌥⌘\\)"`; `pub enum PaneAction {
NewTerminal, NewBrowser, SplitRight, SplitDown }` with `pub const ALL: [PaneAction; 4]`, `pub fn label(self) ->
&'static str`, `pub fn keys(self) -> Option<&'static str>` (`⌘T`, none, `⌘D`, `⇧⌘D`);
`Workspace::open_pane_actions(pane: PaneId, at: Point<Pixels>, window, cx)` and `run_pane_action(pane, action,
window, cx)`; `look(ui, status, active, pane_focused) -> Look` loses its `top_accent` field and gives the active
tab `fill: Some(ui.card)`. `TAB_BAR` becomes 30 and `TAB_TOP_GAP` 4 in this task.

- [ ] **Step 1: failing tests:**
  - `the_buttons_are_plus_more_and_the_right_sidebar`: `BarButton::ALL` order and tooltips;
  - `the_menu_lists_the_pane_actions_with_their_keys`: `PaneAction::ALL` labels and keys exactly as the constraints;
  - `the_active_tab_takes_the_card_colour_and_has_no_top_line` (replaces
    `the_active_tab_is_raised_and_lit_only_in_the_focused_pane`); `a_waiting_tab_is_tinted_active_or_not` updated
    (active waiting = `card`);
  - `Overlay::ALL` contains `PaneActions`.
  - ui-shots (Mocha, Latte): `tab-bars` (30 pt bar, active tab merges: the pixel 1 pt above the bar's bottom inside
    the active tab equals the terminal background); `tab-bars-hover` (× and `tab_hover`); `tab-bars-button`
    (hover maths for 3 buttons); `tab-bars-more`: ⋯ open shows the 4 items under the button, right-aligned, the ⋯
    lit; clicking "Split Right" splits that pane; Escape closes it; `overlays` covers `PaneActions`;
    `tab-bars-narrow` (720 pt).
- [ ] **Step 2: run** → FAIL. **Step 3: implement.** **Step 4: run** `cargo test`, clippy (both), fmt, full
  `__ui-shots`.
- [ ] **Step 5: commit** `feat(m11i): a cleaner tab bar — tabs that merge into the pane, +, ⋯ menu, sidebar toggle`.

### Task 6: The Files tree — icons, guides, root header, collapse all

**Files:** Modify `src/file_tree.rs` (`collapse_all`, `root_collapsed`, the per-root memory), `src/right_sidebar.rs`
(`file_row`: icon, guides, `INDENT` 14; `render_files_panel`: the root header above the filter; hover/selected use
`card_hover` / `card_selected`), `src/tab_bar.rs` (`Glyph::{Folder, FolderOpen, CollapseAll}`), `src/ui_shots.rs`
(update `files`, `files-filter`, `files-bare`; new `files-header`).

**Interfaces — Produces:** in `file_tree.rs`: `pub fn collapse_all(&mut self)`, `pub fn set_root_collapsed(&mut
self, collapsed: bool)`, `pub fn root_collapsed(&self) -> bool`, `rows()` returns nothing while the root is
collapsed; `expanded_set` / `restore_expanded` carry `root_collapsed` (a `TreeMemory { expanded: HashSet<PathBuf>,
root_collapsed: bool }`). In `right_sidebar.rs`: `pub enum FileKind { Red, Green, Yellow, Blue, Magenta, Cyan, Plain }`,
`pub fn file_kind(name: &str) -> FileKind` (the note's table, case-insensitive extension; dotfiles and no extension
→ Plain) and `pub fn kind_color(kind: FileKind, theme: &Theme) -> Rgb` (ANSI 1–6 pushed to `MARK_MIN` on `card`;
Plain → `text_dim`); `pub fn guides(depth: usize) -> Vec<f32>` (x of each guide: `8 + d * 14 + 6` for d in
0..depth).

- [ ] **Step 1: failing tests:**
  - `collapse_all_closes_every_folder_and_keeps_listings` (re-expanding needs no new listing: `toggle` returns no
    `Want`); the selection moves to its top-level ancestor; the root stays open;
  - `a_collapsed_root_shows_no_rows` and the memory restores it after a re-root and back;
  - `file_kind` for every row of the table, `Cargo.LOCK` (case), `.gitignore`, `Makefile`, `archive.tar.gz` (gz →
    Plain);
  - `kind_color` reaches 3:1 on `card` for every built-in theme (Review Focus 5);
  - `guides(0)` empty, `guides(3)` = [14, 28, 42].
  - ui-shots (Mocha, Latte): `files-header`: the root header with name, path (start-cut at a 300 pt sidebar for a
    long path), collapse-all button; after a click on it every folder is closed; a click on the name hides the tree;
    `files`: folder and file icons with tints, guides at depth 1–2, marks unchanged.
- [ ] **Step 2: run** → FAIL. **Step 3: implement.** **Step 4: run** `cargo test`, clippy (both), fmt,
  `__ui-shots files` and the full set.
- [ ] **Step 5: commit** `feat(m11i): Files tree — type icons, indent guides, a root header and collapse all`.

### Task 7: The status bar, ports, and the update button moved

**Files:** Create `src/status_bar.rs` (model, fitting, drawing); Modify `src/procs.rs` (`parse_lsof`,
`listening_ports`), `src/workspace.rs` (`render`: the bar under the row of sidebars and cards; `ports` state;
`tick_ports`), `src/sidebar.rs` (`render_update_button` removed from the sidebar; `visible_statuses` includes the
status bar's terminal), `src/tab_bar.rs` (`Glyph::Branch`), `src/main.rs` (`mod`), `src/ui_shots.rs` (new
`status-bar`, `status-bar-narrow`, `status-bar-ports`; `update-button` scenes now shoot the status bar).

**Interfaces — Consumes:** Task 2 (`Programs::state`, `Programs::table`, `descendants`), `right_sidebar.followed`,
`Places`, `updater::button`, `update_fits`, `browser_in`.
**Produces:**
- `procs.rs`: `pub fn parse_lsof(stdout: &str) -> Vec<u16>` (from `-Fpn`: `n*:3000`, `n127.0.0.1:5173`,
  `n[::1]:8080` → ports; sorted, deduped); `pub fn listening_ports(pids: &[u32]) -> Option<Vec<u16>>` (runs
  `/usr/sbin/lsof -nP -iTCP -sTCP:LISTEN -a -p <comma list> -Fpn`, capped 2 s; None = failed; an empty pid list →
  `Some(vec![])` without running).
- `status_bar.rs`: `pub enum StatusWord { Working, NeedsInput, Done, Running, Ready, Idle }` and
  `pub fn status_word(status: Status, program: &ProgramState) -> StatusWord`; `pub fn word_text(word: StatusWord,
  age: Option<Duration>) -> String` (`Working · 4m`, `Needs input · 2m`, `Done · 1m`, `Running`, `Ready`,
  `Idle`); `pub struct Measured { pub status_word: f32, pub branch: f32, pub branch_cut: f32, pub folder: f32,
  pub folder_min: f32, pub ports: f32, pub ports_collapsed: f32, pub update_wide: f32, pub update_narrow: f32 }`
  and `pub struct Shown { pub update_wide: bool, pub ports: PortsShown, pub branch: Cut, pub folder: Cut,
  pub status_word: bool }` with `pub enum PortsShown { All, Collapsed, None }`, `pub enum Cut { Full, Short,
  Hidden }`; `pub fn fit_status_bar(m: &Measured, room: f32) -> Shown` (drops in the constraints' order);
  `pub fn port_chips(ports: &[u16]) -> (Vec<String>, Option<usize>)` (max 3 + overflow count);
  `pub const PORTS_REFRESH: Duration = 5 s`; `Workspace::render_status_bar(ui, window, cx) -> AnyElement`.

- [ ] **Step 1: failing tests:**
  - `parse_lsof`: IPv4, IPv6 and `*` forms of the same port merge; a `p` line alone; garbage lines skipped; empty;
  - `listening_ports(&[])` is `Some([])` and runs nothing;
  - the 30 s back-off after a failed `lsof` and the reset on success (pure `back_off`-style helper, as
    `right_sidebar::back_off`);
  - `status_word` for each status × program state; `word_text` strings exactly;
  - `fit_status_bar`: plenty of room → everything; then each step of the drop order in turn as `room` shrinks; the
    status mark and the (narrow) update pill always fit (Review Focus 4);
  - `port_chips(&[3000, 5173, 8080, 9000, 9229])` → three chips and `Some(2)`.
  - ui-shots (Mocha, Latte): `status-bar`: a working terminal (`Working · 4m`, branch `main`, `~/Kerja/proj/fuxx-cli`);
    a waiting one (`Needs input` in `ring_waiting`); a file tab focused keeps the terminal's facts;
    `status-bar-ports`: three chips and `+2`, hover fill; clicking `:3000` opens a browser tab at
    `http://localhost:3000`; `status-bar-narrow` (720 pt, 40-char branch, deep folder, 6 ports, update pending): no
    overlap (the pill's bounds and the folder's don't intersect); `update-*`: the pill at the bar's right end, its
    narrow label at 720 pt, nothing above the sidebar footer; Reduce motion: the mark is the still variant.
- [ ] **Step 2: run** → FAIL. **Step 3: implement.** **Step 4: run** `cargo test`, clippy (both), fmt, full
  `__ui-shots`, and `scripts/bundle-mac.sh --app-only` (a release build; report the `ps` and `lsof` run times from
  the log at debug level).
- [ ] **Step 5: commit** `feat(m11i): a status bar — status, branch, folder, listening ports, and the update button`.

### Task 8: Merge and push (your word) — 👀

- [ ] On your word: merge `m11i-ui-uplift` into `master` (all tests, clippy both, fmt, full `__ui-shots` green
  first), push.

### Task 9: Release (your word) and on screen — 👀

- [ ] On your word: version **0.5.0** in `Cargo.toml` (a new look), `cargo test`, commit
  `chore(release): 0.5.0 — UI uplift 2 (M11i)`, push, tag `v0.5.0`, watch the Release run to green.
- [ ] You: the 20-item on-screen checklist in note 60.

### Task 10: Docs

- [ ] Note 60 (status, an "As built" section for what the build settled, the checklist's state). This plan (ticks,
  header). The roadmap. The Plan Tracker. The Progress Log. `STRUCTURE.md` (`procs.rs`, `smart_name.rs`,
  `status_bar.rs`, `pane_actions.rs`). `README.md` (smart names, the status bar, ports).
- [ ] Commit `docs(m11i): …`.
