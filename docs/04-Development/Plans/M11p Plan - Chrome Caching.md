---
tags: [development, plan, milestone]
milestone: M11p
status: built
updated: 2026-11-14
---

# M11p Plan — Chrome caching

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [x]`) syntax for tracking.

| At a glance | |
|---|---|
| **Design (the spec)** | [[77 - Milestone 11p - Chrome Caching]] |
| **Status** | 🟨 Built and reviewed on branch `m11p-chrome-cache` (not merged); on-screen check pending. Results: −24 % idle CPU (glow on), −23 % with an agent working; the 30 % target narrowly missed, see note 77. |
| **Tasks done** | 8 / 8 (Task 7 Step 1 run by you 2026-11-14) |
| **▶ Pick up here** | Your on-screen check (note 77), then merge on your word |
| **Decisions used** | Yours: M11p after M11z, before M11aa's UI; the hard rule "every chrome part always current". Everything else: AI's calls in note 77 (you said "go ahead"), plus "Plan-time calls" below. |
| **Needs your eyes** | 👀 Task 0 and Task 7 measurements (you run one script); the on-screen checklist from Task 7 |

**Goal:** Idle CPU with the glow breathing (and with an agent working) drops ≥ 30 %, because a breath frame no
longer rebuilds the sidebar, status bar, tab bars or title area — and every one of those parts still always shows
current information.

**Architecture:** First make `Workspace::render` free of synchronous, visible state changes (Task 2) and stop it
building the sidebar rows several times per frame (Task 3). Then each chrome part becomes a `ChromePart` view
(`src/chrome_cache.rs`) that renders the existing `render_*` code through the Workspace and is embedded with GPUI's
`Entity::cached(style)` (Task 4); each part re-renders when the Workspace notifies, when globals change (the Workspace
already observes them) and on GPUI's whole-window refresh (resize, activation, drag start/end). Moving status marks
get their own tiny views driven by a shared phase clock (Task 6). Render-count scenes in the ui-shots harness prove
each data source reaches its part (Tasks 1, 5).

**Tech Stack:** Rust; GPUI at the pinned rev `1d2a4b3f…` (`Entity::cached` `view.rs:232`, `Window::mark_view_dirty`
`window.rs:1814`, `WindowInvalidator::invalidate_view` `window.rs:153` — a notify during a draw reaches no observer;
`bounds_changed` → `refresh()` `window.rs:2290`; `Context::observe_window_bounds` `app/context.rs:426`); the ui-shots
harness (`VisualTestAppContext`, feature `ui-shots`); `scripts/bench/run.py` (M11n). **No new crates.**

**Spec:** [[77 - Milestone 11p - Chrome Caching]]. **Spike:** branch `m11p-spike` (`f5b150e`) — its
`src/chrome_spike.rs` is the starting point for Task 4 (rename, harden, drop the stderr counter printing).

## Global Constraints

- **No new crates.** No new user setting. One env switch: `FUXX_CHROME_CACHE=off` draws every part uncached (read
  once at startup).
- **No visible change**: same look, hover, animations (Done pop, chevron, drag slides), Reduce motion, clicks, drags,
  drops. Every chrome part always shows current data (the user's hard rule).
- Cached parts: **sidebar, status bar, each pane's tab bar, title area** (workspace name + band buttons). **Not**
  cached: the right sidebar, the notifications popover, the palette, preferences, menus; the panes stay cached as
  today.
- Each cached part is embedded at a **definite size** (`Entity::cached` never measures content): sidebar
  `flex_none().w(px(sidebar.width)).h_full()`; status bar `flex_none().w_full().h(px(STATUS_BAR))`; tab bar
  `flex_none().w_full().h(px(TAB_BAR))`; title area `absolute().top_0().left_0().w_full().h(px(TOP_BAND))`.
- **Invalidation (coarse):** each part observes the Workspace (`cx.observe(&workspace, |_, _, cx| cx.notify())`).
  Globals (`Appearance`, `SystemReduceMotion`, `WindowVisible`) already notify the Workspace (`workspace.rs:839-853`);
  `updater::Updates` and plugins use `cx.refresh_windows()` (bypasses caches); window resize / activation call
  `window.refresh()` (bypasses caches) — Task 5 tests that instead of adding an observer.
- **Nothing visible is changed synchronously inside `Workspace::render`** after Task 2: render may start timers and
  read state; anything it would set that a part shows is set outside drawing (a timer, an event handler, or
  `cx.defer`) followed by `cx.notify()`.
- Timer periods unchanged: folders checked ≤ 1 s, places loop 5 s, programs 3 s, ports 5 s, right sidebar 2 s, marks
  100 ms, ages as today.
- **Gate per task:** `cargo test --bin fuxx && cargo clippy --all-targets && cargo clippy --all-targets --features
  ui-shots && cargo fmt --check && cargo run --features ui-shots -- __ui-shots chrome` (no new clippy warnings over the
  baselines recorded in Task 0; the pre-existing `title-bar-sliding` scene failure is not M11p's; it was fixed on master). The full
  `__ui-shots` run once per task before its commit. (`title-bar-sliding` was a test bug, fixed on master in `7894f5e` and merged into this branch.)
- Work in a git worktree on branch `m11p-chrome-cache` from the then-current `master`. Commit per task; merging is the
  user's call.

## Plan-time calls (AI's, from reading the code; Task 7 copies them into note 77)

1. **Render-count tests live in ui-shots scenes**, not `cargo test`: GPUI's test contexts exist only with
   `--features ui-shots` here, and AppKit windows abort under `cargo test` threads (`ui_shots.rs:1-9`).
2. **The `tick_*` restarters may stay in render** (`tick_marks`, `tick_places`, `tick_programs`, `tick_ports`,
   `tick_right_sidebar`, `refresh_ages`): they only spawn timers whose later notifies happen outside drawing. What
   leaves render are the **synchronous visible writes**: the in-render folder check (`refresh_places`),
   `right_sidebar.followed.replace` (`refresh_right_sidebar`), `ports.clear()` (`refresh_ports`), the per-frame
   `set_window_title`, and `render_sidebar`'s write to `sidebar.listed`.
3. **No window-bounds observer**: `bounds_changed` already refreshes (bypassing caches); Task 5 proves the title
   hides/shows on resize; an observer is added only if that scene fails.
4. **Tab-bar parts are keyed by `(PaneId, drags_window)`** and pruned in `Workspace::sync` (the spike never removed
   them).
5. **Measurement is a script the user runs** from Terminal.app with fuxx quit (`scripts/bench/m11p.sh`), because the
   agent's own session runs inside the user's fuxx, which came to the front and broke the spike's runs. One branch
   build measures cache on vs `FUXX_CHROME_CACHE=off`, alongside a `master` build.

## Review Focus

1. **A plain `cd` with no OSC 7** (folder found only by polling): the sidebar row's folder and the status bar update
   within ~6 s as on master (each terminal's folder is looked up every 5 s; the 1 s timer only picks who is due;
   Ruling 7), with the glow breathing and with it off. → Task 2 scene `chrome-folder-polled`.
2. **An agent in another workspace changes state while fuxx sits idle**: ring, mark, age label and tab mark update.
   → Task 5 scene `chrome-status-elsewhere`.
3. **Window resized across the app-title threshold, and full screen toggled**: the title shows/hides; band buttons
   move. → Task 5 scene `chrome-resize-title`.
4. **A row or tab drag cancelled with Esc, or dropped outside**: drop indicators and slide offsets clear in every
   part. → Task 5 scene `chrome-drag-cancel`.
5. **A pane split, closed, or moved**: the new pane's tab bar appears, the closed one's part is dropped (no stale bar,
   no leak). → Task 4 scene `chrome-pane-churn`.

---

### Task 0: Baseline measurement script (the user runs it)

**Files:** Create `scripts/bench/m11p.sh`; modify `scripts/bench/run.py` (a `cpu_working` phase; pass
`FUXX_CHROME_CACHE` through, as the spike did); record results in this plan's "At a glance" and note 77.

**Interfaces:**
- Produces: `scripts/bench/m11p.sh baseline|final [--dry-run]` — builds release binaries into a private
  `CARGO_TARGET_DIR` (`baseline`: `master`; `final`: `master` + the branch), bundles them (`run.py … bundle`), then
  runs, interleaved, `cpu_base` × 5 and `cpu_working` × 3 per variant (`final` variants: `master`, `branch`, `branch`
  with `FUXX_CHROME_CACHE=off`) plus `cpu_noglow` × 1 on `master`, then `run.py … summary`; prints the medians table
  and writes it to `scripts/bench/results/m11p-<mode>-<date>.md` (gitignored).
- `cpu_working`: like `cpu_base`, but the first terminal runs a loop printing a line every 0.5 s (enough for
  `StatusTracker` to hold `Working`, so the marks tick) — confirm in one run that the status reads Working.

- [x] **Step 1:** Record clippy baselines on the branch base (`cargo clippy --all-targets 2>&1 | grep -c '^warning'`,
  and the same with `--features ui-shots`); write both into "At a glance".
- [x] **Step 2:** Add `cpu_working` to `run.py` and write `m11p.sh`. Dry-run `bash scripts/bench/m11p.sh baseline
  --dry-run` (prints the commands, runs nothing).
- [x] **Step 3: The user runs** `bash scripts/bench/m11p.sh baseline` from Terminal.app with every fuxx quit, Chrome
  and Teams quit, Do Not Disturb on, Mac untouched (~10 min). Paste the medians into "At a glance" and note 77. If a
  variant has fewer than 3 counted runs, the user re-runs it.
- [x] **Step 4: Commit** `M11p: baseline measurement script and numbers`.

### Task 1: Render counters for chrome (ui-shots only)

**Files:** Create `src/chrome_cache.rs` (counters only in this task); modify `src/workspace.rs` (count
`Workspace::render`), `src/sidebar.rs`, `src/status_bar.rs`, `src/tab_bar.rs`, `src/title_area.rs` (count each
`render_*`), `src/main.rs` (`mod chrome_cache;`), `src/ui_shots.rs` (helpers + one scene).

**Interfaces:**
- Produces (in `chrome_cache.rs`): `pub enum Counted { Workspace, Sidebar, StatusBar, TabBar, Title, SidebarRows }`;
  `pub fn bump(c: Counted)` (an empty function without the `ui-shots` feature, so call sites need no `cfg`);
  `#[cfg(feature = "ui-shots")] pub fn get(c: Counted) -> u64` and `pub fn reset()`.
