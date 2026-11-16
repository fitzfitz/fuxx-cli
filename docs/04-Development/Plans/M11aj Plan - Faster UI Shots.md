---
tags: [development, plan, milestone]
milestone: M11aj
status: planned
updated: 2026-11-16
---

# M11aj Plan — Faster UI shots

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

| At a glance | |
|---|---|
| **Design (the spec)** | [[81 - Milestone 11aj - Faster UI Shots]] |
| **Status** | 📋 Planned 2026-11-16; starts after M11ae merges |
| **Tasks done** | 0 / 11 |
| **▶ Pick up here** | Task 1 |
| **Decisions used** | Yours: "all three, your order is fine", "approved". The calls listed in note 81 ("Calls made after the builder check"). |
| **Needs your eyes** | 👀 Task 1's numbers if a goal looks out of reach; the on-screen check in note 81 after Task 11 |

**Goal:** The screenshot suite runs a task's own scenes in ≤ 30 s and the whole suite in ≤ 90 s (goals, measured),
split into area files of ≤ ~3,000 lines, with every existing check kept and state-only checks moving to `cargo test`.

**Architecture:** Measure first (Task 1). Then move the harness out of the 22k-line file without changing behaviour
(Task 2), add a declared scene registry and run options with PNGs on demand (Task 3), split the scenes into area files
while tagging each (Tasks 4–6), then cut per-shot setup (Task 7), run in parallel processes (Task 8), run what a change
touches (Task 9), move state-only checks (Task 10), and trial reference fingerprints (Task 11, droppable). Scene
functions keep their `fn()` shape and names; the theme choice reaches them through `shot_themes()`.

**Tech Stack:** Rust, GPUI pinned rev `1d2a4b3f…` (`VisualTestAppContext`, `open_offscreen_window`,
`capture_screenshot`, `advance_clock`), bash for `scripts/ui-shots.sh`. **No new crates.**

**Spec:** [[81 - Milestone 11aj - Faster UI Shots]]

## Global Constraints

- Branch `m11aj-ui-shots` in worktree `.worktrees/m11aj-ui-shots`, from master **after M11ae merges**. Commit per task;
  merging is the user's call.
- **Gate per task:** `cargo test --bin fuxx && cargo clippy --all-targets && cargo clippy --all-targets --features
  ui-shots && cargo fmt --check`, then a full suite run (until Task 8, `cargo run --features ui-shots -- __ui-shots`;
  after, `scripts/ui-shots.sh`). No new clippy warnings over the baselines Task 1 records.
- **Nothing lost:** every scene name in today's `SCENES` (105) still exists and runs (or its checks moved to
  `cargo test` with a one-line pointer left in the area file); the printed `ui-shots: <name> ok` set at the end of each
  task equals the set before it, plus any added.
- No harness or scene file over about **3,000 lines** at the end (Task 6 checks).
- AppKit: parallelism is processes, never threads. GPUI APIs are read from the pinned source, never guessed.
- App modules' `#[cfg(feature = "ui-shots")]` hooks are not moved or widened.
- Scenes that re-run fuxx as a child (`FUXX_CHROME_CACHE=off`, `FUXX_SHOTS_WEBKIT`) keep that pattern, are `serial`,
  and Mermaid's WebKit child keeps running first and alone.
- Output wording: `ui-shots: <name> ok (<s> s)`, `ui-shots: <name> FAILED (<s> s) — pictures in <dir>`, and a final
  summary `ui-shots: <passed> passed, <failed> failed, <total> s` followed by the 15 slowest.
- Measurements: on this Mac, nothing else heavy running, results into note 81.

## Review Focus

