---
tags: [milestone, design]
status: built
---

# Milestone 11ab - Chat-Style Shell Tabs (design)

Designed with you 2026-11-20 in browser mockups (`.superpowers/brainstorm/…/shell-tab-directions.html`,
`running-command.html`). Part 2 of [[Proposal - Chat View and Agent Tabs]]; built on M11l's command marks
([[63 - Milestone 11l - Command Blocks]]).

## Goal

A zsh shell tab reads like a conversation: every finished command is a **message card** (when, how long, how it
ended, the command, its output), and the bottom of the pane is the **live box**: the real terminal, holding your
prompt or the command that is running right now. When a command ends it moves up as a card.

## Why (your words and choices)

- The idea (2026-11-13): "turn this terminal that basically our core to be a chat like UI terminal."
- 2026-11-20: "let's go with M11ab", ahead of M11ad and M11am (**order changed by you**; the 2026-11-19 order had
  M11ab after M11am so shell tabs could borrow the reworked agent tab's look. M11ab now sets its own look, and
  M11am can follow it).
- Mockups, 2026-11-20: **A, message cards** (over B, quiet blocks in the flow, and C, chat bubbles); **R2, a running
  command lives in the live box and becomes a card when it ends** (over R1, live inside its card); the live box
  **grows to the full pane** when a command needs it ("go with your proposal, grow to full pane").
- How it's built: **approach 1, cards are views onto the grid** ("go with 1").

## Goals

1. Finished commands as cards: header (start time · duration · `exit N`), the command, the output in a panel; a
   failed command's panel outlined in red; long output folded; Copy and Rerun.
2. The live box at the bottom: your prompt, or the running command, drawn and typed into exactly as today.
3. zsh stays whole: completion, ⌃R, autosuggestions, vi mode, your prompt theme, right prompts.
4. Full-screen programs take the whole pane; the cards come back when they exit.
5. A setting turns it off: the plain terminal is one switch away.

## Non-goals

- bash, fish, SSH remote shells, tmux, `zsh -f`, or zsh with `command-blocks = off`: no marks, so today's terminal.
- Agent tabs (M11aa) and their rework (M11am); M11ad's background agents.
- A fuxx-owned text box for typing commands.
- Selecting across several cards; search; filters; sharing a card; "ask the agent about this card".
- Exposing cards to the scripting API, MCP or `fuxx text`.

## Anti-goals (a failure even if it works)

- **The terminal gets worse.** Anything that runs in the live box (prompts, progress bars, `npm init`'s questions,
  `claude` typed in a shell, which draws inline) behaves exactly as today. With the setting off, nothing changes.
- **zsh loses a feature** (the line editor is zsh's, never fuxx's).
- **A card shows something other than what the terminal printed** (colours, wide characters, links).
- **Typing feels slower,** or a long scrollback makes the pane stutter.

## Approaches considered (you picked 1)

1. **Cards are views onto the grid (chosen).** One alacritty grid per tab, as today. A card paints its command's
   output lines with today's line painter (`grid.rs` `bg_rects` / `text_spans`); folding skips lines; the live box
   paints the current block's rows. One source of truth: background sessions, restore and resize work as now.
   Cost: a map from card rows back to grid lines for selection, links and clicks, over finished (static) lines only.
2. **Cards copy their output out at the end mark** into their own styled text. Survives scrollback trimming and
   re-wraps freely, but a second copy of every output, selection and links rebuilt for styled text, and restored
   sessions would still have to build cards from the grid.
3. **A little terminal per command,** the shell's bytes split at the marks into separate grids. Each card wraps at
   its own width, but every byte is parsed twice, two grids can disagree, and it's the largest build.

## The design

### 1. What the pane shows (card view)

The pane is one scrolling column, oldest at the top:

```
┌ pane ───────────────────────────────────────────────────┐
│ (earlier output: rows that belong to no command)        │
│                                                         │
│ 10:41 · 0.04s                                           │
│ ❯ git status -s                                         │
│ ┌─────────────────────────────────────────────────────┐ │
│ │ ?? docs/plan.md                                     │ │
│ └─────────────────────────────────────────────────────┘ │
│ 10:42 · 3.2s · exit 1                                   │
│ ❯ npm test                                              │
│ ┌ (red outline) ──────────────────────────────────────┐ │
│ │ ▸ 9 earlier lines                                   │ │
│ │ FAIL  src/rsvp.test.ts …                            │ │
│ └─────────────────────────────────────────────────────┘ │
│                                       Copy   Rerun      │
│ ─────────────────────────────────────────────────────── │
│ ~/code/api   ⎇ main ?1                         (chips)  │
│ ┌ live box ───────────────────────────────────────────┐ │
│ │ ~/code/api main ❯ █                                 │ │
│ └─────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────┘
```

