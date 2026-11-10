---
tags: [development, milestone, design]
status: built
---

# 65 - Milestone 11n - Performance (design)

**Status:: 🟨 Built, reviewed, merged and pushed 2026-11-10 (1289 tests; the full ui-shots set passes, 147 shots). Every target met (numbers in "M11n as built"). Released in v0.5.0 (2026-11-10). Your on-screen check is pending (11 items, below). Every design detail is AI's call (the program is autonomous). Ends with the one combined release v0.5.0 (M11i–M11n).**  ·  **Phase 4: an M11 extension.**  ·
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

**Re-measured (Task 0, 2026-11-09 evening, `scripts/bench/`, this branch at `41db015` = master `f661d5e` code, release,
arm64, seeded "welcomed" `sessions.json`, every `cpu_*` run frontmost 15/15 or 168/168):**

| Metric | fuxx (`f661d5e`) | Ghostty 1.4 |
|---|---|---|
| Idle CPU, glow on (`cpu_base`, median of 3) | **7.32 %** (7.32, 7.46, 6.83) | **0.33 %** (3 x 0.33) |
| Idle CPU, glow off (`cpu_noglow`, 2 runs) | **0.73 %** (0.73, 0.73) | n/a |
| Idle CPU after the breath's rest (`cpu_rest`, 305 s in, 1 run) | **6.99 %** | n/a |
| `cat` 100 MB (median of 3) | **0.969 s** (0.969-1.002) | **0.331 s** (0.322-0.428) |
| 200k coloured lines (median of 3) | **0.227 s** (0.215-0.228) | **0.154 s** (0.145-0.156) |
| Launch (median of 5) | 167 ms | 136 ms |
| Memory 10 s / 60 s after ready (RSS / footprint) | 119.1 / 54 MB; 118.9 / 51 MB; 10 tabs 124.4 / 59 MB | 112.7 / 98 MB; 113.1 / 54 MB |

Against the first run: glow-on is higher (7.3 vs 6.3-6.7 %; unexplained: the Mac's load differed, and the seeded
Welcome fix was expected to raise it, but nothing here isolates a cause), glow-off 0.73 vs 0.93 %, `cat` 0.969 vs
0.826 s, colours 0.227 vs 0.282 s, launch 167 vs 143 ms: one session's comparisons only. `cpu_rest` (6.99 %) shows
today's glow never rests. The footprint column differs from the first run (fuxx 54 vs 100 MB, and Ghostty's 98 MB at
10 s against 54 MB at 60 s); that is **unexplained**, so memory targets compare RSS.

**Method of the re-measure (`scripts/bench/run.py`):**
- Every `cpu_*` run is tried up to 5 times until the app is frontmost at every 2 s check (15/15; the 335 s `cpu_rest`
  checks all 168); the last try counts and the earlier tries stay in the result's `tries`. The `mem` phase's own CPU
  figure is **not** frontmost-checked, so it is not comparable with `cpu_*`.
- The bench Ghostty is a `ditto` copy with Ghostty's own bundle id, and cfprefsd ignores `CFFIXED_USER_HOME`, so it writes
  the user's real `com.mitchellh.ghostty` prefs. Ghostty's `grid` and `launch` runs happened **before** the backup rule
  existed; that first run rewrote the real plist (only `NSWindowLastPosition` changed; the old value is lost). Since
  then every Ghostty phase runs `defaults export` first, and after every bench Ghostty process has exited it runs
  `defaults import` and verifies the content is equal (`plutil -convert xml1`, `cmp`); the plist's mtime changes, the
  content is equal. A race remains: a prefs edit the user makes in their own Ghostty during a phase would be overwritten
  by the restore.
- Every process the harness starts is killed on any exit path (error, SIGTERM, Ctrl-C) before the restore.

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

## M11n as built (2026-11-10)

### Measured against every target

One session, HEAD `6fbced0`, release builds, the Mac otherwise idle, every `cpu_*` run frontmost 15/15 on its first
try (`cpu_rest` 168/168). "Base" is `f661d5e` (master before M11n), run in the same session. Prefs checks and
leftovers: the user's fuxx prefs were untouched after all 24 phases, every Ghostty prefs restore compared equal, no
bench process was left.

