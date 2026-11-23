//! The command palette (M11j): its scopes, keys and toggles, actions, Recent, Files, Settings,
//! a narrow window and Reduce motion.

use super::*;

/// This file, as `file!()` gives it (repo-relative under cargo): `registry::AREAS` pairs it with
/// `SCENES`, so `--changed` runs these scenes when this file changes.
pub(super) const FILE: &str = file!();

/// This area's scenes (`registry.rs` runs them in its `ORDER`).
pub(super) const SCENES: &[Scene] = &[
    Scene {
        name: "palette",
        run: palette_scene,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/palette.rs",
            "src/palette_items.rs",
            "src/palette_match.rs",
            "src/overlay_panel.rs",
            "src/name_input.rs",
            "src/help_shortcuts.rs",
            "src/switcher.rs",
        ],
    },
    Scene {
        name: "palette-workspaces",
        run: palette_workspaces_scene,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: &[
            "src/palette.rs",
            "src/palette_items.rs",
            "src/palette_match.rs",
            "src/overlay_panel.rs",
            "src/name_input.rs",
            "src/switcher.rs",
            "src/layout_model.rs",
        ],
    },
    Scene {
        name: "palette-actions",
        run: palette_actions_scene,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/palette.rs",
            "src/palette_items.rs",
            "src/palette_match.rs",
            "src/overlay_panel.rs",
            "src/name_input.rs",
            "src/help_shortcuts.rs",
            "src/layout_model.rs",
        ],
    },
    Scene {
        name: "palette-keys",
        run: palette_keys_scene,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: &[
            "src/palette.rs",
            "src/palette_items.rs",
            "src/palette_match.rs",
            "src/overlay_panel.rs",
            "src/name_input.rs",
            "src/switcher.rs",
            "src/palette_files.rs",
            "src/layout_model.rs",
        ],
    },
    Scene {
        name: "palette-run",
        run: palette_run_scene,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: &[
            "src/palette.rs",
            "src/palette_items.rs",
            "src/palette_match.rs",
            "src/overlay_panel.rs",
            "src/name_input.rs",
            "src/file_tab.rs",
            "src/layout_model.rs",
        ],
    },
    Scene {
        name: "palette-refresh",
        run: palette_refresh_scene,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: &[
            "src/palette.rs",
            "src/palette_items.rs",
            "src/palette_match.rs",
            "src/overlay_panel.rs",
            "src/name_input.rs",
            "src/switcher.rs",
            "src/status.rs",
        ],
    },
    Scene {
        name: "palette-recent",
        run: palette_recent_scene,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: &[
            "src/palette.rs",
            "src/palette_items.rs",
            "src/palette_match.rs",
            "src/overlay_panel.rs",
            "src/name_input.rs",
            "src/switcher.rs",
            "src/palette_files.rs",
        ],
    },
    Scene {
        name: "palette-files",
        run: palette_files_scene,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/palette.rs",
            "src/palette_items.rs",
            "src/palette_match.rs",
            "src/overlay_panel.rs",
            "src/name_input.rs",
            "src/palette_files.rs",
            "src/file_tab.rs",
            "src/right_sidebar.rs",
            "src/ssh_target.rs",
            "src/layout_model.rs",
        ],
    },
    Scene {
        name: "palette-relist",
        run: palette_relist_scene,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: &[
            "src/palette.rs",
            "src/palette_items.rs",
            "src/palette_match.rs",
            "src/overlay_panel.rs",
            "src/name_input.rs",
            "src/palette_files.rs",
        ],
    },
    Scene {
        name: "palette-settings",
        run: palette_settings_scene,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/palette.rs",
            "src/palette_items.rs",
            "src/palette_match.rs",
            "src/overlay_panel.rs",
            "src/name_input.rs",
            "src/preferences.rs",
        ],
    },
    Scene {
        name: "palette-narrow",
        run: palette_narrow_scene,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/palette.rs",
            "src/palette_items.rs",
            "src/palette_match.rs",
            "src/overlay_panel.rs",
            "src/name_input.rs",
            "src/palette_files.rs",
        ],
    },
    Scene {
        name: "palette-motion",
        run: palette_motion_scene,
        themes: Themes::One,
        // Serial: (c) nothing moves over 200 ms of real time.
        serial: true,
        kind: Kind::Visual,
        sources: &[
            "src/palette.rs",
            "src/palette_items.rs",
            "src/palette_match.rs",
            "src/overlay_panel.rs",
            "src/name_input.rs",
            "src/switcher.rs",
            "src/mark_view.rs",
            "src/status_mark.rs",
            "src/reduce_motion.rs",
        ],
    },
];

/// The open palette's lines, scope and selected row's key.
fn palette_state(
    cx: &mut VisualTestAppContext,
    ws: &gpui::Entity<Workspace>,
) -> (
    Vec<crate::palette_items::Line>,
    crate::palette_items::Scope,
    Option<crate::palette_items::Key>,
) {
    let palette = palette_of(cx, ws).expect("the palette is open");
    cx.read(|cx| {
        let p = palette.read(cx);
        (p.lines().to_vec(), p.scope(), p.selected_key().cloned())
    })
}

/// The names of the workspaces the open palette lists, in order.
fn palette_names(cx: &mut VisualTestAppContext, ws: &gpui::Entity<Workspace>) -> Vec<String> {
    let palette = palette_of(cx, ws).expect("the palette is open");
    cx.read(|cx| {
        palette
            .read(cx)
            .shown_workspaces()
            .map(|e| e.label.clone())
            .collect()
    })
}

/// The workspaces' ids in display order (#1 first) and the id of the one named `label`.
fn workspace_ids(cx: &mut VisualTestAppContext, ws: &gpui::Entity<Workspace>) -> Vec<u64> {
    cx.read(|cx| {
        ws.read(cx)
            .model
            .display_order()
            .iter()
            .map(|w| w.0)
            .collect()
    })
}

/// The ⌘K palette over `switcher_workspace` (shell's terminal focused), in both themes:
/// `palette`, just opened on All: Workspaces (the five most urgent, then `… 3 more
/// workspaces`) and Actions (the first five, then how many more), no Recent yet.
///
/// Panics unless the lines are exactly those, the first row is selected and filled
/// `row_active` (the next one isn't), the `All` chip is filled `row_active` (the Actions chip
/// isn't), and the panel is 600 pt wide.
fn palette_scene() {
    use crate::help_shortcuts::binding_pairs;
    use crate::palette_items::{Focused, Key, Kind, Line, Scope, action_rows};
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    type Seen = (
        gpui::Bounds<Pixels>,
        [gpui::Bounds<Pixels>; 2],
        [gpui::Bounds<Pixels>; 2],
    );
    let dims = size(px(960.0), px(600.0));
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let ui = theme.ui();
        let slot: Slot = Rc::new(RefCell::new(None));
        let build = palette_build(theme, slot.clone());
        let seen: Rc<Cell<Option<Seen>>> = Rc::default();
        let found = seen.clone();
        let n = format!("palette-{slug}");
        let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
            let ws = slotted(&slot);
            let ids = workspace_ids(cx, &ws);
            open_palette_on(cx, window, &ws, Scope::All);
            // The actions a terminal has with persistent sessions off, the presets' rows in.
            let presets: Vec<String> = cx.read(|cx| {
                cx.global::<Appearance>()
                    .presets
                    .iter()
                    .map(|p| p.name.clone())
                    .collect()
            });
            let mut bindings = crate::new_bindings().to_vec();
            bindings.extend(crate::key_bindings());
            let mut actions = action_rows(
                Focused::Terminal,
                false,
                false, // no repo: `switcher_workspace` has no branch
                &presets,
                &binding_pairs(&bindings),
            );
            // M11aa: its workspaces are this Mac's, so New Claude Tab is listed too.
            crate::palette_items::add_agent_rows(&mut actions, true);
            let item = |key| Line::Item {
                key,
                positions: Vec::new(),
            };
            let mut want = vec![Line::Header("Workspaces".into())];
            // claude, agent, tests, build, codex: most urgent first.
            want.extend([0, 4, 2, 5, 1].map(|i| item(Key::Workspace(ids[i]))));
            want.push(Line::More {
                kind: Kind::Workspace,
                count: 3,
            });
            want.push(Line::Header("Actions".into()));
            want.extend(
                [
                    "New Workspace",
                    "New Workspace in Folder\u{2026}",
                    "New SSH Workspace\u{2026}",
                    "Rename Workspace\u{2026}",
                    "Move Workspace Up",
                ]
                .map(|a| item(Key::Action(a))),
            );
            want.push(Line::More {
                kind: Kind::Action,
                count: actions.len() - 5,
            });
            let (lines, scope, selected) = palette_state(cx, &ws);
            assert_eq!(lines, want, "{n}: the lines");
            assert_eq!(scope, Scope::All, "{n}: on All");
            assert_eq!(selected, Some(Key::Workspace(ids[0])), "{n}: claude chosen");
            let palette = palette_of(cx, &ws).expect("open");
            found.set(cx.read(|cx| {
                let p = palette.read(cx);
                Some((
                    p.panel_bounds()?,
                    [p.row_bounds(1)?, p.row_bounds(2)?],
                    [p.chip_bounds(Scope::All)?, p.chip_bounds(Scope::Actions)?],
                ))
            }));
            slot.borrow_mut().take();
        };
        let shot = render_with(&theme, dims, build, act);
        save_shot("palette", slug, &shot);
        let (panel, [first, second], [all, actions]) = seen.get().expect("drawn");
        let n = format!("palette-{slug}");
        // Inside its 1 pt edge (what an absolute child fills): 600 pt with it, centred in the
        // window less the 16 pt each side `top_panel` keeps.
        let outer = f32::from(panel.size.width) + 2.0;
        assert!(
            (outer - 600.0).abs() < 0.5,
            "{n}: the panel is {outer} wide"
        );
        let left = f32::from(panel.left()) - 1.0;
        assert!((left - 180.0).abs() < 0.5, "{n}: centred ({left})");
        // In a row's left padding, and in a chip's: their fill, clear of any text.
        let fill = |b: gpui::Bounds<Pixels>, inset: f32| {
            pixel_at(
                &shot,
                dims,
                f32::from(b.left()) + inset,
                f32::from(b.center().y),
            )
        };
        assert!(
            near(fill(first, 4.0), ui.row_active),
            "{n}: the first row is chosen"
        );
        assert!(near(fill(second, 4.0), ui.panel), "{n}: the second isn't");
        assert!(
            near(fill(all, 3.0), ui.row_active),
            "{n}: the All chip is lit"
        );
        assert!(
            near(fill(actions, 3.0), ui.panel),
            "{n}: the Actions chip isn't"
        );
    }
}

