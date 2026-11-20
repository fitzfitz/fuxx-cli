//! Panes and tabs: cached panes, tabs, splits, workspaces, tab bars, tab drag and the browser tab.

use super::sidebar::{EIGHT, row_point, with_progress};
use super::*;

/// This file, as `file!()` gives it (repo-relative under cargo): `registry::AREAS` pairs it with
/// `SCENES`, so `--changed` runs these scenes when this file changes.
pub(super) const FILE: &str = file!();

/// This area's scenes (`registry.rs` runs them in its `ORDER`).
pub(super) const SCENES: &[Scene] = &[
    Scene {
        name: "cached-panes",
        run: cached_panes,
        themes: Themes::One,
        serial: false,
        kind: Kind::Visual,
        sources: &["src/terminal_view.rs", "src/workspace.rs"],
    },
    Scene {
        name: "tabs",
        run: tab_scenes,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/sidebar.rs",
            "src/tab_color.rs",
            "src/tab_menu.rs",
            "src/palette.rs",
        ],
    },
    Scene {
        name: "splits",
        run: split_scenes,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/panes.rs",
            "src/split_tree.rs",
            "src/tab_bar.rs",
            "src/workspace.rs",
        ],
    },
    Scene {
        name: "workspaces",
        run: workspace_scenes,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: &["src/sidebar.rs", "src/layout_model.rs", "src/workspace.rs"],
    },
    Scene {
        name: "tab-bars",
        run: tab_bar_scenes,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/tab_bar.rs",
            "src/pane_tab_menu.rs",
            "src/tab_menu.rs",
            "src/layout_model.rs",
            "src/pane_actions.rs",
            // `providers::all()`: one New ⟨agent⟩ Tab item per provider.
            "src/providers/mod.rs",
        ],
    },
    Scene {
        name: "tabs-minimal",
        run: tabs_minimal_scenes,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/tab_bar.rs",
            "src/mark_view.rs",
            "src/status_mark.rs",
            "src/theme.rs",
        ],
    },
    Scene {
        name: "tab-drag",
        run: tab_drag_scenes,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/tab_bar.rs",
            "src/drag_gap.rs",
            "src/layout_model.rs",
            "src/sidebar.rs",
            "src/pane_browser.rs",
        ],
    },
    Scene {
        name: "browser-tab",
        run: browser_tab_scenes,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/browser_tab.rs",
            "src/browser_history.rs",
            "src/pane_browser.rs",
            "src/browser_url.rs",
        ],
    },
];

/// Terminal panes are cached: a workspace redraw (the marks' ticker, a sidebar change) doesn't
/// repaint a terminal, while the terminal's own notify does. Writes to a terminal's screen
/// without telling it, redraws after a workspace notify and checks the frame is unchanged; then
/// has the terminal notify and checks the new text shows. No PNG.
fn cached_panes() {
    use std::cell::RefCell;
    use std::rc::Rc;

    let theme = shot_theme("Catppuccin Mocha");
    let dims = size(px(640.0), px(400.0));
    let mut cx = ShotApp::new();
    let slot = Rc::new(RefCell::new(None));
    let window = {
        let slot = slot.clone();
        cx.open_offscreen_window(dims, move |window, cx| {
            cx.set_global(shot_appearance(theme, cx));
            let ws = cx.new(|cx| {
                let mut ws = Workspace::for_shots(window, cx);
                let project = std::path::Path::new("/Users/me/code/api");
                ws.open_shot_session("api", project, SHELL_TEXT, None, window, cx);
                ws
            });
            *slot.borrow_mut() = Some(ws.clone());
            cx.new(|_| Root {
                theme,
                scene: ws.into(),
            })
        })
        .expect("open an off-screen window")
    };
    cx.run_until_parked();
    let frame = |cx: &mut gpui::VisualTestAppContext| -> Vec<u8> {
        cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
            .expect("draw");
        cx.capture_screenshot(window.into())
            .expect("capture the window")
            .into_raw()
    };
    // The frames as the app gets them first: the sidebar's pending scroll to the active row is
    // done there (a tight list), not by the frame under test.
    settle_frames(&mut cx, window.into());
    frame(&mut cx);
    let before = frame(&mut cx);
    let ws = slot.borrow().clone().expect("the workspace");
    let view = cx.read(|cx| ws.read(cx).sessions.items()[0].view.clone());

    cx.read(|cx| view.read(cx).show_text("\r\nnot told yet"));
    cx.update(|cx| ws.update(cx, |_, cx| cx.notify()));
    assert!(
        frame(&mut cx) == before,
        "cached-panes: a workspace redraw repainted the terminal"
    );
    cx.update(|cx| view.update(cx, |_, cx| cx.notify()));
    assert!(
        frame(&mut cx) != before,
        "cached-panes: the terminal's own notify didn't repaint it"
    );
}

/// Sidebar rows as tabs, from real workspaces in both themes (the `sidebar` scene's eight
/// sessions; claude orange, codex blue, tests green, the long one purple; shell active):
/// - `tabs` and `tabs-narrow`: the rows with their colour dots and ⌘N, at 290 and 180 px (no
///   ages there).
/// - `tabs-hover`: the pointer over codex: a × in its mark's place; `tabs-hover-close`: on the
///   × itself; `tabs-hover-active`: over shell, the active row (which shows its mark otherwise).
/// - `tabs-menu`: codex's tab menu open (right-click), blue ringed.
/// - `tabs-switcher`: ⌘P (the palette on Workspaces), the dots beside the names, over the
///   split's panes (tests and shell).
///
/// Panics unless the dots are drawn in their colour on the rows, the menu opens on the row
/// right-clicked, a click on a swatch sets the colour, and ⌘P opens the palette on Workspaces.
fn tab_scenes() {
    use crate::tab_color::TabColor;
    use gpui::{Modifiers, MouseButton};
    use std::cell::RefCell;
    use std::rc::Rc;

    let colors = [
        (0, TabColor::Orange),
        (1, TabColor::Blue),
        (2, TabColor::Green),
        (4, TabColor::Purple),
    ];
    let dims = size(px(960.0), px(600.0));
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let ui = theme.ui();
        for scene in [
            "tabs",
            "tabs-narrow",
            "tabs-hover",
            "tabs-hover-close",
            "tabs-hover-active",
            "tabs-menu",
            "tabs-switcher",
        ] {
            let width = if scene == "tabs-narrow" {
                180.0
            } else {
                ui_tokens::size::SIDEBAR_DEFAULT_WIDTH
            };
            let dims = if scene == "tabs-narrow" {
                size(px(720.0), px(600.0))
            } else {
                dims
            };
            let slot: Slot = Rc::new(RefCell::new(None));
            let keep = slot.clone();
            let build = move |window: &mut Window, cx: &mut App| -> AnyView {
                cx.set_global(sidebar_appearance(theme, cx));
                let ws = cx.new(|cx| {
                    let mut ws = sidebar_workspace(&EIGHT, None, 3, &[WEB], window, cx);
                    ws.sidebar.width = width;
                    ws.sidebar.hint_dismissed = true;
                    with_progress(&mut ws);
                    for (i, color) in colors {
                        let row = ws.model.display_order()[i];
                        ws.model.set_color(row, Some(color));
                    }
                    ws
                });
                *keep.borrow_mut() = Some(ws.clone());
                ws.into()
            };
            let acting = slot.clone();
            // codex's name line, where its dot is (M11i: its row has one line under its name,
            // 32 pt, the name line's middle 6.5 pt above the row's).
            let dot_y = Rc::new(std::cell::Cell::new(0.0f32));
            let noted = dot_y.clone();
            let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let ws = slotted(&acting);
                let none = Modifiers::none();
                // codex is row 2 (under api's header).
                let codex = row_point(cx, &ws, 2, 0.0);
                noted.set(f32::from(row_point(cx, &ws, 2, -6.5).y));
                match scene {
                    "tabs-hover" => cx.simulate_mouse_move(window, codex, None, none),
                    "tabs-hover-active" => {
                        let shell = row_point(cx, &ws, 4, 0.0);
                        cx.simulate_mouse_move(window, shell, None, none)
                    }
                    "tabs-hover-close" => {
                        // The mark slot: the row's inset and padding, then half the slot.
                        let at = point(px(6.0 + 8.0 + 7.0), px(noted.get()));
                        cx.simulate_mouse_move(window, at, None, none)
                    }
                    "tabs-menu" => {
                        cx.simulate_mouse_down(window, codex, MouseButton::Right, none);
                        let open = cx.read(|cx| {
                            let ws = ws.read(cx);
                            let row = ws.model.display_order()[1];
                            ws.sidebar.tab_menu.as_ref().map(|m| m.workspace == row)
                        });
                        assert_eq!(open, Some(true), "{scene}-{slug}: codex's menu opens");
                    }
                    "tabs-switcher" => {
                        cx.update(|cx| {
                            cx.bind_keys([gpui::KeyBinding::new(
                                "cmd-p",
                                crate::workspace::OpenSwitcher,
                                None,
                            )])
                        });
                        cx.simulate_keystrokes(window, "cmd-p");
                        let scope =
                            cx.read(|cx| ws.read(cx).palette.as_ref().map(|p| p.read(cx).scope()));
                        assert_eq!(
                            scope,
                            Some(crate::palette_items::Scope::Workspaces),
                            "{scene}-{slug}: ⌘P opens the palette on Workspaces"
                        );
                    }
                    _ => {}
                }
                // Not kept past the window: a handle left over is reported as a leak.
                acting.borrow_mut().take();
            };
            let path = shoot_with(scene, slug, &theme, dims, build.clone(), act.clone());
            println!("ui-shots: {}", path.display());
            if scene == "tabs" || scene == "tabs-narrow" {
                // codex's colour is its avatar's (M11r: no separate dot), on its row's line: its
                // letters' since 2026-11-20 (the tile has no fill).
                let shot = render_with(&theme, dims, build, act);
                // The letters sit on the row's middle, 6.5 pt under its name line.
                let y = dot_y.get() + 6.5;
                let mid = crate::sidebar::avatar_ink(Some(TabColor::Blue), &ui);
                let scale = shot.width as f32 / f32::from(dims.width);
                let dots = (0..(width * scale) as usize)
                    .filter(|&x| gap(pixel_at(&shot, dims, x as f32 / scale, y), mid) <= 14)
                    .count();
                assert!(dots > 0, "{scene}-{slug}: codex's blue letters");
            }
        }

        // Acting on rows, no picture: Escape closes the menu; a swatch click sets the colour (and
        // closes it); a middle-click closes a session, and so does its ×.
        let slot: Slot = Rc::new(RefCell::new(None));
        let keep = slot.clone();
        let build = move |window: &mut Window, cx: &mut App| -> AnyView {
            cx.set_global(sidebar_appearance(theme, cx));
            let ws = cx.new(|cx| {
                let mut ws = sidebar_workspace(&EIGHT, None, 3, &[WEB], window, cx);
                ws.sidebar.hint_dismissed = true;
                ws
            });
            *keep.borrow_mut() = Some(ws.clone());
            ws.into()
        };
        let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
            use crate::tab_menu::Dismiss;
            use gpui::KeyBinding;

            let ws = slotted(&slot);
            let none = Modifiers::none();
            let menu_open = |cx: &mut VisualTestAppContext| {
                cx.read(|cx| ws.read(cx).sidebar.tab_menu.is_some())
            };
            let names = |cx: &mut VisualTestAppContext| {
                cx.read(|cx| {
                    let ws = ws.read(cx);
                    let names = ws.sessions.items().iter().map(|s| s.label().to_string());
                    names.collect::<Vec<_>>()
                })
            };
            // As main.rs binds it.
            cx.update(|cx| cx.bind_keys([KeyBinding::new("escape", Dismiss, Some("TabMenu"))]));
            let codex = row_point(cx, &ws, 2, 0.0);
            cx.simulate_mouse_down(window, codex, MouseButton::Right, none);
            assert!(menu_open(cx), "{slug}: the menu opens");
            cx.simulate_keystrokes(window, "escape");
            assert!(!menu_open(cx), "{slug}: Escape closes it");
            cx.simulate_mouse_down(window, codex, MouseButton::Right, none);
            cx.run_until_parked();
            // The blue swatch: found by its colour (no row has a blue dot here).
            let image = cx.capture_screenshot(window).expect("capture the window");
            let shot = Shot {
                width: image.width(),
                height: image.height(),
                rgba: image.as_raw().to_vec(),
            };
            let at = centre_of(&shot, dims, ui.tab(TabColor::Blue)).expect("the blue swatch");
            cx.simulate_click(window, at, none);
            cx.read(|cx| {
                let ws = ws.read(cx);
                let row = ws.model.display_order()[1];
                let color = ws.model.workspace(row).and_then(|w| w.color);
                assert_eq!(color, Some(TabColor::Blue), "{slug}");
                assert!(ws.sidebar.tab_menu.is_none(), "{slug}: the menu closed");
            });
            // Middle-click tests (row 3): closed, and nothing else selected.
            let tests = row_point(cx, &ws, 3, 0.0);
            cx.simulate_mouse_down(window, tests, MouseButton::Middle, none);
            cx.simulate_mouse_up(window, tests, MouseButton::Middle, none);
            assert!(
                !names(cx).contains(&"tests".to_string()),
                "{slug}: middle-click"
            );
            assert_eq!(
                cx.read(|cx| ws.read(cx).sessions.active().map(|s| s.label().to_string())),
                Some("shell".to_string()),
                "{slug}: the active session stays"
            );
            // codex's × (on hover, over its avatar: the row `ROW_INSET` in, the avatar
            // `ROW_PAD` into it, centred in the row).
            use crate::sidebar::{AVATAR, ROW_INSET, ROW_PAD};
            let line = row_point(cx, &ws, 2, 0.0);
            let close = point(px(ROW_INSET + ROW_PAD + AVATAR / 2.0), line.y);
            cx.simulate_mouse_move(window, close, None, none);
            cx.simulate_click(window, close, none);
            let left = names(cx);
            assert!(!left.contains(&"codex".to_string()), "{slug}: the × closes");
            assert_eq!(left.len(), 6, "{slug}: {left:?}");
            slot.borrow_mut().take();
        };
        render_with(&theme, dims, build, act);
    }
}

/// Three panes of the active session: api (left), claude (top right, waiting) and codex (bottom
/// right, working), split as a right split then a down split. `focus` is the focused one.
fn split_workspace(focus: usize, window: &mut Window, cx: &mut Context<Workspace>) -> Workspace {
    use std::time::{Duration, Instant};

    let panes = [
        (shot("api", API, Status::Idle, 0, SHELL_TEXT), None),
        (
            shot("claude", API, Status::Waiting, 4 * 60 + 12, CLAUDE_PROMPT),
            Some(Axis::Row),
        ),
        (
            shot("codex", API, Status::Working, 32, AGENT_TEXT),
            Some(Axis::Column),
        ),
    ];
    let mut ws = Workspace::for_shots(window, cx);
    for (s, axis) in &panes {
        let project = std::path::Path::new(s.project);
        ws.open_shot_session(s.name, project, s.text, *axis, window, cx);
    }
    for (i, (s, _)) in panes.iter().enumerate() {
        ws.set_status(i, s.status, false, cx);
        let session = ws.sessions.get_mut(i).expect("just opened");
        session.status_since = Instant::now() - Duration::from_secs(s.age);
        session.pop = None;
    }
    ws.select(focus, window, cx);
    ws.sidebar.hint_dismissed = true;
    ws
}

