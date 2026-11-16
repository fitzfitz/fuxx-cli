//! Off-screen UI screenshots (M11), compiled only with the `ui-shots` cargo feature.
//!
//! `shoot` renders a view in a hidden GPUI window and keeps the shot in memory for the scene's
//! checks; with `--png` (or `UI_SHOTS_PNG=1`) it also saves `target/ui-shots/<name>-<slug>.png`,
//! so a UI task can look at its scenes in Catppuccin Mocha and Catppuccin Latte.
//!
//! AppKit only makes windows on the main thread and `cargo test` runs every test on a spawned one
//! (it aborts with "Rust cannot catch foreign exceptions"), so the scenes are not `#[test]`s.
//! They are plain functions declared in `SCENES` and run from `main` on the main thread:
//! `cargo run --features ui-shots -- __ui-shots [filter] [--png] [--all-themes]`. A scene panics
//! when its checks fail; the run goes on, writes that scene's shots to
//! `target/ui-shots/failed/<scene>/`, and exits 1 at the end.

use std::cell::Cell;
use std::panic::AssertUnwindSafe;
use std::path::PathBuf;
use std::process::exit;
use std::sync::OnceLock;
use std::time::Instant;

use gpui::VisualTestAppContext;
use gpui::{
    AnyView, AnyWindowHandle, App, BoxShadow, Context, Div, Font, Hsla, IntoElement, Pixels,
    Render, Size, Window, div, point, prelude::*, px, size,
};
use gpui_platform::current_platform;

use crate::appearance::Appearance;
use crate::chrome_cache::{self, Counted};
use crate::fonts;
use crate::procs::ProgramState;
use crate::split_tree::Axis;
use crate::status::Status;
use crate::status_mark::{self, BREATHS_PER_BEAT, TURNS_PER_BEAT};
use crate::terminal_view::hsla;
use crate::theme::{self, Theme, UiColors};
use crate::ui_tokens::{self, contrast};
use crate::workspace::{Overlay, Workspace};

mod args;
mod branch;
mod changes;
mod chrome;
mod chrome_parts;
mod file_tab;
mod file_tab_md;
mod harness;
mod panes;
mod right_sidebar;
mod sidebar;
mod window;

pub use args::{RunOptions, parse_args};
pub(super) use harness::*;

/// How many themes a scene is shot in by default (`--all-themes` shoots every scene in both).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Themes {
    /// Catppuccin Mocha only.
    One,
    /// Catppuccin Mocha and Catppuccin Latte.
    Both,
}

/// What a scene's checks read.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Kind {
    /// Pixels.
    Visual,
    /// View state only (bounds, scroll, words, counts).
    State,
}

/// A scene, declared: its name (also its filter and file-name prefix), its function, and what
/// the runner needs to know to schedule it.
pub(super) struct Scene {
    pub(super) name: &'static str,
    pub(super) run: fn(),
    pub(super) themes: Themes,
    /// Must run alone (real-time waits, a child fuxx); read by `--shard` (Task 8).
    #[allow(dead_code)]
    pub(super) serial: bool,
    #[allow(dead_code)]
    pub(super) kind: Kind,
    /// The app files its views come from; read by `--changed` (Task 9).
    #[allow(dead_code)]
    pub(super) sources: &'static [&'static str],
}

/// A scene as every one is until Tasks 4–6 tag them: both themes, not serial, visual, no
/// sources.
const fn scene(name: &'static str, run: fn()) -> Scene {
    Scene {
        name,
        run,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &[],
    }
}

/// This file's scenes (those not yet moved to an area file).
const SCENES: &[Scene] = &[
    scene("api-background", api_background),
    scene("banner-click", banner_click),
    scene("default-name", default_name),
    scene("names", names),
    scene("renames", renames),
    scene("rename-box", rename_box_scenes),
    scene("palette", palette_scene),
    scene("palette-workspaces", palette_workspaces_scene),
    scene("palette-actions", palette_actions_scene),
    scene("palette-keys", palette_keys_scene),
    scene("palette-run", palette_run_scene),
    scene("palette-refresh", palette_refresh_scene),
    scene("palette-recent", palette_recent_scene),
    scene("palette-files", palette_files_scene),
    scene("palette-relist", palette_relist_scene),
    scene("palette-settings", palette_settings_scene),
    scene("palette-narrow", palette_narrow_scene),
    scene("palette-motion", palette_motion_scene),
    scene("welcome", welcome_scene),
    scene("welcome-checking", welcome_checking_scene),
    scene("welcome-keys", welcome_keys_scene),
    scene("welcome-narrow", welcome_narrow_scene),
    scene("welcome-start", welcome_start_scene),
    scene("welcome-folder", welcome_folder_scene),
    scene("welcome-first-run", welcome_first_run_scene),
    scene("welcome-reopen", welcome_reopen_scene),
    scene("overlays", overlays),
    scene("help", help_scenes),
    scene("panels", panel_scenes),
    scene("preferences", preferences_scenes),
    scene("status-bar", status_bar_scenes),
    scene("status-bar-hidden", status_bar_hidden_scenes),
    scene("status-bar-ports", status_bar_ports_scenes),
    scene("status-bar-narrow", status_bar_narrow_scenes),
    scene("update-button", update_button_scenes),
    scene("cursor", cursor_scenes),
    scene("cursor-flood", cursor_flood_scene),
    scene("command-blocks", command_blocks_scene),
    scene("command-blocks-alt-screen", command_blocks_alt_screen_scene),
    scene("command-blocks-narrow", command_blocks_narrow_scene),
    scene("command-blocks-rprompt", command_blocks_rprompt_scene),
    scene("command-blocks-small-font", command_blocks_small_font_scene),
    scene("command-blocks-off", command_blocks_off_scene),
    scene("command-blocks-jump", command_blocks_jump_scene),
    scene("command-blocks-copy", command_blocks_copy_scene),
    scene("command-blocks-rerun", command_blocks_rerun_scene),
];

/// Each area's scenes. A UI task adds its scenes to its area's `SCENES` and their names to `ORDER`.
const AREAS: &[&[Scene]] = &[
    window::SCENES,
    sidebar::SCENES,
    panes::SCENES,
    chrome_parts::SCENES,
    chrome::SCENES,
    file_tab::SCENES,
    file_tab_md::SCENES,
    right_sidebar::SCENES,
    changes::SCENES,
    branch::SCENES,
    SCENES,
];

