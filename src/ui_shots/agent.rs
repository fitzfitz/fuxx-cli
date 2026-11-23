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
/// The chat selection's files (chore after M11ar), on top of the tab's.
const SELECT: &[&str] = &[
    "src/agent_tab.rs",
    "src/agent_rows.rs",
    "src/chat_select.rs",
    "src/md_preview.rs",
    "src/composer.rs",
];

const TAB: &[&str] = &[
    "src/agent_tab.rs",
    "src/agent_top.rs",
    "src/agent_chat.rs",
    "src/agent_tool_modal.rs",
    "src/logos.rs",
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
        name: "agent-tab-wide",
        run: agent_tab_wide,
        themes: Themes::Both,
        serial: false,
        kind: Kind::State,
        sources: &[
            "src/agent_tab.rs",
            "src/agent_box.rs",
            "src/ui_tokens.rs",
            "src/providers/fixtures/claude-text.jsonl",
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
        name: "agent-chat-latest-prompt",
        run: agent_chat_latest_prompt,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/agent_tab.rs",
            "src/agent_latest.rs",
            "src/agent_rows.rs",
            "src/agent_lanes_view.rs",
            "src/agent_chat.rs",
            "src/agent_conversation.rs",
            "src/sidebar_toggle.rs",
        ],
    },
    Scene {
        name: "agent-box-sweep",
        run: agent_box_sweep,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: TAB,
    },
    Scene {
        name: "agent-tab-modal",
        run: agent_tab_modal,
        themes: Themes::Both,
        serial: false,
        kind: Kind::State,
        sources: TAB,
    },
    Scene {
        name: "agent-tab-modal-open",
        run: agent_tab_modal_open,
        themes: Themes::One,
        serial: true,
        kind: Kind::State,
        sources: TAB,
    },
    Scene {
        name: "agent-tab-modal-guard",
        run: agent_tab_modal_guard,
        themes: Themes::Both,
        serial: false,
        kind: Kind::State,
        sources: TAB,
    },
    Scene {
        name: "agent-tab-bar",
        run: agent_tab_bar,
        themes: Themes::Both,
        serial: false,
        kind: Kind::State,
        sources: TAB,
    },
    Scene {
        name: "agent-tab-thinking",
        run: agent_tab_thinking,
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
        name: "agent-select",
        run: agent_select,
        themes: Themes::Both,
        serial: false,
        kind: Kind::State,
        sources: SELECT,
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
        name: "agent-edit-reason-live-only",
        run: agent_edit_reason_live_only,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: &[
            "src/agent_tab.rs",
            "src/edit_notes.rs",
            "src/providers/mod.rs",
            "src/providers/claude.rs",
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

pub(super) type Slot = Rc<RefCell<Option<Entity<AgentTab>>>>;

const DIMS: (f32, f32) = (720.0, 640.0);

/// A folder under your home (so it reads `~/code/my-app` on any Mac); nothing is read from it.
fn cwd() -> PathBuf {
    crate::workspace::home_dir()
        .unwrap_or_else(|| PathBuf::from("/Users/me"))
        .join("code/my-app")
}

pub(super) fn start() -> Start {
    Start {
        cwd: cwd(),
        resume: None,
        new_id: None,
        mode: None,
        extra_args: Vec::new(),
        model: None,
        effort: None,
        branch: None,
    }
}

/// A fixture's lines, decoded as the process's would be.
pub(super) fn decode(fixture: &str) -> Vec<Event> {
    let mut t = CLAUDE.translator();
    fixture.lines().flat_map(|l| t.decode(l)).collect()
}

pub(super) fn tab_of(slot: &Slot) -> Entity<AgentTab> {
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
pub(super) fn build_with(
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
pub(super) fn feed(
    cx: &mut VisualTestAppContext,
    tab: &Entity<AgentTab>,
    f: impl FnOnce(&mut AgentTab, &mut Context<AgentTab>),
) {
    cx.update(|cx| tab.update(cx, f));
    cx.run_until_parked();
}

/// `events` fed one by one as the process would bring them, each permission allowed as you
/// would (the recording's own answers aren't in its stdout).
pub(super) fn play(cx: &mut VisualTestAppContext, tab: &Entity<AgentTab>, events: Vec<Event>) {
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
    scene_sized(name, DIMS, act)
}

/// `scene` in a window of `dims`.
pub(super) fn scene_sized(
    name: &str,
    dims: (f32, f32),
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
            size(px(dims.0), px(dims.1)),
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
        feed(cx, tab, |t, cx| t.shot_open_tool(edit, cx));
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

/// Spec A §B: an Edit's detail in the modal over the tab (motion off, so the shot is its end
/// state): the scrim under the top bar, the header, the diff in its well.
pub(super) fn agent_tab_modal() {
    scene("agent-tab-modal", |n, cx, _, tab| {
        cx.update(|cx| cx.set_reduce_motion(true));
        let events = decode(include_str!("../providers/fixtures/claude-tools.jsonl"));
        feed(cx, tab, |t, cx| t.send("Change alpha to beta".into(), cx));
        play(cx, tab, events);
        let edit = cx.read(|cx| {
            tab.read(cx)
                .shot_items()
                .iter()
                .rposition(
                    |i| matches!(i, Item::Tool { name, ok: Some(true), .. } if name == "Edit"),
                )
                .expect("an Edit row")
        });
        feed(cx, tab, |t, cx| t.shot_open_tool(edit, cx));
        let id = cx.read(|cx| match &tab.read(cx).shot_items()[edit] {
            Item::Tool { id, .. } => id.clone(),
            _ => unreachable!(),
        });
        assert!(
            cx.read(|cx| tab.read(cx).shot_open(&id)),
            "{n}: the modal shows it"
        );
    });
}

/// Your report (2026-11-22: "when I click to show the modal, it first show blinking first then
/// show the modal"): once the modal has faded in, no later frame loses it — a point on its panel
/// keeps its colour through the open animation's end. (Real time: the open animation runs on
/// the wall clock. A frame here takes ~90 ms, so a one-frame blink can fall between samples:
/// `agent_tab::modal_motion`'s test pins that cause; this catches a lasting loss.)
pub(super) fn agent_tab_modal_open() {
    scene("agent-tab-modal-open", |n, cx, window, tab| {
        feed(cx, tab, |t, cx| t.send("List the folder".into(), cx));
        play(
            cx,
            tab,
            vec![
                Event::ToolStarted {
                    id: "b1".into(),
                    name: "Bash".into(),
                    summary: "ls".into(),
                    parent: None,
                    detail: Some(crate::providers::ToolDetail::Command("ls".into())),
                    writes: Vec::new(),
                    texts: Vec::new(),
                },
                Event::ToolFinished {
                    id: "b1".into(),
                    ok: true,
                    summary: "ok".into(),
                    output: Some("a\nb".into()),
                },
            ],
        );
        feed(cx, tab, |t, cx| t.shot_open_tool(1, cx));
        let started = std::time::Instant::now();
        let mut seen: Vec<(u128, [u8; 3])> = Vec::new();
        while started.elapsed() < std::time::Duration::from_millis(450) {
            let shot = frame_fresh(cx, window);
            if let Some(panel) = cx.read(|cx| tab.read(cx).shot_part("modal")) {
                let at = (f32::from(panel.left()) + 6.0, f32::from(panel.top()) + 20.0);
                let dims = cx
                    .update_window(window, |_, w, _| w.viewport_size())
                    .expect("the window's size");
                let p = super::pixel_at(&shot, dims, at.0, at.1);
                seen.push((started.elapsed().as_millis(), p));
            }
            std::thread::sleep(std::time::Duration::from_millis(8));
        }
        let last = seen.last().expect("the modal was drawn").1;
        let close = |p: [u8; 3]| p.iter().zip(last).all(|(a, b)| a.abs_diff(b) <= 3);
        let shown = seen
            .iter()
            .position(|(_, p)| close(*p))
            .expect("it settles");
        let lost: Vec<_> = seen[shown..].iter().filter(|(_, p)| !close(*p)).collect();
        assert!(
            lost.is_empty(),
            "{n}: the modal blinks after showing (ms, colour): {lost:?} (settled {last:?})"
        );
    });
}

/// The review's finding 1 and 2: with the modal open over a working tab, nothing under it is
/// pressed — a click on the dim area over Stop closes the modal and leaves the agent working,
/// one over another tool's row doesn't open that tool — and Esc never stops the agent, even
/// once the tab is focused again (as the Workspace does after a palette or a tab switch).
pub(super) fn agent_tab_modal_guard() {
    scene("agent-tab-modal-guard", |n, cx, window, tab| {
        use gpui::{Modifiers, point};
        feed(cx, tab, |t, cx| t.send("List the folder".into(), cx));
        let bash = |id: &str, cmd: &str| Event::ToolStarted {
            id: id.into(),
            name: "Bash".into(),
            summary: cmd.into(),
            parent: None,
            detail: Some(crate::providers::ToolDetail::Command(cmd.into())),
            writes: Vec::new(),
            texts: Vec::new(),
        };
        let done = |id: &str| Event::ToolFinished {
            id: id.into(),
            ok: true,
            summary: "ok".into(),
            output: Some("a\nb".into()),
        };
        play(
            cx,
            tab,
            vec![bash("b1", "ls"), done("b1"), bash("b2", "pwd"), done("b2")],
        );
        let working = |cx: &mut VisualTestAppContext| {
            cx.read(|cx| tab.read(cx).shot_stop_label()) == Some("■ Stop  esc")
        };
        assert!(working(cx), "{n}: it works");
        let open = |cx: &mut VisualTestAppContext| {
            feed(cx, tab, |t, cx| t.shot_open_tool(1, cx));
            let _ = frame_fresh(cx, window);
            assert!(cx.read(|cx| tab.read(cx).shot_open("b1")), "{n}: open");
        };
        // The dim area over Stop.
        open(cx);
        let stop = cx
            .read(|cx| tab.read(cx).shot_part("send"))
            .expect("the Stop button");
        cx.simulate_click(window, stop.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(!cx.read(|cx| tab.read(cx).shot_open("b1")), "{n}: closed");
        assert!(working(cx), "{n}: the click didn't reach Stop");
        // Closed, the keyboard is back in the box (the modal had it).
        let _ = frame_fresh(cx, window);
        let in_box = cx.update(|cx| {
            let composer = tab.read(cx).shot_composer().read(cx).focus_handle(cx);
            window
                .update(cx, |_, window, _| composer.is_focused(window))
                .unwrap_or(false)
        });
        assert!(in_box, "{n}: the box has the keyboard again");
        // The dim area over the second tool's row (rows from 48: the bar, a 16 gap, ~30 each).
        open(cx);
        cx.simulate_click(window, point(px(DIMS.0 - 12.), px(128.)), Modifiers::none());
        cx.run_until_parked();
        assert!(
            !cx.read(|cx| tab.read(cx).shot_open("b2")),
            "{n}: no other tool opened"
        );
        // Esc after the tab is focused again: the modal goes, the agent works on.
        open(cx);
        cx.update(|cx| {
            let handle = tab.read(cx).focus_handle(cx);
            let _ = window.update(cx, |_, window, cx| handle.focus(window, cx));
        });
        cx.run_until_parked();
        cx.simulate_keystrokes(window, "escape");
        cx.run_until_parked();
        assert!(
            !cx.read(|cx| tab.read(cx).shot_open("b1")),
            "{n}: Esc closed it"
        );
        assert!(working(cx), "{n}: Esc didn't stop the agent");
        open(cx);
    });
}

/// Spec A §C: the top bar after two turns — Done, the cost and what the last turn added, the
/// context meter in its critical band (92 %).
pub(super) fn agent_tab_bar() {
    scene("agent-tab-bar", |n, cx, _, tab| {
        cx.update(|cx| cx.set_reduce_motion(true));
        let turn = |text: &str, pct: u8, usd: f64| {
            vec![
                Event::MessageDone {
                    message: format!("m-{pct}"),
                    text: text.into(),
                    parent: None,
                },
                Event::Usage {
                    context_pct: Some(pct),
                    cost_usd: Some(usd),
                },
                Event::TurnDone {
                    ok: true,
                    message: None,
                },
            ]
        };
        feed(cx, tab, |t, cx| t.send("Look at the tests".into(), cx));
        play(cx, tab, turn("They pass.", 40, 3.57));
        feed(cx, tab, |t, cx| t.send("And the docs?".into(), cx));
        play(cx, tab, turn("The docs are up to date.", 92, 3.99));
        cx.read(|cx| assert_eq!(tab.read(cx).status(), Status::Done, "{n}"));
    });
}

/// Spec B-B: "Cultivating…" (your word; the spec's "Thinking…") under the last row while the turn works and nothing moves (motion
/// off: the fade's end).
pub(super) fn agent_tab_thinking() {
    scene("agent-tab-thinking", |n, cx, _, tab| {
        cx.update(|cx| cx.set_reduce_motion(true));
        feed(cx, tab, |t, cx| t.send("Find the flaky test".into(), cx));
        play(
            cx,
            tab,
            vec![Event::MessageDone {
                message: "m".into(),
                text: "Let me look at the recent runs.".into(),
                parent: None,
            }],
        );
        cx.read(|cx| assert_eq!(tab.read(cx).status(), Status::Working, "{n}"));
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

/// A wide window (your words, 2026-11-22: "put it at center"): the conversation's column and the
/// box under it centred, and the box as wide as the column (it ran the full width before).
pub(super) fn agent_tab_wide() {
    const W: f32 = 1600.0;
    scene_sized("agent-tab-wide", (W, 640.0), |n, cx, window, tab| {
        let events = decode(include_str!("../providers/fixtures/claude-text.jsonl"));
        feed(cx, tab, |t, cx| t.send("Say hello in one word".into(), cx));
        feed(cx, tab, |t, cx| t.shot_feed(events, cx));
        let _ = frame_fresh(cx, window);
        cx.read(|cx| {
            let t = tab.read(cx);
            let outer = t
                .shot_part("outer")
                .unwrap_or_else(|| panic!("{n}: the box"));
            let gutter = t
                .shot_part("column")
                .unwrap_or_else(|| panic!("{n}: the reply's avatar column"));
            let (l, r) = (f32::from(outer.left()), f32::from(outer.right()));
            assert!(
                r - l <= crate::ui_tokens::chat::COLUMN_MAX + 1.0,
                "{n}: the box as wide as the column at most ({l}..{r})"
            );
            assert!(
                ((l + r) / 2.0 - W / 2.0).abs() <= 2.0,
                "{n}: the box centred ({l}..{r})"
            );
            assert!(
                (f32::from(gutter.left()) - l).abs() <= 2.0,
                "{n}: the column starts where the box does ({:?} vs {l})",
                gutter.left()
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
                assert_eq!(mode(cx).as_deref(), Some("auto"), "{n}: the default");
                // Before the first message the menu's choices apply, one after another (each a
                // real change from the one before).
                for want in ["plan", "accept-edits", "auto"] {
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
        feed(cx, tab, |t, cx| t.shot_open_tool(big, cx));
        feed(cx, tab, |t, cx| t.shot_modal_key("escape", cx));
    });
}

/// The latest-prompt strip (chore after M11ar): your message scrolled off by the agent's output
/// shows as one line over the chat's top (the chat under it doesn't move), the panel's card
/// that lights under the pointer with its tooltip; a click glides the chat back to the message
/// (the strip hides, following the end stops) and scrolling to the end shows it again. A lane's
/// view says the task Main gave it. A second message replaces the line; the picture keeps it.
pub(super) fn agent_chat_latest_prompt() {
    use gpui::{Modifiers, ScrollDelta, ScrollWheelEvent, TouchPhase, point};
    const FIRST: &str = "Fix the failing date test\n\nin fuxx-cli/src   and run cargo test again";
    const FIRST_LINE: &str = "Fix the failing date test in fuxx-cli/src and run cargo test again";
    const SECOND: &str = "Now add a note about the fix to the README in code/my-app";
    fn turn(tag: &str, replies: usize) -> Vec<Event> {
        let mut events: Vec<Event> = (0..replies)
            .map(|i| Event::MessageDone {
                message: format!("msg_{tag}_{i}"),
                text: format!(
                    "Step {i}: read the next file in fuxx-cli/src.\n\nIt parses dates before \
                     comparing them; nothing else changes here."
                ),
                parent: None,
            })
            .collect();
        events.push(Event::TurnDone {
            ok: true,
            message: None,
        });
        events
    }
    fn settle(cx: &mut VisualTestAppContext, window: AnyWindowHandle, tab: &Entity<AgentTab>) {
        let until = Instant::now() + Duration::from_secs(5);
        loop {
            settle_frames(cx, window);
            if !cx.read(|cx| tab.read(cx).shot_gliding()) {
                break;
            }
            assert!(Instant::now() < until, "the jump's glide ends");
        }
        settle_frames(cx, window);
    }
    scene("agent-chat-latest-prompt", |n, cx, window, tab| {
        let strip = |cx: &mut VisualTestAppContext| cx.read(|cx| tab.read(cx).shot_latest_strip());
        let list = cx.read(|cx| tab.read(cx).shot_list());
        assert_eq!(strip(cx), None, "{n}: nothing said: no strip");
        feed(cx, tab, |t, cx| t.send(FIRST.into(), cx));
        settle(cx, window, tab);
        assert_eq!(strip(cx), None, "{n}: your message on screen: no strip");
        let viewport = list.viewport_bounds();
        feed(cx, tab, |t, cx| t.shot_feed(turn("a", 14), cx));
        settle(cx, window, tab);
        assert_eq!(
            strip(cx).as_deref(),
            Some(FIRST_LINE),
            "{n}: scrolled off by the output: one line of it"
        );
        assert_eq!(
            list.viewport_bounds(),
            viewport,
            "{n}: the strip lies over the chat, which doesn't move"
        );
        // The panel's card; a step lighter under the pointer, with its tooltip.
        let colors = cx.read(|cx| {
            crate::agent_tab::Colors::of(&cx.global::<crate::appearance::Appearance>().theme)
        });
        let card = cx
            .read(|cx| tab.read(cx).shot_part("latest-strip"))
            .unwrap_or_else(|| panic!("{n}: the strip drawn"));
        assert!(
            (28.0..34.0).contains(&f32::from(card.size.height)),
            "{n}: about a row tall: {card:?}"
        );
        let dims = size(px(DIMS.0), px(DIMS.1));
        let fill_at = (f32::from(card.left()) + 5.0, f32::from(card.center().y));
        let rest = frame_fresh(cx, window);
        let p = super::pixel_at(&rest, dims, fill_at.0, fill_at.1);
        assert!(super::near(p, colors.panel), "{n}: the panel's fill: {p:?}");
        if let Ok(mut shown) = crate::sidebar_toggle::SHOWN_TIP.lock() {
            *shown = None;
        }
        cx.simulate_mouse_move(window, card.center(), None, Modifiers::none());
        cx.advance_clock(Duration::from_secs(1));
        settle(cx, window, tab);
        let hovered = frame_fresh(cx, window);
        let tip = crate::sidebar_toggle::SHOWN_TIP
            .lock()
            .ok()
            .and_then(|t| t.clone());
        assert_eq!(
            tip.as_deref(),
            Some(crate::agent_tab::STRIP_TIP),
            "{n}: its tooltip"
        );
        let p = super::pixel_at(&hovered, dims, fill_at.0, fill_at.1);
        assert!(
            super::near(p, colors.panel_hover),
            "{n}: lit under the pointer: {p:?}"
        );
        // A click: the chat glides back to your message, at the top; the strip hides and the
        // chat stops following its end.
        cx.simulate_click(window, card.center(), Modifiers::none());
        settle(cx, window, tab);
        let row = cx
            .read(|cx| tab.read(cx).shot_latest_row())
            .expect("its row");
        assert_eq!(
            list.logical_scroll_top().item_ix,
            row,
            "{n}: your message at the top"
        );
        assert_eq!(strip(cx), None, "{n}: back on screen: no strip");
        assert!(!list.is_following_tail(), "{n}: following the end stopped");
        // Scrolling back to the end: following again, and the strip back.
        let chat = list.viewport_bounds();
        let at = point(chat.center().x, chat.bottom() - px(40.));
        cx.simulate_mouse_move(window, at, None, Modifiers::none());
        cx.simulate_event(
            window,
            ScrollWheelEvent {
                position: at,
                delta: ScrollDelta::Pixels(point(px(0.0), px(-100_000.0))),
                modifiers: Modifiers::none(),
                touch_phase: TouchPhase::Moved,
            },
        );
        settle(cx, window, tab);
        assert!(list.is_following_tail(), "{n}: at the end it follows again");
        assert_eq!(
            strip(cx).as_deref(),
            Some(FIRST_LINE),
            "{n}: scrolled down: shown again"
        );
        // A lane's view: the task Main gave it, the same way.
        let mut events = vec![
            Event::ToolStarted {
                id: "lint".into(),
                name: "Agent".into(),
                summary: "Lint the code".into(),
                parent: None,
                detail: None,
                writes: Vec::new(),
                texts: Vec::new(),
            },
            Event::SubagentStarted {
                id: "lint".into(),
                parent: None,
                label: "Lint the code".into(),
                kind: "general-purpose".into(),
                task: "Lint the code in fuxx-cli/src,\n then report".into(),
                model: None,
                controls: crate::providers::LaneControls { stop: true },
            },
        ];
        events.extend((0..14).map(|i| Event::MessageDone {
            message: format!("msg_lint_{i}"),
            text: format!("Lint pass {i}: no warnings in this file.\n\nOn to the next one."),
            parent: Some("lint".into()),
        }));
        events.push(Event::SubagentEnded {
            id: "lint".into(),
            end: crate::providers::LaneEnd::Done,
        });
        events.push(Event::ToolFinished {
            id: "lint".into(),
            ok: true,
            summary: "done".into(),
            output: None,
        });
        feed(cx, tab, |t, cx| t.shot_feed(events, cx));
        let lane = crate::agent_lanes::View::Lane("lint".into());
        feed(cx, tab, |t, cx| t.show_view(lane.clone(), cx));
        settle(cx, window, tab);
        assert_eq!(
            strip(cx).as_deref(),
            Some("Lint the code in fuxx-cli/src, then report"),
            "{n}: a lane's view: its task"
        );
        let card = cx
            .read(|cx| tab.read(cx).shot_part("latest-strip"))
            .expect("the lane's strip");
        cx.simulate_click(window, card.center(), Modifiers::none());
        settle(cx, window, tab);
        assert_eq!(strip(cx), None, "{n}: the lane's task back on screen");
        feed(cx, tab, |t, cx| {
            t.show_view(crate::agent_lanes::View::Main, cx)
        });
        settle(cx, window, tab);
        // A second message: on screen at once, then scrolled off: its line replaces the first.
        feed(cx, tab, |t, cx| t.send(SECOND.into(), cx));
        settle(cx, window, tab);
        assert_eq!(strip(cx), None, "{n}: the new message on screen: no strip");
        feed(cx, tab, |t, cx| t.shot_feed(turn("b", 14), cx));
        settle(cx, window, tab);
        assert_eq!(
            strip(cx).as_deref(),
            Some(SECOND),
            "{n}: the second message's line"
        );
        cx.simulate_mouse_move(window, at, None, Modifiers::none());
        settle(cx, window, tab);
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

/// Selecting chat text (chore after M11ar): a double-click takes a word, ⌘A with an empty box
/// the whole chat, a press on no text clears, and a drag from the reply's heading into its
/// paragraph selects across pieces; ⌘C (the box has nothing selected) copies each. The picture
/// keeps the drag's selection.
pub(super) fn agent_select() {
    use gpui::{Modifiers, MouseButton, MouseDownEvent, point};
    const REPLY: &str = "## What changed\n\nThe test expects past dates to be rejected; \
`isPastDate` compared **strings**.\n\n- Parse dates before comparing\n\n\
```ts\nconst d = parseDate(date)\n```\n";
    scene("agent-select", |n, cx, window, tab| {
        feed(cx, tab, |t, cx| t.send("Explain the fix".into(), cx));
        feed(cx, tab, |t, cx| {
            t.shot_feed(
                vec![
                    Event::MessageDone {
                        message: "msg_sel".into(),
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
        let _ = frame_fresh(cx, window);
        let boxes = cx.read(|cx| tab.read(cx).shot_text_boxes());
        let find = |start: &str| {
            boxes
                .iter()
                .find(|(t, _)| t.starts_with(start))
                .map(|(_, b)| *b)
                .unwrap_or_else(|| panic!("{n}: no painted text starting {start:?} in {boxes:?}"))
        };
        let heading = find("What changed");
        let para = find("The test expects");
        find("Explain the fix");
        find("const d");
        let copied = |cx: &mut VisualTestAppContext| {
            cx.simulate_keystrokes(window, "cmd-c");
            cx.read_from_clipboard()
                .and_then(|i| i.text())
                .unwrap_or_default()
        };
        // A double-click on "changed" takes the word.
        let on_changed = point(
            heading.left() + heading.size.width * 0.8,
            heading.center().y,
        );
        cx.simulate_event(
            window,
            MouseDownEvent {
                position: on_changed,
                modifiers: Modifiers::none(),
                button: MouseButton::Left,
                click_count: 2,
                first_mouse: false,
            },
        );
        cx.simulate_mouse_up(window, on_changed, MouseButton::Left, Modifiers::none());
        let _ = frame_fresh(cx, window);
        assert_eq!(copied(cx), "changed", "{n}: a double-click takes the word");
        // ⌘A with nothing typed: the whole chat, your message to the reply's code.
        cx.simulate_keystrokes(window, "cmd-a");
        let _ = frame_fresh(cx, window);
        let all = copied(cx);
        assert!(
            all.starts_with("Explain the fix\n\nWhat changed\n")
                && all.ends_with("const d = parseDate(date)"),
            "{n}: ⌘A selects the whole chat: {all:?}"
        );
        // A press on no text (left of your bubble) clears: ⌘C leaves the clipboard as it was.
        cx.write_to_clipboard(gpui::ClipboardItem::new_string("untouched".into()));
        let empty = point(heading.left() + px(4.), heading.top() - px(40.));
        cx.simulate_click(window, empty, Modifiers::none());
        let _ = frame_fresh(cx, window);
        assert_eq!(
            copied(cx),
            "untouched",
            "{n}: a press on no text clears the selection"
        );
        // A drag from the heading's start into the paragraph selects across the two.
        let from = point(heading.left() + px(1.), heading.center().y);
        let to = point(para.left() + para.size.width * 0.3, para.top() + px(6.));
        cx.simulate_mouse_down(window, from, MouseButton::Left, Modifiers::none());
        cx.simulate_mouse_move(window, to, MouseButton::Left, Modifiers::none());
        cx.simulate_mouse_up(window, to, MouseButton::Left, Modifiers::none());
        let _ = frame_fresh(cx, window);
        let dragged = copied(cx);
        assert!(
            dragged.starts_with("What changed\nThe test") && !dragged.contains("Explain"),
            "{n}: the drag selects from the heading into the paragraph: {dragged:?}"
        );
        // Typing in the box still works, and its own selection wins ⌘C again.
        cx.simulate_input(window, "next");
        cx.simulate_keystrokes(window, "cmd-a");
        assert_eq!(
            copied(cx),
            "next",
            "{n}: the box's own selection is what ⌘C copies"
        );
        cx.simulate_keystrokes(window, "cmd-backspace");
        let _ = frame_fresh(cx, window);
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
                    by: None,
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
        cx.simulate_input(window, &line.repeat(6));
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
            t.shot_history(vec![Past::you("earlier")], cx)
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
    // Minor 4, since the redesign: a tool row's click opens the modal; a click in it keeps it, the
    // scrim closes it.
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
        let _ = frame_fresh(cx, window);
        // The row starts under the top bar (32) and its 14 pt gap: a click on it opens the
        // tool's detail in the modal (spec A §B), not inline.
        cx.simulate_click(window, point(px(300.), px(60.)), Modifiers::none());
        cx.run_until_parked();
        assert!(
            cx.read(|cx| tab.read(cx).shot_open("e1")),
            "{n}: the row's click opens the modal"
        );
        let _ = frame_fresh(cx, window);
        // In the panel (its header at 48–92, the diff below): it stays open.
        cx.simulate_click(window, point(px(DIMS.0 / 2.), px(130.)), Modifiers::none());
        cx.run_until_parked();
        assert!(
            cx.read(|cx| tab.read(cx).shot_open("e1")),
            "{n}: a click in the modal keeps it open"
        );
        // On the scrim, beside the panel: closed.
        cx.simulate_click(window, point(px(8.), px(DIMS.1 - 40.)), Modifiers::none());
        cx.run_until_parked();
        assert!(
            !cx.read(|cx| tab.read(cx).shot_open("e1")),
            "{n}: the scrim closes it"
        );
        keep.borrow_mut().take();
    });

    println!("ui-shots: {n}: boot send, force stop, failed resume, late send, spawn failure");
}

/// M11aa Task 8: a Workspace the agent-workspace scenes draw (the sidebar's presets, no hint
/// card), built by `build`, kept in `slot`.
pub(super) fn agent_workspace(
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
                    // The status bar follows the focused agent tab (your report, 2026-11-22:
                    // "it doesn't follow the active tab status"): its status and its folder.
                    let agent = ws.agents[&tab].status;
                    assert_ne!(agent, crate::status::Status::Idle, "{n}: it has news");
                    let facts = ws.status_bar_facts().expect("the bar says something");
                    assert_eq!(facts.status, agent, "{n}: the bar says the agent's status");
                    assert_eq!(
                        facts.folder.as_deref(),
                        Some(std::path::Path::new("/Users/me/code/my-app")),
                        "{n}: and its folder"
                    );
                    let keys = view.read(cx).focus_handle(cx).is_focused(window);
                    assert!(keys, "{n}: the new agent tab has the keyboard");
                })
                .unwrap();
            },
        );
        println!("ui-shots: {n}: {}", path.display());
    }
}

/// The box's sweep (2026-11-22, the lead designer's spec): it runs once when the agent tab is
/// shown — first shown, and back from another workspace — and never on a redraw or when focus
/// moves between panes already on screen.
pub(super) fn agent_box_sweep() {
    let n = "agent-box-sweep";
    let (slug, theme_name) = shot_themes()[0];
    let theme = shot_theme(theme_name);
    let slot: Rc<RefCell<Option<Entity<crate::workspace::Workspace>>>> = Rc::default();
    let build = agent_workspace(theme, slot.clone(), |ws, window, cx| {
        cx.update_global::<crate::appearance::Appearance, _>(|a, _| a.agent_sweep = true);
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
            let (_, view) = cx
                .read(|cx| ws.read(cx).focused_agent_tab())
                .expect("the agent tab");
            let sweeps = |cx: &mut VisualTestAppContext| cx.read(|cx| view.read(cx).shot_sweeps());
            assert_eq!(sweeps(cx), 1, "{n}: shown: one sweep");
            // A redraw shows nothing new.
            cx.update(|cx| ws.update(cx, |_, cx| cx.notify()));
            settle_frames(cx, window);
            assert_eq!(sweeps(cx), 1, "{n}: a redraw: none");
            // Focus to the shell beside it: both still on screen.
            let shell_pane = cx.read(|cx| {
                let ws = ws.read(cx);
                let w = ws.model.focused_workspace().expect("a workspace");
                ws.model.workspace(w).expect("it").panes[0].id
            });
            cx.update(|cx| {
                ws.update(cx, |ws, cx| {
                    ws.model.focus_pane(shell_pane);
                    cx.notify();
                })
            });
            settle_frames(cx, window);
            assert_eq!(sweeps(cx), 1, "{n}: focus between panes on screen: none");
            // Another workspace, then back: shown again.
            let here = cx.read(|cx| ws.read(cx).model.focused_workspace().expect("a workspace"));
            cx.update_window(window, |_, window, cx| {
                ws.update(cx, |ws, cx| {
                    let api = std::path::Path::new("/Users/me/code/api");
                    ws.open_shot_session("api", api, super::SHELL_TEXT, None, window, cx);
                })
            })
            .unwrap();
            settle_frames(cx, window);
            assert_eq!(sweeps(cx), 1, "{n}: hidden: none");
            cx.update(|cx| {
                ws.update(cx, |ws, cx| {
                    ws.model.focus_workspace(here);
                    cx.notify();
                })
            });
            settle_frames(cx, window);
            assert_eq!(sweeps(cx), 2, "{n}: back: one more");
            // The box redrawing (its caret breathing, the keyboard back in it) doesn't end it;
            // typing does (your report, 2026-11-22: "when I go back, it doesnt animate").
            let running =
                |cx: &mut VisualTestAppContext| cx.read(|cx| view.read(cx).sweep_at.is_some());
            let composer = cx.read(|cx| view.read(cx).composer.clone());
            cx.update(|cx| composer.update(cx, |_, cx| cx.notify()));
            settle_frames(cx, window);
            assert!(running(cx), "{n}: a redraw of the box keeps it");
            cx.update(|cx| composer.update(cx, |c, cx| c.set_text("hi".into(), cx)));
            settle_frames(cx, window);
            assert!(!running(cx), "{n}: typing ends it");
            // The shot: part way round (wall clock; to look at, not checked).
            cx.update(|cx| view.update(cx, |t, cx| t.shown(cx)));
            let _ = frame_fresh(cx, window);
            std::thread::sleep(std::time::Duration::from_millis(330));
            let _ = frame_fresh(cx, window);
        },
    );
    println!("ui-shots: {n}: {}", path.display());
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
                let past = vec![Past::you("earlier")];
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
                                by: None,
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

// (`redraw=0`: a scripted shell, it never redraws its prompt, so a resize must not empty it.)
const MARK_A: &str = "\x1b]133;A;redraw=0\x07";
const MARK_B: &str = "\x1b]133;B\x07";

/// `npm test` ends with exit 1, and the next prompt shows.
const ATTACHMENT_END: &str = "\x1b]133;D;1\x07\x1b]133;A;redraw=0\x07$ \x1b]133;B\x07";

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
                    Item::You {
                        text, attachments, ..
                    } => Some((text.clone(), attachments.clone())),
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
                    Item::You {
                        text, attachments, ..
                    } => Some((text.clone(), attachments.clone())),
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

/// Checks only (M11ag Task 7): the agent's reason for an edit is caught from live events only. A
/// tab fed a text and an `Edit` (then its finish) tells one `EditNoted` carrying that text and the
/// call's new text; a tab given the same as history (a restored conversation) tells none.
pub(super) fn agent_edit_reason_live_only() {
    use crate::agent_tab::AgentTabEvent;
    use crate::edit_notes::EditNote;
    use crate::providers::EditText;
    let n = "agent-edit-reason-live-only";
    let theme = shot_theme("Catppuccin Mocha");
    let heard: Rc<RefCell<Vec<EditNote>>> = Rc::default();
    let (live_slot, past_slot): (Slot, Slot) = (Rc::default(), Rc::default());
    let events = || {
        vec![
            Event::MessageDone {
                message: "m1".into(),
                text: "reuse an open tab".into(),
                parent: None,
            },
            Event::ToolStarted {
                id: "t1".into(),
                name: "Edit".into(),
                summary: "src/a.rs".into(),
                parent: None,
                detail: None,
                writes: vec!["src/a.rs".into()],
                texts: vec![EditText::Inserted("let x = 1;".into())],
            },
            Event::ToolFinished {
                id: "t1".into(),
                ok: true,
                summary: String::new(),
                output: None,
            },
        ]
    };
    let (make_heard, keep_live, keep_past) = (heard.clone(), live_slot.clone(), past_slot.clone());
    render_with(
        &theme,
        size(px(DIMS.0), px(DIMS.1)),
        move |window, cx| {
            cx.set_global(shot_appearance(theme, cx));
            let mut tabs = Vec::new();
            for keep in [&keep_live, &keep_past] {
                let tab = cx.new(|cx| AgentTab::for_shots(&CLAUDE, start(), window, cx));
                let heard = make_heard.clone();
                cx.subscribe(&tab, move |_, e: &AgentTabEvent, _| {
                    if let AgentTabEvent::EditNoted { note, .. } = e {
                        heard.borrow_mut().push(note.clone());
                    }
                })
                .detach();
                *keep.borrow_mut() = Some(tab.clone());
                tabs.push(tab);
            }
            tabs.remove(0).into()
        },
        move |cx, _| {
            let live = tab_of(&live_slot);
            let past = tab_of(&past_slot);
            feed(cx, &past, |t, cx| {
                t.shot_history(events().into_iter().map(Past::Event).collect(), cx)
            });
            assert!(heard.borrow().is_empty(), "{n}: history gives no note");
            feed(cx, &live, |t, cx| t.shot_history(Vec::new(), cx));
            feed(cx, &live, |t, cx| t.shot_feed(events(), cx));
            let notes = heard.borrow();
            assert_eq!(notes.len(), 1, "{n}: the live edit gives one note");
            assert_eq!(&*notes[0].reason, "reuse an open tab", "{n}");
            assert_eq!(
                notes[0].texts,
                vec![EditText::Inserted("let x = 1;".into())],
                "{n}"
            );
            live_slot.borrow_mut().take();
            past_slot.borrow_mut().take();
        },
    );
    println!("ui-shots: {n}: live edit noted once, history not");
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

    // Your report (2026-11-22: "when I `/clear` claude first time it happens like this, the
    // second `/clear` really clears it"): a resumed run whose first message is `/clear` — the
    // agent resets, then starts under a new id. That is the clear, not a failed resume.
    run_scene(
        "agent-tab-runs-resumed-clear",
        resumed.clone(),
        |n, cx, tab, _| {
            feed(cx, tab, |t, cx| t.send("/clear".into(), cx));
            feed(cx, tab, |t, cx| {
                t.shot_feed(vec![Event::Cleared, started(CLEARED)], cx)
            });
            assert_eq!(
                notices(cx, tab, BAD_RESUME_NOTICE),
                0,
                "{n}: cleared, not a failed resume"
            );
            assert_eq!(
                cx.read(|cx| tab.read(cx).resume_id()).as_deref(),
                Some(CLEARED),
                "{n}: follows the cleared conversation"
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
