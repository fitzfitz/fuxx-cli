---
tags: [workflow, ai, template]
---

# 53 - Task Handoff Template

Copy this when delegating a task to an AI assistant. Filling it in forces clarity and
gives the assistant everything it needs.

```
## Task
<one specific task — e.g. "spawn a child process and print its stdout">

## Milestone
<which milestone this belongs to, e.g. [[41 - Milestone 1 - Launch and Capture]]>

## Context
- Language: Rust. I am a beginner; explain Rust concepts as you go.
- Project: fuxx-cli, a wrapper that watches an agent's output for notifications.
- Architecture model: wrapper process (NOT terminal emulation). See scope note below.
- Current state: <what already works / paste relevant code>

## What "done" looks like
<the success check — e.g. "cargo run -- echo hello prints hello back">

## Constraints
- Do NOT add terminal emulation or libghostty (out of scope).
- Keep it to this one task; don't build ahead.
- Explain every non-obvious line so I can learn it.

## After you're done
- Tell me what to run to verify.
- Summarize what changed in one paragraph for my progress log.
```

## Related

- [[50 - Agentic Workflow]] · [[54 - Session Startup Checklist]]
