//! Events and waits without any I/O: the lines `events.subscribe` streams, and the pending
//! `session.wait`s with the rules for when each one ends.
//!
//! Pure and GPUI-free (the caller passes the sessions, the time and the screen text), so every
//! rule is unit-tested.

use std::collections::HashMap;
use std::time::Instant;

use serde_json::{Value, json};

use crate::api::{ApiError, WaitFor, wait_met};
use crate::status::Status;

/// A session as an event names it.
pub struct EventSession<'a> {
    pub id: u64,
    /// Its 1-based row in the sidebar.
    pub number: usize,
    pub name: &'a str,
}

pub enum Event<'a> {
    Opened,
    Closed,
    StatusChanged { from: Status, to: Status },
    TitleChanged { title: Option<&'a str> },
    Renamed { name: &'a str },
    FocusChanged,
    Notification { title: &'a str, body: &'a str },
}

/// One event line's object: `event`, the `session` it's about (if any), its details, and `ts`
/// (unix milliseconds).
pub fn event_json(event: &Event, session: Option<&EventSession>, ts: u64) -> Value {
    let (name, mut line) = match event {
        Event::Opened => ("session.opened", json!({})),
        Event::Closed => ("session.closed", json!({})),
        Event::StatusChanged { from, to } => (
            "status.changed",
            json!({ "from": from.as_str(), "to": to.as_str() }),
        ),
        Event::TitleChanged { title } => ("title.changed", json!({ "title": title })),
        Event::Renamed { name } => ("session.renamed", json!({ "name": name })),
        Event::FocusChanged => ("focus.changed", json!({})),
        Event::Notification { title, body } => {
            ("notification", json!({ "title": title, "body": body }))
        }
    };
    line["event"] = json!(name);
    if let Some(s) = session {
        line["session"] = json!({ "id": format!("s{}", s.id), "number": s.number, "name": s.name });
    }
    line["ts"] = json!(ts);
    line
}

/// Which sessions a wait watches.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Watch {
    One(u64),
    Any,
}

/// How a wait ended.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Outcome {
    /// The condition holds for this session.
    Met(u64),
    /// The watched session closed, and the condition can't hold any more.
    Closed(u64),
    TimedOut,
}

/// One pending `session.wait`, and the connection its reply goes to.
pub struct Wait<C> {
    pub conn: C,
    /// The request's id, for the reply.
    pub request: Option<u64>,
    pub watch: Watch,
    pub cond: WaitFor,
    /// Each watched session's status when the wait began (for `change`).
    start: Vec<(u64, Status)>,
    /// Text waits: each watched session's last lines as of the last check (at first, when the
    /// wait began). Only occurrences beyond these count (see `new_occurrences`).
    seen: Vec<(u64, String)>,
    pub deadline: Option<Instant>,
}

impl<C> Wait<C> {
    /// `sessions`: every open session's (id, status), in sidebar order; `screen`: a session's
    /// last lines (read only for text waits, as their starting point).
    pub fn new(
        conn: C,
        request: Option<u64>,
        watch: Watch,
        cond: WaitFor,
        sessions: &[(u64, Status)],
        deadline: Option<Instant>,
        screen: &mut dyn FnMut(u64) -> String,
    ) -> Self {
        let start: Vec<(u64, Status)> = sessions
            .iter()
            .filter(|(id, _)| watch == Watch::Any || watch == Watch::One(*id))
            .copied()
            .collect();
        let seen = match cond {
            WaitFor::Text(_) => start.iter().map(|&(id, _)| (id, screen(id))).collect(),
            _ => Vec::new(),
        };
        Self {
            conn,
            request,
            watch,
            cond,
            start,
            seen,
            deadline,
        }
    }

    /// Whether the wait ends now, and how. `sessions`: every open session's (id, status) in
    /// sidebar order; `closed`: sessions closing now (they may still be in `sessions`, for a last
    /// look at their status and screen); `screen`: a session's last lines (read only for text
    /// waits).
    pub fn outcome(
        &mut self,
        now: Instant,
        sessions: &[(u64, Status)],
        closed: &[u64],
        screen: &mut dyn FnMut(u64) -> String,
    ) -> Option<Outcome> {
        let ended = self.met(sessions, closed, screen);
        if ended.is_some() {
            return ended;
        }
        self.deadline
            .is_some_and(|d| now >= d)
            .then_some(Outcome::TimedOut)
    }

