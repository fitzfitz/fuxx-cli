//! M11ar's scenes: the subagent pill strip, and the chat switched to a subagent.

use gpui::{Entity, VisualTestAppContext, px};
use std::time::Duration;

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

/// The lanes chore's scenes (resume, relay, the closing bar): the box too.
const LANES_CHORE: &[&str] = &[
    "src/agent_lanes_view.rs",
    "src/agent_lanes.rs",
    "src/agent_tab.rs",
    "src/agent_box.rs",
    "src/agent_rows.rs",
    "src/agent_chat.rs",
    "src/agent_conversation.rs",
    "src/composer.rs",
    "src/providers/claude.rs",
    "src/providers/fixtures/claude-subagent-resume.jsonl",
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
        name: "agent-lanes-ask",
        run: agent_lanes_ask,
        themes: Themes::Both,
        serial: false,
        kind: Kind::State,
        sources: SOURCES,
    },
    Scene {
        name: "agent-lanes-narrow",
        run: agent_lanes_narrow,
        themes: Themes::Both,
        serial: false,
        kind: Kind::State,
        sources: SOURCES,
    },
    Scene {
        name: "agent-lanes-agy",
        run: agent_lanes_agy,
        themes: Themes::Both,
        serial: false,
        kind: Kind::State,
        sources: &[
            "src/agent_lanes_view.rs",
            "src/agent_tab.rs",
            "src/providers/agy.rs",
            "src/providers/fixtures/agy-subagents.jsonl",
            "src/providers/fixtures/agy-subagents-transcript-a.jsonl",
            "src/providers/fixtures/agy-subagents-transcript-b.jsonl",
        ],
    },
    Scene {
        name: "agent-lanes-background",
        run: agent_lanes_background,
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
    Scene {
        name: "agent-lanes-resumed",
        run: agent_lanes_resumed,
        themes: Themes::Both,
        serial: false,
        kind: Kind::State,
        sources: LANES_CHORE,
    },
    Scene {
        name: "agent-lanes-relay",
        run: agent_lanes_relay,
        themes: Themes::Both,
        serial: false,
        kind: Kind::State,
        sources: LANES_CHORE,
    },
    Scene {
        name: "agent-lanes-relay-narrow",
        run: agent_lanes_relay_narrow,
        themes: Themes::Both,
        serial: false,
        kind: Kind::State,
        sources: LANES_CHORE,
    },
    Scene {
        name: "agent-lanes-closing-bar",
        run: agent_lanes_closing_bar,
        themes: Themes::Both,
        serial: false,
        kind: Kind::State,
        sources: LANES_CHORE,
    },
    Scene {
        name: "agent-lanes-closing-bar-failed",
        run: agent_lanes_closing_bar_failed,
        themes: Themes::Both,
        serial: false,
        kind: Kind::State,
        sources: LANES_CHORE,
    },
    Scene {
        name: "agent-lanes-closing-bar-stopped",
        run: agent_lanes_closing_bar_stopped,
        themes: Themes::Both,
        serial: false,
        kind: Kind::State,
        sources: LANES_CHORE,
    },
    Scene {
        name: "agent-lanes-closing-bar-ended",
        run: agent_lanes_closing_bar_ended,
        themes: Themes::Both,
        serial: false,
        kind: Kind::State,
        sources: LANES_CHORE,
    },
    Scene {
        name: "agent-lanes-closing-bar-foreground",
        run: agent_lanes_closing_bar_foreground,
        themes: Themes::Both,
        serial: false,
        kind: Kind::State,
        sources: LANES_CHORE,
    },
    Scene {
        name: "agent-lanes-bar-narrow",
        run: agent_lanes_bar_narrow,
        themes: Themes::Both,
        serial: false,
        kind: Kind::State,
        sources: LANES_CHORE,
    },
    Scene {
        name: "agent-lanes-bar-typing",
        run: agent_lanes_bar_typing,
        themes: Themes::Both,
        serial: false,
        kind: Kind::State,
        sources: LANES_CHORE,
    },
    Scene {
        name: "agent-lanes-bar-about",
        run: agent_lanes_bar_about,
        themes: Themes::Both,
        serial: false,
        kind: Kind::State,
        sources: LANES_CHORE,
    },
    Scene {
        name: "agent-lanes-finish-mid-draft",
        run: agent_lanes_finish_mid_draft,
        themes: Themes::Both,
        serial: false,
        kind: Kind::State,
        sources: LANES_CHORE,
    },
    Scene {
        name: "agent-lanes-list-grouped",
        run: agent_lanes_list_grouped,
        themes: Themes::Both,
        serial: false,
        kind: Kind::State,
        sources: LANES_CHORE,
    },
    Scene {
        name: "agent-lanes-relay-history",
        run: agent_lanes_relay_history,
        themes: Themes::Both,
        serial: false,
        kind: Kind::State,
        sources: LANES_CHORE,
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
            controls: LaneControls {
                stop: true,
                message: true,
                woken_by_steps: false,
            },
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
                "{n}: +N in the strip ({more:?} in {strip:?})"
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

const SUB_ASK: &str = include_str!("../providers/fixtures/claude-subagent-ask.jsonl");

/// A subagent asks while you look at another: the card shows in the shown view (Enter answers
/// it there), headed by who asks; the asking lane's pill is tinted, Main reads "Needs input".
pub(super) fn agent_lanes_ask() {
    scene_sized("agent-lanes-ask", (1100.0, 640.0), |n, cx, window, tab| {
        feed(cx, tab, |t, cx| {
            t.send("Make hello.txt with a subagent".into(), cx)
        });
        let mut events = decode(SUB_ASK);
        let ask = events
            .iter()
            .position(|e| matches!(e, Event::Permission { .. }))
            .expect("the subagent asks");
        events.truncate(ask + 1);
        events.extend(started("other", "Lint the code"));
        feed(cx, tab, |t, cx| t.shot_feed(events, cx));
        let lanes = lanes_of(cx, tab);
        assert_eq!(lanes[0].2, LaneStatus::Waiting, "{n}: the asking lane");
        assert_eq!(
            cx.read(|cx| tab.read(cx).status()),
            crate::status::Status::Waiting,
            "{n}: Main needs input"
        );
        feed(cx, tab, |t, cx| t.show_view(View::Lane("other".into()), cx));
        let rows = cx.read(|cx| tab.read(cx).shot_list().item_count());
        // from Main, the question (no steps of its own yet).
        assert_eq!(rows, 2, "{n}: the question shows in the other lane's view");
        let _ = super::frame_fresh(cx, window);
        let _ = part(cx, tab, n, "strip");
    });
}

/// A 420 pt tab: labels cut short, the lane header keeps its time and Stop; and sending from a
/// Claude lane running in the background is a relay (the lanes chore): it stays in the lane.
pub(super) fn agent_lanes_narrow() {
    scene_sized(
        "agent-lanes-narrow",
        (420.0, 640.0),
        |n, cx, window, tab| {
            feed(cx, tab, |t, cx| t.send("Fan out".into(), cx));
            let mut events = started("one", "Find the config loader");
            events.extend(started("two", "Run the whole test suite"));
            events.push(Event::SubagentProgress {
                id: "one".into(),
                tokens: Some(18_200),
                tools: Some(12),
                last_tool: Some("Grep".into()),
            });
            feed(cx, tab, |t, cx| t.shot_feed(events, cx));
            feed(cx, tab, |t, cx| t.show_view(View::Lane("one".into()), cx));
            let placeholder = cx.read(|cx| {
                tab.read(cx)
                    .shot_composer()
                    .read(cx)
                    .placeholder()
                    .to_string()
            });
            // Under 480 pt the placeholder is short (the lanes review).
            assert_eq!(
                placeholder, "Message Find the config loader via Main",
                "{n}"
            );
            let _ = super::frame_fresh(cx, window);
            cx.simulate_input(window, "and the docs too");
            cx.simulate_keystrokes(window, "enter");
            cx.run_until_parked();
            assert_eq!(
                cx.read(|cx| tab.read(cx).view.clone()),
                View::Lane("one".into()),
                "{n}: a relay keeps you in the lane"
            );
            assert!(
                written(cx, tab).iter().any(|l| l.contains("[fuxx relay]")),
                "{n}: sent as a relay"
            );
            let _ = super::frame_fresh(cx, window);
            let _ = part(cx, tab, n, "lane-stop");
            let header = part(cx, tab, n, "lane-header");
            assert!(
                f32::from(header.size.width) <= 420.0,
                "{n}: the header fits ({header:?})"
            );
        },
    );
}

/// agy's two subagents (its recording, through the tab's own translator), then their
/// transcripts as the follower reads them: two pills, each lane's steps, both done — and no
/// Stop (agy can't stop one alone).
pub(super) fn agent_lanes_agy() {
    use super::agent::{Slot, build_with, start, tab_of};
    use crate::agent_process::ProcessEvent;
    use crate::providers::agy::AGY;
    use std::rc::Rc;
    const SUBS: &str = include_str!("../providers/fixtures/agy-subagents.jsonl");
    const A: &str = include_str!("../providers/fixtures/agy-subagents-transcript-a.jsonl");
    const B: &str = include_str!("../providers/fixtures/agy-subagents-transcript-b.jsonl");
    let name = "agent-lanes-agy";
    for &(slug, theme_name) in super::shot_themes() {
        let theme = super::shot_theme(theme_name);
        let slot: Slot = Rc::default();
        let keep = slot.clone();
        let n = format!("{name}-{slug}");
        let path = super::shoot_with(
            name,
            slug,
            &theme,
            gpui::size(px(1100.0), px(640.0)),
            build_with(&AGY, theme, slot, start()),
            move |cx, window| {
                still(cx);
                let tab = tab_of(&keep);
                feed(cx, &tab, |t, cx| t.send("Read a.txt and b.txt".into(), cx));
                let lines: Vec<&str> = SUBS.lines().collect();
                let (body, last) = lines.split_at(lines.len() - 1);
                for l in body {
                    let line = l.to_string();
                    feed(cx, &tab, |t, cx| {
                        t.shot_process(ProcessEvent::Line(line), cx)
                    });
                }
                let lanes = lanes_of(cx, &tab);
                assert_eq!(lanes.len(), 2, "{n}");
                for ((id, _, _), file) in lanes.iter().zip([A, B]) {
                    let lines: Vec<String> = file.lines().map(str::to_string).collect();
                    feed(cx, &tab, |t, cx| t.shot_followed(id, lines, cx));
                }
                let line = last[0].to_string();
                feed(cx, &tab, |t, cx| {
                    t.shot_process(ProcessEvent::Line(line), cx)
                });
                let lanes = lanes_of(cx, &tab);
                assert!(
                    lanes.iter().all(|l| l.2 == LaneStatus::Done),
                    "{n}: both reported: {lanes:?}"
                );
                let first = lanes[0].0.clone();
                let steps = cx.read(|cx| tab.read(cx).shot_lane_steps(&first));
                assert_eq!(steps, 2, "{n}: read_file, send_message");
                feed(cx, &tab, |t, cx| t.show_view(View::Lane(first.clone()), cx));
                // The user's rule, for agy too: a finished lane gets the closing bar, never a
                // box to Main (the lanes review, fix 1).
                assert_eq!(
                    mode_of(cx, &tab),
                    crate::agent_tab::BoxMode::Bar(crate::agent_lanes::Bar::Finished(
                        LaneStatus::Done
                    )),
                    "{n}"
                );
                cx.read(|cx| tab.read(cx).shot_forget(&["lane-stop", "closing-bar"]));
                let _ = super::frame_fresh(cx, window);
                assert!(
                    !cx.read(|cx| tab.read(cx).shot_part_now("lane-stop")),
                    "{n}: no Stop for agy"
                );
                assert!(
                    cx.read(|cx| tab.read(cx).shot_part_now("closing-bar")),
                    "{n}: the closing bar"
                );
                keep.borrow_mut().take();
            },
        );
        println!("ui-shots: {}", path.display());
    }
}

/// Main finished but a subagent runs on (your report, 2026-11-22: "it says done … but in
/// reality there are subagents running"): the tab shows Working and the pill "1 subagent
/// running" until it ends; then Done.
pub(super) fn agent_lanes_background() {
    use crate::status::Status;
    scene_sized(
        "agent-lanes-background",
        (1100.0, 640.0),
        |n, cx, window, tab| {
            feed(cx, tab, |t, cx| {
                t.send("Review it in the background".into(), cx)
            });
            let mut events = started("bg", "Review the branch");
            events.push(Event::TurnDone {
                ok: true,
                message: None,
            });
            feed(cx, tab, |t, cx| t.shot_feed(events, cx));
            let shown = |cx: &mut VisualTestAppContext| {
                cx.read(|cx| (tab.read(cx).status(), tab.read(cx).shown_status()))
            };
            assert_eq!(
                shown(cx),
                (Status::Done, Status::Working),
                "{n}: Main done, the tab still working"
            );
            // Ended: Done everywhere (checked, then back to the running picture).
            feed(cx, tab, |t, cx| {
                t.shot_feed(
                    vec![Event::SubagentEnded {
                        id: "bg".into(),
                        end: LaneEnd::Done,
                    }],
                    cx,
                )
            });
            assert_eq!(shown(cx), (Status::Done, Status::Done), "{n}: all finished");
            // Main starts another in its turn, and finishes: "Orchestrating…" (your words, 2026-11-22; renamed 2026-11-24).
            let mut again = started("bg2", "Watch the tests");
            again.push(Event::TurnDone {
                ok: true,
                message: None,
            });
            feed(cx, tab, |t, cx| t.shot_feed(again, cx));
            let _ = super::frame_fresh(cx, window);
            assert_eq!(shown(cx), (Status::Done, Status::Working), "{n}");
        },
    );
}

// ---- The lanes chore (2026-11-24): resume, relay, the closing bar ----

const SUB_RESUME: &str = include_str!("../providers/fixtures/claude-subagent-resume.jsonl");

/// What the tab wrote to its agent (a scene tab has no process).
fn written(cx: &VisualTestAppContext, tab: &Entity<AgentTab>) -> Vec<String> {
    cx.read(|cx| tab.read(cx).shot_written())
}

/// The box's mode now.
fn mode_of(cx: &VisualTestAppContext, tab: &Entity<AgentTab>) -> crate::agent_tab::BoxMode {
    cx.read(|cx| tab.read(cx).box_mode(cx))
}

/// The box's words now.
fn box_text(cx: &VisualTestAppContext, tab: &Entity<AgentTab>) -> String {
    cx.read(|cx| tab.read(cx).shot_composer().read(cx).text().to_string())
}

/// `text`'s lines up to (not with) the first that `stop` picks, through the tab's own
/// translator (as its process would bring them).
fn lines_until(
    cx: &mut VisualTestAppContext,
    tab: &Entity<AgentTab>,
    text: &str,
    stop: impl Fn(&str) -> bool,
) {
    use crate::agent_process::ProcessEvent;
    for l in text.lines().take_while(|l| !stop(l)) {
        let line = l.to_string();
        feed(cx, tab, |t, cx| {
            t.shot_process(ProcessEvent::Line(line), cx)
        });
    }
}

/// Still pictures: no fades or flashes mid-shot.
fn still(cx: &mut VisualTestAppContext) {
    cx.update(|cx| cx.set_reduce_motion(true));
}

/// A Claude subagent "Explore" running in the background, Main's turn still going: the
/// resume fixture up to its first report, through the tab's translator.
fn explore_running(cx: &mut VisualTestAppContext, tab: &Entity<AgentTab>) {
    feed(cx, tab, |t, cx| t.send("Explore the test setup".into(), cx));
    // Main read it (its echo).
    feed(cx, tab, |t, cx| {
        t.shot_feed(vec![Event::Taken { id: None }], cx)
    });
    lines_until(cx, tab, SUB_RESUME, |l| l.contains("\"task_updated\""));
}

/// Claude's subagent resumed by Main's SendMessage (the stuck lanes' fix): one lane — its
/// first run's end a divider "✓ Done in … · resumed by Main", what Main said ("from Main"),
/// the second run's step — and it ends Done, nothing spinning.
pub(super) fn agent_lanes_resumed() {
    scene_sized(
        "agent-lanes-resumed",
        (1100.0, 640.0),
        |n, cx, window, tab| {
            still(cx);
            feed(cx, tab, |t, cx| t.send("Explore the test setup".into(), cx));
            lines_until(cx, tab, SUB_RESUME, |_| false);
            let lanes = lanes_of(cx, tab);
            assert_eq!(
                lanes.len(),
                1,
                "{n}: one lane, no \u{201c}Explore 2\u{201d}"
            );
            assert_eq!(lanes[0].2, LaneStatus::Done, "{n}: closed by its own end");
            assert_eq!(
                cx.read(|cx| tab.read(cx).shot_lane_steps("toolu_agent1")),
                2,
                "{n}: both runs' steps"
            );
            feed(cx, tab, |t, cx| {
                t.show_view(View::Lane("toolu_agent1".into()), cx)
            });
            let _ = super::frame_fresh(cx, window);
            let divider = part(cx, tab, n, "lane-resumed");
            let end = part(cx, tab, n, "lane-end");
            assert!(
                divider.bottom() <= end.top(),
                "{n}: the divider before the end"
            );
        },
    );
}

/// A relay (the lanes chore): in a background Claude lane the box says "To Explore · via
/// Main"; Enter sends your words to Main framed with the subagent's agent id and keeps you in
/// the lane. Three of them show its captions: delivered ✓, not passed on (Send again), and
/// held while Main works.
pub(super) fn agent_lanes_relay() {
    scene_sized(
        "agent-lanes-relay",
        (1100.0, 640.0),
        |n, cx, window, tab| {
            still(cx);
            explore_running(cx, tab);
            let lane = View::Lane("toolu_agent1".into());
            feed(cx, tab, |t, cx| t.show_view(lane.clone(), cx));
            assert_eq!(
                mode_of(cx, tab),
                crate::agent_tab::BoxMode::Relay {
                    images: false,
                    ready: true
                },
                "{n}"
            );
            let _ = super::frame_fresh(cx, window);
            let _ = part(cx, tab, n, "relay-target");
            let relay = |cx: &mut VisualTestAppContext, words: &str| {
                cx.simulate_input(window, words);
                cx.simulate_keystrokes(window, "enter");
                cx.run_until_parked();
            };
            // One delivered: Main read it and passed it on.
            relay(cx, "Also check the docs folder");
            assert_eq!(
                cx.read(|cx| tab.read(cx).view.clone()),
                lane,
                "{n}: still in the lane"
            );
            let frame = written(cx, tab)
                .into_iter()
                .rfind(|l| l.contains("[fuxx relay]"))
                .expect("a relay written");
            assert!(
                frame.contains("a7c41e2b9d"),
                "{n}: named by its agent id: {frame}"
            );
            assert!(
                box_text(cx, &tab.clone()).is_empty(),
                "{n}: the box cleared"
            );
            feed(cx, tab, |t, cx| {
                t.shot_feed(
                    vec![
                        Event::Taken { id: None },
                        Event::PassedOn {
                            call: "toolu_send9".into(),
                            lane: "toolu_agent1".into(),
                        },
                        Event::TurnDone {
                            ok: true,
                            message: None,
                        },
                    ],
                    cx,
                )
            });
            // One Main read and didn't pass on.
            relay(cx, "Skip the vendored code");
            feed(cx, tab, |t, cx| {
                t.shot_feed(
                    vec![
                        Event::Taken { id: None },
                        Event::TurnDone {
                            ok: true,
                            message: None,
                        },
                    ],
                    cx,
                )
            });
            // One held: Main hasn't read it yet.
            relay(cx, "And list what you skipped");
            let states: Vec<(crate::agent_conversation::RelayState, bool)> = cx.read(|cx| {
                tab.read(cx)
                    .shot_items()
                    .iter()
                    .filter_map(|i| match i {
                        crate::agent_conversation::Item::You {
                            to: Some(r), taken, ..
                        } => Some((r.state.clone(), *taken)),
                        _ => None,
                    })
                    .collect()
            });
            use crate::agent_conversation::RelayState;
            assert_eq!(
                states,
                [
                    (RelayState::Delivered, true),
                    (RelayState::NotPassed, true),
                    (RelayState::Sent, false)
                ],
                "{n}"
            );
            let _ = super::frame_fresh(cx, window);
            let _ = part(cx, tab, n, "relay-caption");
        },
    );
}

/// The relay box under 480 pt: the target pill says only "To Explore", the placeholder
/// "Message Explore via Main".
pub(super) fn agent_lanes_relay_narrow() {
    scene_sized(
        "agent-lanes-relay-narrow",
        (420.0, 640.0),
        |n, cx, window, tab| {
            still(cx);
            explore_running(cx, tab);
            feed(cx, tab, |t, cx| {
                t.show_view(View::Lane("toolu_agent1".into()), cx)
            });
            let _ = super::frame_fresh(cx, window);
            let pill = part(cx, tab, n, "relay-target");
            assert!(
                f32::from(pill.size.width) < 130.0,
                "{n}: \u{201c}To Explore\u{201d} alone ({pill:?})"
            );
            let placeholder = cx.read(|cx| {
                tab.read(cx)
                    .shot_composer()
                    .read(cx)
                    .placeholder()
                    .to_string()
            });
            assert_eq!(placeholder, "Message Explore via Main", "{n}");
        },
    );
}

/// A Claude lane, then `end` (none: still running in the foreground), Main's turn done.
fn lane_then(
    cx: &mut VisualTestAppContext,
    tab: &Entity<AgentTab>,
    end: Option<LaneEnd>,
    process_ended: bool,
) {
    feed(cx, tab, |t, cx| t.send("Explore the test setup".into(), cx));
    let mut events = started("lane", "Explore");
    if let Some(Event::SubagentStarted { controls, task, .. }) = events.last_mut() {
        controls.message = end.is_some() || process_ended;
        *task = "Read the test setup".into();
    }
    events.push(Event::SubagentProgress {
        id: "lane".into(),
        tokens: Some(18_200),
        tools: Some(4),
        last_tool: Some("Read".into()),
    });
    if let Some(end) = end {
        events.push(Event::SubagentEnded {
            id: "lane".into(),
            end,
        });
        events.push(Event::TurnDone {
            ok: true,
            message: None,
        });
    }
    feed(cx, tab, |t, cx| t.shot_feed(events, cx));
    if process_ended {
        feed(cx, tab, |t, cx| {
            t.shot_process(
                crate::agent_process::ProcessEvent::Exited {
                    code: Some(0),
                    stderr_tail: String::new(),
                },
                cx,
            )
        });
    }
}

/// The closing bar in place of the box's contents, its height the box's own.
/// A lane's closing bar (`took`: its clock frozen there, so the scene checks a real time).
fn closing_bar(name: &str, end: Option<LaneEnd>, ended: bool, width: f32) {
    closing_bar_took(name, end, ended, width, None);
}

fn closing_bar_took(
    name: &str,
    end: Option<LaneEnd>,
    ended: bool,
    width: f32,
    took: Option<Duration>,
) {
    scene_sized(name, (width, 640.0), move |n, cx, window, tab| {
        still(cx);
        let _ = super::frame_fresh(cx, window);
        let empty_box = part(cx, tab, n, "box");
        lane_then(cx, tab, end.clone(), ended);
        if let Some(took) = took {
            feed(cx, tab, |t, _| t.shot_lane_took("lane", took));
            let shown = cx.read(|cx| tab.read(cx).lane_elapsed("lane"));
            assert_eq!(shown, Some(took), "{n}: the clock frozen");
        }
        feed(cx, tab, |t, cx| t.show_view(View::Lane("lane".into()), cx));
        assert!(
            matches!(mode_of(cx, tab), crate::agent_tab::BoxMode::Bar(_)),
            "{n}: {:?}",
            mode_of(cx, tab)
        );
        cx.read(|cx| tab.read(cx).shot_forget(&["box", "closing-bar"]));
        let _ = super::frame_fresh(cx, window);
        let bar = part(cx, tab, n, "closing-bar");
        let the_box = part(cx, tab, n, "box");
        assert!(
            (f32::from(the_box.size.height) - f32::from(empty_box.size.height)).abs() < 0.5,
            "{n}: the bar keeps the box's height ({the_box:?} vs {empty_box:?})"
        );
        assert!(bar.top() >= the_box.top(), "{n}: in the box");
        // Enter is Back to Main.
        cx.simulate_keystrokes(window, "enter");
        cx.run_until_parked();
        assert_eq!(
            cx.read(|cx| tab.read(cx).view.clone()),
            View::Main,
            "{n}: ⏎"
        );
        feed(cx, tab, |t, cx| t.show_view(View::Lane("lane".into()), cx));
        let _ = super::frame_fresh(cx, window);
    });
}

/// A done lane's closing bar: "Explore finished in 1:33" (its clock frozen there, so the
/// time display is checked), Ask Main about it, Back to Main ⏎.
pub(super) fn agent_lanes_closing_bar() {
    closing_bar_took(
        "agent-lanes-closing-bar",
        Some(LaneEnd::Done),
        false,
        1100.0,
        Some(Duration::from_secs(93)),
    );
}

pub(super) fn agent_lanes_closing_bar_failed() {
    closing_bar(
        "agent-lanes-closing-bar-failed",
        Some(LaneEnd::Failed("no such file: tests/setup.rs".into())),
        false,
        1100.0,
    );
}

pub(super) fn agent_lanes_closing_bar_stopped() {
    closing_bar(
        "agent-lanes-closing-bar-stopped",
        Some(LaneEnd::Stopped),
        false,
        1100.0,
    );
}

/// The agent exited with the lane still going: it ended without a report (a ring, no spinner).
pub(super) fn agent_lanes_closing_bar_ended() {
    closing_bar("agent-lanes-closing-bar-ended", None, true, 1100.0);
}

/// A live lane in the foreground: "Explore is working · Main is waiting on it, so messages
/// can't reach it yet".
pub(super) fn agent_lanes_closing_bar_foreground() {
    closing_bar("agent-lanes-closing-bar-foreground", None, false, 1100.0);
}

/// The closing bar under 480 pt: no "or start typing", no "Back to Main ⏎" words.
pub(super) fn agent_lanes_bar_narrow() {
    closing_bar("agent-lanes-bar-narrow", Some(LaneEnd::Done), false, 420.0);
}

/// Typing in a finished lane's bar: the bar becomes the box in place, the letters carried,
/// "To Main · about Explore".
pub(super) fn agent_lanes_bar_typing() {
    scene_sized(
        "agent-lanes-bar-typing",
        (1100.0, 640.0),
        |n, cx, window, tab| {
            still(cx);
            lane_then(cx, tab, Some(LaneEnd::Done), false);
            feed(cx, tab, |t, cx| t.show_view(View::Lane("lane".into()), cx));
            let _ = super::frame_fresh(cx, window);
            cx.simulate_input(window, "What did it find?");
            cx.run_until_parked();
            assert_eq!(
                mode_of(cx, tab),
                crate::agent_tab::BoxMode::About { live: false },
                "{n}"
            );
            assert_eq!(box_text(cx, tab), "What did it find?", "{n}: carried");
            let _ = super::frame_fresh(cx, window);
            let _ = part(cx, tab, n, "relay-target");
            // ⌫ in the emptied box brings the bar back.
            feed(cx, tab, |t, cx| {
                t.shot_composer()
                    .update(cx, |c, cx| c.set_text(String::new(), cx))
            });
            cx.simulate_keystrokes(window, "backspace");
            cx.run_until_parked();
            assert!(
                matches!(mode_of(cx, tab), crate::agent_tab::BoxMode::Bar(_)),
                "{n}: back to the bar"
            );
            cx.simulate_input(window, "What did it find?");
            cx.run_until_parked();
            let _ = super::frame_fresh(cx, window);
        },
    );
}

/// Enter in the opened bar: the message goes to Main about the lane (never resuming it), and
/// the chat switches to Main, the bubble led by "About Explore".
pub(super) fn agent_lanes_bar_about() {
    scene_sized(
        "agent-lanes-bar-about",
        (1100.0, 640.0),
        |n, cx, window, tab| {
            still(cx);
            lane_then(cx, tab, Some(LaneEnd::Done), false);
            feed(cx, tab, |t, cx| t.show_view(View::Lane("lane".into()), cx));
            let _ = super::frame_fresh(cx, window);
            cx.simulate_input(window, "What did it find?");
            cx.run_until_parked();
            cx.simulate_keystrokes(window, "enter");
            cx.run_until_parked();
            assert_eq!(cx.read(|cx| tab.read(cx).view.clone()), View::Main, "{n}");
            let about = written(cx, tab)
                .into_iter()
                .rfind(|l| l.contains("[fuxx about]"))
                .expect("an about message");
            assert!(about.contains("don't resume it"), "{n}: {about}");
            assert!(
                !written(cx, tab).iter().any(|l| l.contains("[fuxx relay]")),
                "{n}: never a relay to a finished lane"
            );
            let _ = super::frame_fresh(cx, window);
            let _ = part(cx, tab, n, "relay-chip");
        },
    );
}

/// A lane that finishes while you type to it: the box stays (never swapped under your
/// fingers), its pill now "To Main · about Explore"; an Enter right after does nothing and the
/// draft is kept.
pub(super) fn agent_lanes_finish_mid_draft() {
    scene_sized(
        "agent-lanes-finish-mid-draft",
        (1100.0, 640.0),
        |n, cx, window, tab| {
            still(cx);
            explore_running(cx, tab);
            feed(cx, tab, |t, cx| {
                t.show_view(View::Lane("toolu_agent1".into()), cx)
            });
            let _ = super::frame_fresh(cx, window);
            cx.simulate_input(window, "and the fixtures too");
            cx.run_until_parked();
            // The rest of its first run: its end.
            let rest: String = SUB_RESUME
                .lines()
                .skip_while(|l| !l.contains("\"task_updated\""))
                .collect::<Vec<_>>()
                .join("\n");
            lines_until(cx, tab, &rest, |l| l.contains("\"result\""));
            assert_eq!(lanes_of(cx, tab)[0].2, LaneStatus::Done, "{n}");
            assert_eq!(
                mode_of(cx, tab),
                crate::agent_tab::BoxMode::About { live: false },
                "{n}: the box kept, retargeted"
            );
            let before = written(cx, tab).len();
            cx.simulate_keystrokes(window, "enter");
            cx.run_until_parked();
            assert_eq!(
                written(cx, tab).len(),
                before,
                "{n}: Enter within 600 ms sends nothing"
            );
            assert_eq!(
                box_text(cx, tab),
                "and the fixtures too",
                "{n}: the draft kept"
            );
            assert_eq!(
                cx.read(|cx| tab.read(cx).view.clone()),
                View::Lane("toolu_agent1".into()),
                "{n}"
            );
            let _ = super::frame_fresh(cx, window);
            let _ = part(cx, tab, n, "relay-target");
        },
    );
}

/// The "+N" list with running and finished lanes: grouped, each newest first; "+N" carries a
/// live mark. A lane finishing while it is open keeps its row.
pub(super) fn agent_lanes_list_grouped() {
    scene_sized(
        "agent-lanes-list-grouped",
        (720.0, 640.0),
        |n, cx, window, tab| {
            still(cx);
            feed(cx, tab, |t, cx| t.send("Fan out".into(), cx));
            let mut events: Vec<Event> = (1..=8)
                .flat_map(|i| started(&format!("agent{i}"), &format!("Check module {i}")))
                .collect();
            for i in [1, 2, 4] {
                events.push(Event::SubagentEnded {
                    id: format!("agent{i}"),
                    end: LaneEnd::Done,
                });
            }
            feed(cx, tab, |t, cx| t.shot_feed(events, cx));
            let _ = super::frame_fresh(cx, window);
            feed(cx, tab, |t, cx| t.shot_toggle_lanes_menu(cx));
            // One finishes while it is open: its row stays where it was.
            feed(cx, tab, |t, cx| {
                t.shot_feed(
                    vec![Event::SubagentEnded {
                        id: "agent3".into(),
                        end: LaneEnd::Done,
                    }],
                    cx,
                )
            });
            let _ = super::frame_fresh(cx, window);
            let _ = part(cx, tab, n, "lanes-menu");
        },
    );
}

/// A resumed conversation's relays: your words with "→ Explore" (and "About Explore"), Main's
/// SendMessage as "Passed on to Explore" — never the frame round them.
pub(super) fn agent_lanes_relay_history() {
    use crate::providers::claude::{about_frame, relay_frame, transcript_events};
    use serde_json::json;
    scene_sized(
        "agent-lanes-relay-history",
        (1100.0, 640.0),
        |n, cx, window, tab| {
            still(cx);
            let lines = [
                json!({"type":"user","uuid":"u0","message":{"content":"Explore the test setup"}}),
                json!({"type":"assistant","uuid":"a1","message":{"id":"m1","content":[
                    {"type":"tool_use","id":"toolu_agent1","name":"Agent",
                     "input":{"description":"Explore","subagent_type":"Explore",
                              "prompt":"Read the test setup"}}]}}),
                json!({"type":"user","uuid":"r1","message":{"content":[
                    {"type":"tool_result","tool_use_id":"toolu_agent1",
                     "content":"Async agent launched successfully.\nagentId: a7c41e2b9d"}]}}),
                json!({"type":"user","uuid":"u1","message":{"content":
                    relay_frame("Explore", Some("a7c41e2b9d"), "Also check the docs folder")}}),
                json!({"type":"assistant","uuid":"a2","message":{"id":"m2","content":[
                    {"type":"tool_use","id":"toolu_send1","name":"SendMessage",
                     "input":{"to":"a7c41e2b9d","message":"Also check the docs folder"}}]}}),
                json!({"type":"user","uuid":"r2","message":{"content":[
                    {"type":"tool_result","tool_use_id":"toolu_send1","content":"Sent."}]}}),
                json!({"type":"user","uuid":"u2","message":{"content":
                    about_frame("Explore", Some("a7c41e2b9d"), "What did it find?", false)}}),
                json!({"type":"assistant","uuid":"a3","message":{"id":"m3","content":[
                    {"type":"text","text":"It found one setup function in tests/setup.rs."}]}}),
            ];
            let text = lines
                .iter()
                .map(|v| v.to_string())
                .collect::<Vec<_>>()
                .join("\n");
            let past = transcript_events(&text);
            feed(cx, tab, |t, cx| t.shot_history(past, cx));
            let shown_frame = cx.read(|cx| {
                tab.read(cx).shot_items().iter().any(|i| {
                    matches!(i, crate::agent_conversation::Item::You { text, .. }
                        if text.contains("[fuxx"))
                })
            });
            assert!(!shown_frame, "{n}: the frame is never shown");
            let _ = super::frame_fresh(cx, window);
            let _ = part(cx, tab, n, "relay-chip");
        },
    );
}
