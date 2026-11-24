//! The chat's status line (the 2026-11-24 chores: the status moved out of the folder tab, which
//! it ran past): under the last row while a turn is live, `Cultivating… · <running action> ·
//! 0:31`, led by the plan's task when there is one; `Waiting for you · <question>` while the
//! agent asks; `Done · 0:42` (after the strip's ✓) once the turn ends. Under it a dim counts line, `4 changed ·
//! checks 3/7`, from the ledger. Pure: the view hands it the facts.

use std::time::{Duration, Instant};

use crate::agent_lanes::clock;
use crate::agent_ledger::{Part, PartKind, Running, SEP};
use crate::status::Status;

/// What the line is made from.
#[derive(Clone, Debug)]
pub struct LineState<'a> {
    /// The status word as the tab's pill would say it ("Cultivating…").
    pub word: &'a str,
    pub status: Status,
    /// The open question, while Waiting.
    pub question: Option<&'a str>,
    /// The plan's task being built, and how many there are (`Task 5/6`).
    pub task: Option<(usize, usize)>,
    /// The call running now.
    pub running: Option<&'a Running>,
    /// The ledger's parts (only `Changed` and `Checks` are read).
    pub parts: &'a [Part],
    /// How long the last turn worked (Done).
    pub took: Option<Duration>,
    pub now: Instant,
}

/// A text's first line (the line is one row).
fn first_line(text: &str) -> String {
    text.lines().next().unwrap_or("").trim_end().to_string()
}

/// The main line, and the counts line under it (none with nothing to count).
pub fn status_line(s: &LineState) -> (String, Option<String>) {
    let main = match s.status {
        Status::Waiting => {
            let mut words = vec!["Waiting for you".to_string()];
            words.extend(s.question.map(first_line));
            words.join(SEP)
        }
        Status::Done => {
            // The strip's mark is the ✓.
            let mut words = vec!["Done".to_string()];
            words.extend(s.took.map(clock));
            words.join(SEP)
        }
        Status::Working | Status::Idle => {
            let mut words: Vec<String> = Vec::new();
            words.extend(s.task.map(|(at, of)| format!("Task {at}/{of}")));
            words.push(s.word.to_string());
            if let Some(r) = s.running {
                words.push(first_line(&r.title));
                words.extend(r.started.map(|t| clock(s.now.saturating_duration_since(t))));
                if r.more > 0 {
                    words.push(format!("+{} more", r.more));
                }
            }
            words.join(SEP)
        }
    };
    let counts: Vec<&str> = s
        .parts
        .iter()
        .filter(|p| matches!(p.kind, PartKind::Changed | PartKind::Checks))
        .map(|p| p.text.as_str())
        .collect();
    (main, (!counts.is_empty()).then(|| counts.join(SEP)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn part(kind: PartKind, text: &str) -> Part {
        Part {
            kind,
            text: text.into(),
        }
    }

    fn state<'a>(now: Instant) -> LineState<'a> {
        LineState {
            word: "Cultivating…",
            status: Status::Working,
            question: None,
            task: None,
            running: None,
            parts: &[],
            took: None,
            now,
        }
    }

    fn running(now: Instant, more: usize) -> Running {
        Running {
            id: "t1".into(),
            title: "Run the full e2e suite".into(),
            started: Some(now - Duration::from_secs(31)),
            more,
        }
    }

    #[test]
    fn working_names_the_running_action_and_its_time() {
        let now = Instant::now();
        let r = running(now, 0);
        let s = LineState {
            running: Some(&r),
            ..state(now)
        };
        assert_eq!(
            status_line(&s).0,
            "Cultivating… · Run the full e2e suite · 0:31"
        );
    }

    #[test]
    fn working_with_nothing_running_is_the_word_alone() {
        assert_eq!(status_line(&state(Instant::now())).0, "Cultivating…");
    }

    #[test]
    fn parallel_actions_say_plus_more() {
        let now = Instant::now();
        let r = running(now, 2);
        let s = LineState {
            running: Some(&r),
            ..state(now)
        };
        assert_eq!(
            status_line(&s).0,
            "Cultivating… · Run the full e2e suite · 0:31 · +2 more"
        );
    }

    #[test]
    fn a_plan_leads_with_the_task() {
        let now = Instant::now();
        let r = running(now, 0);
        let s = LineState {
            running: Some(&r),
            task: Some((5, 6)),
            ..state(now)
        };
        assert_eq!(
            status_line(&s).0,
            "Task 5/6 · Cultivating… · Run the full e2e suite · 0:31"
        );
    }

    #[test]
    fn waiting_says_waiting_for_you() {
        let s = LineState {
            status: Status::Waiting,
            question: Some("Allow Bash?"),
            ..state(Instant::now())
        };
        assert_eq!(status_line(&s).0, "Waiting for you · Allow Bash?");
        let bare = LineState {
            question: None,
            ..s
        };
        assert_eq!(status_line(&bare).0, "Waiting for you");
    }

    /// A permission's command or a call's title may run over lines (a heredoc): the line keeps
    /// its first (the review's finding 11).
    #[test]
    fn multi_line_words_keep_their_first_line() {
        let s = LineState {
            status: Status::Waiting,
            question: Some("Claude wants to run: cat <<EOF\nhello\nEOF"),
            ..state(Instant::now())
        };
        assert_eq!(
            status_line(&s).0,
            "Waiting for you · Claude wants to run: cat <<EOF"
        );
        let now = Instant::now();
        let r = Running {
            title: "Run a script\nwith two lines".into(),
            started: None,
            ..running(now, 0)
        };
        let s = LineState {
            running: Some(&r),
            ..state(now)
        };
        assert_eq!(status_line(&s).0, "Cultivating… · Run a script");
    }

    #[test]
    fn done_says_done_and_the_time() {
        let s = LineState {
            status: Status::Done,
            took: Some(Duration::from_secs(42)),
            ..state(Instant::now())
        };
        assert_eq!(status_line(&s).0, "Done · 0:42");
        let untimed = LineState { took: None, ..s };
        assert_eq!(status_line(&untimed).0, "Done");
    }

    #[test]
    fn counts_line_joins_changed_and_checks() {
        let parts = [
            part(PartKind::Running, "Run the full e2e suite 0:31"),
            part(PartKind::Changed, "4 changed"),
            part(PartKind::Checks, "checks pass"),
        ];
        let s = LineState {
            parts: &parts,
            ..state(Instant::now())
        };
        assert_eq!(
            status_line(&s).1.as_deref(),
            Some("4 changed · checks pass")
        );
        assert_eq!(status_line(&state(Instant::now())).1, None);
    }
}
