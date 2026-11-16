//! The window as a whole: the harness smoke test, the design tokens, the window frame, the status
//! marks, the pane cards, and the title area and title bar.

use super::panes::quiet_line;
use super::sidebar::CLAUDE_QUESTION;
use super::*;

/// This area's scenes (`mod.rs` runs them in its `ORDER`).
pub(super) const SCENES: &[Scene] = &[
    Scene {
        name: "smoke",
        run: smoke,
        themes: Themes::One,
        serial: false,
        kind: Kind::Visual,
        sources: &[],
    },
    Scene {
        name: "tokens",
        run: tokens,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &["src/theme.rs", "src/ui_tokens.rs"],
    },
    Scene {
        name: "window",
        run: window_scenes,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/workspace.rs",
            "src/panes.rs",
            "src/terminal_view.rs",
            "src/sidebar.rs",
            "src/tab_bar.rs",
            "src/status_bar.rs",
        ],
    },
    Scene {
        name: "marks",
        run: marks,
        themes: Themes::One,
        serial: false,
        kind: Kind::Visual,
        sources: &["src/status_mark.rs"],
    },
    Scene {
        name: "title-area",
        run: title_area_scenes,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/title_area.rs",
            "src/notification_list.rs",
            "src/sidebar.rs",
            "src/sidebar_toggle.rs",
            "src/tab_menu.rs",
        ],
    },
    Scene {
        name: "title-bar",
        run: title_bar_scenes,
        themes: Themes::Both,
        serial: true,
        kind: Kind::Visual,
        sources: &[
            "src/title_area.rs",
            "src/sidebar_toggle.rs",
            "src/notification_list.rs",
            "src/sidebar.rs",
            "src/right_sidebar.rs",
        ],
    },
    Scene {
        name: "cards",
        run: card_scenes,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/workspace.rs",
            "src/panes.rs",
            "src/right_sidebar.rs",
            "src/sidebar.rs",
        ],
    },
    Scene {
        name: "cards-split",
        run: card_split_scenes,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &["src/panes.rs", "src/split_tree.rs", "src/workspace.rs"],
    },
    Scene {
        name: "cards-corners",
        run: card_corner_scenes,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &["src/panes.rs", "src/terminal_view.rs"],
    },
];

/// A themed panel standing in for a real scene: proves the harness end to end.
fn smoke() {
    let dims = size(px(320.0), px(200.0));
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let ui = theme.ui();
        let scene = |_: &mut Window, cx: &mut App| -> AnyView { cx.new(|_| Panel(ui)).into() };
        let path = shoot("smoke", slug, &theme, dims, scene);
        assert!(path.ends_with(format!("smoke-{slug}.png")));
        // The shot as kept in memory; with `--png`, the file written from it.
        let bytes = last_png();
        if options().png {
            assert!(
                std::fs::read(&path).expect("the PNG exists") == bytes,
                "{slug}: the PNG file"
            );
        }
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
        let w = u32::from_be_bytes(bytes[16..20].try_into().unwrap());
        let h = u32::from_be_bytes(bytes[20..24].try_into().unwrap());
        // The capture is in device pixels: a whole multiple of the point size.
        assert!(w >= 320 && w % 320 == 0, "width {w}");
        assert_eq!(h * 320, w * 200, "aspect ratio");

        let shot = render(&theme, dims, |_, cx| cx.new(|_| Panel(ui)).into());
        let first = &shot.rgba[..4];
        assert!(
            shot.rgba.chunks(4).any(|px| px != first),
            "{slug}: the shot is one flat colour"
        );
    }
    // The runner's check of a failing scene: `UI_SHOTS_FAIL_DEMO=1` fails this one on purpose.
    assert!(
        !std::env::var("UI_SHOTS_FAIL_DEMO").is_ok_and(|v| v == "1"),
        "smoke: failing on purpose (UI_SHOTS_FAIL_DEMO=1)"
    );
}

/// The token swatch board, one PNG per built-in theme and the default theme: every `UiColors`
/// token as a chip with its weakest drawn contrast, a mock sidebar (active, hovered, waiting,
/// done and idle rows), an overlay panel and pane dividers. Panics if any drawn pair misses its
/// floor.
fn tokens() {
    let mut themes: Vec<(String, String, Theme)> = theme::BUILTIN_THEMES
        .iter()
        .map(|&name| {
            let slug = name
                .to_lowercase()
                .replace("catppuccin ", "")
                .replace(' ', "-");
            (slug, name.to_string(), shot_theme(name))
        })
        .collect();
    themes.push(("default".into(), "Default".into(), Theme::default()));
    for (slug, label, theme) in themes {
        for p in theme.drawn_pairs() {
            let ratio = contrast(p.fg, p.bg);
            assert!(
                ratio >= p.min,
                "{label}: {} on {} is {ratio:.2}, needs {}",
                p.fg_name,
                p.bg_name,
                p.min
            );
        }
        let path = shoot(
            "tokens",
            &slug,
            &theme,
            size(px(1020.0), px(600.0)),
            |_, cx| {
                let mono = mono_font(cx);
                cx.new(|_| TokenBoard { label, theme, mono }).into()
            },
        );
        println!("ui-shots: {}", path.display());
    }
}

struct TokenBoard {
    label: String,
    theme: Theme,
    mono: Font,
}

fn hex(c: alacritty_terminal::vte::ansi::Rgb) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)
}

impl Render for TokenBoard {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let ui = self.theme.ui();
        let mono = self.mono.clone();
        let pairs = self.theme.drawn_pairs();
        let tokens = [
            ("sidebar_background", ui.sidebar_background),
            ("row_hover", ui.row_hover),
            ("row_active", ui.row_active),
            ("waiting_tint", ui.waiting_tint),
            ("done_tint", ui.done_tint),
            ("panel", ui.panel),
            ("shadow", ui.shadow),
            ("text", ui.text),
            ("text_dim", ui.text_dim),
            ("text_faint", ui.text_faint),
            ("accent", ui.accent),
            ("focus_ring", ui.focus_ring),
            ("ring_idle", ui.ring_idle),
            ("ring_working", ui.ring_working),
            ("ring_waiting", ui.ring_waiting),
            ("ring_done", ui.ring_done),
            ("sidebar_border", ui.sidebar_border),
            ("divider", ui.divider),
            ("panel_border", ui.panel_border),
            ("selection", ui.selection),
            // M11i: cards, the quiet selection, the tab bar and the Files tree's guides.
            ("card", ui.card),
            ("card_border", ui.card_border),
            // The card's shadow, shown as it falls on the canvas.
            (
                "card_shadow_alpha",
                theme::mix(ui.sidebar_background, ui.shadow, ui.card_shadow_alpha),
            ),
            ("row_selected", ui.row_selected),
            ("card_hover", ui.card_hover),
            ("card_selected", ui.card_selected),
            ("tab_bar", ui.tab_bar),
            ("tab_hover", ui.tab_hover),
            ("guide", ui.guide),
        ];
        let chips = tokens.map(|(name, color)| {
            // The pair closest to its floor, if this token is drawn on something.
            let worst = pairs
                .iter()
                .filter(|p| p.fg_name == name)
                .map(|p| (contrast(p.fg, p.bg), p))
                .min_by(|a, b| (a.0 / a.1.min).total_cmp(&(b.0 / b.1.min)));
            let detail = match worst {
                _ if name == "card_shadow_alpha" => {
                    format!("{} at {:.2}", hex(ui.shadow), ui.card_shadow_alpha)
                }
                Some((ratio, p)) => {
                    format!("{} {ratio:.2} on {} ≥{}", hex(color), p.bg_name, p.min)
                }
                None => format!("{} surface", hex(color)),
            };
            div()
                .w(px(244.0))
                .h(px(34.0))
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(
                    div()
                        .flex_none()
                        .w(px(28.0))
                        .h(px(20.0))
                        .rounded(px(3.0))
                        .border_1()
                        .border_color(hsla(ui.panel_border))
                        .bg(hsla(color)),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .child(div().text_size(px(12.0)).child(name))
                        .child(
                            div()
                                .font(mono.clone())
                                .text_size(px(10.0))
                                .text_color(hsla(ui.text_dim))
                                .whitespace_nowrap()
                                .child(detail),
                        ),
                )
        });
        div()
            .flex()
            .size_full()
            .text_color(hsla(ui.text))
            .child(sidebar_mock(ui, &mono))
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .p(px(16.0))
                    .gap(px(14.0))
                    .child(
                        div()
                            .flex()
                            .gap(px(10.0))
                            .items_center()
                            .child(div().text_size(px(13.0)).child(self.label.clone()))
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(hsla(ui.text_faint))
                                    .child("UI tokens: each with its weakest drawn contrast"),
                            ),
                    )
                    .child(div().flex().flex_wrap().gap_x(px(8.0)).children(chips))
                    .child(
                        div()
                            .flex()
                            .gap(px(20.0))
                            .child(panel_mock(ui, &mono))
                            .child(dividers_mock(ui, self.theme.background)),
                    ),
            )
    }
}

/// A status mark in its still form: idle is a hollow ring, the rest solid dots.
fn mark(color: alacritty_terminal::vte::ansi::Rgb, hollow: bool) -> Div {
    let side = ui_tokens::size::STATUS_MARK;
    let dot = div().size(px(8.0)).rounded_full();
    let dot = if hollow {
        dot.border_1().border_color(hsla(color))
    } else {
        dot.bg(hsla(color))
    };
    div()
        .flex_none()
        .size(px(side))
        .flex()
        .items_center()
        .justify_center()
        .child(dot)
}

/// One sidebar row: optional fill and accent bar, mark, number, name, time, question line.
#[allow(clippy::too_many_arguments)]
fn row(
    ui: UiColors,
    mono: &Font,
    fill: Option<alacritty_terminal::vte::ansi::Rgb>,
    active: bool,
    status: Div,
    number: &'static str,
    name: &'static str,
    time: &'static str,
    question: Option<&'static str>,
) -> Div {
    use ui_tokens::{size, text};
    let meta = |s: &'static str| {
        div()
            .flex_none()
            .font(mono.clone())
            .text_size(px(ui_tokens::text::META))
            .text_color(hsla(ui.text_faint))
            .child(s)
    };
    let line = div()
        .h(px(size::SIDEBAR_ROW))
        .flex()
        .items_center()
        .gap(px(8.0))
        .child(status)
        .child(meta(number).w(px(10.0)))
        .child(
            div()
                .flex_1()
                .truncate()
                .text_size(px(text::NAME))
                .text_color(hsla(if active { ui.text } else { ui.text_dim }))
                .child(name),
        )
        .child(meta(time));
    let height = if question.is_some() {
        size::SIDEBAR_ROW_TWO
    } else {
        size::SIDEBAR_ROW
    };
    let mut r = div()
        .relative()
        .mx(px(6.0))
        .px(px(8.0))
        .h(px(height))
        .rounded(px(5.0))
        .flex()
        .flex_col()
        .child(line);
    if let Some(q) = question {
        r = r.child(
            div()
                .mt(px(-6.0))
                .pl(px(38.0))
                .truncate()
                .text_size(px(ui_tokens::text::META))
                .text_color(hsla(ui.text_faint))
                .child(q),
        );
    }
    if let Some(f) = fill {
        r = r.bg(hsla(f));
    }
    if active {
        r = r.child(
            div()
                .absolute()
                .left(px(0.0))
                .top(px(6.0))
                .bottom(px(6.0))
                .w(px(2.0))
                .rounded(px(1.0))
                .bg(hsla(ui.accent)),
        );
    }
    r
}

fn sidebar_mock(ui: UiColors, mono: &Font) -> Div {
    use ui_tokens::{size, text};
    div()
        .flex_none()
        .w(px(size::SIDEBAR_DEFAULT_WIDTH))
        .h_full()
        .bg(hsla(ui.sidebar_background))
        .border_r_1()
        .border_color(hsla(ui.sidebar_border))
        .pt(px(size::TOP_BAND))
        .flex()
        .flex_col()
        .gap(px(2.0))
        .child(
            div()
                .h(px(size::GROUP_HEADER))
                .px(px(14.0))
                .flex()
                .items_center()
                .gap(px(6.0))
                .text_size(px(text::GROUP_HEADER))
                .text_color(hsla(ui.text_faint))
                .child("AGENTS")
                .child(div().font(mono.clone()).child("5")),
        )
        .child(row(
            ui,
            mono,
            Some(ui.row_active),
            true,
            mark(ui.ring_working, false),
            "1",
            "claude · api refactor",
            "4m",
            None,
        ))
        .child(row(
            ui,
            mono,
            Some(ui.row_hover),
            false,
            mark(ui.ring_idle, true),
            "2",
            "shell (hovered)",
            "12s",
            None,
        ))
        .child(row(
            ui,
            mono,
            Some(ui.waiting_tint),
            false,
            mark(ui.ring_waiting, false),
            "3",
            "codex",
            "1m",
            Some("Do you want to make this edit to auth.rs?"),
        ))
        .child(row(
            ui,
            mono,
            Some(ui.done_tint),
            false,
            mark(ui.ring_done, false),
            "4",
            "tests",
            "32s",
            None,
        ))
        .child(row(
            ui,
            mono,
            None,
            false,
            mark(ui.ring_idle, true),
            "5",
            "╭∩╮",
            "2h",
            None,
        ))
}

