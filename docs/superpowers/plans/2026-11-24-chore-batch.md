# Chore batch 2026-11-24 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task (native, your call 2026-11-19). Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The twelve chores agreed on 2026-11-24, built small → medium → big.

**Architecture:** Pure, GPUI-free helpers carry each rule so it is unit-tested (`hover_delay`, `caret_blink`,
`tab_bar::tab_mark`, `agent_status_line`, `undo_history`, `plan_progress`, `agent_phases`); the views only ask them.
Shared pieces are written once and used by every caller (the hover delay by the gauge and the clock; the undo history
by `Composer` and `NameInput`).

**Tech Stack:** Rust, GPUI (pinned git rev), Lucide SVGs in `assets/icons/lucide`; zsh/bash in fuxx-completes.

**Spec:** `docs/superpowers/specs/2026-11-24-chore-batch-design.md`

## Global Constraints

- Hover panels open after **800 ms** of rest; never on click.
- Caret blink: **500 ms** on/off, solid while typing, idle stop after **20 s**; a timer, never `with_animation`.
- Context warning dot past **80 %**.
- Gate per tier (not per task, your call): `cargo test --bin fuxx`, both `cargo clippy` runs as note 50 lists them,
  `cargo fmt --check`, the affected `scripts/ui-shots.sh` scenes, then `scripts/prune-target.sh`.
- Test scratch only through `crate::test_temp::dir`. Provider quirks stay in `src/providers/<agent>.rs`.
- No work-project names in code, scenes or docs (public repo): use fuxx-cli's own paths in fixtures.
- Stage files by name; never `git add -A`.

## Review Focus

- A hover that leaves and comes back within 800 ms must restart the wait, not open early (a stale timer firing).
- A workspace restored from an old sessions file (no saved folder) must still find its folder from its terminal.
- Undo after a send/commit must not bring back the sent text; ⌘Z at a shell prompt must not reach vim/htop.
- A long running action or folder name must never push anything past the folder tab's edge (the bug behind 12).
- No burger for a conversation without a task list, or with a single-item list.

---

### Task 1 (small): the hover delay, the gauge on hover, the gap, the tab marks

**Files:**
- Create: `src/hover_delay.rs` (pure; `pub const OPEN_AFTER: Duration = Duration::from_millis(800)`).
- Modify: `src/agent_box.rs` (usage button ~1190–1250: drop `on_click`, hover on the button and its panel),
  `src/agent_tab.rs` (status pill padding ~5650), `src/card_view.rs` (shell tab gap), `src/tab_bar.rs` (~618, ~823–845,
  `select_tab` ~1175), `src/workspace.rs` (`Session.done_unseen`, `set_status_told` ~2653), `src/workspace_agent.rs`
  (agent slot `done_unseen`, ~1340).

**Interfaces — Produces:**
- `HoverDelay` with `enter(&mut self, part: Part) -> Option<u64>` (a ticket to wait on, when nothing was over before),
  `leave(&mut self, part: Part) -> bool` (true: it closed), `fire(&mut self, ticket: u64) -> bool` (true: open now),
  `is_open(&self) -> bool`; `Part::{Anchor, Panel}`. Used again by Task 3.
- `tab_bar::tab_mark(status: Status, done_unseen: bool) -> Option<TabMark>`; `TabMark::{Working, Waiting, Unseen}`.
- `workspace::done_unseen_after(old: Status, new: Status, shown: bool, was: bool) -> bool`.

