//! The window as a whole: the harness smoke test, the design tokens, the window frame, the status
//! marks, the pane cards, and the title area and title bar.

use super::panes::quiet_line;
use super::sidebar::CLAUDE_QUESTION;
use super::*;

/// This file, as `file!()` gives it (repo-relative under cargo): `registry::AREAS` pairs it with
/// `SCENES`, so `--changed` runs these scenes when this file changes.
pub(super) const FILE: &str = file!();

/// This area's scenes (`registry.rs` runs them in its `ORDER`).
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
        // Serial: (c) a shot mid-slide, the slide set 60 ms back by the real clock.
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
    Scene {
        name: "card",
        run: one_card_scene,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &["src/workspace.rs", "src/panes.rs", "src/terminal_view.rs"],
    },
    Scene {
        name: "card-split",
        run: one_card_split_scenes,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/workspace.rs",
            "src/panes.rs",
            "src/split_tree.rs",
            "src/terminal_view.rs",
        ],
    },
    Scene {
        name: "card-sidebar-hidden",
        run: one_card_sidebar_hidden_scene,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &["src/workspace.rs", "src/panes.rs", "src/title_area.rs"],
    },
    Scene {
        name: "card-right-hidden",
        run: one_card_right_hidden_scene,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/workspace.rs",
            "src/panes.rs",
            "src/terminal_view.rs",
            "src/right_sidebar.rs",
        ],
    },
    Scene {
        name: "card-narrow",
        run: one_card_narrow_scene,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &["src/workspace.rs", "src/panes.rs", "src/right_sidebar.rs"],
    },
    Scene {
        name: "card-slide",
        run: one_card_slide_scene,
        themes: Themes::Both,
        // Serial: a shot mid-slide, the slide set 20 ms back by the real clock.
        serial: true,
        kind: Kind::Visual,
        sources: &[
            "src/workspace.rs",
            "src/panes.rs",
            "src/right_sidebar.rs",
            "src/sidebar_toggle.rs",
        ],
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
            // M11i: cards, the quiet selection and the Files tree's guides; M11al's shown tab.
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
            ("tab_active", ui.tab_active),
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

/// Panics unless the padding of the pane at the right (inside the one card, M11al) is all
/// terminal background, the insets round the card (and with the sidebar hidden, the top band)
/// are the canvas, and the left pane's grid starts just inside its left padding.
fn check_window(
    shot: &Shot,
    theme: &Theme,
    dims: Size<Pixels>,
    split: bool,
    hidden: bool,
    scene: &str,
    slug: &str,
) {
    use ui_tokens::size::{CARD_RADIUS, HAIRLINE, STATUS_BAR, TAB_BAR, TOP_BAND};

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
    // The card starts under the band (sidebar hidden) or the `CARD_INSET` strip, and ends
    // `CARD_INSET` short of the window's right and the status bar (M11al); inside its 1 pt edge,
    // each pane's tab bar, then its terminal.
    let card_top = if hidden { TOP_BAND } else { CARD_INSET };
    let top = card_top + HAIRLINE + TAB_BAR;
    let (right, bottom) = (
        w - CARD_INSET - HAIRLINE,
        h - STATUS_BAR - CARD_INSET - HAIRLINE,
    );
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
    // Its top padding (right of a split's hairline: the pane area starts after the sidebar, at
    // most 1/4 in), short of the corner.
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
        canvas(w - CARD_INSET / 2.0, y);
    }
    canvas(w * 0.75, h - STATUS_BAR - CARD_INSET / 2.0);
    canvas(w * 0.75, card_top / 2.0);
    if hidden {
        for x in [w * 0.75, w - 20.0] {
            canvas(x, 6.0);
            canvas(x, TOP_BAND - CARD_INSET / 2.0);
        }
    }
    // The left pane's left padding: where the terminal background starts (after the sidebar,
    // or the inset, and the card's edge), `pad_x` of background, then the grid's first column (its
    // prompt and output) within a cell and a half (the grid's leftover width is shared by both
    // sides).
    let mid = (top + h) / 2.0;
    let left = (0..w as usize)
        .map(|x| x as f32)
        .find(|&x| is_bg(x, mid))
        .expect("a terminal");
    // Down to where the card's corner starts to round (as the right padding above): the
    // curve (`CARD_RADIUS`, more than the bottom padding) takes the padding's corner.
    let rows = (top + pad_y) as usize..(bottom - pad_y.max(CARD_RADIUS)) as usize;
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

/// M11i floating panels, M11al's one card, from a real `Workspace` in both themes (1280 × 600,
/// the sidebar at its default width):
/// - `cards`: one pane and the right sidebar open (300 wide): the card flush against the
///   sidebar, the right sidebar in it behind a hairline (M11al §3.2).
/// - `cards-narrow`: the same in a 440 pt window, too narrow for the right sidebar.
///
/// Panics unless the card's left edge is the sidebar's right (no canvas between); the insets
/// (the card to the window's right edge, above and below it) show the canvas; between the
/// panes and the right sidebar lies a 1 px `card_border` hairline, no canvas; 20 pt in, the
/// panes show the card's fill and the right sidebar its `side`; the left sidebar has no right
/// border line; just outside the card's top-left curve is the canvas (no square tab corner
/// pokes out); and the first tab's own top-left corner is rounded inside the bar's. In
/// `cards-narrow`, unless the right sidebar isn't drawn and the card ends `CARD_INSET` from the
/// window's right edge.
fn card_scenes() {
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;
    use ui_tokens::size::{HAIRLINE, SIDEBAR_DEFAULT_WIDTH, STATUS_BAR};

    let dims = size(px(1280.0), px(600.0));
    let (left, right) = (SIDEBAR_DEFAULT_WIDTH, 1280.0_f32);
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
        // The first tab's top-left corner, as laid out.
        let first_tab: Rc<Cell<(f32, f32)>> = Rc::default();
        let seen = first_tab.clone();
        let acting = slot.clone();
        let act = move |cx: &mut VisualTestAppContext, _: AnyWindowHandle| {
            let ws = slotted(&acting);
            let width = cx.read(|cx| ws.read(cx).right_sidebar.width);
            assert_eq!(width, 300.0, "cards-{slug}: the right sidebar is 300 wide");
            let pane = shown_panes(cx, &ws)[0];
            let tab = tab_bounds(cx, &ws, pane, 0).expect("laid out");
            seen.set((f32::from(tab.left()), f32::from(tab.top())));
            acting.borrow_mut().take();
        };
        let shot = render_with(&theme, dims, build, act);
        let path = save_shot("cards", slug, &shot);
        let n = format!("cards-{slug}");
        let at = |x: f32, y: f32| pixel_at(&shot, dims, x, y);
        // The card ends `CARD_INSET` from the window's edge; the right sidebar is its last 300,
        // the 1 px hairline just left of it (M11al §3.2).
        let card_right = right - CARD_INSET;
        let panel_left = card_right - 300.0;
        let line = panel_left - HAIRLINE;
        let below = 600.0 - STATUS_BAR - CARD_INSET / 2.0;
        // Between the panes and the right sidebar: the hairline, no canvas either side.
        for y in [200.0, 400.0] {
            let p = at(line + 0.5, y);
            assert!(
                near(p, ui.card_border),
                "{n}: the hairline at ({}, {y}): {p:?}",
                line + 0.5
            );
            for x in [line - 0.5, panel_left + 0.5] {
                let p = at(x, y);
                assert!(
                    !is_canvas(p, &ui),
                    "{n}: no canvas beside the line at ({x}, {y}): {p:?}"
                );
            }
        }
        for (what, x, y) in [
            ("card → window edge", right - CARD_INSET / 2.0, 400.0),
            ("above the panes", 600.0, CARD_INSET / 2.0),
            ("above the right sidebar", 1100.0, CARD_INSET / 2.0),
            ("below the panes", 600.0, below),
            ("below the right sidebar", 1100.0, below),
        ] {
            let p = at(x, y);
            assert!(is_canvas(p, &ui), "{n}: canvas {what} at ({x}, {y}): {p:?}");
        }
        // Flush against the sidebar: its edge starts where the sidebar ends, high and low.
        for y in [100.0, 500.0] {
            let p = at(left + 0.5, y);
            assert!(
                near(p, ui.card_border),
                "{n}: the pane card's edge against the sidebar at y {y}: {p:?}"
            );
        }
        for (what, x, y, fill) in [
            ("panes", left + 20.0, 100.0, ui.card),
            ("panes, right end", line - 20.0, 400.0, ui.card),
            ("right sidebar", panel_left + 20.0, 400.0, ui.side),
            (
                "right sidebar, right end",
                card_right - 20.0,
                400.0,
                ui.side,
            ),
        ] {
            let p = at(x, y);
            assert!(
                near(p, fill),
                "{n}: {what} at ({x}, {y}): {p:?}, want {fill:?}"
            );
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
        let (card_left, card_top) = (left, CARD_INSET);
        for (dx, dy) in [(1.0, 1.0), (1.5, 1.5), (1.0, 2.0), (2.0, 1.0), (2.5, 2.5)] {
            let (x, y) = (card_left + dx, card_top + dy);
            let p = at(x, y);
            // The canvas (within the shadow's reach), nearer it than the card: the bar has no
            // fill of its own since M11al Task 5, so the card's colour is what a square bar or
            // tab corner would show here.
            assert!(
                is_canvas(p, &ui),
                "{n}: canvas outside the card's top-left curve at ({x}, {y}): {p:?}"
            );
        }
        // The first tab's own top-left corner is rounded (Task 5 review, Minor 1; M11al: all
        // its corners, `TAB_RADIUS`) and clear of the card's curve: 1.5 pt into its corner, the
        // bar shows (the card, no fill of its own), not the shown tab's `tab_active` wash.
        let (tab_left, tab_top) = first_tab.get();
        let (x, y) = (tab_left + 1.5, tab_top + 1.5);
        let p = at(x, y);
        assert!(
            near(p, ui.card) && gap(p, ui.card) < gap(p, ui.tab_active),
            "{n}: the bar inside the first tab's rounded corner at ({x}, {y}): {p:?}"
        );
        println!("ui-shots: {}", path.display());

        // A window too narrow for the right sidebar beside the panes (its drawn width 0): no
        // card of it at all (not a 2 pt stub), and the pane card reaches the window's right
        // inset (Task 4 review, M7).
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
        let edge = 440.0 - CARD_INSET - 0.5;
        let p = at(edge, 150.0);
        assert!(
            near(p, ui.card_border),
            "{n}: the pane card's right edge at {edge}: {p:?}"
        );
        for x in [440.0 - CARD_INSET / 2.0, 440.0 - 1.0] {
            let p = at(x, 150.0);
            assert!(is_canvas(p, &ui), "{n}: canvas right of it at {x}: {p:?}");
        }
        println!("ui-shots: {}", path.display());
    }
}

/// A workspace whose panes' grids are filled blue (`ESC [44m`, then clear), once each terminal
/// has its size in the card: so an unfocused pane's veil shows.
fn fill_blue(cx: &mut VisualTestAppContext, ws: &gpui::Entity<Workspace>) {
    let views: Vec<_> = cx.read(|cx| {
        ws.read(cx)
            .sessions
            .items()
            .iter()
            .map(|s| s.view.clone())
            .collect()
    });
    for view in views {
        cx.read(|cx| view.read(cx).show_text("\x1b[44m\x1b[2J"));
        cx.update(|cx| view.update(cx, |_, cx| cx.notify()));
    }
}

/// The vertical hairline of a split at `ratio` across a pane area from `left` to `right` (the
/// card's inside): its middle. The sides give up the 1 px line in proportion to their shares.
fn hairline_middle(left: f32, right: f32, ratio: f32) -> f32 {
    use ui_tokens::size::HAIRLINE;
    left + ratio * (right - left - HAIRLINE) + HAIRLINE / 2.0
}

/// M11i floating panels, a split, now inside M11al's one card, in both themes (960 × 600, the
/// sidebar at its default width), each grid blue (`fill_blue`):
/// - `cards-split`: two panes, the left one focused: the right one veiled, the left not; a 1 px
///   `card_border` hairline between them, no canvas.
/// - `cards-split-hover`: the pointer over the hairline: the 2 pt quiet line in its middle.
/// - `cards-split-refocus`: a click in the right pane: the veil moves to the left one.
///
/// Panics unless so, with no accent anywhere along the probed row (no focus ring) and the
/// card's outer edges `card_border`.
fn card_split_scenes() {
    use gpui::Modifiers;
    use std::cell::RefCell;
    use std::rc::Rc;
    use ui_tokens::size::{HAIRLINE, SIDEBAR_DEFAULT_WIDTH, UNFOCUSED_VEIL};

    let dims = size(px(960.0), px(600.0));
    let (left_card, right_end) = (SIDEBAR_DEFAULT_WIDTH, 960.0 - CARD_INSET);
    let middle = hairline_middle(left_card + HAIRLINE, right_end - HAIRLINE, 0.5);
    let (first_right, second_left) = (middle - HAIRLINE / 2.0, middle + HAIRLINE / 2.0);
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let ui = theme.ui();
        let blue = theme.palette[4];
        let veiled = crate::theme::mix(blue, ui.card, UNFOCUSED_VEIL);
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
                fill_blue(cx, &ws);
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
            let scale = shot.width as f32 / 960.0;
            let (x0, x1) = (middle - 4.0, middle + 4.0);
            for x in [middle - 3.0, middle + 3.0] {
                let p = at(x, 450.0);
                assert!(
                    !is_canvas(p, &ui),
                    "{n}: no canvas by the line at {x}: {p:?}"
                );
            }
            if scene == "cards-split-hover" {
                // The quiet line (M11x: not the accent), 2 pt, over the hairline (half
                // strength: over the card beside it, over `card_border` in its middle).
                let quiet = quiet_over_line(&shot, dims, &ui, 450.0, (x0, x1));
                assert_eq!(quiet, (2.0 * scale) as usize, "{n}: the 2 pt quiet line");
            } else {
                let lines = count_along(&shot, dims, ui.card_border, 450.0, x0, x1);
                assert_eq!(lines, (HAIRLINE * scale) as usize, "{n}: the 1 pt hairline");
            }
            // No focus ring: no accent on the card's edges or by the line (the grids are left
            // out: a theme's blue can be its accent).
            for (x0, x1) in [
                (left_card - 2.0, left_card + 4.0),
                (x0, x1),
                (right_end - 4.0, right_end + 2.0),
            ] {
                let accent = count_along(&shot, dims, ui.accent, 450.0, x0, x1);
                assert_eq!(accent, 0, "{n}: no accent from {x0} to {x1}");
            }
            // The veil: on the unfocused pane only.
            let first_focused = scene != "cards-split-refocus";
            for (which, x, focused) in [
                ("left pane", (left_card + first_right) / 2.0, first_focused),
                (
                    "right pane",
                    (second_left + right_end) / 2.0,
                    !first_focused,
                ),
            ] {
                let p = at(x, 300.0);
                if focused {
                    assert!(near(p, blue), "{n}: the {which}, focused, unveiled: {p:?}");
                } else {
                    assert!(
                        near_by(p, veiled, 4) && !near(p, blue),
                        "{n}: the {which} veiled: {p:?}, want {veiled:?}"
                    );
                }
            }
            // The card's outer edges, half a point in: `card_border`, never a ring.
            for (which, x) in [
                ("left edge", left_card + 0.5),
                ("right edge", right_end - 0.5),
            ] {
                let p = at(x, 300.0);
                assert!(near(p, ui.card_border), "{n}: the card's {which}: {p:?}");
            }
            println!("ui-shots: {}", path.display());
        }
    }
}