/// An overlay: raised panel, its border and shadow, a focused filter box and two list rows.
fn panel_mock(ui: UiColors, mono: &Font) -> Div {
    div()
        .w(px(300.0))
        .p(px(10.0))
        .flex()
        .flex_col()
        .gap(px(6.0))
        .bg(hsla(ui.panel))
        .border_1()
        .border_color(hsla(ui.panel_border))
        .rounded(px(8.0))
        .shadow(vec![BoxShadow {
            color: Hsla {
                a: 0.35,
                ..hsla(ui.shadow)
            },
            offset: point(px(0.0), px(6.0)),
            blur_radius: px(18.0),
            spread_radius: px(0.0),
            inset: false,
        }])
        .child(
            div()
                .h(px(26.0))
                .px(px(8.0))
                .flex()
                .items_center()
                .rounded(px(4.0))
                .border_2()
                .border_color(hsla(ui.focus_ring))
                .text_size(px(13.0))
                .child("cod"),
        )
        .child(
            row(
                ui,
                mono,
                Some(ui.row_active),
                true,
                mark(ui.ring_waiting, false),
                "3",
                "codex",
                "1m",
                None,
            )
            .mx(px(0.0)),
        )
        .child(
            row(
                ui,
                mono,
                None,
                false,
                mark(ui.ring_done, false),
                "4",
                "tests",
                "32s",
                None,
            )
            .mx(px(0.0)),
        )
}

/// Three panes on the terminal background: a quiet 1 px divider and a 2 px accent one (hover).
fn dividers_mock(ui: UiColors, terminal: alacritty_terminal::vte::ansi::Rgb) -> Div {
    use ui_tokens::size;
    let pane = |label: &'static str| {
        div()
            .flex_1()
            .p(px(8.0))
            .bg(hsla(terminal))
            .text_size(px(11.0))
            .text_color(hsla(ui.text_faint))
            .child(label)
    };
    div()
        .w(px(320.0))
        .h(px(130.0))
        .flex()
        .border_1()
        .border_color(hsla(ui.panel_border))
        .child(pane("divider"))
        .child(div().w(px(size::DIVIDER)).h_full().bg(hsla(ui.divider)))
        .child(pane("hovered"))
        .child(
            div()
                .w(px(size::DIVIDER_ACTIVE))
                .h_full()
                .bg(hsla(ui.accent)),
        )
        .child(pane("pane"))
}

/// The whole window, as the app draws it, from a real `Workspace` with plain sessions (each a
/// fixed screen over a silent `cat`, so the shots don't depend on your shell): one session; the
/// same split in two; and one session with the sidebar hidden. Checks that the terminal's
/// padding is drawn in the terminal background (nothing painted in it) inside its card, and
/// the top band and the gaps in the canvas (M11i). Off-screen windows have no traffic lights,
/// so the band is empty in the shots.
fn window_scenes() {
    let dims = size(px(960.0), px(600.0));
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        for (scene, split, hidden) in [
            ("window", false, false),
            ("window-split", true, false),
            ("window-no-sidebar", false, true),
        ] {
            let build = move |window: &mut Window, cx: &mut App| -> AnyView {
                cx.set_global(shot_appearance(theme, cx));
                cx.new(|cx| {
                    let mut ws = Workspace::for_shots(window, cx);
                    let project = std::path::Path::new("/Users/me/code/api");
                    ws.open_shot_session("api", project, SHELL_TEXT, None, window, cx);
                    if split {
                        let axis = Some(Axis::Row);
                        ws.open_shot_session("claude", project, AGENT_TEXT, axis, window, cx);
                    }
                    ws.sidebar.hidden = hidden;
                    ws
                })
                .into()
            };
            let path = shoot(scene, slug, &theme, dims, build);
            let shot = render(&theme, dims, build);
            check_window(&shot, &theme, dims, split, hidden, scene, slug);
            println!("ui-shots: {}", path.display());
        }
    }
}

/// Panics unless the padding of the pane at the right (inside its card, M11i) is all terminal
/// background, the gaps round the cards (and with the sidebar hidden, the top band) are the
/// canvas, and the left pane's grid starts just inside its left padding.
fn check_window(
    shot: &Shot,
    theme: &Theme,
    dims: Size<Pixels>,
    split: bool,
    hidden: bool,
    scene: &str,
    slug: &str,
) {
    use ui_tokens::size::{CARD_RADIUS, FOCUS_RING, STATUS_BAR, TAB_BAR, TOP_BAND};

    let scale = shot.width as f32 / f32::from(dims.width);
    let bg = theme.background;
    let ui = theme.ui();
    let is_bg = |x: f32, y: f32| {
        let (x, y) = ((x * scale) as usize, (y * scale) as usize);
        let i = (y * shot.width as usize + x) * 4;
        let px = &shot.rgba[i..i + 3];
        px.iter()
            .zip([bg.r, bg.g, bg.b])
            .all(|(a, b)| a.abs_diff(b) <= 2)
    };
    let (w, h) = (f32::from(dims.width), f32::from(dims.height));
    let (pad_x, pad_y) = if split {
        ui_tokens::size::SPLIT_TERMINAL_PADDING
    } else {
        ui_tokens::size::TERMINAL_PADDING
    };
    // The cards start under the band (sidebar hidden) or the 6 pt strip, and end 6 pt short of
    // the window's right and the status bar (M11i); inside each card's 1 pt edge, its tab bar,
    // then its terminal.
    let card_top = if hidden { TOP_BAND } else { GAP };
    let top = card_top + FOCUS_RING + TAB_BAR;
    let (right, bottom) = (w - GAP - FOCUS_RING, h - STATUS_BAR - GAP - FOCUS_RING);
    // The right pane's right padding, down to where its card's corner starts to round.
    let mut y = top;
    while y < bottom - CARD_RADIUS {
        for dx in 0..pad_x as usize {
            assert!(
                is_bg(right - 1.0 - dx as f32, y),
                "{scene}-{slug}: right padding at y {y}"
            );
        }
        y += 1.0;
    }
    // Its top padding (right of a split's gap: the pane area starts after the sidebar, at most
    // 1/4 in), short of the corner.
    let mut x = w * 0.75;
    while x < right - CARD_RADIUS {
        for dy in 0..pad_y as usize {
            let y = top + dy as f32;
            assert!(is_bg(x, y), "{scene}-{slug}: top padding at ({x}, {y})");
        }
        x += 1.0;
    }
    // The canvas: right of the card and below it, and (hidden) the band above it.
    let canvas = |x: f32, y: f32| {
        let p = pixel_at(shot, dims, x, y);
        assert!(
            is_canvas(p, &ui),
            "{scene}-{slug}: canvas at ({x}, {y}): {p:?}"
        );
    };
    for y in [top + 10.0, h / 2.0, bottom - 20.0] {
        canvas(w - GAP / 2.0, y);
    }
    canvas(w * 0.75, h - STATUS_BAR - GAP / 2.0);
    canvas(w * 0.75, card_top / 2.0);
    if hidden {
        for x in [w * 0.75, w - 20.0] {
            canvas(x, 6.0);
            canvas(x, TOP_BAND - GAP / 2.0);
        }
    }
    // The left pane's left padding: where the terminal background starts (after the sidebar,
    // the gap and the card's edge), `pad_x` of background, then the grid's first column (its
    // prompt and output) within a cell and a half (the grid's leftover width is shared by both
    // sides).
    let mid = (top + h) / 2.0;
    let left = (0..w as usize)
        .map(|x| x as f32)
        .find(|&x| is_bg(x, mid))
        .expect("a terminal");
    let rows = (top + pad_y) as usize..(bottom - pad_y) as usize;
    for y in rows.clone() {
        for dx in 0..pad_x as usize {
            assert!(
                is_bg(left + dx as f32, y as f32),
                "{scene}-{slug}: left padding at y {y}"
            );
        }
    }
    let first_column = (pad_x as usize..pad_x as usize + 14)
        .any(|dx| rows.clone().any(|y| !is_bg(left + dx as f32, y as f32)));
    assert!(
        first_column,
        "{scene}-{slug}: no text just inside the left padding"
    );
}

/// Mark board layout, in points: margin, label column, header row and each (square) cell.
const BOARD_MARGIN: f32 = 16.0;

const BOARD_LABEL: f32 = 88.0;

const BOARD_HEADER: f32 = 40.0;

const BOARD_CELL: f32 = 48.0;

/// Where in its own cycle (a turn, a breath, the pop) each column is.
const BOARD_PHASES: [f32; 3] = [0.0, 0.33, 0.66];

/// The board's rows: (label, status, whether it's the pop).
const BOARD_ROWS: [(&str, Status, bool); 5] = [
    ("idle", Status::Idle, false),
    ("working", Status::Working, false),
    ("waiting", Status::Waiting, false),
    ("done", Status::Done, false),
    ("done pop", Status::Done, true),
];

/// Every status mark at three points of its cycle, at 12 px (1×) and 24 px (2×), on the sidebar:
/// `marks-<theme>` with motion on and `marks-still-<theme>` with it off. Panics unless the moving
/// marks (Working, Waiting, the pop) differ between columns with motion on and are identical
/// with it off.
fn marks() {
    let dims = size(
        px(2.0 * BOARD_MARGIN + BOARD_LABEL + 6.0 * BOARD_CELL),
        px(2.0 * BOARD_MARGIN + BOARD_HEADER + 5.0 * BOARD_CELL),
    );
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        for still in [false, true] {
            let scene = if still { "marks-still" } else { "marks" };
            let build = move |_: &mut Window, cx: &mut App| -> AnyView {
                cx.set_reduce_motion(still);
                let mono = mono_font(cx);
                cx.new(|_| MarkBoard {
                    ui: theme.ui(),
                    mono,
                })
                .into()
            };
            let path = shoot(scene, slug, &theme, dims, build);
            let shot = render(&theme, dims, build);
            check_marks(&shot, dims, still, &format!("{scene}-{slug}"));
            println!("ui-shots: {}", path.display());
        }
    }
}

/// Panics unless each moving row's 1× and 2× cells change across the columns exactly when
/// motion is on.
fn check_marks(shot: &Shot, dims: Size<Pixels>, still: bool, name: &str) {
    let scale = shot.width as f32 / f32::from(dims.width);
    let cell = |row: usize, col: usize| -> Vec<u8> {
        let x0 = ((BOARD_MARGIN + BOARD_LABEL + col as f32 * BOARD_CELL) * scale) as usize;
        let y0 = ((BOARD_MARGIN + BOARD_HEADER + row as f32 * BOARD_CELL) * scale) as usize;
        let side = (BOARD_CELL * scale) as usize;
        let mut out = Vec::new();
        for y in y0..y0 + side {
            let i = (y * shot.width as usize + x0) * 4;
            out.extend_from_slice(&shot.rgba[i..i + side * 4]);
        }
        out
    };
    for (row, (label, status, pop)) in BOARD_ROWS.iter().enumerate() {
        let moves = *pop || matches!(status, Status::Working | Status::Waiting);
        for first in [0, 3] {
            let a = cell(row, first);
            assert!(
                a.chunks(4).any(|p| p != &a[..4]),
                "{name}: {label} is blank"
            );
            let differs = cell(row, first + 1) != a || cell(row, first + 2) != a;
            assert_eq!(
                differs,
                moves && !still,
                "{name}: {label} columns {first}.."
            );
        }
    }
}

struct MarkBoard {
    ui: UiColors,
    mono: Font,
}

impl Render for MarkBoard {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let ui = self.ui;
        let text = |t: String| {
            div()
                .font(self.mono.clone())
                .text_size(px(11.0))
                .text_color(hsla(ui.text_dim))
                .child(t)
        };
        // A line naming each size (12 px is 1×, 24 px what it is on a 2× screen), then the phases.
        let sizes = div()
            .flex()
            .h(px(BOARD_HEADER / 2.0))
            .child(div().w(px(BOARD_LABEL)))
            .children(["12 px (1×)", "24 px (2×)"].map(|t| {
                div()
                    .w(px(3.0 * BOARD_CELL))
                    .flex()
                    .justify_center()
                    .child(text(t.to_string()))
            }));
        let phases = div()
            .flex()
            .h(px(BOARD_HEADER / 2.0))
            .child(div().w(px(BOARD_LABEL)).child(text("phase".to_string())))
            .children((0..2).flat_map(|_| {
                BOARD_PHASES.map(|p| {
                    div()
                        .w(px(BOARD_CELL))
                        .flex()
                        .justify_center()
                        .child(text(format!("{p:.2}")))
                })
            }));
        let rows = BOARD_ROWS.map(|(label, status, pop)| {
            let cells = [12.0, 24.0].into_iter().flat_map(move |side| {
                BOARD_PHASES.map(move |own| {
                    // The shared clock at which this mark is `own` of the way through its cycle.
                    let phase = match status {
                        Status::Working => own / TURNS_PER_BEAT,
                        Status::Waiting => own / BREATHS_PER_BEAT,
                        _ => 0.0,
                    };
                    let popping = pop.then_some(own);
                    div()
                        .size(px(BOARD_CELL))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(status_mark::mark_on(
                            status,
                            &ui,
                            phase,
                            popping,
                            px(side),
                            ui.sidebar_background,
                        ))
                })
            });
            div()
                .flex()
                .items_center()
                .h(px(BOARD_CELL))
                .child(div().w(px(BOARD_LABEL)).child(text(label.to_string())))
                .children(cells)
        });
        div()
            .size_full()
            .p(px(BOARD_MARGIN))
            .bg(hsla(ui.sidebar_background))
            .child(sizes)
            .child(phases)
            .children(rows)
    }
}

/// A workspace for the card scenes: `panes` terminals showing `text`, each split to the right
/// of the one before (one: a lone pane), the first one focused; the sidebar shown (240 pt).
fn card_workspace(
    panes: usize,
    text: &str,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) -> Workspace {
    let mut ws = Workspace::for_shots(window, cx);
    ws.sidebar.hint_dismissed = true;
    let project = std::path::Path::new(API);
    for i in 0..panes {
        let axis = (i > 0).then_some(Axis::Row);
        ws.open_shot_session(&format!("pane{i}"), project, text, axis, window, cx);
    }
    ws.select(0, window, cx);
    ws
}

