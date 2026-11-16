---
tags: [development, milestone, design]
status: built
---

# 77 - Milestone 11p - Chrome Caching (design)

**Status:** 🟨 built, reviewed, on-screen check pending (branch `m11p-chrome-cache`, not merged). Designed 2026-11-13, built and measured 2026-11-13/14. **You skipped the design questions ("go ahead"), so every rule below
is AI's call** unless it quotes you; the ones you may most want to overrule are listed in "AI's calls".
**Asked for:** 2026-11-10, by you, after M11o ("oke, masukkan ke rencana setelah M11o. selama sidebar masih mendapatkan
update terkini, saya lanjut"), following M11n's "Found for later". Ordered before M11aa's UI tasks (your call,
2026-11-13: "Safe part, then M11p").
**Spike:** branch `m11p-spike` (`f5b150e`, throwaway, never merged) and its report (summarised in "What the spike
showed").

## Goal

fuxx left idle in front, with the cursor glow breathing, uses clearly less CPU — and **every part of the chrome
always shows current information**. The chrome is the sidebar, each pane's tab bar, the status bar and the title
area. Today every breath frame of the focused terminal makes GPUI rebuild the whole window; after M11p a breath frame
rebuilds only the terminal and a thin `Workspace::render`, and each chrome part is redrawn only when its own data
changed.

## Your hard rule (2026-11-10, quoted in [[40 - Roadmap]])

"selama sidebar masih mendapatkan update terkini" — **the sidebar, and every other chrome part, always shows current
information**: agent states, branch and folder, notification rings, ports, names, theme, hover and drag. A part that
shows stale data is a bug, and **every source of change gets a test**.

## What the spike showed (2026-11-13)

- **Caching holds:** the window redrew ~410 times per run (the breath), while the cached sidebar, status bar and tab
  bars drew 4 times each, all at startup. GPUI's `Entity::cached(style)` (pinned rev, `crates/gpui/src/view.rs`)
  reuses a view's last layout and paint while it isn't notified; `Window::mark_view_dirty` dirties a notified view
  and its **ancestors**, not its siblings — so a breath tick on the terminal leaves sibling chrome parts clean.
- **Saving (directional, two noisy sessions):** glow-on idle CPU 3.71 → 2.83 % (sidebar + status bar), and in a
  cleaner but cut-short session 4.65 → 3.93 % (sidebar + status bar) → 3.56 % (+ tab bars): roughly **15–25 % less**.
  Not precise enough to set a target; Task 0 of the plan re-measures properly.
- **The real risk is stale chrome, not the caching itself.** `Workspace::render` *changes state while drawing*:
  `refresh_places` (each terminal's folder, a ≤ 1 s check that only runs when the window redraws), `refresh_right_sidebar`
  (which terminal the status bar follows), `refresh_ports`, the tickers (`tick_marks`, `refresh_ages`,
  `tick_programs`, `tick_places`, `tick_ports`), the drag-state reset and the slide bookkeeping. A `cx.notify()` made
  during drawing reaches no observer, so under caching those results would never reach a cached part. (Today the
  breath is what drives the 1 s folder check — a latent bug: with the glow off it runs only when something else
  redraws.)
- **Other stale traps found:** the sidebar reads the window width (whether the app title shows) but its own bounds
  don't change with it; `render_sidebar` writes `sidebar.listed`, drag offsets and the reveal-scroll that the rest of
  the Workspace reads every frame; drag start / cancel (Esc) / end-without-drop weren't verified.
- **While an agent works, the saving disappears:** the status marks' 100 ms ticker notifies the Workspace, which
  rebuilds every cached part 10×/s.
- **Remaining idle cost:** `Workspace::render` still runs every breath frame, and builds the full sidebar row list
  twice per frame (`tick_marks` and `refresh_ages` both call `sidebar_rows`), plus the window title, the pane band,
  ~70 action listeners and the uncached tree.

## Goals

1. **Nothing changes state inside `Workspace::render`** (a precondition, done first). Every in-render refresh and
   ticker moves to a timer or an event that calls `cx.notify()` when its result changed.
2. **The sidebar's rows are computed once per change**, not per frame, in one place that `render_sidebar`, keyboard
   moves, drop slots and the tickers all read (replacing `render_sidebar`'s writes to `sidebar.listed` etc.).
3. **Cached chrome parts:** the sidebar, the status bar, each pane's tab bar, and the title area — each its own view,
   embedded with `.cached(...)` at a definite size.
4. **Each part is redrawn when its data changes** (see "Invalidation").
5. **The status marks animate without rebuilding whole parts:** the moving marks (Working / Waiting) become their
   own small views (or elements) that the 100 ms ticker notifies, so an agent working costs a few marks, not the
   whole sidebar.
6. **A thinner `Workspace::render`:** the window title is set only when it changes; nothing else per frame that can
   be done on change.
7. **A test for every source of change** (your rule): for each item in the dependency lists below, a test that
   changing it re-renders the part that shows it (and, where cheap, that the part shows the new value).
8. **Measured:** a baseline and a final number with the same, controlled method (below).

## Non-goals

- A renderer change or partial-window repaint: GPUI still presents whole frames; the saving is the CPU work of the
  cached parts. The stack stays as in [[30 - Tech Stack]].
- Changing the breath itself (M11n already made it cheaper).
- Caching the right sidebar (Files / Changes / Activity) and overlays (palette, preferences, menus): not in the
  roadmap's list; Task 0's profile records their cost, and caching them is a later milestone if it matters.
- Per-part fine-grained dirty flags (each part watching only its own fields): only if Task 0/the final measure shows
  the coarse rule (below) loses most of the saving in common cases.
- The fuxx-to-front behaviour seen during the spike (below, "Found along the way").

## Anti-goals (a failure even if CPU drops)

- **Any chrome part showing stale information**, ever — including after `cd`, a resize, a theme change, a drag that's
  cancelled, an agent changing state in another workspace, a port opening, an update becoming available.
- Any visible change in how the chrome looks or behaves (hover, animations, Reduce motion, drag and slide).
- Clicks, drags or drops in a cached part reaching the wrong place.

## Invalidation (AI's call: coarse, with three additions)

- **Base rule:** each cached part observes the Workspace; any Workspace `cx.notify()` redraws every part. This is
  enough for the idle goal, because a breath frame notifies the terminal, not the Workspace. It is simple and can't
  miss a Workspace field.
- **Window bounds:** the sidebar and title area also redraw on window resize and full-screen changes (they read the
  window size but their own bounds don't always change).
- **Globals:** `Appearance` (theme, animations, Reduce motion), `WindowVisible`, `updater::Updates` and the active-drag
  state redraw every part (observe the globals; the updater already uses `refresh_windows`, which bypasses caches).
- **Other entities:** data that lives in another entity and is read during render (e.g. `TerminalView::working_folder()`)
  must reach the Workspace as an event that notifies it — never be polled from render.
- **Animations inside a part** (hover, `with_animation` for the Done pop and the chevron, the drag-slide frames) keep
  working: GPUI notifies the current view, which is the part.

## Dependency lists (from the spike; the tests in Goal 7 cover each)

- **Sidebar:** sessions (status, `status_since`, question, progress, note, lost, title, pop, ssh, project); the model
  (workspaces, order, focus, names, colour, pinned, groups, `branch_terminal`, `named_terminal`); `collapsed`;
  `sidebar` (width, hidden, scroll, reveal, menus, hint, drag source / slot / drop); `renaming`; `notice`;
  notifications (unseen count, popover); places (folder, branch, home); programs (running); the marks' phase;
  `drag_slides`; `tab_drop`; `Appearance`; `WindowVisible`; active drag (row, tab); window width and full screen.
- **Status bar:** `right_sidebar.followed`; that session's status, age, ssh, project, pop; programs; places; ports;
  `updater::Updates`; `Appearance`; the phase; window width.
- **Tab bars:** the pane (tabs, active, focus); each tab's session (status, title, pop, phase); `pane_actions`;
  `right_sidebar.hidden`; `tab_scroll`; the tab drag (`tab_grab`, landing, slides); Reduce motion; the theme.
- **Title area:** the builder lists it the same way in Task 1 (the spike didn't cache it).

## Measurement (AI's call)

- **Where:** from Terminal.app, with every other fuxx quit (during the spike, the user's own fuxx — which this
  session ran in — came to the front and broke the frontmost checks), Chrome/Teams quit, Do Not Disturb on.
- **How:** `scripts/bench/run.py` (M11n's harness), idle CPU glow on, **5 runs each, variants interleaved**, the
  median; plus glow off (1 run) as the floor; plus "one agent Working" (a terminal whose status is held Working, so
  the marks tick) — 3 runs each — to check goal 5.
- **Target (AI's call):** glow-on idle **≥ 30 % below** the Task 0 baseline, and the "agent Working" case **≥ 30 %
  below** its baseline too; no regression in the M11n throughput numbers (`tput`). If the target isn't met, the
  milestone still ships what's measured as a saving and records why (as M11n did).

## A kill switch (AI's call)

`FUXX_CHROME_CACHE=off` in the environment draws the chrome uncached, as today — for debugging a suspected stale
part. Not a user setting.

## Found along the way (not M11p)

- During the spike runs, the user's own fuxx (running this agent session) came to the front by itself, several minutes
  in. Worth checking separately whether something activates the app (e.g. a status change of the agent inside it).

## AI's calls you may want to overrule

1. The precondition (nothing changes state in `Workspace::render`) comes first and is part of M11p.
2. Coarse invalidation (observe the Workspace + window bounds + globals), no per-part dirty flags.
3. The title area and tab bars are cached too, not just the sidebar and status bar.
4. The moving status marks become their own small views (Goal 5).
5. The right sidebar and overlays are not cached in M11p.
6. The target: ≥ 30 % less idle CPU (glow on) and with an agent working, measured from Terminal.app.
7. A hidden `FUXX_CHROME_CACHE=off` switch.

## Left to the builder

- The part views' exact shape (the spike's `ChromePart { workspace: WeakEntity<Workspace>, part }` calling the existing
  `render_*` through the Workspace works; a cleaner split is welcome if it keeps the draw code unchanged).
- Timer periods for the moved ticks (keep today's: folders ≤ 1 s, ages as today, programs 3 s, ports as today).
- How render-count tests are written (a `cfg(test)` counter per part, or ui-shots scenes).

## What was built

Eight plan tasks, each reviewed, then a merge of master (M11ac 1 + 2, the title-bar test fix), a whole-branch review
and one fix wave. 1661 tests; the full `__ui-shots` run passes 110 / 110 at the merge.

- **Render purity.** Nothing visible changes inside `Workspace::render`: the folder check runs on its own 1 s timer
  (`check_folders`), the status bar's followed terminal is set by events (`follow_terminal`, with a deferred safety
  net after every draw), ports clear in that same deferred call, the window title is set only when it changes, and
  `sidebar.listed` is computed on demand.
- **Rows cache.** The sidebar rows are built once per change: the cache is keyed on GPUI's `observe_self` (every
  Workspace notify bumps a generation) plus the age bucket (the granularity the age labels turn at). Per frame the
  rows went from 3 builds to 0.
- **Cached `ChromePart` views** (`src/chrome_cache.rs`) for the sidebar, the status bar, each pane's tab bar (keyed
  by pane and `drags_window`, pruned in `sync`) and the title area, each embedded with `.cached(...)` at a definite
  size.
- **Mark views.** The moving status marks are their own `MarkView`s driven by a `MarkClock`; the 100 ms ticker
  notifies only the clock, and runs only while a mark is on screen (`marks_on_screen`).
- **Ages on their own timer.** `refresh_ages` always schedules its own timer (earliest deadline), so an age label
  turns with no other redraw.
- **Tooltips are built on open**, so every new hover shows the current age.
- **Kill switch** `FUXX_CHROME_CACHE=off` (read once): the chrome draws uncached, as before.
- **Tests:** about 27 chrome ui-shots scenes (`__ui-shots chrome`), including at least one per data source in the
  dependency lists (status elsewhere, model edits, notifications, places / programs / ports, globals, resize,
  drag cancel, renaming, the age timer, row sources, header marks, file-tab toolbar), each recorded RED first where
  the source is ours to break.
- **The post-merge `pin_tab` fix** (`d539b6d`): master's Preview and File | Diff buttons kept a preview tab without
  notifying, so the cached tab bar kept it italic; `pin_tab` now notifies (scene `chrome-file-tab-toolbar`).

## Results

Measured 2026-11-14 08:19 by you, from Terminal.app, with `scripts/bench/m11p.sh final` (one release build per
variant; variants interleaved; medians of the runs that were frontmost 15 / 15; the full table is in
`scripts/bench/results/m11p-final-2026-11-14.md`, gitignored). CPU %:

| Variant | glow on, idle | agent working | glow off |
|---|---|---|---|
| master | 5.03 (4 runs) | 4.20 (3 runs) | 1.40 (1 run) |
| branch, cache on | **3.80** (5 runs) | **3.23** (3 runs) | not measured |
| branch, `FUXX_CHROME_CACHE=off` | 5.29 (4 runs) | 3.94 (2 runs) | not measured |
| branch vs master | **−24 %** | **−23 %** | |

Some runs were dropped for not being frontmost 15 / 15 (master idle 1, cache-off idle 1, cache-off working 1), so
each variant has 2–5 counted runs: directional, not precise. **You did not confirm by eye that the status bar read
"Working" during `cpu_working`** (the script holds a terminal printing every 0.5 s); the working numbers are
recorded on that basis. The M11n throughput numbers (`tput`) were not re-run in this session.

**The ≥ 30 % target was narrowly missed** (−24 % idle, −23 % working). Read against the floor: with the glow off
fuxx idles at 1.40 %, so the glow costs about 3.6 % on master and about 2.4 % on the branch — **caching removes
about a third of the glow's cost above the floor**. Why not more:

- `Workspace::render` still runs every breath frame (about 70 listeners, the pane band, the uncached tree), and the
  focused terminal draws.
- While an agent works, GPUI dirties the **ancestors** of a notified view (Ruling 12): a mark view inside a cached
  part dirties the part, so the status bar and the Working tab's bar still redraw per tick (the sidebar, the title
  area and the other bars do not).
- Per-frame `marks_on_screen` (twice) and `rows_in_view` clones remain.

Follow-ups that could close the gap: an **overlay layer** for the moving marks (the only way to reach +0 while an
agent works; risks stacking, clipping, slide and drag glitches against your "no visible change" rule), a thinner
`Workspace::render`, and removing the per-frame row clones.

## Limits

- **GPUI-owned refresh paths** (window resize, full screen, drag release / stop, focus, `set_reduce_motion`) are not
  RED-provable without editing GPUI; the scenes pass, but their observers can't be broken to show the scene bites.
- **"Maybe" steps:** Reduce motion, window visible and drag-released steps only prove "equal to a full redraw";
  their own observers are not isolable (the breath re-renders anyway). Extra frames from catch-up or ticking marks
  can mask staleness (the RED runs show the scenes still bite).
- **A tooltip left open keeps its text** (Ruling 10): GPUI builds a tooltip once per hover, so one open across an age
  change can show "59s" for up to a minute; every new hover is current.
- **The scene-retry residual** (Ruling 17): a step retries once when an age turned during its check; a real stale in
  the same view as a turning label that coincides with the turn is repaired by the turn's timer redraw before the
  retry, and a turn in the microseconds between catch-up and the first label read reads as STALE (a false positive,
  not a hide).
- `tick_places` (the 5 s places loop) is now redundant with the 1 s folder timer.
- Pressing Esc doesn't cancel a sidebar row drag (pre-existing, found by the drag-cancel scene).
- The `title-bar-sliding` scene runs on real time and has about a 20 ms margin from build to frame; a slow machine
  can fail it.
- A plain `cd` with no OSC 7 shows within about 6 s, as on master (Ruling 7, below).
- Task 5 limits: `TitleChanged` / `DaemonLost` are emitted on the view (skipping the event translation), `report()`
  uses the `shot_status` hook, and full screen is argued by analogy to resize.

## Plan-time calls (AI's, from reading the code)

1. **Render-count tests live in ui-shots scenes**, not `cargo test`: GPUI's test contexts exist only with
   `--features ui-shots` here, and AppKit windows abort under `cargo test` threads.
2. **The `tick_*` restarters may stay in render** (`tick_marks`, `tick_places`, `tick_programs`, `tick_ports`,
   `tick_right_sidebar`, `refresh_ages`): they only spawn timers whose later notifies happen outside drawing. What
   leaves render are the synchronous visible writes: the in-render folder check (`refresh_places`),
   `right_sidebar.followed.replace` (`refresh_right_sidebar`), `ports.clear()` (`refresh_ports`), the per-frame
   `set_window_title`, and `render_sidebar`'s write to `sidebar.listed`.
3. **No window-bounds observer**: `bounds_changed` already refreshes (bypassing caches); a scene proves the title
   hides / shows on resize; an observer is added only if that scene fails.
4. **Tab-bar parts are keyed by `(PaneId, drags_window)`** and pruned in `Workspace::sync`.
5. **Measurement is a script you run** from Terminal.app with fuxx quit (`scripts/bench/m11p.sh`), because the
   agent's own session runs inside your fuxx, which came to the front and broke the spike's runs. One branch build
   measures cache on vs `FUXX_CHROME_CACHE=off`, alongside a `master` build.

**Rulings that changed the design:**

- **Ruling 7:** the design said a plain `cd` shows "within ~1 s". Wrong: each terminal's folder is looked up every
  5 s (`folder_due`); the 1 s timer only picks who is due. Behaviour is as on master: **a plain `cd` without OSC 7
  shows within ~6 s** (with OSC 7, at once). No regression.
- **Ruling 12:** GPUI's `mark_view_dirty` dirties a notified view **and all its ancestors**, and a dirty cached
  view re-renders, so a mark view inside a cached part re-renders the part every tick. "Parts +0 while marks tick"
  is unreachable without an overlay layer or a GPUI patch. Built instead: the ticker notifies only the clock; the
  Title, the tab bars without a moving mark and the row rebuilds go to 0 per tick; the sidebar, the status bar and
  the Working tab's bar still redraw per tick (cheaper than before). The overlay layer is the recorded follow-up.
  This changes Goal 5: marks cost a few views plus the parts that hold them, not nothing.
- **Ruling 13:** stale `marks` entries kept their last status and re-dirtied the window while ticking; the ticker
  now stops when nothing on screen moves (agent working only elsewhere: 0 frames over 10 ticks), and the age timer
  is scheduled by `refresh_ages` itself.

## On-screen checklist (for you, after the build)

Run from the branch build. At least:

- `cd` in a terminal updates the sidebar folder and the status bar: **with OSC 7 at once; a plain `cd` within ~6 s**.
- Resize the window across the width where the app title hides / shows; toggle full screen.
- Switch theme; toggle Reduce motion.
- An agent goes Working → Waiting → Done in another workspace (ring, mark, age).
- **An agent working in another workspace: no mark pulses on screen, and CPU stays low** (Activity Monitor).
- Open a dev server port (it shows in the sidebar row and the status bar).
- Drag a row and drop it outside (Esc doesn't cancel a row drag, pre-existing); drag a tab between panes and drop it
  outside.
- Rename a workspace and a tab; the update pill.
- **`FUXX_CHROME_CACHE=off` looks identical** (launch with it set and compare the chrome).
- Idle CPU in Activity Monitor with the glow on (about 3.8 % measured, against 5.0 % before).

## Related

[[40 - Roadmap]] (M11p) · [[65 - Milestone 11n - Performance]] · [[12 - Scope]] · [[30 - Tech Stack]]
