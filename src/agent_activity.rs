//! The agent activity model (the agent activity chore): one provider-neutral `Activity` per tool
//! call, shaped after ACP's tool call (`kind`, `locations`, `content`, `status`), so a provider
//! fills it once and the views only read it. Providers build it from their own events
//! (`src/providers/<agent>.rs`) through the shared readers in `agent_activity_command` and
//! `agent_activity_checks`.

use std::borrow::Cow;

use crate::agent_activity_checks::{first_line, outcome, strip_ansi};
use crate::agent_activity_command::{read_command, scope_checks, scope_filter};
use crate::agent_conversation::Item;
use crate::diff_view::{RowKind, parse_diff};
use crate::providers::ToolDetail;

/// What kind of thing a call does (ACP's `ToolKind`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ToolKind {
    Read,
    Edit,
    Delete,
    Move,
    Search,
    Execute,
    Fetch,
    #[allow(dead_code)] // ACP's kind: an ACP provider sends it (fuxx's two providers don't).
    Think,
    #[default]
    Other,
}

/// How a call touched a file.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum How {
    Read {
        lines: Option<(u32, u32)>,
    },
    /// `at`: only from a sed address (a diff doesn't know its line).
    Changed {
        plus: Option<u32>,
        minus: Option<u32>,
        at: Option<u32>,
    },
    /// Named inside a script: it may only read it, so never "changed".
    ScriptTouched,
    Searched {
        matches: Option<u32>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Touch {
    pub path: String,
    pub how: How,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CheckWhat {
    Tests,
    Build,
    Lint,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Outcome {
    Passed {
        n: Option<u32>,
    },
    Failed {
        failed: Option<u32>,
        of: Option<u32>,
        first_error: Option<String>,
    },
    Unknown,
}

/// A check a call ran: what, on which scope (the command normalised: runner, target, filter),
/// and how it went.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Check {
    pub what: CheckWhat,
    pub scope: String,
    pub outcome: Outcome,
}

/// One tool call, as the activity view reads it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Activity {
    pub kind: ToolKind,
    /// The agent's own words (Claude's Bash `description`, agy's `toolSummary`).
    pub said: Option<String>,
    /// fuxx's verb + object ("Read api_client.rs 80–400").
    pub derived: String,
    pub touches: Vec<Touch>,
    /// The call itself failed (not a check): its first error line.
    pub failed: Option<String>,
    /// Denied (a stopped call is read from the item: not running, `ok` None).
    pub not_run: bool,
    pub check: Option<Check>,
    /// The check's failure was announced (the narration or the call's own words).
    pub planned_failure: bool,
    pub exit: Option<i32>,
}

impl Activity {
    /// The agent's words, else fuxx's.
    pub fn title(&self) -> &str {
        self.said
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or(&self.derived)
    }

    /// While it runs: the agent's words, else the present tense ("Running tests").
    pub fn running_title(&self) -> String {
        if let Some(said) = self.said.as_deref().filter(|s| !s.trim().is_empty()) {
            return said.to_string();
        }
        if let Some(check) = &self.check {
            return match check.what {
                CheckWhat::Tests => "Running tests",
                CheckWhat::Build => "Building",
                CheckWhat::Lint => "Running lint",
            }
            .to_string();
        }
        let past = if self.derived.is_empty() {
            derived_words(self.kind, &self.touches, None, "")
        } else {
            self.derived.clone()
        };
        let (verb, rest) = past.split_once(' ').unwrap_or((&past, ""));
        let now = match verb {
            "Read" => "Reading",
            "Edited" => "Editing",
            "Deleted" => "Deleting",
            "Moved" => "Moving",
            "Searched" => "Searching",
            "Listed" => "Listing",
            "Ran" => "Running",
            "Fetched" => "Fetching",
            "Thought" => "Thinking",
            _ => return past,
        };
        if rest.is_empty() {
            now.to_string()
        } else {
            format!("{now} {rest}")
        }
    }

    /// The files it changed: none when it failed or didn't run.
    pub fn changed(&self) -> impl Iterator<Item = &Touch> {
        let counts = self.failed.is_none() && !self.not_run;
        self.touches
            .iter()
            .filter(move |t| counts && matches!(t.how, How::Changed { .. }))
    }

    /// A failure that isn't planned and isn't a denial.
    pub fn real_failure(&self) -> bool {
        !self.not_run && (self.failed.is_some() || self.check_failed() && !self.planned_failure)
    }

    /// It ran a check that failed.
    pub fn check_failed(&self) -> bool {
        matches!(
            self.check,
            Some(Check {
                outcome: Outcome::Failed { .. },
                ..
            })
        )
    }
}

/// How a call ended, as its provider reports it.
pub struct Finish<'a> {
    pub ok: bool,
    pub exit: Option<i32>,
    pub output: &'a str,
    /// The provider's own first error line, when it knows one.
    pub error: Option<String>,
    pub not_run: bool,
    /// The exit code isn't the check's own (a pipe's last, or a command after it: `; echo`,
    /// `|| true`): `Reading::exit_trusted` false.
    pub exit_hidden: bool,
}

