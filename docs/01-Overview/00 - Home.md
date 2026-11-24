---
tags: [moc, home]
---

# 00 - Home

Top-level map for **fuxx** — a native terminal app for running AI coding agents, built in Rust.

## The one-paragraph pitch

You run many AI coding agents. fuxx is a GPU-accelerated terminal that manages them in one window:
each agent session gets a pane and a **notification "ring"** that lights up when it finishes or needs
you, with at-a-glance status across all sessions. Claude Code and Antigravity can also run in **agent
tabs**, a chat fuxx draws itself (tool calls, diffs, questions, subagents, what each step touched). Like
cmux — but the terminal *you* build, in Rust.

## The maps

- ▶ **[[00 - Plan Tracker (Start Here)]]**: where the build stands, the next action, open decisions
- [[10 - What Is fuxx]] — product & vision
- [[11 - Features]] — the full feature catalog (phased)
- [[12 - Scope]] — in / out of scope
- [[20 - Architecture Overview]] — how it's built
- [[30 - Tech Stack]] — GPUI, alacritty_terminal, portable-pty · exact APIs: [[31 - API Fact Sheet (pinned rev)]]
- [[40 - Roadmap]] — every phase and milestone, M0 → M13 (each with its note; plans in `04-Development/Plans/`)
- [[70 - Terminal App Pivot — Vision and Stack]] — the pivot decision record
- [[50 - Agentic Workflow]] · [[60 - Progress Log]]
- **Outside the vault (repo root):** `STRUCTURE.md` (repo layout, which milestone adds each file) ·
  `mockups/index.html` (illustrative HTML mockups of M4, Phase 2 and the full vision; open in a browser)

## Status

Pivoted from the shipped v1 CLI wrapper (see [[90-Archive-v1-CLI-Wrapper]]). Repo repurposed.
Released as **v0.13.1** (2026-11-24). Current milestone: [[93 - Milestone 11ar - Subagents]] and its chores,
waiting for your on-screen check. See [[00 - Plan Tracker (Start Here)]] and [[40 - Roadmap]].
