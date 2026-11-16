//! The shared screenshot harness: rendering and shot primitives, the shot appearance, generic
//! pixel helpers, frame helpers, and `Steps` (the chrome scenes' checked steps). Scenes use them
//! through `ui_shots`, which re-exports this module.

use std::cell::RefCell;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use gpui::{
    AnyView, AnyWindowHandle, App, Context, Entity, Font, IntoElement, Pixels, Render, Size,
    VisualTestAppContext, Window, div, prelude::*,
};
use gpui_platform::current_platform;

use crate::appearance::Appearance;
use crate::chrome_cache::{self, Counted};
use crate::restore::RestoreMode;
use crate::status::Status;
use crate::terminal_view::hsla;
use crate::theme::{self, Theme};
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
#[derive(Clone)]
pub struct Shot {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// The window's root: the theme's background with the scene on top.
pub(super) struct Root {
    pub(super) theme: Theme,
    pub(super) scene: AnyView,
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
    let started = Instant::now();
    let mut cx = gpui::VisualTestAppContext::new(current_platform(false));
    let mut t = prof::lap(prof::NEW_APP, started);
    // As the app does at start (appearance.rs), so the bundled fonts are there to shape text with.
    fonts::register_bundled_fonts(&cx.text_system().all_font_names());
    t = prof::lap(prof::FONTS, t);
    let theme = *theme;
    let window = cx
        .open_offscreen_window(size, move |window, cx| {
            let scene = build(window, cx);
            cx.new(|_| Root { theme, scene })
        })
        .expect("open an off-screen window");
    t = prof::lap(prof::OPEN, t);
    cx.run_until_parked();
    t = prof::lap(prof::SETTLE, t);
    act(&mut cx, window.into());
    cx.run_until_parked();
    t = prof::lap(prof::ACT, t);
    let image = cx
        .capture_screenshot(window.into())
        .expect("capture the window");
    t = prof::lap(prof::CAPTURE, t);
    let shot = Shot {
        width: image.width(),
        height: image.height(),
        rgba: image.as_raw().to_vec(),
    };
    prof::add(prof::RENDER_WITH, started.elapsed());
    drop(cx);
    prof::lap(prof::TEARDOWN, t);
    shot
}

/// Render, keep the shot for the scene (`keep_shot`) as `<name>-<slug>`; returns its PNG's path
/// (written only with `--png`).
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
    keep_owned(format!("{name}-{slug}"), shot)
}

thread_local! {
    /// The running scene's shots, by file stem: written to `target/ui-shots/failed/<scene>/` if
    /// it fails. Cleared as each scene starts (`forget_shots`).
    static KEPT: RefCell<Vec<(String, Shot)>> = const { RefCell::new(Vec::new()) };
}

/// `target/ui-shots/`, where the PNGs go.
pub(super) fn shots_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("ui-shots")
}

/// Keep a copy of `shot` for the running scene as `stem` (its failure picture); with `--png`
/// (or `UI_SHOTS_PNG=1`) also write `target/ui-shots/<stem>.png`. Returns that path.
pub(super) fn keep_shot(stem: &str, shot: &Shot) -> PathBuf {
    keep_owned(stem.to_string(), shot.clone())
}

fn keep_owned(stem: String, shot: Shot) -> PathBuf {
    let path = shots_dir().join(format!("{stem}.png"));
    if super::options().png {
        std::fs::create_dir_all(shots_dir()).expect("create target/ui-shots");
        let started = Instant::now();
        std::fs::write(&path, png(&shot)).expect("write the PNG");
        prof::lap(prof::PNG, started);
    }
    KEPT.with_borrow_mut(|kept| kept.push((stem, shot)));
    path
}

/// The PNG bytes of the running scene's latest kept shot.
pub(super) fn last_png() -> Vec<u8> {
    KEPT.with_borrow(|kept| png(&kept.last().expect("a kept shot").1))
}

/// A scene starts: drop the last one's shots and its old failure pictures.
pub(super) fn forget_shots(scene: &str) {
    KEPT.with_borrow_mut(Vec::clear);
    let _ = std::fs::remove_dir_all(failed_dir(scene));
}

