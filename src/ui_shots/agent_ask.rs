//! Batch 3, chore 8's scenes: the agent's question card (Claude's AskUserQuestion). No agent
//! runs: the tabs read the synthetic fixture `claude-ask.jsonl` (marked so until a recording
//! exists) through their own translator (`shot_process`), so what an answer writes is what Claude
//! would get. Keys wait out the card's 250 ms guard first (a real sleep: it is an `Instant`).

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui::{AnyWindowHandle, Entity, ListOffset, Modifiers, VisualTestAppContext, px, size};
use serde_json::{Value, json};

use super::agent::{Slot, build_with, feed, scene_sized, start, tab_of};
use super::chrome::settle_frames;
use super::{Kind, Scene, Themes, render_with, shoot_with, shot_theme, shot_themes};
use crate::agent_conversation::{Item, Reply};
use crate::agent_process::ProcessEvent;
use crate::agent_tab::AgentTab;
use crate::providers::agy::AGY;
use crate::providers::claude::CLAUDE;
use crate::providers::{Event, Past, Provider, QAnswer, Start};
use crate::status::Status;

pub(super) const FILE: &str = file!();

const SOURCES: &[&str] = &[
    "src/agent_ask.rs",
    "src/agent_ask_view.rs",
    "src/agent_tab.rs",
    "src/agent_rows.rs",
    "src/agent_box.rs",
    "src/agent_chat.rs",
    "src/agent_conversation.rs",
    "src/composer.rs",
    "src/icons.rs",
    "src/providers/mod.rs",
    "src/providers/claude.rs",
];

const fn visual(name: &'static str, run: fn()) -> Scene {
    Scene {
        name,
        run,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: SOURCES,
    }
}

/// agy's questions, answered by a message, and its refused mode (2026-11-24).
const AGY_SOURCES: &[&str] = &[
    "src/agent_ask.rs",
    "src/agent_ask_view.rs",
    "src/agent_tab.rs",
    "src/agent_box.rs",
    "src/agent_chat.rs",
    "src/agent_conversation.rs",
    "src/mode_memory.rs",
    "src/providers/mod.rs",
    "src/providers/agy.rs",
];

const fn visual_agy(name: &'static str, run: fn()) -> Scene {
    Scene {
        name,
        run,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: AGY_SOURCES,
    }
}

const fn state(name: &'static str, run: fn()) -> Scene {
    Scene {
        name,
        run,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: SOURCES,
    }
}

pub(super) const SCENES: &[Scene] = &[
    visual("agent-tab-question", agent_tab_question),
    visual("agent-tab-question-multi", agent_tab_question_multi),
    visual("agent-tab-question-preview", agent_tab_question_preview),
    visual("agent-tab-question-review", agent_tab_question_review),
    visual("agent-tab-question-answered", agent_tab_question_answered),
    visual("agent-tab-question-other", agent_tab_question_other_look),
    state("agent-tab-question-keys", agent_tab_question_other),
    state("agent-tab-question-skip", agent_tab_question_skip),
    state(
        "agent-tab-question-typing-guard",
        agent_tab_question_typing_guard,
    ),
    visual("agent-tab-question-lane", agent_tab_question_lane),
    visual("agent-tab-question-restored", agent_tab_question_restored),
    visual("agent-tab-question-dock", agent_tab_question_dock),
    visual("agent-tab-question-lane-main", agent_tab_question_lane_main),
    visual("agent-tab-question-queued", agent_tab_question_queued),
    visual("agent-tab-question-narrow", agent_tab_question_narrow),
    visual("agent-tab-question-anyway", agent_tab_question_anyway),
    visual("agent-tab-question-armed", agent_tab_question_armed),
    visual("agent-tab-question-dock-tiny", agent_tab_question_dock_tiny),
    visual(
        "agent-tab-question-answered-line",
        agent_tab_question_answered_line,
    ),
    state(
        "agent-tab-question-head-clear",
        agent_tab_question_head_clear,
    ),
    visual_agy("agent-tab-question-agy", agent_tab_question_agy),
    visual_agy("agent-tab-agy-mode-refused", agent_tab_agy_mode_refused),
    Scene {
        name: "agent-question-agy-anywhere",
        run: agent_question_agy_anywhere,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: &[
            "src/agent_ask_view.rs",
            "src/agent_tab.rs",
            "src/mode_memory.rs",
            "src/providers/agy.rs",
            "src/workspace.rs",
            "src/workspace_agent.rs",
        ],
    },
    Scene {
        name: "agent-question-anywhere",
        run: agent_question_anywhere,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: &[
            "src/agent_ask_view.rs",
            "src/agent_ask.rs",
            "src/workspace.rs",
            "src/workspace_agent.rs",
            "src/tab_bar.rs",
            "src/palette_items.rs",
            "src/main.rs",
        ],
    },
];

const ASK: &str = include_str!("../providers/fixtures/claude-ask.jsonl");
/// The fixture's first question, as `question()` says it (and how many more).
const ASKS: &str = "Claude asks: Which test runner should I set up? (+2 more)";
const DIMS: (f32, f32) = (720.0, 700.0);

/// The fixture's lines up to its question (the `can_use_tool` request).
fn lines_to_question() -> Vec<String> {
    let lines: Vec<String> = ASK.lines().map(String::from).collect();
    let at = lines
        .iter()
        .position(|l| l.contains("\"can_use_tool\""))
        .expect("the request");
    lines[..=at].to_vec()
}

/// `lines` read as the tab's process would bring them (its own translator decodes them).
/// The window drawn as the key window (the card's light shows only then: the design review).
fn hear(cx: &mut VisualTestAppContext, tab: &Entity<AgentTab>, lines: Vec<String>) {
    be_key(cx, tab);
    for line in lines {
        feed(cx, tab, |t, cx| {
            t.shot_process(ProcessEvent::Line(line), cx)
        });
    }
}

/// The tab's box drawn as in the key window, or not (the card's light follows it).
fn be_key(cx: &mut VisualTestAppContext, tab: &Entity<AgentTab>) {
    let composer = cx.read(|cx| tab.read(cx).shot_composer());
    cx.update(|cx| composer.update(cx, |c, _| c.shot_be_key()));
}

fn not_key(cx: &mut VisualTestAppContext, tab: &Entity<AgentTab>) {
    let composer = cx.read(|cx| tab.read(cx).shot_composer());
    cx.update(|cx| composer.update(cx, |c, _| c.shot_not_key()));
}

/// A message sent, and the fixture up to its question.
fn to_question(cx: &mut VisualTestAppContext, tab: &Entity<AgentTab>) {
    feed(cx, tab, |t, cx| {
        t.send("Set up tests for the app".into(), cx)
    });
    hear(cx, tab, lines_to_question());
}

/// The card's guard waited out (an `Instant`, so a real sleep).
fn past_guard() {
    std::thread::sleep(crate::agent_ask::GUARD + Duration::from_millis(40));
}

/// `keys` typed with the card drawn (its key context is set as it draws).
fn keys(cx: &mut VisualTestAppContext, window: AnyWindowHandle, keys: &str) {
    settle_frames(cx, window);
    cx.simulate_keystrokes(window, keys);
    settle_frames(cx, window);
}

fn ask_of(cx: &VisualTestAppContext, tab: &Entity<AgentTab>) -> crate::agent_tab::ShotAsk {
    cx.read(|cx| tab.read(cx).shot_ask(cx))
        .expect("a card is open")
}

/// The answer lines the tab wrote, parsed.
fn written(cx: &VisualTestAppContext, tab: &Entity<AgentTab>) -> Vec<Value> {
    cx.read(|cx| tab.read(cx).shot_written())
        .iter()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .filter(|v| v["type"] == "control_response")
        .collect()
}

