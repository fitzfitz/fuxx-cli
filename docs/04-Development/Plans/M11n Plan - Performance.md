---
tags: [development, plan, milestone]
milestone: M11n
status: planned
updated: 2026-11-09
---

# M11n Plan — Performance

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

| At a glance | |
|---|---|
| **Design (the spec)** | [[65 - Milestone 11n - Performance]] |
| **Status** | 🟨 Designed and planned 2026-11-09, not built |
| **Tasks done** | 0 / 9 |
| **▶ Pick up here** | Task 0 (the benchmark harness in the repo, and the baseline on this branch before any change). |
| **Decisions used** | Yours (2026-11-09): a performance milestone after M11l (now M11n, after the M11m title bar); "push the release after performance milestone" (the one combined release v0.5.0, M11i–M11n, is this plan's Task 7). Everything else: AI's calls in note 65. |
| **Needs your eyes** | 👀 the on-screen check in note 65, after the release |

**Goal:** Cut fuxx's idle CPU with the breathing glow on, and drain output faster, without changing what you see:
the breath keeps its curve and never takes a bigger step than today, every byte still reaches the screen, the
scanners, the agent-state detection and the command blocks.

**Architecture:** Two independent halves. **Output** (Tasks 1–2): the byte scanners jump over plain text with
`memchr`; the PTY reader thread only reads, a new parser thread takes everything read so far (up to 64 KiB) and
parses it under one lock, and a gate sends the UI one `Output` per batch only when it has seen the last one.
**Idle** (Tasks 3–4): `Theme::ui()` is computed once per theme instead of on every render; the breath is drawn at the moments its opacity
crosses the next step instead of every 50 ms, sleeps through the 0.6 s hold, and rests at full strength after five
minutes untouched. Task 0 brings the 2026-11-09 benchmark harness into the repo and measures the baseline; Task 5
measures the result against the targets.

**Tech Stack:** Rust, GPUI at the pinned rev (`1d2a4b3f…`; no new GPUI API: `cx.notify`, `background_executor().timer`
as today), `alacritty_terminal` 0.26 (unchanged), **`memchr` 2.8** (new direct dependency, already in `Cargo.lock`
at 2.8.3 via GPUI's tree; API checked in Context7 `/burntsushi/memchr`: `memchr(n, h) -> Option<usize>`,
`memchr2(n1, n2, h) -> Option<usize>`). std threads, `Mutex` and `Condvar` for the reader/parser hand-off.

**Spec:** [[65 - Milestone 11n - Performance]]

## Global Constraints

- **No behaviour change you could notice**, except the breath's two stated changes (note 65 §4): it is drawn at
  level crossings (same curve, same 1.0 and 0.35 extremes, no step larger than `BREATH_STEP` = 0.064, today's largest)
  and it rests at full strength after `BREATH_REST_AT` (188 breaths, 300.8 s) untouched. Same colours, blur (6 px),
  glow alpha (0.30), period (1.6 s), hold (0.6 s), glide, trail, sparks, Reduce motion and Animations rules.
- **Every byte is processed, in order.** Nothing is dropped, merged away or reordered on its way to the `Term`, the
  OSC 9/99/777 detector, the OSC 7 reader, the OSC 133 mark scanner and tagger, and the clear counter. Backpressure,
  not loss: the reader waits when `INBOX_CAP` (1 MiB) is queued.
- **Order of events is kept:** every `Term` event raised while parsing a batch (query replies, title, bell) is sent
  before that batch's `Output`; `Exited` comes after the last batch's `Output`.
- **An `Output` that carries notifications is never merged** with another (`notifications > 0` always sends its own
  event with its own count).
- **The UI's per-frame lock is never held up by more than one batch:** a batch is at most `BATCH_MAX` = 64 KiB.
- **Idle memory unchanged:** a drained terminal keeps at most `IDLE_KEEP` = 64 KiB of buffer capacity.
- **No new settings, keys, menu items, palette rows or Help rows. No GPUI fork, no change to `alacritty_terminal`.**
  `memchr` is the only new dependency.
- The background service (persistent sessions) shares `pty::spawn_reader` and the scanners, so it gets Tasks 1–2
  as they are; its one frame per batch stays far under `MAX_FRAME` (16 MiB).
- Subagents never `cargo run` the app. Allowed: `cargo test`, `cargo clippy --all-targets`,
  `cargo clippy --all-targets --features ui-shots`, `cargo fmt --check`,
  `cargo run --features ui-shots -- __ui-shots [filter]`. **Exception:** Tasks 0 and 5 run the benchmark harness,
  which starts its own isolated copies of fuxx and Ghostty (scratch `HOME`, its own bundle ids) and quits them; it
  never touches the user's HOME, config, sessions or running apps.
- Use a private build directory (`CARGO_TARGET_DIR`) for the whole branch, as M11m did.

## Review Focus

1. **Lost output under a flood.** A batch split, a full inbox or the reader/parser hand-off drops or repeats bytes.
   Expected: `seq 1 300000` ends with `299999`, `300000`, `END` on screen in order. (Task 2:
   `every_byte_of_a_flood_reaches_the_screen`; `the_inbox_keeps_order_and_blocks_when_full`.)
2. **A missed agent state or notification.** An OSC 9/99/777 split across reads or batches is not counted, or its
   `Output` is merged into a plain one, so a working agent never turns Done or Waiting. Expected: the same counts as
   feeding byte by byte; the tracker still hears every batch. (Task 1: `fast_scanners_match_bytewise_at_every_split`;
   Task 2: `a_notification_batch_always_sends_its_own_output`, `an_agent_still_settles_after_a_batched_flood`.)
3. **A stale frame after output stops.** The gate swallows the last batch's `Output` because one was still pending,
   so the screen shows an older frame until the next keystroke. Expected: after the UI acknowledges an `Output`, the
   next batch always sends one; the screen at the last event holds the last line. (Task 2:
   `the_batch_after_an_acknowledged_output_always_sends`, and the `END` check above.)
4. **The glow stuck or not resuming.** The level-crossing schedule stops after the hold, after a focus change, or
   after the rest; or the rest begins mid-fade. Expected: breathing resumes 0.6 s after any output or keystroke and
   on refocus, solid at first; the rest begins exactly at a peak (alpha 1.0). (Task 4: `breath_frames_are_level_crossings`,
   `the_rest_starts_at_a_peak_and_any_change_ends_it`; ui-shots `cursor-breath-resumes`.)
5. **A block mark missed.** An OSC 133 mark that straddles two reads, two batches or the memchr jump is lost, so a
   command has no bar. Expected: the same marks, at the same byte positions, as one whole feed. (Task 1:
   `fast_scanners_match_bytewise_at_every_split` covers `MarkScanner`; Task 2: `marks_survive_batch_boundaries`.)

---

### Task 0: The benchmark harness in the repo, and the baseline

**Files:** Create `scripts/bench/run.py` (the 2026-11-09 harness, from the session scratchpad's `bench/scripts/run.py`,
with the fixes below), `scripts/bench/wininfo.swift` (as it was), `scripts/bench/make_data.py`.

**Interfaces — Produces:** `python3 -I scripts/bench/run.py --bench DIR bundle NAME BINARY` (a single-arch
`NAME.app` in `DIR/apps`, bundle id `io.github.fitzfitz.fuxx.bench-NAME`, ad-hoc signed);
`python3 -I scripts/bench/run.py --bench DIR APP PHASE` with `APP` ∈ {`fuxx-base`, `fuxx-m11n`, `ghostty`} and
`PHASE` ∈ {`launch`, `mem`, `tput`, `cpu_base`, `cpu_noglow`, `cpu_rest`, `grid`}; `python3 -I scripts/bench/run.py --bench DIR
summary` prints the medians as in the 2026-11-09 `summary.txt`. Results: `DIR/results/<APP>-<PHASE>.json`.

- [ ] **Step 1: port the harness**, keeping every method of note 65 §Baseline, with these fixes:
  - `sessions.json` is **seeded with `{"version":2}`** instead of deleted (since M11k a missing file shows the
    Welcome card, which takes the keyboard from the terminal, so the glow never runs and `cpu_base` reads ~1 %).
  - The zsh rc starts with **`zmodload zsh/datetime`** (else `$EPOCHREALTIME` is empty and `tput` reads 0).
  - Every `cpu_*` phase checks the frontmost app every 2 s over its 30 s window (`wininfo`) and records
    `frontmost: k/15`; `summary` counts **only runs with 15/15** and says how many it dropped.
  - `HOME` is a short symlink `/private/tmp/claude-502/fxb-<APP>` → `DIR/homes/<APP>` (fuxx's socket path limit);
    the config for `cpu_noglow` is `cursor-glow = off`; Ghostty's app is a `ditto` copy of
    `~/Applications/Ghostty.app` as `DIR/apps/Ghostty-bench.app`, **not re-signed** (a re-signed copy can't load
    Sparkle). The harness signals only PIDs it started and their descendants, as before.
  - **New phase `cpu_rest`:** as `cpu_base`, but the 30 s window starts 305 s after ready (past the breath's
    300.8 s rest), frontmost checked over the whole 335 s.
  - `make_data.py DIR` writes `DIR/data/big.txt` (100 MiB of plain ASCII lines of 3–18 words, seed 7) and
    `DIR/data/colors.txt` (200,000 lines with SGR 30–37, bold, 256-colour runs, seed 7), deterministic.
- [ ] **Step 2: baseline**, before any code change on this branch (= master `f661d5e`): `cargo build --release`,
  `run.py bundle fuxx-base <target>/release/fuxx`, then `run.py fuxx-base` and `run.py ghostty` for `cpu_base` ×3,
  `cpu_noglow` ×2 and `cpu_rest` ×1 (fuxx only), `tput`, `launch`, `mem`. Keep the results directory for Task 5 (same machine, same
  data). Write the medians into note 65 §Baseline as "re-measured (Task 0)".
- [ ] **Step 3: commit** `chore(m11n): the benchmark harness in scripts/bench (seeded Welcome, datetime, frontmost check)`.

### Task 1: The byte scanners jump over plain text

**Files:** Modify `Cargo.toml` (`memchr = "2.8"` with the "already in Cargo.lock" comment), `src/detector.rs`
(`OscDetector::feed`, `CwdReports::feed`), `src/shell_marks.rs` (`MarkScanner::feed`), `src/terminal.rs`
(`ClearCounter::feed`).

**Interfaces:** signatures unchanged. Each scanner gains a private `fn at_rest(&self) -> bool` (no intro partly
matched, no open body, no pending ESC or OSC 9 look-ahead). **Rule:** while `at_rest`, jump with `memchr(0x1b, rest)`
to the next ESC (none: done with the chunk); inside an open OSC body, copy up to the next `memchr2(0x07, 0x1b, rest)`
in one `extend_from_slice` (respecting `CWD_LIMIT` and the mark body's overflow rule) and handle the terminator
byte by byte as today. `MarkScanner::feed` keeps returning marks with their byte offsets in the chunk.

- [ ] **Step 1: failing test** `fast_scanners_match_bytewise_at_every_split` (one per scanner file, sharing a
  `#[cfg(test)]` stream builder): a splitmix-seeded generator (seeds 1..=200) builds streams of plain text mixed
  with `ESC]9;msg BEL`, `ESC]9;4;1;50 BEL` (a progress bar, not a notification), `ESC]99;…ESC\`, `ESC]777;notify;…BEL`,
  `ESC]7;file://h/p BEL`, `ESC]133;A/B/C/D;0 BEL`, `ESC[2J`, `ESC[3J`, `ESC c`, stray ESC, ESC ESC, CAN, SUB and a
  4,097-byte OSC 7; each is fed (a) one byte at a time to the **unchanged bytewise implementation kept as the
  oracle** (`#[cfg(test)] mod bytewise`, a verbatim copy of today's `feed` bodies) and (b) in random chunks of
  1..=4096 bytes to the new `feed`. Assert equal notification counts, equal last `CwdReport`, equal mark list
  (kind, payload and absolute byte offset), equal clear count. Plus, per scanner, the exhaustive check: one stream
  holding every sequence above, split at **every** offset into two chunks.
- [ ] **Step 2: run** `cargo test detector shell_marks terminal::` → the oracle copy compiles, the test passes
  trivially only if `feed` is unchanged; make it fail first by asserting `at_rest` exists (`let _ = s.at_rest();`).
- [ ] **Step 3: implement** per Interfaces.
- [ ] **Step 4: run** `cargo test`, both clippy runs, `cargo fmt --check` → green.
- [ ] **Step 5: commit** `perf(m11n): the byte scanners jump over plain text with memchr`.

### Task 2: A parser thread, 64 KiB batches, one Output per batch

**Files:** Modify `src/pty.rs` (`spawn_reader`, new `Inbox`), `src/terminal.rs` (`OutputGate`, the local reader
closure, `TerminalCore::output_seen`), `src/terminal_view.rs` (`on_term_event` acknowledges an `Output`),
`src/daemon_session.rs` (no code change expected; its test suite must stay green).

**Interfaces — Produces:**
- `pty.rs`: `pub const BATCH_MAX: usize = 64 * 1024; pub const INBOX_CAP: usize = 1 << 20; pub const IDLE_KEEP:
  usize = 64 * 1024;` `struct Inbox` with `fn push(&self, bytes: &[u8]) -> bool` (appends; waits while ≥
  `INBOX_CAP` is queued; `false` once the parser side is gone), `fn close(&self)` (EOF), `fn take(&self, into: &mut
  Vec<u8>) -> bool` (waits for bytes or EOF; moves up to `BATCH_MAX` bytes into `into`; `false` = EOF and nothing
  left). `spawn_reader(reader, on_output, on_eof)` keeps its signature: thread `fuxx-pty-reader` reads (8 KiB
  buffer) and pushes; thread `fuxx-pty-parser` loops `take` → `on_output(batch)`, then `on_eof()` after the last
  batch. Buffers shrink to `IDLE_KEEP` when drained.
- `terminal.rs`: `pub(crate) struct OutputGate { pending: AtomicBool }` with `fn after_batch(&self, notifications:
  usize) -> Option<TermEvent>` (notifications > 0 → always `Some(Output { notifications })`; else `Some(Output { 0 })`
  only if no plain `Output` is pending, and marks it pending) and `fn seen(&self)` (clears pending).
  `TerminalCore::output_seen(&self)` calls it (no-op for a daemon replica). The local closure: scanners → one lock →
  `blocks::advance` + `clears.feed` → unlock → `gate.after_batch(n)`.
- `terminal_view.rs`: `TermEvent::Output` handling begins with `self.core.output_seen()`.

- [ ] **Step 1: failing tests**
  - (pty) `the_inbox_keeps_order_and_blocks_when_full`: one thread pushes 3 MiB in random-sized pieces (seeded)
    while another takes with random pauses; the concatenation equals the input; with no taker, a push past
    `INBOX_CAP` is still waiting after 200 ms and returns once 64 KiB is taken; every `take` moves ≤ `BATCH_MAX`.
  - (pty) `eof_comes_after_the_last_batch`: push, close; `take` returns the bytes, then `false`.
  - (terminal) `a_notification_batch_always_sends_its_own_output`: `after_batch(0)` → `Output{0}`;
    `after_batch(0)` → `None`; `after_batch(2)` → `Output{2}`; the sum of delivered notifications over a seeded
    sequence of batches equals the sum fed.
  - (terminal) `the_batch_after_an_acknowledged_output_always_sends`: `after_batch(0)` → Some; `seen()`;
    `after_batch(0)` → Some.
  - (terminal, real PTY, the existing `drain` helpers) `every_byte_of_a_flood_reaches_the_screen`: spawn `/bin/sh -c
    'seq 1 300000; printf "\033]9;done\007"; echo END'`, never call `output_seen`; wait for `Exited`. Assert:
    `tail_text(3)` is `299999`/`300000`/`END` in order; the events are exactly one `Output { notifications: 0 }`,
    one `Output { notifications: 1 }` and `Exited`, last (**the deterministic counter: one plain `Output`
    for a ~2 MB flood the UI hasn't acknowledged**, where today there is one per ~1 KiB read).
  - (terminal) `marks_survive_batch_boundaries`: a zsh-integration stream (three prompts and commands, OSC 133
    A/B/C/D) fed through the local path in `BATCH_MAX`-sized pieces whose boundaries land inside each mark → the
    same `blocks()` as one `show_local` of the whole stream.
  - (terminal_view `status` path, pure) `an_agent_still_settles_after_a_batched_flood`: a `StatusTracker` fed one
    `on_output` per gate event of the flood test above (typed first, outside the echo window) is Working, and a
    `tick` 3 s after the last event with a prompt tail turns it Done; with an `Output { notifications: 1 }` it settles
    on the next tick, as today.
- [ ] **Step 2: run** `cargo test pty terminal::` → FAIL (undefined `Inbox`, `OutputGate`, `output_seen`).
- [ ] **Step 3: implement** per Interfaces. `Inbox` = `Mutex<(Vec<u8>, usize /*read pos*/, bool /*closed*/,
  bool /*parser gone*/)>` + two `Condvar`s (bytes, room). The parser thread owns the `on_output` closure, so the
  scanners, `Term` and event channel see the batches in read order.
- [ ] **Step 4: run** `cargo test` (including `daemon_session` and `daemon_client`), both clippy runs,
  `cargo fmt --check`, then `cargo run --features ui-shots -- __ui-shots cursor` and `… command-blocks` → green.
- [ ] **Step 5: commit** `perf(m11n): a parser thread takes the PTY's reads in 64 KiB batches; one Output per batch`.

### Task 3: Cheaper frames — `Theme::ui()` once per theme

**Files:** Modify `src/theme.rs` (`ui`).

**Interfaces — Produces:** `pub fn ui(&self) -> UiColors` unchanged for its 100+ callers; it returns a thread-local
copy computed for the last theme it saw (`thread_local! { static UI: Cell<Option<(Theme, UiColors)>> }`; both types
are `Copy + PartialEq`), recomputing only when the theme differs. `#[cfg(test)] fn ui_computations() -> usize`.

- [ ] **Step 1: failing test** `ui_is_computed_once_per_theme`: `t.ui()` twice → one computation, equal results;
  `t2.ui()` → two; `t.ui()` again → three, equal to the first; every built-in theme's `ui()` equals a fresh uncached
  computation (`ui_uncached`, the old body renamed).
- [ ] **Step 2: run** `cargo test theme` → FAIL (`ui_computations` undefined).
- [ ] **Step 3: implement** per Interfaces.
- [ ] **Step 4: run** `cargo test`, both clippy runs, `cargo fmt --check`, the full `__ui-shots` → green (the
  ui-shots compare colours and would catch a wrong cached `UiColors`).
- [ ] **Step 5: commit** `perf(m11n): Theme::ui is computed once per theme, not on every render`.

### Task 4: The breath — drawn at level crossings, asleep through the hold, at rest after five minutes

**Files:** Modify `src/cursor_look.rs` (schedule, rest), `src/terminal_view.rs` (`breathe`, `CursorSettings::look`),
`src/ui_shots.rs` (cursor scenes).

**Interfaces — Produces** (`cursor_look.rs`):
- `pub const BREATH_STEP: f32 = 0.064;` (today's largest step: 20 fps at the cosine's steepest point)
- `pub const BREATH_MIN: Duration = Duration::from_millis(50);` (replaces `BREATH_TICK`; the floor between frames)
- `pub const BREATH_REST_AT: Duration = Duration::from_millis(300_800);` (188 breaths of 1.6 s, measured on the
  breath clock, i.e. after the hold)
- `pub fn breath_wait(clock: Duration) -> Duration` — the time from `clock` to the next frame: the earliest later
  clock at which `breath_alpha` reaches the next level `1.0 − k·BREATH_STEP` (k = 1..=10) in its direction of travel,
  or the trough (0.8 s into a breath, alpha 0.35) or the peak (1.6 s, alpha 1.0), whichever is first, but at least
  `BREATH_MIN`. Closed form: `clock(α) = P/(2π)·acos((α − mid)/amp)` on the falling half, `P − …` on the rising half.
- `pub fn breath_rests(clock: Duration) -> bool` = `clock >= BREATH_REST_AT`; `breath_alpha` returns 1.0 there.
- `needs_frames(focused, key_window, motion, glowing, animating)` unchanged; the view passes `glowing = … &&
  !breath_rests(clock)`.
- `terminal_view.rs` `breathe`: sleeps `HOLD − since` while the hold lasts (no wake-ups at 50 ms), then
  `breath_wait(clock)` between frames; `cx.notify()` at each wake past the hold; stops as today when the pane loses
  focus or the key window, and when `breath_rests` (after drawing that peak frame).

- [ ] **Step 1: failing tests** (`cursor_look`)
  - `breath_frames_are_level_crossings`: simulate from clock 0 over 10 breaths, stepping by `breath_wait`: per breath
    ≤ 22 frames (today 32); consecutive drawn alphas differ by ≤ `BREATH_STEP` + 1e-4; every breath draws 1.0 and
    0.35 (±1e-4); no wait < `BREATH_MIN` or > 0.17 s (the longest is either side of a peak, 0.1625 s).
  - `the_rest_starts_at_a_peak_and_any_change_ends_it`: `breath_alpha` at `BREATH_REST_AT` is 1.0 and `BREATH_REST_AT`
    is a whole number of breaths; `breath_rests(BREATH_REST_AT − 1 ms)` false; `needs_frames(… glowing: false …)` is
    `Frames::None`; a change (`since_change` back to 0) makes `breath_rests(…)` false again.
  - Existing `breath_alpha` and `the_breath_redraws_only_once_its_hold_is_over` tests stay as they are.
- [ ] **Step 2: failing ui-shots** (family `cursor`, Mocha): `cursor-breath-resumes`: a focused prompt with
  `pretend_key_window`; `cursor_frames()` is `Breath`; set the motion's `changed` to 301.5 s ago (a `#[cfg(feature =
  "ui-shots")]` seam `age_breath(Duration)`) and run frames → `Frames::None` and the cursor pixel's alpha is the full
  colour; `type_bytes(b"x")` → `Breath` again; blur the pane and focus it back → `Breath`, alpha 1.0 at first.
- [ ] **Step 3: run** `cargo test cursor_look terminal_view` and `__ui-shots cursor` → FAIL.
- [ ] **Step 4: implement** per Interfaces.
- [ ] **Step 5: run** `cargo test`, both clippy runs, `cargo fmt --check`, the full `__ui-shots` → green.
- [ ] **Step 6: commit** `perf(m11n): the breath is drawn at level crossings, sleeps through the hold and rests after five minutes`.

### Task 5: The benchmark — prove the targets

- [ ] `cargo build --release` at the branch head; `run.py bundle fuxx-m11n <target>/release/fuxx`; in one session,
  interleaved, with the Mac otherwise idle and the window in front: `fuxx-base`, `fuxx-m11n`, `ghostty` ×
  `cpu_base` ×3 each; `fuxx-base` and `fuxx-m11n` × `cpu_noglow` ×2 and `cpu_rest` ×1; all three × `tput`, `launch`, `mem`; then
  `run.py summary`. A `cpu_*` run that isn't frontmost 15/15 is repeated (up to 5 tries).
- [ ] **Targets** (note 65 §Targets; all compare runs from this one session):
  - idle, glow on, breathing (`cpu_base`): `fuxx-m11n` ≤ **0.8 ×** `fuxx-base`;
  - idle, glow on, after five minutes (`cpu_rest`): `fuxx-m11n` ≤ `fuxx-base`'s `cpu_noglow` + 0.3 points (and
    `fuxx-base`'s `cpu_rest` still breathes, ≈ its `cpu_base`);
  - idle, glow off: `fuxx-m11n` ≤ `fuxx-base` + 0.2 points;
  - `cat` 100 MiB: `fuxx-m11n` ÷ `ghostty` ≤ **1.8** (baseline 2.68);
  - 200k coloured lines: `fuxx-m11n` ÷ `ghostty` ≤ **1.4** (baseline 1.58);
  - no regression: launch ≤ `fuxx-base` + 10 %, idle RSS and 10-tab RSS ≤ `fuxx-base` + 5 MB.
- [ ] A missed target is a bug to debug (superpowers:systematic-debugging) before merging, not a target to move;
  record the numbers in note 65 §As built either way.

### Task 6: Merge and push (pre-authorized, autonomous program)

- [ ] Full gate on the branch: `cargo test`, `cargo clippy --all-targets`, `cargo clippy --all-targets --features
  ui-shots`, `cargo fmt --check`, the full `cargo run --features ui-shots -- __ui-shots` green; a whole-branch review
  against note 65 and this plan's Review Focus; then merge `m11n-performance` into `master` and push.

### Task 7: Release v0.5.0 — the one combined release (M11i–M11n)

The workflow (`.github/workflows/release.yml`, M11e) builds on a `v*` tag: `scripts/check-release-tag.sh` (the tag
must be `v` + the `Cargo.toml` version), `cargo test`, `scripts/bundle-mac.sh --app-only`, zip, `gh release create
--generate-notes`. Earlier releases: `chore(release): 0.4.0 — …` (`28b825f`) touched only `Cargo.toml` and
`Cargo.lock`.

- [ ] On `master` after Task 6: `version = "0.5.0"` in `Cargo.toml`; `cargo build` (updates `Cargo.lock`);
  `cargo test` green.
- [ ] Commit `chore(release): 0.5.0 — UI uplift 2, Changes and Activity, ⌘K palette, first-run screen, command
  blocks, title bar, performance (M11i–M11n)`; push `master`.
- [ ] `git tag v0.5.0 && git push origin v0.5.0`; watch the run: `gh run list --workflow Release --limit 1`, then
  `gh run watch <id> --exit-status`. Green means the release `fuxx 0.5.0` exists with `fuxx-0.5.0.zip`
  (`gh release view v0.5.0`). A red run: read `gh run view <id> --log-failed`, fix on `master`, delete and re-push the
  tag only after the fix.
- [ ] Then fuxx 0.4.1 shows Update Available for 0.5.0 (M11e) — part of your on-screen check.

### Task 8: Docs

- [ ] Note 65: status, "M11n as built" (the measured numbers of Task 5 against the targets, anything the build
  settled), the checklist's state. This plan (ticks, header). `40 - Roadmap.md` (M11n, the release line). The Plan
  Tracker. The Progress Log (and its snapshot). `12 - Scope.md` (the Performance line). `STRUCTURE.md` (`pty.rs`:
  the reader and parser threads; `scripts/bench/`). `README.md` only if it states a performance figure. Notes 59–64:
  "released in v0.5.0" in each status line.
- [ ] Commit `docs(m11n): …`.