/// The middle of the pixels that are `c`, in points; none if there are none.
fn centre_of(
    shot: &Shot,
    dims: Size<Pixels>,
    c: alacritty_terminal::vte::ansi::Rgb,
) -> Option<gpui::Point<Pixels>> {
    let scale = shot.width as f32 / f32::from(dims.width);
    let (mut n, mut sx, mut sy) = (0usize, 0usize, 0usize);
    for (i, p) in shot.rgba.chunks_exact(4).enumerate() {
        if p[..3]
            .iter()
            .zip([c.r, c.g, c.b])
            .all(|(a, b)| a.abs_diff(b) <= 3)
        {
            n += 1;
            sx += i % shot.width as usize;
            sy += i / shot.width as usize;
        }
    }
    (n > 0).then(|| {
        point(
            px(sx as f32 / n as f32 / scale),
            px(sy as f32 / n as f32 / scale),
        )
    })
}

/// How many device pixels along the row at `y` points, between `x0` and `x1` points, are `c`.
/// The resize line's colour over `canvas` (M11x, `panes::resize_line`): faint text at half
/// strength.
pub(super) fn quiet_line(
    canvas: [u8; 3],
    ui: &crate::theme::UiColors,
) -> alacritty_terminal::vte::ansi::Rgb {
    let half = |c: u8, t: u8| ((u16::from(c) + u16::from(t) + 1) / 2) as u8;
    alacritty_terminal::vte::ansi::Rgb {
        r: half(canvas[0], ui.text_faint.r),
        g: half(canvas[1], ui.text_faint.g),
        b: half(canvas[2], ui.text_faint.b),
    }
}

/// A 3-pane split in both themes, from a real `Workspace` (M11al: the panes in the one card,
/// 1 px hairlines between them; the card's inside spans from the sidebar's edge + 1 to
/// `CARD_INSET` + 1 short of the window's right):
/// - `splits`: api focused, claude waiting, codex working; lines quiet.
/// - `splits-waiting`: the waiting pane focused.
/// - `splits-hover`: the pointer over the vertical line (the 2 px quiet line in its middle).
/// - `splits-drag`: that line dragged left (the quiet line, the panes follow).
///
/// Panics unless each shows 3 tabs, the focused pane's shown tab has the accent wash
/// (`tab_active`) and the other pane's shown tab the neutral one (`card_hover`, M11al §3.3; the
/// veil shows the unfocused panes, `card-split`), the bars have no fill above the tabs, and the
/// vertical line is a 1 px `card_border` hairline (never the canvas) except when hovered or
/// dragged.
fn split_scenes() {
    use gpui::{Modifiers, MouseButton};
    use std::cell::RefCell;
    use std::rc::Rc;

    let dims = size(px(960.0), px(600.0));
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let ui = theme.ui();
        for scene in ["splits", "splits-waiting", "splits-hover", "splits-drag"] {
            let slot: Slot = Rc::new(RefCell::new(None));
            let keep = slot.clone();
            let focus = if scene == "splits-waiting" { 1 } else { 0 };
            let build = move |window: &mut Window, cx: &mut App| -> AnyView {
                cx.set_global(shot_appearance(theme, cx));
                let ws = cx.new(|cx| split_workspace(focus, window, cx));
                *keep.borrow_mut() = Some(ws.clone());
                ws.into()
            };
            // The card's inside, and the middle of the vertical hairline: the sides give it up
            // in proportion.
            use crate::ui_tokens::size::{HAIRLINE, SIDEBAR_DEFAULT_WIDTH, TAB_BAR};
            let (area_left, area_right) = (
                SIDEBAR_DEFAULT_WIDTH + HAIRLINE,
                960.0 - CARD_INSET - HAIRLINE,
            );
            let area = area_right - area_left;
            let at = |ratio: f32| area_left + ratio * (area - HAIRLINE) + HAIRLINE / 2.0;
            let acting = slot.clone();
            // api's and claude's tabs' places (the left pane's and the top right one's).
            let places: Rc<RefCell<Vec<gpui::Bounds<Pixels>>>> = Rc::default();
            let seen = places.clone();
            let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let ws = slotted(&acting);
                let tabs = cx.read(|cx| ws.read(cx).tab_sessions().len());
                assert_eq!(tabs, 3, "{scene}-{slug}: a tab per pane");
                let none = Modifiers::none();
                let left = MouseButton::Left;
                let on = point(px(at(0.5)), px(300.0));
                match scene {
                    "splits-hover" => cx.simulate_mouse_move(window, on, None, none),
                    "splits-drag" => {
                        // To 0.4 of the pane area.
                        let to = point(px(area_left + 0.4 * area), px(300.0));
                        cx.simulate_mouse_down(window, on, left, none);
                        cx.simulate_mouse_move(window, on - point(px(40.0), px(0.0)), left, none);
                        cx.simulate_mouse_move(window, to, left, none);
                    }
                    _ => {}
                }
                // Where the tabs are drawn now (a drag re-fits the panes on its next frame).
                draw(cx, window);
                let panes = shown_panes(cx, &ws);
                *seen.borrow_mut() = panes[..2]
                    .iter()
                    .map(|&p| tab_bounds(cx, &ws, p, 0).expect("laid out"))
                    .collect();
                // Not kept past the window: a handle left over is reported as a leak.
                acting.borrow_mut().take();
            };
            let path = shoot_with(scene, slug, &theme, dims, build.clone(), act.clone());
            let shot = render_with(&theme, dims, build, act);
            let n = format!("{scene}-{slug}");
            // Tab fills, away from the text (in each tab's right padding): api (left), claude
            // (top right), both their pane's shown (and only) tab, in the bars that start
            // inside the card's 1 pt edge, `CARD_INSET` + 1 down; the tabs in their middle. The
            // focused pane's has the accent wash, the other's the neutral one (`card_lift`) (claude's too,
            // though it waits: a shown tab isn't tinted).
            let ratio = if scene == "splits-drag" { 0.4 } else { 0.5 };
            let bar_top = CARD_INSET + HAIRLINE;
            let middle = bar_top + TAB_BAR / 2.0;
            let after = crate::tab_bar::AFTER;
            let is = |p: [u8; 3], c: alacritty_terminal::vte::ansi::Rgb| {
                p.iter()
                    .zip([c.r, c.g, c.b])
                    .all(|(a, b)| a.abs_diff(b) <= 3)
            };
            let px_at = |(x, y): (f32, f32)| pixel_at(&shot, dims, x, y);
            for (i, place) in places.borrow().iter().enumerate() {
                let which = ["api", "claude"][i];
                let x = f32::from(place.right()) - after - crate::tab_bar::TAB_PAD / 2.0;
                let want = if i == focus {
                    ui.tab_active
                } else {
                    ui.card_lift
                };
                let p = px_at((x, middle));
                assert!(is(p, want), "{n}: {which}'s tab {p:?}, want {want:?}");
                // No fill above it: the bar is the card.
                let p = px_at((f32::from(place.center().x), bar_top + 3.0));
                assert!(near_by(p, ui.card, 3), "{n}: the bar above {which}: {p:?}");
            }
            // The vertical line, a 1 px `card_border` hairline with no canvas beside it; the
            // 2 px quiet line over it only when hovered or dragged. Measured across at
            // y = 450, in the lower panes: not at 300, where the right column's own line
            // (claude over codex) lies along the row. 3 pt either side: the grab strip's half.
            let (x0, x1) = (at(ratio) - 3.0, at(ratio) + 3.0);
            let scale = shot.width as f32 / 960.0;
            let accent = count_along(&shot, dims, ui.accent, 450.0, x0, x1);
            for x in [x0, x1] {
                let p = px_at((x, 450.0));
                assert!(
                    !is_canvas(p, &ui),
                    "{n}: no canvas by the line at {x}: {p:?}"
                );
            }
            if matches!(scene, "splits-hover" | "splits-drag") {
                // The quiet line (M11x: not the accent), 2 px, over the hairline.
                let quiet = super::window::quiet_over_line(&shot, dims, &ui, 450.0, (x0, x1));
                assert_eq!(quiet, (2.0 * scale) as usize, "{n}: 2 px quiet line");
                assert_eq!(accent, 0, "{n}: the line isn't the accent");
            } else {
                assert_eq!(accent, 0, "{n}: no accent by a quiet line");
                let lines = count_along(&shot, dims, ui.card_border, 450.0, x0, x1);
                assert_eq!(lines, (HAIRLINE * scale) as usize, "{n}: the 1 px hairline");
            }
            println!("ui-shots: {}", path.display());
        }
    }
}

/// The `workspaces` scene's sessions, in the order they open (an axis: split from the one
/// before).
const WORKSPACES: [(ShotSession, Option<Axis>); 5] = [
    (shot("server", WEB, Status::Idle, 0, SHELL_TEXT), None),
    (shot("agent", WEB, Status::Waiting, 95, CLAUDE_PROMPT), None),
    (shot("api", API, Status::Idle, 0, SHELL_TEXT), None),
    (
        shot("tests", API, Status::Done, 12 * 60, SHELL_TEXT),
        Some(Axis::Row),
    ),
    (
        shot("codex", API, Status::Working, 32, AGENT_TEXT),
        Some(Axis::Column),
    ),
];

/// M11b: the sidebar lists workspaces. Web's `server` (idle) and `agent` (waiting, its own
/// workspace), then api: a workspace of three terminals (`api` left, `tests` done top right,
/// `codex` working bottom right), focused on `api`. In both themes:
/// - `workspaces`: api's three panes on screen; the sidebar has one row per workspace, api's
///   with its most urgent terminal's ring (tests: done) and agent's waiting with its question.
///
/// Panics unless the sidebar has the two headers and three rows (not five), api's row says
/// tests' status, the waiting row and api's are both 32 pt (M11i: one line under the name),
/// and the three panes each have a tab.
fn workspace_scenes() {
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::time::{Duration, Instant};

    let dims = size(px(960.0), px(600.0));
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let slot: Slot = Rc::new(RefCell::new(None));
        let keep = slot.clone();
        let build = move |window: &mut Window, cx: &mut App| -> AnyView {
            cx.set_global(sidebar_appearance(theme, cx));
            let ws = cx.new(|cx| {
                let mut ws = Workspace::for_shots(window, cx);
                ws.sidebar.hint_dismissed = true;
                for (s, axis) in &WORKSPACES {
                    let project = std::path::Path::new(s.project);
                    ws.open_shot_session(s.name, project, s.text, *axis, window, cx);
                }
                for (s, _) in &WORKSPACES {
                    let i = ws
                        .sessions
                        .position(|x| x.label() == s.name)
                        .expect("opened");
                    ws.set_status(i, s.status, false, cx);
                    let session = ws.sessions.get_mut(i).expect("opened");
                    session.status_since = Instant::now() - Duration::from_secs(s.age);
                    session.pop = None;
                }
                let api = ws.sessions.position(|x| x.label() == "api").expect("api");
                ws.select(api, window, cx);
                ws
            });
            *keep.borrow_mut() = Some(ws.clone());
            ws.into()
        };
        let check = move |cx: &mut VisualTestAppContext, _: AnyWindowHandle| {
            let ws = slotted(&slot);
            cx.read(|cx| {
                let ws = ws.read(cx);
                let n = format!("workspaces-{slug}");
                assert_eq!(ws.model.workspaces().len(), 3, "{n}: three workspaces");
                // Web's header, server, agent; api's header, api.
                assert_eq!(ws.sidebar.scroll.children_count(), 5, "{n}: rows");
                let height = |i: usize| {
                    let b = ws.sidebar.scroll.bounds_for_item(i).expect("laid out");
                    f32::from(b.size.height)
                };
                // The waiting one with "Needs input" under its name; api, in its folder's group,
                // who and where under its name: every row `SIDEBAR_ROW_TWO` (56, M11al).
                let two = crate::ui_tokens::size::SIDEBAR_ROW_TWO;
                assert_eq!(height(2), two, "{n}: agent's waiting row");
                assert_eq!(height(4), two, "{n}: api's row");
                let api = ws.model.focused_workspace().expect("focused");
                let ring = ws
                    .rollup_session(api)
                    .map(|i| ws.sessions.items()[i].status);
                assert_eq!(ring, Some(Status::Done), "{n}: api's ring is tests'");
                assert_eq!(ws.tab_sessions().len(), 3, "{n}: three panes");
            });
            slot.borrow_mut().take();
        };
        let path = shoot_with("workspaces", slug, &theme, dims, build, check);
        println!("ui-shots: {}", path.display());
    }
}

/// What a bar keeps from its tabs (`tab_bar`, M11al §4.6): + right after the row (`PLUS_GAP`
/// on; chore 2026-11-20), then ⋯ and the right sidebar's pinned, `BUTTON_GAP` apart,
/// `BUTTONS_PAD` in from the ends.
const BAR_BUTTONS: f32 = crate::tab_bar::PLUS_GAP
    + crate::tab_bar::BUTTON
    + 2.0 * crate::tab_bar::BUTTONS_PAD
    + 2.0 * crate::tab_bar::BUTTON
    + crate::tab_bar::BUTTON_GAP;

/// How far left of its pane's right edge ⋯'s centre is in a bar that isn't the top-right one
/// (the left bars these scenes use): the bar's last button since 2026-11-20 (the right
/// sidebar's button is the top-right bar's alone), half a button in from its padding.
const MORE_FROM_EDGE: f32 = crate::tab_bar::BUTTONS_PAD + crate::tab_bar::BUTTON / 2.0;

/// A bar's buttons sit in its middle.
const BUTTON_TOP: f32 = (ui_tokens::size::TAB_BAR - crate::tab_bar::BUTTON) / 2.0;

/// What a tab's place in a bar holds after the tab: the separator and its room (M11al §4.2).
const AFTER: f32 = crate::tab_bar::AFTER;

/// The two-pane tab bar and tab drag scenes' window width: wide enough (M11al: 100 pt tabs,
/// 28 pt buttons) that the left pane's four tabs fit unscrolled.
const TAB_BARS_WIDTH: f32 = 1440.0;

/// The left pane's bar in the two-pane scenes (M11al): half the inside of the one card (from
/// the sidebar's edge to `CARD_INSET` short of the window's right, less its 1 pt edge on either
/// side), less half the 1 pt hairline between the panes.
const LEFT_BAR_WIDTH: f32 = (TAB_BARS_WIDTH
    - ui_tokens::size::SIDEBAR_DEFAULT_WIDTH
    - ui_tokens::size::CARD_INSET
    - 2.0 * ui_tokens::size::HAIRLINE
    - ui_tokens::size::HAIRLINE)
    / 2.0;