/// M11i Review Focus 1, in both themes (960 × 600, the sidebar at its default width):
/// `cards-corners`, a lone pane whose program filled the screen with a blue background
/// (`ESC [44m`, then clear).
///
/// Panics unless the pixel 2 pt inside each of the card's bottom corners is the canvas (the
/// terminal's background and its cells stay inside the `CARD_RADIUS` corner), the grid is blue
/// up to near the corner (the padding is all that's spared), and the card's edges are
/// `card_border` with no focus ring.
fn card_corner_scenes() {
    use std::cell::RefCell;
    use std::rc::Rc;
    use ui_tokens::size::{CARD_RADIUS, HAIRLINE, SIDEBAR_DEFAULT_WIDTH, TERMINAL_PADDING};

    let dims = size(px(960.0), px(600.0));
    // The card ends `CARD_INSET` above the status bar (M11al), flush against the sidebar.
    let bar = ui_tokens::size::STATUS_BAR;
    let (left, right, bottom) = (
        SIDEBAR_DEFAULT_WIDTH,
        960.0 - CARD_INSET,
        600.0 - bar - CARD_INSET,
    );
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
            fill_blue(cx, &ws);
            slot.borrow_mut().take();
        };
        let shot = render_with(&theme, dims, build, act);
        let path = save_shot("cards-corners", slug, &shot);
        let n = format!("cards-corners-{slug}");
        let at = |x: f32, y: f32| pixel_at(&shot, dims, x, y);
        let p = at(600.0, 300.0);
        assert!(near(p, blue), "{n}: the grid is blue: {p:?}");
        // Blue up to the rounded corners: up each side's first column (just inside the
        // padding, past the grid's half of its spare width), the blue starts within the bottom padding and a line of the card's bottom
        // (the grid's spare height is at its bottom).
        let (pad_x, pad_y) = TERMINAL_PADDING;
        for (which, x) in [
            ("left", left + HAIRLINE + pad_x + 6.0),
            ("right", right - HAIRLINE - pad_x - 6.0),
        ] {
            let from_bottom = (1..80)
                .map(|d| d as f32)
                .find(|&d| near(at(x, bottom - d), blue))
                .unwrap_or_else(|| panic!("{n}: no blue up the {which} side"));
            assert!(
                from_bottom <= HAIRLINE + pad_y + 24.0 && from_bottom > CARD_RADIUS / 2.0,
                "{n}: the grid's blue starts {from_bottom} pt above the {which} corner"
            );
        }
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

