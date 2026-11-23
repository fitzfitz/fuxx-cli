//! Batch 3 (wave 2a): the shared right-click menu on a terminal and a file tab (chore 4), links
//! opening where "Open links in" says (chore 6), and the Files tree's menu (chore 7).

use super::*;
use crate::context_menu::{self, Entry};
use crate::layout_model::{FileMode, TabId, TabKind};

/// Show tab `tab` (as a click on it does).
fn select(
    cx: &mut VisualTestAppContext,
    window: AnyWindowHandle,
    ws: &Entity<Workspace>,
    tab: TabId,
) {
    cx.update_window(window, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.select_tab(tab, window, cx))
    })
    .expect("select");
}
use gpui::{Entity, Focusable, Modifiers, MouseButton, Point};
use std::path::{Path, PathBuf};

/// This file, as `file!()` gives it (repo-relative under cargo): `registry::AREAS` pairs it with
/// `SCENES`, so `--changed` runs these scenes when this file changes.
pub(super) const FILE: &str = file!();

/// This area's scenes (`registry.rs` runs them in its `ORDER`).
pub(super) const SCENES: &[Scene] = &[
    Scene {
        name: "context-menu-terminal",
        run: context_menu_terminal,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/context_menu.rs",
            "src/workspace_menu.rs",
            "src/terminal_view.rs",
            "src/terminal.rs",
            "src/mouse.rs",
            "src/overlay_panel.rs",
        ],
    },
    Scene {
        name: "context-menu-file-tab",
        run: context_menu_file_tab,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: &[
            "src/context_menu.rs",
            "src/workspace_menu.rs",
            "src/file_tab.rs",
            "src/workspace_agent.rs",
        ],
    },
    Scene {
        name: "link-opens-split",
        run: link_opens_split,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: &[
            "src/link_route.rs",
            "src/links.rs",
            "src/terminal_view.rs",
            "src/workspace_browser.rs",
            "src/tab_bar.rs",
            "src/file_tab.rs",
        ],
    },
    Scene {
        name: "files-menu",
        run: files_menu,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/files_menu.rs",
            "src/context_menu.rs",
            "src/workspace_menu.rs",
            "src/right_sidebar.rs",
            "src/file_tree.rs",
            "src/name_input.rs",
        ],
    },
];

const DIMS: (f32, f32) = (1100.0, 600.0);

fn dims() -> Size<Pixels> {
    size(px(DIMS.0), px(DIMS.1))
}

/// A few frames, so focus changes, menus and work off the UI thread land.
fn frames(cx: &mut VisualTestAppContext, window: AnyWindowHandle) {
    for _ in 0..3 {
        cx.update_window(window, |_, window, cx| window.draw(cx).clear())
            .expect("draw");
        cx.run_until_parked();
    }
}

/// The open menu's labels (separators as "-"), if one is open.
fn menu_labels(cx: &mut VisualTestAppContext, ws: &Entity<Workspace>) -> Option<Vec<String>> {
    cx.read(|cx| {
        let menu = ws.read(cx).context_menu.as_ref()?;
        Some(labels(&menu.entries))
    })
}

fn labels(entries: &[Entry]) -> Vec<String> {
    entries
        .iter()
        .map(|e| match e {
            Entry::Item(i) => i.label.clone(),
            Entry::Separator => "-".to_string(),
        })
        .collect()
}

/// The middle of the open menu's item `label`, as it is drawn.
fn item_at(cx: &mut VisualTestAppContext, ws: &Entity<Workspace>, label: &str) -> Point<Pixels> {
    cx.read(|cx| {
        let menu = ws.read(cx).context_menu.as_ref().expect("a menu");
        let mut y = f32::from(menu.at.y) + 1.0 + context_menu::PAD;
        for e in &menu.entries {
            match e {
                Entry::Item(i) if i.label == label => {
                    return point(menu.at.x + px(40.0), px(y + context_menu::ITEM / 2.0));
                }
                Entry::Item(_) => y += context_menu::ITEM,
                Entry::Separator => y += context_menu::SEPARATOR,
            }
        }
        panic!("no {label} in the menu")
    })
}

/// Choose `label` in the open menu with a click.
fn choose(
    cx: &mut VisualTestAppContext,
    window: AnyWindowHandle,
    ws: &Entity<Workspace>,
    label: &str,
) {
    let at = item_at(cx, ws, label);
    cx.simulate_click(window, at, Modifiers::none());
    frames(cx, window);
}

/// The first terminal's view, and where its cell (`col`, `row`) was drawn.
fn terminal(
    cx: &mut VisualTestAppContext,
    ws: &Entity<Workspace>,
) -> Entity<crate::terminal_view::TerminalView> {
    cx.read(|cx| ws.read(cx).sessions.items()[0].view.clone())
}

fn cell(
    cx: &mut VisualTestAppContext,
    ws: &Entity<Workspace>,
    col: usize,
    row: usize,
) -> gpui::Bounds<Pixels> {
    let view = terminal(cx, ws);
    cx.read(|cx| view.read(cx).cell_bounds(col, row).expect("the grid drawn"))
}