- **Earlier output:** grid rows above the first tagged command (a login message, output from before the
  integration loaded, a restored session started by an older fuxx) are shown as plain rows, no card.
- **Cards:** one per finished command (`done` tag), in grid order.
- **The live box:** every row after the newest card's last row, down to the lower of the cursor's row and the last
  non-blank row (so a zsh menu or autosuggestion under the prompt is inside it; the blank rows under it are not).
  At a prompt, that is your prompt (and anything typed). While a command runs, it is that command's prompt,
  command line and output so far, drawn exactly as today, cursor and all. With no card at all (a new tab, after
  `clear`, after a program like Claude Code clears the scrollback and its tags), every row is the live box.
- **The live box grows with its content, up to the whole pane** (your call). When it fills the pane, the cards are
  above it, out of view; scrolling up reaches them. Nothing scrolls inside the box itself: the column scrolls.
- **The column sits at the bottom, like a chat:** when everything is shorter than the pane, the space is at the top.
- **Stuck to the bottom** like a terminal: new output keeps the bottom in view unless you've scrolled up; typing
  snaps back to the bottom (as today).
- **When a command ends** (its D mark), its rows leave the live box and become the newest card; the box shrinks back
  to the next prompt.

### 2. A card

- **Header** (UI font, `text::META`, `text_faint`): the start time (`HH:MM`, the clock fuxx already uses), ` · `
  the duration (the same short form as M11q's "Done in 3m 12s": `0.04s`, `3.2s`, `3m 12s`), and ` · exit N` in
  `block_failed` when not 0. A part the tag doesn't carry is left out (an older tag: no time, no duration).
- **Command line:** `❯ ` then the tag's `cmd` in the terminal font (all its lines; a multi-line command keeps its
  lines). A tag with no `cmd` (`t=1`, too long) shows the command row as the terminal printed it.
- **Output panel:** the output's grid lines, painted by today's line painter at the terminal font, inside a panel
  (radius 8, 1 pt edge, 10 pt inner padding). No output: no panel.
