# Milestone 11aj — Faster UI Shots

**Status:** 📝 design written 2026-11-16, waiting for your approval; then a plan.

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
