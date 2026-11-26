//! Batch 3 (wave 2a): the shared right-click menu on a terminal and a file tab (chore 4), links
//! opening where "Open links in" says (chore 6), and the Files tree's menu (chore 7). Wave 3:
//! the agent chat's and its box's menus, and the Markdown Preview's.

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
            "src/file_tab_preview.rs",
            "src/chat_select.rs",
            "src/workspace_agent.rs",
        ],
    },
    Scene {
        name: "context-menu-chat",
        run: context_menu_chat,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/context_menu.rs",
            "src/workspace_menu.rs",
            "src/chat_select.rs",
            "src/agent_tab.rs",
            "src/md_preview.rs",
            "src/link_route.rs",
        ],
    },
    Scene {
        name: "context-menu-composer",
        run: context_menu_composer,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: &[
            "src/context_menu.rs",
            "src/workspace_menu.rs",
            "src/composer.rs",
            "src/agent_tab.rs",
        ],
    },
    Scene {
        name: "files-drag-pill",
        run: files_drag_pill,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/agent_pills.rs",
            "src/agent_tab.rs",
            "src/agent_box.rs",
            "src/agent_rows.rs",
            "src/composer.rs",
            "src/right_sidebar.rs",
            "src/files_menu.rs",
            "src/tab_bar.rs",
        ],
    },
    Scene {
        name: "agent-box-finder-drop",
        run: agent_box_finder_drop,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: &[
            "src/agent_pills.rs",
            "src/agent_tab.rs",
            "src/agent_box.rs",
            "src/composer.rs",
        ],
    },
    Scene {
        name: "terminal-drop-paths",
        run: terminal_drop_paths,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: &["src/agent_pills.rs", "src/terminal_view.rs"],
    },
    Scene {
        name: "agent-queued-cancel",
        run: agent_queued_cancel,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: &[
            "src/agent_cancel.rs",
            "src/agent_rewind.rs",
            "src/agent_tab.rs",
            "src/agent_conversation.rs",
            "src/providers/claude.rs",
        ],
    },
    Scene {
        name: "link-auto-open",
        run: link_auto_open,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: &[
            "src/auto_open.rs",
            "src/browser_url.rs",
            "src/link_route.rs",
            "src/workspace_api.rs",
            "src/workspace_browser.rs",
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
    let md = dir.join("README.md");
    std::fs::write(
        &md,
        "# Guide\n\n[the guide](https://example.com/guide) says it all.\n\nSecond paragraph here.\n",
    )
    .expect("the README");
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
        // Wave 3: the Markdown Preview. A right-click on a link offers its items and Copy
        // (nothing selected: disabled); a double-click selects a word, and Copy copies it.
        let readme = md.clone();
        cx.update_window(window, |_, window, cx| {
            ws.update(cx, |ws, cx| {
                ws.open_file_tab(readme.clone(), FileMode::Plain, window, cx)
            })
        })
        .expect("the README");
        frames(cx, window);
        let preview = cx.read(|cx| {
            let ws = ws.read(cx);
            ws.file_tabs[&ws.model.focused_tab().expect("it")].clone()
        });
        cx.update_window(window, |_, window, cx| {
            preview.update(cx, |v, cx| v.choose_preview_for_shots(true, window, cx))
        })
        .expect("Preview on");
        frames(cx, window);
        frames(cx, window);
        let boxes = cx.read(|cx| preview.read(cx).shot_preview_boxes());
        let (_, para) = boxes
            .iter()
            .find(|(t, _)| t.starts_with("the guide"))
            .cloned()
            .unwrap_or_else(|| panic!("{n}: the paragraph drawn: {boxes:?}"));
        let on_link = point(para.left() + px(12.0), para.top() + px(8.0));
        cx.simulate_mouse_down(window, on_link, MouseButton::Right, Modifiers::none());
        frames(cx, window);
        assert_eq!(
            menu_labels(cx, &ws),
            Some(labels(&context_menu::text_menu(
                false,
                Some("https://example.com/guide"),
                None
            ))),
            "{n}: the Preview's link menu"
        );
        choose(cx, window, &ws, "Copy Link");
        let clip = cx.read_from_clipboard().and_then(|c| c.text());
        assert_eq!(
            clip.as_deref(),
            Some("https://example.com/guide"),
            "{n}: Copy Link"
        );
        let (_, second) = boxes
            .iter()
            .find(|(t, _)| t.starts_with("Second"))
            .cloned()
            .expect("the second paragraph");
        let word = point(second.left() + px(10.0), second.top() + px(8.0));
        cx.simulate_event(
            window,
            gpui::MouseDownEvent {
                button: MouseButton::Left,
                position: word,
                modifiers: Modifiers::none(),
                click_count: 2,
                first_mouse: false,
            },
        );
        cx.simulate_mouse_up(window, word, MouseButton::Left, Modifiers::none());
        frames(cx, window);
        cx.simulate_mouse_down(window, word, MouseButton::Right, Modifiers::none());
        frames(cx, window);
        let copy_on = cx.read(|cx| {
            let menu = ws.read(cx).context_menu.as_ref().expect("a menu");
            matches!(&menu.entries[0], Entry::Item(i) if i.label == "Copy" && i.enabled)
        });
        assert!(copy_on, "{n}: Copy works in the Preview");
        choose(cx, window, &ws, "Copy");
        let clip = cx.read_from_clipboard().and_then(|c| c.text());
        assert_eq!(clip.as_deref(), Some("Second"), "{n}: the word copied");
        slot.borrow_mut().take();
    };
    render_with(&theme, dims(), build, act);
    println!("ui-shots: context-menu-file-tab ok");
}

