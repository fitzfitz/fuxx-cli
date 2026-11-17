# Milestone 11aa — Agent Tabs and Providers

**Status:** 🟨 built 2026-11-17 on branch `m11aa-ui` (from master; the safe part, Tasks 0, 2, 3, 4 and 6, was built on
2026-11-13 and is already on master), every task reviewed, whole-branch review and fix wave clean, **merged to master and pushed 2026-11-17 (`250faeb`)**, waiting for your on-screen
check (below). Plan [[M11aa Plan - Agent Tabs]], 14 / 14 tasks, then a whole-branch review and its fix wave. At the
branch tip: 1,787 tests pass, clippy 14 / 27 (default) and 16 / 29 (`ui-shots`) = the baselines, `cargo fmt --check`
clean, full `scripts/ui-shots.sh` 137 scenes passed, 0 failed, in 91.8 s.

**Asked for:** 2026-11-13, by you: a chat view for agents. Picking **claude** or **agy** opens a workspace with a shell
and an agent tab side by side. The design was chosen with you in mockups (direction C, then C1: one kind of tab per
agent) and approved the same day. It is below, under "The design". Part 2 of the old proposal (chat-style shell tabs)
is M11ab and stays in [[Proposal - Chat View and Agent Tabs]].

## Goal

Picking **claude** or **agy** opens a workspace with a shell and an **agent tab** side by side. The agent tab runs the
agent in its machine-readable mode and shows the conversation as a chat: streamed text, tool rows that open to diffs or
output, a todo checklist, an Allow / Allow all edits / Deny card. The tab's status drives the rings, marks,
notifications, Activity and the scripting API. Typing `claude` in a shell is still the classic TUI, and nothing about
that changes.

## What was built

By task. Each agent keeps its own knowledge in its own provider file.

1. **Task 0: record real output (go / no-go).** Real Claude and agy runs, saved as fixtures with personal text scrubbed.
   Result: **GO**. Claude's Allow and Deny both work with `--permission-prompt-tool stdio` plus an `initialize`
   request. Findings are in the proposal.
2. **Task 1: presets.** The built-in presets are now claude, agy, opencode, codex. A preset's extra words reach the
   agent's command line.
3. **Task 2: providers** (`src/providers/`). One event language (text, tools, todos, permission asks, usage, turn
   done) and one set of actions back (send, allow, always allow, deny, interrupt). `claude.rs` translates Claude's JSON
   lines. `agy.rs` (Task 12) translates agy's.
4. **Task 3: the conversation model** (`agent_conversation.rs`, `agent_markdown.rs`). Events fold into chat items and a
   status (working, needs you, done, idle). Message text is parsed as Markdown.
5. **Task 4: the process** (`agent_process.rs`). The agent runs as a piped child in its own process group, with a reader
   thread. Long lines, split reads, exit codes and kill are handled and tested.
6. **Task 5: the tab in the layout** (`TabKind::Agent`, `SavedTab::Agent`). It carries the provider, the folder, the
   conversation id and mode, and the title. Two fresh Claude tabs in one folder both restore. An unknown provider
   restores as an error tab.
7. **Task 6: the composer** (`composer.rs`). A multi-line input.
8. **Task 7: the agent tab view** (`agent_tab.rs`). Streamed messages drawn with the Markdown reader's renderer, tool
   rows, a todo list, the permission card, the mode chip (agy), model and cost chips, `@` file picker, Stop with
   "Stopping…" and Force stop after 10 s, history read from the agent's own transcript.
9. **Task 8: the tab in the workspace** (`workspace_agent.rs`). The + menu lists the claude and agy presets (a shell and
   an agent tab side by side); "New Claude Tab" and "New Antigravity Tab" are in the pane's ⋯ menu and ⌘K. Also
   rename, close (asks while Working or Waiting), "Open in classic view", quit stops agents, the row label rule.
10. **Task 9: status everywhere.** A row's mark can come from an agent tab (the most urgent of the shell and the
    agents; a tie goes to the shell). The tab's own mark turns while it works. Ring, 🔔 and banner when it asks while
    you are elsewhere. The Activity panel follows the focused agent tab, and its header's age keeps turning. An agent
    error or non-zero exit adds an Activity note. ⇧⌘U ("Go to What Needs You") reaches a Waiting agent tab.
