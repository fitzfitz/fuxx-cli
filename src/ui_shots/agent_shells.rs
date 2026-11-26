//! Chore 16's scenes (note 93): an agent's background shells in the `▣ done/all` tab beside the
//! plan tab (with the plan, alone, all done, one failed) and their popover.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use gpui::{Entity, VisualTestAppContext, px};
use serde_json::json;

use super::agent::{decode, feed};
use super::agent_activity::{DIMS, click, give, part, shoot};
use super::{Kind, Scene, Themes, frame_fresh};
use crate::agent_tab::{AgentTab, AgentTabEvent};
use crate::providers::{Event, TodoItem, TodoState};

pub(super) const FILE: &str = file!();

const SOURCES: &[&str] = &[
    "src/agent_shells.rs",
    "src/agent_shells_view.rs",
    "src/agent_box.rs",
    "src/agent_box_menus.rs",
    "src/agent_rows.rs",
    "src/agent_conversation.rs",
    "src/providers/claude/shells.rs",
];

const fn scene(name: &'static str, run: fn()) -> Scene {
    Scene {
        name,
        run,
        themes: Themes::Both,
        serial: false,
        kind: Kind::State,
        sources: SOURCES,
    }
}

pub(super) const SCENES: &[Scene] = &[
    scene("agent-shells-with-plan", agent_shells_with_plan),
    scene("agent-shells-alone", agent_shells_alone),
    scene("agent-shells-done", agent_shells_done),
    scene("agent-shells-failed", agent_shells_failed),
    scene("agent-shells-popover", agent_shells_popover),
    scene("agent-shells-cleared", agent_shells_cleared),
];

/// The shells the scenes run: their description, command, how long ago they started and their
/// output's last line.
const SHELLS: &[(&str, &str, u64, &str)] = &[
    (
        "Run the test suite",
        "cargo test --workspace",
        252,
        "test retry::backs_off_twice ... ok",
    ),
    (
        "Serve my-app on port 3000",
        "npm run dev -- --port 3000",
        1105,
        "ready in 412 ms \u{b7} http://localhost:3000",
    ),
    (
        "Watch retry.rs and re-run its tests",
        "cargo watch -x 'test retry'",
        61,
        "[Finished running. Exit status: 0]",
    ),
];

/// Claude's lines for shell `n` of `SHELLS`: its Bash call (`run_in_background`), its task and
/// the call's result naming its output file.
fn started(n: usize) -> String {
    let (description, command, _, _) = SHELLS[n];
    let (call, id) = (format!("toolu_shell{n}"), format!("bsh{n}"));
    [
        json!({"type":"assistant","message":{"id":format!("m{n}"),"content":[{"type":"tool_use",
            "id":call,"name":"Bash","input":{"command":command,"description":description,
            "run_in_background":true}}]}}),
        json!({"type":"system","subtype":"task_started","task_id":id,"tool_use_id":call,
            "description":description,"is_backgrounded":true,"task_type":"local_bash"}),
        json!({"type":"user","message":{"role":"user","content":[{"type":"tool_result",
            "tool_use_id":call,"is_error":false,"content":format!(
            "Command running in background with ID: {id}. Output is being written to: \
            /tmp/fx/tasks/{id}.output. You will be notified when it completes.")}]}}),
    ]
    .map(|l| l.to_string())
    .join("\n")
}

/// Claude's notification that shell `n` ended `status` (exit `code`).
pub(super) fn ended(n: usize, status: &str, code: i32) -> String {
    json!({"type":"system","subtype":"task_notification","task_id":format!("bsh{n}"),
        "tool_use_id":format!("toolu_shell{n}"),"status":status,
        "output_file":format!("/tmp/fx/tasks/bsh{n}.output"),
        "summary":format!("Background command \"{}\" completed (exit code {code})", SHELLS[n].0)})
    .to_string()
}

/// You asked; shells `0..count` started (as Claude says it), dated back as `SHELLS` says, with
/// their last lines; then `ends` (Claude's notifications) came.
fn run_shells(
    cx: &mut VisualTestAppContext,
    tab: &Entity<AgentTab>,
    count: usize,
    ends: &[String],
) {
    feed(cx, tab, |t, cx| {
        t.send("Run the checks and serve my-app".into(), cx)
    });
    give_shells(cx, tab, count, ends);
}

/// `run_shells` after your message (the corner tabs' scenes send their own).
pub(super) fn give_shells(
    cx: &mut VisualTestAppContext,
    tab: &Entity<AgentTab>,
    count: usize,
    ends: &[String],
) {
    // One translator reads them all (it knows which tasks are shells); the endings are given
    // after the shells are dated back.
    let mut lines: Vec<String> = (0..count).map(started).collect();
    lines.extend(ends.iter().cloned());
    let (endings, events): (Vec<Event>, Vec<Event>) = decode(&lines.join("\n"))
        .into_iter()
        .partition(|e| matches!(e, Event::ShellEnded { .. }));
    give(cx, tab, events);
    feed(cx, tab, |t, cx| {
        t.shot_shells(
            |list| {
                for (shell, &(_, _, ago, line)) in list.iter_mut().zip(SHELLS) {
                    shell.started -= Duration::from_secs(ago);
                    shell.last_line = Some(line.to_string());
                }
            },
            cx,
        )
    });
    give(cx, tab, endings);
}

