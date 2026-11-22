# M11ar Subagents Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans (this repo's build mode: the session
> builds every task itself; per-task gate `cargo test --bin fuxx`, both clippy runs, `cargo fmt --check`; screenshots
> and one whole-branch review only at the end). Steps use checkbox (`- [ ]`) syntax.

**Goal:** See every Claude and agy subagent, switch the agent tab's chat to one by a pill row and follow it live, and
stop one where the agent allows.

**Architecture:** one provider-neutral subagent model — a **lane** per subagent, keyed by the tool call that started
it (the id today's items carry as `parent`) — fed only by events. Each provider's translator turns its own signals
into those events and says, per lane, what it can do (`LaneControls`). Claude reads its stream (`task_*` system
messages, `parent_tool_use_id`, `stop_task`); agy reads its `subagent` step plus each subagent's transcript, tailed by
a generic follower and decoded by agy's translator. The agent tab gets a view (`Main | Lane(id)`) with one list per
view, a pill strip, a lane header and Main's subagent row, in new files beside `agent_tab.rs`.

**Tech Stack:** Rust, GPUI (pinned rev), `claude -p` / `agy -p` stream-json, `serde_json`.

**Spec:** [[93 - Milestone 11ar - Subagents]] (approved 2026-11-22: "But above is approved!").

**Revised 2026-11-22** on the three leads' picks (your words: "I'll go with Lead Software Architecture, Lead Fullstack
Developer, Lead UI/UX Designer agents best recommendation picks"): the Lead Software Architect (per-lane controls, one
lane key for actions, agy's end-of-turn rule in agy's translator, history events for resume, nesting-ready ids), the
Lead Fullstack Developer (a move-only Task 0, a list per view, lane-aware settling, the follower as a task, a split
Task 5, fixture hygiene), the Lead UI/UX Designer (resumed labels by the live rule, elapsed that stops, failed/stopped
scenes, Task 5's lost items).

## Global Constraints

- Provider-specific parsing only in `src/providers/claude.rs` / `src/providers/agy.rs` (memory: provider code in
  provider files); generic code reads events and `LaneControls`, never a provider's name.
- The agent box always talks to Main; sending from a lane view switches the view to Main. Esc / ■ stop the whole turn.
- The view never switches by itself.
- Pills: Main first, then lanes in start order (nested subagents flat); labels ≤ 20 characters (≤ 12 under a 480 pt
  tab) with an ellipsis; a repeat gets " 2"; keys ⌥⌘[ / ⌥⌘] in the `AgentTab` context.
- Elapsed shows only when known; it stops when the lane ends; never "0:00".
- Fixtures: recorded in `/tmp/fx` with tiny prompts, home paths rewritten to `/Users/me`, Claude's init noise (MCP,
  plugin, memory paths) dropped; no work names.
- Out: typing to a lane, per-lane box/model/mode, agy swarms, split view, toasts, closing or reordering pills.

## Rulings (the leads' picks and mine; the spec is the authority)

- **agy "done"** (kept by all three): when its own transcript shows its `send_message` to the parent conversation. A
  later step re-opens it; a late Done replaces Stopped (the follower can lag a poll).
