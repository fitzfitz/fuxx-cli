---
tags: [development, milestone, design]
status: approved
---

# 59 - Milestones 11g–11h - Right Sidebar and File Tabs (design)

**Status:: ✅ Design approved by you 2026-11-08 ("lets go"); Scope updated. M11g's plan next, then M11h's.**  ·  **Phase 4: M11 extensions.**
**Changes the scope** (your decision, 2026-11-08): [[12 - Scope]] says "Being a general IDE / editor" is out. This
design brings in a **read-only file viewer**, but **not an editor**. If you approve, Scope is updated before
planning.

## Goal

A **right sidebar** that shows what is going on in the terminal you're looking at: its files, what has changed since
the last commit, and the agent's activity. Clicking a file opens it **in a pane, as a tab, like an IDE**. The tab is
read-only, with syntax colours, line numbers and coloured change bars, and it updates live while the agent edits.

## Why (your words, 2026-11-08)

- "Let's think about a right sidebar." Asked what cmux and Warp do: cmux has Files / Sessions / Feed on its right,
  and Warp builds its right side around reviewing the agent's changes (diffs, line comments sent to the agent).
- "I want the file explorer with additional functionality to view the file in the middle like an IDE does."
- You chose **syntect + two-face** for highlighting ("i'd go with your recommendation").
- You asked for **line numbers and coloured lines on changes**.
- You asked for **an icon on the right of each pane** to show or hide the right sidebar.
- "I like those ideas" for: one right sidebar per window, the Activity panel, and two milestones. Those three were
  AI's suggestions, which you accepted.
- Everything else below is AI's call. It is marked where it matters; correct anything.

## Goals, non-goals, anti-goals

- **Goals:** see the terminal's files and what changed without leaving fuxx; read code comfortably (colours, line
  numbers, search); review an agent's changes (change bars, diffs); follow one agent's activity.
- **Non-goals:** editing, autocomplete, language servers, go-to-definition, debugging, refactoring, AI summaries, a
  file manager (no rename, delete or move), browsing folders outside the terminal's own folder.
- **Anti-goals (a failure even if it works):**
  - the terminal gets slower or the window stutters on a big repo;
  - fuxx starts to feel like a worse VS Code;
  - anything changes files on disk;
  - a git command blocks the UI.

## The right sidebar

- **One per window** (your accepted choice). It follows the **focused terminal**: its folder (what fuxx already knows
  per terminal; see `git_branch.rs`), and that folder's git repo if it has one. Switching tab, pane or workspace
  switches it. With a browser or file tab focused, it keeps the last terminal it showed.
- **Open and close:**
  - **⌥⌘\\** (AI's call; free today, the mirror of ⌘\\ for the left);
  - **View → Toggle Right Sidebar**;
  - **a right-sidebar icon at the far right of every pane's tab bar** (your idea). It is a line icon (a box with its
    right side filled), drawn like the other tab-bar icons, and highlighted while the sidebar is open. Clicking it
    in a pane focuses that pane and opens or closes the sidebar.
- Resizable from its left edge like the left sidebar (min 220, max 560, default 300 pt; AI's call). Open state and
  width are remembered in `sessions.json`, like the left sidebar's (`right_sidebar_hidden`, `right_sidebar_width`;
  corrected while planning: the left sidebar's state lives there, not in the settings file).
- **Panels**, chosen with a small tab row at its top: **Files**, **Changes**, **Activity**. The last one shown is
  remembered.

### Files (M11g)
- The tree of the terminal's folder, folders first, then files, sorted by name. Folders expand on click.
- **Hidden:** whatever git ignores (`.gitignore`, via `git ls-files`/`git check-ignore`), plus `.git`. Outside a git
  repo: everything except dot-folders. A folder lists at most **10,000** entries; past that its last row reads
  "… N more (open in Finder)" (AI's call), so a huge folder (e.g. `node_modules` outside git) can't stall the tree.
- **Git markers** beside changed files and their parent folders: `M` modified, `A` added (new, untracked), `D`
  deleted, in the theme's change colours.
- A filter box at the top (by name, as you type).
- Clicking a file opens it in a **file tab** (below) in the focused pane. If that file already has a tab in that pane,
  it is selected instead.
- Refreshes when the sidebar is shown, when the focused terminal changes, and every 2 s while it's visible (AI's
  call), always off the UI thread.

### Changes (M11h)
- The files that differ from the **last commit** (your accepted choice: like VS Code, Zed and `git diff`), from
  `git status --porcelain` and `git diff --numstat`: path, status letter, and +added / −removed line counts.
- A click opens that file's **diff view**.
- Empty state: "No changes since the last commit." Outside a repo: "Not a git repository."
- "Since this agent started" is **not** in this design (noted as a later option).

### Activity (M11h)
- The focused terminal's timeline: each status change with its time (Working 14:02, Waiting 14:09, Done 14:20 —
  11 min).
- The current question in full, while it is waiting.
- The agent's own `fuxx note` lines and `fuxx progress`, which exist today.
- Kept for the run only (not saved).

## File tabs (M11g)

- A **new kind of pane tab**, next to terminal and browser tabs (`TabKind::File { path }` in `layout_model.rs`).
  Its title is the file name with a file icon, and the full path in the tooltip. It's dragged, split, closed (⌘W) and
  restored like the others.
- **Read-only.** Nothing in fuxx writes to the file. An **"Open in Editor"** button in the tab's toolbar opens it with
  macOS's default app for that file (`open`, AI's call), or with `$VISUAL`/`$EDITOR` when that is a GUI editor
  command fuxx can run (the builder's call).
- **Line numbers** in a gutter on the left (dim theme text, right-aligned).
- **Syntax colours** with **syntect** (pure-Rust mode, `default-fancy`) plus **two-face**'s grammars. The colours are
  **taken from the fuxx theme's palette** (AI's call), so code matches the terminal: keywords, strings, comments,
  numbers, types and functions map onto the theme's ANSI colours.
