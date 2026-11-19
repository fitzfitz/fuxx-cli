//! M11ag Part 1: Ask the agent from the code. These scenes drive `Workspace::ask_code` directly
//! (the selection bar is a later task's) and read state: which agent tab holds the chip, which
//! tab is shown and has the keyboard, and that nothing was written to an agent.

use std::path::PathBuf;

use gpui::{Entity, Focusable};

use super::chrome::settle_frames;
use super::*;
use crate::agent_tab::Attachment;
use crate::code_ask::{AskTo, CodeLines, CodeRef};
use crate::layout_model::{FileMode, TabId, TabKind};

/// This file, as `file!()` gives it (repo-relative under cargo): `registry::AREAS` pairs it with
/// `SCENES`, so `--changed` runs these scenes when this file changes.
pub(super) const FILE: &str = file!();

/// The files a scene here is made from.
const SOURCES: &[&str] = &[
    "src/workspace.rs",
    "src/workspace_agent.rs",
    "src/file_tab.rs",
    "src/agent_tab.rs",
    "src/code_ask.rs",
];

/// This area's scenes (`registry.rs` runs them in its `ORDER`).
pub(super) const SCENES: &[Scene] = &[
    Scene {
        name: "ask-existing-tab",
        run: ask_existing_tab,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: SOURCES,
    },
    Scene {
        name: "ask-new-tab",
        run: ask_new_tab,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: SOURCES,
    },
];

/// A made-up `src/workspace.rs` under a scratch folder named `fuxx-cli`.
fn scratch_doc() -> (PathBuf, PathBuf) {
    let root = std::env::temp_dir()
        .join("fuxx-ui-shots-ask")
        .join("fuxx-cli");
    let dir = root.join("src");
    std::fs::create_dir_all(&dir).expect("the scratch folder");
    let root = std::fs::canonicalize(&root).unwrap_or(root);
    let path = root.join("src").join("workspace.rs");
    let text: String = (1..=60)
        .map(|n| format!("    let line_{n} = {n};\n"))
        .collect();
    std::fs::write(&path, format!("fn main() {{\n{text}}}\n")).expect("the scratch doc");
    (root, path)
}

/// A reference to `first..=last` of `path`.
fn code(path: &std::path::Path, first: usize, last: usize) -> CodeRef {
    CodeRef {
        path: path.to_path_buf(),
        real: None,
        lines: CodeLines::Lines { first, last },
        fence: Some("rust".into()),
        body: (first..=last)
            .map(|n| format!("    let line_{n} = {n};"))
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

/// Build a Workspace with `setup`, then run `act` on it after the first frames.
fn drive(
    setup: impl FnOnce(&mut Workspace, &mut Window, &mut Context<Workspace>) + 'static,
    act: impl FnOnce(&mut VisualTestAppContext, AnyWindowHandle, Entity<Workspace>),
) {
    let (_, name) = shot_themes()[0];
    let theme = shot_theme(name);
    let slot: Slot = Default::default();
    let keep = slot.clone();
    let build = move |window: &mut Window, cx: &mut App| -> AnyView {
        cx.set_global(shot_appearance(theme, cx));
        let ws = cx.new(|cx| {
            let mut ws = Workspace::for_shots(window, cx);
            ws.sidebar.hint_dismissed = true;
            setup(&mut ws, window, cx);
            ws
        });
        *keep.borrow_mut() = Some(ws.clone());
        ws.into()
    };
    let run = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
        let ws = slotted(&slot);
        settle_frames(cx, window);
        act(cx, window, ws);
    };
    render_with(&theme, size(px(1100.0), px(600.0)), build, run);
}

/// Ask from file tab `file` with `code`, as the bar's button will.
fn ask(
    cx: &mut VisualTestAppContext,
    window: AnyWindowHandle,
    ws: &Entity<Workspace>,
    file: TabId,
    code: CodeRef,
) {
    cx.update_window(window, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.ask_code(file, code, window, cx))
    })
    .expect("ask");
    settle_frames(cx, window);
}

/// The agent tab `tab`'s chips' labels and the lines written to its agent.
fn chips(cx: &mut VisualTestAppContext, ws: &Entity<Workspace>, tab: TabId) -> Vec<String> {
    cx.read(|cx| {
        let view = ws.read(cx).agents[&tab].view.clone();
        view.read(cx)
            .shot_attachments()
            .iter()
            .map(Attachment::label)
            .collect()
    })
}

fn written(cx: &mut VisualTestAppContext, ws: &Entity<Workspace>, tab: TabId) -> usize {
    cx.read(|cx| ws.read(cx).agents[&tab].view.read(cx).shot_written().len())
}

