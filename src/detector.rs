//! Spots the "look at me" notifications agents send, in the raw PTY bytes. `alacritty_terminal`
//! ignores these OSC sequences, so they never reach the parser's events.
//!
//! Counted (each by its intro, so a sequence split across reads is still found):
//! - OSC 9 `ESC ]9;` (iTerm2 style), except `ESC ]9;4;`, which is a progress bar, not a notification
//! - OSC 99 `ESC ]99;` (kitty)
//! - OSC 777 `ESC ]777;notify;` (Ghostty / urxvt)

/// Each intro starts with ESC and contains no other ESC, so a failed partial match can only
/// restart at an ESC.
const OSC_9: &[u8] = b"\x1b]9;";
const OSC_99: &[u8] = b"\x1b]99;";
const OSC_777_NOTIFY: &[u8] = b"\x1b]777;notify;";
/// What follows `ESC ]9;` in a ConEmu-style progress bar (Claude Code sends these while working).
const PROGRESS: &[u8] = b"4;";

/// Matches one intro across calls.
struct Intro {
    target: &'static [u8],
    /// How many leading bytes of `target` have matched so far.
    matched: usize,
}

impl Intro {
    fn new(target: &'static [u8]) -> Self {
        Intro { target, matched: 0 }
    }

    /// Feeds one byte; true when the whole intro has just matched.
    fn step(&mut self, b: u8) -> bool {
        if b == self.target[self.matched] {
            self.matched += 1;
            if self.matched == self.target.len() {
                self.matched = 0; // ready for the next sequence
                return true;
            }
        } else {
            // The byte that broke the match may itself be ESC: begin a fresh match from it,
            // so e.g. "ESC ESC ]9;" is still detected.
            self.matched = usize::from(b == self.target[0]);
        }
        false
    }
}

pub struct OscDetector {
    osc9: Intro,
    osc99: Intro,
    osc777: Intro,
    /// Just after an OSC 9 intro: how many bytes of `PROGRESS` have followed it.
    after_osc9: Option<usize>,
}

impl OscDetector {
    pub fn new() -> Self {
        OscDetector {
            osc9: Intro::new(OSC_9),
            osc99: Intro::new(OSC_99),
            osc777: Intro::new(OSC_777_NOTIFY),
            after_osc9: None,
        }
    }

    /// Number of notifications whose intro completed in `bytes`. An OSC 9 is counted one or two
    /// bytes after its intro, once it's clear it isn't a progress bar.
    pub fn feed(&mut self, bytes: &[u8]) -> usize {
        let mut detections = 0;
        for &b in bytes {
            if let Some(seen) = self.after_osc9 {
                if b != PROGRESS[seen] {
                    detections += 1; // a real OSC 9 notification
                    self.after_osc9 = None;
                } else if seen + 1 == PROGRESS.len() {
                    self.after_osc9 = None; // `ESC ]9;4;`: a progress bar
                } else {
                    self.after_osc9 = Some(seen + 1);
                }
            }
            if self.osc9.step(b) {
                self.after_osc9 = Some(0);
            }
            if self.osc99.step(b) {
                detections += 1;
            }
            if self.osc777.step(b) {
                detections += 1;
            }
        }
        detections
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_full_intro_in_one_chunk() {
        let mut d = OscDetector::new();
        assert_eq!(d.feed(b"\x1b]9;hello\x07"), 1);
    }

    #[test]
    fn detects_intro_split_across_chunks() {
        let mut d = OscDetector::new();
        assert_eq!(d.feed(b"some output\x1b"), 0);
        assert_eq!(d.feed(b"]9;done\x07"), 1);
    }

    #[test]
    fn ignores_plain_text() {
        let mut d = OscDetector::new();
        assert_eq!(d.feed(b"hello world\n"), 0);
    }

    #[test]
    fn counts_two_intros_in_one_chunk() {
        let mut d = OscDetector::new();
        assert_eq!(d.feed(b"\x1b]9;a\x07\x1b]9;b\x07"), 2);
    }

    #[test]
    fn restarts_on_repeated_esc() {
        let mut d = OscDetector::new();
        // A stray ESC before the real intro must not swallow the match.
        assert_eq!(d.feed(b"\x1b\x1b]9;x\x07"), 1);
    }

    #[test]
    fn detects_kitty_osc_99() {
        let mut d = OscDetector::new();
        // Claude Code's kitty notification: title, body, then "done + focus".
        let kitty = b"\x1b]99;i=1:d=0:p=title;Claude Code\x1b\\\x1b]99;i=1:p=body;Done\x1b\\\x1b]99;i=1:d=1:a=focus;\x1b\\";
        assert_eq!(d.feed(kitty), 3);
    }

    #[test]
    fn detects_ghostty_osc_777_notify() {
        let mut d = OscDetector::new();
        assert_eq!(d.feed(b"\x1b]777;notify;Claude Code;Done\x07"), 1);
    }

    #[test]
    fn ignores_other_osc_777() {
        let mut d = OscDetector::new();
        assert_eq!(d.feed(b"\x1b]777;preexec\x07"), 0);
    }

    #[test]
    fn ignores_osc_9_progress_bar() {
        let mut d = OscDetector::new();
        assert_eq!(
            d.feed(b"\x1b]9;4;3;\x07\x1b]9;4;1;50\x07\x1b]9;4;0;\x07"),
            0
        );
    }

    #[test]
    fn ignores_progress_bar_split_across_chunks() {
        let mut d = OscDetector::new();
        assert_eq!(d.feed(b"\x1b]9;"), 0);
        assert_eq!(d.feed(b"4"), 0);
        assert_eq!(d.feed(b";1;50\x07"), 0);
    }

    #[test]
    fn osc_9_message_starting_with_4_still_counts() {
        let mut d = OscDetector::new();
        assert_eq!(d.feed(b"\x1b]9;42 tests passed\x07"), 1);
    }

    #[test]
    fn osc_9_is_counted_in_the_chunk_after_its_intro() {
        let mut d = OscDetector::new();
        assert_eq!(d.feed(b"\x1b]9;"), 0);
        assert_eq!(d.feed(b"done\x07"), 1);
    }
}
