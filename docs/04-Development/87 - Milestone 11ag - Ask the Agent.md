# Milestone 11ag — Ask the Agent from the Code

**Status:** 🟨 built on branch m11ag-ask-agent, waiting for your on-screen check; not merged. Designed and approved 2026-11-19 ("approved"); all 14 plan tasks (0–13) built 2026-11-19 ([[M11ag Plan - Ask the Agent]]), Tasks 0–12 each reviewed; the whole-branch review and its fix wave still follow (the numbers below are from before it).

**Asked for:** 2026-11-16, by you, in the file-view mockups (near-future idea 3, [[Proposal - File View Upgrades]]):

> - Select lines in a file tab → **Ask claude** (and ⌘↵) puts them into that workspace's agent tab box as a **context
>   chip** beside what you type; **Copy with path** beside it.
> - The agent's own explanation for an edit (from its reply) can show as a **note under the lines it changed**.

Order (yours): M11ak's box first ("box first", 2026-11-17), then M11al and M11ai (both built and pushed 2026-11-18),
then this. On 2026-11-18 you said "lets go" to the 11 points and to **B** for the case with no agent tab, then picked
**A**, note rows inside the code ("go with A").

## Why

The agent and the code sit side by side in fuxx, but asking about a piece of code today means copying it, switching to
the agent tab, pasting it and typing where it came from. The agent edits files and says why in its reply, but the reason
stays in the conversation while the change shows in the file tab with no explanation. This milestone joins the two:
point at code and ask, and read the agent's reason next to what it changed.

## Goals

1. Select lines in a file tab and, in one key or click, have them waiting as a chip in an agent tab's box, so you only
   type the question.
2. Copy code with its path and lines, ready to paste anywhere (another agent, a chat, an issue).
3. Read the agent's own reason for an edit as a note under the lines it changed, while you scroll the code.

## Non-goals and anti-goals

- **Nothing is sent without your send.** Ask only fills the box. An Ask that sends by itself is a failure, even if it
  "works".
- **Still a read-only viewer** ([[12 - Scope]]): no editing, no language servers, no services.
- No asking from the Markdown Preview, the terminal or the diff list of the right panel (only the file tab's code, in
  File and Diff modes).
- No notes for edits fuxx can only guess at (a plain `claude` in a terminal, M11af's "Guess"). Notes come only from
  agent tabs, where fuxx sees the edit and the text before it.
- No asking the agent to write notes (no prompt changes). The note is what the agent already said.
- No saved notes: they last for the fuxx session, not across a restart.
- No typing into a program running in a terminal (option A of the "no agent tab" question, turned down).

## Constraints

- M11p's rule: nothing visible changes inside render; no always-on timer. Note rows are built when the line record
  changes, not in render. Times are shown as a clock time ("10:42"), not "2 min ago", so nothing ticks.
- Provider-specific code stays in `src/providers/<agent>.rs`; generic code asks the provider.
- The public-repo rule: example paths are fuxx-cli's own (`src/workspace.rs`), never names from other projects.
- `ui-shots` scenes for every visible part; no `--update-refs`.
- Builds on: M11aa (agent tabs), M11af (`edit_ledger`, `live_lines::LineRecord`), M11ah (`RowMap` rows that are not
  code lines, Diff cards, side by side), M11ak (the box's attachments row), M11al (the card look), M11ai (the path form
  of a copied heading link).

## Approaches

**No agent tab in the workspace** (asked 2026-11-18; you said "lets go" to the pick):
- A. Type the reference into the terminal that runs `claude`, unsent. Turned down: fuxx would be typing into a program
  it doesn't control.
- **B. Open a new agent tab and put the chip there.** Picked.
- C. Grey out Ask; only Copy with path. Turned down: a dead button where the feature should be.

**How a note shows** (asked 2026-11-18; you: "go with A"):
- **A. A note row inside the code, under the changed lines**, as in the mockup. Picked: you read the reason while you
  scroll, without hunting. Costs a new kind of row in the code view's layout (`RowMap`), which selection, find,
  folding and Diff must step around.
- B. A margin mark that opens the note in a popover. Turned down: cheap, but notes stay hidden until clicked.

**Where a note's lines come from** (asked 2026-11-19; you: "go ahead"):
- Light: only edits a shown file tab sees, through M11af's compare. Turned down: a hidden tab or a file opened later gets
  no notes, which is the usual case after Ask.
- **From the edit itself:** the edit's new text is found in the file. Picked. Costs a note store and per-provider
  parsing of the edit's content; agy may only get the light way (checked in the build's first task).

