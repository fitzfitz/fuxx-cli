# Milestone 11aj — Faster UI Shots

**Status:** 📋 design approved 2026-11-16 ("approved"); plan [[M11aj Plan - Faster UI Shots]]; built after M11ae merges.

**Asked for:** 2026-11-16, by you: "let's talk about ui_shots. right now, the file is very bloated and it takes like
almost hours to run. GIve me your best suggestions as a senior fullstack developer who crave for best approach on
architecturing things." Choice: **"all three, your order is fine"** — A (quick wins), B (restructure), C (fewer
screenshot tests), as one milestone **right after M11ae**, before M11aa's remaining UI tasks.

## Why

The off-screen screenshot harness (`cargo run --features ui-shots -- __ui-shots`) is the only way fuxx checks real
drawing, so every UI task runs it — and it has become the slowest, most-edited thing in the repo:

- **One file:** `src/ui_shots.rs` is 22,203 lines (224 functions, 105 scene groups in one `SCENES` list), plus
  `src/ui_shots/chrome_sources.rs` (1,897). Every UI task edits it; reviews and merges collide in it.
- **Every shot starts from scratch:** each `render`/`shoot` builds a new `VisualTestAppContext` on a new Mac platform
  and registers the bundled fonts again (`ui_shots.rs:86-89`) — about 450 times a run.
- **~770 MB of PNGs every run:** 449 uncompressed PNGs at about 1.8 MB each, though the checks are pixel/state probes in
  code and the pictures are only for human eyes.
- **Strictly one after another:** AppKit allows windows only on the main thread, so one process runs every scene in turn
  while the Mac's other cores sit idle.
- **Every scene twice:** once per theme (`SHOT_THEMES`), though most checks don't depend on the theme.
- **The workflow multiplies it:** a separate `--features ui-shots` build, one full run per task, another per fix
  round. A full run measured **8 min 08 s** (M11ae Task 2); across a milestone that adds up to hours.

## Goals

1. **Fast feedback:** a task's own scenes run in **≤ 30 s** after the build; the full suite in **≤ 90 s** on this Mac
   (after the build). Both measured and recorded.
2. **Readable and mergeable:** no harness or scene file over about **3,000 lines**; each area of the app has its own
   file; adding a scene touches one area file.
3. **Nothing lost:** every check that exists today still runs — in a scene, or moved to `cargo test` — and every scene
   keeps its name (filters, docs and notes that name scenes stay valid).
4. **Fewer screenshot tests over time:** checks that don't need a real window live in `cargo test`.

## Non-goals and anti-goals

- **Not** a new test framework or new crates.
- **Anti-goal: flaky.** Parallel runs must not make timing-sensitive scenes fail at random; a scene that fails only under
  load is a bug in this milestone.
- **Anti-goal: hidden failures.** A failed scene must still say which check failed and leave its picture for you to
  look at.
- **Anti-goal: lost coverage.** Faster by skipping checks doesn't count.

## The approaches (you chose all three)

- **A · Quick wins, same layout** — measure, then cut per-shot setup, PNG writes, the second theme, and run in parallel
  processes. Fastest gain; leaves the 22k-line file.
- **B · Restructure** — split by area, scenes declared with tags/themes/sources, run only the scenes a change affects.
  Keeps it fast and readable as scenes grow.
- **C · Fewer screenshot tests** — move checks that only read view state into `cargo test`; keep screenshots for real
  visual checks; try compact reference images. The biggest long-term win, done partly now and continued by later
  milestones.

## The design

### Part 0 — Measure first

- Each scene prints its time (`ui-shots: file-tab ok (12.4 s)`), and the run ends with the 15 slowest and the total.
- One run with a per-shot breakdown (app + platform creation, font registration, settling, capture, PNG write) decides
  what A1 does; the numbers go in this note.
- Timings are kept in `target/ui-shots/timings.tsv` (scene, seconds) for sharding (A4).

