---
tags: [milestone, git, design]
updated: 2026-11-22
---

# 92 - Milestone 11aq - Commit

**Status:** 🟨 built 2026-11-22 (5 tasks, a whole-branch review and its fix pass), merged, **released in v0.12.3**, waiting for your on-screen check (the checklist below).

## Why

Your words (2026-11-22): "now I want to add more feature to our git tab. give me suggestions to enrich our git
workspace", the Lead UI/UX Designer's ranked suggestions, then "let's do this" for its first milestone. fuxx is where
agents change your code and you review it — but every review ends with a trip to a shell to commit. This closes that
loop in the Git tab, and lets the agent that is already here write the message.

## Goals

- **Commit from the Git tab**: a message field and a Commit button, ⌘↵.
- **✦ Write the message**: the workspace's agent writes one from the staged changes, into the field, for you to edit.
- Quiet: one new line at rest, only while there are changes.

## Not in it (and why)

- Amend, undo a commit (needs `git reset`, which fuxx never does), commit-and-push in one step, sign-off.
- Stage or revert single hunks; PR status; conflicts; "who changed it"; stash — the designer's next milestones, in
  that order (hunks first), each its own decision.
- Never: `--no-verify`, `--amend`, force, committing during a merge or rebase.

## Approaches

- **A. A field above the branch bar (chosen, the designer's pick):** review at the top, ship at the bottom, the
  field right over Push; one line at rest.
- B. A commit button that opens a modal for the message: one more step every time, and the message hides the list.
- C. Commit from a palette command only: nothing to see, hard to find.

## The design (the Lead UI/UX Designer's spec, 2026-11-22)

```
 ⌄ STAGED  2   ✓ A notes.md                 +119
 ⌄ CHANGES 4   ○ M changes_panel.rs     +544 −20
 › GRAPH
 ───────────────────────────────────────────────
  Message (⌘↵ to commit)                  ✦   ✓2
 ⑂ main                                    ⟳  ↑
   ↑1 ahead · fetched 3 min ago
```

- **The field**, just above the branch bar, shown only while the repo has changes: a placeholder "Message (⌘↵ to
  commit)", no fill and no border at rest (the bar's hairline above it), one line that grows to four.
- **Commit**: a borderless ✓ with the staged count (`✓2`), disabled with a tooltip saying why when nothing is staged,
  the message is empty, or a merge/rebase is in progress. ⌘↵ commits; Esc leaves the field; ⌘⇧C puts the keyboard in
  it. After a commit the field clears and the bar shows ↑N (Push lights). `git commit -m` with the message as you
  wrote it; staged files only.
- **A failed commit** (a hook, nothing staged after all): one red line under the field — "Commit failed · Show
  output" (the output in a read-only tab) — and the message is kept.
- **The draft** is kept per repo for the run (switching tabs or repos doesn't lose it; not saved to disk).
- **✦ Write the message**: borderless, inside the field, shown only when an agent can run (Claude or Antigravity
  installed). It runs that agent once (`claude -p` / `agy -p`, the provider's one-shot form, in the repo's folder) on
  the **staged diff only** (capped, files that look like secrets — `.env*`, keys — left out with a note), asking for a
  short conventional message. While it works the placeholder says "Writing…" and the field shows the agent box's sweep;
  Esc cancels. The text arrives selected, to edit or replace. It never commits. Failure: a faint "Couldn't write a
  message", the field untouched. Which agent: the one the right panel follows (an agent tab's provider), else Claude,
  else Antigravity. Provider details in `src/providers/<agent>.rs`.
- **Privacy**: ✦ sends staged code to that agent's provider — you press it each time; nothing is sent otherwise.

## Left to the builder

Exact sizes and colours from the panel's tokens (`GIT_*`, `changes_panel` constants); the diff cap (a size that keeps
the run quick); the prompt's wording; how the one-shot run is spawned and cancelled (off the UI thread, killed on
Esc); tests for the commit path on real temporary repos (`git_stage`'s style), and scenes for the field's states.

## Checklist for you (once built)

1. Type a message, ⌘↵: it commits the staged files only; Push lights up.
2. Nothing staged / empty message: Commit is dim and says why.
3. A failing pre-commit hook: the red line, Show output, the message kept.
4. ✦ with staged changes: a message appears selected; Esc while it writes cancels.
5. Switch repos and back: the draft is still there.

## As built (2026-11-22)

- `git_commit` (commit, `commit_checked` — refused during a merge, rebase, cherry-pick or revert at the commit itself —
  what's in progress, the staged diff without secret-looking files, cut on bytes at a line end), providers'
  `message_probe` (Claude: `-p … --output-format text --model haiku --no-session-persistence --tools ""`, no tools: it
  only writes; agy: `-p … --output-format text`), `commit_message_prompt`, `clean_message`, `on_path`,
  `agent_catalog::run_probe_stoppable`; `commit_line` (state, the line, ✦, ⌘↵).
- **Rulings** (against this design; the plan's ledger has them all): one-line field (no multi-line box yet; ✦ writes
  the summary line); "Show output" expands under the field (no text-in-a-tab yet); ⌘⇧C dropped (Copy Command Output in
  terminals); Enter does nothing in the field (only ⌘↵ commits); while ✦ writes, "Writing…" shows in ✓'s place and ✦
  stops it when pressed (the sweep on the field not built).
- **The review** (one whole-branch review): fixed a crash (the diff's cut inside a multibyte character panicked in a
  background task, which aborts the app), a commit possible during a merge started elsewhere, "Committing…" never
  showing with a message in the field, Esc going to the terminal instead of stopping ✦; Claude's ✦ run made tool-less.
  Deferred minors: closing quotes/backticks stripped by `clean_message`; ✓/✦ looking enabled in another repo while one
  repo's work runs; text typed during a commit cleared on success; the failure line staying until the next tab commit;
  `is_secret` missing `.npmrc`/`.netrc`/`.pgpass`/keystores/`*.tfvars`/`.htpasswd`; no scripted ✦ scene.
- **Plain hints** (after the build: the two designers' review for non-developers, your call "B": keep the git words,
  teach them): the field's hint "Describe your change" (short, for a narrow panel), what stops a commit on a faint
  line under the field, "Commit N" with a green ✓ when it can commit, ✦ "Let {agent} write the commit message" and a
  stop square while it writes, "Couldn't write a message · Try again", Enter's "Press ⌘↵ to commit", a failed
  commit's "Ask your agent to fix what it says, then commit again.", "↑N to push" / "↓N to pull" on the branch line,
  the bar's tooltips in two lines (git words, then plain words). The field's text lines up with the branch icon.
- Also on this branch (a chore): Stop's circle is reddish while the agent works (`stop_fill`).
- Checks: 2391 tests; clippy at the baseline; scene `changes-commit` (6 states × 2 themes); the full ui-shots run 272/272 (before and after the fix pass).

## Chores after M11aq

- **The agent tab centred on a wide screen** (your words, 2026-11-22: "the chatbox doesn't full width on larger
  screen … or make it like this but put it at center"): the conversation's column (760 at most) is centred in the tab,
  and the agent box under it is as wide as the column and centred too (it ran the full width under a column at the
  left). Scene `agent-tab-wide` (1600 wide: the box centred, as wide as the column at most, the column starting where
  the box does).