/// Every scene's name, in run order (the order of a plain run and of `timings.tsv`).
const ORDER: &[&str] = &[
    "smoke",
    "tokens",
    "window",
    "marks",
    "sidebar-marks",
    "cached-panes",
    "chrome-breath-skips-parts",
    "chrome-notify-redraws-parts",
    "chrome-pane-churn",
    "chrome-kill-switch",
    "chrome-pixels-same",
    "chrome-ages-current",
    "chrome-working-marks",
    "chrome-folder-polled",
    "chrome-followed",
    "chrome-title-once",
    "chrome-rows-once",
    "chrome-status-elsewhere",
    "chrome-model-edits",
    "chrome-notifications",
    "chrome-places-programs-ports",
    "chrome-globals",
    "chrome-resize-title",
    "chrome-drag-cancel",
    "chrome-renaming",
    "chrome-new-bar-reveal",
    "chrome-session-facts",
    "chrome-bars-and-edges",
    "chrome-sidebar-scroll",
    "chrome-sidebar-header-mark",
    "chrome-row-sources",
    "chrome-file-tab-toolbar",
    "sidebar",
    "sidebar-rows",
    "sidebar-selection",
    "sidebar-button",
    "title-area",
    "title-bar",
    "tabs",
    "splits",
    "cards",
    "cards-split",
    "cards-corners",
    "workspaces",
    "tab-bars",
    "tab-drag",
    "browser-tab",
    "file-tab",
    "file-diff",
    "file-tab-md",
    "file-tab-md-mermaid",
    "file-tab-scrollbar",
    "right-sidebar",
    "files",
    "files-follow",
    "right-panels",
    "changes",
    "changes-bar",
    "changes-staging",
    "changes-graph",
    "changes-sticky",
    "activity",
    "api-background",
    "banner-click",
    "default-name",
    "names",
    "renames",
    "rename-box",
    "palette",
    "palette-workspaces",
    "palette-actions",
    "palette-keys",
    "palette-run",
    "palette-refresh",
    "palette-recent",
    "palette-files",
    "palette-relist",
    "palette-settings",
    "palette-narrow",
    "palette-motion",
    "branch-switcher",
    "branch-switcher-confirm",
    "branch-switcher-real",
    "branch-switcher-open",
    "welcome",
    "welcome-checking",
    "welcome-keys",
    "welcome-narrow",
    "welcome-start",
    "welcome-folder",
    "welcome-first-run",
    "welcome-reopen",
    "overlays",
    "help",
    "panels",
    "preferences",
    "status-bar",
    "status-bar-hidden",
    "status-bar-ports",
    "status-bar-narrow",
    "update-button",
    "cursor",
    "cursor-flood",
    "command-blocks",
    "command-blocks-alt-screen",
    "command-blocks-narrow",
    "command-blocks-rprompt",
    "command-blocks-small-font",
    "command-blocks-off",
    "command-blocks-jump",
    "command-blocks-copy",
    "command-blocks-rerun",
];

/// Every scene, in `ORDER`. Panics, naming the scene, if one is in no area or in two, or if
/// `ORDER` misses or repeats one.
fn scenes() -> Vec<&'static Scene> {
    let all: Vec<&'static Scene> = AREAS.iter().flat_map(|area| area.iter()).collect();
    for scene in &all {
        let declared = all.iter().filter(|s| s.name == scene.name).count();
        assert!(
            declared == 1,
            "ui-shots: scene {} is declared {declared} times",
            scene.name
        );
        let listed = ORDER.iter().filter(|&&n| n == scene.name).count();
        assert!(
            listed == 1,
            "ui-shots: scene {} is in ORDER {listed} times",
            scene.name
        );
    }
    ORDER
        .iter()
        .map(|&name| {
            *all.iter()
                .find(|s| s.name == name)
                .unwrap_or_else(|| panic!("ui-shots: ORDER names {name}, which no area declares"))
        })
        .collect()
}

static OPTIONS: OnceLock<RunOptions> = OnceLock::new();

/// The run's options (the defaults until `run` sets them).
pub(super) fn options() -> &'static RunOptions {
    OPTIONS.get_or_init(RunOptions::default)
}

thread_local! {
    /// The running scene's `themes` (set by `run` before each scene).
    static SCENE_THEMES: Cell<Themes> = const { Cell::new(Themes::Both) };
}

/// The themes the running scene is shot in: (file slug, built-in theme name). Mocha only for a
/// `Themes::One` scene, unless `--all-themes`.
pub(super) fn shot_themes() -> &'static [(&'static str, &'static str)] {
    const ALL: &[(&str, &str)] = &SHOT_THEMES;
    match SCENE_THEMES.get() {
        Themes::One if !options().all_themes => &ALL[..1],
        _ => ALL,
    }
}

/// Set in a child run of this program (`child_run`): it leaves the summary and `timings.tsv`
/// to its parent.
const CHILD_ENV: &str = "FUXX_SHOTS_CHILD";

/// This program re-run on the scenes matching `filter`, as a child (`FUXX_SHOTS_CHILD=1`),
/// with this run's `--png` and `--all-themes`. The caller adds its own environment.
pub(super) fn child_run(filter: &str) -> std::process::Command {
    let exe = std::env::current_exe().expect("this program");
    let mut command = std::process::Command::new(exe);
    command.args(["__ui-shots", filter]).env(CHILD_ENV, "1");
    if options().png {
        command.arg("--png");
    }
    if options().all_themes {
        command.arg("--all-themes");
    }
    command
}

