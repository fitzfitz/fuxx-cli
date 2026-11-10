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
│                         M5 · paste_bytes   M6 · mac_shortcut (cmd/option + arrows)
├── colors.rs             M1 · cell color → RGB (256-color table)   M6 · palette from the Theme
│
│   settings and looks (M6)
├── settings.rs           M6 · read ~/.config/fuxx/config (key = value, includes)
├── theme.rs              M6 · Theme (defaults → theme file → your values), built-in themes, UiColors
├── fonts.rs              M6 · font family/size/features; bundled icon font registered with macOS
├── app_icon.rs          M11c · the Dock icon for a build not running from the app package (`cargo run`)
├── appearance.rs         M6 · loads all of the above once at startup (GPUI Global)
│
├── grid.rs               M1 · GridSnapshot: a copy of the screen + paint batching
│                         M2 · tail_text(n), the last lines for "waiting" detection
│                         M5 · screen_tail_text (live screen), selection colour, underline_cells
├── pty.rs                M1 · spawn the login shell on a PTY; write; resize; kill on drop
│                         M5 · pty_size with real pixel sizes
│                         M11n · the reader thread only reads into an `Inbox` (1 MiB cap, backpressure); a parser
│                         thread takes up to 64 KiB at a time and parses it under one lock
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
├── session_list.rs       M3 · ordered sessions + active index; switching/closing rules   M7 · insert, move   M11b · deselect while a browser tab is focused
├── workspace.rs          M3 · root view: sidebar, active pane, session lifecycle, actions
│                         M4 · per-session status rings, window focus, notifications
│                         M7 · groups, rename, drag/move, presets, save/restore   M8 · split layouts
├── groups.rs             M7 · names (╭∩╮ default, M11c), group labels/rows, rolled-up ring, ordering rules
├── name_input.rs         M7 · the rename text box (emoji / IME input)
├── presets.rs            M7 · agent presets (claude / opencode / codex + agent-preset lines)
├── restore.rs            M7 · session-restore setting, sessions.json format   M8 · layouts
├── split_tree.rs         M8 · nested split tree: split, close, neighbours, ratios, sanitize
├── panes.rs              M8 · draws a layout: nested panes, dividers, dimming   M11i · rounded cards, 6 pt gaps as grab strips, the focus ring
│
│   scripting (M9)
├── api.rs                M9 · protocol: requests/replies, targets, key names, wait conditions
├── api_server.rs         M9 · private socket: same-user check, caps, per-connection threads
├── api_client.rs         M9 · the client side the `fuxx` command and `fuxx mcp` use
├── api_events.rs         M9 · event JSON and the wait rules (pure)
├── cli.rs                M9 · `fuxx <command>` parsing, output and exit codes
├── mcp.rs                M9 · `fuxx mcp`: Model Context Protocol server on stdio
├── workspace_api.rs      M9 · the workspace answering requests, waits, events, note/progress
│
│   persistent sessions (M10a)
├── vt_snapshot.rs        M10a · a terminal's screen and modes as bytes that rebuild it (for reattaching)
├── mux_proto.rs          M10a · the window ↔ background service wire format: frames, decoder, versions
├── daemon.rs             M10a · `fuxx daemon`: the service, its socket, session table, idle exit, notifications
├── daemon_session.rs     M10a · one session the service owns: shell, screen, numbered output, status
├── daemon_client.rs      M10a · the window's side of the service socket; starts the service detached
├── private_socket.rs     M10a · private-socket helpers shared by M9's socket and the service
├── workspace_daemon.rs   M10a · the workspace using the service: connect, reopen, lost service, quit count
├── adopt.rs              M10a · reopen rules: live sessions into their saved places, the rest restored (pure)
│
│   SSH workspaces (M10b)
├── ssh_target.rs         M10b · the `ssh` command line from argv: host check, reuse/keepalive options, tmux name
├── ssh_hosts.rs          M10b · hosts from `~/.ssh/config` (Include, no wildcards), recent hosts, `ssh -G` lookup
├── ssh_control.rs        M10b · the private shared-connection folder and the health check before reuse
├── workspace_ssh.rs      M10b · the workspace's SSH sessions: open, hold on disconnect, reconnect, restore
├── host_picker.rs        M10b · the ⇧⌘S host picker: rows, filter, keys, tmux tick box
│
│   agent browser (M10c)
├── browser_url.rs        M10c · browser addresses: http(s) only, scheme added to bare hosts (pure)
├── browser_js.rs         M10c · page-side scripts (snapshot with refs, click, fill, press, read, wait) and the outline
├── browser.rs            M10c · one WebKit window per session (objc2): load, eval, screenshot, Edit keys, ⌘W   Follow-up · keep-data store choice and wipe
├── browser/bar.rs        Follow-up · the compact top bar: back/forward/reload, address field, loading line, 🖕 mark (objc2)
│   └── browser/probe.rs  M10c · hidden `fuxx __browser-probe` self-test: a real off-screen web view, end to end
├── workspace_browser.rs  M10c · the session ↔ browser map, ⇧⌘B address box, `browser.*` socket methods   M11b · browser tabs per workspace, Pop Out, agents drive a tab
│
│   UI uplift (M11)
├── ui_tokens.rs          M11 · sizes, text scale and the WCAG contrast maths behind UiColors (pure)
├── ui_shots.rs           M11 · off-screen screenshots (feature `ui-shots`): `cargo run --features ui-shots -- __ui-shots`
├── status_mark.rs        M11 · the drawn status marks (ring, arc, pulsing dot, check), pop curve, ticker rule
├── reduce_motion.rs      M11 · macOS "Reduce motion": read at start, refreshed by its change notification
├── sidebar.rs            M11 · the sidebar: row model, rows, headers, scroll, resize, footer, hint card, drag
├── switcher.rs           M11 · workspace facts for the palette's Workspaces: urgency order, the row, ⇧⌘U's next-needing-you (M11j: the ⌘P panel itself is gone)
├── palette.rs            M11j · the ⌘K command palette: the overlay, chips, list, picks (⌘P opens it on Workspaces)
├── palette_items.rs      M11j · scopes, keys, Recent, every action and setting, `compose` into the list's lines (pure)
├── palette_match.rs      M11j · the fuzzy matcher: word starts, runs, gaps, a file-name bonus (pure)
├── palette_files.rs      M11j · the palette's files: git ls-files or a bounded walk, ranked off the UI thread
├── welcome.rs            M11k · the Welcome card: folder, agents, three shortcuts; opening it on a first run or from Help; the pick
├── welcome_items.rs      M11k · the card's rules: rows, the default row, what a pick opens and closes, the copy (pure)
├── agent_detect.rs       M11k · is a preset's program on your login shell's PATH? (pure core; one capped background read)
├── shell_marks.rs        M11l · OSC 133 mark scanner over raw bytes (A, B, C with the command, D with the exit code; pure)
├── blocks.rs             M11l · command tags kept as hidden hyperlinks on the grid; `advance` splits at each mark; list, visible, output text, prompt_ready
├── shell_integration.rs  M11l · installs the zsh scripts, `ZDOTDIR` env for a login zsh, the `command-blocks` switch
├── block_view.rs         M11l · the view's pure rules: jump, rerun bytes, chip labels, width and right-prompt limit, colours
├── overlay_panel.rs      M11 · the shared panel and filter box (switcher, host picker, address box)
├── sidebar_toggle.rs     M11m · the sidebar button at the sidebar band's right edge, or at the band's far right when collapsed: position rules, slide curve
├── tab_menu.rs           Follow-up · the right-click row menu: actions (close others / to the right), bulk-close text, overlay
├── tab_color.rs          Follow-up · the 11 tab colours and their saved names
│
│   workspaces, pane tabs and the in-pane browser (M11b)
├── layout_model.rs       M11b · workspaces → panes → tabs: every rule (new, split, close cascades, move, roll-up, pinning, ⌘1–9, old-file loader), GPUI-free
├── tab_bar.rs            M11b · the tab bar over each pane (replaces pane headers): tabs, marks, ×, buttons, drag, scroll   M11i · 30 pt, tabs merge into the pane, +, ⋯, sidebar toggle
├── drag_gap.rs           M11b · Task 7b: where a dragged tab or row lands (the gap nearest its copy), the others' offsets, the 150 ms ease-out slides; GPUI-free
├── pane_tab_menu.rs      M11b · a pane tab's right-click menu: Close Tab, Close Other Tabs in Pane, Rename Tab…, Pop Out
├── pane_browser.rs       M11b · the web view placed over a pane each frame, and the rule for hiding it behind fuxx's own menus
│   └── pane_browser/probe.rs  M11b · hidden `fuxx __pane-browser-probe` self-test: placing, hiding, keys, focus, a real off-screen page
├── browser_tab.rs        M11b · a browser tab: the GPUI bar, address field, suggestions, page keys (⌘R / ⌘[ / ⌘] / zoom)
├── browser_history.rs    M11b · visited pages for the address suggestions; kept only with "Keep history and data" On
├── git_branch.rs         M11b · a workspace's folder (OSC 7 or the shell's cwd) and git branch for the row's second line, cached off the UI thread
├── notification_list.rs  M11b · this run's notifications for the 🔔 popover: newest first, unseen dot, closed terminals greyed
├── title_area.rs         M11m · the app title, the band's 🔔 ⚙ and sidebar buttons when collapsed, the workspace name, the popover from the footer or the band
│
│   settings window
├── prefs.rs              M6b · the window's choices (themes, fonts, size, ligatures), GPUI-free
├── preferences.rs        M6b · the Preferences window   M7 · Restore sessions row   Follow-up · vertical section tabs
│
│   notifications
├── native_notify.rs      M11c · fuxx's own banners (UserNotifications, app bundle only), banner ids (terminal + run), the click delegate, PendingShow
├── notifier.rs           M4 · picks fuxx's banner or the osascript one (M11c), escaping, focus-aware rule
│
│   the Help menu (M11d)
├── help_menu.rs          M11d · the Help menu, which window a Help item goes to, NSApp's Help menu (macOS search), the status sheet's copy, the issue URL
├── help_panel.rs         M11d · the panel over the window: Keyboard Shortcuts (⌘/) and the Command Reference pages, filter, scroll
├── help_shortcuts.rs     M11d · the shortcut list from the real key bindings: names, groups, Mac key labels, search (pure)
├── help_commands.rs      M11d · the command reference parsed from `fuxx help`'s text, plus the MCP setup row (pure)
├── app_log.rs            M11d · the window's ~/Library/Logs/fuxx/fuxx.log when run from fuxx.app (1 MiB cap at launch)
│
│   updating itself (M11e)
├── update_release.rs     M11e · versions, GitHub's latest-release JSON, the offer/sheet copy, the swap script's text, the checks (pure)
├── highlight.rs          M11g · syntect + two-face: a syntax theme built from the fuxx theme (3:1 floor), file classification (pure)
├── git_status.rs         M11g · repo root, status marks, ignore checks, per-file line changes (git, capped, last good kept)
├── branch_list.rs        M11o · the branch switcher's model: rows, marks (current, local only, ↑↓), filter, typed-name row, conflict count, copy (pure)
├── branch_git.rs         M11o · the switcher's git: refs, dirty files, files a branch would change, fetch (auto and button), switch (background, capped)
├── branch_switcher.rs    M11o · the branch switcher modal: list, box, Fetch, confirm step, switch, refresh after; opened from the row's branch, the status bar, ⌘K
├── file_tree.rs          M11g · the Files panel's lazy tree model: rows (cached), marks, filter, keyboard selection (pure)
├── right_sidebar.rs      M11g · the right sidebar: toggle, pane icon, resize, follows the focused terminal, the Files panel   M11i · type icons, guides, root header, collapse all
├── procs.rs              M11i · the `ps` table and `lsof` listening ports (absolute paths, capped, background), parsing, what runs in a terminal and the stable rule (pure)
├── pane_actions.rs       M11i · the ⋯ menu on a pane's tab bar: New Terminal Tab, New Browser Tab, Split Right, Split Down
├── status_bar.rs         M11i · the status bar: status word, branch, folder, port chips, the update pill; fitting by drop order (pure core)
├── diff_view.rs          M11h · a file's whole diff against the last commit as rows, the dimmed inks, the copy (pure)
├── changes_panel.rs      M11h · the right sidebar's Changes panel: files changed since the last commit, counts, click → diff
├── activity.rs           M11h · each terminal's activity log: status changes, notes, progress (in memory, last 200; pure)
├── activity_panel.rs     M11h · the right sidebar's Activity panel: header, the waiting question, the timeline
├── file_tab.rs           M11g · read-only file tabs: colours, line numbers, change bars, live reload, find, selection, Open in Editor
├── cursor_look.rs        M11f · the cursor's rules: shape (the HollowBlock marker = no program has chosen), colour, breathing, glide, trail, sparks, frames (pure)
└── updater.rs            M11e · the check (curl, at launch + daily + the menu), the Updates state, the install (download, verify, swap, reopen)
```

Unit tests live next to the code, in each file's `#[cfg(test)] mod tests`. The plans target
21 / 38 / 44 / 50 passing tests at the end of M1 / M2 / M3 / M4. The repo has 85 at the end of M5, 106 at the end of M6, 150 after M6b + M7 + M8, 243 after M9, and 399 after M10a.

