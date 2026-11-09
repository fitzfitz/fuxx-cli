---
tags: [development, plan, milestone]
milestone: M11h
status: built
updated: 2026-11-08
---

# M11h Plan — Changes, diff view, Activity

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

| At a glance | |
|---|---|
| **Design (the spec)** | [[59 - Milestones 11g-11h - Right Sidebar and File Tabs]] (the M11h parts, incl. "M11h settled before planning") |
| **Status** | 🟨 Built, reviewed, merged and pushed 2026-11-09 (1089 tests); release with M11l; on-screen check pending |
| **Tasks done** | 8 / 9 (Task 8, the release, is folded into the one release after M11n) |
| **▶ Pick up here** | Nothing to build. Your 14-item on-screen check in note 59 after the combined release |
| **Decisions used** | Yours (2026-11-08): note 59's design (Changes since the last commit, a diff mode for file tabs, Activity, two milestones), M11h after M11i. Everything else: AI's calls in note 59's "M11h settled before planning" (you skipped the design questions). |
| **Needs your eyes** | 👀 Task 8 (folded into the one release after M11n), then the 14-item on-screen check in note 59 |

**Goal:** the right sidebar gets its panel tabs (Files · Changes · Activity); Changes lists what differs from the
last commit with line counts; a click opens the file tab in a new diff mode (old and new lines in one column,
syntax colours on, a File | Diff toggle); Activity shows the followed terminal's timeline, its question and the
agent's notes and progress.

**Architecture:** Pure cores first, GPUI drawing second. `git_status.rs` gains the Changes list (Task 1) and a
file's whole diff (Task 2); a new pure `diff_view.rs` turns that diff into rows, colours and dimmed inks (Task 2).
The file tab gets a `FileMode`-driven diff body and toolbar toggle (Task 3). The right sidebar gets a `Panel`
switch in its strip and a new `changes_panel.rs` (Task 4). A pure `activity.rs` log rides on each `Session`,
filled by `set_status` and the note/progress API (Task 5), and `activity_panel.rs` draws it (Task 6). Everything
slow (git, reading files, highlighting) runs on the background executor through the existing capped runner
(`git_branch::run_git` / `run_git_outcome`, `GIT_CAP` 2 s).

**Tech Stack:** GPUI at the pinned rev (`1d2a4b3f…`), only APIs already used in this codebase (`uniform_list`,
`canvas`, `paint_quad`, `shape_line`, `PaintQuad::corner_radii`); syntect + two-face as added in M11g; `git` as a
process. **No new crates** (so no Context7 look-up is needed: no new crate API is called).

**Spec:** [[59 - Milestones 11g-11h - Right Sidebar and File Tabs]]

## Global Constraints

