//! The gap a drag opens (M11b Task 7b): a dragged pane tab or sidebar row leaves its own place
//! empty (the copy under the pointer *is* it), and the others slide aside to open a gap as big
//! as it where a drop would put it, as in Ghostty; there is no insertion line.
//!
//! `landing_slot`, `gap_offsets` and `slide_at` are the pure rules, along one line (a bar's x, the
//! sidebar's y); `Slides` keeps each item's slide between frames.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::layout_model::{TabId, WorkspaceId};

/// How long an item takes to slide to its new place.
pub const SLIDE: Duration = Duration::from_millis(150);

/// The slot a dragged item lands in among the others, from its copy's `centre`: how many of the
/// others have their centre before it. `spans` are every item's `(start, end)` along the line as
/// laid out (scrolled), the dragged one at `own` among them (none: it comes from elsewhere); the
/// others are taken as closed up, those after it moved back by `size` (the room it takes), so a
/// pointer resting on a boundary decides the same way every frame. NaN: the first slot.
pub fn landing_slot(centre: f32, spans: &[(f32, f32)], own: Option<usize>, size: f32) -> usize {
    spans
        .iter()
        .enumerate()
        .filter(|(i, _)| Some(*i) != own)
        .map(|(i, (start, end))| {
            let back = if own.is_some_and(|own| i > own) {
                size
            } else {
                0.0
            };
            (start + end) / 2.0 - back
        })
        .take_while(|middle| *middle < centre)
        .count()
}

/// How far each of `count` items is drawn from its place along the line, with the dragged one
/// at `own` among them (none: it comes from elsewhere) and the gap at slot `gap` among the others
/// (none: no gap here, so the others close up over its place). `size`: the gap's size.
pub fn gap_offsets(count: usize, own: Option<usize>, gap: Option<usize>, size: f32) -> Vec<f32> {
    (0..count)
        .map(|i| {
            if Some(i) == own {
                return 0.0; // it isn't drawn: its copy is under the pointer
            }
            // Its place among the others, and how far its own place is from that (closed up).
            let (slot, closed) = match own {
                Some(own) if i > own => (i - 1, -size),
                _ => (i, 0.0),
            };
            let gap_opened = gap.is_some_and(|gap| slot >= gap);
            closed + if gap_opened { size } else { 0.0 }
        })
        .collect()
}

/// Where an item sliding from `from` to `to` is `elapsed` into its slide: easing out over
/// `SLIDE` (fast at first, settling at the end).
pub fn slide_at(from: f32, to: f32, elapsed: Duration) -> f32 {
    let t = (elapsed.as_secs_f32() / SLIDE.as_secs_f32()).clamp(0.0, 1.0);
    let eased = 1.0 - (1.0 - t).powi(3);
    from + (to - from) * eased
}

/// What slides: a pane tab or a sidebar row.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Slid {
    Tab(TabId),
    Row(WorkspaceId),
}

/// One item's slide: from where, to where, since when.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Slide {
    from: f32,
    to: f32,
    start: Instant,
}

/// Each item's slide while a drag is on (dropped with the drag: by then the real layout matches
/// the gap).
#[derive(Debug, Default)]
pub struct Slides {
    items: HashMap<Slid, Slide>,
}

impl Slides {
    /// Where to draw `key` at `now`, on its way to `target`: a new target starts a slide from
    /// where it is now (no jump); `animate` off, it is there at once.
    pub fn offset(&mut self, key: Slid, target: f32, now: Instant, animate: bool) -> f32 {
        if !animate {
            self.items.remove(&key);
            return target;
        }
        match self.items.get_mut(&key) {
            Some(slide) => {
                let at = slide_at(
                    slide.from,
                    slide.to,
                    now.saturating_duration_since(slide.start),
                );
                if slide.to != target {
                    *slide = Slide {
                        from: at,
                        to: target,
                        start: now,
                    };
                }
                at
            }
            None if target == 0.0 => 0.0,
            None => {
                let slide = Slide {
                    from: 0.0,
                    to: target,
                    start: now,
                };
                self.items.insert(key, slide);
                0.0
            }
        }
    }

    /// Whether any item is still on its way at `now` (so another frame is wanted).
    pub fn moving(&self, now: Instant) -> bool {
        self.items
            .values()
            .any(|s| s.from != s.to && now.saturating_duration_since(s.start) < SLIDE)
    }

