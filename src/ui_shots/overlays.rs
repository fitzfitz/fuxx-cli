//! Boxes over the window: one overlay at a time, the Help panel, the host picker and address
//! box, and Preferences (M11x).

use super::*;

/// This area's scenes (`mod.rs` runs them in its `ORDER`).
pub(super) const SCENES: &[Scene] = &[
    Scene {
        name: "overlays",
        run: overlays,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: &[
            "src/overlay_panel.rs",
            "src/name_input.rs",
            "src/palette.rs",
            "src/branch_switcher.rs",
            "src/help_panel.rs",
            "src/host_picker.rs",
            "src/workspace_browser.rs",
            "src/welcome.rs",
            "src/pane_tab_menu.rs",
            "src/tab_menu.rs",
            "src/pane_actions.rs",
            "src/title_area.rs",
            "src/preferences.rs",
        ],
    },
    Scene {
        name: "help",
        run: help_scenes,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: &[
            "src/help_panel.rs",
            "src/help_shortcuts.rs",
            "src/help_commands.rs",
            "src/overlay_panel.rs",
            "src/name_input.rs",
            "src/palette.rs",
            "src/sidebar.rs",
        ],
    },
    Scene {
        name: "panels",
        run: panel_scenes,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: &[
            "src/host_picker.rs",
            "src/ssh_hosts.rs",
            "src/ssh_target.rs",
            "src/workspace_browser.rs",
            "src/overlay_panel.rs",
            "src/name_input.rs",
        ],
    },
    Scene {
        name: "preferences",
        run: preferences_scenes,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/preferences.rs",
            "src/preferences_plugins.rs",
            "src/plugins.rs",
            "src/plugin_store.rs",
            "src/plugin_source.rs",
            "src/prefs.rs",
            "src/name_input.rs",
        ],
    },
];

