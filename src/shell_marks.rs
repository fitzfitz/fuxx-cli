//! Reads shell-integration marks (OSC 133, the FinalTerm convention) from a terminal's raw
//! output (M11l). Neither `vte` nor `alacritty_terminal` handles OSC 133, so, like
//! `detector.rs` does for OSC 7 / 9 / 99 / 777, the bytes are scanned here, one at a time, so a
//! mark split across reads is still found.
//!
//! Read (each `ESC ]133;` + letter, ended by BEL or `ESC \`):
//! - `A` the prompt starts; `B` the prompt ends (typing starts);
//! - `C` output starts, with an optional `;cmdline_url=<%-encoded command>` (Kitty's key);
//! - `D` the command finished, with an optional `;<exit>`.
//!
//! Other letters and keys are ignored. A mark longer than `MARK_CAP` bytes keeps only its
//! letter (memory stays bounded): an overlong `C` still reads as output starting, without its
//! command (`truncated`); any other overlong mark is dropped.

use memchr::{memchr, memchr2};

/// The longest mark body (what follows `ESC ]133;`) kept whole; past it only the letter is.
pub const MARK_CAP: usize = 8192;

/// The intro. It starts with ESC and holds no other ESC, so a failed partial match can only
/// restart at an ESC.
const INTRO: &[u8] = b"\x1b]133;";
const CMDLINE_URL: &[u8] = b"cmdline_url=";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Mark {
    PromptStart,
    CommandStart,
    /// `cmd` is the decoded command line, if the mark had a valid one no longer than
    /// `blocks::CMD_CAP` bytes; `truncated` when it had one that was longer.
    OutputStart {
        cmd: Option<String>,
        truncated: bool,
    },
    Done {
        exit: Option<i32>,
    },
}

/// Finds the marks in a terminal's raw output, across reads.
pub struct MarkScanner {
    /// How many leading bytes of `INTRO` have matched so far.
    matched: usize,
    /// After the intro: the mark's body so far; none while not inside one.
    body: Option<Vec<u8>>,
    /// The body ran past `MARK_CAP`: `body` holds only its first two bytes (the letter and
    /// what follows it) and takes no more.
    overflow: bool,
    /// The body's last byte was an ESC, which may start its `ESC \` end.
    esc: bool,
}

impl MarkScanner {
    pub fn new() -> Self {
        MarkScanner {
            matched: 0,
            body: None,
            overflow: false,
            esc: false,
        }
    }

    /// The marks ended in `bytes`, each with the index just past its terminator. A mark split
    /// across calls is reported by the call that ends it.
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<(usize, Mark)> {
        let mut found = Vec::new();
        let mut i = 0;
        while i < bytes.len() {
            if self.at_rest() {
                match memchr(0x1b, &bytes[i..]) {
                    Some(skip) => i += skip,
                    None => break,
                }
            } else if self.in_plain_body() {
                // Inside a mark, up to the next BEL, ESC, CAN or SUB every byte is just appended.
                let rest = &bytes[i..];
                let mut span = memchr2(0x07, 0x1b, rest).unwrap_or(rest.len());
                if let Some(cancel) = memchr2(0x18, 0x1a, &rest[..span]) {
                    span = cancel;
                }
                if !self.overflow
                    && let Some(body) = self.body.as_mut()
                {
                    // One past the cap is enough to know the mark overflows.
                    let room = (MARK_CAP + 1).saturating_sub(body.len());
                    body.extend_from_slice(&rest[..span.min(room)]);
                    if body.len() > MARK_CAP {
                        body.truncate(2);
                        self.overflow = true;
                    }
                }
                if span > 0 {
                    i += span;
                    continue;
                }
            }
            let b = bytes[i];
            let at = i;
            i += 1;
            if let Some(mut body) = self.body.take() {
                let esc = std::mem::take(&mut self.esc);
                match (esc, b) {
                    (false, 0x07) | (true, b'\\') => {
                        let mark = if self.overflow {
                            overlong(&body)
                        } else {
                            parse(&body)
                        };
                        if let Some(mark) = mark {
                            found.push((at + 1, mark));
                        }
                    }
                    // An ESC that doesn't end it, or a CAN / SUB (which cancel any sequence
                    // in `vte`): something else began; this one is abandoned.
                    (true, _) | (false, 0x18 | 0x1a) => {}
                    (false, 0x1b) => {
                        self.esc = true;
                        self.body = Some(body);
                    }
                    (false, _) => {
                        if !self.overflow {
                            body.push(b);
                            if body.len() > MARK_CAP {
                                body.truncate(2);
                                self.overflow = true;
                            }
                        }
                        self.body = Some(body);
                    }
                }
            }
            if self.step_intro(b) {
                self.body = Some(Vec::new());
                self.overflow = false;
                self.esc = false;
            }
        }
        found
    }