/// How many device pixels along the row at `y`, between `x0` and `x1` points, show the 2 px
/// resize line (`quiet_line`, half strength) over a split's hairline: over the card's fill
/// (at `x0`) or over the hairline (`card_border`).
pub(super) fn quiet_over_line(
    shot: &Shot,
    dims: Size<Pixels>,
    ui: &UiColors,
    y: f32,
    (x0, x1): (f32, f32),
) -> usize {
    let border = ui.card_border;
    let over_card = quiet_line(pixel_at(shot, dims, x0, y), ui);
    let over_line = quiet_line([border.r, border.g, border.b], ui);
    count_along(shot, dims, over_card, y, x0, x1) + count_along(shot, dims, over_line, y, x0, x1)
}

/// The sum of a pixel's channels: lower is darker.
fn brightness(p: [u8; 3]) -> u32 {
    p.iter().map(|&c| u32::from(c)).sum()
}

/// How many device pixels down the column at `x` points, between `y0` and `y1` points, are `c`
/// (give or take 3 per channel).
fn count_down(
    shot: &Shot,
    dims: Size<Pixels>,
    c: alacritty_terminal::vte::ansi::Rgb,
    x: f32,
    y0: f32,
    y1: f32,
) -> usize {
    let scale = shot.width as f32 / f32::from(dims.width);
    let (a, b) = ((y0 * scale) as usize, (y1 * scale) as usize);
    (a..b)
        .filter(|&dy| near(pixel_at(shot, dims, x, dy as f32 / scale), c))
        .count()
}

