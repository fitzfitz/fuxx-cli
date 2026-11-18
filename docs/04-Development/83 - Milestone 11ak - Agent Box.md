# Milestone 11ak — Agent Box

**Status:** 🟨 built, **merged to master and pushed 2026-11-18**; waiting for your on-screen check. All 15 plan tasks (0–14) built 2026-11-17/18, each reviewed ([[M11ak Plan - Agent Box]]); the whole-branch review's findings fixed 2026-11-18 ("Final review" below); design approved 2026-11-17 ("approved").

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
| Model list | the `initialize` control response (`models`); confirmed (Task 0 findings) | `agy models`: id + name, effort built in (`claude-opus-5-5-high` "Claude Opus 5.5 (High)") |
| Effort | `--effort low…max` | `--effort low…max` |
| Change model / effort / mode mid-conversation | live control requests (`set_model`, `set_permission_mode`, `apply_flag_settings`); confirmed (Task 0 findings) | restart on the same conversation with new flags |
| Modes | default (ask) / acceptEdits / plan / auto | accept-edits / auto / plan |
| Usage | `/usage` in print mode: $0, zero turns; text lines `Current session: 0% used · resets …`, `Current week (all models): 37% used · resets …`; plus `rate_limit_event` messages when the status changes | `/usage` in print mode: $0, zero turns, **structured** (`command.data.groups[].buckets[]`: `window`, `remaining_fraction`, `reset_time`) |
| `/` list | init message lists `slash_commands`, `skills`, `agents` | `/help` in print mode: structured `commands[]` (name, aliases, description) |
| Images | base64 image blocks in a user message | a path in the message (agy reads it; Task 0 findings) |

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
  API limit; R8: for Claude the 5 MB is the image's base64, so 3.75 MB of file).

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

## Task 0 findings (recorded 2026-11-17)