    /// No intro partly matched and no mark open: a byte that is not ESC changes nothing.
    fn at_rest(&self) -> bool {
        self.body.is_none() && self.matched == 0
    }

    /// Inside a mark whose last byte was not an ESC, with no intro partly matched.
    fn in_plain_body(&self) -> bool {
        self.body.is_some() && !self.esc && self.matched == 0
    }

    /// Feeds one byte to the intro match; true when the whole intro has just matched.
    fn step_intro(&mut self, b: u8) -> bool {
        if b == INTRO[self.matched] {
            self.matched += 1;
            if self.matched == INTRO.len() {
                self.matched = 0;
                return true;
            }
        } else {
            // The byte that broke the match may itself be ESC: begin a fresh match from it.
            self.matched = usize::from(b == INTRO[0]);
        }
        false
    }
}

/// A mark's body (after `ESC ]133;`, before its terminator): the letter, then `;`-separated
/// options. Unknown letters, or a letter followed by anything but `;`, give none.
fn parse(body: &[u8]) -> Option<Mark> {
    let (&letter, rest) = body.split_first()?;
    let options = match rest {
        [] => &[][..],
        [b';', options @ ..] => options,
        _ => return None,
    };
    let mut options = options.split(|&b| b == b';').filter(|o| !o.is_empty());
    match letter {
        b'A' => Some(Mark::PromptStart),
        b'B' => Some(Mark::CommandStart),
        b'C' => {
            let url = options.find_map(|o| o.strip_prefix(CMDLINE_URL));
            let decoded = url.and_then(percent_decode).filter(|cmd| !cmd.is_empty());
            let truncated = decoded
                .as_ref()
                .is_some_and(|cmd| cmd.len() > crate::blocks::CMD_CAP);
            let cmd = decoded.filter(|_| !truncated);
            Some(Mark::OutputStart { cmd, truncated })
        }
        b'D' => {
            // The exit code is the first option, when it is a number.
            let exit = body
                .get(2..)
                .and_then(|o| o.split(|&b| b == b';').next())
                .and_then(|o| std::str::from_utf8(o).ok())
                .and_then(|o| o.parse::<i32>().ok());
            Some(Mark::Done { exit })
        }
        _ => None,
    }
}

/// A mark that ran past `MARK_CAP`, from its first two bytes: a `C` (with options) still
/// means output started, its command too long to keep; anything else is dropped.
fn overlong(start: &[u8]) -> Option<Mark> {
    (start == b"C;").then_some(Mark::OutputStart {
        cmd: None,
        truncated: true,
    })
}

/// `%XX`-decodes `encoded` (as `detector::cwd_from_report` does) into UTF-8 text; bad
/// encoding or text that isn't UTF-8 gives none.
pub fn percent_decode(encoded: &[u8]) -> Option<String> {
    let mut out = Vec::with_capacity(encoded.len());
    let mut bytes = encoded.iter();
    while let Some(&b) = bytes.next() {
        if b != b'%' {
            out.push(b);
            continue;
        }
        let hex = |b: Option<&u8>| (*b? as char).to_digit(16);
        out.push((hex(bytes.next())? * 16 + hex(bytes.next())?) as u8);
    }
    String::from_utf8(out).ok()
}

/// The verbatim byte-at-a-time `feed` from before M11n: the oracle the fast one is proven against.
#[cfg(test)]
mod bytewise {
    use super::*;

    impl MarkScanner {
        pub(super) fn feed_bytewise(&mut self, bytes: &[u8]) -> Vec<(usize, Mark)> {
            let mut found = Vec::new();
            for (i, &b) in bytes.iter().enumerate() {
                if let Some(mut body) = self.body.take() {
                    let esc = std::mem::take(&mut self.esc);
                    match (esc, b) {
                        (false, 0x07) | (true, b'\\') => {
                            let mark = if self.overflow {
                                overlong(&body)
                            } else {
                                parse(&body)
                            };
                            if let Some(mark) = mark {
                                found.push((i + 1, mark));
                            }
                        }
                        (true, _) | (false, 0x18 | 0x1a) => {}
                        (false, 0x1b) => {
                            self.esc = true;
                            self.body = Some(body);
                        }
                        (false, _) => {
                            if !self.overflow {
                                body.push(b);
                                if body.len() > MARK_CAP {
                                    body.truncate(2);
                                    self.overflow = true;
                                }
                            }
                            self.body = Some(body);
                        }
                    }
                }
                if self.step_intro(b) {
                    self.body = Some(Vec::new());
                    self.overflow = false;
                    self.esc = false;
                }
            }
            found
        }
    }
}

#[cfg(test)]
mod equivalence {
    use super::*;
    use crate::detector::fixtures::*;

