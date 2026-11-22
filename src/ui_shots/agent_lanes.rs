//! M11ar's scenes: the subagent pill strip, and the chat switched to a subagent.

use gpui::{Entity, VisualTestAppContext, px};

use super::agent::{decode, feed, scene_sized};
use super::{Kind, Scene, Themes};
use crate::agent_conversation::LaneStatus;
use crate::agent_lanes::View;
use crate::agent_tab::AgentTab;
use crate::providers::{Event, LaneControls, LaneEnd};

pub(super) const FILE: &str = file!();

const SOURCES: &[&str] = &[
    "src/agent_lanes_view.rs",
    "src/agent_lanes.rs",
    "src/agent_tab.rs",
    "src/agent_rows.rs",
    "src/agent_chat.rs",
    "src/agent_conversation.rs",
    "src/providers/claude.rs",
    "src/providers/fixtures/claude-subagents.jsonl",
];

pub(super) const SCENES: &[Scene] = &[
    Scene {
        name: "agent-lanes-strip",
        run: agent_lanes_strip,
        themes: Themes::Both,
        serial: false,
        kind: Kind::State,
        sources: SOURCES,
    },
    Scene {
        name: "agent-lanes-overflow",
        run: agent_lanes_overflow,
        themes: Themes::Both,
        serial: false,
        kind: Kind::State,
        sources: SOURCES,
    },
    Scene {
        name: "agent-lanes-view",
        run: agent_lanes_view,
        themes: Themes::Both,
        serial: false,
        kind: Kind::State,
        sources: SOURCES,
    },
    Scene {
        name: "agent-lanes-states",
        run: agent_lanes_states,
        themes: Themes::Both,
        serial: false,
        kind: Kind::State,
        sources: SOURCES,
    },
];

const SUBAGENTS: &str = include_str!("../providers/fixtures/claude-subagents.jsonl");

fn part(
    cx: &VisualTestAppContext,
    tab: &Entity<AgentTab>,
    n: &str,
    name: &str,
) -> gpui::Bounds<gpui::Pixels> {
    cx.read(|cx| tab.read(cx).shot_part(name))
        .unwrap_or_else(|| panic!("{n}: {name} drawn"))
}

fn lanes_of(
    cx: &VisualTestAppContext,
    tab: &Entity<AgentTab>,
) -> Vec<(String, String, LaneStatus)> {
    cx.read(|cx| {
        tab.read(cx)
            .shot_lanes()
            .iter()
            .map(|l| (l.id.clone(), l.label.clone(), l.status.clone()))
            .collect()
    })
}

/// A made-up subagent (its lane, its row).
fn started(id: &str, label: &str) -> Vec<Event> {
    vec![
        Event::ToolStarted {
            id: id.into(),
            name: "Agent".into(),
            summary: label.into(),
            parent: None,
            detail: None,
            writes: Vec::new(),
            texts: Vec::new(),
        },
        Event::SubagentStarted {
            id: id.into(),
            parent: None,
            label: label.into(),
            kind: "general-purpose".into(),
            task: format!("{label}, then report"),
            model: None,
            controls: LaneControls { stop: true },
        },
    ]
}

/// Claude's two parallel subagents (the recording) with the second still working: the strip
/// under the top bar in the centred column — Main, then the two in start order — and the chat
/// switched to the first shows only its steps; back on Main its steps aren't rows.
pub(super) fn agent_lanes_strip() {
    scene_sized(
        "agent-lanes-strip",
        (1100.0, 640.0),
        |n, cx, window, tab| {
            let mut events = decode(SUBAGENTS);
            let second = events
                .iter()
                .filter_map(|e| match e {
                    Event::SubagentStarted { id, .. } => Some(id.clone()),
                    _ => None,
                })
                .nth(1)
                .expect("two subagents");
            // The second hasn't finished, and nor has the turn.
            events.retain(|e| {
                !matches!(e, Event::SubagentEnded { id, .. } if *id == second)
                    && !matches!(e, Event::TurnDone { .. })
            });
            feed(cx, tab, |t, cx| {
                t.send("Read a.txt and b.txt with two subagents".into(), cx)
            });
            feed(cx, tab, |t, cx| t.shot_feed(events, cx));
            let lanes = lanes_of(cx, tab);
            assert_eq!(lanes.len(), 2, "{n}");
            assert_eq!(lanes[0].2, LaneStatus::Done, "{n}");
            assert_eq!(lanes[1].2, LaneStatus::Working, "{n}");
            let _ = super::frame_fresh(cx, window);
            let strip = part(cx, tab, n, "strip");
            assert!(strip.top() > px(0.), "{n}: under the top bar ({strip:?})");
            assert!(
                f32::from(strip.size.width) <= crate::ui_tokens::chat::COLUMN_MAX + 1.0,
                "{n}: in the column"
            );
            let centre = f32::from(strip.left() + strip.size.width / 2.0);
            assert!((centre - 550.0).abs() <= 2.0, "{n}: centred ({centre})");
            // A lane's view: only its steps.
            let main_rows = cx.read(|cx| tab.read(cx).shot_list().item_count());
            let first = lanes[0].0.clone();
            feed(cx, tab, |t, cx| t.show_view(View::Lane(first.clone()), cx));
            let lane_rows = cx.read(|cx| tab.read(cx).shot_list().item_count());
            let steps = cx.read(|cx| tab.read(cx).shot_lane_steps(&first));
            assert_eq!(
                lane_rows,
                steps + 2,
                "{n}: from Main, the lane's own steps, its end ({lane_rows} rows)"
            );
            feed(cx, tab, |t, cx| t.show_view(View::Main, cx));
            assert_eq!(
                cx.read(|cx| tab.read(cx).shot_list().item_count()),
                main_rows,
                "{n}: Main's list as it was"
            );
        },
    );
}

