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
- **Agent-state detection** (reuse v1's OSC-9 idea, extended to OSC 99 / OSC 777 / BEL, + heuristics)
  to drive the rings. The shell's environment is inherited as-is; stripping it, or documenting an
  agent setting as a workaround, was considered and kept out (2026-10-26).
- **fuxx's own settings file** (added 2026-10-27 on request): `~/.config/fuxx/config`, plain `key = value`
  lines using Ghostty's key names, plus theme files in `~/.config/fuxx/themes` and six built-in themes.
  fuxx does **not** read Ghostty's config.
- **JetBrains Mono bundled as the default font** (M6b, added 2026-10-27 on request), so ligatures work without
  installing anything.
- A **settings window** (M6b, added 2026-10-27 on request): File → Preferences… / cmd-, to pick the theme,
  font, size and ligatures and override colors, saved to fuxx's own settings file and applied live.
- **Session ergonomics** (M7): project groups, rename/reorder, agent presets, session restore.
- **Split panes** (M8, moved into scope 2026-10-27 on request): nested splits, each pane its own session.
- macOS first (Linux capable via the Rust stack).
- A **tester build** after the MVP (M4b, added 2026-10-26): an unsigned `.app` in a DMG. Signing,
  notarization and Homebrew stay later (M11, M12).

## Out of scope (for now — resist scope creep)

- Being a general IDE / editor.
- A plugin ecosystem, themes marketplace, or a config *language* (scripting, conditionals) beyond the
  plain `key = value` settings file above.
- Windows support (later).
- cmux's extras until the core is solid: **embedded browser, SSH workspaces, remote/tmux daemon** —
  all deferred (see [[40 - Roadmap]] "Later"). *(Split panes were on this list until 2026-10-27, when
  you moved them into scope as M8.)*

## The rule

If a feature doesn't serve **"run and watch AI agents in one window,"** it waits. Build the terminal +
the multi-agent ring layer first; everything cmux-extra is post-core.

## Related

- [[10 - What Is fuxx]] · [[40 - Roadmap]]
