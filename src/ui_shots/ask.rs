//! M11ag Part 1: Ask the agent from the code. The first scenes drive `Workspace::ask_code`
//! directly; the `ask-bar-*` ones work the selection bar and ⌘↵ the way a hand does (a drag, a
//! click, a key) and read state and pixels: which agent tab holds the chip, which tab is shown and
//! has the keyboard, that nothing was written to an agent, and where the bar was drawn.

use std::path::PathBuf;

use gpui::{Bounds, Entity, Focusable, Modifiers, MouseButton, Point};

use super::chrome::settle_frames;
use super::*;
use crate::agent_tab::Attachment;
use crate::code_ask::{AskTo, CodeLines, CodeRef};
use crate::file_tab::FileTab;
use crate::layout_model::{FileMode, TabId, TabKind};
use crate::ui_tokens::{TEXT_MIN, contrast};

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
    Scene {
        name: "ask-bar",
        run: ask_bar,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: SOURCES,
    },
    Scene {
        name: "ask-bar-above",
        run: ask_bar_above,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: SOURCES,
    },
    Scene {
        name: "ask-bar-hides",
        run: ask_bar_hides,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: SOURCES,
    },
    Scene {
        name: "ask-bar-agy",
        run: ask_bar_agy,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: SOURCES,
    },
    Scene {
        name: "ask-bar-ssh",
        run: ask_bar_ssh,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: SOURCES,
    },
    Scene {
        name: "ask-bar-too-long",
        run: ask_bar_too_long,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: SOURCES,
    },
];

/// A made-up `src/workspace.rs` under a scratch folder named `fuxx-cli`.
fn scratch_doc() -> (PathBuf, PathBuf) {
    scratch_file("workspace.rs", 60)
}

/// A made-up `src/<name>` of `lines` lines (`fn main() {`, `lines` numbered lines, `}`) under the
/// scratch folder named `fuxx-cli`.
fn scratch_file(name: &str, lines: usize) -> (PathBuf, PathBuf) {
    let root = std::env::temp_dir()
        .join("fuxx-ui-shots-ask")
        .join("fuxx-cli");
    let dir = root.join("src");
    std::fs::create_dir_all(&dir).expect("the scratch folder");
    let root = std::fs::canonicalize(&root).unwrap_or(root);
    let path = root.join("src").join(name);
    let text: String = (1..=lines)
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
    drive_in(shot_themes()[0].1, setup, act);
}

/// `drive` in the theme called `theme_name`, with the real key bindings.
fn drive_in(
    theme_name: &str,
    setup: impl FnOnce(&mut Workspace, &mut Window, &mut Context<Workspace>) + 'static,
    act: impl FnOnce(&mut VisualTestAppContext, AnyWindowHandle, Entity<Workspace>),
) {
    let theme = shot_theme(theme_name);
    let slot: Slot = Default::default();
    let keep = slot.clone();
    let build = move |window: &mut Window, cx: &mut App| -> AnyView {
        cx.set_global(shot_appearance(theme, cx));
        cx.bind_keys(crate::key_bindings());
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

// ---- the selection bar ------------------------------------------------------------------------

/// The window, as the scenes' `drive` makes it.
fn dims() -> Size<Pixels> {
    size(px(1100.0), px(600.0))
}

/// Workspace setup: a shell in `root`, an agent tab for each of `agents` (in order), the file
/// `path` open in a tab.
fn bar_setup(
    root: PathBuf,
    path: PathBuf,
    agents: &'static [&'static str],
) -> impl FnOnce(&mut Workspace, &mut Window, &mut Context<Workspace>) + 'static {
    move |ws, window, cx| {
        ws.open_shot_session("shell", &root, "", None, window, cx);
        let pane = ws.model.focused_pane().expect("a pane");
        for agent in agents {
            ws.new_agent_tab(pane, agent, window, cx);
        }
        ws.open_file_tab(path.clone(), FileMode::Plain, window, cx);
    }
}

fn file_view(
    cx: &mut VisualTestAppContext,
    ws: &Entity<Workspace>,
    file: TabId,
) -> Entity<FileTab> {
    cx.read(|cx| ws.read(cx).file_tabs[&file].clone())
}

fn geom_of(cx: &mut VisualTestAppContext, tab: &Entity<FileTab>) -> crate::file_tab::Geom {
    cx.read(|cx| tab.read(cx).geom())
        .expect("the text was drawn")
}

