# Repository layout

Where everything lives in this repo, and which milestone (or chore) added each source file. The vault in `docs/`
is the source of truth (the roadmap, a note per milestone, the plans in `docs/04-Development/Plans/`); this file only
maps it. Last checked against `src/` at v0.13.1 (2026-11-24).

The planning-day mockups are in [`mockups/index.html`](mockups/index.html) (illustrative only; see the end of this file).

## Top level

```
fuxx-cli/
├── Cargo.toml            package manifest (crate name: fuxx, version); GPUI as a pinned git dependency;
│                         dev builds keep line tables only (`--profile debugging` for full debug info)
├── Cargo.lock            exact dependency versions, incl. the resolved GPUI commit
├── src/                  the app, one focused module per file (below)
├── assets/               fonts, themes, icons/lucide (the icon set), logos (Claude, Antigravity), logo (fuxx's own),
│                         badges, shell-integration/zsh, vendor/mermaid; compiled into the binary where used
├── examples/plugins/     sunset-themes (a theme plugin) and hello-helper (a helper-program plugin)
├── tests/ui-shots-ref/   the screenshot scenes' reference fingerprints
├── scripts/              bundling, release checks, screenshots, build-folder pruning, benchmarks, fixtures (below)
├── packaging/macos/      Info.plist and the app icon
├── .github/workflows/    release.yml (below)
├── docs/                 Obsidian vault: the source of truth for the project (see below)
├── mockups/              static HTML mockups from the planning days (open index.html)
├── README.md             what fuxx does, install, keys, plugins
├── STRUCTURE.md          this file
├── AGENTS.md             operating rules for AI agents working in this repo
├── CLAUDE.md             points Claude Code at AGENTS.md
├── .mcp.json             MCP servers for this repo (Context7 for crate docs)
├── .obsidian/            Obsidian settings for the docs/ vault
├── .superpowers/         Superpowers workflow scratch (git-ignored)
├── .claude/              Claude Code local settings
└── target/               build output (git-ignored; `scripts/prune-target.sh` trims it)
```

## `src/` (every module, and the milestone that added it)

