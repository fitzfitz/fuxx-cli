//! The terminal cursor's rules (M11f, spec note 58): which shape to draw, its colour, the
//! breathing glow, the glide between cells, the trail behind a glide and the sparks off a
//! keystroke. Pure: every function takes its `Instant`s and `Duration`s as arguments and never
//! reads the clock, so each rule is unit-tested. Drawing lives in the terminal view.

use std::f32::consts::TAU;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use alacritty_terminal::vte::ansi::{CursorShape, CursorStyle, Rgb};

use crate::appearance::CursorShapeSetting;
use crate::status::Status;
use crate::theme::{Theme, UiColors};

/// What the view paints. Not alacritty's `CursorShape`: the user's "Hollow" shape must not
/// collide with `CursorShape::HollowBlock`, which fuxx uses as the "no program has chosen"
/// marker (`CURSOR_UNSET`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaintShape {
    Bar,
    Underline,
    /// A 1 px outline of the cell.
    Hollow,
    Block,
    Hidden,
}

/// A shape to paint and whether it breathes (the blinking flag of the program's choice, or
/// true for the user's setting; the Breathing glow setting still has the last word).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PaintStyle {
    pub shape: PaintShape,
    pub blinking: bool,
}

/// The style to paint. Any `HollowBlock` means no program chose a shape (programs cannot ask
/// for one; curses' `ESC [ ? 12 l` may clear its blinking flag but leaves the shape), so the
/// setting's shape is drawn, blinking. A real choice (`ESC [ n q`) is kept, flag included.
pub fn effective_style(term_style: CursorStyle, setting: CursorShapeSetting) -> PaintStyle {
    let shape = match term_style.shape {
        CursorShape::HollowBlock => {
            return PaintStyle {
                shape: match setting {
                    CursorShapeSetting::Bar => PaintShape::Bar,
                    CursorShapeSetting::Underline => PaintShape::Underline,
                    CursorShapeSetting::Hollow => PaintShape::Hollow,
                    CursorShapeSetting::Block => PaintShape::Block,
                },
                blinking: true,
            };
        }
        CursorShape::Beam => PaintShape::Bar,
        CursorShape::Underline => PaintShape::Underline,
        CursorShape::Block => PaintShape::Block,
        CursorShape::Hidden => PaintShape::Hidden,
    };
    PaintStyle {
        shape,
        blinking: term_style.blinking,
    }
}

/// The cursor's colour: the pane's status colour when it follows, else the theme's cursor.
pub fn cursor_color(status: Status, follows: bool, ui: &UiColors, theme: &Theme) -> Rgb {
    let own = theme.cursor.unwrap_or(theme.foreground);
    if !follows {
        return own;
    }
    match status {
        Status::Working => ui.ring_working,
        Status::Waiting => ui.ring_waiting,
        Status::Done => ui.ring_done,
        Status::Idle => own,
    }
}

/// How long the breath holds at full strength after the cursor changes.
pub const HOLD: Duration = Duration::from_millis(600);
const BREATH_PERIOD: f32 = 1.6;
const BREATH_LOW: f32 = 0.35;

/// The breathing opacity: 1.0 while the cursor changed less than 0.6 s ago, then a cosine
/// between 1.0 and 0.35 with a 1.6 s period. `clock` is the time since the breath began, so
/// 0 is full, 0.8 s is the trough and 1.6 s is full again. The caller passes `clock` measured
/// from the end of the hold (`since_change - 600 ms`) so the breath starts smoothly at full.
/// 1.0 again once the breath rests (`breath_rests`).
pub fn breath_alpha(since_change: Duration, clock: Duration) -> f32 {
    if since_change < HOLD || breath_rests(clock) {
        return 1.0;
    }
    // Whole nanoseconds into the breath: exact however long the clock has run.
    let phase = breath_phase(clock) as f32 / BREATH_PERIOD;
    let mid = (1.0 + BREATH_LOW) / 2.0;
    mid + (1.0 - mid) * (TAU * phase).cos()
}