/// `fuxx __ui-shots [filter] [--png] [--all-themes]` (`args` are those after `__ui-shots`): run
/// the scenes whose name contains `filter`, each to the end even if one fails, then exit (1 if
/// any failed). A failed scene's shots are written to `target/ui-shots/failed/<scene>/`.
pub fn run(args: &[String]) -> ! {
    let mut opts = match parse_args(args) {
        Ok(opts) => opts,
        Err(e) => {
            eprintln!("ui-shots: {e}");
            eprintln!(
                "usage: fuxx __ui-shots [filter] [--png] [--all-themes] [--shard I/N] [--changed <base>] [--update-refs]"
            );
            exit(2);
        }
    };
    if opts.shard.is_some() || opts.changed.is_some() || opts.update_refs {
        eprintln!("ui-shots: --shard, --changed and --update-refs are not built yet");
        exit(2);
    }
    opts.png |= std::env::var("UI_SHOTS_PNG").is_ok_and(|v| v == "1");
    let filter = opts.filter.clone();
    OPTIONS.set(opts).expect("the run's options are set once");
    let started = Instant::now();
    let mut times: Vec<(&str, f64)> = Vec::new();
    let mut failed = 0;
    for scene in scenes() {
        let name = scene.name;
        if filter.as_deref().is_some_and(|f| !name.contains(f)) {
            continue;
        }
        let (rw, sleep, child) = (
            prof::get(prof::RENDER_WITH),
            prof::get(prof::SLEEP),
            prof::get(prof::CHILD),
        );
        forget_shots(name);
        // Process-wide test switches back to their defaults, should a failed scene before
        // have left one set (its unwind skipped the reset).
        chrome_cache::shots_off::set(false);
        SCENE_THEMES.set(scene.themes);
        let t = Instant::now();
        // A failed check panics (the default hook prints it); the run goes on to the next scene.
        let passed = std::panic::catch_unwind(AssertUnwindSafe(scene.run)).is_ok();
        let secs = t.elapsed().as_secs_f64();
        if passed {
            println!("ui-shots: {name} ok ({secs:.1} s)");
        } else {
            failed += 1;
            let dir = write_failed(name);
            println!(
                "ui-shots: {name} FAILED ({secs:.1} s) \u{2014} pictures in {}",
                dir.display()
            );
        }
        if prof::enabled() {
            // Outside `render_with`: scenes that build their own app, waits, probes.
            println!(
                "ui-shots: {name} profile: render_with {:.1} s, other {:.1} s, sleep {:.1} s, child {:.1} s",
                prof::get(prof::RENDER_WITH) - rw,
                secs - (prof::get(prof::RENDER_WITH) - rw),
                prof::get(prof::SLEEP) - sleep,
                prof::get(prof::CHILD) - child,
            );
        }
        times.push((name, secs));
    }
    if times.is_empty() {
        eprintln!("ui-shots: no scene matches {filter:?}");
        exit(1);
    }
    let total = started.elapsed().as_secs_f64();
    // A child process (cache-off, WebKit) is a filtered run of this binary: it leaves the
    // timings and the summary to its parent.
    let child = std::env::var(CHILD_ENV).is_ok_and(|v| v == "1");
    if !child {
        if filter.is_none() {
            let tsv: String = times
                .iter()
                .map(|(n, s)| format!("{n}\t{s:.1}\n"))
                .collect();
            let dir = shots_dir();
            std::fs::create_dir_all(&dir).expect("create target/ui-shots");
            std::fs::write(dir.join("timings.tsv"), tsv).expect("write timings.tsv");
        }
        println!(
            "ui-shots: {} passed, {failed} failed, {total:.1} s",
            times.len() - failed
        );
        let mut slowest = times.clone();
        slowest.sort_by(|a, b| b.1.total_cmp(&a.1));
        for (name, secs) in slowest.iter().take(15) {
            println!("ui-shots:   {secs:6.1} s  {name}");
        }
    }
    if prof::enabled() {
        prof::print_sums();
    }
    exit(if failed > 0 { 1 } else { 0 })
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
    fn every_declared_scene_runs_once_in_order() {
        let declared: usize = AREAS.iter().map(|area| area.len()).sum();
        let run: Vec<&str> = scenes().iter().map(|s| s.name).collect();
        assert_eq!(run, ORDER);
        assert_eq!(run.len(), declared);
    }

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

    /// A `w` × `h` device-pixel shot of `bg`, with column `x` (all rows) mixed `cover` of the
    /// way from `bg` to `ink`.
    fn columns(w: u32, h: u32, bg: [u8; 3], ink: [u8; 3], cover: &[(u32, f32)]) -> Shot {
        let mut rgba = Vec::new();
        for _ in 0..h {
            for x in 0..w {
                let c = cover
                    .iter()
                    .find(|(cx, _)| *cx == x)
                    .map_or(0.0, |(_, c)| *c);
                for k in 0..3 {
                    let v = f32::from(bg[k]) + (f32::from(ink[k]) - f32::from(bg[k])) * c;
                    rgba.push(v.round() as u8);
                }
                rgba.push(255);
            }
        }
        Shot {
            width: w,
            height: h,
            rgba,
        }
    }

    /// A stem whose left edge column is `left` covered and right edge column `right` covered,
    /// solid in between (device columns 9 to 20 of 40, 1x).
    fn stem(bg: [u8; 3], ink: [u8; 3], left: f32, right: f32) -> Shot {
        let mut cover = vec![(9, left), (20, right)];
        cover.extend((10..20).map(|x| (x, 1.0)));
        columns(40, 4, bg, ink, &cover)
    }

    #[test]
    fn ink_span_reads_an_edge_pixel_s_coverage_as_a_fraction_of_it() {
        let (bg, ink) = ([25, 25, 38], [156, 162, 186]);
        let dims = size(px(40.0), px(4.0));
        let shot = stem(bg, ink, 0.4, 0.3);
        let (x0, x1, y0, y1) = ink_span(&shot, dims, (0.0, 0.0, 40.0, 4.0), bg, ink).expect("ink");
        assert!((x0 - 9.6).abs() < 0.02, "left edge {x0}");
        assert!((x1 - 20.3).abs() < 0.02, "right edge {x1}");
        assert_eq!((y0, y1), (0.0, 4.0));
    }

    #[test]
    fn ink_span_moves_a_little_where_ink_box_jumps_a_whole_pixel() {
        // The same edge pixel just under and just over half covered: `ink_box` (a half-way
        // threshold) moves its left edge by a whole device pixel, `ink_span` by 0.04.
        let (bg, ink) = ([232, 235, 239], [90, 90, 108]);
        let dims = size(px(40.0), px(4.0));
        let area = (0.0, 0.0, 40.0, 4.0);
        let under = stem(bg, ink, 0.48, 1.0);
        let over = stem(bg, ink, 0.52, 1.0);
        let jump = ink_box(&under, dims, area, bg, ink).expect("ink").0
            - ink_box(&over, dims, area, bg, ink).expect("ink").0;
        assert_eq!(jump, 1.0);
        let slide = ink_span(&under, dims, area, bg, ink).expect("ink").0
            - ink_span(&over, dims, area, bg, ink).expect("ink").0;
        assert!((slide - 0.04).abs() < 0.02, "moved {slide}");
    }

    #[test]
    fn ink_span_is_the_same_on_a_dark_and_a_light_background() {
        // Light text on dark and dark text on light, the same coverage: the same span.
        let dims = size(px(40.0), px(4.0));
        let area = (0.0, 0.0, 40.0, 4.0);
        let (dark, light) = (
            ([25, 25, 38], [156, 162, 186]),
            ([232, 235, 239], [90, 90, 108]),
        );
        let a = ink_span(
            &stem(dark.0, dark.1, 0.45, 0.35),
            dims,
            area,
            dark.0,
            dark.1,
        );
        let b = ink_span(
            &stem(light.0, light.1, 0.45, 0.35),
            dims,
            area,
            light.0,
            light.1,
        );
        let (a, b) = (a.expect("ink"), b.expect("ink"));
        assert!(
            (a.0 - b.0).abs() < 0.02 && (a.1 - b.1).abs() < 0.02,
            "{a:?} {b:?}"
        );
    }

    #[test]
    fn ink_span_works_in_points_at_2x_and_finds_nothing_on_bare_background() {
        let (bg, ink) = ([25, 25, 38], [156, 162, 186]);
        // 40 device pixels across a 20 pt window: the edges come back in points.
        let dims = size(px(20.0), px(2.0));
        let shot = stem(bg, ink, 0.5, 0.5);
        let (x0, x1, _, _) = ink_span(&shot, dims, (0.0, 0.0, 20.0, 2.0), bg, ink).expect("ink");
        assert!(
            (x0 - 4.75).abs() < 0.02 && (x1 - 10.25).abs() < 0.02,
            "{x0} {x1}"
        );
        // Over the bare background beside the stem: nothing.
        assert_eq!(ink_span(&shot, dims, (12.0, 0.0, 8.0, 2.0), bg, ink), None);
    }
}

