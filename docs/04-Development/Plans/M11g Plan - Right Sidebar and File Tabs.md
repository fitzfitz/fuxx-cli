---
tags: [development, plan, milestone]
milestone: M11g
status: built
updated: 2026-11-08
---

# M11g Plan — Right sidebar, Files, file tabs

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

| At a glance | |
|---|---|
| **Design (the spec)** | [[59 - Milestones 11g-11h - Right Sidebar and File Tabs]] (M11g parts; "Settled after a builder's read") |
| **Status** | 🟨 Built and reviewed 2026-11-08 (953 tests); merging and releasing v0.4.0 (your "7-8 go ahead") |
| **Tasks done** | 8 / 9; v0.4.0 released; your on-screen check pending |
| **▶ Pick up here** | Task 8: your on-screen check (14 items in note 59) after v0.4.0 |
| **Decisions used** | Yours (2026-11-08): the right sidebar, a file explorer opening files in the middle (read-only), syntect + two-face, line numbers and change bars, an icon on every pane; approved "lets go". The rest: AI's calls in note 59. |
| **Needs your eyes** | 👀 Tasks 7–8 (merge/push/release on your word, then on screen) |

**Goal:** a toggleable right sidebar that follows the focused terminal and shows its repo's files with git markers;
a click opens a read-only file tab in the pane, with syntax colours, line numbers, change bars, live reload, find
and copy.

**Architecture:** Pure cores first: `highlight.rs` (syntect + two-face, a syntect `Theme` built from the fuxx
theme, file classification), `git_status.rs` (repo root, status marks, ignore checks, per-file line changes parsed
from `git diff HEAD -U0`, via the existing capped `git` runner), and `file_tree.rs` (a lazy tree model). Then the
model (`TabKind::File`), the sidebar shell (`right_sidebar.rs`, mirroring the left sidebar's state, slide and
resize), the Files panel, and the file view (`file_tab.rs`: a `uniform_list` of shaped lines). Everything slow runs
on the background executor, and polling follows `tick_places` (visible-only, a `*_ticking` flag).

**Tech Stack:** GPUI at the pinned rev; new crates **syntect 5.3** and **two-face 0.5** (pure-Rust regex);
`git` through `git_branch.rs`'s runner.

**Spec:** [[59 - Milestones 11g-11h - Right Sidebar and File Tabs]]

## Global Constraints

- **New crates, exactly:**
  `syntect = { version = "5.3", default-features = false, features = ["parsing", "regex-fancy"] }` and
  `two-face = { version = "0.5", default-features = false, features = ["syntect-fancy"] }`. Nothing else new. No
  `onig`: `cargo tree -i onig` must be empty. No file watcher, no git library.
- **Read-only:** no code path writes, renames or deletes a user file. The only processes run are `git` (via
  `find_git`/the capped runner, `GIT_CAP` 2 s) and `/usr/bin/open <path>` for Open in Editor.
- **Nothing blocks the UI thread:** listing folders, git, reading files and highlighting run on the background
  executor. The syntax set (`two_face::syntax::extra_newlines()`) is loaded once, off the UI thread.