const GLIDE: Duration = Duration::from_millis(90);
const TRAIL_TAIL: Duration = Duration::from_millis(150);
const TRAIL_COPIES: usize = 5;
const MAX_GLIDE_ROWS: usize = 20;

/// A glide of the drawn cursor to a new cell. Positions are `(column, row)` in cells.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Glide {
    pub from: (f32, f32),
    pub to: (usize, usize),
    pub start: Instant,
}

fn ease_out_cubic(t: f32) -> f32 {
    1.0 - (1.0 - t.clamp(0.0, 1.0)).powi(3)
}

impl Glide {
    /// Where the cursor is drawn at `now`: 90 ms, cubic ease-out, from `from` to `to`.
    pub fn position(&self, now: Instant) -> (f32, f32) {
        let t = now.saturating_duration_since(self.start).as_secs_f32() / GLIDE.as_secs_f32();
        let e = ease_out_cubic(t);
        (
            self.from.0 + (self.to.0 as f32 - self.from.0) * e,
            self.from.1 + (self.to.1 as f32 - self.from.1) * e,
        )
    }

    fn length(&self) -> f32 {
        let dx = self.to.0 as f32 - self.from.0;
        let dy = self.to.1 as f32 - self.from.1;
        (dx * dx + dy * dy).sqrt()
    }
}

/// The glide to a new cell, from where the cursor is drawn now (so a retarget mid-glide does
/// not jump). `None` means jump: more than 20 rows, a screen switch, or nowhere to go.
/// A glide already heading to `to` is kept as it is.
pub fn glide_from_now(
    current: Option<&Glide>,
    drawn: (f32, f32),
    to: (usize, usize),
    rows_moved: usize,
    screen_changed: bool,
    now: Instant,
) -> Option<Glide> {
    if screen_changed || rows_moved > MAX_GLIDE_ROWS {
        return None;
    }
    if let Some(g) = current.filter(|g| g.to == to) {
        return Some(*g);
    }
    if drawn == (to.0 as f32, to.1 as f32) {
        return None;
    }
    Some(Glide {
        from: drawn,
        to,
        start: now,
    })
}

/// The trail: 5 copies lagging behind the head along the path, each smaller and fainter,
/// as `(column, row, scale, alpha)`. They fade out over 150 ms once the glide ends, then the
/// list is empty. Empty too for a glide of 1 cell or less.
pub fn trail(glide: &Glide, now: Instant) -> Vec<(f32, f32, f32, f32)> {
    if glide.length() <= 1.0 {
        return Vec::new();
    }
    let elapsed = now.saturating_duration_since(glide.start);
    if elapsed >= GLIDE + TRAIL_TAIL {
        return Vec::new();
    }
    let fade = 1.0 - elapsed.saturating_sub(GLIDE).as_secs_f32() / TRAIL_TAIL.as_secs_f32();
    (1..=TRAIL_COPIES)
        .map(|i| {
            let lag = Duration::from_millis(12 * i as u64);
            let at = glide.start + elapsed.saturating_sub(lag);
            let (x, y) = glide.position(at);
            let k = i as f32 / (TRAIL_COPIES as f32 + 1.0);
            (x, y, 1.0 - 0.5 * k, (1.0 - k) * fade)
        })
        .collect()
}

const SPARK_LIFE: Duration = Duration::from_millis(350);
const SPARKS: usize = 6;

/// One spark: a dot flying out from `origin` (pixels) at `angle` radians for `reach` pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spark {
    pub origin: (f32, f32),
    pub angle: f32,
    pub reach: f32,
    pub start: Instant,
}