- **Failed** (exit not 0): the panel's edge in `block_failed`.
- **Folding (AI's call: the end of the output is what matters most — test failures, build errors, summaries):** an
  output longer than **20** lines shows its **last 12**, with `▸ N earlier lines` as the panel's first row; clicking
  it unfolds the whole output (`▾ Show less` then folds it back). Remembered per card while the tab is open.
- **Actions (AI's call: shown on hover, so a long history stays calm):** under the panel at the right, `Copy` (the
  output, as M11l's Copy output) and `Rerun` (M11l's rule, unchanged: **types the command at the prompt, never
  presses Return**, and only when the prompt is empty). The fold row is always visible when folded.
- **Chosen card** (⇧⌘↑ / ⇧⌘↓, as M11l's chosen block): its panel edge in the accent; ⇧⌘C copies its output.

### 3. The chips row above the live box

- The working folder and the branch (with its `?N` / `+N` counts), from what the status bar already knows.
- **While a command runs:** first a chip `● <command> · <elapsed>` in `block_running` (the command's short form,
  M11q's `command_name`), the elapsed time ticking once a second.

### 4. Width: one column for cards and the live box

- Cards and the live box share one content width: the pane less its padding, less the panel's edge and inner
  padding. **The terminal's column count is taken from that width**, so a line wraps at the same place in a card
  and in the box, and the shell sees the width it is drawn at.
- Switching the setting, or entering / leaving the alternate screen, changes the width: the grid is resized and
  reflows as it does on a pane resize today (M11q's prompt-redraw rule applies).

### 5. The full-screen case

- On the alternate screen (vim, htop, less, tmux, a full-screen TUI) the whole pane is today's terminal view, at the
  pane's full width. When the program leaves the alternate screen, card view returns.
- **Inline programs** (Claude Code typed in a shell draws on the main screen; `npm run dev`; `tail -f`) are a
  running command: they live in the live box, which grows to the whole pane. While they run, the pane looks like
  today's terminal.

### 6. Mouse and keys

- **Typing** always goes to the terminal, as today, wherever the pointer is.
- **The live box** behaves exactly as today's terminal: selection, links, mouse reports to programs, the wheel when
  a program asked for it.
- **In a card:** drag to select within its output (one card at a time; AI's call); copy (⌘C) gives the selected
  cells' text as today's copy does; ⌘-click opens links as today. No mouse reports (the command has ended).
- **The wheel** scrolls the column; a program in the live box that asked for the wheel gets it while the pointer is
  over the box.
- **⇧⌘↑ / ⇧⌘↓** move between cards (scrolling the chosen one into view); past the newest they clear the choice and
  scroll to the bottom. **⇧⌘C** copies the chosen card's output (or the newest's). M11l's palette commands keep
  working.

### 7. M11l's gutter bars and hover chip

In card view they are not drawn: the card carries the outcome and the actions. In plain view (setting off, or no
marks) M11l's blocks look and behave exactly as today.

### 8. The tag gains two optional fields

- `t0` (the start time, Unix seconds, set at the C mark) and `ms` (the duration in milliseconds, set at the D mark)
  in the `fuxx-block:` tag. Optional: an older tag reads as before and its card shows no time or duration. The
  service (persistent sessions) and the window tag alike, as for the other fields.

### 9. The setting

- **`shell-cards = on | off`, default `on`** (AI's call: it's your idea and the plain view is one switch away). In
  **Preferences → Sessions**, under Command blocks: `Commands as cards` / sub-line `Shows each finished command as a
  card. Needs Command blocks.` Applies at once to open tabs (it changes only how the grid is drawn).
- With `command-blocks = off` there are no marks, so there is nothing to card: the switch shows as off and can't be
  turned on until Command blocks is on.

### 10. Unchanged

The agent-state detector, notifications, the sidebar's status, Activity, M11q's command names and banners, the
scripting API / MCP / CLI, agent tabs, file and browser tabs, splits, drag and drop of tabs.

## Hard cases

| Case | What happens |
|---|---|
| A command that never ends (`npm run dev`, `claude`) | Stays in the live box (up to the whole pane) until it ends; no card yet. |
| Huge output | The card folds it; only visible lines are painted. Past the 10,000-line scrollback, the oldest cards' rows go and their cards with them (as M11l's blocks). |
| `clear`, `reset`, a scrollback clear | Rows dropped from the grid take their cards with them; `ESC [2J` pushes the screen into scrollback, so the cards stay. |
| Output arriving after its D mark (a background job prints later) | Rows after a card and before the next prompt: they belong to no command and show as plain rows between cards. |
| Duplicate or faked marks (`cat` of a file with marks) | As M11l: a fake card can appear; Rerun still never presses Return and refuses control characters. |
| Resize | The grid reflows as today; cards are read again from the tags (they move with their cells). |
| Persistent sessions (M10a) | Same grid, same tags: cards appear after reattaching. A service without the new tag fields gives cards without times. |
| A split pane too narrow for the panel's padding | Under a minimum width (left to the builder, about 260 pt), the pane uses plain view. |
| A theme change | Cards repaint in the new colours, as the terminal does. |

## Constraints

- **Built on M11l's marks and tags** (`blocks.rs`): no new way of finding commands.
- **No full-scrollback walk per frame:** `blocks::list` reads the whole scrollback; the card view keeps its block
  list and updates it when a mark arrives or the grid is cleared or resized (off the UI thread where it must walk).
- **Only visible cards are painted** (a long history stays smooth).
- Colours from the theme's tokens (`ui_tokens.rs`, `UiColors`), held to the contrast floors M11 set: header text on
  the pane at `TEXT_MIN`, `block_failed` / `block_running` / accent edges at `MARK_MIN`.
- Provider-specific code stays in `src/providers/` (none is expected here).
- Tests: the grid ↔ card map, folding, width, the tag fields and the setting are plain data and unit-tested; the
  look is checked by screenshot scenes (`scripts/ui-shots.sh`), both themes.

## Settled after the builder's read (AI's calls, 2026-11-20)

A fresh reviewer read this note against the code as the person who'd plan it and raised six points; each is
settled here as the AI's call, for you to check.

1. **What turns card view on: the tab, not the tags.** A tab is in card view when its zsh was started with fuxx's
   integration (Command blocks on at its start) and `shell-cards` is on. It never flips on whether tags happen to be
   in the grid right now, so the width doesn't change back and forth (`exec bash` in a card-view tab stays in card
   view; its rows just stop becoming cards). Only the setting and the alternate screen switch it.
2. **No tag for the current command** (before the first prompt, after `reset`/`clear`, Claude Code clearing the
   scrollback mid-run, a running command whose tag scrolled past the cap): no special case. The live box is every
   row after the newest card (§1), so the pane is one big live box at card width, as today's terminal in its frame,
   until a command ends with a tag and becomes a card. A D mark that finds no tag makes no card.
3. **The box's height and the column's anchor:** §1 (to the cursor or the last non-blank row, whichever is lower;
   the column bottom-anchored). A program that moves the cursor above the box's first row writes into grid rows a
   card shows; the card then shows what is in the grid (a card is a view onto the grid, never a copy). Rare and
   accepted.
4. **The live box's frame and the chips row always stay in card view,** also when the box fills the pane (Claude
   Code typed in a shell, `npm run dev`). **The terminal's rows are the height inside the box at its tallest** (the
   pane less the chips row and the frame), and its columns the width inside (§4), so the size the program sees only
   changes with the pane, the setting or the alternate screen. While such a program runs, the pane is today's
   terminal inside the frame, with the running chip above it.
