//! Spots the "look at me" notifications agents send, in the raw PTY bytes. `alacritty_terminal`
//! ignores these OSC sequences, so they never reach the parser's events.
//!
//! Counted (each by its intro, so a sequence split across reads is still found):
//! - OSC 9 `ESC ]9;` (iTerm2 style), except `ESC ]9;4;`, which is a progress bar, not a notification
//! - OSC 99 `ESC ]99;` (kitty)
//! - OSC 777 `ESC ]777;notify;` (Ghostty / urxvt)
//!
//! `CwdReports` reads OSC 7 (`ESC ]7;file://host/path`), the working folder a shell reports
//! after each `cd` when it's set up to (M11b: the sidebar row's folder and git branch).

use memchr::{memchr, memchr2};
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;

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
        let mut i = 0;
        while i < bytes.len() {
            if self.at_rest() {
                // Nothing partly matched: only an ESC can begin anything, so jump to the next.
                match memchr(0x1b, &bytes[i..]) {
                    Some(skip) => i += skip,
                    None => break,
                }
            }
            let b = bytes[i];
            i += 1;
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

    /// No intro partly matched and no OSC 9 look-ahead pending: a byte that is not ESC changes
    /// nothing.
    fn at_rest(&self) -> bool {
        self.osc9.matched == 0
            && self.osc99.matched == 0
            && self.osc777.matched == 0
            && self.after_osc9.is_none()
    }
}

/// OSC 7's intro, and the longest report kept (a longer one is dropped).
const OSC_7: &[u8] = b"\x1b]7;";
const CWD_LIMIT: usize = 4096;

/// A working-folder report: the machine it's on (as the shell named it; empty when it didn't)
/// and the folder there.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CwdReport {
    pub host: String,
    pub path: PathBuf,
}

/// Finds the working-folder reports (OSC 7) in a terminal's raw output, across reads.
pub struct CwdReports {
    intro: Intro,
    /// After the intro: the report so far (and whether its last byte was an ESC, which may
    /// start its `ESC \\` end); none while not inside one, or once it has run too long.
    report: Option<Vec<u8>>,
    esc: bool,
}

impl CwdReports {
    pub fn new() -> Self {
        CwdReports {
            intro: Intro::new(OSC_7),
            report: None,
            esc: false,
        }
    }

    /// The last folder reported in full in `bytes`, if any.
    pub fn feed(&mut self, bytes: &[u8]) -> Option<CwdReport> {
        let mut found = None;
        let mut i = 0;
        while i < bytes.len() {
            if self.at_rest() {
                match memchr(0x1b, &bytes[i..]) {
                    Some(skip) => i += skip,
                    None => break,
                }
            } else if self.in_plain_body() {
                // Inside a report, up to the next BEL or ESC every byte is just appended.
                let rest = &bytes[i..];
                let span = memchr2(0x07, 0x1b, rest).unwrap_or(rest.len());
                if let Some(report) = self.report.as_mut() {
                    // One past the limit is enough to know the report is dropped.
                    let room = (CWD_LIMIT + 1).saturating_sub(report.len());
                    report.extend_from_slice(&rest[..span.min(room)]);
                    if report.len() > CWD_LIMIT {
                        self.report = None;
                    }
                }
                if span > 0 {
                    i += span;
                    continue;
                }
            }
            let b = bytes[i];
            i += 1;
            if let Some(mut report) = self.report.take() {
                let esc = std::mem::take(&mut self.esc);
                match (esc, b) {
                    (false, 0x07) | (true, b'\\') => found = cwd_from_report(&report).or(found),
                    // An ESC that doesn't end it: something else began; this one is abandoned.
                    (true, _) => {}
                    (false, 0x1b) => {
                        self.esc = true;
                        self.report = Some(report);
                    }
                    (false, _) => {
                        report.push(b);
                        if report.len() <= CWD_LIMIT {
                            self.report = Some(report);
                        }
                    }
                }
            }
            if self.intro.step(b) {
                self.report = Some(Vec::new());
                self.esc = false;
            }
        }
        found
    }

    /// No intro partly matched and no report open: a byte that is not ESC changes nothing.
    fn at_rest(&self) -> bool {
        self.report.is_none() && self.intro.matched == 0
    }