/// What a shell shows in the scenes, the same every time.
const SHELL_TEXT: &str = "~/code/api $ cargo test\r\n   Compiling api v0.4.2 (/Users/me/code/api)\r\n    Finished `test` profile [unoptimized + debuginfo] target(s) in 2.31s\r\n     Running unittests src/main.rs\r\n\r\nrunning 42 tests\r\ntest result: ok. 42 passed; 0 failed; 0 ignored\r\n\r\n~/code/api $ ";

const AGENT_TEXT: &str =
    "~/code/api $ claude\r\n\r\n  Reading src/auth.rs\r\n  Editing src/auth.rs (+12 -3)\r\n\r\n> ";

/// What the waiting session shows: a Claude Code style prompt, drawn in a box 40 columns wide so
/// it fits a split pane's grid (about 42 columns, M11i's padding) without wrapping mid-word.
const CLAUDE_PROMPT: &str = "  Editing src/auth.rs (+12 -3)\r\n\r\n╭──────────────────────────────────────╮\r\n│ Edit file                            │\r\n│ src/auth.rs                          │\r\n│                                      │\r\n│ Do you want to edit auth.rs?         │\r\n│ ❯ 1. Yes                             │\r\n│   2. Yes, don't ask again            │\r\n│   3. No, tell Claude what to do      │\r\n╰──────────────────────────────────────╯\r\n";

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

/// The box, `(x0, x1, y0, y1)` in points, round the ink inside `area` (`x`, `y`, `w`, `h` in
/// points), to a fraction of a device pixel: each pixel's coverage is how far it is from `bg`
/// toward `ink` (0 to 1), and each side of the box sits inside its outermost inked pixel by the
/// part of that pixel its column's (or row's) most covered pixel leaves bare. `None` when nothing
/// is inked (no pixel 1/50 of the way). For text, where `ink_box` is not enough: anti-aliased
/// glyph edges cover a pixel part-way, by different amounts in a dark and a light theme
/// (CoreGraphics thickens light-on-dark strokes), so a half-way threshold moves a box edge a
/// whole device pixel when an edge pixel's coverage crosses one half; here it moves smoothly.
fn ink_span(
    shot: &Shot,
    dims: Size<Pixels>,
    area: (f32, f32, f32, f32),
    bg: [u8; 3],
    ink: [u8; 3],
) -> Option<(f32, f32, f32, f32)> {
    const FAINT: f32 = 0.02;
    let towards: [f32; 3] = std::array::from_fn(|k| f32::from(ink[k]) - f32::from(bg[k]));
    let full: f32 = towards.iter().map(|d| d * d).sum();
    let scale = shot.width as f32 / f32::from(dims.width);
    let (x, y, w, h) = area;
    let (left, top) = ((x * scale) as usize, (y * scale) as usize);
    let right = (left + (w * scale) as usize).min(shot.width as usize);
    let bottom = (top + (h * scale) as usize).min(shot.height as usize);
    if right <= left || bottom <= top || full == 0.0 {
        return None;
    }
    let (mut cols, mut rows) = (vec![0.0f32; right - left], vec![0.0f32; bottom - top]);
    for py in top..bottom {
        for px_ in left..right {
            let i = (py * shot.width as usize + px_) * 4;
            let along: f32 = (0..3)
                .map(|k| (f32::from(shot.rgba[i + k]) - f32::from(bg[k])) * towards[k])
                .sum();
            let c = (along / full).clamp(0.0, 1.0);
            cols[px_ - left] = cols[px_ - left].max(c);
            rows[py - top] = rows[py - top].max(c);
        }
    }
    // The near and far edges, in device pixels from the area's start, of the inked run.
    let edges = |cover: &[f32]| -> Option<(f32, f32)> {
        let first = cover.iter().position(|&c| c >= FAINT)?;
        let last = cover.iter().rposition(|&c| c >= FAINT)?;
        Some((first as f32 + 1.0 - cover[first], last as f32 + cover[last]))
    };
    let (x0, x1) = edges(&cols)?;
    let (y0, y1) = edges(&rows)?;
    Some((
        (left as f32 + x0) / scale,
        (left as f32 + x1) / scale,
        (top as f32 + y0) / scale,
        (top as f32 + y1) / scale,
    ))
}

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

/// How far a gap's pixel may be from the canvas: the cards' shadows (`card_shadow_alpha`,
/// blurred 6 pt) reach into the 6 pt gaps and darken them a little (M11i).
const SHADOW_SLACK: u8 = 12;

/// Whether pixel `p` shows the canvas (`sidebar_background`) between cards: within the
/// shadow's reach of it, and nearer it than the card's fill.
fn is_canvas(p: [u8; 3], ui: &UiColors) -> bool {
    let canvas = gap(p, ui.sidebar_background);
    canvas <= SHADOW_SLACK && canvas < gap(p, ui.card)
}

/// The 6 pt gap (M11i) all round the left sidebar's neighbours.
const GAP: f32 = ui_tokens::size::CARD_GAP;

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

/// How far left of its pane's right edge the + button's centre is: past the right sidebar's
/// and ⋯ (each a button and a gap), then half a button.
const PLUS_FROM_EDGE: f32 = 4.0 + 2.0 * (20.0 + 2.0) + 10.0;
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