fn reply_of(cx: &VisualTestAppContext, tab: &Entity<AgentTab>) -> Option<Reply> {
    cx.read(|cx| {
        tab.read(cx).shot_items().iter().find_map(|i| match i {
            Item::Question { reply, .. } => Some(reply.clone()),
            _ => None,
        })
    })?
}

/// A scene in one theme with no shot (state checks only).
fn once(
    n: &'static str,
    act: impl FnOnce(&mut VisualTestAppContext, AnyWindowHandle, &Entity<AgentTab>) + 'static,
) {
    once_sized(n, DIMS, act);
}

/// `once` in a window `dims` big.
fn once_sized(
    n: &'static str,
    dims: (f32, f32),
    act: impl FnOnce(&mut VisualTestAppContext, AnyWindowHandle, &Entity<AgentTab>) + 'static,
) {
    let theme = shot_theme("Catppuccin Mocha");
    let slot: Slot = Rc::default();
    let keep = slot.clone();
    render_with(
        &theme,
        size(px(dims.0), px(dims.1)),
        build_with(&CLAUDE, theme, slot, start()),
        move |cx, window| {
            let tab = tab_of(&keep);
            act(cx, window, &tab);
            keep.borrow_mut().take();
        },
    );
    println!("ui-shots: {n}");
}

/// The card as it arrives: Claude waits on you, the box is its Other, no tool row and no
/// permission card; the keys are the card's.
pub(super) fn agent_tab_question() {
    scene_sized("agent-tab-question", DIMS, |n, cx, window, tab| {
        to_question(cx, tab);
        settle_frames(cx, window);
        cx.read(|cx| {
            let t = tab.read(cx);
            assert_eq!(t.status(), Status::Waiting, "{n}");
            assert_eq!(
                t.question().as_deref(),
                Some(ASKS),
                "{n}: question_reads_claude_asks"
            );
            assert_eq!(
                t.shot_composer().read(cx).placeholder().as_ref(),
                "Other answer for Test runner\u{2026}",
                "{n}: placeholder_when_a_question_waits"
            );
            assert_eq!(
                t.shot_box(cx).send,
                crate::agent_box::ANSWER_GLYPH,
                "{n}: the button says Answer"
            );
            let items = t.shot_items();
            assert!(
                !items.iter().any(|i| matches!(i, Item::Permission { .. })),
                "{n}: no permission card"
            );
            assert!(
                !items
                    .iter()
                    .any(|i| matches!(i, Item::Tool { name, .. } if name == "AskUserQuestion")),
                "{n}: no tool row"
            );
            assert!(t.shot_part("ask-card").is_some(), "{n}: the card is drawn");
        });
        let ask = ask_of(cx, tab);
        assert_eq!((ask.step, ask.cursor), (0, 0), "{n}: on option 1");
        assert!(
            ask.focus && ask.keys_on && !ask.docked,
            "{n}: the card has the keys, in sight"
        );
    });
}

/// The second question (multi-select): two checked, its checkboxes and the stepper.
pub(super) fn agent_tab_question_multi() {
    scene_sized("agent-tab-question-multi", DIMS, |n, cx, window, tab| {
        to_question(cx, tab);
        past_guard();
        keys(cx, window, "1");
        assert_eq!(ask_of(cx, tab).step, 1, "{n}: 1 picked Vitest and moved on");
        keys(cx, window, "space down down space");
        let ask = ask_of(cx, tab);
        assert_eq!(ask.picks[0], [0], "{n}");
        assert_eq!(ask.picks[1], [0, 2], "{n}: Lint and Unit tests checked");
        assert_eq!(ask.cursor, 2, "{n}");
        assert!(written(cx, tab).is_empty(), "{n}: nothing sent yet");
    });
}

/// A request for one single-select question with previews, its second option recommended.
fn preview_request() -> String {
    json!({"type": "control_request", "request_id": "req-preview", "request": {
        "subtype": "can_use_tool", "tool_name": "AskUserQuestion", "tool_use_id": "toolu_preview",
        "input": {"questions": [{"question": "Which layout for the settings page?",
            "header": "Layout", "multiSelect": false, "options": [
            {"label": "Tabs", "description": "One tab per section",
             "preview": "┌ General │ Editor │ Keys ┐\n│ Font size     [ 13 ]   │\n│ Theme         [Mocha]  │\n└────────────────────────┘"},
            {"label": "Sidebar (Recommended)", "description": "Sections listed on the left",
             "preview": "┌──────────┬─────────────┐\n│ General  │ Font size 13│\n│ Editor   │ Theme Mocha │\n│ Keys     │             │\n│ Agents   │             │\n└──────────┴─────────────┘"}]}]}}})
    .to_string()
}

/// The user's UX bar: "(Recommended)" as a tag with the cursor on it, its preview in the well;
/// Enter sends the label as Claude wrote it, and the preview as an annotation.
pub(super) fn agent_tab_question_preview() {
    scene_sized("agent-tab-question-preview", DIMS, |n, cx, window, tab| {
        feed(cx, tab, |t, cx| {
            t.send("Sketch the settings page".into(), cx)
        });
        hear(cx, tab, vec![preview_request()]);
        settle_frames(cx, window);
        let ask = ask_of(cx, tab);
        assert_eq!(ask.cursor, 1, "{n}: the recommended option has the cursor");
        assert_eq!(
            cx.read(|cx| tab.read(cx).question()).as_deref(),
            Some("Claude asks: Which layout for the settings page?"),
            "{n}"
        );
    });
    once("agent-tab-question-preview: Enter", |cx, window, tab| {
        let n = "agent-tab-question-preview";
        feed(cx, tab, |t, cx| {
            t.send("Sketch the settings page".into(), cx)
        });
        hear(cx, tab, vec![preview_request()]);
        past_guard();
        keys(cx, window, "enter");
        let out = written(cx, tab);
        assert_eq!(out.len(), 1, "{n}: one answer");
        let input = &out[0]["response"]["response"]["updatedInput"];
        assert_eq!(
            input["answers"]["Which layout for the settings page?"], "Sidebar (Recommended)",
            "{n}: the label goes back unchanged"
        );
        assert!(
            input["annotations"]["Which layout for the settings page?"]["preview"]
                .as_str()
                .is_some_and(|p| p.contains("General  │")),
            "{n}: the chosen preview as an annotation"
        );
        assert!(matches!(reply_of(cx, tab), Some(Reply::Answered(_))), "{n}");
    });
}

/// The user's UX bar: after the last question, a review of every answer; a row goes back to
/// change it.
pub(super) fn agent_tab_question_review() {
    scene_sized("agent-tab-question-review", DIMS, |n, cx, window, tab| {
        to_question(cx, tab);
        past_guard();
        // Vitest; Lint and Type check; Next; Coverage: No; Next to the review.
        keys(cx, window, "1 1 2 enter 2 enter");
        let ask = ask_of(cx, tab);
        assert!(ask.review, "{n}: the review: {ask:?}");
        assert_eq!(ask.picks, vec![vec![0], vec![0, 1], vec![1]], "{n}");
        assert_eq!(
            cx.read(|cx| tab
                .read(cx)
                .shot_composer()
                .read(cx)
                .placeholder()
                .to_string()),
            "Send your answers with \u{23ce}",
            "{n}"
        );
        // A review row back to its question; Enter there back to the review.
        feed(cx, tab, |t, cx| t.shot_ask_review_row(1, cx));
        let ask = ask_of(cx, tab);
        assert_eq!(
            (ask.step, ask.cursor, ask.review),
            (1, 0, false),
            "{n}: back on CI checks"
        );
        keys(cx, window, "enter");
        assert!(ask_of(cx, tab).review, "{n}: Enter returns to the review");
        // A step chip, then → to the review.
        feed(cx, tab, |t, cx| t.shot_ask_go_to(2, cx));
        keys(cx, window, "right");
        assert!(ask_of(cx, tab).review, "{n}");
        assert!(
            written(cx, tab).is_empty(),
            "{n}: nothing sent before Enter"
        );
    });
}