- **agy at the end of a turn** (architect, developer): agy's translator, on `result`, ends its lanes that haven't
  reported as Stopped (agy's `-p` waits for its subagents). No generic rule. *Cost if wrong:* none generic.
- **Controls per lane** (architect): `SubagentStarted.controls: LaneControls { stop }` decided by the translator;
  who-asks and stats come from the data (`Permission.by`, `Option` stats). No static provider capability.
- **One key** (architect): actions name the lane id (`Action::StopSubagent { lane }`); the translator keeps its own
  maps (lane → Claude `task_id`, `agent_id` → lane).
- **Resume through events** (architect, designer): `Provider::history` emits `SubagentStarted` + `SubagentEnded` for
  past subagents, labelled by the live rule (Claude: from the Agent call's input — a custom `subagent_type`, else
  `description`; task = `prompt`). No "tool with steps" heuristic. agy has no `history()` today, so a resumed agy tab
  has no lanes. *Cost if wrong:* agy pills come back only once agy history exists (spec said "also on resume").
- **Elapsed** (designer): the tab keeps each lane's first-seen and ended `Instant`; resumed lanes have neither, so no
  time shows ("✓ Done · Back to Main ›").
- **Rows** (developer): a subagent's row in Main reads its state, label and task from the lane for both agents; agy
  sends ordinary `ToolStarted`/`ToolFinished` for its subagent call.

## Review Focus

1. Two subagents at once (both agents): each lane gets only its own steps; pills in start order. → Tasks 2, 3.
2. The process ends or restarts mid-subagent: working lanes become Stopped, followers stop, no pill spins. → Task 1
   `process_end_stops_working_lanes`, Task 3 follower tests.
3. Claude's background subagent outlives Main's `result`: its lane, its running steps and its open permission stay
   live until its `task_notification`. → Task 1 `settling_a_turn_leaves_a_working_lanes_steps`, Task 2
   `a_background_lane_ends_after_the_turn`.
4. A subagent asks while you view another lane: the card shows in the current view, Enter allows, the asking lane's
   pill is tinted. → Task 6 scene `agent-lanes-ask`.
5. The followed agy file is missing, shrinks, or is cut mid-line: no panic; the half line is read once complete. →
   Task 3 `lane_follow` tests.

---

### Task 0: Move the row drawing out of `agent_tab.rs` (move only)

**Files:** create `src/agent_rows.rs` (an `impl AgentTab` block, as `agent_box.rs` does); move `item_row`,
`item_body`, `avatar`, `tool`, `diff_block`, `permission_card`, `you_text`, `blocks_of` and their private helpers out of
`src/agent_tab.rs`. No behaviour change.

- [ ] Move; widen visibility only as needed (`pub(crate)`/`pub(super)`). Gate (tests, clippy unchanged) and the agent
  scenes (`scripts/ui-shots.sh agent`: all pass, fingerprints "same"). Commit `M11ar Task 0: the agent rows in their
  own file (move only)`.

### Task 1: The shared model — events, lanes, controls

**Files:** `src/providers/mod.rs`, `src/agent_conversation.rs`, `src/agent_lanes.rs` (new, pure), `src/main.rs` (mod).

**Interfaces — produces:**
- `Event::SubagentStarted { id: String, parent: Option<String>, label: String, kind: String, task: String,
  model: Option<String>, controls: LaneControls }`; `#[derive(Clone, Copy, Debug, Default, PartialEq)] pub struct
  LaneControls { pub stop: bool }`.
- `Event::SubagentProgress { id, tokens: Option<u64>, tools: Option<u32>, last_tool: Option<String> }`.
- `Event::SubagentEnded { id, end: LaneEnd }`; `pub enum LaneEnd { Done, Failed(String), Stopped }`.
- `Event::Follow { id: String, path: PathBuf }`.
- `Event::Permission` and `Item::Permission` gain `by: Option<String>`.
- `Action::StopSubagent { lane: String }`.
- `Translator::decode_followed(&mut self, lane: &str, line: &str) -> Vec<Event>` (default empty).
- `Lane { id, parent: Option<String>, label, kind, task, model: Option<String>, status: LaneStatus, tokens:
  Option<u64>, tools: Option<u32>, last_tool: Option<String>, controls: LaneControls }`; `pub enum LaneStatus {
  Working, Waiting, Done, Failed(String), Stopped }`; `Conversation::{lanes(), lane(id), lane_steps(id) -> usize}`.
- `agent_lanes`: `pill_label(label, max) -> String`, `pill_labels(&[Lane], max) -> Vec<String>`, `enum View { Main,
  Lane(String) }` (Hash, Eq), `step(&View, &[Lane], dir) -> View`.

- [ ] Tests (`agent_conversation`): `a_subagent_becomes_a_lane`; `progress_fills_the_lanes_stats`;
  `an_end_sets_done_failed_or_stopped`; `a_late_done_replaces_stopped`; `a_lanes_permission_makes_it_waiting`;
  `a_step_after_done_reopens_the_lane`; `process_end_stops_working_lanes` (process_ended, paused, paused_with);
  `clearing_drops_the_lanes` (`Cleared`, `keep_first`); `settling_a_turn_leaves_a_working_lanes_steps` (at `TurnDone`
  a working lane's running tool stays running and its open permission stays open; they settle when the lane ends);
  `lane_steps_counts_its_tools`.
- [ ] Tests (`agent_lanes`): `pill_label("Workspace File Reader", 12) == "Workspace F…"`; repeats `["Reader",
  "Reader 2"]`; `step` (Main +1 → first lane, Main −1 → Main, last +1 → last).
- [ ] Fail, implement, pass. Gate. Commit `M11ar Task 1: the shared subagent model — events, lanes, controls`.

### Task 2: Claude — lanes from its stream, Stop, who asks, resume

**Files:** `src/providers/claude.rs`; fixtures `claude-subagents.jsonl`, `claude-subagent-stop.jsonl`,
`claude-subagent-ask.jsonl` (new, recorded); a fixtures hygiene test in `src/providers/mod.rs`.

- [ ] Record (in `/tmp/fx`, `--model haiku`): two parallel subagents; one stopped mid-run with `stop_task`; one whose
  Bash asks. Scrub (paths → `/Users/me`, init noise out). Note which field links `can_use_tool.agent_id` to
  `task_started`.
- [ ] Tests: `task_started_becomes_a_lane` (label: custom `subagent_type`, else `description`; built-ins
  `general-purpose`, `Explore`, `Plan` aren't names; `kind` = `subagent_type`; `task` = `prompt`; `id` =
  `tool_use_id`; `controls.stop`); `task_progress_becomes_progress`; `task_notification_ends_the_lane` (completed →
  Done, failed → Failed(reason), killed/stopped → Stopped); `two_subagents_keep_their_own_steps`;
  `a_background_lane_ends_after_the_turn`; `a_subagents_permission_says_who`; `stop_subagent_writes_stop_task`
  (encode by lane → its `task_id`); `the_stop_fixture_ends_one_lane_only`;
  `history_restores_lanes_by_the_live_rule`; `no_fixture_names_a_real_home` (every `src/providers/fixtures/*` has no
  `/Users/` other than `/Users/me`).
- [ ] Fail, implement (`system`: `task_started`, `task_progress`, `task_notification`; `can_use_tool`; `encode`;
  `history`), pass. Gate. Commit `M11ar Task 2: Claude's subagents — lanes, Stop, who asks, resume`.

### Task 3: agy — the subagent step and the followed transcript

**Files:** `src/providers/agy.rs`, `src/lane_follow.rs` (new, pure), `src/agent_lanes_view.rs` (new: the tab's
followers), fixtures `agy-subagents.jsonl`, `agy-subagent-transcript-{a,b}.jsonl` (recorded, scrubbed).

**Interfaces:**
- agy's `subagent` step, per `subagents[n]`: lane id `<step_id>#<n>`; ACTIVE → `ToolStarted { id, name: "Subagent",
  summary: role, … }` + `SubagentStarted { label: role, kind: type_name, task: initial_prompt, controls: default }`;
  DONE (spawned) → `ToolFinished { id, ok: true, … }` + `Follow { id, path }` (only `file://` `log_uri`s,
  percent-decoded). On `result`: `SubagentEnded { Stopped }` for each lane without a report.
