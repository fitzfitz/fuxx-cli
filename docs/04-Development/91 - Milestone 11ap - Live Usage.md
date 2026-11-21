---
tags: [milestone, m11ap]
---

# 91 - Milestone 11ap - Live Usage

**Status:** 🟨 building (2026-11-21). Added on your words: "is it possible to get claude and antigravity usages
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
2. **One look for limits:** `src/usage_view.rs`, "% used" with band colours, `UsageInk`, agy's names; the box
   panel uses it and `live_usage`.
3. **The strip:** pure summary (tightest limit, short reset, states, tooltip) and its place in `fit_status_bar`;
   drawn in the status bar.
4. **The popover:** header, refresh with its spinner, per-agent sections; close rules.
5. Scenes, the full ui-shots run, one review, docs.