11. **Task 10: scripting API and MCP.** Agent tabs are `a1`, `a2`, ... next to `s1`, `s2`. `fuxx ls` lists them.
    `status`, `question`, `answer` and `wait` take `aN`.
12. **Task 11: drag a command block onto an agent tab.** Drag a block's chip from a shell onto the agent tab (its body
    or its tab in the bar). The command, its exit code and its output (the last 64 KB) become an attachment chip under
    the box. Nothing is sent until you send. A dragged block can be removed with the **×** on its chip (you approved
    this: "yes, add the ×"). A drop onto an error agent tab does nothing.
13. **Task 12: the agy provider** (`providers/agy.rs`). agy 1.5.7, re-recorded. Three modes (accept-edits, auto, plan) in
    the mode chip, chosen before the first message. Because agy has no way to stop a turn on stdin, Stop on an agy tab
    offers **Force stop** at once. A restored agy tab whose conversation is gone shows a "new conversation" notice.
14. **Task 13: this note.**

Review fix rounds changed a lot after the first build, mostly around R13 (Open in classic view) and privacy:
fixtures are scrubbed of your name, paths, account and hook text; the transcript's own command lines are shown as
"You" and its internal ones are skipped.

## The refresh calls (R1–R13)

Made by the AI on 2026-11-16, after M11p (cached chrome) and M11ac (the Markdown renderer) changed the places this plugs
into.

- **R1: a Workspace mirror of each agent tab.** `Workspace::agents` holds the view and what the chrome shows (id, label,
  badge, status, since, question, Activity log). Only `on_agent_event`, outside drawing, writes it. Chrome reads the
  slot, never the tab.
- **R2: agent marks have their own place.** Keyed `"agent-tab"` and the agent id, so the two id spaces do not mix.
- **R3: a row's subject may be an agent tab.** The most urgent of the shell and its agents wins; a tie goes to the
  shell.
- **R4: the tab carries its folder.** Claude keeps conversations per folder, so a restored tab starts where it
  was started.
- **R5: two fresh agent tabs in one folder are not duplicates.** They are told apart by tab id.
- **R6: shell-then-agent uses the existing "then" hook.** The agent tab is split in after the shell opens.
- **R7: messages are drawn by the Markdown reader's renderer.** Mermaid in chat shows as code (you: "fine").
- **R8: history is read off the UI thread.** `AgentTab::new` never emits, and nothing is emitted from `render`.
- **R9: the status bar keeps following the terminal** while an agent tab is focused, as with a file or browser tab.
  The Activity panel shows the agent tab's log.
- **R10: quit ends agents synchronously** (`flush_on_quit` calls `end_every_started`, which also ends the processes
  of tabs already closed).
- **R11: the row label.** An unnamed workspace whose shell has no name and no preset is labelled "Claude" (the first
  agent tab's label).
- **R12: scenes feed decoded events** (`shot_feed`), so a picture test uses the real path from the event on and no
  process runs.
- **R13: "Open in classic view" never makes two writers of one conversation.** It stops the tab's own process (asking
  first when Working or Waiting), shows "Open in classic view — this tab is paused", and opens `claude --resume <id>`
  in a terminal tab. The tab restarts only when you next send from it; if `claude` still runs in that terminal, the
  tab warns and keeps your text. Details of the holder check are under "Calls made".

## Calls made while building

1. **The user rule (yours, 2026-11-17):** "claude thing/fixes stays inside claude". Agent-specific code lives in that
   agent's provider file: flags, the id's shape, the program name, the npm title quirk, the model label. Generic code
   asks the provider (`conversation_in_args`, `classic_argv`, `program`, `model_label`, `interrupts`). Reviews checked
   it from then on.
2. **Quit.** Agents stop when fuxx quits and resume (with `--resume`) next launch. Making agents follow **Keep running
   after quit** is a follow-up, **M11ad**, design first.
3. **Allow all edits.** For Write and Edit, Claude's "always allow" means "accept all edits for this session", so the
   button says "Allow all edits".
