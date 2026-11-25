//! Status marks (M11): the shape in front of every session, so its state reads by shape and
//! motion as well as by colour (also to colour-blind eyes):
//! - **Idle:** a hollow ring. In a sidebar row (M11i, `idle_mark`) it says one thing more: a
//!   filled dot while a program runs there but is quiet, a hollow ring at the shell's prompt.
//! - **Working:** a 270° arc that turns once every 1.2 s.
//! - **Waiting:** a solid dot with a halo that breathes, once every 1.6 s.
//! - **Done:** a solid dot with a check, which pops once when the agent finishes.
//!
//! Every mark reads one shared clock, `phase`: 0 to 1 over a 4.8 s beat, so the arc's four turns
//! and the halo's three breaths both fit it exactly and every spinner on screen turns together.
//! The Workspace's ticker advances it (`advance`) every `TICK` while `should_tick`, in a
//! `mark_view::MarkClock` that only the moving marks' own views (and the open palette) watch
//! (M11p); that ticker is the only continuous redraw in fuxx.
//!
//! With motion off (`App::reduce_motion`, which fuxx sets from the `animations` setting and
//! macOS Reduce motion, see `reduce_motion`) the marks are drawn still: the arc doesn't turn, the
//! halo holds at its middle strength and Done doesn't pop.

use std::f32::consts::TAU;
use std::time::Duration;

use alacritty_terminal::vte::ansi::Rgb;
use gpui::{
    AnyElement, App, Hsla, IntoElement, Pixels, Transformation, Window, div, prelude::*, px,
    radians,
};

use crate::icons::{Icon, icon};
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
/// The beat's phase at which the moving marks look as they do drawn still (Reduce motion):
/// Working's arc from the top (four turns a beat: a whole turn) and Waiting's halo halfway
/// (three breaths a beat: three-quarters of one, where the breath is at its middle). (The
/// Activity panel drew its still marks at it until M11v part 4 gave it dots; the tests keep it;
/// a question the agent didn't wait on draws its Waiting mark still at it, 2026-11-24.)
pub const STILL_PHASE: f32 = 0.25;
/// How long the Done pop lasts.
pub const POP: Duration = Duration::from_millis(350);

/// Whether the marks' 100 ms ticker should run: only with animations on, macOS Reduce motion
/// off, the window visible (not hidden, minimised or covered), and a mark that moves (Working or
/// Waiting) on screen.
pub fn should_tick(animations: bool, reduce: bool, window_visible: bool, marks: &[Status]) -> bool {
    animations
        && !reduce
        && window_visible
        && marks
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

/// `status`'s mark in a `size` box (12 px in the chrome; it scales with the box), on a surface
/// of colour `fill` (the sidebar, a filled or tinted row): Done's check is cut out of its dot in
/// that colour, so it reads as a hole whatever the row is. `phase` is the shared clock (see the
/// module); `popping`, how far (0-1) through its pop a Done mark is.
pub fn mark_on(
    status: Status,
    ui: &UiColors,
    phase: f32,
    popping: Option<f32>,
    size: Pixels,
    fill: Rgb,
) -> impl IntoElement {
    Mark {
        status,
        ui: *ui,
        phase,
        popping,
        size,
        fill,
    }
}

/// A mark, laid out when it renders (so it reads `App::reduce_motion` then).
#[derive(IntoElement)]
struct Mark {
    status: Status,
    ui: UiColors,
    phase: f32,
    popping: Option<f32>,
    size: Pixels,
    fill: Rgb,
}

impl RenderOnce for Mark {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let still = cx.reduce_motion();
        let box_ = f32::from(self.size);
        let pieces = layers(
            self.status,
            &self.ui,
            self.phase,
            self.popping,
            still,
            self.fill,
            box_,
        );
        div()
            .flex_none()
            .relative()
            .size(self.size)
            .children(pieces.into_iter().map(|l| l.into_element(box_)))
    }
}

/// A sidebar row's Idle mark (M11i): 6 pt across; the ring's line as wide as the other marks'.
const IDLE_MARK: f32 = 6.0;
const IDLE_RING: f32 = 1.5;

/// A sidebar row's Idle mark (M11i): a filled 6 pt dot in `text_dim` while a program other than
/// the shell runs there but is quiet (a server, an idle agent); a hollow 6 pt ring in
/// `text_faint` when the shell is at its prompt, or what runs isn't known.
pub fn idle_mark(running: bool, ui: &UiColors) -> impl IntoElement {
    let mark = div().flex_none().size(px(IDLE_MARK)).rounded_full();
    if running {
        mark.bg(hsla(ui.text_dim))
    } else {
        mark.border(px(IDLE_RING)).border_color(hsla(ui.text_faint))
    }
}

/// The sidebar's Done (chore, 2026-11-17: colour means activity, so a finished one isn't a
/// bright dot): the check alone in `color` (a muted one), in a `size` box. Drawn still.
pub fn done_check(color: Rgb, size: Pixels) -> impl IntoElement {
    icon(Icon::MarkCheck, size, hsla(color))
}