Claude Code 2.1.326 and agy 1.5.10, in `~/code/scratch` (and `~/code/scratch/moved`), one-word prompts. Fixtures in
`src/providers/fixtures/` (paths scrubbed to `/tmp/fx`; the user's own skills, plugins, sub-agents and MCP servers
replaced by Claude's built-ins plus one made-up `example-skill` / `example-plugin`; `models` kept whole). Recorder:
`scripts/record-agent-fixture.py` (new scenarios below, and `--scrub-file` for a plain command's output). The M11aa
`claude-*.jsonl` fixtures' init lists (they named the user's installed skills, sub-agents and plugins) went through
`--scrub-file` too: the same built-ins plus made-up entries (`example-agent` where no built-in agent was left).

**Claude — the start-up probe (`claude-initialize.jsonl`).** A stream-json process that sends `initialize` gets its
`control_response` in **0.5–0.6 s** (from spawn) and leaves **no transcript** (nothing new under
`~/.claude/projects/<folder>/`). The answer (`response.response`) has: `models[]` (14 here) with `value`
(`default`, `opus`, `fable`, `sonnet`, `haiku`, then full ids such as `claude-opus-4-8`), `displayName`,
`resolvedModel`, `description`, `supportsEffort` and `supportedEffortLevels` (a model without effort — Haiku 4.5 —
has neither key; some older models list `low, medium, high, max` without `xhigh`); `commands[]` (`name`,
`description`, `argumentHint`, optional `aliases`, `builtin: true` for Claude's own; a skill's description ends
`(user)`); `agents[]` (`name`, `description`, optional `model`); `current_permission_mode` (`"default"`);
`available_output_styles`, `account` (scrubbed), `claude_code_version`. It does **not** say the current model or
effort: a `get_settings` control request (no model use, answered at once) does —
`response.applied.model` (`"claude-opus-5-5"`) and `response.applied.effort` (`"high"`).

**Claude — live changes. All three are live, before the first message and mid-conversation (sent after a `result`;
each recorded both ways).** Each is a control request on stdin; each is answered by a `control_response` with the same
`request_id` and `subtype: "success"`:

| Change | Request (`{"type":"control_request","request_id":…,"request":…}`) | What confirms it |
|---|---|---|
| Model | `{"subtype":"set_model","model":"sonnet"}` | the answer's `response.model` (`"claude-sonnet-5-5"`, the resolved id); the next `system/init` `model`; each `assistant` `message.model`; `result.modelUsage` keys. Mid-conversation (after a `result`) it also writes a `user` line `<local-command-stdout>Set model to \`opus (claude-opus-5-5)\`</local-command-stdout>` (`claude-set-model.jsonl`) |
| Effort | `{"subtype":"apply_flag_settings","settings":{"effortLevel":"low"}}` (the SDK's `applyFlagSettings`) | the answer has no body; `{"subtype":"get_settings"}` then says `applied.effort: "low"` (and `effective.effortLevel`) — before and after the turn; mid-conversation `"medium"` likewise reads back `applied.effort: "medium"` (`claude-set-effort.jsonl`). Nothing in the stream itself names the effort, and unlike `set_model` a mid-conversation change writes no `<local-command-stdout>` line |
| Mode | `{"subtype":"set_permission_mode","mode":"plan"}` | the answer's `response.mode` (`"plan"`); a `system/status` with `permissionMode: "plan"`; the next `system/init` `permissionMode`. Asked to write `hello.txt`, Claude wrote only its plan and asked `ExitPlanMode` (denied by the recorder): no `hello.txt`. Then, mid-conversation, `{"subtype":"set_permission_mode","mode":"acceptEdits"}`: answer `{"mode":"acceptEdits"}`, `system/status` and the next `system/init` say `acceptEdits`, and the same request wrote `hello.txt` with no permission prompt (the recorder denies every prompt here) (`claude-set-mode.jsonl`). No `<local-command-stdout>` line either |

**Claude — image (`claude-image.jsonl`).** A user message whose `content` is
`[{"type":"image","source":{"type":"base64","media_type":"image/png","data":…}},{"type":"text","text":…}]` works: a
1×1 red PNG → "Red".

**Claude — move (`claude-move.jsonl`).** `--resume <id>` from `~/code/scratch/moved` carries the conversation on (same
`session_id`; `system/init` `cwd` is the new folder; it recalled the first message). The transcript **stays in the
first folder's entry** (`~/.claude/projects/<scratch>/<id>.jsonl`, new lines carry the new `cwd`); the new folder's
entry gets only an empty `memory/`. So a resumed tab's history is read from the folder the conversation **started**
in, not the folder it is in now.

**Claude — usage (`claude-usage.json`).** `claude -p "/usage" --output-format json`: about 4 s, `num_turns: 0`,
`total_cost_usd: 0`, `local_command: "usage"`; `result` holds the text lines (`Current session: 13% used · resets Nov
17 at 8:10pm (<zone>)`, `Current week (all models): …`, `Current week (Fable): …`) followed by a "What's contributing to
your usage?" section (the fixture drops it and writes the zone as `UTC`: both are the user's). **It leaves a
transcript** (a new `<id>.jsonl` in the folder's entry). `--no-session-persistence` (in `claude --help`: "Don't save
sessions to disk … only works with --print") stops that: recorded again with it, nothing new appeared. The fixture is
that second run. `rate_limit_event` (in `claude-set-model.jsonl`): `rate_limit_info.status`, `resetsAt` (unix),
`rateLimitType` (`five_hour`), `unifiedWindows.{five_hour,seven_day}.{utilization,resetsAt,status}`.

**agy (`agy-models.txt`, `agy-help.json`, `agy-usage.json`).** `agy models`: id TAB name per line on stdout ("Fetching
available models..." goes to stderr), the effort built into the id (`gemini-3.8-flash-low` "Gemini 3.8 Flash (Low)");
one id has none (`gemini-pro-orchestrator-g1-pro` "Orchestrator (Argon)"). `/help` and `/usage` in print mode (about
3 s, `num_turns: 0`, `conversation_id: ""`): `command.data.commands[]` (`name`, `description`, optional `aliases`) and
`command.data.groups[].buckets[]` (`id`, `name`, `window`, `remaining_fraction`, `reset_time`, optional
`description`), as the design says.

**agy — restart with new flags (`agy-restart-flags.jsonl`).** A one-word conversation, then the same
`--conversation <id>` with `--mode plan --model gemini-3.8-flash-low` (an id exactly as `agy-models.txt` lists it, no
`--effort`): it **carries on** (recalled "Say hi in one word"); the second `init` names the model (`init.model:
"gemini-3.8-flash-low"`, the whole id; the first `init`, started without `--model`, has no `model` key); plan mode
**takes** (asked to write `hello.txt`, it wrote an implementation plan into its own
`~/.gemini/antigravity-cli/brain/<id>/` and asked for approval; no `hello.txt`). `init.permission_mode` says
`request-review` whatever `--mode` is, so it can't confirm a mode.
**Model flag form** (three recordings): `--model <id from agy models>` alone works (`agy-restart-flags.jsonl`, above).
`--model <id without its effort suffix> --effort <level>` also works (`agy-restart-flags-effort.jsonl`: `--model
gemini-3.8-flash --effort low`, a name `agy models` does not list; `init.model: "gemini-3.8-flash"`; the effort is
accepted but nothing reports it). An id with effort **plus a different `--effort` is refused**
(`agy-restart-flags-conflict.jsonl` and its `.stderr`): stderr `error: invalid model selection (--model
"gemini-3.8-flash-medium" --effort "low"): --model gemini-3.8-flash-medium conflicts with --effort=low`, a `result`
with `status: "ERROR"`, `conversation_id: ""` and the same `error` text, exit 1. So fuxx passes the `agy models` id
alone and no `--effort`.

**agy — move and image.** `--conversation <id>` from `~/code/scratch/moved` carries on (`init.cwd` is the new folder;
it recalled the first message) (`agy-move.jsonl`). A message naming a PNG's absolute path works: agy reads it with
`read_file` and answers "Red" (`agy-image.jsonl`).

| Change | Claude | agy |
|---|---|---|
| Model | **Live** (`set_model`) | **Restart** (same `--conversation`, `--model <id>`) |
| Effort | **Live** (`apply_flag_settings` `effortLevel`; read back with `get_settings`) | **Restart** (the effort is in the model id) |
| Mode | **Live** (`set_permission_mode`) | **Restart** (same `--conversation`, `--mode`) |
| Folder (Move) | **Restart** (`--resume` from the new folder; 2.1.326 ≥ 2.1.223) | **Restart** (same `--conversation` from the new folder) |

## What was built

All fifteen plan tasks (0–14), on branch `m11ak-agent-box`; each task reviewed clean (most after one fix round). No new
crate. The box is one `AgentBox` drawn by the agent tab; the provider files say what each agent can do.

0. **Task 0 — recordings.** Real fixtures from Claude and agy and the "Task 0 findings" above; `scripts/record-agent-fixture.py`.
1. **The provider's catalog** (`src/providers/`): models with efforts, modes, slash commands, skills and sub-agents,
   usage limits, "live or restart" per change; `model` and `effort` saved with the agent tab.
2. **Usage and rate limits** read from `/usage` (Claude's lines, agy's structured groups) and Claude's `rate_limit_event`.
3. **`src/agent_catalog.rs`** — short probe processes per agent and folder (kept for the app's run; 30 s deadline; a
   usage reading kept 60 s). A probe starts only from a tab opening, a folder change or a menu opening.
4. **Applying a change** — Claude live (`set_model`, `apply_flag_settings`, `set_permission_mode`), agy and a folder
   Move by restart on the same conversation; a change waits for the turn to end; a quiet "Switched to …" row after a restart.
5. **The box's layout** (`src/agent_box.rs`) — folder and branch chips on top, the box with its chips inside, `+`, model,
   mode and send along the bottom; cost and context % moved to the pane's top line.
6. **Folder chip and picker** (`src/folder_picker.rs`) — Search, Recent (up to 8), This Computer ▸, New Project (macOS
   panel with New Folder), No Project; after the first message Move this conversation or New conversation.
7. **Branch chip** — the folder's branch; a click opens the branch switcher (M11o).
8. **Model menu with effort** — models, ✓, effort in grey, `▸` effort list, `(default list)` fallback, "Loading…" and
   "Couldn't read the model list".
9. **View Usage ▸** (`src/agent_box_menus.rs` took the menus out of `agent_box.rs` first, no behaviour change) — "N% left"
   with a ring and "Resets in 3d 16h"; a warning dot on the model chip while a limit is close or reached.
10. **Mode chip and menu** — Claude's four modes, agy's three; agy's mode lock is gone; nothing cycles modes.
11. **`+` attach** — File… and Image…, paste an image, drop files; Claude gets image blocks, agy a path; too big refused
    (Claude 3.75 MB of file, 5 MB once in base64; agy 5 MB; R8).
12. **`/` menu** — commands, skills, sub-agents filtered as you type; Tab completes; `/usage`, `/model`, `/effort` open
    the box's own menus.
13. **`@` menu** — Files, Agents, Skills.
14. **Docs and hand-off** — this note, the roadmap, the Progress Log, the plan's table.

## Calls made while building (AI's, for you to check)

**Rulings during the run**

- **R1:** every later task was built on Task 0's findings; where a finding differs from the plan's text, the finding wins.
- **R2:** the plan-time calls below stand (you saw them with the plan and said "go ahead").
- **R3:** Task 0's fixtures were scrubbed of your own skills, plugins, agents and paths before commit.
- **R4:** the folder-change hook for the catalog is Task 6's, not Task 3's (the tab had no folder change before then).
- **R5:** sends are held only while a waiting change applies by **restart** (agy, a folder Move). A Claude live change
  never holds a message you send.
- **R6:** the box's menus moved into `src/agent_box_menus.rs` (the file had passed 1,900 lines) as a pure move.
- **R7:** the idle-CPU measurement needs a real window, so it is your check (item 15), as in M11af; the stray-process
  check was done here (below).
- **R8:** Claude takes images in the message, and the API's 5 MB per-image limit is on the base64 (Anthropic's vision
  docs), so for Claude an image over 3.75 MB of file (5 MB once encoded) is refused with "Image too big for Claude (3.75
  MB max): <name>". agy is told a path and keeps 5 MB of file and "Image over 5 MB: <name>". The limit is the provider's
  (`Provider::image_limit`); nothing is downscaled. The plan's raw-5 MB constraint reasoned from the wrong unit.
- **R9:** the mode Claude reports (`system/status` and `system/init` `permissionMode`) is followed when no mode change of
  yours is pending (asked, or waiting for the turn): after an approved plan Claude leaves plan mode itself, and the chip,
  the menu's ✓ and the saved tab now say so (choosing Plan again works; a restart doesn't resume a stale plan mode).
- **R10:** the final fix wave also took four small ones: a failed version read (Move needs it) is tried again; Return
  is blocked only while the `/` list is "Loading…"; the folder picker checks which folders are there off the UI thread;
  the doc fixes below.

**Plan-time calls**

- The agent already runs when a tab opens, so a change applies by the rules in section 7 from the start (note 83's
  first wording, "before the first message nothing restarts", was written before this was checked).
- Model and effort are saved with the tab and come back after a restart of fuxx.
- Flags (`--model`, `--effort`, `--permission-mode`) go to the agent only when you chose something.
- Claude's reset words keep Claude's own zone-less text ("Nov 22 at 10pm"); agy's come from its timestamp.
- agy's pasted images are written to `$TMPDIR/fuxx-images/` and cleaned when older than 24 h, at app start.
- The start-up probe's deadline is 30 s; a failed list is retried when a menu opens.
- `/clear` is not intercepted (the tab already follows the new conversation id).
- An own command (`/usage`, `/model`, `/effort`) is answered by the box only when it is the whole line; with more text
  it is sent as text. While the `/` list shows "Loading…", Return does nothing (it never sends a half-typed `/xx`); R10
  changed "Couldn't read the list": Return sends the line as typed.

**From the tasks**

- agy's mode lock (M11aa) is removed; its mode, model and effort change by restart on the same conversation.
- Nothing cycles modes: the old click-to-cycle (`next_mode`) is gone; Claude's mode changes only from the menu.
- agy gets `--model <id from agy models>` alone and never `--effort` (an id with an effort plus a different `--effort` is
  refused by agy: Task 0).
- The effort rule on a model switch: keep the level if the new model has it; an agy model must name one (medium, else the first).
- Base64 for Claude's images is fuxx's own small encoder in `src/providers/mod.rs` (no crate), run on a background job.
- Claude's moved-conversation history is found by scanning `~/.claude/projects/*/<id>.jsonl` (the transcript stays under
  the folder it started in, Task 0), off the UI thread and only when it is not under the current folder.