    /// Forget every slide (the drag ended).
    pub fn clear(&mut self) {
        self.items.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MS: fn(u64) -> Duration = Duration::from_millis;

    #[test]
    fn the_landing_slot_counts_the_closed_up_centres_before_the_copy_s() {
        // Four 100 px tabs from x = 240, the third dragged: closed up, the others' centres are
        // 290, 390 and (the fourth, moved back) 490.
        let tabs = [
            (240.0, 340.0),
            (340.0, 440.0),
            (440.0, 540.0),
            (540.0, 640.0),
        ];
        let own = Some(2);
        // First, middle, end.
        assert_eq!(landing_slot(250.0, &tabs, own, 100.0), 0);
        assert_eq!(landing_slot(289.0, &tabs, own, 100.0), 0);
        assert_eq!(landing_slot(291.0, &tabs, own, 100.0), 1);
        assert_eq!(landing_slot(391.0, &tabs, own, 100.0), 2);
        // The fourth counts where it would be with the dragged one out (490), not at 590.
        assert_eq!(landing_slot(489.0, &tabs, own, 100.0), 2);
        assert_eq!(landing_slot(491.0, &tabs, own, 100.0), 3);
        assert_eq!(landing_slot(900.0, &tabs, own, 100.0), 3);
        // From elsewhere: every tab counts where it is.
        assert_eq!(landing_slot(489.0, &tabs, None, 100.0), 2);
        assert_eq!(landing_slot(491.0, &tabs, None, 100.0), 3);
        assert_eq!(landing_slot(591.0, &tabs, None, 100.0), 4);
        // A bar scrolled 150 px left: the spans move with it.
        let scrolled = tabs.map(|(l, r)| (l - 150.0, r - 150.0));
        assert_eq!(landing_slot(250.0, &scrolled, own, 100.0), 2);
        // An empty bar, and NaN: the first slot.
        assert_eq!(landing_slot(300.0, &[], None, 100.0), 0);
        assert_eq!(landing_slot(f32::NAN, &tabs, own, 100.0), 0);
        // Its own span alone (nothing else in the bar): the only slot.
        assert_eq!(landing_slot(900.0, &tabs[..1], Some(0), 100.0), 0);
    }

    #[test]
    fn the_gap_moves_along_its_own_line_and_the_others_slide_aside() {
        // Dragged from 2 of 5 (100 px): at its own place, nothing moves.
        assert_eq!(gap_offsets(5, Some(2), Some(2), 100.0), [0.0; 5]);
        // To the front: the two before it slide right into its place.
        assert_eq!(
            gap_offsets(5, Some(2), Some(0), 100.0),
            [100.0, 100.0, 0.0, 0.0, 0.0]
        );
        assert_eq!(
            gap_offsets(5, Some(2), Some(1), 100.0),
            [0.0, 100.0, 0.0, 0.0, 0.0]
        );
        // Further on: the ones it passes slide left into its place.
        assert_eq!(
            gap_offsets(5, Some(2), Some(3), 100.0),
            [0.0, 0.0, 0.0, -100.0, 0.0]
        );
        assert_eq!(
            gap_offsets(5, Some(2), Some(4), 100.0),
            [0.0, 0.0, 0.0, -100.0, -100.0]
        );
    }

    #[test]
    fn with_the_gap_elsewhere_its_line_closes_up_and_another_opens_one() {
        // No gap on its own line: the ones after it close up.
        assert_eq!(
            gap_offsets(4, Some(1), None, 80.0),
            [0.0, 0.0, -80.0, -80.0]
        );
        // Another line, the gap at 1: the ones from there on make room.
        assert_eq!(gap_offsets(3, None, Some(1), 80.0), [0.0, 80.0, 80.0]);
        assert_eq!(gap_offsets(3, None, Some(3), 80.0), [0.0; 3]);
        // Neither: nothing moves.
        assert_eq!(gap_offsets(3, None, None, 80.0), [0.0; 3]);
        assert_eq!(gap_offsets(0, None, Some(0), 80.0), Vec::<f32>::new());
    }

    #[test]
    fn a_slide_eases_out_from_start_to_end() {
        assert_eq!(slide_at(0.0, 100.0, Duration::ZERO), 0.0);
        assert_eq!(slide_at(0.0, 100.0, SLIDE), 100.0);
        assert_eq!(slide_at(0.0, 100.0, MS(400)), 100.0);
        assert_eq!(slide_at(40.0, -60.0, SLIDE), -60.0);
        // Easing out: past half way at half the time.
        assert!(slide_at(0.0, 100.0, SLIDE / 2) > 50.0);
        // Never back, never past.
        let mut last = 0.0;
        for ms in 0..=160 {
            let at = slide_at(0.0, 100.0, MS(ms));
            assert!(at >= last && at <= 100.0, "{ms} ms: {at}");
            last = at;
        }
        let mut last = 0.0;
        for ms in 0..=160 {
            let at = slide_at(0.0, -100.0, MS(ms));
            assert!(at <= last && at >= -100.0, "{ms} ms: {at}");
            last = at;
        }
    }

    #[test]
    fn a_new_target_mid_slide_carries_on_from_where_it_is() {
        let key = Slid::Tab(TabId(1));
        let t0 = Instant::now();
        let mut slides = Slides::default();
        // At rest, nothing slides.
        assert_eq!(slides.offset(key, 0.0, t0, true), 0.0);
        assert!(!slides.moving(t0));
        // A gap opens: it starts where it was.
        assert_eq!(slides.offset(key, 100.0, t0, true), 0.0);
        assert!(slides.moving(t0));
        let mid = slides.offset(key, 100.0, t0 + MS(50), true);
        assert_eq!(mid, slide_at(0.0, 100.0, MS(50)));
        // Turned back half way: from there, no jump, then home.
        assert_eq!(slides.offset(key, 0.0, t0 + MS(50), true), mid);
        let back = slides.offset(key, 0.0, t0 + MS(80), true);
        assert_eq!(back, slide_at(mid, 0.0, MS(30)));
        assert!(back < mid && back > 0.0);
        assert_eq!(slides.offset(key, 0.0, t0 + MS(50) + SLIDE, true), 0.0);
        assert!(!slides.moving(t0 + MS(50) + SLIDE));
        // Without motion: there at once.
        let other = Slid::Row(WorkspaceId(7));
        assert_eq!(slides.offset(other, 42.0, t0, false), 42.0);
        assert!(!slides.moving(t0 + MS(50) + SLIDE));
        // The drag ended: every slide goes.
        slides.offset(key, 100.0, t0, true);
        slides.clear();
        assert_eq!(slides.offset(key, 0.0, t0 + MS(10), true), 0.0);
    }
}