fn has_keys(
    cx: &mut VisualTestAppContext,
    window: AnyWindowHandle,
    ws: &Entity<Workspace>,
    tab: TabId,
) -> bool {
    cx.update_window(window, |_, window, cx| {
        let view = ws.read(cx).agents[&tab].view.clone();
        view.read(cx).focus_handle(cx).is_focused(window)
    })
    .expect("window")
}

/// The file tab of workspace `ws`, and the agent tabs by provider.
fn tabs_of(cx: &mut VisualTestAppContext, ws: &Entity<Workspace>) -> (TabId, Vec<(String, TabId)>) {
    cx.read(|cx| {
        let ws = ws.read(cx);
        let id = ws.model.focused_workspace().expect("a workspace");
        let all = ws.model.workspace(id).expect("it");
        let mut file = None;
        let mut agents = Vec::new();
        for tab in all.panes.iter().flat_map(|p| &p.tabs) {
            match &tab.kind {
                TabKind::File { .. } => file = Some(tab.id),
                TabKind::Agent { provider, .. } => agents.push((provider.clone(), tab.id)),
                _ => {}
            }
        }
        (file.expect("a file tab"), agents)
    })
}

/// A workspace with a file tab and two agent tabs (claude, agy), the agy tab focused last: Ask
/// puts the chip in the agy tab, shows it (in the file tab's pane, or beside it) with the keyboard
/// in its box, and writes nothing. The file tab's label reads agy.
fn ask_existing_tab() {
    let (root, doc) = scratch_doc();
    for beside in [false, true] {
        let n = format!("ask-existing-tab (beside: {beside})");
        let (project, path) = (root.clone(), doc.clone());
        let setup = move |ws: &mut Workspace, window: &mut Window, cx: &mut Context<Workspace>| {
            ws.open_shot_session("shell", &project, "", None, window, cx);
            let pane = ws.model.focused_pane().expect("a pane");
            ws.new_agent_tab(pane, "claude", window, cx);
            ws.new_agent_tab(pane, "agy", window, cx);
            if beside {
                // The file tab in a pane of its own, to the right.
                let kind = TabKind::File {
                    path: path.clone(),
                    mode: FileMode::Plain,
                };
                ws.model.split(pane, crate::split_tree::Axis::Row, kind);
                ws.sync();
            } else {
                ws.open_file_tab(path.clone(), FileMode::Plain, window, cx);
            }
        };
        let doc = doc.clone();
        let n_in = n.clone();
        drive(setup, move |cx, window, ws| {
            let n = n_in;
            let (file, agents) = tabs_of(cx, &ws);
            let find = |p: &str| agents.iter().find(|(n, _)| n == p).expect("tab").1;
            let (claude, agy) = (find("claude"), find("agy"));
            // The agy tab used last, then the file tab.
            for tab in [claude, agy, file] {
                cx.update_window(window, |_, window, cx| {
                    ws.update(cx, |ws, cx| ws.select_tab(tab, window, cx))
                })
                .expect("select");
                settle_frames(cx, window);
            }
            let probe = |cx: &mut VisualTestAppContext| {
                cx.read(|cx| ws.read(cx).file_tabs[&file].read(cx).ask_to())
            };
            assert_eq!(probe(cx), AskTo::Agent("agy"), "{n}: the bar names agy");
            ask(cx, window, &ws, file, code(&doc, 3, 5));
            assert_eq!(chips(cx, &ws, agy), ["workspace.rs:3\u{2013}5"], "{n}");
            assert!(chips(cx, &ws, claude).is_empty(), "{n}: claude got nothing");
            assert_eq!(written(cx, &ws, agy), 0, "{n}: nothing was sent");
            let (focused, active) = cx.read(|cx| {
                let ws = ws.read(cx);
                let (_, pane) = ws.model.locate(agy).expect("placed");
                (
                    ws.model.focused_tab(),
                    ws.model.pane(pane).map(|p| p.active),
                )
            });
            assert_eq!(focused, Some(agy), "{n}: the agy tab is the focused one");
            assert_eq!(active, Some(agy), "{n}: and its pane's shown tab");
            assert!(has_keys(cx, window, &ws, agy), "{n}: its box has the keys");
            assert_eq!(probe(cx), AskTo::Agent("agy"), "{n}: the bar still agy");
            // The same lines again: still one chip.
            ask(cx, window, &ws, file, code(&doc, 3, 5));
            assert_eq!(
                chips(cx, &ws, agy).len(),
                1,
                "{n}: the same lines, one chip"
            );
        });
        println!("ui-shots: {n} ok");
    }
}