/// Panics unless the one card (M11al §3.1) spans `left`–`right` and `top`–`bottom` (its outer
/// edges, in points): each edge `card_border` half a point in and the card's fill 20 pt in
/// (`right_fill` inside its right edge: the card's, or the right panel's `side`, §6.1), the
/// canvas 2 pt in from each corner of its box diagonally (outside the `CARD_RADIUS` curve), and
/// 4 pt under its bottom middle darker than the canvas (its shadow).
fn check_one_card(
    shot: &Shot,
    dims: Size<Pixels>,
    ui: &UiColors,
    (left, top, right, bottom): (f32, f32, f32, f32),
    right_fill: alacritty_terminal::vte::ansi::Rgb,
    n: &str,
) {
    let at = |x: f32, y: f32| pixel_at(shot, dims, x, y);
    let (mid_x, mid_y) = ((left + right) / 2.0, (top + bottom) / 2.0);
    for (which, x, y) in [
        ("left", left + 0.5, mid_y),
        ("top", mid_x, top + 0.5),
        ("right", right - 0.5, mid_y),
        ("bottom", mid_x, bottom - 0.5),
    ] {
        let p = at(x, y);
        assert!(
            near(p, ui.card_border),
            "{n}: the card's {which} edge at ({x}, {y}): {p:?}"
        );
    }
    for (which, x, y, fill) in [
        ("left", left + 20.0, mid_y, ui.card),
        ("right", right - 20.0, mid_y, right_fill),
        ("bottom", mid_x, bottom - 4.0, ui.card),
    ] {
        let p = at(x, y);
        assert!(
            near(p, fill),
            "{n}: the fill inside the card's {which} edge at ({x}, {y}): {p:?}, want {fill:?}"
        );
    }
    for (which, x, y) in [
        ("top left", left + 2.0, top + 2.0),
        ("top right", right - 2.0, top + 2.0),
        ("bottom left", left + 2.0, bottom - 2.0),
        ("bottom right", right - 2.0, bottom - 2.0),
    ] {
        let p = at(x, y);
        assert!(
            is_canvas(p, ui),
            "{n}: canvas outside the {which} curve at ({x}, {y}): {p:?}"
        );
    }
    let canvas = ui.sidebar_background;
    let p = at(mid_x, bottom + 4.0);
    assert!(
        is_canvas(p, ui) && brightness(p) < brightness([canvas.r, canvas.g, canvas.b]),
        "{n}: the shadow 4 pt under the card at ({mid_x}, {}): {p:?} vs the canvas {canvas:?}",
        bottom + 4.0
    );
}

/// The one card's outer bounds (left, top, right, bottom) in a window `w` × `h` with the left
/// sidebar shown at its default width (M11al §3.1): flush against the sidebar, `CARD_INSET`
/// under the window's top (the drag strip), from its right and above the status bar.
fn one_card_bounds(w: f32, h: f32) -> (f32, f32, f32, f32) {
    use ui_tokens::size::{CARD_INSET, SIDEBAR_DEFAULT_WIDTH, STATUS_BAR};
    (
        SIDEBAR_DEFAULT_WIDTH,
        CARD_INSET,
        w - CARD_INSET,
        h - STATUS_BAR - CARD_INSET,
    )
}

/// M11al §3.1, §3.2, §6.1, in both themes (960 × 600, the sidebar shown at its default width):
/// `card`, one pane and the right panel (300 wide) in the one floating card. Panics unless the
/// card's left edge is the sidebar's right edge (no canvas between), its top `CARD_INSET` under
/// the window's top (the drag strip), its right `CARD_INSET` from the window's right and its
/// bottom `CARD_INSET` above the status bar (`check_one_card`: edges, corners, shadow; the
/// panel's `side` inside the right edge, its rounded corners outside); the canvas shows above
/// and right of it; and a 1 px `card_border` hairline runs the card's full height (beside the
/// panel's header and its body) between the panes (the card's fill, left of it) and the panel
/// (`side`, right of it), 300 from the card's outer right, with the panel's fill just inside
/// its rounded bottom-right corner.
fn one_card_scene() {
    use std::cell::RefCell;
    use std::rc::Rc;
    use ui_tokens::size::{CARD_INSET, HAIRLINE, SIDEBAR_DEFAULT_WIDTH, TAB_BAR};

    let dims = size(px(960.0), px(600.0));
    let (w, h) = (960.0_f32, 600.0_f32);
    let bounds = one_card_bounds(w, h);
    let (_, top, right, bottom) = bounds;
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
            let ws = slotted(&acting);
            let width = cx.read(|cx| ws.read(cx).right_sidebar.width);
            assert_eq!(width, 300.0, "card-{slug}: the right panel is 300 wide");
            acting.borrow_mut().take();
        };
        let shot = render_with(&theme, dims, build, act);
        let path = save_shot("card", slug, &shot);
        let n = format!("card-{slug}");
        let at = |x: f32, y: f32| pixel_at(&shot, dims, x, y);
        check_one_card(&shot, dims, &ui, bounds, ui.side, &n);
        // Flush against the sidebar: its last column is its own colour, the next the card's
        // edge (above), so no canvas lies between.
        let p = at(SIDEBAR_DEFAULT_WIDTH - 0.5, 300.0);
        assert!(near(p, ui.sidebar_background), "{n}: the sidebar: {p:?}");
        for (what, x, y) in [
            ("above the card", 600.0, CARD_INSET / 2.0),
            ("right of the card", w - CARD_INSET / 2.0, 300.0),
        ] {
            let p = at(x, y);
            assert!(is_canvas(p, &ui), "{n}: canvas {what} at ({x}, {y}): {p:?}");
        }
        // The hairline: the panel runs to the card's outer right, 300 wide, the line just
        // left of it, from the card's top edge to its bottom edge (beside the header too).
        let line = right - 300.0 - HAIRLINE;
        for y in [
            top + HAIRLINE + TAB_BAR / 2.0,
            top + TAB_BAR + 40.0,
            300.0,
            bottom - 20.0,
        ] {
            let p = at(line + 0.5, y);
            assert!(
                near(p, ui.card_border),
                "{n}: the hairline at ({}, {y}): {p:?}",
                line + 0.5
            );
            // Exactly 1 px: not the line's colour either side of it.
            for x in [line - 0.5, line + HAIRLINE + 0.5] {
                let p = at(x, y);
                assert!(
                    !near(p, ui.card_border) || near(p, ui.card) || near(p, ui.side),
                    "{n}: the hairline is 1 px: ({x}, {y}) {p:?}"
                );
            }
        }
        for y in [300.0, bottom - 20.0] {
            let p = at(line - 0.5, y);
            assert!(
                near(p, ui.card),
                "{n}: the panes left of the line at y {y}: {p:?}"
            );
            let p = at(line + HAIRLINE + 0.5, y);
            assert!(
                near(p, ui.side),
                "{n}: the panel's side right of it at y {y}: {p:?}"
            );
        }
        // Inside the panel's rounded bottom-right corner (radius `CARD_RADIUS` − 1 inside the
        // card's edge): its fill, not the card's.
        let (x, y) = (right - 6.0, bottom - 6.0);
        let p = at(x, y);
        assert!(
            near(p, ui.side),
            "{n}: the panel's fill in its bottom-right corner at ({x}, {y}): {p:?}"
        );
        println!("ui-shots: {}", path.display());
    }
}