/// Answered: Enter on the review sends every answer keyed by its question; the card leaves a
/// summary line, which opens to the full questions and answers.
pub(super) fn agent_tab_question_answered() {
    scene_sized("agent-tab-question-answered", DIMS, |n, cx, window, tab| {
        to_question(cx, tab);
        past_guard();
        keys(cx, window, "1 1 3 enter 2 enter enter");
        let out = written(cx, tab);
        assert_eq!(out.len(), 1, "{n}: one answer");
        assert_eq!(out[0]["response"]["request_id"], "req-ask-1", "{n}");
        assert_eq!(
            out[0]["response"]["response"]["updatedInput"]["answers"],
            json!({
                "Which test runner should I set up?": "Vitest",
                "Which checks should run in CI?": "Lint, Unit tests",
                "Should I add a coverage report?": "No",
            }),
            "{n}"
        );
        cx.read(|cx| {
            let t = tab.read(cx);
            assert_eq!(t.status(), Status::Working, "{n}: the turn goes on");
            assert_eq!(t.question(), None, "{n}");
            assert!(t.shot_ask(cx).is_none(), "{n}: no card");
            assert_ne!(t.shot_box(cx).send, crate::agent_box::ANSWER_GLYPH, "{n}");
        });
        // The rest of the turn: its result closes nothing more, the reply stays.
        let rest: Vec<String> = ASK
            .lines()
            .skip(lines_to_question().len())
            .map(String::from)
            .collect();
        hear(cx, tab, rest);
        assert!(
            matches!(reply_of(cx, tab), Some(Reply::Answered(a)) if a.len() == 3),
            "{n}: what you chose is kept"
        );
        // The summary opens on a click.
        settle_frames(cx, window);
        let at = cx
            .read(|cx| tab.read(cx).shot_part("ask-summary"))
            .expect("the summary line");
        cx.simulate_click(window, at.center(), Modifiers::none());
        settle_frames(cx, window);
        assert!(
            cx.read(|cx| tab.read(cx).shot_summary_open("toolu_ask_1")),
            "{n}: the summary opened"
        );
    });
}

/// Typing goes to the box (its Other): the card lets go of the keys, Enter answers the question
/// shown with your words and moves on — it is never sent as a message.
/// Other mode (the designer's state F): the box's text mirrored in the Other row, its mark
/// filled and lit, the send circle a check.
pub(super) fn agent_tab_question_other_look() {
    scene_sized("agent-tab-question-other", DIMS, |n, cx, window, tab| {
        to_question(cx, tab);
        settle_frames(cx, window);
        cx.simulate_input(window, "Node's own test runner");
        settle_frames(cx, window);
        let ask = ask_of(cx, tab);
        assert!(!ask.keys_on, "{n}: the box has the keys");
        assert_eq!(
            cx.read(|cx| tab.read(cx).shot_composer().read(cx).text().to_string()),
            "Node's own test runner",
            "{n}: typed in the box, not picked"
        );
    });
}

pub(super) fn agent_tab_question_other() {
    once("agent-tab-question-keys", |cx, window, tab| {
        let n = "agent-tab-question-keys";
        to_question(cx, tab);
        past_guard();
        settle_frames(cx, window);
        cx.simulate_input(window, "Node's own runner");
        settle_frames(cx, window);
        let ask = ask_of(cx, tab);
        assert!(!ask.focus && !ask.keys_on, "{n}: the box has the keys");
        keys(cx, window, "enter");
        let ask = ask_of(cx, tab);
        assert_eq!(ask.step, 1, "{n}: box_enter_answers_the_question_not_sends");
        cx.read(|cx| {
            let t = tab.read(cx);
            assert_eq!(
                t.shot_composer().read(cx).text(),
                "",
                "{n}: the box is cleared"
            );
            let yours = t
                .shot_items()
                .iter()
                .filter(|i| matches!(i, Item::You { .. }))
                .count();
            assert_eq!(yours, 1, "{n}: nothing sent as a message");
            assert_eq!(
                t.shot_composer().read(cx).placeholder().as_ref(),
                "Other answer for CI checks\u{2026}",
                "{n}"
            );
        });
        // Tab gives the box the keys; ↑ in the empty box takes them back to the card.
        assert!(
            ask_of(cx, tab).keys_on,
            "{n}: the box is empty again: the card's keys"
        );
        keys(cx, window, "tab");
        assert!(!ask_of(cx, tab).keys_on, "{n}: Tab: the box");
        keys(cx, window, "up");
        assert!(ask_of(cx, tab).keys_on, "{n}: ↑: the card");
        keys(cx, window, "tab");
        keys(cx, window, "escape");
        assert!(
            ask_of(cx, tab).keys_on,
            "{n}: Esc in the box: the card, not a stop"
        );
        assert_eq!(
            cx.read(|cx| tab.read(cx).shot_stop_label()),
            None,
            "{n}: nothing stopping"
        );
        // Finish: the first answer is your words.
        keys(cx, window, "1 enter enter enter");
        let out = written(cx, tab);
        assert_eq!(
            out[0]["response"]["response"]["updatedInput"]["answers"]["Which test runner should I set up?"],
            "Node's own runner",
            "{n}"
        );
    });
}

/// Esc on the card skips: a deny with the skip message, a "Skipped" line.
pub(super) fn agent_tab_question_skip() {
    once("agent-tab-question-skip", |cx, window, tab| {
        let n = "agent-tab-question-skip";
        to_question(cx, tab);
        // A second question while the first waits: drawn as queued, the first keeps the box.
        let mut second: Value = serde_json::from_str(lines_to_question().last().unwrap()).unwrap();
        second["request_id"] = json!("req-ask-2");
        second["request"]["tool_use_id"] = json!("toolu_ask_2");
        hear(cx, tab, vec![second.to_string()]);
        settle_frames(cx, window);
        assert_eq!(
            cx.read(|cx| tab.read(cx).shot_box(cx).send.clone()),
            crate::agent_box::ANSWER_GLYPH,
            "{n}"
        );
        past_guard();
        // The designer: Esc arms Skip ("Esc again to skip"); a second Esc skips.
        keys(cx, window, "escape");
        let ask = ask_of(cx, tab);
        assert!(ask.armed, "{n}: armed");
        assert!(written(cx, tab).is_empty(), "{n}: not skipped by one Esc");
        keys(cx, window, "escape");
        let out = written(cx, tab);
        assert_eq!(
            out.first().map(|v| v["response"]["response"].clone()),
            Some(json!({"behavior": "deny", "message": "The user skipped these questions."})),
            "{n}"
        );
        assert_eq!(reply_of(cx, tab), Some(Reply::Skipped), "{n}");
        // The queued one is the card now, fresh (its own guard).
        let ask = ask_of(cx, tab);
        assert_eq!(
            (ask.step, ask.armed),
            (0, false),
            "{n}: the second question's card"
        );
        assert_eq!(
            cx.read(|cx| tab.read(cx).status()),
            Status::Waiting,
            "{n}: still waits"
        );
        past_guard();
        keys(cx, window, "escape escape");
        assert_eq!(cx.read(|cx| tab.read(cx).status()), Status::Working, "{n}");
    });
}

/// Keys in the card's first 250 ms are ignored: what you were typing doesn't answer it.
pub(super) fn agent_tab_question_typing_guard() {
    once("agent-tab-question-typing-guard", |cx, window, tab| {
        let n = "agent-tab-question-typing-guard";
        to_question(cx, tab);
        keys(cx, window, "2 enter escape");
        let ask = ask_of(cx, tab);
        assert_eq!((ask.step, ask.cursor), (0, 0), "{n}: nothing moved");
        assert!(ask.picks.iter().all(Vec::is_empty), "{n}");
        assert!(written(cx, tab).is_empty(), "{n}: nothing sent or skipped");
        past_guard();
        keys(cx, window, "2");
        assert_eq!(
            ask_of(cx, tab).picks[0],
            [1],
            "{n}: after the guard, keys work"
        );
    });
}

