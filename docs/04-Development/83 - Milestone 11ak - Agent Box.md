# Milestone 11ak — Agent Box

**Status:** 📝 design approved 2026-11-17 ("approved") on branch `m11ak-agent-box`; plan written 2026-11-17 ([[M11ak Plan - Agent Box]], 15 tasks), waiting for your review.

**Asked for:** 2026-11-17, by you, while scoping M11ag ("Ask the agent from the code"). You showed three screenshots of
another app's message box — a folder and branch chip above the box; a folder picker (Search, Recent, This Computer ▸,
New Project, No Project); a model menu with effort, View Usage ▸ and a mode chip — and said "these additional layout
that we are trying to build". Then: "yes, split it — box first, drop mic and main agent", and for `/` and `@`: "include
both in the box milestone". By default "the active folder is the folder the active terminal we are currently on".

**Order:** M11ak (this) → M11ag (ask from the code), whose chips go into this box. It also starts on your M11aa
feedback ("the agent tab is not what I expect it be, the layout, the flow") for the box; the conversation area above it
is not redesigned here.

## Why

The agent tab's box today is a plain text box with a few chips above it. You want the box to be where you choose
**where** the agent works (folder, branch), **how** it works (model, effort, mode) and **what it costs you** (usage),
and where you reach the agent's own commands (`/`) and things to mention (`@`) — the way modern agent apps do it, but
for Claude and agy running inside fuxx.

## Goals

1. **The box's new layout** (sketch below): folder and branch chips on top; the text box with its attachment chips
   inside; `+`, the model, the mode and send along the bottom.