/// One box over the window at a time: opening the rename box, the palette, the address box or
/// the Help panel over another closes that one, and the new one keeps the keyboard (the old one's
/// late "lost focus" must not take it back). Rename -> palette -> rename -> address box ->
/// palette -> help -> rename -> help -> palette, each checked after frames have run the focus
/// changes. Then every overlay (`Overlay::ALL`) is opened
/// in turn: each must count in the in-pane pages' hide signal (`overlays().any()`), and closing
/// the overlays must clear it. Panics on the first that doesn't hold.
fn overlays() {
    use gpui::Focusable;
    use std::cell::RefCell;
    use std::rc::Rc;

    let theme = shot_theme("Catppuccin Mocha");
    let slot: Slot = Rc::new(RefCell::new(None));
    let keep = slot.clone();
    let build = move |window: &mut Window, cx: &mut App| -> AnyView {
        cx.set_global(sidebar_appearance(theme, cx));
        let ws = cx.new(|cx| switcher_workspace(window, cx));
        *keep.borrow_mut() = Some(ws.clone());
        ws.into()
    };
    let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
        let ws = slotted(&slot);
        // Which box is open, and whether it has the keyboard.
        let open = |cx: &mut VisualTestAppContext| {
            cx.update_window(window, |_, window, cx| {
                let ws = ws.read(cx);
                let mut open = vec![];
                if let Some((_, input)) = &ws.renaming {
                    open.push(("rename", input.focus_handle(cx).is_focused(window)));
                }
                if let Some(p) = &ws.palette {
                    open.push(("palette", p.focus_handle(cx).is_focused(window)));
                }
                if let Some(a) = &ws.address_box {
                    open.push(("address", a.focus_handle(cx).is_focused(window)));
                }
                if let Some(p) = &ws.host_picker {
                    open.push(("picker", p.focus_handle(cx).is_focused(window)));
                }
                if let Some(h) = &ws.help {
                    open.push(("help", h.focus_handle(cx).is_focused(window)));
                }
                open
            })
            .expect("read the boxes")
        };
        for (step, expect) in [
            ("rename", "rename"),
            ("palette", "palette"),
            ("rename", "rename"),
            ("address", "address"),
            ("palette", "palette"),
            ("help", "help"),
            ("rename", "rename"),
            ("help", "help"),
            ("palette", "palette"),
        ] {
            cx.update_window(window, |_, window, cx| {
                ws.update(cx, |ws, cx| match step {
                    "rename" => ws.start_rename(ws.model.display_order()[3], window, cx),
                    "address" => ws.open_address_box(window, cx),
                    "help" => {
                        ws.open_help(crate::help_panel::HelpPage::Shortcuts, true, window, cx)
                    }
                    _ => ws.open_palette(crate::palette_items::Scope::All, window, cx),
                })
            })
            .expect("open it");
            // Frames, so focus changes (and the boxes' blur events) run.
            for _ in 0..3 {
                cx.update_window(window, |_, window, cx| window.draw(cx).clear())
                    .expect("draw");
                cx.run_until_parked();
            }
            assert_eq!(open(cx), [(expect, true)], "overlays: after opening {step}");
        }
        // Every overlay hides the pages while it's open: one forgotten in `overlays()` would sit
        // under a page.
        for &overlay in Overlay::ALL {
            cx.update_window(window, |_, window, cx| {
                ws.update(cx, |ws, cx| {
                    let first = ws.model.display_order()[0];
                    let at = point(px(300.0), px(200.0));
                    match overlay {
                        Overlay::Rename => ws.start_rename(first, window, cx),
                        Overlay::HostPicker => {
                            // Made up hosts: no `~/.ssh` read, no `ssh -G` run.
                            ws.close_overlays(Overlay::HostPicker);
                            let picker = cx.new(|cx| {
                                crate::host_picker::HostPicker::new(vec![], vec![], window, cx)
                            });
                            picker.focus_handle(cx).focus(window, cx);
                            ws.host_picker = Some(picker);
                        }
                        Overlay::AddressBox => ws.open_address_box(window, cx),
                        Overlay::Palette => {
                            ws.open_palette(crate::palette_items::Scope::All, window, cx)
                        }
                        Overlay::BranchSwitcher => {
                            let sw = cx.new(|cx| {
                                crate::branch_switcher::BranchSwitcher::seeded(
                                    switcher_repo(),
                                    vec![],
                                    switcher_snapshot(),
                                    Default::default(),
                                    None,
                                    window,
                                    cx,
                                )
                            });
                            ws.show_branch_switcher(sw, window, cx)
                        }
                        Overlay::TabMenu => ws.open_tab_menu(first, at, window, cx),
                        Overlay::PaneTabMenu => {
                            let tab = ws.model.focused_tab().expect("a focused tab");
                            ws.open_pane_tab_menu(tab, at, window, cx)
                        }
                        Overlay::PaneActions => {
                            let pane = ws.model.focused_pane().expect("a focused pane");
                            ws.open_pane_actions(pane, at, window, cx)
                        }
                        Overlay::Notifications => ws.open_notifications(window, cx),
                        Overlay::Help => {
                            ws.open_help(crate::help_panel::HelpPage::Shortcuts, true, window, cx)
                        }
                        Overlay::Welcome => {
                            ws.open_welcome(crate::welcome_items::Origin::Help, window, cx)
                        }
                        Overlay::Preferences => ws.toggle_preferences(window, cx),
                    }
                })
            })
            .expect("open it");
            cx.update_window(window, |_, window, cx| window.draw(cx).clear())
                .expect("draw");
            cx.run_until_parked();
            let other = if overlay == Overlay::Rename {
                Overlay::Palette
            } else {
                Overlay::Rename
            };
            cx.update(|cx| {
                assert!(
                    ws.read(cx).overlays(cx).any(),
                    "overlays: {overlay:?} hides the pages"
                );
                ws.update(cx, |ws, _| ws.close_overlays(other));
                assert!(
                    !ws.read(cx).overlays(cx).any(),
                    "overlays: closing them clears {overlay:?}"
                );
            });
        }
        // The + menu (no keyboard of its own) hides the pages too, and any box opening closes
        // it: the palette isn't left under its click-away backdrop.
        cx.update(|cx| {
            ws.update(cx, |ws, _| ws.sidebar.new_menu = true);
            assert!(
                ws.read(cx).overlays(cx).any(),
                "overlays: the + menu hides the pages"
            );
        });
        cx.update_window(window, |_, window, cx| {
            ws.update(cx, |ws, cx| {
                ws.open_palette(crate::palette_items::Scope::All, window, cx)
            })
        })
        .expect("open the palette");
        cx.update(|cx| {
            let ws = ws.read(cx);
            assert!(
                !ws.sidebar.new_menu && ws.palette.is_some(),
                "overlays: ⌘K closes the + menu"
            );
        });
        cx.update(|cx| ws.update(cx, |ws, _| ws.close_overlays(Overlay::Rename)));
        assert!(
            cx.update(|cx| !ws.read(cx).overlays(cx).any()),
            "overlays: closing them clears the + menu"
        );
    };
    render_with(&theme, size(px(960.0), px(600.0)), build, act);
}

