//! A terminal's activity log (M11h): its status changes, `fuxx note` lines and `fuxx progress`
//! updates, newest last, in memory only, the last `MAX`. Pure: the caller passes the time in.
//! The Activity panel draws it.

use crate::status::Status;
use std::collections::VecDeque;
use std::time::{Duration, SystemTime};

/// Entries kept per terminal; older ones fall off.
pub const MAX: usize = 200;

#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    /// The terminal turned `status`; `lasted` is how long the status before it had lasted.
    Status {
        status: Status,
        lasted: Duration,
        question: Option<String>,
    },
    Note(String),
    Progress(u8, Option<String>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub at: SystemTime,
    pub kind: Kind,
}

#[derive(Default, Debug)]
pub struct ActivityLog {
    entries: VecDeque<Entry>,
}

impl ActivityLog {
    fn push(&mut self, at: SystemTime, kind: Kind) {
        if self.entries.len() >= MAX {
            self.entries.pop_front();
        }
        self.entries.push_back(Entry { at, kind });
    }

    /// A real status change (the caller skips non-changes).
    pub fn status(
        &mut self,
        status: Status,
        lasted: Duration,
        question: Option<String>,
        at: SystemTime,
    ) {
        self.push(
            at,
            Kind::Status {
                status,
                lasted,
                question,
            },
        );
    }

    /// A note: blank, cleared or the same as the last note entry is nothing.
    pub fn note(&mut self, note: Option<&str>, at: SystemTime) {
        let Some(note) = note.map(str::trim).filter(|n| !n.is_empty()) else {
            return;
        };
        let last = self.entries.iter().rev().find_map(|e| match &e.kind {
            Kind::Note(n) => Some(n.as_str()),
            _ => None,
        });
        if last != Some(note) {
            self.push(at, Kind::Note(note.to_string()));
        }
    }

    /// A progress update: cleared is nothing; right after a progress entry it replaces that one.
    pub fn progress(&mut self, progress: Option<(u8, Option<String>)>, at: SystemTime) {
        let Some((percent, label)) = progress else {
            return;
        };
        let kind = Kind::Progress(percent, label);
        match self.entries.back_mut() {
            Some(last) if matches!(last.kind, Kind::Progress(..)) => *last = Entry { at, kind },
            _ => self.push(at, kind),
        }
    }

