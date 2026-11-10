---
tags: [development, milestone, design]
status: design
---

# 66 - Milestone 11o - Branch Switcher (design)

**Status:: ⬜ Design approved 2026-11-10 by you ("oke setuju, lanjut tulis plan-nya"). Plan: [[M11o Plan - Branch
Switcher]]. Not built.**  ·  **Phase 4: an M11
extension (the chores round after v0.5.0).**
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
  asked for it. Neither ever waits on a terminal.
- **On failure** the modal stays usable with local data and says `Fetch failed: <git's first line>` in
  `text_faint`. No origin remote: no fetch, no button, no ORIGIN section.
- Closing the modal mid-fetch lets the fetch finish in the background; nothing is shown.

## 6. Switching, the box, and after

- **A local branch:** `git switch <name>`.
- **An ORIGIN row:** `git switch --track origin/<name>` (a local branch tracking it), after the same confirmation.
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

## Links

- [[12 - Scope]] · [[40 - Roadmap]] · [[60 - Milestone 11i - UI Uplift 2]] (the row's branch line, "After
  release") · [[61 - Milestone 11j - Command Palette]] (the panel it copies) · [[59 - Milestones 11g-11h - Right
  Sidebar and File Tabs]] (git status parsing)