/// The Help panel (M11d) over the ⌘P fixture's workspace, in Mocha (both themes with
/// `--all-themes`):
/// - `help-shortcuts`: Keyboard Shortcuts just opened, every listed binding in its group;
/// - `help-split`: "split" typed, which leaves Split Right and Split Down;
/// - `help-none`: "zzz" typed, which finds nothing ("No shortcut matches");
/// - `help-commands`: the Command Reference with "browser" typed;
/// - `help-palette`: "palette" typed, which finds Command Palette… (⌘K);
/// - `help-over-palette`: Keyboard Shortcuts opened while the command palette is open;
/// - `help-toggle`: Keyboard Shortcuts opened and opened again (as ⌘/ twice does);
/// - `help-tips`: Help → Show Tips Card with the sidebar hidden and the tips card closed.
///
/// Panics unless the rows shown are the ones the pure modules list for that text (Command
/// Palette… with ⌘K and Go to Workspace… with ⌘P among them), the panel opened over the palette
/// closed it and has the keyboard, and the second open closed the panel
/// and gave the keyboard back to the active terminal. `help-toggle` first checks the panel's
/// keys: ↓ scrolls the list and ↑ scrolls it back, Enter leaves it open, Esc closes it (the
/// keyboard back to the terminal), ↓ with nothing found leaves the list at the top once the box
/// is cleared, Help → fuxx Command Reference over Keyboard Shortcuts replaces that page, and
/// asking for the shown page from another key window keeps it open. `help-tips` panics unless
/// the sidebar is shown again with the card back.
fn help_scenes() {
    use crate::help_commands::{entry_matches, reference};
    use crate::help_panel::{HelpPage, ScrollDown, ScrollUp, shortcut_list};
    use crate::help_shortcuts::row_matches;
    use gpui::{Focusable, KeyBinding};
    use std::cell::RefCell;
    use std::rc::Rc;

    let shortcuts = shortcut_list(false);
    let all_shortcuts = shortcuts.len();
    let split = shortcuts.iter().filter(|r| row_matches(r, "split")).count();
    let exe = std::env::current_exe().expect("this program's path");
    let browser = reference(&exe)
        .entries
        .iter()
        .filter(|e| entry_matches(e, "browser"))
        .count();
    assert_eq!(split, 2, "help: \"split\" finds Split Right and Split Down");
    let palette = shortcuts
        .iter()
        .filter(|r| row_matches(r, "palette"))
        .count();
    assert_eq!(palette, 1, "help: \"palette\" finds Command Palette…");
    for (name, keys) in [
        ("Command Palette\u{2026}", "\u{2318}K"),
        ("Go to Workspace\u{2026}", "\u{2318}P"),
    ] {
        assert!(
            shortcuts.iter().any(|r| r.name == name && r.keys == keys),
            "help: {name} {keys} is listed"
        );
    }
    assert!(
        browser >= 8,
        "help: every browser command matches \"browser\""
    );

    let dims = size(px(960.0), px(600.0));
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        for scene in [
            "help-shortcuts",
            "help-split",
            "help-none",
            "help-palette",
            "help-commands",
            "help-over-palette",
            "help-toggle",
            "help-tips",
        ] {
            let slot: Slot = Rc::new(RefCell::new(None));
            let keep = slot.clone();
            let build = move |window: &mut Window, cx: &mut App| -> AnyView {
                cx.set_global(sidebar_appearance(theme, cx));
                let ws = cx.new(|cx| switcher_workspace(window, cx));
                *keep.borrow_mut() = Some(ws.clone());
                ws.into()
            };
            let n = format!("{scene}-{slug}");
            let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let ws = slotted(&slot);
                cx.update(|cx| {
                    cx.bind_keys([
                        KeyBinding::new("up", ScrollUp, Some("HelpPanel")),
                        KeyBinding::new("down", ScrollDown, Some("HelpPanel")),
                        KeyBinding::new("enter", crate::name_input::Confirm, Some("NameInput")),
                        KeyBinding::new("escape", crate::name_input::Cancel, Some("NameInput")),
                        KeyBinding::new(
                            "cmd-backspace",
                            crate::name_input::Clear,
                            Some("NameInput"),
                        ),
                    ])
                });
                // Frames, so focus changes (and the boxes' blur events) and scrolls run.
                let frames = |cx: &mut VisualTestAppContext| {
                    for _ in 0..3 {
                        cx.update_window(window, |_, window, cx| window.draw(cx).clear())
                            .expect("draw");
                        cx.run_until_parked();
                    }
                };
                let open = |cx: &mut VisualTestAppContext, page: HelpPage| {
                    cx.update_window(window, |_, window, cx| {
                        ws.update(cx, |ws, cx| ws.open_help(page, true, window, cx))
                    })
                    .expect("open the help");
                    frames(cx);
                };
                let press = |cx: &mut VisualTestAppContext, keys: &str| {
                    for key in keys.split(' ') {
                        cx.simulate_keystrokes(window, key);
                        frames(cx);
                    }
                };
                // (page, rows shown, whether its box has the keyboard), or none while closed.
                let help = |cx: &mut VisualTestAppContext| {
                    cx.update_window(window, |_, window, cx| {
                        ws.read(cx).help.clone().map(|h| {
                            let panel = h.read(cx);
                            let focused = panel.focus_handle(cx).is_focused(window);
                            (panel.page(), panel.shown_rows(), focused)
                        })
                    })
                    .expect("read the help")
                };
                let terminal_focused = |cx: &mut VisualTestAppContext| {
                    cx.update_window(window, |_, window, cx| {
                        let ws = ws.read(cx);
                        let active = ws.sessions.active().expect("an active terminal");
                        active.view.focus_handle(cx).is_focused(window)
                    })
                    .expect("read the focus")
                };
                match scene {
                    "help-shortcuts" | "help-split" | "help-none" | "help-palette" => {
                        open(cx, HelpPage::Shortcuts);
                        let (typed, rows) = match scene {
                            "help-split" => ("split", split),
                            "help-none" => ("zzz", 0),
                            "help-palette" => ("palette", palette),
                            _ => ("", all_shortcuts),
                        };
                        if !typed.is_empty() {
                            cx.simulate_input(window, typed);
                        }
                        assert_eq!(
                            help(cx),
                            Some((HelpPage::Shortcuts, rows, true)),
                            "{n}: the rows for {typed:?}, the box with the keyboard"
                        );
                    }
                    "help-commands" => {
                        open(cx, HelpPage::Commands);
                        cx.simulate_input(window, "browser");
                        assert_eq!(
                            help(cx),
                            Some((HelpPage::Commands, browser, true)),
                            "{n}: the browser commands"
                        );
                    }
                    "help-over-palette" => {
                        cx.update_window(window, |_, window, cx| {
                            ws.update(cx, |ws, cx| {
                                ws.open_palette(crate::palette_items::Scope::All, window, cx)
                            })
                        })
                        .expect("open the palette");
                        cx.run_until_parked();
                        open(cx, HelpPage::Shortcuts);
                        assert!(
                            cx.read(|cx| ws.read(cx).palette.is_none()),
                            "{n}: the help closed the palette"
                        );
                        assert_eq!(
                            help(cx),
                            Some((HelpPage::Shortcuts, all_shortcuts, true)),
                            "{n}: the help has the keyboard"
                        );
                    }
                    "help-tips" => {
                        cx.update(|cx| {
                            ws.update(cx, |ws, _| {
                                // `Workspace::for_shots` already holds saving.
                                ws.sidebar.hidden = true;
                                ws.sidebar.hint_dismissed = true;
                            })
                        });
                        cx.update_window(window, |_, window, cx| {
                            ws.update(cx, |ws, cx| {
                                ws.show_welcome_tips(window, cx);
                                // The picture shows where the slide ends, not part way.
                                ws.slide = None;
                            })
                        })
                        .expect("show the tips");
                        frames(cx);
                        let (hidden, dismissed) = cx.read(|cx| {
                            let ws = ws.read(cx);
                            (ws.sidebar.hidden, ws.sidebar.hint_dismissed)
                        });
                        assert!(!hidden, "{n}: the sidebar is shown");
                        assert!(!dismissed, "{n}: the tips card is back");
                    }
                    _ => {
                        // The keys: ↓ scrolls a row, ↑ back; Enter does nothing; Esc closes.
                        open(cx, HelpPage::Shortcuts);
                        let offset = |cx: &mut VisualTestAppContext| {
                            cx.read(|cx| {
                                let help = ws.read(cx).help.clone().expect("the help is open");
                                f32::from(help.read(cx).scroll_offset())
                            })
                        };
                        assert_eq!(offset(cx), 0.0, "{n}: opens at the top");
                        press(cx, "down");
                        let one = offset(cx);
                        assert!(one < 0.0, "{n}: ↓ scrolls the list ({one})");
                        press(cx, "down");
                        let two = offset(cx);
                        assert!(two < one, "{n}: ↓ again scrolls a row more ({two})");
                        press(cx, "up");
                        assert_eq!(offset(cx), one, "{n}: ↑ scrolls a row back");
                        press(cx, "up");
                        assert_eq!(offset(cx), 0.0, "{n}: ↑ again, at the top");
                        press(cx, "enter");
                        assert!(help(cx).is_some(), "{n}: Enter leaves it open");
                        press(cx, "escape");
                        assert_eq!(help(cx), None, "{n}: Esc closes it");
                        assert!(terminal_focused(cx), "{n}: Esc gives the terminal the keys");
                        // The other page replaces the open one.
                        open(cx, HelpPage::Shortcuts);
                        open(cx, HelpPage::Commands);
                        assert!(
                            matches!(help(cx), Some((HelpPage::Commands, _, true))),
                            "{n}: the Command Reference replaced Keyboard Shortcuts"
                        );
                        // ⌘/ twice: open, then closed, the keyboard back to the terminal.
                        open(cx, HelpPage::Shortcuts);
                        assert!(
                            matches!(help(cx), Some((HelpPage::Shortcuts, _, true))),
                            "{n}: opened"
                        );
                        // ↓ while nothing is found queues no scroll: cleared, the list is at
                        // the top.
                        cx.simulate_input(window, "zzz");
                        frames(cx);
                        press(cx, "down");
                        press(cx, "cmd-backspace");
                        assert_eq!(
                            help(cx),
                            Some((HelpPage::Shortcuts, all_shortcuts, true)),
                            "{n}: cleared, every row"
                        );
                        assert_eq!(offset(cx), 0.0, "{n}: cleared, at the top");
                        // From Preferences or a popped-out page (the main window not key),
                        // asking for the shown page keeps it open.
                        cx.update_window(window, |_, window, cx| {
                            ws.update(cx, |ws, cx| {
                                ws.open_help(HelpPage::Shortcuts, false, window, cx)
                            })
                        })
                        .expect("ask again from another window");
                        frames(cx);
                        assert!(
                            matches!(help(cx), Some((HelpPage::Shortcuts, _, true))),
                            "{n}: kept open, its box with the keyboard"
                        );
                        open(cx, HelpPage::Shortcuts);
                        assert_eq!(help(cx), None, "{n}: the second one closed it");
                        assert!(terminal_focused(cx), "{n}: the terminal has the keys again");
                    }
                }
                // Not kept past the window: a handle left over is reported as a leak.
                slot.borrow_mut().take();
            };
            let path = shoot_with(scene, slug, &theme, dims, build, act);
            println!("ui-shots: {}", path.display());
        }
    }
}

