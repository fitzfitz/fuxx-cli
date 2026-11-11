# Milestone 11v — Branch Bar

**Status:** 🟨 built 2026-11-11 on branch `m11v-branch-bar` (not committed); 1378 tests pass; the `changes-bar` ui-shots draw every state in both themes. Waiting for your on-screen check (below). Plan [[M11v Plan - Branch Bar]].
**Asked for:** 2026-11-11, by you: "do you think it's best to put the branch here [the sidebar's group header] or maybe
we remove it and we improve this [the Changes tab]? we add branch popup picker, we can fetch, pull, etc." →
"remove it from sidebar, add it to changes tab, we update everything on a new milestone" → three rounds of browser
mockups: "I love B, but the current branch information and buttons become bottom bar" → "go with B3. But the button
needs to be icon only no text, but show tooltip when hover position top." → "ok B3 is good, write the milestone and
plan and directly implement it".

## Why

The branch belongs to a folder, so the sidebar showed it once per folder group, crowding the header (the name was cut
to `fix/hdw-appro…`). The Changes tab is where you look at a repo's state, so the branch and the git actions you
reach for (fetch, pull, push) go there, at the bottom of the tab.

## What it does

1. **The sidebar's group header loses the branch.** The letter tile, the folder name, the count and the chevron stay.
   (A row outside a group keeps its own place lines; the status bar keeps the focused terminal's branch.)
2. **The Changes header is one line:** the repo's name, then `N files · +A −R` at the right (the totals move up from
   their own line). With no changes, the name alone.
3. **A bottom bar (B3)** in the Changes tab, for the followed terminal's repo, edge to edge with a hairline above it:
   - **Left, two lines:** `⎇ branch ▾` (13 pt semibold; a click opens the branch switcher, M11o, for that terminal),
     and under it one faint line saying where the branch stands (below).
   - **Right, icon buttons only (28 pt), no text:** **Fetch** (↻, always there), then **at most one** action, filled
     with the accent, when there is something to do: **Pull** (arrow down to a line), **Push** (arrow up from a
     line) or **Publish** (a cloud with an arrow up).
   - **Each button's tooltip shows above it** after a short hover (0.4 s), saying exactly what a click does: "Fetch
     from origin", "Pull 2 commits from origin", "Push 1 commit to origin", "Publish feat/x to origin". The
     rightmost one lines up with the button's right edge so it stays inside the panel.
