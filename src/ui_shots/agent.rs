//! M11aa Task 7: the agent tab's scenes. No process runs in any of them: the tab is made with
//! `AgentTab::for_shots` and fed decoded fixture events through `shot_feed` (R12), the same
//! path a process line takes. Each one that keeps a picture is shot in Mocha and Latte (the
//! plan's "each in Mocha and Latte": `Themes::Both`); the checks-only ones in Mocha.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui::{
    AnyView, AnyWindowHandle, App, AppContext, Context, Entity, Focusable, IntoElement, Render,
    StyleRefinement, VisualTestAppContext, Window, div, prelude::*, px, size,
};

use super::chrome::settle_frames;
use super::{
    Kind, Scene, Themes, bytes_differ, frame_fresh, keep_shot, render_with, save_shot, shoot_with,
    shot_appearance, shot_theme, shot_themes, shown,
};
use crate::agent_conversation::{Answer, Item};
use crate::agent_process::ProcessEvent;
use crate::agent_tab::{AgentTab, BAD_RESUME_NOTICE};
use crate::providers::claude::CLAUDE;
use crate::providers::{Event, Past, Provider, Start, TodoItem, TodoState};
use crate::status::Status;

/// This file, as `file!()` gives it (repo-relative under cargo): `registry::AREAS` pairs it with
/// `SCENES`, so `--changed` runs these scenes when this file changes.
pub(super) const FILE: &str = file!();

/// The files every agent tab view is drawn from: the tab, its conversation and composer, the
/// Markdown its replies are drawn with, the diff rows of its edits (and their colours, from the
/// file tab), the `@` list's ranking, and the provider whose fixture events it is fed. (A
/// scene's own fixtures are added to its row.)
const TAB: &[&str] = &[
    "src/agent_tab.rs",
    "src/agent_conversation.rs",
    "src/agent_markdown.rs",
    "src/composer.rs",
    "src/md_preview.rs",
    "src/diff_view.rs",
    "src/file_tab.rs",
    "src/palette_files.rs",
    "src/providers/mod.rs",
    "src/providers/claude.rs",
];

/// This area's scenes (`registry.rs` runs them in its `ORDER`).
pub(super) const SCENES: &[Scene] = &[
    Scene {
        name: "agent-tab-empty",
        run: agent_tab_empty,
        themes: Themes::Both,
        serial: false,
        kind: Kind::State,
        sources: TAB,
    },
    Scene {
        name: "agent-tab-working",
        run: agent_tab_working,
        themes: Themes::Both,
        serial: false,
        kind: Kind::State,
        sources: &[
            "src/agent_tab.rs",
            "src/agent_conversation.rs",
            "src/agent_markdown.rs",
            "src/composer.rs",
            "src/md_preview.rs",
            "src/diff_view.rs",
            "src/file_tab.rs",
            "src/palette_files.rs",
            "src/providers/mod.rs",
            "src/providers/claude.rs",
            "src/providers/fixtures/claude-tools.jsonl",
        ],
    },
    Scene {
        name: "agent-tab-permission",
        run: agent_tab_permission,
        themes: Themes::Both,
        serial: false,
        kind: Kind::State,
        sources: &[
            "src/agent_tab.rs",
            "src/agent_conversation.rs",
            "src/agent_markdown.rs",
            "src/composer.rs",
            "src/md_preview.rs",
            "src/diff_view.rs",
            "src/file_tab.rs",
            "src/palette_files.rs",
            "src/providers/mod.rs",
            "src/providers/claude.rs",
            "src/providers/fixtures/claude-permission-allow.jsonl",
        ],
    },
    Scene {
        name: "agent-tab-done",
        run: agent_tab_done,
        themes: Themes::Both,
        serial: false,
        kind: Kind::State,
        sources: &[
            "src/agent_tab.rs",
            "src/agent_conversation.rs",
            "src/agent_markdown.rs",
            "src/composer.rs",
            "src/md_preview.rs",
            "src/diff_view.rs",
            "src/file_tab.rs",
            "src/palette_files.rs",
            "src/providers/mod.rs",
            "src/providers/claude.rs",
            "src/providers/fixtures/claude-text.jsonl",
            "src/providers/fixtures/claude-tools.jsonl",
        ],
    },
    Scene {
        name: "agent-tab-failed",
        run: agent_tab_failed,
        themes: Themes::Both,
        serial: false,
        kind: Kind::State,
        sources: &[
            "src/agent_tab.rs",
            "src/agent_conversation.rs",
            "src/agent_markdown.rs",
            "src/composer.rs",
            "src/md_preview.rs",
            "src/diff_view.rs",
            "src/file_tab.rs",
            "src/palette_files.rs",
            "src/providers/mod.rs",
            "src/providers/claude.rs",
            "src/agent_process.rs",
        ],
    },
    Scene {
        name: "agent-tab-bad-resume",
        run: agent_tab_bad_resume,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: &[
            "src/agent_tab.rs",
            "src/agent_conversation.rs",
            "src/agent_markdown.rs",
            "src/composer.rs",
            "src/md_preview.rs",
            "src/diff_view.rs",
            "src/file_tab.rs",
            "src/palette_files.rs",
            "src/providers/mod.rs",
            "src/providers/claude.rs",
            "src/agent_process.rs",
            "src/providers/fixtures/claude-bad-resume.jsonl",
        ],
    },
    Scene {
        name: "agent-tab-long",
        run: agent_tab_long,
        themes: Themes::Both,
        serial: false,
        kind: Kind::State,
        sources: TAB,
    },
    Scene {
        name: "agent-tab-markdown",
        run: agent_tab_markdown,
        themes: Themes::Both,
        serial: false,
        kind: Kind::State,
        sources: TAB,
    },
    Scene {
        name: "agent-tab-cached-current",
        run: agent_tab_cached_current,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: TAB,
    },
    Scene {
        name: "agent-tab-at-files",
        run: agent_tab_at_files,
        themes: Themes::Both,
        serial: false,
        kind: Kind::State,
        sources: TAB,
    },
    Scene {
        name: "agent-tab-close",
        run: agent_tab_close,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: &[
            "src/agent_tab.rs",
            "src/agent_conversation.rs",
            "src/agent_markdown.rs",
            "src/composer.rs",
            "src/md_preview.rs",
            "src/diff_view.rs",
            "src/file_tab.rs",
            "src/palette_files.rs",
            "src/providers/mod.rs",
            "src/providers/claude.rs",
            "src/agent_process.rs",
            "src/layout_model.rs",
            "src/workspace_agent.rs",
        ],
    },
    Scene {
        name: "agent-tab-keys",
        run: agent_tab_keys,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: TAB,
    },
    Scene {
        name: "agent-tab-review",
        run: agent_tab_review,
        themes: Themes::One,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/agent_tab.rs",
            "src/agent_conversation.rs",
            "src/agent_markdown.rs",
            "src/composer.rs",
            "src/md_preview.rs",
            "src/diff_view.rs",
            "src/file_tab.rs",
            "src/palette_files.rs",
            "src/providers/mod.rs",
            "src/providers/claude.rs",
            "src/agent_process.rs",
        ],
    },
    Scene {
        name: "agent-tab-held",
        run: agent_tab_held,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: &[
            "src/agent_tab.rs",
            "src/agent_conversation.rs",
            "src/agent_markdown.rs",
            "src/composer.rs",
            "src/md_preview.rs",
            "src/diff_view.rs",
            "src/file_tab.rs",
            "src/palette_files.rs",
            "src/providers/mod.rs",
            "src/providers/claude.rs",
            "src/workspace_agent.rs",
        ],
    },
    Scene {
        name: "agent-workspace-split",
        run: agent_workspace_split,
        themes: Themes::Both,
        serial: false,
        kind: Kind::State,
        sources: &[
            "src/agent_tab.rs",
            "src/agent_conversation.rs",
            "src/agent_markdown.rs",
            "src/composer.rs",
            "src/md_preview.rs",
            "src/diff_view.rs",
            "src/file_tab.rs",
            "src/palette_files.rs",
            "src/providers/mod.rs",
            "src/providers/claude.rs",
            "src/workspace_agent.rs",
            "src/layout_model.rs",
            "src/presets.rs",
            "src/sidebar.rs",
            "src/tab_bar.rs",
            "src/providers/fixtures/claude-text.jsonl",
        ],
    },
    Scene {
        name: "agent-preset-menu",
        run: agent_preset_menu,
        themes: Themes::Both,
        serial: false,
        kind: Kind::State,
        sources: &[
            "src/sidebar.rs",
            "src/presets.rs",
            "src/workspace_agent.rs",
            "src/providers/mod.rs",
            "src/providers/claude.rs",
            "src/providers/agy.rs",
        ],
    },
    Scene {
        name: "agent-pane-menu",
        run: agent_pane_menu,
        themes: Themes::Both,
        serial: false,
        kind: Kind::State,
        sources: &[
            "src/pane_actions.rs",
            "src/overlay_panel.rs",
            "src/tab_bar.rs",
            "src/layout_model.rs",
        ],
    },
    Scene {
        name: "agent-sidebar-waiting",
        run: agent_sidebar_waiting,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/sidebar.rs",
            "src/workspace_agent.rs",
            "src/agent_tab.rs",
            "src/agent_conversation.rs",
            "src/notifier.rs",
            "src/tab_bar.rs",
            "src/providers/mod.rs",
            "src/providers/claude.rs",
        ],
    },
    Scene {
        name: "agent-tab-composer-wrap",
        run: agent_tab_composer_wrap,
        themes: Themes::Both,
        serial: false,
        kind: Kind::State,
        sources: TAB,
    },
    Scene {
        name: "agent-tab-attachment",
        run: agent_tab_attachment,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/agent_tab.rs",
            "src/agent_conversation.rs",
            "src/agent_markdown.rs",
            "src/composer.rs",
            "src/md_preview.rs",
            "src/diff_view.rs",
            "src/file_tab.rs",
            "src/palette_files.rs",
            "src/providers/mod.rs",
            "src/providers/claude.rs",
            "src/workspace_agent.rs",
            "src/block_view.rs",
            "src/blocks.rs",
            "src/shell_marks.rs",
            "src/terminal_view.rs",
            "src/terminal.rs",
            "src/tab_bar.rs",
            "src/layout_model.rs",
            "src/sidebar.rs",
            "src/providers/fixtures/claude-text.jsonl",
        ],
    },
    Scene {
        name: "agent-tab-runs",
        run: agent_tab_runs,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: &[
            "src/agent_tab.rs",
            "src/agent_conversation.rs",
            "src/agent_markdown.rs",
            "src/composer.rs",
            "src/md_preview.rs",
            "src/diff_view.rs",
            "src/file_tab.rs",
            "src/palette_files.rs",
            "src/providers/mod.rs",
            "src/providers/claude.rs",
            "src/workspace_agent.rs",
        ],
    },
    Scene {
        name: "agent-tab-agy",
        run: agent_tab_agy,
        themes: Themes::Both,
        serial: false,
        kind: Kind::State,
        sources: &[
            "src/agent_tab.rs",
            "src/agent_conversation.rs",
            "src/agent_markdown.rs",
            "src/composer.rs",
            "src/md_preview.rs",
            "src/diff_view.rs",
            "src/file_tab.rs",
            "src/palette_files.rs",
            "src/providers/mod.rs",
            "src/providers/agy.rs",
            "src/providers/fixtures/agy-text.jsonl",
            "src/providers/fixtures/agy-tools.jsonl",
            "src/providers/fixtures/agy-bad-resume.jsonl",
        ],
    },
];

type Slot = Rc<RefCell<Option<Entity<AgentTab>>>>;

const DIMS: (f32, f32) = (720.0, 640.0);

/// A folder under your home (so it reads `~/code/my-app` on any Mac); nothing is read from it.
fn cwd() -> PathBuf {
    crate::workspace::home_dir()
        .unwrap_or_else(|| PathBuf::from("/Users/me"))
        .join("code/my-app")
}

fn start() -> Start {
    Start {
        cwd: cwd(),
        resume: None,
        new_id: None,
        mode: None,
        extra_args: Vec::new(),
        model: None,
        effort: None,
    }
}

/// A fixture's lines, decoded as the process's would be.
fn decode(fixture: &str) -> Vec<Event> {
    let mut t = CLAUDE.translator();
    fixture.lines().flat_map(|l| t.decode(l)).collect()
}

fn tab_of(slot: &Slot) -> Entity<AgentTab> {
    slot.borrow().clone().expect("the agent tab")
}

/// The window's view: the tab, made with no process, the keyboard in its composer.
fn build(theme: crate::theme::Theme, slot: Slot) -> impl FnOnce(&mut Window, &mut App) -> AnyView {
    build_in(theme, slot, start())
}