/// New terminals with no name of yours (M11c Task 3b): **╭∩╮** (`groups::DEFAULT_NAME`), the
/// user's signature, whether in a folder or not (the one shown draws it in its tab and row).
/// Shot in both themes (`default-name-*.png`) to see that the glyphs draw (no missing-glyph
/// boxes); panics unless both are called that, as `Session::label` (the scripting API's name).
fn default_name() {
    use std::cell::RefCell;
    use std::rc::Rc;

    for &(slug, name) in shot_themes() {
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
const FUXX_CLI: &str = "/Users/me/code/fuxx-cli";
const INVITEE: &str = "/Users/me/w/invitee";
const SHOT_HOME: &str = "/Users/me";
/// What devbox's remote shell reported (OSC 7).
const DEVBOX_SRC: &str = "/home/me/src";
/// A shell theme's title (OSC 2): the name of a terminal you haven't named.
const SHELL_TITLE: &str = "me@mac:~/code/fuxx-cli";
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

    for &(slug, name) in shot_themes() {
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
                // A row whose terminal has only the default name is "New Fuxxpace" (2026-11-10).
                let new = crate::groups::NEW_WORKSPACE_NAME;
                let mut want = [AGENT_TITLE, new, SHELL_TITLE, new, "api", new];
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
            size(px(1440.0), px(680.0)), // every row two lines (M11r): all six in view
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

    for &(slug, name) in shot_themes() {
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
    for &(slug, name) in shot_themes() {
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
    for &(slug, name) in shot_themes() {
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
                false, // no repo: `switcher_workspace` has no branch
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
    for &(slug, name) in shot_themes() {
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
    for &(slug, name) in shot_themes() {
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
    for &(slug, name) in shot_themes() {
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
    use std::cell::RefCell;
    use std::rc::Rc;

    let dims = size(px(960.0), px(600.0));
    // Its own name (another run's can't clash), removed at the end.
    let dir = std::env::temp_dir().join(format!("fuxx-ui-shots-palette-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("make the folder");
    let file = dir.join("notes.rs");
    std::fs::write(&file, "fn main() {\n    let found = 1;\n}\n").expect("write the file");
    let is_find =
        |l: &Line| matches!(l, Line::Item { key: Key::Action(a), .. } if a.starts_with("Find"));
    for &(slug, name) in shot_themes() {
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
            // Preferences…: the window opens its panel (M11x: the window handles ⌘, itself).
            open_palette_on(cx, window, &ws, Scope::All);
            cx.simulate_input(window, "preferences");
            settle(cx, window);
            assert_eq!(
                palette_state(cx, &ws).2,
                Some(Key::Action("Preferences\u{2026}")),
                "{n}"
            );
            press_keys(cx, window, "enter");
            let open = cx.read(|cx| ws.read(cx).preferences.is_some());
            assert!(open, "{n}: Preferences… opened the panel");
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
    for &(slug, name) in shot_themes() {
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
    for &(slug, name) in shot_themes() {
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
    for &(slug, name) in shot_themes() {
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
/// Preferences opens on (`LastSection`) and opens the Preferences panel on it (M11x).
fn palette_settings_scene() {
    use crate::palette_items::{Key, Scope, selectable};
    use crate::preferences::Section;
    use crate::sidebar::MARK_SLOT;
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    let dims = size(px(960.0), px(600.0));
    for &(slug, name) in shot_themes() {
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
                // Preferences open over the window (M11x): Enter switches its section.
                if enter {
                    cx.update_window(window, |_, window, cx| {
                        ws.update(cx, |ws, cx| ws.toggle_preferences(window, cx))
                    })
                    .expect("open Preferences");
                    settle(cx, window);
                }
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
                if enter {
                    let before = cx.read(crate::preferences::last_section);
                    assert_ne!(before, Section::Appearance, "{n}: not yet");
                    press_keys(cx, window, "enter");
                    assert!(palette_of(cx, &ws).is_none(), "{n}: Enter closes it");
                    let last = cx.read(crate::preferences::last_section);
                    assert_eq!(last, Section::Appearance, "{n}: LastSection");
                    let shown = cx.read(|cx| {
                        ws.read(cx)
                            .preferences
                            .as_ref()
                            .map(|p| p.read(cx).section())
                    });
                    assert_eq!(
                        shown,
                        Some(Section::Appearance),
                        "{n}: the panel is open on Appearance"
                    );
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
    for &(slug, name) in shot_themes() {
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
    for &(slug, name) in shot_themes() {
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
                prof::sleep(Duration::from_millis(200));
                cx.advance_clock(Duration::from_millis(200));
                // The marks' clock on by 200 ms whether or not its ticker runs.
                let clock = cx.read(|cx| ws.read(cx).clock.clone());
                clock.update(cx, |clock, cx| {
                    clock.phase = status_mark::advance(status_mark::advance(clock.phase));
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

/// The branch switcher's made-up repo (`/Users/me/code/api`) and git (none: a seeded or a
/// faked switcher runs no git).
fn switcher_repo() -> crate::branch_switcher::RepoAt {
    crate::branch_switcher::RepoAt {
        git: PathBuf::from("/nonexistent/git"),
        dir: PathBuf::from(API),
        label: "~/code/api".into(),
    }
}

/// The branch switcher scenes' repo: on main; local main, feature/rows (↑2 ↓1), spike (local
/// only) and feature/old, newest first; origin has main, feature/rows and fix-login; one file is
/// dirty.
fn switcher_snapshot() -> crate::branch_git::Snapshot {
    use crate::branch_list::{Head, LocalRef, RemoteRef, Track};
    let local = |name: &str, upstream: Option<&str>, track: Track, date: i64| LocalRef {
        name: name.into(),
        upstream: upstream.map(String::from),
        track,
        date,
    };
    let remote = |name: &str, date: i64| RemoteRef {
        name: name.into(),
        date,
    };
    crate::branch_git::Snapshot {
        root: PathBuf::from(API),
        head: Head::Branch("main".into()),
        local: vec![
            local("main", Some("origin/main"), Track::Even, 50),
            local(
                "feature/rows",
                Some("origin/feature/rows"),
                Track::Diverged {
                    ahead: 2,
                    behind: 1,
                },
                40,
            ),
            local("spike", None, Track::None, 30),
            local("feature/old", None, Track::None, 20),
        ],
        origin: vec![
            remote("main", 50),
            remote("feature/rows", 40),
            remote("fix-login", 10),
        ],
        pushed: std::collections::HashMap::new(),
        has_origin: true,
        dirty: [PathBuf::from("src/main.rs")].into_iter().collect(),
        orphans: 0,
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
                        Overlay::BranchSwitcher => {
                            let sw = cx.new(|cx| {
                                crate::branch_switcher::BranchSwitcher::seeded(
                                    switcher_repo(),
                                    vec![],
                                    switcher_snapshot(),
                                    Default::default(),
                                    None,
                                    window,
                                    cx,
                                )
                            });
                            ws.show_branch_switcher(sw, window, cx)
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
                        Overlay::Preferences => ws.toggle_preferences(window, cx),
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
    for &(slug, name) in shot_themes() {
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
    for &(slug, name) in shot_themes() {
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

/// Open Preferences over `ws` on `section` (the tips card `dismissed` or not) and draw.
fn open_preferences_on(
    cx: &mut VisualTestAppContext,
    window: AnyWindowHandle,
    ws: &gpui::Entity<Workspace>,
    section: crate::preferences::Section,
    dismissed: bool,
) {
    cx.update_window(window, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            ws.toggle_preferences(window, cx);
            let panel = ws.preferences.clone().expect("Preferences opened");
            panel.update(cx, |panel, cx| {
                panel.shown_as(section, dismissed);
                cx.notify();
            });
        })
    })
    .expect("open Preferences");
    settle(cx, window);
}

/// Preferences as a panel over the window (M11x): each section in both themes
/// (`preferences-<section>-mocha`, `-latte`; General with the tips card dismissed, so its button
/// is live) and `preferences-small-mocha` (a window smaller than the panel). Not shot: the keys.
///
/// Panics unless, outside the panel, the window is dimmed (each pixel the plain window's pulled
/// toward the shadow colour by `SCRIM_ALPHA`), the page's top is the panel's colour, the section
/// list has exactly one filled row, General's switches show the accent (they're on), and
/// Appearance's preview shows the theme's background; small: the panel starts 24 pt in and the
/// page scrolls; keys: Esc closes it, ⌘W closes it and leaves every terminal open, and while it's
/// open the keyboard is the panel's, not a terminal's.
fn preferences_scenes() {
    use crate::preferences::{SCRIM_ALPHA, Section, panel_size};
    use alacritty_terminal::vte::ansi::Rgb;
    use std::cell::RefCell;
    use std::rc::Rc;

    // Wide and tall enough that the sampled points outside the panel are clear of its shadow.
    let (w, h) = (1400.0, 900.0);
    let dims = size(px(w), px(h));
    let (pw, ph) = panel_size(w, h);
    let (x0, y0) = ((w - pw) / 2.0, (h - ph) / 2.0);
    // The section list's width (`preferences::NAV_WIDTH`).
    let nav = 200.0;
    let near = |p: [u8; 3], c: Rgb, by: u8| {
        p.iter()
            .zip([c.r, c.g, c.b])
            .all(|(a, b)| a.abs_diff(b) <= by)
    };
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let ui = theme.ui();
        // The window without the panel: what the scrim dims.
        let plain = {
            let slot: Slot = Rc::new(RefCell::new(None));
            render(&theme, dims, palette_build(theme, slot))
        };
        for section in Section::ALL {
            let scene = format!(
                "preferences-{}-{slug}",
                format!("{section:?}").to_lowercase()
            );
            let slot: Slot = Rc::new(RefCell::new(None));
            let build = palette_build(theme, slot.clone());
            let act = {
                let slot = slot.clone();
                move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                    let ws = slotted(&slot);
                    open_preferences_on(cx, window, &ws, section, true);
                    slot.borrow_mut().take();
                }
            };
            let path = shoot_with(
                "preferences",
                &format!("{}-{slug}", format!("{section:?}").to_lowercase()),
                &theme,
                dims,
                build.clone(),
                act.clone(),
            );
            let shot = render_with(&theme, dims, build, act);
            // Outside the panel: dimmed.
            for (x, y) in [(60.0, h / 2.0), (w - 60.0, h / 2.0), (w / 2.0, 30.0)] {
                let before = pixel_at(&plain, dims, x, y);
                let dim = |a: u8, s: u8| {
                    (f32::from(a) * (1.0 - SCRIM_ALPHA) + f32::from(s) * SCRIM_ALPHA).round() as u8
                };
                let want = Rgb {
                    r: dim(before[0], ui.shadow.r),
                    g: dim(before[1], ui.shadow.g),
                    b: dim(before[2], ui.shadow.b),
                };
                let got = pixel_at(&shot, dims, x, y);
                assert!(
                    near(got, want, 6),
                    "{scene}: ({x}, {y}) isn't dimmed: {got:?}, want {want:?}"
                );
            }
            // The page's top padding is the panel's colour.
            let top = pixel_at(&shot, dims, x0 + nav + 20.0, y0 + 12.0);
            assert!(near(top, ui.panel, 3), "{scene}: the page's top {top:?}");
            // The section list: exactly one filled row (the shown one), right of the names.
            let bands = (0..(ph * 2.0) as usize)
                .map(|i| y0 + i as f32 / 2.0)
                .map(|y| near(pixel_at(&shot, dims, x0 + nav - 24.0, y), ui.row_active, 3))
                .collect::<Vec<_>>()
                .windows(2)
                .filter(|pair| !pair[0] && pair[1])
                .count();
            assert_eq!(bands, 1, "{scene}: {bands} filled rows in the section list");
            // The page's area, sampled every 2 pt.
            let page_has = |c: Rgb, by: u8| {
                let mut y = y0 + 60.0;
                while y < y0 + ph - 20.0 {
                    let mut x = x0 + nav + 20.0;
                    while x < x0 + pw - 20.0 {
                        if near(pixel_at(&shot, dims, x, y), c, by) {
                            return true;
                        }
                        x += 2.0;
                    }
                    y += 2.0;
                }
                false
            };
            if section == Section::General {
                assert!(page_has(ui.accent, 6), "{scene}: no switch is on");
            }
            if section == Section::Appearance {
                assert!(
                    page_has(theme.background, 2),
                    "{scene}: no terminal background in the preview"
                );
            }
            println!("ui-shots: {}", path.display());
        }
    }

    // A window smaller than the panel: 24 pt in, and the page scrolls.
    let theme = shot_theme(shot_themes()[0].1);
    let ui = theme.ui();
    let (sw, sh) = (760.0, 520.0);
    let small = size(px(sw), px(sh));
    let slot: Slot = Rc::new(RefCell::new(None));
    let build = palette_build(theme, slot.clone());
    let act = {
        let slot = slot.clone();
        move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
            let ws = slotted(&slot);
            open_preferences_on(cx, window, &ws, Section::Appearance, true);
            let scroll = cx.read(|cx| {
                ws.read(cx)
                    .preferences
                    .as_ref()
                    .map(|p| p.read(cx).scroll_for_shots())
                    .expect("open")
            });
            assert!(
                f32::from(scroll.max_offset().y) > 0.0,
                "preferences-small: the page doesn't scroll"
            );
            slot.borrow_mut().take();
        }
    };
    let path = shoot_with(
        "preferences",
        "small-mocha",
        &theme,
        small,
        build.clone(),
        act.clone(),
    );
    // The Sound page in the small window: its rows' controls under their words (review fix).
    let slot: Slot = Rc::new(RefCell::new(None));
    let sound_build = palette_build(theme, slot.clone());
    let sound_act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
        let ws = slotted(&slot);
        open_preferences_on(cx, window, &ws, Section::Sound, true);
        slot.borrow_mut().take();
    };
    let sound_path = shoot_with(
        "preferences",
        "small-sound-mocha",
        &theme,
        small,
        sound_build,
        sound_act,
    );
    println!("ui-shots: {}", sound_path.display());
    let shot = render_with(&theme, small, build, act);
    let (spw, _) = panel_size(sw, sh);
    assert_eq!(spw, sw - 48.0, "preferences-small: the panel's width");
    let inside = pixel_at(&shot, small, 24.0 + 4.0, sh / 2.0);
    assert!(
        near(inside, ui.sidebar_background, 3),
        "preferences-small: the section list doesn't start 24 pt in: {inside:?}"
    );
    let outside = pixel_at(&shot, small, 24.0 - 6.0, sh / 2.0);
    assert!(
        !near(outside, ui.sidebar_background, 3),
        "preferences-small: the panel starts left of 24 pt"
    );
    println!("ui-shots: {}", path.display());

    // The keys.
    let slot: Slot = Rc::new(RefCell::new(None));
    let build = palette_build(theme, slot.clone());
    let act = {
        let slot = slot.clone();
        move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
            let ws = slotted(&slot);
            let is_open =
                |cx: &mut VisualTestAppContext| cx.read(|cx| ws.read(cx).preferences.is_some());
            let sessions =
                |cx: &mut VisualTestAppContext| cx.read(|cx| ws.read(cx).sessions.items().len());
            let before = sessions(cx);
            // Esc closes it.
            open_preferences_on(cx, window, &ws, Section::General, true);
            assert!(is_open(cx), "preferences-keys: opened");
            press_keys(cx, window, "escape");
            assert!(!is_open(cx), "preferences-keys: Esc closes it");
            // ⌘, opens and closes it.
            press_keys(cx, window, "cmd-,");
            assert!(is_open(cx), "preferences-keys: ⌘, opens it");
            press_keys(cx, window, "cmd-,");
            assert!(!is_open(cx), "preferences-keys: ⌘, closes it");
            // A click inside the panel (the nav's Appearance row) is the panel's: it stays open and
            // shows Appearance (the scrim behind it gets only clicks outside it).
            open_preferences_on(cx, window, &ws, Section::General, true);
            let (pw, ph) = panel_size(w, h);
            let (x0, y0) = ((w - pw) / 2.0, (h - ph) / 2.0);
            // Appearance's row: under "Settings" (18 + 21 pt) and General's row (32 + 2 pt).
            let row = point(px(x0 + 100.0), px(y0 + 18.0 + 21.0 + 34.0 + 16.0));
            cx.simulate_click(window, row, gpui::Modifiers::none());
            settle(cx, window);
            assert!(is_open(cx), "preferences-keys: a click inside closed it");
            let shown = cx.read(|cx| {
                ws.read(cx)
                    .preferences
                    .as_ref()
                    .map(|p| p.read(cx).section())
            });
            assert_eq!(
                shown,
                Some(Section::Appearance),
                "preferences-keys: the nav's click"
            );
            press_keys(cx, window, "escape");
            // While it's open the keyboard is the panel's; ⌘W closes it, never a terminal.
            open_preferences_on(cx, window, &ws, Section::General, true);
            let focused = cx
                .update_window(window, |_, window, cx| {
                    ws.read(cx).preferences.as_ref().map(|p| {
                        gpui::Focusable::focus_handle(p.read(cx), cx).contains_focused(window, cx)
                    })
                })
                .expect("read focus");
            assert_eq!(
                focused,
                Some(true),
                "preferences-keys: the panel has the keys"
            );
            press_keys(cx, window, "cmd-w");
            assert!(!is_open(cx), "preferences-keys: ⌘W closes it");
            assert_eq!(
                sessions(cx),
                before,
                "preferences-keys: ⌘W closed a terminal"
            );
            // A terminal takes the keyboard while the panel is up (a script, a shortcut that
            // focuses one): the panel goes, and ⌘W still never closes that terminal unseen.
            // (The panel also closes when the keyboard leaves it, but an off-screen window is never
            // active, so GPUI tells it no focus-out: here ⌘W with a terminal focused under it.)
            open_preferences_on(cx, window, &ws, Section::General, true);
            cx.update_window(window, |_, window, cx| {
                ws.update(cx, |ws, cx| ws.focus_active(window, cx))
            })
            .expect("focus a terminal");
            settle(cx, window);
            press_keys(cx, window, "cmd-w");
            assert!(!is_open(cx), "preferences-keys: ⌘W left the panel up");
            assert_eq!(
                sessions(cx),
                before,
                "preferences-keys: ⌘W closed a terminal"
            );
            slot.borrow_mut().take();
        }
    };
    render_with(&theme, dims, build, act);
    println!("ui-shots: preferences-keys ok");
    preferences_plugins_scene();
}

/// `preferences-plugins-installed-<theme>`: the Plugins page with one installed plugin (no
/// network: the records are made up here) and, in one of two renders, a newer version waiting.
///
/// Two helper plugins that run code are listed too, with the second on in one render and off in
/// another.
///
/// Panics unless the page draws, the waiting update changes it (the Update button is there) and
/// so does switching the second helper off (its On/Off switch is there).
fn preferences_plugins_scene() {
    use crate::plugin_source::Installed;
    use crate::plugin_store::Record;
    use crate::plugins::Plugins;
    use crate::preferences::Section;
    use std::cell::RefCell;
    use std::rc::Rc;

    let dims = size(px(1400.0), px(900.0));
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let mut shots = Vec::new();
        // (a newer version waiting, the second helper on)
        for (with_update, second_on) in [(false, true), (true, true), (false, false), (true, false)]
        {
            let slot: Slot = Rc::new(RefCell::new(None));
            let build = palette_build(theme, slot.clone());
            let act = {
                let slot = slot.clone();
                move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                    let ws = slotted(&slot);
                    cx.update(|cx| {
                        let helper = |id: &str, name: &str, enabled: bool| Record {
                            id: id.into(),
                            name: name.into(),
                            version: "0.3.0".into(),
                            // As long as fuxx-completes' own: a description that has to wrap
                            // beside the switch and the buttons (it ran under them, 2026-11-13).
                            description: Some(
                                "Grey ghost-text suggestions from your history as you type (zsh)."
                                    .into(),
                            ),
                            repo: format!("someone/fuxx-{id}"),
                            installed: Installed::Tag("v0.3.0".into()),
                            themes: vec![],
                            installed_at: 0,
                            enabled,
                            bin: Some("bin".into()),
                            zsh: vec!["init.zsh".into()],
                        };
                        let mut plugins = Plugins {
                            records: vec![
                                Record {
                                    id: "sunset".into(),
                                    name: "Sunset".into(),
                                    version: "1.0.0".into(),
                                    description: Some("Warm evening colours.".into()),
                                    repo: "someone/fuxx-sunset".into(),
                                    installed: Installed::Tag("v1.0.0".into()),
                                    themes: vec!["Sunset".into(), "Sunset Light".into()],
                                    installed_at: 0,
                                    enabled: true,
                                    bin: None,
                                    zsh: vec![],
                                },
                                helper("tidy", "Tidy", true),
                                helper("sparkle", "Sparkle", second_on),
                            ],
                            ..Plugins::default()
                        };
                        if with_update {
                            plugins
                                .updates
                                .insert("sunset".into(), Installed::Tag("v1.1.0".into()));
                            // A helper with its switch, an update and Uninstall: the widest card.
                            plugins
                                .updates
                                .insert("tidy".into(), Installed::Tag("v0.3.1".into()));
                        }
                        cx.set_global(plugins);
                    });
                    open_preferences_on(cx, window, &ws, Section::Plugins, true);
                    slot.borrow_mut().take();
                }
            };
            if with_update && !second_on {
                let path = shoot_with(
                    "preferences",
                    &format!("plugins-installed-{slug}"),
                    &theme,
                    dims,
                    build.clone(),
                    act.clone(),
                );
                println!("ui-shots: {}", path.display());
            }
            shots.push(render_with(&theme, dims, build, act));
        }
        assert_ne!(
            shots[0].rgba, shots[1].rgba,
            "preferences-plugins-installed-{slug}: a waiting update doesn't change the page"
        );
        assert_ne!(
            shots[0].rgba, shots[2].rgba,
            "preferences-plugins-installed-{slug}: switching a helper off doesn't change the page"
        );
    }
}

/// The folder the status bar's terminals work in, a deep one, and a 40-character branch.
const FUXX: &str = "/Users/me/code/fuxx-cli";
const DEEP: &str =
    "/Users/me/code/clients/acme-corporation/platform/services/payments/api-gateway";
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
                let ws = cx.new(|cx| {
                    bar_workspace(0, SIDEBAR_DEFAULT_WIDTH, (FUXX, "main"), &[], window, cx)
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

/// Keep `shot` as `<name>-<slug>` (`keep_shot`: its PNG in `target/ui-shots/` with `--png`).
fn save_shot(name: &str, slug: &str, shot: &Shot) -> PathBuf {
    let path = keep_shot(&format!("{name}-{slug}"), shot);
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
    for &(slug, name) in shot_themes() {
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
            // A setting that stops the breath, turned back on at rest (the glow, Animations,
            // Reduce motion off): breathing again at once, solid at first.
            type Toggle = fn(&mut CursorRig, bool);
            let toggles: [(&str, Toggle); 3] = [
                ("the glow", |rig, on| rig.appearance(|a| a.cursor_glow = on)),
                ("Animations", |rig, on| {
                    rig.appearance(|a| a.animations = on)
                }),
                ("Reduce motion off", |rig, on| {
                    rig.cx.update(|cx| cx.set_reduce_motion(!on))
                }),
            ];
            for (label, set) in toggles {
                rest(&mut rig);
                assert_eq!(
                    rig.read(|v| v.cursor_frames()),
                    Frames::None,
                    "{n}: {label}"
                );
                set(&mut rig, false);
                rig.frame();
                set(&mut rig, true);
                let shot = rig.frame();
                assert_eq!(
                    rig.read(|v| v.cursor_frames()),
                    Frames::Breath,
                    "{n}: {label} back on ends the rest"
                );
                assert!(
                    is_colour(&shot, rig.dims, x, mid, own),
                    "{n}: {label} back on, the bar is solid at first, saw {:?}",
                    pixel_at(&shot, rig.dims, x, mid)
                );
            }
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

    /// A file removed when the scene ends, a failed check included.
    struct RemovedOnDrop(std::path::PathBuf);
    impl Drop for RemovedOnDrop {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    let (slug, name) = shot_themes()[0];
    let n = format!("cursor-flood-{slug}");
    // The flood waits for `go` to exist, so it starts only once the scene is watching. The
    // child stays alive after `END`, so no exit repaints the screen either.
    let go = RemovedOnDrop(
        std::env::temp_dir().join(format!("fuxx-ui-shots-{}-flood-go", std::process::id())),
    );
    let _ = std::fs::remove_file(&go.0);
    let program = vec![
        "/bin/sh".into(),
        "-c".into(),
        format!(
            "while [ ! -e '{}' ]; do sleep 0.01; done; seq 1 300000; echo END; exec sleep 60",
            go.0.display()
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
    std::fs::write(&go.0, b"").expect("start the flood");
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
        prof::sleep(Duration::from_millis(5));
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
    assert!(
        screen.lines().any(|l| l.trim_end() == "END"),
        "{n}: once the output stops, the last frame drawn (of {frames}) holds END, saw {screen:?}"
    );
}

/// The view's chip, read for the checks: its label, buttons and painted bounds.
fn chip_of(rig: &CursorRig) -> Option<(String, [&'static str; 3], gpui::Bounds<Pixels>)> {
    rig.read(|v| v.chip_shot())
}

/// Draw frames until `done` holds (at most 3 s: the program's echo comes from its own thread).
fn wait_for(rig: &mut CursorRig, done: impl Fn(&crate::terminal_view::TerminalView) -> bool) {
    let start = std::time::Instant::now();
    while !rig.read(&done) && start.elapsed() < std::time::Duration::from_secs(3) {
        prof::sleep(std::time::Duration::from_millis(10));
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
///   empty prompt. No hairline above any prompt (removed 2026-11-10). Hovering block 2's gutter widens its bar to 3 pt and opens the chip (`exit 1 ·
///   false`, the three buttons) 4 pt right of the gutter, one row tall on the command row (the
///   pixel rows just above and below it untouched); the chip stays while the pointer is on it
///   (200 ms on), and goes 150 ms after it leaves both.
///   Nothing moves: two frames 200 ms apart with the chip open are the same pixels, with
///   motion on and with Reduce motion on.
fn command_blocks_scene() {
    use crate::block_view::{CHIP_GAP, CHIP_HIDE_MS};
    use std::time::Duration;
    let dims = size(px(480.0), px(240.0));
    for &(slug, name) in shot_themes() {
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
                // No hairline anywhere (removed 2026-11-10, your call): above a block's first
                // prompt row, the live prompt, the first line of scrollback or an empty prompt.
                for row in [0, 2, 3, 5, 8] {
                    assert!(
                        at(&shot, g.sep_x, g.top(row) + 0.5, bg),
                        "{n}: no hairline above row {row}: {:?}",
                        seen(&shot, g.sep_x, g.top(row) + 0.5)
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
            prof::sleep(Duration::from_millis(200));
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
    for &(slug, name) in shot_themes() {
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
    }
}

/// `command-blocks-narrow`: a split's pane with a 200 pt grid: the bars 6 pt left of the grid,
/// the chip's buttons short (`Cmd`, `Output`, `Rerun`), the chip one row tall and inside the
/// pane.
fn command_blocks_narrow_scene() {
    // 12 pt of split padding each side: a 200 pt grid area.
    let dims = size(px(224.0), px(240.0));
    for &(slug, name) in shot_themes() {
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
    for &(slug, name) in shot_themes() {
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
    for &(slug, name) in shot_themes() {
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
    for &(slug, name) in shot_themes() {
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
        prof::sleep(std::time::Duration::from_millis(10));
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
    for &(slug, name) in shot_themes() {
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
    for &(slug, name) in shot_themes() {
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
        prof::sleep(std::time::Duration::from_millis(100));
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
    for &(slug, name) in shot_themes() {
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
        prof::sleep(std::time::Duration::from_millis(200));
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
        prof::sleep(std::time::Duration::from_millis(200));
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
        prof::sleep(std::time::Duration::from_millis(50));
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
