# AGENTS.md — Operating Instructions for fuxx-cli

This file is the system prompt for any AI agent working in this repo. Read it fully
before doing **anything** — including asking clarifying questions or exploring files.

The **Obsidian vault in `docs/` is the single source of truth.** This file does not
restate it; it enforces it. When this file and the vault disagree, the vault wins and
you must flag the conflict to the user.

---

## The Four Hard Rules (non-negotiable)

1. **Superpowers workflow, always.** Every task runs through the Superpowers agentic
   workflow. Invoke the relevant skill *before* acting (see "Superpowers workflow" below).
   No task is exempt.

2. **Stay on the current milestone. Never work ahead.** Do exactly the one next task
   from the current milestone in [`docs/04-Development/40 - Development Roadmap.md`].
   One milestone at a time; one task at a time. Verify before moving on.

3. **NEVER suggest or assume anything outside the written plan.** Do not propose
   features, refactors, dependencies, architecture, or "nice to haves" that are not
   already written in the vault's milestones and plans. If the user asks for something
   out of scope, do not silently comply and do not improvise — **stop and say it is
   out of the written plan**, cite the note that governs it (usually
   [`docs/01-Overview/12 - Scope Boundaries.md`]), and ask whether the plan itself
   should change first. The plan in Obsidian is the base; you stick to it.

4. **Update the docs whenever a task is ready.** A task is not done until the vault
   reflects it: flip the milestone's status/checklist in its note **and** add a
   [`docs/06-Progress-Log/60 - Progress Log.md`] entry using the template in
   [`docs/06-Progress-Log/61 - Log Entry Template.md`]. The docs are part of the
   deliverable, not an afterthought.

---

## Superpowers workflow (how to run every task)

Announce "Using [skill] to [purpose]" and follow the skill exactly. Typical mapping:

- **Any creative / feature / design work** → `superpowers:brainstorming` **first**,
  before planning or code. Never jump to implementation.
- **Executing a milestone's plan** → `superpowers:writing-plans` /
  `superpowers:executing-plans`.
- **Writing any feature or fix** → `superpowers:test-driven-development` (write the
  test/success-check first).
- **Any bug, test failure, or surprise** → `superpowers:systematic-debugging` before
  proposing a fix.
- **Before claiming anything is done/fixed/passing** →
  `superpowers:verification-before-completion`. Evidence (real command output) before
  assertions, always. Rule 4 of [`docs/05-Agentic-Workflow/51 - Delegation Principles.md`]:
  verify, don't trust.

If even a 1% chance a skill applies, use it.

---

## Library docs — always use Context7

Context7 is configured for this project as an MCP server (`.mcp.json`). Use it as the
**mandatory** source for any crate/library/API documentation.

- Before adding or using **any** crate (e.g. `clap`, `notify-rust`, `portable-pty`),
  call Context7 (`resolve-library-id` → `query-docs` /`get-library-docs`) to get the
  current API and version. Do this even for well-known crates — training data drifts.
- **Never trust a remembered version number or API signature.** This is the direct fix
  for the "crate versions drift" trap in
  [`docs/05-Agentic-Workflow/52 - Known Risks and Gotchas.md`] and the version note in
  [`docs/03-Tech-Stack/33 - Key Crates.md`].
- Prefer Context7 over web search for library docs. If Context7 has no entry for a
  library, say so, then fall back to crates.io / official docs.

---

## Session lifecycle (from the vault — follow it every session)

**Startup** — do this before touching anything, per
[`docs/05-Agentic-Workflow/54 - Session Startup Checklist.md`]:

1. Read the **most recent entry** in [`docs/06-Progress-Log/60 - Progress Log.md`]. This
   is the project's memory across sessions; you have no other.
2. Note the **current milestone** and its status in the roadmap.
3. Identify the **one next task**.

**During** — per [`docs/05-Agentic-Workflow/50 - Agentic Workflow.md`] and
[`docs/05-Agentic-Workflow/53 - Task Handoff Template.md`]:

4. Frame the task with the handoff template (goal, context, done-check, constraints).
5. Do the one task. Explain every non-obvious line — the user is learning Rust; AI is a
   tutor, not a replacement for their understanding
   ([`docs/05-Agentic-Workflow/51 - Delegation Principles.md`]).
6. Verify the milestone's own success check on the real machine.

**Shutdown** — do not skip (Rule 4):

7. Update the milestone note's status/checklist.
8. Add a Progress Log entry (date, did, learned, blocked, next task).
9. Commit only when the user asks; branch first if on `master`. Commit code **and** vault
   together.

---

## Guardrails specific to this project

- **Scope boundary is the wrapper-process model, not terminal emulation.** If an idea
  requires emulating or rendering a terminal (libghostty, panes, dashboard, daemon), it
  is out of scope for v1 by definition. Guard with
  [`docs/01-Overview/12 - Scope Boundaries.md`]. "Do what cmux does" energy is the main
  threat — resist it.
- **Known traps** are already documented in
  [`docs/05-Agentic-Workflow/52 - Known Risks and Gotchas.md`] (TTY-vs-pipe, bytes-vs-
  strings, OSC variants, crate-version drift). Check there before designing around them.
- **Verify crate versions and APIs via Context7** (see "Library docs" above) — never
  trust a remembered version number; fall back to crates.io only if Context7 lacks it.

---

## What you must NOT do

- Do not skip the Superpowers workflow "to save time."
- Do not work ahead of the current milestone or bundle future work in.
- Do not introduce anything not in the written plan without the user first changing the
  plan.
- Do not mark work done without updating the milestone note and the Progress Log.
- Do not claim success without showing the command output that proves it.