- `AgyTranslator::decode_followed`: `PLANNER_RESPONSE` → `MessageDone{parent: Some(lane)}` + `ToolStarted{parent:
  Some(lane)}` per call (ids `<lane>-<step>-<n>`, remembered); `GENERIC` → `ToolFinished` for the remembered calls;
  `send_message` to the parent conversation (from `init`) → `SubagentEnded{Done}`; malformed → nothing.
- `lane_follow`: `pub struct Tail { offset: u64, partial: String }`, `Tail::feed(&mut self, bytes: &[u8]) ->
  Vec<String>` (complete lines only), `pub fn read_new(path: &Path, from: u64, max: usize) -> io::Result<Vec<u8>>`
  (≤ 1 MB; a file shorter than `from` → `Err`, the tail stops).
- `AgentTab.followers: HashMap<String, Task<()>>`: per `Follow`, a task looping 500 ms → `read_new` off the UI thread →
  `on_followed(lane, lines)` (lines through the translator). Kept while the process lives (also after Done); one last
  read on `TurnDone`; dropped on process end, `Cleared` and tab close.

- [ ] Record one `agy -p` run with two parallel subagents in `/tmp/fx`; copy both transcripts; scrub.
- [ ] Tests (agy): `a_subagent_step_is_a_row_and_a_lane`; `spawned_starts_following`; `two_subagents_are_two_lanes`;
  `a_transcript_line_becomes_the_lanes_steps`; `the_report_to_the_parent_ends_the_lane`;
  `a_bad_line_is_skipped`; `the_turns_end_stops_unreported_lanes`; `only_file_uris_are_followed`.
- [ ] Tests (`lane_follow`, temp files removed after): `new_lines_arrive_in_order`; `a_half_written_line_waits`;
  `a_missing_file_reads_nothing_yet`; `a_shrunk_file_stops_the_tail`; `reads_are_capped`.
- [ ] Fail, implement, pass. Gate. Commit `M11ar Task 3: agy's subagents — the subagent step and its followed
  transcript`.

