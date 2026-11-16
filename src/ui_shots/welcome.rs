//! The Welcome card (M11k): its look, detection still running, its keys, a small window, what a
//! pick opens, the folder row, a first run and Help → Welcome to fuxx.

use super::*;

/// This file, as `file!()` gives it (repo-relative under cargo): `registry::AREAS` pairs it with
/// `SCENES`, so `--changed` runs these scenes when this file changes.
pub(super) const FILE: &str = file!();

/// This area's scenes (`registry.rs` runs them in its `ORDER`).
pub(super) const SCENES: &[Scene] = &[
    Scene {
        name: "welcome",
        run: welcome_scene,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/welcome.rs",
            "src/welcome_items.rs",
            "src/presets.rs",
            "src/agent_detect.rs",
            "src/overlay_panel.rs",
            "src/help_shortcuts.rs",
        ],
    },
    Scene {
        name: "welcome-checking",
        run: welcome_checking_scene,
        themes: Themes::One,
        // Serial: (c) nothing moves over 200 ms of real time.
        serial: true,
        kind: Kind::Visual,
        sources: &[
            "src/welcome.rs",
            "src/welcome_items.rs",
            "src/presets.rs",
            "src/agent_detect.rs",
            "src/reduce_motion.rs",
        ],
    },
    Scene {
        name: "welcome-keys",
        run: welcome_keys_scene,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: &[
            "src/welcome.rs",
            "src/welcome_items.rs",
            "src/presets.rs",
            "src/palette.rs",
            "src/tab_bar.rs",
            "src/layout_model.rs",
            "src/name_input.rs",
            "src/overlay_panel.rs",
        ],
    },
    Scene {
        name: "welcome-narrow",
        run: welcome_narrow_scene,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/welcome.rs",
            "src/welcome_items.rs",
            "src/presets.rs",
            "src/overlay_panel.rs",
        ],
    },
    Scene {
        name: "welcome-start",
        run: welcome_start_scene,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: &[
            "src/welcome.rs",
            "src/welcome_items.rs",
            "src/presets.rs",
            "src/layout_model.rs",
            "src/workspace_daemon.rs",
            "src/file_tab.rs",
            "src/workspace_agent.rs",
            "src/agent_tab.rs",
        ],
    },
    Scene {
        name: "welcome-folder",
        run: welcome_folder_scene,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: &["src/welcome.rs", "src/welcome_items.rs", "src/presets.rs"],
    },
    Scene {
        name: "welcome-first-run",
        run: welcome_first_run_scene,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: &["src/welcome.rs", "src/welcome_items.rs", "src/presets.rs"],
    },
    Scene {
        name: "welcome-reopen",
        run: welcome_reopen_scene,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: &["src/welcome.rs", "src/welcome_items.rs", "src/presets.rs"],
    },
];

/// The Welcome scenes' workspace: `presets`, the app's key bindings, and one terminal in
/// `/Users/me` (the made-up home) with nothing on its screen; its id goes to `over`.
fn welcome_build(
    theme: Theme,
    presets: Vec<crate::presets::Preset>,
    still: bool,
    keep: Slot,
    over: std::rc::Rc<std::cell::Cell<u64>>,
) -> impl FnOnce(&mut Window, &mut App) -> AnyView + Clone {
    move |window: &mut Window, cx: &mut App| -> AnyView {
        cx.set_reduce_motion(still);
        cx.set_global(Appearance {
            presets,
            ..shot_appearance(theme, cx)
        });
        cx.bind_keys(crate::key_bindings());
        let ws = cx.new(|cx| {
            let mut ws = Workspace::for_shots(window, cx);
            ws.sidebar.hint_dismissed = true;
            let home = std::path::Path::new("/Users/me");
            over.set(ws.open_unnamed_shot_session(Some(home), "", window, cx));
            ws
        });
        *keep.borrow_mut() = Some(ws.clone());
        ws.into()
    }
}

/// The open Welcome card; none while it's closed.
fn welcome_of(
    cx: &mut VisualTestAppContext,
    ws: &gpui::Entity<Workspace>,
) -> Option<gpui::Entity<crate::welcome::Welcome>> {
    cx.read(|cx| ws.read(cx).welcome.clone())
}

/// Open the card as the first run or Help does, then let it take the keyboard.
fn open_welcome_on(
    cx: &mut VisualTestAppContext,
    window: AnyWindowHandle,
    ws: &gpui::Entity<Workspace>,
    origin: crate::welcome_items::Origin,
) {
    cx.update_window(window, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.open_welcome(origin, window, cx))
    })
    .expect("open the card");
    settle(cx, window);
}

/// Give the open card detection's answer, as the background check would.
fn welcome_found(
    cx: &mut VisualTestAppContext,
    window: AnyWindowHandle,
    ws: &gpui::Entity<Workspace>,
    found: Vec<crate::agent_detect::Found>,
) {
    let card = welcome_of(cx, ws).expect("the card is open");
    cx.update(|cx| card.update(cx, |c, cx| c.set_found(found, cx)));
    settle(cx, window);
}

/// The open card's chosen row and the text it drew.
fn welcome_state(
    cx: &mut VisualTestAppContext,
    ws: &gpui::Entity<Workspace>,
) -> (crate::welcome_items::Row, Vec<String>) {
    let card = welcome_of(cx, ws).expect("the card is open");
    cx.read(|cx| {
        let c = card.read(cx);
        (c.chosen(), c.texts())
    })
}

/// The card's place in a shot from its inside bounds (an absolute child's, inside the 1 pt
/// edge): its outer edges.
fn outer_edges(b: gpui::Bounds<Pixels>) -> (f32, f32, f32, f32) {
    (
        f32::from(b.left()) - 1.0,
        f32::from(b.right()) + 1.0,
        f32::from(b.top()) - 1.0,
        f32::from(b.bottom()) + 1.0,
    )
}