/// `build`, starting as `start` says.
fn build_in(
    theme: crate::theme::Theme,
    slot: Slot,
    start: Start,
) -> impl FnOnce(&mut Window, &mut App) -> AnyView {
    build_with(&CLAUDE, theme, slot, start)
}

/// `build_in`, a tab of `provider`.
fn build_with(
    provider: &'static dyn Provider,
    theme: crate::theme::Theme,
    slot: Slot,
    start: Start,
) -> impl FnOnce(&mut Window, &mut App) -> AnyView {
    move |window, cx| {
        cx.set_global(shot_appearance(theme, cx));
        cx.bind_keys(crate::key_bindings());
        let tab = cx.new(|cx| AgentTab::for_shots(provider, start, window, cx));
        let focus = tab.read(cx).focus_handle(cx);
        focus.focus(window, cx);
        *slot.borrow_mut() = Some(tab.clone());
        tab.into()
    }
}

/// `act` on the tab, then let it settle (as the app would get its frames).
fn feed(
    cx: &mut VisualTestAppContext,
    tab: &Entity<AgentTab>,
    f: impl FnOnce(&mut AgentTab, &mut Context<AgentTab>),
) {
    cx.update(|cx| tab.update(cx, f));
    cx.run_until_parked();
}

/// `events` fed one by one as the process would bring them, each permission allowed as you
/// would (the recording's own answers aren't in its stdout).
fn play(cx: &mut VisualTestAppContext, tab: &Entity<AgentTab>, events: Vec<Event>) {
    for event in events {
        let ask = matches!(event, Event::Permission { .. });
        feed(cx, tab, |t, cx| t.shot_feed(vec![event], cx));
        if ask {
            feed(cx, tab, |t, cx| {
                t.answer(Answer::Allowed, cx).expect("waiting");
            });
        }
    }
}

/// Shoot scene `name` in both themes: `act` sets the tab up (and checks it) before the shot.
fn scene(
    name: &str,
    act: impl Fn(&str, &mut VisualTestAppContext, AnyWindowHandle, &Entity<AgentTab>) + Clone,
) {
    for &(slug, theme_name) in shot_themes() {
        let theme = shot_theme(theme_name);
        let slot: Slot = Rc::default();
        let n = format!("{name}-{slug}");
        let act = act.clone();
        let keep = slot.clone();
        let path = shoot_with(
            name,
            slug,
            &theme,
            size(px(DIMS.0), px(DIMS.1)),
            build(theme, slot),
            move |cx, window| {
                let tab = tab_of(&keep);
                act(&n, cx, window, &tab);
                keep.borrow_mut().take();
            },
        );
        println!("ui-shots: {}", path.display());
    }
}

/// The empty tab: the line asking for a task, the composer with the keyboard.
pub(super) fn agent_tab_empty() {
    scene("agent-tab-empty", |n, cx, window, tab| {
        cx.update_window(window, |_, window, cx| {
            let t = tab.read(cx);
            assert_eq!(t.status(), Status::Idle, "{n}");
            assert!(t.shot_items().is_empty(), "{n}");
            assert!(
                t.focus_handle(cx).is_focused(window),
                "{n}: the composer has the keyboard"
            );
        })
        .unwrap();
    });
}

/// The tools fixture up to its Bash call (still running), your message first, a checklist
/// (synthetic: this Claude build has no TodoWrite) and the edit opened to its diff.
pub(super) fn agent_tab_working() {
    scene("agent-tab-working", |n, cx, _, tab| {
        let events = decode(include_str!("../providers/fixtures/claude-tools.jsonl"));
        let bash = events
            .iter()
            .position(|e| matches!(e, Event::ToolStarted { name, summary, .. } if name == "Bash" && summary == "ls"))
            .expect("the ls call");
        let mut first: Vec<Event> = events[..=bash].to_vec();
        first.insert(
            1,
            Event::Todo(vec![
                TodoItem {
                    text: "Read notes.txt".into(),
                    state: TodoState::Done,
                },
                TodoItem {
                    text: "Change alpha to beta".into(),
                    state: TodoState::Active,
                },
                TodoItem {
                    text: "List the folder".into(),
                    state: TodoState::Pending,
                },
            ]),
        );
        feed(cx, tab, |t, cx| {
            t.send(
                "Read notes.txt, change alpha to beta, then run ls".into(),
                cx,
            )
        });
        play(cx, tab, first);
        // Open the last edit (the one that went through) to its diff.
        let edit = cx.read(|cx| {
            tab.read(cx)
                .shot_items()
                .iter()
                .rposition(
                    |i| matches!(i, Item::Tool { name, ok: Some(true), .. } if name == "Edit"),
                )
                .expect("an Edit row")
        });
        feed(cx, tab, |t, cx| t.shot_toggle(edit, cx));
        cx.read(|cx| {
            let t = tab.read(cx);
            assert_eq!(t.status(), Status::Working, "{n}");
            assert_eq!(t.shot_stop_label(), Some("■ Stop  esc"), "{n}");
            assert!(
                t.shot_items()
                    .iter()
                    .any(|i| matches!(i, Item::Tool { name, running: true, .. } if name == "Bash")),
                "{n}: Bash runs"
            );
        });
    });
}

/// A Write waiting for your answer: the card, its buttons, the composer's deny hint. Then
/// (not shot) Enter on the empty composer allows it, as the Composer's own key does.
pub(super) fn agent_tab_permission() {
    let to_card = |cx: &mut VisualTestAppContext, tab: &Entity<AgentTab>| {
        let events = decode(include_str!(
            "../providers/fixtures/claude-permission-allow.jsonl"
        ));
        let card = events
            .iter()
            .position(|e| matches!(e, Event::Permission { .. }))
            .expect("a permission");
        feed(cx, tab, |t, cx| {
            t.send("Create hello.txt containing the word hi".into(), cx)
        });
        feed(cx, tab, |t, cx| t.shot_feed(events[..=card].to_vec(), cx));
    };
    scene("agent-tab-permission", move |n, cx, _, tab| {
        to_card(cx, tab);
        cx.read(|cx| {
            let t = tab.read(cx);
            assert_eq!(t.status(), Status::Waiting, "{n}");
            assert_eq!(
                t.question().as_deref(),
                Some("Claude wants to run: hello.txt"),
                "{n}"
            );
            let composer = t.shot_composer();
            assert_eq!(
                composer.read(cx).placeholder().as_ref(),
                "Deny and say why… (optional)",
                "{n}"
            );
            assert!(t.shot_items().iter().any(|i| matches!(
                i,
                Item::Permission { always: Some(a), answer: None, .. } if a == "Allow all edits this session"
            )));
        });
    });
    // Enter on the empty composer: Allow. Text then Enter: Deny with it.
    let theme = shot_theme("Catppuccin Mocha");
    for (typed, want) in [
        ("", Answer::Allowed),
        ("not now", Answer::Denied(Some("not now".into()))),
    ] {
        let slot: Slot = Rc::default();
        let keep = slot.clone();
        render_with(
            &theme,
            size(px(DIMS.0), px(DIMS.1)),
            build(theme, slot),
            move |cx, window| {
                let tab = tab_of(&keep);
                to_card(cx, &tab);
                if !typed.is_empty() {
                    cx.simulate_input(window, typed);
                }
                cx.simulate_keystrokes(window, "enter");
                cx.run_until_parked();
                cx.read(|cx| {
                    let t = tab.read(cx);
                    assert!(
                        t.shot_items().iter().any(|i| matches!(
                            i,
                            Item::Permission { answer: Some(a), .. } if *a == want
                        )),
                        "agent-tab-permission: enter with {typed:?} answers {want:?}"
                    );
                    assert_eq!(t.status(), Status::Working);
                    assert_eq!(t.question(), None);
                    assert_eq!(t.shot_composer().read(cx).text(), "", "the box is cleared");
                });
                keep.borrow_mut().take();
            },
        );
    }
}

/// A finished turn: the reply, the model and cost chips, the context line.
pub(super) fn agent_tab_done() {
    scene("agent-tab-done", |n, cx, _, tab| {
        let events = decode(include_str!("../providers/fixtures/claude-text.jsonl"));
        feed(cx, tab, |t, cx| t.send("Say hello in one word".into(), cx));
        feed(cx, tab, |t, cx| t.shot_feed(events, cx));
        let tools = decode(include_str!("../providers/fixtures/claude-tools.jsonl"));
        feed(cx, tab, |t, cx| t.send("Now fix notes.txt".into(), cx));
        play(cx, tab, tools);
        cx.read(|cx| {
            let t = tab.read(cx);
            assert_eq!(t.status(), Status::Done, "{n}");
            assert_eq!(t.shot_stop_label(), None, "{n}");
            assert!(
                t.shot_items()
                    .iter()
                    .all(|i| !matches!(i, Item::Tool { running: true, .. })),
                "{n}"
            );
        });
    });
}

/// An Antigravity tab (M11aa Task 12), fed agy's own recordings through its own decoder: its
/// mode is chosen through the box's menu (`shot_choose_mode`) before and after the first
/// message (no lock); the tools pair up and the turns end Done. (The replies' AG avatar is in the picture only: this
/// scene reads view state, not pixels.)
pub(super) fn agent_tab_agy() {
    use crate::providers::agy::AGY;
    let name = "agent-tab-agy";
    let decode = |fixture: &str| -> Vec<Event> {
        let mut t = AGY.translator();
        fixture.lines().flat_map(|l| t.decode(l)).collect()
    };
    for &(slug, theme_name) in shot_themes() {
        let theme = shot_theme(theme_name);
        let slot: Slot = Rc::default();
        let keep = slot.clone();
        let n = format!("{name}-{slug}");
        let start = Start {
            mode: AGY.info().default_mode.map(String::from),
            ..start()
        };
        let path = shoot_with(
            name,
            slug,
            &theme,
            size(px(DIMS.0), px(DIMS.1)),
            build_with(&AGY, theme, slot, start),
            move |cx, _| {
                let tab = tab_of(&keep);
                let mode = |cx: &mut VisualTestAppContext| {
                    cx.read(|cx| tab.read(cx).mode().map(str::to_string))
                };
                assert_eq!(
                    mode(cx).as_deref(),
                    Some("accept-edits"),
                    "{n}: the default"
                );
                // Before the first message the menu's choices apply, one after another.
                for want in ["auto", "plan", "accept-edits"] {
                    feed(cx, &tab, |t, cx| t.shot_choose_mode(want, cx));
                    assert_eq!(mode(cx).as_deref(), Some(want), "{n}");
                }
                feed(cx, &tab, |t, cx| t.send("Say hello in one word".into(), cx));
                feed(cx, &tab, |t, cx| {
                    t.shot_feed(
                        decode(include_str!("../providers/fixtures/agy-text.jsonl")),
                        cx,
                    )
                });
                feed(cx, &tab, |t, cx| t.send("Now fix notes.txt".into(), cx));
                feed(cx, &tab, |t, cx| {
                    t.shot_feed(
                        decode(include_str!("../providers/fixtures/agy-tools.jsonl")),
                        cx,
                    )
                });
                // After it the mode can still be chosen (agy restarts on the same conversation).
                feed(cx, &tab, |t, cx| t.shot_choose_mode("plan", cx));
                assert_eq!(mode(cx).as_deref(), Some("plan"), "{n}: no lock");
                feed(cx, &tab, |t, cx| t.shot_choose_mode("accept-edits", cx));
                cx.read(|cx| {
                    let t = tab.read(cx);
                    assert_eq!(t.status(), Status::Done, "{n}");
                    let items = t.shot_items();
                    let tools = items
                        .iter()
                        .filter(|i| matches!(i, Item::Tool { running: false, .. }))
                        .count();
                    assert_eq!(tools, 4, "{n}: read, edit, ls, task list");
                    assert!(
                        items
                            .iter()
                            .all(|i| !matches!(i, Item::Tool { running: true, .. })),
                        "{n}"
                    );
                    let replies = items
                        .iter()
                        .filter(|i| matches!(i, Item::Agent { done: true, .. }))
                        .count();
                    assert_eq!(replies, 2, "{n}: Hello, and Done.");
                });
                keep.borrow_mut().take();
            },
        );
        println!("ui-shots: {}", path.display());
    }
    agy_dead_conversation_and_stop(name, decode);
}

