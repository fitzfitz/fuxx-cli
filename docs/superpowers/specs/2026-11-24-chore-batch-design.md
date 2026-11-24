# Chore batch — 2026-11-24 (design)

Agreed with you item by item on 2026-11-24, before any code. "Your call" marks a choice you made;
"my call" marks one I made that you can overrule. Built on branch `chore/batch-2026-11-24`, in three
tiers (small → medium → big); the tests and the affected screenshot scenes run once per tier.

## Why

A round of polish on the agent box, tabs and text fields, plus two bugs, so you stop asking the
agent "status?" and stop hitting rough edges: long status text floating out of the folder tab,
icons nobody can read, a workspace that falls out of its folder, no undo anywhere.

## Non-goals

- No new agent integrations beyond reading the task list the agents already send.
- No click-to-open on the hover panels (your call: hover only).
- No ACP work; the plan tab reads ACP plan updates only when providers move to ACP later.

## Small tier

1. **Usage gauge opens on hover only.** After the pointer rests on it for **0.8 s**; leaving sooner
   opens nothing. It stays open while the pointer is on the gauge or its panel and closes when it
   leaves both. The click goes away. One shared hover-delay piece (also used by chore 3).
2. **Smaller gap** (about half) between the status and the folder chip, in the agent box's folder
   tab and the shell tab's.
4. **Gutter-bar suggestion list** (fuxx-completes, zsh and bash/ble.sh): one blank line under the
   input; rows `▎ 1  command`; the whole list greyed (bar, number, command dim); the selected row's
   bar in the accent colour and its command at normal brightness; no "Suggestions" title, no hint
   line.
7. **A blinking caret in every `NameInput` field** (search, commit message, palette, branch
   switcher, rename, preferences, file and browser tabs) — not the agent box or the terminal
   chatbox. An on/off timer every ~0.5 s that redraws only that field; solid while typing, blinking
   again ~0.5 s after you stop; stops when the field loses focus, the window goes to the
   background, or after ~20 s without typing. Not GPUI's frame animation. Follows the existing
   cursor-glow switch in Preferences (off: a solid caret).
8. **Tab marks: only Working (the turning arc) and Waiting (the breathing dot).** Done's green check
   and its reserved width go; the tooltip still says "done 4m". The one done mark left: a small
   **"done, not seen" dot** on a tab that finished while you weren't looking at it, cleared when you
   open that tab.

## Medium tier

3. **Cost and Context move to a clock icon** (Lucide `clock`) between the model pill and the gauge,
   opening on the same 0.8 s hover: `+$1.31 $314.65 Context ▬ 69%`. The row leaves the top bar. The
   clock gets a warning dot past **80 %** context (as the gauge's limit dot). Narrow footer: the
   clock and gauge stay, the model name shortens first.
9. **Close confirmation on every agent and terminal tab** (× and ⌘W), reusing the existing close
   confirmation. The dialog has a **"Don't ask again"** box; a Preferences switch turns asking back
   on. Closing a workspace or the window asks **once**, listing what still runs ("2 agents working,
   1 terminal running npm").
10. **Bug: a workspace leaves its folder.** Cause (confirmed by your repro): `Workspace::folder_group`
    reads the folder from the workspace's first *terminal* tab; agent, browser and file tabs carry
    none, so a workspace left with only an agent tab has no group and its folder disappears. Fix: the
    workspace keeps its own folder, set when it's made; it also feeds new tabs opened from it and is
    saved and restored. A failing test first.
12. **Status out of the folder tab** (it ran past the tab's edge). The chat's working line
    (`THINKING_WORD`) becomes `◌ Cultivating… · <running action> · 0:31` (+N more; cut with …),
    led by the task when a plan exists (`Task 5/6 · …`); under it a dim `4 changed · checks 3/7`
    that opens the ledger popover. While the agent waits on you the line says `Waiting for you ·
    <question>` and the tab shows the waiting dot. The folder tab keeps only the status icon, the
    folder and the branch, each cut with … before the edge. My call: when the turn ends the line
    becomes `✓ Done · 0:42` with the counts line kept under it.

## Big tier

11. **Readable activity tracks** (your pick: "C2 with C1's hover"; mockup
    `.superpowers/brainstorm/84410-1795514530/content/tracks-timeline-v2.html`). Beads become Lucide
    icons in tinted tiles — eye read · search · file-pen edit · terminal command · check / x tests;
    a red tile is an action that failed — with no connector lines. Columns stay one action each in
    time order, under **phase bands**: Looking around (reads, searches) → Ran (commands) → Changing
    (edits) → Checking (tests), a band per run of same-phase columns. Hovering a column (and ←/→)
    highlights it and names the action (`#5 · Read tab_bar.rs, lines 1280–1296`). Summaries spell
    states in words ("1st try failed", "expected — test written first"). Both themes; narrow-pane
    rows use the same icons. Supersedes §2's bead glyphs in
    `2026-11-24-agent-activity-view-design.md`.
6. **⌘Z / ⇧⌘Z in every text input.** One undo history shared by `Composer` (agent box) and
   `NameInput` (every other field): typing groups into word-sized steps; a paste, a cut or a deleted
   selection is one step; removing an attachment chip and recalling a prompt with ↑ are undoable;
   each field keeps its own history, cleared when the message is sent or the commit made. At the
   shell prompt (the terminal chatbox is zsh's own line editor) ⌘Z sends zsh's undo (Ctrl-_) and
   ⇧⌘Z a key fuxx-completes binds to zsh's `redo`; while another program runs, both pass through.
5. **Plan progress: a burger tab with `4/6`** at the right end of the folder-tab row (mockup look
   B). Click opens a popover: each task ✓ done (with its commit, which opens it), ◐ being built
   (with its uncommitted changes) or ○ next, for Main and each subagent. Workflow-agnostic sources,
   best first: the agent's own task list (Claude's current task tools — fuxx reads only the old
   `TodoWrite` today, fixed first), `- [ ]`/`- [x]` checklists in a plan file the agent works from,
   git (commits on the branch, uncommitted changes) as the fallback. Shown only while a task list
   exists; all done keeps `6/6 ✓` until your next message; a key opens it.

## Left to the builder

Exact sizes and colours (follow the existing tokens and both themes), the key for the plan
popover (one not taken), how a commit is matched to a task (checkbox ticks first, then a
"Task N" in the message, then order).