/// A workspace with one terminal, `api`, showing `text`, and the real keys.
fn one_terminal(
    theme: Theme,
    text: String,
    slot: Slot,
) -> impl FnOnce(&mut Window, &mut App) -> AnyView + Clone + 'static {
    move |window: &mut Window, cx: &mut App| -> AnyView {
        cx.set_global(shot_appearance(theme, cx));
        cx.bind_keys(crate::key_bindings());
        let ws = cx.new(|cx| {
            let mut ws = Workspace::for_shots(window, cx);
            ws.sidebar.hint_dismissed = true;
            let project = PathBuf::from("/Users/me/code/my-app");
            ws.open_shot_session("api", &project, &text, None, window, cx);
            ws
        });
        *slot.borrow_mut() = Some(ws.clone());
        ws.into()
    }
}

const TERMINAL_TEXT: &str =
    "$ cargo run\r\nsee https://example.com/docs for help\r\nmail mailto:team@example.com\r\n";

/// `context-menu-terminal` (both themes): a selection kept by a right-click on a link, which
/// opens the menu with the link's items, Copy, Add to Claude Chat, Paste and Select All, the
/// link underlined while it is open; ↓ lights the first item, Esc closes it and the terminal
/// has the keys again; ⇧F10 opens it at the caret (no link); ⌃-click opens it; a right-click
/// elsewhere opens it there; Copy, Copy Link and Select All do what they say. Shot: the menu
/// open on the link. Then (first theme, not shot) Add to Claude Chat makes a Claude tab with
/// the selection as a quote chip.
fn context_menu_terminal() {
    use std::cell::RefCell;
    use std::rc::Rc;
    for (k, &(slug, name)) in shot_themes().iter().enumerate() {
        let theme = shot_theme(name);
        let ui = theme.ui();
        let n = format!("context-menu-terminal-{slug}");
        let slot: Slot = Rc::new(RefCell::new(None));
        let build = one_terminal(theme, TERMINAL_TEXT.to_string(), slot.clone());
        let at_menu: Rc<RefCell<Option<Point<Pixels>>>> = Rc::default();
        let seen = at_menu.clone();
        let act = {
            let n = n.clone();
            move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let ws = slotted(&slot);
                frames(cx, window);
                let none = Modifiers::none();
                let select_row0 = |cx: &mut VisualTestAppContext| {
                    let (from, to) = (cell(cx, &ws, 0, 0), cell(cx, &ws, 10, 0));
                    let to = point(to.right() - px(1.0), to.center().y);
                    cx.simulate_mouse_down(window, from.center(), MouseButton::Left, none);
                    cx.simulate_mouse_move(window, to, Some(MouseButton::Left), none);
                    cx.simulate_mouse_up(window, to, MouseButton::Left, none);
                    frames(cx, window);
                };
                select_row0(cx);
                let view = terminal(cx, &ws);
                let selected =
                    |cx: &mut VisualTestAppContext| cx.read(|cx| view.read(cx).selection_text());
                assert_eq!(selected(cx).as_deref(), Some("$ cargo run"), "{n}");
                let url = cell(cx, &ws, 8, 1).center();
                let link_menu = labels(&context_menu::terminal_menu(
                    true,
                    Some("https://example.com/docs"),
                    Some("Claude"),
                ));
                cx.simulate_mouse_down(window, url, MouseButton::Right, none);
                frames(cx, window);
                assert_eq!(
                    menu_labels(cx, &ws),
                    Some(link_menu.clone()),
                    "{n}: the link's menu"
                );
                assert_eq!(selected(cx).as_deref(), Some("$ cargo run"), "{n}: kept");
                let underlined = cx.read(|cx| ws.read(cx).menu_underline);
                let api = cx.read(|cx| ws.read(cx).sessions.items()[0].id);
                assert_eq!(underlined, Some(api), "{n}: its link underlined");
                // ↓ lights the first item; Esc closes it, the terminal has the keys again.
                assert_eq!(
                    cx.read(|cx| ws.read(cx).context_menu.as_ref().unwrap().lit),
                    None
                );
                press_keys(cx, window, "down");
                assert_eq!(
                    cx.read(|cx| ws.read(cx).context_menu.as_ref().unwrap().lit),
                    Some(0),
                    "{n}: ↓ lights Open Link"
                );
                press_keys(cx, window, "escape");
                assert_eq!(menu_labels(cx, &ws), None, "{n}: Esc closes it");
                assert_eq!(
                    cx.read(|cx| ws.read(cx).menu_underline),
                    None,
                    "{n}: not underlined"
                );
                let keys = cx
                    .update_window(window, |_, window, cx| {
                        view.read(cx).focus_handle(cx).is_focused(window)
                    })
                    .expect("read");
                assert!(keys, "{n}: the terminal has the keys back");
                // ⇧F10: at the caret, no link.
                press_keys(cx, window, "shift-f10");
                assert_eq!(
                    menu_labels(cx, &ws),
                    Some(labels(&context_menu::terminal_menu(
                        true,
                        None,
                        Some("Claude")
                    ))),
                    "{n}: ⇧F10"
                );
                press_keys(cx, window, "escape");
                // ⌃-click on the mail link.
                let mail = cell(cx, &ws, 8, 2).center();
                cx.simulate_mouse_down(window, mail, MouseButton::Left, Modifiers::control());
                frames(cx, window);
                let mail_menu = labels(&context_menu::terminal_menu(
                    true,
                    Some("mailto:team@example.com"),
                    Some("Claude"),
                ));
                assert_eq!(menu_labels(cx, &ws), Some(mail_menu), "{n}: ⌃-click");
                // Copy Link, then Copy.
                choose(cx, window, &ws, "Copy Link");
                let clip = cx.read_from_clipboard().and_then(|c| c.text());
                assert_eq!(clip.as_deref(), Some("mailto:team@example.com"), "{n}");
                cx.simulate_mouse_down(window, url, MouseButton::Right, none);
                frames(cx, window);
                choose(cx, window, &ws, "Copy");
                let clip = cx.read_from_clipboard().and_then(|c| c.text());
                assert_eq!(clip.as_deref(), Some("$ cargo run"), "{n}: Copy");
                // Select All.
                cx.simulate_mouse_down(window, url, MouseButton::Right, none);
                frames(cx, window);
                choose(cx, window, &ws, "Select All");
                let all = selected(cx).unwrap_or_default();
                assert!(
                    all.contains("$ cargo run") && all.contains("mailto:"),
                    "{n}: {all:?}"
                );
                // A right-click elsewhere while it's open: it opens there.
                select_row0(cx);
                cx.simulate_mouse_down(window, url, MouseButton::Right, none);
                frames(cx, window);
                let row0 = cell(cx, &ws, 4, 0).center();
                cx.simulate_mouse_down(window, row0, MouseButton::Right, none);
                // The app's frame loop: each frame runs its callbacks, then draws. The press
                // goes again two frames on, once a drawing without the backdrop is there.
                for _ in 0..2 {
                    cx.update_window(window, |_, window, cx| {
                        window.simulate_next_frame(cx);
                        window.draw(cx).clear();
                    })
                    .expect("a frame");
                    cx.run_until_parked();
                }
                frames(cx, window);
                assert_eq!(
                    menu_labels(cx, &ws),
                    Some(labels(&context_menu::terminal_menu(
                        true,
                        None,
                        Some("Claude")
                    ))),
                    "{n}: opened again where right-clicked (no link there)"
                );
                // The shot: open on the link.
                press_keys(cx, window, "escape");
                cx.simulate_mouse_down(window, url, MouseButton::Right, none);
                frames(cx, window);
                assert_eq!(
                    menu_labels(cx, &ws),
                    Some(link_menu),
                    "{n}: open for the shot"
                );
                *seen.borrow_mut() = cx.read(|cx| ws.read(cx).context_menu.as_ref().map(|m| m.at));
                slot.borrow_mut().take();
            }
        };
        let shot = render_with(&theme, dims(), build.clone(), act.clone());
        let at = at_menu.borrow().expect("the menu's place");
        // The panel's fill just inside its corner padding, below the pointer and to its right.
        let inside = pixel_at(
            &shot,
            dims(),
            f32::from(at.x) + 200.0,
            f32::from(at.y) + 2.5,
        );
        assert!(
            near_by(inside, ui.panel, 3),
            "{n}: the panel's fill: {inside:?}"
        );
        save_shot("context-menu-terminal", slug, &shot);
        if k > 0 {
            continue;
        }
        // Add to Claude Chat: a new Claude tab (no agent tab yet) with the quote chip.
        let slot: Slot = Rc::new(RefCell::new(None));
        let build = one_terminal(theme, TERMINAL_TEXT.to_string(), slot.clone());
        let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
            let ws = slotted(&slot);
            frames(cx, window);
            let none = Modifiers::none();
            let (from, to) = (cell(cx, &ws, 0, 0), cell(cx, &ws, 10, 0));
            let to = point(to.right() - px(1.0), to.center().y);
            cx.simulate_mouse_down(window, from.center(), MouseButton::Left, none);
            cx.simulate_mouse_move(window, to, Some(MouseButton::Left), none);
            cx.simulate_mouse_up(window, to, MouseButton::Left, none);
            frames(cx, window);
            cx.simulate_mouse_down(window, from.center(), MouseButton::Right, none);
            frames(cx, window);
            choose(cx, window, &ws, "Add to Claude Chat");
            frames(cx, window);
            let (tab, label) = cx.read(|cx| {
                let ws = ws.read(cx);
                let tab = ws.model.focused_tab().expect("a tab");
                let chips = ws.agents.get(&tab).map(|slot| {
                    slot.view
                        .read(cx)
                        .shot_attachments()
                        .iter()
                        .map(|a| (a.label(), a.tip()))
                        .collect::<Vec<_>>()
                });
                (tab, chips)
            });
            assert_eq!(
                label,
                Some(vec![(
                    "$ cargo run".to_string(),
                    Some("Quoted from Terminal api".to_string())
                )]),
                "context-menu-terminal: the quote chip in the new Claude tab ({tab:?})"
            );
            slot.borrow_mut().take();
        };
        render_with(&theme, dims(), build, act);
        println!("ui-shots: context-menu-terminal add-to-chat ok");
    }
}

