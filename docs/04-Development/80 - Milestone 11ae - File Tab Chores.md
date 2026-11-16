# Milestone 11ae — File Tab Chores

**Status:** 🟨 built 2026-11-16 on branch `m11ae-file-tab` (from master `8d2b16f`), reviewed (whole-branch review and fixes clean), **merged to master 2026-11-16, not pushed**, waiting for
your on-screen check (below). Plan [[M11ae Plan - File Tab Chores]], 6 / 6 tasks, each reviewed. At the branch tip:
1681 tests pass, clippy 30 / 32 (= the baselines), `cargo fmt --check` clean, full `__ui-shots` 207 scenes ok in 8 min 21 s
(after the whole-branch review's fix wave: see "Calls made" 7).

**Asked for:** 2026-11-16, by you: "let's do some small chores. Let's fix file preview. right now no scrollbar. and
when I try hold cursor left click and go below, it doesn't scroll + keep blocking the row/character. we need to adjust
the UI on the file file path, let's see your best UI options for me to choose/". Chosen in browser mockups the same day:
**"B with overlay scrollbar"**; the design below approved with **"yes, go ahead"**. Done while M11aa's UI tasks wait
(after its Task 8 fix round).

## Goal

1. A file tab (File, Diff and Preview) has a scrollbar.
2. Drag-selecting past the text's edge scrolls the text, and the selection follows.
3. The path bar reads as a title (option B): the file's name and its change, then where it is and what it is.

## The design

### 1. Path bar — option B, "title and details"

Two lines, left; the controls right, vertically centred. The bar is **42 pt** tall (was 34; the find bar stays 34).

- **First line:** the file name, semibold, in `text`. In a repo, when the file differs from the last commit, a chip
  after it: **`M · +3 −1`** (tracked and changed) or **`A · +12`** (untracked or no commits yet: every line added;
  no `−` part when nothing was removed). No chip for an unchanged file, outside a repo, or before git answered.
  - `+` is the lines added and `−` the lines removed, as `git diff --numstat` counts them: from the `-U0` diff fuxx
    already runs for the change bars (each hunk `@@ -a,b +c,d @@` adds `d` and `b`). No extra git call.
- **Second line**, smaller, in `text_dim`, parts joined by ` · `:
  - **Where:** in a repo, the repo folder's name then the file's folders, joined by ` / `
    (`fuxx-cli / src / ui_shots`; a file at the root: just the repo's name). Outside a repo, its
    folder with your home as `~` (`~/Downloads`).
  - **Type:** the syntax's name (`Markdown`, `Rust`, `JSON`), as the colouring finds it (extension, file name, first
    line); `Plain Text` when none. Left out until the syntaxes are loaded, and for a file too large to colour.
  - **Lines:** `1 line`, `42 lines`, `12,345 lines`. Left out for a file with no text shown (missing, refused).
  - **Edited:** from the file's modified time: `edited just now` (under a minute), `edited 5 min ago`,
    `edited 3 h ago`, `edited 2 d ago` (under 30 days), `edited 4 mo ago` (under a year), `edited 2 y ago`. A time in
    the future reads `edited just now`. Left out when the disk gave no time.
  - When the pane is narrow, **Where** is cut first (ending in `…`); the other parts stay whole.
- **The age stays current with no new timer:** it is worked out in the tab's existing 1 s disk poll (only while the
  tab is shown), kept on the tab, and the tab redraws only when the words change. Nothing visible changes inside
  `render` (M11p's rule).
- **Controls**, right to left: Open in Editor, Wrap, the view switch.
  - **One view switch:** `File | Diff | Preview` joined segments, as today's File | Diff toggle. **Diff** only where
    today's toggle shows (in a repo, or in Diff mode). **Preview** only for a `.md` / `.markdown` file, in either mode
    (from Diff, Preview shows the rendered file). Over the Preview limit, Preview is drawn faint, does nothing and its
    tooltip says "Too large to preview" (as today). The switch shows only when it has at least two segments.
  - The current segment: Diff in Diff mode; Preview in File mode with Preview on (and fitting); else File. File turns
    Preview off; Preview turns it on (and File mode). What is saved per tab doesn't change (`mode`, `preview`).
  - **Wrap** becomes an icon button (a new `Wrap` glyph: three lines, the last ending in a return arrow), filled when
    on, as today; tooltip as today.
  - **Open in Editor** becomes an icon button (a new `OpenExternal` glyph: a box with an arrow leaving its top right);
    tooltip "Open in Editor". A refused file keeps its middle "Show in Finder" button and gets no toolbar icon (as
    today).
  - The file glyph at the bar's left goes (the name is the title now).

### 2. Overlay scrollbar (File, Diff and Preview)

- A **thumb** only, no track: 6 pt wide, 3 pt corners, 3 pt in from the text's right edge, at least 24 pt long, in
  `text` at 35 % opacity (55 % while hovered or dragged). Its length is the visible share of the text; its place, the
  scroll.
- **Shown** while the text scrolls (wheel, keys, find, a reveal, the auto-scroll below) and while the pointer is within
  12 pt of the text's right edge; **hidden** 1.0 s after the last of those, with a 200 ms fade. Nothing is drawn when
  everything fits. A hidden scrollbar costs nothing: the fade's frames run only after activity and stop at 0.
- **Drag** the thumb: the text follows, the grab point kept under the pointer. **Click** in the 12 pt edge zone off
  the thumb: the thumb jumps to centre on the click (then can be dragged on). A press in the zone never starts a
  selection.
- **Sideways:** with Wrap off and a line wider than the text, a matching thumb along the bottom (6 pt tall, 3 pt up
  from the bottom edge; zone 12 pt tall), same rules.
- **Preview:** the same vertical thumb, over the rendered list (GPUI's `ListState` scrollbar helpers).

### 3. Drag-select past the edge

- Today the selection only moves when the mouse moves, and past the bottom it grows into lines you can't see: the
  view never follows.
- Now, while selecting with the pointer **above or below the text** (or, with Wrap off, **left or right** of it), the
  text scrolls on its own every 16 ms, **faster the further out**: `(distance / 8).clamp(1, 3 × line height)` points
  per step. Each step the selection's moving end goes to the place under the pointer **held inside the text** (the
  edge row and column), so it grows with the scroll.
- It stops on release, when the pointer comes back inside, or at the end of the text. The scrollbar shows while it
  runs. Diff mode the same; Preview has no selection (unchanged).

## What was built

All three parts of the design, as written above, in `src/file_tab.rs`, with the scrollbar's geometry, fade state and
painting in a new `src/scrollbar.rs` (shared by the text canvas and the Preview list). No new crates. Nothing visible
changes inside `render`, except the one case under "Calls made" (3).

1. **Header facts (Task 1).** Pure helpers, unit-tested: the change counts from the existing `-U0` diff (no extra git
   call), the chip words, the Where, Type, Lines and Edited words. The age is worked out in the tab's 1 s disk poll and
   the tab redraws only when the words change. The counts and the Type carry over a reload, so the chip doesn't flash.
   In Diff mode the chip's counts come from the diff itself (its added and removed rows, exact as numstat; `A` when
   git says the whole file is new), so a tab opened in Diff (a Changes-panel row, a restored tab) has its chip at
   once, it follows each edit, and it goes when the file is reverted.