    /// Inside a report whose last byte was not an ESC, with no intro partly matched.
    fn in_plain_body(&self) -> bool {
        self.report.is_some() && !self.esc && self.intro.matched == 0
    }
}

/// An OSC 7 report read: `file://host/path` (percent-encoded) gives `host` and `/path`.
/// Anything else (another scheme, no path, bad encoding, control characters in the path) gives
/// none.
pub fn cwd_from_report(report: &[u8]) -> Option<CwdReport> {
    let rest = report.strip_prefix(b"file://")?;
    // Past the host, the path.
    let slash = rest.iter().position(|&b| b == b'/')?;
    let host = String::from_utf8_lossy(&rest[..slash]).into_owned();
    let path = &rest[slash..];
    let mut out = Vec::with_capacity(path.len());
    let mut bytes = path.iter();
    while let Some(&b) = bytes.next() {
        if b != b'%' {
            out.push(b);
            continue;
        }
        let hex = |b: Option<&u8>| (*b? as char).to_digit(16);
        out.push((hex(bytes.next())? * 16 + hex(bytes.next())?) as u8);
    }
    // A folder with control characters (a newline, ESC, a bidi override) isn't printed.
    let shown = String::from_utf8_lossy(&out);
    if shown.chars().any(|c| {
        c.is_control()
            || ('\u{202a}'..='\u{202e}').contains(&c)
            || ('\u{2066}'..='\u{2069}').contains(&c)
    }) {
        return None;
    }
    Some(CwdReport {
        host,
        path: PathBuf::from(OsStr::from_bytes(&out)),
    })
}

/// The verbatim byte-at-a-time `feed` bodies from before M11n, kept as the oracle the fast
/// scanners are proven against.
#[cfg(test)]
mod bytewise {
    use super::*;