/// ⌘P's list in the palette (its Workspaces scope), in Mocha (both themes with `--all-themes`):
/// `palette-workspaces`: "build" typed, which finds web's build by its name and build-box's two
/// by their group.
///
/// Panics unless, just opened, the list is the old ⌘P one (every workspace, most urgent first,
/// under a Workspaces heading), "build" leaves build, deploy and logs, and (not shot) Enter goes
/// to build and closes the palette, its terminal with the keyboard.
fn palette_workspaces_scene() {
    use crate::palette_items::{Line, Scope};
    use std::cell::RefCell;
    use std::rc::Rc;

    let dims = size(px(960.0), px(600.0));
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        for enter in [false, true] {
            let slot: Slot = Rc::new(RefCell::new(None));
            let build = palette_build(theme, slot.clone());
            let n = format!("palette-workspaces-{slug}");
            let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let ws = slotted(&slot);
                let ids = workspace_ids(cx, &ws);
                open_palette_on(cx, window, &ws, Scope::Workspaces);
                let (lines, scope, _) = palette_state(cx, &ws);
                assert_eq!(scope, Scope::Workspaces, "{n}");
                assert_eq!(lines[0], Line::Header("Workspaces".into()), "{n}");
                assert_eq!(
                    palette_names(cx, &ws),
                    [
                        "claude", "agent", "tests", "build", "codex", "deploy", "shell", "logs"
                    ],
                    "{n}: most urgent first"
                );
                cx.simulate_input(window, "build");
                settle(cx, window);
                assert_eq!(
                    palette_names(cx, &ws),
                    ["build", "deploy", "logs"],
                    "{n}: by name and group"
                );
                if enter {
                    press_keys(cx, window, "enter");
                    assert!(palette_of(cx, &ws).is_none(), "{n}: Enter closes it");
                    let focused = cx.read(|cx| ws.read(cx).model.focused_workspace());
                    assert_eq!(focused.map(|w| w.0), Some(ids[5]), "{n}: build is shown");
                    let active = cx.read(|cx| ws.read(cx).sessions.active().map(|s| s.label()));
                    assert_eq!(active.as_deref(), Some("build"), "{n}: its terminal");
                    assert!(
                        active_terminal_focused(cx, window, &ws),
                        "{n}: with the keys"
                    );
                }
                slot.borrow_mut().take();
            };
            let shot = render_with(&theme, dims, build, act);
            if !enter {
                save_shot("palette-workspaces", slug, &shot);
            }
        }
    }
}

/// Actions from the palette, in both themes:
/// - `palette-actions`: "split r" typed: Split Right first, ⌘D on its right;
/// - `palette-actions-prefix`: ">" typed on All: the Actions chip lit.
///
/// Panics unless Split Right is the first row with its keys drawn in `text_dim`, (not shot)
/// Enter splits shell's pane (two panes, a terminal with the keyboard, the palette closed), and
/// ">" puts the palette on Actions with that chip filled `row_active` (All's not).
fn palette_actions_scene() {
    use crate::palette_items::{Key, Scope, selectable};
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    let dims = size(px(960.0), px(600.0));
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let ui = theme.ui();
        for stage in ["shot", "enter", "prefix"] {
            let slot: Slot = Rc::new(RefCell::new(None));
            let build = palette_build(theme, slot.clone());
            let seen: Rc<Cell<Option<[gpui::Bounds<Pixels>; 2]>>> = Rc::default();
            let found = seen.clone();
            let n = format!("palette-actions-{stage}-{slug}");
            let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let ws = slotted(&slot);
                open_palette_on(cx, window, &ws, Scope::All);
                if stage == "prefix" {
                    cx.simulate_input(window, ">");
                    settle(cx, window);
                    let (_, scope, _) = palette_state(cx, &ws);
                    assert_eq!(scope, Scope::Actions, "{n}: > searches Actions");
                    let palette = palette_of(cx, &ws).expect("open");
                    found.set(cx.read(|cx| {
                        let p = palette.read(cx);
                        Some([p.chip_bounds(Scope::Actions)?, p.chip_bounds(Scope::All)?])
                    }));
                    slot.borrow_mut().take();
                    return;
                }
                cx.simulate_input(window, "split r");
                settle(cx, window);
                let (lines, _, selected) = palette_state(cx, &ws);
                let first = selectable(&lines)[0];
                assert_eq!(selected, Some(Key::Action("Split Right")), "{n}: first");
                let palette = palette_of(cx, &ws).expect("open");
                let keys = cx.read(|cx| palette.read(cx).action_keys("Split Right"));
                assert_eq!(keys.as_deref(), Some("\u{2318}D"), "{n}: its keys");
                let row = cx.read(|cx| palette.read(cx).row_bounds(first));
                found.set(row.map(|r| [r, r]));
                if stage == "enter" {
                    // A first listing in (the scenes list nothing themselves): Enter doesn't
                    // wait for one.
                    seed_palette_files(cx, &ws, &[]);
                    let before = shown_panes(cx, &ws).len();
                    press_keys(cx, window, "enter");
                    assert!(palette_of(cx, &ws).is_none(), "{n}: Enter closes it");
                    assert_eq!(before, 1, "{n}: one pane before");
                    assert_eq!(shown_panes(cx, &ws).len(), 2, "{n}: split");
                    let terminal = cx.read(|cx| ws.read(cx).focused_terminal());
                    assert!(terminal.is_some(), "{n}: a terminal is focused");
                    assert!(
                        active_terminal_focused(cx, window, &ws),
                        "{n}: with the keys"
                    );
                }
                slot.borrow_mut().take();
            };
            let shot = render_with(&theme, dims, build, act);
            match stage {
                "shot" => {
                    save_shot("palette-actions", slug, &shot);
                    let [row, _] = seen.get().expect("drawn");
                    // ⌘D, right-aligned in `text_dim`: some of its pixels in the row's last
                    // 60 pt (anti-aliasing aside, a glyph's stems are the colour itself).
                    let right = f32::from(row.right());
                    let ink: usize = (0..12)
                        .map(|k| f32::from(row.top()) + 8.0 + k as f32)
                        .map(|y| count_along(&shot, dims, ui.text_dim, y, right - 60.0, right))
                        .sum();
                    assert!(ink > 0, "palette-actions-{slug}: ⌘D is drawn ({ink})");
                }
                "prefix" => {
                    save_shot("palette-actions-prefix", slug, &shot);
                    let [actions, all] = seen.get().expect("drawn");
                    let fill = |b: gpui::Bounds<Pixels>| {
                        pixel_at(
                            &shot,
                            dims,
                            f32::from(b.left()) + 3.0,
                            f32::from(b.center().y),
                        )
                    };
                    assert!(near(fill(actions), ui.row_active), "{slug}: Actions lit");
                    assert!(near(fill(all), ui.panel), "{slug}: All isn't");
                }
                _ => {}
            }
        }
    }
}