/// The Welcome card (M11k Task 2) over one terminal in the made-up home, the built-in presets
/// (claude, opencode, codex), in both themes: `welcome`, opened as on a first run, detection
/// answered (claude and codex found, opencode not), then codex clicked.
///
/// Panics unless claude is chosen once the answer lands and codex after the click (the Start
/// button then says `Start codex`); the card is 520 pt wide and centred below the top band;
/// it drew `Welcome to fuxx`, `~`, `Not found on your PATH` once, `⌃⌘1`, the three shortcuts'
/// keys and `Start claude`; the chosen row is filled `row_active` and the Start button
/// `accent`; the scrim darkens the window left of the card and the top band too, down to its
/// foot (each against the same window with no card). With no presets, it says so and Plain terminal is chosen.
fn welcome_scene() {
    use crate::agent_detect::Found;
    use crate::welcome_items::{Origin, Row};
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    type Seen = (
        gpui::Bounds<Pixels>,
        gpui::Bounds<Pixels>,
        gpui::Bounds<Pixels>,
    );
    let dims = size(px(960.0), px(600.0));
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let ui = theme.ui();
        let n = format!("welcome-{slug}");
        let presets = crate::presets::builtins();

        // The same window without the card.
        let slot: Slot = Rc::new(RefCell::new(None));
        let over = Rc::new(Cell::new(0));
        let build = welcome_build(theme, presets.clone(), false, slot.clone(), over.clone());
        let bare = render_with(&theme, dims, build, move |cx, window| {
            settle(cx, window);
            slot.borrow_mut().take();
        });

        let slot: Slot = Rc::new(RefCell::new(None));
        let build = welcome_build(theme, presets.clone(), false, slot.clone(), over.clone());
        let seen: Rc<Cell<Option<Seen>>> = Rc::default();
        let found = seen.clone();
        let m = n.clone();
        let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
            let ws = slotted(&slot);
            open_welcome_on(cx, window, &ws, Origin::FirstRun { over: over.get() });
            welcome_found(
                cx,
                window,
                &ws,
                vec![Found::Yes, Found::Yes, Found::No, Found::Yes],
            );
            let (chosen, texts) = welcome_state(cx, &ws);
            assert_eq!(chosen, Row::Preset(0), "{m}: the first found is chosen");
            for want in [
                "Welcome to fuxx",
                "~",
                "\u{2303}\u{2318}1",
                "\u{2318}K",
                "\u{2318}N",
                "\u{21e7}\u{2318}U",
                "Start claude",
            ] {
                assert!(
                    texts.iter().any(|t| t == want),
                    "{m}: {want:?} drawn: {texts:?}"
                );
            }
            let missing = texts
                .iter()
                .filter(|t| *t == "Not found on your PATH")
                .count();
            assert_eq!(missing, 1, "{m}: opencode alone is missing: {texts:?}");

            let card = welcome_of(cx, &ws).expect("open");
            let codex = cx
                .read(|cx| card.read(cx).row_bounds(Row::Preset(3)))
                .expect("codex's row is drawn");
            cx.simulate_click(window, codex.center(), gpui::Modifiers::none());
            settle(cx, window);
            assert!(
                welcome_of(cx, &ws).is_some(),
                "{m}: a click on a row keeps the card open"
            );
            let (chosen, texts) = welcome_state(cx, &ws);
            assert_eq!(chosen, Row::Preset(3), "{m}: the click chose codex");
            assert!(
                texts.iter().any(|t| t == "Start codex"),
                "{m}: the button follows: {texts:?}"
            );
            found.set(cx.read(|cx| {
                let c = card.read(cx);
                Some((
                    c.card_bounds()?,
                    c.row_bounds(Row::Preset(3))?,
                    c.start_bounds()?,
                ))
            }));
            slot.borrow_mut().take();
        };
        let shot = render_with(&theme, dims, build, act);
        save_shot("welcome", slug, &shot);
        let (card, codex, start) = seen.get().expect("drawn");
        let (left, right, top, bottom) = outer_edges(card);
        assert!(
            (right - left - 520.0).abs() < 0.5,
            "{n}: the card is {} wide",
            right - left
        );
        let (cx_, cy) = ((left + right) / 2.0, (top + bottom) / 2.0);
        let band = ui_tokens::size::TOP_BAND;
        let below = band + (600.0 - band) / 2.0;
        assert!(
            (cx_ - 480.0).abs() <= 1.0 && (cy - below).abs() <= 1.0,
            "{n}: centred at ({cx_}, {cy}), not (480, {below})"
        );
        let fill = |b: gpui::Bounds<Pixels>, inset: f32| {
            pixel_at(
                &shot,
                dims,
                f32::from(b.left()) + inset,
                f32::from(b.center().y),
            )
        };
        assert!(
            near(fill(codex, 4.0), ui.row_active),
            "{n}: the chosen row is row_active"
        );
        assert!(
            near(fill(start, 3.0), ui.accent),
            "{n}: the Start button is accent"
        );
        let sum = |p: [u8; 3]| p.iter().map(|&c| c as u32).sum::<u32>();
        let (scrim, plain) = (
            pixel_at(&shot, dims, 100.0, 300.0),
            pixel_at(&bare, dims, 100.0, 300.0),
        );
        assert!(
            sum(scrim) + 6 <= sum(plain),
            "{n}: the scrim darkens ({scrim:?} vs {plain:?})"
        );
        // The top band is dimmed as much (painted only: its clicks still land, `welcome-keys`),
        // so no seam runs along its foot.
        for x in [100.0, 480.0, 900.0] {
            let (with, without) = (
                pixel_at(&shot, dims, x, band / 2.0),
                pixel_at(&bare, dims, x, band / 2.0),
            );
            // (Pure black, as a tab bar can now be, has nothing left to dim: it must not
            // brighten either.)
            if sum(without) <= 6 {
                assert!(
                    sum(with) <= sum(without),
                    "{n}: the black top band at {x} brightened ({with:?} vs {without:?})"
                );
            } else {
                assert!(
                    sum(with) + 6 <= sum(without),
                    "{n}: the top band at {x} is dimmed ({with:?} vs {without:?})"
                );
            }
        }
        let (above, below) = (
            pixel_at(&shot, dims, 480.0, band - 1.0),
            pixel_at(&bare, dims, 480.0, band - 1.0),
        );
        assert!(
            sum(above) + 6 <= sum(below),
            "{n}: dimmed down to the band's foot ({above:?} vs {below:?})"
        );

        // No presets: the note, and the plain terminal chosen.
        let slot: Slot = Rc::new(RefCell::new(None));
        let over = Rc::new(Cell::new(0));
        let build = welcome_build(theme, Vec::new(), false, slot.clone(), over.clone());
        let m = format!("{n}-none");
        render_with(&theme, dims, build, move |cx, window| {
            let ws = slotted(&slot);
            open_welcome_on(cx, window, &ws, Origin::FirstRun { over: over.get() });
            let (chosen, texts) = welcome_state(cx, &ws);
            assert_eq!(chosen, Row::Plain, "{m}: Plain terminal chosen");
            assert!(
                texts.iter().any(|t| t == "No agent presets set."),
                "{m}: the note: {texts:?}"
            );
            assert!(
                texts.iter().any(|t| t == "Open Terminal"),
                "{m}: the button: {texts:?}"
            );
            slot.borrow_mut().take();
        });
    }
}