**How the chip reaches the agent** (AI's call): as text, the same way today's attachments do (`wire_text` adds each
attachment after the message). A separate content block would need Claude-only code and gives the agent nothing more;
agy only takes text anyway.

## The design

### Part 1 — Ask the agent about code

**1. The selection bar.** When a selection in a file tab is finished (mouse released, or a keyboard selection),
a small bar appears just below the selection's last row, at the right end of the code's visible width:

```
 43 ▌ if let Some(tab) = pane.find(&path) {
 44 ▌   return self.show(tab);
 45 ▌ }
                                   [ Ask claude ⌘↵ ] [ Copy with path ]
 46   self.new_tab("file", path)
```

- It is an overlay (like M11af's mark tooltip, `anchored` at a window point), not a row: the code doesn't move.
- It moves with the selection as you scroll; it hides while the selection's last row is off screen, while you drag,
  on Esc (which already clears the selection), and when the selection is cleared.
- If there's no room below (the last row is at the bottom edge), it sits above the selection's first row.
- "Ask claude" names the agent it will go to: "Ask agy" for an agy tab. With no agent tab it names the agent of the
  agent tab you used last in any workspace, else Claude (AI's call: fuxx has no default agent today; the pane menu
  and ⌘K offer one "New … Tab" per agent).
- Agent tabs exist only in workspaces on this Mac. In an SSH workspace the bar shows only Copy with path, and ⌘↵ does
  nothing (AI's call: it can't work there, so it isn't offered, rather than a dead button).

**2. Ask (button or ⌘↵).** ⌘↵ is bound in the file tab's key context ("FileTab"); it is free there today (checked:
no `cmd-enter` binding anywhere). With no selection, ⌘↵ does nothing.

Ask:
1. picks the agent tab (below),
2. adds a **code chip** to its box's attachments row, beside whatever is already typed,
3. shows that agent tab and puts the cursor in its box.

Nothing is sent. The chip waits like any attachment; × removes it; several Asks add several chips (the same file and
lines twice adds one chip, not two).

**3. Which agent tab.** In the file tab's workspace:
- The agent tab you used last: fuxx starts remembering, per workspace, the last agent tab that had focus (new; there
  is no such record today).
- If that tab is gone, the first agent tab in reading order (`Model::agent_tabs`).
- Agent tabs that failed to start (no provider) are skipped.
- **None:** a new agent tab is opened in the file tab's pane, the same way the "new agent tab" action opens one, and
  the chip goes into it once its box exists (the box is made on the next frame, so the chip waits on the tab's slot
  until then). Its folder is chosen the way a new agent tab's is today (the pane's terminal, else the active
  terminal); the chip's path is relative to that folder when the file is inside it, else absolute.

If the agent is busy, the chip still goes in; sending follows M11ak's rules (held only while a restart change waits).

**4. The chip.** Label `workspace.rs:43–45` (one line: `workspace.rs:43`), with a code glyph. Its hover shows the
full path.

What the agent receives, after your message (the same place other attachments go):

````
src/workspace.rs:43-45
```rust
if let Some(tab) = pane.find(&path) {
    return self.show(tab);
}
```
````

- **Whole lines.** A selection that starts or ends mid-line takes the whole lines it touches, because the chip names
  lines.
- **The path** is relative to the agent's folder (M11ak's folder chip) when the file is inside it, else absolute.
- **The language** on the fence comes from the file's extension, the same names the code view's colouring uses; none
  if unknown.
- **A cap:** 500 lines or 64 KB, whichever comes first. Past that the Ask button reads "Too long to ask" and is
  disabled; Copy with path still works.

