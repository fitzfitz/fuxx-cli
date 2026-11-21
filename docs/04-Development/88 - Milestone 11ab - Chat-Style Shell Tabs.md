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

1. A new tab: the composer at the bottom: the folder and the branch in the top row, the inner box in the sidebar's
   colour with `❯ Type a command`; neither side of your prompt shows.
2. Type `ls`: `❯ ls`; Return: a soft card with a green dot on the rail; `false`: a red dot and `exit 1`.
3. Type a long command until it wraps: all of it shows, nothing cut on the right.
4. Tab-complete at a prompt on the bottom row (a list under it): the prompt stays left out.
5. Make the window narrow so the prompt wraps: the box shows only your typing row.
6. `ping localhost`: the box grows, only output inside it, the peach chip ticking in the context row; ⌃C: a card.
7. Click the branch: the branch switcher; the folder is a plain label (chores below).
8. Switch the theme (Latte and a dark one): the inner box follows the sidebar's colour.

### Chores after the rework (2026-11-21, on master)

Your words, on a screenshot of the composer: "make the gap between command box and the outter border smaller. git
remove the chevron. and why there's no folder information beside git branch?", then "when I click the folder, it
opens a folder window, is it a good UX? I don't think so".

- **The gap:** the inner box sits 3 pt in from the outer box (was 5); the terminal gains about a column and a row.
- **No ▾s:** the branch has none; a click on it still opens the branch switcher. The folder has none either.
- **The folder shows:** the row read only the folder your shell reports (OSC 7), and yours doesn't send one; the
  workspace now hands each terminal the folder it already finds for the branch (the report, else the shell process's
  own folder), so the folder shows whenever the branch does (`Places::place`, test
  `a_terminal_gets_its_folder_with_its_branch`).
- **The folder is a plain label:** a click does nothing (opening Finder from it was the AI's call in the rework, and
  out of place in a terminal).