/// The call's activity when it ends: a check's outcome read from its output, else a failed
/// call's first error line.
pub fn finished(start: Activity, end: Finish<'_>) -> Activity {
    let mut a = start;
    a.exit = end.exit;
    a.not_run = end.not_run;
    let exit_failed = end.exit.is_some_and(|c| c != 0);
    match &mut a.check {
        Some(check) if !end.not_run => {
            check.outcome = outcome(check.what, end.output, end.exit, end.ok, end.exit_hidden);
        }
        Some(_) => {}
        // grep / rg exit 1 when nothing matched: no failure, no matches.
        None if a.kind == ToolKind::Search
            && end.exit == Some(1)
            && end.output.trim().is_empty()
            && a.touches
                .iter()
                .all(|t| matches!(t.how, How::Searched { .. })) =>
        {
            for t in &mut a.touches {
                t.how = How::Searched { matches: Some(0) };
            }
        }
        None if !end.ok || exit_failed => {
            a.failed = Some(
                end.error
                    .or_else(|| first_line(&strip_ansi(end.output)))
                    .unwrap_or_else(|| "failed".into()),
            );
        }
        None => {}
    }
    a
}

/// A path's file name.
pub fn file_name(path: &str) -> &str {
    path.trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or(path)
}

/// fuxx's verb + object for a call ("Read api_client.rs 80–400", "Edited api.rs:543", "Ran
/// tests cli::", "Searched 2 files for METHODS", "Fetched docs.rs").
pub fn derived_words(
    kind: ToolKind,
    touches: &[Touch],
    check: Option<&Check>,
    object: &str,
) -> String {
    let one = |verb: &str, many: &str| match touches {
        [] if object.is_empty() => verb.to_string(),
        [] => format!("{verb} {object}"),
        [t] => format!("{verb} {}{}", file_name(&t.path), detail(&t.how)),
        _ => format!("{verb} {} {many}", touches.len()),
    };
    if let Some(check) = check {
        let what = what_word(check.what);
        let whats = scope_checks(&check.scope);
        if whats.len() > 1 {
            let mut words: Vec<&str> = whats.into_iter().map(what_word).collect();
            words.dedup();
            return format!("Ran {}", words.join(" + "));
        }
        let filter = scope_filter(&check.scope);
        return if filter.is_empty() {
            format!("Ran {what}")
        } else {
            format!("Ran {what} {filter}")
        };
    }
    match kind {
        ToolKind::Read => one("Read", "files"),
        ToolKind::Edit => one("Edited", "files"),
        ToolKind::Delete => one("Deleted", "files"),
        ToolKind::Move => match touches {
            [from, to] => format!("Moved {} to {}", file_name(&from.path), file_name(&to.path)),
            _ => one("Moved", "files"),
        },
        ToolKind::Search => {
            let what = if object.is_empty() {
                String::new()
            } else {
                format!(" for {object}")
            };
            match touches {
                [] => format!("Searched{what}"),
                [t] => format!("Searched {}{what}", file_name(&t.path)),
                _ => format!("Searched {} files{what}", touches.len()),
            }
        }
        ToolKind::Execute => {
            let scripted: Vec<&Touch> = touches
                .iter()
                .filter(|t| t.how == How::ScriptTouched)
                .collect();
            match scripted.as_slice() {
                [t] => format!("Ran a script on {}", file_name(&t.path)),
                [_, ..] => format!("Ran a script on {} files", scripted.len()),
                [] if object.is_empty() => "Ran a command".into(),
                [] => format!("Ran {object}"),
            }
        }
        ToolKind::Fetch => match object.split_once("://") {
            Some((_, rest)) => format!("Fetched {}", rest.split('/').next().unwrap_or(rest)),
            None if object.is_empty() => "Fetched a page".into(),
            None => format!("Fetched {object}"),
        },
        ToolKind::Think => "Thought".into(),
        ToolKind::Other if object.is_empty() => "Used a tool".into(),
        ToolKind::Other => object.to_string(),
    }
}

