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

use gpui::{
    AnyView, App, Context, IntoElement, Pixels, Render, Size, Window, div, prelude::*, px, size,
};
use gpui_platform::current_platform;

use crate::settings;
use crate::terminal_view::hsla;
use crate::theme::{self, Theme};

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
    let mut cx = gpui::VisualTestAppContext::new(current_platform(false));
    let theme = *theme;
    let window = cx
        .open_offscreen_window(size, move |window, cx| {
            let scene = build(window, cx);
            cx.new(|_| Root { theme, scene })
        })
        .expect("open an off-screen window");
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
    let shot = render(theme, size, build);
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

/// Every scene: (name, function). A UI task adds its scenes here.
const SCENES: &[(&str, fn())] = &[("smoke", smoke)];

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
