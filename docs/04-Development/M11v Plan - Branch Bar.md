# M11v Branch Bar Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move the branch from the sidebar's group header into a bottom bar in the Changes tab (B3): the branch, where it stands against origin, and icon-only Fetch / Pull / Push / Publish buttons with tooltips above them.

**Architecture:** A new pure-plus-git module `src/branch_sync.rs` reads where the branch stands (local git only) and runs the four actions. `ChangesPanel` keeps the last `Sync`, the action under way, the last error and a hovered tip, reads `Sync` alongside its 2 s list refresh, and renders a one-line header and the bar. The sidebar's group header drops its branch button.

**Tech Stack:** Rust, GPUI (pinned rev `1d2a4b3`), the system `git` through `git_branch::{run_git, run_git_checked}`.

**Spec:** [[73 - Milestone 11v - Branch Bar]]

## Global Constraints

- Only remote `origin`. Pull = `pull --ff-only`; Push = `push` (no `--force`); Publish = `push -u origin HEAD`; Fetch = `branch_git::fetch` (`fetch --prune origin`).
- Caps: fetch / push / publish `FETCH_CAP` (30 s); pull `SWITCH_CAP` (120 s). `GIT_TERMINAL_PROMPT=0` (already in `run_git_checked`).
- Automatic fetch: `FetchMode::Auto`, only when the tab starts showing a root whose `FETCH_HEAD` is older than 5 min (or missing). Buttons use `FetchMode::Button` env.
- Tooltips: above the button, after 0.4 s hover; the rightmost aligned to its right edge.
- Copy (exact): "Fetch from origin", "Pull N commit(s) from origin" (`1 commit` / `2 commits`), "Push N commit(s) to origin", "Publish <branch> to origin"; lines per the spec's table.
- No spinner, no new timers beyond the 0.4 s tip delay; no confirmation dialogs.

## Review Focus

1. A pull blocked by uncommitted files → git's first line shown in red; the tree untouched (Task 2 test `pull_refuses_to_overwrite_local_changes`).
2. A branch name that starts with `-` or contains odd characters → never passed as an argument (Publish uses `HEAD`; Task 2 has no name argument anywhere).
3. Switching terminals mid-action → the answer for the old root is dropped (Task 4 test `an_action_answer_for_another_root_is_dropped`).
4. Detached HEAD / unborn branch / no origin → no action button, no crash (Task 1 tests).
5. `FETCH_HEAD` missing → no "fetched" part, auto-fetch due (Task 1 `ago_label` and `auto_fetch_due` tests).

---

### Task 1: `branch_sync` — read where the branch stands, and the words

**Files:** Create `src/branch_sync.rs` (add `mod branch_sync;` in `src/main.rs`).

**Interfaces — Produces:**
- `pub enum Head { Branch(String), Detached(String), Unborn(String) }`
- `pub enum Upstream { None, Gone, Track { ahead: u32, behind: u32 } }`
- `pub struct Sync { pub head: Head, pub upstream: Upstream, pub has_origin: bool, pub fetched: Option<SystemTime> }` (`Clone, PartialEq, Debug`)
- `pub enum Action { Fetch, Pull(u32), Push(u32), Publish }`
- `pub fn read(git: &Path, root: &Path) -> Option<Sync>` (blocking; local only)
- `pub fn parse_track(track: &str) -> Upstream` (`""` → Track{0,0}, `"ahead 1, behind 2"`, `"gone"`)
- `pub fn action(sync: &Sync) -> Option<Action>` (the one accent button: Pull when behind & !ahead; Push when ahead & !behind; Publish when Branch + Upstream::None + has_origin; else None)
- `pub fn shows_fetch(sync: &Sync) -> bool` (= `has_origin`)
- `pub enum Tone { Faint, Good, Warn, Bad }`; `pub fn line(sync: &Sync, now: SystemTime) -> (String, Tone)`
- `pub fn ago_label(fetched: SystemTime, now: SystemTime) -> String` (`just now` < 60 s, `N min ago`, `N h ago`, `N d ago`)
- `pub fn tip(action: Action, branch: &str) -> String`
- `pub fn working_text(action: Action) -> String` (`Fetching…`, `Pulling 2 commits…`, `Pushing 1 commit…`, `Publishing…`)
- `pub fn auto_fetch_due(fetched: Option<SystemTime>, now: SystemTime) -> bool` (none or older than 300 s)

