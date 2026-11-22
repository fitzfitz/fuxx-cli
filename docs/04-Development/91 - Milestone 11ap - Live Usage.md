---
tags: [milestone, m11ap]
---

# 91 - Milestone 11ap - Live Usage

**Status:** 🟨 built (2026-11-21), on master, not pushed; waiting for your on-screen check. Added on your words: "is it possible to get claude and antigravity usages
live without doing their command … in orca they have something like this and its cool", then "push then we do
this, this should be fast" (my recommendation: Claude's live stream + a background `/usage` read, an Orca-style
strip in the status bar with a popover; the lead designer's spec below).

## What was found

- **Claude** reports usage live in every turn: its `rate_limit_event` carries `unifiedWindows` (`five_hour`,
  `seven_day`: `utilization`, `resetsAt`, `status`). fuxx read only the warning state. Per-model weekly limits
  (e.g. "Current week (Fable)") come only from `claude -p /usage`.
- **Antigravity**'s stream carries no usage; only `agy -p /usage --output-format json` (groups of buckets, % left,
  resets).
- Ruled out: calling the providers' private usage web endpoints with the CLIs' stored logins (undocumented, and
  fuxx would read your login tokens).

## Design (the lead designer's spec, 2026-11-21)

- **One rule:** every usage figure shows the percent **used** (Antigravity: `100 − left`); bands as the agent tab's
  context meter: under 70 normal, 70–89 caution, 90+ critical.
- **Status-bar strip** (`status_bar.rs`, right side, before the port chips): per agent its logo (12), `77%`, the reset
  in its largest unit (`23h`, `5d`, `51m`, `now`; none for a reset in words), 10 apart, 12 pt tabular. The number is
  the agent's **tightest** limit (highest used; ties: the earlier reset; a live value newer than the reading wins).
  % in `text_dim` / caution / critical by band. Fixed order Claude, Antigravity; only agents that can be read
  (`usage_probe`) or have a reading; none: no width. States: a value (kept while reading or after a failure); `…`
  reading with no value; `–` couldn't read with no value. One target (16 high, chip padding and radius), `row_hover`
  on hover, `row_active` while open; click toggles the popover; tooltip "Claude 77% used · resets in 23h 1m ·
  Antigravity …". Narrow: drops the resets, then shows one agent, then hides (in `fit_status_bar`'s order: update
  version, ports collapse, **resets**, branch cut, folder cut, **one agent**, word, folder, branch, **hidden**, ports
  none). Follows Settings → Status Bar; background reads only while it shows.
- **Popover:** above the strip (6 over the bar), right edges aligned, 8 from the window edge; the overlay shell,
  300 wide, scrolls past 480; closes on a click outside, Esc, or the strip; never takes the terminal's focus. Header
  (28): "Usage" (13 MEDIUM), "Updated just now / 4m ago" (META, dim), a spacer, a 22 refresh button (`Reload` 14,
  hover fill, "Refresh usage"). Per agent, always open: a heading (logo 14, name, "Couldn't refresh" at the right
  when the last read failed), then the agent's `usage_rows` drawn by the box panel's renderers; a 1 px line between.
- **One look for limits:** `limit_row`, `heading`, `quiet`, `top_row` move to `src/usage_view.rs`, used by the box's
  panel and the popover. `limit_row`: "N% used", the bar fills the used share in the band's colour on a `track`
  (`UsageInk::of(theme)`), the % in its band's colour; the reset and note lines stay. Antigravity's bucket names lose
  a trailing " Remaining".
- **Motion:** the refresh icon turns (800 ms a turn) only while a read runs (reads have a deadline); bars glide 400 ms
  when a value changes while the popover is open; nothing else moves.
- **Risks:** the strip's width changing with values; Antigravity's tightest bucket may be a group you don't use;
  "77%" alone could read as left (tooltip, popover say used); the box panel now says "% used".

## Plan (built in the session; per-task gate as AGENTS.md)

1. **Data** ✅ (`ad7f394`): Claude's every window with its use (`RateLimit.used_pct`); `Catalogs::live_usage` (the
   last good reading, kept while the next is read, with newer live windows over it); `refresh_due` and the
   Workspace's background read every 5 min while the window is seen.
2. **One look for limits** ✅ (`122e1fb`): `src/usage_view.rs` ("% used", band colours, `UsageInk`), agy's names
   without " Remaining"; the box panel uses it and `live_usage` (it keeps its numbers through a failed or running read).
