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
    ("browser-tab", browser_tab_scenes),
    ("switcher", switcher_scenes),
    ("overlays", overlays),
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
            "🖕",
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

/// The appearance for sidebar scenes: the built-in presets, for the "+ New" menu.
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
/// - `sidebar-menu`: the "+ New" menu open.
/// - `sidebar-drag`: a row dragged down its group (lifted copy, dimmed source, insertion line).
/// - `sidebar-scroll`: 24 sessions, the last one active, so the list scrolls to it.
///
/// Panics unless the question was read off the boxed prompt, a waiting row is 56 px and a
/// plain one 42 px (M11b: each has its folder line), the active row is scrolled into view and
/// rows scrolled away don't count as on screen.
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

        // The pointer over web's (closed) header; the "+ New" menu open; a row mid-drag.
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
            let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let ws = slotted(&slot);
                let none = Modifiers::none();
                match scene {
                    "sidebar-hover" => {
                        let at = row_point(cx, &ws, 6, 0.0);
                        cx.simulate_mouse_move(window, at, None, none);
                    }
                    "sidebar-menu" => {
                        let at = point(px(30.0), dims.height - px(18.0));
                        cx.simulate_click(window, at, none);
                        assert!(cx.read(|cx| ws.read(cx).sidebar.new_menu), "menu opens");
                    }
                    _ => {
                        // codex (row 2) down past tests, into the lower half of shell (row 4).
                        let from = row_point(cx, &ws, 2, 0.0);
                        let to = row_point(cx, &ws, 4, 6.0);
                        cx.simulate_mouse_down(window, from, MouseButton::Left, none);
                        let left = MouseButton::Left;
                        cx.simulate_mouse_move(window, from + point(px(0.0), px(8.0)), left, none);
                        cx.simulate_mouse_move(window, to, left, none);
                        let line = cx.read(|cx| ws.read(cx).sidebar.drop_at.map(|(_, b)| b));
                        assert_eq!(line, Some(true), "the line is below shell");
                    }
                }
            };
            let path = shoot_with(scene, slug, &theme, dims, build, act);
            println!("ui-shots: {}", path.display());
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
            // The vertical divider, 1 px; the accent 2 px, only when hovered or dragged.
            let (x0, x1) = (at(ratio) - 4.0, at(ratio) + 5.0);
            let scale = shot.width as f32 / 960.0;
            let accent = count_along(&shot, dims, ui.accent, 300.0, x0, x1);
            let plain = count_along(&shot, dims, ui.divider, 300.0, x0, x1);
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

