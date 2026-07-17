# AGENTS.md — Operating Instructions for fuxx (terminal app)

This file is the system prompt for any AI agent working in this repo. Read it fully before doing
**anything** — including asking clarifying questions or exploring files.

**This repo was repurposed (2026-07-17).** It shipped **fuxx-cli v1** (a Rust CLI wrapper; on
`origin/master`, docs under `docs/90-Archive-v1-CLI-Wrapper/`) and now builds **fuxx**, a native
GPU terminal app for running AI coding agents. The **Obsidian vault in `docs/` is the source of
truth** for the current project; this file enforces it. When they disagree, the vault wins — flag it.

---

## The Four Hard Rules (non-negotiable)

1. **Superpowers workflow, always.** Every task runs through the Superpowers agentic workflow.
   Invoke the relevant skill *before* acting. No task is exempt. (See "Superpowers workflow" below.)

2. **Stay on the current milestone. Never work ahead.** Do exactly the one next task from the
   current milestone in [`docs/04-Development/40 - Roadmap.md`]. One milestone at a time; verify
   before moving on.

3. **NEVER suggest or assume anything outside the written plan.** Do not add features, dependencies,
   architecture, or "nice to haves" not in the vault's plan. If the user asks for something out of
   scope, **stop, say it's out of the written plan, cite the governing note**
   (usually [`docs/01-Overview/12 - Scope.md`]), and ask whether the plan itself should change first.

4. **Update the docs whenever a task is ready.** A task isn't done until the vault reflects it: flip
   the milestone's status/checklist **and** add a [`docs/06-Progress-Log/60 - Progress Log.md`] entry.
   The docs are part of the deliverable.

---

## Superpowers workflow (how to run every task)

Announce "Using [skill] to [purpose]" and follow the skill exactly.
- **Creative / feature / design work** → `superpowers:brainstorming` **first**, before code.
- **Executing a milestone** → `superpowers:writing-plans` / `superpowers:subagent-driven-development`.
- **Any feature or fix** → `superpowers:test-driven-development` where it applies. (Note: GPU/GUI
  behavior often can't be unit-tested — verify those manually, and say so, like v1's `vim`/banner checks.)
- **Any bug/surprise** → `superpowers:systematic-debugging` before proposing a fix.
- **Before claiming done/fixed/passing** → `superpowers:verification-before-completion`. Evidence
  (real command output, or a user-confirmed screenshot for GUI) before assertions.

If even a 1% chance a skill applies, use it.

## Library docs — always use Context7

Before adding or using any crate (GPUI, `alacritty_terminal`, `portable-pty`, `cosmic-text`, …),
consult **Context7** (`resolve-library-id` → `query-docs`) for current APIs/versions; fall back to
crates.io / the source repo if Context7 lacks it. **Never trust a remembered version or API.** GPUI
is pre-1.0 and consumed as a **pinned git dep** — verify the rev. See [`docs/03-Tech-Stack/30 - Tech Stack.md`].

## Session lifecycle (from the vault)

- **Startup:** read the latest [`docs/06-Progress-Log/60 - Progress Log.md`] entry; note the current
  milestone in the roadmap; identify the one next task.
- **During:** frame the task, do it, explain non-obvious Rust/GPUI as you go (still a learning
  project), verify on the real machine.
- **Shutdown:** update the milestone note + Progress Log; commit only when the user asks (branch
  first if on `master`).

## Guardrails specific to this project

- **Scope is a terminal for AI agents, not a general IDE.** In: a real terminal (via
  `alacritty_terminal`), multi-session panes, a sidebar, notification rings, agent-state detection.
  Out (for now): embedded browser, SSH, remote/tmux daemon, split panes, config DSL, Windows — all
  deferred. Guard with [`docs/01-Overview/12 - Scope.md`]. The threat now is "add every cmux feature
  at once," not terminal emulation.
- **The old v1 scope is reversed and archived.** Terminal emulation IS the product now. The notes
  under `docs/90-Archive-v1-CLI-Wrapper/` (incl. "Why Not libghostty" / "Scope Boundaries") describe
  the *wrapper* project and no longer govern.
- **macOS notifications:** use `osascript`, not `notify-rust` (v1 finding — see memory
  `macos-notify-rust-broken`).
- **Verify crate versions/APIs via Context7** (above); GPUI rev must be pinned.

## What you must NOT do

- Skip the Superpowers workflow "to save time."
- Work ahead of the current milestone, or pull deferred cmux extras in early.
- Introduce anything not in the written plan without the user changing the plan first.
- Mark work done without updating the milestone note + Progress Log.
- Claim a GUI feature works without a user-confirmed observation (you can't see the window).