3. **The strip** and 4. **the popover** ✅ (`2cd4866`): `src/usage_strip.rs` (the tightest limit, the short reset, which
   agents show, the tooltip); `status_bar.rs` measures, fits and draws the strip and `usage_popover`; the Workspace
   redraws on a usage change. Scene `status-bar-usage` (the strip, the popover open).
5. Full ui-shots 269 / 269; one review (below); docs.

## The review (opus, "with fixes") and its fix pass (`920b3fa`, `HEAD`)

Fixed, each test-first: (1) on an ordinary turn Claude's line says its own window's use only in `unifiedWindows`, so
the live **session** number was lost — now taken from there; (2) a press on the strip couldn't close the popover (its
"outside" press runs first) — the strip toggles on the press and the popover ignores presses on it; (3) Esc now closes
the popover (caught before the terminal, only while it is open); (4) no background reads while the status bar is
off; (5) a limit whose reset has passed counts as unused, and kept numbers after a failed read say "Couldn't refresh ·
updated Nm ago" in the box panel. Scene `status-bar-usage` now clicks: open, close, Esc, a press elsewhere.

Deferred (minor): an agent that never read shows "…" briefly at each 5-minute retry; a tie between agents goes to the
later one; "Updated … ago" and the strip's resets refresh only when something redraws (up to 5 min late); the
refresh button does nothing for a reading under a minute old; the popover's open flag outlives the strip being cut
away (narrow window); an unknown `unifiedWindows` kind is listed under its raw key; the 400 ms bar glide isn't
built.

## Calls made without asking

