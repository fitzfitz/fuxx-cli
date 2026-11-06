//! This run's notifications (M11b), for the 🔔 popover in the title area: every notification
//! fuxx raises (a terminal finished, or waits for you: the M4 rules; or a script's `fuxx notify`),
//! whether or not macOS showed it. Newest first, at most `MAX` (the oldest go). The ones raised since the popover was last
//! opened are unseen, and the 🔔 has a dot while there are any.
//!
//! A terminal closed since keeps its entries, greyed and not clickable (`shown`): terminal ids
//! are never reused, so an entry can't jump to another terminal. A script's notification is
//! about the terminal it ran in, if any (`scripted`); one from outside fuxx has none to go to. Pure; the popover is in
//! `title_area`.

use std::collections::VecDeque;
use std::time::Instant;

use crate::status::Status;

/// How many notifications the list keeps.
pub const MAX: usize = 50;

/// One notification as it was raised.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    /// The terminal it was about (`sN`): where a click goes. None: a script's notification
    /// from outside fuxx's terminals.
    pub terminal: Option<u64>,
    /// Its title: the name of the workspace it was in then, or the title a script gave.
    pub title: String,
    /// What the notification said under its title: a waiting terminal's question, how long a
    /// finished one worked, or a script's text.
    pub message: String,
    /// Done or Waiting; none for a script's notification.
    pub status: Option<Status>,
    pub at: Instant,
}

impl Entry {
    /// A script's notification (`fuxx notify`): `title` and `body` as it gave them, about the
    /// terminal it ran in (`caller`, its `FUXX_SESSION`) while that one is `open`.
    pub fn scripted(
        title: &str,
        body: &str,
        caller: Option<u64>,
        open: impl Fn(u64) -> bool,
        at: Instant,
    ) -> Entry {
        Entry {
            terminal: caller.filter(|&id| open(id)),
            title: title.into(),
            message: body.into(),
            status: None,
            at,
        }
    }
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
        self.newest_first()
            .map(|e| (e, e.terminal.is_some_and(&open)))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// Notification `n` (terminal `sN`, raised `n` seconds after `start`).
    fn entry(start: Instant, n: u64) -> Entry {
        Entry {
            terminal: Some(n),
            title: format!("ws{n}"),
            message: format!("question {n}"),
            status: Some(if n % 2 == 0 {
                Status::Done
            } else {
                Status::Waiting
            }),
            at: start + Duration::from_secs(n),
        }
    }

    fn terminals(list: &NotificationList) -> Vec<u64> {
        list.newest_first().filter_map(|e| e.terminal).collect()
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
        let shown: Vec<(Option<u64>, bool)> = list
            .shown(|id| id != 1)
            .into_iter()
            .map(|(e, open)| (e.terminal, open))
            .collect();
        let s = Some;
        assert_eq!(
            shown,
            [(s(3), true), (s(1), false), (s(2), true), (s(1), false)]
        );
    }

    #[test]
    fn a_scripts_notification_goes_to_the_terminal_it_ran_in() {
        let at = Instant::now();
        let open = |id| id == 4;
        let from_s4 = Entry::scripted("Build", "Tests passed", Some(4), open, at);
        assert_eq!(
            from_s4,
            Entry {
                terminal: Some(4),
                title: "Build".into(),
                message: "Tests passed".into(),
                status: None,
                at,
            }
        );
        // From outside fuxx's terminals, or one closed since (an id never comes back): no
        // terminal to go to.
        assert_eq!(Entry::scripted("fuxx", "hi", None, open, at).terminal, None);
        assert_eq!(
            Entry::scripted("fuxx", "hi", Some(9), open, at).terminal,
            None
        );
    }

    #[test]
    fn a_notification_with_no_terminal_is_listed_but_cannot_be_clicked() {
        let at = Instant::now();
        let mut list = NotificationList::default();
        list.push(Entry::scripted("fuxx", "deployed", None, |_| true, at));
        list.push(entry(at, 2));
        assert_eq!(list.unseen(), 2, "it counts for the dot");
        let shown: Vec<(&str, bool)> = list
            .shown(|_| true)
            .into_iter()
            .map(|(e, open)| (e.message.as_str(), open))
            .collect();
        assert_eq!(shown, [("question 2", true), ("deployed", false)]);
    }
}
