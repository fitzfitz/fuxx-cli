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

/// OSC 7's intro, and the longest report kept (a longer one is dropped).
const OSC_7: &[u8] = b"\x1b]7;";
const CWD_LIMIT: usize = 4096;

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
    pub fn feed(&mut self, bytes: &[u8]) -> Option<PathBuf> {
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

/// An OSC 7 report's folder: `file://host/path` (percent-encoded) gives `/path`. Anything
/// else (another scheme, no path, bad encoding) gives none.
pub fn cwd_from_report(report: &[u8]) -> Option<PathBuf> {
    let rest = report.strip_prefix(b"file://")?;
    // Past the host, the path.
    let path = &rest[rest.iter().position(|&b| b == b'/')?..];
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
    Some(PathBuf::from(OsStr::from_bytes(&out)))
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
        assert_eq!(bel, Some(PathBuf::from("/Users/me/code/api")));
        let st = c.feed(b"\x1b]7;file://mac.local/tmp\x1b\\");
        assert_eq!(st, Some(PathBuf::from("/tmp")));
        assert_eq!(c.feed(b"plain output\n"), None);
    }

    #[test]
    fn a_report_split_across_reads_is_still_read_and_the_last_wins() {
        let mut c = CwdReports::new();
        assert_eq!(c.feed(b"\x1b]7;file://h/Us"), None);
        assert_eq!(c.feed(b"ers/me\x1b"), None);
        assert_eq!(c.feed(b"\\"), Some(PathBuf::from("/Users/me")));
        let two = c.feed(b"\x1b]7;file://h/a\x07\x1b]7;file://h/b\x07");
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
            c.feed(b"\x1b]7;file://h/ok\x07"),
            Some(PathBuf::from("/ok"))
        );
    }

    #[test]
    fn a_report_url_gives_its_decoded_path() {
        let path = |s: &str| cwd_from_report(s.as_bytes());
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
    }
}