- **The folder's tooltip** (your words: "on folder information, give tooltip"): its whole path as the app writes
  folders (`~/…`), on hover. Not in a screenshot (the scenes don't hover); yours to check.
- **Checks:** 2284 tests; clippy at the baseline; fmt; ui-shots: the 6 known display-dependent failures only.

### Chore design: the panel darker, the window lighter (2026-11-21, approved)

Your words: "I just realized how bad our theme color combination is. It supposed to be darker in the floating panel
and the window background is ligher. Can we switch it? ensure others follow the palette rules". Two ways offered: A,
lift the window above the terminal; B, darken the panel below the theme's background. You picked **B** ("let's try
B"), then approved this design ("approved, go ahead").

- **The switch:** the terminal and the floating panel take today's window colour: each built-in theme's
  `chrome-canvas` (Mocha `#11111b`, Latte `#dce0e8`, Nord `#242933`, Dracula `#191a21`, Gruvbox `#1d2021`,
  TokyoNight `#15161e`), else today's derived one (dark: the background 42 % toward black; light: 8 % toward the ink);
  the window and the sidebar take the theme's own background. Done once where a theme is settled
  (`theme::resolve`), so everything that reads the background follows.
- **The rest follows the rules:** what sits on the panel (cards, the composer's outer box, file and diff tabs,
  previews, hovers, hairlines) mixes from the darker panel; what sits on the window (sidebar rows, search box, tints)
  from the lighter one; text and marks keep their contrast floors in every theme.
- **The composer's inner box** is the panel colour (the darkest, an inset), replacing "the sidebar's colour".
- **Light themes** follow the same rule. A theme's ANSI colours are unchanged.
- **Tried on a branch** (`theme-dark-panel`), screenshots in Mocha and Latte for you before it reaches master.

**As built (branch `theme-dark-panel`):** `Theme::with_dark_panel` at the end of `theme::resolve` (the screenshot
harness too); the prefs swatches show the panel colour. Along the way, from your looks at it:
- **One window colour** ("sidebar background color and the floating panel background … should be same"): the
  panel's drop shadow darkened the 8 pt of window round it; it's gone (`card_shadow_alpha` 0), the panel stands out by
  its colour.
- **Corners** ("the border radius on the floating panel, is not matching the window"): 14 (the window's own) widened
  the gap at the corners and the strict parallel curve (6) read too tight; you picked **9 pt** from renders side by
  side, then **11 pt** ("add more border radius on floating, now it doesn't match the outter"). The diff view's
  cards follow it.
- **AI's calls:** a config `background = …` line without its own `chrome-canvas` drops the theme's canvas (it was
  picked for the theme's background, not yours), so the panel derives from your colour; the pinned dim and faint
  text values moved (Mocha `9da3b9` / `9fa3ae`, Latte `515263` / `52525f`), every contrast floor holding. **For your
  check:** in Latte the dim text is now close to the full text (the darker panel pulls it down to stay readable).
- **Checks:** 2289 tests; clippy at the baseline; fmt; ui-shots 253 passed, the 6 known display-dependent failures
  (scene probes moved for the smaller corners and the missing shadow).

### Chore: the reference app's palette rules (2026-11-21, a try on `theme-dark-panel`)

Your words: "learn this app's color palette, how it present their color combinations", then "yes adapt it, show me
the screenshots". Measured from your screenshot: one window colour (`#2a2b26`: sidebar, title bar, strips), the
panel a quiet step below (`#23241f`, 1.1:1), what you work in raised above the panel (composer `#2d2e29`), edges
lighter than their fill and lit along the top, full text near white (`#f7f7f0`) with dim text well below
(`#878781`), colour only where it means something. Adapted, each still derived from the theme's own colours:

- **The panel a quiet step below the window:** dark 17 % toward black, light 5 % toward the ink; the built-in
  themes' deep `chrome-canvas` lines dropped (yours, set in your config, still win).
- **The composer raised:** outer box and inner box a shade and two above the panel, the inner box's top edge the
  lightest (replaces "the panel colour" for the inner box).
- **Text hierarchy:** the chrome's full text half way to white (dark) or black (light), the terminal keeping its
  theme's own; dim text half way down, pushed to its floor: full to dim 1.6:1 or more in every theme (Latte was
  1.12). Latte's dim and faint now meet at its floor.
- **Found on the way:** Latte's lighter panel had met `panel_hover` (a pressed title-bar button's fill, 1.01:1); it's
  held off the card at `SOFT_LINE_MIN` now.
- **Checks:** 2292 tests; clippy at the baseline; fmt; ui-shots 253 passed, the 6 known failures (the file tab's
  glyph check now reads `tile_ink`, the colour the tab draws it in).

**The lead designer's review, taken as given** (your words: "ask 'lead UI/UX designer' agent and then I'd go with
that agent suggestion"; a fresh Opus agent, read only):
1. Command cards are the panel with a 1 px edge (held to `SOFT_LINE_MIN`), not a fill lighter than the composer.
2. Light themes: raised is lighter, an input field: outer 40 % and inner 80 % toward white, darker hairlines (the
   inner one at `QUIET_LINE_MIN`), no lit top line.
3. Dark composer at the reference's step: outer 6 %, inner 9 %, edges 11 %, the inner box's top 18 %.
4. Selection neutral: the selected sidebar row 8 % toward the text colour, hover 4 %, the shown pane tab 6 %; the
   2 pt accent bar is the one accent mark.
5. Faint text is dim text: two greys, rank below dim by size and place.
6. Kept: no shadow, the 11/14/8 pt corners. Its two flags, taken (your words: "I meant I agreed with the 2 things
   the designer flagged"): the panel step on dark themes **22 %** (was 17 %: its edge got lost on a dim display) and
   the chrome's full text **30 %** toward white or black (was 50 %: it pulled the eye off the terminal); full to dim
   is now 1.5:1 or more (Mocha 1.98, Latte 1.64, Nord 1.56), and a dark theme's border may sit at 1.45 on the
   window (Nord 1.41).
- **AI's call on the way:** a pressed or lifted row (`row_active`, a dragged row's copy) at 14 % on every theme, clear
  of the 8 % selection (Latte's 10 % had met it).
- **Checks:** 2295 tests; clippy at the baseline; fmt; ui-shots 253 passed, the 6 known failures.

### Fix: pieces of the prompt piling up after many resizes (2026-11-21)

Your report: after resizing, "it still showing the older terminal slightly … the previous chat command goes up and
create gaps"; "only happens in current tab which resized multiple times (50x at least)". Found by recording your
own zsh and Powerlevel10k through 121 fast resizes and replaying the bytes through fuxx's grid: Powerlevel10k writes
a redraw in two pieces (A, the left side, B; then the right side placed for the width zsh knew). A resize between
the pieces made the second one stale: in the narrower grid its right side wrapped and the cursor ended a row lower,
where the next redraw started, leaving the piece behind and pushing the prompt (and the cards) up a row each time.

- A width change that empties the live prompt remembers its row (from the top of the scrollback, so scrolling
  doesn't move it); the shell's next A finds the prompt there: the rows between (at most 4, never a tagged one) are
  emptied and the redraw starts back on the prompt's row.
- A second width change before the redraw (the prompt's tag already gone) empties the same rows again, so they
  never reflow and the remembered row stays true (without it, a reflow above pushed it a row off and output was lost).
- Tests: `a_resize_mid_redraw_leaves_no_pieces`, `two_resizes_before_the_redraw_keep_the_output`; your recorded
  drag replays to the grid as it was before it.

### Chore: no git tab outside a repo (2026-11-21, on master)

Your words: "hide git tab if the folder is not git". The right sidebar now works out whether the followed folder is
in a repo whichever panel shows (the Changes panel's root lookup, `find_root`, split out as `root_changes`); once
it's known not to be one, the strip shows Files and Activity only, and Changes chosen there shows Files. While the
answer is on its way the tab stays (no flicker); an SSH terminal's folder is unchanged. Test
`the_git_tab_hides_outside_a_repo`; the scene `changes-empty-norepo` now checks the two tabs and Files shown. Checks:
2296 tests; clippy at the baseline; ui-shots the 6 known failures.

### Chore: the right panel redesigned (2026-11-21)

Your words: "ask the same designer agent to help me redesign this right tabs, looks so bad at the moment", then "yes
build it, show me the screenshots in a web server" and "merge it and push". The lead designer's diagnosis: three
stacked boxes (a track filled with the window colour reading as a hole, a lozenge pill per icon, a filled repo card,
a filled filter). As built, its spec as given:

- **Tabs** like the pane tabs: no track, 28 pt square icon tabs (16 pt icons), left-aligned 8 in, 2 apart, corners
  8; idle unfilled in `text_dim`, hover `side_tab_hover` (4 %), shown `side_tab_selected` (8 %), names in tooltips;
  with two tabs (no git folder) nothing stretches. The strip's empty part drags the window.
- **Header:** a plain 36 pt row (no repo card); the folder's name and ▾ one quiet button padded 6 so its words land
  on the 12 pt edge; collapse-all `text_dim`, `side_tab_hover` under the pointer. Changes keeps its bar (44 pt);
  Activity's header 36 to match.
- **Filter:** 30 pt, 8 from the sides, corners 8, `side_field` (5 %), no border, icon and placeholder `text_dim`.
- **One divider:** the strip's bottom line.
- **Checks:** 2297 tests; clippy at the baseline; ui-shots 254 passed, 5 known failures (`right-panel-comfortable`,
  its checks rewritten for the new design, now passes).

### Fix: the right panel follows an agent tab (2026-11-21)

Your report: "agent tab doesnt update the right panel" (an agent tab in one folder, the panel showing the last
terminal's). The panel followed only terminals and treated an agent tab like a browser or file tab ("keep the last").
Now a focused tab decides (`Follow::of`): a terminal is followed as before, an agent tab by its folder (its `cwd`;
Files and Changes show it, never as SSH), a browser or file tab keeps what was followed. Test
`an_agent_tab_is_followed_by_its_folder`; the status bar and Activity still describe the followed terminal.

### Chore: the composer as a folder tab; clean window corners (2026-11-21)

Your words: "can we update the design to be more elegant? like this [a rough paint] … Ask designer agent to get the
details", then about the window: "on the very edge corner there is a slight border or shadow … happening on 4
corners … can we make it gone so it looks clean?"

- **Folder-tab composer** (the lead designer's details, as given): the box's top edge runs `TAB_RISE` (25) below a
  tab rising at its left that holds the context as pills (22 tall, 8 in, round, on `composer_inner`, 4 apart, 3 in
  from the tab); the tab's top corners 14, the box's 15, a concave 8 where the tab steps down; same fill and edge as
  the box, no seam. The running command is the first pill (its dot `ring_working`, the composer's one colour); the
  branch pill takes `composer_pill_hover` and the text colour under the pointer and opens the switcher; the folder
  pill keeps its path tooltip and gives way first. No pills, no tab. The tab fits the pills as laid out (their row
  measured each frame, `pills_w`), never wider than the box less 48. `CTX_ROW` 28. Tests
  `the_composer_tab_fits_its_pills`, `the_composer_pill_hover_is_a_step_over_the_pills`.
- **Window corners:** fuxx's one-pixel window edge followed a circular 14 pt corner; macOS's corner is a smooth
  curve, so at the corners the two parted and a sliver of the edge showed inside macOS's rim. The edge now runs
  along the four straight sides only, stopping short of the corners, which are macOS's own. The `status-bar` scene
  checks a dark theme's corner diagonal (it read the edge's black before the fix).
- **Checks:** 2300 tests; clippy at the baseline; ui-shots 254 passed, the 5 known failures.

### Chores: the right prompt near the typing; capped pills; the typing's second row (2026-11-21)

- **The right prompt stayed in the box** (your screenshot: `cd ../../hube_` then `✔ / 14:38:07`). Recorded from your
  shell keystroke by keystroke: zsh keeps the right prompt until one blank is left before it, where `crop`'s
  two-blank rule no longer finds it. The right-hand part found once for a prompt is remembered (its column and
  text, `seen_right_side`) and stays cut while the cursor is left of it and its cells still match
  (`remembered_cut`); typed over or dropped by zsh, nothing is cut. Test `the_right_side_once_seen_stays_cut`.
- **Pills capped** (your words: "add max width on the folder and branch, make it ellipsis"): each at most `PILL_MAX`
  (160 pt, the AI's call), the name ending in "…"; the branch pill's tooltip has its whole name.
- **The typing's second row hangs under the first** ("the second line is supposed to align with the first line,
  the icon below should be empty"): rows under the first shift `INDENT_COLS` (2, the width of `❯ `), the terminal
  two columns narrower in card view so they still fit (which also closes the deferred "text can run 2 cells past
  the box"). Scene `shell-cards-wrap` (it failed with the indent taken out).
- **Checks:** 2301 tests; clippy at the baseline; ui-shots 255 passed, the 5 known failures.

### Chore: the agent tab's composer as the shell tab's; the folder fixed once you've spoken (2026-11-21)

Your words: "let's redesign the chat on agent tab to be more likely in terminal tab. make sure when we already make a
prompt, disabled the change project/folder", then "just go ahead with your best recommendation" (the AI's calls).
Kept to the composer: the agent tab's layout and flow stay M11am's, which waits for a discussion.

- **Folder-tab composer:** the agent box takes the shell composer's shape and palette: the folder and branch as pills
  (the app's Lucide icons, no 📁/⑂/▾ text, `PILL_MAX` with "…", tooltips with the full path / branch) in a tab at
  the box's top-left; the box you type in (attachments, the text, `+`, model, mode, send) is the inner box
  (`composer_inner`, its edge, the focus ring while typing), `OUTER_INSET` inside the outer box. Built from elements
  (`AgentTab::folder_tab`): the tab sizes to its pills, the concave step a page-coloured corner over the box's fill.
  The conversation's column lines up with the outer box.
- **The folder is fixed once you've spoken** (`folder_changeable`): before the first message the folder pill opens
  the picker as before; after it the pill is a plain label (its path in the tooltip) and `open_folder_picker`
  refuses from any route. The Move this conversation / New conversation choice that followed became unreachable;
  its two scenes became `agent-box-folder-locked`, which fails with the lock off.
- **The move code removed** (your words: "yes remove it"): the Move / New conversation choice (`BoxMenu::Move`,
  `move_choice`, `can_move`, `MOVE_HERE`), the agent-version reading that existed only to ask whether a Claude
  could move a conversation (`Provider::version_probe` and `can_move`, Claude's `MOVE_SINCE` / `version_at_least`,
  the catalog's version store and `load_version`, the `claude-version.txt` fixture) and their tests; about 230 lines
  net. Kept: "New conversation" (other menus use it) and a folder change before the first message.
- **Checks:** 2302 tests; clippy at the baseline; ui-shots 254 passed, the 5 known failures (the 71 agent scenes
  pass).

### Chore: the agent box's controls and menus redesigned; the mode waits too (2026-11-21)

Your words: "view usage broken, model list is ugly", "the chat is different, should have no border, the '+' and model
picker trigger is ugly (gap) … add circle background to '+' and make the model picker to be pill, same goes the mode
picker", "Make sure I can choose mode mid run and the next prompt will pick up my chosen mode. same goes model". The
lead designer's details, as given:

- **The box:** no border and no focus ring; the controls 8 in from its edges. `+` a 28 pt circle (`control` fill, a
  Lucide plus); the model and mode pickers 28 pt pills (`control` fill, a chevron-down, the mode with its icon:
  shield / file-pen / list-checks / zap); under the pointer the pill's fill steps up and its text turns full
  (`control_hover`, readable). Send a circle with an arrow-up, the accent when there's text, quiet when empty.
- **Menus:** headings 24 tall in the dim text; rows radius 6, the check (Lucide, text colour) on the right.
  Model menu: the effort's chevron only on the hovered row and the current one; "Default" with "recommended" quiet
  beside it and a line under it; the last row "Usage" with a gauge. Mode menu 300 wide: an icon column, the name,
  its description (two lines at most), 2 pt between rows.
- **Usage:** 300 wide, scrolls past 480; each limit its name and "N% left", a 4 pt bar (the waiting colour at 10 %
  left or less), "Resets …" under it (the rings are gone); a "Top …" line becomes a heading and up to three rows,
  shown once (`top_list`).
- **The mode chosen mid-run:** the model already waited for the turn's end and went with the next prompt; the mode did
  too, but the picker kept showing the old one until then. Now it shows the chosen one at once
  (`mode_chosen`); scene `agent-box-mode-waits` (the picker shows Plan mid-run; the next send sets the mode before
  the message).
- **Checks:** 2302 tests; clippy at the baseline; ui-shots 255 passed, the 5 known failures.

### Chore: the agent box edits like the shell's line (2026-11-21)

Your words: "make sure the font the textbox nature like cursor move with keyboard, delete text/sentences,etc same as
terminal textbox". The box had the Mac keys (⌥←/⌥→, ⌘←/⌘→, ⌘⌫) but none of the shell's editing keys. Added, in
the "Composer" key context: **⌥⌫ and ⌃W** delete the word before the caret (`delete_word_left`; across a line break,
as zsh does); **⌃K** to the end of the line (`delete_to_line_end`; at the end it joins the next line); **⌃U** to
its start (as ⌘⌫); **⌃A / ⌃E** line start and end; **⌃B / ⌃F / ⌃P / ⌃N** a character or a row (⌃P / ⌃N walk an
open `@` or `/` list, as ↑ / ↓); **⌃H / ⌃D** a character back and forward. Not added: zsh's kill ring (⌃Y pastes
what ⌃W / ⌃K / ⌃U took) and ⌘↑ / ⌘↓ (the terminal uses them to scroll).

- **Checks:** 2305 tests (the two operations, the key map); scene `agent-box-shell-keys` (real keystrokes; fails
  with ⌥⌫ unbound); clippy at the baseline; ui-shots 256 passed, the 5 known failures.

### Chore: the agent box and the shell box made the same (2026-11-21)

Your words: "Because right now, you see the differences? also the cursor is very different". The lead designer's
call: the shell's typing is zsh's line in the grid, so the agent box moves to it; what fuxx draws itself follows one
rule in both.

- **The typing:** the agent box types in the terminal's font, size and cell height (`CellMetrics`), wrapping a cell
  short of the edge for the caret.
- **The cursor:** the agent box's caret is the terminal's cursor: the cursor setting's shape (bar, underline, hollow
  or block) in the theme's cursor colour, one cell; hollow when the box isn't focused in the key window; a block's
  letter drawn again in the cursor's text colour (`cursor_look::box_caret`, `caret_rect`). The shell box is
  unchanged: zsh and programs still choose its shape.
- **`❯`:** the agent box has the shell's, in the accent, the typing two cells after it (wrapped rows too); in both
  boxes it sits 14 in from the inner box's left and the first line 12 down.
- **The placeholder:** one style in both: the terminal's font and size, dim, one cell after the cursor's, cut with
  "…" when it doesn't fit (`card_view::fit_words`). Each keeps its words.
- **The inner box:** no edge in either (the shell's 1 px edge and lit top edge, and their two colour tokens, are
  gone); its fill on the outer box sets it apart. The shell box's text moved in by the edge's 1 pt.
- **The AI's calls:** the agent caret doesn't breathe (the cursor glow is the terminal's animation) and keeps the
  theme's cursor colour when the cursor follows the status (an agent box's typing is idle anyway); the shell box's
  "…" only shows below about 17 cells, narrower than its card view draws.
- **Checks:** 2308 tests; scenes `agent-box-terminal-text` (cell height, block caret one cell, hollow when not key,
  placeholder one cell on, `❯` 14 in, typing two cells after, 12 down; fails before the change) and
  `shell-cards-placeholder`; two older scenes given more text (the smaller cell wraps the same words into 8 rows,
  not more); clippy at the baseline; ui-shots 258 passed, the 5 known failures.

### Chore: the agent caret is the terminal's cursor; send in a notch (2026-11-21)

Your words: "it doesn't respect the cursor setting, and it doesnt glow and smooth like terminal tab", then "can you make
the button on the bottom right to be like this? its outside but we play radius on the inner box on the bottom right",
then "button background make it same as inner box background color, and add more radius on the right bottom chat box
so the radius rounded and the button circle match".

- **The caret is the terminal's own cursor code**, not a look-alike: the composer builds a small grid of its shown
  rows and asks `CursorSettings::look` for the cursor, as a pane does, so every cursor setting applies: the shape,
  the glide between cells, the trail or sparks (`cursor-fun`), the breathing glow, the status colour, hollow without
  focus. Its breathing timer is the pane's (`breath_sleep`, `breath_redraw`). The terminal's cursor drawing moved into
  `paint_cursor_under` / `paint_cursor_over` (the pane draws through them too), and its sparks into
  `CursorMotion::spark`. A changed setting redraws the box. (The last chore's stand-in, `box_caret` and
  `caret_rect`, is gone.)
- **Send in a notch** (the lead designer's details): a 34 circle in the outer box's bottom-right, 3 in from its inside
  edges; the inner box cut round it by a concave arc (radius 20 about the circle's centre) with 8 round joints where
  its straight edges meet the cut; the cut drawn under the box's pills and text. The controls row 3 from the inner
  box's bottom and left (its centre level with the circle's), the mode pill ending 11 before the circle; the inner
  box's bottom-left 17. Stop takes the same circle: a 12 square (quiet while stopping, the waiting colour for Force
  stop), its words in a tooltip.
- **Your two follow-ups:** the circle is the inner box's colour in every state, its arrow dim when empty and in the
  accent when there's text (`send_arrow`, held to `MARK_MIN` on the box); the outer box's bottom corners are
  concentric with the circle (17 + 3 + the 1 px edge = 21).
- **Your next look** ("make it cleaner, add gap between chatbox with button and I think the button background should
  back to the first one the blue one"; the designer's numbers): the cut 6 round the circle (arc radius 23, its straight
  edges 43 in, the mode pill 51 in); the circle the accent again, `on_accent` arrow, a step lighter or darker under the
  pointer; empty, a dimmed accent (`mix(composer_outer, accent, 0.60)`: the designer's 0.45 looked muddy on Mocha,
  its fallback taken) with its arrow held to `MARK_MIN`. (The inner-box-coloured circle of your previous look is gone.)
- **The designer's honest review** (your words: "is the button color accepted and match with the combinations?"): the
  ready blue belongs (one muted accent on the one primary action, tied to the `❯`); the dimmed blue at rest didn't
  (louder than any pill for a button that does nothing, a muddy dark arrow at 3.29:1, and blue at rest says nothing
  when you start typing). On your word ("go with the designer, grey when empty"): empty, the circle is a control
  like `+` (`control`, its arrow `dim`, no hover); it turns blue when you can send.
- **Checks:** 2306 tests; scenes `agent-box-terminal-text` (now also: a changed setting redraws; a typed letter glides,
  failing with glide off) and `agent-box-send-notch` (34 across, 3 in, level with the controls, the mode pill 11
  before it, 14 since the gap; unchanged as the box grows); clippy at the baseline; ui-shots 259 passed, the 5 known failures
  (`file-tab-md-mermaid` failed once in a full run and passed alone twice: timing).

### Chores: the agent tab redesign — tool modal, top bar, live context, chat, logos, both clears (2026-11-21, 🟨 built, reviewed, pushed `19ef2bb`)

Your words: "each action accordion arrow, change to chevron. when its clicked change it into a modal instead on
dropdown"; "agent top status bar, make it excellent, the color, gaps, animation, is the price and context live?";
"Go with the Lead Designer agent recommendations"; "go ahead, make context live". Plan:
[[Chores Plan - Agent Tab Redesign]].

**Live context (done, `5ec0b41`).** Before, cost and context changed once a turn, from Claude's end-of-turn `result`.
Now each main-thread model call moves the context (its `usage`), measured against the model's window, which a
finished turn reports and fuxx remembers per model for the app's run (so the first turn on a model after launch
still updates at its end). Cost stays exact and updates at each turn's end (Claude gives no live cost; estimating
from a price table was ruled out). agy reports neither. A sub-agent's calls don't move the meter.

**The lead designer's spec A (2026-11-21).**

*A. Chevron.* `Icon::ChevronRight`, 12 px box (`right_sidebar::CHEVRON`), never rotates (it opens a modal: "go
into"); rest `panel_dim`; hover: the whole row, pointer, fill `row_hover = mix(ui.panel, fg, 0.04)`, chevron
`panel_text`; no motion. Rows that can't open keep the empty 12 px slot (results stay on one right edge).
`tool_name` fitted on both `panel` and `row_hover` (`readable(accent, [panel, row_hover], TEXT_MIN)`).

*B. Tool detail modal.* Scoped to the agent tab: the scrim covers the tab below its top bar (the bar stays lit and
live). Top 48 px from the tab's top, centred; width `min(pane_w − 48, 720)` (`pane_w − 16` under 360); height fits,
up to `pane_h − 72`, top-anchored (live output grows it downward). Scrim `ui.shadow` α 0.35 dark / 0.25 light, a
click on it closes. Surface as `overlay_panel::shell` (panel, 1 px `panel_border`, shadow α 0.3 y 8 blur 24),
radius 12. Header 44 px, padding 14 left / 8 right, gap 10, 1 px divider: Back (in a sub-step) · status mark 12 px
(running: the Working arc; done: `done_check` in `text_dim`, popping once (`POP`) if it finished while open;
failed: `Icon::Octagon` in `failed_panel`) · name 13.5 MEDIUM `tool_name` · summary 13.5 `panel_text`, truncating
("Task › " prefix in `panel_dim` inside a sub-step) · "N steps" `panel_dim` · right text 12.5 tabular (running:
elapsed; diff: `+a` in `added_ink`, `−r` in `removed_ink`; failed: the result in `failed_panel`, no "✗";
else the result in `panel_dim`) · ✕ 28 px r8, 15 px glyph, `text_dim` → `text` on `panel_hover`. Body: one scroll,
padding 16, gap 12; each section labelled (11.5 MEDIUM `panel_dim`, 6 px above its well). Wells: fill `page`
(not `code_fill`, which is 1.00:1 on `panel` in dark themes), 1 px `panel_border`, r8, padding 10/12, mono 12, line
height 1.45; text `c.text`, dim `c.dim`. Sections in order, each when it applies: Command ("$ " dim, wraps),
Changes (`diff_block`), Output ("… N earlier lines" first; "No output yet" while running with none), Steps (the
children via `item_body`; a step's click replaces the content (a stack), Back = `Icon::Back` 14 px in 28 px r8).
Running: elapsed ticks on the tab's 1 s redraw, output appends, the body follows the bottom only while within
24 px of it. Motion: scrim fades in 140 ms ease-out; the panel fades and rises 8 px over 180 ms ease-out-cubic;
closing and content switches instant; nothing with Reduce motion. Keys (context `AgentToolModal`): Esc back one
level, then close; ✕ / scrim close; ↑/↓ previous/next openable tool among the level's siblings, no wrap;
PageUp/PageDown scroll; focus returns to the agent box; the modal closes if its tool disappears. Copy buttons on the
Command and Output wells only if the Markdown reader's Copy is a straight reuse.

*C. Top status bar.* 32 px (was 26), `AREA_X` padding, no fill, no line. Left: the status pill — 22 px, r11,
padding 8/10, gap 6, word 12.5 MEDIUM; its fill hangs at `AREA_X − 8` so the mark sits on the conversation's
column; mark: a 12 px `MarkView` on the shared clock (in step with the sidebar and tab marks).
`PILL_TINT` 0.16 dark / 0.12 light:

| State | Fill | Mark | Word |
|---|---|---|---|
| Working | `mix(page, ring_working, PILL_TINT)` (#403c35 / #e3d6d3) | turning arc, `ring_working` | "Working", `ring_working` fitted to 4.5 on the fill |
| Needs input | `mix(page, ring_waiting, PILL_TINT)` | breathing dot, `ring_waiting` | "Needs input", fitted |
| Done | `mix(page, fg, 0.06)` | `done_check` `text_dim`, pops once | "Done", `text_dim` fitted |
| Stopping… | neutral | Working arc | "Stopping…", `text_dim` |
| Idle | none | idle ring `ring_idle` | "Idle", `c.dim` |

State change: fill and word blend 200 ms ease-out-cubic (keyed by a change counter); width snaps. No shimmer (the
10 fps mark clock stays the only continuous redraw). Right: cost "$3.99" 12.5 MEDIUM tabular `c.text`; after a turn
raises it, "+$0.42" 6 px to its left in `c.dim`, fading in 200 ms, gone when the next turn starts working; none for
the first value; tooltip "Conversation cost · updates when each turn ends". 16 px. Context: "Context" `c.dim` · 8 ·
meter 56×4 r2 on `meter_track` (`mix(page, fg, 0.12)` held to `QUIET_LINE_MIN`), fill `pct/100 × 56` (≥ 2 px above
0) · 8 · the percent, min width 32, right-aligned. Below 70 %: fill `meter_ok` (accent held to `MARK_MIN` on page and
track), percent `c.dim`; 70–89 %: `meter_caution` (palette 3), percent in it fitted; 90 %+: `meter_critical`
(palette 1). Each new value moves the fill old → new over 400 ms ease-out-cubic (the live signal); the number
updates at once; tooltip "Context window used · updates as the agent works". Missing data: the group isn't drawn
(agy: an empty right side); a group appearing fades in 200 ms. Narrow: the delta goes below 480 px, "Context"
below 420, the meter below 320.

*D. Audit (the designer's).* Every text pair ≥ 4.5:1 and every mark/line ≥ 3:1 across seven themes (tightest: Latte
working word 4.50, Gruvbox critical meter 3.01, Latte caution 3.01). Three colour roles, one job each: state colour
(the pill, with motion), accent (the meter), caution/critical (only near a full window). Done stays muted (colour
means activity). "Needs input" replaces "needs you", as the sidebar and status bar say.

*E. On-screen checks.* (1) Esc with the modal open never stops the agent; ↑/↓ never reach the box's history.
(2) Only the marks redraw per tick (no observing the clock from the tab). (3) Animations keyed by change counters,
not restarted by the 1 s redraw. (4) The scrim's bottom corners against the card's radius, and in split panes.
(5) The modal follows a long Bash at the bottom and stops when you scroll up. (6) The new pairs in the contrast
tests. (7) Context mid-turn against Claude's `/context`. (8) Each pill state dark and light; the meter at 30 / 75 /
95 %; the delta; an agy tab; narrow trimming. (9) The modal: Bash running → done, failed Bash, an Edit, a Task's
steps (in, Back), ↑/↓ at both ends, a split pane. (10) Reduce motion: nothing animates.

**More asks (2026-11-21):** "add colored icon for claude and antigravity. Icon on tab change to claude icon (not "CC")
… do it also with antigravity"; "chat icon as well change it to the each respective icon"; "make the chat better and
excellent. Im handing the design to the Lead UI/UX Designer agent"; "inactive tabs, make it slighly more
darker/transparent on the background"; "fix the `/clear` on claude … when the session clear, remove all chat history
in the session. make it looks like the chat is being cleared".

**The lead designer's spec B (2026-11-21).** Values: default dark (page #1f2229) / Catppuccin Latte (#e7e9ee).

*B-A. Logos.* `ProviderInfo.logo: Option<Logo>` (`Logo::{Claude, Antigravity}`); `TabIcon::Agent` and the chat's
`Look` carry it; others keep the text badge. Assets in `assets/logos/` (not `icons/lucide`, which `svg()` tints;
logos draw in full colour with `img()`), with a `NOTICE.md` (Claude: Simple Icons, CC0 SVG, Anthropic's trademark,
filled #D97757; Antigravity: Google's mark, from the Antigravity IDE). GPUI rasterises an SVG `img` at 2× the
file's own width/height, so each drawn size gets its own `width`/`height`, and each `Arc<Image>` is built once per
(logo, size). Tab: bare 14 pt in the `TILE` slot (no tile); fallback today's letters. Chat avatar: bare 18 pt in a
22 × 22 slot; fallback today's tile. Empty state: 40 pt (fallback tile 40 r10, letters 16 semibold). Inactive tab,
dark themes: the tile (and the Lucide glyphs of other inactive tabs) at opacity 0.75, 1.0 hovered or active; light
themes always 1.0 (the logos are only ~2.5:1 on Latte). Brand colours are never contrast-fitted. (The sidebar's
preset rows' "CC" at 16 could take the logo too — not asked for.)

*B-B. Chat.* Tokens `ui_tokens::chat`: COLUMN_MAX 760 (the whole column incl. avatar, left-aligned at `AREA_X`),
AVATAR 22, LOGO 18, AVATAR_GAP 10, BODY 14 / 1.5 (your message 13 → 14), META 12.5, FIRST_TOP 16, LAST_BOTTOM 20,
GAP_TURN 24 (before You, and before the first agent item after You), GAP_ITEM 12 (text → text, before todo or
permission), GAP_TEXT_TOOL 8, GAP_STACK 0 (tool → tool), GAP_NOTICE 16, TOOL_ROW 30, TOOL_PAD_X 10, TOOL_GAP 8,
TOOL_STATUS 12, TOOL_DOT 6, TOOL_DIVIDER_INSET 30, THINKING_H 24, THINKING_FADE 500 ms, THINKING_HOLD 0.4,
TODO_METER 56 × 4. Radii: 6 chips/buttons, 8 tool groups/code/wells, 12 cards, bubble 16 with a 6 bottom-right.
Gaps from the previous visible item. The avatar on the first visible agent-side item after You, a Notice, an Error or
the list's start, and on the thinking line after You; its top offset: text 2, tool 6, todo 11, permission 16,
answered 2, thinking 3. No timestamps (no data).
- **Your message:** bubble `max_w min(78 %, 520)`, padding 14 × 9, the radii above, 14 / 1.5, fill
  `ui.composer_inner`, 1 px `ui.composer_edge`, text fitted on it. Attachments under it (gap 6, wrapping): chips 22
  high, padding 8, r6, fill `mix(you_fill, fg, .06)`, Lucide Paperclip 12 + 4 + name META `you_dim` (no 📎).
- **Agent message:** `Sizes::CHAT` — headings [18, 16, 15, 14, 14, 13], heading line 1.35, above 20, below 6;
  block gap 12, item gap 6, indent 22, marker gap 6; code 12.5 / 1.5, pad (12, 10), r8; rule margin 8; quote 12 / 3;
  table cell (10, 5). No H1/H2 underline in chat (only with `draw.reader`). List markers `dim`, numbers tabular.
  Content capped at COLUMN_MAX − 32.
- **Tool rows:** a run of consecutive top-level tools is one group (children skipped). Rows 30 high, `panel`;
  shape from neighbours — Solo r8 all, borders all; First top r8, borders top/left/right, divider below; Middle no
  radius, left/right, divider; Last bottom r8, left/right/bottom. Borders 1 px `panel_border`; divider 1 px
  `tool_divider = readable(mix(panel, fg, .07), [panel], SOFT_LINE_MIN)`, inset 30. Contents: status slot 12
  (running: the Working arc; ok: 6 px dot `tool_ok = readable(ring_done, [panel, row_hover], MARK_MIN)`; failed:
  6 px dot `failed_panel`; neither: 6 px hollow ring 1 px `panel_dim`) · name META medium `tool_name` · summary META
  `panel_text`, truncating, **hidden when it equals the name** · result META `panel_dim`, max 40 %, truncating
  (failed: `failed_panel`, no ✗; running: elapsed, tabular) · chevron (spec A). Hover `row_hover` with the shape's
  corners. No ◐ spinner; no inline expansion.
- **Todo:** a card — `panel`, 1 px `panel_border`, r12, padding 14 × 10. Header 20 high: ListChecks 12 `panel_dim`
  + 6 + "Tasks" (META semibold `panel_text`) · spacer · "3 of 7" (META `panel_dim`, tabular) + 8 + meter 56 × 4 r2
  (track `readable(mix(panel, fg, .10), [panel], 1.2)`, fill `tool_ok`, still). Divider `tool_divider`, 8 above
  and below. Rows 13 / 1.45, gap 4, mark slot 14, gap 8: done Check 12 `tool_ok`, text `panel_dim`; active
  CircleDot 12 `todo_active`, text `panel_text` medium; pending Circle 12 `panel_dim`, text `panel_text`.
- **Permission (open):** `ask_fill`, 1 px `ask_edge`, r12, padding 14 × 12, gap 10. Header Shield 14 `ask_head` + 8
  + "Claude wants to run Bash" (14 semibold `ask_head`). Command well `code_fill`, r8, padding 12 × 10, mono 12.5.
  Detail mono META `ask_dim`, ≤ 12 lines. Buttons 28 high, padding 14, r6 (Allow ⏎ primary). Hint META `ask_dim`.
  **Answered:** one 22 px line, no card — Check 12 `tool_ok` (allowed) / `Icon::Stop` 12 `failed` (denied) + 6 +
  "Allowed · Bash cd …" (META `dim`, truncating).
- **Notice:** on the agent's text column (`pl = AVATAR + AVATAR_GAP`): Info 12 `dim` + 6 + META `dim`, wrapping.
- **Error:** a card, r12, padding 14 × 12, fill `mix(page, block_failed, .08)`, 1 px
  `readable(mix(page, block_failed, .30), [page, fill], QUIET_LINE_MIN)`, Octagon 14
  `readable(block_failed, [fill], MARK_MIN)` + 8 + the message in `text` (fitted on the fill); buttons below, gap 10.
- **Thinking line:** while Working and the last visible item isn't streaming text, a running tool or an open
  permission. Drawn inside the last row, under it; 24 high; after You the avatar logo in the slot, else empty; the
  Working arc 12 + 8 + "Thinking…" (META `dim`); its top gap as an agent item's after that item. Fades in
  over 500 ms, opacity `clamp((t − 0.4) / 0.6)` eased out (a built-in delay: short gaps never flash); still with
  Reduce motion.
- **Queued:** the bubble, quieter — fill `ui.composer_outer`, edge `composer_edge`, text `fit(text_dim,
  composer_outer)`; the "Starting Claude…" caption stays.
- **Empty state:** centred, 24 above the true centre: logo 40 · 16 · "Ask Claude to do something" (15 semibold
  `text`) · 6 · the place (mono 12.5 `dim`).
- Motion: only the thinking fade, the arcs (the 10 fps clock) and the bar's meter. No row entrances; instant hovers.

*B-C. Inactive tabs.* Today inactive (`card_hover`, .06) equals active (`tab_active`, .06). New `tab_idle =
mix(card, fg, 0.025)` (#25282e / #e3e5eb) and `tab_idle_hover = mix(card, fg, 0.045)`; active stays `tab_active`; the
active tab of an unfocused pane stays `card_lift`; a waiting inactive tab keeps its tint; titles stay `text_faint`
(7.46 / 5.67). Both added to `drawn_pairs`' tab bars.

*B-D. Audit.* Every text pair ≥ 4.5 (tightest: tool name on hover 4.50 dark, failed result on hover 4.53, error red
4.50 Latte, todo active 4.50 Latte); no new hues (every token a mix of `UiColors`).

*B-E. Risks.* (1) A tool group row's shape depends on the next item and the thinking line on being last: fold
both into the row's fingerprint so `changed_rows` lays the old last row out again. (2) Arcs inside the cached tab
redraw it at 10 fps while working; make the running tools' `MarkView`s in `after_change`, keyed by tool id; time a
long conversation; fallback a still dot. (3) The SVG logos at 14 pt (Antigravity's mask, clip-path, blurs; Claude's
rays); opacity reaching the `img`; Antigravity's faded top on Latte. (4) Licensing: both marks are brand assets in a
public repo — your OK, plus a NOTICE. (5) The thinking fade restarts when the line moves to a new last row. (6)
Per-side borders and corners; `max_w` wrapping inside the `list`. (7) New Lucide icons: circle, circle-dot,
paperclip. (8) The composer stays full width while the chat caps at 760 (left alone, as asked).

**What was built (2026-11-21, on master, 14 commits `5ec0b41`…`3f82e6c`).** Plan:
[[Chores Plan - Agent Tab Redesign]], tasks 0–8, built in the session, one whole-branch review at the end.
- **Live context** (`5ec0b41`): see above.
- **Top bar** (`51a6836`, `19d70c1`): `src/agent_top.rs` (the pill, the meter's bands, the cost delta, narrow
  trimming, and `TopState`, which keys the animations by change counters); `agent_tab::top_line` draws spec A §C.
  The pill's mark is a `MarkView` on the Workspace's clock (`AgentTab::use_clock`).
- **Tool modal** (`233f087`, `ad1b5ed`): `src/agent_tool_modal.rs` (the stack, ↑/↓ among a level's tools,
  pruning when a tool goes, sizing, following the bottom, the header's right side); `agent_tab::modal_layer` draws
  spec A §B. Keys in context `AgentToolModal`. No Copy buttons (not a straight reuse).
- **`/clear` in the agent tab** (`91d1c10`): Claude Code 2.1.330 answers `/clear` with a `conversation_reset`
  line, then a new `system/init` under a new session id; fuxx ignored the line, so only the id changed. Now
  `Event::Cleared` empties the chat (and its cost and context) and the emptied chat fades in. agy has no `/clear`.
- **The shell tab's first `clear`** (`0339453`, your second report): macOS `clear` sends `ESC[3J` before
  `ESC[H ESC[2J`, and alacritty's 2J scrolls the screen into the scrollback the 3J had just emptied, so the old
  output stayed until a second `clear`. `shell_marks::FullClear` finds that pair (only cursor homes between) and
  `blocks::advance` empties the scrollback again where it ends; a plain 2J (zsh's ⌃L) keeps the scrollback. This
  also holds in the classic terminal view (what `clear`'s 3J asks for).
- **Logos and inactive tabs** (`51601e7`): `src/logos.rs` (sized and rasterised once per size, so the cached tab
  bar draws them on its first frame), `assets/logos/` with a NOTICE; `ProviderInfo.logo`; `tab_idle`,
  `tab_idle_hover`, `tab_bar::icon_opacity`.
- **The chat** (`b6b94f2`): `src/agent_chat.rs` (rows, gaps, groups, avatar, thinking line, the row print that
  lays a row out again when its neighbours change it), `ui_tokens::chat`, the drawing in `agent_tab`, `Sizes::CHAT`,
  Lucide circle, circle-dot and paperclip.
- **Scenes** (`967c94f`, `8892acf`): `agent-tab-modal`, `agent-tab-bar`, `agent-tab-thinking`; tab scenes expect
  the quieter tabs; `chrome-pixels-same` lets the pill's 200 ms blend end before it compares frames.

- **Usage beside the model pill** (`e069e7e`, your word: "take out usage from the agent tab from model popover to
  outside … (:pluss-icon [model] :usage-icon)"): the model menu ends with its models; a round gauge button after the
  model pill opens the usage panel over itself (and `/usage` still does); the limit-warning dot moved onto it.
- **"Cultivating…"** (`125f3d3`, your word): the pill's Working word and the line under the last row.
- **The whole-branch review** (opus, "with fixes") and its fix pass (`3f82e6c`): a click on the modal's dim area no
  longer reaches what is under it (it could press Stop); while the modal is open the tab's keyboard goes to it, and
  the box's Esc backs out of it, so Esc never stops the agent; a long tool opens at its top and follows the bottom
  only from there; each animation plays once, when its change happens, not again when the tab is shown; a modal
  closed because its tool went takes no keyboard from another pane; the "Task › " breadcrumb is one line. Deferred:
  the pill's breathing mark also redraws the tab while it Needs input (add to the CPU check); PageUp/PageDown in
  the modal; the shell `clear` fix inside a synchronized update; small duplications and doc wording.

**Calls made without asking** (the ledger's rulings, for you to check):
1. Context is live only once a finished turn has told fuxx the model's window: the first turn on a model after
   launch still updates at its end. The window is remembered per model for the app's run.
2. The pill's arc inside the agent tab redraws the tab at the clock's 10 fps while it works (GPUI redraws a moving
   view's parents). Fallback if it costs too much CPU: a still arc.
3. After `clear` the old screen isn't in the scrollback any more, in the classic view too (as Terminal.app does).
4. After `/clear` the emptied chat fades in (the old rows are gone at once; nothing is left to fade out).
5. Brand logos are never contrast-fitted (Claude's orange is ~2.6:1 on Latte); on dark themes every inactive tab's
   icon is at 0.75.
6. Gaps the spec left open: a notice or error, then the agent: 12; a tool after a todo or permission: 12. Notices
   and errors sit on the agent's text column.
7. The two brand marks are committed to this public repo with a NOTICE (Claude: Simple Icons CC0, Anthropic's
   trademark; Antigravity: from the Antigravity IDE, Google's trademark) — **your OK is asked before the push.**

**On-screen check** (I can't see the window):
1. Agent tab, working: the pill turns amber with its arc and says Cultivating…; Needs input; Done with its check
   popping once; Stopping….
2. Context moves during a turn (compare with Claude's `/context`); the cost changes at the turn's end with "+$…".
3. A context over 70 % and over 90 %: the meter's colour and the percent's.
4. Click a tool row: the modal opens (fade, small rise); Esc closes it and never stops the agent; ↑/↓ walk the tools
   and never reach the box's history; a click on the dim area closes it; ✕ closes it.
5. A Task with steps: open it, open a step (Back appears), Esc backs out, then closes.
6. A long Bash while the modal is open: the output follows at the bottom, stops following when you scroll up.
7. `/clear` in a Claude tab: the chat empties and fades back in empty; the cost and context go.
8. `clear` in a shell tab: the old output goes the first time; ⌃L keeps the scrollback.
9. The Claude and Antigravity logos on their tabs (dimmer while inactive on dark themes) and as the chat's avatar;
   Antigravity's arch on a light theme.
10. Inactive tabs a step quieter than the shown one, in dark and light.
11. The chat: your bubble, grouped tool rows (dots, the running arc, chevrons), the Tasks card, the permission card,
    the answered line, a notice, an error card, "Cultivating…" after your message and after a tool, the empty state.
12. CPU while an agent works for a while, and while one sits in Needs input (item 2 of the calls above).
13. The usage button after the model pill: its panel opens over it; the warning dot on it near a limit; `/usage`.
14. With the modal open: click the dim area over Stop (nothing stops); ⌘K then Esc (the modal closes, the agent
    works on); switch tabs and back (nothing replays its animation).

## Related

[[Proposal - Chat View and Agent Tabs]] · [[63 - Milestone 11l - Command Blocks]] ·
[[68 - Milestone 11q - Which Command Finished]] · [[79 - Milestone 11aa - Agent Tabs]] · [[40 - Roadmap]] ·
[[12 - Scope]]