/// The window point at line `line`'s middle row height, `col` cells into its text (0-based).
fn text_point(g: &crate::file_tab::Geom, line: usize, col: usize) -> Point<Pixels> {
    point(
        g.bounds.left() + g.text_left + g.cell_width * col as f32,
        g.bounds.top() + g.line_height * (line as f32 + 0.5) - g.scroll.y,
    )
}

/// Press at `from` and drag to `to` (each `(line, col)`, 0-based); release there unless `held`.
fn drag(
    cx: &mut VisualTestAppContext,
    window: AnyWindowHandle,
    tab: &Entity<FileTab>,
    from: (usize, usize),
    to: (usize, usize),
    held: bool,
) {
    let g = geom_of(cx, tab);
    let (a, b) = (text_point(&g, from.0, from.1), text_point(&g, to.0, to.1));
    let (left, none) = (MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_down(window, a, left, none);
    cx.simulate_mouse_move(window, b, Some(left), none);
    if !held {
        cx.simulate_mouse_up(window, b, left, none);
    }
    settle_frames(cx, window);
}

fn scroll_to(
    cx: &mut VisualTestAppContext,
    window: AnyWindowHandle,
    tab: &Entity<FileTab>,
    y: f32,
) {
    cx.update(|cx| tab.update(cx, |t, cx| t.set_scroll(point(px(0.0), px(y)), cx)));
    settle_frames(cx, window);
}

/// The bar's and its buttons' bounds as last drawn, and the Ask button's words.
fn bar_of(cx: &mut VisualTestAppContext, tab: &Entity<FileTab>) -> Option<Bounds<Pixels>> {
    cx.read(|cx| tab.read(cx).sel_bar_bounds())
}

fn same(a: Pixels, b: Pixels) -> bool {
    f32::from(a - b).abs() < 0.6
}

fn press(cx: &mut VisualTestAppContext, window: AnyWindowHandle, keys: &str) {
    cx.simulate_keystrokes(window, keys);
    settle_frames(cx, window);
}

fn select_tab(
    cx: &mut VisualTestAppContext,
    window: AnyWindowHandle,
    ws: &Entity<Workspace>,
    tab: TabId,
) {
    cx.update_window(window, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.select_tab(tab, window, cx))
    })
    .expect("select");
    settle_frames(cx, window);
}

fn click(cx: &mut VisualTestAppContext, window: AnyWindowHandle, at: Bounds<Pixels>) {
    cx.simulate_click(window, at.center(), Modifiers::none());
    settle_frames(cx, window);
}

/// The text Ask gives for lines 43-45 of the scratch file.
fn copied_43_45(fence: &str) -> String {
    let body = (42..=44)
        .map(|n| format!("    let line_{n} = {n};"))
        .collect::<Vec<_>>()
        .join("\n");
    format!("src/workspace.rs:43-45\n```{fence}\n{body}\n```")
}