/// The host picker and the address box over the same workspace as the palette shots, in Mocha
/// (both themes with `--all-themes`; `host-picker-*`, `address-box-*`), to compare with
/// `palette-*`: one panel look.
/// The picker is given made-up recent hosts and no config hosts, so no `ssh -G` runs and
/// `~/.ssh` is not read. Panics unless the box opened is the one asked for.
fn panel_scenes() {
    use crate::ssh_hosts::Recent;
    use crate::ssh_target::SshTarget;
    use gpui::Focusable;
    use std::cell::RefCell;
    use std::rc::Rc;

    let host = |host: &str, tmux, used_at| Recent {
        target: SshTarget {
            host: host.into(),
            tmux,
            tmux_name: None,
        },
        used_at,
    };
    let dims = size(px(960.0), px(600.0));
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        for scene in ["host-picker", "address-box"] {
            let slot: Slot = Rc::new(RefCell::new(None));
            let keep = slot.clone();
            let build = move |window: &mut Window, cx: &mut App| -> AnyView {
                cx.set_global(sidebar_appearance(theme, cx));
                let ws = cx.new(|cx| switcher_workspace(window, cx));
                *keep.borrow_mut() = Some(ws.clone());
                ws.into()
            };
            let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let ws = slotted(&slot);
                let recent = vec![
                    host("build-box", true, 3),
                    host("deploy@staging.example.com", false, 2),
                    host("pi", false, 1),
                ];
                cx.update_window(window, |_, window, cx| {
                    ws.update(cx, |ws, cx| match scene {
                        "host-picker" => {
                            let picker = cx.new(|cx| {
                                crate::host_picker::HostPicker::new(recent, vec![], window, cx)
                            });
                            picker.focus_handle(cx).focus(window, cx);
                            ws.host_picker = Some(picker);
                            cx.notify();
                        }
                        _ => ws.open_address_box(window, cx),
                    })
                })
                .expect("open it");
                cx.run_until_parked();
                cx.read(|cx| {
                    let ws = ws.read(cx);
                    let open = match scene {
                        "host-picker" => ws.host_picker.is_some(),
                        _ => ws.address_box.is_some(),
                    };
                    assert!(open, "{scene}-{slug}: the box is open");
                });
            };
            let path = shoot_with(scene, slug, &theme, dims, build, act);
            println!("ui-shots: {}", path.display());
        }
    }
}

