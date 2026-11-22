//! The agent box's sweep (the lead designer's spec, 2026-11-22; your words: "a Rotating
//! Conic-Gradient animation every time user visit the tab … animating once"): one comet of the
//! accent runs once round the inner box's rim, clockwise from its top-left corner, and fades
//! back into the rim. Pure: the rim's outline, the comet's colour along its tail and its
//! timing; `agent_box` paints it, `Workspace` starts it when the tab is shown.

use alacritty_terminal::vte::ansi::Rgb;

use crate::theme::mix;

/// The pass: a short wait (the box laid out), then one lap.
pub const DELAY_MS: f32 = 60.0;
pub const LAP_MS: f32 = 1200.0;
/// Its fades: in over the lap's first 150 ms, out over its last 350.
const FADE_IN_MS: f32 = 150.0;
const FADE_OUT_FROM_MS: f32 = 850.0;
/// The tail's length (a share of the rim) and how many pieces it is painted in (a path each):
/// longer since your words (2026-11-22: "the effect is not too outstanding … make it great").
pub const TAIL: f32 = 0.30;
pub const PIECES: usize = 28;
/// The head's glow: round layers about it, each its radius and opacity (dark, light), the widest
/// and faintest first (round, so no square ends show).
pub const GLOW: [(f32, f32, f32); 3] = [(7.0, 0.10, 0.07), (4.5, 0.22, 0.16), (2.0, 0.95, 0.9)];
/// The landing: over the lap's last quarter the whole rim lights once and fades.
pub const LANDING_FROM: f32 = 0.75;
/// With reduce motion: nothing moves; the rim tints and fades back over this long.
pub const STILL_MS: f32 = 500.0;

/// One frame of the pass: how far round the head is (0 to 1 of the rim, eased), how much of
/// the comet shows (its fades), and how much the whole rim lights (its landing, 0 to 1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    pub head: f32,
    pub fade: f32,
    pub landing: f32,
}

/// The frame `ms` after the pass was asked for; none once it is over (nothing is drawn then).
pub fn frame(ms: f32) -> Option<Frame> {
    let lap = ms - DELAY_MS;
    if !(0.0..=LAP_MS).contains(&lap) {
        return (lap < 0.0).then_some(Frame {
            head: 0.0,
            fade: 0.0,
            landing: 0.0,
        });
    }
    let head = crate::agent_top::ease_out_cubic(lap / LAP_MS);
    let fade_in = (lap / FADE_IN_MS).min(1.0);
    let out = ((lap - FADE_OUT_FROM_MS) / (LAP_MS - FADE_OUT_FROM_MS)).clamp(0.0, 1.0);
    let fade_out = 1.0 - out * out;
    let late = ((lap / LAP_MS - LANDING_FROM) / (1.0 - LANDING_FROM)).clamp(0.0, 1.0);
    Some(Frame {
        head,
        fade: fade_in.min(fade_out),
        landing: (late * std::f32::consts::PI).sin(),
    })
}

/// The comet's colour and opacity at `t` behind its head (0 the head, 1 the tail's end, where it
/// has faded into the rim), on a dark or light theme.
pub fn ink(t: f32, dark: bool, accent: Rgb, glow_top: Rgb) -> (Rgb, f32) {
    let t = t.clamp(0.0, 1.0);
    let lerp = |a: f32, b: f32, k: f32| a + (b - a) * k;
    let (near, mid) = if dark { (0.95, 0.5) } else { (0.9, 0.45) };
    let alpha = if t < 0.12 {
        lerp(1.0, near, t / 0.12)
    } else if t < 0.5 {
        lerp(near, mid, (t - 0.12) / 0.38)
    } else {
        lerp(mid, 0.0, (t - 0.5) / 0.5)
    };
    let color = if dark {
        let head = mix(
            accent,
            Rgb {
                r: 255,
                g: 255,
                b: 255,
            },
            0.5,
        );
        if t < 0.12 {
            mix(head, accent, t / 0.12)
        } else {
            accent
        }
    } else if t < 0.12 {
        accent
    } else {
        mix(accent, glow_top, (0.5 * (t - 0.12) / 0.38).min(0.5))
    };
    (color, alpha)
}