    impl OscDetector {
        pub(super) fn feed_bytewise(&mut self, bytes: &[u8]) -> usize {
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

    impl CwdReports {
        pub(super) fn feed_bytewise(&mut self, bytes: &[u8]) -> Option<CwdReport> {
            let mut found = None;
            for &b in bytes {
                if let Some(mut report) = self.report.take() {
                    let esc = std::mem::take(&mut self.esc);
                    match (esc, b) {
                        (false, 0x07) | (true, b'\\') => found = cwd_from_report(&report).or(found),
                        // An ESC that doesn't end it: something else began; this one is abandoned.
                        (true, _) => {}
                        (false, 0x1b) => {
                            self.esc = true;
                            self.report = Some(report);
                        }
                        (false, _) => {
                            report.push(b);
                            if report.len() <= CWD_LIMIT {
                                self.report = Some(report);
                            }
                        }
                    }
                }
                if self.intro.step(b) {
                    self.report = Some(Vec::new());
                    self.esc = false;
                }
            }
            found
        }
    }
}

/// Streams shared by the three scanner files' equivalence tests (detector, shell marks, clear
/// counter): seeded plain text mixed with every sequence they read.
#[cfg(test)]
pub(crate) mod fixtures {
    pub fn splitmix(state: &mut u64) -> u64 {
        *state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = *state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// Every sequence the scanners read, and near misses.
    pub fn sequences() -> Vec<Vec<u8>> {
        let mut long_cwd = b"\x1b]7;file://h/".to_vec();
        long_cwd.extend(std::iter::repeat_n(b'a', 4097 - 9));
        long_cwd.push(0x07);
        let mut long_mark = b"\x1b]133;C;cmdline_url=".to_vec();
        long_mark.extend(std::iter::repeat_n(b'a', 9000));
        long_mark.push(0x07);
        let mut huge_cwd_st = b"\x1b]7;file://h/".to_vec();
        huge_cwd_st.extend(std::iter::repeat_n(b'b', 4090));
        huge_cwd_st.extend_from_slice(b"\x1b\\");
        vec![
            b"\x1b]9;msg\x07".to_vec(),
            b"\x1b]9;4;1;50\x07".to_vec(),
            b"\x1b]9;4;3\x1b\\".to_vec(),
            b"\x1b]99;i=1;hello\x1b\\".to_vec(),
            b"\x1b]777;notify;title;body\x07".to_vec(),
            b"\x1b]7;file://h/p\x07".to_vec(),
            b"\x1b]7;file://h/a%20b\x1b\\".to_vec(),
            b"\x1b]133;A\x07".to_vec(),
            b"\x1b]133;B\x07".to_vec(),
            b"\x1b]133;C;cmdline_url=ls%20-la\x07".to_vec(),
            b"\x1b]133;D;0\x07".to_vec(),
            b"\x1b]133;D;127\x1b\\".to_vec(),
            b"\x1b[2J".to_vec(),
            b"\x1b[3J".to_vec(),
            b"\x1bc".to_vec(),
            b"\x1b".to_vec(),
            b"\x1b\x1b".to_vec(),
            b"\x1b]7;file://h/x\x1b]9;m\x07".to_vec(),
            b"\x1b]133;C;a\x18b\x07".to_vec(),
            b"\x1b]133;C;a\x1ab\x07".to_vec(),
            b"\x1b]7;ab\x18cd\x07".to_vec(),
            b"\x18".to_vec(),
            b"\x1a".to_vec(),
            b"\x1b]13".to_vec(),
            b"\x1b[".to_vec(),
            b"\x1b[2".to_vec(),
            b"\x1b]9".to_vec(),
            long_cwd,
            long_mark,
            huge_cwd_st,
        ]
    }

    fn plain(state: &mut u64) -> Vec<u8> {
        const ALPHABET: &[u8] = b"abc xyz[]2379;4cJ\\\n\r\t\x07";
        let n = (splitmix(state) % 40) as usize;
        (0..n)
            .map(|_| ALPHABET[(splitmix(state) % ALPHABET.len() as u64) as usize])
            .collect()
    }

    pub fn random_stream(seed: u64) -> Vec<u8> {
        let mut state = seed;
        let seqs = sequences();
        let mut out = Vec::new();
        for _ in 0..(20 + splitmix(&mut state) % 40) {
            out.extend(plain(&mut state));
            let k = (splitmix(&mut state) % seqs.len() as u64) as usize;
            out.extend(&seqs[k]);
        }
        out.extend(plain(&mut state));
        out
    }

    /// One stream holding every sequence, each between bits of text.
    pub fn every_sequence() -> Vec<u8> {
        let mut out = Vec::new();
        for s in sequences() {
            out.extend_from_slice(b"text ");
            out.extend(s);
        }
        out.extend_from_slice(b" tail");
        out
    }

    /// `bytes` cut into random chunks of 1..=4096 bytes.
    pub fn random_chunks(seed: u64, bytes: &[u8]) -> Vec<&[u8]> {
        let mut state = seed ^ 0xdead_beef;
        let mut out = Vec::new();
        let mut at = 0;
        while at < bytes.len() {
            let n = (1 + splitmix(&mut state) % 4096) as usize;
            let end = (at + n).min(bytes.len());
            out.push(&bytes[at..end]);
            at = end;
        }
        out
    }
}

#[cfg(test)]
mod equivalence {
    use super::fixtures::*;
    use super::*;

    #[test]
    fn fast_scanners_match_bytewise_at_every_split() {
        let _ = OscDetector::new().at_rest();
        let _ = CwdReports::new().at_rest();
        for seed in 1..=200u64 {
            let stream = random_stream(seed);
            let mut slow = OscDetector::new();
            let want: usize = stream.chunks(1).map(|c| slow.feed_bytewise(c)).sum();
            let mut fast = OscDetector::new();
            let got: usize = random_chunks(seed, &stream)
                .iter()
                .map(|c| fast.feed(c))
                .sum();
            assert_eq!(got, want, "notifications, seed {seed}");

            let mut slow = CwdReports::new();
            let want = stream
                .chunks(1)
                .fold(None, |last, c| slow.feed_bytewise(c).or(last));
            let mut fast = CwdReports::new();
            let got = random_chunks(seed, &stream)
                .iter()
                .fold(None, |last, c| fast.feed(c).or(last));
            assert_eq!(got, want, "last cwd, seed {seed}");
        }
    }