/// A 60-line file in a workspace with one claude agent tab: drag from 43:7 to 45:2 and release ->
/// the bar hangs under row 45, its top-right at the code's right edge and its top the gap under
/// the row's bottom; "Ask claude ⌘↵" and "Copy with path" side by side; no code row moved. Copy
/// with path puts the spec's text (with `src/workspace.rs:43-45`) on the clipboard; ⌘↵ puts the
/// chip in the agent tab and writes nothing; with no selection ⌘↵ does nothing. Both themes: the
/// shot is kept and the Ask button's fill read.
fn ask_bar() {
    let (root, doc) = scratch_doc();
    for &(slug, name) in shot_themes() {
        let n = format!("ask-bar-{slug}");
        let ui = shot_theme(name).ui();
        let setup = bar_setup(root.clone(), doc.clone(), &["claude"]);
        let n_in = n.clone();
        drive_in(name, setup, move |cx, window, ws| {
            let n = n_in;
            let (file, agents) = tabs_of(cx, &ws);
            let claude = agents[0].1;
            let tab = file_view(cx, &ws, file);
            cx.update(|cx| tab.update(cx, |t, cx| t.pretend_rel_for_shots("src/workspace.rs", cx)));
            // The tab's lines 43-45 in sight.
            let g0 = geom_of(cx, &tab);
            scroll_to(cx, window, &tab, f32::from(g0.line_height) * 30.0);
            let g0 = geom_of(cx, &tab);
            assert!(bar_of(cx, &tab).is_none(), "{n}: no bar before a selection");

            // Dragging: no bar yet. Released: the bar.
            drag(cx, window, &tab, (42, 6), (44, 1), true);
            assert!(bar_of(cx, &tab).is_none(), "{n}: no bar while dragging");
            cx.simulate_mouse_up(
                window,
                text_point(&g0, 44, 1),
                MouseButton::Left,
                Modifiers::none(),
            );
            settle_frames(cx, window);
            let selection = cx.read(|cx| tab.read(cx).selection());
            assert_eq!(
                selection.map(|(a, b)| ((a.line, a.col), (b.line, b.col))),
                Some(((42, 6), (44, 1))),
                "{n}: the selection"
            );
            let g = geom_of(cx, &tab);
            let bar = bar_of(cx, &tab).expect("the bar after the release");
            let row_bottom = g.bounds.top() + g.line_height * 45.0 - g.scroll.y;
            assert!(
                same(bar.right(), g.bounds.right()),
                "{n}: right edge {bar:?} in {:?}",
                g.bounds
            );
            assert!(
                same(bar.top(), row_bottom + px(6.0)),
                "{n}: {:?} under row 45's bottom {:?}",
                bar.top(),
                row_bottom
            );
            // The code didn't move for it: the same rows, the same scroll.
            assert_eq!(
                (g.bounds, g.line_height, g.scroll, g.max_scroll),
                (g0.bounds, g0.line_height, g0.scroll, g0.max_scroll),
                "{n}: no code row moved"
            );
            let (words, enabled) = cx.read(|cx| tab.read(cx).ask_button()).expect("Ask");
            assert_eq!(
                (words.as_str(), enabled),
                ("Ask claude \u{2318}\u{21b5}", true),
                "{n}"
            );
            let ask = cx
                .read(|cx| tab.read(cx).ask_button_bounds())
                .expect("Ask's place");
            let copy = cx
                .read(|cx| tab.read(cx).copy_button_bounds())
                .expect("Copy's place");
            assert!(
                ask.right() <= copy.left() && same(ask.top(), copy.top()),
                "{n}: side by side"
            );
            assert!(
                same(ask.size.height, copy.size.height),
                "{n}: as tall as each other"
            );
            assert!(
                bar.contains(&ask.center()) && bar.contains(&copy.center()),
                "{n}: both in the bar"
            );
            assert!(
                f32::from(bar.right() - copy.right()) < 2.0,
                "{n}: Copy ends the bar (inside its 1 pt border): {copy:?} {bar:?}"
            );

            // The look: the shot, and the Ask button's fill (`panel`) read in it.
            let shot = shown(cx, window);
            keep_shot(&n, &shot);
            let p = pixel_at(
                &shot,
                dims(),
                f32::from(ask.left()) + 4.0,
                f32::from(ask.center().y),
            );
            assert!(near(p, ui.panel), "{n}: the button's fill {p:?}, not panel");

            // Copy with path.
            click(cx, window, copy);
            let clip = cx
                .read_from_clipboard()
                .and_then(|c| c.text())
                .expect("a clipboard text");
            assert_eq!(clip, copied_43_45("rust"), "{n}: the copied text");
            assert!(
                chips(cx, &ws, claude).is_empty(),
                "{n}: copying asks nobody"
            );

            // ⌘↵ with the selection (the bar's key): the chip, nothing written.
            select_tab(cx, window, &ws, file);
            assert!(
                bar_of(cx, &tab).is_some(),
                "{n}: the bar again with the tab shown"
            );
            click_text(cx, window, &tab);
            // (a plain press cleared the selection: select again)
            drag(cx, window, &tab, (42, 6), (44, 1), false);
            press(cx, window, "cmd-enter");
            assert_eq!(
                chips(cx, &ws, claude),
                ["workspace.rs:43\u{2013}45"],
                "{n}: the chip"
            );
            assert_eq!(written(cx, &ws, claude), 0, "{n}: nothing was sent");

            // With no selection ⌘↵ does nothing: the file tab keeps the keys, no chip is added.
            select_tab(cx, window, &ws, file);
            click_text(cx, window, &tab);
            assert!(bar_of(cx, &tab).is_none(), "{n}: no bar with no selection");
            press(cx, window, "cmd-enter");
            assert_eq!(chips(cx, &ws, claude).len(), 1, "{n}: still one chip");
            let focused = cx.read(|cx| ws.read(cx).model.focused_tab());
            assert_eq!(focused, Some(file), "{n}: the file tab kept the keys");
        });
        println!("ui-shots: {n} ok");
    }
}