2. **📁 Folder chip** — defaults to the active terminal's folder; a picker to choose another.
3. **⑂ Branch chip** — the branch of that folder; opens the branch switcher fuxx already has (M11o).
4. **Model menu** — the agent's models, with effort, View Usage ▸.
5. **View Usage** — % left and reset time per limit, for Claude and agy.
6. **Mode chip** — the agent's permission modes.
7. **`+`** — attach a file or an image.
8. **`/` menu** — the agent's own commands, skills and sub-agents, filtered as you type.
9. **`@` menu** — files (today's picker), the agent's sub-agents and skills.

```
╭──────────────────────────────────────────────────────────────╮
│ 📁 my-app ▾   ⑂ master ▾                                      │
│ ╭──────────────────────────────────────────────────────────╮ │
│ │ [📎 exit 1 · npm test ×]                                  │ │
│ │ Ask anything, @ to mention, / for actions                 │ │
│ │ +   Claude Opus 5.5  High ▾           🛡 accept edits ▾  ➤ │ │
│ ╰──────────────────────────────────────────────────────────╯ │
╰──────────────────────────────────────────────────────────────╯
```

## Non-goals and anti-goals

- **Not in it:** the 🎤 mic (needs a speech service; Scope says offline, no new services; macOS dictation works in
  any text box), "Main Agent ▸" / "Orchestrator" (another app's features), M11ag's "Ask claude from the code" and
  notes under edits, redesigning the conversation area above the box, shell tabs (M11ab).
- **Anti-goals:**
  - Anything sent to the agent without your send — choosing a model, mode or folder never sends a message.
  - Losing a conversation: a change that needs a restart resumes the same conversation.
  - A menu that lies: a list fuxx could not read says so ("Couldn't read the model list") rather than showing a
    made-up one.
  - Spending your quota to fill a menu: lists and usage come only from commands that run without the model ($0,
    zero turns — checked 2026-11-17, below).
  - Private names: the screenshots that inspired this show your work projects' folder names; none of them goes into
    fuxx's docs, mockups, tests or fixtures (examples use `my-app`, `scratch`).

## What the agents give (checked 2026-11-17)

| | Claude (`claude -p`, stream-json) | agy (`agy -p`) |
|---|---|---|
| Folder | any; `--resume <id>` also works from another folder (Claude ≥ 2.1.223) | any; has its own projects (`--project`, `--new-project`) |
| Model list | documented in the `initialize` control response (`models`); **to confirm (Task 0)** | `agy models`: id + name, effort built in (`claude-opus-5-5-high` "Claude Opus 5.5 (High)") |
| Effort | `--effort low…max` | `--effort low…max` |
| Change model / effort / mode mid-conversation | documented as live requests in Claude's SDK (`set_model`, `set_permission_mode`, flag settings); **wire shapes to confirm (Task 0)** | restart on the same conversation with new flags |
| Modes | default (ask) / acceptEdits / plan / auto | accept-edits / auto / plan |
| Usage | `/usage` in print mode: $0, zero turns; text lines `Current session: 0% used · resets …`, `Current week (all models): 37% used · resets …`; plus `rate_limit_event` messages when the status changes | `/usage` in print mode: $0, zero turns, **structured** (`command.data.groups[].buckets[]`: `window`, `remaining_fraction`, `reset_time`) |
| `/` list | init message lists `slash_commands`, `skills`, `agents` | `/help` in print mode: structured `commands[]` (name, aliases, description) |
| Images | base64 image blocks in a user message | unknown — files by path |

## Constraints

- **Provider-specific code stays in `src/providers/<agent>.rs`.** Generic code asks the provider: its models, efforts,
  modes, usage, commands, how to apply a change (live or restart).
- M11p: nothing visible changes inside `render`; no always-on timer (usage is fetched when its menu opens).
- Read-only toward your setup: fuxx never writes Claude's or agy's settings files.
- A debug build never writes your real `sessions.json` (test with `FUXX_DAEMON_SOCKET=/tmp/fuxx-dev.sock cargo run`).

## Approaches

- **A. Live where the agent can, restart where it can't (chosen; AI's pick, you handed over the how).** Each provider
  says how a change applies: Claude switches model, effort and mode in place with control requests on its running
  process; agy restarts on the same conversation id with the new flags (as fuxx already resumes). Lists and usage come
  from each agent's own $0 commands. Fast for Claude, works for agy today.
- **B. Always restart.** One code path, works for both, but every model or mode change costs a restart and a cold
  prompt cache — slow and wasteful for Claude, which can switch live.
- **C. Only offer what both can do live.** Nothing for agy; drops most of the box for one agent. Dropped.

## The design

### 0. Task 0 — confirm on the wire (first task, before UI)

As M11aa's Task 0: record real fixtures from Claude and agy (in `~/code/scratch`, no private names) for: Claude's
`initialize` response with `models`; a live `set_model`, effort change and `set_permission_mode` and what the stream
says after each; a `rate_limit_event`; `/usage` and `/help` output for both; an image message to Claude; **agy
restarted on the same conversation with a changed `--mode`, `--model` and `--effort`** (does the conversation carry on,
do the new flags take); whether `claude -p "/usage"` leaves a transcript behind (and the documented way to stop that, if
any); the short start-up probe of section 11. Write what was found into this note ("Task 0 findings"). If a Claude live change doesn't work as documented, that change falls back to
restart for Claude too (Approach A's fallback) — no question to you needed.

### 1. The box layout

- Top row, outside the text box: **📁 folder chip**, **⑂ branch chip** (shown only when the folder is in a git repo).
- The text box: attachment chips (today's command-block chips; later M11ag's code chips) on top inside it, the text,
  the placeholder "Ask anything, @ to mention, / for actions".
- Bottom row inside the box: **`+`**, **model chip** ("Claude Opus 5.5  High ▾"), then on the right **mode chip**
  (🛡 + mode name ▾) and **send** (➤; ■ Stop while working, as today).
- Today's chips above the box (model, branch, cost) go: model and branch move into the new chips; cost and context %
  move to the pane's top status line (AI's call).
- Look: dark rounded box and chips as in your screenshots, in fuxx's theme colours; menus use fuxx's existing menu
  style (the branch switcher's and the tab menu's).

### 2. Folder chip and picker

- **Default:** the active terminal's folder (where it is now — the folder your new-terminal chore uses), else the
  workspace's folder.
- **Picker** (opens up from the chip):
  - **Search** at the top (filters everything below).
  - **Recent** — folders agent tabs and workspaces were opened in, newest first (fuxx keeps this list; up to 8).
  - **This Computer ▸** — every project folder fuxx knows (workspace folders, saved sessions' folders) plus the
    agent's own known folders where the provider can read them (Claude: the `projects` keys of `~/.claude.json`, read
    only, skipped if missing or unreadable; agy: none unless Task 0 finds a list). Shown as `~/…` paths.
  - **New Project** — the macOS folder panel with "New Folder" allowed; the chosen or created folder is used.
  - **No Project** — your home folder.
  - A ✓ on the current folder.
- **Before the first message:** choosing a folder just changes where the agent will start.
- **After the first message:** the picker's choice asks **Move this conversation** (restart the agent in the new
  folder on the same conversation — Claude ≥ 2.1.223 resumes across folders; agy per Task 0) or **New conversation**
  (a new agent tab in that folder). If the provider can't move a conversation, only New conversation shows.

