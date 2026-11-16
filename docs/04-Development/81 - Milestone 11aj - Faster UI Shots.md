# Milestone 11aj — Faster UI Shots

**Status:** 🟨 built 2026-11-17 on branch `m11aj-ui-shots` (from master `4883073`), whole-branch review and fix wave clean, **merged to master 2026-11-17, not pushed**, all 11 plan tasks;
waiting for your on-screen check (below). Design approved 2026-11-16 ("approved"); plan [[M11aj Plan - Faster UI Shots]].

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

### B3 — measured (M11aj Task 9, 2026-11-17)

`scripts/ui-shots.sh --changed HEAD` after a comment edit, on a clean tree:

| Edit | Decision | Result |
|---|---|---|
| `src/file_tab.rs` | 13 scenes (file-tab, file-diff, file-tab-md, files, changes, palette, welcome-start, status-bar, ...) | **58.8 s** (65 s with the build) |
| `src/ui_shots/file_tab.rs` (a scene's own file) | 3 scenes: file-tab, file-diff, file-tab-scrollbar | 36.9 s |
| `src/ui_shots/registry.rs` | nothing (no src changes; since the final review: "only runner files changed: src/ui_shots/registry.rs") | 5 s |
| a docs file | nothing (no src changes) | 4 s |
| `src/theme.rs` | all (shared: src/theme.rs) | a full run, 82.8 s |

A filter that names no scene (`scripts/ui-shots.sh zzz-typo --changed HEAD`) prints "no scene matches" and exits 1,
as it does without `--changed`; "nothing to run" (exit 0 from the script) is only for a filter whose scenes the change
doesn't touch (final review fix, so a typo can't turn the gate green with nothing run).