/// Pane tab bars (M11b Task 3; M11al's minimal tabs, §4), from real workspaces in both themes:
/// - `tab-bars`: two panes; the left (focused) one with api (shown: the accent wash,
///   `tab_active`), claude (waiting, tinted), codex (working, no fill) and a browser tab ("New
///   tab", globe); the right one with tests (done; shown in an unfocused pane: `card_hover`).
///   The `TAB_BAR` bars have no fill (the card) and a `card_border` line along their bottom;
///   their tabs are `TAB` tall pills in the middle, each as wide as its title (chore 2026-11-20).
/// - `tab-bars-hover`: the pointer over codex (`card_lift`, × shown); `tab-bars-button`: over
///   the left bar's + (right after its last tab).
/// - `tab-bars-more`: the left bar's ⋯ clicked: its menu (New Terminal Tab ⌘T, New Browser Tab,
///   Split Right ⌘D, Split Down ⇧⌘D) under it, right edges together, the ⋯ lit.
/// - `tab-bars-menu`: claude's right-click menu (Close Tab, Close Other Tabs in Pane, Rename
///   Tab…); `tab-bars-rename`: codex double-clicked (shown, its rename box open).
/// - `tab-bars-single`: one pane, one tab (the bar still shows).
/// - `tab-bars-narrow`: a narrow window, one pane of seven tabs, the last shown: the tabs stop at
///   `TAB_MIN_WIDTH` and the row scrolls to the shown one.
///
/// Panics unless the tabs have the geometry and fills above, the menus and rename box open
/// where clicked; and (not shot) unless ⌃Tab / ⌃⇧Tab cycle the focused pane's tabs, a click
/// shows a tab, × closes one, a middle-click closes one, Escape closes the ⋯ menu, and its
/// Split Right splits the pane whose ⋯ it was (not the focused one).
fn tab_bar_scenes() {
    use crate::layout_model::TabKind;
    use crate::workspace::{NextTab, PrevTab, Renaming};
    use gpui::{KeyBinding, Modifiers, MouseButton, MouseDownEvent, MouseUpEvent};
    use std::cell::RefCell;
    use std::rc::Rc;

    let wide = size(px(TAB_BARS_WIDTH), px(600.0));
    use crate::tab_bar::{BUTTON, BUTTON_GAP, CLOSE, TAB, TAB_MAX_WIDTH, TAB_MIN_WIDTH, TAB_PAD};
    use crate::ui_tokens::size::{HAIRLINE, TAB_BAR};
    // The bars' top, inside the card's 1 pt edge under the `CARD_INSET` strip; the tabs' top,
    // `TAB` tall in the bar's middle.
    let bar_top = CARD_INSET + HAIRLINE;
    let tabs_top = bar_top + (TAB_BAR - TAB) / 2.0;
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let ui = theme.ui();
        let is = |p: [u8; 3], c: alacritty_terminal::vte::ansi::Rgb| {
            p.iter()
                .zip([c.r, c.g, c.b])
                .all(|(a, b)| a.abs_diff(b) <= 3)
        };
        for scene in [
            "tab-bars",
            "tab-bars-hover",
            "tab-bars-button",
            "tab-bars-more",
            "tab-bars-menu",
            "tab-bars-rename",
        ] {
            let slot: Slot = Rc::new(RefCell::new(None));
            let keep = slot.clone();
            let build = move |window: &mut Window, cx: &mut App| -> AnyView {
                cx.set_global(shot_appearance(theme, cx));
                let ws = cx.new(|cx| tab_bar_workspace(true, 0, window, cx));
                *keep.borrow_mut() = Some(ws.clone());
                ws.into()
            };
            let geometry: Rc<RefCell<Vec<gpui::Bounds<Pixels>>>> = Rc::default();
            let seen = geometry.clone();
            let acting = slot.clone();
            let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let ws = slotted(&acting);
                let n = format!("{scene}-{slug}");
                let none = Modifiers::none();
                let panes = shown_panes(cx, &ws);
                assert_eq!(panes.len(), 2, "{n}: two panes");
                let tabs: Vec<_> = (0..4)
                    .map(|i| tab_bounds(cx, &ws, panes[0], i).expect("laid out"))
                    .collect();
                // Four tabs in the left bar, each as wide as its title within the bounds (each
                // place holds its pill and the gap after it; chore 2026-11-20); `TAB` tall in the
                // bar's middle.
                for b in &tabs {
                    let w = f32::from(b.size.width) - AFTER;
                    assert!(
                        (TAB_MIN_WIDTH - 0.5..=TAB_MAX_WIDTH + 0.5).contains(&w),
                        "{n}: tab width {w}"
                    );
                    assert_eq!(f32::from(b.top()), tabs_top, "{n}: in the bar's middle");
                    assert_eq!(f32::from(b.size.height), TAB, "{n}: `TAB` tall");
                }
                // "New tab" (the browser's title) is wider than "api": not all one width.
                let width = |b: &gpui::Bounds<Pixels>| f32::from(b.size.width);
                assert!(
                    width(&tabs[3]) > width(&tabs[0]),
                    "{n}: as wide as their titles"
                );
                assert_eq!(
                    cx.read(|cx| ws.read(cx).tab_sessions().len()),
                    4,
                    "{n}: four terminal tabs on screen"
                );
                *seen.borrow_mut() = tabs.clone();
                match scene {
                    "tab-bars-hover" => {
                        cx.simulate_mouse_move(window, tabs[2].center(), None, none);
                    }
                    "tab-bars-button" => {
                        // The left pane's +, right after its last tab.
                        let at = plus_centre(cx, &ws, panes[0]).expect("the left bar's +");
                        cx.simulate_mouse_move(window, at, None, none);
                    }
                    "tab-bars-more" => {
                        // The left bar's ⋯: its menu hangs from the button's bottom right.
                        let right = tabs[0].left() - px(crate::ui_tokens::size::TABS_LEFT)
                            + px(LEFT_BAR_WIDTH);
                        let at = point(right - px(MORE_FROM_EDGE), tabs[0].center().y);
                        cx.simulate_click(window, at, none);
                        let menu = cx.read(|cx| {
                            let ws = ws.read(cx);
                            ws.pane_actions.as_ref().map(|m| (m.pane, m.at))
                        });
                        let corner = point(
                            right - px(MORE_FROM_EDGE - BUTTON / 2.0),
                            px(bar_top + BUTTON_TOP + BUTTON),
                        );
                        // (Within half a point: the bar's half-point width rounds.)
                        let (pane, at) = menu.expect("the ⋯ menu");
                        assert_eq!(pane, panes[0], "{n}: the left ⋯'s menu");
                        assert!(
                            (at.x - corner.x).abs() <= px(0.5) && at.y == corner.y,
                            "{n}: under the left ⋯: {at:?}, {corner:?}"
                        );
                    }
                    "tab-bars-menu" => {
                        cx.simulate_mouse_down(window, tabs[1].center(), MouseButton::Right, none);
                        let menu = cx.read(|cx| ws.read(cx).pane_tab_menu.as_ref().map(|m| m.tab));
                        let claude =
                            cx.read(|cx| ws.read(cx).model.pane(panes[0]).unwrap().tabs[1].id);
                        assert_eq!(menu, Some(claude), "{n}: the menu opens on claude");
                    }
                    "tab-bars-rename" => {
                        let at = tabs[2].center();
                        for count in [1, 2] {
                            cx.simulate_event(
                                window,
                                MouseDownEvent {
                                    position: at,
                                    modifiers: none,
                                    button: MouseButton::Left,
                                    click_count: count,
                                    first_mouse: false,
                                },
                            );
                            cx.simulate_event(
                                window,
                                MouseUpEvent {
                                    position: at,
                                    modifiers: none,
                                    button: MouseButton::Left,
                                    click_count: count,
                                },
                            );
                        }
                        cx.read(|cx| {
                            let ws = ws.read(cx);
                            let codex = ws.sessions.items().iter().find(|s| s.label() == "codex");
                            let codex = codex.expect("codex").id;
                            assert_eq!(ws.focused_terminal(), Some(codex), "{n}: codex shown");
                            let renaming = ws.renaming.as_ref().map(|(r, _)| *r);
                            assert_eq!(
                                renaming,
                                Some(Renaming::Terminal(codex)),
                                "{n}: rename box"
                            );
                        });
                    }
                    _ => {}
                }
                acting.borrow_mut().take();
            };
            let path = shoot_with(scene, slug, &theme, wide, build.clone(), act.clone());
            let shot = render_with(&theme, wide, build, act);
            let n = format!("{scene}-{slug}");
            let px_at = |x: f32, y: f32| pixel_at(&shot, wide, x, y);
            let tabs = geometry.borrow().clone();
            let (api, claude, codex) = (tabs[0], tabs[1], tabs[2]);
            let middle = f32::from(api.center().y);
            let bar_right =
                f32::from(api.left()) - crate::ui_tokens::size::TABS_LEFT + LEFT_BAR_WIDTH;
            // A tab's right end (its place less the separator's room).
            let right_of = |b: gpui::Bounds<Pixels>| f32::from(b.right()) - AFTER;
            // Away from the text, the tile, the dot and the ×: in each tab's right padding.
            let fill_at = |b: gpui::Bounds<Pixels>| (right_of(b) - TAB_PAD / 2.0, middle);
            // Whether something is drawn in tab `b`'s × box (on a tab filled `fill`).
            let close_drawn = |b: gpui::Bounds<Pixels>, fill| {
                let right = right_of(b) - TAB_PAD;
                (2..=(CLOSE as i32 - 2)).any(|dx| {
                    (-4..=4).any(|dy| gap(px_at(right - dx as f32, middle + dy as f32), fill) > 30)
                })
            };
            // The corner of a bar button whose centre is `from_edge` left of the bar's right.
            let button_corner = |from_edge: f32| {
                px_at(
                    bar_right - from_edge - BUTTON / 2.0 + 2.0,
                    bar_top + BUTTON_TOP + 2.0,
                )
            };
            match scene {
                "tab-bars" => {
                    // api, the focused pane's shown tab: the accent wash, standing off the bar.
                    let (x, y) = fill_at(api);
                    let shown = px_at(x, y);
                    assert!(
                        is(shown, ui.tab_active),
                        "{n}: api is `tab_active`: {shown:?}"
                    );
                    assert!(!is(shown, ui.card), "{n}: api stands off the bar");
                    // The bar: no fill above the tabs (the card), its `card_border` line along
                    // its bottom, the terminal right under it.
                    let above = px_at(x, bar_top + 2.0);
                    assert!(
                        near_by(above, ui.card, 3),
                        "{n}: the bar above api: {above:?}"
                    );
                    let line = px_at(x, bar_top + TAB_BAR - 0.5);
                    assert!(is(line, ui.card_border), "{n}: the bar's line: {line:?}");
                    let under = px_at(x, bar_top + TAB_BAR + 1.5);
                    assert!(is(under, theme.background), "{n}: the terminal: {under:?}");
                    let (x, y) = fill_at(claude);
                    assert!(is(px_at(x, y), ui.waiting_tint), "{n}: claude tinted");
                    // codex: a quiet pill (chore 2026-11-20), no ×.
                    let (x, y) = fill_at(codex);
                    assert!(is(px_at(x, y), ui.card_hover), "{n}: codex a quiet pill");
                    assert!(!close_drawn(codex, ui.card_hover), "{n}: codex's × hidden");
                    // The right pane's tests: shown in the unfocused pane, the neutral wash
                    // (M11al §3.3; `card_lift` among pills), in its left padding.
                    let tests_left = f32::from(api.left()) + LEFT_BAR_WIDTH + HAIRLINE;
                    let x = tests_left + TAB_PAD / 2.0;
                    let p = px_at(x, middle);
                    assert!(is(p, ui.card_lift), "{n}: tests is `card_lift`: {p:?}");
                    let line = px_at(x, bar_top + TAB_BAR - 0.5);
                    assert!(
                        is(line, ui.card_border),
                        "{n}: the right bar's line: {line:?}"
                    );
                }
                "tab-bars-hover" => {
                    let (x, y) = fill_at(codex);
                    let p = px_at(x, y);
                    assert!(is(p, ui.card_lift), "{n}: codex hovered: {p:?}");
                    assert!(close_drawn(codex, ui.card_lift), "{n}: codex's × shown");
                }
                "tab-bars-button" => {
                    // + right after the last tab: its corner, hovered.
                    let plus_x = f32::from(tabs[3].right()) + crate::tab_bar::PLUS_GAP;
                    let plus = px_at(plus_x + 2.0, bar_top + BUTTON_TOP + 2.0);
                    let more = button_corner(MORE_FROM_EDGE);
                    assert!(near_by(plus, ui.card_hover, 3), "{n}: + hovered: {plus:?}");
                    assert!(near_by(more, ui.card, 3), "{n}: ⋯ not: {more:?}");
                }
                "tab-bars-more" => {
                    // ⋯ lit: its fill stands off the bar in the gap beside it (both under the
                    // menu's shadow, so compared with each other, not the tokens).
                    let more = button_corner(MORE_FROM_EDGE);
                    let beside = px_at(
                        bar_right - MORE_FROM_EDGE - BUTTON / 2.0 - BUTTON_GAP / 2.0,
                        bar_top + BUTTON_TOP + 2.0,
                    );
                    let rgb = |[r, g, b]: [u8; 3]| alacritty_terminal::vte::ansi::Rgb { r, g, b };
                    let h = ui.card_hover;
                    let lit = gap([h.r, h.g, h.b], ui.card);
                    assert!(
                        gap(more, rgb(beside)) * 2 >= lit,
                        "{n}: ⋯ lit: {more:?} beside {beside:?}"
                    );
                    // The menu's top right at the ⋯'s bottom right.
                    let (mx, my) = (
                        bar_right - MORE_FROM_EDGE + BUTTON / 2.0,
                        bar_top + BUTTON_TOP + BUTTON,
                    );
                    let inside = px_at(mx - 2.5, my + 30.0);
                    assert!(is(inside, ui.panel), "{n}: the menu under ⋯: {inside:?}");
                    let right = px_at(mx + 3.0, my + 30.0);
                    assert!(!is(right, ui.panel), "{n}: right edges together: {right:?}");
                    let above = px_at(mx - 100.0, my - 2.0);
                    assert!(!is(above, ui.panel), "{n}: under the button: {above:?}");
                    // Six items (26 pt each, inside the 1 pt edge and 4 pt padding), each with
                    // its name; the keys at the right of all but New Browser Tab and (M11aa)
                    // the New ⟨agent⟩ Tab items, one per provider; the menu ends after the last.
                    let inked = |x0: f32, x1: f32, y: f32| {
                        (x0 as i32..x1 as i32).any(|x| {
                            (-4..=4).any(|dy| gap(px_at(x as f32, y + dy as f32), ui.panel) > 60)
                        })
                    };
                    let items = crate::pane_actions::PaneAction::items(true);
                    assert_eq!(items.len(), 6, "{n}: the menu's items");
                    for (k, item) in items.iter().enumerate() {
                        let y = my + 5.0 + 26.0 * k as f32 + 13.0;
                        assert!(inked(mx - 192.0, mx - 110.0, y), "{n}: item {k}'s name");
                        let keys = inked(mx - 40.0, mx - 10.0, y);
                        assert_eq!(keys, item.keys().is_some(), "{n}: item {k}'s keys");
                    }
                    let end = my + 1.0 + 4.0 + items.len() as f32 * 26.0 + 4.0 + 1.0;
                    // (Clear of the panel's rounded corners.)
                    let last = px_at(mx - 20.0, end - 3.0);
                    assert!(is(last, ui.panel), "{n}: the menu's foot: {last:?}");
                    let past = px_at(mx - 20.0, end + 3.0);
                    assert!(!is(past, ui.panel), "{n}: nothing after the last: {past:?}");
                }
                _ => {}
            }
            println!("ui-shots: {}", path.display());
        }

        // One pane, one tab; and a narrow window whose seven tabs scroll.
        for (scene, many, dims) in [
            ("tab-bars-single", 1, size(px(960.0), px(400.0))),
            ("tab-bars-narrow", 7, size(px(720.0), px(400.0))),
        ] {
            let slot: Slot = Rc::new(RefCell::new(None));
            let keep = slot.clone();
            let build = move |window: &mut Window, cx: &mut App| -> AnyView {
                cx.set_global(shot_appearance(theme, cx));
                let ws = cx.new(|cx| tab_bar_workspace(false, many, window, cx));
                *keep.borrow_mut() = Some(ws.clone());
                ws.into()
            };
            let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                // A second frame: the first laid the tabs out, this one scrolls to the shown one.
                cx.update_window(window, |_, window, _| window.refresh())
                    .expect("refresh");
                cx.run_until_parked();
                let ws = slotted(&slot);
                let n = format!("{scene}-{slug}");
                let panes = shown_panes(cx, &ws);
                assert_eq!(panes.len(), 1, "{n}: one pane");
                let first = tab_bounds(cx, &ws, panes[0], 0).expect("laid out");
                let offset = cx.read(|cx| {
                    let scrolls = ws.read(cx).tab_scroll.borrow();
                    f32::from(scrolls[&panes[0]].0.offset().x)
                });
                let width = f32::from(first.size.width) - AFTER;
                if many == 1 {
                    // One tab: as wide as its title (chore 2026-11-20), within the bounds.
                    assert!(
                        (TAB_MIN_WIDTH..=TAB_MAX_WIDTH).contains(&width),
                        "{n}: as wide as its title: {width}"
                    );
                    assert_eq!(offset, 0.0, "{n}: no scroll");
                } else {
                    assert_eq!(width, TAB_MIN_WIDTH, "{n}: narrowest");
                    assert!(offset < 0.0, "{n}: scrolled to the last tab");
                    let last = tab_bounds(cx, &ws, panes[0], many - 1).expect("laid out");
                    assert!(
                        f32::from(last.right()) - AFTER
                            <= 720.0 - CARD_INSET - 1.0 - BAR_BUTTONS + 1.0,
                        "{n}: last in view"
                    );
                }
                slot.borrow_mut().take();
            };
            let path = shoot_with(scene, slug, &theme, dims, build, act);
            println!("ui-shots: {}", path.display());
        }

        // Acting on the bars (not shot): ⌃Tab / ⌃⇧Tab, a click, ×, a middle-click, split.
        let slot: Slot = Rc::new(RefCell::new(None));
        let keep = slot.clone();
        let build = move |window: &mut Window, cx: &mut App| -> AnyView {
            cx.set_global(shot_appearance(theme, cx));
            let ws = cx.new(|cx| tab_bar_workspace(true, 0, window, cx));
            *keep.borrow_mut() = Some(ws.clone());
            ws.into()
        };
        let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
            let ws = slotted(&slot);
            let n = format!("tab-bars-act-{slug}");
            let none = Modifiers::none();
            // As main.rs binds them.
            cx.update(|cx| {
                cx.bind_keys([
                    KeyBinding::new("ctrl-tab", NextTab, Some("Terminal")),
                    KeyBinding::new("ctrl-shift-tab", PrevTab, Some("Terminal")),
                ])
            });
            let shown = |cx: &mut VisualTestAppContext| {
                cx.read(|cx| {
                    let ws = ws.read(cx);
                    let tab = ws.model.focused_tab().expect("a tab");
                    match &ws.model.tab(tab).expect("known").kind {
                        TabKind::Terminal(id) => {
                            let i = ws.index_of(*id).expect("a session");
                            ws.sessions.items()[i].label().to_string()
                        }
                        TabKind::Browser { .. } => "browser".to_string(),
                        TabKind::File { .. } => "file".to_string(),
                        TabKind::Agent { .. } => "agent".to_string(),
                    }
                })
            };
            let panes = shown_panes(cx, &ws);
            assert_eq!(shown(cx), "api", "{n}");
            cx.simulate_keystrokes(window, "ctrl-tab");
            assert_eq!(shown(cx), "claude", "{n}: ⌃Tab");
            cx.simulate_keystrokes(window, "ctrl-shift-tab ctrl-shift-tab");
            assert_eq!(shown(cx), "browser", "{n}: ⌃⇧Tab wraps");
            // The browser tab has no terminal to take the keys: show api by a click.
            let api = tab_bounds(cx, &ws, panes[0], 0).expect("laid out");
            cx.simulate_click(window, api.center(), none);
            assert_eq!(shown(cx), "api", "{n}: a click shows a tab");
            // The right pane's tests, clicked: it and its pane take the focus.
            let tests = tab_bounds(cx, &ws, panes[1], 0).expect("laid out");
            cx.simulate_click(window, tests.center(), none);
            assert_eq!(shown(cx), "tests", "{n}: a click in the other pane");
            let count = |cx: &mut VisualTestAppContext| {
                cx.read(|cx| ws.read(cx).model.pane(panes[0]).map_or(0, |p| p.tabs.len()))
            };
            // The browser tab's ×: on hover at its right end.
            let browser = tab_bounds(cx, &ws, panes[0], 3).expect("laid out");
            let close = point(
                browser.right() - px(AFTER + TAB_PAD + CLOSE / 2.0),
                browser.center().y,
            );
            cx.simulate_mouse_move(window, close, None, none);
            cx.simulate_click(window, close, none);
            assert_eq!(count(cx), 3, "{n}: × closed the browser tab");
            // A middle-click on codex ends it.
            let codex = tab_bounds(cx, &ws, panes[0], 2).expect("laid out");
            cx.simulate_mouse_down(window, codex.center(), MouseButton::Middle, none);
            cx.simulate_mouse_up(window, codex.center(), MouseButton::Middle, none);
            assert_eq!(count(cx), 2, "{n}: middle-click closed codex");
            let codex_gone = cx.read(|cx| ws.read(cx).sessions.position(|s| s.label() == "codex"));
            assert_eq!(codex_gone, None, "{n}: codex ended");
            // The left bar's ⋯ (the right pane, tests, has the focus): Escape closes its menu.
            let right = api.left() - px(crate::ui_tokens::size::TABS_LEFT) + px(LEFT_BAR_WIDTH);
            let more = point(right - px(MORE_FROM_EDGE), api.center().y);
            cx.update(|cx| {
                cx.bind_keys([KeyBinding::new(
                    "escape",
                    crate::tab_menu::Dismiss,
                    Some("TabMenu"),
                )])
            });
            let open = |cx: &mut VisualTestAppContext| {
                cx.read(|cx| ws.read(cx).pane_actions.as_ref().map(|m| (m.pane, m.at)))
            };
            cx.simulate_click(window, more, none);
            let (pane, at) = open(cx).expect("the ⋯ menu");
            assert_eq!(pane, panes[0], "{n}: the left pane's menu");
            cx.simulate_keystrokes(window, "escape");
            assert_eq!(open(cx), None, "{n}: Escape closes it");
            // Again, and its Split Right (the fourth item since M11aa's New Claude Tab): the
            // left pane splits, beside it a third pane, focused; the right pane is still last.
            cx.simulate_click(window, more, none);
            assert!(open(cx).is_some(), "{n}: open again");
            let k = crate::pane_actions::PaneAction::items(true)
                .iter()
                .position(|a| *a == crate::pane_actions::PaneAction::SplitRight)
                .expect("Split Right") as f32;
            let split = point(at.x - px(100.0), at.y + px(5.0 + 26.0 * k + 13.0));
            cx.simulate_click(window, split, none);
            assert_eq!(open(cx), None, "{n}: a choice closes it");
            let panes_now = shown_panes(cx, &ws);
            assert_eq!(panes_now.len(), 3, "{n}: Split Right made a pane");
            assert_eq!(
                (panes_now[0], panes_now[2]),
                (panes[0], panes[1]),
                "{n}: beside the left pane"
            );
            let focused = cx.read(|cx| ws.read(cx).model.focused_pane());
            assert_eq!(focused, Some(panes_now[1]), "{n}: the new pane focused");
            slot.borrow_mut().take();
        };
        render_with(&theme, wide, build, act);
        println!("ui-shots: tab-bars-act-{slug} ok");
    }
}

