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
use crate::procs::ProgramState;
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
    ("sidebar-selection", sidebar_selection_scenes),
    ("sidebar-button", sidebar_button_scenes),
    ("title-area", title_area_scenes),
    ("title-bar", title_bar_scenes),
    ("tabs", tab_scenes),
    ("splits", split_scenes),
    ("cards", card_scenes),
    ("cards-split", card_split_scenes),
    ("cards-corners", card_corner_scenes),
    ("workspaces", workspace_scenes),
    ("tab-bars", tab_bar_scenes),
    ("tab-drag", tab_drag_scenes),
    ("browser-tab", browser_tab_scenes),
    ("file-tab", file_tab_scenes),
    ("file-diff", file_diff_scenes),
    ("right-sidebar", right_sidebar_scenes),
    ("files", files_scenes),
    ("right-panels", right_panel_scenes),
    ("changes", changes_scenes),
    ("activity", activity_scenes),
    ("api-background", api_background),
    ("banner-click", banner_click),
    ("default-name", default_name),
    ("names", names),
    ("renames", renames),
    ("rename-box", rename_box_scenes),
    ("palette", palette_scene),
    ("palette-workspaces", palette_workspaces_scene),
    ("palette-actions", palette_actions_scene),
    ("palette-keys", palette_keys_scene),
    ("palette-run", palette_run_scene),
    ("palette-refresh", palette_refresh_scene),
    ("palette-recent", palette_recent_scene),
    ("palette-files", palette_files_scene),
    ("palette-relist", palette_relist_scene),
    ("palette-settings", palette_settings_scene),
    ("palette-narrow", palette_narrow_scene),
    ("palette-motion", palette_motion_scene),
    ("welcome", welcome_scene),
    ("welcome-checking", welcome_checking_scene),
    ("welcome-keys", welcome_keys_scene),
    ("welcome-narrow", welcome_narrow_scene),
    ("welcome-start", welcome_start_scene),
    ("welcome-folder", welcome_folder_scene),
    ("welcome-first-run", welcome_first_run_scene),
    ("welcome-reopen", welcome_reopen_scene),
    ("overlays", overlays),
    ("help", help_scenes),
    ("panels", panel_scenes),
    ("preferences", preferences_scenes),
    ("status-bar", status_bar_scenes),
    ("status-bar-ports", status_bar_ports_scenes),
    ("status-bar-narrow", status_bar_narrow_scenes),
    ("update-button", update_button_scenes),
    ("cursor", cursor_scenes),
    ("cursor-flood", cursor_flood_scene),
    ("command-blocks", command_blocks_scene),
    ("command-blocks-alt-screen", command_blocks_alt_screen_scene),
    ("command-blocks-narrow", command_blocks_narrow_scene),
    ("command-blocks-rprompt", command_blocks_rprompt_scene),
    ("command-blocks-small-font", command_blocks_small_font_scene),
    ("command-blocks-off", command_blocks_off_scene),
    ("command-blocks-jump", command_blocks_jump_scene),
    ("command-blocks-copy", command_blocks_copy_scene),
    ("command-blocks-rerun", command_blocks_rerun_scene),
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
        command_blocks: true,
    }
}

/// What a shell shows in the scenes, the same every time.
const SHELL_TEXT: &str = "~/code/api $ cargo test\r\n   Compiling api v0.4.2 (/Users/me/code/api)\r\n    Finished `test` profile [unoptimized + debuginfo] target(s) in 2.31s\r\n     Running unittests src/main.rs\r\n\r\nrunning 42 tests\r\ntest result: ok. 42 passed; 0 failed; 0 ignored\r\n\r\n~/code/api $ ";

const AGENT_TEXT: &str =
    "~/code/api $ claude\r\n\r\n  Reading src/auth.rs\r\n  Editing src/auth.rs (+12 -3)\r\n\r\n> ";

/// The whole window, as the app draws it, from a real `Workspace` with plain sessions (each a
/// fixed screen over a silent `cat`, so the shots don't depend on your shell): one session; the
/// same split in two; and one session with the sidebar hidden. Checks that the terminal's
/// padding is drawn in the terminal background (nothing painted in it) inside its card, and
/// the top band and the gaps in the canvas (M11i). Off-screen windows have no traffic lights,
/// so the band is empty in the shots.
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
                // Idle (M11i): server runs a program (a filled dot), shell is at its prompt (a
                // ring).
                let id = |i: usize| ws.sessions.items()[i].id;
                let (shell, server) = (id(3), id(5));
                ws.programs
                    .set_for_shots(server, ProgramState::Running("npm run dev".into()));
                ws.programs.set_for_shots(shell, ProgramState::Prompt);
                // The hint closed, so every row (and both idle marks) shows.
                ws.sidebar.hint_dismissed = true;
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

/// What the waiting session shows: a Claude Code style prompt, drawn in a box 40 columns wide so
/// it fits a split pane's grid (about 42 columns, M11i's padding) without wrapping mid-word.
const CLAUDE_PROMPT: &str = "  Editing src/auth.rs (+12 -3)\r\n\r\n╭──────────────────────────────────────╮\r\n│ Edit file                            │\r\n│ src/auth.rs                          │\r\n│                                      │\r\n│ Do you want to edit auth.rs?         │\r\n│ ❯ 1. Yes                             │\r\n│   2. Yes, don't ask again            │\r\n│   3. No, tell Claude what to do      │\r\n╰──────────────────────────────────────╯\r\n";

/// The question the sidebar must find in `CLAUDE_PROMPT`.
const CLAUDE_QUESTION: &str = "Do you want to edit auth.rs?";

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
/// Panics unless the question was read off the boxed prompt, a waiting row and a plain one are
/// both 32 pt (M11i: two lines), the active row is scrolled into view and
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
                    // Every row has its folder line, two lines in all (M11i); the waiting one
                    // says "Needs input" there instead.
                    assert_eq!(height(1), 32.0, "{scene}-{slug}: waiting row");
                    assert_eq!(height(3), 32.0, "{scene}-{slug}: the split workspace's row");
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
                        // The footer's + (an icon now, no "New" label), at its left, over the
                        // status bar (M11i).
                        let bar = ui_tokens::size::STATUS_BAR;
                        let at = point(px(18.0), dims.height - px(bar + 18.0));
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
            // shell (selected: the selection's tint) slid up by codex's room; the gap below it,
            // down to where shell ended, is filled by codex's copy (as tall as codex's row), top
            // to bottom: no empty band, no line.
            let up = px_at(shell.top() - px(room) + px(3.0));
            assert!(is(up, ui.row_selected), "{n}: shell slid up: {up:?}");
            let gap_top = shell.bottom() - codex.size.height;
            for y in [
                gap_top + px(3.0),
                shell.center().y,
                shell.bottom() - px(3.0),
            ] {
                let p = px_at(y);
                assert!(
                    !is(p, ui.sidebar_background) && !is(p, ui.row_selected),
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

/// The sidebar rows (M11b; compact in M11i), from real workspaces in both themes, at 240 pt
/// (`sidebar-rows`) and 180 pt (`sidebar-rows-narrow`): deploy and claude pinned (📌, above
/// every group header), claude waiting with "Needs input · question" under its name ("Needs
/// input" alone at 180 pt); notes in no group, a gap under the pinned rows; each row's
/// `branch • ~/folder`; payments selected (the selection's tint and the accent bar, its text in
/// `text`), its long branch cut first, then its long folder at its start (keeping `payments`);
/// the long name cut; notes in /tmp (no `~`, not in a repo: no branch).
///
/// Panics unless the pinned rows lead, the rows carry their branch and folder, every row is
/// 32 pt (two lines), and the selected row is tinted with the accent bar at its left and its
/// name in `text`.
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
                    // Every row has a second line: 32 pt (M11i; the waiting one was 56).
                    for i in [0, 1, 2, 4, 6, 8] {
                        let h = f32::from(bounds(i).size.height);
                        assert_eq!(h, 32.0, "{n}: row {i} is two lines");
                    }
                    let b = ws.sidebar.scroll.bounds_for_item(active).expect("laid out");
                    found.set(Some((
                        f32::from(b.left()),
                        f32::from(b.right()),
                        f32::from(b.top()),
                        f32::from(b.size.height),
                    )));
                });
                // Not kept past the window: a handle left over is reported as a leak.
                slot.borrow_mut().take();
            };
            let path = shoot_with(scene, slug, &theme, dims, build.clone(), act.clone());
            println!("ui-shots: {}", path.display());
            let shot = render_with(&theme, dims, build, act);
            let (left, right, top, height) = selected.get().expect("the selected row's bounds");
            let n = format!("{scene}-{slug}");
            check_selected(&shot, dims, &ui, &n, (left, right, top, height));
        }
    }
}

/// A sidebar row as laid out: (left, right, top, height), in points.
type RowBox = (f32, f32, f32, f32);

/// A sidebar row's line 1 (17 pt) and line 2 (13 pt), centred in the row (M11i).
const ROW_LINE_ONE: f32 = 17.0;
const ROW_LINE_TWO: f32 = 13.0;

/// The middle of line 1 and of line 2 of a sidebar row `height` tall whose top is at `top`.
fn row_lines(top: f32, height: f32) -> (f32, f32) {
    let two = height > crate::ui_tokens::size::SIDEBAR_ROW;
    let content = ROW_LINE_ONE + if two { ROW_LINE_TWO } else { 0.0 };
    let first = top + (height - content) / 2.0;
    (
        first + ROW_LINE_ONE / 2.0,
        first + ROW_LINE_ONE + ROW_LINE_TWO / 2.0,
    )
}

/// How far pixel `p` is from colour `c`: its largest difference in any channel.
fn gap(p: [u8; 3], c: alacritty_terminal::vte::ansi::Rgb) -> u8 {
    p.iter()
        .zip([c.r, c.g, c.b])
        .map(|(a, b)| a.abs_diff(b))
        .max()
        .unwrap_or(0)
}

/// Whether pixel `p` is colour `c` (give or take 3 per channel).
fn near(p: [u8; 3], c: alacritty_terminal::vte::ansi::Rgb) -> bool {
    gap(p, c) <= 3
}

/// The selected row (M11i) at `(left, right, top, height)`: the 2 pt accent bar at its left
/// edge, the selection's tint inside, and its name in `text`, never `on_accent`.
fn check_selected(
    shot: &Shot,
    dims: Size<Pixels>,
    ui: &UiColors,
    n: &str,
    (left, right, top, height): RowBox,
) {
    let middle = top + height / 2.0;
    let bar = pixel_at(shot, dims, left + 1.0, middle);
    assert!(near(bar, ui.accent), "{n}: the accent bar: {bar:?}");
    // Just right of the bar, in the padding left of the mark, and in the row's middle.
    for (what, x) in [("padding", left + 5.0), ("middle", (left + right) / 2.0)] {
        let tint = pixel_at(shot, dims, x, middle);
        assert!(
            near(tint, ui.row_selected),
            "{n}: tint at its {what}: {tint:?}"
        );
    }
    let (line_one, _) = row_lines(top, height);
    let name = count_along(shot, dims, ui.text, line_one, left, right);
    assert!(name > 0, "{n}: the selected row's name in text");
    if !near([ui.text.r, ui.text.g, ui.text.b], ui.on_accent) {
        let ink = count_along(shot, dims, ui.on_accent, line_one, left, right);
        assert_eq!(ink, 0, "{n}: nothing on the selected row in on_accent");
    }
}

/// The compact rows' selection and marks (M11i), from real workspaces in both themes, all in
/// api: `api` selected, `server` idle running a program, `shell` idle at its prompt, `claude`
/// waiting, `docs` idle and hovered.
/// - `sidebar-selection`: api selected (the tint and the accent bar), the pointer over docs.
/// - `sidebar-selection-waiting`: claude selected instead: it takes the selection's tint and
///   keeps its waiting mark and "Needs input".
///
/// Panics unless the selected row has the accent bar at its left edge, the selection's tint
/// inside and its name in `text` (not `on_accent`); a waiting selected row keeps its waiting
/// colour; the hovered row shows `row_hover` and its ⌘ shortcut while the others hide theirs;
/// and the idle marks are a filled `text_dim` dot (server) and a hollow `text_faint` ring
/// (shell).
fn sidebar_selection_scenes() {
    use gpui::Modifiers;
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    const SESSIONS: [ShotSession; 5] = [
        shot("api", API, Status::Idle, 0, SHELL_TEXT),
        shot("server", API, Status::Idle, 0, SHELL_TEXT),
        shot("shell", API, Status::Idle, 0, SHELL_TEXT),
        shot("claude", API, Status::Waiting, 4 * 60, CLAUDE_PROMPT),
        shot("docs", API, Status::Idle, 0, SHELL_TEXT),
    ];
    let dims = size(px(540.0), px(360.0));
    for (slug, name) in SHOT_THEMES {
        let theme = shot_theme(name);
        let ui = theme.ui();
        for (scene, active) in [("sidebar-selection", 0), ("sidebar-selection-waiting", 3)] {
            let slot: Slot = Rc::new(RefCell::new(None));
            let keep = slot.clone();
            let build = move |window: &mut Window, cx: &mut App| -> AnyView {
                cx.set_global(sidebar_appearance(theme, cx));
                let ws = cx.new(|cx| {
                    let mut ws = sidebar_workspace(&SESSIONS, None, active, &[], window, cx);
                    ws.sidebar.hint_dismissed = true;
                    let id = |i: usize| ws.sessions.items()[i].id;
                    let (api, server, shell, docs) = (id(0), id(1), id(2), id(4));
                    let running = ProgramState::Running("npm run dev".into());
                    ws.programs.set_for_shots(server, running);
                    for prompt in [api, shell, docs] {
                        ws.programs.set_for_shots(prompt, ProgramState::Prompt);
                    }
                    ws
                });
                *keep.borrow_mut() = Some(ws.clone());
                ws.into()
            };
            // Each row's (left, right, top, height): the header is item 0, then api..docs.
            let rows: Rc<Cell<[RowBox; 5]>> = Rc::default();
            let seen = rows.clone();
            let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let ws = slotted(&slot);
                let mut all = [(0.0, 0.0, 0.0, 0.0); 5];
                cx.read(|cx| {
                    let scroll = &ws.read(cx).sidebar.scroll;
                    for (i, row) in all.iter_mut().enumerate() {
                        let b = scroll.bounds_for_item(i + 1).expect("laid out");
                        *row = (
                            f32::from(b.left()),
                            f32::from(b.right()),
                            f32::from(b.top()),
                            f32::from(b.size.height),
                        );
                    }
                });
                seen.set(all);
                // The pointer over docs.
                let at = row_point(cx, &ws, 5, 0.0);
                cx.simulate_mouse_move(window, at, None, Modifiers::none());
                slot.borrow_mut().take();
            };
            let path = shoot_with(scene, slug, &theme, dims, build.clone(), act.clone());
            println!("ui-shots: {}", path.display());
            let shot = render_with(&theme, dims, build, act);
            let n = format!("{scene}-{slug}");
            let rows = rows.get();
            check_selected(&shot, dims, &ui, &n, rows[active]);
            // The mark slot: 8 pt in from the row's left, 14 pt wide.
            let mark_at = |(left, _, top, height): RowBox| {
                (left + 8.0, left + 22.0, row_lines(top, height).0)
            };
            if active == 3 {
                // Selected and waiting: its mark and "Needs input" keep the waiting colour.
                let (x0, x1, y) = mark_at(rows[3]);
                let mark = count_along(&shot, dims, ui.ring_waiting, y, x0, x1);
                assert!(mark > 0, "{n}: the waiting mark on the selection");
                let (left, right, top, height) = rows[3];
                let two = row_lines(top, height).1;
                let words = count_along(&shot, dims, ui.ring_waiting, two, left, right);
                assert!(words > 0, "{n}: Needs input on the selection");
            }
            // Hovered docs: row_hover and its shortcut (⌘5) at the right; shell, not hovered,
            // shows no shortcut there.
            // (Small text at 1x is all smoothing: "something drawn" is a pixel well off the row.)
            let right_end = |(_, right, top, height): RowBox, surface| {
                let y = row_lines(top, height).0;
                (right as usize - 40..right as usize - 4)
                    .filter(|&x| gap(pixel_at(&shot, dims, x as f32, y), surface) > 48)
                    .count()
            };
            let (left, _, top, height) = rows[4];
            let hover = pixel_at(&shot, dims, left + 4.0, top + height / 2.0);
            assert!(near(hover, ui.row_hover), "{n}: docs hovered: {hover:?}");
            let shows = right_end(rows[4], ui.row_hover);
            assert!(shows > 0, "{n}: docs shows its shortcut");
            let hides = right_end(rows[2], ui.sidebar_background);
            assert_eq!(hides, 0, "{n}: shell hides its shortcut");
            // Idle marks: server's program runs (a filled text_dim dot); shell is at its prompt
            // (a hollow text_faint ring: its middle is the sidebar, its sides the ring).
            let (x0, x1, y) = mark_at(rows[1]);
            let dot = pixel_at(&shot, dims, (x0 + x1) / 2.0, y);
            assert!(near(dot, ui.text_dim), "{n}: server's dot: {dot:?}");
            let (x0, x1, y) = mark_at(rows[2]);
            let hole = pixel_at(&shot, dims, (x0 + x1) / 2.0, y);
            assert!(
                near(hole, ui.sidebar_background),
                "{n}: shell's ring is hollow: {hole:?}"
            );
            // Its line is 1.5 pt, so at 1x no pixel is wholly covered: the strongest one along
            // its middle is text_faint give or take its edge's smoothing, and not text_dim.
            let along: Vec<[u8; 3]> = (0..(x1 - x0) as usize)
                .map(|dx| pixel_at(&shot, dims, x0 + dx as f32, y))
                .collect();
            let strongest = along
                .iter()
                .copied()
                .max_by_key(|&p| gap(p, ui.sidebar_background))
                .expect("pixels along the mark");
            let (faint, dim) = (gap(strongest, ui.text_faint), gap(strongest, ui.text_dim));
            assert!(
                faint <= 12 && faint < dim,
                "{n}: shell's ring in text_faint ({:?}), not text_dim: {strongest:?}",
                ui.text_faint
            );
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

/// The box, `(x0, x1, y0, y1)` in points, round every device pixel inside `area` (`x`, `y`,
/// `w`, `h` in points) that is drawn in ink: at least half-way from `bg` to `ink` (its squared
/// distance from `bg`, times 4, at least `ink`'s); `None` when there is none. Every device pixel
/// is read, so a box edge is the same at 1x and 2x (a point-by-point read rounds each side its
/// own way, by up to a pixel).
fn ink_box(
    shot: &Shot,
    dims: Size<Pixels>,
    area: (f32, f32, f32, f32),
    bg: [u8; 3],
    ink: [u8; 3],
) -> Option<(f32, f32, f32, f32)> {
    let dist = |p: &[u8], c: [u8; 3]| -> i32 {
        (0..3)
            .map(|k| (i32::from(p[k]) - i32::from(c[k])).pow(2))
            .sum()
    };
    let full = dist(&ink, bg);
    let scale = shot.width as f32 / f32::from(dims.width);
    let step = 1.0 / scale;
    let (x, y, w, h) = area;
    let (left, top) = ((x * scale) as usize, (y * scale) as usize);
    let (cols, rows) = ((w * scale) as usize, (h * scale) as usize);
    let mut found: Option<(f32, f32, f32, f32)> = None;
    for py in top..(top + rows).min(shot.height as usize) {
        for px_ in left..(left + cols).min(shot.width as usize) {
            let i = (py * shot.width as usize + px_) * 4;
            if dist(&shot.rgba[i..i + 3], bg) * 4 >= full {
                let (x, y) = (px_ as f32 / scale, py as f32 / scale);
                found = Some(match found {
                    None => (x, x + step, y, y + step),
                    Some((x0, x1, y0, y1)) => {
                        (x0.min(x), x1.max(x + step), y0.min(y), y1.max(y + step))
                    }
                });
            }
        }
    }
    found
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

/// A 3-pane split in both themes, from a real `Workspace` (M11i: each pane a card, 6 pt gaps;
/// the cards span 246–954 and 6–594):
/// - `splits`: api focused (its card ringed), claude waiting, codex working; gaps quiet.
/// - `splits-waiting`: the waiting pane focused.
/// - `splits-hover`: the pointer over the vertical gap (2 px accent in its middle).
/// - `splits-drag`: that gap dragged left (accent, the panes follow).
///
/// Panics unless each shows 3 tabs, each pane's active tab is the card's colour with no line on
/// top (M11i: the focus ring shows the focused pane, `cards-split`), and the gap shows the
/// canvas except when hovered or dragged.
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
            // The middle of the vertical gap: the sides give it up in proportion.
            let at = |ratio: f32| 246.0 + 708.0 * ratio + GAP * (0.5 - ratio);
            let acting = slot.clone();
            let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let tabs = cx.read(|cx| slotted(&acting).read(cx).tab_sessions().len());
                assert_eq!(tabs, 3, "{scene}-{slug}: a tab per pane");
                let none = Modifiers::none();
                let left = MouseButton::Left;
                let on = point(px(at(0.5)), px(300.0));
                match scene {
                    "splits-hover" => cx.simulate_mouse_move(window, on, None, none),
                    "splits-drag" => {
                        // To 0.4 of the pane area (246 + 0.4 × 708).
                        let to = point(px(246.0 + 0.4 * 708.0), px(300.0));
                        cx.simulate_mouse_down(window, on, left, none);
                        cx.simulate_mouse_move(window, on - point(px(40.0), px(0.0)), left, none);
                        cx.simulate_mouse_move(window, to, left, none);
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
            // active (and only) tab, 160 px into it (it is 200 wide; × sits at its end), in
            // the bars that start inside the cards' 1 pt edges, 7 pt down; the tabs 4 pt
            // further.
            let ratio = if scene == "splits-drag" { 0.4 } else { 0.5 };
            let right_tab = at(ratio) + 160.0;
            let (left_header, right_header) = ((400.0, 30.0), (right_tab, 30.0));
            let is = |p: [u8; 3], c: alacritty_terminal::vte::ansi::Rgb| {
                p.iter()
                    .zip([c.r, c.g, c.b])
                    .all(|(a, b)| a.abs_diff(b) <= 3)
            };
            let px_at = |(x, y): (f32, f32)| pixel_at(&shot, dims, x, y);
            assert!(is(px_at(left_header), ui.card), "{n}: active tab");
            assert!(
                is(px_at(right_header), ui.card),
                "{n}: active waiting tab, the card's colour too"
            );
            // No line along the top of either tab (at 12, 1 pt into the tabs at 11).
            for (which, x) in [("left", 400.0), ("right", right_tab)] {
                let p = px_at((x, 12.0));
                assert!(is(p, ui.card), "{n}: no line on the {which} tab: {p:?}");
            }
            // The vertical gap, 6 pt of canvas; the 2 px accent in its middle only when hovered
            // or dragged. Measured across at y = 450, in the lower panes: not at 300, where the
            // right column's own gap (claude over codex) lies along the row. Only the gap: the
            // focused card's edge beside it is the accent too.
            // (Half a point in from each side: at 0.4 the gap starts mid-pixel.)
            let (x0, x1) = (at(ratio) - GAP / 2.0 + 0.5, at(ratio) + GAP / 2.0 - 0.5);
            let scale = shot.width as f32 / 960.0;
            let accent = count_along(&shot, dims, ui.accent, 450.0, x0, x1);
            if matches!(scene, "splits-hover" | "splits-drag") {
                assert_eq!(accent, (2.0 * scale) as usize, "{n}: 2 px accent line");
            } else {
                assert_eq!(accent, 0, "{n}: no accent in a quiet gap");
                let p = px_at((at(ratio), 450.0));
                assert!(is_canvas(p, &ui), "{n}: the gap is the canvas: {p:?}");
            }
            println!("ui-shots: {}", path.display());
        }
    }
}

/// How far a gap's pixel may be from the canvas: the cards' shadows (`card_shadow_alpha`,
/// blurred 6 pt) reach into the 6 pt gaps and darken them a little (M11i).
const SHADOW_SLACK: u8 = 12;

/// Whether pixel `p` shows the canvas (`sidebar_background`) between cards: within the
/// shadow's reach of it, and nearer it than the card's fill.
fn is_canvas(p: [u8; 3], ui: &UiColors) -> bool {
    let canvas = gap(p, ui.sidebar_background);
    canvas <= SHADOW_SLACK && canvas < gap(p, ui.card)
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

/// The 6 pt gap (M11i) all round the left sidebar's neighbours.
const GAP: f32 = ui_tokens::size::CARD_GAP;

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
    for (slug, name) in SHOT_THEMES {
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
    for (slug, name) in SHOT_THEMES {
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
                let accent = count_along(&shot, dims, ui.accent, 450.0, first_right, second_left);
                assert_eq!(accent, (2.0 * scale) as usize, "{n}: the 2 pt accent line");
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
    for (slug, name) in SHOT_THEMES {
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
                // Each with one line under its name (M11i): its folder, or the waiting one's
                // "Needs input".
                assert_eq!(height(2), 32.0, "{n}: agent's waiting row");
                assert_eq!(height(4), 32.0, "{n}: api's row");
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

/// A bar's buttons (`tab_bar`): each 20 pt, 2 pt apart, 4 pt in from the ends; three of them
/// (M11i: +, ⋯, the right sidebar's).
const BAR_BUTTONS: f32 = 4.0 + 3.0 * 20.0 + 2.0 * 2.0 + 4.0;
/// How far left of its pane's right edge the + button's centre is: past the right sidebar's
/// and ⋯ (each a button and a gap), then half a button.
const PLUS_FROM_EDGE: f32 = 4.0 + 2.0 * (20.0 + 2.0) + 10.0;
/// The same for ⋯ (past the right sidebar's) and for the right sidebar's button, the last.
const MORE_FROM_EDGE: f32 = 4.0 + 20.0 + 2.0 + 10.0;
const RIGHT_SIDEBAR_FROM_EDGE: f32 = 4.0 + 10.0;
/// A bar's buttons sit in its middle: 5 pt from its top (30 pt bar, 20 pt buttons).
const BUTTON_TOP: f32 = (ui_tokens::size::TAB_BAR - 20.0) / 2.0;
/// The left pane's bar in the 1280 pt two-pane scenes (M11i): half the 1028 pt card area, less
/// half the 6 pt gap and the card's 1 pt edge on either side.
const LEFT_BAR_WIDTH: f32 = 1028.0 / 2.0 - 3.0 - 2.0;

/// Pane tab bars (M11b Task 3; M11i's cleaner bar), from real workspaces in both themes:
/// - `tab-bars`: two panes; the left (focused) one with api (active: the card's colour, merging
///   into the terminal below it, no line on top), claude (waiting, tinted), codex (working) and
///   a browser tab ("New tab", globe); the right one with tests (done; active, the card's colour
///   too). The 30 pt bars hold their tabs 4 pt under their top; the tabs shrink below 200 pt.
/// - `tab-bars-hover`: the pointer over codex (`tab_hover`, × shown); `tab-bars-button`: over
///   the left bar's + (the first of its three buttons).
/// - `tab-bars-more`: the left bar's ⋯ clicked: its menu (New Terminal Tab ⌘T, New Browser Tab,
///   Split Right ⌘D, Split Down ⇧⌘D) under it, right edges together, the ⋯ lit.
/// - `tab-bars-menu`: claude's right-click menu (Close Tab, Close Other Tabs in Pane, Rename
///   Tab…); `tab-bars-rename`: codex double-clicked (shown, its rename box open).
/// - `tab-bars-single`: one pane, one tab (the bar still shows).
/// - `tab-bars-narrow`: a narrow window, one pane of seven tabs, the last shown: the tabs stop at
///   90 px and the row scrolls to the shown one.
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
                // Four tabs in the left bar, equally shrunk, between the bounds; 4 pt under the
                // 30 pt bar's top (inside the card's 1 pt edge, under the 6 pt strip), down to
                // its bottom.
                for b in &tabs {
                    let w = f32::from(b.size.width);
                    assert!((90.0..200.0).contains(&w), "{n}: tab width {w}");
                    let first = f32::from(tabs[0].size.width);
                    assert!((w - first).abs() <= 1.0, "{n}: equal widths ({w}, {first})");
                    assert_eq!(f32::from(b.top()), GAP + 1.0 + 4.0, "{n}: tabs 4 pt down");
                    assert_eq!(
                        f32::from(b.size.height),
                        26.0,
                        "{n}: to a 30 pt bar's bottom"
                    );
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
                        // The left pane's buttons sit at its right edge: + is the first of
                        // three.
                        let right = tabs[0].left() + px(LEFT_BAR_WIDTH);
                        let at = point(right - px(PLUS_FROM_EDGE), tabs[0].center().y);
                        cx.simulate_mouse_move(window, at, None, none);
                    }
                    "tab-bars-more" => {
                        // The left bar's ⋯: its menu hangs from the button's bottom right.
                        let right = tabs[0].left() + px(LEFT_BAR_WIDTH);
                        let at = point(right - px(MORE_FROM_EDGE), tabs[0].center().y);
                        cx.simulate_click(window, at, none);
                        let menu = cx.read(|cx| {
                            let ws = ws.read(cx);
                            ws.pane_actions.as_ref().map(|m| (m.pane, m.at))
                        });
                        let bar_top = tabs[0].top() - px(4.0);
                        let corner = point(
                            right - px(MORE_FROM_EDGE - 10.0),
                            bar_top + px(BUTTON_TOP + 20.0),
                        );
                        assert_eq!(menu, Some((panes[0], corner)), "{n}: under the left ⋯");
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
            let (top, bottom) = (f32::from(api.top()), f32::from(api.bottom()));
            let (bar_top, bar_right) = (top - 4.0, f32::from(api.left()) + LEFT_BAR_WIDTH);
            // Away from the text, the icon and the ×: near each tab's right end, low in the bar.
            let fill_at = |b: gpui::Bounds<Pixels>| (f32::from(b.right()) - 26.0, bottom - 6.0);
            // Whether something is drawn in tab `b`'s × box (on a tab filled `fill`).
            let close_drawn = |b: gpui::Bounds<Pixels>, fill| {
                let (right, mid) = (f32::from(b.right()), f32::from(b.center().y));
                (6..=18).any(|dx| {
                    (-4..=4).any(|dy| gap(px_at(right - dx as f32, mid + dy as f32), fill) > 30)
                })
            };
            // The corner of a bar button whose centre is `from_edge` left of the bar's right.
            let button_corner =
                |from_edge: f32| px_at(bar_right - from_edge - 8.0, bar_top + BUTTON_TOP + 2.0);
            match scene {
                "tab-bars" => {
                    // api: the card's colour, with no line on top, merging into the terminal
                    // under it: 1 pt above the bar's bottom is the terminal's background.
                    let (x, y) = fill_at(api);
                    assert!(is(px_at(x, y), ui.card), "{n}: api is the card's colour");
                    let merged = px_at(x, bottom - 1.0);
                    assert!(is(merged, theme.background), "{n}: api merges: {merged:?}");
                    let line = px_at(x, top + 1.0);
                    assert!(is(line, ui.card), "{n}: no line on api: {line:?}");
                    // Above the tabs, the bar.
                    let above = px_at(x, bar_top + 2.0);
                    assert!(is(above, ui.tab_bar), "{n}: the bar above api: {above:?}");
                    // The empty strip stands apart from the active tab's card colour.
                    let rgb = |p: [u8; 3]| alacritty_terminal::vte::ansi::Rgb {
                        r: p[0],
                        g: p[1],
                        b: p[2],
                    };
                    let apart = contrast(rgb(above), rgb(px_at(x, y)));
                    // (Light themes, and dark ones whose card is near black, reach less than the
                    // minimum: see `Theme::ui`.)
                    let floor = if theme.is_dark() && ui.tab_bar != rgb([0, 0, 0]) {
                        ui_tokens::TAB_BAR_MIN
                    } else {
                        ui_tokens::TAB_BAR_LOW_MIN
                    };
                    assert!(apart >= floor - 0.01, "{n}: strip vs api tab {apart:.3}");
                    let (x, y) = fill_at(claude);
                    assert!(is(px_at(x, y), ui.waiting_tint), "{n}: claude tinted");
                    // codex: no fill, no ×.
                    let (x, y) = fill_at(codex);
                    assert!(is(px_at(x, y), ui.tab_bar), "{n}: codex on the bar");
                    assert!(!close_drawn(codex, ui.tab_bar), "{n}: codex's × hidden");
                    // The right pane's tests: active, the card's colour, merging too.
                    let x = f32::from(api.left()) + 520.0 + 150.0;
                    assert!(is(px_at(x, bottom - 6.0), ui.card), "{n}: tests");
                    assert!(
                        is(px_at(x, bottom - 1.0), theme.background),
                        "{n}: tests merge"
                    );
                    assert!(is(px_at(x, top + 1.0), ui.card), "{n}: no line on tests");
                }
                "tab-bars-hover" => {
                    let (x, y) = fill_at(codex);
                    let p = px_at(x, y);
                    assert!(is(p, ui.tab_hover), "{n}: codex hovered: {p:?}");
                    assert!(close_drawn(codex, ui.tab_hover), "{n}: codex's × shown");
                }
                "tab-bars-button" => {
                    let (plus, more) =
                        (button_corner(PLUS_FROM_EDGE), button_corner(MORE_FROM_EDGE));
                    assert!(is(plus, ui.tab_hover), "{n}: + hovered: {plus:?}");
                    assert!(is(more, ui.tab_bar), "{n}: ⋯ not: {more:?}");
                }
                "tab-bars-more" => {
                    // ⋯ lit: its fill stands off the bar in the 2 pt gap beside it (both under
                    // the menu's shadow, so compared with each other, not the tokens).
                    let more = button_corner(MORE_FROM_EDGE);
                    let beside = px_at(
                        bar_right - MORE_FROM_EDGE - 11.0,
                        bar_top + BUTTON_TOP + 2.0,
                    );
                    let rgb = |[r, g, b]: [u8; 3]| alacritty_terminal::vte::ansi::Rgb { r, g, b };
                    let h = ui.tab_hover;
                    let lit = gap([h.r, h.g, h.b], ui.tab_bar);
                    assert!(
                        gap(more, rgb(beside)) * 2 >= lit,
                        "{n}: ⋯ lit: {more:?} beside {beside:?}"
                    );
                    // The menu's top right at the ⋯'s bottom right.
                    let (mx, my) = (
                        bar_right - MORE_FROM_EDGE + 10.0,
                        bar_top + BUTTON_TOP + 20.0,
                    );
                    let inside = px_at(mx - 2.5, my + 30.0);
                    assert!(is(inside, ui.panel), "{n}: the menu under ⋯: {inside:?}");
                    let right = px_at(mx + 3.0, my + 30.0);
                    assert!(!is(right, ui.panel), "{n}: right edges together: {right:?}");
                    let above = px_at(mx - 100.0, my - 2.0);
                    assert!(!is(above, ui.panel), "{n}: under the button: {above:?}");
                    // Four items (26 pt each, inside the 1 pt edge and 4 pt padding), each with
                    // its name; the keys at the right of all but New Browser Tab; the menu ends
                    // after the fourth.
                    let inked = |x0: f32, x1: f32, y: f32| {
                        (x0 as i32..x1 as i32).any(|x| {
                            (-4..=4).any(|dy| gap(px_at(x as f32, y + dy as f32), ui.panel) > 60)
                        })
                    };
                    for k in 0..4 {
                        let y = my + 5.0 + 26.0 * k as f32 + 13.0;
                        assert!(inked(mx - 192.0, mx - 110.0, y), "{n}: item {k}'s name");
                        let keys = inked(mx - 40.0, mx - 10.0, y);
                        assert_eq!(keys, k != 1, "{n}: item {k}'s keys");
                    }
                    let end = my + 1.0 + 4.0 + 4.0 * 26.0 + 4.0 + 1.0;
                    // (Clear of the panel's rounded corners.)
                    let last = px_at(mx - 20.0, end - 3.0);
                    assert!(is(last, ui.panel), "{n}: the menu's foot: {last:?}");
                    let past = px_at(mx - 20.0, end + 3.0);
                    assert!(
                        !is(past, ui.panel),
                        "{n}: nothing after the fourth: {past:?}"
                    );
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
                if many == 1 {
                    assert_eq!(f32::from(first.size.width), 200.0, "{n}: widest");
                    assert_eq!(offset, 0.0, "{n}: no scroll");
                } else {
                    assert_eq!(f32::from(first.size.width), 90.0, "{n}: narrowest");
                    assert!(offset < 0.0, "{n}: scrolled to the last tab");
                    let last = tab_bounds(cx, &ws, panes[0], many - 1).expect("laid out");
                    assert!(
                        f32::from(last.right()) <= 720.0 - GAP - 1.0 - BAR_BUTTONS + 1.0,
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
            // The left bar's ⋯ (the right pane, tests, has the focus): Escape closes its menu.
            let right = api.left() + px(LEFT_BAR_WIDTH);
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
            // Again, and its Split Right (the third item): the left pane splits, beside it a
            // third pane, focused; the right pane is still last.
            cx.simulate_click(window, more, none);
            assert!(open(cx).is_some(), "{n}: open again");
            let split = point(at.x - px(100.0), at.y + px(5.0 + 26.0 * 2.0 + 13.0));
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
            // The tabs' top: 4 pt into the bars, inside the cards' 1 pt edges, under the 6 pt
            // strip (M11i).
            let top = f32::from(tabs[0].top());
            let width = f32::from(tabs[0].size.width);
            let (chip_x, chip_y) = (f32::from(g.chip.x), f32::from(g.chip.y));
            // No insertion line: no accent across the left bar's tabs.
            let no_line = || {
                for x in (f32::from(tabs[0].left()) as i32..f32::from(tabs[3].right()) as i32)
                    .map(|x| x as f32 + 0.5)
                {
                    assert!(!is(px_at(x, top + 13.0), ui.accent), "{n}: no line at {x}");
                }
            };
            // A place in the left bar with nothing drawn in it (a tab gone from there).
            let empty = |tab: gpui::Bounds<Pixels>, what: &str| {
                let left = f32::from(tab.left());
                for dx in [6.0, 20.0, 40.0, 60.0, 80.0] {
                    for y in [top + 7.0, top + 13.0, top + 19.0] {
                        let p = px_at(left + dx, y);
                        assert!(is(p, ui.tab_bar), "{n}: {what} empty at +{dx},{y}: {p:?}");
                    }
                }
            };
            match scene {
                "tab-drag-gap" => {
                    // claude (tinted) slid left into api's place; the gap is where claude was.
                    let claude = px_at(f32::from(tabs[0].center().x), top + 3.0);
                    assert!(
                        is(claude, ui.waiting_tint),
                        "{n}: claude slid left: {claude:?}"
                    );
                    let gap = px_at(f32::from(tabs[1].right()) - 4.0, top + 3.0);
                    assert!(is(gap, ui.tab_bar), "{n}: the gap: {gap:?}");
                    // The chip is where api was taken: its left edge at the pointer less the
                    // grab, level with the bar (not under it).
                    assert_eq!(chip_y, top, "{n}: the chip's top is the bar's");
                    let inside = px_at(chip_x + 3.0, top + 3.0);
                    assert!(
                        is(inside, ui.panel),
                        "{n}: the chip's left edge: {inside:?}"
                    );
                    let outside = px_at(chip_x - 3.0, top + 3.0);
                    assert!(!is(outside, ui.panel), "{n}: left of the chip: {outside:?}");
                    let low = px_at(chip_x + 10.0, top + 23.0);
                    assert!(is(low, ui.panel), "{n}: the chip down the bar: {low:?}");
                    assert!(
                        (chip_x - (f32::from(tabs[1].left()) - 10.0)).abs() <= 1.0,
                        "{n}: the chip's centre 10 px short of claude's middle"
                    );
                    no_line();
                }
                "tab-drag-other" => {
                    // Its own bar closed up: claude in api's place, the last place empty.
                    let claude = px_at(f32::from(tabs[0].center().x), top + 3.0);
                    assert!(
                        is(claude, ui.waiting_tint),
                        "{n}: claude slid left: {claude:?}"
                    );
                    empty(tabs[3], "the left bar's last place");
                    // The right bar: tests (the card's colour, as its pane's shown tab) slid
                    // right by api's width; the gap's end shows the bar.
                    let left = f32::from(g.right.left());
                    let tests = px_at(left + width + 6.0, top + 3.0);
                    assert!(is(tests, ui.card), "{n}: tests slid right: {tests:?}");
                    let gap = px_at(left + width - 6.0, top + 3.0);
                    assert!(is(gap, ui.tab_bar), "{n}: the gap: {gap:?}");
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
/// - `file-tab-end`: 80 long lines, all changed and found ("x"), scrolled to the end: the
///   bottom room (as tall as the card's corner radius) stays the background, so no tint, match
///   or selection reaches the card's rounded bottom corners.
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
    // 80 lines, each running far past the right edge (for `file-tab-end`).
    let end_rs = dir.join("end.rs");
    let end_lines = 80;
    let end_text: String = (0..end_lines)
        .map(|i| format!("let line_{i:02} = \"{}\";\n", "x".repeat(200)))
        .collect();
    std::fs::write(&end_rs, end_text).expect("end.rs");
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
                    let mut tab = FileTab::new(
                        path.clone(),
                        crate::layout_model::FileMode::Plain,
                        window,
                        cx,
                    );
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

        // Scrolled to its end, every line changed (tinted the full width) and found: nothing
        // is painted in the bottom room, where the card's bottom corners are (Task 4 review, M2).
        let n = format!("file-tab-end-{slug}");
        let slot: TabSlot = Rc::default();
        let build = build_for(end_rs.clone(), slot.clone());
        let act = {
            let n = n.clone();
            move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let tab = tab_of(&slot);
                frame(cx, window);
                cx.update(|cx| {
                    tab.update(cx, |t, cx| {
                        // Lines counted from 1, as git does.
                        let all = (1..=end_lines).map(|i| (i, LineChange::Changed));
                        t.set_changes(all.collect(), cx)
                    })
                });
                cx.update_window(window, |_, window, cx| {
                    tab.read(cx).focus_handle(cx).focus(window, cx)
                })
                .expect("focus");
                cx.simulate_keystrokes(window, "cmd-f");
                cx.simulate_input(window, "x");
                frame(cx, window);
                let g = cx.read(|cx| tab.read(cx).geom()).expect("drawn");
                for _ in 0..3 {
                    cx.simulate_event(
                        window,
                        ScrollWheelEvent {
                            position: g.bounds.center(),
                            delta: ScrollDelta::Pixels(point(px(0.0), px(-50_000.0))),
                            modifiers: Modifiers::none(),
                            touch_phase: TouchPhase::Moved,
                        },
                    );
                }
                frame(cx, window);
                let (scroll, g) = cx.read(|cx| (tab.read(cx).scroll(), tab.read(cx).geom()));
                let g = g.expect("drawn");
                assert!(scroll.y > px(0.0), "{n}: scrolled down");
                assert_eq!(scroll.y, g.max_scroll.y, "{n}: to the end");
                slot.borrow_mut().take();
            }
        };
        let path = shoot_with(
            "file-tab-end",
            slug,
            &theme,
            dims,
            build.clone(),
            act.clone(),
        );
        println!("ui-shots: {}", path.display());
        let shot = render_with(&theme, dims, build, act);
        let (w, h) = (f32::from(dims.width), f32::from(dims.height));
        let tint = crate::theme::mix(theme.background, fit(theme.palette[4]), 0.12);
        // The last line, tinted, above the room ...
        let room = ui_tokens::size::CARD_RADIUS;
        let p = pixel_at(&shot, dims, w - 2.0, h - room - 2.0);
        assert!(
            near(p, tint, 4) && !near(p, theme.background, 4),
            "{n}: the last line is tinted: {p:?}, not {tint:?}"
        );
        // ... and the room under it, across the width, the background.
        for x in [1.0, 2.0, 6.0, w / 2.0, w - 6.0, w - 2.0, w - 1.0] {
            for y in [h - 1.0, h - 2.0, h - room / 2.0] {
                let p = pixel_at(&shot, dims, x, y);
                assert!(
                    near(p, theme.background, 4),
                    "{n}: nothing in the bottom room at ({x}, {y}): {p:?}"
                );
            }
        }

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

/// The diff fixture: `FILE_VIEW_FIXTURE` with line 7 changed (row 6 removed, row 7 added) and
/// a line added after line 18 (row 19), as git prints it with the whole file as context.
fn file_diff_fixture() -> String {
    let old: Vec<&str> = FILE_VIEW_FIXTURE.lines().collect();
    let mut u = format!(
        "diff --git a/view.rs b/view.rs\n--- a/view.rs\n+++ b/view.rs\n@@ -1,{} +1,{} @@\n",
        old.len(),
        old.len() + 1
    );
    for (i, line) in old.iter().enumerate() {
        if i == 6 {
            u.push_str(&format!("-{line}\n+const NAME: &str = \"fuxx diff\";\n"));
            continue;
        }
        u.push_str(&format!(" {line}\n"));
        if i == 17 {
            u.push_str("+    pub fn port(&self) -> u16 { self.port }\n");
        }
    }
    u
}

/// The file tab's diff mode (M11h Task 3), in both themes:
/// - `file-diff`: a tab on the `view.rs` fixture in Diff mode, given `file_diff_fixture`: its
///   rows coloured; a removed row tinted red at 12 % with a `−` in the sign column, an added row
///   green with a `+`, an unchanged row untinted; the old number column of an added row and the
///   new number column of a removed row hold no number (the other one does); opened with its
///   first change 3 rows from the top; ⌘F "fn" counts the diff text's matches; two rows dragged
///   over and ⌘C copy their text without signs or numbers.
/// - `file-diff-states-*`: `same` ("No changes since the last commit."), `binary` ("This file is
///   binary: no diff to show."), `norepo` (a file outside any repo, looked up for real: "Not a
///   git repository."), `missing-rows` (a deleted file's removed rows show, not "This file no
///   longer exists"), `missing-same` (a deleted file with nothing to show: that note).
/// - `file-diff-toggle`: a workspace with the Files panel; `open_file_tab(view.rs, Diff)`: the
///   toolbar's toggle with Diff `card_selected` and File `panel`. Not shot: File and Diff
///   clicked both ways, each mode keeping its own scroll; a click on File (the tab and the
///   model Plain, the same tab); a click on line 7's change bar (Diff, its
///   change scrolled into view, the model Diff); a Files click on view.rs (the same tab, Plain).
fn file_diff_scenes() {
    use crate::diff_view::{BINARY, NO_CHANGES, NO_REPO, RowKind, first_change, reveal_row};
    use crate::file_tab::{FileTab, MISSING};
    use crate::git_status::LineChange;
    use crate::layout_model::{FileMode, TabKind};
    use crate::right_sidebar::{FILTER_BAND, ROOT_HEADER, ROW};
    use crate::ui_tokens::size::{FOCUS_RING, TAB_BAR};
    use crate::ui_tokens::{MARK_MIN, ensure_contrast};
    use alacritty_terminal::vte::ansi::Rgb;
    use gpui::{Focusable, Modifiers, MouseButton};
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    let dir = std::env::temp_dir().join("fuxx-ui-shots-file-diff");
    std::fs::create_dir_all(dir.join("src")).expect("the fixture folder");
    let dir = std::fs::canonicalize(&dir).unwrap_or(dir);
    let view_rs = dir.join("src/view.rs");
    std::fs::write(&view_rs, FILE_VIEW_FIXTURE).expect("view.rs");
    let missing = dir.join("src/gone.rs");
    let _ = std::fs::remove_file(&missing);
    let unified = Rc::new(file_diff_fixture());
    // The fixture's rows (as `parse_diff` makes them): 6 removed, 7 and 19 added, 9 unchanged.
    let (removed, added, same, added_late) = (6usize, 7usize, 9usize, 19usize);

    type TabSlot = Rc<RefCell<Option<gpui::Entity<FileTab>>>>;
    let tab_of = |slot: &TabSlot| slot.borrow().clone().expect("the file tab");
    let dims = size(px(760.0), px(460.0));
    for (slug, name) in SHOT_THEMES {
        let theme = shot_theme(name);
        let ui = theme.ui();
        let near = |p: [u8; 3], c: Rgb, by: u8| {
            p.iter()
                .zip([c.r, c.g, c.b])
                .all(|(a, b)| a.abs_diff(b) <= by)
        };
        let fit = |c: Rgb| ensure_contrast(c, theme.background, MARK_MIN);
        let (green, red) = (fit(theme.palette[2]), fit(theme.palette[1]));
        let build_for = |path: PathBuf, slot: TabSlot, frozen: bool| {
            move |window: &mut Window, cx: &mut App| -> AnyView {
                cx.set_global(shot_appearance(theme, cx));
                cx.bind_keys(crate::key_bindings());
                let tab = cx.new(|cx| {
                    let mut tab = FileTab::new(path.clone(), FileMode::Diff, window, cx);
                    tab.frozen = frozen;
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

        // The diff: rows, tints, signs, two number columns, the first change near the top,
        // find and copy.
        let n = format!("file-diff-{slug}");
        let slot: TabSlot = Rc::default();
        let build = build_for(view_rs.clone(), slot.clone(), true);
        let copied: Rc<RefCell<Option<String>>> = Rc::default();
        let seen = copied.clone();
        let geom_seen: Rc<Cell<Option<crate::file_tab::Geom>>> = Rc::default();
        let keep_geom = geom_seen.clone();
        let texts: Rc<RefCell<Vec<String>>> = Rc::default();
        let keep_texts = texts.clone();
        let act = {
            let n = n.clone();
            let unified = unified.clone();
            move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let tab = tab_of(&slot);
                frame(cx, window);
                assert_eq!(
                    cx.read(|cx| tab.read(cx).diff_state_name()),
                    "waiting",
                    "{n}"
                );
                cx.update(|cx| tab.update(cx, |t, cx| t.set_diff_for_shots(&unified, cx)));
                frame(cx, window);
                let (first, lines, fn_count) = cx.read(|cx| {
                    let t = tab.read(cx);
                    assert_eq!(t.mode(), FileMode::Diff, "{n}");
                    assert_eq!(t.diff_state_name(), "rows", "{n}");
                    let d = t.diff_doc().expect("its rows");
                    assert!(d.doc.spans.is_some(), "{n}: coloured");
                    assert_eq!(d.rows[removed].kind, RowKind::Removed, "{n}");
                    assert_eq!(d.rows[added].kind, RowKind::Added, "{n}");
                    assert_eq!(d.rows[added_late].kind, RowKind::Added, "{n}");
                    assert_eq!(d.rows[same].kind, RowKind::Same, "{n}");
                    assert_eq!(d.doc.lines.len(), d.rows.len(), "{n}: a line per row");
                    let lines: Vec<String> = (0..d.rows.len())
                        .map(|i| d.doc.line(i).to_string())
                        .collect();
                    let fns = lines.iter().map(|l| l.to_lowercase().matches("fn").count());
                    (first_change(&d.rows), lines.clone(), fns.sum::<usize>())
                });
                *keep_texts.borrow_mut() = lines;
                let g = cx.read(|cx| tab.read(cx).geom()).expect("drawn");
                let first = first.expect("a change");
                assert_eq!(
                    g.scroll.y,
                    g.line_height * first.saturating_sub(3) as f32,
                    "{n}: opened with its first change 3 rows from the top"
                );
                cx.update_window(window, |_, window, cx| {
                    tab.read(cx).focus_handle(cx).focus(window, cx)
                })
                .expect("focus");
                cx.simulate_keystrokes(window, "cmd-f");
                cx.simulate_input(window, "fn");
                frame(cx, window);
                let label = cx.read(|cx| tab.read(cx).find_label());
                assert_eq!(label, format!("1 of {fn_count}"), "{n}: ⌘F fn in the diff");
                // From the removed row's start to the added row's 5th byte.
                let g = cx.read(|cx| tab.read(cx).geom()).expect("drawn");
                let at = |row: usize, x: f32| {
                    point(
                        g.bounds.left() + g.text_left + px(x),
                        g.bounds.top() + g.line_height * (row as f32 + 0.5) - g.scroll.y,
                    )
                };
                let cw = f32::from(g.cell_width);
                let end = at(added, cw * 5.0 + 1.0);
                cx.simulate_mouse_down(
                    window,
                    at(removed, 0.5),
                    MouseButton::Left,
                    Modifiers::none(),
                );
                cx.simulate_mouse_move(window, end, MouseButton::Left, Modifiers::none());
                cx.simulate_mouse_up(window, end, MouseButton::Left, Modifiers::none());
                cx.simulate_keystrokes(window, "cmd-c");
                *seen.borrow_mut() = cx.read_from_clipboard().and_then(|c| c.text());
                frame(cx, window);
                keep_geom.set(cx.read(|cx| tab.read(cx).geom()));
                slot.borrow_mut().take();
            }
        };
        let path = shoot_with("file-diff", slug, &theme, dims, build.clone(), act.clone());
        println!("ui-shots: {}", path.display());
        let shot = render_with(&theme, dims, build, act);
        let texts = texts.borrow();
        let expect = format!("{}\n{}", texts[removed], &texts[added][..5]);
        assert_eq!(
            copied.borrow().as_deref(),
            Some(expect.as_str()),
            "{n}: ⌘C, no signs"
        );
        let g = geom_seen.get().expect("drawn");
        let at = |x: Pixels, y: Pixels| pixel_at(&shot, dims, f32::from(x), f32::from(y));
        let row_y = |row: usize| g.bounds.top() + g.line_height * (row as f32 + 0.5) - g.scroll.y;
        // The tints, 6 pt inside the right edge.
        let right = g.bounds.right() - px(6.0);
        let red_tint = crate::theme::mix(theme.background, red, 0.12);
        let green_tint = crate::theme::mix(theme.background, green, 0.12);
        for (row, tint, what) in [
            (removed, red_tint, "removed"),
            (added, green_tint, "added"),
            (added_late, green_tint, "added"),
            (same, theme.background, "unchanged"),
        ] {
            let p = at(right, row_y(row));
            assert!(
                near(p, tint, 5),
                "{n}: the {what} row's tint {p:?}, not {tint:?}"
            );
        }
        // An unchanged row's colours dimmed (`diff_view::dim`), an added row's not: "pub" is a
        // keyword on both.
        let keyword = fit(theme.palette[5]);
        let dimmed = crate::diff_view::dim(keyword, theme.background);
        let ink_in = |row: usize, c: Rgb, by: u8| {
            (0..300).any(|dx| {
                (-4..=4).any(|dy| {
                    let x = g.bounds.left() + g.text_left + px(dx as f32);
                    near(at(x, row_y(row) + px(dy as f32)), c, by)
                })
            })
        };
        assert!(
            ink_in(same, dimmed, 24),
            "{n}: an unchanged row's keyword in {dimmed:?}"
        );
        assert!(
            ink_in(added_late, keyword, 30),
            "{n}: an added row's keyword in {keyword:?}"
        );
        if !near([dimmed.r, dimmed.g, dimmed.b], keyword, 40) {
            assert!(
                !ink_in(same, keyword, 16),
                "{n}: an unchanged row's keyword not at full colour"
            );
        }
        // The signs, in the one-cell sign column.
        let sign_in = |row: usize, c: Rgb| {
            let top = row_y(row) - g.line_height / 2.0;
            (0..(f32::from(g.cell_width) as i32)).any(|dx| {
                (2..(f32::from(g.line_height) as i32 - 2)).any(|dy| {
                    near(
                        at(
                            g.bounds.left() + g.bar_left + px(dx as f32),
                            top + px(dy as f32),
                        ),
                        c,
                        40,
                    )
                })
            })
        };
        assert!(sign_in(removed, red), "{n}: the − in red");
        assert!(sign_in(added, green), "{n}: the + in green");
        assert!(
            !sign_in(same, red) && !sign_in(same, green),
            "{n}: no sign on an unchanged row"
        );
        // The number columns: (left, right) edges.
        let old_right = g.old_numbers_right.expect("two number columns");
        let old_col = (px(12.0), old_right);
        let new_col = (old_right + px(8.0), g.numbers_right);
        let inked = |row: usize, (l, r): (Pixels, Pixels), base: Rgb| {
            let top = row_y(row) - g.line_height / 2.0;
            (f32::from(l) as i32..f32::from(r) as i32).any(|x| {
                (2..(f32::from(g.line_height) as i32 - 2)).any(|dy| {
                    !near(
                        at(g.bounds.left() + px(x as f32), top + px(dy as f32)),
                        base,
                        6,
                    )
                })
            })
        };
        assert!(
            !inked(added, old_col, green_tint),
            "{n}: no old number on an added row"
        );
        assert!(
            inked(added, new_col, green_tint),
            "{n}: the added row's new number"
        );
        assert!(
            !inked(removed, new_col, red_tint),
            "{n}: no new number on a removed row"
        );
        assert!(
            inked(removed, old_col, red_tint),
            "{n}: the removed row's old number"
        );
        assert!(
            inked(same, old_col, theme.background) && inked(same, new_col, theme.background),
            "{n}: both numbers on an unchanged row"
        );

        // The states: a note instead of rows, or a deleted file's rows.
        let states = [
            ("same", view_rs.clone(), Some(""), "same", Some(NO_CHANGES)),
            (
                "binary",
                view_rs.clone(),
                Some("diff --git a/v b/v\nindex 1..2 100644\nBinary files a/v and b/v differ\n"),
                "binary",
                Some(BINARY),
            ),
            // A file outside any repo, looked up for real (not frozen).
            (
                "norepo",
                dir.join("src/view.rs"),
                None,
                "norepo",
                Some(NO_REPO),
            ),
            (
                "missing-rows",
                missing.clone(),
                Some("@@ -1,2 +0,0 @@\n-fn old() {}\n-// gone\n"),
                "rows",
                None,
            ),
            (
                "missing-same",
                missing.clone(),
                Some(""),
                "same",
                Some(MISSING),
            ),
        ];
        for (case, file, given, state, note) in states {
            let n = format!("file-diff-states-{case}-{slug}");
            let slot: TabSlot = Rc::default();
            let build = build_for(file, slot.clone(), given.is_some());
            let act = {
                let n = n.clone();
                move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                    let tab = tab_of(&slot);
                    frame(cx, window);
                    if let Some(u) = given {
                        cx.update(|cx| tab.update(cx, |t, cx| t.set_diff_for_shots(u, cx)));
                    }
                    frame(cx, window);
                    cx.read(|cx| {
                        let t = tab.read(cx);
                        assert_eq!(t.diff_state_name(), state, "{n}");
                        assert_eq!(t.shown_note(), note, "{n}");
                        if case.starts_with("missing") {
                            assert_eq!(t.state_name(), "missing", "{n}");
                        }
                    });
                    slot.borrow_mut().take();
                }
            };
            let path = shoot_with(
                &format!("file-diff-states-{case}"),
                slug,
                &theme,
                dims,
                build.clone(),
                act.clone(),
            );
            println!("ui-shots: {}", path.display());
            // What the body shows: the note's words in the middle, or the removed rows' tint.
            let shot = render_with(&theme, dims, build, act);
            let (w, h) = (f32::from(dims.width), f32::from(dims.height));
            let middle = (0..(w as i32)).step_by(2).any(|x| {
                // The body's middle, under the toolbar.
                (-40..=40).any(|dy| {
                    near(
                        pixel_at(&shot, dims, x as f32, h / 2.0 + dy as f32),
                        ui.text,
                        40,
                    )
                })
            });
            if note.is_some() {
                assert!(middle, "{n}: the note's words in the middle");
            } else {
                let red_tint = crate::theme::mix(theme.background, red, 0.12);
                let p = pixel_at(&shot, dims, w - 6.0, 70.0);
                assert!(
                    near(p, red_tint, 5),
                    "{n}: a removed row {p:?}, not {red_tint:?}"
                );
            }
        }

        // The toggle, in a workspace with the Files panel (as the files scenes lay it out).
        let window_dims = size(px(1100.0), px(560.0));
        let sidebar_left = 1100.0 - GAP - 300.0 + FOCUS_RING;
        let list_top = GAP + FOCUS_RING + TAB_BAR + ROOT_HEADER + FILTER_BAND;
        let build_ws = |slot: Slot| {
            let dir = dir.clone();
            move |window: &mut Window, cx: &mut App| -> AnyView {
                use crate::file_tree::{CAP, FileTree};
                use crate::git_status::list_dir;
                cx.set_global(shot_appearance(theme, cx));
                cx.bind_keys(crate::key_bindings());
                let ws = cx.new(|cx| {
                    let mut ws = Workspace::for_shots(window, cx);
                    ws.sidebar.hint_dismissed = true;
                    ws.right_sidebar.hidden = false;
                    let src = dir.join("src");
                    ws.open_shot_session("api", &src, SHELL_TEXT, None, window, cx);
                    let mut tree = FileTree::new(dir.clone(), Some(PathBuf::from("src")));
                    for rel in tree.wanted() {
                        let (entries, more) = list_dir(&dir.join(&rel), CAP);
                        tree.expand(rel, entries, more);
                    }
                    ws.right_sidebar.files.show_for_shots(tree);
                    ws
                });
                *slot.borrow_mut() = Some(ws.clone());
                ws.into()
            }
        };
        let open_diff = {
            let view_rs = view_rs.clone();
            let unified = unified.clone();
            move |cx: &mut VisualTestAppContext,
                  window: AnyWindowHandle,
                  ws: &gpui::Entity<Workspace>| {
                cx.update_window(window, |_, window, cx| {
                    ws.update(cx, |ws, cx| {
                        ws.open_file_tab(view_rs.clone(), FileMode::Diff, window, cx)
                    })
                })
                .expect("open the diff");
                frame(cx, window);
                let view = cx.read(|cx| ws.read(cx).focused_file_tab().expect("its view").clone());
                cx.update(|cx| view.update(cx, |t, cx| t.set_diff_for_shots(&unified, cx)));
                frame(cx, window);
                view
            }
        };
        let n = format!("file-diff-toggle-{slug}");
        let toggle: Rc<Cell<[Option<gpui::Bounds<Pixels>>; 2]>> = Rc::default();
        let slot: Slot = Rc::new(RefCell::new(None));
        let build = build_ws(slot.clone());
        let act = {
            let n = n.clone();
            let toggle = toggle.clone();
            let view_rs = view_rs.clone();
            let open_diff = open_diff.clone();
            move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let ws = slotted(&slot);
                frame(cx, window);
                let view = open_diff(cx, window, &ws);
                cx.read(|cx| {
                    let ws = ws.read(cx);
                    let tab = ws.model.focused_tab().expect("a tab");
                    let kind = ws.model.tab(tab).expect("known").kind.clone();
                    assert_eq!(
                        kind,
                        TabKind::File {
                            path: view_rs.clone(),
                            mode: FileMode::Diff
                        },
                        "{n}"
                    );
                    assert_eq!(view.read(cx).diff_state_name(), "rows", "{n}");
                });
                toggle.set(cx.read(|cx| view.read(cx).toggle_bounds()));
                slot.borrow_mut().take();
            }
        };
        let path = shoot_with(
            "file-diff-toggle",
            slug,
            &theme,
            window_dims,
            build.clone(),
            act.clone(),
        );
        println!("ui-shots: {}", path.display());
        let shot = render_with(&theme, window_dims, build, act);
        let [file_seg, diff_seg] = toggle.get();
        let (file_seg, diff_seg) = (file_seg.expect("File drawn"), diff_seg.expect("Diff drawn"));
        // Inside each segment's padding (no words there).
        let fill_of = |b: gpui::Bounds<Pixels>| {
            pixel_at(
                &shot,
                window_dims,
                f32::from(b.left()) + 5.0,
                f32::from(b.center().y),
            )
        };
        let p = fill_of(diff_seg);
        assert!(
            near(p, ui.card_selected, 3),
            "{n}: Diff is current {p:?}, not {:?}",
            ui.card_selected
        );
        let p = fill_of(file_seg);
        assert!(
            near(p, ui.panel, 3),
            "{n}: File is not {p:?}, not {:?}",
            ui.panel
        );
        assert!(
            file_seg.right() <= diff_seg.left() + px(1.0),
            "{n}: File, then Diff"
        );

        // Clicks: File, a change bar, a Files row (not shot).
        let slot: Slot = Rc::new(RefCell::new(None));
        let build = build_ws(slot.clone());
        let act = {
            let n = format!("file-diff-toggle-clicks-{slug}");
            let view_rs = view_rs.clone();
            move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let ws = slotted(&slot);
                frame(cx, window);
                let view = open_diff(cx, window, &ws);
                let tab = cx.read(|cx| ws.read(cx).model.focused_tab().expect("a tab"));
                let modes = |cx: &mut VisualTestAppContext| {
                    cx.read(|cx| {
                        let ws = ws.read(cx);
                        let model = match &ws.model.tab(tab).expect("the same tab").kind {
                            TabKind::File { mode, .. } => *mode,
                            other => panic!("not a file tab: {other:?}"),
                        };
                        assert_eq!(ws.model.focused_tab(), Some(tab), "the same tab");
                        assert_eq!(ws.file_tabs.len(), 1, "one view");
                        (view.read(cx).mode(), model)
                    })
                };
                // Each mode keeps its own scroll: the diff scrolled, File (scrolled too), Diff
                // again, File again, by the toggle both ways.
                cx.update(|cx| view.update(cx, |t, cx| t.pretend_in_repo_for_shots(cx)));
                frame(cx, window);
                let lh = cx
                    .read(|cx| view.read(cx).geom())
                    .expect("drawn")
                    .line_height;
                let segment = |cx: &mut VisualTestAppContext, i: usize| {
                    cx.read(|cx| view.read(cx).toggle_bounds()[i])
                        .expect("the toggle")
                        .center()
                };
                let scroll = |cx: &mut VisualTestAppContext| cx.read(|cx| view.read(cx).scroll());
                cx.update(|cx| view.update(cx, |t, cx| t.set_scroll(point(px(0.0), lh * 5.0), cx)));
                frame(cx, window);
                let diff_at = scroll(cx);
                assert_eq!(diff_at.y, lh * 5.0, "{n}: the diff scrolled");
                let at = segment(cx, 0);
                cx.simulate_click(window, at, Modifiers::none());
                frame(cx, window);
                assert_eq!(modes(cx), (FileMode::Plain, FileMode::Plain), "{n}: File");
                assert_eq!(scroll(cx).y, px(0.0), "{n}: the file's own scroll");
                cx.update(|cx| view.update(cx, |t, cx| t.set_scroll(point(px(0.0), lh * 2.0), cx)));
                frame(cx, window);
                let at = segment(cx, 1);
                cx.simulate_click(window, at, Modifiers::none());
                frame(cx, window);
                assert_eq!(modes(cx), (FileMode::Diff, FileMode::Diff), "{n}: Diff");
                assert_eq!(scroll(cx), diff_at, "{n}: the diff's scroll kept");
                let at = segment(cx, 0);
                cx.simulate_click(window, at, Modifiers::none());
                frame(cx, window);
                assert_eq!(scroll(cx).y, lh * 2.0, "{n}: the file's scroll kept");
                let at = segment(cx, 1);
                cx.simulate_click(window, at, Modifiers::none());
                frame(cx, window);
                assert_eq!(modes(cx), (FileMode::Diff, FileMode::Diff), "{n}: Diff");
                // Scrolled to the diff's end, away from its changes.
                cx.update(|cx| {
                    view.update(cx, |t, cx| t.set_scroll(point(px(0.0), px(1.0e5)), cx))
                });
                frame(cx, window);
                let file_seg = cx
                    .read(|cx| view.read(cx).toggle_bounds()[0])
                    .expect("File");
                cx.simulate_click(window, file_seg.center(), Modifiers::none());
                frame(cx, window);
                assert_eq!(modes(cx), (FileMode::Plain, FileMode::Plain), "{n}: File");
                // Line 7's change bar: the diff, at that change.
                cx.update(|cx| {
                    view.update(cx, |t, cx| {
                        t.set_changes(vec![(7, LineChange::Changed)], cx)
                    })
                });
                frame(cx, window);
                let g = cx.read(|cx| view.read(cx).geom()).expect("drawn");
                let bar = point(
                    g.bounds.left() + g.bar_left + px(1.5),
                    g.bounds.top() + g.line_height * 6.5 - g.scroll.y,
                );
                cx.simulate_click(window, bar, Modifiers::none());
                frame(cx, window);
                assert_eq!(
                    modes(cx),
                    (FileMode::Diff, FileMode::Diff),
                    "{n}: a bar's click"
                );
                let (row, g) = cx.read(|cx| {
                    let t = view.read(cx);
                    let rows = &t.diff_doc().expect("its rows").rows;
                    (
                        reveal_row(rows, (7, LineChange::Changed)).expect("line 7's row"),
                        t.geom(),
                    )
                });
                let g = g.expect("drawn");
                let (top, h) = (g.line_height * row as f32, g.bounds.size.height);
                assert!(
                    top >= g.scroll.y && top + g.line_height <= g.scroll.y + h,
                    "{n}: row {row} in view ({:?})",
                    g.scroll
                );
                // A Files click on the same file: the same tab, Plain.
                let row_at = |i: usize| {
                    point(
                        px(sidebar_left + 80.0),
                        px(list_top + (i as f32 + 0.5) * ROW),
                    )
                };
                let rows = cx.read(|cx| {
                    let files = &ws.read(cx).right_sidebar.files;
                    files
                        .rows
                        .iter()
                        .map(|r| r.rel.display().to_string())
                        .collect::<Vec<_>>()
                });
                let i = rows
                    .iter()
                    .position(|r| r == "src/view.rs")
                    .expect("view.rs listed");
                cx.simulate_click(window, row_at(i), Modifiers::none());
                frame(cx, window);
                assert_eq!(
                    modes(cx),
                    (FileMode::Plain, FileMode::Plain),
                    "{n}: a Files click"
                );
                let path = cx.read(
                    |cx| match &ws.read(cx).model.tab(tab).expect("known").kind {
                        TabKind::File { path, .. } => path.clone(),
                        _ => unreachable!(),
                    },
                );
                assert_eq!(path, view_rs, "{n}");
                slot.borrow_mut().take();
            }
        };
        render_with(&theme, window_dims, build, act);
        println!("ui-shots: file-diff-toggle-clicks-{slug} ok");
    }
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
            // Each pane's bar's right end, as laid out (M11i: inside its card's 1 pt edge; the
            // left one's: where the right one's bar starts, less both edges and the gap).
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
                    window_right - GAP - 300.0 - GAP - 1.0
                } else {
                    window_right - GAP - 1.0
                };
                *seen.borrow_mut() = vec![f32::from(second.left()) - 2.0 - GAP, pane_right];
                cx.read(|cx| {
                    let ws = ws.read(cx);
                    assert_eq!(ws.right_sidebar.hidden, !open, "{n}");
                    assert_eq!(ws.right_sidebar.width, 300.0, "{n}: 300 pt by default");
                    assert!(ws.right_sidebar.slide.is_none(), "{n}: still");
                });
                if scene == "right-sidebar-resize" {
                    // The gap at its left (971, between the pane card at 968 and its card at
                    // 974) taken and moved 80 left, then let go.
                    let (from, to) = (point(px(971.0), px(300.0)), point(px(891.0), px(300.0)));
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
                    seen.borrow_mut()[1] = window_right - GAP - 380.0 - GAP - 1.0;
                    // The panes re-fit to the narrower area on the next frame.
                    cx.update_window(window, |_, window, _| window.refresh())
                        .expect("refresh");
                    cx.run_until_parked();
                    let second = tab_bounds(cx, &ws, panes[1], 0).expect("laid out");
                    seen.borrow_mut()[0] = f32::from(second.left()) - 2.0 - GAP;
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
            // The sidebar's body, low in the window: a card (M11i) 6 pt from the window's
            // right edge, its fill across its width; the 6 pt gap left of it, then the pane's
            // card with the terminal.
            let y = 400.0;
            let card_right = window_right - GAP;
            if width > 0.0 {
                for x in [card_right - width + 6.0, card_right - 6.0] {
                    let p = at(x, y);
                    assert!(near(p, ui.card), "{n}: sidebar at {x}: {p:?}");
                }
                // (Beside its middle: after a resize the pointer is still there, lighting it.)
                for x in [card_right - width - GAP + 1.0, window_right - GAP / 2.0] {
                    let p = at(x, y);
                    assert!(is_canvas(p, &ui), "{n}: the gap at {x}: {p:?}");
                }
                if scene == "right-sidebar-resize" {
                    let p = at(card_right - width - GAP / 2.0, y);
                    assert!(near(p, ui.accent), "{n}: the hovered gap's line: {p:?}");
                }
                let p = at(card_right - width - GAP - 6.0, y);
                assert!(near(p, theme.background), "{n}: the pane left of it: {p:?}");
            } else {
                let p = at(card_right - 6.0, y);
                assert!(near(p, theme.background), "{n}: no right sidebar: {p:?}");
                let p = at(window_right - GAP / 2.0, y);
                assert!(
                    is_canvas(p, &ui),
                    "{n}: the gap at the window's edge: {p:?}"
                );
            }
            // Every pane's icon: lit (the hover fill beside the glyph, the glyph's filled side
            // in the text colour) while open; else on the bar, dim.
            for (i, right) in edges.borrow().iter().enumerate() {
                let centre = right - RIGHT_SIDEBAR_FROM_EDGE;
                assert!(*right > left, "{n}: pane {i}'s edge {right}");
                // The bars run 7–37 (under the 6 pt strip and the cards' edges).
                let (fill, glyph) = (at(centre - 8.0, 22.0), at(centre + 3.0, 22.0));
                if width > 0.0 {
                    assert!(near(fill, ui.tab_hover), "{n}: pane {i} lit: {fill:?}");
                    assert!(near(glyph, ui.text), "{n}: pane {i} glyph: {glyph:?}");
                } else {
                    let bar = ui.tab_bar;
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
                let icon = point(
                    px(window_right - GAP - 1.0 - RIGHT_SIDEBAR_FROM_EDGE),
                    px(22.0),
                );
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
    // Made by the `files-keys` scene, which removes it; gone if a failed run left it.
    std::fs::remove_file(root.join("src/zz.rs")).ok();
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
/// no git): src open and selected (with `util` open too when `open_util`), and a made-up mark
/// map: main.rs and README.md modified, new.rs added, old.rs deleted (so not on the disk).
fn files_tree(root: &std::path::Path, open_util: bool) -> crate::file_tree::FileTree {
    use crate::file_tree::{CAP, FileTree};
    use crate::git_status::{Mark, list_dir};
    use std::collections::HashMap;

    let mut tree = FileTree::new(root.to_path_buf(), Some(PathBuf::from("src")));
    let mut folders = tree.wanted();
    if open_util {
        folders.push(PathBuf::from("src/util"));
    }
    for rel in folders {
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
/// - `files`: the left sidebar shown, util open too. The tree: docs, src (open, selected, `M`:
///   it holds a modified and an added file), util (open), a.rs, main.rs (`M`, blue), new.rs
///   (`A`, green), Cargo.toml, notes.txt, old.rs (`D`, red: gone from the disk), README.md
///   (`M`). The panel's header row is level with the pane's tab bar, at the top. M11i: folder
///   icons in the dim text colour (open for src and util), file icons tinted by type (.rs
///   red, .toml cyan, .txt and .md dim), indent guides at depth 1 and 2, none at depth 0.
/// - `files-filter`: "main" typed in the filter box: src and main.rs only.
/// - `files-bare`: the left sidebar hidden: the header row is still level with the tab bar,
///   both under the 38 pt band.
/// - `files-header` (M11i): the root header above the filter: ▾, the root's name, collapse
///   all at the right, and the root's path under them, its start cut at the 300 pt sidebar.
///   After a click on collapse all, every folder is closed (the selection on src).
/// - `files-header-collapsed`: then a click on the name: the root folds away (▸), the filter
///   and the tree are gone; a click again brings them back (every folder still closed), and a
///   third folds it away again for the shot.
///
/// Not shot: in the tree, ↓ → ← ↑ and Return move, open and close; a click on main.rs opens its
/// file tab, which takes the keyboard; Esc in the tree gives the keyboard back to the pane;
/// with the filter box focused, ⌥⌘\ closes the sidebar and the terminal takes the keyboard.
fn files_scenes() {
    use crate::git_status::Mark;
    use crate::layout_model::{FileMode, TabKind};
    use crate::right_sidebar::{
        FILTER_BAND, FileKind, ROOT_HEADER, ROW, kind_color, mark_color, root_path_line,
    };
    use crate::ui_tokens::size::{FOCUS_RING, TAB_BAR, TAB_TOP_GAP, TOP_BAND};
    use gpui::{Focusable, Modifiers};
    use std::cell::RefCell;
    use std::rc::Rc;

    let dims = size(px(1100.0), px(560.0));
    let window_right = 1100.0_f32;
    // Inside the right sidebar's card (M11i): 6 pt from the window's right, 300 wide, a 1 pt
    // edge; its header 1 pt under the card's top (under the 6 pt strip, or the band).
    let sidebar_left = window_right - GAP - 300.0 + FOCUS_RING;
    let card_top = GAP + FOCUS_RING;
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
        let build_for = |bare: bool, open_util: bool, slot: Slot| {
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
                    let tree = files_tree(&root, open_util);
                    ws.right_sidebar.files.show_for_shots(tree);
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
            let top = if bare {
                TOP_BAND + FOCUS_RING
            } else {
                card_top
            };
            let slot: Slot = Rc::new(RefCell::new(None));
            let open_util = scene == "files";
            let build = build_for(bare, open_util, slot.clone());
            let tab_top: Rc<RefCell<Option<f32>>> = Rc::default();
            let seen = tab_top.clone();
            let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let ws = slotted(&slot);
                let n = format!("{scene}-{slug}");
                frame(cx, window);
                let mut rows = tree_rows.to_vec();
                if open_util {
                    rows.insert(3, "src/util/a.rs");
                }
                assert_eq!(rows_of(cx, &ws), rows, "{n}: the tree");
                let pane = cx.read(|cx| ws.read(cx).model.focused_pane().expect("a pane"));
                let tab = tab_bounds(cx, &ws, pane, 0).expect("the tab laid out");
                *seen.borrow_mut() = Some(f32::from(tab.top()));
                if scene == "files-filter" {
                    let field = point(
                        px(sidebar_left + 60.0),
                        px(top + TAB_BAR + ROOT_HEADER + FILTER_BAND / 2.0),
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
            // The header row lines up with the pane's tab bar: same top (its tabs 4 pt under
            // it, M11i), and its bottom edge where the bar ends.
            let bar_top = tab_top.borrow().expect("the tab's top") - TAB_TOP_GAP;
            assert!(
                (bar_top - top).abs() < 0.5,
                "{n}: the tab bar at {bar_top}, not {top}"
            );
            // M11h: the strip holds the panel tabs on `tab_bar` (its empty part at the right),
            // with no border under it: the card right below.
            let inside = at(sidebar_left + 250.0, top + TAB_BAR / 2.0);
            assert!(near(inside, ui.tab_bar, 3), "{n}: the strip: {inside:?}");
            let edge = at(sidebar_left + 250.0, top + TAB_BAR - 0.5);
            assert!(
                near(edge, ui.tab_bar, 3),
                "{n}: no border under the strip: {edge:?}"
            );
            let below = at(sidebar_left + 250.0, top + TAB_BAR + 0.5);
            assert!(near(below, ui.card, 3), "{n}: the card under it: {below:?}");
            // Each row's mark colour, somewhere along its name and letter (drawn in it), right
            // of its icon.
            let list_top = top + TAB_BAR + ROOT_HEADER + FILTER_BAND;
            let depths: &[usize] = match scene {
                "files" => &[0, 0, 1, 2, 1, 1, 0, 0, 0, 0],
                "files-filter" => &[0, 1],
                _ => &[0, 0, 1, 1, 1, 0, 0, 0, 0],
            };
            // A row's icon box: 8 pt in, 14 pt a level, after the 12 pt chevron column.
            let icon_left = |row: usize| sidebar_left + 8.0 + depths[row] as f32 * 14.0 + 12.0;
            let ink_in = |row: usize, x0: f32, x1: f32, c: alacritty_terminal::vte::ansi::Rgb| {
                let y0 = list_top + row as f32 * ROW;
                let steps = ((x1 - x0) * 2.0) as usize;
                (4..(ROW as usize * 2 - 4)).any(|dy| {
                    let y = y0 + dy as f32 * 0.5;
                    (0..steps).any(|dx| near(at(x0 + dx as f32 * 0.5, y), c, 24))
                })
            };
            let inked = |row: usize, c: alacritty_terminal::vte::ansi::Rgb| {
                let x0 = icon_left(row) + 12.0 + 1.0;
                ink_in(row, x0, sidebar_left + 290.0, c)
            };
            let (m, a, d) = (
                mark_color(Mark::Modified, &theme),
                mark_color(Mark::Added, &theme),
                mark_color(Mark::Deleted, &theme),
            );
            let expect: Vec<(usize, Option<alacritty_terminal::vte::ansi::Rgb>)> = match scene {
                "files-filter" => vec![(0, Some(m)), (1, Some(m))],
                "files" => vec![
                    (0, None),
                    (1, Some(m)),
                    (3, None),
                    (4, Some(m)),
                    (5, Some(a)),
                    (7, None),
                    (8, Some(d)),
                    (9, Some(m)),
                ],
                _ => vec![
                    (0, None),
                    (1, Some(m)),
                    (3, Some(m)),
                    (4, Some(a)),
                    (6, None),
                    (7, Some(d)),
                    (8, Some(m)),
                ],
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
            if scene != "files" {
                continue;
            }
            // M11i: each row's icon in its own colour, in its 12 pt box: folders dim, files
            // tinted by type (the names keep their marks' colours).
            let (red, cyan) = (
                kind_color(FileKind::Red, &theme),
                kind_color(FileKind::Cyan, &theme),
            );
            let icon = |row: usize, c| ink_in(row, icon_left(row), icon_left(row) + 12.0, c);
            for (row, what, c) in [
                (0, "docs", ui.text_dim),
                (1, "src (open)", ui.text_dim),
                (2, "util (open)", ui.text_dim),
                (3, "a.rs", red),
                (4, "main.rs", red),
                (6, "Cargo.toml", cyan),
                (7, "notes.txt", ui.text_dim),
                (9, "README.md", ui.text_dim),
            ] {
                assert!(icon(row, c), "{n}: {what}'s icon in {c:?}");
            }
            for (row, what) in [(6, "Cargo.toml"), (7, "notes.txt"), (9, "README.md")] {
                assert!(!icon(row, red), "{n}: {what}'s icon isn't red");
            }
            assert!(!icon(4, cyan), "{n}: main.rs's icon isn't cyan");
            // An open folder's icon differs from a closed one's: its flap's top edge crosses
            // the middle of the 12 pt box (5 pt down the row), where a closed folder is empty.
            let open_flap = |row: usize| {
                let (x, y) = (
                    icon_left(row) + 6.0,
                    list_top + row as f32 * ROW + 5.0 + 5.5,
                );
                near(at(x, y), ui.text_dim, 40)
            };
            assert!(open_flap(1) && !open_flap(0), "{n}: src open, docs closed");
            // Indent guides: 1 pt in `guide` at each level above the row, at the middle of
            // that level's chevron column (8 + 14 d + 6), the row's full height; none at the
            // top level. (It spans x - 0.5 to x + 0.5: on a 1x shot it lands on one side.)
            let guide_at = |row: usize, x: f32| {
                let y0 = list_top + row as f32 * ROW;
                [x - 1.0, x].iter().any(|&x| {
                    [0.5, ROW / 2.0, ROW - 0.5]
                        .iter()
                        .all(|dy| near(at(sidebar_left + x, y0 + dy), ui.guide, 3))
                })
            };
            for row in [2, 3, 4, 5] {
                assert!(guide_at(row, 14.0), "{n}: row {row}'s depth-1 guide");
            }
            assert!(guide_at(3, 28.0), "{n}: a.rs's depth-2 guide");
            assert!(!guide_at(4, 28.0), "{n}: main.rs is at depth 1");
            for row in [6, 7, 9] {
                assert!(
                    !guide_at(row, 14.0),
                    "{n}: no guide at the top level ({row})"
                );
            }
        }

        // M11i: the root header; collapse all, then the root folded away.
        for scene in ["files-header", "files-header-collapsed"] {
            let fold = scene == "files-header-collapsed";
            let slot: Slot = Rc::new(RefCell::new(None));
            let build = build_for(false, true, slot.clone());
            let header_top = card_top + TAB_BAR;
            // Line 1's middle: 3 pt (the header's room round its 34 pt of lines) + 10.
            let line_one = header_top + (ROOT_HEADER - 34.0) / 2.0 + 10.0;
            let button = point(
                px(window_right - GAP - FOCUS_RING - 6.0 - 10.0),
                px(line_one),
            );
            let name = point(px(sidebar_left + 60.0), px(line_one));
            let root_for = root.clone();
            let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let ws = slotted(&slot);
                let n = format!("{scene}-{slug}");
                let none = Modifiers::none();
                frame(cx, window);
                let state = |cx: &mut VisualTestAppContext| {
                    cx.read(|cx| {
                        let tree = ws.read(cx).right_sidebar.files.tree.as_ref();
                        let tree = tree.expect("the tree");
                        let selected = tree.selected().map(|s| s.display().to_string());
                        (tree.expanded_set(), selected)
                    })
                };
                // The path line, as drawn at this width: its start cut, its own name kept.
                let drawn = cx
                    .update_window(window, |_, window, cx| {
                        let ws = ws.read(cx);
                        let room = ws.right_sidebar.width - 2.0 * FOCUS_RING - 10.0 - 6.0;
                        let wrapper = RefCell::new(window.text_system().line_wrapper(
                            gpui::TextStyle::default().font(),
                            px(crate::ui_tokens::text::META),
                        ));
                        let fits = |s: &str| {
                            wrapper
                                .borrow_mut()
                                .should_truncate_line(
                                    s,
                                    px(room),
                                    "\u{2026}",
                                    gpui::TruncateFrom::End,
                                )
                                .is_none()
                        };
                        root_path_line(&root_for, ws.places.home.as_deref(), fits)
                    })
                    .expect("read");
                assert!(
                    drawn.starts_with("\u{2026}/") && drawn.ends_with("/fuxx-ui-shots-files"),
                    "{n}: the path's start is cut at 300 pt, at a folder boundary: {drawn}"
                );
                cx.simulate_click(window, button, none);
                frame(cx, window);
                assert_eq!(
                    rows_of(cx, &ws),
                    [
                        "docs",
                        "src",
                        "Cargo.toml",
                        "notes.txt",
                        "old.rs",
                        "README.md"
                    ],
                    "{n}: collapse all closes every folder"
                );
                let (memory, selected) = state(cx);
                assert!(memory.expanded.is_empty(), "{n}: none open: {memory:?}");
                assert!(!memory.root_collapsed, "{n}: the root stays open");
                assert_eq!(selected.as_deref(), Some("src"), "{n}: the selection");
                if fold {
                    cx.simulate_click(window, name, none);
                    frame(cx, window);
                    assert!(rows_of(cx, &ws).is_empty(), "{n}: the tree is hidden");
                    assert!(state(cx).0.root_collapsed, "{n}: folded away");
                    // A click again: back, its folders as they were (all closed).
                    cx.simulate_click(window, name, none);
                    frame(cx, window);
                    assert_eq!(rows_of(cx, &ws).len(), 6, "{n}: back");
                    cx.simulate_click(window, name, none);
                    frame(cx, window);
                    assert!(rows_of(cx, &ws).is_empty(), "{n}: and away again");
                }
                // The pointer away over the pane: no hover or tooltip in the shot.
                cx.simulate_mouse_move(window, point(px(500.0), px(300.0)), None, none);
                frame(cx, window);
                slot.borrow_mut().take();
            };
            let path = shoot_with(scene, slug, &theme, dims, build.clone(), act.clone());
            println!("ui-shots: {}", path.display());
            let shot = render_with(&theme, dims, build, act);
            let n = format!("{scene}-{slug}");
            let at = |x: f32, y: f32| pixel_at(&shot, dims, x, y);
            let has = |x0: f32, x1: f32, y0: f32, y1: f32, c, by| {
                let (w, h) = (((x1 - x0) * 2.0) as usize, ((y1 - y0) * 2.0) as usize);
                (0..h).any(|dy| {
                    (0..w).any(|dx| near(at(x0 + dx as f32 * 0.5, y0 + dy as f32 * 0.5), c, by))
                })
            };
            let line_top = header_top + (ROOT_HEADER - 34.0) / 2.0;
            let (left, right) = (sidebar_left + 10.0, window_right - GAP - FOCUS_RING - 6.0);
            // Line 1: the name in the text colour; line 2: the path, faint, inside the pads.
            assert!(
                has(
                    left + 10.0,
                    left + 120.0,
                    line_top,
                    line_top + 20.0,
                    ui.text,
                    24
                ),
                "{n}: the root's name"
            );
            assert!(
                has(
                    left,
                    left + 200.0,
                    line_top + 20.0,
                    line_top + 34.0,
                    ui.text_faint,
                    24
                ),
                "{n}: the root's path"
            );
            assert!(
                !has(
                    left,
                    left + 200.0,
                    line_top + 20.0,
                    line_top + 34.0,
                    ui.text,
                    12
                ),
                "{n}: the path is faint, not text"
            );
            // Collapse all's two chevrons, dim, in its 20 pt box at the right of line 1.
            assert!(
                has(
                    right - 20.0,
                    right,
                    line_top,
                    line_top + 20.0,
                    ui.text_dim,
                    24
                ),
                "{n}: the collapse all button"
            );
            let filter_middle = header_top + ROOT_HEADER + FILTER_BAND / 2.0;
            let filter_box = (0..548)
                .map(|i| sidebar_left + 6.0 + i as f32 * 0.5)
                .any(|x| !near(at(x, filter_middle), ui.card, 3));
            if fold {
                // Only the header: no filter box, no rows under it.
                assert!(!filter_box, "{n}: no filter box");
                let rows_top = header_top + ROOT_HEADER;
                assert!(
                    !has(
                        sidebar_left + 2.0,
                        right,
                        rows_top,
                        rows_top + 120.0,
                        ui.text,
                        24
                    ),
                    "{n}: no rows"
                );
            } else {
                assert!(filter_box, "{n}: the filter box under the header");
            }
        }

        // Keys, a click, Esc (not shot).
        let slot: Slot = Rc::new(RefCell::new(None));
        let build = build_for(false, false, slot.clone());
        let root_for = root.clone();
        let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
            let ws = slotted(&slot);
            let n = format!("files-keys-{slug}");
            let none = Modifiers::none();
            frame(cx, window);
            let list_top = card_top + TAB_BAR + ROOT_HEADER + FILTER_BAND;
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
            // A file made in src while it was closed (by an agent, say): Return opens src from
            // its kept listing and lists it again, so the file shows (Task 6 review).
            let made = root_for.join("src/zz.rs");
            std::fs::write(&made, "\n").expect("a file made while src was closed");
            cx.simulate_keystrokes(window, "enter");
            frame(cx, window);
            let mut with_made = tree_rows.to_vec();
            with_made.insert(5, "src/zz.rs");
            let rows = rows_of(cx, &ws);
            std::fs::remove_file(&made).ok();
            assert_eq!(rows, with_made, "{n}: and opens it, listed again");
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
        let build = build_for(false, false, slot.clone());
        let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
            let ws = slotted(&slot);
            let n = format!("files-close-{slug}");
            frame(cx, window);
            let field = point(
                px(sidebar_left + 60.0),
                px(card_top + TAB_BAR + ROOT_HEADER + FILTER_BAND / 2.0),
            );
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

/// A workspace for the right sidebar's panel scenes (M11h): the Files panel's fixture beside a
/// terminal (`api`) in its src/, the right sidebar open `width` wide showing `panel`, the Files
/// tree made as in `files_scenes`; `ready` then sets up anything else.
fn right_panel_workspace(
    theme: Theme,
    root: PathBuf,
    width: f32,
    panel: crate::right_sidebar::Panel,
    slot: Slot,
    ready: impl Fn(&mut Workspace) + Clone + 'static,
) -> impl FnOnce(&mut Window, &mut App) -> AnyView + Clone + 'static {
    move |window: &mut Window, cx: &mut App| -> AnyView {
        cx.set_global(shot_appearance(theme, cx));
        cx.bind_keys(crate::key_bindings());
        let ws = cx.new(|cx| {
            let mut ws = Workspace::for_shots(window, cx);
            ws.sidebar.hint_dismissed = true;
            ws.right_sidebar.hidden = false;
            ws.right_sidebar.width = width;
            ws.right_sidebar.panel = panel;
            let src = root.join("src");
            ws.open_shot_session("api", &src, SHELL_TEXT, None, window, cx);
            ws.right_sidebar
                .files
                .show_for_shots(files_tree(&root, false));
            ready(&mut ws);
            ws
        });
        *slot.borrow_mut() = Some(ws.clone());
        ws.into()
    }
}

/// The right sidebar's panel tabs (M11h Task 4) on the Files panel's fixture, in a 1100 × 560
/// window, in both themes:
/// - `right-panels`: Files shown: its tab filled with the card (merging into the panel), the
///   Changes tab unfilled on the strip's `tab_bar`, the hovered Activity tab `tab_hover`; the
///   tabs 6 pt from the strip's left, 2 pt apart, 4 pt under its top and flush with its
///   bottom, in Files · Changes · Activity order; no border under the strip; its empty part
///   `tab_bar`. Then (not shot) the filter box focused and a click on Changes: Changes shown,
///   saved as `changes`, the terminal has the keyboard back (not the Files panel), and now the
///   Changes tab is the filled one.
/// - `right-panels-narrow`: the sidebar at its narrowest (220 pt): the three tabs fit, the
///   Activity tab's right edge at least 6 pt inside the card's.
fn right_panel_scenes() {
    use crate::right_sidebar::{FILTER_BAND, Panel, ROOT_HEADER};
    use crate::ui_tokens::size::{FOCUS_RING, TAB_BAR, TAB_TOP_GAP};
    use gpui::{Bounds, Focusable, Modifiers};
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    let dims = size(px(1100.0), px(560.0));
    let window_right = 1100.0_f32;
    let card_right = window_right - GAP;
    let card_top = GAP + FOCUS_RING;
    let root = files_fixture();
    type Tabs = Rc<Cell<[Option<Bounds<Pixels>>; 3]>>;
    let tabs_of = |cx: &mut VisualTestAppContext, ws: &gpui::Entity<Workspace>| {
        let all = cx.read(|cx| ws.read(cx).right_sidebar.panel_tab_bounds());
        all.map(|b| b.expect("each tab laid out"))
    };
    let frame = |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
        cx.update_window(window, |_, window, _| window.refresh())
            .expect("refresh");
        cx.run_until_parked();
    };
    for (slug, name) in SHOT_THEMES {
        let theme = shot_theme(name);
        let ui = theme.ui();
        let near = |p: [u8; 3], c: alacritty_terminal::vte::ansi::Rgb| {
            p.iter()
                .zip([c.r, c.g, c.b])
                .all(|(a, b)| a.abs_diff(b) <= 3)
        };
        assert_ne!(ui.tab_bar, ui.card, "{slug}: told apart");
        assert_ne!(ui.tab_hover, ui.tab_bar, "{slug}: told apart");

        for scene in ["right-panels", "right-panels-narrow"] {
            let narrow = scene == "right-panels-narrow";
            let width = if narrow { 220.0 } else { 300.0 };
            let sidebar_left = card_right - width + FOCUS_RING;
            let n = format!("{scene}-{slug}");
            let slot: Slot = Rc::new(RefCell::new(None));
            let build = right_panel_workspace(
                theme,
                root.clone(),
                width,
                Panel::Files,
                slot.clone(),
                |_| (),
            );
            let seen: Tabs = Rc::default();
            let act = {
                let (n, seen) = (n.clone(), seen.clone());
                move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                    let ws = slotted(&slot);
                    frame(cx, window);
                    let [files, changes, activity] = tabs_of(cx, &ws);
                    let (l, r, t, b) = (
                        |b: Bounds<Pixels>| f32::from(b.left()),
                        |b: Bounds<Pixels>| f32::from(b.right()),
                        |b: Bounds<Pixels>| f32::from(b.top()),
                        |b: Bounds<Pixels>| f32::from(b.bottom()),
                    );
                    let close = |a: f32, b: f32| (a - b).abs() < 0.5;
                    assert!(
                        close(l(files), sidebar_left + 6.0),
                        "{n}: Files 6 pt from the strip's left: {files:?}"
                    );
                    assert!(close(l(changes), r(files) + 2.0), "{n}: 2 pt apart");
                    assert!(close(l(activity), r(changes) + 2.0), "{n}: 2 pt apart");
                    for tab in [files, changes, activity] {
                        assert!(close(t(tab), card_top + TAB_TOP_GAP), "{n}: 4 pt under");
                        assert!(close(b(tab), card_top + TAB_BAR), "{n}: flush: {tab:?}");
                    }
                    assert!(
                        r(activity) <= card_right - 6.0,
                        "{n}: Activity ends at {}, past the card's right − 6",
                        r(activity)
                    );
                    seen.set([Some(files), Some(changes), Some(activity)]);
                    // The pointer on the Activity tab: its hover fill in the shot.
                    let on = point(px(l(activity) + 4.0), px(card_top + TAB_BAR - 4.0));
                    cx.simulate_mouse_move(window, on, None, Modifiers::none());
                    frame(cx, window);
                    slot.borrow_mut().take();
                }
            };
            let path = shoot_with(scene, slug, &theme, dims, build.clone(), act.clone());
            println!("ui-shots: {}", path.display());
            let shot = render_with(&theme, dims, build, act);
            let at = |x: f32, y: f32| pixel_at(&shot, dims, x, y);
            let [files, changes, activity] = seen.get().map(|b| b.expect("seen"));
            // Inside each tab's left padding (no words there), low in it.
            let pad = |b: Bounds<Pixels>| at(f32::from(b.left()) + 4.0, card_top + TAB_BAR - 6.0);
            assert!(
                near(pad(files), ui.card),
                "{n}: Files is shown: {:?}",
                pad(files)
            );
            assert!(
                near(pad(changes), ui.tab_bar),
                "{n}: Changes isn't: {:?}",
                pad(changes)
            );
            assert!(
                near(pad(activity), ui.tab_hover),
                "{n}: Activity hovered: {:?}",
                pad(activity)
            );
            // The strip: above the tabs and its empty part in `tab_bar`; no border under it
            // (its last row `tab_bar` beside the tabs, the card right under it).
            let empty = card_right - 10.0;
            let x_files = f32::from(files.left()) + 4.0;
            for (what, x, y) in [
                ("above the tabs", x_files, card_top + 2.0),
                ("its empty part", empty, card_top + TAB_BAR / 2.0),
                ("its last row", empty, card_top + TAB_BAR - 0.5),
            ] {
                let p = at(x, y);
                assert!(near(p, ui.tab_bar), "{n}: the strip {what}: {p:?}");
            }
            let p = at(empty, card_top + TAB_BAR + 0.5);
            assert!(near(p, ui.card), "{n}: the card right under it: {p:?}");
            let p = at(x_files, card_top + TAB_BAR - 0.5);
            assert!(near(p, ui.card), "{n}: Files merges into the panel: {p:?}");
        }

        // The filter box focused, a click on Changes (not shot).
        let n = format!("right-panels-click-{slug}");
        let sidebar_left = card_right - 300.0 + FOCUS_RING;
        let slot: Slot = Rc::new(RefCell::new(None));
        let build = right_panel_workspace(
            theme,
            root.clone(),
            300.0,
            Panel::Files,
            slot.clone(),
            |_| (),
        );
        let seen: Tabs = Rc::default();
        let act = {
            let (n, seen) = (n.clone(), seen.clone());
            move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let ws = slotted(&slot);
                frame(cx, window);
                let field = point(
                    px(sidebar_left + 60.0),
                    px(card_top + TAB_BAR + ROOT_HEADER + FILTER_BAND / 2.0),
                );
                cx.simulate_click(window, field, Modifiers::none());
                let keys = |cx: &mut VisualTestAppContext| {
                    cx.update_window(window, |_, window, cx| {
                        let ws = ws.read(cx);
                        let files = ws.right_sidebar.files.focus.clone().expect("its focus");
                        let terminal = ws.sessions.active().expect("the terminal");
                        (
                            files.contains_focused(window, cx),
                            terminal.view.focus_handle(cx).is_focused(window),
                        )
                    })
                    .expect("read")
                };
                assert_eq!(
                    keys(cx),
                    (true, false),
                    "{n}: the filter box has the keyboard"
                );
                let [_, changes, _] = tabs_of(cx, &ws);
                cx.simulate_click(window, changes.center(), Modifiers::none());
                frame(cx, window);
                cx.read(|cx| {
                    let ws = ws.read(cx);
                    assert_eq!(ws.right_sidebar.panel, Panel::Changes, "{n}: shown");
                    assert_eq!(
                        ws.saved_state().right_sidebar_panel.as_deref(),
                        Some("changes"),
                        "{n}: saved"
                    );
                });
                assert_eq!(
                    keys(cx),
                    (false, true),
                    "{n}: the terminal has the keyboard back"
                );
                seen.set(tabs_of(cx, &ws).map(Some));
                slot.borrow_mut().take();
            }
        };
        let shot = render_with(&theme, dims, build, act);
        let [files, changes, _] = seen.get().map(|b| b.expect("seen"));
        let pad = |b: Bounds<Pixels>| {
            pixel_at(
                &shot,
                dims,
                f32::from(b.left()) + 4.0,
                card_top + TAB_BAR - 6.0,
            )
        };
        assert!(near(pad(changes), ui.card), "{n}: Changes is shown now");
        assert!(near(pad(files), ui.tab_bar), "{n}: Files isn't");
        println!("ui-shots: {n} ok");
    }
}

/// The Changes panel (M11h Task 4) in the right sidebar, beside a terminal, in a 1100 × 560
/// window, in both themes, its list given (`show_for_shots`; nothing looked up):
/// - `changes`: six rows and 3 more for the fixture folder as a repo: src/main.rs (M, +12 −3),
///   src/new.rs (A, +4), old.rs (D, −9), docs/logo.png (A, binary), a file four folders down
///   (M, +2 −2) and a long name (A, not counted), then `… 3 more`. The header reads the repo's
///   name, `9 files` and the totals (+18 −14); each letter in its mark's colour (M blue, A
///   green, D red); `+12` in the added ink and `−3` in the removed one, a binary or uncounted
///   row in neither; the hovered row (the fifth) `card_hover`, the others the card.
/// - `changes-empty-clean`: an empty list in a repo: the header (name only), then
///   "No changes since the last commit."; `changes-empty-norepo`: not a repo: "Not a git
///   repository." and no header; `changes-empty-ssh`: an SSH terminal followed: "Changes
///   aren't shown for SSH terminals.", no header.
/// - `changes-click`: a click on the M row opens src/main.rs in the focused pane in Diff mode
///   (the model's tab), and that row is drawn `card_selected`.
fn changes_scenes() {
    use crate::changes_panel::{NO_CHANGES, NO_REPO, SSH_NOTE, file_count_label, totals};
    use crate::git_status::{Change, ChangeList, Counts, Mark};
    use crate::layout_model::{FileMode, TabKind};
    use crate::right_sidebar::{Panel, ROOT_HEADER, ROW, mark_color};
    use crate::ui_tokens::size::{FOCUS_RING, TAB_BAR};
    use alacritty_terminal::vte::ansi::Rgb;
    use gpui::{Focusable, Modifiers};
    use std::cell::RefCell;
    use std::rc::Rc;

    let dims = size(px(1100.0), px(560.0));
    let window_right = 1100.0_f32;
    let sidebar_left = window_right - GAP - 300.0 + FOCUS_RING;
    let sidebar_right = window_right - GAP - FOCUS_RING;
    let card_top = GAP + FOCUS_RING;
    let header_top = card_top + TAB_BAR;
    let list_top = header_top + ROOT_HEADER;
    let root = files_fixture();
    let lines = |added, removed| Counts::Lines { added, removed };
    let change = |path: &str, mark, counts| Change {
        path: PathBuf::from(path),
        mark,
        counts,
        folder: false,
    };
    let six = ChangeList {
        entries: vec![
            change("src/main.rs", Mark::Modified, lines(12, 3)),
            change("src/new.rs", Mark::Added, lines(4, 0)),
            change("old.rs", Mark::Deleted, lines(0, 9)),
            change("docs/logo.png", Mark::Added, Counts::Binary),
            change("src/ui/widgets/deep/panel.rs", Mark::Modified, lines(2, 2)),
            change(
                "a_very_long_file_name_that_goes_on_well_past_the_sidebar_edge.rs",
                Mark::Added,
                Counts::Unknown,
            ),
        ],
        more: 3,
        counts_failed: false,
    };
    let frame = |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
        cx.update_window(window, |_, window, _| window.refresh())
            .expect("refresh");
        cx.run_until_parked();
    };
    let row_mid = |i: usize| list_top + (i as f32 + 0.5) * ROW;
    for (slug, name) in SHOT_THEMES {
        let theme = shot_theme(name);
        let ui = theme.ui();
        let (added, removed) = crate::changes_panel::count_inks(&theme);
        let (m, a, d) = (
            mark_color(Mark::Modified, &theme),
            mark_color(Mark::Added, &theme),
            mark_color(Mark::Deleted, &theme),
        );
        let near = |p: [u8; 3], c: Rgb, by: u8| {
            p.iter()
                .zip([c.r, c.g, c.b])
                .all(|(x, y)| x.abs_diff(y) <= by)
        };
        let build_for = |list: Option<ChangeList>, in_repo: bool, ssh: bool, slot: Slot| {
            let root_for = root.clone();
            right_panel_workspace(
                theme,
                root.clone(),
                300.0,
                Panel::Changes,
                slot,
                move |ws| {
                    ws.right_sidebar.changes.show_for_shots(
                        root_for.clone(),
                        in_repo,
                        list.clone(),
                    );
                    if ssh {
                        let i = ws.sessions.position(|s| s.label() == "api").expect("api");
                        ws.sessions.get_mut(i).expect("opened").ssh =
                            Some(crate::ssh_target::SshTarget {
                                host: "devbox".into(),
                                tmux: false,
                                tmux_name: None,
                            });
                    }
                },
            )
        };

        // The list.
        let n = format!("changes-{slug}");
        let slot: Slot = Rc::new(RefCell::new(None));
        let build = build_for(Some(six.clone()), true, false, slot.clone());
        let act = {
            let n = n.clone();
            move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let ws = slotted(&slot);
                frame(cx, window);
                let shown = cx.read(|cx| ws.read(cx).changes_note(cx));
                assert_eq!(shown, None, "{n}: rows, no note");
                // The pointer on the fifth row: its hover fill in the shot.
                let on = point(px(sidebar_left + 150.0), px(row_mid(4)));
                cx.simulate_mouse_move(window, on, None, Modifiers::none());
                frame(cx, window);
                slot.borrow_mut().take();
            }
        };
        let path = shoot_with("changes", slug, &theme, dims, build.clone(), act.clone());
        println!("ui-shots: {}", path.display());
        let shot = render_with(&theme, dims, build, act);
        let at = |x: f32, y: f32| pixel_at(&shot, dims, x, y);
        let has = |x0: f32, x1: f32, y0: f32, y1: f32, c: Rgb, by: u8| {
            let (w, h) = (((x1 - x0) * 2.0) as usize, ((y1 - y0) * 2.0) as usize);
            (0..h).any(|dy| {
                (0..w).any(|dx| near(at(x0 + dx as f32 * 0.5, y0 + dy as f32 * 0.5), c, by))
            })
        };
        let in_row = |i: usize, x0: f32, x1: f32, c: Rgb| {
            let y0 = list_top + i as f32 * ROW;
            has(x0, x1, y0 + 2.0, y0 + ROW - 2.0, c, 24)
        };
        // The header: the repo's name, `9 files` at the right, the totals under them.
        assert_eq!(file_count_label(six.entries.len() + six.more), "9 files");
        assert_eq!(totals(&six), (18, 14), "{n}: the totals");
        let line_top = header_top + (ROOT_HEADER - 34.0) / 2.0;
        let (left, right) = (sidebar_left + 10.0, sidebar_right - 6.0);
        assert!(
            has(left, left + 120.0, line_top, line_top + 20.0, ui.text, 24),
            "{n}: the repo's name"
        );
        assert!(
            has(
                right - 45.0,
                right,
                line_top,
                line_top + 20.0,
                ui.text_faint,
                24
            ),
            "{n}: the file count, faint, at the right"
        );
        assert!(
            !has(right - 45.0, right, line_top, line_top + 20.0, ui.text, 12),
            "{n}: the file count isn't in the text colour"
        );
        for (what, c) in [("added", added), ("removed", removed)] {
            assert!(
                has(left, left + 80.0, line_top + 20.0, line_top + 34.0, c, 24),
                "{n}: the {what} total"
            );
        }
        // Each row's letter in its mark's colour, and in no other mark's.
        let letter = |i: usize, c: Rgb| in_row(i, sidebar_left + 8.0, sidebar_left + 20.0, c);
        for (i, mark) in [(0, m), (1, a), (2, d), (3, a), (4, m), (5, a)] {
            assert!(letter(i, mark), "{n}: row {i}'s letter in {mark:?}");
            for other in [m, a, d].into_iter().filter(|o| *o != mark) {
                assert!(!letter(i, other), "{n}: row {i}'s letter isn't {other:?}");
            }
        }
        // The counts at the right: `+12 −3` in both inks; `+4` added only; `−9` removed
        // only; `binary` faint in neither; an uncounted row has none.
        let counts =
            |i: usize, c: Rgb| in_row(i, sidebar_right - 8.0 - 60.0, sidebar_right - 8.0, c);
        for (i, want_added, want_removed) in [
            (0, true, true),
            (1, true, false),
            (2, false, true),
            (3, false, false),
            (5, false, false),
        ] {
            assert_eq!(counts(i, added), want_added, "{n}: row {i}'s added count");
            assert_eq!(
                counts(i, removed),
                want_removed,
                "{n}: row {i}'s removed count"
            );
        }
        assert!(counts(3, ui.text_faint), "{n}: `binary`, faint");
        // `… 3 more`, faint, under the names, last.
        let more = |i: usize, c: Rgb| in_row(i, sidebar_left + 26.0, sidebar_left + 120.0, c);
        assert!(more(6, ui.text_faint), "{n}: the more row");
        assert!(!more(7, ui.text_faint), "{n}: nothing after it");
        // The hovered row filled `card_hover`, the one above it the card.
        let fill = |i: usize| at(sidebar_left + 3.0, row_mid(i));
        assert!(
            near(fill(4), ui.card_hover, 3),
            "{n}: hovered {:?}",
            fill(4)
        );
        assert!(near(fill(3), ui.card, 3), "{n}: not hovered {:?}", fill(3));

        // The empty states.
        for (case, list, in_repo, ssh, note) in [
            (
                "clean",
                Some(ChangeList::default()),
                true,
                false,
                NO_CHANGES,
            ),
            ("norepo", None, false, false, NO_REPO),
            ("ssh", Some(six.clone()), true, true, SSH_NOTE),
        ] {
            let scene = format!("changes-empty-{case}");
            let n = format!("{scene}-{slug}");
            let slot: Slot = Rc::new(RefCell::new(None));
            let build = build_for(list, in_repo, ssh, slot.clone());
            let act = {
                let n = n.clone();
                move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                    let ws = slotted(&slot);
                    frame(cx, window);
                    let shown = cx.read(|cx| ws.read(cx).changes_note(cx));
                    assert_eq!(shown, Some(note), "{n}");
                    slot.borrow_mut().take();
                }
            };
            let path = shoot_with(&scene, slug, &theme, dims, build.clone(), act.clone());
            println!("ui-shots: {}", path.display());
            let shot = render_with(&theme, dims, build, act);
            let at = |x: f32, y: f32| pixel_at(&shot, dims, x, y);
            // Tight: Latte's dim and full text colours are close.
            let has = |y0: f32, y1: f32, c: Rgb| {
                (0..((y1 - y0) * 2.0) as usize).any(|dy| {
                    (0..400).any(|dx| {
                        let (x, y) = (sidebar_left + 2.0 + dx as f32 * 0.5, y0 + dy as f32 * 0.5);
                        near(at(x, y), c, 6)
                    })
                })
            };
            // The header (in a repo only), then the note's words, dim.
            let header = case == "clean";
            assert_eq!(
                has(header_top, header_top + 30.0, ui.text),
                header,
                "{n}: the header {}",
                if header { "shows" } else { "doesn't" }
            );
            let note_top = if header { list_top } else { header_top };
            assert!(
                has(note_top, note_top + 30.0, ui.text_dim),
                "{n}: the note's words"
            );
            // No rows: no mark letters.
            for c in [m, a, d] {
                assert!(
                    !has(note_top + 30.0, note_top + 120.0, c),
                    "{n}: no rows ({c:?})"
                );
            }
        }

        // A click on the M row: its diff, selected.
        let n = format!("changes-click-{slug}");
        let slot: Slot = Rc::new(RefCell::new(None));
        let build = build_for(Some(six.clone()), true, false, slot.clone());
        let act = {
            let (n, root) = (n.clone(), root.clone());
            move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let ws = slotted(&slot);
                frame(cx, window);
                let on = point(px(sidebar_left + 100.0), px(row_mid(0)));
                cx.simulate_click(window, on, Modifiers::none());
                frame(cx, window);
                cx.read(|cx| {
                    let ws = ws.read(cx);
                    let tab = ws.model.focused_tab().expect("a tab");
                    let kind = ws.model.tab(tab).expect("known").kind.clone();
                    assert_eq!(
                        kind,
                        TabKind::File {
                            path: root.join("src/main.rs"),
                            mode: FileMode::Diff
                        },
                        "{n}: the click opens src/main.rs's diff"
                    );
                    let view = ws.focused_file_tab().expect("its view");
                    assert_eq!(view.read(cx).mode(), FileMode::Diff, "{n}: the tab's mode");
                });
                let keys = cx
                    .update_window(window, |_, window, cx| {
                        let view = ws.read(cx).focused_file_tab().expect("its view");
                        view.focus_handle(cx).is_focused(window)
                    })
                    .expect("read");
                assert!(keys, "{n}: the diff tab has the keyboard");
                // The pointer away: no hover or tooltip in the shot.
                cx.simulate_mouse_move(
                    window,
                    point(px(500.0), px(300.0)),
                    None,
                    Modifiers::none(),
                );
                frame(cx, window);
                slot.borrow_mut().take();
            }
        };
        let path = shoot_with(
            "changes-click",
            slug,
            &theme,
            dims,
            build.clone(),
            act.clone(),
        );
        println!("ui-shots: {}", path.display());
        let shot = render_with(&theme, dims, build, act);
        let fill = |i: usize| pixel_at(&shot, dims, sidebar_left + 3.0, row_mid(i));
        assert!(
            near(fill(0), ui.card_selected, 3),
            "{n}: the M row is selected: {:?}",
            fill(0)
        );
        assert!(
            near(fill(1), ui.card, 3),
            "{n}: the next isn't: {:?}",
            fill(1)
        );
    }
}

/// A screen asking a question one line long on the terminal, wrapped in the Activity card.
const ASKING_TEXT: &str =
    "~/code/api $ ./deploy\r\nRun the database migration on production now? (y/n) ";

/// The Activity panel's window (M11h Task 6): the right sidebar open on Activity, 300 pt wide,
/// with terminals `sessions` (name, screen; none: no terminal to follow), the first focused;
/// Reduce motion as `still`.
fn activity_workspace(
    theme: Theme,
    sessions: &'static [(&'static str, &'static str)],
    still: bool,
    slot: Slot,
) -> impl FnOnce(&mut Window, &mut App) -> AnyView + Clone + 'static {
    move |window: &mut Window, cx: &mut App| -> AnyView {
        cx.set_global(shot_appearance(theme, cx));
        cx.bind_keys(crate::key_bindings());
        cx.set_reduce_motion(still);
        let ws = cx.new(|cx| {
            let mut ws = Workspace::for_shots(window, cx);
            ws.sidebar.hint_dismissed = true;
            ws.right_sidebar.hidden = false;
            ws.right_sidebar.width = 300.0;
            ws.right_sidebar.panel = crate::right_sidebar::Panel::Activity;
            for (name, text) in sessions {
                let at = std::path::Path::new(API);
                ws.open_shot_session(name, at, text, None, window, cx);
            }
            if !sessions.is_empty() {
                ws.select(0, window, cx);
            }
            ws
        });
        *slot.borrow_mut() = Some(ws.clone());
        ws.into()
    }
}

/// The Activity panel (M11h Task 6) in the right sidebar, in a 1100 × 560 window, both themes:
/// - `activity`: api driven as the app drives it: Working, 11 min later Working again (no
///   change) and Done, the note `a` twice, progress 10 then 20 %. Its log: 2 status entries,
///   1 note, 1 progress; its rows newest first `Progress 20%`, `a`, `Done — 11 min` (how long
///   it worked: `status_since` aged before the change, so `lasted` is read before the reset),
///   `Working`; the header `api` and `Done · 4m`. In the shot: the rows' marks in that order
///   (two faint dots, Done's, Working's), nothing after them; the name in `text`, the status
///   words `text_dim`. Then (not shot) the focus to web: its empty log, `No activity yet.`.
/// - `activity-waiting`: api Waiting on a question long enough to wrap: the header's words
///   `Needs input · 2m` in `ring_waiting`; the card 6 pt in, filled `waiting_tint`, at least two
///   lines of question tall, `Needs input` in it `ring_waiting`, the question in `text`.
/// - `activity-empty-none`: a terminal with no entries: `No activity yet.` under its header;
///   `activity-empty-unfollowed`: no terminal: `No terminal to follow.` and no header. Then (not
///   shot) a busy log with Reduce motion on, and again with it off: two frames 200 ms apart are
///   the same in the panel (nothing in it moves).
fn activity_scenes() {
    use crate::activity::Kind;
    use crate::activity_panel::{NO_ACTIVITY, NO_TERMINAL, RowMark};
    use crate::right_sidebar::{ROOT_HEADER, ROW};
    use crate::ui_tokens::size::{FOCUS_RING, TAB_BAR};
    use alacritty_terminal::vte::ansi::Rgb;
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::time::{Duration, Instant};

    let dims = size(px(1100.0), px(560.0));
    let window_right = 1100.0_f32;
    let sidebar_left = window_right - GAP - 300.0 + FOCUS_RING;
    let sidebar_right = window_right - GAP - FOCUS_RING;
    let card_top = GAP + FOCUS_RING;
    let header_top = card_top + TAB_BAR;
    let list_top = header_top + ROOT_HEADER;
    // A row: 8 pt in, the 36 pt time column, the 10 pt mark, 6 pt, the words.
    let mark_x = sidebar_left + 8.0 + 36.0;
    let words_x = mark_x + 10.0 + 6.0;
    let frame = |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
        cx.update_window(window, |_, window, _| window.refresh())
            .expect("refresh");
        cx.run_until_parked();
    };
    let ago = |secs: u64| Instant::now() - Duration::from_secs(secs);
    for (slug, name) in SHOT_THEMES {
        let theme = shot_theme(name);
        let ui = theme.ui();
        let near = |p: [u8; 3], c: Rgb, by: u8| {
            p.iter()
                .zip([c.r, c.g, c.b])
                .all(|(x, y)| x.abs_diff(y) <= by)
        };
        // Whether any pixel in the box is `c` (within `by`).
        let has = |shot: &Shot, x0: f32, x1: f32, y0: f32, y1: f32, c: Rgb, by: u8| {
            let (w, h) = (((x1 - x0) * 2.0) as usize, ((y1 - y0) * 2.0) as usize);
            (0..h).any(|dy| {
                (0..w).any(|dx| {
                    let p = pixel_at(shot, dims, x0 + dx as f32 * 0.5, y0 + dy as f32 * 0.5);
                    near(p, c, by)
                })
            })
        };
        let marks = [ui.ring_working, ui.ring_waiting, ui.ring_done];

        // The timeline.
        let n = format!("activity-{slug}");
        let slot: Slot = Rc::new(RefCell::new(None));
        let build = activity_workspace(
            theme,
            &[("api", SHELL_TEXT), ("web", SHELL_TEXT)],
            true,
            slot.clone(),
        );
        let act = {
            let (n, slot) = (n.clone(), slot.clone());
            move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let ws = slotted(&slot);
                ws.update(cx, |ws, cx| {
                    let i = ws.sessions.position(|s| s.label() == "api").expect("api");
                    ws.set_status(i, Status::Working, false, cx);
                    // It works 11 min: aged before the next change (read as `lasted`).
                    ws.sessions.get_mut(i).expect("api").status_since = ago(11 * 60 + 5);
                    ws.set_status(i, Status::Working, false, cx);
                    ws.set_status(i, Status::Done, false, cx);
                    ws.set_note(i, Some("a".into()), cx);
                    ws.set_note(i, Some("a".into()), cx);
                    ws.set_progress(i, Some((10, None)), cx);
                    ws.set_progress(i, Some((20, None)), cx);
                    let s = ws.sessions.get_mut(i).expect("api");
                    s.status_since = ago(4 * 60 + 5);
                    s.pop = None;
                });
                frame(cx, window);
                cx.read(|cx| {
                    let ws = ws.read(cx);
                    let api = &ws.sessions.items()[0];
                    let kinds: Vec<_> = api.activity.newest_first().map(|e| &e.kind).collect();
                    let count = |f: fn(&Kind) -> bool| kinds.iter().filter(|k| f(k)).count();
                    assert_eq!(count(|k| matches!(k, Kind::Status { .. })), 2, "{n}");
                    assert_eq!(count(|k| matches!(k, Kind::Note(_))), 1, "{n}");
                    assert_eq!(count(|k| matches!(k, Kind::Progress(..))), 1, "{n}");
                    let rows = ws.activity_rows();
                    let words: Vec<_> = rows.iter().map(|r| r.text.as_str()).collect();
                    assert_eq!(
                        words,
                        ["Progress 20%", "a", "Done \u{2014} 11 min", "Working"],
                        "{n}: newest first; Done says how long it worked"
                    );
                    let marks: Vec<_> = rows.iter().map(|r| r.mark).collect();
                    assert_eq!(
                        marks,
                        [
                            RowMark::Dot,
                            RowMark::Dot,
                            RowMark::Status(Status::Done),
                            RowMark::Status(Status::Working)
                        ],
                        "{n}"
                    );
                    let (label, words, waiting) = ws.activity_header().expect("followed");
                    assert_eq!(
                        (label.as_ref(), words.as_str(), waiting),
                        ("api", "Done \u{b7} 4m", false),
                        "{n}: the header"
                    );
                    assert_eq!(ws.activity_note(), None, "{n}: rows, no note");
                });
                slot.borrow_mut().take();
            }
        };
        let path = shoot_with("activity", slug, &theme, dims, build.clone(), act.clone());
        println!("ui-shots: {}", path.display());
        let shot = render_with(&theme, dims, build, act);
        let row_box = |i: usize| {
            (
                list_top + i as f32 * ROW + 2.0,
                list_top + (i + 1) as f32 * ROW - 2.0,
            )
        };
        let in_mark = |i: usize, c: Rgb| {
            let (y0, y1) = row_box(i);
            has(&shot, mark_x - 1.0, mark_x + 11.0, y0, y1, c, 24)
        };
        for i in [0, 1] {
            assert!(in_mark(i, ui.text_faint), "{n}: row {i}'s dot");
            for c in marks {
                assert!(!in_mark(i, c), "{n}: row {i} has no status mark ({c:?})");
            }
        }
        assert!(in_mark(2, ui.ring_done), "{n}: row 2 is Done's");
        assert!(!in_mark(2, ui.ring_working), "{n}: row 2 isn't Working's");
        assert!(in_mark(3, ui.ring_working), "{n}: row 3 is Working's");
        assert!(!in_mark(3, ui.ring_done), "{n}: row 3 isn't Done's");
        let (y0, y1) = row_box(4);
        assert!(
            !has(
                &shot,
                sidebar_left + 4.0,
                sidebar_right - 4.0,
                y0,
                y1,
                ui.text,
                24
            ),
            "{n}: nothing after the four rows"
        );
        // The times faint in their column, the words in `text`.
        let (y0, y1) = row_box(3);
        assert!(
            has(&shot, sidebar_left + 8.0, mark_x, y0, y1, ui.text_faint, 12),
            "{n}: a time"
        );
        assert!(
            has(&shot, words_x, words_x + 60.0, y0, y1, ui.text, 12),
            "{n}: the words"
        );
        // The header: the name in `text`, under it the status words `text_dim`.
        let line_top = header_top + (ROOT_HEADER - 34.0) / 2.0;
        let left = sidebar_left + 10.0;
        assert!(
            has(
                &shot,
                left,
                left + 60.0,
                line_top,
                line_top + 20.0,
                ui.text,
                12
            ),
            "{n}: the name"
        );
        assert!(
            has(
                &shot,
                left,
                left + 60.0,
                line_top + 20.0,
                line_top + 34.0,
                ui.text_dim,
                12
            ),
            "{n}: the status words"
        );
        assert!(
            !has(
                &shot,
                left,
                left + 60.0,
                line_top + 20.0,
                line_top + 34.0,
                ui.ring_waiting,
                12
            ),
            "{n}: not in the waiting colour"
        );

        // The focus to web (not shot): its own log, empty.
        let n = format!("activity-switch-{slug}");
        let slot: Slot = Rc::new(RefCell::new(None));
        let build = activity_workspace(
            theme,
            &[("api", SHELL_TEXT), ("web", SHELL_TEXT)],
            true,
            slot.clone(),
        );
        let act = {
            let (n, slot) = (n.clone(), slot.clone());
            move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let ws = slotted(&slot);
                ws.update(cx, |ws, cx| {
                    let i = ws.sessions.position(|s| s.label() == "api").expect("api");
                    ws.set_status(i, Status::Working, false, cx);
                    ws.set_note(i, Some("busy".into()), cx);
                });
                frame(cx, window);
                assert_eq!(
                    cx.read(|cx| ws.read(cx).activity_rows().len()),
                    2,
                    "{n}: api's"
                );
                cx.update_window(window, |_, window, cx| {
                    ws.update(cx, |ws, cx| {
                        let i = ws.sessions.position(|s| s.label() == "web").expect("web");
                        ws.select(i, window, cx);
                    })
                })
                .expect("focus web");
                frame(cx, window);
                cx.read(|cx| {
                    let ws = ws.read(cx);
                    let web = ws.sessions.items().iter().find(|s| s.label() == "web");
                    assert_eq!(
                        ws.right_sidebar.followed,
                        web.map(|s| s.id),
                        "{n}: follows web"
                    );
                    assert_eq!(
                        ws.activity_note(),
                        Some(NO_ACTIVITY),
                        "{n}: web's empty log"
                    );
                    assert!(ws.activity_rows().is_empty(), "{n}");
                    let (label, _, _) = ws.activity_header().expect("followed");
                    assert_eq!(label.as_ref(), "web", "{n}: its header");
                });
                slot.borrow_mut().take();
            }
        };
        let shot = render_with(&theme, dims, build, act);
        assert!(
            has(
                &shot,
                sidebar_left + 2.0,
                sidebar_right,
                list_top,
                list_top + 30.0,
                ui.text_dim,
                6
            ),
            "{n}: the note's words"
        );
        for c in marks {
            assert!(
                !has(
                    &shot,
                    mark_x - 1.0,
                    mark_x + 11.0,
                    list_top,
                    list_top + 4.0 * ROW,
                    c,
                    24
                ),
                "{n}: no rows ({c:?})"
            );
        }
        println!("ui-shots: {n} ok");

        // Waiting, on a question that wraps.
        let n = format!("activity-waiting-{slug}");
        let slot: Slot = Rc::new(RefCell::new(None));
        let build = activity_workspace(theme, &[("api", ASKING_TEXT)], true, slot.clone());
        let act = {
            let (n, slot) = (n.clone(), slot.clone());
            move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let ws = slotted(&slot);
                ws.update(cx, |ws, cx| {
                    ws.set_status(0, Status::Working, false, cx);
                    ws.set_status(0, Status::Waiting, false, cx);
                    ws.sessions.get_mut(0).expect("api").status_since = ago(2 * 60 + 5);
                });
                frame(cx, window);
                cx.read(|cx| {
                    let ws = ws.read(cx);
                    let question = ws.sessions.items()[0].question.clone();
                    assert_eq!(
                        question.as_deref(),
                        Some("Run the database migration on production now? (y/n)"),
                        "{n}: read off the screen"
                    );
                    let (_, words, waiting) = ws.activity_header().expect("followed");
                    assert_eq!(
                        (words.as_str(), waiting),
                        ("Needs input \u{b7} 2m", true),
                        "{n}"
                    );
                    let tip = ws.activity_rows()[0].tip.clone();
                    assert_eq!(
                        tip.as_deref(),
                        Some("Needs input\nRun the database migration on production now? (y/n)"),
                        "{n}: the entry keeps its question"
                    );
                });
                slot.borrow_mut().take();
            }
        };
        let path = shoot_with(
            "activity-waiting",
            slug,
            &theme,
            dims,
            build.clone(),
            act.clone(),
        );
        println!("ui-shots: {}", path.display());
        let shot = render_with(&theme, dims, build, act);
        let line_top = header_top + (ROOT_HEADER - 34.0) / 2.0;
        let left = sidebar_left + 10.0;
        assert!(
            has(
                &shot,
                left,
                left + 80.0,
                line_top + 20.0,
                line_top + 34.0,
                ui.ring_waiting,
                6
            ),
            "{n}: the header's words in the waiting colour"
        );
        // The card: 6 pt in from the sides and under the header, filled `waiting_tint`.
        let (card_left, card_right, top) =
            (sidebar_left + 6.0, sidebar_right - 6.0, list_top + 6.0);
        let tint = |x: f32, y: f32| near(pixel_at(&shot, dims, x, y), ui.waiting_tint, 3);
        assert!(tint(card_left + 4.0, top + 10.0), "{n}: the card's fill");
        assert!(tint(card_right - 4.0, top + 10.0), "{n}: across");
        assert!(
            !tint(sidebar_left + 3.0, top + 10.0),
            "{n}: 6 pt in from the left"
        );
        assert!(
            !tint(card_left + 4.0, list_top + 3.0),
            "{n}: 6 pt under the header"
        );
        // Measured down a column just past its 6 pt corners (in its 8 pt padding).
        let tall = (0..400)
            .map(|dy| top + dy as f32 * 0.5)
            .take_while(|&y| tint(card_left + 7.0, y))
            .count() as f32
            * 0.5;
        println!("ui-shots: {n}: the card is {tall} pt tall");
        // Padding 8 + `Needs input` (11 pt) + two lines of question (13 pt) + 8.
        assert!(
            tall >= 60.0,
            "{n}: two lines of question at least: {tall} pt"
        );
        assert!(tall < 120.0, "{n}: not the whole panel: {tall} pt");
        let first = top + 8.0;
        assert!(
            has(
                &shot,
                card_left + 8.0,
                card_left + 90.0,
                first,
                first + 14.0,
                ui.ring_waiting,
                6
            ),
            "{n}: Needs input"
        );
        assert!(
            has(
                &shot,
                card_left + 8.0,
                card_right - 8.0,
                first + 16.0,
                top + tall - 8.0,
                ui.text,
                12
            ),
            "{n}: the question"
        );
        // The timeline under it: Waiting's mark, then Working's.
        let rows_top = top + tall + 6.0;
        assert!(
            has(
                &shot,
                mark_x - 1.0,
                mark_x + 11.0,
                rows_top,
                rows_top + ROW,
                ui.ring_waiting,
                24
            ),
            "{n}: the newest row is Waiting's"
        );

        // Empty.
        for case in ["none", "unfollowed"] {
            let scene = format!("activity-empty-{case}");
            let n = format!("{scene}-{slug}");
            let slot: Slot = Rc::new(RefCell::new(None));
            let sessions: &'static [(&str, &str)] = match case {
                "none" => &[("api", SHELL_TEXT)],
                _ => &[],
            };
            let build = activity_workspace(theme, sessions, true, slot.clone());
            let act = {
                let (n, slot) = (n.clone(), slot.clone());
                move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                    let ws = slotted(&slot);
                    frame(cx, window);
                    cx.read(|cx| {
                        let ws = ws.read(cx);
                        let (note, header) = match case {
                            "none" => (NO_ACTIVITY, true),
                            _ => (NO_TERMINAL, false),
                        };
                        assert_eq!(ws.activity_note(), Some(note), "{n}");
                        assert_eq!(ws.activity_header().is_some(), header, "{n}: the header");
                    });
                    slot.borrow_mut().take();
                }
            };
            let path = shoot_with(&scene, slug, &theme, dims, build.clone(), act.clone());
            println!("ui-shots: {}", path.display());
            let shot = render_with(&theme, dims, build, act);
            let header = case == "none";
            assert_eq!(
                has(
                    &shot,
                    sidebar_left + 2.0,
                    sidebar_right,
                    header_top,
                    header_top + 30.0,
                    ui.text,
                    6
                ),
                header,
                "{n}: the header {}",
                if header { "shows" } else { "doesn't" }
            );
            let note_top = if header { list_top } else { header_top };
            assert!(
                has(
                    &shot,
                    sidebar_left + 2.0,
                    sidebar_right,
                    note_top,
                    note_top + 30.0,
                    ui.text_dim,
                    6
                ),
                "{n}: the note's words"
            );
        }

        // Nothing moves: two frames 200 ms apart, the same in the panel.
        for still in [true, false] {
            let n = format!("activity-still-{slug}-{still}");
            let slot: Slot = Rc::new(RefCell::new(None));
            let build = activity_workspace(
                theme,
                &[("api", ASKING_TEXT), ("web", SHELL_TEXT)],
                still,
                slot.clone(),
            );
            let act = {
                let n = n.clone();
                move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                    let ws = slotted(&slot);
                    ws.update(cx, |ws, cx| {
                        ws.set_status(0, Status::Working, false, cx);
                        ws.set_note(0, Some("migrating".into()), cx);
                        ws.set_status(0, Status::Waiting, false, cx);
                        ws.set_status(0, Status::Working, false, cx);
                        ws.set_status(0, Status::Done, false, cx);
                        ws.set_status(0, Status::Working, false, cx);
                        ws.set_status(0, Status::Waiting, false, cx);
                        ws.sessions.get_mut(0).expect("api").status_since = ago(4 * 60 + 30);
                    });
                    frame(cx, window);
                    let grab = |cx: &mut VisualTestAppContext| {
                        let image = cx.capture_screenshot(window).expect("capture");
                        let (w, raw) = (image.width() as usize, image.as_raw().to_vec());
                        let scale = w as f32 / f32::from(dims.width);
                        let (x0, x1) = (
                            (sidebar_left * scale) as usize,
                            (sidebar_right * scale) as usize,
                        );
                        let (y0, y1) = (
                            (header_top * scale) as usize,
                            ((f32::from(dims.height) - 20.0) * scale) as usize,
                        );
                        (y0..y1)
                            .flat_map(|y| raw[(y * w + x0) * 4..(y * w + x1) * 4].to_vec())
                            .collect::<Vec<u8>>()
                    };
                    let before = grab(cx);
                    std::thread::sleep(Duration::from_millis(200));
                    // Drawn again for certain (not a cached frame).
                    ws.update(cx, |_, cx| cx.notify());
                    frame(cx, window);
                    let after = grab(cx);
                    assert!(before == after, "{n}: the panel moved in 200 ms");
                    assert_eq!(cx.read(|cx| ws.read(cx).activity_rows().len()), 7, "{n}");
                    slot.borrow_mut().take();
                }
            };
            render_with(&theme, dims, build, act);
            println!("ui-shots: {n} ok");
        }
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

/// New terminals with no name of yours (M11c Task 3b): **╭∩╮** (`groups::DEFAULT_NAME`), the
/// user's signature, whether in a folder or not (the one shown draws it in its tab and row).
/// Shot in both themes (`default-name-*.png`) to see that the glyphs draw (no missing-glyph
/// boxes); panics unless both are called that, as `Session::label` (the scripting API's name).
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
                let api = Some(std::path::Path::new(API));
                ws.open_unnamed_shot_session(api, SHELL_TEXT, window, cx);
                ws.open_unnamed_shot_session(None, SHELL_TEXT, window, cx);
                ws
            });
            *keep.borrow_mut() = Some(ws.clone());
            ws.into()
        };
        let acting = slot.clone();
        let act = move |cx: &mut VisualTestAppContext, _window: AnyWindowHandle| {
            let ws = slotted(&acting);
            cx.read(|cx| {
                let ws = ws.read(cx);
                let named = |folder: bool| {
                    let s = ws
                        .sessions
                        .items()
                        .iter()
                        .find(|s| s.project.is_some() == folder);
                    let s = s.expect("both opened");
                    (ws.terminal_label(s.id).to_string(), s.label().to_string())
                };
                let default = crate::groups::DEFAULT_NAME.to_string();
                assert_eq!(
                    named(true),
                    (default.clone(), default.clone()),
                    "default-name: ╭∩╮ for a terminal in a folder too"
                );
                assert_eq!(
                    named(false),
                    (default.clone(), default),
                    "default-name: ╭∩╮ for a terminal with no folder"
                );
            });
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

/// The folders of `names`'s terminals.
const FUXX_CLI: &str = "/Users/me/Kerja/proj/fuxx-cli";
const INVITEE: &str = "/Users/me/w/invitee";
const SHOT_HOME: &str = "/Users/me";
/// What devbox's remote shell reported (OSC 7).
const DEVBOX_SRC: &str = "/home/me/src";
/// A shell theme's title (OSC 2): the name of a terminal you haven't named.
const SHELL_TITLE: &str = "me@mac:~/Kerja/proj/fuxx-cli";
/// The title Claude Code sets (OSC 2): it names its terminal (the program renames it).
const AGENT_TITLE: &str = "✳ Claude Code";
/// The preset `names`'s at-home terminal was started from.
const SHOT_PRESET: &str = "logs";

/// The terminals of `names`'s shown workspace, by id.
#[derive(Clone, Copy, Default)]
struct NameIds {
    claude: u64,
    prompt: u64,
}

/// `names`'s window. The shown workspace: claude (running `claude` in fuxx-cli, its title
/// Claude Code's, focused), a dev server (`npm run dev` in invitee, no title) and api (yours,
/// running `claude`, with Claude Code's title) in the left pane; the dashboard page, a shell at
/// its prompt in fuxx-cli (with a shell theme's title), one at home started from the `logs`
/// preset and an SSH terminal to devbox in the right. Then a workspace each for: `npm run dev`
/// in invitee, a prompt in fuxx-cli (titled), a prompt at home, api (titled too), and devbox. Programs and
/// folders are set, not looked up.
fn names_workspace(window: &mut Window, cx: &mut Context<Workspace>) -> (Workspace, NameIds) {
    use std::path::Path;
    use std::time::Instant;

    use crate::browser_tab::BarState;
    use crate::layout_model::TabKind;
    use crate::procs::ProgramState::{Prompt, Running};
    use crate::workspace::At;

    let mut ws = Workspace::for_shots(window, cx);
    ws.sidebar.hint_dismissed = true;
    let (fuxx, invitee, home) = (
        Some(Path::new(FUXX_CLI)),
        Some(Path::new(INVITEE)),
        Some(Path::new(SHOT_HOME)),
    );
    let running = |name: &str| Running(name.into());
    let claude = ws.open_shot_at(None, fuxx, AGENT_TEXT, At::New, window, cx);
    let left = ws.model.focused_pane().expect("a pane");
    let npm = ws.open_shot_at(None, invitee, SHELL_TEXT, At::Tab(left), window, cx);
    let api = ws.open_shot_at(Some("api"), fuxx, AGENT_TEXT, At::Tab(left), window, cx);
    let url = Some(PAGE_URL.to_string());
    let page = TabKind::Browser { url: url.clone() };
    let opened = ws.model.split(left, Axis::Row, page).expect("split");
    ws.sync();
    let view = ws.make_browser_tab(opened.tab, url, window, cx);
    view.update(cx, |view, _| {
        view.show_state(BarState {
            url: PAGE_URL.into(),
            title: PAGE_TITLE.into(),
            loading: false,
            can_back: false,
            can_forward: false,
        })
    });
    ws.browser_titles.insert(opened.tab, PAGE_TITLE.into());
    let right = At::Tab(opened.pane);
    let prompt = ws.open_shot_at(None, fuxx, SHELL_TEXT, right, window, cx);
    let at_home = ws.open_shot_at(None, home, SHELL_TEXT, right, window, cx);
    let devbox = ws.open_shot_at(None, None, SHELL_TEXT, right, window, cx);
    // A workspace of its own each.
    let rows = [
        (None, invitee, Some(running("npm"))),
        (None, fuxx, Some(Prompt)),
        (None, home, Some(Prompt)),
        (Some("api"), fuxx, Some(running("claude"))),
        (None, None, None),
    ];
    let mut opened_rows = Vec::new();
    for (name, folder, state) in rows {
        let id = ws.open_shot_at(name, folder, SHELL_TEXT, At::New, window, cx);
        if let Some(state) = state {
            ws.programs.set_for_shots(id, state);
        }
        opened_rows.push(id);
    }
    let (row_prompt, row_api, row_devbox) = (opened_rows[1], opened_rows[3], opened_rows[4]);
    for (id, state) in [
        (claude, running("claude")),
        (npm, running("npm")),
        (api, running("claude")),
        (prompt, Prompt),
        (at_home, Prompt),
    ] {
        ws.programs.set_for_shots(id, state);
    }
    for id in [devbox, row_devbox] {
        let i = ws.index_of(id).expect("opened");
        ws.sessions.get_mut(i).expect("opened").ssh = Some(crate::ssh_target::SshTarget {
            host: "devbox".into(),
            tmux: false,
            tmux_name: None,
        });
        ws.places
            .set_folder(id, Some(DEVBOX_SRC.into()), Instant::now());
    }
    for (id, title) in [
        (prompt, SHELL_TITLE),
        (row_prompt, SHELL_TITLE),
        (claude, AGENT_TITLE),
        (api, AGENT_TITLE),
        (row_api, AGENT_TITLE),
    ] {
        let i = ws.index_of(id).expect("opened");
        ws.sessions.get_mut(i).expect("opened").title = Some(title.into());
    }
    let i = ws.index_of(at_home).expect("opened");
    ws.sessions.get_mut(i).expect("opened").preset = Some(crate::presets::Preset {
        name: SHOT_PRESET.into(),
        command: "tail -f log".into(),
    });
    ws.sync();
    ws.select(ws.index_of(claude).expect("claude"), window, cx);
    (ws, NameIds { claude, prompt })
}

/// Names (as before M11i, the user's call of 2026-11-09): every tab, row and the window title
/// say a terminal's `Session::label`: your name, else the program's own title (a program that
/// sets one renames its terminal), else its preset's name, else ╭∩╮, whatever runs there and
/// wherever it is. In both themes (`names-*.png`). Panics unless:
/// - the tabs read Claude Code's title, ╭∩╮ (the dev server: no title), `api` (yours wins
///   over the title), the page's title, the shell theme's title, `logs` (the preset) and ╭∩╮
///   (devbox); the rows read the shown workspace's focused terminal's name and ╭∩╮, the shell
///   theme's title, ╭∩╮, `api` and ╭∩╮; the window title the shown workspace's;
/// - each terminal's name is its `Session::label` (the scripting API's);
/// - only a name that isn't its title has the title under it in a tooltip (api's tab and row);
/// - Enter on an untouched automatic name, in the tab's rename box and the row's, keeps the
///   terminal and the workspace automatic, also when the title changes while the box is open
///   (the name then follows the new title).
fn names() {
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;
    use std::time::Instant;

    use gpui::KeyBinding;

    use crate::groups::DEFAULT_NAME;
    use crate::sidebar::SidebarRow;
    use crate::workspace::window_title;

    for (slug, name) in SHOT_THEMES {
        let theme = shot_theme(name);
        let slot: Slot = Rc::new(RefCell::new(None));
        let keep = slot.clone();
        let ids = Rc::new(Cell::new(NameIds::default()));
        let set_ids = ids.clone();
        let build = move |window: &mut Window, cx: &mut App| -> AnyView {
            cx.set_global(shot_appearance(theme, cx));
            let ws = cx.new(|cx| {
                let (ws, ids) = names_workspace(window, cx);
                set_ids.set(ids);
                ws
            });
            *keep.borrow_mut() = Some(ws.clone());
            ws.into()
        };
        let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
            let ws = slotted(&slot);
            let ids = ids.get();
            let n = format!("names-{slug}");
            cx.read(|cx| {
                let ws = ws.read(cx);
                let now = Instant::now();
                let panes = ws.shown_layout().expect("a layout").leaves();
                let bars: Vec<Vec<crate::tab_bar::BarTab>> = panes
                    .iter()
                    .map(|&p| ws.pane_bar_tabs(ws.model.pane(p).expect("its pane"), now))
                    .collect();
                let titles = |bar: &[crate::tab_bar::BarTab]| -> Vec<String> {
                    bar.iter().map(|t| t.title.to_string()).collect()
                };
                assert_eq!(
                    titles(&bars[0]),
                    [AGENT_TITLE, DEFAULT_NAME, "api"],
                    "{n}: the left pane's tabs"
                );
                assert_eq!(
                    titles(&bars[1]),
                    [PAGE_TITLE, SHELL_TITLE, SHOT_PRESET, DEFAULT_NAME],
                    "{n}: the right pane's tabs"
                );
                let tips: Vec<&str> = bars
                    .iter()
                    .flatten()
                    .filter(|t| t.terminal.is_some())
                    .map(|t| t.tip.as_str())
                    .collect();
                assert_eq!(
                    tips,
                    [
                        AGENT_TITLE,
                        DEFAULT_NAME,
                        &format!("api\n{AGENT_TITLE}"),
                        SHELL_TITLE,
                        SHOT_PRESET,
                        DEFAULT_NAME,
                    ],
                    "{n}: the title is under a name only when it isn't the name"
                );
                // Every terminal's name is the scripting API's, whatever runs there.
                for s in ws.sessions.items() {
                    assert_eq!(ws.terminal_label(s.id), s.label(), "{n}: s{}", s.id);
                }
                let rows: Vec<(String, Option<String>)> = ws
                    .rows_in_view(now)
                    .into_iter()
                    .filter_map(|row| match row {
                        SidebarRow::Workspace(w) => Some((w.label, ws.workspace_tip(w.workspace))),
                        SidebarRow::Header(_) => None,
                    })
                    .collect();
                let mut labels: Vec<&str> = rows.iter().map(|(l, _)| l.as_str()).collect();
                labels.sort();
                let mut want = [
                    AGENT_TITLE,
                    DEFAULT_NAME,
                    SHELL_TITLE,
                    DEFAULT_NAME,
                    "api",
                    DEFAULT_NAME,
                ];
                want.sort();
                assert_eq!(labels, want, "{n}: the rows");
                let tips: Vec<&str> = rows.iter().filter_map(|(_, t)| t.as_deref()).collect();
                assert_eq!(
                    tips,
                    [format!("api\n{AGENT_TITLE}").as_str()],
                    "{n}: only api's row has a tooltip, the title under the name"
                );
                let shown = ws.model.focused_workspace().expect("shown");
                let number = ws.workspace_number(shown).expect("numbered");
                assert_eq!(
                    window_title(number, &ws.workspace_label(shown)),
                    window_title(number, AGENT_TITLE),
                    "{n}: the window title"
                );
                let i = ws.index_of(ids.prompt).expect("open");
                assert_eq!(ws.sessions.items()[i].label(), SHELL_TITLE, "{n}: API");
            });
            // Enter on an untouched automatic name: the tab's rename box, then the row's.
            cx.update(|cx| {
                cx.bind_keys([KeyBinding::new(
                    "enter",
                    crate::name_input::Confirm,
                    Some("NameInput"),
                )])
            });
            let shown_ws = cx.read(|cx| ws.read(cx).model.focused_workspace().expect("shown"));
            // Each box twice: untouched; then with the title changing while it's open (claude
            // says what it's at): Enter on the text it opened with still stores no name
            // (Settled #14), and the name follows the title.
            const NEW_TITLE: &str = "✳ Fix the tests";
            let set_title = |cx: &mut VisualTestAppContext, title: &str| {
                cx.update(|cx| {
                    ws.update(cx, |ws, cx| {
                        let i = ws.index_of(ids.claude).expect("open");
                        ws.sessions.get_mut(i).expect("open").title =
                            Some(title.to_string().into());
                        cx.notify();
                    })
                });
                cx.run_until_parked();
            };
            for (tab, changes) in [(true, false), (false, false), (true, true), (false, true)] {
                cx.update_window(window, |_, window, cx| {
                    ws.update(cx, |ws, cx| {
                        if tab {
                            ws.start_tab_rename(ids.claude, window, cx)
                        } else {
                            ws.start_rename(shown_ws, window, cx)
                        }
                    })
                })
                .expect("open the rename box");
                cx.run_until_parked();
                cx.read(|cx| {
                    let ws = ws.read(cx);
                    let (_, input) = ws.renaming.as_ref().expect("the rename box is open");
                    assert_eq!(
                        input.read(cx).text(),
                        AGENT_TITLE,
                        "{n}: the box starts with the name"
                    );
                });
                if changes {
                    set_title(cx, NEW_TITLE);
                }
                cx.simulate_keystrokes(window, "enter");
                cx.run_until_parked();
                cx.read(|cx| {
                    let ws = ws.read(cx);
                    let m = format!("{n} (tab: {tab}, changed while open: {changes})");
                    assert!(ws.renaming.is_none(), "{m}: Enter closed the box");
                    let i = ws.index_of(ids.claude).expect("open");
                    assert_eq!(ws.sessions.items()[i].name, None, "{m}: still automatic");
                    let w = ws.model.workspace(shown_ws).expect("shown");
                    assert_eq!(w.name, None, "{m}: the workspace still automatic");
                    let now = if changes { NEW_TITLE } else { AGENT_TITLE };
                    assert_eq!(ws.terminal_label(ids.claude).as_ref(), now, "{m}");
                    assert_eq!(ws.workspace_label(shown_ws).as_ref(), now, "{m}");
                });
                set_title(cx, AGENT_TITLE);
            }
            slot.borrow_mut().take();
        };
        let path = shoot_with(
            "names",
            slug,
            &theme,
            size(px(1440.0), px(560.0)),
            build,
            act,
        );
        println!("ui-shots: {}", path.display());
    }
}

/// A made-up shell pid for terminal `id` in `renames`'s `ps` tables.
fn shot_shell_pid(id: u64) -> u32 {
    1000 + id as u32
}

/// What the programs' lookup does with each `ps` table (`Programs::take` and a redraw; the
/// `ps` itself, and the 3 s timer that runs it, are left out: `for_shots` freezes the lookups):
/// each of `states`' terminals at its prompt (`false`) or running `make all` (`true`), offered
/// twice so the steadying takes it.
fn programs_lookup(
    cx: &mut VisualTestAppContext,
    ws: &gpui::Entity<Workspace>,
    states: &[(u64, bool)],
) {
    use crate::procs::Proc;
    let proc = |pid: u32, pgid: u32, tpgid: u32, args: &str| Proc {
        pid,
        ppid: 1,
        pgid,
        tpgid,
        stat: "S".into(),
        args: args.into(),
    };
    let mut table = Vec::new();
    let mut shells = Vec::new();
    for &(id, running) in states {
        let shell = shot_shell_pid(id);
        let job = shell + 500;
        table.push(proc(
            shell,
            shell,
            if running { job } else { shell },
            "-zsh",
        ));
        if running {
            table.push(proc(job, job, job, "make all"));
        }
        shells.push((id, shell));
    }
    cx.update(|cx| {
        ws.update(cx, |ws, cx| {
            ws.programs.take(Some(table.clone()), &shells);
            ws.programs.take(Some(table), &shells);
            cx.notify();
        })
    });
    cx.run_until_parked();
}

/// The ids of `renames`'s terminals.
#[derive(Clone, Copy, Default)]
struct RenameIds {
    prompt: u64,
    running: u64,
}

/// Renaming by typing (the user's report, 2026-11-09: "I cannot rename them at all"), for a
/// terminal at its prompt and one running a program, each in a workspace of its own. Each box
/// is opened by a real gesture; a programs lookup (`programs_lookup`) runs while it's open and
/// after Enter, and the program sets a new title (OSC 2, through the terminal's own parser and
/// `TerminalEvent::TitleChanged`) between the typing and Enter:
/// - a double-click on its tab: the tab's box opens with the keyboard (with ╭∩╮: no title, no
///   preset), stays open and focused through the lookup and the title, and the typed name
///   names the terminal: the tab, the row (its workspace has no name) and the window title
///   show it, not the title;
/// - a double-click on its row (the sidebar takes the keyboard back on each press first): the
///   row's box opens with the keyboard, holds, and the typed name names the workspace: the row
///   and the window title show it, the tab the terminal's;
/// - ⌘R: the same, a second name for the workspace.
///
/// In both themes (`renames-*.png`, the second workspace renamed). Panics on the first that
/// doesn't hold.
fn renames() {
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;
    use std::time::Instant;

    use gpui::{
        Focusable, KeyBinding, Modifiers, MouseButton, MouseDownEvent, MouseUpEvent, Point,
    };

    use crate::sidebar::SidebarRow;
    use crate::workspace::{At, RenameSession, Renaming, window_title};

    for (slug, name) in SHOT_THEMES {
        let theme = shot_theme(name);
        let slot: Slot = Rc::new(RefCell::new(None));
        let keep = slot.clone();
        let ids = Rc::new(Cell::new(RenameIds::default()));
        let set_ids = ids.clone();
        let build = move |window: &mut Window, cx: &mut App| -> AnyView {
            cx.set_global(shot_appearance(theme, cx));
            let ws = cx.new(|cx| {
                let mut ws = Workspace::for_shots(window, cx);
                ws.sidebar.hint_dismissed = true;
                let fuxx = Some(std::path::Path::new(FUXX_CLI));
                let prompt = ws.open_shot_at(None, fuxx, SHELL_TEXT, At::New, window, cx);
                let running = ws.open_shot_at(None, fuxx, AGENT_TEXT, At::New, window, cx);
                ws.programs.set_for_shots(prompt, ProgramState::Prompt);
                ws.programs
                    .set_for_shots(running, ProgramState::Running("claude".into()));
                ws.sync();
                set_ids.set(RenameIds { prompt, running });
                ws
            });
            *keep.borrow_mut() = Some(ws.clone());
            ws.into()
        };
        let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
            let ws = slotted(&slot);
            let ids = ids.get();
            cx.update(|cx| {
                cx.bind_keys([
                    KeyBinding::new("enter", crate::name_input::Confirm, Some("NameInput")),
                    KeyBinding::new("cmd-r", RenameSession, None),
                ])
            });
            let none = Modifiers::none();
            let double_click = |cx: &mut VisualTestAppContext, at: Point<Pixels>| {
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
                cx.run_until_parked();
            };
            // Whether the rename box is open on `on`, with the keyboard, and its text.
            let open_on = |cx: &mut VisualTestAppContext, on: Renaming| {
                cx.update_window(window, |_, window, cx| {
                    let ws = ws.read(cx);
                    ws.renaming
                        .as_ref()
                        .filter(|(r, _)| *r == on)
                        .map(|(_, input)| {
                            let input = input.read(cx);
                            (
                                input.focus_handle(cx).is_focused(window),
                                input.text().to_string(),
                            )
                        })
                })
                .expect("read the box")
            };
            // The program sets terminal `id`'s title (OSC 2), through the terminal's parser.
            let set_title = |cx: &mut VisualTestAppContext, id: u64, title: &str| {
                cx.update(|cx| {
                    let ws = ws.read(cx);
                    let i = ws.index_of(id).expect("open");
                    let view = ws.sessions.items()[i].view.read(cx);
                    view.show_text(&format!("\x1b]2;{title}\x07"));
                });
                cx.run_until_parked();
                let got = cx.read(|cx| {
                    let ws = ws.read(cx);
                    let i = ws.index_of(id).expect("open");
                    ws.sessions.items()[i].title.as_ref().map(|t| t.to_string())
                });
                assert_eq!(got.as_deref(), Some(title), "the title reached the session");
            };
            // What the tab, the row and the window title say about terminal `id`'s workspace.
            let shown = |cx: &mut VisualTestAppContext, id: u64| {
                cx.read(|cx| {
                    let ws = ws.read(cx);
                    let now = Instant::now();
                    let pane = ws.model.focused_pane().expect("a pane");
                    let tabs = ws.pane_bar_tabs(ws.model.pane(pane).expect("its pane"), now);
                    let tab = tabs
                        .iter()
                        .find(|t| t.terminal == Some(id))
                        .expect("its tab");
                    let w = ws.model.focused_workspace().expect("shown");
                    let row = ws.rows_in_view(now).into_iter().find_map(|row| match row {
                        SidebarRow::Workspace(r) if r.workspace == w => Some(r.label),
                        _ => None,
                    });
                    let number = ws.workspace_number(w).expect("numbered");
                    (
                        tab.title.to_string(),
                        row.expect("its row"),
                        window_title(number, &ws.workspace_label(w)),
                        number,
                    )
                })
            };
            // Type `typed` into the box open on `on` (whose text is `from`), with a lookup
            // before and the title changing before Enter, then a lookup after.
            let rename = |cx: &mut VisualTestAppContext,
                          id: u64,
                          running: bool,
                          on: Renaming,
                          from: &str,
                          typed: &str,
                          m: &str| {
                assert_eq!(
                    open_on(cx, on),
                    Some((true, from.to_string())),
                    "{m}: the box is open with the keyboard, with the name"
                );
                programs_lookup(cx, &ws, &[(id, !running)]);
                assert_eq!(
                    open_on(cx, on).map(|(focused, _)| focused),
                    Some(true),
                    "{m}: still open with the keyboard after a lookup"
                );
                cx.simulate_input(window, typed);
                set_title(cx, id, &format!("✳ {typed} title"));
                assert_eq!(
                    open_on(cx, on),
                    Some((true, typed.to_string())),
                    "{m}: still open with the keyboard and the typing after a new title"
                );
                cx.simulate_keystrokes(window, "enter");
                cx.run_until_parked();
                programs_lookup(cx, &ws, &[(id, running)]);
                assert!(
                    cx.read(|cx| ws.read(cx).renaming.is_none()),
                    "{m}: Enter closed the box"
                );
            };
            let default = crate::groups::DEFAULT_NAME;
            for (id, running, typed, ws_clicked, ws_keyed) in [
                (ids.prompt, false, "my-shell", "shell-work", "shell-2"),
                (ids.running, true, "my-agent", "agent-work", "agent-2"),
            ] {
                let n = format!("renames-{slug} (running: {running})");
                cx.update_window(window, |_, window, cx| {
                    ws.update(cx, |ws, cx| {
                        let i = ws.index_of(id).expect("open");
                        ws.select(i, window, cx);
                    })
                })
                .expect("show it");
                cx.run_until_parked();
                // The tab: a double-click on it.
                let pane = shown_panes(cx, &ws)[0];
                let at = tab_bounds(cx, &ws, pane, 0).expect("laid out").center();
                double_click(cx, at);
                let m = format!("{n}, the tab");
                rename(cx, id, running, Renaming::Terminal(id), default, typed, &m);
                cx.read(|cx| {
                    let ws = ws.read(cx);
                    let i = ws.index_of(id).expect("open");
                    let name = ws.sessions.items()[i].name.as_deref();
                    assert_eq!(name, Some(typed), "{m}: the terminal is named");
                });
                let (tab, row, title, number) = shown(cx, id);
                assert_eq!(tab, typed, "{m}: the tab, not the title");
                assert_eq!(row, typed, "{m}: the row (its workspace has no name)");
                assert_eq!(title, window_title(number, typed), "{m}: the window title");

                // The row: a double-click on it, then ⌘R.
                let (w, at) = cx.read(|cx| {
                    let ws = ws.read(cx);
                    let w = ws.model.focused_workspace().expect("shown");
                    let item = ws
                        .rows_in_view(Instant::now())
                        .iter()
                        .position(|row| matches!(row, SidebarRow::Workspace(r) if r.workspace == w))
                        .expect("its row");
                    let b = ws.sidebar.scroll.bounds_for_item(item).expect("laid out");
                    (w, b.center())
                });
                for (gesture, from, typed_ws) in [
                    ("double-click", typed, ws_clicked),
                    ("cmd-r", ws_clicked, ws_keyed),
                ] {
                    let m = format!("{n}, the row by {gesture}");
                    match gesture {
                        "double-click" => double_click(cx, at),
                        _ => {
                            cx.simulate_keystrokes(window, "cmd-r");
                            cx.run_until_parked();
                        }
                    }
                    rename(cx, id, running, Renaming::Workspace(w), from, typed_ws, &m);
                    cx.read(|cx| {
                        let ws = ws.read(cx);
                        let name = ws.model.workspace(w).and_then(|w| w.name.clone());
                        assert_eq!(name.as_deref(), Some(typed_ws), "{m}: the workspace");
                    });
                    let (tab, row, title, number) = shown(cx, id);
                    assert_eq!(tab, typed, "{m}: the tab keeps the terminal's name");
                    assert_eq!(row, typed_ws, "{m}: the row");
                    assert_eq!(
                        title,
                        window_title(number, typed_ws),
                        "{m}: the window title"
                    );
                }
            }
            slot.borrow_mut().take();
        };
        let path = shoot_with(
            "renames",
            slug,
            &theme,
            size(px(960.0), px(360.0)),
            build,
            act,
        );
        println!("ui-shots: {}", path.display());
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
                // Under the 30 pt tab bar (M11i), centred in the 34 px bar; the card starts
                // under the 6 pt strip, inside its 1 pt edge.
                assert_eq!(f32::from(field.size.height), 24.0, "{n}: field height");
                let top = f32::from(field.top());
                assert!((40.0..=43.0).contains(&top), "{n}: field top {top}");
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

/// The ⌘P list's eight sessions (the palette scenes'): api (waiting with a boxed question,
/// working, done, idle), web (waiting with no question found, done) and the SSH host build-box
/// (working, idle).
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

/// The rename box over a real workspace, in both themes:
/// - `rename`: the rename box just opened on shell (the selected row, tinted with the accent),
///   its old name selected; `rename-typed`: "api-2" typed over it.
///
/// Panics unless the rename box has the old (or typed) name, and the box's text is drawn in
/// `text` (it reads on the box, whatever the row under it is filled with).
fn rename_box_scenes() {
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    let dims = size(px(960.0), px(600.0));
    for (slug, name) in SHOT_THEMES {
        let theme = shot_theme(name);
        let ui = theme.ui();
        for scene in ["rename", "rename-typed"] {
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
                cx.update_window(window, |_, window, cx| {
                    ws.update(cx, |ws, cx| {
                        ws.start_rename(ws.model.display_order()[3], window, cx)
                    })
                })
                .expect("open it");
                cx.run_until_parked();
                if scene == "rename-typed" {
                    cx.simulate_input(window, "api-2");
                }
                cx.read(|cx| {
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
                });
                // Not kept past the window: a handle left over is reported as a leak.
                slot.borrow_mut().take();
            };
            let path = shoot_with(scene, slug, &theme, dims, build.clone(), act.clone());
            println!("ui-shots: {}", path.display());
            {
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

/// Frames, so focus changes (and the boxes' blur events) and scrolls run.
fn settle(cx: &mut VisualTestAppContext, window: AnyWindowHandle) {
    for _ in 0..3 {
        cx.update_window(window, |_, window, cx| window.draw(cx).clear())
            .expect("draw");
        cx.run_until_parked();
    }
}

/// The open palette; none while it's closed.
fn palette_of(
    cx: &mut VisualTestAppContext,
    ws: &gpui::Entity<Workspace>,
) -> Option<gpui::Entity<crate::palette::Palette>> {
    cx.read(|cx| ws.read(cx).palette.clone())
}

/// The open palette's lines, scope and selected row's key.
fn palette_state(
    cx: &mut VisualTestAppContext,
    ws: &gpui::Entity<Workspace>,
) -> (
    Vec<crate::palette_items::Line>,
    crate::palette_items::Scope,
    Option<crate::palette_items::Key>,
) {
    let palette = palette_of(cx, ws).expect("the palette is open");
    cx.read(|cx| {
        let p = palette.read(cx);
        (p.lines().to_vec(), p.scope(), p.selected_key().cloned())
    })
}

/// The names of the workspaces the open palette lists, in order.
fn palette_names(cx: &mut VisualTestAppContext, ws: &gpui::Entity<Workspace>) -> Vec<String> {
    let palette = palette_of(cx, ws).expect("the palette is open");
    cx.read(|cx| {
        palette
            .read(cx)
            .shown_workspaces()
            .map(|e| e.label.clone())
            .collect()
    })
}

/// Open the palette on `scope` as ⌘K / ⌘P do, then let it take the keyboard.
fn open_palette_on(
    cx: &mut VisualTestAppContext,
    window: AnyWindowHandle,
    ws: &gpui::Entity<Workspace>,
    scope: crate::palette_items::Scope,
) {
    cx.update_window(window, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.open_palette(scope, window, cx))
    })
    .expect("open the palette");
    settle(cx, window);
}

/// Press `keys` (space-separated keystrokes), a few frames after each.
fn press_keys(cx: &mut VisualTestAppContext, window: AnyWindowHandle, keys: &str) {
    for key in keys.split(' ') {
        cx.simulate_keystrokes(window, key);
        settle(cx, window);
    }
}

/// The workspaces' ids in display order (#1 first) and the id of the one named `label`.
fn workspace_ids(cx: &mut VisualTestAppContext, ws: &gpui::Entity<Workspace>) -> Vec<u64> {
    cx.read(|cx| {
        ws.read(cx)
            .model
            .display_order()
            .iter()
            .map(|w| w.0)
            .collect()
    })
}

/// Whether the active terminal's view has the keyboard.
fn active_terminal_focused(
    cx: &mut VisualTestAppContext,
    window: AnyWindowHandle,
    ws: &gpui::Entity<Workspace>,
) -> bool {
    use gpui::Focusable;
    cx.update_window(window, |_, window, cx| {
        let ws = ws.read(cx);
        let active = ws.sessions.active().expect("an active terminal");
        active.view.focus_handle(cx).is_focused(window)
    })
    .expect("read the focus")
}

/// `switcher_workspace` with the app's whole key-binding table bound (⌘K, ⌘P, the palette's
/// keys, the box's Enter and Esc).
fn palette_build(
    theme: Theme,
    keep: Slot,
) -> impl FnOnce(&mut Window, &mut App) -> AnyView + Clone {
    move |window: &mut Window, cx: &mut App| -> AnyView {
        cx.set_global(sidebar_appearance(theme, cx));
        cx.bind_keys(crate::key_bindings());
        let ws = cx.new(|cx| switcher_workspace(window, cx));
        *keep.borrow_mut() = Some(ws.clone());
        ws.into()
    }
}

/// The ⌘K palette over `switcher_workspace` (shell's terminal focused), in both themes:
/// `palette`, just opened on All: Workspaces (the five most urgent, then `… 3 more
/// workspaces`) and Actions (the first five, then how many more), no Recent yet.
///
/// Panics unless the lines are exactly those, the first row is selected and filled
/// `row_active` (the next one isn't), the `All` chip is filled `row_active` (the Actions chip
/// isn't), and the panel is 600 pt wide.
fn palette_scene() {
    use crate::help_shortcuts::binding_pairs;
    use crate::palette_items::{Focused, Key, Kind, Line, Scope, action_rows};
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    type Seen = (
        gpui::Bounds<Pixels>,
        [gpui::Bounds<Pixels>; 2],
        [gpui::Bounds<Pixels>; 2],
    );
    let dims = size(px(960.0), px(600.0));
    for (slug, name) in SHOT_THEMES {
        let theme = shot_theme(name);
        let ui = theme.ui();
        let slot: Slot = Rc::new(RefCell::new(None));
        let build = palette_build(theme, slot.clone());
        let seen: Rc<Cell<Option<Seen>>> = Rc::default();
        let found = seen.clone();
        let n = format!("palette-{slug}");
        let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
            let ws = slotted(&slot);
            let ids = workspace_ids(cx, &ws);
            open_palette_on(cx, window, &ws, Scope::All);
            // The actions a terminal has with persistent sessions off, the presets' rows in.
            let presets: Vec<String> = cx.read(|cx| {
                cx.global::<Appearance>()
                    .presets
                    .iter()
                    .map(|p| p.name.clone())
                    .collect()
            });
            let mut bindings = crate::new_bindings().to_vec();
            bindings.extend(crate::key_bindings());
            let actions = action_rows(
                Focused::Terminal,
                false,
                &presets,
                &binding_pairs(&bindings),
            );
            let item = |key| Line::Item {
                key,
                positions: Vec::new(),
            };
            let mut want = vec![Line::Header("Workspaces".into())];
            // claude, agent, tests, build, codex: most urgent first.
            want.extend([0, 4, 2, 5, 1].map(|i| item(Key::Workspace(ids[i]))));
            want.push(Line::More {
                kind: Kind::Workspace,
                count: 3,
            });
            want.push(Line::Header("Actions".into()));
            want.extend(
                [
                    "New Workspace",
                    "New Workspace in Folder\u{2026}",
                    "New SSH Workspace\u{2026}",
                    "Rename Workspace\u{2026}",
                    "Move Workspace Up",
                ]
                .map(|a| item(Key::Action(a))),
            );
            want.push(Line::More {
                kind: Kind::Action,
                count: actions.len() - 5,
            });
            let (lines, scope, selected) = palette_state(cx, &ws);
            assert_eq!(lines, want, "{n}: the lines");
            assert_eq!(scope, Scope::All, "{n}: on All");
            assert_eq!(selected, Some(Key::Workspace(ids[0])), "{n}: claude chosen");
            let palette = palette_of(cx, &ws).expect("open");
            found.set(cx.read(|cx| {
                let p = palette.read(cx);
                Some((
                    p.panel_bounds()?,
                    [p.row_bounds(1)?, p.row_bounds(2)?],
                    [p.chip_bounds(Scope::All)?, p.chip_bounds(Scope::Actions)?],
                ))
            }));
            slot.borrow_mut().take();
        };
        let shot = render_with(&theme, dims, build, act);
        save_shot("palette", slug, &shot);
        let (panel, [first, second], [all, actions]) = seen.get().expect("drawn");
        let n = format!("palette-{slug}");
        // Inside its 1 pt edge (what an absolute child fills): 600 pt with it, centred in the
        // window less the 16 pt each side `top_panel` keeps.
        let outer = f32::from(panel.size.width) + 2.0;
        assert!(
            (outer - 600.0).abs() < 0.5,
            "{n}: the panel is {outer} wide"
        );
        let left = f32::from(panel.left()) - 1.0;
        assert!((left - 180.0).abs() < 0.5, "{n}: centred ({left})");
        // In a row's left padding, and in a chip's: their fill, clear of any text.
        let fill = |b: gpui::Bounds<Pixels>, inset: f32| {
            pixel_at(
                &shot,
                dims,
                f32::from(b.left()) + inset,
                f32::from(b.center().y),
            )
        };
        assert!(
            near(fill(first, 4.0), ui.row_active),
            "{n}: the first row is chosen"
        );
        assert!(near(fill(second, 4.0), ui.panel), "{n}: the second isn't");
        assert!(
            near(fill(all, 3.0), ui.row_active),
            "{n}: the All chip is lit"
        );
        assert!(
            near(fill(actions, 3.0), ui.panel),
            "{n}: the Actions chip isn't"
        );
    }
}

/// ⌘P's list in the palette (its Workspaces scope), in both themes:
/// `palette-workspaces`: "build" typed, which finds web's build by its name and build-box's two
/// by their group.
///
/// Panics unless, just opened, the list is the old ⌘P one (every workspace, most urgent first,
/// under a Workspaces heading), "build" leaves build, deploy and logs, and (not shot) Enter goes
/// to build and closes the palette, its terminal with the keyboard.
fn palette_workspaces_scene() {
    use crate::palette_items::{Line, Scope};
    use std::cell::RefCell;
    use std::rc::Rc;

    let dims = size(px(960.0), px(600.0));
    for (slug, name) in SHOT_THEMES {
        let theme = shot_theme(name);
        for enter in [false, true] {
            let slot: Slot = Rc::new(RefCell::new(None));
            let build = palette_build(theme, slot.clone());
            let n = format!("palette-workspaces-{slug}");
            let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let ws = slotted(&slot);
                let ids = workspace_ids(cx, &ws);
                open_palette_on(cx, window, &ws, Scope::Workspaces);
                let (lines, scope, _) = palette_state(cx, &ws);
                assert_eq!(scope, Scope::Workspaces, "{n}");
                assert_eq!(lines[0], Line::Header("Workspaces".into()), "{n}");
                assert_eq!(
                    palette_names(cx, &ws),
                    [
                        "claude", "agent", "tests", "build", "codex", "deploy", "shell", "logs"
                    ],
                    "{n}: most urgent first"
                );
                cx.simulate_input(window, "build");
                settle(cx, window);
                assert_eq!(
                    palette_names(cx, &ws),
                    ["build", "deploy", "logs"],
                    "{n}: by name and group"
                );
                if enter {
                    press_keys(cx, window, "enter");
                    assert!(palette_of(cx, &ws).is_none(), "{n}: Enter closes it");
                    let focused = cx.read(|cx| ws.read(cx).model.focused_workspace());
                    assert_eq!(focused.map(|w| w.0), Some(ids[5]), "{n}: build is shown");
                    let active = cx.read(|cx| ws.read(cx).sessions.active().map(|s| s.label()));
                    assert_eq!(active.as_deref(), Some("build"), "{n}: its terminal");
                    assert!(
                        active_terminal_focused(cx, window, &ws),
                        "{n}: with the keys"
                    );
                }
                slot.borrow_mut().take();
            };
            let shot = render_with(&theme, dims, build, act);
            if !enter {
                save_shot("palette-workspaces", slug, &shot);
            }
        }
    }
}

/// Actions from the palette, in both themes:
/// - `palette-actions`: "split r" typed: Split Right first, ⌘D on its right;
/// - `palette-actions-prefix`: ">" typed on All: the Actions chip lit.
///
/// Panics unless Split Right is the first row with its keys drawn in `text_dim`, (not shot)
/// Enter splits shell's pane (two panes, a terminal with the keyboard, the palette closed), and
/// ">" puts the palette on Actions with that chip filled `row_active` (All's not).
fn palette_actions_scene() {
    use crate::palette_items::{Key, Scope, selectable};
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    let dims = size(px(960.0), px(600.0));
    for (slug, name) in SHOT_THEMES {
        let theme = shot_theme(name);
        let ui = theme.ui();
        for stage in ["shot", "enter", "prefix"] {
            let slot: Slot = Rc::new(RefCell::new(None));
            let build = palette_build(theme, slot.clone());
            let seen: Rc<Cell<Option<[gpui::Bounds<Pixels>; 2]>>> = Rc::default();
            let found = seen.clone();
            let n = format!("palette-actions-{stage}-{slug}");
            let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let ws = slotted(&slot);
                open_palette_on(cx, window, &ws, Scope::All);
                if stage == "prefix" {
                    cx.simulate_input(window, ">");
                    settle(cx, window);
                    let (_, scope, _) = palette_state(cx, &ws);
                    assert_eq!(scope, Scope::Actions, "{n}: > searches Actions");
                    let palette = palette_of(cx, &ws).expect("open");
                    found.set(cx.read(|cx| {
                        let p = palette.read(cx);
                        Some([p.chip_bounds(Scope::Actions)?, p.chip_bounds(Scope::All)?])
                    }));
                    slot.borrow_mut().take();
                    return;
                }
                cx.simulate_input(window, "split r");
                settle(cx, window);
                let (lines, _, selected) = palette_state(cx, &ws);
                let first = selectable(&lines)[0];
                assert_eq!(selected, Some(Key::Action("Split Right")), "{n}: first");
                let palette = palette_of(cx, &ws).expect("open");
                let keys = cx.read(|cx| palette.read(cx).action_keys("Split Right"));
                assert_eq!(keys.as_deref(), Some("\u{2318}D"), "{n}: its keys");
                let row = cx.read(|cx| palette.read(cx).row_bounds(first));
                found.set(row.map(|r| [r, r]));
                if stage == "enter" {
                    // A first listing in (the scenes list nothing themselves): Enter doesn't
                    // wait for one.
                    seed_palette_files(cx, &ws, &[]);
                    let before = shown_panes(cx, &ws).len();
                    press_keys(cx, window, "enter");
                    assert!(palette_of(cx, &ws).is_none(), "{n}: Enter closes it");
                    assert_eq!(before, 1, "{n}: one pane before");
                    assert_eq!(shown_panes(cx, &ws).len(), 2, "{n}: split");
                    let terminal = cx.read(|cx| ws.read(cx).focused_terminal());
                    assert!(terminal.is_some(), "{n}: a terminal is focused");
                    assert!(
                        active_terminal_focused(cx, window, &ws),
                        "{n}: with the keys"
                    );
                }
                slot.borrow_mut().take();
            };
            let shot = render_with(&theme, dims, build, act);
            match stage {
                "shot" => {
                    save_shot("palette-actions", slug, &shot);
                    let [row, _] = seen.get().expect("drawn");
                    // ⌘D, right-aligned in `text_dim`: some of its pixels in the row's last
                    // 60 pt (anti-aliasing aside, a glyph's stems are the colour itself).
                    let right = f32::from(row.right());
                    let ink: usize = (0..12)
                        .map(|k| f32::from(row.top()) + 8.0 + k as f32)
                        .map(|y| count_along(&shot, dims, ui.text_dim, y, right - 60.0, right))
                        .sum();
                    assert!(ink > 0, "palette-actions-{slug}: ⌘D is drawn ({ink})");
                }
                "prefix" => {
                    save_shot("palette-actions-prefix", slug, &shot);
                    let [actions, all] = seen.get().expect("drawn");
                    let fill = |b: gpui::Bounds<Pixels>| {
                        pixel_at(
                            &shot,
                            dims,
                            f32::from(b.left()) + 3.0,
                            f32::from(b.center().y),
                        )
                    };
                    assert!(near(fill(actions), ui.row_active), "{slug}: Actions lit");
                    assert!(near(fill(all), ui.panel), "{slug}: All isn't");
                }
                _ => {}
            }
        }
    }
}

/// The palette's keys and toggles (not shot): ↓ and ↑ move through the selectable rows (the
/// `… more` rows among them), round at both ends; Tab and ⇧Tab change scope, round; ⌘K on All
/// closes it, ⌘K then ⌘P puts it on Workspaces with the text kept; Esc closes it, the keyboard
/// back to the terminal; Enter on `… 3 more workspaces` goes to Workspaces and stays open; a
/// replaced palette's late Cancel is ignored; a click on a chip changes scope; ⌘K with ">"
/// typed on All still closes it (the toggle follows the chosen chip). Panics on the first that
/// doesn't hold.
fn palette_keys_scene() {
    use crate::palette_items::{Key, Kind, LISTING, Line, Scope};
    use gpui::Modifiers;
    use std::cell::RefCell;
    use std::rc::Rc;

    let dims = size(px(960.0), px(600.0));
    for (slug, name) in SHOT_THEMES {
        let theme = shot_theme(name);
        let slot: Slot = Rc::new(RefCell::new(None));
        let build = palette_build(theme, slot.clone());
        let n = format!("palette-keys-{slug}");
        let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
            let ws = slotted(&slot);
            let ids = workspace_ids(cx, &ws);
            let selected = |cx: &mut VisualTestAppContext| palette_state(cx, &ws).2;
            let scope = |cx: &mut VisualTestAppContext| palette_state(cx, &ws).1;
            press_keys(cx, window, "cmd-k");
            assert_eq!(scope(cx), Scope::All, "{n}: ⌘K opens All");
            assert_eq!(selected(cx), Some(Key::Workspace(ids[0])), "{n}");
            // Over the `… more` row, into Actions; then up past the top, round to the last.
            press_keys(cx, window, "down down down down down");
            assert_eq!(
                selected(cx),
                Some(Key::More(Kind::Workspace)),
                "{n}: ↓ to more"
            );
            press_keys(cx, window, "down");
            assert_eq!(
                selected(cx),
                Some(Key::Action("New Workspace")),
                "{n}: past it"
            );
            press_keys(cx, window, "up up up up up up up");
            assert_eq!(selected(cx), Some(Key::More(Kind::Action)), "{n}: ↑ wraps");
            press_keys(cx, window, "down");
            assert_eq!(selected(cx), Some(Key::Workspace(ids[0])), "{n}: ↓ wraps");
            // Scopes, round both ways.
            press_keys(cx, window, "tab");
            assert_eq!(scope(cx), Scope::Actions, "{n}: Tab");
            press_keys(cx, window, "shift-tab shift-tab");
            assert_eq!(scope(cx), Scope::Settings, "{n}: ⇧Tab ×2");
            // ⌘K elsewhere puts it on All; on All it closes.
            press_keys(cx, window, "cmd-k");
            assert_eq!(scope(cx), Scope::All, "{n}: ⌘K back to All");
            press_keys(cx, window, "cmd-k");
            assert!(palette_of(cx, &ws).is_none(), "{n}: ⌘K on All closes");
            // ⌘K, "co", ⌘P: Workspaces, with "co" still typed (codex only).
            press_keys(cx, window, "cmd-k");
            cx.simulate_input(window, "co");
            settle(cx, window);
            press_keys(cx, window, "cmd-p");
            assert_eq!(scope(cx), Scope::Workspaces, "{n}: ⌘P rescopes");
            assert_eq!(palette_names(cx, &ws), ["codex"], "{n}: the text kept");
            press_keys(cx, window, "escape");
            assert!(palette_of(cx, &ws).is_none(), "{n}: Esc closes");
            assert!(
                active_terminal_focused(cx, window, &ws),
                "{n}: Esc, keys back"
            );
            // Enter on `… 3 more workspaces`: Workspaces, still open.
            press_keys(cx, window, "cmd-k down down down down down enter");
            assert_eq!(scope(cx), Scope::Workspaces, "{n}: more goes to Workspaces");
            assert_eq!(palette_names(cx, &ws).len(), 8, "{n}: every workspace");
            // A palette closed and replaced still reporting "lost focus" (as its box does once the
            // next frame sees the focus gone): ignored, the new one stays open with the keys.
            let old = palette_of(cx, &ws).expect("open");
            press_keys(cx, window, "cmd-p");
            assert!(
                palette_of(cx, &ws).is_none(),
                "{n}: ⌘P on Workspaces closes"
            );
            press_keys(cx, window, "cmd-k");
            let new = palette_of(cx, &ws).expect("opened again");
            assert_ne!(old, new, "{n}: a new palette");
            cx.update(|cx| old.update(cx, |_, cx| cx.emit(crate::palette::PaletteEvent::Cancel)));
            settle(cx, window);
            assert_eq!(
                palette_of(cx, &ws),
                Some(new.clone()),
                "{n}: the old one's is ignored"
            );
            drop(old);
            press_keys(cx, window, "cmd-p");
            // A click on the Files chip: Files, its first listing not in yet (the screenshots list
            // nothing themselves).
            let palette = palette_of(cx, &ws).expect("open");
            let chip = cx.read(|cx| palette.read(cx).chip_bounds(Scope::Files));
            let chip = chip.expect("the Files chip is drawn");
            cx.simulate_click(window, chip.center(), Modifiers::none());
            settle(cx, window);
            let (lines, scope_now, _) = palette_state(cx, &ws);
            assert_eq!(scope_now, Scope::Files, "{n}: a chip click");
            assert_eq!(lines, [Line::Note(LISTING.into())], "{n}: the note");
            // ⌘K with ">" typed on All (Actions shown): the chosen chip is All, so ⌘K closes.
            press_keys(cx, window, "escape cmd-k");
            cx.simulate_input(window, ">");
            settle(cx, window);
            assert_eq!(scope(cx), Scope::Actions, "{n}: > shows Actions");
            press_keys(cx, window, "cmd-k");
            assert!(palette_of(cx, &ws).is_none(), "{n}: ⌘K on All (>) closes");
            // ">" typed on All (Actions shown), then a scope chosen: the ">" goes, the rest
            // stays. ⇧Tab steps from the shown Actions to All; the All chip does the same; Tab
            // goes on to Workspaces.
            let typed = |cx: &mut VisualTestAppContext| {
                let palette = palette_of(cx, &ws).expect("open");
                cx.read(|cx| palette.read(cx).box_text(cx))
            };
            press_keys(cx, window, "cmd-k");
            cx.simulate_input(window, "> split");
            settle(cx, window);
            assert_eq!(scope(cx), Scope::Actions, "{n}: > shows Actions");
            press_keys(cx, window, "shift-tab");
            assert_eq!(scope(cx), Scope::All, "{n}: ⇧Tab from Actions (>)");
            assert_eq!(typed(cx), "split", "{n}: ⇧Tab dropped the >");
            assert_eq!(
                palette_state(cx, &ws).2,
                Some(Key::Action("Split Right")),
                "{n}: All, split typed"
            );
            press_keys(cx, window, "cmd-backspace");
            cx.simulate_input(window, "> split");
            settle(cx, window);
            let palette = palette_of(cx, &ws).expect("open");
            let chip = cx.read(|cx| palette.read(cx).chip_bounds(Scope::All));
            cx.simulate_click(
                window,
                chip.expect("the All chip").center(),
                Modifiers::none(),
            );
            settle(cx, window);
            assert_eq!(scope(cx), Scope::All, "{n}: the All chip (>)");
            assert_eq!(typed(cx), "split", "{n}: the chip dropped the >");
            press_keys(cx, window, "cmd-backspace");
            cx.simulate_input(window, "> co");
            settle(cx, window);
            press_keys(cx, window, "tab");
            assert_eq!(scope(cx), Scope::Workspaces, "{n}: Tab from Actions (>)");
            assert_eq!(palette_names(cx, &ws), ["codex"], "{n}: co, without the >");
            slot.borrow_mut().take();
        };
        render_with(&theme, dims, build, act);
        println!("ui-shots: palette-keys-{slug} ok");
    }
}

/// An action from the palette lands where its key would (not shot): with a file tab focused,
/// "find" lists Find in File… and Enter opens the tab's find bar; with a terminal focused no
/// Find is listed; Rename Workspace… opens the rename box (the palette closed); Preferences…
/// reaches the app's handler exactly once. Panics on the first that doesn't hold.
fn palette_run_scene() {
    use crate::palette_items::{Key, Line, Scope};
    use gpui::Focusable;
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    let dims = size(px(960.0), px(600.0));
    // Its own name (another run's can't clash), removed at the end.
    let dir = std::env::temp_dir().join(format!("fuxx-ui-shots-palette-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("make the folder");
    let file = dir.join("notes.rs");
    std::fs::write(&file, "fn main() {\n    let found = 1;\n}\n").expect("write the file");
    let is_find =
        |l: &Line| matches!(l, Line::Item { key: Key::Action(a), .. } if a.starts_with("Find"));
    for (slug, name) in SHOT_THEMES {
        let theme = shot_theme(name);

        // A file tab focused.
        let slot: Slot = Rc::new(RefCell::new(None));
        let keep = slot.clone();
        let opened = file.clone();
        let build = move |window: &mut Window, cx: &mut App| -> AnyView {
            cx.set_global(sidebar_appearance(theme, cx));
            cx.bind_keys(crate::key_bindings());
            let ws = cx.new(|cx| {
                let mut ws = Workspace::for_shots(window, cx);
                ws.sidebar.hint_dismissed = true;
                ws.open_shot_session(
                    "api",
                    std::path::Path::new(API),
                    SHELL_TEXT,
                    None,
                    window,
                    cx,
                );
                ws.open_file_tab(opened, crate::layout_model::FileMode::Plain, window, cx);
                ws
            });
            *keep.borrow_mut() = Some(ws.clone());
            ws.into()
        };
        let n = format!("palette-run-file-{slug}");
        let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
            let ws = slotted(&slot);
            settle(cx, window);
            let tab = cx.read(|cx| ws.read(cx).focused_file_tab().cloned());
            let tab = tab.expect("the file tab is focused");
            // A first listing in (the scenes list nothing themselves): Enter doesn't wait.
            seed_palette_files(cx, &ws, &[]);
            open_palette_on(cx, window, &ws, Scope::All);
            cx.simulate_input(window, "find");
            settle(cx, window);
            let (lines, _, selected) = palette_state(cx, &ws);
            assert!(lines.iter().any(is_find), "{n}: Find is listed");
            assert_eq!(selected, Some(Key::Action("Find in File\u{2026}")), "{n}");
            press_keys(cx, window, "enter");
            assert!(palette_of(cx, &ws).is_none(), "{n}: closed");
            let (open, keys) = cx
                .update_window(window, |_, window, cx| {
                    let t = tab.read(cx);
                    (
                        t.find_open(),
                        t.focus_handle(cx).contains_focused(window, cx),
                    )
                })
                .expect("read the tab");
            assert!(open, "{n}: the file tab's find bar is open");
            assert!(keys, "{n}: the file tab has the keyboard");
            slot.borrow_mut().take();
        };
        render_with(&theme, dims, build, act);

        // A terminal focused.
        let slot: Slot = Rc::new(RefCell::new(None));
        let build = palette_build(theme, slot.clone());
        let n = format!("palette-run-terminal-{slug}");
        let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
            let ws = slotted(&slot);
            let preferences = Rc::new(Cell::new(0));
            let count = preferences.clone();
            cx.update(|cx| {
                cx.on_action(move |_: &crate::OpenPreferences, _| count.set(count.get() + 1));
            });
            // A first listing in (the scenes list nothing themselves): Enter doesn't wait.
            seed_palette_files(cx, &ws, &[]);
            open_palette_on(cx, window, &ws, Scope::All);
            cx.simulate_input(window, "find");
            settle(cx, window);
            let (lines, _, _) = palette_state(cx, &ws);
            assert!(!lines.iter().any(is_find), "{n}: no Find for a terminal");
            // Rename Workspace…: the rename box takes over, with the keyboard.
            press_keys(cx, window, "cmd-backspace");
            cx.simulate_input(window, "rename workspace");
            settle(cx, window);
            assert_eq!(
                palette_state(cx, &ws).2,
                Some(Key::Action("Rename Workspace\u{2026}")),
                "{n}"
            );
            press_keys(cx, window, "enter");
            let renaming = cx
                .update_window(window, |_, window, cx| {
                    let ws = ws.read(cx);
                    ws.renaming
                        .as_ref()
                        .map(|(_, input)| input.focus_handle(cx).is_focused(window))
                })
                .expect("read");
            assert_eq!(renaming, Some(true), "{n}: the rename box, with the keys");
            assert!(palette_of(cx, &ws).is_none(), "{n}: the palette closed");
            // Preferences…: the app's handler, once.
            open_palette_on(cx, window, &ws, Scope::All);
            cx.simulate_input(window, "preferences");
            settle(cx, window);
            assert_eq!(
                palette_state(cx, &ws).2,
                Some(Key::Action("Preferences\u{2026}")),
                "{n}"
            );
            press_keys(cx, window, "enter");
            assert_eq!(preferences.get(), 1, "{n}: Preferences… once");
            assert!(palette_of(cx, &ws).is_none(), "{n}: closed");
            slot.borrow_mut().take();
        };
        render_with(&theme, dims, build, act);
        println!("ui-shots: palette-run-{slug} ok");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// A list that moves while open (not shot): on Workspaces with tests (the third row) chosen,
/// logs starts waiting and moves above it: tests stays chosen; tests' terminal closes (as a
/// script's close would): the palette stays open, with the keyboard, and the first row is
/// chosen. Panics on the first that doesn't hold.
fn palette_refresh_scene() {
    use crate::palette_items::{Key, Scope};
    use gpui::Focusable;
    use std::cell::RefCell;
    use std::rc::Rc;

    let dims = size(px(960.0), px(600.0));
    for (slug, name) in SHOT_THEMES {
        let theme = shot_theme(name);
        let slot: Slot = Rc::new(RefCell::new(None));
        let build = palette_build(theme, slot.clone());
        let n = format!("palette-refresh-{slug}");
        let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
            let ws = slotted(&slot);
            let ids = workspace_ids(cx, &ws);
            open_palette_on(cx, window, &ws, Scope::Workspaces);
            press_keys(cx, window, "down down");
            assert_eq!(
                palette_state(cx, &ws).2,
                Some(Key::Workspace(ids[2])),
                "{n}"
            );
            // logs (#8) starts waiting: Waiting first, so it moves above tests.
            cx.update(|cx| {
                ws.update(cx, |ws, cx| {
                    let logs = ws.sessions.get_mut(7).expect("logs");
                    logs.status = Status::Waiting;
                    logs.status_since = std::time::Instant::now();
                    cx.notify();
                })
            });
            settle(cx, window);
            assert_eq!(
                palette_names(cx, &ws)[..4],
                ["claude", "agent", "logs", "tests"],
                "{n}: logs moved up"
            );
            assert_eq!(
                palette_state(cx, &ws).2,
                Some(Key::Workspace(ids[2])),
                "{n}: tests is still chosen"
            );
            // tests' only terminal closes.
            cx.update_window(window, |_, window, cx| {
                ws.update(cx, |ws, cx| {
                    let tests = ws
                        .sessions
                        .position(|s| s.label() == "tests")
                        .expect("tests");
                    ws.close_at(tests, window, cx);
                })
            })
            .expect("close tests");
            settle(cx, window);
            let palette = palette_of(cx, &ws).expect("{n}: still open");
            let keys = cx
                .update_window(window, |_, window, cx| {
                    palette.read(cx).focus_handle(cx).is_focused(window)
                })
                .expect("read");
            assert!(keys, "{n}: with the keyboard");
            assert!(
                !palette_names(cx, &ws).contains(&"tests".to_string()),
                "{n}"
            );
            assert_eq!(
                palette_state(cx, &ws).2,
                Some(Key::Workspace(ids[0])),
                "{n}: the first row is chosen"
            );
            slot.borrow_mut().take();
        };
        render_with(&theme, dims, build, act);
        println!("ui-shots: palette-refresh-{slug} ok");
    }
}

/// Recent (not shot): Split Right picked, the palette reopened shows it under Recent; workspace
/// #2 picked too, both, newest first; then src/view.rs and Cursor breathing glow picked: those
/// two first, newest first, before the others. Panics unless so.
fn palette_recent_scene() {
    use crate::palette_items::{Key, Line, Scope};
    use std::cell::RefCell;
    use std::rc::Rc;

    let dims = size(px(960.0), px(600.0));
    for (slug, name) in SHOT_THEMES {
        let theme = shot_theme(name);
        let slot: Slot = Rc::new(RefCell::new(None));
        let build = palette_build(theme, slot.clone());
        let n = format!("palette-recent-{slug}");
        let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
            let ws = slotted(&slot);
            let ids = workspace_ids(cx, &ws);
            let item = |key| Line::Item {
                key,
                positions: Vec::new(),
            };
            // A first listing in (the scenes list nothing themselves): Enter doesn't wait.
            seed_palette_files(cx, &ws, &[]);
            open_palette_on(cx, window, &ws, Scope::All);
            cx.simulate_input(window, "split right");
            settle(cx, window);
            press_keys(cx, window, "enter");
            open_palette_on(cx, window, &ws, Scope::All);
            let (lines, _, _) = palette_state(cx, &ws);
            assert_eq!(
                lines[..3],
                [
                    Line::Header("Recent".into()),
                    item(Key::Action("Split Right")),
                    Line::Header("Workspaces".into()),
                ],
                "{n}: Split Right under Recent"
            );
            press_keys(cx, window, "escape");
            open_palette_on(cx, window, &ws, Scope::Workspaces);
            cx.simulate_input(window, "2");
            settle(cx, window);
            assert_eq!(
                palette_state(cx, &ws).2,
                Some(Key::Workspace(ids[1])),
                "{n}"
            );
            press_keys(cx, window, "enter");
            open_palette_on(cx, window, &ws, Scope::All);
            let (lines, _, _) = palette_state(cx, &ws);
            assert_eq!(
                lines[..4],
                [
                    Line::Header("Recent".into()),
                    item(Key::Workspace(ids[1])),
                    item(Key::Action("Split Right")),
                    Line::Header("Workspaces".into()),
                ],
                "{n}: newest first"
            );
            // ⌘P (Workspaces, nothing typed): exactly the old list, no Recent.
            press_keys(cx, window, "escape");
            open_palette_on(cx, window, &ws, Scope::Workspaces);
            let (lines, _, _) = palette_state(cx, &ws);
            assert_eq!(lines[0], Line::Header("Workspaces".into()), "{n}: ⌘P");
            assert!(
                !lines.contains(&Line::Header("Recent".into())),
                "{n}: no Recent on ⌘P"
            );
            assert_eq!(
                palette_names(cx, &ws).len(),
                8,
                "{n}: every workspace, once"
            );
            // A file and a setting picked too: first, newest first.
            press_keys(cx, window, "escape");
            seed_palette_files(cx, &ws, &PALETTE_FILES);
            let _preferences = shot_preferences_window(cx);
            let view = Key::File(PathBuf::from(PALETTE_ROOT).join("src/view.rs"));
            let glow = Key::Setting("Cursor breathing glow");
            for (query, key) in [("view", &view), ("glow", &glow)] {
                open_palette_on(cx, window, &ws, Scope::All);
                cx.simulate_input(window, query);
                settle(cx, window);
                assert_eq!(palette_state(cx, &ws).2.as_ref(), Some(key), "{n}: {query}");
                press_keys(cx, window, "enter");
                assert!(palette_of(cx, &ws).is_none(), "{n}: {query} picked");
            }
            open_palette_on(cx, window, &ws, Scope::All);
            let (lines, _, _) = palette_state(cx, &ws);
            assert_eq!(
                lines[..6],
                [
                    Line::Header("Recent".into()),
                    item(glow.clone()),
                    item(view.clone()),
                    item(Key::Workspace(ids[1])),
                    item(Key::Action("Split Right")),
                    Line::Header("Workspaces".into()),
                ],
                "{n}: the setting, the file, then the rest"
            );
            // A listing without src/view.rs: still Recent, its file tab is open.
            press_keys(cx, window, "escape");
            seed_palette_files(cx, &ws, &["Cargo.toml"]);
            open_palette_on(cx, window, &ws, Scope::All);
            let (lines, _, _) = palette_state(cx, &ws);
            assert_eq!(
                lines[..3],
                [
                    Line::Header("Recent".into()),
                    item(glow.clone()),
                    item(view.clone()),
                ],
                "{n}: open in a tab, so still Recent"
            );
            slot.borrow_mut().take();
        };
        render_with(&theme, dims, build, act);
        println!("ui-shots: palette-recent-{slug} ok");
    }
}

/// The files the palette scenes list (M11j Task 5): fuxx-cli's, as if just listed for the
/// followed terminal's folder.
const PALETTE_ROOT: &str = "/w/fuxx-cli";
const PALETTE_FILES: [&str; 7] = [
    "Cargo.toml",
    "README.md",
    "docs/a.md",
    "src/main.rs",
    "src/palette.rs",
    "src/view.rs",
    "src/workspace.rs",
];

/// List `paths` under `PALETTE_ROOT` for the followed terminal's folder, as if just listed.
fn seed_palette_files(cx: &mut VisualTestAppContext, ws: &gpui::Entity<Workspace>, paths: &[&str]) {
    cx.update(|cx| {
        ws.update(cx, |ws, cx| {
            let (folder, ssh) = ws.palette_folder(cx).expect("a terminal folder");
            assert!(!ssh, "a local terminal is followed");
            let paths = paths.iter().map(|p| p.to_string()).collect();
            ws.palette_files
                .set_for_shots(folder, PathBuf::from(PALETTE_ROOT), paths);
            cx.notify();
        })
    });
}

/// An off-screen Preferences window, as if it were open (`preferences::open_at` switches it
/// instead of opening another).
fn shot_preferences_window(
    cx: &mut VisualTestAppContext,
) -> gpui::WindowHandle<crate::preferences::PreferencesView> {
    let handle = cx
        .open_offscreen_window(size(px(700.0), px(480.0)), |window, cx| {
            cx.new(|cx| crate::preferences::PreferencesView::new(window, cx))
        })
        .expect("open Preferences off screen");
    cx.update(|cx| crate::preferences::set_window_for_shots(handle, cx));
    cx.run_until_parked();
    handle
}

/// The rightmost x (points) along the row's middle band where a pixel is colour `c`, between
/// `x0` and `x1`; none if there is none.
fn rightmost(
    shot: &Shot,
    dims: Size<Pixels>,
    c: alacritty_terminal::vte::ansi::Rgb,
    row: gpui::Bounds<Pixels>,
    x0: f32,
    x1: f32,
) -> Option<f32> {
    let scale = shot.width as f32 / f32::from(dims.width);
    let (top, bottom) = (f32::from(row.top()) + 4.0, f32::from(row.bottom()) - 4.0);
    let mut best: Option<f32> = None;
    let mut y = top;
    while y < bottom {
        let (a, b) = ((x0 * scale) as usize, (x1 * scale) as usize);
        for dx in a..b {
            let x = dx as f32 / scale;
            if near(pixel_at(shot, dims, x, y), c) && best.is_none_or(|r| x > r) {
                best = Some(x);
            }
        }
        y += 1.0 / scale;
    }
    best
}

/// The most inked pixel (the furthest from `bg`) in the row's middle band between `x0` and `x1`.
fn strongest(
    shot: &Shot,
    dims: Size<Pixels>,
    bg: alacritty_terminal::vte::ansi::Rgb,
    row: gpui::Bounds<Pixels>,
    x0: f32,
    x1: f32,
) -> Option<[u8; 3]> {
    let scale = shot.width as f32 / f32::from(dims.width);
    let (top, bottom) = (f32::from(row.top()) + 4.0, f32::from(row.bottom()) - 4.0);
    let mut best: Option<[u8; 3]> = None;
    for dy in (top * scale) as usize..(bottom * scale) as usize {
        for dx in (x0 * scale) as usize..(x1 * scale) as usize {
            let p = pixel_at(shot, dims, dx as f32 / scale, dy as f32 / scale);
            if best.is_none_or(|b| gap(p, bg) > gap(b, bg)) {
                best = Some(p);
            }
        }
    }
    best
}

/// How many pixels in the box are colour `c` (within `tol`).
fn count_in(
    shot: &Shot,
    dims: Size<Pixels>,
    c: alacritty_terminal::vte::ansi::Rgb,
    tol: u8,
    (x0, x1): (f32, f32),
    (y0, y1): (f32, f32),
) -> usize {
    let scale = shot.width as f32 / f32::from(dims.width);
    let mut count = 0;
    for dy in (y0 * scale) as usize..(y1 * scale) as usize {
        for dx in (x0 * scale) as usize..(x1 * scale) as usize {
            if near_by(
                pixel_at(shot, dims, dx as f32 / scale, dy as f32 / scale),
                c,
                tol,
            ) {
                count += 1;
            }
        }
    }
    count
}

/// Files in the palette (M11j Task 5), over `switcher_workspace` with fuxx-cli's files listed for
/// shell's folder, in both themes:
/// - `palette-files`: "view" typed on All: `Files in fuxx-cli` first, src/view.rs chosen;
/// - `palette-files-empty`: the Files scope, nothing typed: `Type to search files in fuxx-cli`.
///
/// Panics unless so (Enter typed straight after a letter waits for that letter's ranking),
/// src/view.rs's icon has a pixel in `palette_file_ink(Red)` in its slot and
/// its folder (`src`) is drawn in `text_faint` right of its name; and (not shot) Enter opens
/// src/view.rs in a Plain file tab in the focused pane, the palette closed; with logs (an SSH
/// terminal) followed, Files says `SSH_NOTE`; with a first listing running, `LISTING` (and on All
/// no Files section, no note).
fn palette_files_scene() {
    use crate::layout_model::{FileMode, TabKind};
    use crate::palette::palette_file_ink;
    use crate::palette_items::{Key, LISTING, Line, SSH_NOTE, Scope};
    use crate::right_sidebar::FileKind;
    use crate::sidebar::MARK_SLOT;
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    let dims = size(px(960.0), px(600.0));
    let view = PathBuf::from(PALETTE_ROOT).join("src/view.rs");
    for (slug, name) in SHOT_THEMES {
        let theme = shot_theme(name);
        let ui = theme.ui();
        for stage in [
            "shot",
            "enter",
            "empty",
            "ssh",
            "listing",
            "typing",
            "enter-early",
            "enter-retyped",
            "enter-listing",
            "enter-rescoped",
        ] {
            let slot: Slot = Rc::new(RefCell::new(None));
            let build = palette_build(theme, slot.clone());
            let seen: Rc<Cell<Option<gpui::Bounds<Pixels>>>> = Rc::default();
            let found = seen.clone();
            let n = format!("palette-files-{stage}-{slug}");
            let view = view.clone();
            let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let ws = slotted(&slot);
                let lines = |cx: &mut VisualTestAppContext| palette_state(cx, &ws).0;
                match stage {
                    "ssh" => {
                        // logs (#8) is an SSH terminal to build-box: shown, so followed.
                        cx.update_window(window, |_, window, cx| {
                            ws.update(cx, |ws, cx| ws.select(7, window, cx))
                        })
                        .expect("select logs");
                        settle(cx, window);
                        let ssh = cx.read(|cx| ws.read(cx).palette_folder(cx).map(|f| f.1));
                        assert_eq!(ssh, Some(true), "{n}: an SSH terminal followed");
                        open_palette_on(cx, window, &ws, Scope::Files);
                        assert_eq!(lines(cx), [Line::Note(SSH_NOTE.into())], "{n}");
                        cx.simulate_input(window, "main");
                        settle(cx, window);
                        assert_eq!(lines(cx), [Line::Note(SSH_NOTE.into())], "{n}: typed");
                    }
                    "enter-listing" | "enter-rescoped" => {
                        // The first listing still running: Enter waits for it (and the
                        // query's ranking), then picks the best for the query.
                        cx.update(|cx| {
                            ws.update(cx, |ws, cx| {
                                let (folder, _) = ws.palette_folder(cx).expect("a folder");
                                ws.palette_files.started(&folder);
                            })
                        });
                        open_palette_on(cx, window, &ws, Scope::All);
                        if stage == "enter-rescoped" {
                            // A waiting Enter then Tab: the Enter was for All, so nothing is
                            // picked on Actions (Split Right isn't run), nor once listed.
                            cx.simulate_input(window, "split right");
                            settle(cx, window);
                            press_keys(cx, window, "enter");
                            assert!(palette_of(cx, &ws).is_some(), "{n}: Enter waits");
                            press_keys(cx, window, "tab");
                            assert_eq!(palette_state(cx, &ws).1, Scope::Actions, "{n}: Tab");
                            assert!(palette_of(cx, &ws).is_some(), "{n}: open after Tab");
                            assert_eq!(shown_panes(cx, &ws).len(), 1, "{n}: nothing split");
                            seed_palette_files(cx, &ws, &PALETTE_FILES);
                            settle(cx, window);
                            assert!(palette_of(cx, &ws).is_some(), "{n}: open once listed");
                            assert_eq!(shown_panes(cx, &ws).len(), 1, "{n}: still one pane");
                            slot.borrow_mut().take();
                            return;
                        }
                        cx.simulate_input(window, "view");
                        settle(cx, window);
                        press_keys(cx, window, "enter");
                        assert!(palette_of(cx, &ws).is_some(), "{n}: Enter waits");
                        seed_palette_files(cx, &ws, &PALETTE_FILES);
                        settle(cx, window);
                        assert!(palette_of(cx, &ws).is_none(), "{n}: picked once listed");
                        let kind = cx.read(|cx| {
                            let ws = ws.read(cx);
                            let tab = ws.model.focused_tab()?;
                            ws.model.tab(tab).map(|t| t.kind.clone())
                        });
                        assert_eq!(
                            kind,
                            Some(TabKind::File {
                                path: view.clone(),
                                mode: FileMode::Plain
                            }),
                            "{n}: the best for \"view\""
                        );
                    }
                    "enter-early" | "enter-retyped" => {
                        // "vie" ranked (vie/w.md first), then "w" and Enter at once, before the
                        // "view" ranking has run: Enter waits for it and opens src/view.rs. A
                        // letter typed after that Enter (before the answer) drops it.
                        seed_palette_files(cx, &ws, &["vie/w.md", "src/view.rs"]);
                        open_palette_on(cx, window, &ws, Scope::Files);
                        cx.simulate_input(window, "vie");
                        settle(cx, window);
                        let vie_w = Key::File(PathBuf::from(PALETTE_ROOT).join("vie/w.md"));
                        assert_eq!(palette_state(cx, &ws).2, Some(vie_w), "{n}: vie");
                        // Dispatched without running the executor: no ranking runs between.
                        cx.dispatch_keystroke(
                            window,
                            gpui::Keystroke {
                                modifiers: gpui::Modifiers::default(),
                                key: "w".into(),
                                key_char: Some("w".into()),
                            },
                        );
                        cx.dispatch_keystroke(
                            window,
                            gpui::Keystroke::parse("enter").expect("a keystroke"),
                        );
                        if stage == "enter-retyped" {
                            cx.dispatch_keystroke(
                                window,
                                gpui::Keystroke {
                                    modifiers: gpui::Modifiers::default(),
                                    key: "s".into(),
                                    key_char: Some("s".into()),
                                },
                            );
                            settle(cx, window);
                            assert!(palette_of(cx, &ws).is_some(), "{n}: still open");
                            assert_eq!(
                                palette_state(cx, &ws).2,
                                Some(Key::File(view.clone())),
                                "{n}: \"views\" ranked"
                            );
                            slot.borrow_mut().take();
                            return;
                        }
                        settle(cx, window);
                        assert!(palette_of(cx, &ws).is_none(), "{n}: Enter picked");
                        let kind = cx.read(|cx| {
                            let ws = ws.read(cx);
                            let tab = ws.model.focused_tab()?;
                            ws.model.tab(tab).map(|t| t.kind.clone())
                        });
                        assert_eq!(
                            kind,
                            Some(TabKind::File {
                                path: view.clone(),
                                mode: FileMode::Plain
                            }),
                            "{n}: the best for \"view\", not the last answer's"
                        );
                    }
                    "typing" => {
                        // Typed a letter at a time, each ranking in before the next letter:
                        // "vie" puts vie/w.md first; "view" puts src/view.rs first, and that is
                        // what's chosen (not the row the last answer had first).
                        seed_palette_files(cx, &ws, &["vie/w.md", "src/view.rs"]);
                        open_palette_on(cx, window, &ws, Scope::Files);
                        let vie_w = Key::File(PathBuf::from(PALETTE_ROOT).join("vie/w.md"));
                        for letter in ["v", "i", "e"] {
                            cx.simulate_input(window, letter);
                            settle(cx, window);
                        }
                        assert_eq!(palette_state(cx, &ws).2, Some(vie_w.clone()), "{n}: vie");
                        cx.simulate_input(window, "w");
                        settle(cx, window);
                        let (lines, _, selected) = palette_state(cx, &ws);
                        assert!(
                            lines
                                .iter()
                                .any(|l| matches!(l, Line::Item { key, .. } if *key == vie_w)),
                            "{n}: vie/w.md still listed"
                        );
                        assert_eq!(
                            selected,
                            Some(Key::File(view.clone())),
                            "{n}: the best chosen"
                        );
                        // Moved by hand, the choice stays put when an answer comes in.
                        press_keys(cx, window, "down");
                        assert_eq!(palette_state(cx, &ws).2, Some(vie_w.clone()), "{n}: ↓");
                        // A new listing (the 10 s re-list): ranked again, the choice kept.
                        seed_palette_files(cx, &ws, &["vie/w.md", "src/view.rs"]);
                        settle(cx, window);
                        assert_eq!(palette_state(cx, &ws).2, Some(vie_w), "{n}: kept");
                    }
                    "listing" => {
                        cx.update(|cx| {
                            ws.update(cx, |ws, cx| {
                                let (folder, _) = ws.palette_folder(cx).expect("a folder");
                                ws.palette_files.started(&folder);
                            })
                        });
                        open_palette_on(cx, window, &ws, Scope::Files);
                        assert_eq!(lines(cx), [Line::Note(LISTING.into())], "{n}");
                        cx.simulate_input(window, "main");
                        settle(cx, window);
                        assert_eq!(lines(cx), [Line::Note(LISTING.into())], "{n}: typed");
                        // On All: no Files section and no note while listing.
                        press_keys(cx, window, "tab tab");
                        assert_eq!(palette_state(cx, &ws).1, Scope::All, "{n}");
                        let all = lines(cx);
                        assert!(
                            !all.iter().any(|l| matches!(l, Line::Note(_))),
                            "{n}: {all:?}"
                        );
                        assert!(
                            !all.iter()
                                .any(|l| matches!(l, Line::Header(h) if h.starts_with("Files"))),
                            "{n}: {all:?}"
                        );
                    }
                    _ => {
                        seed_palette_files(cx, &ws, &PALETTE_FILES);
                        if stage == "empty" {
                            open_palette_on(cx, window, &ws, Scope::Files);
                            assert_eq!(
                                lines(cx),
                                [Line::Note("Type to search files in fuxx-cli".into())],
                                "{n}"
                            );
                            slot.borrow_mut().take();
                            return;
                        }
                        open_palette_on(cx, window, &ws, Scope::All);
                        cx.simulate_input(window, "view");
                        settle(cx, window);
                        let (lines, _, selected) = palette_state(cx, &ws);
                        assert_eq!(lines[0], Line::Header("Files in fuxx-cli".into()), "{n}");
                        assert!(
                            matches!(&lines[1], Line::Item { key: Key::File(p), .. } if *p == view),
                            "{n}: {:?}",
                            lines[1]
                        );
                        assert_eq!(selected, Some(Key::File(view.clone())), "{n}: chosen");
                        let palette = palette_of(cx, &ws).expect("open");
                        found.set(cx.read(|cx| palette.read(cx).row_bounds(1)));
                        if stage == "enter" {
                            press_keys(cx, window, "enter");
                            assert!(palette_of(cx, &ws).is_none(), "{n}: Enter closes it");
                            let kind = cx.read(|cx| {
                                let ws = ws.read(cx);
                                let tab = ws.model.focused_tab()?;
                                ws.model.tab(tab).map(|t| t.kind.clone())
                            });
                            assert_eq!(
                                kind,
                                Some(TabKind::File {
                                    path: view.clone(),
                                    mode: FileMode::Plain
                                }),
                                "{n}: a Plain file tab, focused"
                            );
                        }
                    }
                }
                slot.borrow_mut().take();
            };
            let shot = render_with(&theme, dims, build, act);
            match stage {
                "shot" => {
                    save_shot("palette-files", slug, &shot);
                    let row = seen.get().expect("the row is drawn");
                    let n = format!("palette-files-{slug}");
                    let left = f32::from(row.left()) + 8.0;
                    let (top, bottom) = (f32::from(row.top()), f32::from(row.bottom()));
                    let ink = palette_file_ink(FileKind::Red, &theme);
                    let icon =
                        count_in(&shot, dims, ink, 6, (left, left + MARK_SLOT), (top, bottom));
                    assert!(icon > 0, "{n}: the icon in its kind's ink ({icon})");
                    // The name (`text`) then, right of it, its folder in `text_faint`.
                    let names = left + MARK_SLOT;
                    let right = f32::from(row.right()) - 8.0;
                    let name_end = rightmost(&shot, dims, ui.text, row, names, right)
                        .expect("the name is drawn");
                    // Small text at 1×: its darkest stem, closer to `text_faint` than to `text`.
                    let ink = strongest(&shot, dims, ui.row_active, row, name_end + 4.0, right)
                        .expect("right of the name");
                    assert!(
                        gap(ink, ui.row_active) >= 24
                            && gap(ink, ui.text_faint) < gap(ink, ui.text),
                        "{n}: the folder, in text_faint, right of the name ({name_end}, {ink:?})"
                    );
                }
                "empty" => {
                    save_shot("palette-files-empty", slug, &shot);
                }
                _ => {}
            }
            println!("ui-shots: palette-files-{stage}-{slug} ok");
        }
    }
}

/// Files listed again while the palette is open (M11j Task 5 fix), not shot: a terminal whose
/// folder changes while Files is shown (as a `cd` would) gets the new folder listed; a listing
/// asked for while another ran (and so turned away) is listed once that one ends. Real listings
/// of two made-up folders (no repo: the walk). Panics unless each ends on its folder's
/// "Type to search" note, never stuck on `LISTING`.
fn palette_relist_scene() {
    use crate::palette_items::{Line, Scope};
    use std::cell::RefCell;
    use std::rc::Rc;

    let dims = size(px(960.0), px(600.0));
    let folder = |name: &str| {
        // Its own name (another run's can't clash), removed at the end.
        let dir = std::env::temp_dir().join(format!(
            "fuxx-ui-shots-relist-{}-{name}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("make the folder");
        std::fs::write(dir.join(format!("{name}.txt")), name).expect("write a file");
        std::fs::canonicalize(&dir).expect("its real path")
    };
    let (a, b) = (folder("a"), folder("b"));
    let note = |dir: &std::path::Path| {
        let name = dir.file_name().and_then(|n| n.to_str()).unwrap_or("");
        Line::Note(format!("Type to search files in {name}"))
    };
    let theme = shot_theme("Catppuccin Mocha");
    for stage in ["moved", "turned-away"] {
        let slot: Slot = Rc::new(RefCell::new(None));
        let keep = slot.clone();
        let start = a.clone();
        let build = move |window: &mut Window, cx: &mut App| -> AnyView {
            cx.set_global(sidebar_appearance(theme, cx));
            cx.bind_keys(crate::key_bindings());
            let ws = cx.new(|cx| {
                let mut ws = Workspace::for_shots(window, cx);
                ws.sidebar.hint_dismissed = true;
                ws.open_shot_session("api", &start, "", None, window, cx);
                ws.palette_files.frozen = false;
                ws
            });
            *keep.borrow_mut() = Some(ws.clone());
            ws.into()
        };
        let n = format!("palette-relist-{stage}");
        let (a, b) = (a.clone(), b.clone());
        let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
            let ws = slotted(&slot);
            let move_to = |ws: &mut Workspace, dir: &std::path::Path| {
                ws.sessions.get_mut(0).expect("api").project = Some(dir.to_path_buf());
            };
            if stage == "moved" {
                open_palette_on(cx, window, &ws, Scope::Files);
                settle(cx, window);
                assert_eq!(palette_state(cx, &ws).0, [note(&a)], "{n}: a listed");
                cx.update(|cx| {
                    ws.update(cx, |ws, cx| {
                        move_to(ws, &b);
                        cx.notify();
                    })
                });
                settle(cx, window);
                settle(cx, window);
            } else {
                // a's listing starts; before it runs, the terminal moves to b and the palette
                // opens there (its listing turned away: a's is running).
                cx.update_window(window, |_, window, cx| {
                    ws.update(cx, |ws, cx| {
                        ws.list_palette_files(cx);
                        move_to(ws, &b);
                        ws.open_palette(Scope::Files, window, cx);
                    })
                })
                .expect("open");
                settle(cx, window);
                settle(cx, window);
            }
            assert_eq!(palette_state(cx, &ws).0, [note(&b)], "{n}: b listed");
            slot.borrow_mut().take();
        };
        render_with(&theme, dims, build, act);
        println!("ui-shots: palette-relist-{stage} ok");
    }
    for dir in [&a, &b] {
        let _ = std::fs::remove_dir_all(dir);
    }
}

/// Settings in the palette (M11j Task 5), in both themes: `palette-settings`: "glow" typed on
/// All: Cursor breathing glow chosen, `Appearance` on its right in `text_faint`, the gear in its
/// slot in `text_dim`.
///
/// Panics unless so, and (not shot) Enter closes the palette, makes Appearance the section
/// Preferences opens on (`LastSection`) and switches the open Preferences window to it.
fn palette_settings_scene() {
    use crate::palette_items::{Key, Scope, selectable};
    use crate::preferences::Section;
    use crate::sidebar::MARK_SLOT;
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    let dims = size(px(960.0), px(600.0));
    for (slug, name) in SHOT_THEMES {
        let theme = shot_theme(name);
        let ui = theme.ui();
        for enter in [false, true] {
            let slot: Slot = Rc::new(RefCell::new(None));
            let build = palette_build(theme, slot.clone());
            let seen: Rc<Cell<Option<gpui::Bounds<Pixels>>>> = Rc::default();
            let found = seen.clone();
            let n = format!("palette-settings-{slug}-{enter}");
            let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let ws = slotted(&slot);
                let preferences = enter.then(|| shot_preferences_window(cx));
                if enter {
                    // A first listing in (the scenes list nothing themselves): Enter doesn't
                    // wait for one.
                    seed_palette_files(cx, &ws, &[]);
                }
                open_palette_on(cx, window, &ws, Scope::All);
                cx.simulate_input(window, "glow");
                settle(cx, window);
                let (lines, _, selected) = palette_state(cx, &ws);
                let first = selectable(&lines)[0];
                assert!(
                    matches!(
                        &lines[first],
                        crate::palette_items::Line::Item { key: Key::Setting(name), .. }
                            if *name == "Cursor breathing glow"
                    ),
                    "{n}: first: {:?}",
                    lines[first]
                );
                assert_eq!(selected, Some(Key::Setting("Cursor breathing glow")), "{n}");
                let palette = palette_of(cx, &ws).expect("open");
                found.set(cx.read(|cx| palette.read(cx).row_bounds(first)));
                if let Some(preferences) = preferences {
                    let before = cx.read(crate::preferences::last_section_for_shots);
                    assert_ne!(before, Some(Section::Appearance), "{n}: not yet");
                    press_keys(cx, window, "enter");
                    assert!(palette_of(cx, &ws).is_none(), "{n}: Enter closes it");
                    let last = cx.read(crate::preferences::last_section_for_shots);
                    assert_eq!(last, Some(Section::Appearance), "{n}: LastSection");
                    let shown = cx
                        .read(|cx| preferences.read(cx).map(|p| p.section_for_shots()))
                        .expect("read Preferences");
                    assert_eq!(shown, Section::Appearance, "{n}: the open window switched");
                }
                slot.borrow_mut().take();
            };
            let shot = render_with(&theme, dims, build, act);
            if !enter {
                save_shot("palette-settings", slug, &shot);
                let row = seen.get().expect("the row is drawn");
                let n = format!("palette-settings-{slug}");
                let (top, bottom) = (f32::from(row.top()), f32::from(row.bottom()));
                let left = f32::from(row.left()) + 8.0;
                let gear = count_in(
                    &shot,
                    dims,
                    ui.text_dim,
                    6,
                    (left, left + MARK_SLOT),
                    (top, bottom),
                );
                assert!(gear > 0, "{n}: the gear, text_dim ({gear})");
                // `Appearance`, right-aligned in `text_faint`, in the row's last 90 pt.
                let right = f32::from(row.right()) - 8.0;
                let faint = rightmost(&shot, dims, ui.text_faint, row, right - 90.0, right + 1.0);
                assert!(
                    faint.is_some_and(|x| x > right - 10.0),
                    "{n}: Appearance ({faint:?})"
                );
            }
            println!("ui-shots: palette-settings-{slug}-{enter} ok");
        }
    }
}

/// The palette in a narrow window (M11j Task 5), both themes: `palette-narrow`: 480 pt wide, the
/// Files scope with "abcdefghij" typed, the one file a name 60 capitals long.
///
/// Panics unless the panel keeps 16 pt from the window's right edge, every row ends inside it,
/// and the long name is cut: drawn up to the row's right padding and nothing in `text` past it
/// (in the padding, the panel's edge or the window beyond); and (not shot) a folder too long for
/// its row is cut at its start (its end shows).
fn palette_narrow_scene() {
    use crate::palette_items::{Scope, selectable};
    use std::cell::RefCell;
    use std::rc::Rc;

    let width = 480.0_f32;
    let dims = size(px(width), px(600.0));
    // 60 capitals: well past what the row can hold (lower case nearly fits).
    let long = format!("src/{}.rs", "ABCDEFGHIJ".repeat(6));
    for (slug, name) in SHOT_THEMES {
        let theme = shot_theme(name);
        let ui = theme.ui();
        // One terminal with nothing on its screen, so only the palette draws text there.
        let narrow = |keep: Slot| {
            move |window: &mut Window, cx: &mut App| -> AnyView {
                cx.set_global(sidebar_appearance(theme, cx));
                cx.bind_keys(crate::key_bindings());
                let ws = cx.new(|cx| {
                    let mut ws = Workspace::for_shots(window, cx);
                    ws.sidebar.hint_dismissed = true;
                    ws.open_shot_session("api", std::path::Path::new(API), "", None, window, cx);
                    ws
                });
                *keep.borrow_mut() = Some(ws.clone());
                ws.into()
            }
        };
        let slot: Slot = Rc::new(RefCell::new(None));
        let build = narrow(slot.clone());
        type Seen = (
            gpui::Bounds<Pixels>,
            Vec<gpui::Bounds<Pixels>>,
            gpui::Bounds<Pixels>,
        );
        let seen: Rc<RefCell<Option<Seen>>> = Rc::default();
        let found = seen.clone();
        let n = format!("palette-narrow-{slug}");
        let long = long.clone();
        let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
            let ws = slotted(&slot);
            seed_palette_files(cx, &ws, &[long.as_str(), "src/main.rs"]);
            open_palette_on(cx, window, &ws, Scope::Files);
            cx.simulate_input(window, "abcdefghij");
            settle(cx, window);
            let (lines, _, _) = palette_state(cx, &ws);
            let first = selectable(&lines)[0];
            let palette = palette_of(cx, &ws).expect("open");
            *found.borrow_mut() = cx.read(|cx| {
                let p = palette.read(cx);
                let rows = (0..lines.len()).filter_map(|at| p.row_bounds(at)).collect();
                Some((p.panel_bounds()?, rows, p.row_bounds(first)?))
            });
            assert_eq!(
                lines.len(),
                2,
                "{n}: the heading and the one file: {lines:?}"
            );
            slot.borrow_mut().take();
        };
        let shot = render_with(&theme, dims, build, act);
        save_shot("palette-narrow", slug, &shot);
        let n = format!("palette-narrow-{slug}");
        let (panel, rows, row) = seen.borrow().clone().expect("drawn");
        // The panel's outer edge is 1 pt out from what its absolute child fills.
        let panel_right = f32::from(panel.right()) + 1.0;
        assert!(
            panel_right <= width - 16.0 + 0.5,
            "{n}: the panel ends at {panel_right}"
        );
        for b in &rows {
            assert!(
                f32::from(b.right()) <= f32::from(panel.right()) + 0.5,
                "{n}: a row past the panel: {b:?}"
            );
        }
        let padding = f32::from(row.right()) - 8.0;
        let end = rightmost(&shot, dims, ui.text, row, f32::from(row.left()), width)
            .expect("the name is drawn");
        // The cut leaves the ellipsis a little short of the padding.
        assert!(
            end > padding - 40.0,
            "{n}: the name runs to the row's end ({end})"
        );
        assert!(
            end <= padding + 0.5,
            "{n}: nothing in text past the row's padding ({end})"
        );
        // On one line (a name that wrapped would spill out of its row).
        let scale = shot.width as f32 / f32::from(dims.width);
        let (x0, x1) = (f32::from(row.left()) + 30.0, padding);
        let band = ((f32::from(row.top()) - 12.0) * scale) as usize
            ..((f32::from(row.bottom()) + 12.0) * scale) as usize;
        let inked: Vec<f32> = band
            .map(|dy| dy as f32 / scale)
            .filter(|&y| count_along(&shot, dims, ui.text, y, x0, x1) > 0)
            .collect();
        let (top, bottom) = (inked[0], inked[inked.len() - 1]);
        assert!(
            bottom - top <= 16.0,
            "{n}: the name is one line ({top}..{bottom})"
        );

        // A folder too long for its row is cut at its start, so its end shows: dots then wide
        // letters show the letters (and the other way round, the dots).
        let (thin, wide) = (".".repeat(200), "W".repeat(60));
        let wide_end = format!("{thin}/{wide}/a.rs");
        let thin_end = format!("{wide}/{thin}/b.rs");
        let slot: Slot = Rc::new(RefCell::new(None));
        let build = narrow(slot.clone());
        let rows: Rc<RefCell<Vec<gpui::Bounds<Pixels>>>> = Rc::default();
        let found = rows.clone();
        let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
            let ws = slotted(&slot);
            seed_palette_files(cx, &ws, &[wide_end.as_str(), thin_end.as_str()]);
            open_palette_on(cx, window, &ws, Scope::Files);
            cx.simulate_input(window, ".rs");
            settle(cx, window);
            let (lines, _, _) = palette_state(cx, &ws);
            let palette = palette_of(cx, &ws).expect("open");
            // a.rs's row, then b.rs's.
            let at = |name: &str| {
                lines
                    .iter()
                    .position(|l| matches!(l, crate::palette_items::Line::Item { key: crate::palette_items::Key::File(p), .. } if p.ends_with(name)))
                    .expect("listed")
            };
            let (a, b) = (at("a.rs"), at("b.rs"));
            *found.borrow_mut() = cx.read(|cx| {
                let p = palette.read(cx);
                [p.row_bounds(a), p.row_bounds(b)]
                    .into_iter()
                    .flatten()
                    .collect()
            });
            slot.borrow_mut().take();
        };
        let shot = render_with(&theme, dims, build, act);
        let rows = rows.borrow().clone();
        assert_eq!(rows.len(), 2, "{n}: both rows drawn");
        // Inked pixels in each row's folder, past its name (`a.rs`: under 40 pt with the slot).
        let ink = |row: gpui::Bounds<Pixels>| {
            let scale = shot.width as f32 / f32::from(dims.width);
            let (x0, x1) = (f32::from(row.left()) + 70.0, f32::from(row.right()) - 8.0);
            let (y0, y1) = (f32::from(row.top()) + 4.0, f32::from(row.bottom()) - 4.0);
            let bg = pixel_at(
                &shot,
                dims,
                f32::from(row.left()) + 3.0,
                f32::from(row.center().y),
            );
            let bg = alacritty_terminal::vte::ansi::Rgb {
                r: bg[0],
                g: bg[1],
                b: bg[2],
            };
            let mut count = 0;
            for dy in (y0 * scale) as usize..(y1 * scale) as usize {
                for dx in (x0 * scale) as usize..(x1 * scale) as usize {
                    if gap(
                        pixel_at(&shot, dims, dx as f32 / scale, dy as f32 / scale),
                        bg,
                    ) >= 40
                    {
                        count += 1;
                    }
                }
            }
            count
        };
        let (wide_shown, thin_shown) = (ink(rows[0]), ink(rows[1]));
        assert!(
            wide_shown > 2 * thin_shown,
            "{n}: the folders' ends show ({wide_shown} vs {thin_shown})"
        );
    }
}

/// Reduce motion and the palette (M11j Task 5): `palette-motion`: Workspaces open over
/// `switcher_workspace` (codex and deploy Working), Reduce motion on.
///
/// Panics unless, with Reduce motion on, two frames 200 ms apart (the marks' clock moved on
/// between them) are pixel-identical in the panel; and (not shot) with it off, they differ (so
/// the check can tell).
fn palette_motion_scene() {
    use crate::palette_items::Scope;
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::time::Duration;

    let dims = size(px(960.0), px(600.0));
    for (slug, name) in SHOT_THEMES {
        let theme = shot_theme(name);
        for still in [true, false] {
            let slot: Slot = Rc::new(RefCell::new(None));
            let inner = palette_build(theme, slot.clone());
            let build = move |window: &mut Window, cx: &mut App| -> AnyView {
                cx.set_reduce_motion(still);
                inner(window, cx)
            };
            let n = format!("palette-motion-{slug}-{still}");
            let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let ws = slotted(&slot);
                open_palette_on(cx, window, &ws, Scope::Workspaces);
                let palette = palette_of(cx, &ws).expect("open");
                let panel = cx
                    .read(|cx| palette.read(cx).panel_bounds())
                    .expect("the panel is drawn");
                let grab = |cx: &mut VisualTestAppContext| {
                    let image = cx.capture_screenshot(window).expect("capture");
                    let (w, raw) = (image.width() as usize, image.as_raw().to_vec());
                    let scale = w as f32 / f32::from(dims.width);
                    let (x0, x1) = (
                        (f32::from(panel.left()) * scale) as usize,
                        (f32::from(panel.right()) * scale) as usize,
                    );
                    let (y0, y1) = (
                        (f32::from(panel.top()) * scale) as usize,
                        (f32::from(panel.bottom()) * scale) as usize,
                    );
                    (y0..y1)
                        .flat_map(|y| raw[(y * w + x0) * 4..(y * w + x1) * 4].to_vec())
                        .collect::<Vec<u8>>()
                };
                let before = grab(cx);
                std::thread::sleep(Duration::from_millis(200));
                cx.advance_clock(Duration::from_millis(200));
                // The marks' clock on by 200 ms whether or not its ticker runs.
                ws.update(cx, |ws, cx| {
                    ws.phase = status_mark::advance(status_mark::advance(ws.phase));
                    cx.notify();
                });
                settle(cx, window);
                let after = grab(cx);
                assert_eq!(before == after, still, "{n}: the panel moved = {}", !still);
                slot.borrow_mut().take();
            };
            let shot = render_with(&theme, dims, build, act);
            if still {
                save_shot("palette-motion", slug, &shot);
            }
            println!("ui-shots: palette-motion-{slug}-{still} ok");
        }
    }
}

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
    for (slug, name) in SHOT_THEMES {
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
            welcome_found(cx, window, &ws, vec![Found::Yes, Found::No, Found::Yes]);
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
                .read(|cx| card.read(cx).row_bounds(Row::Preset(2)))
                .expect("codex's row is drawn");
            cx.simulate_click(window, codex.center(), gpui::Modifiers::none());
            settle(cx, window);
            assert!(
                welcome_of(cx, &ws).is_some(),
                "{m}: a click on a row keeps the card open"
            );
            let (chosen, texts) = welcome_state(cx, &ws);
            assert_eq!(chosen, Row::Preset(2), "{m}: the click chose codex");
            assert!(
                texts.iter().any(|t| t == "Start codex"),
                "{m}: the button follows: {texts:?}"
            );
            found.set(cx.read(|cx| {
                let c = card.read(cx);
                Some((
                    c.card_bounds()?,
                    c.row_bounds(Row::Preset(2))?,
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

/// The Welcome card while detection hasn't answered, in both themes: `welcome-checking`.
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
    for (slug, name) in SHOT_THEMES {
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
                assert_eq!(found, [Found::Checking; 3], "{n}: nothing answered yet");
                let (chosen, texts) = welcome_state(cx, &ws);
                assert_eq!(chosen, Row::Preset(0), "{n}: claude chosen meanwhile");
                let checking = texts.iter().filter(|t| *t == "Checking\u{2026}").count();
                assert_eq!(checking, 3, "{n}: each row checking: {texts:?}");

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
                welcome_found(cx, window, &ws, vec![Found::No; 3]);
                assert_eq!(welcome_state(cx, &ws).0, Row::Preset(1), "{n}: kept moved");
                open_welcome_on(cx, window, &ws, first);
                welcome_found(cx, window, &ws, vec![Found::No; 3]);
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
                std::thread::sleep(Duration::from_millis(200));
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

/// The Welcome card's keys and what closes it, in both themes: `welcome-keys`.
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
    for (slug, name) in SHOT_THEMES {
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
            press_keys(cx, window, "down down down down");
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
    for (slug, name) in SHOT_THEMES {
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

/// What a pick on the Welcome card opens and closes, in both themes (not shot: the picks start
/// real shells): `welcome-start`, over `s1`, a terminal in the made-up home.
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
    for (slug, name) in SHOT_THEMES {
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
                press_keys(cx, window, "down down");
                assert_eq!(welcome_state(cx, ws).0, Row::Preset(2), "{n}: codex");
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
                press_keys(cx, window, "down down enter");
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
                press_keys(cx, window, "down down down");
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
                assert!(
                    active_terminal_focused(cx, window, ws),
                    "{n}: the new terminal types"
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

/// Choose… and Return on the folder row, in both themes: `welcome-folder`, the folder panel
/// answered by the scene (the off-screen platform has none).
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
    for (slug, name) in SHOT_THEMES {
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

            press_keys(cx, window, "down down down");
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

/// Help → Welcome to fuxx over a terminal in a project (M11k Task 4), in both themes:
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
    for (slug, name) in SHOT_THEMES {
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
    for (slug, name) in SHOT_THEMES {
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

/// One box over the window at a time: opening the rename box, the palette, the address box or
/// the Help panel over another closes that one, and the new one keeps the keyboard (the old one's
/// late "lost focus" must not take it back). Rename -> palette -> rename -> address box ->
/// palette -> help -> rename -> help -> palette, each checked after frames have run the focus
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
                if let Some(p) = &ws.palette {
                    open.push(("palette", p.focus_handle(cx).is_focused(window)));
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
            ("palette", "palette"),
            ("rename", "rename"),
            ("address", "address"),
            ("palette", "palette"),
            ("help", "help"),
            ("rename", "rename"),
            ("help", "help"),
            ("palette", "palette"),
        ] {
            cx.update_window(window, |_, window, cx| {
                ws.update(cx, |ws, cx| match step {
                    "rename" => ws.start_rename(ws.model.display_order()[3], window, cx),
                    "address" => ws.open_address_box(window, cx),
                    "help" => {
                        ws.open_help(crate::help_panel::HelpPage::Shortcuts, true, window, cx)
                    }
                    _ => ws.open_palette(crate::palette_items::Scope::All, window, cx),
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
                        Overlay::Palette => {
                            ws.open_palette(crate::palette_items::Scope::All, window, cx)
                        }
                        Overlay::TabMenu => ws.open_tab_menu(first, at, window, cx),
                        Overlay::PaneTabMenu => {
                            let tab = ws.model.focused_tab().expect("a focused tab");
                            ws.open_pane_tab_menu(tab, at, window, cx)
                        }
                        Overlay::PaneActions => {
                            let pane = ws.model.focused_pane().expect("a focused pane");
                            ws.open_pane_actions(pane, at, window, cx)
                        }
                        Overlay::Notifications => ws.open_notifications(window, cx),
                        Overlay::Help => {
                            ws.open_help(crate::help_panel::HelpPage::Shortcuts, true, window, cx)
                        }
                        Overlay::Welcome => {
                            ws.open_welcome(crate::welcome_items::Origin::Help, window, cx)
                        }
                    }
                })
            })
            .expect("open it");
            cx.update_window(window, |_, window, cx| window.draw(cx).clear())
                .expect("draw");
            cx.run_until_parked();
            let other = if overlay == Overlay::Rename {
                Overlay::Palette
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
        // it: the palette isn't left under its click-away backdrop.
        cx.update(|cx| {
            ws.update(cx, |ws, _| ws.sidebar.new_menu = true);
            assert!(
                ws.read(cx).overlays(cx).any(),
                "overlays: the + menu hides the pages"
            );
        });
        cx.update_window(window, |_, window, cx| {
            ws.update(cx, |ws, cx| {
                ws.open_palette(crate::palette_items::Scope::All, window, cx)
            })
        })
        .expect("open the palette");
        cx.update(|cx| {
            let ws = ws.read(cx);
            assert!(
                !ws.sidebar.new_menu && ws.palette.is_some(),
                "overlays: ⌘K closes the + menu"
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

/// The Help panel (M11d) over the ⌘P fixture's workspace, in both themes:
/// - `help-shortcuts`: Keyboard Shortcuts just opened, every listed binding in its group;
/// - `help-split`: "split" typed, which leaves Split Right and Split Down;
/// - `help-none`: "zzz" typed, which finds nothing ("No shortcut matches");
/// - `help-commands`: the Command Reference with "browser" typed;
/// - `help-palette`: "palette" typed, which finds Command Palette… (⌘K);
/// - `help-over-palette`: Keyboard Shortcuts opened while the command palette is open;
/// - `help-toggle`: Keyboard Shortcuts opened and opened again (as ⌘/ twice does);
/// - `help-tips`: Help → Show Tips Card with the sidebar hidden and the tips card closed.
///
/// Panics unless the rows shown are the ones the pure modules list for that text (Command
/// Palette… with ⌘K and Go to Workspace… with ⌘P among them), the panel opened over the palette
/// closed it and has the keyboard, and the second open closed the panel
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
    let palette = shortcuts
        .iter()
        .filter(|r| row_matches(r, "palette"))
        .count();
    assert_eq!(palette, 1, "help: \"palette\" finds Command Palette…");
    for (name, keys) in [
        ("Command Palette\u{2026}", "\u{2318}K"),
        ("Go to Workspace\u{2026}", "\u{2318}P"),
    ] {
        assert!(
            shortcuts.iter().any(|r| r.name == name && r.keys == keys),
            "help: {name} {keys} is listed"
        );
    }
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
            "help-palette",
            "help-commands",
            "help-over-palette",
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
                    "help-shortcuts" | "help-split" | "help-none" | "help-palette" => {
                        open(cx, HelpPage::Shortcuts);
                        let (typed, rows) = match scene {
                            "help-split" => ("split", split),
                            "help-none" => ("zzz", 0),
                            "help-palette" => ("palette", palette),
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
                    "help-over-palette" => {
                        cx.update_window(window, |_, window, cx| {
                            ws.update(cx, |ws, cx| {
                                ws.open_palette(crate::palette_items::Scope::All, window, cx)
                            })
                        })
                        .expect("open the palette");
                        cx.run_until_parked();
                        open(cx, HelpPage::Shortcuts);
                        assert!(
                            cx.read(|cx| ws.read(cx).palette.is_none()),
                            "{n}: the help closed the palette"
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

/// The host picker and the address box over the same workspace as the palette shots, in both
/// themes (`host-picker-*`, `address-box-*`), to compare with `palette-*`: one panel look.
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
        if section == Section::Sessions {
            command_blocks_row(&shot, &theme, &scene, build);
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

/// Preferences → Sessions' Command blocks row (M11l) in `shot`: its label, its switch and its
/// note line are drawn where the layout puts them. Then, in a second window over a temporary
/// settings file, clicking its Off writes `command-blocks = off` (and nothing else).
fn command_blocks_row(
    shot: &Shot,
    theme: &Theme,
    scene: &str,
    build: impl Fn(&mut Window, &mut App) -> AnyView + 'static,
) {
    use crate::preferences;

    let dims = preferences::window_size();
    let ui = theme.ui();
    let inked = |(x, y, w, h): (f32, f32, f32, f32)| {
        (0..h as usize).any(|dy| {
            let along = count_along(shot, dims, ui.sidebar_background, y + dy as f32, x, x + w);
            let scale = shot.width as f32 / f32::from(dims.width);
            along < ((x + w) * scale) as usize - (x * scale) as usize
        })
    };
    let [label, switch, note] = preferences::command_blocks_areas();
    assert!(inked(label), "{scene}: no Command blocks label");
    assert!(inked(switch), "{scene}: no Command blocks switch");
    assert!(inked(note), "{scene}: no Command blocks note line");
    // On is chosen (the default): its option is filled. Off starts just past that fill.
    let (sx, sy, sw, sh) = switch;
    let mid = sy + sh / 2.0;
    let on_right = (0..(sw * 2.0) as usize)
        .rev()
        .map(|i| sx + i as f32 / 2.0)
        .find(|&x| {
            let p = pixel_at(shot, dims, x, mid);
            p.iter()
                .zip([ui.row_active.r, ui.row_active.g, ui.row_active.b])
                .all(|(a, b)| a.abs_diff(b) <= 3)
        })
        .unwrap_or_else(|| panic!("{scene}: On isn't chosen"));
    let dir = std::env::temp_dir().join(format!("fuxx-shots-blocks-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    // Laid out as a config home (`<dir>/fuxx/config`), so the reload after the write reads only
    // from here (`appearance::ShotsHome`), never your own settings.
    let config = crate::settings::settings_path(&dir, Some(&dir));
    std::fs::create_dir_all(config.parent().expect("a folder")).expect("temp dir");
    std::fs::write(&config, "theme = Nord\n").expect("write the settings");
    let settings = config.clone();
    let home = dir.clone();
    let with_file = move |window: &mut Window, cx: &mut App| -> AnyView {
        let view = build(window, cx);
        cx.global_mut::<Appearance>().settings_path = Some(settings.clone());
        cx.set_global(crate::appearance::ShotsHome(home.clone()));
        view
    };
    let kept = config.clone();
    let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
        let off = point(px(on_right + 8.0), px(mid));
        cx.simulate_click(window, off, gpui::Modifiers::none());
        // Reloaded from the temporary file: a second change would go there too.
        let now = cx.read(|cx| cx.global::<Appearance>().settings_path.clone());
        assert_eq!(now, Some(kept), "the reload left the temporary settings");
        let blocks = cx.read(|cx| cx.global::<Appearance>().command_blocks);
        assert!(!blocks, "the reload didn't read the change");
    };
    render_with(theme, dims, with_file, act);
    let written = std::fs::read_to_string(&config).expect("read the settings");
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(
        written, "theme = Nord\ncommand-blocks = off\n",
        "{scene}: Off didn't write the setting"
    );
}

/// The sidebar button in the top band, as small windows of the top left corner, in both themes:
/// `sidebar-button-shown`, `-hidden`, `-hover` (the pointer over it) and `-sliding` (a hide
/// 60 ms in, the sidebar partly out and over the pane column).
///
/// Panics unless the icon is drawn at the sidebar band's right edge shown (M11m) and at the
/// window's right hidden (the collapsed band's last button), the hover paints the hit area, and
/// mid-slide the sidebar has left the middle of the window (it is only partly out).
fn sidebar_button_scenes() {
    use crate::sidebar_toggle::{BUTTON_Y, Slide, open_button_x};
    use crate::title_area::band_buttons;
    use crate::ui_tokens::size::SIDEBAR_DEFAULT_WIDTH;
    use gpui::Modifiers;
    use std::time::{Duration, Instant};

    // Tall enough that the pane's terminal still shows above the status bar (M11i).
    let dims = size(px(360.0), px(140.0));
    // Shown, the button is at the sidebar band's right edge (M11m); hidden, it is the collapsed
    // band's last button, at the window's right.
    let (bx, by) = (open_button_x(SIDEBAR_DEFAULT_WIDTH), BUTTON_Y);
    let hx = band_buttons(f32::from(dims.width))[2];
    // The hit area's left margin (inside it, outside the icon), and the icon's left edge.
    let (margin, edge) = (point(bx + 1.5, by + 12.0), point(bx + 4.75, by + 12.0));
    let hidden_edge = point(hx + 4.75, by + 12.0);
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
        // The icon's left edge is not the band's background: shown, at the sidebar band's
        // right edge; hidden, at the window's right (`band_buttons`).
        assert_ne!(
            at("shown", edge),
            bg(ui.sidebar_background),
            "{slug}: icon shown"
        );
        // (With the sidebar hidden the band is the canvas, the sidebar's colour, since M11i.)
        assert_ne!(
            at("hidden", hidden_edge),
            bg(ui.sidebar_background),
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
    // Where the title's ink centre sits against its box's centre: measured 1 pt left (GPUI
    // snaps the text's origin, and the glyphs' side bearings are uneven). Rebaseline it if the
    // system font or the GPUI rev changes.
    const TITLE_INK_SHIFT: f32 = -1.0;
    // The narrowest sidebar that shows the title, up to the next half point.
    let threshold = (2.0 * (lights_clear(false) + TITLE_ROOM / 2.0) * 2.0).ceil() / 2.0;
    let dims = size(px(760.0), px(200.0));
    let height = f32::from(dims.height);
    for (slug, name) in SHOT_THEMES {
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
            let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/ui-shots");
            std::fs::create_dir_all(&dir).expect("create target/ui-shots");
            let path = dir.join(format!("{scene}-{slug}.png"));
            std::fs::write(&path, png(&shot)).expect("write the PNG");
            println!("ui-shots: {}", path.display());
            let n = format!("{scene}-{slug}");
            let button_x = open_button_x(sidebar_width);
            let title = ink_box(&shot, dims, (0.0, 0.0, button_x, TOP_BAND), band, dim);
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
                    // (±1.5) than baseline ± 0.5 reaches (−1.5 to −0.5), so it never fails first.
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
    for (slug, name) in SHOT_THEMES {
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
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/ui-shots");
        std::fs::create_dir_all(&dir).expect("create target/ui-shots");
        let path = dir.join(format!("{scene}-{slug}.png"));
        std::fs::write(&path, png(&shot)).expect("write the PNG");
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
    for (slug, name) in SHOT_THEMES {
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
            let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/ui-shots");
            std::fs::create_dir_all(&dir).expect("create target/ui-shots");
            let path = dir.join(format!("{scene}-{slug}.png"));
            std::fs::write(&path, png(&shot)).expect("write the PNG");
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
            match scene {
                "title-bar-collapsed" => {
                    icons_at_slots();
                    assert_ne!(at(dot.0, dot.1), bg(ui.accent), "{n}: no dot");
                    centred(name_ink);
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
                    assert_ne!(at(30.0, 80.0), bg(theme.background), "{n}: still partly in");
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
    for (slug, name) in SHOT_THEMES {
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
            let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/ui-shots");
            std::fs::create_dir_all(&dir).expect("create target/ui-shots");
            let path = dir.join(format!("{scene}-{slug}.png"));
            std::fs::write(&path, png(&shot)).expect("write the PNG");
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

/// The folder the status bar's terminals work in, a deep one, and a 40-character branch.
const FUXX: &str = "/Users/me/Kerja/proj/fuxx-cli";
const DEEP: &str =
    "/Users/me/Kerja/clients/acme-corporation/platform/services/payments/api-gateway";
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

/// Whether pixel `p` is colour `c`, give or take `tol` a channel.
fn near_by(p: [u8; 3], c: alacritty_terminal::vte::ansi::Rgb, tol: u8) -> bool {
    p.iter()
        .zip([c.r, c.g, c.b])
        .all(|(a, b)| a.abs_diff(b) <= tol)
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
    let scale = shot.width as f32 / f32::from(dims.width);
    let mut runs = Vec::new();
    let mut start: Option<f32> = None;
    for dx in 0..shot.width as usize {
        let x = dx as f32 / scale;
        let on = near_by(pixel_at(shot, dims, x, y), c, 3);
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

/// How far in from a chip's end its fill starts 1.5 pt below its top (its corner's curve).
fn chip_corner_inset() -> f32 {
    use crate::status_bar::CHIP_RADIUS;
    let r = CHIP_RADIUS;
    r - (r * r - (r - 1.5) * (r - 1.5)).sqrt()
}

/// The status bar over a real workspace, in both themes:
/// - `status-bar`: claude working: its mark, `Working · 4m` in `text_dim`, the branch glyph and
///   `main`, `~/Kerja/proj/fuxx-cli` in `text_faint`;
/// - `status-bar-waiting`: agent waiting: `Needs input · 2m` in `ring_waiting`;
/// - `status-bar-file`: a file tab focused in claude's pane: the bar keeps claude's facts;
/// - `status-bar-still`: Reduce motion on: claude's mark is the still one, its open quarter at
///   the top left whatever the clock (moving, at a tenth of the beat, it has turned away).
///
/// Panics unless the bar is 22 pt of canvas along the window's whole bottom, under the cards
/// (6 pt between), says what it should (`status_bar_facts`) and draws it: the left side's ink
/// as wide as its pieces measured, in their colours.
fn status_bar_scenes() {
    use crate::status_bar::{
        BRANCH_GAP, BRANCH_GLYPH, ITEM_GAP, MARK, MARK_GAP, PAD, StatusWord, text_width,
    };
    use crate::ui_tokens::size::{CARD_GAP, SIDEBAR_DEFAULT_WIDTH, STATUS_BAR};
    use gpui::FontWeight;
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    let dims = size(px(1000.0), px(480.0));
    let (w, h) = (f32::from(dims.width), f32::from(dims.height));
    let (top, mid) = (h - STATUS_BAR, h - STATUS_BAR / 2.0);
    let file = PathBuf::from(format!("{FUXX}/src/main.rs"));
    for (slug, name) in SHOT_THEMES {
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
                    let mut ws = bar_workspace(
                        active,
                        SIDEBAR_DEFAULT_WIDTH,
                        (FUXX, "main"),
                        &[],
                        window,
                        cx,
                    );
                    // A tenth into the beat: a moving Working mark has turned 144°.
                    ws.phase = 0.1;
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
                        // The marks' ticker counts the bar's mark: the two rows, claude's tab
                        // and the bar.
                        let marks = cx.read(|cx| ws.read(cx).visible_statuses().len());
                        assert_eq!(marks, 4, "{n}: the bar's mark is on screen");
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
                            "Working \u{b7} 4m",
                            Some("main"),
                            "~/Kerja/proj/fuxx-cli",
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
            // 22 pt of canvas along the whole bottom (between its pieces), the cards' bottom
            // 6 pt above it.
            for x in [2.0, w / 2.0 + 37.0, w - 2.0] {
                assert!(canvas(at(x, top + 1.0)), "{n}: the bar's top at {x}");
                assert!(canvas(at(x, h - 1.0)), "{n}: the bar's bottom at {x}");
            }
            // The card's inside (the terminal's background) stops at its 1 pt edge, 6 pt above
            // the bar (its shadow shades the gap, so the gap isn't looked at).
            let card_x = SIDEBAR_DEFAULT_WIDTH + 40.0;
            let inside = (0..(top * scale) as usize)
                .map(|dy| dy as f32 / scale)
                .filter(|&y| near_by(at(card_x, y), ui.card, 2))
                .fold(0.0, f32::max)
                + 1.0 / scale;
            assert!(
                (top - inside - (CARD_GAP + 1.0)).abs() <= 0.75,
                "{n}: the card's inside ends at {inside}, {} above the bar",
                top - inside
            );
            // The left side's ink, from the mark to the folder's end, as wide as measured.
            let (word_w, branch_w, folder_w) = widths.get();
            let ys: Vec<f32> = (0..((STATUS_BAR - 4.0) * scale) as usize)
                .map(|dy| top + 2.0 + dy as f32 / scale)
                .collect();
            let inked = |x: f32| ys.iter().any(|&y| !canvas(at(x, y)));
            let xs: Vec<f32> = (0..(w / 2.0 * scale) as usize)
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
    // Inside the chips, above their words.
    let line = mid - CHIP / 2.0 + 1.5;
    let inset = chip_corner_inset();
    let ports = [3000, 5173, 8080, 9000, 9229];
    for (slug, name) in SHOT_THEMES {
        let theme = shot_theme(name);
        let ui = theme.ui();
        // Where the chips are, from a first shot with the pointer away.
        let build = move |window: &mut Window, cx: &mut App| -> AnyView {
            cx.set_global(sidebar_appearance(theme, cx));
            cx.new(|cx| bar_workspace(0, SIDEBAR_DEFAULT_WIDTH, (FUXX, "main"), &ports, window, cx))
                .into()
        };
        let plain = render(&theme, dims, build);
        let chips = runs_of(&plain, dims, line, ui.row_hover);
        println!("ui-shots: status-bar-ports-{slug}: chips {chips:?}");
        assert_eq!(chips.len(), 4, "{slug}: three chips and +2: {chips:?}");
        let last = chips[3].1;
        assert!(
            (last + inset - (w - PAD)).abs() <= 1.0,
            "{slug}: the chips end at the bar's right end: {last}"
        );
        for pair in chips.windows(2) {
            let gap = pair[1].0 - pair[0].1 - 2.0 * inset;
            assert!((gap - CHIP_GAP).abs() <= 1.0, "{slug}: {gap} between chips");
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
    for (slug, name) in SHOT_THEMES {
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
        let line = mid - crate::status_bar::CHIP / 2.0 + 1.5;
        let chips = runs_of(&shot, dims, line, ui.row_hover);
        assert_eq!(chips.len(), 1, "{n}: one `6 ports` chip: {chips:?}");
        // Nothing overlaps: canvas from the left side's last ink to the right side's start.
        let right = chips[0].0 - chip_corner_inset();
        let canvas = |p: [u8; 3]| near_by(p, ui.sidebar_background, 2);
        let ys: Vec<f32> = (0..((STATUS_BAR - 4.0) * scale) as usize)
            .map(|dy| top + 2.0 + dy as f32 / scale)
            .collect();
        let clear = |x: f32| ys.iter().all(|&y| canvas(pixel_at(&shot, dims, x, y)));
        let mut x = right - 1.0 / scale;
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

/// v0.3.2, the newer release in the update scenes.
fn shot_release() -> crate::update_release::Release {
    crate::update_release::Release {
        version: crate::update_release::Version(0, 3, 2),
        page_url: "https://github.com/fitzfitz/fuxx-cli/releases/tag/v0.3.2".into(),
        notes: "Faster startup".into(),
        zip: None,
    }
}

/// The update pill at the status bar's right end (M11i; it was above the sidebar's footer), over
/// `switcher_workspace`, in both themes, with v0.3.2 the newer release:
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
            assert!(
                (icon - PILL_ICON).abs() <= 1.0,
                "{n}: the icon's ink is {icon} pt wide, its box {PILL_ICON}"
            );
            // The words' own side bearing adds a little to the gap.
            assert!(
                (PILL_GAP - 0.5..=PILL_GAP + 1.5).contains(&gap),
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
        let dims = size(px(320.0), px(120.0));
        Self::sized(theme, text, dims, false, (focus, key, still), tweak)
    }

    /// `new` at `dims`, the pane laid out as one of a split when `split`; `(focus, key, still)`
    /// as `new` takes them.
    fn sized(
        theme: Theme,
        text: &str,
        dims: Size<Pixels>,
        split: bool,
        flags: (bool, bool, bool),
        tweak: impl FnOnce(&mut Appearance),
    ) -> Self {
        Self::spawned(
            theme,
            vec!["/bin/cat".into()],
            text,
            dims,
            split,
            flags,
            tweak,
        )
    }

    /// `sized`, running `program` instead of `cat`.
    fn spawned(
        theme: Theme,
        program: Vec<String>,
        text: &str,
        dims: Size<Pixels>,
        split: bool,
        (focus, key, still): (bool, bool, bool),
        tweak: impl FnOnce(&mut Appearance),
    ) -> Self {
        use crate::terminal_view::TerminalView;
        use gpui::Focusable;
        use std::cell::RefCell;
        use std::rc::Rc;

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
                let view = TerminalView::spawn(cx, Some(program), None, None, &quiet)
                    .expect("start the program");
                view.read(cx).show_text(&text);
                view.read(cx).set_split(split);
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

        // The breath at rest (M11n): five minutes untouched, solid at full strength with its
        // glow and no frames; a keystroke, a program's write or a refocus brings it back.
        // Glide off, so cat's echo moving the cursor leaves the breath's frames, not a glide's.
        {
            let mut rig = CursorRig::new(theme, text, true, true, false, |a| {
                a.cursor_glide = false;
            });
            rig.frame();
            let n = format!("cursor-breath-resumes-{slug}");
            let cell = rig.cell(2, 1);
            let left = f32::from(cell.left());
            let mid = f32::from(cell.top() + cell.bottom()) / 2.0;
            assert_eq!(rig.read(|v| v.cursor_frames()), Frames::Breath, "{n}");
            let rest = |rig: &mut CursorRig| {
                rig.read(|v| v.age_breath(std::time::Duration::from_millis(301_500)));
                let view = rig.view.clone();
                rig.cx.update(|cx| view.update(cx, |_, cx| cx.notify()));
                rig.frame()
            };
            let shot = rest(&mut rig);
            save_shot("cursor-breath-resumes", slug, &shot);
            assert_eq!(
                rig.read(|v| v.cursor_frames()),
                Frames::None,
                "{n}: at rest after five minutes, no frames"
            );
            assert!(
                is_colour(&shot, rig.dims, left + 0.5, mid, own),
                "{n}: at rest the bar is the full colour, saw {:?}",
                pixel_at(&shot, rig.dims, left + 0.5, mid)
            );
            assert!(
                !is_colour(&shot, rig.dims, left - 3.0, mid, bg),
                "{n}: at rest the glow stays"
            );
            // One paint now, before anything else runs, and the frames it asked for (read
            // before the capture, which paints again): what the very next frame after a change
            // asks for. (A cursor move ends the rest by itself; cat's echo reaches the `Term`
            // from the parser thread before its `Output` is heard.)
            let paint_now = |rig: &mut CursorRig| {
                rig.cx
                    .update_window(rig.window, |_, window, cx| window.draw(cx).clear())
                    .expect("draw");
                let frames = rig.read(|v| v.cursor_frames());
                let image = rig
                    .cx
                    .capture_screenshot(rig.window)
                    .expect("capture the window");
                let shot = Shot {
                    width: image.width(),
                    height: image.height(),
                    rgba: image.as_raw().to_vec(),
                };
                (frames, shot)
            };
            // A keystroke cat doesn't echo (an erase on its empty line): only the keystroke
            // itself can end the rest.
            let view = rig.view.clone();
            rig.cx
                .update(|cx| view.update(cx, |v, cx| v.type_bytes(b"\x7f", cx)));
            assert_eq!(
                paint_now(&mut rig).0,
                Frames::Breath,
                "{n}: a keystroke ends the rest"
            );
            // A typed letter.
            rest(&mut rig);
            assert_eq!(rig.read(|v| v.cursor_frames()), Frames::None, "{n}");
            rig.cx
                .update(|cx| view.update(cx, |v, cx| v.type_bytes(b"x", cx)));
            assert_eq!(
                paint_now(&mut rig).0,
                Frames::Breath,
                "{n}: typing ends the rest"
            );
            // Output that leaves the cursor where it was: cat echoes `a`, then its erase
            // (`\b \b`), so only the `Output` itself can end the rest.
            wait_for(&mut rig, |v| {
                v.text(crate::grid::TextScope::Screen).contains("$ x")
            });
            rig.write("\x1b[2;3H\x1b[K");
            rig.frame();
            rest(&mut rig);
            assert_eq!(rig.read(|v| v.cursor_frames()), Frames::None, "{n}");
            rig.read(|v| v.write_to_program(b"a\x7f"));
            wait_for(&mut rig, |v| v.cursor_frames() == Frames::Breath);
            assert_eq!(
                rig.read(|v| v.cursor_frames()),
                Frames::Breath,
                "{n}: output ends the rest"
            );
            // A program's write that moves the cursor.
            rest(&mut rig);
            assert_eq!(rig.read(|v| v.cursor_frames()), Frames::None, "{n}");
            rig.write("y");
            rig.frame();
            assert_eq!(
                rig.read(|v| v.cursor_frames()),
                Frames::Breath,
                "{n}: a cursor move ends the rest"
            );
            // Blur, then focus back: breathing again, solid at first, from the first paint.
            rest(&mut rig);
            assert_eq!(rig.read(|v| v.cursor_frames()), Frames::None, "{n}");
            rig.cx
                .update_window(rig.window, |_, window, _| window.blur())
                .expect("blur");
            rig.frame();
            assert_eq!(
                rig.read(|v| v.cursor_frames()),
                Frames::None,
                "{n}: blurred"
            );
            let window = rig.window;
            rig.cx
                .update_window(window, |_, window, cx| {
                    use gpui::Focusable;
                    view.focus_handle(cx).focus(window, cx)
                })
                .expect("focus");
            // The focus change paints as the update ends: that one paint must already ask for
            // the breath (nothing else would paint again in the app).
            assert_eq!(
                rig.read(|v| v.cursor_frames()),
                Frames::Breath,
                "{n}: a refocus ends the rest, from its first paint"
            );
            let (frames, shot) = paint_now(&mut rig);
            assert_eq!(frames, Frames::Breath, "{n}: a refocus ends the rest");
            // The bar sits after `$ y` (cat's echo of the `x` was erased by the line above).
            let at = rig.cell(3, 1);
            let x = f32::from(at.left()) + 0.5;
            assert!(
                is_colour(&shot, rig.dims, x, mid, own),
                "{n}: back in focus the bar is solid at first, saw {:?}",
                pixel_at(&shot, rig.dims, x, mid)
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

/// The shot terminal of the `command-blocks*` scenes: four prompts marked as fuxx's zsh
/// integration marks them, then a live one. Rows: 0 `$ echo hi` (exit 0), 1 its output,
/// 2 an older empty prompt, 3-4 a two-line prompt and `false` (exit 1, no output),
/// 5 `sleep 9` (running, never ended), 6-7 its output, 8 the live prompt.
fn blocks_text() -> String {
    blocks_text_with(0)
}

/// `blocks_text` with `filler` more lines of output after block 1's `hi`.
fn blocks_text_with(filler: usize) -> String {
    const A: &str = "\x1b]133;A\x07";
    const B: &str = "\x1b]133;B\x07";
    let c = |cmd: &str| format!("\x1b]133;C;cmdline_url={cmd}\x07");
    let d = |x: i32| format!("\x1b]133;D;{x}\x07");
    [
        format!(
            "{A}$ {B}echo hi\r\n{}hi\r\n{}{}",
            c("echo%20hi"),
            (1..=filler)
                .map(|n| format!("line {n}\r\n"))
                .collect::<String>(),
            d(0)
        ),
        format!("{A}$ {B}\r\n{}", d(0)),
        format!("{A}~/code\r\n$ {B}false\r\n{}{}", c("false"), d(1)),
        format!("{A}$ {B}sleep 9\r\n{}one\r\ntwo\r\n", c("sleep%209")),
        format!("{A}$ {B}"),
    ]
    .concat()
}

/// A `command-blocks*` rig: the blocks text in a `cat` terminal, unfocused (its cursor a still
/// outline), at `dims`, a split's pane when `split`.
fn blocks_rig(
    theme: Theme,
    dims: Size<Pixels>,
    split: bool,
    still: bool,
    tweak: impl FnOnce(&mut Appearance),
) -> CursorRig {
    CursorRig::sized(
        theme,
        &blocks_text(),
        dims,
        split,
        (false, false, still),
        tweak,
    )
}

/// What a `command-blocks*` check reads off a rig: where row `r`'s middle and top are, and
/// where the bars' centre is, in points.
struct BlockGeometry {
    bar_x: f32,
    origin_x: f32,
    sep_x: f32,
    rows: Vec<(f32, f32)>,
}

impl BlockGeometry {
    fn of(rig: &CursorRig, split: bool) -> Self {
        let first = rig.cell(0, 0);
        let origin_x = f32::from(first.left());
        let rows = (0..10)
            .map(|r| {
                let c = rig.cell(0, r);
                (f32::from(c.top()), f32::from(c.center().y))
            })
            .collect();
        Self {
            bar_x: origin_x - crate::block_view::gutter_x(split),
            origin_x,
            // Right of every row's text, inside the grid.
            sep_x: f32::from(rig.cell(20, 0).left()),
            rows,
        }
    }
    fn top(&self, r: usize) -> f32 {
        self.rows[r].0
    }
    fn mid(&self, r: usize) -> f32 {
        self.rows[r].1
    }
}

/// How wide, in points, the pixels along `y` from `x0` to `x1` that aren't `bg` are (a
/// partly covered edge pixel counts by how far it is from `bg`).
fn ink_width(
    shot: &Shot,
    dims: Size<Pixels>,
    bg: alacritty_terminal::vte::ansi::Rgb,
    y: f32,
    x0: f32,
    x1: f32,
) -> f32 {
    let scale = shot.width as f32 / f32::from(dims.width);
    let (a, b) = ((x0 * scale) as usize, (x1 * scale) as usize);
    let mut full = 0.0;
    let mut far = 0.0f32;
    for dx in a..b {
        let p = pixel_at(shot, dims, dx as f32 / scale, y);
        let d = p
            .iter()
            .zip([bg.r, bg.g, bg.b])
            .map(|(a, b)| a.abs_diff(b) as f32)
            .fold(0.0, f32::max);
        far = far.max(d);
        full += d;
    }
    // Each pixel's share: its distance from `bg` over the bar's own (the farthest seen).
    if far == 0.0 { 0.0 } else { full / far / scale }
}

/// The first device pixel (in points) that differs between shots `a` and `b` in the box from
/// (`x0`, `y0`) to (`x1`, `y1`), if any.
fn first_change(
    a: &Shot,
    b: &Shot,
    dims: Size<Pixels>,
    (x0, y0): (f32, f32),
    (x1, y1): (f32, f32),
) -> Option<(f32, f32, [u8; 3], [u8; 3])> {
    let scale = a.width as f32 / f32::from(dims.width);
    let (px0, px1) = ((x0 * scale) as usize, (x1 * scale).ceil() as usize);
    let (py0, py1) = ((y0 * scale) as usize, (y1 * scale).ceil() as usize);
    (py0..py1.max(py0 + 1)).find_map(|py| {
        (px0..px1).find_map(|px_| {
            let (x, y) = (px_ as f32 / scale, py as f32 / scale);
            let (p, q) = (pixel_at(a, dims, x, y), pixel_at(b, dims, x, y));
            (p != q).then_some((x, y, p, q))
        })
    })
}

/// The view's chip, read for the checks: its label, buttons and painted bounds.
/// A flood reaches the screen (M11n): a real pane runs `seq 1 300000; echo END`, with
/// Animations off so nothing but its output asks for a repaint, and the window is drawn only
/// when the view asks (`cx.notify`), as the app's frame loop draws it. Once the output stops,
/// with no input at all, the last frame drawn holds `END`. This fails if the view stops
/// acknowledging `Output` (`output_seen`): the parser's gate then sends no further `Output`
/// and the screen stays on an early frame until a keystroke.
fn cursor_flood_scene() {
    use crate::grid::TextScope;
    use std::cell::Cell;
    use std::rc::Rc;
    use std::time::{Duration, Instant};

    let (slug, name) = SHOT_THEMES[0];
    let n = format!("cursor-flood-{slug}");
    // The flood waits for `go` to exist, so it starts only once the scene is watching. The
    // child stays alive after `END`, so no exit repaints the screen either.
    let go = std::env::temp_dir().join(format!("fuxx-ui-shots-{}-flood-go", std::process::id()));
    let _ = std::fs::remove_file(&go);
    let program = vec![
        "/bin/sh".into(),
        "-c".into(),
        format!(
            "while [ ! -e '{}' ]; do sleep 0.01; done; seq 1 300000; echo END; exec sleep 60",
            go.display()
        ),
    ];
    let mut rig = CursorRig::spawned(
        shot_theme(name),
        program,
        "",
        size(px(320.0), px(120.0)),
        false,
        (true, true, false),
        |a| a.animations = false,
    );
    let asked = Rc::new(Cell::new(0usize));
    let _watch = {
        let asked = asked.clone();
        let view = rig.view.clone();
        rig.cx
            .update(|cx| cx.observe(&view, move |_, _| asked.set(asked.get() + 1)))
    };
    std::fs::write(&go, b"").expect("start the flood");
    let (mut drawn, mut frames) = (0, 0);
    let mut screen = String::new();
    let start = Instant::now();
    let mut ended: Option<Instant> = None;
    // Until the `Term` holds `END` and a further half second has passed with no input.
    while ended.is_none_or(|at| at.elapsed() < Duration::from_millis(500)) {
        assert!(
            start.elapsed() < Duration::from_secs(60),
            "{n}: the flood never finished"
        );
        std::thread::sleep(Duration::from_millis(5));
        rig.cx.run_until_parked();
        if asked.get() != drawn {
            drawn = asked.get();
            frames += 1;
            rig.cx
                .update_window(rig.window, |_, window, cx| window.draw(cx).clear())
                .expect("draw");
            screen = rig.read(|v| v.text(TextScope::Screen));
        }
        if ended.is_none() && rig.read(|v| v.text(TextScope::Screen).contains("END")) {
            ended = Some(Instant::now());
        }
    }
    let _ = std::fs::remove_file(&go);
    assert!(
        screen.lines().any(|l| l.trim_end() == "END"),
        "{n}: once the output stops, the last frame drawn (of {frames}) holds END, saw {screen:?}"
    );
}

fn chip_of(rig: &CursorRig) -> Option<(String, [&'static str; 3], gpui::Bounds<Pixels>)> {
    rig.read(|v| v.chip_shot())
}

/// Draw frames until `done` holds (at most 3 s: the program's echo comes from its own thread).
fn wait_for(rig: &mut CursorRig, done: impl Fn(&crate::terminal_view::TerminalView) -> bool) {
    let start = std::time::Instant::now();
    while !rig.read(&done) && start.elapsed() < std::time::Duration::from_secs(3) {
        std::thread::sleep(std::time::Duration::from_millis(10));
        let _ = rig.frame();
    }
    let _ = rig.frame();
}

/// Move the pointer to `at` and let the rig settle.
fn point_at(rig: &mut CursorRig, at: gpui::Point<Pixels>) {
    let window = rig.window;
    rig.cx
        .simulate_mouse_move(window, at, None, gpui::Modifiers::none());
    rig.cx.run_until_parked();
}

/// Command blocks (M11l Task 4), in both themes, from a `cat` terminal shown the marked text
/// of `blocks_text`:
/// - `command-blocks`: each block's bar, 8 pt left of the grid, in `block_ok`, `block_failed`
///   (both rows of the two-line prompt) and `block_running`; none beside output or the older
///   empty prompt. A 1 pt `card_border` hairline at the top of block 2's and 3's first prompt
///   rows and of the live prompt's, none on the first line of scrollback or above the empty
///   prompt. Hovering block 2's gutter widens its bar to 3 pt and opens the chip (`exit 1 ·
///   false`, the three buttons) 4 pt right of the gutter, one row tall on the command row (the
///   pixel rows just above and below it untouched); the chip stays while the pointer is on it
///   (200 ms on), and goes 150 ms after it leaves both.
///   Nothing moves: two frames 200 ms apart with the chip open are the same pixels, with
///   motion on and with Reduce motion on.
fn command_blocks_scene() {
    use crate::block_view::{CHIP_GAP, CHIP_HIDE_MS};
    use std::time::Duration;
    let dims = size(px(480.0), px(240.0));
    for (slug, name) in SHOT_THEMES {
        let theme = shot_theme(name);
        let ui = theme.ui();
        let bg = theme.background;
        let n = format!("command-blocks-{slug}");
        for still in [false, true] {
            let mut rig = blocks_rig(theme, dims, false, still, |_| ());
            let shot = rig.frame();
            let g = BlockGeometry::of(&rig, false);
            let at = |shot: &Shot, x: f32, y: f32, c| is_colour(shot, dims, x, y, c);
            let seen = |shot: &Shot, x: f32, y: f32| pixel_at(shot, dims, x, y);
            if !still {
                save_shot("command-blocks-plain", slug, &shot);
                for (row, colour, what) in [
                    (0, ui.block_ok, "echo hi (exit 0)"),
                    (3, ui.block_failed, "false's first prompt row"),
                    (4, ui.block_failed, "false (exit 1)"),
                    (5, ui.block_running, "sleep 9 (running)"),
                ] {
                    assert!(
                        at(&shot, g.bar_x, g.mid(row), colour),
                        "{n}: row {row}, {what}: {colour:?}, saw {:?}",
                        seen(&shot, g.bar_x, g.mid(row))
                    );
                }
                for (row, what) in [(1, "output"), (2, "the empty prompt"), (6, "output")] {
                    assert!(
                        at(&shot, g.bar_x, g.mid(row), bg),
                        "{n}: no bar beside {what} (row {row}): {:?}",
                        seen(&shot, g.bar_x, g.mid(row))
                    );
                }
                let w = ink_width(&shot, dims, bg, g.mid(4), g.bar_x - 4.0, g.bar_x + 4.0);
                assert!((1.5..=2.5).contains(&w), "{n}: a 2 pt bar, saw {w} pt");
                for row in [3, 5, 8] {
                    let y = g.top(row);
                    assert!(
                        at(&shot, g.sep_x, y + 0.5, ui.card_border),
                        "{n}: a hairline above row {row}: {:?}",
                        seen(&shot, g.sep_x, y + 0.5)
                    );
                    assert!(
                        at(&shot, g.sep_x, y + 1.5, bg),
                        "{n}: 1 pt only above row {row}: {:?}",
                        seen(&shot, g.sep_x, y + 1.5)
                    );
                }
                for (row, what) in [(0, "the first line of scrollback"), (2, "an empty prompt")] {
                    assert!(
                        at(&shot, g.sep_x, g.top(row) + 0.5, bg),
                        "{n}: no hairline above {what}"
                    );
                }
            }
            // Hover block 2's gutter.
            let ids = rig.read(|v| v.block_ids());
            assert_eq!(ids.len(), 3, "{n}: three blocks");
            point_at(&mut rig, point(px(g.bar_x), px(g.mid(4))));
            assert_eq!(
                rig.read(|v| v.hovered_block().map(str::to_string)),
                Some(ids[1].clone()),
                "{n}: block 2 hovered"
            );
            let shown = rig.frame();
            let (label, buttons, chip) = chip_of(&rig).expect("the chip is open");
            assert!(label.contains("exit 1 · false"), "{n}: chip says {label:?}");
            assert_eq!(buttons, ["Copy command", "Copy output", "Rerun"], "{n}");
            // (Layout snaps boxes to the pixel grid: within half a point.)
            let left = f32::from(chip.left());
            assert!(
                (left - (g.origin_x + CHIP_GAP)).abs() <= 0.5,
                "{n}: the chip's left edge {left} is 4 pt right of the gutter ({})",
                g.origin_x
            );
            assert!(
                (f32::from(chip.top()) - g.top(4)).abs() <= 0.5,
                "{n}: the chip opens at the command row's top"
            );
            let row = g.top(5) - g.top(4);
            let tall = f32::from(chip.size.height);
            assert!(
                (tall - row).abs() <= 0.5,
                "{n}: the chip is one row tall ({row} pt), saw {tall}"
            );
            if !still {
                save_shot("command-blocks", slug, &shown);
                // It covers its row only: the pixel rows just above and just below it (the
                // row under it starts with block 3's hairline) are as they were before it
                // opened, across the grid (the gutter's bar widens; it is left of this).
                let grid = rig.read(|v| v.grid_bounds()).expect("painted");
                let scale = shown.width as f32 / f32::from(dims.width);
                let (x0, x1) = (g.origin_x, f32::from(grid.right()));
                for (y, what) in [
                    (f32::from(chip.top()) - 1.0 / scale, "above"),
                    (f32::from(chip.bottom()), "below"),
                ] {
                    let change = first_change(&shot, &shown, dims, (x0, y), (x1, y));
                    assert!(
                        change.is_none(),
                        "{n}: the chip changed the pixel row {what} it: {change:?}"
                    );
                }
                // Hovered: 3 pt wide, still red.
                let w = ink_width(&shown, dims, bg, g.mid(4), g.bar_x - 4.0, g.bar_x + 4.0);
                assert!(
                    (2.5..=3.5).contains(&w),
                    "{n}: the hovered bar is 3 pt, saw {w}"
                );
                assert!(
                    at(&shown, g.bar_x, g.mid(4), ui.block_failed),
                    "{n}: still red"
                );
                let inside = (left + 3.0, f32::from(chip.center().y));
                assert!(
                    at(&shown, inside.0, inside.1, ui.panel),
                    "{n}: the chip is the panel colour: {:?}",
                    seen(&shown, inside.0, inside.1)
                );
            }
            if !still {
                use crate::grid::TextScope;
                // A program printing on the prompt's row (`cat` echoes what it is sent)
                // doesn't move the block: the chip stays open.
                rig.read(|v| v.write_to_program(b"z"));
                wait_for(&mut rig, |v| v.text(TextScope::Screen).contains("$ z"));
                assert!(
                    rig.read(|v| v.hovered_block().is_some()) && chip_of(&rig).is_some(),
                    "{n}: output that leaves the block in place keeps the chip"
                );
                // Scripted typing and pastes (the scripting API; Rerun's paste in Task 5)
                // keep the chip and the picked block.
                let view = rig.view.clone();
                let first = ids[0].clone();
                rig.cx.update(|cx| {
                    view.update(cx, |v, cx| {
                        v.set_chosen_block(Some(first), cx);
                        v.type_bytes(b"y", cx);
                        v.paste("w", cx);
                    })
                });
                wait_for(&mut rig, |v| v.text(TextScope::Screen).contains("$ zyw"));
                assert!(
                    chip_of(&rig).is_some(),
                    "{n}: a scripted paste keeps the chip"
                );
                assert_eq!(
                    rig.read(|v| v.chosen_block().map(str::to_string)),
                    Some(ids[0].clone()),
                    "{n}: and the picked block"
                );
                rig.cx
                    .update(|cx| view.update(cx, |v, cx| v.set_chosen_block(None, cx)));
            }
            let shown = rig.frame();
            // Nothing animates: 200 ms on (real time for what paints by the clock, the app's
            // clock for its timers), the same pixels.
            std::thread::sleep(Duration::from_millis(200));
            rig.cx.advance_clock(Duration::from_millis(200));
            let later = rig.frame();
            assert!(
                shown.rgba == later.rgba,
                "{n} (still {still}): the open chip's frames differ 200 ms apart"
            );
            // Onto the chip (the app's clock drives the hide timer): it stays.
            point_at(&mut rig, chip.center());
            rig.cx.advance_clock(Duration::from_millis(200));
            rig.cx.run_until_parked();
            assert!(
                chip_of(&rig).is_some(),
                "{n}: the chip stays while pointed at"
            );
            // Off both: still there at once and 140 ms on, gone at 150 ms.
            let away = rig.cell(30, 10).center();
            point_at(&mut rig, away);
            assert!(chip_of(&rig).is_some(), "{n}: the chip doesn't go at once");
            rig.cx
                .advance_clock(Duration::from_millis(CHIP_HIDE_MS - 10));
            rig.cx.run_until_parked();
            assert!(chip_of(&rig).is_some(), "{n}: the chip is there 140 ms on");
            rig.cx.advance_clock(Duration::from_millis(10));
            rig.cx.run_until_parked();
            assert!(rig.read(|v| v.hovered_block().is_none()), "{n}: unhovered");
            let gone = rig.frame();
            assert!(chip_of(&rig).is_none(), "{n}: the chip went 150 ms after");
            assert!(
                !at(&gone, left + 3.0, f32::from(chip.center().y), ui.panel),
                "{n}: no chip drawn"
            );
            let w = ink_width(&gone, dims, bg, g.mid(4), g.bar_x - 4.0, g.bar_x + 4.0);
            assert!(
                (1.5..=2.5).contains(&w),
                "{n}: the bar is back to 2 pt, saw {w}"
            );
            if !still {
                // The chosen block (picked in Task 5): a 3 pt accent bar down its whole
                // height, output rows too; typing drops it.
                let view = rig.view.clone();
                let third = ids[2].clone();
                rig.cx
                    .update(|cx| view.update(cx, |v, cx| v.set_chosen_block(Some(third), cx)));
                let chosen = rig.frame();
                save_shot("command-blocks-chosen", slug, &chosen);
                for row in [5, 6, 7] {
                    assert!(
                        at(&chosen, g.bar_x, g.mid(row), ui.accent),
                        "{n}: the chosen bar at row {row}: {:?}",
                        seen(&chosen, g.bar_x, g.mid(row))
                    );
                }
                let w = ink_width(&chosen, dims, bg, g.mid(6), g.bar_x - 4.0, g.bar_x + 4.0);
                assert!(
                    (2.5..=3.5).contains(&w),
                    "{n}: the chosen bar is 3 pt, saw {w}"
                );
                assert!(
                    at(&chosen, g.bar_x, g.mid(8), bg),
                    "{n}: not past the block"
                );
                // A press on a bar dragged into the grid selects nothing; the same drag
                // from the grid does.
                let (window, none) = (rig.window, gpui::Modifiers::none());
                let left = Some(gpui::MouseButton::Left);
                let to = rig.cell(6, 6).center();
                for (from, selects) in [
                    (point(px(g.bar_x), px(g.mid(4))), false),
                    (rig.cell(1, 4).center(), true),
                ] {
                    rig.cx
                        .simulate_mouse_down(window, from, gpui::MouseButton::Left, none);
                    rig.cx.simulate_mouse_move(window, to, left, none);
                    rig.cx
                        .simulate_mouse_up(window, to, gpui::MouseButton::Left, none);
                    rig.cx.run_until_parked();
                    let text = rig.read(|v| v.selection_text()).unwrap_or_default();
                    assert_eq!(!text.is_empty(), selects, "{n}: from {from:?}: {text:?}");
                }
                // An open chip closes on a press in the grid, on a typed key (which also
                // drops the picked block), and on output that scrolls its block.
                let open = |rig: &mut CursorRig, dy: f32| {
                    point_at(rig, point(px(g.bar_x), px(g.mid(4) + dy)));
                    assert!(chip_of(rig).is_some(), "{n}: the chip opens again");
                };
                open(&mut rig, 1.0);
                let grid_at = rig.cell(10, 2).center();
                rig.cx
                    .simulate_mouse_down(window, grid_at, gpui::MouseButton::Left, none);
                rig.cx
                    .simulate_mouse_up(window, grid_at, gpui::MouseButton::Left, none);
                rig.cx.run_until_parked();
                assert!(chip_of(&rig).is_none(), "{n}: a grid press closes the chip");
                open(&mut rig, -1.0);
                let third = ids[2].clone();
                rig.cx
                    .update(|cx| view.update(cx, |v, cx| v.set_chosen_block(Some(third), cx)));
                rig.cx
                    .update_window(window, |_, window, cx| {
                        use gpui::Focusable;
                        view.focus_handle(cx).focus(window, cx)
                    })
                    .expect("focus");
                rig.cx.simulate_keystrokes(window, "q");
                rig.cx.run_until_parked();
                assert!(chip_of(&rig).is_none(), "{n}: a typed key closes the chip");
                assert!(
                    rig.read(|v| v.chosen_block().is_none()),
                    "{n}: and drops the picked block"
                );
                open(&mut rig, 2.0);
                // Three Returns: `cat`'s echo and its copy of each line scroll the screen a
                // few rows, block 2 still on it.
                rig.read(|v| v.write_to_program(b"\r\r\r"));
                wait_for(&mut rig, |v| v.hovered_block().is_none());
                let screen = rig.read(|v| v.text(crate::grid::TextScope::Screen));
                assert!(
                    screen.contains("$ false") && !screen.starts_with("$ echo hi"),
                    "{n}: scrolled, block 2 still shown: {screen:?}"
                );
                assert!(
                    rig.read(|v| v.hovered_block().is_none()),
                    "{n}: output that moves the block closes the chip"
                );
                assert!(chip_of(&rig).is_none(), "{n}: no chip");
            }
        }
    }
}

/// `command-blocks-alt-screen`: with a full-screen program up (`ESC [?1049h`), no bar, no
/// hairline and no chip; back on the main screen (`ESC [?1049l`) the blocks are as they were.
fn command_blocks_alt_screen_scene() {
    let dims = size(px(480.0), px(240.0));
    for (slug, name) in SHOT_THEMES {
        let theme = shot_theme(name);
        let ui = theme.ui();
        let bg = theme.background;
        let n = format!("command-blocks-alt-screen-{slug}");
        let mut rig = blocks_rig(theme, dims, false, false, |_| ());
        let _ = rig.frame();
        let g = BlockGeometry::of(&rig, false);
        rig.write("\x1b[?1049hvim, full screen\r\n");
        let shot = rig.frame();
        save_shot("command-blocks-alt-screen", slug, &shot);
        let inks = [ui.block_ok, ui.block_failed, ui.block_running, ui.accent];
        for row in 0..10 {
            for c in inks {
                assert!(
                    !is_colour(&shot, dims, g.bar_x, g.mid(row), c),
                    "{n}: a bar on the alternate screen at row {row}"
                );
            }
            assert!(
                is_colour(&shot, dims, g.sep_x, g.top(row) + 0.5, bg),
                "{n}: a hairline on the alternate screen above row {row}"
            );
        }
        point_at(&mut rig, point(px(g.bar_x), px(g.mid(4))));
        assert!(rig.read(|v| v.hovered_block().is_none()), "{n}: no hover");
        assert!(chip_of(&rig).is_none(), "{n}: no chip");
        rig.write("\x1b[?1049l");
        let back = rig.frame();
        assert!(
            is_colour(&back, dims, g.bar_x, g.mid(4), ui.block_failed),
            "{n}: back on the main screen, the bars are back"
        );
        assert!(
            is_colour(&back, dims, g.sep_x, g.top(5) + 0.5, ui.card_border),
            "{n}: and the hairlines"
        );
    }
}

/// `command-blocks-narrow`: a split's pane with a 200 pt grid: the bars 6 pt left of the grid,
/// the chip's buttons short (`Cmd`, `Output`, `Rerun`), the chip one row tall and inside the
/// pane.
fn command_blocks_narrow_scene() {
    // 12 pt of split padding each side: a 200 pt grid area.
    let dims = size(px(224.0), px(240.0));
    for (slug, name) in SHOT_THEMES {
        let theme = shot_theme(name);
        let ui = theme.ui();
        let n = format!("command-blocks-narrow-{slug}");
        let mut rig = blocks_rig(theme, dims, true, false, |_| ());
        let _ = rig.frame();
        let g = BlockGeometry::of(&rig, true);
        point_at(&mut rig, point(px(g.bar_x), px(g.mid(4))));
        let shot = rig.frame();
        save_shot("command-blocks-narrow", slug, &shot);
        assert!(
            is_colour(&shot, dims, g.bar_x, g.mid(0), ui.block_ok),
            "{n}: the bar 6 pt left of the grid: {:?}",
            pixel_at(&shot, dims, g.bar_x, g.mid(0))
        );
        let (label, buttons, chip) = chip_of(&rig).expect("the chip is open");
        assert_eq!(buttons, ["Cmd", "Output", "Rerun"], "{n}: {label:?}");
        let grid = rig.read(|v| v.grid_bounds()).expect("painted");
        assert!(
            chip.right() <= grid.right(),
            "{n}: the chip's right edge {:?} is inside the grid ({:?})",
            chip.right(),
            grid.right()
        );
        assert!(
            f32::from(chip.left()) >= g.origin_x,
            "{n}: the chip starts right of the gutter"
        );
        let (row, tall) = (g.top(5) - g.top(4), f32::from(chip.size.height));
        assert!((tall - row).abs() <= 0.5, "{n}: one row tall, saw {tall}");
    }
}

/// `command-blocks-small-font`: a 9 pt terminal (12 pt rows): the chip is still one row tall,
/// its text smaller to fit and inked inside it, and the pixel rows just above and below it are
/// as they were before it opened.
fn command_blocks_small_font_scene() {
    let dims = size(px(480.0), px(240.0));
    for (slug, name) in SHOT_THEMES {
        let theme = shot_theme(name);
        let ui = theme.ui();
        let n = format!("command-blocks-small-font-{slug}");
        let mut rig = blocks_rig(theme, dims, false, false, |a| a.font_size = 9.0);
        let before = rig.frame();
        let g = BlockGeometry::of(&rig, false);
        point_at(&mut rig, point(px(g.bar_x), px(g.mid(4))));
        let shown = rig.frame();
        save_shot("command-blocks-small-font", slug, &shown);
        let (_, _, chip) = chip_of(&rig).expect("the chip is open");
        let (row, tall) = (g.top(5) - g.top(4), f32::from(chip.size.height));
        assert!(row < 13.0, "{n}: a short row, {row} pt");
        assert!((tall - row).abs() <= 0.5, "{n}: one row tall, saw {tall}");
        let grid = rig.read(|v| v.grid_bounds()).expect("painted");
        let scale = shown.width as f32 / f32::from(dims.width);
        let (x0, x1) = (g.origin_x, f32::from(grid.right()));
        for (y, what) in [
            (f32::from(chip.top()) - 1.0 / scale, "above"),
            (f32::from(chip.bottom()), "below"),
        ] {
            let change = first_change(&before, &shown, dims, (x0, y), (x1, y));
            assert!(
                change.is_none(),
                "{n}: the chip changed the pixel row {what} it: {change:?}"
            );
        }
        // Its words are drawn: pixels well off the panel's colour inside it.
        let inked = (0..(f32::from(chip.size.width) * scale) as usize)
            .flat_map(|dx| (0..(tall * scale) as usize).map(move |dy| (dx as f32, dy as f32)))
            .filter(|&(dx, dy)| {
                let p = pixel_at(
                    &shown,
                    dims,
                    f32::from(chip.left()) + (dx + 0.5) / scale,
                    f32::from(chip.top()) + (dy + 0.5) / scale,
                );
                p.iter()
                    .zip([ui.panel.r, ui.panel.g, ui.panel.b])
                    .any(|(a, b)| a.abs_diff(b) > 60)
            })
            .count();
        assert!(
            inked > 40,
            "{n}: the chip's text is drawn ({inked} inked pixels)"
        );
    }
}

/// `command-blocks-rprompt`: block 2's command row (`$ false`) given a right prompt, a run of
/// text after two or more blank cells ending a cell short of the right edge (as zsh draws an
/// RPROMPT, powerlevel10k's `✔ / 15:05:41` among them):
/// - `-room`: a short one leaves room for the chip with its long buttons: they stay long and
///   the chip ends before it;
/// - `-tight`: a longer one leaves less than the short chip would take whole (but more than
///   its narrowest): the buttons go short (`Cmd`, `Output`, `Rerun`), the command is cut, and
///   the chip ends exactly a cell before it;
/// - `-overlap`: one that leaves less than the chip's narrowest: the chip keeps that width,
///   its short buttons inside it, over the start of the run.
///
/// Where the chip ends before the run, the run's pixels are as they were before it opened.
fn command_blocks_rprompt_scene() {
    use crate::block_view::{CHIP_GAP, CHIP_MIN_WIDTH, CHIP_PAD};
    let dims = size(px(480.0), px(240.0));
    for (slug, name) in SHOT_THEMES {
        let theme = shot_theme(name);
        for (case, run, long) in [
            ("room", "ok / 15:05:41", true),
            ("tight", "", false),
            (
                "overlap",
                "ok / 15:05:41 / ~/code/fuxx-cli / main / x",
                false,
            ),
        ] {
            let n = format!("command-blocks-rprompt-{case}-{slug}");
            let mut rig = blocks_rig(theme, dims, false, false, |_| ());
            let _ = rig.frame();
            let g = BlockGeometry::of(&rig, false);
            let grid = rig.read(|v| v.grid_bounds()).expect("painted");
            let cell = f32::from(rig.cell(0, 0).size.width);
            let cols = (f32::from(grid.size.width) / cell).round() as usize;
            // The run ends one cell short of the right edge (zsh's ZLE_RPROMPT_INDENT). The
            // tight one starts a cell past where the narrowest chip would end.
            let run = match case {
                "tight" => {
                    let start = ((CHIP_GAP + CHIP_MIN_WIDTH) / cell).ceil() as usize + 2;
                    let text = "ok / 15:05:41 / ~/code/fuxx-cli/src/terminal_view.rs";
                    let cut: String = text.chars().take(cols - 1 - start).collect();
                    cut.trim_end().to_string()
                }
                _ => run.to_string(),
            };
            let run = run.as_str();
            let len = run.chars().count();
            let start = cols - 1 - len;
            rig.write(&format!("\x1b7\x1b[5;{}H{run}\x1b8", start + 1));
            let before = rig.frame();
            assert!(
                rig.read(|v| v.text(crate::grid::TextScope::Screen))
                    .contains(&format!("$ false{}{run}", " ".repeat(start - 7))),
                "{n}: the run is on the command row"
            );
            point_at(&mut rig, point(px(g.bar_x), px(g.mid(4))));
            let shown = rig.frame();
            save_shot(&format!("command-blocks-rprompt-{case}"), slug, &shown);
            let (label, buttons, chip) = chip_of(&rig).expect("the chip is open");
            assert!(label.contains("exit 1"), "{n}: {label:?}");
            let (row, tall) = (g.top(5) - g.top(4), f32::from(chip.size.height));
            assert!((tall - row).abs() <= 0.5, "{n}: one row tall, saw {tall}");
            let want = if long {
                ["Copy command", "Copy output", "Rerun"]
            } else {
                ["Cmd", "Output", "Rerun"]
            };
            assert_eq!(buttons, want, "{n}");
            let run_left = f32::from(rig.cell(start, 4).left());
            let right = f32::from(chip.right());
            let (left, width) = (f32::from(chip.left()), f32::from(chip.size.width));
            assert!(
                (left - (g.origin_x + CHIP_GAP)).abs() <= 0.5,
                "{n}: the left anchor is the gutter's"
            );
            // Whatever its width, the buttons fit inside it.
            let (bounds, _, _) = rig.read(|v| v.chip_buttons_shot()).expect("the chip");
            for (i, b) in bounds.iter().enumerate() {
                let b = b.expect("laid out");
                assert!(
                    f32::from(b.right()) <= right - CHIP_PAD + 0.5,
                    "{n}: button {i} ({:?}) inside the chip ({right})",
                    b.right()
                );
            }
            if case == "overlap" {
                assert!(
                    (width - CHIP_MIN_WIDTH).abs() <= 0.5,
                    "{n}: at its narrowest ({CHIP_MIN_WIDTH}), saw {width}"
                );
                assert!(right > run_left, "{n}: over the run's start");
            } else {
                // GPUI rounds the chip's left and its max width to device pixels separately, so
                // its right edge lands within (strictly less than) one device pixel of
                // `run_left - cell`: 1 pt at 1x, 0.5 pt at 2x. A real overlap is a whole cell.
                let scale = shown.width as f32 / f32::from(dims.width);
                let dp = 1.0 / scale;
                assert!(
                    right < run_left - cell + dp,
                    "{n}: the chip ends at {right}, a cell before the run at {run_left}"
                );
                if case == "tight" {
                    assert!(
                        (right - (run_left - cell)).abs() < dp && width >= CHIP_MIN_WIDTH,
                        "{n}: cut to end a cell before the run ({run_left}), at {right}"
                    );
                }
                // From the first device pixel wholly right of the chip: its edge is rounded to
                // the pixel grid, so it may end up to (just under) a device pixel past
                // `run_left - cell`.
                let from = ((run_left - cell) * scale).ceil() / scale;
                let change = first_change(
                    &before,
                    &shown,
                    dims,
                    (from, g.top(4)),
                    (f32::from(grid.right()), g.top(5)),
                );
                assert!(
                    change.is_none(),
                    "{n}: the run's pixels changed: {change:?}"
                );
            }
        }
    }
}

/// `command-blocks-off`: with `command-blocks = off`, the same terminal draws no bar or
/// hairline, and hovering the gutter opens nothing.
fn command_blocks_off_scene() {
    let dims = size(px(480.0), px(240.0));
    for (slug, name) in SHOT_THEMES {
        let theme = shot_theme(name);
        let ui = theme.ui();
        let bg = theme.background;
        let n = format!("command-blocks-off-{slug}");
        let mut rig = blocks_rig(theme, dims, false, false, |a| a.command_blocks = false);
        let shot = rig.frame();
        save_shot("command-blocks-off", slug, &shot);
        let g = BlockGeometry::of(&rig, false);
        for row in 0..10 {
            assert!(
                is_colour(&shot, dims, g.bar_x, g.mid(row), bg),
                "{n}: nothing in the gutter at row {row}"
            );
            assert!(
                is_colour(&shot, dims, g.sep_x, g.top(row) + 0.5, bg),
                "{n}: no hairline above row {row}"
            );
        }
        assert!(
            !is_colour(&shot, dims, g.bar_x, g.mid(4), ui.block_failed),
            "{n}: no red bar"
        );
        point_at(&mut rig, point(px(g.bar_x), px(g.mid(4))));
        assert!(rig.read(|v| v.hovered_block().is_none()), "{n}: no hover");
        assert!(chip_of(&rig).is_none(), "{n}: no chip");
    }
}

/// Give a `command-blocks*` rig fuxx's key bindings and the focus, so keys reach its terminal.
fn keyed(rig: &mut CursorRig) {
    use gpui::Focusable;
    let view = rig.view.clone();
    rig.cx.update(|cx| cx.bind_keys(crate::key_bindings()));
    rig.cx
        .update_window(rig.window, |_, window, cx| {
            view.focus_handle(cx).focus(window, cx)
        })
        .expect("focus");
    rig.cx.run_until_parked();
}

fn press(rig: &mut CursorRig, keys: &str) {
    let window = rig.window;
    rig.cx.simulate_keystrokes(window, keys);
    let _ = rig.frame();
}

fn chosen(rig: &CursorRig) -> Option<String> {
    rig.read(|v| v.chosen_block().map(str::to_string))
}

fn clipboard(rig: &mut CursorRig) -> Option<String> {
    rig.cx
        .update(|cx| cx.read_from_clipboard().and_then(|c| c.text()))
}

fn set_clipboard(rig: &mut CursorRig, text: &str) {
    rig.cx
        .update(|cx| cx.write_to_clipboard(gpui::ClipboardItem::new_string(text.into())));
}

/// Draw frames until the clipboard isn't `was` (a copy's text is read on the background
/// executor), at most 3 s; then the clipboard.
fn copied(rig: &mut CursorRig, was: &str) -> Option<String> {
    let start = std::time::Instant::now();
    while clipboard(rig).as_deref() == Some(was)
        && start.elapsed() < std::time::Duration::from_secs(3)
    {
        std::thread::sleep(std::time::Duration::from_millis(10));
        let _ = rig.frame();
    }
    clipboard(rig)
}

/// Press chip button `i` (0 Copy command, 1 Copy output, 2 Rerun) of the open chip.
fn press_chip_button(rig: &mut CursorRig, i: usize) {
    let (buttons, _, _) = rig
        .read(|v| v.chip_buttons_shot())
        .expect("the chip is open");
    let at = buttons[i].expect("the button was laid out").center();
    let window = rig.window;
    rig.cx.simulate_click(window, at, gpui::Modifiers::none());
    let _ = rig.frame();
}

/// `command-blocks-jump` (M11l Task 5), in both themes: the blocks text with 40 lines of
/// output under `echo hi`, five rows tall (so `false`'s first prompt row is just above the
/// view). ⇧⌘↑ picks `sleep 9` (all in view: no scroll), again `false`, scrolled so its first
/// prompt row is the top row with its bar in the accent down both rows; ⇧⌘↓ twice comes back
/// to the bottom with nothing picked; ⌘↑ still scrolls a page; a typed key drops the pick.
fn command_blocks_jump_scene() {
    let dims = size(px(480.0), px(120.0));
    for (slug, name) in SHOT_THEMES {
        let theme = shot_theme(name);
        let ui = theme.ui();
        let n = format!("command-blocks-jump-{slug}");
        let mut rig = CursorRig::sized(
            theme,
            &blocks_text_with(40),
            dims,
            false,
            (false, false, false),
            |_| (),
        );
        keyed(&mut rig);
        let _ = rig.frame();
        let g = BlockGeometry::of(&rig, false);
        let line_height = f32::from(rig.cell(0, 1).top() - rig.cell(0, 0).top());
        let grid = rig.read(|v| v.grid_bounds()).expect("painted");
        let rows = (f32::from(grid.size.height) / line_height).round() as usize;
        let blocks = rig.read(|v| v.shot_blocks());
        let ids: Vec<String> = blocks.iter().map(|b| b.id.clone()).collect();
        assert_eq!(ids.len(), 3, "{n}: three blocks");
        assert!(
            blocks[1].prompt_top.0 < 0,
            "{n}: false's prompt starts above the view ({rows} rows): {blocks:?}"
        );
        let offset = |rig: &CursorRig| rig.read(|v| v.display_offset());
        assert_eq!(offset(&rig), 0);

        press(&mut rig, "cmd-shift-up");
        assert_eq!(chosen(&rig), Some(ids[2].clone()), "{n}: sleep 9 picked");
        assert_eq!(offset(&rig), 0, "{n}: all in view, no scroll");

        press(&mut rig, "cmd-shift-up");
        assert_eq!(chosen(&rig), Some(ids[1].clone()), "{n}: false picked");
        let top = blocks[1].prompt_top.0;
        assert_eq!(
            offset(&rig) as i32,
            -top,
            "{n}: its first prompt row is the top row"
        );
        let shot = rig.frame();
        save_shot("command-blocks-jump", slug, &shot);
        for row in [0, 1] {
            assert!(
                is_colour(&shot, dims, g.bar_x, g.mid(row), ui.accent),
                "{n}: the picked bar at row {row}: {:?}",
                pixel_at(&shot, dims, g.bar_x, g.mid(row))
            );
        }
        let w = ink_width(
            &shot,
            dims,
            theme.background,
            g.mid(1),
            g.bar_x - 4.0,
            g.bar_x + 4.0,
        );
        assert!((2.5..=3.5).contains(&w), "{n}: a 3 pt bar, saw {w}");

        press(&mut rig, "cmd-shift-down");
        assert_eq!(chosen(&rig), Some(ids[2].clone()), "{n}: back to sleep 9");
        press(&mut rig, "cmd-shift-down");
        assert_eq!(chosen(&rig), None, "{n}: past the newest, nothing picked");
        assert_eq!(offset(&rig), 0, "{n}: at the bottom");
        // Past the oldest, ⇧⌘↑ stays.
        for _ in 0..3 {
            press(&mut rig, "cmd-shift-up");
        }
        assert_eq!(chosen(&rig), Some(ids[0].clone()), "{n}: echo hi picked");
        let at_oldest = offset(&rig);
        press(&mut rig, "cmd-shift-up");
        assert_eq!(
            chosen(&rig),
            Some(ids[0].clone()),
            "{n}: stays on the oldest"
        );
        assert_eq!(offset(&rig), at_oldest, "{n}: and doesn't scroll");
        press(&mut rig, "cmd-shift-down");
        press(&mut rig, "cmd-shift-down");
        press(&mut rig, "cmd-shift-down");
        assert_eq!((chosen(&rig), offset(&rig)), (None, 0), "{n}: bottom again");

        // ⌘↑ still scrolls a page.
        press(&mut rig, "cmd-up");
        assert_eq!(offset(&rig), rows, "{n}: ⌘↑ scrolled {rows} rows");
        press(&mut rig, "cmd-down");
        assert_eq!(offset(&rig), 0, "{n}: ⌘↓ back down");

        // A typed key drops the pick.
        press(&mut rig, "cmd-shift-up");
        assert!(chosen(&rig).is_some());
        press(&mut rig, "q");
        assert_eq!(chosen(&rig), None, "{n}: typing drops the pick");

        // On the alternate screen the keys do nothing.
        rig.write("\x1b[?1049h");
        press(&mut rig, "cmd-shift-up");
        assert_eq!(chosen(&rig), None, "{n}: no pick on the alternate screen");
        rig.write("\x1b[?1049l");
        // With command blocks off, nothing either.
        rig.appearance(|a| a.command_blocks = false);
        press(&mut rig, "cmd-shift-up");
        assert_eq!(chosen(&rig), None, "{n}: no pick with command blocks off");
    }
}

/// `command-blocks-copy` (M11l Task 5), in both themes, the blocks text: with nothing picked
/// ⇧⌘C copies the newest finished block's output (`false`: none, so the clipboard is left
/// as it was); a click on
/// `echo hi`'s bar picks it (no scroll) and ⇧⌘C copies `hi`; its chip's Copy command copies
/// `echo hi` and Copy output `hi`; the palette's Copy Command and Output copies
/// `$ echo hi\nhi`; a click on `sleep 9`'s bar picks it, still no scroll, and ⇧⌘C copies its
/// output so far. With command blocks off, ⇧⌘C copies nothing.
fn command_blocks_copy_scene() {
    use crate::terminal_view::{CopyCommand, CopyCommandAndOutput};
    let dims = size(px(480.0), px(240.0));
    for (slug, name) in SHOT_THEMES {
        let theme = shot_theme(name);
        let n = format!("command-blocks-copy-{slug}");
        let mut rig = blocks_rig(theme, dims, false, false, |_| ());
        let _keep = ClipboardKeeper::new(&rig);
        keyed(&mut rig);
        let _ = rig.frame();
        let g = BlockGeometry::of(&rig, false);
        let ids = rig.read(|v| v.block_ids());
        let window = rig.window;
        let none = gpui::Modifiers::none();

        set_clipboard(&mut rig, "before");
        press(&mut rig, "cmd-shift-c");
        // Waits the full 3 s for a change that never comes: nothing is written.
        assert_eq!(
            copied(&mut rig, "before").as_deref(),
            Some("before"),
            "{n}: false's output is empty: the clipboard keeps what it had"
        );
        assert_eq!(chosen(&rig), None, "{n}: ⇧⌘C picks nothing");

        // A click on echo hi's bar picks it.
        rig.cx
            .simulate_click(window, point(px(g.bar_x), px(g.mid(0))), none);
        let _ = rig.frame();
        assert_eq!(chosen(&rig), Some(ids[0].clone()), "{n}: echo hi picked");
        set_clipboard(&mut rig, "before");
        press(&mut rig, "cmd-shift-c");
        assert_eq!(copied(&mut rig, "before").as_deref(), Some("hi"), "{n}");

        // Its chip: Copy command, then Copy output.
        point_at(&mut rig, point(px(g.bar_x), px(g.mid(0))));
        let _ = rig.frame();
        assert_eq!(
            rig.read(|v| v.hovered_block().map(str::to_string)),
            Some(ids[0].clone())
        );
        set_clipboard(&mut rig, "before");
        press_chip_button(&mut rig, 0);
        assert_eq!(clipboard(&mut rig).as_deref(), Some("echo hi"), "{n}");
        set_clipboard(&mut rig, "before");
        press_chip_button(&mut rig, 1);
        assert_eq!(copied(&mut rig, "before").as_deref(), Some("hi"), "{n}");
        assert!(chip_of(&rig).is_some(), "{n}: the chip stays");
        assert_eq!(chosen(&rig), Some(ids[0].clone()), "{n}: the pick is kept");

        // The palette's Copy Command and Output (dispatched as the palette does).
        set_clipboard(&mut rig, "before");
        rig.cx
            .update_window(window, |_, window, cx| {
                window.dispatch_action(Box::new(CopyCommandAndOutput), cx)
            })
            .expect("dispatch");
        assert_eq!(
            copied(&mut rig, "before").as_deref(),
            Some("$ echo hi\nhi"),
            "{n}"
        );
        set_clipboard(&mut rig, "before");
        rig.cx
            .update_window(window, |_, window, cx| {
                window.dispatch_action(Box::new(CopyCommand), cx)
            })
            .expect("dispatch");
        let _ = rig.frame();
        assert_eq!(clipboard(&mut rig).as_deref(), Some("echo hi"), "{n}");

        // A click on sleep 9's bar picks it, and nothing scrolls.
        rig.cx
            .simulate_click(window, point(px(g.bar_x), px(g.mid(5))), none);
        let shot = rig.frame();
        save_shot("command-blocks-copy", slug, &shot);
        assert_eq!(chosen(&rig), Some(ids[2].clone()), "{n}: sleep 9 picked");
        assert_eq!(rig.read(|v| v.display_offset()), 0, "{n}: no scroll");
        set_clipboard(&mut rig, "before");
        press(&mut rig, "cmd-shift-c");
        assert_eq!(
            copied(&mut rig, "before").as_deref(),
            Some("one\ntwo"),
            "{n}: a running block's output so far"
        );

        // Off: nothing is copied.
        rig.appearance(|a| a.command_blocks = false);
        set_clipboard(&mut rig, "before");
        press(&mut rig, "cmd-shift-c");
        std::thread::sleep(std::time::Duration::from_millis(100));
        let _ = rig.frame();
        assert_eq!(clipboard(&mut rig).as_deref(), Some("before"), "{n}: off");
    }
}

/// `command-blocks-rerun` (M11l Task 5), in both themes, the blocks text in `cat` at its live
/// prompt (the tagger knows where typing starts): `echo hi`'s chip offers Rerun; pressed, the
/// terminal shows `echo hi` after the live prompt (the tty's echo of what was sent) and the
/// cursor stays on that row (no Return). Then the cursor has left the input start: Rerun is
/// dimmed, its tooltip says why, and pressing it changes nothing.
fn command_blocks_rerun_scene() {
    use crate::block_view::Refusal;
    use crate::grid::TextScope;
    let dims = size(px(480.0), px(240.0));
    for (slug, name) in SHOT_THEMES {
        let theme = shot_theme(name);
        let ui = theme.ui();
        let n = format!("command-blocks-rerun-{slug}");
        let mut rig = blocks_rig(theme, dims, false, false, |_| ());
        keyed(&mut rig);
        let _ = rig.frame();
        let g = BlockGeometry::of(&rig, false);
        let ids = rig.read(|v| v.block_ids());
        let screen = |rig: &CursorRig| rig.read(|v| v.text(TextScope::Screen));
        let before = screen(&rig);

        point_at(&mut rig, point(px(g.bar_x), px(g.mid(0))));
        let _ = rig.frame();
        let (_, _, rerun) = rig.read(|v| v.chip_buttons_shot()).expect("the chip");
        assert_eq!(rerun, Ok(()), "{n}: Rerun offered at the empty prompt");
        let shown = rig.frame();
        save_shot("command-blocks-rerun", slug, &shown);
        press_chip_button(&mut rig, 2);
        wait_for(&mut rig, |v| {
            v.text(TextScope::Screen).lines().nth(8) == Some("$ echo hi")
        });
        let after = screen(&rig);
        assert_eq!(
            after.lines().nth(8),
            Some("$ echo hi"),
            "{n}: typed after the live prompt: {after:?}"
        );
        // No Return: `cat` never gets a line to copy back.
        std::thread::sleep(std::time::Duration::from_millis(200));
        let _ = rig.frame();
        let settled = screen(&rig);
        assert_eq!(settled, after, "{n}: nothing more came");
        assert_eq!(
            settled.lines().count(),
            before.lines().count(),
            "{n}: the cursor stayed on the prompt's row"
        );
        assert!(chip_of(&rig).is_some(), "{n}: the chip stays");

        // Typed now (the cursor left the input start): Rerun is disabled.
        let (buttons, enabled_now, rerun) = rig.read(|v| v.chip_buttons_shot()).expect("the chip");
        assert_eq!(enabled_now, [true, true, false], "{n}");
        assert_eq!(rerun, Err(Refusal::Typed), "{n}");
        let dim = rig.frame();
        save_shot("command-blocks-rerun-disabled", slug, &dim);
        // Its label in the disabled ink: no pixel of it is as strong as `text_dim`.
        let b = buttons[2].expect("Rerun laid out");
        let strongest = (0..(f32::from(b.size.width) as usize))
            .map(|dx| {
                pixel_at(
                    &dim,
                    dims,
                    f32::from(b.left()) + dx as f32,
                    f32::from(b.center().y),
                )
            })
            .map(|p| {
                crate::ui_tokens::contrast(
                    alacritty_terminal::vte::ansi::Rgb {
                        r: p[0],
                        g: p[1],
                        b: p[2],
                    },
                    ui.panel,
                )
            })
            .fold(0.0f32, f32::max);
        let enabled = crate::ui_tokens::contrast(ui.text_dim, ui.panel);
        assert!(
            strongest > 1.05 && strongest < enabled - 0.3,
            "{n}: Rerun's label {strongest:.2} vs enabled {enabled:.2}"
        );
        press_chip_button(&mut rig, 2);
        std::thread::sleep(std::time::Duration::from_millis(200));
        let _ = rig.frame();
        assert_eq!(screen(&rig), settled, "{n}: a disabled Rerun types nothing");
        let _ = ids;

        rerun_odd_blocks(theme, &n);
    }
}

/// The `command-blocks-rerun` checks on a second terminal: a `t=1` block (its command was too
/// long to record; no output either) and a two-line command, at a live prompt whose shell
/// asked for bracketed paste. The `t=1` block's chip has all three buttons disabled; pressing
/// them keeps the chip open and sends and copies nothing. The two-line command's Rerun sends exactly
/// `ESC [200~echo a\necho b ESC [201~`: wrapped, with no `\r`.
fn rerun_odd_blocks(theme: Theme, n: &str) {
    use crate::block_view::Refusal;
    const A: &str = "\x1b]133;A\x07";
    const B: &str = "\x1b]133;B\x07";
    let c = |cmd: &str| format!("\x1b]133;C;cmdline_url={cmd}\x07");
    let d0 = "\x1b]133;D;0\x07";
    let text = [
        // Row 0: a command longer than CMD_CAP (recorded as t=1, no command).
        format!("{A}$ {B}long\r\n{}{d0}", c(&"x".repeat(2100))),
        // Rows 1-2: a two-line command; rows 3-4 its output.
        format!(
            "{A}$ {B}echo a\r\n> echo b\r\n{}a\r\nb\r\n{d0}",
            c("echo%20a%0Aecho%20b")
        ),
        // Row 5: the live prompt, the shell asking for bracketed paste (as zsh does).
        format!("{A}$ {B}\x1b[?2004h"),
    ]
    .concat();
    let dims = size(px(480.0), px(240.0));
    let mut rig = CursorRig::sized(theme, &text, dims, false, (false, false, false), |_| ());
    let _keep = ClipboardKeeper::new(&rig);
    keyed(&mut rig);
    let _ = rig.frame();
    let g = BlockGeometry::of(&rig, false);
    let blocks = rig.read(|v| v.shot_blocks());
    assert_eq!(blocks.len(), 2, "{n}: {blocks:?}");
    assert_eq!(blocks[0].cmd, None, "{n}: t=1, no command");
    let view = rig.view.clone();
    let take_sent = |rig: &mut CursorRig| rig.cx.update(|cx| view.update(cx, |v, _| v.take_sent()));
    let _ = take_sent(&mut rig);

    // The t=1 block (no output either): every button disabled.
    point_at(&mut rig, point(px(g.bar_x), px(g.mid(0))));
    let _ = rig.frame();
    let (_, enabled, rerun) = rig.read(|v| v.chip_buttons_shot()).expect("the chip");
    assert_eq!(enabled, [false, false, false], "{n}: t=1 block's buttons");
    assert_eq!(rerun, Err(Refusal::NoCommand), "{n}");
    set_clipboard(&mut rig, "before");
    for i in [0, 1, 2] {
        press_chip_button(&mut rig, i);
        std::thread::sleep(std::time::Duration::from_millis(50));
        let _ = rig.frame();
        assert!(
            chip_of(&rig).is_some(),
            "{n}: a disabled button keeps the chip"
        );
        assert_eq!(
            rig.read(|v| v.hovered_block().map(str::to_string)),
            Some(blocks[0].id.clone()),
            "{n}: still the t=1 block's chip"
        );
        assert_eq!(
            clipboard(&mut rig).as_deref(),
            Some("before"),
            "{n}: copied nothing"
        );
        assert!(take_sent(&mut rig).is_empty(), "{n}: sent nothing");
        assert_eq!(chosen(&rig), None, "{n}: picked nothing");
    }

    // The two-line command: Rerun sends it wrapped, no `\r`.
    point_at(&mut rig, point(px(g.bar_x), px(g.mid(1))));
    let _ = rig.frame();
    assert_eq!(
        rig.read(|v| v.hovered_block().map(str::to_string)),
        Some(blocks[1].id.clone()),
        "{n}: the two-line command's chip"
    );
    let (_, enabled, rerun) = rig.read(|v| v.chip_buttons_shot()).expect("the chip");
    assert_eq!((enabled, rerun), ([true, true, true], Ok(())), "{n}");
    press_chip_button(&mut rig, 2);
    let sent = take_sent(&mut rig);
    assert_eq!(
        sent,
        b"\x1b[200~echo a\necho b\x1b[201~",
        "{n}: {:?}",
        String::from_utf8_lossy(&sent)
    );
    assert!(!sent.contains(&b'\r'), "{n}: no carriage return");
}

/// Keeps the clipboard as it was before a scene that copies (the shots run on the real
/// macOS platform, so they use your clipboard): saved when made, put back when dropped, a
/// failing scene's unwinding included. Made after its rig, so dropped before it.
struct ClipboardKeeper {
    app: std::rc::Rc<gpui::AppCell>,
    saved: Option<gpui::ClipboardItem>,
}

impl ClipboardKeeper {
    fn new(rig: &CursorRig) -> Self {
        let app = rig.cx.app.clone();
        let saved = app.borrow().read_from_clipboard();
        Self { app, saved }
    }
}

impl Drop for ClipboardKeeper {
    fn drop(&mut self) {
        // Not while the app is borrowed (a panic inside an update): never a second panic.
        if let Ok(app) = self.app.try_borrow_mut() {
            let item = self
                .saved
                .take()
                .unwrap_or_else(|| gpui::ClipboardItem::new_string(String::new()));
            app.write_to_clipboard(item);
        }
    }
}
