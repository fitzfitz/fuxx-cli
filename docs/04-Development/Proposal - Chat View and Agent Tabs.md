---
tags: [development, proposal, design]
status: proposal
---

> [!warning] Proposal — not yet in the plan (2026-11-13)
> Designed with you in a brainstorm on 2026-11-13. Per AGENTS.md rule 3 it is **not in scope** until
> [[12 - Scope]] and [[40 - Roadmap]] are changed to include it. That edit (and a Progress Log entry) is left for
> later on purpose: another agent is building on `master` right now and owns those files. Nothing here has been
> built. The mockups are in `.superpowers/brainstorm/65655-1794524053/content/` (gitignored).

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
in `~/Kerja/proj/weds`) produced, in order: `system/hook_started` ×4 and `system/hook_response` ×4 (your
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

## Part 1 — Agent tabs and providers

### 1.1 The agent tab

- `TabKind` gains an agent variant (shape is the builder's; it needs at least the provider name and the
  conversation id for resume).
- **Tab bar:** a small provider badge instead of `>_` (`CC` for Claude, `AG` for agy; colour per provider), the
  tab's title (AI's call: the first line of your first message, renameable like any tab), and the status mark.
- **Layout (from `c1-agent-tab.html`):** conversation stream on top, composer at the bottom.
  - **Your messages:** right-aligned card. Attachments show as a chip under it.
  - **Agent messages:** provider avatar + text, streamed as it arrives (partial messages). Markdown basics: paragraphs,
    inline code, code blocks, lists. Links clickable.
  - **Plan/todo:** when the agent publishes a todo list, show it as a checklist that updates in place (✓ / ◐ / ○).
  - **Tool calls:** one row each: tool name (accent colour), its target (file, command), a result summary at the
    right (`84 lines`, `✓ 4 passed`, `+3 −1`). Click to expand: edits expand to a diff, Bash to its output, reads to
    nothing (AI's call). Running tool: spinner + elapsed time.
  - **Subagents** (`parent_tool_use_id` set): nested under the tool row that started them, collapsed by default
    (AI's call).
  - **Hook output** (`system/hook_*`): hidden by default; failures show as a quiet warning row (AI's call).
  - **Errors / stops:** an error row in the theme's red, with the provider's message.
- **Top line of the pane:** status word (working / needs you / idle) and context used (`context 18%`), from usage
  events.
- **Composer:** a multi-line text box (Enter sends, ⇧Enter newline). Chips above it: model, branch, cost so far.
  Hints: `/` for commands (sent as text), `@` for files (AI's call: a file picker from the workspace folder that
  inserts the path). While the agent works, the button is **■ Stop (Esc)** and anything you send is queued for the
  agent's next turn.
- **Empty state:** a short line ("Ask Claude to do something in ~/K/p/weds") and the composer focused.

### 1.2 Permission prompts

- **Claude (verified 2026-11-13 while planning M11aa):** start with the hidden flag `--permission-prompt-tool
  stdio` and send `{"type":"control_request","request_id":…,"request":{"subtype":"initialize","hooks":null}}`
  before the first message. Each prompt then arrives as `control_request` / `can_use_tool` (`tool_name`,
  `display_name`, `input`, `description`, `permission_suggestions`, `tool_use_id`), answered with
  `{"type":"control_response","response":{"subtype":"success","request_id":…,"response":{"behavior":"allow",
  "updatedInput":…}}}` — a test Write went through. **Without the initialize request, `--permission-prompts host`
  alone asks no one and denies every prompt** (`system/permission_denied`). Deny and "always allow" responses are
  recorded in M11aa Task 0. Your own hooks (e.g. GateGuard) run in these sessions too; a hook that blocks a tool
  arrives as a failed tool result.
- **The card** (`c1-agent-tab.html`, right): amber border, "Claude wants to run", the exact command / file / diff,
  buttons **Allow ⏎ · Always allow <thing> · Deny**. Typing in the composer and sending = **Deny with that text as
  the reason**. "Always allow" writes the rule the way the provider's own "always" would (for Claude: the permission
  response's update, not fuxx's own settings).
- **agy (AI's call):** since it has no host routing, an agy tab starts in a mode you choose: a mode chip on the
  composer with `accept-edits` (default) / `auto` / `plan`. No permission cards for agy until it supports routing.
  Never `--dangerously-skip-permissions` by default.
- When a prompt is waiting: the tab's status is **needs you**, the sidebar ring turns red and fuxx notifies you,
  exactly like today's waiting state.

### 1.3 Providers (`src/providers/`)

- **`mod.rs`** — the shared language and nothing agent-specific:
  - a provider description: name, badge text/colour, the command it starts, capabilities (can route permissions,
    can resume, supports partial streaming, supports modes);
  - **fuxx events** the chat view consumes, at minimum: session started (id, model, cwd), text delta, message done,
    tool started (id, name, summary, parent), tool finished (id, ok/err, summary, optional diff/output), todo list,
    permission request (id, what, details), usage (context %, cost), turn done, error, process ended;
  - **fuxx actions** the view sends: user message (text + attachments), permission answer (allow / always / deny +
    reason), stop / interrupt, close.
- **`claude.rs`** — starts `claude -p --input-format stream-json --output-format stream-json --verbose
  --include-partial-messages` (+ `--resume <id>` when reopening, + the permission flag from 1.2) in the tab's folder,
  and translates both ways.
- **`agy.rs`** — starts `agy -p --input-format stream-json --output-format stream-json` (+ `--conversation <id>`,
  `--mode`), and translates both ways.
- **Unknown events are ignored, not fatal** (agents update often). A provider that can't parse its agent's output
  shows one error row and offers "Open in classic view".
- **Adding OpenCode or Codex later = one new file + one line registering it.** Nothing else should change; if it
  would, `mod.rs` is wrong.

### 1.4 Presets and the menu

- A preset is backed by a provider when its **name** matches a registered provider (AI's call).
- **Picking it (menu or ⌃⌘N) opens a new workspace split in two (your choice, 2026-11-13, after the builder
  check):** a shell tab on one side and the agent tab on the other, both in the workspace's folder (the option 2
  mockup, `tabs-and-threads.html`). The sidebar row keeps reading its branch, folder and ports from that shell, so
  the sidebar, branch bar and Changes panel need no rework. The row's agent label and status come from the agent
  tab.
- **The tab bar's `+` can also add an agent tab** to any pane (AI's call on how: the same preset list, showing only
  presets with a provider).
- **`agy` becomes a fourth built-in preset** (built-ins: `claude`, `agy`, `opencode`, `codex`; AI's call on the
  order). `opencode` and `codex` keep today's behaviour.
- In the menu, presets that open a chat view are marked (AI's call: the provider badge before the name).
- A user `agent-preset = claude: claude --model sonnet` still matches the `claude` provider by name; the extra
  arguments are passed through (AI's call; builder decides which flags are safe to pass and which fuxx owns).
- **Classic view stays one keystroke away:** typing `claude` / `agy` in a shell tab runs the TUI exactly as today.
  An agent tab's `···` menu has **Open in classic view**, which opens a shell tab running
  `claude --resume <id>` / `agy --conversation <id>` (AI's call).

### 1.5 Status, notifications, activity

- The provider's events set the tab's status directly, **using the existing `Status` enum, no new variant (AI's
  call, after the builder check)**: tool running / streaming = `Working`; permission request = `Waiting`; turn done
  = `Done`; nothing yet = `Idle`. An error or a non-zero process exit = `Done` plus an error row in the tab and an
  Activity note — no fifth "failed" status, so the API, rings and notifications don't change. This replaces
  `StatusTracker`'s heuristics **for agent tabs only**; terminal tabs keep `StatusTracker`.
- Same ring colours, same notification path, same Activity entries as today.

### 1.5b Scripting API and MCP (AI's call, after the builder check)

- Agent tabs **are listed** by `fuxx status` and count for `fuxx wait --any --for waiting|done`, so launching
  `claude` from the menu doesn't drop out of existing automation. They need a scripting id alongside the terminals'
  `sN` (shape left to the builder).
- `fuxx question` on an agent tab returns the pending permission request; `fuxx answer` answers it (allow / deny
  + reason). Conversation text stays unexposed (Non-goals).

### 1.6 Lifetime and restore (AI's calls)

- The agent process is a **child of the app**, not of the background service (M10a). Quitting fuxx ends it.
- The tab remembers its provider and conversation id. On restore (M11u), it reopens with `--resume` /
  `--conversation` and shows the history the agent replays, or "Conversation restored" if it replays none.
- **Classic sessions keep restoring as terminals (AI's call, after the builder check).** Today `restore.rs` and
  `presets::agent_for` look presets up by name, and M11u's `agent` field records an agent typed at a prompt. Those
  keep meaning "a terminal that runs this command". Agent tabs are saved as their own tab kind (provider +
  conversation id), and only that kind restores as a chat view. So the menu's lookup ("does this preset have a
  provider?") and restore's lookup ("what did this terminal run?") are separate. A saved session from before this
  change, or a `claude` typed in a shell, comes back exactly as today.
- **Local only (AI's call, after the builder check).** Agent tabs run on this Mac. In an SSH workspace (M10b),
  provider presets behave as classic presets (the command is typed into the remote shell, as today) and the menu
  shows no chat badge there.
- Closing an agent tab while it's working asks first (like closing a busy terminal today).

### 1.7 Handing shell output to an agent

- Drag a command block (M11l) from a shell tab onto an agent tab: it's attached to your next message as
  "command + exit code + output" text (AI's call; replaces the ⌘L of the C1 mockup, which assumed paired threads).
- Plain text dropped or pasted into the composer works as normal text.

### 1.8 Part 1 order (for the plan)

1. **Verify before building — a go/no-go gate (AI's call, after the builder check):** record a real Claude stream
   with tool calls, a partial message, a permission request in host mode (answer it, including an "always allow"),
   a typed `/compact`, and `--resume`. Do the same for agy (if its login is set up). Write what you saw into this
   note. **If Claude can't route permission prompts to fuxx well enough for the card (goal 4), stop and bring it
   back to you** rather than quietly shipping Claude like agy.
2. `providers/mod.rs` + `claude.rs` with recorded streams as test fixtures (pure, GPUI-free, unit-tested like
   `presets.rs`).
3. The agent tab view (stream, composer, tool rows, permission card) for Claude.
4. Presets/menu, status/notifications, restore.
5. `agy.rs` against its recorded fixtures.

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

## Left to the builder

- Exact Rust types and module layout inside `src/providers/` (beyond one file per agent + `mod.rs`).
- Which preset arguments pass through and which flags fuxx owns.
- Markdown rendering approach in GPUI and how much of it.
- Colours and sizes, from the existing design tokens (`ui_tokens.rs`), held to the contrast floor M11 set.
- Choosing between `--permission-prompts host` and `--permission-prompt-tool` after verification.

## Related

- [[12 - Scope]] · [[40 - Roadmap]] · [[63 - Milestone 11l - Command Blocks]] · [[Input - Agent Sessions Research]]
  (2026-10-27, "not yet"; its F1 "Agent Bridge" and F7 "Resume" overlap with this proposal).
