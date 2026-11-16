//! The right sidebar (M11g, M11h): the sidebar itself, its panel tabs and the Files panel.

use super::*;

/// This area's scenes (`mod.rs` runs them in its `ORDER`).
pub(super) const SCENES: &[Scene] = &[
    Scene {
        name: "right-sidebar",
        run: right_sidebar_scenes,
        themes: Themes::Both,
        serial: true,
        kind: Kind::Visual,
        sources: &[
            "src/right_sidebar.rs",
            "src/sidebar_toggle.rs",
            "src/tab_bar.rs",
            "src/layout_model.rs",
            "src/reduce_motion.rs",
        ],
    },
    Scene {
        name: "files",
        run: files_scenes,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/right_sidebar.rs",
            "src/file_tree.rs",
            "src/git_status.rs",
            "src/file_tab.rs",
            "src/layout_model.rs",
            "src/tab_bar.rs",
        ],
    },
    Scene {
        name: "files-follow",
        run: files_follow_scenes,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: &[
            "src/right_sidebar.rs",
            "src/file_tree.rs",
            "src/git_status.rs",
            "src/file_tab.rs",
            "src/layout_model.rs",
        ],
    },
    Scene {
        name: "right-panels",
        run: right_panel_scenes,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/right_sidebar.rs",
            "src/file_tree.rs",
            "src/changes_panel.rs",
            "src/activity_panel.rs",
        ],
    },
];

const RIGHT_SIDEBAR_FROM_EDGE: f32 = 4.0 + 10.0;