**5. Copy with path.** Copies the same text the agent would get, with the path in M11ai's form (repo-relative, else
`~`-relative, else absolute), since the paste goes outside fuxx. No cap.

**6. Diff mode.** A selection in Diff mode (unified or side by side) maps back to the file through each row's
current-file line number (`DiffRow.new`).
- The chip names the current file's lines the selection spans.
- The text keeps what you saw: each line with its `+`, `−` or space marker, in a ```diff fence.
- A selection of only removed lines (no current-file line) is named after the old lines: `workspace.rs (removed,
  was 12–14)`.
- Card titles and "⋯ unchanged" rows in the selection are left out of the text.

### Part 2 — The agent's reason, under its edit

**7. Catching the reason.** When an agent tab's agent edits a file, the reason is **the last text the agent wrote in
this turn before the edit started**:
- Claude: the `text` item before the `tool_use` in the same reply (`assistant()` already emits them in order), else the
  turn's last earlier text.
- agy: the last finished text step before the edit step (agy's text and tools come as separate steps).
- An edit with no text before it in the turn gets no note.
- A sub-agent's edit takes its own last text (the same `parent`).
- Only live events: a conversation replayed on restore gives no notes.
- Kept to 2,000 characters; whitespace runs collapse to one space; shown as plain text.

**7b. Which lines the edit changed — from the edit itself** (your call 2026-11-19, "go ahead" to option 2). The file
tab can't be relied on to see each edit: a hidden file tab doesn't watch the disk, and a file opened later was never
watched. So the edit's own content says where its note goes:
- Claude's `Edit` carries the old and new text; `MultiEdit` several pairs; `Write` the whole file. When the edit
  finishes successfully, fuxx reads the file (off the UI thread) and finds each new text in it. That gives the note's
  **anchor**: the new text's lines, plus where they were at that moment (a line hint).
- `replace_all` gives one note under the first match.
- A `Write` gives one note under line 1 (it replaces the whole file; there is no smaller block to point at).
- A pure deletion (empty new text) has nothing to find: its note appears only if a shown file tab sees that change on
  its own, under the line where the removed lines were.
- `NotebookEdit` gives no note.
- agy: its `edit_file` step, as fuxx reads it today, carries only the path. The build's first task checks agy's
  recordings for the new text. If it's there, agy works like Claude. If not, an agy edit gets a note only when a
  shown file tab sees that change on its own (the "light" way); when one change carries several reasons, the latest
  one goes under the first block.
- Provider-specific parsing (which tool, which fields) stays in `src/providers/<agent>.rs`; the provider hands
  generic "new texts" to the rest of fuxx.

**7c. Where notes are kept.** An app-wide note store (beside M11af's ledger), per file path: who, when, the reason,
the anchor text and the line hint. It lasts for the fuxx session (not saved), so a file opened later still gets its
notes. Capped at 200 notes per file and 2,000 files; the oldest go first.

**8. The note row.** Under the last line of the block the edit changed (for a block that only removed lines: under the
line where they were):

```
 43 ▌ if let Some(tab) = pane.find(&path) {
 44 ▌   return self.show(tab);
 45 ▌ }
    ┃ claude · 10:42 — reuse an open tab instead of opening the same file twice
    ┃ (you asked: "don't duplicate tabs").                              more
 46   self.new_tab("file", path)