4. **The line under the branch:**

   | State | Line | Action button |
   |---|---|---|
   | Up to date | `✓ Up to date · fetched 3 min ago` (green ✓) | none |
   | Behind | `↓2 behind · fetched 3 min ago` | Pull |
   | Ahead | `↑1 ahead · fetched 3 min ago` | Push |
   | Diverged | `⚠ ↓2 ↑1 diverged · merge or rebase in the terminal` (yellow) | none |
   | No upstream | `Local only · not on origin` | Publish |
   | Upstream gone | `Upstream gone · fetched 3 min ago` | none |
   | Upstream not on origin | `Tracks upstream/main · not origin` | none |
   | Detached HEAD | `⎇ a1b2c3d` (the short commit) and `Detached HEAD` | none |
   | No `origin` remote | `No remote named origin` | none, and no Fetch |
   | Working | `Fetching…` / `Pulling 2 commits…` / `Pushing 1 commit…` / `Publishing…` | all buttons disabled |
   | Failed | git's first line in red, e.g. `Pull failed: Your local changes … would be overwritten` | as before it |

   "fetched N ago" is when origin was last fetched in this repo (the time on git's `FETCH_HEAD`): `just now`, `N min
   ago`, `N h ago`, `N d ago`; left out if it never was.
5. **No bar** when the Changes tab shows a note instead of a repo (not a repo, an SSH terminal, no folder).

## Part 2: Staged and Changes (added 2026-11-11)

**Asked for:** "can you group this by staged and changes? and I can revert and add from changes to staged as well.
possible? make the UI great" → "include it in M11v, show me the mockups" → mockups round 4 → **"C with R1, go ahead
and implement it"**.

- **Layout C, check circles.** The header: a circle (empty: nothing staged; a dash: some; a filled ✓: all), the
  repo's name, and `2 of 19 staged` (the staged count green); under it a 3 pt bar, the staged share in green.
  Clicking the header's circle stages everything, or unstages everything when all is staged.
- **Two groups**, each under a small-caps divider with a hairline: **STAGED** (only when something is staged), then
  **CHANGES** (only when something isn't). A file staged and then edited again shows in both, its staged part above
  and its new part below; each row's counts are that part's (`git diff --cached` / `git diff`, untracked counted
  from disk).
- **Each row** has a circle at its left: filled green ✓ under Staged, empty under Changes (green-edged on hover).
  Clicking the circle stages or unstages that file; clicking the rest of the row opens its diff, as before.
- **Revert (R1):** hovering a row under Changes swaps its counts for ↺ (red on hover); hovering the CHANGES divider
  shows ↺ "Revert all…". Each opens a dialog over the Changes tab saying what is lost, e.g. "Revert 60 - Progress
  Log.md? Its 12 added and 1 removed line(s) since it was last staged or committed are thrown away. This can't be
  undone." with **Cancel** and a red **Revert**; for a new (untracked) file it says **Delete**. Clicking outside the
  dialog cancels it.
- **The git:** stage `add -A -- <path>`; unstage `restore --staged -- <path>` (no commits yet:
  `rm --cached -q -- <path>`); revert a changed file `restore --worktree -- <path>` (back to its staged or
  committed state; its staged part stays); revert a new file `clean -f -q -- <path>`; "all" versions run on the
  whole list. Every path goes after `--` with `--literal-pathspecs`; a folder entry (a nested repo) can't be reverted.
  One git write at a time per repo: while one runs, the bar and the circles wait, and the bar's line says
  `Staging…` / `Unstaging…` / `Reverting…`; a refusal shows in red in the bar's line.
- **AI's calls (you said "go ahead"):** the diff tab stays as today (the whole change against the last commit), not
  "staged part only" (that needs a new diff mode across the file tabs: later, if you want it); the dialog sits over
  the Changes tab, not the whole window, and Esc doesn't reach it (the tab takes no keys): Cancel or a click outside;
  no Undo; the header shows `N of M staged` instead of the line totals (as mockup C), M counting files, not parts;
  revert is offered only under Changes (unstage first to revert a staged part).
- **Not in it:** committing, staging single lines or hunks, stash, the staged-only diff view.

### Part 2 as built

- A second fresh review (no case found where Revert touches a file the dialog didn't name; 5 fixes, each with a test
  that failed first): clicks outside the dialog no longer reach the rows, circles or bar buttons under it; a
  `git add -N` file gets no ↺ (git's clean would skip it and a restore would empty it); the header circle's look and
  its click follow one rule (a file staged and edited again, or files past the 1000 cap, count as not all staged:
  never "unstage all" when it isn't all); ↺ asks about the file by its path, so a refresh can't swap the file the
  dialog names.
- Left for later (minor): the dialog doesn't follow later edits to its file; Revert does nothing (undimmed) while
  another git write runs; before the first commit, unstaging a file staged then edited fails with git's message;
  stage-all adds nested repos; non-UTF-8 names match nothing (fail-safe); a conflicted file can't be reverted; both
  rows of a half-staged file highlight when its diff is open.

## Part 3: Status Bar on/off (added 2026-11-11)

**Asked for:** "add 1 more setting on General to hide status bar with toggle On/Off … "Status Bar: On/Off"" →
(asked where it goes) "Part of M11v".

- Settings → **General** gets **Status Bar** with an On / Off switch, On by default. Off hides the bar at the
  window's bottom (status, branch, folder, ports, the update button); the panes take its room. Saved like the
  other settings (`status-bar = on|off`) and applied at once.
- **AI's calls:** while the status bar is hidden, the Update Available button has no place to show; it shows again
  when the bar is on (the update itself still downloads in the background as before). The label is "Status Bar",
  as you wrote it. With the bar off, the 🔔 list and the + menu sit on the footer (no gap where the bar was).

## How (the git)

- **Reading** where the branch stands is local only (no network): with the Changes list's 2 s refresh, while the tab
  shows. `symbolic-ref` for the branch; `for-each-ref` for its upstream and `[ahead N, behind M]` / `[gone]`; `remote`
  for origin; `rev-parse --git-path FETCH_HEAD` for the fetch time.
- **Fetch:** `git fetch --prune origin`, as the branch switcher's Fetch button (M11o): credential helpers may ask.
- **An automatic fetch** (never asks for a login: M11o's `Auto` mode) when the tab starts showing a repo whose last
  fetch is older than 5 minutes, or never happened. Not on a timer.
- **Pull:** `git pull --ff-only` (only when behind and not ahead, with an upstream). Git refuses, and its message is
  shown, when it can't fast-forward or would overwrite your uncommitted files; nothing is ever merged, rebased,
  stashed or forced.
- **Push:** `git push` (only when ahead and not behind, with an upstream). Never `--force`.
- **Publish:** `git push -u origin HEAD` (a branch with no upstream; origin exists): makes origin's branch of the same
  name and sets it as the upstream.
- Every command is spelled out, never takes a typed name, never waits on a terminal (`GIT_TERMINAL_PROMPT=0`), and has
  a time cap (fetch and push 30 s, pull 120 s as a switch). One action at a time per tab; the line is read again
  after it.

## The AI's calls (you skipped the details: "directly implement it")

- **No confirmation** before Fetch, Pull, Push or Publish: none of them can lose work (fast-forward only, never force)
  and the tooltip says what will happen. (The branch switcher keeps its confirmation.)
- **The branch opens the existing switcher modal** (M11o), not a new popover: same list, same safety.
- **No spinner while working:** the buttons dim and the line says what's happening. A spinner would redraw the window
  every frame (the cost M11p is about).
- **The error stays** until the next action, a different repo, or the branch changing.
- **The status bar keeps its branch** (the focused terminal's; you only asked for the sidebar's to go).
- **Diverged gets no button:** merging or rebasing stays a terminal job.

## As built (AI's calls while building)

- The tooltips hang from each button's top-right corner (lined up with its right edge), Fetch's too, not centred:
  GPUI can't centre a tooltip without measuring it; none runs past the panel.
- The line's colours are the theme's ANSI green, yellow and red, pushed to read on the bar (a test holds them to
  the text contrast in every theme); the bar's fill is the tab strip's; the action button is the accent with its
  glyph in the accent's text colour.
- "fetched N ago" is worked out when the panel redraws; no timer was added for it.
- The tooltip's box is shared with the other tooltips (`tip_box`).
- After a fresh review (5 fixes, each with a test that failed first): a failed pull or push says git's refusal, not
  its `From …` / `To …` line; a tag named like the branch no longer turns a tracked branch into "Local only" with
  Publish; a branch tracking another remote (`upstream/main`) gets no button and says so; an action still running
  on a repo you left keeps that repo's bar busy when you come back (no second git command on it); a failed
  **automatic** fetch says nothing in red (you didn't ask for it).
- Left for later (minor): a tooltip can linger if a refresh removes its button; each linked worktree has its own
  "fetched" time; Pull isn't held back while an agent works in the same repo (git refuses only if your uncommitted
  files would be overwritten).
- Tested by machine: the git reads and the four actions against real throwaway repos (a pull that would overwrite
  your file is refused, the file untouched; a diverged pull is refused; push and publish reach origin). Not by
  machine: the 0.4 s hover delay and a real click (your check items 4–8).

## Not in it

Merge, rebase, stash, force-push, commit, staging, creating or deleting branches, other remotes than `origin`, a
popover picker, a fetch timer, the branch for SSH terminals.

## Your check (on screen)

1. The sidebar's group headers show no branch; the folder name has the room.
2. Changes' header: the repo name and `N files · +A −R` on one line.
3. The bottom bar shows `⎇ branch ▾` and its line; a click on the branch opens the branch switcher.
4. Hover ↻: after a moment, "Fetch from origin" appears **above** it.
5. ↻ fetches ("Fetching…", then "fetched just now").
6. A branch behind origin shows ↓N and the accent Pull button; Pull brings it up to date.
7. A branch with a local commit shows ↑N and Push; Push sends it.
8. A new local branch shows "Local only · not on origin" and Publish; Publish makes it on origin.
9. Diverged: the yellow line and no action button.
10. A pull that would overwrite your uncommitted file shows git's message in red.
11. Light theme: the bar, the line and the tooltip read well.
12. Part 2: Changes shows STAGED (✓ green circles) above CHANGES (empty circles); the header says `N of M staged`
    with a green share in its bar.
13. Click a row's circle under Changes: it moves up to STAGED; click a ✓: it moves back. The diff doesn't open.
14. The header's circle: a click stages everything; when all is staged it shows ✓ and a click unstages everything.
15. Stage a file, edit it again: it shows in both groups, each with its own counts.
16. Hover a row under Changes: ↺ replaces its counts; click: a dialog names the file and what's lost; Cancel, or a
    click outside, keeps everything (and clicks nothing under it); Revert puts the file back.
17. A new (untracked) file: the dialog says Delete; it's gone after.
18. Hover the CHANGES divider: ↺ "Revert all…" asks with the count of files.
19. Part 3: Settings → General → Status Bar Off: the bar at the bottom goes at once, the panes reach down, the 🔔 list
    and the + menu sit on the footer; On brings it back.