| Target | Result | |
|---|---|---|
| **Idle, glow on (`cpu_base`)** ≤ 0.8 × base | 5 runs each: base median **6.99 %**, M11n median **5.35 %** = **0.765 ×** | PASS |
| **After 5 min (`cpu_rest`)** ≤ base glow-off + 0.3 points | **6.99 → 0.93 %** (base glow-off 0.75 %, so ≤ 1.05) | PASS |
| **Idle, glow off (`cpu_noglow`)** ≤ base + 0.2 | **0.75 → 0.74 %** | PASS |
| **`cat` 100 MiB** ÷ Ghostty ≤ 1.8 | **0.931 → 0.481 s**, Ghostty 0.338 s = **1.42 ×** (was 2.68–2.93 ×) | PASS |
| **200k coloured lines** ÷ Ghostty ≤ 1.4 | **0.219 → 0.156 s**, Ghostty 0.155 s = **1.01 ×** | PASS |
| **Launch** ≤ base + 10 % | **163 → 162 ms** (Ghostty 135) | PASS |
| **RSS** (idle / 10 tabs) ≤ base + 5 MB | **121.1 / 124.6 → 119.2 / 125.0 MB** | PASS |

Read these with their caveats:

- **`cpu_base`: the median passes at 0.765 ×, the mean at 0.851 ×.** The base's five runs were 2.83, 6.99, 6.92, 7.06 and
  7.03 %; the 2.83 % was the session's first run and is **unexplained** (15/15 frontmost, nothing in the results file
  shows a lost key window or a Welcome card). It is **kept**, because nothing showed it was invalid. It drags the base's
  mean to 6.17 % and the mean ratio over 0.8. Without it the ratio is **0.75 ×**. The three-run version of the same
  session read 0.775 × (median 6.92), so this target was firmed up to five runs; the result matches the frame model's
  prediction (about 0.76 ×). The harness stores only each run's aggregate, so a per-sample CPU log is a possible
  follow-up that would settle the 2.83 %.
- **Throughput bias:** when `cat` returns, up to 1 MiB (`INBOX_CAP`) can still be unparsed in the inbox, about 6 % of
  the 200k-colour run's 18 MB, so the true colour ratio may be about **1.07 ×** rather than 1.01 ×. It still passes.
  The `cat` 100 MiB run is not affected in any way that matters (1 MiB of 100).
