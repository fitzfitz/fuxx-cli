# M11ar Subagents Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans (this repo's build mode: the session
> builds every task itself; per-task gate `cargo test --bin fuxx`, both clippy runs, `cargo fmt --check`; screenshots
> and one whole-branch review only at the end). Steps use checkbox (`- [ ]`) syntax.

**Goal:** See every Claude and agy subagent, switch the agent tab's chat to one by a pill row and follow it live, and
stop one where the agent allows.

**Architecture:** one provider-neutral subagent model — a **lane** per subagent, keyed by the tool call that started
it (the same id today's items carry as `parent`) — fed by new `Event`s and declared capabilities. Each provider file
translates its own signals: Claude from its stream (`task_*` system messages, `parent_tool_use_id`, `stop_task`), agy
from its `subagent` step plus a **followed file** (the subagent's `transcript.jsonl`, tailed by generic code and
decoded by agy's file). The agent tab gains a view (`Main | Lane(id)`), a pill strip, a lane header and the Main row.

**Tech Stack:** Rust, GPUI (pinned rev), `claude -p` / `agy -p` stream-json, `serde_json`.

**Spec:** [[93 - Milestone 11ar - Subagents]] (approved 2026-11-22: "But above is approved!").

## Global Constraints

- Provider-specific parsing and argv only in `src/providers/claude.rs` / `src/providers/agy.rs`; generic code asks
  the provider (memory: provider code in provider files).
- The UI shows Stop / the "wants to run" heading / stats only when the provider's capability says so (Claude: all;
  agy: none).
- The agent box always talks to Main; sending from a lane view switches the view to Main. Esc / ■ stop the whole turn.
- The view never switches by itself (no auto-switch on start, ask or end).
- Pills: Main first, then lanes in start order; labels ≤ 20 characters (≤ 12 under a 480 pt tab) with an ellipsis, a
  repeated label gets " 2"; keys ⌥⌘[ / ⌥⌘] in the `AgentTab` context.
- Fixtures are recorded from real runs (Claude 2.1.330+, agy 1.5.14+) with a tiny prompt in a scratch folder; no
  work names, no `/Users/<name>` paths in them (replace with `/Users/me`).
- Out: typing to a lane, per-lane box/model/mode, agy swarms, split view, toasts, closing or reordering pills, nested
  trees (nested subagents are flat lanes in start order).

## Rulings (made while planning; the spec is the authority)

- **agy "done":** a lane is done when its own transcript shows its `send_message` to the parent's conversation id (its
  report) — the same moment the spec's parent-transcript `sender=` line marks, read from the file already followed
  instead of a second one. A later step re-opens it as working (a woken subagent). *Cost if wrong:* a lane that
  reports then keeps working shows done briefly.
- **Elapsed:** the tab keeps each lane's first-seen `Instant` (like `tool_started`); the conversation model stays free
  of clocks.
- **Resume:** a resumed conversation's subagent tool rows with child items get a lane each (status done, label from
  the tool's summary); live lanes come only from a running process.

## Review Focus

1. Two subagents running at once (Claude and agy): each lane gets only its own steps; pills in start order. → Task 2,
   Task 3 tests with the two-subagent fixtures.
