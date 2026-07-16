# fuxx-cli — Knowledge Vault

This is the Obsidian knowledge graph for **fuxx-cli**, a Rust CLI tool that watches AI
coding-agent sessions and notifies you when one finishes or needs attention — the
"notification ring" idea from cmux, built as a standalone tool.

> [!note] How to use this vault
> Open this folder in [Obsidian](https://obsidian.md) as a vault. Use **Graph View**
> (the connected-dots icon) to see the whole structure. Every note links to related
> notes with `[[wikilinks]]`, which is what makes the graph render.

## Start here — the maps of content (MOCs)

- [[00 - Project Home]] — the top-level map, read this first
- [[10 - What Is fuxx-cli]] — the business / product layer
- [[20 - Architecture Overview]] — how the tool works, high level
- [[30 - Tech Stack Overview]] — languages, crates, tooling
- [[40 - Development Roadmap]] — the ordered milestones
- [[50 - Agentic Workflow]] — how to delegate work to an AI assistant safely
- [[60 - Progress Log]] — running record so state survives across sessions

## Guiding principle

You are learning Rust. AI is a **pair and tutor**, not a replacement for
understanding your own code. Every task delegated to an AI should end with *you*
understanding what changed and why. See [[51 - Delegation Principles]].
