//! Off-screen UI screenshots (M11), compiled only with the `ui-shots` cargo feature.
//!
//! `shoot` renders a view in a hidden GPUI window and saves it as `target/ui-shots/<name>-<slug>.png`,
//! so each UI task can look at its scenes in Catppuccin Mocha and Catppuccin Latte.
//!
//! AppKit only makes windows on the main thread and `cargo test` runs every test on a spawned one
//! (it aborts with "Rust cannot catch foreign exceptions"), so the scenes are not `#[test]`s.
//! They are plain functions listed in `SCENES` and run from `main` on the main thread:
//! `cargo run --features ui-shots -- __ui-shots [filter]`. A scene panics when its checks fail.

use std::path::PathBuf;
use std::process::exit;

use gpui::VisualTestAppContext;
use gpui::{
    AnyView, AnyWindowHandle, App, BoxShadow, Context, Div, Font, Hsla, IntoElement, Pixels,
    Render, Size, Window, div, point, prelude::*, px, size,
};
use gpui_platform::current_platform;

use crate::appearance::Appearance;
use crate::restore::RestoreMode;
use crate::split_tree::Axis;
use crate::status::Status;
use crate::status_mark::{self, BREATHS_PER_BEAT, TURNS_PER_BEAT};
use crate::terminal_view::hsla;
use crate::theme::{self, Theme, UiColors};
use crate::ui_tokens::{self, contrast};
use crate::workspace::{Overlay, Workspace};
use crate::{fonts, settings};

/// The two themes every scene is shot in: (file slug, built-in theme name).
pub const SHOT_THEMES: [(&str, &str); 2] =
    [("mocha", "Catppuccin Mocha"), ("latte", "Catppuccin Latte")];

/// A built-in theme resolved the way the app resolves `theme = <name>`.
pub fn shot_theme(name: &str) -> Theme {
    let text = theme::builtin_theme(name).expect("a built-in theme");
    let mut t = Theme::default();
    theme::apply(&mut t, &settings::parse(text));
    t
}

/// A rendered scene: size in device pixels and RGBA bytes, row by row.
pub struct Shot {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// The window's root: the theme's background with the scene on top.
struct Root {
    theme: Theme,
    scene: AnyView,
}

impl Render for Root {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .bg(hsla(self.theme.background))
            .child(self.scene.clone())
    }
}

/// Render `build`'s view at `size` (in points) in a hidden window.
pub fn render(
    theme: &Theme,
    size: Size<Pixels>,
    build: impl FnOnce(&mut Window, &mut App) -> AnyView,
) -> Shot {
    render_with(theme, size, build, |_, _| ())
}

/// `render`, after `act` has had the window (to move the mouse over it, click, drag, or check
/// what it drew) and it has settled again.
pub fn render_with(
    theme: &Theme,
    size: Size<Pixels>,
    build: impl FnOnce(&mut Window, &mut App) -> AnyView,
    act: impl FnOnce(&mut VisualTestAppContext, AnyWindowHandle),
) -> Shot {
    let mut cx = gpui::VisualTestAppContext::new(current_platform(false));
    // As the app does at start (appearance.rs), so the bundled fonts are there to shape text with.
    fonts::register_bundled_fonts(&cx.text_system().all_font_names());
    let theme = *theme;
    let window = cx
        .open_offscreen_window(size, move |window, cx| {
            let scene = build(window, cx);
            cx.new(|_| Root { theme, scene })
        })
        .expect("open an off-screen window");
    cx.run_until_parked();
    act(&mut cx, window.into());
    cx.run_until_parked();
    let image = cx
        .capture_screenshot(window.into())
        .expect("capture the window");
    Shot {
        width: image.width(),
        height: image.height(),
        rgba: image.as_raw().to_vec(),
    }
}

/// Render and save `target/ui-shots/<name>-<slug>.png`; returns the path.
pub fn shoot(
    name: &str,
    slug: &str,
    theme: &Theme,
    size: Size<Pixels>,
    build: impl FnOnce(&mut Window, &mut App) -> AnyView,
) -> PathBuf {
    shoot_with(name, slug, theme, size, build, |_, _| ())
}

/// `shoot`, after `act` (see `render_with`).
pub fn shoot_with(
    name: &str,
    slug: &str,
    theme: &Theme,
    size: Size<Pixels>,
    build: impl FnOnce(&mut Window, &mut App) -> AnyView,
    act: impl FnOnce(&mut VisualTestAppContext, AnyWindowHandle),
) -> PathBuf {
    let shot = render_with(theme, size, build, act);
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("ui-shots");
    std::fs::create_dir_all(&dir).expect("create target/ui-shots");
    let path = dir.join(format!("{name}-{slug}.png"));
    std::fs::write(&path, png(&shot)).expect("write the PNG");
    path
}

/// A minimal PNG: 8-bit RGBA, no filtering, stored (uncompressed) deflate blocks. The `image`
/// crate that GPUI uses isn't a direct dependency, and screenshots don't need to be small.
fn png(shot: &Shot) -> Vec<u8> {
    let row = shot.width as usize * 4;
    let mut raw = Vec::with_capacity((row + 1) * shot.height as usize);
    for line in shot.rgba.chunks(row) {
        raw.push(0); // filter type: none
        raw.extend_from_slice(line);
    }
    let mut z = vec![0x78, 0x01];
    let mut blocks = raw.chunks(65535).peekable();
    while let Some(block) = blocks.next() {
        z.push(blocks.peek().is_none() as u8); // final-block flag, stored type
        z.extend_from_slice(&(block.len() as u16).to_le_bytes());
        z.extend_from_slice(&(!(block.len() as u16)).to_le_bytes());
        z.extend_from_slice(block);
    }
    z.extend_from_slice(&adler32(&raw).to_be_bytes());

    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&shot.width.to_be_bytes());
    ihdr.extend_from_slice(&shot.height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]); // 8-bit, RGBA, deflate, no filter, no interlace
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(&mut out, b"IDAT", &z);
    chunk(&mut out, b"IEND", &[]);
    out
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let start = out.len();
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let crc = crc32(&out[start..]);
    out.extend_from_slice(&crc.to_be_bytes());
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &byte in data {
        a = (a + byte as u32) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & (!(crc & 1)).wrapping_add(1));
        }
    }
    !crc
}

/// The terminal's font as the app builds it with no font settings (bundled JetBrains Mono).
pub fn mono_font(cx: &App) -> Font {
    fonts::gpui_font(
        &fonts::from_entries(&[]),
        &cx.text_system().all_font_names(),
    )
}

/// Every scene: (name, function). A UI task adds its scenes here.
const SCENES: &[(&str, fn())] = &[
    ("smoke", smoke),
    ("tokens", tokens),
    ("window", window_scenes),
    ("marks", marks),
    ("sidebar-marks", sidebar_marks),
    ("cached-panes", cached_panes),
    ("sidebar", sidebar_scenes),
    ("sidebar-rows", sidebar_row_scenes),
    ("sidebar-button", sidebar_button_scenes),
    ("title-area", title_area_scenes),
    ("tabs", tab_scenes),
    ("splits", split_scenes),
    ("workspaces", workspace_scenes),
    ("tab-bars", tab_bar_scenes),
    ("tab-drag", tab_drag_scenes),
    ("browser-tab", browser_tab_scenes),
    ("file-tab", file_tab_scenes),
    ("right-sidebar", right_sidebar_scenes),
    ("files", files_scenes),
    ("api-background", api_background),
    ("banner-click", banner_click),
    ("default-name", default_name),
    ("switcher", switcher_scenes),
    ("overlays", overlays),
    ("help", help_scenes),
    ("panels", panel_scenes),
    ("preferences", preferences_scenes),
    ("update-button", update_button_scenes),
    ("cursor", cursor_scenes),
];

/// `fuxx __ui-shots [filter]`: run the scenes whose name contains `filter`, then exit.
pub fn run(filter: Option<&str>) -> ! {
    let mut ran = 0;
    for (name, scene) in SCENES {
        if filter.is_some_and(|f| !name.contains(f)) {
            continue;
        }
        scene();
        println!("ui-shots: {name} ok");
        ran += 1;
    }
    if ran == 0 {
        eprintln!("ui-shots: no scene matches {filter:?}");
        exit(1);
    }
    exit(0)
}