4. **The R13 holder check.** Before a paused tab resumes (and at launch for a restored one), fuxx reads the process
   table off the UI thread and asks the provider whether any other process holds the conversation. If one does, the tab
   warns and does not start. If the check fails, it warns and does not start (fail-closed). Its limits:
   - While the classic view is **Blind** (a background-service terminal fuxx cannot see inside), any bare `claude`
     anywhere on the Mac counts as holding the conversation. It would rather refuse than risk two writers.
   - **Restarts keep the classic session id**, so a saved classic terminal is still recognised after a restart.
   - Only ids shaped like a UUID count as "another conversation".
5. **The scripting API never starts or resumes an agent.** `send`, `key` and `paste` refuse `aN`. `fuxx answer a1 yes`
   answers only a permission that is already waiting; a paused tab has none, so it replies "not waiting". Nothing can
   bypass the holder check.
6. **A drop of a block that has scrolled out of the grid silently does nothing.** (A limit; see below.)
7. **Status changes are emitted for a classic-view pause** too (as for terminals), but no notification.
8. **The unit-test and scene counts** moved with the work: 1,661 tests on master before the UI tasks, 1,787 now.
9. **Dead code.** The last `#[allow(dead_code)]` in the agent files was on `routes_permissions`, a provider flag for
   "asks fuxx before a tool" that nothing read. It was removed rather than kept: agy's permissions are not routed
   (below), and nothing needs the flag. `rg "allow\(dead_code\)"` over the agent files and `src/providers` finds
   nothing.
10. **Final review fixes (2026-11-17).**
    - **A new Claude conversation starts under an id fuxx chooses** (`claude … --session-id <uuid>`, through the
      provider hook `fresh_conversation_id`; Ruling 40). The tab takes that id as its conversation with your first
      message, so it is saved at once and `ps` always shows it: a `claude` left running by a crash is seen as the
      holder on the next launch. If the run ends before Claude says it started, the next start picks a new id (Claude
      refuses an id it has seen). Checked with Claude Code 2.1.326: its `init` reports the chosen id.
    - **The conversation id is followed when it changes** mid-run (Claude's `/clear`): the tab saves the new id, and
      restart, classic view and the holder check use it.
    - **The classic view's terminal is saved as a plain shell** (no preset, no agent), so restoring with agents never
      types today's `claude` preset into it.
    - **Only an id of the agent's shape is typed into the classic view's shell** (`is_conversation_id`); any other
      id is refused with a notice.
    - Force stop right after the agent exited no longer leaves sends queued; a failed resume puts queued messages
      back in the box; text you edit while a paused tab is checked stays in the box; a lost background-service
      terminal keeps its classic-view note when closed.

## Limits

- **Ruling 17's straggler.** If the agent exits on its own while a leftover background process of its (for example a
  `cmd &` it started) still holds the output pipe, closing the tab cannot signal that process, so it runs until it
  exits. The tab treats "the agent finished" apart from "the pipe closed".
- **Stop is best effort.** Claude does not stop a running foreground `Bash` tool on an interrupt, only a streaming
  reply. The button shows "Stopping…" and offers **Force stop** after 10 s (ends the process and resumes with
  `--resume`).
- **agy cannot be interrupted.** Stop on an agy tab offers Force stop at once.
- **agy's permission prompts are not routed.** In accept-edits mode agy ran `ls` without asking. fuxx shows what agy
  did, but cannot ask first.
- **Restored agy tabs show no earlier messages.** agy does not replay history on stdout and fuxx has no reader for its
  own records yet. The conversation itself continues. (Claude tabs do show history.)
