---
tags: [development, plan, milestone]
milestone: M11o
status: planned
updated: 2026-11-10
---

# M11o Plan — Branch switcher

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

| At a glance | |
|---|---|
| **Design (the spec)** | [[66 - Milestone 11o - Branch Switcher]] |
| **Status** | ⬜ Planned 2026-11-10; not started |
| **Tasks done** | 0 / 6 |
| **▶ Pick up here** | Task 1 |
| **Decisions used** | Yours (2026-11-10): a modal; a colliding branch disabled with "conflict x file(s)"; a confirmation every time; ahead/behind if cheap; a background fetch on open plus a Fetch button; a typed name from origin, "not found" otherwise. AI's calls are marked in note 66. |
| **Needs your eyes** | 👀 the on-screen checklist written into note 66 by Task 6 |

**Goal:** A modal that switches the git branch of a terminal's folder, as note 66 describes.

**Architecture:** Three new files, split by what can be tested how. `branch_list.rs` is pure: it parses git's ref
listing, builds and orders the rows, their marks, the filter, the typed-name row, the conflict count and every line
of copy. `branch_git.rs` runs git (blocking, called on the background executor): read the refs, the dirty paths, the
paths a target changes, fetch, fetch one branch, switch; it is tested against real temporary repos with a bare
"origin". `branch_switcher.rs` is the GPUI modal (shaped like `host_picker.rs` / `palette.rs`) plus its `impl
Workspace` block (open, events, refresh), wired as a new overlay next to the palette. The openers (sidebar branch
line, status bar branch, ⌘K) come last.

**Tech Stack:** Rust, GPUI at the pinned rev, the system `git` (found by `git_branch::find_git`). Only GPUI APIs
already used in `palette.rs`, `host_picker.rs`, `status_bar.rs` and `sidebar.rs`. **No new crate.**

**Spec:** [[66 - Milestone 11o - Branch Switcher]]

## Global Constraints

- No new crate; git is run as a program, as `git_branch.rs` and `git_status.rs` do.
- Every git call off the UI thread (the background executor), never blocking a frame.
- git never waits on a terminal: `GIT_TERMINAL_PROMPT=0`, no stdin, its own process group killed at the cap.
- Caps: reads `GIT_CAP` (2 s, existing); `git switch` **10 s** (`SWITCH_CAP`); `git fetch` **30 s** (`FETCH_CAP`).
- Never stash, reset, force (`-f`), discard, or create a branch from scratch. Only `git switch <name>` and `git
  switch --track origin/<name>` change the work tree.
- Only the remote named `origin`.
- Nothing runs while the modal is closed (no new idle work).
- Copy, exactly: title `Switch branch · <repo folder name>`; box placeholder `type a branch name…`; marks `↑n ↓m`
  (non-zero parts only), `✓`, `local only`, `upstream gone`, `current`, `detached at <short sha>`; section `ORIGIN`;
  `conflict <n> file(s)`; `Fetching…`; button `Fetch`; `Fetch failed: <git's first line>`; typed row `Check out
  <name> from origin`; `Branch <name> not found`; confirmation `Switch to <name>?`, `<program> is running in this
  folder` / `<n> programs are running in this folder`, `<n> uncommitted file(s) come along`, `<n> commit(s) on no
  branch: after switching they are only in the reflog`; buttons `Cancel`, `Switch`; ⌘K action `Switch branch…`.
- Error text in `ui.ring_waiting` (the theme's existing error-text colour; there is no failure token); marks and
  the folder in `ui.text_faint`; the panel from `overlay_panel::shell` / `filter_box`.
- Both themes; keyboard reachable (↑/↓ skip disabled rows, Enter, Escape).

## Review Focus

1. **A typed name git would read as an option or an invalid ref** (`-f`, `a..b`, `foo bar`): it must never reach git
   as an argument; it reads `Branch <name> not found`. Test in Task 2 (`check-ref-format --branch` first).
2. **The terminal is in a subfolder of the repo:** dirty paths and changed paths must both be repo-root-relative or
   conflicts are never found. Test in Task 2.
3. **A repo with no commits yet (unborn HEAD) or no `origin`:** the modal opens, shows the current branch only, no
   Fetch button, no ORIGIN section, no error. Tests in Task 2 (git side) and Task 1 (rows).
4. **git refuses a switch the count said was free** (a race, or git stricter than the estimate): the modal stays,
   shows git's own message, nothing changed. Test in Task 2 (the error carries git's stderr) and Task 4.