/// The palette's keys and toggles (not shot): ↓ and ↑ move through the selectable rows (the
/// `… more` rows among them), round at both ends; Tab and ⇧Tab change scope, round; ⌘K on All
/// closes it, ⌘K then ⌘P puts it on Workspaces with the text kept; Esc closes it, the keyboard
/// back to the terminal; Enter on `… 3 more workspaces` goes to Workspaces and stays open; a
/// replaced palette's late Cancel is ignored; a click on a chip changes scope; ⌘K with ">"
/// typed on All still closes it (the toggle follows the chosen chip). Panics on the first that
/// doesn't hold.
fn palette_keys_scene() {
    use crate::palette_items::{Key, Kind, LISTING, Line, Scope};
    use gpui::Modifiers;
    use std::cell::RefCell;
    use std::rc::Rc;

    let dims = size(px(960.0), px(600.0));
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let slot: Slot = Rc::new(RefCell::new(None));
        let build = palette_build(theme, slot.clone());
        let n = format!("palette-keys-{slug}");
        let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
            let ws = slotted(&slot);
            let ids = workspace_ids(cx, &ws);
            let selected = |cx: &mut VisualTestAppContext| palette_state(cx, &ws).2;
            let scope = |cx: &mut VisualTestAppContext| palette_state(cx, &ws).1;
            press_keys(cx, window, "cmd-k");
            assert_eq!(scope(cx), Scope::All, "{n}: ⌘K opens All");
            assert_eq!(selected(cx), Some(Key::Workspace(ids[0])), "{n}");
            // Over the `… more` row, into Actions; then up past the top, round to the last.
            press_keys(cx, window, "down down down down down");
            assert_eq!(
                selected(cx),
                Some(Key::More(Kind::Workspace)),
                "{n}: ↓ to more"
            );
            press_keys(cx, window, "down");
            assert_eq!(
                selected(cx),
                Some(Key::Action("New Workspace")),
                "{n}: past it"
            );
            press_keys(cx, window, "up up up up up up up");
            assert_eq!(selected(cx), Some(Key::More(Kind::Action)), "{n}: ↑ wraps");
            press_keys(cx, window, "down");
            assert_eq!(selected(cx), Some(Key::Workspace(ids[0])), "{n}: ↓ wraps");
            // Scopes, round both ways.
            press_keys(cx, window, "tab");
            assert_eq!(scope(cx), Scope::Actions, "{n}: Tab");
            press_keys(cx, window, "shift-tab shift-tab");
            assert_eq!(scope(cx), Scope::Settings, "{n}: ⇧Tab ×2");
            // ⌘K elsewhere puts it on All; on All it closes.
            press_keys(cx, window, "cmd-k");
            assert_eq!(scope(cx), Scope::All, "{n}: ⌘K back to All");
            press_keys(cx, window, "cmd-k");
            assert!(palette_of(cx, &ws).is_none(), "{n}: ⌘K on All closes");
            // ⌘K, "co", ⌘P: Workspaces, with "co" still typed (codex only).
            press_keys(cx, window, "cmd-k");
            cx.simulate_input(window, "co");
            settle(cx, window);
            press_keys(cx, window, "cmd-p");
            assert_eq!(scope(cx), Scope::Workspaces, "{n}: ⌘P rescopes");
            assert_eq!(palette_names(cx, &ws), ["codex"], "{n}: the text kept");
            press_keys(cx, window, "escape");
            assert!(palette_of(cx, &ws).is_none(), "{n}: Esc closes");
            assert!(
                active_terminal_focused(cx, window, &ws),
                "{n}: Esc, keys back"
            );
            // Enter on `… 3 more workspaces`: Workspaces, still open.
            press_keys(cx, window, "cmd-k down down down down down enter");
            assert_eq!(scope(cx), Scope::Workspaces, "{n}: more goes to Workspaces");
            assert_eq!(palette_names(cx, &ws).len(), 8, "{n}: every workspace");
            // A palette closed and replaced still reporting "lost focus" (as its box does once the
            // next frame sees the focus gone): ignored, the new one stays open with the keys.
            let old = palette_of(cx, &ws).expect("open");
            press_keys(cx, window, "cmd-p");
            assert!(
                palette_of(cx, &ws).is_none(),
                "{n}: ⌘P on Workspaces closes"
            );
            press_keys(cx, window, "cmd-k");
            let new = palette_of(cx, &ws).expect("opened again");
            assert_ne!(old, new, "{n}: a new palette");
            cx.update(|cx| old.update(cx, |_, cx| cx.emit(crate::palette::PaletteEvent::Cancel)));
            settle(cx, window);
            assert_eq!(
                palette_of(cx, &ws),
                Some(new.clone()),
                "{n}: the old one's is ignored"
            );
            drop(old);
            press_keys(cx, window, "cmd-p");
            // A click on the Files chip: Files, its first listing not in yet (the screenshots list
            // nothing themselves).
            let palette = palette_of(cx, &ws).expect("open");
            let chip = cx.read(|cx| palette.read(cx).chip_bounds(Scope::Files));
            let chip = chip.expect("the Files chip is drawn");
            cx.simulate_click(window, chip.center(), Modifiers::none());
            settle(cx, window);
            let (lines, scope_now, _) = palette_state(cx, &ws);
            assert_eq!(scope_now, Scope::Files, "{n}: a chip click");
            assert_eq!(lines, [Line::Note(LISTING.into())], "{n}: the note");
            // ⌘K with ">" typed on All (Actions shown): the chosen chip is All, so ⌘K closes.
            press_keys(cx, window, "escape cmd-k");
            cx.simulate_input(window, ">");
            settle(cx, window);
            assert_eq!(scope(cx), Scope::Actions, "{n}: > shows Actions");
            press_keys(cx, window, "cmd-k");
            assert!(palette_of(cx, &ws).is_none(), "{n}: ⌘K on All (>) closes");
            // ">" typed on All (Actions shown), then a scope chosen: the ">" goes, the rest
            // stays. ⇧Tab steps from the shown Actions to All; the All chip does the same; Tab
            // goes on to Workspaces.
            let typed = |cx: &mut VisualTestAppContext| {
                let palette = palette_of(cx, &ws).expect("open");
                cx.read(|cx| palette.read(cx).box_text(cx))
            };
            press_keys(cx, window, "cmd-k");
            cx.simulate_input(window, "> split");
            settle(cx, window);
            assert_eq!(scope(cx), Scope::Actions, "{n}: > shows Actions");
            press_keys(cx, window, "shift-tab");
            assert_eq!(scope(cx), Scope::All, "{n}: ⇧Tab from Actions (>)");
            assert_eq!(typed(cx), "split", "{n}: ⇧Tab dropped the >");
            assert_eq!(
                palette_state(cx, &ws).2,
                Some(Key::Action("Split Right")),
                "{n}: All, split typed"
            );
            press_keys(cx, window, "cmd-backspace");
            cx.simulate_input(window, "> split");
            settle(cx, window);
            let palette = palette_of(cx, &ws).expect("open");
            let chip = cx.read(|cx| palette.read(cx).chip_bounds(Scope::All));
            cx.simulate_click(
                window,
                chip.expect("the All chip").center(),
                Modifiers::none(),
            );
            settle(cx, window);
            assert_eq!(scope(cx), Scope::All, "{n}: the All chip (>)");
            assert_eq!(typed(cx), "split", "{n}: the chip dropped the >");
            press_keys(cx, window, "cmd-backspace");
            cx.simulate_input(window, "> co");
            settle(cx, window);
            press_keys(cx, window, "tab");
            assert_eq!(scope(cx), Scope::Workspaces, "{n}: Tab from Actions (>)");
            assert_eq!(palette_names(cx, &ws), ["codex"], "{n}: co, without the >");
            slot.borrow_mut().take();
        };
        render_with(&theme, dims, build, act);
        println!("ui-shots: palette-keys-{slug} ok");
    }
}

