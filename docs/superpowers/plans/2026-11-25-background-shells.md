# Background Shells (Chore 16) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task (the user's build mode: built in the session, screenshots and one review at the end). Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Show an agent's background shells (Claude's `run_in_background` Bash) in a `▣ done/all` tab beside the plan tab, with a popover (status, elapsed, command, live last line, ■ stop, click to follow the output), a "running in the background · M:SS" chat row, and a banner when one finishes.

**Architecture:** Claude's translator turns `system/task_started` (`task_type: "local_bash"`, `is_backgrounded: true`) + the Bash call's tool_result (the output file) into a provider-neutral `Event::ShellStarted`, and `task_notification` into `Event::ShellEnded`; ■ is `Action::StopShell`, encoded as the same `stop_task` control request a subagent's stop uses. A pure `agent_shells::Shells` in the conversation keeps them; the box draws one corner tab group (shells pill, plan pill) and a popover hung from the group's right edge; the tab reads the last output line off the UI thread once a second while the popover is open.

**Tech Stack:** Rust, GPUI (pinned), serde_json; Claude Code 2.1.331 stream-json.

**Spec:** `docs/superpowers/specs/2026-11-25-background-shells-design.md`

## Global Constraints

- Tab: Lucide terminal icon (`Icon::Terminal`, already in `src/icons.rs`) then `done/all`; peach (`c.todo_active`) while any runs; green `n/n ✓` (`c.tool_ok`) when all finished, kept until your next message; red with ✕ (`c.failed_panel`) when one failed, until you open the popover. Shown only once a shell started.
- With the plan tab: shells pill on the plan pill's left, the two in one tab flush right, the mirrored step on the group's left; alone, it takes the corner.
- Popover: over the chat, right edge on the box's; header `"1 of 3 done · 2 running"`; rows: mark (arc / ✓ / ✕), description, elapsed `4:12`, command (small mono), last output line, ■ for a running one; finished rows dim and drop off 2 minutes after they end; footer `"Click a shell to follow its output"`. A row click opens its output file (`AgentTabEvent::OpenFile`).
- Chat: the Bash step that started it reads `"running in the background · 4:12"` while it runs.
- Banner when a shell finishes while you're not looking at the tab: `"<description> · done in 6:40"` (failed: `"<description> · failed"`).
- Last line: read off the UI thread, ~1 s cadence, only while the popover is open; never per frame.
- Provider-specific parsing stays in `src/providers/claude.rs`. Claude only (agy's `run_command` goes async by `WaitMsBeforeAsync` with no task events).
- Scenes use neutral names only (my-app, `cargo test`, retry.rs).

## Review Focus

- A foreground Bash also sends a `local_bash` `task_started` (`is_backgrounded: false`, claude-queue fixture): it must make no shell (Task 1 test).
- `completed` with a non-zero exit in the summary ("… completed (exit code 1)") is a failure (Task 1 test).
- A `task_notification` for a shell whose tool_result never came still ends it, its output from the notification (Task 1 test).
- The agent process ending (a restart, a crash) leaves no shell "running" forever (Task 2 test: `process_gone` stops them, no banner).
- A shell started before your last message and still running stays in the tab after you send (Task 2 test).

---

### Task 1: Events, Claude's reading and the stop

**Files:**
- Modify: `src/providers/mod.rs` (Event, Action), `src/providers/claude.rs` (system/user/assistant/encode + tests), `src/providers/agy.rs` (encode arm)
- Create: `src/providers/fixtures/claude-background-shells.jsonl` (recorded: `scripts/record-agent-fixture.py claude background-shells`; scenario added)

**Interfaces — Produces:**
- `Event::ShellStarted { id: String, call: String, description: String, command: String, output: Option<PathBuf> }` (`id`: the task id)
- `Event::ShellEnded { id: String, end: ShellEnd, output: Option<PathBuf> }`; `pub enum ShellEnd { Done, Failed, Stopped }` in mod.rs
- `Action::StopShell { id: String }` → Claude: `{"type":"control_request","request_id":"fuxx-stop-<id>","request":{"subtype":"stop_task","task_id":<id>}}`; agy: nothing.

Claude: a backgrounded `local_bash` `task_started` is held by its `tool_use_id` until that call's tool_result ("Output is being written to: <path>.output") and then sent as one `ShellStarted`; a `task_notification` for a held one flushes it first. The command comes from the Bash tool_use's `input.command`.

- [ ] Tests (`claude.rs`): `background_shells_fixture` (2 starts with descriptions, commands and `.output` paths; one `Done`, one `Stopped`); `a_foreground_bash_task_is_no_shell` (claude-queue fixture: no ShellStarted/ShellEnded); `a_nonzero_exit_is_a_failed_shell`; `a_notification_before_the_result_still_starts_and_ends_it`; `stop_shell_writes_stop_task`. Run `cargo test --bin fuxx shell` → FAIL, implement, → PASS.

### Task 2: The shells' state

**Files:** Create `src/agent_shells.rs` (pure); modify `src/agent_conversation.rs` (field, apply, `you_to`, `process_gone`/`process_ended`), `src/main.rs` (mod).

**Interfaces — Produces:** `Shell { id, call, description, command, output, started: Instant, ended: Option<(ShellEnd, Instant)>, last_line: Option<String> }`; `Shells` with `start(.., now)`, `end(id, end, output, now) -> Option<Shell>` (the ended one, for the banner), `you_sent()`, `gone(now)`, `counts() -> (usize, usize)`, `tone() -> Tone { Running, Done, Failed }`, `seen()`, `rows(now) -> Vec<&Shell>` (running, then ended under `DROP_AFTER` = 120 s), `header() -> String`, `running_for_call(call) -> Option<&Shell>`, `set_last_line(id, line)`; free fns `last_line(tail: &str) -> Option<String>`, `elapsed(shell, now) -> Duration`, `banner_words(shell) -> String`, `row_words(elapsed) -> String`. `Conversation::shells() -> &Shells`, `shells_mut()`.

- [ ] Tests (`agent_shells.rs`): counts/tone through start → end Done → all Done → you_sent clears finished, keeps running; a failure is red until `seen`; rows drop a finished one after 120 s; header "1 of 3 done · 2 running"; `last_line` skips trailing blank lines and `\r` progress; banner words "Build · done in 6:40" / "Build · failed"; `gone` stops running ones. Conversation test: ShellStarted/ShellEnded through `apply` bump the Bash row's version.

### Task 3: The tab and its popover

**Files:** Create `src/agent_shells_view.rs` (impl AgentTab: `shells_tab`, `shells_popover`, `toggle_shells`, `stop_shell`, `open_shell_output`, `poll_shell_lines`); modify `src/agent_box.rs` (corner group: `folder_tab` takes the corner pills + popover; shared `corner_pill`; `BoxMenu::Shells`), `src/agent_box_menus.rs` (plan popover hung from the group; Shells closes on press-out), `src/agent_tab.rs` (fields: `shell_poll: Option<Task<()>>`), `src/main.rs`.

Popover anchored to the group at `MenuPlace::Right(0.)`, `TAB_PAD` down (the plan's popover keeps its place). Opening it calls `shells.seen()` and starts the poll: every 1 s, the open rows' output files' last 4 KB read on the background executor, `last_line` of each set on the shell; it ends when the menu isn't Shells.

- [ ] Gates; GUI checked in Task 5's scenes.

### Task 4: The chat row, the ticking and the banner

**Files:** `src/agent_rows.rs` (a Bash row whose call has a running shell: the arc and "running in the background · M:SS"), `src/agent_tab.rs` (`keep_ticking` counts running shells and splices their rows; on `ShellEnded`, emit `AgentTabEvent::ShellEnded { title, failed }`), `src/workspace_agent.rs` (not looking: the 🔔 entry and the banner, through a shared tail of `notify_agent`).

- [ ] Tests: `row_words` above; the workspace's looking rule is the existing `focused_tab && window_active`.

### Task 5: Scenes, docs, commits

**Files:** Create `src/ui_shots/agent_shells.rs` (scenes `agent-shells-with-plan`, `agent-shells-alone`, `agent-shells-done`, `agent-shells-failed`, `agent-shells-popover`); register in `registry.rs` (AREAS, ORDER) and `mod.rs`; note 93 chore 16 + Progress Log.

- [ ] Assertions: the tab's words (`1/3`, `3/3 ✓`, `1/2 ✕`), the shells pill left of the plan pill, the group flush with the box's right edge, the popover's right edge on the box's, the header, a row click → `OpenFile`, ■ → a `stop_task` written, the Bash row's "running in the background".
- [ ] `scripts/ui-shots.sh --changed master`; look at the new PNGs; commit by name; `scripts/prune-target.sh`.