/// `context-menu-file-tab`: a file tab's right-click with nothing selected offers Copy
/// (disabled) only; with a selection, Copy and Add to Claude Chat; ⇧F10 opens it too; Copy
/// copies the lines; Add to Claude Chat puts a code chip in a new Claude tab.
fn context_menu_file_tab() {
    use std::cell::RefCell;
    use std::rc::Rc;
    let dir = crate::test_temp::dir("fuxx-ui-shots-menu-file");
    std::fs::create_dir_all(&dir).expect("the folder");
    let path = dir.join("lib.rs");
    std::fs::write(&path, "fn one() {}\nfn two() {}\nfn three() {}\n").expect("the file");
    let theme = shot_theme(shot_themes()[0].1);
    let slot: Slot = Rc::new(RefCell::new(None));
    let build = {
        let (slot, path, dir) = (slot.clone(), path.clone(), dir.clone());
        move |window: &mut Window, cx: &mut App| -> AnyView {
            cx.set_global(shot_appearance(theme, cx));
            cx.bind_keys(crate::key_bindings());
            let ws = cx.new(|cx| {
                let mut ws = Workspace::for_shots(window, cx);
                ws.sidebar.hint_dismissed = true;
                ws.open_shot_session("api", &dir, "", None, window, cx);
                ws.open_file_tab(path.clone(), FileMode::Plain, window, cx);
                ws
            });
            *slot.borrow_mut() = Some(ws.clone());
            ws.into()
        }
    };
    let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
        let n = "context-menu-file-tab";
        let ws = slotted(&slot);
        frames(cx, window);
        let (file, view) = cx.read(|cx| {
            let ws = ws.read(cx);
            let tab = ws.model.focused_tab().expect("the file tab");
            (tab, ws.file_tabs[&tab].clone())
        });
        let middle = cx.read(|cx| {
            let r = ws.read(cx).pane_area_rect().expect("drawn");
            point(px(r.x + r.w / 2.0), px(r.y + r.h / 2.0))
        });
        cx.simulate_mouse_down(window, middle, MouseButton::Right, Modifiers::none());
        frames(cx, window);
        assert_eq!(
            menu_labels(cx, &ws),
            Some(vec!["Copy".to_string()]),
            "{n}: nothing selected"
        );
        let copy_on = cx.read(|cx| {
            let menu = ws.read(cx).context_menu.as_ref().expect("a menu");
            matches!(&menu.entries[0], Entry::Item(i) if i.enabled)
        });
        assert!(!copy_on, "{n}: Copy disabled");
        press_keys(cx, window, "escape");
        cx.update(|cx| view.update(cx, |v, cx| v.select_for_shots((0, 0), (1, 11), cx)));
        frames(cx, window);
        cx.simulate_mouse_down(window, middle, MouseButton::Right, Modifiers::none());
        frames(cx, window);
        let both = vec!["Copy".to_string(), "Add to Claude Chat".to_string()];
        assert_eq!(
            menu_labels(cx, &ws),
            Some(both.clone()),
            "{n}: with a selection"
        );
        choose(cx, window, &ws, "Copy");
        let clip = cx.read_from_clipboard().and_then(|c| c.text());
        assert_eq!(
            clip.as_deref(),
            Some("fn one() {}\nfn two() {}"),
            "{n}: Copy"
        );
        // The keyboard: ⇧F10 in the file tab.
        press_keys(cx, window, "shift-f10");
        assert_eq!(menu_labels(cx, &ws), Some(both), "{n}: ⇧F10");
        choose(cx, window, &ws, "Add to Claude Chat");
        frames(cx, window);
        let chips = cx.read(|cx| {
            let ws = ws.read(cx);
            let tab = ws.model.focused_tab().expect("a tab");
            assert_ne!(tab, file, "{n}: the Claude tab shown");
            ws.agents.get(&tab).map(|slot| {
                slot.view
                    .read(cx)
                    .shot_attachments()
                    .iter()
                    .map(|a| a.label())
                    .collect::<Vec<_>>()
            })
        });
        assert_eq!(
            chips,
            Some(vec!["lib.rs:1\u{2013}2".to_string()]),
            "{n}: the code chip"
        );
        slot.borrow_mut().take();
    };
    render_with(&theme, dims(), build, act);
    println!("ui-shots: context-menu-file-tab ok");
}