/// An action from the palette lands where its key would (not shot): with a file tab focused,
/// "find" lists Find in File… and Enter opens the tab's find bar; with a terminal focused no
/// Find is listed; Rename Workspace… opens the rename box (the palette closed); Preferences…
/// reaches the app's handler exactly once. Panics on the first that doesn't hold.
fn palette_run_scene() {
    use crate::palette_items::{Key, Line, Scope};
    use gpui::Focusable;
    use std::cell::RefCell;
    use std::rc::Rc;

    let dims = size(px(960.0), px(600.0));
    // Its own name (another run's can't clash), removed at the end.
    let dir = crate::test_temp::dir("fuxx-ui-shots-palette");
    std::fs::create_dir_all(&dir).expect("make the folder");
    let file = dir.join("notes.rs");
    std::fs::write(&file, "fn main() {\n    let found = 1;\n}\n").expect("write the file");
    let is_find =
        |l: &Line| matches!(l, Line::Item { key: Key::Action(a), .. } if a.starts_with("Find"));
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);

        // A file tab focused.
        let slot: Slot = Rc::new(RefCell::new(None));
        let keep = slot.clone();
        let opened = file.clone();
        let build = move |window: &mut Window, cx: &mut App| -> AnyView {
            cx.set_global(sidebar_appearance(theme, cx));
            cx.bind_keys(crate::key_bindings());
            let ws = cx.new(|cx| {
                let mut ws = Workspace::for_shots(window, cx);
                ws.sidebar.hint_dismissed = true;
                ws.open_shot_session(
                    "api",
                    std::path::Path::new(API),
                    SHELL_TEXT,
                    None,
                    window,
                    cx,
                );
                ws.open_file_tab(opened, crate::layout_model::FileMode::Plain, window, cx);
                ws
            });
            *keep.borrow_mut() = Some(ws.clone());
            ws.into()
        };
        let n = format!("palette-run-file-{slug}");
        let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
            let ws = slotted(&slot);
            settle(cx, window);
            let tab = cx.read(|cx| ws.read(cx).focused_file_tab().cloned());
            let tab = tab.expect("the file tab is focused");
            // A first listing in (the scenes list nothing themselves): Enter doesn't wait.
            seed_palette_files(cx, &ws, &[]);
            open_palette_on(cx, window, &ws, Scope::All);
            cx.simulate_input(window, "find");
            settle(cx, window);
            let (lines, _, selected) = palette_state(cx, &ws);
            assert!(lines.iter().any(is_find), "{n}: Find is listed");
            assert_eq!(selected, Some(Key::Action("Find in File\u{2026}")), "{n}");
            press_keys(cx, window, "enter");
            assert!(palette_of(cx, &ws).is_none(), "{n}: closed");
            let (open, keys) = cx
                .update_window(window, |_, window, cx| {
                    let t = tab.read(cx);
                    (
                        t.find_open(),
                        t.focus_handle(cx).contains_focused(window, cx),
                    )
                })
                .expect("read the tab");
            assert!(open, "{n}: the file tab's find bar is open");
            assert!(keys, "{n}: the file tab has the keyboard");
            slot.borrow_mut().take();
        };
        render_with(&theme, dims, build, act);

        // A terminal focused.
        let slot: Slot = Rc::new(RefCell::new(None));
        let build = palette_build(theme, slot.clone());
        let n = format!("palette-run-terminal-{slug}");
        let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
            let ws = slotted(&slot);
            // A first listing in (the scenes list nothing themselves): Enter doesn't wait.
            seed_palette_files(cx, &ws, &[]);
            open_palette_on(cx, window, &ws, Scope::All);
            cx.simulate_input(window, "find");
            settle(cx, window);
            let (lines, _, _) = palette_state(cx, &ws);
            assert!(!lines.iter().any(is_find), "{n}: no Find for a terminal");
            // Rename Workspace…: the rename box takes over, with the keyboard.
            press_keys(cx, window, "cmd-backspace");
            cx.simulate_input(window, "rename workspace");
            settle(cx, window);
            assert_eq!(
                palette_state(cx, &ws).2,
                Some(Key::Action("Rename Workspace\u{2026}")),
                "{n}"
            );
            press_keys(cx, window, "enter");
            let renaming = cx
                .update_window(window, |_, window, cx| {
                    let ws = ws.read(cx);
                    ws.renaming
                        .as_ref()
                        .map(|(_, input)| input.focus_handle(cx).is_focused(window))
                })
                .expect("read");
            assert_eq!(renaming, Some(true), "{n}: the rename box, with the keys");
            assert!(palette_of(cx, &ws).is_none(), "{n}: the palette closed");
            // Preferences…: the window opens its panel (M11x: the window handles ⌘, itself).
            open_palette_on(cx, window, &ws, Scope::All);
            cx.simulate_input(window, "preferences");
            settle(cx, window);
            assert_eq!(
                palette_state(cx, &ws).2,
                Some(Key::Action("Preferences\u{2026}")),
                "{n}"
            );
            press_keys(cx, window, "enter");
            let open = cx.read(|cx| ws.read(cx).preferences.is_some());
            assert!(open, "{n}: Preferences… opened the panel");
            assert!(palette_of(cx, &ws).is_none(), "{n}: closed");
            slot.borrow_mut().take();
        };
        render_with(&theme, dims, build, act);
        println!("ui-shots: palette-run-{slug} ok");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// A list that moves while open (not shot): on Workspaces with tests (the third row) chosen,
/// logs starts waiting and moves above it: tests stays chosen; tests' terminal closes (as a
/// script's close would): the palette stays open, with the keyboard, and the first row is
/// chosen. Panics on the first that doesn't hold.
fn palette_refresh_scene() {
    use crate::palette_items::{Key, Scope};
    use gpui::Focusable;
    use std::cell::RefCell;
    use std::rc::Rc;

    let dims = size(px(960.0), px(600.0));
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let slot: Slot = Rc::new(RefCell::new(None));
        let build = palette_build(theme, slot.clone());
        let n = format!("palette-refresh-{slug}");
        let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
            let ws = slotted(&slot);
            let ids = workspace_ids(cx, &ws);
            open_palette_on(cx, window, &ws, Scope::Workspaces);
            press_keys(cx, window, "down down");
            assert_eq!(
                palette_state(cx, &ws).2,
                Some(Key::Workspace(ids[2])),
                "{n}"
            );
            // logs (#8) starts waiting: Waiting first, so it moves above tests.
            cx.update(|cx| {
                ws.update(cx, |ws, cx| {
                    let logs = ws.sessions.get_mut(7).expect("logs");
                    logs.status = Status::Waiting;
                    logs.status_since = std::time::Instant::now();
                    cx.notify();
                })
            });
            settle(cx, window);
            assert_eq!(
                palette_names(cx, &ws)[..4],
                ["claude", "agent", "logs", "tests"],
                "{n}: logs moved up"
            );
            assert_eq!(
                palette_state(cx, &ws).2,
                Some(Key::Workspace(ids[2])),
                "{n}: tests is still chosen"
            );
            // tests' only terminal closes.
            cx.update_window(window, |_, window, cx| {
                ws.update(cx, |ws, cx| {
                    let tests = ws
                        .sessions
                        .position(|s| s.label() == "tests")
                        .expect("tests");
                    ws.close_at(tests, window, cx);
                })
            })
            .expect("close tests");
            settle(cx, window);
            let palette = palette_of(cx, &ws).expect("{n}: still open");
            let keys = cx
                .update_window(window, |_, window, cx| {
                    palette.read(cx).focus_handle(cx).is_focused(window)
                })
                .expect("read");
            assert!(keys, "{n}: with the keyboard");
            assert!(
                !palette_names(cx, &ws).contains(&"tests".to_string()),
                "{n}"
            );
            assert_eq!(
                palette_state(cx, &ws).2,
                Some(Key::Workspace(ids[0])),
                "{n}: the first row is chosen"
            );
            slot.borrow_mut().take();
        };
        render_with(&theme, dims, build, act);
        println!("ui-shots: palette-refresh-{slug} ok");
    }
}

/// Recent (not shot): Split Right picked, the palette reopened shows it under Recent; workspace
/// #2 picked too, both, newest first; then src/view.rs and Cursor breathing glow picked: those
/// two first, newest first, before the others. Panics unless so.
fn palette_recent_scene() {
    use crate::palette_items::{Key, Line, Scope};
    use std::cell::RefCell;
    use std::rc::Rc;

    let dims = size(px(960.0), px(600.0));
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let slot: Slot = Rc::new(RefCell::new(None));
        let build = palette_build(theme, slot.clone());
        let n = format!("palette-recent-{slug}");
        let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
            let ws = slotted(&slot);
            let ids = workspace_ids(cx, &ws);
            let item = |key| Line::Item {
                key,
                positions: Vec::new(),
            };
            // A first listing in (the scenes list nothing themselves): Enter doesn't wait.
            seed_palette_files(cx, &ws, &[]);
            open_palette_on(cx, window, &ws, Scope::All);
            cx.simulate_input(window, "split right");
            settle(cx, window);
            press_keys(cx, window, "enter");
            open_palette_on(cx, window, &ws, Scope::All);
            let (lines, _, _) = palette_state(cx, &ws);
            assert_eq!(
                lines[..3],
                [
                    Line::Header("Recent".into()),
                    item(Key::Action("Split Right")),
                    Line::Header("Workspaces".into()),
                ],
                "{n}: Split Right under Recent"
            );
            press_keys(cx, window, "escape");
            open_palette_on(cx, window, &ws, Scope::Workspaces);
            cx.simulate_input(window, "2");
            settle(cx, window);
            assert_eq!(
                palette_state(cx, &ws).2,
                Some(Key::Workspace(ids[1])),
                "{n}"
            );
            press_keys(cx, window, "enter");
            open_palette_on(cx, window, &ws, Scope::All);
            let (lines, _, _) = palette_state(cx, &ws);
            assert_eq!(
                lines[..4],
                [
                    Line::Header("Recent".into()),
                    item(Key::Workspace(ids[1])),
                    item(Key::Action("Split Right")),
                    Line::Header("Workspaces".into()),
                ],
                "{n}: newest first"
            );
            // ⌘P (Workspaces, nothing typed): exactly the old list, no Recent.
            press_keys(cx, window, "escape");
            open_palette_on(cx, window, &ws, Scope::Workspaces);
            let (lines, _, _) = palette_state(cx, &ws);
            assert_eq!(lines[0], Line::Header("Workspaces".into()), "{n}: ⌘P");
            assert!(
                !lines.contains(&Line::Header("Recent".into())),
                "{n}: no Recent on ⌘P"
            );
            assert_eq!(
                palette_names(cx, &ws).len(),
                8,
                "{n}: every workspace, once"
            );
            // A file and a setting picked too: first, newest first.
            press_keys(cx, window, "escape");
            seed_palette_files(cx, &ws, &PALETTE_FILES);
            let view = Key::File(PathBuf::from(PALETTE_ROOT).join("src/view.rs"));
            let glow = Key::Setting("Cursor breathing glow");
            for (query, key) in [("view", &view), ("glow", &glow)] {
                open_palette_on(cx, window, &ws, Scope::All);
                cx.simulate_input(window, query);
                settle(cx, window);
                assert_eq!(palette_state(cx, &ws).2.as_ref(), Some(key), "{n}: {query}");
                press_keys(cx, window, "enter");
                assert!(palette_of(cx, &ws).is_none(), "{n}: {query} picked");
            }
            open_palette_on(cx, window, &ws, Scope::All);
            let (lines, _, _) = palette_state(cx, &ws);
            assert_eq!(
                lines[..6],
                [
                    Line::Header("Recent".into()),
                    item(glow.clone()),
                    item(view.clone()),
                    item(Key::Workspace(ids[1])),
                    item(Key::Action("Split Right")),
                    Line::Header("Workspaces".into()),
                ],
                "{n}: the setting, the file, then the rest"
            );
            // A listing without src/view.rs: still Recent, its file tab is open.
            press_keys(cx, window, "escape");
            seed_palette_files(cx, &ws, &["Cargo.toml"]);
            open_palette_on(cx, window, &ws, Scope::All);
            let (lines, _, _) = palette_state(cx, &ws);
            assert_eq!(
                lines[..3],
                [
                    Line::Header("Recent".into()),
                    item(glow.clone()),
                    item(view.clone()),
                ],
                "{n}: open in a tab, so still Recent"
            );
            slot.borrow_mut().take();
        };
        render_with(&theme, dims, build, act);
        println!("ui-shots: palette-recent-{slug} ok");
    }
}