/// The Welcome card while detection hasn't answered, in Mocha (both themes with
/// `--all-themes`): `welcome-checking`.
///
/// Panics unless every preset row says `Checking…` and claude is chosen; Enter starts claude
/// in the home folder (the card's `Start` event); two frames 200 ms apart are the same in the
/// card, with Reduce motion off and on (nothing in it moves); an answer of all missing after ↓
/// leaves the moved choice (opencode) alone, but on a fresh card chooses Plain terminal.
fn welcome_checking_scene() {
    use crate::agent_detect::Found;
    use crate::welcome::WelcomeEvent;
    use crate::welcome_items::{Choice, Origin, Row};
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;
    use std::time::Duration;

    let dims = size(px(960.0), px(600.0));
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        for still in [false, true] {
            let slot: Slot = Rc::new(RefCell::new(None));
            let over = Rc::new(Cell::new(0));
            let presets = crate::presets::builtins();
            let claude = presets[0].clone();
            let build = welcome_build(theme, presets, still, slot.clone(), over.clone());
            let n = format!("welcome-checking-{slug}-{still}");
            let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let ws = slotted(&slot);
                let first = Origin::FirstRun { over: over.get() };
                open_welcome_on(cx, window, &ws, first);
                let card = welcome_of(cx, &ws).expect("open");
                let found = cx.read(|cx| card.read(cx).found().to_vec());
                assert_eq!(found, [Found::Checking; 4], "{n}: nothing answered yet");
                let (chosen, texts) = welcome_state(cx, &ws);
                assert_eq!(chosen, Row::Preset(0), "{n}: claude chosen meanwhile");
                let checking = texts.iter().filter(|t| *t == "Checking\u{2026}").count();
                assert_eq!(checking, 4, "{n}: each row checking: {texts:?}");

                // Enter starts the chosen row while it's still checking.
                let heard: Rc<RefCell<Vec<WelcomeEvent>>> = Rc::default();
                let hear = heard.clone();
                cx.update(|cx| {
                    cx.subscribe(&card, move |_, event: &WelcomeEvent, _| {
                        hear.borrow_mut().push(event.clone())
                    })
                    .detach()
                });
                press_keys(cx, window, "enter");
                assert_eq!(
                    *heard.borrow(),
                    [WelcomeEvent::Start {
                        folder: "/Users/me".into(),
                        choice: Choice::Preset(claude.clone()),
                    }],
                    "{n}: Enter starts claude"
                );

                // An answer after a move leaves the move; on a fresh card it chooses.
                open_welcome_on(cx, window, &ws, first);
                press_keys(cx, window, "down");
                welcome_found(cx, window, &ws, vec![Found::No; 4]);
                assert_eq!(welcome_state(cx, &ws).0, Row::Preset(1), "{n}: kept moved");
                open_welcome_on(cx, window, &ws, first);
                welcome_found(cx, window, &ws, vec![Found::No; 4]);
                assert_eq!(welcome_state(cx, &ws).0, Row::Plain, "{n}: all missing");

                // Nothing in the card moves.
                open_welcome_on(cx, window, &ws, first);
                let card = welcome_of(cx, &ws).expect("open");
                let b = cx
                    .read(|cx| card.read(cx).card_bounds())
                    .expect("the card is drawn");
                let grab = |cx: &mut VisualTestAppContext| {
                    let image = cx.capture_screenshot(window).expect("capture");
                    let (w, raw) = (image.width() as usize, image.as_raw().to_vec());
                    let scale = w as f32 / f32::from(dims.width);
                    let (x0, x1) = (
                        (f32::from(b.left()) * scale) as usize,
                        (f32::from(b.right()) * scale) as usize,
                    );
                    let (y0, y1) = (
                        (f32::from(b.top()) * scale) as usize,
                        (f32::from(b.bottom()) * scale) as usize,
                    );
                    (y0..y1)
                        .flat_map(|y| raw[(y * w + x0) * 4..(y * w + x1) * 4].to_vec())
                        .collect::<Vec<u8>>()
                };
                let before = grab(cx);
                prof::sleep(Duration::from_millis(200));
                cx.advance_clock(Duration::from_millis(200));
                settle(cx, window);
                let after = grab(cx);
                assert!(before == after, "{n}: the card moved");
                slot.borrow_mut().take();
            };
            let shot = render_with(&theme, dims, build, act);
            if still {
                save_shot("welcome-checking", slug, &shot);
            }
        }
    }
}

