# Agent activity view — design

**Status:** design, awaiting your approval (builder check done; its questions settled below) · **Date:** 2026-11-24 · **Kind:** a chore under "Chores after M11ar" in
[[93 - Milestone 11ar - Subagents]] (your word: "Ensure we do this as chore"), not a milestone.

## Intent

The user, watching an agent work in a fuxx agent tab: "run superpowers mockup to enhance the UI and UX on this"
(a screenshot of a subagent lane full of `Bash  python3 - <<'EOF'` / `sed -n …` rows), then "deep dive it", then
"I love C". The bar for UX work: "make it easy to use and easy to access, feels like required and have the UX great!
we make this different not like any others". Performance and ease of use always win.

Today the tool-call view answers "what command ran", badly. The user wants it to answer **what the agent touched and
how the checks went**, at a glance, while it works.

## What's wrong today (found in the code)

- A row's text is `summary` = the file name, else the command's **first line** (`claude.rs::tool_summary`), so a
  `python3 - <<'EOF' … EOF; cargo test …` shows only `python3 - <<'EOF'`.
- The right side is `result` = the output's first line cut to 60 characters (`short()`): an arbitrary fragment.
- `ok` is only `!is_error`. `cargo test … 2>&1 | tail -20` exits 0 (tail's code), so a failed build can show a
  **green** dot.
- Claude's Bash `description` (plain words the model writes for every call) is never read.
- A run of tool rows is one bordered group with nothing saying what it achieved; in long runs you can't see progress
  or what is running now.

## Goals

1. See **which files a step changed, which it only read, and how its checks went**, without reading commands.
2. Tell a **real failure** from one the agent planned ("red first") at a glance; a real failure is never quiet.
3. Always see **what is running now** and the turn's progress, even scrolled away.
4. Every call stays one hover/click away in plain words (and its full detail, as today).
5. Built on **one provider-neutral activity model**, so a later ACP provider (roadmap: "ACP client", planned later)
   fills it from ACP's `kind` / `locations` / `content` / `status` with no new UI work.

## Non-goals

- No change to what agents run, no model calls, no new agent features.
- No live streaming of a tool's output (Claude sends output only when a call ends).
- Not the ACP client itself.
- No reordering of the conversation: narration stays where the agent wrote it.

## Anti-goals (a failure even if it works)

- A view that **hides a real failure** among reads, or paints a failed check green.
- Claiming certainty fuxx doesn't have: a file a script names says "touched by a script", never "edited" or
  "changed"; a failed call never counts as having changed anything.
- A busier screen: if the tracks read as noise, it failed. Small steps stay plain rows.
- Per-frame cost growth: drawing must stay as cheap as today's rows.

## Constraints

- Native GPUI, fuxx's tokens and themes (light and dark), `crate::reduce_motion`, keyboard reachable.
- Agent-specific parsing stays in `src/providers/<agent>.rs`; generic code asks the activity model.
- fuxx-cli is public: examples use fuxx-cli's own paths (`code/my-app`, `src/api.rs`).
- History: restored Claude conversations must draw the same (times may be unknown there); fuxx reads no agy history.
- AGENTS.md "build for reuse, keep files lean, no duplication": extend the existing tool group, `overlay_panel::shell`,
  the diff view and status pill instead of copying them; new parsing lives in its own focused modules.
- Gate as every chore: tests first, `cargo test --bin fuxx` (+ `--features ui-shots`), both clippy runs at baseline,
  `cargo fmt --check`, the full `scripts/ui-shots.sh`.

## Approaches (considered; C chosen)

- **A · Readable actions.** Same rows; each says what it did ("Searched api_client.rs 80–400", "Edited api.rs:543")
  with outcome chips (+/−, matches, "build failed · E0425", "red → green · 2 passed"). Small, ~3 tasks. Fixes
  legibility, not the "what did it touch" question.
- **B · Step summaries.** Each narration step folds to one line with a bead strip and an outcome sentence; the
  running step stays open; a "now" pill. Medium, ~5 tasks. Calmer, but still organised by time.
- **C · File tracks + turn ledger — chosen** ("I love C"). Organise a busy step by **what it touched**: a track per
  file plus a checks track, one bead per call in time order; small steps stay as A's rows; a turn ledger keeps the
  running call, changed files and checks in view. Large, ~6–7 tasks. A's row wording is part of C (small steps,
  hovers, narrow panes).

## The design

### 1. The activity model (provider-neutral)

Each finished tool call gets an `Activity`, computed **once** when the call finishes (and when history is read), and
stored on the item. Drawing reads it; nothing is parsed per frame.

