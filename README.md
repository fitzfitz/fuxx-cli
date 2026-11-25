<p align="center">
  <img src="assets/logo/3d-cyber/fuxx-app-icon-512.png" alt="fuxx logo: the ╭∩╮ mascot in neon green" width="180">
</p>

<h1 align="center">fuxx</h1>

<p align="center">
  <strong>Your not-so-friendly neighborhood CLI.</strong><br>
  A native, GPU-fast terminal for people who run AI coding agents and refuse to babysit them.
</p>

<p align="center">
  <a href="https://github.com/fitzfitz/fuxx-cli/releases/latest"><img alt="Latest release" src="https://img.shields.io/github/v/release/fitzfitz/fuxx-cli?label=latest&color=00c853"></a>
  <img alt="Version 0.13.1" src="https://img.shields.io/badge/version-0.13.1-0f172a">
  <img alt="macOS 11+" src="https://img.shields.io/badge/macOS-11%2B-black?logo=apple">
  <img alt="Apple Silicon and Intel" src="https://img.shields.io/badge/Apple%20Silicon%20%2B%20Intel-universal-555">
  <img alt="Built with Rust and Chaos" src="assets/badges/built-with-rust-n-chaos.svg">
</p>

---

## So what is it?

You've got three agents going. One is refactoring auth, one is writing tests, one is "just looking around".
You flip between windows like a nervous parent at a playground. Is it done? Is it stuck? Is it asking you
something *right now*?

**fuxx** is a real terminal built for exactly that. Every workspace shows what its agent is doing (working,
waiting for you, done), pings you when it needs a human, and otherwise leaves you alone. Run your agents in a
terminal like always, or give **Claude Code** and **Antigravity** (`agy`) an **agent tab**: a chat fuxx draws
itself, with every tool call, diff, question and subagent laid out where you can actually see it. The mascot is
`╭∩╮`. That's the whole attitude: it gets out of your way, and it's not here to make friends.

Current version: **0.13.1**. macOS 11 or newer, Apple Silicon and Intel.

---

## Install it (on your Mac, or any other Mac)

