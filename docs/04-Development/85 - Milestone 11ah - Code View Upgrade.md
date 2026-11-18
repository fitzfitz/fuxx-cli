# Milestone 11ah — Code View Upgrade

**Status:** 🟨 built, **merged to master and pushed 2026-11-18**, waiting for your on-screen check. Design approved 2026-11-17 on your go-ahead ("we already design it, layout is also built before. just go ahead"); its three open questions settled as AI's calls (below). Plan: [[M11ah Plan - Code View Upgrade]] (Tasks 0–12, all built and reviewed). Anything here that does not quote you is marked **AI's call**.

**Asked for:** 2026-11-16, by you, in the file-view mockups ("everything, all layout good"), recorded in
[[Proposal - File View Upgrades]]:

> **M11ah — Code view upgrade (directions A + B + C, all chosen)**
> - **A · Calm and clear:** the clicked line highlighted (its number bright), quieter other numbers, faint indent
>   guides, rounded change bars, more spacing.
> - **B · Find your way:** a sticky line with the function or heading you're in, a breadcrumb, folding by indent
>   (▸ / ▾), marks on the scrollbar for changes and find matches, ↑ / ↓ to jump between changes.
> - **C · Review-first diffs:** each change a card titled with its function and its +/− counts, the exact changed
>   words marked inside a line, unchanged stretches folded ("⋯ 36 unchanged lines — click to show"), a side-by-side
>   option.

**Order (yours):** M11af → M11ag → **M11ah** → M11ai, with M11ak (agent box) and M11al (UI uplift 3) placed first.
M11ah ships after M11al, so its sizes follow M11al's "comfortable" spacing (see Constraints).

## Why

A file tab today shows every line the same: one long column of code, numbers equally loud, no sense of where you are
in a 900-line file, and a Diff mode that is the whole file with some rows tinted. When an agent changed six places
in `src/auth/session.rs`, you scroll and hunt.

M11ah makes the file tab answer three questions at a glance:

- **Where am I?** The line you clicked stands out; the function you're in stays on screen.
- **Where are the changes?** Marks on the scrollbar, and ↑ / ↓ to go from one to the next.
- **What changed?** In Diff, each change is a small card with its function's name and counts, the changed words
  marked, and the unchanged code folded away.

Who it's for: you, reading and reviewing what an agent wrote, in a read-only file tab.

## Goals

1. **Calm and clear (A).** The current line is highlighted and its number bright; other numbers are quieter; faint
   indent guides; change bars are rounded runs; rows have more room.
2. **Find your way (B).** A sticky line shows the function or heading you're in; a breadcrumb shows the path to it;
   blocks fold and unfold (▸ / ▾); the scrollbar carries marks for changes and find matches; ↑ / ↓ jump between
   changes.
3. **Review-first diffs (C).** Diff mode shows each change as a card titled with its function and `+n −n`; the
   changed words inside a changed line are marked; unchanged stretches fold to "⋯ 36 unchanged lines — click to show";
   a side-by-side option.
4. **Nothing else breaks.** Wrap, Find, live reload, M11af's glow, marks and Follow, big files, and Preview all keep
   working, as described per item below.

## Non-goals and anti-goals

- **Not in it:** editing of any kind; language servers, go-to-definition, symbol search, an outline panel (idea 4,
  "outline and peek", was not picked); a minimap; git blame; staging or reverting a hunk from a card (git writes);
  comparing with anything but the last commit; word marks in File mode; keeping folds across a restart; a spacing
  setting; anything in Markdown Preview (that is M11ai).
- **Anti-goals (a failure even if it works):**
  - **Idle CPU goes up.** Nothing new runs while you just look at a file (M11p, note 77).
  - **A fold hides a change.** A folded block that holds a changed, glowing or marked line must say so.
  - **The view jumps by itself.** A reload, a breadcrumb or a sticky line must never move the code under you.
  - **A slow open.** A big file must open as fast as today; structure arrives after the text, never before it.
  - **Busy chrome.** The calm look is the point: new parts are faint until you need them.

## Constraints

