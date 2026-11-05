//! Status marks (M11): the shape in front of every session, so its state reads by shape and
//! motion as well as by colour (also to colour-blind eyes):
//! - **Idle:** a hollow ring.
//! - **Working:** a 270° arc that turns once every 1.2 s.
//! - **Waiting:** a solid dot with a halo that breathes, once every 1.6 s.
//! - **Done:** a solid dot with a check, which pops once when the agent finishes.
//!
//! Every mark reads one shared clock, `phase`: 0 to 1 over a 4.8 s beat, so the arc's four turns
//! and the halo's three breaths both fit it exactly and every spinner on screen turns together.
//! The Workspace advances it (`advance`) every `TICK` while `should_tick`; that ticker is the
//! only continuous redraw in fuxx.
//!
//! With motion off (`App::reduce_motion`, which fuxx sets from the `animations` setting and
//! macOS Reduce motion, see `reduce_motion`) the marks are drawn still: the arc doesn't turn, the
//! halo holds at its middle strength and Done doesn't pop.

use std::f32::consts::TAU;
use std::time::Duration;

use gpui::{
    App, Bounds, FillOptions, FillRule, Hsla, IntoElement, PathBuilder, PathStyle, Pixels, Point,
    Window, canvas, point, prelude::*, px,
};

use crate::status::Status;
use crate::terminal_view::hsla;
use crate::theme::UiColors;

/// The ticker's interval: 10 frames a second.
pub const TICK: Duration = Duration::from_millis(100);
/// Ticks in one beat of `phase` (4.8 s).
const BEAT_TICKS: f32 = 48.0;
/// Working turns once per 1.2 s: four times a beat.
pub const TURNS_PER_BEAT: f32 = 4.0;
/// Waiting's halo breathes once per 1.6 s: three times a beat.
pub const BREATHS_PER_BEAT: f32 = 3.0;
/// The halo's alpha, faintest and strongest; drawn still, it holds halfway between.
const HALO_MIN: f32 = 0.15;
const HALO_MAX: f32 = 0.45;
/// How long the Done pop lasts.
pub const POP: Duration = Duration::from_millis(350);

/// Whether the marks' 100 ms ticker should run: only with animations on, macOS Reduce motion
/// off, and a mark that moves (Working or Waiting) on screen.
pub fn should_tick(animations: bool, reduce: bool, visible: &[Status]) -> bool {
    animations
        && !reduce
        && visible
            .iter()
            .any(|s| matches!(s, Status::Working | Status::Waiting))
}

/// The clock one tick later: a whole number of ticks into the beat, wrapping at 1.
pub fn advance(phase: f32) -> f32 {
    let tick = (phase * BEAT_TICKS).round() + 1.0;
    (tick % BEAT_TICKS) / BEAT_TICKS
}

/// The Done mark's scale `t` (0-1) of the way through its pop: from 0.6 it eases out past its
/// size to 1.15, then settles back to 1 (an ease-out-back feel). From 1 on it's at rest.
pub fn pop_curve(t: f32) -> f32 {
    const FROM: f32 = 0.6;
    const PEAK: f32 = 1.15;
    // Where in the pop it peaks.
    const TURN: f32 = 0.6;
    if t >= 1.0 {
        return 1.0;
    }
    let t = t.max(0.0);
    if t < TURN {
        let x = t / TURN;
        FROM + (PEAK - FROM) * (1.0 - (1.0 - x).powi(3))
    } else {
        let x = (t - TURN) / (1.0 - TURN);
        PEAK + (1.0 - PEAK) * x * x * (3.0 - 2.0 * x)
    }
}

/// The halo's alpha `phase` into the beat: a smooth breath between `HALO_MIN` and `HALO_MAX`.
fn halo_alpha(phase: f32) -> f32 {
    let breath = (phase * BREATHS_PER_BEAT).fract();
    let mid = (HALO_MIN + HALO_MAX) / 2.0;
    mid - (HALO_MAX - HALO_MIN) / 2.0 * (breath * TAU).cos()
}

