/// OSC 9 desktop-notification intro: ESC ] 9 ;
const TARGET: [u8; 4] = [0x1b, 0x5d, 0x39, 0x3b];

pub struct OscDetector {
    /// How many leading bytes of TARGET have matched so far (0..TARGET.len()).
    matched: usize,
}

impl OscDetector {
    pub fn new() -> Self {
        OscDetector { matched: 0 }
    }

    pub fn feed(&mut self, bytes: &[u8]) -> usize {
        let mut detections = 0;
        for &b in bytes {
            if b == TARGET[self.matched] {
                self.matched += 1;
                if self.matched == TARGET.len() {
                    detections += 1;
                    self.matched = 0; // ready for the next sequence
                }
            } else if b == TARGET[0] {
                // The byte that broke the match is itself ESC: begin a fresh match from it,
                // so e.g. "ESC ESC ]9;" is still detected.
                self.matched = 1;
            } else {
                self.matched = 0;
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
    fn does_not_detect_osc_99() {
        let mut d = OscDetector::new();
        assert_eq!(d.feed(b"\x1b]99;progress\x07"), 0);
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
}