2. **The two-line bar (Task 2).** 42 pt, name and chip on line one, details on line two, one File | Diff | Preview
   switch, Wrap and Open in Editor as icon buttons (two new glyphs), the file glyph gone.
3. **The overlay scrollbar in File and Diff (Task 3).** Thumb, hover zone, fade after 1.0 s, drag, click-to-jump, and the
   bottom thumb for Wrap off. The fade's frames run only after activity and stop at 0.
4. **The scrollbar in Preview (Task 4).** The same thumb over the rendered list (see "Calls made" 4).
5. **Drag-select past the edge (Task 5).** The 16 ms auto-scroll, faster the further out, the selection's moving end
   held at the edge row and column, in File and Diff; it stops on release, on coming back inside, or at the end.

## Calls made while building

Where the plan left a gap, or a task found something while building, these are the calls (all reviewed):

1. **Preview segment.** It shows only for a Markdown file that has text. A missing or refused `.md` gets no Preview
   segment.
2. **Narrow panes.** Where the fixed parts don't fit, Where shrinks to `…` (it never vanishes). Below that width, the
   meta line is clipped at the controls.
3. **Opening a diff.** It shows the scrollbar in the same frame it scrolls to the first change. This is an accepted
   M11p exception: that scroll was already set while drawing, and note 77's rules allow starting a timer there.