1. **Download** `fuxx-<version>.zip` from the [latest release](https://github.com/fitzfitz/fuxx-cli/releases/latest).
2. **Unzip** it (double-click) and drag **`fuxx.app`** into **Applications**.
3. **Open it once the macOS way.** fuxx isn't signed with an Apple Developer ID yet, so the first launch gets
   blocked with a "can't be opened" warning. That's expected:
   - open fuxx (it gets blocked, click **Done**),
   - go to **System Settings → Privacy & Security**, scroll down to the message about fuxx,
   - click **Open Anyway**, then **Open** (and your password if asked).

   You only do this once per Mac.
4. **That's it.** From now on fuxx updates itself: when a new version is out, an **Update Available** button shows
   up in the status bar. One click downloads it, checks it, and reopens fuxx. Or use **fuxx → Check for Updates…**.

> Prefer the terminal? After step 2, `xattr -dr com.apple.quarantine /Applications/fuxx.app` clears the "downloaded
> from the internet" flag so the warning never appears. Only do that for a copy you downloaded from the release page
> above.

No Homebrew cask yet; it's on the roadmap, after a signed build.

**Moving to a new Mac?** Install as above, then copy `~/.config/fuxx/` (your settings and themes) over. Your
workspaces live in `~/Library/Application Support/fuxx/sessions.json` if you want those too.

---

## What it does

### 🧠 Knows what your agents are up to
- Every terminal and agent tab gets a **live status**: working, waiting for you, done, or idle. You see it as a badge
  on its avatar in the sidebar, on its tab, in the status bar, and even as the **cursor's colour**.
- A waiting agent's **question shows right in the sidebar**, with an **Open** button that takes you straight to it.
- **Desktop notifications** from fuxx itself when a background agent finishes or needs you (never about the one you're
  looking at). Click a banner and you land on the terminal that raised it. It even says which command finished.
- **⇧⌘U** jumps to whoever has waited longest for you. Press it again for the next one.

### 🤖 Agent tabs: Claude and agy as a chat
- Pick **Claude** or **agy** from a pane's **+** menu, ⌘K or the Welcome card, and fuxx runs the agent itself and draws
  the conversation: replies, tool calls, diffs, and permission prompts with real buttons. Typing `claude` in a
  terminal still gets you the classic TUI, and **Open in Classic View** is one right-click away.
- **The message box** has folder and branch chips, a model menu with effort, the mode chip (new tabs start in
  **Auto**; **⇧Tab** cycles modes), **+** for images, and `/` and `@` menus. **↑ / ↓** bring back what you sent from
  that folder, and Claude's guess at your next message waits greyed out in the empty box until **Tab** takes it.
  The folder picker leads with each folder's name (**⌘1–⌘8** open recent ones), the branch chip follows a `git
  switch` made anywhere, and a clock shows the turn's cost and the context used.
- **⌘V never stays silent**: files copied in Finder paste as pills, TIFF and BMP images become PNG, and anything fuxx
  can't use says so.
- **Drag files in** from the Files tree (or Finder) and they become pills. Select lines in a file tab, hit
  **Ask claude** (⌘↵), and they land in the box as a chip.
- **Rewind and Fork** from any of your messages (Claude): go back to before it, and undo the agent's file changes too
  if you like, or fork the conversation into a new tab. **Esc Esc** picks a message from the keyboard. agy gets Edit.
- Sent something while the agent was busy? **×** takes it back, as long as the agent hasn't read it yet.
- **Questions come as a card**: when the agent wants you to choose, pick with a digit, a click or the arrow keys, or
  type your own answer. Claude's questions work this way, and so do agy's (answered by message).
- **Plan Mode you can read**: when Claude finishes planning, the plan arrives as a card, rendered like a reply,
  folded when long, its file one click away. **Approve · auto-accept edits** (⏎), **Approve · ask per edit**, or
  **Keep planning…** with a note; a slim bar keeps the answers in reach while you read a long plan.
- **Plan progress**: a **☰ 4/6** tab over the box follows the plan the agent works from: each task done (with its
  commit), in progress, or next.
- **Subagents get lanes**: a pill per subagent above the chat (⌥⌘[ / ⌥⌘]) shows its own steps live. Stop one
  (Claude), message a running one through Main, and a finished one tells you how it ended. Subagent rows wear a
  little robot, and the status says **Orchestrating…** while only subagents work.
- **See what each step actually did**: a busy step draws as **file tracks**, one bead per read, search, edit or
  script, a row per file. Small steps are readable rows with outcome chips ("2 passed", "+41 −0", "build failed ·
  E0425"). Checks are marked honestly: passed, failed, failed on purpose (TDD), or unknown, never a guessed green.
  The **turn ledger** over the box says what's running, how many files changed and how the checks stand; click it
  for the changed files under their folders, with A/M/D and +/−, and any file's diff.
- Your latest message stays **pinned over the chat** while you scroll; click it to jump back. Chat text can be
  selected and copied, and every code block has a copy button.
- **Live usage**: the status bar shows how much of each agent's limit you've used and when it resets.

### 🗂 Workspaces, panes, tabs, and a sidebar that looks good doing it
- A **workspace** is a layout of **split panes** (⌘D / ⇧⌘D, or the two split buttons at the end of each tab bar);
  every pane has its own **tab bar** of terminals, agent tabs, web pages and files.
- The **+** on a tab bar opens a menu: Terminal, Browser, File, or any agent preset (greyed if it isn't installed).
  Start typing to search URLs, open tabs, history and files. **⌘T** makes a tab like the last one: the same agent in
  the same folder, a browser after a browser. **⇧⌘T** is always a terminal.
- **Right-click** (or ⌃-click) a terminal, a file tab or the chat for Copy, Paste, Copy Link, Add to Chat and friends.
- The sidebar gives each workspace a coloured **avatar** and a line saying what it's doing (or where it is).
  Workspaces from the same folder get grouped into a **card**.
- **Drag anything anywhere**: reorder workspaces, move them in and out of groups, drag tabs between panes or onto
  another workspace.
- **⌘K** searches everything (actions, workspaces, files, settings). **⌘P** jumps between workspaces.
- **Pin** what matters, give workspaces a colour, rename with a real text box (yes, the caret moves).
- **⌘Z / ⇧⌘Z** work in every text field and at the shell prompt. Closing a tab that's still working asks first.

### 📁 Your repo, right there
- **⌥⌘\\** opens the right sidebar: **Files**, **Changes** and **Activity**.
- **Files**: single-click a file for a quick **preview tab** (italic, replaced by the next click); double-click to keep
  it. Right-click for New File, Rename, Reveal in Finder, Copy Path or Move to Trash (⌘Z puts it back). Double-click
  a folder and your terminal `cd`s there; `cd` in a terminal and the tree follows.
- **Changes**: stage, unstage and revert files, then **commit** right there in a two-line box (↵ or ⌘↵), or let ✦
  have your agent write the message from the staged diff. A branch bar says where you stand against origin, with
  Fetch, Pull, Push or Publish, and opens the **branch switcher** (with a confirm step and conflict warnings). A
  **Graph** shows the history with lanes and a hover card.
- **File tabs** are read-only viewers that update live while your agent edits: lines it just changed **glow**, each
  one marked with who wrote it, and the agent's own reason shows under its edit. Syntax colours, folding, a sticky
  line, ⌘F, and a **File ↔ Diff** switch with review-style cards or side by side. fuxx never changes your files.
- **Markdown** gets a **Preview**: a proper reader with an outline, callouts, coloured code and **Mermaid** diagrams
  (drawn offline).

### 🌐 A browser in a pane
- Open web pages as tabs right next to your terminals (⇧⌘B). Click a port like `:3000` in the status bar to open your
  dev server.
- Links open where you want them (**Preferences → Browser → Open links in**): a browser split on the right (the
  default), a tab in the same pane, or your default browser. Pages a program opens by itself (`npm run dev` with
  auto-open, say) land the same way.
- Agents can drive the browser too (`fuxx browser open|click|fill|snapshot…`, or the MCP tools).

### 🔌 SSH, done properly
- **⇧⌘S** picks a host from your `~/.ssh/config`. It runs your own `ssh`, so keys, jump hosts and 2FA just work.
  Optionally keep it alive in tmux on the host.

### ✨ The fun stuff
- **Chat-style shell tabs** (zsh): each finished command reads like a message card with its output underneath, and
  the live prompt sits in a box at the bottom (`shell-cards = off` brings back the classic look).
- **Command blocks** (zsh): every command gets a slim bar (red when it failed). Hover it to copy the command, copy
  its output, or rerun it. ⇧⌘↑ / ⇧⌘↓ jump between commands.
- **A cursor with personality**: it glides, breathes instead of blinking, and can leave a **comet trail** or throw
  **sparks** as you type (Settings → Appearance → Cursor).
- **Key sounds**: give Enter and Ctrl+C a sound (a file, or a link to an mp3), each with its own On/Off. Click-clack.
- **Themes**: Catppuccin, Dracula, Gruvbox, Nord, TokyoNight and more, with a live preview. Light themes look good too.

### 🧰 Grown-up stuff
- **Restore**: your workspaces, panes, tabs, pins, folders and agents come back after a restart.
- **Keep running after quit** (opt-in): sessions live on in a tiny background service, and reopening brings them back live.
- **Agent presets**: one key (⌃⌘1–9) to start `claude`, `agy`, `codex` and friends where you want them.
- **Plugins**: paste a GitHub link in Settings → Plugins to add themes or helper programs; one click removes them
  again (below).
- **Help menu**: a searchable list of every shortcut (⌘/), the `fuxx` command reference, and Show Logs.
- **Scriptable**: the `fuxx` command and an **MCP server**, so agents can drive fuxx (below).

---

## Script it

Inside any fuxx terminal, the `fuxx` command talks to the app:

```sh
fuxx ls                                   # every terminal and agent tab: id, status, name…
fuxx new --preset claude --split right    # start an agent next to you (prints its id, e.g. s4)
fuxx wait s4                              # block until it needs you (or --for done|exit)
fuxx question s4                          # what's it asking?
fuxx answer s4 "yes"                      # tell it
fuxx events                               # live stream of everything that happens
fuxx status a1 · fuxx answer a1 yes       # agent tabs are a1, a2…: status, question, answer, wait
fuxx note "building" · fuxx progress 40   # show a note / progress bar on your own row
fuxx link open http://localhost:3000      # open a page the way a clicked link would
```

Let your agents drive fuxx: `claude mcp add fuxx -- fuxx mcp`. `fuxx help` lists every command.

---

## Make it yours

Settings live in **⌘,** (General, Appearance, Sessions, Browser, Sound, Plugins) and in `~/.config/fuxx/config`, one
`key = value` per line:

```ini
theme = TokyoNight
font-size = 14
cursor-fun = sparks
session-restore = agents        # off | layout | agents
persistent-sessions = on        # keep sessions running after quit
open-links-in = split           # split | tab | browser
agent-suggestions = on          # Claude's next-message guess in the box
shell-cards = on                # chat-style shell tabs
sound-enter = /System/Library/Sounds/Tink.aiff
```

---

## Making a plugin

A plugin is a public GitHub repo with a `fuxx-plugin.conf` file at its top. Anyone can install it from
**Settings → Plugins** by pasting the repo's link, and remove it again from the same page.

A plugin can share **themes** or bring **helper programs** (below). A theme is a plain text file in fuxx's
theme format. Here is a plugin that shares two:

```ini
# A theme plugin
format      = 1
id          = sunset-themes
name        = Sunset Themes
version     = 1.0.0
description = Warm themes from the design team.
theme       = themes/Sunset
theme       = themes/Sunset Light
```

Put that file at the top of the repo, add the two theme files under `themes/`, push it, and share the link.
There is a complete example in [`examples/plugins/sunset-themes`](examples/plugins/sunset-themes).

| Key | Required | What it is |
| :--- | :--- | :--- |
| `format` | yes | The plugin format. Today that is `1`. |
| `id` | yes | `a-z`, `0-9` and `-`, 2 to 40 characters. Also the plugin's folder name. |
| `name` | yes | What Settings → Plugins shows. |
| `version` | yes | Shown next to the name. Free text. |
| `description` | no | One line under the name. |
| `bin` | no | A folder of programs to put on `PATH`. See Helper programs. |
| `zsh` | no, repeat it | A zsh script to load in every new terminal. |
| `theme` | no, repeat it | A theme file, as a path inside the plugin. Its file name is the theme's name. |

Good to know:

- Keep any data your plugin needs in `$FUXX_PLUGIN_DATA` (set only while your scripts load; see Helper
  programs). fuxx can only remove what's in its own folders, so that is how uninstalling leaves nothing behind.
- Without releases, fuxx installs and updates from the default branch, so pushing is enough to offer an update.
  Once the repo has a release, fuxx installs the newest release instead (a `fuxx-plugin-macos-*.zip` asset if you
  attach one, otherwise the release's source), so make a new release after you change the plugin.

### Helper programs

A plugin can also bring a program and a zsh script. Add these keys:

```ini
bin = bin
zsh = shell/hello.zsh
```

- `bin` is a folder in the plugin. fuxx puts it on your `PATH` in every new terminal, so what's inside runs by name.
- `zsh` is a script (repeat the key for more). fuxx loads it in every new terminal, after the user's own `.zshrc`.
- While your scripts load, `$FUXX_PLUGIN_DIR` (the plugin's folder) and `$FUXX_PLUGIN_DATA` (a folder for your data)
  are set. They are unset afterwards, so keep a copy if you need them later, as the example does.
- zsh only. Terminals running bash or fish don't get helper plugins.
- People see this before installing: "Runs a program on your Mac in every new terminal. fuxx can't limit what it
  does, so only install plugins from people you trust."
- Scripts work straight from a plain repo. A compiled program goes in a release zip named
  `fuxx-plugin-macos-arm64.zip` or `fuxx-plugin-macos-x86_64.zip`, or one universal `fuxx-plugin-macos.zip`.
- If one plugin's script fails, the others still load.
- `exec zsh` or a zsh started inside a fuxx terminal starts without plugins; open a new terminal instead.

### Settings

A helper plugin can declare settings. fuxx shows each one on the plugin's card in **Settings → Plugins**: a switch
for `bool`, a number field for `number`. One line each, `setting.<name> = <type> <default> <label>`:

```ini
setting.list       = bool on    Show a suggestion list when you pause
setting.list_delay = number 3   Seconds of pause before the list shows
```

- `<name>` is `a-z`, `0-9` and `_`, starting with a letter. A `bool` is `on` or `off`; a `number` is a plain decimal
  like `3`, `0.5` or `-1`. The label is the rest of the line.
- While your scripts load, every setting the user **changed** is set as `FUXX_PLUGIN_SETTING_<NAME>` (the name in
  capitals, e.g. `$FUXX_PLUGIN_SETTING_LIST_DELAY`), next to `$FUXX_PLUGIN_DIR`, and unset afterwards. A setting
  left at its default isn't set, so your script decides what happens then (your own default, or a variable the
  user set in `.zshrc`).
- fuxx keeps the values in its settings file (`plugin.<id>.<name> = <value>`); a change reaches new terminals.
  Uninstalling removes them.
- A setting line fuxx can't read is skipped; the rest of the plugin still installs. fuxx doesn't check ranges, so
  say the range in the label and keep your script to it.
- Installing, updating, switching off or removing a helper changes new terminals; terminals already open keep
  what they had until you close them.

There is a complete example in [`examples/plugins/hello-helper`](examples/plugins/hello-helper).

---

## Handy keys

| Keys | What happens |
| :--- | :--- |
| ⌘N / ⌘T / ⇧⌘T | New workspace / new tab like the last one / new terminal tab |
| ⌘D / ⇧⌘D | Split right / split down |
| ⌘K / ⌘P | Search everything / jump to a workspace |
| ⇧⌘U | Go to the agent that needs you |
| ⌘1 … ⌘9 / ⌃⌘1 … ⌃⌘9 | Pick a workspace / start an agent preset |
| ⌘\\ / ⌥⌘\\ | Hide the sidebar / open the right sidebar |
| ⇧⌘S / ⇧⌘B | SSH session / browser tab |
| ⇧Tab (agent tab) | Next agent mode |
| ⌥⌘[ / ⌥⌘] (agent tab) | Previous / next subagent lane |
| ↑ / ↓ (agent box) | Bring back what you sent |
| ⌘/ / ⌘, | Every shortcut / Preferences |
| ⌘= / ⌘− / ⌘0 | Bigger / smaller / actual font size |

---

## Under the hood

Rust · [GPUI](https://github.com/zed-industries/zed) (the UI framework behind Zed) · `alacritty_terminal` ·
`portable-pty`. Agent tabs talk to Claude Code and Antigravity in their stream-json modes, one provider file per
agent. The `docs/` folder is an Obsidian vault with the vision, the roadmap and a note for every milestone.
[`STRUCTURE.md`](STRUCTURE.md) maps the repo.

Building from source: `cargo run` (Rust stable, macOS). The off-screen screenshot checks of every screen:
`scripts/ui-shots.sh` (parallel; `--changed <base>` runs only the scenes a change touches, `--png` saves the
pictures to `target/ui-shots/`).

## Status

Young, moving fast, and used daily. macOS first. Things will change, mostly for the better.

## History

This repo started life as a tiny Rust CLI that ran one agent and pinged you when it finished. It grew up into this
terminal. The old code is in the git history, its docs under `docs/90-Archive-v1-CLI-Wrapper/`.
