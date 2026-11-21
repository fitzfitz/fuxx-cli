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

## Calls made without asking

1. An agent whose read never worked (most likely not installed) isn't in the strip, rather than showing "–"
   forever (the spec's "–" for "couldn't read, no value").
2. The popover doesn't close on Esc (it never takes the keyboard, so Esc stays the terminal's); a press outside it or on
   the strip closes it.
3. The background read runs whether or not the status bar is shown (cheap; the spec read only while it shows).
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
