//! The status bar (M11v): its facts, hidden, its port chips, a narrow window, and the update pill.

use super::*;

/// This file, as `file!()` gives it (repo-relative under cargo): `registry::AREAS` pairs it with
/// `SCENES`, so `--changed` runs these scenes when this file changes.
pub(super) const FILE: &str = file!();

/// This area's scenes (`registry.rs` runs them in its `ORDER`).
pub(super) const SCENES: &[Scene] = &[
    Scene {
        name: "status-bar",
        run: status_bar_scenes,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/status_bar.rs",
            "src/status_mark.rs",
            "src/sidebar.rs",
            "src/git_branch.rs",
            "src/procs.rs",
            "src/status.rs",
            "src/file_tab.rs",
            "src/layout_model.rs",
            "src/reduce_motion.rs",
        ],
    },
    Scene {
        name: "status-bar-hidden",
        run: status_bar_hidden_scenes,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &["src/status_bar.rs", "src/panes.rs", "src/git_branch.rs"],
    },
    Scene {
        name: "status-bar-ports",
        run: status_bar_ports_scenes,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/status_bar.rs",
            "src/procs.rs",
            "src/browser_tab.rs",
            "src/workspace_browser.rs",
            "src/layout_model.rs",
            "src/git_branch.rs",
        ],
    },
    Scene {
        name: "status-bar-narrow",
        run: status_bar_narrow_scenes,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/status_bar.rs",
            "src/updater.rs",
            "src/procs.rs",
            "src/git_branch.rs",
            "src/sidebar.rs",
        ],
    },
    Scene {
        name: "update-button",
        run: update_button_scenes,
        themes: Themes::One,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/status_bar.rs",
            "src/updater.rs",
            "src/update_release.rs",
        ],
    },
];

/// The folder the status bar's terminals work in, a deep one, and a 40-character branch.
const FUXX: &str = "/Users/me/code/fuxx-cli";

const DEEP: &str = "/Users/me/code/clients/acme-corporation/platform/services/payments/api-gateway";

const BRANCH_40: &str = "feature/status-bar-ports-and-update-pill";

/// The status bar's two terminals: claude working 4 minutes in fuxx-cli (on `main`, running
/// `claude`), agent waiting 2 minutes in web; claude active.
const BAR_SESSIONS: [ShotSession; 2] = [
    shot("claude", FUXX, Status::Working, 4 * 60 + 5, AGENT_TEXT),
    shot("agent", WEB, Status::Waiting, 2 * 60 + 5, CLAUDE_PROMPT),
];

/// `BAR_SESSIONS`, `active` the one shown, claude found in `folder` on `branch` and running
/// `claude`, the sidebar `width` wide; `ports`, claude's listening ports.
fn bar_workspace(
    active: usize,
    width: f32,
    (folder, branch): (&str, &str),
    ports: &[u16],
    window: &mut Window,
    cx: &mut Context<Workspace>,
) -> Workspace {
    let mut ws = sidebar_workspace(&BAR_SESSIONS, None, active, &[], window, cx);
    ws.sidebar.hint_dismissed = true;
    ws.sidebar.width = width;
    let claude = ws.sessions.items()[0].id;
    // As looked up (a folder no terminal is found in loses its branch on the next sync).
    let found = Some(PathBuf::from(folder));
    ws.places
        .set_folder(claude, found, std::time::Instant::now());
    ws.places
        .set_branch(std::path::Path::new(folder), Some(branch.into()));
    ws.programs
        .set_for_shots(claude, ProgramState::Running("claude".into()));
    ws.ports.set_for_shots(claude, ports.to_vec());
    ws
}

/// `bar_workspace` short of room (Review Focus 4): claude in a deep folder on a 40-character
/// branch, listening on 6 ports, beside the narrowest sidebar.
fn narrow_bar_workspace(window: &mut Window, cx: &mut Context<Workspace>) -> Workspace {
    let ports = [3000, 3001, 5173, 6006, 8080, 9229];
    let width = crate::ui_tokens::size::SIDEBAR_MIN_WIDTH;
    bar_workspace(0, width, (DEEP, BRANCH_40), &ports, window, cx)
}

/// The runs of device pixels along the line `y` (points) that are colour `c`, 6 pt long or
/// more (not text's anti-aliasing that happens to match): (start, end) in points, end
/// exclusive.
fn runs_of(
    shot: &Shot,
    dims: Size<Pixels>,
    y: f32,
    c: alacritty_terminal::vte::ansi::Rgb,
) -> Vec<(f32, f32)> {
    runs_over(shot, dims, y, c, None)
}

/// `runs_of`, and with `backdrop` a point count above `y` (below, if negative), only where the pixel there differs from
/// the one at `y`: a fill that the card's shadow over the bar (M11al) darkens to about its own
/// colour is told from that shadow.
fn runs_over(
    shot: &Shot,
    dims: Size<Pixels>,
    y: f32,
    c: alacritty_terminal::vte::ansi::Rgb,
    backdrop: Option<f32>,
) -> Vec<(f32, f32)> {
    let scale = shot.width as f32 / f32::from(dims.width);
    let mut runs = Vec::new();
    let mut start: Option<f32> = None;
    for dx in 0..shot.width as usize {
        let x = dx as f32 / scale;
        let here = pixel_at(shot, dims, x, y);
        let apart = backdrop.is_none_or(|up| {
            let there = pixel_at(shot, dims, x, y - up);
            here.iter().zip(there).any(|(a, b)| a.abs_diff(b) > 3)
        });
        let on = near_by(here, c, 3) && apart;
        match (on, start) {
            (true, None) => start = Some(x),
            (false, Some(s)) => {
                runs.push((s, x));
                start = None;
            }
            _ => {}
        }
    }
    if let Some(s) = start {
        runs.push((s, f32::from(dims.width)));
    }
    runs.retain(|(a, b)| b - a >= 6.0);
    runs
}

