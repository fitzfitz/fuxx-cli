---
tags: [architecture, core]
---

# 21 - The Wrapper Process Model

The foundational idea: fuxx-cli does not *become* the agent, it *hosts* it.

## The model

When you run `fuxx claude`, fuxx-cli:

1. Treats everything after `fuxx` as the command to run (`claude` and its arguments).
2. Spawns that command as a **child process**.
3. Connects the child's input/output to your terminal so it behaves normally.
4. Also taps the child's output so fuxx-cli can read it too.
5. Waits for the child to exit, then exits with the same status code.

To the user, `fuxx claude` should feel identical to `claude` — plus notifications.

## Why a child process (not emulation)

A child process is a standard, well-understood operating-system concept. Reading a
child's output is a solved problem in every language. This is what lets fuxx-cli avoid
the entire terminal-emulator problem. See [[23 - Why Not libghostty]].

## The one real subtlety

Many interactive programs behave differently when their output is a real terminal
(a "TTY") versus a plain pipe. If fuxx-cli just captures output through a normal pipe,
the agent may disable colors or interactive features. Handling this properly may later
require a **pseudo-terminal (PTY)** so the agent still thinks it's talking to a real
terminal while fuxx-cli reads the stream. This is flagged as a known risk in
[[41 - Milestone 1 - Launch and Capture]] and [[52 - Known Risks and Gotchas]].

## Related

- [[22 - The Output Stream Pipeline]]
- [[31 - Rust Language Choice]]
- [[33 - Key Crates]]