### Task 4: The pill strip and a list per view

**Files:** `src/agent_lanes_view.rs` (strip, view state, keys), `src/agent_tab.rs` (one `ListState` + rows per view),
`src/agent_chat.rs` (`rows_of`), `src/main.rs`, `src/help_shortcuts.rs`, `src/ui_shots/agent_lanes.rs` (new),
`src/ui_shots/registry.rs`.

**Interfaces:** `agent_chat::rows_of(items: &[Item], view: &View) -> Vec<usize>` (Main: today's rows; a lane: its
items) replaces per-item `is_row`; `gap_before`, `group_shape`, `shows_avatar`, `row_print` take the row slice.
`AgentTab.panes: HashMap<View, Pane>` (`Pane { list: ListState (FollowMode::Tail), rows }`), synced when shown;
`AgentTab::show_view(&mut self, View, cx)`; actions `PrevLane`/`NextLane` (⌥⌘[ / ⌥⌘]); `agent_lanes::fit(widths:
&[f32], avail: f32, active: usize) -> (Vec<usize>, usize)` (shown pills, hidden count) fed by the strip's width
measured last frame.

- [ ] Tests: `rows_of` (Main hides lane items; a lane view only its own); `fit` (all fit; overflow → "+N", the active
  pill kept); `the_lane_keys_are_bound_in_the_agent_tab`.
- [ ] Build the strip per the spec (32 pt, under the top bar, in the centred column; marks/tints from the tool row's
  tokens; active neutral fill; "+N" popup in the `tab_menu` style; tooltip kind · task · elapsed).
- [ ] Scenes `agent-lanes-strip` (two pills, working and done; wide and narrow; both themes),
  `agent-lanes-overflow` (eight lanes: "+N", popup), `agent-lanes-states` (failed and stopped marks). Gate. Commit
  `M11ar Task 4: the pill strip and a list per view`.

### Task 5: Main's subagent row and the lane view

**Files:** `src/agent_lanes_view.rs`, `src/agent_rows.rs`, `src/agent_tool_modal.rs` (sub-agent steps section out;
subagent rows not openable), `src/ui_shots/agent_lanes.rs`.

- [ ] Tests: `a_subagent_row_opens_its_lane_not_the_modal`; `elapsed_stops_when_the_lane_ends` (pure:
  `agent_lanes::elapsed(started, ended, now) -> Option<Duration>`; none without a start).
- [ ] Build: Main's subagent row from the lane (label, task, last tool or "N steps", elapsed, ›; click → its lane);
  the lane header (label SEMIBOLD; kind, model `text_dim`; elapsed/tokens/tools `text_faint` when known); the task as
  the first bubble captioned "from Main" (plain text); "Thinking…" between steps while it runs; end lines ("✓ Done in
  … · Back to Main ›" — or "✓ Done · Back to Main ›" without a time — "✕ Failed · reason", "Stopped"); Back to Main
  scrolls Main to that row.
- [ ] Scenes `agent-lanes-view` (working lane; done lane; failed; stopped). Gate. Commit `M11ar Task 5: Main's
  subagent row and the lane view`.

### Task 6: Stop, who asks, the box in a lane, narrow

**Files:** `src/agent_lanes_view.rs`, `src/agent_rows.rs` (permission card heading), `src/agent_box.rs`,
`src/ui_shots/agent_lanes.rs`.

- [ ] Tests: `sending_from_a_lane_returns_to_main`; `stop_shows_only_when_the_lane_can` (pure:
  `agent_lanes::shows_stop(&Lane) -> bool`: working with `controls.stop`).
- [ ] Build: Stop chip → `Action::StopSubagent { lane }`; the box placeholder "Message Main — it can pass this to
  {label}"; the permission card "{label} wants to run {tool}" in the shown view, the asking lane's pill tinted, Main's
  notch "Needs input"; under 480 pt the header keeps elapsed and Stop.
- [ ] Scenes `agent-lanes-ask`, `agent-lanes-narrow`, `agent-lanes-agy` (agy fixtures: rows, pills, a followed lane,
  no Stop). Gate. Commit `M11ar Task 6: Stop, who asks, the box in a lane`.

### Task 7: Finish — screenshots, review, docs

- [ ] Full `scripts/ui-shots.sh`; one whole-branch review on the most capable model; one fix wave; the full screenshot
  run again.
- [ ] Docs: note 93 "as built" and its checklist, Roadmap (M11ar built), Plan Tracker, Progress Log. Commit.