- [ ] **Step 1: Failing tests** (`#[cfg(test)] mod tests` in the file):
  - `parse_track_reads_git_words`: `""`→Track{0,0}; `"ahead 1"`→{1,0}; `"behind 2"`→{0,2}; `"ahead 1, behind 2"`→{1,2}; `"gone"`→Gone.
  - `one_action_at_most`: behind 2 → `Pull(2)`; ahead 1 → `Push(1)`; both → None; up to date → None; Branch+None+origin → Publish; Branch+None+no origin → None; Gone → None.
  - `lines_read_as_the_spec`: up to date with fetched 3 min → (`"✓ Up to date · fetched 3 min ago"`, Good); behind 2 → `"↓2 behind · fetched 3 min ago"`; diverged → (`"⚠ ↓2 ↑1 diverged · merge or rebase in the terminal"`, Warn); no upstream → `"Local only · not on origin"`; no origin → `"No remote named origin"`; gone → `"Upstream gone · fetched 3 min ago"`; detached → `"Detached HEAD"`; never fetched → no ` · fetched …` part.
  - `ago_label_steps`: 10 s → `just now`; 180 s → `3 min ago`; 2 h → `2 h ago`; 3 d → `3 d ago`.
  - `tips_and_working_words`: `tip(Pull(1),"x")` = `"Pull 1 commit from origin"`; `tip(Push(2),"x")` = `"Push 2 commits to origin"`; `tip(Publish,"feat/x")` = `"Publish feat/x to origin"`; `tip(Fetch,_)` = `"Fetch from origin"`; `working_text(Pull(2))` = `"Pulling 2 commits…"`.
  - `auto_fetch_due_after_five_minutes`: None → true; 299 s → false; 301 s → true.
  - `read_sees_a_real_repo` (temp repos as `branch_git`'s tests: bare origin + work clone): tracking & equal → Track{0,0}, `has_origin`; a local commit → ahead 1; a new branch → Upstream::None; `git checkout --detach` → Head::Detached; no remote → `has_origin` false; a fetch writes FETCH_HEAD → `fetched` is Some.
- [ ] **Step 2:** `cargo test branch_sync` → fails (module missing).
- [ ] **Step 3:** Implement. Head from `symbolic-ref -q --short HEAD` (+ `rev-parse -q --verify HEAD` for unborn), else `rev-parse --short HEAD`; upstream for a branch via `for-each-ref --format=%(upstream)%00%(upstream:track,nobracket) refs/heads/<name>` (empty upstream → None); `remote` lines contain `origin`; fetched = mtime of the path printed by `rev-parse --git-path FETCH_HEAD` (relative to root).
- [ ] **Step 4:** `cargo test branch_sync` → pass.

### Task 2: `branch_sync` — the actions

**Files:** Modify `src/branch_sync.rs`, `src/branch_git.rs` (`FetchMode::env` → `pub(crate)`).

**Interfaces — Produces:** `pub fn run(git: &Path, root: &Path, action: Action, mode: FetchMode) -> Result<(), String>`. Fetch → `branch_git::fetch`; Pull → `["pull","--ff-only","--no-rebase"]` with `SWITCH_CAP`; Push → `["push"]`; Publish → `["push","-u","origin","HEAD"]`, each with `FETCH_CAP` and `mode.env()`.

- [ ] **Step 1: Failing tests:** `pull_fast_forwards` (origin gains a commit from a second clone, fetch, run Pull → `read` says Track{0,0} and the file has the new text); `pull_refuses_to_overwrite_local_changes` (same, plus an uncommitted edit to that file → `Err` containing `overwritten`, the edit still on disk); `pull_refuses_when_diverged` (`Err`, HEAD unchanged); `push_sends_a_local_commit` (ahead 1 → Push → Track{0,0}); `publish_makes_the_branch_on_origin` (new branch → Publish → `git ls-remote origin` lists it, upstream set).
- [ ] **Step 2:** run → fail. **Step 3:** implement `run`. **Step 4:** run → pass.

### Task 3: Glyphs for Pull, Push and Publish

**Files:** Modify `src/tab_bar.rs` (`enum Glyph`, `glyph_sized`).

**Interfaces — Produces:** `Glyph::Pull` (an arrow down to a line), `Glyph::Push` (an arrow up from a line), `Glyph::Publish` (a cloud outline with an arrow up), in the 12-unit box like the others. Fetch reuses `Glyph::Reload`.

- [ ] **Step 1:** add the variants and their paths. **Step 2:** `cargo build` clean; seen in Task 5's ui-shot.

### Task 4: The Changes panel — one-line header, the bar, the actions

**Files:** Modify `src/changes_panel.rs`.

**Interfaces — Consumes:** Tasks 1–3. `Workspace::open_branch_switcher(terminal, window, cx)`; `self.right_sidebar.followed`.
**Produces on `ChangesPanel`:** `sync: Option<Sync>`, `busy: Option<Action>`, `error: Option<String>`, `tip: Option<(TipAt, bool)>` (hovered button; shown after 0.4 s), `auto_fetched: Option<PathBuf>` (the root the automatic fetch last ran for). `fn apply_sync(&mut self, root: &Path, sync: Option<Sync>) -> bool` (dropped for another root; a branch change clears `error`); `fn action_done(&mut self, root: &Path, action: Action, result: Result<(), String>)`.

- [ ] **Step 1: Failing tests:** `an_action_answer_for_another_root_is_dropped` (rooted at /a, busy Pull, `action_done(/b, …, Err)` → busy still Some, error None); `a_failed_action_says_why_until_the_branch_changes` (`action_done(/a, Pull, Err("x"))` → error `"Pull failed: x"`, busy None; `apply_sync` with another branch → error None); `rerooting_forgets_the_bar` (`rooted` at another root → sync, busy, error None).
- [ ] **Step 2:** run → fail. **Step 3:** implement: `refresh_changes` also runs `branch_sync::read` in the same background task (return both); when `apply_sync` first lands for a root and `auto_fetch_due(sync.fetched)` and `auto_fetched != root`, run `Fetch` in `FetchMode::Auto` (sets `auto_fetched`). Header: one line (name, then `N files · +A −R`). Bar under the list (hairline above in `ui.panel_border`, a fill one step darker than the panel): left column `glyph(Branch)` + branch (13 pt semibold, `▾` faint) clickable → `open_branch_switcher(followed)`; under it the line in its tone (`Faint`→`text_faint`, `Good`→ the green count ink, `Warn`→ a yellow from the theme, `Bad`→ the red count ink), or `working_text` while busy, or the error. Right: 28 pt icon buttons (Reload, then the action's glyph filled with the accent), disabled while busy. Tooltip: `on_hover` sets `tip` and spawns a 400 ms timer that marks it shown; a `deferred` absolute `Tip` above the button (right-aligned for the last button).
- [ ] **Step 4:** `cargo test` all pass; `cargo clippy` no new warnings.

### Task 5: The sidebar header without the branch; ui-shots; docs

**Files:** Modify `src/sidebar.rs` (drop the header's branch button and `GroupHeader.branch`/`branch_from` if nothing else reads them), `src/ui_shots.rs` (fix scenes that expect the header branch; add `changes-bar` scenes: up to date, behind with the tip shown, diverged, no upstream, working, failed, in dark and latte). Docs: `12 - Scope.md`, `40 - Roadmap.md`, note 73's status, the Progress Log.

- [ ] **Step 1:** update the sidebar tests that assert the header's branch to assert it is gone; run → fail; remove; run → pass.
- [ ] **Step 2:** `cargo run --features ui-shots -- __ui-shots sidebar`, `… changes`, `… changes-bar`, `… branch` → all `ok`; look at the PNGs.
- [ ] **Step 3:** full `cargo test`, `cargo fmt --check`, `cargo clippy` (no new warnings).
- [ ] **Step 4:** docs updated (status 🟨 built, waiting for your check).

---

## Part 2 — Staged and Changes (C + R1; spec: note 73 "Part 2")

### Task 6: `git_status::changes` reads both sides

**Files:** Modify `src/git_status.rs` (and every `Change { … }` constructor: add `staged`).

**Interfaces — Produces:** `Change.staged: bool`; `pub fn parse_sides(z: &[u8]) -> Vec<(PathBuf, Mark, bool)>` (porcelain v1 `XY`: X ≠ ' '/'?' → a staged entry with X's mark; Y ≠ ' ' → an unstaged entry with Y's mark; `??` → unstaged Added; `U`/`AA`/`DD` → one unstaged Modified; `!!` skipped); `changes()` returns entries sorted staged first, then by path; counts from `diff --cached --numstat -z --no-renames` (staged) and `diff --numstat -z --no-renames` (unstaged), untracked counted from disk.

- [ ] Failing tests: `parse_sides_splits_index_and_worktree` (`M ` → staged M; ` M` → unstaged M; `MM` → both; `A ` → staged A; `AM` → staged A + unstaged M; `D ` staged D; ` D` unstaged D; `??` unstaged A; `UU` one unstaged M); `changes_reads_staged_and_unstaged_counts` (real repo: a file staged then edited again appears twice with each part's counts; an untracked file unstaged with its line count; a staged new file staged Added).
- [ ] Implement; run `cargo test git_status` → pass; full suite.

### Task 7: `src/git_stage.rs` — stage, unstage, revert

**Interfaces — Produces:** `pub enum FileWork { Stage(Vec<PathBuf>), Unstage(Vec<PathBuf>), Revert { tracked: Vec<PathBuf>, untracked: Vec<PathBuf> } }`; `pub fn run(git, root, work: &FileWork) -> Result<(), String>` (cap `FETCH_CAP`, refusal via `branch_sync::refusal`); `pub fn working_text(&FileWork) -> &'static str` (`Staging…`, `Unstaging…`, `Reverting…`); `pub fn verb(&FileWork) -> &'static str` (`Stage`, `Unstage`, `Revert`); `pub fn revert_words(name: &str, counts: Counts, untracked: bool) -> (String, String, &'static str)` (title, body, button); `pub fn revert_all_words(tracked: usize, untracked: usize) -> (String, String, &'static str)`; `pub fn staged_summary(list: &ChangeList) -> (usize, usize)` (files with a staged part, distinct files).

- [ ] Failing tests (real repos): `stage_then_unstage`; `unstage_before_the_first_commit`; `revert_keeps_the_staged_part`; `revert_deletes_a_new_file`; `paths_with_spaces_and_a_leading_dash`; pure: `revert_words_say_what_is_lost`, `staged_summary_counts_files_not_parts`.
- [ ] Implement; tests pass.

### Task 8: The panel — circles, groups, revert dialog

**Files:** Modify `src/changes_panel.rs`.

**Interfaces:** `running: HashMap<PathBuf, Work>` with `pub(crate) enum Work { Bar(Action), Files(FileWork) }` (one git write per repo); `confirm: Option<Confirm>` (`Confirm { title, body, button, work: FileWork }`); `fn run_file_work(&mut self, work: FileWork, cx)`; `fn ask_revert(..)`; rows rendered from a flat list of items: dividers + entries (`uniform_list` keeps one height: dividers take a row).

- [ ] Failing tests: `file_work_and_bar_actions_never_overlap` (Files running → busy blocks a bar action, and the reverse); `rerooting_closes_the_dialog`; `the_rows_are_staged_then_changes_under_dividers` (pure `rows_of(list) -> Vec<RowItem>`).
- [ ] Implement render: header circle + `N of M staged` + 3 pt progress; dividers; row circles; hover ↺; the dialog over the panel with a scrim.
- [ ] Tests pass; fmt; clippy.

### Task 9: ui-shots and docs

- [ ] `changes` scene adjusted; new `changes-staging` scenes (mixed in both themes; all staged; the dialog) with pixel checks for the green circles and the dialog's red button. Run every scene.
- [ ] Docs: note 73 "As built" for Part 2, Progress Log, Roadmap.

---

## Part 3 — Status Bar on/off (note 73 "Part 3")

### Task 10: `status-bar` setting

- [ ] Failing tests: the setting parses `on`/`off` (default on) and round-trips; the workspace's layout leaves out the status bar when off (a model/pure check where the layout is computed).
- [ ] Implement: the setting, the General row (On/Off switch like the others), the render skips the bar.
- [ ] ui-shots: `preferences` (the new row), a scene with the bar off.

---

## Part 4 — Files / Activity redesign, sizes, chevrons (note 73 "Part 4")

### Task 11: shared sizes and the Files tab (A)
- [ ] Constants: right sidebar `ROW` 22→28, text 14 / 12, icons 16, header 42, tab labels 13; Changes adopts them.
- [ ] Files: header (name + tooltip path + Collapse all icon button), borderless filter field, tree without the
  disclosure column (folder icon open/closed; click toggles), quiet grey icons, colour only on changed names and
  letters, struck-through deleted names, a dot on folders with changes, 16 pt hairline guides, pill hover/selection.
- [ ] Tests for any pure helper (e.g. a folder-has-changes check); `files`, `right-sidebar`, `right-panels`,
  `changes`, `changes-staging`, `changes-bar` ui-shots updated to the new geometry and passing.

### Task 12: Activity (A)
- [ ] Header with the status pill; the timeline rows (rail, dot, title, second line, time, progress bar, question
  box), a day heading. Pure helpers tested (the second line's words, the day heading). `activity` ui-shots updated.

### Task 13: thin chevrons
- [ ] A thin stroked chevron glyph (› / ⌄); the left sidebar's group headers use it at the far right after the
  count (name 14 pt, count 12 pt); any other ▸/▾ disclosure in the app too. `sidebar`, `workspaces` ui-shots pass.

### Task 14: verify and docs
- [ ] Full `cargo test`, fmt, clippy (no new warnings), every ui-shot scene; note 73, Progress Log, Roadmap.