/// `status`'s mark in a `size` box (12 px in the chrome; it scales with the box). `phase` is the
/// shared clock (see the module); `popping`, how far (0-1) through its pop a Done mark is.
pub fn mark(
    status: Status,
    ui: &UiColors,
    phase: f32,
    popping: Option<f32>,
    size: Pixels,
) -> impl IntoElement {
    let ui = *ui;
    canvas(
        |_, _, _| (),
        move |bounds, _, window, cx| paint(status, ui, phase, popping, bounds, window, cx),
    )
    .flex_none()
    .size(size)
}

/// Line width, in a 12 px box.
const STROKE: f32 = 1.5;
/// The ring's and the arc's radius to the middle of the line, in a 12 px box (so 10 px across).
const RING: f32 = 4.25;
/// Waiting's dot and halo, and Done's dot, in a 12 px box.
const WAITING_DOT: f32 = 3.5;
const HALO: f32 = 6.0;
const DONE_DOT: f32 = 5.0;
/// Fine flattening: the marks are tiny, so the default 0.1 px tolerance shows facets.
const TOLERANCE: f32 = 0.01;

fn paint(
    status: Status,
    ui: UiColors,
    phase: f32,
    popping: Option<f32>,
    bounds: Bounds<Pixels>,
    window: &mut Window,
    cx: &mut App,
) {
    let still = cx.reduce_motion();
    // One unit is a pixel of a 12 px box.
    let unit = f32::from(bounds.size.width.min(bounds.size.height)) / 12.0;
    let c = bounds.center();
    match status {
        Status::Idle => {
            let ring = annulus(
                c,
                (RING - STROKE / 2.0) * unit,
                (RING + STROKE / 2.0) * unit,
            );
            draw(window, ring, hsla(ui.ring_idle));
        }
        Status::Working => {
            let turn = if still {
                0.0
            } else {
                (phase * TURNS_PER_BEAT).fract()
            };
            // From the top, clockwise, leaving a quarter open.
            let start = -TAU / 4.0 + turn * TAU;
            let arc = round_arc(c, RING * unit, STROKE * unit, start, start + 0.75 * TAU);
            draw(window, arc, hsla(ui.ring_working));
        }
        Status::Waiting => {
            let alpha = if still {
                (HALO_MIN + HALO_MAX) / 2.0
            } else {
                halo_alpha(phase)
            };
            let halo = Hsla {
                a: alpha,
                ..hsla(ui.ring_waiting)
            };
            draw(window, disc(c, HALO * unit), halo);
            draw(window, disc(c, WAITING_DOT * unit), hsla(ui.ring_waiting));
        }
        Status::Done => {
            let scale = match popping {
                Some(t) if !still => pop_curve(t),
                _ => 1.0,
            };
            let u = unit * scale;
            draw(window, disc(c, DONE_DOT * u), hsla(ui.ring_done));
            draw(window, check(c, u), hsla(ui.sidebar_background));
        }
    }

    fn draw(window: &mut Window, path: PathBuilder, color: Hsla) {
        if let Ok(path) = path.build() {
            window.paint_path(path, color);
        }
    }
}

/// A fill path builder with the fine tolerance.
fn filled() -> PathBuilder {
    PathBuilder::fill().with_style(PathStyle::Fill(FillOptions::tolerance(TOLERANCE)))
}

/// The point at `angle` (radians, clockwise from the right, as y grows down) `r` from `c`.
fn at(c: Point<Pixels>, r: f32, angle: f32) -> Point<Pixels> {
    point(c.x + px(r * angle.cos()), c.y + px(r * angle.sin()))
}

/// A circle of radius `r` around `c` as two half arcs, added to `path`.
fn circle(path: &mut PathBuilder, c: Point<Pixels>, r: f32) {
    let radii = point(px(r), px(r));
    path.move_to(at(c, r, 0.0));
    path.arc_to(radii, px(0.0), false, true, at(c, r, TAU / 2.0));
    path.arc_to(radii, px(0.0), false, true, at(c, r, 0.0));
    path.close();
}

fn disc(c: Point<Pixels>, r: f32) -> PathBuilder {
    let mut path = filled();
    circle(&mut path, c, r);
    path
}

