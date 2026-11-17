# Milestone 11af — Live Changes, with Who Wrote Them

**Status:** 🟨 built 2026-11-17 on branch `m11af-live-changes`, all 8 plan tasks, each reviewed clean, final-review fixes in; **merged to master 2026-11-17 (`9afbb29`), not pushed**, waiting for your on-screen check (below). Design approved 2026-11-17 ("approve"); plan [[M11af Plan - Live Changes]].

**Asked for:** 2026-11-16, by you, as near-future idea **1** of the file-view mockups ("1 and 3" → M11af and M11ag;
order "your order is fine, go ahead"). Direction: [[Proposal - File View Upgrades]], mockup
`.superpowers/brainstorm/27118-1794825628/content/file-view-future.html` (panel 1). On 2026-11-17: "alright go ahead"
(the how left to the AI).

## Why

An agent edits files while you watch, and today a file tab only shows that the file changed by reloading it: you
cannot see *which* lines just changed, or whether the agent or you changed them, without opening the diff and
comparing in your head. M11af makes the file tab answer "what did the agent just do to this file?" at a glance —
while you watch it work, and when you come back later.

Who it's for: you, watching or checking an agent (Claude or agy, in an agent tab or a plain terminal) that edits a
file you have open in a file tab. It is a **file tab** feature; the agent tab only supplies exact edit events.

## Goals

1. **Fresh lines glow.** When the file changes on disk, the lines that changed glow and cool down over about a
   minute, the newest brightest. While a line is warm, a small age ("just now", "12 s ago") shows at its end.
2. **Who changed it.** A margin mark per line changed *since the tab opened*: the agent's letter (**C** for Claude,
   **A** for agy) or your letter (**F**). The marks stay as long as the tab is open, so you can come back later and
   still see what the agent touched.
3. **Follow.** A pill in the file tab's bar, "Follow claude" / "● Following claude": while on, each new agent change
   scrolls into view; scrolling yourself turns it off.

## Non-goals and anti-goals

- **Not in it:** rewind / older versions (idea 2, not picked), the agent's explanation as a note (M11ag), any editing,
  language servers, git blame, keeping marks across a restart, glow in Diff or Preview mode, a mark for removed lines.