/// The inner box's shape for its outline: its corners' radii and the send notch's wave.
#[derive(Clone, Copy, Debug)]
pub struct Shape {
    pub top_left: f32,
    pub top_right: f32,
    pub bottom_left: f32,
    pub notch: Notch,
}

/// The wave round the send circle: the circle's centre, the cut's radius and the joints'.
#[derive(Clone, Copy, Debug)]
pub struct Notch {
    pub centre: (f32, f32),
    pub cut: f32,
    pub joint: f32,
}

/// The rim's outline as points along it and the distance to each from the start.
#[derive(Clone, Debug, Default)]
pub struct Outline {
    points: Vec<(f32, f32)>,
    along: Vec<f32>,
}

impl Outline {
    /// Its length.
    pub fn length(&self) -> f32 {
        self.along.last().copied().unwrap_or(0.0)
    }

    /// The point `s` along it (clamped to its ends).
    pub fn at(&self, s: f32) -> (f32, f32) {
        let s = s.clamp(0.0, self.length());
        let i = self.along.partition_point(|&a| a < s).max(1);
        if i >= self.points.len() {
            return *self.points.last().unwrap_or(&(0.0, 0.0));
        }
        let (a, b) = (self.along[i - 1], self.along[i]);
        let k = if b > a { (s - a) / (b - a) } else { 0.0 };
        let (p, q) = (self.points[i - 1], self.points[i]);
        (p.0 + (q.0 - p.0) * k, p.1 + (q.1 - p.1) * k)
    }

    /// The points from `a` to `b` along it (each end on the line, the corners between kept).
    pub fn span(&self, a: f32, b: f32) -> Vec<(f32, f32)> {
        let (a, b) = (a.clamp(0.0, self.length()), b.clamp(0.0, self.length()));
        let mut pts = vec![self.at(a)];
        pts.extend(
            self.points
                .iter()
                .zip(&self.along)
                .filter(|&(_, &s)| s > a && s < b)
                .map(|(p, _)| *p),
        );
        pts.push(self.at(b));
        pts
    }
}

/// Points along the arc about `centre` of radius `r` from angle `from` to `to` (radians, y
/// down), about 2 pt apart, both ends included.
fn arc(pts: &mut Vec<(f32, f32)>, (cx, cy): (f32, f32), r: f32, from: f32, to: f32) {
    let steps = ((r * (to - from).abs()) / 2.0).ceil().max(1.0) as usize;
    for k in 0..=steps {
        let a = from + (to - from) * k as f32 / steps as f32;
        pts.push((cx + r * a.cos(), cy + r * a.sin()));
    }
}