/// M11i floating panels, from a real `Workspace` in both themes (1280 × 600, the sidebar 240
/// wide):
/// - `cards`: one pane and the right sidebar open (300 wide): both are cards on the canvas.
/// - `cards-narrow`: the same in a 440 pt window, too narrow for the right sidebar's card.
///
/// Panics unless the gaps (between the sidebar and the pane card, the pane card and the right
/// sidebar's, the right card and the window's right edge, above and below the cards) show the
/// canvas, 20 pt into each card shows its fill, the left sidebar has no right border line, and
/// just outside the pane card's top-left curve is the canvas (no square tab corner pokes out);
/// in `cards-narrow`, unless the right card isn't drawn and the pane card ends 6 pt from the
/// window's right edge.
fn card_scenes() {
    use std::cell::RefCell;
    use std::rc::Rc;

    let dims = size(px(1280.0), px(600.0));
    let (left, right) = (240.0_f32, 1280.0_f32);
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let ui = theme.ui();
        let slot: Slot = Rc::new(RefCell::new(None));
        let keep = slot.clone();
        let build = move |window: &mut Window, cx: &mut App| -> AnyView {
            cx.set_global(shot_appearance(theme, cx));
            let ws = cx.new(|cx| {
                let mut ws = card_workspace(1, "", window, cx);
                ws.right_sidebar.hidden = false;
                ws
            });
            *keep.borrow_mut() = Some(ws.clone());
            ws.into()
        };
        let acting = slot.clone();
        let act = move |cx: &mut VisualTestAppContext, _: AnyWindowHandle| {
            let width = cx.read(|cx| slotted(&acting).read(cx).right_sidebar.width);
            assert_eq!(width, 300.0, "cards-{slug}: the right sidebar is 300 wide");
            acting.borrow_mut().take();
        };
        let shot = render_with(&theme, dims, build, act);
        let path = save_shot("cards", slug, &shot);
        let n = format!("cards-{slug}");
        let at = |x: f32, y: f32| pixel_at(&shot, dims, x, y);
        // The right card spans 974–1274: 6 pt from the window's edge, 300 wide.
        let right_card = right - GAP - 300.0;
        let pane_card_right = right_card - GAP;
        for (what, x, y) in [
            ("sidebar → pane card", left + GAP / 2.0, 100.0),
            ("sidebar → pane card, low", left + GAP / 2.0, 500.0),
            ("pane card → right card", pane_card_right + GAP / 2.0, 400.0),
            ("right card → window edge", right - GAP / 2.0, 400.0),
            ("above the pane card", 600.0, GAP / 2.0),
            ("above the right card", 1100.0, GAP / 2.0),
            ("below the pane card", 600.0, 600.0 - GAP / 2.0),
            ("below the right card", 1100.0, 600.0 - GAP / 2.0),
        ] {
            let p = at(x, y);
            assert!(is_canvas(p, &ui), "{n}: canvas {what} at ({x}, {y}): {p:?}");
        }
        for (what, x, y) in [
            ("pane card", left + GAP + 20.0, 100.0),
            ("pane card, right end", pane_card_right - 20.0, 400.0),
            ("right card", right_card + 20.0, 400.0),
            ("right card, right end", right - GAP - 20.0, 400.0),
        ] {
            let p = at(x, y);
            assert!(near(p, ui.card), "{n}: {what} at ({x}, {y}): {p:?}");
        }
        // No line down the sidebar's right edge: its last column is its own colour.
        let p = at(left - 0.5, 300.0);
        assert!(
            near(p, ui.sidebar_background),
            "{n}: no sidebar border: {p:?}"
        );
        // Nothing square pokes out of the pane card's top-left corner (Task 4 review, M1): just
        // outside its curve, where a square first tab's corner (its fill, a top line) would
        // be, the canvas shows, not the tab or the card.
        let (card_left, card_top) = (left + GAP, GAP);
        for (dx, dy) in [(1.0, 1.0), (1.5, 1.5), (1.0, 2.0), (2.0, 1.0)] {
            let (x, y) = (card_left + dx, card_top + dy);
            let p = at(x, y);
            assert!(
                is_canvas(p, &ui) && gap(p, ui.sidebar_background) < gap(p, ui.tab_bar),
                "{n}: canvas outside the card's top-left curve at ({x}, {y}): {p:?}"
            );
        }
        // The first tab's own top-left corner is rounded (Task 5 review, Minor 1): just inside
        // the bar, 1.5 pt into the tab's 6 pt corner (the tab starts at the card's 1 pt edge,
        // 4 pt under the bar's top), the bar shows, not the active tab's card fill.
        let (x, y) = (card_left + 2.5, card_top + 6.5);
        let p = at(x, y);
        assert!(
            gap(p, ui.tab_bar) < gap(p, ui.card),
            "{n}: the bar inside the first tab's rounded corner at ({x}, {y}): {p:?}"
        );
        println!("ui-shots: {}", path.display());

        // A window too narrow for the right sidebar beside the panes (its drawn width 0): no
        // card of it at all (not a 2 pt stub), and the pane card reaches the window's right
        // gap (Task 4 review, M7).
        let narrow = size(px(440.0), px(300.0));
        let build = move |window: &mut Window, cx: &mut App| -> AnyView {
            cx.set_global(shot_appearance(theme, cx));
            cx.new(|cx| {
                let mut ws = card_workspace(1, "", window, cx);
                ws.right_sidebar.hidden = false;
                ws
            })
            .into()
        };
        let shot = render_with(&theme, narrow, build, |_, _| ());
        let path = save_shot("cards-narrow", slug, &shot);
        let n = format!("cards-narrow-{slug}");
        let at = |x: f32, y: f32| pixel_at(&shot, narrow, x, y);
        let edge = 440.0 - GAP - 0.5;
        let p = at(edge, 150.0);
        assert!(
            near(p, ui.card_border),
            "{n}: the pane card's right edge at {edge}: {p:?}"
        );
        for x in [440.0 - GAP / 2.0, 440.0 - 1.0] {
            let p = at(x, 150.0);
            assert!(is_canvas(p, &ui), "{n}: canvas right of it at {x}: {p:?}");
        }
        println!("ui-shots: {}", path.display());
    }
}

/// M11i floating panels, a split, in both themes (960 × 600, the sidebar 240 wide; the cards
/// span 246–954 and 6–594, each split side 351 wide with the 6 pt gap between, 597–603):
/// - `cards-split`: two panes, the left one focused: its card's edge is `accent` (the focus
///   ring), the other's `card_border`; the gap between them is the canvas.
/// - `cards-split-hover`: the pointer over the gap: the 2 pt accent line in its middle.
/// - `cards-split-refocus`: a click in the right pane: the ring follows it.
fn card_split_scenes() {
    use gpui::Modifiers;
    use std::cell::RefCell;
    use std::rc::Rc;

    let dims = size(px(960.0), px(600.0));
    let (left_card, right_end) = (240.0 + GAP, 960.0 - GAP);
    let middle = (left_card + right_end) / 2.0;
    let (first_right, second_left) = (middle - GAP / 2.0, middle + GAP / 2.0);
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let ui = theme.ui();
        for scene in ["cards-split", "cards-split-hover", "cards-split-refocus"] {
            let slot: Slot = Rc::new(RefCell::new(None));
            let keep = slot.clone();
            let build = move |window: &mut Window, cx: &mut App| -> AnyView {
                cx.set_global(shot_appearance(theme, cx));
                let ws = cx.new(|cx| card_workspace(2, "", window, cx));
                *keep.borrow_mut() = Some(ws.clone());
                ws.into()
            };
            let acting = slot.clone();
            let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let ws = slotted(&acting);
                let panes = shown_panes(cx, &ws);
                assert_eq!(panes.len(), 2, "{scene}-{slug}: two panes");
                let none = Modifiers::none();
                match scene {
                    "cards-split-hover" => {
                        let on = point(px(middle), px(300.0));
                        cx.simulate_mouse_move(window, on, None, none);
                    }
                    "cards-split-refocus" => {
                        let on = point(px(second_left + 100.0), px(300.0));
                        cx.simulate_click(window, on, none);
                        let focused = cx.read(|cx| ws.read(cx).model.focused_pane());
                        assert_eq!(focused, Some(panes[1]), "{scene}-{slug}: focused");
                    }
                    _ => {}
                }
                acting.borrow_mut().take();
            };
            let shot = render_with(&theme, dims, build, act);
            let path = save_shot(scene, slug, &shot);
            let n = format!("{scene}-{slug}");
            let at = |x: f32, y: f32| pixel_at(&shot, dims, x, y);
            let p = at(middle, 450.0);
            if scene == "cards-split-hover" {
                let scale = shot.width as f32 / 960.0;
                // In the gap only (the focused card's edge beside it is the accent too).
                // The quiet line (M11x: not the accent), 2 pt.
                let line = quiet_line(at(first_right + 1.0, 450.0), &ui);
                let quiet = count_along(&shot, dims, line, 450.0, first_right, second_left);
                assert_eq!(quiet, (2.0 * scale) as usize, "{n}: the 2 pt quiet line");
                let accent = count_along(&shot, dims, ui.accent, 450.0, first_right, second_left);
                assert_eq!(accent, 0, "{n}: the line isn't the accent");
                // In its middle: 2 pt of canvas either side.
                for x in [first_right + 1.0, second_left - 1.0] {
                    let p = at(x, 450.0);
                    assert!(
                        is_canvas(p, &ui),
                        "{n}: canvas beside the line at {x}: {p:?}"
                    );
                }
            } else {
                assert!(is_canvas(p, &ui), "{n}: the gap is the canvas: {p:?}");
                let accent = count_along(&shot, dims, ui.accent, 450.0, first_right, second_left);
                assert_eq!(accent, 0, "{n}: no accent line in a quiet gap");
            }
            // Each card's outer edges, half a point in: the focused one's in `accent`.
            let first_focused = scene != "cards-split-refocus";
            for (which, x, focused) in [
                ("left card's left edge", left_card + 0.5, first_focused),
                ("left card's right edge", first_right - 0.5, first_focused),
                ("right card's left edge", second_left + 0.5, !first_focused),
                ("right card's right edge", right_end - 0.5, !first_focused),
            ] {
                let p = at(x, 300.0);
                let want = if focused { ui.accent } else { ui.card_border };
                assert!(near(p, want), "{n}: {which} ({focused}): {p:?}");
            }
            println!("ui-shots: {}", path.display());
        }
    }
}

/// M11i Review Focus 1, in both themes (960 × 600, the sidebar 240 wide): `cards-corners`, a
/// lone pane whose program filled the screen with a blue background (`ESC [44m`, then clear).
///
/// Panics unless the pixel 2 pt inside each of the card's bottom corners is the canvas (the
/// terminal's background and its cells stay inside the rounded corner), the grid is blue, and
/// the lone card's edges are `card_border` with no focus ring.
fn card_corner_scenes() {
    use std::cell::RefCell;
    use std::rc::Rc;

    let dims = size(px(960.0), px(600.0));
    // The card ends 6 pt above the status bar (M11i).
    let bar = ui_tokens::size::STATUS_BAR;
    let (left, right, bottom) = (240.0 + GAP, 960.0 - GAP, 600.0 - bar - GAP);
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let ui = theme.ui();
        let blue = theme.palette[4];
        let slot: Slot = Rc::new(RefCell::new(None));
        let keep = slot.clone();
        let build = move |window: &mut Window, cx: &mut App| -> AnyView {
            cx.set_global(shot_appearance(theme, cx));
            let ws = cx.new(|cx| card_workspace(1, "", window, cx));
            *keep.borrow_mut() = Some(ws.clone());
            ws.into()
        };
        // Once the terminal has its size in the card: blue, then clear (every cell of the
        // grid takes the blue, as a full-screen program's background does).
        let act = move |cx: &mut VisualTestAppContext, _: AnyWindowHandle| {
            let ws = slotted(&slot);
            let view = cx.read(|cx| ws.read(cx).sessions.items()[0].view.clone());
            cx.read(|cx| view.read(cx).show_text("\x1b[44m\x1b[2J"));
            cx.update(|cx| view.update(cx, |_, cx| cx.notify()));
            slot.borrow_mut().take();
        };
        let shot = render_with(&theme, dims, build, act);
        let path = save_shot("cards-corners", slug, &shot);
        let n = format!("cards-corners-{slug}");
        let at = |x: f32, y: f32| pixel_at(&shot, dims, x, y);
        let p = at(600.0, 300.0);
        assert!(near(p, blue), "{n}: the grid is blue: {p:?}");
        for (which, x, y) in [
            ("bottom left", left + 2.0, bottom - 2.0),
            ("bottom right", right - 2.0, bottom - 2.0),
        ] {
            let p = at(x, y);
            assert!(
                is_canvas(p, &ui),
                "{n}: {which} corner is the canvas: {p:?}"
            );
            assert!(!near(p, blue), "{n}: {which} corner isn't blue: {p:?}");
        }
        for (which, x, y) in [
            ("left", left + 0.5, 300.0),
            ("right", right - 0.5, 300.0),
            ("bottom", 600.0, bottom - 0.5),
        ] {
            let p = at(x, y);
            assert!(
                !near(p, ui.accent),
                "{n}: no ring on a lone pane, {which}: {p:?}"
            );
            assert!(near(p, ui.card_border), "{n}: {which} edge: {p:?}");
        }
        println!("ui-shots: {}", path.display());
    }
}

/// A long question, so an entry's second line is cut.
const LONG_QUESTION: &str = "Do you want to run cargo test --workspace --all-features -- \
    --include-ignored --test-threads=1 in /Users/me/code/web before the migration?";

