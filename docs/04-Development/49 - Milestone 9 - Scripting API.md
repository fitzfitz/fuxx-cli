---
tags: [development, milestone]
status: in-progress
---

# 49 - Milestone 9 - Scripting API

**Status:: 🟨 In progress (plan written 2026-10-27).**  ·  **Phase 3: fuxx you can drive from scripts and from agents.**
**Plan:** [[M9 Plan - Scripting API]] (step-by-step, with status per task)

## Goal

Scripts, tools and **other agents** can control fuxx. They can open, split, name and close sessions, type into them,
read their screens, and, unlike any other terminal, **ask what each agent is doing, wait until one needs you, read
its question and answer it**. A live event stream reports what happens across all sessions. fuxx also works as an
**MCP server**, so Claude Code in one pane can run and watch agents in other panes. ([[40 - Roadmap]] M9;
[[11 - Features]] E.)

## Your choices (2026-10-27)

- You skipped the design questions ("go ahead") and asked for **more functionality than cmux, Warp or iTerm2**.
- Everything below is the AI's call. Say if any should change.

## Where the others stand (checked 2026-10-27)

- **cmux** has a JSON socket and CLI. You can list, create, select and close workspaces, split panes, send text and
  keys, read the screen, post notifications, and set sidebar status, progress and log lines
  ([cmux API](https://cmux.com/docs/api)). It has no event stream, no waiting, and doesn't know what an agent is doing.
- **iTerm2** has a full Python API: sessions, splits, send text, read the screen. To react to changes you write and
  run your own Python scripts.
- **kitty** has strong remote control (launch, send-text, get-text, match windows by title or folder).
- **Warp** has no scripting API, only a URL scheme and saved launch configurations.

**What fuxx adds that none of them have:**
- Agent status in the API: idle, working, waiting or done.
- `wait` until an agent needs you, finishes, prints something, or exits.
- `question`: what the agent is asking. `answer`: reply only if it's actually waiting.
- A live JSON event stream for every session.
- An MCP server, so agents can drive fuxx directly.
- Each shell knows its own session (`fuxx` with no target means "this session").

## How it behaves

### Turning it on, and who can use it

- **On by default.** `scripting = off` in the settings file turns it off at the next start.
- fuxx listens on a private local socket at `~/Library/Application Support/fuxx/fuxx.sock`.
  - The file is readable only by you, inside a folder only you can open.
  - fuxx also checks that every connection comes from your macOS user, and refuses anyone else.
  - Nothing listens on the network.
- If another fuxx is already running and listening, the second one runs without scripting and logs one line.
  A leftover socket file from a crash is replaced.
- Every shell fuxx starts gets three things:
  - `FUXX_SOCKET` (the socket path);
  - `FUXX_SESSION` (that session's id);
  - the folder of the `fuxx` program added to the **end** of `PATH`, so typing `fuxx …` works in any fuxx session.
- Only a fuxx that is actually listening sets these. One that isn't (scripting off, or another fuxx has the socket)
  **removes** `FUXX_SOCKET` and `FUXX_SESSION` from its shells, so a fuxx started from inside another one can't
  drive the other one's sessions by accident.

### The `fuxx` command

`fuxx` with no arguments opens the app, as today. `fuxx <command>` talks to the running app and prints the answer.
Each command takes `--json` for machine-readable output.

**Targets.** Commands that act on a session take one:
- a session id like `s4`;
- `#2` for the 2nd row in the sidebar;
- a name or title, matched exactly and ignoring case.

With no target, the command acts on **the session it's typed in** (`FUXX_SESSION`); outside fuxx, it acts on the
focused session. If the session it's typed in has closed (e.g. a background job outliving it), that's an error
(`not_found`, "your session s3 is gone"), never the focused session. A target that matches nothing, or more than
one session, is an error that names the candidates.

| Command | What it does |
|---|---|
| `fuxx ls` | List sessions: id, number, name, status, group folder, preset, focused / on screen, note, progress. A leading `*` marks the focused session, `+` another one on screen; progress and note follow as `40% tests · building` |
| `fuxx new [--folder DIR] [--preset NAME] [--run CMD] [--name NAME] [--split right\|down [--of TARGET]] [--background]` | Open a session (or a split next to a session). `--run` types a command into the new shell like a preset does. `--background` keeps focus where it is. Prints the new id. |
| `fuxx close [TARGET]` | Close a session (its program is stopped) |
| `fuxx focus [TARGET]` | Show and focus a session (opens its group / layout) |
| `fuxx rename [TARGET] NAME` | Rename (`""` → back to the automatic name) |
| `fuxx send [TARGET] TEXT [--enter]` | Type text; `--enter` presses Enter after it |
| `fuxx key [TARGET] KEY…` | Press keys: `enter tab esc backspace delete up down left right home end pageup pagedown`, `ctrl-c`-style combos, `f1`–`f12` |
| `fuxx paste [TARGET]` | Paste standard input as one block (bracketed paste when the program asked for it) |
| `fuxx read [TARGET] [--lines N] [--all]` | Print the screen text (default: what's visible; `--lines N`: the last N lines; `--all`: all scrollback too) |
| `fuxx status [TARGET]` | Print `idle`, `working`, `waiting` or `done` |
| `fuxx question [TARGET]` | When the session is waiting: print what it's asking (the last lines of its screen). Otherwise print nothing and exit 1 |
| `fuxx answer [TARGET] TEXT` | Only if the session is waiting: type the text and press Enter. Otherwise refuse (exit 1) unless `--force` |
| `fuxx wait [TARGET \| --any] [--for waiting\|done\|idle\|working\|change\|exit] [--text TEXT] [--timeout SECS]` | Block until the condition holds, then print the session id and exit 0 (at once if it already holds). On timeout exit 124. `--text` waits for the text to appear on screen: only text that appears **after the wait starts** counts. The command you typed is echoed on screen too, so use a marker the command prints but doesn't contain, e.g. `echo ALL''DONE` or `echo $((1+1))DONE`. `--for exit` means the session closed (its shell ended). `--any` watches every session. The default is `--for waiting`. |
| `fuxx events [--json]` | Print one line per event until stopped (see below) |
| `fuxx note [TARGET] TEXT` / `--clear` | A short line shown under the session's name in the sidebar |
| `fuxx progress [TARGET] 0–100 [--label TEXT]` / `--clear` | A thin progress bar under the session's row |
| `fuxx notify [--title T] BODY` | A macOS notification (same as fuxx's own) |
| `fuxx equalize [TARGET]` | Even out the panes of a session's layout |
| `fuxx presets` | List agent presets |
| `fuxx ping` / `fuxx version` | Check that fuxx is reachable; print the version and the list of methods |
| `fuxx mcp` | Run as an MCP server on standard input/output (below) |

Exit codes: `0` ok · `1` refused / nothing to show · `2` bad arguments · `3` fuxx not running or scripting off · `124` wait timed out.

### Events

`fuxx events` (and the `events.subscribe` method) prints a JSON line for each of these:
- a session opened or closed;
- its status changed (with the old and new status);
- its title changed, or it was renamed;
- the focused session changed;
- fuxx posted a notification.

Each line carries the session id, number and name, and a timestamp.

### The protocol (for tools that don't use the `fuxx` command)

- One JSON object per line: `{"id": 1, "method": "session.list", "params": {}}`.
- The reply is `{"id": 1, "result": …}` or `{"id": 1, "error": {"code": "not_found", "message": "…"}}`.
- Methods mirror the commands: `system.ping`, `system.version`, `session.list|new|close|focus|rename|send|key|paste|read|status|question|answer|wait|note|progress`, `pane.equalize`, `preset.list`, `notify`, `events.subscribe`.
- A connection can send several requests. `events.subscribe` turns the connection into an event stream.

### MCP server: agents driving fuxx

- `fuxx mcp` speaks the Model Context Protocol (protocol version **2025-11-25**) on stdio and forwards to the running fuxx.
- **Tools:** `list_sessions`, `new_session`, `send`, `press_keys`, `read_screen`, `status`, `question`, `answer`,
  `wait`, `close_session`, `focus`, `rename`, `note`, `notify`.
- **Setup in Claude Code:** `claude mcp add fuxx -- fuxx mcp`. A Claude in one pane can then start another agent,
  wait for it to need input, read its question and answer it.

### Unchanged

The app, its sessions, and everything M5–M8 did. Scripting only does what you could do by hand: it types into
sessions, reads them, and arranges them. It never runs anything outside a session.

## Rough scope / checklist

- [ ] Private socket (on by default, `scripting = off`), same-user check, env vars and PATH in sessions.
- [ ] `fuxx` command: ls, new (folder/preset/run/name/split/background), close, focus, rename.
- [ ] Input and output: send, key, paste, read (screen, last N, all).
- [ ] Agent-aware: status, question, answer, wait (status/text/exit, any session, timeout).
- [ ] Event stream; note and progress in the sidebar; notify; equalize; presets; ping/version.
- [ ] `fuxx mcp` MCP server.

## Not in M9

- Scripting over the network, or from other macOS users.
- Running commands outside a session, or a plugin system inside fuxx.
- Changing settings or themes from scripts.
- Regular expressions in `wait --text`: it's plain text for now.
- Remembering session ids across restarts. Ids are fresh each launch; use names to find restored sessions.
- Any UI beyond the note line and progress bar (the UI/UX uplift is M11).

## Success check

In one fuxx session, run `fuxx new --preset claude --split right --background` and get an id back. Then
`fuxx wait <id>` returns when Claude asks something, `fuxx question <id>` prints the question, and
`fuxx answer <id> "yes"` answers it. `fuxx events` in another pane shows the status changes as they happen. After
`claude mcp add fuxx -- fuxx mcp`, Claude Code lists fuxx's sessions and reads one when asked.

## Related

- [[40 - Roadmap]] · [[11 - Features]] · [[12 - Scope]] · [[47 - Milestone 7 - Session Ergonomics]] · [[48 - Milestone 8 - Split Panes]]