    #[test]
    fn fast_scanners_match_bytewise_split_at_every_offset() {
        let stream = every_sequence();
        let mut slow = OscDetector::new();
        let want_n = slow.feed_bytewise(&stream);
        let mut slow = CwdReports::new();
        let want_c = stream
            .chunks(1)
            .fold(None, |last, c| slow.feed_bytewise(c).or(last));
        assert!(want_n > 0 && want_c.is_some());
        for cut in 0..=stream.len() {
            let (a, b) = stream.split_at(cut);
            let mut fast = OscDetector::new();
            assert_eq!(
                fast.feed(a) + fast.feed(b),
                want_n,
                "notifications, cut {cut}"
            );
            let mut fast = CwdReports::new();
            let got = fast.feed(a);
            let got = fast.feed(b).or(got);
            assert_eq!(got, want_c, "cwd, cut {cut}");
        }
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

    #[test]
    fn a_reported_folder_is_read_whichever_way_it_ends() {
        let mut c = CwdReports::new();
        let bel = c.feed(b"$ cd api\r\n\x1b]7;file://mac.local/Users/me/code/api\x07$ ");
        assert_eq!(
            bel,
            Some(CwdReport {
                host: "mac.local".into(),
                path: "/Users/me/code/api".into()
            })
        );
        let st = c.feed(b"\x1b]7;file://mac.local/tmp\x1b\\").map(|r| r.path);
        assert_eq!(st, Some(PathBuf::from("/tmp")));
        assert_eq!(c.feed(b"plain output\n"), None);
    }

    #[test]
    fn a_report_split_across_reads_is_still_read_and_the_last_wins() {
        let mut c = CwdReports::new();
        assert_eq!(c.feed(b"\x1b]7;file://h/Us"), None);
        assert_eq!(c.feed(b"ers/me\x1b"), None);
        assert_eq!(c.feed(b"\\").map(|r| r.path), Some("/Users/me".into()));
        let two = c
            .feed(b"\x1b]7;file://h/a\x07\x1b]7;file://h/b\x07")
            .map(|r| r.path);
        assert_eq!(two, Some(PathBuf::from("/b")));
    }

    #[test]
    fn other_sequences_and_bad_reports_are_not_folders() {
        let mut c = CwdReports::new();
        // Notifications and titles aren't reports.
        assert_eq!(c.feed(b"\x1b]777;notify;x;y\x07\x1b]0;title\x07"), None);
        // An ESC that isn't the end abandons it.
        assert_eq!(c.feed(b"\x1b]7;file://h/x\x1b[0m\x07"), None);
        // Too long: dropped, and the next one still reads.
        let mut long = b"\x1b]7;file://h/".to_vec();
        long.extend(std::iter::repeat_n(b'a', CWD_LIMIT + 10));
        long.push(0x07);
        assert_eq!(c.feed(&long), None);
        assert_eq!(
            c.feed(b"\x1b]7;file://h/ok\x07").map(|r| r.path),
            Some(PathBuf::from("/ok"))
        );
    }

    #[test]
    fn a_report_url_gives_its_decoded_path() {
        let path = |s: &str| cwd_from_report(s.as_bytes()).map(|r| r.path);
        assert_eq!(path("file://mac/Users/me"), Some("/Users/me".into()));
        // No host.
        assert_eq!(path("file:///Users/me"), Some("/Users/me".into()));
        // Percent-encoded, including a space and UTF-8.
        assert_eq!(
            path("file://mac/Users/me/My%20Code/caf%C3%A9"),
            Some("/Users/me/My Code/café".into())
        );
        assert_eq!(path("kitty-shell-cwd://mac/x"), None);
        assert_eq!(path("file://mac"), None);
        assert_eq!(path("file://mac/a%2"), None);
        assert_eq!(path("file://mac/a%zz"), None);
        assert_eq!(path(""), None);
        // Control characters (a newline, ESC, a right-to-left override) are no folder a row
        // should print.
        assert_eq!(path("file://mac/a%0Ab"), None);
        assert_eq!(path("file://mac/a%1B[31m"), None);
        assert_eq!(path("file://mac/a%E2%80%AEb"), None);
    }

    #[test]
    fn a_report_keeps_the_host_it_names() {
        let host = |s: &str| cwd_from_report(s.as_bytes()).map(|r| r.host);
        assert_eq!(host("file://Mac.local/Users/me"), Some("Mac.local".into()));
        assert_eq!(host("file://devbox/home/me"), Some("devbox".into()));
        assert_eq!(host("file:///Users/me"), Some(String::new()));
    }
}