    /// Marks with offsets made absolute, fed in `chunks`.
    fn run(chunks: &[&[u8]], fast: bool) -> Vec<(usize, Mark)> {
        let mut s = MarkScanner::new();
        let mut at = 0;
        let mut out = Vec::new();
        for c in chunks {
            let marks = if fast { s.feed(c) } else { s.feed_bytewise(c) };
            out.extend(marks.into_iter().map(|(i, m)| (at + i, m)));
            at += c.len();
        }
        out
    }

    #[test]
    fn fast_scanners_match_bytewise_at_every_split() {
        let _ = MarkScanner::new().at_rest();
        for seed in 1..=200u64 {
            let stream = random_stream(seed);
            let ones: Vec<&[u8]> = stream.chunks(1).collect();
            let want = run(&ones, false);
            let got = run(&random_chunks(seed, &stream), true);
            assert_eq!(got, want, "marks, seed {seed}");
        }
    }

    #[test]
    fn fast_scanners_match_bytewise_split_at_every_offset() {
        for (k, stream) in exhaustive_streams().iter().enumerate() {
            let ones: Vec<&[u8]> = stream.chunks(1).collect();
            let want = run(&ones, false);
            if k == 0 {
                assert!(want.len() >= 6, "the stream holds several marks");
            }
            for cut in 0..=stream.len() {
                let (a, b) = stream.split_at(cut);
                assert_eq!(run(&[a, b], true), want, "stream {k}, cut {cut}");
            }
        }
    }

