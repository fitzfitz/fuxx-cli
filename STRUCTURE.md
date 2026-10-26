# Repository layout

Where everything lives in this repo, and which milestone adds each source file. The plans in
`docs/04-Development/Plans/` are the source of truth; this file only maps them.

To see what the finished app will look like, open [`mockups/index.html`](mockups/index.html) in a browser.

## Today (M0 done)

```
fuxx-cli/
├── Cargo.toml            package manifest (crate name: fuxx); GPUI as a git dependency
├── Cargo.lock            exact dependency versions, incl. the resolved GPUI commit
├── src/
│   └── main.rs           M0: GPUI entry point + placeholder window
├── docs/                 Obsidian vault: the source of truth for the project (see below)
├── mockups/              static HTML mockups of the finished app (open index.html)
├── README.md             project pitch
├── STRUCTURE.md          this file
├── AGENTS.md             operating rules for AI agents working in this repo
├── CLAUDE.md             points Claude Code at AGENTS.md
├── .mcp.json             MCP servers for this repo (Context7 for crate docs)
├── .obsidian/            Obsidian settings for the docs/ vault
├── .superpowers/         Superpowers workflow scratch (task briefs, review diffs)
├── .claude/              Claude Code local settings
└── target/               build output (git-ignored)
```

## After M4 (the first usable version)

All names below are fixed by the M0–M4 plans.

```
src/
├── main.rs               M0 · window, key bindings, opens the Workspace
│
│   terminal core (one pane)
├── keys.rs               M1 · keystroke → bytes for the PTY (xterm conventions)
├── colors.rs             M1 · cell color → RGB (Alacritty palette, 256-color table)
├── grid.rs               M1 · GridSnapshot: a copy of the screen + paint batching
│                         M2 · tail_text(n), the last lines for "waiting" detection
│                         M5 · screen_tail_text (live screen), selection colour, underline_cells
├── pty.rs                M1 · spawn the login shell on a PTY; write; resize; kill on drop
│                         M5 · pty_size with real pixel sizes
├── terminal.rs           M1 · TerminalCore: alacritty_terminal + reader thread + events
│                         M2 · counts OSC 9 in the output stream
│                         M5 · input (snaps to bottom), scroll, selection, link_at, CellPx resize
├── terminal_view.rs      M1 · GPUI view: measure, paint, keys, ticker
│                         M2 · owns a StatusTracker
│                         M3 · emits TitleChanged / Exited, focusable
│                         M4 · emits StatusChanged
│                         M5 · wheel, mouse, selection, copy/paste, cmd-click links
├── mouse.rs              M5 · cell under the pointer, wheel accumulator, xterm mouse reports
├── links.rs              M5 · link under the pointer (http/https/mailto only; text and OSC 8)
│
│   agent state
├── detector.rs           M2 · OscDetector: finds ESC ]9; across chunks (restored from v1)
├── status.rs             M2 · Status (Idle/Working/Waiting/Done) + StatusTracker heuristics
│
│   many sessions
├── session_list.rs       M3 · ordered sessions + active index; switching/closing rules
├── workspace.rs          M3 · root view: sidebar, active pane, session lifecycle, actions
│                         M4 · per-session status rings, window focus, notifications
│
│   notifications
└── notifier.rs           M4 · osascript notifications, escaping, focus-aware rule
```

Unit tests live next to the code, in each file's `#[cfg(test)] mod tests`. The plans target
21 / 38 / 44 / 50 passing tests at the end of M1 / M2 / M3 / M4. The repo has 85 at the end of M5.

How the pieces connect:

```
keyboard ─► terminal_view ─► keys ─► pty ─► shell / agent
                 ▲                            │
                 │ snapshot (grid, colors)    │ bytes
                 │                            ▼
            terminal (alacritty_terminal) ◄── reader thread ── detector (OSC 9)
                 │
                 ▼
             status ─► terminal_view ─► workspace ─► sidebar rings
                                            └──────► notifier (osascript)
```

## Tester build (M4b)

```
scripts/
├── bundle-mac.sh         M4b · universal release build → fuxx.app → target/dist/fuxx-<version>.dmg
└── check-dmg.sh          M4b · the test: asserts a DMG is installable (both CPUs, macOS 11.0, version, seal)
packaging/
└── macos/Info.plist      M4b · bundle metadata (id io.github.fitzfitz.fuxx); @VERSION@ from Cargo.toml
```

To make a new tester build: bump `version` in `Cargo.toml`, run `scripts/bundle-mac.sh`, then
`scripts/check-dmg.sh target/dist/fuxx-<version>.dmg`.

## Later (Phase 2–4)

No file names yet: those milestones get detailed plans when we reach them (see
`docs/04-Development/40 - Roadmap.md`). What each one adds:

| Milestone | Adds | Mockup |
|---|---|---|
| M5 · Terminal UX | scrollback, selection and copy, mouse, clickable URLs, robust resize | [Screen 2](mockups/2-daily-driver.html) |
| M6 · Fonts & config | font family/size/ligatures; theme from Ghostty's config | [Screen 2](mockups/2-daily-driver.html) |
| M7 · Session ergonomics | project groups, rename/reorder, agent presets, session restore | [Screen 2](mockups/2-daily-driver.html) |
| M8 · Split panes | several sessions visible at once | [Screen 3](mockups/3-full-vision.html) |
| M9 · Socket/scripting API | a socket per session for scripts | [Screen 3](mockups/3-full-vision.html) |
| M10 · cmux extras | in-app browser, SSH workspaces, remote/tmux daemon | [Screen 3](mockups/3-full-vision.html) |
| M11 · Real `.app` | bundle, signing, notarization; notifications as "fuxx"; click → jump | [Screen 3](mockups/3-full-vision.html) |
| M12 · Homebrew cask | `brew install --cask` | [Screen 3](mockups/3-full-vision.html) |

## `docs/` (the vault)

```
docs/
├── 01-Overview/            Home, What Is fuxx, Features (the catalog), Scope
├── 02-Architecture/        how the pieces fit
├── 03-Tech-Stack/          crates and why; API Fact Sheet for the pinned versions
├── 04-Development/         Roadmap, one note per milestone
│   └── Plans/              step-by-step plans for M0–M4 + Plan Tracker (start here)
├── 05-Agentic-Workflow/    how to work with AI agents here; startup checklist; known risks
├── 06-Progress-Log/        one entry per session, newest first
├── 07-Terminal-App-Pivot/  why the repo became a terminal app
└── 90-Archive-v1-CLI-Wrapper/  docs for the shipped v1 CLI (history only)
```

**Picking work back up:** open `docs/04-Development/Plans/00 - Plan Tracker (Start Here).md`.

## `mockups/`

```
mockups/
├── index.html                 overview: every feature, its priority, milestone and screen
├── 1-first-usable-m4.html     end of M4: sidebar, rings, terminal, background notification
├── 2-daily-driver.html        Phase 2: scrollback, search, links, fonts, presets, restore
├── 3-full-vision.html         Phase 3–4: splits, socket API, browser, SSH, daemon, .app, Homebrew
├── style.css                  shared styles (colors and sizes from the plans)
└── mock.js                    shared behavior (switching sessions, simulated notifications)
```

These are illustrative only. They don't build or ship anything, and nothing in `src/` uses them.
