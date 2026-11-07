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
    ("api-background", api_background),
    ("banner-click", banner_click),
    ("default-name", default_name),
    ("switcher", switcher_scenes),
    ("overlays", overlays),
    ("help", help_scenes),
    ("panels", panel_scenes),
    ("preferences", preferences_scenes),
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
            size(px(960.0), px(600.0)),
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
        ];
        let chips = tokens.map(|(name, color)| {
            // The pair closest to its floor, if this token is drawn on something.
            let worst = pairs
                .iter()
                .filter(|p| p.fg_name == name)
                .map(|p| (contrast(p.fg, p.bg), p))
                .min_by(|a, b| (a.0 / a.1.min).total_cmp(&(b.0 / b.1.min)));
            let detail = match worst {
                Some((ratio, p)) => {
                    format!("{} {ratio:.2} on {} ≥{}", hex(color), p.bg_name, p.min)
                }
                None => format!("{} surface", hex(color)),
            };
            div()
                .w(px(224.0))
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
/// right edge is all terminal background.
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
                        // area, less half the divider): split right is the third of four.
                        let right = tabs[0].left() + px(519.5);
                        let at = point(right - px(4.0 + 20.0 + 2.0 + 10.0), px(13.0));
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
                        f32::from(last.right()) <= 720.0 - 94.0 + 1.0,
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
            let split = point(right - px(4.0 + 20.0 + 2.0 + 10.0), px(13.0));
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
