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
        name: "agent-tab-composer-wrap",
        run: agent_tab_composer_wrap,
        themes: Themes::Both,
        serial: false,
        kind: Kind::State,
        sources: TAB,
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
        mode: None,
        extra_args: Vec::new(),
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
    move |window, cx| {
        cx.set_global(shot_appearance(theme, cx));
        cx.bind_keys(crate::key_bindings());
        let tab = cx.new(|cx| AgentTab::for_shots(&CLAUDE, start, window, cx));
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

/// The sidebar's + menu: each preset fuxx runs as an agent tab (`claude`) has its provider's
/// badge before its name; the others (agy, opencode, codex) none.
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
                    assert_eq!(badged, ["claude"], "{n}: the presets drawn with a badge");
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