/// A subagent's question: its lane waits, and the card waits at the end of the lane's view.
pub(super) fn agent_tab_question_lane() {
    scene_sized("agent-tab-question-lane", DIMS, |n, cx, window, tab| {
        feed(cx, tab, |t, cx| t.send("Review the tests".into(), cx));
        let started = json!({"type": "system", "subtype": "task_started", "task_id": "task1",
            "tool_use_id": "toolu_lane", "task_type": "local_agent", "subagent_type": "Explore",
            "description": "Read the test setup"})
        .to_string();
        let mut ask: Value = serde_json::from_str(lines_to_question().last().unwrap()).unwrap();
        ask["request"]["agent_id"] = json!("task1");
        hear(cx, tab, vec![started, ask.to_string()]);
        cx.read(|cx| {
            let t = tab.read(cx);
            let lane = t
                .shot_lanes()
                .iter()
                .find(|l| l.id == "toolu_lane")
                .expect("the lane");
            assert_eq!(
                lane.status,
                crate::agent_conversation::LaneStatus::Waiting,
                "{n}"
            );
            assert_eq!(t.status(), Status::Waiting, "{n}");
        });
        feed(cx, tab, |t, cx| {
            t.show_view(crate::agent_lanes::View::Lane("toolu_lane".into()), cx)
        });
        settle_frames(cx, window);
        assert!(
            cx.read(|cx| tab.read(cx).shot_part("ask-card").is_some()),
            "{n}: in the lane's view"
        );
        assert!(ask_of(cx, tab).keys_on, "{n}: its keys work there");
    });
}