/// `target/ui-shots/failed/<scene>/`.
pub(super) fn failed_dir(scene: &str) -> PathBuf {
    shots_dir().join("failed").join(scene)
}

/// A scene failed: write its kept shots to `failed_dir(scene)` (as `<stem>.png`); returns it.
pub(super) fn write_failed(scene: &str) -> PathBuf {
    let dir = failed_dir(scene);
    std::fs::create_dir_all(&dir).expect("create target/ui-shots/failed/<scene>");
    KEPT.with_borrow(|kept| {
        for (stem, shot) in kept {
            std::fs::write(dir.join(format!("{stem}.png")), png(shot)).expect("write the PNG");
        }
    });
    dir
}

/// Timing for `UI_SHOTS_PROFILE=1` (per-phase sums of `render_with`/`shoot_with`) and for the
/// run's summary (time in `thread::sleep` and in child processes). Always accumulated (cheap);
/// printed only when profiling.
pub(crate) mod prof {
    use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
    use std::time::{Duration, Instant};

    pub const NEW_APP: usize = 0;
    pub const FONTS: usize = 1;
    pub const OPEN: usize = 2;
    pub const SETTLE: usize = 3;
    pub const ACT: usize = 4;
    pub const CAPTURE: usize = 5;
    pub const PNG: usize = 6;
    /// Whole `render_with` calls (the phases above except `PNG`).
    pub const RENDER_WITH: usize = 7;
    pub const SLEEP: usize = 8;
    pub const CHILD: usize = 9;
    /// Dropping the app and its window at the end of `render_with` (not in `RENDER_WITH`).
    pub const TEARDOWN: usize = 10;
    const NAMES: [&str; 7] = [
        "new_app", "fonts", "open", "settle", "act", "capture", "png",
    ];
    static NANOS: [AtomicU64; 11] = [const { AtomicU64::new(0) }; 11];

    pub fn enabled() -> bool {
        static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        *ON.get_or_init(|| std::env::var_os("UI_SHOTS_PROFILE").is_some())
    }
    pub fn add(slot: usize, d: Duration) {
        NANOS[slot].fetch_add(d.as_nanos() as u64, Relaxed);
    }
    pub fn get(slot: usize) -> f64 {
        NANOS[slot].load(Relaxed) as f64 / 1e9
    }
    /// Adds the time since `since` to `slot`; returns now, for the next phase.
    pub fn lap(slot: usize, since: Instant) -> Instant {
        let now = Instant::now();
        add(slot, now - since);
        now
    }
    /// `std::thread::sleep`, counted.
    pub fn sleep(d: Duration) {
        add(SLEEP, d);
        std::thread::sleep(d);
    }
    pub fn print_sums() {
        let total: f64 = (0..NAMES.len()).map(get).sum();
        let sums: Vec<String> = NAMES
            .iter()
            .enumerate()
            .map(|(i, n)| format!("{n} {:.1}", get(i)))
            .collect();
        println!(
            "ui-shots: profile (s): {} | sum {total:.1}",
            sums.join(", ")
        );
        println!(
            "ui-shots: profile (s): render_with total {:.1}, teardown (after) {:.1}, thread::sleep {:.1}, child processes {:.1}",
            get(RENDER_WITH),
            get(TEARDOWN),
            get(SLEEP),
            get(CHILD)
        );
    }
}

/// A minimal PNG: 8-bit RGBA, no filtering, stored (uncompressed) deflate blocks. The `image`
/// crate that GPUI uses isn't a direct dependency, and screenshots don't need to be small.
pub(super) fn png(shot: &Shot) -> Vec<u8> {
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

pub(super) fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let start = out.len();
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let crc = crc32(&out[start..]);
    out.extend_from_slice(&crc.to_be_bytes());
}

