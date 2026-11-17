//! M11ak: the agent box's scenes. As the agent tab's (`agent.rs`), no process runs: the tab is
//! made with `AgentTab::for_shots`, fed decoded events through `shot_feed`, and what it would
//! write to its agent is read back with `shot_written`.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use gpui::{AppContext, Context, Entity, VisualTestAppContext, px, size};

use super::{Kind, Scene, Themes, render_with, shot_appearance, shot_theme};
use crate::agent_box::Change;
use crate::agent_conversation::Item;
use crate::agent_tab::{AgentTab, AgentTabEvent};
use crate::providers::agy::AGY;
use crate::providers::claude::CLAUDE;
use crate::providers::{ChangeKind, Event, Provider, Start};
use crate::status::Status;

/// This file, as `file!()` gives it: `registry::AREAS` pairs it with `SCENES`.
pub(super) const FILE: &str = file!();

/// The files a box change goes through: the tab, the box's rules, the conversation, the
/// providers that say how a change applies and write it.
const BOX: &[&str] = &[
    "src/agent_tab.rs",
    "src/agent_box.rs",
    "src/agent_conversation.rs",
    "src/providers/mod.rs",
    "src/providers/claude.rs",
    "src/providers/agy.rs",
    "src/providers/fixtures/agy-text.jsonl",
];

/// This area's scenes (`registry.rs` runs them in its `ORDER`).
pub(super) const SCENES: &[Scene] = &[
    Scene {
        name: "agent-box-change-waits",
        run: agent_box_change_waits,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: BOX,
    },
    Scene {
        name: "agent-box-change-restart",
        run: agent_box_change_restart,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: BOX,
    },
    Scene {
        name: "agent-box-restart-keeps-queued-text",
        run: agent_box_restart_keeps_queued_text,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: BOX,
    },
];

type Slot = Rc<RefCell<Option<Entity<AgentTab>>>>;
/// The settings the tab told (`AgentTabEvent::Settings`): model, effort, mode.
type Told = Rc<RefCell<Vec<(Option<String>, Option<String>, Option<String>)>>>;

/// A folder under your home (`~/code/my-app`); nothing is read from it.
fn cwd() -> PathBuf {
    crate::workspace::home_dir()
        .unwrap_or_else(|| PathBuf::from("/home/me"))
        .join("code/my-app")
}

fn start(mode: Option<&str>, model: Option<&str>) -> Start {
    Start {
        cwd: cwd(),
        resume: None,
        new_id: None,
        mode: mode.map(String::from),
        model: model.map(String::from),
        effort: None,
        extra_args: Vec::new(),
    }
}

fn feed(
    cx: &mut VisualTestAppContext,
    tab: &Entity<AgentTab>,
    f: impl FnOnce(&mut AgentTab, &mut Context<AgentTab>),
) {
    cx.update(|cx| tab.update(cx, f));
    cx.run_until_parked();
}

/// Checks only: a `provider` tab with no process, `start` given, its history in (its process
/// "started": `shot_last_argv` is what it would run); the settings it tells are kept.
fn run_scene(
    name: &str,
    provider: &'static dyn Provider,
    start: Start,
    act: impl FnOnce(&str, &mut VisualTestAppContext, &Entity<AgentTab>, &Told) + 'static,
) {
    let theme = shot_theme("Catppuccin Mocha");
    let slot: Slot = Rc::default();
    let keep = slot.clone();
    let told: Told = Rc::default();
    let heard = told.clone();
    let n = name.to_string();
    render_with(
        &theme,
        size(px(720.), px(640.)),
        move |window, cx| {
            cx.set_global(shot_appearance(theme, cx));
            let tab = cx.new(|cx| AgentTab::for_shots(provider, start, window, cx));
            cx.subscribe(&tab, move |_, e: &AgentTabEvent, _| {
                if let AgentTabEvent::Settings {
                    model,
                    effort,
                    mode,
                } = e
                {
                    heard
                        .borrow_mut()
                        .push((model.clone(), effort.clone(), mode.clone()));
                }
            })
            .detach();
            *keep.borrow_mut() = Some(tab.clone());
            tab.into()
        },
        move |cx, _| {
            let tab = slot.borrow().clone().expect("the agent tab");
            feed(cx, &tab, |t, cx| t.shot_history(Vec::new(), cx));
            act(&n, cx, &tab, &told);
            slot.borrow_mut().take();
        },
    );
}

fn written(cx: &VisualTestAppContext, tab: &Entity<AgentTab>) -> Vec<serde_json::Value> {
    cx.read(|cx| tab.read(cx).shot_written())
        .iter()
        .map(|l| serde_json::from_str(l).expect("a JSON line"))
        .collect()
}

