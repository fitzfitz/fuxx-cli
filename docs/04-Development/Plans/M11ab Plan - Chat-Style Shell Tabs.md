# M11ab — Chat-Style Shell Tabs Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A zsh shell tab in card view shows each finished command as a message card painted from its grid lines,
with the live terminal in a framed box at the bottom that grows to the whole pane.

**Architecture:** The tab keeps its one alacritty grid. `blocks.rs` lists every tagged segment (cards, empty
prompts, the live one) with start times and durations; a pure model (`card_view.rs`) turns them into a column of
items (earlier rows, cards, loose rows, the live box); `TerminalView` renders that column in a `gpui::list` when card
mode is on, painting each item's grid lines with the line painter split out of `paint_grid`, and sizes the terminal
to the live box's inside.

**Tech Stack:** Rust, GPUI (pinned rev; `gpui::list` / `ListState` with `FollowMode::Tail`, as `agent_tab.rs`),
`alacritty_terminal` (grid, `Line`, hyperlink tags).

**Spec:** `docs/04-Development/88 - Milestone 11ab - Chat-Style Shell Tabs.md` (approved 2026-11-20; read §1–§9 and
"Settled after the builder's read").

## Global Constraints

- Branch `m11ab-shell-cards`. Gate per task: `cargo test --bin fuxx`, `cargo clippy --all-targets` and
  `cargo clippy --all-targets --features ui-shots` (no new warnings), `cargo fmt --check`. Screenshot scenes
  (`scripts/ui-shots.sh`) only in Task 7, then the whole-branch review.
- Card view needs: `shell-cards` on (default on), the tab's zsh started with fuxx's integration, not on the
  alternate screen, and a pane at least `CARD_MIN_WIDTH` = 260 pt wide. Otherwise today's terminal, unchanged.
- Fold: an output longer than **20** lines shows its **last 12**, with `▸ N earlier lines` as its first row;
  unfolded, `▾ Show less`.
- Header: start time `HH:MM` (`activity::clock_hm`), ` · ` duration, ` · exit N` in `block_failed` when not 0; a part
  the tag lacks is left out.
- Rerun keeps M11l's rule: types the command, never presses Return, only at an empty prompt.
- The live box: rows after the newest card down to the lower of the cursor row and the last non-blank row; with
  no card, every row. The column is bottom-anchored. The live box's frame and the chips row always show in card
  view; the terminal's rows and columns are the box's inside at its tallest.
- Empty prompts (an empty Return, ⌃C on a half-typed line) are hidden in card view.
- No full-scrollback walk per frame: the column is rebuilt on a mark, a resize, a clear, a setting or alt-screen
  change only.
- Colours from `UiColors` / `ui_tokens.rs`, held to M11's floors (`TEXT_MIN`, `MARK_MIN`).
- Never run a program named `fuxx`; test the app only with `FUXX_DAEMON_SOCKET=/tmp/fuxx-dev.sock cargo run`.

## Plan-time rulings (deviations from the spec's letter, for your check)

