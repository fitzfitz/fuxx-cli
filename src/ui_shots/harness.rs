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
use crate::theme::{self, Theme, UiColors};
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
    } else {
        // No picture from an earlier `--png` run behind the path the scene may print.
        let _ = std::fs::remove_file(&path);
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
/// A disk error is reported and the run goes on.
pub(super) fn write_failed(scene: &str) -> PathBuf {
    let dir = failed_dir(scene);
    if let Err(e) = std::fs::create_dir_all(&dir) {
        eprintln!("ui-shots: {scene}: can't create {}: {e}", dir.display());
        return dir;
    }
    KEPT.with_borrow(|kept| {
        for (stem, shot) in kept {
            let path = dir.join(format!("{stem}.png"));
            if let Err(e) = std::fs::write(&path, png(shot)) {
                eprintln!("ui-shots: {scene}: can't write {}: {e}", path.display());
            }
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

/// The box, `(x0, x1, y0, y1)` in points, round every device pixel inside `area` (`x`, `y`,
/// `w`, `h` in points) that is drawn in ink: at least half-way from `bg` to `ink` (its squared
/// distance from `bg`, times 4, at least `ink`'s); `None` when there is none. Every device pixel
/// is read, so a box edge is the same at 1x and 2x (a point-by-point read rounds each side its
/// own way, by up to a pixel).
pub(super) fn ink_box(
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
pub(super) fn ink_span(
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

pub(super) fn count_along(
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
pub(super) fn is_canvas(p: [u8; 3], ui: &UiColors) -> bool {
    let canvas = gap(p, ui.sidebar_background);
    canvas <= SHADOW_SLACK && canvas < gap(p, ui.card)
}

/// Frames, so focus changes (and the boxes' blur events) and scrolls run.
pub(super) fn settle(cx: &mut VisualTestAppContext, window: AnyWindowHandle) {
    for _ in 0..3 {
        cx.update_window(window, |_, window, cx| window.draw(cx).clear())
            .expect("draw");
        cx.run_until_parked();
    }
}

/// Press `keys` (space-separated keystrokes), a few frames after each.
pub(super) fn press_keys(cx: &mut VisualTestAppContext, window: AnyWindowHandle, keys: &str) {
    for key in keys.split(' ') {
        cx.simulate_keystrokes(window, key);
        settle(cx, window);
    }
}

/// The most inked pixel (the furthest from `bg`) in the row's middle band between `x0` and `x1`.
pub(super) fn strongest(
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

/// Whether pixel `p` is colour `c`, give or take `tol` a channel.
pub(super) fn near_by(p: [u8; 3], c: alacritty_terminal::vte::ansi::Rgb, tol: u8) -> bool {
    p.iter()
        .zip([c.r, c.g, c.b])
        .all(|(a, b)| a.abs_diff(b) <= tol)
}

/// Keep `shot` as `<name>-<slug>` (`keep_shot`: its PNG in `target/ui-shots/` with `--png`).
pub(super) fn save_shot(name: &str, slug: &str, shot: &Shot) -> PathBuf {
    let path = keep_shot(&format!("{name}-{slug}"), shot);
    println!("ui-shots: {}", path.display());
    path
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{px, size};

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