fn argv(cx: &VisualTestAppContext, tab: &Entity<AgentTab>) -> Vec<String> {
    cx.read(|cx| tab.read(cx).shot_last_argv())
}

/// The word after `flag` in the last command line.
fn flag(argv: &[String], flag: &str) -> Option<String> {
    argv.windows(2).find(|w| w[0] == flag).map(|w| w[1].clone())
}

fn notices(cx: &VisualTestAppContext, tab: &Entity<AgentTab>) -> Vec<String> {
    cx.read(|cx| {
        tab.read(cx)
            .shot_items()
            .iter()
            .filter_map(|i| match i {
                Item::Notice(t) => Some(t.clone()),
                _ => None,
            })
            .collect()
    })
}

fn said(cx: &VisualTestAppContext, tab: &Entity<AgentTab>) -> Vec<String> {
    cx.read(|cx| {
        tab.read(cx)
            .shot_items()
            .iter()
            .filter_map(|i| match i {
                Item::You { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect()
    })
}

fn turn_done() -> Event {
    Event::TurnDone {
        ok: true,
        message: None,
    }
}

fn agy_text() -> Vec<Event> {
    let mut t = AGY.translator();
    include_str!("../providers/fixtures/agy-text.jsonl")
        .lines()
        .flat_map(|l| t.decode(l))
        .collect()
}

/// The conversation agy's text recording started.
fn agy_conversation() -> String {
    agy_text()
        .iter()
        .find_map(|e| match e {
            Event::Started { conversation, .. } => Some(conversation.clone()),
            _ => None,
        })
        .expect("agy-text starts a conversation")
}

/// Review Focus #1: a change chosen while Claude works waits for the turn — nothing is written
/// until `TurnDone`, then the set request once (never a message); the chip's start flags change
/// only when Claude confirms; a refusal is a notice and changes nothing. Idle, an effort alone
/// is asked as an effort, a model with an effort as both.
pub(super) fn agent_box_change_waits() {
    run_scene(
        "agent-box-change-waits",
        &CLAUDE,
        start(None, Some("opus")),
        |n, cx, tab, told| {
            feed(cx, tab, |t, cx| t.send("Fix notes.txt".into(), cx));
            assert_eq!(cx.read(|cx| tab.read(cx).status()), Status::Working, "{n}");
            let before = written(cx, tab).len();
            let sonnet = Change::Model {
                id: "sonnet".into(),
                effort: None,
            };
            let chosen = sonnet.clone();
            feed(cx, tab, |t, cx| t.choose(chosen, cx));
            assert_eq!(
                written(cx, tab).len(),
                before,
                "{n}: nothing written mid-turn"
            );
            assert_eq!(
                cx.read(|cx| tab.read(cx).waiting_change().cloned()),
                Some(sonnet),
                "{n}: it waits"
            );
            let argv_before = argv(cx, tab);
            // A live change holds nothing back (R5): a note goes at once, and the turn it
            // starts is waited for too.
            feed(cx, tab, |t, cx| t.send("Also check todo.txt".into(), cx));
            let lines = written(cx, tab);
            assert_eq!(lines.len(), before + 1, "{n}: the note is written at once");
            assert_eq!(lines[before]["type"], "user", "{n}");
            assert_eq!(
                cx.read(|cx| tab.read(cx).shot_pending()),
                0,
                "{n}: not held"
            );
            let before = lines.len();
            feed(cx, tab, |t, cx| t.shot_feed(vec![turn_done()], cx));
            assert_eq!(cx.read(|cx| tab.read(cx).status()), Status::Working, "{n}");
            assert_eq!(
                written(cx, tab).len(),
                before,
                "{n}: the note's turn is still to run"
            );
            feed(cx, tab, |t, cx| t.shot_feed(vec![turn_done()], cx));
            let lines = written(cx, tab);
            assert_eq!(lines.len(), before + 1, "{n}: one line after the turn");
            let line = &lines[before];
            assert_eq!(line["type"], "control_request", "{n}: {line}");
            assert_eq!(line["request"]["subtype"], "set_model", "{n}");
            assert_eq!(line["request"]["model"], "sonnet", "{n}");
            assert!(
                lines.iter().filter(|l| l["type"] == "user").count() == 2,
                "{n}: choosing never sends a message (yours: two)"
            );
            assert_eq!(argv(cx, tab), argv_before, "{n}: live, not a restart");
            assert_eq!(cx.read(|cx| tab.read(cx).waiting_change().cloned()), None);
            assert!(told.borrow().is_empty(), "{n}: not until Claude confirms");
            feed(cx, tab, |t, cx| {
                t.shot_feed(
                    vec![Event::Applied {
                        kind: ChangeKind::Model,
                        value: "claude-sonnet-5-5".into(),
                        ok: true,
                    }],
                    cx,
                )
            });
            assert_eq!(
                told.borrow().last().cloned(),
                Some((Some("sonnet".into()), None, None)),
                "{n}: confirmed, the tab starts with it from now on"
            );
            assert!(
                notices(cx, tab).is_empty(),
                "{n}: a live switch leaves no row"
            );

            // A refusal: one notice, nothing changes.
            feed(cx, tab, |t, cx| {
                t.choose(
                    Change::Model {
                        id: "haiku".into(),
                        effort: None,
                    },
                    cx,
                )
            });
            let told_before = told.borrow().len();
            feed(cx, tab, |t, cx| {
                t.shot_feed(
                    vec![Event::Applied {
                        kind: ChangeKind::Model,
                        value: "Unknown model".into(),
                        ok: false,
                    }],
                    cx,
                )
            });
            assert_eq!(notices(cx, tab), ["Couldn't switch: Unknown model"], "{n}");
            assert_eq!(told.borrow().len(), told_before, "{n}: nothing saved");

            // Idle: an effort alone is asked as an effort; a new model with an effort as both.
            let before = written(cx, tab).len();
            feed(cx, tab, |t, cx| {
                t.choose(
                    Change::Model {
                        id: "sonnet".into(),
                        effort: Some("low".into()),
                    },
                    cx,
                )
            });
            let lines = written(cx, tab);
            let subtypes: Vec<&str> = lines[before..]
                .iter()
                .map(|l| l["request"]["subtype"].as_str().unwrap_or(""))
                .collect();
            assert_eq!(subtypes, ["apply_flag_settings", "get_settings"], "{n}");
            let before = lines.len();
            feed(cx, tab, |t, cx| {
                t.choose(
                    Change::Model {
                        id: "opus".into(),
                        effort: Some("max".into()),
                    },
                    cx,
                )
            });
            let lines = written(cx, tab);
            let subtypes: Vec<&str> = lines[before..]
                .iter()
                .map(|l| l["request"]["subtype"].as_str().unwrap_or(""))
                .collect();
            assert_eq!(
                subtypes,
                ["set_model", "apply_flag_settings", "get_settings"],
                "{n}"
            );
        },
    );
}

/// agy after a message, idle: choosing a mode restarts it on the same conversation with the new
/// `--mode` and leaves one quiet row; chosen mid-turn, a model waits for the turn's end.
pub(super) fn agent_box_change_restart() {
    run_scene(
        "agent-box-change-restart",
        &AGY,
        start(Some("accept-edits"), None),
        |n, cx, tab, told| {
            let id = agy_conversation();
            feed(cx, tab, |t, cx| t.send("Say hello in one word".into(), cx));
            feed(cx, tab, |t, cx| t.shot_feed(agy_text(), cx));
            assert_eq!(cx.read(|cx| tab.read(cx).status()), Status::Done, "{n}");
            feed(cx, tab, |t, cx| t.choose(Change::Mode("plan".into()), cx));
            let a = argv(cx, tab);
            assert_eq!(flag(&a, "--conversation"), Some(id.clone()), "{n}: {a:?}");
            assert_eq!(flag(&a, "--mode").as_deref(), Some("plan"), "{n}: {a:?}");
            assert_eq!(notices(cx, tab), ["Switched to Plan"], "{n}");
            assert_eq!(
                cx.read(|cx| tab.read(cx).mode().map(str::to_string)),
                Some("plan".into()),
                "{n}"
            );
            assert_eq!(
                told.borrow().last().cloned(),
                Some((None, None, Some("plan".into()))),
                "{n}: saved"
            );
            assert!(
                written(cx, tab)
                    .iter()
                    .filter(|l| l["event"] == "user")
                    .count()
                    == 1,
                "{n}: nothing sent"
            );

            // Mid-turn: the model waits for the turn's end, then restarts with it.
            feed(cx, tab, |t, cx| t.send("Now fix notes.txt".into(), cx));
            let a = argv(cx, tab);
            feed(cx, tab, |t, cx| {
                t.choose(
                    Change::Model {
                        id: "gemini-3.8-flash-low".into(),
                        effort: None,
                    },
                    cx,
                )
            });
            assert_eq!(argv(cx, tab), a, "{n}: no restart mid-turn");
            feed(cx, tab, |t, cx| t.shot_feed(vec![turn_done()], cx));
            let a = argv(cx, tab);
            assert_eq!(flag(&a, "--conversation"), Some(id), "{n}: {a:?}");
            assert_eq!(
                flag(&a, "--model").as_deref(),
                Some("gemini-3.8-flash-low"),
                "{n}: {a:?}"
            );
            assert_eq!(flag(&a, "--mode").as_deref(), Some("plan"), "{n}: kept");
            assert!(
                !a.iter().any(|w| w == "--effort"),
                "{n}: never --effort beside agy's id"
            );
            assert_eq!(
                notices(cx, tab).last().map(String::as_str),
                Some("Switched to gemini-3.8-flash-low"),
                "{n}"
            );
        },
    );
}

/// A restart waiting for the turn, then the mode it runs chosen again: the change is dropped,
/// and what you sent meanwhile still goes when the turn ends (never left waiting).
fn cancelled_change_still_sends() {
    run_scene(
        "agent-box-restart-keeps-queued-text-cancelled",
        &AGY,
        start(Some("accept-edits"), None),
        |n, cx, tab, _| {
            feed(cx, tab, |t, cx| t.send("Say hello in one word".into(), cx));
            let mut events = agy_text();
            let done = events
                .iter()
                .position(|e| matches!(e, Event::TurnDone { .. }))
                .expect("a turn end");
            let rest = events.split_off(done);
            feed(cx, tab, |t, cx| t.shot_feed(events, cx));
            let a = argv(cx, tab);
            feed(cx, tab, |t, cx| t.choose(Change::Mode("plan".into()), cx));
            feed(cx, tab, |t, cx| t.send("hi".into(), cx));
            assert_eq!(cx.read(|cx| tab.read(cx).shot_pending()), 1, "{n}: held");
            feed(cx, tab, |t, cx| {
                t.choose(Change::Mode("accept-edits".into()), cx)
            });
            assert_eq!(cx.read(|cx| tab.read(cx).waiting_change().cloned()), None);
            feed(cx, tab, |t, cx| t.shot_feed(rest, cx));
            assert_eq!(argv(cx, tab), a, "{n}: no restart");
            assert_eq!(
                cx.read(|cx| tab.read(cx).shot_pending()),
                0,
                "{n}: not stuck"
            );
            let lines = written(cx, tab);
            let last = lines.last().expect("a line");
            assert_eq!(last["message"]["content"], "hi", "{n}: {last}");
        },
    );
}

/// Review Focus #3: a message sent while a restart waits for the turn is held (not written to
/// the process about to end) and sent to the restarted one; if that restart can't start, what
/// was sent stays in the conversation (the tab's bad-resume path) — nothing typed is lost.
pub(super) fn agent_box_restart_keeps_queued_text() {
    run_scene(
        "agent-box-restart-keeps-queued-text",
        &AGY,
        start(Some("accept-edits"), None),
        |n, cx, tab, _| {
            feed(cx, tab, |t, cx| t.send("Say hello in one word".into(), cx));
            let mut events = agy_text();
            // The turn's end comes later: it is still working.
            let done = events
                .iter()
                .position(|e| matches!(e, Event::TurnDone { .. }))
                .expect("a turn end");
            let rest = events.split_off(done);
            feed(cx, tab, |t, cx| t.shot_feed(events, cx));
            assert_eq!(cx.read(|cx| tab.read(cx).status()), Status::Working, "{n}");
            feed(cx, tab, |t, cx| t.choose(Change::Mode("plan".into()), cx));
            let sent = |cx: &VisualTestAppContext| {
                written(cx, tab)
                    .iter()
                    .filter(|l| l["event"] == "user")
                    .count()
            };
            assert_eq!(sent(cx), 1, "{n}");
            feed(cx, tab, |t, cx| t.send("And then this".into(), cx));
            assert_eq!(sent(cx), 1, "{n}: held while the restart waits");
            assert_eq!(cx.read(|cx| tab.read(cx).shot_pending()), 1, "{n}");
            feed(cx, tab, |t, cx| t.shot_feed(rest, cx));
            let a = argv(cx, tab);
            assert_eq!(flag(&a, "--mode").as_deref(), Some("plan"), "{n}: {a:?}");
            assert_eq!(cx.read(|cx| tab.read(cx).shot_pending()), 0, "{n}");
            let lines = written(cx, tab);
            let last = lines.last().expect("a line");
            assert_eq!(last["event"], "user", "{n}: {last}");
            assert_eq!(
                last["message"]["content"], "And then this",
                "{n}: sent to the restarted agent"
            );
            assert_eq!(said(cx, tab), ["Say hello in one word", "And then this"]);

            // That restart couldn't start: the tab's bad-resume path; the message stays.
            feed(cx, tab, |t, cx| {
                t.shot_process(
                    crate::agent_process::ProcessEvent::Exited {
                        code: Some(1),
                        stderr_tail: "network down".into(),
                    },
                    cx,
                )
            });
            assert!(
                said(cx, tab).iter().any(|s| s == "And then this"),
                "{n}: nothing typed is lost"
            );
        },
    );
    cancelled_change_still_sends();
}