```

- A new kind of row in the code view's layout (`RowMap`), beside M11ah's card titles and unchanged-stretch rows. It
  uses whole rows of the code's row height: 2 rows folded, every line of the note when opened. "more" / "less" opens
  and folds it.
- No line number; a 2 pt bar on its left in the agent's colour (M11af's mark colour); "claude · 10:42 —" in the dim
  text colour, then the note in the UI font, indented to the code's text column, wrapped to the code's visible width.
- **One note per reason per file:** when one text is followed by several edits in the same file, the note shows once,
  under the first block.
- A newer edit of the same lines with a new reason replaces the old note.

**9. The notes pill.** In the file tab's bar, beside Follow: "2 notes from claude" (agy: "from agy"; both: "2 notes").
- Click: scroll to the next note below the view (wrapping to the first).
- A small eye on the pill hides or shows all notes in this tab (not saved; a new tab shows them).
- No notes: no pill.

**10. Notes find their lines.** Each time a file tab reads the file (opening it, or a change on disk), it places the
file's notes by finding each note's anchor text in what it read, nearest the line hint, and puts the note under the
anchor's last line.
- If the anchor text isn't in the file any more (changed or removed since), the note isn't shown. It stays in the store,
  so it comes back if the text does (an undo).
- A newer note whose anchor overlaps an older note's lines hides the older one.
- Finding runs off the UI thread with the read, not in render; a file over M11af's compare limits gets no notes.

**11. Everything else steps around note rows.**
- Selection, Copy and Ask skip note rows: their text is never copied or asked.
- Find doesn't search notes. ⌥↑/⌥↓ (jump between changes) skip them.
- Folding: a note under a folded line hides with the fold.
- The sticky line and the breadcrumb ignore note rows.
- The scrollbar gets no marks for notes.
- Diff mode: notes show under the current-file line in unified Diff and across the full width under the row pair in
  side by side. Wrap on: the note wraps to the visible width like the code.
- Follow (M11af) scrolls to the edit's lines, not its note.

## Calls made in this design (AI's, for you to check)

- Ask fills the box and moves the focus there, even when the agent tab is in the same pane (the file tab goes behind
  it). The point is to type the question.
- Whole lines go to the agent, not the exact characters selected.
- 500 lines / 64 KB cap for Ask; none for Copy.
- The same file and lines twice make one chip.
- Paths: the agent's folder for Ask, M11ai's repo-relative form for Copy.
- Reasons come only from the same turn, live, up to 2,000 characters, plain text.
- The notes switch is per tab and not saved.
- Notes last for the fuxx session in an app-wide store (200 per file, 2,000 files).
- A `Write` gets one note under line 1; `replace_all` one note under the first match; a pure deletion only the light
  way; no notes for notebooks.
- A note whose text is gone stays hidden in the store and comes back if the text does.
- With no agent tab, Ask starts the agent you used last anywhere, else Claude; SSH workspaces offer only Copy with path.
- Clock time on notes, not "ago", so no timer.

## Left to the builder

- Exact sizes, gaps and colours within M11al's tokens (`ui_tokens`); the selection bar's look matches M11al's buttons.
- How the "last agent tab" is recorded (on focus of an agent tab's box or view) and where the chip waits for a new
  tab's box.
- How note rows fit into `RowMap` and hit-testing; which existing consumers of `Row` need a new arm.
- The ui-shots scenes: the bar (below / above), Ask into an existing tab, Ask with no agent tab, Diff mode selection,
  a note folded and opened, the pill, notes hidden, a note following a later edit, a note under a fold.

## Answered questions

1. **Notes for edits the file tab didn't see** (a hidden tab, or a file opened later). Asked 2026-11-19 after the
   builder check found that, as first written, notes would rarely appear in the Ask → chat → go back flow. Options: keep
   it light (only edits a shown tab sees), or work out each edit's lines from the edit itself. You: "go ahead" to the
   second (7b, 7c, 10).

## What was built

All 14 plan tasks (0–13) on `m11ag-ask-agent`; Tasks 0–12 each reviewed (7, 8 and 11 after one fix round). Task 13 (this
hand-off) goes to the whole-branch review next. Not merged.

**Part 1 — Ask the agent about code**

- **Task 1, code references:** a pure `src/code_ask.rs` turns a selection into a `CodeRef` (path, whole lines, fence
  language, body) for File and (Task 5) Diff mode; text made as plan-time call 5 says. `md_reader::shown_path` gives
  Copy its M11ai path form.
- **Task 2, the code chip:** `Attachment::Code` in the agent box (a `</>` chip, the file name and lines, a hover
  tooltip with the path), one chip for the same lines, an × to remove it; the agent's text is made at send time
  (`wire_text(.., cwd)`), the path relative to the agent's folder.
- **Task 3, Ask finds its agent tab:** `pick_agent_tab` (last used, else reading order, a failed tab skipped, a new tab
  still waiting for its view counts); none: a new tab of the agent you used last (else Claude) with the chip waiting;
  the box is focused, nothing sent. The bar's label is pushed by `tell_ask_targets` / `set_ask`.
- **Task 4, the selection bar:** under a finished selection (above it when there is no room below, inside the text when
  neither fits): "Ask claude ⌘↵" and "Copy with path", `AskAgent` (⌘↵), Esc clears the selection, "Too long to ask"
  over 500 lines / 64 KB (Copy has no cap), Copy only in an SSH workspace.
- **Task 5, Ask from Diff mode:** the current-file lines only; held `⋯` stretches, closed cards and Gap rows left out.

**Part 2 — The agent's reason, under its edit**

- **Task 6, edits carry their new text:** `EditText` on `Event::ToolStarted` for Claude's `Edit`, `MultiEdit`, `Write`
  and notebook edits (provider code stays in `src/providers/claude.rs`; agy carries none).
- **Task 7, the reason:** the agent tab keeps the turn's last text before an edit as its reason (same turn, live,
  at most 2,000 characters, whitespace collapsed); cleared when a turn ends any way (force stop, exit).
- **Task 8, the note store:** `src/edit_notes.rs`, app-wide, 200 notes per file, 2,000 files, oldest out first; at an
  edit's finish the workspace reads the file off the UI thread, finds where the new text sits and files the note (a
  `Light` reason when it cannot).
- **Task 9, notes find their lines:** each file tab places its file's notes in the text it shows, off the UI thread
  after each read; nearest the hint wins when the text appears twice; moved with later edits; a note whose text is
  gone stays hidden and comes back with it; the light way claims the newest `Light` by an exact agent stamp.
- **Task 10, note rows in the row map:** `Row::Note` rows in File and both Diff layouts; every consumer of `Row`
  steps around them (sticky line, breadcrumb, scrollbar marks, selection).
- **Task 11, note rows drawn:** a bar in the agent's colour, `claude · 10:42 — ` in dim text, the reason as raw
  characters, wrapped at the code's visible width, two rows folded and all open, "more" / "less" when over two; an
  opened note whose last words would run under "less" gets one more row for it (fix round).
- **Task 12, the pill and the eye:** "1 note from claude" / "2 notes from claude" / "2 notes" in the bar, a click
  steps to the next note, the eye hides or shows them (per tab, not saved).

**M11p check (Task 13 Step 1).** The branch adds no `timer(` and no `request_animation_frame`. Its four `spawn(`s are
two one-shot background jobs, each followed by one `cx.spawn` that swaps the answer in: the placing of a file's notes
after a read (`file_tab.rs`) and the read of an edited file at an edit's finish (`workspace_agent.rs`). Nothing loops,
nothing ticks (note times are clock strings).

## Calls made while building (AI's, for you to check)

Plan-time calls (made reading the code; the plan holds them in full):

1. agy carries no edit text (all three recordings checked at plan time and again in Task 0), so an agy edit gets a note
   the light way.
2. There is no keyboard selection in the file tab, so the bar shows when a mouse selection ends and is not empty.
3. Esc did not clear a selection; now, with no find bar or outline panel open, it does (with no selection it still
   goes on).
4. The chip keeps the absolute path; the agent's text is made at send time, so a folder changed between Ask and send is
   honoured. The lines are taken at Ask.
5. The agent's text: a header line (`path:43-45`, `path:43`, `path (removed, was 12-14)`) then a fence; the chip label
   uses the file name and an en dash; diff bodies use ASCII `+` / `-`; `\r` dropped, tabs kept.
6. The fence language is the colouring's syntax name lowercased when one plain word, else the file's extension, else
   none.
7. File mode takes the hidden lines of a fold inside the selection (as ⌘C does); Diff mode leaves out folded stretches,
   closed cards and Gap rows.
8. A new agent tab still waiting for its view is a target, so two quick Asks fill one new tab.
9. The bar's label is pushed from the workspace on every focus change, not polled.
10. The last agent tab is remembered per workspace, its provider app-wide; not saved.
11. An edit's new text is looked for as its first copy in the file read at the finish; the note anchors to the whole
    lines it spans; not found, no note.
12. One note per reason per file, shown under the topmost placed block.
13. The light way (pure deletions, agy): a pending `Light` reason lasts 600 s and is taken by a file tab whose live
    record stamps a change exactly to that agent; a deletion's note goes on the line above the first deleted edge.
14. Notes are placed for any text that is not `plain`; placement gives up after 50 ms (no notes for that read).
15. A note's rows: two folded, all open, from the wrapped line count of `claude · 10:42 — reason` in the UI font at the
    code's visible width; "more" / "less" only over two lines.
16. In Diff mode a note shows under the diff row for its line; none when the neighbourhood has only removed lines.
17. The pill counts the notes placed (shown or hidden by the eye), in File and Diff modes, not Preview; "next note"
    is the first below the view, else the first.
18. The exact words: `Ask claude ⌘↵`, `Too long to ask`, `Copy with path`, `more`, `less`, `1 note from claude`,
    `2 notes from claude`, `2 notes`, `Hide notes`, `Show notes`, the chip glyph `</>`, the note prefix `claude · 10:42 — `.
19. The clock is made once when the note is filed and kept as a string.

Calls made in the build (the controller's rulings, each with the reason in the build ledger):

- **Task 0 skipped the full ui-shots run:** your rule is the full run only before the final review and after its
  fix wave, so Task 0's baseline for scenes is M11ai's last full run (226 passed, 0 failed, 151.4 s).
- **Plan ticks wait for Task 13:** the plan's status table, ticks and "Pick up here" are updated here, not per task.
- **Names:** the file tab's target is `ask_to` (probe `ask_to()`), the action is `ask`; `BAR_GAP` (6 pt) is the gap
  between the selection and the bar, the bar's height is M11al's text-button token. Between Tasks 4 and 5 the bar could
  show in Diff mode with Ask / Copy doing nothing; Task 5 filled it in.
- **No reference fingerprints for the new scenes:** a new Visual scene needs none (ui-shots only reports "without a
  reference"); `--update-refs` was never run and nothing was added under `tests/ui-shots-ref/`.
- **Every `#[allow(dead_code)]`** added for a task is removed by the task that wires it.
- **The 500-line cap counts lines by `split('\n')`,** so a 501-line body that ends blank is over the cap (Task 4).
- **⌘↵ is Unlisted in the help list:** a help row needs a palette command, which the plan does not include
  ([[12 - Scope]], rule 3). Say so if you want one.
- **The bar's look** (flush right with the code's edge, 24 pt) is left to your on-screen check.
- **A disabled "Too long to ask"** is a variant of the shared text button; Esc during a held drag does not clear the
  selection (Task 5).
- **A reason is cleared when any turn ends** (TurnDone, a force stop, an exit, a failed start), so an old reason never
  lands on the next turn's edit (Task 7's fix).