/// A plain press in the text (no drag): the selection is empty after it.
fn click_text(cx: &mut VisualTestAppContext, window: AnyWindowHandle, tab: &Entity<FileTab>) {
    let g = geom_of(cx, tab);
    let line = (g.scroll.y / g.line_height) as usize + 3;
    cx.simulate_click(window, text_point(&g, line, 2), Modifiers::none());
    settle_frames(cx, window);
}

/// The selection's last row at the view's bottom row: no room under it, so the bar sits above the
/// first row, a gap (6 pt) between. Both themes.
fn ask_bar_above() {
    let (root, doc) = scratch_doc();
    for &(slug, name) in shot_themes() {
        let n = format!("ask-bar-above-{slug}");
        let setup = bar_setup(root.clone(), doc.clone(), &["claude"]);
        let n_in = n.clone();
        drive_in(name, setup, move |cx, window, ws| {
            let n = n_in;
            let (file, _) = tabs_of(cx, &ws);
            let tab = file_view(cx, &ws, file);
            let g0 = geom_of(cx, &tab);
            let lh = f32::from(g0.line_height);
            scroll_to(cx, window, &tab, lh * 30.0);
            drag(cx, window, &tab, (42, 6), (44, 1), false);
            // Scrolled so row 45's bottom is 4 pt inside the view's bottom edge.
            let view_h = f32::from(g0.bounds.size.height);
            scroll_to(cx, window, &tab, lh * 45.0 - view_h + 4.0);
            let g = geom_of(cx, &tab);
            let bar = bar_of(cx, &tab).expect("a bar");
            let first_top = g.bounds.top() + g.line_height * 42.0 - g.scroll.y;
            assert!(
                same(bar.bottom(), first_top - px(6.0)),
                "{n}: bar bottom {:?}, first row's top {first_top:?}",
                bar.bottom()
            );
            assert!(same(bar.right(), g.bounds.right()), "{n}: right edge");
            keep_shot(&n, &shown(cx, window));
        });
        println!("ui-shots: {n} ok");
    }
}

/// While dragging (`selecting`) no bar; scrolled until the last row is off screen no bar, scrolled
/// back the bar; Esc clears the selection (no bar), a second Esc is not taken by the tab.
fn ask_bar_hides() {
    let n = "ask-bar-hides";
    let (root, doc) = scratch_doc();
    drive(bar_setup(root, doc, &["claude"]), move |cx, window, ws| {
        let (file, _) = tabs_of(cx, &ws);
        let tab = file_view(cx, &ws, file);
        let lh = f32::from(geom_of(cx, &tab).line_height);
        // Lines 3-5 (0-based 2..4), dragged and held.
        drag(cx, window, &tab, (2, 4), (4, 1), true);
        assert!(
            cx.read(|cx| tab.read(cx).selection()).is_some(),
            "{n}: selecting"
        );
        assert!(bar_of(cx, &tab).is_none(), "{n}: no bar while dragging");
        let g = geom_of(cx, &tab);
        cx.simulate_mouse_up(
            window,
            text_point(&g, 4, 1),
            MouseButton::Left,
            Modifiers::none(),
        );
        settle_frames(cx, window);
        assert!(bar_of(cx, &tab).is_some(), "{n}: the bar once released");
        // The last row (5) off the top: hidden; back: shown.
        scroll_to(cx, window, &tab, lh * 8.0);
        assert!(
            bar_of(cx, &tab).is_none(),
            "{n}: no bar with the last row off screen"
        );
        scroll_to(cx, window, &tab, 0.0);
        assert!(bar_of(cx, &tab).is_some(), "{n}: the bar is back");
        // Esc: the selection goes, so does the bar; the tab has nothing more for a second Esc.
        assert!(
            cx.read(|cx| tab.read(cx).esc_takes_selection_for_shots()),
            "{n}: Esc is taken"
        );
        press(cx, window, "escape");
        assert_eq!(
            cx.read(|cx| tab.read(cx).selection()),
            None,
            "{n}: the selection is gone"
        );
        assert!(bar_of(cx, &tab).is_none(), "{n}: and the bar");
        assert!(
            !cx.read(|cx| tab.read(cx).esc_takes_selection_for_shots()),
            "{n}: a second Esc goes on"
        );
        press(cx, window, "escape");
        assert_eq!(
            cx.read(|cx| tab.read(cx).selection()),
            None,
            "{n}: still none"
        );
    });
    println!("ui-shots: {n} ok");
}