2. The process ends mid-subagent (Force stop, agy's restart on a model/mode change, a crash): working lanes become
   stopped, followers stop, no pill spins forever. → Task 1 `process_end_stops_working_lanes`, Task 3
   `the_follower_stops_with_its_lane`.
3. Claude's background subagent outlives the turn's `result`: its lane keeps working until `task_notification`. →
   Task 2 `a_background_lane_ends_after_the_turn`.
4. A permission request from a subagent while you view another lane: the card shows in the current view, Enter allows,
   the asking lane's pill is tinted. → Task 5 scene `agent-lanes-ask`.
5. The followed agy file is missing, unreadable or cut mid-line: the lane stays (no steps), no panic, the half line is
   read once complete. → Task 3 `a_half_written_line_waits`.

---

### Task 1: The shared model — events, lanes, capabilities

**Files:** `src/providers/mod.rs` (events, action, caps), `src/agent_conversation.rs` (lanes), `src/agent_lanes.rs`
(new: pure lane helpers for the UI), `src/main.rs` (mod).

**Interfaces — produces:**
- `Event::SubagentStarted { id: String, handle: Option<String>, label: String, kind: String, task: String,
  model: Option<String> }` — `id` = the tool call it hangs under; `handle` = the provider's own id for actions
  (Claude's `task_id`).
- `Event::SubagentProgress { id: String, tokens: Option<u64>, tools: Option<u32>, last_tool: Option<String> }`.
- `Event::SubagentEnded { id: String, end: LaneEnd }`; `pub enum LaneEnd { Done, Failed(String), Stopped }`.
- `Event::Follow { id: String, path: PathBuf }` — "read this file's new lines as lane `id`'s".
- `Event::Permission` gains `by: Option<String>` (the lane id; `None` = Main).
- `Action::StopSubagent { handle: String }` (a provider without the capability writes nothing).
- `pub struct SubagentCaps { pub stop: bool, pub asks: bool, pub stats: bool }`; `Provider::subagents(&self) ->
  SubagentCaps` (default all false); `Provider::decode_followed(&self, lane: &str, line: &str) -> Vec<Event>`
  (default empty).
- `agent_conversation`: `pub struct Lane { pub id, pub handle: Option<String>, pub label, pub kind, pub task,
  pub model: Option<String>, pub status: LaneStatus, pub tokens: Option<u64>, pub tools: Option<u32>,
  pub last_tool: Option<String>, pub steps: usize }`; `pub enum LaneStatus { Working, Waiting, Done,
  Failed(String), Stopped }`; `Conversation::lanes(&self) -> &[Lane]` (start order); `Conversation::lane(&self, id)
  -> Option<&Lane>`.
- `agent_lanes`: `pub fn pill_label(label: &str, max: usize) -> String`; `pub fn pill_labels(lanes: &[Lane], max:
  usize) -> Vec<String>` (repeats get " 2", " 3"); `pub enum View { Main, Lane(String) }`; `pub fn step(view: &View,
  lanes: &[Lane], dir: i32) -> View` (Main is first; no wrap).

- [ ] Tests (`agent_conversation`): `a_subagent_becomes_a_lane` (Started → one lane, Working, its label/kind/task);
  `progress_fills_the_lanes_stats`; `an_end_sets_done_failed_or_stopped`; `a_lanes_permission_makes_it_waiting`
  (Permission{by: Some(id)} → Waiting; answered → Working); `a_step_after_done_reopens_the_lane`;
  `process_end_stops_working_lanes` (`process_ended` and `paused*` → Stopped; Done stays Done);
  `a_resumed_task_with_steps_gets_a_done_lane` (Ruling: resume). `steps` counts the lane's tool items.
- [ ] Tests (`agent_lanes`): `pill_label("Workspace File Reader", 12) == "Workspace F…"`; `pill_labels` gives
  `["Reader", "Reader 2"]` for two "Reader"s; `step` from Main by +1 is the first lane, by −1 stays Main, from the last
  lane by +1 stays there.
- [ ] Fail, implement, pass (existing `Permission` constructors get `by: None`). Gate. Commit `M11ar Task 1: the
  shared subagent model — events, lanes, capabilities`.

### Task 2: Claude — lanes from its stream, Stop, who asks

**Files:** `src/providers/claude.rs`, `src/providers/fixtures/claude-subagents.jsonl`,
`claude-subagent-stop.jsonl`, `claude-subagent-ask.jsonl` (new, recorded).

**Interfaces:** consumes Task 1. Produces `CLAUDE.subagents() == SubagentCaps { stop: true, asks: true, stats: true }`.

- [ ] Record the fixtures (stream-json with `--verbose`, `--model haiku`): two parallel subagents (each lists a tiny
  file); one stopped with `{"type":"control_request","request_id":…,"request":{"subtype":"stop_task","task_id":…}}`
  written mid-run; one whose Bash needs permission (no `--allowedTools`). Note in the fixture's test which field links
  `can_use_tool`'s `agent_id` to `task_started` (`task_id` or another).
- [ ] Tests: `task_started_becomes_a_lane` (label: a custom `subagent_type`, else `description`; built-ins
  `general-purpose`, `Explore`, `Plan` are not names; `kind` = `subagent_type`; `handle` = `task_id`; `id` =
  `tool_use_id`); `task_progress_becomes_progress`; `task_notification_ends_the_lane` (completed → Done, failed →
  Failed(reason), killed → Stopped); `two_subagents_keep_their_own_steps` (fixture: items' `parent` per lane);
  `a_background_lane_ends_after_the_turn` (a `task_notification` after `result` still ends it);
  `a_subagents_permission_says_who` (`Permission.by` = that lane's id); `stop_subagent_writes_stop_task` (encode:
  one line, `subtype: "stop_task"`, the handle as `task_id`); `the_stop_fixture_ends_one_lane_only`.
- [ ] Fail, implement in `ClaudeTranslator::system` (`task_started`, `task_progress`, `task_notification`) and the
  `can_use_tool` reader, pass. Gate. Commit `M11ar Task 2: Claude's subagents — lanes, Stop, who asks`.

### Task 3: agy — lanes from its `subagent` step and the followed transcript

**Files:** `src/providers/agy.rs`, `src/lane_follow.rs` (new: the generic file tailer), `src/agent_tab.rs` (start and
stop followers, feed their lines through `decode_followed`), `src/providers/fixtures/agy-subagents.jsonl` and
`agy-subagent-transcript-{a,b}.jsonl` (new, recorded; paths rewritten to `/Users/me`).

**Interfaces:**
- agy's `subagent` step: on ACTIVE, `ToolStarted { id: "sub-<step_index>", name: "Subagent", summary: <role>,
  parent: None, … }` and `SubagentStarted { id: "sub-<step_index>", handle: None, label: role, kind: type_name, task:
  initial_prompt, model: None }`; on DONE (spawned) `Follow { id, path: <log_uri as a path> }`. No `ToolFinished`
  then: the row stays open as the lane's while it works.
- `AGY.decode_followed(lane, line)`: a `PLANNER_RESPONSE` → `MessageDone{parent: Some(lane)}` for `content` and
  `ToolStarted{parent: Some(lane)}` per `tool_calls[]` (ids `"<lane>-<step_index>-<n>"`); a `GENERIC` tool result →
  `ToolFinished` for the previous step's calls; a `send_message` whose `Recipient` is the parent conversation →
  `SubagentEnded{Done}` (Ruling: agy done); malformed → nothing.
- `lane_follow::Follower::start(path: PathBuf, every: Duration) -> Follower` with `fn take_lines(&mut self) ->
  Vec<String>` (complete lines only) and `fn stop(&mut self)`; polls off the UI thread; a missing file is retried until
  stopped.
- `AgentTab`: on `Event::Follow`, a follower per lane (500 ms); on each tick its lines go through
  `provider.decode_followed` into `on_events`; stopped when the lane ends, the process ends, or the tab closes.
- At `TurnDone`, lanes still Working become Stopped when the provider can't stop one (`subagents().stop` false: agy's
  `-p` waits for its subagents, so one still working was cut off) — a capability, not a provider name, decides.

- [ ] Record: one `agy -p` run with two parallel subagents; copy both transcripts; rewrite paths.
- [ ] Tests (agy): `a_subagent_step_is_a_row_and_a_lane`; `done_spawned_starts_following` (Follow with the
  transcript path); `two_subagents_are_two_lanes`; `a_transcript_line_becomes_the_lanes_steps`;
  `the_report_to_the_parent_ends_the_lane`; `a_bad_line_is_skipped`; `agy_has_no_subagent_controls`.
- [ ] Tests (`lane_follow`, on a temp file, removed after): `new_lines_arrive_in_order`;
  `a_half_written_line_waits` (no newline yet → nothing; completed → one line); `a_missing_file_is_waited_for`;
  `the_follower_stops_with_its_lane` (after `stop`, appended lines never arrive).
- [ ] Fail, implement, pass. Gate. Commit `M11ar Task 3: agy's subagents — the subagent step and its followed
  transcript`.

### Task 4: The pill strip and the view

**Files:** `src/agent_tab.rs` (view state, strip, row filtering, keys), `src/agent_chat.rs` (`is_row` per view),
`src/main.rs` (bindings), `src/help_shortcuts.rs` (list them), `src/ui_shots/agent_lanes.rs` (new scenes),
`src/ui_shots/registry.rs`.

**Interfaces:** consumes `Conversation::lanes`, `agent_lanes::{View, step, pill_labels}`. Produces
`AgentTab::show_lane(&mut self, view: View, cx)`, actions `agent_tab::{PrevLane, NextLane}` (⌥⌘[ / ⌥⌘]),
`agent_chat::is_row_in(items, ix, view: &View) -> bool` (Main: today's rule; a lane: items whose `parent` is the lane).

- [ ] Tests: `is_row_in` (Main hides a lane's items; the lane view shows only its own, as rows);
  `the_lane_keys_are_bound_in_the_agent_tab` (keymap: ⌥⌘[ → PrevLane, ⌥⌘] → NextLane in `AgentTab`).
- [ ] Build the strip per the spec (its own 32 pt strip under the top bar, aligned to the column, shown once a lane
  exists; marks and tints from the tool row's tokens; the active pill's neutral fill; "+N" overflow popup in the
  `tab_menu` style; tooltip: kind, task, elapsed). The list's rows follow the view; switching keeps each view's scroll
  at its bottom when it was there.
- [ ] Scenes `agent-lanes-strip` (Claude fixture: Main + two pills, one working one done; wide and narrow; both themes)
  and `agent-lanes-overflow` (eight lanes: "+N" and its popup). Checks by part (`strip`, `pill-<n>`) and pixel.
- [ ] Gate. Commit `M11ar Task 4: the pill strip and the lane view`.

### Task 5: The lane in Main, the lane view, Stop, who asks

**Files:** `src/agent_tab.rs`, `src/agent_box.rs` (placeholder; send → Main), `src/agent_tool_modal.rs` (sub-agent
steps section removed; subagent rows not openable), `src/ui_shots/agent_lanes.rs`.

**Interfaces:** consumes Tasks 1–4; `Action::StopSubagent`.

- [ ] Tests: `a_subagent_row_opens_its_lane_not_the_modal` (`agent_tool_modal::openable` false for a tool that has a
  lane); `sending_from_a_lane_returns_to_main` (state test on the tab); `stop_shows_only_with_the_capability`
  (pure: `agent_lanes::shows_stop(caps, status)` → Claude working true, done false, agy false).
- [ ] Build per the spec: Main's subagent row (label, task, last tool or "N steps", elapsed, ›, click → its lane);
  the lane header (label SEMIBOLD, kind and model `text_dim`; elapsed/tokens/tools `text_faint` when known; Stop chip
  with the capability → `Action::StopSubagent`); the task as the first bubble captioned "from Main"; end lines
  ("✓ Done in … · Back to Main ›", "✕ Failed · reason", "Stopped"); the box placeholder "Message Main — it can pass
  this to {label}" in a lane view; the permission card headed "{label} wants to run {tool}" in whichever view is
  shown, the asking lane's pill tinted, Main's notch "Needs input". Under 480 pt: header keeps elapsed and Stop.
- [ ] Scenes `agent-lanes-view` (a working lane: header, bubble, steps, Stop; a done lane: end line),
  `agent-lanes-ask` (viewing lane B while lane A asks: the card shows, A's pill tinted), `agent-lanes-agy` (agy
  fixtures: rows, pills, a followed lane's steps, no Stop). Both themes, wide and narrow where the spec differs.
- [ ] Gate. Commit `M11ar Task 5: the lane in Main, the lane view, Stop and who asks`.

### Task 6: Finish — screenshots, review, docs

- [ ] Full `scripts/ui-shots.sh`; one whole-branch review on the most capable model; one fix wave; the full screenshot
  run again.
- [ ] Docs: note 93 "as built" and its checklist, Roadmap (M11ar built), Plan Tracker, Progress Log. Commit.