- **Right sidebar:**
  - Toggle `alt-cmd-\\` (action `ToggleRightSidebar`, global); View menu item `Toggle Right Sidebar` (after
    `Toggle Sidebar`); a `BarButton` at the far right of every pane's tab bar, drawn as a box with its right side
    filled, **lit** (`ui.text` glyph on `ui.row_hover` fill) while the sidebar is open.
  - Width min **220**, max **560**, default **300** pt.
  - Remembered in `sessions.json` (`SavedState`): `right_sidebar_hidden: bool` (default **true**, so it starts
    closed) and `right_sidebar_width: Option<f32>`, both `#[serde(default)]`. This is NOT in the settings file
    (ruling: the left sidebar's state lives there too).
  - Slides like the left sidebar (160 ms), not when Reduce motion is on.
  - In M11g it shows **only the Files panel**, under a "Files" header. M11h adds the panel tabs.
- **Files panel:**
  - Root: inside a repo, `git rev-parse --show-toplevel`, with the terminal's current folder expanded and revealed;
    otherwise the terminal's current folder.
  - Folders first, then files, by name (case-insensitive). Listed lazily, one folder per expand, at most
    **10,000** entries, then a row `… N more`.
  - Ignored entries hidden: one `git check-ignore --stdin` batch per listed folder. `.git` is always hidden;
    outside a repo, dot-folders are hidden.
  - Marks from `git status --porcelain=v1 -z`: `M` modified, `A` added/untracked, `D` deleted, on the file and its
    parent folders. Colours: the theme's ANSI **blue** (M), **green** (A), **red** (D).
  - Filter box (name contains, case-insensitive; matching files show with their folders).
  - Keyboard when focused: ↑ ↓, → ← (expand/collapse), Return (open), Esc (focus back to the pane).
  - Expanded folders remembered **per root** for the run.
  - Refresh: when shown, when the focused terminal or its folder changes, and every **2 s** while visible.
  - SSH terminals: the panel says "Files aren't shown for SSH terminals."
- **File tabs:**
  - `TabKind::File { path: PathBuf, mode: FileMode }`, `FileMode { Plain, Diff }` (only Plain is used in M11g);
    `SavedTab::File { path, #[serde(default)] mode }`.
  - Title: the file name, with a file glyph; tooltip: the full path.
  - Opening a file whose tab is already in the focused pane selects it.
  - Read-only. Gutter line numbers (`ui.text_faint`, right-aligned).
  - Syntax colours from a syntect `Theme` built at runtime from the fuxx theme, rebuilt on theme change.
  - Change bars: **3 pt** in the gutter, ANSI green = added, blue = changed, plus a red **▸** marker between lines
    where lines were deleted. Each changed line gets a tint of its colour at **12%**. They come from
    `git diff HEAD -U0 -- <file>`; an untracked file or a repo with no commits is all green; no repo, no bars.
  - Live reload: every **1 s** while visible (mtime + length), keeping the scroll.
  - Long lines scroll horizontally. Tabs show as 4 spaces.
  - ⌘F find bar with next/previous and "n of m"; ⌘G / ⇧⌘G; matches tinted, the current one stronger. Actions
    `Find`, `FindNext`, `FindPrev` in key context `FileTab`.
  - Mouse selection (line/column) and ⌘C; `ctrl-tab` / `ctrl-shift-tab` NextTab/PrevTab in `FileTab` too.
  - Toolbar button `Open in Editor` → `/usr/bin/open <path>`.
  - Limits: over **2 MB** → plain text, no colours, a note "Too large to colour"; over **20 MB** or a NUL in the
    first **8 KB** → "This file is too large or binary to show" + Open in Editor; missing → "This file no longer
    exists" + its path + Close.
- **Help panel:** `help_shortcuts::describe` lists `ToggleRightSidebar` under **Window & app** and the file tab's
  `Find` / `FindNext` / `FindPrev` under **Tabs**, so the "every binding is listed" test passes.
- Subagents never `cargo run` the app. Allowed: `cargo test`, `cargo clippy --all-targets` (both feature sets),
  `cargo fmt --check`, `cargo run --features ui-shots -- __ui-shots [filter]`, `scripts/bundle-mac.sh --app-only`.
  Release builds are needed to judge highlighting speed (fancy-regex is very slow in debug).

## Review Focus

1. **A huge repo or folder** (`node_modules` outside git, a monorepo). Expected: expanding stays responsive (lazy,
   capped, ignored folders never walked), no UI stall. (Task 2: `list_dir` cap test; Task 5: the tree model never
   lists unexpanded folders.)
2. **The agent rewriting the open file every second.** Expected: the tab reloads without flicker or losing its
   scroll, highlighting catches up off-thread, no growing memory. (Task 6: the reload keeps `scroll_top`; stale
   highlight results for an older version are dropped.)
3. **Odd files:** binary, 50 MB, no newline at the end, CRLF, invalid UTF-8, a file deleted while open, a symlink
   pointing outside the repo. Expected: a clear message or a lossy display, no panic, nothing written. (Task 1:
   `classify` tests; Task 6: the missing/binary scene.)
4. **No git, an SSH terminal, a folder outside any repo, a repo with no commits.** Expected: the tree still works
   where it can, no bars when there's no repo, the SSH message, no error spam. (Task 2: `line_changes` and `status`
   tests on a temp repo with no commits and on a non-repo dir.)
5. **Old `sessions.json` files** (no right-sidebar fields, no File tabs) and a saved File tab whose file is gone.
   Expected: they load; the sidebar starts closed; the missing file shows its message. (Task 3: restore tests.)

---

### Task 1: Highlighting core (pure) and the new crates — ✅

**Files:** Modify `Cargo.toml`, `Cargo.lock`; Create `src/highlight.rs`; Modify `src/main.rs` (`mod`).

