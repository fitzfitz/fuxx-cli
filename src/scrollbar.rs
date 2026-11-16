//! M11ae: the overlay scrollbar of a file tab (File, Diff and, from Task 4, Preview) — a thumb
//! with no track, shown while the text scrolls or the pointer is near its edge, faded out 1.0 s
//! after the last of those (note 80 §2).
//!
//! `thumb`, `offset_for` and `alpha` are pure and tested; `Bar` is the state a view keeps (when
//! it was last active, hovered, dragged, fading); `paint` draws a thumb.

use std::time::{Duration, Instant};

use alacritty_terminal::vte::ansi::Rgb;
use gpui::{Bounds, Pixels, Window, fill, point, px, size};

use crate::terminal_view::hsla;

/// The thumb's width (6 pt), corner radius, inset from the text's edge, and least length.
const WIDTH: f32 = 6.0;
const RADIUS: f32 = 3.0;
const INSET: f32 = 3.0;
const MIN_LEN: f32 = 24.0;
/// How near the text's right (or bottom) edge the pointer shows the bar and a press works it.
pub const ZONE: f32 = 12.0;
/// The thumb's opacity when shown, and when hovered or dragged.
const SHOWN: f32 = 0.35;
const HOT: f32 = 0.55;
/// Shown this long after the last activity, then faded out over `FADE`.
const HOLD: Duration = Duration::from_millis(1000);
const FADE: Duration = Duration::from_millis(200);
/// The fade task's step.
pub const STEP: Duration = Duration::from_millis(16);

/// Which way a thumb runs: at the text's right edge, or along its bottom (Wrap off).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    Vertical,
    Horizontal,
}

/// A thumb along its axis, from the track's start, in points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Thumb {
    pub start: f32,
    pub len: f32,
}

/// The thumb for a `viewport` long view of `content`, scrolled `offset` in: none when everything
/// fits. Its length is the visible share (at least `MIN_LEN`, at most the track); its start maps
/// the scroll `0..=content - viewport` onto `0..=viewport - len`.
pub fn thumb(viewport: f32, content: f32, offset: f32) -> Option<Thumb> {
    if viewport <= 0.0 || content <= viewport {
        return None;
    }
    let len = (viewport * viewport / content).max(MIN_LEN).min(viewport);
    let max = content - viewport;
    let start = offset.clamp(0.0, max) / max * (viewport - len);
    Some(Thumb { start, len })
}

/// The scroll that puts the thumb's start at `thumb_start` (the inverse of `thumb`), kept within
/// the content.
pub fn offset_for(thumb_start: f32, viewport: f32, content: f32) -> f32 {
    let Some(t) = thumb(viewport, content, 0.0) else {
        return 0.0;
    };
    let room = viewport - t.len;
    if room <= 0.0 {
        return 0.0;
    }
    (thumb_start / room).clamp(0.0, 1.0) * (content - viewport)
}

/// The thumb's opacity `since_active` after the last scroll or hover; `hot` (hovered or dragged)
/// holds it at its strongest. Else `SHOWN` until `HOLD`, then linear to 0 over `FADE`.
pub fn alpha(since_active: Duration, hot: bool) -> f32 {
    if hot {
        return HOT;
    }
    let Some(fading) = since_active.checked_sub(HOLD) else {
        return SHOWN;
    };
    SHOWN * (1.0 - fading.as_secs_f32() / FADE.as_secs_f32()).max(0.0)
}

/// A view's scrollbar: when it was last active, whether the pointer is in its edge zone, the
/// thumb being dragged (its axis and where along the thumb it was grabbed), and whether a fade
/// task runs.
#[derive(Clone, Copy, Debug, Default)]
pub struct Bar {
    active_at: Option<Instant>,
    hot: bool,
    drag: Option<(Axis, f32)>,
    animating: bool,
}

impl Bar {
    /// Activity (a scroll, the pointer coming to or leaving the edge, a drag's end): shown, for
    /// `HOLD` from `now`.
    pub fn poke(&mut self, now: Instant) {
        self.active_at = Some(now);
    }

    pub fn is_shown(&self, now: Instant) -> bool {
        self.alpha_at(now) > 0.0
    }

    /// The thumb's opacity at `now`.
    pub fn alpha_at(&self, now: Instant) -> f32 {
        let hot = self.hot || self.drag.is_some();
        match self.active_at {
            Some(at) => alpha(now.saturating_duration_since(at), hot),
            None if hot => HOT,
            None => 0.0,
        }
    }