/// The Welcome card's keys and what closes it, in Mocha (both themes with `--all-themes`):
/// `welcome-keys`.
///
/// Panics unless ↓ four times from claude wraps round to the folder row and ↑ then chooses
/// Plain terminal (the button saying `Open Terminal`); Esc closes the card and gives the
/// terminal the keyboard; a click on the scrim closes it; ⌘K's palette replaces it; the
/// focused tab given the keyboard from code (`focus_active`) leaves it open with the keyboard;
/// ⌘T, ⌘N and the tab bar's + (in the top band) open terminals behind it while it keeps the
/// keyboard, and Esc then gives it to the newest; and ⌘W closes the card, not the only
/// terminal.
fn welcome_keys_scene() {
    use crate::welcome_items::{Origin, Row};
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    let dims = size(px(960.0), px(600.0));
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let slot: Slot = Rc::new(RefCell::new(None));
        let over = Rc::new(Cell::new(0));
        let build = welcome_build(
            theme,
            crate::presets::builtins(),
            false,
            slot.clone(),
            over.clone(),
        );
        let n = format!("welcome-keys-{slug}");
        let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
            let ws = slotted(&slot);
            let first = Origin::FirstRun { over: over.get() };
            open_welcome_on(cx, window, &ws, first);
            assert_eq!(welcome_state(cx, &ws).0, Row::Preset(0), "{n}: claude");
            press_keys(cx, window, "down down down down down");
            assert_eq!(
                welcome_state(cx, &ws).0,
                Row::Folder,
                "{n}: round to the folder"
            );
            press_keys(cx, window, "up");
            let (chosen, texts) = welcome_state(cx, &ws);
            assert_eq!(chosen, Row::Plain, "{n}: up to Plain terminal");
            assert!(
                texts.iter().any(|t| t == "Open Terminal"),
                "{n}: the button: {texts:?}"
            );

            press_keys(cx, window, "escape");
            assert!(welcome_of(cx, &ws).is_none(), "{n}: Esc closes it");
            assert!(
                active_terminal_focused(cx, window, &ws),
                "{n}: the terminal types after Esc"
            );

            open_welcome_on(cx, window, &ws, first);
            cx.simulate_click(window, point(px(100.0), px(300.0)), gpui::Modifiers::none());
            settle(cx, window);
            assert!(
                welcome_of(cx, &ws).is_none(),
                "{n}: a scrim click closes it"
            );
            assert!(
                active_terminal_focused(cx, window, &ws),
                "{n}: the terminal types after the scrim"
            );

            open_welcome_on(cx, window, &ws, first);
            press_keys(cx, window, "cmd-k");
            assert!(
                welcome_of(cx, &ws).is_none(),
                "{n}: ⌘K's palette replaces it"
            );
            assert!(palette_of(cx, &ws).is_some(), "{n}: the palette is open");

            open_welcome_on(cx, window, &ws, first);
            assert!(
                palette_of(cx, &ws).is_none(),
                "{n}: it replaces the palette"
            );
            // Code handing the keyboard to the focused tab (a select, a new terminal, a
            // script) leaves it with the card, which stays open.
            cx.update_window(window, |_, window, cx| {
                ws.update(cx, |ws, cx| ws.focus_active(window, cx))
            })
            .expect("focus the active tab");
            settle(cx, window);
            assert!(
                welcome_of(cx, &ws).is_some(),
                "{n}: focusing the active tab leaves it open"
            );
            assert!(
                welcome_focused(cx, window, &ws),
                "{n}: the card keeps the keyboard"
            );
            assert!(
                !active_terminal_focused(cx, window, &ws),
                "{n}: the terminal behind it doesn't type"
            );
            slot.borrow_mut().take();
        };
        let shot = render_with(&theme, dims, build, act);
        save_shot("welcome-keys", slug, &shot);

        // ⌘T, ⌘N and the tab bar's + (in the top band) open terminals behind the card, which
        // keeps the keyboard; Esc then gives it to the newest. Not shot: real shells.
        let slot: Slot = Rc::new(RefCell::new(None));
        let over = Rc::new(Cell::new(0));
        let build = welcome_build(
            theme,
            crate::presets::builtins(),
            false,
            slot.clone(),
            over.clone(),
        );
        let n = format!("welcome-keys-{slug}-behind");
        render_with(&theme, dims, build, move |cx, window| {
            let ws = slotted(&slot);
            cx.update(|cx| cx.bind_keys(crate::new_bindings()));
            let first = Origin::FirstRun { over: over.get() };
            let count = |cx: &mut VisualTestAppContext| welcome_layout(cx, &ws).0.len();
            open_welcome_on(cx, window, &ws, first);
            for (keys, want) in [("cmd-t", 2), ("cmd-n", 3)] {
                press_keys(cx, window, keys);
                assert_eq!(count(cx), want, "{n}: {keys} opened a terminal");
                assert!(welcome_of(cx, &ws).is_some(), "{n}: {keys} leaves it open");
                assert!(
                    welcome_focused(cx, window, &ws),
                    "{n}: the card keeps the keyboard after {keys}"
                );
            }
            press_keys(cx, window, "escape");
            assert!(welcome_of(cx, &ws).is_none(), "{n}: Esc closes it");
            let newest = welcome_layout(cx, &ws).0.into_iter().max();
            let active = cx.read(|cx| ws.read(cx).sessions.active().map(|s| s.id));
            assert_eq!(active, newest, "{n}: the newest terminal is the active one");
            assert!(
                active_terminal_focused(cx, window, &ws),
                "{n}: the newest terminal types after Esc"
            );

            // The tab bar's +, in the top band (not under the scrim): a click there still
            // opens a tab.
            open_welcome_on(cx, window, &ws, first);
            let right = 960.0 - GAP - 1.0;
            let plus = point(
                px(right - PLUS_FROM_EDGE),
                px(GAP + 1.0 + ui_tokens::size::TAB_BAR / 2.0),
            );
            cx.simulate_click(window, plus, gpui::Modifiers::none());
            settle(cx, window);
            assert_eq!(count(cx), 4, "{n}: the band's + opened a tab");
            assert!(welcome_of(cx, &ws).is_some(), "{n}: the + leaves it open");
            assert!(
                welcome_focused(cx, window, &ws),
                "{n}: the card keeps the keyboard after the +"
            );
            slot.borrow_mut().take();
        });

        // ⌘W while the card has the keyboard closes the card, not the only terminal (fuxx
        // would quit with it).
        let slot: Slot = Rc::new(RefCell::new(None));
        let over = Rc::new(Cell::new(0));
        let build = welcome_build(
            theme,
            crate::presets::builtins(),
            false,
            slot.clone(),
            over.clone(),
        );
        let n = format!("welcome-keys-{slug}-close");
        render_with(&theme, dims, build, move |cx, window| {
            let ws = slotted(&slot);
            let first = Origin::FirstRun { over: over.get() };
            open_welcome_on(cx, window, &ws, first);
            press_keys(cx, window, "cmd-w");
            assert!(welcome_of(cx, &ws).is_none(), "{n}: ⌘W closes the card");
            assert_eq!(
                welcome_layout(cx, &ws).0,
                vec![over.get()],
                "{n}: the terminal stays"
            );
            assert!(
                cx.update_window(window, |_, _, _| ()).is_ok(),
                "{n}: the window stays (fuxx doesn't quit)"
            );
            assert!(
                active_terminal_focused(cx, window, &ws),
                "{n}: the terminal types after ⌘W"
            );
            slot.borrow_mut().take();
        });
    }
}

/// Whether the open Welcome card has the keyboard.
fn welcome_focused(
    cx: &mut VisualTestAppContext,
    window: AnyWindowHandle,
    ws: &gpui::Entity<Workspace>,
) -> bool {
    use gpui::Focusable;
    let card = welcome_of(cx, ws).expect("the card is open");
    cx.update_window(window, |_, window, cx| {
        card.focus_handle(cx).is_focused(window)
    })
    .expect("read the focus")
}