/// Last-used agent tab agy: "Ask agy ⌘↵"; that tab closed: the label follows the next pick.
fn ask_bar_agy() {
    let n = "ask-bar-agy";
    let (root, doc) = scratch_doc();
    drive(
        bar_setup(root, doc, &["claude", "agy"]),
        move |cx, window, ws| {
            let (file, agents) = tabs_of(cx, &ws);
            let find = |p: &str| agents.iter().find(|(n, _)| n == p).expect("tab").1;
            let (claude, agy) = (find("claude"), find("agy"));
            for tab in [claude, agy, file] {
                select_tab(cx, window, &ws, tab);
            }
            let tab = file_view(cx, &ws, file);
            drag(cx, window, &tab, (2, 4), (4, 1), false);
            let words = cx.read(|cx| tab.read(cx).ask_button());
            assert_eq!(
                words,
                Some(("Ask agy \u{2318}\u{21b5}".into(), true)),
                "{n}: names agy"
            );
            cx.update_window(window, |_, window, cx| {
                ws.update(cx, |ws, cx| ws.close_tab(agy, window, cx))
            })
            .expect("close");
            settle_frames(cx, window);
            select_tab(cx, window, &ws, file);
            assert!(bar_of(cx, &tab).is_some(), "{n}: the bar stays");
            let words = cx.read(|cx| tab.read(cx).ask_button());
            assert_eq!(
                words,
                Some(("Ask claude \u{2318}\u{21b5}".into(), true)),
                "{n}: follows the next pick"
            );
        },
    );
    println!("ui-shots: {n} ok");
}

/// An SSH workspace has no agent tabs: the bar shows only "Copy with path", ⌘↵ does nothing (no
/// tab opens). Case 0: the SSH session first, the file tab in it; case 1: a local workspace whose
/// file tab was told its agent, then an SSH session opened beside it (the bar follows).
fn ask_bar_ssh() {
    use crate::restore::{Launch, SavedSession};
    use crate::ssh_target::SshTarget;
    use crate::workspace::At;
    let (root, doc) = scratch_doc();
    let ssh_launch = || Launch {
        saved: SavedSession {
            ssh: Some(SshTarget {
                host: "build.example".into(),
                tmux: false,
                tmux_name: None,
            }),
            ..Default::default()
        },
        run_preset: false,
    };
    let cat = || Some(vec!["/bin/cat".to_string()]);
    for case in 0..2 {
        let n = format!("ask-bar-ssh (case {case})");
        let (project, path) = (root.clone(), doc.clone());
        let setup = move |ws: &mut Workspace, window: &mut Window, cx: &mut Context<Workspace>| {
            if case == 0 {
                ws.open_now(ssh_launch(), At::New, cat(), None, window, cx);
            } else {
                ws.open_shot_session("shell", &project, "", None, window, cx);
            }
            ws.open_file_tab(path.clone(), FileMode::Plain, window, cx);
        };
        let n_in = n.clone();
        drive(setup, move |cx, window, ws| {
            let n = n_in;
            let (file, agents) = tabs_of(cx, &ws);
            assert!(agents.is_empty(), "{n}: no agent tab");
            let tab = file_view(cx, &ws, file);
            if case == 1 {
                // Local: told claude. Then an SSH session beside the file tab's pane.
                assert_eq!(
                    cx.read(|cx| tab.read(cx).ask_to()),
                    AskTo::Agent("claude"),
                    "{n}: local, so Ask names an agent"
                );
                let pane = cx.read(|cx| ws.read(cx).model.locate(file).expect("placed").1);
                cx.update_window(window, |_, window, cx| {
                    ws.update(cx, |ws, cx| {
                        let launch = Launch {
                            saved: SavedSession {
                                ssh: Some(SshTarget {
                                    host: "build.example".into(),
                                    tmux: false,
                                    tmux_name: None,
                                }),
                                ..Default::default()
                            },
                            run_preset: false,
                        };
                        let at = At::Tab(pane);
                        ws.open_now(
                            launch,
                            at,
                            Some(vec!["/bin/cat".to_string()]),
                            None,
                            window,
                            cx,
                        );
                    })
                })
                .expect("open ssh");
                settle_frames(cx, window);
                select_tab(cx, window, &ws, file);
            }
            assert_eq!(
                cx.read(|cx| tab.read(cx).ask_to()),
                AskTo::CopyOnly,
                "{n}: copy only"
            );
            let lh = f32::from(geom_of(cx, &tab).line_height);
            scroll_to(cx, window, &tab, lh * 30.0);
            drag(cx, window, &tab, (42, 6), (44, 1), false);
            assert!(bar_of(cx, &tab).is_some(), "{n}: the bar shows");
            assert_eq!(
                cx.read(|cx| tab.read(cx).ask_button()),
                None,
                "{n}: no Ask button"
            );
            assert!(
                cx.read(|cx| tab.read(cx).copy_button_bounds()).is_some(),
                "{n}: Copy with path"
            );
            let before = cx.read(|cx| ws.read(cx).model.focused_tab());
            press(cx, window, "cmd-enter");
            let (_, after) = tabs_of(cx, &ws);
            assert!(after.is_empty(), "{n}: ⌘↵ opened no agent tab");
            assert_eq!(
                cx.read(|cx| ws.read(cx).model.focused_tab()),
                before,
                "{n}: nothing moved"
            );
        });
        println!("ui-shots: {n} ok");
    }
}