1. **A scene that fails mid-run:** the run continues, the summary says FAILED with its pictures' folder, the exit code
   is 1 (Task 3's runner test with a deliberately failing scene behind an env var).
2. **Shared state between shots after A1 reuse:** globals set by one shot (`Appearance`, settings) leaking into the
   next shot of the same scene (Task 7 compares every shot's pixels before and after reuse on a sample of 10 scenes).
3. **Parallel flakes:** five consecutive full sharded runs pass before the shard default is set (Task 8).
4. **A change no scene claims:** `--changed` runs everything and prints the file (Task 9 unit test).
5. **`--png` matches today's pictures:** with `--png --all-themes`, the PNG count equals today's (449 plus scenes added
   since) (Task 3 and Task 6).

---

### Task 1: Measure (Part 0)

**Files:** Modify `src/ui_shots.rs` (`run` at `:345`, `render_with` at `:80`); note 81.

**Interfaces — produces:** per-scene timing lines; `target/ui-shots/timings.tsv` (`<scene>\t<seconds>` per line);
`UI_SHOTS_PROFILE=1` prints a per-shot breakdown: `new_app`, `fonts`, `open`, `settle`, `act`, `capture`, `png`.

- [ ] Record clippy baselines (both counts) in the report.
- [ ] Add the timing lines, the summary (wording above) and `timings.tsv`; add the profile breakdown.
- [ ] Run the full suite twice (cold, warm) and once with `UI_SHOTS_PROFILE=1`; record total, the 15 slowest scenes,
  the summed breakdown per phase, the time spent in `thread::sleep`, and the child-process scenes' time.
- [ ] Write the numbers into note 81 ("Part 0 — measured") with the expected gain of each Part A item. **If a goal
  (90 s full, 30 s per task) looks out of reach, stop and report DONE_WITH_CONCERNS with the expected numbers** — the
  controller tells the user before Task 7.
- [ ] Gate; commit `M11aj Task 1: measure the suite`.

### Task 2: Move the harness out (no behaviour change)

**Files:** `src/ui_shots.rs` → `src/ui_shots/mod.rs` (runner, `SCENES`, scenes for now); create
`src/ui_shots/harness.rs` (`render`, `render_with`, `shoot`, `shoot_with`, `Shot`, `png`, `shot_theme`, pixel helpers
such as `pixel_at`, `gap`, `diff_count`); move `Steps` and its helpers from `chrome_sources.rs` to harness.

**Interfaces — produces:** `pub(super) use harness::*` so scene code compiles unchanged.

- [ ] Move with `git mv` first (history), then cut the harness out; no logic edits.
- [ ] Gate; the printed scene set and the PNG file list in `target/ui-shots/` equal Task 1's. Commit
  `M11aj Task 2: harness in its own file`.

### Task 3: Scene registry, run options, PNGs on demand, failure pictures

**Files:** `src/ui_shots/mod.rs`, `src/ui_shots/harness.rs`, `src/main.rs:853-854` (pass all args).

**Interfaces — produces:**
- `pub(super) struct Scene { name: &'static str, run: fn(), themes: Themes, serial: bool, kind: Kind, sources: &'static [&'static str] }`;
  `enum Themes { One, Both }`; `enum Kind { Visual, State }`. Until Tasks 4–6 every row is `Both`, `serial: false`,
  `Visual`, `sources: &[]`.
- `struct RunOptions { filter: Option<String>, png: bool, all_themes: bool, shard: Option<(usize, usize)>, changed: Option<String>, update_refs: bool }`
  parsed by `pub fn parse_args(args: &[String]) -> Result<RunOptions, String>`; held in a `OnceLock`.
- `pub(super) fn shot_themes() -> &'static [(&'static str, &'static str)]` — the current scene's themes (`One` →
  Mocha only) unless `--all-themes`; every `SHOT_THEMES` use in scene code becomes `shot_themes()` (mechanical).
- `shoot`/`shoot_with` keep the shot in a per-scene in-memory list and write the PNG only with `--png` /
  `UI_SHOTS_PNG=1`; `run` wraps each scene in `catch_unwind`; on failure writes the scene's shots to
  `target/ui-shots/failed/<scene>/`, prints the FAILED line, continues; exit 1 if any failed.
- `smoke`'s file-header checks run on the in-memory PNG bytes (`png(&shot)`), not the file.

- [ ] Tests (`cargo test --bin fuxx --features ui-shots ui_shots::` — confirm this command runs pure tests without
  opening windows; if it can't, put the pure functions in `src/shot_args.rs` outside the feature and say so):
  `parse_args(["__ui-shots"])` defaults; `["file-tab","--png"]`; `["--shard","3/8"]` → `Some((3,8))`;
  `["--shard","9/8"]` → `Err`; `["--changed","master","--all-themes"]`.
- [ ] Runner check: `UI_SHOTS_FAIL_DEMO=1` makes `smoke` fail on purpose → FAILED line, `failed/smoke/` has its PNGs,
  other scenes still run, exit 1.
- [ ] Gate; with `--png --all-themes` the PNG list equals Task 1's; without `--png` no PNG is written. Commit
  `M11aj Task 3: scene registry, run options, PNGs on demand`.

### Tasks 4, 5, 6: Split the scenes by area and tag each

One task per batch; each moves its scenes' functions (and their private helpers) into area files under
`src/ui_shots/`, declares their `Scene` rows there (`pub(super) const SCENES: &[Scene]`), and `mod.rs` concatenates the
area lists **in today's order**. While moving, each scene is tagged from reading it:
- `themes: Both` if any check reads a theme colour, contrast or a theme-specific look; else `One`;
- `serial: true` if it sleeps on wall-clock time, measures a real animation, spawns a child fuxx, or must run alone;
- `kind: State` if every check reads view state (bounds, scroll, words, counts), never pixels;
- `sources`: the app files the scene's views come from (`src/<module>.rs`, …).

A table of every scene → themes / serial / kind / sources goes in the task report (it becomes note 81's appendix in
Task 11).

- **Task 4 — window and chrome:** `smoke`, `tokens`, `window`, `marks`, `sidebar-marks`, `cached-panes`, all
  `chrome-*` (into `chrome.rs`, absorbing `chrome_sources.rs`), `sidebar*`, `title-area`, `title-bar`, `tabs`,
  `splits`, `cards*`, `workspaces`, `tab-bars`, `tab-drag`, `browser-tab`. Commit `M11aj Task 4: window, chrome,
  sidebar and pane scenes in area files`.
- **Task 5 — files and git:** `file-tab`, `file-diff`, `file-tab-md`, `file-tab-md-mermaid`, `file-tab-scrollbar` and
  M11ae's other file-tab scenes, `right-sidebar`, `files`, `files-follow`, `right-panels`, `changes*`, `activity`,
  `branch-switcher*`. Commit `M11aj Task 5: file and git scenes in area files`.
- **Task 6 — everything else:** `api-background`, `banner-click`, `default-name`, `names`, `renames`, `rename-box`,
  `palette*`, `welcome*`, `overlays`, `help`, `panels`, `preferences`, `status-bar*`, `update-button`, `cursor*`,
  `command-blocks*`, plus any scene not yet moved. `mod.rs` keeps only the runner and registry. Check: no file over
  ~3,000 lines (`wc -l src/ui_shots/*.rs`). Commit `M11aj Task 6: remaining scenes in area files`.

Each of 4–6: gate; the printed scene set equals before; `--png --all-themes` PNG list equals before.

### Task 7: Less setup per shot (A1)

**Files:** `src/ui_shots/harness.rs`, `src/ui_shots/mod.rs`.

- [ ] From Task 1's breakdown, apply what pays: fonts registered once per process if the platform's text system allows;
  one `VisualTestAppContext` per scene run (its shots each in a new off-screen window, closed after capture) if
  `new_app` dominates. A scene's run starts from a fresh app. Mermaid's kept-alive contexts stay as they are.
- [ ] Leak check: for 10 scenes (5 `Both`, 5 with several shots), the in-memory shots before and after the change are
  pixel-identical (`diff_count == 0`); record which 10.
- [ ] Gate; record the new total and per-phase sums in note 81. Commit `M11aj Task 7: less setup per shot`.

### Task 8: Parallel shards, the script, fewer real-time waits (A4, A5)

**Files:** `src/ui_shots/mod.rs`; create `scripts/ui-shots.sh`; the area files with `thread::sleep`.

**Interfaces — produces:** `pub fn shard_of<'a>(timings: &[(&str, f64)], names: &[&'a str], n: usize) -> Vec<Vec<&'a str>>`
(longest first onto the least-loaded shard; scenes without a timing get the median; `serial` scenes never sharded);
`scripts/ui-shots.sh [filter] [--png] [--all-themes] [--changed <base>] [-j N]`.

- [ ] Tests: `shard_of` balances `[10,9,2,2,1]` over 2 into loads `{12, 12}` or `{13, 11}`; every name lands exactly
  once; an untimed name gets the median; `n = 1` → one shard in order.
- [ ] The script: `cargo build --features ui-shots` once; start `-j` shards (default `min(8, cores − 2)`, at least 1)
  of the built binary with `--shard i/N`, prefix lines `[i/N]`; then the `serial` scenes in one process; merge
  `timings.tsv`; print the summary (Global Constraints wording); exit 1 if any shard failed. bash 3.2-safe.
- [ ] A5: replace `thread::sleep`s on GPUI timers with `advance_clock` / `run_until_parked`; the rest stay and their
  scenes are `serial` (list them in note 81).
- [ ] Flake check: five consecutive full `scripts/ui-shots.sh` runs pass; if not, lower the default `-j` or mark more
  scenes `serial` until they do; record the default and the five totals.
- [ ] Gate; commit `M11aj Task 8: parallel shards and the ui-shots script`.

### Task 9: Run what a change touches (B3, B4)

**Files:** `src/ui_shots/mod.rs`; `docs/05-Agentic-Workflow/50 - Agentic Workflow.md`; the plan template the vault
uses (`docs/05-Agentic-Workflow/53 - Task Handoff Template.md`).

**Interfaces — produces:** `pub fn affected(changed: &[&str], scenes: &[Scene], shared: &[&str]) -> Affected` with
`enum Affected { All { unclaimed: Vec<String> }, Some(Vec<&'static str>), Nothing }`; `SHARED: &[&str]` in `mod.rs`
(`src/ui_shots/harness.rs`, `src/ui_shots/mod.rs`, `src/theme.rs`, `src/ui_tokens.rs`, `src/workspace.rs`,
`src/appearance.rs`, `Cargo.toml`, `Cargo.lock`).

- [ ] Tests: a change to `src/file_tab.rs` → only scenes with it in `sources`; to `src/theme.rs` → `All`; to an
  unclaimed `src/foo.rs` → `All { unclaimed: ["src/foo.rs"] }`; to `docs/x.md` only → `Nothing`; an `assets/` change →
  the scenes claiming it, else `All`.
- [ ] `--changed <base>` runs `git diff --name-only <base>...HEAD` plus uncommitted changes, prints what it chose and
  why, and shards as usual.
- [ ] Docs: the per-task gate is `scripts/ui-shots.sh --changed <task base>` (with `--png` when looking); once per
  milestone the full `scripts/ui-shots.sh --all-themes`. C1's rule (window-free logic → `cargo test`) goes in the same
  note.
- [ ] Gate; commit `M11aj Task 9: run the scenes a change touches`.

### Task 10: Move state-only checks to `cargo test` (C2)

**Files:** the area files tagged `kind: State`; the app modules whose logic is lifted.

- [ ] For each `State` scene: if its checks can run on pure functions lifted from the view **without changing app
  behaviour** (moving code into a function and calling it from the view is fine; new behaviour is not), move the checks
  to `cargo test` next to the function, delete the scene, and leave a one-line pointer in the area file. Otherwise list
  it ("stays: needs <reason>").
- [ ] Gate; the list of moved and stayed scenes goes in the report. Commit `M11aj Task 10: state-only checks to cargo
  test`.

### Task 11: Reference fingerprints (C3, trial, droppable) and docs

**Files:** `src/ui_shots/harness.rs`, `tests/ui-shots-ref/` (new), note 81, this plan, Roadmap, Progress Log.

**Interfaces — produces:** `pub fn fingerprint(shot: &Shot) -> Vec<u8>` (greyscale, downscaled to 1/8 of the shot's
point size, one byte per cell) and `pub fn distance(a: &[u8], b: &[u8]) -> f32` (mean absolute difference, 0–255;
`f32::INFINITY` for a size mismatch).

- [ ] Tests: identical → 0; one cell changed by 80 in 1,000 → 0.08; different sizes → infinity.
- [ ] For `Visual` scenes, compare each shot's fingerprint with `tests/ui-shots-ref/<scene>-<slug>.fp`; print
  `ui-shots: <scene>-<slug> looks different (<d>)` above a tolerance chosen from five runs of unchanged code (record
  it); **never fail the run**. `--update-refs` writes them. Commit the refs only if their total is under 5 MB; else
  report and drop C3 to a follow-up (the controller asks the user).
- [ ] Full `scripts/ui-shots.sh --all-themes`; record the final numbers (full run, a `--changed` run on a one-line
  `file_tab.rs` change, the build time) in note 81 with the scene table appendix; note 81 status 🟨 built; this plan
  11 / 11; Roadmap; a Progress Log entry. Commit `M11aj: results and docs`.