- The box's edge follows keyboard focus by reading focus while drawing (the tab is cached, so a listener could not).
- Provider-specific code stayed in the provider files; a new `Provider::stands_for_default` and `own_command` ask them.
- The `/` list's selected row uses the panel's hover fill (the code fill was invisible in the dark theme).

## Learned

- Claude's `/usage` in print mode leaves a hidden transcript unless `--no-session-persistence` is passed; fuxx passes it.
- A Claude `initialize` probe answers in about half a second and leaves no transcript.
- agy's `init.permission_mode` always says `request-review`, so a mode can't be confirmed from agy; the chip shows what you chose.
- A scene cannot drive the macOS file panel, a real Finder drop or the clipboard; those are on-screen checks (items 10–11).

## Idle CPU (the M11p anti-goal): code-level evidence

Not measured by the builder: it needs a real window with an agent tab shown, by hand (your check, item 15). What the code does:

- **No new always-on timer.** A catalog probe or a usage read starts only from a tab opening, a folder change or a menu
  opening; none from drawing or from a timer. A probe ends on its answer or at the 30 s deadline.
- The box's menus and lists are drawn only while open; the chips read cached text.
- Claude's `rate_limit_event` updates the limits when it arrives (an event the agent already sends), not on a poll.

## Stray processes (checked 2026-11-18)