- **`cpu_rest` was one run per app** (as planned), 305 s in.
- Absolute CPU varies with the Mac's load; only the ratios inside one session mean anything.
- **The final fix wave** (after `6fbced0`: the daemon's queue caps, comments, constants, tests) did not touch the
  local path that was measured, so the numbers were not re-run.

### What the build settled (AI's rulings beyond the plan)

- **`MarkScanner` also cuts at CAN and SUB** (0x18, 0x1A), not only at ESC and BEL. It is equivalent to the old
  byte-by-byte oracle, and needed: removing the cut was caught by the oracle test (mutation 0).
- **One buffer per terminal, not two:** a drained terminal holds at most `IDLE_KEEP` (64 KiB) in total, so ten idle
  tabs do not hold 128 KiB each.
- **The daemon's queues keep their real old bound.** macOS reads were about 1 KiB, so before M11n the subscriber queue
  held about 1 MiB (1024 frames) and a connection's writer queue about 256 KiB (256 frames). With 64 KiB batches the
  frame caps alone would allow 64 MiB and 16 MiB. Both now have a **1 MiB byte cap** next to the frame cap. A frame
  bigger than the cap (a Snapshot can be) waits for an empty queue and goes in alone, so nothing deadlocks and nothing
  is dropped. The protocol and frame shapes are unchanged, and the daemon is compatible both ways.
- **Turning the glow, Animations or Reduce motion back on ends the rest**, so a solid cursor never waits for a keystroke
  after a settings change. **A hidden cursor reappearing, or the style returning to a breathing one, also restarts the
  breath:** 0.6 s solid, then breathing from a peak. Before, it resumed mid-phase.
- **A ui-shots `cursor-flood` scene guards `output_seen`:** a flood with Animations off must show its last line with no
  input, so the view can never stop acknowledging an `Output` (the stale-frame class of bug).
- **The benchmark harness is in `scripts/bench/`** (`run.py`, `make_data.py`, `wininfo.swift`). It backs up, restores
  and verifies the user's Ghostty prefs: the bench copy shares Ghostty's bundle id and cfprefsd ignores
  `CFFIXED_USER_HOME`. The first Ghostty runs (grid, launch) rewrote the user's `NSWindowLastPosition` before that rule
  existed; the old value is lost, and you were told. Every process the harness starts is killed on any exit path.

### Found for later (the chores round; not done)

- **The breath still costs about 5.3 %** (5.35 % against the glow-off 0.74 %), because every breath frame re-renders the
  whole window chrome (sidebar, tab bars, status bar, title area): only the panes are `.cached()`. Caching the chrome
  could save more than repainting only the cursor, and does not have that idea's layering problems (see "Not in M11n").
  **→ M11p (added to the plan 2026-11-10 by you, after M11o; see [[40 - Roadmap]]).**
- A per-sample CPU log in the harness (above).

### Limits

- **Nothing has been verified on the real screen yet;** that is your on-screen check below. The tests and the benchmark
  show the numbers and the structure, not how the glow looks.
- `cpu_rest` has one run per app; the `cpu_base` base set has one unexplained low run (above).
- Still not verified on 60 Hz displays or on Intel Macs.

## On-screen checklist (pending; v0.5.0 is released)

Nothing here has been seen on a real screen yet; the benchmark and the tests cannot show how the glow looks. A copy of this list, as plain steps, is in your test list.

1. **Idle feel:** fuxx in front at an idle prompt, nothing moving: Activity Monitor's % CPU is well under v0.4.1's (about 6 % before; the benchmark read about 5.4 %). Note the number. The window feels as quick as before.
2. **The glow looks the same:** a new terminal's cursor breathes softly, 100 % to dim and back about every 1.6 s, with its glow, like v0.4.1's. Watch a few breaths; if you see stepping near the brightest or dimmest moment, say so. While you type the cursor holds solid and breathes again about 0.6 s after you stop; click another app and back: solid, then breathing.
3. **The rest:** leave a prompt untouched for over 5 minutes: the cursor is solid and glowing, not breathing. Press a key, print some output, or click away and back: the cursor stays solid for 0.6 s and then breathes again.
4. **Big output:** `cat` a big file (`seq 1 5000000 > /tmp/s; cat /tmp/s`): it finishes noticeably faster than in v0.4.1, the last line is `5000000`, and you can scroll back through the output. `seq 1 1000000` ends on `1000000`; Ctrl-C during a long `yes` stops it at once.
5. **Agents:** run `claude` (or another agent): Working, Done and Waiting still show on the tab, the sidebar and the cursor colour.
6. **Notification rings:** a finished agent in another tab still rings and sends its notification.
7. **Command blocks:** run a few commands, including one that prints a lot (the `cat` above): every command still has its bar (red when it failed), also those that ran during the flood, and ⇧⌘↑ / ⇧⌘↓ still jump between them.
8. **Full-screen programs:** vim, htop and less draw and redraw correctly and respond to keys; quit and the prompt is back.
9. **Splits:** split a pane (⌘D), run `cat` of the big file in one half while typing in the other: typing stays responsive and both halves are right afterwards.
10. **Persistent sessions survive the update:** with persistent sessions on in 0.4.x, leave a session running, update to 0.5.0 and reopen: the session is still there and live.
11. **Update:** fuxx 0.4.x shows **Update Available** for 0.5.0; Update and Restart brings 0.5.0.

## Links

[[40 - Roadmap]] · [[12 - Scope]] · [[M11n Plan - Performance]] · [[58 - Milestone 11f - Cursor Style]] (the breath)
· [[63 - Milestone 11l - Command Blocks]] (the marks and the tagger) · [[42 - Milestone 2 - Agent State Detection]]
· [[57 - Milestone 11e - Update Available]] (the release pipeline) · [[64 - Milestone 11m - Title Bar]] ·
[[60 - Progress Log]]