- [ ] Tests (`hover_delay.rs`): `opens_after_a_rest`, `leaving_before_the_delay_opens_nothing`,
  `coming_back_restarts_the_wait` (the old ticket's `fire` is false), `the_panel_keeps_it_open`.
- [ ] Tests (`tab_bar.rs`): `idle_and_done_draw_no_mark`, `working_and_waiting_keep_their_marks`,
  `an_unseen_finish_draws_the_dot`, `natural_width_reserves_no_dot_for_done`.
- [ ] Tests (`workspace.rs`): `done_while_not_shown_is_unseen`, `done_while_shown_is_seen`, `leaving_done_clears_unseen`.
- [ ] Implement; the Unseen dot reuses `title_area::unseen_dot`; the gap ≈ half of today's.
- [ ] Commit.

### Task 2 (small): the blinking caret; the gutter-bar suggestion list

**Files:**
- Create: `src/caret_blink.rs` (pure: `shown(&self, now) -> bool`, `next_flip(&self, now) -> Option<Duration>`; None once
  idle 20 s, unfocused or the window in the background).
- Modify: `src/name_input.rs` (caret painted only when shown; one timer; reset on input and focus; the cursor-glow
  preference off → always solid).
- fuxx-completes: `adapters/zsh/fuxx-autosuggestions.plugin.zsh` (`_fuxx_as_list_draw` ~1205), the bash adapter's
  list drawing, their tests.

- [ ] Tests (`caret_blink.rs`): `solid_while_typing`, `flips_every_half_second`, `stops_after_twenty_idle_seconds`,
  `unfocused_has_no_timer`.
- [ ] Tests (fuxx-completes): a blank first line; rows `▎ N  item`; no title or hint; the selected bar's span in the
  accent style, the rest `fg=8`.
- [ ] Implement; commit in each repo.

**Small-tier gate.**

### Task 3 (medium): the clock; the status out of the folder tab

**Files:**
- Add: `assets/icons/lucide/clock.svg` (Lucide's), `Icon::Clock` in `src/icons.rs`.
- Create: `src/agent_status_line.rs` (pure: `status_line(&LineState) -> (String, Option<String>)`).
- Modify: `src/agent_box.rs` (the clock between the model pill and the gauge, `HoverDelay`, the warning dot, narrow
  footer), `src/agent_top.rs` + `src/agent_tab.rs` ~5790–5830 (the context row leaves the top bar into the clock panel),
  `src/agent_rows.rs` ~97 + `src/agent_chat.rs` (the thinking line becomes the status line + counts line),
  `src/agent_tab.rs::status_pill` (icon only).

- [ ] Tests (`agent_status_line.rs`): `working_names_the_running_action_and_its_time`
  (`"Cultivating… · Run the full e2e suite · 0:31"`), `parallel_actions_say_plus_more`, `a_plan_leads_with_the_task`,
  `waiting_says_waiting_for_you` (`"Waiting for you · Allow Bash?"`), `done_says_done_and_the_time` (`"✓ Done · 0:42"`),
  `counts_line_joins_changed_and_checks` (`"4 changed · checks 3/7"`; None when neither).
- [ ] Tests: `context_warning_past_80_percent`, `narrow_footer_shortens_the_model_first`.
- [ ] Implement; scenes for the clock panel and the status line; commit.

### Task 4 (medium): close confirmation; the workspace keeps its folder

**Files:**
- Modify: `src/tab_bar.rs` (× and ⌘W), `src/tab_menu.rs` (`confirm_close`, `confirm_workspace_text`), Preferences +
  prefs (`OnOff::ConfirmClose`, default on), the window-close path; `src/layout_model.rs` (`Workspace.folder:
  Option<PathBuf>`, serde default), `src/workspace.rs` (`folder_group`, `active_project`, set at creation), save/restore.

- [ ] Tests: `closing_an_agent_or_terminal_tab_asks` (browser/file never), `dont_ask_again_turns_the_pref_off`,
  `window_close_lists_what_runs`.
- [ ] Tests (written first, seen failing): `a_workspace_keeps_its_folder_with_only_an_agent_tab`,
  `a_new_tab_from_an_agent_only_workspace_gets_its_folder`, `an_old_save_without_a_folder_still_groups_by_its_terminal`.
- [ ] Implement; commit.

**Medium-tier gate.**

### Task 5 (big): readable activity tracks

**Files:**
- Create: `src/agent_phases.rs` (pure: `Phase::{Looking, Ran, Changing, Checking}`, `phase_of`, `bands(&[Phase]) ->
  Vec<(Phase, Range<usize>)>`).
- Modify: the tracks view, `src/agent_steps.rs` (summary words), theme tokens for the tile tints (both themes),
  the `agent-tracks-*` scenes.

- [ ] Tests: `reads_and_searches_are_looking_around`, `commands_ran`, `edits_are_changing`, `checks_are_checking`,
  `bands_merge_runs_of_one_phase`, `a_failed_edit_says_first_try_failed`, `a_planned_failure_says_expected`,
  `the_column_tip_names_the_action` (`"#5 · Read tab_bar.rs, lines 1280–1296"`).
- [ ] Implement; commit.

### Task 6 (big): undo and redo everywhere

**Files:**
- Create: `src/undo_history.rs` (pure, generic over the field's state: `record`, `undo`, `redo`, `clear`;
  `EditKind::{Typed(char), Paste, Cut, Delete, Chip, Recall}`; a run of typed word characters is one step).
- Modify: `src/composer.rs`, `src/name_input.rs` (`cmd-z`, `cmd-shift-z`), the send and commit paths (`clear`),
  `src/terminal_view.rs` (⌘Z → `\x1f`, ⇧⌘Z → the redo sequence, only at the shell's prompt), fuxx-completes' zsh
  plugin (`bindkey` for `redo`).

- [ ] Tests: `typing_a_word_undoes_as_one_step`, `paste_is_one_step`, `redo_after_undo`, `a_new_edit_drops_redo`,
  `removing_a_chip_undoes`, `recall_undoes_to_the_draft`, `send_clears_history`, `cmd_z_at_the_prompt_sends_undo`,
  `cmd_z_while_a_program_runs_passes_through`.
- [ ] Implement; commit in each repo.

### Task 7 (big): plan progress

**Files:**
- Create: `src/plan_progress.rs` (pure: tasks with `Done { commit } | Building | Next`, `from_todos`, `from_checklist`,
  `match_commits`, `counts`).
- Modify: `src/providers/claude.rs` (Claude's current task tools → `Event::Todo`; names confirmed from a real stream
  first), `src/agent_box.rs` (the burger tab, its popover, a key), `src/icons.rs` (`Icon::Menu`, Lucide `menu.svg`).

- [ ] Tests: `todos_become_tasks`, `a_checklist_becomes_tasks`, `a_commit_named_task_3_marks_task_3`,
  `the_first_unfinished_is_building`, `no_list_no_burger`, `all_done_keeps_6_of_6`, `subagents_get_their_own_list`,
  `task_tools_map_to_a_todo_event`.
- [ ] Implement; scenes `agent-plan-burger`, `agent-plan-popover`; commit.

**Big-tier gate**, then the full `scripts/ui-shots.sh`, one whole-branch review on the most capable model, one fix
wave, the full run again; note 93 "Chores after M11ar" and a Progress Log entry; merge to master (standing word);
push and release only on your word.
