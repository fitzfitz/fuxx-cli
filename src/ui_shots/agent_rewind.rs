//! Batch 3, chore 2's scenes: Rewind and Fork from your messages in the agent tab, and agy's
//! Edit. No agent runs: the tabs are fed events (`shot_feed`); a transcript is read from a
//! scene folder of its own (never your home); the file sheet is answered by the scene.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{AnyWindowHandle, Entity, Focusable, Modifiers, VisualTestAppContext, px, size};

use super::agent::{Slot, build_with, feed, start, tab_of};
use super::chrome::settle_frames;
use super::{Kind, Scene, Themes, shoot_with, shot_theme, shot_themes};
use crate::agent_conversation::Item;
use crate::agent_process::ProcessEvent;
use crate::agent_tab::AgentTab;
use crate::providers::claude::CLAUDE;
use crate::providers::{Event, FileRewind, Provider};
use crate::status::Status;

pub(super) const FILE: &str = file!();

const SOURCES: &[&str] = &[
    "src/agent_rewind.rs",
    "src/agent_tab.rs",
    "src/agent_rows.rs",
    "src/agent_chat.rs",
    "src/agent_conversation.rs",
    "src/composer.rs",
    "src/icons.rs",
    "src/providers/mod.rs",
    "src/providers/claude.rs",
];

pub(super) const SCENES: &[Scene] = &[
    Scene {
        name: "agent-rewind",
        run: agent_rewind,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: SOURCES,
    },
    Scene {
        name: "agent-fork",
        run: agent_fork,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/agent_rewind.rs",
            "src/agent_tab.rs",
            "src/agent_rows.rs",
            "src/agent_conversation.rs",
            "src/providers/mod.rs",
            "src/providers/claude.rs",
            "src/workspace_agent.rs",
            "src/workspace.rs",
            "src/tab_bar.rs",
            "src/layout_model.rs",
        ],
    },
    Scene {
        name: "agent-rewind-agy",
        run: agent_rewind_agy,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: &[
            "src/agent_rewind.rs",
            "src/agent_tab.rs",
            "src/agent_rows.rs",
            "src/providers/mod.rs",
            "src/providers/agy.rs",
        ],
    },
];

const DIMS: (f32, f32) = (720.0, 640.0);
const FIRST: &str = "Fix the failing parser test";
const SECOND: &str = "Now refactor the parser into modules";
/// The reply's entry in the fake transcript (the cut's `parentUuid`).
const REPLY: &str = "3f0e9a52-1b7c-4d2e-8a61-0c9b7e4d5a10";

/// The conversation's id, as the tab chose it (`--session-id`).
fn session(cx: &VisualTestAppContext, tab: &Entity<AgentTab>) -> String {
    let argv = cx.read(|cx| tab.read(cx).shot_last_argv());
    value(&argv, "--session-id").expect("a chosen session id")
}

fn value(argv: &[String], flag: &str) -> Option<String> {
    argv.windows(2).find(|w| w[0] == flag).map(|w| w[1].clone())
}

/// Your messages' ids, in order.
fn ids(cx: &VisualTestAppContext, tab: &Entity<AgentTab>) -> Vec<String> {
    cx.read(|cx| {
        tab.read(cx)
            .shot_items()
            .iter()
            .filter_map(|i| match i {
                Item::You { id: Some(id), .. } => Some(id.clone()),
                _ => None,
            })
            .collect()
    })
}

fn index_of(cx: &VisualTestAppContext, tab: &Entity<AgentTab>, text: &str) -> usize {
    cx.read(|cx| {
        tab.read(cx)
            .shot_items()
            .iter()
            .position(|i| matches!(i, Item::You { text: t, .. } if t == text))
            .expect("the message")
    })
}