/// The files the palette scenes list (M11j Task 5): fuxx-cli's, as if just listed for the
/// followed terminal's folder.
const PALETTE_ROOT: &str = "/w/fuxx-cli";

const PALETTE_FILES: [&str; 7] = [
    "Cargo.toml",
    "README.md",
    "docs/a.md",
    "src/main.rs",
    "src/palette.rs",
    "src/view.rs",
    "src/workspace.rs",
];

/// List `paths` under `PALETTE_ROOT` for the followed terminal's folder, as if just listed.
fn seed_palette_files(cx: &mut VisualTestAppContext, ws: &gpui::Entity<Workspace>, paths: &[&str]) {
    cx.update(|cx| {
        ws.update(cx, |ws, cx| {
            let (folder, ssh) = ws.palette_folder(cx).expect("a terminal folder");
            assert!(!ssh, "a local terminal is followed");
            let paths = paths.iter().map(|p| p.to_string()).collect();
            ws.palette_files
                .set_for_shots(folder, PathBuf::from(PALETTE_ROOT), paths);
            cx.notify();
        })
    });
}

/// The rightmost x (points) along the row's middle band where a pixel is colour `c`, between
/// `x0` and `x1`; none if there is none.
fn rightmost(
    shot: &Shot,
    dims: Size<Pixels>,
    c: alacritty_terminal::vte::ansi::Rgb,
    row: gpui::Bounds<Pixels>,
    x0: f32,
    x1: f32,
) -> Option<f32> {
    let scale = shot.width as f32 / f32::from(dims.width);
    let (top, bottom) = (f32::from(row.top()) + 4.0, f32::from(row.bottom()) - 4.0);
    let mut best: Option<f32> = None;
    let mut y = top;
    while y < bottom {
        let (a, b) = ((x0 * scale) as usize, (x1 * scale) as usize);
        for dx in a..b {
            let x = dx as f32 / scale;
            if near(pixel_at(shot, dims, x, y), c) && best.is_none_or(|r| x > r) {
                best = Some(x);
            }
        }
        y += 1.0 / scale;
    }
    best
}

/// How many pixels in the box are colour `c` (within `tol`).
fn count_in(
    shot: &Shot,
    dims: Size<Pixels>,
    c: alacritty_terminal::vte::ansi::Rgb,
    tol: u8,
    (x0, x1): (f32, f32),
    (y0, y1): (f32, f32),
) -> usize {
    let scale = shot.width as f32 / f32::from(dims.width);
    let mut count = 0;
    for dy in (y0 * scale) as usize..(y1 * scale) as usize {
        for dx in (x0 * scale) as usize..(x1 * scale) as usize {
            if near_by(
                pixel_at(shot, dims, dx as f32 / scale, dy as f32 / scale),
                c,
                tol,
            ) {
                count += 1;
            }
        }
    }
    count
}

