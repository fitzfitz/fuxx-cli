# fuxx

A native, GPU-accelerated terminal for running AI coding agents.

Run several coding agents at once in a single window. Each session shows a live status —
**working**, **done**, or **waiting for input** — so you can step away and get pulled back exactly
when an agent needs you, instead of babysitting a wall of terminal windows.

## What it does

- **A real terminal** — GPU-rendered, runs any shell or program.
- **Workspaces, panes and tabs** — the sidebar lists workspaces (rows show `branch • folder`, and a Needs input badge
  when an agent asks you something); each workspace is a split layout of panes, and each pane has its own tab bar of
  terminals and web pages. See the next section.
- **Notification rings** — a per-session status badge plus desktop notifications when a background
  session finishes or needs input, and never a nag about the session you're already looking at.
- **Groups, presets, restore, split panes** — workspaces per project folder, one-key agent presets,
  your layout back after a restart, nested splits.
- **Scriptable, and agent-aware** — the `fuxx` command and an MCP server (see below).
- **Keep running after quit** (opt-in) — turn on Preferences → Sessions → Keep running after quit (or
  `persistent-sessions = on` in `~/.config/fuxx/config`) and your sessions stay alive in a small background
  service when you quit; reopening brings them back live. `fuxx daemon status` shows it, `fuxx daemon stop`
  (add `--force` to end running sessions) stops it; ⌥⌘Q quits and ends everything.

## SSH sessions

Press ⇧⌘S (File → New SSH Session…) to pick a host from your `~/.ssh/config` or recent hosts, or type one. The
pane runs your own `ssh`, so your keys, config, jump hosts and 2FA work as in any terminal; nothing is installed
on the host. Tick "Keep running on the host (tmux)" to run inside tmux there, which reconnects by itself after a
network drop; otherwise Enter reconnects. From a script: `fuxx new --ssh HOST [--tmux]`. If you used the background
service before this version, restart it once with `fuxx daemon stop`. Built and reviewed, not yet tried on screen.

## Workspaces, pane tabs and the browser in a pane

The sidebar lists **workspaces**. A workspace is a layout of split panes, and every pane has a tab bar. A tab is a
terminal or a web page. Built and reviewed, not yet tried on screen.

- **New:** **⌘N** makes a new workspace (one pane, one terminal). **⌘T** adds a terminal tab to the focused pane.
  **⌘D** / **⇧⌘D** split the focused pane right / down. **⌘W** closes the focused tab; closing a pane's last tab closes
  the pane, and a workspace's last pane closes the workspace (closing a workspace with two or more terminals asks first).
- **Moving around:** **⌘1…⌘9** pick a workspace, **⇧⌘]** / **⇧⌘[** go to the next / previous one, **⌃Tab** / **⌃⇧Tab**
  cycle the tabs of the focused pane, **⌘[** / **⌘]** and **⌥⌘ arrows** move between panes, **⌘K** opens the command palette (actions, workspaces, files, settings), **⌘P** opens it on workspaces
  (most urgent first), **⇧⌘U** jumps to the one that needs you, **⌘\** hides the sidebar.
- **Rows:** a bold name, `branch • ~/folder` underneath (the branch only inside a git repo), and, when an agent waits, its
  question and a 🔔 **Needs input** badge. The selected row is filled with the accent colour. Right-click a row to
  **Pin** it: pinned workspaces sit at the top with a 📌. Rename Workspace…, Workspace Color and the closes are in the same menu.
- **Tabs:** each tab shows an icon, its title, a status mark for terminals, and a × on hover. The buttons at the bar's
  right add a terminal tab, add a browser tab, split right and split down. Right-click a tab for Close Tab, Close Other
  Tabs in Pane, Rename Tab… and (pages) Pop Out. **Drag tabs anywhere:** to reorder, into another pane's bar, or onto a
  workspace in the sidebar to move the tab there. Drag the empty part of a tab bar at the top of the window to move the window.
- **The browser in a pane:** a browser tab has a bar (back, forward, reload/stop and an address field) over a real web
  view. Type in the field for suggestions from your visited pages (when **Keep history and data** is On) and a last
  "Search Google for …" row. **⌘L** focuses the field. While a web page is in front, fuxx's own menus and boxes hide it
  so nothing disappears behind it. Inside a page: **⌘R** reloads, **⌘[** / **⌘]** go back / forward, and **⌘=** /
  **⌘−** / **⌘0** zoom the page; **⌘W**, **⌘T**, **⌘N**, **⌘D**, **⌘1…⌘9**, **⌘P**, **⇧⌘U** and **⌃Tab** still do
  fuxx's thing.
- **Footer:** the 🔔 beside the Preferences gear lists this run's notifications (a dot while some are unseen; click one
  to jump to that terminal). The **+** opens the new-workspace menu. With the sidebar hidden, the footer is hidden too and
  the workspace name shows at the top.