/// M11al §6.7, §10 "Right panel hidden", in both themes (960 × 600, the sidebar shown):
/// `card-right-hidden`, one pane, the right panel closed. Panics unless the card keeps its
/// bounds and insets (`check_one_card`, the card's fill inside its right edge: the canvas
/// outside its right corners' curves), the terminal owns its right corners (the card's fill
/// just inside the bottom-right curve and down its right edge under the tab bar), and there's
/// no hairline or `side` where the panel's would be.
fn one_card_right_hidden_scene() {
    use ui_tokens::size::{HAIRLINE, TAB_BAR};

    let dims = size(px(960.0), px(600.0));
    let (w, h) = (960.0_f32, 600.0_f32);
    let bounds = one_card_bounds(w, h);
    let (_, top, right, bottom) = bounds;
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let ui = theme.ui();
        let build = move |window: &mut Window, cx: &mut App| -> AnyView {
            cx.set_global(shot_appearance(theme, cx));
            cx.new(|cx| {
                let mut ws = card_workspace(1, "", window, cx);
                ws.right_sidebar.hidden = true;
                ws
            })
            .into()
        };
        let shot = render_with(&theme, dims, build, |_, _| ());
        let path = save_shot("card-right-hidden", slug, &shot);
        let n = format!("card-right-hidden-{slug}");
        let at = |x: f32, y: f32| pixel_at(&shot, dims, x, y);
        check_one_card(&shot, dims, &ui, bounds, ui.card, &n);
        let line = right - 300.0 - HAIRLINE;
        for (what, x, y) in [
            ("inside the bottom-right curve", right - 6.0, bottom - 6.0),
            ("down the right edge", right - 3.0, top + TAB_BAR + 20.0),
            ("down the right edge, low", right - 3.0, bottom - 20.0),
            ("where the hairline would be", line + 0.5, 300.0),
            ("where the panel would be", right - 150.0, 300.0),
        ] {
            let p = at(x, y);
            assert!(
                near(p, ui.card),
                "{n}: the terminal {what} at ({x}, {y}): {p:?}"
            );
        }
        println!("ui-shots: {}", path.display());
    }
}

/// M11al §10 "Narrow window", in both themes (440 × 300, the sidebar shown at its default
/// width, the right panel open): `card-narrow`, a window where `drawn_width` is 0. Panics
/// unless the card keeps its bounds (`check_one_card`, the card's fill inside its right edge)
/// and, across its middle row from its left edge to its right, there is no hairline and no
/// panel: the card's fill only.
fn one_card_narrow_scene() {
    use crate::right_sidebar::drawn_width;
    use ui_tokens::size::{HAIRLINE, SIDEBAR_DEFAULT_WIDTH};

    let dims = size(px(440.0), px(300.0));
    let (w, h) = (440.0_f32, 300.0_f32);
    assert_eq!(
        drawn_width(300.0, w, SIDEBAR_DEFAULT_WIDTH),
        0.0,
        "card-narrow: a window too narrow for the panel"
    );
    let bounds = one_card_bounds(w, h);
    let (left, top, right, bottom) = bounds;
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let ui = theme.ui();
        let build = move |window: &mut Window, cx: &mut App| -> AnyView {
            cx.set_global(shot_appearance(theme, cx));
            cx.new(|cx| {
                let mut ws = card_workspace(1, "", window, cx);
                ws.right_sidebar.hidden = false;
                ws
            })
            .into()
        };
        let shot = render_with(&theme, dims, build, |_, _| ());
        let path = save_shot("card-narrow", slug, &shot);
        let n = format!("card-narrow-{slug}");
        check_one_card(&shot, dims, &ui, bounds, ui.card, &n);
        let y = (top + bottom) / 2.0 + 20.0;
        let mut x = left + HAIRLINE + 2.0;
        while x < right - HAIRLINE - 2.0 {
            let p = pixel_at(&shot, dims, x, y);
            assert!(
                near(p, ui.card),
                "{n}: the card's fill only at ({x}, {y}): {p:?}"
            );
            x += 1.0;
        }
        println!("ui-shots: {}", path.display());
    }
}

/// M11al §6.7, in both themes (960 × 600, the sidebar shown): `card-slide`, the right panel
/// part-way through sliding in (its slide set 20 ms back: about a third shown). Panics unless
/// the panes keep their final width at once (the card's fill from their final right edge to
/// the sliding hairline); the hairline (1 px `card_border`) lies between there and the card's
/// right edge; the panel's `side` shows right of it up to the card's right edge, which is
/// still `card_border`; and nothing of the panel shows past the card (the canvas right of it)
/// or on the canvas outside the card's rounded right corners (the canvas there, nearer it than
/// the panel's fill).
fn one_card_slide_scene() {
    use crate::sidebar_toggle::Slide;
    use std::time::{Duration, Instant};
    use ui_tokens::size::{CARD_INSET, HAIRLINE};

    let dims = size(px(960.0), px(600.0));
    let (w, h) = (960.0_f32, 600.0_f32);
    let bounds = one_card_bounds(w, h);
    let (_, top, right, bottom) = bounds;
    // Where the panes end (the final hairline's left), as soon as the slide starts.
    let panes_right = right - 300.0 - HAIRLINE;
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let ui = theme.ui();
        let build = move |window: &mut Window, cx: &mut App| -> AnyView {
            cx.set_global(shot_appearance(theme, cx));
            cx.new(|cx| {
                let mut ws = card_workspace(1, "", window, cx);
                ws.right_sidebar.hidden = false;
                // Real time (`Instant`): of the 160 ms `SLIDE`, 20 ms in is about a third
                // shown; the checks hold anywhere from a few points to most of it shown.
                ws.right_sidebar.slide = Some(Slide {
                    showing: true,
                    since: Instant::now() - Duration::from_millis(20),
                    from: 0.0,
                });
                ws
            })
            .into()
        };
        let shot = render_with(&theme, dims, build, |_, _| ());
        let path = keep_shot(&format!("card-slide-{slug}"), &shot);
        println!("ui-shots: {}", path.display());
        let n = format!("card-slide-{slug}");
        let at = |x: f32, y: f32| pixel_at(&shot, dims, x, y);
        let y = 300.0;
        // The sliding hairline: the first `card_border` column right of the panes' final edge.
        let line = (0..300)
            .map(|i| panes_right + 0.5 + i as f32)
            .find(|&x| near(at(x, y), ui.card_border))
            .unwrap_or_else(|| panic!("{n}: no hairline between {panes_right} and {right}"));
        println!("ui-shots: {n}: the hairline at {line}");
        assert!(
            line > panes_right + 2.0 && line < right - 24.0,
            "{n}: the panel part-way in (hairline at {line}, {panes_right} when in)"
        );
        // The panes took their final width at once: the card's fill from their edge to the
        // line, none of it the panel's.
        let mut x = panes_right - 20.0;
        while x < line - 1.0 {
            let p = at(x, y);
            assert!(
                near(p, ui.card),
                "{n}: the card's fill at ({x}, {y}): {p:?}"
            );
            x += 1.0;
        }
        // The panel: its fill right of the line, cut at the card's right edge.
        for (what, x, y) in [
            ("beside the line", line + 1.5, y),
            ("at the card's right edge", right - 20.0, y),
            ("low, at the card's right edge", right - 20.0, bottom - 20.0),
        ] {
            let p = at(x, y);
            assert!(
                near(p, ui.side),
                "{n}: the panel's fill {what} at ({x}, {y}): {p:?}"
            );
        }
        let p = at(right - 0.5, y);
        assert!(near(p, ui.card_border), "{n}: the card's right edge: {p:?}");
        // Nothing of it past the card, or outside the card's rounded right corners.
        for (what, x, y) in [
            ("right of the card", right + CARD_INSET / 2.0, y),
            ("right of the card, low", right + 2.0, bottom - 20.0),
            ("outside the top-right curve", right - 2.0, top + 2.0),
            (
                "outside the top-right curve, nearer",
                right - 1.5,
                top + 1.5,
            ),
            ("outside the bottom-right curve", right - 2.0, bottom - 2.0),
            (
                "outside the bottom-right curve, nearer",
                right - 1.5,
                bottom - 1.5,
            ),
        ] {
            let p = at(x, y);
            assert!(
                is_canvas(p, &ui) && gap(p, ui.sidebar_background) < gap(p, ui.side),
                "{n}: canvas {what} at ({x}, {y}): {p:?}"
            );
        }
    }
}