/// splitmix64: a small integer hash, so sparks are deterministic from a seed without `rand`.
fn mix(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Six sparks for a keystroke: angles spread round the circle (each nudged a little), reach
/// 6 to 14 px. The same seed gives the same sparks.
pub fn sparks_for_key(seed: u64, origin: (f32, f32), now: Instant) -> [Spark; SPARKS] {
    std::array::from_fn(|i| {
        let h = mix(seed ^ mix(i as u64 + 1));
        let jitter = ((h & 0xFFFF) as f32 / 65535.0 - 0.5) * 0.4;
        let reach = 6.0 + ((h >> 16) & 0xFFFF) as f32 / 65535.0 * 8.0;
        Spark {
            origin,
            angle: TAU * i as f32 / SPARKS as f32 + jitter,
            reach,
            start: now,
        }
    })
}

/// A spark's position (pixels) and alpha at `now`; `None` once its 350 ms are over.
pub fn spark_at(s: &Spark, now: Instant) -> Option<((f32, f32), f32)> {
    let elapsed = now.saturating_duration_since(s.start);
    if elapsed >= SPARK_LIFE {
        return None;
    }
    let t = elapsed.as_secs_f32() / SPARK_LIFE.as_secs_f32();
    let d = s.reach * ease_out_cubic(t);
    Some((
        (
            s.origin.0 + s.angle.cos() * d,
            s.origin.1 + s.angle.sin() * d,
        ),
        1.0 - t,
    ))
}

/// The opacity step between two breath frames (M11n): the largest one M11f's 20 fps took, at
/// the cosine's steepest point (0.325 · 2π / 1.6 s · 50 ms ≈ 0.0638).
pub const BREATH_STEP: f32 = 0.064;
/// The least time between two breath frames.
pub const BREATH_MIN: Duration = Duration::from_millis(50);
/// When the breath rests, on its own clock (after the hold): 188 breaths of 1.6 s, so at a
/// peak. From then on the cursor stays at full strength and nothing redraws.
pub const BREATH_REST_AT: Duration = Duration::from_millis(300_800);

/// The breath's period, for the schedule.
const PERIOD: Duration = Duration::from_millis(1600);
/// A frame due less than this ahead counts as reached (a timer that fires a little early
/// draws it, and the next wait aims at the frame after).
const BREATH_SLACK: f64 = 0.001;

/// Where `clock` is within its breath, in seconds from the breath's peak (0 to 1.6).
fn breath_phase(clock: Duration) -> f64 {
    (clock.as_nanos() % PERIOD.as_nanos()) as f64 / 1e9
}

/// The 22 times within a breath, in seconds from its peak, at which it is drawn: on the way
/// down each level `1.0 - k·BREATH_STEP` (k = 1..=10), the trough at 0.8 s, on the way up the
/// same levels the other way round, and the next peak at 1.6 s. Closed form of `breath_alpha`
/// solved for its clock: `P/(2π)·acos((α - mid)/amp)` falling, `P` minus that rising.
/// Worked out once: the breathing timer asks on every wake.
fn breath_marks() -> &'static [f64] {
    static MARKS: OnceLock<Vec<f64>> = OnceLock::new();
    MARKS.get_or_init(work_out_breath_marks)
}

fn work_out_breath_marks() -> Vec<f64> {
    let p = PERIOD.as_secs_f64();
    let low = f64::from(BREATH_LOW);
    let (mid, amp) = ((1.0 + low) / 2.0, (1.0 - low) / 2.0);
    let falling: Vec<f64> = (1..)
        .map(|k| 1.0 - f64::from(k) * f64::from(BREATH_STEP))
        .take_while(|level| *level > low)
        .map(|level| p / std::f64::consts::TAU * ((level - mid) / amp).acos())
        .collect();
    let mut marks = falling.clone();
    marks.push(p / 2.0);
    marks.extend(falling.iter().rev().map(|t| p - t));
    marks.push(p);
    marks
}

/// The time from `clock` (the breath's own clock, as `breath_alpha` takes it) to the next
/// frame: the next level crossing, trough or peak (`breath_marks`) in its direction of
/// travel, but at least `BREATH_MIN`. Rounded up to the nanosecond, so the timer lands on or
/// just after the crossing.
pub fn breath_wait(clock: Duration) -> Duration {
    let at = breath_phase(clock);
    let marks = breath_marks();
    let next = marks
        .iter()
        .copied()
        .chain([PERIOD.as_secs_f64() + marks[0]])
        .find(|m| *m > at + BREATH_SLACK)
        .expect("the next breath's first level is always ahead");
    Duration::from_nanos(((next - at) * 1e9).ceil() as u64).max(BREATH_MIN)
}

