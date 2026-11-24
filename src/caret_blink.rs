//! A text field's caret blinking (the 2026-11-24 chores: "just usual blinking like its text
//! input"): solid for `FLIP` after any typing or caret move, then off and on every `FLIP`,
//! solid again (and no more timers) once `IDLE_STOP` has passed without input. Not focused: no
//! caret. In the background, or with the cursor-glow setting off: solid, no timer. Pure: the
//! field asks `next_flip` how long to sleep, then redraws once.

use std::time::{Duration, Instant};

/// Each on or off stretch.
pub const FLIP: Duration = Duration::from_millis(500);
/// After this long without input the caret stays solid and the timer stops.
pub const IDLE_STOP: Duration = Duration::from_secs(20);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CaretBlink {
    /// The last typing or caret move.
    pub last_input: Instant,
    pub focused: bool,
    /// The window is in front.
    pub window_active: bool,
    /// The cursor-glow setting (off: a solid caret).
    pub enabled: bool,
}

impl CaretBlink {
    pub fn new(now: Instant) -> Self {
        Self {
            last_input: now,
            focused: false,
            window_active: true,
            enabled: true,
        }
    }

    /// The caret is drawn `now`.
    pub fn shown(&self, now: Instant) -> bool {
        if !self.focused {
            return false;
        }
        match self.blinking_for(now) {
            Some(since) => (since.as_millis() / FLIP.as_millis()) % 2 == 0,
            None => true,
        }
    }

    /// How long until the caret next flips; none when it never will again (unfocused, solid,
    /// or idle past `IDLE_STOP`).
    pub fn next_flip(&self, now: Instant) -> Option<Duration> {
        let since = self.blinking_for(now)?;
        let into = Duration::from_millis((since.as_millis() % FLIP.as_millis()) as u64);
        Some((FLIP - into).min(IDLE_STOP - since))
    }

    /// How long since the last input, while the caret blinks (focused, in front, on, and not
    /// yet idle past `IDLE_STOP`).
    fn blinking_for(&self, now: Instant) -> Option<Duration> {
        let since = now.saturating_duration_since(self.last_input);
        (self.focused && self.window_active && self.enabled && since < IDLE_STOP).then_some(since)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn focused(at: Instant) -> CaretBlink {
        CaretBlink {
            focused: true,
            ..CaretBlink::new(at)
        }
    }

    #[test]
    fn solid_while_typing() {
        let t0 = Instant::now();
        let b = focused(t0);
        assert!(b.shown(t0));
        assert!(b.shown(t0 + Duration::from_millis(499)));
    }

    #[test]
    fn flips_every_half_second() {
        let t0 = Instant::now();
        let b = focused(t0);
        assert!(!b.shown(t0 + Duration::from_millis(600)));
        assert!(b.shown(t0 + Duration::from_millis(1100)));
        assert!(!b.shown(t0 + Duration::from_millis(1600)));
        assert_eq!(
            b.next_flip(t0 + Duration::from_millis(600)),
            Some(Duration::from_millis(400))
        );
    }

    #[test]
    fn stops_after_twenty_idle_seconds() {
        let t0 = Instant::now();
        let b = focused(t0);
        let late = t0 + IDLE_STOP + Duration::from_millis(700);
        assert!(b.shown(late), "solid once idle");
        assert_eq!(b.next_flip(late), None);
        // The last flip lands on the stop, not past it.
        let near = t0 + IDLE_STOP - Duration::from_millis(100);
        assert_eq!(b.next_flip(near), Some(Duration::from_millis(100)));
    }

    #[test]
    fn unfocused_has_no_caret_and_no_timer() {
        let t0 = Instant::now();
        let b = CaretBlink::new(t0);
        assert!(!b.shown(t0));
        assert_eq!(b.next_flip(t0), None);
    }

    #[test]
    fn off_or_in_the_background_is_solid() {
        let t0 = Instant::now();
        let later = t0 + Duration::from_millis(600);
        for b in [
            CaretBlink {
                enabled: false,
                ..focused(t0)
            },
            CaretBlink {
                window_active: false,
                ..focused(t0)
            },
        ] {
            assert!(b.shown(later));
            assert_eq!(b.next_flip(later), None);
        }
    }
}