/// Whether two pixels differ by more than the card's shadow shading the bar would make them.
fn apart(a: [u8; 3], b: [u8; 3]) -> bool {
    a.iter().zip(b).any(|(x, y)| x.abs_diff(y) > 4)
}

/// How far in from a chip's end its fill starts 1.5 pt below its top (its corner's curve).
fn chip_corner_inset() -> f32 {
    use crate::status_bar::CHIP_RADIUS;
    let r = CHIP_RADIUS;
    r - (r * r - (r - 1.5) * (r - 1.5)).sqrt()
}

/// `status-bar-hidden` (M11v part 3): Settings → General → Status Bar off: no bar along the
/// window's bottom (no folder words where it was); the cards reach down instead.
fn status_bar_hidden_scenes() {
    use crate::ui_tokens::size::{SIDEBAR_DEFAULT_WIDTH, STATUS_BAR};
    use std::cell::RefCell;
    use std::rc::Rc;

    let dims = size(px(1000.0), px(480.0));
    let (w, h) = (f32::from(dims.width), f32::from(dims.height));
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let ui = theme.ui();
        for shown in [true, false] {
            let n = format!("status-bar-hidden-{shown}-{slug}");
            let slot: Slot = Rc::new(RefCell::new(None));
            let keep = slot.clone();
            let build = move |window: &mut Window, cx: &mut App| -> AnyView {
                cx.set_global(Appearance {
                    status_bar: shown,
                    ..sidebar_appearance(theme, cx)
                });
                // A deep folder and a long branch: the words run on right of the (290 pt) sidebar,
                // where nothing but the bar draws faint text.
                let ws = cx.new(|cx| {
                    bar_workspace(0, SIDEBAR_DEFAULT_WIDTH, (DEEP, BRANCH_40), &[], window, cx)
                });
                *keep.borrow_mut() = Some(ws.clone());
                ws.into()
            };
            let act = move |_: &mut VisualTestAppContext, _: AnyWindowHandle| {
                slot.borrow_mut().take();
            };
            let shot = render_with(&theme, dims, build, act);
            if !shown {
                save_shot("status-bar-hidden", slug, &shot);
            }
            // The bar's folder words (faint) sit right of the sidebar, along the bottom 22 pt.
            let rows = ((STATUS_BAR - 6.0) * 2.0) as usize;
            let cols = ((w - 40.0 - SIDEBAR_DEFAULT_WIDTH) * 2.0) as usize;
            let faint = (0..rows).any(|dy| {
                (0..cols).any(|dx| {
                    let p = pixel_at(
                        &shot,
                        dims,
                        SIDEBAR_DEFAULT_WIDTH + dx as f32 * 0.5,
                        h - STATUS_BAR + 3.0 + dy as f32 * 0.5,
                    );
                    p.iter()
                        .zip([ui.text_faint.r, ui.text_faint.g, ui.text_faint.b])
                        .all(|(a, b)| a.abs_diff(b) <= 10)
                })
            });
            assert_eq!(faint, shown, "{n}: the bar's words along the bottom");
        }
    }
}