/// M11al §3.1 with the sidebar hidden (⌘B), in both themes (960 × 600): `card-sidebar-hidden`.
/// Panics unless the card starts `CARD_INSET` from the window's left, right under the window's
/// top band (`TOP_BAND`), and keeps its insets on the right and above the status bar
/// (`check_one_card`); the canvas shows left of it and in the band.
fn one_card_sidebar_hidden_scene() {
    use ui_tokens::size::{CARD_INSET, STATUS_BAR, TOP_BAND};

    let dims = size(px(960.0), px(600.0));
    let (w, h) = (960.0_f32, 600.0_f32);
    let bounds = (
        CARD_INSET,
        TOP_BAND,
        w - CARD_INSET,
        h - STATUS_BAR - CARD_INSET,
    );
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let ui = theme.ui();
        let build = move |window: &mut Window, cx: &mut App| -> AnyView {
            cx.set_global(shot_appearance(theme, cx));
            cx.new(|cx| {
                let mut ws = card_workspace(1, "", window, cx);
                ws.sidebar.hidden = true;
                ws
            })
            .into()
        };
        let shot = render_with(&theme, dims, build, |_, _| ());
        let path = save_shot("card-sidebar-hidden", slug, &shot);
        let n = format!("card-sidebar-hidden-{slug}");
        let at = |x: f32, y: f32| pixel_at(&shot, dims, x, y);
        check_one_card(&shot, dims, &ui, bounds, ui.card, &n);
        // Left of the card, and the band above it (away from the name centred in it and the
        // buttons at its right).
        for (what, x, y) in [
            ("left of the card", CARD_INSET / 2.0, 300.0),
            ("in the band", 200.0, TOP_BAND / 2.0),
            ("in the band, just above the card", 200.0, TOP_BAND - 2.0),
        ] {
            let p = at(x, y);
            assert!(is_canvas(p, &ui), "{n}: canvas {what} at ({x}, {y}): {p:?}");
        }
        println!("ui-shots: {}", path.display());
    }
}

