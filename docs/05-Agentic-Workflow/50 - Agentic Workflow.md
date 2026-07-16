---
tags: [moc, workflow, ai]
---

# 50 - Agentic Workflow

How to use AI assistance to build fuxx-cli **without losing understanding or progress**.
This is a workflow for *delegating tasks*, not for delegating *comprehension*.

## The core stance

You are learning Rust. AI is a fast pair-programmer and a patient tutor. It can write a
milestone's code, explain a compiler error, or suggest a design — but the loop only
works if *you* end each task able to explain what changed. A codebase you can't
understand is one you can't debug when it breaks. And it will break.

See [[51 - Delegation Principles]] for the rules that keep this true.

## The loop (repeat per task)

1. **Pick one task** from a milestone in [[40 - Development Roadmap]].
2. **Write a handoff** using [[53 - Task Handoff Template]] — goal, context, done-check.
3. **Delegate** to the AI assistant.
4. **Review**: read every line, ask the AI to explain anything unclear.
5. **Verify**: run it; confirm the milestone's success check.
6. **Record**: add an entry to [[60 - Progress Log]] (what changed, what you learned,
   what's next). This is how progress survives across sessions.

## Why the log matters most

AI assistants don't remember previous sessions. The [[60 - Progress Log]] IS the memory.
If you keep it current, any new session (with any assistant) can be caught up in seconds
by reading the last entry. Skipping the log is how projects get lost.

## Related

- [[51 - Delegation Principles]] · [[52 - Known Risks and Gotchas]]
- [[53 - Task Handoff Template]] · [[54 - Session Startup Checklist]]