**Interfaces — Produces:**
- `pub enum Viewable { Text, PlainTooLarge, Refuse }` and `pub fn classify(len: u64, head: &[u8]) -> Viewable`
  (2 MB / 20 MB / a NUL in `head`, where `head` ≤ 8 KB).
- `pub struct Highlighter` (owns the `SyntaxSet` and a syntect `Theme`), with
  `pub fn load(theme: &crate::theme::Theme) -> Highlighter` (blocking; call off-thread) and
  `pub fn set_theme(&mut self, theme: &Theme)`.
- `pub type Span = (std::ops::Range<usize>, crate::theme::Rgb)` and
  `pub fn highlight(&self, path: &Path, text: &str) -> Vec<Vec<Span>>`: one Vec per line, byte ranges into that
  line. The syntax is found by extension, then by first line, else plain.
- `pub fn syntect_theme(theme: &Theme) -> syntect::highlighting::Theme`, mapping scopes to the theme's colours:
  comment → faint/bright-black, string → green, keyword/storage → magenta, constant/number → yellow (or orange),
  entity.name.function → blue, entity.name.type/support.type → cyan, variable/default → foreground, invalid →
  red. AI may refine.

- [x] **Step 1: failing tests:**
  - `classify` at each boundary (2 MB exactly vs +1; 20 MB vs +1; a NUL at byte 0 and at byte 8191; text);
  - `highlight` on a small Rust snippet gives ≥ 3 distinct colours and covers every byte of each line;
  - an unknown extension gives a single foreground span per line;
  - `syntect_theme` for Mocha and for Latte uses the theme's own green for strings;
  - an empty file gives one empty line.
- [x] **Step 2: run** → FAIL. **Step 3: implement** (add the crates with the exact lines above; verify the APIs with
  Context7 `/trishume/syntect` and docs.rs two-face). **Step 4: run** `cargo test`, `cargo tree -i onig` (empty),
  clippy (both), fmt.
- [x] **Step 5: commit** `feat(m11g): highlighting core — syntect + two-face, colours from the fuxx theme`.

### Task 2: Git status, line changes and folder listing — ✅

**Files:** Create `src/git_status.rs`; Modify `src/git_branch.rs` (make `run_git`/`run_capped` `pub(crate)`; add a
`run_capped_with_stdin(cmd, input: &[u8], cap)` variant); `src/main.rs` (`mod`).

**Interfaces — Produces:**
- `pub fn repo_root(git: &Path, dir: &Path) -> Option<PathBuf>`.
- `#[derive(Clone, Copy, PartialEq, Eq, Debug)] pub enum Mark { Modified, Added, Deleted }`.
- `pub fn parse_porcelain(z: &[u8]) -> Vec<(PathBuf, Mark)>` (pure) and
  `pub fn status(git: &Path, root: &Path) -> HashMap<PathBuf, Mark>` (relative paths; parents marked by the caller).
- `pub fn ignored(git: &Path, root: &Path, rel: &[PathBuf]) -> HashSet<PathBuf>` (one `check-ignore --stdin` call).
- `#[derive(Clone, Copy, PartialEq, Eq, Debug)] pub enum LineChange { Added, Changed, DeletedAfter }` and
  `pub fn parse_hunks(diff: &str) -> Vec<(usize /*1-based new line*/, LineChange)>` (pure, from `-U0` hunk
  headers: `@@ -a,b +c,d @@`; b=0 → Added, d=0 → DeletedAfter at c, else Changed for min(b,d) and Added for the
  rest) and `pub fn line_changes(git: &Path, root: &Path, rel: &Path) -> Option<Vec<(usize, LineChange)>>` (None =
  no repo; untracked or no commits → all Added).
- `pub struct DirEntry { pub name: String, pub is_dir: bool }` and
  `pub fn list_dir(dir: &Path, cap: usize) -> (Vec<DirEntry>, usize /*more*/)` (folders first, then case-insensitive
  by name; skips `.git`).

- [x] **Step 1: failing tests:**
  - `parse_porcelain` (modified, untracked `??`, deleted, renamed → Added for the new path, a path with spaces);
  - `parse_hunks` (each header shape, including `@@ -3,0 +4,2 @@`, `@@ -5,2 +4,0 @@`, `@@ -7 +7 @@`);
  - on a temp repo made with the real `git` (skip if no git): `repo_root`, `status`, `ignored` (`target/` in
    `.gitignore`), `line_changes` (an edited line → Changed, a new file → all Added), and a repo with no commits;
  - a non-repo dir → `repo_root` None, `line_changes` None;
  - `list_dir` order, `.git` skipped, the cap with 12 entries and cap 10 → 10 + more 2.
