---
tags: [milestone, m11an, placeholder]
status: placeholder
---

# Milestone 11an - Full-width typing line (placeholder)

Added 2026-11-21 on your words: "if this is the best way, create a milestone, we'll come back later for this. give me
more detail. What we gain, what we lose when we build this". **Not designed, not planned.** When we come back: a
design talk first (the choice below is yours), then a plan.

## The problem

In a shell tab's composer the first line of what you type is short; the lines after it are full width (your
screenshot, 2026-11-21: `❯ sdfsdfs fsdf sdf …` stopping halfway, the next lines running to the edge).

Why: the composer is a window onto the terminal's grid. zsh writes your prompt first (`~/K/p/fuxx-cli ╱ master ▓▒░`,
about half a line), your typing right after it on the same row, and wraps to the next row when the row is full. The
composer covers the prompt and shows `❯` instead (M11ab rework), but the room the prompt takes on that row is still
gone. The rows after it hold only typing, so they're full. The first line's length also jumps with the prompt: a
longer folder or branch name, a shorter first line.

fuxx can't move zsh's line breaks: zsh decides them, measuring from its own prompt, and moving text in the grid would
make zsh lose track of its cursor.

## Two ways to fix it

### A. Re-lay the typing out for display (what we talked about first)

Leave the grid alone and draw the typing afresh: read the typed part of the first row and the rows wrapped after it
(one soft-wrapped line in the grid) and lay it out at the box's full width, `❯ ` then text across the whole first
line, full lines under it. Lines that aren't part of it (a completion list under the command) stay as they are.

### B. A short prompt from the shell itself, in card view only

fuxx's zsh integration (`assets/shell-integration/zsh/fuxx-integration.zsh`) already edits `PS1` to add its marks.
In a tab shown as cards it could also make the prompt short (just `❯ `, the folder and branch already being in the
composer's pills), so zsh itself wraps at the full width. Nothing to translate: the grid and the box agree again.

## What we gain (either way)

- **Full lines from the first one**: a long command reads like a message box, in fewer lines.
- **A steady layout**: the first line no longer shrinks or grows with the folder or branch name.
- **Less wasted room**: on a narrow split the first line can be almost nothing today.

## What we lose, or pay

### With A (display re-layout)

- **A translation layer** between "where it looks" and "where it really is", used by everything that touches the
  typing line: the cursor, mouse clicks and drag-selection, double-click on a word, copy, ⌘-click on a link, the
  input method's (IME) candidate window position, Rerun's typed text. Each one is a place for an off-by-one.
- **Edge cases to get right and test**: wide characters (CJK, emoji: two cells) at a line's end; zsh's syntax
  colours and the autosuggestion's grey text carried with their letters; the right prompt (already cut today) near
  the typing; zsh redrawing the line (Tab completion, ⌃R search, vi mode); a command over several lines (`\`
  continuation, here-documents with their `> ` prompt); resizing while typing.
- **No longer a 1:1 view** of the terminal on that line: a bug shows as a misplaced cursor or a selection that grabs
  the wrong letters, the kind of thing that erodes trust in a terminal.
- **Size**: a few hours plus tests; every later change to the composer must keep the mapping right.

### With B (short prompt in card view)

- **Your prompt's look changes in card-view tabs**: Powerlevel10k's segments (folder, git status `!1`, time, exit
  ✔) aren't drawn by zsh there; the composer's pills carry the folder and branch, but not p10k's extras (dirty
  marks, time, last exit) unless the pills learn them.
- **Switching card view off in an open tab** shows the short prompt until the next prompt is drawn (or a new shell);
  the integration needs to know, per tab, which look to use (an environment variable at spawn, or a message from
  fuxx to the shell when the setting changes).
- **Custom prompts and themes** other than p10k need the same handling; a prompt that rebuilds itself every time
  (p10k does) has to be caught where it's built, as the marks already are.
- **Smaller and exact**: no translation, the cursor and selection stay true; most of the work is in the shell
  integration and its tests.

## Open questions (for the design talk)

1. A or B? (B looks simpler and exact; A keeps your prompt untouched in the grid.)
2. With B: which p10k extras should the pills show (dirty state, last exit, time)?
3. bash and fish shells: card view is zsh-only today; stays that way?

## Related

[[88 - Milestone 11ab - Chat-Style Shell Tabs]] · [[40 - Roadmap]] · [[12 - Scope]]
