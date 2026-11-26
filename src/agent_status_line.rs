//! The chat's status line (the 2026-11-24 chores: the status moved out of the folder tab, which
//! it ran past): under the last row while a turn is live, `Cultivating… · <running action> ·
//! 0:31`, led by the plan's task when there is one; `Waiting for you · <question>` while the
//! agent asks; `Done · 0:42` (after the strip's ✓) once the turn ends. On its right, pinned, the
//! ledger's chips (chore 22): `⧉ 4 changed` and `✓ checks`. Pure: the view hands it the facts.

use std::time::{Duration, Instant};

use crate::agent_lanes::clock;
use crate::agent_ledger::{Ledger, Running, SEP};
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
    /// How long the last turn worked (Done).
    pub took: Option<Duration>,
    pub now: Instant,
}

/// A text's first line (the line is one row).
fn first_line(text: &str) -> String {
    text.lines().next().unwrap_or("").trim_end().to_string()
}

/// The line in three: what leads it (the task, the status word), the action (the running
/// call's title, or the question) that gives way with "…" when room runs short, and what
/// trails it (its time, "+N more").
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineParts {
    pub lead: String,
    pub action: Option<String>,
    pub tail: Option<String>,
}

impl LineParts {
    /// The parts as one line (the tests' and the scenes' view of it).
    #[cfg(any(test, feature = "ui-shots"))]
    pub fn joined(&self) -> String {
        std::iter::once(self.lead.clone())
            .chain(self.action.clone())
            .chain(self.tail.clone())
            .collect::<Vec<_>>()
            .join(SEP)
    }
}

/// The line's parts (`LineParts`).
pub fn line_parts(s: &LineState) -> LineParts {
    let joined = |words: Vec<String>| (!words.is_empty()).then(|| words.join(SEP));
    match s.status {
        Status::Waiting => LineParts {
            lead: "Waiting for you".into(),
            action: s.question.map(first_line),
            tail: None,
        },
        // The strip's mark is the ✓.
        Status::Done => LineParts {
            lead: joined(
                std::iter::once("Done".to_string())
                    .chain(s.took.map(clock))
                    .collect(),
            )
            .unwrap_or_default(),
            action: None,
            tail: None,
        },
        Status::Working | Status::Idle => {
            let mut lead: Vec<String> = Vec::new();
            lead.extend(s.task.map(|(at, of)| format!("Task {at}/{of}")));
            lead.push(s.word.to_string());
            let mut tail: Vec<String> = Vec::new();
            if let Some(r) = s.running {
                tail.extend(r.started.map(|t| clock(s.now.saturating_duration_since(t))));
                if r.more > 0 {
                    tail.push(format!("+{} more", r.more));
                }
            }
            LineParts {
                lead: lead.join(SEP),
                action: s.running.map(|r| first_line(&r.title)),
                tail: joined(tail),
            }
        }
    }
}

/// The line's words as one string (its counts are the chips on its right: `chips`).
#[cfg(test)]
pub fn status_line(s: &LineState) -> String {
    line_parts(s).joined()
}

/// A chip on the status row's right (chore 22): what it counts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChipKind {
    /// `⧉ 4 changed`: opens the changed files.
    Changed,
    /// `✓ checks`: opens the check results.
    Checks,
}

/// How a chip stands: its glyph and ink.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChipState {
    /// The changed files' (dim, no state).
    Quiet,
    /// A check runs now: the arc.
    Running,
    /// Every check's latest run passed: ✓.
    Passed,
    /// Checks ran, not all known to pass: a ring.
    Ran,
    /// One fails for real: red, ✕.
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chip {
    pub kind: ChipKind,
    pub state: ChipState,
    pub text: String,
}