/// A check's kind in a word ("tests", "build", "lint").
pub fn what_word(what: CheckWhat) -> &'static str {
    match what {
        CheckWhat::Tests => "tests",
        CheckWhat::Build => "build",
        CheckWhat::Lint => "lint",
    }
}

/// The line part of a touch in words: " 80–400" for a read range, ":543" for a sed edit.
fn detail(how: &How) -> String {
    match how {
        How::Read {
            lines: Some((a, b)),
        } if a == b => format!(" {a}"),
        How::Read {
            lines: Some((a, b)),
        } => format!(" {a}–{b}"),
        How::Changed { at: Some(n), .. } => format!(":{n}"),
        _ => String::new(),
    }
}

/// An activity for a call its provider sent none for, from the item's own fields.
pub fn fallback(
    name: &str,
    summary: &str,
    detail: Option<&ToolDetail>,
    ok: Option<bool>,
    output: Option<&str>,
) -> Activity {
    let (start, exit_hidden) = match detail {
        Some(ToolDetail::Command(cmd)) => {
            let r = read_command(cmd);
            let check = r.check.map(|(what, scope)| Check {
                what,
                scope,
                outcome: Outcome::Unknown,
            });
            let a = Activity {
                kind: r.kind,
                derived: r.derived,
                touches: r.touches,
                check,
                ..Default::default()
            };
            (a, !r.exit_trusted)
        }
        Some(ToolDetail::Diff { path, unified }) => {
            let rows = parse_diff(unified).rows;
            let count = |k: RowKind| rows.iter().filter(|r| r.kind == k).count() as u32;
            let touches = vec![Touch {
                path: path.clone(),
                how: How::Changed {
                    plus: Some(count(RowKind::Added)),
                    minus: Some(count(RowKind::Removed)),
                    at: None,
                },
            }];
            let derived = derived_words(ToolKind::Edit, &touches, None, "");
            (
                Activity {
                    kind: ToolKind::Edit,
                    derived,
                    touches,
                    ..Default::default()
                },
                false,
            )
        }
        None => {
            let lower = name.to_ascii_lowercase();
            let kind = if lower.contains("read") {
                ToolKind::Read
            } else if lower.contains("grep") || lower.contains("glob") || lower.contains("search") {
                ToolKind::Search
            } else if lower.contains("fetch") {
                ToolKind::Fetch
            } else {
                ToolKind::Other
            };
            let object = if kind == ToolKind::Other {
                name
            } else {
                summary
            };
            let derived = derived_words(kind, &[], None, object);
            (
                Activity {
                    kind,
                    derived,
                    ..Default::default()
                },
                false,
            )
        }
    };
    match ok {
        Some(ok) => finished(
            start,
            Finish {
                ok,
                exit: None,
                output: output.unwrap_or(""),
                error: None,
                not_run: false,
                exit_hidden,
            },
        ),
        None => start,
    }
}