/// A workspace with a shell and a Claude tab beside it (no process: the scene feeds it).
fn agent_beside_shell(
    theme: Theme,
    slot: Slot,
) -> impl FnOnce(&mut Window, &mut App) -> AnyView + 'static {
    super::agent::agent_workspace(theme, slot, |ws, window, cx| {
        let my_app = Path::new("/Users/me/code/my-app");
        let id = ws.open_unnamed_shot_session(Some(my_app), super::SHELL_TEXT, window, cx);
        ws.agent_beside(id, "claude", window, cx);
    })
}

/// Your message `text` sent, read and answered with `reply` (one turn).
fn talk(
    cx: &mut VisualTestAppContext,
    view: &Entity<crate::agent_tab::AgentTab>,
    text: &str,
    reply: &str,
) {
    use crate::providers::Event;
    super::agent::feed(cx, view, |t, cx| t.shot_history(Vec::new(), cx));
    super::agent::feed(cx, view, |t, cx| t.send(text.into(), cx));
    let reply = reply.to_string();
    super::agent::feed(cx, view, |t, cx| {
        t.shot_feed(
            vec![
                Event::Started {
                    conversation: "3f0e9a52-1b7c-4d2e-8a61-0c9b7e4d5a11".into(),
                    model: None,
                },
                Event::Taken { id: None },
                Event::TextDelta {
                    message: "m1".into(),
                    text: reply.clone(),
                    parent: None,
                },
                Event::MessageDone {
                    message: "m1".into(),
                    text: reply,
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
}

const CHAT_URL: &str = "https://example.com/docs";

/// `context-menu-chat` (both themes): an agent reply with a link. A right-click on it (nothing
/// selected) opens the link's items, Copy (disabled) and Select All; Copy Link copies it; a
/// plain click opens it where "Open links in" says (a browser split, your focus kept); Select
/// All, then Copy, copies the chat; Add to Claude Chat makes a quote chip in the same tab.
/// Shot: the menu open on the link.
fn context_menu_chat() {
    use std::cell::RefCell;
    use std::rc::Rc;
    for (k, &(slug, name)) in shot_themes().iter().enumerate() {
        let theme = shot_theme(name);
        let slot: Slot = Rc::new(RefCell::new(None));
        let build = agent_beside_shell(theme, slot.clone());
        let full = k == 0;
        let n = format!("context-menu-chat-{slug}");
        let path = shoot_with(
            "context-menu-chat",
            slug,
            &theme,
            // Wide enough that the agent's pane takes a split for its links.
            size(px(1600.0), px(640.0)),
            build,
            move |cx, window| {
                let ws = slotted(&slot);
                frames(cx, window);
                let (tab, view) = cx
                    .read(|cx| ws.read(cx).focused_agent_tab())
                    .expect("the agent tab");
                talk(
                    cx,
                    &view,
                    "Where are the docs?",
                    &format!("[The docs]({CHAT_URL}) explain the parser."),
                );
                frames(cx, window);
                let link_at = |cx: &mut VisualTestAppContext| {
                    let boxes = cx.read(|cx| view.read(cx).shot_text_boxes());
                    let (_, b) = boxes
                        .into_iter()
                        .find(|(t, _)| t.starts_with("The docs"))
                        .expect("the reply drawn");
                    point(b.left() + px(12.0), b.top() + px(8.0))
                };
                let on_link = link_at(cx);
                let nothing = labels(&context_menu::chat_menu(false, Some(CHAT_URL), "Claude"));
                if full {
                    cx.simulate_mouse_down(window, on_link, MouseButton::Right, Modifiers::none());
                    frames(cx, window);
                    assert_eq!(menu_labels(cx, &ws), Some(nothing.clone()), "{n}");
                    choose(cx, window, &ws, "Copy Link");
                    let clip = cx.read_from_clipboard().and_then(|c| c.text());
                    assert_eq!(clip.as_deref(), Some(CHAT_URL), "{n}: Copy Link");
                    // A plain click: a browser split on the right, the agent tab keeps focus.
                    cx.simulate_click(window, on_link, Modifiers::none());
                    frames(cx, window);
                    let opened = cx.read(|cx| {
                        let ws = ws.read(cx);
                        let id = ws.model.focused_workspace().expect("a workspace");
                        ws.model
                            .workspace(id)
                            .expect("it")
                            .panes
                            .iter()
                            .flat_map(|p| p.tabs.iter())
                            .any(|t| {
                                matches!(&t.kind, TabKind::Browser { url: Some(u) } if u == CHAT_URL)
                            })
                    });
                    assert!(opened, "{n}: the link opened in a browser tab");
                    let focused = cx.read(|cx| ws.read(cx).model.focused_tab());
                    assert_eq!(focused, Some(tab), "{n}: your focus stays");
                    // Select All, then Copy and Add to Chat.
                    cx.simulate_mouse_down(window, on_link, MouseButton::Right, Modifiers::none());
                    frames(cx, window);
                    choose(cx, window, &ws, "Select All");
                    cx.simulate_mouse_down(window, on_link, MouseButton::Right, Modifiers::none());
                    frames(cx, window);
                    assert_eq!(
                        menu_labels(cx, &ws),
                        Some(labels(&context_menu::chat_menu(
                            true,
                            Some(CHAT_URL),
                            "Claude"
                        ))),
                        "{n}: with a selection"
                    );
                    choose(cx, window, &ws, "Copy");
                    let clip = cx
                        .read_from_clipboard()
                        .and_then(|c| c.text())
                        .unwrap_or_default();
                    assert!(
                        clip.contains("Where are the docs?") && clip.contains("explain the parser"),
                        "{n}: the chat copied: {clip:?}"
                    );
                    cx.simulate_mouse_down(window, on_link, MouseButton::Right, Modifiers::none());
                    frames(cx, window);
                    choose(cx, window, &ws, "Add to Claude Chat");
                    let quote = cx.read(|cx| {
                        view.read(cx)
                            .shot_attachments()
                            .iter()
                            .find_map(|a| match a {
                                crate::agent_tab::Attachment::Quote { from, .. } => {
                                    Some(from.clone())
                                }
                                _ => None,
                            })
                    });
                    assert_eq!(quote.as_deref(), Some("Claude"), "{n}: a quote chip, here");
                    let focused = cx.read(|cx| ws.read(cx).model.focused_tab());
                    assert_eq!(focused, Some(tab), "{n}: the same tab");
                    // The selection goes with a press off the text.
                    let off = link_at(cx) + point(px(0.0), px(60.0));
                    cx.simulate_mouse_down(window, off, MouseButton::Left, Modifiers::none());
                    cx.simulate_mouse_up(window, off, MouseButton::Left, Modifiers::none());
                    frames(cx, window);
                }
                // The shot: the menu open on the link, nothing selected.
                let on_link = link_at(cx);
                cx.simulate_mouse_down(window, on_link, MouseButton::Right, Modifiers::none());
                frames(cx, window);
                assert_eq!(menu_labels(cx, &ws), Some(nothing), "{n}: shot");
            },
        );
        println!("ui-shots: {}", path.display());
    }
}

/// `context-menu-composer`: the agent tab's box. A right-click with nothing selected offers
/// Cut and Copy disabled, Paste and Select All; Select All, then Copy and Cut (the box empty),
/// then Paste (back); ⇧F10 in the box opens it at the caret.
fn context_menu_composer() {
    use std::cell::RefCell;
    use std::rc::Rc;
    let theme = shot_theme(shot_themes()[0].1);
    let slot: Slot = Rc::new(RefCell::new(None));
    let build = agent_beside_shell(theme, slot.clone());
    let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
        let n = "context-menu-composer";
        let ws = slotted(&slot);
        frames(cx, window);
        let (_, view) = cx
            .read(|cx| ws.read(cx).focused_agent_tab())
            .expect("the agent tab");
        let composer = cx.read(|cx| view.read(cx).shot_composer());
        cx.update(|cx| composer.update(cx, |c, cx| c.set_text("hello world".into(), cx)));
        frames(cx, window);
        let caret = |cx: &mut VisualTestAppContext| {
            cx.read(|cx| composer.read(cx).shot_caret())
                .map(|(_, b)| b.center())
                .expect("the caret drawn")
        };
        let text =
            |cx: &mut VisualTestAppContext| cx.read(|cx| composer.read(cx).text().to_string());
        let at = caret(cx);
        cx.simulate_mouse_down(window, at, MouseButton::Right, Modifiers::none());
        frames(cx, window);
        assert_eq!(
            menu_labels(cx, &ws),
            Some(labels(&context_menu::editable_menu(false))),
            "{n}: nothing selected"
        );
        choose(cx, window, &ws, "Select All");
        let at = caret(cx);
        cx.simulate_mouse_down(window, at, MouseButton::Right, Modifiers::none());
        frames(cx, window);
        assert_eq!(
            menu_labels(cx, &ws),
            Some(labels(&context_menu::editable_menu(true))),
            "{n}: all selected"
        );
        choose(cx, window, &ws, "Copy");
        let clip = cx.read_from_clipboard().and_then(|c| c.text());
        assert_eq!(clip.as_deref(), Some("hello world"), "{n}: Copy");
        press_keys(cx, window, "shift-f10");
        assert_eq!(
            menu_labels(cx, &ws),
            Some(labels(&context_menu::editable_menu(true))),
            "{n}: ⇧F10 in the box"
        );
        choose(cx, window, &ws, "Cut");
        assert_eq!(text(cx), "", "{n}: Cut empties the box");
        let at = caret(cx);
        cx.simulate_mouse_down(window, at, MouseButton::Right, Modifiers::none());
        frames(cx, window);
        choose(cx, window, &ws, "Paste");
        assert_eq!(text(cx), "hello world", "{n}: Paste puts it back");
        slot.borrow_mut().take();
    };
    render_with(&theme, dims(), build, act);
    println!("ui-shots: context-menu-composer ok");
}

/// A scene folder of its own for the Files tree (never the repo or yours).
fn drag_root(slug: &str) -> PathBuf {
    let root = crate::test_temp::dir(&format!("fuxx-ui-shots-drag-pill-{slug}"));
    std::fs::create_dir_all(root.join("src")).expect("the folders");
    for file in ["src/main.rs", "src/lib.rs", "README.md"] {
        std::fs::write(root.join(file), "fn main() {}\n").expect("the files");
    }
    std::fs::canonicalize(&root).unwrap_or(root)
}

/// Drag from `from` to `to` with the left button, released there unless `hold`.
fn drag_to(
    cx: &mut VisualTestAppContext,
    window: AnyWindowHandle,
    from: Point<Pixels>,
    to: Point<Pixels>,
    hold: bool,
) {
    let left = Some(MouseButton::Left);
    cx.simulate_mouse_down(window, from, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(
        window,
        from + point(px(8.0), px(4.0)),
        left,
        Modifiers::none(),
    );
    frames(cx, window);
    cx.simulate_mouse_move(
        window,
        to - point(px(6.0), px(0.0)),
        left,
        Modifiers::none(),
    );
    cx.simulate_mouse_move(window, to, left, Modifiers::none());
    frames(cx, window);
    if !hold {
        cx.simulate_mouse_up(window, to, MouseButton::Left, Modifiers::none());
        frames(cx, window);
    }
}

/// `files-drag-pill` (both themes; a folder of the scene's own): a Files row dragged to the
/// agent tab tints its page, rings its box and says "Drop to add to your message"; dropped,
/// it is a path pill (`src/main.rs `) in the box; a folder dropped on the tab's pill is
/// `src/`; the tree menu's Add to Claude Chat adds `README.md`; a pill's click opens its file
/// tab; sent, the message lists them as file chips and the agent reads the paths in the text.
/// Shot: a row held over the page.
fn files_drag_pill() {
    use std::cell::RefCell;
    use std::rc::Rc;
    for (k, &(slug, name)) in shot_themes().iter().enumerate() {
        let theme = shot_theme(name);
        let root = drag_root(slug);
        let slot: Slot = Rc::new(RefCell::new(None));
        let build = {
            let (root, slot) = (root.clone(), slot.clone());
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
                    let id = ws.open_unnamed_shot_session(Some(&root), "", window, cx);
                    ws.agent_beside(id, "claude", window, cx);
                    ws.right_sidebar
                        .files
                        .show_for_shots(files_menu_tree(&root));
                    ws
                });
                *slot.borrow_mut() = Some(ws.clone());
                ws.into()
            }
        };
        let full = k == 0;
        let n = format!("files-drag-pill-{slug}");
        let path = shoot_with(
            "files-drag-pill",
            slug,
            &theme,
            size(px(1600.0), px(640.0)),
            build,
            move |cx, window| {
                let ws = slotted(&slot);
                frames(cx, window);
                let (agent, view) = cx
                    .read(|cx| ws.read(cx).focused_agent_tab())
                    .expect("the agent tab");
                let composer = cx.read(|cx| view.read(cx).shot_composer());
                let text = |cx: &mut VisualTestAppContext| {
                    cx.read(|cx| composer.read(cx).text().to_string())
                };
                let page = |cx: &mut VisualTestAppContext| {
                    let b = cx
                        .read(|cx| composer.read(cx).shot_rows().2)
                        .expect("the box drawn");
                    point(b.center().x, b.top() - px(160.0))
                };
                if full {
                    // A file onto the page: its drop states, then the pill.
                    let (from, to) = (row_point(cx, &ws, "src/main.rs"), page(cx));
                    drag_to(cx, window, from, to, true);
                    let (dropping, words) = cx.read(|cx| {
                        let v = view.read(cx);
                        (
                            v.dropping(cx),
                            v.shot_composer().read(cx).placeholder().to_string(),
                        )
                    });
                    assert!(dropping, "{n}: its drop states");
                    assert_eq!(words, crate::agent_tab::pills::DROP_PLACEHOLDER, "{n}");
                    cx.simulate_mouse_up(window, to, MouseButton::Left, Modifiers::none());
                    frames(cx, window);
                    assert_eq!(text(cx), "src/main.rs ", "{n}: the pill");
                    assert!(
                        !cx.read(|cx| view.read(cx).dropping(cx)),
                        "{n}: states gone"
                    );
                    let pills = cx.read(|cx| composer.read(cx).shot_pills().len());
                    assert_eq!(pills, 1, "{n}: drawn as a pill");
                    // A folder onto the tab's pill in its bar.
                    let pane = cx.read(|cx| ws.read(cx).model.locate(agent).expect("it").1);
                    let pill = super::tab_bounds(cx, &ws, pane, 0).expect("the tab laid out");
                    let from = row_point(cx, &ws, "src");
                    drag_to(cx, window, from, pill.center(), false);
                    assert_eq!(text(cx), "src/main.rs src/ ", "{n}: a folder's pill");
                    // The tree menu's Add to Claude Chat.
                    let at = row_point(cx, &ws, "README.md");
                    cx.simulate_mouse_down(window, at, MouseButton::Right, Modifiers::none());
                    frames(cx, window);
                    choose(cx, window, &ws, "Add to Claude Chat");
                    assert_eq!(text(cx), "src/main.rs src/ README.md ", "{n}: Add to Chat");
                    // A pill's click opens its file tab.
                    let first = cx.read(|cx| composer.read(cx).shot_pills()[0]);
                    cx.simulate_click(window, first.center(), Modifiers::none());
                    frames(cx, window);
                    let opened = cx.read(|cx| {
                        let ws = ws.read(cx);
                        let tab = ws.model.focused_tab().expect("a tab");
                        ws.file_tabs.get(&tab).map(|v| v.read(cx).path.clone())
                    });
                    assert_eq!(opened, Some(root.join("src/main.rs")), "{n}: its file tab");
                    select(cx, window, &ws, agent);
                    // Sent: file chips under it, the paths in what the agent reads.
                    super::agent::feed(cx, &view, |t, cx| t.shot_history(Vec::new(), cx));
                    let typed = text(cx);
                    super::agent::feed(cx, &view, |t, cx| t.send(typed.clone(), cx));
                    let chips = cx.read(|cx| {
                        view.read(cx)
                            .shot_items()
                            .iter()
                            .rev()
                            .find_map(|i| match i {
                                crate::agent_conversation::Item::You { attachments, .. } => {
                                    Some(attachments.clone())
                                }
                                _ => None,
                            })
                    });
                    assert_eq!(
                        chips,
                        Some(vec![
                            "main.rs".to_string(),
                            "src/".to_string(),
                            "README.md".to_string()
                        ]),
                        "{n}: file chips"
                    );
                    let wire = cx.read(|cx| view.read(cx).shot_written());
                    assert!(
                        wire.iter()
                            .any(|l| l.contains("src/main.rs src/ README.md")),
                        "{n}: the paths in the text: {wire:?}"
                    );
                    // Read and answered (no spinner in the shot).
                    super::agent::feed(cx, &view, |t, cx| {
                        use crate::providers::Event;
                        t.shot_feed(
                            vec![
                                Event::Started {
                                    conversation: "3f0e9a52-1b7c-4d2e-8a61-0c9b7e4d5a13".into(),
                                    model: None,
                                },
                                Event::Taken { id: None },
                                Event::TurnDone {
                                    ok: true,
                                    message: None,
                                },
                            ],
                            cx,
                        )
                    });
                    frames(cx, window);
                }
                // The shot: a row held over the page.
                let (from, to) = (row_point(cx, &ws, "src/lib.rs"), page(cx));
                drag_to(cx, window, from, to, true);
                assert!(
                    cx.read(|cx| view.read(cx).dropping(cx)),
                    "{n}: held over it"
                );
            },
        );
        println!("ui-shots: {}", path.display());
    }
}

/// `agent-box-finder-drop`: files dragged in from Finder (`ExternalPaths`) onto the agent tab:
/// while over it, its drop states; dropped, a file and a folder are path pills (whole: they are
/// outside the agent's folder) and an image an image pill, as a pasted one.
fn agent_box_finder_drop() {
    use std::cell::RefCell;
    use std::rc::Rc;
    let theme = shot_theme(shot_themes()[0].1);
    let dir = crate::test_temp::dir("fuxx-ui-shots-finder-drop");
    std::fs::create_dir_all(dir.join("docs")).expect("the folder");
    let notes = dir.join("notes.txt");
    std::fs::write(&notes, "made-up").expect("a file");
    let shot = dir.join("shot.png");
    std::fs::write(&shot, b"\x89PNG\r\n\x1a\nmade-up").expect("an image");
    let docs = dir.join("docs");
    let slot: super::agent::Slot = Rc::new(RefCell::new(None));
    let keep = slot.clone();
    let build = super::agent::build_with(
        &crate::providers::claude::CLAUDE,
        theme,
        slot,
        super::agent::start(),
    );
    render_with(
        &theme,
        size(px(720.0), px(640.0)),
        build,
        move |cx, window| {
            let n = "agent-box-finder-drop";
            let tab = super::agent::tab_of(&keep);
            frames(cx, window);
            let composer = cx.read(|cx| tab.read(cx).shot_composer());
            let b = cx
                .read(|cx| composer.read(cx).shot_rows().2)
                .expect("the box drawn");
            let over = point(b.center().x, b.top() - px(200.0));
            let paths = gpui::ExternalPaths(vec![notes.clone(), docs.clone(), shot.clone()].into());
            cx.simulate_event(
                window,
                gpui::FileDropEvent::Entered {
                    position: over,
                    paths,
                },
            );
            cx.simulate_event(window, gpui::FileDropEvent::Pending { position: over });
            frames(cx, window);
            assert!(cx.read(|cx| tab.read(cx).dropping(cx)), "{n}: drop states");
            cx.simulate_event(window, gpui::FileDropEvent::Submit { position: over });
            let want = format!("{} {}/ [Image #1]", notes.display(), docs.display());
            let start = std::time::Instant::now();
            while cx.read(|cx| composer.read(cx).text().to_string()) != want
                && start.elapsed() < std::time::Duration::from_secs(5)
            {
                frames(cx, window);
            }
            assert_eq!(
                cx.read(|cx| composer.read(cx).text().to_string()),
                want,
                "{n}: path pills and an image pill"
            );
            assert!(!cx.read(|cx| tab.read(cx).dropping(cx)), "{n}: states gone");
            let pills = cx.read(|cx| composer.read(cx).shot_pills().len());
            // (A long path wraps: its pill is drawn on each of its rows.)
            assert!(pills >= 3, "{n}: three pills, drawn: {pills}");
            keep.borrow_mut().take();
        },
    );
    println!("ui-shots: agent-box-finder-drop ok");
}

/// `terminal-drop-paths`: files dropped on a terminal (from Finder, then a Files row) are typed
/// as their shell-escaped paths, apart by spaces, a space after.
fn terminal_drop_paths() {
    use std::cell::RefCell;
    use std::rc::Rc;
    let theme = shot_theme(shot_themes()[0].1);
    let slot: Slot = Rc::new(RefCell::new(None));
    let build = one_terminal(theme, "$ ".into(), slot.clone());
    render_with(&theme, dims(), build, move |cx, window| {
        let n = "terminal-drop-paths";
        let ws = slotted(&slot);
        frames(cx, window);
        let view = terminal(cx, &ws);
        cx.update(|cx| view.update(cx, |v, _| v.take_sent()));
        let at = cell(cx, &ws, 4, 2).center();
        let paths = gpui::ExternalPaths(
            vec![
                PathBuf::from("/tmp/My Notes (1).txt"),
                PathBuf::from("/tmp/it's.md"),
            ]
            .into(),
        );
        cx.simulate_event(
            window,
            gpui::FileDropEvent::Entered {
                position: at,
                paths,
            },
        );
        cx.simulate_event(window, gpui::FileDropEvent::Pending { position: at });
        cx.simulate_event(window, gpui::FileDropEvent::Submit { position: at });
        frames(cx, window);
        let sent = cx.update(|cx| view.update(cx, |v, _| v.take_sent()));
        assert_eq!(
            String::from_utf8_lossy(&sent),
            "/tmp/My\\ Notes\\ \\(1\\).txt /tmp/it\\'s.md ",
            "{n}: typed, escaped"
        );
        slot.borrow_mut().take();
    });
    println!("ui-shots: terminal-drop-paths ok");
}

/// `agent-queued-cancel`: × on a message Claude hasn't read asks it to drop it
/// (`cancel_async_message` with its id); dropped, it leaves the chat and no turn is waited on;
/// one already read stays, with a note; a message fuxx still holds (the agent not there yet)
/// goes before it is ever written.
fn agent_queued_cancel() {
    use crate::agent_conversation::Item;
    use crate::providers::Event;
    use std::cell::RefCell;
    use std::rc::Rc;
    let theme = shot_theme(shot_themes()[0].1);
    let slot: super::agent::Slot = Rc::new(RefCell::new(None));
    let keep = slot.clone();
    let build = super::agent::build_with(
        &crate::providers::claude::CLAUDE,
        theme,
        slot,
        super::agent::start(),
    );
    render_with(
        &theme,
        size(px(720.0), px(640.0)),
        build,
        move |cx, window| {
            use super::agent::feed;
            let n = "agent-queued-cancel";
            let tab = super::agent::tab_of(&keep);
            // Held before the agent is there (it boots): × drops it, never written.
            cx.update(|cx| tab.update(cx, |t, _| t.shot_boot()));
            feed(cx, &tab, |t, cx| t.send("held".into(), cx));
            assert_eq!(cx.read(|cx| tab.read(cx).shot_pending()), 1, "{n}: held");
            feed(cx, &tab, |t, cx| t.cancel_held(0, cx));
            assert_eq!(cx.read(|cx| tab.read(cx).shot_pending()), 0, "{n}: dropped");
            feed(cx, &tab, |t, cx| t.shot_history(Vec::new(), cx));
            assert!(
                !cx.read(|cx| tab.read(cx).shot_written())
                    .iter()
                    .any(|l| l.contains("held")),
                "{n}: never written"
            );
            // A turn running, then a note it hasn't read.
            feed(cx, &tab, |t, cx| t.send("Fix the parser".into(), cx));
            let id = |cx: &mut VisualTestAppContext, text: &str| {
                cx.read(|cx| {
                    tab.read(cx).shot_items().iter().find_map(|i| match i {
                        Item::You {
                            text: t,
                            id: Some(id),
                            ..
                        } if t == text => Some(id.clone()),
                        _ => None,
                    })
                })
            };
            let first = id(cx, "Fix the parser").expect("an id");
            feed(cx, &tab, |t, cx| {
                t.shot_feed(
                    vec![
                        Event::Started {
                            conversation: "3f0e9a52-1b7c-4d2e-8a61-0c9b7e4d5a12".into(),
                            model: None,
                        },
                        Event::Taken {
                            id: Some(first.clone()),
                        },
                    ],
                    cx,
                )
            });
            feed(cx, &tab, |t, cx| t.send("also add a test".into(), cx));
            frames(cx, window);
            let ix = cx.read(|cx| {
                tab.read(cx)
                    .shot_items()
                    .iter()
                    .position(|i| matches!(i, Item::You { text, .. } if text == "also add a test"))
                    .expect("the note")
            });
            let note = id(cx, "also add a test").expect("its id");
            let button = cx
                .read(|cx| {
                    tab.read(cx)
                        .rw
                        .shot_buttons
                        .borrow()
                        .get(&format!("cancel:{ix}"))
                        .copied()
                })
                .expect("× on the unread note");
            cx.simulate_mouse_move(window, button.center(), None, Modifiers::none());
            frames(cx, window);
            cx.simulate_click(window, button.center(), Modifiers::none());
            frames(cx, window);
            let asked = cx
                .read(|cx| tab.read(cx).shot_written())
                .iter()
                .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
                .any(|v| {
                    v["request"]["subtype"] == "cancel_async_message"
                        && v["request"]["message_uuid"] == note.as_str()
                });
            assert!(asked, "{n}: Claude asked to drop it");
            let none_read = cx.read(|cx| {
                tab.read(cx)
                    .rw
                    .shot_buttons
                    .borrow()
                    .contains_key("cancel:0")
            });
            assert!(!none_read, "{n}: no × on a read message");
            feed(cx, &tab, |t, cx| {
                t.shot_feed(
                    vec![Event::QueuedCancelled {
                        id: note.clone(),
                        cancelled: true,
                    }],
                    cx,
                )
            });
            assert!(id(cx, "also add a test").is_none(), "{n}: it left the chat");
            // Another, already taken in when asked: it stays, with a note.
            feed(cx, &tab, |t, cx| t.send("and the docs".into(), cx));
            let late = id(cx, "and the docs").expect("its id");
            let ix = cx.read(|cx| tab.read(cx).shot_items().len() - 1);
            feed(cx, &tab, |t, cx| t.cancel_message(ix, cx));
            feed(cx, &tab, |t, cx| {
                t.shot_feed(
                    vec![Event::QueuedCancelled {
                        id: late.clone(),
                        cancelled: false,
                    }],
                    cx,
                )
            });
            assert!(id(cx, "and the docs").is_some(), "{n}: it stays");
            let noted = cx.read(|cx| {
                tab.read(cx)
                    .shot_items()
                    .iter()
                    .any(|i| matches!(i, Item::Notice(t) if t == crate::agent_tab::CANCEL_LATE))
            });
            assert!(noted, "{n}: the note says why");
            keep.borrow_mut().take();
        },
    );
    println!("ui-shots: agent-queued-cancel ok");
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

/// `link-auto-open`: a program in a terminal opens a browser by itself (`npm run dev` with
/// auto-open: the `open` shim's `fuxx link open`, the `link.open` request). It lands in a
/// browser split on the right, its pill flashing, the terminal keeping your focus; a second is
/// a tab in that pane, flashing too; the same address again shows its tab. A terminal in a
/// workspace you aren't looking at opens beside itself, and you stay where you are. A local
/// page (chore 35) opens as a web page does. No caller, an unknown one, or another address
/// (another file, `mailto:`) is refused (the shim then runs the real `open`); with
/// "Your default browser" it goes to the OS.
fn link_auto_open() {
    use crate::appearance::{Appearance, OpenLinksIn};
    use serde_json::json;
    use std::cell::RefCell;
    use std::rc::Rc;
    let text = "$ npm run dev\r\n\r\n  VITE v5.4.0  ready in 312 ms\r\n\r\n  \u{279c}  Local:   http://localhost:5173/\r\n".to_string();
    let theme = shot_theme(shot_themes()[0].1);
    let slot: Slot = Rc::new(RefCell::new(None));
    let build = one_terminal(theme, text, slot.clone());
    let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
        let n = "link-auto-open";
        let ws = slotted(&slot);
        frames(cx, window);
        let term_tab = cx.read(|cx| ws.read(cx).model.focused_tab().expect("the terminal"));
        let id = cx.read(|cx| ws.read(cx).sessions.items()[0].id);
        let open = |cx: &mut VisualTestAppContext, p: serde_json::Value| {
            let got = cx
                .update_window(window, |_, window, cx| {
                    ws.update(cx, |ws, cx| ws.link_open(&p, window, cx))
                })
                .expect("link.open");
            frames(cx, window);
            got.map_err(|e| e.code)
        };
        let panes = |cx: &mut VisualTestAppContext| {
            cx.read(|cx| {
                let ws = ws.read(cx);
                let (wid, _) = ws.model.locate(term_tab).expect("its workspace");
                ws.model
                    .workspace(wid)
                    .expect("it")
                    .layout
                    .leaves()
                    .into_iter()
                    .filter_map(|p| ws.model.pane(p))
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
        let flash =
            |cx: &mut VisualTestAppContext| cx.read(|cx| ws.read(cx).link_flash.map(|(t, ..)| t));
        let url = "http://localhost:5173/";
        // The dev server's auto-open: a browser split on the right, flashing; focus stays.
        assert_eq!(open(cx, json!({"url": url, "caller": id})), Ok(json!({})));
        let now = panes(cx);
        assert_eq!(now.len(), 2, "{n}: split: {now:?}");
        assert!(now[0].1.is_empty(), "{n}: the terminal's pane has no page");
        assert_eq!(now[1].1, [url], "{n}: the page on the right");
        let first = cx.read(|cx| ws.read(cx).model.pane(now[1].0).unwrap().tabs[0].id);
        assert_eq!(
            flash(cx),
            Some(first),
            "{n}: a new split's pill flashes too"
        );
        assert_eq!(focused(cx), Some(term_tab), "{n}: your focus stays");
        // A second address: a tab in that pane, flashing.
        let docs = "https://example.com/docs";
        assert_eq!(open(cx, json!({"url": docs, "caller": id})), Ok(json!({})));
        let now = panes(cx);
        assert_eq!(now.len(), 2, "{n}: no second split");
        assert_eq!(now[1].1, [url, docs], "{n}: a tab in the link pane");
        let second = cx.read(|cx| ws.read(cx).model.pane(now[1].0).unwrap().tabs[1].id);
        assert_eq!(flash(cx), Some(second), "{n}: its pill flashes");
        assert_eq!(focused(cx), Some(term_tab), "{n}: your focus stays");
        // The first again: its tab shown, no third.
        assert_eq!(open(cx, json!({"url": url, "caller": id})), Ok(json!({})));
        let now = panes(cx);
        assert_eq!(now[1].1.len(), 2, "{n}: not opened twice");
        let shown = cx.read(|cx| ws.read(cx).model.pane(now[1].0).unwrap().active);
        assert_eq!(shown, first, "{n}: its tab shown");
        assert_eq!(flash(cx), Some(first), "{n}: and flashing");
        // Chore 35: a page on disk (`open index.html`, the shim's absolute path, `fuxx link
        // open`'s file:// address): a tab in that pane, as a web page.
        let site = crate::test_temp::dir("my-app-site");
        std::fs::create_dir_all(&site).expect("the site folder");
        std::fs::write(site.join("index.html"), "<h1>my-app</h1>").expect("the page");
        let page = crate::browser_url::page_url(&site.join("index.html")).expect("a page");
        assert_eq!(open(cx, json!({"url": page, "caller": id})), Ok(json!({})));
        let now = panes(cx);
        assert_eq!(
            now[1].1,
            [url, docs, page.as_str()],
            "{n}: the local page's tab"
        );
        // Refused, so the shim runs the real open: no caller, an unknown one, not a web page.
        assert_eq!(open(cx, json!({"url": url})), Err("bad_request"), "{n}");
        assert_eq!(
            open(cx, json!({"url": url, "caller": 999})),
            Err("not_found")
        );
        assert_eq!(
            open(cx, json!({"url": url, "agent": "a9"})),
            Err("not_found")
        );
        assert_eq!(
            open(cx, json!({"url": "file:///etc/hosts", "caller": id})),
            Err("bad_request")
        );
        assert_eq!(
            open(cx, json!({"url": "mailto:team@example.com", "caller": id})),
            Err("bad_request")
        );
        assert_eq!(panes(cx)[1].1.len(), 3, "{n}: nothing opened for those");
        // A terminal in another workspace: its page opens beside it; you stay where you are.
        let other = cx
            .update_window(window, |_, window, cx| {
                ws.update(cx, |ws, cx| {
                    let project = PathBuf::from("/Users/me/code/my-app");
                    ws.open_shot_session("web", &project, "$ npm run dev\r\n", None, window, cx);
                    ws.sessions.items().last().map(|s| s.id)
                })
            })
            .expect("a second session")
            .expect("its id");
        frames(cx, window);
        let other_tab = cx.read(|cx| ws.read(cx).model.terminal_tab(other).expect("its tab"));
        select(cx, window, &ws, term_tab);
        frames(cx, window);
        let mine = cx.read(|cx| ws.read(cx).model.locate(term_tab).map(|(w, _)| w));
        let theirs = cx.read(|cx| ws.read(cx).model.locate(other_tab).map(|(w, _)| w));
        assert_ne!(mine, theirs, "{n}: a workspace of its own");
        {
            let preview = "http://localhost:5174/";
            assert_eq!(
                open(cx, json!({"url": preview, "caller": other})),
                Ok(json!({}))
            );
            assert_eq!(focused(cx), Some(term_tab), "{n}: you stay where you are");
            let (beside, its_focus) = cx.read(|cx| {
                let ws = ws.read(cx);
                let w = ws.model.workspace(theirs.unwrap()).unwrap();
                let urls: Vec<String> = w
                    .panes
                    .iter()
                    .flat_map(|p| &p.tabs)
                    .filter_map(|t| match &t.kind {
                        TabKind::Browser { url } => url.clone(),
                        _ => None,
                    })
                    .collect();
                let focus = ws.model.pane(w.focused).map(|p| p.active);
                (urls, focus)
            });
            assert_eq!(beside, [preview], "{n}: beside the other terminal");
            assert_eq!(
                its_focus,
                Some(other_tab),
                "{n}: its terminal keeps its focus"
            );
        }
        // "Your default browser": the OS, as for a click.
        cx.update(|cx| {
            cx.update_global::<Appearance, _>(|a, _| a.open_links_in = OpenLinksIn::Browser);
        });
        frames(cx, window);
        assert_eq!(open(cx, json!({"url": docs, "caller": id})), Ok(json!({})));
        let outside = cx.read(|cx| ws.read(cx).shot_opened_outside.clone());
        assert_eq!(outside, [docs], "{n}: to the default browser");
        slot.borrow_mut().take();
    };
    render_with(&theme, dims(), build, act);
    println!("ui-shots: link-auto-open ok");
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
                    Some(labels(&context_menu::file_tree_menu(
                        false,
                        Some("Claude"),
                        true
                    ))),
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
                        Some(labels(&context_menu::file_tree_menu(
                            false,
                            Some("Claude"),
                            true
                        ))),
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
                        Some(labels(&context_menu::file_tree_menu(
                            true,
                            Some("Claude"),
                            false
                        ))),
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