pub(super) fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &byte in data {
        a = (a + byte as u32) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

pub(super) fn crc32(data: &[u8]) -> u32 {
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

/// The appearance a scene's app runs with: `theme`, and the default font and size.
pub(super) fn shot_appearance(theme: Theme, cx: &App) -> Appearance {
    Appearance {
        theme,
        font: mono_font(cx),
        font_size: fonts::DEFAULT_SIZE,
        theme_name: None,
        font_settings: fonts::from_entries(&[]),
        settings_path: None,
        user_theme_dir: None,
        plugin_folders: None,
        plugin_themes: Vec::new(),
        presets: Vec::new(),
        restore: RestoreMode::default(),
        scripting: false,
        persistent: false,
        animations: true,
        browser_address_bar: true,
        status_bar: true,
        browser_keep_data: false,
        check_updates: true,
        cursor_style: Default::default(),
        cursor_glide: true,
        cursor_glow: true,
        cursor_follows_status: true,
        cursor_fun: Default::default(),
        command_blocks: true,
        file_wrap: false,
        sound_links: Default::default(),
        sound_on: [true; crate::sounds::SoundKey::ALL.len()],
    }
}

/// How far pixel `p` is from colour `c`: its largest difference in any channel.
pub(super) fn gap(p: [u8; 3], c: alacritty_terminal::vte::ansi::Rgb) -> u8 {
    p.iter()
        .zip([c.r, c.g, c.b])
        .map(|(a, b)| a.abs_diff(b))
        .max()
        .unwrap_or(0)
}

/// Whether pixel `p` is colour `c` (give or take 3 per channel).
pub(super) fn near(p: [u8; 3], c: alacritty_terminal::vte::ansi::Rgb) -> bool {
    gap(p, c) <= 3
}

/// The window pixel at (`x`, `y`) points.
pub(super) fn pixel_at(shot: &Shot, dims: Size<Pixels>, x: f32, y: f32) -> [u8; 3] {
    let scale = shot.width as f32 / f32::from(dims.width);
    let (x, y) = ((x * scale) as usize, (y * scale) as usize);
    let i = (y * shot.width as usize + x) * 4;
    [shot.rgba[i], shot.rgba[i + 1], shot.rgba[i + 2]]
}

/// One explicit draw of `window` (what `cached_panes` does per frame).
pub(super) fn draw(cx: &mut VisualTestAppContext, window: AnyWindowHandle) {
    cx.update_window(window, |_, window, cx| window.draw(cx).clear())
        .expect("draw");
}

/// Frames as the app gets them, and nothing else: the effects settle (timers due by the
/// harness's clock run, and a notify draws), then each frame something asked for. No explicit
/// draw (one would run `Workspace::render`, which restarts timers and hides a missing one).
pub(super) fn settle_frames(cx: &mut VisualTestAppContext, window: AnyWindowHandle) {
    cx.run_until_parked();
    for _ in 0..100 {
        if !next_frame(cx, window) {
            break;
        }
        cx.run_until_parked();
    }
}

/// The platform's next frame, as its display link would deliver it: what was asked for the
/// next frame (`request_animation_frame`, `on_next_frame`) runs, and the window draws if that
/// notified something (as the app's would: no explicit draw, which would redraw the workspace
/// whatever was asked). Whether anything was asked.
pub(super) fn next_frame(cx: &mut VisualTestAppContext, window: AnyWindowHandle) -> bool {
    cx.update_window(window, |_, window, cx| window.simulate_next_frame(cx) > 0)
        .expect("the next frame")
}

/// The window as it is now (a draw, then its pixels).
pub(super) fn frame_now(cx: &mut VisualTestAppContext, window: AnyWindowHandle) -> Shot {
    draw(cx, window);
    let image = cx.capture_screenshot(window).expect("capture the window");
    Shot {
        width: image.width(),
        height: image.height(),
        rgba: image.into_raw(),
    }
}

/// The window drawn afresh: a refresh, which draws every cached part again.
pub(super) fn frame_fresh(cx: &mut VisualTestAppContext, window: AnyWindowHandle) -> Shot {
    cx.update_window(window, |_, window, _| window.refresh())
        .expect("refresh");
    frame_now(cx, window)
}

/// How many bytes of two shots of one window differ (all of them if the sizes do).
pub(super) fn bytes_differ(a: &Shot, b: &Shot) -> usize {
    if (a.width, a.height) != (b.width, b.height) {
        return a.rgba.len().max(b.rgba.len());
    }
    a.rgba.iter().zip(&b.rgba).filter(|(x, y)| x != y).count()
}

/// The chrome's cached parts (M11p Task 4), as counted.
pub(super) const PARTS: [Counted; 4] = [
    Counted::Sidebar,
    Counted::StatusBar,
    Counted::TabBar,
    Counted::Title,
];

/// How late the app may draw an age label that turned, by design: the marks' next tick while
/// they move (or the age's timer, 20 ms past the turn), and the harness's next catch-up after
/// it (`Steps::catch_up`).
pub(super) const LABEL_LATE: Duration = Duration::from_millis(250);

/// What a step's source should do to the screen.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Seen {
    /// Something shown changes (the step is void if nothing does).
    Changes,
    /// Maybe nothing shown changes (the source is still followed by a redraw that must be
    /// current).
    Maybe,
}

/// A scene's steps, each checked as it runs (`step`), and the stale ones (kept going).
pub(super) struct Steps {
    n: &'static str,
    count: usize,
    stale: Vec<String>,
    /// When the harness's clock last caught up with the real one (`catch_up`).
    synced: Instant,
    /// The scene's workspace (for the ages it shows).
    ws: Entity<Workspace>,
}

impl Steps {
    /// A scene's steps over `window`, once what its setup started has settled and the window
    /// is drawn afresh (so the first step's frames are its own).
    pub(super) fn new(
        n: &'static str,
        cx: &mut VisualTestAppContext,
        window: AnyWindowHandle,
        ws: &Entity<Workspace>,
    ) -> Self {
        settle_frames(cx, window);
        frame_fresh(cx, window);
        settle_frames(cx, window);
        Self {
            n,
            count: 0,
            stale: Vec::new(),
            synced: Instant::now(),
            ws: ws.clone(),
        }
    }

    /// The app's timers run on the harness's clock, the labels they turn (ages) read the real
    /// one: move the harness's clock on by the real time since it last caught up, so what came
    /// due meanwhile runs as it would in the app, then its frames.
    fn catch_up(&mut self, cx: &mut VisualTestAppContext, window: AnyWindowHandle) {
        cx.advance_clock(self.synced.elapsed());
        self.synced = Instant::now();
        settle_frames(cx, window);
    }

    /// Before a step that stops what turns age labels (the window hidden: by design nothing is
    /// redrawn then, and showing it again redraws): if a label turns within `within`, wait
    /// until it has and is drawn, so it doesn't turn unseen during the step.
    pub(super) fn clear_of_age_turns(
        &mut self,
        cx: &mut VisualTestAppContext,
        window: AnyWindowHandle,
        within: Duration,
    ) {
        let soonest = cx.read(|cx| {
            let now = Instant::now();
            let ws = self.ws.read(cx);
            ws.sessions
                .items()
                .iter()
                .filter(|s| s.status != Status::Idle)
                .map(|s| {
                    crate::sidebar::next_age_change(now.saturating_duration_since(s.status_since))
                })
                .min()
        });
        if let Some(wait) = soonest.filter(|w| *w < within) {
            crate::ui_shots::prof::sleep(wait + LABEL_LATE);
            self.catch_up(cx, window);
        }
    }

    /// Each terminal's age (not idle), now: what a stale step reports, to tell an age label
    /// caught turning from a part not drawn.
    fn ages(&self, cx: &VisualTestAppContext) -> Vec<(String, Duration)> {
        cx.read(|cx| {
            let ws = self.ws.read(cx);
            let ticking = ws.marks_ticking;
            ws.sessions
                .items()
                .iter()
                .filter(|s| s.status != Status::Idle)
                .map(|s| {
                    (
                        format!("{} (ticking {ticking})", s.label()),
                        s.status_since.elapsed(),
                    )
                })
                .collect()
        })
    }

    /// Each terminal's age label (not idle), now.
    fn age_labels(&self, cx: &VisualTestAppContext) -> Vec<String> {
        cx.read(|cx| {
            let now = Instant::now();
            self.ws
                .read(cx)
                .sessions
                .items()
                .iter()
                .filter(|s| s.status != Status::Idle)
                .map(|s| crate::status::age_label(now.saturating_duration_since(s.status_since)))
                .collect()
        })
    }

    /// After a step: if an age label (by the real clock) turned just now, within what the app
    /// takes by design to draw a turned label (its timer, 20 ms past the turn; or the marks'
    /// next tick, while they move), wait that out and let it draw (`catch_up`), so the check
    /// isn't about a label caught mid-turn (`chrome-ages-current` is about those).
    fn past_age_turns(&mut self, cx: &mut VisualTestAppContext, window: AnyWindowHandle) {
        let late = LABEL_LATE;
        let turned = cx.read(|cx| {
            let now = Instant::now();
            self.ws
                .read(cx)
                .sessions
                .items()
                .iter()
                .filter(|s| s.status != Status::Idle)
                .any(|s| {
                    let age = now.saturating_duration_since(s.status_since);
                    age >= late && crate::sidebar::next_age_change(age - late) <= late
                })
        });
        if turned {
            crate::ui_shots::prof::sleep(late);
            self.catch_up(cx, window);
        }
    }

    /// One source of change: `act` changes it as the app does; then frames as the app gets them
    /// (`settle_frames`, and the timers that came due meanwhile, `catch_up`); the window as
    /// shown must be a full redraw's and (`Seen::Changes`) not what it showed before. Panics
    /// (or, kept going, records) when a part is stale.
    pub(super) fn step(
        &mut self,
        cx: &mut VisualTestAppContext,
        window: AnyWindowHandle,
        what: &str,
        seen: Seen,
        act: impl FnOnce(&mut VisualTestAppContext),
    ) {
        self.count += 1;
        let n = self.n;
        self.catch_up(cx, window);
        let before = shown(cx, window);
        chrome_cache::reset();
        act(cx);
        settle_frames(cx, window);
        let parts = PARTS.map(|c| (c, chrome_cache::get(c)));
        self.catch_up(cx, window);
        self.past_age_turns(cx, window);
        let ages = self.ages(cx);
        // An age label turning between the two captures would show as a stale part (or hide
        // one): read the labels around them; if one turned, wait the turn out and capture once
        // more, and fail as a turn (not as stale) if one turns again.
        let mut retried = false;
        let (now, fresh) = loop {
            let labels = self.age_labels(cx);
            let now = shown(cx, window);
            let fresh = frame_fresh(cx, window);
            if labels == self.age_labels(cx) {
                break (now, fresh);
            }
            assert!(
                !retried,
                "{n}: {what}: an age turned during the check (twice)"
            );
            retried = true;
            crate::ui_shots::prof::sleep(LABEL_LATE);
            self.catch_up(cx, window);
        };
        let stale = bytes_differ(&now, &fresh);
        let changed = bytes_differ(&before, &fresh);
        println!("ui-shots: {n}: {what}: parts drawn {parts:?}; {changed} bytes changed");
        if seen == Seen::Changes {
            assert!(
                changed > 0,
                "{n}: {what}: nothing shown changed (the step tests nothing)"
            );
        }
        if stale > 0 {
            let k = self.count;
            keep_shot(&format!("{n}-{k}-stale-now"), &now);
            keep_shot(&format!("{n}-{k}-stale-fresh"), &fresh);
            let what = format!(
                "{n}: {what}: {stale} bytes differ from a full redraw (stale); ages then {ages:?}"
            );
            if !keep_going() {
                panic!("{what}");
            }
            println!("ui-shots: STALE {what}");
            self.stale.push(what);
        }
    }

    /// Panics if a step was stale (kept going).
    pub(super) fn done(self) {
        let n = self.n;
        assert!(
            self.stale.is_empty(),
            "{n}: {} of {} steps stale: {:#?}",
            self.stale.len(),
            self.count,
            self.stale
        );
        println!("ui-shots: {n}: {} steps, each current", self.count);
    }
}

/// Whether a stale step lets the scene go on (`FUXX_SHOTS_KEEP_GOING=1`).
pub(super) fn keep_going() -> bool {
    std::env::var("FUXX_SHOTS_KEEP_GOING").is_ok_and(|v| v == "1")
}

/// The window as it was last drawn (no draw now).
pub(super) fn shown(cx: &mut VisualTestAppContext, window: AnyWindowHandle) -> Shot {
    let image = cx.capture_screenshot(window).expect("capture the window");
    Shot {
        width: image.width(),
        height: image.height(),
        rgba: image.into_raw(),
    }
}