/// Open Preferences over `ws` on `section` (the tips card `dismissed` or not) and draw.
fn open_preferences_on(
    cx: &mut VisualTestAppContext,
    window: AnyWindowHandle,
    ws: &gpui::Entity<Workspace>,
    section: crate::preferences::Section,
    dismissed: bool,
) {
    cx.update_window(window, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.toggle_preferences(window, cx);
            let panel = ws.preferences.clone().expect("Preferences opened");
            panel.update(cx, |panel, cx| {
                panel.shown_as(section, dismissed);
                cx.notify();
            });
        })
    })
    .expect("open Preferences");
    settle(cx, window);
}

/// Preferences as a panel over the window (M11x): each section in both themes
/// (`preferences-<section>-mocha`, `-latte`; General with the tips card dismissed, so its button
/// is live) and `preferences-small-mocha` (a window smaller than the panel). Not shot: the keys.
///
/// Panics unless, outside the panel, the window is dimmed (each pixel the plain window's pulled
/// toward the shadow colour by `SCRIM_ALPHA`), the page's top is the panel's colour, the section
/// list has exactly one filled row, General's switches show the accent (they're on), and
/// Appearance's preview shows the theme's background; small: the panel starts 24 pt in and the
/// page scrolls; keys: Esc closes it, ⌘W closes it and leaves every terminal open, and while it's
/// open the keyboard is the panel's, not a terminal's.
fn preferences_scenes() {
    use crate::preferences::{SCRIM_ALPHA, Section, panel_size};
    use alacritty_terminal::vte::ansi::Rgb;
    use std::cell::RefCell;
    use std::rc::Rc;

    // Wide and tall enough that the sampled points outside the panel are clear of its shadow.
    let (w, h) = (1400.0, 900.0);
    let dims = size(px(w), px(h));
    let (pw, ph) = panel_size(w, h);
    let (x0, y0) = ((w - pw) / 2.0, (h - ph) / 2.0);
    // The section list's width (`preferences::NAV_WIDTH`).
    let nav = 200.0;
    let near = |p: [u8; 3], c: Rgb, by: u8| {
        p.iter()
            .zip([c.r, c.g, c.b])
            .all(|(a, b)| a.abs_diff(b) <= by)
    };
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let ui = theme.ui();
        // The window without the panel: what the scrim dims.
        let plain = {
            let slot: Slot = Rc::new(RefCell::new(None));
            render(&theme, dims, palette_build(theme, slot))
        };
        for section in Section::ALL {
            let scene = format!(
                "preferences-{}-{slug}",
                format!("{section:?}").to_lowercase()
            );
            let slot: Slot = Rc::new(RefCell::new(None));
            let build = palette_build(theme, slot.clone());
            let act = {
                let slot = slot.clone();
                move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                    let ws = slotted(&slot);
                    open_preferences_on(cx, window, &ws, section, true);
                    slot.borrow_mut().take();
                }
            };
            let path = shoot_with(
                "preferences",
                &format!("{}-{slug}", format!("{section:?}").to_lowercase()),
                &theme,
                dims,
                build.clone(),
                act.clone(),
            );
            let shot = render_with(&theme, dims, build, act);
            // Outside the panel: dimmed.
            for (x, y) in [(60.0, h / 2.0), (w - 60.0, h / 2.0), (w / 2.0, 30.0)] {
                let before = pixel_at(&plain, dims, x, y);
                let dim = |a: u8, s: u8| {
                    (f32::from(a) * (1.0 - SCRIM_ALPHA) + f32::from(s) * SCRIM_ALPHA).round() as u8
                };
                let want = Rgb {
                    r: dim(before[0], ui.shadow.r),
                    g: dim(before[1], ui.shadow.g),
                    b: dim(before[2], ui.shadow.b),
                };
                let got = pixel_at(&shot, dims, x, y);
                assert!(
                    near(got, want, 6),
                    "{scene}: ({x}, {y}) isn't dimmed: {got:?}, want {want:?}"
                );
            }
            // The page's top padding is the panel's colour.
            let top = pixel_at(&shot, dims, x0 + nav + 20.0, y0 + 12.0);
            assert!(near(top, ui.panel, 3), "{scene}: the page's top {top:?}");
            // The section list: exactly one filled row (the shown one), right of the names.
            let bands = (0..(ph * 2.0) as usize)
                .map(|i| y0 + i as f32 / 2.0)
                .map(|y| near(pixel_at(&shot, dims, x0 + nav - 24.0, y), ui.row_active, 3))
                .collect::<Vec<_>>()
                .windows(2)
                .filter(|pair| !pair[0] && pair[1])
                .count();
            assert_eq!(bands, 1, "{scene}: {bands} filled rows in the section list");
            // The page's area, sampled every 2 pt.
            let page_has = |c: Rgb, by: u8| {
                let mut y = y0 + 60.0;
                while y < y0 + ph - 20.0 {
                    let mut x = x0 + nav + 20.0;
                    while x < x0 + pw - 20.0 {
                        if near(pixel_at(&shot, dims, x, y), c, by) {
                            return true;
                        }
                        x += 2.0;
                    }
                    y += 2.0;
                }
                false
            };
            if section == Section::General {
                assert!(page_has(ui.accent, 6), "{scene}: no switch is on");
            }
            if section == Section::Appearance {
                assert!(
                    page_has(theme.background, 2),
                    "{scene}: no terminal background in the preview"
                );
            }
            println!("ui-shots: {}", path.display());
        }
    }

    // A window smaller than the panel: 24 pt in, and the page scrolls.
    let theme = shot_theme(shot_themes()[0].1);
    let ui = theme.ui();
    let (sw, sh) = (760.0, 520.0);
    let small = size(px(sw), px(sh));
    let slot: Slot = Rc::new(RefCell::new(None));
    let build = palette_build(theme, slot.clone());
    let act = {
        let slot = slot.clone();
        move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
            let ws = slotted(&slot);
            open_preferences_on(cx, window, &ws, Section::Appearance, true);
            let scroll = cx.read(|cx| {
                ws.read(cx)
                    .preferences
                    .as_ref()
                    .map(|p| p.read(cx).scroll_for_shots())
                    .expect("open")
            });
            assert!(
                f32::from(scroll.max_offset().y) > 0.0,
                "preferences-small: the page doesn't scroll"
            );
            slot.borrow_mut().take();
        }
    };
    let path = shoot_with(
        "preferences",
        "small-mocha",
        &theme,
        small,
        build.clone(),
        act.clone(),
    );
    // The Sound page in the small window: its rows' controls under their words (review fix).
    let slot: Slot = Rc::new(RefCell::new(None));
    let sound_build = palette_build(theme, slot.clone());
    let sound_act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
        let ws = slotted(&slot);
        open_preferences_on(cx, window, &ws, Section::Sound, true);
        slot.borrow_mut().take();
    };
    let sound_path = shoot_with(
        "preferences",
        "small-sound-mocha",
        &theme,
        small,
        sound_build,
        sound_act,
    );
    println!("ui-shots: {}", sound_path.display());
    let shot = render_with(&theme, small, build, act);
    let (spw, _) = panel_size(sw, sh);
    assert_eq!(spw, sw - 48.0, "preferences-small: the panel's width");
    let inside = pixel_at(&shot, small, 24.0 + 4.0, sh / 2.0);
    assert!(
        near(inside, ui.sidebar_background, 3),
        "preferences-small: the section list doesn't start 24 pt in: {inside:?}"
    );
    let outside = pixel_at(&shot, small, 24.0 - 6.0, sh / 2.0);
    assert!(
        !near(outside, ui.sidebar_background, 3),
        "preferences-small: the panel starts left of 24 pt"
    );
    println!("ui-shots: {}", path.display());

    // The keys.
    let slot: Slot = Rc::new(RefCell::new(None));
    let build = palette_build(theme, slot.clone());
    let act = {
        let slot = slot.clone();
        move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
            let ws = slotted(&slot);
            let is_open =
                |cx: &mut VisualTestAppContext| cx.read(|cx| ws.read(cx).preferences.is_some());
            let sessions =
                |cx: &mut VisualTestAppContext| cx.read(|cx| ws.read(cx).sessions.items().len());
            let before = sessions(cx);
            // Esc closes it.
            open_preferences_on(cx, window, &ws, Section::General, true);
            assert!(is_open(cx), "preferences-keys: opened");
            press_keys(cx, window, "escape");
            assert!(!is_open(cx), "preferences-keys: Esc closes it");
            // ⌘, opens and closes it.
            press_keys(cx, window, "cmd-,");
            assert!(is_open(cx), "preferences-keys: ⌘, opens it");
            press_keys(cx, window, "cmd-,");
            assert!(!is_open(cx), "preferences-keys: ⌘, closes it");
            // A click inside the panel (the nav's Appearance row) is the panel's: it stays open and
            // shows Appearance (the scrim behind it gets only clicks outside it).
            open_preferences_on(cx, window, &ws, Section::General, true);
            let (pw, ph) = panel_size(w, h);
            let (x0, y0) = ((w - pw) / 2.0, (h - ph) / 2.0);
            // Appearance's row: under "Settings" (18 + 21 pt) and General's row (32 + 2 pt).
            let row = point(px(x0 + 100.0), px(y0 + 18.0 + 21.0 + 34.0 + 16.0));
            cx.simulate_click(window, row, gpui::Modifiers::none());
            settle(cx, window);
            assert!(is_open(cx), "preferences-keys: a click inside closed it");
            let shown = cx.read(|cx| {
                ws.read(cx)
                    .preferences
                    .as_ref()
                    .map(|p| p.read(cx).section())
            });
            assert_eq!(
                shown,
                Some(Section::Appearance),
                "preferences-keys: the nav's click"
            );
            press_keys(cx, window, "escape");
            // While it's open the keyboard is the panel's; ⌘W closes it, never a terminal.
            open_preferences_on(cx, window, &ws, Section::General, true);
            let focused = cx
                .update_window(window, |_, window, cx| {
                    ws.read(cx).preferences.as_ref().map(|p| {
                        gpui::Focusable::focus_handle(p.read(cx), cx).contains_focused(window, cx)
                    })
                })
                .expect("read focus");
            assert_eq!(
                focused,
                Some(true),
                "preferences-keys: the panel has the keys"
            );
            press_keys(cx, window, "cmd-w");
            assert!(!is_open(cx), "preferences-keys: ⌘W closes it");
            assert_eq!(
                sessions(cx),
                before,
                "preferences-keys: ⌘W closed a terminal"
            );
            // A terminal takes the keyboard while the panel is up (a script, a shortcut that
            // focuses one): the panel goes, and ⌘W still never closes that terminal unseen.
            // (The panel also closes when the keyboard leaves it, but an off-screen window is never
            // active, so GPUI tells it no focus-out: here ⌘W with a terminal focused under it.)
            open_preferences_on(cx, window, &ws, Section::General, true);
            cx.update_window(window, |_, window, cx| {
                ws.update(cx, |ws, cx| ws.focus_active(window, cx))
            })
            .expect("focus a terminal");
            settle(cx, window);
            press_keys(cx, window, "cmd-w");
            assert!(!is_open(cx), "preferences-keys: ⌘W left the panel up");
            assert_eq!(
                sessions(cx),
                before,
                "preferences-keys: ⌘W closed a terminal"
            );
            slot.borrow_mut().take();
        }
    };
    render_with(&theme, dims, build, act);
    println!("ui-shots: preferences-keys ok");
    preferences_plugins_scene();
}