- Read-only, offline, no new services, no language servers ([[12 - Scope]]).
- Builds on the file tab as it is (`src/file_tab.rs` module docs; notes 78, 80, 82): one canvas draws the lines in
  sight; reads, colours, git and Find run off the UI thread and carry a `version`; Wrap is a list of screen rows
  (`Wrapped`); the overlay scrollbar is `src/scrollbar.rs`; Diff is `src/diff_view.rs` rows over a whole-file diff
  (`context_for`: every line as context).
- **M11p (note 77):** nothing visible changes inside `render`; no new always-on timer; the tab redraws only when what
  it shows changed. No animations in M11ah (folds open and close at once, AI's call), so no new frame tasks.
- **M11af (note 82) keeps working:** the glow, the age words, the mark column and Follow, in File mode.
- **Sizes follow M11al.** Every new size is a named constant in one place (a `code` group in `src/ui_tokens.rs`,
  AI's call), so M11al's roomier spacing can move them together. The numbers below are starting points.
- **No new crate.** Checked against the source and Context7 on 2026-11-17:
  - `similar` 3.2.1 (already a dependency, `default-features = false, features = ["std"]`) needs its **`inline`**
    feature turned on for word marks: `TextDiff::iter_inline_changes_with_options_deadline` with
    `InlineChangeOptions` (`InlineChangeMode::Words` or `Chars`, a deadline). A feature flag, not a new crate.
  - `syntect` 5.3 (already used) gives the scopes for "named" lines through `ParseState::parse_line` and
    `ScopeStack`, beside `HighlightState` for colours, in the same pass.
- Limits stay: over 2 MB a file is plain text (`PlainTooLarge`), over 20 MB refused (`highlight.rs`).
- On-screen tests with `FUXX_DAEMON_SOCKET=/tmp/fuxx-dev.sock cargo run` (debug, never `--release`).

## Approaches

### 1. Where the structure comes from (sticky line, breadcrumb, folding, card titles)

- **A. Indentation only.** A line opens a block when the next non-blank line is indented deeper. Works for every
  language and for plain text, costs one pass, needs no syntax. But every block counts: the sticky line and card
  titles can read `if token.expired() {` instead of `fn refresh`.
- **B. Syntax scopes only.** Ask syntect which lines name something (`entity.name.function`, `entity.name.class`,
  `entity.name.type`, `markup.heading`, …). Gives good names, but only for files that get colours (≤ 2 MB, a known
  syntax), quality varies by language, and syntect gives names, not where blocks end, so folding still needs
  something else.
- **C. Both: indentation for the shape, syntax for the names (AI's pick).** Indentation decides the blocks (folding,
  guides, nesting). During the colouring pass, which already parses each line, fuxx also notes which lines carry a
  "name" scope. The sticky line, the breadcrumb and card titles use the innermost **named** block; if none is named
  (plain text, an unknown syntax, a file over 2 MB) they fall back to the innermost block of any kind. Markdown is
  the one exception: its shape comes from `#` headings, not indentation.

### 2. Showing folds and cards on screen

- **A. One row map for everything (AI's pick).** Today a screen row is a line, or with Wrap a part of a line. M11ah
  makes the row list always present: each row is a part of a line, a fold's "⋯" row, or a card's title row. Painting,
  clicks, selection, scrolling, the scrollbar, Find's reveal, change bars and Follow all go through it. The text,
  line numbers, Find and Copy stay on the real lines, untouched. With no folds, no Wrap and no cards it is the
  identity, as today.
- **B. Build a shorter text with the folded lines left out.** Simpler to draw, but line numbers, Find, Copy,
  selection and M11af's line record would all need translating. Dropped.

### 3. Side by side

- **A. The same canvas, paired rows (AI's pick).** A side-by-side row holds an old line on the left and a new line on
  the right (either may be empty); one scroll, one row map, two gutters, a hairline between. Cards and folds work the
  same in both layouts.
- **B. Two canvases with their scrolls tied together.** Each half is the existing view, but every scroll, fold and
  reveal must be mirrored, and they drift with Wrap. Dropped.

### 4. Changed words

- **A. `similar`'s inline changes (AI's pick).** Turn on its `inline` feature; it pairs removed and added lines and
  gives the changed words in each, with a deadline. Already in the tree.
- **B. A word diff of our own.** No feature flag, but more code to own for the same result. Dropped.

## The design

### A · Calm and clear

**The current line**
- The current line is the line of your last click in the text (a press without a drag; for a drag, where it ends).
  A ⌥↑ / ⌥↓ jump, a Find step and a breadcrumb or sticky-line click also make their line current (AI's call), so you
  always see where you landed.
- It gets a full-width band behind the text: `text` at 5 % over the background (AI's call; under the selection, find
  matches, the M11af glow and change tints, which all stay readable on it). Its line number is drawn in `text`.
- **Quieter numbers:** every other number in `text_dim`, kept at least 3:1 against the background (`MARK_MIN`, AI's
  call), so they stay readable.
- None when the tab opens; the band stays until you click elsewhere. Esc does not clear it (Esc closes Find).
- In Diff mode the current row works the same, on the side you clicked in side by side.
- Kept over a reload: it follows its line as M11af's marks do (the reload's line comparison); a line that is gone
  clears it (AI's call).

**Indent guides**
- One 1 pt vertical line at each indent level, in `text` at 7 % (AI's call). The guide of the block the current line
  is in is a little stronger, 14 % (AI's call), so you see the block you are in.
- The indent step is worked out per file (the most common step between nested lines: usually 2 or 4 cells; tabs
  count as 4, as today). Guides run through blank lines inside a block.
- With Wrap, a line's continuation rows carry its guides too. In Diff mode, guides are drawn as well (on both halves
  in side by side). Not drawn for a file with no structure (AI's call).

**Rounded change bars**
- A run of lines with the same change (added, changed) is one bar, 3 pt wide with fully rounded ends, instead of a
  square bar per line. Two different runs next to each other keep a 2 pt gap (AI's call). The "lines deleted here"
  mark stays a small ▸, with rounded corners.
- A press on a bar still opens Diff at that change (M11h), now at its card.

**More spacing** (AI's call: numbers below, all in the `code` group so M11al can move them)
- Row height: the font's line height × 1.2, text centred in the row (today × 1.0).
- Space above the first line: 16 pt (today 12). Left of the numbers: 16 pt (12). Numbers to bar: 10 pt (8). Bar to
  text: 14 pt (10).
- The terminal's font size and font still decide the text; only the room around it grows.

### B · Find your way

**Where blocks come from (approach 1C)**
- Worked out **off the UI thread with each read**, as the M11af comparison is, into an outline: for each line, its
  indent, the block it is in, and where each block ends. A block that ends just before a closing line at the
  opener's indent (`}`, `)`, `]`, `end`, `</…>`) takes that line in too, so a fold reads `fn refresh() { ⋯ }`
  (AI's call).
- The "named" lines come from the colouring pass (syntect scopes). They arrive after the outline; until then, and for
  plain text, the fallback is used.
- Markdown in File mode: a heading's block runs to the next heading of the same or a higher level.
- **Limit (AI's call):** if the outline takes longer than 50 ms (a huge file with no structure), that read gets none:
  no guides, folds, sticky line or breadcrumb; everything else works.

**The sticky line**
- When the top of the view is inside a named block whose first line has scrolled above the top, that first line is
  drawn pinned at the top of the text, with its number, on the background, and a 1 pt `divider` line under it.
- **One line only, the innermost** (AI's call; the breadcrumb shows the outer ones).
- Clicking it scrolls back to that line (it becomes the top line and current).
- With Wrap, it is one row, cut with "…". The text under it is not covered: scrolling keeps one row of room for it so
  nothing is hidden behind it (AI's call).
- File mode only. In Diff mode the card's title row is sticky instead (C below).

**The breadcrumb**
- A strip 26 pt tall (AI's call) between the path bar (and the find bar, when open) and the text: the file's name,
  then each block you are in, outer to inner: `session.rs › impl Session › fn refresh`. 12 pt UI text in `text_dim`,
  the last part in `text`.
- A part's words: its first line, trimmed, with a trailing `{`, `:` or `=>` dropped, cut at 32 characters with "…";
  a Markdown heading without its `#`s (AI's call).
- Clicking a part scrolls to its line (the top line, current); the file's name goes to the top.
- It follows the **top of the view**, not the current line, so it changes as you scroll (worked out when the scroll
  changes, not while drawing).
- Always shown in File mode for a file with structure, so the text never moves when it fills in. When narrow, the
  outer parts collapse into "…" first; the innermost stays.
- Not shown in Diff (cards have titles), in Preview, or for a file without structure (AI's call).

**Folding**
- A fold column, 14 pt, between the line numbers and the M11af mark column, shown in File mode when the file has
  blocks. A block's first line shows **▾** (open) faintly while the pointer is over the gutter, and **▸** (folded)
  always.
- Click ▾ to fold: the block's first line stays, the rest is hidden, and a pill after its text says **"⋯ 24 lines"**.
  Click ▸ or the pill to unfold.
- Keys (AI's call): **⌥⌘[** folds the block the current line is in, **⌥⌘]** unfolds it. The palette gets **Fold All**
  and **Unfold All**.
- **A fold never hides a change:** if the hidden lines hold a git change, the change bar is drawn on the folded line;
  if they hold M11af glow or marks, the folded line takes the warmest glow and a mark in the mark column (AI's call).
- Line numbers skip the hidden lines (12, then 37). Copy of a selection over a fold copies the real lines, hidden
  ones included.
- Kept over a reload (each fold follows its first line, as marks do); a fold whose first line changed opens (AI's
  call). Folds are per tab, not kept over a restart (AI's call).

**Scrollbar marks**
- A 4 pt lane at the text's right edge, under the M11ae thumb: git changes as short bars in the change colours; find
  matches as 2 pt ticks in the match colour, the current match brighter.
- **Always shown** when there is something to mark, even while the thumb is faded (AI's call: they are drawn once
  per change, cost nothing idle). A mark is at least 2 pt tall; marks on the same point merge.
- Positions follow the row map (folds and Wrap included). They are worked out when the changes, the matches, the
  folds or the wrap width change, kept, and only read while drawing.
- Clicking the lane jumps as a click on the track does today.
- In Diff mode, one mark per card (green, red, or blue for both). M11af's glow is not on the lane (AI's call: the
  proposal names changes and matches).

**⌥↑ / ⌥↓ between changes**
- **↑** goes to the previous change, **↓** to the next, from the current line (none: from the top of the view). A
  change is a run of changed lines (File mode, git's change bars) or a card (Diff mode).
- The change lands 3 lines below the top (as a diff opens, `REVEAL_ABOVE`), below the sticky line, and its first line
  becomes current. A change inside a fold opens that fold.
- Two 24 pt icon buttons, ↑ and ↓, in the bar left of the view switch, shown when the file has a change; tooltips
  "Previous change (↑)" and "Next change (↓)". At the first or last change the button is faint and the key does
  nothing (AI's call: no wrap-around).
- The arrows act only when the tab has the keyboard (not the find box). Plain ↑ / ↓ do nothing in a file tab today, and stay unbound (call 1 below).
- A jump turns Follow off (it is you moving the view, as in note 82).

### C · Review-first diffs

**Cards**
- Diff mode groups the changed rows into **cards**: each run of added and removed rows, with 3 unchanged lines above
  and below (git's usual context; AI's call). Two runs whose context would touch share a card.
- A card is a rounded panel (`CARD_RADIUS`), inset 12 pt from the text's sides, 1 pt `divider` border, 10 pt between
  cards (AI's call).
- Its **title row**, 30 pt: the innermost named block the change is in (from the new file; for a change that only
  removes, from the old file): `fn refresh`, then `+12 −3` in the added and removed colours, and on the right
  `lines 40–58` in `text_dim`. A change outside any block reads **"Top level"** (AI's call).
- A new file is one card, **"New file · +120"**; a deleted file **"Deleted file · −80"**, with nothing folded.
- While a card is taller than the view, its title row is sticky at the top until the card scrolls away.
- Clicking the title folds the card to its title; clicking again opens it (AI's call).

**Unchanged stretches folded**
- Between cards, and before the first and after the last, unchanged lines fold into one row: **"⋯ 36 unchanged lines —
  click to show"**, centred, `text_dim`, 12 pt UI text.
- Only stretches of **8 or more** lines fold; shorter ones are just shown (AI's call: a fold that saves 3 lines is
  noise).
- Click shows the whole stretch. The palette's **Show All Unchanged Lines** opens every one (AI's call); leaving and
  coming back to Diff folds them again.
- Kept over a reload when the stretch starts on the same new-file line; otherwise folded again (AI's call).

**Changed words**
- In a card, removed and added lines that replace each other are paired (`similar`'s inline changes, by words), and
  the words that differ get a stronger tint with 2 pt rounded corners: removed words red at about 35 %, added green
  at about 35 %, over the row's usual light tint (AI's call; the builder tunes the alphas).
- Worked out off the UI thread with the diff, given up after 50 ms (the rows still show; just no word marks). Lines
  over 1,000 cells (`LONG_LINE`) and lines that share less than half their words are not paired (AI's call: a
  rewritten line is better read whole).
- In unified and side-by-side alike.

**Side by side**
- An icon button in the bar, **Side by side** (two columns), shown in Diff mode only, filled when on, like Wrap.
- Left: the old file, with its numbers; right: the new file, with its numbers; a 1 pt hairline between. Unchanged
  lines show on both sides. Removed and added rows that replace each other sit level; extra rows face an empty,
  faintly shaded space.
- Card titles and "⋯ unchanged" rows span both halves.
- **Too narrow:** it needs at least 60 cells per half. Narrower, the view shows unified, the button is faint with the
  tooltip "Too narrow for side by side", and the choice comes back when the pane is wide again (AI's call).
- **Off by default** and kept per tab, saved with the tab as its mode is (AI's call; see Questions).
- Selection stays in the half where the press started; Copy copies that half's text. Find matches show on both halves;
  stepping goes row by row, left before right.

### How each part works with the rest

| | Wrap | Find | Live reload | M11af (glow, marks, Follow) | Big files | Diff / Preview |
| --- | --- | --- | --- | --- | --- | --- |
| **Current line** | band on all its rows | a step makes the match's line current | follows its line | drawn under the glow | as today | Diff: yes; Preview: no |
| **Indent guides** | on continuation rows too | — | from the new outline | — | none past the 50 ms limit | Diff: yes; Preview: no |
| **Rounded bars** | a run spans its rows | — | as today's bars | the mark column stays between numbers and bars | none over 20 MB (as today) | File only (Diff has signs) |
| **Sticky line** | one row, cut with "…" | a match under it is scrolled clear of it | rechecked on the new outline | Follow reveals below it | needs the outline | File only; Diff uses sticky card titles |
| **Breadcrumb** | — | — | rechecked | — | needs the outline | File only |
| **Folding** | a folded line is one row | counts hidden matches; a step into a fold opens it | folds follow their first line | folded line shows the warmest glow and a mark; Follow opens a fold the agent changed | needs the outline | File (blocks); Diff (unchanged stretches, cards) |
| **Scrollbar marks** | placed by screen row | ticks for matches (up to the 10,000 cap) | moved when changes come | — | as git's bars allow | File and Diff; Preview keeps a plain thumb |
| **⌥↑ / ⌥↓** | — | — | the list of changes is the new one | turns Follow off | as git's bars allow | File: change runs; Diff: cards |
| **Cards, folded stretches** | rows wrap inside cards | counts hidden; a step opens it | rebuilt with each new diff | Diff shows no glow (as today) | cards at any size git's diff is accepted; word marks only within the 50 ms limit | Diff only |
| **Side by side** | each half wraps at its own width; the shorter side padded | both halves | rebuilt | — | as unified | Diff only |

Two notes on M11af:
- **Follow** reveals the agent's change 3 lines below the sticky line, opening a fold if the change is inside one.
  Folding, unfolding and clicking the breadcrumb are you moving the view: they turn Follow off (AI's call).
- **The mark column** stays where it is; the fold column sits left of it (numbers · fold · marks · bar · text).

## AI's calls you may most want to overrule

- ~~Plain ↑ / ↓ jump between changes~~ — superseded: jumps are **⌥↑ / ⌥↓**; plain ↑ / ↓ stay unbound (call 1).
- The breadcrumb is **its own 26 pt strip**, always shown for a file with structure.
- Side by side is **off by default, per tab**.
- **One** sticky line (the innermost), not a stack.
- Unchanged stretches fold only from **8** lines; cards keep **3** lines of context.
- Folds are **not kept** over a restart.
- Row height **× 1.2**, before M11al's numbers.

## Calls on the open questions (AI's, on your go-ahead — for you to check)

1. **Jumping between changes is ⌥↑ / ⌥↓** (plus the two bar buttons); plain ↑ / ↓ stay as they are (unbound in a
   file tab: they do nothing there). Reason: the jumps get keys of their own, so no key you already press changes
   meaning, and plain arrows stay free for later.
2. **Side by side is off until you switch it on, and kept per tab** (no global setting yet). Reason: most panes are
   too narrow for two columns; turning it on is one click.
3. **The breadcrumb gets its own strip above the code**, shown for files with structure — the path bar keeps its
   folders. Reason: you asked for a roomier, comfortable view; the strip is clickable and doesn't crowd the path.

## Left to the builder

- Where the outline, row map and card grouping live (likely small new modules beside `diff_view.rs`), and how the
  colouring pass also returns the named lines (`ParseState` + `HighlightState` in place of `HighlightLines`).
- How the row map replaces `Wrapped` (keyed by mode, version, width, folds) and stays a cache of layout, as `Wrapped`
  is today, never a change of what is shown made while drawing.
- The exact alphas, the indent step rule, the `similar` mode (`Words` first; `Chars` if words read too coarse).
- How side-by-side rows pair inside a card (removed and added in order, as git lists them).
- Tests: the outline (indentation, closers, Markdown headings, the fallback), the row map with folds, cards and Wrap,
  card grouping and titles, word marks, side-by-side pairing, next / previous change, scrollbar mark positions and
  merging, folds and the current line following a reload — all pure and unit-tested. ui-shots scenes for each part,
  light and dark, unified and side by side, with M11af's glow and Follow on a folded file.
- Measure idle CPU with a file tab open (as M11af's item 12): it must match master.

## What was built

All 13 plan tasks, each reviewed, on `m11ah-code-view`, merged to master and pushed 2026-11-18.

- **Task 0, baselines:** the numbers this branch must not exceed (tests, clippy, ui-shots).
- **Task 1, sizes and the current line:** every size in one `code` group in `ui_tokens.rs`; rows 1.2 times the font's line height; quieter line numbers; a click makes a line current (faint band, bright number).
- **Task 2, the outline:** the file's blocks from indentation (a Markdown file: from headings), and the colouring pass names the lines that open functions, types and headings. Worked out with each read, off the UI thread, given up after 50 ms.
- **Task 3, guides and bars:** faint indent guides (the current block's stronger), change bars as rounded runs with a small gap between kinds, a bar spanning a wrapped line's rows.
- **Task 4, the row map:** one map of screen rows (replaces the old wrap cache) that knows hidden lines, card titles, folded stretches and side-by-side pairs, so a click always lands on the line drawn there.
- **Task 5, folding:** ▸ / ▾ in a fold column, a pill ("⋯ 24 lines"), ⌥⌘[ and ⌥⌘], Fold All / Unfold All in the palette. A fold never hides a change.
- **Task 6, sticky line and breadcrumb:** a 26 pt strip with the file name and the blocks you are in, and the innermost block pinned over the first row; a click on either puts that line at the top.
- **Task 7, scrollbar marks:** a thin lane with change marks and find ticks.
- **Task 8, jumps between changes:** ⌥↓ / ⌥↑ and two small buttons; they open a fold around the target and turn Follow off.
- **Task 9, Diff cards:** each change a card with a title (`fn refresh` · `+12 −3` · `lines 40–58`), 3 lines of context, unchanged stretches of 8 or more folded into "⋯ 36 unchanged lines — click to show", "New file" / "Deleted file" cards, a pinned title, three palette entries.
- **Task 10, changed words:** the exact changed words tinted inside a changed line (`similar`'s `inline` feature on, no new crate).
- **Task 11, side by side:** a button (off by default, kept per tab, saved in the layout) that draws a diff as two equal halves; needs 60 cells per half, otherwise the button says "Too narrow for side by side".
- **Task 12, this hand-off:** the docs, the headless outline timing, the dead-code allowance removed from `code_outline.rs`.
- **Final review fixes:** a Find step that opens a fold or a folded stretch now lands the match on screen (it was held to the shorter rows' scroll limit); the scrollbar's change marks are worked out from the changes and the folds, not every row, and only when the rows or the changes change (a Find keystroke, ⌘G or a resize only rescales them and redraws the ticks); the outline ignores indent jumps wider than 64 cells when it picks the step (no memory that grows with a line's indent); the last dead-code allowance (`ui_tokens::code`) removed.

### Calls made while building (AI's, for you to check)

- Jumps are ⌥↑ / ⌥↓ with tooltips to match; plain ↑ / ↓ stay unbound in a file tab, as the spec's "Calls on the open questions" say (its body text said plain arrows).
- Docs for the whole milestone were written once, in the last task, not per task.
- The part names in the breadcrumb are cut at 32 characters (the spec's number); the plan's 33-character example was a mistake.
- The sticky line is the innermost named block whose first line has scrolled above the top.
- Markdown files draw no indent guides (their blocks are headings, not indentation).
- The row map remembers its own row height and is rebuilt when the font size changes.
- The sticky line overlays the top row while you scroll freely, instead of pushing the text down a row (that would make the view jump by itself); reveals (Find, Follow, jumps) leave one row of room.
- A click on the sticky line or a breadcrumb puts that line on top, below any sticky line pinned there.
- A resize or font-size change keeps the line at the top of the view, so the sticky line and breadcrumb stay right.
- A find match inside a fold ticks the lane at the fold's row; a deletion-only edge gets a lane mark too; no lane when the text fits.
- A current line hidden by a fold moves to the fold's line; ⌥⌘[ on an already folded block folds the next block out; one hidden line says "⋯ 1 line".
- A deletion edge touching a changed run is one stop for the jump, as git shows it as one change.
- Card gap sits above every card but the first; "lines a–b" spans the card including context; a card's title pins whenever its card is under the top of the view, and pressing it folds that card; the lane shows find ticks in Diff too; cards you closed stay closed over a reload.
- Show All Unchanged Lines keeps your top line.
- Changed words use `similar`'s character mode, each changed range grown to the whole word it touches (its word mode splits on spaces only and would mark `old();` whole).
- Both side-by-side halves use the wider of the two line-number columns, so they are equal width and wrap at the same column.
- A change in whether side by side fits notifies once through one deferred notify (no timer, no frame task); entering Diff from File uses the last measured fit.
- A deletion edge just before a hidden line draws its mark, and is hit, on the fold's row (its ▸ line).
- Diff's current row is cleared when a new diff arrives (it is not carried over).
- The lane's change marks look as before; with Wrap on, a fold holding only deletions now gets one mark at its line's first row instead of one per wrapped row.
- When the outline picks the indent step, a jump of more than 64 cells is not counted (a generated or broken line).

### Known small gaps (things you might notice)

- With Wrap on, a fold's pill can run past the right edge, and a tab at the wrap boundary can offset the pill's click spot by a cell.
- A reload can leave the current line inside a fold with no band showing.
- Card titles and the "⋯ N unchanged lines" rows have no pointing-hand cursor.
- A pure insertion (`café` to `cafés`) marks only the new side's word.
- A deletion inside a fold whose strongest change is an addition has no separate lane mark.
- In side by side, a selection made in unified view and carried over is invisible but copies both halves mixed; the sideways scrollbar's track spans both halves.
- A diff of a file emptied entirely reads "Deleted file".
- Pressing a pinned card title or a fold's top half can prefer a hidden change over a deletion edge above it.
- Folds are not kept over a restart (as designed).
- A big file whose outline takes close to the 50 ms limit can gain or lose its structure (breadcrumb strip, fold column, folds) from one reload to the next.
- A press on the sticky line or a breadcrumb part whose row above is outside the target's block lands the target one row below the top, with no sticky line over it.

## Measured headlessly (outline time)

Outline time, headless: `code_outline::outline` on this branch, debug build, run once from a temporary ignored test that was not committed (text generated in memory):

| Input | Lines | With the 50 ms `LIMIT` | Without a limit |
|---|---|---|---|
| 15 MB flat text | 247,497 | 17 ms, outline built | 14 ms |
| 15 MB indented text (6 levels) | 479,993 | 52 ms, **gave up** (no outline) | 80 ms |
| 1.9 MB source-like | 137,304 | 28 ms, outline built | 27 ms |

So the outline adds at most about 50 ms to a read and a file that is too slow simply gets no outline. Idle CPU and tab-open-to-first-frame need a real window: they are items 13 and 14 below.

## Final numbers (this Mac, debug build, 2026-11-18)

- `cargo test --bin fuxx`: 1,920 passed, 0 failed, 1 ignored (baseline 1,840), after the final review's fixes.
- clippy: 14 (bin) / 27 (test) default, 16 / 29 with `ui-shots` (baselines 14 / 27 and 16 / 29; unchanged); `cargo fmt --check` clean.
- `scripts/ui-shots.sh` (full, after the final review's fixes): 159 passed, 1 failed, 140.4 s: the failure is the known clock-dependent `chrome-agent-status`, which passed when run alone. The reference fingerprints are stale on this Mac (they say "look different" and never fail); not updated.

## On-screen check (yours, after the build)

Run `FUXX_DAEMON_SOCKET=/tmp/fuxx-dev.sock cargo run` (debug only: **do not add `--release`**) from the worktree. Use a scratch repo (`git init`, commit a 900-line Rust file such as `~/code/my-app/src/auth/session.rs`, then change six places in it) and open it in a file tab.

1. **Current line:** click a line: a faint band across it and its number bright; the other numbers quieter.
2. **Guides and bars:** faint vertical guides at each indent level, the block you are in a little stronger; change bars are rounded with a small gap between an added and a changed run.
3. **Sticky line and breadcrumb:** scroll into a long function: its first line pins at the top, the strip above reads like `session.rs › impl Session › fn refresh`; click a crumb and the pinned line: each puts that line at the top and current.
4. **Folding:** hover the gutter, click ▾ to fold a function ("⋯ 24 lines"), click ▸ to open; ⌥⌘[ and ⌥⌘] fold and unfold where the current line is; fold a block holding a change: its bar still shows on the folded line.
5. **Jumps:** ⌥↓ / ⌥↑ and the two buttons step between changes, open a fold around the target, stop at the ends (no wrap-around).
6. **Scrollbar marks:** a thin lane at the right edge with your changes; open Find: match ticks join it.
7. **Diff cards:** switch to Diff: one card per change with `fn refresh · +12 −3 · lines 40–58`, 3 lines of context, a long unchanged stretch as "⋯ 36 unchanged lines — click to show"; click it to open it; scroll a tall card: its title pins; fold a card by its title.
8. **Changed words:** in a changed line only the words that changed are tinted more strongly (try an accent such as `café` and a tab-indented line).
9. **Side by side:** the button in the bar: two equal halves, old left and new right, when the pane is wide enough (60 cells per half); in a narrow pane the button is greyed ("Too narrow for side by side").
10. **Follow with folds:** fold a function, turn Follow on, have an agent change a line inside it: the fold opens and the change shows below the sticky line.
11. **Wrap:** turn Wrap on and repeat a click below a fold and below a card title: it lands on the line drawn there.
12. **Palette:** Fold All, Unfold All and Show All Unchanged Lines each do what they say.
13. **Idle CPU (the M11p anti-goal), by hand:** with the 900-line Rust file shown and nothing warm, run `top -l 6 -s 5 -pid <fuxx pid>` in another terminal and take the median CPU of the samples; do the same with `master` built the same way (debug, `FUXX_DAEMON_SOCKET=/tmp/fuxx-dev.sock cargo run`). They should match within noise.
14. **Open time, by hand:** open a 15 MB generated text file and a 1.9 MB source file, branch and `master`: the first text should appear as fast as before (the outline adds up to 50 ms, see the headless table above).
15. **Side by side survives a restart:** switch it on in a diff, quit and start fuxx again: the tab is still side by side (the saved layout's round trip is tested; the wiring from the button to the saved file is not covered by a test).

## Related

[[Proposal - File View Upgrades]] · [[82 - Milestone 11af - Live Changes]] · [[80 - Milestone 11ae - File Tab Chores]] ·
[[78 - Milestone 11ac - File Tab Polish]] · [[77 - Milestone 11p - Chrome Caching]] ·
[[59 - Milestones 11g-11h - Right Sidebar and File Tabs]] · [[84 - Milestone 11al - UI Uplift 3]] · [[12 - Scope]] ·
[[40 - Roadmap]]