/// The status row's chips (chore 22): `N changed` once a file under `root` (the tab's folder)
/// changed (chore 36: those outside it aren't counted); the checks once one ran or runs —
/// running (the arc) while one runs, failing (red, what fails), passing, or ran.
pub fn chips(ledger: &Ledger, root: &std::path::Path) -> Vec<Chip> {
    use crate::agent_activity::CheckWhat;
    let mut out = Vec::new();
    let changed = ledger.changed_in(root);
    if changed > 0 {
        out.push(Chip {
            kind: ChipKind::Changed,
            state: ChipState::Quiet,
            text: format!("{changed} changed"),
        });
    }
    let checks = |state, text: &str| Chip {
        kind: ChipKind::Checks,
        state,
        text: text.to_string(),
    };
    let chip = match ledger.failing {
        _ if ledger.checking => Some(checks(ChipState::Running, "checks")),
        Some(CheckWhat::Tests) => Some(checks(ChipState::Failed, "tests")),
        Some(CheckWhat::Build) => Some(checks(ChipState::Failed, "build")),
        Some(CheckWhat::Lint) => Some(checks(ChipState::Failed, "lint")),
        None if ledger.scopes.is_empty() => None,
        None if ledger
            .scopes
            .iter()
            .all(crate::agent_ledger::Scope::passing) =>
        {
            Some(checks(ChipState::Passed, "checks"))
        }
        None => Some(checks(ChipState::Ran, "checks")),
    };
    out.extend(chip);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_activity::{CheckWhat, Outcome};
    use crate::agent_ledger::Scope;

    fn state<'a>(now: Instant) -> LineState<'a> {
        LineState {
            word: "Cultivating…",
            status: Status::Working,
            question: None,
            task: None,
            running: None,
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
            status_line(&s),
            "Cultivating… · Run the full e2e suite · 0:31"
        );
    }

    #[test]
    fn working_with_nothing_running_is_the_word_alone() {
        assert_eq!(status_line(&state(Instant::now())), "Cultivating…");
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
            status_line(&s),
            "Cultivating… · Run the full e2e suite · 0:31 · +2 more"
        );
    }

    /// The review of chores 17–22 (finding 3): only the action gives way ("…"); the word before
    /// it and the time and "+N more" after it stay.
    #[test]
    fn the_line_splits_round_its_action() {
        let now = Instant::now();
        let r = running(now, 2);
        let s = LineState {
            running: Some(&r),
            task: Some((5, 6)),
            ..state(now)
        };
        assert_eq!(
            line_parts(&s),
            LineParts {
                lead: "Task 5/6 · Cultivating…".into(),
                action: Some("Run the full e2e suite".into()),
                tail: Some("0:31 · +2 more".into()),
            }
        );
        let asking = LineState {
            status: Status::Waiting,
            question: Some("Allow Bash?"),
            ..state(now)
        };
        assert_eq!(
            line_parts(&asking),
            LineParts {
                lead: "Waiting for you".into(),
                action: Some("Allow Bash?".into()),
                tail: None,
            }
        );
        let done = LineState {
            status: Status::Done,
            took: Some(Duration::from_secs(42)),
            ..state(now)
        };
        assert_eq!(line_parts(&done).joined(), "Done · 0:42");
        assert_eq!(line_parts(&s).joined(), status_line(&s));
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
            status_line(&s),
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
        assert_eq!(status_line(&s), "Waiting for you · Allow Bash?");
        let bare = LineState {
            question: None,
            ..s
        };
        assert_eq!(status_line(&bare), "Waiting for you");
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
            status_line(&s),
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
        assert_eq!(status_line(&s), "Cultivating… · Run a script");
    }

    #[test]
    fn done_says_done_and_the_time() {
        let s = LineState {
            status: Status::Done,
            took: Some(Duration::from_secs(42)),
            ..state(Instant::now())
        };
        assert_eq!(status_line(&s), "Done · 0:42");
        let untimed = LineState { took: None, ..s };
        assert_eq!(status_line(&untimed), "Done");
    }

    fn ledger(changed: usize, runs: &[Outcome], checking: bool) -> Ledger {
        Ledger {
            running: None,
            changed: (0..changed)
                .map(|n| crate::agent_ledger::Changed {
                    path: format!("/tmp/fx/code/my-app/src/f{n}.rs").into(),
                    plus: None,
                    minus: None,
                })
                .collect(),
            scripted: Vec::new(),
            read: Vec::new(),
            checks: Vec::new(),
            scopes: if runs.is_empty() {
                Vec::new()
            } else {
                vec![Scope {
                    what: CheckWhat::Tests,
                    scope: "cargo test".into(),
                    runs: runs.iter().map(|o| (o.clone(), false)).collect(),
                }]
            },
            passed: 0,
            total: 0,
            failing: None,
            checking,
        }
    }

    const ROOT: &str = "/tmp/fx/code/my-app";

    fn chips_in(ledger: &Ledger) -> Vec<Chip> {
        chips(ledger, std::path::Path::new(ROOT))
    }

    /// Chore 36: a file outside the tab's folder (an agent's log in /tmp) isn't counted; with
    /// only such files changed there is no changed chip.
    #[test]
    fn files_outside_the_project_are_not_counted() {
        let outside = |path: &str| crate::agent_ledger::Changed {
            path: path.into(),
            plus: None,
            minus: None,
        };
        let mut l = ledger(1, &[], false);
        l.changed.push(outside("/tmp/fx/logs/run.log"));
        l.changed.push(outside("/tmp/fx/code/my-app-old/x.rs"));
        assert_eq!(
            chips_in(&l),
            [chip(ChipKind::Changed, ChipState::Quiet, "1 changed")]
        );
        l.changed.remove(0);
        assert!(chips_in(&l).is_empty(), "only outside: {:?}", chips_in(&l));
    }

    fn chip(kind: ChipKind, state: ChipState, text: &str) -> Chip {
        Chip {
            kind,
            state,
            text: text.into(),
        }
    }

    /// Chore 22: the counts are chips on the row's right, as soon as there is something to say.
    #[test]
    fn chips_say_changed_and_how_the_checks_stand() {
        let passed = Outcome::Passed { n: Some(2) };
        assert_eq!(
            chips_in(&ledger(4, std::slice::from_ref(&passed), false)),
            [
                chip(ChipKind::Changed, ChipState::Quiet, "4 changed"),
                chip(ChipKind::Checks, ChipState::Passed, "checks"),
            ]
        );
        assert!(chips_in(&ledger(0, &[], false)).is_empty(), "nothing yet");
        assert_eq!(
            chips_in(&ledger(0, &[], true)),
            [chip(ChipKind::Checks, ChipState::Running, "checks")],
            "a check running, none finished yet"
        );
        assert_eq!(
            chips_in(&ledger(1, std::slice::from_ref(&passed), true))[1],
            chip(ChipKind::Checks, ChipState::Running, "checks"),
            "running again"
        );
        assert_eq!(
            chips_in(&ledger(0, &[Outcome::Unknown], false)),
            [chip(ChipKind::Checks, ChipState::Ran, "checks")]
        );
        let mut failing = ledger(0, &[Outcome::Unknown], false);
        failing.failing = Some(CheckWhat::Build);
        assert_eq!(
            chips_in(&failing),
            [chip(ChipKind::Checks, ChipState::Failed, "build")]
        );
    }
}
