---
tags: [development, milestone, design]
status: planned
---

# 65 - Milestone 11n - Performance (design)

**Status:: 🟨 Designed and planned 2026-11-09, not built. Every design detail is AI's call (the program is
autonomous). Ends with the one combined release v0.5.0 (M11i–M11n).**  ·  **Phase 4: an M11 extension.**  ·
**Plan:** [[M11n Plan - Performance]]
**Added to the plan 2026-11-09 by you** (see [[12 - Scope]], "Performance"), after M11l; renumbered M11n when the
title bar took M11m.

## Goal

fuxx left idle in front costs much less CPU with the breathing glow on, and drains a flood of output faster, with
nothing you'd notice changed: the same breath, every byte on screen, the same agent states, notifications and
command blocks.

## Why (your words, 2026-11-09)

- You asked how fuxx compares with Ghostty, cmux and Warp. The local benchmark (2026-11-09) found fuxx idling at
  about **6 % CPU** against Ghostty's **0.2 %** (about **0.9 %** with the glow off), and draining plain output about
  **2.7× slower** than Ghostty.
- "alright for performance milestone, we can put it after M11l". It is M11n, after the M11m title bar, and "push the
  release after performance milestone": the one combined release v0.5.0 is this milestone's last step.
- Scope (the brief): the idle cost and the output throughput, plus anything a profile shows to be a clear, cheap win.
  No visual or behaviour change you'd notice, except where the cursor glow must change to be cheap, and then it must
  look the same or nearly the same: it is your chosen look from M11f, so §4 says exactly what changes.

## Baseline (2026-11-09 benchmark)

Same Mac, release builds, each app in its own scratch `HOME` (a short symlink `/private/tmp/claude-502/fxb-<app>`,
because of fuxx's socket path limit), launched by the harness (`run.py`) as a separate process that it alone signals
and quits. The shell is zsh with a scratch `ZDOTDIR` whose rc writes `$EPOCHREALTIME` to a marker file (the
"ready" moment) and runs `$BENCH_CMD`. fuxx's window opened at 1280×800 (a 127×41 grid), Ghostty's at 1044×784
(130×44).

| Metric | How | fuxx (master, 0.4.1+) | fuxx 0.4.1 | Ghostty 1.4 |
|---|---|---|---|---|
| **Idle CPU, glow on** (`cpu_base`) | `ps` CPU time over 30 s, from 10 s after ready, window in front | **6.3–6.7 %** | 6.1–6.2 % | **0.23 %** |
| **Idle CPU, glow off** (`cpu_noglow`) | the same with `cursor-glow = off` | **0.93 %** | — | — |
| **`cat` 100 MB** (`tput`) | `cat` of 100 MB of plain text timed inside the shell with `$EPOCHREALTIME`; median of 3 | **0.826 s** | 0.843 s | **0.308 s** (fuxx 2.68×) |
| **200k coloured lines** (`tput`) | the same, 18 MB of SGR-coloured lines | **0.282 s** | 0.284 s | **0.178 s** (fuxx 1.58×) |
| Launch (`launch`) | process start → shell ready; median of 5 after a warm-up | 143 ms | 140 ms | 108 ms |
| Memory (`mem`) | RSS / footprint 10 s and 60 s after ready; 10 tabs (`fuxx new --tab` ×9) | 117.5 / 100 MB; 10 tabs 123 MB | 112.5 / 95 MB; 119 MB | 112 / 91 MB |

The harness's apps, homes and data were deleted after that run (only `run.py`, the results and the logs remain), so
the plan brings the harness into the repo and re-measures the baseline first (Task 0). Two harness faults found
while profiling, fixed there: since M11k a deleted `sessions.json` shows the **Welcome card**, which takes the
keyboard from the terminal, so the glow never runs and `cpu_base` reads ~1 %; and `$EPOCHREALTIME` needs
`zmodload zsh/datetime`. Absolute CPU varies with the Mac's load (the same build read 7.3–9.3 % in the evening
re-runs), so every target compares runs from one session.

## What the profile shows (2026-11-09, this branch at `f661d5e`, release build)

Measured with `sample` (10 s idle, 2 s during `cat`), `ps` and test variants built from scratch copies of the
source (never this worktree): an isolated `HOME` seeded "welcomed", the frontmost app checked every 2 s, every
instance quit after.