- **Restore:** workspaces, panes, tabs, pins and pages' addresses come back after a restart.
- **Resizing** the window no longer makes an idle terminal look busy.

## Agent browser

Browsers now live in a pane (see above). `fuxx browser open …` from an agent opens a browser tab in the agent's own
workspace, in a new pane to the right of its terminal (the next `open` reuses it). Right-click a browser tab → **Pop Out**
moves the page into a window of its own, and press ⇧⌘B (File → Open Browser…) to open an address in the focused
workspace's browser tab. A browser window is a plain WebKit view (no
tabs) with a compact top bar like Safari's: back, forward and reload/stop buttons, an address field (type an address and
press Enter; Escape puts the current one back; **⌘L** focuses it), a thin loading line, and the fuxx mark. The bar's light
or dark follows your fuxx theme; web pages still follow macOS. Only `http://` and `https://` addresses are opened
(`localhost:3000` gets `http://` added). Agents drive the page: `fuxx browser open|snapshot|click|fill|press|read|eval|wait|screenshot|url|back|reload|close`,
or the matching `browser_*` tools through `fuxx mcp`. `snapshot` lists the page's links, buttons and fields with
refs (`e3`) that `click` and `fill` take. Safari's Web Inspector can attach (Develop menu). Pages in a pane come back after a restart on their last address; popped-out windows aren't restored.

Two settings in Preferences → Browser: **Show address bar** (On by default; Off keeps the buttons and the page title)
and **Keep history and data** (Off by default: logins and cookies live in memory for one run; On saves them for next
time; turning it Off deletes what was saved). Config keys: `browser-address-bar = on|off`, `browser-keep-data = on|off`.
Built and reviewed, not yet tried on screen.

## What's new in the look

The window is now the app: no grey titlebar, padding around the terminal, and colours that hold up in light
themes too. Each session shows a shaped status mark: a hollow ring (idle), a spinning arc (working), a pulsing dot
(waiting) and a dot with a check that pops once (done). A waiting session also shows the question it's asking under
its name. New shortcuts: **⇧⌘U** jumps to the session that needs you, **⌘K** searches everything and **⌘P** switches workspaces (most urgent first),
**⌘\** hides the sidebar, **⌘=** / **⌘−** / **⌘0** change the font size. Split panes each get a tab bar.
Preferences (⌘,) has sections down the left (General, Appearance, Sessions, Browser) and remembers the last one; Appearance has theme swatches and a live preview, General has **Animations** On/Off (`animations = off` in the config; macOS
"Reduce motion" is respected too) and "Show the tips card again". A sidebar button beside the window buttons hides and shows the sidebar (same as ⌘\). Rows in the sidebar behave like tabs: hover shows a × to close, middle-click closes, and right-click opens a menu (Close Workspace, Close Other Workspaces, Close Workspaces Below, Show All Workspaces, Rename Workspace…, Pin, Workspace Color); closing several at once always asks first. Rows show ⌘1…⌘9 on the right. Screenshots of every screen, in a dark and a light theme, come from
`cargo run --features ui-shots -- __ui-shots` (PNGs in `target/ui-shots/`). Built and reviewed, not yet tried on screen.

## The 2026 look (M11i)

A new terminal is called `╭∩╮` until you rename it (or a program gives it a title). Sidebar rows are two compact lines with a soft tinted selection. Panes
and the right sidebar float as rounded cards with small gaps; the focused split has an accent edge. The tab bar has
only **+**, **⋯** (New Terminal Tab, New Browser Tab, Split Right, Split Down) and the right-sidebar toggle. The Files
tree has coloured file icons, indent guides, a root header and collapse all. A **status bar** along the bottom shows
the focused terminal's status, branch and folder, the ports its programs listen on (click `:3000` to open it in a
browser tab), and Update Available when there is one.

## Scripting fuxx

Inside any fuxx session, the `fuxx` command talks to the running app (outside, put the binary on your `PATH`):

```sh
fuxx ls                                    # terminals: id, number, status, name … (--json adds workspace, pane, kind)
fuxx new --preset claude --split right --background   # prints the new id, e.g. s4 (no flag: a new workspace; --tab --of s2: a tab)
fuxx wait s4                               # until the agent needs you (also --for done|exit, --text, --any)
fuxx question s4                           # what it's asking
fuxx answer s4 "yes"                       # answer it (refused unless it's waiting)
fuxx events                                # live stream of status changes, opens, closes …
fuxx read s4 --lines 20 · fuxx send s4 "make test" --enter · fuxx note "building" · fuxx progress 40
```