/// The status bar over a real workspace, in both themes:
/// - `status-bar`: claude working: its mark, `Working · 4m` in `text_dim`, the branch glyph and
///   `main`, `~/code/fuxx-cli` in `text_faint`;
/// - `status-bar-waiting`: agent waiting: `Needs input · 2m` in `ring_waiting`;
/// - `status-bar-file`: a file tab focused in claude's pane: the bar keeps claude's facts;
/// - `status-bar-still`: Reduce motion on: claude's mark is the still one, its open quarter at
///   the top left whatever the clock (moving, at a tenth of the beat, it has turned away).
///
/// Panics unless the bar is `STATUS_BAR` pt of canvas (the card's shadow over it) along the window's whole bottom, under the cards
/// (6 pt between), says what it should (`status_bar_facts`) and draws it: the left side's ink
/// as wide as its pieces measured, in their colours.
fn status_bar_scenes() {
    use crate::status_bar::{
        BRANCH_GAP, BRANCH_GLYPH, ITEM_GAP, MARK, MARK_GAP, PAD, StatusWord, text_width,
    };
    use crate::ui_tokens::size::{CARD_INSET, HAIRLINE, SIDEBAR_DEFAULT_WIDTH, STATUS_BAR};
    use gpui::FontWeight;
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    let dims = size(px(1000.0), px(480.0));
    let (w, h) = (f32::from(dims.width), f32::from(dims.height));
    let (top, mid) = (h - STATUS_BAR, h - STATUS_BAR / 2.0);
    let file = PathBuf::from(format!("{FUXX}/src/main.rs"));
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let ui = theme.ui();
        for scene in [
            "status-bar",
            "status-bar-waiting",
            "status-bar-file",
            "status-bar-still",
        ] {
            let n = format!("{scene}-{slug}");
            let slot: Slot = Rc::new(RefCell::new(None));
            let keep = slot.clone();
            let active = if scene == "status-bar-waiting" { 1 } else { 0 };
            let still = scene == "status-bar-still";
            let build = move |window: &mut Window, cx: &mut App| -> AnyView {
                cx.set_global(sidebar_appearance(theme, cx));
                cx.set_reduce_motion(still);
                let ws = cx.new(|cx| {
                    let ws = bar_workspace(
                        active,
                        SIDEBAR_DEFAULT_WIDTH,
                        (FUXX, "main"),
                        &[],
                        window,
                        cx,
                    );
                    // A tenth into the beat: a moving Working mark has turned 144°.
                    ws.clock.update(cx, |clock, _| clock.phase = 0.1);
                    ws
                });
                *keep.borrow_mut() = Some(ws.clone());
                ws.into()
            };
            // The left side's pieces as measured: the word, the branch and the folder.
            let widths: Rc<Cell<(f32, f32, f32)>> = Rc::default();
            let act = {
                let n = n.clone();
                let file = file.clone();
                let widths = widths.clone();
                move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                    let ws = slotted(&slot);
                    let claude = cx.read(|cx| ws.read(cx).status_bar_facts());
                    if scene == "status-bar" {
                        // The marks' ticker counts the bar's mark (M11p: with the tab's; the
                        // rows' badges are still).
                        let marks = cx.read(|cx| ws.read(cx).marks_on_screen());
                        assert!(
                            marks.iter().any(|((place, _), _)| *place == "status-bar"),
                            "{n}: the bar's mark is on screen: {marks:?}"
                        );
                    }
                    if scene == "status-bar-file" {
                        cx.update_window(window, |_, window, cx| {
                            ws.update(cx, |ws, cx| {
                                ws.open_file_tab(
                                    file.clone(),
                                    crate::layout_model::FileMode::Plain,
                                    window,
                                    cx,
                                )
                            })
                        })
                        .expect("open the file");
                        cx.run_until_parked();
                        let focused = cx.read(|cx| ws.read(cx).focused_terminal());
                        assert_eq!(focused, None, "{n}: the file tab is focused");
                    }
                    let facts = cx
                        .read(|cx| ws.read(cx).status_bar_facts())
                        .expect("the bar's terminal");
                    let (word, text, branch, folder) = match scene {
                        "status-bar-waiting" => (
                            StatusWord::NeedsInput,
                            "Needs input \u{b7} 2m",
                            None,
                            "~/code/web",
                        ),
                        _ => (
                            StatusWord::Working,
                            "Cultivating \u{b7} 4m",
                            Some("main"),
                            "~/code/fuxx-cli",
                        ),
                    };
                    assert_eq!(facts.word, word, "{n}");
                    assert_eq!(facts.text, text, "{n}");
                    assert_eq!(facts.branch.as_deref(), branch, "{n}");
                    let shown = facts
                        .folder
                        .as_deref()
                        .map(|f| crate::sidebar::home_relative(f, Some("/Users/me".as_ref())));
                    assert_eq!(shown.as_deref(), Some(folder), "{n}");
                    if scene == "status-bar-file" {
                        assert_eq!(Some(facts.clone()), claude, "{n}: claude's facts kept");
                    }
                    cx.update_window(window, |_, window, _| {
                        let plain = |t: &str| text_width(t, FontWeight::NORMAL, false, window);
                        widths.set((
                            plain(text),
                            branch.map_or(0.0, |b| BRANCH_GLYPH + BRANCH_GAP + plain(b)),
                            plain(folder),
                        ));
                    })
                    .expect("measure");
                    slot.borrow_mut().take();
                }
            };
            let shot = render_with(&theme, dims, build, act);
            save_shot(scene, slug, &shot);
            let at = |x: f32, y: f32| pixel_at(&shot, dims, x, y);
            let scale = shot.width as f32 / w;
            let canvas = |p: [u8; 3]| near_by(p, ui.sidebar_background, 2);
            // `STATUS_BAR` of canvas along the bar's left end (under the sidebar, past the card's
            // shadow), the card's bottom `CARD_INSET` above it.
            assert!(
                canvas(at(2.0, top + 1.0)),
                "{n}: the bar's top at its left end"
            );
            assert!(
                canvas(at(2.0, h - 1.0)),
                "{n}: the bar's bottom at its left end"
            );
            // No fill of its own (M11al §7): under the sidebar the bar is the canvas's colour,
            // and just under the card the card's shadow falls over it, darker than the canvas.
            assert!(canvas(at(2.0, mid)), "{n}: the bar's middle is the canvas");
            let (under, bare) = (
                at(SIDEBAR_DEFAULT_WIDTH + 40.0, top + 1.0),
                at(2.0, top + 1.0),
            );
            assert!(
                under.iter().zip(bare).all(|(a, b)| *a <= b) && under != bare,
                "{n}: the card's shadow darkens the bar's top: {under:?} vs canvas {bare:?}"
            );
            // The card's inside (the terminal's background) stops at the rim's 1 pt ring of the
            // canvas colour inside its 1 pt edge (2026-11-20), `CARD_INSET` above the bar (its
            // shadow shades the inset, so the inset isn't looked at): the lowest pixel of the
            // fill with the card's edge (the canvas colour since 2026-11-20) right under it.
            let card_x = SIDEBAR_DEFAULT_WIDTH + 40.0;
            let inside = (0..(top * scale) as usize)
                .map(|dy| dy as f32 / scale)
                .filter(|&y| {
                    near_by(at(card_x, y), ui.card, 2)
                        && near_by(at(card_x, y + 1.0 / scale), ui.sidebar_background, 2)
                })
                .fold(0.0, f32::max)
                + 1.0 / scale;
            assert!(
                (top - inside - (CARD_INSET + HAIRLINE)).abs() <= 0.75,
                "{n}: the card's inside ends at {inside}, {} above the bar",
                top - inside
            );
            // The left side's ink, from the mark to the folder's end, as wide as measured.
            let (word_w, branch_w, folder_w) = widths.get();
            // (The bar's upper rows: the window's edge, 2026-11-20, rounds its bottom corners
            // through the lower ones.)
            let ys: Vec<f32> = (0..((STATUS_BAR / 2.0 - 2.0) * scale) as usize)
                .map(|dy| top + 2.0 + dy as f32 / scale)
                .collect();
            // Ink is where a pixel differs from the one before it: the card's shadow over the bar
            // only shades it smoothly.
            let inked = |x: f32| ys.iter().any(|&y| apart(at(x, y), at(x - 1.0 / scale, y)));
            // (Past the window's 1 pt edge, 2026-11-20.)
            let edge = crate::panes::WINDOW_EDGE + 1.0;
            let xs: Vec<f32> = ((edge * scale) as usize..(w / 2.0 * scale) as usize)
                .map(|dx| dx as f32 / scale)
                .collect();
            let ink: Vec<f32> = xs.iter().copied().filter(|&x| inked(x)).collect();
            let (first, last) = (ink[0], ink[ink.len() - 1] + 1.0 / scale);
            let mut expected = MARK + MARK_GAP + word_w + ITEM_GAP + folder_w;
            if branch_w > 0.0 {
                expected += ITEM_GAP + branch_w;
            }
            println!(
                "ui-shots: {n}: left ink {first}..{last}, expected {PAD}..{}",
                PAD + expected
            );
            assert!(
                (first - PAD).abs() <= 2.5,
                "{n}: the mark starts at {first}"
            );
            assert!(
                (last - (PAD + expected)).abs() <= 2.5,
                "{n}: the left side ends at {last}, expected {}",
                PAD + expected
            );
            // Each piece in its colour: some of its ink at full strength.
            let any_in = |x0: f32, x1: f32, c, tol| {
                xs.iter()
                    .filter(|&&x| x >= x0 && x < x1)
                    .any(|&x| ys.iter().any(|&y| near_by(at(x, y), c, tol)))
            };
            let word_x = PAD + MARK + MARK_GAP;
            let word_color = if scene == "status-bar-waiting" {
                ui.ring_waiting
            } else {
                ui.text_dim
            };
            assert!(
                any_in(word_x, word_x + word_w, word_color, 12),
                "{n}: the word in {word_color:?}"
            );
            let folder_x = PAD + expected - folder_w;
            assert!(
                any_in(folder_x, folder_x + folder_w, ui.text_faint, 12),
                "{n}: the folder in text_faint"
            );
            // (In a theme where both are pushed to the same 4.5:1 floor, they're one colour.)
            let faint = [ui.text_faint.r, ui.text_faint.g, ui.text_faint.b];
            if !near_by(faint, ui.text_dim, 12) {
                assert!(
                    !any_in(folder_x, folder_x + folder_w, ui.text_dim, 4),
                    "{n}: the folder not in text_dim"
                );
            }
            if branch_w > 0.0 {
                let branch_x = word_x + word_w + ITEM_GAP;
                assert!(
                    any_in(branch_x, branch_x + BRANCH_GLYPH, ui.text_dim, 12),
                    "{n}: the branch glyph in text_dim"
                );
            }
            // The Working mark: a three-quarter arc from the top, clockwise. Still, its open
            // quarter is at the top left; moving, a tenth into the beat it has turned 144° and
            // the arc is there.
            if scene != "status-bar-waiting" {
                let r = 4.25 * MARK / 12.0 * std::f32::consts::FRAC_1_SQRT_2;
                let top_left = at(PAD + MARK / 2.0 - r, mid - r);
                assert_eq!(
                    canvas(top_left),
                    still,
                    "{n}: the mark's top left {top_left:?} (still: {still})"
                );
            }
        }
    }
}