/// `link-opens-split`: ⌘-clicked links in a terminal. The first splits a browser pane to the
/// right (your focus stays on the terminal); a second is a new tab in that pane; the first
/// again shows its tab and flashes its pill; mail goes to the OS; a path in the output opens a
/// file tab at its line; "A new tab in this pane" opens one in the terminal's pane; "Your
/// default browser" hands it to the OS.
fn link_opens_split() {
    use crate::appearance::{Appearance, OpenLinksIn};
    use std::cell::RefCell;
    use std::rc::Rc;
    let dir = crate::test_temp::dir("fuxx-ui-shots-links");
    std::fs::create_dir_all(&dir).expect("the folder");
    let file = std::fs::canonicalize(&dir).unwrap_or(dir).join("main.rs");
    let body: String = (1..=60).map(|i| format!("// line {i}\n")).collect();
    std::fs::write(&file, body).expect("the file");
    let text = format!(
        "$ cargo run\r\n   Compiling my-app\r\n    Finished\r\nsee https://example.com/docs now\r\nmail mailto:team@example.com\r\nalso https://example.org/b\r\nthen https://example.net/c\r\nerror at {}:42:1\r\n",
        file.display()
    );
    let theme = shot_theme(shot_themes()[0].1);
    let slot: Slot = Rc::new(RefCell::new(None));
    let build = one_terminal(theme, text, slot.clone());
    let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
        let n = "link-opens-split";
        let ws = slotted(&slot);
        frames(cx, window);
        let api_tab = cx.read(|cx| ws.read(cx).model.focused_tab().expect("api"));
        // A ⌘-click on `needle` where it is on the screen now (a split reflows the text; the
        // long path, which wraps, is the last line, so the rows before it are the screen's).
        let click = |cx: &mut VisualTestAppContext, needle: &str| {
            let view = terminal(cx, &ws);
            let screen = cx.read(|cx| view.read(cx).text(crate::grid::TextScope::Screen));
            let (row, col) = screen
                .lines()
                .enumerate()
                .find_map(|(r, line)| line.find(needle).map(|c| (r, c + 2)))
                .unwrap_or_else(|| panic!("{needle} on the screen: {screen:?}"));
            let at = cell(cx, &ws, col, row).center();
            let cmd = Modifiers::command();
            cx.simulate_mouse_move(window, at, None, cmd);
            cx.simulate_mouse_down(window, at, MouseButton::Left, cmd);
            cx.simulate_mouse_up(window, at, MouseButton::Left, cmd);
            frames(cx, window);
            frames(cx, window);
        };
        let browsers = |cx: &mut VisualTestAppContext| {
            cx.read(|cx| {
                let ws = ws.read(cx);
                let id = ws.model.focused_workspace().expect("a workspace");
                ws.model
                    .workspace(id)
                    .expect("it")
                    .panes
                    .iter()
                    .map(|p| {
                        let urls: Vec<String> = p
                            .tabs
                            .iter()
                            .filter_map(|t| match &t.kind {
                                TabKind::Browser { url } => url.clone(),
                                _ => None,
                            })
                            .collect();
                        (p.id, urls)
                    })
                    .collect::<Vec<_>>()
            })
        };
        let focused = |cx: &mut VisualTestAppContext| cx.read(|cx| ws.read(cx).model.focused_tab());
        // The first link: a browser pane split on the right; the terminal keeps the focus.
        click(cx, "example.com/docs");
        let panes = browsers(cx);
        assert_eq!(panes.len(), 2, "{n}: split: {panes:?}");
        assert_eq!(
            panes[1].1,
            ["https://example.com/docs"],
            "{n}: the page on the right"
        );
        assert_eq!(focused(cx), Some(api_tab), "{n}: your focus stays");
        // A second: a new tab in that pane.
        click(cx, "example.org");
        let panes = browsers(cx);
        assert_eq!(panes.len(), 2, "{n}: no second split");
        assert_eq!(
            panes[1].1,
            ["https://example.com/docs", "https://example.org/b"],
            "{n}: a tab in the link pane: {panes:?}, outside {:?}",
            cx.read(|cx| ws.read(cx).shot_opened_outside.clone())
        );
        assert_eq!(focused(cx), Some(api_tab), "{n}: your focus stays");
        // The first again: its tab shown, its pill flashing; no third tab.
        click(cx, "example.com/docs");
        let panes = browsers(cx);
        assert_eq!(panes[1].1.len(), 2, "{n}: the same URL is not opened again");
        let (shown, flash) = cx.read(|cx| {
            let ws = ws.read(cx);
            let pane = ws.model.pane(panes[1].0).expect("the link pane");
            (pane.active, ws.link_flash.map(|(t, ..)| t))
        });
        let first = cx.read(|cx| ws.read(cx).model.pane(panes[1].0).unwrap().tabs[0].id);
        assert_eq!(shown, first, "{n}: its tab shown");
        assert_eq!(flash, Some(first), "{n}: its pill flashes");
        assert_eq!(focused(cx), Some(api_tab), "{n}: your focus stays");
        // Mail: to the OS, nothing opened here.
        click(cx, "team@");
        let outside = cx.read(|cx| ws.read(cx).shot_opened_outside.clone());
        assert_eq!(outside, ["mailto:team@example.com"], "{n}: mail to the OS");
        assert_eq!(browsers(cx)[1].1.len(), 2, "{n}: no tab for mail");
        // A path in the output: a file tab at its line, in the terminal's pane.
        click(cx, "/private");
        let (kind, line) = cx.read(|cx| {
            let ws = ws.read(cx);
            let tab = ws.model.focused_tab().expect("a tab");
            let kind = ws.model.tab(tab).map(|t| t.kind.clone());
            let line = ws
                .file_tabs
                .get(&tab)
                .and_then(|v| v.read(cx).current_line_for_shots());
            (kind, line)
        });
        assert!(
            matches!(&kind, Some(TabKind::File { path, .. }) if path == &file),
            "{n}: the file opened: {kind:?}"
        );
        assert_eq!(line, Some(41), "{n}: at line 42");
        // "A new tab in this pane": the next link opens beside the terminal, in its pane.
        select(cx, window, &ws, api_tab);
        cx.update(|cx| {
            cx.update_global::<Appearance, _>(|a, _| a.open_links_in = OpenLinksIn::ThisPane);
        });
        frames(cx, window);
        assert_eq!(focused(cx), Some(api_tab), "{n}: the terminal shown again");
        click(cx, "example.net");
        let panes = browsers(cx);
        assert_eq!(
            panes[0].1,
            ["https://example.net/c"],
            "{n}: in the terminal's pane: {panes:?}, focused {:?}, outside {:?}",
            focused(cx),
            cx.read(|cx| ws.read(cx).shot_opened_outside.clone())
        );
        assert_eq!(panes[1].1.len(), 2, "{n}: the link pane untouched");
        // "Your default browser": the OS.
        select(cx, window, &ws, api_tab);
        cx.update(|cx| {
            cx.update_global::<Appearance, _>(|a, _| a.open_links_in = OpenLinksIn::Browser);
        });
        frames(cx, window);
        click(cx, "example.org");
        let outside = cx.read(|cx| ws.read(cx).shot_opened_outside.clone());
        assert_eq!(
            outside,
            ["mailto:team@example.com", "https://example.org/b"],
            "{n}: to the default browser"
        );
        slot.borrow_mut().take();
    };
    render_with(&theme, dims(), build, act);
    println!("ui-shots: link-opens-split ok");
}