5. **The modal closes, or another overlay replaces it, while a fetch or a count is running:** the late result is
   dropped without a panic or a stray notify. Test in Task 3.

---

### Task 1: `branch_list` — the pure model

**Files:**
- Create: `src/branch_list.rs` (and `mod branch_list;` in `src/main.rs`)
- Test: in-file `#[cfg(test)] mod tests`

**Interfaces:**
- Produces:
  - `pub enum Head { Branch(String), Detached(String) /* short sha */, Unborn(String) /* branch name */ }`
  - `pub enum Track { None, Even, Diverged { ahead: u32, behind: u32 }, Gone }`
  - `pub struct LocalRef { pub name: String, pub upstream: Option<String> /* e.g. "origin/foo" */, pub track: Track, pub date: i64 }`
  - `pub struct RemoteRef { pub name: String /* without "origin/" */, pub date: i64 }`
  - `pub fn parse_refs(out: &str) -> (Vec<LocalRef>, Vec<RemoteRef>)` — input is `git for-each-ref
    --format='%(refname)%00%(upstream)%00%(upstream:track)%00%(committerdate:unix)' refs/heads refs/remotes/origin`
    (one ref per line, fields NUL-separated); `refs/remotes/origin/HEAD` dropped.
  - `pub enum Target { Local(String), Origin(String) }` (`Target::name(&self) -> &str`)
  - `pub enum Mark { Current, Even, Diverged { ahead: u32, behind: u32 }, LocalOnly, UpstreamGone, None }`
  - `pub struct Row { pub target: Target, pub label: String, pub mark: Mark, pub conflict: Option<usize>, pub origin_section: bool }`
  - `pub fn rows(head: &Head, local: &[LocalRef], origin: &[RemoteRef], pushed: &HashMap<String, (u32, u32)>, conflicts: &HashMap<String, usize>) -> Vec<Row>`
    — `pushed` holds ahead/behind for local branches with no upstream but a same-named `origin/<name>` (note 66 §2).
  - `pub fn visible(rows: &[Row], query: &str) -> Vec<usize>` — `palette_match::fuzzy` on `label`, list order kept.
  - `pub fn typed_name(rows: &[Row], query: &str) -> Option<String>` — the trimmed query when non-empty and equal to
    no row's `Target::name`.
  - `pub fn selectable(row: &Row) -> bool` — not `Current`, no conflict > 0.
  - `pub fn conflict_count(dirty: &HashSet<PathBuf>, changed: &HashSet<PathBuf>) -> usize`
  - Copy: `pub fn mark_text(mark: &Mark) -> Option<String>`, `pub fn conflict_text(n: usize) -> String`,
    `pub fn running_line(programs: &[String]) -> Option<String>`, `pub fn confirm_lines(name: &str, programs:
    &[String], carried: usize, orphans: usize) -> Vec<String>` (title first, then each warning that applies).

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn refs_parse_with_their_tracking() {
    let out = "refs/heads/main\0refs/remotes/origin/main\0\01700000300\n\
               refs/heads/feature/rows\0refs/remotes/origin/feature/rows\0[ahead 2, behind 1]\01700000200\n\
               refs/heads/spike\0\0\01700000100\n\
               refs/heads/fix/cache\0refs/remotes/origin/fix/cache\0[gone]\01700000050\n\
               refs/remotes/origin/HEAD\0\0\01700000300\n\
               refs/remotes/origin/fix-login\0\0\01700000400\n";
    let (local, origin) = parse_refs(out);
    assert_eq!(local.iter().map(|l| l.name.as_str()).collect::<Vec<_>>(), ["main", "feature/rows", "spike", "fix/cache"]);
    assert_eq!(local[0].track, Track::Even);
    assert_eq!(local[1].track, Track::Diverged { ahead: 2, behind: 1 });
    assert_eq!((local[2].upstream.as_deref(), &local[2].track), (None, &Track::None));
    assert_eq!(local[3].track, Track::Gone);
    assert_eq!(origin.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(), ["main", "feature/rows", "fix/cache", "fix-login"]);
}

