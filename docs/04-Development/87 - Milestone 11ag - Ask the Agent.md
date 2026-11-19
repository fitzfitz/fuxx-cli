# Milestone 11ag — Ask the Agent from the Code

**Status:** 📝 designed and **approved 2026-11-19** ("approved"); plan next. Not built.

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

## Related

[[Proposal - File View Upgrades]] · [[82 - Milestone 11af - Live Changes]] · [[83 - Milestone 11ak - Agent Box]] ·
[[85 - Milestone 11ah - Code View Upgrade]] · [[86 - Milestone 11ai - Markdown Reader Upgrade]] · [[12 - Scope]]