/// A failed subagent's mark (the lanes review, fix 4: a dot read as Waiting's): a small ✕ in
/// `color`, in a `size` box, its line the check's. Drawn still.
pub fn failed_cross(color: Rgb, size: Pixels) -> impl IntoElement {
    icon(Icon::MarkCross, size, hsla(color))
}

/// Line width, in a 12 px box.
const STROKE: f32 = 1.5;
/// The ring's and the arc's radius to the middle of the line, in a 12 px box (so 10 px across).
const RING: f32 = 4.25;
/// Waiting's dot and halo, and Done's dot, in a 12 px box.
const WAITING_DOT: f32 = 3.5;
const HALO: f32 = 6.0;
const DONE_DOT: f32 = 5.0;
/// The check's glyph box, in a 12 px box (its SVG is 8 units across, as `done_check`'s box).
const CHECK_BOX: f32 = 8.0;

/// One piece of a mark, centred in its box, `d` px across. Discs and rings are rounded quads and
/// the arc and the check SVG sprites (the scroll-lag chore, 2026-11-25): each painted path cost
/// the GPU a whole-window pass, and a long chat drew one per finished step, every frame.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Layer {
    Disc {
        d: f32,
        color: Hsla,
    },
    /// A ring `d` across on its outside, its line `width`.
    Ring {
        d: f32,
        width: f32,
        color: Hsla,
    },
    /// An SVG glyph in a `d` box, turned `turn` radians clockwise about its middle.
    Glyph {
        icon: Icon,
        d: f32,
        turn: f32,
        color: Hsla,
    },
}

impl Layer {
    pub(crate) fn d(&self) -> f32 {
        match self {
            Layer::Disc { d, .. } | Layer::Ring { d, .. } | Layer::Glyph { d, .. } => *d,
        }
    }

    /// This piece centred in a `box_` px square.
    fn into_element(self, box_: f32) -> AnyElement {
        let at = (box_ - self.d()) / 2.0;
        match self {
            Layer::Disc { d, color } => centred(at, d).rounded_full().bg(color).into_any_element(),
            Layer::Ring { d, width, color } => centred(at, d)
                .rounded_full()
                .border(px(width))
                .border_color(color)
                .into_any_element(),
            Layer::Glyph {
                icon: glyph,
                d,
                turn,
                color,
            } => {
                let sprite = icon(glyph, px(d), color)
                    .absolute()
                    .left(px(at))
                    .top(px(at));
                if turn == 0.0 {
                    sprite.into_any_element()
                } else {
                    sprite
                        .with_transformation(Transformation::rotate(radians(turn)))
                        .into_any_element()
                }
            }
        }
    }
}

/// An absolutely placed `d` px square, `at` px in from its box's top-left.
fn centred(at: f32, d: f32) -> gpui::Div {
    div().absolute().left(px(at)).top(px(at)).size(px(d))
}

