# fuxx — Knowledge Vault

This is the Obsidian knowledge graph for **fuxx**, a native, GPU-accelerated **terminal app for
running AI coding agents** — in the spirit of cmux, built in Rust.

> [!note] Project history
> This repo began as **fuxx-cli v1**, a Rust CLI *wrapper* (PTY host + OSC 9 detection + macOS
> notification) — shipped and on `origin/master`. As of 2026-07-17 the project **pivoted** to a
> standalone terminal app and the repo was **repurposed**. The v1 wrapper docs are preserved under
> [[90-Archive-v1-CLI-Wrapper]]; the v1 code remains in git history.

## Start here — maps of content

- [[00 - Home]] — top-level map, read this first
- [[10 - What Is fuxx]] — the product / vision
- [[11 - Features]] — full feature catalog (phased)
- [[12 - Scope]] — what the terminal is and is not
- [[20 - Architecture Overview]] — GPUI + alacritty_terminal + sessions + rings
- [[30 - Tech Stack]] — GPUI, alacritty_terminal, portable-pty, rendering
- [[40 - Roadmap]] — the ordered milestones (M0 → M4)
- [[70 - Terminal App Pivot — Vision and Stack]] — why we pivoted and how we chose the stack
- [[50 - Agentic Workflow]] · [[60 - Progress Log]] — how we work + running memory (carried over from v1)

## Guiding principle

Still a learning project in Rust: AI is a **pair and tutor**, not a replacement for understanding
your own code. Ship a real, usable terminal, one small milestone at a time.
