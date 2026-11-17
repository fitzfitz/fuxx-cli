---
tags: [development, proposal, design]
status: proposal
---

> [!info] Part 1 is now note 79 (2026-11-17). In the plan as M11aa (Part 1) / M11ab (Part 2), 2026-11-13
> You approved it on 2026-11-13. **Part 1 is M11aa — Agent tabs and providers** ([[M11aa Plan - Agent Tabs]]) and
> **Part 2 is M11ab — Chat-style shell tabs** (design first, not planned), both in [[12 - Scope]] and [[40 - Roadmap]],
> **after M11z and after M11p**. The safe part of M11aa (Tasks 0, 2, 3, 4, 6: new pure modules, no UI) is being built
> on branch `m11aa-agent-tabs`; its UI tasks wait for M11p. The mockups are in
> `.superpowers/brainstorm/65655-1794524053/content/` (gitignored).

# Proposal - Chat View and Agent Tabs (design)

## Goal

fuxx's pane stops looking like a raw scrollback stream. Agents get a real **chat view**: fuxx starts the agent
itself in its machine-readable mode and draws its replies, tool calls, diffs and permission prompts as a
conversation, with buttons where the TUI had keystrokes. Shell tabs get a **chat-style** look too: each command
reads like a message, with its output underneath. Each agent's wiring lives in **its own provider file** so new
agents are one file each. The first two providers are **Claude Code** and **Antigravity (`agy`)**.

## Why (your words and choices, 2026-11-13)

- The idea: "turn this terminal that basically our core to be a chat like UI terminal."
- From the mockups you chose direction **C** (the chat is about the *agent*, not just prettier shell output),
  then **C1** (shell and agent are separate conversations, not one mixed timeline).
- "fuxx can launch Claude itself."
- "Start with claude and agy first. I want them to have their own providers in a separate file so it's easy to
  maintain… therefore, this [the preset menu: claude ^⌘1, opencode ^⌘2, codex ^⌘3] now becomes meaningful."
- Picking **claude** from the menu opens the **chat view**. For the classic TUI you type `claude` / `agy` in a shell.
- Tabs, not an in-tab switch: you chose **option 2, each tab is a shell *or* an agent**, and agreed the in-tab
  Shell/Agent switch (option 1) was overkill. Working beside an agent = split the pane.
- You asked whether this is "the same harness". It is (checked, below), which is why the chat view drives the real
  `claude` rather than a re-implementation.

## Goals

1. A new tab kind, the **agent tab**, that shows one agent conversation as a chat (mockup `c1-agent-tab.html`).
2. **Providers:** one file per agent under `src/providers/`, all speaking one fuxx event language. Claude and agy
   first.
3. The preset menu means something: presets with a provider open an agent tab; the rest behave exactly as today.
4. Permission prompts become an **Allow / Always allow / Deny** card; Enter allows, typing a reply denies with a reason.
5. The agent tab's status (working / needs you / done / failed) drives the sidebar ring and notifications **from
   the provider's events**, not screen heuristics.
6. Shell tabs get the chat-style look (mockup `c-shell-pov.html`, C1), as **Part 2** (see "Two parts").

## Non-goals (for this proposal)

- OpenCode and Codex providers. They stay classic terminal presets until someone writes their provider file.
- More than one conversation per tab. More conversations = more tabs or splits.
- Rebuilding Claude Code's interactive pickers and screens (`/model`, `/config`, `/login`, `/resume` picker, the
  status line, its own key bindings). Typed slash commands that are just messages still go through (to verify).