/// A small tree to change: src/ (main.rs, marked; lib.rs) and README.md, in a folder of its own.
fn files_menu_root(slug: &str) -> PathBuf {
    let root = crate::test_temp::dir(&format!("fuxx-ui-shots-files-menu-{slug}"));
    std::fs::create_dir_all(root.join("src")).expect("the folders");
    for file in ["src/main.rs", "src/lib.rs", "README.md"] {
        std::fs::write(root.join(file), "fn main() {}\n").expect("the files");
    }
    std::fs::canonicalize(&root).unwrap_or(root)
}

fn files_menu_tree(root: &Path) -> crate::file_tree::FileTree {
    use crate::file_tree::{CAP, FileTree};
    use crate::git_status::{Mark, list_dir};
    let mut tree = FileTree::new(root.to_path_buf(), Some(PathBuf::from("src")));
    for rel in tree.wanted() {
        let (entries, more) = list_dir(&root.join(&rel), CAP);
        tree.expand(rel, entries, more);
    }
    tree.set_marks(std::collections::HashMap::from([(
        PathBuf::from("src/main.rs"),
        Mark::Modified,
    )]));
    tree
}

/// The right-click point on Files row `rel`, as last drawn.
fn row_point(cx: &mut VisualTestAppContext, ws: &Entity<Workspace>, rel: &str) -> Point<Pixels> {
    cx.read(|cx| {
        let files = &ws.read(cx).right_sidebar.files;
        let i = files
            .rows
            .iter()
            .position(|r| r.more.is_none() && r.rel == Path::new(rel))
            .unwrap_or_else(|| panic!("no row {rel}"));
        let foot = files.row_place(i).expect("the row drawn");
        point(
            foot.x + px(20.0),
            foot.y - px(crate::right_sidebar::ROW / 2.0),
        )
    })
}