```
Activity {
  kind:    Read | Edit | Delete | Move | Search | Execute | Fetch | Think | Other   // ACP's ToolKind
  title:   String              // plain words: the agent's own description if any, else derived verb + object
  touches: Vec<Touch>          // files: { path, how: Read{lines?} | Changed{plus?, minus?} | ScriptTouched | Searched{matches?} }
  failed:  Option<String>      // the call itself failed (not a check): its first error line; it touched nothing
  check:   Option<Check>       // { what: Tests|Build|Lint, scope: String, outcome: Passed{n?} | Failed{failed?, of?, first_error?} | Unknown }
  planned_failure: bool        // the failure was announced (see 4)
  exit:    Option<i32>         // when the provider reports one
}
```

- Providers fill it: Claude and agy today by parsing their tool events; an ACP provider later straight from
  `kind`, `locations` (paths + lines), `content` (diffs, terminal output) and `status`. A provider-trait method
  returns it; generic code never parses commands itself except through the shared helpers below.
- **Shared helpers (generic, pure, unit-tested):**
  - **Command reading** (Bash/execute): split on `&&`, `;`, `|` and newlines, skipping heredoc bodies; map the first
    word to a verb (`sed -n` / `cat` / `head` / `rg` / `grep` = read or search; `sed -i`, `>` redirects = change;
    `cargo test` / `cargo build` / `cargo clippy` / `pytest` / `jest` / `vitest` / `go test` = check); collect file
    arguments, and paths named inside a heredoc (those count as `ScriptTouched`: a script may only read them).
  - **Check reading** from output (exit codes lie through pipes): cargo test's `test result:` lines and
    `error[E…]` / "could not compile"; pytest's `N passed, M failed`; jest/vitest's `Tests: … failed … passed`;
    `go test`'s `FAIL` / `ok`. Anything else: the provider's exit or ok/failed, else Unknown.
  - **Exit codes:** Claude's failing Bash result starts `Exit code N`; agy's output starts with a header holding
    `Command exited with code N.`.
- What each provider already gives: Read `file_path`/`offset`/`limit`; Edit/Write a unified diff (+/− already
  computed; the diff doesn't know its line, so a line number only comes from a `sed` address) and `writes`;
  Grep/Glob `pattern`/`path`/`glob`; WebFetch `url`; Claude's Bash `description`. agy: `toolSummary` only in its
  transcript files (subagent lanes), not Main's stream, so agy Main uses fuxx's wording; agy edits carry no text, so
  no +/−; its exit line (`Command exited with code N.`) is in a header at the **start** of the output. Task/Agent
  stays a lane (M11ar) and breaks a track group (as do "Passed on" rows); TodoWrite stays its own item.
- **Times are new:** tool items store no start/finish time today; record both (live only) for elapsed and headers.

### 2. How a step draws

A **step** is the agent's narration line plus the run of tool calls after it (today's group).

- **1–2 calls:** readable rows (approach A's wording), as today's 30 pt rows:
  `● Edited link_route.rs                        +41 −0 ›` — status mark, title, a dim mono trace of the command
  filling the middle when it adds something, outcome chips on the right.
- **3+ calls:** **tracks**, in the same bordered group:

```
Now implement the CLI side.
┌───────────────────────────────────────────────────────────────────────────┐
│ 8 actions · 5 files                                                  1:52 │  header (dim; time when known)
│ Checks          ●───────────────○✕   2 passed → build failed E0425 · fixed │  checks track
│ cli.rs                ◆               touched by a script                  │  one track per file
│ api_client.rs   ● ●       ◆           read 80–400, 78–120 · script 2×      │
│ api.rs                  ■             edited at :543                       │
│ help_commands.rs             ●        searched · 3 matches                 │
└───────────────────────────────────────────────────────────────────────────┘
```

- **Columns are calls in time order**; a call that touches two files puts a bead on both tracks in its column.
- **Beads** (one 8 pt glyph per call per track): ● read · ▲ search · ■ edit (+/− in its tooltip) · ◆ script ·
  ✓ check passed · ✕ check failed (filled red) · ○ failed as planned (hollow red ring) · spinning arc = running.
  Shape carries the meaning, colour backs it (never colour alone).
- **A failed call that isn't a check** (an Edit that didn't apply, a Read of a missing file, a denied call, a
  command that exited non-zero, one cut off by Stop) draws its kind's bead with a small red ✕ mark; it counts as
  touching nothing (not "changed"), its first error line is its tooltip, and the header says "N failed". Denied and
  stopped calls draw dim ("not run").
- **More calls than fit** (the column holds about 24): the tracks show the **latest 24 calls**, with a "+M earlier"
  cell at the left; a click opens the full grid, scrolling sideways (hover, click and ←/→ work the same there).
- **Track label** (the file name; full path in its tooltip) on the left; a **one-line summary** per track on the
  right ("edited at :543 · +1 −0", "read 80–400", "touched by a script", "searched · 3 matches").
- Files are ordered: changed first (by first change), then read/searched. **More than 6 file tracks:** the top 5
  show, then a "+N files" track that opens the full list.
- **A real failure** also shows its first error line under the group in the failure ink
  (`error[E0425]: cannot find function \`call_within\` …`), so it can't hide; it stays until a later check of the same
  kind and scope passes (see 3).