/// Files in the palette (M11j Task 5), over `switcher_workspace` with fuxx-cli's files listed for
/// shell's folder, in both themes:
/// - `palette-files`: "view" typed on All: `Files in fuxx-cli` first, src/view.rs chosen;
/// - `palette-files-empty`: the Files scope, nothing typed: `Type to search files in fuxx-cli`.
///
/// Panics unless so (Enter typed straight after a letter waits for that letter's ranking),
/// src/view.rs's icon has a pixel in `palette_file_ink(Red)` in its slot and
/// its folder (`src`) is drawn in `text_faint` right of its name; and (not shot) Enter opens
/// src/view.rs in a Plain file tab in the focused pane, the palette closed; with logs (an SSH
/// terminal) followed, Files says `SSH_NOTE`; with a first listing running, `LISTING` (and on All
/// no Files section, no note).
fn palette_files_scene() {
    use crate::layout_model::{FileMode, TabKind};
    use crate::palette::palette_file_ink;
    use crate::palette_items::{Key, LISTING, Line, SSH_NOTE, Scope};
    use crate::right_sidebar::FileKind;
    use crate::sidebar::MARK_SLOT;
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    let dims = size(px(960.0), px(600.0));
    let view = PathBuf::from(PALETTE_ROOT).join("src/view.rs");
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let ui = theme.ui();
        for stage in [
            "shot",
            "enter",
            "empty",
            "ssh",
            "listing",
            "typing",
            "enter-early",
            "enter-retyped",
            "enter-listing",
            "enter-rescoped",
        ] {
            let slot: Slot = Rc::new(RefCell::new(None));
            let build = palette_build(theme, slot.clone());
            let seen: Rc<Cell<Option<gpui::Bounds<Pixels>>>> = Rc::default();
            let found = seen.clone();
            let n = format!("palette-files-{stage}-{slug}");
            let view = view.clone();
            let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let ws = slotted(&slot);
                let lines = |cx: &mut VisualTestAppContext| palette_state(cx, &ws).0;
                match stage {
                    "ssh" => {
                        // logs (#8) is an SSH terminal to build-box: shown, so followed.
                        cx.update_window(window, |_, window, cx| {
                            ws.update(cx, |ws, cx| ws.select(7, window, cx))
                        })
                        .expect("select logs");
                        settle(cx, window);
                        let ssh = cx.read(|cx| ws.read(cx).palette_folder(cx).map(|f| f.1));
                        assert_eq!(ssh, Some(true), "{n}: an SSH terminal followed");
                        open_palette_on(cx, window, &ws, Scope::Files);
                        assert_eq!(lines(cx), [Line::Note(SSH_NOTE.into())], "{n}");
                        cx.simulate_input(window, "main");
                        settle(cx, window);
                        assert_eq!(lines(cx), [Line::Note(SSH_NOTE.into())], "{n}: typed");
                    }
                    "enter-listing" | "enter-rescoped" => {
                        // The first listing still running: Enter waits for it (and the
                        // query's ranking), then picks the best for the query.
                        cx.update(|cx| {
                            ws.update(cx, |ws, cx| {
                                let (folder, _) = ws.palette_folder(cx).expect("a folder");
                                ws.palette_files.started(&folder);
                            })
                        });
                        open_palette_on(cx, window, &ws, Scope::All);
                        if stage == "enter-rescoped" {
                            // A waiting Enter then Tab: the Enter was for All, so nothing is
                            // picked on Actions (Split Right isn't run), nor once listed.
                            cx.simulate_input(window, "split right");
                            settle(cx, window);
                            press_keys(cx, window, "enter");
                            assert!(palette_of(cx, &ws).is_some(), "{n}: Enter waits");
                            press_keys(cx, window, "tab");
                            assert_eq!(palette_state(cx, &ws).1, Scope::Actions, "{n}: Tab");
                            assert!(palette_of(cx, &ws).is_some(), "{n}: open after Tab");
                            assert_eq!(shown_panes(cx, &ws).len(), 1, "{n}: nothing split");
                            seed_palette_files(cx, &ws, &PALETTE_FILES);
                            settle(cx, window);
                            assert!(palette_of(cx, &ws).is_some(), "{n}: open once listed");
                            assert_eq!(shown_panes(cx, &ws).len(), 1, "{n}: still one pane");
                            slot.borrow_mut().take();
                            return;
                        }
                        cx.simulate_input(window, "view");
                        settle(cx, window);
                        press_keys(cx, window, "enter");
                        assert!(palette_of(cx, &ws).is_some(), "{n}: Enter waits");
                        seed_palette_files(cx, &ws, &PALETTE_FILES);
                        settle(cx, window);
                        assert!(palette_of(cx, &ws).is_none(), "{n}: picked once listed");
                        let kind = cx.read(|cx| {
                            let ws = ws.read(cx);
                            let tab = ws.model.focused_tab()?;
                            ws.model.tab(tab).map(|t| t.kind.clone())
                        });
                        assert_eq!(
                            kind,
                            Some(TabKind::File {
                                path: view.clone(),
                                mode: FileMode::Plain
                            }),
                            "{n}: the best for \"view\""
                        );
                    }
                    "enter-early" | "enter-retyped" => {
                        // "vie" ranked (vie/w.md first), then "w" and Enter at once, before the
                        // "view" ranking has run: Enter waits for it and opens src/view.rs. A
                        // letter typed after that Enter (before the answer) drops it.
                        seed_palette_files(cx, &ws, &["vie/w.md", "src/view.rs"]);
                        open_palette_on(cx, window, &ws, Scope::Files);
                        cx.simulate_input(window, "vie");
                        settle(cx, window);
                        let vie_w = Key::File(PathBuf::from(PALETTE_ROOT).join("vie/w.md"));
                        assert_eq!(palette_state(cx, &ws).2, Some(vie_w), "{n}: vie");
                        // Dispatched without running the executor: no ranking runs between.
                        cx.dispatch_keystroke(
                            window,
                            gpui::Keystroke {
                                modifiers: gpui::Modifiers::default(),
                                key: "w".into(),
                                key_char: Some("w".into()),
                            },
                        );
                        cx.dispatch_keystroke(
                            window,
                            gpui::Keystroke::parse("enter").expect("a keystroke"),
                        );
                        if stage == "enter-retyped" {
                            cx.dispatch_keystroke(
                                window,
                                gpui::Keystroke {
                                    modifiers: gpui::Modifiers::default(),
                                    key: "s".into(),
                                    key_char: Some("s".into()),
                                },
                            );
                            settle(cx, window);
                            assert!(palette_of(cx, &ws).is_some(), "{n}: still open");
                            assert_eq!(
                                palette_state(cx, &ws).2,
                                Some(Key::File(view.clone())),
                                "{n}: \"views\" ranked"
                            );
                            slot.borrow_mut().take();
                            return;
                        }
                        settle(cx, window);
                        assert!(palette_of(cx, &ws).is_none(), "{n}: Enter picked");
                        let kind = cx.read(|cx| {
                            let ws = ws.read(cx);
                            let tab = ws.model.focused_tab()?;
                            ws.model.tab(tab).map(|t| t.kind.clone())
                        });
                        assert_eq!(
                            kind,
                            Some(TabKind::File {
                                path: view.clone(),
                                mode: FileMode::Plain
                            }),
                            "{n}: the best for \"view\", not the last answer's"
                        );
                    }
                    "typing" => {
                        // Typed a letter at a time, each ranking in before the next letter:
                        // "vie" puts vie/w.md first; "view" puts src/view.rs first, and that is
                        // what's chosen (not the row the last answer had first).
                        seed_palette_files(cx, &ws, &["vie/w.md", "src/view.rs"]);
                        open_palette_on(cx, window, &ws, Scope::Files);
                        let vie_w = Key::File(PathBuf::from(PALETTE_ROOT).join("vie/w.md"));
                        for letter in ["v", "i", "e"] {
                            cx.simulate_input(window, letter);
                            settle(cx, window);
                        }
                        assert_eq!(palette_state(cx, &ws).2, Some(vie_w.clone()), "{n}: vie");
                        cx.simulate_input(window, "w");
                        settle(cx, window);
                        let (lines, _, selected) = palette_state(cx, &ws);
                        assert!(
                            lines
                                .iter()
                                .any(|l| matches!(l, Line::Item { key, .. } if *key == vie_w)),
                            "{n}: vie/w.md still listed"
                        );
                        assert_eq!(
                            selected,
                            Some(Key::File(view.clone())),
                            "{n}: the best chosen"
                        );
                        // Moved by hand, the choice stays put when an answer comes in.
                        press_keys(cx, window, "down");
                        assert_eq!(palette_state(cx, &ws).2, Some(vie_w.clone()), "{n}: ↓");
                        // A new listing (the 10 s re-list): ranked again, the choice kept.
                        seed_palette_files(cx, &ws, &["vie/w.md", "src/view.rs"]);
                        settle(cx, window);
                        assert_eq!(palette_state(cx, &ws).2, Some(vie_w), "{n}: kept");
                    }
                    "listing" => {
                        cx.update(|cx| {
                            ws.update(cx, |ws, cx| {
                                let (folder, _) = ws.palette_folder(cx).expect("a folder");
                                ws.palette_files.started(&folder);
                            })
                        });
                        open_palette_on(cx, window, &ws, Scope::Files);
                        assert_eq!(lines(cx), [Line::Note(LISTING.into())], "{n}");
                        cx.simulate_input(window, "main");
                        settle(cx, window);
                        assert_eq!(lines(cx), [Line::Note(LISTING.into())], "{n}: typed");
                        // On All: no Files section and no note while listing.
                        press_keys(cx, window, "tab tab");
                        assert_eq!(palette_state(cx, &ws).1, Scope::All, "{n}");
                        let all = lines(cx);
                        assert!(
                            !all.iter().any(|l| matches!(l, Line::Note(_))),
                            "{n}: {all:?}"
                        );
                        assert!(
                            !all.iter()
                                .any(|l| matches!(l, Line::Header(h) if h.starts_with("Files"))),
                            "{n}: {all:?}"
                        );
                    }
                    _ => {
                        seed_palette_files(cx, &ws, &PALETTE_FILES);
                        if stage == "empty" {
                            open_palette_on(cx, window, &ws, Scope::Files);
                            assert_eq!(
                                lines(cx),
                                [Line::Note("Type to search files in fuxx-cli".into())],
                                "{n}"
                            );
                            slot.borrow_mut().take();
                            return;
                        }
                        open_palette_on(cx, window, &ws, Scope::All);
                        cx.simulate_input(window, "view");
                        settle(cx, window);
                        let (lines, _, selected) = palette_state(cx, &ws);
                        assert_eq!(lines[0], Line::Header("Files in fuxx-cli".into()), "{n}");
                        assert!(
                            matches!(&lines[1], Line::Item { key: Key::File(p), .. } if *p == view),
                            "{n}: {:?}",
                            lines[1]
                        );
                        assert_eq!(selected, Some(Key::File(view.clone())), "{n}: chosen");
                        let palette = palette_of(cx, &ws).expect("open");
                        found.set(cx.read(|cx| palette.read(cx).row_bounds(1)));
                        if stage == "enter" {
                            press_keys(cx, window, "enter");
                            assert!(palette_of(cx, &ws).is_none(), "{n}: Enter closes it");
                            let kind = cx.read(|cx| {
                                let ws = ws.read(cx);
                                let tab = ws.model.focused_tab()?;
                                ws.model.tab(tab).map(|t| t.kind.clone())
                            });
                            assert_eq!(
                                kind,
                                Some(TabKind::File {
                                    path: view.clone(),
                                    mode: FileMode::Plain
                                }),
                                "{n}: a Plain file tab, focused"
                            );
                        }
                    }
                }
                slot.borrow_mut().take();
            };
            let shot = render_with(&theme, dims, build, act);
            match stage {
                "shot" => {
                    save_shot("palette-files", slug, &shot);
                    let row = seen.get().expect("the row is drawn");
                    let n = format!("palette-files-{slug}");
                    let left = f32::from(row.left()) + 8.0;
                    let (top, bottom) = (f32::from(row.top()), f32::from(row.bottom()));
                    let ink = palette_file_ink(FileKind::Red, &theme);
                    // (The icon set's thin outline, 2026-11-20: its anti-aliased stroke comes near
                    // the ink, not always onto it.)
                    let icon = count_in(
                        &shot,
                        dims,
                        ink,
                        24,
                        (left, left + MARK_SLOT),
                        (top, bottom),
                    );
                    assert!(icon > 0, "{n}: the icon in its kind's ink ({icon})");
                    // The name (`text`) then, right of it, its folder in `text_faint`.
                    let names = left + MARK_SLOT;
                    let right = f32::from(row.right()) - 8.0;
                    let name_end = rightmost(&shot, dims, ui.text, row, names, right)
                        .expect("the name is drawn");
                    // Small text at 1×: its darkest stem, closer to `text_faint` than to `text`.
                    let ink = strongest(&shot, dims, ui.row_active, row, name_end + 4.0, right)
                        .expect("right of the name");
                    assert!(
                        gap(ink, ui.row_active) >= 24
                            && gap(ink, ui.text_faint) < gap(ink, ui.text),
                        "{n}: the folder, in text_faint, right of the name ({name_end}, {ink:?})"
                    );
                }
                "empty" => {
                    save_shot("palette-files-empty", slug, &shot);
                }
                _ => {}
            }
            println!("ui-shots: palette-files-{stage}-{slug} ok");
        }
    }
}