/// The right sidebar (M11g Task 4), over `tab_bar_workspace`'s two panes in a 1280 × 600
/// window with the left sidebar shown (240 pt), in both themes:
/// - `right-sidebar-closed`: as a first run has it: no right sidebar; each pane's right-sidebar
///   icon (the far right of its bar) unlit.
/// - `right-sidebar-open`: open, 300 pt wide at the window's right; the icon lit (`ui.text` on
///   `ui.row_hover`) in every pane.
/// - `right-sidebar-resize`: open, its left edge dragged from x to x − 80: 80 pt wider.
///
/// Not shot: ⌥⌘\ opens it with a slide and closes it, and with Reduce motion on, without; the
/// icon in the right pane focuses that pane and opens it; it follows the focused terminal, and a
/// browser tab keeps the last one; ⌘W closes a tab and leaves the sidebar as it was.
fn right_sidebar_scenes() {
    use crate::workspace::{CloseSession, ToggleRightSidebar};
    use gpui::{KeyBinding, Modifiers, MouseButton};
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::time::Duration;

    let dims = size(px(1280.0), px(600.0));
    let (left, window_right) = (240.0_f32, 1280.0_f32);
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let ui = theme.ui();
        let near = |p: [u8; 3], c: alacritty_terminal::vte::ansi::Rgb| {
            p.iter()
                .zip([c.r, c.g, c.b])
                .all(|(a, b)| a.abs_diff(b) <= 3)
        };
        assert_ne!(
            ui.sidebar_background, theme.background,
            "{slug}: told apart"
        );
        assert_ne!(ui.row_hover, ui.sidebar_background, "{slug}: told apart");
        for scene in [
            "right-sidebar-closed",
            "right-sidebar-open",
            "right-sidebar-resize",
        ] {
            let slot: Slot = Rc::new(RefCell::new(None));
            let keep = slot.clone();
            let build = move |window: &mut Window, cx: &mut App| -> AnyView {
                cx.set_global(shot_appearance(theme, cx));
                let ws = cx.new(|cx| {
                    let mut ws = tab_bar_workspace(true, 0, window, cx);
                    ws.right_sidebar.hidden = scene == "right-sidebar-closed";
                    ws
                });
                *keep.borrow_mut() = Some(ws.clone());
                ws.into()
            };
            // Each pane's bar's right end, as laid out (M11i: inside its card's 1 pt edge; the
            // left one's: where the right one's bar starts, less both edges and the gap).
            let edges: Rc<RefCell<Vec<f32>>> = Rc::default();
            let seen = edges.clone();
            let acting = slot.clone();
            let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let ws = slotted(&acting);
                let n = format!("{scene}-{slug}");
                let none = Modifiers::none();
                let open = scene != "right-sidebar-closed";
                let panes = shown_panes(cx, &ws);
                assert_eq!(panes.len(), 2, "{n}: two panes");
                let second = tab_bounds(cx, &ws, panes[1], 0).expect("laid out");
                let pane_right = if open {
                    window_right - GAP - 300.0 - GAP - 1.0
                } else {
                    window_right - GAP - 1.0
                };
                *seen.borrow_mut() = vec![
                    f32::from(second.left()) - crate::ui_tokens::size::TABS_LEFT - 2.0 - GAP,
                    pane_right,
                ];
                cx.read(|cx| {
                    let ws = ws.read(cx);
                    assert_eq!(ws.right_sidebar.hidden, !open, "{n}");
                    assert_eq!(ws.right_sidebar.width, 300.0, "{n}: 300 pt by default");
                    assert!(ws.right_sidebar.slide.is_none(), "{n}: still");
                });
                if scene == "right-sidebar-resize" {
                    // The gap at its left (971, between the pane card at 968 and its card at
                    // 974) taken and moved 80 left, then let go.
                    let (from, to) = (point(px(971.0), px(300.0)), point(px(891.0), px(300.0)));
                    cx.simulate_mouse_down(window, from, MouseButton::Left, none);
                    assert!(
                        cx.read(|cx| ws.read(cx).right_sidebar.resizing),
                        "{n}: taken"
                    );
                    cx.simulate_mouse_move(window, to, MouseButton::Left, none);
                    cx.simulate_mouse_up(window, to, MouseButton::Left, none);
                    cx.read(|cx| {
                        let ws = ws.read(cx);
                        assert_eq!(ws.right_sidebar.width, 380.0, "{n}: 80 pt wider");
                        assert!(!ws.right_sidebar.resizing, "{n}: let go");
                    });
                    seen.borrow_mut()[1] = window_right - GAP - 380.0 - GAP - 1.0;
                    // The panes re-fit to the narrower area on the next frame.
                    cx.update_window(window, |_, window, _| window.refresh())
                        .expect("refresh");
                    cx.run_until_parked();
                    let second = tab_bounds(cx, &ws, panes[1], 0).expect("laid out");
                    seen.borrow_mut()[0] =
                        f32::from(second.left()) - crate::ui_tokens::size::TABS_LEFT - 2.0 - GAP;
                }
                acting.borrow_mut().take();
            };
            let path = shoot_with(scene, slug, &theme, dims, build.clone(), act.clone());
            println!("ui-shots: {}", path.display());
            let shot = render_with(&theme, dims, build, act);
            let n = format!("{scene}-{slug}");
            let at = |x: f32, y: f32| pixel_at(&shot, dims, x, y);
            let width = match scene {
                "right-sidebar-closed" => 0.0,
                "right-sidebar-open" => 300.0,
                _ => 380.0,
            };
            // The sidebar's body, low in the window: a card (M11i) 6 pt from the window's
            // right edge, its fill across its width; the 6 pt gap left of it, then the pane's
            // card with the terminal.
            let y = 400.0;
            let card_right = window_right - GAP;
            if width > 0.0 {
                for x in [card_right - width + 6.0, card_right - 6.0] {
                    let p = at(x, y);
                    assert!(near(p, ui.card), "{n}: sidebar at {x}: {p:?}");
                }
                // (Beside its middle: after a resize the pointer is still there, lighting it.)
                for x in [card_right - width - GAP + 1.0, window_right - GAP / 2.0] {
                    let p = at(x, y);
                    assert!(is_canvas(p, &ui), "{n}: the gap at {x}: {p:?}");
                }
                if scene == "right-sidebar-resize" {
                    // A quiet line (M11x, your ask: the blue one was "ugly"): faint text at
                    // half strength over the gap, never the accent.
                    let gap = at(card_right - width - GAP + 1.0, y);
                    let half = |g: u8, t: u8| ((u16::from(g) + u16::from(t)) / 2) as u8;
                    let want = alacritty_terminal::vte::ansi::Rgb {
                        r: half(gap[0], ui.text_faint.r),
                        g: half(gap[1], ui.text_faint.g),
                        b: half(gap[2], ui.text_faint.b),
                    };
                    let p = at(card_right - width - GAP / 2.0, y);
                    let close = p
                        .iter()
                        .zip([want.r, want.g, want.b])
                        .all(|(a, b)| a.abs_diff(b) <= 6);
                    assert!(close, "{n}: the hovered gap's line: {p:?}, want {want:?}");
                    assert!(!near(p, ui.accent), "{n}: the line is the accent");
                }
                let p = at(card_right - width - GAP - 6.0, y);
                assert!(near(p, theme.background), "{n}: the pane left of it: {p:?}");
            } else {
                let p = at(card_right - 6.0, y);
                assert!(near(p, theme.background), "{n}: no right sidebar: {p:?}");
                let p = at(window_right - GAP / 2.0, y);
                assert!(
                    is_canvas(p, &ui),
                    "{n}: the gap at the window's edge: {p:?}"
                );
            }
            // Every pane's icon: lit (the hover fill beside the glyph, the glyph's filled side
            // in the text colour) while open; else on the bar, dim.
            for (i, right) in edges.borrow().iter().enumerate() {
                let centre = right - RIGHT_SIDEBAR_FROM_EDGE;
                assert!(*right > left, "{n}: pane {i}'s edge {right}");
                // The bars run 7–37 (under the 6 pt strip and the cards' edges).
                let (fill, glyph) = (at(centre - 8.0, 22.0), at(centre + 3.0, 22.0));
                if width > 0.0 {
                    assert!(near(fill, ui.tab_hover), "{n}: pane {i} lit: {fill:?}");
                    assert!(near(glyph, ui.text), "{n}: pane {i} glyph: {glyph:?}");
                } else {
                    let bar = ui.tab_bar;
                    assert!(near(fill, bar), "{n}: pane {i} unlit: {fill:?}");
                    assert!(
                        near(glyph, ui.text_dim),
                        "{n}: pane {i} glyph dim: {glyph:?}"
                    );
                }
            }
        }

        // Acting on it (not shot): the key, Reduce motion, the icon, following, ⌘W.
        for still in [false, true] {
            let slot: Slot = Rc::new(RefCell::new(None));
            let keep = slot.clone();
            let build = move |window: &mut Window, cx: &mut App| -> AnyView {
                cx.set_global(shot_appearance(theme, cx));
                cx.set_reduce_motion(still);
                let ws = cx.new(|cx| tab_bar_workspace(true, 0, window, cx));
                *keep.borrow_mut() = Some(ws.clone());
                ws.into()
            };
            let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let ws = slotted(&slot);
                let n = format!(
                    "right-sidebar-act-{slug}{}",
                    if still { "-still" } else { "" }
                );
                let none = Modifiers::none();
                // As main.rs binds them.
                cx.update(|cx| {
                    cx.bind_keys([
                        KeyBinding::new("alt-cmd-\\", ToggleRightSidebar, None),
                        KeyBinding::new("cmd-w", CloseSession, None),
                    ])
                });
                let state = |cx: &mut VisualTestAppContext| {
                    cx.read(|cx| {
                        let rs = &ws.read(cx).right_sidebar;
                        (rs.hidden, rs.slide.map(|s| s.showing))
                    })
                };
                let frame = |cx: &mut VisualTestAppContext| {
                    cx.update_window(window, |_, window, _| window.refresh())
                        .expect("refresh");
                    cx.run_until_parked();
                };
                assert!(state(cx).0, "{n}: closed at first");
                cx.simulate_keystrokes(window, "alt-cmd-\\");
                let slide = if still { None } else { Some(true) };
                assert_eq!(state(cx), (false, slide), "{n}: ⌥⌘\\ opens it");
                cx.simulate_keystrokes(window, "alt-cmd-\\");
                let slide = if still { None } else { Some(false) };
                assert_eq!(state(cx), (true, slide), "{n}: and closes it");
                // Once it has slid away, the right pane's icon: that pane takes the focus and
                // the sidebar opens.
                prof::sleep(crate::sidebar_toggle::SLIDE + Duration::from_millis(30));
                frame(cx);
                assert_eq!(state(cx), (true, None), "{n}: slid away");
                let panes = shown_panes(cx, &ws);
                let icon = point(
                    px(window_right - GAP - 1.0 - RIGHT_SIDEBAR_FROM_EDGE),
                    px(22.0),
                );
                cx.simulate_click(window, icon, none);
                cx.read(|cx| {
                    let ws = ws.read(cx);
                    assert_eq!(ws.model.focused_pane(), Some(panes[1]), "{n}: its pane");
                    assert!(!ws.right_sidebar.hidden, "{n}: the icon opens it");
                });
                frame(cx);
                // It follows the focused terminal: tests, in the right pane.
                let id_of = |cx: &mut VisualTestAppContext, name: &str| {
                    cx.read(|cx| {
                        let ws = ws.read(cx);
                        ws.sessions
                            .items()
                            .iter()
                            .find(|s| s.label() == name)
                            .map(|s| s.id)
                    })
                };
                let tests = id_of(cx, "tests");
                let followed = |cx: &mut VisualTestAppContext| {
                    cx.read(|cx| {
                        let ws = ws.read(cx);
                        (ws.right_sidebar.followed, ws.right_sidebar_folder(cx))
                    })
                };
                let api_folder = Some((PathBuf::from(API), false));
                assert_eq!(
                    followed(cx),
                    (tests, api_folder.clone()),
                    "{n}: tests' folder"
                );
                // The left pane's browser tab shown (as a click on it does; the narrowed pane's
                // row scrolls it under the buttons): still tests'.
                let select = |cx: &mut VisualTestAppContext, i: usize| {
                    cx.update_window(window, |_, window, cx| {
                        ws.update(cx, |ws, cx| {
                            let tab = ws.model.pane(panes[0]).expect("the pane").tabs[i].id;
                            ws.select_tab(tab, window, cx)
                        })
                    })
                    .expect("select");
                    frame(cx);
                };
                select(cx, 3);
                let focused_terminal = cx.read(|cx| ws.read(cx).focused_terminal());
                assert_eq!(focused_terminal, None, "{n}: the browser tab is shown");
                assert_eq!(followed(cx).0, tests, "{n}: keeps the last terminal");
                // api shown: it's followed.
                select(cx, 0);
                assert_eq!(followed(cx).0, id_of(cx, "api"), "{n}: follows api");
                // ⌘W: api's tab closes; the sidebar stays open, as wide.
                cx.simulate_keystrokes(window, "cmd-w");
                cx.read(|cx| {
                    let ws = ws.read(cx);
                    assert_eq!(
                        ws.sessions.position(|s| s.label() == "api"),
                        None,
                        "{n}: ⌘W"
                    );
                    assert!(!ws.right_sidebar.hidden, "{n}: still open");
                    assert_eq!(ws.right_sidebar.width, 300.0, "{n}: as wide");
                });
                slot.borrow_mut().take();
            };
            render_with(&theme, dims, build, act);
            let still = if still { "-still" } else { "" };
            println!("ui-shots: right-sidebar-act-{slug}{still} ok");
        }
    }
}

