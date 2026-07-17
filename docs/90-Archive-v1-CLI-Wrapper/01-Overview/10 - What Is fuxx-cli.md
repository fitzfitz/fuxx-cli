---
tags: [moc, business, product]
---

# 10 - What Is fuxx-cli

The product / business layer. Answers *what*, *who for*, and *why*.

## What it is

fuxx-cli is a command-line tool. You launch your AI coding agent through it:

```
fuxx claude       # instead of: claude
fuxx opencode     # instead of: opencode
```

fuxx-cli starts the agent as a child process, passes your terminal through to it so it
works exactly as normal, and simultaneously watches the agent's output stream. When it
detects that the agent has finished a task or is waiting for input, it fires a native
desktop notification.

## The problem it solves

When you run one or more AI coding agents, they work for minutes at a time. You either
sit and watch (wasting time) or walk away and forget (losing momentum). fuxx-cli lets
you walk away and get pinged the moment an agent needs you. This is the same core value
as [[11 - Relationship to cmux|cmux]]'s notification rings, minus the terminal-emulator
complexity.

## Who it's for

- Developers running AI coding agents (Claude Code, OpenCode, Codex, etc.)
- People who run agents in the background and multitask
- Initially: the author (you), on macOS. That's a legitimate and focused target user.

## Why it can exist as a small tool

Agents already announce their state using standard terminal escape sequences
(OSC 9 / 99 / 777). fuxx-cli only has to *listen* for signals that already exist — it
does not generate them and does not render a terminal. See
[[22 - The Output Stream Pipeline]].

## Success definition for v1

One user (you) runs an agent through fuxx-cli on macOS and reliably gets a desktop
notification when the agent finishes. That's the whole v1. Everything else is later.

## Related

- [[12 - Scope Boundaries]]
- [[40 - Development Roadmap]]
- [[20 - Architecture Overview]]