- **No new crates** (`Cargo.toml` `[dependencies]` unchanged). No new settings, key bindings or menu items; the
  scripting API is unchanged (Activity isn't exposed to scripts). Read-only: nothing writes a user file.
- **Nothing blocks the UI thread:** every git call, file read, line count and highlight runs on the background
  executor; git via `run_git` / `run_git_outcome` (2 s cap, process group killed at the cap). A failed or slow
  answer keeps the last good result; the Changes refresh backs off with `right_sidebar::back_off` (2 → 4 → … 30 s).
- **Panel tabs** (`right_sidebar::Panel`): labels exactly `Files`, `Changes`, `Activity`, in that order, in the
  card's 30 pt (`TAB_BAR`) strip filled `tab_bar` with the card's top corners (`CARD_RADIUS` 8); tabs
  `TAB_TOP_GAP` 4 from its top, flush with its bottom, top radius 6, 13 pt (`text::NAME`), 10 pt padding each side,
  2 pt apart, the first 6 pt from the strip's left; active fill `card` + `text`; inactive no fill + `text_dim`,
  `tab_hover` on hover; no border under the strip. The strip's empty part drags the window (as today).
  Saved: `SavedState.right_sidebar_panel: Option<String>` = `"files" | "changes" | "activity"`
  (`#[serde(default, skip_serializing_if = "Option::is_none")]`); missing or unknown → Files.
- **Only the shown panel works:** `refresh_files` runs only with Files shown, `refresh_changes` only with Changes;
  switching sets the new panel's `refreshed = None`. Leaving Files while `files.focus` contains the focus calls
  `focus_active`. Changes and Activity have no focus handle and no key context.
- **Changes (git):** `git status --porcelain=v1 -z --untracked-files=all --no-renames`;
  `git diff HEAD --numstat -z --no-renames --no-color --no-ext-diff` (skipped with no `HEAD`: every entry is
  counted from disk). Marks as `parse_porcelain` (M/A/D). Sorted by `(path.to_lowercase(), path)`.
  `CHANGES_CAP = 1_000` rows, then `… N more`. Untracked (and pre-first-commit) files counted from disk:
  `COUNT_FILES_MAX = 200` per refresh, each a plain file ≤ `PLAIN_ABOVE` (2 MB, made `pub(crate)`) with no NUL in
  its first 8 KB (`classify`); otherwise `Counts::Unknown`; NUL → `Counts::Binary`.
- **Changes (look):** header `ROOT_HEADER` 40 pt (line 1 20 pt: repo name 13 pt semibold `text`, right `N files` /
  `1 file` 11 pt `text_faint`; line 2 14 pt: totals `+A −R` 11 pt), padding as the Files header (10 left, 6
  right). Rows `ROW` 22 pt: 8 pt left pad, the mark letter in a 12 pt column in `mark_ink`, 6 pt, the name 13 pt
  `text`, 6 pt, the folder 11 pt `text_faint` cut at its end, the counts right-aligned 11 pt tabular with 8 pt
  right pad; hover `card_hover`; the focused diff tab's file `card_selected`; tooltip: the whole relative path.
  Counts `+12 −3` with U+2212, a zero side left out, `binary` in `text_faint`; inks: palette 2 (added) and 1
  (removed) through `readable(.., &[card, card_hover, card_selected], TEXT_MIN)`. Copy exactly:
  `No changes since the last commit.`, `Not a git repository.`, `Changes aren't shown for SSH terminals.`,
  `No terminal folder to show.` (the existing `NO_FOLDER`), `… N more`, `binary`.
- **One tab per file per pane:** `Model::open_file(pane, path, mode)` selects the pane's tab for `path` in any
  mode and sets `mode`; Files opens `Plain`, Changes `Diff`.
- **Diff (git):** `git --literal-pathspecs diff HEAD -U<n> --no-color --no-ext-diff -- <rel>` with
  `n = context_for(lines on disk)` = lines + 1 (a missing file: 1). Untracked, or no `HEAD`: all added from the
  disk text. Output with a `Binary files ` line and no `@@`: binary. `\ No newline at end of file` lines skipped.
- **Diff (look):** gutter `GUTTER_LEFT` 12, old numbers, `NUMBER_GAP` 8, new numbers, 8, a one-cell sign column,
  `TEXT_GAP` 10, text; numbers `text_faint`, each column as wide as its largest number (min 2 digits). Signs `−`
  (U+2212) in `BarColours::deleted`, `+` in `BarColours::added`. Row tints at `TINT` 0.12 (removed: deleted,
  added: added); unchanged rows: no tint, colours through `diff_view::dim` (mix toward the background
  `DIM_MIX = 0.35`, held at `min(contrast(c, bg), TEXT_MIN).max(MARK_MIN)`). Opens with its first change
  `REVEAL_ABOVE = 3` rows from the top; each mode keeps its own scroll. Over 2 MB: uncoloured; over 20 MB:
  `REFUSED`. Copy exactly: `No changes since the last commit.`, `This file is binary: no diff to show.`,
  `Not a git repository.`
- **Toggle:** two joined 24 pt (`BUTTON`) segments `File` | `Diff`, 13 pt, 10 pt padding, 1 pt `panel_border`,
  radius 5, before Open in Editor; current segment `card_selected` + `text`, the other `panel` + `text_dim`,
  `panel_hover` on hover; tooltips `Show the file` / `Show the changes since the last commit`. Shown when
  `shows_toggle(in_repo, mode)`: in a repo, or in Diff mode.
- **Diff refresh:** as M11g's change bars: on each new disk version, every `CHANGES_EVERY` (5) polls of the 1 s
  `POLL`, at most once per `LOOKUP_GAP` (2 s), one at a time (`OneAtATime`); in Diff mode the tab asks for the
  diff instead of the bars.
- **Activity:** `activity::MAX = 200` entries per terminal, in memory, on `Session`, gone with it. Status entries
  on every real change (Idle too); `lasted` = the previous status's length; Waiting keeps its question. Notes:
  blank, cleared or the same as the last note entry → nothing. Progress: cleared → nothing; a progress right after
  a progress entry replaces it (time updated). Notes/progress restored from the background service aren't
  entries. Times `HH:MM` local 24-hour (`cli::local_utc_offset`, made `pub(crate)`). Durations: `< 60 s` → `N s`,
  `< 60 min` → `N min`, else `H h M min` (`H h` when M is 0). Row words: `Working`, `Needs input`,
  `Done — 11 min` (em dash, `lasted`), `Idle`, the note, `Progress 42%` / `Progress 42% · build`.
- **Activity (look):** header 40 pt: the terminal's `terminal_label` (13 pt semibold), line 2 the status bar's
  `word_text(status_word(..), age)` 11 pt (`text_dim`; `ring_waiting` while Waiting). Waiting card: 6 pt from the
  sides and below the header, padding 8, radius 6, fill `waiting_tint`; `Needs input` 11 pt `ring_waiting`, then
  the question 13 pt `text`, wrapped. Timeline newest first, rows 22 pt: 8 pt pad, time 11 pt tabular
  `text_faint` in a 36 pt column, the still mark 10 pt (`status_mark::mark_on`, phase 0, no pop; Idle:
  `idle_mark(false)`) or a 4 pt `text_faint` dot for notes/progress, 6 pt, the words 13 pt `text`, cut at the end;
  tooltip: the whole words and a Waiting entry's question. Copy exactly: `No activity yet.`,
  `No terminal to follow.` Nothing animates (Reduce motion holds trivially).
- **Contrast floors on every built-in theme:** text ≥ 4.5:1 on its surface (counts on card/hover/selected; dimmed
  foreground on the background), marks and tints ≥ 3:1, as M11i's tests.
- Every visual change has ui-shots in **Catppuccin Mocha and Latte**. Existing tests keep passing; scenes whose
  pixels change are updated in the task that changes them (named per task).
- Subagents never `cargo run` the app. Allowed: `cargo test`, `cargo clippy --all-targets` (both feature sets),
  `cargo fmt --check`, `cargo run --features ui-shots -- __ui-shots [filter]`, `scripts/bundle-mac.sh --app-only`.

## Review Focus

1. **Huge or odd change sets.** An unignored `node_modules` (thousands of untracked files), renames, paths with
   spaces, unicode or a tab, binary files, a repo with no commits, a conflict. Expected: 1,000 rows then `… N
   more`, `binary` for binaries, no stall (git capped, counting bounded). (Task 1: `changes_cap_and_more`,
   `changes_end_to_end`, `changes_without_commits`, `numstat_counts_lines_and_binaries`.)
2. **Edge files in a diff.** Deleted, untracked, empty, no trailing newline, CRLF, only deletions, two hunks
   (the file changed between the line count and git's read). Expected: correct rows and numbers, a gap row rather
   than a wrong join. (Task 2: the `parse_diff` / `all_added` / `diff_outcome` cases and `file_diff_end_to_end`.)
3. **A noisy agent.** `fuxx progress` ten times a second, status flapping Working ↔ Waiting for an hour. Expected:
   the log stays at 200, progress is one entry, the panel stays smooth. (Task 5: `it_keeps_the_last_200`,
   `progress_updates_in_a_row_are_one_entry`.)
4. **Moving on while git runs.** Switching terminal or repo, or switching panels, during a Changes refresh or a
   diff look-up. Expected: an answer for another root or an older version is dropped; a failed answer keeps the
   list and backs off. (Task 4: `a_changes_answer_for_another_root_is_dropped_and_a_failure_keeps_the_list`;
   Task 3: the diff answer carries its version, as `accept_changes`.)
5. **Modes across clicks and relaunch.** A Diff tab restored for a file now clean, deleted, or outside a repo; a
   Files click on a file whose tab is in Diff; toggling repeatedly. Expected: one tab per file per pane, the right
   note, the mode saved. (Task 3: `opening_a_file_sets_its_mode_and_keeps_one_tab`, ui-shots `file-diff-states`
   and `file-diff-toggle`.)

---

### Task 1: Changes — the git core

**Files:** Modify `src/git_status.rs` (types, `parse_numstat`, `count_lines`, `changes`, tests), `src/highlight.rs`
(`PLAIN_ABOVE` → `pub(crate)`).

**Interfaces — Produces** (in `git_status.rs`):
- `#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum Counts { Lines { added: u32, removed: u32 }, Binary, Unknown }`
- `#[derive(Clone, Debug, PartialEq, Eq)] pub struct Change { pub path: PathBuf, pub mark: Mark, pub counts: Counts }`
- `#[derive(Clone, Debug, Default, PartialEq, Eq)] pub struct ChangeList { pub entries: Vec<Change>, pub more: usize }`
- `pub const CHANGES_CAP: usize = 1_000;` `pub const COUNT_FILES_MAX: usize = 200;`
- `pub fn parse_numstat(z: &[u8]) -> HashMap<PathBuf, Counts>` (`added\tremoved\tpath\0`, split on the first two
  tabs only; `-\t-` → Binary; malformed skipped)
- `pub fn count_lines(path: &Path) -> Counts` (blocking; plain file only, read through `PLAIN_ABOVE + 1`; lines as
  `all_added` counts them, as `Lines { added: n, removed: 0 }`)
- `pub fn changes(git: &Path, root: &Path) -> Option<ChangeList>` (blocking; None when status fails or hits the cap,
  or outside a repo; a failed numstat with a good status → the tracked entries' counts `Unknown`)

- [x] **Step 1: failing tests** (temp repos as `temp_repo_end_to_end` / `repo` / `git_in` do):
  - `numstat_counts_lines_and_binaries`: `parse_numstat(b"3\t1\tsrc/a.rs\0-\t-\tlogo.png\0")` →
    `{a.rs: Lines{3,1}, logo.png: Binary}`; `b"0\t0\tmode.sh\0"` → `Lines{0,0}`; `b"x\ty\0"` and `b""` → empty;
    `b"1\t0\ta\tb.txt\0"` → key `a\tb.txt`;
  - `count_lines_reads_only_small_text_files`: `"a\nb\n"` and `"a\nb"` → `Lines{2,0}`; empty → `Lines{0,0}`; a
    NUL in the first 8 KB → `Binary`; 2 MB + 1 byte → `Unknown`; a folder → `Unknown`;
  - `changes_end_to_end`: commit `a.rs` (3 lines), `b.rs`, `c.rs`; edit one line of `a.rs`; delete `b.rs`;
    `git mv c.rs d.rs`; add untracked `new dir/ü.txt` (2 lines) and `bin.dat` (with a NUL) →
    entries in order `a.rs` M `Lines{1,1}`, `b.rs` D `Lines{0,n}`, `bin.dat` A `Binary`, `c.rs` D, `d.rs` A,
    `new dir/ü.txt` A `Lines{2,0}`; `more == 0`; staging the `a.rs` edit gives the same entry;
  - `changes_cap_and_more`: 1,005 untracked one-line files → 1,000 entries, `more == 5`; entries past the 200th
    counted file are `Unknown`;
  - `changes_without_commits`: a fresh repo with `x.txt` (2 lines, staged) and `y.txt` (untracked) → both A
    `Lines{2,0}` / their counts;
  - `changes_outside_a_repo_is_no_answer`: a plain temp folder → `None`.
- [x] **Step 2: run** `cargo test git_status` → FAIL (names undefined).
- [x] **Step 3: implement** the signatures above (`--no-renames` on both commands; `rev-parse --verify -q HEAD`
  through `yes_or_no` decides whether numstat runs; sort, then cap, then count untracked within the cap).
- [x] **Step 4: run** `cargo test`, `cargo clippy --all-targets` (both feature sets), `cargo fmt --check` → green.
- [x] **Step 5: commit** `feat(m11h): Changes core — files changed since the last commit, with line counts`.

### Task 2: Diff core — a file's whole diff as rows

**Files:** Create `src/diff_view.rs`; Modify `src/git_status.rs` (`FileDiff`, `file_diff`), `src/main.rs` (`mod
diff_view`).

**Interfaces — Consumes:** `highlight::Span`, `ui_tokens::{contrast, ensure_contrast, MARK_MIN, TEXT_MIN}`,
`theme::mix`.
**Produces:**
- `git_status.rs`: `#[derive(Clone, Debug, PartialEq, Eq)] pub enum FileDiff { Unified(String), AllAdded, Binary }`;
  `pub fn file_diff(git: &Path, root: &Path, rel: &Path, context: usize) -> Option<FileDiff>` (blocking; None
  outside a repo or when git fails / hits the cap; the `has_head` / `tracked` checks shared with `line_changes`).
- `diff_view.rs`:
  - `#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum RowKind { Same, Added, Removed, Gap }`
  - `#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub struct DiffRow { pub kind: RowKind, pub old: Option<usize>, pub new: Option<usize> }` (1-based)
  - `#[derive(Clone, Debug, Default, PartialEq, Eq)] pub struct Parsed { pub text: String, pub rows: Vec<DiffRow> }`
    (`text`: one line per row, `\n`-joined, no prefix; a Gap row's line is empty)
  - `pub fn parse_diff(unified: &str) -> Parsed` (everything before the first `@@` skipped; a Gap row between
    hunks); `pub fn all_added(text: &str) -> Parsed`; `pub fn context_for(disk_lines: usize) -> usize`
  - `pub enum Outcome { Rows(Parsed), Same, Binary }`; `pub fn diff_outcome(diff: FileDiff, disk_text: Option<&str>) -> Outcome`
    (`Unified` with no rows → Same; `AllAdded` with `Some(text)` → `Rows(all_added)`, with None → Same)
  - `pub fn sides(p: &Parsed) -> (String, String)` (old text = Same + Removed lines, new = Same + Added, each line
    ending `\n`)
  - `pub fn diff_spans(rows: &[DiffRow], old: &[Vec<Span>], new: &[Vec<Span>]) -> Vec<Vec<Span>>` (Removed →
    `old[old-1]`, Same/Added → `new[new-1]`, Gap or out of range → empty)
  - `pub fn first_change(rows: &[DiffRow]) -> Option<usize>`
  - `pub fn reveal_row(rows: &[DiffRow], change: (usize, LineChange)) -> Option<usize>`: Added/Changed at new line
    n → the row with `new == Some(n)`, moved back over the Removed rows right before it; `DeletedAfter` at c → the
    row after the row with `new == Some(c)` (c = 0 → row 0)
  - `pub const DIM_MIX: f32 = 0.35;` `pub fn dim(c: Rgb, background: Rgb) -> Rgb` (Global Constraints rule)
  - `pub const NO_CHANGES: &str`, `BINARY: &str`, `NO_REPO: &str` (exact copy in Global Constraints)

- [x] **Step 1: failing tests:**
  - `parse_diff_keeps_every_row_in_order`: headers (`diff --git`, `index`, `---`, `+++`) then
    `@@ -1,3 +1,4 @@\n a\n-b\n+B\n c\n+d\n` → text `"a\nb\nB\nc\nd"`, rows `(Same,1,1) (Removed,2,–) (Added,–,2)
    (Same,3,3) (Added,–,4)`;
  - `no_newline_markers_are_skipped`; `crlf_lines_lose_their_cr` (`+a\r\n` → `"a"`);
  - `two_hunks_have_a_gap_between`: `@@ -1 +1 @@` … `@@ -9 +9 @@` → a `Gap` row (both numbers None) between;
  - `a_deleted_file_is_all_removed`: `@@ -1,2 +0,0 @@\n-x\n-y\n` → `(Removed,1,–) (Removed,2,–)`;
  - `all_added_numbers_every_line`: `"x\ny\n"` → `(Added,–,1) (Added,–,2)`; `""` → no rows;
  - `diff_outcome`: `Unified("")` → Same; `AllAdded` + `None` → Same; `Binary` → Binary; `AllAdded` + `Some("q\n")`
    → one Added row;
  - `sides_rebuild_old_and_new` and `diff_spans_take_each_row_from_its_side` (a Removed row gets `old[1]`, the Same
    row after it `new[1]`, a Gap row nothing);
  - `reveal_row`: a Changed line → its hunk's first Removed row; an Added line → its own row; `DeletedAfter` 3 →
    the row after new line 3; `DeletedAfter` 0 → 0; an unknown line → None; `first_change` of an all-Same list →
    None;
  - `dimmed_text_stays_readable_on_every_theme`: for every `theme::every_theme()`:
    `contrast(dim(fg, bg), bg) ≥ 4.5` and `< contrast(fg, bg)`; for palette 1–6 pushed to `MARK_MIN`,
    `contrast(dim(c, bg), bg) ≥ 3.0`;
  - `file_diff_end_to_end` (temp repo): a modified tracked file with `context_for(lines)` → `Unified` whose
    `parse_diff` rows hold every old and new line once; an untracked file → `AllAdded`; a repo with no commits →
    `AllAdded`; a modified binary → `Binary`; an unchanged file → `Unified("")`; a deleted file → only Removed
    rows; a name with a space and a `*` works; a plain folder → `None`.
- [x] **Step 2: run** `cargo test diff_view git_status` → FAIL.
- [x] **Step 3: implement** (pure functions as specified; `file_diff` flags per Global Constraints).
- [x] **Step 4: run** `cargo test`, clippy (both), fmt → green.
- [x] **Step 5: commit** `feat(m11h): diff core — a file's whole diff against the last commit, as rows`.

### Task 3: Diff mode in file tabs

**Files:** Modify `src/layout_model.rs` (`open_file` takes a mode, `set_file_mode`, tests), `src/file_tab.rs`
(mode, `DiffState`, the toggle, the two-column gutter, diff painting, the diff look-up, change-bar clicks,
per-mode scroll, `FileTabEvent::Mode`, `Workspace::open_file_tab(path, mode, ..)`, the event handler that sets the
model's mode and saves), `src/right_sidebar.rs` (`files_act` opens `FileMode::Plain`), `src/ui_shots.rs` (new
`file-diff`, `file-diff-states`, `file-diff-toggle`; `file-tab*` unchanged: a frozen tab has no git place, so no
toggle).

**Interfaces — Consumes:** Task 2 (`file_diff`, `diff_outcome`, `parse_diff`, `sides`, `diff_spans`, `first_change`,
`reveal_row`, `dim`, `context_for`, the copy).
**Produces:**
- `Model::open_file(&mut self, pane: PaneId, path: PathBuf, mode: FileMode) -> Option<Opened>`;
  `Model::set_file_mode(&mut self, tab: TabId, mode: FileMode) -> bool` (false: unknown, not a file tab, or the
  same mode).
- `file_tab.rs`: `pub enum DiffState { Waiting, Rows(DiffDoc), Same, Binary, NoRepo, Refuse }`;
  `#[derive(Clone)] pub struct DiffDoc { pub doc: Doc, pub rows: Arc<Vec<DiffRow>> }`;
  `pub enum Gutter { Plain { digits: usize }, Diff { old_digits: usize, new_digits: usize } }`;
  `pub struct Columns { pub old_right: Option<f32>, pub new_right: f32, pub bar_left: f32, pub text_left: f32 }`;
  `pub fn columns(cell: f32, gutter: Gutter) -> Columns` (pure; `Geom::of` uses it, `Geom` gains
  `old_numbers_right: Option<Pixels>`; in Diff `bar_left` is the sign column's left);
  `pub fn shows_toggle(in_repo: bool, mode: FileMode) -> bool`;
  `FileTab::new(path: PathBuf, mode: FileMode, window, cx)`; `pub fn mode(&self) -> FileMode`;
  `pub(crate) fn set_mode(&mut self, mode: FileMode, reveal: Option<(usize, LineChange)>, cx)`;
  `FileTabEvent::Mode(FileMode)` (the toggle, a change-bar click);
  `Workspace::open_file_tab(&mut self, path: PathBuf, mode: FileMode, window, cx)`;
  ui-shots only: `pub fn set_diff_for_shots(&mut self, unified: &str, cx)` (rows at once, coloured when the shared
  highlighter is loaded), `pub fn diff_state_name(&self) -> &'static str` (`waiting`, `rows`, `same`, `binary`,
  `norepo`, `refuse`), `pub fn diff_doc(&self) -> Option<&DiffDoc>`.

- [x] **Step 1: failing tests:**
  - `opening_a_file_sets_its_mode_and_keeps_one_tab`: open `/w/a.rs` Plain, then Diff → the same `TabId`, kind
    `File { mode: Diff }`; Plain again → same tab, Plain; another pane gets its own tab;
    `set_file_mode_is_false_for_a_terminal_or_the_same_mode`; a model with a Diff tab round-trips through
    `SavedWorkspace` with its mode (the existing `open_file` tests pass `FileMode::Plain`);
  - `columns_lay_out_one_or_two_number_columns`: `columns(8.0, Plain { digits: 2 })` → new_right 28, bar_left 36,
    text_left 49, old_right None; `columns(8.0, Diff { old_digits: 3, new_digits: 2 })` → old_right 36, new_right
    60, bar_left 68, text_left 86;
  - `the_toggle_shows_in_a_repo_or_in_diff_mode`: `(false, Plain)` → false; `(true, Plain)`, `(false, Diff)`,
    `(true, Diff)` → true.
  - ui-shots (Mocha, Latte):
    - `file-diff`: a tab on the `view.rs` fixture, `set_diff_for_shots` with a diff holding Same, Removed and
      Added rows: `diff_state_name() == "rows"`, coloured (`doc.spans` some); the pixel 6 pt inside the right edge
      of a Removed row ≈ red at 12 % over the background, of an Added row ≈ green at 12 %, of a Same row = the
      background; the sign column of a Removed row holds a pixel near `BarColours::deleted` and of an Added row
      near `added`; the old-number column of an Added row and the new-number column of a Removed row are
      background only; `geom.scroll.y == max(first_change − 3, 0) × line_height`; ⌘F `fn` counts the matches in
      the diff text; a drag over two rows and ⌘C copies their text with no `+`/`−` or numbers;
    - `file-diff-states`: `Same` shows `NO_CHANGES`, `Binary` shows `BINARY`, `NoRepo` shows `NO_REPO`; a missing
      file with Removed rows shows the rows, not `MISSING`; a missing file with `Same` shows `MISSING`;
    - `file-diff-toggle` (a workspace window): `open_file_tab(path, Diff)` → the toolbar's Diff segment is
      `card_selected`; a click on `File` → the tab's and the model's mode are Plain and the tab id unchanged;
      after `set_changes([(7, Changed)])`, a click on line 7's bar → Diff, with `reveal_row`'s row in view; a
      Files-panel click on the same file → the same tab, Plain.
- [x] **Step 2: run** `cargo test` → FAIL; `__ui-shots file-diff` → FAIL.
- [x] **Step 3: implement:** in Diff mode `look_up_changes` asks `file_diff(.., context_for(disk lines))` and builds
  `DiffState` off the UI thread (`diff_outcome`, then `Doc::new(parsed.text)`; colours: highlight both `sides` with
  the shared highlighter and `diff_spans`; skipped when the disk text is plain-too-large); no git place → `NoRepo`;
  disk `Refuse` → `Refuse`. The answer carries the disk version and is taken as `accept_changes` takes bars.
  `paint_body` takes the diff rows (tints, signs, two number columns, `dim` on Same rows) when the tab shows a
  diff. A press in the bar column (`bar_left − 2` to `bar_left + BAR_WIDTH + 2`) on a changed line or a ▸ edge
  emits `Mode(Diff)` with its change. Find and selection work on whichever `Doc` is shown; switching mode runs
  `find_again`.
- [x] **Step 4: run** `cargo test`, clippy (both), fmt, `__ui-shots file-diff`, `__ui-shots file-tab` and the full
  `__ui-shots`.
- [x] **Step 5: commit** `feat(m11h): diff mode in file tabs — old and new lines in one column, a File | Diff toggle`.

### Task 4: Panel tabs and the Changes panel

**Files:** Create `src/changes_panel.rs`; Modify `src/right_sidebar.rs` (`Panel`, the strip's tabs,
`show_right_panel`, refresh gating, the body by panel; `RightSidebarState` gains `panel` and `changes`; `load`
reads the panel; made `pub(crate)` for the new panels: `find_root`, `mark_ink`, `followed_ssh`, `ROOT_LINE`,
`PATH_LINE`, `ROOT_PAD_LEFT`, `ROOT_PAD_RIGHT`; until Task 6 the Activity tab's body is the `No activity yet.`
note), `src/restore.rs` (`right_sidebar_panel`), `src/workspace.rs` (the
save writes it), `src/main.rs` (`mod changes_panel`), `src/ui_shots.rs` (new `right-panels`,
`right-panels-narrow`, `changes`, `changes-empty`, `changes-click`; update `right-sidebar*` and `files*` where they
check the old "Files" strip).

**Interfaces — Consumes:** Task 1 (`changes`, `ChangeList`, `Change`, `Counts`, `CHANGES_CAP`), Task 3
(`open_file_tab(path, FileMode::Diff, ..)`, `FileTab::mode`), `back_off`, `REFRESH`, `mark_ink`, `followed_ssh`,
`right_sidebar_folder`.
**Produces:**
- `right_sidebar.rs`: `#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)] pub enum Panel { #[default] Files,
  Changes, Activity }` with `pub const ALL: [Panel; 3]`, `pub fn label(self) -> &'static str`,
  `pub fn key(self) -> &'static str` (`files`, `changes`, `activity`), `pub fn from_key(key: Option<&str>) ->
  Panel`; `Workspace::show_right_panel(&mut self, panel: Panel, window, cx)` (sets, saves, refreshes the new one
  at once, hands the keyboard back when Files held it).
- `restore.rs`: `SavedState.right_sidebar_panel: Option<String>`.
- `changes_panel.rs`: `pub(crate) struct ChangesPanel { .. }` with `pub(crate) fn new() -> Self`,
  `fn apply(&mut self, root: &Path, list: Option<ChangeList>) -> bool` (false and nothing done for another root;
  None keeps the list and `wait = back_off(wait, false)`), ui-shots `pub(crate) fn show_for_shots(&mut self, root:
  PathBuf, in_repo: bool, list: Option<ChangeList>)`; pure `pub enum CountsLabel { Lines { added: Option<String>,
  removed: Option<String> }, Binary, Nothing }`, `pub fn counts_label(c: Counts) -> CountsLabel`,
  `pub fn totals(list: &ChangeList) -> (u64, u64)`, `pub fn file_count_label(n: usize) -> String`,
  `pub fn split_path(rel: &Path) -> (String, String)` (name, folder; `""` at the top),
  `pub fn count_inks(theme: &Theme) -> (Rgb, Rgb)`; `impl Workspace { pub(crate) fn refresh_changes(&mut self, cx)
  ; pub(crate) fn render_changes_panel(&self, ui: UiColors, width: f32, cx) -> AnyElement }`.

- [x] **Step 1: failing tests:**
  - `panels_are_files_changes_activity_and_load_by_key`: `ALL` order and labels; `from_key(Some("changes"))` →
    Changes; `from_key(None)` and `from_key(Some("bogus"))` → Files;
  - `the_right_sidebar_panel_round_trips_and_an_old_file_shows_files` (as
    `the_right_sidebar_round_trips_and_a_file_without_it_starts_it_closed`);
  - `a_changes_answer_for_another_root_is_dropped_and_a_failure_keeps_the_list`: `apply(other, Some(..))` → false;
    `apply(root, None)` keeps the entries, wait 2 → 4 s; `apply(root, Some(..))` → wait `REFRESH`;
  - `counts_label`: `Lines{12,3}` → `("+12", "−3")`; `Lines{4,0}` → `("+4", None)`; `Lines{0,0}` → `Nothing`;
    `Binary` → `Binary`; `Unknown` → `Nothing`; `file_count_label(1) == "1 file"`, `(12) == "12 files"`;
    `split_path("src/ui/a.rs") == ("a.rs", "src/ui")`, `("a.rs") == ("a.rs", "")`; `totals` sums `Lines` only;
  - `count_inks_read_on_every_files_row_fill`: every built-in theme, both inks ≥ 4.5 on `card`, `card_hover`,
    `card_selected`.
  - ui-shots (Mocha, Latte):
    - `right-panels`: Files active: a pixel inside the Files tab = `card`, inside the Changes tab = `tab_bar`; the
      filter box focused, a click on `Changes` → `panel == Changes`, the saved state's `right_sidebar_panel ==
      Some("changes")`, and the focused tab's handle has the keyboard;
    - `right-panels-narrow` (220 pt sidebar): the Activity tab's right edge ≤ the card's right − 6;
    - `changes`: `show_for_shots` with 6 entries (M with `Lines{12,3}`, A, D, a binary, a nested path, a long
      name) and `more: 3`: the header reads the repo name and `9 files` and the totals; each letter's pixels are
      its `mark_ink`; `+12` pixels in the added ink, `−3` in the removed ink; the last row reads `… 3 more`;
      hovering a row fills it `card_hover`;
    - `changes-empty`: an empty list in a repo → `No changes since the last commit.`; not a repo →
      `Not a git repository.`; an SSH terminal followed → `Changes aren't shown for SSH terminals.`;
    - `changes-click`: a click on the M row → the focused pane shows a file tab for `root/rel` in Diff mode, and
      that row is drawn `card_selected`.
- [x] **Step 2: run** → FAIL.
- [x] **Step 3: implement** (`refresh_changes` mirrors `refresh_files`: a new folder looks its root up with
  `find_root`; then every `wait` while Changes shows, `changes` on the background executor; SSH and no-folder as
  the Files panel; the rows in a `uniform_list` with `ROW`).
- [x] **Step 4: run** `cargo test`, clippy (both), fmt, `__ui-shots right-panels`, `__ui-shots changes`, then the
  full set (update `right-sidebar*`, `files*` checks of the strip).
- [x] **Step 5: commit** `feat(m11h): right sidebar panel tabs and the Changes panel`.

### Task 5: Activity — the per-terminal log

**Files:** Create `src/activity.rs`; Modify `src/workspace.rs` (`Session.activity`; `set_status` records a real
change; `pub(crate) fn set_note(&mut self, index: usize, note: Option<String>, cx)` and `pub(crate) fn
set_progress(&mut self, index: usize, progress: Option<(u8, Option<String>)>, cx)`, which set the field, record,
`push_meta` and notify), `src/workspace_api.rs` (`session.note` / `session.progress` call them), `src/cli.rs`
(`local_utc_offset` → `pub(crate)`), `src/main.rs` (`mod activity`).

**Interfaces — Produces** (`activity.rs`):
- `pub const MAX: usize = 200;`
- `#[derive(Clone, Debug, PartialEq)] pub enum Kind { Status { status: Status, lasted: Duration, question:
  Option<String> }, Note(String), Progress(u8, Option<String>) }`
- `#[derive(Clone, Debug, PartialEq)] pub struct Entry { pub at: SystemTime, pub kind: Kind }`
- `#[derive(Default, Debug)] pub struct ActivityLog { entries: VecDeque<Entry> }` with
  `pub fn status(&mut self, status: Status, lasted: Duration, question: Option<String>, at: SystemTime)`,
  `pub fn note(&mut self, note: Option<&str>, at: SystemTime)`,
  `pub fn progress(&mut self, progress: Option<(u8, Option<String>)>, at: SystemTime)`,
  `pub fn newest_first(&self) -> impl Iterator<Item = &Entry>`, `pub fn len(&self) -> usize`,
  `pub fn is_empty(&self) -> bool`
- `pub fn clock_hm(unix_secs: i64, utc_offset_secs: i64) -> String`; `pub fn lasted_label(d: Duration) -> String`;
  `pub fn entry_text(kind: &Kind) -> String`

- [x] **Step 1: failing tests:**
  - `a_status_change_is_an_entry_with_how_long_the_last_lasted`;
  - `it_keeps_the_last_200`: 205 status entries → `len() == 200`, `newest_first().next()` is the 205th;
  - `progress_updates_in_a_row_are_one_entry`: progress 10 at t0, 20 at t1, 30 at t2 → one entry `Progress(30, _)`
    at t2; a status between two progresses keeps both;
  - `notes_skip_blank_cleared_and_repeats`: `Some("a")`, `Some("a")` → one; `Some("  ")`, `None` → none;
    `Some("b")` → a second;
  - `cleared_progress_is_not_an_entry`;
  - `clock_hm_is_local_24_hour`: `(0, 0)` → `00:00`; `(14*3600 + 2*60, 0)` → `14:02`; `(23*3600, 3600)` →
    `00:00`; `(30*60, -3600)` → `23:30`;
  - `lasted_label`: 45 s → `45 s`; 11 min → `11 min`; 65 min → `1 h 5 min`; 120 min → `2 h`;
  - `entry_text`: Working → `Working`; Waiting → `Needs input`; Done with 11 min → `Done — 11 min`; Idle → `Idle`;
    `Progress(42, Some("build"))` → `Progress 42% · build`; `Progress(42, None)` → `Progress 42%`; `Note("x")` →
    `x`.
  - (Recording through `set_status` / `set_note` / `set_progress` is checked in Task 6's `activity` scene.)
- [x] **Step 2: run** `cargo test activity` → FAIL.
- [x] **Step 3: implement** (`set_status`: when `old != new`, record `new` with `lasted = status_since.elapsed()`
  read before it's reset and the new question; adopted/restored meta isn't routed through `set_note`).
- [x] **Step 4: run** `cargo test`, clippy (both), fmt → green; the existing `workspace_api` note/progress tests pass.
- [x] **Step 5: commit** `feat(m11h): an activity log per terminal — status changes, notes and progress, in memory`.

### Task 6: The Activity panel

**Files:** Create `src/activity_panel.rs`; Modify `src/right_sidebar.rs` (Activity body), `src/main.rs` (`mod
activity_panel`), `src/ui_shots.rs` (new `activity`, `activity-waiting`, `activity-empty`).

**Interfaces — Consumes:** Task 5 (`ActivityLog`, `entry_text`, `clock_hm`), `cli::local_utc_offset`,
`status_bar::{status_word, word_text}`, `Programs::state`, `terminal_label`, `status_mark::{mark_on, idle_mark}`,
`right_sidebar.followed`.
**Produces:** `#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum RowMark { Status(Status), Dot }`;
`#[derive(Clone, Debug, PartialEq, Eq)] pub struct ActivityRow { pub time: String, pub mark: RowMark, pub text:
String, pub tip: Option<String> }`; `pub fn rows(log: &ActivityLog, offset: impl Fn(SystemTime) -> i64) ->
Vec<ActivityRow>` (newest first; tip = the words plus, for Waiting, the question on a second line, for a cut-able
note the whole note); `impl Workspace { pub(crate) fn render_activity_panel(&self, ui: UiColors, width: f32, cx)
-> AnyElement }`.

- [x] **Step 1: failing tests:**
  - `rows_are_newest_first_with_local_times_and_tips`: a log Working (t), Waiting with "Proceed? (y/n)" (t + 7 min),
    note "migrating", Done (lasted 11 min) with offset 0 → rows `Done — 11 min`, `migrating` (Dot), `Needs input`
    (tip contains the question), `Working`, times from `clock_hm`.
  - ui-shots (Mocha, Latte):
    - `activity`: a terminal driven through `set_status` Working → Working (no change) → Done, `set_note` "a", "a",
      `set_progress` 10, 20 → its log has 2 status entries, 1 note, 1 progress (`Progress 20%`); the panel shows
      them newest first; the header reads the terminal's name and the status word; switching the focus to a
      second terminal shows its (empty) log;
    - `activity-waiting`: Waiting with a question long enough to wrap → the card's fill pixel is `waiting_tint`,
      the card is at least two text lines tall, `Needs input` pixels are `ring_waiting`;
    - `activity-empty`: a terminal with no entries → `No activity yet.`; no terminal followed →
      `No terminal to follow.`; with Reduce motion on, two frames 200 ms apart are pixel-identical in the panel.
- [x] **Step 2: run** → FAIL. **Step 3: implement** (rows in a `uniform_list` of 22 pt; the header and the card as
  the constraints). **Step 4: run** `cargo test`, clippy (both), fmt, full `__ui-shots`, and
  `scripts/bundle-mac.sh --app-only` (a release build).
- [x] **Step 5: commit** `feat(m11h): the Activity panel — timeline, the waiting question, notes and progress`.

### Task 7: Merge and push (pre-authorized, autonomous program)

- [x] Full gate on the branch: `cargo test`, `cargo clippy --all-targets` (both feature sets), `cargo fmt --check`,
  full `__ui-shots` green; a whole-branch review; then merge `m11h-changes-activity` into `master` and push.

### Task 8: Release — folded into the one release after M11n

- [ ] ~~Version bump, `chore(release): …`, tag, watch the Release run~~ Skipped by ruling: one release after M11n
  (your rule, 2026-11-08).
- [ ] You: the 14-item on-screen checklist in note 59 (after the combined release).

### Task 9: Docs

- [x] Note 59: status, an "M11h as built" section (what the build settled or changed), the checklist's state.
  This plan (ticks, header). The roadmap (`40 - Roadmap.md`). The Plan Tracker. The Progress Log. `STRUCTURE.md`
  (`diff_view.rs`, `changes_panel.rs`, `activity.rs`, `activity_panel.rs`). `README.md` (Changes, the diff view,
  Activity).
- [x] Commit `docs(m11h): …`.