    /// The pointer in (or out of) the edge zone; whether that changed.
    pub fn set_hot(&mut self, hot: bool) -> bool {
        std::mem::replace(&mut self.hot, hot) != hot
    }

    /// A press on the thumb (or the jump to it) along `axis`, `grab` points from its start.
    pub fn start_drag(&mut self, axis: Axis, grab: f32) {
        self.drag = Some((axis, grab));
    }

    /// The release: the drag ends, active from `now`. Whether one was going.
    pub fn end_drag(&mut self, now: Instant) -> bool {
        let was = self.drag.take().is_some();
        if was {
            self.poke(now);
        }
        was
    }

    pub fn drag(&self) -> Option<(Axis, f32)> {
        self.drag
    }

    /// The view went off screen (its listeners with it, so no release or move will come): the
    /// drag and the hover end and nothing shows, with no poke (a hidden view isn't drawn; shown
    /// again, the thumb waits for the next activity). Whether a drag was going.
    pub fn hide(&mut self) -> bool {
        self.hot = false;
        self.active_at = None;
        self.drag.take().is_some()
    }

    #[cfg(feature = "ui-shots")]
    pub fn animating(&self) -> bool {
        self.animating
    }

    /// Whether a fade task should start: none runs (it then counts as running).
    pub fn begin_fade(&mut self) -> bool {
        !std::mem::replace(&mut self.animating, true)
    }

    /// One step of the fade task at `now`: whether to redraw (only while fading, and once more
    /// at 0 to clear it), and whether the task goes on. It ends once faded out, or while hot or
    /// dragged (held at full strength with no frames; the next poke starts another).
    pub fn fade_step(&mut self, now: Instant) -> (bool, bool) {
        let hot = self.hot || self.drag.is_some();
        let since = self
            .active_at
            .map_or(Duration::MAX, |at| now.saturating_duration_since(at));
        let redraw = !hot && since >= HOLD;
        self.animating = !hot && alpha(since, false) > 0.0;
        (redraw, self.animating)
    }
}

/// Where thumb `t` along `axis` sits in `area` (the track's area: its length along `axis` is the
/// viewport): `WIDTH` across, `INSET` in from the area's right (or bottom) edge.
pub fn thumb_bounds(area: Bounds<Pixels>, axis: Axis, t: &Thumb) -> Bounds<Pixels> {
    match axis {
        Axis::Vertical => Bounds::new(
            point(area.right() - px(INSET + WIDTH), area.top() + px(t.start)),
            size(px(WIDTH), px(t.len)),
        ),
        Axis::Horizontal => Bounds::new(
            point(area.left() + px(t.start), area.bottom() - px(INSET + WIDTH)),
            size(px(t.len), px(WIDTH)),
        ),
    }
}