/// The status bar's port chips, in both themes, claude listening on 5 ports:
/// - `status-bar-ports`: `:3000`, `:5173`, `:8080` and `+2`, filled `row_hover`, at the bar's
///   right end; the pointer on `:3000`: `row_active`;
/// - `status-bar-ports-click`: `:3000` clicked: a browser tab at `http://localhost:3000`, split
///   right of claude's pane and focused; the bar keeps claude's ports.
fn status_bar_ports_scenes() {
    use crate::layout_model::TabKind;
    use crate::status_bar::{CHIP, CHIP_GAP, PAD};
    use crate::ui_tokens::size::{SIDEBAR_DEFAULT_WIDTH, STATUS_BAR};
    use gpui::Modifiers;
    use std::cell::RefCell;
    use std::rc::Rc;

    let dims = size(px(1000.0), px(480.0));
    let (w, h) = (f32::from(dims.width), f32::from(dims.height));
    let mid = h - STATUS_BAR / 2.0;
    // Inside the chips, below their words (there the card's shadow over the bar is lightest).
    let line = mid + CHIP / 2.0 - 1.5;
    let inset = chip_corner_inset();
    let ports = [3000, 5173, 8080, 9000, 9229];
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let ui = theme.ui();
        // Where the chips are, from a first shot with the pointer away.
        let build = move |window: &mut Window, cx: &mut App| -> AnyView {
            cx.set_global(sidebar_appearance(theme, cx));
            cx.new(|cx| bar_workspace(0, SIDEBAR_DEFAULT_WIDTH, (FUXX, "main"), &ports, window, cx))
                .into()
        };
        let plain = render(&theme, dims, build);
        let chips = runs_over(&plain, dims, line, ui.row_hover, Some(-4.0));
        println!("ui-shots: status-bar-ports-{slug}: chips {chips:?}");
        assert_eq!(chips.len(), 4, "{slug}: three chips and +2: {chips:?}");
        let last = chips[3].1;
        assert!(
            (last + inset - (w - PAD)).abs() <= 1.0,
            "{slug}: the chips end at the bar's right end: {last}"
        );
        for pair in chips.windows(2) {
            let gap = pair[1].0 - pair[0].1 - 2.0 * inset;
            assert!((gap - CHIP_GAP).abs() <= 1.5, "{slug}: {gap} between chips");
        }
        let first = chips[0];
        let on = point(px((first.0 + first.1) / 2.0), px(mid));
        for scene in ["status-bar-ports", "status-bar-ports-click"] {
            let n = format!("{scene}-{slug}");
            let slot: Slot = Rc::new(RefCell::new(None));
            let keep = slot.clone();
            let build = move |window: &mut Window, cx: &mut App| -> AnyView {
                cx.set_global(sidebar_appearance(theme, cx));
                let ws = cx.new(|cx| {
                    bar_workspace(0, SIDEBAR_DEFAULT_WIDTH, (FUXX, "main"), &ports, window, cx)
                });
                *keep.borrow_mut() = Some(ws.clone());
                ws.into()
            };
            let act = {
                let n = n.clone();
                move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                    let ws = slotted(&slot);
                    cx.simulate_mouse_move(window, on, None, Modifiers::none());
                    if scene == "status-bar-ports-click" {
                        cx.simulate_click(window, on, Modifiers::none());
                        cx.run_until_parked();
                        let (urls, focused) = cx.read(|cx| {
                            let ws = ws.read(cx);
                            let ws_id = ws.model.focused_workspace().expect("a workspace");
                            let urls: Vec<Option<String>> = ws
                                .model
                                .tabs_of(ws_id)
                                .into_iter()
                                .filter_map(|t| match &ws.model.tab(t)?.kind {
                                    TabKind::Browser { url } => Some(url.clone()),
                                    _ => None,
                                })
                                .collect();
                            let focused = ws
                                .model
                                .focused_tab()
                                .and_then(|t| ws.model.tab(t))
                                .map(|t| matches!(t.kind, TabKind::Browser { .. }));
                            (urls, focused)
                        });
                        assert_eq!(
                            urls,
                            [Some("http://localhost:3000".to_string())],
                            "{n}: one browser tab, at the port"
                        );
                        assert_eq!(focused, Some(true), "{n}: shown and focused");
                        let facts = cx.read(|cx| ws.read(cx).status_bar_facts());
                        assert_eq!(
                            facts.map(|f| f.ports),
                            Some(ports.to_vec()),
                            "{n}: the bar keeps claude's ports"
                        );
                    }
                    slot.borrow_mut().take();
                }
            };
            let shot = render_with(&theme, dims, build, act);
            save_shot(scene, slug, &shot);
            if scene == "status-bar-ports" {
                let hovered = pixel_at(&shot, dims, first.0 + 2.0, line);
                assert!(
                    near_by(hovered, ui.row_active, 3),
                    "{n}: the hovered chip is row_active: {hovered:?}"
                );
                let other = pixel_at(&shot, dims, chips[1].0 + 2.0, line);
                assert!(near_by(other, ui.row_hover, 3), "{n}: the others aren't");
            }
        }
    }
}