/// Checks only: a restored agy tab whose conversation agy no longer has (agy goes on in a new
/// one: `agy-bad-resume.jsonl`) says so in place of "restored" and keeps the new id; and Stop,
/// agy having no interrupt (`ProviderInfo::interrupts`), offers Force stop at once.
fn agy_dead_conversation_and_stop(name: &str, decode: impl Fn(&str) -> Vec<Event> + 'static) {
    use crate::agent_tab::RESTORED_NOTICE;
    use crate::providers::agy::AGY;
    const ZERO: &str = "00000000-0000-0000-0000-000000000000";
    let n = format!("{name}-dead-conversation");
    let theme = shot_theme("Catppuccin Mocha");
    let slot: Slot = Rc::default();
    let keep = slot.clone();
    render_with(
        &theme,
        size(px(DIMS.0), px(DIMS.1)),
        move |window, cx| {
            cx.set_global(shot_appearance(theme, cx));
            let resume = Start {
                resume: Some(ZERO.into()),
                mode: AGY.info().default_mode.map(String::from),
                ..start()
            };
            let tab = cx.new(|cx| AgentTab::for_shots(&AGY, resume, window, cx));
            *keep.borrow_mut() = Some(tab.clone());
            tab.into()
        },
        move |cx, _| {
            let tab = tab_of(&slot);
            // agy keeps no history fuxx reads: the "restored" notice.
            feed(cx, &tab, |t, cx| t.shot_history(Vec::new(), cx));
            cx.read(|cx| {
                let items = tab.read(cx).shot_items();
                assert!(
                    matches!(items, [Item::Notice(a)] if a == RESTORED_NOTICE),
                    "{n}: {items:?}"
                );
            });
            let events = decode(include_str!("../providers/fixtures/agy-bad-resume.jsonl"));
            assert!(
                matches!(events.first(), Some(Event::Started { conversation, .. }) if conversation != ZERO),
                "{n}: agy began another conversation"
            );
            feed(cx, &tab, |t, cx| t.send("Say hello in one word".into(), cx));
            feed(cx, &tab, |t, cx| t.shot_feed(events, cx));
            cx.read(|cx| {
                let t = tab.read(cx);
                let items = t.shot_items();
                assert!(
                    matches!(items.first(), Some(Item::Notice(a)) if a == BAD_RESUME_NOTICE),
                    "{n}: the notice says it's a new one: {items:?}"
                );
                assert!(
                    !items
                        .iter()
                        .any(|i| matches!(i, Item::Notice(a) if a == RESTORED_NOTICE)),
                    "{n}: not restored: {items:?}"
                );
                assert!(
                    items.iter().any(|i| matches!(i, Item::You { .. })),
                    "{n}: your message stays"
                );
                assert_eq!(t.shot_resume(), None, "{n}: the dead id is forgotten");
                assert_eq!(t.status(), Status::Done, "{n}");
            });
            // A new turn, then Stop: Force stop is offered at once (no "Stopping…").
            feed(cx, &tab, |t, cx| t.send("Write a long essay".into(), cx));
            cx.read(|cx| assert_eq!(tab.read(cx).status(), Status::Working, "{n}"));
            feed(cx, &tab, |t, cx| t.shot_press_stop(cx));
            cx.read(|cx| assert_eq!(tab.read(cx).shot_stop_label(), Some("Force stop"), "{n}"));
            slot.borrow_mut().take();
        },
    );
    println!("ui-shots: {name}: a dead conversation is a new one; Stop offers Force stop at once");
}

/// The agent exits 1 ("not logged in"): one error row with Open in classic view, status Done.
/// And a tab for an agent fuxx doesn't know: its error and Close.
pub(super) fn agent_tab_failed() {
    scene("agent-tab-failed", |n, cx, _, tab| {
        feed(cx, tab, |t, cx| t.send("Say hello in one word".into(), cx));
        feed(cx, tab, |t, cx| {
            t.shot_feed(
                vec![Event::Started {
                    conversation: "ed50a2b7-6678-4a7c-9ed9-cfa63240ae19".into(),
                    model: Some("claude-opus-5-5".into()),
                }],
                cx,
            )
        });
        feed(cx, tab, |t, cx| {
            t.shot_process(ProcessEvent::Ended { code: Some(1) }, cx)
        });
        feed(cx, tab, |t, cx| {
            t.shot_process(
                ProcessEvent::Exited {
                    code: Some(1),
                    stderr_tail: "not logged in".into(),
                },
                cx,
            )
        });
        cx.read(|cx| {
            let t = tab.read(cx);
            assert_eq!(t.status(), Status::Done, "{n}");
            let errors: Vec<&String> = t
                .shot_items()
                .iter()
                .filter_map(|i| match i {
                    Item::Error(e) => Some(e),
                    _ => None,
                })
                .collect();
            assert_eq!(errors.len(), 1, "{n}: one error row, {errors:?}");
            assert!(errors[0].contains("not logged in"), "{n}");
        });
    });
    for &(slug, theme_name) in shot_themes() {
        let theme = shot_theme(theme_name);
        let path = shoot_with(
            "agent-tab-failed-unknown",
            slug,
            &theme,
            size(px(DIMS.0), px(DIMS.1)),
            move |window: &mut Window, cx: &mut App| -> AnyView {
                cx.set_global(shot_appearance(theme, cx));
                // No provider: no process is started.
                let tab = cx.new(|cx| {
                    AgentTab::new(
                        None,
                        "gone".into(),
                        start(),
                        Box::new(|| crate::workspace_agent::Classic::Closed),
                        window,
                        cx,
                    )
                });
                assert_eq!(tab.read(cx).status(), Status::Done);
                tab.into()
            },
            |_, _| {},
        );
        println!("ui-shots: {}", path.display());
    }
}

/// A resume that fails (the process ends before saying it started): what it said goes, one
/// notice, a new conversation (no process in a scene: the respawn is a no-op here).
pub(super) fn agent_tab_bad_resume() {
    let theme = shot_theme("Catppuccin Mocha");
    let slot: Slot = Rc::default();
    let keep = slot.clone();
    render_with(
        &theme,
        size(px(DIMS.0), px(DIMS.1)),
        move |window, cx| {
            cx.set_global(shot_appearance(theme, cx));
            let resume = Start {
                resume: Some("00000000-0000-0000-0000-000000000000".into()),
                ..start()
            };
            let tab = cx.new(|cx| AgentTab::for_shots(&CLAUDE, resume, window, cx));
            *keep.borrow_mut() = Some(tab.clone());
            tab.into()
        },
        move |cx, _| {
            let tab = tab_of(&slot);
            let events = decode(include_str!(
                "../providers/fixtures/claude-bad-resume.jsonl"
            ));
            // No transcript: the "restored" notice; then the run fails before it starts.
            feed(cx, &tab, |t, cx| t.shot_history(Vec::new(), cx));
            feed(cx, &tab, |t, cx| t.shot_feed(events, cx));
            feed(cx, &tab, |t, cx| {
                t.shot_process(
                    ProcessEvent::Exited {
                        code: Some(1),
                        stderr_tail: "No conversation found".into(),
                    },
                    cx,
                )
            });
            cx.read(|cx| {
                let t = tab.read(cx);
                let items = t.shot_items();
                assert!(
                    matches!(items, [Item::Notice(a)] if a == BAD_RESUME_NOTICE),
                    "agent-tab-bad-resume: {items:?}"
                );
                assert_eq!(t.status(), Status::Done);
            });
            slot.borrow_mut().take();
        },
    );
    println!("ui-shots: agent-tab-bad-resume: one notice, a new conversation");
}

/// 300 rows and a tool that printed 100,000 lines, scrolled to the end: only the rows in sight
/// are laid out.
pub(super) fn agent_tab_long() {
    scene("agent-tab-long", |n, cx, window, tab| {
        for i in 0..100 {
            feed(cx, tab, |t, cx| {
                t.send(format!("Question {i}: what is in file {i}?"), cx)
            });
            let id = format!("toolu_{i}");
            let mut events = vec![
                Event::ToolStarted {
                    id: id.clone(),
                    name: "Read".into(),
                    summary: format!("file{i}.txt"),
                    parent: None,
                    detail: None,
                    writes: Vec::new(),
                    texts: Vec::new(),
                },
                Event::ToolFinished {
                    id: id.clone(),
                    ok: true,
                    summary: "12 lines".into(),
                    output: Some("note\n".repeat(12)),
                },
                Event::MessageDone {
                    message: format!("msg_{i}"),
                    text: format!("File {i} holds **twelve** lines of notes."),
                    parent: None,
                },
            ];
            if i == 99 {
                let output: String = (0..100_000).map(|l| format!("line {l}\n")).collect();
                events.push(Event::ToolStarted {
                    id: "toolu_big".into(),
                    name: "Bash".into(),
                    summary: "seq 100000".into(),
                    parent: None,
                    detail: Some(crate::providers::ToolDetail::Command("seq 100000".into())),
                    writes: Vec::new(),
                    texts: Vec::new(),
                });
                events.push(Event::ToolFinished {
                    id: "toolu_big".into(),
                    ok: true,
                    summary: "line 0".into(),
                    output: Some(output),
                });
            }
            events.push(Event::TurnDone {
                ok: true,
                message: None,
            });
            feed(cx, tab, |t, cx| t.shot_feed(events, cx));
        }
        let built = cx.read(|cx| tab.read(cx).rows_built.clone());
        built.set(0);
        let _ = frame_fresh(cx, window);
        let laid_out = built.get();
        cx.read(|cx| {
            let t = tab.read(cx);
            let items = t.shot_items();
            assert!(items.len() >= 300, "{n}: {} items", items.len());
            assert!(
                items.iter().any(
                    |i| matches!(i, Item::Tool { dropped_lines, .. } if *dropped_lines >= 98_000)
                ),
                "{n}: the big output keeps its tail"
            );
            let list = t.shot_list();
            assert!(list.is_following_tail(), "{n}: at the end");
            println!(
                "ui-shots: {n}: {} items, {laid_out} rows laid out",
                items.len()
            );
        });
        assert!(laid_out > 0, "{n}: something was drawn");
        assert!(laid_out < 40, "{n}: {laid_out} rows laid out");
        // The big output opened: still drawn (its kept tail), the rows still few.
        let big = cx.read(|cx| {
            tab.read(cx)
                .shot_items()
                .iter()
                .position(|i| matches!(i, Item::Tool { id, .. } if id == "toolu_big"))
                .expect("the big tool")
        });
        feed(cx, tab, |t, cx| t.shot_toggle(big, cx));
        feed(cx, tab, |t, cx| t.shot_toggle(big, cx));
    });
}

/// Markdown in a reply, drawn by `md_preview::column`: a heading, a list, a table, a quote and
/// code (a Mermaid block shows as code, R7).
pub(super) fn agent_tab_markdown() {
    const REPLY: &str = "## What changed\n\nThe test expects past dates to be rejected; \
`isPastDate` compared **strings**. See [the docs](https://example.com/dates).\n\n\
- Parse dates before comparing\n- Keep `today()` as it is\n  - nested note\n\n\
| File | Lines |\n|:--|--:|\n| src/rsvp.ts | +3 −1 |\n| test/rsvp.test.ts | 0 |\n\n\
> Dates like \"Feb 1\" now parse too.\n\n\
```ts\nconst d = parseDate(date)\nif (!d) return false\nreturn d.getTime() < today().getTime()\n```\n\n\
```mermaid\ngraph TD; A-->B\n```\n";
    scene("agent-tab-markdown", |n, cx, _, tab| {
        feed(cx, tab, |t, cx| t.send("Explain the fix".into(), cx));
        feed(cx, tab, |t, cx| {
            t.shot_feed(
                vec![
                    Event::MessageDone {
                        message: "msg_md".into(),
                        text: REPLY.into(),
                        parent: None,
                    },
                    Event::TurnDone {
                        ok: true,
                        message: None,
                    },
                ],
                cx,
            )
        });
        cx.read(|cx| assert_eq!(tab.read(cx).status(), Status::Done, "{n}"));
    });
}

/// The view embedded `.cached(size_full)` as the Workspace embeds it (Task 8).
struct Host {
    tab: Entity<AgentTab>,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(
            self.tab
                .clone()
                .cached(StyleRefinement::default().size_full()),
        )
    }
}