/// The notifications fuxx would have raised over `switcher_workspace`, oldest first: a script's
/// `fuxx notify` from outside fuxx's terminals (50 m ago), build finished (40 m), a terminal since closed (`s99`, 25 m), tests finished (12 m), claude
/// waits (4 m) and agent waits with a long question (95 s).
pub(super) fn shot_notifications(ws: &mut Workspace) {
    use crate::notification_list::Entry;
    use std::time::{Duration, Instant};

    let now = Instant::now();
    let raised = |ws: &Workspace, name: &str, status, message: &str, secs| {
        let i = ws.sessions.position(|s| s.label() == name).expect(name);
        let workspace = ws.workspace_of(i).expect("in a workspace");
        Entry {
            terminal: Some(ws.sessions.items()[i].id),
            title: ws.workspace_label(workspace).to_string(),
            message: message.into(),
            status: Some(status),
            at: now - Duration::from_secs(secs),
        }
    };
    let entries = [
        Entry::scripted(
            "deploy.sh",
            "Staging is live at https://staging.example.com",
            None,
            |_| true,
            now - Duration::from_secs(50 * 60),
        ),
        raised(ws, "build", Status::Done, "Worked for 3m", 40 * 60),
        Entry {
            terminal: Some(99),
            title: "migrate".into(),
            message: "Worked for 18m".into(),
            status: Some(Status::Done),
            at: now - Duration::from_secs(25 * 60),
        },
        raised(ws, "tests", Status::Done, "Worked for 12m", 12 * 60),
        raised(ws, "claude", Status::Waiting, CLAUDE_QUESTION, 4 * 60),
        raised(ws, "agent", Status::Waiting, LONG_QUESTION, 95),
    ];
    for entry in entries {
        ws.note_notification(entry);
    }
}

/// A workspace name too long for a narrow window's band (`title-area`, `title-bar`).
const LONG_NAME: &str = "Refactor the auth middleware to use the new session store";

/// The title area over `switcher_workspace` (shell active), in both themes:
/// - `title-band`: the sidebar shown, its top band holding the title and the sidebar button
///   (M11m, checked in `title-bar`), and the footer's +, 🔔 and gear, nothing unseen;
/// - `title-hover`: the same with the pointer on the footer's 🔔 (its fill, its outline brighter);
/// - `title-unseen`: the same with notifications unseen (the 🔔's dot);
/// - `title-footer-narrow`: the narrowest sidebar (180 px) with the dot: the three buttons fit;
/// - `title-hidden`: the sidebar hidden: the band runs over the panes with the workspace's name
///   centred on the window, and no footer, so no 🔔 there (the band's own 🔔, ⚙ and sidebar
///   button at its right are checked in `title-bar`);
/// - `title-narrow`: hidden, a 560 px window and a long name, cut with an ellipsis;
/// - `title-popover`: the popover open upward from the 🔔 over six notifications, newest first
///   (agent's long question cut, a closed terminal's entry greyed, a script's last with its own
///   title);
/// - `title-popover-empty`: the popover with none yet.
///
/// Panics unless the dot shows exactly while some are unseen (and never with the sidebar
/// hidden), the band (sidebar hidden) holds no 🔔 or + where they were, opening the popover sees
/// them all (and it counts as an overlay), the name is drawn in the band only with the sidebar
/// hidden and never past its inset; and (Mocha) unless a press where the buttons were still arms
/// a window drag, a press on the 🔔 doesn't, a click on the 🔔 opens the popover and Escape or a
/// click outside closes it, a click on the + opens its menu, a click on a closed terminal's entry
/// (or a script's with no terminal) does nothing, a click on agent's entry shows agent (its
/// workspace and tab) with the keyboard, and hiding the sidebar closes the popover.
fn title_area_scenes() {
    use crate::sidebar_toggle::{BUTTON_Y, HIT, lights_clear};
    use crate::tab_menu::Dismiss;
    use crate::title_area::{
        DOT, DOT_INSET, FOOTER_BUTTON, band_buttons, entry_top, footer_buttons, name_inset,
        popover_anchor, popover_width,
    };
    use crate::ui_tokens::size::{SIDEBAR_DEFAULT_WIDTH, SIDEBAR_MIN_WIDTH, STATUS_BAR};
    use gpui::{Focusable, KeyBinding, Modifiers, MouseButton};
    use std::cell::RefCell;
    use std::rc::Rc;

    const FOOTER: f32 = 36.0;
    // Where the 🔔 and + sat in the band, right of the sidebar button's old place (x 80), before
    // they moved. With the sidebar shown the title is there now; hidden, the band is bare there.
    let (sx, sy) = (lights_clear(false), BUTTON_Y);
    let old_buttons = (sx + HIT + 2.0, sx + HIT + 2.0 + 2.0 * HIT + 2.0);
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let ui = theme.ui();
        let bg = |c: alacritty_terminal::vte::ansi::Rgb| [c.r, c.g, c.b];
        for scene in [
            "title-band",
            "title-hover",
            "title-unseen",
            "title-footer-narrow",
            "title-bare-narrow",
            "title-hidden",
            "title-narrow",
            "title-popover",
            "title-popover-empty",
            "title-popover-narrow",
        ] {
            let dims = match scene {
                "title-narrow" => size(px(560.0), px(200.0)),
                "title-footer-narrow" => size(px(360.0), px(200.0)),
                "title-popover" | "title-popover-empty" => size(px(960.0), px(480.0)),
                "title-popover-narrow" => size(px(360.0), px(480.0)),
                _ => size(px(760.0), px(200.0)),
            };
            let sidebar_width = if matches!(
                scene,
                "title-footer-narrow" | "title-bare-narrow" | "title-popover-narrow"
            ) {
                SIDEBAR_MIN_WIDTH
            } else {
                SIDEBAR_DEFAULT_WIDTH
            };
            let (height, width) = (f32::from(dims.height), f32::from(dims.width));
            let [plus_x, bell_x, gear_x] = footer_buttons(sidebar_width);
            // The footer's buttons sit centred in the footer under its 1 px edge.
            // The footer sits on the status bar (M11i).
            let centre_y = height - STATUS_BAR - FOOTER + 1.0 + (FOOTER - 1.0) / 2.0;
            let slot: Slot = Rc::new(RefCell::new(None));
            let keep = slot.clone();
            let build = move |window: &mut Window, cx: &mut App| -> AnyView {
                cx.set_global(sidebar_appearance(theme, cx));
                let ws = cx.new(|cx| {
                    let mut ws = switcher_workspace(window, cx);
                    ws.sidebar.hidden = matches!(scene, "title-hidden" | "title-narrow");
                    ws.sidebar.width = sidebar_width;
                    if scene != "title-popover-empty" {
                        shot_notifications(&mut ws);
                    }
                    if matches!(scene, "title-band" | "title-hover" | "title-bare-narrow") {
                        ws.notifications.mark_seen();
                    }
                    if scene == "title-narrow" {
                        let shown = ws.model.focused_workspace().expect("focused");
                        ws.model.set_name(shown, Some(LONG_NAME.into()));
                    }
                    ws
                });
                *keep.borrow_mut() = Some(ws.clone());
                ws.into()
            };
            let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let ws = slotted(&slot);
                let n = format!("{scene}-{slug}");
                if scene == "title-hover" {
                    let bell = point(px(bell_x + FOOTER_BUTTON / 2.0), px(centre_y));
                    cx.simulate_mouse_move(window, bell, None, gpui::Modifiers::none());
                }
                if scene.starts_with("title-popover") {
                    cx.update_window(window, |_, window, cx| {
                        ws.update(cx, |ws, cx| ws.open_notifications(window, cx))
                    })
                    .expect("open it");
                    cx.run_until_parked();
                    cx.update(|cx| {
                        let ws = ws.read(cx);
                        assert_eq!(ws.notifications.unseen(), 0, "{n}: all seen");
                        assert!(ws.overlays(cx).any(), "{n}: an overlay");
                    });
                }
                slot.borrow_mut().take();
            };
            let shot = render_with(&theme, dims, build, act);
            let path = keep_shot(&format!("{scene}-{slug}"), &shot);
            println!("ui-shots: {}", path.display());
            let at = |x: f32, y: f32| pixel_at(&shot, dims, x, y);
            let n = format!("{scene}-{slug}");
            let hidden = matches!(scene, "title-hidden" | "title-narrow");
            // Whether any pixel of colour `c` is in the 🔔's column, down the footer: its dome,
            // rim and clapper are on that line.
            let bell_has = |c: alacritty_terminal::vte::ansi::Rgb| -> bool {
                let x = bell_x + FOOTER_BUTTON / 2.0;
                (0..28).any(|k| at(x, centre_y - 14.0 + k as f32) == bg(c))
            };
            // The dot's middle: the 🔔's top right (its FOOTER_BUTTON-wide button, centred in the footer).
            let dot = (
                bell_x + FOOTER_BUTTON - DOT_INSET - DOT / 2.0,
                centre_y - FOOTER_BUTTON / 2.0 + DOT_INSET + DOT / 2.0,
            );
            if !hidden {
                match scene {
                    "title-band" | "title-bare-narrow" => {
                        assert_ne!(at(dot.0, dot.1), bg(ui.accent), "{n}: no dot");
                        assert!(bell_has(ui.text_dim), "{n}: the bell, dim");
                        // Each icon's drawn ink (pixels at least half-way from the footer's
                        // colour to the dim text colour): 14-16 px tall, the three centred on
                        // one line, each centred in its `footer_buttons` slot, and the + as far
                        // from the sidebar's left edge as the gear is from its right edge (no
                        // border there since M11i). Every device pixel of a button is read, so
                        // a box edge is the same at 1x and 2x (a point-by-point read rounds
                        // each side its own way, by up to a pixel).
                        let foot = bg(ui.sidebar_background);
                        let top = centre_y - FOOTER_BUTTON / 2.0;
                        let slot_box = |x0: f32| -> (f32, f32, f32, f32) {
                            let area = (x0, top, FOOTER_BUTTON, FOOTER_BUTTON);
                            ink_box(&shot, dims, area, foot, bg(ui.text_dim))
                                .unwrap_or_else(|| panic!("{n}: no ink in the slot at {x0}"))
                        };
                        let boxes = [slot_box(plus_x), slot_box(bell_x), slot_box(gear_x)];
                        println!("ui-shots: {n}: footer ink boxes (x0, x1, y0, y1) {boxes:?}");
                        for (name, b) in ["+", "bell", "gear"].iter().zip(boxes) {
                            let tall = b.3 - b.2;
                            assert!(
                                (14.0..=16.0).contains(&tall),
                                "{n}: the {name} is {tall} px tall"
                            );
                        }
                        let mids = boxes.map(|b| (b.2 + b.3) / 2.0);
                        for m in mids {
                            assert!(
                                (m - mids[0]).abs() <= 0.5,
                                "{n}: the icons' centres agree: {mids:?}"
                            );
                        }
                        // Where the model puts each button (the popover hangs from the gear's
                        // edge) is where it is drawn: the ink's centre is the slot's.
                        for ((name, slot), b) in ["+", "bell", "gear"]
                            .iter()
                            .zip([plus_x, bell_x, gear_x])
                            .zip(boxes)
                        {
                            let (mid, want) = ((b.0 + b.1) / 2.0, slot + FOOTER_BUTTON / 2.0);
                            assert!(
                                (mid - want).abs() <= 0.5,
                                "{n}: the {name}'s ink is centred at {mid}, its slot at {want}"
                            );
                        }
                        let (left, right) = (boxes[0].0, sidebar_width - boxes[2].1);
                        assert!(
                            (left - right).abs() <= 0.5,
                            "{n}: the + is {left} from the left, the gear {right} from the right"
                        );
                    }
                    "title-hover" => {
                        assert!(bell_has(ui.text), "{n}: the bell, hovered");
                        assert!(!bell_has(ui.text_dim), "{n}: nothing dim left on it");
                    }
                    "title-unseen" | "title-footer-narrow" => {
                        assert_eq!(at(dot.0, dot.1), bg(ui.accent), "{n}: the dot")
                    }
                    _ => {}
                }
                if scene.starts_with("title-popover") {
                    // The 🔔 shows its popover is open: lit, on the hover fill (as the + with
                    // its menu). The popover's shadow falls on the footer, so colours are
                    // compared, not matched: the bell's most inked pixel (furthest from its
                    // button's fill) is nearer `text` than `text_dim`, and the button's fill
                    // is off the footer beside it as `row_hover` is off the sidebar's colour.
                    let dist = |a: [u8; 3], b: [u8; 3]| -> i32 {
                        a.iter()
                            .zip(b)
                            .map(|(&x, y)| (i32::from(x) - i32::from(y)).pow(2))
                            .sum()
                    };
                    let fill = at(bell_x + 3.0, centre_y);
                    let ink = (0..28)
                        .map(|k| at(bell_x + FOOTER_BUTTON / 2.0, centre_y - 14.0 + k as f32))
                        .max_by_key(|&p| dist(p, fill))
                        .expect("the bell's column");
                    assert!(
                        dist(ink, bg(ui.text)) < dist(ink, bg(ui.text_dim)),
                        "{n}: the bell, lit while open ({ink:?})"
                    );
                    let beside = at(bell_x + FOOTER_BUTTON + 2.0, centre_y);
                    let lum = |c: [u8; 3]| c.iter().map(|&v| i32::from(v)).sum::<i32>();
                    let want = lum(bg(ui.row_hover)) - lum(bg(ui.sidebar_background));
                    let got = lum(fill) - lum(beside);
                    assert!(
                        got.signum() == want.signum() && got.abs() >= want.abs() / 2,
                        "{n}: the bell's button filled ({fill:?} beside {beside:?})"
                    );
                    // The popover hangs from the footer's right buttons: its right edge is the
                    // gear's, over the sidebar (at the narrowest, from the footer's left inset).
                    let (left, bottom) = popover_anchor(sidebar_width, height, STATUS_BAR);
                    let right = left + popover_width(sidebar_width);
                    let y = bottom - 20.0;
                    assert_eq!(at(left + 3.0, y), bg(ui.panel), "{n}: the popover's left");
                    assert_eq!(at(right - 3.0, y), bg(ui.panel), "{n}: the popover's right");
                    assert_ne!(at(right + 3.0, y), bg(ui.panel), "{n}: nothing past it");
                    if scene != "title-popover-narrow" {
                        assert_eq!(
                            right,
                            footer_buttons(sidebar_width)[2] + FOOTER_BUTTON,
                            "{n}: at the gear's right edge"
                        );
                        assert!(right < sidebar_width, "{n}: inside the sidebar");
                    }
                }
                // The + is drawn in the footer's left (ink in its box), the gear's box at the
                // right edge is inside the sidebar.
                let inked = |x0: f32| -> usize {
                    let mut inked = 0;
                    for dy in 0..FOOTER_BUTTON as usize {
                        for dx in 0..FOOTER_BUTTON as usize {
                            let (x, y) =
                                (x0 + dx as f32, centre_y - FOOTER_BUTTON / 2.0 + dy as f32);
                            inked += usize::from(at(x, y) != bg(ui.sidebar_background));
                        }
                    }
                    inked
                };
                assert!(inked(plus_x) > 8, "{n}: the + is drawn");
                assert!(inked(footer_buttons(sidebar_width)[2]) > 8, "{n}: the gear");
                assert!(
                    plus_x + FOOTER_BUTTON <= bell_x && bell_x + FOOTER_BUTTON <= sidebar_width,
                    "{n}: the buttons fit the sidebar"
                );
            } else {
                // No footer, so no 🔔, whatever is unseen: nothing at the foot but the panes.
                assert_ne!(at(dot.0, dot.1), bg(ui.accent), "{n}: no dot");
            }
            // With the sidebar hidden the band holds no 🔔 or + where they were: only the canvas
            // the cards sit on (the sidebar's colour since M11i) from the band's left (the lights,
            // which shots don't draw) to the name's inset; its buttons are at its right now
            // (M11m). (With it shown the band holds the title there now: `title-bar`.)
            let band_bg = ui.sidebar_background;
            let inset = name_inset(false);
            if hidden {
                let stop = inset - 2.0;
                for y in [sy + 2.0, sy + HIT / 2.0, sy + HIT - 2.0] {
                    let mut x = 0.0;
                    while x < stop {
                        // (Near it: the cards' shadows reach a little way up into the band.)
                        let p = at(x, y);
                        assert!(
                            near(p, band_bg),
                            "{n}: the band is bare at ({x}, {y}): {p:?}"
                        );
                        x += 1.0;
                    }
                }
            }
            // The name: ink in the band's middle with the sidebar hidden, none past its inset
            // (the band is the canvas, M11i).
            let band_ink = |x0: f32, x1: f32| -> usize {
                (0..20)
                    .map(|k| 9.0 + k as f32)
                    .map(|y| {
                        let mut x = x0;
                        let mut inked = 0;
                        while x < x1 {
                            inked += usize::from(!near(at(x, y), band_bg));
                            x += 1.0;
                        }
                        inked
                    })
                    .sum()
            };
            if hidden {
                assert!(
                    band_ink(inset, width - inset) > 20,
                    "{n}: the name in the band"
                );
                // Between the name's room and the band's buttons (from `band_buttons`' 🔔 on).
                assert_eq!(
                    band_ink(width - inset, band_buttons(width)[0]),
                    0,
                    "{n}: nothing between the name's room and the band's buttons"
                );
                // The pane tab bars start under the band.
                assert_ne!(at(width / 2.0, 50.0), bg(theme.background), "{n}: a bar");
            }
        }
    }

    // What the title area does (Mocha only: the behaviour is the same in every theme).
    let theme = shot_theme("Catppuccin Mocha");
    let slot: Slot = Rc::new(RefCell::new(None));
    let keep = slot.clone();
    let build = move |window: &mut Window, cx: &mut App| -> AnyView {
        cx.set_global(sidebar_appearance(theme, cx));
        let ws = cx.new(|cx| {
            let mut ws = switcher_workspace(window, cx);
            ws.sidebar.width = SIDEBAR_DEFAULT_WIDTH;
            shot_notifications(&mut ws);
            ws
        });
        *keep.borrow_mut() = Some(ws.clone());
        ws.into()
    };
    let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
        let ws = slotted(&slot);
        let none = Modifiers::none();
        let (width, height) = (960.0, 600.0);
        cx.update(|cx| cx.bind_keys([KeyBinding::new("escape", Dismiss, Some("TabMenu"))]));
        let open = |cx: &mut VisualTestAppContext| {
            cx.read(|cx| ws.read(cx).notifications_popover.is_some())
        };
        let pressed = |cx: &mut VisualTestAppContext| cx.read(|cx| ws.read(cx).band_pressed);
        let active = |cx: &mut VisualTestAppContext| {
            cx.read(|cx| {
                let ws = ws.read(cx);
                ws.sessions.active().map(|s| s.label().to_string())
            })
        };
        let [plus_x, bell_x, _] = footer_buttons(SIDEBAR_DEFAULT_WIDTH);
        let foot_y = height - STATUS_BAR - FOOTER / 2.0;
        let bell = point(px(bell_x + FOOTER_BUTTON / 2.0), px(foot_y));
        let plus = point(px(plus_x + FOOTER_BUTTON / 2.0), px(foot_y));
        // A press on the 🔔 is its own (it doesn't arm a window drag) and, released, opens the
        // popover, which sees everything; Escape closes it.
        assert_eq!(cx.read(|cx| ws.read(cx).notifications.unseen()), 6);
        cx.simulate_mouse_down(window, bell, MouseButton::Left, none);
        assert!(!pressed(cx), "title-area: the 🔔 doesn't drag the window");
        cx.simulate_mouse_up(window, bell, MouseButton::Left, none);
        assert!(open(cx), "title-area: the 🔔 opens the popover");
        assert_eq!(cx.read(|cx| ws.read(cx).notifications.unseen()), 0);
        cx.simulate_keystrokes(window, "escape");
        assert!(!open(cx), "title-area: Escape closes it");
        // Where the 🔔 and + were in the band (on the title now), the band drags the window: no
        // dead hit area.
        let was = point(px(old_buttons.0 + HIT / 2.0), px(sy + HIT / 2.0));
        cx.simulate_mouse_down(window, was, MouseButton::Left, none);
        assert!(
            pressed(cx),
            "title-area: where the buttons were the band drags"
        );
        cx.simulate_mouse_up(window, was, MouseButton::Left, none);
        // The + opens its menu (and a click on it again closes it).
        let menu = |cx: &mut VisualTestAppContext| cx.read(|cx| ws.read(cx).sidebar.new_menu);
        cx.simulate_click(window, plus, none);
        assert!(menu(cx), "title-area: the + opens the new menu");
        cx.simulate_click(window, point(px(width - 20.0), px(40.0)), none);
        assert!(!menu(cx), "title-area: a click outside closes it");
        // A click outside closes the popover.
        cx.simulate_click(window, bell, none);
        cx.simulate_click(window, point(px(800.0), px(40.0)), none);
        assert!(!open(cx), "title-area: a click outside closes it");
        // The closed terminal's entry (the fourth) does nothing; agent's (the first) jumps.
        let popover_left = popover_anchor(SIDEBAR_DEFAULT_WIDTH, height, STATUS_BAR).0;
        let entry = |i: usize| {
            point(
                px(popover_left + 60.0),
                px(entry_top(i, 6, SIDEBAR_DEFAULT_WIDTH, height) + 20.0),
            )
        };
        cx.simulate_click(window, bell, none);
        cx.simulate_click(window, entry(3), none);
        assert!(
            open(cx),
            "title-area: a closed terminal's entry does nothing"
        );
        assert_eq!(active(cx).as_deref(), Some("shell"));
        // So does a script's from outside fuxx's terminals (the sixth).
        cx.simulate_click(window, entry(5), none);
        assert!(
            open(cx),
            "title-area: a script's entry with no terminal does nothing"
        );
        assert_eq!(active(cx).as_deref(), Some("shell"));
        // The hover fill shows where the first entry is (so the click lands on it).
        cx.simulate_mouse_move(window, entry(0), None, none);
        cx.run_until_parked();
        let image = cx.capture_screenshot(window).expect("capture the window");
        let shot = Shot {
            width: image.width(),
            height: image.height(),
            rgba: image.as_raw().to_vec(),
        };
        let ui = theme.ui();
        let fill = pixel_at(
            &shot,
            size(px(width), px(height)),
            popover_left + 12.0,
            entry_top(0, 6, SIDEBAR_DEFAULT_WIDTH, height) + 2.0,
        );
        assert_eq!(
            fill,
            [ui.panel_hover.r, ui.panel_hover.g, ui.panel_hover.b],
            "title-area: the first entry is where `entry_top` says"
        );
        cx.simulate_click(window, entry(0), none);
        assert!(!open(cx), "title-area: a jump closes it");
        assert_eq!(
            active(cx).as_deref(),
            Some("agent"),
            "title-area: agent shown"
        );
        cx.update_window(window, |_, window, cx| {
            let ws = ws.read(cx);
            let view = &ws.sessions.active().expect("agent").view;
            assert!(
                view.focus_handle(cx).is_focused(window),
                "title-area: agent has the keyboard"
            );
        })
        .expect("read the focus");
        // A shortcut that moves the keyboard closes the popover and the + menu: else the popover
        // stays drawn without the keyboard (Escape would go to the terminal) and the pages stay
        // hidden. ⇧⌘U (to the next that needs you, then to the one shown), ⌘1, and a new tab
        // as ⌘T opens one (`open_shot_tab`: a real ⌘T would start a shell).
        cx.update(|cx| {
            cx.bind_keys([
                KeyBinding::new("cmd-shift-u", crate::workspace::NextNeedingYou, None),
                KeyBinding::new("cmd-1", crate::workspace::SelectSession1, None),
            ])
        });
        let hidden = |cx: &mut VisualTestAppContext| cx.read(|cx| ws.read(cx).overlays(cx).any());
        for step in ["cmd-shift-u", "cmd-shift-u", "cmd-1", "new tab"] {
            for (opener, at) in [("popover", bell), ("+ menu", plus)] {
                cx.simulate_click(window, at, none);
                let opened = if opener == "popover" {
                    open(cx)
                } else {
                    menu(cx)
                };
                assert!(opened, "title-area: the {opener} opens before {step}");
                if step == "new tab" {
                    cx.update_window(window, |_, window, cx| {
                        ws.update(cx, |ws, cx| {
                            ws.open_shot_tab("tab", std::path::Path::new(API), "", window, cx)
                        })
                    })
                    .expect("a new tab");
                } else {
                    cx.simulate_keystrokes(window, step);
                }
                cx.run_until_parked();
                assert!(
                    !open(cx) && !menu(cx),
                    "title-area: {step} closes the {opener}"
                );
                assert!(
                    !hidden(cx),
                    "title-area: {step} leaves no page held hidden by the {opener}"
                );
                cx.update_window(window, |_, window, cx| {
                    let ws = ws.read(cx);
                    let view = &ws.sessions.active().expect("a terminal").view;
                    assert!(
                        view.focus_handle(cx).is_focused(window),
                        "title-area: after {step} its terminal has the keyboard"
                    );
                })
                .expect("read the focus");
            }
        }
        // Hiding the sidebar takes the footer, and the popover open over it, away.
        cx.simulate_click(window, bell, none);
        assert!(open(cx), "title-area: reopened");
        cx.update_window(window, |_, window, cx| {
            ws.update(cx, |ws, cx| ws.toggle_sidebar(window, cx))
        })
        .expect("hide the sidebar");
        assert!(
            !open(cx),
            "title-area: hiding the sidebar closes the popover"
        );
        slot.borrow_mut().take();
    };
    render_with(&theme, size(px(960.0), px(600.0)), build, act);
}