/// The Welcome card in a small window, in both themes: `welcome-narrow`, 480 × 420, nine
/// presets, a 120-character folder, the last preset chosen.
///
/// Panics unless the card keeps 16 pt from the window's sides and bottom and stays below the
/// top band; the chosen row (scrolled to) is inside the agent list's visible part; and the
/// folder is cut at its start (a folder ending in wide letters shows more ink than one ending
/// in dots).
fn welcome_narrow_scene() {
    use crate::welcome_items::{Origin, Row};
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    let (w, h) = (480.0_f32, 420.0_f32);
    let dims = size(px(w), px(h));
    let nine: Vec<crate::presets::Preset> = (1..=9)
        .map(|i| crate::presets::Preset {
            name: format!("agent-{i}"),
            command: format!("agent-{i}"),
        })
        .collect();
    // Dots then wide letters, and the other way round.
    let wide_end = format!("/{}/{}", ".".repeat(78), "W".repeat(40));
    let thin_end = format!("/{}/{}", "W".repeat(40), ".".repeat(78));
    assert_eq!(wide_end.chars().count(), 120);
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let ui = theme.ui();
        let n = format!("welcome-narrow-{slug}");
        // The folder field's ink with `folder`, and (for the first) the card's place.
        type Seen = (
            gpui::Bounds<Pixels>,
            gpui::Bounds<Pixels>,
            gpui::Bounds<Pixels>,
            gpui::Bounds<Pixels>,
        );
        let run = |folder: String| {
            let slot: Slot = Rc::new(RefCell::new(None));
            let over = Rc::new(Cell::new(0));
            let build = welcome_build(theme, nine.clone(), false, slot.clone(), over.clone());
            let seen: Rc<Cell<Option<Seen>>> = Rc::default();
            let found = seen.clone();
            let m = n.clone();
            let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let ws = slotted(&slot);
                open_welcome_on(cx, window, &ws, Origin::FirstRun { over: over.get() });
                let card = welcome_of(cx, &ws).expect("open");
                let path = std::path::PathBuf::from(&folder);
                cx.update(|cx| card.update(cx, |c, cx| c.set_folder(path.clone(), cx)));
                settle(cx, window);
                let shown = cx.read(|cx| card.read(cx).folder().to_path_buf());
                assert_eq!(shown, path, "{m}: the folder set");
                press_keys(cx, window, "down down down down down down down down");
                assert_eq!(welcome_state(cx, &ws).0, Row::Preset(8), "{m}: the last");
                found.set(cx.read(|cx| {
                    let c = card.read(cx);
                    Some((
                        c.card_bounds()?,
                        c.list_bounds()?,
                        c.row_bounds(Row::Preset(8))?,
                        c.folder_bounds()?,
                    ))
                }));
                slot.borrow_mut().take();
            };
            let shot = render_with(&theme, dims, build, act);
            (shot, seen.get().expect("drawn"))
        };
        let (shot, (card, list, last, field)) = run(wide_end.clone());
        save_shot("welcome-narrow", slug, &shot);
        let (left, right, top, bottom) = outer_edges(card);
        assert!(
            left >= 16.0 - 0.5 && right <= w - 16.0 + 0.5,
            "{n}: the sides at {left} and {right}"
        );
        assert!(
            top >= ui_tokens::size::TOP_BAND - 0.5 && bottom <= h - 16.0 + 0.5,
            "{n}: the top at {top}, the bottom at {bottom}"
        );
        assert!(
            f32::from(last.top()) >= f32::from(list.top()) - 0.5
                && f32::from(last.bottom()) <= f32::from(list.bottom()) + 0.5,
            "{n}: the chosen row {last:?} shows in the list {list:?}"
        );
        // Inked pixels in the folder field, inside its padding.
        let ink = |shot: &Shot, field: gpui::Bounds<Pixels>| {
            let scale = shot.width as f32 / w;
            let (x0, x1) = (
                f32::from(field.left()) + 8.0,
                f32::from(field.right()) - 8.0,
            );
            let (y0, y1) = (
                f32::from(field.top()) + 4.0,
                f32::from(field.bottom()) - 4.0,
            );
            let mut count = 0;
            for dy in (y0 * scale) as usize..(y1 * scale) as usize {
                for dx in (x0 * scale) as usize..(x1 * scale) as usize {
                    let p = pixel_at(shot, dims, dx as f32 / scale, dy as f32 / scale);
                    if gap(p, ui.panel_hover) >= 40 {
                        count += 1;
                    }
                }
            }
            count
        };
        let wide_shown = ink(&shot, field);
        let (thin_shot, (_, _, _, thin_field)) = run(thin_end.clone());
        let thin_shown = ink(&thin_shot, thin_field);
        assert!(
            wide_shown * 2 > thin_shown * 3,
            "{n}: the folder's end shows ({wide_shown} vs {thin_shown})"
        );
    }
}

/// The built-in presets' names with a harmless command each: the pick scenes start real
/// shells, which then run it.
fn welcome_quiet_presets() -> Vec<crate::presets::Preset> {
    crate::presets::builtins()
        .into_iter()
        .map(|p| crate::presets::Preset {
            command: "true".into(),
            ..p
        })
        .collect()
}

/// The sessions' ids in list order, and how many workspaces there are.
fn welcome_layout(
    cx: &mut VisualTestAppContext,
    ws: &gpui::Entity<Workspace>,
) -> (Vec<u64>, usize) {
    cx.read(|cx| {
        let ws = ws.read(cx);
        (
            ws.sessions.items().iter().map(|s| s.id).collect(),
            ws.model.workspaces().len(),
        )
    })
}

/// Set the open card's folder, as the folder panel's answer does.
fn welcome_folder_to(
    cx: &mut VisualTestAppContext,
    window: AnyWindowHandle,
    ws: &gpui::Entity<Workspace>,
    folder: &str,
) {
    let card = welcome_of(cx, ws).expect("the card is open");
    let folder = PathBuf::from(folder);
    cx.update(|cx| card.update(cx, |c, cx| c.set_folder(folder, cx)));
    settle(cx, window);
}

/// One stage of `welcome-start`: given the workspace, `s1`'s id and the stage's name.
type WelcomeStage = Box<
    dyn FnOnce(&mut VisualTestAppContext, AnyWindowHandle, &gpui::Entity<Workspace>, u64, &str),
>;