- [x] **Step 2: run** → FAIL. **Step 3: implement.** **Step 4: run** `cargo test`, clippy (both), fmt.
- [x] **Step 5: commit** `feat(m11g): git status, line changes and folder listing (capped, off the UI thread)`.

### Task 3: `TabKind::File` in the model — ✅

**Files:** Modify `src/layout_model.rs` (`TabKind`, `SavedTab`, `to_saved`, the restore builder, `terminals`,
`shown_elsewhere`), and every match the compiler flags: `src/adopt.rs`, `src/tab_bar.rs` (`icon`, `bar_tabs`,
`close_tab`, `TabIcon` + a `File` glyph), `src/workspace.rs` (`sync`, `terminal_of`, `render_panes`'
view closure → a placeholder element until Task 6), `src/pane_tab_menu.rs`, `src/sidebar.rs`, `src/switcher.rs`,
`src/workspace_ssh.rs`; `src/restore.rs` tests.

**Interfaces — Produces:** `TabKind::File { path: PathBuf, mode: FileMode }`, `pub enum FileMode { Plain, Diff }`
(serde lowercase, default Plain); `Workspace::open_file_tab(path: PathBuf, window, cx)` (adds or selects in the
focused pane, saves, activates); `close_file_tab` in `close_tab`; `Workspace.file_tabs: HashMap<TabId, Entity<FileTab>>`
created by an `ensure_file_tabs` (like `ensure_browser_tabs`). The `FileTab` entity in this task is a stub showing the
path; Task 6 replaces its body.

- [x] **Step 1: failing tests:**
  - a model with a File tab round-trips through `SavedTab`;
  - an old JSON (`RECORDED_V1`) and a v2 file without File tabs still load;
  - a saved `{"kind":"file","path":"/x"}` (no mode) loads as Plain;
  - closing a File tab leaves the terminals alone;
  - `open_file_tab` twice for the same path in one pane gives one tab, selected.
- [x] **Step 2: run** → FAIL. **Step 3: implement.** **Step 4: run** `cargo test`, clippy (both), fmt, full
  `__ui-shots`.
- [x] **Step 5: commit** `feat(m11g): a file tab kind in panes (model, restore, close, drag)`.

### Task 4: The right sidebar shell — ✅

**Files:** Create `src/right_sidebar.rs`; Modify `src/workspace.rs` (state, render: a sibling after the pane column,
the `ToggleRightSidebar` handler, save/load), `src/restore.rs` (`SavedState` fields), `src/tab_bar.rs`
(`BarButton::RightSidebar`, its glyph, the lit state), `src/main.rs` (binding, View menu, action), `src/help_shortcuts.rs`,
`src/ui_shots.rs` (scene `right-sidebar`; fix the `tab-bars-button` hover maths for the extra icon).

**Interfaces — Produces:** `pub struct RightSidebarState { pub hidden: bool, pub width: f32, resizing, slide }`
with `load(saved)`/`clamp`; `Workspace::toggle_right_sidebar(window, cx)`; `Workspace::right_sidebar_folder(cx) ->
Option<(PathBuf, bool /*ssh*/)>`: the focused terminal's folder, resolved like `refresh_places`
(`working_folder()`, `process_cwd`, `project`), cached and refreshed on focus change and every 2 s while the sidebar
is visible; for a focused browser/file tab, the last terminal's. The panel body is a placeholder ("Files") until
Task 5.

- [x] **Step 1: failing tests:**
  - `SavedState` without the fields → hidden, 300 wide;
  - clamp 100 → 220, 900 → 560;
  - the keymap resolves `alt-cmd-\\` to `ToggleRightSidebar` from a terminal, a browser page and a file tab;
  - the View menu lists `Toggle Right Sidebar` after `Toggle Sidebar`;
  - `BarButton::ALL` ends with `RightSidebar`.
  - ui-shot `right-sidebar` (Mocha, Latte): closed (no sidebar, icon unlit); open (300 pt wide on the right, icon
    lit in every pane); a resize drag from x to x−80 → width +80; reduce motion → no slide; ⌘W/close paths
    untouched.
- [x] **Step 2: run** → FAIL. **Step 3: implement.** **Step 4: run** `cargo test`, clippy (both), fmt, full
  `__ui-shots`.
- [x] **Step 5: commit** `feat(m11g): the right sidebar — toggle, pane icons, resize, remembered`.

### Task 5: The Files panel — ✅