- **A block that has scrolled out of the grid cannot be dropped.** The drop does nothing and says nothing.
- **Mermaid in chat shows as code.**
- **Claude's TodoWrite.** The Claude provider turns a `TodoWrite` call into the checklist, but this Claude build did not offer the tool during recording, so only a hand-written test line covers it. agy has no TodoWrite at all.
- **The status bar** keeps following the shell, not the agent (R9).
- **Quit stops agents** (M11ad is the follow-up).
- **Shortcut numbers moved.** ⌃⌘1 and ⌃⌘2 are now claude and agy, so opencode is ⌃⌘3 and codex ⌃⌘4 (they were ⌃⌘2 and ⌃⌘3).
- **The palette and the workspace switcher** show the shell's status, not an agent tab's. ⇧⌘U does reach waiting agent tabs.
- **`--fork-session`.** A Claude preset with `--fork-session` shows the "new conversation" notice.
- **An agy run left by a crash can be missed.** agy cannot be told a new conversation's id, so a brand-new agy
  conversation has no id on its command line. If fuxx crashes during its first turn, the `agy` it started keeps
  running unseen by the next launch's check, and the restored tab may start a second writer. Quit that `agy`
  (`pgrep -fl agy`) before sending from the restored tab. (Claude tabs are covered: fuxx chooses their id, except an npm-installed `claude`, whose `ps` line hides its
  arguments; quit a leftover `claude` the same way.)
- **Scripting gaps.** No open or close events for agent tabs; `status.changed` also fires for restored tabs; a blank
  `fuxx answer` is a Deny; `new` over the API with a preset still opens a classic terminal. An agent tab's events
  have no `number` (no `#N` targets it): name it by its `aN`.
- **R13 edges.** With the background service off, a stale saved classic id keeps the tab Blind on every run (fuxx
  cannot ask the service whether that terminal still exists). With it on, a stale id keeps the tab Blind for that
  run. Open in classic view during a send's check shows a notice instead of focusing the classic terminal.