/// The Files panel's fixture folder (under the temp folder; written, never read back as the
/// user's): docs/ (guide.md), src/ (main.rs, new.rs, util/a.rs), Cargo.toml, README.md and
/// notes.txt. Its real path (the temp folder is a link on macOS).
pub(super) fn files_fixture() -> PathBuf {
    let root = std::env::temp_dir().join("fuxx-ui-shots-files");
    for folder in ["docs", "src/util"] {
        std::fs::create_dir_all(root.join(folder)).expect("the fixture's folders");
    }
    // Made by the `files-keys` scene, which removes it; gone if a failed run left it.
    std::fs::remove_file(root.join("src/zz.rs")).ok();
    for file in [
        "docs/guide.md",
        "src/main.rs",
        "src/new.rs",
        "src/util/a.rs",
        "Cargo.toml",
        "README.md",
        "notes.txt",
    ] {
        std::fs::write(root.join(file), "fn main() {}\n").expect("the fixture's files");
    }
    std::fs::canonicalize(&root).unwrap_or(root)
}

/// The fixture's tree as the panel builds it for a terminal in its src/ (listed from the disk,
/// no git): src open and selected (with `util` open too when `open_util`), and a made-up mark
/// map: main.rs and README.md modified, new.rs added, old.rs deleted (so not on the disk).
fn files_tree(root: &std::path::Path, open_util: bool) -> crate::file_tree::FileTree {
    use crate::file_tree::{CAP, FileTree};
    use crate::git_status::{Mark, list_dir};
    use std::collections::HashMap;

    let mut tree = FileTree::new(root.to_path_buf(), Some(PathBuf::from("src")));
    let mut folders = tree.wanted();
    if open_util {
        folders.push(PathBuf::from("src/util"));
    }
    for rel in folders {
        let (entries, more) = list_dir(&root.join(&rel), CAP);
        tree.expand(rel, entries, more);
    }
    tree.set_marks(HashMap::from([
        (PathBuf::from("src/main.rs"), Mark::Modified),
        (PathBuf::from("src/new.rs"), Mark::Added),
        (PathBuf::from("old.rs"), Mark::Deleted),
        (PathBuf::from("README.md"), Mark::Modified),
    ]));
    tree
}

