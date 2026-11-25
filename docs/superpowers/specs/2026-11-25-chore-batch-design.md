# Chore batch — 2026-11-25 (design)

Agreed with you on 2026-11-25, before any code. You picked **A** for every mockup (folder picker,
changed files, plan card). "Your call" marks a choice you made; "my call" marks one I made that you
can overrule. Built on branch `chore/batch-2026-11-25`; recorded in note 93 ("Chores after M11ar")
and the Progress Log.

## Why

A polish round on the agent box and the chat, so the places you look at every turn — the commit
box, the folder picker, the list of changed files, the usage panel, your messages' hover icons —
read well and behave, plus one gap: an agent in Plan Mode sends its plan, and fuxx shows it as
unreadable JSON.

## Non-goals

- No agy plan card: agy's stream has no plan-approval step that fuxx can read today (Claude only).
- No new data sources beyond git's own counts for the changed files.
- The folder picker's "This Computer ›" side panel stays as it is (option B's chips were not picked).

## Approaches

Each chore has one sensible way; the two real choices were made on the mockups:
- **Folder picker:** A (name first, its place under it) over B (A plus filter chips while searching).
- **Changed files:** A (grouped by folder) over B (flat, biggest change first, with action buttons).
- **Plan card:** A (the rendered plan in the chat) over B (a short card in the chat, the plan in a
  side panel) — A reads without an extra click.

## The chores

1. **Commit box: a text area, at most 2 lines.** The message wraps and grows to 2 lines, then
   scrolls inside. ↵ commits, ⇧↵ makes a new line (my call). ✦ (write a message) and Commit go
   light grey and take no clicks or hover while they can't act: nothing staged, or (Commit) no
   message.