/// The plan's six tasks, four done (`☰ 4/6`).
fn plan(cx: &mut VisualTestAppContext, tab: &Entity<AgentTab>) {
    plan_done(cx, tab, 4);
}

/// The plan's six tasks, the first `done` done and the next one active.
pub(super) fn plan_done(cx: &mut VisualTestAppContext, tab: &Entity<AgentTab>, done: usize) {
    let item = |text: &str, state| TodoItem {
        text: text.into(),
        state,
    };
    give(
        cx,
        tab,
        vec![Event::Todo(
            [
                "Task 1: the retry policy",
                "Task 2: back off on 429",
                "Task 3: retry.rs tests",
                "Task 4: wire it into the client",
                "Task 5: run the suite",
                "Task 6: docs",
            ]
            .iter()
            .enumerate()
            .map(|(i, text)| {
                let state = match i.cmp(&done) {
                    std::cmp::Ordering::Less => TodoState::Done,
                    std::cmp::Ordering::Equal => TodoState::Active,
                    std::cmp::Ordering::Greater => TodoState::Pending,
                };
                item(text, state)
            })
            .collect(),
        )],
    );
}

/// The shells pill's words, as drawn.
fn tab_words(cx: &VisualTestAppContext, tab: &Entity<AgentTab>) -> String {
    cx.read(|cx| crate::agent_shells::tab_words(tab.read(cx).conversation.shells()))
}

/// The corner tabs end at the box's right edge.
fn flush_right(cx: &VisualTestAppContext, tab: &Entity<AgentTab>, n: &str) {
    let corner = part(cx, tab, n, "corner-tabs");
    let outer = part(cx, tab, n, "outer");
    assert!(
        (corner.right() - outer.right()).abs() < px(1.5),
        "{n}: the corner tabs end at the box's right edge ({corner:?} vs {outer:?})"
    );
}

/// Three shells with the plan: `▣ 1/3` in peach on the plan tab's left, the two in one tab
/// flush right; each running Bash row reads "running in the background · 4:12".
pub(super) fn agent_shells_with_plan() {
    shoot("agent-shells-with-plan", DIMS, |n, cx, window, tab, _| {
        run_shells(cx, tab, 3, &[ended(2, "completed", 0)]);
        plan(cx, tab);
        let _ = frame_fresh(cx, window);
        cx.run_until_parked();
        let _ = frame_fresh(cx, window);
        assert_eq!(tab_words(cx, tab), "1/3", "{n}");
        let shells = part(cx, tab, n, "shells-tab");
        let plan = part(cx, tab, n, "plan-tab");
        assert!(
            shells.right() <= plan.left(),
            "{n}: the shells tab on the plan tab's left ({shells:?} vs {plan:?})"
        );
        assert!(
            (shells.center().y - plan.center().y).abs() < px(1.),
            "{n}: one row"
        );
        assert!(
            shells.left() > part(cx, tab, n, "folder").right(),
            "{n}: right of the folder tab"
        );
        flush_right(cx, tab, n);
        let running = cx.read(|cx| {
            let c = &tab.read(cx).conversation;
            c.shells().running_for_call("toolu_shell0").is_some()
        });
        assert!(running, "{n}: its Bash row has a running shell");
    });
}

/// One shell, no plan: the shells tab alone takes the corner.
pub(super) fn agent_shells_alone() {
    shoot("agent-shells-alone", DIMS, |n, cx, window, tab, _| {
        run_shells(cx, tab, 2, &[]);
        let _ = frame_fresh(cx, window);
        cx.run_until_parked();
        let _ = frame_fresh(cx, window);
        assert_eq!(tab_words(cx, tab), "0/2", "{n}");
        assert!(cx.read(|cx| tab.read(cx).shot_part("plan-tab")).is_none());
        let _ = part(cx, tab, n, "shells-tab");
        flush_right(cx, tab, n);
    });
}