Grouped by area; within a group, roughly in the order they arrived.

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
├── ui_shots/             M11 · off-screen screenshots (feature `ui-shots`)   M11aj · split by area: `mod.rs` the runner, `harness.rs`, `registry.rs` (areas, `ORDER`), one file per area
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
├── agent_catalog.rs      M11ak · what an agent says about itself (models, commands, usage), read by short probe processes per agent and folder; no timer
├── agent_box.rs          M11ak · the agent tab's box: the pure rules (a choice, when it applies, chip texts, usage and list rows) and the box's drawing
├── agent_box_menus.rs    M11ak · the box's menus: the folder picker and its panels, model, effort, usage, mode, attach (a submodule of agent_box.rs)
├── folder_picker.rs      M11ak · the folder picker's pure part: Recent (up to 8), the folders fuxx knows, the rows for a search, which folders are there (checked off the UI thread)
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
├── tab_bar.rs            M11b · the tab bar over each pane (replaces pane headers): tabs, marks, ×, buttons, drag, scroll   M11i · 30 pt, tabs merge into the pane, +, sidebar toggle   chores after M11ar · the + opens `new_tab_menu`; Split Right / Split Down buttons replace the ⋯
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
├── status_bar.rs         M11i · the status bar: status word, branch, folder, port chips, the update pill; fitting by drop order (pure core)
├── diff_view.rs          M11h · a file's whole diff against the last commit as rows, the dimmed inks, the copy (pure)
├── changes_panel.rs      M11h · the right sidebar's Changes panel: files changed since the last commit, counts, click → diff
├── activity.rs           M11h · each terminal's activity log: status changes, notes, progress (in memory, last 200; pure)
├── activity_panel.rs     M11h · the right sidebar's Activity panel: header, the waiting question, the timeline
├── file_tab.rs           M11g · read-only file tabs: colours, line numbers, change bars, live reload, find, selection, Open in Editor
├── live_lines.rs        M11af · compare two texts, the per-line record (who changed it, exact or guessed, when) that follows its lines, glow steps, age words (pure)
├── code_outline.rs      M11ah · the outline of a file's blocks (indentation, Markdown headings, named lines): guides, folds, sticky line, breadcrumb, card titles (pure)
├── row_map.rs           M11ah · the one map of screen rows (hidden lines, card titles, folded stretches, side-by-side pairs, Wrap): y ↔ line, next change (pure)
├── diff_cards.rs        M11ah · Diff mode's cards: grouping with context, titles and counts, folded unchanged stretches, changed-word marks, side-by-side pairing (pure)
├── edit_ledger.rs       M11af · the app-wide record of agents' file edits and working spans, and the rule that decides who made a change (pure)
├── mermaid.rs            M11ac · Mermaid in the Markdown Preview: which blocks, limits, cache key (SHA-256) and LRU, the page and render script (pure)
├── mermaid_render.rs     M11ac · the hidden WKWebView that renders Mermaid offline and snapshots it to PNG; the `Mermaids` cache global
├── cursor_look.rs        M11f · the cursor's rules: shape (the HollowBlock marker = no program has chosen), colour, breathing, glide, trail, sparks, frames (pure)
├── updater.rs            M11e · the check (curl, at launch + daily + the menu), the Updates state, the install (download, verify, swap, reopen)
│
│   chrome, icons and small shared parts
├── chrome_cache.rs       M11p · the window chrome as cached parts (sidebar, status bar, tab bars, title area) and render counters
├── mark_view.rs          M11p · the moving status marks in their own small views, so their ticker doesn't redraw the chrome
├── icons.rs              chore 2026-11-20 · Lucide's outline icons (lucide-static 0.460.0), embedded and served to GPUI's `svg()`
├── logos.rs              chores after M11ab · the Claude and Antigravity marks, drawn in their own colours
├── scrollbar.rs          M11ae · the overlay scrollbar of a file tab (File, Diff, Preview)
├── sounds.rs             M11s · key sounds: which key a press is, links (file or web, downloaded once), `NSSound` playback
├── test_temp.rs          chore after M11ar · scratch folders for tests and scenes in `$TMPDIR/fuxx-t/<pid>`, removed at exit
│
│   shell tabs: which command, cards (M11q, M11ab)
├── command_name.rs       M11q · a command line's short form (program + one subcommand), the only part that leaves the terminal
├── workspace_command.rs  M11q · what the prompt's command does to a session: its name while it runs, how it ended, once
├── card_view.rs          M11ab · a shell tab as message cards painted from their grid lines, the live terminal boxed at the bottom
│
│   git in the Changes tab (M11v, M11w, M11aq)
├── branch_sync.rs        M11v · the branch bar: where the branch stands against origin (local git only), Pull / Push / Publish
├── git_stage.rs          M11v · stage, unstage and revert (`--literal-pathspecs`), and the panel's words about them
├── git_graph.rs          M11w · the GRAPH's git: commits to draw, a commit's details, its GitHub link (off the UI thread, capped)
├── graph_layout.rs       M11w · which lane each commit sits in and how its row's lines run (pure)
├── graph_panel.rs        M11w · the GRAPH accordion: lanes, branch pills, hover card, scope and refresh
├── git_commit.rs         M11aq · commit the staged files, what blocks a commit, the staged diff ✦ writes from (no secret-looking files)
├── commit_line.rs        M11aq · the commit line above the branch bar: message field, ✦, ✓N, ⌘↵, a failed hook's output
│
│   file tabs, more (M11ac–M11ai, M11ag)
├── md_preview.rs         M11ac · a Markdown file shown rendered, read-only, as a GPUI list of blocks
├── md_reader.rs          M11ai · the reader's pure half: front matter, callouts, badge rows, heading slugs, "On this page"
├── file_tab_preview.rs   M11ac/M11ai · the Preview's body, Mermaid blocks, links and scrollbar handlers (moved out of file_tab.rs)
├── code_ask.rs           M11ag · what a file-tab selection becomes as a chip and as the text the agent gets (pure)
├── file_tab_ask.rs       M11ag · the bar under a selection: "Ask claude ⌘↵", "Copy with path"
├── edit_notes.rs         M11ag · the agent's own reason for an edit, kept to show under the changed lines
├── file_tab_notes.rs     M11ag · those notes drawn under their edit: shaped once, wrapped, rows in the row map
│
│   plugins (M11y, M11z, M11au)
├── plugin_manifest.rs    M11y · a plugin's `fuxx-plugin.conf`: parse it, then check the files it names
├── plugin_source.rs      M11y · GitHub links, GitHub's JSON answers, what to download (no network)
├── plugin_fetch.rs       M11y · download, unpack and check a plugin: the only plugin code that touches the network
├── plugin_store.rs       M11y · where plugins live, the installed record, the only code that deletes plugin files
├── plugin_themes.rs      M11y · the theme names installed plugins bring, and the config line an uninstall takes out
├── plugins.rs            M11y · the installed plugins as Settings sees them; install, update, uninstall off the UI thread
├── preferences_plugins.rs M11y · Settings → Plugins: the link field with Install, the list of installed plugins
├── plugin_shell.rs       M11z · `plugins.zsh`, sourced after your `.zshrc`, built from the enabled helper plugins
├── plugin_settings.rs    M11au · declared plugin settings: the `setting.` lines, your values, which ones are exported
│
│   agent tabs (M11aa onward)
├── providers/mod.rs      M11aa · fuxx's own event language for an agent conversation; knows no agent by name
├── providers/claude.rs   M11aa · Claude Code's stream-json to events and back (recorded fixtures in providers/fixtures/)
│   └── providers/claude/activity.rs   activity view · Claude's tool calls as activities (files, `Exit code N`, tool errors)
├── providers/agy.rs      M11aa · Antigravity's (`agy`) stream-json to events and back
│   └── providers/agy/activity.rs      activity view · agy's tool calls as activities (exit header, a lane's `toolSummary`)
├── agent_process.rs      M11aa · runs an agent as a piped child (own session and process group), streams its lines
├── agent_conversation.rs M11aa · the conversation model: folds a provider's events into chat items, lanes and a status (pure)
├── agent_tab.rs          M11aa · the agent tab: process → translator → conversation → a GPUI list of rows; the box, keys, flows
├── agent_rows.rs         M11ar · an item's row, tool rows, diffs, the permission card (moved out of agent_tab.rs)
├── agent_markdown.rs     M11aa · the markdown subset agent messages use (also the Markdown Preview's parser; pure)
├── composer.rs           M11aa · the message box: multi-line input with IME, Enter sends, ⇧Enter newline, Esc stops; pills
├── workspace_agent.rs    M11aa · agent tabs in the workspace: launching a preset as shell + agent tab, closing, Open in Classic View
├── agent_chat.rs         chores after M11ab · the chat's layout rules: rows, gaps, tool groups, the avatar, "Thinking…"
├── agent_top.rs          chores after M11ab · the top bar's words: status pill, cost, the live context meter
├── agent_tool_modal.rs   chores after M11ab · a tool row's detail modal, a stack walked with ↑/↓
├── agent_sweep.rs        chore 2026-11-22 · the box's one-time comet sweep round its rim
├── usage_strip.rs        M11ap · the status bar's usage strip: per agent, its tightest limit used and when it resets
├── usage_view.rs         M11ap · one look for usage rows (the box's usage panel, the status bar's popover)
│
│   subagents (M11ar)
├── agent_lanes.rs        M11ar · which view is shown, the pills' labels, stepping between Main and lanes (pure)
├── agent_lanes_view.rs   M11ar · the pill strip, following agy's subagent transcripts, ⌥⌘[ / ⌥⌘], the closing bar
├── lane_follow.rs        M11ar · the bytes a followed file gained, cut into whole lines (pure)
├── agent_lane_row.rs     chore 2026-11-24 · a subagent's row in Main's chat: its mark, an outlined robot, name, task
│
│   chores after M11ar (v0.12.7 to v0.13.1; note 93, "Chores after M11ar")
├── chat_select.rs        selecting and copying chat text: drag, double/triple-click, ⇧-click, ⌘C, ⌘A
├── new_tab_menu.rs       the pane bar's + menu (buttons, agent presets, search) and what ⌘T makes
├── mode_memory.rs        modes an agent refused on this Mac, so a new tab starts in the mode it ran in
├── prompt_history.rs     ↑/↓ in the box: the tab's own prompts, then the folder's (a JSONL file, per folder, capped)
├── agent_latest.rs       your latest message pinned over the chat while it is scrolled away; a click jumps back
├── agent_rewind.rs       Rewind and Fork from your messages (Claude), Edit for agy, Esc Esc to pick
├── context_menu.rs       the one right-click menu every surface builds its items in (pure)
├── workspace_menu.rs     that menu as the workspace opens, draws and acts on it (⇧F10 from the keyboard)
├── files_menu.rs         the Files tree's right-click actions: Rename / New boxes, Move to Trash, ⌘Z
├── link_route.rs         where a clicked link opens ("Open links in": a split on the right, a tab, the default browser)
├── auto_open.rs          a program's own browser opening lands like a clicked link (`fuxx link open`, the open shims)
├── agent_pills.rs        files as pills in the box (dragged from Files or Finder), chips in the sent message
├── agent_cancel.rs       taking back a message the agent hasn't read (Claude's `cancel_async_message`)
├── agent_ask.rs          the agent's question card: what it shows and what each key does (pure)
├── agent_ask_view.rs     the question card drawn in the chat, its keys, the answers sent back
│
│   the agent activity view (v0.13.1)
├── agent_activity.rs     one provider-neutral `Activity` per tool call (kind, files, first error, check, exit)
├── agent_activity_command.rs  reading a shell command: files read, searched or changed, checks and their scope (pure)
├── agent_activity_checks.rs   a check's outcome from its output (cargo, pytest, jest / vitest, go test; pure)
├── agent_steps.rs        the step model: a narration line and the tool calls after it (pure)
├── agent_step_words.rs   the step model's words: headers, track summaries, outcome chips (pure)
├── agent_tracks.rs       a busy step's file tracks: a header, a checks track, a bead per call per file
├── agent_ledger.rs       the turn ledger in the box's folder tab and its popover (pure)
└── fixtures/checks/      recorded check outputs the activity readers are tested on
```

Unit tests live next to the code, in each file's `#[cfg(test)] mod tests`. The plans target
21 / 38 / 44 / 50 passing tests at the end of M1 / M2 / M3 / M4. The repo has 85 at the end of M5, 106 at the end of M6, 150 after M6b + M7 + M8, 243 after M9, 399 after M10a, and 2852 at v0.13.1 (2902 with the `ui-shots` feature).

