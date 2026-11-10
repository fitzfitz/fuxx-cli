---
tags: [development, milestone, design]
status: built
---

# 66 - Milestone 11o - Branch Switcher (design)

**Status:: 🟨 Built 2026-11-10; merged and pushed 2026-11-10 (merge `0dece54`), released in **v0.6.0** (2026-11-10, run 42753078149); 1325 tests pass. Waiting for your on-screen check (12 items, below); nothing here has been seen on a real screen. Design approved 2026-11-10 by you ("oke setuju, lanjut tulis plan-nya"). Plan: [[M11o Plan - Branch Switcher]].**  ·  **Phase 4: an M11 extension (the chores round after v0.5.0).**
**Asked for 2026-11-10 by you** in the chores conversation, after the sidebar row got its own branch line. It is
in [[12 - Scope]] since its approval (2026-11-10). It is fuxx's **first action that changes a repo**;
everything git-related before it (the row's branch, Files, Changes) only reads.

## Goal

A **modal** to switch the git branch of the folder a terminal is in. It lists the local branches, says which are only
local and which are on origin (with how far ahead and behind), lists origin's branches that aren't local yet, and has
a box to type a branch name from origin. A branch that would collide with your uncommitted changes is **disabled**
with **"conflict x file(s)"**; any other branch asks for a **confirmation**, then switches.

## Why (your words, 2026-11-10)

- "branch bisa ga jadi dropdown untuk tampil dan pilih branch yang sedang ada di lokal dengan pembeda antara yang
  masih lokal dan sudah di push ke origin? dan saya juga bisa input text jika saya mau checkout branch dari origin
  yang tidak ada dilokal?"
- "untuk B, sepertinya jangan dropdown, mungkin sebaiknya popup modal saja?" (AI agreed: it needs a text box,
  sections, warnings and fetch progress, more than a dropdown holds.)
- "Jika ingin pindah branch (klik branch yang mau dipilih) kemudian akan ada conflict, branch disabled dan ada
  tulisan "conflict x file(s)"."
- Told that switching under a running agent changes its files mid-task: "kalau begitu jangan pindah bebas, tapi ada
  konfirmasi".
- Ahead/behind: "boleh jika tidak terlalu berat dan tidak membebankan performance".
- Fetch: "otomatis di latar belakang saat modal dibuka dan ada tombol fetch".
- A typed name that is on neither side: "bilang tidak ditemukan saja".

## What exists today

- `git_branch.rs` (M11b): each workspace row's branch, `git rev-parse --abbrev-ref HEAD` in its subject terminal's
  folder, off the UI thread, cached per folder (`Places`), refreshed at most every 5 s for rows on screen. git runs
  through `run_git` (`GIT_TERMINAL_PROMPT=0`, `GIT_OPTIONAL_LOCKS=0`, no terminal, a 2 s cap, `git_program` finds
  git). `git_status.rs` (M11g) parses `status --porcelain=v1 -z`.
- The sidebar row shows the branch on its own line, only inside a repo (2026-11-10, note 60 "After release"); the
  status bar shows the followed terminal's branch (M11i).
- `palette.rs` (M11j): the ⌘K panel near the top of the window: a box (`NameInput`) over a list, ↑/↓, Enter,
  Escape or a click outside closes it.
- The sidebar knows whether a terminal runs a program (`running`) and its agent status (`Status`).

## Decided by you

1. **A modal, not a dropdown.**
2. **Click a branch to switch to it.** A branch whose switch would collide is **disabled**, its row saying
   **"conflict x file(s)"**.
3. **No free switching: a confirmation every time** a branch can be chosen.
4. **Ahead/behind on each branch**, provided it stays cheap (it does: §2).
5. **Fetch automatically in the background when the modal opens, and a Fetch button.**
6. **A text box** to check out a branch from origin that isn't local; a name on neither side says **not found**,
   nothing is created.

## 1. Opening it

- **Click the branch line** of a sidebar row, **click the branch** in the status bar, or **⌘K → "Switch branch…"**.
  *(AI's call: all three.)*
- **Which repo:** the folder of the terminal whose branch was clicked: the row's subject terminal, or the terminal
  the status bar follows; from ⌘K, the focused terminal's. The modal's title names it: `Switch branch · <repo
  folder name>` and the folder in `text_faint` under it. *(AI's call.)*
- Outside a git repo there is no branch line to click, the status bar shows no branch, and ⌘K doesn't list the
  action. An SSH terminal (its folder is on another machine) never offers it.
- Looks like the ⌘K panel: same place, width, surface, box and row style. *(AI's call: one family of panels.)*

## 2. The list

Top to bottom *(layout AI's call)*:

```
 Switch branch · frontendweb_react_tnlm          [Fetch]  Fetching…
 ~/Kerja/hubexo/frontendweb_react_tnlm
 ┌──────────────────────────────────────────────┐
 │ type a branch name…                          │
 └──────────────────────────────────────────────┘
 ✓ master                 current       ↑0 ↓2       ← the checked-out branch: marked, not clickable
   feature/rows                         ↑2 ↓1       ← on origin: ahead 2, behind 1
   main                                 ✓           ← on origin, even
   spike/test             local only                ← no upstream
   feature/old            conflict 3 file(s)        ← disabled
   fix/cache              upstream gone             ← its origin branch was deleted
 ORIGIN
   origin/fix-login                                 ← on origin, not local
```

- **Local branches:** one `git for-each-ref refs/heads --format=…` gives each branch with its upstream and its
  ahead/behind (`%(upstream:track)`): milliseconds, no network, run only when the modal opens and after a fetch, off
  the UI thread. Nothing runs while the modal is closed: idle CPU is untouched.
- **Marks:** `↑n ↓m` (only the non-zero parts; `✓` when even) for a branch with an upstream; **local only** without
  one; **upstream gone** when the upstream ref no longer exists. Counts come from the last fetch.
- **Pushed without an upstream** *(AI's call, after the builder check)*: a local `foo` with no upstream but an
  `origin/foo` counts as **on origin** (matched by name), with `↑n ↓m` against `origin/foo` from one `rev-list
  --left-right --count foo...origin/foo` each. Such branches are rare; these few extra calls run with the list, not
  per frame. A branch whose upstream is on another remote shows its `↑n ↓m` against that upstream like any other.
- **ORIGIN:** `refs/remotes/origin/*` with **no local branch of the same name** (tracking it or not), `origin/HEAD`
  left out. So a row never offers a branch that already exists locally.
  Only the remote named `origin` *(AI's call: your words name origin; other remotes are out, see "Not in")*.
- **Order:** current first; then local branches by most recent commit; then ORIGIN by most recent commit.
  *(AI's call.)*
- **Detached HEAD:** the top row reads `detached at <short sha>`, marked current; every branch is a target. If HEAD
  has commits on no branch or remote branch (`rev-list HEAD --not --branches --remotes`), the confirmation says
  **"2 commit(s) on no branch: after switching they are only in the reflog"** *(AI's call, after the builder check:
  warn, don't block; nothing is deleted, but they are easy to lose).*
- Keyboard: ↑/↓ move (skipping disabled rows), Enter chooses, Escape closes; typing goes to the box.

## 3. Conflicts

- **What "conflict" means:** git refuses a switch when it would overwrite your uncommitted work. fuxx counts, per
  target, the files that are **both** dirty in the work tree (staged, modified, deleted, or untracked and not
  ignored: `status --porcelain=v1 -z`) **and** different between HEAD and the target (`diff --name-only -z HEAD
  <target>`). That count is the row's **"conflict x file(s)"**, and the row is disabled.
- **Uncommitted changes that don't collide are carried** to the new branch (git's own behaviour); the confirmation
  says how many (§4).
- **Cheap enough:** with a clean work tree there is nothing to count (no conflict is possible). Otherwise the counts
  run in the background after the list shows (rows get their mark as each lands), only for rows in view, at most a
  few at a time. *(AI's call.)*
- **The count is fuxx's estimate, git decides:** git is sometimes more lenient (a local change identical to the
  target's version). fuxx never forces a switch; if git still refuses, its message shows (§6).

## 4. Confirmation

Choosing an enabled branch opens a confirmation inside the modal *(AI's call: an inline step, not a second
window)*:

```
 Switch to feature/rows?
 ⚠ Claude Code is running in this folder          ← when any fuxx terminal in this repo runs a program
 3 uncommitted file(s) come along                  ← when the work tree is dirty
                                   [Cancel]  [Switch]
```

- **Every time**, even with a clean tree and nothing running. *(AI's call, from your "jangan pindah bebas, tapi ada
  konfirmasi": one rule is easier to trust than a rule with exceptions.)*
- **"… is running in this folder":** for each fuxx terminal whose folder is inside this repo and that runs a program
  (not at its prompt), its program's name (its title, else "a program"); several: "2 programs are running in this
  folder". Only fuxx's own terminals are known.
- **Enter** = Switch, **Escape** = Cancel (back to the list, not closed). Switch is the default button.

## 5. Fetch

- **On open:** `git fetch --prune origin` in the background *(AI's call: `--prune` so deleted origin branches leave
  the list; it only removes remote-tracking refs, never a local branch)*. The list shows at once from local refs;
  `Fetching…` shows beside the button; when it ends the list is read again.
- **The Fetch button** runs it again (disabled while one runs).
- **Never hangs:** no terminal, `GIT_TERMINAL_PROMPT=0`, a time cap *(AI's call: 30 s)*.
- **Credential windows** *(AI's call, after the builder check)*: the **automatic** fetch never opens one (git's
  credential helpers told not to be interactive, e.g. `GCM_INTERACTIVE=never`; ssh in batch mode), so opening the
  modal never pops a login or Keychain window; without stored credentials it just says `Fetch failed`. The **Fetch
  button** lets the helper open its own window (a browser sign-in, a Keychain prompt, an ssh askpass), because you
  asked for it. Neither ever waits on a terminal. *(As built: git never asks for the automatic fetch, but an ssh
  agent that confirms each use, or a macOS Keychain prompt, may still ask on its own.)*
- **On failure** the modal stays usable with local data and says `Fetch failed: <git's first line>` in
  `text_faint`. No origin remote: no fetch, no button, no ORIGIN section.
- Closing the modal mid-fetch lets the fetch finish in the background; nothing is shown.

## 6. Switching, the box, and after

- **A local branch:** `git switch <name>`.
- **An ORIGIN row:** `git switch --track origin/<name>` (a local branch tracking it), after the same confirmation.
  *(As built: the branch is made first, then switched to; see "M11o as built" #5.)*
- **The box filters** the list as you type (by substring, case-insensitive) *(AI's call: the ⌘K matcher)*. When the
  typed name matches no local or ORIGIN branch exactly, one extra row appears: **Check out `<name>` from origin**.
  After the automatic fetch a normal clone already lists every origin branch, so this row matters while that fetch
  runs or after it failed, and on a clone that fetches only some branches (`--single-branch`). Choosing it runs
  `git fetch origin <name>:refs/remotes/origin/<name>` *(AI's call: into the remote-tracking ref, so the switch
  works on a narrow clone too; that ref is not a branch, so "nothing created" still holds for branches)*; found →
  its conflict count → confirmation → `git switch --track origin/<name>`; not on origin → **`Branch <name> not
  found`**, nothing created.
- **A typed branch that would conflict:** the confirmation step shows **"conflict x file(s)"** in place of its
  text, and Switch is disabled (only Cancel). *(AI's call: the list row's rule, for a branch that has no row.)*
- **After a switch:** the modal closes; the sidebar rows and the status bar for that folder show the new branch at
  once (fuxx drops the folder's cached branch and looks again, no 5 s wait). The Files and Changes tabs refresh as
  they do when files change.
- **When git refuses or fails** (the tree changed since the counts, a lock, a hook): the modal stays open on the
  list, git's message under the box in the failure colour, nothing changed. The counts are read again.
- **Never** stash, reset, force (`-f`), discard or create a branch from scratch.

## Settled after the builder check (AI's calls, 2026-11-10)

A reviewer read this note as its builder and asked five things; none needed you:
1. Approval: this note waits for yours (that's the next step).
2. A branch pushed without an upstream: on origin by name (§2).
3. Credential windows: never from the automatic fetch, allowed from the Fetch button (§5).
4. A detached HEAD with commits on no branch: warned in the confirmation, not blocked (§2).
5. The typed-name row on a narrow clone: fetched into `refs/remotes/origin/<name>`; a conflicting typed branch
   shows its count with Switch disabled (§6).

Also for the builder: today's `run_capped_*` drops git's stderr; the fetch failure and a refused switch need it.

## Constraints

- git through the existing helpers (`git_program`, `run_git`-style: no terminal, no prompt); the switch itself
  takes git's normal lock (it writes).
- Nothing on the UI thread: every git call on the background executor.
- No new crate. *(AI's call: plain git commands, as `git_branch.rs` and `git_status.rs` do.)*
- Both themes, Reduce motion respected (the modal appears without animation under it), keyboard reachable.
- No change to how fuxx idles: no git runs while the modal is closed, beyond today's 5 s branch check.

## Not in M11o

- Creating, renaming, deleting, merging, rebasing branches; pulling or pushing; stashing.
- Remotes other than `origin`; choosing a remote.
- Worktrees (a branch checked out in another worktree: git refuses and its message shows).
- Submodules.
- Branches of an SSH terminal's remote folder.

## Limits (known, accepted)

- The conflict count is fuxx's estimate (§3); git has the last word.
- Switching under a running agent is allowed after the confirmation: the agent may read the new branch's files
  mid-task (your call).
- Ahead/behind and ORIGIN are as of the last fetch until the background fetch lands.
- Only fuxx's own terminals count for "running in this folder".

## Left to the builder

- The modal's exact sizes and colours (from the ⌘K panel's tokens), and where its code lives.
- How rows show while their conflict count is still being computed.
- How many conflict counts run at once.
- The exact `for-each-ref` format and parsing.
- How the ⌘K action is named in the palette's lists beyond "Switch branch…".

## M11o as built (2026-11-10)

### The three files

- **`src/branch_list.rs`** (pure, no git, no window): parses git's ref listing, builds the rows and their order, the
  marks (`current`, `✓`, `local only`, `upstream gone`, `↑n ↓n`), the filter, the typed-name row, the conflict count
  and every line of copy. Tested with plain data.
- **`src/branch_git.rs`** (runs git, always on the background executor): reads the refs and the dirty files, lists the
  files a target branch would change, fetches (automatic or by button), fetches one named branch, and switches. Git's
  stderr is kept, so a failed fetch or a refused switch can show git's own words. Tested against real temporary repos
  with a bare "origin".
- **`src/branch_switcher.rs`** (the GPUI modal plus its `impl Workspace` block): the panel, the box, the list, the
  confirm step, the busy states, and the refresh after a switch. It sits next to the ⌘K palette and copies its look.

### How it behaves

- **Three ways in:** click the branch line of a sidebar row, click the branch in the status bar, or ⌘K → "Switch
  branch…". Outside a repo, or on an SSH terminal, none of the three does anything (and ⌘K does not list the command).
- **The list:** the current branch first (marked, not clickable), then local branches, then an ORIGIN section for
  origin's branches that are not local. Each row has its mark: `local only`, `upstream gone`, or how far ahead and
  behind it is. The box filters as you type.
- **Fetch:** a background fetch starts when the modal opens (`Fetching…`), and the Fetch button runs it again. For the
  automatic fetch git never asks for a login; an ssh agent that confirms each use, or a macOS Keychain prompt, may
  still ask on its own. The button lets a credential helper open its window. A failure shows
  `Fetch failed: <git's first line>` and the list stays usable. Fetch is disabled on the confirm step.
- **Conflicts:** a branch that would collide with your uncommitted changes is disabled and says `conflict n file(s)`.
  The counts are worked out only for rows in view, at most 4 at a time, and again as you scroll or filter.
- **Confirm step:** choosing a branch shows what will happen: the branch, the files carried over, and a warning line
  if a program runs in that folder (named by its tab title, else its program, else "a program"), and a detached HEAD
  with commits on no branch. Switch or Cancel; Escape goes back to the list. A read landing while it shows (the
  automatic fetch's, say) rebuilds it for the same branch.
- **A typed name:** a row `Check out <name> from origin` appears when the name matches nothing exactly. Choosing it
  fetches that one branch; found → its conflict count → confirm; not found → `Branch <name> not found`, nothing created.
- **Switch:** git does the switch. On success the modal closes and the sidebar rows and the status bar for that folder
  show the new branch at once. If git refuses, nothing in the repo changed: the modal stays open on the list with git's
  message under the box, and the counts are read again.
- **While git works on a choice** (counting, looking for a typed name, switching): keys and clicks in the modal are
  ignored, Escape too, and nothing replaces it: the three openers and ⌘K do nothing. If you click elsewhere in fuxx
  meanwhile, the box gets the keyboard back when git is done.

### What the build settled (all AI's calls, marked as such; none needed you)

1. **The current row says only `current`** (AI's call). The design's sketch showed `current ↑0 ↓2`, but a row has room
   for one mark, and the sketch's own `↑0` broke the "no zero" rule. If you want the current branch's ahead/behind, it
   can be added later.
2. **Dirty files are counted wider than a plain status** (AI's call): untracked files inside a new folder are listed
   one by one (not as `dir/`), and a rename counts both its old and new path. Otherwise a collision through those
   files was never counted. Cost: a slower status in a repo with a huge untracked tree.
3. **git's messages are forced to English** (AI's call, `LC_ALL=C`): fuxx reads git's text to tell "not found" from
   other failures, and a translated message would break that. Cost: a user with a localized git sees English in the
   error line.
4. **`git switch` has a 120 s time cap** (AI's call, changed during the build). First it was 10 s, but two reviewers
   pointed out that killing a switch half-way can leave a half-switched tree and a `.git/index.lock`, which breaks
   "nothing changes unless git agreed". fuxx must never hang, so there is still a cap; it is long enough that only a
   truly stuck switch (a hook, say) meets it.
5. **An origin branch is made first, then switched to** (AI's call, from the final review). `git switch --track
   origin/<name>` (and `switch -c`) can write the target's files into the tree and index and only then fail to make
   the branch (a narrow clone; a name clashing with a local branch as a folder, `feature` against `feature/x`),
   leaving the target's files staged among yours. So fuxx first runs `git branch --no-track <name>
   refs/remotes/origin/<name>` and writes its upstream (`branch.<name>.remote` = origin, `branch.<name>.merge` =
   `refs/heads/<name>`): none of that touches the tree, and `git branch` refuses a clash cleanly. Then `git switch
   --no-guess <name>`. If the upstream can't be written or git refuses the switch, fuxx takes back exactly what it
   made: the upstream keys go back to what they were, and the branch is deleted only while it still points where it
   was made (`git update-ref -d refs/heads/<name> <commit>`, git checks) and HEAD is not on it. git's refusal is what
   you see. So the only command that changes the work tree is a switch to a local branch that exists, which git
   either refuses whole or does. It works whatever origin's fetch rule is. Cost: `--track`'s extras (e.g.
   `branch.autoSetupRebase`) are not applied.
6. **The typed name's fetch may open a credential window** (AI's call), like the Fetch button: you chose the name and
   asked for it. A typed name also waits for a fetch that is already running (up to 30 s; it can't be cancelled).
7. **The typed row appears only when the repo has an origin** (AI's call): with no origin there is nothing to fetch.
8. **Choosing a row whose count has not landed yet counts it first** (AI's call), then goes to the confirm step, or
   marks the row `conflict n file(s)` and stays on the list. A pending count must never lead to a confirm.
9. **Colours:** `Fetch failed` is drawn in faint text (the design's §5); a refused switch and `not found` use the error
   colour (the design's §6).
10. **Busy texts that were not in the design's copy list** (AI's call): `Counting conflicts…`, `Looking for <name> on
    origin…` and `Switching…`.
11. **The switcher follows the row's FIRST terminal**, the one the row's branch is read from, so the modal and the
    branch line you clicked always agree.
12. **One "switchable folder" rule** (AI's call): ⌘K and the three openers use the same check, so a listed command
    never silently does nothing.
13. **The running-programs list is taken when the modal opens** (and matched to the repo's root after git's first read,
    off the UI thread).
14. **No guessing, full names** (AI's call, from the final review): a local switch is `git switch --no-guess <name>`,
    so a branch deleted between the list and your Switch is refused, not re-made from origin's; origin's branches
    are always named in full (`refs/remotes/origin/<name>`), so a local branch literally called `origin/<name>` can't
    be taken for it.
15. **`@` is not a branch name** (AI's call): git's name check lets it through, but it means HEAD; a typed `@` says
    `Branch @ not found`.
16. **Nothing replaces a busy switcher** (AI's call, from the final review): while git works on a choice the openers
    and ⌘K do nothing (a switch can't be called back), and if the box lost the keyboard meanwhile it gets it back
    when git is done.

### Limits

- A switch killed at the 120 s cap can leave a half-switched tree or a `.git/index.lock`; you would have to clean it up
  by hand.
- The running-programs list is a snapshot from the moment the modal opens.
- A switch that finishes after the modal was closed or replaced cannot report back; the rows catch up at the next 5 s
  refresh.
- The conflict count is still fuxx's estimate; git has the last word.
- Two quick re-opens can overlap fetches (a spurious `cannot lock ref` in `Fetch failed`), and a fetch killed at the
  30 s cap can leave `.lock` files under `.git`.
- If fuxx stops between making an origin branch and switching to it (a crash), the new local branch stays, tracking
  origin's: harmless, and it shows in the list.
- Other boxes (help, the notifications popover, a menu) still close a busy switcher; the switch runs on and the rows
  catch up at the next 5 s refresh.

## On-screen checklist (pending)

Nothing here has been seen on a real screen yet. The ui-shots draw the panel but cannot show the network, a sign-in
window, a real conflict, or the pointer.

1. **Open from the sidebar:** in a repo, click the branch line of a row: the modal opens, titled `Switch branch · <folder>` with the folder under it. The pointer is a hand over the branch line.
2. **Open from the status bar and ⌘K:** click the branch at the bottom (a hand over it): the modal opens for the terminal the bar follows. Press ⌘K, type `switch`, Enter: it opens for the focused terminal.
3. **The marks:** against a repo you know, `current` on your branch, `local only` on one never pushed, `↑n ↓n` where you are ahead or behind, `upstream gone` where origin deleted it, and origin's other branches under ORIGIN.
4. **Fetch:** on open you see `Fetching…`, then the list refreshes (new origin branches appear). Press Fetch: same again. Try a repo whose login needs a sign-in: for the automatic fetch git never asks (an ssh agent that confirms each use, or a macOS Keychain prompt, may still ask on its own); the Fetch button may open a sign-in window.
5. **Offline:** turn Wi-Fi off and open it: `Fetch failed: …` in faint text, and the list still works.
6. **A conflicting branch:** make an uncommitted edit to a file that another branch also changes: that branch is greyed with `conflict 1 file(s)` and cannot be chosen. A branch that does not touch your files can.
7. **Confirm with an agent running:** start an agent in that folder, open the modal, choose another branch: the confirm step names the running program. Escape goes back to the list; Cancel does too.
8. **A real switch:** confirm Switch: the modal closes, the sidebar row's branch line and the status bar show the new branch at once, and the files under the terminal changed.
9. **A typed origin branch:** type the name of a branch only on origin (right after opening, before the fetch lands, or one that your clone does not fetch): `Check out <name> from origin` appears; choose it, confirm, and you land on a local branch tracking origin.
10. **A name that does not exist:** type a nonsense name and choose its row: `Branch <name> not found` in the error colour, nothing created (`git branch` shows nothing new).
11. **Outside a repo and over SSH:** in a plain folder there is no branch line, no branch in the status bar, no "Switch branch…" in ⌘K; an SSH terminal shows none of them either.
12. **Looks:** open the modal in both themes, and on a 1x and a 2x display: text is sharp, the marks are faint, the panel matches ⌘K, and Reduce motion shows it without animation.

## Links

- [[12 - Scope]] · [[40 - Roadmap]] · [[60 - Milestone 11i - UI Uplift 2]] (the row's branch line, "After
  release") · [[61 - Milestone 11j - Command Palette]] (the panel it copies) · [[59 - Milestones 11g-11h - Right
  Sidebar and File Tabs]] (git status parsing)