/// Files listed again while the palette is open (M11j Task 5 fix), not shot: a terminal whose
/// folder changes while Files is shown (as a `cd` would) gets the new folder listed; a listing
/// asked for while another ran (and so turned away) is listed once that one ends. Real listings
/// of two made-up folders (no repo: the walk). Panics unless each ends on its folder's
/// "Type to search" note, never stuck on `LISTING`.
fn palette_relist_scene() {
    use crate::palette_items::{Line, Scope};
    use std::cell::RefCell;
    use std::rc::Rc;

    let dims = size(px(960.0), px(600.0));
    let folder = |name: &str| {
        // Its own name (another run's can't clash), removed at the end.
        let dir = crate::test_temp::dir(&format!("fuxx-ui-shots-relist-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("make the folder");
        std::fs::write(dir.join(format!("{name}.txt")), name).expect("write a file");
        std::fs::canonicalize(&dir).expect("its real path")
    };
    let (a, b) = (folder("a"), folder("b"));
    let note = |dir: &std::path::Path| {
        let name = dir.file_name().and_then(|n| n.to_str()).unwrap_or("");
        Line::Note(format!("Type to search files in {name}"))
    };
    let theme = shot_theme("Catppuccin Mocha");
    for stage in ["moved", "turned-away"] {
        let slot: Slot = Rc::new(RefCell::new(None));
        let keep = slot.clone();
        let start = a.clone();
        let build = move |window: &mut Window, cx: &mut App| -> AnyView {
            cx.set_global(sidebar_appearance(theme, cx));
            cx.bind_keys(crate::key_bindings());
            let ws = cx.new(|cx| {
                let mut ws = Workspace::for_shots(window, cx);
                ws.sidebar.hint_dismissed = true;
                ws.open_shot_session("api", &start, "", None, window, cx);
                ws.palette_files.frozen = false;
                ws
            });
            *keep.borrow_mut() = Some(ws.clone());
            ws.into()
        };
        let n = format!("palette-relist-{stage}");
        let (a, b) = (a.clone(), b.clone());
        let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
            let ws = slotted(&slot);
            let move_to = |ws: &mut Workspace, dir: &std::path::Path| {
                ws.sessions.get_mut(0).expect("api").project = Some(dir.to_path_buf());
            };
            if stage == "moved" {
                open_palette_on(cx, window, &ws, Scope::Files);
                settle(cx, window);
                assert_eq!(palette_state(cx, &ws).0, [note(&a)], "{n}: a listed");
                cx.update(|cx| {
                    ws.update(cx, |ws, cx| {
                        move_to(ws, &b);
                        cx.notify();
                    })
                });
                settle(cx, window);
                settle(cx, window);
            } else {
                // a's listing starts; before it runs, the terminal moves to b and the palette
                // opens there (its listing turned away: a's is running).
                cx.update_window(window, |_, window, cx| {
                    ws.update(cx, |ws, cx| {
                        ws.list_palette_files(cx);
                        move_to(ws, &b);
                        ws.open_palette(Scope::Files, window, cx);
                    })
                })
                .expect("open");
                settle(cx, window);
                settle(cx, window);
            }
            assert_eq!(palette_state(cx, &ws).0, [note(&b)], "{n}: b listed");
            slot.borrow_mut().take();
        };
        render_with(&theme, dims, build, act);
        println!("ui-shots: palette-relist-{stage} ok");
    }
    for dir in [&a, &b] {
        let _ = std::fs::remove_dir_all(dir);
    }
}

/// Settings in the palette (M11j Task 5), in both themes: `palette-settings`: "glow" typed on
/// All: Cursor breathing glow chosen, `Appearance` on its right in `text_faint`, the gear in its
/// slot in `text_dim`.
///
/// Panics unless so, and (not shot) Enter closes the palette, makes Appearance the section
/// Preferences opens on (`LastSection`) and opens the Preferences panel on it (M11x).
fn palette_settings_scene() {
    use crate::palette_items::{Key, Scope, selectable};
    use crate::preferences::Section;
    use crate::sidebar::MARK_SLOT;
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    let dims = size(px(960.0), px(600.0));
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let ui = theme.ui();
        for enter in [false, true] {
            let slot: Slot = Rc::new(RefCell::new(None));
            let build = palette_build(theme, slot.clone());
            let seen: Rc<Cell<Option<gpui::Bounds<Pixels>>>> = Rc::default();
            let found = seen.clone();
            let n = format!("palette-settings-{slug}-{enter}");
            let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let ws = slotted(&slot);
                // Preferences open over the window (M11x): Enter switches its section.
                if enter {
                    cx.update_window(window, |_, window, cx| {
                        ws.update(cx, |ws, cx| ws.toggle_preferences(window, cx))
                    })
                    .expect("open Preferences");
                    settle(cx, window);
                }
                if enter {
                    // A first listing in (the scenes list nothing themselves): Enter doesn't
                    // wait for one.
                    seed_palette_files(cx, &ws, &[]);
                }
                open_palette_on(cx, window, &ws, Scope::All);
                cx.simulate_input(window, "glow");
                settle(cx, window);
                let (lines, _, selected) = palette_state(cx, &ws);
                let first = selectable(&lines)[0];
                assert!(
                    matches!(
                        &lines[first],
                        crate::palette_items::Line::Item { key: Key::Setting(name), .. }
                            if *name == "Cursor breathing glow"
                    ),
                    "{n}: first: {:?}",
                    lines[first]
                );
                assert_eq!(selected, Some(Key::Setting("Cursor breathing glow")), "{n}");
                let palette = palette_of(cx, &ws).expect("open");
                found.set(cx.read(|cx| palette.read(cx).row_bounds(first)));
                if enter {
                    let before = cx.read(crate::preferences::last_section);
                    assert_ne!(before, Section::Appearance, "{n}: not yet");
                    press_keys(cx, window, "enter");
                    assert!(palette_of(cx, &ws).is_none(), "{n}: Enter closes it");
                    let last = cx.read(crate::preferences::last_section);
                    assert_eq!(last, Section::Appearance, "{n}: LastSection");
                    let shown = cx.read(|cx| {
                        ws.read(cx)
                            .preferences
                            .as_ref()
                            .map(|p| p.read(cx).section())
                    });
                    assert_eq!(
                        shown,
                        Some(Section::Appearance),
                        "{n}: the panel is open on Appearance"
                    );
                }
                slot.borrow_mut().take();
            };
            let shot = render_with(&theme, dims, build, act);
            if !enter {
                save_shot("palette-settings", slug, &shot);
                let row = seen.get().expect("the row is drawn");
                let n = format!("palette-settings-{slug}");
                let (top, bottom) = (f32::from(row.top()), f32::from(row.bottom()));
                let left = f32::from(row.left()) + 8.0;
                // 12: the gear's thin strokes blend with the row at 1×, and the darker
                // `text_dim` of the dark-panel chore sits further from the blend.
                let gear = count_in(
                    &shot,
                    dims,
                    ui.text_dim,
                    12,
                    (left, left + MARK_SLOT),
                    (top, bottom),
                );
                assert!(gear > 0, "{n}: the gear, text_dim ({gear})");
                // `Appearance`, right-aligned in `text_faint`, in the row's last 90 pt.
                let right = f32::from(row.right()) - 8.0;
                let faint = rightmost(&shot, dims, ui.text_faint, row, right - 90.0, right + 1.0);
                assert!(
                    faint.is_some_and(|x| x > right - 10.0),
                    "{n}: Appearance ({faint:?})"
                );
            }
            println!("ui-shots: palette-settings-{slug}-{enter} ok");
        }
    }
}