### Part 0 — measured (M11aj Task 1, 2026-11-16)

Debug build, this Mac, nothing else heavy running (load average about 2, no process above 1 % CPU). 112 scenes
(105 listed in the design plus M11ae's), all passing.

| Run | Total |
| --- | --- |
| First (cold PNG folder) | 493.2 s |
| Second (warm) | 507.4 s |
| `UI_SHOTS_PROFILE=1` (two runs) | 507.1 s, 512.9 s |

Run-to-run spread is about 3 %. **The suite is about 8.4 minutes.**

**Slowest 15** (profiled run): preferences 40.5, file-tab 38.2, title-bar 24.6, sidebar 21.7, file-tab-md-mermaid 21.3,
palette-files 18.1, help 16.7, tabs 15.7, file-tab-md 15.2, title-area 15.1, tab-bars 14.8, tab-drag 11.2, file-diff
10.4, right-sidebar 9.2, branch-switcher-real 9.0 s. The 15 are 282 s, 55 % of the run; the longest is 40 s.

**Where the 513 s goes** (summed over about 450 shots; `UI_SHOTS_PROFILE=1`):

| Phase | Sum | Share |
| --- | --- | --- |
| `teardown` (dropping the app and window at the end of `render_with`) | **158.0 s** | 31 % |
| `open` (`open_offscreen_window`, includes the scene's own `build` closure) | **92.2 s** | 18 % |
| `act` (the act closure and second park) | **68.1 s** | 13 % |
| `settle` (first `run_until_parked`) | 42.5 s | 8 % |
| `png` (stored-deflate PNG and write) | 30.9 s | 6 % |
| `fonts` (`register_bundled_fonts`) | 16.5 s | 3 % |
| `capture` (`capture_screenshot`) | 16.4 s | 3 % |
| `new_app` (`VisualTestAppContext::new`) | 0.2 s | 0 % |
| Not in `render_with`: scenes that build their own app, scene bodies, waits, child processes | about 88 s | 17 % |

`teardown` was not in the design's list of phases; it was found because the phases above summed to only 267 s of 507 s.
It is the single largest cost. Creating the app is free (0.2 s); *dropping* it is not (about 0.35 s a shot).

**Real-time waits:** 33 `thread::sleep` sites, now counted by `prof::sleep`: **28.4 s** (5.5 %) in total, measured, not
estimated. Biggest: file-tab 5.9 s, command-blocks-copy 3.5, file-tab-scrollbar 3.0, sidebar 2.9,
chrome-places-programs-ports 2.9, tab-drag 2.2.

**Child-process scenes:** 20.5 s — `file-tab-md-mermaid` (WebKit child) 19.1 s of its 21.3 s;
`chrome-kill-switch` (cache-off child) about 1.4 s.

**Scenes with their own app** (not through `render_with`; about 40 scenes with no `render_with` time, about 57 s;
sites: `cached_panes`, `chrome_window_with` (the chrome scenes), the cursor scenes' `spawned`, and three in the Mermaid
scene).

### Expected gain of each Part A item (estimates from the numbers above)

| Item | Expected saving (serial) | Why |
| --- | --- | --- |
| A1 one app per scene, fonts once | **100-170 s** (fonts 16.5 s certain; teardown 158 s if it is per app and not per window; `new_app` nothing) | Part 0 shows creating the app is cheap but tearing it down is the biggest phase. Task 7 must first check whether teardown cost follows the app or the window. |
| A2 PNGs only when wanted | **about 31 s** (and 770 MB less disk) | `png` phase. |
| A3 one theme by default | **60-90 s** | roughly half the shots, if about half the scenes are `Themes::One`; scene-split audit decides. |
| A4 parallel shards | divides what is left by 5-6 (cores minus 2, at most 8) | needs balancing: the longest scene is 40 s, Mermaid (21 s) is serial. |
| A5 fewer real-time waits | **at most about 10 s**; the animation (`SLIDE`) and WebKit waits stay | 28.4 s of sleeps; many are real-time animation margins. |

**Projection:** 513 s minus A1 (about 130), A2 (31), A3 (about 70), A5 (about 10) leaves about 270 s of serial work;
over 6 shards that is about 45-55 s plus a serial tail (Mermaid 21 s, kill switch, the `SLIDE` scenes) of about 30 s:
**roughly 75-90 s.** The 90 s goal looks reachable but tight, and depends on A1's teardown saving. The 30 s per-task goal
looks comfortable: a task's own scenes are one to a few of the scenes above, and the biggest is 40 s serial (about 15 s
after A1-A3).

### A1 — measured (M11aj Task 7, 2026-11-17)

**What the teardown was:** neither the app nor the window — the **terminals**. A `sample` of the `tabs` scene put
1,638 of its 3,057 main-thread samples in `drop(cx)`, and 1,615 of those in `Pty::drop` (`src/pty.rs`) → portable-pty's
`ChildKiller::kill` for `std::process::Child`, which sends the child a SIGHUP, asks at once whether it has exited (it
hasn't yet), then sleeps 50 ms before asking again. Every shot's `cat`s paid that one after another (the `tabs` shots
have several each: 9.8 s of teardown for 14 shots, at 24 % CPU). GPUI's own teardown (the `MacWindow` drop, the
`LeakDetector`, the app's entities) was a few samples.

**What A1 does** (the harness only; the app's `Pty::drop` is unchanged):

- **Terminals hung up together:** before a shot's app drops, `HangUp` sends a SIGHUP to every child started since the
  shot began that leads its own session, and waits up to 500 ms, without reaping, until they have exited; each `Pty`
  then finds its child gone and doesn't sleep. Those children are the terminals' (portable-pty `setsid`s them) and also
  the app's own capped git, `ps` and `lsof` commands and its agent processes, which run in a session of their own too:
  hanging them up is harmless, as they belong to the app being dropped. A child fuxx run and the git a scene runs itself
  share the harness's session and are untouched. In `render_with`, and through `ShotApp` (a fresh app with this guard)
  in the scenes that keep their own app (`chrome_window`, `cached-panes`, the cursor rig, the cursor status block).
  Mermaid's kept contexts and its WebKit child are unchanged.
- **Fonts once per process:** the bundled fonts are registered with CoreText for the whole process (and
  `register_bundled_fonts` already kept its first answer), so only the first app asks CoreText for every installed font
  name (about 40 ms a time); `mono_font` reuses that list too (it was asked again in every shot's `shot_appearance`).
- **Not done: one app per scene.** Creating an app costs 0.2 s in a whole run, and the teardown follows the terminals,
  which a shared app would still drop with each window; so it would gain nothing and risk state leaks.

| Run (this Mac, debug build) | Before | After |
| --- | --- | --- |
| Default (`UI_SHOTS_PROFILE=1`) | 433.9 s | **212.9 s** |
| `--png --all-themes` (`UI_SHOTS_PROFILE=1`) | 511.7 s | **276.3 s** |

Phase sums, default run (s): new_app 0.3 → 0.2, fonts 19.7 → 0.0, open 85.0 → 47.7, settle 39.6 → 38.9, act 57.4 → 54.7,
capture 15.2 → 15.2, **teardown 146.4 → 1.9**; render_with total 217.4 → 156.7; sleep 24.2 / 24.3; child 17.6 / 17.4.
With `--png --all-themes`: fonts 16.9 → 0.0, open 89.3 → 54.1, teardown 159.5 → 2.1, png 42.3 → 37.2, the rest within
5 %. Outside `render_with` and its teardown ("other", now printed apart from teardown per scene): 71 s in the default
run, of which the Mermaid WebKit child is 16 s.

Slowest after A1 (default run): file-tab 33.3, file-tab-md-mermaid 17.3, file-tab-md 13.8, preferences 13.4, file-diff
7.6 s; no other scene over 6.2 s.

**Same pictures:** the `--png --all-themes` run passes the same 207 `ok` lines and writes the same 451 PNGs, 444 of them
byte-identical. The other 7 (`activity`, `activity-waiting` ×2 themes, `branch-switcher-open-mocha`,
`welcome-checking` ×2) differ between two runs of the unchanged build too (they show ages and a real scratch repo); that
was the list at Task 7, and Task 8 makes it 11 (see "A4, A5 — measured": 4 pictures that show the Files fixture's age). The
leak check (in-memory shots before and after, `diff_count == 0`): `tabs`, `title-bar`, `changes-bar`, `rename-box`,
`command-blocks-rerun` (`Both`) and `help`, `update-button`, `command-blocks-rprompt`, `file-tab-scrollbar`, `panels`
(with `right-panels`) (several shots): 91 shots, all 0. One repeat of `title-bar` once drew `title-bar-sliding-mocha`
2,996 bytes apart (a hide slide set 60 ms back in real time, so it moves with how long build to frame takes; the scene
still passed); five more runs of it matched, as did four of the unchanged build: a real-time shot for A4's `serial` list.

### A4, A5 — measured (M11aj Task 8, 2026-11-17)

**What A4 does:** `__ui-shots --shard I/N` runs the `I`th of `N` shares of the scenes that aren't `serial`
(`shard_of`: longest first onto the least-loaded shard, by `target/ui-shots/timings.tsv`; a scene with no time counts
the median; each shard in `ORDER`); `--serial-only` runs only the `serial` scenes, in `ORDER`. Each writes its own
`timings.<I>.tsv` / `timings.serial.tsv` (scene, seconds, `ok`/`FAILED`), whatever the filter (a shard is not a child
run). `scripts/ui-shots.sh [filter] [--png] [--all-themes] [--changed <base>] [-j N]` builds once
(`cargo build --features ui-shots`), starts `N` shards of `target/debug/fuxx` side by side (default CPU cores − 2, at most
8, at least 1: **8** on this 10-core Mac; each line prefixed `[I/N]`), then the serial scenes in one process
(`[serial]`), merges the parts into `timings.tsv` on a full run (no filter, no `--changed`, every part written), and
prints the summary (`ui-shots: <passed> passed, <failed> failed, <total> s`, the wall time after the build, then the 15
slowest); exit 1 if any process failed. bash 3.2 (no associative arrays, `wait -n` or `mapfile`); Ctrl-C ends the
shards too.

**What A5 does:** the GPUI test clock (`advance_clock`) moves only GPUI timers and `executor.now()`; the app reads
`std::time::Instant::now()` for its slides (`sidebar_toggle`, `drag_gap`), the scrollbar fade, the age labels and the
file tab's 2 s look-up gate, and the scenes already pair those sleeps with `advance_clock` for the timer half. So a
sleep moves to the test clock only where it waited on GPUI's own executor. Three did, all in `terminal.rs`: a copy's
text is read on the background executor, which in `VisualTestAppContext` is the test scheduler that a frame's
`run_until_parked` runs to the end, so `copied` reads the clipboard after one frame (it polled up to 3 s, and waited the
whole 3 s where nothing is copied), the "off: nothing is copied" check drops its 100 ms and the disabled chip
buttons drop their 3 × 50 ms. `command-blocks-copy` 3.3 → 0.2 s, `command-blocks-rerun` 1.7 → 1.3 s. Every other sleep
waits on the real clock or a real process and stays.

**What "serial" means (ruling 7, Task 8 follow-up):** a scene runs alone, after the shards, only if (a) it starts a
child fuxx, (b) it changes shared state other scenes read, or (c) a check's result depends on elapsed real time, so a
slowed process can fail it ("still shown after X", "slid at most Y", "nothing moved in 200 ms"). A scene that only polls
until a condition with a deadline (real git, a real shell or pty), or sleeps for *at least* some time (a slide's end, a
fade's end), runs in the shards: load only makes it take longer. Each serial scene says its reason beside
`serial: true` in its area file.

**Serial (14 scenes; seconds alone, guard run 2):**

| Reason | Scene | Why |
| --- | --- | --- |
| (a) | `file-tab-md-mermaid` 17.0 | Mermaid's WebKit child fuxx, first and alone (Call 2) |
| (a) | `chrome-kill-switch` 1.9 | a child fuxx with `FUXX_CHROME_CACHE=off` |
| (b) | `files` 2.9 | adds `src/zz.rs` to the shared Files fixture for a while; 7 other scenes list or open it |
| (c) | `title-bar` 2.6 | `title-bar-sliding`: a shot mid-slide, the slide set 60 ms back by the real clock (one run in 11 drifted, Task 7) |
| (c) | `sidebar-button` 0.4 | a shot mid-slide, the slide set 60 ms back by the real clock |
| (c) | `right-sidebar` 2.8 | checks the slide still runs right after ⌥⌘\ (`sidebar_toggle::SLIDE`, 160 ms, real clock) |
| (c) | `chrome-ages-current` 2.1 | "59s" set 59.6 s back must still read 59s: a 400 ms margin |
| (c) | `chrome-rows-once` 0.6 | the same 400 ms margin on api's age |
| (c) | `activity` 2.8 | "nothing moves" over 200 ms of real time (the panel shows real ages) |
| (c) | `command-blocks` 1.3 | "nothing animates" over 200 ms of real time, chip open |
| (c) | `welcome-checking` 1.1 | "nothing moves" over 200 ms of real time |
| (c) | `palette-motion` 1.0 | "nothing moves" over 200 ms of real time |
| (c) | `cursor` 1.4 | the glide (`GLIDE`, 90 ms between its first paint and the last), the sparks (`SPARK_LIFE`, 350 ms) and the breath's 600 ms solid hold, all painted from `Instant::now()` (fix round 2; it also polls `cat`'s echo) |
| (c) | `file-tab-scrollbar` 3.0 | the thumb checked *shown* after a scroll: it holds 1 s from the scroll by the real clock (`scrollbar::HOLD`) |

**Moved into the shards (17), each with why it can't fail when slowed:**

| Scene | Why it is safe beside the others |
| --- | --- |
| `file-tab` | Its real-clock waits: the drag-select 300 ms sleep only lets the fade advance (its checks read the auto-scroll, which runs on GPUI timers); "no thumb at the edge" is hover-held (`hot`, no clock); "gone 1.5 s after leaving" is at least 1.2 s (`HOLD` + `FADE`); git polls wait up to 20 s (the look-up gap is 2 s); "a frame in < 1 s" measured 1 ms; "edited just now" holds for 60 s. Its clipboard is its app's own. |
| `file-diff` | The clipboard is per app (`VisualTestPlatform::clipboard`, `visual_test.rs:36`); the Mac's pasteboard `changeCount` didn't move. No real-time wait. |
| `command-blocks-copy` | The same clipboard; A5 left it no real-time wait. |
| `command-blocks-rerun` | Polls the pty's echo (3 s deadline); "nothing more came" waits at least 200 ms for an echo that must never come. |
| `cursor-flood` | Reads until the flood's END plus 500 ms quiet (60 s deadline); checks only the last frame. |
| `branch-switcher-real`, `-confirm`, `-open` | Poll real git (`settle_until`, 20 s). |
| `chrome-places-programs-ports` | Polls a real shell's folder, programs and ports (20 s deadline). |
| `chrome-folder-polled` | Polls a real shell's working folder (5 s deadline). |
| `tab-drag` | Its pixel checks follow `settle_slides`: at least `drag_gap::SLIDE` + 30 ms, then a fresh frame. |
| `sidebar` | The same `settle_slides` before its drop and pixel checks; its ages are minutes from a turn. |
| `chrome-drag-cancel` | `drag_to` sleeps at least `SLIDE` + 30 ms before its checks. |
| `chrome-notify-redraws-parts`, `chrome-notifications`, `chrome-resize-title` | Draw frames until the sidebar's slide ends (3.2-5 s deadlines), then check. |
| `chrome-bars-and-edges` | The right sidebar's step sleeps at least 400 ms (a 160 ms slide); the sidebar's slide is polled to its end. |

**The Files fixture** keeps its one path (the panel and the status bar draw it, so a folder per process would change
pictures). Its files are written only when missing or different, then whole (to a temp file, renamed in): a file tab open
on one in another process never sees it change. Their "edited …" age is therefore the fixture's, and `changes-click` and
`files-panel-follows-active-tab` (both themes) draw it: those 4 pictures now vary with the fixture's age, like the 7
that vary with real ages.

**Numbers** (this Mac, debug build, after the build, `-j 8`):

| Run | One process (Task 7) | Task 8, 31 serial | **Ruling 7, 13 serial** (guard) |
| --- | --- | --- | --- |
| Default `scripts/ui-shots.sh` | 211.6 s | 133.7 s (median of five) | **81.4 s** (median of five: 80.9, 81.5, 81.4, 81.3, 82.0) |
| Serial process | n/a | 114 s | **39.4-39.7 s** |
| Longest shard | n/a | 17.7 s | 38.8-39.9 s (`file-tab` alone, 33 s unloaded) |
| `--png --all-themes` | 276.3 s | 161.0 s | **99.2 s** |

**Fix round 2** put `cursor` back in the serial list (14 serial, 17 in the shards; it moves only toward serial, so the
guard wasn't rerun): one full run **82.5 s**, the serial process **41.0 s**, the longest shard 39.0 s, 112 passed.

The run is now two halves of about 40 s: the shards, bounded by `file-tab` on its own shard, and the serial process,
of which Mermaid is 17 s. Below about 80 s would need `file-tab` itself faster (or split, which would change its name),
or Mermaid's WebKit run shorter. **The guard:** five consecutive full runs at the default `-j 8`, 112 passed and
0 failed each; nothing heavy ran beside them (load average 1.9-2.9 from the runs themselves; at most a VM helper at
19 % and Docker Desktop at 18 % in one sample each). The default stays `min(8, cores − 2)`. (The first round's guard,
with 31 serial: five runs 134.1, 133.7, 133.6, 133.9, 133.5 s, all passing.)

**Same results:** the `--png --all-themes` run prints the same 207 `ok` lines (sorted; 204 different) as Task 7 and
writes the same 451 PNGs. 440 are byte-identical. The other 11 are the 7 that vary between runs of an unchanged build
(`activity`, `activity-waiting` ×2 themes, `branch-switcher-open-mocha`, `welcome-checking` ×2) and the 4 that show the
fixture's age (`changes-click` ×2, `files-panel-follows-active-tab` ×2: only the file bar's "edited 27 min ago", pixels
y 63-74).

**The script** also: finds the binary from `cargo build --message-format=json-render-diagnostics` (so
`CARGO_TARGET_DIR` is honoured); on Ctrl-C (or `kill -INT`) ends only its own parts and their processes, removes its temp
folder and exits 130; says when a part wrote no results ("shard 1/2 wrote no results: its scenes are not counted") as
well as how it exited.

### Part A — Quick wins

- **A1 · Less setup per shot.** Reuse what's safe to reuse, chosen from Part 0's numbers: at least the font
  registration once per process; ideally one app per scene function (its shots share it, each in its own window, closed
  after capture). **Each scene still starts from a fresh app**, so no state leaks between scenes.
- **A2 · PNGs only when wanted.** Shots stay in memory for the checks. PNGs are written when:
  - you ask (`--png`, or `UI_SHOTS_PNG=1`) — for a task's look at its own scenes;
  - a scene fails: its most recent shot(s) go to `target/ui-shots/failed/` and the failure message names the file.
- **A3 · One theme by default.** A scene declares `Themes::One` (Mocha) or `Themes::Both`. **Both** for any scene that
  checks colours, contrast or a theme-specific look (the split audit, B1, decides each). `--all-themes` shoots every
  scene in both; the milestone-end run uses it.
- **A4 · Parallel processes.** `__ui-shots --shard I/N` runs one Nth of the scenes, balanced by `timings.tsv` (round
  robin when there's none). A script, `scripts/ui-shots.sh`, builds once, starts N shards (default: CPU cores − 2, at
  most 8), prefixes each line with its shard, waits for all, and prints one summary (passed, failed, slowest, total).
  Scenes marked **`serial`** (real-time animations and timing margins) run alone after the shards.
- **A5 · Fewer real-time waits.** The 22 `thread::sleep`s are replaced by GPUI's test clock where the code under test
  uses GPUI timers; the ones that must stay real-time mark their scene `serial`.

### Part B — Restructure

- **B1 · Split by area.** `src/ui_shots/` becomes:
  - `mod.rs` — the runner, the command line (`[filter]`, `--shard`, `--png`, `--all-themes`, `--changed`), the registry;
  - `harness.rs` — `render`, `shoot`, `Shot`, pixel helpers, `Steps` (from `chrome_sources.rs`), the failure-picture
    hook;
  - one file per area, e.g. `tokens.rs`, `window.rs` (title bar, window chrome), `sidebar.rs`, `panes.rs` (tabs,
    splits, drag), `terminal.rs` (terminal, blocks, links, marks), `file_tab.rs` (file, diff, Markdown, Mermaid),
    `right_sidebar.rs` (Files, Changes, Graph, branch), `overlays.rs` (palette, preferences, menus, notifications),
    `browser.rs`, `chrome.rs` (today's `chrome_sources.rs`). The final split follows what the scenes actually cover; no
    file over about 3,000 lines.
  - The test-only hooks in app modules (`#[cfg(feature = "ui-shots")]` blocks) stay where they are.
- **B2 · Scenes declared, not just listed.** Each area file exposes its scenes:
  `Scene { name, run, tags, themes, serial, sources }` — `sources` are the app files it covers (e.g. `file-tab` covers
  `src/file_tab.rs`, `src/md_preview.rs`, `src/scrollbar.rs`). The order of today's `SCENES` list is kept for a plain
  run.
- **B3 · Run what a change touches.** `__ui-shots --changed <base>` asks git for the files changed since `<base>` and
  runs the scenes whose `sources` include one. A change to a shared file (`src/ui_shots/harness.rs`, `src/theme.rs`,
  `src/ui_tokens.rs`, `src/workspace.rs`, `Cargo.toml`, …; the list lives in `mod.rs`) runs everything. A changed file
  no scene claims is printed, so the map can be kept honest.
- **B4 · The workflow.** Plans' gates change to: per task, `scripts/ui-shots.sh --changed <task base>` (plus `--png`
  for the scenes the task looks at); once per milestone, the full `scripts/ui-shots.sh --all-themes`. The vault's
  [[50 - Agentic Workflow]] and the plan template say so; AGENTS.md is unchanged.

### Part C — Fewer screenshot tests

- **C1 · The rule from now on:** logic that doesn't need a real window (what a view would show, positions from
  geometry, labels, state transitions) lives in plain functions tested by `cargo test`; a scene is for what needs a
  window: layout, pixels, real mouse and key events, focus. Written into [[50 - Agentic Workflow]].
- **C2 · Sort, then move the easy ones.** During B1 each scene is tagged `visual` or `state` (its checks only read view
  state). `state` scenes whose logic can be lifted into pure functions without changing app behaviour move to
  `cargo test` in this milestone; the rest are listed in this note, and later milestones move them when they touch that
  view.
- **C3 · Reference fingerprints (a trial).** For `visual` scenes, a compact fingerprint of each shot (downscaled
  greyscale, a few KB) saved in `tests/ui-shots-ref/` and compared with a tolerance; `--update-refs` accepts new ones.
  In this milestone it only **reports** differences (never fails a run); whether it should fail runs is your call after
  you've seen it in use. (Full PNG references are too big for a public repo: ~770 MB.)

## Constraints

- No new crates; the script is plain bash (macOS's bash 3.2 and zsh-safe).
- AppKit: one window-owning thread per process — parallelism is processes, never threads.
- GPUI is the pinned rev; any API used (`VisualTestAppContext`, test clock, offscreen windows) is read from its source.
- Every existing scene name stays; every existing check stays (in a scene or in `cargo test`).
- The test hooks inside app modules are not moved or widened.
- Measured on this Mac with nothing else heavy running; numbers recorded here.

## Calls made after the builder check (AI's, for you to check)

A fresh reviewer read this design as if it had to plan from it; its questions were settled as follows.

1. **A scene stays one function with its current name.** It gains a small context argument that says which themes to
   shoot (`for (slug, name) in shots.themes()` instead of `SHOT_THEMES`); shot file names (`smoke-mocha`) and substring
   filters are unchanged. `Themes::One` / `Both`, `serial`, shards and `--changed` all work per scene function.
2. **Scenes that re-run fuxx as a child process** (the chrome-cache-off check, `FUXX_CHROME_CACHE=off`; Mermaid's
   WebKit run, `FUXX_SHOTS_WEBKIT`) keep that pattern. They are **`serial`** automatically, keep their required order
   (Mermaid's WebKit child first and alone, as its comment requires), and their time counts toward their parent scene.
3. **The 90 s and 30 s numbers are goals, not gates.** Part 0 measures first; if its numbers show a goal can't be
   reached (for example real-time waits or GPU capture dominate), the note records why and the expected number before
   Part A is built, and you're told.
4. **Parallel runs that flake fall back safely:** fewer shards first, then more scenes marked `serial`. A shard count
   that flakes is never the default.
5. **Failure pictures:** a scene's shots stay in memory until it passes; when it fails, all of its shots are written to
   `target/ui-shots/failed/`. Checks that read a PNG from disk today (`smoke` does) keep doing so under `--png`, or are
   changed to check the in-memory shot.
6. **Real-time waits:** those that wait on GPUI timers move to the test clock; animations measured with wall-clock time
   (`drag_gap::SLIDE`, `sidebar_toggle::SLIDE`) and WebKit stay real and make their scene `serial`. Part 0 reports how
   much time those take.
7. **`sources` are written by hand** in each area file. A changed `src/` file that no scene claims runs **everything**
   (and is printed so the map gets fixed); changes outside `src/`, `Cargo.toml` and `assets/` (docs, scripts) run
   nothing.
8. **C3 (fingerprints) is the last task and can be dropped:** if it threatens the rest, it moves to a follow-up and
   you're asked.

## Left to the builder

- The exact area split (B1) and which scenes are `Both` / `serial` / `state` — from reading each scene.
- What A1 reuses — from Part 0's numbers.
- How shards are balanced beyond "by recorded time".
- The fingerprint's size and tolerance (C3).

## On-screen check (yours, after the build)

1. `scripts/ui-shots.sh` runs the full suite in your Terminal in about 90 s or less (after the build) and prints a clear
   summary.
2. `scripts/ui-shots.sh --changed master` after a small file-tab change runs only file-tab scenes.
3. A deliberately broken check fails with its picture in `target/ui-shots/failed/`.
4. `--png` writes the pictures for the scenes you ran.

## Related

[[40 - Roadmap]] · [[80 - Milestone 11ae - File Tab Chores]] · [[50 - Agentic Workflow]] ·
[[77 - Milestone 11p - Chrome Caching]] (its render-count scenes live in `chrome_sources.rs`)
