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
use crate::workspace::Workspace;
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
    ("splits", split_scenes),
    ("switcher", switcher_scenes),
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
    // A split's panes have a header (not terminal background) above their terminal's padding.
    let header = ui_tokens::size::PANE_HEADER;
    let top = if split { band + header } else { band };
    // The right pane's right and top padding (skipping its header), and (hidden) the band above.
    let in_header = |y: f32| split && y >= band && y < band + header;
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
///   script's progress, done and idle split together, a long title, a closed group) and the
///   first-run hint; `sidebar-narrow` and `sidebar-wide` at 180 and 400 px.
/// - `sidebar-hover`: the pointer over the closed group's header (its "+" shows).
/// - `sidebar-menu`: the "+ New" menu open.
/// - `sidebar-drag`: a row dragged down its group (lifted copy, dimmed source, insertion line).
/// - `sidebar-scroll`: 24 sessions, the last one active, so the list scrolls to it.
///
/// Panics unless the question was read off the boxed prompt, a waiting row is 42 px and a
/// plain one 28 px, the active row is scrolled into view and rows scrolled away don't count
/// as on screen.
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
                    // Rows: api header, its five sessions, web's header.
                    let height = |i: usize| {
                        let b = ws.sidebar.scroll.bounds_for_item(i).expect("laid out");
                        f32::from(b.size.height)
                    };
                    assert_eq!(height(1), 42.0, "{scene}-{slug}: waiting row");
                    assert_eq!(height(4), 28.0, "{scene}-{slug}: plain row");
                    assert_eq!(ws.sidebar.scroll.children_count(), 7, "{scene}-{slug}");
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
                    let mut ws = sidebar_workspace(&EIGHT, Some(3), 3, &[WEB], window, cx);
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
/// - `splits`: api focused (accent line under its header), claude waiting (tinted header),
///   codex working; dividers quiet.
/// - `splits-waiting`: the waiting pane focused (tint and accent together).
/// - `splits-hover`: the pointer over the vertical divider (2 px accent).
/// - `splits-drag`: that divider dragged left (accent, the panes follow).
///
/// Panics unless each shows 3 headers, their fills and the focus line are the right colours,
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
                let headers = cx.read(|cx| slotted(&acting).read(cx).header_sessions().len());
                assert_eq!(headers, 3, "{scene}-{slug}: a header per pane");
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
            // Header fills, away from the text: api (left), claude (top right).
            let (left_header, right_header) = ((400.0, 11.0), (900.0, 11.0));
            let is = |p: [u8; 3], c: alacritty_terminal::vte::ansi::Rgb| {
                p.iter()
                    .zip([c.r, c.g, c.b])
                    .all(|(a, b)| a.abs_diff(b) <= 3)
            };
            let px_at = |(x, y): (f32, f32)| pixel_at(&shot, dims, x, y);
            assert!(
                is(px_at(left_header), ui.sidebar_background),
                "{n}: plain header"
            );
            assert!(
                is(px_at(right_header), ui.waiting_tint),
                "{n}: waiting tint"
            );
            // The focus line: under the focused pane's header only.
            let (line_left, line_right) = (px_at((400.0, 21.0)), px_at((900.0, 21.0)));
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
            let ratio = if scene == "splits-drag" { 0.4 } else { 0.5 };
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
/// - `rename`: the rename box just opened on shell, its old name selected.
///
/// Panics unless the rows are in that order, the filter leaves three, and the rename box has
/// the old name.
fn switcher_scenes() {
    use std::cell::RefCell;
    use std::rc::Rc;

    let dims = size(px(960.0), px(600.0));
    for (slug, name) in SHOT_THEMES {
        let theme = shot_theme(name);
        for scene in ["switcher", "switcher-filter", "rename"] {
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
                cx.update_window(window, |_, window, cx| {
                    ws.update(cx, |ws, cx| match scene {
                        "rename" => ws.start_rename(3, window, cx),
                        _ => ws.open_switcher(window, cx),
                    })
                })
                .expect("open it");
                cx.run_until_parked();
                if scene == "switcher-filter" {
                    cx.simulate_input(window, "build");
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
                        assert_eq!(input.read(cx).text(), "shell", "{scene}-{slug}");
                    }),
                }
            };
            let path = shoot_with(scene, slug, &theme, dims, build, act);
            println!("ui-shots: {}", path.display());
        }
    }
}