/// What a pick on the Welcome card opens and closes, in Mocha (both themes with `--all-themes`;
/// not shot: the picks start real shells): `welcome-start`, over `s1`, a terminal in the made-up
/// home.
///
/// Panics unless: (a) on a first run with `s1` alone, codex in `/w/app` by Enter closes the
/// card and replaces `s1` (one session, not `s1`, in `/w/app` with the codex preset, one
/// workspace) and the new terminal has the keyboard; (b) the same with a file tab beside `s1`
/// opens codex and keeps `s1` (two sessions, two workspaces); (c) Plain terminal in home
/// opens nothing and keeps `s1`; (d) from Help with two workspaces, claude makes a third and
/// closes none, and the new terminal (no project: home) has the keyboard; (e) when the new
/// terminal can't open (refused while the background service starts) `s1` stays and fuxx
/// doesn't quit; (f) with `s1` closed by a script under the card (behind a second workspace),
/// the pick opens its terminal, closes nothing more, and fuxx doesn't quit.
fn welcome_start_scene() {
    use crate::welcome_items::{Origin, Row};
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    let dims = size(px(960.0), px(600.0));
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let run = |stage: &'static str, act: WelcomeStage| {
            let slot: Slot = Rc::new(RefCell::new(None));
            let over = Rc::new(Cell::new(0));
            let build = welcome_build(
                theme,
                welcome_quiet_presets(),
                false,
                slot.clone(),
                over.clone(),
            );
            let n = format!("welcome-start-{slug}-{stage}");
            render_with(&theme, dims, build, move |cx, window| {
                let ws = slotted(&slot);
                act(cx, window, &ws, over.get(), &n);
                slot.borrow_mut().take();
            });
        };

        // (a) Codex in /w/app replaces the first terminal.
        run(
            "a",
            Box::new(|cx, window, ws, s1, n| {
                open_welcome_on(cx, window, ws, Origin::FirstRun { over: s1 });
                welcome_folder_to(cx, window, ws, "/w/app");
                press_keys(cx, window, "down down down");
                assert_eq!(welcome_state(cx, ws).0, Row::Preset(3), "{n}: codex");
                press_keys(cx, window, "enter");
                assert!(
                    welcome_of(cx, ws).is_none(),
                    "{n}: the pick closes the card"
                );
                let (ids, workspaces) = welcome_layout(cx, ws);
                assert_eq!(ids.len(), 1, "{n}: one session: {ids:?}");
                assert_ne!(ids[0], s1, "{n}: the first terminal was replaced");
                assert_eq!(workspaces, 1, "{n}: one workspace");
                let (project, preset) = cx.read(|cx| {
                    let s = &ws.read(cx).sessions.items()[0];
                    (s.project.clone(), s.preset.as_ref().map(|p| p.name.clone()))
                });
                assert_eq!(project, Some(PathBuf::from("/w/app")), "{n}: in /w/app");
                assert_eq!(preset.as_deref(), Some("codex"), "{n}: the codex preset");
                assert!(
                    active_terminal_focused(cx, window, ws),
                    "{n}: the new terminal types"
                );
            }),
        );

        // (b) A file tab beside the first terminal: it stays.
        run(
            "b",
            Box::new(|cx, window, ws, s1, n| {
                let file = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"));
                cx.update_window(window, |_, window, cx| {
                    ws.update(cx, |ws, cx| {
                        ws.open_file_tab(file, crate::layout_model::FileMode::Plain, window, cx)
                    })
                })
                .expect("open a file tab");
                settle(cx, window);
                assert_eq!(welcome_layout(cx, ws), (vec![s1], 1), "{n}: one terminal");
                open_welcome_on(cx, window, ws, Origin::FirstRun { over: s1 });
                welcome_folder_to(cx, window, ws, "/w/app");
                press_keys(cx, window, "down down down enter");
                assert!(
                    welcome_of(cx, ws).is_none(),
                    "{n}: the pick closes the card"
                );
                let (ids, workspaces) = welcome_layout(cx, ws);
                assert_eq!(ids.len(), 2, "{n}: two sessions: {ids:?}");
                assert!(ids.contains(&s1), "{n}: the first terminal stays: {ids:?}");
                assert_eq!(workspaces, 2, "{n}: two workspaces");
            }),
        );

        // (c) Plain terminal in home: what is already open.
        run(
            "c",
            Box::new(|cx, window, ws, s1, n| {
                open_welcome_on(cx, window, ws, Origin::FirstRun { over: s1 });
                press_keys(cx, window, "down down down down");
                assert_eq!(welcome_state(cx, ws).0, Row::Plain, "{n}: Plain terminal");
                press_keys(cx, window, "enter");
                assert!(
                    welcome_of(cx, ws).is_none(),
                    "{n}: the pick closes the card"
                );
                assert_eq!(welcome_layout(cx, ws), (vec![s1], 1), "{n}: s1 alone");
                assert!(
                    active_terminal_focused(cx, window, ws),
                    "{n}: the first terminal types"
                );
            }),
        );

        // (d) From Help with two workspaces: a third, none closed.
        run(
            "d",
            Box::new(|cx, window, ws, s1, n| {
                let s2 = cx
                    .update_window(window, |_, window, cx| {
                        ws.update(cx, |ws, cx| {
                            let home = std::path::Path::new("/Users/me");
                            ws.open_unnamed_shot_session(Some(home), "", window, cx)
                        })
                    })
                    .expect("a second workspace");
                settle(cx, window);
                open_welcome_on(cx, window, ws, Origin::Help);
                assert_eq!(welcome_state(cx, ws).0, Row::Preset(0), "{n}: claude");
                press_keys(cx, window, "enter");
                assert!(
                    welcome_of(cx, ws).is_none(),
                    "{n}: the pick closes the card"
                );
                let (ids, workspaces) = welcome_layout(cx, ws);
                assert_eq!(workspaces, 3, "{n}: a third workspace");
                assert_eq!(ids.len(), 3, "{n}: three sessions: {ids:?}");
                assert!(ids.contains(&s1) && ids.contains(&s2), "{n}: none closed");
                let project = cx.read(|cx| {
                    let ws = ws.read(cx);
                    let new = ws
                        .sessions
                        .items()
                        .iter()
                        .find(|s| s.id != s1 && s.id != s2)
                        .expect("the new terminal");
                    new.project.clone()
                });
                assert_eq!(project, None, "{n}: home is no project");
                // M11aa: claude is a provider's preset: its shell (above) and, split beside it,
                // its agent tab, which takes the keyboard once made (the next frame).
                settle(cx, window);
                let agent = cx
                    .update_window(window, |_, window, cx| {
                        let ws = ws.read(cx);
                        let (tab, view) = ws.focused_agent_tab()?;
                        let w = ws.model.locate(tab)?.0;
                        let panes = ws.model.workspace(w)?.panes.len();
                        let keys =
                            gpui::Focusable::focus_handle(view.read(cx), cx).is_focused(window);
                        Some((panes, keys, ws.workspace_label(w).to_string()))
                    })
                    .expect("read the workspace");
                assert_eq!(
                    agent,
                    Some((2, true, "Claude".to_string())),
                    "{n}: a shell and the Claude tab beside it, which types"
                );
            }),
        );

        // (f) A script closed the first terminal (behind a second workspace) under the card.
        run(
            "f",
            Box::new(|cx, window, ws, s1, n| {
                let s2 = cx
                    .update_window(window, |_, window, cx| {
                        ws.update(cx, |ws, cx| {
                            let home = std::path::Path::new("/Users/me");
                            ws.open_unnamed_shot_session(Some(home), "", window, cx)
                        })
                    })
                    .expect("a second workspace");
                settle(cx, window);
                open_welcome_on(cx, window, ws, Origin::FirstRun { over: s1 });
                welcome_folder_to(cx, window, ws, "/w/app");
                cx.update_window(window, |_, window, cx| {
                    ws.update(cx, |ws, cx| {
                        let at = ws.index_of(s1).expect("s1");
                        ws.close_at(at, window, cx)
                    })
                })
                .expect("close s1");
                settle(cx, window);
                assert_eq!(welcome_layout(cx, ws), (vec![s2], 1), "{n}: s2 left");
                let card = welcome_of(cx, ws).expect("the card stays open");
                let focused = cx
                    .update_window(window, |_, window, cx| {
                        use gpui::Focusable;
                        card.read(cx).focus_handle(cx).is_focused(window)
                    })
                    .expect("read the focus");
                assert!(focused, "{n}: the card keeps the keyboard");
                press_keys(cx, window, "enter");
                assert!(
                    welcome_of(cx, ws).is_none(),
                    "{n}: the pick closes the card"
                );
                let (ids, workspaces) = welcome_layout(cx, ws);
                assert_eq!(ids.len(), 2, "{n}: s2 and the new one: {ids:?}");
                assert!(ids.contains(&s2), "{n}: s2 stays: {ids:?}");
                assert_eq!(workspaces, 2, "{n}: two workspaces");
                assert!(!cx.read(|cx| ws.read(cx).quitting), "{n}: fuxx stays");
            }),
        );

        // (e) The new terminal can't open: the first stays, fuxx stays.
        run(
            "e",
            Box::new(|cx, window, ws, s1, n| {
                open_welcome_on(cx, window, ws, Origin::FirstRun { over: s1 });
                welcome_folder_to(cx, window, ws, "/w/app");
                // Opens are refused while the background service starts.
                cx.update(|cx| {
                    ws.update(cx, |ws, _| {
                        ws.service = crate::workspace_daemon::Service::Starting
                    })
                });
                press_keys(cx, window, "enter");
                assert!(
                    welcome_of(cx, ws).is_none(),
                    "{n}: the pick closes the card"
                );
                assert_eq!(welcome_layout(cx, ws), (vec![s1], 1), "{n}: s1 stays");
                assert!(!cx.read(|cx| ws.read(cx).quitting), "{n}: fuxx stays");
                cx.update(|cx| {
                    ws.update(cx, |ws, _| {
                        ws.service = crate::workspace_daemon::Service::Off
                    })
                });
            }),
        );
        println!("ui-shots: welcome-start-{slug} ok");
    }
}