/// The palette in a narrow window (M11j Task 5), both themes: `palette-narrow`: 480 pt wide, the
/// Files scope with "abcdefghij" typed, the one file a name 60 capitals long.
///
/// Panics unless the panel keeps 16 pt from the window's right edge, every row ends inside it,
/// and the long name is cut: drawn up to the row's right padding and nothing in `text` past it
/// (in the padding, the panel's edge or the window beyond); and (not shot) a folder too long for
/// its row is cut at its start (its end shows).
fn palette_narrow_scene() {
    use crate::palette_items::{Scope, selectable};
    use std::cell::RefCell;
    use std::rc::Rc;

    let width = 480.0_f32;
    let dims = size(px(width), px(600.0));
    // 60 capitals: well past what the row can hold (lower case nearly fits).
    let long = format!("src/{}.rs", "ABCDEFGHIJ".repeat(6));
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let ui = theme.ui();
        // One terminal with nothing on its screen, so only the palette draws text there.
        let narrow = |keep: Slot| {
            move |window: &mut Window, cx: &mut App| -> AnyView {
                cx.set_global(sidebar_appearance(theme, cx));
                cx.bind_keys(crate::key_bindings());
                let ws = cx.new(|cx| {
                    let mut ws = Workspace::for_shots(window, cx);
                    ws.sidebar.hint_dismissed = true;
                    ws.open_shot_session("api", std::path::Path::new(API), "", None, window, cx);
                    ws
                });
                *keep.borrow_mut() = Some(ws.clone());
                ws.into()
            }
        };
        let slot: Slot = Rc::new(RefCell::new(None));
        let build = narrow(slot.clone());
        type Seen = (
            gpui::Bounds<Pixels>,
            Vec<gpui::Bounds<Pixels>>,
            gpui::Bounds<Pixels>,
        );
        let seen: Rc<RefCell<Option<Seen>>> = Rc::default();
        let found = seen.clone();
        let n = format!("palette-narrow-{slug}");
        let long = long.clone();
        let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
            let ws = slotted(&slot);
            seed_palette_files(cx, &ws, &[long.as_str(), "src/main.rs"]);
            open_palette_on(cx, window, &ws, Scope::Files);
            cx.simulate_input(window, "abcdefghij");
            settle(cx, window);
            let (lines, _, _) = palette_state(cx, &ws);
            let first = selectable(&lines)[0];
            let palette = palette_of(cx, &ws).expect("open");
            *found.borrow_mut() = cx.read(|cx| {
                let p = palette.read(cx);
                let rows = (0..lines.len()).filter_map(|at| p.row_bounds(at)).collect();
                Some((p.panel_bounds()?, rows, p.row_bounds(first)?))
            });
            assert_eq!(
                lines.len(),
                2,
                "{n}: the heading and the one file: {lines:?}"
            );
            slot.borrow_mut().take();
        };
        let shot = render_with(&theme, dims, build, act);
        save_shot("palette-narrow", slug, &shot);
        let n = format!("palette-narrow-{slug}");
        let (panel, rows, row) = seen.borrow().clone().expect("drawn");
        // The panel's outer edge is 1 pt out from what its absolute child fills.
        let panel_right = f32::from(panel.right()) + 1.0;
        assert!(
            panel_right <= width - 16.0 + 0.5,
            "{n}: the panel ends at {panel_right}"
        );
        for b in &rows {
            assert!(
                f32::from(b.right()) <= f32::from(panel.right()) + 0.5,
                "{n}: a row past the panel: {b:?}"
            );
        }
        let padding = f32::from(row.right()) - 8.0;
        let end = rightmost(&shot, dims, ui.text, row, f32::from(row.left()), width)
            .expect("the name is drawn");
        // The cut leaves the ellipsis a little short of the padding.
        assert!(
            end > padding - 40.0,
            "{n}: the name runs to the row's end ({end})"
        );
        assert!(
            end <= padding + 0.5,
            "{n}: nothing in text past the row's padding ({end})"
        );
        // On one line (a name that wrapped would spill out of its row).
        let scale = shot.width as f32 / f32::from(dims.width);
        let (x0, x1) = (f32::from(row.left()) + 30.0, padding);
        let band = ((f32::from(row.top()) - 12.0) * scale) as usize
            ..((f32::from(row.bottom()) + 12.0) * scale) as usize;
        let inked: Vec<f32> = band
            .map(|dy| dy as f32 / scale)
            .filter(|&y| count_along(&shot, dims, ui.text, y, x0, x1) > 0)
            .collect();
        let (top, bottom) = (inked[0], inked[inked.len() - 1]);
        assert!(
            bottom - top <= 16.0,
            "{n}: the name is one line ({top}..{bottom})"
        );

        // A folder too long for its row is cut at its start, so its end shows: dots then wide
        // letters show the letters (and the other way round, the dots).
        let (thin, wide) = (".".repeat(200), "W".repeat(60));
        let wide_end = format!("{thin}/{wide}/a.rs");
        let thin_end = format!("{wide}/{thin}/b.rs");
        let slot: Slot = Rc::new(RefCell::new(None));
        let build = narrow(slot.clone());
        let rows: Rc<RefCell<Vec<gpui::Bounds<Pixels>>>> = Rc::default();
        let found = rows.clone();
        let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
            let ws = slotted(&slot);
            seed_palette_files(cx, &ws, &[wide_end.as_str(), thin_end.as_str()]);
            open_palette_on(cx, window, &ws, Scope::Files);
            cx.simulate_input(window, ".rs");
            settle(cx, window);
            let (lines, _, _) = palette_state(cx, &ws);
            let palette = palette_of(cx, &ws).expect("open");
            // a.rs's row, then b.rs's.
            let at = |name: &str| {
                lines
                    .iter()
                    .position(|l| matches!(l, crate::palette_items::Line::Item { key: crate::palette_items::Key::File(p), .. } if p.ends_with(name)))
                    .expect("listed")
            };
            let (a, b) = (at("a.rs"), at("b.rs"));
            *found.borrow_mut() = cx.read(|cx| {
                let p = palette.read(cx);
                [p.row_bounds(a), p.row_bounds(b)]
                    .into_iter()
                    .flatten()
                    .collect()
            });
            slot.borrow_mut().take();
        };
        let shot = render_with(&theme, dims, build, act);
        let rows = rows.borrow().clone();
        assert_eq!(rows.len(), 2, "{n}: both rows drawn");
        // Inked pixels in each row's folder, past its name (`a.rs`: under 40 pt with the slot).
        let ink = |row: gpui::Bounds<Pixels>| {
            let scale = shot.width as f32 / f32::from(dims.width);
            let (x0, x1) = (f32::from(row.left()) + 70.0, f32::from(row.right()) - 8.0);
            let (y0, y1) = (f32::from(row.top()) + 4.0, f32::from(row.bottom()) - 4.0);
            let bg = pixel_at(
                &shot,
                dims,
                f32::from(row.left()) + 3.0,
                f32::from(row.center().y),
            );
            let bg = alacritty_terminal::vte::ansi::Rgb {
                r: bg[0],
                g: bg[1],
                b: bg[2],
            };
            let mut count = 0;
            for dy in (y0 * scale) as usize..(y1 * scale) as usize {
                for dx in (x0 * scale) as usize..(x1 * scale) as usize {
                    if gap(
                        pixel_at(&shot, dims, dx as f32 / scale, dy as f32 / scale),
                        bg,
                    ) >= 40
                    {
                        count += 1;
                    }
                }
            }
            count
        };
        let (wide_shown, thin_shown) = (ink(rows[0]), ink(rows[1]));
        assert!(
            wide_shown > 2 * thin_shown,
            "{n}: the folders' ends show ({wide_shown} vs {thin_shown})"
        );
    }
}

/// Reduce motion and the palette (M11j Task 5): `palette-motion`: Workspaces open over
/// `switcher_workspace` (codex and deploy Working), Reduce motion on.
///
/// Panics unless, with Reduce motion on, two frames 200 ms apart (the marks' clock moved on
/// between them) are pixel-identical in the panel; and (not shot) with it off, they differ (so
/// the check can tell).
fn palette_motion_scene() {
    use crate::palette_items::Scope;
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::time::Duration;

    let dims = size(px(960.0), px(600.0));
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        for still in [true, false] {
            let slot: Slot = Rc::new(RefCell::new(None));
            let inner = palette_build(theme, slot.clone());
            let build = move |window: &mut Window, cx: &mut App| -> AnyView {
                cx.set_reduce_motion(still);
                inner(window, cx)
            };
            let n = format!("palette-motion-{slug}-{still}");
            let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let ws = slotted(&slot);
                open_palette_on(cx, window, &ws, Scope::Workspaces);
                let palette = palette_of(cx, &ws).expect("open");
                let panel = cx
                    .read(|cx| palette.read(cx).panel_bounds())
                    .expect("the panel is drawn");
                let grab = |cx: &mut VisualTestAppContext| {
                    let image = cx.capture_screenshot(window).expect("capture");
                    let (w, raw) = (image.width() as usize, image.as_raw().to_vec());
                    let scale = w as f32 / f32::from(dims.width);
                    let (x0, x1) = (
                        (f32::from(panel.left()) * scale) as usize,
                        (f32::from(panel.right()) * scale) as usize,
                    );
                    let (y0, y1) = (
                        (f32::from(panel.top()) * scale) as usize,
                        (f32::from(panel.bottom()) * scale) as usize,
                    );
                    (y0..y1)
                        .flat_map(|y| raw[(y * w + x0) * 4..(y * w + x1) * 4].to_vec())
                        .collect::<Vec<u8>>()
                };
                let before = grab(cx);
                prof::sleep(Duration::from_millis(200));
                cx.advance_clock(Duration::from_millis(200));
                // The marks' clock on by 200 ms whether or not its ticker runs.
                let clock = cx.read(|cx| ws.read(cx).clock.clone());
                clock.update(cx, |clock, cx| {
                    clock.phase = status_mark::advance(status_mark::advance(clock.phase));
                    cx.notify();
                });
                settle(cx, window);
                let after = grab(cx);
                assert_eq!(before == after, still, "{n}: the panel moved = {}", !still);
                slot.borrow_mut().take();
            };
            let shot = render_with(&theme, dims, build, act);
            if still {
                save_shot("palette-motion", slug, &shot);
            }
            println!("ui-shots: palette-motion-{slug}-{still} ok");
        }
    }
}