- **Anti-goals (a failure even if it works):**
  - The glow costs CPU when nothing is warm (M11p's idle numbers must not move; note 77).
  - A guess shown as a fact: a guessed agent mark must say it is a guess.
  - Follow jumping the view when you did not ask for it.
  - A slow reload: a big file must never stall the 1 s reload or the UI thread.

## Constraints

- Read-only, offline, no new services ([[12 - Scope]]).
- Builds on what the file tab already does (note 80; `src/file_tab.rs` module docs): the disk is checked once a
  second **only while the tab is on screen and the window is seen**; each read is a new `version`; colours and change
  bars are carried over a reload.
- Agent-specific code stays in the provider files (`src/providers/<agent>.rs`); generic code asks the provider.
- M11p's chrome caching (note 77): no new always-on timer; nothing new redrawn each frame.
- On-screen tests with `FUXX_DAEMON_SOCKET=/tmp/fuxx-dev.sock cargo run` (debug).

## Approaches

- **A. Compare the old text with the new text in fuxx (chosen; AI's pick, you handed over the how).** On each reload,
  a line diff of the text shown before against the text read now, off the UI thread. Exact for every change, including
  a second edit to an already-changed line and files outside git; runs on the existing 1 s reload. Costs one new
  dependency, `similar` (a widely used pure-Rust diff crate) — version and API checked with Context7 before the plan;
  if it is unsuitable, a small Myers line diff of our own (~150 lines) instead.
- **B. Reuse git's change bars** (a line is new when its bar appears). No new diff code, but git is asked only every
  2 s, a re-edit of an already-changed line is missed, and nothing works outside a repo. Dropped: it fails goal 1.
- **C. Watch the disk with file-system events (FSEvents) instead of the 1 s poll.** Dropped: the poll exists and is
  cheap; changing how the file tab watches the disk is a separate change with its own risks.

## The design

### 1. Finding the changed lines

- Each open file tab keeps a **line record**: for each line of the text shown, who last changed it this session
  (nobody / agent *X* / you), whether that is exact or guessed, and when.
- The first read when the tab opens is the baseline: nothing is marked.
- Each reload that gives new text (`Loaded::Text`) diffs the previous text against the new one **on the background
  executor**, as part of the read. The diff gives (a) which new lines are inserted or changed and (b) where each
  unchanged line moved, so existing marks follow their lines when lines are added or removed above them.
- A changed line gets a new entry (who, exact or guessed, `when` — see 5). Removed lines just drop out.
- **Limits (AI's calls):** no diff for a text shown plain (too large to colour, `PlainTooLarge`) or a refused file; a
  diff that takes longer than **50 ms** is given up (`similar` takes a deadline). Either way that reload marks nothing
  and **clears** the tab's marks (they could no longer be placed on the right lines). A file that goes missing keeps
  its record; when it comes back, the new text is compared with the last text seen.

### 2. The glow

- A changed line's background is tinted in a warm accent (the mockup's peach), strongest when new, fading to nothing
  over **60 s** (AI's call for "about a minute").
- **Steps, not animation:** three levels (under 15 s, under 35 s, under 60 s), as the mockup's three tints. Levels are
  worked out on the file tab's existing 1 s poll; the tab redraws only when some line's level or age label changes.
  When nothing is warm, nothing extra runs. A hidden tab has no poll; when it is shown again, levels come from `when`.
- **The age label** ("just now" under 5 s, then "12 s ago") shows small, in the accent, at the end of the **first line
  of each warm changed block** (one label per block, not per line; AI's call).
- Selection, find matches and change bars draw over the glow as they do over the change tint today.
- **File mode only.** Diff and Preview modes look as today (AI's call; M11ah redesigns Diff, M11ai Preview).

### 3. Who changed it

**The mark:** a small circle with the letter in a narrow column between the line numbers and the change bars: the
agent's in its provider colour, yours in a neutral teal (as the mockup). A **guessed** agent mark is drawn hollow
(outline and letter only), an exact one filled. Hover shows a tooltip: "claude · 12 s ago", "probably claude ·
12 s ago (it was working)", "you · 3 min ago".

**Your letter:** the first letter of `git config user.name` for the file's repo, else of the macOS account's full
name, else "Y" (AI's call).

**Deciding who (AI's proposal of 2026-11-16, kept):**

1. **Exact — an agent tab's edit.** Agent tabs already report their tool calls (`Event::ToolStarted` with
   `ToolDetail::Diff { path, .. }` for Claude's Edit and Write, then `ToolFinished`). fuxx keeps one app-wide **edit
   ledger**: (canonical path, agent, finish time, ok) for each edit tool call, kept 10 minutes. A change a file tab
   sees to path *P* is the agent's, **exact**, when an agent tab's edit to *P* finished ok since the tab's previous read
   (with 2 s of slack for the poll and the read). Any workspace's agent tab counts: the path is what matters.
   - **A finish that arrives late** (the file is written before the tool's finish reaches fuxx, so the 1 s read can
     see the change first): an edit to *P* that has **started but not finished** at the read counts as exact for now,
     and a change decided as guessed or yours is **revised to exact** if a matching edit finishes ok within 2 s after
     the read. **An edit still open 2 s after the read is settled without it** (ruling R10, final review): if no edit
     to *P* finished ok in those 2 s (it is still open — e.g. Claude waiting for your Allow — or it failed), the change
     is decided again as if that edit did not exist: a guess when an agent in the workspace was working, else yours.
     That may downgrade the mark. The line record allows that one revision; Follow acts on it (AI's call, after the
     builder check).
   - Which tool calls write which paths is the **provider's** answer (a new provider method), not a name check in
     generic code. Claude: Edit, Write, MultiEdit, NotebookEdit. agy: whatever its events give; if they carry no
     path, agy's edits fall to the guess below.
2. **Guess — an agent was working.** Otherwise, when an agent in the **file tab's own workspace** was **Working** at
   any time since the previous read, the change is that agent's, **guessed**. "An agent" is an agent tab, or a
   terminal fuxx already counts as an agent (its preset or foreground program is a known provider, as the sidebar
   label decides) — never a plain shell running a build. Two agents working: the most recently started one (AI's call).
   - **Which agents (yours, 2026-11-17: "All it recognises"):** every agent the sidebar recognises in a terminal
     (`procs::AGENTS`: claude, codex, opencode, aider, gemini, goose, amp) counts. Claude and agy get their provider's
     letter, colour and name. The others get their program's first letter in a neutral grey, always hollow (always a
     guess), named by the program ("probably codex · 12 s ago", "Follow codex").
   - **agy is added to `procs::AGENTS`**, so agy typed by hand in a shell counts too. This also makes its sidebar row
     read "agy" (a small change outside M11af, your call).
3. **Otherwise it's yours** — "you" also covers your editor, a formatter, `git checkout`. Drawn filled (it is the
   default, as in the mockup).

A file that changed while its tab was hidden is decided the same way on the next read, from the ledger and a small
app-wide record of **agent working spans** (workspace, agent, start, end; kept 10 minutes) kept next to the ledger.
The Activity log is not enough: it is per terminal, keeps 200 entries and does not say which program was running
(AI's call, after the builder check).

### 4. Follow

- A pill at the right of the file tab's bar, shown in File mode once an agent (exact or guessed) has changed the file
  in this tab: "Follow claude" (off) / "● Following claude" (on, accent tinted). Click toggles. The agent's name comes
  from the provider.
- **Off by default, never turns itself on** (AI's call: no surprise jumps).
- While on: after a reload in which the agent changed lines, if the first changed line is not in view, the view
  scrolls so it sits 3 lines from the top, as a diff opens. Your own changes never move the view.
- **Turns off** when you scroll (wheel, scrollbar, keys, a find step, a change-bar click), switch mode (Diff, or
  Markdown Preview on or off), or the tab's marks are cleared. Selecting text does not turn it off.

### 5. Times

- `when` is the moment fuxx saw the change, unless the file's mtime is earlier (a change made while the tab was
  hidden): then the mtime, so a change from 10 minutes ago is marked but does not glow.

## Calls made after the builder check (for you to check)

- A late tool finish: an edit started but not finished at the read counts as exact; a guessed or "you" mark is
  revised to exact if a matching edit finishes within 2 s after the read; an edit still open (or failed) 2 s after
  the read no longer counts, and the change is decided again without it (section 3, rule 1; final review).
- Hidden-tab changes use a new app-wide record of agent working spans, not the Activity log (section 3).
- Yours: every recognised terminal agent counts, others in grey and hollow; agy joins the sidebar's agent list.

## Left to the builder

- Where the line record and the ledger live (file tab vs a small new module), and the provider method's shape.
- The mark column's width and sizes; the three glow alphas (start from the mockup's .20 / .11 / .05).
- How the diff result is tied to `version` (as colours and bars are), so an older answer is dropped.
- Tests: the diff mapping, the deciding rules (times passed in, as `status.rs` does), glow levels and age labels are
  pure and unit-tested; ui-shots scenes for a glowing file, exact and guessed marks, Follow on and off, light and dark.

## What was built

All eight plan tasks, on branch `m11af-live-changes`. One new crate: `similar` 3.2.1 (pure Rust, no default features).

1. **`src/live_lines.rs` (Task 1).** Compare two texts (50 ms limit), the per-line record that follows its lines,
   glow levels (under 15 s, 35 s, 60 s), age words.
2. **Providers say which files a tool call writes (Task 2).** `Event::ToolStarted` carries `writes`; Claude's Edit,
   Write, MultiEdit and NotebookEdit; agy's `edit_file` and `write_file`. `agy` joins `procs::AGENTS`, so a hand-typed
   `agy` row reads `agy`.
3. **`src/edit_ledger.rs` (Tasks 3, 4).** The app-wide record of edits and working spans (10 minutes) and the rule
   that decides who made a change. Agent tabs and terminals that run a known agent feed it; a gone terminal or agent
   slot closes its state.
4. **The file tab keeps the record (Task 5).** Each reload that gives new text is compared with the last text on the
   background executor, with the read; the ledger decides who; a late finish revises a mark to exact once within 2 s,
   and a change decided while an edit was open is settled at that 2 s check (final review).
5. **Drawing (Task 6).** The glow (three tints), the age words at the end of each warm block's first line, the mark
   column (a circle and a letter, filled when exact, hollow when guessed) and its tooltip.
6. **Follow (Task 7).** The pill in the bar, off by default; scrolls the agent's newest change into view; turns off
   on scrolling, a mode switch or cleared marks.
7. **Docs and hand-off (Task 8).** This note, the roadmap, the Progress Log, `STRUCTURE.md`, the file tab's module
   docs.

## Calls made while building (AI's, for you to check)

- **Follow's place:** the change sits **3 lines from the top, as a diff opens** (`REVEAL_ABOVE`), not "about a third
  from the top" as section 4 first said. One constant if you want it lower.
- **Call ids are qualified by the agent tab's slot** ("a3:toolu_..."): tool ids are only unique per conversation, so
  two agent tabs could close each other's edit.
- **Working-span keys are namespaced:** a terminal uses its session id, an agent tab `(1<<63) | slot`; the two id
  spaces can overlap.
- **A run's open edits close as not ok when it ends** (exit, stop, close, failure): an edit left open would claim
  later changes as exact for 10 minutes.
- **`since` is the tab's previous look at the disk**, any poll including an unchanged one, not only the previous text
  read. An agent that worked minutes ago is not credited with your later edit.
- **The workspace hook** (which workspace a file tab belongs to) lives in `Workspace::show_file_tabs`, in
  `src/file_tab.rs`; the plan pointed at `src/workspace.rs`.
- **Colours:** yours is the theme's Teal; an agent with no colour of its own (codex and the rest) is the theme's Grey,
  always hollow; Claude's colour is adjusted per theme to stay readable; the age words are darkened in the light
  theme. The theme has no peach, so the glow and the words use the mockup's peach.
- **The mark column** sits between the line numbers and the change bars, as section 3 says; the mockup had it left of
  the numbers.
- **Age words under Wrap** go on the row where the first line of the block ends.

## Learned

- agy's `edit_file` and `write_file` calls **do** carry a `Path`, so agy in an agent tab gets exact marks too, not
  only a guess.
- agy's paths may start with `~` (its `write_file` into its brain folder); fuxx expands it before comparing.
- The file tab's compare trims `\r` and `\n` from each line first, so a line-ending-only change marks nothing.

## Idle CPU (the M11p anti-goal) — code-level evidence

Not measured by the builder: it needs a real window with a file tab opened by hand (your check, item 12). What the
code does:

- **No new always-on timer.** The only new timer is the one-shot 2 s revision after a guessed, "you" or open-edit
  mark; it ends within 2 s of the last such read. The final review's fixes reuse it (no new timer).
- The file tab's poll (the existing 1 s one) runs only while the tab is shown and the window is seen. Levels are
  worked out on it, and the tab redraws (`cx.notify()`) **only when a glow level or an age label changes**. With
  nothing warm, `Live::refresh` does no work.
- `render` only clones two `Rc`s and a cached flag; with no marks the geometry is exactly the old one.

## Findings

- **`tests/ui-shots-ref/` is stale on this Mac.** Master itself shows about 360 "look different" against the
  references today (the refs were made earlier on this Mac and no longer match), and so does this branch. It is a
  report, not a failure; the refs were not updated here.

## Final review (whole branch, 2026-11-17)

A whole-branch review after Task 8 found two Important issues and three minors; all fixed in one wave, each test
first (RED seen, then GREEN):

- **Follow stayed on in Markdown Preview** (Important). Preview is a flag inside File mode, so only Diff ended
  Follow; an agent change while Preview showed moved the hidden File view, and a stale mark tooltip could come back.
  Now Preview on or off ends Follow and drops the tooltip, and Follow never scrolls while Preview shows. The
  `file-follow-mode` scene does File → Preview → File too.
- **An open agent edit claimed any change as exact, never corrected** (Important, ruling R10). Claude's edit is
  "open" while it waits for your Allow; a save of yours meanwhile was marked Claude's, filled, and stayed so. Now the
  2 s check settles such a look: no ok finish in those 2 s → decided again without open edits (a guess or yours),
  which may downgrade it (section 3, rule 1). `Ledger::settle` in `src/edit_ledger.rs` (generic).
- **Age words per change block** (minor): two separate changes side by side at the same glow level shared one
  label; runs now split where the change time differs.
- **A read with no changed line** (a `touch`) no longer counts as the latest change (minor).
- **Pill wording** in Goal 3 and check item 8 now reads "● Following claude" (minor); a new Limits line for
  terminal agents seen late.

## Limits (small things you might notice)

- A mark's tooltip does not count up while you hover it ("12 s ago" stays until you move).
- A reload that changes nothing (a `touch`, a line-ending-only rewrite) within 2 s of a change can drop that change's
  pending 2 s check, so a late finish then stays as guessed, and a change claimed by a then-open edit stays exact.
- Follow with Wrap on rebuilds the wrapping on each reload.
- The poll stops while the tab is hidden, so the glow refreshes when it is shown again.
- Your letter comes from the git name known when the first mark appears, else the account's full name.
- A terminal agent whose Working begins before fuxx sees its program as a known agent (the process check is
  debounced) gets no working span for that stretch, so its edits then are marked as yours.

## Final numbers (this Mac, debug build, 2026-11-17)

| Check | Before (master, M11aa) | Now |
| --- | --- | --- |
| `cargo test --bin fuxx` | 1,787 passed (1 ignored) | **1,839 passed** (1 ignored; 1,834 before the final review's 5 new tests) |
| `cargo clippy --all-targets` | 14 (bin) / 27 (test) | 14 / 27 (no new warnings) |
| `cargo clippy --all-targets --features ui-shots` | 16 / 29 | 16 / 29 (no new warnings) |
| `cargo fmt --check` | clean | clean |
| Full `scripts/ui-shots.sh` | 137 scenes | **146 passed, 0 failed**, 115.7 s (9 new scenes; after the final review) |
| Idle CPU, file tab shown, nothing changing | n/a | not measured by the builder: your check, item 12 |

The reference fingerprints are report only and stale on this Mac (see "Findings"): they print "look different" for
most scenes on master as well, so they say nothing about this branch.

## On-screen check (yours, after the build)

Run `FUXX_DAEMON_SOCKET=/tmp/fuxx-dev.sock cargo run` (debug only: **do not add `--release`**, it writes your real
`sessions.json`) from the repo. Use a scratch folder, e.g. `~/code/scratch` (`git init` it, put a file with 40 or
so lines in it, `app.txt`, and open it in a file tab).

**The glow and the age**

1. Open an agent tab (claude) in `~/code/scratch`, ask it to change two lines of `app.txt` near the top. The file
   tab glows on those lines, the peach brightest, "just now" at the end of the first one, a filled `C` in the margin
   (between the numbers and the change bars).
2. Watch for a minute: the glow steps down (3 tints) and is gone at about 60 s; the words change ("12 s ago"...)
   and go with it. The `C` stays. Hover the `C`: "claude · 40 s ago".
3. A tab with nothing changed since you opened it looks exactly as before (no extra column, no glow).

**Who changed it**

4. In a plain terminal in the same folder, run `claude` and have it edit another line. That line gets a **hollow**
   `C`; the tooltip says "probably claude · ... (it was working)".
5. Edit another line in your own editor and save. It gets your letter (the first letter of your git name), filled;
   tooltip "you · ...".
6. If codex (or another agent) is installed: run it in a plain terminal and have it edit a line. The mark is its
   first letter, grey, hollow; "probably codex · ...".
7. In an agent tab running agy, have it edit a line: its letter `A`, filled (exact). Typing `agy` by hand in a
   terminal shows `agy` on that sidebar row.

**Follow**

8. Follow is off when the pill first appears ("Follow claude", top right of the file tab's bar, only once an agent has
   changed the file). Turn it on: "● Following claude". Have the agent edit a line far down: the view scrolls so it is
   3 lines from the top. Your own save never moves the view.
9. With Follow on, turn the wheel: it turns off. Switching to Diff turns it off too, and so does Preview on a Markdown
   file (back in File the pill says "Follow claude", the marks are still there, no tooltip left over).

**Left alone, and limits**

10. Diff and Preview look as before (no glow, no marks).
11. A file over the size limit (over 2 MB shows as plain text, over 20 MB is refused) shows no marks.
12. **Idle CPU:** with a file tab open on a file nothing is changing, compare fuxx's CPU in Activity Monitor on this
    branch with master (same window, a minute each). They should match within noise.

## Related

[[Proposal - File View Upgrades]] · [[80 - Milestone 11ae - File Tab Chores]] · [[79 - Milestone 11aa - Agent Tabs]] ·
[[77 - Milestone 11p - Chrome Caching]] · [[12 - Scope]] · [[40 - Roadmap]]