/// The inner box's outline from (`x0`, `y0`) to (`x1`, `y1`) as `shape` rounds and cuts it,
/// clockwise from where its top-left corner meets its top edge (under the folder tab), its
/// curves in steps of about 2 pt; the start and end are the same point.
pub fn outline(x0: f32, y0: f32, x1: f32, y1: f32, shape: Shape) -> Outline {
    use std::f32::consts::{FRAC_PI_2, PI};
    let Shape {
        top_left: tl,
        top_right: tr,
        bottom_left: bl,
        notch: n,
    } = shape;
    let (cx, cy) = n.centre;
    let (top, left, j) = (cy - n.cut, cx - n.cut, n.joint);
    let mut points = vec![(x0 + tl, y0)];
    arc(&mut points, (x1 - tr, y0 + tr), tr, -FRAC_PI_2, 0.0);
    arc(&mut points, (x1 - j, top - j), j, 0.0, FRAC_PI_2);
    arc(&mut points, (cx, cy), n.cut, -FRAC_PI_2, -PI);
    arc(&mut points, (left - j, y1 - j), j, 0.0, FRAC_PI_2);
    arc(&mut points, (x0 + bl, y1 - bl), bl, FRAC_PI_2, PI);
    arc(&mut points, (x0 + tl, y0 + tl), tl, PI, 1.5 * PI);
    let mut along = Vec::with_capacity(points.len());
    let mut s = 0.0;
    for (i, p) in points.iter().enumerate() {
        if i > 0 {
            let q = points[i - 1];
            s += ((p.0 - q.0).powi(2) + (p.1 - q.1).powi(2)).sqrt();
        }
        along.push(s);
    }
    Outline { points, along }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_box() -> Outline {
        // A 600 × 80 box: corners 12, 12 and 17; the notch's circle 22 in from its corner.
        outline(
            0.0,
            0.0,
            600.0,
            80.0,
            Shape {
                top_left: 12.0,
                top_right: 12.0,
                bottom_left: 17.0,
                notch: Notch {
                    centre: (583.0, 63.0),
                    cut: 25.0,
                    joint: 14.0,
                },
            },
        )
    }

    #[test]
    fn the_outline_runs_round_the_box_from_its_top_left() {
        let o = a_box();
        assert_eq!(o.at(0.0), (12.0, 0.0), "from the top-left corner's end");
        let end = o.at(o.length());
        assert!(
            (end.0 - 12.0).abs() < 0.01 && end.1.abs() < 0.01,
            "and back: {end:?}"
        );
        // About the box's perimeter (its corners and the wave take a little off).
        let around = 2.0 * (600.0 + 80.0);
        assert!(
            o.length() > 0.85 * around && o.length() < around,
            "length {}",
            o.length()
        );
        for s in (0..o.length() as usize).step_by(7) {
            let (x, y) = o.at(s as f32);
            assert!(
                (-0.01..=600.01).contains(&x) && (-0.01..=80.01).contains(&y),
                "inside the box at {s}: {x},{y}"
            );
        }
        // Clockwise: a fifth of the way round it is well along the top edge.
        let (x, y) = o.at(o.length() * 0.2);
        assert!(x > 100.0 && y < 1.0, "clockwise ({x},{y})");
    }

    #[test]
    fn a_span_keeps_its_ends_on_the_line() {
        let o = a_box();
        let pts = o.span(100.0, 700.0);
        assert_eq!(pts.first().copied(), Some(o.at(100.0)));
        assert_eq!(pts.last().copied(), Some(o.at(700.0)));
        assert!(pts.len() > 10, "the corner between kept");
    }

    #[test]
    fn the_pass_waits_runs_once_and_fades_in_and_out() {
        assert_eq!(
            frame(0.0).map(|f| f.fade),
            Some(0.0),
            "waiting: nothing shows"
        );
        let start = frame(DELAY_MS).expect("starts");
        assert_eq!((start.head, start.fade, start.landing), (0.0, 0.0, 0.0));
        // The landing: dark until the last quarter, lit in its middle, gone at the end.
        assert_eq!(frame(DELAY_MS + 600.0).map(|f| f.landing), Some(0.0));
        let lit = frame(DELAY_MS + LAP_MS * 0.875).expect("landing");
        assert!(lit.landing > 0.99, "{lit:?}");
        let end = frame(DELAY_MS + LAP_MS).expect("the last frame");
        assert!(end.landing < 0.01, "{end:?}");
        let mid = frame(DELAY_MS + 600.0).expect("runs");
        assert!(mid.fade > 0.99 && mid.head > 0.8, "{mid:?}");
        let near = frame(DELAY_MS + LAP_MS - 1.0).expect("still on");
        assert!(
            near.fade < 0.02 && near.head > 0.99,
            "gone as it arrives {near:?}"
        );
        assert_eq!(frame(DELAY_MS + LAP_MS + 1.0), None, "over: nothing drawn");
        // The head only moves forward.
        let heads: Vec<f32> = (0..=12)
            .filter_map(|k| frame(DELAY_MS + k as f32 * 100.0))
            .map(|f| f.head)
            .collect();
        assert!(heads.windows(2).all(|w| w[0] <= w[1]), "{heads:?}");
    }

    #[test]
    fn the_tail_fades_into_the_rim() {
        let accent = Rgb {
            r: 203,
            g: 166,
            b: 247,
        };
        let glow = Rgb {
            r: 255,
            g: 255,
            b: 255,
        };
        for dark in [true, false] {
            assert_eq!(ink(0.0, dark, accent, glow).1, 1.0, "the head whole");
            assert_eq!(ink(1.0, dark, accent, glow).1, 0.0, "its end gone");
            let alphas: Vec<f32> = (0..=10)
                .map(|k| ink(k as f32 / 10.0, dark, accent, glow).1)
                .collect();
            assert!(alphas.windows(2).all(|w| w[0] >= w[1]), "fading {alphas:?}");
        }
    }
}
