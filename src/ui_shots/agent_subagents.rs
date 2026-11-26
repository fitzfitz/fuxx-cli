//! Chore 19's scenes (note 93): the subagents in a `🤖 done/all` corner tab beside the shells
//! and plan tabs, its popover (a row opens that subagent; one that needs you in red takes you
//! to its question), and the top row gone on Main, a slim `← Main › 🤖 <label>` inside a
//! subagent.

use gpui::{Entity, VisualTestAppContext, px};

use super::agent::decode;
use super::agent_activity::{DIMS, click, give, part, shoot};
use super::{Kind, Scene, Themes, frame_fresh};
use crate::agent_conversation::LaneStatus;
use crate::agent_lanes::View;
use crate::agent_tab::AgentTab;
use crate::providers::{Event, TodoItem, TodoState};

pub(super) const FILE: &str = file!();

const SOURCES: &[&str] = &[
    "src/agent_subagents.rs",
    "src/agent_subagents_view.rs",
    "src/agent_lanes_view.rs",
    "src/agent_box.rs",
    "src/agent_box_menus.rs",
    "src/agent_conversation.rs",
    "src/providers/fixtures/claude-subagents.jsonl",
    "src/providers/fixtures/claude-subagent-ask.jsonl",
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

/// Chore 34's corner tabs together: the subagents', the shells' and the plan's.
const CORNER: &[&str] = &[
    "src/corner_tabs.rs",
    "src/agent_subagents.rs",
    "src/agent_subagents_view.rs",
    "src/agent_shells.rs",
    "src/agent_shells_view.rs",
    "src/agent_box.rs",
    "src/agent_box_menus.rs",
    "src/agent_conversation.rs",
    "src/plan_progress.rs",
    "src/providers/claude/shells.rs",
    "src/providers/fixtures/claude-subagents.jsonl",
    "src/providers/fixtures/claude-subagent-ask.jsonl",
];

const fn corner(name: &'static str, run: fn()) -> Scene {
    Scene {
        name,
        run,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: CORNER,
    }
}

pub(super) const SCENES: &[Scene] = &[
    scene("agent-subagents-tab", agent_subagents_tab),
    scene("agent-subagents-popover", agent_subagents_popover),
    scene("agent-subagents-waiting", agent_subagents_waiting),
    scene("agent-subagents-many", agent_subagents_many),
    corner("agent-corner-tabs-running", agent_corner_tabs_running),
    corner("agent-corner-tabs-some-done", agent_corner_tabs_some_done),
    corner("agent-corner-tabs-all-done", agent_corner_tabs_all_done),
    corner("agent-corner-tabs-needs-you", agent_corner_tabs_needs_you),
];

const SUBAGENTS: &str = include_str!("../providers/fixtures/claude-subagents.jsonl");
const SUB_ASK: &str = include_str!("../providers/fixtures/claude-subagent-ask.jsonl");

/// You asked; Claude's two parallel subagents (the recording) ran, the second still working;
/// a plan of three tasks, one done.
fn two_subagents(cx: &mut VisualTestAppContext, tab: &Entity<AgentTab>) -> Vec<String> {
    let mut events = decode(SUBAGENTS);
    let ids: Vec<String> = events
        .iter()
        .filter_map(|e| match e {
            Event::SubagentStarted { id, .. } => Some(id.clone()),
            _ => None,
        })
        .collect();
    events.retain(|e| {
        !matches!(e, Event::SubagentEnded { id, .. } if *id == ids[1])
            && !matches!(e, Event::TurnDone { .. })
    });
    super::agent::feed(cx, tab, |t, cx| {
        t.send("Read a.txt and b.txt with two subagents".into(), cx)
    });
    let item = |text: &str, state| TodoItem {
        text: text.into(),
        state,
    };
    events.push(Event::Todo(vec![
        item("Read a.txt", TodoState::Done),
        item("Read b.txt", TodoState::Active),
        item("Report both", TodoState::Pending),
    ]));
    give(cx, tab, events);
    ids
}

fn lanes(
    cx: &VisualTestAppContext,
    tab: &Entity<AgentTab>,
) -> Vec<crate::agent_conversation::Lane> {
    cx.read(|cx| tab.read(cx).shot_lanes().to_vec())
}

fn drawn(cx: &VisualTestAppContext, tab: &Entity<AgentTab>, name: &str) -> bool {
    cx.read(|cx| tab.read(cx).shot_part_now(name))
}

fn view(cx: &VisualTestAppContext, tab: &Entity<AgentTab>) -> View {
    cx.read(|cx| tab.read(cx).view.clone())
}

/// The tab: `🤖 1/2` in the corner tab, left of the plan's `☰`, on the folder tab's row; on
/// Main no top row (no pills, no breadcrumb), the chat 16 pt down (chore 21). ⌥⌘] steps into the newest subagent: the slim row
/// `← Main › 🤖 <label>` shows 16 pt down (the running subagent's own view is the running mark, so
/// the folder tab has none and the subagents tab no arc: chore 31); `← Main` goes back; ⌥⌘[ from
/// the newest steps to Main.
pub(super) fn agent_subagents_tab() {
    shoot("agent-subagents-tab", DIMS, |n, cx, window, tab, _| {
        let ids = two_subagents(cx, tab);
        let _ = frame_fresh(cx, window);
        assert_eq!(
            crate::agent_subagents::tab_words(&lanes(cx, tab)),
            "1/2",
            "{n}"
        );
        let robot = part(cx, tab, n, "subagents-tab");
        let plan = part(cx, tab, n, "plan-tab");
        let corner = part(cx, tab, n, "corner-tabs");
        assert!(robot.right() <= plan.left(), "{n}: left of the plan pill");
        assert!(
            robot.left() >= corner.left() && plan.right() <= corner.right(),
            "{n}: both in the corner tab"
        );
        assert!(
            (robot.center().y - plan.center().y).abs() < px(1.),
            "{n}: one row"
        );
        assert!(!drawn(cx, tab, "strip"), "{n}: no pill row on Main");
        assert!(!drawn(cx, tab, "lane-header"), "{n}: no breadcrumb on Main");
        // No empty band at the top (chore 21): Main's first row starts the chat's standard gap
        // under the tab's top (the top bar's empty 32 pt is gone).
        let gap = crate::ui_tokens::chat::FIRST_TOP;
        let first = part(cx, tab, n, "you-bubble");
        assert!(
            f32::from(first.top()) <= gap + 1.0,
            "{n}: Main's first row {gap} pt down, not {:?}",
            first.top()
        );
        cx.simulate_keystrokes(window, "alt-cmd-]");
        cx.run_until_parked();
        assert_eq!(
            view(cx, tab),
            View::Lane(ids[1].clone()),
            "{n}: ⌥⌘] to the newest"
        );
        cx.read(|cx| tab.read(cx).shot_forget(&["status", "subagents-tab-arc"]));
        let _ = frame_fresh(cx, window);
        let back = part(cx, tab, n, "lane-back");
        let header = part(cx, tab, n, "lane-header");
        assert!(
            back.left() <= header.left() + px(1.),
            "{n}: ← Main leads the row"
        );
        assert!(
            (f32::from(header.top()) - gap).abs() <= 1.0,
            "{n}: the slim row {gap} pt down, not {:?}",
            header.top()
        );
        assert!(
            !drawn(cx, tab, "status") && !drawn(cx, tab, "subagents-tab-arc"),
            "{n}: one running mark: the running subagent's view (chore 31)"
        );
        click(cx, window, back);
        assert_eq!(view(cx, tab), View::Main, "{n}: ← Main goes back");
        cx.simulate_keystrokes(window, "alt-cmd-]");
        cx.simulate_keystrokes(window, "alt-cmd-[");
        cx.run_until_parked();
        assert_eq!(view(cx, tab), View::Main, "{n}: ⌥⌘[ back to Main");
        cx.simulate_keystrokes(window, "alt-cmd-]");
        cx.run_until_parked();
        let _ = frame_fresh(cx, window);
    });
}

/// The popover: "1 of 2 done · 1 running", the running one first; a click on the finished one
/// opens its view and closes the popover. The plan's popover lists only its tasks now.
pub(super) fn agent_subagents_popover() {
    shoot("agent-subagents-popover", DIMS, |n, cx, window, tab, _| {
        let ids = two_subagents(cx, tab);
        let all = lanes(cx, tab);
        let _ = frame_fresh(cx, window);
        click(cx, window, part(cx, tab, n, "plan-tab"));
        let _ = frame_fresh(cx, window);
        let labels = cx.read(|cx| tab.read(cx).shot_menu_labels());
        assert!(
            labels
                .iter()
                .all(|l| l != "Subagents" && all.iter().all(|a| &a.label != l)),
            "{n}: the plan popover has no subagents: {labels:?}"
        );
        assert!(labels.iter().any(|l| l == "Read b.txt"), "{n}: {labels:?}");
        click(cx, window, part(cx, tab, n, "subagents-tab"));
        let _ = frame_fresh(cx, window);
        assert_eq!(
            cx.read(|cx| tab.read(cx).shot_menu()),
            Some(crate::agent_box::BoxMenu::Subagents),
            "{n}: the robot opens its popover"
        );
        assert_eq!(
            crate::agent_subagents::header(&all),
            "1 of 2 done \u{b7} 1 running",
            "{n}"
        );
        let popover = part(cx, tab, n, "subagents-popover");
        let outer = part(cx, tab, n, "outer");
        assert!(
            popover.right() <= outer.right() + px(1.) && popover.right() > outer.right() - px(24.),
            "{n}: its right edge on the box's ({popover:?} vs {outer:?})"
        );
        let labels = cx.read(|cx| tab.read(cx).shot_menu_labels());
        assert_eq!(
            labels,
            [all[1].label.clone(), all[0].label.clone()],
            "{n}: the running one first"
        );
        let done_row = cx
            .read(|cx| tab.read(cx).shot_menu_row(&all[0].label))
            .unwrap_or_else(|| panic!("{n}: the finished row"));
        click(cx, window, done_row);
        assert_eq!(view(cx, tab), View::Lane(ids[0].clone()), "{n}: its view");
        assert!(
            cx.read(|cx| tab.read(cx).shot_menu()).is_none(),
            "{n}: the popover closed"
        );
        click(cx, window, part(cx, tab, n, "subagents-tab"));
        let _ = frame_fresh(cx, window);
    });
}

/// A subagent asks: the tab is red (`TabTone::NeedsYou`), its row first, "Needs you"; a click
/// opens its view, where its question is, the keys on it.
pub(super) fn agent_subagents_waiting() {
    shoot("agent-subagents-waiting", DIMS, |n, cx, window, tab, _| {
        super::agent::feed(cx, tab, |t, cx| {
            t.send("Make hello.txt with a subagent".into(), cx)
        });
        let mut events = decode(SUB_ASK);
        let ask = events
            .iter()
            .position(|e| matches!(e, Event::Permission { .. }))
            .expect("the subagent asks");
        events.truncate(ask + 1);
        give(cx, tab, events);
        let all = lanes(cx, tab);
        assert_eq!(all[0].status, LaneStatus::Waiting, "{n}");
        assert_eq!(
            crate::agent_subagents::tab_tone(&all),
            crate::agent_subagents::TabTone::NeedsYou,
            "{n}"
        );
        let _ = frame_fresh(cx, window);
        click(cx, window, part(cx, tab, n, "subagents-tab"));
        let _ = frame_fresh(cx, window);
        let words = part(cx, tab, n, "subagent-needs-you");
        let row = cx
            .read(|cx| tab.read(cx).shot_menu_row(&all[0].label))
            .unwrap_or_else(|| panic!("{n}: its row"));
        assert!(
            words.top() >= row.top() && words.bottom() <= row.bottom(),
            "{n}: \"Needs you\" on its row"
        );
        let shot = frame_fresh(cx, window);
        let c = cx.read(|cx| {
            crate::agent_tab::Colors::of(&cx.global::<crate::appearance::Appearance>().theme)
        });
        let dims = gpui::size(px(DIMS.0), px(DIMS.1));
        let ui = cx.read(|cx| cx.global::<crate::appearance::Appearance>().theme.ui());
        let ink = super::ink_in(
            &shot,
            dims,
            words,
            [
                ui.composer_inner.r,
                ui.composer_inner.g,
                ui.composer_inner.b,
            ],
        );
        assert!(
            super::gap(ink, c.failed_panel) < 40,
            "{n}: in red ({ink:?} vs {:?})",
            c.failed_panel
        );
        click(cx, window, row);
        assert_eq!(
            view(cx, tab),
            View::Lane(all[0].id.clone()),
            "{n}: its view"
        );
        let _ = frame_fresh(cx, window);
        assert!(drawn(cx, tab, "lane-header"), "{n}: its breadcrumb");
    });
}

/// Fourteen subagents in a short window: the popover's list scrolls (ten rows show) and the
/// popover stays inside the window.
pub(super) fn agent_subagents_many() {
    shoot(
        "agent-subagents-many",
        (720.0, 520.0),
        |n, cx, window, tab, _| {
            super::agent::feed(cx, tab, |t, cx| t.send("Fan out".into(), cx));
            let events: Vec<Event> = (1..=14)
                .flat_map(|i| {
                    let id = format!("agent{i}");
                    [
                        Event::ToolStarted {
                            id: id.clone(),
                            name: "Agent".into(),
                            summary: format!("Check module {i}"),
                            parent: None,
                            detail: None,
                            writes: Vec::new(),
                            texts: Vec::new(),
                        },
                        Event::SubagentStarted {
                            id,
                            parent: None,
                            label: format!("Check module {i}"),
                            kind: "general-purpose".into(),
                            task: "Check it, then report".into(),
                            model: None,
                            controls: crate::providers::LaneControls::default(),
                        },
                    ]
                })
                .collect();
            give(cx, tab, events);
            let _ = frame_fresh(cx, window);
            assert_eq!(
                crate::agent_subagents::tab_words(&lanes(cx, tab)),
                "0/14",
                "{n}"
            );
            click(cx, window, part(cx, tab, n, "subagents-tab"));
            let _ = frame_fresh(cx, window);
            let popover = part(cx, tab, n, "subagents-popover");
            let list = part(cx, tab, n, "subagent-rows");
            assert!(
                popover.top() >= px(0.),
                "{n}: inside the window ({popover:?})"
            );
            assert!(
                list.size.height <= px(10. * 26. + 1.),
                "{n}: ten rows, then it scrolls ({list:?})"
            );
        },
    );
}

/// Chore 34's four states (the mockup's): what the subagents, shells and plan have done.
#[derive(Clone, Copy, PartialEq)]
enum Corner {
    /// One of two subagents, one of three shells and four of six tasks done.
    Running,
    /// The subagents and shells all finished, the plan still going.
    SomeDone,
    /// Everything finished, the turn over.
    AllDone,
    /// A subagent waits on you; the shells done, the plan going.
    NeedsYou,
}

/// How a corner tab looks: its count shown, or its icon alone.
#[derive(Clone, Copy)]
enum Look {
    Counting,
    Done,
}

/// One state: you asked; the subagents (the recordings), three shells and the plan as `state`
/// says; then each tab is checked: a finished one is its icon alone in green, an unfinished one
/// keeps its count (peach while running, the plan's dim, red while one needs you).
fn corner_tabs(name: &'static str, state: Corner) {
    shoot(name, DIMS, move |n, cx, window, tab, _| {
        use super::agent_shells::{ended, give_shells, plan_done};
        let ask = state == Corner::NeedsYou;
        super::agent::feed(cx, tab, |t, cx| {
            t.send("Read a.txt and b.txt with two subagents".into(), cx)
        });
        let all = decode(if ask { SUB_ASK } else { SUBAGENTS });
        let asked = all
            .iter()
            .position(|e| matches!(e, Event::Permission { .. }));
        let keep = if ask {
            asked.expect("the subagent asks") + 1
        } else {
            all.len()
        };
        let second = all
            .iter()
            .filter_map(|e| match e {
                Event::SubagentStarted { id, .. } => Some(id.clone()),
                _ => None,
            })
            .nth(1);
        let (mut events, mut turn_done) = (Vec::new(), Vec::new());
        for e in all.into_iter().take(keep) {
            match &e {
                Event::TurnDone { .. } => turn_done.push(e),
                // Running: the second subagent still works.
                Event::SubagentEnded { id, .. }
                    if state == Corner::Running && Some(id) == second.as_ref() => {}
                _ => events.push(e),
            }
        }
        give(cx, tab, events);
        let ends: Vec<String> = match state {
            Corner::Running => vec![ended(0, "completed", 0)],
            _ => (0..3).map(|i| ended(i, "completed", 0)).collect(),
        };
        give_shells(cx, tab, 3, &ends);
        plan_done(cx, tab, if state == Corner::AllDone { 6 } else { 4 });
        if state == Corner::AllDone {
            give(cx, tab, turn_done);
        }
        let shot = frame_fresh(cx, window);
        let c = cx.read(|cx| {
            crate::agent_tab::Colors::of(&cx.global::<crate::appearance::Appearance>().theme)
        });
        let dims = gpui::size(px(DIMS.0), px(DIMS.1));
        let (counting, done) = (|ink| (Look::Counting, ink), |ink| (Look::Done, ink));
        let looks = match state {
            Corner::Running => [
                counting(c.todo_active),
                counting(c.todo_active),
                counting(c.dim),
            ],
            Corner::SomeDone => [done(c.tool_ok), done(c.tool_ok), counting(c.dim)],
            Corner::AllDone => [done(c.tool_ok), done(c.tool_ok), done(c.tool_ok)],
            Corner::NeedsYou => [counting(c.failed_panel), done(c.tool_ok), counting(c.dim)],
        };
        let outer = [c.outer.r, c.outer.g, c.outer.b];
        for (name, (look, ink)) in ["subagents-tab", "shells-tab", "plan-tab"]
            .into_iter()
            .zip(looks)
        {
            let b = part(cx, tab, n, name);
            // A pill: 7 of room each side of the 13 pt icon, then 5 and its words.
            match look {
                Look::Done => assert!(
                    b.size.width <= px(28.),
                    "{n}: {name} is its icon alone {b:?}"
                ),
                Look::Counting => {
                    assert!(b.size.width >= px(40.), "{n}: {name} keeps its count {b:?}")
                }
            }
            let got = super::ink_in(&shot, dims, b, outer);
            assert!(
                super::gap(got, ink) < 40,
                "{n}: {name} in {ink:?}, not {got:?}"
            );
        }
        if state == Corner::AllDone {
            // A click on a finished tab still opens its list (and closes it again).
            for (name, menu) in [
                ("subagents-tab", crate::agent_box::BoxMenu::Subagents),
                ("shells-tab", crate::agent_box::BoxMenu::Shells),
                ("plan-tab", crate::agent_box::BoxMenu::Plan),
            ] {
                click(cx, window, part(cx, tab, n, name));
                assert_eq!(
                    cx.read(|cx| tab.read(cx).shot_menu()),
                    Some(menu),
                    "{n}: {name} opens its list"
                );
                click(cx, window, part(cx, tab, n, name));
                let _ = frame_fresh(cx, window);
            }
            assert!(
                cx.read(|cx| tab.read(cx).shot_menu()).is_none(),
                "{n}: closed again"
            );
            // The pointer off the tabs, so the picture shows them at rest.
            let away = gpui::point(px(40.), px(40.));
            cx.simulate_mouse_move(window, away, None, gpui::Modifiers::none());
            let _ = frame_fresh(cx, window);
        }
    });
}

pub(super) fn agent_corner_tabs_running() {
    corner_tabs("agent-corner-tabs-running", Corner::Running);
}

pub(super) fn agent_corner_tabs_some_done() {
    corner_tabs("agent-corner-tabs-some-done", Corner::SomeDone);
}

pub(super) fn agent_corner_tabs_all_done() {
    corner_tabs("agent-corner-tabs-all-done", Corner::AllDone);
}

pub(super) fn agent_corner_tabs_needs_you() {
    corner_tabs("agent-corner-tabs-needs-you", Corner::NeedsYou);
}