- **A conversation id that changes mid-run** (e.g. Claude's `/clear`) is followed by the tab, but the running
  process's command line still shows the old id; if fuxx crashes after that, the leftover process writing the new
  conversation isn't seen by the next launch's check. Quit leftover agents before sending from a restored tab.
- **Test files from before the final fixes.** A `sessions-dev.json` written by an earlier test build may still
  carry a classic-view terminal's preset; with a `claude -c` preset it could start a second writer on relaunch.
  Close classic-view terminals (or delete `sessions-dev.json`) before testing this build.
- **A preset's extra words** are split simply, so a quoted argument with spaces is not kept together.
- **Dev builds touch the real fuxx's background service** unless you point them elsewhere; see the first line of the
  check below.

## On-screen check (yours, after the build)

> **Your first look (2026-11-17):** "the agent tab is not what I expect it be, the layout, the flow is just different
> from what we discussed. But we can skip this later." The agent tab's layout and flow are to be revisited with you
> (what differs from the design is not written down yet). The rest of this check is still open.

To try it before merging, build the branch from Terminal.app so a dev build does not touch your real fuxx:
`FUXX_DAEMON_SOCKET=/tmp/fuxx-dev.sock cargo run` (a debug build). **Do not add `--release`:** only a debug build
saves to `sessions-dev.json`; a release build saves to your real `sessions.json`, and a fuxx from before this branch
cannot read agent tabs, so it would start empty and save over your workspaces. To rerun the pictures,
`scripts/ui-shots.sh`. Use a
scratch folder you do not mind an agent editing (`~/code/scratch`). Items 1–17 need `claude` logged in; items 18–21 need
`agy`; items 22–25 need an agent tab open.

**Open and read**

1. ⌃⌘1 opens a workspace with a shell and Claude side by side, and the sidebar row reads "Claude".
2. Send a message that gets a reply with a list, a code block and a table. They stream in and look right.
3. A tool row opens to a diff (for an edit) or its output (for a command).
4. Ask for something that writes a file. The card asks. **Allow**, **Allow all edits** and **Deny** each work (try each
   once; after Allow all edits it stops asking for edits).

**Stop, status, notifications**

5. While it works, Esc stops it ("Stopping…"). If it was running a long command, **Force stop** shows after 10 s.
6. Focus another workspace, then make the agent ask. The row gets a ring and a mark, a 🔔 entry and a banner appear.
   Press ⇧⌘U: it takes you to the agent tab.
7. The agent tab's mark turns while it works. Its title becomes your first message. Rename the tab.
8. The Activity panel with the agent tab focused shows its log, and the age in its header keeps counting. Make the
   agent fail (for example, run it with no network or kill it from another terminal): an Activity note appears.

**Close, quit, restore**

9. Close a Working agent tab: it asks. Afterwards `pgrep -f "claude -p"` finds nothing.
10. Quit and relaunch: the conversation comes back with its history, and the shell does not type `claude` into itself.
11. Typing `claude` in the shell is still the classic TUI.

**Open in classic view**

12. Open in classic view (the tab's menu) pauses the tab and shows "Open in classic view — this tab is paused".
13. With `claude` still running in the classic terminal, send from the agent tab: it warns and keeps your text, and
    starts nothing.
14. Quit `claude` in the classic terminal, send again: the agent tab resumes.

**Dragging a block**

15. In the shell, run `npm test` (or any command that fails). Drag its chip onto the agent tab (the body, then try the
    tab in the bar). A chip appears under the box with "exit 1 · npm test". Nothing is sent yet; send, and the agent
    sees the output.
16. Remove the chip with its **×** before sending: it goes, and nothing is attached.
17. A drop onto an error agent tab does nothing. To get one: open an agent tab and quit the dev build; back up
    `~/Library/Application Support/fuxx/sessions-dev.json` (the dev build's file; the released app uses
    `sessions.json`); in the tab entry with `"kind": "agent"`, change `"provider": "claude"` to `"provider": "nope"`;
    relaunch. You should see one error row, "No agent called nope", and no composer. Drag a block onto it: nothing
    happens. Restore the backup afterwards. Also, dropping a block that has scrolled out of the grid does nothing (a
    known limit).

**agy** (if you use it)

18. New Antigravity Tab: the mode chip cycles accept-edits, auto, plan before the first message and then stays fixed.
    Open a new agy tab for each mode (the chip locks after the first message): pick the mode, send "list this
    folder", and note what agy did (whether it asked, ran the command, or only planned).
19. Stop on an agy tab offers **Force stop** at once.
20. Quit and relaunch with an agy tab that has a conversation: it continues, but earlier messages are not shown (a limit).
    If you deleted that conversation in agy, the tab shows a "new conversation" notice.
21. In accept-edits, agy may run `ls` or edit files without asking (a limit, not a bug).

**Scripting**

22. `fuxx ls` lists the agent tab as `a1`. `fuxx status a1` prints its status.
23. `fuxx wait --any --for waiting` returns when an agent tab asks.
24. `fuxx answer a1 yes` allows a waiting permission. When nothing is waiting, it says "not waiting".
25. `fuxx send a1 hi`, `fuxx key a1 ...` and `fuxx paste a1 ...` are refused ("a1 is an agent tab, not a terminal").

**Chrome**

26. `FUXX_CHROME_CACHE=off` looks identical with an agent tab open.
27. With the window idle and no agent working, Activity Monitor shows fuxx's CPU as before.

## Not in it

Chat-style shell tabs (M11ab); agents that keep running while fuxx is closed (M11ad); other agents than claude and agy
(opencode and codex stay terminal presets); Mermaid rendering in chat; agy's permission routing and history replay;
remote (SSH) workspaces (the menu offers agent tabs only locally).

## The design

Moved from [[Proposal - Chat View and Agent Tabs]] (Part 1), as approved on 2026-11-13. Where the build chose
differently, "Calls made" and the refresh calls above win.

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
- **Empty state:** a short line ("Ask Claude to do something in ~/code/my-app") and the composer focused.

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
- **`agy.rs`** — starts `agy -p= --input-format stream-json --output-format stream-json` (+ `--conversation <id>`,
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
- **The pane `···` menu (and ⌘K) can add an agent tab** to any pane (AI's call on how: the same preset list, showing
  only presets with a provider). The tab bar's `+` has no menu and stays "New terminal tab". (Corrected 2026-11-13.)
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
- Closing an agent tab while it's working asks first. (Corrected 2026-11-13: terminals never ask today, `close_tab` "Never asks"; this is new behaviour, for agent tabs only.)

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

## Related

[[40 - Roadmap]] · [[12 - Scope]] · [[M11aa Plan - Agent Tabs]] · [[Proposal - Chat View and Agent Tabs]] ·
[[77 - Milestone 11p - Chrome Caching]] · [[78 - Milestone 11ac - File Tab Polish]] ·
[[81 - Milestone 11aj - Faster UI Shots]]
