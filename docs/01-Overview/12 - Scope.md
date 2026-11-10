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
  notarization and Homebrew stay later (M12, M13).
- **Updating itself** (M11e, added 2026-11-08 by you): GitHub Releases built by GitHub Actions on a version tag,
  and an "Update Available" button in the sidebar that downloads, swaps and reopens fuxx. No Developer ID needed.
  See [[57 - Milestone 11e - Update Available]].
- **A cursor with style** (M11f, added 2026-11-08 by you): a bar cursor that glides, breathes, follows the agent's
  status colour, with optional trail and sparks; set in Preferences. See [[58 - Milestone 11f - Cursor Style]].
- **A right sidebar and read-only file tabs** (M11g–M11h, added 2026-11-08 by you): Files (tree with git markers),
  Changes (since the last commit, with diffs) and Activity for the focused terminal; files open read-only in pane
  tabs with syntax colours (syntect + two-face), line numbers and change bars. See
  [[59 - Milestones 11g-11h - Right Sidebar and File Tabs]].
- **A modern layout, a command palette, a first-run screen and command blocks** (M11i–M11l, added 2026-11-08 by you,
  "I love all 1-11"): smart names, compact rows, a subtle selection, a richer Files tree, floating panels, terminal
  padding, a status bar and a cleaner tab bar (M11i); a ⌘K palette for actions, workspaces, files and settings (M11j);
  a first-run screen (M11k); Warp-style command blocks (M11l). Each is designed and planned on its own.
  *(M11l as designed 2026-11-09, AI's calls: blocks come from OSC 133 marks sent by fuxx's own zsh integration,
  injected through `ZDOTDIR` without touching your dotfiles; copy and rerun are in; **collapse is out** (alacritty
  can't fold rows), and bash and fish aren't injected. See [[63 - Milestone 11l - Command Blocks]].)*
- **Title bar** (M11m, added 2026-11-09 by you, after M11l; designed and planned 2026-11-09 in [[64 - Milestone 11m - Title Bar]]): with the sidebar open, the sidebar icon sits at the right edge of the sidebar's top band beside the centred title "the Fuxx?" (🔔 and ⚙ stay in the footer); collapsed, the title hides, the workspace name is centred in the top band and 🔔, ⚙ and the sidebar button sit at the far right.
- **Performance** (M11n, added 2026-11-09 by you, after M11m; built 2026-11-10, see [[65 - Milestone 11n - Performance]]): fuxx idles at about 6 % CPU (the cursor's breathing
  glow redraws 20×/s; about 0.9 % with it off) against Ghostty's 0.2 %, and drains plain output about 2.7× slower
  (local benchmark, 2026-11-09). Built 2026-11-10 and measured: idle with the glow breathing about 0.77× before, about 0.9 % after five minutes untouched, `cat` of 100 MiB 1.42× Ghostty's time. The one combined release (M11i–M11n, v0.5.0) was released 2026-11-10; a chores round is planned with you next. Caching the window chrome for a cheaper breath was not in M11n; it is M11p (below, added 2026-11-10).

- **Small changes after v0.5.0** (2026-11-10, by you): no grey line above prompts (note 63, "After release"); in
  a git repo the sidebar row shows its branch and its folder on two lines, no branch line outside one (note 60,
  "After release"); the release build sped up (2026-11-10, by you: "yes we can do both of these"): the Release
  workflow caches cargo's work and builds the Apple Silicon and Intel halves in parallel; it also runs (test and
  build, no publish) on master pushes that touch code, which keeps the cache warm and checks every push.
- **A branch switcher** (M11o, asked for and approved 2026-11-10 by you: "oke setuju, lanjut tulis plan-nya"): a
  modal to switch the git branch of a terminal's folder: local branches with ahead/behind or "local only", origin's
  branches not yet local, a box to type one from origin, a disabled "conflict x file(s)" row where a switch would
  collide, a confirmation every time, a background fetch on open and a Fetch button. fuxx's first action that
  changes a repo; it never stashes, resets, forces or creates a branch from scratch. See
  [[66 - Milestone 11o - Branch Switcher]].
- **Chrome caching** (M11p, added 2026-11-10 by you, after M11o: "oke, masukkan ke rencana setelah M11o. selama
  sidebar masih mendapatkan update terkini, saya lanjut"): the window chrome (sidebar, tab bars, status bar, title
  area) is split into cached GPUI views that redraw only when their own data changes, so the breathing cursor stops
  re-rendering them on every frame. Same look and behaviour; **the sidebar and every chrome part must always show
  current information**. Starts with a prototype that measures the saving, then a design with you. Not in it: a new
  renderer, repainting only the cursor on its own layer, or changing the cursor's look. See [[40 - Roadmap]].

## Out of scope (for now — resist scope creep)

- Being a general IDE / **editor**. *(Changed 2026-11-08 by you: a **read-only file viewer** is now in scope
  (the right sidebar's Files and file tabs, M11g–M11h, [[59 - Milestones 11g-11h - Right Sidebar and File Tabs]]).
  Editing, autocomplete, language servers and debugging stay out.)*
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