/// A restored conversation (the design review): a past question replays with what you chose
/// (read back from its result), a skipped one as skipped, one with no result as not answered;
/// none is left open. Read through Claude's own transcript reader.
pub(super) fn agent_tab_question_restored() {
    scene_sized("agent-tab-question-restored", DIMS, |n, cx, window, tab| {
        let input: Value = serde_json::from_str::<Value>(ASK.lines().nth(2).unwrap()).unwrap()
            ["message"]["content"][0]["input"]
            .clone();
        let coverage = json!({"questions": [input["questions"][2].clone()]});
        let you = |uuid: &str, text: &str| json!({"type": "user", "uuid": uuid, "message": {"role": "user", "content": text}});
        let asked = |uuid: &str, id: &str, input: &Value| {
            json!({"type": "assistant", "uuid": uuid, "message": {"id": uuid,
                "role": "assistant", "content": [{"type": "tool_use", "id": id,
                "name": "AskUserQuestion", "input": input}]}})
        };
        let result = |uuid: &str, id: &str, text: &str, error: bool| {
            json!({"type": "user", "uuid": uuid, "message": {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": id, "content": text, "is_error": error}]}})
        };
        let said = |uuid: &str, text: &str| {
            json!({"type": "assistant", "uuid": uuid, "message": {"id": uuid,
                "role": "assistant", "content": [{"type": "text", "text": text}]}})
        };
        let lines = [
            you("u1", "Set up tests for the app"),
            asked("a1", "toolu_old", &input),
            result(
                "u2",
                "toolu_old",
                "User has answered your questions: \"Which test runner should I set up?\"=\"Vitest\", \"Which checks should run in CI?\"=\"Lint, Unit tests\", \"Should I add a coverage report?\"=\"No\". You can now continue with the user's answers in mind.",
                false,
            ),
            said("a2", "Set up Vitest with lint and unit tests in CI."),
            you("u3", "Add a coverage report too"),
            asked("a3", "toolu_skip", &coverage),
            result(
                "u4",
                "toolu_skip",
                "The user skipped these questions.",
                true,
            ),
            you("u5", "Then just the badge"),
            asked("a4", "toolu_left", &coverage),
        ];
        let text: String = lines.iter().map(|l| format!("{l}\n")).collect();
        let past = crate::providers::claude::transcript_events(&text);
        feed(cx, tab, |t, cx| t.shot_history(past, cx));
        settle_frames(cx, window);
        cx.read(|cx| {
            let t = tab.read(cx);
            assert!(t.shot_ask(cx).is_none(), "{n}: nothing open");
            assert_ne!(t.status(), Status::Waiting, "{n}");
            let replies: Vec<Option<Reply>> = t
                .shot_items()
                .iter()
                .filter_map(|i| match i {
                    Item::Question { reply, .. } => Some(reply.clone()),
                    _ => None,
                })
                .collect();
            let picked = |labels: &[&str]| crate::providers::QAnswer {
                labels: labels.iter().map(|l| l.to_string()).collect(),
                other: None,
            };
            assert_eq!(
                replies,
                [
                    Some(Reply::Answered(vec![
                        picked(&["Vitest"]),
                        picked(&["Lint", "Unit tests"]),
                        picked(&["No"]),
                    ])),
                    Some(Reply::Skipped),
                    Some(Reply::Expired),
                ],
                "{n}: what you chose, skipped, not answered"
            );
        });
    });
}

/// A long conversation about it (the card's row scrolled away and back).
fn long_past() -> Vec<Past> {
    (0..8)
        .flat_map(|i| {
            [
                Past::you(format!("Step {i}: look at the next part of the app")),
                Past::Event(Event::MessageDone {
                    message: format!("m{i}"),
                    text: "A long answer about that part of the app.\n\n".repeat(6),
                    parent: None,
                }),
            ]
        })
        .collect()
}

/// The user's UX bar: scrolled away from its row, the card docks over the box (the same card,
/// the same keys); back in sight, the docked copy goes.
pub(super) fn agent_tab_question_dock() {
    scene_sized("agent-tab-question-dock", DIMS, |n, cx, window, tab| {
        feed(cx, tab, |t, cx| t.shot_history(long_past(), cx));
        to_question(cx, tab);
        settle_frames(cx, window);
        assert!(
            !ask_of(cx, tab).docked,
            "{n}: in sight at the end, not docked"
        );
        assert!(
            cx.read(|cx| tab.read(cx).shot_part("ask-dock").is_none()),
            "{n}"
        );
        // Scrolled to the top: its row is out of sight.
        cx.read(|cx| {
            tab.read(cx).shot_list().scroll_to(ListOffset {
                item_ix: 0,
                offset_in_item: px(0.),
            })
        });
        cx.update_window(window, |_, window, _| window.refresh())
            .unwrap();
        settle_frames(cx, window);
        let ask = ask_of(cx, tab);
        assert!(
            ask.docked && ask.keys_on,
            "{n}: docked, its keys on: {ask:?}"
        );
        assert!(
            cx.read(|cx| tab.read(cx).shot_part("ask-dock").is_some()),
            "{n}: drawn"
        );
        // The same state: a key on the docked card moves the one card.
        past_guard();
        keys(cx, window, "down");
        assert_eq!(ask_of(cx, tab).cursor, 1, "{n}");
        // Back to the end: in sight again, the dock goes.
        cx.read(|cx| tab.read(cx).shot_list().scroll_to_end());
        cx.update_window(window, |_, window, _| window.refresh())
            .unwrap();
        settle_frames(cx, window);
        assert!(!ask_of(cx, tab).docked, "{n}: back in sight, undocked");
        // And away once more for the shot.
        cx.read(|cx| {
            tab.read(cx).shot_list().scroll_to(ListOffset {
                item_ix: 0,
                offset_in_item: px(0.),
            })
        });
        cx.update_window(window, |_, window, _| window.refresh())
            .unwrap();
        settle_frames(cx, window);
        assert!(ask_of(cx, tab).docked, "{n}");
    });
}

/// From anywhere: two workspaces' Claudes ask; "Answer Waiting Question" (⌘⇧A) shows the one
/// that asked first with the keys on its card, and so does a click on a banner about nothing;
/// an agent's banner names its tab, and its click goes there. The banner says "Claude asks: …".
pub(super) fn agent_question_anywhere() {
    use super::agent::agent_workspace;
    let n = "agent-question-anywhere";
    let theme = shot_theme("Catppuccin Mocha");
    let slot: Rc<RefCell<Option<Entity<crate::workspace::Workspace>>>> = Rc::default();
    let keep = slot.clone();
    let build = agent_workspace(theme, slot, |ws, window, cx| {
        let my_app = std::path::Path::new("/Users/me/code/my-app");
        let a = ws.open_unnamed_shot_session(Some(my_app), super::SHELL_TEXT, window, cx);
        ws.agent_beside(a, "claude", window, cx);
        let cli = std::path::Path::new("/Users/me/fuxx-cli/src");
        let b = ws.open_unnamed_shot_session(Some(cli), super::SHELL_TEXT, window, cx);
        ws.agent_beside(b, "claude", window, cx);
    });
    render_with(
        &theme,
        size(px(1100.), px(700.)),
        build,
        move |cx, window| {
            let ws = keep.borrow_mut().take().expect("the workspace");
            settle_frames(cx, window);
            let tabs: Vec<(crate::layout_model::TabId, Entity<AgentTab>)> = cx.read(|cx| {
                let ws = ws.read(cx);
                let mut tabs: Vec<_> = ws
                    .agents
                    .iter()
                    .map(|(t, s)| (*t, s.view.clone()))
                    .collect();
                tabs.sort_by_key(|(_, v)| v.entity_id());
                tabs
            });
            assert_eq!(tabs.len(), 2, "{n}: two agent tabs");
            let focused = cx.read(|cx| ws.read(cx).focused_agent_tab().map(|(t, _)| t));
            // The one not in front asks first, then the one in front.
            let (first, second) = if Some(tabs[0].0) == focused {
                (tabs[1].clone(), tabs[0].clone())
            } else {
                (tabs[0].clone(), tabs[1].clone())
            };
            for (_, view) in [&first, &second] {
                to_question(cx, view);
                std::thread::sleep(Duration::from_millis(5));
            }
            settle_frames(cx, window);
            let banners = cx.read(|cx| ws.read(cx).shot_banners.clone());
            assert!(
                banners.iter().any(|(_, _, body)| body == ASKS),
                "{n}: the banner says Claude asks: {banners:?}"
            );
            // ⌘⇧A: the first to ask, in front, the keys on its card.
            cx.simulate_keystrokes(window, "cmd-shift-a");
            settle_frames(cx, window);
            let now = cx.read(|cx| ws.read(cx).focused_agent_tab().map(|(t, _)| t));
            assert_eq!(
                now,
                Some(first.0),
                "{n}: the oldest question's tab is shown"
            );
            assert!(ask_of(cx, &first.1).keys_on, "{n}: the keys on its card");
            // A click on an agent's banner (no terminal): the same.
            let other = second.0;
            cx.update_window(window, |_, window, cx| {
                ws.update(cx, |ws, cx| ws.select_tab(other, window, cx))
            })
            .unwrap();
            settle_frames(cx, window);
            cx.update_window(window, |_, window, cx| {
                ws.update(cx, |ws, cx| ws.shot_banner_clicked(None, window, cx))
            })
            .unwrap();
            settle_frames(cx, window);
            let now = cx.read(|cx| ws.read(cx).focused_agent_tab().map(|(t, _)| t));
            assert_eq!(
                now,
                Some(first.0),
                "{n}: a banner click lands on the oldest question"
            );
            // A banner naming its tab (the opens fixed, 2026-11-23): that tab, the keys on its
            // card, though another asked first; one naming a tab gone since: the oldest.
            let named = |id: u64| {
                Some(crate::native_notify::About::Tab {
                    run: crate::native_notify::this_run(),
                    id,
                })
            };
            cx.update_window(window, |_, window, cx| {
                ws.update(cx, |ws, cx| {
                    ws.shot_banner_clicked(named(other.0), window, cx)
                })
            })
            .unwrap();
            settle_frames(cx, window);
            let now = cx.read(|cx| ws.read(cx).focused_agent_tab().map(|(t, _)| t));
            assert_eq!(now, Some(other), "{n}: a banner lands on its own tab");
            assert!(ask_of(cx, &second.1).keys_on, "{n}: the keys on its card");
            cx.update_window(window, |_, window, cx| {
                ws.update(cx, |ws, cx| {
                    ws.shot_banner_clicked(named(u64::MAX), window, cx)
                })
            })
            .unwrap();
            settle_frames(cx, window);
            let now = cx.read(|cx| ws.read(cx).focused_agent_tab().map(|(t, _)| t));
            assert_eq!(now, Some(first.0), "{n}: its tab gone, the oldest question");
            // Answered there, ⌘⇧A goes to the other.
            past_guard();
            cx.update(|cx| first.1.update(cx, |t, cx| t.shot_ask_go_to(3, cx)));
            // Nothing answered: the first Enter lights it ("Send anyway"), the second sends.
            keys(cx, window, "enter");
            assert!(ask_of(cx, &first.1).anyway, "{n}: send anyway");
            keys(cx, window, "enter");
            cx.simulate_keystrokes(window, "cmd-shift-a");
            settle_frames(cx, window);
            let now = cx.read(|cx| ws.read(cx).focused_agent_tab().map(|(t, _)| t));
            assert_eq!(now, Some(second.0), "{n}: then the next one");
        },
    );
    println!("ui-shots: {n}: palette command, ⌘⇧A, banner click");
}

/// A subagent's question seen in Main (the design review): its head names the subagent's type
/// with its task after it, and offers "Open ⟨its task⟩".
pub(super) fn agent_tab_question_lane_main() {
    scene_sized(
        "agent-tab-question-lane-main",
        DIMS,
        |n, cx, window, tab| {
            feed(cx, tab, |t, cx| t.send("Review the tests".into(), cx));
            let started = json!({"type": "system", "subtype": "task_started", "task_id": "task1",
            "tool_use_id": "toolu_lane", "task_type": "local_agent", "subagent_type": "Explore",
            "description": "Read the test setup"})
            .to_string();
            let mut ask: Value = serde_json::from_str(lines_to_question().last().unwrap()).unwrap();
            ask["request"]["agent_id"] = json!("task1");
            hear(cx, tab, vec![started, ask.to_string()]);
            settle_frames(cx, window);
            cx.read(|cx| {
                let t = tab.read(cx);
                assert_eq!(
                    t.question().as_deref(),
                    Some("Explore asks: Which test runner should I set up? (+2 more)"),
                    "{n}: by the subagent's type"
                );
                assert!(t.shot_part("ask-card").is_some(), "{n}: the card in Main");
            });
        },
    );
}

/// Two open asks (the design review): the oldest is the card; a permission after it waits as
/// a quiet "Next" line, not a second card.
pub(super) fn agent_tab_question_queued() {
    scene_sized("agent-tab-question-queued", DIMS, |n, cx, window, tab| {
        to_question(cx, tab);
        let permission = json!({"type": "control_request", "request_id": "req-bash",
            "request": {"subtype": "can_use_tool", "tool_name": "Bash", "tool_use_id": "toolu_bash",
            "input": {"command": "npm test", "description": "Run the tests"}}})
        .to_string();
        hear(cx, tab, vec![permission]);
        settle_frames(cx, window);
        cx.read(|cx| {
            let t = tab.read(cx);
            assert!(t.shot_part("ask-card").is_some(), "{n}: the question first");
            assert!(
                t.shot_part("ask-queued").is_some(),
                "{n}: the permission queued"
            );
        });
    });
}

/// A narrow card (the design review): only the current step chip and "2 of 3", no hint.
pub(super) fn agent_tab_question_narrow() {
    scene_sized(
        "agent-tab-question-narrow",
        (480.0, 700.0),
        |n, cx, window, tab| {
            to_question(cx, tab);
            past_guard();
            keys(cx, window, "1");
            let card = cx
                .read(|cx| tab.read(cx).shot_part("ask-card"))
                .expect("the card");
            assert!(
                f32::from(card.size.width) < 440.0,
                "{n}: narrow: {:?}",
                card.size
            );
            assert_eq!(ask_of(cx, tab).step, 1, "{n}");
        },
    );
}

/// The review with an answer missing (the design review): the first Enter lights it and the
/// primary reads "Send anyway"; nothing is sent yet.
pub(super) fn agent_tab_question_anyway() {
    scene_sized("agent-tab-question-anyway", DIMS, |n, cx, window, tab| {
        to_question(cx, tab);
        past_guard();
        // Vitest; no CI check; Coverage: No; then the review.
        keys(cx, window, "1 right 2");
        feed(cx, tab, |t, cx| t.shot_ask_go_to(3, cx));
        keys(cx, window, "enter");
        let ask = ask_of(cx, tab);
        assert!(ask.review && ask.anyway, "{n}: {ask:?}");
        assert_eq!(ask.cursor, 1, "{n}: the missing one lit");
        assert!(written(cx, tab).is_empty(), "{n}: not sent yet");
    });
}

/// Esc arms Skip (the design review's scene): "Esc again to skip" with the waiting edge.
pub(super) fn agent_tab_question_armed() {
    scene_sized("agent-tab-question-armed", DIMS, |n, cx, window, tab| {
        to_question(cx, tab);
        past_guard();
        keys(cx, window, "escape");
        assert!(ask_of(cx, tab).armed, "{n}");
        assert!(written(cx, tab).is_empty(), "{n}");
    });
}

/// A short chat (the design review's scene): the docked card is its head row only, with
/// "Show in chat".
pub(super) fn agent_tab_question_dock_tiny() {
    scene_sized(
        "agent-tab-question-dock-tiny",
        (720.0, 420.0),
        |n, cx, window, tab| {
            feed(cx, tab, |t, cx| t.shot_history(long_past(), cx));
            to_question(cx, tab);
            settle_frames(cx, window);
            cx.read(|cx| {
                tab.read(cx).shot_list().scroll_to(ListOffset {
                    item_ix: 0,
                    offset_in_item: px(0.),
                })
            });
            cx.update_window(window, |_, window, _| window.refresh())
                .unwrap();
            settle_frames(cx, window);
            let ask = ask_of(cx, tab);
            assert_eq!(
                ask.dock,
                Some(crate::agent_ask::DockKind::Tiny),
                "{n}: {ask:?}"
            );
        },
    );
    // "Show in chat" glides (the opens fixed, 2026-11-23), as the latest-prompt strip's jump
    // does, to the chat's end, which it follows again; with motion reduced, it jumps.
    for still in [false, true] {
        once_sized(
            "agent-tab-question-dock-tiny: show in chat",
            (720.0, 420.0),
            move |cx, window, tab| {
                let n = format!("agent-tab-question-dock-tiny (reduce motion {still})");
                feed(cx, tab, |t, cx| t.shot_history(long_past(), cx));
                to_question(cx, tab);
                settle_frames(cx, window);
                let list = cx.read(|cx| tab.read(cx).shot_list());
                list.scroll_to(ListOffset {
                    item_ix: 0,
                    offset_in_item: px(0.),
                });
                cx.update_window(window, |_, window, _| window.refresh())
                    .unwrap();
                settle_frames(cx, window);
                assert!(ask_of(cx, tab).dock.is_some(), "{n}: docked");
                cx.update(|cx| cx.set_reduce_motion(still));
                cx.update(|cx| tab.update(cx, |t, cx| t.shot_show_in_chat(cx)));
                assert_eq!(
                    cx.read(|cx| tab.read(cx).shot_gliding()),
                    !still,
                    "{n}: a glide, or a jump"
                );
                let until = Instant::now() + Duration::from_secs(5);
                loop {
                    settle_frames(cx, window);
                    if !cx.read(|cx| tab.read(cx).shot_gliding()) {
                        break;
                    }
                    assert!(Instant::now() < until, "{n}: the glide ends");
                }
                settle_frames(cx, window);
                assert!(
                    list.is_following_tail(),
                    "{n}: the chat follows its end again"
                );
                let ask = ask_of(cx, tab);
                assert_eq!(ask.dock, None, "{n}: the card in sight: {ask:?}");
                assert!(ask.keys_on, "{n}: the keys on its card");
                cx.update(|cx| cx.set_reduce_motion(false));
            },
        );
    }
}

/// Answered, the line as it is left live (the design review's scene): "Answered · Test runner:
/// Vitest · …". Opening it keeps the line where it was, though the chat followed its end.
pub(super) fn agent_tab_question_answered_line() {
    scene_sized(
        "agent-tab-question-answered-line",
        DIMS,
        |n, cx, window, tab| {
            to_question(cx, tab);
            past_guard();
            keys(cx, window, "1 1 3 enter 2 enter enter");
            settle_frames(cx, window);
            assert!(
                matches!(reply_of(cx, tab), Some(Reply::Answered(a)) if a.len() == 3),
                "{n}"
            );
            assert!(
                cx.read(|cx| tab.read(cx).shot_part("ask-summary"))
                    .is_some(),
                "{n}: the line"
            );
        },
    );
    once(
        "agent-tab-question-answered-line: open",
        |cx, window, tab| {
            let n = "agent-tab-question-answered-line";
            feed(cx, tab, |t, cx| t.shot_history(long_past(), cx));
            to_question(cx, tab);
            past_guard();
            keys(cx, window, "1 1 3 enter 2 enter enter");
            settle_frames(cx, window);
            assert!(
                cx.read(|cx| tab.read(cx).shot_list().is_following_tail()),
                "{n}: the chat follows its end"
            );
            let before = cx
                .read(|cx| tab.read(cx).shot_part("ask-summary"))
                .expect("the line");
            cx.simulate_click(window, before.center(), Modifiers::none());
            settle_frames(cx, window);
            assert!(
                cx.read(|cx| tab.read(cx).shot_summary_open("toolu_ask_1")),
                "{n}: opened"
            );
            let after = cx
                .read(|cx| tab.read(cx).shot_part("ask-summary"))
                .expect("the line");
            assert!(
                (f32::from(after.top()) - f32::from(before.top())).abs() < 1.0,
                "{n}: the line stays put: {before:?} → {after:?}"
            );
        },
    );
}

/// The design review: the latest-prompt strip never covers a waiting card's head, and the
/// light goes out while the window isn't the key window.
pub(super) fn agent_tab_question_head_clear() {
    // A chat shorter than the card, so its head can sit under the strip.
    once_sized(
        "agent-tab-question-head-clear",
        (720.0, 470.0),
        |cx, window, tab| {
            let n = "agent-tab-question-head-clear";
            feed(cx, tab, |t, cx| t.shot_history(long_past(), cx));
            to_question(cx, tab);
            settle_frames(cx, window);
            // Your message just above the view, the card's head at the top under the strip.
            let (row, list) = cx.read(|cx| {
                let t = tab.read(cx);
                let row = t
                    .shot_items()
                    .iter()
                    .rposition(|i| matches!(i, Item::Question { .. }))
                    .expect("the question");
                (row, t.shot_list())
            });
            list.scroll_to(ListOffset {
                item_ix: row,
                offset_in_item: px(10.),
            });
            cx.update_window(window, |_, window, _| window.refresh())
                .unwrap();
            settle_frames(cx, window);
            let top = cx.read(|cx| tab.read(cx).shot_list().logical_scroll_top());
            assert_eq!(top.item_ix, row, "{n}: the card's row at the top: {top:?}");
            assert_eq!(
                cx.read(|cx| tab.read(cx).shot_latest_strip()),
                None,
                "{n}: no strip over the card's head"
            );
            // A row further down: the card above the view, the strip back.
            list.scroll_to_end();
            cx.update_window(window, |_, window, _| window.refresh())
                .unwrap();
            settle_frames(cx, window);
            assert!(ask_of(cx, tab).lit, "{n}: lit in the key window");
            not_key(cx, tab);
            cx.update_window(window, |_, window, _| window.refresh())
                .unwrap();
            settle_frames(cx, window);
            assert!(
                !ask_of(cx, tab).lit,
                "{n}: no light in a window in the back"
            );
            be_key(cx, tab);
            cx.update_window(window, |_, window, _| window.refresh())
                .unwrap();
            settle_frames(cx, window);
            assert!(ask_of(cx, tab).lit, "{n}: back");
        },
    );
}

// ---- 2026-11-24: agy's skipped questions, answered by a message ----

const AGY_ASK: &str = include_str!("../providers/fixtures/agy-ask.jsonl");
const AGY_ASK_TRANSCRIPT: &str = include_str!("../providers/fixtures/agy-ask-transcript.jsonl");
const AGY_ASKED: &str = "agy asked: Which test runner should code/my-app use?";
/// The message agy took in the recording.
const AGY_ANSWER: &str =
    "My answers to your questions:\n- Which test runner should code/my-app use? \u{2192} Vitest";

/// An agy tab in its default mode.
fn agy_start() -> Start {
    Start {
        mode: AGY.info().default_mode.map(String::from),
        ..start()
    }
}

/// Scene `name` in both themes with an agy tab.
fn agy_scene(
    name: &str,
    act: impl Fn(&str, &mut VisualTestAppContext, AnyWindowHandle, &Entity<AgentTab>) + Clone,
) {
    for &(slug, theme_name) in shot_themes() {
        let theme = shot_theme(theme_name);
        let slot: Slot = Rc::default();
        let keep = slot.clone();
        let n = format!("{name}-{slug}");
        let act = act.clone();
        let path = shoot_with(
            name,
            slug,
            &theme,
            size(px(DIMS.0), px(DIMS.1)),
            build_with(&AGY, theme, slot, agy_start()),
            move |cx, window| {
                let tab = tab_of(&keep);
                act(&n, cx, window, &tab);
                keep.borrow_mut().take();
            },
        );
        println!("ui-shots: {}", path.display());
    }
}

/// `once` with an agy tab.
fn agy_once(
    n: &'static str,
    act: impl FnOnce(&mut VisualTestAppContext, AnyWindowHandle, &Entity<AgentTab>) + 'static,
) {
    let theme = shot_theme("Catppuccin Mocha");
    let slot: Slot = Rc::default();
    let keep = slot.clone();
    render_with(
        &theme,
        size(px(DIMS.0), px(DIMS.1)),
        build_with(&AGY, theme, slot, agy_start()),
        move |cx, window| {
            let tab = tab_of(&keep);
            act(cx, window, &tab);
            keep.borrow_mut().take();
        },
    );
    println!("ui-shots: {n}");
}

/// A message sent, agy's stdout for the turn (its ask is a bare `unknown` step there), then
/// its main transcript as the follower reads it: the skipped ask becomes the card.
fn agy_asks(cx: &mut VisualTestAppContext, tab: &Entity<AgentTab>) {
    be_key(cx, tab);
    feed(cx, tab, |t, cx| {
        t.send(
            "Ask me one multiple-choice question about which test runner code/my-app should use"
                .into(),
            cx,
        )
    });
    for line in AGY_ASK.lines() {
        feed(cx, tab, |t, cx| {
            t.shot_process(ProcessEvent::Line(line.into()), cx)
        });
    }
    let lines: Vec<String> = AGY_ASK_TRANSCRIPT
        .lines()
        .take(7)
        .map(String::from)
        .collect();
    feed(cx, tab, |t, cx| {
        t.shot_followed(crate::providers::agy::MAIN_FOLLOW, lines, cx)
    });
}

/// The last thing you said, as its bubble shows it.
fn last_you(cx: &VisualTestAppContext, tab: &Entity<AgentTab>) -> Option<String> {
    cx.read(|cx| {
        tab.read(cx)
            .shot_items()
            .iter()
            .rev()
            .find_map(|i| match i {
                Item::You { text, .. } => Some(text.clone()),
                _ => None,
            })
    })
}

/// The `user` lines written to agy, their text.
fn agy_said(cx: &VisualTestAppContext, tab: &Entity<AgentTab>) -> Vec<String> {
    cx.read(|cx| tab.read(cx).shot_written())
        .iter()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .filter(|v| v["event"] == "user")
        .filter_map(|v| v["message"]["content"].as_str().map(String::from))
        .collect()
}

/// The user's words (2026-11-24): "it doesn't want to show the popup, it says it skips it".
/// agy's print mode skipped its own question and went on; the card shows it anyway, saying
/// agy couldn't wait (a Waiting mark that doesn't breathe), why, and that your picks go back as
/// a new message. The tab isn't Waiting; the banner line says "agy asked: …". Picking sends
/// the message through the normal send (the bubble); Skip only closes the card; another
/// message leaves it not answered.
pub(super) fn agent_tab_question_agy() {
    agy_scene("agent-tab-question-agy", |n, cx, window, tab| {
        agy_asks(cx, tab);
        settle_frames(cx, window);
        cx.read(|cx| {
            let t = tab.read(cx);
            assert!(t.questions_by_message(), "{n}");
            assert_ne!(t.status(), Status::Waiting, "{n}: agy isn't blocked");
            assert_eq!(t.question().as_deref(), Some(AGY_ASKED), "{n}");
            assert!(t.asking(), "{n}: the box and Enter answer it");
            assert!(t.waiting_ask_since().is_some(), "{n}: ⇧⌘A reaches it");
            assert!(t.shot_part("ask-card").is_some(), "{n}: the card is drawn");
            let tools = t
                .shot_items()
                .iter()
                .filter(|i| matches!(i, Item::Tool { .. }))
                .count();
            assert_eq!(
                tools, 1,
                "{n}: stdout's run_command; the transcript adds no rows"
            );
        });
        let ask = ask_of(cx, tab);
        assert_eq!(ask.cursor, 0, "{n}: on the recommended option");
        assert!(ask.focus && ask.keys_on, "{n}: the card has the keys");
    });
    agy_once(
        "agent-tab-question-agy: picking sends",
        |cx, window, tab| {
            let n = "agent-tab-question-agy";
            agy_asks(cx, tab);
            let before = agy_said(cx, tab).len();
            past_guard();
            keys(cx, window, "enter");
            assert_eq!(
                last_you(cx, tab).as_deref(),
                Some(AGY_ANSWER),
                "{n}: the bubble"
            );
            let said = agy_said(cx, tab);
            assert_eq!(said.len(), before + 1, "{n}: one message");
            assert_eq!(said.last().map(String::as_str), Some(AGY_ANSWER), "{n}");
            assert_eq!(
                reply_of(cx, tab),
                Some(Reply::Answered(vec![QAnswer {
                    labels: vec!["(Recommended) Vitest".into()],
                    other: None,
                }])),
                "{n}"
            );
            assert!(cx.read(|cx| tab.read(cx).shot_ask(cx)).is_none(), "{n}");
        },
    );
    agy_once("agent-tab-question-agy: skip", |cx, window, tab| {
        let n = "agent-tab-question-agy: skip";
        agy_asks(cx, tab);
        let before = cx.read(|cx| tab.read(cx).shot_written()).len();
        past_guard();
        keys(cx, window, "escape escape");
        assert_eq!(reply_of(cx, tab), Some(Reply::Skipped), "{n}");
        assert_eq!(
            cx.read(|cx| tab.read(cx).shot_written()).len(),
            before,
            "{n}: closed here, nothing sent"
        );
    });
    agy_once("agent-tab-question-agy: another message", |cx, _, tab| {
        let n = "agent-tab-question-agy: another message";
        agy_asks(cx, tab);
        feed(cx, tab, |t, cx| t.send("Never mind, use Jest".into(), cx));
        assert_eq!(reply_of(cx, tab), Some(Reply::Expired), "{n}: not answered");
        assert_eq!(last_you(cx, tab).as_deref(), Some("Never mind, use Jest"));
    });
}

/// agy's account has no Auto (2026-11-24): agy says so on stderr and runs in its default mode.
/// The chip follows it, one quiet notice says so, and this Mac's new agy tabs start in Accept
/// edits from now on.
pub(super) fn agent_tab_agy_mode_refused() {
    const REFUSED: &str =
        "warning: --mode auto is not available for this account; continuing in the default mode";
    agy_scene("agent-tab-agy-mode-refused", |n, cx, window, tab| {
        be_key(cx, tab);
        assert_eq!(
            cx.read(|cx| crate::mode_memory::start_mode_in(cx, "agy", Some("auto"))),
            Some("auto".to_string()),
            "{n}: nothing refused yet"
        );
        for _ in 0..2 {
            feed(cx, tab, |t, cx| {
                t.shot_process(ProcessEvent::ErrLine(REFUSED.into()), cx)
            });
        }
        feed(cx, tab, |t, cx| t.send("Say hello in one word".into(), cx));
        for line in include_str!("../providers/fixtures/agy-text.jsonl").lines() {
            feed(cx, tab, |t, cx| {
                t.shot_process(ProcessEvent::Line(line.into()), cx)
            });
        }
        settle_frames(cx, window);
        cx.read(|cx| {
            let t = tab.read(cx);
            assert_eq!(t.mode(), Some("accept-edits"), "{n}: agy's real mode");
            assert_eq!(
                t.shot_box(cx).mode.as_deref(),
                Some("Accept edits"),
                "{n}: the chip shows it"
            );
            let notices: Vec<&str> = t
                .shot_items()
                .iter()
                .filter_map(|i| match i {
                    Item::Notice(text) => Some(text.as_str()),
                    _ => None,
                })
                .collect();
            assert_eq!(
                notices,
                ["Auto isn't available on this agy account \u{2014} running in Accept edits"],
                "{n}: said once"
            );
            assert_eq!(
                crate::mode_memory::start_mode_in(cx, "agy", Some("auto")).as_deref(),
                Some("accept-edits"),
                "{n}: new agy tabs start there"
            );
        });
    });
}

/// In a workspace (2026-11-24): agy's question in a tab not in front makes a banner saying
/// "agy asked: …" (titled "agy asked a question") though the tab never waits, and ⌘⇧A lands on its card. After agy refused
/// Auto, a new agy tab starts in Accept edits.
pub(super) fn agent_question_agy_anywhere() {
    use super::agent::agent_workspace;
    use crate::layout_model::TabKind;
    let n = "agent-question-agy-anywhere";
    let theme = shot_theme("Catppuccin Mocha");
    let slot: Rc<RefCell<Option<Entity<crate::workspace::Workspace>>>> = Rc::default();
    let keep = slot.clone();
    let build = agent_workspace(theme, slot, |ws, window, cx| {
        let my_app = std::path::Path::new("/Users/me/code/my-app");
        let a = ws.open_unnamed_shot_session(Some(my_app), super::SHELL_TEXT, window, cx);
        ws.agent_beside(a, "agy", window, cx);
        let cli = std::path::Path::new("/Users/me/fuxx-cli/src");
        let b = ws.open_unnamed_shot_session(Some(cli), super::SHELL_TEXT, window, cx);
        ws.agent_beside(b, "agy", window, cx);
    });
    render_with(
        &theme,
        size(px(1100.), px(700.)),
        build,
        move |cx, window| {
            let ws = keep.borrow_mut().take().expect("the workspace");
            settle_frames(cx, window);
            let tabs: Vec<(crate::layout_model::TabId, Entity<AgentTab>)> = cx.read(|cx| {
                ws.read(cx)
                    .agents
                    .iter()
                    .map(|(t, s)| (*t, s.view.clone()))
                    .collect()
            });
            assert_eq!(tabs.len(), 2, "{n}: two agy tabs");
            let mode_of = |cx: &VisualTestAppContext, tab| {
                cx.read(
                    |cx| match ws.read(cx).model.tab(tab).map(|t| t.kind.clone()) {
                        Some(TabKind::Agent { mode, .. }) => mode,
                        _ => None,
                    },
                )
            };
            assert_eq!(mode_of(cx, tabs[0].0).as_deref(), Some("auto"), "{n}");
            let focused = cx.read(|cx| ws.read(cx).focused_agent_tab().map(|(t, _)| t));
            let (behind, _) = tabs
                .iter()
                .find(|(t, _)| Some(*t) != focused)
                .cloned()
                .expect("a tab not in front");
            let view = tabs.iter().find(|(t, _)| *t == behind).unwrap().1.clone();
            agy_asks(cx, &view);
            settle_frames(cx, window);
            assert_ne!(
                cx.read(|cx| view.read(cx).shown_status()),
                Status::Waiting,
                "{n}: the tab doesn't wait"
            );
            let banners = cx.read(|cx| ws.read(cx).shot_banners.clone());
            assert!(
                banners.iter().any(|(_, _, body)| body == AGY_ASKED),
                "{n}: the banner says agy asked: {banners:?}"
            );
            // Its own title, not Waiting's "Needs your input" (the lead's nit).
            assert!(
                banners.iter().any(|(title, _, body)| body == AGY_ASKED
                    && title.ends_with("\u{2014} agy asked a question")),
                "{n}: the banner's title: {banners:?}"
            );
            cx.simulate_keystrokes(window, "cmd-shift-a");
            settle_frames(cx, window);
            let now = cx.read(|cx| ws.read(cx).focused_agent_tab().map(|(t, _)| t));
            assert_eq!(now, Some(behind), "{n}: ⌘⇧A shows its tab");
            assert!(ask_of(cx, &view).keys_on, "{n}: the keys on its card");
            // agy refused Auto in that tab: a new agy tab starts in Accept edits.
            feed(cx, &view, |t, cx| {
                t.shot_process(
                    ProcessEvent::ErrLine(
                        "--mode auto is not available; continuing in the default mode".into(),
                    ),
                    cx,
                )
            });
            let before: Vec<_> = tabs.iter().map(|(t, _)| *t).collect();
            cx.update_window(window, |_, window, cx| {
                ws.update(cx, |ws, cx| {
                    let s = ws.open_unnamed_shot_session(None, super::SHELL_TEXT, window, cx);
                    ws.agent_beside(s, "agy", window, cx);
                })
            })
            .unwrap();
            settle_frames(cx, window);
            let new = cx.read(|cx| {
                ws.read(cx)
                    .agents
                    .keys()
                    .copied()
                    .find(|t| !before.contains(t))
            });
            let new = new.expect("the new agy tab");
            assert_eq!(
                mode_of(cx, new).as_deref(),
                Some("accept-edits"),
                "{n}: remembered on this Mac"
            );
        },
    );
    println!("ui-shots: {n}");
}