Goal 1 (a task's gate near 30 s) is not met for a `file_tab.rs` change. All 13 claims are accurate, so this is
not over-claiming: the `file-tab` scene alone takes 34.7 s, and the WebKit Mermaid scene (`file-tab-md-mermaid`, serial)
adds about 17 s after the shards. Narrowing the gate further means splitting `file-tab` or moving its logic into
`cargo test` (C1/C2).

### C3 — measured (M11aj Task 11, 2026-11-17)

**What C3 does (a trial, report only):** after a `Kind::Visual` scene passes, each shot it kept (the in-memory shot, not
a PNG) is turned into a fingerprint: greyscale (Rec. 601 luma), 1/8 of the shot's point size, one byte a cell (8 × 8 pt;
16 × 16 device pixels on this Mac's 2x screen, measured once per process from a small window), each cell the mean of
its pixels. It is compared with `tests/ui-shots-ref/<scene>/<scene>-<slug>.fp` (the slug is the shot's stem as `shoot`
names it; the scene's name is not repeated where the stem already starts with it), as the mean absolute difference of
the cells (0–255; a size change is infinite). Above the tolerance the run prints
`ui-shots: <scene>-<slug> looks different (<d>)`; a fingerprint of another size prints `… differs in size from its
reference` instead. Each process ends with `ui-shots: fingerprints: <n> same, <n> look different, <n> without a
reference`, adding `<n> differ in size (another screen scale?)` when there are any, so a run on a 1x screen doesn't read
as a regression; `scripts/ui-shots.sh` sums the parts' lines into one `fingerprints, all parts: …` line in its summary. **It never fails a run.** `--update-refs` (also through `scripts/ui-shots.sh`) writes the references
instead, always in both themes (it implies `--all-themes`). With no filter and no `--changed`, it replaces each
scene's folder and then, once (the direct or serial run), removes any folder no visual scene claims, printing it; with
a filter or `--changed` it rewrites only the shots it took. A `cargo test` (`ui-shots` feature) fails if a folder in
`tests/ui-shots-ref/` belongs to no visual scene, and `.gitattributes` marks the references binary.
Skipped: `State` scenes, the 11 pictures that vary between runs of unchanged code (`VARIES` in `harness.rs`: `activity`,
`activity-waiting`, `welcome-checking`, `changes-click`, `files-panel-follows-active-tab` in both themes, and
`branch-switcher-open-mocha`), child runs (cache-off, WebKit) and failed scenes.

**Size:** written with `--all-themes --update-refs`: **393 references, 2,685,502 bytes (2.56 MB)**, under the 5 MB cap,
so they are committed. The plain per-cell layout was enough; no coarser grid or packing was needed. The largest is
19,775 bytes (each `preferences` page, 1400 × 900 pt). 58 of the 79 visual scenes have references; the other 21
(`cached-panes`, `welcome-checking` and 19 chrome scenes) check pixels in memory and keep no shot.

**Tolerance: 0.01.** Five full `scripts/ui-shots.sh --all-themes` runs of unchanged code, each with every distance
printed (`UI_SHOTS_FP_DISTANCES=1`): 112 passed each, 92.8–93.8 s, and **all 393 distances were exactly 0 in every run**
(the largest seen: 0). So the tolerance sits just above 0: 0.01 is, in a 1,000-cell shot, one cell 10 grey levels
off. Checked by hand: one cell of `smoke-mocha`'s reference moved by 80 (of 1,000) printed
`smoke-mocha looks different (0.080)` with exit 0; a cut-short reference printed `(inf)`.

**Cost:** about 1 s on a full run (83.7 s, against 82.7 s at Task 10).

### Part A — Quick wins

- **A1 · Less setup per shot.** Reuse what's safe to reuse, chosen from Part 0's numbers: at least the font
  registration once per process; ideally one app per scene function (its shots share it, each in its own window, closed
  after capture). **Each scene still starts from a fresh app**, so no state leaks between scenes.
- **A2 · PNGs only when wanted.** Shots stay in memory for the checks. PNGs are written when:
  - you ask (`--png`, or `UI_SHOTS_PNG=1`) — for a task's look at its own scenes;
  - a scene fails: its most recent shot(s) go to `target/ui-shots/failed/` and the failure message names the file.
    (As built: only the shots the scene kept, through `shoot` / `shoot_with`, `save_shot` or `keep_shot`. A
    `render` / `render_with` result never passed to `save_shot`, and a failed assert inside an `act` closure, which
    fails before the capture, leave no picture.)
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
   `target/ui-shots/failed/`. (As built, "all of its shots" means the shots it kept, through `shoot`, `save_shot` or
   `keep_shot`: a `render` / `render_with` result not passed to `save_shot`, and an assert inside `act` that fails
   before the capture, leave none.) Checks that read a PNG from disk today (`smoke` does) keep doing so under `--png`, or are
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

## What was built

All eleven plan tasks, on branch `m11aj-ui-shots` (from master `4883073`). No new crates; the app's code is unchanged
apart from the test hooks it already had and one pure helper's tests (Task 10).

1. **Measured first (Task 1).** Per-scene times, the 15 slowest, `timings.tsv`, `UI_SHOTS_PROFILE=1`'s per-phase sums
   ("Part 0 — measured").
2. **The harness in its own file (Task 2).** `src/ui_shots.rs` became `src/ui_shots/mod.rs` (`git mv`) and the shared
   harness `harness.rs`; no logic edits.
3. **A scene registry and run options (Task 3).** `Scene { name, run, themes, serial, kind, sources }`; `[filter]
   [--png] [--all-themes] [--shard I/N | --serial-only] [--changed <base>] [--update-refs]`; PNGs only with `--png`; a
   failed scene's shots in `target/ui-shots/failed/<scene>/`.
4. **Split by area (Tasks 4–6).** 16 area files (`window`, `sidebar`, `panes`, `chrome_parts`, `chrome`, `file_tab`,
   `file_tab_md`, `right_sidebar`, `changes`, `branch`, `naming`, `palette`, `welcome`, `overlays`, `status_bar`,
   `terminal`), each scene tagged (the appendix). The largest file is `window.rs`, 2,700 lines; `registry.rs` holds the
   areas and `ORDER`.
5. **Less setup per shot (Task 7, A1).** The terminals are hung up together before a shot's app drops (the teardown was
   portable-pty's 50 ms wait per terminal), and the fonts are registered once per process.
6. **Parallel shards and the script (Task 8, A4, A5).** `scripts/ui-shots.sh` builds once, runs 8 shards side by side
   and then the 14 `serial` scenes alone, and prints one summary; three waits moved to the test clock.
7. **Run what a change touches (Task 9, B3, B4).** `--changed <base>`; each area file claims its own scenes; the
   per-task and per-milestone gates are in [[50 - Agentic Workflow]].
8. **State-only checks (Task 10, C1, C2).** The rule is written down; no scene could move whole (each goes through a
   `Workspace` or real keys, focus or processes); `help`'s window-free checks moved to `help_shortcuts` tests.
9. **Reference fingerprints (Task 11, C3).** Report only ("C3 — measured").

## Final numbers (this Mac, debug build, 2026-11-17)

| Run | Before (Task 1) | Now |
| --- | --- | --- |
| Full `scripts/ui-shots.sh` (after the build) | 507.4 s, one process | **83.7 s** (112 passed; 84.3 s with the 0.5 s no-op build) |
| `scripts/ui-shots.sh --all-themes` | 511.7 s (`--png --all-themes`, Task 7's before) | **93.1 s** (112 passed, 393 fingerprints the same) |
| `--changed HEAD`, one-line comment in `src/file_tab.rs` | n/a | 13 scenes, **59.0 s** (61.7 s with the build) |
| `--changed HEAD`, one-line comment in `src/ui_shots/file_tab.rs` (a scene file only) | n/a | 3 scenes (`file-tab`, `file-diff`, `file-tab-scrollbar`), **37.1 s** (39.9 s with the build) |
| Build (`cargo build --features ui-shots`) | n/a | **2.7–4.0 s** after a one-line change; 14.8 s for the `fuxx` crate from scratch (dependencies built) |

**Against the goals:** (1) the full suite in ≤ 90 s: **met**, 83.7 s (6x faster). A task's own scenes in ≤ 30 s: **not
met** for the file tab: `file-tab` alone is 33–40 s and a `src/file_tab.rs` change also runs the WebKit Mermaid scene
(17 s, serial) and 11 more that read that file; going lower needs `file-tab` split or its logic moved to `cargo test`.
(2) No file over about 3,000 lines: **met** (largest 2,700). Adding a scene touches its area file and also
`ORDER` in `registry.rs` (one line, its run position; a startup check and a test fail if it's missing). (3) Nothing lost: **met**, the same 112 scenes and names,
every check kept. (4) Fewer screenshot tests over time: the rule is in place; none moved in this milestone (Task 10).

## On-screen check (yours, after the build)

1. `scripts/ui-shots.sh` runs the full suite in your Terminal in about 90 s or less (after the build) and prints a clear
   summary.
2. `scripts/ui-shots.sh --changed master` after a small file-tab change runs only file-tab scenes.
3. A deliberately broken check fails with its picture in `target/ui-shots/failed/`.
4. `--png` writes the pictures for the scenes you ran.
5. **Your call (C3):** after seeing the fingerprints in use (a run prints `fingerprints: … same, … look different`), say
   whether "looks different" should stay a report or fail runs.

## Related

[[40 - Roadmap]] · [[80 - Milestone 11ae - File Tab Chores]] · [[50 - Agentic Workflow]] ·
[[77 - Milestone 11p - Chrome Caching]] (its render-count scenes live in `src/ui_shots/chrome.rs` and
`chrome_parts.rs`)

## Appendix — every scene

Generated from the registry (`registry.rs`'s `AREAS` and `ORDER`, each area file's `SCENES`, the serial reason from
the comment beside `serial: true`), in run order. Every scene's area file is also among its sources (for
`--changed`). 112 scenes: 52 `Both`, 60 `One`; 79 visual, 33 state; 14 serial.

| # | Scene | Area file | Themes | Serial (why) | Kind | Sources (besides its area file) |
|---|---|---|---|---|---|---|
| 1 | `smoke` | `window.rs` | One (Mocha) | no | visual | none |
| 2 | `tokens` | `window.rs` | Both | no | visual | `src/theme.rs`, `src/ui_tokens.rs` |
| 3 | `window` | `window.rs` | Both | no | visual | `src/workspace.rs`, `src/panes.rs`, `src/terminal_view.rs`, `src/sidebar.rs`, `src/tab_bar.rs`, `src/status_bar.rs` |
| 4 | `marks` | `window.rs` | One (Mocha) | no | visual | `src/status_mark.rs` |
| 5 | `sidebar-marks` | `sidebar.rs` | One (Mocha) | no | visual | `src/sidebar.rs`, `src/status_mark.rs`, `src/mark_view.rs`, `src/procs.rs` |
| 6 | `cached-panes` | `panes.rs` | One (Mocha) | no | visual | `src/terminal_view.rs`, `src/workspace.rs` |
| 7 | `chrome-breath-skips-parts` | `chrome_parts.rs` | One (Mocha) | no | state | `src/chrome_cache.rs`, `src/sidebar.rs`, `src/status_bar.rs`, `src/tab_bar.rs`, `src/title_area.rs` |
| 8 | `chrome-notify-redraws-parts` | `chrome_parts.rs` | One (Mocha) | no | visual | `src/chrome_cache.rs`, `src/sidebar.rs`, `src/status_bar.rs`, `src/tab_bar.rs`, `src/title_area.rs`, `src/sidebar_toggle.rs`, `src/notification_list.rs` |
| 9 | `chrome-pane-churn` | `chrome_parts.rs` | One (Mocha) | no | visual | `src/chrome_cache.rs`, `src/panes.rs`, `src/split_tree.rs`, `src/sidebar.rs`, `src/status_bar.rs`, `src/tab_bar.rs`, `src/title_area.rs` |
| 10 | `chrome-kill-switch` | `chrome_parts.rs` | One (Mocha) | yes: (a) a child fuxx (FUXX_CHROME_CACHE=off). | state | `src/chrome_cache.rs`, `src/sidebar.rs`, `src/status_bar.rs`, `src/tab_bar.rs`, `src/title_area.rs` |
| 11 | `chrome-pixels-same` | `chrome_parts.rs` | One (Mocha) | no | visual | `src/chrome_cache.rs`, `src/sidebar.rs`, `src/status_bar.rs`, `src/tab_bar.rs`, `src/title_area.rs` |
| 12 | `chrome-ages-current` | `chrome_parts.rs` | One (Mocha) | yes: (c) "59s" read within 400 ms by the real clock. | visual | `src/chrome_cache.rs`, `src/sidebar.rs`, `src/status_bar.rs`, `src/status.rs` |
| 13 | `chrome-working-marks` | `chrome_parts.rs` | One (Mocha) | no | state | `src/chrome_cache.rs`, `src/mark_view.rs`, `src/status_mark.rs`, `src/sidebar.rs`, `src/status_bar.rs`, `src/tab_bar.rs`, `src/title_area.rs` |
| 14 | `chrome-folder-polled` | `chrome_parts.rs` | One (Mocha) | no | state | `src/chrome_cache.rs`, `src/git_branch.rs`, `src/sidebar.rs`, `src/status_bar.rs` |
| 15 | `chrome-followed` | `chrome_parts.rs` | One (Mocha) | no | state | `src/chrome_cache.rs`, `src/status_bar.rs`, `src/right_sidebar.rs` |
| 16 | `chrome-title-once` | `chrome_parts.rs` | One (Mocha) | no | state | `src/chrome_cache.rs`, `src/title_area.rs`, `src/workspace.rs` |
| 17 | `chrome-rows-once` | `chrome_parts.rs` | One (Mocha) | yes: (c) "59s" read within 400 ms by the real clock. | visual | `src/chrome_cache.rs`, `src/sidebar.rs` |
| 18 | `chrome-status-elsewhere` | `chrome.rs` | One (Mocha) | no | visual | `src/chrome_cache.rs`, `src/sidebar.rs`, `src/tab_bar.rs`, `src/status_mark.rs`, `src/status.rs` |
| 19 | `chrome-model-edits` | `chrome.rs` | One (Mocha) | no | visual | `src/chrome_cache.rs`, `src/layout_model.rs`, `src/sidebar.rs`, `src/tab_bar.rs`, `src/tab_menu.rs`, `src/title_area.rs` |
| 20 | `chrome-notifications` | `chrome.rs` | One (Mocha) | no | visual | `src/chrome_cache.rs`, `src/notification_list.rs`, `src/title_area.rs`, `src/sidebar.rs` |
| 21 | `chrome-places-programs-ports` | `chrome.rs` | One (Mocha) | no | visual | `src/chrome_cache.rs`, `src/git_branch.rs`, `src/procs.rs`, `src/sidebar.rs`, `src/status_bar.rs` |
| 22 | `chrome-globals` | `chrome.rs` | Both | no | visual | `src/chrome_cache.rs`, `src/appearance.rs`, `src/reduce_motion.rs`, `src/updater.rs`, `src/update_release.rs`, `src/sidebar.rs`, `src/status_bar.rs`, `src/tab_bar.rs`, `src/title_area.rs` |
| 23 | `chrome-resize-title` | `chrome.rs` | One (Mocha) | no | visual | `src/chrome_cache.rs`, `src/title_area.rs`, `src/sidebar.rs` |
| 24 | `chrome-drag-cancel` | `chrome.rs` | One (Mocha) | no | visual | `src/chrome_cache.rs`, `src/drag_gap.rs`, `src/sidebar.rs`, `src/tab_bar.rs` |
| 25 | `chrome-renaming` | `chrome.rs` | One (Mocha) | no | visual | `src/chrome_cache.rs`, `src/name_input.rs`, `src/sidebar.rs`, `src/tab_bar.rs`, `src/tab_menu.rs` |
| 26 | `chrome-new-bar-reveal` | `chrome.rs` | One (Mocha) | no | visual | `src/chrome_cache.rs`, `src/tab_bar.rs` |
| 27 | `chrome-session-facts` | `chrome.rs` | One (Mocha) | no | visual | `src/chrome_cache.rs`, `src/sidebar.rs`, `src/tab_bar.rs`, `src/terminal_view.rs`, `src/ssh_target.rs` |
| 28 | `chrome-bars-and-edges` | `chrome.rs` | One (Mocha) | no | visual | `src/chrome_cache.rs`, `src/sidebar.rs`, `src/tab_bar.rs`, `src/sidebar_toggle.rs`, `src/right_sidebar.rs`, `src/title_area.rs` |
| 29 | `chrome-sidebar-scroll` | `chrome.rs` | One (Mocha) | no | visual | `src/chrome_cache.rs`, `src/sidebar.rs` |
| 30 | `chrome-sidebar-header-mark` | `chrome.rs` | One (Mocha) | no | visual | `src/chrome_cache.rs`, `src/sidebar.rs`, `src/groups.rs`, `src/status_mark.rs` |
| 31 | `chrome-row-sources` | `chrome.rs` | One (Mocha) | no | visual | `src/chrome_cache.rs`, `src/sidebar.rs`, `src/git_branch.rs`, `src/ssh_target.rs`, `src/restore.rs` |
| 32 | `chrome-file-tab-toolbar` | `chrome.rs` | One (Mocha) | no | visual | `src/chrome_cache.rs`, `src/file_tab.rs`, `src/tab_bar.rs` |
| 33 | `sidebar` | `sidebar.rs` | Both | no | visual | `src/sidebar.rs`, `src/drag_gap.rs`, `src/groups.rs`, `src/workspace.rs` |
| 34 | `sidebar-rows` | `sidebar.rs` | Both | no | visual | `src/sidebar.rs`, `src/groups.rs`, `src/git_branch.rs` |
| 35 | `sidebar-selection` | `sidebar.rs` | Both | no | visual | `src/sidebar.rs`, `src/status_mark.rs` |
| 36 | `sidebar-button` | `sidebar.rs` | Both | yes: (c) a shot mid-slide, the slide set 60 ms back by the real clock. | visual | `src/sidebar.rs`, `src/sidebar_toggle.rs`, `src/title_area.rs` |
| 37 | `title-area` | `window.rs` | Both | no | visual | `src/title_area.rs`, `src/notification_list.rs`, `src/sidebar.rs`, `src/sidebar_toggle.rs`, `src/tab_menu.rs` |
| 38 | `title-bar` | `window.rs` | Both | yes: (c) a shot mid-slide, the slide set 60 ms back by the real clock. | visual | `src/title_area.rs`, `src/sidebar_toggle.rs`, `src/notification_list.rs`, `src/sidebar.rs`, `src/right_sidebar.rs` |
| 39 | `tabs` | `panes.rs` | Both | no | visual | `src/sidebar.rs`, `src/tab_color.rs`, `src/tab_menu.rs`, `src/palette.rs` |
| 40 | `splits` | `panes.rs` | Both | no | visual | `src/panes.rs`, `src/split_tree.rs`, `src/tab_bar.rs`, `src/workspace.rs` |
| 41 | `cards` | `window.rs` | Both | no | visual | `src/workspace.rs`, `src/panes.rs`, `src/right_sidebar.rs`, `src/sidebar.rs` |
| 42 | `cards-split` | `window.rs` | Both | no | visual | `src/panes.rs`, `src/split_tree.rs`, `src/workspace.rs` |
| 43 | `cards-corners` | `window.rs` | Both | no | visual | `src/panes.rs`, `src/terminal_view.rs` |
| 44 | `workspaces` | `panes.rs` | One (Mocha) | no | state | `src/sidebar.rs`, `src/layout_model.rs`, `src/workspace.rs` |
| 45 | `tab-bars` | `panes.rs` | Both | no | visual | `src/tab_bar.rs`, `src/pane_tab_menu.rs`, `src/tab_menu.rs`, `src/layout_model.rs` |
| 46 | `tab-drag` | `panes.rs` | Both | no | visual | `src/tab_bar.rs`, `src/drag_gap.rs`, `src/layout_model.rs`, `src/sidebar.rs`, `src/pane_browser.rs` |
| 47 | `browser-tab` | `panes.rs` | Both | no | visual | `src/browser_tab.rs`, `src/browser_history.rs`, `src/pane_browser.rs`, `src/browser_url.rs` |
| 48 | `file-tab` | `file_tab.rs` | Both | no | visual | `src/file_tab.rs`, `src/layout_model.rs`, `src/tab_bar.rs`, `src/highlight.rs`, `src/git_status.rs`, `src/git_branch.rs`, `src/scrollbar.rs`, `src/name_input.rs` |
| 49 | `file-diff` | `file_tab.rs` | Both | no | visual | `src/file_tab.rs`, `src/diff_view.rs`, `src/git_status.rs`, `src/highlight.rs`, `src/layout_model.rs`, `src/right_sidebar.rs`, `src/file_tree.rs`, `src/name_input.rs` |
| 50 | `file-tab-md` | `file_tab_md.rs` | Both | no | visual | `src/file_tab.rs`, `src/md_preview.rs`, `src/agent_markdown.rs`, `src/scrollbar.rs`, `src/name_input.rs` |
| 51 | `file-tab-md-mermaid` | `file_tab_md.rs` | One (Mocha) | yes: (a) a child fuxx: Mermaid's WebKit run, first and alone. | state | `src/file_tab.rs`, `src/md_preview.rs`, `src/mermaid.rs`, `src/mermaid_render.rs`, `src/agent_markdown.rs`, `src/browser.rs` |
| 52 | `file-tab-scrollbar` | `file_tab.rs` | One (Mocha) | yes: (c) the thumb still shown after a scroll (it fades 1 s on, real clock). | state | `src/file_tab.rs`, `src/scrollbar.rs` |
| 53 | `right-sidebar` | `right_sidebar.rs` | Both | yes: (c) the slide still running right after the key (160 ms, real clock). | visual | `src/right_sidebar.rs`, `src/sidebar_toggle.rs`, `src/tab_bar.rs`, `src/layout_model.rs`, `src/reduce_motion.rs` |
| 54 | `files` | `right_sidebar.rs` | Both | yes: (b) adds src/zz.rs to the shared Files fixture for a while. | visual | `src/right_sidebar.rs`, `src/file_tree.rs`, `src/git_status.rs`, `src/file_tab.rs`, `src/layout_model.rs`, `src/tab_bar.rs` |
| 55 | `files-follow` | `right_sidebar.rs` | One (Mocha) | no | state | `src/right_sidebar.rs`, `src/file_tree.rs`, `src/git_status.rs`, `src/file_tab.rs`, `src/layout_model.rs` |
| 56 | `right-panels` | `right_sidebar.rs` | Both | no | visual | `src/right_sidebar.rs`, `src/file_tree.rs`, `src/changes_panel.rs`, `src/activity_panel.rs` |
| 57 | `changes` | `changes.rs` | Both | no | visual | `src/changes_panel.rs`, `src/right_sidebar.rs`, `src/git_status.rs`, `src/git_stage.rs`, `src/ssh_target.rs`, `src/layout_model.rs`, `src/file_tab.rs` |
| 58 | `changes-bar` | `changes.rs` | Both | no | visual | `src/changes_panel.rs`, `src/right_sidebar.rs`, `src/branch_list.rs`, `src/branch_sync.rs`, `src/git_status.rs` |
| 59 | `changes-staging` | `changes.rs` | Both | no | visual | `src/changes_panel.rs`, `src/right_sidebar.rs`, `src/git_status.rs`, `src/git_stage.rs` |
| 60 | `changes-graph` | `changes.rs` | Both | no | visual | `src/changes_panel.rs`, `src/graph_panel.rs`, `src/git_graph.rs`, `src/graph_layout.rs`, `src/git_status.rs`, `src/right_sidebar.rs` |
| 61 | `changes-sticky` | `changes.rs` | Both | no | visual | `src/changes_panel.rs`, `src/graph_panel.rs`, `src/git_graph.rs`, `src/graph_layout.rs`, `src/git_status.rs`, `src/right_sidebar.rs` |
| 62 | `activity` | `changes.rs` | Both | yes: (c) nothing moves over 200 ms of real time. | visual | `src/activity.rs`, `src/activity_panel.rs`, `src/right_sidebar.rs`, `src/status.rs`, `src/reduce_motion.rs` |
| 63 | `api-background` | `naming.rs` | One (Mocha) | no | state | `src/workspace_api.rs`, `src/workspace_browser.rs`, `src/browser_tab.rs`, `src/layout_model.rs` |
| 64 | `banner-click` | `naming.rs` | One (Mocha) | no | state | `src/native_notify.rs`, `src/daemon_client.rs`, `src/layout_model.rs` |
| 65 | `default-name` | `naming.rs` | One (Mocha) | no | state | `src/groups.rs`, `src/tab_bar.rs`, `src/sidebar.rs` |
| 66 | `names` | `naming.rs` | One (Mocha) | no | state | `src/groups.rs`, `src/tab_bar.rs`, `src/sidebar.rs`, `src/name_input.rs`, `src/layout_model.rs`, `src/browser_tab.rs`, `src/workspace_browser.rs`, `src/presets.rs`, `src/ssh_target.rs`, `src/procs.rs` |
| 67 | `renames` | `naming.rs` | One (Mocha) | no | state | `src/name_input.rs`, `src/tab_bar.rs`, `src/sidebar.rs`, `src/groups.rs`, `src/procs.rs`, `src/terminal_view.rs`, `src/terminal.rs`, `src/layout_model.rs` |
| 68 | `rename-box` | `naming.rs` | Both | no | visual | `src/name_input.rs`, `src/sidebar.rs` |
| 69 | `palette` | `palette.rs` | Both | no | visual | `src/palette.rs`, `src/palette_items.rs`, `src/palette_match.rs`, `src/overlay_panel.rs`, `src/name_input.rs`, `src/help_shortcuts.rs`, `src/switcher.rs` |
| 70 | `palette-workspaces` | `palette.rs` | One (Mocha) | no | state | `src/palette.rs`, `src/palette_items.rs`, `src/palette_match.rs`, `src/overlay_panel.rs`, `src/name_input.rs`, `src/switcher.rs`, `src/layout_model.rs` |
| 71 | `palette-actions` | `palette.rs` | Both | no | visual | `src/palette.rs`, `src/palette_items.rs`, `src/palette_match.rs`, `src/overlay_panel.rs`, `src/name_input.rs`, `src/pane_actions.rs`, `src/help_shortcuts.rs`, `src/layout_model.rs` |
| 72 | `palette-keys` | `palette.rs` | One (Mocha) | no | state | `src/palette.rs`, `src/palette_items.rs`, `src/palette_match.rs`, `src/overlay_panel.rs`, `src/name_input.rs`, `src/switcher.rs`, `src/palette_files.rs`, `src/layout_model.rs` |
| 73 | `palette-run` | `palette.rs` | One (Mocha) | no | state | `src/palette.rs`, `src/palette_items.rs`, `src/palette_match.rs`, `src/overlay_panel.rs`, `src/name_input.rs`, `src/file_tab.rs`, `src/pane_actions.rs`, `src/layout_model.rs` |
| 74 | `palette-refresh` | `palette.rs` | One (Mocha) | no | state | `src/palette.rs`, `src/palette_items.rs`, `src/palette_match.rs`, `src/overlay_panel.rs`, `src/name_input.rs`, `src/switcher.rs`, `src/status.rs` |
| 75 | `palette-recent` | `palette.rs` | One (Mocha) | no | state | `src/palette.rs`, `src/palette_items.rs`, `src/palette_match.rs`, `src/overlay_panel.rs`, `src/name_input.rs`, `src/switcher.rs`, `src/palette_files.rs`, `src/pane_actions.rs` |
| 76 | `palette-files` | `palette.rs` | Both | no | visual | `src/palette.rs`, `src/palette_items.rs`, `src/palette_match.rs`, `src/overlay_panel.rs`, `src/name_input.rs`, `src/palette_files.rs`, `src/file_tab.rs`, `src/right_sidebar.rs`, `src/ssh_target.rs`, `src/layout_model.rs` |
| 77 | `palette-relist` | `palette.rs` | One (Mocha) | no | state | `src/palette.rs`, `src/palette_items.rs`, `src/palette_match.rs`, `src/overlay_panel.rs`, `src/name_input.rs`, `src/palette_files.rs` |
| 78 | `palette-settings` | `palette.rs` | Both | no | visual | `src/palette.rs`, `src/palette_items.rs`, `src/palette_match.rs`, `src/overlay_panel.rs`, `src/name_input.rs`, `src/preferences.rs` |
| 79 | `palette-narrow` | `palette.rs` | Both | no | visual | `src/palette.rs`, `src/palette_items.rs`, `src/palette_match.rs`, `src/overlay_panel.rs`, `src/name_input.rs`, `src/palette_files.rs` |
| 80 | `palette-motion` | `palette.rs` | One (Mocha) | yes: (c) nothing moves over 200 ms of real time. | visual | `src/palette.rs`, `src/palette_items.rs`, `src/palette_match.rs`, `src/overlay_panel.rs`, `src/name_input.rs`, `src/switcher.rs`, `src/mark_view.rs`, `src/status_mark.rs`, `src/reduce_motion.rs` |
| 81 | `branch-switcher` | `branch.rs` | Both | no | visual | `src/branch_switcher.rs`, `src/branch_list.rs`, `src/branch_git.rs`, `src/overlay_panel.rs`, `src/palette.rs`, `src/palette_items.rs` |
| 82 | `branch-switcher-confirm` | `branch.rs` | Both | no | visual | `src/branch_switcher.rs`, `src/branch_list.rs`, `src/branch_git.rs`, `src/overlay_panel.rs`, `src/procs.rs` |
| 83 | `branch-switcher-real` | `branch.rs` | One (Mocha) | no | state | `src/branch_switcher.rs`, `src/branch_git.rs`, `src/branch_list.rs`, `src/git_branch.rs` |
| 84 | `branch-switcher-open` | `branch.rs` | One (Mocha) | no | state | `src/branch_switcher.rs`, `src/palette.rs`, `src/palette_items.rs`, `src/sidebar.rs`, `src/status_bar.rs`, `src/git_branch.rs`, `src/procs.rs` |
| 85 | `welcome` | `welcome.rs` | Both | no | visual | `src/welcome.rs`, `src/welcome_items.rs`, `src/presets.rs`, `src/agent_detect.rs`, `src/overlay_panel.rs`, `src/help_shortcuts.rs` |
| 86 | `welcome-checking` | `welcome.rs` | One (Mocha) | yes: (c) nothing moves over 200 ms of real time. | visual | `src/welcome.rs`, `src/welcome_items.rs`, `src/presets.rs`, `src/agent_detect.rs`, `src/reduce_motion.rs` |
| 87 | `welcome-keys` | `welcome.rs` | One (Mocha) | no | state | `src/welcome.rs`, `src/welcome_items.rs`, `src/presets.rs`, `src/palette.rs`, `src/tab_bar.rs`, `src/layout_model.rs`, `src/name_input.rs`, `src/overlay_panel.rs` |
| 88 | `welcome-narrow` | `welcome.rs` | Both | no | visual | `src/welcome.rs`, `src/welcome_items.rs`, `src/presets.rs`, `src/overlay_panel.rs` |
| 89 | `welcome-start` | `welcome.rs` | One (Mocha) | no | state | `src/welcome.rs`, `src/welcome_items.rs`, `src/presets.rs`, `src/layout_model.rs`, `src/workspace_daemon.rs`, `src/file_tab.rs` |
| 90 | `welcome-folder` | `welcome.rs` | One (Mocha) | no | state | `src/welcome.rs`, `src/welcome_items.rs`, `src/presets.rs` |
| 91 | `welcome-first-run` | `welcome.rs` | One (Mocha) | no | state | `src/welcome.rs`, `src/welcome_items.rs`, `src/presets.rs` |
| 92 | `welcome-reopen` | `welcome.rs` | One (Mocha) | no | state | `src/welcome.rs`, `src/welcome_items.rs`, `src/presets.rs` |
| 93 | `overlays` | `overlays.rs` | One (Mocha) | no | state | `src/overlay_panel.rs`, `src/name_input.rs`, `src/palette.rs`, `src/branch_switcher.rs`, `src/help_panel.rs`, `src/host_picker.rs`, `src/workspace_browser.rs`, `src/welcome.rs`, `src/pane_tab_menu.rs`, `src/tab_menu.rs`, `src/pane_actions.rs`, `src/title_area.rs`, `src/preferences.rs` |
| 94 | `help` | `overlays.rs` | One (Mocha) | no | state | `src/help_panel.rs`, `src/help_shortcuts.rs`, `src/help_commands.rs`, `src/overlay_panel.rs`, `src/name_input.rs`, `src/palette.rs`, `src/sidebar.rs` |
| 95 | `panels` | `overlays.rs` | One (Mocha) | no | state | `src/host_picker.rs`, `src/ssh_hosts.rs`, `src/ssh_target.rs`, `src/workspace_browser.rs`, `src/overlay_panel.rs`, `src/name_input.rs` |
| 96 | `preferences` | `overlays.rs` | Both | no | visual | `src/preferences.rs`, `src/preferences_plugins.rs`, `src/plugins.rs`, `src/plugin_store.rs`, `src/plugin_source.rs`, `src/prefs.rs`, `src/name_input.rs` |
| 97 | `status-bar` | `status_bar.rs` | Both | no | visual | `src/status_bar.rs`, `src/status_mark.rs`, `src/sidebar.rs`, `src/git_branch.rs`, `src/procs.rs`, `src/status.rs`, `src/file_tab.rs`, `src/layout_model.rs`, `src/reduce_motion.rs` |
| 98 | `status-bar-hidden` | `status_bar.rs` | Both | no | visual | `src/status_bar.rs`, `src/panes.rs`, `src/git_branch.rs` |
| 99 | `status-bar-ports` | `status_bar.rs` | Both | no | visual | `src/status_bar.rs`, `src/procs.rs`, `src/browser_tab.rs`, `src/workspace_browser.rs`, `src/layout_model.rs`, `src/git_branch.rs` |
| 100 | `status-bar-narrow` | `status_bar.rs` | Both | no | visual | `src/status_bar.rs`, `src/updater.rs`, `src/procs.rs`, `src/git_branch.rs`, `src/sidebar.rs` |
| 101 | `update-button` | `status_bar.rs` | One (Mocha) | no | visual | `src/status_bar.rs`, `src/updater.rs`, `src/update_release.rs` |
| 102 | `cursor` | `terminal.rs` | Both | yes: (c) the glide (90 ms), sparks (350 ms) and the breath's 600 ms hold, read from Instant. | visual | `src/terminal_view.rs`, `src/terminal.rs`, `src/grid.rs`, `src/pty.rs`, `src/cursor_look.rs`, `src/appearance.rs`, `src/status.rs` |
| 103 | `cursor-flood` | `terminal.rs` | One (Mocha) | no | state | `src/terminal_view.rs`, `src/terminal.rs`, `src/grid.rs`, `src/pty.rs` |
| 104 | `command-blocks` | `terminal.rs` | Both | yes: (c) nothing animates over 200 ms of real time. | visual | `src/terminal_view.rs`, `src/terminal.rs`, `src/grid.rs`, `src/pty.rs`, `src/blocks.rs`, `src/block_view.rs`, `src/shell_marks.rs`, `src/reduce_motion.rs` |
| 105 | `command-blocks-alt-screen` | `terminal.rs` | Both | no | visual | `src/terminal_view.rs`, `src/terminal.rs`, `src/grid.rs`, `src/pty.rs`, `src/blocks.rs`, `src/block_view.rs`, `src/shell_marks.rs` |
| 106 | `command-blocks-narrow` | `terminal.rs` | Both | no | visual | `src/terminal_view.rs`, `src/terminal.rs`, `src/grid.rs`, `src/pty.rs`, `src/blocks.rs`, `src/block_view.rs`, `src/shell_marks.rs` |
| 107 | `command-blocks-rprompt` | `terminal.rs` | One (Mocha) | no | visual | `src/terminal_view.rs`, `src/terminal.rs`, `src/grid.rs`, `src/pty.rs`, `src/blocks.rs`, `src/block_view.rs`, `src/shell_marks.rs` |
| 108 | `command-blocks-small-font` | `terminal.rs` | Both | no | visual | `src/terminal_view.rs`, `src/terminal.rs`, `src/grid.rs`, `src/pty.rs`, `src/blocks.rs`, `src/block_view.rs`, `src/shell_marks.rs` |
| 109 | `command-blocks-off` | `terminal.rs` | Both | no | visual | `src/terminal_view.rs`, `src/terminal.rs`, `src/grid.rs`, `src/pty.rs`, `src/blocks.rs`, `src/block_view.rs`, `src/shell_marks.rs`, `src/appearance.rs` |
| 110 | `command-blocks-jump` | `terminal.rs` | Both | no | visual | `src/terminal_view.rs`, `src/terminal.rs`, `src/grid.rs`, `src/pty.rs`, `src/blocks.rs`, `src/block_view.rs`, `src/shell_marks.rs` |
| 111 | `command-blocks-copy` | `terminal.rs` | One (Mocha) | no | state | `src/terminal_view.rs`, `src/terminal.rs`, `src/grid.rs`, `src/pty.rs`, `src/blocks.rs`, `src/block_view.rs`, `src/shell_marks.rs`, `src/palette_items.rs` |
| 112 | `command-blocks-rerun` | `terminal.rs` | Both | no | visual | `src/terminal_view.rs`, `src/terminal.rs`, `src/grid.rs`, `src/pty.rs`, `src/blocks.rs`, `src/block_view.rs`, `src/shell_marks.rs` |