/// A tool item's activity: the one its provider sent, else a fallback. None: not a tool.
pub fn of(item: &Item) -> Option<Cow<'_, Activity>> {
    match item {
        Item::Tool {
            activity: Some(a), ..
        } => Some(Cow::Borrowed(&**a)),
        Item::Tool {
            name,
            summary,
            detail,
            ok,
            output,
            ..
        } => Some(Cow::Owned(fallback(
            name,
            summary,
            detail.as_ref(),
            *ok,
            output.as_deref(),
        ))),
        _ => None,
    }
}

/// Whether the agent's words announce a failure ("red first", "(red)", "failing test first",
/// "should fail", "expect … to fail", "TDD").
pub fn announces_failure(text: &str) -> bool {
    let lower = text.to_lowercase();
    if [
        "red first",
        "(red)",
        "failing first",
        "failing test first",
        "should fail",
    ]
    .iter()
    .any(|k| lower.contains(k))
    {
        return true;
    }
    if lower.split(['.', '!', '?', '\n']).any(|s| {
        s.find("expect")
            .is_some_and(|at| s[at..].contains("to fail"))
    }) {
        return true;
    }
    lower
        .split(|c: char| !c.is_alphanumeric())
        .any(|w| w == "tdd")
}

#[cfg(test)]
mod tests {
    use super::*;

    const CARGO_TEST_FAILED_TAIL: &str = include_str!("fixtures/checks/cargo-test-failed-tail.txt");

    #[test]
    fn a_search_with_no_match_is_not_a_failure() {
        let r = read_command("rg -n NOPE src/api.rs");
        let start = Activity {
            kind: r.kind,
            derived: r.derived,
            touches: r.touches,
            ..Default::default()
        };
        let a = finished(
            start,
            Finish {
                ok: false,
                exit: Some(1),
                output: "",
                error: None,
                not_run: false,
                exit_hidden: false,
            },
        );
        assert_eq!(a.failed, None);
        assert_eq!(a.touches[0].how, How::Searched { matches: Some(0) });
    }

    #[test]
    fn the_agents_words_lead_the_title() {
        let mut a = Activity {
            derived: "Ran tests cli::".into(),
            said: Some("Run the CLI tests".into()),
            ..Default::default()
        };
        assert_eq!(a.title(), "Run the CLI tests");
        a.said = None;
        assert_eq!(a.title(), "Ran tests cli::");
    }

    #[test]
    fn derived_words_say_verb_and_object() {
        let t = |p: &str, how| {
            vec![Touch {
                path: p.into(),
                how,
            }]
        };
        assert_eq!(
            derived_words(
                ToolKind::Read,
                &t(
                    "/tmp/fx/code/my-app/src/api_client.rs",
                    How::Read {
                        lines: Some((80, 400))
                    }
                ),
                None,
                ""
            ),
            "Read api_client.rs 80–400"
        );
        assert_eq!(
            derived_words(
                ToolKind::Edit,
                &t(
                    "src/api.rs",
                    How::Changed {
                        plus: None,
                        minus: None,
                        at: Some(543)
                    }
                ),
                None,
                ""
            ),
            "Edited api.rs:543"
        );
        let tests = Check {
            what: CheckWhat::Tests,
            scope: "cargo test --bin fuxx cli::".into(),
            outcome: Outcome::Unknown,
        };
        assert_eq!(
            derived_words(ToolKind::Execute, &[], Some(&tests), ""),
            "Ran tests cli::"
        );
        let two = [
            Touch {
                path: "src/a.rs".into(),
                how: How::Searched { matches: None },
            },
            Touch {
                path: "src/b.rs".into(),
                how: How::Searched { matches: None },
            },
        ];
        assert_eq!(
            derived_words(ToolKind::Search, &two, None, "METHODS"),
            "Searched 2 files for METHODS"
        );
        assert_eq!(
            derived_words(ToolKind::Fetch, &[], None, "https://docs.rs/gpui"),
            "Fetched docs.rs"
        );
    }