`assets/fonts/` holds the bundled icon font, JetBrains Mono (the default font, M6b) and their licences; `assets/themes/` the six built-in themes and their credits. `assets/shell-integration/zsh/` the zsh scripts for command blocks (`.zshenv`, `fuxx-integration.zsh`; M11l), compiled into the binary.

Example `~/.config/fuxx/config` (all optional; fuxx never writes it):

```
font-family = MesloLGS NF
font-size = 13
theme = Catppuccin Mocha        # or light:Catppuccin Latte,dark:Catppuccin Mocha
background = #1e1e2e            # overrides the theme
font-feature = -calt            # ligatures off
```

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
├── bundle-mac.sh         M4b · universal release build → fuxx.app (with its icon, M11c) → target/dist/fuxx-<version>.dmg;
│                         --app-only, --build <target> (one slice), --prebuilt (bundle built slices)
├── make_app_icon.py      M11c · the 3D Cyber logo → packaging/macos/fuxx.icns + the browser bar's mark (run by hand)
├── check-dmg.sh          M4b · the test: asserts a DMG is installable (both CPUs, macOS 11.0, version, seal)
├── check-release-tag.sh  M11e · refuses a release tag that isn't v + Cargo.toml's version
└── bench/                M11n · the benchmark harness: run.py (isolated fuxx and Ghostty copies; CPU, throughput,
                          launch, memory), make_data.py, wininfo.swift; it backs up and restores Ghostty's prefs
.github/workflows/
└── release.yml           M11e · test + two slices in parallel (cached) → lipo → fuxx.app → zip; publishes on a v* tag,
                          tests and builds on code pushes to master (keeps the cache warm)
packaging/
├── macos/Info.plist      M4b · bundle metadata (id io.github.fitzfitz.fuxx, icon fuxx); @VERSION@ from Cargo.toml
└── macos/fuxx.icns       M11c · the app icon (made by scripts/make_app_icon.py)
```

To make a new tester build: bump `version` in `Cargo.toml`, run `scripts/bundle-mac.sh`, then
`scripts/check-dmg.sh target/dist/fuxx-<version>.dmg`.

To release: see `docs/05-Agentic-Workflow/55 - Releasing.md` (bump `version` in `Cargo.toml`, commit, push a tag
`v<version>`; GitHub Actions builds and publishes it, and every installed fuxx offers it through Update Available).

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
| M11 · UI/UX uplift | polish of look and feel; scope decided when we get there | — |
| M12 · Real `.app` | bundle, signing, notarization; notifications as "fuxx"; click → jump | [Screen 3](mockups/3-full-vision.html) |
| M13 · Homebrew cask | `brew install --cask` | [Screen 3](mockups/3-full-vision.html) |

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
