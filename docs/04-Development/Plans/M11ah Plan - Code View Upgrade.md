---
tags: [development, plan, milestone]
milestone: M11ah
status: planned
updated: 2026-11-17
---

# M11ah Plan — Code view upgrade

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

| At a glance | |
|---|---|
| **Design (the spec)** | [[85 - Milestone 11ah - Code View Upgrade]] (incl. its "Calls on the open questions") |
| **Status** | 📝 Plan written 2026-11-17, waiting for build |
| **Tasks done** | 0 / 13 (Task 0 – Task 12) |
| **Baselines** | *(Task 0 fills in: test count, clippy bin/test counts default and `ui-shots`, fmt)* |
| **▶ Pick up here** | Task 0 |
| **Decisions used** | Yours: directions A + B + C ("everything, all layout good"), go-ahead on the design. The open questions settled as AI's calls in note 85: jumps are **⌥↑ / ⌥↓** (plain ↑ / ↓ untouched), side by side **off by default, per tab**, breadcrumb **its own 26 pt strip**. Every number below is note 85's (AI's calls, approved with the design). |
| **Needs your eyes** | 👀 the on-screen check in note 85, after Task 12 |

**Goal:** The read-only file tab answers "where am I, where are the changes, what changed": a current line, quieter
numbers, indent guides, rounded bars and roomier rows; a sticky line, a breadcrumb, folding, scrollbar marks and
⌥↑ / ⌥↓ between changes; and in Diff mode cards with titles and counts, changed words marked, unchanged stretches
folded, and a side-by-side option.

**Architecture:** Three new pure modules carry the logic, unit-tested in `cargo test`: `src/code_outline.rs` (blocks
from indentation, closers and Markdown headings, named by syntect scopes; guides, sticky, breadcrumb, folds),
`src/row_map.rs` (the one row list every screen row goes through — parts of lines, a Diff card's title, a folded
stretch, a side-by-side pair — with variable row heights, replacing `Wrapped`; plus change steps), `src/diff_cards.rs`
(cards, unchanged stretches, titles, side-by-side pairs, word marks). The outline is made with each read off the UI
thread (as M11af's compare, 50 ms limit); named lines come with the colours; cards and word marks are made with the
diff, off the UI thread. `src/file_tab.rs` keeps state (current line, folds, the place shown) changed only in event
handlers and reads, and draws from the row map, which stays a layout cache built in the canvas prepaint as `Wrapped`
is today.

**Tech Stack:** Rust 2024, GPUI (pinned rev, unchanged), syntect 5.3 (already used: `ParseState::parse_line` +
`ScopeStack::apply` beside `HighlightState` / `HighlightIterator` in place of `HighlightLines`), **`similar` 3.2.1
(already a dependency) gains its `inline` feature** — read from the 3.2.1 source on 2026-11-17: `inline = ["text"]`;
`TextDiff::configure().algorithm(Algorithm::Myers).deadline(Instant).diff_lines(old, new)`, `TextDiff::ops()`,
`TextDiff::iter_inline_changes_with_options_deadline(&op, InlineChangeOptions, Option<Instant>)` yielding
`InlineChange` with `old_index()`, `new_index()`, `values() -> &[(bool, &str)]` (true: emphasized);
`InlineChangeOptions::new()` then `.mode(InlineChangeMode::Words)` and `.min_ratio(0.5)` (`&mut Self` builders; below
the ratio a change falls back to no emphasis). Task 10 re-checks this with Context7 before touching `Cargo.toml`.

**Spec:** [[85 - Milestone 11ah - Code View Upgrade]]

## Global Constraints

- **Where:** only in the worktree `.worktrees/m11ah-code-view`, branch
  `m11ah-code-view` (from master). This is a **parallel lane**: M11ak is being built at the same time on another branch
  in the main checkout — **never touch the main checkout** or another worktree. Commit per task; merge and push are
  the user's call.
- **Gate per task:** `cargo test --bin fuxx && cargo clippy --all-targets && cargo clippy --all-targets --features
  ui-shots && cargo fmt --check`; then `scripts/ui-shots.sh --changed master`; the full `scripts/ui-shots.sh` before
  the commit of any task that touches drawing. Clippy counts never go above Task 0's baselines.
- **Reference fingerprints** in `tests/ui-shots-ref` are stale on this Mac (they only report "looks different", never
  fail): **never run `--update-refs`**.
- **`chrome-agent-status`** is a known clock-dependent failure ("an age turned during the check"), unrelated to this
  milestone: a failure of it alone does not block a task (rerun it once to confirm).
- **M11p (note 77):** nothing visible changes inside `render` or a canvas prepaint except layout caches keyed by their
  inputs (as `Wrapped` is today: the row map, the lane, the breadcrumb's fit); no new always-on timer, no animation
  (folds open and close at once), no new frame task. State (current line, folds, the place shown, hover) changes only
  in event handlers, reads and answers, and notifies only when it changed.
- **M11af keeps working (note 82):** the glow, the age words, the mark column and Follow, in File mode; its scenes
  (`file-live-*`, `file-follow-*`) pass after every task. Follow reveals below the sticky line and opens a fold the
  agent changed; folding, unfolding and a breadcrumb or sticky click turn Follow off.
- **Read-only viewer, no new services**, offline, no language servers ([[12 - Scope]]). No new crate; the only crate
  change is `similar`'s `inline` feature, checked with Context7 first.
- **Sizes are named constants** in a new `pub mod code` in `src/ui_tokens.rs` (Task 1), so M11al can restyle them
  together; strings are `const`s in `src/file_tab.rs`. Values as note 85: row = font line height × **1.2**, text
  centred; top room **16** (12), numbers' left **16** (12), numbers → bar **10** (8), bar → text **14** (10); fold
  column **14**; current band `text` at **5 %**; guides **1 pt** at **7 %**, the current block's **14 %**; bar **3 pt**,
  fully rounded, **2 pt** gap between different runs; breadcrumb **26 pt**, **12 pt** UI text; lane **4 pt**, marks
  ≥ **2 pt**, match ticks **2 pt**; nav buttons **24 pt**; card inset **12**, border **1 pt**, gap **10**, title row
  **30 pt**, radius `CARD_RADIUS`; word tint ≈ **35 %**, corners **2 pt**; numbers kept ≥ `MARK_MIN` (3:1).
- **Counts and limits:** outline and word marks give up after **50 ms**; breadcrumb part ≤ **32** chars then `…`;
  cards keep **3** lines of context; unchanged stretches fold from **8** lines; side by side needs **60** cells per
  half; jumps land `REVEAL_ABOVE` (3) rows from the top (+1 under a sticky line); no wrap-around.
- **Words (exact):** `⋯ 24 lines` (fold pill); `⋯ 36 unchanged lines — click to show`; `fn refresh` · `+12 −3` ·
  `lines 40–58`; `Top level`; `New file · +120`; `Deleted file · −80`; breadcrumb `session.rs › impl Session › fn
  refresh`; tooltips `Previous change (⌥↑)`, `Next change (⌥↓)`, `Side by side`, `Too narrow for side by side`;
  palette `Fold All`, `Unfold All`, `Show All Unchanged Lines`. `⋯` U+22EF, `—` U+2014, `−` U+2212, `–` U+2013,
  `·` U+00B7 with spaces, `›` U+203A with spaces, `…` U+2026. Counts over 999 use `thousands`.
- **Limits stay:** over 2 MB plain (`PlainTooLarge`: no names, indentation outline only), over 20 MB refused.
- **Example content is made up** (`~/code/my-app`, `src/auth/session.rs`, `fn refresh`); no work-project names in code,
  tests, fixtures or docs.
- **Commits** stage files by path (never `git add -A` / `.`), message `M11ah Task N: …`, **no AI attribution lines**.
- **Docs** (note 85 "What was built" and "On-screen check", status; roadmap; Progress Log; this plan's table;
  `STRUCTURE.md`) are updated in the last task (Task 12).
- Logic is tested in `cargo test`; drawing in ui-shots scenes (`--features ui-shots`), both themes where they check
  colour. Each new scene gets a `Scene` row in `src/ui_shots/file_tab.rs`'s `SCENES` (sources listed) **and** its name
  in `src/ui_shots/registry.rs`'s `ORDER`, after `file-follow-mode`.

## Review Focus

1. **A Markdown file whose fenced code block holds `# comment` lines** (shell snippets): those are not headings; the
   breadcrumb, sticky line and folds follow the real headings only. Pinned in Task 2
   (`fenced_code_hashes_are_not_headings`).
2. **A reload that removes the current line, a folded block's first line, or shrinks the file below a fold:** the
   current line clears, that fold opens, nothing indexes past the end. Pinned in Task 5
   (`reload_drops_folds_and_current_line_that_are_gone`).
3. **Mixed tabs and spaces, or an odd continuation indent** (a 2-space file with a 3-space wrapped argument): tabs are 4
   cells, the step is the most common one, guides are `indent / step` whole levels, never zero-width or past the text.
   Pinned in Task 2 (`tabs_count_four_and_odd_indents_round_down`).
4. **Changed words on non-ASCII text and tabs** (`café` → `cafés`, a tab-indented line): every mark is a byte range on
   character boundaries of its own line, drawn through the tab expansion. Pinned in Task 10
   (`word_marks_sit_on_char_boundaries`).
5. **A click or drag below a fold, a folded stretch or a card title, with Wrap on:** it lands on the line drawn there,
   not on a hidden one. Pinned in Task 4 (`row_at_finds_the_row_drawn_at_y_with_hidden_lines_and_titles`, hidden
   lines and Wrap), Task 9 (`row_at_finds_rows_below_titles_and_stretches`) and Task 5 scene `file-code-fold-keys`
   (a press under a fold selects the line after it).

---

### Task 0: Baselines

**Files:**
- Modify: this plan's "At a glance" (Baselines row)

- [ ] **Step 1:** In the worktree, on a clean `m11ah-code-view`, run `cargo test --bin fuxx` (note "N passed; M
  ignored"), `cargo clippy --all-targets 2>&1 | grep -c '^warning'` split by bin / test as the M11af plan records them,
  the same with `--features ui-shots`, and `cargo fmt --check` (clean or not).
- [ ] **Step 2:** Run the full `scripts/ui-shots.sh`; note passed / failed (expected: all but possibly
  `chrome-agent-status`).
- [ ] **Step 3:** Write the numbers into the Baselines row.
- [ ] **Step 4: Commit** `docs/04-Development/Plans/M11ah Plan - Code View Upgrade.md` — `M11ah Task 0: baselines`.

### Task 1: The `code` sizes, roomier rows, quieter numbers, the current line

**Files:**
- Modify: `src/ui_tokens.rs` (new `pub mod code`)
- Modify: `src/file_tab.rs` — drop `GUTTER_LEFT`, `TOP_ROOM`, `NUMBER_GAP`, `BAR_WIDTH`, `TEXT_GAP` for `code::*`;
  `Geom::of` takes the row height; `paint_body` (band, numbers); `mouse_up`, `find_step`, `loaded`
- Modify: `src/ui_shots/file_tab.rs` (`plain_text_left` and any hard-coded gutter numbers; new scene)

**Interfaces:**
- Produces:
  ```rust
  // src/ui_tokens.rs — every M11ah size (later tasks use these names; values from Global Constraints)
  pub mod code {
      pub const ROW_SCALE: f32 = 1.2;   pub const TOP_ROOM: f32 = 16.0;  pub const GUTTER_LEFT: f32 = 16.0;
      pub const NUMBER_GAP: f32 = 10.0; pub const TEXT_GAP: f32 = 14.0;  pub const BAR_WIDTH: f32 = 3.0;
      pub const BAR_RUN_GAP: f32 = 2.0; pub const FOLD_COLUMN: f32 = 14.0; pub const CURRENT_ALPHA: f32 = 0.05;
      pub const GUIDE: f32 = 1.0; pub const GUIDE_ALPHA: f32 = 0.07; pub const GUIDE_ACTIVE_ALPHA: f32 = 0.14;
      pub const STICKY_RULE: f32 = 1.0; pub const BREADCRUMB: f32 = 26.0; pub const BREADCRUMB_TEXT: f32 = 12.0;
      pub const LANE: f32 = 4.0; pub const LANE_MARK_MIN: f32 = 2.0; pub const LANE_TICK: f32 = 2.0;
      pub const NAV_BUTTON: f32 = 24.0; pub const CARD_INSET: f32 = 12.0; pub const CARD_BORDER: f32 = 1.0;
      pub const CARD_GAP: f32 = 10.0; pub const CARD_TITLE: f32 = 30.0; pub const UNCHANGED_TEXT: f32 = 12.0;
      pub const WORD_ALPHA: f32 = 0.35; pub const WORD_RADIUS: f32 = 2.0; pub const SIDE_HAIRLINE: f32 = 1.0;
      pub const SIDE_EMPTY_ALPHA: f32 = 0.03;
  }
  // src/file_tab.rs
  pub fn row_height(line_height: Pixels) -> Pixels;        // line_height × code::ROW_SCALE
  fn number_ink(ui: &UiColors, bg: Rgb, current: bool) -> Rgb; // current: ui.text; else ui.text_dim held ≥ MARK_MIN
  fn carry_line(new_of_old: Option<&[Option<usize>]>, line: usize, same: impl Fn(usize) -> bool) -> Option<usize>;
  // FileTab gains `current: Option<usize>` (File mode: a line) and `diff_current: Option<usize>` (Diff mode: a
  // DiffDoc row), both None when a tab opens.
  ```
  `Geom::line_height` now holds the **row** height (every row pitch already reads it); text is painted with that
  height, so GPUI centres it in the row.

- [ ] **Step 1: Write the failing tests** (`file_tab::tests`):
  - `rows_are_the_line_height_times_1_2`: `row_height(px(15.0)) == px(18.0)`.
  - `columns_use_the_code_sizes`: `columns(8.0, Gutter::Plain { digits: 2, marks: false })` →
    `new_right == 16 + 16`, `bar_left == 32 + 10`, `text_left == 42 + 3 + 14`; with marks the column is 16 more.
  - `quiet_numbers_stay_readable`: for every built-in theme (`every_theme`), `number_ink(.., false)` has
    `contrast ≥ MARK_MIN` on the background and is not `ui.text`; `number_ink(.., true) == ui.text`.
  - `the_current_line_follows_its_line_or_clears`: with `new_of_old = [Some(2), None, Some(3)]`, line 0 → `Some(2)`,
    line 1 → `None`; with no comparison, kept when `same(line)`, else `None`.
- [ ] **Step 2: Run** `cargo test --bin fuxx file_tab` — FAIL.
- [ ] **Step 3: Implement.** Sizes from `code`; the body's top padding `code::TOP_ROOM`. The band: a full-width
  quad `ui.text` at `CURRENT_ALPHA` behind the current line's rows, painted **first** (under change tints, the glow,
  the selection and matches). Numbers via `number_ink`. `current` is set in `mouse_up` from the selection's moving end
  (a press without a drag: the press), in `find_step` to the match's line; Esc does not clear it; a reload carries it
  with `carry_line` (the compare's `new_of_old`; no compare: kept if that line's text is unchanged). In Diff mode
  the same events set `diff_current`, cleared when the diff's rows change.
- [ ] **Step 4: Write the failing scene** `file-code-calm` (both themes, Visual; sources `src/file_tab.rs`,
  `src/ui_tokens.rs`): a 14-line Rust fixture, line 5 pressed → `Geom::line_height == row_height(metrics)`; the text's
  left at the new columns; at the right edge of row 5 the pixel is `mix(bg, ui.text, 0.05)` and rows 4 and 6 are `bg`;
  line 5's number drawn in `ui.text`, line 4's in the quiet ink; the first row's top `code::TOP_ROOM` below the body's.
  Update `plain_text_left` to the new sizes. Add the `ORDER` entry.
- [ ] **Step 5: Run** `scripts/ui-shots.sh file-code-calm` — FAIL, implement until PASS; then the gate and the full
  `scripts/ui-shots.sh` (all file scenes still pass at the new spacing).
- [ ] **Step 6: Commit** `M11ah Task 1: code sizes, roomier rows, quieter numbers, the current line`.

### Task 2: The outline (`src/code_outline.rs`) and named lines from the colouring pass

**Files:**
- Create: `src/code_outline.rs` (declared in `src/main.rs`)
- Modify: `src/highlight.rs` (`Coloured`, `Highlighter::colour`; `highlight` becomes `colour(..).spans`)
- Modify: `src/file_tab.rs` — `Doc` gains `outline`; `read_file` makes it; `colour_text` and `Doc::accept_spans` take
  the named lines; `carry_over` carries both

**Interfaces:**
- Produces:
  ```rust
  // src/code_outline.rs (pure)
  pub const LIMIT: Duration = Duration::from_millis(50);
  pub const PART_MAX: usize = 32;
  #[derive(Clone, Debug, PartialEq, Eq)]
  pub struct Block { pub start: usize, pub end: usize /* exclusive, closer included */, pub indent: usize /* cells */,
                     pub depth: usize, pub named: bool }
  #[derive(Clone, Debug, Default, PartialEq)]
  pub struct Outline { pub step: usize, pub blocks: Vec<Block> /* by start; outer before inner */, /* per-line levels */ }
  /// None: gave up after `limit`. Markdown: heading blocks; else indentation blocks.
  pub fn outline(lines: &[&str], markdown: bool, limit: Duration) -> Option<Outline>;
  pub fn indent_cells(line: &str) -> Option<usize>; // None: blank; tabs 4 cells
  pub fn part_words(line: &str, markdown: bool) -> String;
  impl Outline {
      pub fn has_structure(&self) -> bool;
      pub fn name(&mut self, named_lines: &[usize]) -> bool;  // marks blocks starting on them; true if changed
      pub fn levels(&self, line: usize) -> usize;              // guides on `line`
      pub fn containing(&self, line: usize) -> Vec<usize>;     // start <= line < end, outer → inner
      pub fn starting_at(&self, line: usize) -> Option<usize>;
      pub fn titled(&self, line: usize) -> Option<usize>;      // innermost named containing; else innermost
      pub fn path(&self, line: usize) -> Vec<usize>;           // named containing (outer → inner); none named: all
      pub fn sticky(&self, top: usize) -> Option<usize>;       // `titled`'s rule, but only start < top
  }
  // src/highlight.rs
  pub struct Coloured { pub spans: Vec<Vec<Span>>, pub named: Vec<usize> }
  impl Highlighter { pub fn colour(&self, path: &Path, text: &str) -> Coloured; }
  // src/file_tab.rs
  // Doc gains `pub outline: Option<Arc<Outline>>` (None: gave up; never for a refused file).
  pub fn accept_spans(&mut self, version: u64, spans: Vec<Vec<Span>>, named: &[usize]) -> bool;
  ```
  The named scopes: `entity.name.function, entity.name.class, entity.name.type, entity.name.struct, entity.name.enum,
  entity.name.trait, entity.name.impl, entity.name.module, entity.name.namespace, entity.name.section, markup.heading`
  (one `ScopeSelectors`), matched on the stack after each `Push` of `parse_line`'s ops.

- [ ] **Step 1: Write the failing tests** (`code_outline::tests`; fixtures made up):
  - `indentation_opens_a_block_when_the_next_line_is_deeper`: `["fn a() {", "    x();", "}", "fn b() {}"]` → one
    block `0..3` (the `}` taken in), `b` none.
  - `closers_join_their_block`: `}`, `)`, `]`, `end`, `</div>` at the opener's indent are inside; a deeper `}` is not
    a closer of the outer block.
  - `blank_lines_inside_a_block_belong_to_it` (and take its guide levels); `a_trailing_blank_line_does_not`.
  - `markdown_headings_run_to_the_next_same_or_higher`: `# A`, `## B`, `text`, `## C`, `# D` → A `0..4`, B `1..3`,
    C `3..4`, D `4..5`.
  - `fenced_code_hashes_are_not_headings` (Review Focus 1): a ```` ``` ```` block holding `# install` adds no block.
  - `tabs_count_four_and_odd_indents_round_down` (Review Focus 3): a 2-space file with one 3-space line → `step == 2`,
    that line `levels == 1`; a tab-indented line counts 4 cells.
  - `the_step_is_the_most_common_one_default_four`; `no_structure_in_flat_text`; `empty_and_blank_files_have_no_blocks`.
  - `gave_up_over_the_limit`: `outline(.., Duration::ZERO)` on 50,000 nested lines → `None`.
  - `names_prefer_the_innermost_named_block`: `impl S {` (named) › `fn r() {` (named) › `if x {` → `titled(in if)` is
    `fn r`; with no names, the `if`; `path` gives `[impl, fn]`; `sticky(top)` is none while `top == fn's start`.
  - `part_words_trim_and_cut`: `"    pub fn refresh(&mut self) -> bool {"` → `"pub fn refresh(&mut self) -> bool"`;
    trailing `:` and `=>` dropped; 40 chars → 32 + `…`; `"## Install"` (markdown) → `"Install"`.
  - `highlight::tests::named_lines_come_with_the_colours`: a Rust snippet with `impl S {` on line 0 and `fn r()` on
    line 1 → `named == [0, 1]`; `colour(..).spans` equals the old `highlight` output for the existing fixtures.
- [ ] **Step 2: Run** `cargo test --bin fuxx code_outline highlight` — FAIL.
- [ ] **Step 3: Implement.** `read_file` runs `outline(&line_slices(..), is_markdown(path), LIMIT)` for every text
  (plain-too-large too) after the compare; `colour_text` uses `colour` and `accept_spans` names the doc's outline
  (`Arc::make_mut`); `colour_rows` returns each side's named lines too (Task 9 uses them).
- [ ] **Step 4: Run** — PASS; the gate.
- [ ] **Step 5: Commit** `M11ah Task 2: the outline, and named lines from the colouring pass`.

### Task 3: Indent guides and rounded change bars

**Files:**
- Modify: `src/file_tab.rs` — `paint_body` (guides under the text; bars as runs); `Frame` gains the outline and
  `current`; Diff gets an outline of its rows' text for guides (`DiffDoc::outline`, made in `diff_state`)
- Modify: `src/ui_shots/file_tab.rs` (scenes)

**Interfaces:**
- Consumes: Task 2's `Outline::levels`, `containing`; Task 1's `current`, `code::*`.
- Produces:
  ```rust
  /// Runs of neighbouring shown rows with the same change, in row order: (rows, change).
  pub fn bar_runs(changes: &[(usize /* row */, LineChange)]) -> Vec<(Range<usize>, LineChange)>;
  /// The pixel span of a run drawn at `top..bottom`: shortened by half of `code::BAR_RUN_GAP` at an end that
  /// touches a run of another kind.
  pub fn bar_span(top: f32, bottom: f32, gap_above: bool, gap_below: bool) -> (f32, f32);
  ```

- [ ] **Step 1: Write the failing tests:** `bar_runs_join_rows_of_one_kind` (rows 2,3,4 Added + 5 Changed → two runs
  `2..5`, `5..6`); `bar_span_leaves_a_gap_only_between_kinds` (a 2 pt gap total between touching runs, none at free
  ends).
- [ ] **Step 2: Write the failing scenes** (both themes, Visual): `file-code-guides` — a nested Rust fixture: a 1 pt
  guide at cells `0` and `step` on a level-2 line in `ui.text` at 7 %, the current line's block guide at 14 %, guides
  through a blank line inside a block, on a wrapped line's continuation rows (Wrap on), none for a flat text file, and
  in Diff mode; `file-code-bars` — three Added rows are one rounded bar (its top and bottom corner pixels are `bg`, its
  middle the green), an Added run touching a Changed run shows a 2 pt `bg` gap, the deleted-lines ▸ still drawn; a
  press on any of a run's rows still opens Diff (state check).
- [ ] **Step 3: Run** them — FAIL. **Step 4: Implement.** Guides use `Outline::levels(line)` × `step` cells from the
  text's left (scrolled sideways with Wrap off, inside the text mask); the active guide is the innermost block that
  holds `current`. Bars: `bar_runs` over the rows in sight (a run cut by the view's edge keeps square ends there),
  quads with `corner_radii(BAR_WIDTH / 2)`.
- [ ] **Step 5: Run** — PASS; the gate; full `scripts/ui-shots.sh`.
- [ ] **Step 6: Commit** `M11ah Task 3: indent guides and rounded change bars`.

### Task 4: The row map (`src/row_map.rs`) replaces `Wrapped`

**Files:**
- Create: `src/row_map.rs` (declared in `src/main.rs`); `visual_rows` and `first_rows` move here with their tests
- Modify: `src/file_tab.rs` — `Wrapped` and `visible_rows` go; `wrapped: Rc<RefCell<Option<Wrapped>>>` becomes
  `rows: Rc<RefCell<Option<(MapKey, RowMap)>>>`; `pos_at`, `bar_under`, `reveal_current`, `reveal_diff`,
  `follow_latest`, `rescroll_row`, `bar_at_rows`, `paint_body` and `Geom::of` read it

**Interfaces:**
- Produces:
  ```rust
  #[derive(Clone, Copy, Debug, PartialEq, Eq)]
  pub enum Row {
      Line { line: usize, cell0: usize },               // a part of a doc line (cell0 0: its first row)
      Title { card: usize },                            // Diff: a card's title row (Task 9)
      Unchanged { stretch: usize },                     // Diff: a folded unchanged stretch (Task 9)
      Pair { old: Option<(usize, usize)>, new: Option<(usize, usize)> }, // Diff side by side (Task 11)
  }
  #[derive(Clone)]
  pub struct RowMap { /* rows, each row's top (points), each line's first row, columns, total height */ }
  impl RowMap {
      /// A file's rows: each shown line (`hidden` ranges left out) cut at `columns` cells when wrapping.
      pub fn file(widths: &[usize], columns: Option<usize>, hidden: &[Range<usize>], row_h: f32) -> RowMap;
      pub fn len(&self) -> usize;
      pub fn row(&self, r: usize) -> Option<Row>;
      pub fn top(&self, r: usize) -> f32;          // past the end: the height
      pub fn height(&self) -> f32;
      pub fn columns(&self) -> Option<usize>;
      pub fn row_at(&self, y: f32) -> usize;       // the row drawn at y (clamped to the last)
      pub fn rows_in(&self, y: Range<f32>) -> Range<usize>;
      pub fn first_row(&self, line: usize) -> Option<usize>; // None: hidden
  }
  /// What a map was built for (rebuilt when any differs): mode, doc version, columns, folds' generation,
  /// the Diff layout's generation, side by side.
  #[derive(Clone, Copy, Debug, PartialEq, Eq)]
  pub struct MapKey { pub mode: FileMode, pub version: u64, pub columns: Option<usize>, pub folds: u64,
                      pub layout: u64, pub side: bool }
  pub fn rescroll_row(top: usize, from: Option<&RowMap>, to: &RowMap) -> usize; // file_tab.rs, same line on top
  ```

- [ ] **Step 1: Write the failing tests** (`row_map::tests`): `with_nothing_hidden_and_no_wrap_it_is_the_identity`
  (row r is `Line { line: r, cell0: 0 }`, `top(r) == r × row_h`); `wrapped_lines_take_their_rows` (the moved
  `visual_rows` cases); `hidden_lines_have_no_row` (`hidden = [3..7]` → `first_row(5) == None`,
  `first_row(7) == Some(4)`); `row_at_finds_the_row_drawn_at_y_with_hidden_lines_and_titles` (Review Focus 5; here with
  hidden lines and Wrap; titles join in Task 9); `rows_in_covers_partly_shown_rows`.
- [ ] **Step 2: Run** `cargo test --bin fuxx row_map` — FAIL.
- [ ] **Step 3: Implement**, then switch `file_tab.rs` over. The map is built in the canvas prepaint only when its
  `MapKey` changes (a layout cache, never a change of what is shown). Existing `file_tab::tests` that used `Wrapped`
  (`switching_wrap_keeps_the_top_line`, `bars_are_hit_by_their_rows_when_wrapped`, …) keep their assertions on
  `RowMap`.
- [ ] **Step 4: Run** — PASS; the gate; the full `scripts/ui-shots.sh`: every scene passes with no visible change
  (`file-tab`, `file-diff`, `file-tab-scrollbar`, `file-live-*`, `file-follow-*`, `file-code-*`).
- [ ] **Step 5: Commit** `M11ah Task 4: one row map for every screen row`.

### Task 5: Folding in File mode

**Files:**
- Modify: `src/code_outline.rs` (`hidden_ranges`)
- Modify: `src/file_tab.rs` — `FileTab` gains `folded: BTreeSet<usize>` (block starts) and `folds_gen: u64`; the
  fold column in `columns`/`Geom`; ▸ / ▾ and the pill in `paint_body`; presses in `mouse_down`; actions; reload carry;
  `reveal_current`, `follow_latest` open folds; `pos_at` through the map
- Modify: `src/main.rs` (bindings + keymap test), `src/palette_items.rs`, `src/help_shortcuts.rs`
- Modify: `src/ui_shots/file_tab.rs`

**Interfaces:**
- Consumes: Task 2's `Outline::starting_at`, `containing` (its last: the innermost); Task 4's
  `RowMap::file(.., hidden, ..)`; `thousands` (made `pub(crate)`).
- Produces:
  ```rust
  // src/code_outline.rs
  pub fn hidden_ranges(outline: &Outline, folded: &BTreeSet<usize>) -> Vec<Range<usize>>; // merged, outer wins
  // src/file_tab.rs
  gpui::actions!(file_tab, [Find, FindNext, FindPrev, CloseFind, FoldHere, UnfoldHere, FoldAll, UnfoldAll]);
  // Gutter::Plain gains `folds: bool`; Columns and Geom gain `fold_left: Option<f32 / Pixels>`
  // (numbers · fold · marks · bar · text).
  fn fold_pill_words(hidden: usize) -> String;                      // "⋯ 24 lines"
  /// What a folded line shows for its hidden lines: the strongest change (all Added: Added; all deletions:
  /// DeletedAfter; else Changed), the warmest glow level, the newest mark.
  fn folded_summary(hidden: Range<usize>, changes: &Changes, live: Option<&LiveFrame>) -> FoldSummary;
  struct FoldSummary { change: Option<LineChange>, glow: Option<u8>, mark: Option<Mark> }
  fn carry_folds(folded: &BTreeSet<usize>, new_of_old: Option<&[Option<usize>]>, same: impl Fn(usize) -> bool,
                 outline: &Outline) -> BTreeSet<usize>;                // a start that changed or is gone opens
  ```
  Keys in the `FileTab` context: `alt-cmd-[` → `FoldHere`, `alt-cmd-]` → `UnfoldHere`. Palette (`Place::File`):
  `Fold All`, `Unfold All`.

- [ ] **Step 1: Write the failing tests:** `hidden_ranges_merge_nested_folds`; `fold_pill_says_its_hidden_lines`
  (`24` → `⋯ 24 lines`, `1204` → `⋯ 1,204 lines`); `a_fold_never_hides_a_change`
  (`folded_summary` over hidden Added + Changed → `Changed`; only deletions → `DeletedAfter`; glow levels 1 and 3 → 3;
  the newest mark); `reload_drops_folds_and_current_line_that_are_gone` (Review Focus 2: an opener whose
  `new_of_old` is `None`, or past the new end, opens; one moved down by 2 follows); `numbers_skip_hidden_lines`
  (a fold at 11 hiding 12..36 → the row after it is line 36, drawn `37`); keymap
  `the_fold_keys_work_in_the_file_tab` (`alt-cmd-[` / `alt-cmd-]` run `FoldHere` / `UnfoldHere` in `FileTab`, not in a
  terminal); palette `fold_all_is_a_file_command`.
- [ ] **Step 2: Run** them — FAIL.
- [ ] **Step 3: Write the failing scenes:** `file-code-fold` (both themes, Visual): the fold column 14 pt between
  numbers and marks; ▸ on a folded line always, ▾ faint only while the pointer is over the gutter; the pill after the
  folded line's text; the line numbers 12 then 37; a fold holding a change draws the bar on the folded line; a fold
  holding warm lines takes the warmest glow and its mark (M11af's glow and marks on a folded file).
  `file-code-fold-keys` (One, State): ⌥⌘[ on the current line folds its innermost block, ⌥⌘] unfolds; clicking ▸ or
  the pill unfolds; `Fold All` / `Unfold All`; a press under a fold selects the line after it (Review Focus 5); Copy
  over a fold copies the hidden lines; a find step to a hidden match opens its fold; Follow (on) after an agent change
  inside a fold opens it; folding turns Follow off; a tab with no structure has no fold column.
- [ ] **Step 4: Implement.** The fold column shows in File mode when `outline.has_structure()`; ▾ draws only while
  `gutter_hover` (set in `mouse_move`, notify on change). A fold changes `folded` and `folds_gen` (the map rebuilds),
  turns Follow off, keeps the top line in place. `FoldHere` folds the innermost block holding `current` (none: the top
  line); not in Diff or Preview.
- [ ] **Step 5: Run** — PASS; the gate; full `scripts/ui-shots.sh`.
- [ ] **Step 6: Commit** `M11ah Task 5: fold blocks in File mode; a fold never hides a change`.

### Task 6: The sticky line and the breadcrumb

**Files:**
- Modify: `src/file_tab.rs` — `FileTab` gains `place: Place`, refreshed by `refresh_place()` from `scrolled`, `loaded`,
  the colours landing, a fold or mode change (never while drawing); the sticky line painted at the canvas top; the
  breadcrumb strip (its own canvas) between the find bar and the body; clicks
- Modify: `src/ui_shots/file_tab.rs`

**Interfaces:**
- Consumes: Task 2's `Outline::sticky`, `path`, `part_words`; Task 4's map (`row_at(scroll.y)` → the top line).
- Produces:
  ```rust
  #[derive(Clone, Debug, Default, PartialEq)]
  struct Place { sticky: Option<Sticky>, crumbs: Vec<Crumb> }
  #[derive(Clone, Copy, Debug, PartialEq, Eq)] enum Sticky { Line(usize), Title(usize /* card, Task 9 */) }
  #[derive(Clone, Debug, PartialEq, Eq)] pub struct Crumb { pub words: String, pub line: Option<usize> /* None: the file */ }
  pub fn crumbs(file_name: &str, outline: &Outline, lines: &[&str], top: usize, markdown: bool) -> Vec<Crumb>;
  /// Which crumbs fit `room` points (widths measured): outer ones fold into one "…" first; the last always stays.
  pub fn fit_crumbs(widths: &[f32], room: f32, ellipsis: f32, separator: f32) -> Vec<Option<usize>>; // None: "…"
  /// The first row to show so `target` sits `REVEAL_ABOVE` rows down, one more when a sticky line shows there.
  fn reveal_top(target_row: usize, sticky_at: impl Fn(usize) -> bool) -> usize;
  ```

- [ ] **Step 1: Write the failing tests:** `crumbs_go_outer_to_inner_from_the_file` (`session.rs › impl Session › fn
  refresh` for a top line inside `fn refresh`); `crumbs_follow_the_top_not_the_current_line`;
  `fit_crumbs_folds_outer_parts_first` (narrow: `[None, Some(2)]`; very narrow: `[Some(2)]` still);
  `reveal_top_leaves_room_for_the_sticky_line` (target 40, sticky there → 36; none → 37);
  `the_sticky_line_is_none_at_its_own_first_line`.
- [ ] **Step 2: Write the failing scenes** (both themes, Visual unless noted): `file-code-sticky` — scrolled into
  `fn refresh`, its first line pinned at the text's top with its number, on `bg`, a 1 pt `divider` under it; with Wrap
  on, one row cut with `…`; clicking it scrolls back (that line on top and current; State check); Follow and a find
  step land below it. `file-code-breadcrumb` — the strip 26 pt tall under the bar (and under the find bar when open),
  12 pt text, parts in `text_dim`, the last in `text`; narrow → `… › fn refresh`; a part's click puts its line on top
  and current and turns Follow off; the file name goes to the top; no strip in Diff, in Preview, or for a flat text
  file; the text's top does not move when the colours (names) land.
- [ ] **Step 3: Run** — FAIL. **Step 4: Implement.** File mode only (Diff's sticky title comes in Task 9). The strip
  is shown whenever the shown doc's outline `has_structure()`, so it is there from the first frame the text is.
  `reveal_current`, `follow_latest` and (Task 8) the jumps use `reveal_top`.
- [ ] **Step 5: Run** — PASS; the gate; full `scripts/ui-shots.sh`.
- [ ] **Step 6: Commit** `M11ah Task 6: the sticky line and the breadcrumb`.

### Task 7: Scrollbar marks

**Files:**
- Modify: `src/scrollbar.rs` (pure `lane`, `paint_lane`)
- Modify: `src/file_tab.rs` — the lane kept in `lane: Rc<RefCell<Option<(LaneKey, Vec<LaneMark>)>>>`, rebuilt in the
  prepaint only when its key (map key, changes' version, find generation and current) changes; painted under the thumb
  every frame, whether the thumb shows or not
- Modify: `src/ui_shots/file_tab.rs`

**Interfaces:**
- Produces:
  ```rust
  // src/scrollbar.rs
  #[derive(Clone, Copy, Debug, PartialEq)] pub struct LaneMark { pub top: f32, pub len: f32, pub color: Rgb }
  /// Items (content top, content height, colour) scaled onto a `track`-long lane for `content`, each at least
  /// `min` long, touching or overlapping marks of one colour merged.
  pub fn lane(items: &[(f32, f32, Rgb)], content: f32, track: f32, min: f32) -> Vec<LaneMark>;
  pub fn paint_lane(window: &mut Window, area: Bounds<Pixels>, marks: &[LaneMark]); // code::LANE wide, right edge
  ```
  File mode: each `bar_runs` run (its rows' top and height from the map; a fold's summary change on its line) in the
  bar colours; each find match a `LANE_TICK` tick in the accent at `MATCH_TINT`, the current at `CURRENT_TINT`, drawn
  after. Diff mode's per-card marks come in Task 9. Preview keeps its plain thumb.

- [ ] **Step 1: Write the failing tests** (`scrollbar::tests`): `lane_marks_scale_to_the_track`;
  `a_lane_mark_is_at_least_two_points`; `lane_marks_on_one_point_merge` (two runs 1 pt apart on a long file → one);
  `different_colours_do_not_merge`.
- [ ] **Step 2: Write the failing scene** `file-code-lane` (both themes, Visual): a 400-line file with changes near
  the top, middle and end and three find matches: the lane's pixels at those heights carry the change colours and the
  accent; the current match brighter; marks still drawn when the thumb has faded (scene time past the fade); with a
  fold above a change the mark moves up; a press on the lane jumps as a track press does (State check).
- [ ] **Step 3: Run** — FAIL. **Step 4: Implement.** **Step 5: Run** — PASS; the gate; full `scripts/ui-shots.sh`.
- [ ] **Step 6: Commit** `M11ah Task 7: scrollbar marks for changes and find matches`.

### Task 8: ⌥↑ / ⌥↓ between changes

**Files:**
- Modify: `src/row_map.rs` (`change_starts`, `step_change`)
- Modify: `src/file_tab.rs` — actions `PrevChange`, `NextChange`; two icon buttons left of the view switch;
  `jump_change(forward)`
- Modify: `src/tab_bar.rs` (`Glyph::ChangeUp`, `Glyph::ChangeDown`: an arrow up / down, drawn like `Pull` / `Push`)
- Modify: `src/main.rs` (bindings + keymap test), `src/help_shortcuts.rs`, `src/palette_items.rs` (`Previous Change`,
  `Next Change`, `Place::File`)
- Modify: `src/ui_shots/file_tab.rs`

**Interfaces:**
- Consumes: Task 6's `reveal_top`; Task 5's folds.
- Produces:
  ```rust
  // src/row_map.rs
  /// Each run of changed lines' first line (deleted-lines edges count as a change at the line after them,
  /// held to the last line), in order.
  pub fn change_starts(by_line: &HashMap<usize, LineChange>, deleted: &[usize], lines: usize) -> Vec<usize>;
  /// From the current line (strictly after / before it), or with none from the view's `top` (forward: at or after
  /// it; back: before it). None at the ends: no wrap-around.
  pub fn step_change(starts: &[usize], from: Option<usize>, top: usize, forward: bool) -> Option<usize>;
  // src/file_tab.rs
  gpui::actions!(file_tab, [.., PrevChange, NextChange]); // `alt-up` / `alt-down` in the FileTab context
  ```

- [ ] **Step 1: Write the failing tests:** `change_starts_are_runs` (lines 3–5 and 9 changed, a deletion after 12 →
  `[3, 9, 12]`); `step_change_moves_from_the_current_line`; `step_change_from_the_top_without_one`;
  `step_change_stops_at_the_ends`; keymap `the_change_keys_work_in_the_file_tab_only` (`alt-up` / `alt-down` run them in
  `FileTab`; not bound in a terminal: ⌥↑ stays the terminal's; in the find box the handler does nothing — it acts
  only while the tab's own focus handle is focused).
- [ ] **Step 2: Write the failing scene** `file-code-nav` (One, State): the ↑ / ↓ buttons (24 pt) show only when the
  file has a change, tooltips `Previous change (⌥↑)` / `Next change (⌥↓)`; ⌥↓ puts the next change 3 rows from the
  top (4 under a sticky line) and makes its first line current; a change in a fold opens it; at the last change ↓ is
  faint and ⌥↓ does nothing; ⌥↑ back; a jump turns Follow off; plain ↑ / ↓ change nothing (as today).
- [ ] **Step 3: Run** — FAIL. **Step 4: Implement.** **Step 5: Run** — PASS; the gate; full `scripts/ui-shots.sh`.
- [ ] **Step 6: Commit** `M11ah Task 8: jump between changes with ⌥↑ / ⌥↓`.

### Task 9: Diff cards and folded unchanged stretches (`src/diff_cards.rs`)

**Files:**
- Create: `src/diff_cards.rs` (declared in `src/main.rs`)
- Modify: `src/row_map.rs` (`RowMap::diff`, `DiffLayout`)
- Modify: `src/file_tab.rs` — `DiffDoc` gains `grouped`, `titles`, both sides' outlines (made in `diff_state`, names
  from `colour_rows`); `FileTab` gains `shown_stretches: BTreeSet<usize>`, `closed_cards: BTreeSet<usize>` (keyed by
  new-file line), cleared on entering Diff; panels, title rows, `⋯` rows painted; presses; the sticky title;
  `reveal_diff` and a bar press land on the card; ⌥↑ / ⌥↓ by card; the lane per card; action
  `ShowAllUnchanged` (palette `Show All Unchanged Lines`, `Place::File`)
- Modify: `src/ui_shots/file_tab.rs` — the existing `file-diff` checks read row positions from the map
  (`FileTab::line_top` under `ui-shots`) instead of `line_height × row`

**Interfaces:**
- Consumes: Task 2 (`Outline`, `part_words`), Task 4 (`Row::Title`, `Row::Unchanged`), Task 6 (`Sticky::Title`),
  Task 7 (`lane`), Task 8 (`step_change`).
- Produces:
  ```rust
  pub const CONTEXT: usize = 3;
  pub const FOLD_MIN: usize = 8;
  #[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum CardKind { Added, Removed, Both }
  #[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum Whole { New, Deleted }
  #[derive(Clone, Debug, PartialEq, Eq)]
  pub struct Card { pub rows: Range<usize> /* with context */, pub changed: Range<usize>, pub added: usize,
                    pub removed: usize, pub kind: CardKind }
  #[derive(Clone, Debug, PartialEq, Eq)]
  pub struct Stretch { pub rows: Range<usize>, pub new_start: Option<usize> /* 1-based */ }
  #[derive(Clone, Debug, Default, PartialEq, Eq)]
  pub struct Grouped { pub cards: Vec<Card>, pub stretches: Vec<Stretch>, pub whole: Option<Whole> }
  pub fn group(rows: &[DiffRow]) -> Grouped;
  #[derive(Clone, Debug, PartialEq, Eq)]
  pub struct CardTitle { pub name: String, pub added: usize, pub removed: usize, pub lines: Option<(usize, usize)> }
  /// The innermost named block of the card's first change (new side; an only-removing card: old side),
  /// "Top level" outside any; a whole new or deleted file: "New file" / "Deleted file".
  pub fn title(card: &Card, rows: &[DiffRow], whole: Option<Whole>, new: Option<(&Outline, &[&str])>,
               old: Option<(&Outline, &[&str])>, markdown: bool) -> CardTitle;
  pub fn unchanged_words(n: usize) -> String; // "⋯ 36 unchanged lines — click to show"
  // src/row_map.rs
  pub struct DiffLayout<'a> { pub grouped: &'a Grouped, pub shown: &'a [bool] /* per stretch */,
                              pub closed: &'a [bool] /* per card */,
                              pub pairs: Option<&'a [Vec<(Option<usize>, Option<usize>)>]> /* Task 11 */ }
  impl RowMap { pub fn diff(widths: &[usize], columns: Option<usize>, layout: &DiffLayout, row_h: f32) -> RowMap; }
  // a card's title row is code::CARD_TITLE tall, plus code::CARD_GAP above every card but the first
  ```

- [ ] **Step 1: Write the failing tests** (`diff_cards::tests`, `row_map::tests`):
  - `a_change_gets_three_lines_of_context`; `runs_whose_context_touches_share_a_card` (gap 6 → one card, gap 7 → two
    with one plain row between); `stretches_fold_from_eight_lines` (7 → shown rows, 8 → one stretch, before the first
    and after the last card too); `a_new_or_deleted_file_is_one_card_with_nothing_folded`;
  - `titles_name_the_innermost_named_block` (`fn refresh`, `+12 −3`, `lines 40–58`); `a_removal_only_titles_from_the_old_side`;
    `outside_any_block_reads_top_level`; `whole_file_titles` (`New file · +120`, `Deleted file · −80`);
  - `diff_rows_have_titles_and_folded_stretches` (`RowMap::diff`: title rows' heights, a stretch is one row, a shown
    stretch is its lines, a closed card is its title only); `row_at_finds_rows_below_titles_and_stretches`
    (Review Focus 5 with titles).
- [ ] **Step 2: Write the failing scenes:** `file-diff-cards` (both themes, Visual): rounded panels inset 12 pt with a
  1 pt `divider` border and 10 pt between; titles with name, `+n` green, `−n` red, `lines a–b` right in `text_dim`;
  the `⋯ 36 unchanged lines — click to show` row centred, 12 pt, `text_dim`; guides inside cards; a new file's single
  card. `file-diff-cards-fold` (One, State): clicking a stretch shows it; clicking a title folds the card to its title
  and back; `Show All Unchanged Lines` opens all; leaving Diff and coming back folds them again; a reload keeps a shown
  stretch that starts on the same new line; a card taller than the view keeps its title sticky until it scrolls away;
  ⌥↓ moves card to card (the title at the top, the first changed row current); a change bar's press in File mode opens
  Diff at its card; a find step into a stretch opens it; the lane has one mark per card (green, red, blue for both).
- [ ] **Step 3: Run** — FAIL. **Step 4: Implement.** Diff's wrap columns are measured inside the card insets.
- [ ] **Step 5: Run** — PASS (existing `file-diff` and `file-live-modes` too); the gate; full `scripts/ui-shots.sh`.
- [ ] **Step 6: Commit** `M11ah Task 9: Diff cards with titles, unchanged stretches folded`.

### Task 10: Changed words

**Files:**
- Modify: `Cargo.toml` (`similar = { version = "3.2.1", default-features = false, features = ["std", "inline"] }`)
- Modify: `src/diff_cards.rs` (`pair`, `word_marks`)
- Modify: `src/file_tab.rs` — `DiffDoc` gains `words: Option<Arc<HashMap<usize, Vec<Range<usize>>>>>`, made in
  `diff_state`; `paint_body` draws them over the row tint, under the selection and matches

**Interfaces:**
- Consumes: Task 9's `Card`; `file_tab::LONG_LINE` (made `pub(crate)`).
- Produces:
  ```rust
  pub const WORD_LIMIT: Duration = Duration::from_millis(50);
  /// A card's rows side by side: a Same row with itself; each run of Removed rows then Added rows zipped in
  /// order, the shorter side padded with None.
  pub fn pair(rows: &[DiffRow], range: Range<usize>) -> Vec<(Option<usize>, Option<usize>)>;
  /// The changed words of each paired Removed / Added row (doc row → byte ranges in its line), by words, each
  /// pair diffed alone with `min_ratio(0.5)`; a pair with a line over `LONG_LINE` cells skipped. None: gave up.
  pub fn word_marks(rows: &[DiffRow], lines: &[&str], widths: &[usize], cards: &[Card], limit: Duration)
      -> Option<HashMap<usize, Vec<Range<usize>>>>;
  ```

- [ ] **Step 1: Check with Context7** (`resolve-library-id` "similar" → `query-docs` on inline changes): the feature
  name, `iter_inline_changes_with_options_deadline`, `InlineChangeOptions` builders; note any difference from the Tech
  Stack line in this plan before going on.
- [ ] **Step 2: Write the failing tests:** `pairs_uneven_runs_pad_the_shorter_side` (3 removed, 1 added → `(r0, a0),
  (r1, None), (r2, None)`); `only_the_changed_word_is_marked` (`let token = old();` → `let token = fresh();`: one range
  each, on `old` / `fresh`); `lines_sharing_under_half_their_words_get_no_marks`;
  `word_marks_sit_on_char_boundaries` (Review Focus 4: `café`→`cafés`, a tab-indented line);
  `long_lines_are_not_paired`; `gave_up_gives_no_marks` (`Duration::ZERO` → `None`).
- [ ] **Step 3: Run** `cargo test --bin fuxx diff_cards` — FAIL. **Step 4: Implement.**
- [ ] **Step 5: Write the failing scene** `file-diff-words` (both themes, Visual): the changed word's box is red /
  green at about 35 % over the row's tint with 2 pt corners, the rest of the row only the light tint; Wrap on keeps
  marks on the right row. **Run** — FAIL, implement drawing, PASS.
- [ ] **Step 6: Run** the gate; full `scripts/ui-shots.sh`.
- [ ] **Step 7: Commit** (by path: `Cargo.toml`, `Cargo.lock`, the source files above) `M11ah Task 10: mark the
  changed words in a card`.

### Task 11: Side by side

**Files:**
- Modify: `src/layout_model.rs` — `Model.side_by_sides: Vec<TabId>`, `side_by_side`, `set_side_by_side`;
  `SavedTab::File` gains `side_by_side: bool` (`#[serde(default, skip_serializing_if = "std::ops::Not::not")]`) as
  `preview` is; save and restore
- Modify: `src/file_tab.rs` — `FileTab` gains `side_by_side: bool`, `fits_side: Rc<Cell<bool>>`, `current_half` and
  the selection's half; `FileTabEvent::SideBySide(bool)`; the button; `RowMap::diff` with pairs; two gutters, a
  hairline; Copy of one half; `make_file_tab` sets it from the model
- Modify: `src/row_map.rs` (`Row::Pair` rows, each half wrapping at its own columns, the shorter padded)
- Modify: `src/tab_bar.rs` (`Glyph::SideBySide`: a box split in two columns)
- Modify: `src/ui_shots/file_tab.rs`

**Interfaces:**
- Consumes: Task 9 (`DiffLayout.pairs`), Task 10 (`pair`, words on both halves).
- Produces:
  ```rust
  pub const SIDE_MIN_CELLS: usize = 60;
  #[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum Half { Old, New }
  /// Whether a body `width` points wide fits two halves of `SIDE_MIN_CELLS` cells with their gutters.
  pub fn fits_side_by_side(width: f32, cell: f32, gutter: Gutter) -> bool;
  /// The selected text of one half: only rows on that side (Old: Same + Removed; New: Same + Added).
  pub fn half_text(doc: &Doc, rows: &[DiffRow], a: Pos, b: Pos, half: Half) -> String;
  // layout_model.rs
  pub fn side_by_side(&self, tab: TabId) -> bool;
  pub fn set_side_by_side(&mut self, tab: TabId, on: bool) -> bool;
  ```

- [ ] **Step 1: Write the failing tests:** `layout_model` `side_by_side_is_saved_per_file_tab_and_off_by_default`
  (old saved files load off; on round-trips; not written when off); `fits_needs_sixty_cells_a_half`;
  `half_text_copies_one_side`; `row_map` `pairs_take_the_taller_half_when_wrapped`.
- [ ] **Step 2: Write the failing scenes:** `file-diff-side` (both themes, Visual): old on the left with its numbers,
  new on the right with its numbers, a 1 pt hairline; a removed and an added row level, an extra row facing a faintly
  shaded empty half; card titles and `⋯` rows across both halves; word marks on both halves; the button filled when on.
  `file-diff-side-narrow` (One, State): below 60 cells a half the view is unified, the button faint with the tooltip
  `Too narrow for side by side`, and side by side comes back when the pane is wide again; the choice is kept per tab
  (`FileTabEvent::SideBySide` reaches the model); a selection stays in the half it started in and Copy gives that
  half; find matches show on both halves and step left before right; the current row is on the half clicked.
- [ ] **Step 3: Run** — FAIL. **Step 4: Implement.** The fit is decided in the prepaint (part of the map's key) and
  kept in `fits_side`; the toolbar reads it, and a change of it notifies once (as Wrap's rescroll updates the kept
  scroll there).
- [ ] **Step 5: Run** — PASS; the gate; full `scripts/ui-shots.sh`.
- [ ] **Step 6: Commit** `M11ah Task 11: side by side diffs, per tab`.

### Task 12: Measure, docs, hand-off

**Files:**
- Modify: note 85 ("What was built", "On-screen check", **Status**), `docs/04-Development/40 - Roadmap.md` (M11ah's
  line), `docs/06-Progress-Log/60 - Progress Log.md`, this plan's table, `src/file_tab.rs` module docs (a "Code view
  (M11ah)" paragraph: outline, row map, folds, place, cards, side by side), `STRUCTURE.md` (the three new modules)

- [ ] **Step 1: Idle CPU (M11p anti-goal):** a file tab with a 900-line Rust file shown, nothing warm, debug build
  (`FUXX_DAEMON_SOCKET=/tmp/fuxx-dev.sock cargo run`, never `--release`): `top -l 6 -s 5 -pid <fuxx>` median on the
  branch and on master; both into note 85; they match within noise.
- [ ] **Step 2: Open time (anti-goal "a slow open"):** time from tab open to first text frame for a 15 MB generated
  text file and a 1.9 MB source file, branch vs master (log line or `app_log` timestamp); within the outline's 50 ms.
- [ ] **Step 3: Write note 85's on-screen check** (about 12 items): click a line (band, bright number); guides and the
  current block's; rounded bars; scroll into a function (sticky line, breadcrumb, clicks); fold with ▾ and ⌥⌘[, a fold
  over a change; ⌥↓ / ⌥↑ and the buttons; scrollbar marks with Find open; Diff cards, a stretch, a title fold; changed
  words; side by side wide and narrow; an agent edit with Follow on into a folded block; a 15 MB file opens as fast as
  before.
- [ ] **Step 4: Full gate and full `scripts/ui-shots.sh`**; numbers into note 85 and the Progress Log; roadmap M11ah
  "built, waiting for your check"; this plan's table (status, tasks done, pick up here).
- [ ] **Step 5: Commit** (docs and `src/file_tab.rs` by path) `M11ah Task 12: docs and hand-off`.