#[test]
fn rows_put_the_current_first_then_local_then_origin_by_date() {
    // head = main; local: main, feature/rows (newer), spike; origin: main, feature/rows, fix-login, old-one
    // expect labels: main (Current), feature/rows (Diverged 2/1), spike (LocalOnly), then ORIGIN fix-login, old-one
    // (origin rows with a same-named local branch are left out, tracked or not)
}

#[test]
fn a_branch_pushed_without_an_upstream_is_on_origin() {
    // local "spike" with upstream None, origin has "spike", pushed = {"spike": (1, 0)}
    // → its mark is Diverged { ahead: 1, behind: 0 }, and no ORIGIN row "spike"
}

#[test]
fn detached_and_unborn_heads() {
    // Head::Detached("abc1234") → first row label "detached at abc1234", mark Current, not selectable;
    //   every local branch is selectable.
    // Head::Unborn("trunk") with no refs → one row "trunk" (Current); no ORIGIN rows.
}

#[test]
fn conflicts_disable_a_row_and_count_the_overlap() {
    let dirty: HashSet<PathBuf> = ["src/a.rs", "notes.txt", "new.rs"].map(PathBuf::from).into();
    let changed: HashSet<PathBuf> = ["src/a.rs", "new.rs", "src/b.rs"].map(PathBuf::from).into();
    assert_eq!(conflict_count(&dirty, &changed), 2);
    // a row built with conflicts {"feature/old": 3} has conflict Some(3) and !selectable
    assert_eq!(conflict_text(3), "conflict 3 file(s)");
}

#[test]
fn the_box_filters_and_offers_a_typed_name() {
    // rows main, feature/rows, ORIGIN fix-login
    // visible("rows") == [index of feature/rows]; typed_name("rows") == Some("rows")
    // typed_name("fix-login") == None (an exact row exists); typed_name("  ") == None
}