/// The title bar (M11m) over `switcher_workspace`, in both themes. With the sidebar open, 760 ×
/// 200:
/// - `title-bar-open`: the default 240 pt sidebar: "the Fuxx?" in the middle of its band, the
///   sidebar button at the band's right edge, over the footer gear;
/// - `title-bar-open-narrow`: the narrowest sidebar (180 pt): the button only, no title;
/// - `title-bar-open-threshold`: the sidebar exactly as wide as the title needs;
/// - `title-bar-open-tiny`: a 200 × 200 window, narrower than the 240 pt sidebar: the button
///   comes in to the window's edge, and (Mocha) a click on it hides the sidebar.
///
/// Collapsed (760 × 200 unless noted): the workspace's name centred on the window and [🔔] [⚙]
/// [sidebar] at the band's far right (`band_buttons`):
/// - `title-bar-collapsed`: nothing unseen, a hide slide just finished; `-hover` and
///   `-hover-sidebar`: the pointer on the ⚙ or the sidebar button; `title-bar-unseen`: the 🔔's
///   dot;
/// - `title-bar-collapsed-narrow` (360 pt, a long name, cut) and `-tiny` (240 pt, no name);
/// - `title-bar-collapsed-right`: 960 × 300, the right sidebar shown;
/// - `title-bar-sliding`: a hide slide 60 ms in (no name until the sidebar has left).
///
/// The popover from the band's 🔔 (collapsed, six notifications): `title-bar-popover` (960 ×
/// 480), `-narrow` (360 × 480) and `-short` (960 × 300, the list cut to the room).
///
/// Panics unless the title's ink is centred on the sidebar's middle and the band's, no wider than
/// `TITLE_ROOM`, clear of the lights' area and short of the button by `TITLE_GAP`; the button's ink
/// is centred in its `open_button_x` box and on the footer gear's ink; nothing is drawn where the
/// button used to be (x 0 to 80); collapsed, each band icon's ink is centred in its slot (over the
/// right sidebar's band too, and mid-slide), the name's on the window's middle inside its insets
/// (cut, or left out under 260 pt, and not drawn until a hide slide has ended), the hover fills the
/// slot, the dot shows while some are unseen, and no button is left at x 80; and (Mocha) unless a
/// press on the title or the collapsed name arms a window drag, a press on any button doesn't, a
/// click on the open button hides the sidebar and ⌘\ shows it, a click on the band 🔔 opens the
/// popover (and Escape closes it) and one on the band's sidebar button shows the sidebar. The
/// band's popover: its inside exactly at `band_popover_anchor` / `band_popover_width` (top right
/// under the band, inside the window's margins), its foot `EDGE_MARGIN` above the status bar, the
/// band 🔔 lit on its fill; and (Mocha) unless its entries are where `band_entry_top` says, a closed
/// terminal's does nothing, an open one's jumps with the sidebar still hidden, and every toggle
/// (band 🔔 then shown, footer 🔔 then ⌘\) closes it, leaves no overlay or backdrop and gives the
/// terminal the keyboard.
fn title_bar_scenes() {
    use crate::sidebar_toggle::{
        BAND_EDGE, BUTTON_Y, HIT, Slide, lights_clear, open_button_at, open_button_x,
    };
    use crate::tab_menu::Dismiss;
    use crate::title_area::{
        DOT, DOT_INSET, EDGE_MARGIN, FOOTER_BUTTON, TITLE_GAP, TITLE_ROOM, band_buttons,
        band_entry_top, band_popover_anchor, band_popover_width, footer_buttons, name_inset,
    };
    use crate::ui_tokens::size::{SIDEBAR_DEFAULT_WIDTH, SIDEBAR_MIN_WIDTH, STATUS_BAR, TOP_BAND};
    use gpui::Focusable;
    use gpui::{KeyBinding, Modifiers, MouseButton};
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::time::{Duration, Instant};

    const FOOTER: f32 = 36.0;
    // Where the title's ink centre sits against the sidebar's middle, read to a fraction of a
    // device pixel (`ink_span`): measured 0.27 pt left on a 2x screen and 0.72 pt left on a 1x
    // one, in both themes. GPUI rounds the text's measured width up to a whole device pixel and
    // the centred box's edges to device pixels (0.12 pt left of exact at 2x, 0.62 at 1x, for
    // its 60.77 pt), and the glyphs' side bearings put the ink about 0.13 pt further left. The
    // shots run at the main screen's scale, so the baseline is the middle of the two, and
    // half a point either way holds both. Rebaseline it if the system font or the GPUI rev
    // changes.
    const TITLE_INK_SHIFT: f32 = -0.5;
    // The narrowest sidebar that shows the title, up to the next half point.
    let threshold = (2.0 * (lights_clear(false) + TITLE_ROOM / 2.0) * 2.0).ceil() / 2.0;
    let dims = size(px(760.0), px(200.0));
    let height = f32::from(dims.height);
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let ui = theme.ui();
        let bg = |c: alacritty_terminal::vte::ansi::Rgb| [c.r, c.g, c.b];
        let (band, dim) = (bg(ui.sidebar_background), bg(ui.text_dim));
        for (scene, sidebar_width) in [
            ("title-bar-open", SIDEBAR_DEFAULT_WIDTH),
            ("title-bar-open-narrow", SIDEBAR_MIN_WIDTH),
            ("title-bar-open-threshold", threshold),
        ] {
            let build = move |window: &mut Window, cx: &mut App| -> AnyView {
                cx.set_global(sidebar_appearance(theme, cx));
                let ws = cx.new(|cx| {
                    let mut ws = switcher_workspace(window, cx);
                    ws.sidebar.width = sidebar_width;
                    ws
                });
                ws.into()
            };
            let shot = render_with(&theme, dims, build, |_, _| ());
            let path = keep_shot(&format!("{scene}-{slug}"), &shot);
            println!("ui-shots: {}", path.display());
            let n = format!("{scene}-{slug}");
            let button_x = open_button_x(sidebar_width);
            // The title's ink to a fraction of a pixel (`ink_span`): a half-way threshold
            // (`ink_box`) takes or drops a whole device pixel at each side as the theme thickens
            // or thins its anti-aliased edges, and moved the centre 0.25 pt between the themes.
            let title = ink_span(&shot, dims, (0.0, 0.0, button_x, TOP_BAND), band, dim);
            let button = ink_box(&shot, dims, (button_x, BUTTON_Y, HIT, HIT), band, dim)
                .unwrap_or_else(|| panic!("{n}: no button ink"));
            // The footer gear's ink, in its slot centred in the footer under its 1 px edge.
            let gear_x = footer_buttons(sidebar_width)[2];
            let centre_y = height - STATUS_BAR - FOOTER + 1.0 + (FOOTER - 1.0) / 2.0;
            let gear_area = (
                gear_x,
                centre_y - FOOTER_BUTTON / 2.0,
                FOOTER_BUTTON,
                FOOTER_BUTTON,
            );
            let gear = ink_box(&shot, dims, gear_area, band, dim)
                .unwrap_or_else(|| panic!("{n}: no gear ink"));
            println!("ui-shots: {n}: title {title:?}, button {button:?}, gear {gear:?}");
            let mid = |b: (f32, f32, f32, f32)| ((b.0 + b.1) / 2.0, (b.2 + b.3) / 2.0);
            // The button: centred in its box at the band's right edge, on the gear's centre.
            let (bx, by) = mid(button);
            assert!(
                (bx - (button_x + HIT / 2.0)).abs() <= 0.5,
                "{n}: the button's ink is centred at {bx}, its box at {}",
                button_x + HIT / 2.0
            );
            assert!(
                (by - TOP_BAND / 2.0).abs() <= 0.5,
                "{n}: the button's ink is centred at y {by}, the band's middle {}",
                TOP_BAND / 2.0
            );
            let (gx, _) = mid(gear);
            assert!(
                (bx - gx).abs() <= 0.5,
                "{n}: the button's ink at {bx}, the gear's at {gx}"
            );
            // Where the button was (right of the lights, which shots don't draw): nothing.
            let old = ink_box(&shot, dims, (0.0, 0.0, 80.0, TOP_BAND), band, dim);
            assert_eq!(old, None, "{n}: ink in the band's left (x 0 to 80)");
            match scene {
                "title-bar-open-narrow" => {
                    let between = (80.0, 0.0, button_x - 80.0, TOP_BAND);
                    let ink = ink_box(&shot, dims, between, band, dim);
                    assert_eq!(ink, None, "{n}: no title in a narrow sidebar");
                }
                _ => {
                    let t = title.unwrap_or_else(|| panic!("{n}: no title"));
                    let (tx, ty) = mid(t);
                    let wide = t.1 - t.0;
                    assert!(
                        (30.0..=TITLE_ROOM).contains(&wide),
                        "{n}: the title is {wide} wide (TITLE_ROOM {TITLE_ROOM})"
                    );
                    // Against the measured baseline (both ways within half a point): this is
                    // the binding check. The sanity bound around the sidebar's middle is wider
                    // (±1.5) than baseline ± 0.5 reaches (−1 to 0), so it never fails first.
                    assert!(
                        (tx - (sidebar_width / 2.0 + TITLE_INK_SHIFT)).abs() <= 0.5,
                        "{n}: the title's ink is centred at {tx}, its baseline {}",
                        sidebar_width / 2.0 + TITLE_INK_SHIFT
                    );
                    assert!(
                        (tx - sidebar_width / 2.0).abs() <= 1.5,
                        "{n}: the title is centred at {tx}, the sidebar's middle {}",
                        sidebar_width / 2.0
                    );
                    assert!(
                        (ty - TOP_BAND / 2.0).abs() <= 1.0,
                        "{n}: the title's middle at y {ty}"
                    );
                    assert!(
                        t.0 >= 80.0 && t.1 <= button_x - TITLE_GAP,
                        "{n}: the title {t:?} runs past [80, {}]",
                        button_x - TITLE_GAP
                    );
                }
            }
        }
    }

    // A window narrower than the sidebar (200 pt, the default 240 pt sidebar): the open button
    // comes in to the window's edge (`open_button_at`), wholly inside it, and the title keeps
    // `TITLE_GAP` short of it.
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let ui = theme.ui();
        let bg = |c: alacritty_terminal::vte::ansi::Rgb| [c.r, c.g, c.b];
        let (band, dim) = (bg(ui.sidebar_background), bg(ui.text_dim));
        let scene = "title-bar-open-tiny";
        let dims = size(px(200.0), px(200.0));
        let build = move |window: &mut Window, cx: &mut App| -> AnyView {
            cx.set_global(sidebar_appearance(theme, cx));
            let ws = cx.new(|cx| {
                let mut ws = switcher_workspace(window, cx);
                ws.sidebar.width = SIDEBAR_DEFAULT_WIDTH;
                ws
            });
            ws.into()
        };
        let shot = render_with(&theme, dims, build, |_, _| ());
        let path = keep_shot(&format!("{scene}-{slug}"), &shot);
        println!("ui-shots: {}", path.display());
        let n = format!("{scene}-{slug}");
        let x = open_button_at(SIDEBAR_DEFAULT_WIDTH, 200.0);
        assert!(
            x + HIT <= 200.0 - BAND_EDGE,
            "{n}: the button's box inside the window"
        );
        let button = ink_box(&shot, dims, (x, BUTTON_Y, HIT, HIT), band, dim)
            .unwrap_or_else(|| panic!("{n}: no button ink at {x}"));
        let title = ink_box(&shot, dims, (0.0, 0.0, x, TOP_BAND), band, dim);
        println!("ui-shots: {n}: button {button:?}, title {title:?}");
        let mid = (button.0 + button.1) / 2.0;
        assert!(
            (mid - (x + HIT / 2.0)).abs() <= 0.5,
            "{n}: the button's ink is centred at {mid}, its box at {}",
            x + HIT / 2.0
        );
        if let Some(t) = title {
            assert!(
                t.1 <= x - TITLE_GAP,
                "{n}: the title {t:?} runs into the button at {x}"
            );
        }
    }
    // And (Mocha) a click on it hides the sidebar.
    {
        let theme = shot_theme("Catppuccin Mocha");
        let slot: Slot = Rc::new(RefCell::new(None));
        let keep = slot.clone();
        let build = move |window: &mut Window, cx: &mut App| -> AnyView {
            cx.set_global(sidebar_appearance(theme, cx));
            let ws = cx.new(|cx| {
                let mut ws = switcher_workspace(window, cx);
                ws.sidebar.width = SIDEBAR_DEFAULT_WIDTH;
                ws
            });
            *keep.borrow_mut() = Some(ws.clone());
            ws.into()
        };
        let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
            let ws = slotted(&slot);
            let x = open_button_at(SIDEBAR_DEFAULT_WIDTH, 200.0) + HIT / 2.0;
            let on = point(px(x), px(BUTTON_Y + HIT / 2.0));
            cx.simulate_click(window, on, Modifiers::none());
            assert!(
                cx.read(|cx| ws.read(cx).sidebar.hidden),
                "title-bar open, 200 pt window: a click on the button hides the sidebar"
            );
            slot.borrow_mut().take();
        };
        render_with(&theme, size(px(200.0), px(200.0)), build, act);
    }

    // Collapsed (M11m Task 2): the workspace's name centred on the window, [🔔] [⚙] [sidebar] at
    // the band's far right.
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let ui = theme.ui();
        let bg = |c: alacritty_terminal::vte::ansi::Rgb| [c.r, c.g, c.b];
        let (band, dim, full) = (bg(ui.sidebar_background), bg(ui.text_dim), bg(ui.text));
        for (scene, w, h) in [
            ("title-bar-collapsed", 760.0, 200.0),
            ("title-bar-collapsed-hover", 760.0, 200.0),
            ("title-bar-collapsed-hover-sidebar", 760.0, 200.0),
            ("title-bar-unseen", 760.0, 200.0),
            ("title-bar-collapsed-narrow", 360.0, 200.0),
            ("title-bar-collapsed-tiny", 240.0, 200.0),
            ("title-bar-collapsed-right", 960.0, 300.0),
            ("title-bar-sliding", 760.0, 200.0),
        ] {
            let dims = size(px(w), px(h));
            let b = band_buttons(w);
            let build = move |window: &mut Window, cx: &mut App| -> AnyView {
                cx.set_global(sidebar_appearance(theme, cx));
                let ws = cx.new(|cx| {
                    let mut ws = switcher_workspace(window, cx);
                    ws.sidebar.width = SIDEBAR_DEFAULT_WIDTH;
                    ws.sidebar.hidden = true;
                    shot_notifications(&mut ws);
                    if scene != "title-bar-unseen" {
                        ws.notifications.mark_seen();
                    }
                    if scene == "title-bar-collapsed-narrow" {
                        let shown = ws.model.focused_workspace().expect("focused");
                        ws.model.set_name(shown, Some(LONG_NAME.into()));
                    }
                    if scene == "title-bar-collapsed-right" {
                        ws.right_sidebar.hidden = false;
                    }
                    if scene == "title-bar-sliding" {
                        // Real time (`Instant`; GPUI's timers run on the test clock): of the
                        // 160 ms `SLIDE`, the fill at x 30 lasts until 12.5 % shown, 80 ms in,
                        // so this fails if build to frame takes over ~20 ms (about 1 ms today).
                        ws.slide = Some(Slide {
                            showing: false,
                            since: Instant::now() - Duration::from_millis(60),
                            from: 1.0,
                        });
                    }
                    if scene == "title-bar-collapsed" {
                        // A hide slide that has finished: the name is back.
                        ws.slide = Some(Slide {
                            showing: false,
                            since: Instant::now() - Duration::from_millis(400),
                            from: 1.0,
                        });
                    }
                    ws
                });
                ws.into()
            };
            let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let on = match scene {
                    "title-bar-collapsed-hover" => Some(b[1]),
                    "title-bar-collapsed-hover-sidebar" => Some(b[2]),
                    _ => None,
                };
                if let Some(x) = on {
                    let at = point(px(x + HIT / 2.0), px(BUTTON_Y + HIT / 2.0));
                    cx.simulate_mouse_move(window, at, None, Modifiers::none());
                }
            };
            let shot = render_with(&theme, dims, build, act);
            let path = keep_shot(&format!("{scene}-{slug}"), &shot);
            println!("ui-shots: {}", path.display());
            let n = format!("{scene}-{slug}");
            let at = |x: f32, y: f32| pixel_at(&shot, dims, x, y);
            let mid = |b: (f32, f32, f32, f32)| ((b.0 + b.1) / 2.0, (b.2 + b.3) / 2.0);
            // The three icons' ink, each read over its 24 × 24 slot: centred in it, all on the
            // band's middle line.
            let icons_at_slots = || {
                let boxes = b.map(|x| {
                    ink_box(&shot, dims, (x, BUTTON_Y, HIT, HIT), band, dim)
                        .unwrap_or_else(|| panic!("{n}: no icon ink in the slot at {x}"))
                });
                println!("ui-shots: {n}: band icons (x0, x1, y0, y1) {boxes:?}");
                for ((name, x), ink) in ["bell", "gear", "sidebar"].iter().zip(b).zip(boxes) {
                    let (ix, iy) = mid(ink);
                    assert!(
                        (ix - (x + HIT / 2.0)).abs() <= 0.5,
                        "{n}: the {name}'s ink is centred at {ix}, its slot at {}",
                        x + HIT / 2.0
                    );
                    assert!(
                        (iy - TOP_BAND / 2.0).abs() <= 0.5,
                        "{n}: the {name}'s ink is centred at y {iy}, the band's middle {}",
                        TOP_BAND / 2.0
                    );
                }
                let ys = boxes.map(|ink| mid(ink).1);
                for y in ys {
                    assert!((y - ys[0]).abs() <= 0.5, "{n}: the icons' middles {ys:?}");
                }
            };
            // The name's ink (toward `text`), in the band left of the 🔔.
            let name_ink = ink_box(&shot, dims, (0.0, 0.0, b[0], TOP_BAND), band, full);
            println!("ui-shots: {n}: name {name_ink:?}");
            let centred = |ink: Option<(f32, f32, f32, f32)>| {
                let ink = ink.unwrap_or_else(|| panic!("{n}: no name"));
                let (nx, _) = mid(ink);
                assert!(
                    (nx - w / 2.0).abs() <= 1.0,
                    "{n}: the name is centred at {nx}, the window's middle {}",
                    w / 2.0
                );
                assert!(
                    ink.0 >= name_inset(false) && ink.1 <= w - name_inset(false),
                    "{n}: the name {ink:?} runs past its insets"
                );
            };
            // The 🔔's unseen dot, at its top right.
            let dot = (
                b[0] + HIT - DOT_INSET - DOT / 2.0,
                BUTTON_Y + DOT_INSET + DOT / 2.0,
            );
            // The device pixels of the sidebar's own fill (`sidebar_background`) down the line
            // x = 30, over the pane's body (under the band, above the status bar): none once the
            // sidebar has gone (the pane is there: its card is the terminal's colour), some
            // while it is partly in (round its search box, rows and footer). Not one point: the
            // sidebar's group cards are `card`, which is the terminal's background
            // (`theme.rs`), so a point on one can't tell the sidebar from the pane.
            let sidebar_fill = || {
                let scale = shot.width as f32 / w;
                let (top, foot) = (TOP_BAND + 8.0, h - STATUS_BAR - 12.0);
                ((top * scale) as usize..(foot * scale) as usize)
                    .filter(|&y| {
                        near(
                            pixel_at(&shot, dims, 30.0, y as f32 / scale),
                            ui.sidebar_background,
                        )
                    })
                    .count()
            };
            match scene {
                "title-bar-collapsed" => {
                    icons_at_slots();
                    assert_ne!(at(dot.0, dot.1), bg(ui.accent), "{n}: no dot");
                    centred(name_ink);
                    let fill = sidebar_fill();
                    assert_eq!(fill, 0, "{n}: {fill} pixels of sidebar at x 30, slid out");
                    // Where the sidebar button was (x 80) and the band's left: bare, and no
                    // title anywhere.
                    let left = ink_box(&shot, dims, (0.0, 0.0, 100.0, TOP_BAND), band, dim);
                    assert_eq!(left, None, "{n}: ink in the band's left (x 0 to 100)");
                }
                "title-bar-collapsed-hover" => {
                    assert_eq!(at(b[1] + 1.5, 19.0), bg(ui.panel_hover), "{n}: ⚙ hovered");
                    assert_eq!(at(b[0] + 1.5, 19.0), band, "{n}: 🔔 not hovered");
                }
                "title-bar-collapsed-hover-sidebar" => {
                    assert_eq!(
                        at(b[2] + 1.5, 19.0),
                        bg(ui.panel_hover),
                        "{n}: the sidebar button hovered"
                    );
                    assert_eq!(at(b[1] + 1.5, 19.0), band, "{n}: ⚙ not hovered");
                }
                "title-bar-unseen" => {
                    assert_eq!(at(dot.0, dot.1), bg(ui.accent), "{n}: the dot");
                    assert_eq!(at(dot.0 + DOT / 2.0 - 1.0, dot.1), band, "{n}: its ring");
                }
                "title-bar-collapsed-narrow" => {
                    icons_at_slots();
                    let ink = name_ink.unwrap_or_else(|| panic!("{n}: no name"));
                    assert!(
                        ink.0 >= name_inset(false) && ink.1 <= w - name_inset(false),
                        "{n}: the name {ink:?} runs past [100, {}]",
                        w - name_inset(false)
                    );
                    // Enough of it is drawn to read (cut with an ellipsis, not left out).
                    let scale = shot.width as f32 / w;
                    let inked = (0..(TOP_BAND * scale) as usize)
                        .flat_map(|y| (0..(b[0] * scale) as usize).map(move |x| (x, y)))
                        .filter(|&(x, y)| {
                            let p = pixel_at(&shot, dims, x as f32 / scale, y as f32 / scale);
                            gap(p, ui.sidebar_background) > gap(p, ui.text)
                        })
                        .count();
                    assert!(inked > 20, "{n}: {inked} pixels of the name");
                }
                "title-bar-collapsed-tiny" => {
                    icons_at_slots();
                    assert_eq!(name_ink, None, "{n}: no name in a window this narrow");
                }
                "title-bar-collapsed-right" => {
                    // Over the right sidebar's band, not under it or clipped by the pane column;
                    // the name on the window's middle, not the pane column's.
                    icons_at_slots();
                    centred(name_ink);
                }
                "title-bar-sliding" => {
                    icons_at_slots();
                    // The sidebar is partly in (and partly out), as in `sidebar-button-sliding`.
                    let fill = sidebar_fill();
                    assert!(fill > 0, "{n}: no sidebar at x 30, not still partly in");
                    assert_eq!(at(150.0, 80.0), bg(theme.background), "{n}: slid out");
                    // The sliding sidebar still carries its own button, and nothing is drawn at
                    // the collapsed button's old place (x 80) over it.
                    let own = ink_box(&shot, dims, (0.0, BUTTON_Y, 60.0, HIT), band, dim);
                    assert!(own.is_some(), "{n}: the sliding sidebar's button");
                    let old = ink_box(&shot, dims, (80.0, BUTTON_Y, HIT, HIT), band, dim);
                    assert_eq!(old, None, "{n}: a button at the old place (x 80)");
                    // The name waits for the sidebar to leave: none in the band between its
                    // insets (it would paint over the sliding sidebar's title and rows).
                    let inset = name_inset(false);
                    let room = (inset, 0.0, w - 2.0 * inset, TOP_BAND);
                    let name = ink_box(&shot, dims, room, band, full);
                    assert_eq!(name, None, "{n}: the name drawn mid-slide");
                }
                _ => unreachable!(),
            }
        }
    }

    // The popover from the band's 🔔 (M11m Task 3): it drops down from under the band, its right
    // edge the band's, inside the window however narrow.
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let ui = theme.ui();
        let bg = |c: alacritty_terminal::vte::ansi::Rgb| [c.r, c.g, c.b];
        for (scene, w, h) in [
            ("title-bar-popover", 960.0, 480.0),
            ("title-bar-popover-narrow", 360.0, 480.0),
            ("title-bar-popover-short", 960.0, 300.0),
        ] {
            let dims = size(px(w), px(h));
            let b = band_buttons(w);
            let slot: Slot = Rc::new(RefCell::new(None));
            let keep = slot.clone();
            let build = move |window: &mut Window, cx: &mut App| -> AnyView {
                cx.set_global(sidebar_appearance(theme, cx));
                let ws = cx.new(|cx| {
                    let mut ws = switcher_workspace(window, cx);
                    ws.sidebar.width = SIDEBAR_DEFAULT_WIDTH;
                    ws.sidebar.hidden = true;
                    shot_notifications(&mut ws);
                    ws
                });
                *keep.borrow_mut() = Some(ws.clone());
                ws.into()
            };
            let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let ws = slotted(&slot);
                let n = format!("{scene}-{slug}");
                cx.update_window(window, |_, window, cx| {
                    ws.update(cx, |ws, cx| ws.open_notifications(window, cx))
                })
                .expect("open it");
                cx.run_until_parked();
                cx.update(|cx| {
                    let ws = ws.read(cx);
                    assert_eq!(ws.notifications.unseen(), 0, "{n}: all seen");
                    assert!(ws.overlays(cx).any(), "{n}: an overlay");
                });
                slot.borrow_mut().take();
            };
            let shot = render_with(&theme, dims, build, act);
            let path = keep_shot(&format!("{scene}-{slug}"), &shot);
            println!("ui-shots: {}", path.display());
            let n = format!("{scene}-{slug}");
            let at = |x: f32, y: f32| pixel_at(&shot, dims, x, y);
            let (r, t) = band_popover_anchor(w);
            let pw = band_popover_width(w);
            let panel = bg(ui.panel);
            // Under the band, its right edge the band's, its left `pw` from it.
            let y = t + 20.0;
            assert_eq!(at(r - 3.0, y), panel, "{n}: the popover's right");
            assert_eq!(at(r - pw + 3.0, y), panel, "{n}: the popover's left");
            assert_ne!(at(r + 3.0, y), panel, "{n}: nothing past its right");
            assert_ne!(
                at(r - 20.0, t - 6.0),
                panel,
                "{n}: it starts under the band"
            );
            // Every device pixel of row `y`: the panel's ink keeps inside the window's margins.
            let scale = shot.width as f32 / w;
            let xs: Vec<f32> = (0..shot.width as usize)
                .map(|x| x as f32 / scale)
                .filter(|&x| at(x, y) == panel)
                .collect();
            let dp = 1.0 / scale;
            let (first, last) = (xs[0], xs[xs.len() - 1] + dp);
            println!("ui-shots: {n}: the panel's row {y} runs {first}..{last}");
            assert!(first >= EDGE_MARGIN, "{n}: the panel starts at {first}");
            assert!(last <= w - BAND_EDGE, "{n}: the panel ends at {last}");
            // Exactly where the model says, inside its 1 pt edge: the anchor is in whole points,
            // so the edges fall on the pixel grid (less than a device pixel off).
            assert!(
                (last - (r - 1.0)).abs() < dp && (first - (r - pw + 1.0)).abs() < dp,
                "{n}: the panel's inside runs {first}..{last}, the model's {}..{}",
                r - pw + 1.0,
                r - 1.0
            );
            // Down column `r − 20` (clear of the rounded corners), every device pixel: its top
            // just under the band, inside its edge; its foot above the status bar by the margin
            // (and, in the short window, the list fills the room down to there).
            let ys: Vec<f32> = (0..shot.height as usize)
                .map(|y| y as f32 / scale)
                .filter(|&y| at(r - 20.0, y) == panel)
                .collect();
            let (top, foot) = (ys[0], ys[ys.len() - 1] + dp);
            println!("ui-shots: {n}: the panel's column runs {top}..{foot}");
            assert!(
                (top - (t + 1.0)).abs() < dp,
                "{n}: the panel's inside starts at {top}, the model's {}",
                t + 1.0
            );
            let floor = h - STATUS_BAR - EDGE_MARGIN;
            assert!(
                foot <= floor - 1.0 + dp,
                "{n}: the panel's foot {foot} past {floor}"
            );
            if scene == "title-bar-popover-short" {
                assert!(
                    foot >= floor - 1.0 - dp,
                    "{n}: the list stops at {foot}, short of the room down to {floor}"
                );
            }
            // The band's 🔔 shows its popover is open: lit (its most inked pixel, furthest from
            // its fill, nearer `text` than `text_dim`), on the hover fill.
            let dist = |a: [u8; 3], c: [u8; 3]| -> i32 {
                a.iter()
                    .zip(c)
                    .map(|(&x, y)| (i32::from(x) - i32::from(y)).pow(2))
                    .sum()
            };
            // The panel's shadow reaches up into the band (a level or two darker in Mocha, six in
            // Latte), so the fill is compared, not matched (as `title-area` does for the footer's
            // 🔔): the slot's corner differs from the band just left of the 🔔, under the same
            // shadow, by what `panel_hover` differs from the band's colour: within 1 a channel,
            // and the same way on every channel that differs at all (in Latte that is 3, 1, 1, so
            // an unlit slot, which differs by nothing, fails on all three).
            let fill = at(b[0] + 1.5, 19.0);
            let beside = at(b[0] - 3.0, 19.0);
            let (hover, band) = (bg(ui.panel_hover), bg(ui.sidebar_background));
            let off = (0..3).all(|k| {
                let got = i32::from(fill[k]) - i32::from(beside[k]);
                let want = i32::from(hover[k]) - i32::from(band[k]);
                (got - want).abs() <= 1 && (want == 0 || got.signum() == want.signum())
            });
            assert!(
                off,
                "{n}: the band 🔔 on its fill ({fill:?} beside {beside:?}; panel_hover {hover:?} \
                 on the band's {band:?})"
            );
            let ink = (0..HIT as usize)
                .map(|k| at(b[0] + HIT / 2.0, BUTTON_Y + k as f32))
                .max_by_key(|&p| dist(p, fill))
                .expect("the bell's column");
            assert!(
                dist(ink, bg(ui.text)) < dist(ink, bg(ui.text_dim)),
                "{n}: the band 🔔, lit while open ({ink:?})"
            );
        }
    }

    // What the popover from the band does (Mocha only), 960 × 600, the sidebar hidden: an
    // entry is where `band_entry_top` says, a closed terminal's does nothing, one that's open
    // jumps (the sidebar stays hidden); and every toggle of the sidebar closes the popover,
    // whichever 🔔 it hung from, and gives the keyboard back to the terminal.
    let theme = shot_theme("Catppuccin Mocha");
    let slot: Slot = Rc::new(RefCell::new(None));
    let keep = slot.clone();
    let build = move |window: &mut Window, cx: &mut App| -> AnyView {
        cx.set_global(sidebar_appearance(theme, cx));
        let ws = cx.new(|cx| {
            let mut ws = switcher_workspace(window, cx);
            ws.sidebar.width = SIDEBAR_DEFAULT_WIDTH;
            ws.sidebar.hidden = true;
            shot_notifications(&mut ws);
            ws
        });
        *keep.borrow_mut() = Some(ws.clone());
        ws.into()
    };
    let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
        let ws = slotted(&slot);
        let none = Modifiers::none();
        let n = "title-bar popover";
        let (width, height) = (960.0, 600.0);
        cx.update(|cx| {
            cx.bind_keys([
                KeyBinding::new("escape", Dismiss, Some("TabMenu")),
                KeyBinding::new("cmd-\\", crate::workspace::ToggleSidebar, None),
            ])
        });
        let open = |cx: &mut VisualTestAppContext| {
            cx.read(|cx| ws.read(cx).notifications_popover.is_some())
        };
        let hidden = |cx: &mut VisualTestAppContext| cx.read(|cx| ws.read(cx).sidebar.hidden);
        let pressed = |cx: &mut VisualTestAppContext| cx.read(|cx| ws.read(cx).band_pressed);
        let active = |cx: &mut VisualTestAppContext| {
            cx.read(|cx| {
                let ws = ws.read(cx);
                ws.sessions.active().map(|s| s.label().to_string())
            })
        };
        let b = band_buttons(width);
        let band_bell = point(px(b[0] + HIT / 2.0), px(BUTTON_Y + HIT / 2.0));
        let (r, _) = band_popover_anchor(width);
        let left = r - band_popover_width(width);
        let entry = |i: usize| point(px(left + 60.0), px(band_entry_top(i) + 20.0));
        // The closed terminal's entry (the fourth) does nothing.
        cx.simulate_click(window, band_bell, none);
        assert!(open(cx), "{n}: the band 🔔 opens it");
        cx.simulate_click(window, entry(3), none);
        assert!(open(cx), "{n}: a closed terminal's entry does nothing");
        assert_eq!(active(cx).as_deref(), Some("shell"));
        // The hover fill shows where the first entry is (so the click lands on it).
        cx.simulate_mouse_move(window, entry(0), None, none);
        cx.run_until_parked();
        let image = cx.capture_screenshot(window).expect("capture the window");
        let shot = Shot {
            width: image.width(),
            height: image.height(),
            rgba: image.as_raw().to_vec(),
        };
        let ui = theme.ui();
        let fill = pixel_at(
            &shot,
            size(px(width), px(height)),
            left + 12.0,
            band_entry_top(0) + 2.0,
        );
        let hover = [ui.panel_hover.r, ui.panel_hover.g, ui.panel_hover.b];
        assert_eq!(
            fill, hover,
            "{n}: the first entry is where `band_entry_top` says"
        );
        // And it starts there: a point above it is the panel's padding, not the entry's fill.
        let above = pixel_at(
            &shot,
            size(px(width), px(height)),
            left + 12.0,
            band_entry_top(0) - 1.0,
        );
        assert_ne!(
            above, hover,
            "{n}: the first entry starts at `band_entry_top`"
        );
        // A click on it shows agent with the keyboard; the sidebar stays hidden.
        cx.simulate_click(window, entry(0), none);
        assert!(!open(cx), "{n}: a jump closes it");
        assert_eq!(active(cx).as_deref(), Some("agent"), "{n}: agent shown");
        assert!(hidden(cx), "{n}: the sidebar still hidden");
        // After a toggle: no popover, no overlay, the active terminal has the keyboard, and no
        // backdrop is left over the band (a press on it arms a window drag).
        let closed = |cx: &mut VisualTestAppContext, how: &str| {
            assert!(!open(cx), "{n}: {how} closes the popover");
            assert!(
                !cx.read(|cx| ws.read(cx).overlays(cx).any()),
                "{n}: {how} leaves no overlay"
            );
            cx.update_window(window, |_, window, cx| {
                let ws = ws.read(cx);
                let view = &ws.sessions.active().expect("a terminal").view;
                assert!(
                    view.focus_handle(cx).is_focused(window),
                    "{n}: after {how} the terminal has the keyboard"
                );
            })
            .expect("read the focus");
            let on_band = point(px(width / 2.0), px(TOP_BAND / 2.0));
            cx.simulate_mouse_down(window, on_band, MouseButton::Left, none);
            assert!(pressed(cx), "{n}: after {how} the band drags (no backdrop)");
            cx.simulate_mouse_up(window, on_band, MouseButton::Left, none);
        };
        // Open from the band's 🔔, then show the sidebar: the 🔔 it hung from is gone.
        cx.simulate_click(window, band_bell, none);
        assert!(open(cx), "{n}: reopened from the band");
        cx.update_window(window, |_, window, cx| {
            ws.update(cx, |ws, cx| ws.toggle_sidebar(window, cx))
        })
        .expect("show the sidebar");
        assert!(!hidden(cx), "{n}: shown");
        closed(cx, "showing the sidebar");
        // Open from the footer's 🔔 (the slide done), then ⌘\ hides the sidebar: the same.
        cx.update(|cx| {
            ws.update(cx, |ws, cx| {
                ws.slide = None;
                cx.notify();
            })
        });
        cx.run_until_parked();
        let bell_x = footer_buttons(SIDEBAR_DEFAULT_WIDTH)[1];
        let foot_y = height - STATUS_BAR - FOOTER / 2.0;
        cx.simulate_click(
            window,
            point(px(bell_x + FOOTER_BUTTON / 2.0), px(foot_y)),
            none,
        );
        assert!(open(cx), "{n}: opened from the footer");
        cx.simulate_keystrokes(window, "cmd-\\");
        assert!(hidden(cx), "{n}: ⌘\\ hides the sidebar");
        closed(cx, "hiding the sidebar");
        slot.borrow_mut().take();
    };
    render_with(&theme, size(px(960.0), px(600.0)), build, act);

    // What the band does (Mocha only: the behaviour is the same in every theme).
    let theme = shot_theme("Catppuccin Mocha");
    let slot: Slot = Rc::new(RefCell::new(None));
    let keep = slot.clone();
    let build = move |window: &mut Window, cx: &mut App| -> AnyView {
        cx.set_global(sidebar_appearance(theme, cx));
        let ws = cx.new(|cx| {
            let mut ws = switcher_workspace(window, cx);
            ws.sidebar.width = SIDEBAR_DEFAULT_WIDTH;
            ws
        });
        *keep.borrow_mut() = Some(ws.clone());
        ws.into()
    };
    let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
        let ws = slotted(&slot);
        let none = Modifiers::none();
        cx.update(|cx| {
            cx.bind_keys([KeyBinding::new(
                "cmd-\\",
                crate::workspace::ToggleSidebar,
                None,
            )])
        });
        let pressed = |cx: &mut VisualTestAppContext| cx.read(|cx| ws.read(cx).band_pressed);
        let hidden = |cx: &mut VisualTestAppContext| cx.read(|cx| ws.read(cx).sidebar.hidden);
        // A press on the title arms a window drag: it takes no clicks.
        let on_title = point(px(SIDEBAR_DEFAULT_WIDTH / 2.0), px(TOP_BAND / 2.0));
        cx.simulate_mouse_down(window, on_title, MouseButton::Left, none);
        assert!(
            pressed(cx),
            "title-bar: a press on the title arms a window drag"
        );
        cx.simulate_mouse_up(window, on_title, MouseButton::Left, none);
        assert!(!pressed(cx), "title-bar: released");
        // A press on the button is its own; released, it is a click that hides the sidebar.
        let x = open_button_x(SIDEBAR_DEFAULT_WIDTH) + HIT / 2.0;
        let on_button = point(px(x), px(TOP_BAND / 2.0));
        cx.simulate_mouse_down(window, on_button, MouseButton::Left, none);
        assert!(
            !pressed(cx),
            "title-bar: the button doesn't drag the window"
        );
        assert!(!hidden(cx), "title-bar: not before the click");
        cx.simulate_mouse_up(window, on_button, MouseButton::Left, none);
        assert!(
            hidden(cx),
            "title-bar: a click on the button hides the sidebar"
        );
        cx.simulate_keystrokes(window, "cmd-\\");
        assert!(!hidden(cx), "title-bar: ⌘\\ shows it again");
        // With the sidebar shown there are no band buttons: where the band's sidebar button would
        // be is the pane tab bar's own control (◧, the right sidebar), which opens the right
        // sidebar and leaves this one shown.
        let right = |cx: &mut VisualTestAppContext| cx.read(|cx| ws.read(cx).right_sidebar.hidden);
        assert!(right(cx), "title-bar: the right sidebar starts hidden");
        let slot_x = band_buttons(960.0)[2] + HIT / 2.0;
        cx.simulate_click(window, point(px(slot_x), px(TOP_BAND / 2.0)), none);
        assert!(
            !hidden(cx),
            "title-bar: no band sidebar button while the sidebar is shown"
        );
        assert!(
            !right(cx),
            "title-bar: the tab bar's ◧ opened the right sidebar"
        );
        slot.borrow_mut().take();
    };
    render_with(&theme, size(px(960.0), px(600.0)), build, act);

    // What the collapsed band does (Mocha only), with and without the right sidebar shown.
    for right in [false, true] {
        let theme = shot_theme("Catppuccin Mocha");
        let slot: Slot = Rc::new(RefCell::new(None));
        let keep = slot.clone();
        let build = move |window: &mut Window, cx: &mut App| -> AnyView {
            cx.set_global(sidebar_appearance(theme, cx));
            let ws = cx.new(|cx| {
                let mut ws = switcher_workspace(window, cx);
                ws.sidebar.width = SIDEBAR_DEFAULT_WIDTH;
                ws.sidebar.hidden = true;
                ws.right_sidebar.hidden = !right;
                shot_notifications(&mut ws);
                ws
            });
            *keep.borrow_mut() = Some(ws.clone());
            ws.into()
        };
        let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
            let ws = slotted(&slot);
            let none = Modifiers::none();
            let n = if right {
                "title-bar collapsed, right sidebar shown"
            } else {
                "title-bar collapsed"
            };
            cx.update(|cx| cx.bind_keys([KeyBinding::new("escape", Dismiss, Some("TabMenu"))]));
            let pressed = |cx: &mut VisualTestAppContext| cx.read(|cx| ws.read(cx).band_pressed);
            let hidden = |cx: &mut VisualTestAppContext| cx.read(|cx| ws.read(cx).sidebar.hidden);
            let open = |cx: &mut VisualTestAppContext| {
                cx.read(|cx| ws.read(cx).notifications_popover.is_some())
            };
            let b = band_buttons(960.0);
            let y = BUTTON_Y + HIT / 2.0;
            // A press on the name arms a window drag: it takes no clicks.
            let on_name = point(px(480.0), px(y));
            cx.simulate_mouse_down(window, on_name, MouseButton::Left, none);
            assert!(pressed(cx), "{n}: a press on the name arms a window drag");
            cx.simulate_mouse_up(window, on_name, MouseButton::Left, none);
            assert!(!pressed(cx), "{n}: released");
            // A press on each band button is its own, released off it (so it is no click: the
            // ⚙ opens no Preferences window).
            for (name, x) in ["bell", "gear", "sidebar"].iter().zip(b) {
                let on = point(px(x + HIT / 2.0), px(y));
                cx.simulate_mouse_down(window, on, MouseButton::Left, none);
                assert!(
                    !pressed(cx),
                    "{n}: a press on the {name} doesn't drag the window"
                );
                cx.simulate_mouse_up(window, on_name, MouseButton::Left, none);
                assert!(
                    hidden(cx) && !open(cx),
                    "{n}: released off the {name}, no click"
                );
            }
            // A click on the 🔔 opens the popover, which sees everything; Escape closes it.
            assert_eq!(cx.read(|cx| ws.read(cx).notifications.unseen()), 6);
            cx.simulate_click(window, point(px(b[0] + HIT / 2.0), px(y)), none);
            assert!(open(cx), "{n}: the 🔔 opens the popover");
            assert_eq!(cx.read(|cx| ws.read(cx).notifications.unseen()), 0);
            cx.simulate_keystrokes(window, "escape");
            assert!(!open(cx), "{n}: Escape closes it");
            // And the 🔔 is no longer lit: its slot's fill is gone (the pointer moved off it, so
            // no hover fill either), the band's own colour again.
            cx.simulate_mouse_move(window, point(px(480.0), px(300.0)), None, none);
            cx.run_until_parked();
            let image = cx.capture_screenshot(window).expect("capture the window");
            let shot = Shot {
                width: image.width(),
                height: image.height(),
                rgba: image.as_raw().to_vec(),
            };
            let ui = theme.ui();
            let corner = pixel_at(&shot, size(px(960.0), px(600.0)), b[0] + 1.5, 19.0);
            let band = [
                ui.sidebar_background.r,
                ui.sidebar_background.g,
                ui.sidebar_background.b,
            ];
            assert_eq!(corner, band, "{n}: after Escape the 🔔's fill is gone");
            // A click on the sidebar button shows the sidebar.
            cx.simulate_click(window, point(px(b[2] + HIT / 2.0), px(y)), none);
            assert!(!hidden(cx), "{n}: the sidebar button shows the sidebar");
            slot.borrow_mut().take();
        };
        render_with(&theme, size(px(960.0), px(600.0)), build, act);
    }
}