    /// The limit fixtures sit where they claim (as the oracle has it): a `D` body of
    /// `MARK_CAP` bytes is read, one byte more is dropped whole (a long `D` too), and an
    /// overlong `C` still reads as output starting, without its command.
    #[test]
    fn the_mark_cap_fixtures_straddle_the_cap() {
        let marks = |s: &[u8]| -> Vec<Mark> {
            let ones: Vec<&[u8]> = s.chunks(1).collect();
            run(&ones, false).into_iter().map(|(_, m)| m).collect()
        };
        let done = vec![Mark::Done { exit: Some(0) }];
        let overlong_c = vec![Mark::OutputStart {
            cmd: None,
            truncated: true,
        }];
        let l = limits();
        let got: Vec<Vec<Mark>> = l[5..].iter().map(|s| marks(s)).collect();
        assert_eq!(got[..4], [done.clone(), done.clone(), done, Vec::new()]);
        assert_eq!(got[4].len(), 1, "a `C` at the cap is read: {:?}", got[4]);
        assert_eq!(got[5], overlong_c);
        assert_eq!(got[6], Vec::new(), "a long `D` is dropped, not read cut");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed(bytes: &[u8]) -> Vec<(usize, Mark)> {
        MarkScanner::new().feed(bytes)
    }

    fn marks(bytes: &[u8]) -> Vec<Mark> {
        feed(bytes).into_iter().map(|(_, m)| m).collect()
    }

    #[test]
    fn reads_the_four_marks() {
        assert_eq!(
            feed(b"\x1b]133;A\x07x\x1b]133;B\x1b\\"),
            vec![(8, Mark::PromptStart), (18, Mark::CommandStart)]
        );
        assert_eq!(
            marks(b"\x1b]133;C;cmdline_url=echo%20hi\x07"),
            vec![Mark::OutputStart {
                cmd: Some("echo hi".into()),
                truncated: false
            }]
        );
        assert_eq!(
            marks(b"\x1b]133;D;1\x07"),
            vec![Mark::Done { exit: Some(1) }]
        );
        assert_eq!(marks(b"\x1b]133;D\x07"), vec![Mark::Done { exit: None }]);
        // Other keys after the letter are ignored.
        assert_eq!(marks(b"\x1b]133;A;cl=m;aid=7\x07"), vec![Mark::PromptStart]);
        assert_eq!(
            marks(b"\x1b]133;C\x07"),
            vec![Mark::OutputStart {
                cmd: None,
                truncated: false
            }]
        );
        assert_eq!(
            marks(b"\x1b]133;D;-3;aid=7\x07"),
            vec![Mark::Done { exit: Some(-3) }]
        );
    }

    #[test]
    fn a_mark_split_at_every_byte_is_found_once() {
        let all: &[u8] = b"ab\x1b]133;C;cmdline_url=ls\x07cd";
        let end = all.len() - 2; // just past the BEL
        for split in 0..=all.len() {
            let mut scanner = MarkScanner::new();
            let first = scanner.feed(&all[..split]);
            let second = scanner.feed(&all[split..]);
            let want = Mark::OutputStart {
                cmd: Some("ls".into()),
                truncated: false,
            };
            if split >= end {
                assert_eq!(first, vec![(end, want)], "split {split}");
                assert!(second.is_empty(), "split {split}: {second:?}");
            } else {
                // Before the mark or inside it: the second feed ends it.
                assert!(first.is_empty(), "split {split}: {first:?}");
                assert_eq!(second, vec![(end - split, want)], "split {split}");
            }
        }
        // `ESC \` split between its two bytes.
        let mut scanner = MarkScanner::new();
        assert!(scanner.feed(b"\x1b]133;B\x1b").is_empty());
        assert_eq!(scanner.feed(b"\\z"), vec![(1, Mark::CommandStart)]);
    }

    #[test]
    fn ignores_other_letters_and_bad_input() {
        assert!(marks(b"\x1b]133;P\x07").is_empty());
        assert!(marks(b"\x1b]133;Z\x07").is_empty());
        assert!(marks(b"\x1b]1330;A\x07").is_empty());
        assert!(marks(b"\x1b]133;AB\x07").is_empty());
        assert!(marks(b"\x1b]133;\x07").is_empty());
        assert_eq!(marks(b"\x1b]133;D;x\x07"), vec![Mark::Done { exit: None }]);
        assert_eq!(
            marks(b"\x1b]133;C;cmdline_url=a%G1\x07"),
            vec![Mark::OutputStart {
                cmd: None,
                truncated: false
            }]
        );
        assert_eq!(
            marks(b"\x1b]133;C;cmdline_url=a%2\x07"),
            vec![Mark::OutputStart {
                cmd: None,
                truncated: false
            }]
        );
        // Not UTF-8 once decoded.
        assert_eq!(
            marks(b"\x1b]133;C;cmdline_url=%FF\x07"),
            vec![Mark::OutputStart {
                cmd: None,
                truncated: false
            }]
        );
        // An ESC inside a mark abandons it; one that begins a new mark still counts.
        assert!(marks(b"\x1b]133;A\x1b[0m\x07").is_empty());
        assert_eq!(
            feed(b"\x1b]133;A\x1b]133;B\x07"),
            vec![(15, Mark::CommandStart)]
        );
        // A BEL outside a mark is just a bell.
        assert!(marks(b"\x07\x1b\\x").is_empty());
    }

    #[test]
    fn drops_an_overlong_mark() {
        // An overlong C still says output started, without its command (fix round 1).
        let mut bytes = b"\x1b]133;C;cmdline_url=".to_vec();
        bytes.extend(std::iter::repeat_n(b'a', 9000));
        bytes.push(0x07);
        let mut scanner = MarkScanner::new();
        assert_eq!(
            scanner.feed(&bytes),
            vec![(
                bytes.len(),
                Mark::OutputStart {
                    cmd: None,
                    truncated: true
                }
            )]
        );
        // Split across reads, ended by `ESC \`.
        let mut scanner = MarkScanner::new();
        assert!(scanner.feed(&bytes[..9010]).is_empty());
        let mut rest = bytes[9010..bytes.len() - 1].to_vec();
        rest.extend_from_slice(b"\x1b\\");
        assert_eq!(
            scanner.feed(&rest),
            vec![(
                rest.len(),
                Mark::OutputStart {
                    cmd: None,
                    truncated: true
                }
            )]
        );
        // Any other overlong mark is dropped; the next mark is still found.
        let mut overlong_a = b"\x1b]133;A;".to_vec();
        overlong_a.extend(std::iter::repeat_n(b'k', 9000));
        overlong_a.push(0x07);
        assert!(scanner.feed(&overlong_a).is_empty());
        let mut not_c = b"\x1b]133;Cx".to_vec();
        not_c.extend(std::iter::repeat_n(b'k', 9000));
        not_c.push(0x07);
        assert!(scanner.feed(&not_c).is_empty());
        // An ESC still abandons an overlong mark.
        let mut abandoned = b"\x1b]133;C;".to_vec();
        abandoned.extend(std::iter::repeat_n(b'k', 9000));
        abandoned.extend_from_slice(b"\x1b[0m\x07");
        assert!(scanner.feed(&abandoned).is_empty());
        assert_eq!(
            scanner.feed(b"\x1b]133;A\x07"),
            vec![(8, Mark::PromptStart)]
        );
        // Exactly at the cap is kept.
        let mut at_cap = b"\x1b]133;D;0;".to_vec();
        at_cap.extend(std::iter::repeat_n(b'k', MARK_CAP - 4));
        at_cap.push(0x07);
        assert_eq!(marks(&at_cap), vec![Mark::Done { exit: Some(0) }]);
        let mut over = b"\x1b]133;D;0;".to_vec();
        over.extend(std::iter::repeat_n(b'k', MARK_CAP - 3));
        over.push(0x07);
        assert!(marks(&over).is_empty());
    }
}