/// Choose… and Return on the folder row, in Mocha (both themes with `--all-themes`):
/// `welcome-folder`, the folder panel answered by the scene (the off-screen platform has none).
///
/// Panics unless, from Plain terminal round to the folder row, Return asks for a folder and,
/// while it's asked (the keyboard taken from the card), the folder stays; the answer
/// `/w/other` becomes the folder, Plain terminal is chosen again and the card has the
/// keyboard back; a cancelled panel leaves the folder, and gives the row and the keyboard back
/// the same; a click on the folder field, and one on Choose…, each ask too (answered, the
/// folder changes and the card has the keyboard back); and asking again while the panel is up
/// opens no second one (its answer never lands, the first one's does).
fn welcome_folder_scene() {
    use crate::welcome_items::{Origin, Row};
    use futures::channel::oneshot;
    use gpui::Focusable;
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    let dims = size(px(960.0), px(600.0));
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let slot: Slot = Rc::new(RefCell::new(None));
        let over = Rc::new(Cell::new(0));
        let build = welcome_build(
            theme,
            crate::presets::builtins(),
            false,
            slot.clone(),
            over.clone(),
        );
        let n = format!("welcome-folder-{slug}");
        let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
            let ws = slotted(&slot);
            open_welcome_on(cx, window, &ws, Origin::FirstRun { over: over.get() });
            let card = welcome_of(cx, &ws).expect("open");
            let folder =
                |cx: &mut VisualTestAppContext| cx.read(|cx| card.read(cx).folder().to_path_buf());
            let card_focused = |cx: &mut VisualTestAppContext| {
                cx.update_window(window, |_, window, cx| {
                    card.read(cx).focus_handle(cx).is_focused(window)
                })
                .expect("read the focus")
            };
            // The panel takes the keyboard while it's up.
            let panel_up = |cx: &mut VisualTestAppContext| {
                cx.update_window(window, |_, window, cx| {
                    let ws = ws.read(cx);
                    let active = ws.sessions.active().expect("a terminal");
                    active.view.focus_handle(cx).focus(window, cx);
                })
                .expect("focus the terminal");
                settle(cx, window);
            };

            press_keys(cx, window, "down down down down");
            for (answer, want) in [(Some("/w/other"), "/w/other"), (None, "/w/other")] {
                let (tx, rx) = oneshot::channel();
                cx.update(|cx| card.update(cx, |c, _| c.answer_panel_with(rx)));
                // Plain terminal, then round to the folder row.
                assert_eq!(welcome_state(cx, &ws).0, Row::Plain, "{n}: Plain terminal");
                press_keys(cx, window, "down");
                assert_eq!(welcome_state(cx, &ws).0, Row::Folder, "{n}: the folder row");
                let before = folder(cx);
                press_keys(cx, window, "enter");
                assert!(welcome_of(cx, &ws).is_some(), "{n}: the card stays open");
                panel_up(cx);
                assert!(!card_focused(cx), "{n}: the panel has the keyboard");
                assert_eq!(folder(cx), before, "{n}: unchanged while asked");
                tx.send(answer.map(PathBuf::from))
                    .expect("answer the panel");
                settle(cx, window);
                assert_eq!(folder(cx), PathBuf::from(want), "{n}: after {answer:?}");
                assert_eq!(
                    welcome_state(cx, &ws).0,
                    Row::Plain,
                    "{n}: Plain terminal chosen again after {answer:?}"
                );
                assert!(card_focused(cx), "{n}: the card types after {answer:?}");
                assert!(welcome_of(cx, &ws).is_some(), "{n}: still open");
            }
            let texts = welcome_state(cx, &ws).1;
            assert!(
                texts.iter().any(|t| t == "/w/other"),
                "{n}: the field shows the folder: {texts:?}"
            );

            // A click on the folder field, or on Choose…, asks too.
            let field = cx.read(|cx| card.read(cx).folder_bounds());
            let choose = cx.read(|cx| card.read(cx).choose_bounds());
            for (what, at, picked) in [
                ("the field", field, "/w/field"),
                ("Choose\u{2026}", choose, "/w/choose"),
            ] {
                let at = at.unwrap_or_else(|| panic!("{n}: {what} is drawn"));
                let (tx, rx) = oneshot::channel();
                cx.update(|cx| card.update(cx, |c, _| c.answer_panel_with(rx)));
                cx.simulate_click(window, at.center(), gpui::Modifiers::none());
                settle(cx, window);
                assert!(
                    welcome_of(cx, &ws).is_some(),
                    "{n}: {what}: the card stays open"
                );
                panel_up(cx);
                tx.send(Some(PathBuf::from(picked)))
                    .expect("answer the panel");
                settle(cx, window);
                assert_eq!(folder(cx), PathBuf::from(picked), "{n}: {what} asks");
                assert!(card_focused(cx), "{n}: the card types after {what}");
            }

            // One panel at a time: asking again while it's up does nothing.
            let (first_tx, first_rx) = oneshot::channel();
            cx.update(|cx| card.update(cx, |c, _| c.answer_panel_with(first_rx)));
            press_keys(cx, window, "down enter");
            assert_eq!(welcome_state(cx, &ws).0, Row::Folder, "{n}: the folder row");
            let (second_tx, second_rx) = oneshot::channel();
            cx.update(|cx| card.update(cx, |c, _| c.answer_panel_with(second_rx)));
            press_keys(cx, window, "enter");
            let _ = second_tx.send(Some(PathBuf::from("/w/second")));
            settle(cx, window);
            assert_eq!(
                folder(cx),
                PathBuf::from("/w/choose"),
                "{n}: no second panel while one is up"
            );
            first_tx
                .send(Some(PathBuf::from("/w/first")))
                .expect("answer the panel");
            settle(cx, window);
            assert_eq!(
                folder(cx),
                PathBuf::from("/w/first"),
                "{n}: the first panel's answer"
            );
            slot.borrow_mut().take();
        };
        let shot = render_with(&theme, dims, build, act);
        save_shot("welcome-folder", slug, &shot);
    }
}