fn box_text(cx: &VisualTestAppContext, tab: &Entity<AgentTab>) -> String {
    cx.read(|cx| tab.read(cx).shot_composer().read(cx).text().to_string())
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

fn reply(text: &str) -> Vec<Event> {
    vec![
        Event::TextDelta {
            message: format!("m-{text}"),
            text: text.into(),
            parent: None,
        },
        Event::MessageDone {
            message: format!("m-{text}"),
            text: text.into(),
            parent: None,
        },
    ]
}

fn turn_done(ok: bool) -> Event {
    Event::TurnDone { ok, message: None }
}

/// A new conversation: the first message read and answered, the second read and stopped
/// mid-answer (as Claude says a stop: "Stopped", then the turn's end). The transcript (under
/// the scene's own `home`) holds both messages, the second after the reply.
fn talk_and_stop(cx: &mut VisualTestAppContext, tab: &Entity<AgentTab>, home: &std::path::Path) {
    feed(cx, tab, |t, cx| t.shot_history(Vec::new(), cx));
    let s = session(cx, tab);
    feed(cx, tab, |t, cx| t.send(FIRST.into(), cx));
    let u1 = ids(cx, tab)[0].clone();
    feed(cx, tab, |t, cx| {
        let mut events = vec![
            Event::Started {
                conversation: s.clone(),
                model: Some("claude-opus-5-5".into()),
            },
            Event::Taken {
                id: Some(u1.clone()),
            },
        ];
        events.extend(reply("The test expected a trailing comma; fixed."));
        events.push(turn_done(true));
        t.shot_feed(events, cx)
    });
    feed(cx, tab, |t, cx| t.send(SECOND.into(), cx));
    let u2 = ids(cx, tab)[1].clone();
    feed(cx, tab, |t, cx| {
        let mut events = vec![Event::Taken {
            id: Some(u2.clone()),
        }];
        events.extend(reply("Splitting it into lexer, parser and"));
        t.shot_feed(events, cx)
    });
    feed(cx, tab, |t, cx| t.shot_press_stop(cx));
    feed(cx, tab, |t, cx| {
        t.shot_feed(
            vec![
                Event::Notice(crate::agent_tab::STOPPED_NOTICE.into()),
                turn_done(false),
            ],
            cx,
        )
    });
    let dir = home.join(".claude/projects/-scene");
    std::fs::create_dir_all(&dir).expect("the transcript's folder");
    let user = |id: &str, parent: Option<&str>, text: &str| {
        serde_json::json!({
            "type": "user", "uuid": id, "parentUuid": parent,
            "message": {"role": "user", "content": text},
        })
        .to_string()
    };
    let lines = [
        user(&u1, None, FIRST),
        serde_json::json!({"type": "assistant", "uuid": REPLY, "parentUuid": u1}).to_string(),
        user(&u2, Some(REPLY), SECOND),
    ];
    std::fs::write(dir.join(format!("{s}.jsonl")), lines.join("\n")).expect("the transcript");
    let home = home.to_path_buf();
    feed(cx, tab, |t, _| t.rw.shot_home = Some(home));
}

/// Click what the tab laid out under `label` ("rewind:2", "stopped-rewind:5").
fn click(
    cx: &mut VisualTestAppContext,
    window: AnyWindowHandle,
    tab: &Entity<AgentTab>,
    label: &str,
) {
    settle_frames(cx, window);
    let at = cx
        .read(|cx| tab.read(cx).rw.shot_buttons.borrow().get(label).copied())
        .unwrap_or_else(|| panic!("{label} laid out"));
    // The pointer there first (a hover action shows on its row under the pointer).
    cx.simulate_mouse_move(window, at.center(), None, Modifiers::none());
    settle_frames(cx, window);
    cx.simulate_click(window, at.center(), Modifiers::none());
    settle_frames(cx, window);
}

fn answer(files: &[&str], lines: Option<(u64, u64)>) -> FileRewind {
    FileRewind {
        can_rewind: true,
        error: None,
        files: files.iter().map(|f| f.to_string()).collect(),
        insertions: lines.map(|l| l.0),
        deletions: lines.map(|l| l.1),
    }
}

fn files_answer(
    cx: &mut VisualTestAppContext,
    tab: &Entity<AgentTab>,
    message: &str,
    dry: bool,
    o: FileRewind,
) {
    let message = message.to_string();
    feed(cx, tab, |t, cx| {
        t.shot_feed(
            vec![Event::FilesRewound {
                message,
                dry_run: dry,
                outcome: o,
            }],
            cx,
        )
    });
}

/// The `rewind_files` requests the tab wrote (their `dry_run`).
fn rewinds_written(cx: &VisualTestAppContext, tab: &Entity<AgentTab>) -> Vec<bool> {
    cx.read(|cx| tab.read(cx).shot_written())
        .iter()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .filter(|v| v["request"]["subtype"] == "rewind_files")
        .map(|v| v["request"]["dry_run"].as_bool().unwrap_or(false))
        .collect()
}

/// Rewind (batch 3, chore 2), shot in both themes after a stop: "Stopped · Rewind to edit your
/// message", the stopped message's actions kept shown, the first message's shown under the
/// pointer, "Esc twice to rewind" in the box. Before the shot (checks):
/// - the hover actions: Copy, Rewind, Fork on a read message; an unread one has Copy only;
/// - the link rewinds: no files changed (the agent's dry run) → one click, the chat cut before
///   the message, its text back in the box, "Rewound · Undo", Claude started on a branch
///   (`--resume S --resume-session-at P --fork-session --session-id N`);
/// - Undo puts the chat back and resumes S; a second rewind with files changed asks (the sheet's
///   words), "Rewind and Undo Changes" undoes them through the agent, then rewinds;
/// - the next send takes N (no Undo after it); a `Started` with another id is adopted;
/// - a branch that ends before it started puts the old conversation back (never a new one);
/// - Esc Esc picks the newest message (↑ the older), ↩ rewinds it; the palette's Rewind Last
///   Prompt rewinds the newest.
pub(super) fn agent_rewind() {
    // The checks, once, each in a window of its own.
    checks("agent-rewind-flow", rewind_checks);
    checks("agent-rewind-failed", failed_checks);
    checks("agent-rewind-pick", pick_checks);
    checks("agent-rewind-working", working_checks);
    for &(slug, theme_name) in shot_themes() {
        let theme = shot_theme(theme_name);
        let slot: Slot = Rc::default();
        let keep = slot.clone();
        let n = format!("agent-rewind-{slug}");
        let home = crate::test_temp::dir(&format!("fuxx-ui-shots-rewind-{slug}"));
        let path = shoot_with(
            "agent-rewind",
            slug,
            &theme,
            size(px(DIMS.0), px(DIMS.1)),
            build_with(&CLAUDE, theme, slot, start()),
            move |cx, window| {
                let tab = tab_of(&keep);
                talk_and_stop(cx, &tab, &home);
                settle_frames(cx, window);
                let first = index_of(cx, &tab, FIRST);
                let second = index_of(cx, &tab, SECOND);
                let placeholder = cx.read(|cx| {
                    tab.read(cx)
                        .shot_composer()
                        .read(cx)
                        .placeholder()
                        .to_string()
                });
                assert_eq!(placeholder, "Esc twice to rewind", "{n}");
                let buttons = cx.read(|cx| tab.read(cx).rw.shot_buttons.borrow().clone());
                for name in ["copy", "rewind", "fork"] {
                    assert!(
                        buttons.contains_key(&format!("{name}:{second}")),
                        "{n}: {name}"
                    );
                }
                // The first message's actions under the pointer.
                let bubble = buttons[&format!("copy:{first}")];
                let at = gpui::point(bubble.right() + px(60.), bubble.center().y);
                cx.simulate_mouse_move(window, at, None, Modifiers::none());
                settle_frames(cx, window);
                keep.borrow_mut().take();
            },
        );
        println!("ui-shots: {}", path.display());
    }
}

/// `act` on a Claude tab in a Mocha window of its own (a scene folder for its transcript).
fn checks(
    name: &'static str,
    act: fn(&str, &mut VisualTestAppContext, AnyWindowHandle, &Entity<AgentTab>, &std::path::Path),
) {
    let theme = shot_theme("Catppuccin Mocha");
    let slot: Slot = Rc::default();
    let keep = slot.clone();
    let home = crate::test_temp::dir(&format!("fuxx-ui-shots-{name}"));
    super::render_with(
        &theme,
        size(px(DIMS.0), px(DIMS.1)),
        build_with(&CLAUDE, theme, slot, start()),
        move |cx, window| {
            let tab = tab_of(&keep);
            act(name, cx, window, &tab, &home);
            keep.borrow_mut().take();
        },
    );
    println!("ui-shots: {name}: checked");
}

fn rewind_checks(
    n: &str,
    cx: &mut VisualTestAppContext,
    window: AnyWindowHandle,
    tab: &Entity<AgentTab>,
    home: &std::path::Path,
) {
    use crate::agent_tab::BAD_RESUME_NOTICE;
    talk_and_stop(cx, tab, home);
    let s = session(cx, tab);
    let u2 = ids(cx, tab)[1].clone();
    let second = index_of(cx, tab, SECOND);
    cx.read(|cx| assert_eq!(tab.read(cx).status(), Status::Done, "{n}"));
    // The link after "Stopped" rewinds the stopped message; the agent's dry run says no file
    // changed: one click.
    let stopped = cx.read(|cx| {
        tab.read(cx)
            .shot_items()
            .iter()
            .rposition(|i| matches!(i, Item::Notice(t) if t == crate::agent_tab::STOPPED_NOTICE))
            .expect("the notice")
    });
    click(cx, window, tab, &format!("stopped-rewind:{stopped}"));
    assert_eq!(
        rewinds_written(cx, tab),
        [true],
        "{n}: the agent's dry run asked"
    );
    files_answer(cx, tab, &u2, true, answer(&[], None));
    settle_frames(cx, window);
    cx.read(|cx| {
        let t = tab.read(cx);
        assert!(t.rw.shot_asked.is_none(), "{n}: no sheet");
        let items = t.shot_items();
        assert_eq!(
            items.len(),
            second + 1,
            "{n}: cut before it, then the notice"
        );
        assert!(
            matches!(&items[second], Item::Notice(x) if x == "Rewound"),
            "{n}"
        );
    });
    assert_eq!(box_text(cx, tab), SECOND, "{n}: the text back in the box");
    let argv = cx.read(|cx| tab.read(cx).shot_last_argv());
    assert_eq!(value(&argv, "--resume").as_deref(), Some(s.as_str()), "{n}");
    assert_eq!(
        value(&argv, "--resume-session-at").as_deref(),
        Some(REPLY),
        "{n}"
    );
    assert!(argv.iter().any(|a| a == "--fork-session"), "{n}: {argv:?}");
    let branch = value(&argv, "--session-id").expect("the branch's id");
    assert_ne!(branch, s, "{n}: a new session id");
    // Undo: the chat and the old conversation back, the box emptied.
    click(cx, window, tab, &format!("rewound-undo:{second}"));
    cx.read(|cx| {
        let t = tab.read(cx);
        assert_eq!(t.shot_items().len(), second + 3, "{n}: all of it back");
        assert_eq!(t.resume_id().as_deref(), Some(s.as_str()), "{n}");
    });
    let argv = cx.read(|cx| tab.read(cx).shot_last_argv());
    assert_eq!(value(&argv, "--resume").as_deref(), Some(s.as_str()), "{n}");
    assert!(!argv.iter().any(|a| a == "--fork-session"), "{n}: {argv:?}");
    assert_eq!(box_text(cx, tab), "", "{n}");
    // Again, from the hover action, with files changed: the sheet, and the files undone first.
    feed(cx, tab, |t, _| t.rw.shot_answer = Some(0));
    click(cx, window, tab, &format!("rewind:{second}"));
    files_answer(
        cx,
        tab,
        &u2,
        true,
        answer(&["src/lexer.rs", "src/parser.rs"], Some((12, 4))),
    );
    settle_frames(cx, window);
    cx.read(|cx| {
        let asked = tab.read(cx).rw.shot_asked.clone().expect("the sheet");
        assert_eq!(asked.title, "Rewind to this message?", "{n}");
        assert_eq!(
            asked.detail,
            "The agent changed 2 files after it (+12 \u{2212}4). Undo those changes too, or keep \
             them and rewind only the chat.",
            "{n}"
        );
        assert_eq!(
            asked.buttons,
            ["Rewind and Undo Changes", "Rewind Chat Only", "Cancel"],
            "{n}"
        );
    });
    assert_eq!(
        rewinds_written(cx, tab),
        [true, true, false],
        "{n}: then undone for real"
    );
    files_answer(
        cx,
        tab,
        &u2,
        false,
        answer(&["src/lexer.rs", "src/parser.rs"], None),
    );
    settle_frames(cx, window);
    assert_eq!(box_text(cx, tab), SECOND, "{n}");
    assert_eq!(
        notices(cx, tab).last().map(String::as_str),
        Some("Rewound"),
        "{n}: no failure noted"
    );
    let branch = value(&cx.read(|cx| tab.read(cx).shot_last_argv()), "--session-id").unwrap();
    // The next send goes to the branch (its id is the tab's); Undo is gone.
    feed(cx, tab, |t, cx| {
        t.send("Refactor only the lexer".into(), cx)
    });
    feed(cx, tab, |t, cx| t.shot_feed(Vec::new(), cx));
    cx.read(|cx| {
        let t = tab.read(cx);
        assert_eq!(t.resume_id().as_deref(), Some(branch.as_str()), "{n}");
    });
    let buttons = cx.read(|cx| tab.read(cx).rw.shot_buttons.borrow().clone());
    let _ = buttons;
    settle_frames(cx, window);
    cx.read(|cx| {
        assert!(
            !tab.read(cx)
                .rw
                .shot_buttons
                .borrow()
                .keys()
                .any(|k| k.starts_with("rewound-undo")),
            "{n}: no Undo after a send"
        )
    });
    // Task 0's fallback: Claude names the branch another id; the tab takes it.
    const OTHER: &str = "9b2d7c41-5e3f-4a8b-b6c0-1d2e3f4a5b6c";
    feed(cx, tab, |t, cx| {
        t.shot_feed(
            vec![Event::Started {
                conversation: OTHER.into(),
                model: None,
            }],
            cx,
        )
    });
    cx.read(|cx| assert_eq!(tab.read(cx).resume_id().as_deref(), Some(OTHER), "{n}"));
    assert!(
        !notices(cx, tab).iter().any(|t| t == BAD_RESUME_NOTICE),
        "{n}"
    );
}

/// A branch that ends before the agent said it started: the old conversation back (never a new
/// one), why, and what was sent back in the box.
fn failed_checks(
    n: &str,
    cx: &mut VisualTestAppContext,
    window: AnyWindowHandle,
    tab: &Entity<AgentTab>,
    home: &std::path::Path,
) {
    use crate::agent_tab::BAD_RESUME_NOTICE;
    // A branch that ends before it started: the old conversation back, never a new one.
    talk_and_stop(cx, tab, home);
    let s = session(cx, tab);
    let second = index_of(cx, tab, SECOND);
    let u2 = ids(cx, tab)[1].clone();
    let whole = cx.read(|cx| tab.read(cx).shot_items().len());
    click(cx, window, tab, &format!("rewind:{second}"));
    files_answer(cx, tab, &u2, true, answer(&[], None));
    feed(cx, tab, |t, cx| t.send("Try again".into(), cx));
    feed(cx, tab, |t, cx| {
        t.shot_process(ProcessEvent::Ended { code: Some(1) }, cx)
    });
    feed(cx, tab, |t, cx| {
        t.shot_process(
            ProcessEvent::Exited {
                code: Some(1),
                stderr_tail: "No message found with message.uuid".into(),
            },
            cx,
        )
    });
    cx.read(|cx| {
        let t = tab.read(cx);
        assert_eq!(
            t.resume_id().as_deref(),
            Some(s.as_str()),
            "{n}: the old one"
        );
        assert_eq!(t.shot_items().len(), whole + 1, "{n}: all back, and why");
    });
    assert_eq!(
        notices(cx, tab).last().map(String::as_str),
        Some("Couldn't rewind: No message found with message.uuid"),
        "{n}"
    );
    assert!(
        !notices(cx, tab).iter().any(|t| t == BAD_RESUME_NOTICE),
        "{n}"
    );
    let argv = cx.read(|cx| tab.read(cx).shot_last_argv());
    assert_eq!(value(&argv, "--resume").as_deref(), Some(s.as_str()), "{n}");
    assert!(!argv.iter().any(|a| a == "--fork-session"), "{n}");
    assert_eq!(
        box_text(cx, tab),
        "Try again",
        "{n}: what was sent, back in the box"
    );
}

/// Esc Esc picks the newest message (↑ the older, Esc leaves, ↩ rewinds); the palette's Rewind
/// Last Prompt rewinds the newest left.
fn pick_checks(
    n: &str,
    cx: &mut VisualTestAppContext,
    window: AnyWindowHandle,
    tab: &Entity<AgentTab>,
    home: &std::path::Path,
) {
    talk_and_stop(cx, tab, home);
    let (u1, u2) = {
        let ids = ids(cx, tab);
        (ids[0].clone(), ids[1].clone())
    };
    // Esc Esc: the pick on the newest message; ↑ the older; Esc leaves; ↩ rewinds.
    let first = index_of(cx, tab, FIRST);
    let second = index_of(cx, tab, SECOND);
    cx.update_window(window, |_, window, cx| {
        let focus = tab.read(cx).focus_handle(cx);
        focus.focus(window, cx);
    })
    .unwrap();
    cx.simulate_keystrokes(window, "escape escape");
    settle_frames(cx, window);
    cx.read(|cx| assert_eq!(tab.read(cx).rw.pick, Some(second), "{n}: the newest"));
    cx.simulate_keystrokes(window, "up");
    cx.read(|cx| assert_eq!(tab.read(cx).rw.pick, Some(first), "{n}: the older"));
    cx.simulate_keystrokes(window, "escape");
    cx.read(|cx| assert_eq!(tab.read(cx).rw.pick, None, "{n}: left"));
    cx.simulate_keystrokes(window, "escape escape");
    cx.simulate_keystrokes(window, "enter");
    settle_frames(cx, window);
    assert_eq!(
        rewinds_written(cx, tab).last(),
        Some(&true),
        "{n}: ↩ rewinds"
    );
    files_answer(cx, tab, &u2, true, answer(&[], None));
    settle_frames(cx, window);
    assert_eq!(box_text(cx, tab), SECOND, "{n}");
    // The palette's Rewind Last Prompt: the newest message left (the first).
    cx.update_window(window, |_, window, cx| {
        window.dispatch_action(Box::new(crate::agent_tab::RewindLastPrompt), cx)
    })
    .unwrap();
    settle_frames(cx, window);
    files_answer(cx, tab, &u1, true, answer(&[], None));
    settle_frames(cx, window);
    cx.read(|cx| {
        let items = tab.read(cx).shot_items();
        assert!(
            matches!(items, [Item::Notice(x)] if x == "Rewound"),
            "{n}: cut before the first: {items:?}"
        );
    });
    let argv = cx.read(|cx| tab.read(cx).shot_last_argv());
    assert!(
        value(&argv, "--resume").is_none() && value(&argv, "--session-id").is_some(),
        "{n}: the first cut: a fresh session, {argv:?}"
    );
}

/// Rewind while a turn runs: it stops the turn first ("Stopping…"), and goes on only once the
/// turn has ended.
fn working_checks(
    n: &str,
    cx: &mut VisualTestAppContext,
    window: AnyWindowHandle,
    tab: &Entity<AgentTab>,
    home: &std::path::Path,
) {
    talk_and_stop(cx, tab, home);
    let first = index_of(cx, tab, FIRST);
    let u1 = ids(cx, tab)[0].clone();
    // A new turn, working.
    feed(cx, tab, |t, cx| t.send("Add a changelog entry".into(), cx));
    let u3 = ids(cx, tab)[2].clone();
    feed(cx, tab, |t, cx| {
        let mut events = vec![Event::Taken { id: Some(u3) }];
        events.extend(reply("Opening CHANGELOG.md"));
        t.shot_feed(events, cx)
    });
    cx.read(|cx| assert_eq!(tab.read(cx).status(), Status::Working, "{n}"));
    click(cx, window, tab, &format!("rewind:{first}"));
    cx.read(|cx| assert_eq!(tab.read(cx).shot_stop_label(), Some("Stopping…"), "{n}"));
    assert!(
        rewinds_written(cx, tab).is_empty(),
        "{n}: nothing asked before the turn ends"
    );
    feed(cx, tab, |t, cx| {
        t.shot_feed(
            vec![
                Event::Notice(crate::agent_tab::STOPPED_NOTICE.into()),
                turn_done(false),
            ],
            cx,
        )
    });
    settle_frames(cx, window);
    assert_eq!(rewinds_written(cx, tab), [true], "{n}: then the dry run");
    files_answer(cx, tab, &u1, true, answer(&[], None));
    settle_frames(cx, window);
    assert_eq!(box_text(cx, tab), FIRST, "{n}");
}

/// Fork (batch 3, chore 2), shot in both themes: a Claude tab's second message forked from its
/// hover action. The fork opens right after the source in its pane, shown and focused, titled
/// "⟨source⟩ (fork)", the chat before the message with "Forked from ⟨source⟩" and the shared
/// folder's warning first, the message in its box unsent; it starts on a branch of the source's
/// conversation; the source goes on untouched; the tooltip's second line says where it came
/// from; the name in the first row shows the source.
pub(super) fn agent_fork() {
    use crate::layout_model::TabKind;
    for &(slug, theme_name) in shot_themes() {
        let theme = shot_theme(theme_name);
        let n = format!("agent-fork-{slug}");
        let home = crate::test_temp::dir(&format!("fuxx-ui-shots-fork-{slug}"));
        let slot: Rc<RefCell<Option<Entity<crate::workspace::Workspace>>>> = Rc::default();
        let build = super::agent::agent_workspace(theme, slot.clone(), |ws, window, cx| {
            let my_app = std::path::Path::new("/Users/me/code/my-app");
            let id = ws.open_unnamed_shot_session(Some(my_app), super::SHELL_TEXT, window, cx);
            ws.agent_beside(id, "claude", window, cx);
        });
        let path = shoot_with(
            "agent-fork",
            slug,
            &theme,
            size(px(1100.0), px(640.0)),
            build,
            move |cx, window| {
                let ws = slot.borrow_mut().take().expect("the workspace");
                settle_frames(cx, window);
                let (source, view) = cx
                    .read(|cx| ws.read(cx).focused_agent_tab())
                    .expect("the agent tab");
                talk_and_stop(cx, &view, &home);
                let s = session(cx, &view);
                let second = index_of(cx, &view, SECOND);
                let before = cx.read(|cx| view.read(cx).shot_items().to_vec());
                click(cx, window, &view, &format!("fork:{second}"));
                settle_frames(cx, window);
                let (fork, fork_view) = cx
                    .read(|cx| ws.read(cx).focused_agent_tab())
                    .expect("the fork, focused");
                assert_ne!(fork, source, "{n}: a new tab");
                cx.read(|cx| {
                    let ws = ws.read(cx);
                    let (_, pane) = ws.model.locate(source).expect("the source");
                    let tabs: Vec<_> = ws
                        .model
                        .pane(pane)
                        .unwrap()
                        .tabs
                        .iter()
                        .map(|t| t.id)
                        .collect();
                    let at = tabs.iter().position(|t| *t == source).unwrap();
                    assert_eq!(tabs.get(at + 1), Some(&fork), "{n}: right after the source");
                    match ws.model.tab(fork).map(|t| &t.kind) {
                        Some(TabKind::Agent { title, cwd, .. }) => {
                            assert_eq!(
                                title.as_deref(),
                                Some(format!("{FIRST} (fork)").as_str()),
                                "{n}"
                            );
                            assert_eq!(cwd, std::path::Path::new("/Users/me/code/my-app"), "{n}");
                        }
                        other => panic!("{n}: {other:?}"),
                    }
                    let tip = ws.tab_tip(fork, std::time::Instant::now()).expect("a tip");
                    assert!(
                        tip.ends_with(&format!(
                            "\nForked from {FIRST} at \u{201c}{SECOND}\u{201d}"
                        )),
                        "{n}: {tip}"
                    );
                    assert_eq!(
                        view.read(cx).shot_items(),
                        &before[..],
                        "{n}: the source untouched"
                    );
                    let items = fork_view.read(cx).shot_items();
                    assert!(matches!(&items[0], Item::Notice(t) if t == "Forked"), "{n}");
                    assert_eq!(&items[1..], &before[..second], "{n}: the chat before it");
                });
                assert_eq!(box_text(cx, &fork_view), SECOND, "{n}: unsent, in its box");
                // Its first start branches off the source's conversation.
                feed(cx, &fork_view, |t, cx| t.shot_history(Vec::new(), cx));
                let argv = cx.read(|cx| fork_view.read(cx).shot_last_argv());
                assert_eq!(value(&argv, "--resume").as_deref(), Some(s.as_str()), "{n}");
                assert_eq!(
                    value(&argv, "--resume-session-at").as_deref(),
                    Some(REPLY),
                    "{n}"
                );
                assert!(argv.iter().any(|a| a == "--fork-session"), "{n}");
                // The source's name shows the source; back to the fork for the picture.
                click(cx, window, &fork_view, "fork-source:0");
                let focused = cx.read(|cx| ws.read(cx).focused_agent_tab().map(|(t, _)| t));
                assert_eq!(focused, Some(source), "{n}: the source shown");
                cx.update_window(window, |_, window, cx| {
                    ws.update(cx, |ws, cx| ws.select_tab(fork, window, cx))
                })
                .unwrap();
                settle_frames(cx, window);
            },
        );
        println!("ui-shots: agent-fork-{slug}: {}", path.display());
    }
}

/// agy (batch 3, chore 2): it can't go back from fuxx, so your message offers Copy and Edit
/// only; Edit puts its text back in the box and leaves the chat as it is; Esc Esc picks nothing.
pub(super) fn agent_rewind_agy() {
    use crate::providers::agy::AGY;
    let n = "agent-rewind-agy";
    let theme = shot_theme("Catppuccin Mocha");
    let slot: Slot = Rc::default();
    let keep = slot.clone();
    let start = crate::providers::Start {
        mode: AGY.info().default_mode.map(String::from),
        ..start()
    };
    let path = shoot_with(
        n,
        "mocha",
        &theme,
        size(px(DIMS.0), px(DIMS.1)),
        build_with(&AGY, theme, slot, start),
        move |cx, window| {
            let tab = tab_of(&keep);
            feed(cx, &tab, |t, cx| t.shot_history(Vec::new(), cx));
            feed(cx, &tab, |t, cx| t.send("Say hello in one word".into(), cx));
            feed(cx, &tab, |t, cx| {
                let mut events = vec![Event::Started {
                    conversation: "agy-conversation".into(),
                    model: None,
                }];
                events.extend(reply("Hello"));
                events.push(turn_done(true));
                t.shot_feed(events, cx)
            });
            settle_frames(cx, window);
            let buttons = cx.read(|cx| tab.read(cx).rw.shot_buttons.borrow().clone());
            let mut names: Vec<&str> = buttons
                .keys()
                .filter_map(|k| k.strip_suffix(":0"))
                .collect();
            names.sort();
            assert_eq!(names, ["copy", "edit"], "{n}");
            let items = cx.read(|cx| tab.read(cx).shot_items().len());
            click(cx, window, &tab, "edit:0");
            assert_eq!(box_text(cx, &tab), "Say hello in one word", "{n}");
            assert_eq!(
                cx.read(|cx| tab.read(cx).shot_items().len()),
                items,
                "{n}: not cut"
            );
            feed(cx, &tab, |t, cx| {
                t.shot_composer()
                    .update(cx, |c, cx| c.set_text(String::new(), cx))
            });
            cx.update_window(window, |_, window, cx| {
                let focus = tab.read(cx).focus_handle(cx);
                focus.focus(window, cx);
            })
            .unwrap();
            cx.simulate_keystrokes(window, "escape escape");
            cx.read(|cx| assert_eq!(tab.read(cx).rw.pick, None, "{n}: no pick"));
            keep.borrow_mut().take();
        },
    );
    println!("ui-shots: {n}: {}", path.display());
}