/// `tab_bar_workspace`'s two panes (left: api, claude, codex, a browser tab; right: tests) and,
/// below it in the sidebar, a second workspace with one tab, docs. api is shown.
fn tab_drag_workspace(window: &mut Window, cx: &mut Context<Workspace>) -> Workspace {
    let mut ws = tab_bar_workspace(true, 0, window, cx);
    let project = std::path::Path::new(API);
    ws.open_shot_session("docs", project, SHELL_TEXT, None, window, cx);
    let api = ws.sessions.position(|x| x.label() == "api").expect("api");
    ws.select(api, window, cx);
    ws
}

/// Where a tab-drag scene's checks look (see `tab_drag_scenes`).
#[derive(Clone, Default)]
struct TabDragGeometry {
    /// The left bar's four tabs' places, and the right bar's first.
    tabs: Vec<gpui::Bounds<Pixels>>,
    right: gpui::Bounds<Pixels>,
    /// Where the chip's top-left should be: the pointer less where the tab was taken.
    chip: gpui::Point<Pixels>,
    /// docs's sidebar row.
    row: gpui::Bounds<Pixels>,
}

/// Wait out a drag's slides (M11b Task 7b), then draw again: everything is where it slides to.
pub(super) fn settle_slides(cx: &mut VisualTestAppContext, window: AnyWindowHandle) {
    prof::sleep(crate::drag_gap::SLIDE + std::time::Duration::from_millis(30));
    cx.update_window(window, |_, window, _| window.refresh())
        .expect("refresh");
    cx.run_until_parked();
}