/// The Files panel (M11g Task 5) in the right sidebar, on `files_fixture` (no git: a made-up
/// mark map), beside a terminal in its src/, in a 1100 × 560 window, in both themes:
/// - `files`: the left sidebar shown, util open too. The tree: docs, src (open, selected, `M`:
///   it holds a modified and an added file), util (open), a.rs, main.rs (`M`, blue), new.rs
///   (`A`, green), Cargo.toml, notes.txt, old.rs (`D`, red: gone from the disk), README.md
///   (`M`). The panel's header row is level with the pane's tab bar, at the top. M11i: folder
///   icons in the dim text colour (open for src and util), file icons tinted by type (.rs
///   red, .toml cyan, .txt and .md dim), indent guides at depth 1 and 2, none at depth 0.
/// - `files-filter`: "main" typed in the filter box: src and main.rs only.
/// - `files-bare`: the left sidebar hidden: the header row is still level with the tab bar,
///   both under the 38 pt band.
/// - `files-header` (M11i): the root header above the filter: ▾, the root's name, collapse
///   all at the right, and the root's path under them, its start cut at the 300 pt sidebar.
///   After a click on collapse all, every folder is closed (the selection on src).
/// - `files-header-collapsed`: then a click on the name: the root folds away (▸), the filter
///   and the tree are gone; a click again brings them back (every folder still closed), and a
///   third folds it away again for the shot.
///
/// Not shot: in the tree, ↓ → ← ↑ and Return move, open and close; a click on main.rs opens its
/// file tab, which takes the keyboard; Esc in the tree gives the keyboard back to the pane;
/// with the filter box focused, ⌥⌘\ closes the sidebar and the terminal takes the keyboard.
fn files_scenes() {
    use crate::git_status::Mark;
    use crate::layout_model::{FileMode, TabKind};
    use crate::right_sidebar::{FILTER_BAND, HEADER, LIST_TOP, ROW, mark_color};
    use crate::ui_tokens::size::{FOCUS_RING, TAB_BAR, TAB_TOP_GAP, TOP_BAND};
    use gpui::{Focusable, Modifiers};
    use std::cell::RefCell;
    use std::rc::Rc;

    let dims = size(px(1100.0), px(560.0));
    let window_right = 1100.0_f32;
    // Inside the right sidebar's card (M11i): 6 pt from the window's right, 300 wide, a 1 pt
    // edge; its header 1 pt under the card's top (under the 6 pt strip, or the band).
    let sidebar_left = window_right - GAP - 300.0 + FOCUS_RING;
    let card_top = GAP + FOCUS_RING;
    let root = files_fixture();
    let rows_of = |cx: &mut VisualTestAppContext, ws: &gpui::Entity<Workspace>| {
        cx.read(|cx| {
            let files = &ws.read(cx).right_sidebar.files;
            let rows = files.rows.iter().filter(|r| r.more.is_none());
            rows.map(|r| r.rel.display().to_string())
                .collect::<Vec<_>>()
        })
    };
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let ui = theme.ui();
        let near = |p: [u8; 3], c: alacritty_terminal::vte::ansi::Rgb, by: u8| {
            p.iter()
                .zip([c.r, c.g, c.b])
                .all(|(a, b)| a.abs_diff(b) <= by)
        };
        let build_for = |bare: bool, open_util: bool, slot: Slot| {
            let root = root.clone();
            move |window: &mut Window, cx: &mut App| -> AnyView {
                cx.set_global(shot_appearance(theme, cx));
                cx.bind_keys(crate::key_bindings());
                let ws = cx.new(|cx| {
                    let mut ws = Workspace::for_shots(window, cx);
                    ws.sidebar.hint_dismissed = true;
                    ws.sidebar.hidden = bare;
                    ws.right_sidebar.hidden = false;
                    let src = root.join("src");
                    ws.open_shot_session("api", &src, SHELL_TEXT, None, window, cx);
                    let tree = files_tree(&root, open_util);
                    ws.right_sidebar.files.show_for_shots(tree);
                    ws
                });
                *slot.borrow_mut() = Some(ws.clone());
                ws.into()
            }
        };
        let frame = |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
            cx.update_window(window, |_, window, _| window.refresh())
                .expect("refresh");
            cx.run_until_parked();
        };
        let tree_rows = [
            "docs",
            "src",
            "src/util",
            "src/main.rs",
            "src/new.rs",
            "Cargo.toml",
            "notes.txt",
            "old.rs",
            "README.md",
        ];

        for scene in ["files", "files-filter", "files-bare"] {
            let bare = scene == "files-bare";
            // The header row's top: under the band with the left sidebar hidden.
            let top = if bare {
                TOP_BAND + FOCUS_RING
            } else {
                card_top
            };
            let slot: Slot = Rc::new(RefCell::new(None));
            let open_util = scene == "files";
            let build = build_for(bare, open_util, slot.clone());
            let tab_top: Rc<RefCell<Option<f32>>> = Rc::default();
            let seen = tab_top.clone();
            let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let ws = slotted(&slot);
                let n = format!("{scene}-{slug}");
                frame(cx, window);
                let mut rows = tree_rows.to_vec();
                if open_util {
                    rows.insert(3, "src/util/a.rs");
                }
                assert_eq!(rows_of(cx, &ws), rows, "{n}: the tree");
                let pane = cx.read(|cx| ws.read(cx).model.focused_pane().expect("a pane"));
                let tab = tab_bounds(cx, &ws, pane, 0).expect("the tab laid out");
                *seen.borrow_mut() = Some(f32::from(tab.top()));
                if scene == "files-filter" {
                    let field = point(
                        px(sidebar_left + 60.0),
                        px(top + TAB_BAR + HEADER + FILTER_BAND / 2.0),
                    );
                    cx.simulate_click(window, field, Modifiers::none());
                    let typing = cx
                        .update_window(window, |_, window, cx| {
                            let ws = ws.read(cx);
                            let input = ws.right_sidebar.files.input.clone().expect("the box");
                            input.focus_handle(cx).is_focused(window)
                        })
                        .expect("read");
                    assert!(typing, "{n}: a click in the box focuses it");
                    cx.simulate_input(window, "main");
                    frame(cx, window);
                    assert_eq!(
                        rows_of(cx, &ws),
                        ["src", "src/main.rs"],
                        "{n}: main.rs with its folder"
                    );
                }
                slot.borrow_mut().take();
            };
            let path = shoot_with(scene, slug, &theme, dims, build.clone(), act.clone());
            println!("ui-shots: {}", path.display());
            let shot = render_with(&theme, dims, build, act);
            let n = format!("{scene}-{slug}");
            let at = |x: f32, y: f32| pixel_at(&shot, dims, x, y);
            // The header row lines up with the pane's tab bar: same top (its tabs 4 pt under
            // it, M11i), and its bottom edge where the bar ends.
            let bar_top = tab_top.borrow().expect("the tab's top") - TAB_TOP_GAP;
            assert!(
                (bar_top - top).abs() < 0.5,
                "{n}: the tab bar at {bar_top}, not {top}"
            );
            // M11h: the strip holds the panel tabs on `tab_bar` (its empty part at the right),
            // with no border under it: the card right below.
            let inside = at(sidebar_left + 250.0, top + TAB_BAR / 2.0);
            assert!(near(inside, ui.tab_bar, 3), "{n}: the strip: {inside:?}");
            let edge = at(sidebar_left + 250.0, top + TAB_BAR - 0.5);
            assert!(
                near(edge, ui.tab_bar, 3),
                "{n}: no border under the strip: {edge:?}"
            );
            let below = at(sidebar_left + 250.0, top + TAB_BAR + 0.5);
            assert!(near(below, ui.card, 3), "{n}: the card under it: {below:?}");
            // Each row's mark colour, somewhere along its name and letter (drawn in it), right
            // of its icon.
            let list_top = top + TAB_BAR + HEADER + FILTER_BAND + LIST_TOP;
            let depths: &[usize] = match scene {
                "files" => &[0, 0, 1, 2, 1, 1, 0, 0, 0, 0],
                "files-filter" => &[0, 1],
                _ => &[0, 0, 1, 1, 1, 0, 0, 0, 0],
            };
            // A row's icon box (M11v part 4): on the 12 pt edge (the pill 6 pt in, its padding
            // 6), 16 pt a level, no chevron column.
            let icon_left = |row: usize| sidebar_left + 12.0 + depths[row] as f32 * 16.0;
            let ink_in = |row: usize, x0: f32, x1: f32, c: alacritty_terminal::vte::ansi::Rgb| {
                let y0 = list_top + row as f32 * ROW;
                let steps = ((x1 - x0) * 2.0) as usize;
                (4..(ROW as usize * 2 - 4)).any(|dy| {
                    let y = y0 + dy as f32 * 0.5;
                    (0..steps).any(|dx| near(at(x0 + dx as f32 * 0.5, y), c, 24))
                })
            };
            let inked = |row: usize, c: alacritty_terminal::vte::ansi::Rgb| {
                let x0 = icon_left(row) + 12.0 + 1.0;
                ink_in(row, x0, sidebar_left + 290.0, c)
            };
            let (m, a, d) = (
                mark_color(Mark::Modified, &theme),
                mark_color(Mark::Added, &theme),
                mark_color(Mark::Deleted, &theme),
            );
            let expect: Vec<(usize, Option<alacritty_terminal::vte::ansi::Rgb>)> = match scene {
                "files-filter" => vec![(0, Some(m)), (1, Some(m))],
                "files" => vec![
                    (0, None),
                    (1, Some(m)),
                    (3, None),
                    (4, Some(m)),
                    (5, Some(a)),
                    (7, None),
                    (8, Some(d)),
                    (9, Some(m)),
                ],
                _ => vec![
                    (0, None),
                    (1, Some(m)),
                    (3, Some(m)),
                    (4, Some(a)),
                    (6, None),
                    (7, Some(d)),
                    (8, Some(m)),
                ],
            };
            for (row, colour) in expect {
                match colour {
                    Some(c) => assert!(inked(row, c), "{n}: row {row} in {c:?}"),
                    None => {
                        for c in [m, a, d] {
                            assert!(!inked(row, c), "{n}: row {row} unmarked, has {c:?}");
                        }
                    }
                }
            }
            if scene != "files" {
                continue;
            }
            // M11v part 4: every icon one quiet grey (`text_faint`), in its 16 pt box; colour is
            // left to the changed files' names and letters.
            let quiet = ui.text_faint;
            let (red, cyan) = (theme.palette[1], theme.palette[6]);
            let icon = |row: usize, c| ink_in(row, icon_left(row), icon_left(row) + 16.0, c);
            for (row, what) in [
                (0, "docs"),
                (1, "src (open)"),
                (2, "util (open)"),
                (3, "a.rs"),
                (4, "main.rs"),
                (6, "Cargo.toml"),
                (7, "notes.txt"),
                (9, "README.md"),
            ] {
                assert!(icon(row, quiet), "{n}: {what}'s icon in the quiet grey");
                assert!(
                    !icon(row, red) && !icon(row, cyan),
                    "{n}: {what}'s icon isn't tinted by its type"
                );
            }
            // An open folder's icon differs from a closed one's: its flap's top edge crosses
            // the middle of the 16 pt box (6 pt down the 28 pt row), where a closed folder is
            // empty. (No triangles, part 4: the icon alone says open or closed.)
            let open_flap = |row: usize| {
                let (x, y) = (
                    icon_left(row) + 8.0,
                    list_top + row as f32 * ROW + 6.0 + 7.3,
                );
                near(at(x, y), quiet, 40)
            };
            assert!(open_flap(1) && !open_flap(0), "{n}: src open, docs closed");
            // Indent guides: 1 pt in `guide` at each level above the row, at the middle of
            // that level's chevron column (8 + 14 d + 6), the row's full height; none at the
            // top level. (It spans x - 0.5 to x + 0.5: on a 1x shot it lands on one side.)
            let guide_at = |row: usize, x: f32| {
                let y0 = list_top + row as f32 * ROW;
                [x - 1.0, x].iter().any(|&x| {
                    [0.5, ROW / 2.0, ROW - 0.5]
                        .iter()
                        .all(|dy| near(at(sidebar_left + x, y0 + dy), ui.guide, 3))
                })
            };
            // Part 4: a level's guide runs down the middle of that level's icon box: 12 pt
            // in, 16 pt a level, 8 pt to its middle.
            for row in [2, 3, 4, 5] {
                assert!(guide_at(row, 20.0), "{n}: row {row}'s depth-1 guide");
            }
            assert!(guide_at(3, 36.0), "{n}: a.rs's depth-2 guide");
            assert!(!guide_at(4, 36.0), "{n}: main.rs is at depth 1");
            for row in [6, 7, 9] {
                assert!(
                    !guide_at(row, 20.0),
                    "{n}: no guide at the top level ({row})"
                );
            }
        }

        // M11i: the root header; collapse all, then the root folded away.
        for scene in ["files-header", "files-header-collapsed"] {
            let fold = scene == "files-header-collapsed";
            let slot: Slot = Rc::new(RefCell::new(None));
            let build = build_for(false, true, slot.clone());
            let header_top = card_top + TAB_BAR;
            // Part 4: one line, centred in the 42 pt header; Collapse all is a 26 pt button 8 pt
            // in from the right.
            let line_one = header_top + HEADER / 2.0;
            let button = point(
                px(window_right - GAP - FOCUS_RING - 8.0 - 13.0),
                px(line_one),
            );
            let name = point(px(sidebar_left + 60.0), px(line_one));
            let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let ws = slotted(&slot);
                let n = format!("{scene}-{slug}");
                let none = Modifiers::none();
                frame(cx, window);
                let state = |cx: &mut VisualTestAppContext| {
                    cx.read(|cx| {
                        let tree = ws.read(cx).right_sidebar.files.tree.as_ref();
                        let tree = tree.expect("the tree");
                        let selected = tree.selected().map(|s| s.display().to_string());
                        (tree.expanded_set(), selected)
                    })
                };
                cx.simulate_click(window, button, none);
                frame(cx, window);
                assert_eq!(
                    rows_of(cx, &ws),
                    [
                        "docs",
                        "src",
                        "Cargo.toml",
                        "notes.txt",
                        "old.rs",
                        "README.md"
                    ],
                    "{n}: collapse all closes every folder"
                );
                let (memory, selected) = state(cx);
                assert!(memory.expanded.is_empty(), "{n}: none open: {memory:?}");
                assert!(!memory.root_collapsed, "{n}: the root stays open");
                assert_eq!(selected.as_deref(), Some("src"), "{n}: the selection");
                if fold {
                    cx.simulate_click(window, name, none);
                    frame(cx, window);
                    assert!(rows_of(cx, &ws).is_empty(), "{n}: the tree is hidden");
                    assert!(state(cx).0.root_collapsed, "{n}: folded away");
                    // A click again: back, its folders as they were (all closed).
                    cx.simulate_click(window, name, none);
                    frame(cx, window);
                    assert_eq!(rows_of(cx, &ws).len(), 6, "{n}: back");
                    cx.simulate_click(window, name, none);
                    frame(cx, window);
                    assert!(rows_of(cx, &ws).is_empty(), "{n}: and away again");
                }
                // The pointer away over the pane: no hover or tooltip in the shot.
                cx.simulate_mouse_move(window, point(px(500.0), px(300.0)), None, none);
                frame(cx, window);
                slot.borrow_mut().take();
            };
            let path = shoot_with(scene, slug, &theme, dims, build.clone(), act.clone());
            println!("ui-shots: {}", path.display());
            let shot = render_with(&theme, dims, build, act);
            let n = format!("{scene}-{slug}");
            let at = |x: f32, y: f32| pixel_at(&shot, dims, x, y);
            let has = |x0: f32, x1: f32, y0: f32, y1: f32, c, by| {
                let (w, h) = (((x1 - x0) * 2.0) as usize, ((y1 - y0) * 2.0) as usize);
                (0..h).any(|dy| {
                    (0..w).any(|dx| near(at(x0 + dx as f32 * 0.5, y0 + dy as f32 * 0.5), c, by))
                })
            };
            // Part 4: one line, centred in the 42 pt header: the name in the text colour on the
            // 12 pt edge (its path is in its tooltip, no second line), Collapse all's icon in a
            // 26 pt button 8 pt in from the right.
            let line_top = header_top + (HEADER - 20.0) / 2.0;
            let (left, right) = (sidebar_left + 12.0, window_right - GAP - FOCUS_RING - 8.0);
            assert!(
                has(left, left + 140.0, line_top, line_top + 20.0, ui.text, 24),
                "{n}: the root's name"
            );
            assert!(
                !has(
                    left,
                    left + 200.0,
                    line_top + 21.0,
                    header_top + HEADER - 1.0,
                    ui.text_faint,
                    12
                ),
                "{n}: no path line under the name"
            );
            assert!(
                has(
                    right - 26.0,
                    right,
                    line_top,
                    line_top + 20.0,
                    ui.text_faint,
                    24
                ) || has(
                    right - 26.0,
                    right,
                    line_top,
                    line_top + 20.0,
                    ui.text_dim,
                    24
                ),
                "{n}: the collapse all button"
            );
            let filter_middle = header_top + HEADER + FILTER_BAND / 2.0;
            let filter_box = (0..548)
                .map(|i| sidebar_left + 6.0 + i as f32 * 0.5)
                .any(|x| !near(at(x, filter_middle), ui.card, 3));
            if fold {
                // Only the header: no filter box, no rows under it.
                assert!(!filter_box, "{n}: no filter box");
                let rows_top = header_top + HEADER;
                assert!(
                    !has(
                        sidebar_left + 2.0,
                        right,
                        rows_top,
                        rows_top + 120.0,
                        ui.text,
                        24
                    ),
                    "{n}: no rows"
                );
            } else {
                assert!(filter_box, "{n}: the filter box under the header");
            }
        }

        // Keys, a click, Esc (not shot).
        let slot: Slot = Rc::new(RefCell::new(None));
        let build = build_for(false, false, slot.clone());
        let root_for = root.clone();
        let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
            let ws = slotted(&slot);
            let n = format!("files-keys-{slug}");
            let none = Modifiers::none();
            frame(cx, window);
            let list_top = card_top + TAB_BAR + HEADER + FILTER_BAND + LIST_TOP;
            let row_at = |i: usize| {
                point(
                    px(sidebar_left + 80.0),
                    px(list_top + (i as f32 + 0.5) * ROW),
                )
            };
            let selected = |cx: &mut VisualTestAppContext| {
                cx.read(|cx| {
                    let files = &ws.read(cx).right_sidebar.files;
                    let tree = files.tree.as_ref().expect("the tree");
                    tree.selected().map(|s| s.display().to_string())
                })
            };
            let panel_keys = |cx: &mut VisualTestAppContext| {
                cx.update_window(window, |_, window, cx| {
                    let focus = ws
                        .read(cx)
                        .right_sidebar
                        .files
                        .focus
                        .clone()
                        .expect("its focus");
                    (focus.is_focused(window), focus.contains_focused(window, cx))
                })
                .expect("read")
            };
            // A click below the rows: the panel takes the keyboard.
            cx.simulate_click(window, row_at(12), none);
            assert_eq!(
                panel_keys(cx),
                (true, true),
                "{n}: the tree has the keyboard"
            );
            assert_eq!(
                selected(cx).as_deref(),
                Some("src"),
                "{n}: the terminal's folder"
            );
            cx.simulate_keystrokes(window, "down");
            assert_eq!(selected(cx).as_deref(), Some("src/util"), "{n}: ↓");
            cx.simulate_keystrokes(window, "right");
            frame(cx, window);
            let rows = rows_of(cx, &ws);
            assert!(
                rows.contains(&"src/util/a.rs".to_string()),
                "{n}: → lists util: {rows:?}"
            );
            cx.simulate_keystrokes(window, "left");
            frame(cx, window);
            assert!(
                !rows_of(cx, &ws).contains(&"src/util/a.rs".to_string()),
                "{n}: ← closes it"
            );
            cx.simulate_keystrokes(window, "up enter");
            frame(cx, window);
            assert_eq!(selected(cx).as_deref(), Some("src"), "{n}: ↑");
            assert!(
                !rows_of(cx, &ws).contains(&"src/main.rs".to_string()),
                "{n}: Return closes src"
            );
            // A file made in src while it was closed (by an agent, say): Return opens src from
            // its kept listing and lists it again, so the file shows (Task 6 review).
            let made = root_for.join("src/zz.rs");
            std::fs::write(&made, "\n").expect("a file made while src was closed");
            cx.simulate_keystrokes(window, "enter");
            frame(cx, window);
            let mut with_made = tree_rows.to_vec();
            with_made.insert(5, "src/zz.rs");
            let rows = rows_of(cx, &ws);
            std::fs::remove_file(&made).ok();
            assert_eq!(rows, with_made, "{n}: and opens it, listed again");
            // A click on main.rs: its file tab, in the focused pane, with the keyboard.
            cx.simulate_click(window, row_at(3), none);
            frame(cx, window);
            let main_rs = root_for.join("src/main.rs");
            let (kind, tab_keys) = cx
                .update_window(window, |_, window, cx| {
                    let ws = ws.read(cx);
                    let pane = ws.model.focused_pane().expect("a pane");
                    let p = ws.model.pane(pane).expect("known");
                    let active = p.tabs.iter().find(|t| t.id == p.active).expect("its tab");
                    let view = ws.focused_file_tab().expect("the file tab's view");
                    (
                        active.kind.clone(),
                        view.read(cx).focus_handle(cx).is_focused(window),
                    )
                })
                .expect("read");
            assert_eq!(
                kind,
                TabKind::File {
                    path: main_rs,
                    mode: FileMode::Plain
                },
                "{n}: the click opens main.rs"
            );
            assert!(tab_keys, "{n}: the file tab has the keyboard");
            // M11t: that single click previews it; a single click on new.rs shows it in the same
            // tab (no new one), still a preview.
            let shown = |cx: &mut VisualTestAppContext| {
                cx.read(|cx| {
                    let ws = ws.read(cx);
                    let pane = ws.model.focused_pane().expect("a pane");
                    let p = ws.model.pane(pane).expect("known");
                    let active = p.tabs.iter().find(|t| t.id == p.active).expect("its tab");
                    (
                        active.id,
                        active.kind.clone(),
                        ws.model.is_preview(active.id),
                        p.tabs.len(),
                    )
                })
            };
            let (first, _, preview, count) = shown(cx);
            assert!(preview, "{n}: a single click previews");
            cx.simulate_click(window, row_at(4), none);
            frame(cx, window);
            let (again, kind, preview, after) = shown(cx);
            assert_eq!(again, first, "{n}: the same preview tab");
            assert_eq!(
                kind,
                TabKind::File {
                    path: root_for.join("src/new.rs"),
                    mode: FileMode::Plain
                },
                "{n}: now showing new.rs"
            );
            assert!(preview && after == count, "{n}: no new tab");
            // Back in the tree, Esc: the pane's tab takes the keyboard back.
            cx.simulate_click(window, row_at(12), none);
            assert_eq!(panel_keys(cx), (true, true), "{n}: the tree again");
            cx.simulate_keystrokes(window, "escape");
            let back = cx
                .update_window(window, |_, window, cx| {
                    let ws = ws.read(cx);
                    let view = ws.focused_file_tab().expect("the file tab's view");
                    view.read(cx).focus_handle(cx).is_focused(window)
                })
                .expect("read");
            assert!(back, "{n}: Esc gives the keyboard to the pane");
            assert_eq!(panel_keys(cx), (false, false), "{n}: not the tree's");
            slot.borrow_mut().take();
        };
        render_with(&theme, dims, build, act);
        println!("ui-shots: files-keys-{slug} ok");

        // The filter box focused, ⌥⌘\: the sidebar closes, the terminal takes the keyboard.
        let slot: Slot = Rc::new(RefCell::new(None));
        let build = build_for(false, false, slot.clone());
        let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
            let ws = slotted(&slot);
            let n = format!("files-close-{slug}");
            frame(cx, window);
            let field = point(
                px(sidebar_left + 60.0),
                px(card_top + TAB_BAR + HEADER + FILTER_BAND / 2.0),
            );
            cx.simulate_click(window, field, Modifiers::none());
            let typing = cx
                .update_window(window, |_, window, cx| {
                    let input = ws
                        .read(cx)
                        .right_sidebar
                        .files
                        .input
                        .clone()
                        .expect("the box");
                    input.focus_handle(cx).is_focused(window)
                })
                .expect("read");
            assert!(typing, "{n}: the box has the keyboard");
            // ← → from the box don't touch the tree.
            let tree_state = |cx: &mut VisualTestAppContext| {
                cx.read(|cx| {
                    let tree = ws.read(cx).right_sidebar.files.tree.as_ref().map(|t| {
                        let selected = t.selected().map(|s| s.to_path_buf());
                        (t.expanded_set(), selected)
                    });
                    tree.expect("the tree")
                })
            };
            let before = tree_state(cx);
            cx.simulate_keystrokes(window, "right left left");
            assert_eq!(tree_state(cx), before, "{n}: ← → in the box change nothing");
            cx.simulate_keystrokes(window, "alt-cmd-\\");
            frame(cx, window);
            let (hidden, terminal_keys) = cx
                .update_window(window, |_, window, cx| {
                    let ws = ws.read(cx);
                    let s = ws.sessions.active().expect("the terminal");
                    (
                        ws.right_sidebar.hidden,
                        s.view.focus_handle(cx).is_focused(window),
                    )
                })
                .expect("read");
            assert!(hidden, "{n}: closed");
            assert!(terminal_keys, "{n}: the terminal has the keyboard");
            slot.borrow_mut().take();
        };
        render_with(&theme, dims, build, act);
        println!("ui-shots: files-close-{slug} ok");
    }
}