    /// The condition's outcome, deadline aside.
    fn met(
        &mut self,
        sessions: &[(u64, Status)],
        closed: &[u64],
        screen: &mut dyn FnMut(u64) -> String,
    ) -> Option<Outcome> {
        let exit_counts = matches!(self.cond, WaitFor::Exit | WaitFor::Change);
        match self.watch {
            Watch::One(id) => {
                let open = sessions.iter().find(|(s, _)| *s == id);
                if let Some(&(_, now)) = open
                    && self.holds(id, now, screen)
                {
                    return Some(Outcome::Met(id));
                }
                if closed.contains(&id) {
                    return Some(if exit_counts {
                        Outcome::Met(id)
                    } else {
                        Outcome::Closed(id)
                    });
                }
                open.is_none().then_some(Outcome::Closed(id))
            }
            Watch::Any => {
                if exit_counts && let Some(&id) = closed.first() {
                    return Some(Outcome::Met(id));
                }
                sessions
                    .iter()
                    .find(|&&(id, now)| self.holds(id, now, screen))
                    .map(|&(id, _)| Outcome::Met(id))
            }
        }
    }

    /// Whether the condition holds for an open session. One opened after the wait began starts
    /// from idle (every session opens idle, and ids are never reused) and from a blank screen.
    /// A text wait holds when the text appears anew since the last check.
    fn holds(&mut self, id: u64, now: Status, screen: &mut dyn FnMut(u64) -> String) -> bool {
        if let WaitFor::Text(text) = &self.cond {
            let lines = screen(id);
            let slot = self.seen.iter().position(|(s, _)| *s == id);
            let before = slot.map_or("", |i| self.seen[i].1.as_str());
            let met = new_occurrences(before, &lines, text) > 0;
            match slot {
                Some(i) => self.seen[i].1 = lines,
                None => self.seen.push((id, lines)),
            }
            return met;
        }
        let start = self
            .start
            .iter()
            .find(|(s, _)| *s == id)
            .map_or(Status::Idle, |&(_, start)| start);
        wait_met(&self.cond, start, now, "", false)
    }
}

/// How many more times `text` is in `now` than in what is left of `before` (both a session's
/// last lines, oldest first). Output scrolls `before`'s first lines away, so `now` is lined up
/// where `before`'s lines match its first lines for longest (the fewest scrolled away on a tie);
/// the lines above that are gone and their occurrences don't count as still there. Nothing
/// lining up (a cleared screen, or a whole window of new output) makes all of `now` new.
fn new_occurrences(before: &str, now: &str, text: &str) -> usize {
    if text.is_empty() {
        return 0;
    }
    let old: Vec<&str> = before.lines().collect();
    let new: Vec<&str> = now.lines().collect();
    let mut gone = old.len();
    let mut longest = 0;
    for start in 0..old.len() {
        let matching = old[start..]
            .iter()
            .zip(&new)
            .take_while(|(a, b)| a == b)
            .count();
        if matching > longest {
            (gone, longest) = (start, matching);
        }
    }
    let kept = old[gone..].join("\n");
    now.matches(text)
        .count()
        .saturating_sub(kept.matches(text).count())
}

/// The reply a finished wait sends.
pub fn wait_reply(outcome: Outcome) -> Result<Value, ApiError> {
    match outcome {
        Outcome::Met(id) => Ok(json!({ "id": format!("s{id}") })),
        Outcome::Closed(id) => Err(ApiError::not_found(format!(
            "s{id} closed before the wait was met"
        ))),
        Outcome::TimedOut => Err(ApiError::timeout("timed out waiting")),
    }
}

/// Every pending wait.
pub struct WaitTable<C> {
    waits: Vec<Wait<C>>,
}

impl<C> WaitTable<C> {
    pub fn new() -> Self {
        Self { waits: Vec::new() }
    }

    pub fn is_empty(&self) -> bool {
        self.waits.is_empty()
    }

    pub fn push(&mut self, wait: Wait<C>) {
        self.waits.push(wait);
    }

