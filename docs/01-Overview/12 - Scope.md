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
- **A premium sidebar** (M11r, asked for 2026-11-10 by you: "make it looks premium" → "love the D and E combined" →
  "cool do it directly here on master"; built **before M11p** on your word): an avatar per workspace with its status as a
  badge, the program's title under the name, an Open button on a waiting row, lettered group headers with the branch,
  borderless cards, a "Jump to a workspace…" button (⌘P) and a "New Fuxxpace" button. Not in it: a filter strip, a
  progress shimmer, reading the screen's last line. See [[69 - Milestone 11r - Premium Sidebar]].
- **Key sounds** (M11s, asked for 2026-11-10 by you: "disetting tambah 1 tab namanya sound…"; "New milestone, next";
  "ok setuju, lanjut kerjakan sound tab"): Settings → Sound lists Enter and Ctrl + C, each with a link (a file or a
  web link, downloaded once and kept) that plays when that key is pressed in a terminal. Not in it: other keys,
  volume, per-agent sounds. See [[70 - Milestone 11s - Key Sounds]].
- **Preview tabs** (M11t, asked for 2026-11-11 by you: "ketika single klik file apakah bisa buka sementara?…";
  "commit, push dan deploy release dulu, lalu kerjakan"): a single click in Files or Changes opens a file in the
  pane's one italic preview tab, replaced by the next single click; a double click opens it for good. Not in it:
  previews of terminals or pages, a setting. See [[71 - Milestone 11t - Preview Tabs]].
- **Restore where you were** (M11u, asked for 2026-11-11 by you: "can you check did our save session working?…";
  "ya, kerjakan dua-duanya"): each terminal's current folder and its running agent (one of your presets) are saved
  and come back on reopen; a test build keeps its own sessions file. Not in it: re-running other commands,
  scrollback. See [[72 - Milestone 11u - Restore Where You Were]].