**Files:** Create `src/file_tree.rs` (the model); Modify `src/right_sidebar.rs` (rendering, keys, filter, refresh
ticker), `src/workspace.rs` (wiring `open_file_tab`), `src/main.rs` (bindings in a `FileTree` context),
`src/ui_shots.rs` (scene `files`).

**Interfaces — Produces:** `pub struct FileTree` with
`new(root, revealed: Option<PathBuf>)`, `expand(&mut self, rel, entries, more)`, `collapse`, `set_marks(HashMap)`,
`set_ignored`, `rows(&self, filter: &str) -> Vec<Row>` (`Row { rel, depth, is_dir, expanded, mark, more: Option<usize> }`),
`select_next/prev`, `toggle_selected`, and `expanded_set()` (for the per-root memory). Pure; the I/O stays in the
panel.

- [x] **Step 1: failing tests (pure `FileTree`):**
  - rows order and depth;
  - an unexpanded folder has no children listed;
  - marks propagate to parents;
  - the filter keeps matching files plus their ancestors;
  - the `… N more` row;
  - keyboard moves skip nothing and clamp at the ends;
  - collapse keeps the children for a fast re-expand;
  - re-rooting resets, the memory restores.
  - ui-shot `files` on a fixture folder (with a fake mark map, no real git): the tree with `M` / `A` / `D`
    colours, the filter narrowing it, a click opening a file tab, and Esc returning focus to the pane.
- [x] **Step 2: run** → FAIL. **Step 3: implement.** **Step 4: run** `cargo test`, clippy (both), fmt, full
  `__ui-shots`.
- [x] **Step 5: commit** `feat(m11g): the Files panel — repo tree, git marks, filter, keyboard`.

### Task 6: The file view — ✅

**Files:** Create/replace `src/file_tab.rs`; Modify `src/workspace.rs` (render the tab, focus), `src/main.rs`
(`FileTab` bindings: cmd-f, cmd-g, shift-cmd-g, cmd-c, ctrl-tab, ctrl-shift-tab, escape closes find),
`src/help_shortcuts.rs`, `src/ui_shots.rs` (scene `file-tab`). Extract the font metrics from `terminal_view.rs`'s
`CellMetrics` if sharing is cleaner (or duplicate them; the builder's call).

**Interfaces — Consumes:** Tasks 1 (`classify`, `Highlighter`, `highlight`), 2 (`line_changes`, `repo_root`), 3
(the entity slot). **Produces:** `FileTab::new(path, window, cx)`. Its states: `Loading`, `Text { lines, spans:
Option<..>, changes, version }`, `PlainTooLarge`, `Refuse`, `Missing`. A 1 s visible-only poll; a shared
`Highlighter` as a GPUI Global loaded once in the background; find state; selection.

- [x] **Step 1: failing tests:** pure helpers:
  - tab expansion (column mapping with tabs → 4 spaces);
  - the find (matches across lines, case-insensitive, wrap-around next/prev);
  - selection to text (a multi-line range, partial first/last lines);
  - stale-version drop (a highlight result for version 3 is ignored once version 4 is loaded).
  - ui-shot `file-tab` (Mocha, Latte):
    - a Rust fixture with colours and the gutter numbers;
    - change bars from a given change list (green, blue, red ▸) and the 12% tint;
    - ⌘F "fn" → "1 of N" highlighted;
    - a long line scrolled horizontally;
    - the Missing, Refuse and PlainTooLarge states;
    - a reload with new text keeps the scroll.
- [x] **Step 2: run** → FAIL. **Step 3: implement.** **Step 4: run** `cargo test`, clippy (both), fmt, full
  `__ui-shots`, and `scripts/bundle-mac.sh --app-only` (a release build: open a 5,000-line file in a ui-shot or a
  test and check that the highlighting finishes in a reasonable time; report the number).
- [x] **Step 5: commit** `feat(m11g): read-only file tabs — colours, line numbers, change bars, live reload, find, copy`.

### Task 7: Merge and push (your word) — ✅ (2026-11-08)

### Task 8: Release (your word) and on screen — 👀

- [x] On your word: version **0.4.0** (a new feature), commit, push, tag, and watch the Release run. (green)
- [ ] You: on-screen checklist (written into note 59 in Task 9).

### Task 9: Docs — ✅

- [x] Note 59 (M11g status, "As built", the on-screen checklist). This plan (ticks). The roadmap. The Plan Tracker.
  The Progress Log. `STRUCTURE.md` (the 5 modules). `README.md` (the right sidebar and file tabs).
- [x] Commit `docs(m11g): …`.