/// Help → Welcome to fuxx over a terminal in a project (M11k Task 4), in Mocha (both themes
/// with `--all-themes`):
/// `welcome-reopen`, not shot. The menu item runs `open_welcome` with `Origin::Help` (the
/// action's handler needs real windows, so the scene makes that call itself).
///
/// Panics unless the card is open with `origin() == Help` and its folder is the active
/// terminal's project (`/Users/me/work/api`, not the home), and Esc closes it.
fn welcome_reopen_scene() {
    use crate::welcome_items::Origin;
    use std::cell::RefCell;
    use std::rc::Rc;

    let dims = size(px(960.0), px(600.0));
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let slot: Slot = Rc::new(RefCell::new(None));
        let keep = slot.clone();
        let build = move |window: &mut Window, cx: &mut App| -> AnyView {
            cx.set_global(Appearance {
                presets: welcome_quiet_presets(),
                ..shot_appearance(theme, cx)
            });
            cx.bind_keys(crate::key_bindings());
            let ws = cx.new(|cx| {
                let mut ws = Workspace::for_shots(window, cx);
                ws.sidebar.hint_dismissed = true;
                let project = std::path::Path::new("/Users/me/work/api");
                ws.open_unnamed_shot_session(Some(project), "", window, cx);
                ws
            });
            *keep.borrow_mut() = Some(ws.clone());
            ws.into()
        };
        let n = format!("welcome-reopen-{slug}");
        render_with(&theme, dims, build, move |cx, window| {
            let ws = slotted(&slot);
            assert!(welcome_of(cx, &ws).is_none(), "{n}: no card yet");
            open_welcome_on(cx, window, &ws, Origin::Help);
            let card = welcome_of(cx, &ws).expect("the card is open");
            let (origin, folder) =
                cx.read(|cx| (card.read(cx).origin(), card.read(cx).folder().to_path_buf()));
            assert_eq!(origin, Origin::Help, "{n}: opened from Help");
            assert_eq!(
                folder,
                std::path::PathBuf::from("/Users/me/work/api"),
                "{n}: the active terminal's project"
            );
            press_keys(cx, window, "escape");
            assert!(welcome_of(cx, &ws).is_none(), "{n}: Esc closes it");
            println!("ui-shots: {n} ok");
        });
    }
}

/// The card on a first run (not shot: a real shell starts): `welcome-first-run`. A workspace
/// that hasn't welcomed anyone opens its first terminal as a launch with nothing to restore
/// does.
///
/// Panics unless, once it has opened, the card is open over it (`FirstRun` over its id), has
/// the keyboard, and the workspace counts as welcomed; and unless a workspace that has
/// welcomed shows no card.
fn welcome_first_run_scene() {
    use crate::welcome_items::Origin;
    use gpui::Focusable;
    use std::cell::RefCell;
    use std::rc::Rc;

    let dims = size(px(960.0), px(600.0));
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        for welcomed in [false, true] {
            let slot: Slot = Rc::new(RefCell::new(None));
            let keep = slot.clone();
            let build = move |window: &mut Window, cx: &mut App| -> AnyView {
                cx.set_global(Appearance {
                    presets: welcome_quiet_presets(),
                    ..shot_appearance(theme, cx)
                });
                cx.bind_keys(crate::key_bindings());
                let ws = cx.new(|cx| {
                    let mut ws = Workspace::for_shots(window, cx);
                    ws.sidebar.hint_dismissed = true;
                    ws.welcomed = welcomed;
                    ws.restoring = true;
                    ws
                });
                *keep.borrow_mut() = Some(ws.clone());
                ws.into()
            };
            let n = format!("welcome-first-run-{slug}-{welcomed}");
            let m = n.clone();
            render_with(&theme, dims, build, move |cx, window| {
                let n = m;
                let ws = slotted(&slot);
                cx.update_window(window, |_, window, cx| {
                    ws.update(cx, |ws, cx| ws.open_first(window, cx))
                })
                .expect("open the first terminal");
                settle(cx, window);
                let (ids, restoring, now_welcomed) = cx.read(|cx| {
                    let ws = ws.read(cx);
                    (
                        ws.sessions.items().iter().map(|s| s.id).collect::<Vec<_>>(),
                        ws.restoring,
                        ws.welcomed,
                    )
                });
                assert_eq!(ids.len(), 1, "{n}: the first terminal: {ids:?}");
                assert!(!restoring, "{n}: restoring finished");
                assert!(now_welcomed, "{n}: welcomed from now on");
                let card = welcome_of(cx, &ws);
                if welcomed {
                    assert!(card.is_none(), "{n}: no card once welcomed");
                } else {
                    let card = card.expect("the card on a first run");
                    let origin = cx.read(|cx| card.read(cx).origin());
                    assert_eq!(origin, Origin::FirstRun { over: ids[0] }, "{n}: over it");
                    let focused = cx
                        .update_window(window, |_, window, cx| {
                            card.read(cx).focus_handle(cx).is_focused(window)
                        })
                        .expect("read the focus");
                    assert!(focused, "{n}: the card has the keyboard");
                }
                slot.borrow_mut().take();
            });
            println!("ui-shots: {n} ok");
        }
    }
}
