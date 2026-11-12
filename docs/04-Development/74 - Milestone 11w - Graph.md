# Milestone 11w — Graph

**Status:** 🟨 built 2026-11-12, committed (`7d94c1a`) and released in **v0.9.0** (run 43177618415) on your word ("commit, push and release"); waiting for your on-screen check (below). Plan [[M11w Plan - Graph]].
**Asked for:** 2026-11-12, by you: "If everything good, I want to add more accordion below changes called "Graph"" →
"see the commit history of the branch with lanes" → (mockups round 8, then your screenshot of Cursor's / VS Code's
Source Control Graph) "I want something like this like in cursor/IDE. do we need any library for this?" → (no
library; proposal) "commit and push, then start M11w".

## What it is

A third accordion in the Changes tab, **GRAPH**, under STAGED and CHANGES: the repo's commit history with coloured
lanes, as Cursor / VS Code show it.

## What it does

1. **The header** (as the other groups: a thin chevron, `GRAPH` in small caps, a hairline; a click opens or closes
   it) with icon buttons at the right: **Auto** (a branch glyph and the word: which commits are drawn, below), **Go
   to the current commit** (◎: scrolls to and marks HEAD's row), **Fetch**, **Pull**, **Push** (the branch bar's
   own actions, same rules) and **Refresh**.
2. **The rows**, newest first, one line each (28 pt), on the panel's 12 pt edge:
   - **Lanes** at the left: one column per open line of history, each in its own colour from the theme (its ANSI
     colours, cycled), straight down, with a curve where a branch forks or a merge comes in. A commit is a dot on its
     lane: filled; a merge commit a ringed dot; **HEAD** (where you are) a larger hollow ring.
   - **The subject** (the first line of the message), cut at its end; bold for HEAD's row.
   - **Branch pills** after the subject, at the right: your branch with a ◎ (filled with its lane's colour), an
     origin branch with a cloud, a tag with a tag glyph; at most two shown, then `+N`.
   - **The author**, faint, after the subject when there's room.
3. **The hover card** (after the pointer rests 0.4 s on a row), to the left of the panel: the author and how long
   ago with the full date (`Andri Ferinata, 1 hour ago (November 12, 2026 at 10:52 AM)`), the whole message, the
   files it changed (`9 files changed, 774 insertions(+), 65 deletions(-)`), the short hash (a click copies the
   whole one) and **Open on GitHub** (when origin is a GitHub remote; opens the commit page in fuxx's browser tab).
4. **Which commits — Auto:** the current branch, its upstream, and the branch they forked from (origin's default
   branch); the button's menu also offers **All branches** and **Current branch only**. 200 commits at first, then a
   "Load more" row.
5. **When it reads:** when GRAPH is opened, when HEAD or a branch moves (the Changes panel's 2 s refresh notices),
   and on Refresh; never on a timer of its own. Only while the Changes tab shows and GRAPH is open.

## How (the AI's calls)

- **No new library:** `git log --topo-order --format=<hash, parents, author, time, refs, subject>` for the rows;
  `git show -s --format=%B` and `git show --shortstat` for the hover card (only for the row hovered). As every other
  git read in fuxx: off the UI thread, capped, never prompting.
- **The lane layout** is fuxx's own (pure and tested): walk the commits newest first, keep a list of open lanes
  (each waiting for a commit), put a commit in the lane that waits for it (else a new one), then its first parent
  takes its lane and the other parents get lanes of their own; edges from row to row are straight lines or curves.
- **Drawing:** GPUI paths in a canvas per row (as fuxx's icons are drawn), lanes 2 pt, dots 8 pt.
- **Open on GitHub:** built from origin's URL (`git@github.com:o/r.git` or `https://github.com/o/r`) →
  `https://github.com/o/r/commit/<hash>`; other hosts: no link.

## As built (2026-11-12, AI's calls)

- `src/graph_layout.rs` (lanes and edges, 6 tests), `src/git_graph.rs` (commits with full-name refs, first-parent
  stats for a merge, GitHub links, scopes; 5 tests on real repos), `src/graph_panel.rs` (the accordion; 4 tests for its
  words) and the Changes list's rows (`all_rows`: a clean repo shows "No changes since the last commit." as a row,
  the GRAPH under it).
- **The header's buttons:** the scope (Auto → All → Current, one click each), ◎ go to HEAD and Refresh. Fetch, Pull
  and Push are left out of the graph's header: the branch bar right under it has them (two sets would only confuse).
- **The scope is a button that cycles**, not a menu.
- **A row:** lanes (12 pt a column, at most 6), the subject (at least 90 pt; bold at HEAD), up to two pills (your
  branch filled in its lane's colour with ◎, others tinted, a tag grey `tag v1`), `+N` for more, and the author only
  on a row without pills (as Cursor shows it: no room for both).
- **The card** hangs to the left of the panel; its details (the whole message, what it changed) are read when it
  shows; the hash copies the whole hash; Open on GitHub opens in the followed terminal's workspace's browser tab.
- Reads with the Changes panel's 2 s refresh only when GRAPH is open and a ref moved (`stamp`: HEAD and every branch's
  commit, the scope, the page).
- **Sticky headers** (your ask, 2026-11-12: "make other accordion sticky … make it easy for the user experience"):
  while the Changes list scrolls, the header of the group you're in (STAGED, CHANGES or GRAPH) stays pinned at the
  list's top, on the panel's fill with a hairline under it, so its chevron and buttons are always one click away.
- ui-shots `changes-graph`, `changes-graph-card` and `changes-sticky` (60 commits scrolled 8 rows: GRAPH pinned at
  the top; both themes).

## Not in it

Checking out, reverting, cherry-picking or resetting from the graph (right-click menus), searching it, filtering by
author or file, showing a commit's diff in a tab (a later milestone if you want it), drag to rebase.

## Your check (on screen)

1. GRAPH under CHANGES opens and closes; the lanes match `git log --graph` for the same repo.
2. Your branch's pill (◎) and origin's (cloud) are on the right rows; HEAD's row is bold with a hollow ring.
3. A merge shows its curve coming in from the merged lane.
4. Hover a row: the card shows the author, the date, the whole message, the files changed and the hash; a click on the
   hash copies it; Open on GitHub opens the commit.
5. Auto / All branches / Current branch only change what's drawn; Load more adds 200.
6. ◎ scrolls to HEAD; scrolling the list keeps the current group's header (STAGED / CHANGES / GRAPH) pinned at the top.
7. Light theme: the lanes and the pills read well.
