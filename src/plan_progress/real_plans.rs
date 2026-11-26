//! Chore 32 (the ☰ tab sat at 8/9 on a built plan): a file a task only runs isn't one it
//! changes, and a task that changes none ticks once a commit lands after the previous task's.
//! Read against the real chores 25–30 plan and its commits.

use super::*;

/// The chores 25–30 plan (docs/superpowers/plans/2026-11-26-chores-25-30.md) as it was built,
/// boxes unticked: its task headings, `**Files:**` lines and steps, verbatim.
const CHORES_25_30_PLAN: &str = r##"
### Task 1: Chore 27 — a mode chosen mid-turn goes to Claude at once
**Files:** Modify `src/agent_box.rs` (`step_for` + its table test), `src/providers/mod.rs` (`Provider::mode_mid_turn`), `src/providers/claude.rs` (true; a fixture test), `src/agent_tab.rs` (`apply_changes`), `src/ui_shots/agent_box.rs` (`agent-box-mode-follows-claude`'s mid-turn leg); add `src/providers/fixtures/claude-mode-mid-turn.jsonl`, `claude-plan-auto.jsonl`, `scripts/record-agent-fixture.py` (scenarios `mode-mid-turn`, `plan-auto`).
- [ ] Test first: `step_for_covers_every_case` gains rows `(Live, running, spoken, working, mid_turn=true) → Step::Live` and keeps `AfterTurn` with mid_turn false; a Claude test decodes `claude-mode-mid-turn.jsonl` and finds `Event::Applied { kind: Mode, value: "auto", ok: true }` then `Event::Mode("auto")` before the turn's `TurnDone`. Run → FAIL.
- [ ] Implement: `apply_changes` passes `mid_turn = matches!(change, Change::Mode(_)) && provider.mode_mid_turn()`. Update the scene's leg: a mode chosen mid-turn is written at once (`set_permission_mode`), nothing waits. → PASS. Gates. Commit `Chore 27: …`.
### Task 2: Chore 26 — "Approve · Auto mode" on the plan card
**Files:** Modify `src/agent_conversation.rs` (`Answer::ApprovedInto { mode, label }`), `src/providers/mod.rs` (`Action::AllowInto { id, mode }`, `Provider::plan_into_mode`), `src/providers/claude.rs` (encode; `plan_into_mode` → `Some("auto")`), `src/providers/agy.rs` (ignores `AllowInto`), `src/agent_tab.rs` (`answer`, `plan_answer_line`, `answer_line`), `src/agent_plan_card.rs` (the button), `src/agent_rows.rs` (approved check), `src/ui_shots/agent_activity.rs` (scene `agent-plan-approved-auto`), `src/ui_shots/registry.rs`.
- [ ] Test first: Claude encodes `AllowInto { id, mode: "auto" }` for a pending ExitPlanMode as `behavior: allow` with `updatedPermissions == [{"type":"setMode","mode":"auto","destination":"session"}]` (from `claude-plan-auto.jsonl`'s request); `plan_answer_line(&ApprovedInto{mode:"auto",label:"Auto"}) == "Plan approved · Auto mode ✓"`. Run → FAIL.
- [ ] Implement: the card's buttons read `Approve · auto-accept ⏎`, `Approve · Auto mode` (when `plan_into_mode` names a mode the provider lists; label from `ProviderInfo::modes`), `Approve`, `Keep planning`. Answering it writes `AllowInto` and sets the tab's mode to it as a report would (`reported_mode`), so the chip shows Auto at once. Scene `agent-plan-approved-auto`: click it; the written line carries the setMode; the folded line reads "Plan approved · Auto mode ✓"; `mode_now` is `auto`. → PASS. Gates. Commit `Chore 26: …`.
### Task 3: Chore 30 — chevrons on the cards' fold links
**Files:** Create `assets/icons/lucide/chevron-up.svg`, `src/agent_card_parts.rs` (`fold_link`, `slim_button`); modify `src/icons.rs` (`Icon::ChevronUp`), `src/agent_plan_card.rs` (Show plan / Show less / Show all; buttons via `slim_button`), `src/card_view.rs` (`Fold { words, chevron }` for the shell card's fold), `src/terminal_view.rs` (paints the chevron sprite before the words), `src/main.rs` (mod), `src/ui_shots/agent_activity.rs` (the plan scenes pin `plan-show-chevron` / `plan-more-chevron`).
- [ ] Test first: `card_view` fold test expects `Fold { words: "Show less", chevron: Up }` / `"12 earlier lines", Right` (no glyph in the words); an `icons` test that `Icon::ChevronUp`'s file loads and is in `ALL`; a test that no fold label in `agent_plan_card.rs` holds ▾ ▴ › (`plan_fold_words`). Run → FAIL.
- [ ] Implement; the scenes check the chevron's box sits right of the words and the words' height ≥ the chevron's. → PASS. Gates. Commit `Chore 30: …`.
### Task 4: Chore 25 — a calmer, shorter permission card
**Files:** Create `src/agent_permission_card.rs` (the open card; `fold_lines`); modify `src/agent_rows.rs` (calls it; drops the old card), `src/providers/mod.rs` + `src/agent_conversation.rs` (`about: Option<String>` on `Event::Permission` / `Item::Permission`), `src/providers/claude.rs` (`about` = a Bash input's `description`; `detail` empty for Bash; Bash rules read `git commit *`), `src/agent_tab.rs` (`input_open` set), every `Event::Permission {` constructor (scenes, tests), `src/ui_shots/agent.rs` (scenes `agent-permission-heredoc`, `agent-permission-narrow`; `agent-tab-permission` pins the non-Bash card).
- [ ] Test first: `fold_lines("a\nb\nc\nd\ne\nf\ng\nh", 6) == ("a\nb\nc\nd\ne\nf", 2)`, a single long line → `(line, 0)`; Claude: a Bash request → `about == Some(description)`, `detail == ""`, `always == "Always allow git commit *"` for rule content `git commit:*`; Write → `about None`, `detail` the JSON. Run → FAIL.
- [ ] Implement the card: one header line (shield 14 in accent, the title = `about` or "{label} wants to run {tool}", a small `Bash` tag; a subagent's label in the tag), the command once (mono, `text::META + 0.5`, folded after 6 lines: "… N more lines · Show all" + chevron-down, open: "Show less" + chevron-up, state in `command_open`), "Show input" + chevron-right (open: chevron-down) when `detail` isn't empty, the answers as `slim_button`s (`Allow ⏎`, the always label, `Deny`) and "or type to redirect" in dim on the same row; a 1.5 px accent border on the panel fill, no tinted fill. Scenes: short Bash, long heredoc (folded, then Show all), Write (Show input), narrow pane (buttons wrap, nothing past the card). → PASS. Gates. Commit `Chore 25: …`.
### Task 5: Chore 28 — the commit box's caret: a thin blinking bar
**Files:** Modify `src/caret_blink.rs` (`Blinker`: the blink state, its `seen` key and one timer), `src/name_input.rs` (uses `Blinker`), `src/composer.rs` (`set_plain_caret`; paints a `cursor_look::BAR`-wide bar in the text colour while `Blinker` says shown; no glide, glow, trail or sparks), `src/commit_line.rs` (sets it), `src/ui_shots/changes.rs` (`changes-commit` checks the caret's shape and width).
- [ ] Test first: `Blinker` tests (a changed `seen` restarts solid; a wait is asked once while a timer runs; none unfocused); a scene assertion that the commit box's caret is `PaintShape::Bar` of width `BAR` with `Frames::None` (no breath). Run → FAIL.
- [ ] Implement; the timer notifies only the Composer entity (no parent redraw). → PASS. Gates. Commit `Chore 28: …`.
### Task 6: Chore 29 — one inner padding for every text input
**Files:** Modify `src/ui_tokens.rs` (`pub mod input { pub const PAD_X: f32 = 6.0; }`), `src/name_input.rs` (its frame's `px`), the bare inputs' frames that put text closer than `PAD_X` to their edge (found by a search of `NameInput::new` / `Composer::new` / `filter_box` callers), `src/ui_shots/*` (the input scenes check the text's left ≥ the frame's left + `PAD_X`).
- [ ] Test first: a token test (`input::PAD_X` between 4 and 8) and scene checks on the rename box, the branch picker's input and the commit box. Run → FAIL where the gap is short.
- [ ] Implement in one place each frame is drawn. → PASS. Gates. Commit `Chore 29: …`.
### Task 7: Chore 24 — scenes that end on the folded approved plan (added by the user, 2026-11-26: "include 24 too")
**Files:** Modify `src/ui_shots/agent_activity.rs` (scenes `agent-plan-approved-folded` and Task 2's `agent-plan-approved-auto` end on the folded line), `src/ui_shots/registry.rs`.
- [ ] Test first: the scene asserts `plan-answered` is shown, `plan-answer-line`'s words (`plan_answer_line`) read "Plan approved · auto-accept edits ✓", `plan-show` and its chevron are in the line, and no `plan-body` is drawn. Run → FAIL (no scene). Implement → PASS. The landing's `pnpm sync-shots` / ship is left to the coordinator. Gates. Commit `Chore 24: …`.
### Task 8: Chore 31 — only the status line spins (added by the user, 2026-11-26: "yes, build 31 too")
**Files:** Modify the relay chip ("To <subagent> · via Main", found by search in `src/agent_box*.rs` / `src/agent_lanes_view.rs`) and the 🤖 corner tab (`src/agent_subagents_view.rs`) to ask chore 17's `status_line_shown` (one signal, no second); `src/ui_shots/agent_subagents.rs` (a scene with a running subagent and the relay chip).
- [ ] Test first: a pure rule (`running_mark(status_line_shown, running) -> bool`, or the existing helper if one fits) tested both ways; the scene asserts the relay chip draws the 🤖 icon (no arc part) and the corner tab has no arc while the status line shows. Run → FAIL. Implement → PASS. Gates. Commit `Chore 31: …`.
### Task 9: Docs, screenshots, review
- [ ] `scripts/ui-shots.sh --changed 5ba8ffc`; look at the changed scenes' PNGs. One whole-change review (Opus), one fix wave, tests first. Note 93's six entries flipped to built; a Progress Log entry. `scripts/prune-target.sh`; scratch removed.
"##;

fn commit(hash: &str, subject: &str, files: &[&str]) -> GitCommit {
    GitCommit {
        hash: hash.into(),
        subject: subject.into(),
        at: 0,
        files: files.iter().map(|f| f.to_string()).collect(),
    }
}

/// The plan's commits on master (`git log 5ba8ffc..991d32a --name-only`), oldest first.
fn chores_25_30_commits() -> Vec<GitCommit> {
    vec![
        commit(
            "8b1e0ca",
            "Chore 27: a mode chosen mid-turn goes to Claude at once — Claude takes set_permission_mode mid-turn (recorded), so it no longer waits for the turn to end; plan for chores 24–30",
            &[
                "docs/superpowers/plans/2026-11-26-chores-25-30.md",
                "scripts/record-agent-fixture.py",
                "src/agent_box.rs",
                "src/agent_tab.rs",
                "src/providers/claude.rs",
                "src/providers/fixtures/claude-mode-mid-turn.jsonl",
                "src/providers/fixtures/claude-plan-auto.jsonl",
                "src/providers/mod.rs",
                "src/ui_shots/agent_box.rs",
            ],
        ),
        commit(
            "237ffb7",
            "docs: chore 31 queued — only the status line spins",
            &["docs/04-Development/93 - Milestone 11ar - Subagents.md"],
        ),
        commit(
            "e12608d",
            "Chore 26: approve a plan into Auto mode — \"Approve · Auto mode\" allows the plan with the session set to Auto (Claude's setMode, recorded), the mode chip shows Auto at once",
            &[
                "docs/superpowers/plans/2026-11-26-chores-25-30.md",
                "src/agent_conversation.rs",
                "src/agent_plan_card.rs",
                "src/agent_rows.rs",
                "src/agent_tab.rs",
                "src/providers/agy.rs",
                "src/providers/claude.rs",
                "src/providers/mod.rs",
                "src/ui_shots/agent_activity.rs",
                "src/ui_shots/registry.rs",
            ],
        ),
        commit(
            "35000d5",
            "Chore 30: chevrons, not arrow glyphs, on the cards' fold links",
            &[
                "assets/icons/lucide/chevron-up.svg",
                "src/agent_card_parts.rs",
                "src/agent_plan_card.rs",
                "src/agent_tab.rs",
                "src/card_view.rs",
                "src/icons.rs",
                "src/terminal_view.rs",
                "src/ui_shots/agent_activity.rs",
            ],
        ),
        commit(
            "4ccfb3d",
            "Chore 25: a calmer, shorter permission card",
            &[
                "src/agent_card_parts.rs",
                "src/agent_chat.rs",
                "src/agent_conversation.rs",
                "src/agent_items.rs",
                "src/agent_permission_card.rs",
                "src/agent_rows.rs",
                "src/agent_tab.rs",
                "src/agent_view_cache/tests.rs",
                "src/providers/claude.rs",
                "src/providers/mod.rs",
                "src/ui_shots/agent.rs",
                "src/ui_shots/agent_activity.rs",
                "src/ui_shots/agent_box.rs",
                "src/ui_shots/agent_lanes.rs",
                "src/ui_shots/agent_permission.rs",
                "src/ui_shots/chrome.rs",
                "src/ui_shots/mod.rs",
                "src/ui_shots/registry.rs",
            ],
        ),
        commit(
            "9ef8d2a",
            "Chore 28: a thin blinking caret in the commit box",
            &[
                "src/caret_blink.rs",
                "src/commit_line.rs",
                "src/composer.rs",
                "src/name_input.rs",
                "src/ui_shots/changes.rs",
            ],
        ),
        commit(
            "a01d116",
            "Chore 29: a little padding inside every text input",
            &[
                "src/branch_switcher.rs",
                "src/name_input.rs",
                "src/overlay_panel.rs",
                "src/ui_shots/branch.rs",
                "src/ui_tokens.rs",
            ],
        ),
        commit(
            "af083d6",
            "Chore 24: a scene that ends on the folded approved plan",
            &["src/ui_shots/agent_activity.rs", "src/ui_shots/registry.rs"],
        ),
        commit(
            "10f87f2",
            "Chore 31: only the chat's running mark spins",
            &[
                "src/agent_box.rs",
                "src/agent_lanes_view.rs",
                "src/agent_shells_view.rs",
                "src/agent_subagents.rs",
                "src/agent_subagents_view.rs",
                "src/agent_tab.rs",
                "src/ui_shots/agent_lanes.rs",
            ],
        ),
        commit(
            "868dca3",
            "Chores 27, 31: a mode asked mid-turn shows on the chip at once; the scenes follow",
            &[
                "src/agent_tab.rs",
                "src/ui_shots/agent_box.rs",
                "src/ui_shots/agent_subagents.rs",
            ],
        ),
        commit(
            "1a0872d",
            "Chore 30: the shell card's fold row pinned",
            &["src/terminal_view.rs", "src/ui_shots/terminal.rs"],
        ),
        commit(
            "15ec0d5",
            "Chore 31 (review): the folder tab keeps a wait on you in a subagent's view",
            &[
                "src/agent_subagents.rs",
                "src/agent_tab.rs",
                "src/ui_tokens.rs",
            ],
        ),
        commit(
            "991d32a",
            "docs: chores 24–31 built — note 93 entries flipped with causes, commits and checks; Progress Log entry",
            &[
                "docs/04-Development/93 - Milestone 11ar - Subagents.md",
                "docs/06-Progress-Log/60 - Progress Log.md",
            ],
        ),
    ]
}

fn commit_of(t: &PlanTask) -> Option<&str> {
    match &t.state {
        TaskState::Done { commit } => commit.as_deref(),
        _ => None,
    }
}

/// The plan tab's reading of the real plan with every commit landed: 9 of 9, each task by the
/// commit that built it, the closing task by the docs commit (the review: not by a fix-wave
/// commit, which touches the plan's files).
#[test]
fn chores_25_30s_plan_reads_9_of_9() {
    let mut newest_first = chores_25_30_commits();
    newest_first.reverse();
    let mut tasks = from_checklist(CHORES_25_30_PLAN);
    mark_done(&mut tasks, CHORES_25_30_PLAN, &newest_first);
    assert_eq!(counts(&tasks), (9, 9), "{tasks:?}");
    let by: Vec<Option<&str>> = tasks.iter().map(commit_of).collect();
    assert_eq!(
        by,
        [
            Some("8b1e0ca"),
            Some("e12608d"),
            Some("35000d5"),
            Some("4ccfb3d"),
            Some("9ef8d2a"),
            Some("a01d116"),
            Some("af083d6"),
            Some("10f87f2"),
            Some("991d32a"),
        ]
    );
}

/// Replayed commit by commit: the closing task waits for a commit after Task 8's that touches
/// none of the plan's files (the fix wave's 868dca3, 1a0872d, 15ec0d5 do; the docs one doesn't).
#[test]
fn the_closing_task_waits_for_a_commit_after_the_last_built_one() {
    let all = chores_25_30_commits();
    let at = |hash: &str| all.iter().position(|c| c.hash == hash).unwrap();
    for landed in 1..=all.len() {
        let mut newest_first = all[..landed].to_vec();
        newest_first.reverse();
        let mut tasks = from_checklist(CHORES_25_30_PLAN);
        mark_done(&mut tasks, CHORES_25_30_PLAN, &newest_first);
        let closing_done = matches!(tasks[8].state, TaskState::Done { .. });
        assert_eq!(
            closing_done,
            landed > at("991d32a"),
            "after {}",
            all[landed - 1].hash
        );
    }
}

/// Only the `**Files:**` block names the files a task changes: a script it adds there counts;
/// the closing task, which only runs scripts, names none.
#[test]
fn only_the_files_block_names_changed_files() {
    let files = task_files(CHORES_25_30_PLAN);
    assert_eq!(files.len(), 9);
    assert!(
        files[0]
            .iter()
            .any(|f| f == "scripts/record-agent-fixture.py")
    );
    assert!(
        !files[1].iter().any(|f| f == "claude-plan-auto.jsonl"),
        "a file only Task 2's steps name isn't the task's: {:?}",
        files[1]
    );
    assert!(files[8].is_empty(), "{:?}", files[8]);
    // A list under `**Files:**` counts until the steps start.
    let md = "### Task 1: A\n**Files:**\n- Modify: `src/retry.rs`\n* Create: `src/backoff.rs`\n\n\
              **Interfaces:** `src/other.rs`\n- [ ] Run `scripts/check.sh`, edit `src/step.rs`\n";
    assert_eq!(task_files(md), [vec!["src/retry.rs", "src/backoff.rs"]]);
}

/// A task with no files ticks after the previous task's commit, however that one was marked
/// (here, a commit naming "Task 1"); a first task with no files has no previous one to follow.
#[test]
fn a_task_with_no_files_follows_the_previous_tasks_commit() {
    let md = "### Task 1: A\n- [ ] x\n### Task 2: Docs\n- [ ] Run `scripts/prune-target.sh`\n";
    let named = commit("aaa1111", "Task 1: A", &["src/retry.rs"]);
    let later = commit("bbb2222", "docs: the plan built", &["docs/notes.md"]);
    let mut tasks = from_checklist(md);
    mark_done(&mut tasks, md, &[later.clone(), named.clone()]);
    assert_eq!(counts(&tasks), (2, 2), "{tasks:?}");
    assert_eq!(commit_of(&tasks[1]), Some("bbb2222"));
    // Without a commit after Task 1's, Task 2 is being built.
    let mut tasks = from_checklist(md);
    mark_done(&mut tasks, md, std::slice::from_ref(&named));
    assert_eq!(tasks[1].state, TaskState::Building);
    // Nothing done before it: a files-less first task isn't ticked by any commit.
    let mut tasks = from_checklist(md);
    mark_done(&mut tasks, md, &[later]);
    assert_eq!(counts(&tasks), (0, 2));
}

/// The review of chore 32: a commit that touches a file the plan names (a fix to an earlier
/// task) doesn't tick a task naming no files; the next one touching none of them does.
#[test]
fn a_fix_to_an_earlier_task_doesnt_tick_the_closing_one() {
    let md = "### Task 1: A\n**Files:** Modify `src/retry.rs`\n### Task 2: Docs\n- [ ] Run `scripts/prune-target.sh`\n";
    let built = commit("aaa1111", "Retry with backoff", &["src/retry.rs"]);
    let fix = commit("bbb2222", "Retry: review fix", &["src/retry.rs"]);
    let docs = commit("ccc3333", "docs: retry built", &["docs/notes.md"]);
    let mut tasks = from_checklist(md);
    mark_done(&mut tasks, md, &[fix.clone(), built.clone()]);
    assert_eq!(tasks[1].state, TaskState::Building, "{tasks:?}");
    let mut tasks = from_checklist(md);
    mark_done(&mut tasks, md, &[docs, fix, built]);
    assert_eq!(commit_of(&tasks[1]), Some("ccc3333"));
}

/// The plan text with every box of tasks 1..=`upto` ticked.
fn ticked(md: &str, upto: usize) -> String {
    let mut task = 0;
    md.lines()
        .map(|line| {
            if line.starts_with("### Task ") {
                task += 1;
            }
            if task <= upto {
                line.replacen("- [ ]", "- [x]", 1)
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The review of chore 32: builds tick a task's boxes with its commit, and a task ticked by its
/// boxes still moves the floor, so an earlier task's commit (Task 2's e12608d touches both of
/// Task 7's files) doesn't tick a later task before it is built.
#[test]
fn a_plan_ticked_as_it_is_built_reads_one_task_at_a_time() {
    let all = chores_25_30_commits();
    let builds = [
        "8b1e0ca", "e12608d", "35000d5", "4ccfb3d", "9ef8d2a", "a01d116", "af083d6", "10f87f2",
        "991d32a",
    ];
    for landed in 1..=all.len() {
        let built = builds
            .iter()
            .filter(|h| all[..landed].iter().any(|c| c.hash == **h))
            .count();
        let md = ticked(CHORES_25_30_PLAN, built);
        let mut newest_first = all[..landed].to_vec();
        newest_first.reverse();
        let mut tasks = from_checklist(&md);
        mark_done(&mut tasks, &md, &newest_first);
        let done: Vec<usize> = (0..tasks.len())
            .filter(|&i| matches!(tasks[i].state, TaskState::Done { .. }))
            .collect();
        assert_eq!(
            done,
            (0..built).collect::<Vec<_>>(),
            "after {}",
            all[landed - 1].hash
        );
    }
}