/// A workspace for the right sidebar's panel scenes (M11h): the Files panel's fixture beside a
/// terminal (`api`) in its src/, the right sidebar open `width` wide showing `panel`, the Files
/// tree made as in `files_scenes`; `ready` then sets up anything else.
pub(super) fn right_panel_workspace(
    theme: Theme,
    root: PathBuf,
    width: f32,
    panel: crate::right_sidebar::Panel,
    slot: Slot,
    ready: impl Fn(&mut Workspace) + Clone + 'static,
) -> impl FnOnce(&mut Window, &mut App) -> AnyView + Clone + 'static {
    move |window: &mut Window, cx: &mut App| -> AnyView {
        cx.set_global(shot_appearance(theme, cx));
        cx.bind_keys(crate::key_bindings());
        let ws = cx.new(|cx| {
            let mut ws = Workspace::for_shots(window, cx);
            ws.sidebar.hint_dismissed = true;
            ws.right_sidebar.hidden = false;
            ws.right_sidebar.width = width;
            ws.right_sidebar.panel = panel;
            let src = root.join("src");
            ws.open_shot_session("api", &src, SHELL_TEXT, None, window, cx);
            ws.right_sidebar
                .files
                .show_for_shots(files_tree(&root, false));
            ready(&mut ws);
            ws
        });
        *slot.borrow_mut() = Some(ws.clone());
        ws.into()
    }
}