/// Drawn cached, each change the tab shows reaches the window: a delta, a tool's finish and a
/// permission, each followed only by the frames the app would get; the window then equals a
/// full redraw and differs from before.
pub(super) fn agent_tab_cached_current() {
    let n = "agent-tab-cached-current";
    for &(slug, theme_name) in shot_themes() {
        let theme = shot_theme(theme_name);
        let mut cx = VisualTestAppContext::new(gpui_platform::current_platform(false));
        crate::fonts::register_bundled_fonts(&cx.text_system().all_font_names());
        let slot: Slot = Rc::default();
        let keep = slot.clone();
        let window = cx
            .open_offscreen_window(size(px(DIMS.0), px(DIMS.1)), move |window, cx| {
                cx.set_global(shot_appearance(theme, cx));
                let tab = cx.new(|cx| AgentTab::for_shots(&CLAUDE, start(), window, cx));
                *keep.borrow_mut() = Some(tab.clone());
                let host = cx.new(|_| Host { tab });
                cx.new(|_| super::Root {
                    theme,
                    scene: host.into(),
                })
            })
            .expect("open an off-screen window");
        let window: AnyWindowHandle = window.into();
        cx.run_until_parked();
        let tab = tab_of(&slot);
        feed(&mut cx, &tab, |t, cx| t.send("Run the tests".into(), cx));
        feed(&mut cx, &tab, |t, cx| {
            t.shot_feed(
                vec![
                    Event::Started {
                        conversation: "c1".into(),
                        model: Some("claude-opus-5-5".into()),
                    },
                    Event::TextDelta {
                        message: "m1".into(),
                        text: "Running the".into(),
                        parent: None,
                    },
                    Event::ToolStarted {
                        id: "t1".into(),
                        name: "Bash".into(),
                        summary: "npm test".into(),
                        parent: None,
                        detail: Some(crate::providers::ToolDetail::Command("npm test".into())),
                        writes: Vec::new(),
                        texts: Vec::new(),
                    },
                ],
                cx,
            )
        });
        settle_frames(&mut cx, window);
        let _ = frame_fresh(&mut cx, window);
        settle_frames(&mut cx, window);
        let steps: Vec<(&str, Vec<Event>)> = vec![
            (
                "a delta",
                vec![Event::TextDelta {
                    message: "m1".into(),
                    text: " tests now; this takes a moment.".into(),
                    parent: None,
                }],
            ),
            (
                "a tool's finish",
                vec![Event::ToolFinished {
                    id: "t1".into(),
                    ok: true,
                    summary: "✓ 4 passed".into(),
                    output: Some("✓ 4 passed".into()),
                }],
            ),
            (
                "a permission",
                vec![Event::Permission {
                    id: "p1".into(),
                    tool: "Bash".into(),
                    what: "git commit -m \"fix rsvp\"".into(),
                    detail: String::new(),
                    always: Some("Always allow Bash(git commit:*)".into()),
                }],
            ),
        ];
        for (k, (what, events)) in steps.into_iter().enumerate() {
            let before = shown(&mut cx, window);
            feed(&mut cx, &tab, |t, cx| t.shot_feed(events, cx));
            settle_frames(&mut cx, window);
            let now = shown(&mut cx, window);
            let fresh = frame_fresh(&mut cx, window);
            let stale = bytes_differ(&now, &fresh);
            let changed = bytes_differ(&before, &fresh);
            println!("ui-shots: {n}-{slug}: {what}: {changed} bytes changed, {stale} stale");
            if stale > 0 {
                keep_shot(&format!("{n}-{slug}-{k}-stale-now"), &now);
                keep_shot(&format!("{n}-{slug}-{k}-stale-fresh"), &fresh);
            }
            assert!(changed > 0, "{n}-{slug}: {what}: nothing shown changed");
            assert_eq!(stale, 0, "{n}-{slug}: {what}: the window is stale");
        }
        save_shot(n, slug, &shown(&mut cx, window));
        slot.borrow_mut().take();
    }
}

/// The `@` list over the composer: the files ranked for what follows the `@`.
pub(super) fn agent_tab_at_files() {
    scene("agent-tab-at-files", |n, cx, _, tab| {
        let paths: Vec<String> = [
            "src/rsvp.ts",
            "src/rsvp_dates.ts",
            "test/rsvp.test.ts",
            "src/main.ts",
            "README.md",
            "package.json",
        ]
        .map(String::from)
        .to_vec();
        feed(cx, tab, |t, cx| t.shot_at(cwd(), paths, "rsvp", cx));
        let until = Instant::now() + Duration::from_secs(5);
        while cx.read(|cx| tab.read(cx).at_hits()) == 0 {
            assert!(Instant::now() < until, "{n}: the files were never ranked");
            std::thread::sleep(Duration::from_millis(20));
            cx.run_until_parked();
        }
        assert_eq!(cx.read(|cx| tab.read(cx).at_hits()), 3, "{n}");
    });
}

