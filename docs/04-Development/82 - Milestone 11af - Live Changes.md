# Milestone 11af — Live Changes, with Who Wrote Them

**Status:** 📝 design approved 2026-11-17 ("approve") on branch `m11af-live-changes`; plan written ([[M11af Plan - Live Changes]], 8 tasks), waiting for your review.

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
3. **Follow.** A pill in the file tab's bar, "Follow claude" / "Following claude": while on, each new agent change
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
     see the change first): an edit to *P* that has **started but not finished** at the read counts as exact, and a
     change decided as guessed or yours is **revised to exact** if a matching edit finishes ok within 2 s after the
     read. The line record allows that one revision; Follow acts on it (AI's call, after the builder check).
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
  scrolls so it sits about a third from the top (as a diff opens today). Your own changes never move the view.
- **Turns off** when you scroll (wheel, scrollbar, keys, a find step, a change-bar click), switch mode, or the tab's
  marks are cleared. Selecting text does not turn it off.

### 5. Times

- `when` is the moment fuxx saw the change, unless the file's mtime is earlier (a change made while the tab was
  hidden): then the mtime, so a change from 10 minutes ago is marked but does not glow.

## Calls made after the builder check (for you to check)

- A late tool finish: an edit started but not finished at the read counts as exact; a guessed or "you" mark is
  revised to exact if a matching edit finishes within 2 s after the read (section 3, rule 1).
- Hidden-tab changes use a new app-wide record of agent working spans, not the Activity log (section 3).
- Yours: every recognised terminal agent counts, others in grey and hollow; agy joins the sidebar's agent list.

## Left to the builder

- Where the line record and the ledger live (file tab vs a small new module), and the provider method's shape.
- The mark column's width and sizes; the three glow alphas (start from the mockup's .20 / .11 / .05).
- How the diff result is tied to `version` (as colours and bars are), so an older answer is dropped.
- Tests: the diff mapping, the deciding rules (times passed in, as `status.rs` does), glow levels and age labels are
  pure and unit-tested; ui-shots scenes for a glowing file, exact and guessed marks, Follow on and off, light and dark.

## Related

[[Proposal - File View Upgrades]] · [[80 - Milestone 11ae - File Tab Chores]] · [[79 - Milestone 11aa - Agent Tabs]] ·
[[77 - Milestone 11p - Chrome Caching]] · [[12 - Scope]] · [[40 - Roadmap]]