/// Whether the breath is at rest: `BREATH_REST_AT` or more on its clock with no change.
pub fn breath_rests(clock: Duration) -> bool {
    clock >= BREATH_REST_AT
}

/// How often the cursor needs a redraw.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Frames {
    None,
    Breath,
    EveryFrame,
}

/// Frames are wanted only for a focused pane in the key window with motion allowed:
/// every frame while something moves, the breath's timer (`breath_wait`) for the breathing
/// glow alone. The view passes `glowing` false once the breath rests.
pub fn needs_frames(
    focused: bool,
    key_window: bool,
    motion: bool,
    glowing: bool,
    animating: bool,
) -> Frames {
    if !(focused && key_window && motion) {
        Frames::None
    } else if animating {
        Frames::EveryFrame
    } else if glowing {
        Frames::Breath
    } else {
        Frames::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn style(shape: CursorShape, blinking: bool) -> CursorStyle {
        CursorStyle { shape, blinking }
    }

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn marker_becomes_the_setting_blinking_whatever_its_flag() {
        for blinking in [true, false] {
            for (setting, shape) in [
                (CursorShapeSetting::Bar, PaintShape::Bar),
                (CursorShapeSetting::Underline, PaintShape::Underline),
                (CursorShapeSetting::Hollow, PaintShape::Hollow),
                (CursorShapeSetting::Block, PaintShape::Block),
            ] {
                assert_eq!(
                    effective_style(style(CursorShape::HollowBlock, blinking), setting),
                    PaintStyle {
                        shape,
                        blinking: true
                    }
                );
            }
        }
    }

    #[test]
    fn a_programs_choice_is_unchanged() {
        for blinking in [true, false] {
            for (shape, paint) in [
                (CursorShape::Beam, PaintShape::Bar),
                (CursorShape::Underline, PaintShape::Underline),
                (CursorShape::Block, PaintShape::Block),
                (CursorShape::Hidden, PaintShape::Hidden),
            ] {
                assert_eq!(
                    effective_style(style(shape, blinking), CursorShapeSetting::Hollow),
                    PaintStyle {
                        shape: paint,
                        blinking
                    }
                );
            }
        }
    }

    fn colours() -> (UiColors, Theme) {
        let theme = Theme::default();
        (theme.ui(), theme)
    }

    #[test]
    fn colour_follows_status_or_stays_the_themes() {
        let (ui, theme) = colours();
        let own = theme.cursor.unwrap_or(theme.foreground);
        assert_eq!(
            cursor_color(Status::Working, true, &ui, &theme),
            ui.ring_working
        );
        assert_eq!(
            cursor_color(Status::Waiting, true, &ui, &theme),
            ui.ring_waiting
        );
        assert_eq!(cursor_color(Status::Done, true, &ui, &theme), ui.ring_done);
        assert_eq!(cursor_color(Status::Idle, true, &ui, &theme), own);
        for s in [Status::Working, Status::Waiting, Status::Done, Status::Idle] {
            assert_eq!(cursor_color(s, false, &ui, &theme), own);
        }
    }

    #[test]
    fn breath_holds_then_cycles() {
        assert_eq!(breath_alpha(ms(300), ms(800)), 1.0);
        assert_eq!(breath_alpha(ms(599), ms(800)), 1.0);
        assert!((breath_alpha(ms(600), ms(800)) - 0.35).abs() < 1e-5);
        assert!((breath_alpha(ms(700), ms(0)) - 1.0).abs() < 1e-5);
        assert!((breath_alpha(ms(700), ms(800)) - 0.35).abs() < 1e-5);
        assert!((breath_alpha(ms(700), ms(1600)) - 1.0).abs() < 1e-5);
        let a = breath_alpha(ms(700), ms(400));
        assert!(a > 0.35 && a < 1.0);
    }

    /// The schedule, drawn: from clock 0, stepping by `breath_wait`, the clock
    /// and alpha of every frame up to the end of the tenth breath.
    fn frames_over(breaths: u32) -> Vec<(Duration, f32)> {
        let end = ms(1600) * breaths;
        let mut clock = Duration::ZERO;
        let mut frames = Vec::new();
        while clock <= end {
            let wait = breath_wait(clock);
            assert!(
                wait >= BREATH_MIN && wait <= ms(170),
                "the wait at {clock:?} is {wait:?}"
            );
            clock += wait;
            frames.push((clock, breath_alpha(clock + HOLD, clock)));
        }
        frames
    }

    #[test]
    fn breath_frames_are_level_crossings() {
        let frames = frames_over(10);
        let mut last = breath_alpha(HOLD, Duration::ZERO);
        for &(clock, alpha) in &frames {
            assert!(
                (alpha - last).abs() <= BREATH_STEP + 1e-4,
                "a step of {} at {clock:?}",
                (alpha - last).abs()
            );
            last = alpha;
        }
        // Breath i is (i·1.6 s, (i+1)·1.6 s], its peak frame included (shifted by 1 ms for
        // the nanosecond each wait is rounded up by).
        for i in 0..10u32 {
            let (from, to) = (ms(1600) * i + ms(1), ms(1600) * (i + 1) + ms(1));
            let breath: Vec<f32> = frames
                .iter()
                .filter(|(c, _)| *c > from && *c <= to)
                .map(|f| f.1)
                .collect();
            assert!(breath.len() <= 22, "breath {i}: {} frames", breath.len());
            assert!(
                breath.iter().any(|a| (a - 0.35).abs() < 1e-4),
                "breath {i} draws its trough: {breath:?}"
            );
            assert!(
                breath.iter().any(|a| (a - 1.0).abs() < 1e-4),
                "breath {i} draws its peak: {breath:?}"
            );
        }
    }

    #[test]
    fn the_rest_starts_at_a_peak_and_any_change_ends_it() {
        assert_eq!(
            BREATH_REST_AT.as_nanos() % ms(1600).as_nanos(),
            0,
            "a whole number of breaths"
        );
        assert_eq!(breath_alpha(BREATH_REST_AT + HOLD, BREATH_REST_AT), 1.0);
        assert!(!breath_rests(BREATH_REST_AT - ms(1)));
        assert!(breath_rests(BREATH_REST_AT));
        // At rest the cursor is full, where the breath would be at its trough.
        let trough = BREATH_REST_AT + ms(800);
        assert_eq!(breath_alpha(trough + HOLD, trough), 1.0);
        // The schedule's frame that starts the rest is the peak, on the dot.
        let mut clock = BREATH_REST_AT - ms(1600);
        while !breath_rests(clock) {
            clock += breath_wait(clock);
        }
        assert!(
            clock - BREATH_REST_AT < ms(1),
            "the rest begins at {clock:?}, not at the peak"
        );
        // Resting asks for no frames.
        assert_eq!(needs_frames(true, true, true, false, false), Frames::None);
        // That a change ends the rest is the view's part (it sets the time of the change, so
        // the breath clock starts again from 0): `CursorMotion`'s tests in `terminal_view` and
        // the ui-shots scene `cursor-breath-resumes` cover it.
    }

    fn glide(now: Instant) -> Glide {
        Glide {
            from: (0.0, 0.0),
            to: (10, 0),
            start: now,
        }
    }

    #[test]
    fn glide_position_eases_out() {
        let t0 = Instant::now();
        let g = glide(t0);
        assert_eq!(g.position(t0), (0.0, 0.0));
        let mid = g.position(t0 + ms(45)).0;
        assert!((mid - 8.75).abs() < 1e-3, "{mid}");
        assert_eq!(g.position(t0 + ms(90)), (10.0, 0.0));
        assert_eq!(g.position(t0 + ms(500)), (10.0, 0.0));
    }

    #[test]
    fn far_jumps_and_screen_switches_do_not_glide() {
        let t0 = Instant::now();
        assert!(glide_from_now(None, (0.0, 0.0), (0, 25), 25, false, t0).is_none());
        assert!(glide_from_now(None, (0.0, 0.0), (3, 1), 1, true, t0).is_none());
        assert!(glide_from_now(None, (0.0, 0.0), (3, 20), 20, false, t0).is_some());
        assert!(glide_from_now(None, (0.0, 0.0), (0, 21), 21, false, t0).is_none());
        assert!(glide_from_now(None, (3.0, 4.0), (3, 4), 0, false, t0).is_none());
    }

    #[test]
    fn a_retarget_starts_from_the_drawn_point() {
        let t0 = Instant::now();
        let g = glide(t0);
        let drawn = g.position(t0 + ms(45));
        let next = glide_from_now(Some(&g), drawn, (2, 3), 3, false, t0 + ms(45)).unwrap();
        assert_eq!(next.from, drawn);
        assert_eq!(next.to, (2, 3));
        assert_eq!(next.start, t0 + ms(45));
        // The same target keeps the running glide.
        let same = glide_from_now(Some(&g), drawn, (10, 0), 0, false, t0 + ms(45)).unwrap();
        assert_eq!(same, g);
    }

    #[test]
    fn trail_counts() {
        let t0 = Instant::now();
        let g = glide(t0);
        assert_eq!(trail(&g, t0 + ms(30)).len(), 5);
        let xs: Vec<f32> = trail(&g, t0 + ms(80)).iter().map(|c| c.0).collect();
        assert!(xs.windows(2).all(|w| w[0] > w[1]), "{xs:?}");
        assert!(xs[0] < g.position(t0 + ms(80)).0);
        // 149 ms after the glide ends the copies are all but gone; at 150 ms they are.
        let late = trail(&g, t0 + ms(239));
        assert_eq!(late.len(), 5);
        assert!(late.iter().all(|c| c.3 < 0.01));
        assert!(trail(&g, t0 + ms(240)).is_empty());
        assert_eq!(trail(&g, t0 + ms(150)).len(), 5);
        assert!(trail(&g, t0 + ms(240)).is_empty());
        assert!(trail(&g, t0 + ms(400)).is_empty());
        let short = Glide {
            from: (4.0, 0.0),
            to: (5, 0),
            start: t0,
        };
        assert!(trail(&short, t0 + ms(30)).is_empty());
        for (_, _, scale, alpha) in trail(&g, t0 + ms(30)) {
            assert!(scale > 0.0 && scale < 1.0 && alpha > 0.0 && alpha < 1.0);
        }
    }

    #[test]
    fn sparks_are_deterministic_spread_and_short_lived() {
        let t0 = Instant::now();
        let a = sparks_for_key(42, (100.0, 50.0), t0);
        assert_eq!(a, sparks_for_key(42, (100.0, 50.0), t0));
        assert_ne!(a, sparks_for_key(43, (100.0, 50.0), t0));
        for (i, s) in a.iter().enumerate() {
            assert!((6.0..=14.0).contains(&s.reach), "{}", s.reach);
            for o in &a[i + 1..] {
                assert!((s.angle - o.angle).abs() > 0.1);
            }
        }
        let (p, alpha) = spark_at(&a[0], t0).unwrap();
        assert_eq!(p, (100.0, 50.0));
        assert!((alpha - 1.0).abs() < 1e-5);
        assert!(spark_at(&a[0], t0 + ms(349)).is_some());
        assert!(spark_at(&a[0], t0 + ms(350)).is_none());
    }

    #[test]
    fn frames_wanted() {
        assert_eq!(needs_frames(false, true, true, true, true), Frames::None);
        assert_eq!(needs_frames(true, false, true, true, true), Frames::None);
        assert_eq!(needs_frames(true, true, false, true, true), Frames::None);
        assert_eq!(needs_frames(true, true, true, true, false), Frames::Breath);
        assert_eq!(
            needs_frames(true, true, true, false, true),
            Frames::EveryFrame
        );
        assert_eq!(
            needs_frames(true, true, true, true, true),
            Frames::EveryFrame
        );
        assert_eq!(needs_frames(true, true, true, false, false), Frames::None);
    }
}