/// Dragging pane tabs (M11b Tasks 7 and 7b), from real workspaces in both themes, each shot once
/// the other tabs have slid out of the way:
/// - `tab-drag-gap`: api dragged one place right, between claude and codex: claude slid left into
///   api's place, the gap where claude was, api's chip (icon and title) where it was taken (its
///   top-left the pointer less the grab), no insertion line.
/// - `tab-drag-other`: api dragged over the right pane's bar, before tests: the gap there (tests
///   slid right by api's width), its own bar closed up.
/// - `tab-drag-row`: codex dragged onto docs's sidebar row: the row lit and ringed in the accent,
///   its own bar closed up.
///
/// Panics unless the gaps, the chip and the ring are drawn there; and (not shot) unless dropping reorders
/// the bar, moves a tab into the other pane's bar (its empty part: the end), a pane emptied by
/// a move closes, a drop on a row moves the tab to the end of that workspace's pane and the view
/// follows it, a workspace emptied by a move closes without asking, a browser tab moves with its
/// view, a drop elsewhere or Esc changes nothing, and a moved terminal keeps its `sN`, view,
/// screen, status and running program, with the keyboard.
fn tab_drag_scenes() {
    use crate::grid::TextScope;
    use crate::layout_model::{PaneId, TabDrop, TabKind, WorkspaceId};
    use crate::pane_browser::page_hidden;
    use gpui::{Focusable, Modifiers, MouseButton};
    use std::cell::RefCell;
    use std::rc::Rc;

    let wide = size(px(TAB_BARS_WIDTH), px(600.0));
    let none = Modifiers::none();
    let left = MouseButton::Left;
    // Press on `from`, move past GPUI's drag threshold, then to `to` (still held).
    let drag = move |cx: &mut VisualTestAppContext,
                     window: AnyWindowHandle,
                     from: gpui::Point<Pixels>,
                     to: gpui::Point<Pixels>| {
        cx.simulate_mouse_down(window, from, left, none);
        cx.simulate_mouse_move(window, from + point(px(8.0), px(0.0)), left, none);
        cx.simulate_mouse_move(window, to, left, none);
    };
    // The sidebar's second workspace (docs's) and its row's place in the list (its group's
    // header first, then the two workspaces).
    let docs_row = 2;
    let workspaces = |cx: &mut VisualTestAppContext, ws: &gpui::Entity<Workspace>| {
        cx.read(|cx| {
            let model = &ws.read(cx).model;
            model.display_order()
        })
    };
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let ui = theme.ui();
        let is = |p: [u8; 3], c: alacritty_terminal::vte::ansi::Rgb| {
            p.iter()
                .zip([c.r, c.g, c.b])
                .all(|(a, b)| a.abs_diff(b) <= 3)
        };
        for scene in ["tab-drag-gap", "tab-drag-other", "tab-drag-row"] {
            let slot: Slot = Rc::new(RefCell::new(None));
            let keep = slot.clone();
            let build = move |window: &mut Window, cx: &mut App| -> AnyView {
                cx.set_global(shot_appearance(theme, cx));
                let ws = cx.new(|cx| tab_drag_workspace(window, cx));
                *keep.borrow_mut() = Some(ws.clone());
                ws.into()
            };
            // What the checks need: the left bar's four tabs' places, the right bar's first, the
            // chip's expected top-left (the pointer less where the tab was taken), and docs's row.
            let geometry: Rc<RefCell<TabDragGeometry>> = Rc::default();
            let seen = geometry.clone();
            let acting = slot.clone();
            let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let ws = slotted(&acting);
                let n = format!("{scene}-{slug}");
                let panes = shown_panes(cx, &ws);
                assert_eq!(panes.len(), 2, "{n}: two panes");
                let tabs: Vec<_> = (0..4)
                    .map(|i| tab_bounds(cx, &ws, panes[0], i).expect("laid out"))
                    .collect();
                let right = tab_bounds(cx, &ws, panes[1], 0).expect("laid out");
                let order = workspaces(cx, &ws);
                assert_eq!(order.len(), 2, "{n}: two workspaces");
                let drop = cx.read(|cx| ws.read(cx).tab_drop);
                assert_eq!(drop, None, "{n}: nothing dragged yet");
                // The tab taken (its middle), the pointer's end, and where it lands.
                let (taken, to, lands) = match scene {
                    // api, one place right: its chip's centre 10 px left of claude's middle,
                    // which is past claude as closed up (in api's place) and short of codex as
                    // closed up (in claude's): it lands between them.
                    "tab-drag-gap" => {
                        let to = point(tabs[1].center().x - px(10.0) + px(8.0), tabs[0].center().y);
                        let at = TabDrop::Bar {
                            pane: panes[0],
                            gap: 2,
                        };
                        (0, to, at)
                    }
                    // api over the right bar, just in from its first tab's left: before tests.
                    "tab-drag-other" => {
                        let to = point(right.left() + px(5.0), tabs[0].center().y);
                        let at = TabDrop::Bar {
                            pane: panes[1],
                            gap: 0,
                        };
                        (0, to, at)
                    }
                    // codex onto docs's row.
                    _ => {
                        let to = row_point(cx, &ws, docs_row, 0.0);
                        (2, to, TabDrop::Workspace(order[1]))
                    }
                };
                let from = tabs[taken].center();
                drag(cx, window, from, to);
                let drop = cx.read(|cx| ws.read(cx).tab_drop);
                assert_eq!(drop, Some(lands), "{n}: where it lands");
                assert!(cx.read(|cx| cx.has_active_drag()), "{n}: still dragging");
                // GPUI took the tab where the pointer was as the drag started (past its 8 px
                // move): the chip keeps that spot under the pointer.
                let grab = from + point(px(8.0), px(0.0)) - tabs[taken].origin;
                let held = cx
                    .read(|cx| ws.read(cx).tab_drag.get())
                    .expect("a tab's drag");
                assert_eq!(held.at, grab, "{n}: where the tab was taken");
                settle_slides(cx, window);
                let row = cx.read(|cx| {
                    let scroll = &ws.read(cx).sidebar.scroll;
                    let mut b = scroll.bounds_for_item(docs_row).expect("laid out");
                    b.origin.y += scroll.offset().y;
                    b
                });
                *seen.borrow_mut() = TabDragGeometry {
                    tabs,
                    right,
                    chip: to - grab,
                    row,
                };
                acting.borrow_mut().take();
            };
            let path = shoot_with(scene, slug, &theme, wide, build.clone(), act.clone());
            println!("ui-shots: {}", path.display());
            let shot = render_with(&theme, wide, build, act);
            let n = format!("{scene}-{slug}");
            let px_at = |x: f32, y: f32| pixel_at(&shot, wide, x, y);
            let g = geometry.borrow().clone();
            let tabs = &g.tabs;
            // The tabs' top (`TAB` tall in the bars' middle, M11al): the probes 3 pt under it,
            // above the tiles and the words.
            let top = f32::from(tabs[0].top());
            let width = f32::from(tabs[0].size.width);
            let (chip_x, chip_y) = (f32::from(g.chip.x), f32::from(g.chip.y));
            // No insertion line: no accent across the left bar's tabs.
            let no_line = || {
                for x in (f32::from(tabs[0].left()) as i32..f32::from(tabs[3].right()) as i32)
                    .map(|x| x as f32 + 0.5)
                {
                    assert!(!is(px_at(x, top + 3.0), ui.accent), "{n}: no line at {x}");
                }
            };
            // The end of the left bar the taken tab's place left (`gone` wide, ending where its
            // last tab's place ended) with nothing drawn in it: the bar, which has no fill (the
            // card, M11al). (Tabs are as wide as their titles since 2026-11-20, so it is the
            // taken tab's width, not the last place.)
            let empty = |last: gpui::Bounds<Pixels>, gone: f32, what: &str| {
                let (from, to) = (
                    f32::from(last.right()) - gone + 6.0,
                    f32::from(last.right()) - 6.0,
                );
                let mut x = from;
                while x <= to {
                    for y in [
                        top + 5.0,
                        top + crate::tab_bar::TAB / 2.0,
                        top + crate::tab_bar::TAB - 5.0,
                    ] {
                        let p = px_at(x, y);
                        assert!(
                            near_by(p, ui.card, 3),
                            "{n}: {what} empty at {x},{y}: {p:?}"
                        );
                    }
                    x += 10.0;
                }
            };
            match scene {
                "tab-drag-gap" => {
                    // claude (tinted) slid left into api's place; the gap is where claude was.
                    let claude = px_at(f32::from(tabs[0].center().x), top + 3.0);
                    assert!(
                        near_by(claude, ui.waiting_tint, 3),
                        "{n}: claude slid left: {claude:?}"
                    );
                    // (Its right end: the chip covers the rest.)
                    let gap = px_at(f32::from(tabs[1].right()) - 4.0, top + 3.0);
                    assert!(near_by(gap, ui.card, 3), "{n}: the gap: {gap:?}");
                    // The chip is where api was taken: its left edge at the pointer less the
                    // grab, level with the tabs (not under them).
                    assert_eq!(chip_y, top, "{n}: the chip's top is the tabs'");
                    let inside = px_at(chip_x + 3.0, top + 10.0);
                    assert!(
                        is(inside, ui.panel),
                        "{n}: the chip's left edge: {inside:?}"
                    );
                    let outside = px_at(chip_x - 3.0, top + 10.0);
                    assert!(!is(outside, ui.panel), "{n}: left of the chip: {outside:?}");
                    let low = px_at(chip_x + 6.0, top + 23.0);
                    assert!(is(low, ui.panel), "{n}: the chip down the bar: {low:?}");
                    // (Its left edge half api's width left of that: tabs are as wide as their
                    // titles since 2026-11-20.)
                    let want = f32::from(tabs[1].center().x) - 10.0 - width / 2.0;
                    assert!(
                        (chip_x - want).abs() <= 1.0,
                        "{n}: the chip's centre 10 px short of claude's middle ({chip_x}, {want})"
                    );
                    no_line();
                }
                "tab-drag-other" => {
                    // Its own bar closed up: claude in api's place, the last place empty.
                    let claude = px_at(f32::from(tabs[0].center().x), top + 3.0);
                    assert!(
                        near_by(claude, ui.waiting_tint, 3),
                        "{n}: claude slid left: {claude:?}"
                    );
                    empty(tabs[3], width, "the left bar's end");
                    // The right bar: tests (shown in the unfocused pane: `card_lift` among pills,
                    // M11al) slid right by api's place; the gap's end shows the bar (the card).
                    let left = f32::from(g.right.left());
                    let tests = px_at(left + width + 4.0, top + crate::tab_bar::TAB / 2.0);
                    assert!(is(tests, ui.card_lift), "{n}: tests slid right: {tests:?}");
                    let gap = px_at(left + width - 6.0, top + 3.0);
                    assert!(near_by(gap, ui.card, 3), "{n}: the gap: {gap:?}");
                    no_line();
                }
                _ => {
                    // The ring along docs's row's left edge, mid-height (M11al: the row is its
                    // place's full width, no group card around it).
                    let ring = crate::ui_tokens::size::DIVIDER_ACTIVE / 2.0;
                    let (x, y) = (f32::from(g.row.left()) + ring, f32::from(g.row.center().y));
                    assert!(is(px_at(x, y), ui.accent), "{n}: docs's row ringed");
                    // Its own bar closed up: codex's width at its end is empty.
                    empty(tabs[3], f32::from(tabs[2].size.width), "the left bar's end");
                    no_line();
                }
            }
        }

        // Dropping (not shot).
        let slot: Slot = Rc::new(RefCell::new(None));
        let keep = slot.clone();
        let build = move |window: &mut Window, cx: &mut App| -> AnyView {
            cx.set_global(shot_appearance(theme, cx));
            let ws = cx.new(|cx| tab_drag_workspace(window, cx));
            *keep.borrow_mut() = Some(ws.clone());
            ws.into()
        };
        let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
            let ws = slotted(&slot);
            let n = format!("tab-drag-act-{slug}");
            // Each tab's name in `pane`'s bar (a browser tab: "browser").
            let names = |cx: &mut VisualTestAppContext, pane: PaneId| -> Vec<String> {
                cx.read(|cx| {
                    let ws = ws.read(cx);
                    let Some(p) = ws.model.pane(pane) else {
                        return Vec::new();
                    };
                    p.tabs
                        .iter()
                        .map(|t| match t.kind {
                            TabKind::Terminal(id) => {
                                let i = ws.index_of(id).expect("a session");
                                ws.sessions.items()[i].label().to_string()
                            }
                            TabKind::Browser { .. } => "browser".to_string(),
                            TabKind::File { .. } => "file".to_string(),
                            TabKind::Agent { .. } => "agent".to_string(),
                        })
                        .collect()
                })
            };
            // A terminal's id, view, screen, status and process, by its name.
            let facts = |cx: &mut VisualTestAppContext, name: &str| {
                cx.read(|cx| {
                    let ws = ws.read(cx);
                    let s = ws.sessions.items().iter().find(|s| s.label() == name);
                    let s = s.expect("a session");
                    let view = s.view.read(cx);
                    let pid = view.working_folder().1.expect("a local terminal");
                    (
                        s.id,
                        s.view.entity_id(),
                        view.text(TextScope::All),
                        s.status,
                        pid,
                    )
                })
            };
            let running = |pid: u32| {
                std::process::Command::new("/bin/kill")
                    .args(["-0", &pid.to_string()])
                    .status()
                    .is_ok_and(|s| s.success())
            };
            // The focused tab's name, and whether its view has GPUI's keyboard.
            let focused = |cx: &mut VisualTestAppContext| -> (String, bool) {
                cx.update_window(window, |_, window, cx| {
                    let ws = ws.read(cx);
                    let tab = ws.model.focused_tab().expect("a tab");
                    match ws.model.tab(tab).expect("known").kind {
                        TabKind::Terminal(id) => {
                            let s = &ws.sessions.items()[ws.index_of(id).expect("a session")];
                            let keys = s.view.focus_handle(cx).is_focused(window);
                            (s.label().to_string(), keys)
                        }
                        TabKind::Browser { .. } => {
                            let view = ws.browser_tabs.get(&tab).expect("its view");
                            let keys =
                                gpui::Focusable::focus_handle(view.read(cx), cx).is_focused(window);
                            ("browser".to_string(), keys)
                        }
                        TabKind::File { .. } => {
                            let view = ws.file_tabs.get(&tab).expect("its view");
                            let keys =
                                gpui::Focusable::focus_handle(view.read(cx), cx).is_focused(window);
                            ("file".to_string(), keys)
                        }
                        TabKind::Agent { .. } => {
                            let view = &ws.agents.get(&tab).expect("its view").view;
                            let keys =
                                gpui::Focusable::focus_handle(view.read(cx), cx).is_focused(window);
                            ("agent".to_string(), keys)
                        }
                    }
                })
                .expect("the window")
            };
            let release = |cx: &mut VisualTestAppContext, at: gpui::Point<Pixels>| {
                cx.simulate_mouse_up(window, at, left, none);
                assert!(!cx.read(|cx| cx.has_active_drag()), "{n}: the drag ended");
            };
            let panes = shown_panes(cx, &ws);
            let (lp, rp) = (panes[0], panes[1]);
            let tab = |cx: &mut VisualTestAppContext, pane: PaneId, i: usize| {
                tab_bounds(cx, &ws, pane, i).expect("laid out")
            };
            let before = facts(cx, "codex");
            assert!(running(before.4), "{n}: codex runs");

            // Esc mid-drag: nothing moves, the drag is over.
            let to = tab(cx, lp, 1).center();
            let from = tab(cx, lp, 2).center();
            drag(cx, window, from, to);
            cx.simulate_keystrokes(window, "escape");
            assert!(
                !cx.read(|cx| cx.has_active_drag()),
                "{n}: Esc ends the drag"
            );
            assert_eq!(
                cx.read(|cx| ws.read(cx).tab_drop),
                None,
                "{n}: nowhere to land"
            );
            cx.simulate_mouse_up(window, to, left, none);
            assert_eq!(
                names(cx, lp),
                ["api", "claude", "codex", "browser"],
                "{n}: Esc"
            );

            // Dropped on a terminal (not a bar or a row): nothing moves.
            let terminal = point(tab(cx, lp, 0).left() + px(200.0), px(300.0));
            let from = tab(cx, lp, 2).center();
            drag(cx, window, from, terminal);
            release(cx, terminal);
            assert_eq!(
                names(cx, lp),
                ["api", "claude", "codex", "browser"],
                "{n}: dropped elsewhere"
            );

            // A shaky click: pressed, moved 3 px (past GPUI's 2 px drag threshold), released on
            // the tab itself. It is shown with the keyboard, as a click would be.
            let from = tab(cx, lp, 1).center();
            let shaken = from + point(px(3.0), px(0.0));
            cx.simulate_mouse_down(window, from, left, none);
            cx.simulate_mouse_move(window, shaken, left, none);
            assert!(
                cx.read(|cx| cx.has_active_drag()),
                "{n}: 3 px starts a drag"
            );
            release(cx, shaken);
            assert_eq!(names(cx, lp)[1], "claude", "{n}: claude stays put");
            assert_eq!(
                focused(cx),
                ("claude".to_string(), true),
                "{n}: shaky click"
            );
            // Shakier: 3 px to start the drag, then 10 more to the right before the release.
            // Less than half a tab from where it was: its own place, nothing moves.
            cx.simulate_mouse_down(window, from, left, none);
            cx.simulate_mouse_move(window, shaken, left, none);
            let jitter = shaken + point(px(10.0), px(0.0));
            cx.simulate_mouse_move(window, jitter, left, none);
            assert_eq!(
                cx.read(|cx| ws.read(cx).tab_drop),
                None,
                "{n}: jitter: own place"
            );
            release(cx, jitter);
            assert_eq!(
                names(cx, lp),
                ["api", "claude", "codex", "browser"],
                "{n}: a shaky drag right moves nothing"
            );
            assert_eq!(focused(cx).0, "claude", "{n}: shaky drag right");

            // The same on the sidebar: the first workspace's row (three lines) shaken 3 px down,
            // then 10 more: its own place, the order stays (and nothing is saved: no drop slot).
            let order = workspaces(cx, &ws);
            let from = row_point(cx, &ws, docs_row - 1, 0.0);
            cx.simulate_mouse_down(window, from, left, none);
            cx.simulate_mouse_move(window, from + point(px(0.0), px(3.0)), left, none);
            assert!(cx.read(|cx| cx.has_active_drag()), "{n}: a row's drag");
            let jitter = from + point(px(0.0), px(13.0));
            cx.simulate_mouse_move(window, jitter, left, none);
            let lands = cx.read(|cx| ws.read(cx).sidebar.drop_slot);
            assert_eq!(lands, None, "{n}: a shaky row: its own place");
            release(cx, jitter);
            assert_eq!(workspaces(cx, &ws), order, "{n}: a shaky row moves nothing");

            // A file dragged in from Finder after that: the row dragged last isn't taken for
            // the one dragged now (it would show empty).
            let file = gpui::ExternalPaths(vec![std::path::PathBuf::from("/tmp/notes.txt")].into());
            cx.simulate_event(
                window,
                gpui::FileDropEvent::Entered {
                    position: from,
                    paths: file,
                },
            );
            cx.run_until_parked();
            assert!(cx.read(|cx| cx.has_active_drag()), "{n}: a file's drag");
            assert_eq!(
                cx.read(|cx| ws.read(cx).dragged_row(cx)),
                None,
                "{n}: no row is dragged"
            );
            cx.simulate_event(window, gpui::FileDropEvent::Exited);
            cx.run_until_parked();
            assert!(!cx.read(|cx| cx.has_active_drag()), "{n}: the file left");

            // Dropped where it is (its own gaps): nothing moves, and it is shown, as a click.
            let own = point(tab(cx, lp, 2).center().x - px(30.0), px(13.0));
            let from = tab(cx, lp, 2).center();
            drag(cx, window, from, own);
            assert_eq!(
                cx.read(|cx| ws.read(cx).tab_drop),
                None,
                "{n}: its own place"
            );
            release(cx, own);
            assert_eq!(names(cx, lp)[2], "codex", "{n}: its own place");
            assert_eq!(focused(cx), ("codex".to_string(), true), "{n}: codex shown");

            // Reordered: codex between api and claude (its chip's centre on claude's, the gap
            // nearest it), shown, with the keyboard, the same terminal.
            let to = tab(cx, lp, 1).center();
            let from = tab(cx, lp, 2).center();
            drag(cx, window, from, to);
            release(cx, to);
            assert_eq!(
                names(cx, lp),
                ["api", "codex", "claude", "browser"],
                "{n}: reordered"
            );
            assert_eq!(
                focused(cx),
                ("codex".to_string(), true),
                "{n}: codex focused"
            );
            let after = facts(cx, "codex");
            assert_eq!(after, before, "{n}: the same terminal, screen and status");
            assert!(running(after.4), "{n}: still running");

            // claude into the right pane's bar, on its empty part: at the end.
            let filler = point(tab(cx, rp, 0).right() + px(60.0), px(13.0));
            let from = tab(cx, lp, 2).center();
            drag(cx, window, from, filler);
            release(cx, filler);
            assert_eq!(
                names(cx, rp),
                ["tests", "claude"],
                "{n}: into the other pane"
            );
            assert_eq!(names(cx, lp), ["api", "codex", "browser"], "{n}: left it");
            assert_eq!(
                focused(cx),
                ("claude".to_string(), true),
                "{n}: claude focused"
            );

            // The browser tab, shown (a click), into the right pane before tests: its page hides
            // for the drag (hide rule (c)) and its view goes with it.
            let page = cx.read(|cx| {
                let ws = ws.read(cx);
                let tab = ws.model.pane(lp).expect("left").tabs[2].id;
                (tab, ws.browser_tabs[&tab].entity_id())
            });
            let showing = |cx: &mut VisualTestAppContext| {
                cx.read(|cx| ws.read(cx).browser_tabs[&page.0].read(cx).showing.get())
            };
            let from = tab(cx, lp, 2).center();
            cx.simulate_click(window, from, none);
            assert_eq!(focused(cx).0, "browser", "{n}: a click still shows a tab");
            let shown = showing(cx);
            assert!(
                shown.tab_shown && !page_hidden(shown),
                "{n}: the page shows"
            );
            let to = point(tab(cx, rp, 0).left() + px(5.0), px(13.0));
            drag(cx, window, from, to);
            let mid = showing(cx);
            assert!(
                mid.dragging && page_hidden(mid),
                "{n}: the page hides mid-drag"
            );
            release(cx, to);
            cx.run_until_parked();
            assert!(!showing(cx).dragging, "{n}: not after it");
            assert_eq!(
                names(cx, rp),
                ["browser", "tests", "claude"],
                "{n}: the browser tab moved"
            );
            let kept = cx.read(|cx| ws.read(cx).browser_tabs.get(&page.0).map(|v| v.entity_id()));
            assert_eq!(kept, Some(page.1), "{n}: the same page view");
            assert_eq!(
                focused(cx),
                ("browser".to_string(), true),
                "{n}: browser focused"
            );

            // api, then codex, to the front of the right pane: the left pane, emptied, closes and
            // the right one fills the workspace.
            let to = point(tab(cx, rp, 0).left() + px(5.0), px(13.0));
            for _ in 0..2 {
                let at = tab(cx, lp, 0).center();
                drag(cx, window, at, to);
                release(cx, to);
            }
            assert_eq!(shown_panes(cx, &ws), [rp], "{n}: the emptied pane closed");
            assert_eq!(
                names(cx, rp),
                ["codex", "api", "browser", "tests", "claude"],
                "{n}: every tab in the one pane"
            );

            // Onto docs's row: at the end of docs's pane, shown, the view follows.
            let order = workspaces(cx, &ws);
            let docs: WorkspaceId = order[1];
            let docs_pane = cx.read(|cx| ws.read(cx).model.workspace(docs).expect("docs").focused);
            let at = tab(cx, rp, 0).center();
            let to = row_point(cx, &ws, docs_row, 0.0);
            drag(cx, window, at, to);
            release(cx, to);
            assert_eq!(names(cx, docs_pane), ["docs", "codex"], "{n}: into docs");
            let shown = cx.read(|cx| ws.read(cx).model.focused_workspace());
            assert_eq!(shown, Some(docs), "{n}: docs shown");
            assert_eq!(
                focused(cx),
                ("codex".to_string(), true),
                "{n}: codex focused"
            );
            let after = facts(cx, "codex");
            assert_eq!(after, before, "{n}: still the same terminal");
            assert!(running(after.4), "{n}: still running");
            let id = cx.read(|cx| {
                let ws = ws.read(cx);
                ws.sessions.position(|s| s.label() == "codex")
            });
            let list_place = cx.read(|cx| {
                let ws = ws.read(cx);
                ws.workspace_of(id.expect("codex"))
            });
            assert_eq!(list_place, Some(docs), "{n}: scripts see it in docs");

            // docs's pane: every tab to the first workspace's row, the last closes docs (no ask:
            // nothing ends).
            let first_row = docs_row - 1;
            for _ in 0..2 {
                let at = tab(cx, docs_pane, 0).center();
                let to = row_point(cx, &ws, first_row, 0.0);
                drag(cx, window, at, to);
                release(cx, to);
                let shown = cx.read(|cx| ws.read(cx).model.focused_workspace());
                assert_eq!(shown, Some(order[0]), "{n}: the first workspace shown");
                // Back to docs, to drag its next tab.
                if cx.read(|cx| ws.read(cx).model.workspace(docs).is_some()) {
                    let row = row_point(cx, &ws, docs_row, 0.0);
                    cx.simulate_click(window, row, none);
                }
            }
            assert_eq!(workspaces(cx, &ws), [order[0]], "{n}: docs closed");
            assert_eq!(
                names(cx, rp),
                ["api", "browser", "tests", "claude", "docs", "codex"],
                "{n}: each at the end of the first workspace's pane"
            );
            let all = cx.read(|cx| ws.read(cx).sessions.items().len());
            assert_eq!(all, 5, "{n}: every terminal still runs");
            assert!(running(facts(cx, "codex").4), "{n}: codex still runs");
            slot.borrow_mut().take();
        };
        render_with(&theme, wide, build, act);
        println!("ui-shots: tab-drag-act-{slug} ok");
    }
}