    #[test]
    fn running_titles_use_the_present() {
        let tests = Check {
            what: CheckWhat::Tests,
            scope: "cargo test".into(),
            outcome: Outcome::Unknown,
        };
        assert_eq!(
            Activity {
                kind: ToolKind::Execute,
                check: Some(tests),
                ..Default::default()
            }
            .running_title(),
            "Running tests"
        );
        let read = vec![Touch {
            path: "src/api.rs".into(),
            how: How::Read { lines: None },
        }];
        assert_eq!(
            Activity {
                kind: ToolKind::Read,
                touches: read,
                ..Default::default()
            }
            .running_title(),
            "Reading api.rs"
        );
    }

    #[test]
    fn a_failed_call_changes_nothing() {
        let edit = vec![Touch {
            path: "src/api.rs".into(),
            how: How::Changed {
                plus: Some(1),
                minus: Some(0),
                at: None,
            },
        }];
        let mut a = Activity {
            kind: ToolKind::Edit,
            touches: edit,
            ..Default::default()
        };
        assert_eq!(a.changed().count(), 1);
        a.failed = Some("String to replace not found in file.".into());
        assert_eq!(a.changed().count(), 0);
        assert!(a.real_failure());
        a.not_run = true;
        assert!(!a.real_failure());
    }

    #[test]
    fn failure_announcements_are_read_from_the_agents_words() {
        for yes in [
            "Write the failing test first (red first).",
            "Run it (red)",
            "Run the failing test first",
            "This should fail",
            "I expect the new test to fail",
            "TDD: add the test",
        ] {
            assert!(announces_failure(yes), "{yes}");
        }
        for no in [
            "Run the tests",
            "Fix the failing build",
            "Expected output",
            "Read the red theme",
        ] {
            assert!(!announces_failure(no), "{no}");
        }
    }

    #[test]
    fn finishing_reads_a_check_s_outcome_and_a_plain_command_s_exit() {
        let check = Check {
            what: CheckWhat::Tests,
            scope: "cargo test".into(),
            outcome: Outcome::Unknown,
        };
        let start = Activity {
            kind: ToolKind::Execute,
            check: Some(check),
            ..Default::default()
        };
        let done = finished(
            start,
            Finish {
                ok: true,
                exit: None,
                output: CARGO_TEST_FAILED_TAIL,
                error: None,
                not_run: false,
                exit_hidden: true,
            },
        );
        assert!(done.check_failed() && done.failed.is_none());
        let plain = Activity {
            kind: ToolKind::Execute,
            derived: "Ran ls".into(),
            ..Default::default()
        };
        let done = finished(
            plain,
            Finish {
                ok: false,
                exit: Some(2),
                output: "ls: nope: No such file or directory",
                error: None,
                not_run: false,
                exit_hidden: false,
            },
        );
        assert_eq!(
            (done.failed.as_deref(), done.exit),
            (Some("ls: nope: No such file or directory"), Some(2))
        );
    }

    #[test]
    fn a_provider_that_sends_none_gets_a_fallback() {
        let diff = ToolDetail::Diff {
            path: "/tmp/fx/code/my-app/src/api.rs".into(),
            unified: "@@ -1,1 +1,2 @@\n-a\n+b\n+c\n".into(),
        };
        let a = fallback("Edit", "api.rs", Some(&diff), Some(true), None);
        assert_eq!(a.kind, ToolKind::Edit);
        assert_eq!(a.changed().count(), 1);
        let cmd = ToolDetail::Command("cargo test".into());
        assert_eq!(
            fallback(
                "Bash",
                "cargo test",
                Some(&cmd),
                Some(true),
                Some("test result: ok. 2 passed; 0 failed")
            )
            .check
            .map(|c| c.outcome),
            Some(Outcome::Passed { n: Some(2) })
        );
    }
}
