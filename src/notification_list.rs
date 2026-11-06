//! This run's notifications (M11b), for the 🔔 popover in the title area: every notification
//! fuxx raises (a terminal finished, or waits for you: the M4 rules), whether or not macOS showed
//! it. Newest first, at most `MAX` (the oldest go). The ones raised since the popover was last
//! opened are unseen, and the 🔔 has a dot while there are any.
//!
//! A terminal closed since keeps its entries, greyed and not clickable (`shown`): terminal ids
//! are never reused, so an entry can't jump to another terminal. Pure; the popover is in
//! `title_area`.

use std::collections::VecDeque;
use std::time::Instant;

use crate::status::Status;

/// How many notifications the list keeps.
pub const MAX: usize = 50;

/// One notification as it was raised.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    /// The terminal it was about (`sN`): where a click goes.
    pub terminal: u64,
    /// The name of the workspace it was in then.
    pub title: String,
    /// What the notification said under its title: a waiting terminal's question, or how long a
    /// finished one worked.
    pub message: String,
    /// Done or Waiting.
    pub status: Status,
    pub at: Instant,
}

/// The list: oldest first inside, newest first out.
#[derive(Default)]
pub struct NotificationList {
    entries: VecDeque<Entry>,
    unseen: usize,
}

impl NotificationList {
    /// A notification was raised: it goes first, unseen, and past `MAX` the oldest goes.
    pub fn push(&mut self, entry: Entry) {
        self.entries.push_back(entry);
        if self.entries.len() > MAX {
            self.entries.pop_front();
        }
        self.unseen = (self.unseen + 1).min(self.entries.len());
    }

    /// The entries, newest first.
    pub fn newest_first(&self) -> impl Iterator<Item = &Entry> {
        self.entries.iter().rev()
    }

    /// How many were raised since the popover was last opened (still in the list).
    pub fn unseen(&self) -> usize {
        self.unseen
    }

    /// The popover opened (or is open as one arrives): everything in it has been seen.
    pub fn mark_seen(&mut self) {
        self.unseen = 0;
    }

    /// The entries newest first, each with whether its terminal is still open (`open`), so it
    /// can be clicked; a closed one's is drawn greyed.
    pub fn shown(&self, open: impl Fn(u64) -> bool) -> Vec<(&Entry, bool)> {
        self.newest_first().map(|e| (e, open(e.terminal))).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// Notification `n` (terminal `sN`, raised `n` seconds after `start`).
    fn entry(start: Instant, n: u64) -> Entry {
        Entry {
            terminal: n,
            title: format!("ws{n}"),
            message: format!("question {n}"),
            status: if n % 2 == 0 {
                Status::Done
            } else {
                Status::Waiting
            },
            at: start + Duration::from_secs(n),
        }
    }

    fn terminals(list: &NotificationList) -> Vec<u64> {
        list.newest_first().map(|e| e.terminal).collect()
    }

    #[test]
    fn the_newest_comes_first() {
        let start = Instant::now();
        let mut list = NotificationList::default();
        assert_eq!(list.newest_first().count(), 0);
        for n in 1..=3 {
            list.push(entry(start, n));
        }
        assert_eq!(terminals(&list), [3, 2, 1]);
        // Kept as raised.
        let newest = list.newest_first().next().expect("an entry");
        assert_eq!(*newest, entry(start, 3));
    }

    #[test]
    fn past_fifty_the_oldest_go() {
        let start = Instant::now();
        let mut list = NotificationList::default();
        for n in 1..=MAX as u64 {
            list.push(entry(start, n));
        }
        assert_eq!(terminals(&list).len(), 50);
        list.push(entry(start, 51));
        list.push(entry(start, 52));
        let shown = terminals(&list);
        assert_eq!(shown.len(), 50);
        assert_eq!(shown.first(), Some(&52));
        assert_eq!(shown.last(), Some(&3), "1 and 2 went");
    }

    #[test]
    fn what_came_since_the_popover_opened_is_unseen() {
        let start = Instant::now();
        let mut list = NotificationList::default();
        assert_eq!(list.unseen(), 0);
        list.push(entry(start, 1));
        list.push(entry(start, 2));
        assert_eq!(list.unseen(), 2);
        list.mark_seen();
        assert_eq!(list.unseen(), 0);
        assert_eq!(terminals(&list), [2, 1], "seen ones stay listed");
        list.push(entry(start, 3));
        assert_eq!(list.unseen(), 1);
        // Never more unseen than listed: 60 raised unseen, 50 kept.
        let mut full = NotificationList::default();
        for n in 1..=60 {
            full.push(entry(start, n));
        }
        assert_eq!(full.unseen(), MAX);
    }

    #[test]
    fn a_closed_terminals_entries_stay_but_cannot_be_clicked() {
        let start = Instant::now();
        let mut list = NotificationList::default();
        for n in [1, 2, 1, 3] {
            list.push(entry(start, n));
        }
        // s1 closed since.
        let shown: Vec<(u64, bool)> = list
            .shown(|id| id != 1)
            .into_iter()
            .map(|(e, open)| (e.terminal, open))
            .collect();
        assert_eq!(shown, [(3, true), (1, false), (2, true), (1, false)]);
    }
}