5. **A prompt with no command** (an empty Return, or ⌃C on a half-typed line) leaves no card and no rows in the
   column: its rows are hidden. The plain view (setting off) still shows them as today.
6. **M11aa's "drag a command onto an agent tab" keeps working:** in card view the drag starts from a card's
   `❯ command` line (the same drag the M11l chip's command text starts today).

## Left to the builder

- The exact module split (a `card_view.rs` beside `terminal_view.rs` is the expected shape) and how the column is
  laid out in GPUI (a `list` of segments, or one custom element).
- Exact sizes and colours (panel fill, gaps between cards, the chips' look) from the existing tokens.
- The narrow-pane threshold.
- How the block list is kept up to date incrementally.

## M11ab as built (2026-11-21)

Branch `m11ab-shell-cards`, 7 tasks built in the session (executing-plans), then one whole-branch review (Opus) and one
fix pass. Plan: [[M11ab Plan - Chat-Style Shell Tabs]].

- **Tags** carry `t0` (start, Unix seconds) and `ms` (duration); `blocks::segments` lists every command and empty prompt.
- **`card_view.rs`** (plain data, tested): the column (`column`: earlier rows, cards, loose rows, the live box), folding
  (20 → last 12), the header (`10:42 · 3.2s · exit 1`), sizes (`card_dims`: the terminal is the box's inside), the
  segment cache (rows from the scrollback's top; a rotation at the cap found by the newest card's tag, no full read),
  the layout, the rows in view, pointing (`card_point`), the chips, the scroll (bottom-anchored, held while scrolled
  up, page keys).
- **`terminal_view.rs`** paints the column on one canvas (`paint_cards`), each card's output with today's line
  painter; the live box with today's grid painter and cursor; Copy / Rerun on hover at the header's right, the fold
  row, and the command line as M11aa's drag handle are real elements placed from the last frame; selection, ⌘-click
  links and the program's mouse reports run through today's code (`point_at` maps the pointer through the rows
  painted), a selection held to one card.
- **Setting:** `shell-cards = on | off` (default on), Preferences → Sessions → Terminal, "Commands as cards", disabled
  while Command blocks is off.
- **Who gets cards:** a shell this window spawned with fuxx's marks; a background-service session once it shows a
  real mark, or a tag in its grid at reattach (never a bash session).
- **Checks:** 2274 tests; clippy at the pre-M11ab baseline; fmt; ui-shots 6 new scenes (`shell-cards`, `-select`,
  `-running-full`, `-alt-screen`, `-off`, `-toggle`), both themes.

### Rulings (the AI's, for your check)

From the plan: no `?N` / `+N` counts on the branch chip; `card_duration` (`0.04s`, `3.2s`, `3m 12s`); no duration for a
command started before a reattach. While building: one canvas instead of `gpui::list`; a segment cache (measured: a full
read is 333 µs over 10,040 lines); Copy / Rerun at the header's right on hover, not under the panel; the running chip in
whole seconds; the branch pushed on the workspace's 1 s folder check; a running command's rows that scrolled off the
screen show as plain rows above the box (so the chips and frame stay on screen, Settled 4); ⌘-hover's underline in
card view left for later (⌘-click opens).

### Deferred (minor)

Stale M11l gutter hits after a plain frame; no disabled look on card Copy / Rerun; cursor sparks off in card view; a
card from before a DST change shows its time an hour off; a rotation with only prompt tags (after `exec bash`) isn't
noticed; a drag that scrolls the column loses its region.

## On-screen checklist (yours)

With `FUXX_DAEMON_SOCKET=/tmp/fuxx-dev.sock cargo run`, a new zsh tab:

1. Run `ls`, `git status`, `false`: each becomes a card (time · duration · `exit 1` in red for `false`), the prompt
   in the box at the bottom with your folder and branch above it.
2. Press Return on an empty prompt, and ⌃C on a half-typed line: no card, nothing left between the cards.
3. `seq 1 40`: folded to its last 12 with `▸ 28 earlier lines`; click it to unfold, `▾ Show less` to fold back.
4. Hover a card: Copy and Rerun at its header's right; Rerun types the command at the prompt (never runs it).
5. Drag across a card's output: it selects, ⌘C copies; ⌘-click a link in a card opens it.
6. `npm run dev` (or `ping localhost`): the box grows to the whole pane, the running chip ticks; scroll up while it
   prints: what you read stays put.
7. `claude` typed in the tab: it runs inside the box as today's terminal; quit it: its run becomes a card.
8. `vim`: the whole pane, as today; quit: the cards are back.
9. ⇧⌘↑ / ⇧⌘↓ move between cards (the chosen one's edge in the accent); Shift+Page Up / Down scroll.
10. Drag a card's command line onto an agent tab: it attaches, as M11aa's chip did.
11. Preferences → Sessions → Commands as cards off: today's terminal at once; on again: cards.
12. A bash tab (or with Command blocks off): today's terminal, unchanged.

## Rework (2026-11-21, after your first look)

Your words, on screenshots of the build with your Powerlevel10k prompt: "Very ugly". The framed box took columns, so
your two-sided prompt wrapped; its second half leaked between cards; a new tab was a tiny box under a blank pane; the
branch showed twice. Redesigned with you in browser mockups (`card-rework-*.html`): you chose **D2 + D3 combined**
("Combine D2 and D3"), then the live prompt as a **composer** like your reference ("Pay attention to details, the
border, background, outer border, gap between the inner box and the outer border"), then **your prompt left out of
it** ("now that we already have folder and git information, we don't need this"); "let's try this. make sure for
icons, we use the icons that we already used".

1. **Cards (D2 + D3):** each finished command on one soft surface (a shade above the pane, radius 8, no border); a
   **rail** down the left in the pane's padding (no terminal columns) with a dot per card beside its header: green
   done, red failed, the accent ring around a chosen card's dot. No red outline: the dot and `exit N` carry a failure.
   A card covers all its command's rows (a wrapped command line's second row too), so nothing is left between cards.
2. **The composer** at the pane's bottom, measured from your reference: an **outer box** a shade above the pane (1 pt
   edge a little lighter, radius 16) holding the **context row** (the running chip first, peach `● cmd · 1m 04s`;
   then `Folder` icon + folder `ChevronDown`; then `Branch` icon + branch `ChevronDown`, the app's Lucide icons) and,
   5 pt in from its sides and bottom, the **inner box** a shade above that (1 pt edge, lightest along its top,
   radius 12, 14 pt padding).
3. **In the inner box only what you type:** your prompt's rows are left out (from the mark where the prompt ends:
   the typing starts at B's column; a multi-line prompt's upper rows go too) and its right-hand part (the run after
   two blank cells, as M11l's chip finds it). A `❯` in the accent leads; empty, a quiet `Type a command`. Completion
   menus and suggestions under the line still show. **A running command:** only its output (its command is in the
   chip), the box growing up to the pane. A session reattached before its next prompt shows the row as the terminal
   has it until the next prompt.
4. **The terminal's size** is the inner box's inside at its tallest (pane less padding, the outer box's edge and
   inset, the inner box's edge and padding, the context row).
5. **Unchanged from the build:** bottom-anchored column, folding, Copy / Rerun on hover, ⇧⌘↑/↓, drag onto an agent
   tab, the alternate screen in today's view, `shell-cards`.

**AI's calls:** the context row's ▾s act: the branch opens M11o's branch switcher, the folder shows it in Finder
(you didn't say; plain labels if you'd rather); the composer's shades are new theme colours stepped from the
terminal background, held to the text floor; the chosen card shows by an accent ring on its dot and an accent edge
on its surface.

### Rework as built (2026-11-21)

Branch `m11ab-rework`, 4 tasks in the session (executing-plans), one whole-branch review (Opus), one fix pass. Plan:
[[M11ab Rework Plan - Composer and Rail]].

- **`card_view.rs`:** cards keep all their command's rows; the live box runs from where the typing starts (or a
  running command's output); `crop` (what of the typing row shows: from B's column, the right-hand part cut);
  the composer's sizes (`COMPOSER_FRAME`, `card_dims`).
- **The prompt's tag records where the typing starts** (`br`, `bc`: rows below the tag's row and B's column), so it
  moves with the text: a prompt on the screen's last row with a list printed under it keeps its crop, and a prompt
  that wraps starts the box at its second row (the review's #1 and #2).
- **The right-hand cut** applies only right of the cursor and only while the cursor is on that row, so nothing you
  typed is hidden once the line wraps (the review's #3).
- **Shades** (`theme.rs`): card surface, composer outer box and edges stepped from the terminal background; the
  **inner box is the sidebar's colour** in every theme (your call after the rework: "inner terminal chat background
  use the background color of this [the sidebar]. We make it follow the palette").
- **`terminal_view.rs`:** `paint_cards` paints the rail, the soft cards and the composer; `context_row` places the
  running chip, folder and branch (Lucide `Folder`, `Branch`, `ChevronDown`) over the outer box.
- **Checks:** 2283 tests; clippy at the baseline (14/27, 16/29); fmt; ui-shots 253 passed, the 6 known
  display-dependent failures; a new scene `shell-cards-prompt` (a two-sided prompt: the box shows `❯ ls` only).

**Rulings (the AI's):** the done dots use the theme's done green, a card whose exit wasn't reported a faint dot; the
old chips are removed (the context row replaces them); a press over `❯` lands at the hidden prompt's column 0; a
right-hand part counts only when it reaches the row's end (a double space typed mid-line isn't one); the review's
unjudged items stand as built: a right prompt with a double space inside it leaves its earlier parts (M11l's rule), a
session reattached mid-command shows the command's rows until the next prompt, the cursor jumps (no glide) when the
crop turns on.

**Deferred (minor, from the review):** a selection can't cross from the typing row to the rows under it; a copy to
the row's end can include the hidden right prompt; a prompt shorter than 2 cells lets text run 2 cells past the box;
the context row doesn't truncate in a narrow split; the folder shows your user name at home and nothing at `/` (not
`~` / `/`); the running chip's peach isn't held to the text floor on the outer box; the chosen card's ring clips about
1 pt in a split pane; the crop's painting has no unit test of its own; a prompt redraw may flash the full prompt for
one frame.

### On-screen checklist for the rework (yours)

With `FUXX_DAEMON_SOCKET=/tmp/fuxx-dev.sock cargo run`, a new zsh tab with your Powerlevel10k prompt:

1. A new tab: the composer at the bottom: folder ▾ and branch ▾ in the top row, the inner box in the sidebar's
   colour with `❯ Type a command`; neither side of your prompt shows.
2. Type `ls`: `❯ ls`; Return: a soft card with a green dot on the rail; `false`: a red dot and `exit 1`.
3. Type a long command until it wraps: all of it shows, nothing cut on the right.
4. Tab-complete at a prompt on the bottom row (a list under it): the prompt stays left out.
5. Make the window narrow so the prompt wraps: the box shows only your typing row.
6. `ping localhost`: the box grows, only output inside it, the peach chip ticking in the context row; ⌃C: a card.
7. Click the branch ▾: the branch switcher; the folder ▾: Finder.
8. Switch the theme (Latte and a dark one): the inner box follows the sidebar's colour.

## Related

[[Proposal - Chat View and Agent Tabs]] · [[63 - Milestone 11l - Command Blocks]] ·
[[68 - Milestone 11q - Which Command Finished]] · [[79 - Milestone 11aa - Agent Tabs]] · [[40 - Roadmap]] ·
[[12 - Scope]]