Agents can drive fuxx too: `claude mcp add fuxx -- fuxx mcp` (use the full path to the binary if `fuxx`
isn't on your `PATH`). `fuxx help` lists every command; `scripting = off` in `~/.config/fuxx/config`
turns it off. Details: `docs/04-Development/49 - Milestone 9 - Scripting API.md`.

## See it

Open [`mockups/index.html`](mockups/index.html) in a browser for illustrative mockups of the finished
app: the first usable version (M4), the daily driver (Phase 2) and the full vision (Phase 3–4), with
every planned feature labeled by milestone. [`STRUCTURE.md`](STRUCTURE.md) maps the repo layout.

## Built with

Rust · [GPUI](https://github.com/zed-industries/zed).

## Install and updates

Download `fuxx-<version>.zip` from [Releases](https://github.com/fitzfitz/fuxx-cli/releases/latest), unzip it and move
`fuxx.app` to Applications. fuxx isn't signed with an Apple Developer ID yet, so macOS asks once: open it, then
System Settings → Privacy & Security → **Open Anyway**. After that fuxx updates itself: when a newer version is out,
a blue **Update Available** button appears above the sidebar's footer, and one click downloads it, checks it and
reopens fuxx. **fuxx → Check for Updates…** checks at any time; Preferences → General turns the daily check off.

## The right sidebar and file tabs

**⌥⌘\\** (or the icon at the right of any pane's tab bar) opens a right sidebar showing the focused terminal's repo:
a file tree with git marks (**M** modified, **A** added, **D** deleted), a filter, and keyboard navigation. Click a
file to open it as a **read-only tab** in the pane, with syntax colours, line numbers and change bars against the
last commit; it updates live while an agent edits it. ⌘F finds; "Open in Editor" opens it in your text editor (it
never runs a file). fuxx never changes your files.

## Command blocks (M11l)

In a zsh terminal fuxx marks each command with a thin bar in the left padding (red when it failed) and a hairline
between commands. Hover a bar for a one-row chip with **Copy command**, **Copy output** and **Rerun** (Rerun types
the command at an empty prompt and never presses Return). **⇧⌘↑** / **⇧⌘↓** jump to the previous / next command and
**⇧⌘C** copies the chosen command's output. fuxx finds commands through a small zsh integration it injects with
`ZDOTDIR`: your dotfiles are not touched, and it works with powerlevel10k. Turn it off with `command-blocks = off`
in `~/.config/fuxx/config` (or Preferences → Sessions); new terminals only. bash and fish aren't injected, and
collapsing a block isn't supported. Built and reviewed, not yet tried on screen.

## The first launch (M11k)

The very first time fuxx opens, a Welcome card asks for a folder and an agent (your presets, each checked on your
PATH) and shows three shortcuts: **⌘K** the command palette, **⌘N** a new workspace, **⇧⌘U** the agent that needs you.
Esc skips it. Help → **Welcome to fuxx** brings it back any time.

## Changes and Activity (M11h)

The right sidebar has three tabs: **Files**, **Changes** and **Activity**. Changes lists the files that differ from
the last commit (**M**/**A**/**D** with `+added −removed` counts); click one to open it in **diff mode**: removed
lines red, added lines green, both line numbers, syntax colours on. A **File | Diff** switch in the tab's toolbar
flips between the file and its changes, and clicking a change bar in a file jumps to the diff. Activity shows the
focused terminal's timeline (Working, Needs input, Done — 11 min), the question it's waiting on, and the agent's
`fuxx note` / `fuxx progress` lines.

## The cursor

The terminal's cursor is a thin bar by default that glides to where it goes, breathes softly instead of blinking,
and takes the colour of what the agent in that terminal is doing (working, waiting for you, done). Preferences →
Appearance → Cursor changes the shape (Bar, Underline, Hollow, Block), turns each effect off, or adds a Fun one
(a comet trail, or sparks as you type). Programs that set their own cursor, like vim, still choose their shape, and
Animations off or macOS Reduce motion keeps it still.

## Status

Early and in active development. macOS first (Linux-capable via the Rust stack). The `docs/` folder
is an Obsidian vault with the vision, architecture, and roadmap.

## History

This repository previously shipped a small Rust CLI that hosted a single agent and fired a
notification on completion. It has since been repurposed into the terminal app; that earlier code
remains in the git history, with its docs under `docs/90-Archive-v1-CLI-Wrapper/`.