/// All three done: the tab its green icon alone (chore 34), kept until your next message; each ending told (the
/// Workspace's banner when you aren't looking: "Run the test suite · done in 6:40").
pub(super) fn agent_shells_done() {
    shoot("agent-shells-done", DIMS, |n, cx, window, tab, _| {
        let heard: Rc<RefCell<Vec<String>>> = Rc::default();
        let keep = heard.clone();
        cx.update(|cx| {
            cx.subscribe(tab, move |_, e: &AgentTabEvent, _| {
                if let AgentTabEvent::ShellEnded { words, .. } = e {
                    keep.borrow_mut().push(words.clone());
                }
            })
            .detach()
        });
        let ends: Vec<String> = (0..3).map(|i| ended(i, "completed", 0)).collect();
        run_shells(cx, tab, 3, &ends);
        let _ = frame_fresh(cx, window);
        assert_eq!(tab_words(cx, tab), "", "{n}: no count, no check (chore 34)");
        let heard = heard.borrow();
        assert_eq!(heard.len(), 3, "{n}: each ending told once: {heard:?}");
        assert!(
            heard[0].starts_with("Run the test suite \u{b7} done in "),
            "{n}: {heard:?}"
        );
    });
}

/// One of two failed: red `1/2 ✕`, until you open the popover.
pub(super) fn agent_shells_failed() {
    shoot("agent-shells-failed", DIMS, |n, cx, window, tab, _| {
        run_shells(cx, tab, 2, &[ended(0, "completed", 101)]);
        let _ = frame_fresh(cx, window);
        assert_eq!(tab_words(cx, tab), "1/2 \u{2715}", "{n}");
        let _ = part(cx, tab, n, "shells-tab");
    });
}

/// All done and the popover open, then your next message: the tab goes, and its popover with it
/// (the review's finding 2: it stayed open unseen, and `/` opened nothing).
pub(super) fn agent_shells_cleared() {
    shoot("agent-shells-cleared", DIMS, |n, cx, window, tab, _| {
        let ends: Vec<String> = (0..2).map(|i| ended(i, "completed", 0)).collect();
        run_shells(cx, tab, 2, &ends);
        let _ = frame_fresh(cx, window);
        click(cx, window, part(cx, tab, n, "shells-tab"));
        let _ = frame_fresh(cx, window);
        assert_eq!(
            cx.read(|cx| tab.read(cx).shot_menu()),
            Some(crate::agent_box::BoxMenu::Shells),
            "{n}: open"
        );
        feed(cx, tab, |t, cx| t.send("Now update the docs".into(), cx));
        let _ = frame_fresh(cx, window);
        assert!(
            cx.read(|cx| tab.read(cx).conversation.shells().is_empty()),
            "{n}"
        );
        assert_eq!(
            cx.read(|cx| tab.read(cx).shot_menu()),
            None,
            "{n}: the popover went with its tab"
        );
    });
}

/// The popover over the chat, its right edge on the box's: "1 of 3 done · 2 running", each
/// shell's time, command and last line; ■ stops one (`stop_task`); a row opens its output.
pub(super) fn agent_shells_popover() {
    shoot("agent-shells-popover", DIMS, |n, cx, window, tab, told| {
        run_shells(cx, tab, 3, &[ended(2, "completed", 0)]);
        plan(cx, tab);
        let _ = frame_fresh(cx, window);
        let pill = part(cx, tab, n, "shells-tab");
        click(cx, window, pill);
        let _ = frame_fresh(cx, window);
        assert_eq!(
            cx.read(|cx| tab.read(cx).shot_menu()),
            Some(crate::agent_box::BoxMenu::Shells),
            "{n}: the click opens it"
        );
        let header = cx.read(|cx| tab.read(cx).conversation.shells().header());
        assert_eq!(header, "1 of 3 done \u{b7} 2 running", "{n}");
        let popover = part(cx, tab, n, "shells-popover");
        let outer = part(cx, tab, n, "outer");
        assert!(
            popover.right() <= outer.right() + px(1.) && popover.right() > outer.right() - px(24.),
            "{n}: the popover ends at the box's right edge ({popover:?} vs {outer:?})"
        );
        assert!(popover.bottom() < pill.top(), "{n}: over the chat");
        // A row: its output in a file tab (the popover closes); open it again for the shot.
        click(cx, window, part(cx, tab, n, "shell-row"));
        let _ = frame_fresh(cx, window);
        assert_eq!(
            told.borrow().as_slice(),
            [PathBuf::from("/tmp/fx/tasks/bsh0.output")],
            "{n}: the first row opens its output"
        );
        click(cx, window, part(cx, tab, n, "shells-tab"));
        let _ = frame_fresh(cx, window);
        // ■ stops it as a subagent is stopped (it keeps running until Claude says).
        click(cx, window, part(cx, tab, n, "shell-stop"));
        let _ = frame_fresh(cx, window);
        let written = cx.read(|cx| tab.read(cx).shot_written());
        assert!(
            written
                .iter()
                .any(|l| l.contains("\"stop_task\"") && l.contains("\"bsh0\"")),
            "{n}: ■ writes stop_task: {written:?}"
        );
        assert_eq!(
            cx.read(|cx| tab.read(cx).shot_menu()),
            Some(crate::agent_box::BoxMenu::Shells),
            "{n}: ■ leaves it open"
        );
    });
}