/// The status bar short of room (Review Focus 4), in both themes: a 720 pt window, the
/// narrowest sidebar, a 40-character branch, a deep folder, 6 ports and an update pending.
///
/// Panics unless the update pill is whole at the bar's right end with its short label
/// ("Update Available", the version dropped), the ports are one `6 ports` chip, and the left
/// side's ink stops at least `ITEM_GAP` short of the right side (nothing overlaps).
fn status_bar_narrow_scenes() {
    use crate::status_bar::{
        ITEM_GAP, PAD, PILL, PILL_GAP, PILL_ICON, PILL_PAD, UPDATE_FILL, text_width,
    };
    use crate::ui_tokens::size::STATUS_BAR;
    use crate::updater::Updates;
    use gpui::FontWeight;
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    let dims = size(px(720.0), px(480.0));
    let (w, h) = (f32::from(dims.width), f32::from(dims.height));
    let (top, mid) = (h - STATUS_BAR, h - STATUS_BAR / 2.0);
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let ui = theme.ui();
        let n = format!("status-bar-narrow-{slug}");
        let slot: Slot = Rc::new(RefCell::new(None));
        let keep = slot.clone();
        let build = move |window: &mut Window, cx: &mut App| -> AnyView {
            cx.set_global(sidebar_appearance(theme, cx));
            cx.set_global(Updates {
                state: crate::updater::UpdateState::Available(shot_release()),
            });
            let ws = cx.new(|cx| narrow_bar_workspace(window, cx));
            *keep.borrow_mut() = Some(ws.clone());
            ws.into()
        };
        let label = Rc::new(Cell::new((0.0, 0.0)));
        let act = {
            let label = label.clone();
            move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                cx.update_window(window, |_, window, _| {
                    let medium = |t: &str| text_width(t, FontWeight::MEDIUM, false, window);
                    label.set((
                        medium("Update Available"),
                        medium("Update Available \u{b7} v0.3.2"),
                    ));
                })
                .expect("measure");
                slot.borrow_mut().take();
            }
        };
        let shot = render_with(&theme, dims, build, act);
        save_shot("status-bar-narrow", slug, &shot);
        let (short, long) = label.get();
        // Along its middle the icon and words cut the run: its first start, its last end.
        let pills = runs_of(&shot, dims, mid, UPDATE_FILL);
        assert!(!pills.is_empty(), "{n}: the update pill");
        let pill = (pills[0].0, pills[pills.len() - 1].1);
        let expected = 2.0 * PILL_PAD + PILL_ICON + PILL_GAP + short;
        println!("ui-shots: {n}: pill {pill:?}, expected {expected} wide (long {long})");
        assert!(
            (pill.1 - (w - PAD)).abs() <= 1.0,
            "{n}: the pill ends at the bar's right end: {pill:?}"
        );
        assert!(
            (pill.1 - pill.0 - expected).abs() <= 2.0,
            "{n}: the pill is whole, its version dropped: {} wide, expected {expected}",
            pill.1 - pill.0
        );
        // 16 pt tall, centred in the bar.
        let scale = shot.width as f32 / w;
        // Down the gap between its icon and its words.
        let x = pill.0 + PILL_PAD + PILL_ICON + PILL_GAP / 2.0;
        let rows = (0..(h * scale) as usize)
            .map(|dy| dy as f32 / scale)
            .filter(|&y| near_by(pixel_at(&shot, dims, x, y), UPDATE_FILL, 3))
            .count();
        assert_eq!(rows, (PILL * scale) as usize, "{n}: the pill is 16 pt tall");
        // The ports, one chip.
        let line = mid + crate::status_bar::CHIP / 2.0 - 1.5;
        let mut chips: Vec<(f32, f32)> = Vec::new();
        for run in runs_over(&shot, dims, line, ui.row_hover, Some(-4.0)) {
            // The `p`'s tail cuts the chip's fill at this height: its two sides are one chip.
            match chips.last_mut() {
                Some(last) if run.0 - last.1 < 6.0 => last.1 = run.1,
                _ => chips.push(run),
            }
        }
        assert_eq!(chips.len(), 1, "{n}: one `6 ports` chip: {chips:?}");
        // Nothing overlaps: canvas from the left side's last ink to the right side's start.
        let right = chips[0].0 - chip_corner_inset();
        let ys: Vec<f32> = (0..((STATUS_BAR - 4.0) * scale) as usize)
            .map(|dy| top + 2.0 + dy as f32 / scale)
            .collect();
        let clear = |x: f32| {
            ys.iter().all(|&y| {
                let here = pixel_at(&shot, dims, x, y);
                !apart(here, pixel_at(&shot, dims, x - 1.0 / scale, y))
            })
        };
        // (From 1 pt before the chip: its own edge is a step in the pixels.)
        let mut x = right - 1.0;
        while x > 0.0 && clear(x) {
            x -= 1.0 / scale;
        }
        let gap = right - x - 1.0 / scale;
        println!("ui-shots: {n}: {gap} pt clear before the ports chip at {right}");
        assert!(
            gap >= ITEM_GAP - 2.0,
            "{n}: only {gap} pt between the left side and the right"
        );
        assert!(
            x > w / 3.0,
            "{n}: the left side shows (its ink ends at {x})"
        );
    }
}