/// A ring between radii `inner` and `outer` (even-odd: the inner circle is the hole).
fn annulus(c: Point<Pixels>, inner: f32, outer: f32) -> PathBuilder {
    let mut path = PathBuilder::fill().with_style(PathStyle::Fill(
        FillOptions::tolerance(TOLERANCE).with_fill_rule(FillRule::EvenOdd),
    ));
    circle(&mut path, c, outer);
    circle(&mut path, c, inner);
    path
}

/// An arc of radius `r` and line width `width` from angle `from` clockwise to `to` (less than a
/// full turn, more than half), with round ends. Drawn as one filled outline, since GPUI's
/// stroke has no round caps: out along the outer edge, round the end, back along the inner
/// edge, round the start.
fn round_arc(c: Point<Pixels>, r: f32, width: f32, from: f32, to: f32) -> PathBuilder {
    let h = width / 2.0;
    let (outer, inner) = (point(px(r + h), px(r + h)), point(px(r - h), px(r - h)));
    let cap = point(px(h), px(h));
    let mut path = filled();
    path.move_to(at(c, r + h, from));
    path.arc_to(outer, px(0.0), true, true, at(c, r + h, to));
    path.arc_to(cap, px(0.0), false, true, at(c, r - h, to));
    path.arc_to(inner, px(0.0), true, false, at(c, r - h, from));
    path.arc_to(cap, px(0.0), false, true, at(c, r + h, from));
    path.close();
    path
}

/// The check inside Done's dot, `unit` a pixel of a 12 px box.
fn check(c: Point<Pixels>, unit: f32) -> PathBuilder {
    let p = |x: f32, y: f32| point(c.x + px(x * unit), c.y + px(y * unit));
    let mut path = PathBuilder::stroke(px(STROKE * unit));
    path.move_to(p(-2.3, 0.1));
    path.line_to(p(-0.7, 1.7));
    path.line_to(p(2.3, -1.5));
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_tick_only_when_something_moves() {
        use Status::*;
        // Something spins or pulses, with motion on: tick.
        assert!(should_tick(true, false, &[Idle, Working]));
        assert!(should_tick(true, false, &[Done, Waiting]));
        // Only still marks on screen (or none): no ticker.
        assert!(!should_tick(true, false, &[Idle, Done]));
        assert!(!should_tick(true, false, &[]));
        // Animations off, or macOS Reduce motion on: no ticker whatever is on screen.
        assert!(!should_tick(false, false, &[Working, Waiting]));
        assert!(!should_tick(true, true, &[Working, Waiting]));
    }

    #[test]
    fn pop_curve_ends_at_rest() {
        assert!((pop_curve(0.0) - 0.6).abs() < 1e-4, "starts small");
        assert!((pop_curve(1.0) - 1.0).abs() < 1e-6, "ends at rest");
        // Past the end (a late frame) it stays at rest.
        assert_eq!(pop_curve(1.5), 1.0);
        // It overshoots to about 1.15 once, and never beyond.
        let samples: Vec<f32> = (0..=100).map(|i| pop_curve(i as f32 / 100.0)).collect();
        let peak = samples.iter().copied().fold(0.0, f32::max);
        assert!((peak - 1.15).abs() < 0.01, "peak {peak}");
        assert!(
            samples
                .iter()
                .all(|s| (0.6 - 1e-4..=1.15 + 1e-4).contains(s))
        );
        // Continuous: no jumps between neighbouring samples.
        assert!(samples.windows(2).all(|w| (w[1] - w[0]).abs() < 0.05));
    }

    #[test]
    fn the_clock_wraps_after_one_beat() {
        let mut phase = 0.0;
        for _ in 0..48 {
            phase = advance(phase);
            assert!((0.0..1.0).contains(&phase));
        }
        assert_eq!(phase, 0.0);
        // Twelve ticks (1.2 s) are one turn of the arc.
        let quarter = (0..12).fold(0.0, |p, _| advance(p));
        assert!((quarter * TURNS_PER_BEAT - 1.0).abs() < 1e-6);
    }
}
