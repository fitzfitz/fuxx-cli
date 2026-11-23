---
tags: [moc, workflow, ai]
---

# 50 - Agentic Workflow

How to use AI assistance to build fuxx **without losing understanding or progress**.
This is a workflow for *delegating tasks*, not for delegating *comprehension*.

## The core stance

You are learning Rust. AI is a fast pair-programmer and a patient tutor. It can write a
milestone's code, explain a compiler error, or suggest a design — but the loop only
works if *you* end each task able to explain what changed. A codebase you can't
understand is one you can't debug when it breaks. And it will break.

See [[51 - Delegation Principles]] for the rules that keep this true.

## The loop (repeat per task)

1. **Pick one task** from a milestone in [[40 - Roadmap]].
2. **Write a handoff** using [[53 - Task Handoff Template]] — goal, context, done-check.
3. **Delegate** to the AI assistant.
4. **Review**: read every line, ask the AI to explain anything unclear.
5. **Verify**: run it; confirm the milestone's success check. For UI work, that includes the
   screenshot gate below.
6. **Record**: add an entry to [[60 - Progress Log]] (what changed, what you learned,
   what's next). This is how progress survives across sessions.

## Checking UI work

fuxx draws its views off screen and checks them in the screenshot scenes
(`scripts/ui-shots.sh`). Each scene lists the app files it covers, so a task only needs the
scenes its change touches.

- **Per task: no screenshot scenes** (your call, 2026-11-19). Each task runs `cargo test --bin fuxx`,
  `cargo clippy --all-targets`, `cargo clippy --all-targets --features ui-shots` and `cargo fmt --check`.
  `scripts/ui-shots.sh --changed <base>` stays available for looking at one scene while working on it
  (`--png` writes the pictures to `target/ui-shots/`), but it is not part of the task's gate.
- **Once per milestone, twice:** the full `scripts/ui-shots.sh` right before the whole-branch review, and
  again after its fix wave, before the milestone is called done.
- **Any display** (2026-11-23): the scenes always shoot at 2x, whichever display is attached or main, so the
  reference fingerprints in `tests/ui-shots-ref/` hold on any Mac. `UI_SHOTS_SCALE=1 scripts/ui-shots.sh`
  checks the scenes at 1x (no fingerprints compared then; `--update-refs` refuses it).
- **The rule:** logic that doesn't need a real window (what a view would show, positions from
  geometry, labels, state transitions) goes in plain functions tested by `cargo test`. A scene
  is for what needs a window: layout, pixels, real mouse and key events, focus.

## How a milestone is built (your call, 2026-11-19)

Chosen to cut the wall-clock time per milestone ("I'd go with your 3 suggestions… do this 3 suggestions in
the next milestone"). Applies from the milestone after M11q.

1. **Fewer, bigger tasks.** A plan has about 4–7 tasks, each a whole piece a person would notice or a
   reviewer could reject on its own. Setup, scaffolding, small helpers and docs fold into the task that
   needs them; no task exists only for baselines or only for docs (baselines go in the first task, docs
   and the hand-off in the last).
2. **Built in the session, not by subagents.** After the plan is approved, the assistant in the session
   builds every task itself with `superpowers:executing-plans`: test first (`superpowers:test-driven-development`),
   the per-task gate above, a commit per task. No implementer or per-task reviewer subagents.
3. **One review at the end.** When every task is in, the full screenshot run, then one fresh reviewer on
   the most capable model available reads the whole branch against the design and the plan; one fix wave;
   a short re-check of the fixes; the full screenshot run again. Then the milestone note, the Roadmap and
   the Progress Log, and the merge.
4. **Keep the build folder small** (your call, 2026-11-23). `scripts/ui-shots.sh` ends by running
   `scripts/prune-target.sh` (it deletes the object files no current fuxx binary uses; each rebuild otherwise
   leaves a set on macOS); run it after a gate too. At a milestone's end, if `du -sh target` is over 30 GB,
   `cargo clean`. Dev builds keep only line tables (`[profile.dev]` in `Cargo.toml`); `--profile debugging`
   gives the debugger full info.

The trade: no independent review until the end, and a broken look is found at the end rather than at the
task that broke it.

## Why the log matters most

AI assistants don't remember previous sessions. The [[60 - Progress Log]] IS the memory.
If you keep it current, any new session (with any assistant) can be caught up in seconds
by reading the last entry. Skipping the log is how projects get lost.

## Related

- [[51 - Delegation Principles]] · [[52 - Known Risks and Gotchas]]
- [[53 - Task Handoff Template]] · [[54 - Session Startup Checklist]] · [[55 - Releasing]]