- **A branch bar in Changes** (M11v, asked for 2026-11-11 by you: "remove it from sidebar, add it to changes tab,
  we update everything on a new milestone"; mockups B → B3; "ok B3 is good, write the milestone and plan and
  directly implement it"): the branch leaves the sidebar's group header; the Changes tab's header is one line and a
  bar at its bottom shows the branch (a click opens the branch switcher), where it stands against origin, and icon
  buttons (Fetch, and Pull, Push or Publish when there's something to do) with tooltips above them. Pull is
  fast-forward only; nothing is ever forced, merged, rebased or stashed. Not in it: merge, rebase, stash, force,
  commit, other remotes, a fetch timer. See [[73 - Milestone 11v - Branch Bar]].
  *Part 2 (2026-11-11, by you: "group this by staged and changes … revert and add … include it in M11v" → "C with
  R1, go ahead"):* the list in STAGED and CHANGES groups with check circles to stage and unstage, `N of M staged`
  with a bar, and ↺ to revert (a new file: delete) behind a confirmation every time. Not in it: committing,
  hunks, stash, a staged-only diff. *Part 3 (by you, "Part of M11v"):* Settings → General → **Status Bar** On/Off.
- **A Graph in Changes** (M11w, asked for 2026-11-12 by you: "add more accordion below changes called "Graph"" →
  "I want something like this like in cursor/IDE" → "commit and push, then start M11w"): the commit history with
  coloured lanes, branch pills, a hover card (author, date, message, files changed, hash, Open on GitHub) and header
  actions (Auto / All / Current, go to HEAD, fetch, pull, push, refresh). No new library. Not in it: checkout, revert,
  cherry-pick or reset from the graph, search, a commit's diff in a tab. See [[74 - Milestone 11w - Graph]].
- **Preferences as a panel** (M11x, asked for 2026-11-12 by you: "redesign the "Preference" window … tabs (sidebar),
  category, fields, labels, window style" → "a panel over the terminal like the screenshot"; plus "a new sound field for
  notification … this is oot but its small" → "only with a banner"): Preferences opens as a rounded panel over the
  dimmed fuxx window (no window of its own), each section a page of grouped cards; the same settings plus **Banner
  sound**, played only when fuxx shows a banner. No new library. Not in it: a search field, projects or account parts,
  other new settings. Part 2 (your ask mid-build: "beside File | Diff … add 1 more toggle called Wrap"): a Wrap button
  in a file tab's bar that wraps long lines at the tab's width. See [[75 - Milestone 11x - Preferences Panel]].
- **Say which command finished** (M11q, asked for 2026-11-10 by you, after M11p: "ok, brainstorm it, after M11p";
  design waiting for your review): while a command started at the prompt runs, the pane is named after it; when it
  ends, the banner, the 🔔 list and Activity say which command and how it ended, from the shell's OSC 133 marks.
  Only a short form (program and one subcommand) is ever shown outside the terminal. Not in it: a new Failed status,
  changing the silence guess, bash or fish integration. See [[68 - Milestone 11q - Which Command Finished]].
- **Plugins** (M11y–M11z, moved into scope 2026-11-12 by you: "yes, plugins in scope, start the design"; design
  approved 2026-11-13: "before M11p, approved, update the docs"): paste a public GitHub repo link in Settings →
  Plugins to install, one button to uninstall, and uninstall removes it completely. Every plugin is a folder with a
  `fuxx-plugin.conf` (plain `key = value`). This round: **theme plugins** (M11y, data only) and **helper-program
  plugins** (M11z, a program plus a zsh hook loaded after your `.zshrc`; fuxx-completes is the first). Not in it:
  plugins that show screens (sidebar widgets, panels, plugin tabs), fuxx drawing fuxx-completes' menu, private
  repos, automatic updates, a store, sandboxing, bash and fish hooks. See [[76 - Milestones 11y-11z - Plugins]].
- **Agent tabs and chat-style shell tabs** (M11aa–M11ab, added 2026-11-13 by you, **after M11z and after M11p**: "M11aa
  maybe? you tell me best"; designed with you in a browser-mockup brainstorm, you chose direction C, then C1: shell and
  agent are separate conversations, one kind per tab): **M11aa — Agent tabs and providers** (Part 1): picking
  **claude** (or **agy**) from the preset menu opens a workspace with a shell tab and an **agent tab** side by side;
  fuxx starts the agent itself in its machine-readable mode and draws replies, tool calls, diffs and permission
  prompts as a chat with buttons, each agent's wiring in its own provider file (`src/providers/`; Claude Code and
  Antigravity first). Typing `claude` in a shell stays the classic TUI. **M11ab — Chat-style shell tabs** (Part 2):
  each command reads like a message with its output underneath; **design first, on its own** (like M11l), not
  planned. Not in them: one mixed shell-and-agent timeline, agents on remote hosts, other providers than the first
  two. See [[Proposal - Chat View and Agent Tabs]] and [[M11aa Plan - Agent Tabs]].
- **File tab polish** (M11ac, added 2026-11-13 by you: "active file doesn't reflect/active on the file tab. If it's
  .md file, add 1 toggle button like wrap called "Preview" so that the showing .md file is an md reader with rich
  format"; your choices: "New M11ac, in parallel", "Highlight + reveal"): the Files panel follows the focused pane's
  file tab (its row selected, its folders opened, scrolled into view), and a Markdown file tab gets a **Preview**
  toggle beside Wrap that shows it rendered, read-only (remembered per tab). Not in it: editing, Mermaid or math,
  HTML in Markdown, a side-by-side source-and-preview, syntax colours in the Preview's code blocks. See
  [[78 - Milestone 11ac - File Tab Polish]].

## Out of scope (for now — resist scope creep)

- Being a general IDE / **editor**. *(Changed 2026-11-08 by you: a **read-only file viewer** is now in scope
  (the right sidebar's Files and file tabs, M11g–M11h, [[59 - Milestones 11g-11h - Right Sidebar and File Tabs]]).
  Editing, autocomplete, language servers and debugging stay out.)*
- A plugin store or marketplace, or a config *language* (scripting, conditionals) beyond the plain `key = value`
  settings file above. *(Changed 2026-11-12 by you: **plugins** installed from a GitHub link are now in scope,
  M11y–M11z above; a store, search or ratings stay out.)*
- Windows support (later).
- cmux's extras until the core is solid: **embedded browser, SSH workspaces, remote/tmux daemon** —
  all deferred (see [[40 - Roadmap]] "Later"). *(Split panes were on this list until 2026-10-27, when
  you moved them into scope as M8.)*

## The rule

If a feature doesn't serve **"run and watch AI agents in one window,"** it waits. Build the terminal +
the multi-agent ring layer first; everything cmux-extra is post-core.

## Related

- [[10 - What Is fuxx]] · [[40 - Roadmap]]
