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

/// An event's words (M11v part 4): a status's word (`Done`, its time on the second line), a
/// note's text, `Progress 42% · build`.
pub fn event_title(kind: &Kind) -> String {
    match kind {
        Kind::Status { status, .. } => match status {
            Status::Idle => "Idle".into(),
            Status::Working => "Cultivating".into(),
            Status::Waiting => "Needs input".into(),
            Status::Done => "Done".into(),
        },
        Kind::Note(n) => n.clone(),
        Kind::Progress(p, Some(l)) => format!("Progress {p}% \u{b7} {l}"),
        Kind::Progress(p, None) => format!("Progress {p}%"),
    }
}

/// An event's faint second line: how long it worked, under Done (`Worked 11 min`); `Note`
/// under a note. None for the others (a question has its box, progress its bar).
pub fn event_detail(kind: &Kind) -> Option<String> {
    match kind {
        Kind::Status {
            status: Status::Done,
            lasted,
            ..
        } => Some(format!("Worked {}", lasted_label(*lasted))),
        Kind::Note(_) => Some("Note".into()),
        _ => None,
    }
}

/// The day (from 1970-01-01) `unix_secs` falls on, `utc_offset_secs` east of UTC.
pub fn local_day(unix_secs: i64, utc_offset_secs: i64) -> i64 {
    (unix_secs + utc_offset_secs).div_euclid(86_400)
}

/// Day `day` (from 1970-01-01) as (year, month 1–12, day 1–31), in the proleptic Gregorian
/// calendar (H. Hinnant's `civil_from_days`).
pub fn civil_date(day: i64) -> (i64, u32, u32) {
    let z = day + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

/// A day's heading over its events (part 4): `Today`, else its date (`10 Nov`; `11 Nov 2025`
/// in another year). The panel draws it in capitals.
pub fn day_heading(day: i64, today: i64) -> String {
    if day == today {
        return "Today".into();
    }
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let (y, m, d) = civil_date(day);
    let month = MONTHS[(m - 1) as usize];
    if y == civil_date(today).0 {
        format!("{d} {month}")
    } else {
        format!("{d} {month} {y}")
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

    /// Part 4 (Activity A): an event's words and its faint second line.
    #[test]
    fn an_event_s_title_and_second_line() {
        let s = |status, mins: u64| Kind::Status {
            status,
            lasted: Duration::from_secs(mins * 60),
            question: Some("q?".into()),
        };
        assert_eq!(event_title(&s(Status::Working, 0)), "Cultivating");
        assert_eq!(event_title(&s(Status::Waiting, 0)), "Needs input");
        assert_eq!(event_title(&s(Status::Done, 11)), "Done");
        assert_eq!(event_title(&s(Status::Idle, 0)), "Idle");
        assert_eq!(
            event_title(&Kind::Progress(42, Some("build".into()))),
            "Progress 42% \u{b7} build"
        );
        assert_eq!(event_title(&Kind::Progress(42, None)), "Progress 42%");
        assert_eq!(event_title(&Kind::Note("x".into())), "x");
        // How long it worked under Done; "Note" under a note; nothing else has one (a question
        // has its box, progress its bar).
        assert_eq!(
            event_detail(&s(Status::Done, 11)).as_deref(),
            Some("Worked 11 min")
        );
        assert_eq!(
            event_detail(&Kind::Note("x".into())).as_deref(),
            Some("Note")
        );
        for kind in [
            s(Status::Working, 3),
            s(Status::Waiting, 3),
            s(Status::Idle, 3),
            Kind::Progress(5, None),
        ] {
            assert_eq!(event_detail(&kind), None, "{kind:?}");
        }
    }

    /// The day an event falls on, where you are, and its heading: `Today`, else the date (with
    /// its year when it isn't this one).
    #[test]
    fn days_and_their_headings() {
        // 2026-11-11 00:30 UTC: still the 10th an hour west of UTC.
        let secs = 20_768 * 86_400 + 30 * 60;
        assert_eq!(local_day(secs, 0), 20_768);
        assert_eq!(local_day(secs, -3600), 20_767);
        assert_eq!(local_day(secs - 3600, 3600), 20_768);
        assert_eq!(civil_date(20_768), (2026, 11, 11));
        assert_eq!(civil_date(0), (1970, 1, 1));
        assert_eq!(civil_date(-1), (1969, 12, 31));
        assert_eq!(civil_date(11_016), (2000, 2, 29));
        assert_eq!(day_heading(20_768, 20_768), "Today");
        assert_eq!(day_heading(20_767, 20_768), "10 Nov");
        assert_eq!(day_heading(20_768 - 365, 20_768), "11 Nov 2025");
    }
}