2. **Folder picker, mockup A.** A row: a tile, the folder's **name** in bold, where it lives
   (`~/code`) in dim text under it. The current folder: an accent tile and ✓. `~` reads
   **Home**. ⌘1–⌘9 open the first nine Recent rows (shown at each row's right). A divider, then the
   actions with their own icons (Browse this computer ›, New project…, No project). A key-hint
   footer: ↑↓ move · ↵ open · esc close.
3. **Changed files, mockup A.** A total line (`8 files changed  +142 −37`), files grouped under
   their folder (each group collapsible, with its count), an A/M/D badge and +/− on every row.
   Counts the turn's edits don't give come from git (`--numstat`; a new file's lines), read off the
   UI thread (my call). Click: the file's diff; ⌥-click: the file. **Bug fixed:** a path a shell
   command wrote is resolved after any `cd` in that command, each file is listed once, folders are
   left out.
4. **Usage panel centred** over its clock icon, kept inside the window near an edge.
5. **Plan card, mockup A.** Claude's `ExitPlanMode` request (its input: `plan` markdown,
   `planFilePath`) becomes a Plan card instead of the generic permission card; the provider file
   reads it into a provider-neutral event. The plan renders as markdown (the chat's renderer), titled
   from its `#` heading, folded after ~12 lines with "Show the whole plan". ↗ opens the plan file in
   a file tab. Answers: **Approve · auto-accept edits** (allow, then the tab's mode goes to accept
   edits), **Approve · ask per edit** (allow, the default mode), **Keep planning…** (a box for your
   note; ↵ sends it as the refusal). Answered, the card folds with "Approved ✓" / "Kept planning".
   **Refined on the fuxx-look mockup (2026-11-25, "Love these" / "loved the idea"):**
   - The card has an accent rim, not the permission card's red: a plan isn't a risk warning.
     Header: ☰ **Claude's plan** and the file chip ↗. Under it the title, then the body.
   - ↵ in an empty box takes **Approve · auto-accept edits** (Claude Code's own first choice). The
     box's placeholder while a plan waits: `Tell Claude what to change in the plan… (optional)`.
     The status line reads `Waiting for you · Claude's plan is ready`.
   - Typing in the box (or **Keep planning…**) hides the card's buttons, so ↵ sends the note and
     can't approve by accident. Esc brings the buttons back. A re-plan arrives as a new card; the
     old one folds with "Kept planning".
   - **Show the whole plan** opens the card in place (no overlay, and no scroll area inside the
     card: the chat's own scroll moves it). **Show less ▴** sits at the top and the bottom. A long
     plan gets outline chips under its title, one per `##` heading; a click scrolls to that part.
   - While the open card's buttons are scrolled out of view, a slim answer bar pinned above the
     agent box (☰, title, the three answers) keeps them in reach. It goes when the card's footer
     is back in view, or once you answer.
   - Answered: one line (☰, title, `N steps · ▸ show`, the answer in green). ▸ opens the plan for
     reading.
6. **Your message's hover icons stay** while the pointer moves onto them (today they vanish).
   Debugged to its cause first.
7. **The plan tab sits flush with the box's right edge** (added 2026-11-25, your ask: make its
   corner like the folder tab's). It mirrors the folder tab: its right side runs straight down into
   the box's side, the box's top-right corner is square under it, and a concave step (the folder
   tab's, flipped) sits on its left. Today it's inset 24 pt (`PLAN_TAB_INSET`).
8. **A track column's cursor clears on a click elsewhere** (added 2026-11-25: "the 'active' border
   is not removed when I clicked anywhere"). Today only Esc clears it. A mouse-down outside the
   step's tracks clears it and gives the keyboard back to the box, except while the call's detail
   it opened is still showing (my call: you're still looking at that call).
9. **The plan popover is left-aligned** (added 2026-11-25: "should be left aligned"). The
   `☰ 0/6` tab's popover opens with its left edge on the tab's left edge, and the window edge
   still pushes it back in if needed.
10. **Your message's bubble wraps in a narrow pane** (added 2026-11-25: "user's chat not as
    responsive"). Today a long one-line message in a narrow agent pane runs off the column's left
    edge and is cut. It wraps to the chat column's width instead. Debugged to its cause first.
11. **A copy icon on the chat's code blocks** (added 2026-11-25: "no copy icon to copy the
    command"). Today only the file tab's Markdown preview has one (its header strip with Copy,
    M11ai); the chat draws code blocks bare. Each chat code block gets a copy icon at its top-right
    corner: always shown, dim, full ink under the pointer, tooltip "Copy". It copies the block's
    whole text and turns into ✓ for about 1.5 s. No header strip in the chat (my call: it would
    make every small command block twice as tall).
12. **No ◐ before work has started** (added 2026-11-25, your pick). The plan tab marked the first
    unfinished task "being built" whenever none was, so a plan still being discussed showed Task 1
    in progress. Now a task shows ◐ only from real progress: a ticked box in it, the agent's own
    todo marking it active, or (between tasks) an earlier task already done by a commit.

13. **The agent box's branch chip keeps up with the repo** (added 2026-11-25: "branch on outer
    chatbox is not align"; the chip said `feature/rows…` while the switcher's ✓ was on
    `feature/plan-card`). Today the chip reads the branch only at boot, when the folder
    changes, and when fuxx's own switcher switches (`agent_tab.rs` `read_branch`), so a
    `git switch` by the agent or in another terminal leaves it stale. It reads the branch again
    after each of the agent's shell commands finishes (at most once per 500 ms), at each turn's
    end, and when fuxx's window comes back to the front (my call: covers a switch made outside
    fuxx without polling).

The plan has one task per chore (your pick, 2026-11-25), so the plan tab counts chores, plus the
last task for the docs, screenshots and review. Each commit names its task (`Chore (Task N): …`)
so the tab ticks it off.

## Left to the builder

- Exact sizes and spacing, within the app's tokens (`ui_tokens.rs`), with no functional text under
  11 px.
- How the git counts are cached between turns.
- Which ui-shots scenes to add or update for each chore.