1. An agent whose read never worked (most likely not installed) isn't in the strip, rather than showing "–"
   forever (the spec's "–" for "couldn't read, no value").
2. The popover never takes the keyboard; Esc still closes it (caught only while it is open).
4. A failed or running read keeps the last numbers in the box panel too (no more "Couldn't read usage" once a reading
   worked); the popover says "Couldn't refresh" beside the agent.
5. The refresh button reads again only what is more than a minute old (the existing freshness rule).
6. Each Claude read runs `claude -p /usage`, which starts a short Claude process every 5 min and runs your
   SessionStart hooks.

## On-screen check

1. Start fuxx: within a few seconds the strip shows Claude (and Antigravity, if installed) with a percent and a reset.
2. Run a Claude turn in an agent tab: the strip's Claude percent follows the stream (its session/week windows).
3. Click the strip: the popover opens above it; each agent's limits as "% used"; "Updated … ago"; the refresh turns
   while it reads; a click outside or on the strip closes it.
4. Narrow the window: the resets go first, then one agent, then the strip.
5. The agent box's usage button: "% used" with the band colours; the same numbers as the popover.
6. CPU: idle with the strip shown (a read every 5 min only).

## Fix after M11ap: a working workspace showed ✓ (2026-11-21)

Your report: "it still in progress but sidebar indicator says ✓". Cause: a sidebar row (and a tab's roll-up) shows
its tabs' most urgent status, and the order since M7 / M11b was **Waiting > Done > Working > Idle**, so a finished
agent tab beside a working terminal put ✓ on the row. Changed on your word to **Waiting > Working > Done > Idle**
(`groups::urgency`): what needs you, then what still runs, then what finished — as the closed group header already
ranked since 2026-11-17. Tests: `rolled_up_picks_the_most_urgent`, `a_working_terminal_beats_a_finished_agent_tab`,
`rollup_is_the_most_urgent_terminal`; the `workspaces` scene. On-screen: a workspace with a working terminal and a
finished agent tab shows the working arc on its row; it shows ✓ once nothing runs.

## Chore: the chat box's feel (2026-11-21)

Your reference (two screenshots of another app's box; "the idea, the looks, the FEELS"). Measured relative to the page
in both: the outer box ~2.5 % above it with a ~7 % hairline, the inner box ~5.5 %, its edge lit from above (~14 % at
the top fading to ~8 %), a soft shadow under it, a white send button. Applied to both boxes (agent and shell), every
theme: `composer_outer` 2.5 %, `composer_inner` 5.5 %, `composer_edge` 7 % (dark); new `composer_glow_top` /
`composer_glow_bottom` (a 1 pt gradient rim behind the inner box); a drop shadow (black 35 % dark, 10 % light, y 4,
blur 12); send ready = the ink's circle with the page's arrow (white on dark, dark on light). Light themes keep their
fills; the light still comes from above (a white top, a darker bottom). Test `the_box_steps_up_softly_and_is_lit_from_
above`; send's contrast in the tab's test. Your bubble and queued messages follow (they use the box's colours).
On-screen: does it feel like the reference?

Then (2026-11-22): "the chat's inner and outer gap/padding is too small add more": the gap between the boxes 5 pt
(`OUTER_INSET`, was 3; the send circle's inset follows so the curves stay concentric). Your second reference: ready,
send is the accent's circle with its arrow (the white one in the first was the empty box's voice button); empty, a
quiet control.

Then (2026-11-22, your two reports): the shell tab's box kept a 3 pt gap on top — its inner box hung from `CTX_ROW`
(28), now `TAB_RISE + OUTER_INSET`, so the top gap is the sides'; the agent tab's lit edge stopped at the curve round
send — now stroked along the inner box's outline there too (`notch_rim`: down the right edge, round the joint, along
the cut, round the other joint, into the bottom edge).

Then (2026-11-22, "can we add more radius on here so it looks good?"): the inner box's two corners where it meets the
send cut (`JOINT`, was 8) now as large as the room before the cut (17), so each runs straight into the circle's curve.

Then (2026-11-22, "make it like in orca show popover when hover to list and popover closed when move out", as a
chore): the popover is a short list — a row per agent (logo, name, the tightest limit's reset, a small bar and its
percent, a chevron); hovering a row shows that agent's limits beside the list; it opens 150 ms after the pointer rests
on the strip (a press opens it at once) and closes 250 ms after the pointer has left the strip and the popover; Esc
and a press outside close it too. Scene `status-bar-usage` hovers: open, a row's detail, away closes, Esc.

Then (2026-11-22, "why it show everything including the top skills etc? i dont need that"): Claude's "Top skills /
subagents / plugins / MCP servers" lists after its limits are left out of the usage rows (popover and box panel).

Then (2026-11-22, "when hover the model that has effort, just directly show it on the side"): a model row's hover opens
its effort list beside it (a model without efforts closes it). Scene `agent-box-effort-list` hovers.

## Chore: the model menu redesigned (2026-11-22, built) — the lead designer's spec

Your words: "about the popup model, make the popup UI great, its too boring". Spec ("quiet glass, one lit row"):
- **Surface** in `menu_frame` (so mode, folder, attach menus share it): outer div `linear_gradient(180°, glow_top @0,
  glow_bottom @0.6)`, p 1, r 13; inner `composer_inner`, r 12, p 4. Shadows on the outer: contact (0,1) blur 2 black
  .30 dark / .06 Latte; ambient (0,12) blur 32 .40 / .12. Width 260; list max 360.
- **Header** "Model" (h 26, px 10, META `text_dim`) with the current model on the right in `text` ("Haiku 5.5 · High").
- **Rows** h 32, r 8, pl 10 / pr 6, gap 6; label split at its first space: family in `text`, version in `text_dim`;
  hover `composer_pill_hover`; **current** row: fill `menu_current` (accent over `composer_inner` 12 % dark / 8 %
  Latte, lowered until `text`/`text_dim` ≥ 4.5), a 2×14 accent bar at left 3, ✓ in `accent`; its effort a pill (h 18,
  px 7, r 9, `composer_pill_hover`, META `text_dim`) before the ✓; › 12 `text_dim` on hover / current / open row.
- **Default · recommended**: "recommended" a hairline tag (h 18, px 7, r 9, 1 px `composer_edge`, no fill); the line
  under it replaced by a 4 px gap.
- **Grouping** (≥ 7 models and some older): pure `group_models(labels) -> (latest, older)` — latest = each family's
  first in catalog order; "Latest" heading (h 22, META dim); "Older  N ⌄" row (h 28) expands in place
  (`BoxMenu::Model { older_open }`, open when the current model is older; reset on close). Else flat.
- **Effort panel**: same surface, width 140, heading "Effort", the model-row anatomy, current as above.
- **Motion**: open 120 ms ease-out (fade + 4 px rise); effort panel 90 ms fade; close and hover instant.
- **Contrast**: `composer_inner`, `composer_pill_hover`, `menu_current` among `text_surfaces`; accent ≥ MARK_MIN on
  `menu_current`; tests over every theme.
- **Risks**: the rim may vanish on Latte (then ambient .16, no dark border); "Latest" relies on newest-first catalog
  order per family (agy's labels: flat if odd); a long label must ellipsize before the effort pill and ✓.

Then (2026-11-22): the agent box's `❯` is a sparkle (fuxx's own filled icon, `assets/icons/fuxx/sparkle.svg`; the icon
set's outline rule kept for Lucide's); and "resize the font and icon to match what we have done on git tab … same as
activity, also git bottom bar": the right panel's shared sizes are the Git tab's (rows 24, names and text 12, small
11, icons 12; were 32 / 13.5–14 / 12 / 16), and the Git bottom bar's text and icons 12 with 28 pt buttons (were
12.5 / 16 / 15 / 34).

## Chores and fixes (2026-11-22, late)

- **Send notch**: "make it so that its fits perfectly" — the inner box's joints at the cut 6 (were 17: their rounding
  squares painted over the box's text), 8 round the circle (was 6). Your reference since (a wide, smooth wave round
  the button) is the next pass.