`tests/ui-shots-ref/` holds the screenshot scenes' reference fingerprints (M11aj C3, report only; `scripts/ui-shots.sh --update-refs` writes them).

`assets/fonts/` holds the bundled icon font, JetBrains Mono (the default font, M6b) and their licences; `assets/themes/` the six built-in themes and their credits. `assets/shell-integration/zsh/` the zsh scripts for command blocks (`.zshenv`, `fuxx-integration.zsh`; M11l), compiled into the binary. `assets/vendor/mermaid/` the bundled mermaid.js (12.2.0, MIT; its LICENSE and a README with the source and SHA-256; M11ac Part 2), compiled into the binary.

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
             status ─► terminal_view ─► workspace ─► sidebar marks, tab marks, status bar
                                            └──────► notifier (fuxx's banner, osascript fallback)

agent process (claude / agy, stream-json) ─► providers/<agent>.rs ─► Event ─► agent_conversation
       ▲                                                                       │ items, lanes, activity
       └──────── composer (your message) ◄── agent_tab ◄───────────────────────┘ ─► rows, tracks, ledger
```

## Scripts, release and packaging

```
scripts/
├── bundle-mac.sh         M4b · universal release build → fuxx.app (with its icon, M11c) → target/dist/fuxx-<version>.dmg;
│                         --app-only, --build <target> (one slice), --prebuilt (bundle built slices)
├── make_app_icon.py      M11c · the 3D Cyber logo → packaging/macos/fuxx.icns + the browser bar's mark (run by hand)
├── check-dmg.sh          M4b · the test: asserts a DMG is installable (both CPUs, macOS 11.0, version, seal)
├── check-release-tag.sh  M11e · refuses a release tag that isn't v + Cargo.toml's version
├── ui-shots.sh           M11aj · the screenshot suite in parallel shards, then its serial scenes; `--changed <base>`;
│                         ends by running prune-target.sh
├── prune-target.sh       chore 2026-11-23 · deletes fuxx's stale object files from target/ (none a current binary uses)
├── record-agent-fixture.py  M11aa · records a real headless claude / agy session as a scrubbed JSONL fixture
├── generate_logos.py     the logo suite: multi-resolution PNGs from the master SVGs (run by hand)
├── test_logo_assets.py, test_make_app_icon.py   checks for the two logo scripts
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

## Still to come

No file names yet: these get designs and plans when we reach them (see
`docs/04-Development/40 - Roadmap.md`).

| Milestone | Adds |
|---|---|
| M11ad · Background agents | agent tabs follow "Keep running after quit": the daemon hosts the agent processes |
| M11am · Agent tab rework | a placeholder, after a thorough talk with you |
| M11an · Full-width typing line | the shell composer's first line at full width |
| M11ao · One chat box with add-ons | one box shared by shell and agent tabs |
| M11as · Agent browser · M11at · Browser testing | fuxx's browser tools in every agent tab; console, network, screenshots |
| ACP client | one generic ACP provider (agy first, then Codex, Gemini, opencode) |
| M12 · Real `.app` | signing and notarization with a Developer ID |
| M13 · Homebrew cask | `brew install --cask` |

## `docs/` (the vault)

```
docs/
├── 01-Overview/            Home, What Is fuxx, Features (the catalog), Scope
├── 02-Architecture/        how the pieces fit
├── 03-Tech-Stack/          crates and why; API Fact Sheet for the pinned versions
├── 04-Development/         Roadmap, one note per milestone
│   └── Plans/              a step-by-step plan per milestone + Plan Tracker (start here)
├── 05-Agentic-Workflow/    how to work with AI agents here; startup checklist; known risks
├── 06-Progress-Log/        one entry per session, newest first
├── 07-Terminal-App-Pivot/  why the repo became a terminal app
├── superpowers/            design specs and plans written with the Superpowers workflow (specs/, plans/)
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

These are illustrative only, from before M4. They don't build or ship anything, nothing in `src/` uses them, and the
app has long since moved past them.