fn rows(cx: &mut VisualTestAppContext, ws: &Entity<Workspace>) -> Vec<String> {
    cx.read(|cx| {
        let files = &ws.read(cx).right_sidebar.files;
        files
            .rows
            .iter()
            .filter(|r| r.more.is_none())
            .map(|r| r.rel.display().to_string())
            .collect()
    })
}

/// `files-menu` (both themes; on a folder of its own, never the repo's or yours): a file row's
/// right-click selects it and opens its menu (Show Changes on the marked file), a folder's its
/// own; Copy Path and ⌥⇧⌘C copy the paths; ⇧F10 opens it on the selected row; Reveal in
/// Finder asks for the file; Open to the Side opens it in a pane on the right; Rename selects
/// the stem and renames on disk; a clash shows under the box; New File and New Folder make
/// them; Open in Terminal opens a terminal tab there; Move to Trash moves it to "the Trash" (a
/// folder of the scene's) and ⌘Z puts it back. Shot: the menu open on main.rs.
fn files_menu() {
    use std::cell::RefCell;
    use std::rc::Rc;
    for (k, &(slug, name)) in shot_themes().iter().enumerate() {
        let theme = shot_theme(name);
        let ui = theme.ui();
        let root = files_menu_root(slug);
        let trash = crate::test_temp::dir(&format!("fuxx-ui-shots-files-trash-{slug}"));
        std::fs::create_dir_all(&trash).expect("the trash");
        let slot: Slot = Rc::new(RefCell::new(None));
        let build = {
            let (root, slot, trash) = (root.clone(), slot.clone(), trash.clone());
            move |window: &mut Window, cx: &mut App| -> AnyView {
                // No status bar: it would say the scene's temporary folder, another each run.
                let mut appearance = shot_appearance(theme, cx);
                appearance.status_bar = false;
                cx.set_global(appearance);
                cx.bind_keys(crate::key_bindings());
                let ws = cx.new(|cx| {
                    let mut ws = Workspace::for_shots(window, cx);
                    ws.sidebar.hint_dismissed = true;
                    ws.right_sidebar.hidden = false;
                    ws.open_shot_session("api", &root.join("src"), "", None, window, cx);
                    ws.right_sidebar
                        .files
                        .show_for_shots(files_menu_tree(&root));
                    ws.right_sidebar.files.fake_trash = Some(trash.clone());
                    ws
                });
                *slot.borrow_mut() = Some(ws.clone());
                ws.into()
            }
        };
        let full = k == 0;
        let n = format!("files-menu-{slug}");
        let lit_danger: Rc<RefCell<Option<Point<Pixels>>>> = Rc::default();
        let seen = lit_danger.clone();
        let act = {
            let (root, trash, n) = (root.clone(), trash.clone(), n.clone());
            move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let ws = slotted(&slot);
                frames(cx, window);
                let right = |cx: &mut VisualTestAppContext, rel: &str| {
                    let at = row_point(cx, &ws, rel);
                    cx.simulate_mouse_down(window, at, MouseButton::Right, Modifiers::none());
                    frames(cx, window);
                };
                let selected = |cx: &mut VisualTestAppContext| {
                    cx.read(|cx| {
                        let files = &ws.read(cx).right_sidebar.files;
                        files
                            .tree
                            .as_ref()
                            .and_then(|t| t.selected())
                            .map(|p| p.display().to_string())
                    })
                };
                right(cx, "src/main.rs");
                assert_eq!(
                    selected(cx).as_deref(),
                    Some("src/main.rs"),
                    "{n}: selected"
                );
                assert_eq!(
                    menu_labels(cx, &ws),
                    Some(labels(&context_menu::file_tree_menu(false, None, true))),
                    "{n}: a marked file's menu"
                );
                if full {
                    choose(cx, window, &ws, "Copy Path");
                    let clip = cx.read_from_clipboard().and_then(|c| c.text());
                    let main = root.join("src/main.rs");
                    assert_eq!(
                        clip.as_deref(),
                        Some(main.to_str().unwrap()),
                        "{n}: Copy Path"
                    );
                    // The tree has the keys back: ⌥⇧⌘C copies the relative path.
                    press_keys(cx, window, "alt-shift-cmd-c");
                    let clip = cx.read_from_clipboard().and_then(|c| c.text());
                    assert_eq!(clip.as_deref(), Some("src/main.rs"), "{n}: ⌥⇧⌘C");
                    press_keys(cx, window, "shift-f10");
                    assert_eq!(
                        menu_labels(cx, &ws),
                        Some(labels(&context_menu::file_tree_menu(false, None, true))),
                        "{n}: ⇧F10 on the selected row"
                    );
                    choose(cx, window, &ws, "Reveal in Finder");
                    let revealed =
                        cx.read(|cx| ws.read(cx).right_sidebar.files.shot_revealed.clone());
                    assert_eq!(revealed, std::slice::from_ref(&main), "{n}: Reveal");
                    // A folder's menu.
                    right(cx, "src");
                    assert_eq!(
                        menu_labels(cx, &ws),
                        Some(labels(&context_menu::file_tree_menu(true, None, false))),
                        "{n}: a folder's menu"
                    );
                    press_keys(cx, window, "escape");
                    // Rename: the stem selected; "README" → "GUIDE".
                    right(cx, "README.md");
                    choose(cx, window, &ws, "Rename\u{2026}");
                    let boxed = cx.read(|cx| {
                        let files = &ws.read(cx).right_sidebar.files;
                        files
                            .edit
                            .as_ref()
                            .map(|e| e.input.read(cx).text().to_string())
                    });
                    assert_eq!(boxed.as_deref(), Some("README.md"), "{n}: the box");
                    cx.simulate_input(window, "GUIDE");
                    press_keys(cx, window, "enter");
                    frames(cx, window);
                    assert!(root.join("GUIDE.md").is_file(), "{n}: renamed on disk");
                    assert!(!root.join("README.md").exists(), "{n}: the old name gone");
                    assert!(
                        rows(cx, &ws).contains(&"GUIDE.md".to_string()),
                        "{n}: the row"
                    );
                    assert_eq!(selected(cx).as_deref(), Some("GUIDE.md"), "{n}: selected");
                    // A clash: lib.rs → main.rs says so under the box, and Esc leaves it be.
                    right(cx, "src/lib.rs");
                    choose(cx, window, &ws, "Rename\u{2026}");
                    press_keys(cx, window, "cmd-a");
                    cx.simulate_input(window, "main.rs");
                    press_keys(cx, window, "enter");
                    let problem = cx.read(|cx| {
                        let files = &ws.read(cx).right_sidebar.files;
                        files.edit.as_ref().and_then(|e| e.problem.clone())
                    });
                    assert_eq!(
                        problem.as_deref(),
                        Some("\u{201c}main.rs\u{201d} is already here."),
                        "{n}: the clash"
                    );
                    press_keys(cx, window, "escape");
                    assert!(
                        cx.read(|cx| ws.read(cx).right_sidebar.files.edit.is_none()),
                        "{n}: Esc ends it"
                    );
                    assert!(root.join("src/lib.rs").is_file(), "{n}: lib.rs as it was");
                    // New File and New Folder in src.
                    right(cx, "src");
                    choose(cx, window, &ws, "New File\u{2026}");
                    cx.simulate_input(window, "util.rs");
                    press_keys(cx, window, "enter");
                    frames(cx, window);
                    assert!(root.join("src/util.rs").is_file(), "{n}: the new file");
                    assert!(
                        rows(cx, &ws).contains(&"src/util.rs".to_string()),
                        "{n}: its row"
                    );
                    right(cx, "src");
                    choose(cx, window, &ws, "New Folder\u{2026}");
                    cx.simulate_input(window, "parts");
                    press_keys(cx, window, "enter");
                    frames(cx, window);
                    assert!(root.join("src/parts").is_dir(), "{n}: the new folder");
                    // Move to Trash (the sheet's Move to Trash), then ⌘Z.
                    cx.update(|cx| {
                        ws.update(cx, |ws, cx| ws.trash_now(PathBuf::from("src/util.rs"), cx))
                    });
                    frames(cx, window);
                    assert!(!root.join("src/util.rs").exists(), "{n}: trashed");
                    assert!(trash.join("util.rs").is_file(), "{n}: in the scene's Trash");
                    assert!(
                        !rows(cx, &ws).contains(&"src/util.rs".to_string()),
                        "{n}: row gone"
                    );
                    right(cx, "src/lib.rs");
                    press_keys(cx, window, "escape");
                    press_keys(cx, window, "cmd-z");
                    frames(cx, window);
                    assert!(root.join("src/util.rs").is_file(), "{n}: put back");
                    assert_eq!(
                        selected(cx).as_deref(),
                        Some("src/util.rs"),
                        "{n}: selected"
                    );
                    // Open in Terminal: a terminal tab more.
                    let before = cx.read(|cx| ws.read(cx).sessions.items().len());
                    right(cx, "src");
                    choose(cx, window, &ws, "Open in Terminal");
                    frames(cx, window);
                    let after = cx.read(|cx| ws.read(cx).sessions.items().len());
                    assert_eq!(after, before + 1, "{n}: a terminal tab in src");
                    // Open to the Side: main.rs in a pane on the right, shown and focused.
                    let api = cx.read(|cx| {
                        let ws = ws.read(cx);
                        ws.model
                            .terminal_tab(ws.sessions.items()[0].id)
                            .expect("api")
                    });
                    select(cx, window, &ws, api);
                    frames(cx, window);
                    right(cx, "src/main.rs");
                    choose(cx, window, &ws, "Open to the Side");
                    let side = cx.read(|cx| {
                        let ws = ws.read(cx);
                        let id = ws.model.focused_workspace().expect("a workspace");
                        let panes = ws.model.workspace(id).expect("it").panes.len();
                        let tab = ws
                            .model
                            .focused_tab()
                            .and_then(|t| ws.model.tab(t))
                            .map(|t| t.kind.clone());
                        (panes, tab)
                    });
                    assert_eq!(side.0, 2, "{n}: a pane on the right");
                    assert!(
                        matches!(&side.1, Some(TabKind::File { path, .. }) if path == &main),
                        "{n}: main.rs in it: {:?}",
                        side.1
                    );
                }
                // The shot: main.rs's menu open, Move to Trash lit.
                right(cx, "src/main.rs");
                let danger = item_at(cx, &ws, "Move to Trash");
                cx.simulate_mouse_move(window, danger, None, Modifiers::none());
                frames(cx, window);
                *seen.borrow_mut() = Some(danger);
                slot.borrow_mut().take();
            }
        };
        let shot = render_with(&theme, dims(), build.clone(), act.clone());
        let at = lit_danger.borrow().expect("the item");
        // Between the label and its keys (the item's middle is 40 in from the menu's left).
        let lit = pixel_at(&shot, dims(), f32::from(at.x) + 110.0, f32::from(at.y));
        assert!(
            near_by(lit, ui.panel_hover, 3),
            "{n}: lit in panel_hover: {lit:?}"
        );
        save_shot("files-menu", slug, &shot);
    }
}