/// The update pill at the status bar's right end (M11i; it was above the sidebar's footer), over
/// `switcher_workspace`, in Mocha (both themes with `--all-themes`), with v0.3.2 the newer
/// release:
/// - `update-idle`: nothing known: no pill;
/// - `update-available`: a newer release known: the blue pill, a white package icon, then
///   white "Update Available" and a fainter " · v0.3.2";
/// - `update-hover`: the pointer on it: the darker blue;
/// - `update-downloading`: "Downloading v0.3.2…", with the pointer on it and a click: still
///   the plain blue (no hover) and still downloading;
/// - `update-narrow`: a 720 pt window short of room (`narrow_bar_workspace`): the version left
///   out, "Update Available" whole.
///
/// Panics unless the pill shows exactly when there is something to say, and nowhere above the
/// bar (not over the sidebar's footer any more); is 16 pt tall, centred in the bar, ending
/// `PAD` from the window's right edge; filled `#1F6FEB` (`#1A5FCC` hovered while clickable);
/// its icon's ink is about 12 pt wide and 6 pt before the words, the pair `PILL_PAD` from each
/// end; the words' ink is as wide as the text it should say; and the version part is white at
/// 75% (never full white) while the words are full white.
fn update_button_scenes() {
    use crate::status_bar::{
        PAD, PILL, PILL_GAP, PILL_ICON, PILL_PAD, UPDATE_FILL, UPDATE_HOVER, UPDATE_TEXT,
        text_width,
    };
    use crate::ui_tokens::size::STATUS_BAR;
    use crate::update_release::Version;
    use crate::updater::{UpdateState, Updates};
    use gpui::{FontWeight, Modifiers};
    use std::cell::RefCell;
    use std::rc::Rc;

    let version = Version(0, 3, 2);
    let near = |p: [u8; 3], c| near_by(p, c, 3);
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        // The ink's width, per scene, to compare.
        let mut ink = Vec::new();
        for scene in [
            "update-idle",
            "update-available",
            "update-hover",
            "update-downloading",
            "update-narrow",
        ] {
            let narrow = scene == "update-narrow";
            let dims = if narrow {
                size(px(720.0), px(480.0))
            } else {
                size(px(760.0), px(420.0))
            };
            let (w, h) = (f32::from(dims.width), f32::from(dims.height));
            let (top, bottom) = (h - (STATUS_BAR + PILL) / 2.0, h - (STATUS_BAR - PILL) / 2.0);
            let mid = (top + bottom) / 2.0;
            let state = match scene {
                "update-idle" => UpdateState::Idle,
                "update-downloading" => UpdateState::Downloading(version),
                _ => UpdateState::Available(shot_release()),
            };
            // What it should say, all of it (`None`: no pill).
            let says: Option<&str> = match scene {
                "update-idle" => None,
                "update-downloading" => Some("Downloading v0.3.2\u{2026}"),
                "update-narrow" => Some("Update Available"),
                _ => Some("Update Available \u{b7} v0.3.2"),
            };
            let slot: Slot = Rc::new(RefCell::new(None));
            let keep = slot.clone();
            let build = move |window: &mut Window, cx: &mut App| -> AnyView {
                cx.set_global(sidebar_appearance(theme, cx));
                cx.set_global(Updates { state });
                let ws = cx.new(|cx| {
                    if narrow {
                        narrow_bar_workspace(window, cx)
                    } else {
                        switcher_workspace(window, cx)
                    }
                });
                *keep.borrow_mut() = Some(ws.clone());
                ws.into()
            };
            let n = format!("{scene}-{slug}");
            // The words' widths as the pill draws them: what it says, and "Update Available".
            let widths: Rc<RefCell<(f32, f32)>> = Rc::default();
            let act = {
                let n = n.clone();
                let widths = widths.clone();
                move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                    cx.update_window(window, |_, window, _| {
                        let measure = |t: &str| text_width(t, FontWeight::MEDIUM, false, window);
                        *widths.borrow_mut() =
                            (measure(says.unwrap_or("")), measure("Update Available"));
                    })
                    .expect("measure the words");
                    let (said, _) = *widths.borrow();
                    let pill_w = 2.0 * PILL_PAD + PILL_ICON + PILL_GAP + said;
                    let on = point(px(w - PAD - pill_w / 2.0), px(mid));
                    if matches!(scene, "update-hover" | "update-downloading") {
                        cx.simulate_mouse_move(window, on, None, Modifiers::none());
                    }
                    if scene == "update-downloading" {
                        cx.simulate_click(window, on, Modifiers::none());
                        cx.run_until_parked();
                        let state = cx.read(|cx| cx.global::<Updates>().state.clone());
                        assert_eq!(
                            state,
                            UpdateState::Downloading(version),
                            "{n}: the click is ignored"
                        );
                        // And the pointer is put back over it, for the still.
                        cx.simulate_mouse_move(window, on, None, Modifiers::none());
                    }
                    slot.borrow_mut().take();
                }
            };
            let shot = render_with(&theme, dims, build, act);
            save_shot(scene, slug, &shot);

            let scale = shot.width as f32 / w;
            let at = |x: f32, y: f32| pixel_at(&shot, dims, x, y);
            let blue = |p: [u8; 3]| near(p, UPDATE_FILL) || near(p, UPDATE_HOVER);
            // Nowhere above the bar: not over the sidebar's footer, not anywhere.
            let above = (0..((h - STATUS_BAR) * scale) as usize)
                .step_by(2)
                .any(|dy| {
                    (0..shot.width as usize)
                        .step_by(2)
                        .any(|dx| blue(at(dx as f32 / scale, dy as f32 / scale)))
                });
            assert!(!above, "{n}: blue above the status bar");
            let pills: Vec<(f32, f32)> = runs_of(&shot, dims, mid, UPDATE_FILL)
                .into_iter()
                .chain(runs_of(&shot, dims, mid, UPDATE_HOVER))
                .collect();
            let Some(says) = says else {
                assert!(pills.is_empty(), "{n}: no pill, but blue at {pills:?}");
                continue;
            };
            // Its ends along its middle (the icon and words cut the run: first start, last end).
            let left = pills.iter().map(|r| r.0).fold(f32::MAX, f32::min);
            let right = pills.iter().map(|r| r.1).fold(0.0, f32::max);
            println!("ui-shots: {n}: pill {left}..{right}");
            assert!(
                (right - (w - PAD)).abs() <= 1.0,
                "{n}: the pill ends {PAD} from the right edge: {right}"
            );
            // Down the gap between its icon and its words: one unbroken 16 pt run, centred in
            // the bar.
            let x = left + PILL_PAD + PILL_ICON + PILL_GAP / 2.0;
            let rows: Vec<f32> = (0..(h * scale) as usize)
                .map(|dy| dy as f32 / scale)
                .filter(|&y| blue(at(x, y)))
                .collect();
            assert!(!rows.is_empty(), "{n}: no pill");
            let (first, last) = (rows[0], rows[rows.len() - 1] + 1.0 / scale);
            assert!(
                (first - top).abs() <= 0.5 && (last - bottom).abs() <= 0.5,
                "{n}: the pill spans {first}..{last}, expected {top}..{bottom}"
            );
            assert_eq!(rows.len(), (PILL * scale) as usize, "{n}: one 16 pt run");
            let fill = if scene == "update-hover" {
                UPDATE_HOVER
            } else {
                UPDATE_FILL
            };
            assert!(
                near(at(x, mid), fill),
                "{n}: filled {fill:?}: {:?}",
                at(x, mid)
            );
            // The ink: any pixel far brighter than the blue (red over 128; the fills' is 0x1F
            // or 0x1A). Looked for inside the rounded ends, where the canvas doesn't show.
            let ys: Vec<f32> = (0..((PILL - 4.0) * scale) as usize)
                .map(|dy| top + 2.0 + dy as f32 / scale)
                .collect();
            let brightest = |x: f32| ys.iter().map(|&y| at(x, y)[0]).max().unwrap_or(0);
            let xs: Vec<f32> = (0..((right - left - 12.0) * scale) as usize)
                .map(|dx| left + 6.0 + dx as f32 / scale)
                .collect();
            let inked: Vec<f32> = xs.iter().copied().filter(|&x| brightest(x) > 128).collect();
            assert!(!inked.is_empty(), "{n}: no icon or words");
            let step = 1.0 / scale;
            let (w0, w1) = (inked[0], inked[inked.len() - 1] + step);
            // The icon: the first stretch of ink, up to the first clear gap of 3 pt or more.
            let icon_end = inked
                .windows(2)
                .find(|w| w[1] - w[0] >= 3.0)
                .map(|w| w[0] + step)
                .expect("a gap between the icon and the words");
            let words_start = inked.iter().copied().find(|&x| x > icon_end).unwrap();
            let (icon, gap) = (icon_end - w0, words_start - icon_end);
            let (said, plain) = *widths.borrow();
            println!(
                "ui-shots: {n}: icon {icon} pt, gap {gap} pt, ink {} pt for {said} pt of words, \
                 {} from the left, {} from the right",
                w1 - w0,
                w0 - left,
                right - w1
            );
            assert!(
                (w0 - left - PILL_PAD).abs() <= 1.5,
                "{n}: the icon starts {} in, expected {PILL_PAD}",
                w0 - left
            );
            assert!(
                (right - (w0 + PILL_ICON + PILL_GAP + said) - PILL_PAD).abs() <= 2.0,
                "{n}: the words end short of {PILL_PAD} from the right"
            );
            // The icon set's package (2026-11-20) keeps a margin inside its box: its ink is
            // three quarters of it or more.
            assert!(
                (PILL_ICON * 0.75..=PILL_ICON + 1.0).contains(&icon),
                "{n}: the icon's ink is {icon} pt wide, its box {PILL_ICON}"
            );
            // The words' own side bearing adds a little to the gap, and the icon's margin
            // inside its box up to a point and a half more.
            assert!(
                (PILL_GAP - 0.5..=PILL_GAP + 3.0).contains(&gap),
                "{n}: {gap} pt between the icon and the words, expected {PILL_GAP}"
            );
            let expected = PILL_ICON + PILL_GAP + said;
            assert!(
                ((w1 - w0) - expected).abs() <= 2.0,
                "{n}: ink {} pt wide, expected {expected} for {says:?}",
                w1 - w0
            );
            let white = |p: [u8; 3]| near(p, UPDATE_TEXT);
            let any_white = |x0: f32, x1: f32| {
                xs.iter()
                    .filter(|&&x| x >= x0 && x < x1)
                    .any(|&x| ys.iter().any(|&y| white(at(x, y))))
            };
            assert!(any_white(w0, icon_end), "{n}: the icon is white");
            assert!(
                any_white(words_start, words_start + plain),
                "{n}: the words are white"
            );
            if scene != "update-narrow" && scene != "update-downloading" {
                // The version part, after "Update Available": white at 75% over the blue, never
                // full white, but plainly drawn.
                let from = words_start + plain + 1.0;
                let faint: Vec<u8> = xs
                    .iter()
                    .filter(|&&x| x >= from && x < w1)
                    .map(|&x| brightest(x))
                    .collect();
                let most = faint.iter().copied().max().unwrap_or(0);
                println!("ui-shots: {n}: the version's brightest red {most}");
                assert!(
                    (150..=225).contains(&most),
                    "{n}: the version part's brightest red is {most}, expected about 199 (75% white)"
                );
            }
            ink.push((scene, w1 - w0));
        }
        let wide = |s: &str| ink.iter().find(|(n, _)| *n == s).map(|(_, w)| *w).unwrap();
        assert!(
            wide("update-narrow") + 20.0 < wide("update-available"),
            "{slug}: the narrow pill left its version out: {ink:?}"
        );
    }
}