/// Eight subagents in a 720 pt tab: the pills that fit, then "+N"; its list opens.
pub(super) fn agent_lanes_overflow() {
    scene_sized(
        "agent-lanes-overflow",
        (720.0, 640.0),
        |n, cx, window, tab| {
            feed(cx, tab, |t, cx| t.send("Fan out".into(), cx));
            let events: Vec<Event> = (1..=8)
                .flat_map(|i| started(&format!("agent{i}"), &format!("Check module {i}")))
                .collect();
            feed(cx, tab, |t, cx| t.shot_feed(events, cx));
            let _ = super::frame_fresh(cx, window);
            assert_eq!(lanes_of(cx, tab).len(), 8, "{n}");
            let more = part(cx, tab, n, "more");
            let strip = part(cx, tab, n, "strip");
            assert!(
                more.right() <= strip.right() + px(1.),
                "{n}: +N in the strip"
            );
            feed(cx, tab, |t, cx| t.shot_toggle_lanes_menu(cx));
            let _ = super::frame_fresh(cx, window);
            let menu = part(cx, tab, n, "lanes-menu");
            assert!(menu.top() >= more.top(), "{n}: the list under +N");
        },
    );
}

/// Each lane state's pill: working, waiting on you (tinted), done, failed, stopped.
pub(super) fn agent_lanes_states() {
    scene_sized(
        "agent-lanes-states",
        (1100.0, 640.0),
        |n, cx, window, tab| {
            feed(cx, tab, |t, cx| t.send("Five ways".into(), cx));
            let mut events: Vec<Event> = ["Working", "Waiting", "Done", "Failed", "Stopped"]
                .iter()
                .flat_map(|l| started(&l.to_lowercase(), l))
                .collect();
            events.push(Event::Permission {
                id: "p1".into(),
                tool: "Bash".into(),
                what: "ls".into(),
                detail: String::new(),
                always: None,
                by: Some("waiting".into()),
            });
            for (id, end) in [
                ("done", LaneEnd::Done),
                ("failed", LaneEnd::Failed("no such file".into())),
                ("stopped", LaneEnd::Stopped),
            ] {
                events.push(Event::SubagentEnded { id: id.into(), end });
            }
            feed(cx, tab, |t, cx| t.shot_feed(events, cx));
            let _ = super::frame_fresh(cx, window);
            let statuses: Vec<LaneStatus> = lanes_of(cx, tab).into_iter().map(|l| l.2).collect();
            assert_eq!(
                statuses,
                [
                    LaneStatus::Working,
                    LaneStatus::Waiting,
                    LaneStatus::Done,
                    LaneStatus::Failed("no such file".into()),
                    LaneStatus::Stopped
                ],
                "{n}"
            );
            let _ = part(cx, tab, n, "strip");
        },
    );
}

/// The chat switched to a subagent: its header, what Main asked it ("from Main"), its steps, and
/// how it ended — done (with Back to Main), failed (its reason), stopped; a working one has no
/// end line. The picture: the done lane.
pub(super) fn agent_lanes_view() {
    scene_sized("agent-lanes-view", (1100.0, 640.0), |n, cx, window, tab| {
        feed(cx, tab, |t, cx| {
            t.send("Read a.txt and b.txt with two subagents".into(), cx)
        });
        let mut events = decode(SUBAGENTS);
        events.extend(started("failed", "Check the build"));
        events.push(Event::SubagentEnded {
            id: "failed".into(),
            end: LaneEnd::Failed("no such file".into()),
        });
        events.extend(started("stopped", "Watch the tests"));
        events.push(Event::SubagentEnded {
            id: "stopped".into(),
            end: LaneEnd::Stopped,
        });
        events.extend(started("working", "Lint the code"));
        feed(cx, tab, |t, cx| t.shot_feed(events, cx));
        let lanes = lanes_of(cx, tab);
        let has = |cx: &mut VisualTestAppContext, name: &str| {
            cx.read(|cx| tab.read(cx).shot_part_now(name))
        };
        for (id, end) in [
            ("working", false),
            ("failed", true),
            ("stopped", true),
            (lanes[0].0.as_str(), true),
        ] {
            let to = View::Lane(id.to_string());
            cx.read(|cx| {
                tab.read(cx)
                    .shot_forget(&["lane-header", "lane-task", "lane-end"])
            });
            feed(cx, tab, |t, cx| t.show_view(to.clone(), cx));
            let _ = super::frame_fresh(cx, window);
            assert!(has(cx, "lane-header"), "{n}: {id}'s header");
            assert!(has(cx, "lane-task"), "{n}: {id}: from Main");
            assert_eq!(has(cx, "lane-end"), end, "{n}: {id}'s end line");
        }
        let header = part(cx, tab, n, "lane-header");
        let task = part(cx, tab, n, "lane-task");
        let end = part(cx, tab, n, "lane-end");
        assert!(
            header.bottom() <= task.top(),
            "{n}: the header over the list"
        );
        assert!(
            task.bottom() <= end.top(),
            "{n}: from Main first, the end last"
        );
    });
}