- **Notes are looked for in a file's text with the byte-safe cut** (Task 8's fix: a new text ending in a multi-byte
  character crashed the first version).
- **A removal-only change is decided by the ledger** (who), not "you", so an agent's deletion is stamped to it.
- **"A note under a folded line hides with the fold":** a note whose line is inside a fold is hidden; a note under the
  fold's shown header stays.
- **Notes wrap at the visible width, Wrap on or off** (call 15 and the spec); a narrower window gives more note lines.
- **An opened note whose last line would run under "less" gets its own row for it** (Task 11's fix); otherwise it takes
  exactly its wrapped lines.

## Limits (what it does not do)

- An `Edit` whose new text also appears earlier in the file anchors its note to the first copy.
- agy edits and pure deletions get notes only through a file tab that is showing the file when the edit lands (the
  light way); an edit read by no shown tab gets no note for those.
- Notes are gone after a restart (the store is in memory).
- There is no keyboard selection in the file tab, so the bar follows the mouse only.
- Two edits by one agent to one file within about 2 s, while the first is still read open: the second's reason can
  land on the first's block (the light way takes the newest).
- A light reason can be missed when the tab reads the change in the few milliseconds between the ledger finishing and
  the store holding it; it then waits for that agent's next exact change.
- A note under a fold counts in the pill but the pill's click skips it: with every note folded the click does nothing
  while the pill says "1 note".
- Note rows when the code is wrapped or the window is resized can snap the scroll to the top of a row (up to one row's
  jump).
- A turn you stop (Stop) clears the pending reasons, so an interrupted turn that keeps editing gets no note.
- No notes for notebooks, and none in Preview; in Diff mode none where only removed lines surround.
- Ask works from a mouse selection that is scrolled off screen too (the bar is hidden then, ⌘↵ still asks).

## Final numbers

Before the final review (Task 13, 2026-11-19):

- `cargo test --bin fuxx`: **2,146 passed, 0 failed, 1 ignored**.
- Clippy: default **14 / 27** warnings (bin / test), `ui-shots` **16 / 29** — the baselines, no new warnings.
  `cargo fmt --check`: clean.
- Full `scripts/ui-shots.sh`: **247 passed, 0 failed, 158.9 s** (Task 0's baseline was M11ai's 226 / 0 / 151.4 s; the
  21 more are this milestone's scenes). Fingerprints: `0 same, 339 look different, 52 differ in size, 226 without a
  reference` (the known stale-reference state of this Mac, not a branch effect; reported, never a failure).
- Public-repo grep of the branch's added lines: no personal paths, names, emails or time zones in code, scenes or docs
  (the only matches are the rule's own wording in the plan and its context).

After the final review's fix wave: to be filled in by the controller (the full run again, then the same numbers).

## On-screen check (for you; 15 items)

Run it as `FUXX_DAEMON_SOCKET=/tmp/fuxx-dev.sock cargo run` (debug only, never `--release`), on fuxx-cli's own files such
as `src/workspace.rs`, with a Claude agent tab open in the same workspace.

1. **The bar, below and above:** drag over a few lines in `src/workspace.rs`; the bar sits under the selection; select
   the last visible line (or scroll so it is) and it moves above or inside. Its look: flush at the code's right edge,
   buttons like the app's other text buttons.
2. **Ask into the last-used tab:** with ⌘↵ and again with the button: the agent tab's box takes focus with a `</>`
   chip, nothing is sent; a second Ask of the same lines makes no second chip; × removes the chip.
3. **A chip's hover tooltip** shows the file path and lines.
4. **Ask with no agent tab:** close every agent tab, Ask: a new tab opens with the chip waiting in its box.
5. **Copy with path:** press it and paste into a text editor: the path (repo-relative), the lines, no more.
6. **Too long:** select over 500 lines: the bar says "Too long to ask" (disabled); Copy with path still works.
7. **Diff mode**, unified and side by side: select lines; the bar shows; Ask or Copy takes the current file's lines
   (removed lines come with `-`, added with `+`).
8. **An SSH workspace:** the bar offers only Copy with path.
9. **Help list:** ⌘↵ is not in it (Unlisted, by design); Esc clears the selection and hides the bar.
10. **A note:** ask Claude to edit `src/workspace.rs` (a small change) and read the note under the change: `claude ·
    10:42 — its reason`, folded to two rows with "more", "less" when opened; an opened note whose last line would run
    under "less" gets its own row for it.
11. **The selection band across note rows:** drag a selection over a note: the band runs unbroken across its rows
    and Copy does not take the note's words.
12. **The note after a later edit:** have the agent edit above the note; the note follows its block.
13. **The pill and the eye:** with notes below the view, the pill reads "2 notes from claude" (or as many) and steps to the
    next; the eye hides them and shows them again; the pill stays while hidden.
14. **A fold over a note:** fold the block above a note; the note goes with the fold and comes back when unfolded.
15. **Idle CPU** with a file tab showing notes: `top -l 6 -s 5 -pid <pid>` for the branch build and again for master;
    the two should match (note rows are built only when the notes or the lines change).

## Related

[[Proposal - File View Upgrades]] · [[82 - Milestone 11af - Live Changes]] · [[83 - Milestone 11ak - Agent Box]] ·
[[85 - Milestone 11ah - Code View Upgrade]] · [[86 - Milestone 11ai - Markdown Reader Upgrade]] · [[12 - Scope]]
