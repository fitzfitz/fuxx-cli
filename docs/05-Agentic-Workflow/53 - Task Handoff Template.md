---
tags: [workflow, ai, template]
---

# 53 - Task Handoff Template

Copy this when delegating a task to an AI assistant. Filling it in forces clarity and gives the
assistant exactly what it needs.

```
## Task
<one specific task — e.g. "render the alacritty_terminal grid in a GPUI view">

## Milestone
<which milestone this belongs to, e.g. [[41 - Milestone 1 - One Terminal Pane]]>

## Context
- Language: Rust (edition 2024). I am learning; explain Rust/GPUI concepts as you go.
- Project: fuxx — a native GPU terminal for running AI coding agents.
- Stack: GPUI (app shell) + alacritty_terminal (VT core) + portable-pty. See [[30 - Tech Stack]].
- Current state: <what already works / paste relevant code>

## What "done" looks like
<the success check — and for GUI work, what I should SEE when I build & run>

## Constraints
- Stay on this one task; don't build ahead or pull in deferred features ([[12 - Scope]]).
- GUI behaviour is verified by me (you can't see the window) — tell me exactly what to run and
  what to look for.
- Explain every non-obvious line so I can learn it.

## After you're done
- Tell me what to run to verify.
- Summarize what changed in one paragraph for my progress log.
```

## Related

- [[50 - Agentic Workflow]] · [[54 - Session Startup Checklist]]