    /// One pass over every wait: those whose client `is_gone` are dropped unanswered, and those
    /// that ended are taken out with their outcome. Each session's screen is read at most once.
    pub fn check(
        &mut self,
        now: Instant,
        sessions: &[(u64, Status)],
        closed: &[u64],
        mut screen: impl FnMut(u64) -> String,
        is_gone: impl Fn(&C) -> bool,
    ) -> Vec<(Wait<C>, Outcome)> {
        let mut seen: HashMap<u64, String> = HashMap::new();
        let mut cached = |id: u64| seen.entry(id).or_insert_with(|| screen(id)).clone();
        let mut finished = Vec::new();
        let mut i = 0;
        while i < self.waits.len() {
            if is_gone(&self.waits[i].conn) {
                self.waits.remove(i);
            } else if let Some(outcome) = self.waits[i].outcome(now, sessions, closed, &mut cached)
            {
                finished.push((self.waits.remove(i), outcome));
            } else {
                i += 1;
            }
        }
        finished
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli;
    use std::time::Duration;

    const S4: EventSession = EventSession {
        id: 4,
        number: 2,
        name: "api",
    };

    #[test]
    fn event_shapes() {
        let ts = 43_263_000u64;
        let session = json!({"id": "s4", "number": 2, "name": "api"});
        let cases = [
            (
                event_json(&Event::Opened, Some(&S4), ts),
                json!({"event": "session.opened", "session": session, "ts": ts}),
            ),
            (
                event_json(&Event::Closed, Some(&S4), ts),
                json!({"event": "session.closed", "session": session, "ts": ts}),
            ),
            (
                event_json(
                    &Event::StatusChanged {
                        from: Status::Working,
                        to: Status::Waiting,
                    },
                    Some(&S4),
                    ts,
                ),
                json!({"event": "status.changed", "session": session, "from": "working", "to": "waiting", "ts": ts}),
            ),
            (
                event_json(
                    &Event::TitleChanged {
                        title: Some("claude"),
                    },
                    Some(&S4),
                    ts,
                ),
                json!({"event": "title.changed", "session": session, "title": "claude", "ts": ts}),
            ),
            (
                event_json(&Event::TitleChanged { title: None }, Some(&S4), ts),
                json!({"event": "title.changed", "session": session, "title": null, "ts": ts}),
            ),
            (
                event_json(&Event::Renamed { name: "api" }, Some(&S4), ts),
                json!({"event": "session.renamed", "session": session, "name": "api", "ts": ts}),
            ),
            (
                event_json(&Event::FocusChanged, Some(&S4), ts),
                json!({"event": "focus.changed", "session": session, "ts": ts}),
            ),
            (
                event_json(
                    &Event::Notification {
                        title: "build",
                        body: "done",
                    },
                    None,
                    ts,
                ),
                json!({"event": "notification", "title": "build", "body": "done", "ts": ts}),
            ),
        ];
        for (got, want) in cases {
            assert_eq!(got, want);
        }
    }

    #[test]
    fn events_print_as_the_cli_expects() {
        let line = |e: Event, s: Option<&EventSession>| {
            cli::render("events", &event_json(&e, s, 43_263_000), false)
        };
        let status = line(
            Event::StatusChanged {
                from: Status::Working,
                to: Status::Waiting,
            },
            Some(&S4),
        );
        assert!(
            status.ends_with("  s4 #2 api  working → waiting"),
            "{status}"
        );
        let opened = line(Event::Opened, Some(&S4));
        assert!(opened.ends_with("  s4 #2 api  session.opened"), "{opened}");
        let note = line(
            Event::Notification {
                title: "build",
                body: "done",
            },
            None,
        );
        assert!(note.ends_with("  notification build: done"), "{note}");
    }

    fn at(base: Instant, ms: u64) -> Instant {
        base + Duration::from_millis(ms)
    }

    fn no_screen() -> impl FnMut(u64) -> String {
        |id| panic!("screen of s{id} read for a non-text wait")
    }

    fn wait(watch: Watch, cond: WaitFor, sessions: &[(u64, Status)]) -> Wait<()> {
        Wait::new((), Some(1), watch, cond, sessions, None, &mut no_screen())
    }

    /// A text wait started while every session showed `start`.
    fn text_wait(watch: Watch, text: &str, sessions: &[(u64, Status)], start: &str) -> Wait<()> {
        let mut screen = |_| start.to_string();
        Wait::new(
            (),
            Some(1),
            watch,
            WaitFor::Text(text.into()),
            sessions,
            None,
            &mut screen,
        )
    }

    #[test]
    fn new_occurrences_count_only_text_that_wasnt_there() {
        // Nothing new: the same lines, or more lines without it.
        assert_eq!(new_occurrences("x PASS\ny", "x PASS\ny", "PASS"), 0);
        assert_eq!(new_occurrences("x PASS\ny", "x PASS\ny\nz", "PASS"), 0);
        // A second one.
        assert_eq!(
            new_occurrences("PASS\n$ again", "PASS\n$ again\nPASS", "PASS"),
            1
        );
        // Appended to a line already there (e.g. `Building... PASS`).
        assert_eq!(
            new_occurrences("a\nBuilding...", "a\nBuilding... PASS", "PASS"),
            1
        );
        // The old one scrolled off the top in the same pass a new one appeared.
        assert_eq!(new_occurrences("a\nPASS\nb\nc", "b\nc\nd\nPASS", "PASS"), 1);
        // Erased (a cleared screen, a redrawn prompt) isn't new, and what follows is.
        assert_eq!(new_occurrences("PASS\n$ clear", "$", "PASS"), 0);
        assert_eq!(new_occurrences("$", "$\nPASS", "PASS"), 1);
        // Nothing lines up (a screenful of new output, or a new session): all of it is new.
        assert_eq!(new_occurrences("a\nb", "PASS\nc\nPASS", "PASS"), 2);
        assert_eq!(new_occurrences("", "PASS", "PASS"), 1);
        assert_eq!(new_occurrences("", "", "PASS"), 0);
    }

    #[test]
    fn text_on_screen_when_the_wait_starts_doesnt_count() {
        // `fuxx send s4 "make test && echo ALLDONE" --enter; fuxx wait s4 --text ALLDONE`:
        // the echoed command line already holds the text.
        let now = Instant::now();
        let open = [(4, Status::Idle)];
        let start = "ALLDONE (an earlier run)\n$ make test && echo ALLDONE";
        let mut w = text_wait(Watch::One(4), "ALLDONE", &open, start);
        let mut screen = |_| start.to_string();
        assert_eq!(w.outcome(now, &open, &[], &mut screen), None);
        let mut screen = |_| format!("{start}\nrunning 3 tests");
        assert_eq!(w.outcome(now, &open, &[], &mut screen), None);
        let mut screen = |_| format!("{start}\nrunning 3 tests\nALLDONE");
        assert_eq!(
            w.outcome(now, &open, &[], &mut screen),
            Some(Outcome::Met(4))
        );
    }

    #[test]
    fn any_text_wait_counts_from_each_sessions_start() {
        let now = Instant::now();
        let start = [(1, Status::Idle), (4, Status::Idle)];
        let mut w = text_wait(Watch::Any, "PASS", &start, "old PASS");
        let mut screen = |_| "old PASS".to_string();
        assert_eq!(w.outcome(now, &start, &[], &mut screen), None);
        // s7 opened after the wait began: everything on it is new.
        let later = [(1, Status::Idle), (4, Status::Idle), (7, Status::Idle)];
        assert_eq!(
            w.outcome(now, &later, &[], &mut screen),
            Some(Outcome::Met(7))
        );
    }

    #[test]
    fn one_session_waits_for_its_status() {
        let now = Instant::now();
        let mut w = wait(
            Watch::One(4),
            WaitFor::Status(Status::Waiting),
            &[(4, Status::Working)],
        );
        let mut screen = no_screen();
        assert_eq!(
            w.outcome(now, &[(4, Status::Working)], &[], &mut screen),
            None
        );
        // Another session waiting doesn't count.
        assert_eq!(
            w.outcome(
                now,
                &[(1, Status::Waiting), (4, Status::Done)],
                &[],
                &mut screen
            ),
            None
        );
        assert_eq!(
            w.outcome(now, &[(4, Status::Waiting)], &[], &mut screen),
            Some(Outcome::Met(4))
        );
    }

    #[test]
    fn change_compares_with_the_status_at_the_start() {
        let now = Instant::now();
        let mut screen = no_screen();
        let mut w = wait(Watch::One(4), WaitFor::Change, &[(4, Status::Working)]);
        assert_eq!(
            w.outcome(now, &[(4, Status::Working)], &[], &mut screen),
            None
        );
        assert_eq!(
            w.outcome(now, &[(4, Status::Done)], &[], &mut screen),
            Some(Outcome::Met(4))
        );
        // Closing is a change too.
        let mut w = wait(Watch::One(4), WaitFor::Change, &[(4, Status::Working)]);
        assert_eq!(
            w.outcome(now, &[], &[4], &mut screen),
            Some(Outcome::Met(4))
        );
    }

    #[test]
    fn a_closed_session_meets_exit_and_ends_other_waits() {
        let now = Instant::now();
        let mut screen = no_screen();
        let open = [(4, Status::Idle), (5, Status::Idle)];
        let mut exit = wait(Watch::One(4), WaitFor::Exit, &open);
        assert_eq!(exit.outcome(now, &open, &[], &mut screen), None);
        assert_eq!(
            exit.outcome(now, &[(4, Status::Idle)], &[5], &mut screen),
            None
        );
        assert_eq!(
            exit.outcome(now, &[(5, Status::Idle)], &[4], &mut screen),
            Some(Outcome::Met(4))
        );
        let mut status = wait(Watch::One(4), WaitFor::Status(Status::Done), &open);
        assert_eq!(
            status.outcome(now, &[(5, Status::Idle)], &[4], &mut screen),
            Some(Outcome::Closed(4))
        );
        // Gone without being reported closed (never, but not forever pending either).
        let mut status = wait(Watch::One(4), WaitFor::Status(Status::Done), &open);
        assert_eq!(
            status.outcome(now, &[(5, Status::Idle)], &[], &mut screen),
            Some(Outcome::Closed(4))
        );
    }

    #[test]
    fn any_is_met_by_the_first_session_in_sidebar_order() {
        let now = Instant::now();
        let mut screen = no_screen();
        let start = [(1, Status::Working), (4, Status::Working)];
        let mut w = wait(Watch::Any, WaitFor::Status(Status::Done), &start);
        assert_eq!(w.outcome(now, &start, &[], &mut screen), None);
        assert_eq!(
            w.outcome(
                now,
                &[(4, Status::Done), (1, Status::Done)],
                &[],
                &mut screen
            ),
            Some(Outcome::Met(4))
        );
        // `exit`: any session closing, named in the reply.
        let mut w = wait(Watch::Any, WaitFor::Exit, &start);
        assert_eq!(w.outcome(now, &start, &[], &mut screen), None);
        assert_eq!(
            w.outcome(now, &[(1, Status::Working)], &[4], &mut screen),
            Some(Outcome::Met(4))
        );
        // A status wait isn't met by a session closing.
        let mut w = wait(Watch::Any, WaitFor::Status(Status::Done), &start);
        assert_eq!(
            w.outcome(now, &[(1, Status::Working)], &[4], &mut screen),
            None
        );
    }

    #[test]
    fn any_change_counts_new_sessions_from_idle() {
        let now = Instant::now();
        let mut screen = no_screen();
        let mut w = wait(Watch::Any, WaitFor::Change, &[(1, Status::Working)]);
        // s7 opened after the wait began: every session opens idle, so idle is no change...
        let later = [(1, Status::Working), (7, Status::Idle)];
        assert_eq!(w.outcome(now, &later, &[], &mut screen), None);
        assert_eq!(
            w.outcome(
                now,
                &[(1, Status::Working), (7, Status::Working)],
                &[],
                &mut screen
            ),
            Some(Outcome::Met(7))
        );
        // ...and one first seen already working has changed (from idle), even unseen at idle.
        let mut w = wait(Watch::Any, WaitFor::Change, &[(1, Status::Working)]);
        assert_eq!(
            w.outcome(
                now,
                &[(1, Status::Working), (8, Status::Working)],
                &[],
                &mut screen
            ),
            Some(Outcome::Met(8))
        );
    }

    #[test]
    fn a_closing_session_gets_a_last_look() {
        // Checked just before removal: the closing session is still in `sessions`, and in
        // `closed`. PASS printed and the shell exited within one tick still meets the wait.
        let now = Instant::now();
        let mut screen = |id: u64| {
            if id == 4 {
                "$ make test\ntests PASS".to_string()
            } else {
                String::new()
            }
        };
        let open = [(1, Status::Idle), (4, Status::Done)];
        let mut text = text_wait(Watch::One(4), "PASS", &open, "$ make test");
        assert_eq!(
            text.outcome(now, &open, &[4], &mut screen),
            Some(Outcome::Met(4))
        );
        let mut done = wait(Watch::One(4), WaitFor::Status(Status::Done), &open);
        assert_eq!(
            done.outcome(now, &open, &[4], &mut screen),
            Some(Outcome::Met(4))
        );
        let mut any = text_wait(Watch::Any, "PASS", &open, "$ make test");
        assert_eq!(
            any.outcome(now, &open, &[4], &mut screen),
            Some(Outcome::Met(4))
        );
        // Not on its last screen either: closed.
        let mut fail = text_wait(Watch::One(4), "FAIL", &open, "$ make test");
        assert_eq!(
            fail.outcome(now, &open, &[4], &mut screen),
            Some(Outcome::Closed(4))
        );
    }

    #[test]
    fn text_waits_read_the_screen() {
        let now = Instant::now();
        let mut reads = Vec::new();
        let mut screen = |id: u64| {
            reads.push(id);
            if id == 4 {
                "tests PASS".to_string()
            } else {
                String::new()
            }
        };
        let open = [(1, Status::Idle), (4, Status::Idle)];
        let mut one = text_wait(Watch::One(1), "PASS", &open, "");
        assert_eq!(one.outcome(now, &open, &[], &mut screen), None);
        let mut any = text_wait(Watch::Any, "PASS", &open, "");
        assert_eq!(
            any.outcome(now, &open, &[], &mut screen),
            Some(Outcome::Met(4))
        );
        assert_eq!(reads, vec![1, 1, 4]);
    }

    #[test]
    fn deadlines() {
        let base = Instant::now();
        let mut screen = no_screen();
        let open = [(4, Status::Working)];
        let mut w = Wait::new(
            (),
            None,
            Watch::One(4),
            WaitFor::Status(Status::Done),
            &open,
            Some(at(base, 1000)),
            &mut no_screen(),
        );
        assert_eq!(w.outcome(at(base, 999), &open, &[], &mut screen), None);
        // Met beats the deadline in the same pass.
        assert_eq!(
            w.outcome(at(base, 1000), &[(4, Status::Done)], &[], &mut screen),
            Some(Outcome::Met(4))
        );
        assert_eq!(
            w.outcome(at(base, 1000), &open, &[], &mut screen),
            Some(Outcome::TimedOut)
        );
        // No deadline: never times out.
        let mut w = wait(Watch::One(4), WaitFor::Status(Status::Done), &open);
        assert_eq!(
            w.outcome(at(base, 86_400_000), &open, &[], &mut screen),
            None
        );
    }

    #[test]
    fn replies() {
        assert_eq!(wait_reply(Outcome::Met(4)), Ok(json!({"id": "s4"})));
        assert_eq!(
            cli::render("wait", &wait_reply(Outcome::Met(4)).unwrap(), false),
            "s4"
        );
        assert_eq!(wait_reply(Outcome::TimedOut).unwrap_err().code, "timeout");
        let closed = wait_reply(Outcome::Closed(4)).unwrap_err();
        assert_eq!(closed.code, "not_found");
        assert!(closed.message.contains("s4"), "{}", closed.message);
    }

    #[test]
    fn table_drops_gone_clients_and_takes_out_finished_waits() {
        let now = Instant::now();
        let open = [(1, Status::Working), (4, Status::Working)];
        let mut table = WaitTable::new();
        // conn = (name, gone)
        for (conn, watch) in [
            (("done", false), Watch::One(4)),
            (("pending", false), Watch::One(1)),
            (("gone", true), Watch::One(4)),
        ] {
            table.push(Wait::new(
                conn,
                None,
                watch,
                WaitFor::Status(Status::Done),
                &open,
                None,
                &mut no_screen(),
            ));
        }
        let finished = table.check(
            now,
            &[(1, Status::Working), (4, Status::Done)],
            &[],
            no_screen(),
            |c: &(&str, bool)| c.1,
        );
        let names: Vec<_> = finished.iter().map(|(w, o)| (w.conn.0, *o)).collect();
        assert_eq!(names, vec![("done", Outcome::Met(4))]);
        assert!(!table.is_empty());
        let finished = table.check(now, &[(1, Status::Done)], &[4], no_screen(), |c| c.1);
        assert_eq!(finished.len(), 1);
        assert!(table.is_empty());
    }

    #[test]
    fn table_reads_each_screen_once_per_pass() {
        let now = Instant::now();
        let open = [(4, Status::Idle)];
        let mut table = WaitTable::new();
        for text in ["PASS", "FAIL"] {
            table.push(text_wait(Watch::One(4), text, &open, ""));
        }
        let mut reads = 0;
        let finished = table.check(
            now,
            &open,
            &[],
            |_| {
                reads += 1;
                "all PASS".to_string()
            },
            |_| false,
        );
        assert_eq!(reads, 1);
        assert_eq!(finished.len(), 1);
    }
}