    pub fn newest_first(&self) -> impl Iterator<Item = &Entry> {
        self.entries.iter().rev()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// `HH:MM`, 24-hour, at `utc_offset_secs` east of UTC.
pub fn clock_hm(unix_secs: i64, utc_offset_secs: i64) -> String {
    let day = (unix_secs + utc_offset_secs).rem_euclid(86_400);
    format!("{:02}:{:02}", day / 3600, day % 3600 / 60)
}

/// `45 s`, `11 min`, `1 h 5 min`, `2 h`.
pub fn lasted_label(d: Duration) -> String {
    let secs = d.as_secs();
    if secs < 60 {
        return format!("{secs} s");
    }
    let mins = secs / 60;
    if mins < 60 {
        return format!("{mins} min");
    }
    match mins % 60 {
        0 => format!("{} h", mins / 60),
        m => format!("{} h {m} min", mins / 60),
    }
}

/// The row's words.
pub fn entry_text(kind: &Kind) -> String {
    match kind {
        Kind::Status { status, lasted, .. } => match status {
            Status::Idle => "Idle".into(),
            Status::Working => "Working".into(),
            Status::Waiting => "Needs input".into(),
            Status::Done => format!("Done \u{2014} {}", lasted_label(*lasted)),
        },
        Kind::Note(n) => n.clone(),
        Kind::Progress(p, Some(l)) => format!("Progress {p}% \u{b7} {l}"),
        Kind::Progress(p, None) => format!("Progress {p}%"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::UNIX_EPOCH;

    fn t(s: u64) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(s)
    }

    fn st(log: &mut ActivityLog, s: Status, at: u64) {
        log.status(s, Duration::ZERO, None, t(at));
    }

    #[test]
    fn a_status_change_is_an_entry_with_how_long_the_last_lasted() {
        let mut log = ActivityLog::default();
        log.status(
            Status::Done,
            Duration::from_secs(660),
            Some("q?".into()),
            t(5),
        );
        let e = log.newest_first().next().unwrap();
        assert_eq!(e.at, t(5));
        assert_eq!(
            e.kind,
            Kind::Status {
                status: Status::Done,
                lasted: Duration::from_secs(660),
                question: Some("q?".into())
            }
        );
    }

    #[test]
    fn it_keeps_the_last_200() {
        let mut log = ActivityLog::default();
        for i in 0..205 {
            st(&mut log, Status::Working, i);
        }
        assert_eq!(log.len(), 200);
        assert_eq!(log.newest_first().next().unwrap().at, t(204));
        assert_eq!(log.newest_first().last().unwrap().at, t(5));
    }

    #[test]
    fn progress_updates_in_a_row_are_one_entry() {
        let mut log = ActivityLog::default();
        log.progress(Some((10, None)), t(0));
        log.progress(Some((20, None)), t(1));
        log.progress(Some((30, None)), t(2));
        assert_eq!(log.len(), 1);
        let e = log.newest_first().next().unwrap();
        assert_eq!(e.kind, Kind::Progress(30, None));
        assert_eq!(e.at, t(2));
        log.progress(Some((40, None)), t(3));
        st(&mut log, Status::Done, 4);
        log.progress(Some((50, None)), t(5));
        assert_eq!(log.len(), 3);
    }

    #[test]
    fn notes_skip_blank_cleared_and_repeats() {
        let mut log = ActivityLog::default();
        log.note(Some("a"), t(0));
        log.note(Some("a"), t(1));
        assert_eq!(log.len(), 1);
        log.note(Some("  "), t(2));
        log.note(None, t(3));
        assert_eq!(log.len(), 1);
        log.note(Some("b"), t(4));
        assert_eq!(log.len(), 2);
    }

    fn notes(log: &ActivityLog) -> Vec<String> {
        log.newest_first()
            .filter_map(|e| match &e.kind {
                Kind::Note(n) => Some(n.clone()),
                _ => None,
            })
            .collect()
    }

    /// "The same as the last note entry": the last note in the log, whatever came between.
    #[test]
    fn a_note_repeated_after_a_status_change_is_still_a_repeat() {
        let mut log = ActivityLog::default();
        log.note(Some("a"), t(0));
        st(&mut log, Status::Working, 1);
        log.note(Some("a"), t(2));
        assert_eq!(notes(&log), ["a"]);
        assert_eq!(log.len(), 2, "1 note and 1 status");
    }

    #[test]
    fn a_note_back_after_another_is_a_new_entry() {
        let mut log = ActivityLog::default();
        log.note(Some("a"), t(0));
        log.note(Some("b"), t(1));
        log.note(Some("a"), t(2));
        assert_eq!(notes(&log), ["a", "b", "a"]);
    }

    #[test]
    fn cleared_progress_is_not_an_entry() {
        let mut log = ActivityLog::default();
        log.progress(None, t(0));
        assert!(log.is_empty());
        log.progress(Some((5, None)), t(1));
        log.progress(None, t(2));
        assert_eq!(log.len(), 1);
    }

    #[test]
    fn clock_hm_is_local_24_hour() {
        assert_eq!(clock_hm(0, 0), "00:00");
        assert_eq!(clock_hm(14 * 3600 + 2 * 60, 0), "14:02");
        assert_eq!(clock_hm(23 * 3600, 3600), "00:00");
        assert_eq!(clock_hm(30 * 60, -3600), "23:30");
    }

    #[test]
    fn lasted_label_formats() {
        let m = |n: u64| Duration::from_secs(n * 60);
        assert_eq!(lasted_label(Duration::from_secs(45)), "45 s");
        assert_eq!(lasted_label(m(11)), "11 min");
        assert_eq!(lasted_label(m(65)), "1 h 5 min");
        assert_eq!(lasted_label(m(120)), "2 h");
    }

    #[test]
    fn entry_text_words() {
        let s = |status, mins: u64| Kind::Status {
            status,
            lasted: Duration::from_secs(mins * 60),
            question: None,
        };
        assert_eq!(entry_text(&s(Status::Working, 0)), "Working");
        assert_eq!(entry_text(&s(Status::Waiting, 0)), "Needs input");
        assert_eq!(entry_text(&s(Status::Done, 11)), "Done \u{2014} 11 min");
        assert_eq!(entry_text(&s(Status::Idle, 0)), "Idle");
        assert_eq!(
            entry_text(&Kind::Progress(42, Some("build".into()))),
            "Progress 42% \u{b7} build"
        );
        assert_eq!(entry_text(&Kind::Progress(42, None)), "Progress 42%");
        assert_eq!(entry_text(&Kind::Note("x".into())), "x");
    }
}