### Idle

| Variant (one window, one terminal, in front 10/10) | Idle CPU |
|---|---|
| Glow off | 1.0–1.5 % |
| Glow on, today (a redraw every 50 ms) | 7.3–9.3 % |
| Glow on, a redraw every 100 ms | 4.8–5.2 % |
| Glow on, a redraw every 200 ms | 3.1 % |
| Glow on, a redraw only when the opacity moved ≥ 0.064 since the last (a rough prototype, ~9 frames/s, steps up to 0.13) | 3.5 % |

- **Root cause: every breath tick redraws the whole window.** The breath timer calls `cx.notify()` on the focused
  `TerminalView` 20 times a second. GPUI marks that view **and every ancestor** dirty (`mark_view_dirty` walks to the
  root, and the root view always re-renders), so each tick is a full frame: `Workspace::render` of the sidebar, tab
  bars, status bar and title area, Taffy layout of the whole tree, the paint (the focused grid re-snapshotted under
  the `Term` lock and re-painted), and a Metal present. The main thread spent 363 of 7,868 samples in
  `Window::draw` over 10 s: layout 130 (36 %), paint 115 (32 %), render 98 (27 %); with the glow off, ~0. Other
  panes are already cached views and are not repainted.
- **CPU follows the number of breath frames:** about **0.3 % per frame per second** on this Mac (5 → 10 → 20 frames
  a second read 3.1 → ~5.0 → 7.3–8.0 %), on top of the ~1 % floor and a fixed ~0.8 % whenever any breath runs
  (GPUI presenting regularly; not ours to change).
- **A cheap win in every frame:** `Theme::ui()` recomputes the chrome colours, with WCAG contrast (`powf`), on every
  render and every call: 30 of the 363 draw samples (8 %), including the tab bar's colours.
- **Already right (M11f, checked in the code):** the glow runs only for the focused terminal in the key window, with
  Animations on and Reduce motion off; it holds without redraws for 0.6 s after any change.