/// The Workspace's side (Task 5's carry-over): an agent tab beside a terminal takes the
/// keyboard when focused, and closing it ends its process, drops its view and its map entry.
pub(super) fn agent_tab_close() {
    use crate::layout_model::TabKind;
    use crate::workspace::Workspace;

    let n = "agent-tab-close";
    let theme = shot_theme("Catppuccin Mocha");
    type WsSlot = Rc<RefCell<Option<Entity<Workspace>>>>;
    let slot: WsSlot = Rc::default();
    let keep = slot.clone();
    render_with(
        &theme,
        size(px(900.0), px(560.0)),
        move |window, cx| {
            cx.set_global(shot_appearance(theme, cx));
            cx.bind_keys(crate::key_bindings());
            let ws = cx.new(|cx| {
                let mut ws = Workspace::for_shots(window, cx);
                let project = std::path::Path::new("/Users/me/code/api");
                ws.open_shot_session("api", project, super::SHELL_TEXT, None, window, cx);
                let pane = ws.model.focused_pane().expect("a pane");
                // A folder that exists: the stand-in process starts in it. Its view is made by
                // `ensure_agent_tabs` on the first frame (with no agent: the scenes').
                ws.model
                    .add_tab(
                        pane,
                        TabKind::Agent {
                            provider: "claude".into(),
                            cwd: std::env::temp_dir(),
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
            cx.run_until_parked();
            let view = cx.read(|cx| {
                let ws = ws.read(cx);
                let tab = ws.model.focused_tab().expect("a tab");
                ws.agents
                    .get(&tab)
                    .expect("its slot, made on the first frame")
                    .view
                    .clone()
            });
            cx.update(|cx| {
                view.update(cx, |t, cx| {
                    t.shot_spawn(["/bin/sh", "-c", "sleep 30"].map(String::from).to_vec(), cx)
                })
            });
            drop(view);
            // The process's pid is published by its thread once it has started.
            let until = Instant::now() + Duration::from_secs(4);
            loop {
                let started = cx.read(|cx| {
                    let ws = ws.read(cx);
                    let tab = ws.model.focused_tab().expect("a tab");
                    ws.agents[&tab]
                        .view
                        .read(cx)
                        .process()
                        .and_then(|p| p.pid())
                        .is_some()
                });
                if started {
                    break;
                }
                assert!(Instant::now() < until, "{n}: its process never started");
                std::thread::sleep(Duration::from_millis(20));
            }
            let (tab, view, keys, pid) = cx
                .update_window(window, |_, window, cx| {
                    let ws = ws.read(cx);
                    let tab = ws.model.focused_tab().expect("a tab");
                    let view = ws.agents.get(&tab).expect("its view").view.clone();
                    let keys = view.read(cx).focus_handle(cx).is_focused(window);
                    let pid = view
                        .read(cx)
                        .process()
                        .and_then(|p| p.pid())
                        .expect("its process runs");
                    (tab, view.downgrade(), keys, pid)
                })
                .unwrap();
            assert!(keys, "{n}: the focused agent tab has the keyboard");
            let running = |pid: i32| {
                std::process::Command::new("/bin/kill")
                    .args(["-0", &pid.to_string()])
                    .status()
                    .is_ok_and(|s| s.success())
            };
            assert!(running(pid), "{n}: its process runs");
            cx.update_window(window, |_, window, cx| {
                ws.update(cx, |ws, cx| ws.close_tab(tab, window, cx))
            })
            .unwrap();
            cx.run_until_parked();
            cx.read(|cx| {
                let ws = ws.read(cx);
                assert!(!ws.agents.contains_key(&tab), "{n}: its map entry went");
                assert!(ws.model.tab(tab).is_none(), "{n}: the tab went");
            });
            assert!(view.upgrade().is_none(), "{n}: its view was dropped");
            let until = Instant::now() + Duration::from_secs(4);
            while running(pid) {
                assert!(Instant::now() < until, "{n}: process {pid} still runs");
                std::thread::sleep(Duration::from_millis(50));
            }
            slot.borrow_mut().take();
        },
    );
    println!("ui-shots: {n}: closed, its process ended");
}

/// The composer soft-wraps a long message, keeps the caret's row in view past `MAX_ROWS`, and
/// the wheel scrolls it.
pub(super) fn agent_tab_composer_wrap() {
    use gpui::{Modifiers, ScrollDelta, ScrollWheelEvent, TouchPhase, point};
    scene("agent-tab-composer-wrap", |n, cx, window, tab| {
        let composer = cx.read(|cx| tab.read(cx).shot_composer());
        let line = "Please read src/rsvp.ts and the failing test, then explain why past dates are \
                    accepted and fix the comparison without changing today() at all. ";
        cx.simulate_input(window, &line.repeat(4));
        cx.run_until_parked();
        let _ = frame_fresh(cx, window);
        let (rows, first, bounds) = cx.read(|cx| composer.read(cx).shot_rows());
        assert!(rows > 8, "{n}: the long line wraps into rows ({rows})");
        assert!(first > 0, "{n}: scrolled to keep the caret in view");
        let bounds = bounds.expect("drawn");
        cx.simulate_event(
            window,
            ScrollWheelEvent {
                position: bounds.center(),
                delta: ScrollDelta::Pixels(point(px(0.0), px(400.0))),
                modifiers: Modifiers::none(),
                touch_phase: TouchPhase::Moved,
            },
        );
        cx.run_until_parked();
        let _ = frame_fresh(cx, window);
        let (_, after, _) = cx.read(|cx| composer.read(cx).shot_rows());
        assert_eq!(after, 0, "{n}: the wheel scrolled it to the top");
    });
}

/// Keys, no shot: in the `@` list ↓ then Enter puts the second file in place of `@rsvp`; Esc
/// while the turn runs reads "Stopping…", offers Force stop after `STOP_FORCE_AFTER`, and the
/// turn's end clears it.
pub(super) fn agent_tab_keys() {
    let n = "agent-tab-keys";
    let theme = shot_theme("Catppuccin Mocha");
    let slot: Slot = Rc::default();
    let keep = slot.clone();
    render_with(
        &theme,
        size(px(DIMS.0), px(DIMS.1)),
        build(theme, slot),
        move |cx, window| {
            let tab = tab_of(&keep);
            let paths: Vec<String> = ["src/rsvp.ts", "src/rsvp_dates.ts", "README.md"]
                .map(String::from)
                .to_vec();
            feed(cx, &tab, |t, cx| t.shot_at(cwd(), paths, "rsvp", cx));
            let until = Instant::now() + Duration::from_secs(5);
            while cx.read(|cx| tab.read(cx).at_hits()) == 0 {
                assert!(Instant::now() < until, "{n}: the files were never ranked");
                std::thread::sleep(Duration::from_millis(20));
                cx.run_until_parked();
            }
            let _ = frame_fresh(cx, window);
            cx.simulate_keystrokes(window, "down enter");
            cx.run_until_parked();
            cx.read(|cx| {
                let t = tab.read(cx);
                assert_eq!(t.at_hits(), 0, "{n}: the list closed");
                assert_eq!(
                    t.shot_composer().read(cx).text(),
                    "src/rsvp_dates.ts ",
                    "{n}: the picked path"
                );
                assert!(t.shot_items().is_empty(), "{n}: nothing was sent");
            });
            // Stop.
            feed(cx, &tab, |t, cx| t.send("Run the tests".into(), cx));
            let _ = frame_fresh(cx, window);
            cx.simulate_keystrokes(window, "escape");
            cx.run_until_parked();
            cx.read(|cx| assert_eq!(tab.read(cx).shot_stop_label(), Some("Stopping…"), "{n}"));
            cx.advance_clock(crate::agent_tab::STOP_FORCE_AFTER + Duration::from_millis(100));
            cx.run_until_parked();
            cx.read(|cx| assert_eq!(tab.read(cx).shot_stop_label(), Some("Force stop"), "{n}"));
            feed(cx, &tab, |t, cx| {
                t.shot_feed(
                    vec![
                        Event::Notice("Stopped".into()),
                        Event::TurnDone {
                            ok: false,
                            message: None,
                        },
                    ],
                    cx,
                )
            });
            cx.read(|cx| {
                let t = tab.read(cx);
                assert_eq!(t.shot_stop_label(), None, "{n}");
                assert_eq!(t.status(), Status::Done, "{n}");
            });
            keep.borrow_mut().take();
        },
    );
    println!("ui-shots: {n}: @ pick and Stop");
}

/// Whether the top `frac` of two shots of one window differ (the stream, not the composer).
fn top_differs(a: &super::Shot, b: &super::Shot, frac: f32) -> bool {
    let rows = (a.height as f32 * frac) as usize;
    let n = rows * a.width as usize * 4;
    a.rgba[..n] != b.rgba[..n]
}

/// Review fixes (fix round 1), no shot: a message sent while the tab boots shows at once and is
/// sent after the history; Force stop ends the process and starts it again resuming the same
/// conversation; a resume that fails for another reason keeps the conversation; a message sent
/// between the process's end and its report waits and goes to the restarted process; a spawn
/// failure says which agent.
pub(super) fn agent_tab_review() {
    let n = "agent-tab-review";
    let theme = shot_theme("Catppuccin Mocha");
    let dims = size(px(DIMS.0), px(DIMS.1));
    let errors = |t: &AgentTab| -> Vec<String> {
        t.shot_items()
            .iter()
            .filter_map(|i| match i {
                Item::Error(e) => Some(e.clone()),
                _ => None,
            })
            .collect()
    };

    // Important 1: sent while booting.
    let slot: Slot = Rc::default();
    let keep = slot.clone();
    render_with(&theme, dims, build(theme, slot), move |cx, window| {
        let tab = tab_of(&keep);
        cx.update(|cx| tab.update(cx, |t, _| t.shot_boot()));
        let before = frame_fresh(cx, window);
        cx.simulate_input(window, "hello there");
        cx.simulate_keystrokes(window, "enter");
        cx.run_until_parked();
        let after = frame_fresh(cx, window);
        cx.read(|cx| {
            let t = tab.read(cx);
            assert_eq!(t.shot_pending(), 1, "{n}: held for the process");
            assert!(
                t.shot_items().is_empty(),
                "{n}: not in the conversation yet"
            );
            assert_eq!(
                t.shot_composer().read(cx).text(),
                "",
                "{n}: the box is cleared"
            );
        });
        assert!(
            top_differs(&before, &after, 0.7),
            "{n}: a message sent while booting shows at once"
        );
        save_shot("agent-tab-review-boot", "mocha", &after);
        feed(cx, &tab, |t, cx| {
            t.shot_history(vec![Past::You("earlier".into())], cx)
        });
        cx.read(|cx| {
            let t = tab.read(cx);
            assert_eq!(t.shot_pending(), 0, "{n}");
            assert!(
                matches!(t.shot_items(), [Item::You { text: a, .. }, Item::You { text: b, .. }]
                    if a == "earlier" && b == "hello there"),
                "{n}: after the history: {:?}",
                t.shot_items()
            );
            assert_eq!(t.status(), Status::Working, "{n}: sent");
        });
        keep.borrow_mut().take();
    });

    // Important 2: Force stop (a stand-in `sleep` as the process, in a folder that exists).
    let slot: Slot = Rc::default();
    let keep = slot.clone();
    let here = Start {
        cwd: std::env::temp_dir(),
        ..start()
    };
    render_with(
        &theme,
        dims,
        build_in(theme, slot, here),
        move |cx, window| {
            let tab = tab_of(&keep);
            feed(cx, &tab, |t, cx| {
                t.shot_spawn(["/bin/sh", "-c", "sleep 30"].map(String::from).to_vec(), cx)
            });
            let until = Instant::now() + Duration::from_secs(4);
            let pid = loop {
                if let Some(pid) = cx.read(|cx| tab.read(cx).process().and_then(|p| p.pid())) {
                    break pid;
                }
                assert!(Instant::now() < until, "{n}: the stand-in never started");
                std::thread::sleep(Duration::from_millis(20));
                cx.run_until_parked();
            };
            feed(cx, &tab, |t, cx| {
                t.shot_feed(
                    vec![Event::Started {
                        conversation: "c1".into(),
                        model: None,
                    }],
                    cx,
                )
            });
            feed(cx, &tab, |t, cx| t.send("Run the long tests".into(), cx));
            let _ = frame_fresh(cx, window);
            cx.simulate_keystrokes(window, "escape");
            cx.run_until_parked();
            cx.advance_clock(crate::agent_tab::STOP_FORCE_AFTER + Duration::from_millis(100));
            cx.run_until_parked();
            cx.read(|cx| assert_eq!(tab.read(cx).shot_stop_label(), Some("Force stop"), "{n}"));
            feed(cx, &tab, |t, cx| t.shot_press_stop(cx));
            let running = |pid: i32| {
                std::process::Command::new("/bin/kill")
                    .args(["-0", &pid.to_string()])
                    .status()
                    .is_ok_and(|s| s.success())
            };
            let until = Instant::now() + Duration::from_secs(4);
            while running(pid) {
                assert!(Instant::now() < until, "{n}: force stop left {pid} running");
                std::thread::sleep(Duration::from_millis(50));
            }
            cx.read(|cx| {
                let t = tab.read(cx);
                assert!(
                    matches!(t.shot_items().last(), Some(Item::Notice(s)) if s == "Stopped"),
                    "{n}: {:?}",
                    t.shot_items()
                );
                assert!(errors(t).is_empty(), "{n}: no red row");
                assert_eq!(t.status(), Status::Done, "{n}");
                assert_eq!(t.shot_stop_label(), None, "{n}");
                let argv = t.shot_last_argv();
                assert!(
                    argv.windows(2).any(|w| w == ["--resume", "c1"]),
                    "{n}: restarted resuming c1: {argv:?}"
                );
            });
            keep.borrow_mut().take();
        },
    );

    // Minor 1: a resume that fails, but not for an unknown conversation, keeps it.
    render_with(
        &theme,
        dims,
        move |window, cx| {
            cx.set_global(shot_appearance(theme, cx));
            let resume = Start {
                resume: Some("c9".into()),
                ..start()
            };
            cx.new(|cx| {
                let mut t = AgentTab::for_shots(&CLAUDE, resume, window, cx);
                t.shot_history(Vec::new(), cx);
                t.shot_process(
                    ProcessEvent::Exited {
                        code: Some(1),
                        stderr_tail: "Invalid API key · Please run /login".into(),
                    },
                    cx,
                );
                let errs = errors(&t);
                assert_eq!(
                    errs,
                    ["Couldn't start Claude: Invalid API key · Please run /login"],
                    "{n}: {:?}",
                    t.shot_items()
                );
                assert_eq!(
                    t.shot_resume().as_deref(),
                    Some("c9"),
                    "{n}: the id is kept"
                );
                assert!(
                    !t.shot_items()
                        .iter()
                        .any(|i| matches!(i, Item::Notice(x) if x == BAD_RESUME_NOTICE)),
                    "{n}: not a new conversation"
                );
                t
            })
            .into()
        },
        |_, _| {},
    );

    // Minor 2: sent between the process's end and its report.
    let slot: Slot = Rc::default();
    let keep = slot.clone();
    render_with(&theme, dims, build(theme, slot), move |cx, _| {
        let tab = tab_of(&keep);
        feed(cx, &tab, |t, cx| {
            t.shot_feed(
                vec![Event::Started {
                    conversation: "c1".into(),
                    model: None,
                }],
                cx,
            )
        });
        feed(cx, &tab, |t, cx| t.send("first".into(), cx));
        feed(cx, &tab, |t, cx| {
            t.shot_process(ProcessEvent::Ended { code: Some(1) }, cx)
        });
        feed(cx, &tab, |t, cx| t.send("second".into(), cx));
        cx.read(|cx| {
            let t = tab.read(cx);
            assert_eq!(
                t.shot_pending(),
                1,
                "{n}: held, not written to a dead process"
            );
            assert!(
                !t.shot_items()
                    .iter()
                    .any(|i| matches!(i, Item::You { text, .. } if text == "second")),
                "{n}"
            );
        });
        feed(cx, &tab, |t, cx| {
            t.shot_process(
                ProcessEvent::Exited {
                    code: Some(1),
                    stderr_tail: "crashed".into(),
                },
                cx,
            )
        });
        cx.read(|cx| {
            let t = tab.read(cx);
            let items = t.shot_items();
            let k = items.len();
            assert!(
                matches!(&items[k - 2..], [Item::Error(e), Item::You { text, .. }]
                    if e.contains("crashed") && text == "second"),
                "{n}: the exit, then the held message: {items:?}"
            );
            assert_eq!(t.shot_pending(), 0, "{n}");
            assert!(
                t.shot_last_argv()
                    .windows(2)
                    .any(|w| w == ["--resume", "c1"]),
                "{n}: restarted resuming c1"
            );
        });
        keep.borrow_mut().take();
    });

    // Minor 6: a spawn failure says which agent couldn't start.
    let slot: Slot = Rc::default();
    let keep = slot.clone();
    render_with(&theme, dims, build(theme, slot), move |cx, _| {
        let tab = tab_of(&keep);
        feed(cx, &tab, |t, cx| {
            t.shot_process(
                ProcessEvent::SpawnFailed("claude: No such file or directory".into()),
                cx,
            )
        });
        cx.read(|cx| {
            assert_eq!(
                errors(tab.read(cx)),
                ["Couldn't start Claude: claude: No such file or directory"],
                "{n}"
            );
        });
        keep.borrow_mut().take();
    });
    // Minor 4: only the header toggles a tool row; a click in its diff doesn't.
    let slot: Slot = Rc::default();
    let keep = slot.clone();
    render_with(&theme, dims, build(theme, slot), move |cx, window| {
        use gpui::{Modifiers, point};
        let tab = tab_of(&keep);
        feed(cx, &tab, |t, cx| {
            t.shot_feed(
                vec![
                    Event::ToolStarted {
                        id: "e1".into(),
                        name: "Edit".into(),
                        summary: "notes.txt".into(),
                        parent: None,
                        detail: Some(crate::providers::ToolDetail::Diff {
                            path: "/w/notes.txt".into(),
                            unified: "@@ -1,3 +1,3 @@\n-alpha\n+beta\n-one\n+two\n-three\n+four\n"
                                .into(),
                        }),
                        writes: Vec::new(),
                        texts: Vec::new(),
                    },
                    Event::ToolFinished {
                        id: "e1".into(),
                        ok: true,
                        summary: "updated".into(),
                        output: None,
                    },
                ],
                cx,
            )
        });
        feed(cx, &tab, |t, cx| t.shot_toggle(0, cx));
        let _ = frame_fresh(cx, window);
        // The row starts under the top line (26) and its 14 pt gap: the header is ~40–70, the
        // diff's six lines below it.
        cx.simulate_click(window, point(px(300.), px(120.)), Modifiers::none());
        cx.run_until_parked();
        assert!(
            cx.read(|cx| tab.read(cx).shot_open("e1")),
            "{n}: a click in the diff keeps it open"
        );
        cx.simulate_click(window, point(px(300.), px(55.)), Modifiers::none());
        cx.run_until_parked();
        assert!(
            !cx.read(|cx| tab.read(cx).shot_open("e1")),
            "{n}: the header closes it"
        );
        keep.borrow_mut().take();
    });

    println!("ui-shots: {n}: boot send, force stop, failed resume, late send, spawn failure");
}

/// M11aa Task 8: a Workspace the agent-workspace scenes draw (the sidebar's presets, no hint
/// card), built by `build`, kept in `slot`.
fn agent_workspace(
    theme: crate::theme::Theme,
    slot: Rc<RefCell<Option<Entity<crate::workspace::Workspace>>>>,
    build: impl FnOnce(
        &mut crate::workspace::Workspace,
        &mut Window,
        &mut Context<crate::workspace::Workspace>,
    ) + 'static,
) -> impl FnOnce(&mut Window, &mut App) -> AnyView {
    move |window, cx| {
        cx.set_global(super::sidebar_appearance(theme, cx));
        cx.bind_keys(crate::key_bindings());
        let ws = cx.new(|cx| {
            let mut ws = crate::workspace::Workspace::for_shots(window, cx);
            ws.sidebar.hint_dismissed = true;
            build(&mut ws, window, cx);
            ws
        });
        *slot.borrow_mut() = Some(ws.clone());
        ws.into()
    }
}

/// A provider preset's launch (spec §1.4, R6): an unnamed shell, then (the launch's `then`
/// hook's body, `agent_beside`) a Claude tab split to its right in the same folder; the row
/// reads "Claude" (R11). The tab's first message titles it.
pub(super) fn agent_workspace_split() {
    use crate::layout_model::TabKind;
    let n = "agent-workspace-split";
    for &(slug, theme_name) in shot_themes() {
        let theme = shot_theme(theme_name);
        let slot: Rc<RefCell<Option<Entity<crate::workspace::Workspace>>>> = Rc::default();
        let build = agent_workspace(theme, slot.clone(), |ws, window, cx| {
            let my_app = std::path::Path::new("/Users/me/code/my-app");
            let id = ws.open_unnamed_shot_session(Some(my_app), super::SHELL_TEXT, window, cx);
            ws.agent_beside(id, "claude", window, cx);
        });
        let path = shoot_with(
            n,
            slug,
            &theme,
            size(px(1100.0), px(640.0)),
            build,
            |cx, window| {
                let ws = slot.borrow_mut().take().expect("the workspace");
                settle_frames(cx, window);
                let (tab, view) = cx
                    .read(|cx| ws.read(cx).focused_agent_tab())
                    .expect("the agent tab is focused, its view made");
                let events = decode(include_str!("../providers/fixtures/claude-text.jsonl"));
                cx.update(|cx| {
                    view.update(cx, |t, cx| {
                        t.send("Say hello in one word".into(), cx);
                        t.shot_feed(events, cx);
                    })
                });
                settle_frames(cx, window);
                cx.update_window(window, |_, window, cx| {
                    let ws = ws.read(cx);
                    let w = ws.model.focused_workspace().expect("a workspace");
                    assert_eq!(
                        ws.workspace_label(w).as_ref(),
                        "Claude",
                        "{n}: the row's name"
                    );
                    let w = ws.model.workspace(w).expect("it");
                    assert_eq!(
                        w.panes.len(),
                        2,
                        "{n}: a shell and the agent tab side by side"
                    );
                    let kinds: Vec<bool> = w
                        .panes
                        .iter()
                        .map(|p| {
                            matches!(
                                ws.model.tab(p.active).map(|t| &t.kind),
                                Some(TabKind::Agent { .. })
                            )
                        })
                        .collect();
                    assert_eq!(kinds, [false, true], "{n}: the agent tab on the right");
                    match ws.model.tab(tab).map(|t| &t.kind) {
                        Some(TabKind::Agent { cwd, title, .. }) => {
                            assert_eq!(cwd, std::path::Path::new("/Users/me/code/my-app"), "{n}");
                            assert_eq!(title.as_deref(), Some("Say hello in one word"), "{n}");
                        }
                        other => panic!("{n}: {other:?}"),
                    }
                    assert_eq!(ws.agents[&tab].id, 1, "{n}: the first agent tab is a1");
                    let keys = view.read(cx).focus_handle(cx).is_focused(window);
                    assert!(keys, "{n}: the new agent tab has the keyboard");
                })
                .unwrap();
            },
        );
        println!("ui-shots: {n}: {}", path.display());
    }
}

/// The sidebar's + menu: each preset fuxx runs as an agent tab (`claude`, `agy`) has its
/// provider's badge before its name; the others (opencode, codex) none.
pub(super) fn agent_preset_menu() {
    let n = "agent-preset-menu";
    for &(slug, theme_name) in shot_themes() {
        let theme = shot_theme(theme_name);
        let slot: Rc<RefCell<Option<Entity<crate::workspace::Workspace>>>> = Rc::default();
        let build = agent_workspace(theme, slot.clone(), |ws, window, cx| {
            let api = std::path::Path::new("/Users/me/code/api");
            ws.open_shot_session("api", api, super::SHELL_TEXT, None, window, cx);
            ws.sidebar.new_menu = true;
        });
        let path = shoot_with(
            n,
            slug,
            &theme,
            size(px(960.0), px(600.0)),
            build,
            |cx, window| {
                let ws = slot.borrow_mut().take().expect("the workspace");
                settle_frames(cx, window);
                cx.read(|cx| {
                    assert!(ws.read(cx).sidebar.new_menu, "{n}: the + menu is open");
                    let presets = &cx.global::<crate::appearance::Appearance>().presets;
                    let badged: Vec<&str> = presets
                        .iter()
                        .filter(|p| crate::presets::provider_for(p).is_some())
                        .map(|p| p.name.as_str())
                        .collect();
                    assert_eq!(
                        badged,
                        ["claude", "agy"],
                        "{n}: the presets drawn with a badge"
                    );
                });
            },
        );
        println!("ui-shots: {n}: {}", path.display());
    }
}

/// A pane's ⋯ menu lists New Claude Tab (here, a workspace on this Mac); one of an SSH
/// workspace wouldn't (`PaneAction::items(false)`, unit-tested).
pub(super) fn agent_pane_menu() {
    use crate::pane_actions::PaneAction;
    let n = "agent-pane-menu";
    for &(slug, theme_name) in shot_themes() {
        let theme = shot_theme(theme_name);
        let slot: Rc<RefCell<Option<Entity<crate::workspace::Workspace>>>> = Rc::default();
        let build = agent_workspace(theme, slot.clone(), |ws, window, cx| {
            let api = std::path::Path::new("/Users/me/code/api");
            ws.open_shot_session("api", api, super::SHELL_TEXT, None, window, cx);
        });
        let path = shoot_with(
            n,
            slug,
            &theme,
            size(px(960.0), px(600.0)),
            build,
            |cx, window| {
                let ws = slot.borrow_mut().take().expect("the workspace");
                settle_frames(cx, window);
                cx.update_window(window, |_, window, cx| {
                    ws.update(cx, |ws, cx| {
                        let pane = ws.model.focused_pane().expect("a pane");
                        ws.open_pane_actions(pane, gpui::point(px(930.0), px(40.0)), window, cx)
                    })
                })
                .unwrap();
                settle_frames(cx, window);
                cx.read(|cx| {
                    let ws = ws.read(cx);
                    let w = ws.model.focused_workspace().expect("a workspace");
                    assert!(ws.pane_actions.is_some(), "{n}: the menu is open");
                    assert!(!ws.workspace_is_ssh(w), "{n}");
                    let labels: Vec<String> =
                        PaneAction::items(true).iter().map(|a| a.label()).collect();
                    assert!(
                        labels.contains(&"New Claude Tab".to_string()),
                        "{n}: {labels:?}"
                    );
                });
            },
        );
        println!("ui-shots: {n}: {}", path.display());
    }
}

/// R13 at boot: a restored tab whose conversation another process holds (the boot's `ps`
/// found it: its classic view left running) starts paused with the warning, and no process is
/// started (no command line built); what you typed while it booted is back in the box. A
/// failed `ps` also starts it paused. Nothing held: it starts (`--resume`).
pub(super) fn agent_tab_held() {
    use crate::workspace_agent::Holder;
    let n = "agent-tab-held";
    let theme = shot_theme("Catppuccin Mocha");
    for (holder, starts) in [
        (Holder::Held, false),
        (Holder::Unchecked, false),
        (Holder::Free, true),
    ] {
        let slot: Slot = Rc::default();
        let keep = slot.clone();
        render_with(
            &theme,
            size(px(DIMS.0), px(DIMS.1)),
            move |window, cx| {
                cx.set_global(shot_appearance(theme, cx));
                let resume = Start {
                    resume: Some("c1".into()),
                    ..start()
                };
                let tab = cx.new(|cx| AgentTab::for_shots(&CLAUDE, resume, window, cx));
                *keep.borrow_mut() = Some(tab.clone());
                tab.into()
            },
            move |cx, _| {
                let tab = tab_of(&slot);
                // Typed while it booted: held for the process.
                feed(cx, &tab, |t, cx| {
                    t.shot_boot();
                    t.send("are you there".into(), cx)
                });
                let past = vec![Past::You("earlier".into())];
                let h = holder;
                feed(cx, &tab, |t, cx| t.shot_history_held(past, h, cx));
                cx.read(|cx| {
                    let t = tab.read(cx);
                    let argv = t.shot_last_argv();
                    let text = t.shot_composer().read(cx).text().to_string();
                    // A scene tab never probes the real agent.
                    assert!(
                        cx.try_global::<crate::agent_catalog::Catalogs>()
                            .is_none_or(|c| c.catalog("claude", &start().cwd).is_none()),
                        "{n}: {holder:?}: no probe"
                    );
                    if starts {
                        assert!(!t.shot_paused(), "{n}: {holder:?}: started");
                        assert!(argv.windows(2).any(|w| w == ["--resume", "c1"]), "{n}");
                        assert_eq!(t.status(), Status::Working, "{n}: the held message sent");
                    } else {
                        assert!(t.shot_paused(), "{n}: {holder:?}: starts paused");
                        assert!(
                            argv.is_empty(),
                            "{n}: {holder:?}: nothing started: {argv:?}"
                        );
                        assert_eq!(t.status(), Status::Done, "{n}");
                        assert_eq!(t.shot_pending(), 0, "{n}: nothing left to send later");
                        assert_eq!(
                            text, "are you there",
                            "{n}: the held text is back in the box"
                        );
                        let last = t.shot_items().last().cloned();
                        let Some(Item::Notice(notice)) = last else {
                            panic!("{n}: {holder:?}: a notice, not {last:?}");
                        };
                        let want = match holder {
                            Holder::Unchecked => "Couldn't check whether Claude",
                            _ => "Claude is still running this conversation in another terminal",
                        };
                        assert!(notice.starts_with(want), "{n}: {notice}");
                    }
                });
                slot.borrow_mut().take();
            },
        );
        println!("ui-shots: {n}: {holder:?} ok");
    }
}

/// M11aa Task 9: a workspace whose Claude tab (not shown) asks for a permission: its row's badge
/// is red (as a waiting terminal's), the question under its name with Open. api's workspace is
/// shown.
pub(super) fn agent_sidebar_waiting() {
    use crate::sidebar::{SidebarRow, Sub};
    let n = "agent-sidebar-waiting";
    for &(slug, theme_name) in shot_themes() {
        let theme = shot_theme(theme_name);
        let slot: Rc<RefCell<Option<Entity<crate::workspace::Workspace>>>> = Rc::default();
        let build = agent_workspace(theme, slot.clone(), |ws, window, cx| {
            let api = std::path::Path::new("/Users/me/code/api");
            let web = std::path::Path::new("/Users/me/code/web");
            ws.open_shot_session("api", api, super::SHELL_TEXT, None, window, cx);
            ws.open_shot_session("web", web, super::SHELL_TEXT, None, window, cx);
            let web = ws.sessions.position(|s| s.label() == "web").expect("web");
            let id = ws.sessions.items()[web].id;
            ws.agent_beside(id, "claude", window, cx);
            let api = ws.sessions.position(|s| s.label() == "api").expect("api");
            ws.select(api, window, cx);
        });
        let path = shoot_with(
            n,
            slug,
            &theme,
            size(px(960.0), px(600.0)),
            build,
            |cx, window| {
                let ws = slot.borrow_mut().take().expect("the workspace");
                settle_frames(cx, window);
                let (tab, view) = cx.read(|cx| {
                    let ws = ws.read(cx);
                    let (tab, slot) = ws.agents.iter().next().expect("web's Claude tab");
                    (*tab, slot.view.clone())
                });
                cx.update(|cx| {
                    view.update(cx, |t, cx| {
                        t.send("Run the tests".into(), cx);
                        t.shot_feed(
                            vec![Event::Permission {
                                id: "p1".into(),
                                tool: "Bash".into(),
                                what: "npm test".into(),
                                detail: "npm test".into(),
                                always: None,
                            }],
                            cx,
                        );
                    })
                });
                // Waiting 4 minutes (a live "0s" could read "1s" under load).
                cx.update(|cx| {
                    ws.update(cx, |ws, cx| {
                        ws.shot_agent_since(tab, Duration::from_secs(4 * 60 + 10), cx)
                    })
                });
                settle_frames(cx, window);
                let row = cx.read(|cx| {
                    let ws = ws.read(cx);
                    let (w, _) = ws.model.locate(tab).expect("placed");
                    ws.rows(Instant::now())
                        .iter()
                        .find_map(|r| match r {
                            SidebarRow::Workspace(r) if r.workspace == w => Some(r.clone()),
                            _ => None,
                        })
                        .expect("web's row")
                });
                assert_eq!(
                    (row.agent, row.status, row.sub.clone()),
                    (
                        Some(tab),
                        Status::Waiting,
                        Some(Sub::Question("Claude wants to run: npm test".into()))
                    ),
                    "{n}: web's row waits on its Claude"
                );
                assert_eq!(row.age.as_deref(), Some("4m"), "{n}: its age");
                // Who is in front: its Claude tab (M11al §2.4); the line itself says the
                // question while it waits (plan-time call 1).
                assert_eq!(
                    row.who,
                    crate::sidebar::Who::Agent {
                        glyph: "\u{2733}",
                        name: "Claude"
                    },
                    "{n}: web's row is Claude's"
                );
            },
        );
        println!("ui-shots: {n}: {}", path.display());
    }
}

/// The shell of `agent-tab-attachment`, marked as fuxx's zsh integration marks it: a finished
/// `echo done` (exit 0, rows 0-1), then `npm test` (row 2), still running, its output so far
/// on rows 3-11. `ATTACHMENT_END` ends it (exit 1) and shows the next prompt.
fn attachment_shell() -> String {
    let c = |cmd: &str| format!("\x1b]133;C;cmdline_url={cmd}\x07");
    [
        format!(
            "{MARK_A}$ {MARK_B}echo done\r\n{}done\r\n",
            c("echo%20done")
        ),
        "\x1b]133;D;0\x07".to_string(),
        format!(
            "{MARK_A}$ {MARK_B}npm test\r\n{}{ATTACHMENT_OUTPUT}",
            c("npm%20test")
        ),
    ]
    .concat()
}

const MARK_A: &str = "\x1b]133;A\x07";
const MARK_B: &str = "\x1b]133;B\x07";

/// `npm test` ends with exit 1, and the next prompt shows.
const ATTACHMENT_END: &str = "\x1b]133;D;1\x07\x1b]133;A\x07$ \x1b]133;B\x07";

/// `npm test`'s output in `attachment_shell` (its rows end in `\r\n` on the screen).
const ATTACHMENT_OUTPUT: &str = "> app@1.0.0 test\r\n> node --test\r\n\r\n\
     ✖ adds two numbers (2.1ms)\r\n  expected 4, got 5\r\n\r\nℹ tests 3\r\nℹ pass 2\r\nℹ fail 1\r\n";

/// Run frames and the app's tasks until `done` holds (a block's output is read on the
/// background executor), for at most five seconds.
fn settle_until(
    cx: &mut VisualTestAppContext,
    window: AnyWindowHandle,
    what: &str,
    done: impl Fn(&App) -> bool,
) {
    let started = Instant::now();
    loop {
        settle_frames(cx, window);
        if cx.read(|cx| done(cx)) {
            return;
        }
        assert!(started.elapsed() < Duration::from_secs(5), "{what}");
        super::prof::sleep(Duration::from_millis(10));
    }
}

/// Drag the command of block `row`'s chip (the bar's first row is `row`) in `shell` to `to`,
/// and drop it there, as you would: the pointer over the bar opens the chip, a press on its
/// command, a move past GPUI's drag threshold, a move to `to` (`over` runs there, the block
/// still held), the release. The chip closes once the pointer has left it.
fn drag_block(
    cx: &mut VisualTestAppContext,
    window: AnyWindowHandle,
    shell: &Entity<crate::terminal_view::TerminalView>,
    row: usize,
    to: gpui::Point<gpui::Pixels>,
    over: impl FnOnce(&mut VisualTestAppContext),
) {
    use gpui::{Modifiers, MouseButton, point};
    let cell = cx
        .read(|cx| shell.read(cx).cell_bounds(0, row))
        .expect("the shell was painted");
    let bar = point(cell.left() - px(4.0), cell.center().y);
    cx.simulate_mouse_move(window, bar, None, Modifiers::none());
    settle_frames(cx, window);
    let handle = cx
        .read(|cx| shell.read(cx).chip_command_shot())
        .expect("the chip shows its command");
    let from = handle.center();
    let left = MouseButton::Left;
    cx.simulate_mouse_down(window, from, left, Modifiers::none());
    cx.simulate_mouse_move(
        window,
        from + point(px(0.0), px(8.0)),
        left,
        Modifiers::none(),
    );
    cx.simulate_mouse_move(window, to, left, Modifiers::none());
    settle_frames(cx, window);
    over(cx);
    cx.simulate_mouse_up(window, to, left, Modifiers::none());
    cx.simulate_mouse_move(window, to, None, Modifiers::none());
    settle_frames(cx, window);
    cx.advance_clock(Duration::from_millis(crate::block_view::CHIP_HIDE_MS));
    settle_frames(cx, window);
    assert!(
        cx.read(|cx| shell.read(cx).hovered_block().is_none()),
        "the chip closed after the drop"
    );
}

/// M11aa Task 11 (spec §1.7): a command block dragged from a shell onto an agent tab is
/// attached to your next message as "command + exit code + output" text.
/// - `npm test`, still running, dragged by its chip's command onto the Claude tab's body
///   (tinted while it is over it: `-dragging`), ends (exit 1) before the drop: the attachment
///   has its exit code (read as the block is when dropped). Its chip shows under the composer
///   with a × (`-composer`); nothing is sent or started.
/// - Sent: it shows under your message's card, and the box has none left (the main shot).
/// - `echo done`, dropped on the Claude tab in its bar (tinted beyond its hover), is attached;
///   with `npm test` attached again, the first chip's × takes `echo done` off, and the next
///   message carries only `npm test`.
/// - R13: a block dropped on a tab paused for "Open in classic view" only waits there: the tab
///   stays paused and starts nothing.
pub(super) fn agent_tab_attachment() {
    use crate::block_view::{BlockDrag, attachment_text};
    use crate::blocks::Outcome;
    use gpui::Modifiers;
    let n = "agent-tab-attachment";
    let dims = size(px(1100.0), px(640.0));
    for &(slug, theme_name) in shot_themes() {
        let theme = shot_theme(theme_name);
        let slot: Rc<RefCell<Option<Entity<crate::workspace::Workspace>>>> = Rc::default();
        let build = agent_workspace(theme, slot.clone(), |ws, window, cx| {
            let my_app = std::path::Path::new("/Users/me/code/my-app");
            let shell = attachment_shell();
            let id = ws.open_unnamed_shot_session(Some(my_app), &shell, window, cx);
            ws.agent_beside(id, "claude", window, cx);
        });
        render_with(&theme, dims, build, |cx, window| {
            let ws = slot.borrow_mut().take().expect("the workspace");
            settle_frames(cx, window);
            let (tab, view) = cx
                .read(|cx| ws.read(cx).focused_agent_tab())
                .expect("the agent tab");
            let shell = cx.read(|cx| ws.read(cx).sessions.items()[0].view.clone());
            let panes = super::shown_panes(cx, &ws);
            let bar_tab = super::tab_bounds(cx, &ws, panes[1], 0).expect("Claude's tab");
            let body = gpui::point(bar_tab.left() + px(120.0), bar_tab.bottom() + px(200.0));
            // As the grid gives it back: rows joined by newlines, none after the last.
            let output = ATTACHMENT_OUTPUT.replace('\r', "");
            let npm = attachment_text(Some("npm test"), Outcome::Failed(1), output.trim_end());
            let done = attachment_text(Some("echo done"), Outcome::Ok, "done");
            // (A command block's chip's label is its whole text.)
            let attached = |cx: &App| -> Vec<String> {
                view.read(cx)
                    .shot_attachments()
                    .iter()
                    .map(crate::agent_tab::Attachment::label)
                    .collect()
            };
            let block = |cmd: &str, cx: &mut VisualTestAppContext| {
                cx.read(|cx| {
                    let b = shell.read(cx).shot_blocks();
                    let b = b.iter().find(|b| b.cmd.as_deref() == Some(cmd)).expect(cmd);
                    BlockDrag {
                        id: b.id.clone(),
                        command: b.cmd.clone(),
                        outcome: b.outcome,
                        core: shell.read(cx).shot_core(),
                    }
                })
            };
            assert_eq!(
                block("npm test", cx).outcome,
                Outcome::Running,
                "{n}: npm test runs as it is picked up"
            );

            // npm test, dragged onto the tab's body; it ends while held.
            // Where the tint shows: the body's empty page, away from the chip at the pointer.
            let page = (body.x.into(), f32::from(body.y) + 120.0);
            let held: RefCell<Option<super::Shot>> = RefCell::default();
            drag_block(cx, window, &shell, 2, body, |cx| {
                // Held over the body: it is tinted, the block's chip at the pointer.
                let shot = frame_fresh(cx, window);
                save_shot(&format!("{n}-dragging"), slug, &shot);
                *held.borrow_mut() = Some(shot);
                cx.read(|cx| shell.read(cx).show_text(ATTACHMENT_END));
            });
            let held = held.take().expect("a shot while held");
            settle_until(cx, window, "npm test attached", |cx| {
                !view.read(cx).shot_attachments().is_empty()
            });
            let got = cx.read(|cx| attached(cx));
            assert_eq!(got.len(), 1, "{n}: one block attached");
            assert_eq!(
                got[0], npm,
                "{n}: command + exit code (as it ended) + output"
            );
            assert!(
                got[0].starts_with("$ npm test\n(exit 1)\n> app@1.0.0 test"),
                "{n}"
            );
            cx.read(|cx| {
                let t = view.read(cx);
                assert!(t.shot_items().is_empty(), "{n}: nothing sent");
                assert!(t.shot_last_argv().is_empty(), "{n}: nothing started");
            });
            let dropped = frame_fresh(cx, window);
            assert_ne!(
                super::pixel_at(&held, dims, page.0, page.1),
                super::pixel_at(&dropped, dims, page.0, page.1),
                "{n}: the body is tinted while a block is over it, not after"
            );
            save_shot(&format!("{n}-composer"), slug, &dropped);

            // Sent: under your card; the box keeps none. The reply ends the turn (no spinner
            // in the shot), and its age is dated back (no live "0s").
            let reply = decode(include_str!("../providers/fixtures/claude-text.jsonl"));
            cx.update(|cx| {
                view.update(cx, |t, cx| {
                    t.send("Why does this fail?".into(), cx);
                    t.shot_feed(reply, cx);
                })
            });
            cx.update(|cx| {
                ws.update(cx, |ws, cx| {
                    ws.shot_agent_since(tab, Duration::from_secs(4 * 60 + 10), cx)
                })
            });
            settle_frames(cx, window);
            cx.read(|cx| {
                let t = view.read(cx);
                assert!(
                    t.shot_attachments().is_empty(),
                    "{n}: sent with the message"
                );
                let you = t.shot_items().iter().find_map(|i| match i {
                    Item::You { text, attachments } => Some((text.clone(), attachments.clone())),
                    _ => None,
                });
                let (text, with) = you.expect("your message");
                assert_eq!(text, "Why does this fail?", "{n}");
                assert_eq!(with, got, "{n}: the block went with it");
            });
            save_shot(n, slug, &frame_fresh(cx, window));

            // echo done, dropped on Claude's tab in its bar.
            // The tab's fill before its icon, clear of the block's chip at the pointer.
            let on_tab = (f32::from(bar_tab.left()) + 3.0, bar_tab.center().y.into());
            // Against the pointer merely on it (a tab's hover shade).
            cx.simulate_mouse_move(window, bar_tab.center(), None, Modifiers::none());
            settle_frames(cx, window);
            let before = frame_fresh(cx, window);
            let tinted = RefCell::new(false);
            drag_block(cx, window, &shell, 0, bar_tab.center(), |cx| {
                let shot = frame_fresh(cx, window);
                *tinted.borrow_mut() = super::pixel_at(&shot, dims, on_tab.0, on_tab.1)
                    != super::pixel_at(&before, dims, on_tab.0, on_tab.1);
            });
            assert!(
                tinted.take(),
                "{n}: the tab is tinted while a block is over it (not only hovered)"
            );
            settle_until(cx, window, "echo done attached", |cx| {
                !view.read(cx).shot_attachments().is_empty()
            });
            let got = cx.read(|cx| attached(cx));
            assert_eq!(got, std::slice::from_ref(&done), "{n}: dropped on its tab");
            cx.read(|cx| {
                assert_eq!(ws.read(cx).model.focused_tab(), Some(tab), "{n}: shown");
            });

            // Two waiting; the first one's × takes it off, and only the other is sent.
            let drag = block("npm test", cx);
            cx.update(|cx| ws.update(cx, |ws, cx| ws.attach_block(tab, &drag, cx)));
            settle_until(cx, window, "npm test attached again", |cx| {
                view.read(cx).shot_attachments().len() == 2
            });
            assert_eq!(
                cx.read(|cx| attached(cx)),
                [done.clone(), npm.clone()],
                "{n}"
            );
            let x = cx
                .read(|cx| view.read(cx).shot_remove_bounds(0))
                .expect("the first chip's ×");
            cx.simulate_click(window, x.center(), Modifiers::none());
            settle_frames(cx, window);
            assert_eq!(
                cx.read(|cx| attached(cx)),
                std::slice::from_ref(&npm),
                "{n}: × took the first off"
            );
            cx.update(|cx| view.update(cx, |t, cx| t.send("And with this?".into(), cx)));
            settle_frames(cx, window);
            cx.read(|cx| {
                let t = view.read(cx);
                assert!(t.shot_attachments().is_empty(), "{n}");
                let last = t.shot_items().iter().rev().find_map(|i| match i {
                    Item::You { text, attachments } => Some((text.clone(), attachments.clone())),
                    _ => None,
                });
                let (text, with) = last.expect("your second message");
                assert_eq!(text, "And with this?", "{n}");
                assert_eq!(
                    with,
                    std::slice::from_ref(&npm),
                    "{n}: only what's left went"
                );
            });

            // R13: dropped on a paused tab, it only waits.
            cx.update(|cx| view.update(cx, |t, cx| t.pause_for_classic(cx)));
            let argv = cx.read(|cx| view.read(cx).shot_last_argv());
            let drag = block("echo done", cx);
            cx.update(|cx| ws.update(cx, |ws, cx| ws.attach_block(tab, &drag, cx)));
            settle_until(cx, window, "attached while paused", |cx| {
                !view.read(cx).shot_attachments().is_empty()
            });
            cx.read(|cx| {
                let t = view.read(cx);
                assert!(t.shot_paused(), "{n}: still paused");
                assert_eq!(t.shot_last_argv(), argv, "{n}: nothing started");
                assert_eq!(t.status(), Status::Done, "{n}");
                assert_eq!(
                    t.shot_attachments(),
                    [crate::agent_tab::Attachment::Text(done.clone())],
                    "{n}"
                );
            });
        });
        println!("ui-shots: {n}-{slug}: dropped on the body and the tab, removed, sent, paused");
    }
}

/// The ids a tab told the Workspace (`AgentTabEvent::Conversation`), in order.
type Told = Rc<RefCell<Vec<String>>>;

/// Checks only: a tab made with no process (R12), `start` given, its `Conversation` events
/// kept in `told`; `act` runs once the tab's history is in (as `new`'s boot task leaves it:
/// its process (re)started, so `shot_last_argv` is the command it would run).
fn run_scene(
    name: &str,
    start: Start,
    act: impl FnOnce(&str, &mut VisualTestAppContext, &Entity<AgentTab>, &Told) + 'static,
) {
    use crate::agent_tab::AgentTabEvent;
    let theme = shot_theme("Catppuccin Mocha");
    let slot: Slot = Rc::default();
    let keep = slot.clone();
    let told: Told = Rc::default();
    let heard = told.clone();
    let n = name.to_string();
    render_with(
        &theme,
        size(px(DIMS.0), px(DIMS.1)),
        move |window, cx| {
            cx.set_global(shot_appearance(theme, cx));
            let tab = cx.new(|cx| AgentTab::for_shots(&CLAUDE, start, window, cx));
            cx.subscribe(&tab, move |_, e: &AgentTabEvent, _| {
                if let AgentTabEvent::Conversation(id) = e {
                    heard.borrow_mut().push(id.clone());
                }
            })
            .detach();
            *keep.borrow_mut() = Some(tab.clone());
            tab.into()
        },
        move |cx, _| {
            let tab = tab_of(&slot);
            feed(cx, &tab, |t, cx| t.shot_history(Vec::new(), cx));
            act(&n, cx, &tab, &told);
            slot.borrow_mut().take();
        },
    );
}

/// The word after `flag` in the command line the tab last (re)started with.
fn argv_value(cx: &VisualTestAppContext, tab: &Entity<AgentTab>, flag: &str) -> Option<String> {
    let argv = cx.read(|cx| tab.read(cx).shot_last_argv());
    argv.windows(2).find(|w| w[0] == flag).map(|w| w[1].clone())
}

fn notices(cx: &VisualTestAppContext, tab: &Entity<AgentTab>, text: &str) -> usize {
    cx.read(|cx| {
        tab.read(cx)
            .shot_items()
            .iter()
            .filter(|i| matches!(i, Item::Notice(t) if t == text))
            .count()
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

fn started(id: &str) -> Event {
    Event::Started {
        conversation: id.into(),
        model: Some("claude-opus-5-5".into()),
    }
}

/// Checks only, on the paths a process's runs take (final review fixes):
/// - a new conversation starts under an id fuxx chose (`--session-id`, Ruling 40), the tab's
///   conversation from your first message; the agent's own `Started` with it is no "new
///   conversation"; a later id (Claude's `/clear`) is followed and told again; a run that ended
///   before the agent took the id starts under a new one (Claude refuses an id used before);
/// - a resumed run's `Started` with its id, then another id: no false "Couldn't resume";
/// - Force stop right after the process ended: a send goes at once (not queued forever);
/// - a resume after the classic view leaves text you edited during the check in the box;
/// - a resume that failed for another reason than a gone conversation puts what was queued
///   back in the box.
pub(super) fn agent_tab_runs() {
    use crate::agent_tab::RESTORED_NOTICE;
    use crate::workspace_agent::Holder;
    const OLD: &str = "6a1c234d-7469-40ef-b093-c74ad8006c28";
    const CLEARED: &str = "0b3c4a5e-1f2d-4e6a-9b8c-7d6e5f4a3b2c";

    run_scene("agent-tab-runs-fresh", start(), |n, cx, tab, told| {
        let chosen = argv_value(cx, tab, "--session-id").expect("started under a chosen id");
        assert_eq!(argv_value(cx, tab, "--resume"), None, "{n}");
        assert_eq!(
            cx.read(|cx| tab.read(cx).resume_id()),
            None,
            "{n}: nothing said yet"
        );
        assert!(told.borrow().is_empty(), "{n}");
        feed(cx, tab, |t, cx| t.send("Say hello".into(), cx));
        assert_eq!(
            cx.read(|cx| tab.read(cx).resume_id()),
            Some(chosen.clone()),
            "{n}: the chosen id is its conversation from the first message"
        );
        assert_eq!(
            *told.borrow(),
            std::slice::from_ref(&chosen),
            "{n}: told at once (saved)"
        );
        feed(cx, tab, |t, cx| t.shot_feed(vec![started(&chosen)], cx));
        assert_eq!(notices(cx, tab, BAD_RESUME_NOTICE), 0, "{n}: not a new one");
        assert_eq!(
            *told.borrow(),
            std::slice::from_ref(&chosen),
            "{n}: the same id: not told again"
        );
        // `/clear`: the agent goes on under another id, which the tab follows.
        feed(cx, tab, |t, cx| {
            t.shot_feed(
                vec![
                    Event::TurnDone {
                        ok: true,
                        message: None,
                    },
                    started(CLEARED),
                ],
                cx,
            )
        });
        assert_eq!(notices(cx, tab, BAD_RESUME_NOTICE), 0, "{n}");
        assert_eq!(
            cx.read(|cx| tab.read(cx).resume_id()).as_deref(),
            Some(CLEARED),
            "{n}: follows the new id"
        );
        assert_eq!(
            *told.borrow(),
            [chosen, CLEARED.to_string()],
            "{n}: told again"
        );
    });

    run_scene("agent-tab-runs-unconfirmed", start(), |n, cx, tab, told| {
        let chosen = argv_value(cx, tab, "--session-id").expect("a chosen id");
        feed(cx, tab, |t, cx| t.send("Say hello".into(), cx));
        // Stopped before the agent said it started: its id may never have been taken.
        feed(cx, tab, |t, cx| t.shot_force_stop(cx));
        let again = argv_value(cx, tab, "--session-id").expect("a new conversation again");
        assert_ne!(
            again, chosen,
            "{n}: under a new id (an id used once is refused)"
        );
        assert_eq!(
            argv_value(cx, tab, "--resume"),
            None,
            "{n}: nothing to resume"
        );
        feed(cx, tab, |t, cx| t.send("Say hello again".into(), cx));
        assert_eq!(
            cx.read(|cx| tab.read(cx).resume_id()),
            Some(again.clone()),
            "{n}"
        );
        assert_eq!(*told.borrow(), [chosen, again], "{n}");
    });

    let resumed = Start {
        resume: Some(OLD.into()),
        ..start()
    };
    run_scene(
        "agent-tab-runs-resumed",
        resumed.clone(),
        |n, cx, tab, told| {
            assert_eq!(argv_value(cx, tab, "--resume").as_deref(), Some(OLD), "{n}");
            assert_eq!(argv_value(cx, tab, "--session-id"), None, "{n}");
            feed(cx, tab, |t, cx| t.send("Go on".into(), cx));
            feed(cx, tab, |t, cx| {
                t.shot_feed(
                    vec![
                        started(OLD),
                        Event::TurnDone {
                            ok: true,
                            message: None,
                        },
                        started(CLEARED),
                    ],
                    cx,
                )
            });
            assert_eq!(
                notices(cx, tab, BAD_RESUME_NOTICE),
                0,
                "{n}: no false notice"
            );
            assert_eq!(notices(cx, tab, RESTORED_NOTICE), 1, "{n}: still restored");
            assert_eq!(
                cx.read(|cx| tab.read(cx).resume_id()).as_deref(),
                Some(CLEARED),
                "{n}"
            );
            assert_eq!(
                told.borrow().last().map(String::as_str),
                Some(CLEARED),
                "{n}"
            );
        },
    );

    run_scene(
        "agent-tab-runs-force-after-exit",
        start(),
        |n, cx, tab, _| {
            feed(cx, tab, |t, cx| t.send("Write a long essay".into(), cx));
            let id = cx.read(|cx| tab.read(cx).resume_id()).expect("its id");
            feed(cx, tab, |t, cx| t.shot_feed(vec![started(&id)], cx));
            // The process ends; within END_WAIT, Force stop.
            feed(cx, tab, |t, cx| {
                t.shot_process(ProcessEvent::Ended { code: Some(0) }, cx);
                t.shot_force_stop(cx);
            });
            feed(cx, tab, |t, cx| t.send("and then this".into(), cx));
            cx.read(|cx| assert_eq!(tab.read(cx).shot_pending(), 0, "{n}: not queued"));
            assert_eq!(
                said(cx, tab),
                ["Write a long essay", "and then this"],
                "{n}: sent"
            );
        },
    );

    run_scene("agent-tab-runs-classic-edit", start(), |n, cx, tab, _| {
        feed(cx, tab, |t, cx| t.send("Fix the date check".into(), cx));
        let id = cx.read(|cx| tab.read(cx).resume_id()).expect("its id");
        feed(cx, tab, |t, cx| {
            t.shot_feed(
                vec![
                    started(&id),
                    Event::TurnDone {
                        ok: true,
                        message: None,
                    },
                ],
                cx,
            );
            t.pause_for_classic(cx);
        });
        let composer = cx.read(|cx| tab.read(cx).shot_composer());
        // Sent, then edited while the Workspace checked: the edit stays.
        feed(cx, tab, |t, cx| {
            composer.update(cx, |c, cx| c.set_text("and the tests, edited".into(), cx));
            t.resume_after_classic("and the tests".into(), Holder::Free, None, cx);
        });
        assert_eq!(
            cx.read(|cx| composer.read(cx).text().to_string()),
            "and the tests, edited",
            "{n}: the edited text stays"
        );
        assert_eq!(
            said(cx, tab).last().map(String::as_str),
            Some("and the tests"),
            "{n}"
        );
        // Not edited: the box is cleared.
        feed(cx, tab, |t, cx| {
            t.pause_for_classic(cx);
            composer.update(cx, |c, cx| c.set_text("and the docs".into(), cx));
            t.resume_after_classic("and the docs".into(), Holder::Free, None, cx);
        });
        assert_eq!(
            cx.read(|cx| composer.read(cx).text().to_string()),
            "",
            "{n}"
        );
    });

    run_scene("agent-tab-runs-failed-resume", resumed, |n, cx, tab, _| {
        feed(cx, tab, |t, cx| t.send("Go on".into(), cx));
        // It ends before it said it started, not for a gone conversation; a send meanwhile
        // waits for the end.
        feed(cx, tab, |t, cx| {
            t.shot_process(ProcessEvent::Ended { code: Some(1) }, cx);
            t.send("and this".into(), cx);
        });
        cx.read(|cx| assert_eq!(tab.read(cx).shot_pending(), 1, "{n}: waits"));
        feed(cx, tab, |t, cx| {
            t.shot_process(
                ProcessEvent::Exited {
                    code: Some(1),
                    stderr_tail: "network is down".into(),
                },
                cx,
            )
        });
        cx.read(|cx| {
            let t = tab.read(cx);
            assert_eq!(t.shot_pending(), 0, "{n}: nothing left waiting");
            assert_eq!(
                t.shot_composer().read(cx).text(),
                "and this",
                "{n}: back in the box"
            );
            assert!(
                t.shot_items()
                    .iter()
                    .any(|i| matches!(i, Item::Error(e) if e.contains("network is down"))),
                "{n}"
            );
        });
    });
    println!("ui-shots: agent-tab-runs: chosen ids, /clear, Force stop, classic edits, queues");
}