#[test]
fn marks_and_confirmation_copy() {
    assert_eq!(mark_text(&Mark::Diverged { ahead: 2, behind: 1 }).as_deref(), Some("↑2 ↓1"));
    assert_eq!(mark_text(&Mark::Diverged { ahead: 2, behind: 0 }).as_deref(), Some("↑2"));
    assert_eq!(mark_text(&Mark::Even).as_deref(), Some("✓"));
    assert_eq!(mark_text(&Mark::LocalOnly).as_deref(), Some("local only"));
    assert_eq!(mark_text(&Mark::UpstreamGone).as_deref(), Some("upstream gone"));
    assert_eq!(running_line(&["Claude Code".into()]).as_deref(), Some("Claude Code is running in this folder"));
    assert_eq!(running_line(&["a".into(), "b".into()]).as_deref(), Some("2 programs are running in this folder"));
    assert_eq!(running_line(&[]), None);
    assert_eq!(
        confirm_lines("feature/rows", &["Claude Code".into()], 3, 2),
        ["Switch to feature/rows?", "Claude Code is running in this folder", "3 uncommitted file(s) come along",
         "2 commit(s) on no branch: after switching they are only in the reflog"]
    );
    assert_eq!(confirm_lines("main", &[], 0, 0), ["Switch to main?"]);
}
```

Fill the commented tests with the inputs and assertions their comments state.

- [ ] **Step 2: Run them; they fail** — `cargo test -q branch_list` → does not compile (no module).
- [ ] **Step 3: Implement the interfaces above in `src/branch_list.rs`.** Order (note 66 §2): the current row first;
  local branches by `date` descending; ORIGIN rows (`origin_section: true`) by `date` descending.
- [ ] **Step 4: Run them; they pass** — `cargo test -q branch_list` → all pass; `cargo clippy --all-targets -q` adds
  no warning.
- [ ] **Step 5: Commit** — `feat(m11o): branch_list — rows, marks, conflicts and copy for the branch switcher`

---

### Task 2: `branch_git` — git, against real repos

**Files:**
- Modify: `src/git_branch.rs` (a stderr-capturing runner)
- Create: `src/branch_git.rs` (and `mod branch_git;` in `src/main.rs`)
- Test: in-file tests using a temp repo and a bare `origin` (copy `git_status.rs`'s `git_in` / `temp` helpers'
  approach; skip when `find_git()` is `None`)

**Interfaces:**
- Consumes: `branch_list::{parse_refs, Head, LocalRef, RemoteRef, Target}` (Task 1); `git_branch::{find_git, run_git,
  run_git_bytes, GIT_CAP}`; `git_status::parse_porcelain`.
- Produces:
  - In `git_branch.rs`: `pub const SWITCH_CAP: Duration = Duration::from_secs(10);`, `pub const FETCH_CAP: Duration
    = Duration::from_secs(30);`, `pub(crate) fn run_git_checked(git: &Path, dir: &Path, args: &[&str], cap: Duration,
    env: &[(&str, &str)]) -> Result<String, String>` — like `git_command` but **without** `GIT_OPTIONAL_LOCKS=0`
    (it writes) and with stderr captured: `Ok(stdout)` on exit 0, else `Err(first non-empty stderr line)`, `Err("git
    timed out")` at the cap, `Err("git could not run")` if it doesn't start.
  - In `branch_git.rs`:
    - `pub struct Snapshot { pub root: PathBuf, pub head: Head, pub local: Vec<LocalRef>, pub origin: Vec<RemoteRef>, pub pushed: HashMap<String, (u32, u32)>, pub has_origin: bool, pub dirty: HashSet<PathBuf>, pub orphans: usize }`
    - `pub fn read(git: &Path, dir: &Path) -> Option<Snapshot>` — `None` outside a repo. `root` from `rev-parse
      --show-toplevel`; `head` from `symbolic-ref -q --short HEAD` (unborn when `rev-parse -q --verify HEAD` fails),
      else `rev-parse --short HEAD`; refs from Task 1's `for-each-ref` format; `pushed` from `rev-list --left-right
      --count <name>...origin/<name>` for each local branch with no upstream and a same-named origin ref; `has_origin`
      from `git remote`; `dirty` from `status --porcelain=v1 -z` run **at `root`** (every path root-relative);
      `orphans` only when detached: `rev-list --count HEAD --not --branches --remotes`.
    - `pub fn changed(git: &Path, root: &Path, target: &Target) -> Option<HashSet<PathBuf>>` — `diff --name-only -z
      --no-renames HEAD <ref> --` at `root`, `<ref>` = the branch or `origin/<name>`.
    - `pub enum FetchMode { Auto, Button }` — `Auto` env: `GCM_INTERACTIVE=never`, `GIT_ASKPASS=/usr/bin/false`,
      `SSH_ASKPASS_REQUIRE=never`; `Button`: none of these (helpers may open their own windows). Both keep
      `GIT_TERMINAL_PROMPT=0`.
    - `pub fn fetch(git: &Path, root: &Path, mode: FetchMode) -> Result<(), String>` — `fetch --prune origin`, `FETCH_CAP`.
    - `pub fn fetch_branch(git: &Path, root: &Path, name: &str, mode: FetchMode) -> Result<bool, String>` — first
      `check-ref-format --branch <name>` (invalid → `Ok(false)`), then `fetch origin
      refs/heads/<name>:refs/remotes/origin/<name>`; `Ok(false)` when git says the remote ref isn't there
      ("couldn't find remote ref"), `Ok(true)` when fetched.
    - `pub fn switch(git: &Path, root: &Path, target: &Target) -> Result<(), String>` — `switch <name>` or `switch
      --track origin/<name>`, `SWITCH_CAP`; never another flag.

- [ ] **Step 1: Write the failing tests** (each builds a repo with commits on `trunk`, `feature`, and a bare clone
  pushed as `origin`):
  - `read_lists_branches_tracking_and_dirt` — after `push -u origin feature`, one commit ahead locally: `feature`'s
    track is `Diverged { ahead: 1, behind: 0 }`; an untracked file and a modified file are both in `dirty`.
  - `paths_are_repo_relative_from_a_subfolder` — `read` called with `dir = root/src`: `dirty` holds `src/a.rs`, and
    `changed(.., Local("feature"))` holds the same spelling.
  - `an_unborn_repo_and_no_origin` — `git init` only: `read` gives `Head::Unborn("trunk")`, no refs,
    `has_origin == false`.
  - `a_detached_head_counts_its_orphans` — detach, commit once: `head` is `Detached(_)`, `orphans == 1`.
  - `fetch_prunes_and_fetch_branch_finds_or_not` — a branch made and deleted in origin disappears after
    `fetch(.., Auto)`; `fetch_branch(.., "only-on-origin", Auto) == Ok(true)` and `origin/only-on-origin` exists;
    `fetch_branch(.., "nope", Auto) == Ok(false)`; `fetch_branch(.., "-f", Auto) == Ok(false)`;
    `fetch_branch(.., "a..b", Auto) == Ok(false)`.
  - `switch_moves_and_refuses_with_gits_message` — `switch(Local("feature"))` → `Ok`, HEAD is `feature`;
    `switch(Origin("only-on-origin"))` → `Ok`, a local branch tracking `origin/only-on-origin`; with a dirty file that
    differs on the target, `switch` → `Err(msg)` where `msg` contains `would be overwritten`, and HEAD unchanged.
  - `no_origin_fetch_fails_cleanly` — `fetch` in a repo without origin → `Err(_)`, no panic.
- [ ] **Step 2: Run them; they fail** — `cargo test -q branch_git` → does not compile.
- [ ] **Step 3: Implement `run_git_checked` and the `branch_git` interfaces above.**
- [ ] **Step 4: Run them; they pass** — `cargo test -q branch_git` and `cargo test -q git_branch` → all pass.
- [ ] **Step 5: Commit** — `feat(m11o): branch_git — read refs and dirt, fetch, switch; git's stderr kept for errors`

---

### Task 3: The modal — list, filter, fetch, conflict counts

**Files:**
- Create: `src/branch_switcher.rs` (and `mod branch_switcher;` in `src/main.rs`)
- Modify: `src/workspace.rs` (field, `Overlay` variant, `close_overlays`, render through `top_panel`),
  `src/main.rs` (key bindings for the `"BranchSwitcher"` context, as the palette's), `src/ui_shots.rs` (scene)

**Interfaces:**
- Consumes: Tasks 1–2. `NameInput` (`name_input.rs`), `overlay_panel::{shell, filter_box}`, the palette's row
  look (`palette.rs` `item_row`: selected `row_active`, hover `panel_hover`).
- Produces:
  - `pub struct RepoAt { pub git: PathBuf, pub dir: PathBuf, pub label: String /* folder, home-relative */ }`
  - `pub struct BranchSwitcher` (an entity), `pub enum BranchSwitcherEvent { Switched { root: PathBuf }, Cancel }`,
    `impl EventEmitter<BranchSwitcherEvent>`, `Focusable` (the box's handle).
  - `BranchSwitcher::new(repo: RepoAt, programs: Vec<String>, window: &mut Window, cx: &mut Context<Self>) -> Self`
    — reads (`branch_git::read`) on the background executor, then starts `fetch(.., FetchMode::Auto)` if
    `has_origin`; after a fetch, reads again.
  - `#[cfg(feature = "ui-shots")] BranchSwitcher::seeded(repo: RepoAt, snapshot: Snapshot, conflicts:
    HashMap<String, usize>, window, cx) -> Self` — no git runs (for shots).
  - In `impl Workspace` (in `branch_switcher.rs`, as `palette.rs` does): `pub(crate) branch_switcher:
    Option<Entity<BranchSwitcher>>` on `Workspace`; `Overlay::BranchSwitcher`; `fn open_branch_switcher(&mut self,
    terminal: u64, window: &mut Window, cx: &mut Context<Self>)` (Task 5 calls it; here a test hook opens it);
    `fn on_branch_switcher_event(&mut self, sw: &Entity<BranchSwitcher>, event: &BranchSwitcherEvent, window, cx)`
    — ignore events from a replaced switcher (as `on_palette_event` does); `Cancel` closes and `focus_active`.

Behaviour to build (note 66 §1, §2, §3, §5): title `Switch branch · <root's folder name>` with `label` under it in
`text_faint`; the `Fetch` button (runs `fetch(.., FetchMode::Button)`, disabled while any fetch runs) and `Fetching…` /
`Fetch failed: …` beside it (no button when `!has_origin`); the
box; the rows with `mark_text` / `conflict_text`; an `ORIGIN` header before the first origin row; ↑/↓ move over
`selectable` rows only; typing filters (`visible`) and, when `typed_name` is `Some`, adds a last row `Check out
<name> from origin` (selectable). Conflict counts: only when `dirty` is non-empty; computed per row with
`branch_git::changed` + `conflict_count` on the background executor, at most 4 at a time, rows in view first; each
result sets that row's `conflict`. Every spawned task holds a `WeakEntity` and drops its result when the entity is
gone. Enter on a row: Task 4 (here it does nothing).

- [ ] **Step 1: Write the failing ui-shot `branch-switcher`** (copy `palette_scene`'s rig: `palette_build`,
  `switcher_workspace`, `render_with`, `save_shot`), both themes, opened `seeded` with: head `main`; local `main`,
  `feature/rows` (↑2 ↓1), `spike` (local only), `feature/old` (conflict 3); origin `fix-login`. Checks: the panel is
  drawn where `top_panel` puts the palette; the `feature/old` row's text is in `text_faint` and pressing ↓ from
  `spike` lands on the row after `feature/old` (it skipped it); typing `zzz` shows exactly one row (`Check out zzz
  from origin`); Escape closes it and focus is back on the active terminal (`active_terminal_focused`).
  And a unit-level test `a_late_result_after_close_is_dropped` (an entity dropped before its read completes: no
  panic, the workspace's `branch_switcher` stays `None`).
- [ ] **Step 2: Run it; it fails** — `cargo run -q --features ui-shots -- __ui-shots branch-switcher` → panics (no scene).
- [ ] **Step 3: Implement the modal and the overlay wiring.**
- [ ] **Step 4: Run it; it passes** — the scene prints `ui-shots: branch-switcher ok`; `cargo test -q` passes; look
  at both PNGs in `target/ui-shots/` and confirm they match note 66 §2's sketch.
- [ ] **Step 5: Commit** — `feat(m11o): the branch switcher modal — list, marks, filter, background fetch and conflict counts`

---

### Task 4: Confirm, switch, and after

**Files:**
- Modify: `src/branch_switcher.rs`, `src/ui_shots.rs`

**Interfaces:**
- Consumes: `confirm_lines`, `selectable`, `branch_git::{switch, fetch_branch, read}`, `Places::set_branch`,
  `git_branch::branch_at`.
- Produces: the confirm step inside the modal (note 66 §4, §6), `BranchSwitcherEvent::Switched { root }`, and in
  `on_branch_switcher_event`: on `Switched`, close, `focus_active`, then look the branch up again for every folder
  in `places` under `root` (`branch_at` on the background executor → `places.set_branch`), so rows and the status
  bar update without waiting for the 5 s refresh.

Behaviour: Enter or a click on a selectable row opens the confirm step showing `confirm_lines(name, programs,
dirty.len() - conflict, orphans)` (carried = dirty files not in that target's changed set; the count is already
known from Task 3's conflict pass, or 0 with a clean tree) with `Cancel` and `Switch` (default). Enter = Switch,
Escape = back to the list. The typed row: `fetch_branch` first (`Busy` while it runs) → `Ok(false)` shows `Branch
<name> not found` under the box; `Ok(true)` → read again, count its conflict → a conflict shows `conflict <n>
file(s)` in the confirm step with `Switch` disabled; else the confirm step. Switch runs `branch_git::switch` (`Busy`:
input ignored); `Err(msg)` returns to the list with `msg` under the box in `ring_waiting` and reads again; `Ok` emits
`Switched`. The `programs` list is given by the opener (Task 5).

- [ ] **Step 1: Write the failing ui-shot `branch-switcher-confirm`**, both themes: seeded with `programs = ["Claude
  Code"]`, two dirty files, orphans 0; select `feature/rows`, press Enter → the confirm step shows exactly
  `Switch to feature/rows?`, `Claude Code is running in this folder`, `2 uncommitted file(s) come along` and the two
  buttons; Escape → back to the list (still open); Enter, Enter on a seeded switcher whose git is a stub that fails
  (`seeded` takes `fail_switch: Option<String>` under `ui-shots`) → the list shows that message under the box.
- [ ] **Step 2: Run it; it fails.**
- [ ] **Step 3: Implement the confirm step, the switch, the typed-name path, and the refresh on `Switched`.**
- [ ] **Step 4: Run it; it passes** — scene `ok`; `cargo test -q` passes. Then the real thing once, in a scratch
  repo under the scratchpad (never a real project): open the modal from a test hook on a real `RepoAt`, switch, and
  check `git -C <scratch> branch --show-current`.
- [ ] **Step 5: Commit** — `feat(m11o): confirm and switch — warnings, git's refusal shown, rows and status bar refreshed`

---

### Task 5: The openers — sidebar branch line, status bar branch, ⌘K

**Files:**
- Modify: `src/sidebar.rs` (the `LineTwo::Place` branch line), `src/status_bar.rs` (the branch element),
  `src/palette_items.rs` (a `Switch branch…` command and its condition), `src/workspace.rs` (the `SwitchBranch`
  action and its listener), `src/main.rs` (menu entry if the parity test requires it), `src/ui_shots.rs`

**Interfaces:**
- Consumes: `open_branch_switcher(terminal, window, cx)` (Task 3).
- Produces:
  - `fn branch_terminal(&self, ws: WorkspaceId) -> Option<u64>` — the terminal the row's branch came from: the
    workspace's **first** terminal (`self.model.terminals(ws).first()`, as `sidebar_rows` reads it). *(Ruling: note
    66 says "the row's subject terminal"; the row's branch is actually read from the first terminal, so the switcher
    follows what was clicked.)*
  - `fn switcher_programs(&self, root: &Path) -> Vec<String>` — for each non-SSH session whose folder
    (`places.folder(id)`, else `project`) is under `root` and whose `programs.state(id)` is `Running(name)`: its
    `title` if set, else `name`.
  - `open_branch_switcher` builds `RepoAt` from the terminal's folder (refuses quietly when SSH, no folder, no git,
    or `places.branch(folder)` is `None`) and passes `switcher_programs`.
  - Palette: a `Place`-like condition that the command applies only when the focused terminal is in a repo and not
    SSH — add `in_repo: bool` to `applies` / `action_rows`, computed in `open_palette` from `palette_folder(cx)` and
    `places.branch`.

Behaviour: the sidebar branch line gets an `.id`, `.cursor_pointer()`, a left `on_mouse_down` that stops
propagation (no drag) and an `on_click` that stops propagation (no row select) and opens the switcher for
`branch_terminal(ws)`. The status bar branch element: same, for `right_sidebar.followed`. ⌘K: `Switch branch…` runs
`SwitchBranch`, which opens it for the focused terminal.

- [ ] **Step 1: Write the failing tests:** a unit test `the_switch_branch_command_needs_a_repo` (`action_rows` with
  `in_repo: false` has no `Switch branch…`; with `true` it does); a ui-shot `branch-switcher-open`: clicking the
  seeded row's branch line opens the switcher and leaves the selected workspace unchanged; clicking the status bar's
  branch opens it; an SSH workspace's row (`switcher_workspace` sessions 6–7) has no clickable branch.
- [ ] **Step 2: Run them; they fail.**
- [ ] **Step 3: Implement the three openers.**
- [ ] **Step 4: Run them; they pass** — and the whole suite: `cargo test -q`; every ui-shot scene, one at a time
  (the run stops at the first panic; `title-bar` already fails on master at 1x — note it, don't fix it here).
- [ ] **Step 5: Commit** — `feat(m11o): open the branch switcher from the row's branch, the status bar and ⌘K`

---

### Task 6: Docs

**Files:** note 66 (status, an "M11o as built" section, an on-screen checklist), [[40 - Roadmap]], [[00 - Plan
Tracker (Start Here)]], this plan's table, [[60 - Progress Log]], `STRUCTURE.md` (the three new files), README if it
lists features.

- [ ] **Step 1:** Write the on-screen checklist into note 66 (about 12 items: open from each of the three places; the
  marks against a real repo; a conflicting branch disabled with its count; the confirmation with an agent running;
  Escape back to the list; a switch updating the row and the status bar; the Fetch button; offline fetch failure; a
  typed origin branch; a typed name that doesn't exist; no branch line or ⌘K action outside a repo or over SSH).
- [ ] **Step 2:** Flip the statuses, add the Progress Log entry, commit — `docs(m11o): built — note 66 as built,
  checklist, roadmap, tracker, Progress Log`.