/// `preferences-plugins-installed-<theme>`: the Plugins page with one installed plugin (no
/// network: the records are made up here) and, in one of two renders, a newer version waiting.
///
/// Two helper plugins that run code are listed too, with the second on in one render and off in
/// another.
///
/// Panics unless the page draws, the waiting update changes it (the Update button is there) and
/// so does switching the second helper off (its On/Off switch is there).
fn preferences_plugins_scene() {
    use crate::plugin_source::Installed;
    use crate::plugin_store::Record;
    use crate::plugins::Plugins;
    use crate::preferences::Section;
    use std::cell::RefCell;
    use std::rc::Rc;

    let dims = size(px(1400.0), px(900.0));
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let mut shots = Vec::new();
        // (a newer version waiting, the second helper on)
        for (with_update, second_on) in [(false, true), (true, true), (false, false), (true, false)]
        {
            let slot: Slot = Rc::new(RefCell::new(None));
            let build = palette_build(theme, slot.clone());
            let act = {
                let slot = slot.clone();
                move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                    let ws = slotted(&slot);
                    cx.update(|cx| {
                        let helper = |id: &str, name: &str, enabled: bool| Record {
                            id: id.into(),
                            name: name.into(),
                            version: "0.3.0".into(),
                            // As long as fuxx-completes' own: a description that has to wrap
                            // beside the switch and the buttons (it ran under them, 2026-11-13).
                            description: Some(
                                "Grey ghost-text suggestions from your history as you type (zsh)."
                                    .into(),
                            ),
                            repo: format!("someone/fuxx-{id}"),
                            installed: Installed::Tag("v0.3.0".into()),
                            themes: vec![],
                            installed_at: 0,
                            enabled,
                            bin: Some("bin".into()),
                            zsh: vec!["init.zsh".into()],
                        };
                        let mut plugins = Plugins {
                            records: vec![
                                Record {
                                    id: "sunset".into(),
                                    name: "Sunset".into(),
                                    version: "1.0.0".into(),
                                    description: Some("Warm evening colours.".into()),
                                    repo: "someone/fuxx-sunset".into(),
                                    installed: Installed::Tag("v1.0.0".into()),
                                    themes: vec!["Sunset".into(), "Sunset Light".into()],
                                    installed_at: 0,
                                    enabled: true,
                                    bin: None,
                                    zsh: vec![],
                                },
                                helper("tidy", "Tidy", true),
                                helper("sparkle", "Sparkle", second_on),
                            ],
                            ..Plugins::default()
                        };
                        if with_update {
                            plugins
                                .updates
                                .insert("sunset".into(), Installed::Tag("v1.1.0".into()));
                            // A helper with its switch, an update and Uninstall: the widest card.
                            plugins
                                .updates
                                .insert("tidy".into(), Installed::Tag("v0.3.1".into()));
                        }
                        cx.set_global(plugins);
                    });
                    open_preferences_on(cx, window, &ws, Section::Plugins, true);
                    slot.borrow_mut().take();
                }
            };
            if with_update && !second_on {
                let path = shoot_with(
                    "preferences",
                    &format!("plugins-installed-{slug}"),
                    &theme,
                    dims,
                    build.clone(),
                    act.clone(),
                );
                println!("ui-shots: {}", path.display());
            }
            shots.push(render_with(&theme, dims, build, act));
        }
        assert_ne!(
            shots[0].rgba, shots[1].rgba,
            "preferences-plugins-installed-{slug}: a waiting update doesn't change the page"
        );
        assert_ne!(
            shots[0].rgba, shots[2].rgba,
            "preferences-plugins-installed-{slug}: switching a helper off doesn't change the page"
        );
    }
}
