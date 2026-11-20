# Git tab: docked GRAPH and smaller indented rows — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The Changes panel's GRAPH sits docked at the bottom (above the branch bar) with its own scrolling
commit list and a drag divider (1/4 to 1/2 of the shared room, default 1/3, remembered); Git-tab rows are 24 pt,
smaller and indented.

**Architecture:** `ChangesPanel::all_rows` splits into `change_rows` (top list: STAGED, CHANGES, `… N more`,
"No changes") and `GraphState::rows_shown` (bottom list: commits, "Load more"), each in its own `uniform_list`
with its own scroll handle. The GRAPH header is drawn fixed above its list. The divider drag follows the right
sidebar edge's pattern (`render_right_resize_edge`: a `resizing` flag, window-wide mouse capture, save on release).

**Tech Stack:** Rust, GPUI (pinned rev; `uniform_list`, `relative()` heights, `canvas` for bounds).

**Spec:** `docs/superpowers/specs/2026-11-20-git-tab-docked-graph-design.md`

## Global Constraints

- GRAPH height: at least 1/4, at most 1/2 of the room the two regions share; 1/3 at first; remembered as a fraction
  in the sessions file (`SavedState`), a file without it giving 1/3.
- Git-tab rows (headers, files, commits, "Load more", "… N more", "No changes"): 24 pt; name 12 pt; folder and
  counts 11 pt; check circle 12 pt; mark letter 11 pt; row content indented 16 pt more than today. One height for
  every row of a list (`uniform_list` measures one).
- The Files tab is unchanged (`right_sidebar::ROW`, `NAME`, … stay as they are).
- Chore on master (note 68), test first; gate: `cargo test --bin fuxx`, both clippy runs (no new warnings),
  `cargo fmt --check`, full `scripts/ui-shots.sh` (the 6 known display-dependent failures only).

## Review Focus

1. A tiny window (a room under 100 pt): the fraction still clamps and nothing overlaps the branch bar — Task 1's
   `graph_fraction` test with a 40 pt room.
2. A drag released outside the window: the drag ends and the fraction is saved once — Task 3 ends it like the
   right edge (a move without the button pressed).
3. GRAPH closed: no divider, its header alone at the bottom, the top list takes the rest — Task 3's scene.
4. A clean repo: "No changes since the last commit." stays in the top region, the GRAPH docked below — Task 2's
   `a_clean_repo_keeps_its_note_on_top`.
5. ◎ (go to HEAD) scrolls the GRAPH's own list, not the changes list — Task 2's `head_is_found_in_the_graph_rows`.

---

### Task 1: The fraction (geometry and memory)

**Files:** `src/graph_panel.rs`, `src/restore.rs`, `src/right_sidebar.rs` (`load`), `src/workspace.rs` (saved
state).

**Interfaces — produces:** `pub(crate) const GRAPH_MIN: f32 = 0.25; GRAPH_MAX: f32 = 0.5; GRAPH_DEFAULT: f32 =
1.0 / 3.0;` `pub(crate) fn graph_fraction(room_top: f32, room_height: f32, y: f32) -> f32` (the share for a divider
at window `y`: `(room_top + room_height - y) / room_height` clamped to `[GRAPH_MIN, GRAPH_MAX]`; a room of 0 or
less: `GRAPH_DEFAULT`); `pub(crate) fn saved_fraction(saved: Option<f32>) -> f32` (clamped; none or not finite:
`GRAPH_DEFAULT`); `GraphState.fraction: f32`; `SavedState.graph_fraction: Option<f32>` (serde default, skipped
when none).

- [ ] Failing tests: `the_graph_takes_a_quarter_to_a_half_of_the_room` (room top 0, height 400: y 300 → 0.25,
  y 250 → 0.375, y 100 → 0.5, y 390 → 0.25, y -50 → 0.5; height 40, y 20 → 0.5; height 0 → 1/3),
  `a_saved_fraction_is_clamped` (None → 1/3, 0.1 → 0.25, 0.9 → 0.5, NaN → 1/3, 0.4 → 0.4),
  `the_graph_fraction_round_trips_and_an_old_file_has_none` (restore.rs).