### 3. Branch chip

- Shows the folder's current branch (fuxx's git look-up, as the branch bar does). Click opens the existing branch
  switcher (M11o) for that folder. Not a repo → no chip. Switching branch never restarts the agent.

### 4. Model menu (with effort)

- Opens up from the model chip. **Model** heading, then the provider's models:
  - Claude: from the `initialize` response (Task 0), else the documented aliases (`opus`, `sonnet`, `haiku`,
    `fable`) as a fallback list marked "(default list)".
  - agy: from `agy models` (run once per app launch, $0), grouped by model name with the effort in the id
    (`… (High)`) shown as the effort.
- Each model row: name, current effort in grey, ✓ on the current one, **▸** opens its effort list
  (low / medium / high / xhigh / max, only the levels the provider says that model has).
- Below a divider: **View Usage ▸** (section 5).
- Choosing applies per section 7 and never sends a message.

### 5. View Usage

- A submenu like your screenshot: per group, each limit's name, "N% left" with a small ring, and "Resets in 3d 16h".
  - agy: from `/usage`'s structured groups and buckets (`remaining_fraction`, `reset_time`).
  - Claude: from `/usage`'s lines (`name: N% used · resets <time>`), shown as "100 − N % left". A line fuxx can't read
    is shown as plain text instead of a ring (never a wrong number).
- Fetched when the menu opens, as a separate quick process in the box's folder (not in your conversation), with
  "Checking…" while it runs; the answer is kept for 60 s. Claude's `rate_limit_event` messages update the Claude rows
  live (a "Close to limit" / "Limit reached · resets 14:00" line), and the model chip shows a small warning dot while a
  limit is close or reached.
- An agent with no usage command: "Usage isn't available for <agent>".

### 6. Mode chip

