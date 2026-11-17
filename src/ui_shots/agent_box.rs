//! M11ak: the agent box's scenes. As the agent tab's (`agent.rs`), no process runs: the tab is
//! made with `AgentTab::for_shots`, fed decoded events through `shot_feed`, and what it would
//! write to its agent is read back with `shot_written`.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use gpui::{
    AnyWindowHandle, AppContext, Bounds, Context, Entity, Focusable, Modifiers, Pixels,
    VisualTestAppContext, px, size,
};

use super::{
    Kind, Scene, Themes, frame_fresh, render_with, shoot_with, shot_appearance, shot_theme,
    shot_themes,
};
use crate::agent_box::{
    BoxMenu, CHECKING, Change, MOVE_HERE, ModelRow, NEW_CONVERSATION, PLACEHOLDER, UsageRow,
};
use crate::agent_catalog::{Catalogs, Lookup};
use crate::agent_conversation::Item;
use crate::agent_tab::{AgentTab, AgentTabEvent};
use crate::folder_picker::{KnownFolders, PickerRow};
use crate::providers::agy::AGY;
use crate::providers::claude::CLAUDE;
use crate::providers::{
    Catalog, ChangeKind, EffortChoice, Event, LimitState, ModelChoice, Provider, RateLimit, Reset,
    Start, UsageGroup, UsageLimit,
};
use crate::status::Status;

/// This file, as `file!()` gives it: `registry::AREAS` pairs it with `SCENES`.
pub(super) const FILE: &str = file!();

/// The files a box change goes through: the tab, the box's rules, the conversation, the
/// providers that say how a change applies and write it.
const BOX: &[&str] = &[
    "src/agent_tab.rs",
    "src/agent_box.rs",
    "src/agent_box_menus.rs",
    "src/agent_conversation.rs",
    "src/providers/mod.rs",
    "src/providers/claude.rs",
    "src/providers/agy.rs",
    "src/providers/fixtures/agy-text.jsonl",
];

/// The files the box is drawn from: the change files, the composer inside it, the catalog its
/// chips read, and the menu panel's look.
const LAYOUT: &[&str] = &[
    "src/agent_tab.rs",
    "src/agent_box.rs",
    "src/agent_box_menus.rs",
    "src/agent_conversation.rs",
    "src/agent_catalog.rs",
    "src/composer.rs",
    "src/overlay_panel.rs",
    "src/providers/mod.rs",
    "src/providers/claude.rs",
];

/// The files the folder picker is made of: the box's, the picker's lists, its search box, and
/// the version and folder lists the providers read.
const FOLDER: &[&str] = &[
    "src/agent_tab.rs",
    "src/agent_box.rs",
    "src/agent_box_menus.rs",
    "src/agent_conversation.rs",
    "src/agent_catalog.rs",
    "src/folder_picker.rs",
    "src/name_input.rs",
    "src/composer.rs",
    "src/overlay_panel.rs",
    "src/providers/mod.rs",
    "src/providers/claude.rs",
];

/// The files the model menu is made of: the box's, the catalog it lists, and the providers that
/// say each model's efforts and how a change applies.
const MODEL: &[&str] = &[
    "src/agent_tab.rs",
    "src/agent_box.rs",
    "src/agent_box_menus.rs",
    "src/agent_conversation.rs",
    "src/agent_catalog.rs",
    "src/composer.rs",
    "src/overlay_panel.rs",
    "src/providers/mod.rs",
    "src/providers/claude.rs",
    "src/providers/agy.rs",
];

/// The files View Usage is made of: the model menu's, and the usage the providers read.
const USAGE: &[&str] = MODEL;

/// The files the branch chip is made of: the box's, and the branch look-up and switcher it
/// opens.
const BRANCH: &[&str] = &[
    "src/agent_tab.rs",
    "src/agent_box.rs",
    "src/agent_box_menus.rs",
    "src/agent_catalog.rs",
    "src/composer.rs",
    "src/git_branch.rs",
    "src/branch_switcher.rs",
    "src/workspace_agent.rs",
    "src/providers/mod.rs",
    "src/providers/claude.rs",
];

