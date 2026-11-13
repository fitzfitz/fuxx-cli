---
tags: [development, milestone, design]
status: designed
---

# 77 - Milestone 11p - Chrome Caching (design)

**Status:** 📝 Designed 2026-11-13; plan next. **You skipped the design questions ("go ahead"), so every rule below
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

## On-screen checklist (for you, after the build)

To be written by the plan's last task: at least — `cd` in a terminal updates the sidebar folder and the status bar
within a second; resize the window across the width where the app title hides/shows; switch theme; toggle Reduce
motion; an agent goes Working → Waiting → Done in another workspace (ring, mark, age); open a dev server port; drag a
row and cancel with Esc; drag a tab between panes; rename a workspace; the update pill; idle CPU in Activity Monitor
with the glow on.

## Related

[[40 - Roadmap]] (M11p) · [[65 - Milestone 11n - Performance]] · [[12 - Scope]] · [[30 - Tech Stack]]