After the full `cargo test --bin fuxx` and the full `scripts/ui-shots.sh`: `ps -axo pid,command | grep -E 'claude -p|agy -p|fuxx __ui-shots'` finds **no process** (0 matches): every probe and every scene's process had ended.

## Limits (small things you might notice)

- The model chip shows the running model while a change waits for the turn to end; the menu shows the waiting one.
- A conversation row lists attachments as 📎 for images too (the box's chip shows 🖼).
- **Image…** silently skips non-image files, and a pasted format other than png, jpeg, gif or webp is ignored.
- A conversation whose Claude session id is not a UUID shape has no history after a restart (fuxx refuses to read it).
- The folder picker lists a folder once a quick check (off the UI thread) finds it there, so Recent can appear a moment
  after the picker opens.
- The `@` file rows have no matched-letter highlight; skills seen before Claude's first start-up message may sit under
  Commands in `/`, not `@` Skills, until it arrives.
- Moving between two repos, the branch chip can flicker off and on once while the new branch is read.

## Final numbers (this Mac, debug build, 2026-11-18)

| Check | Before (master/Task 0) | Now |
| --- | --- | --- |
| `cargo test --bin fuxx` | 1,840 passed (1 ignored) | **1,944 passed** (1 ignored) |
| `cargo clippy --all-targets` | 14 (bin) / 27 (test) | 14 / 27 (no new warnings) |
| `cargo clippy --all-targets --features ui-shots` | 16 / 29 | 16 / 29 (no new warnings) |
| `cargo fmt --check` | clean | clean |
| Full `scripts/ui-shots.sh` | 147 scenes (master) | **191 passed, 0 failed**, 125.8 s (44 new `agent-box-*` scenes in the list; master had 147) |
| Stray `claude -p` / `agy -p` / `fuxx __ui-shots` processes | n/a | none left (see above) |
| Idle CPU, agent tab shown, nothing happening | n/a | not measured by the builder: your check, item 15 |

The reference fingerprints are report only and stale on this Mac (note 82, "Findings").

## On-screen check (yours, after the build)

Run `FUXX_DAEMON_SOCKET=/tmp/fuxx-dev.sock cargo run` (debug only: **do not add `--release`**, it writes your real
`sessions.json`) from the repo. Use a scratch folder, `~/code/scratch` (`git init` it, with a second folder next to
it such as `~/code/my-app` for the picker). Open an agent tab (claude) there. Do this once with claude and, where an item says so, with agy.

**The box and its folder**

1. The box shows the folder chip (the active terminal's folder) and, in a git repo, the branch chip above it; inside:
   the placeholder "Ask anything, @ to mention, / for actions", `+`, the model chip, the mode chip and ➤. Click the
   box's edge area: the text caret takes focus and the edge shows it.
2. Folder chip: the picker opens upward with Search, Recent, This Computer ▸, New Project, No Project and a ✓ on the
   current folder. Type to search. Pick `~/code/my-app`: the chip changes and nothing is sent. **New Project** opens the
   macOS panel with New Folder; the folder you make is used. **No Project** is your home folder.
3. Send one message, then pick another folder: the picker asks **Move this conversation** or **New conversation**.
   Move: the same conversation carries on in the new folder (ask "what did I just say?"). New conversation: a new agent
   tab in that folder.
4. The branch chip opens the branch switcher for that folder; switching a branch does not restart the agent.

**Model, effort, usage and mode**

5. Model chip: Model heading, the models, ✓ on the current one, its effort in grey, `▸` opens its effort list. Choose a
   model: nothing is sent; Claude's chip changes at once (live). With agy the chat restarts on the same conversation
   after the turn and a quiet "Switched to …" row appears.
6. **View Usage ▸**, with claude and then agy: a panel with each limit's "% left", a ring and "Resets in …"; it says
   "Checking…" on a first read. Close and reopen within a minute: no new wait.
7. Mode chip: Claude's four modes switch live (try Plan, then ask for a file: it plans; then Accept edits); agy's three
   restart on the same conversation, and the chip no longer locks after the first message. In Plan, approve Claude's
   plan: the chip leaves Plan by itself (R9).
8. Change the model while the agent is working: the change waits for the turn to end; it never cuts the turn.

**`+`, paste and drop**

9. `+`: **File…** opens the macOS panel; the file shows as a chip and goes to the agent as a path. **Image…** the same
   for an image; Claude describes it.
10. Paste a screenshot (⌃⌘⇧4, then ⌘V in the box): a 🖼 chip. Send it: Claude describes it; with agy it gets a path.
11. Drop a file from Finder onto the box: a chip, the box tinted while you hold it over. An image too big is refused with
    a note: with claude, over 3.75 MB, "Image too big for Claude (3.75 MB max): <name>"; with agy, over 5 MB,
    "Image over 5 MB: <name>".

**`/` and `@`**

12. Type `/` at the start: commands, skills and sub-agents, filtered as you type; ↑↓ move, Tab completes, Esc closes.
    `/usage` opens View Usage; `/model` the model menu. `/clear` still starts a new conversation.
13. Type `@`: Files (as before), Agents and Skills; picking a file inserts its path, an agent or skill inserts `@name`.

**A change that waits, and the cost**

14. While the agent works, choose a mode (agy) or a folder Move: it says it waits and applies after the turn; a message
    you type meanwhile goes to the switched agent.
15. **Idle CPU:** with an agent tab shown and idle (no menu open), compare fuxx's CPU in Activity Monitor on this branch
    with master (same window, a minute each). They should match within noise.

## Final review (2026-11-18)

The whole-branch review (`168aa86..b29fae5`) found two important things and some small ones; all were fixed in one
wave (rulings R8–R10 above):

- **Claude's image limit was measured on the file, not on its base64** (the API's 5 MB is after encoding): a 3.75–5 MB
  image passed fuxx and Claude's API refused it. Now each provider says its limit (`Provider::image_limit`): Claude's is
  5 MB of base64, checked before encoding; agy's stays 5 MB of file.
- **The mode chip and the saved mode drifted** when Claude left plan mode itself: the chip still said Plan, choosing
  Plan again did nothing, and a restart resumed in plan mode. Claude's reported mode is now followed (R9).
- Small: a failed version read is tried again (Move could stay hidden for the app's run); Return sends on a `/` list
  that couldn't be read (blocked only while it loads); the folder picker no longer checks folders on the disk on every
  keystroke (a check off the UI thread, kept with the open picker); STRUCTURE.md's four new rows aligned; this note's
  Limits, on-screen items 6, 7 and 11, and the plan's front-matter and steps.

After the fixes: `cargo test --bin fuxx` 1,944 passed (1 ignored); clippy 14 / 27 and 16 / 29 (no new warnings);
`cargo fmt --check` clean; full `scripts/ui-shots.sh` 191 passed, 0 failed (two new scenes:
`agent-box-mode-follows-claude`, `agent-box-slash-failed-return-sends`); no stray `claude -p` / `agy -p` /
`fuxx __ui-shots` process after.

## Related

[[Proposal - Chat View and Agent Tabs]] · [[79 - Milestone 11aa - Agent Tabs]] · [[Proposal - File View Upgrades]] ·
[[82 - Milestone 11af - Live Changes]] · [[12 - Scope]] · [[40 - Roadmap]]
