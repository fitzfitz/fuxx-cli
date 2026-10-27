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
    pub deadline: Option<Instant>,
}

impl<C> Wait<C> {
    /// `sessions`: every open session's (id, status), in sidebar order.
    pub fn new(
        conn: C,
        request: Option<u64>,
        watch: Watch,
        cond: WaitFor,
        sessions: &[(u64, Status)],
        deadline: Option<Instant>,
    ) -> Self {
        let start = sessions
            .iter()
            .filter(|(id, _)| watch == Watch::Any || watch == Watch::One(*id))
            .copied()
            .collect();
        Self {
            conn,
            request,
            watch,
            cond,
            start,
            deadline,
        }
    }

    /// Whether the wait ends now, and how. `sessions`: every open session's (id, status) in
    /// sidebar order; `closed`: sessions that closed since the last check; `screen`: a
    /// session's last lines (read only for text waits).
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
                if closed.contains(&id) {
                    return Some(if exit_counts {
                        Outcome::Met(id)
                    } else {
                        Outcome::Closed(id)
                    });
                }
                let Some(&(_, now)) = sessions.iter().find(|(s, _)| *s == id) else {
                    return Some(Outcome::Closed(id));
                };
                self.holds(id, now, screen).then_some(Outcome::Met(id))
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

    /// Whether the condition holds for an open session. One first seen now starts from its
    /// current status.
    fn holds(&mut self, id: u64, now: Status, screen: &mut dyn FnMut(u64) -> String) -> bool {
        let start = match self.start.iter().find(|(s, _)| *s == id) {
            Some(&(_, start)) => start,
            None => {
                self.start.push((id, now));
                now
            }
        };
        let text = match self.cond {
            WaitFor::Text(_) => screen(id),
            _ => String::new(),
        };
        wait_met(&self.cond, start, now, &text, false)
    }
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
        assert!(note.ends_with("  notification build"), "{note}");
    }

    fn at(base: Instant, ms: u64) -> Instant {
        base + Duration::from_millis(ms)
    }

    fn no_screen() -> impl FnMut(u64) -> String {
        |id| panic!("screen of s{id} read for a non-text wait")
    }

    fn wait(watch: Watch, cond: WaitFor, sessions: &[(u64, Status)]) -> Wait<()> {
        Wait::new((), Some(1), watch, cond, sessions, None)
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
    fn any_change_counts_from_when_a_session_is_first_seen() {
        let now = Instant::now();
        let mut screen = no_screen();
        let mut w = wait(Watch::Any, WaitFor::Change, &[(1, Status::Working)]);
        // s7 opened after the wait began: its first status is its start, not a change.
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
        let mut one = wait(Watch::One(1), WaitFor::Text("PASS".into()), &open);
        assert_eq!(one.outcome(now, &open, &[], &mut screen), None);
        let mut any = wait(Watch::Any, WaitFor::Text("PASS".into()), &open);
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
            table.push(Wait::new(
                (),
                None,
                Watch::One(4),
                WaitFor::Text(text.into()),
                &open,
                None,
            ));
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