/// A themed panel standing in for a real scene: proves the harness end to end.
fn smoke() {
    let dims = size(px(320.0), px(200.0));
    for (slug, name) in SHOT_THEMES {
        let theme = shot_theme(name);
        let ui = theme.ui();
        let scene = |_: &mut Window, cx: &mut App| -> AnyView { cx.new(|_| Panel(ui)).into() };
        let path = shoot("smoke", slug, &theme, dims, scene);
        assert!(path.ends_with(format!("smoke-{slug}.png")));
        let bytes = std::fs::read(&path).expect("the PNG exists");
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
            .text_size(px(text::META))
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
        size::WAITING_ROW
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
                .text_size(px(text::META))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn png_has_the_signature_header_and_known_checksums() {
        assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
        assert_eq!(adler32(b"Wikipedia"), 0x11e6_0398);
        let shot = Shot {
            width: 2,
            height: 1,
            rgba: vec![1, 2, 3, 255, 4, 5, 6, 255],
        };
        let bytes = png(&shot);
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
        assert_eq!(&bytes[12..16], b"IHDR");
        assert_eq!(&bytes[16..24], &[0, 0, 0, 2, 0, 0, 0, 1]);
        assert!(bytes.ends_with(&[0, 0, 0, 0, b'I', b'E', b'N', b'D', 0xae, 0x42, 0x60, 0x82]));
    }
}

/// The appearance a scene's app runs with: `theme`, and the default font and size.
fn shot_appearance(theme: Theme, cx: &App) -> Appearance {
    Appearance {
        theme,
        font: mono_font(cx),
        font_size: fonts::DEFAULT_SIZE,
        theme_name: None,
        font_settings: fonts::from_entries(&[]),
        settings_path: None,
        user_theme_dir: None,
        presets: Vec::new(),
        restore: RestoreMode::default(),
        scripting: false,
        persistent: false,
        animations: true,
        browser_address_bar: true,
        browser_keep_data: false,
        check_updates: true,
        cursor_style: Default::default(),
        cursor_glide: true,
        cursor_glow: true,
        cursor_follows_status: true,
        cursor_fun: Default::default(),
    }
}

/// What a shell shows in the scenes, the same every time.
const SHELL_TEXT: &str = "~/code/api $ cargo test\r\n   Compiling api v0.4.2 (/Users/me/code/api)\r\n    Finished `test` profile [unoptimized + debuginfo] target(s) in 2.31s\r\n     Running unittests src/main.rs\r\n\r\nrunning 42 tests\r\ntest result: ok. 42 passed; 0 failed; 0 ignored\r\n\r\n~/code/api $ ";

const AGENT_TEXT: &str =
    "~/code/api $ claude\r\n\r\n  Reading src/auth.rs\r\n  Editing src/auth.rs (+12 -3)\r\n\r\n> ";

/// The whole window, as the app draws it, from a real `Workspace` with plain sessions (each a
/// fixed screen over a silent `cat`, so the shots don't depend on your shell): one session; the
/// same split in two; and one session with the sidebar hidden. Checks that the terminal's
/// padding and the top band are drawn in the terminal background (nothing painted in them).
/// Off-screen windows have no traffic lights, so the band is empty in the shots.
fn window_scenes() {
    let dims = size(px(960.0), px(600.0));
    for (slug, name) in SHOT_THEMES {
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

/// Panics unless the padding (and with the sidebar hidden, the top band) of the pane at the
/// right edge is all terminal background, and the left pane's grid starts just inside its
/// left padding.
fn check_window(
    shot: &Shot,
    theme: &Theme,
    dims: Size<Pixels>,
    split: bool,
    hidden: bool,
    scene: &str,
    slug: &str,
) {
    let scale = shot.width as f32 / f32::from(dims.width);
    let bg = theme.background;
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
    let band = if hidden {
        ui_tokens::size::TOP_BAND
    } else {
        0.0
    };
    // Every pane has a tab bar (not terminal background) above its terminal's padding.
    let header = ui_tokens::size::TAB_BAR;
    let top = band + header;
    // The right pane's right and top padding (skipping its tab bar), and (hidden) the band above.
    let in_header = |y: f32| y >= band && y < band + header;
    let mut y = 0.0;
    while y < h {
        if in_header(y) {
            y += 1.0;
            continue;
        }
        for dx in 0..pad_x as usize {
            assert!(
                is_bg(w - 1.0 - dx as f32, y),
                "{scene}-{slug}: right padding at y {y}"
            );
        }
        y += 1.0;
    }
    // Right of a split's divider (the pane area starts after the sidebar, at most 1/4 in).
    let mut x = w * 0.75;
    while x < w {
        for dy in 0..(top + pad_y) as usize {
            if in_header(dy as f32) {
                continue;
            }
            assert!(is_bg(x, dy as f32), "{scene}-{slug}: top padding at x {x}");
        }
        x += 1.0;
    }
    // The left pane's left padding: where the terminal background starts (after the sidebar),
    // `pad_x` of background, then the grid's first column (its prompt and output) within a cell
    // and a half (the grid's leftover width is shared by both sides).
    let mid = (top + h) / 2.0;
    let left = (0..w as usize)
        .map(|x| x as f32)
        .find(|&x| is_bg(x, mid))
        .expect("a terminal");
    let rows = (top + pad_y) as usize..(h - pad_y) as usize;
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
    for (slug, name) in SHOT_THEMES {
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
                        .child(status_mark::mark(status, &ui, phase, popping, px(side)))
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

/// The real sidebar with the new marks: sessions in every status across two groups.
fn sidebar_marks() {
    let dims = size(px(480.0), px(320.0));
    for (slug, name) in SHOT_THEMES {
        let theme = shot_theme(name);
        let build = move |window: &mut Window, cx: &mut App| -> AnyView {
            cx.set_global(shot_appearance(theme, cx));
            cx.new(|cx| {
                let mut ws = Workspace::for_shots(window, cx);
                let api = std::path::Path::new("/Users/me/code/api");
                let web = std::path::Path::new("/Users/me/code/web");
                let sessions = [
                    ("claude", api, Status::Working),
                    ("codex", api, Status::Waiting),
                    ("tests", api, Status::Done),
                    ("shell", api, Status::Idle),
                    ("agent", web, Status::Working),
                    ("server", web, Status::Idle),
                ];
                for (i, (name, project, status)) in sessions.into_iter().enumerate() {
                    ws.open_shot_session(name, project, SHELL_TEXT, None, window, cx);
                    ws.sessions.get_mut(i).expect("just opened").status = status;
                }
                ws
            })
            .into()
        };
        let path = shoot("sidebar-marks", slug, &theme, dims, build);
        println!("ui-shots: {}", path.display());
    }
}

/// Terminal panes are cached: a workspace redraw (the marks' ticker, a sidebar change) doesn't
/// repaint a terminal, while the terminal's own notify does. Writes to a terminal's screen
/// without telling it, redraws after a workspace notify and checks the frame is unchanged; then
/// has the terminal notify and checks the new text shows. No PNG.
fn cached_panes() {
    use std::cell::RefCell;
    use std::rc::Rc;

    let theme = shot_theme("Catppuccin Mocha");
    let dims = size(px(640.0), px(400.0));
    let mut cx = gpui::VisualTestAppContext::new(current_platform(false));
    fonts::register_bundled_fonts(&cx.text_system().all_font_names());
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

/// What the waiting session shows: a Claude Code style prompt, drawn in a box.
const CLAUDE_PROMPT: &str = "  Editing src/auth.rs (+12 -3)\r\n\r\n╭──────────────────────────────────────────────────╮\r\n│ Edit file                                        │\r\n│ src/auth.rs                                      │\r\n│                                                  │\r\n│ Do you want to make this edit to auth.rs?        │\r\n│ ❯ 1. Yes                                         │\r\n│   2. Yes, and don't ask again this session       │\r\n│   3. No, and tell Claude what to do differently  │\r\n╰──────────────────────────────────────────────────╯\r\n";

/// The question the sidebar must find in `CLAUDE_PROMPT`.
const CLAUDE_QUESTION: &str = "Do you want to make this edit to auth.rs?";

/// A session in a sidebar scene: name, group folder, status, how long in it, its screen.
struct ShotSession {
    name: &'static str,
    project: &'static str,
    status: Status,
    age: u64,
    text: &'static str,
}

const fn shot(
    name: &'static str,
    project: &'static str,
    status: Status,
    age: u64,
    text: &'static str,
) -> ShotSession {
    ShotSession {
        name,
        project,
        status,
        age,
        text,
    }
}

const API: &str = "/Users/me/code/api";
const WEB: &str = "/Users/me/code/web";

/// The sidebar scene's eight sessions in two groups: in api a waiting one (with a boxed
/// question), a working one with a script's progress, a done one and an idle one (split
/// together, the idle one active), and one with a long title; web closed, holding a waiting,
/// an idle and a done one.
const EIGHT: [ShotSession; 8] = [
    shot("claude", API, Status::Waiting, 4 * 60 + 12, CLAUDE_PROMPT),
    shot("codex", API, Status::Working, 32, AGENT_TEXT),
    shot("tests", API, Status::Done, 12 * 60, SHELL_TEXT),
    shot("shell", API, Status::Idle, 0, SHELL_TEXT),
    shot(
        "✳ Refactor the auth middleware to use the new session store",
        API,
        Status::Working,
        2 * 3600 + 300,
        AGENT_TEXT,
    ),
    shot("agent", WEB, Status::Waiting, 95, SHELL_TEXT),
    shot("server", WEB, Status::Idle, 0, SHELL_TEXT),
    shot("build", WEB, Status::Done, 40, SHELL_TEXT),
];

/// A workspace showing `sessions` (each over a silent `cat`), statuses set the way the app
/// sets them (so the waiting question is read off its screen). Session `split` is split from
/// the one before it; `active` is focused; the `collapsed` folders are closed.
fn sidebar_workspace(
    sessions: &[ShotSession],
    split: Option<usize>,
    active: usize,
    collapsed: &[&str],
    window: &mut Window,
    cx: &mut Context<Workspace>,
) -> Workspace {
    use crate::groups::GroupKey;
    use std::time::{Duration, Instant};

    let mut ws = Workspace::for_shots(window, cx);
    for (i, s) in sessions.iter().enumerate() {
        let axis = (split == Some(i)).then_some(Axis::Row);
        ws.open_shot_session(
            s.name,
            std::path::Path::new(s.project),
            s.text,
            axis,
            window,
            cx,
        );
    }
    for (i, s) in sessions.iter().enumerate() {
        ws.set_status(i, s.status, false, cx);
        let session = ws.sessions.get_mut(i).expect("just opened");
        session.status_since = Instant::now() - Duration::from_secs(s.age);
        session.pop = None; // no pop half-way in a still
    }
    ws.select(active, window, cx);
    ws.collapsed = collapsed
        .iter()
        .map(|p| GroupKey::Folder(p.into()))
        .collect();
    ws
}

/// codex (session 2) as a script would mark it: a progress bar with a label, and a note.
fn with_progress(ws: &mut Workspace) {
    let codex = ws.sessions.get_mut(1).expect("codex");
    codex.progress = Some((60, Some("migrate".into())));
    codex.note = Some("3 of 5 tables".into());
}

/// The appearance for sidebar scenes: the built-in presets, for the + menu.
fn sidebar_appearance(theme: Theme, cx: &App) -> Appearance {
    Appearance {
        presets: crate::presets::builtins(),
        ..shot_appearance(theme, cx)
    }
}

type Slot = std::rc::Rc<std::cell::RefCell<Option<gpui::Entity<Workspace>>>>;

/// The workspace a scene built (`Slot`), to act on and check.
fn slotted(slot: &Slot) -> gpui::Entity<Workspace> {
    slot.borrow().clone().expect("the workspace")
}

/// The window position of sidebar row `row`'s centre, `dy` below it.
fn row_point(
    cx: &mut VisualTestAppContext,
    ws: &gpui::Entity<Workspace>,
    row: usize,
    dy: f32,
) -> gpui::Point<Pixels> {
    cx.read(|cx| {
        let scroll = &ws.read(cx).sidebar.scroll;
        let b = scroll.bounds_for_item(row).expect("the row was laid out");
        let c = b.center();
        point(c.x, c.y + scroll.offset().y + px(dy))
    })
}

/// The sidebar, from real workspaces in both themes:
/// - `sidebar`: eight sessions in two groups (waiting with a boxed question, working with a
///   script's progress, done and idle split together in one workspace (M11b: one row, its two
///   panes on screen with their tab bars), a long title, a closed group) and the first-run hint;
///   `sidebar-narrow` and `sidebar-wide` at 180 and 400 px.
/// - `sidebar-hover`: the pointer over the closed group's header (its "+" shows).
/// - `sidebar-menu`: the footer's + menu open.
/// - `sidebar-drag`: codex's row dragged down its group, its copy right over the gap it opens
///   between shell and the long one, once the rows have slid (M11b Task 7b): tests and shell slid
///   up by codex's height (and the gap), the copy (as tall as codex's row) filling the gap, claude
///   and the rows below still, no insertion line.
/// - `sidebar-scroll`: 24 sessions, the last one active, so the list scrolls to it.
///
/// Panics unless the question was read off the boxed prompt, a waiting row is 56 px and a
/// plain one 42 px (M11b: each has its folder line), the active row is scrolled into view and
/// rows scrolled away don't count as on screen, and the dragged row's run slid as described.
fn sidebar_scenes() {
    use gpui::{Modifiers, MouseButton};
    use std::cell::RefCell;
    use std::rc::Rc;

    let dims = size(px(960.0), px(600.0));
    for (slug, name) in SHOT_THEMES {
        let theme = shot_theme(name);
        for (scene, width, dims) in [
            ("sidebar", 240.0, dims),
            ("sidebar-narrow", 180.0, size(px(720.0), px(600.0))),
            ("sidebar-wide", 400.0, dims),
        ] {
            let slot: Slot = Rc::new(RefCell::new(None));
            let keep = slot.clone();
            let build = move |window: &mut Window, cx: &mut App| -> AnyView {
                cx.set_global(sidebar_appearance(theme, cx));
                let ws = cx.new(|cx| {
                    let mut ws = sidebar_workspace(&EIGHT, Some(3), 3, &[WEB], window, cx);
                    ws.sidebar.width = width;
                    with_progress(&mut ws);
                    ws
                });
                *keep.borrow_mut() = Some(ws.clone());
                ws.into()
            };
            let check = move |cx: &mut VisualTestAppContext, _: AnyWindowHandle| {
                let ws = slotted(&slot);
                cx.read(|cx| {
                    let ws = ws.read(cx);
                    let question = ws.sessions.items()[0].question.as_deref();
                    assert_eq!(question, Some(CLAUDE_QUESTION), "{scene}-{slug}: question");
                    // Rows: api header, its four workspaces (tests and shell split in one),
                    // web's header.
                    let height = |i: usize| {
                        let b = ws.sidebar.scroll.bounds_for_item(i).expect("laid out");
                        f32::from(b.size.height)
                    };
                    // Every row has its folder line; the waiting one its status line too.
                    assert_eq!(height(1), 56.0, "{scene}-{slug}: waiting row");
                    assert_eq!(height(3), 42.0, "{scene}-{slug}: the split workspace's row");
                    assert_eq!(ws.sidebar.scroll.children_count(), 6, "{scene}-{slug}");
                    // The split workspace is on screen: two panes, each with its tab.
                    assert_eq!(ws.tab_sessions().len(), 2, "{scene}-{slug}: tabs");
                });
            };
            let path = shoot_with(scene, slug, &theme, dims, build, check);
            println!("ui-shots: {}", path.display());
        }

        // The pointer over web's (closed) header; the + menu open; a row mid-drag.
        for scene in ["sidebar-hover", "sidebar-menu", "sidebar-drag"] {
            let slot: Slot = Rc::new(RefCell::new(None));
            let keep = slot.clone();
            let build = move |window: &mut Window, cx: &mut App| -> AnyView {
                cx.set_global(sidebar_appearance(theme, cx));
                let ws = cx.new(|cx| {
                    let mut ws = sidebar_workspace(&EIGHT, None, 3, &[WEB], window, cx);
                    ws.sidebar.hint_dismissed = true;
                    with_progress(&mut ws);
                    ws
                });
                *keep.borrow_mut() = Some(ws.clone());
                ws.into()
            };
            // The drag's rows (claude, codex, tests, shell, the long one) as laid out, for the
            // checks.
            let rows: Rc<RefCell<Vec<gpui::Bounds<Pixels>>>> = Rc::default();
            let seen = rows.clone();
            let acting = slot.clone();
            let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let ws = slotted(&acting);
                let none = Modifiers::none();
                match scene {
                    "sidebar-hover" => {
                        let at = row_point(cx, &ws, 6, 0.0);
                        cx.simulate_mouse_move(window, at, None, none);
                    }
                    "sidebar-menu" => {
                        // The footer's + (an icon now, no "New" label), at its left.
                        let at = point(px(18.0), dims.height - px(18.0));
                        cx.simulate_click(window, at, none);
                        assert!(cx.read(|cx| ws.read(cx).sidebar.new_menu), "menu opens");
                    }
                    _ => {
                        // codex (row 2, three lines tall) down past tests and shell: its copy right
                        // over the gap between shell and the long one, as it opens with codex
                        // taken out (the rows after it moved up by its room: its height and the
                        // gap); the gap ends where shell did.
                        let bounds: Vec<_> = (1..=5)
                            .map(|i| {
                                cx.read(|cx| {
                                    let scroll = &ws.read(cx).sidebar.scroll;
                                    let mut b = scroll.bounds_for_item(i).expect("laid out");
                                    b.origin.y += scroll.offset().y;
                                    b
                                })
                            })
                            .collect();
                        let (codex, shell) = (bounds[1], bounds[3]);
                        let centre = shell.bottom() - codex.size.height / 2.0;
                        // Taken at its middle, 8 px down (GPUI's drag starts past its threshold):
                        // the pointer is 8 px below the copy's centre.
                        let from = codex.center();
                        let to = point(from.x, centre + px(8.0));
                        cx.simulate_mouse_down(window, from, MouseButton::Left, none);
                        let left = MouseButton::Left;
                        cx.simulate_mouse_move(window, from + point(px(0.0), px(8.0)), left, none);
                        cx.simulate_mouse_move(window, to, left, none);
                        let slot = cx.read(|cx| ws.read(cx).sidebar.drop_slot);
                        assert_eq!(slot, Some(3), "the gap is between shell and the long one");
                        settle_slides(cx, window);
                        *seen.borrow_mut() = bounds;
                    }
                }
                // Not kept past its window (the act runs twice for the drag).
                acting.borrow_mut().take();
            };
            let path = shoot_with(scene, slug, &theme, dims, build.clone(), act.clone());
            println!("ui-shots: {}", path.display());
            if scene != "sidebar-drag" {
                continue;
            }
            let shot = render_with(&theme, dims, build, act);
            let n = format!("{scene}-{slug}");
            let ui = theme.ui();
            let is = |p: [u8; 3], c: alacritty_terminal::vte::ansi::Rgb| {
                p.iter()
                    .zip([c.r, c.g, c.b])
                    .all(|(a, b)| a.abs_diff(b) <= 3)
            };
            let rows = rows.borrow().clone();
            let (claude, codex, tests, shell) = (rows[0], rows[1], rows[2], rows[3]);
            let room = f32::from(tests.top() - codex.top());
            let x = f32::from(shell.left()) + 10.0;
            let px_at = |y: Pixels| pixel_at(&shot, dims, x, f32::from(y));
            // shell (selected: the accent) slid up by codex's room; the gap below it, down to
            // where shell ended, is filled by codex's copy (as tall as codex's row), top to
            // bottom: no empty band, no line.
            let up = px_at(shell.top() - px(room) + px(3.0));
            assert!(is(up, ui.accent), "{n}: shell slid up: {up:?}");
            let gap_top = shell.bottom() - codex.size.height;
            for y in [
                gap_top + px(3.0),
                shell.center().y,
                shell.bottom() - px(3.0),
            ] {
                let p = px_at(y);
                assert!(
                    !is(p, ui.sidebar_background) && !is(p, ui.accent),
                    "{n}: the copy in the gap at {y:?}: {p:?}"
                );
            }
            // claude, above codex, stays (tinted: it's waiting).
            let still = px_at(claude.center().y);
            assert!(is(still, ui.waiting_tint), "{n}: claude stays: {still:?}");
        }

        // More sessions than fit: the last one is active, so the list scrolls to it.
        let statuses = [Status::Working, Status::Idle, Status::Done, Status::Waiting];
        let names = [
            "claude", "codex", "tests", "shell", "server", "agent", "docs", "lint",
        ];
        let many: Vec<ShotSession> = [API, WEB, "/Users/me/code/infra"]
            .iter()
            .flat_map(|project| {
                names.iter().enumerate().map(|(i, name)| {
                    shot(name, project, statuses[i % 4], 60 * i as u64, SHELL_TEXT)
                })
            })
            .collect();
        let many: &'static [ShotSession] = Box::leak(many.into_boxed_slice());
        let slot: Slot = Rc::new(RefCell::new(None));
        let keep = slot.clone();
        let build = move |window: &mut Window, cx: &mut App| -> AnyView {
            cx.set_global(sidebar_appearance(theme, cx));
            let ws = cx.new(|cx| {
                let mut ws = sidebar_workspace(many, None, many.len() - 1, &[], window, cx);
                ws.sidebar.hint_dismissed = true;
                ws
            });
            *keep.borrow_mut() = Some(ws.clone());
            ws.into()
        };
        let check = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
            // A second frame: the first laid the rows out, this one scrolls to the active one.
            cx.update_window(window, |_, window, _| window.refresh())
                .expect("refresh");
            cx.run_until_parked();
            let ws = slotted(&slot);
            cx.read(|cx| {
                let ws = ws.read(cx);
                let offset = f32::from(ws.sidebar.scroll.offset().y);
                assert!(
                    offset < 0.0,
                    "sidebar-scroll-{slug}: scrolled to the active row"
                );
                let rows = ws.sidebar.scroll.children_count();
                let on_screen = ws.visible_statuses().len();
                assert!(
                    on_screen < rows,
                    "sidebar-scroll-{slug}: {on_screen} of {rows} rows count as on screen"
                );
            });
        };
        let tall = size(px(960.0), px(520.0));
        let path = shoot_with("sidebar-scroll", slug, &theme, tall, build, check);
        println!("ui-shots: {}", path.display());
    }
}

const INFRA: &str = "/Users/me/code/infra";
const PAYMENTS: &str = "/Users/me/code/a-very-long-project-folder/services/payments";
const SCRATCH: &str = "/tmp/scratch";
const LONG_BRANCH: &str = "feature/a-really-long-branch-name-for-the-payments-service";

/// The rows scene's sessions: deploy and claude (waiting) to be pinned; codex in api; payments
/// (to be selected) with a long folder; a long name in web; notes outside home.
const ROWS: [ShotSession; 6] = [
    shot("deploy", INFRA, Status::Idle, 0, SHELL_TEXT),
    shot("claude", API, Status::Waiting, 4 * 60 + 12, CLAUDE_PROMPT),
    shot("codex", API, Status::Working, 32, AGENT_TEXT),
    shot("payments", PAYMENTS, Status::Idle, 0, SHELL_TEXT),
    shot(
        "✳ Refactor the auth middleware to use the new session store",
        WEB,
        Status::Done,
        40,
        SHELL_TEXT,
    ),
    shot("notes", SCRATCH, Status::Idle, 0, SHELL_TEXT),
];

/// The M11b sidebar rows, from real workspaces in both themes, at 240 px (`sidebar-rows`) and
/// 180 px (`sidebar-rows-narrow`): deploy and claude pinned (📌, above every group header),
/// claude waiting with its 🔔 "Needs input" pill and question (the pill alone at 180 px); notes
/// in no group, a gap under the pinned rows; each row's `branch • ~/folder`;
/// payments selected (filled with the accent, its text and mark in `on_accent`), its long branch
/// cut first, then its long folder at its start (keeping `payments`); the long name cut; notes
/// in /tmp (no `~`, not in a repo: no branch).
///
/// Panics unless the pinned rows lead, the rows carry their branch and folder, and the
/// selected row is filled with the accent with something on it in `on_accent`.
fn sidebar_row_scenes() {
    use std::cell::{Cell, RefCell};
    use std::path::Path;
    use std::rc::Rc;
    use std::time::Instant;

    use crate::sidebar::SidebarRow;

    for (slug, name) in SHOT_THEMES {
        let theme = shot_theme(name);
        let ui = theme.ui();
        for (scene, width) in [("sidebar-rows", 240.0), ("sidebar-rows-narrow", 180.0)] {
            let dims = size(px(width + 300.0), px(560.0));
            let slot: Slot = Rc::new(RefCell::new(None));
            let keep = slot.clone();
            let build = move |window: &mut Window, cx: &mut App| -> AnyView {
                cx.set_global(sidebar_appearance(theme, cx));
                let ws = cx.new(|cx| {
                    let mut ws = sidebar_workspace(&ROWS, None, 3, &[], window, cx);
                    ws.sidebar.width = width;
                    ws.sidebar.hint_dismissed = true;
                    for i in [0, 1] {
                        let pinned = ws.workspace_of(i).expect("its workspace");
                        ws.model.set_pinned(pinned, true);
                    }
                    // notes in no group (its folder as found, not where it started): it leads
                    // the unpinned rows, a gap under the pinned ones.
                    let notes = ws.sessions.get_mut(5).expect("notes");
                    notes.project = None;
                    let id = notes.id;
                    let found = Some(SCRATCH.into());
                    ws.places.set_folder(id, found, Instant::now());
                    ws.sync();
                    for (folder, branch) in [
                        (INFRA, "main"),
                        (API, "feature/rows"),
                        (PAYMENTS, LONG_BRANCH),
                        (WEB, "main"),
                    ] {
                        ws.places.set_branch(Path::new(folder), Some(branch.into()));
                    }
                    ws
                });
                *keep.borrow_mut() = Some(ws.clone());
                ws.into()
            };
            // The selected row's bounds, for the pixel checks.
            let selected = Rc::new(Cell::new(None));
            let found = selected.clone();
            let act = move |cx: &mut VisualTestAppContext, _: AnyWindowHandle| {
                let ws = slotted(&slot);
                cx.read(|cx| {
                    let ws = ws.read(cx);
                    let n = format!("{scene}-{slug}");
                    let rows = ws.rows_in_view(Instant::now());
                    let shape: Vec<(String, bool, Option<String>, Option<String>)> = rows
                        .iter()
                        .map(|r| match r {
                            SidebarRow::Header(h) => (format!("[{}]", h.label), false, None, None),
                            SidebarRow::Workspace(w) => (
                                w.label.clone(),
                                w.pinned,
                                w.branch.clone(),
                                w.folder.clone(),
                            ),
                        })
                        .collect();
                    let row = |label: &str, pinned, branch: Option<&str>, folder: Option<&str>| {
                        (
                            label.to_string(),
                            pinned,
                            branch.map(String::from),
                            folder.map(String::from),
                        )
                    };
                    let header = |label: &str| row(label, false, None, None);
                    let long_folder = "~/code/a-very-long-project-folder/services/payments";
                    assert_eq!(
                        shape,
                        [
                            row("deploy", true, Some("main"), Some("~/code/infra")),
                            row("claude", true, Some("feature/rows"), Some("~/code/api")),
                            row("notes", false, None, Some("/tmp/scratch")),
                            header("[api]"),
                            row("codex", false, Some("feature/rows"), Some("~/code/api")),
                            header("[payments]"),
                            row("payments", false, Some(LONG_BRANCH), Some(long_folder)),
                            header("[web]"),
                            row(ROWS[4].name, false, Some("main"), Some("~/code/web")),
                        ],
                        "{n}"
                    );
                    let active = rows
                        .iter()
                        .position(|r| matches!(r, SidebarRow::Workspace(w) if w.active))
                        .expect("a selected row");
                    assert_eq!(active, 6, "{n}: payments is selected");
                    // A group's gap between the pinned rows and notes.
                    let bounds = |i: usize| ws.sidebar.scroll.bounds_for_item(i).expect("laid out");
                    let gap = f32::from(bounds(2).top() - bounds(1).bottom());
                    assert_eq!(gap, 10.0, "{n}: the gap under the pinned rows");
                    let b = ws.sidebar.scroll.bounds_for_item(active).expect("laid out");
                    found.set(Some((
                        f32::from(b.left()),
                        f32::from(b.right()),
                        f32::from(b.top()),
                    )));
                });
                // Not kept past the window: a handle left over is reported as a leak.
                slot.borrow_mut().take();
            };
            let path = shoot_with(scene, slug, &theme, dims, build.clone(), act.clone());
            println!("ui-shots: {}", path.display());
            let shot = render_with(&theme, dims, build, act);
            let (left, right, top) = selected.get().expect("the selected row's bounds");
            let n = format!("{scene}-{slug}");
            // Its fill, in the padding left of its mark.
            let fill = pixel_at(&shot, dims, left + 3.0, top + 9.0);
            assert_eq!(
                fill,
                [ui.accent.r, ui.accent.g, ui.accent.b],
                "{n}: accent fill"
            );
            // Its name and mark in on_accent.
            let ink = count_along(&shot, dims, ui.on_accent, top + 9.0, left, right);
            assert!(ink > 0, "{n}: on_accent on the selected row");
        }
    }
}

/// Sidebar rows as tabs, from real workspaces in both themes (the `sidebar` scene's eight
/// sessions; claude orange, codex blue, tests green, the long one purple; shell active):
/// - `tabs` and `tabs-narrow`: the rows with their colour dots and ⌘N, at 240 and 180 px (no
///   ages there).
/// - `tabs-hover`: the pointer over codex: a × in its mark's place; `tabs-hover-close`: on the
///   × itself; `tabs-hover-active`: over shell, the active row (which shows its mark otherwise).
/// - `tabs-menu`: codex's tab menu open (right-click), blue ringed.
/// - `tabs-switcher`: the ⌘P switcher, the dots beside the names, over the split's panes (tests
///   and shell).
///
/// Panics unless the dots are drawn in their colour on the rows, the menu opens on the row
/// right-clicked and a click on a swatch sets the colour.
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
    for (slug, name) in SHOT_THEMES {
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
            let width = if scene == "tabs-narrow" { 180.0 } else { 240.0 };
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
            // codex's name line, where its dot is (its row has its folder and a note line under
            // it: 56 px, the name line's middle 15 px above the row's).
            let dot_y = Rc::new(std::cell::Cell::new(0.0f32));
            let noted = dot_y.clone();
            let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let ws = slotted(&acting);
                let none = Modifiers::none();
                // codex is row 2 (under api's header).
                let codex = row_point(cx, &ws, 2, 0.0);
                noted.set(f32::from(row_point(cx, &ws, 2, -15.0).y));
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
                        cx.update_window(window, |_, window, cx| {
                            ws.update(cx, |ws, cx| ws.open_switcher(window, cx))
                        })
                        .expect("open the switcher");
                    }
                    _ => {}
                }
                // Not kept past the window: a handle left over is reported as a leak.
                acting.borrow_mut().take();
            };
            let path = shoot_with(scene, slug, &theme, dims, build.clone(), act.clone());
            println!("ui-shots: {}", path.display());
            if scene == "tabs" || scene == "tabs-narrow" {
                // codex's blue dot, on its row's line.
                let shot = render_with(&theme, dims, build, act);
                let y = dot_y.get();
                let dots = count_along(&shot, dims, ui.tab(TabColor::Blue), y, 0.0, width);
                assert!(dots > 0, "{scene}-{slug}: codex's blue dot");
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
            // codex's × (on hover, in its mark's place).
            let line = row_point(cx, &ws, 2, -7.5);
            let close = point(px(6.0 + 8.0 + 7.0), line.y);
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

/// The window pixel at (`x`, `y`) points.
fn pixel_at(shot: &Shot, dims: Size<Pixels>, x: f32, y: f32) -> [u8; 3] {
    let scale = shot.width as f32 / f32::from(dims.width);
    let (x, y) = ((x * scale) as usize, (y * scale) as usize);
    let i = (y * shot.width as usize + x) * 4;
    [shot.rgba[i], shot.rgba[i + 1], shot.rgba[i + 2]]
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
fn count_along(
    shot: &Shot,
    dims: Size<Pixels>,
    c: alacritty_terminal::vte::ansi::Rgb,
    y: f32,
    x0: f32,
    x1: f32,
) -> usize {
    let scale = shot.width as f32 / f32::from(dims.width);
    let near = |p: [u8; 3]| {
        p.iter()
            .zip([c.r, c.g, c.b])
            .all(|(a, b)| a.abs_diff(b) <= 3)
    };
    let (a, b) = ((x0 * scale) as usize, (x1 * scale) as usize);
    (a..b)
        .filter(|&dx| near(pixel_at(shot, dims, dx as f32 / scale, y)))
        .count()
}

/// A 3-pane split in both themes, from a real `Workspace`:
/// - `splits`: api focused (accent line along its tab's top), claude waiting (tinted tab),
///   codex working; dividers quiet.
/// - `splits-waiting`: the waiting pane focused (tint and accent together).
/// - `splits-hover`: the pointer over the vertical divider (2 px accent).
/// - `splits-drag`: that divider dragged left (accent, the panes follow).
///
/// Panics unless each shows 3 tabs, their fills and the focus line are the right colours,
/// and the divider is the plain colour except when hovered or dragged.
fn split_scenes() {
    use gpui::{Modifiers, MouseButton};
    use std::cell::RefCell;
    use std::rc::Rc;

    let dims = size(px(960.0), px(600.0));
    for (slug, name) in SHOT_THEMES {
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
            let at = |ratio: f32| 240.0 + (960.0 - 240.0) * ratio - ratio;
            let acting = slot.clone();
            let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let tabs = cx.read(|cx| slotted(&acting).read(cx).tab_sessions().len());
                assert_eq!(tabs, 3, "{scene}-{slug}: a tab per pane");
                let none = Modifiers::none();
                let left = MouseButton::Left;
                let on = point(px(at(0.5) + 0.5), px(300.0));
                match scene {
                    "splits-hover" => cx.simulate_mouse_move(window, on, None, none),
                    "splits-drag" => {
                        cx.simulate_mouse_down(window, on, left, none);
                        cx.simulate_mouse_move(window, on - point(px(40.0), px(0.0)), left, none);
                        cx.simulate_mouse_move(window, on - point(px(72.0), px(0.0)), left, none);
                    }
                    _ => {}
                }
                // Not kept past the window: a handle left over is reported as a leak.
                acting.borrow_mut().take();
            };
            let path = shoot_with(scene, slug, &theme, dims, build.clone(), act.clone());
            let shot = render_with(&theme, dims, build, act);
            let n = format!("{scene}-{slug}");
            // Tab fills, away from the text: api (left), claude (top right), both their pane's
            // active (and only) tab, 160 px into it (it is 200 wide; × sits at its end).
            let ratio = if scene == "splits-drag" { 0.4 } else { 0.5 };
            let right_tab = at(ratio) + 160.0;
            let (left_header, right_header) = ((400.0, 13.0), (right_tab, 13.0));
            let is = |p: [u8; 3], c: alacritty_terminal::vte::ansi::Rgb| {
                p.iter()
                    .zip([c.r, c.g, c.b])
                    .all(|(a, b)| a.abs_diff(b) <= 3)
            };
            let px_at = |(x, y): (f32, f32)| pixel_at(&shot, dims, x, y);
            assert!(is(px_at(left_header), ui.panel), "{n}: active tab");
            assert!(
                is(px_at(right_header), ui.waiting_hover),
                "{n}: active waiting tab's tint"
            );
            // The focus line: along the top of the focused pane's active tab only.
            let (line_left, line_right) = (px_at((400.0, 1.0)), px_at((right_tab, 1.0)));
            assert_eq!(
                is(line_left, ui.accent),
                focus == 0,
                "{n}: focus line, left"
            );
            assert_eq!(
                is(line_right, ui.accent),
                focus == 1,
                "{n}: focus line, right"
            );
            // The vertical divider, 1 px; the accent 2 px, only when hovered or dragged. Measured
            // across at y = 450, in the lower panes: not at 300, where the right column's own
            // divider (claude over codex, splitting the 600 px pane area) lies along the row,
            // 299.5–300.5, and adds its width (or not, by how the half point is rasterised).
            let (x0, x1) = (at(ratio) - 4.0, at(ratio) + 5.0);
            let scale = shot.width as f32 / 960.0;
            let accent = count_along(&shot, dims, ui.accent, 450.0, x0, x1);
            let plain = count_along(&shot, dims, ui.divider, 450.0, x0, x1);
            if matches!(scene, "splits-hover" | "splits-drag") {
                assert_eq!(accent, (2.0 * scale) as usize, "{n}: 2 px accent divider");
            } else {
                assert_eq!(accent, 0, "{n}: no accent on a quiet divider");
                assert_eq!(plain, scale as usize, "{n}: 1 px plain divider");
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
/// tests' status, the waiting row is 56 px (api's 42: its folder line), and the three panes
/// each have a tab.
fn workspace_scenes() {
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::time::{Duration, Instant};

    let dims = size(px(960.0), px(600.0));
    for (slug, name) in SHOT_THEMES {
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
                // Each with its folder line; the waiting one its status line too.
                assert_eq!(height(2), 56.0, "{n}: agent's waiting row");
                assert_eq!(height(4), 42.0, "{n}: api's row");
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

/// A workspace for the tab bar scenes, focused on api. `four`: two panes, the left one with
/// four tabs (api idle and active, claude waiting, codex working, a browser tab), the right one
/// with tests (done); else `many` (≥ 1) idle terminals in one pane, the last one shown.
fn tab_bar_workspace(
    four: bool,
    many: usize,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) -> Workspace {
    use crate::layout_model::TabKind;
    use std::time::{Duration, Instant};

    let mut ws = Workspace::for_shots(window, cx);
    ws.sidebar.hint_dismissed = true;
    let project = std::path::Path::new(API);
    if !four {
        let names = [
            "api", "claude", "codex", "tests", "docs", "lint", "server", "agent",
        ];
        ws.open_shot_session(names[0], project, SHELL_TEXT, None, window, cx);
        for name in names.iter().cycle().skip(1).take(many - 1) {
            ws.open_shot_tab(name, project, SHELL_TEXT, window, cx);
        }
        return ws;
    }
    let tabs = [
        shot("api", API, Status::Idle, 0, SHELL_TEXT),
        shot("claude", API, Status::Waiting, 4 * 60 + 12, CLAUDE_PROMPT),
        shot("codex", API, Status::Working, 32, AGENT_TEXT),
    ];
    ws.open_shot_session(tabs[0].name, project, tabs[0].text, None, window, cx);
    for s in &tabs[1..] {
        ws.open_shot_tab(s.name, project, s.text, window, cx);
    }
    let pane = ws.model.focused_pane().expect("a pane");
    ws.model.add_tab(pane, TabKind::Browser { url: None });
    ws.sync();
    ws.open_shot_session("tests", project, SHELL_TEXT, Some(Axis::Row), window, cx);
    let tests = shot("tests", API, Status::Done, 720, "");
    for s in tabs.iter().chain([&tests]) {
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
}

/// Where tab `i` of `pane`'s bar is drawn, from the bar's own scroll; none before it's laid out.
fn tab_bounds(
    cx: &mut VisualTestAppContext,
    ws: &gpui::Entity<Workspace>,
    pane: crate::layout_model::PaneId,
    i: usize,
) -> Option<gpui::Bounds<Pixels>> {
    cx.read(|cx| {
        let scrolls = ws.read(cx).tab_scroll.borrow();
        let (scroll, _) = scrolls.get(&pane)?;
        let mut b = scroll.bounds_for_item(i)?;
        b.origin.x += scroll.offset().x;
        Some(b)
    })
}

/// The panes of the focused workspace, in layout order.
fn shown_panes(
    cx: &mut VisualTestAppContext,
    ws: &gpui::Entity<Workspace>,
) -> Vec<crate::layout_model::PaneId> {
    cx.read(|cx| {
        ws.read(cx)
            .shown_layout()
            .map(|t| t.leaves())
            .unwrap_or_default()
    })
}

/// A bar's buttons (`tab_bar`): each 20 pt, 2 pt apart, 4 pt in from the ends; five of them
/// (new terminal, new browser, split right, split down, the right sidebar's).
const BAR_BUTTONS: f32 = 4.0 + 5.0 * 20.0 + 4.0 * 2.0 + 4.0;
/// How far left of its pane's right edge the split-right button's centre is: past the right
/// sidebar's and split down (each a button and a gap), then half a button.
const SPLIT_RIGHT_FROM_EDGE: f32 = 4.0 + 2.0 * (20.0 + 2.0) + 10.0;
/// The same for the right sidebar's button, the last.
const RIGHT_SIDEBAR_FROM_EDGE: f32 = 4.0 + 10.0;

/// Pane tab bars (M11b Task 3), from real workspaces in both themes:
/// - `tab-bars`: two panes; the left (focused) one with api (active, accent line on top), claude
///   (waiting, tinted), codex (working) and a browser tab ("New tab", globe); the right one with
///   tests (done), unfocused (active but no accent line). The tabs shrink below 200 px to fit.
/// - `tab-bars-hover`: the pointer over codex (hover fill, × shown); `tab-bars-button`: over the
///   left bar's split-right button.
/// - `tab-bars-menu`: claude's right-click menu (Close Tab, Close Other Tabs in Pane, Rename
///   Tab…); `tab-bars-rename`: codex double-clicked (shown, its rename box open).
/// - `tab-bars-single`: one pane, one tab (the bar still shows).
/// - `tab-bars-narrow`: a narrow window, one pane of seven tabs, the last shown: the tabs stop at
///   90 px and the row scrolls to the shown one.
///
/// Panics unless the tabs have the widths and fills above, the menu and rename box open on the
/// tabs clicked; and (not shot) unless ⌃Tab / ⌃⇧Tab cycle the focused pane's tabs, a click
/// shows a tab, × closes one, a middle-click closes one, and the split button splits its pane.
fn tab_bar_scenes() {
    use crate::layout_model::TabKind;
    use crate::workspace::{NextTab, PrevTab, Renaming};
    use gpui::{KeyBinding, Modifiers, MouseButton, MouseDownEvent, MouseUpEvent};
    use std::cell::RefCell;
    use std::rc::Rc;

    let wide = size(px(1280.0), px(600.0));
    for (slug, name) in SHOT_THEMES {
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
                // Four tabs in the left bar, equally shrunk, between the bounds.
                for b in &tabs {
                    let w = f32::from(b.size.width);
                    assert!((90.0..200.0).contains(&w), "{n}: tab width {w}");
                    let first = f32::from(tabs[0].size.width);
                    assert!((w - first).abs() <= 1.0, "{n}: equal widths ({w}, {first})");
                    assert_eq!(f32::from(b.size.height), 26.0, "{n}: 26 px bar");
                }
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
                        // The left pane's buttons sit at its right edge (half the 1040 px pane
                        // area, less half the divider): split right is the third of five.
                        let right = tabs[0].left() + px(519.5);
                        let at = point(right - px(SPLIT_RIGHT_FROM_EDGE), px(13.0));
                        cx.simulate_mouse_move(window, at, None, none);
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
            let (api, claude) = (tabs[0], tabs[1]);
            // Away from the text, the icon and the ×: near each tab's right end, low in the bar.
            let fill_at = |b: gpui::Bounds<Pixels>| (f32::from(b.right()) - 26.0, 20.0);
            if scene == "tab-bars" {
                let (x, y) = fill_at(api);
                assert!(is(px_at(x, y), ui.panel), "{n}: api raised");
                assert!(is(px_at(x, 1.0), ui.accent), "{n}: api's accent line");
                let (x, y) = fill_at(claude);
                assert!(is(px_at(x, y), ui.waiting_tint), "{n}: claude tinted");
                assert!(!is(px_at(x, 1.0), ui.accent), "{n}: no line on claude");
                // The right pane's tests: active, raised, but its pane isn't focused.
                let x = f32::from(api.left()) + 520.0 + 150.0;
                assert!(is(px_at(x, 20.0), ui.panel), "{n}: tests raised");
                assert!(is(px_at(x, 1.0), ui.panel), "{n}: no line on tests");
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
                if many == 1 {
                    assert_eq!(f32::from(first.size.width), 200.0, "{n}: widest");
                    assert_eq!(offset, 0.0, "{n}: no scroll");
                } else {
                    assert_eq!(f32::from(first.size.width), 90.0, "{n}: narrowest");
                    assert!(offset < 0.0, "{n}: scrolled to the last tab");
                    let last = tab_bounds(cx, &ws, panes[0], many - 1).expect("laid out");
                    assert!(
                        f32::from(last.right()) <= 720.0 - BAR_BUTTONS + 1.0,
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
            let close = point(browser.right() - px(4.0 + 8.0), browser.center().y);
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
            // The left bar's split right: a third pane, focused, with one terminal tab.
            let right = api.left() + px(519.5);
            let split = point(right - px(SPLIT_RIGHT_FROM_EDGE), px(13.0));
            cx.simulate_click(window, split, none);
            let panes_now = shown_panes(cx, &ws);
            assert_eq!(panes_now.len(), 3, "{n}: split right made a pane");
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
fn settle_slides(cx: &mut VisualTestAppContext, window: AnyWindowHandle) {
    std::thread::sleep(crate::drag_gap::SLIDE + std::time::Duration::from_millis(30));
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

    let wide = size(px(1280.0), px(600.0));
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
    for (slug, name) in SHOT_THEMES {
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
            let width = f32::from(tabs[0].size.width);
            let (chip_x, chip_y) = (f32::from(g.chip.x), f32::from(g.chip.y));
            // No insertion line: no accent in the left bar below the active tab's top line.
            let no_line = || {
                for x in (f32::from(tabs[0].left()) as i32..f32::from(tabs[3].right()) as i32)
                    .map(|x| x as f32 + 0.5)
                {
                    assert!(!is(px_at(x, 13.0), ui.accent), "{n}: no line at {x}");
                }
            };
            // A place in the left bar with nothing drawn in it (a tab gone from there).
            let empty = |tab: gpui::Bounds<Pixels>, what: &str| {
                let left = f32::from(tab.left());
                for dx in [6.0, 20.0, 40.0, 60.0, 80.0] {
                    for y in [7.0, 13.0, 19.0] {
                        let p = px_at(left + dx, y);
                        assert!(
                            is(p, ui.sidebar_background),
                            "{n}: {what} empty at +{dx},{y}: {p:?}"
                        );
                    }
                }
            };
            match scene {
                "tab-drag-gap" => {
                    // claude (tinted) slid left into api's place; the gap is where claude was.
                    let claude = px_at(f32::from(tabs[0].center().x), 3.0);
                    assert!(
                        is(claude, ui.waiting_tint),
                        "{n}: claude slid left: {claude:?}"
                    );
                    let gap = px_at(f32::from(tabs[1].right()) - 4.0, 3.0);
                    assert!(is(gap, ui.sidebar_background), "{n}: the gap: {gap:?}");
                    // The chip is where api was taken: its left edge at the pointer less the
                    // grab, level with the bar (not under it).
                    assert_eq!(chip_y, 0.0, "{n}: the chip's top is the bar's");
                    let inside = px_at(chip_x + 3.0, 3.0);
                    assert!(
                        is(inside, ui.panel),
                        "{n}: the chip's left edge: {inside:?}"
                    );
                    let outside = px_at(chip_x - 3.0, 3.0);
                    assert!(!is(outside, ui.panel), "{n}: left of the chip: {outside:?}");
                    let low = px_at(chip_x + 10.0, 23.0);
                    assert!(is(low, ui.panel), "{n}: the chip down the bar: {low:?}");
                    assert!(
                        (chip_x - (f32::from(tabs[1].left()) - 10.0)).abs() <= 1.0,
                        "{n}: the chip's centre 10 px short of claude's middle"
                    );
                    no_line();
                }
                "tab-drag-other" => {
                    // Its own bar closed up: claude in api's place, the last place empty.
                    let claude = px_at(f32::from(tabs[0].center().x), 3.0);
                    assert!(
                        is(claude, ui.waiting_tint),
                        "{n}: claude slid left: {claude:?}"
                    );
                    empty(tabs[3], "the left bar's last place");
                    // The right bar: tests (filled, as its pane's shown tab) slid right by
                    // api's width; the gap's end shows the bar.
                    let left = f32::from(g.right.left());
                    let tests = px_at(left + width + 6.0, 3.0);
                    assert!(is(tests, ui.panel), "{n}: tests slid right: {tests:?}");
                    let gap = px_at(left + width - 6.0, 3.0);
                    assert!(is(gap, ui.sidebar_background), "{n}: the gap: {gap:?}");
                    no_line();
                }
                _ => {
                    // The ring along docs's row's left edge, mid-height.
                    let (x, y) = (f32::from(g.row.left()) + 1.0, f32::from(g.row.center().y));
                    assert!(is(px_at(x, y), ui.accent), "{n}: docs's row ringed");
                    // Its own bar closed up: the browser tab's place is empty.
                    empty(tabs[3], "the left bar's last place");
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
                            let keys = view.read(cx).focus_handle(cx).is_focused(window);
                            ("browser".to_string(), keys)
                        }
                        TabKind::File { .. } => {
                            let view = ws.file_tabs.get(&tab).expect("its view");
                            let keys = view.read(cx).focus_handle(cx).is_focused(window);
                            ("file".to_string(), keys)
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

/// What the browser tab scenes' page shows (no page is made off-screen: the bar is given it).
const PAGE_URL: &str = "http://localhost:3000/dashboard";
const PAGE_TITLE: &str = "Acme dashboard";

/// A workspace for the browser tab scenes: api's terminal on the left; on the right, focused, a
/// browser tab on `PAGE_URL` (`loading`: still loading, so its third button is stop).
/// A file tab (M11g) in a pane beside a terminal's tab: opened twice for one file it is one tab,
/// shown, with the keyboard; its bar tab draws the file glyph; closing it leaves the terminal,
/// which takes the keyboard back. Its file isn't on this Mac, so it shows "This file no longer
/// exists", and its Close button closes the tab. Shot once per theme (opened). Then the viewer
/// itself (`file_view_scenes`).
fn file_tab_scenes() {
    use crate::layout_model::TabKind;
    use crate::tab_bar::look;
    use gpui::Focusable;
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    let dims = size(px(900.0), px(400.0));
    let file = PathBuf::from(format!("{API}/src/main.rs"));
    for (slug, name) in SHOT_THEMES {
        let theme = shot_theme(name);
        let ui = theme.ui();
        let tab_at: Rc<Cell<Option<gpui::Bounds<Pixels>>>> = Rc::default();
        let scene = |close: bool| {
            let slot: Slot = Rc::new(RefCell::new(None));
            let keep = slot.clone();
            let build = move |window: &mut Window, cx: &mut App| -> AnyView {
                cx.set_global(shot_appearance(theme, cx));
                let ws = cx.new(|cx| {
                    let mut ws = Workspace::for_shots(window, cx);
                    ws.sidebar.hint_dismissed = true;
                    let project = std::path::Path::new(API);
                    ws.open_shot_session("api", project, SHELL_TEXT, None, window, cx);
                    ws
                });
                *keep.borrow_mut() = Some(ws.clone());
                ws.into()
            };
            let file = file.clone();
            let seen = tab_at.clone();
            let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let ws = slotted(&slot);
                let n = format!("file-tab-{slug}");
                for _ in 0..2 {
                    cx.update_window(window, |_, window, cx| {
                        ws.update(cx, |ws, cx| ws.open_file_tab(file.clone(), window, cx))
                    })
                    .expect("open the file");
                    cx.run_until_parked();
                }
                let pane = cx.read(|cx| ws.read(cx).model.focused_pane().expect("a pane"));
                let (kinds, file_tab) = cx.read(|cx| {
                    let ws = ws.read(cx);
                    let p = ws.model.pane(pane).expect("known");
                    let kinds: Vec<TabKind> = p.tabs.iter().map(|t| t.kind.clone()).collect();
                    assert_eq!(ws.model.focused_tab(), Some(p.tabs[1].id), "{n}: shown");
                    assert_eq!(ws.file_tabs.len(), 1, "{n}: one view");
                    assert_eq!(ws.focused_terminal(), None, "{n}: no terminal focused");
                    (kinds, p.tabs[1].id)
                });
                assert!(matches!(kinds[0], TabKind::Terminal(_)), "{n}");
                assert_eq!(
                    kinds[1..],
                    [TabKind::File {
                        path: file.clone(),
                        mode: crate::layout_model::FileMode::Plain,
                    }],
                    "{n}: one file tab for the file opened twice"
                );
                let keys = cx
                    .update_window(window, |_, window, cx| {
                        let ws = ws.read(cx);
                        let view = ws.focused_file_tab().expect("the file tab's view");
                        view.read(cx).focus_handle(cx).is_focused(window)
                    })
                    .expect("read");
                assert!(keys, "{n}: the file tab has the keyboard");
                seen.set(tab_bounds(cx, &ws, pane, 1));
                if close {
                    cx.update_window(window, |_, window, cx| {
                        ws.update(cx, |ws, cx| ws.close_tab(file_tab, window, cx))
                    })
                    .expect("close the file tab");
                    cx.run_until_parked();
                    cx.update_window(window, |_, window, cx| {
                        let ws = ws.read(cx);
                        let p = ws.model.pane(pane).expect("the pane stays");
                        assert_eq!(p.tabs.len(), 1, "{n}: the terminal's tab is left");
                        assert!(ws.file_tabs.is_empty(), "{n}: the view went");
                        let s = ws.sessions.active().expect("the terminal is active again");
                        assert_eq!(ws.focused_terminal(), Some(s.id), "{n}");
                        let keys = s.view.focus_handle(cx).is_focused(window);
                        assert!(keys, "{n}: the terminal has the keyboard back");
                    })
                    .expect("read");
                }
                slot.borrow_mut().take();
            };
            (build, act)
        };
        let (build, act) = scene(false);
        let path = shoot_with("file-tab", slug, &theme, dims, build.clone(), act.clone());
        println!("ui-shots: {}", path.display());
        // Its bar tab (active, in the focused pane) draws the glyph in the tab's text colour.
        let shot = render_with(&theme, dims, build, act);
        let tab = tab_at.get().expect("the file tab was laid out");
        let text = look(&ui, Status::Idle, true, true).text;
        let near = |p: [u8; 3]| {
            p.iter()
                .zip([text.r, text.g, text.b])
                .all(|(a, b)| a.abs_diff(b) <= 60)
        };
        let (x0, y) = (f32::from(tab.left()) + 8.0, f32::from(tab.center().y));
        let glyph = (0..12).any(|dx| {
            (-6..=6).any(|dy| near(pixel_at(&shot, dims, x0 + dx as f32, y + dy as f32)))
        });
        assert!(glyph, "file-tab-{slug}: the file glyph is drawn");
        // Closing it (not shot).
        let (build, act) = scene(true);
        render_with(&theme, dims, build, act);
        // Its file is gone (no such file here): its Close button closes the tab (not shot).
        let slot: Slot = Rc::new(RefCell::new(None));
        let keep = slot.clone();
        let build = move |window: &mut Window, cx: &mut App| -> AnyView {
            cx.set_global(shot_appearance(theme, cx));
            let ws = cx.new(|cx| {
                let mut ws = Workspace::for_shots(window, cx);
                ws.sidebar.hint_dismissed = true;
                let project = std::path::Path::new(API);
                ws.open_shot_session("api", project, SHELL_TEXT, None, window, cx);
                ws
            });
            *keep.borrow_mut() = Some(ws.clone());
            ws.into()
        };
        let file = file.clone();
        let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
            let ws = slotted(&slot);
            let n = format!("file-tab-close-{slug}");
            cx.update_window(window, |_, window, cx| {
                ws.update(cx, |ws, cx| ws.open_file_tab(file.clone(), window, cx))
            })
            .expect("open the file");
            cx.run_until_parked();
            cx.update_window(window, |_, window, _| window.refresh())
                .expect("refresh");
            cx.run_until_parked();
            let close = cx.read(|cx| {
                let view = ws.read(cx).focused_file_tab().expect("the view").clone();
                assert_eq!(view.read(cx).state_name(), "missing", "{n}");
                view.read(cx).close_bounds().expect("its Close button")
            });
            cx.simulate_click(window, close.center(), gpui::Modifiers::none());
            cx.read(|cx| {
                let ws = ws.read(cx);
                assert!(ws.file_tabs.is_empty(), "{n}: the tab closed");
                let pane = ws.model.focused_pane().expect("a pane");
                assert_eq!(ws.model.pane(pane).expect("known").tabs.len(), 1, "{n}");
            });
            slot.borrow_mut().take();
        };
        render_with(&theme, dims, build, act);
        println!("ui-shots: file-tab-close-{slug} ok");
    }
    file_view_scenes();
}

/// The file tab's viewer (M11g Task 6), a tab on its own in a window, in both themes:
/// - `file-tab-view`: a Rust fixture in colours with its line numbers; change bars from a given
///   list (lines 3–4 added, green; line 7 changed, blue; a red ▸ under line 10 for lines
///   deleted) with each changed line tinted 12 %; ⌘F "fn": "1 of N", the matches tinted, the
///   current one stronger; lines 2–3 selected with the mouse (⌘C copies them).
/// - `file-tab-long`: a 1,200-character line (with two-byte characters: drawn from its
///   checkpoints) scrolled sideways by the wheel (its end shows), and a ▸ for lines deleted above
///   the first, drawn whole under the top edge.
/// - `file-tab-missing`, `file-tab-refuse`, `file-tab-plain`: a file that is gone (its path, a
///   Close button that asks to close the tab), a binary file (Show in Finder), a file over 2 MB
///   (plain text, "Too large to colour").
///
/// - `file-tab-minified`: a 1.5 MB one-line file, ⌘F "e": "1 of 10,000+", and a frame stays
///   quick.
///
/// Not shot: a reload with new text (the file rewritten, the disk checked again) keeps the
/// scroll; a shrink to 10 lines and a growth back keep the view at the top; the file removed
/// with the find bar open closes it, ⌘F then does nothing, and the tab has the keyboard. Also prints how long colouring a 5,000-line Rust file takes (run with `--release`
/// for the real number).
fn file_view_scenes() {
    use crate::file_tab::{FileTab, FileTabEvent, MISSING, REFUSED, TOO_LARGE};
    use crate::git_status::LineChange;
    use crate::highlight::Highlighter;
    use crate::ui_tokens::{MARK_MIN, ensure_contrast};
    use alacritty_terminal::vte::ansi::Rgb;
    use gpui::{Focusable, Modifiers, MouseButton, ScrollDelta, ScrollWheelEvent, TouchPhase};
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    let dir = std::env::temp_dir().join("fuxx-ui-shots-file-tab");
    std::fs::create_dir_all(&dir).expect("the fixture folder");
    let dir = std::fs::canonicalize(&dir).unwrap_or(dir);
    let view_rs = dir.join("view.rs");
    std::fs::write(&view_rs, FILE_VIEW_FIXTURE).expect("view.rs");
    let long_rs = dir.join("long.rs");
    let long_line = Rc::new(format!("let long = \"{}END\";", "abcdéfghij".repeat(120)));
    std::fs::write(
        &long_rs,
        format!("// one long line\n{long_line}\nfn after() {{}}\n"),
    )
    .expect("long.rs");
    let missing = dir.join("gone.rs");
    let _ = std::fs::remove_file(&missing);
    let binary = dir.join("image.bin");
    std::fs::write(&binary, b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR").expect("image.bin");
    let big = dir.join("big.log");
    let row = "2026-11-08 12:00:00 INFO request served in 12 ms, all fine\n";
    std::fs::write(&big, row.repeat(2 * 1024 * 1024 / row.len() + 100)).expect("big.log");
    let reload_rs = dir.join("reload.rs");
    let minified = dir.join("min.js");
    std::fs::write(&minified, "var e=1;".repeat(1_500_000 / 8)).expect("min.js");

    type TabSlot = Rc<RefCell<Option<gpui::Entity<FileTab>>>>;
    let tab_of = |slot: &TabSlot| slot.borrow().clone().expect("the file tab");
    let dims = size(px(760.0), px(460.0));
    let fixture_lines: Rc<Vec<&str>> = Rc::new(FILE_VIEW_FIXTURE.lines().collect());
    let fn_count: usize = fixture_lines
        .iter()
        .map(|l| l.to_lowercase().matches("fn").count())
        .sum();
    for (slug, name) in SHOT_THEMES {
        let theme = shot_theme(name);
        let ui = theme.ui();
        let near = |p: [u8; 3], c: Rgb, by: u8| {
            p.iter()
                .zip([c.r, c.g, c.b])
                .all(|(a, b)| a.abs_diff(b) <= by)
        };
        let build_for = |path: PathBuf, slot: TabSlot| {
            move |window: &mut Window, cx: &mut App| -> AnyView {
                cx.set_global(shot_appearance(theme, cx));
                cx.bind_keys(crate::key_bindings());
                let tab = cx.new(|cx| {
                    let mut tab = FileTab::new(path.clone(), window, cx);
                    tab.frozen = true;
                    tab
                });
                *slot.borrow_mut() = Some(tab.clone());
                tab.into()
            }
        };
        let frame = |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
            cx.update_window(window, |_, window, _| window.refresh())
                .expect("refresh");
            cx.run_until_parked();
        };

        // The view: colours, numbers, change bars and tints, find, a selection and ⌘C.
        let n = format!("file-tab-view-{slug}");
        let slot: TabSlot = Rc::default();
        let build = build_for(view_rs.clone(), slot.clone());
        let copied: Rc<RefCell<Option<String>>> = Rc::default();
        let seen = copied.clone();
        let geom_seen: Rc<Cell<Option<crate::file_tab::Geom>>> = Rc::default();
        let keep_geom = geom_seen.clone();
        let act = {
            let n = n.clone();
            let slot = slot.clone();
            let fixture_lines = fixture_lines.clone();
            move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let tab = tab_of(&slot);
                frame(cx, window);
                cx.read(|cx| {
                    let t = tab.read(cx);
                    assert_eq!(t.state_name(), "text", "{n}");
                    let doc = t.doc().expect("its text");
                    assert_eq!(doc.lines.len(), fixture_lines.len(), "{n}");
                    assert!(doc.spans.is_some(), "{n}: coloured");
                });
                cx.update(|cx| {
                    tab.update(cx, |t, cx| {
                        t.set_changes(
                            vec![
                                (3, LineChange::Added),
                                (4, LineChange::Added),
                                (7, LineChange::Changed),
                                (10, LineChange::DeletedAfter),
                            ],
                            cx,
                        )
                    })
                });
                cx.update_window(window, |_, window, cx| {
                    tab.read(cx).focus_handle(cx).focus(window, cx)
                })
                .expect("focus");
                cx.simulate_keystrokes(window, "cmd-f");
                cx.simulate_input(window, "fn");
                frame(cx, window);
                let label = cx.read(|cx| tab.read(cx).find_label());
                assert_eq!(label, format!("1 of {fn_count}"), "{n}: ⌘F fn");
                // Lines 2–3 (0-based 1–2), from line 2's start to line 3's 5th byte.
                let g = cx.read(|cx| tab.read(cx).geom()).expect("drawn");
                let at = |line: usize, x: f32| {
                    point(
                        g.bounds.left() + g.text_left + px(x),
                        g.bounds.top() + g.line_height * (line as f32 + 0.5),
                    )
                };
                let cw = f32::from(g.cell_width);
                cx.simulate_mouse_down(window, at(1, 0.5), MouseButton::Left, Modifiers::none());
                cx.simulate_mouse_move(
                    window,
                    at(2, cw * 5.0 + 1.0),
                    MouseButton::Left,
                    Modifiers::none(),
                );
                cx.simulate_mouse_up(
                    window,
                    at(2, cw * 5.0 + 1.0),
                    MouseButton::Left,
                    Modifiers::none(),
                );
                cx.simulate_keystrokes(window, "cmd-c");
                *seen.borrow_mut() = cx.read_from_clipboard().and_then(|c| c.text());
                frame(cx, window);
                keep_geom.set(cx.read(|cx| tab.read(cx).geom()));
                slot.borrow_mut().take();
            }
        };
        let path = shoot_with(
            "file-tab-view",
            slug,
            &theme,
            dims,
            build.clone(),
            act.clone(),
        );
        println!("ui-shots: {}", path.display());
        let shot = render_with(&theme, dims, build, act);
        let expect = format!("{}\n{}", fixture_lines[1], &fixture_lines[2][..5]);
        assert_eq!(copied.borrow().as_deref(), Some(expect.as_str()), "{n}: ⌘C");
        let g = geom_seen.get().expect("drawn");
        let at = |x: Pixels, y: Pixels| pixel_at(&shot, dims, f32::from(x), f32::from(y));
        let row_y = |line: usize| g.bounds.top() + g.line_height * (line as f32 + 0.5);
        let fit = |c: Rgb| ensure_contrast(c, theme.background, MARK_MIN);
        let (green, blue, red) = (
            fit(theme.palette[2]),
            fit(theme.palette[4]),
            fit(theme.palette[1]),
        );
        let bar_x = g.bounds.left() + g.bar_left + px(1.5);
        // The bars: 0-based lines 2 and 3 green, 6 blue; none on line 0.
        for (line, c) in [(2, green), (3, green), (6, blue)] {
            let p = at(bar_x, row_y(line));
            assert!(near(p, c, 12), "{n}: line {line}'s bar {p:?}, not {c:?}");
        }
        let p = at(bar_x, row_y(0));
        assert!(near(p, theme.background, 6), "{n}: no bar on line 0: {p:?}");
        // The tint at the far right of a changed line: its colour at 12 % over the background.
        let right = g.bounds.right() - px(6.0);
        let tint = crate::theme::mix(theme.background, green, 0.12);
        let p = at(right, row_y(2));
        assert!(near(p, tint, 6), "{n}: the tint {p:?}, not {tint:?}");
        assert!(
            near(at(right, row_y(0)), theme.background, 4),
            "{n}: untinted"
        );
        // The ▸ under line 10 (the edge above 0-based line 10), red.
        let edge = g.bounds.top() + g.line_height * 10.0;
        let marked = (-6..=6).any(|dy| {
            (-6..=10).any(|dx| near(at(bar_x + px(dx as f32), edge + px(dy as f32)), red, 40))
        });
        assert!(marked, "{n}: the deletion mark");
        // The line numbers, faint, right-aligned before the bar.
        let numbers = (0..(f32::from(g.numbers_right) as i32)).any(|dx| {
            (-5..=5).any(|dy| {
                near(
                    at(g.bounds.left() + px(dx as f32), row_y(0) + px(dy as f32)),
                    ui.text_faint,
                    40,
                )
            })
        });
        assert!(numbers, "{n}: line numbers in text_faint");
        // Several syntax colours in the text.
        let inked = |c: Rgb| {
            (0..12).any(|line| {
                (0..400).any(|dx| {
                    near(
                        at(g.bounds.left() + g.text_left + px(dx as f32), row_y(line)),
                        c,
                        30,
                    )
                })
            })
        };
        let keyword = fit(theme.palette[5]);
        let string = fit(theme.palette[2]);
        let comment = fit(theme.palette[8]);
        for (what, c) in [
            ("keyword", keyword),
            ("string", string),
            ("comment", comment),
        ] {
            assert!(inked(c), "{n}: a {what} in {c:?}");
        }

        // A long line, scrolled sideways by the wheel: its end shows.
        let n = format!("file-tab-long-{slug}");
        let long_geom: Rc<Cell<Option<crate::file_tab::Geom>>> = Rc::default();
        let keep_long = long_geom.clone();
        let slot: TabSlot = Rc::default();
        let build = build_for(long_rs.clone(), slot.clone());
        let act = {
            let n = n.clone();
            let long_line = long_line.clone();
            let long_geom = keep_long.clone();
            move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let tab = tab_of(&slot);
                frame(cx, window);
                let g = cx.read(|cx| tab.read(cx).geom()).expect("drawn");
                let over = g.bounds.center();
                for _ in 0..3 {
                    cx.simulate_event(
                        window,
                        ScrollWheelEvent {
                            position: over,
                            delta: ScrollDelta::Pixels(point(px(-5000.0), px(0.0))),
                            modifiers: Modifiers::none(),
                            touch_phase: TouchPhase::Moved,
                        },
                    );
                }
                frame(cx, window);
                // Lines deleted above the first: its ▸ on the top edge, drawn whole.
                cx.update(|cx| {
                    tab.update(cx, |t, cx| {
                        t.set_changes(vec![(0, LineChange::DeletedAfter)], cx)
                    })
                });
                frame(cx, window);
                let (scroll, g) = cx.read(|cx| (tab.read(cx).scroll(), tab.read(cx).geom()));
                let g = g.expect("drawn");
                long_geom.set(Some(g));
                assert!(scroll.x > px(0.0), "{n}: scrolled sideways");
                assert_eq!(scroll.y, px(0.0), "{n}");
                // As far as it goes: the line's end at the right edge.
                let widest = g.text_left + g.cell_width * long_line.chars().count() as f32;
                assert!(
                    scroll.x + g.bounds.size.width >= widest,
                    "{n}: the end shows ({scroll:?})"
                );
                slot.borrow_mut().take();
            }
        };
        let path = shoot_with(
            "file-tab-long",
            slug,
            &theme,
            dims,
            build.clone(),
            act.clone(),
        );
        println!("ui-shots: {}", path.display());
        let shot = render_with(&theme, dims, build, act);
        let g = long_geom.get().expect("drawn");
        let red = fit(theme.palette[1]);
        let bar_x = g.bounds.left() + g.bar_left + px(1.5);
        let whole = (-6..=10).any(|dx| {
            (0..=6).any(|dy| {
                let y = g.bounds.top() + g.line_height * 0.4 + px(dy as f32 * 0.6);
                let x = bar_x + px(dx as f32);
                near(pixel_at(&shot, dims, f32::from(x), f32::from(y)), red, 40)
            })
        });
        assert!(
            whole,
            "{n}: the top edge's ▸ is drawn whole, under the edge"
        );

        // The states: missing (and its Close), refused, plain.
        for (scene, file, state, words) in [
            ("file-tab-missing", missing.clone(), "missing", MISSING),
            ("file-tab-refuse", binary.clone(), "refuse", REFUSED),
            ("file-tab-plain", big.clone(), "plain", TOO_LARGE),
        ] {
            let n = format!("{scene}-{slug}");
            let slot: TabSlot = Rc::default();
            let build = build_for(file, slot.clone());
            let closed = Rc::new(Cell::new(false));
            let asked = closed.clone();
            let act = {
                let n = n.clone();
                move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                    let tab = tab_of(&slot);
                    frame(cx, window);
                    cx.read(|cx| {
                        let t = tab.read(cx);
                        assert_eq!(t.state_name(), state, "{n}");
                        assert_eq!(t.note(), Some(words), "{n}");
                        if state == "plain" {
                            assert!(t.doc().expect("its text").spans.is_none(), "{n}");
                        }
                    });
                    if state == "missing" {
                        let flag = asked.clone();
                        cx.update(|cx| {
                            cx.subscribe(&tab, move |_, e: &FileTabEvent, _| {
                                flag.set(matches!(e, FileTabEvent::Close))
                            })
                            .detach()
                        });
                        let b = cx.read(|cx| tab.read(cx).close_bounds()).expect("Close");
                        cx.simulate_click(window, b.center(), Modifiers::none());
                    }
                    slot.borrow_mut().take();
                }
            };
            let path = shoot_with(scene, slug, &theme, dims, build, act);
            println!("ui-shots: {}", path.display());
            if state == "missing" {
                assert!(closed.get(), "{n}: Close asks to close the tab");
            }
        }

        // A reload with new text keeps the scroll (not shot).
        let n = format!("file-tab-reload-{slug}");
        let numbered = |count: usize, word: &str| -> String {
            (1..=count)
                .map(|i| format!("let {word}{i} = {i};\n"))
                .collect()
        };
        std::fs::write(&reload_rs, numbered(200, "a")).expect("reload.rs");
        let slot: TabSlot = Rc::default();
        let build = build_for(reload_rs.clone(), slot.clone());
        let file = reload_rs.clone();
        let label = n.clone();
        let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
            let tab = tab_of(&slot);
            frame(cx, window);
            let g = cx.read(|cx| tab.read(cx).geom()).expect("drawn");
            cx.update(|cx| {
                tab.update(cx, |t, cx| {
                    t.set_scroll(point(px(0.0), g.line_height * 40.0), cx)
                })
            });
            frame(cx, window);
            let (before, version) = cx.read(|cx| {
                (
                    tab.read(cx).scroll(),
                    tab.read(cx).doc().expect("text").version,
                )
            });
            assert_eq!(before.y, g.line_height * 40.0, "{n}: scrolled");
            std::fs::write(&file, numbered(220, "b")).expect("rewrite reload.rs");
            cx.update(|cx| tab.update(cx, |t, cx| t.check_disk(cx)));
            frame(cx, window);
            cx.read(|cx| {
                let t = tab.read(cx);
                let doc = t.doc().expect("text");
                assert!(doc.version > version, "{n}: reloaded");
                assert_eq!(doc.lines.len(), 220, "{n}: the new text");
                assert!(doc.text.starts_with("let b1"), "{n}");
                assert_eq!(t.scroll(), before, "{n}: the scroll kept");
            });
            // It shrinks to 10 lines (the view to the top), then grows again: it stays at the top.
            for (count, word) in [(10, "c"), (220, "d")] {
                std::fs::write(&file, numbered(count, word)).expect("rewrite reload.rs");
                cx.update(|cx| tab.update(cx, |t, cx| t.check_disk(cx)));
                frame(cx, window);
                let (lines, scroll) = cx.read(|cx| {
                    let t = tab.read(cx);
                    (t.doc().expect("text").lines.len(), t.scroll())
                });
                assert_eq!(lines, count, "{n}");
                assert_eq!(scroll.y, px(0.0), "{n}: {count} lines, at the top");
            }
            // ⌘F with the file there; then it goes: the bar closes, ⌘F does nothing, the tab has
            // the keyboard.
            cx.update_window(window, |_, window, cx| {
                tab.read(cx).focus_handle(cx).focus(window, cx)
            })
            .expect("focus");
            cx.simulate_keystrokes(window, "cmd-f");
            cx.simulate_input(window, "let");
            frame(cx, window);
            assert_eq!(cx.read(|cx| tab.read(cx).find_label()), "1 of 220", "{n}");
            // Scrolled away from the current match, the file changes: the view stays put.
            cx.update(|cx| {
                tab.update(cx, |t, cx| {
                    t.set_scroll(point(px(0.0), g.line_height * 100.0), cx)
                })
            });
            frame(cx, window);
            let away = cx.read(|cx| tab.read(cx).scroll());
            assert_eq!(away.y, g.line_height * 100.0, "{n}: scrolled away");
            std::fs::write(&file, numbered(230, "e")).expect("rewrite reload.rs");
            cx.update(|cx| tab.update(cx, |t, cx| t.check_disk(cx)));
            frame(cx, window);
            let (label, scroll) = cx.read(|cx| (tab.read(cx).find_label(), tab.read(cx).scroll()));
            assert_eq!(label, "1 of 230", "{n}: found again in the new text");
            assert_eq!(
                scroll, away,
                "{n}: a reload with the find bar open keeps the scroll"
            );
            std::fs::remove_file(&file).expect("remove reload.rs");
            cx.update(|cx| tab.update(cx, |t, cx| t.check_disk(cx)));
            frame(cx, window);
            cx.simulate_keystrokes(window, "cmd-f");
            frame(cx, window);
            let (state, open, keys) = cx
                .update_window(window, |_, window, cx| {
                    let t = tab.read(cx);
                    (
                        t.state_name(),
                        t.find_open(),
                        t.focus_handle(cx).is_focused(window),
                    )
                })
                .expect("read");
            assert_eq!(state, "missing", "{n}");
            assert!(!open, "{n}: no find bar for a missing file");
            assert!(keys, "{n}: the tab has the keyboard back");
            slot.borrow_mut().take();
        };
        render_with(&theme, dims, build, act);
        println!("ui-shots: {label} ok");

        // A minified file, one 1.5 MB line, and ⌘F "e": capped at 10,000, and drawing it stays
        // quick (only the matches in sight are drawn).
        let n = format!("file-tab-minified-{slug}");
        let slot: TabSlot = Rc::default();
        let build = build_for(minified.clone(), slot.clone());
        let act = {
            let n = n.clone();
            move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let tab = tab_of(&slot);
                frame(cx, window);
                assert_eq!(cx.read(|cx| tab.read(cx).state_name()), "plain", "{n}");
                cx.update_window(window, |_, window, cx| {
                    tab.read(cx).focus_handle(cx).focus(window, cx)
                })
                .expect("focus");
                cx.simulate_keystrokes(window, "cmd-f");
                cx.simulate_input(window, "e");
                frame(cx, window);
                let label = cx.read(|cx| tab.read(cx).find_label());
                assert_eq!(label, "1 of 10,000+", "{n}");
                let started = std::time::Instant::now();
                for _ in 0..5 {
                    cx.update_window(window, |_, window, cx| {
                        window.refresh();
                        window.draw(cx).clear();
                    })
                    .expect("draw");
                }
                let each = started.elapsed() / 5;
                println!("ui-shots: {n}: a frame in {} ms", each.as_millis());
                assert!(
                    each < std::time::Duration::from_secs(1),
                    "{n}: {each:?} a frame"
                );
                slot.borrow_mut().take();
            }
        };
        let path = shoot_with("file-tab-minified", slug, &theme, dims, build, act);
        println!("ui-shots: {}", path.display());
    }

    // How long colouring a 5,000-line Rust file takes (the loading of the syntaxes apart).
    let theme = shot_theme("Catppuccin Mocha");
    let started = std::time::Instant::now();
    let h = Highlighter::load(&theme);
    let loaded = started.elapsed();
    let text: String = FILE_VIEW_FIXTURE
        .lines()
        .cycle()
        .take(5000)
        .map(|l| format!("{l}\n"))
        .collect();
    let started = std::time::Instant::now();
    let lines = h.highlight(std::path::Path::new("big.rs"), &text);
    let took = started.elapsed();
    assert_eq!(lines.len(), 5000);
    println!(
        "ui-shots: file-tab highlight 5000 lines: {} ms (syntaxes loaded in {} ms; {} build)",
        took.as_millis(),
        loaded.as_millis(),
        if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        }
    );
}

/// The file tab scenes' Rust fixture: comments, strings, numbers, keywords, types, functions,
/// and "fn" in several places (one in upper case).
const FILE_VIEW_FIXTURE: &str = r#"// A small server, for the file tab's shots.
use std::collections::HashMap;
use std::time::Duration;

/// How long a request may take.
const TIMEOUT: Duration = Duration::from_secs(30);
const NAME: &str = "fuxx";

pub struct Server {
    routes: HashMap<String, Route>,
    port: u16,
}

impl Server {
    pub fn new(port: u16) -> Self {
        Self { routes: HashMap::new(), port }
    }

    /// Add a route; FN-style handlers only.
    pub fn route(&mut self, path: &str, handler: fn() -> String) {
        let route = Route { handler, hits: 0 };
        self.routes.insert(path.to_string(), route);
    }
}

struct Route {
	handler: fn() -> String,
	hits: u64,
}

fn main() {
    let mut server = Server::new(8080);
    server.route("/health", || "ok".to_string());
    println!("listening on {} (timeout {:?})", server.port, TIMEOUT);
}
"#;

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
    for (slug, name) in SHOT_THEMES {
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
            // Each pane's right edge, as laid out (the left one's: where the right one starts,
            // less the divider).
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
                    window_right - 300.0
                } else {
                    window_right
                };
                *seen.borrow_mut() = vec![f32::from(second.left()) - 1.0, pane_right];
                cx.read(|cx| {
                    let ws = ws.read(cx);
                    assert_eq!(ws.right_sidebar.hidden, !open, "{n}");
                    assert_eq!(ws.right_sidebar.width, 300.0, "{n}: 300 pt by default");
                    assert!(ws.right_sidebar.slide.is_none(), "{n}: still");
                });
                if scene == "right-sidebar-resize" {
                    // Its left edge (at 980) taken and moved 80 left, then let go.
                    let (from, to) = (point(px(980.0), px(300.0)), point(px(900.0), px(300.0)));
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
                    seen.borrow_mut()[1] = window_right - 380.0;
                    // The panes re-fit to the narrower area on the next frame.
                    cx.update_window(window, |_, window, _| window.refresh())
                        .expect("refresh");
                    cx.run_until_parked();
                    let second = tab_bounds(cx, &ws, panes[1], 0).expect("laid out");
                    seen.borrow_mut()[0] = f32::from(second.left()) - 1.0;
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
            // The sidebar's body, low in the window: its colour across its width, the
            // terminal's just left of it.
            let y = 400.0;
            if width > 0.0 {
                for x in [window_right - width + 6.0, window_right - 6.0] {
                    let p = at(x, y);
                    assert!(near(p, ui.sidebar_background), "{n}: sidebar at {x}: {p:?}");
                }
                let p = at(window_right - width - 6.0, y);
                assert!(near(p, theme.background), "{n}: the pane left of it: {p:?}");
            } else {
                let p = at(window_right - 6.0, y);
                assert!(near(p, theme.background), "{n}: no right sidebar: {p:?}");
            }
            // Every pane's icon: lit (the hover fill beside the glyph, the glyph's filled side
            // in the text colour) while open; else on the bar, dim.
            for (i, right) in edges.borrow().iter().enumerate() {
                let centre = right - RIGHT_SIDEBAR_FROM_EDGE;
                assert!(*right > left, "{n}: pane {i}'s edge {right}");
                let (fill, glyph) = (at(centre - 8.0, 13.0), at(centre + 3.0, 13.0));
                if width > 0.0 {
                    assert!(near(fill, ui.row_hover), "{n}: pane {i} lit: {fill:?}");
                    assert!(near(glyph, ui.text), "{n}: pane {i} glyph: {glyph:?}");
                } else {
                    let bar = ui.sidebar_background;
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
                std::thread::sleep(crate::sidebar_toggle::SLIDE + Duration::from_millis(30));
                frame(cx);
                assert_eq!(state(cx), (true, None), "{n}: slid away");
                let panes = shown_panes(cx, &ws);
                let icon = point(px(window_right - RIGHT_SIDEBAR_FROM_EDGE), px(13.0));
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
fn files_fixture() -> PathBuf {
    let root = std::env::temp_dir().join("fuxx-ui-shots-files");
    for folder in ["docs", "src/util"] {
        std::fs::create_dir_all(root.join(folder)).expect("the fixture's folders");
    }
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
/// no git): src open and selected, and a made-up mark map: main.rs and README.md modified,
/// new.rs added, old.rs deleted (so not on the disk).
fn files_tree(root: &std::path::Path) -> crate::file_tree::FileTree {
    use crate::file_tree::{CAP, FileTree};
    use crate::git_status::{Mark, list_dir};
    use std::collections::HashMap;

    let mut tree = FileTree::new(root.to_path_buf(), Some(PathBuf::from("src")));
    for rel in tree.wanted() {
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
/// - `files`: the left sidebar shown. The tree: docs, src (open, selected, `M`: it holds a
///   modified and an added file), util, main.rs (`M`, blue), new.rs (`A`, green), Cargo.toml,
///   notes.txt, old.rs (`D`, red: gone from the disk), README.md (`M`). The panel's header row
///   is level with the pane's tab bar, at the top.
/// - `files-filter`: "main" typed in the filter box: src and main.rs only.
/// - `files-bare`: the left sidebar hidden: the header row is still level with the tab bar,
///   both under the 38 pt band.
///
/// Not shot: in the tree, ↓ → ← ↑ and Return move, open and close; a click on main.rs opens its
/// file tab, which takes the keyboard; Esc in the tree gives the keyboard back to the pane;
/// with the filter box focused, ⌥⌘\ closes the sidebar and the terminal takes the keyboard.
fn files_scenes() {
    use crate::git_status::Mark;
    use crate::layout_model::{FileMode, TabKind};
    use crate::right_sidebar::{FILTER_BAND, ROW, mark_color};
    use crate::ui_tokens::size::{TAB_BAR, TOP_BAND};
    use gpui::{Focusable, Modifiers};
    use std::cell::RefCell;
    use std::rc::Rc;

    let dims = size(px(1100.0), px(560.0));
    let window_right = 1100.0_f32;
    let sidebar_left = window_right - 300.0;
    let root = files_fixture();
    let rows_of = |cx: &mut VisualTestAppContext, ws: &gpui::Entity<Workspace>| {
        cx.read(|cx| {
            let files = &ws.read(cx).right_sidebar.files;
            let rows = files.rows.iter().filter(|r| r.more.is_none());
            rows.map(|r| r.rel.display().to_string())
                .collect::<Vec<_>>()
        })
    };
    for (slug, name) in SHOT_THEMES {
        let theme = shot_theme(name);
        let ui = theme.ui();
        let near = |p: [u8; 3], c: alacritty_terminal::vte::ansi::Rgb, by: u8| {
            p.iter()
                .zip([c.r, c.g, c.b])
                .all(|(a, b)| a.abs_diff(b) <= by)
        };
        let build_for = |bare: bool, slot: Slot| {
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
                    ws.right_sidebar.files.show_for_shots(files_tree(&root));
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
            let top = if bare { TOP_BAND } else { 0.0 };
            let slot: Slot = Rc::new(RefCell::new(None));
            let build = build_for(bare, slot.clone());
            let tab_top: Rc<RefCell<Option<f32>>> = Rc::default();
            let seen = tab_top.clone();
            let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let ws = slotted(&slot);
                let n = format!("{scene}-{slug}");
                frame(cx, window);
                assert_eq!(rows_of(cx, &ws), tree_rows, "{n}: the tree");
                let pane = cx.read(|cx| ws.read(cx).model.focused_pane().expect("a pane"));
                let tab = tab_bounds(cx, &ws, pane, 0).expect("the tab laid out");
                *seen.borrow_mut() = Some(f32::from(tab.top()));
                if scene == "files-filter" {
                    let field = point(
                        px(sidebar_left + 60.0),
                        px(top + TAB_BAR + FILTER_BAND / 2.0),
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
            // The header row lines up with the pane's tab bar: same top, and its bottom edge
            // where the bar ends.
            let tab_top = tab_top.borrow().expect("the tab's top");
            assert!(
                (tab_top - top).abs() < 0.5,
                "{n}: the tab bar at {tab_top}, not {top}"
            );
            let edge = at(sidebar_left + 150.0, top + TAB_BAR - 0.5);
            assert!(
                near(edge, ui.sidebar_border, 3),
                "{n}: the header's edge: {edge:?}"
            );
            let inside = at(sidebar_left + 250.0, top + TAB_BAR / 2.0);
            assert!(
                near(inside, ui.sidebar_background, 3),
                "{n}: the header: {inside:?}"
            );
            // Each row's mark colour, somewhere along it (its name and letter are drawn in it).
            let list_top = top + TAB_BAR + FILTER_BAND;
            let inked = |row: usize, c: alacritty_terminal::vte::ansi::Rgb| {
                let y0 = list_top + row as f32 * ROW;
                (4..(ROW as usize * 2 - 4)).any(|dy| {
                    let y = y0 + dy as f32 * 0.5;
                    (0..560).any(|dx| near(at(sidebar_left + 8.0 + dx as f32 * 0.5, y), c, 24))
                })
            };
            let (m, a, d) = (
                mark_color(Mark::Modified, &theme),
                mark_color(Mark::Added, &theme),
                mark_color(Mark::Deleted, &theme),
            );
            let expect: Vec<(usize, Option<alacritty_terminal::vte::ansi::Rgb>)> =
                if scene == "files-filter" {
                    vec![(0, Some(m)), (1, Some(m))]
                } else {
                    vec![
                        (0, None),
                        (1, Some(m)),
                        (3, Some(m)),
                        (4, Some(a)),
                        (6, None),
                        (7, Some(d)),
                        (8, Some(m)),
                    ]
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
        }

        // Keys, a click, Esc (not shot).
        let slot: Slot = Rc::new(RefCell::new(None));
        let build = build_for(false, slot.clone());
        let root_for = root.clone();
        let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
            let ws = slotted(&slot);
            let n = format!("files-keys-{slug}");
            let none = Modifiers::none();
            frame(cx, window);
            let list_top = TAB_BAR + FILTER_BAND;
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
            cx.simulate_keystrokes(window, "enter");
            frame(cx, window);
            assert_eq!(rows_of(cx, &ws), tree_rows, "{n}: and opens it");
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
        let build = build_for(false, slot.clone());
        let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
            let ws = slotted(&slot);
            let n = format!("files-close-{slug}");
            frame(cx, window);
            let field = point(px(sidebar_left + 60.0), px(TAB_BAR + FILTER_BAND / 2.0));
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

fn browser_workspace(loading: bool, window: &mut Window, cx: &mut Context<Workspace>) -> Workspace {
    use crate::browser_tab::BarState;
    use crate::layout_model::TabKind;

    let mut ws = Workspace::for_shots(window, cx);
    ws.sidebar.hint_dismissed = true;
    let project = std::path::Path::new(API);
    ws.open_shot_session("api", project, SHELL_TEXT, None, window, cx);
    let pane = ws.model.focused_pane().expect("a pane");
    let url = Some(PAGE_URL.to_string());
    let opened = ws
        .model
        .split(pane, Axis::Row, TabKind::Browser { url: url.clone() })
        .expect("split");
    ws.sync();
    let view = ws.make_browser_tab(opened.tab, url, window, cx);
    view.update(cx, |view, _| {
        view.show_state(BarState {
            url: PAGE_URL.into(),
            title: PAGE_TITLE.into(),
            loading,
            can_back: true,
            can_forward: false,
        })
    });
    ws.browser_titles.insert(opened.tab, PAGE_TITLE.into());
    ws.activate(window, cx);
    ws
}

/// `fuxx new --background` (and the MCP `background`) while a browser tab has the focus: once
/// the new terminal has opened (taking the focus, as any new session does), the focus goes back
/// to the browser tab, not to the new shell. Panics unless the browser tab is focused again,
/// with GPUI's keyboard.
fn api_background() {
    use std::cell::RefCell;
    use std::rc::Rc;

    let theme = shot_theme("Catppuccin Mocha");
    let slot: Slot = Rc::new(RefCell::new(None));
    let keep = slot.clone();
    let build = move |window: &mut Window, cx: &mut App| -> AnyView {
        cx.set_global(shot_appearance(theme, cx));
        let ws = cx.new(|cx| browser_workspace(false, window, cx));
        *keep.borrow_mut() = Some(ws.clone());
        ws.into()
    };
    let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
        let ws = slotted(&slot);
        let page = cx.read(|cx| {
            let ws = ws.read(cx);
            assert!(ws.focused_browser_tab().is_some(), "api-background: a page");
            ws.model.focused_tab()
        });
        cx.update_window(window, |_, window, cx| {
            ws.update(cx, |ws, cx| {
                // What `session.new` remembers, then its session opening (beside the page).
                let previous = ws.focus_to_give_back();
                ws.open_shot_tab("helper", std::path::Path::new(API), "", window, cx);
                let id = ws.sessions.active().map(|s| s.id);
                assert!(
                    id.is_some(),
                    "api-background: the new terminal took the focus"
                );
                ws.api_opened(id, previous, None, true, window, cx)
                    .expect("opened");
            })
        })
        .expect("a background session.new");
        cx.run_until_parked();
        cx.update_window(window, |_, window, cx| {
            let ws = ws.read(cx);
            assert_eq!(
                ws.model.focused_tab(),
                page,
                "api-background: the page has the focus back"
            );
            let view = ws.focused_browser_tab().expect("the page's tab");
            assert!(
                gpui::Focusable::focus_handle(view.read(cx), cx).contains_focused(window, cx),
                "api-background: with the keyboard"
            );
        })
        .expect("read the focus");
        slot.borrow_mut().take();
    };
    render_with(&theme, size(px(960.0), px(560.0)), build, act);
}

/// A new workspace, which has no name of yours (M11c Task 3b): its sidebar row and its tab show
/// the default, `groups::DEFAULT_NAME`. Shot in both themes (`default-name-*.png`) to see that
/// the glyphs draw (no missing-glyph boxes); panics unless the label is the default.
fn default_name() {
    use std::cell::RefCell;
    use std::rc::Rc;

    for (slug, name) in SHOT_THEMES {
        let theme = shot_theme(name);
        let slot: Slot = Rc::new(RefCell::new(None));
        let keep = slot.clone();
        let build = move |window: &mut Window, cx: &mut App| -> AnyView {
            cx.set_global(shot_appearance(theme, cx));
            let ws = cx.new(|cx| {
                let mut ws = Workspace::for_shots(window, cx);
                ws.sidebar.hint_dismissed = true;
                ws.open_unnamed_shot_session(std::path::Path::new(API), SHELL_TEXT, window, cx);
                ws
            });
            *keep.borrow_mut() = Some(ws.clone());
            ws.into()
        };
        let acting = slot.clone();
        let act = move |cx: &mut VisualTestAppContext, _window: AnyWindowHandle| {
            let ws = slotted(&acting);
            let label = cx.read(|cx| ws.read(cx).sessions.items()[0].label());
            assert_eq!(
                label.as_ref(),
                crate::groups::DEFAULT_NAME,
                "default-name: a new terminal shows the default"
            );
            drop(ws);
            acting.borrow_mut().take();
        };
        let path = shoot_with(
            "default-name",
            slug,
            &theme,
            size(px(960.0), px(360.0)),
            build,
            act,
        );
        println!("{}", path.display());
    }
}

/// Two workspaces for `banner_click`: api's, and docs's split in two: docs and notes in the left
/// pane (docs shown), tests in the right (focused). gone, a tab of the left pane, was closed.
/// api's workspace is the one shown.
fn banner_click_workspace(window: &mut Window, cx: &mut Context<Workspace>) -> Workspace {
    let mut ws = Workspace::for_shots(window, cx);
    ws.sidebar.hint_dismissed = true;
    let project = std::path::Path::new(API);
    ws.open_shot_session("api", project, SHELL_TEXT, None, window, cx);
    ws.open_shot_session("docs", project, SHELL_TEXT, None, window, cx);
    ws.open_shot_tab("notes", project, AGENT_TEXT, window, cx);
    ws.open_shot_tab("gone", project, SHELL_TEXT, window, cx);
    let at = |ws: &Workspace, name: &str| ws.sessions.position(|x| x.label() == name);
    let gone = at(&ws, "gone").expect("gone");
    ws.close_at(gone, window, cx);
    ws.select(at(&ws, "docs").expect("docs"), window, cx);
    ws.open_shot_session("tests", project, SHELL_TEXT, Some(Axis::Row), window, cx);
    ws.select(at(&ws, "api").expect("api"), window, cx);
    ws
}

/// A click on one of fuxx's banners (M11c Task 2), fed in the way the click delegate feeds it:
/// through the clicks channel the workspace listens to (the delegate itself only runs from the
/// app bundle, so it can't run here). From api's workspace:
/// - a click for gone (closed since its banner), one for no terminal (a script's banner) and
///   one for notes's number from an earlier run of fuxx (numbers start again each run)
///   change nothing;
/// - a click for notes while the saved sessions are reopening shows nothing until they have,
///   then (as one with no restore running does at once) docs's workspace, its left pane and
///   notes's tab, with the keyboard;
/// - a click on a background-service banner, which the service passes on as `Show` (fed as
///   `DaemonClient::shows` hands it over), is handled the same way: held while the saved
///   sessions reopen (it opened fuxx), then notes is shown.
///
/// Shot once notes is shown (`banner-click-mocha.png`). Panics unless all of that holds.
fn banner_click() {
    use crate::layout_model::{PaneId, TabId, WorkspaceId};
    use std::cell::RefCell;
    use std::rc::Rc;

    let theme = shot_theme("Catppuccin Mocha");
    let slot: Slot = Rc::new(RefCell::new(None));
    let keep = slot.clone();
    let build = move |window: &mut Window, cx: &mut App| -> AnyView {
        cx.set_global(shot_appearance(theme, cx));
        let ws = cx.new(|cx| banner_click_workspace(window, cx));
        *keep.borrow_mut() = Some(ws.clone());
        ws.into()
    };
    type Shown = (Option<WorkspaceId>, Option<PaneId>, Option<TabId>);
    let shown = |cx: &mut VisualTestAppContext, ws: &gpui::Entity<Workspace>| -> Shown {
        cx.read(|cx| {
            let model = &ws.read(cx).model;
            (
                model.focused_workspace(),
                model.focused_pane(),
                model.focused_tab(),
            )
        })
    };
    let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
        let ws = slotted(&slot);
        let (clicks, heard) = futures::channel::mpsc::unbounded();
        cx.update_window(window, |_, window, cx| {
            ws.update(cx, |ws, cx| ws.listen_for_clicks(heard, window, cx))
        })
        .expect("listen for clicks");
        let (notes, gone, api) = cx.read(|cx| {
            let ws = ws.read(cx);
            let id = |name: &str| {
                ws.sessions
                    .items()
                    .iter()
                    .find(|s| s.label() == name)
                    .map(|s| s.id)
            };
            let notes = id("notes").expect("notes");
            // gone opened right after notes, so it had the next number.
            (notes, notes + 1, id("api"))
        });
        let before = shown(cx, &ws);
        assert_eq!(
            cx.read(|cx| ws.read(cx).focused_terminal()),
            api,
            "banner-click: api's workspace is shown first"
        );
        assert!(
            cx.read(|cx| ws.read(cx).index_of(gone).is_none()),
            "banner-click: gone (s{gone}) is closed"
        );

        // This workspace runs its sessions itself: this run numbers them.
        let run = crate::native_notify::this_run();
        let ours = |id: u64| Some(crate::native_notify::Terminal { run, id });
        let earlier = Some(crate::native_notify::Terminal {
            run: run.wrapping_add(1),
            id: notes,
        });
        for (click, what) in [
            (ours(gone), "a closed terminal"),
            (None, "no terminal"),
            (earlier, "notes's number from an earlier run"),
        ] {
            clicks.unbounded_send(click).expect("send a click");
            cx.run_until_parked();
            assert_eq!(
                shown(cx, &ws),
                before,
                "banner-click: a click for {what} changes nothing"
            );
        }

        // Where notes is, and a check that it is what's shown, with the keyboard.
        let (want_tab, want_ws, want_pane) = cx.read(|cx| {
            let model = &ws.read(cx).model;
            let tab = model.terminal_tab(notes).expect("notes's tab");
            let (w, p) = model.locate(tab).expect("notes's place");
            (tab, w, p)
        });
        let notes_shown = |cx: &mut VisualTestAppContext, how: &str| {
            let (workspace, pane, tab) = shown(cx, &ws);
            assert_ne!(workspace, before.0, "banner-click {how}: another workspace");
            assert_eq!(
                workspace,
                Some(want_ws),
                "banner-click {how}: notes's workspace"
            );
            assert_eq!(pane, Some(want_pane), "banner-click {how}: notes's pane");
            assert_ne!(
                pane, before.1,
                "banner-click {how}: not the pane focused before"
            );
            assert_eq!(tab, Some(want_tab), "banner-click {how}: notes's tab");
            cx.update_window(window, |_, window, cx| {
                let ws = ws.read(cx);
                let index = ws.index_of(notes).expect("notes");
                let view = &ws.sessions.items()[index].view;
                assert!(
                    gpui::Focusable::focus_handle(view.read(cx), cx).contains_focused(window, cx),
                    "banner-click {how}: notes has the keyboard"
                );
            })
            .expect("read the focus");
        };

        // While the saved sessions are reopening, the click waits for them to finish.
        cx.update_window(window, |_, _, cx| {
            ws.update(cx, |ws, _| ws.restoring = true)
        })
        .expect("restoring");
        clicks.unbounded_send(ours(notes)).expect("send a click");
        cx.run_until_parked();
        assert_eq!(
            shown(cx, &ws),
            before,
            "banner-click: nothing is shown while restoring"
        );
        cx.update_window(window, |_, window, cx| {
            ws.update(cx, |ws, cx| ws.finish_restoring(window, cx))
        })
        .expect("finish restoring");
        cx.run_until_parked();
        notes_shown(cx, "after the restore");

        // Back on api's workspace, a click with no restore running shows notes at once.
        cx.update_window(window, |_, window, cx| {
            ws.update(cx, |ws, cx| {
                let api = ws.sessions.position(|x| x.label() == "api").expect("api");
                ws.select(api, window, cx);
            })
        })
        .expect("back to api");
        cx.run_until_parked();
        assert_eq!(
            shown(cx, &ws),
            before,
            "banner-click: api's workspace again"
        );
        clicks.unbounded_send(ours(notes)).expect("send a click");
        cx.run_until_parked();
        notes_shown(cx, "at once");

        // A `Show` from the background service, while the saved sessions are reopening.
        let (shows, heard_shows) = futures::channel::mpsc::unbounded();
        cx.update_window(window, |_, window, cx| {
            ws.update(cx, |ws, cx| {
                // Numbered by the run the workspace numbers by, as the service's are when it
                // runs them.
                ws.listen_for_shows(heard_shows, Some(run), window, cx);
                let api = ws.sessions.position(|x| x.label() == "api").expect("api");
                ws.select(api, window, cx);
                ws.restoring = true;
            })
        })
        .expect("back to api, restoring");
        cx.run_until_parked();
        assert_eq!(
            shown(cx, &ws),
            before,
            "banner-click: api's workspace again"
        );
        shows.unbounded_send(notes).expect("send a Show");
        cx.run_until_parked();
        assert_eq!(
            shown(cx, &ws),
            before,
            "banner-click: a Show waits while restoring"
        );
        cx.update_window(window, |_, window, cx| {
            ws.update(cx, |ws, cx| ws.finish_restoring(window, cx))
        })
        .expect("finish restoring");
        cx.run_until_parked();
        notes_shown(cx, "from the service, after the restore");
        slot.borrow_mut().take();
    };
    let path = shoot_with(
        "banner-click",
        "mocha",
        &theme,
        size(px(1280.0), px(600.0)),
        build,
        act,
    );
    println!("ui-shots: {}", path.display());
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
    for (slug, name) in SHOT_THEMES {
        let theme = shot_theme(name);
        for scene in [
            "browser-tab",
            "browser-tab-title",
            "browser-tab-suggestions",
        ] {
            let typed_at: Rc<std::cell::Cell<Option<gpui::Bounds<Pixels>>>> = Rc::default();
            let typed = typed_at.clone();
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
                // Under the 26 px tab bar, centred in the 34 px bar.
                assert_eq!(f32::from(field.size.height), 24.0, "{n}: field height");
                let top = f32::from(field.top());
                assert!((29.0..=32.0).contains(&top), "{n}: field top {top}");
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
                ws.switcher.as_ref().map(|s| s.read(cx).shown().count())
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
                    let switcher = ws.read(cx).switcher.clone();
                    switcher
                        .map(|s| gpui::Focusable::focus_handle(s.read(cx), cx).is_focused(window))
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
                    ws.close_overlays(crate::workspace::Overlay::Switcher);
                    ws.switcher = None;
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

/// The switcher's eight sessions: api (waiting with a boxed question, working, done, idle), web
/// (waiting with no question found, done) and the SSH host build-box (working, idle).
const SWITCHER_EIGHT: [ShotSession; 8] = [
    shot("claude", API, Status::Waiting, 4 * 60 + 12, CLAUDE_PROMPT),
    shot("codex", API, Status::Working, 32, AGENT_TEXT),
    shot("tests", API, Status::Done, 12 * 60, SHELL_TEXT),
    shot("shell", API, Status::Idle, 0, SHELL_TEXT),
    shot("agent", WEB, Status::Waiting, 95, SHELL_TEXT),
    shot("build", WEB, Status::Done, 40, SHELL_TEXT),
    shot("deploy", OPS, Status::Working, 3 * 60, AGENT_TEXT),
    shot("logs", OPS, Status::Idle, 0, SHELL_TEXT),
];

/// The folder build-box's sessions open in, before they're made SSH sessions to it.
const OPS: &str = "/Users/me/ops";

/// `SWITCHER_EIGHT` with shell (4) active, its last two as SSH sessions to build-box.
fn switcher_workspace(window: &mut Window, cx: &mut Context<Workspace>) -> Workspace {
    let mut ws = sidebar_workspace(&SWITCHER_EIGHT, None, 3, &[], window, cx);
    ws.sidebar.hint_dismissed = true;
    for i in [6, 7] {
        ws.sessions.get_mut(i).expect("opened").ssh = Some(crate::ssh_target::SshTarget {
            host: "build-box".into(),
            tmux: false,
            tmux_name: None,
        });
    }
    ws
}

/// The ⌘P switcher over a real workspace, in both themes:
/// - `switcher`: just opened, every session most urgent first (claude, which waited longest, then
///   agent, the done ones, the working ones, the idle ones);
/// - `switcher-filter`: "build" typed, which finds web's build session by name and build-box's
///   two by group;
/// - `rename`: the rename box just opened on shell (the selected row, filled with the accent),
///   its old name selected; `rename-typed`: "api-2" typed over it.
///
/// Panics unless the rows are in that order, the filter leaves three, the rename box has the
/// old (or typed) name, and the box's text is drawn in `text` (it reads on the box, whatever the
/// row under it is filled with).
fn switcher_scenes() {
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    let dims = size(px(960.0), px(600.0));
    for (slug, name) in SHOT_THEMES {
        let theme = shot_theme(name);
        let ui = theme.ui();
        for scene in ["switcher", "switcher-filter", "rename", "rename-typed"] {
            let slot: Slot = Rc::new(RefCell::new(None));
            let keep = slot.clone();
            let build = move |window: &mut Window, cx: &mut App| -> AnyView {
                cx.set_global(sidebar_appearance(theme, cx));
                let ws = cx.new(|cx| switcher_workspace(window, cx));
                *keep.borrow_mut() = Some(ws.clone());
                ws.into()
            };
            // The renamed row's bounds: left, right, top.
            let renamed = Rc::new(Cell::new(None));
            let found = renamed.clone();
            let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let ws = slotted(&slot);
                let renaming = scene.starts_with("rename");
                cx.update_window(window, |_, window, cx| {
                    ws.update(cx, |ws, cx| {
                        if renaming {
                            ws.start_rename(ws.model.display_order()[3], window, cx)
                        } else {
                            ws.open_switcher(window, cx)
                        }
                    })
                })
                .expect("open it");
                cx.run_until_parked();
                match scene {
                    "switcher-filter" => cx.simulate_input(window, "build"),
                    "rename-typed" => cx.simulate_input(window, "api-2"),
                    _ => {}
                }
                let shown = |cx: &mut VisualTestAppContext| {
                    cx.read(|cx| {
                        let switcher = ws.read(cx).switcher.clone().expect("the switcher");
                        let rows = switcher.read(cx).shown().map(|e| e.label.clone());
                        rows.collect::<Vec<_>>()
                    })
                };
                match scene {
                    "switcher" => assert_eq!(
                        shown(cx),
                        [
                            "claude", "agent", "tests", "build", "codex", "deploy", "shell", "logs"
                        ],
                        "{scene}-{slug}: most urgent first"
                    ),
                    "switcher-filter" => assert_eq!(
                        shown(cx),
                        ["build", "deploy", "logs"],
                        "{scene}-{slug}: filtered by name and group"
                    ),
                    _ => cx.read(|cx| {
                        let ws = ws.read(cx);
                        let (_, input) = ws.renaming.as_ref().expect("the rename box is open");
                        let typed = if scene == "rename" { "shell" } else { "api-2" };
                        assert_eq!(input.read(cx).text(), typed, "{scene}-{slug}");
                        // shell's row: under api's header, after claude, codex and tests.
                        let b = ws.sidebar.scroll.bounds_for_item(4).expect("laid out");
                        found.set(Some((
                            f32::from(b.left()),
                            f32::from(b.right()),
                            f32::from(b.top()),
                        )));
                    }),
                }
                // Not kept past the window: a handle left over is reported as a leak.
                slot.borrow_mut().take();
            };
            let path = shoot_with(scene, slug, &theme, dims, build.clone(), act.clone());
            println!("ui-shots: {}", path.display());
            if scene.starts_with("rename") {
                let shot = render_with(&theme, dims, build, act);
                let (left, right, top) = renamed.get().expect("the renamed row");
                // The box, right of the mark, along the name line.
                let x0 = left + 8.0 + 14.0 + 8.0;
                let ink: usize = (0..30)
                    .map(|n| top + 4.0 + n as f32 * 0.5)
                    .map(|y| count_along(&shot, dims, ui.text, y, x0, right - 40.0))
                    .sum();
                assert!(
                    ink >= 20,
                    "{scene}-{slug}: the box's text is in `text` ({ink} pixels)"
                );
            }
        }
    }
}

/// One box over the window at a time: opening the rename box, the switcher, the address box or
/// the Help panel over another closes that one, and the new one keeps the keyboard (the old one's
/// late "lost focus" must not take it back). Rename -> switcher -> rename -> address box ->
/// switcher -> help -> rename -> help -> switcher, each checked after frames have run the focus
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
                if let Some(s) = &ws.switcher {
                    open.push(("switcher", s.focus_handle(cx).is_focused(window)));
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
            ("switcher", "switcher"),
            ("rename", "rename"),
            ("address", "address"),
            ("switcher", "switcher"),
            ("help", "help"),
            ("rename", "rename"),
            ("help", "help"),
            ("switcher", "switcher"),
        ] {
            cx.update_window(window, |_, window, cx| {
                ws.update(cx, |ws, cx| match step {
                    "rename" => ws.start_rename(ws.model.display_order()[3], window, cx),
                    "address" => ws.open_address_box(window, cx),
                    "help" => {
                        ws.open_help(crate::help_panel::HelpPage::Shortcuts, true, window, cx)
                    }
                    _ => ws.open_switcher(window, cx),
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
                        Overlay::Switcher => ws.open_switcher(window, cx),
                        Overlay::TabMenu => ws.open_tab_menu(first, at, window, cx),
                        Overlay::PaneTabMenu => {
                            let tab = ws.model.focused_tab().expect("a focused tab");
                            ws.open_pane_tab_menu(tab, at, window, cx)
                        }
                        Overlay::Notifications => ws.open_notifications(window, cx),
                        Overlay::Help => {
                            ws.open_help(crate::help_panel::HelpPage::Shortcuts, true, window, cx)
                        }
                    }
                })
            })
            .expect("open it");
            cx.update_window(window, |_, window, cx| window.draw(cx).clear())
                .expect("draw");
            cx.run_until_parked();
            let other = if overlay == Overlay::Rename {
                Overlay::Switcher
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
        // it: ⌘P's switcher isn't left under its click-away backdrop.
        cx.update(|cx| {
            ws.update(cx, |ws, _| ws.sidebar.new_menu = true);
            assert!(
                ws.read(cx).overlays(cx).any(),
                "overlays: the + menu hides the pages"
            );
        });
        cx.update_window(window, |_, window, cx| {
            ws.update(cx, |ws, cx| ws.open_switcher(window, cx))
        })
        .expect("open the switcher");
        cx.update(|cx| {
            let ws = ws.read(cx);
            assert!(
                !ws.sidebar.new_menu && ws.switcher.is_some(),
                "overlays: ⌘P closes the + menu"
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

/// The Help panel (M11d) over the switcher's workspace, in both themes:
/// - `help-shortcuts`: Keyboard Shortcuts just opened, every listed binding in its group;
/// - `help-split`: "split" typed, which leaves Split Right and Split Down;
/// - `help-none`: "zzz" typed, which finds nothing ("No shortcut matches");
/// - `help-commands`: the Command Reference with "browser" typed;
/// - `help-over-switcher`: Keyboard Shortcuts opened while ⌘P's switcher is open;
/// - `help-toggle`: Keyboard Shortcuts opened and opened again (as ⌘/ twice does);
/// - `help-tips`: Help → Show Welcome Tips with the sidebar hidden and the tips card closed.
///
/// Panics unless the rows shown are the ones the pure modules list for that text, the panel
/// opened over the switcher closed it and has the keyboard, and the second open closed the panel
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
    assert!(
        browser >= 8,
        "help: every browser command matches \"browser\""
    );

    let dims = size(px(960.0), px(600.0));
    for (slug, name) in SHOT_THEMES {
        let theme = shot_theme(name);
        for scene in [
            "help-shortcuts",
            "help-split",
            "help-none",
            "help-commands",
            "help-over-switcher",
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
                    "help-shortcuts" | "help-split" | "help-none" => {
                        open(cx, HelpPage::Shortcuts);
                        let (typed, rows) = match scene {
                            "help-split" => ("split", split),
                            "help-none" => ("zzz", 0),
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
                    "help-over-switcher" => {
                        cx.update_window(window, |_, window, cx| {
                            ws.update(cx, |ws, cx| ws.open_switcher(window, cx))
                        })
                        .expect("open the switcher");
                        cx.run_until_parked();
                        open(cx, HelpPage::Shortcuts);
                        assert!(
                            cx.read(|cx| ws.read(cx).switcher.is_none()),
                            "{n}: the help closed the switcher"
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

/// The host picker and the address box over the same workspace as the switcher shots, in both
/// themes (`host-picker-*`, `address-box-*`), to compare with `switcher-*`: one panel look.
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
    for (slug, name) in SHOT_THEMES {
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

/// The Preferences window at its computed size, each section in both themes
/// (`preferences-<section>-mocha`, `-latte`; General with the tips card dismissed, so its button
/// is live, and `preferences-general-shown-mocha` with the card showing), and Appearance with Nord
/// picked and ligatures off at 16 pt (`preferences-appearance-nord`) and 20 pt
/// (`preferences-appearance-nord-20pt`), so the lists, switches and preview follow the choices and
/// the preview's text fits its narrower screen.
///
/// Panics unless nothing is drawn in the bottom padding and Appearance's last control sits just
/// above it (the window is the tallest section's height), the shown section's row in the list
/// has the selection colour and the others don't, and Appearance's preview shows the theme's
/// background and its magenta (the agent's box).
fn preferences_scenes() {
    use crate::preferences::{self, PreferencesView, SECTIONS_WIDTH, Section};

    let dims = preferences::window_size();
    // (slug, theme, section, tips card dismissed, font size changed to)
    let mut shots: Vec<(String, &str, Section, bool, Option<f32>)> = Vec::new();
    for section in Section::ALL {
        let name = format!("{section:?}").to_lowercase();
        for (slug, theme) in SHOT_THEMES {
            shots.push((format!("{name}-{slug}"), theme, section, true, None));
        }
    }
    let mocha = SHOT_THEMES[0].1;
    shots.push((
        "general-shown-mocha".into(),
        mocha,
        Section::General,
        false,
        None,
    ));
    for (slug, size) in [("appearance-nord", 16.0), ("appearance-nord-20pt", 20.0)] {
        shots.push((slug.into(), "Nord", Section::Appearance, true, Some(size)));
    }
    for (slug, name, section, dismissed, changed) in shots {
        let theme = shot_theme(name);
        let build = move |window: &mut Window, cx: &mut App| -> AnyView {
            let mut a = shot_appearance(theme, cx);
            a.theme_name = Some(name.to_string());
            if let Some(size) = changed {
                a.font_settings.size = size;
                a.font_settings.features = vec![("calt".into(), 0)];
                a.font = fonts::gpui_font(&a.font_settings, &cx.text_system().all_font_names());
                a.font_size = size;
            }
            cx.set_global(a);
            cx.new(|cx| PreferencesView::new(window, cx).shown_as(section, dismissed))
                .into()
        };
        let path = shoot("preferences", &slug, &theme, dims, build);
        let shot = render(&theme, dims, build);
        let ui = theme.ui();
        let scene = format!("preferences-{slug}");
        let width = f32::from(dims.width);
        let height = f32::from(dims.height);
        let scale = shot.width as f32 / width;
        // Every pixel along the row at `y`, from `x0` to `x1`, is the window's background.
        let full = |y: f32, x0: f32, x1: f32| {
            let pixels = (x1 * scale) as usize - (x0 * scale) as usize;
            count_along(&shot, dims, ui.sidebar_background, y, x0, x1) == pixels
        };
        // The bottom padding is empty beside the section list's edge: the tallest section ends
        // where the height says it does.
        for y in [height - 14.0, height - 8.0, height - 3.0] {
            assert!(
                full(y, 0.0, SECTIONS_WIDTH - 1.0) && full(y, SECTIONS_WIDTH, width),
                "{scene}: something is drawn at y = {y}"
            );
        }
        // The shown section's row is filled with the selection colour; the others aren't.
        for s in Section::ALL {
            let (x, y, w, h) = preferences::section_row_area(s);
            let filled = count_along(&shot, dims, ui.selection, y + h / 2.0, x, x + w);
            if s == section {
                assert!(
                    filled as f32 > w * scale / 2.0,
                    "{scene}: the {s:?} row isn't highlighted"
                );
            } else {
                assert_eq!(filled, 0, "{scene}: the {s:?} row is highlighted");
            }
        }
        if section != Section::Appearance {
            println!("ui-shots: {}", path.display());
            continue;
        }
        // The last control is drawn just above the padding.
        let (cx0, cy0, cw, ch) = preferences::last_control_area();
        let inked = (0..ch as usize).any(|dy| !full(cy0 + dy as f32, cx0, cx0 + cw));
        assert!(
            inked,
            "{scene}: the last control isn't where the layout puts it"
        );
        // The preview is painted in the theme: its background and the agent box's magenta.
        let (px0, py0, pw, ph) = preferences::preview_area();
        let rows = (py0 as usize + 4..(py0 + ph) as usize - 4).map(|y| y as f32);
        let count = |c, y| count_along(&shot, dims, c, y, px0 + 4.0, px0 + pw - 4.0);
        assert!(
            count(theme.background, py0 + 4.0) > 0,
            "{scene}: no terminal background in the preview"
        );
        assert!(
            rows.into_iter().any(|y| count(theme.palette[5], y) > 0),
            "{scene}: the preview's agent box isn't in the theme's magenta"
        );
        println!("ui-shots: {}", path.display());
    }
}

/// The sidebar button in the top band, as small windows of the top left corner, in both themes:
/// `sidebar-button-shown`, `-hidden`, `-hover` (the pointer over it) and `-sliding` (a hide
/// 60 ms in, the sidebar partly out and over the pane column).
///
/// Panics unless the icon is drawn in the same place shown and hidden, the hover paints the
/// hit area, and mid-slide the sidebar has left the middle of the window (it is only partly out).
fn sidebar_button_scenes() {
    use crate::sidebar_toggle::{Slide, button_origin};
    use gpui::Modifiers;
    use std::time::{Duration, Instant};

    let dims = size(px(360.0), px(100.0));
    let (bx, by) = button_origin(false);
    // The hit area's left margin (inside it, outside the icon), and the icon's left edge.
    let (margin, edge) = (point(bx + 1.5, by + 12.0), point(bx + 4.75, by + 12.0));
    for (slug, name) in SHOT_THEMES {
        let theme = shot_theme(name);
        let ui = theme.ui();
        let mut shots = Vec::new();
        for scene in ["shown", "hidden", "hover", "sliding"] {
            let build = move |window: &mut Window, cx: &mut App| -> AnyView {
                cx.set_global(sidebar_appearance(theme, cx));
                let ws = cx.new(|cx| {
                    let mut ws = sidebar_workspace(&EIGHT, None, 0, &[], window, cx);
                    ws.sidebar.hint_dismissed = true;
                    ws.sidebar.hidden = scene == "hidden" || scene == "sliding";
                    if scene == "sliding" {
                        ws.slide = Some(Slide {
                            showing: false,
                            since: Instant::now() - Duration::from_millis(60),
                            from: 1.0,
                        });
                    }
                    ws
                });
                ws.into()
            };
            let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                if scene == "hover" {
                    let at = point(px(bx + 12.0), px(by + 12.0));
                    cx.simulate_mouse_move(window, at, None, Modifiers::none());
                }
            };
            let shot = render_with(&theme, dims, build, act);
            let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/ui-shots");
            std::fs::create_dir_all(&dir).expect("create target/ui-shots");
            let path = dir.join(format!("sidebar-button-{scene}-{slug}.png"));
            std::fs::write(&path, png(&shot)).expect("write the PNG");
            println!("ui-shots: {}", path.display());
            shots.push((scene, shot));
        }
        let at = |scene: &str, p: gpui::Point<f32>| {
            let (_, shot) = shots.iter().find(|(s, _)| *s == scene).expect("shot");
            pixel_at(shot, dims, p.x, p.y)
        };
        let bg = |rgb: alacritty_terminal::vte::ansi::Rgb| [rgb.r, rgb.g, rgb.b];
        // Same place in both states: the icon's left edge is not the band's background.
        assert_ne!(
            at("shown", edge),
            bg(ui.sidebar_background),
            "{slug}: icon shown"
        );
        assert_ne!(
            at("hidden", edge),
            bg(theme.background),
            "{slug}: icon hidden"
        );
        // Hover: the hit area is panel_hover, not the sidebar's colour.
        assert_eq!(
            at("shown", margin),
            bg(ui.sidebar_background),
            "{slug}: not hovered"
        );
        assert_eq!(at("hover", margin), bg(ui.panel_hover), "{slug}: hovered");
        let (inside, beyond) = (point(30.0, 80.0), point(150.0, 80.0));
        assert_eq!(
            at("sliding", beyond),
            bg(theme.background),
            "{slug}: slid out"
        );
        assert_ne!(
            at("sliding", inside),
            bg(theme.background),
            "{slug}: still partly in"
        );
    }
}

/// A long question, so an entry's second line is cut.
const LONG_QUESTION: &str = "Do you want to run cargo test --workspace --all-features -- \
    --include-ignored --test-threads=1 in /Users/me/code/web before the migration?";

/// The notifications fuxx would have raised over `switcher_workspace`, oldest first: a script's
/// `fuxx notify` from outside fuxx's terminals (50 m ago), build finished (40 m), a terminal since closed (`s99`, 25 m), tests finished (12 m), claude
/// waits (4 m) and agent waits with a long question (95 s).
fn shot_notifications(ws: &mut Workspace) {
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

/// The title area over `switcher_workspace` (shell active), in both themes:
/// - `title-band`: the sidebar shown, its top band holding only the sidebar button, and the
///   footer's +, 🔔 and gear, nothing unseen;
/// - `title-hover`: the same with the pointer on the footer's 🔔 (its fill, its outline brighter);
/// - `title-unseen`: the same with notifications unseen (the 🔔's dot);
/// - `title-footer-narrow`: the narrowest sidebar (180 px) with the dot: the three buttons fit;
/// - `title-hidden`: the sidebar hidden: the band runs over the panes with the workspace's name
///   centred in it, and no footer, so no 🔔;
/// - `title-narrow`: hidden, a 560 px window and a long name, cut with an ellipsis;
/// - `title-popover`: the popover open upward from the 🔔 over six notifications, newest first
///   (agent's long question cut, a closed terminal's entry greyed, a script's last with its own
///   title);
/// - `title-popover-empty`: the popover with none yet.
///
/// Panics unless the dot shows exactly while some are unseen (and never with the sidebar
/// hidden), the band holds no 🔔 or + where they were, opening the popover sees them all (and it
/// counts as an overlay), the name is drawn in the band only with the sidebar hidden and never
/// past its inset; and (Mocha) unless a press where the buttons were still arms a window drag,
/// a press on the 🔔 doesn't, a click on the 🔔 opens the popover and Escape or a click outside
/// closes it, a click on the + opens its menu, a click on a closed terminal's entry (or a
/// script's with no terminal) does nothing, a click on agent's entry shows agent (its workspace
/// and tab) with the keyboard, and hiding the sidebar closes the popover.
fn title_area_scenes() {
    use crate::sidebar_toggle::{HIT, button_origin};
    use crate::tab_menu::Dismiss;
    use crate::title_area::{
        DOT, DOT_INSET, FOOTER_BUTTON, entry_top, footer_buttons, name_inset, popover_anchor,
        popover_width,
    };
    use crate::ui_tokens::size::{SIDEBAR_DEFAULT_WIDTH, SIDEBAR_MIN_WIDTH};
    use gpui::{Focusable, KeyBinding, Modifiers, MouseButton};
    use std::cell::RefCell;
    use std::rc::Rc;

    const LONG_NAME: &str = "Refactor the auth middleware to use the new session store";
    const FOOTER: f32 = 36.0;
    // Where the 🔔 and + sat in the band, right of the sidebar button, before they moved.
    let (sx, sy) = button_origin(false);
    let old_buttons = (sx + HIT + 2.0, sx + HIT + 2.0 + 2.0 * HIT + 2.0);
    for (slug, name) in SHOT_THEMES {
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
            let centre_y = height - FOOTER + 1.0 + (FOOTER - 1.0) / 2.0;
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
            let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/ui-shots");
            std::fs::create_dir_all(&dir).expect("create target/ui-shots");
            let path = dir.join(format!("{scene}-{slug}.png"));
            std::fs::write(&path, png(&shot)).expect("write the PNG");
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
                        // one line, and the + as far from the sidebar's left edge as the gear
                        // is from its right edge (inside the border).
                        let foot = bg(ui.sidebar_background);
                        let full = {
                            let d = bg(ui.text_dim);
                            (0..3)
                                .map(|k| (i32::from(d[k]) - i32::from(foot[k])).pow(2))
                                .sum::<i32>()
                        };
                        let ink_box = |x0: f32| -> (f32, f32, f32, f32) {
                            let (mut x_lo, mut x_hi) = (f32::MAX, f32::MIN);
                            let (mut y_lo, mut y_hi) = (f32::MAX, f32::MIN);
                            for dy in 0..FOOTER_BUTTON as usize {
                                for dx in 0..FOOTER_BUTTON as usize {
                                    let (x, y) = (
                                        x0 + dx as f32,
                                        centre_y - FOOTER_BUTTON / 2.0 + dy as f32,
                                    );
                                    let p = at(x, y);
                                    let d: i32 = (0..3)
                                        .map(|k| (i32::from(p[k]) - i32::from(foot[k])).pow(2))
                                        .sum();
                                    if d * 4 >= full {
                                        (x_lo, x_hi) = (x_lo.min(x), x_hi.max(x + 1.0));
                                        (y_lo, y_hi) = (y_lo.min(y), y_hi.max(y + 1.0));
                                    }
                                }
                            }
                            (x_lo, x_hi, y_lo, y_hi)
                        };
                        let boxes = [ink_box(plus_x), ink_box(bell_x), ink_box(gear_x)];
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
                        let (left, right) = (boxes[0].0, sidebar_width - 1.0 - boxes[2].1);
                        assert!(
                            (left - right).abs() <= 1.0,
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
                    let (left, bottom) = popover_anchor(sidebar_width, height);
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
            // The band holds no 🔔 or + where they were: only the sidebar background (or, with
            // the sidebar hidden, the content's) between the sidebar button and the name.
            let band_bg = if hidden {
                bg(theme.background)
            } else {
                bg(ui.sidebar_background)
            };
            let inset = name_inset(false);
            if !matches!(
                scene,
                "title-footer-narrow" | "title-bare-narrow" | "title-popover-narrow"
            ) {
                let stop = if hidden {
                    inset - 2.0
                } else {
                    old_buttons.1 + HIT
                };
                for y in [sy + 2.0, sy + HIT / 2.0, sy + HIT - 2.0] {
                    let mut x = sx + HIT + 2.0;
                    while x < stop {
                        assert_eq!(at(x, y), band_bg, "{n}: the band is bare at ({x}, {y})");
                        x += 1.0;
                    }
                }
            }
            // The name: ink in the band's middle with the sidebar hidden, none past its inset.
            let band_ink = |x0: f32, x1: f32| -> usize {
                (0..20)
                    .map(|k| 9.0 + k as f32)
                    .map(|y| {
                        let mut x = x0;
                        let mut inked = 0;
                        while x < x1 {
                            inked += usize::from(at(x, y) != bg(theme.background));
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
                assert_eq!(
                    band_ink(width - inset, width),
                    0,
                    "{n}: nothing right of the name's room"
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
        let foot_y = height - FOOTER / 2.0;
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
        // Where the 🔔 and + were in the band, the band drags the window: no dead hit area.
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
        let popover_left = popover_anchor(SIDEBAR_DEFAULT_WIDTH, height).0;
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

/// The Update Available button above the sidebar's footer, over `switcher_workspace`, in both
/// themes, with v0.3.2 the newer release:
/// - `update-idle`: nothing known: no button;
/// - `update-available`: a newer release known: the blue button, a white package icon, then
///   white "Update Available" and a fainter " · v0.3.2";
/// - `update-hover`: the pointer on it: the darker blue;
/// - `update-downloading`: "Downloading v0.3.2…", with the pointer on it and a click: still
///   the plain blue (no hover) and still downloading;
/// - `update-narrow`: the narrowest sidebar: the version left out, "Update Available" whole.
///
/// Panics unless the button shows exactly when there is something to say, is 28 px tall and
/// 8 px above the footer's top edge, inset `ROW_INSET` each side, filled `#1F6FEB` (`#1A5FCC`
/// hovered while clickable); its icon's ink is about 16 px wide and 6 px before the words, the
/// pair centred; the words' ink is as wide as the text it should say (the package, the gap and
/// that text, measured as the button draws it); and the version part is white at 75% (never
/// full white) while the words are full white.
fn update_button_scenes() {
    use crate::sidebar::{FOOTER, ROW_INSET, UPDATE_BUTTON, UPDATE_FILL, UPDATE_GAP};
    use crate::sidebar::{UPDATE_HOVER, UPDATE_ICON, UPDATE_ICON_GAP, UPDATE_RADIUS, UPDATE_TEXT};
    use crate::ui_tokens::size::{SIDEBAR_DEFAULT_WIDTH, SIDEBAR_MIN_WIDTH};
    use crate::update_release::{Release, Version};
    use crate::updater::{UpdateState, Updates};
    use gpui::Modifiers;
    use std::cell::RefCell;
    use std::rc::Rc;

    let dims = size(px(760.0), px(420.0));
    let height = f32::from(dims.height);
    // Where the button should be, in points: its top and bottom.
    let bottom = height - FOOTER - UPDATE_GAP;
    let top = bottom - UPDATE_BUTTON;
    let mid = (top + bottom) / 2.0;
    let version = Version(0, 3, 2);
    let release = Release {
        version,
        page_url: "https://github.com/fitzfitz/fuxx-cli/releases/tag/v0.3.2".into(),
        notes: "Faster startup".into(),
        zip: None,
    };
    let near = |p: [u8; 3], c: alacritty_terminal::vte::ansi::Rgb| {
        p.iter()
            .zip([c.r, c.g, c.b])
            .all(|(a, b)| a.abs_diff(b) <= 3)
    };
    for (slug, name) in SHOT_THEMES {
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
            let width = if scene == "update-narrow" {
                SIDEBAR_MIN_WIDTH
            } else {
                SIDEBAR_DEFAULT_WIDTH
            };
            // Its left and right edges (the sidebar's 1 px right border is outside its content).
            let (left, right) = (ROW_INSET, width - 1.0 - ROW_INSET);
            let state = match scene {
                "update-idle" => UpdateState::Idle,
                "update-downloading" => UpdateState::Downloading(version),
                _ => UpdateState::Available(release.clone()),
            };
            // What it should say, all of it (`None`: no button).
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
                    let mut ws = switcher_workspace(window, cx);
                    ws.sidebar.width = width;
                    ws
                });
                *keep.borrow_mut() = Some(ws.clone());
                ws.into()
            };
            let n = format!("{scene}-{slug}");
            // The words' widths as the button draws them: what it says, and "Update Available".
            let widths: Rc<RefCell<(f32, f32)>> = Rc::default();
            let act = {
                let n = n.clone();
                let widths = widths.clone();
                move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                    cx.update_window(window, |_, window, _| {
                        let measure = |t: &str| crate::sidebar::update_text_width(t, window);
                        *widths.borrow_mut() =
                            (measure(says.unwrap_or("")), measure("Update Available"));
                    })
                    .expect("measure the words");
                    let on = point(px(left + 12.0), px(mid));
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
            let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/ui-shots");
            std::fs::create_dir_all(&dir).expect("create target/ui-shots");
            let path = dir.join(format!("{n}.png"));
            std::fs::write(&path, png(&shot)).expect("write the PNG");
            println!("ui-shots: {}", path.display());

            let scale = shot.width as f32 / f32::from(dims.width);
            let at = |x: f32, y: f32| pixel_at(&shot, dims, x, y);
            // The fill expected where the text isn't: near its left end, half-way down.
            let fill = if scene == "update-hover" {
                UPDATE_HOVER
            } else {
                UPDATE_FILL
            };
            // The device rows, down a line just inside its left end, that are either blue.
            let x = left + 12.0;
            let rows: Vec<f32> = (0..(height * scale) as usize)
                .map(|dy| dy as f32 / scale)
                .filter(|&y| near(at(x, y), UPDATE_FILL) || near(at(x, y), UPDATE_HOVER))
                .collect();
            let Some(says) = says else {
                assert!(rows.is_empty(), "{n}: no button, but blue at {rows:?}");
                continue;
            };
            assert!(!rows.is_empty(), "{n}: no button");
            let (first, last) = (rows[0], rows[rows.len() - 1] + 1.0 / scale);
            println!("ui-shots: {n}: button rows {first}..{last}");
            assert!(
                (first - top).abs() <= 0.5 && (last - bottom).abs() <= 0.5,
                "{n}: the button spans {first}..{last}, expected {top}..{bottom}"
            );
            assert_eq!(
                rows.len(),
                (UPDATE_BUTTON * scale) as usize,
                "{n}: one unbroken 28 px run"
            );
            assert!(
                near(at(x, mid), fill),
                "{n}: filled {fill:?}: {:?}",
                at(x, mid)
            );
            // Inset ROW_INSET each side: blue just inside each end, not just outside.
            let half = 0.5 / scale;
            assert!(near(at(left + half, mid), fill), "{n}: its left end");
            assert!(near(at(right - half, mid), fill), "{n}: its right end");
            assert!(!near(at(left - half, mid), fill), "{n}: nothing left of it");
            assert!(
                !near(at(right + half, mid), fill),
                "{n}: nothing right of it"
            );
            // The ink: any pixel far brighter than the blue (red over 128; the fills' is 0x1F
            // or 0x1A), so anti-aliased lines and the "…" count. Looked for between the
            // corners' radii, where the sidebar doesn't show through.
            let band = UPDATE_BUTTON - 2.0 * UPDATE_RADIUS;
            let ys: Vec<f32> = (0..(band * scale) as usize)
                .map(|dy| top + UPDATE_RADIUS + dy as f32 / scale)
                .collect();
            let brightest = |x: f32| ys.iter().map(|&y| at(x, y)[0]).max().unwrap_or(0);
            let xs: Vec<f32> = (0..((right - left) * scale) as usize)
                .map(|dx| left + dx as f32 / scale)
                .collect();
            let inked: Vec<f32> = xs.iter().copied().filter(|&x| brightest(x) > 128).collect();
            assert!(!inked.is_empty(), "{n}: no icon or words");
            let step = 1.0 / scale;
            let (w0, w1) = (inked[0], inked[inked.len() - 1] + step);
            // The icon: the first stretch of ink, up to the first clear gap of 3 px or more.
            let icon_end = inked
                .windows(2)
                .find(|w| w[1] - w[0] >= 3.0)
                .map(|w| w[0] + step)
                .expect("a gap between the icon and the words");
            let words_start = inked.iter().copied().find(|&x| x > icon_end).unwrap();
            let (icon, gap) = (icon_end - w0, words_start - icon_end);
            let (said, plain) = *widths.borrow();
            println!(
                "ui-shots: {n}: icon {icon} px, gap {gap} px, ink {} px for {said} px of words, \
                 {} from the left, {} from the right",
                w1 - w0,
                w0 - left,
                right - w1
            );
            // The pair centred on the button: the icon's box starts where its ink does (to a
            // pixel), and the pair runs on for the icon, the gap and the words as laid out (the
            // words' last letter's ink stops short of its advance).
            let (l, r) = (
                w0 - left,
                right - (w0 + UPDATE_ICON + UPDATE_ICON_GAP + said),
            );
            assert!(
                (l - r).abs() <= 2.0,
                "{n}: the icon and words sit {l} from the left, {r} from the right"
            );
            assert!(
                (icon - UPDATE_ICON).abs() <= 1.0,
                "{n}: the icon's ink is {icon} px wide, its box {UPDATE_ICON}"
            );
            // The words' own side bearing adds a little to the gap.
            assert!(
                (UPDATE_ICON_GAP - 0.5..=UPDATE_ICON_GAP + 1.5).contains(&gap),
                "{n}: {gap} px between the icon and the words, expected {UPDATE_ICON_GAP}"
            );
            // The words say what they should: as wide as that text (the icon, gap and words).
            let expected = UPDATE_ICON + UPDATE_ICON_GAP + said;
            assert!(
                ((w1 - w0) - expected).abs() <= 2.0,
                "{n}: ink {} px wide, expected {expected} for {says:?}",
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
            "{slug}: the narrow button left its version out: {ink:?}"
        );
    }
}

/// One terminal in a hidden window for the `cursor` scene: a `cat` showing `text`, the
/// Appearance as `tweak` leaves it, motion off when `still`. `focus`: the terminal has the
/// focus; `key`: it counts its window as the key one (an off-screen window never is).
struct CursorRig {
    // Before `cx`: fields drop in order, and a handle must go before its app.
    view: gpui::Entity<crate::terminal_view::TerminalView>,
    cx: VisualTestAppContext,
    window: AnyWindowHandle,
    dims: Size<Pixels>,
}

impl CursorRig {
    fn new(
        theme: Theme,
        text: &str,
        focus: bool,
        key: bool,
        still: bool,
        tweak: impl FnOnce(&mut Appearance),
    ) -> Self {
        use crate::terminal_view::TerminalView;
        use gpui::Focusable;
        use std::cell::RefCell;
        use std::rc::Rc;

        let dims = size(px(320.0), px(120.0));
        let mut cx = gpui::VisualTestAppContext::new(current_platform(false));
        fonts::register_bundled_fonts(&cx.text_system().all_font_names());
        let slot = Rc::new(RefCell::new(None));
        let text = text.to_string();
        let window = {
            let slot = slot.clone();
            cx.open_offscreen_window(dims, move |window, cx| {
                cx.set_reduce_motion(still);
                let mut appearance = shot_appearance(theme, cx);
                tweak(&mut appearance);
                cx.set_global(appearance);
                let quiet = crate::pty::ShellEnv {
                    base: None,
                    set: Vec::new(),
                    remove: Vec::new(),
                };
                let view =
                    TerminalView::spawn(cx, Some(vec!["/bin/cat".into()]), None, None, &quiet)
                        .expect("start cat");
                view.read(cx).show_text(&text);
                if key {
                    view.read(cx).pretend_key_window();
                }
                if focus {
                    view.focus_handle(cx).focus(window, cx);
                }
                *slot.borrow_mut() = Some(view.clone());
                cx.new(|_| Root {
                    theme,
                    scene: view.into(),
                })
            })
            .expect("open an off-screen window")
        };
        cx.run_until_parked();
        let view = slot.borrow().clone().expect("the terminal");
        Self {
            view,
            cx,
            window: window.into(),
            dims,
        }
    }

    /// Draw a frame now and capture it.
    fn frame(&mut self) -> Shot {
        self.cx.run_until_parked();
        self.cx
            .update_window(self.window, |_, window, cx| window.draw(cx).clear())
            .expect("draw");
        let image = self
            .cx
            .capture_screenshot(self.window)
            .expect("capture the window");
        Shot {
            width: image.width(),
            height: image.height(),
            rgba: image.as_raw().to_vec(),
        }
    }

    /// Print `text` as the program would, and have the terminal redraw.
    fn write(&mut self, text: &str) {
        let view = self.view.clone();
        self.cx.update(|cx| {
            view.read(cx).show_text(text);
            view.update(cx, |_, cx| cx.notify());
        });
    }

    fn appearance(&mut self, f: impl FnOnce(&mut Appearance)) {
        self.cx
            .update(|cx| cx.update_global::<Appearance, _>(|a, _| f(a)));
    }

    /// Where cell (`col`, `row`) was last painted, in points.
    fn cell(&self, col: usize, row: usize) -> gpui::Bounds<Pixels> {
        self.cx
            .read(|cx| self.view.read(cx).cell_bounds(col, row))
            .expect("painted")
    }

    fn read<R>(&self, f: impl FnOnce(&crate::terminal_view::TerminalView) -> R) -> R {
        self.cx.read(|cx| f(self.view.read(cx)))
    }
}

/// Write `shot` to `target/ui-shots/<name>-<slug>.png`.
fn save_shot(name: &str, slug: &str, shot: &Shot) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("ui-shots");
    std::fs::create_dir_all(&dir).expect("create target/ui-shots");
    let path = dir.join(format!("{name}-{slug}.png"));
    std::fs::write(&path, png(shot)).expect("write the PNG");
    println!("ui-shots: {}", path.display());
    path
}

/// Whether the pixel at (`x`, `y`) points is `c` (within 3 per channel).
fn is_colour(
    shot: &Shot,
    dims: Size<Pixels>,
    x: f32,
    y: f32,
    c: alacritty_terminal::vte::ansi::Rgb,
) -> bool {
    pixel_at(shot, dims, x, y)
        .iter()
        .zip([c.r, c.g, c.b])
        .all(|(a, b)| a.abs_diff(b) <= 3)
}

/// The terminal cursor (M11f), in both themes, one `cat` terminal per check, its cursor on
/// the blank cell (2, 1):
/// - `cursor-bar|underline|hollow|block`: each Style setting with no program style (glow off,
///   so the colour is exact): the bar is 2 px at the cell's left edge, the underline 2 px at
///   its bottom, the hollow a 1 px outline, the block a filled cell.
/// - `cursor-working`: a Working terminal (its status given by a `Workspace`) draws the cursor
///   in `ring_working`; back to Idle, the theme's cursor colour. A block in a status colour
///   writes its letter in the background colour.
/// - `cursor-glow`: the breathing glow around the bar, on a ~30 fps timer; after a program's
///   `ESC [ 2 q` (`cursor-steady-block`), a steady block with no glow and no frames.
/// - `cursor-unfocused`: an unfocused terminal (and a focused one in a window that isn't key):
///   a 1 px outline in the theme's cursor colour, whatever the status; no frames.
/// - 50 cursor moves before one frame leave one glide, from the old cell to the last.
/// - Sparks come from typed text only: not from output, arrows, ctrl-keys or Enter.
/// - `cursor-still`: with motion off, no glow, no glide (the bar is exactly on its new cell),
///   no sparks, and no frames.
fn cursor_scenes() {
    use crate::appearance::{CursorFun, CursorShapeSetting};
    use crate::cursor_look::Frames;

    let text = "\r\n$ ";
    for (slug, name) in SHOT_THEMES {
        let theme = shot_theme(name);
        let ui = theme.ui();
        let own = theme.cursor.unwrap_or(theme.foreground);
        let bg = theme.background;

        // Each shape, glow off so the colour is exact.
        for setting in CursorShapeSetting::ALL {
            let mut rig = CursorRig::new(theme, text, true, true, false, |a| {
                a.cursor_style = setting;
                a.cursor_glow = false;
            });
            let shot = rig.frame();
            let n = format!("cursor-{}-{slug}", setting.value());
            save_shot(&format!("cursor-{}", setting.value()), slug, &shot);
            let dims = rig.dims;
            let cell = rig.cell(2, 1);
            let scale = shot.width as f32 / f32::from(dims.width);
            let (left, right) = (f32::from(cell.left()), f32::from(cell.right()));
            let (top, bottom) = (f32::from(cell.top()), f32::from(cell.bottom()));
            let mid = (top + bottom) / 2.0;
            let across = |y: f32| count_along(&shot, dims, own, y, left - 4.0, right + 4.0);
            let two = (2.0 * scale).round() as usize;
            match setting {
                CursorShapeSetting::Bar => {
                    assert_eq!(across(mid), two, "{n}: the bar is 2 px wide");
                    assert!(
                        is_colour(&shot, dims, left + 0.5, mid, own),
                        "{n}: the bar is at the cell's left edge"
                    );
                    assert!(
                        is_colour(&shot, dims, left + 2.5, mid, bg),
                        "{n}: the bar stops after 2 px"
                    );
                }
                CursorShapeSetting::Underline => {
                    let wide = across(bottom - 1.0);
                    let cell_px = ((right - left) * scale).round() as usize;
                    assert!(
                        wide.abs_diff(cell_px) <= 2,
                        "{n}: the underline spans the cell ({wide} of {cell_px})"
                    );
                    assert_eq!(across(bottom - 3.0), 0, "{n}: the underline is 2 px tall");
                    assert_eq!(across(mid), 0, "{n}: nothing at the cell's middle");
                }
                CursorShapeSetting::Hollow => {
                    assert_eq!(across(mid), two, "{n}: a 1 px outline, left and right");
                    assert!(
                        is_colour(&shot, dims, (left + right) / 2.0, mid, bg),
                        "{n}: hollow inside"
                    );
                }
                CursorShapeSetting::Block => {
                    assert!(
                        is_colour(&shot, dims, (left + right) / 2.0, mid, own),
                        "{n}: the block fills the cell"
                    );
                }
            }
        }

        // The status colour, from a Workspace.
        {
            use std::cell::RefCell;
            use std::rc::Rc;
            let dims = size(px(640.0), px(400.0));
            let mut cx = gpui::VisualTestAppContext::new(current_platform(false));
            fonts::register_bundled_fonts(&cx.text_system().all_font_names());
            let slot: Slot = Rc::new(RefCell::new(None));
            let window = {
                let slot = slot.clone();
                cx.open_offscreen_window(dims, move |window, cx| {
                    let mut appearance = shot_appearance(theme, cx);
                    appearance.cursor_glow = false;
                    cx.set_global(appearance);
                    let ws = cx.new(|cx| {
                        let mut ws = Workspace::for_shots(window, cx);
                        let project = std::path::Path::new("/Users/me/code/api");
                        ws.open_shot_session("api", project, text, None, window, cx);
                        ws.select(0, window, cx);
                        ws.focus_active(window, cx);
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
            let ws = slot.borrow().clone().expect("the workspace");
            let view = cx.read(|cx| ws.read(cx).sessions.items()[0].view.clone());
            cx.read(|cx| view.read(cx).pretend_key_window());
            let frame = |cx: &mut VisualTestAppContext, status: Status| -> Shot {
                cx.update(|cx| ws.update(cx, |ws, cx| ws.set_status(0, status, false, cx)));
                cx.run_until_parked();
                cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
                    .expect("draw");
                let image = cx
                    .capture_screenshot(window.into())
                    .expect("capture the window");
                Shot {
                    width: image.width(),
                    height: image.height(),
                    rgba: image.as_raw().to_vec(),
                }
            };
            for (status, colour, label) in [
                (Status::Working, ui.ring_working, "working"),
                (Status::Idle, own, "idle"),
            ] {
                let shot = frame(&mut cx, status);
                if status == Status::Working {
                    save_shot("cursor-working", slug, &shot);
                }
                let cell = cx
                    .read(|cx| view.read(cx).cell_bounds(2, 1))
                    .expect("painted");
                let mid = f32::from(cell.top() + cell.bottom()) / 2.0;
                assert!(
                    is_colour(&shot, dims, f32::from(cell.left()) + 0.5, mid, colour),
                    "cursor-working-{slug}: a {label} terminal's bar is {colour:?}, saw {:?}",
                    pixel_at(&shot, dims, f32::from(cell.left()) + 0.5, mid)
                );
            }
        }

        // A block in a status colour: its letter in the background colour.
        {
            let mut rig = CursorRig::new(theme, "\r\n$ x\x1b[D", true, true, false, |a| {
                a.cursor_style = CursorShapeSetting::Block;
                a.cursor_glow = false;
            });
            let view = rig.view.clone();
            rig.cx
                .update(|cx| view.update(cx, |v, cx| v.set_status(Status::Working, cx)));
            let shot = rig.frame();
            let cell = rig.cell(2, 1);
            let (l, r) = (f32::from(cell.left()), f32::from(cell.right()));
            let (t, b) = (f32::from(cell.top()), f32::from(cell.bottom()));
            let n = format!("cursor-block-working-{slug}");
            let mut fill = 0;
            let mut letter = 0;
            let mut y = t + 0.5;
            while y < b {
                fill += count_along(&shot, rig.dims, ui.ring_working, y, l, r);
                letter += count_along(&shot, rig.dims, bg, y, l, r);
                y += 1.0;
            }
            assert!(fill > 0, "{n}: the block is ring_working");
            assert!(letter > 0, "{n}: the letter is the background colour");
        }

        // The glow, then a program's steady block.
        {
            let mut rig = CursorRig::new(theme, text, true, true, false, |_| ());
            let shot = rig.frame();
            save_shot("cursor-glow", slug, &shot);
            let cell = rig.cell(2, 1);
            let left = f32::from(cell.left());
            let mid = f32::from(cell.top() + cell.bottom()) / 2.0;
            let n = format!("cursor-glow-{slug}");
            assert!(
                !is_colour(&shot, rig.dims, left - 3.0, mid, bg),
                "{n}: a glow beside the bar"
            );
            assert_eq!(
                rig.read(|v| v.cursor_frames()),
                Frames::Breath,
                "{n}: breathing redraws on the ~30 fps timer"
            );

            rig.write("\x1b[2 q");
            let shot = rig.frame();
            save_shot("cursor-steady-block", slug, &shot);
            let n = format!("cursor-steady-block-{slug}");
            let centre = f32::from(cell.left() + cell.right()) / 2.0;
            assert!(is_colour(&shot, rig.dims, centre, mid, own), "{n}: a block");
            assert!(
                is_colour(&shot, rig.dims, left - 3.0, mid, bg),
                "{n}: no glow around a steady cursor"
            );
            assert_eq!(
                rig.read(|v| v.cursor_frames()),
                Frames::None,
                "{n}: no frames"
            );
        }

        // Unfocused, and focused in a window that isn't key: a still 1 px outline.
        for (focus, key, label) in [(false, true, "unfocused"), (true, false, "not key")] {
            let mut rig = CursorRig::new(theme, text, focus, key, false, |_| ());
            let view = rig.view.clone();
            rig.cx
                .update(|cx| view.update(cx, |v, cx| v.set_status(Status::Working, cx)));
            let shot = rig.frame();
            if !focus {
                save_shot("cursor-unfocused", slug, &shot);
            }
            let scale = shot.width as f32 / f32::from(rig.dims.width);
            let cell = rig.cell(2, 1);
            let (l, r) = (f32::from(cell.left()), f32::from(cell.right()));
            let mid = f32::from(cell.top() + cell.bottom()) / 2.0;
            let n = format!("cursor-{label}-{slug}");
            assert_eq!(
                count_along(&shot, rig.dims, own, mid, l - 4.0, r + 4.0),
                (2.0 * scale).round() as usize,
                "{n}: a 1 px outline in the theme's cursor colour"
            );
            assert!(
                is_colour(&shot, rig.dims, (l + r) / 2.0, mid, bg),
                "{n}: hollow"
            );
            assert_eq!(
                rig.read(|v| v.cursor_frames()),
                Frames::None,
                "{n}: no frames"
            );
        }

        // 50 moves in one frame: one glide, from the old cell to the last.
        {
            let mut rig = CursorRig::new(theme, text, true, true, false, |_| ());
            rig.frame();
            let mut moves = String::new();
            for i in 0..49 {
                moves.push_str(&format!("\x1b[{};{}H", 2 + i % 3, 3 + i % 7));
            }
            moves.push_str("\x1b[2;13H");
            rig.write(&moves);
            rig.frame();
            let n = format!("cursor-glide-{slug}");
            let glide = rig.read(|v| v.cursor_glide()).expect("a glide");
            assert_eq!(glide.from, (2.0, 1.0), "{n}: from the old cell");
            assert_eq!(glide.to, (12, 1), "{n}: to the last");
            assert_eq!(rig.read(|v| v.cursor_frames()), Frames::EveryFrame, "{n}");
            // Where the first frame draws the bar depends on when the platform drew it (a few
            // ms in, it is already a fraction of a pixel along): the unit tests pin that.
        }

        // Sparks: typed text only.
        {
            let mut rig = CursorRig::new(theme, text, true, true, false, |a| {
                a.cursor_fun = CursorFun::Sparks;
            });
            rig.frame();
            let n = format!("cursor-sparks-{slug}");
            let window = rig.window;
            rig.cx.simulate_keystrokes(window, "a->a");
            assert_eq!(
                rig.read(|v| v.spark_bursts()),
                1,
                "{n}: a typed letter sparks"
            );
            assert_eq!(rig.read(|v| v.live_sparks()), 6, "{n}: six of them");
            rig.write("output");
            rig.cx.simulate_keystrokes(window, "left ctrl-c enter");
            assert_eq!(
                rig.read(|v| v.spark_bursts()),
                1,
                "{n}: no sparks from output, arrows, ctrl-keys or Enter"
            );
        }

        // Motion off: still and solid.
        {
            let mut rig = CursorRig::new(theme, text, true, true, true, |a| {
                a.cursor_fun = CursorFun::Sparks;
            });
            let shot = rig.frame();
            let cell = rig.cell(2, 1);
            let mid = f32::from(cell.top() + cell.bottom()) / 2.0;
            let n = format!("cursor-still-{slug}");
            assert!(
                is_colour(&shot, rig.dims, f32::from(cell.left()) - 3.0, mid, bg),
                "{n}: no glow"
            );
            assert_eq!(
                rig.read(|v| v.cursor_frames()),
                Frames::None,
                "{n}: no frames"
            );
            rig.write("\x1b[2;13H");
            let shot = rig.frame();
            save_shot("cursor-still", slug, &shot);
            let scale = shot.width as f32 / f32::from(rig.dims.width);
            let new = rig.cell(12, 1);
            let (l, r) = (f32::from(new.left()), f32::from(new.right()));
            assert!(rig.read(|v| v.cursor_glide()).is_none(), "{n}: no glide");
            assert!(
                is_colour(&shot, rig.dims, l + 0.5, mid, own),
                "{n}: the bar is on its new cell at once"
            );
            assert_eq!(
                count_along(&shot, rig.dims, own, mid, l - 4.0, r),
                (2.0 * scale).round() as usize,
                "{n}: exactly on the cell's left edge"
            );
            assert_eq!(
                count_along(
                    &shot,
                    rig.dims,
                    own,
                    mid,
                    f32::from(cell.left()) - 1.0,
                    f32::from(cell.right())
                ),
                0,
                "{n}: nothing left at the old cell"
            );
            let window = rig.window;
            rig.cx.simulate_keystrokes(window, "a->a");
            assert_eq!(rig.read(|v| v.spark_bursts()), 0, "{n}: no sparks");
            assert_eq!(rig.read(|v| v.live_sparks()), 0, "{n}: no sparks");
            rig.appearance(|a| a.cursor_fun = CursorFun::Trail);
            rig.write("\x1b[2;3H");
            rig.frame();
            assert!(
                rig.read(|v| v.cursor_glide()).is_none(),
                "{n}: no glide, so no trail"
            );
        }
    }
}