- 🛡 + the mode's name ▾; a menu of the provider's modes with one-line descriptions (Claude: Ask before edits /
  Accept edits / Plan / Auto; agy: accept-edits / auto / plan). Applies per section 7. agy's mode no longer locks after
  the first message (M11aa's lock) — it restarts on the same conversation instead.

### 7. Applying a change (live or restart)

- The provider answers, per change kind (model, effort, mode, folder), **live** or **restart**.
- **Live:** the control request goes to the running process; the chip changes when the agent confirms (Task 0 decides
  what confirms). While the agent is working, the change waits for the turn to end (AI's call: no mid-turn switches).
- **Restart:** after the current turn, the agent restarts with `--resume`/`--conversation` and the new flags; a quiet
  line in the conversation says "Switched to Claude Sonnet 5.5 · medium". Queued text is kept (as M11aa's resume).
- Before the first message nothing restarts: the next start just uses the new choices.

### 8. `+` attach

- A menu: **File…** (the macOS file panel; the path goes in as a chip and is sent as a path in the message — the agent
  reads it), **Image…** (Claude: sent as an image block; agy: as a path, per Task 0). Also paste an image from the
  clipboard and drop files onto the box (AI's call: same chips). Images over 5 MB are refused with a note (the smaller
  API limit).

### 9. `/` menu

- Typing `/` at the start of the box opens a list filtered as you type: the agent's commands (Claude: init
  `slash_commands`; agy: `/help`), its skills, its sub-agents — name and one-line description, grouped.
- Picking one inserts it (`/name `). Some commands fuxx answers itself instead of sending: `/usage` opens View Usage,
  `/model` the model menu, `/effort` the effort list, `/clear` starts a new conversation (as today). Which ones is the
  provider's list. Everything else is sent to the agent as text, as today.
- Esc closes the list; ↑↓ and Return pick; Tab completes.

### 10. `@` menu

- `@` opens today's file picker, now with sections: **Files** (as today), **Agents** (the agent's sub-agents),
  **Skills**. Picking a file inserts its path (as today); an agent or skill inserts `@name`.

### 11. When the lists are read (before the first message)

- A tab that hasn't sent yet has no running agent, but its chips and menus must work. On opening (and when its folder
  changes) fuxx runs a **short start-up probe** per agent and folder, never the model: Claude — a stream-json process
  that sends `initialize`, reads the answer (models, commands, skills, sub-agents) and quits; agy — `agy models` and
  `/help` in print mode. The answer is kept per agent and folder for the app's run; a running agent's own start-up
  message refreshes it. While the probe runs, menus say "Loading…"; if it fails, "Couldn't read the list" (anti-goal:
  no made-up lists). (AI's call, after the builder check.)

## Calls made after the builder check (AI's, for you to check)

- **agy changes need a restart, and Task 0 tests that it works** (same conversation, new mode/model/effort). If
  agy doesn't carry the conversation on with new flags, its model, effort and mode chips change only what the **next**
  conversation uses, and say so in their menu ("applies to a new conversation"), with a **New conversation** item.
- **Usage and the lists come from separate quick commands**, outside your conversation: tested 2026-11-17 for both
  agents at $0 and zero turns; the limits are your account's, so a new session reports the same numbers. agy's leaves
  no conversation (`conversation_id` empty); whether Claude's leaves a hidden transcript is checked in Task 0 and
  avoided if Claude offers a way.
- **The lists before the first message** come from the start-up probe (section 11), not from starting your agent
  early.
- **"Move this conversation" needs Claude ≥ 2.1.223:** fuxx reads `claude --version` once per app run; older versions
  get only **New conversation**. agy's move follows Task 0.
- **Cost and context %** move to the agent pane's existing top line (M11aa: "status word … context 18%").
- **`/` commands fuxx answers itself** extend what the agent tab already does for `/clear` (M11aa follows the new id);
  nothing is replaced.
- **ui-shots scenes use made-up data only** (folders `~/code/my-app`, `~/code/scratch`; made-up models and usage) —
  never your real folders or Claude's `~/.claude.json`.

## Left to the builder

- Where the box's parts live (a new `src/agent_box.rs` next to `composer.rs`, or inside it), and the provider trait's
  new methods' exact shapes.
- Menu sizes, spacing, icons (glyphs already used by fuxx).
- How the Recent list is stored (with the sessions file or a small file next to it).
- Tests: provider parsing (models, efforts, modes, usage, commands) from Task 0's fixtures is pure and unit-tested; each
  menu and chip gets ui-shots scenes in light and dark.

## Related

[[Proposal - Chat View and Agent Tabs]] · [[79 - Milestone 11aa - Agent Tabs]] · [[Proposal - File View Upgrades]] ·
[[82 - Milestone 11af - Live Changes]] · [[12 - Scope]] · [[40 - Roadmap]]