- **Hover a column** → that call's readable row as a tooltip ("Ran tests cli:: · after a script on api_client.rs
  · 0:09"); **click a bead** → today's detail modal for that call. ←/→ move a column cursor when the group has focus
  (keyboard reach), Enter opens the modal.
- The running step's running column has the spinning arc; nothing else moves (reduce motion: a still arc).
- **Narrow panes (chat column under ~560 pt):** tracks fall back to readable rows.

### 3. The turn ledger (in the box's folder tab)

Merged into the agent box's folder tab (where "Done · my-app" sits today), so it costs no height:

```
 ● Working · Running tests 0:14 · 4 changed · checks 2/6     ▢ my-app   ⑂ main
```

- **Extends the existing status pill** (Working / Cultivating / Waiting / Done and its mark stay first); the ledger
  follows it. Nothing running: the status word plus counts ("✓ Done · 4 changed · checks 6/6"). While a question or
  permission is open, only the status pill shows (today's Waiting).
- Shows while a turn runs and stays after it ends until the next send: the running call (its title and elapsed),
  the count of changed files (and "+2 touched by scripts" in the popover, not the count), and the checks.
- **Whose work:** in Main's view, the whole tab — Main's calls and every subagent's (what got touched doesn't depend
  on who did it); in a lane's view, that lane's calls only. Several running at once: the most recently started one,
  plus "+N running".
- **Checks:** `checks P/T` = check runs this turn that passed / all check runs (planned failures shown apart in the
  popover, not as failing). It reads "build failing" or "tests failing" while the latest check of that kind failed
  for real. A real failure clears only when a **later check of the same kind (Tests / Build / Lint) and the same
  scope** (the same command, normalised: runner, target, filter) passes in this turn — a passing lint never hides a
  failing test. The `Check` model therefore carries `scope`.
- **Click** → a popover listing the turn: changed files with +/− (a click opens the file's diff tab — the existing
  file tab / diff view), files only read, and each check with its outcome; Esc closes.
- When it doesn't fit, it trims from the right (checks, then files), keeping the running call.
- The question dock / latest-prompt strip / closing bar keep their existing precedence; the tab ledger hides while a
  question or permission is open (the box answers that first).

### 4. "Failed as planned"

A check failure counts as planned when the step's narration (the agent text just before the group) or the call's
own description says so — a short keyword list: "red first", "(red)", "failing first", "should fail", "expect … to
fail", "TDD". Then the bead is a hollow ring and the summary says "red, as planned". Any other failure is a real
failure (filled red, first error shown). A planned failure that is followed by a passing check reads "red → green".

### 5. Row wording (small steps, hovers, narrow panes)

The agent's own description leads when it exists (Claude's Bash `description`, agy's `toolSummary`); else fuxx's
derived verb + object ("Read api_client.rs 80–400", "Edited api.rs:543", "Ran tests cli::", "Searched 2 files for
METHODS"). The command stays visible as the dim mono trace and in the modal.

### 6. Restored history

History rebuilds the same Activities from Claude's transcript (the same parsers; fuxx reads no agy history). Times
are unknown there, so headers show no time and the ledger appears only for a live turn.

## What's decided by whom

- **The user:** approach C; the lead's picks below ("go with your picks"); do it as a chore.
- **The lead's calls:** "failed as planned" from the agent's own words; the agent's description leads row wording;
  the ledger merged into the box's folder tab with a popover; built on the provider-neutral activity model ahead of
  ACP.
- **The lead's calls after the builder check** (check these while reading): failed non-check calls get a red ✕ bead
  and touch nothing; latest 24 calls with "+M earlier"; Main's ledger covers the whole tab, a lane's only itself;
  the ledger extends the status pill; checks P/T over runs, a real failure cleared only by the same kind and scope;
  script-named files are "touched", not counted as changed; Task and "Passed on" rows break a track group.
- **The designer's (from the C mockup):** the bead set, tracks only for 3+ calls, the "+N files" cap, the narrow
  fallback, the first error under the group.

## Left to the builder

- Exact sizes and tokens (follow the existing tool group: panel fill, 1 pt edge, r8, 30 pt rows; beads 8 pt; track
  rows ~22 pt) and contrast floors (add new tokens to the contrast test).
- The keyword list's exact contents, the verb table's coverage beyond the runners named, and which cargo/pytest/jest
  output shapes to fixture (from real output; no model calls).
- How the popover is anchored and its exact list layout (reuse `overlay_panel::shell`).
- Scene coverage: both themes for tracks (busy step, running step, real failure, planned failure, +N files, narrow
  fallback), the folder-tab ledger (running, done, trimmed) and its popover.

## Links

- Mockups (git-ignored): `.superpowers/brainstorm/14120-1795481670/content/tools-c-tracks.html` (and A, B, overview).
- ACP tool-call fields: agentclientprotocol.com, `tool_call_update` (`kind`, `locations`, `content`, `status`, `title`).