/// The right sidebar's panel tabs (M11h Task 4) on the Files panel's fixture, in a 1100 × 560
/// window, in both themes:
/// - `right-panels`: Files shown: its tab filled with the card (merging into the panel), the
///   Changes tab unfilled on the strip's `tab_bar`, the hovered Activity tab `tab_hover`; the
///   tabs 6 pt from the strip's left, 2 pt apart, 4 pt under its top and flush with its
///   bottom, in Files · Changes · Activity order; no border under the strip; its empty part
///   `tab_bar`. Then (not shot) the filter box focused and a click on Changes: Changes shown,
///   saved as `changes`, the terminal has the keyboard back (not the Files panel), and now the
///   Changes tab is the filled one.
/// - `right-panels-narrow`: the sidebar at its narrowest (220 pt): the three tabs fit, the
///   Activity tab's right edge at least 6 pt inside the card's.
fn right_panel_scenes() {
    use crate::right_sidebar::{FILTER_BAND, HEADER, Panel};
    use crate::ui_tokens::size::{FOCUS_RING, TAB_BAR, TAB_TOP_GAP};
    use gpui::{Bounds, Focusable, Modifiers};
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    let dims = size(px(1100.0), px(560.0));
    let window_right = 1100.0_f32;
    let card_right = window_right - GAP;
    let card_top = GAP + FOCUS_RING;
    let root = files_fixture();
    type Tabs = Rc<Cell<[Option<Bounds<Pixels>>; 3]>>;
    let tabs_of = |cx: &mut VisualTestAppContext, ws: &gpui::Entity<Workspace>| {
        let all = cx.read(|cx| ws.read(cx).right_sidebar.panel_tab_bounds());
        all.map(|b| b.expect("each tab laid out"))
    };
    let frame = |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
        cx.update_window(window, |_, window, _| window.refresh())
            .expect("refresh");
        cx.run_until_parked();
    };
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let ui = theme.ui();
        let near = |p: [u8; 3], c: alacritty_terminal::vte::ansi::Rgb| {
            p.iter()
                .zip([c.r, c.g, c.b])
                .all(|(a, b)| a.abs_diff(b) <= 3)
        };
        assert_ne!(ui.tab_bar, ui.card, "{slug}: told apart");
        assert_ne!(ui.tab_hover, ui.tab_bar, "{slug}: told apart");

        for scene in ["right-panels", "right-panels-narrow"] {
            let narrow = scene == "right-panels-narrow";
            let width = if narrow { 220.0 } else { 300.0 };
            let sidebar_left = card_right - width + FOCUS_RING;
            let n = format!("{scene}-{slug}");
            let slot: Slot = Rc::new(RefCell::new(None));
            let build = right_panel_workspace(
                theme,
                root.clone(),
                width,
                Panel::Files,
                slot.clone(),
                |_| (),
            );
            let seen: Tabs = Rc::default();
            let act = {
                let (n, seen) = (n.clone(), seen.clone());
                move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                    let ws = slotted(&slot);
                    frame(cx, window);
                    let [files, changes, activity] = tabs_of(cx, &ws);
                    let (l, r, t, b) = (
                        |b: Bounds<Pixels>| f32::from(b.left()),
                        |b: Bounds<Pixels>| f32::from(b.right()),
                        |b: Bounds<Pixels>| f32::from(b.top()),
                        |b: Bounds<Pixels>| f32::from(b.bottom()),
                    );
                    let close = |a: f32, b: f32| (a - b).abs() < 0.5;
                    assert!(
                        close(l(files), sidebar_left + 6.0),
                        "{n}: Files 6 pt from the strip's left: {files:?}"
                    );
                    assert!(close(l(changes), r(files) + 2.0), "{n}: 2 pt apart");
                    assert!(close(l(activity), r(changes) + 2.0), "{n}: 2 pt apart");
                    for tab in [files, changes, activity] {
                        assert!(close(t(tab), card_top + TAB_TOP_GAP), "{n}: 4 pt under");
                        assert!(close(b(tab), card_top + TAB_BAR), "{n}: flush: {tab:?}");
                    }
                    assert!(
                        r(activity) <= card_right - 6.0,
                        "{n}: Activity ends at {}, past the card's right − 6",
                        r(activity)
                    );
                    seen.set([Some(files), Some(changes), Some(activity)]);
                    // The pointer on the Activity tab: its hover fill in the shot.
                    let on = point(px(l(activity) + 4.0), px(card_top + TAB_BAR - 4.0));
                    cx.simulate_mouse_move(window, on, None, Modifiers::none());
                    frame(cx, window);
                    slot.borrow_mut().take();
                }
            };
            let path = shoot_with(scene, slug, &theme, dims, build.clone(), act.clone());
            println!("ui-shots: {}", path.display());
            let shot = render_with(&theme, dims, build, act);
            let at = |x: f32, y: f32| pixel_at(&shot, dims, x, y);
            let [files, changes, activity] = seen.get().map(|b| b.expect("seen"));
            // Inside each tab's left padding (no words there), low in it.
            let pad = |b: Bounds<Pixels>| at(f32::from(b.left()) + 4.0, card_top + TAB_BAR - 6.0);
            assert!(
                near(pad(files), ui.card),
                "{n}: Files is shown: {:?}",
                pad(files)
            );
            assert!(
                near(pad(changes), ui.tab_bar),
                "{n}: Changes isn't: {:?}",
                pad(changes)
            );
            assert!(
                near(pad(activity), ui.tab_hover),
                "{n}: Activity hovered: {:?}",
                pad(activity)
            );
            // The strip: above the tabs and its empty part in `tab_bar`; no border under it
            // (its last row `tab_bar` beside the tabs, the card right under it).
            let empty = card_right - 10.0;
            let x_files = f32::from(files.left()) + 4.0;
            for (what, x, y) in [
                ("above the tabs", x_files, card_top + 2.0),
                ("its empty part", empty, card_top + TAB_BAR / 2.0),
                ("its last row", empty, card_top + TAB_BAR - 0.5),
            ] {
                let p = at(x, y);
                assert!(near(p, ui.tab_bar), "{n}: the strip {what}: {p:?}");
            }
            let p = at(empty, card_top + TAB_BAR + 0.5);
            assert!(near(p, ui.card), "{n}: the card right under it: {p:?}");
            let p = at(x_files, card_top + TAB_BAR - 0.5);
            assert!(near(p, ui.card), "{n}: Files merges into the panel: {p:?}");
        }

        // The filter box focused, a click on Changes (not shot).
        let n = format!("right-panels-click-{slug}");
        let sidebar_left = card_right - 300.0 + FOCUS_RING;
        let slot: Slot = Rc::new(RefCell::new(None));
        let build = right_panel_workspace(
            theme,
            root.clone(),
            300.0,
            Panel::Files,
            slot.clone(),
            |_| (),
        );
        let seen: Tabs = Rc::default();
        let act = {
            let (n, seen) = (n.clone(), seen.clone());
            move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let ws = slotted(&slot);
                frame(cx, window);
                let field = point(
                    px(sidebar_left + 60.0),
                    px(card_top + TAB_BAR + HEADER + FILTER_BAND / 2.0),
                );
                cx.simulate_click(window, field, Modifiers::none());
                let keys = |cx: &mut VisualTestAppContext| {
                    cx.update_window(window, |_, window, cx| {
                        let ws = ws.read(cx);
                        let files = ws.right_sidebar.files.focus.clone().expect("its focus");
                        let terminal = ws.sessions.active().expect("the terminal");
                        (
                            files.contains_focused(window, cx),
                            terminal.view.focus_handle(cx).is_focused(window),
                        )
                    })
                    .expect("read")
                };
                assert_eq!(
                    keys(cx),
                    (true, false),
                    "{n}: the filter box has the keyboard"
                );
                let [_, changes, _] = tabs_of(cx, &ws);
                cx.simulate_click(window, changes.center(), Modifiers::none());
                frame(cx, window);
                cx.read(|cx| {
                    let ws = ws.read(cx);
                    assert_eq!(ws.right_sidebar.panel, Panel::Changes, "{n}: shown");
                    assert_eq!(
                        ws.saved_state().right_sidebar_panel.as_deref(),
                        Some("changes"),
                        "{n}: saved"
                    );
                });
                assert_eq!(
                    keys(cx),
                    (false, true),
                    "{n}: the terminal has the keyboard back"
                );
                seen.set(tabs_of(cx, &ws).map(Some));
                slot.borrow_mut().take();
            }
        };
        let shot = render_with(&theme, dims, build, act);
        let [files, changes, _] = seen.get().map(|b| b.expect("seen"));
        let pad = |b: Bounds<Pixels>| {
            pixel_at(
                &shot,
                dims,
                f32::from(b.left()) + 4.0,
                card_top + TAB_BAR - 6.0,
            )
        };
        assert!(near(pad(changes), ui.card), "{n}: Changes is shown now");
        assert!(near(pad(files), ui.tab_bar), "{n}: Files isn't");
        println!("ui-shots: {n} ok");
    }
}