/// The pages the suggestions scene remembers.
fn shot_history() -> Vec<crate::browser_history::Visit> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let visit = |url: &str, title: &str, days: u64, visits: u32| crate::browser_history::Visit {
        url: url.into(),
        title: title.into(),
        last_visit: now - days * 24 * 60 * 60,
        visits,
    };
    vec![
        visit("https://docs.rs/gpui/latest/gpui/", "gpui - Rust", 1, 6),
        visit(
            "https://doc.rust-lang.org/book/",
            "The Rust Programming Language",
            2,
            3,
        ),
        visit(
            "https://www.docker.com/",
            "Docker: Accelerated Container Application Development",
            9,
            1,
        ),
        visit(
            "https://github.com/zed-industries/zed",
            "Zed: docs, issues and code",
            0,
            12,
        ),
        visit("http://localhost:3000/dashboard", "Acme dashboard", 0, 30),
    ]
}

/// The browser tab (M11b Task 4), in both themes. The page itself is a native view, never in a
/// shot: its place shows the theme's background.
/// - `browser-tab`: api's terminal, and beside it a browser tab, focused: its tab titled by the
///   page, under it the 34 px bar (back on, forward off, reload, the rounded address field with
///   the address).
/// - `browser-tab-title`: "Show address bar" off: the page's title in the field's place; the page
///   is loading, so reload is stop.
/// - `browser-tab-suggestions`: the field clicked and "doc" typed (keep-data on): the visited
///   pages whose host starts with it, then one whose title holds it, then "Search Google for
///   “doc”", the first chosen with ↓.
///
/// Panics unless the field is where the bar puts it and the suggestions count as an overlay
/// (the page would hide); and (not shot) unless ⌃Tab cycles from a browser tab, ⌘W closes it,
/// "New browser tab" makes one ready for typing, a page-only workspace is in ⌘P, and Pop Out
/// closes a tab with no page.
fn browser_tab_scenes() {
    use crate::browser_history::BrowserHistory;
    use crate::browser_tab::SuggestNext;
    use crate::layout_model::TabKind;
    use crate::workspace::{CloseSession, NextTab, OpenSwitcher, PrevTab};
    use gpui::{KeyBinding, Modifiers};
    use std::cell::RefCell;
    use std::rc::Rc;

    let dims = size(px(1100.0), px(560.0));
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        for scene in [
            "browser-tab",
            "browser-tab-title",
            "browser-tab-suggestions",
        ] {
            let typed_at: Rc<std::cell::Cell<Option<gpui::Bounds<Pixels>>>> = Rc::default();
            let typed = typed_at.clone();
            let bar_probe: Rc<std::cell::Cell<Option<(Pixels, Pixels)>>> = Rc::default();
            let bar_seen = bar_probe.clone();
            let slot: Slot = Rc::new(RefCell::new(None));
            let keep = slot.clone();
            let build = move |window: &mut Window, cx: &mut App| -> AnyView {
                let mut appearance = shot_appearance(theme, cx);
                appearance.browser_address_bar = scene != "browser-tab-title";
                appearance.browser_keep_data = scene == "browser-tab-suggestions";
                cx.set_global(appearance);
                BrowserHistory::set_for_shots(shot_history(), cx);
                let loading = scene == "browser-tab-title";
                let ws = cx.new(|cx| browser_workspace(loading, window, cx));
                *keep.borrow_mut() = Some(ws.clone());
                ws.into()
            };
            let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let ws = slotted(&slot);
                let n = format!("{scene}-{slug}");
                let field = cx.read(|cx| {
                    let ws = ws.read(cx);
                    let view = ws
                        .focused_browser_tab()
                        .expect("the browser tab is focused");
                    assert_eq!(ws.focused_terminal(), None, "{n}: no terminal focused");
                    assert!(ws.sessions.active().is_none(), "{n}: no active session");
                    view.read(cx).field_bounds()
                });
                if scene == "browser-tab-title" {
                    assert_eq!(field, None, "{n}: no address field");
                    slot.borrow_mut().take();
                    return;
                }
                let field = field.expect("the field is drawn");
                typed.set(Some(field));
                // Under the tab bar (M11i), centred in the 34 px bar; the card starts under the
                // `CARD_INSET` strip, inside its 1 pt edge.
                assert_eq!(f32::from(field.size.height), 24.0, "{n}: field height");
                let top = f32::from(field.top());
                let bar_bottom =
                    CARD_INSET + crate::ui_tokens::size::HAIRLINE + crate::ui_tokens::size::TAB_BAR;
                assert!(
                    (bar_bottom + 3.0..=bar_bottom + 6.0).contains(&top),
                    "{n}: field top {top}"
                );
                // Its bar (M11al §9) sits in the card: the card's fill, the card's hairline under it.
                bar_probe.set(Some((
                    field.left() - px(2.0),
                    px(bar_bottom + crate::browser_tab::BAR),
                )));
                if scene == "browser-tab-suggestions" {
                    cx.update(|cx| {
                        cx.bind_keys([KeyBinding::new("down", SuggestNext, Some("AddressField"))])
                    });
                    cx.simulate_click(window, field.center(), Modifiers::none());
                    cx.simulate_input(window, "doc");
                    cx.simulate_keystrokes(window, "down");
                    let open = cx.read(|cx| ws.read(cx).overlays(cx));
                    assert!(open.suggestions, "{n}: the suggestions are open");
                    assert!(open.any(), "{n}: an overlay (the page hides)");
                }
                slot.borrow_mut().take();
            };
            let path = shoot_with(scene, slug, &theme, dims, build.clone(), act.clone());
            println!("ui-shots: {}", path.display());
            if scene == "browser-tab" {
                let shot = render_with(&theme, dims, build.clone(), act.clone());
                let (x, bottom) = bar_seen.get().expect("the bar was probed");
                let (x, bottom) = (f32::from(x), f32::from(bottom));
                let ui = theme.ui();
                let p = pixel_at(&shot, dims, x, bottom - 17.0);
                assert!(
                    near_by(p, ui.card, 3),
                    "{scene}-{slug}: the bar is {p:?}, not the card {:?}",
                    ui.card
                );
                let p = pixel_at(&shot, dims, x, bottom - 0.5);
                assert!(
                    near_by(p, ui.card_border, 3),
                    "{scene}-{slug}: the bar's line is {p:?}, not {:?}",
                    ui.card_border
                );
            }
            if scene == "browser-tab-suggestions" {
                // What was typed is drawn in the theme's text colour (not the default black).
                let shot = render_with(&theme, dims, build, act);
                let field = typed_at.get().expect("the field was drawn");
                let text = theme.ui().text;
                let near = |p: [u8; 3]| {
                    p.iter()
                        .zip([text.r, text.g, text.b])
                        .all(|(a, b)| a.abs_diff(b) <= 60)
                };
                let y = f32::from(field.center().y);
                let x0 = f32::from(field.left());
                let seen = (0..30).any(|dx| {
                    (-4..=4).any(|dy| near(pixel_at(&shot, dims, x0 + dx as f32, y + dy as f32)))
                });
                assert!(
                    seen,
                    "{scene}-{slug}: the typed text is in the theme's text colour"
                );
            }
        }

        // Acting on a browser tab (not shot).
        let slot: Slot = Rc::new(RefCell::new(None));
        let keep = slot.clone();
        let build = move |window: &mut Window, cx: &mut App| -> AnyView {
            cx.set_global(shot_appearance(theme, cx));
            BrowserHistory::set_for_shots(Vec::new(), cx);
            let ws = cx.new(|cx| browser_workspace(false, window, cx));
            *keep.borrow_mut() = Some(ws.clone());
            ws.into()
        };
        let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
            let ws = slotted(&slot);
            let n = format!("browser-tab-act-{slug}");
            cx.update(|cx| {
                cx.bind_keys([
                    KeyBinding::new("ctrl-tab", NextTab, Some("BrowserTab")),
                    KeyBinding::new("ctrl-shift-tab", PrevTab, Some("BrowserTab")),
                    KeyBinding::new("cmd-w", CloseSession, None),
                    KeyBinding::new("cmd-p", OpenSwitcher, None),
                ])
            });
            let kinds = |cx: &mut VisualTestAppContext| {
                cx.read(|cx| {
                    let ws = ws.read(cx);
                    let pane = ws.model.focused_pane().expect("a pane");
                    let p = ws.model.pane(pane).expect("known");
                    p.tabs
                        .iter()
                        .map(|t| match t.kind {
                            TabKind::Terminal(_) => "terminal",
                            TabKind::Browser { .. } => "browser",
                            TabKind::File { .. } => "file",
                            TabKind::Agent { .. } => "agent",
                        })
                        .collect::<Vec<_>>()
                })
            };
            let focused_kind = |cx: &mut VisualTestAppContext| {
                cx.read(|cx| {
                    let ws = ws.read(cx);
                    match ws.model.tab(ws.model.focused_tab()?)?.kind {
                        TabKind::Terminal(_) => Some("terminal"),
                        TabKind::Browser { .. } => Some("browser"),
                        TabKind::File { .. } => Some("file"),
                        TabKind::Agent { .. } => Some("agent"),
                    }
                })
            };
            // "New browser tab" on the browser's pane: a second browser tab, its field typed in.
            let pane = cx.read(|cx| ws.read(cx).model.focused_pane().expect("a pane"));
            cx.update_window(window, |_, window, cx| {
                ws.update(cx, |ws, cx| ws.new_browser_tab(pane, window, cx))
            })
            .expect("new browser tab");
            cx.run_until_parked();
            assert_eq!(kinds(cx), ["browser", "browser"], "{n}: a new browser tab");
            let editing = cx.read(|cx| ws.read(cx).overlays(cx));
            assert!(
                !editing.suggestions,
                "{n}: nothing typed yet, no suggestions"
            );
            // ⌃Tab from a browser tab (its own key context) goes back to the first; ⌘W closes it.
            cx.update_window(window, |_, window, cx| {
                ws.update(cx, |ws, cx| ws.focus_active(window, cx))
            })
            .expect("focus");
            cx.simulate_keystrokes(window, "ctrl-tab");
            let first = cx.read(|cx| ws.read(cx).model.pane(pane).map(|p| p.tabs[0].id));
            assert_eq!(
                cx.read(|cx| ws.read(cx).model.focused_tab()),
                first,
                "{n}: ⌃Tab"
            );
            cx.simulate_keystrokes(window, "cmd-w");
            assert_eq!(kinds(cx), ["browser"], "{n}: ⌘W closed the browser tab");
            assert_eq!(
                focused_kind(cx),
                Some("browser"),
                "{n}: the other one shows"
            );
            // A workspace of browser tabs only is in ⌘P, numbered in its place.
            cx.update_window(window, |_, window, cx| {
                ws.update(cx, |ws, cx| {
                    ws.model.new_workspace(1, TabKind::Browser { url: None });
                    ws.sync();
                    ws.activate(window, cx);
                })
            })
            .expect("a page-only workspace");
            cx.run_until_parked();
            cx.simulate_keystrokes(window, "cmd-p");
            let rows = cx.read(|cx| {
                let ws = ws.read(cx);
                ws.palette
                    .as_ref()
                    .map(|p| p.read(cx).shown_workspaces().count())
            });
            assert_eq!(rows, Some(2), "{n}: both workspaces in ⌘P");
            // An agent closes its browser tab (api's, not the one shown) while ⌘P is open: the
            // person's ⌘P stays open, with the keyboard.
            cx.update_window(window, |_, window, cx| {
                ws.update(cx, |ws, cx| {
                    let api = ws.model.workspaces()[0].id;
                    let tab = ws
                        .model
                        .tabs_of(api)
                        .into_iter()
                        .find(|t| ws.browser_tabs.contains_key(t))
                        .expect("api's browser tab");
                    ws.close_browser_tab(tab, false, window, cx);
                })
            })
            .expect("an agent's close");
            cx.run_until_parked();
            let still = cx
                .update_window(window, |_, window, cx| {
                    let palette = ws.read(cx).palette.clone();
                    palette
                        .map(|p| gpui::Focusable::focus_handle(p.read(cx), cx).is_focused(window))
                })
                .expect("read");
            assert_eq!(
                still,
                Some(true),
                "{n}: an agent's close leaves ⌘P open, with the keyboard"
            );
            // Pop Out of a tab with no page (off-screen): the tab just closes.
            cx.update_window(window, |_, window, cx| {
                ws.update(cx, |ws, cx| {
                    ws.close_overlays(crate::workspace::Overlay::Palette);
                    ws.palette = None;
                    let tab = ws.model.focused_tab().expect("a tab");
                    ws.pop_out(tab, window, cx);
                })
            })
            .expect("pop out");
            let left = cx.read(|cx| ws.read(cx).model.workspaces().len());
            assert_eq!(left, 1, "{n}: the page-only workspace went with its tab");
            slot.borrow_mut().take();
        };
        render_with(&theme, dims, build, act);
        println!("ui-shots: browser-tab-act-{slug} ok");
    }
}