1. **The chips show the folder and the branch, no `?N` / `+N` counts** (§3 said "from what the status bar already
   knows": it knows only the branch). Counts would need a new git lookup per terminal; left out.
2. **Durations** (§2's `0.04s`, `3.2s`, `3m 12s`): a new `command_name::card_duration(ms)`: under 1 s two decimals,
   under a minute one decimal, from a minute M11q's `ran_label`.
3. **A tab restored or adopted after a relaunch** doesn't know how its zsh was started: it turns card-capable at
   its first tag and stays so (a one-time switch, Settled 1 otherwise holds).
4. **After reattaching to the background service,** a command that started before the reattach gets no duration
   (the new tagger has no start instant): its card shows the time only.

## Review Focus

1. Claude Code (inline) clearing the scrollback mid-run: every tag gone, the column is one live box, no stale card
   ids, no panic — Task 4's `no_tags_is_one_live_box` and Task 5's `cards_rebuild_after_a_clear`.
2. A 10,000-line output, unfolded: only rows inside the visible bounds are read and painted — Task 5's
   `only_visible_rows_are_read`.
3. Switching `shell-cards` off and on with a program running: one resize each way, the prompt-redraw rule applied,
   the program keeps running — Task 7's `shell-cards-toggle` scene.
4. Typing while scrolled up in card view: the column snaps to the bottom, the keys reach zsh — Task 6's
   `typing_snaps_the_column_to_the_bottom`.
5. The replica (background service) path: tags with `t0`/`ms` survive the snapshot round trip; a pre-reattach
   command shows its time without a duration — Task 1's `times_survive_the_snapshot` and
   `a_reattached_tagger_gives_no_duration`.

---

### Task 1: Times on tags, every segment listed, the card duration

**Files:** Modify `src/blocks.rs` (Tag, encode/decode, Tagger, Block, new `segments`), `src/command_name.rs`
(`card_duration`); tests in both.

**Interfaces — produces:**
- `Tag { …, pub t0: Option<u64>, pub ms: Option<u64> }`; URI fields `&t0=<unix secs>` and `&ms=<millis>` (digits).
- `pub struct Now { pub unix: u64, pub at: Instant }` with `Now::current()`; `pub fn advance_at(processor, term,
  tagger, scanner, bytes, now: Now) -> Vec<CommandMark>`; `advance` = `advance_at(…, Now::current())`.
- `ShellState::Running { since: Option<Instant> }` (the C mark's `now.at`); D sets `ms = since.elapsed` in millis.
- `Block { …, pub t0: Option<u64>, pub ms: Option<u64> }`.
- `pub enum Segment { Prompt { id: String, top: Line, bottom: Line }, Command(Block) }` and
  `pub fn segments<L>(term: &Term<L>) -> Vec<Segment>` (oldest first; Prompt-state tags included, each spanning to
  the row before the next tag; empty on the alt screen).
- `pub fn card_duration(ms: u64) -> String` in `command_name.rs`.

- [ ] **Step 1: Failing tests** in `blocks.rs` `mod tests` (use `Shell`; add `Shell::command_at(cmd, out, exit,
  start: Now, end: Now)` feeding through `advance_at`):
  - `a_finished_command_carries_its_start_and_duration`: start `unix 1_795_000_000`, end 3,200 ms later →
    the `done` tag has `t0 == Some(1_795_000_000)`, `ms == Some(3_200)`.
  - `times_round_trip_and_an_old_tag_has_none`: `decode(&encode(&t))` keeps both; a URI without them decodes with
    `None`; `t0=abc` rejects the tag (as `o=abc` does).
  - `times_survive_the_snapshot`: after `vt_snapshot` of the term into a fresh one, `tags()` still has both.
  - `a_reattached_tagger_gives_no_duration`: C through one `Tagger`, D through a new `Tagger::new()` on the same
    term → `ms == None`, `t0` kept.
  - `segments_keep_empty_prompts`: prompt, `ls` (done), empty prompt (A,B then A), `npm test` (done) →
    `[Command(ls), Prompt{..}, Command(npm test), Prompt{..}]` (the last the live prompt).
  - In `command_name.rs`: `card_durations_read_short`: `card_duration(40) == "0.04s"`, `(3_200) == "3.2s"`,
    `(59_900) == "59.9s"`, `(192_000) == "3m 12s"`.
- [ ] **Step 2:** `cargo test --bin fuxx blocks:: command_name::` — Expected: compile errors / FAIL on the new names.
- [ ] **Step 3:** Implement as the Interfaces say; `list` stays as is (its callers unchanged).
- [ ] **Step 4:** Same command — Expected: PASS; then the whole gate.
- [ ] **Step 5:** Commit `M11ab Task 1: start times and durations on command tags; every segment listed`.

### Task 2: The setting, the tab's integration flag, card mode

**Files:** Modify `src/appearance.rs` (`shell_cards`), `src/prefs.rs` (`shell_cards_values`), `src/preferences.rs`
(`OnOff::ShellCards`, a disabled switch), `src/terminal_view.rs` (`integrated` field), `src/workspace.rs` (pass it at
spawn and daemon create); Create `src/card_view.rs` (`card_mode`, `CARD_MIN_WIDTH`), `mod card_view` in `main.rs`.

**Interfaces — produces:**
- `Appearance.shell_cards: bool` from `switch_from_entries(entries, "shell-cards", true)`.
- `prefs::shell_cards_values(on) -> Vec<String>` (nothing when on, `["off"]` when off).
- `OnOff::ShellCards`: key `shell-cards`, label `Commands as cards`, explanation
  `Shows each finished command as a card. Needs Command blocks.`, placed after `CommandBlocks` in Sessions →
  "Terminal". `fn switch_enabled(c: OnOff, a: &Appearance) -> bool` (false for ShellCards while
  `command_blocks` is off); a disabled switch shows off, `text_faint`, no click.
- `pub(crate) fn integrated_in(env: &ShellEnv) -> bool` (env sets `FUXX_ZSH_MARKS=1`); `TerminalView.integrated:
  bool`, set at spawn and for daemon-created sessions from the env they were given; a restored / adopted view
  sets it at its first `command_marked` (ruling 3).
- `pub const CARD_MIN_WIDTH: f32 = 260.0;` `pub fn card_mode(shell_cards: bool, integrated: bool, alt_screen: bool,
  pane_width: f32) -> bool`.

- [ ] **Step 1: Failing tests:** `shell_cards_setting` (appearance: default on, `off` read, a bad value warns);
  `shell_cards_values_write_only_off`; `shell_cards_switch_follows_command_blocks` (preferences: placed right after
  CommandBlocks; disabled while command blocks is off); `integration_is_read_from_the_env`;
  `card_mode_needs_all_four` (each of the four false → false; all true → true; width 259 → false).
- [ ] **Step 2:** Run them — Expected: FAIL / compile errors.
- [ ] **Step 3:** Implement. Update `command_blocks_switch` (it asserted CommandBlocks is last) and the palette
  index test (`all[17]`), keeping what each meant.
- [ ] **Step 4:** Tests pass; whole gate.
- [ ] **Step 5:** Commit `M11ab Task 2: shell-cards setting, a tab's integration flag, card mode`.

### Task 3: Reading and painting any grid lines

**Files:** Modify `src/grid.rs` (extract the cell conversion; `lines_snapshot`), `src/terminal_view.rs` (split
`paint_lines` out of `paint_grid`).

**Interfaces — produces:**
- `fn cell_view(…) -> CellView` (the conversion now inline in `snapshot`, 209–250, moved out unchanged; `snapshot`
  calls it).
- `pub fn lines_snapshot<L: EventListener>(term: &Term<L>, theme: &Theme, top: Line, bottom: Line)
  -> Vec<Vec<CellView>>` (inclusive; scrollback lines too; selection marked as `snapshot` marks it; clamped to the
  grid).
- `fn paint_lines(lines: &[Vec<CellView>], theme: &Theme, m: &CellMetrics, origin: Point<Pixels>, window, cx)`
  (bg rects, then text spans; no fill, no cursor); `paint_grid` calls it between its fill and its cursor.

- [ ] **Step 1: Failing tests** (grid.rs): `lines_snapshot_matches_the_viewport` (a term with coloured, bold and
  wide-char output: `lines_snapshot` over the screen's lines `== snapshot().lines`);
  `lines_snapshot_reads_scrollback` (a line scrolled 30 rows up reads back its text and colours);
  `lines_snapshot_clamps_past_the_grid`.
- [ ] **Step 2:** Run — FAIL.
- [ ] **Step 3:** Implement; `paint_grid`'s output is unchanged (Task 7's existing scenes guard it).
- [ ] **Step 4:** Pass; gate.
- [ ] **Step 5:** Commit `M11ab Task 3: read and paint any range of grid lines`.

### Task 4: The column model

**Files:** Modify `src/card_view.rs` (pure; tests in it).

**Interfaces — consumes:** Task 1's `Segment`, `Block` (`t0`, `ms`), `card_duration`; `activity::clock_hm`,
`cli::local_utc_offset`.
**Produces:**
- `pub enum Item { Earlier { top: Line, bottom: Line }, Card(Card), Loose { top: Line, bottom: Line },
  Live { top: Line, bottom: Line } }`
- `pub struct Card { pub id: String, pub header: Header, pub cmd: Option<String>, pub cmd_rows: (Line, Line),
  pub output: Option<(Line, Line)>, pub failed: bool }`;
  `pub struct Header { pub start: Option<String>, pub took: Option<String>, pub exit: Option<i32> }` with
  `fn words(&self) -> String` (`"10:42 · 3.2s · exit 1"`; missing parts dropped; exit only when not 0).
- `pub fn column(segments: &[Segment], first: Line, cursor: Line, last_text: Line, utc_offset: i64) -> Vec<Item>`:
  done blocks → `Card`; Prompt segments before the live part → dropped (hidden); rows between a card and the next
  segment → `Loose`; rows before the first segment → `Earlier`; everything after the newest card →
  `Live { top, bottom: max(cursor, last_text) }`.
- `pub const FOLD_OVER: usize = 20; pub const FOLD_KEEP: usize = 12;`
  `pub struct Shown { pub hidden: usize, pub top: Line, pub bottom: Line }`;
  `pub fn shown(output: (Line, Line), unfolded: bool) -> Shown`.
- Sizes: `pub const CHIPS_ROW: f32 = 28.0; BOX_PAD: f32 = 10.0; BOX_EDGE: f32 = 1.0;`
  `pub fn card_dims(pane: Size<Pixels>, split: bool, cell_w: f32, line_h: f32) -> TermDims` (the pane less
  `terminal_view::inset`'s padding, less the chips row, less the box's edge and padding on each side).

- [ ] **Step 1: Failing tests:**
  - `cards_for_done_commands_and_empty_prompts_hidden`: segments ls(done, exit 0), empty Prompt, npm test (done,
    exit 1), live Prompt → `[Card(ls), Card(npm test, failed), Live]`, no item for the empty prompt.
  - `no_tags_is_one_live_box`: `column(&[], first, cursor, last)` → `[Live { top: first, bottom: max(..) }]`.
  - `a_running_command_is_in_the_live_box`: …, done, Running → the Running block's rows are inside `Live`.
  - `rows_after_a_card_are_loose`: a gap between a card's last output row and the next tag → `Loose`.
  - `rows_before_the_first_tag_are_earlier`.
  - `the_live_box_ends_at_the_cursor_or_the_last_text`.
  - `header_words`: `Header{start: Some("10:42"), took: Some("3.2s"), exit: Some(1)}.words() ==
    "10:42 · 3.2s · exit 1"`; exit 0 → no exit part; none → `""`.
  - `folding_keeps_the_last_twelve`: 25 lines → `hidden 13`, the last 12 shown; 20 → `hidden 0`; unfolded 25 →
    `hidden 0`.
  - `card_dims_are_the_box_inside`: a 1000 × 700 pane, unsplit, cell 8 × 17 → the columns and rows the formula
    gives (write the numbers in the test from the constants).
- [ ] **Step 2:** Run — FAIL. **Step 3:** Implement. **Step 4:** Pass; gate.
- [ ] **Step 5:** Commit `M11ab Task 4: the card view's column model`.

### Task 5: Card view in the terminal

**Files:** Modify `src/terminal_view.rs` (card state, render, sizing, scroll), `src/card_view.rs` (element
helpers); ui-shots hooks beside the existing ones.

**Interfaces — consumes:** Tasks 2–4. **Produces:**
- `struct CardState { list: ListState, items: Vec<Item>, unfolded: HashSet<String>, clears: u64 }` in
  `TerminalView.cards: Option<CardState>` (Some while card mode is on).
- `fn refresh_cards(&mut self, cx)`: rebuilds `items` from `blocks::segments` + `column` (off the UI thread when the
  grid is long, as `copy_block` reads), `list.reset(items.len())`; called from `command_marked`, after a resize,
  when `GridSnapshot.clears` changes, on an Appearance change and on entering / leaving the alt screen.
- Render: card mode → a bottom-following `list(…)` (`FollowMode::Tail`, as `agent_tab.rs`) of items:
  `Earlier` / `Loose` = plain lines; `Card` = header (UI font `text::META`, `text_faint`; exit in `block_failed`),
  `❯ cmd` (terminal font), panel (radius 8, 1 pt edge `card_border`, `block_failed` when failed, 10 pt padding)
  painting `shown()` lines through `paint_lines` after `lines_snapshot`, the fold row when hidden > 0;
  `Live` = the chips row (`CHIPS_ROW` tall) then the framed box painting its rows with the cursor. Otherwise
  today's render, untouched.
- Sizing: in card mode prepaint resizes the core to `card_dims` instead of `inset` → `dims_for`.
- Scrolling: in card mode the core stays at `display_offset` 0; the wheel scrolls the list, except over the live
  box when the program wants the mouse (`mouse::wants_mouse`), which keeps today's path.
- Only rows inside the visible bounds are read and painted (`lines_snapshot` over the clipped range).

- [ ] **Step 1: Failing tests** (a test rig in `terminal_view.rs` tests, or `card_view.rs` with a `Shell`-like
  fixture): `cards_rebuild_after_a_clear` (two cards, then `ESC [3J` + a prompt → items `[Live]`);
  `only_visible_rows_are_read` (a pure `fn visible_rows(item_top_px, rows, line_h, clip) -> Range<usize>` the
  paint uses; a 10,000-row card clipped to 600 px reads ≤ 600/line_h + 2 rows); `card_mode_resizes_to_the_box`
  (the dims passed to the core equal `card_dims`).
- [ ] **Step 2:** Run — FAIL. **Step 3:** Implement. **Step 4:** Pass; gate.
- [ ] **Step 5:** Commit `M11ab Task 5: the card view (cards, the live box, sizing, scrolling)`.

### Task 6: Card view interaction

**Files:** Modify `src/terminal_view.rs`, `src/card_view.rs`, `src/command_name.rs` (`CommandRun::started`),
`src/workspace.rs` (push the branch chip).

**Interfaces — produces:**
- `pub fn card_point(pos: Point<Pixels>, panel_origin: Point<Pixels>, top: Line, m: &CellMetrics, cols: usize)
  -> (Point, Side)` (pixel in a card's panel → grid point), used for selection (within one card) and ⌘-click links.
- Hover actions under a card: `Copy` (→ `copy_block(id, false)`), `Rerun` (→ `rerun_block(id)`, enabled as M11l);
  the fold row toggles `unfolded`.
- Keys in card mode: ⇧⌘↑ / ⇧⌘↓ scroll the list to the chosen card (`list.scroll_to_reveal_item`) and set
  `chosen_block` (panel edge in `accent`); ⇧⌘C as today on the chosen / newest card. Typing (`user_typed`) scrolls
  the list to the bottom.
- The `❯ cmd` line starts M11aa's `BlockDrag` (the chip's `on_drag`, same payload and preview).
- Chips: folder from `reported_cwd()` (home-relative, `sidebar::home_relative`); branch via
  `TerminalView::set_branch(Option<String>)`, pushed by the workspace from `places.branch(folder)` beside
  `set_status`; while running, first `● <short> · <elapsed>` in `block_running` (`CommandRun::started() ->
  Option<Instant>`; a 1 s repaint only while running; elapsed through `card_duration`, whole seconds).

- [ ] **Step 1: Failing tests:** `card_point_maps_a_panel_pixel_to_its_grid_line` (top line −40, pixel at row 3 col
  5 → `Point(Line(-37), Column(5))`); `typing_snaps_the_column_to_the_bottom` (a pure `fn after_typing(…)` or the
  rig: scrolled up, `user_typed` → at bottom); `elapsed_words` (`started` 75 s ago → `"● npm test · 1m 15s"`);
  `branch_chip_comes_from_the_workspace`.
- [ ] **Step 2:** Run — FAIL. **Step 3:** Implement. **Step 4:** Pass; gate.
- [ ] **Step 5:** Commit `M11ab Task 6: selecting, links, actions, keys and chips in card view`.

### Task 7: Scenes, docs, review

**Files:** Create `src/ui_shots/cards.rs` (area `cards`; `mod` in `ui_shots/mod.rs`, area in `registry.rs`
`AREAS`, names in `ORDER`); Modify note 88 ("M11ab as built", on-screen checklist), `40 - Roadmap.md`,
`60 - Progress Log.md`.

- [ ] **Step 1: Scenes** (both themes; feed marks with `shot_output`, as `command_blocks_scene`):
  - `shell-cards`: ok, failed (25 lines), empty prompt, live prompt → two cards (failed panel edge
    `block_failed`; its fold row shows `▸ 13 earlier lines`), no rows of the empty prompt, the live box framed at
    the bottom, the core's dims == `card_dims`.
  - `shell-cards-running-full`: a running command printing 200 lines → the live box fills the pane, no card visible,
    the running chip shows.
  - `shell-cards-alt-screen`: enter the alt screen → today's full view (no frame, full-width dims); leave → cards back.
  - `shell-cards-off`: `shell-cards = off` → today's look (M11l gutter bars drawn).
  - `shell-cards-toggle`: a running program; off, on → two resizes, the program still running, no doubled prompt.
- [ ] **Step 2:** Full `scripts/ui-shots.sh` — Expected: the new scenes pass; only the known display-dependent
  scenes may fail.
- [ ] **Step 3:** Whole-branch review (executing-plans' final review, most capable model), one fix pass, the
  screenshot run again.
- [ ] **Step 4:** Docs: note 88 "M11ab as built" + the on-screen checklist; roadmap status "🟨 built, on-screen
  check pending"; Progress Log; commit. Push and merge only on your word.