/// M11ac: the Files panel follows the focused pane's file tab, on `files_fixture` beside a
/// terminal in its src/ (util closed), 1100 × 560, both themes:
/// - `files-panel-follows-active-tab` (each follow run where the focused tab changed, none by
///   the frame's safety net): after opening src/util/a.rs (util opens, a.rs
///   selected), ⌃Tab to the terminal (kept), a file outside the folder (kept), docs/guide.md
///   (docs opens), README.md in Diff mode, ⌃⇧Tab back through them by keyboard (each one
///   selected in turn): docs/guide.md's file tab shown and its row selected.
fn files_follow_scenes() {
    use crate::layout_model::FileMode;
    use std::cell::RefCell;
    use std::path::Path;
    use std::rc::Rc;

    let dims = size(px(1100.0), px(560.0));
    let root = files_fixture();
    let outside = std::env::temp_dir().join("fuxx-ui-shots-outside.md");
    std::fs::write(&outside, "# elsewhere\n").expect("the outside file");
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let slot: Slot = Rc::new(RefCell::new(None));
        let build = {
            let root = root.clone();
            let slot = slot.clone();
            move |window: &mut Window, cx: &mut App| -> AnyView {
                cx.set_global(shot_appearance(theme, cx));
                cx.bind_keys(crate::key_bindings());
                let ws = cx.new(|cx| {
                    let mut ws = Workspace::for_shots(window, cx);
                    ws.sidebar.hint_dismissed = true;
                    ws.right_sidebar.hidden = false;
                    ws.open_shot_session("api", &root.join("src"), SHELL_TEXT, None, window, cx);
                    ws.right_sidebar
                        .files
                        .show_for_shots(files_tree(&root, false));
                    ws
                });
                *slot.borrow_mut() = Some(ws.clone());
                ws.into()
            }
        };
        let act = {
            let root = root.clone();
            let outside = outside.clone();
            move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let n = format!("files-panel-follows-active-tab-{slug}");
                let ws = slotted(&slot);
                let frame = |cx: &mut VisualTestAppContext| {
                    for _ in 0..2 {
                        cx.update_window(window, |_, window, _| window.refresh())
                            .expect("refresh");
                        cx.run_until_parked();
                    }
                };
                let selected = |cx: &mut VisualTestAppContext| {
                    cx.read(|cx| {
                        let files = &ws.read(cx).right_sidebar.files;
                        files
                            .tree
                            .as_ref()
                            .and_then(|t| t.selected().map(Path::to_path_buf))
                    })
                };
                let shows = |cx: &mut VisualTestAppContext, rel: &str| {
                    cx.read(|cx| {
                        let files = &ws.read(cx).right_sidebar.files;
                        files.rows.iter().any(|r| r.rel == Path::new(rel))
                    })
                };
                let open = |cx: &mut VisualTestAppContext, path: PathBuf, mode| {
                    cx.update_window(window, |_, window, cx| {
                        ws.update(cx, |ws, cx| ws.open_file_tab(path, mode, window, cx))
                    })
                    .expect("open");
                };
                frame(cx);
                assert_eq!(
                    selected(cx),
                    Some(PathBuf::from("src")),
                    "{n}: the terminal's"
                );
                assert!(!shows(cx, "src/util/a.rs"), "{n}: util closed");
                open(cx, root.join("src/util/a.rs"), FileMode::Plain);
                frame(cx);
                assert_eq!(selected(cx), Some(PathBuf::from("src/util/a.rs")), "{n}");
                assert!(shows(cx, "src/util/a.rs"), "{n}: util opened and listed");
                cx.simulate_keystrokes(window, "ctrl-tab");
                frame(cx);
                assert!(
                    cx.read(|cx| ws.read(cx).focused_file_tab().is_none()),
                    "{n}"
                );
                assert_eq!(
                    selected(cx),
                    Some(PathBuf::from("src/util/a.rs")),
                    "{n}: kept"
                );
                open(cx, outside.clone(), FileMode::Plain);
                frame(cx);
                assert_eq!(
                    selected(cx),
                    Some(PathBuf::from("src/util/a.rs")),
                    "{n}: a file outside the folder changes nothing"
                );
                open(cx, root.join("docs/guide.md"), FileMode::Plain);
                frame(cx);
                assert_eq!(selected(cx), Some(PathBuf::from("docs/guide.md")), "{n}");
                assert!(shows(cx, "docs/guide.md"), "{n}: docs opened and listed");
                open(cx, root.join("README.md"), FileMode::Diff);
                frame(cx);
                assert_eq!(
                    selected(cx),
                    Some(PathBuf::from("README.md")),
                    "{n}: Diff too"
                );
                // By keyboard: README → guide → outside (kept) → a.rs.
                cx.simulate_keystrokes(window, "ctrl-shift-tab");
                frame(cx);
                assert_eq!(
                    selected(cx),
                    Some(PathBuf::from("docs/guide.md")),
                    "{n}: ⌃⇧Tab"
                );
                cx.simulate_keystrokes(window, "ctrl-shift-tab");
                frame(cx);
                assert_eq!(
                    selected(cx),
                    Some(PathBuf::from("docs/guide.md")),
                    "{n}: outside"
                );
                cx.simulate_keystrokes(window, "ctrl-shift-tab");
                frame(cx);
                assert_eq!(
                    selected(cx),
                    Some(PathBuf::from("src/util/a.rs")),
                    "{n}: a.rs"
                );
                cx.simulate_keystrokes(window, "ctrl-tab ctrl-tab");
                frame(cx);
                assert_eq!(
                    selected(cx),
                    Some(PathBuf::from("docs/guide.md")),
                    "{n}: back"
                );
                // Review fix: every follow ran where the focused tab changed, none from a frame.
                let late = cx.read(|cx| ws.read(cx).right_sidebar.files.late_follows.get());
                assert_eq!(late, 0, "{n}: follows started by the frame's safety net");
                slot.borrow_mut().take();
            }
        };
        let path = shoot_with(
            "files-panel-follows-active-tab",
            slug,
            &theme,
            dims,
            build,
            act,
        );
        println!("ui-shots: {}", path.display());
    }
}