4. **The Preview scrollbar has two modes, chosen by block count.**
   - Up to 2,000 blocks, GPUI lays out every block, so the thumb is exact.
   - Above that, the thumb is an estimate (45 pt a block). Dragging to the end still lands on the true end.
   - Measured on a release build: a ~1 MB file (29,115 blocks) relaid out in 160 ms warm and 1.24 s cold; a ~20 KB file
     (585 blocks) in 3 ms warm. Hence the 2,000 threshold, about 11 ms warm.
5. **The horizontal auto-scroll edge** starts where the text starts, not at the line numbers.
6. **Mode switches.** Switching to Preview mid-drag ends the drag. Switching modes also stops the auto-scroll.
7. **Final review fixes.** A tab going off screen mid-drag (its release can't reach it) ends the selection's drag, a
   thumb's drag and the hover, so its auto-scroll stops at the next step. The pointer leaving the window from the
   edge zone lets the thumb fade (GPUI sends a `MouseExitEvent`, not a last move). A tab back on screen with an
   unchanged file updates its "edited …" at once.

## Not in it

Editing; a minimap; a scrollbar in terminal panes or the browser; making the path segments clickable; copying the path
from the bar (the tab's menu has it); a track that is always visible; selection in Preview.

## On-screen check (yours, after the build)

1. The bar: name and `M · +n −n` on a changed file in a repo; `A · +n` on a new one; no chip on a clean file and
   outside a repo. The same in Diff mode: open a changed file from the Changes panel (it opens in Diff), and the chip
   is there; edit the file and it follows; revert it and the chip goes.
2. The second line: folders from the repo's name; `~/…` outside a repo; the type; the line count; "edited …" changing
   from "just now" to "1 min ago" while you watch.
3. A narrow split pane: the folders are cut with `…`, the rest stays whole, the controls stay visible. Narrower still,
   the second line is clipped where the controls start (the `…` stays).
4. The switch: File / Diff / Preview on a Markdown file in a repo; File / Diff on a Rust file; no switch on a file
   outside a repo that isn't Markdown (a missing or refused `.md` shows no Preview segment). Wrap and Open in Editor as icons with tooltips.
5. Scroll a long file: the thumb appears, then fades about a second after you stop. Hover the right edge: it shows.
   Drag it: the file follows. Click above or below it: it jumps there.
6. Wrap off on a file with a very long line: the bottom thumb works the same.
7. Preview of a long Markdown file: the thumb works (exact for a normal file; on a huge one the thumb's size is an
   estimate, but dragging it to the end reaches the real end).
8. Press in the text and drag below the pane, then hold still: the text keeps scrolling and the highlight follows;
   further out is faster; release stops it. Above, the same upwards. Wrap off: left and right too (the left edge is where
   the text starts, so dragging over the line numbers scrolls left). Switching mode mid-drag stops it.
9. With the window idle and no scrolling, Activity Monitor shows fuxx's CPU as before.

## Related

[[40 - Roadmap]] · [[12 - Scope]] · [[78 - Milestone 11ac - File Tab Polish]] ·
[[59 - Milestones 11g-11h - Right Sidebar and File Tabs]] · [[77 - Milestone 11p - Chrome Caching]]
