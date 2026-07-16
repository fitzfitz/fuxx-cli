---
tags: [moc, home]
---

# 00 - Project Home

The central map of the **fuxx-cli** knowledge graph. Everything branches from here.

## The one-sentence pitch

A standalone Rust CLI that runs your AI coding agent as a child process, watches its
output for terminal notification sequences, and alerts you when the agent finishes or
needs input — so you can run agents in the background without babysitting them.

## The four layers of this project

- [[10 - What Is fuxx-cli]] — **Business / product**: what it is, who it's for, scope
- [[20 - Architecture Overview]] — **Architecture**: the wrapper-process model
- [[30 - Tech Stack Overview]] — **Tech**: Rust + crates + Homebrew
- [[40 - Development Roadmap]] — **Execution**: ordered milestones

## Working method

- [[50 - Agentic Workflow]] — how AI assistance fits into the build
- [[60 - Progress Log]] — where progress is recorded every session

## Key scope decision

fuxx-cli is **not** a terminal emulator and does **not** embed libghostty. It is a
process wrapper that listens to an existing terminal's output stream. See
[[23 - Why Not libghostty]] for the reasoning — this is the single most important
scope boundary in the project.

## Related

- [[11 - Relationship to cmux]]
- [[12 - Scope Boundaries]]
