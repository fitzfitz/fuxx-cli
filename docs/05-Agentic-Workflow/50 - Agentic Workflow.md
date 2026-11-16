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

- **Per task:** `scripts/ui-shots.sh --changed <task base>`, where the base is the commit the
  task started from. It runs the scenes that cover a changed file (a scene's own file under
  `src/ui_shots/` counts as covered by its scenes), says which and why, and runs nothing if no
  `src/` file, `assets/` file or `Cargo.*` file changed. Add `--png` when you want to look at the
  pictures (`target/ui-shots/`). A shared file (e.g. the theme, the tokens, the workspace,
  `main.rs`, the harness, `Cargo.toml`) or a `src/` file no scene covers runs everything, and the
  file is named.
- **Once per milestone:** the full `scripts/ui-shots.sh --all-themes`, before the milestone is
  called done.
- **The rule:** logic that doesn't need a real window (what a view would show, positions from
  geometry, labels, state transitions) goes in plain functions tested by `cargo test`. A scene
  is for what needs a window: layout, pixels, real mouse and key events, focus.

## Why the log matters most

AI assistants don't remember previous sessions. The [[60 - Progress Log]] IS the memory.
If you keep it current, any new session (with any assistant) can be caught up in seconds
by reading the last entry. Skipping the log is how projects get lost.

## Related

- [[51 - Delegation Principles]] · [[52 - Known Risks and Gotchas]]
- [[53 - Task Handoff Template]] · [[54 - Session Startup Checklist]] · [[55 - Releasing]]