- **With the glow off, ~1 % remains:** GPUI's display link wakes the main thread at every display refresh while the
  window is visible (GPUI's design, kept). Each terminal's 50 ms housekeeping ticker also wakes 20 times a second,
  but ten idle tabs measured no clear extra cost (1.1 % against 1.8 % for one, both runs with the window covered), so
  it is left alone.
- **GPUI cannot repaint a region.** Any change redraws the window; the only levers are fewer frames and cheaper
  frames.

### Throughput

- **macOS hands a PTY reader at most ~1 KiB per `read`,** whatever the buffer: 100 MB of `cat` took 105,515 reads
  averaging 1,010 bytes with 8 KiB, 64 KiB and 1 MiB buffers alike (a Python `pty` probe). fuxx does everything once
  per read.
- **The single reader thread is the bottleneck** (2 s sample during `cat`): parsing (`vte` + `Term`) **48 %**; four
  byte-by-byte scanners **29 %** (OSC 9/99/777 detector 13 %, OSC 133 mark scanner 6.6 %, OSC 7 reader 6 %, clear
  counter ~4 %); the `read` call itself **19 %**; sending `Output` and waking the main run loop **3 %**; waiting on the
  `Term` lock **0.4 %**. The main thread draws at the display rate and is not the limit.
- **Skipping the scanners on ESC-free reads** (a measurement-only variant, valid only for plain text) drained 4 ×
  100 MB in **2.92–3.12 s against 3.86–4.20 s (−25 %)**, with 28 % less app CPU.
- Ghostty and Alacritty read until the kernel has nothing more, parse under one lock and redraw at most once per
  display frame. fuxx already redraws at most once per frame (GPUI coalesces `notify`); what it lacks is batching and
  overlap of the kernel's 1 KiB reads with parsing.

## The changes (each an AI's call)

### 1. The byte scanners jump over plain text **(AI's call)**

While a scanner is at rest (no sequence half-matched), it jumps straight to the next ESC with `memchr` (SIMD);
inside an open OSC body it copies up to the next BEL or ESC in one go. Plain text, almost all of a flood, costs the
scanners next to nothing. **Why:** 29 % of the reader's time, for bytes that can't start a sequence. Byte-for-byte
the same results: a test feeds random streams one byte at a time to today's code and in random chunks to the new.
`memchr` is already compiled into fuxx (via GPUI's tree); this makes it a direct dependency.

### 2. A parser thread, 64 KiB batches, one `Output` per batch **(AI's call)**

The reader thread only reads and appends to an inbox; a new parser thread takes everything queued (up to 64 KiB),
runs the scanners and parses it under **one** lock, then tells the UI once. The UI gets a new `Output` only after it
has handled the previous one (a notification-carrying batch always gets its own). **Why:** the kernel's 1 KiB reads
(19 %) now overlap with parsing on another core; the lock, the scanner calls and the main-thread wake happen per
batch instead of per KiB; and the UI stops being woken ~100,000 times per 100 MB. **Kept:** every byte in order;
`Term` events before their batch's `Output`; `Exited` after the last; backpressure (the reader waits at 1 MiB
queued, so a flooding program blocks as it does today); a batch never holds the lock long (64 KiB ≈ 0.5 ms), so
frames stay smooth; a drained terminal keeps at most 64 KiB of buffer. The background service uses the same reader
and gets the same gain.

### 3. Cheaper frames: `Theme::ui()` once per theme **(AI's call)**

The chrome colours are computed when the theme changes, not on every render. **Why:** 8 % of every frame, for a
result that only changes with the theme. Benefits every redraw (breath, typing, floods, resizes).

### 4. The breath: the same look, fewer frames **(AI's call; the only visible changes, both small)**

- **Drawn at level crossings.** Today a redraw every 50 ms, whatever the opacity did. Now a frame is drawn exactly
  when the opacity reaches the next step of **0.064** (the largest step it takes today, at the steepest point of the
  1.6 s cosine) and at each **peak (100 %) and trough (35 %)**. Same curve, same period, same 100 %/35 % extremes,
  same 6 px glow at 30 %, same colours. **What changes:** near the brightest and dimmest moments, where today's
  frames change the opacity by tiny amounts, the cursor now changes in the same 0.064 steps it already takes
  mid-fade; it never takes a bigger step than today. 22 frames per breath instead of 32 (≤ 13.75 a second).
- **Asleep through the hold.** The 0.6 s hold after typing or output no longer wakes the timer every 50 ms (it drew
  nothing anyway); it sleeps to the end of the hold.
- **At rest after five minutes untouched.** After **188 breaths (300.8 s)** with no output, keystroke, cursor move or
  focus change, the breath stops **at a peak**: the cursor stays solid at full strength with its full glow, exactly
  its look at the top of a breath, and costs nothing. The next output, keystroke or refocus brings the breath back
  0.6 s later, as after any change today. **What changes:** a prompt you haven't touched for five minutes stops
  breathing until you touch it. **Why:** fuxx left in front with nobody at it (agents in other tabs, you away) is the
  idle that runs for hours; five minutes is far beyond any glance (GTK stops its cursor blink after 10 s; fuxx waits
  30 times longer).
- **Why not more:** a lower fixed rate (10 a second) halves the frames but doubles the largest step, which could show
  as stepping; repainting only the cursor needs a second window root above the workspace (GPUI can't repaint a region),
  which would draw over popovers and the sliding sidebar and change the glow's layering under text: too much risk for
  the gain (see "Not in M11n").

## Targets (proved in Task 5 by the same harness, all runs from one session)

- **Idle, glow on, breathing (`cpu_base`):** ≤ **0.8 ×** the baseline. 22 frames per breath instead of 32
  (13.75 a second instead of 20) predicts ~0.76 × from the frames model above, and `Theme::ui()` takes ~8 % off each
  frame's main-thread work; the fixed part stays. On the 2026-11-09 morning numbers: about 6.4 % → under 5.2 %.
- **Idle, glow on, after five minutes (`cpu_rest`, a new harness phase measuring 305–335 s after ready):** ≤ the
  baseline's glow-off figure + 0.3 points (about 1.2 %): at rest nothing redraws.
- **Idle, glow off:** no worse than the baseline + 0.2 points.
- **`cat` 100 MB:** fuxx ÷ Ghostty ≤ **1.8** (baseline 2.68). The scanner change alone measured −25 % (→ ~2.0);
  the parser thread hides the 19 % spent in `read` and the per-read overhead, which predicts ~1.5–1.6; 1.8 leaves room
  for the hand-off's own cost.
- **200k coloured lines:** fuxx ÷ Ghostty ≤ **1.4** (baseline 1.58). Colour-heavy output is parse-bound
  (SGR), so less of it is scanner and read time.
- **No regression:** launch ≤ baseline + 10 %; idle and 10-tab RSS ≤ baseline + 5 MB.

## Not in M11n

- A quieter housekeeping ticker (each terminal's 50 ms tick at an idle prompt): ten idle tabs showed no clear cost,
  and it settles the agent states, so it isn't worth the risk now.
- Repainting only the cursor (a second window root over a cached workspace, or a Core Animation layer): the glow
  would draw above popovers, menus and the sliding sidebar, and over the letters beside it instead of under them.
  Maybe 1–1.5 points more idle saving; a later milestone if you want it.
- Forking GPUI (its display link, its full-window redraw) or `alacritty_terminal` (its parser).
- A setting for the breath's rate or the rest; turning the glow off by default (it is your look; `cursor-glow = off`
  already exists).
- Capping redraws during a flood below the display rate; faster launch or less memory (they were close to Ghostty and
  get a no-regression check only).
- The background service's own read loop beyond the shared reader (it gets Tasks 1–2 as they are).

## Limits (known, accepted)

- **Ghostty-level idle (0.2 %) is out of reach with the glow on:** each breath frame is a full GPUI frame (~3–4 ms of
  CPU), and GPUI's display link costs ~1 % whenever the window is visible.
- **Throughput stays parse-bound:** `alacritty_terminal`'s parser and grid are ~half the reader's time and are not
  touched; Ghostty's SIMD parser stays ahead.
- **Benchmarks need the Mac quiet and the window in front:** the harness drops `cpu` runs that weren't frontmost the
  whole time; your own use of the Mac during a run makes them noisy.
- **Not verified on 60 Hz displays or on Intel Macs.**

## On-screen checklist (pending, after the v0.5.0 release)

1. **The breath looks the same:** a new terminal's cursor breathes softly, 100 % to dim and back about every 1.6 s,
   with its glow. Watch a few breaths: it should look like v0.4.1's. If you see stepping near the brightest or
   dimmest moment, say so.
2. Type: the cursor holds solid while you type and starts breathing again about 0.6 s after you stop.
3. Click another app, then back: the cursor is solid, then breathes again.
4. **Rest:** leave a prompt untouched for over 5 minutes, then look: the cursor is solid and glowing, not breathing.
   Press a key: it breathes again shortly after.
5. **Idle feel:** Activity Monitor, fuxx in front at an idle prompt for a minute: % CPU well under v0.4.1's (about 6 %
   before). Note the number.
6. `cat` a big file (`seq 1 5000000 > /tmp/s; cat /tmp/s`): it finishes noticeably faster than in v0.4.1, the last line
   is `5000000`, and you can scroll back through the output.
7. `seq 1 1000000` ends on `1000000`; Ctrl-C during a long `yes` stops it at once.
8. **Agents:** run `claude` (or another agent): working, waiting and done still show on the tab, the sidebar and the
   cursor colour; a finished agent in another tab still rings and sends its notification.
9. **Command blocks:** run a few commands, including one that prints a lot: each still has its bar (red when it
   failed), and ⇧⌘↑ / ⇧⌘↓ still jump between them.
10. **Splits:** split a pane (⌘D), run `cat` of the big file in one half while typing in the other: typing stays
    responsive and both halves are right afterwards.
11. vim, htop or less: full-screen programs draw and redraw correctly; quit and the prompt is back.
12. **Update:** fuxx 0.4.1 shows **Update Available** for 0.5.0; Update and Restart brings 0.5.0.

## Links

[[40 - Roadmap]] · [[12 - Scope]] · [[M11n Plan - Performance]] · [[58 - Milestone 11f - Cursor Style]] (the breath)
· [[63 - Milestone 11l - Command Blocks]] (the marks and the tagger) · [[42 - Milestone 2 - Agent State Detection]]
· [[57 - Milestone 11e - Update Available]] (the release pipeline) · [[64 - Milestone 11m - Title Bar]] ·
[[60 - Progress Log]]