- **Shell box**: a prompt that fills its row to the last column (powerlevel10k in a long folder) left the cursor at
  the margin with the wrap pending; zsh types on the next row. The typing start is now that row's column 0 (it
  showed an empty `❯` row with the right prompt under it). `blocks.rs`; test `a_prompt_filling_its_row_starts_the_typing_on_the_next`.
- **Git bar**: "icon button shouldn't be bordered and gap should be smaller": its buttons are the header's icon
  buttons (26 pt, 15 pt icons, no edge, a fill on hover), 2 apart; the line under the branch 11.
- **Usage popover**: it closed under a resting pointer (each panel blocked the mouse, which hid it from the box's
  hover; now the box blocks it). No tooltip on the strip. One usage panel: the agent box's and the status bar's
  detail share `usage_view::agent_rows`, `limit_items`, `limits_panel`.
- **Tabs**: a tab's mark and × share one slot (the × takes the mark's place on hover); inactive tabs quieter
  (`tab_idle` .012, hover .035).
- **Status bar**: it follows a focused agent tab (its status, mark, folder and branch); it kept the last terminal
  ("Ready ~").
- **Model menu**, the lead designer's second pass ("one list, one light"): 240 wide, 26 pt rows at 13 / 11.5, a ✓
  column (`menu_check`), no current fill or bar, one fill (hover, or the open row), dividers (`menu_rule`), "Older
  models" closed each time, the current model always shown (`model_groups`); its efforts hang beside the hovered row
  (first level at the row's top, a 4 pt bridge, a 150 ms wait before swapping to another row, a scroll closes them;
  the level a pick would use ticked quieter on another model).

- **Box controls**: "make the background color of these pills and buttons disappear only show background when hover":
  `+`, the model pill, the usage button and the mode pill have no fill at rest; the hover fill stays.
- **Send notch, the wave** (your reference, 2026-11-22: "pay more detail attention to the rounded part"): the cut is
  one shape in the outer colour painted under the box's text (`notch`), with its lit edge along it; the joints 14 (as
  round as the reference), 8 round the circle, the outer corner concentric. The old square-and-patches is gone.
- **Box controls, clean** ("make it clean!"): with no fills the invisible room set the gaps — `+` now sits under the
  ✦, and 12 between what shows (pills 6 each side, no gap between controls).
- **Sidebar footer**, the lead designer's spec ("make these mess good"): 48 tall (was 62), its line kept; three 28 pt
  controls with nothing drawn at rest (no edge, no fill; `row_hover` on hover or while open), 10 in from the sides as
  the rows; New hugs "+ New Fuxxpace" (⌘N in its tooltip, no keycap; under a 220 pt sidebar its + alone); the bell
  and gear 4 apart; the dot on the bell's shoulder (3 in), its ring the colour under it; `OPEN_EDGE` 12 so the band's
  button stays over the gear. Not shot yet: hover, narrow and dot scenes.

**Released in v0.12.0** (2026-11-22, run 44941540508, `fuxx-0.12.0.zip`).
- **Tool modal blink** ("when I click to show the modal, it first show blinking first then show the modal"): its open
  animations were keyed with "fresh", so when the 180 ms fade ended a new 1 ms animation started at 0 and drew the
  modal see-through for a frame. Now it moves only while fresh (`modal_motion`), under a stable id, and is drawn
  plain after.

**The modal fix released in v0.12.1** (2026-11-22, run 44965749641, `fuxx-0.12.1.zip`).