/// With no agent tab, Ask opens one in the file tab's pane (agy when an agy tab was used last in
/// another workspace, else claude); a second Ask before its box exists fills the same tab; closing
/// it before its box exists drops the waiting chips.
fn ask_new_tab() {
    let (root, doc) = scratch_doc();
    // 0: two Asks of different lines; 1: the same lines twice; 2: the tab closed while waiting;
    // 3: no agent tab used anywhere, so claude.
    for case in 0..4 {
        let n = format!("ask-new-tab (case {case})");
        let (project, path) = (root.clone(), doc.clone());
        let setup = move |ws: &mut Workspace, window: &mut Window, cx: &mut Context<Workspace>| {
            if case < 3 {
                // An agy tab used in another workspace: the app-wide last provider.
                ws.open_shot_session("other", &project, "", None, window, cx);
                let pane = ws.model.focused_pane().expect("a pane");
                ws.new_agent_tab(pane, "agy", window, cx);
                ws.ensure_agent_tabs(window, cx);
            }
            let at = project.join("src");
            ws.open_shot_session("work", &at, "", None, window, cx);
            ws.open_file_tab(path.clone(), FileMode::Plain, window, cx);
        };
        let doc = doc.clone();
        let n_in = n.clone();
        drive(setup, move |cx, window, ws| {
            let n = n_in;
            let (file, before) = tabs_of(cx, &ws);
            assert!(before.is_empty(), "{n}: no agent tab in this workspace");
            let want = if case < 3 { "agy" } else { "claude" };
            let file_pane = cx.read(|cx| ws.read(cx).model.locate(file).expect("placed").1);
            // The first Ask, then (cases 0 and 1) a second, or (case 2) the new tab's close, all
            // before the frame that makes the box: a window update may draw at its end.
            let second = if case == 1 { (3, 5) } else { (9, 10) };
            let seen = cx
                .update_window(window, |_, window, cx| {
                    ws.update(cx, |ws, cx| {
                        ws.ask_code(file, code(&doc, 3, 5), window, cx);
                        let all = ws
                            .model
                            .agent_tabs(ws.model.focused_workspace().expect("ws"));
                        assert_eq!(all.len(), 1, "{n}: one new tab");
                        let tab = all[0];
                        let provider = ws.agents.get(&tab).map(|_| "made");
                        let first = (
                            tab,
                            ws.model.locate(tab).map(|(_, p)| p),
                            ws.pending_chips.get(&tab).map_or(0, Vec::len),
                            provider.is_none(),
                        );
                        if case == 2 {
                            ws.close_tab(tab, window, cx);
                            return (first, 0, ws.pending_chips.len());
                        }
                        ws.ask_code(file, code(&doc, second.0, second.1), window, cx);
                        let again = ws
                            .model
                            .agent_tabs(ws.model.focused_workspace().expect("ws"));
                        assert_eq!(again, [tab], "{n}: still one new tab");
                        (first, ws.pending_chips[&tab].len(), ws.pending_chips.len())
                    })
                })
                .expect("ask");
            let ((tab, in_pane, waiting, slotless), after_second, kept) = seen;
            assert_eq!(in_pane, Some(file_pane), "{n}: in the file tab's pane");
            assert!(slotless && waiting == 1, "{n}: its chip waits");
            if case == 2 {
                // Closed before its box exists: the waiting chip goes, nothing panics.
                assert_eq!(kept, 0, "{n}: no chip kept");
                settle_frames(cx, window);
                let (_, none) = tabs_of(cx, &ws);
                assert!(none.is_empty(), "{n}: the tab is gone");
                return;
            }
            assert_eq!(after_second, if case == 1 { 1 } else { 2 }, "{n}: waiting");
            let (provider, _) =
                cx.read(|cx| match &ws.read(cx).model.tab(tab).expect("tab").kind {
                    TabKind::Agent { provider, .. } => (provider.clone(), ()),
                    _ => unreachable!(),
                });
            assert_eq!(provider, want, "{n}: the new tab's agent");
            // The frame that makes its box: the chips are in it, and it has the keys.
            settle_frames(cx, window);
            let labels = chips(cx, &ws, tab);
            let expect: &[&str] = if case == 1 {
                &["workspace.rs:3\u{2013}5"]
            } else {
                &["workspace.rs:3\u{2013}5", "workspace.rs:9\u{2013}10"]
            };
            assert_eq!(labels, expect, "{n}: the chips");
            assert!(cx.read(|cx| ws.read(cx).pending_chips.is_empty()), "{n}");
            assert_eq!(written(cx, &ws, tab), 0, "{n}: nothing was sent");
            assert!(has_keys(cx, window, &ws, tab), "{n}: its box has the keys");
        });
        println!("ui-shots: {n} ok");
    }
}