/// A 501-line selection: the Ask button reads "Too long to ask" and is disabled (a click and ⌘↵
/// do nothing); Copy with path copies all 501 lines. Both themes: its words are readable on its
/// fill.
fn ask_bar_too_long() {
    let (root, doc) = scratch_file("long.rs", 600);
    for &(slug, name) in shot_themes() {
        let n = format!("ask-bar-too-long-{slug}");
        let ui = shot_theme(name).ui();
        assert!(
            contrast(ui.text_dim, ui.panel) >= TEXT_MIN,
            "{n}: faint words readable"
        );
        let setup = bar_setup(root.clone(), doc.clone(), &["claude"]);
        let n_in = n.clone();
        drive_in(name, setup, move |cx, window, ws| {
            let n = n_in;
            let (file, agents) = tabs_of(cx, &ws);
            let claude = agents[0].1;
            let tab = file_view(cx, &ws, file);
            // Lines 1-501 selected (0-based 0..=500), the last one scrolled into the view's bottom.
            cx.update(|cx| tab.update(cx, |t, cx| t.select_for_shots((0, 0), (500, 3), cx)));
            let g = geom_of(cx, &tab);
            let view_h = f32::from(g.bounds.size.height);
            scroll_to(
                cx,
                window,
                &tab,
                f32::from(g.line_height) * 501.0 - view_h + 4.0,
            );
            let bar = bar_of(cx, &tab).expect("a bar (inside the view)");
            let g = geom_of(cx, &tab);
            assert!(
                same(bar.bottom(), g.bounds.bottom() - px(6.0)),
                "{n}: inside, a gap off the bottom"
            );
            let words = cx.read(|cx| tab.read(cx).ask_button());
            assert_eq!(words, Some(("Too long to ask".into(), false)), "{n}");
            let ask = cx
                .read(|cx| tab.read(cx).ask_button_bounds())
                .expect("Ask's place");
            keep_shot(&n, &shown(cx, window));
            click(cx, window, ask);
            assert!(
                chips(cx, &ws, claude).is_empty(),
                "{n}: the click asked nobody"
            );
            assert_eq!(
                cx.read(|cx| ws.read(cx).model.focused_tab()),
                Some(file),
                "{n}: nothing moved"
            );
            select_tab(cx, window, &ws, file);
            press(cx, window, "cmd-enter");
            assert!(chips(cx, &ws, claude).is_empty(), "{n}: ⌘↵ asked nobody");
            // Copy with path takes all 501 lines.
            let copy = cx
                .read(|cx| tab.read(cx).copy_button_bounds())
                .expect("Copy's place");
            click(cx, window, copy);
            let clip = cx
                .read_from_clipboard()
                .and_then(|c| c.text())
                .expect("a clipboard text");
            let lines: Vec<&str> = clip.lines().collect();
            assert_eq!(lines.len(), 501 + 3, "{n}: header, fence, 501 lines, fence");
            assert!(lines[2].contains("fn main()"), "{n}: from the first line");
            assert!(
                lines[502].contains("line_500"),
                "{n}: to the 501st: {:?}",
                lines[502]
            );
        });
        println!("ui-shots: {n} ok");
    }
}
