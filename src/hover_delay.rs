//! A panel that opens when the pointer rests on its anchor (the 2026-11-24 chores: the usage
//! gauge and the clock). It opens `OPEN_AFTER` after the pointer arrives, if it is still there;
//! it stays open while the pointer is on the anchor or the panel, and closes `CLOSE_GRACE` after
//! it leaves both (the grace lets the pointer cross from one to the other). Pure: the view waits
//! on a timer and calls `settle` with the ticket it was given, so a stale timer does nothing.

use std::time::Duration;

/// How long the pointer rests on the anchor before the panel opens (your words: "the delay is
/// 0.8 second").
pub const OPEN_AFTER: Duration = Duration::from_millis(800);
/// How long the panel stays after the pointer leaves both parts, so it can cross between them.
pub const CLOSE_GRACE: Duration = Duration::from_millis(150);

/// The two places the pointer can be over.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    Anchor,
    Panel,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HoverDelay {
    anchor: bool,
    panel: bool,
    open: bool,
    /// Bumped whenever the pointer arrives or leaves both parts: a timer holding an older one
    /// is stale.
    ticket: u64,
}

impl HoverDelay {
    pub fn is_open(&self) -> bool {
        self.open
    }

    fn over(&self) -> bool {
        self.anchor || self.panel
    }

    fn set(&mut self, part: Part, on: bool) {
        match part {
            Part::Anchor => self.anchor = on,
            Part::Panel => self.panel = on,
        }
    }

    /// The pointer arrived on `part`. A ticket to `settle` after `OPEN_AFTER` when the panel
    /// should open then; none when nothing is to wait for (already over, or already open — an
    /// arrival during the close grace keeps it open).
    pub fn enter(&mut self, part: Part) -> Option<u64> {
        let was_over = self.over();
        self.set(part, true);
        if was_over {
            return None;
        }
        self.ticket += 1;
        (!self.open).then_some(self.ticket)
    }

    /// The pointer left `part`. A ticket to `settle` after `CLOSE_GRACE` when the panel is open
    /// and the pointer is now over neither part.
    pub fn leave(&mut self, part: Part) -> Option<u64> {
        self.set(part, false);
        if self.over() {
            return None;
        }
        self.ticket += 1;
        self.open.then_some(self.ticket)
    }

    /// A timer holding `ticket` ran out: open or close as the pointer now says. True when the
    /// panel changed (redraw).
    pub fn settle(&mut self, ticket: u64) -> bool {
        if ticket != self.ticket || self.open == self.over() {
            return false;
        }
        self.open = self.over();
        true
    }

    /// Closed at once (a click elsewhere, the box losing its menu).
    pub fn close(&mut self) {
        *self = Self {
            ticket: self.ticket + 1,
            ..Self::default()
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opens_after_a_rest() {
        let mut h = HoverDelay::default();
        let t = h.enter(Part::Anchor).expect("a wait");
        assert!(!h.is_open());
        assert!(h.settle(t));
        assert!(h.is_open());
    }

    #[test]
    fn leaving_before_the_delay_opens_nothing() {
        let mut h = HoverDelay::default();
        let t = h.enter(Part::Anchor).unwrap();
        assert_eq!(h.leave(Part::Anchor), None);
        assert!(!h.settle(t));
        assert!(!h.is_open());
    }

    #[test]
    fn coming_back_restarts_the_wait() {
        let mut h = HoverDelay::default();
        let first = h.enter(Part::Anchor).unwrap();
        h.leave(Part::Anchor);
        let second = h.enter(Part::Anchor).unwrap();
        assert!(!h.settle(first), "the first timer is stale");
        assert!(!h.is_open());
        assert!(h.settle(second));
        assert!(h.is_open());
    }

    #[test]
    fn the_panel_keeps_it_open() {
        let mut h = HoverDelay::default();
        let t = h.enter(Part::Anchor).unwrap();
        h.settle(t);
        let closing = h.leave(Part::Anchor).expect("a grace before closing");
        assert_eq!(h.enter(Part::Panel), None);
        assert!(!h.settle(closing));
        assert!(h.is_open());
        let closing = h.leave(Part::Panel).unwrap();
        assert!(h.settle(closing));
        assert!(!h.is_open());
    }

    #[test]
    fn closed_while_over_waits_for_a_fresh_arrival() {
        let mut h = HoverDelay::default();
        let t = h.enter(Part::Anchor).unwrap();
        h.settle(t);
        h.close();
        assert!(!h.is_open());
        assert_eq!(h.leave(Part::Anchor), None, "already closed");
        let t = h.enter(Part::Anchor).expect("a fresh wait");
        assert!(h.settle(t));
    }

    #[test]
    fn close_drops_a_pending_open() {
        let mut h = HoverDelay::default();
        let t = h.enter(Part::Anchor).unwrap();
        h.close();
        assert!(!h.settle(t));
        assert!(!h.is_open());
    }
}