/// One box over the window at a time: opening the rename box, the switcher or the address box
/// over another closes that one, and the new one keeps the keyboard (the old one's late "lost
/// focus" must not take it back). Rename -> switcher -> rename -> address box -> switcher, each
/// checked after frames have run the focus changes. Then every overlay (`Overlay::ALL`) is opened
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
        ] {
            cx.update_window(window, |_, window, cx| {
                ws.update(cx, |ws, cx| match step {
                    "rename" => ws.start_rename(ws.model.display_order()[3], window, cx),
                    "address" => ws.open_address_box(window, cx),
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
    };
    render_with(&theme, size(px(960.0), px(600.0)), build, act);
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
/// - `title-band`: the sidebar shown, the 🔔 and + right of the sidebar button, nothing unseen;
/// - `title-hover`: the same with the pointer on the 🔔 (its fill, its outline brighter);
/// - `title-unseen`: the same with notifications unseen (the 🔔's dot);
/// - `title-hidden`: the sidebar hidden: the band runs over the panes with the workspace's name
///   centred in it, the dot still there;
/// - `title-narrow`: hidden, a 560 px window and a long name, cut with an ellipsis;
/// - `title-popover`: the popover open over six notifications, newest first (agent's long
///   question cut, a closed terminal's entry greyed, a script's last with its own title);
/// - `title-popover-empty`: the popover with none yet.
///
/// Panics unless the dot shows exactly while some are unseen, opening the popover sees them
/// all (and it counts as an overlay), the name is drawn in the band only with the sidebar hidden
/// and never past its inset; and (Mocha) unless a press on the 🔔 doesn't arm a window drag
/// while one between the buttons does, a click on the 🔔 opens the popover and Escape or a
/// click outside closes it, a click on a closed terminal's entry (or a script's with no
/// terminal) does nothing, and a click on
/// agent's entry shows agent (its workspace and tab) with the keyboard.
fn title_area_scenes() {
    use crate::sidebar_toggle::HIT;
    use crate::tab_menu::Dismiss;
    use crate::title_area::{DOT, DOT_INSET, bell_origin, entry_top, name_inset, plus_origin};
    use gpui::{Focusable, KeyBinding, Modifiers, MouseButton};
    use std::cell::RefCell;
    use std::rc::Rc;

    const LONG_NAME: &str = "Refactor the auth middleware to use the new session store";
    let (bx, by) = bell_origin(false);
    let dot = (bx + HIT - DOT_INSET - DOT / 2.0, by + DOT_INSET + DOT / 2.0);
    for (slug, name) in SHOT_THEMES {
        let theme = shot_theme(name);
        let ui = theme.ui();
        let bg = |c: alacritty_terminal::vte::ansi::Rgb| [c.r, c.g, c.b];
        for scene in [
            "title-band",
            "title-hover",
            "title-unseen",
            "title-hidden",
            "title-narrow",
            "title-popover",
            "title-popover-empty",
        ] {
            let dims = match scene {
                "title-narrow" => size(px(560.0), px(200.0)),
                "title-popover" | "title-popover-empty" => size(px(960.0), px(480.0)),
                _ => size(px(760.0), px(200.0)),
            };
            let slot: Slot = Rc::new(RefCell::new(None));
            let keep = slot.clone();
            let build = move |window: &mut Window, cx: &mut App| -> AnyView {
                cx.set_global(sidebar_appearance(theme, cx));
                let ws = cx.new(|cx| {
                    let mut ws = switcher_workspace(window, cx);
                    ws.sidebar.hidden = matches!(scene, "title-hidden" | "title-narrow");
                    if scene != "title-popover-empty" {
                        shot_notifications(&mut ws);
                    }
                    if matches!(scene, "title-band" | "title-hover") {
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
                    let bell = point(px(bx + HIT / 2.0), px(by + HIT / 2.0));
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
            // The bell's rim (1.5 px from 9.5 px down its 14 px box, drawn on one whole pixel
            // row): dim, the full text colour on hover.
            let rim = at(bx + HIT / 2.0, by + (HIT - 14.0) / 2.0 + 9.75);
            match scene {
                "title-band" => {
                    assert_ne!(at(dot.0, dot.1), bg(ui.accent), "{n}: no dot");
                    assert_eq!(rim, bg(ui.text_dim), "{n}: the bell, dim");
                }
                "title-hover" => assert_eq!(rim, bg(ui.text), "{n}: the bell, hovered"),
                "title-unseen" | "title-hidden" => {
                    assert_eq!(at(dot.0, dot.1), bg(ui.accent), "{n}: the dot")
                }
                _ => {}
            }
            // The name: ink in the band's middle with the sidebar hidden, none past its inset.
            let width = f32::from(dims.width);
            let inset = name_inset(false);
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
            if matches!(scene, "title-hidden" | "title-narrow") {
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
            shot_notifications(&mut ws);
            ws
        });
        *keep.borrow_mut() = Some(ws.clone());
        ws.into()
    };
    let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
        let ws = slotted(&slot);
        let none = Modifiers::none();
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
        let bell = point(px(bx + HIT / 2.0), px(by + HIT / 2.0));
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
        // Between the buttons the band still drags the window (the + is pressed the same way as
        // the 🔔, by the same `band_button`).
        let gap = point(px(plus_origin(false).0 - 2.0), px(by + HIT / 2.0));
        cx.simulate_mouse_down(window, gap, MouseButton::Left, none);
        assert!(
            pressed(cx),
            "title-area: between the buttons the band drags"
        );
        cx.simulate_mouse_up(window, gap, MouseButton::Left, none);
        // A click outside closes it.
        cx.simulate_click(window, bell, none);
        cx.simulate_click(window, point(px(800.0), px(400.0)), none);
        assert!(!open(cx), "title-area: a click outside closes it");
        // The closed terminal's entry (the fourth) does nothing; agent's (the first) jumps.
        let entry = |i: usize| point(px(bx + 60.0), px(entry_top(i) + 20.0));
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
            size(px(960.0), px(600.0)),
            bx + 12.0,
            entry_top(0) + 2.0,
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
        slot.borrow_mut().take();
    };
    render_with(&theme, size(px(960.0), px(600.0)), build, act);
}