/// M11al §3.3, in both themes (960 × 600, the sidebar shown): a 2 × 2 split in the one card,
/// the top-left pane focused, each grid filled blue (`ESC [44m`, then clear) so the veil shows:
/// - `card-split`: quiet;
/// - `card-split-hover`: the pointer over the vertical line.
///
/// Panics unless between the panes lies a 1 px `card_border` hairline (down the middle, and
/// across each column) with no canvas beside it; the three unfocused panes are veiled (their
/// blue `UNFOCUSED_VEIL` toward the card) and the focused one isn't; there is no accent (no
/// focus ring) anywhere along the probed rows; and hovered, the 2 px quiet line covers the
/// hairline, centred on it.
fn one_card_split_scenes() {
    use gpui::Modifiers;
    use std::cell::RefCell;
    use std::rc::Rc;
    use ui_tokens::size::{
        CARD_INSET, HAIRLINE, SIDEBAR_DEFAULT_WIDTH, STATUS_BAR, UNFOCUSED_VEIL,
    };

    let dims = size(px(960.0), px(600.0));
    let (w, h) = (960.0_f32, 600.0_f32);
    // Inside the card's 1 px edge.
    let (left, top) = (SIDEBAR_DEFAULT_WIDTH + HAIRLINE, CARD_INSET + HAIRLINE);
    let (right, bottom) = (
        w - CARD_INSET - HAIRLINE,
        h - STATUS_BAR - CARD_INSET - HAIRLINE,
    );
    // Each split at 0.5: the sides give up the hairline in proportion.
    let across = left + 0.5 * (right - left - HAIRLINE) + HAIRLINE / 2.0;
    let down = top + 0.5 * (bottom - top - HAIRLINE) + HAIRLINE / 2.0;
    // Each pane's middle, below its tab bar: top left (focused), top right, bottom left, bottom
    // right.
    let bar = ui_tokens::size::TAB_BAR;
    let middles = [
        ((left + across) / 2.0, (top + bar + down) / 2.0),
        ((across + right) / 2.0, (top + bar + down) / 2.0),
        ((left + across) / 2.0, (down + bar + bottom) / 2.0),
        ((across + right) / 2.0, (down + bar + bottom) / 2.0),
    ];
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let ui = theme.ui();
        let blue = theme.palette[4];
        for scene in ["card-split", "card-split-hover"] {
            let slot: Slot = Rc::new(RefCell::new(None));
            let keep = slot.clone();
            let build = move |window: &mut Window, cx: &mut App| -> AnyView {
                cx.set_global(shot_appearance(theme, cx));
                let ws = cx.new(|cx| {
                    let mut ws = Workspace::for_shots(window, cx);
                    ws.sidebar.hint_dismissed = true;
                    let project = std::path::Path::new(API);
                    // pane0 | pane1, then pane2 under pane0 and pane3 under pane1.
                    let mut ids = Vec::new();
                    for (i, axis, from) in [
                        (0, None, None),
                        (1, Some(Axis::Row), None),
                        (2, Some(Axis::Column), Some(0)),
                        (3, Some(Axis::Column), Some(1)),
                    ] {
                        if let Some(from) = from {
                            ws.select_pane(ids[from], window, cx);
                        }
                        ws.open_shot_session(&format!("pane{i}"), project, "", axis, window, cx);
                        ids.push(ws.model.focused_pane().expect("a pane"));
                    }
                    ws.select_pane(ids[0], window, cx);
                    ws
                });
                *keep.borrow_mut() = Some(ws.clone());
                ws.into()
            };
            // Each pane's (one) tab's place, in layout order.
            let places: Rc<RefCell<Vec<gpui::Bounds<Pixels>>>> = Rc::default();
            let seen = places.clone();
            let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let ws = slotted(&slot);
                let panes = shown_panes(cx, &ws);
                assert_eq!(panes.len(), 4, "{scene}-{slug}: four panes");
                *seen.borrow_mut() = panes
                    .iter()
                    .map(|&p| tab_bounds(cx, &ws, p, 0).expect("laid out"))
                    .collect();
                fill_blue(cx, &ws);
                if scene == "card-split-hover" {
                    let on = point(px(across), px(middles[3].1));
                    cx.simulate_mouse_move(window, on, None, Modifiers::none());
                }
                slot.borrow_mut().take();
            };
            let shot = render_with(&theme, dims, build, act);
            let path = save_shot(scene, slug, &shot);
            let n = format!("{scene}-{slug}");
            let at = |x: f32, y: f32| pixel_at(&shot, dims, x, y);
            let scale = shot.width as f32 / w;
            let one = (HAIRLINE * scale) as usize;
            // The veil: the focused pane's grid is blue; the others' the blue veiled toward the
            // card.
            let veiled = crate::theme::mix(blue, ui.card, UNFOCUSED_VEIL);
            for (i, &(x, y)) in middles.iter().enumerate() {
                let p = at(x, y);
                if i == 0 {
                    assert!(near(p, blue), "{n}: the focused pane, unveiled: {p:?}");
                } else {
                    assert!(
                        near_by(p, veiled, 4) && !near(p, blue),
                        "{n}: pane {i} veiled at ({x}, {y}): {p:?}, want {veiled:?}"
                    );
                }
            }
            // Focus by the tabs (§3.3): the focused pane's shown tab has the accent wash, the
            // others' a neutral one (`card_hover`); read in each tab's right padding.
            let after = 2.0 * crate::tab_bar::SEPARATOR_ROOM + crate::tab_bar::SEPARATOR.0;
            for (i, place) in places.borrow().iter().enumerate() {
                let x = f32::from(place.right()) - after - crate::tab_bar::TAB_PAD / 2.0;
                let y = f32::from(place.center().y);
                let (want, what) = if i == 0 {
                    (ui.tab_active, "tab_active")
                } else {
                    (ui.card_hover, "card_hover")
                };
                let p = at(x, y);
                assert!(near(p, want), "{n}: pane {i}'s shown tab is {what}: {p:?}");
            }
            // No focus ring: no accent on the card's edges or by the lines, along a row through
            // each pair of panes (the grids are left out: a theme's blue can be its accent).
            for (_, y) in [middles[0], middles[2]] {
                for (x0, x1) in [
                    (left - 2.0, left + 3.0),
                    (across - 4.0, across + 4.0),
                    (right - 3.0, right + 2.0),
                ] {
                    let accent = count_along(&shot, dims, ui.accent, y, x0, x1);
                    assert_eq!(accent, 0, "{n}: no accent along y {y} from {x0} to {x1}");
                }
            }
            // The horizontal hairlines, one per column: 1 px of `card_border`, the upper pane
            // (no canvas) above it and the lower pane's tab bar directly under it (§3.3).
            for x in [middles[0].0, middles[1].0] {
                let lines = count_down(&shot, dims, ui.card_border, x, down - 4.0, down + 4.0);
                assert_eq!(lines, one, "{n}: one hairline across the column at x {x}");
                let p = at(x, down - 3.0);
                assert!(
                    !is_canvas(p, &ui),
                    "{n}: no canvas above the line at x {x}: {p:?}"
                );
                // The lower pane's bar right under the line: no fill (the card), and its own
                // `card_border` line `TAB_BAR` further down (M11al §4.1).
                let y = down + HAIRLINE / 2.0 + 0.5;
                let p = at(x, y);
                assert!(
                    near(p, ui.card),
                    "{n}: the bar (the card) right under the line at ({x}, {y}): {p:?}"
                );
                let bar_line = down + HAIRLINE / 2.0 + bar - HAIRLINE / 2.0;
                let p = at(x, bar_line);
                assert!(
                    near(p, ui.card_border),
                    "{n}: the lower bar's bottom line at ({x}, {bar_line}): {p:?}"
                );
            }
            // The vertical one, in the lower panes (clear of the horizontal lines).
            let y = middles[3].1;
            let (x0, x1) = (across - 4.0, across + 4.0);
            for x in [across - 3.0, across + 3.0] {
                let p = at(x, y);
                assert!(
                    !is_canvas(p, &ui),
                    "{n}: no canvas by the line at ({x}, {y})"
                );
            }
            if scene == "card-split-hover" {
                // The 2 px quiet line (M11x) over it, centred: the hairline is covered (the
                // line at half strength over it).
                let quiet = quiet_over_line(&shot, dims, &ui, y, (x0, x1));
                assert_eq!(quiet, (2.0 * scale) as usize, "{n}: the 2 px quiet line");
                let lines = count_along(&shot, dims, ui.card_border, y, x0, x1);
                assert_eq!(lines, 0, "{n}: the hairline under the quiet line");
                let over_card = quiet_line(at(x0, y), &ui);
                let first = (0..(8.0 * scale) as usize)
                    .find(|&d| near(at(x0 + d as f32 / scale, y), over_card))
                    .expect("the line");
                let middle = x0 + (first as f32 + scale) / scale;
                assert!(
                    (middle - across).abs() <= 0.5,
                    "{n}: the line's middle {middle}, the hairline's {across}"
                );
            } else {
                let lines = count_along(&shot, dims, ui.card_border, y, x0, x1);
                assert_eq!(lines, one, "{n}: one hairline down the middle");
            }
            println!("ui-shots: {}", path.display());
        }
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

    use crate::sidebar::FOOTER;
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
            // The 200 pt windows with the sidebar shown are taller by the sidebar chrome's growth
            // (M11al Task 7): its band, search box and footer would not fit above the status
            // bar, and the footer would be pushed down over it.
            let short = 200.0 + SIDEBAR_CHROME_GROWTH;
            let dims = match scene {
                "title-narrow" => size(px(560.0), px(200.0)),
                "title-footer-narrow" => size(px(360.0), px(short)),
                "title-popover" | "title-popover-empty" => size(px(960.0), px(480.0)),
                "title-popover-narrow" => size(px(360.0), px(480.0)),
                _ => size(px(760.0), px(short)),
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
            // The footer's buttons sit centred in it (its 1 px top line is drawn over its
            // padding, M11al Task 7), and the footer sits on the status bar (M11i).
            let centre_y = height - STATUS_BAR - FOOTER / 2.0;
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
                        // one line, the bell and gear centred in their `footer_buttons` slots,
                        // the + centred in the New button's padding (its left edge as far from
                        // the sidebar's left edge as the gear's box is from its right edge: the
                        // footer's inset). Every device pixel of a button is read, so
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
                        // edge) is where it is drawn: the ink's centre is the slot's. The +
                        // is inside the New button: its 1 px edge and 12 pt padding in, the
                        // 15 pt icon's middle 7.5 further.
                        let wants = [
                            plus_x + 1.0 + 12.0 + 7.5,
                            bell_x + FOOTER_BUTTON / 2.0,
                            gear_x + FOOTER_BUTTON / 2.0,
                        ];
                        for ((name, want), b) in ["+", "bell", "gear"].iter().zip(wants).zip(boxes)
                        {
                            let mid = (b.0 + b.1) / 2.0;
                            assert!(
                                (mid - want).abs() <= 0.5,
                                "{n}: the {name}'s ink is centred at {mid}, its slot at {want}"
                            );
                        }
                        let (left, right) = (plus_x, sidebar_width - (gear_x + FOOTER_BUTTON));
                        assert!(
                            (left - right).abs() <= 0.5,
                            "{n}: the button's box is {left} from the left, the gear's {right} \
                             from the right"
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
            let inset = name_inset(false);
            if hidden {
                let stop = inset - 2.0;
                for y in [sy + 2.0, sy + HIT / 2.0, sy + HIT - 2.0] {
                    let mut x = 0.0;
                    while x < stop {
                        // (The canvas, within the shadow's slack: the card's shadow reaches a
                        // little way up into the band.)
                        let p = at(x, y);
                        assert!(
                            is_canvas(p, &ui),
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
                            // (Not the canvas: its shade under the card's shadow isn't ink.)
                            inked += usize::from(!is_canvas(at(x, y), &ui));
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
                // The pane tab bars start under the band: the bar (no fill since M11al, the
                // card) ends `TAB_BAR` under the card's top edge with its `card_border` line.
                let line = ui_tokens::size::TOP_BAND
                    + ui_tokens::size::HAIRLINE
                    + ui_tokens::size::TAB_BAR
                    - 0.5;
                let p = at(width / 2.0, line);
                assert!(
                    near(p, ui.card_border),
                    "{n}: a bar's line at {line}: {p:?}"
                );
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

    // A window narrower than the sidebar (200 pt, the default 290 pt sidebar): the open button
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
            // The bare band at a point: the canvas, give or take the card's shadow (M11al: it
            // reaches a little way up into the band), and nearer it than `other`.
            let on_band = |p: [u8; 3], other: alacritty_terminal::vte::ansi::Rgb| {
                gap(p, ui.sidebar_background) <= SHADOW_SLACK
                    && gap(p, ui.sidebar_background) < gap(p, other)
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
                    let p = at(b[0] + 1.5, 19.0);
                    assert!(on_band(p, ui.panel_hover), "{n}: 🔔 not hovered: {p:?}");
                }
                "title-bar-collapsed-hover-sidebar" => {
                    assert_eq!(
                        at(b[2] + 1.5, 19.0),
                        bg(ui.panel_hover),
                        "{n}: the sidebar button hovered"
                    );
                    let p = at(b[1] + 1.5, 19.0);
                    assert!(on_band(p, ui.panel_hover), "{n}: ⚙ not hovered: {p:?}");
                }
                "title-bar-unseen" => {
                    assert_eq!(at(dot.0, dot.1), bg(ui.accent), "{n}: the dot");
                    let p = at(dot.0 + DOT / 2.0 - 1.0, dot.1);
                    assert!(on_band(p, ui.accent), "{n}: its ring: {p:?}");
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
                    // At x 150, the pane's tab bar above its tab (M11al: no fill, the card,
                    // the terminal's colour; the 52 pt bar fills this short window's card).
                    let bar_above_tab = TOP_BAND
                        + ui_tokens::size::HAIRLINE
                        + (ui_tokens::size::TAB_BAR - crate::tab_bar::TAB) / 4.0;
                    let p = at(150.0, bar_above_tab);
                    assert_eq!(p, bg(theme.background), "{n}: slid out");
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
            // an unlit slot, which differs by nothing, fails on all three). The shadow also
            // darkens the difference itself (the canvas is darker since M11al, so it is a bigger
            // one in Mocha: 37, 38, 46 want, 35, 36, 45 got): up to an eighth of it more.
            // M11al: the card's soft shadow reaches up into the band too, under the bare band
            // beside the slot but not under the slot's fill (drawn over it): what it takes off
            // the band, measured at the same height away from the popover, is allowed on top.
            let fill = at(b[0] + 1.5, 19.0);
            let beside = at(b[0] - 3.0, 19.0);
            let (hover, band) = (bg(ui.panel_hover), bg(ui.sidebar_background));
            let card_shade = at(w * 0.25, 19.0);
            let off = (0..3).all(|k| {
                let got = i32::from(fill[k]) - i32::from(beside[k]);
                let want = i32::from(hover[k]) - i32::from(band[k]);
                let shade = (i32::from(band[k]) - i32::from(card_shade[k])).max(0);
                (got - want).abs() <= 1 + want.abs() / 8 + shade
                    && (want == 0 || got.signum() == want.signum())
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
            // In the band's top strip: the title band with the sidebar hidden, the drag strip
            // above the card with it shown (M11al: the 52 pt tab bar, and its wider tabs, reach
            // up to `CARD_INSET` there, and a press on a tab is the tab's, not a drag).
            let on_band = point(px(width / 2.0), px(CARD_INSET / 2.0));
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
        // With the sidebar shown there are no band buttons: a click over where the band's
        // sidebar button would be (in the strip above the card, `CARD_INSET / 2` down: the
        // slot's middle, at `TOP_BAND / 2`, is now inside the pane tab bar's own ◧, a 28 pt
        // button in the 52 pt bar's middle) leaves this one shown. The ◧ opens the right
        // sidebar and leaves this one shown.
        let right = |cx: &mut VisualTestAppContext| cx.read(|cx| ws.read(cx).right_sidebar.hidden);
        assert!(right(cx), "title-bar: the right sidebar starts hidden");
        let slot_x = band_buttons(960.0)[2] + HIT / 2.0;
        cx.simulate_click(window, point(px(slot_x), px(CARD_INSET / 2.0)), none);
        assert!(
            !hidden(cx),
            "title-bar: no band sidebar button while the sidebar is shown"
        );
        let toggle = point(
            px(960.0
                - CARD_INSET
                - ui_tokens::size::HAIRLINE
                - crate::tab_bar::BUTTONS_PAD
                - crate::tab_bar::BUTTON / 2.0),
            px(CARD_INSET + ui_tokens::size::HAIRLINE + ui_tokens::size::TAB_BAR / 2.0),
        );
        assert!(right(cx), "title-bar: the band's slot is not the ◧");
        cx.simulate_click(window, toggle, none);
        assert!(
            !hidden(cx),
            "title-bar: the tab bar's ◧ leaves the sidebar shown"
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
            // (Give or take the card's shadow, which reaches a little way up into the band since
            // M11al, and nearer the band than the hover fill.)
            let corner = pixel_at(&shot, size(px(960.0), px(600.0)), b[0] + 1.5, 19.0);
            let band = ui.sidebar_background;
            assert!(
                gap(corner, band) <= SHADOW_SLACK
                    && gap(corner, band) < gap(corner, ui.panel_hover),
                "{n}: after Escape the 🔔's fill is gone: {corner:?}"
            );
            // A click on the sidebar button shows the sidebar.
            cx.simulate_click(window, point(px(b[2] + HIT / 2.0), px(y)), none);
            assert!(!hidden(cx), "{n}: the sidebar button shows the sidebar");
            slot.borrow_mut().take();
        };
        render_with(&theme, size(px(960.0), px(600.0)), build, act);
    }
}

struct Panel(theme::UiColors);

impl Render for Panel {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let c = self.0;
        div()
            .flex()
            .size_full()
            .child(div().w(px(110.0)).h_full().bg(hsla(c.sidebar_background)))
            .child(
                div()
                    .flex_1()
                    .p(px(12.0))
                    .child(div().h(px(28.0)).bg(hsla(c.row_active)))
                    .child(div().mt(px(8.0)).h(px(28.0)).bg(hsla(c.ring_waiting))),
            )
    }
}