/// The minimal tabs (M11al §4, style E), from real workspaces in both themes:
/// - `tabs-minimal`: one pane of seven tabs: api (idle, shown), claude (waiting), codex
///   (working), tests (done), a Claude agent tab (idle), a browser tab and notes.md (a preview).
/// - `tabs-minimal-hover`: the pointer over codex.
/// - `tabs-minimal-overflow`: a split in a narrow window, the right pane with twelve tabs.
/// - `tabs-minimal-narrow`: two panes of about 215 pt; the right one's shown tab (working,
///   hovered) shrinks below `TAB_MIN_WIDTH` to fit its row, its tile, dot and × whole.
///
/// Panics unless the bar has no fill (the card shows above the tabs and beside the buttons) and
/// a `card_border` line along its bottom; the tabs are `TAB` tall in its middle; every tab has
/// its outline icon (no tile behind it, 2026-11-20) in its kind's colour (an agent's in its provider's, a
/// terminal's and a browser's in the accent, a file's in `text_dim`); the status dot follows the
/// title (the working arc, the waiting dot, Done's muted ✓ in `text_dim`, nothing when idle);
/// each tab is a pill (round ends, clear card after it, no separator; chore 2026-11-20); the
/// shown tab has the `tab_active` wash, the others `card_hover` (a waiting one its tint), the
/// shown one's title in `text` and the others' in `text_faint`; notes.md is a preview; a
/// hovered tab is `card_lift` with its ×; + is drawn right after the last tab; and,
/// overflowing, each tab keeps at least `TAB_MIN_WIDTH`, the row scrolls, and ＋ ⋯ ▥ stay
/// inside each pane, drawn.
fn tabs_minimal_scenes() {
    use crate::layout_model::{FileMode, TabKind};
    use crate::tab_bar::{
        AFTER, BUTTON, BUTTON_GAP, BUTTON_ICON, BUTTONS_PAD, CLOSE, PART_GAP, PLUS_GAP, STATUS_DOT,
        TAB, TAB_MIN_WIDTH, TAB_PAD, TILE,
    };
    use crate::ui_tokens::size::{CARD_INSET, HAIRLINE, SIDEBAR_DEFAULT_WIDTH, TAB_BAR};
    use crate::ui_tokens::{MARK_MIN, TEXT_MIN};
    use gpui::Modifiers;
    use std::cell::RefCell;
    use std::rc::Rc;

    // A tab's place in the row: the pill, then the clear gap after it.
    let after = AFTER;
    // The bar's right end: ⋯ and the right sidebar's, pinned (chore 2026-11-20); the room the
    // tabs can't use adds + (right after the row) to it.
    let pinned = 2.0 * BUTTONS_PAD + 2.0 * BUTTON + BUTTON_GAP;
    let buttons = PLUS_GAP + BUTTON + pinned;
    let bar_top = CARD_INSET + HAIRLINE;
    let cy = bar_top + TAB_BAR / 2.0;
    let dir =
        std::env::temp_dir().join(format!("fuxx-ui-shots-tabs-minimal-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("the scene's folder");
    let notes = dir.join("notes.md");
    std::fs::write(&notes, "# Notes\n\n- one\n- two\n").expect("notes.md");
    let rgb = |v: u32| alacritty_terminal::vte::ansi::Rgb {
        r: (v >> 16) as u8,
        g: (v >> 8) as u8,
        b: v as u8,
    };
    let claude_rgb = crate::workspace_agent::provider_look("claude").2;

    let wide = size(px(1280.0), px(600.0));
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let ui = theme.ui();
        for scene in ["tabs-minimal", "tabs-minimal-hover"] {
            let slot: Slot = Rc::new(RefCell::new(None));
            let keep = slot.clone();
            let notes = notes.clone();
            let build = move |window: &mut Window, cx: &mut App| -> AnyView {
                cx.set_global(shot_appearance(theme, cx));
                let ws = cx.new(|cx| {
                    let mut ws = Workspace::for_shots(window, cx);
                    ws.sidebar.hint_dismissed = true;
                    let project = std::path::Path::new(API);
                    let tabs = [
                        shot("api", API, Status::Idle, 0, SHELL_TEXT),
                        shot("claude", API, Status::Waiting, 4 * 60 + 12, CLAUDE_PROMPT),
                        shot("codex", API, Status::Working, 32, AGENT_TEXT),
                        shot("tests", API, Status::Done, 720, ""),
                    ];
                    ws.open_shot_session("api", project, SHELL_TEXT, None, window, cx);
                    for s in &tabs[1..] {
                        ws.open_shot_tab(s.name, project, s.text, window, cx);
                    }
                    for s in &tabs {
                        let i = ws.sessions.position(|x| x.label() == s.name).expect("open");
                        ws.set_status(i, s.status, false, cx);
                        let session = ws.sessions.get_mut(i).expect("open");
                        session.pop = None;
                    }
                    let pane = ws.model.focused_pane().expect("a pane");
                    ws.new_agent_tab(pane, "claude", window, cx);
                    ws.model.add_tab(pane, TabKind::Browser { url: None });
                    ws.sync();
                    ws.preview_file_tab(notes.clone(), FileMode::Plain, window, cx);
                    let api = ws.sessions.position(|x| x.label() == "api").expect("api");
                    ws.select(api, window, cx);
                    ws
                });
                *keep.borrow_mut() = Some(ws.clone());
                ws.into()
            };
            let places: Rc<RefCell<Vec<gpui::Bounds<Pixels>>>> = Rc::default();
            let seen = places.clone();
            let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                settle(cx, window);
                let ws = slotted(&slot);
                let n = format!("{scene}-{slug}");
                let panes = shown_panes(cx, &ws);
                assert_eq!(panes.len(), 1, "{n}: one pane");
                let tabs: Vec<_> = (0..7)
                    .map(|i| tab_bounds(cx, &ws, panes[0], i).expect("laid out"))
                    .collect();
                let preview = cx.read(|cx| {
                    let ws = ws.read(cx);
                    let p = ws.model.pane(panes[0]).expect("the pane");
                    p.tabs
                        .iter()
                        .map(|t| ws.model.is_preview(t.id))
                        .collect::<Vec<_>>()
                });
                assert_eq!(
                    preview,
                    [false, false, false, false, false, false, true],
                    "{n}"
                );
                if scene == "tabs-minimal-hover" {
                    cx.simulate_mouse_move(window, tabs[2].center(), None, Modifiers::none());
                }
                *seen.borrow_mut() = tabs;
                slot.borrow_mut().take();
            };
            let shot = render_with(&theme, wide, build, act);
            let path = save_shot(scene, slug, &shot);
            let n = format!("{scene}-{slug}");
            let at = |x: f32, y: f32| pixel_at(&shot, wide, x, y);
            let scale = shot.width as f32 / 1280.0;
            // The pixels in the box (`x0`..`x1`, `y0`..`y1` points).
            let in_box = |x0: f32, x1: f32, y0: f32, y1: f32| {
                let mut all = Vec::new();
                for dy in (y0 * scale) as usize..(y1 * scale) as usize {
                    for dx in (x0 * scale) as usize..(x1 * scale) as usize {
                        all.push(at(dx as f32 / scale, dy as f32 / scale));
                    }
                }
                all
            };
            let nearest = |all: &[[u8; 3]], c| all.iter().map(|&p| gap(p, c)).min().unwrap_or(255);
            let tabs = places.borrow().clone();
            let pane_right = 1280.0 - CARD_INSET - HAIRLINE;
            // The bar: no fill above the tabs and beside the buttons, and its bottom line.
            let beside = pane_right - pinned + BUTTONS_PAD / 2.0;
            for (what, x, y) in [
                ("above api", f32::from(tabs[0].center().x), bar_top + 3.0),
                ("above codex", f32::from(tabs[2].center().x), bar_top + 3.0),
                ("beside the buttons", beside, cy),
            ] {
                let p = at(x, y);
                assert!(near_by(p, ui.card, 3), "{n}: the card {what}: {p:?}");
            }
            for x in [beside, f32::from(tabs[2].center().x)] {
                let line = at(x, bar_top + TAB_BAR - 0.5);
                assert!(
                    near(line, ui.card_border),
                    "{n}: the bar's line at {x}: {line:?}"
                );
                let under = at(x, bar_top + TAB_BAR + 1.5);
                assert!(
                    near(under, theme.background),
                    "{n}: the terminal under it: {under:?}"
                );
            }
            let (kinds, statuses) = (
                [
                    "api", "claude", "codex", "tests", "agent", "browser", "file",
                ],
                [
                    None,
                    Some(ui.ring_waiting),
                    Some(ui.ring_working),
                    Some(ui.text_dim),
                    None,
                    None,
                    None,
                ],
            );
            // Pills (chore 2026-11-20): the others `card_hover`, `card_lift` under the pointer.
            let fills = [
                Some(ui.tab_active),
                Some(ui.waiting_tint),
                Some(if scene == "tabs-minimal-hover" {
                    ui.card_lift
                } else {
                    ui.card_hover
                }),
                Some(ui.card_hover),
                Some(ui.card_hover),
                Some(ui.card_hover),
                Some(ui.card_hover),
            ];
            let glyphs = [
                ui.accent,
                ui.accent,
                ui.accent,
                ui.accent,
                crate::theme::readable(rgb(claude_rgb), &[ui.card_lift], MARK_MIN),
                ui.accent,
                ui.text_dim,
            ];
            for (i, place) in tabs.iter().enumerate() {
                let what = format!("{n}: {}", kinds[i]);
                let (left, right) = (f32::from(place.left()), f32::from(place.right()) - after);
                // `TAB` tall, in the bar's middle; at least the narrowest.
                assert_eq!(
                    f32::from(place.top()),
                    bar_top + (TAB_BAR - TAB) / 2.0,
                    "{what}"
                );
                assert_eq!(f32::from(place.size.height), TAB, "{what}");
                assert!(
                    right - left >= TAB_MIN_WIDTH - 0.5,
                    "{what}: {}",
                    right - left
                );
                // Its fill, away from its parts: in its right padding.
                let fill = fills[i].expect("a fill");
                let p = at(right - TAB_PAD / 2.0, cy);
                assert!(near(p, fill), "{what}: its fill {p:?}, want {fill:?}");
                // Its icon: an outline of the set on the pill itself, no tile behind it
                // (2026-11-20); its ink in its kind's colour.
                let tile = left + TAB_PAD;
                let corner_y = cy - TILE / 2.0 + 0.5;
                let corner = at(tile + 0.5, corner_y);
                // The pill's fill at the same height.
                let fill_there = at(right - TAB_PAD / 2.0, corner_y);
                assert!(
                    corner
                        .iter()
                        .zip(fill_there)
                        .all(|(a, b)| a.abs_diff(b) <= 3),
                    "{what}: no tile behind the icon: {corner:?} beside {fill_there:?}"
                );
                let inside = in_box(tile, tile + TILE, cy - TILE / 2.0, cy + TILE / 2.0);
                let ink = nearest(&inside, glyphs[i]);
                assert!(
                    ink <= 30,
                    "{what}: the glyph's colour {:?} off by {ink}",
                    glyphs[i]
                );
                if i == 6 {
                    assert!(
                        nearest(&inside, ui.accent) > 12,
                        "{what}: no accent on a file"
                    );
                }
                // The title: the shown tab's in `text`, the others' in `text_faint`.
                let title = in_box(
                    tile + TILE + PART_GAP,
                    tile + TILE + PART_GAP + 24.0,
                    cy - 6.0,
                    cy + 6.0,
                );
                let words = if i == 0 { ui.text } else { ui.text_faint };
                let off = nearest(&title, words);
                assert!(off <= 24, "{what}: its title {words:?} off by {off}");
                // The status dot after the title, before the ×'s room.
                let close = right - TAB_PAD - CLOSE;
                let dot = in_box(
                    close - PART_GAP - STATUS_DOT - 1.0,
                    close - PART_GAP + 1.0,
                    cy - STATUS_DOT / 2.0 - 1.0,
                    cy + STATUS_DOT / 2.0 + 1.0,
                );
                match statuses[i] {
                    Some(mark) => {
                        let off = nearest(&dot, mark);
                        assert!(off <= 30, "{what}: its dot {mark:?} off by {off}");
                        if i == 3 {
                            let green = nearest(&dot, ui.ring_done);
                            assert!(green > 30, "{what}: Done is muted, not {:?}", ui.ring_done);
                        }
                    }
                    // Idle: no dot. (api's short title leaves the box empty; a longer idle
                    // title runs on into it, so there only no status colour is checked.)
                    None if i == 0 => {
                        let off = dot.iter().map(|&p| gap(p, fill)).max().unwrap_or(0);
                        assert!(off <= 3, "{what}: no dot, but {off} off its fill");
                    }
                    None => {
                        for mark in [ui.ring_working, ui.ring_waiting] {
                            let off = nearest(&dot, mark);
                            assert!(off > 30, "{what}: no dot, but {mark:?} ({off} off)");
                        }
                    }
                }
                // × only on the hovered tab.
                let x_box = in_box(close + 2.0, close + CLOSE - 2.0, cy - 5.0, cy + 5.0);
                let drawn = x_box.iter().any(|&p| gap(p, fill) > 30);
                let hovered = scene == "tabs-minimal-hover" && i == 2;
                assert_eq!(drawn, hovered, "{what}: its ×");
                // A pill: its round ends leave the card at its corners; clear card after it (no
                // separator line).
                let corner_y = f32::from(place.top()) + 1.0;
                let corner = at(left + 1.0, corner_y);
                assert!(
                    near_by(corner, ui.card, 3),
                    "{what}: a round end: {corner:?}"
                );
                let room = at(right + after / 2.0, cy);
                assert!(
                    near_by(room, ui.card, 3),
                    "{what}: clear after it: {room:?}"
                );
            }
            // + right after the last tab: drawn there, the card between them.
            let last_right = f32::from(tabs[6].right());
            let plus = last_right + PLUS_GAP + BUTTON / 2.0;
            let half = BUTTON_ICON / 2.0;
            let inked = in_box(plus - half, plus + half, cy - half, cy + half)
                .iter()
                .any(|&p| gap(p, ui.card) > 40);
            assert!(inked, "{n}: + drawn after the last tab, at {plus}");
            let between = at(plus + BUTTON, cy);
            assert!(
                near_by(between, ui.card, 3),
                "{n}: nothing just past +: {between:?}"
            );
            // The tile glyphs and dots read (their floors, as `drawn_pairs` checks).
            assert!(
                contrast(glyphs[4], ui.card_lift) >= MARK_MIN,
                "{n}: the agent's glyph"
            );
            assert!(contrast(ui.text_faint, ui.waiting_tint) >= TEXT_MIN, "{n}");
            println!("ui-shots: {}", path.display());
        }

        // A narrow pane (fix round 1): two panes in a 680 pt window, each about 215 pt, the
        // row narrower than one tab's place. The right one has t1 (idle) and codex (working,
        // shown, hovered): codex shrinks below `TAB_MIN_WIDTH` to fit the row, its tile, its
        // status dot and its × whole and drawn inside the row, left of the buttons; t1 keeps
        // `TAB_MIN_WIDTH` and is scrolled away.
        // 680 pt over the old 240 pt sidebar: wider by what the default sidebar grew, so the panes
        // stay about 215 pt each; 20 narrower since the bar's buttons shrank (2026-11-20), so the
        // row stays narrower than one tab's place.
        let small_w = 660.0 + (SIDEBAR_DEFAULT_WIDTH - 240.0);
        let small = size(px(small_w), px(420.0));
        let slot: Slot = Rc::new(RefCell::new(None));
        let keep = slot.clone();
        let build = move |window: &mut Window, cx: &mut App| -> AnyView {
            cx.set_global(shot_appearance(theme, cx));
            let ws = cx.new(|cx| {
                let mut ws = Workspace::for_shots(window, cx);
                ws.sidebar.hint_dismissed = true;
                let project = std::path::Path::new(API);
                ws.open_shot_session("api", project, SHELL_TEXT, None, window, cx);
                ws.open_shot_session("t1", project, SHELL_TEXT, Some(Axis::Row), window, cx);
                ws.open_shot_tab("codex", project, AGENT_TEXT, window, cx);
                let i = ws
                    .sessions
                    .position(|x| x.label() == "codex")
                    .expect("codex");
                ws.set_status(i, Status::Working, false, cx);
                ws
            });
            *keep.borrow_mut() = Some(ws.clone());
            ws.into()
        };
        let places: Rc<RefCell<Vec<gpui::Bounds<Pixels>>>> = Rc::default();
        let seen = places.clone();
        let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
            // A second frame: the first laid the tabs out, this one scrolls to the shown one.
            cx.update_window(window, |_, window, _| window.refresh())
                .expect("refresh");
            cx.run_until_parked();
            let ws = slotted(&slot);
            let n = format!("tabs-minimal-narrow-{slug}");
            let panes = shown_panes(cx, &ws);
            assert_eq!(panes.len(), 2, "{n}: two panes");
            let tabs: Vec<_> = (0..2)
                .map(|i| tab_bounds(cx, &ws, panes[1], i).expect("laid out"))
                .collect();
            cx.simulate_mouse_move(window, tabs[1].center(), None, Modifiers::none());
            *seen.borrow_mut() = tabs;
            slot.borrow_mut().take();
        };
        let shot = render_with(&theme, small, build, act);
        let path = save_shot("tabs-minimal-narrow", slug, &shot);
        let n = format!("tabs-minimal-narrow-{slug}");
        let at = |x: f32, y: f32| pixel_at(&shot, small, x, y);
        let scale = shot.width as f32 / small_w;
        let in_box = |x0: f32, x1: f32, y0: f32, y1: f32| {
            let mut all = Vec::new();
            for dy in (y0 * scale) as usize..(y1 * scale) as usize {
                for dx in (x0 * scale) as usize..(x1 * scale) as usize {
                    all.push(at(dx as f32 / scale, dy as f32 / scale));
                }
            }
            all
        };
        let tabs = places.borrow().clone();
        let (left, right) = (
            SIDEBAR_DEFAULT_WIDTH + HAIRLINE,
            small_w - CARD_INSET - HAIRLINE,
        );
        let across = left + 0.5 * (right - left - HAIRLINE) + HAIRLINE / 2.0;
        let (row_left, row_right) = (
            across + HAIRLINE / 2.0 + crate::ui_tokens::size::TABS_LEFT,
            right - buttons,
        );
        assert!(
            row_right - row_left < TAB_MIN_WIDTH + after,
            "{n}: the row ({row_left}–{row_right}) is narrower than one tab's place"
        );
        // t1 keeps the narrowest width (and is scrolled away).
        let t1 = f32::from(tabs[0].size.width) - after;
        assert!(t1 >= TAB_MIN_WIDTH - 0.5, "{n}: t1 {t1} wide");
        // codex, the shown tab, fits the row: whole, left of the buttons.
        let (codex_left, codex_right) = (
            f32::from(tabs[1].left()),
            f32::from(tabs[1].right()) - after,
        );
        assert!(
            codex_left >= row_left - 0.5 && codex_right <= row_right + 0.5,
            "{n}: the shown tab {codex_left}–{codex_right} inside the row {row_left}–{row_right}"
        );
        // Its icon, its working dot and its × (hovered), drawn inside it.
        let tile = codex_left + TAB_PAD;
        let ink = in_box(tile, tile + TILE, cy - TILE / 2.0, cy + TILE / 2.0)
            .iter()
            .map(|&p| gap(p, ui.accent))
            .min()
            .unwrap_or(255);
        assert!(ink <= 30, "{n}: its icon drawn ({ink} off the accent)");
        let close = codex_right - TAB_PAD - CLOSE;
        let dot = in_box(
            tile + TILE,
            close,
            cy - STATUS_DOT / 2.0 - 1.0,
            cy + STATUS_DOT / 2.0 + 1.0,
        );
        let off = dot
            .iter()
            .map(|&p| gap(p, ui.ring_working))
            .min()
            .unwrap_or(255);
        assert!(off <= 30, "{n}: the working dot is drawn ({off} off)");
        let fill = ui.tab_active;
        let x_box = in_box(close + 2.0, close + CLOSE - 2.0, cy - 5.0, cy + 5.0);
        assert!(
            x_box.iter().any(|&p| gap(p, fill) > 30),
            "{n}: its × drawn on hover"
        );
        // The buttons stay drawn, in the pane: ⋯ and the right sidebar's pinned, + left of them.
        for k in 0..3 {
            let centre = right - BUTTONS_PAD - BUTTON / 2.0 - k as f32 * (BUTTON + BUTTON_GAP)
                + if k == 2 {
                    BUTTON_GAP - BUTTONS_PAD
                } else {
                    0.0
                };
            let half = BUTTON_ICON / 2.0;
            let inked = in_box(centre - half, centre + half, cy - half, cy + half)
                .iter()
                .any(|&p| gap(p, ui.card) > 40);
            assert!(inked, "{n}: button {k} drawn at {centre}");
        }
        println!("ui-shots: {}", path.display());

        // Overflowing: twelve tabs in the right pane of a split, in a narrow window.
        let narrow = size(px(960.0), px(480.0));
        let slot: Slot = Rc::new(RefCell::new(None));
        let keep = slot.clone();
        let build = move |window: &mut Window, cx: &mut App| -> AnyView {
            cx.set_global(shot_appearance(theme, cx));
            let ws = cx.new(|cx| {
                let mut ws = Workspace::for_shots(window, cx);
                ws.sidebar.hint_dismissed = true;
                let project = std::path::Path::new(API);
                ws.open_shot_session("api", project, SHELL_TEXT, None, window, cx);
                ws.open_shot_session("t1", project, SHELL_TEXT, Some(Axis::Row), window, cx);
                for k in 2..=12 {
                    ws.open_shot_tab(&format!("t{k}"), project, SHELL_TEXT, window, cx);
                }
                ws
            });
            *keep.borrow_mut() = Some(ws.clone());
            ws.into()
        };
        let places: Rc<RefCell<Vec<gpui::Bounds<Pixels>>>> = Rc::default();
        let seen = places.clone();
        let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
            // A second frame: the first laid the tabs out, this one scrolls to the shown one.
            cx.update_window(window, |_, window, _| window.refresh())
                .expect("refresh");
            cx.run_until_parked();
            let ws = slotted(&slot);
            let n = format!("tabs-minimal-overflow-{slug}");
            let panes = shown_panes(cx, &ws);
            assert_eq!(panes.len(), 2, "{n}: two panes");
            let offset = cx.read(|cx| {
                let scrolls = ws.read(cx).tab_scroll.borrow();
                f32::from(scrolls[&panes[1]].0.offset().x)
            });
            assert!(offset < 0.0, "{n}: the row scrolled to the last tab");
            let tabs: Vec<_> = (0..12)
                .map(|i| tab_bounds(cx, &ws, panes[1], i).expect("laid out"))
                .collect();
            *seen.borrow_mut() = tabs;
            slot.borrow_mut().take();
        };
        let shot = render_with(&theme, narrow, build, act);
        let path = save_shot("tabs-minimal-overflow", slug, &shot);
        let n = format!("tabs-minimal-overflow-{slug}");
        let at = |x: f32, y: f32| pixel_at(&shot, narrow, x, y);
        let scale = shot.width as f32 / 960.0;
        let tabs = places.borrow().clone();
        // The panes' right edges: the hairline between them halfway across the card's inside.
        let (left, right) = (
            SIDEBAR_DEFAULT_WIDTH + HAIRLINE,
            960.0 - CARD_INSET - HAIRLINE,
        );
        let across = left + 0.5 * (right - left - HAIRLINE) + HAIRLINE / 2.0;
        for place in &tabs {
            let w = f32::from(place.size.width) - after;
            assert!(w >= TAB_MIN_WIDTH - 0.5, "{n}: a tab {w} wide");
        }
        let last = f32::from(tabs[11].right()) - after;
        assert!(
            last <= right - buttons + 0.5,
            "{n}: the shown tab {last} in view, left of the buttons"
        );
        for (pane, pane_right) in [("left", across - HAIRLINE / 2.0), ("right", right)] {
            // The left pane's one tab leaves its row short: its + is right after api, not here;
            // and it has ⋯ alone (the right sidebar's button is the top-right bar's).
            for k in 0..if pane == "left" { 1 } else { 3 } {
                // ⋯ and the right sidebar's pinned, + right after the (full) row, left of them.
                let centre =
                    pane_right - BUTTONS_PAD - BUTTON / 2.0 - k as f32 * (BUTTON + BUTTON_GAP)
                        + if k == 2 {
                            BUTTON_GAP - BUTTONS_PAD
                        } else {
                            0.0
                        };
                let half = BUTTON_ICON / 2.0;
                let mut inked = false;
                for dy in ((cy - half) * scale) as usize..((cy + half) * scale) as usize {
                    for dx in ((centre - half) * scale) as usize..((centre + half) * scale) as usize
                    {
                        inked |= gap(at(dx as f32 / scale, dy as f32 / scale), ui.card) > 40;
                    }
                }
                assert!(inked, "{n}: the {pane} pane's button {k} drawn at {centre}");
                assert!(
                    centre + BUTTON / 2.0 <= pane_right,
                    "{n}: inside the {pane} pane"
                );
            }
        }
        println!("ui-shots: {}", path.display());
    }
}