/// This area's scenes (`registry.rs` runs them in its `ORDER`).
pub(super) const SCENES: &[Scene] = &[
    Scene {
        name: "agent-box-empty",
        run: agent_box_empty,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: LAYOUT,
    },
    Scene {
        name: "agent-box-working",
        run: agent_box_working,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: LAYOUT,
    },
    Scene {
        name: "agent-box-attachment",
        run: agent_box_attachment,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: LAYOUT,
    },
    Scene {
        name: "agent-box-long-text",
        run: agent_box_long_text,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: LAYOUT,
    },
    Scene {
        name: "agent-box-folder-picker",
        run: agent_box_folder_picker,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: FOLDER,
    },
    Scene {
        name: "agent-box-folder-search",
        run: agent_box_folder_search,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: FOLDER,
    },
    Scene {
        name: "agent-box-folder-move-ask",
        run: agent_box_folder_move_ask,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: FOLDER,
    },
    Scene {
        name: "agent-box-folder-move-new-only",
        run: agent_box_folder_move_new_only,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: FOLDER,
    },
    Scene {
        name: "agent-box-folder-before-first",
        run: agent_box_folder_before_first,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: FOLDER,
    },
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
    Scene {
        name: "agent-box-model-menu",
        run: agent_box_model_menu,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: MODEL,
    },
    Scene {
        name: "agent-box-effort-list",
        run: agent_box_effort_list,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: MODEL,
    },
    Scene {
        name: "agent-box-model-agy",
        run: agent_box_model_agy,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: MODEL,
    },
    Scene {
        name: "agent-box-model-loading",
        run: agent_box_model_loading,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: MODEL,
    },
    Scene {
        name: "agent-box-model-failed",
        run: agent_box_model_failed,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: MODEL,
    },
    Scene {
        name: "agent-box-model-fallback",
        run: agent_box_model_fallback,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: MODEL,
    },
    Scene {
        name: "agent-box-model-choose-sends-nothing",
        run: agent_box_model_choose_sends_nothing,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: MODEL,
    },
    Scene {
        name: "agent-box-model-effort-composes",
        run: agent_box_model_effort_composes,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: MODEL,
    },
    Scene {
        name: "agent-box-usage-claude",
        run: agent_box_usage_claude,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: USAGE,
    },
    Scene {
        name: "agent-box-usage-agy",
        run: agent_box_usage_agy,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: USAGE,
    },
    Scene {
        name: "agent-box-usage-checking",
        run: agent_box_usage_checking,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: USAGE,
    },
    Scene {
        name: "agent-box-usage-text-line",
        run: agent_box_usage_text_line,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: USAGE,
    },
    Scene {
        name: "agent-box-usage-limit-close",
        run: agent_box_usage_limit_close,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: USAGE,
    },
    Scene {
        name: "agent-box-usage-cached",
        run: agent_box_usage_cached,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: USAGE,
    },
    Scene {
        name: "agent-box-model-waiting-no-efforts",
        run: agent_box_model_waiting_no_efforts,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: MODEL,
    },
    Scene {
        name: "agent-box-branch",
        run: agent_box_branch,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: BRANCH,
    },
    Scene {
        name: "agent-box-branch-opens-switcher",
        run: agent_box_branch_opens_switcher,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: BRANCH,
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

// ---- The box's layout (Task 5) ----

/// The window the layout scenes draw in.
const DIMS: (f32, f32) = (720.0, 640.0);

/// A made-up catalog: one model, `Claude Example 5`, with three efforts; the agent says it runs
/// in its `default` mode.
fn example_catalog() -> Catalog {
    Catalog {
        models: vec![ModelChoice {
            id: "example-5".into(),
            label: "Claude Example 5".into(),
            resolves_to: Some("claude-example-5".into()),
            efforts: ["low", "medium", "high"]
                .map(|l| EffortChoice {
                    level: l.into(),
                    model_id: None,
                })
                .to_vec(),
        }],
        mode: Some("default".into()),
        ..Catalog::default()
    }
}

/// Claude, `example-5` at `high` chosen, in `~/code/my-app`.
fn example_start() -> Start {
    Start {
        effort: Some("high".into()),
        ..start(None, Some("example-5"))
    }
}

/// Shoot layout scene `name` in both themes: the tab (keys bound, the keyboard in its box) with
/// its history in and the made-up catalog read; `act` sets it up and checks it before the shot.
fn layout_scene(
    name: &str,
    act: impl Fn(&str, &mut VisualTestAppContext, AnyWindowHandle, &Entity<AgentTab>) + Clone + 'static,
) {
    box_scene(
        name,
        &CLAUDE,
        example_start(),
        Lookup::Ready(example_catalog()),
        act,
    );
}

/// `layout_scene` for any agent, its `start` and what its catalog lookup says.
fn box_scene(
    name: &str,
    provider: &'static dyn Provider,
    start: Start,
    lookup: Lookup<Catalog>,
    act: impl Fn(&str, &mut VisualTestAppContext, AnyWindowHandle, &Entity<AgentTab>) + Clone + 'static,
) {
    for &(slug, theme_name) in shot_themes() {
        let theme = shot_theme(theme_name);
        let slot: Slot = Rc::default();
        let keep = slot.clone();
        let n = format!("{name}-{slug}");
        let act = act.clone();
        let (start, lookup) = (start.clone(), lookup.clone());
        let path = shoot_with(
            name,
            slug,
            &theme,
            size(px(DIMS.0), px(DIMS.1)),
            move |window, cx| {
                cx.set_global(shot_appearance(theme, cx));
                cx.bind_keys(crate::key_bindings());
                let tab = cx.new(|cx| AgentTab::for_shots(provider, start, window, cx));
                let focus = tab.read(cx).focus_handle(cx);
                focus.focus(window, cx);
                *keep.borrow_mut() = Some(tab.clone());
                tab.into()
            },
            move |cx, window| {
                let tab = slot.borrow().clone().expect("the agent tab");
                feed(cx, &tab, |t, cx| t.shot_history(Vec::new(), cx));
                feed(cx, &tab, |t, cx| t.shot_catalog(lookup, cx));
                let _ = frame_fresh(cx, window);
                act(&n, cx, window, &tab);
                let _ = frame_fresh(cx, window);
                slot.borrow_mut().take();
            },
        );
        println!("ui-shots: {}", path.display());
    }
}

/// Where `part` of the box was drawn (it must have been).
fn part(cx: &VisualTestAppContext, tab: &Entity<AgentTab>, n: &str, part: &str) -> Bounds<Pixels> {
    cx.read(|cx| tab.read(cx).shot_part(part))
        .unwrap_or_else(|| panic!("{n}: the {part} was never drawn"))
}

/// `inner` lies inside `outer`.
fn inside(inner: Bounds<Pixels>, outer: Bounds<Pixels>) -> bool {
    inner.left() >= outer.left()
        && inner.right() <= outer.right()
        && inner.top() >= outer.top()
        && inner.bottom() <= outer.bottom()
}

/// The composer's text area, as last drawn.
fn composer_bounds(cx: &VisualTestAppContext, tab: &Entity<AgentTab>) -> Bounds<Pixels> {
    cx.read(|cx| tab.read(cx).shot_composer().read(cx).shot_rows().2)
        .expect("the composer was drawn")
}

/// §1's box, empty: the folder above it; inside it the placeholder and, along the bottom, `+`,
/// the model chip (`Claude Example 5  High ▾`), then on the right the mode chip and send. A
/// menu opens upward from its chip, one at a time, and Esc closes it (stopping nothing).
pub(super) fn agent_box_empty() {
    layout_scene("agent-box-empty", |n, cx, window, tab| {
        let texts = cx.read(|cx| tab.read(cx).shot_box(cx));
        assert_eq!(texts.folder, "\u{1f4c1} my-app \u{25be}", "{n}");
        assert_eq!(texts.model, "Claude Example 5  High \u{25be}", "{n}");
        assert_eq!(
            texts.mode.as_deref(),
            Some("\u{1f6e1} Ask before edits \u{25be}"),
            "{n}: the mode Claude reports"
        );
        assert_eq!(texts.send, "\u{27a4}", "{n}");
        assert_eq!(texts.usage, None, "{n}: nothing spent yet");
        let placeholder = cx.read(|cx| {
            tab.read(cx)
                .shot_composer()
                .read(cx)
                .placeholder()
                .to_string()
        });
        assert_eq!(placeholder, PLACEHOLDER, "{n}");

        let (folder, the_box) = (part(cx, tab, n, "folder"), part(cx, tab, n, "box"));
        let (attach, model) = (part(cx, tab, n, "attach"), part(cx, tab, n, "model"));
        let (mode, send) = (part(cx, tab, n, "mode"), part(cx, tab, n, "send"));
        let text = composer_bounds(cx, tab);
        assert!(
            folder.bottom() <= the_box.top(),
            "{n}: the folder sits above the box"
        );
        assert!(!inside(folder, the_box), "{n}");
        for (name, b) in [
            ("+", attach),
            ("model", model),
            ("mode", mode),
            ("send", send),
        ] {
            assert!(
                inside(b, the_box),
                "{n}: {name} {b:?} is inside the box {the_box:?}"
            );
            assert!(b.top() >= text.bottom(), "{n}: {name} is under the text");
            assert!(
                (b.center().y - send.center().y).abs() < px(2.0),
                "{n}: {name} is on the bottom row"
            );
        }
        assert!(inside(text, the_box), "{n}: the text is in the box");
        assert!(attach.right() <= model.left(), "{n}: + then the model");
        assert!(model.right() < mode.left(), "{n}: the mode on the right");
        assert!(mode.right() <= send.left(), "{n}: then send, last");
        assert!(
            the_box.right() - send.right() < px(16.0),
            "{n}: send at the box's right edge"
        );
        assert!(
            attach.left() - the_box.left() < px(16.0),
            "{n}: + at its left edge"
        );

        // A menu opens upward from its chip; one at a time; Esc closes it.
        feed(cx, tab, |t, cx| {
            t.open_menu(
                BoxMenu::Model {
                    efforts_of: None,
                    usage: false,
                },
                cx,
            )
        });
        let _ = frame_fresh(cx, window);
        let menu = part(cx, tab, n, "menu");
        assert!(
            menu.bottom() <= model.top(),
            "{n}: above the model chip {menu:?}"
        );
        assert!(
            (menu.left() - model.left()).abs() < px(1.0),
            "{n}: from the chip's left {menu:?} {model:?}"
        );
        feed(cx, tab, |t, cx| t.open_menu(BoxMenu::Folder, cx));
        let _ = frame_fresh(cx, window);
        assert_eq!(
            cx.read(|cx| tab.read(cx).shot_menu()),
            Some(BoxMenu::Folder),
            "{n}: one at a time"
        );
        let menu = part(cx, tab, n, "menu");
        assert!(
            menu.bottom() <= folder.top(),
            "{n}: above the folder {menu:?}"
        );
        cx.simulate_keystrokes(window, "escape");
        cx.run_until_parked();
        cx.read(|cx| {
            let t = tab.read(cx);
            assert_eq!(t.shot_menu(), None, "{n}: Esc closed it");
            assert_eq!(t.status(), Status::Idle, "{n}");
            assert_eq!(t.shot_stop_label(), None, "{n}: and stopped nothing");
        });
        assert!(written(cx, tab).is_empty(), "{n}: nothing sent");

        // The box's edge shows where the keyboard is: the focus ring while its text has it,
        // the quiet edge when not; a click on the box's own room (not a chip) gives it back.
        let colors = cx.read(|cx| {
            crate::agent_tab::Colors::of(&cx.global::<crate::appearance::Appearance>().theme)
        });
        let edge = |cx: &mut VisualTestAppContext| {
            let shot = frame_fresh(cx, window);
            // Mid-way down the box's left edge (its probe sits just inside it).
            let at = (
                f32::from(the_box.left()) - 0.5,
                f32::from(the_box.center().y),
            );
            (
                cx.update_window(window, |_, window, cx| {
                    tab.read(cx).shot_box_edge(window, cx)
                })
                .expect("the window"),
                super::pixel_at(&shot, size(px(DIMS.0), px(DIMS.1)), at.0, at.1),
            )
        };
        let (focused, focused_px) = edge(cx);
        assert_eq!(focused, colors.box_focus, "{n}: focused, the focus ring");
        cx.update_window(window, |_, window, _| window.blur())
            .expect("the window");
        cx.run_until_parked();
        let (blurred, blurred_px) = edge(cx);
        assert_eq!(blurred, colors.box_edge, "{n}: not focused, the quiet edge");
        assert_ne!(focused_px, blurred_px, "{n}: and it is drawn so");
        let room = gpui::point(the_box.center().x, send.center().y);
        cx.simulate_click(window, room, Modifiers::none());
        cx.run_until_parked();
        cx.update_window(window, |_, window, cx| {
            assert!(
                tab.read(cx).focus_handle(cx).is_focused(window),
                "{n}: a click in the box's room focuses its text"
            );
        })
        .expect("the window");
        let (back, back_px) = edge(cx);
        assert_eq!(back, colors.box_focus, "{n}: focused again");
        assert_eq!(back_px, focused_px, "{n}");
        assert!(written(cx, tab).is_empty(), "{n}: the click sent nothing");
    });
}

/// Working: Stop in send's place; the top line says what the conversation cost and how full
/// its context is (`$0.21 · context 18%`), no longer the chips.
pub(super) fn agent_box_working() {
    layout_scene("agent-box-working", |n, cx, window, tab| {
        let send = part(cx, tab, n, "send");
        feed(cx, tab, |t, cx| t.send("Fix notes.txt".into(), cx));
        feed(cx, tab, |t, cx| {
            t.shot_feed(
                vec![Event::Usage {
                    context_pct: Some(18),
                    cost_usd: Some(0.2116),
                }],
                cx,
            )
        });
        let _ = frame_fresh(cx, window);
        let texts = cx.read(|cx| tab.read(cx).shot_box(cx));
        assert_eq!(cx.read(|cx| tab.read(cx).status()), Status::Working, "{n}");
        assert_eq!(
            texts.send, "\u{25a0} Stop  esc",
            "{n}: Stop in send's place"
        );
        assert_eq!(
            texts.usage.as_deref(),
            Some("$0.21 \u{b7} context 18%"),
            "{n}"
        );
        let stop = part(cx, tab, n, "send");
        assert!(
            (stop.right() - send.right()).abs() < px(1.0),
            "{n}: where send was {stop:?} {send:?}"
        );
        assert!(inside(stop, part(cx, tab, n, "box")), "{n}");
        let usage = part(cx, tab, n, "usage");
        assert!(
            usage.bottom() <= px(30.0),
            "{n}: on the pane's top line {usage:?}"
        );
        assert!(usage.right() > px(DIMS.0 / 2.0), "{n}: at its right");
    });
}

/// A command block attached: its chip inside the box, above the text; its × takes it off
/// (nothing is sent).
pub(super) fn agent_box_attachment() {
    layout_scene("agent-box-attachment", |n, cx, window, tab| {
        let npm = "$ npm test\n(exit 1)\n> app@1.0.0 test\nFAIL src/rsvp.test.ts";
        let done = "$ echo done\n(exit 0)\ndone";
        feed(cx, tab, |t, cx| t.attach(done.into(), cx));
        feed(cx, tab, |t, cx| t.attach(npm.into(), cx));
        let _ = frame_fresh(cx, window);
        let chips = part(cx, tab, n, "attachments");
        let the_box = part(cx, tab, n, "box");
        assert!(inside(chips, the_box), "{n}: the chips are in the box");
        assert!(
            chips.bottom() <= composer_bounds(cx, tab).top(),
            "{n}: above the text"
        );
        let x = cx
            .read(|cx| tab.read(cx).shot_remove_bounds(0))
            .expect("the first chip's \u{d7}");
        assert!(inside(x, chips), "{n}");
        cx.simulate_click(window, x.center(), Modifiers::none());
        cx.run_until_parked();
        cx.read(|cx| {
            let t = tab.read(cx);
            assert_eq!(
                t.shot_attachments(),
                [npm.to_string()],
                "{n}: \u{d7} took it off"
            );
            assert!(t.shot_items().is_empty(), "{n}: nothing sent");
        });
        assert!(written(cx, tab).is_empty(), "{n}");
    });
}

/// A long message: the box grows to 8 rows, then scrolls; the chips above and below the text
/// stay put as more is typed.
pub(super) fn agent_box_long_text() {
    layout_scene("agent-box-long-text", |n, cx, window, tab| {
        let line = "Please read src/rsvp.ts and the failing test, then explain why past dates \
                    are accepted and fix the comparison without changing today() at all. ";
        cx.simulate_input(window, "Fix it");
        let _ = frame_fresh(cx, window);
        let one = composer_bounds(cx, tab);
        cx.simulate_input(window, &line.repeat(4));
        let _ = frame_fresh(cx, window);
        let composer = cx.read(|cx| tab.read(cx).shot_composer());
        let (rows, first, _) = cx.read(|cx| composer.read(cx).shot_rows());
        assert!(rows > 8, "{n}: more rows than shown ({rows})");
        assert!(first > 0, "{n}: scrolled to the caret");
        let eight = composer_bounds(cx, tab);
        let ratio = eight.size.height / one.size.height;
        assert!(
            (7.5..8.5).contains(&ratio),
            "{n}: eight rows tall ({ratio})"
        );
        let parts = |cx: &VisualTestAppContext| {
            ["folder", "box", "attach", "model", "mode", "send"].map(|p| part(cx, tab, n, p))
        };
        let before = parts(cx);
        assert!(inside(eight, before[1]), "{n}: the text stays in the box");
        assert!(
            before[0].bottom() <= before[1].top(),
            "{n}: the folder above"
        );
        assert!(before[0].top() >= px(0.0), "{n}: still in the window");
        cx.simulate_input(window, &line.repeat(3));
        let _ = frame_fresh(cx, window);
        assert_eq!(parts(cx), before, "{n}: the chips stay put");
        assert_eq!(
            composer_bounds(cx, tab),
            eight,
            "{n}: no taller than 8 rows"
        );
    });
}

// ---- The folder chip and picker (Task 6) ----

/// A made-up folder under your home (`~/code/<name>`); nothing is read from it.
fn folder(name: &str) -> PathBuf {
    cwd().with_file_name(name)
}

/// What the tab told of its folder: `Folder`, and `NewConversationIn` (as `true`).
type Moves = Rc<RefCell<Vec<(PathBuf, bool)>>>;

/// Shoot folder scene `name` in both themes: the layout scenes' Claude tab in `~/code/my-app`,
/// fuxx knowing (made-up) `~/code/my-app` and `~/code/scratch` as Recent and as projects, Claude
/// knowing `~/code/scratch`, and Claude's version `version` read; what it tells of its folder is
/// kept. The picker is left as `act` leaves it for the shot.
fn folder_scene(
    name: &str,
    version: &'static str,
    act: impl Fn(&str, &mut VisualTestAppContext, AnyWindowHandle, &Entity<AgentTab>, &Moves)
    + Clone
    + 'static,
) {
    for &(slug, theme_name) in shot_themes() {
        let theme = shot_theme(theme_name);
        let slot: Slot = Rc::default();
        let keep = slot.clone();
        let moves: Moves = Rc::default();
        let heard = moves.clone();
        let n = format!("{name}-{slug}");
        let act = act.clone();
        let path = shoot_with(
            name,
            slug,
            &theme,
            size(px(DIMS.0), px(DIMS.1)),
            move |window, cx| {
                cx.set_global(shot_appearance(theme, cx));
                cx.bind_keys(crate::key_bindings());
                let both = vec![folder("my-app"), folder("scratch")];
                cx.set_global(KnownFolders {
                    recent: both.clone(),
                    projects: both,
                });
                let catalogs = cx.default_global::<Catalogs>();
                catalogs.set_version("claude", Lookup::Ready(Some(version.to_string())));
                catalogs.set_known_folders("claude", Lookup::Ready(vec![folder("scratch")]));
                let tab = cx.new(|cx| AgentTab::for_shots(&CLAUDE, example_start(), window, cx));
                cx.subscribe(&tab, move |_, e: &AgentTabEvent, _| match e {
                    AgentTabEvent::Folder(f) => heard.borrow_mut().push((f.clone(), false)),
                    AgentTabEvent::NewConversationIn(f) => {
                        heard.borrow_mut().push((f.clone(), true))
                    }
                    _ => {}
                })
                .detach();
                let focus = tab.read(cx).focus_handle(cx);
                focus.focus(window, cx);
                *keep.borrow_mut() = Some(tab.clone());
                tab.into()
            },
            move |cx, window| {
                let tab = slot.borrow().clone().expect("the agent tab");
                feed(cx, &tab, |t, cx| t.shot_history(Vec::new(), cx));
                feed(cx, &tab, |t, cx| {
                    t.shot_catalog(Lookup::Ready(example_catalog()), cx)
                });
                let _ = frame_fresh(cx, window);
                act(&n, cx, window, &tab, &moves);
                let _ = frame_fresh(cx, window);
                slot.borrow_mut().take();
            },
        );
        println!("ui-shots: {}", path.display());
    }
}

/// Click the folder chip (a real click), and draw what opened.
fn click_folder_chip(
    n: &str,
    cx: &mut VisualTestAppContext,
    window: AnyWindowHandle,
    tab: &Entity<AgentTab>,
) {
    let chip = part(cx, tab, n, "folder");
    cx.simulate_click(window, chip.center(), Modifiers::none());
    cx.run_until_parked();
    let _ = frame_fresh(cx, window);
}

/// Click the open menu's row saying `label` (a real click), and draw what follows.
fn click_row(
    n: &str,
    cx: &mut VisualTestAppContext,
    window: AnyWindowHandle,
    tab: &Entity<AgentTab>,
    label: &str,
) {
    let row = cx
        .read(|cx| tab.read(cx).shot_menu_row(label))
        .unwrap_or_else(|| panic!("{n}: no row {label} was drawn"));
    cx.simulate_click(window, row.center(), Modifiers::none());
    cx.run_until_parked();
    let _ = frame_fresh(cx, window);
}

fn rows(cx: &VisualTestAppContext, tab: &Entity<AgentTab>) -> Vec<PickerRow> {
    cx.read(|cx| tab.read(cx).shot_picker_rows())
}

fn folder_row_of(name: &str, current: bool) -> PickerRow {
    PickerRow::Folder {
        path: folder(name),
        shown: format!("~/code/{name}"),
        current,
    }
}

fn menu(cx: &VisualTestAppContext, tab: &Entity<AgentTab>) -> Option<BoxMenu> {
    cx.read(|cx| tab.read(cx).shot_menu())
}

fn labels(cx: &VisualTestAppContext, tab: &Entity<AgentTab>) -> Vec<String> {
    cx.read(|cx| tab.read(cx).shot_menu_labels())
}

fn tab_cwd(cx: &VisualTestAppContext, tab: &Entity<AgentTab>) -> PathBuf {
    cx.read(|cx| tab.read(cx).shot_cwd())
}

/// Whether the keyboard is in `focus`'s view.
fn has_keys(
    cx: &mut VisualTestAppContext,
    window: AnyWindowHandle,
    focus: impl Fn(&gpui::App) -> gpui::FocusHandle,
) -> bool {
    cx.update_window(window, |_, window, cx| focus(cx).is_focused(window))
        .expect("the window")
}

/// The search box has the keyboard.
fn search_has_keys(
    cx: &mut VisualTestAppContext,
    window: AnyWindowHandle,
    tab: &Entity<AgentTab>,
) -> bool {
    let tab = tab.clone();
    has_keys(cx, window, move |cx| {
        let search = tab.read(cx).shot_search().expect("the search box");
        search.read(cx).focus_handle(cx)
    })
}

/// The conversation's first message has gone and Claude said it started (under the id fuxx
/// chose): after this, the picker asks before it moves anything.
fn first_message(cx: &mut VisualTestAppContext, tab: &Entity<AgentTab>) -> String {
    let id = flag(&argv(cx, tab), "--session-id").expect("a new conversation's id");
    feed(cx, tab, |t, cx| t.send("Fix notes.txt".into(), cx));
    let started = Event::Started {
        conversation: id.clone(),
        model: None,
    };
    feed(cx, tab, |t, cx| t.shot_feed(vec![started, turn_done()], cx));
    id
}

/// §2's picker from the chip (a click): its search on top, with the keyboard; Recent (✓ on the
/// folder the agent is in); This Computer ▸, which opens fuxx's and Claude's folders beside it;
/// New Project; No Project. Opening it chooses nothing.
pub(super) fn agent_box_folder_picker() {
    folder_scene(
        "agent-box-folder-picker",
        "2.1.326 (Claude Code)",
        |n, cx, window, tab, moves| {
            let before = argv(cx, tab);
            click_folder_chip(n, cx, window, tab);
            assert_eq!(
                menu(cx, tab),
                Some(BoxMenu::Folder),
                "{n}: the chip opens it"
            );
            assert!(
                search_has_keys(cx, window, tab),
                "{n}: the search has the keyboard"
            );
            assert_eq!(
                rows(cx, tab),
                [
                    PickerRow::Heading("Recent"),
                    folder_row_of("my-app", true),
                    folder_row_of("scratch", false),
                    PickerRow::ThisComputer,
                    PickerRow::NewProject,
                    PickerRow::NoProject,
                ],
                "{n}"
            );
            assert_eq!(
                labels(cx, tab),
                [
                    "~/code/my-app",
                    "~/code/scratch",
                    "This Computer",
                    "New Project",
                    "No Project"
                ],
                "{n}: drawn top to bottom"
            );
            let (the_menu, chip) = (part(cx, tab, n, "menu"), part(cx, tab, n, "folder"));
            assert!(
                the_menu.bottom() <= chip.top(),
                "{n}: it opens up from the chip"
            );
            click_row(n, cx, window, tab, "This Computer");
            assert_eq!(
                cx.read(|cx| tab.read(cx).shot_computer_rows()),
                Some(vec![
                    folder_row_of("my-app", true),
                    folder_row_of("scratch", false)
                ]),
                "{n}: fuxx's and Claude's folders, each once"
            );
            let beside = part(cx, tab, n, "computer");
            assert!(
                beside.left() >= the_menu.right(),
                "{n}: beside the picker {beside:?} {the_menu:?}"
            );
            assert_eq!(menu(cx, tab), Some(BoxMenu::Folder), "{n}: still open");
            // The chip pressed while it is open closes it (not closed and opened again); the
            // next press opens it.
            click_folder_chip(n, cx, window, tab);
            assert_eq!(menu(cx, tab), None, "{n}: the chip closes it");
            click_folder_chip(n, cx, window, tab);
            assert_eq!(
                menu(cx, tab),
                Some(BoxMenu::Folder),
                "{n}: and opens it again"
            );
            click_row(n, cx, window, tab, "This Computer");
            assert!(
                cx.read(|cx| tab.read(cx).shot_computer_rows()).is_some(),
                "{n}"
            );
            assert_eq!(argv(cx, tab), before, "{n}: nothing restarted");
            assert!(moves.borrow().is_empty(), "{n}: nothing chosen");
            assert!(written(cx, tab).is_empty(), "{n}: nothing sent");
        },
    );
}

/// The search filters every section as you type (`scr` leaves `~/code/scratch`); Esc closes the
/// picker and gives the keyboard back to the box (stopping nothing).
pub(super) fn agent_box_folder_search() {
    folder_scene(
        "agent-box-folder-search",
        "2.1.326 (Claude Code)",
        |n, cx, window, tab, moves| {
            click_folder_chip(n, cx, window, tab);
            cx.simulate_keystrokes(window, "escape");
            cx.run_until_parked();
            assert_eq!(menu(cx, tab), None, "{n}: Esc closed it");
            let composer = cx.read(|cx| tab.read(cx).shot_composer());
            assert!(
                has_keys(cx, window, move |cx| composer.read(cx).focus_handle(cx)),
                "{n}: the keyboard is back in the box"
            );
            assert_eq!(cx.read(|cx| tab.read(cx).status()), Status::Idle, "{n}");

            click_folder_chip(n, cx, window, tab);
            cx.simulate_input(window, "scr");
            cx.run_until_parked();
            let _ = frame_fresh(cx, window);
            assert_eq!(
                rows(cx, tab),
                [
                    PickerRow::Heading("Recent"),
                    folder_row_of("scratch", false)
                ],
                "{n}"
            );
            assert_eq!(labels(cx, tab), ["~/code/scratch"], "{n}: drawn so");
            assert!(moves.borrow().is_empty(), "{n}: typing chooses nothing");
            assert!(written(cx, tab).is_empty(), "{n}: nothing sent");
        },
    );
}

/// After the first message a folder chosen asks: New conversation opens a new tab there
/// (this one stays as it is); Move this conversation restarts Claude on it in the new folder
/// (`--resume`), with one quiet row, the folder saved and the menus' lists read for it.
pub(super) fn agent_box_folder_move_ask() {
    folder_scene(
        "agent-box-folder-move-ask",
        "2.1.326 (Claude Code)",
        |n, cx, window, tab, moves| {
            let id = first_message(cx, tab);
            let before = argv(cx, tab);
            let sent = |cx: &VisualTestAppContext| {
                written(cx, tab)
                    .iter()
                    .filter(|l| l["type"] == "user")
                    .count()
            };
            assert_eq!(sent(cx), 1, "{n}");

            click_folder_chip(n, cx, window, tab);
            click_row(n, cx, window, tab, "~/code/scratch");
            assert_eq!(menu(cx, tab), Some(BoxMenu::Move(folder("scratch"))), "{n}");
            assert_eq!(
                labels(cx, tab),
                [MOVE_HERE, NEW_CONVERSATION],
                "{n}: it asks"
            );
            click_row(n, cx, window, tab, NEW_CONVERSATION);
            assert_eq!(menu(cx, tab), None, "{n}");
            assert_eq!(
                *moves.borrow(),
                [(folder("scratch"), true)],
                "{n}: a new conversation there"
            );
            assert_eq!(tab_cwd(cx, tab), folder("my-app"), "{n}: this one stays");
            assert_eq!(argv(cx, tab), before, "{n}: and isn't restarted");
            assert!(notices(cx, tab).is_empty(), "{n}");

            click_folder_chip(n, cx, window, tab);
            click_row(n, cx, window, tab, "~/code/scratch");
            click_row(n, cx, window, tab, MOVE_HERE);
            assert_eq!(menu(cx, tab), None, "{n}");
            assert_eq!(tab_cwd(cx, tab), folder("scratch"), "{n}: moved");
            let a = argv(cx, tab);
            assert_eq!(
                flag(&a, "--resume"),
                Some(id),
                "{n}: the same conversation {a:?}"
            );
            assert_eq!(notices(cx, tab), ["Moved to ~/code/scratch"], "{n}");
            assert_eq!(
                moves.borrow().last().cloned(),
                Some((folder("scratch"), false)),
                "{n}: saved"
            );
            assert_eq!(
                cx.read(|cx| cx
                    .global::<Catalogs>()
                    .catalog("claude", &folder("scratch"))
                    .cloned()),
                Some(Lookup::Loading),
                "{n}: the menus' lists are read for the new folder"
            );
            assert_eq!(sent(cx), 1, "{n}: moving sends nothing");
            assert_eq!(
                cx.read(|cx| tab.read(cx).shot_box(cx)).folder,
                "\u{1f4c1} scratch \u{25be}",
                "{n}"
            );

            // The ✓ follows; choosing where it is does nothing; the ask stays open for the shot.
            click_folder_chip(n, cx, window, tab);
            assert_eq!(
                rows(cx, tab)[1..3],
                [
                    folder_row_of("my-app", false),
                    folder_row_of("scratch", true)
                ],
                "{n}"
            );
            click_row(n, cx, window, tab, "~/code/scratch");
            assert_eq!(menu(cx, tab), None, "{n}: the folder it is in: nothing");
            assert_eq!(moves.borrow().len(), 2, "{n}");
            click_folder_chip(n, cx, window, tab);
            click_row(n, cx, window, tab, "~/code/my-app");
            assert_eq!(labels(cx, tab), [MOVE_HERE, NEW_CONVERSATION], "{n}");
            let (the_menu, chip) = (part(cx, tab, n, "menu"), part(cx, tab, n, "folder"));
            assert!(the_menu.bottom() <= chip.top(), "{n}: over the chip");
        },
    );
}

/// A Claude older than 2.1.223 can't resume a conversation from another folder: only New
/// conversation is offered.
pub(super) fn agent_box_folder_move_new_only() {
    folder_scene(
        "agent-box-folder-move-new-only",
        "2.1.200 (Claude Code)",
        |n, cx, window, tab, moves| {
            feed(cx, tab, |t, cx| t.shot_place(folder("scratch"), None, cx));
            first_message(cx, tab);
            click_folder_chip(n, cx, window, tab);
            assert_eq!(
                rows(cx, tab)[1..3],
                [
                    folder_row_of("my-app", false),
                    folder_row_of("scratch", true)
                ],
                "{n}: placed in scratch"
            );
            click_row(n, cx, window, tab, "~/code/my-app");
            assert_eq!(menu(cx, tab), Some(BoxMenu::Move(folder("my-app"))), "{n}");
            assert_eq!(labels(cx, tab), [NEW_CONVERSATION], "{n}: no Move");
            assert!(moves.borrow().is_empty(), "{n}");
            assert_eq!(tab_cwd(cx, tab), folder("scratch"), "{n}");
        },
    );
}

/// Before the first message a folder chosen just changes where the agent starts: the idle
/// process starts again there on a new conversation (no `--resume`, no row), the folder saved;
/// Return in the search picks its first folder.
pub(super) fn agent_box_folder_before_first() {
    folder_scene(
        "agent-box-folder-before-first",
        "2.1.326 (Claude Code)",
        |n, cx, window, tab, moves| {
            let first = flag(&argv(cx, tab), "--session-id").expect("a new conversation");
            // Chosen from This Computer's list, beside the picker.
            click_folder_chip(n, cx, window, tab);
            click_row(n, cx, window, tab, "This Computer");
            click_row(n, cx, window, tab, "This Computer \u{25b8} ~/code/scratch");
            assert_eq!(menu(cx, tab), None, "{n}: chosen, it closes");
            assert_eq!(tab_cwd(cx, tab), folder("scratch"), "{n}: it starts there");
            let a = argv(cx, tab);
            assert_eq!(flag(&a, "--resume"), None, "{n}: nothing to resume {a:?}");
            let fresh = flag(&a, "--session-id").expect("a new conversation");
            assert_ne!(fresh, first, "{n}: started again");
            assert!(
                notices(cx, tab).is_empty(),
                "{n}: no row before a conversation"
            );
            assert_eq!(*moves.borrow(), [(folder("scratch"), false)], "{n}: saved");
            assert!(written(cx, tab).is_empty(), "{n}: nothing sent");
            assert_eq!(
                cx.read(|cx| cx
                    .global::<Catalogs>()
                    .catalog("claude", &folder("scratch"))
                    .cloned()),
                Some(Lookup::Loading),
                "{n}: the lists are read for it"
            );
            let composer = cx.read(|cx| tab.read(cx).shot_composer());
            assert!(
                has_keys(cx, window, move |cx| composer.read(cx).focus_handle(cx)),
                "{n}: the keyboard is back in the box"
            );

            // Return in the search picks the first folder listed.
            click_folder_chip(n, cx, window, tab);
            cx.simulate_input(window, "my-app");
            cx.simulate_keystrokes(window, "enter");
            cx.run_until_parked();
            assert_eq!(tab_cwd(cx, tab), folder("my-app"), "{n}: Return chose it");
            assert_eq!(menu(cx, tab), None, "{n}");
            assert!(written(cx, tab).is_empty(), "{n}: and sent nothing");
            assert_eq!(
                cx.read(|cx| tab.read(cx).shot_box(cx)).folder,
                "\u{1f4c1} my-app \u{25be}",
                "{n}"
            );
        },
    );
}

// ---- The model menu (Task 8) ----

/// A made-up Claude list: `Claude Example 5` (five efforts, `high` running in the scenes),
/// `Claude Example 4` (three) and `Claude Tiny` (none).
fn model_catalog() -> Catalog {
    let levels = |ls: &[&str]| {
        ls.iter()
            .map(|l| EffortChoice {
                level: l.to_string(),
                model_id: None,
            })
            .collect::<Vec<_>>()
    };
    let model = |id: &str, label: &str, ls: &[&str]| ModelChoice {
        id: id.into(),
        label: label.into(),
        resolves_to: None,
        efforts: levels(ls),
    };
    Catalog {
        models: vec![
            model(
                "example-5",
                "Claude Example 5",
                &["low", "medium", "high", "xhigh", "max"],
            ),
            model("example-4", "Claude Example 4", &["low", "medium", "high"]),
            model("tiny", "Claude Tiny", &[]),
        ],
        mode: Some("default".into()),
        ..Catalog::default()
    }
}

/// A made-up agy list: the effort is in each level's id (`example-pro-high`).
fn agy_example_catalog() -> Catalog {
    let model = |id: &str, label: &str, ls: &[&str]| ModelChoice {
        id: id.into(),
        label: label.into(),
        resolves_to: None,
        efforts: ls
            .iter()
            .map(|l| EffortChoice {
                level: l.to_string(),
                model_id: Some(format!("{id}-{l}")),
            })
            .collect(),
    };
    Catalog {
        models: vec![
            model("example-pro", "Example Pro", &["low", "medium", "high"]),
            model("example-flash", "Example Flash", &["low", "high"]),
        ],
        ..Catalog::default()
    }
}

/// Claude on `example-5` at `high`, with the made-up list.
fn model_scene(
    name: &str,
    lookup: Lookup<Catalog>,
    act: impl Fn(&str, &mut VisualTestAppContext, AnyWindowHandle, &Entity<AgentTab>) + Clone + 'static,
) {
    box_scene(name, &CLAUDE, example_start(), lookup, act);
}

/// Click the model chip (a real click), and draw what opened.
fn click_model_chip(
    n: &str,
    cx: &mut VisualTestAppContext,
    window: AnyWindowHandle,
    tab: &Entity<AgentTab>,
) {
    let chip = part(cx, tab, n, "model");
    cx.simulate_click(window, chip.center(), Modifiers::none());
    cx.run_until_parked();
    let _ = frame_fresh(cx, window);
}

/// Click the `▸` of the model row `label` (the row's right end), and draw what opened.
fn click_more(
    n: &str,
    cx: &mut VisualTestAppContext,
    window: AnyWindowHandle,
    tab: &Entity<AgentTab>,
    label: &str,
) {
    let row = cx
        .read(|cx| tab.read(cx).shot_menu_row(label))
        .unwrap_or_else(|| panic!("{n}: no row {label} was drawn"));
    let at = gpui::point(row.right() - px(12.), row.center().y);
    cx.simulate_click(window, at, Modifiers::none());
    cx.run_until_parked();
    let _ = frame_fresh(cx, window);
}

/// Where the effort list's row `level` was drawn (none: not listed).
fn effort_row(
    cx: &VisualTestAppContext,
    tab: &Entity<AgentTab>,
    level: &str,
) -> Option<Bounds<Pixels>> {
    cx.read(|cx| {
        tab.read(cx)
            .shot_menu_row(&format!("Effort \u{25b8} {level}"))
    })
}

fn model_rows_of(cx: &VisualTestAppContext, tab: &Entity<AgentTab>) -> Vec<ModelRow> {
    cx.read(|cx| tab.read(cx).shot_model_rows(cx))
}

/// The words of the model rows that are models, in order.
fn model_names(rows: &[ModelRow]) -> Vec<String> {
    rows.iter()
        .filter_map(|r| match r {
            ModelRow::Model { label, .. } => Some(label.clone()),
            _ => None,
        })
        .collect()
}

/// §4's menu from the chip (a click): the `Model` heading, the models (✓ and the effort, in
/// grey, on the running one; `▸` on those with efforts), a divider and `View Usage ▸`. Opening
/// it chooses nothing; a press on the chip again closes it, and a press on another chip
/// (folder, branch) closes it too.
pub(super) fn agent_box_model_menu() {
    model_scene(
        "agent-box-model-menu",
        Lookup::Ready(model_catalog()),
        |n, cx, window, tab| {
            click_model_chip(n, cx, window, tab);
            assert_eq!(
                menu(cx, tab),
                Some(BoxMenu::Model {
                    efforts_of: None,
                    usage: false
                }),
                "{n}"
            );
            let rows = model_rows_of(cx, tab);
            assert_eq!(
                rows[1],
                ModelRow::Model {
                    ix: 0,
                    label: "Claude Example 5".into(),
                    effort: Some("high".into()),
                    current: true,
                    has_efforts: true
                },
                "{n}: ✓ and the effort on the running one"
            );
            assert_eq!(
                model_names(&rows),
                ["Claude Example 5", "Claude Example 4", "Claude Tiny"],
                "{n}"
            );
            assert!(
                matches!(
                    rows[3],
                    ModelRow::Model {
                        has_efforts: false,
                        ..
                    }
                ),
                "{n}"
            );
            // Rows, top to bottom; View Usage last, under the divider.
            let at = |l: &str| {
                cx.read(|cx| tab.read(cx).shot_menu_row(l))
                    .unwrap_or_else(|| panic!("{n}: no row {l}"))
            };
            let (a, b, t) = (
                at("Claude Example 5"),
                at("Claude Example 4"),
                at("Claude Tiny"),
            );
            let usage = at("View Usage \u{25b8}");
            assert!(a.bottom() <= b.top() && b.bottom() <= t.top(), "{n}");
            assert!(t.bottom() < usage.top(), "{n}: the divider between");
            let menu_box = part(cx, tab, n, "menu");
            assert!(inside(usage, menu_box), "{n}: inside the menu");
            assert!(
                part(cx, tab, n, "model").top() >= menu_box.bottom(),
                "{n}: upward"
            );
            assert!(written(cx, tab).is_empty(), "{n}: opening chooses nothing");

            // The chip again closes it (the press is no reopening).
            click_model_chip(n, cx, window, tab);
            assert_eq!(menu(cx, tab), None, "{n}: the chip closes it");
            // A press on another chip while a menu is open closes it (and opens nothing).
            click_folder_chip(n, cx, window, tab);
            assert_eq!(menu(cx, tab), Some(BoxMenu::Folder), "{n}");
            click_model_chip(n, cx, window, tab);
            assert_eq!(
                menu(cx, tab),
                None,
                "{n}: the model chip's press closes the picker"
            );
            click_model_chip(n, cx, window, tab);
            assert!(matches!(menu(cx, tab), Some(BoxMenu::Model { .. })), "{n}");
            click_folder_chip(n, cx, window, tab);
            assert_eq!(
                menu(cx, tab),
                None,
                "{n}: and the folder chip's, the model menu"
            );
            click_model_chip(n, cx, window, tab);
            assert!(matches!(menu(cx, tab), Some(BoxMenu::Model { .. })), "{n}");
            // And the branch chip's press closes an open menu too.
            feed(cx, tab, |t, cx| {
                t.shot_place(folder("my-app"), Some("master".into()), cx)
            });
            let _ = frame_fresh(cx, window);
            let branch = part(cx, tab, n, "branch");
            cx.simulate_click(window, branch.center(), Modifiers::none());
            cx.run_until_parked();
            assert_eq!(menu(cx, tab), None, "{n}: the branch chip closes the menu");
            // Leave it open for the shot.
            click_model_chip(n, cx, window, tab);
            assert!(menu(cx, tab).is_some(), "{n}");
        },
    );
}

/// `▸` on a model opens its effort list beside the menu: only that model's levels, the running
/// one ticked on the running model; the list follows the `▸` pressed; a model without efforts has
/// no `▸`. A press outside both closes them; one on the effort list is a pick, not outside.
pub(super) fn agent_box_effort_list() {
    model_scene(
        "agent-box-effort-list",
        Lookup::Ready(model_catalog()),
        |n, cx, window, tab| {
            click_model_chip(n, cx, window, tab);
            click_more(n, cx, window, tab, "Claude Example 4");
            assert_eq!(
                menu(cx, tab),
                Some(BoxMenu::Model {
                    efforts_of: Some(1),
                    usage: false
                }),
                "{n}"
            );
            assert!(
                effort_row(cx, tab, "Low").is_some() && effort_row(cx, tab, "High").is_some(),
                "{n}"
            );
            assert!(
                effort_row(cx, tab, "XHigh").is_none(),
                "{n}: Example 4 has three levels only"
            );
            click_more(n, cx, window, tab, "Claude Example 5");
            let levels = ["Low", "Medium", "High", "XHigh", "Max"];
            let spots: Vec<_> = levels.iter().map(|l| effort_row(cx, tab, l)).collect();
            assert!(
                spots.iter().all(Option::is_some),
                "{n}: Low … Max {spots:?}"
            );
            let tops: Vec<_> = spots.iter().map(|b| b.unwrap().top()).collect();
            assert!(
                tops.windows(2).all(|w| w[0] < w[1]),
                "{n}: in order {tops:?}"
            );
            let side = part(cx, tab, n, "effort");
            let menu_box = part(cx, tab, n, "menu");
            assert!(side.left() >= menu_box.right(), "{n}: beside the menu");
            assert!(inside(spots[0].unwrap(), side), "{n}");
            assert!(written(cx, tab).is_empty(), "{n}: opening chooses nothing");
            // A model with no efforts has no ▸: its right end picks it.
            assert!(
                !matches!(
                    model_rows_of(cx, tab)[3],
                    ModelRow::Model {
                        has_efforts: true,
                        ..
                    }
                ),
                "{n}"
            );
            // A press outside closes both.
            cx.simulate_click(window, gpui::point(px(30.), px(120.)), Modifiers::none());
            cx.run_until_parked();
            assert_eq!(menu(cx, tab), None, "{n}: outside closes");
            // Reopened, the effort list is open on the first model for the shot.
            click_model_chip(n, cx, window, tab);
            click_more(n, cx, window, tab, "Claude Example 5");
            assert!(effort_row(cx, tab, "Max").is_some(), "{n}");
        },
    );
}

/// agy's models grouped by name, the effort in each id: `▸` lists that model's levels. Choosing
/// before the first message restarts agy with `--model <id with the effort>` and no `--effort`.
pub(super) fn agent_box_model_agy() {
    box_scene(
        "agent-box-model-agy",
        &AGY,
        start(None, Some("example-pro-high")),
        Lookup::Ready(agy_example_catalog()),
        |n, cx, window, tab| {
            click_model_chip(n, cx, window, tab);
            let rows = model_rows_of(cx, tab);
            assert_eq!(model_names(&rows), ["Example Pro", "Example Flash"], "{n}");
            assert!(
                matches!(&rows[1], ModelRow::Model { current: true, effort: Some(e), .. } if e == "high"),
                "{n}: the effort rides in the id: {rows:?}"
            );
            click_more(n, cx, window, tab, "Example Flash");
            assert!(
                cx.read(|cx| tab.read(cx).shot_menu_row("Effort \u{25b8} Medium"))
                    .is_none(),
                "{n}: Example Flash has no medium"
            );
            click_more(n, cx, window, tab, "Example Pro");
            click_row(n, cx, window, tab, "Effort \u{25b8} Low");
            assert_eq!(menu(cx, tab), None, "{n}: chosen, it closes");
            let a = argv(cx, tab);
            assert_eq!(
                flag(&a, "--model").as_deref(),
                Some("example-pro-low"),
                "{n}: {a:?}"
            );
            assert_eq!(flag(&a, "--effort"), None, "{n}: never --effort {a:?}");
            // Another model: its id with the effort it keeps (low: Flash has it).
            click_model_chip(n, cx, window, tab);
            click_row(n, cx, window, tab, "Example Flash");
            let a = argv(cx, tab);
            assert_eq!(
                flag(&a, "--model").as_deref(),
                Some("example-flash-low"),
                "{n}: {a:?}"
            );
            // Left open, on the effort list, for the shot.
            click_model_chip(n, cx, window, tab);
            click_more(n, cx, window, tab, "Example Pro");
            assert!(
                cx.read(|cx| tab.read(cx).shot_menu_row("Effort \u{25b8} High"))
                    .is_some(),
                "{n}"
            );
        },
    );
}

/// While the list is read the menu says `Loading…` (the divider and View Usage stay).
pub(super) fn agent_box_model_loading() {
    model_scene(
        "agent-box-model-loading",
        Lookup::Loading,
        |n, cx, window, tab| {
            click_model_chip(n, cx, window, tab);
            assert_eq!(
                model_rows_of(cx, tab),
                [
                    ModelRow::Heading,
                    ModelRow::Note("Loading\u{2026}"),
                    ModelRow::Divider,
                    ModelRow::Usage
                ],
                "{n}"
            );
            assert!(
                cx.read(|cx| tab.read(cx).shot_menu_row("View Usage \u{25b8}"))
                    .is_some(),
                "{n}"
            );
        },
    );
}

/// A list that could not be read says so; no models are made up.
pub(super) fn agent_box_model_failed() {
    model_scene(
        "agent-box-model-failed",
        Lookup::Failed("claude: not found".into()),
        |n, cx, window, tab| {
            click_model_chip(n, cx, window, tab);
            assert_eq!(
                model_rows_of(cx, tab),
                [
                    ModelRow::Heading,
                    ModelRow::Note("Couldn't read the model list"),
                    ModelRow::Divider,
                    ModelRow::Usage
                ],
                "{n}"
            );
        },
    );
}

/// The agent's own aliases stand in when it lists none: marked `(default list)`; Haiku (which
/// takes no effort) has no `▸`.
pub(super) fn agent_box_model_fallback() {
    let list = Catalog {
        models: CLAUDE.fallback_models(),
        fallback_models: true,
        ..Catalog::default()
    };
    box_scene(
        "agent-box-model-fallback",
        &CLAUDE,
        start(None, None),
        Lookup::Ready(list),
        |n, cx, window, tab| {
            click_model_chip(n, cx, window, tab);
            let rows = model_rows_of(cx, tab);
            assert_eq!(rows[1], ModelRow::Note("(default list)"), "{n}");
            assert_eq!(
                model_names(&rows),
                ["Opus", "Sonnet", "Haiku", "Fable"],
                "{n}"
            );
            let efforts = |label: &str| {
                rows.iter().find_map(|r| match r {
                    ModelRow::Model {
                        label: l,
                        has_efforts,
                        ..
                    } if l == label => Some(*has_efforts),
                    _ => None,
                })
            };
            assert_eq!(efforts("Haiku"), Some(false), "{n}: Haiku takes no effort");
            assert_eq!(efforts("Opus"), Some(true), "{n}");
            assert!(written(cx, tab).is_empty(), "{n}");
        },
    );
}

/// Choosing never sends a message: idle Claude writes the set request (a model, then an effort
/// of the running model: an effort alone), and no `user` line; the menu closes and the keyboard
/// is back in the box.
pub(super) fn agent_box_model_choose_sends_nothing() {
    model_scene(
        "agent-box-model-choose-sends-nothing",
        Lookup::Ready(model_catalog()),
        |n, cx, window, tab| {
            let subtypes = |cx: &VisualTestAppContext, from: usize| -> Vec<String> {
                written(cx, tab)[from..]
                    .iter()
                    .map(|l| l["request"]["subtype"].as_str().unwrap_or("").to_string())
                    .collect()
            };
            let lines = |cx: &VisualTestAppContext| written(cx, tab).len();
            // A model: asked as a model change (its effort is kept where it has it).
            let before = lines(cx);
            click_model_chip(n, cx, window, tab);
            click_row(n, cx, window, tab, "Claude Example 4");
            assert_eq!(menu(cx, tab), None, "{n}: chosen, it closes");
            let sent = written(cx, tab);
            assert_eq!(subtypes(cx, before), ["set_model"], "{n}: {sent:?}");
            assert_eq!(sent[before]["request"]["model"], "example-4", "{n}");
            assert!(sent.iter().all(|l| l["type"] != "user"), "{n}: no message");
            assert_eq!(said(cx, tab), Vec::<String>::new(), "{n}: nothing said");
            // Claude confirms: the chip follows.
            feed(cx, tab, |t, cx| {
                t.shot_feed(
                    vec![Event::Applied {
                        kind: ChangeKind::Model,
                        value: "example-4".into(),
                        ok: true,
                    }],
                    cx,
                )
            });
            let texts = cx.read(|cx| tab.read(cx).shot_box(cx));
            assert_eq!(
                texts.model, "Claude Example 4  High \u{25be}",
                "{n}: {texts:?}"
            );
            // An effort of the running model alone: an effort request.
            let before = lines(cx);
            click_model_chip(n, cx, window, tab);
            click_more(n, cx, window, tab, "Claude Example 4");
            click_row(n, cx, window, tab, "Effort \u{25b8} Low");
            assert_eq!(
                subtypes(cx, before),
                ["apply_flag_settings", "get_settings"],
                "{n}"
            );
            assert!(written(cx, tab).iter().all(|l| l["type"] != "user"), "{n}");
            // A model with no efforts: the model alone.
            let before = lines(cx);
            click_model_chip(n, cx, window, tab);
            click_row(n, cx, window, tab, "Claude Tiny");
            assert_eq!(subtypes(cx, before), ["set_model"], "{n}");
            let composer = cx.read(|cx| tab.read(cx).shot_composer());
            assert!(
                has_keys(cx, window, move |cx| composer.read(cx).focus_handle(cx)),
                "{n}: the keyboard is back in the box"
            );
        },
    );
}

/// While Claude works, a model picked waits for the turn; an effort picked meanwhile belongs to
/// that picked model (the menu shows it as current): they compose into one change, never one
/// replacing the other. An effort of the model that runs, picked while a model waits, is a
/// choice of that model again: the waiting change is dropped.
pub(super) fn agent_box_model_effort_composes() {
    model_scene(
        "agent-box-model-effort-composes",
        Lookup::Ready(model_catalog()),
        |n, cx, window, tab| {
            feed(cx, tab, |t, cx| t.send("Fix notes.txt".into(), cx));
            assert_eq!(cx.read(|cx| tab.read(cx).status()), Status::Working, "{n}");
            let before = written(cx, tab).len();
            let waiting =
                |cx: &VisualTestAppContext| cx.read(|cx| tab.read(cx).waiting_change().cloned());
            click_model_chip(n, cx, window, tab);
            click_row(n, cx, window, tab, "Claude Example 4");
            assert_eq!(
                waiting(cx),
                Some(Change::Model {
                    id: "example-4".into(),
                    effort: Some("high".into())
                }),
                "{n}: waits, keeping the effort it has"
            );
            // Example 4 now shows as current (✓), with its effort.
            click_model_chip(n, cx, window, tab);
            let rows = model_rows_of(cx, tab);
            assert!(
                matches!(&rows[2], ModelRow::Model { current: true, effort: Some(e), .. } if e == "high"),
                "{n}: the picked model is the current one: {rows:?}"
            );
            assert!(
                matches!(&rows[1], ModelRow::Model { current: false, .. }),
                "{n}"
            );
            // An effort of it: the same model, the new effort, in one waiting change.
            click_more(n, cx, window, tab, "Claude Example 4");
            click_row(n, cx, window, tab, "Effort \u{25b8} Low");
            assert_eq!(
                waiting(cx),
                Some(Change::Model {
                    id: "example-4".into(),
                    effort: Some("low".into())
                }),
                "{n}: model and effort together"
            );
            assert_eq!(
                written(cx, tab).len(),
                before,
                "{n}: nothing written mid-turn"
            );
            // After the turn: the model, then the effort, once.
            feed(cx, tab, |t, cx| t.shot_feed(vec![turn_done()], cx));
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
            assert_eq!(lines[before]["request"]["model"], "example-4", "{n}");
            assert!(
                lines.iter().filter(|l| l["type"] == "user").count() == 1,
                "{n}: only yours"
            );
        },
    );
}

// ---- View Usage (Task 9) ----

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

/// A limit read, Claude's way: the agent's own words for when it resets.
fn claude_limit(name: &str, left_pct: u8, resets: &str) -> UsageLimit {
    UsageLimit::Read {
        name: name.into(),
        left_pct,
        resets: Some(Reset::Words(resets.into())),
    }
}

fn claude_usage() -> Lookup<Vec<UsageGroup>> {
    Lookup::Ready(vec![UsageGroup {
        name: String::new(),
        about: None,
        limits: vec![
            claude_limit("Current session", 100, "Nov 17 at 8:10pm"),
            claude_limit("Current week (all models)", 63, "Nov 22 at 10pm"),
        ],
    }])
}

/// Open the model menu and press `View Usage ▸` (real clicks).
fn open_usage(
    n: &str,
    cx: &mut VisualTestAppContext,
    window: AnyWindowHandle,
    tab: &Entity<AgentTab>,
) {
    click_model_chip(n, cx, window, tab);
    click_row(n, cx, window, tab, "View Usage \u{25b8}");
    assert_eq!(
        menu(cx, tab),
        Some(BoxMenu::Model {
            efforts_of: None,
            usage: true
        }),
        "{n}"
    );
}

fn usage_rows_of(cx: &VisualTestAppContext, tab: &Entity<AgentTab>) -> Vec<UsageRow> {
    cx.read(|cx| tab.read(cx).shot_usage_rows(cx))
}

fn limit_row(name: &str, left: Option<u8>, resets: Option<&str>, note: Option<&str>) -> UsageRow {
    UsageRow::Limit {
        name: name.into(),
        left_pct: left,
        resets: resets.map(String::from),
        note: note.map(String::from),
    }
}

/// The usage panel is drawn beside the model menu, each of `names` has its row in it, top to
/// bottom.
fn assert_usage_panel(n: &str, cx: &VisualTestAppContext, tab: &Entity<AgentTab>, names: &[&str]) {
    let side = part(cx, tab, n, "usage");
    let menu_box = part(cx, tab, n, "menu");
    assert!(side.left() >= menu_box.right(), "{n}: beside the menu");
    let spots: Vec<Bounds<Pixels>> = names
        .iter()
        .map(|name| {
            cx.read(|cx| {
                tab.read(cx)
                    .shot_menu_row(&format!("Usage \u{25b8} {name}"))
            })
            .unwrap_or_else(|| panic!("{n}: no usage row {name}"))
        })
        .collect();
    assert!(spots.iter().all(|r| inside(*r, side)), "{n}: {spots:?}");
    assert!(
        spots.windows(2).all(|w| w[0].bottom() <= w[1].top()),
        "{n}: in order {spots:?}"
    );
}

/// §5, Claude: `View Usage ▸` opens the panel beside the menu: each limit's name, `N% left` with
/// its ring, and `Resets …` in Claude's own words. Opening reads nothing from the screen.
pub(super) fn agent_box_usage_claude() {
    model_scene(
        "agent-box-usage-claude",
        Lookup::Ready(model_catalog()),
        |n, cx, window, tab| {
            feed(cx, tab, |t, cx| {
                t.shot_usage(claude_usage(), Vec::new(), cx)
            });
            open_usage(n, cx, window, tab);
            assert_eq!(
                usage_rows_of(cx, tab),
                [
                    limit_row(
                        "Current session",
                        Some(100),
                        Some("Resets Nov 17 at 8:10pm"),
                        None
                    ),
                    limit_row(
                        "Current week (all models)",
                        Some(63),
                        Some("Resets Nov 22 at 10pm"),
                        None
                    ),
                ],
                "{n}"
            );
            assert_usage_panel(
                n,
                cx,
                tab,
                &["Current session", "Current week (all models)"],
            );
            assert!(
                written(cx, tab).is_empty(),
                "{n}: nothing sent to the agent"
            );
            // A press on a model's `▸` swaps the panel for its efforts.
            click_more(n, cx, window, tab, "Claude Example 4");
            assert!(effort_row(cx, tab, "Low").is_some(), "{n}");
            assert!(
                cx.read(|cx| tab.read(cx).shot_menu_row("Usage \u{25b8} Current session"))
                    .is_none(),
                "{n}: the usage panel makes way"
            );
            // And the panel is back for the shot.
            click_row(n, cx, window, tab, "View Usage \u{25b8}");
            assert_usage_panel(n, cx, tab, &["Current session"]);
        },
    );
}

/// §5, agy: two groups, each named, each limit with its ring; a reset moment counts down.
pub(super) fn agent_box_usage_agy() {
    box_scene(
        "agent-box-usage-agy",
        &AGY,
        start(None, Some("example-pro-high")),
        Lookup::Ready(agy_example_catalog()),
        |n, cx, window, tab| {
            let at = |secs: i64| Some(Reset::At(now_unix() + secs + 30));
            let read = |name: &str, left: u8, resets| UsageLimit::Read {
                name: name.into(),
                left_pct: left,
                resets,
            };
            let found = Lookup::Ready(vec![
                UsageGroup {
                    name: "Example Pro".into(),
                    about: None,
                    limits: vec![
                        read("5 hour", 23, at(2 * 3600 + 5 * 60)),
                        read("Weekly", 78, at(3 * 86_400 + 16 * 3600)),
                    ],
                },
                UsageGroup {
                    name: "Example Flash".into(),
                    about: None,
                    limits: vec![read("Daily", 100, at(59 * 60)), read("Weekly", 0, None)],
                },
            ]);
            feed(cx, tab, |t, cx| t.shot_usage(found, Vec::new(), cx));
            open_usage(n, cx, window, tab);
            assert_eq!(
                usage_rows_of(cx, tab),
                [
                    UsageRow::Heading("Example Pro".into()),
                    limit_row("5 hour", Some(23), Some("Resets in 2h 5m"), None),
                    limit_row("Weekly", Some(78), Some("Resets in 3d 16h"), None),
                    UsageRow::Heading("Example Flash".into()),
                    limit_row("Daily", Some(100), Some("Resets in 59m"), None),
                    limit_row("Weekly", Some(0), None, None),
                ],
                "{n}"
            );
            let side = part(cx, tab, n, "usage");
            assert!(
                side.right() <= px(DIMS.0),
                "{n}: the panel is on screen {side:?}"
            );
        },
    );
}

/// While the reading is made the panel says `Checking…`; opening asks for it (the scene tab runs
/// no process: it marks the reading loading).
pub(super) fn agent_box_usage_checking() {
    model_scene(
        "agent-box-usage-checking",
        Lookup::Ready(model_catalog()),
        |n, cx, window, tab| {
            assert_eq!(
                cx.read(|cx| tab.read(cx).shot_usage_lookup(cx)),
                None,
                "{n}"
            );
            open_usage(n, cx, window, tab);
            assert_eq!(
                cx.read(|cx| tab.read(cx).shot_usage_lookup(cx)),
                Some(Lookup::Loading),
                "{n}: opening asked"
            );
            assert_eq!(
                usage_rows_of(cx, tab),
                [UsageRow::Quiet(CHECKING.into())],
                "{n}"
            );
            // The answer fills the panel in (no reopening).
            feed(cx, tab, |t, cx| {
                t.shot_usage(claude_usage(), Vec::new(), cx)
            });
            let _ = frame_fresh(cx, window);
            assert_eq!(usage_rows_of(cx, tab).len(), 2, "{n}");
            // A reading that failed says so.
            feed(cx, tab, |t, cx| {
                t.shot_usage(Lookup::Failed("exit 1".into()), Vec::new(), cx)
            });
            let _ = frame_fresh(cx, window);
            assert_eq!(
                usage_rows_of(cx, tab),
                [UsageRow::Quiet("Couldn't read usage".into())],
                "{n}"
            );
            // And the shot is of the wait.
            feed(cx, tab, |t, cx| {
                t.shot_usage(Lookup::Loading, Vec::new(), cx)
            });
            let _ = frame_fresh(cx, window);
            assert_eq!(
                usage_rows_of(cx, tab),
                [UsageRow::Quiet(CHECKING.into())],
                "{n}"
            );
        },
    );
}

/// A line fuxx couldn't read is shown as the agent wrote it, beside the limits it could.
pub(super) fn agent_box_usage_text_line() {
    model_scene(
        "agent-box-usage-text-line",
        Lookup::Ready(model_catalog()),
        |n, cx, window, tab| {
            let found = Lookup::Ready(vec![UsageGroup {
                name: String::new(),
                about: None,
                limits: vec![
                    claude_limit("Current session", 87, "Nov 17 at 8:10pm"),
                    UsageLimit::Text("Extra usage: off".into()),
                ],
            }]);
            feed(cx, tab, |t, cx| t.shot_usage(found, Vec::new(), cx));
            open_usage(n, cx, window, tab);
            assert_eq!(
                usage_rows_of(cx, tab),
                [
                    limit_row(
                        "Current session",
                        Some(87),
                        Some("Resets Nov 17 at 8:10pm"),
                        None
                    ),
                    UsageRow::Text("Extra usage: off".into()),
                ],
                "{n}"
            );
        },
    );
}

/// A limit the agent says is close: the model chip carries a warning dot, the limit says
/// `Close to limit`; when it is back to fine the dot goes (the state comes in as the agent's
/// own `rate_limit_event`).
pub(super) fn agent_box_usage_limit_close() {
    model_scene(
        "agent-box-usage-limit-close",
        Lookup::Ready(model_catalog()),
        |n, cx, window, tab| {
            let said = |state| {
                vec![Event::RateLimit(RateLimit {
                    limit: "Current session".into(),
                    state,
                    resets_at: None,
                })]
            };
            let warning =
                |cx: &VisualTestAppContext| cx.read(|cx| tab.read(cx).shot_box(cx).limit_warning);
            feed(cx, tab, |t, cx| {
                t.shot_usage(claude_usage(), Vec::new(), cx)
            });
            let _ = frame_fresh(cx, window);
            assert!(!warning(cx), "{n}: fine: no dot");
            assert!(
                cx.read(|cx| tab.read(cx).shot_part("limit-warning"))
                    .is_none(),
                "{n}"
            );
            feed(cx, tab, |t, cx| t.shot_feed(said(LimitState::Close), cx));
            let _ = frame_fresh(cx, window);
            assert!(warning(cx), "{n}: close: the dot");
            let chip = part(cx, tab, n, "model");
            let dot = part(cx, tab, n, "limit-warning");
            assert!(inside(dot, chip), "{n}: on the chip {dot:?} {chip:?}");
            // Back to fine.
            feed(cx, tab, |t, cx| t.shot_feed(said(LimitState::Fine), cx));
            assert!(!warning(cx), "{n}: fine again");
            feed(cx, tab, |t, cx| t.shot_feed(said(LimitState::Close), cx));
            open_usage(n, cx, window, tab);
            assert_eq!(
                usage_rows_of(cx, tab)[0],
                limit_row(
                    "Current session",
                    Some(100),
                    Some("Resets Nov 17 at 8:10pm"),
                    Some("Close to limit")
                ),
                "{n}"
            );
            // Reached, with its moment: the line says when.
            feed(cx, tab, |t, cx| {
                t.shot_feed(
                    vec![Event::RateLimit(RateLimit {
                        limit: "Current session".into(),
                        state: LimitState::Reached,
                        resets_at: Some(14 * 3600),
                    })],
                    cx,
                )
            });
            let _ = frame_fresh(cx, window);
            let UsageRow::Limit {
                note: Some(note), ..
            } = &usage_rows_of(cx, tab)[0]
            else {
                panic!("{n}: a note under the limit");
            };
            assert!(
                note.starts_with("Limit reached \u{b7} resets "),
                "{n}: {note}"
            );
            // Leave it close for the shot.
            feed(cx, tab, |t, cx| t.shot_feed(said(LimitState::Close), cx));
            let _ = frame_fresh(cx, window);
        },
    );
}

/// Opening the panel again within a minute starts no new reading (and shows the one it has).
pub(super) fn agent_box_usage_cached() {
    model_scene(
        "agent-box-usage-cached",
        Lookup::Ready(model_catalog()),
        |n, cx, window, tab| {
            feed(cx, tab, |t, cx| {
                t.shot_usage(claude_usage(), Vec::new(), cx)
            });
            let ready = Some(claude_usage());
            for round in 0..2 {
                open_usage(n, cx, window, tab);
                assert_eq!(
                    cx.read(|cx| tab.read(cx).shot_usage_lookup(cx)),
                    ready,
                    "{n}: round {round}: no new reading was begun"
                );
                assert_eq!(usage_rows_of(cx, tab).len(), 2, "{n}");
                feed(cx, tab, |t, cx| t.close_menu(cx));
            }
        },
    );
}

/// A model with no efforts, picked mid-turn, shows none in the menu (not the last model's).
pub(super) fn agent_box_model_waiting_no_efforts() {
    model_scene(
        "agent-box-model-waiting-no-efforts",
        Lookup::Ready(model_catalog()),
        |n, cx, window, tab| {
            feed(cx, tab, |t, cx| t.send("Fix notes.txt".into(), cx));
            click_model_chip(n, cx, window, tab);
            click_row(n, cx, window, tab, "Claude Tiny");
            assert_eq!(
                cx.read(|cx| tab.read(cx).waiting_change().cloned()),
                Some(Change::Model {
                    id: "tiny".into(),
                    effort: None
                }),
                "{n}"
            );
            click_model_chip(n, cx, window, tab);
            let rows = model_rows_of(cx, tab);
            assert!(
                matches!(
                    &rows[3],
                    ModelRow::Model {
                        current: true,
                        effort: None,
                        ..
                    }
                ),
                "{n}: Tiny is current with no effort: {rows:?}"
            );
        },
    );
}

// ---- The branch chip (Task 7) ----

/// §3: `⑂ master ▾` beside `📁 my-app ▾` when the folder is in a repo; no chip when it isn't.
pub(super) fn agent_box_branch() {
    layout_scene("agent-box-branch", |n, cx, window, tab| {
        // A folder that isn't in a repo: no chip.
        let texts = cx.read(|cx| tab.read(cx).shot_box(cx));
        assert_eq!(texts.branch, None, "{n}: no branch, no chip");
        feed(cx, tab, |t, cx| t.shot_place(folder("my-app"), None, cx));
        let _ = frame_fresh(cx, window);
        assert_eq!(
            cx.read(|cx| tab.read(cx).shot_part("branch")),
            None,
            "{n}: no chip is drawn"
        );

        feed(cx, tab, |t, cx| {
            t.shot_place(folder("my-app"), Some("master".into()), cx)
        });
        let _ = frame_fresh(cx, window);
        let texts = cx.read(|cx| tab.read(cx).shot_box(cx));
        assert_eq!(
            texts.branch.as_deref(),
            Some("\u{2442} master \u{25be}"),
            "{n}"
        );
        assert_eq!(texts.folder, "\u{1f4c1} my-app \u{25be}", "{n}");
        let (folder_chip, chip) = (part(cx, tab, n, "folder"), part(cx, tab, n, "branch"));
        assert!(
            chip.left() >= folder_chip.right(),
            "{n}: beside the folder chip, after it"
        );
        assert!(
            (chip.center().y - folder_chip.center().y).abs() < px(1.),
            "{n}: on the folder chip's row"
        );
    });
}

/// A click on the chip asks the Workspace to open its branch switcher on the box's folder; the
/// agent's process is not started again.
pub(super) fn agent_box_branch_opens_switcher() {
    use crate::layout_model::TabKind;
    use crate::workspace::Workspace;

    let n = "agent-box-branch-opens-switcher";
    let Some(git) = crate::git_branch::find_git() else {
        println!("ui-shots: {n} skipped (no git on this Mac)");
        return;
    };
    let base =
        std::env::temp_dir().join(format!("fuxx-ui-shots-box-branch-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let repo = base.join("my-app");
    std::fs::create_dir_all(&repo).expect("make the repo");
    let run = |args: &[&str]| super::branch::scratch_git(&git, &repo, args);
    run(&["init", "-q", "-b", "master"]);
    std::fs::write(repo.join("a.txt"), "a\n").unwrap();
    run(&["add", "."]);
    run(&["commit", "-q", "-m", "one"]);

    let theme = shot_theme("Catppuccin Mocha");
    type WsSlot = Rc<RefCell<Option<Entity<Workspace>>>>;
    let slot: WsSlot = Rc::default();
    let keep = slot.clone();
    let at = repo.clone();
    render_with(
        &theme,
        size(px(900.), px(560.)),
        move |window, cx| {
            cx.set_global(shot_appearance(theme, cx));
            cx.bind_keys(crate::key_bindings());
            let ws = cx.new(|cx| {
                let mut ws = Workspace::for_shots(window, cx);
                let project = std::path::Path::new("/Users/me/code/api");
                ws.open_shot_session("api", project, super::SHELL_TEXT, None, window, cx);
                let pane = ws.model.focused_pane().expect("a pane");
                ws.model
                    .add_tab(
                        pane,
                        TabKind::Agent {
                            provider: "claude".into(),
                            cwd: at.clone(),
                            conversation: None,
                            mode: None,
                            title: None,
                            model: None,
                            effort: None,
                        },
                    )
                    .expect("an agent tab");
                ws.sync();
                ws
            });
            *keep.borrow_mut() = Some(ws.clone());
            ws.into()
        },
        move |cx, window| {
            let ws = slot.borrow().clone().expect("the workspace");
            cx.run_until_parked();
            let _ = frame_fresh(cx, window);
            let tab = cx.read(|cx| {
                let ws = ws.read(cx);
                let id = ws.model.focused_tab().expect("a tab");
                ws.agents.get(&id).expect("its view").view.clone()
            });
            feed(cx, &tab, |t, cx| t.shot_history(Vec::new(), cx));
            feed(cx, &tab, |t, cx| {
                t.shot_place(repo.clone(), Some("master".into()), cx)
            });
            let _ = frame_fresh(cx, window);
            let argv_before = argv(cx, &tab);
            assert!(!argv_before.is_empty(), "{n}: it would start the agent");
            assert!(
                cx.read(|cx| ws.read(cx).branch_switcher.is_none()),
                "{n}: closed to begin with"
            );

            let chip = part(cx, &tab, n, "branch");
            cx.simulate_click(window, chip.center(), Modifiers::none());
            cx.run_until_parked();
            let _ = frame_fresh(cx, window);
            let switcher = cx
                .read(|cx| ws.read(cx).branch_switcher.clone())
                .unwrap_or_else(|| panic!("{n}: the chip opens the switcher"));
            assert!(
                super::branch::settle_until(cx, window, 10, |cx| cx
                    .read(|cx| switcher.read(cx).title() == "Switch branch \u{b7} my-app")),
                "{n}: it is on the box's folder: {}",
                cx.read(|cx| switcher.read(cx).title())
            );
            assert_eq!(
                argv(cx, &tab),
                argv_before,
                "{n}: the agent did not restart"
            );
            assert_eq!(tab_cwd(cx, &tab), repo, "{n}: and is where it was");
            slot.borrow_mut().take();
        },
    );
    let _ = std::fs::remove_dir_all(&base);
    println!("ui-shots: {n} ok");
}