/// Draw thumb `t` along `axis` of `area` (as `thumb_bounds`) in `color` at `alpha`, its corners
/// rounded.
pub fn paint(
    window: &mut Window,
    area: Bounds<Pixels>,
    axis: Axis,
    t: &Thumb,
    alpha: f32,
    color: Rgb,
) {
    let quad = fill(thumb_bounds(area, axis, t), hsla(color).opacity(alpha));
    window.paint_quad(quad.corner_radii(px(RADIUS)));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(secs: f32) -> Duration {
        Duration::from_secs_f32(secs)
    }

    #[test]
    fn nothing_when_everything_fits() {
        assert_eq!(thumb(100.0, 100.0, 0.0), None);
        assert_eq!(thumb(100.0, 60.0, 0.0), None);
        assert_eq!(thumb(0.0, 500.0, 0.0), None);
    }

    #[test]
    fn the_thumb_is_the_visible_share_placed_by_the_scroll() {
        assert_eq!(
            thumb(100.0, 400.0, 0.0),
            Some(Thumb {
                start: 0.0,
                len: 25.0
            })
        );
        assert_eq!(thumb(100.0, 400.0, 300.0).map(|t| t.start), Some(75.0));
        // Past either end: kept on the track.
        assert_eq!(thumb(100.0, 400.0, 900.0).map(|t| t.start), Some(75.0));
        assert_eq!(thumb(100.0, 400.0, -10.0).map(|t| t.start), Some(0.0));
    }

    #[test]
    fn a_long_text_keeps_a_thumb_you_can_grab() {
        assert_eq!(thumb(100.0, 100_000.0, 0.0).map(|t| t.len), Some(24.0));
        // Never longer than the track.
        assert_eq!(
            thumb(20.0, 30.0, 5.0).map(|t| (t.start, t.len)),
            Some((0.0, 20.0))
        );
    }

    #[test]
    fn offset_for_is_the_inverse_kept_within_the_text() {
        assert_eq!(offset_for(75.0, 100.0, 400.0), 300.0);
        assert_eq!(offset_for(37.5, 100.0, 400.0), 150.0);
        assert_eq!(offset_for(-5.0, 100.0, 400.0), 0.0);
        assert_eq!(offset_for(999.0, 100.0, 400.0), 300.0);
        // Nothing to scroll, or no room to move the thumb: the top.
        assert_eq!(offset_for(10.0, 100.0, 80.0), 0.0);
        assert_eq!(offset_for(10.0, 20.0, 30.0), 0.0);
    }

    #[test]
    fn alpha_holds_then_fades_in_200_ms() {
        assert_eq!(alpha(s(0.0), false), 0.35);
        assert_eq!(alpha(s(0.5), false), 0.35);
        assert!((alpha(s(1.1), false) - 0.175).abs() < 1e-4);
        assert_eq!(alpha(s(1.2), false), 0.0);
        assert_eq!(alpha(s(1.3), false), 0.0);
        assert_eq!(alpha(s(5.0), true), 0.55);
    }

    #[test]
    fn a_bar_shows_after_a_poke_until_faded() {
        let t0 = Instant::now();
        let mut bar = Bar::default();
        assert!(!bar.is_shown(t0), "never active");
        bar.poke(t0);
        assert!(bar.is_shown(t0 + s(1.1)));
        assert!(!bar.is_shown(t0 + s(1.3)));
        // A later poke starts the second again.
        bar.poke(t0 + s(1.0));
        assert!(bar.is_shown(t0 + s(2.1)));
    }

    #[test]
    fn hovered_or_dragged_shows_at_its_strongest() {
        let t0 = Instant::now();
        let mut bar = Bar::default();
        assert!(bar.set_hot(true), "changed");
        assert!(!bar.set_hot(true), "unchanged");
        assert_eq!(bar.alpha_at(t0), 0.55);
        bar.set_hot(false);
        assert_eq!(bar.alpha_at(t0), 0.0);
        bar.start_drag(Axis::Vertical, 4.0);
        assert_eq!(bar.drag(), Some((Axis::Vertical, 4.0)));
        assert_eq!(bar.alpha_at(t0 + s(9.0)), 0.55);
        assert!(bar.end_drag(t0), "was dragging");
        assert!(!bar.end_drag(t0), "no drag");
        // Released: active from then, so it fades as after a scroll.
        assert_eq!(bar.alpha_at(t0 + s(0.5)), 0.35);
        assert!(!bar.is_shown(t0 + s(1.3)));
    }

    #[test]
    fn hidden_ends_the_drag_and_the_hover_and_shows_nothing() {
        let t0 = Instant::now();
        let mut bar = Bar::default();
        bar.poke(t0);
        bar.set_hot(true);
        bar.start_drag(Axis::Vertical, 4.0);
        assert!(bar.hide(), "was dragging");
        assert_eq!(bar.drag(), None);
        assert!(!bar.is_shown(t0), "nothing shows");
        assert!(!bar.set_hot(false), "not hot any more");
        assert!(!bar.hide(), "no drag");
        // A fade task still running draws once more, then ends.
        assert!(bar.begin_fade());
        assert_eq!(bar.fade_step(t0), (true, false));
    }

    #[test]
    fn the_fade_task_redraws_only_while_fading_and_ends_at_zero() {
        let t0 = Instant::now();
        let mut bar = Bar::default();
        bar.poke(t0);
        assert!(bar.begin_fade(), "none running");
        assert!(!bar.begin_fade(), "one runs already");
        // Holding: no frames needed.
        assert_eq!(bar.fade_step(t0 + s(0.5)), (false, true));
        // Fading: a frame each step.
        assert_eq!(bar.fade_step(t0 + s(1.1)), (true, true));
        // Gone: a last frame clears it, and the task ends.
        assert_eq!(bar.fade_step(t0 + s(1.3)), (true, false));
        assert!(bar.begin_fade(), "ended, so a poke starts another");
        // Hot: held at full strength with no frames; the task ends.
        bar.set_hot(true);
        assert_eq!(bar.fade_step(t0 + s(1.4)), (false, false));
        assert!(bar.begin_fade());
    }
}