- **Change bars** (your request): a 3 pt bar in the gutter beside each line that differs from the last commit, green
  for added, blue for changed, and a small red marker between lines where lines were deleted. The changed line also
  gets a faint tint of the same colour. They come from `git diff` on that file.
- **Live:** the tab notices when the file changes on disk (polled every 1 s while the tab is visible; AI's call, no
  file-watcher dependency) and reloads, keeping the scroll position.
- **Search:** ⌘F opens a find bar (next/previous, match count, highlighted matches). ⌘G and ⇧⌘G step.
- **Selection and copy:** select text with the mouse and ⌘C (read-only doesn't mean un-copyable).
- **Limits** (AI's calls):
  - Files over **2 MB** open as plain text without colours, with a note.
  - Files over **20 MB** or binary files (a NUL byte in the first 8 KB) show "This file is too large / binary to
    show" and the Open in Editor button.
  - Highlighting runs off the UI thread; the plain text shows first.

### Diff view (M11h)
- Opened from Changes (or from a change bar). It's a file tab in **diff mode**: the old and new lines in one column,
  removed lines red-tinted with a `−`, added lines green-tinted with a `+`, unchanged lines dim. Both line numbers
  show in the gutter, and syntax colours stay on.
- A toggle in its toolbar switches between the diff and the plain file.

## How it gets made (constraints)

- **New dependencies (your choice):** `syntect` (5.x, `default-features = false`, features `default-fancy` plus what
  loading the two-face sets needs) and `two-face` (0.5.x). Versions are verified with Context7 or crates.io at
  planning time. No other new crates: no file watcher, no git library. Git runs as `git` the way `git_branch.rs`
  already does (off the UI thread, time-capped, cached per folder).
- GPUI at the pinned rev, drawn like the existing sidebar, tab bar and terminal (the same tokens, themes and
  ui-shots).
- **Nothing blocks the UI thread:** listing folders, git, reading files and highlighting are all background work.
- Respects **Animations / Reduce motion** (no sliding when off).

## Split into two milestones (your accepted choice)

- **M11g — Right sidebar, Files, file tabs:** the sidebar (open/close, the pane icons, resizing, remembering), the
  Files panel with git markers and filter, file tabs (read-only, colours, line numbers, change bars, live reload,
  search, copy, limits, Open in Editor).
- **M11h — Changes, diff view, Activity:** the Changes panel, the diff mode of file tabs, the Activity panel.
- Each gets its own plan, review and release, like M11e/M11f.

## Settled after a builder's read (AI's calls, 2026-11-08)

1. **Activity's data.** Today fuxx keeps only each terminal's *current* status, note and progress, plus the run's 🔔
   list. M11h adds a small **in-memory log per terminal**, with entries recorded as they happen (status changes from
   the detector, each `fuxx note` / `fuxx progress` update). It keeps the last 200 entries, for the run only. That
   is a real task in M11h's plan, not free.
2. **Restoring file tabs.** `TabKind::File { path, mode }`, where `mode` is `Plain` or `Diff`; the scroll position
   isn't saved. A file that's gone at restore opens as a tab saying "This file no longer exists" with its path and
   a Close button. It isn't dropped silently.
3. **The tree's root.** Inside a git repo, it's the **repo's top folder** (`git rev-parse --show-toplevel`), with the
   terminal's current folder expanded and revealed. Outside a repo, it's the terminal's current folder. A `cd`
   inside the same repo keeps the tree as it is; a `cd` into another repo or folder re-roots it. Expanded folders are
   remembered **per root** for the run.
4. **Ignored files.** Folders are listed **lazily, one folder at a time when expanded**. Each listing is checked in
   one batch with `git check-ignore --stdin`, so a big ignored folder like `target/` or `node_modules` is never
   walked. Ignored entries are **hidden, with no toggle** in M11g. Nested repos and submodules show as folders,
   marked by their own state only when expanded.
5. **What change bars compare.** The file on disk against **`HEAD`** (`git diff HEAD -- <file>`), so staged and
   unstaged changes look the same. An untracked file is all green. A repo with no commits yet is all green. A file
   outside a repo has no bars.
6. **Theme colours.** A syntect `Theme` is **built at runtime from the fuxx theme** (not a bundled theme recoloured),
   and rebuilt and re-applied to open tabs when the theme changes. It must read well on light themes too; the
   ui-shots cover a light and a dark theme.
7. **The file view.** A **new component**, not the terminal view: GPUI's text system with a virtualised list of
   lines. Long lines **scroll horizontally** (no wrapping). Tabs show as 4 spaces. Wide characters are measured by
   the text system. Selection and ⌘C are its own (simple, line-based).
8. **Keys and focus.** ⌘F, ⌘G and ⇧⌘G live in the file tab's own key context, so terminals and browser tabs keep
   theirs. The tree is **keyboard-navigable** when it has focus (↑ ↓ to move, → ← to expand and collapse, Return to
   open, Esc back to the pane). Clicking anywhere in the sidebar gives it focus; Esc or clicking a pane gives focus
   back.

## Left to the builder

The exact layout spacing (following `ui_tokens`), how the tree is virtualised for long lists, how syntect scopes
map onto the theme's palette in detail, the polling mechanics, and the internal module split (suggested:
`right_sidebar.rs`, `file_tree.rs`, `file_tab.rs`, `highlight.rs`, `git_status.rs`).
