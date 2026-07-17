---
tags: [business, scope]
---

# 12 - Scope Boundaries

The most valuable thing in this vault: an explicit line between what fuxx-cli **is** and
**is not**, so scope creep doesn't kill the project.

## In scope for v1

- Launch an AI agent as a child process
- Pass the terminal through so the agent works normally
- Watch the agent's output stream
- Detect OSC 9 / 99 / 777 notification sequences
- Fire a native macOS desktop notification on detection
- Install via a Homebrew tap

## Explicitly OUT of scope (for now)

- Terminal emulation / rendering → see [[23 - Why Not libghostty]]
- Watching multiple agents at once (that's v2, a daemon) → [[24 - Future - Daemon and Dashboard]]
- A status dashboard / TUI
- Windows / Linux support (macOS first)
- Any embedded browser or web automation
- Configuration files, plugins, themes

## The rule

If a feature idea requires emulating or rendering a terminal, it is out of scope by
definition. If it can be done by *reading the output stream of a child process*, it is
potentially in scope.

## Related

- [[10 - What Is fuxx-cli]]
- [[40 - Development Roadmap]]
- [[24 - Future - Daemon and Dashboard]]