/// What `status`'s mark is made of in a `box_` px box (see `mark_on`); `still` is motion off.
pub(crate) fn layers(
    status: Status,
    ui: &UiColors,
    phase: f32,
    popping: Option<f32>,
    still: bool,
    fill: Rgb,
    box_: f32,
) -> Vec<Layer> {
    // One unit is a pixel of a 12 px box.
    let unit = box_ / 12.0;
    match status {
        Status::Idle => vec![Layer::Ring {
            d: 2.0 * (RING + STROKE / 2.0) * unit,
            width: STROKE * unit,
            color: hsla(ui.ring_idle),
        }],
        Status::Working => {
            // From the top, clockwise, leaving a quarter open (the glyph's own start).
            let turn = if still {
                0.0
            } else {
                (phase * TURNS_PER_BEAT).fract() * TAU
            };
            vec![Layer::Glyph {
                icon: Icon::MarkArc,
                d: 12.0 * unit,
                turn,
                color: hsla(ui.ring_working),
            }]
        }
        Status::Waiting => {
            let alpha = if still {
                (HALO_MIN + HALO_MAX) / 2.0
            } else {
                halo_alpha(phase)
            };
            vec![
                Layer::Disc {
                    d: 2.0 * HALO * unit,
                    color: Hsla {
                        a: alpha,
                        ..hsla(ui.ring_waiting)
                    },
                },
                Layer::Disc {
                    d: 2.0 * WAITING_DOT * unit,
                    color: hsla(ui.ring_waiting),
                },
            ]
        }
        Status::Done => {
            let scale = match popping {
                Some(t) if !still => pop_curve(t),
                _ => 1.0,
            };
            let u = unit * scale;
            vec![
                Layer::Disc {
                    d: 2.0 * DONE_DOT * u,
                    color: hsla(ui.ring_done),
                },
                Layer::Glyph {
                    icon: Icon::MarkCheck,
                    d: CHECK_BOX * u,
                    turn: 0.0,
                    color: hsla(fill),
                },
            ]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_still_phase_draws_moving_marks_as_reduce_motion_does() {
        // Waiting's halo halfway (as drawn still), Working's arc from the top (turn 0).
        let mid = (HALO_MIN + HALO_MAX) / 2.0;
        assert!((halo_alpha(STILL_PHASE) - mid).abs() < 1e-4);
        assert_eq!((STILL_PHASE * TURNS_PER_BEAT).fract(), 0.0);
    }

    #[test]
    fn should_tick_only_when_something_moves() {
        use Status::*;
        // Something spins or pulses, with motion on and the window in view: tick.
        assert!(should_tick(true, false, true, &[Idle, Working]));
        assert!(should_tick(true, false, true, &[Done, Waiting]));
        // Only still marks on screen (or none): no ticker.
        assert!(!should_tick(true, false, true, &[Idle, Done]));
        assert!(!should_tick(true, false, true, &[]));
        // Animations off, or macOS Reduce motion on: no ticker whatever is on screen.
        assert!(!should_tick(false, false, true, &[Working, Waiting]));
        assert!(!should_tick(true, true, true, &[Working, Waiting]));
        // The window hidden, minimised or fully covered: nothing to see, so no ticker.
        assert!(!should_tick(true, false, false, &[Working, Waiting]));
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

    fn ui() -> UiColors {
        crate::theme::Theme::default().ui()
    }

    fn rgb(r: u8, g: u8, b: u8) -> Rgb {
        Rgb { r, g, b }
    }

    #[test]
    fn a_12px_mark_keeps_the_sizes_the_paths_had() {
        let (ui, fill) = (ui(), rgb(1, 2, 3));
        let at = |s| layers(s, &ui, 0.0, None, true, fill, 12.0);
        // Idle: a ring 10 px across (radius 4.25 to the middle of a 1.5 line).
        assert_eq!(
            at(Status::Idle),
            vec![Layer::Ring {
                d: 10.0,
                width: 1.5,
                color: hsla(ui.ring_idle)
            }]
        );
        // Working: the arc's glyph fills the box.
        assert!(matches!(
            at(Status::Working)[..],
            [Layer::Glyph { icon: Icon::MarkArc, d, .. }] if d == 12.0
        ));
        // Waiting: a 12 px halo under a 7 px dot.
        match &at(Status::Waiting)[..] {
            [Layer::Disc { d: halo, .. }, Layer::Disc { d: dot, color }] => {
                assert_eq!((*halo, *dot), (12.0, 7.0));
                assert_eq!(*color, hsla(ui.ring_waiting));
            }
            other => panic!("waiting: {other:?}"),
        }
        // Done: a 10 px dot, its check (an 8-unit glyph) cut out in the surface's colour.
        assert_eq!(
            at(Status::Done),
            vec![
                Layer::Disc {
                    d: 10.0,
                    color: hsla(ui.ring_done)
                },
                Layer::Glyph {
                    icon: Icon::MarkCheck,
                    d: 8.0,
                    turn: 0.0,
                    color: hsla(fill)
                },
            ]
        );
        // They scale with the box.
        assert_eq!(
            layers(Status::Idle, &ui, 0.0, None, true, fill, 24.0),
            vec![Layer::Ring {
                d: 20.0,
                width: 3.0,
                color: hsla(ui.ring_idle)
            }]
        );
    }

    #[test]
    fn the_arc_turns_with_the_clock_and_stands_still_with_motion_off() {
        let (ui, fill) = (ui(), rgb(0, 0, 0));
        let turn =
            |phase, still| match layers(Status::Working, &ui, phase, None, still, fill, 12.0)[..] {
                [Layer::Glyph { turn, .. }] => turn,
                _ => panic!("one glyph"),
            };
        // An eighth of a beat is half a turn (four turns a beat).
        assert!((turn(0.125, false) - TAU / 2.0).abs() < 1e-4);
        assert_eq!(turn(0.125, true), 0.0, "still: from the top");
    }

    #[test]
    fn the_halo_breathes_and_holds_its_middle_with_motion_off() {
        let (ui, fill) = (ui(), rgb(0, 0, 0));
        let alpha =
            |phase, still| match layers(Status::Waiting, &ui, phase, None, still, fill, 12.0)[..] {
                [Layer::Disc { color, .. }, _] => color.a,
                _ => panic!("halo and dot"),
            };
        assert!((alpha(0.0, false) - HALO_MIN).abs() < 1e-4);
        assert!((alpha(1.0 / 6.0, false) - HALO_MAX).abs() < 1e-4);
        assert!((alpha(0.0, true) - (HALO_MIN + HALO_MAX) / 2.0).abs() < 1e-4);
    }

    #[test]
    fn done_pops_from_small_unless_motion_is_off() {
        let (ui, fill) = (ui(), rgb(0, 0, 0));
        let sizes = |popping, still| {
            layers(Status::Done, &ui, 0.0, popping, still, fill, 12.0)
                .iter()
                .map(Layer::d)
                .collect::<Vec<_>>()
        };
        let close = |a: Vec<f32>, b: [f32; 2]| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-4);
        assert!(close(sizes(Some(0.0), false), [6.0, 4.8]), "starts at 0.6");
        assert!(close(sizes(Some(0.0), true), [10.0, 8.0]));
        assert!(close(sizes(None, false), [10.0, 8.0]));
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