- [ ] See them fail; implement; `load` sets `changes.graph.fraction = saved_fraction(saved.graph_fraction)`; the
  saved state writes `Some(fraction)`; see them pass.

### Task 2: Two row lists

**Files:** `src/changes_panel.rs` (`RowItem` loses `GraphHeader`, `Commit`, `LoadMore`; `change_rows`), 
`src/graph_panel.rs` (`GraphRow { Commit(usize), LoadMore }`, `GraphState.scroll: UniformListScrollHandle`,
`rows_shown`, `graph_head_row`).

**Interfaces — produces:** `ChangesPanel::change_rows(&self) -> Vec<RowItem>` (today's `all_rows` without the
GRAPH part); `GraphState::rows_shown(&self) -> Vec<GraphRow>` (empty while closed; the commits, then `LoadMore`
when a full page came); `GraphState::graph_head_row(&self) -> Option<usize>`.

- [ ] Failing tests: `the_changes_list_holds_no_graph_rows` (2 entries, GRAPH open with commits: `[Divider(false),
  Entry(0), Entry(1)]`), `a_clean_repo_keeps_its_note_on_top` (`[Clean]`),
  `the_graph_rows_are_its_commits_then_load_more` (closed → empty; open, `limit` 2, 2 commits → `[Commit(0),
  Commit(1), LoadMore]`; 1 commit → `[Commit(0)]`), `head_is_found_in_the_graph_rows`.
- [ ] See them fail; implement (the panel's list uses `change_rows`; the sticky header handles `Divider` only; ◎
  scrolls `graph.scroll`); see them pass.

### Task 3: The docked GRAPH and its divider

**Files:** `src/changes_panel.rs` (`render_changes_panel`: the top region `flex_1`, the GRAPH section above the
bar), `src/graph_panel.rs` (`render_graph_section`, the divider, `resize_graph`, `end_graph_resize`;
`GraphState.resizing: bool`, `GraphState.room: Rc<Cell<Option<Bounds<Pixels>>>>`).

- [ ] The room (between the panel header and the bar) is recorded by a `canvas` each frame; the open GRAPH's box
  is `relative(fraction.clamp(GRAPH_MIN, GRAPH_MAX))` of it: its header, then its own `uniform_list`.
- [ ] The divider (open only): a `panes::GRAB` strip on the GRAPH header's top edge, row-resize cursor, the quiet
  line (`panes::resize_line`) on hover and while dragging; press sets `resizing`; a window-wide capture moves it
  with `graph_fraction` and ends it on release or a move without the button (saving once).
- [ ] Scene `changes-graph-docked` (both themes): open, the GRAPH header's top at `room bottom − room/3` (±1 pt);
  closed, its header right above the bar and no line; acting: a drag to the room's top gives 0.5, to its bottom
  0.25.

### Task 4: Smaller, indented rows

**Files:** `src/changes_panel.rs` (`GIT_ROW = 24.0`, `GIT_NAME = 12.0`, `GIT_SMALL = 11.0`, `CIRCLE = 12.0`,
`GIT_INDENT = 16.0`), `src/graph_panel.rs` (header, commit rows and lanes at `GIT_ROW`, text at `GIT_NAME` /
`GIT_SMALL`, indented `GIT_INDENT`).

- [ ] Failing test `git_rows_are_smaller_and_indented` (the constants; `GIT_ROW < right_sidebar::ROW`).
- [ ] Implement (the sticky header's row math uses `GIT_ROW`); update the changes scenes' geometry; run them.

### Task 5: Gate, docs, push

- [ ] Full gate; note 68 "Chores after M11q, part 5" with your on-screen check; Progress Log; commit; scrub; push.