- Produces (in `ui_shots.rs`): `fn draw(cx: &mut VisualTestAppContext, window: AnyWindowHandle)` (one explicit
  `window.draw(cx).clear()`, as the `cached_panes` scene does at `ui_shots.rs:1204-1253`) and
  `fn draws_of(c: Counted, cx: &mut VisualTestAppContext, window: AnyWindowHandle, act: impl FnOnce(&mut
  VisualTestAppContext)) -> u64` (reset, act, draw, read).
- [x] **Step 1:** Scene `chrome-counts-baseline`: a Workspace from `Workspace::for_shots` with two terminals; notify the
  focused `TerminalView` (a breath-like tick) and draw → today every counter rises by 1 per draw (documents the
  problem; Task 4 flips this scene's expectation).
- [x] **Step 2:** `SidebarRows` is bumped in `sidebar_rows` (`sidebar.rs:1175`); the scene asserts the current count
  per frame (3–4 today) so Task 3 has a number to beat.
- [x] **Step 3:** Gate; **commit** `M11p: chrome render counters for ui-shots`.

### Task 2: Nothing visible changes inside `Workspace::render`

**Files:** Modify `src/git_branch.rs` (`refresh_places` `:635`, `tick_places` `:695`), `src/right_sidebar.rs`
(`refresh_right_sidebar` `:1141`), `src/workspace.rs` (`refresh_ports` `:427`, title at `:3179-3189`,
`Workspace::render` `:3145`), `src/sidebar.rs` (`listed`, `drag_row_over_list` `:2318`), `src/ui_shots.rs`.

**Interfaces:**
- Produces:
  - `pub(crate) fn check_folders(&mut self, cx: &mut Context<Self>) -> bool` (git_branch.rs): the old in-render
    "due" folder check, run by its own 1 s timer (`places.check_due` keeps `CHECK` = 1 s) that calls `cx.notify()`
    when a folder changed; `Workspace::render` no longer calls `refresh_places`.
  - `pub(crate) fn follow_terminal(&mut self, cx: &mut Context<Self>)` (right_sidebar.rs): sets
    `right_sidebar.followed` and notifies when it changed. Called from `note_active` (`workspace.rs:2496`), pane focus
    changes and session open/close; and **as a safety net, once per render via `cx.defer`** (a deferred call runs
    after the draw, so its notify reaches the parts).
  - The "no subject → `ports.clear()`" branch of `refresh_ports` moves into the same deferred safety net and notifies
    when it cleared something.
  - `last_title: Option<String>` on `Workspace`: `set_window_title` only when the title changed.
  - `fn listed(&self) -> Vec<Listed>` (sidebar.rs): computed on demand from the current rows for
    `drag_row_over_list`; the `sidebar.listed` field is removed and `render_sidebar` no longer writes it.
- [x] **Step 1: Failing scenes** — `chrome-folder-polled` (Review Focus 1: a terminal's folder changes with no OSC 7,
  via the shot harness's place fixture; after the 1 s timer fires and one draw, the sidebar row and status bar show
  the new folder with no other redraw source); `chrome-followed` (focus another workspace → after one draw the status
  bar describes its terminal); `chrome-title-once` (10 breath draws → `set_window_title` called at most once; count
  it).
- [x] **Step 2:** Run `cargo run --features ui-shots -- __ui-shots chrome-` → those scenes fail.
- [x] **Step 3:** Implement. **Step 4:** Scenes pass; full gate (all existing tests and scenes unchanged).
- [x] **Step 5: Commit** `M11p: no visible state changes inside Workspace::render`.

### Task 3: Sidebar rows built once per change

**Files:** Modify `src/sidebar.rs` (`sidebar_rows` `:1175`, `rows_in_view` `:1330`, `visible_statuses` `:1281`,
`refresh_ages` `:1345`, `workspaces_with_rows` `:1268`), `src/workspace.rs` (`marks_move` `:3092`,
`program_subjects` `:254`), `src/git_branch.rs` (`place_subjects` `:607`).

**Interfaces:**
- Produces: on `Workspace`, `rows_cache: RefCell<Option<RowsCache>>` with `struct RowsCache { generation: u64,
  age_bucket: u64, rows: Rc<Vec<SidebarRow>> }` and `fn rows(&self, now: Instant) -> Rc<Vec<SidebarRow>>`, which
  rebuilds only when the Workspace has notified since the last build (`generation`) or the age labels' bucket changed
  (the granularity `next_age_change` uses). `generation` is bumped on **every** Workspace notify — use GPUI's
  self-observation if the pinned rev has it (check `crates/gpui/src/app/context.rs` for `observe_self` or observing
  `cx.entity()`); otherwise say so and stop (don't hand-instrument every `cx.notify()` call site). Every row user
  (the callers listed above, `tab_menu.rs:183,327`, `render_sidebar`) reads `rows()`.
- [x] **Step 1: Failing scene** `chrome-rows-once`: 10 breath draws with no Workspace change → `SidebarRows` ≤ 1 in
  total. Plus a unit test in `sidebar.rs`: `rows_cache_rebuilds_on_change_and_on_age_bucket` (the cache-key logic as
  a pure function).
- [x] **Step 2:** → fails. **Step 3:** Implement. **Step 4:** pass; full gate.
- [x] **Step 5: Commit** `M11p: sidebar rows built once per change`.

### Task 4: Cached chrome parts

**Files:** Modify `src/chrome_cache.rs` (bring in the spike's `ChromePart`), `src/workspace.rs` (embedding in
`render` and `render_panes` `:2843`, pruning in `sync` `:1592`), `src/ui_shots.rs`.

**Interfaces:**
- Produces:
  ```rust
  pub enum ChromeKind { Sidebar, StatusBar, TabBar { pane: PaneId, drags_window: bool }, Title }
  pub struct ChromePart { workspace: WeakEntity<Workspace>, kind: ChromeKind, _observe: Subscription }
  impl ChromePart { pub fn new(workspace: &Entity<Workspace>, kind: ChromeKind, cx: &mut Context<Self>) -> Self; }
  impl Render for ChromePart { /* workspace.update(cx, |ws, cx| ws.render_<kind>(…)), as the spike does */ }
  pub fn enabled() -> bool;   // FUXX_CHROME_CACHE != "off", read once (OnceLock)
  #[derive(Default)] pub struct ChromeParts { pub sidebar: Option<Entity<ChromePart>>,
      pub status_bar: Option<Entity<ChromePart>>, pub title: Option<Entity<ChromePart>>,
      pub tab_bars: HashMap<(PaneId, bool), Entity<ChromePart>> }   // field `chrome` on Workspace
  ```
  The title part renders `render_workspace_name` + `render_band_buttons` (`title_area.rs:529`, `:569`) in one absolute
  full-width `TOP_BAND` container; the notifications popover stays outside. While the sidebar slides
  (`workspace.rs:3212-3217`) the sidebar part keeps its fixed width inside the moving wrapper.
- [x] **Step 1: Failing scenes** — `chrome-counts-baseline` becomes `chrome-breath-skips-parts`: 10 breath draws →
  `Workspace` +10, every part +0. `chrome-notify-redraws-parts`: one Workspace notify → every part +1.
  `chrome-pane-churn` (Review Focus 5): split, close, move a pane → one tab-bar part per live pane;
  `chrome.tab_bars.len()` equals the live (pane, drags_window) keys. `chrome-kill-switch`: with
  `FUXX_CHROME_CACHE=off` every part counts per draw as today. `chrome-pixels-same`: a busy workspace's screenshot is
  identical with the cache on and off.
- [x] **Step 2:** → fail. **Step 3:** Implement. Check that clicks and drags in each part still reach the Workspace
  (`cx.listener` targets the Workspace) and that the title part's container doesn't swallow clicks meant for what's
  beneath it (scene step: click a band button and the sidebar toggle).
- [x] **Step 4:** pass; full gate. **Step 5: Commit** `M11p: cached chrome parts`.

### Task 5: Every source of change reaches its part (the user's rule)

**Files:** `src/ui_shots.rs` (scenes); fixes wherever a scene fails.

One scene per dependency group (note 77's lists). Each changes the source, draws once, and asserts the part
re-rendered **and** shows the new value (a screenshot that differs from before, or the value read back from the
part's state):
- `chrome-status-elsewhere` (Review Focus 2): a terminal's status in a non-focused workspace → the sidebar's ring,
  mark and age; that workspace's tab mark.
- `chrome-model-edits`: rename, reorder, colour, pin, collapse a group, new / close a workspace.
- `chrome-notifications`: an unseen notification → the footer bell; open the popover.
- `chrome-places-programs-ports`: folder + branch, a running program, a port → sidebar row and status bar.
- `chrome-globals`: theme switch; Reduce motion on/off; window hidden/visible; an update becoming available (the pill).
- `chrome-resize-title` (Review Focus 3): resize across `shows_title`'s threshold; full screen on/off.
- `chrome-drag-cancel` (Review Focus 4): start a row drag, Esc → indicators and slides clear; a tab drag dropped
  outside → the same.
- `chrome-renaming`: start and finish an inline rename in the sidebar and in a tab.
- [x] **Step 1:** Write all scenes. **Step 2:** Run; fix each failure at its source (an event that notifies, or
  `cx.defer` + notify) — never by leaving a part uncached. Name each fix in the commit message.
- [x] **Step 3:** Full gate. **Step 4: Commit** `M11p: a scene for every chrome data source`.

### Task 6: Moving marks animate on their own

**Files:** Create `src/mark_view.rs`; modify `src/status_mark.rs`, `src/sidebar.rs` (`session_mark` `:2209`,
`session_mark_sized` `:2220`, the closed-group header `:1625`), `src/tab_bar.rs` (`:527-535`), `src/status_bar.rs`
(`:612`), `src/palette.rs` (`refresh_palette` `:1184`), `src/workspace.rs` (`tick_marks` `:3106`, `phase`).

**Interfaces:**
- Produces: `pub struct MarkClock { pub phase: f32 }` — an entity; the 100 ms loop in `tick_marks` advances it and
  notifies **only the clock** (not the Workspace). `pub struct MarkView { clock: Entity<MarkClock>, status: Status,
  size: Pixels, fill: Rgb, _observe: Subscription }` — observes the clock, renders `status_mark::mark_on`. The
  Workspace keeps `marks: HashMap<(&'static str, u64), Entity<MarkView>>` keyed by (place, session id), updates a
  view's status/size/fill when its part renders (notifying the view only when they changed), and prunes it in `sync`.
  `session_mark` / `session_mark_sized` return the `MarkView` as a child element; the Done pop (`with_animation`)
  stays as it is; the palette reads `MarkClock::phase`.
- [x] **Step 1: Failing scene** `chrome-working-marks`: one terminal Working; 10 clock ticks → `Sidebar`, `StatusBar`,
  `TabBar` +0, each visible mark view +10. Re-run `chrome-pixels-same` with a Working mark mid-beat.
- [x] **Step 2:** → fail. **Step 3:** Implement. **Step 4:** pass; full gate.
- [x] **Step 5: Commit** `M11p: status marks animate in their own views`.

### Task 7: Final measurement, docs, on-screen checklist

**Files:** note 77 (results, "Plan-time calls", limits, the on-screen checklist), `40 - Roadmap.md`,
`60 - Progress Log.md`, this plan ("At a glance", checkboxes).

- [x] **Step 1: The user runs** `bash scripts/bench/m11p.sh final` (same conditions as Task 0). Record the medians
  (master / branch / branch with the cache off; glow on, agent working, glow off) against the targets: ≥ 30 % below
  the baseline for glow-on and for agent-working; `tput` unchanged within noise. If a target is missed, record why
  (as M11n did).
- [x] **Step 2:** Note 77: what was built, the numbers, limits, the on-screen checklist (note 77's list, plus
  "`FUXX_CHROME_CACHE=off` looks identical"). Roadmap → "🟨 built, on-screen check pending"; a Progress Log entry at
  the top with the test count.
- [x] **Step 3: Commit** `M11p: results and docs`.
