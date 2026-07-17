---
tags: [product, scope]
---

# 12 - Scope

The line between what fuxx **is** and **is not**, so a huge project stays finishable.

> [!warning] This reverses the v1 wrapper's scope
> v1 said "if it requires emulating/rendering a terminal, it's out of scope." That was the *wrapper*
> project. fuxx-the-terminal **is** a terminal emulator — terminal emulation is now the core. The old
> boundary lives in [[90-Archive-v1-CLI-Wrapper]] and no longer governs.

## In scope (the product)

- A real terminal: render a full VT screen (via `alacritty_terminal`), run any program, handle input,
  colors, resize.
- **Multiple concurrent sessions** in one window, each a shell/agent on its own PTY.
- A **session sidebar/list** with per-session **status rings** (working / done / waiting).
- **Agent-state detection** (reuse v1's OSC-9 idea + heuristics) to drive the rings.
- macOS first (Linux capable via the Rust stack).

## Out of scope (for now — resist scope creep)

- Being a general IDE / editor.
- A plugin ecosystem, themes marketplace, config DSL.
- Windows support (later).
- cmux's extras until the core is solid: **embedded browser, SSH workspaces, remote/tmux daemon,
  split panes** — all deferred (see [[40 - Roadmap]] "Later").

## The rule

If a feature doesn't serve **"run and watch AI agents in one window,"** it waits. Build the terminal +
the multi-agent ring layer first; everything cmux-extra is post-core.

## Related

- [[10 - What Is fuxx]] · [[40 - Roadmap]]
