---
tags: [development, milestone, design]
status: approved
---

# 59 - Milestones 11g–11h - Right Sidebar and File Tabs (design)

**Status:: ✅ Design approved by you 2026-11-08 ("lets go"). 🟨 M11g built and reviewed 2026-11-08 (953 tests), releasing as v0.4.0; on-screen check pending. 🟨 M11h built, reviewed, merged and pushed 2026-11-09 (1089 tests; every gap AI's call, below; not released on its own: one release after M11n): [[M11h Plan - Changes, Diff View, Activity]]; on-screen check pending. Ships in v0.5.0 (the release after M11n; pending).**  ·  **Phase 4: M11 extensions.**
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

## M11g as built (2026-11-08)

Where the build settled something the plan left open, or changed it (rulings made while building):

- **Open in Editor opens the file in macOS's default *text* editor** (`/usr/bin/open -t`), never its default app,
  so a script or executable (`gradlew`, `.command`, `.py` with the Python Launcher) can't run. In the "too large or
  binary" state the button is **Show in Finder** (`/usr/bin/open -R`). `$VISUAL`/`$EDITOR` aren't used.
- **When git is slow or fails** (a busy machine, the 2 s cap), the last good marks, hidden entries and change bars are
  kept, and the sidebar's refresh backs off 2 → 4 → … 30 s until git answers again. `check-ignore`'s "none ignored"
  answer counts as success. At the cap, git's whole process group is stopped.
- **The `… N more` row** has no "(open in Finder)" link.
- **Nested repos and submodules:** the folder itself is marked; its contents carry no marks (per-repo state inside
  them isn't in M11g).
- **Change bars are re-checked every 5 s** while a file tab is shown, so a commit clears them without the file
  changing; git look-ups for a tab run at most once every 2 s.
- **Syntax colours reach at least 3:1 contrast** against the background on every theme (Latte's comments were 1.9:1).
- **Find runs off the UI thread** and stops at 10,000 matches ("10,000+"). Very long lines are drawn only where
  they're on screen. Lines over 20,000 bytes show uncoloured.
- A file-only workspace is named after its file; ⌘P counts file views for "since". After a relaunch with a file tab
  focused, the sidebar follows the workspace's first terminal.
- Not built (not in the plan): keyboard scrolling, auto-scroll while selecting, double-click word selection,
  scroll width for wide (CJK/emoji) characters.

## M11g on-screen checklist (pending)

1. **⌥⌘\** opens and closes the right sidebar (from a terminal, a browser page and a file tab); View → Toggle Right
   Sidebar does too; it's remembered after a relaunch.
2. The **icon at the far right of every pane's tab bar** is lit while the sidebar is open; clicking it in another
   pane focuses that pane and toggles.
3. Drag the sidebar's left edge: it resizes between 220 and 560 pt; a narrow window never pushes it off-screen.
4. In a repo: the tree shows the repo with your terminal's folder opened; `target/`, `node_modules/` (ignored) are
   hidden; edited files show **M** (blue), new **A** (green), deleted **D** (red), and their folders too.
5. `cd` into another repo: the tree re-roots. An SSH terminal shows "Files aren't shown for SSH terminals."
6. Type in the filter box: the tree narrows. ↑ ↓ → ← Return work in the tree; Esc returns to the terminal.
7. Close the sidebar while the filter box has the keyboard: the terminal takes keys again at once.
8. Click a file: it opens as a tab in the pane, with **colours, line numbers and change bars** (green/blue/red ▸).
9. Let an agent edit that file: the tab updates within about a second, keeping its scroll; the bars follow.
10. ⌘F in a file tab: "n of m", ⌘G / ⇧⌘G step, Esc closes. Select text with the mouse, ⌘C, paste elsewhere.
11. **Open in Editor** opens the file in your text editor. Try it on an executable script (e.g. `gradlew`): it opens
    as text and **does not run**. A binary file shows **Show in Finder**.
12. Delete an open file: its tab says "This file no longer exists". Quit and reopen fuxx: file tabs come back.
13. Switch themes (light and dark): the file tab's colours follow and stay readable.
14. With the sidebar and a file tab open next to an agent streaming output, fuxx stays smooth (Activity Monitor).

## M11h settled before planning (AI's calls, 2026-11-08)

You skipped the design questions. Re-reading "Changes", "Activity" and "Diff view" as the builder who must plan
them, inside M11i's new look, these were the places they would have had to guess:

1. **The panel tabs.** The "Files" strip at the top of the right sidebar's card becomes a row of three text tabs,
   **Files · Changes · Activity**, drawn like M11i's pane tab bar: the 30 pt strip in `tab_bar`, tabs 4 pt from
   its top with 6 pt top corners, the active one filled with `card` (merging into the panel) in `text`, the others
   unfilled in `text_dim`, `tab_hover` on hover. No counts or badges on them. The strip's empty part still drags
   the window. The last panel shown is remembered in `sessions.json` (`right_sidebar_panel`: `files`, `changes`
   or `activity`; missing or unknown: Files), like the sidebar's open state and width.
2. **Only the panel on screen does work.** Files lists and marks only while Files shows; Changes asks git only while
   Changes shows; Activity asks nothing. Switching panels brings the new one up to date at once. Leaving Files
   while its filter box or tree has the keyboard gives the keyboard to the focused tab (as closing the sidebar
   does).
3. **Changes and Activity don't take the keyboard** (no new key bindings): they are clicked. A click on a Changes
   row opens the diff, and that tab takes the keyboard. Files keeps its M11g keys.
4. **What Changes lists.** Every file that differs from `HEAD`, staged or not, from one
   `git status --porcelain=v1 -z --untracked-files=all --no-renames` and one
   `git diff HEAD --numstat -z --no-renames`. Untracked files are listed one by one (not as a folder). A rename
   shows as the old path **D** and the new one **A**. A conflict is **M**. In a repo with no commits yet, every file
   is **A**. Sorted by path, case-insensitive. At most **1,000** rows, then a `… N more` row (as the Files tree).
5. **Line counts.** From `--numstat`. An untracked file (and every file before the first commit) is counted by
   reading it: only a plain text file of at most 2 MB, and at most 200 such files per refresh; others show no count.
   A binary file shows `binary`. Counts show as `+12 −3` (a zero side left out), in the theme's green and red,
   pushed to 4.5:1 (they are text) on the card, its hover and its selection.
6. **A Changes row (22 pt):** the mark letter in its Files colour, the file's name (13 pt, `text`), its folder
   (11 pt, `text_faint`, cut at its end; the whole path in the tooltip), and the counts at the right. The row of the
   file whose diff tab is focused is drawn selected (`card_selected`). **Header** (40 pt, the Files root header's
   style): the repo's name (13 pt semibold) with `N files` at the right (11 pt, `text_faint`), and under it the
   totals `+120 −34`. Empty states: "No changes since the last commit.", "Not a git repository.", and for an SSH
   terminal "Changes aren't shown for SSH terminals." The root is found as the Files tree's (the repo's top
   folder). Refreshed every 2 s while shown; a failed or slow git keeps the last list and backs off 2 → 30 s, as
   Files does.
7. **One tab per file per pane, whatever its mode.** Opening a file that already has a tab in the pane selects that
   tab and sets its mode: from Files, **Plain**; from Changes, **Diff**. The toggle changes the tab's mode in place.
   The mode is saved with the tab (`TabKind::File { path, mode }`, already in M11g's save format).
8. **Where the diff comes from.** git's whole-file diff against `HEAD` with every line as context
   (`git diff HEAD -U<lines on disk + 1>`), parsed into rows: unchanged, removed, added. An untracked file, or any
   file before the first commit, is all added. A deleted file shows its old lines, all removed (its tab is not
   "This file no longer exists" while git has its old lines). Notes instead of rows: an unchanged file, "No
   changes since the last commit."; a binary one, "This file is binary: no diff to show."; outside a repo, "Not a
   git repository."; over 20 MB, M11g's "too large or binary". Over 2 MB, the diff is shown uncoloured.
9. **How the diff looks.** One column. The gutter: the old line number, the new line number (both `text_faint`,
   right-aligned, 8 pt apart), then a sign column one character wide (`−` in the change bars' red, `+` in their
   green), then the text. Removed rows are tinted red and added rows green at the change bars' 12 %. Unchanged
   rows are **dimmed**: each colour mixed 35 % toward the background, but never below 4.5:1 (or its own contrast, if
   that was lower; never below 3:1). Syntax colours stay on: the old and the new side are each coloured whole, and
   each row takes its side's colours.
10. **The toggle.** Two joined buttons in the file tab's toolbar, **File | Diff**, before Open in Editor; the current
    one filled with `card_selected` in `text`, the other in `text_dim`. Shown for a file inside a repo, and always
    while the tab is in Diff mode (so it can go back). Tooltips: "Show the file" and "Show the changes since the
    last commit".
11. **From a change bar.** Clicking a change bar (or a deleted-lines ▸) in a plain file tab switches it to Diff,
    showing that change. A changed line's change starts at its first removed row.
12. **Scroll, find, copy.** The diff opens with its first change 3 rows from the top. Each mode keeps its own scroll
    while the tab is open. ⌘F searches the rows' text; ⌘C copies it without the signs or numbers.
13. **Live diff.** As the change bars: looked up when the file changes on disk, every 5 s while shown, at most once
    every 2 s; a failed or slow git keeps the rows shown.
14. **What Activity records.** Every status change (Idle too) with its time and how long the status before it
    lasted; a Waiting entry keeps its question. Each `fuxx note` (a blank note, a cleared one, or the same note again
    isn't a new entry). Each `fuxx progress` (cleared progress isn't an entry; updates in a row are **one** entry,
    the latest, so a script reporting ten times a second fills nothing). Notes put back after a relaunch aren't
    entries. The last **200** per terminal, in memory, for the run; a terminal's log goes when it closes. Not shown
    to scripts.
15. **"Done 14:20 — 11 min".** The minutes are how long the status before Done lasted, which is what note 59's
    example adds up to (Waiting 14:09 → Done 14:20). After Working it is how long it worked. Durations read
    `45 s`, `11 min`, `1 h 5 min`, `2 h`. Times are this Mac's local 24-hour `HH:MM`.
16. **The Activity panel.** Header (40 pt, the Files root header's style): the terminal's name (13 pt semibold),
    and under it the status bar's words (`Working · 4m`, `Needs input · 2m` in `ring_waiting`, …). While it waits,
    a card under the header (`waiting_tint`, 6 pt corners, 8 pt padding, 6 pt from the sides): `Needs input` (11 pt,
    `ring_waiting`) and the whole question, wrapped. Then the timeline, **newest first**, 22 pt rows: the time
    (11 pt, `text_faint`), the status's still mark (10 pt; a 4 pt dot for a note or progress), and the words
    (`Working`, `Needs input`, `Done — 11 min`, `Idle`, the note, `Progress 42% · build`); a long row is cut, its
    whole text (and a Waiting entry's question) in the tooltip. Empty states: "No activity yet." and "No terminal
    to follow." Nothing in it animates.
17. **Nothing new besides.** No new crates, settings, key bindings or menu items; the scripting API is unchanged.
    Released in the one release after M11n (your rule), not on its own.

## M11h as built (2026-11-09)

Where the build settled something the plan left open, or changed it (AI's rulings after each task's review and the
final whole-branch review):

- **Diffs read any text.** git's diff output is decoded leniently, so a Latin-1 or Shift-JIS file shows its diff
  (bad bytes as `�`) instead of waiting forever. `--no-textconv` keeps the rows equal to the file on disk, and a
  blank context line that `diff.suppressBlankEmpty` prints without its space still counts.
- **Size limits judge the diff, not only the disk file:** colours only when both the old and new sides are at most
  2 MB; the old version's size is asked first (`git cat-file -s`, capped) and over 20 MB the diff is refused without
  asking git for it; a diff output over 20 MB is refused too.
- **Deleted files show their old lines** whether deleted on disk, with `git rm`, as a `git mv`'s old path, or with
  their whole folder gone (the nearest existing folder finds the repo). `git rm --cached` with the file still on
  disk counts as untracked (all added).
- **A slow or failing git is not "Not a git repository."**: the tab, Files and Changes stay as they are and ask
  again (with back-off); only git's own "not a repo" answer shows that note.
- **An open Diff tab doesn't re-colour** an unchanged diff every 5 s (it did: up to 2 s of work per side near
  2 MB).
- **The `+`/`−` signs reach 3:1 on their own tinted rows** on every theme (Latte's `+` was 2.68).
- **A binary file** in Diff mode says "This file is binary: no diff to show." Known: a file refused for another
  reason at 20 MB or less (no permission to read it, a failed read) says the same.
- **Changes:** a renamed file is **D** (old path) and **A** (new path); a nested repo or submodule row is shown but
  doesn't open a tab; a failed line count still shows the list but waits longer before asking again; past 1,000
  rows the header's totals cover the listed rows. `git init` or deleting `.git` in the followed folder shows only
  after a `cd`.
- **Activity:** progress updates in a row are one entry; a note the same as the last note is skipped; an SSH
  reconnect records the change to Idle; an adopted session's first "lasted" counts from when fuxx adopted it. The
  still marks are drawn at the same moment of their animation with motion on and off. Only the rows in sight are
  built.
- Known and parked: a tab first looked up while its own repo's folder is gone can be bound to an enclosing repo
  (its Diff shows "This file no longer exists"; reopen the tab after restoring the repo). The `title-area`
  screenshot check flakes now and then (2 pt), unrelated to M11h.

## M11h on-screen checklist (pending, after the combined release)

1. The right sidebar's top shows three tabs, **Files · Changes · Activity**; the active one merges into the panel.
   Switch to Changes, quit and reopen fuxx: Changes is still shown.
2. At the narrowest sidebar (220 pt) all three tabs fit.
3. In a repo with edits: Changes lists each changed file with **M**/**A**/**D**, its folder, and `+N −M`; a new
   untracked file is **A** with its line count; a binary file says `binary`. The header shows the repo's name,
   `N files` and the totals.
4. Commit everything: Changes says "No changes since the last commit." within a few seconds. `cd` out of any
   repo: "Not a git repository." An SSH terminal: "Changes aren't shown for SSH terminals."
5. Let an agent edit files: new entries appear within about 2 s, without the window stuttering.
6. Click a Changes row: the file opens as a tab in **diff mode**: removed lines red with `−`, added lines green with
   `+`, unchanged lines dimmer, two line-number columns, syntax colours on, the first change near the top. The
   row stays highlighted while that tab is focused.
7. The toolbar's **File | Diff** switches the same tab between the file and the diff; quit and reopen: the tab
   comes back in the mode you left it.
8. In a plain file tab, click a change bar: it switches to the diff at that change.
9. A deleted file's row opens a diff of its old lines, all removed. An unchanged file in Diff mode says "No
   changes since the last commit."
10. ⌘F in a diff finds text in it; select some lines and ⌘C: the paste has no `+`/`−` signs or numbers.
11. Activity shows the focused terminal's timeline, newest first: `Working 14:02`, `Needs input 14:09`, `Done — 11
    min 14:20`; switching terminals switches it.
12. While an agent waits, Activity shows its whole question in a card under the header.
13. `fuxx note "migrating"` and `fuxx progress 40 --label build` appear as entries; a loop of `fuxx progress` updates
    stays one entry.
14. Light and dark themes (Latte, Mocha and one other of each): tabs, counts, diff tints and dimmed lines read
    clearly; Reduce motion on: nothing in the panels moves.

## Left to the builder

The exact layout spacing (following `ui_tokens`), how the tree is virtualised for long lists, how syntect scopes
map onto the theme's palette in detail, the polling mechanics, and the internal module split (suggested:
`right_sidebar.rs`, `file_tree.rs`, `file_tab.rs`, `highlight.rs`, `git_status.rs`).