- A shared cross-agent protocol (Zed's Agent Client Protocol). Worth researching later; not a dependency here.
- Exposing agent-tab conversations to the scripting API, MCP or `fuxx text`.
- A mixed shell-and-agent timeline (that was C2; you chose C1).

## Anti-goals (would be a failure even if it works)

- **The classic terminal gets worse.** Typing `claude` in a shell tab, full-screen programs, selection, links,
  mouse reporting and scrolling must behave exactly as today.
- **Scraping a TUI's screen** to build the chat. Agent tabs use the agent's structured output or they don't exist.
- **A shell chat view that loses zsh.** Tab completion, history search (⌃R), autosuggestions, vi mode and your
  prompt theme must keep working in shell tabs.
- **Agent-specific code leaking out of its provider file.** The chat view, tab, status and menu code never name a
  specific agent.

## What exists today (checked 2026-11-13)

- **Tabs** (`layout_model.rs:29`): `TabKind` is `Terminal(u64)`, `Browser { url }` or `File { path, mode }`. Tabs live
  in panes, panes in nested splits (M8), workspaces in the sidebar. File and browser tabs already prove a tab can be
  something other than a terminal.
- **Presets** (`presets.rs`): `Preset { name, command }`; built-ins are `claude`, `opencode`, `codex`; settings lines
  `agent-preset = name: command` add, replace or remove; max 9 (⌃⌘1–9). A preset's command is **typed into** the
  new shell (`start_in` → `startup`). `agent_for(program, presets)` maps a running program back to its preset.
- **Agent state** (`status.rs` `StatusTracker`): inferred from output timing, bells, notifications and screen tail
  text. **Activity** (`activity.rs`) keeps status changes per terminal.
- **Command blocks** (M11l, [[63 - Milestone 11l - Command Blocks]]): fuxx's zsh integration sends OSC 133 marks;
  `shell_marks.rs` tags grid rows; blocks are an **overlay** on alacritty's grid (gutter bar + hover chip). M11l
  explicitly left out collapse because folding rows means "a second terminal view" (view-row ↔ grid-line map through
  paint, hit-testing, selection, links, mouse reports, wheel, cursor). Remember this for Part 2.
- **Persistent sessions** (M10a, `daemon_session.rs`): a background service owns each terminal's PTY and `Term`.
- **Scripting / MCP** (M9): `fuxx mcp` is an MCP server fuxx already ships.

### Claude Code headless mode (checked on this Mac, Claude Code 2.1.323)

Flags present in `claude --help`: `-p/--print`, `--input-format stream-json`, `--output-format stream-json`,
`--include-partial-messages`, `--verbose`, `--permission-prompts <target>` ("host" = the SDK host or
`--permission-prompt-tool`, or "none"), `--resume`, `--session-id <uuid>`.

A real run (`echo '{"type":"user",…}' | claude -p --input-format stream-json --output-format stream-json --verbose`
in `~/code/my-app`) produced, in order: `system/hook_started` ×4 and `system/hook_response` ×4 (your
SessionStart hooks), `system/init`, `assistant`, `rate_limit_event`, `result/success`.

- `system/init` reported: model `claude-opus-5-5`, `permissionMode: default`, `apiKeySource: none` (your
  subscription login), 29 tools, 35 MCP servers, 538 slash commands, 420 skills, 77 agents, 13 plugins. **Same
  harness** as the interactive CLI.
- `assistant`: `message.content[]` items by `type` (here `text`), plus `session_id`, `parent_tool_use_id`, `uuid`.
- `result`: `subtype`, `is_error`, `num_turns`, `total_cost_usd`, `session_id`, `permission_denials`, `usage`,
  `modelUsage`, `stop_reason`, and more.
- **Not yet observed:** tool-use / tool-result events, partial-message events, the permission request/response
  exchange in "host" mode, and whether typed slash commands work over stream-json input. The builder verifies
  these first (Part 1, step 1).

#### Recorded 2026-11-13 (M11aa Task 0)

Fixtures: `src/providers/fixtures/claude-*.jsonl`, recorded by `scripts/record-agent-fixture.py` (argv and the
`fuxx-init` initialize request exactly as in the plan's Global Constraints; **no hooks switched off**, so your
GateGuard hook is in every recording). Claude Code 2.1.323, model `claude-opus-5-5`. **Go/no-go: GO.** Allow and
deny both work.

- **Allow works.** `can_use_tool` request → `control_response` with `behavior: "allow"` + `updatedInput` → the tool
  ran (`tool_result` "File created successfully"). `claude-permission-allow.jsonl`.
- **Deny with a message reaches the model.** `{"behavior":"deny","message":"Not now"}` → the next event is a `user`
  `tool_result` with `content: "Not now"`, `is_error: true`; the model then said the write "came back with 'Not
  now'" and did not create the file. `claude-permission-deny.jsonl`.
- **Prompt shape.** `control_request` / `request.subtype: "can_use_tool"` with `tool_name`, `display_name`, `input`,
  `description` (short, e.g. `hello.txt`), `permission_suggestions`, `server_policy_ask_only: true`, `tool_use_id`.
  The tool_use `assistant` event arrives just before it.
- **"Always allow" = send the request's own `permission_suggestions` back as `updatedPermissions`.** What the
  suggestion is depends on the tool: **Write and Edit** offer `[{"type":"setMode","mode":"acceptEdits","destination":
  "session"}]` (a mode switch, not a per-file rule); **Bash** offers `[{"type":"addRules","rules":[{"toolName":
  "Bash","ruleContent":"<the exact command>"}],"behavior":"allow","destination":"localSettings"}]` (written to the
  project's local settings file). With the Write suggestion echoed back, the **next Write (a different file) was
  not prompted**: the whole session has one `control_request`, yet both files were created.
  `claude-permission-always.jsonl`. The card's button label should come from the suggestion's `type`/`rules`.
- **Hooks.** A blocking hook is a `user` `tool_result` with `is_error: true` and content starting
  `PreToolUse:<Tool> hook error: [Fact-Forcing Gate]…`; the model retries the same call. `result.permission_denials`
  lists the hook-blocked tool_use ids too, so it is **not** a record of the user pressing Deny. `system/hook_started`
  and `hook_response` ×4 (SessionStart) precede `init`; in the `claude-text` recording these arrive before the
  initialize `control_response`, which itself is large (the `commands` list, ~220 KB) and also carries `account`
  (scrubbed in the fixtures).
- **Partial messages.** With `--include-partial-messages`, `stream_event` wraps the raw API events: `message_start`,
  `content_block_start`, `content_block_delta` (`delta.type` is `text_delta`, `thinking_delta`, `signature_delta`
  or `input_json_delta` with `partial_json`), `content_block_stop`, `message_delta`, `message_stop`. The complete
  block still arrives later as an `assistant` event (one per content block, same `message.id`). Extra `system`
  subtypes seen: `status` (`requesting`), `thinking_tokens`, `task_started` / `task_notification` /
  `task_updated` / `background_tasks_changed` (Bash runs), `compact_boundary`; plus `tool_progress` heartbeats
  (`elapsed_time_seconds`, every 30 s) and `rate_limit_event`.
- **Tool events.** `assistant` `content[]` `tool_use` (`id`, `name`, `input`, `caller`) ↔ `user` `tool_result`
  (`tool_use_id`, `content` string or array, `is_error`). A `user` event also carries a top-level
  `tool_use_result` with structured data (Read: `{type:"text", file:{filePath, content, numLines, startLine,
  totalLines}}`). Seen tool names: Read, Edit, Write, Bash, ToolSearch. **TodoWrite did not exist** in this
  session (`init.tools` has no Todo/Task-list tool; `ToolSearch` found none), so the TodoWrite shape is **not
  observed**. Edit/Write prompt in `default` mode; Read and the hook-cleared Bash `ls` did not.
- **Interrupt.** `{"type":"control_request","request_id":"fuxx-stop","request":{"subtype":"interrupt"}}`:
  - *While the model streams text* (`claude-interrupt-text.jsonl`): immediate `control_response` `success`
    (`response: {"still_queued": []}`), then a `user` event with text `[Request interrupted by user]`, then
    `result` with `subtype: "error_during_execution"`, `is_error: true`, `terminal_reason: "aborted_streaming"`.
    The process stays alive for the next message.
  - *While a foreground Bash tool runs* (`claude-interrupt.jsonl`, one run): **the interrupt did not stop the
    tool.** The 60 s command ran to completion, the model finished its turn, and the interrupt's `control_response`
    arrived only at the very end (after `result`). So Esc during a long tool may not act until the tool ends; the
    UI must not claim "stopped" until the `control_response` / `result` shows it. One recording only, and the
    ordering of the `control_response` relative to `result` is a single observation (the recorder at that time
    could only capture it because shutdown waited for stragglers); worth a second look in M11aa.
- **A second user message while a turn runs is accepted and queued** (`claude-queue.jsonl`): sent ≥4 s into a
  20 s Bash tool, it produced no event until the first turn's `result`, then a second `system/init` and a normal
  second turn + `result`. `queued_turn_count` was `0` on the first `result` (and on every result seen), so it
  does **not** reflect this queue in this version; **a non-zero value was not observed**. Each turn emits its own
  `system/init`.
- **`--resume <id>`** (`claude-resume.jsonl`): **does not replay history on stdout.** The recording begins with
  the new turn's events only (`assistant` ×2, `result`; the `session_id` is unchanged) and the model knew the
  earlier turn ("Hello"). The history is on disk: `~/.claude/projects/<cwd with / and . replaced by ->/<session_id>.jsonl`
  (e.g. `-private-var-folders-…-T-tmp-XXXX/<id>.jsonl`; the cwd is the realpath). Sessions are per working directory.
- **`/compact`** (`claude-slash.jsonl`): typed over stream-json it **runs**: `system/compact_boundary`
  (`compact_metadata`: `trigger: "manual"`, `pre_tokens`, `post_tokens`, `duration_ms`), a `user` event with the
  continuation summary, a `user` event with `<local-command-stdout>Compacted …`, then a second `result`
  (`success`, `num_turns: 0`). Slash commands work.
- **`bad-resume`** (`claude-bad-resume.jsonl`): stdout is one line, `result` with `subtype:
  "error_during_execution"`, `is_error: true`, `num_turns: 0`, `session_id` = the id given, `errors: ["No
  conversation found with session ID: 0000…"]`; the same sentence on stderr; the process exits with code 1. It
  prints no `system/init` and no response to the initialize request.
- **Context window.** `result.modelUsage.<model>.contextWindow` (here `1000000`; also `maxOutputTokens`,
  `costUSD`, `inputTokens`, `cacheReadInputTokens`, `cacheCreationInputTokens`). The `context N%` line = the last
  turn's input + cache tokens ÷ `contextWindow`. `result` also has `total_cost_usd`, `num_turns`, `duration_ms`,
  `terminal_reason: "completed"`, `credential_expires_at`.

**agy 1.4.3** (`src/providers/fixtures/agy-*.jsonl`; **provider is a later task**). The Global Constraints argv
**does not work as written**: `agy -p --input-format …` makes `-p` swallow `--input-format` as its prompt (exit 2).
`-p=` (an empty prompt attached with `=`) works. **agy's input is not Claude's:** each line must be
`{"event":"user","message":{"content":"<text>"}}` (`"role"` in `message` is rejected; a missing `event` field is an
error). Its output is its own schema, **not** Claude's stream: `{"event":"init","conversation_id":…,"init":{"cwd",
"tools":[…],"permission_mode"}}`, `{"event":"step_update","step_update":{"conversation_id","step_index","state":
"ACTIVE"|"DONE","step_type":"user_input"|"agent_response"|"error_message","text_delta"?,"duration_seconds"?,
"usage"?}}`, and `{"event":"result","result":{"conversation_id","status":"SUCCESS"|"ERROR","response","error"?,
"duration_seconds","num_turns","usage","finished_reason"?}}`; errors also print `error: …` / `AGY_ERROR: {…}` lines
on stderr. A mid-session ad-hoc run (not saved) streamed `text_delta` "Hello" then "\n" and ended `status:
"SUCCESS"`. **All saved agy recordings are failures: after that run the account answered "Your AI credits balance
is too low to continue." (429 RESOURCE_EXHAUSTED)**, so `agy-text`, `agy-tools` and `agy-resume` show the
init → user_input → error_message → result(ERROR) sequence only; **tool events and resume on a real agy turn are
not observed.** Re-record when credits are available. The conversation id is `init.conversation_id` and every
`step_update` / `result`.

**agy 1.5.7, re-recorded 2026-11-17 (M11aa Task 12).** All three scenarios succeeded with the same argv
(`-p= … --mode accept-edits`); the Task 0 credit error is kept as `agy-credits.jsonl`. **text:** `user_input` →
`agent_response` `ACTIVE` with `text_delta: "Hello"` → `DONE` with `text_delta: "\n"` and per-step `usage` →
`result` `SUCCESS` (`response: "Hello\n"`). **tools:** each tool is one `step_type: "tool"` step, `ACTIVE` then
`DONE`, with `tool_name` (`read_file`, `edit_file`, `run_command`, `write_file`; no TodoWrite: agy writes its task
list to `~/.gemini/antigravity-cli/brain/<conversation>/task.md`) and `tool_info: {name, parameters: {Path |
CommandLine}, output?}`; `read_file`'s output is a size ("1 lines, 6 bytes"), `run_command`'s is a header
(`Created At`, `Completed At`, `Command exited with code N.`) then `Command output:` and what it printed; `edit_file`
carries no old/new text (no diff). Between tools, `agent_response` steps that only plan arrive `DONE` with no text.
No permission prompt arrived in `accept-edits` (an `ls` ran unasked). **resume** (`--conversation <id>`): same
`init.conversation_id`, `step_index` goes on from the earlier run (2, 3 `system_message`, 4), **no replay of history
on stdout**, and the model remembered the earlier turn. **bad-resume** (`--conversation 00000000-…`, fix round 1): agy
prints `warning: conversation "<id>" not found` on stderr and goes on in a **new** conversation (a new
`init.conversation_id`, exit 0), so the tab replaces "Conversation restored" with the bad-resume notice when the
agent's reported id differs from the one it resumed. Not observed: a way to interrupt a turn on stdin (agy's
`ProviderInfo.interrupts` is false: Stop offers Force stop at once), a model name. Fragments of one step's `text_delta` can split a path, so the recorder scrubs a
step's joined text (`rejoin_agy_text`).

### Antigravity CLI (`agy` 1.4.3, `~/.local/bin/agy`, checked from `--help` only)

- `-p/--print`, `--input-format text|stream-json` ("reads one NDJSON message per line from stdin and runs a turn for
  each; requires `--output-format stream-json`"), `--output-format text|json|stream-json`.
- `--continue`, `--conversation <id>` (resume), `--model`, `--effort`, `--mode accept-edits|auto|plan`,
  `--sandbox`, `--dangerously-skip-permissions`, `--project`, `--add-dir`.
- **No flag to route permission prompts to a host** was found. Its event schema has not been seen yet; it may or may
  not match Claude's.

## Two parts (AI's call)

The agent tab is a **new view fed by JSON**: no terminal emulation, no change to the grid. The chat-style shell tab is
**the "second terminal view" M11l refused** for collapse. They carry very different risk, so:

- **Part 1 — Agent tabs and providers.** Ships on its own and delivers the core of the idea.
- **Part 2 — Chat-style shell tabs.** Starts after Part 1 is on screen, with its own detailed design (like M11l had)
  before any code. This proposal fixes Part 2's intent and constraints, not its internals.

## Part 1 — Agent tabs and providers (moved)

Part 1 is built as M11aa. Its design now lives in [[79 - Milestone 11aa - Agent Tabs]] ("The design"), with what was
built, the limits and the on-screen check. The recorded research above stays here.

## Part 2 — Chat-style shell tabs (intent and constraints only)

What you saw in `c-shell-pov.html` (C1): each command is a message from `zsh` with the command on top, its output in
a panel below, and a header with time, duration and exit code; failed output outlined in red; long output folded
behind "▸ N more lines"; Copy / Rerun; cwd and branch as chips; a composer at the bottom; when a program asks for
input, your typing goes to that program.

Constraints the Part 2 design must meet:

- **Built on M11l's marks**, not a new way of finding commands.
- **Keep zsh's line editor (AI's call, the most important one to check with you).** A fuxx-owned text box that
  sends lines to zsh would lose completion, ⌃R, autosuggestions and vi mode (the anti-goal). Preferred direction:
  finished blocks render as message cards above, and the live prompt stays a real terminal strip styled as the
  composer.
- **Full-screen programs (alternate screen) and agent TUIs** take over the whole pane as the raw grid until they
  exit, then the chat view returns.
- **No marks (bash, fish, `zsh -f`, SSH) = today's terminal**, unchanged.
- Selection, copy, links and Rerun must work inside cards; mouse reporting only matters for the live part.
- It ships behind a setting so the plain grid stays one toggle away (AI's call).
- Folding long output is in scope here (it was out of M11l) because the card view isn't the grid itself; the Part 2
  design must say exactly how cards relate to grid lines.

## Where it goes (your call with AI's recommendation, 2026-11-13)

You asked for it to stay in M11 ("M11aa maybe? you tell me best"). Recommended:

- **M11aa — Agent tabs and providers** (Part 1).
- **M11ab — Chat-style shell tabs** (Part 2), designed on its own before any code.
- **Order: after M11z and after M11p.** M11p reworks how the sidebar and tab-bar chrome is drawn (caching), which
  is exactly where the agent tab plugs in; building M11aa on top of it avoids doing that wiring twice.
- The [[12 - Scope]] / [[40 - Roadmap]] edits and a Progress Log entry are made once you approve this note and the
  other agent on `master` is between tasks.

## Settled (your decisions)

- Chat view for agents is the core; C1 separate conversations; one kind per tab (option 2).
- The menu's claude / agy open a new workspace with a shell and the agent tab side by side.
- fuxx starts the agent itself in its structured mode.
- Claude and agy first, each in its own provider file.
- Menu presets with a provider open the chat view; classic = type the command in a shell tab.

## AI's calls you may want to overrule

1. Two parts, agent tabs first; shell chat view later with its own design.
2. Shell chat keeps zsh's own line editor as the composer (Part 2).
3. agy runs in a chosen mode (`accept-edits` default) with no permission cards until it can route them.
4. Provider match by preset **name**; `agy` added as a fourth built-in.
5. Drag a command block onto an agent tab to attach it (no ⌘L).
6. Agent processes are app children (not persistent through quit); restore via resume.
7. "Open in classic view" in the tab menu.
8. Hook output hidden unless it fails; subagents collapsed under their tool row.
9. No new "failed" status: errors map to `Done` + an error row (1.5).
10. Agent tabs visible to `fuxx status` / `wait` / `question` / `answer`; conversation text not exposed (1.5b).
11. Classic sessions restore as terminals; only saved agent tabs restore as chat (1.6).
12. Agent tabs are local only; SSH workspaces keep classic presets (1.6).
13. Part 1 step 1 is a go/no-go gate on permission routing (1.8).

## Plan-time calls (AI's, made while reading the code for the M11aa plan, 2026-11-13)

1. **"+ adds an agent tab" becomes the pane `···` menu:** the tab bar's `+` has no menu today
   (`BarButton::NewTerminal`); `PaneAction` gains one "New ⟨Label⟩ tab" item per provider (and the ⌘K palette lists
   them). `+` stays "New terminal tab".
2. **Closing a busy agent tab asks** (Working or Waiting). The spec said "like closing a busy terminal today", but
   terminals never ask (`close_tab`: "Never asks"); this is new behaviour, for agent tabs only.
3. **A banner for an agent tab has no click target** (`banner_about(None)`): clicking it brings fuxx forward
   without focusing the tab. Click-to-tab needs `native_notify::Terminal` widened; left for later.
4. **The sidebar row's agent label** for a workspace with an agent tab is the provider's label ("Claude"),
   taken from the agent tab — `agents_now` is never written for it (that would make "agents" restore re-type
   `claude` into the shell).
5. **Long conversations** render through `gpui::list` (only visible items are laid out). Tool output kept per row
   is the **last 2,000 lines / 256 KiB**, with a "… N earlier lines" row.
6. **agy's mode** is chosen on the composer chip **before the first message** and saved with the tab; after that
   the chip is read-only (changing it would restart the process).
7. **Resume history:** if `--resume` doesn't replay earlier messages on stdout (Task 0 checks), `claude.rs` reads
   the conversation's transcript file (path found in Task 0) for the history; if neither works, the tab shows a
   "Conversation restored" notice and only new messages.

## Left to the builder

- Exact Rust types and module layout inside `src/providers/` (beyond one file per agent + `mod.rs`).
- Which preset arguments pass through and which flags fuxx owns.
- Markdown rendering approach in GPUI and how much of it.
- Colours and sizes, from the existing design tokens (`ui_tokens.rs`), held to the contrast floor M11 set.
- Choosing between `--permission-prompts host` and `--permission-prompt-tool` after verification.

## Related

- [[12 - Scope]] · [[40 - Roadmap]] · [[63 - Milestone 11l - Command Blocks]] · [[Input - Agent Sessions Research]]
  (2026-10-27, "not yet"; its F1 "Agent Bridge" and F7 "Resume" overlap with this proposal).
