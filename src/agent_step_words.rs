//! The step model's words (the agent activity chore): a step's header, a file track's and the
//! checks track's summaries, a call's outcome phrase and hover words, and a readable row's
//! title, trace and chips. Pure and GPUI-free; split out of `agent_steps` (the review's lean
//! files).

use std::path::Path;
use std::time::Duration;

use crate::agent_activity::{
    Activity, Check, CheckWhat, How, Outcome, ToolKind, Touch, of, what_word,
};
use crate::agent_activity_checks::error_code;
use crate::agent_activity_command::scope_filter;
use crate::agent_ask::cut;
use crate::agent_conversation::Item;
use crate::agent_lanes::clock;
use crate::agent_steps::check_key;
use crate::providers::ToolDetail;

/// The longest command trace a readable row keeps, in characters (the row truncates it to its
/// width; a huge script needn't be carried whole).
pub const TRACE_MAX: usize = 160;

/// The longest first error a failed call's chip shows, in characters.
pub const CHIP_ERROR_MAX: usize = 40;

/// The header's time: how long the step took, when known and at least a second.
pub fn head_time(took: Option<Duration>) -> Option<String> {
    took.filter(|d| d.as_secs() >= 1).map(clock)
}

/// The header: the counts, then the time (`head_time`).
#[cfg(test)]
pub fn header(counts: &str, took: Option<Duration>) -> String {
    match head_time(took) {
        Some(t) => format!("{counts} · {t}"),
        None => counts.to_string(),
    }
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// "8 actions · 5 files" (the header's "N failed" is drawn apart, in the failure's ink).
pub fn counts(actions: usize, files: usize) -> String {
    format!(
        "{} · {}",
        plural(actions, "action", "actions"),
        plural(files, "file", "files")
    )
}

/// The Commands track's summary: "2 commands · 1 failed".
pub fn commands_summary(n: usize, failed: usize) -> String {
    let mut out = plural(n, "command", "commands");
    if failed > 0 {
        out.push_str(&format!(" · {failed} failed"));
    }
    out
}

/// A failure the agent planned (a test written first), in words (the 2026-11-24 chores: "I
/// don't understand the state at all").
pub const PLANNED: &str = "expected — test written first";

/// A file track's summary from its touches (each with whether that call failed or didn't run,
/// and the call's kind): "edited at :543 · +1 −0", "read 80–400, 78–120 · script 2×",
/// "touched by a script", "searched · 3 matches", "edit failed".
pub fn file_summary(touches: &[(&Touch, bool, ToolKind)]) -> String {
    let good: Vec<&(&Touch, bool, ToolKind)> =
        touches.iter().filter(|(_, failed, _)| !failed).collect();
    if good.is_empty() {
        let Some((t, _, kind)) = touches.first() else {
            return String::new();
        };
        let verb = match (&t.how, kind) {
            (How::Changed { .. }, ToolKind::Delete) => "delete",
            (How::Changed { .. }, ToolKind::Move) => "move",
            (How::Changed { .. }, _) => "edit",
            (How::Read { .. }, _) => "read",
            (How::Searched { .. }, _) => "search",
            (How::ScriptTouched, _) => "script",
        };
        return format!("{verb} failed");
    }
    let mut parts: Vec<String> = Vec::new();
    let changes: Vec<_> = good
        .iter()
        .filter(|(t, _, _)| matches!(t.how, How::Changed { .. }))
        .collect();
    if !changes.is_empty() {
        let verb = match changes[0].2 {
            ToolKind::Delete => "deleted",
            ToolKind::Move => "moved",
            _ => "edited",
        };
        let mut part = verb.to_string();
        let (mut plus, mut minus, mut known, mut at) = (0, 0, false, None);
        for (t, _, _) in &changes {
            if let How::Changed {
                plus: p,
                minus: m,
                at: a,
            } = &t.how
            {
                plus += p.unwrap_or(0);
                minus += m.unwrap_or(0);
                known |= p.is_some() || m.is_some();
                at = at.or(*a);
            }
        }
        if let Some(n) = at {
            part.push_str(&format!(" at :{n}"));
        }
        if known {
            part.push_str(&format!(" · +{plus} −{minus}"));
        }
        parts.push(part);
    }
    let reads: Vec<Option<(u32, u32)>> = good
        .iter()
        .filter_map(|(t, _, _)| match t.how {
            How::Read { lines } => Some(lines),
            _ => None,
        })
        .collect();
    if !reads.is_empty() {
        let mut ranges: Vec<(u32, u32)> = Vec::new();
        for r in reads.iter().flatten() {
            if !ranges.contains(r) {
                ranges.push(*r);
            }
        }
        let shown: Vec<String> = ranges
            .iter()
            .take(2)
            .map(|(a, b)| {
                if a == b {
                    a.to_string()
                } else {
                    format!("{a}–{b}")
                }
            })
            .collect();
        let mut part = "read".to_string();
        if !shown.is_empty() {
            part.push(' ');
            part.push_str(&shown.join(", "));
            if ranges.len() > 2 {
                part.push('…');
            }
        }
        parts.push(part);
    }
    let searches: Vec<Option<u32>> = good
        .iter()
        .filter_map(|(t, _, _)| match t.how {
            How::Searched { matches } => Some(matches),
            _ => None,
        })
        .collect();
    if !searches.is_empty() {
        let mut part = "searched".to_string();
        if searches.iter().any(Option::is_some) {
            let n: u32 = searches.iter().flatten().sum();
            part.push_str(&format!(" · {}", plural(n as usize, "match", "matches")));
        }
        parts.push(part);
    }
    let scripts = good
        .iter()
        .filter(|(t, _, _)| t.how == How::ScriptTouched)
        .count();
    if scripts > 0 {
        parts.push(match (parts.is_empty(), scripts) {
            (false, n) => format!("script {n}×"),
            (true, 1) => "touched by a script".into(),
            (true, n) => format!("touched by a script {n}×"),
        });
    }
    // Tries that failed before one went through, in words.
    match touches.len() - good.len() {
        0 => {}
        1 => parts.push("1st try failed".into()),
        n => parts.push(format!("{n} tries failed")),
    }
    parts.join(" · ")
}

/// A check run or failed call's outcome in words: "2 passed", "1 of 3 failed" (a row's title
/// already says what ran), "build failed · E0425" (a compile error is the build's, whatever ran
/// it), `PLANNED`; a failed call's first error (cut to `CHIP_ERROR_MAX`), else "failed".
/// None: nothing to say (running, unknown, not run).
pub fn outcome_phrase(a: &Activity) -> Option<String> {
    let Some(check) = &a.check else {
        return (a.failed.is_some() && !a.not_run).then(|| {
            let first = a.failed.as_deref().unwrap_or("").trim();
            if first.is_empty() {
                "failed".to_string()
            } else {
                cut(first, CHIP_ERROR_MAX)
            }
        });
    };
    match &check.outcome {
        Outcome::Passed { n: Some(n) } => Some(format!("{n} passed")),
        Outcome::Passed { n: None } => Some("passed".into()),
        Outcome::Failed { .. } if a.planned_failure => Some(PLANNED.into()),
        Outcome::Failed {
            failed,
            of,
            first_error,
        } => {
            if let Some(code) = first_error.as_deref().and_then(error_code) {
                return Some(format!("build failed · {code}"));
            }
            Some(match (failed, of) {
                (Some(f), Some(of)) => format!("{f} of {of} failed"),
                (Some(f), None) => format!("{f} failed"),
                _ => format!("{} failed", what_word(check.what)),
            })
        }
        Outcome::Unknown => None,
    }
}

/// How a scope's latest run stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Standing {
    Passing,
    Failing,
    Planned,
    Unknown,
}

/// The checks track's summary: each scope's latest state, not a chain of runs. A scope still
/// failing for real leads ("tests failed · cli:: 1 of 3", "+N more"); else a red run the agent
/// announced ("tests · expected — test written first"); else what passes ("tests · 2 passed", "3 passing · 2
/// fixed", "red → green" when a planned failure went green). The kind is always named.
pub fn checks_summary(runs: &[&Activity], cwd: &Path) -> String {
    let scopes = by_scope(runs, cwd);
    let latest: Vec<(&Activity, Standing, &[&Activity])> = scopes
        .iter()
        .filter_map(|list| {
            let last = *list.last()?;
            Some((last, standing(last), list.as_slice()))
        })
        .collect();
    if latest.is_empty() {
        return "ran".into();
    }
    let kind = |a: &Activity| a.check.as_ref().map_or("checks", |c| what_word(c.what));
    let failing: Vec<&Activity> = latest
        .iter()
        .filter(|(_, s, _)| *s == Standing::Failing)
        .map(|(a, ..)| *a)
        .collect();
    if let Some(last) = failing.last() {
        let mut out = failing_words(last);
        if failing.len() > 1 {
            out.push_str(&format!(" · +{} more", failing.len() - 1));
        }
        return out;
    }
    if let Some((a, ..)) = latest.iter().find(|(_, s, _)| *s == Standing::Planned) {
        let mut out = format!("{} · {PLANNED}", kind(a));
        let passing = latest
            .iter()
            .filter(|(_, s, _)| *s == Standing::Passing)
            .count();
        if passing > 0 {
            out.push_str(&format!(" · {passing} passing"));
        }
        return out;
    }
    let passing: Vec<&(&Activity, Standing, &[&Activity])> = latest
        .iter()
        .filter(|(_, s, _)| *s == Standing::Passing)
        .collect();
    let had = |list: &[&Activity], planned: bool| {
        list.iter()
            .any(|a| a.check_failed() && a.planned_failure == planned)
    };
    let fixed = passing.iter().filter(|(_, _, l)| had(l, false)).count();
    let green = passing.iter().any(|(_, _, l)| had(l, true));
    let unknown = latest.len() - passing.len();
    if let [(a, _, list)] = latest.as_slice() {
        let mut parts = vec![kind(a).to_string()];
        if passing.is_empty() {
            parts.push("ran".into());
            return parts.join(" · ");
        }
        if green {
            parts.push("red → green".into());
        }
        parts.extend(outcome_phrase(a));
        if had(list, false) {
            parts.push("fixed".into());
        }
        return parts.join(" · ");
    }
    let mut parts = Vec::new();
    if passing.is_empty() {
        parts.push(format!("{} ran", latest.len()));
    } else {
        parts.push(format!("{} passing", passing.len()));
        if fixed > 0 {
            parts.push(format!("{fixed} fixed"));
        }
        if green {
            parts.push("red → green".into());
        }
        if unknown > 0 {
            parts.push(format!("{unknown} unknown"));
        }
    }
    parts.join(" · ")
}

/// Whether a scope's latest run among `runs` failed for real (the checks track's failure ink).
pub fn checks_failing(runs: &[&Activity], cwd: &Path) -> bool {
    by_scope(runs, cwd).iter().any(|list| {
        list.last()
            .is_some_and(|a| standing(a) == Standing::Failing)
    })
}

/// `runs` grouped by scope (`check_key`), in the order of each scope's first run.
fn by_scope<'a>(runs: &[&'a Activity], cwd: &Path) -> Vec<Vec<&'a Activity>> {
    let mut scopes: Vec<((CheckWhat, String), Vec<&Activity>)> = Vec::new();
    for a in runs {
        let Some(check) = &a.check else { continue };
        let key = check_key(check, cwd);
        match scopes.iter_mut().find(|(k, _)| *k == key) {
            Some((_, list)) => list.push(a),
            None => scopes.push((key, vec![a])),
        }
    }
    scopes.into_iter().map(|(_, list)| list).collect()
}

fn standing(a: &Activity) -> Standing {
    match a.check.as_ref().map(|c| &c.outcome) {
        Some(Outcome::Passed { .. }) => Standing::Passing,
        Some(Outcome::Failed { .. }) if a.planned_failure => Standing::Planned,
        Some(Outcome::Failed { .. }) => Standing::Failing,
        _ => Standing::Unknown,
    }
}

/// A real failure in the checks track's words: "tests failed · cli:: 1 of 3", "build failed ·
/// E0425", "tests failed".
fn failing_words(a: &Activity) -> String {
    let Some(check) = &a.check else {
        return "failed".into();
    };
    let what = what_word(check.what);
    let Outcome::Failed {
        failed,
        of,
        first_error,
    } = &check.outcome
    else {
        return format!("{what} failed");
    };
    if let Some(code) = first_error.as_deref().and_then(error_code) {
        return format!("build failed · {code}");
    }
    let mut detail: Vec<String> = Vec::new();
    let filter = scope_filter(&check.scope);
    if !filter.is_empty() {
        detail.push(filter);
    }
    match (failed, of) {
        (Some(f), Some(of)) => detail.push(format!("{f} of {of}")),
        (Some(f), None) => detail.push(format!("{f} failed")),
        _ => {}
    }
    if detail.is_empty() {
        format!("{what} failed")
    } else {
        format!("{what} failed · {}", detail.join(" "))
    }
}

/// A readable tool row's status mark.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowMark {
    /// The Working arc.
    Running,
    /// A real failure: a red dot.
    Failed,
    /// A failure the agent announced: a red ring.
    Planned,
    /// A check whose outcome is unknown, or a call that didn't run or was stopped: a dim ring.
    Unknown,
    /// Done: a green dot.
    Ok,
}

/// A readable tool row's mark from its state (`ok`: the item's) and activity. An Unknown check
/// is never green, whatever its call's ok (the review's must-fix 3).
pub fn row_mark(running: bool, ok: Option<bool>, a: &Activity) -> RowMark {
    let unknown = matches!(
        a.check,
        Some(Check {
            outcome: Outcome::Unknown,
            ..
        })
    );
    match (running, ok) {
        (true, _) => RowMark::Running,
        _ if a.real_failure() => RowMark::Failed,
        _ if a.planned_failure && a.check_failed() => RowMark::Planned,
        _ if a.not_run || unknown => RowMark::Unknown,
        (false, Some(true)) => RowMark::Ok,
        (false, Some(false)) => RowMark::Failed,
        (false, None) => RowMark::Unknown,
    }
}

/// A readable row's words: its title, a dim trace of its command, and outcome chips.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RowWords {
    pub title: String,
    pub trace: Option<String>,
    pub chips: Vec<String>,
}

/// A tool row's words (approach A): the agent's own words else fuxx's (the present tense while
/// it runs), the command as a trace, and chips ("+41 −0", "3 matches", "2 passed", "failed").
pub fn row_words(item: &Item) -> RowWords {
    let Item::Tool {
        running,
        detail,
        output,
        ..
    } = item
    else {
        return RowWords {
            title: String::new(),
            trace: None,
            chips: Vec::new(),
        };
    };
    let a = of(item).unwrap_or_default();
    let title = if *running {
        a.running_title()
    } else {
        a.title().to_string()
    };
    let trace = match detail {
        Some(ToolDetail::Command(cmd)) => {
            let line = cmd.split_whitespace().collect::<Vec<_>>().join(" ");
            (a.said.is_some() || title != line).then(|| cut(&line, TRACE_MAX))
        }
        _ => None,
    };
    let mut chips = Vec::new();
    if !*running {
        let (mut plus, mut minus, mut known) = (0, 0, false);
        for t in a.changed() {
            if let How::Changed {
                plus: p, minus: m, ..
            } = t.how
            {
                plus += p.unwrap_or(0);
                minus += m.unwrap_or(0);
                known |= p.is_some() || m.is_some();
            }
        }
        if known {
            chips.push(format!("+{plus} −{minus}"));
        }
        let matches: Vec<u32> = a
            .touches
            .iter()
            .filter_map(|t| match t.how {
                How::Searched { matches } => matches,
                _ => None,
            })
            .collect();
        if !matches.is_empty() && a.failed.is_none() {
            chips.push(plural(
                matches.iter().sum::<u32>() as usize,
                "match",
                "matches",
            ));
        }
        // A whole-file read: how many lines it read.
        let whole_read = matches!(
            a.touches.as_slice(),
            [Touch {
                how: How::Read { lines: None },
                ..
            }]
        );
        if let (true, None, Some(out)) = (whole_read, &a.failed, output) {
            chips.push(crate::agent_tab::lines_label(out.lines().count()));
        }
        chips.extend(outcome_phrase(&a));
    }
    RowWords {
        title,
        trace,
        chips,
    }
}

/// A column's hover words: the call's title, its outcome and how long it took.
pub fn column_tip(a: &Activity, took: Option<Duration>) -> String {
    let mut parts = vec![a.title().to_string()];
    parts.extend(outcome_phrase(a));
    parts.extend(took.map(clock));
    parts.join(" · ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_activity::{Activity, CheckWhat, How, Outcome, ToolKind};
    use crate::agent_steps::tests::{CWD, check, failed, passed, read, tool, touching};
    use crate::providers::ToolDetail;
    use std::path::Path;
    use std::time::Duration;

    #[test]
    fn file_summaries() {
        let ch = |at, p, m| Touch {
            path: "src/api.rs".into(),
            how: How::Changed {
                plus: p,
                minus: m,
                at,
            },
        };
        let e = ToolKind::Edit;
        assert_eq!(
            file_summary(&[(&ch(Some(543), Some(1), Some(0)), false, e)]),
            "edited at :543 · +1 −0"
        );
        let r = |a, b| Touch {
            path: "src/api_client.rs".into(),
            how: How::Read {
                lines: Some((a, b)),
            },
        };
        let script = Touch {
            path: "src/api_client.rs".into(),
            how: How::ScriptTouched,
        };
        let x = ToolKind::Execute;
        assert_eq!(
            file_summary(&[
                (&r(80, 400), false, ToolKind::Read),
                (&r(78, 120), false, ToolKind::Read),
                (&script, false, x),
                (&script, false, x),
            ]),
            "read 80–400, 78–120 · script 2×"
        );
        assert_eq!(file_summary(&[(&script, false, x)]), "touched by a script");
        let searched = Touch {
            path: "src/help_commands.rs".into(),
            how: How::Searched { matches: Some(3) },
        };
        assert_eq!(
            file_summary(&[(&searched, false, ToolKind::Search)]),
            "searched · 3 matches"
        );
        assert_eq!(
            file_summary(&[(&ch(None, Some(1), Some(1)), true, e)]),
            "edit failed"
        );
        assert_eq!(
            file_summary(&[(&ch(None, None, None), false, ToolKind::Delete)]),
            "deleted"
        );
    }

    /// A change that took more than one try says so in words (the 2026-11-24 chores).
    #[test]
    fn a_failed_edit_says_first_try_failed() {
        let ch = |at, p, m| Touch {
            path: "src/api.rs".into(),
            how: How::Changed {
                plus: p,
                minus: m,
                at,
            },
        };
        let e = ToolKind::Edit;
        let missed = ch(None, Some(1), Some(1));
        let made = ch(None, Some(4), Some(2));
        assert_eq!(
            file_summary(&[(&missed, true, e), (&made, false, e)]),
            "edited · +4 −2 · 1st try failed"
        );
        assert_eq!(
            file_summary(&[(&missed, true, e), (&missed, true, e), (&made, false, e)]),
            "edited · +4 −2 · 2 tries failed"
        );
    }

    #[test]
    fn checks_summaries_say_each_scopes_latest_state() {
        let cwd = Path::new(CWD);
        let cli = |o| check(CheckWhat::Tests, "cargo test cli::", o);
        let api = |o| check(CheckWhat::Tests, "cargo test api::", o);
        let planned = |o| Activity {
            planned_failure: true,
            ..cli(o)
        };
        let e0425 = Outcome::Failed {
            failed: None,
            of: None,
            first_error: Some("error[E0425]: cannot find function `call_within`".into()),
        };
        let build = |o| check(CheckWhat::Build, "cargo build", o);
        let lint = |o| check(CheckWhat::Lint, "cargo clippy", o);
        let s = |runs: &[Activity]| checks_summary(&runs.iter().collect::<Vec<_>>(), cwd);
        assert_eq!(s(&[cli(passed(2))]), "tests · 2 passed");
        assert_eq!(s(&[cli(passed(2)), build(passed_any())]), "2 passing");
        // Spelled out (the 2026-11-24 chores: "I don't understand the state at all").
        assert_eq!(
            s(&[planned(failed(1, 3))]),
            "tests · expected — test written first"
        );
        assert_eq!(
            s(&[planned(failed(1, 3)), cli(passed(2))]),
            "tests · red → green · 2 passed"
        );
        assert_eq!(
            s(&[cli(failed(1, 3)), cli(passed(3))]),
            "tests · 3 passed · fixed"
        );
        // A scope still failing names its kind and filter, whatever passed since.
        assert_eq!(
            s(&[cli(failed(1, 3)), api(passed(2))]),
            "tests failed · cli:: 1 of 3"
        );
        assert_eq!(
            s(&[cli(failed(1, 3)), cli(failed(1, 3))]),
            "tests failed · cli:: 1 of 3"
        );
        assert_eq!(s(&[build(e0425.clone())]), "build failed · E0425");
        assert_eq!(
            s(&[cli(e0425)]),
            "build failed · E0425",
            "a compile error under cargo test is the build's"
        );
        assert_eq!(s(&[cli(Outcome::Unknown)]), "tests · ran");
        assert_eq!(
            s(&[
                cli(failed(1, 3)),
                api(failed(2, 4)),
                lint(passed_any()),
                cli(passed(3)),
                api(passed(4)),
            ]),
            "3 passing · 2 fixed"
        );
    }

    fn passed_any() -> Outcome {
        Outcome::Passed { n: None }
    }

    #[test]
    fn a_failed_calls_chip_is_its_first_error_and_a_compile_error_is_the_builds() {
        let bad = |e: &str| Activity {
            failed: Some(e.into()),
            ..Default::default()
        };
        assert_eq!(
            outcome_phrase(&bad("String to replace not found in file.")).as_deref(),
            Some("String to replace not found in file.")
        );
        let long = outcome_phrase(&bad(&"x".repeat(80))).unwrap();
        assert_eq!(long.chars().count(), 40);
        assert!(long.ends_with('…'));
        assert_eq!(outcome_phrase(&bad("  ")).as_deref(), Some("failed"));
        assert_eq!(
            outcome_phrase(&check(CheckWhat::Tests, "cargo test", failed(1, 3))).as_deref(),
            Some("1 of 3 failed"),
            "the row's title already says what ran"
        );
        let e0425 = Outcome::Failed {
            failed: None,
            of: None,
            first_error: Some("error[E0425]: cannot find function `call_within`".into()),
        };
        assert_eq!(
            outcome_phrase(&check(CheckWhat::Tests, "cargo test", e0425)).as_deref(),
            Some("build failed · E0425"),
            "the tests never ran"
        );
    }

    #[test]
    fn a_failed_or_denied_calls_title_is_the_plain_verb() {
        let mut a = Activity {
            derived: "Edited cli.rs".into(),
            failed: Some("String to replace not found in file.".into()),
            ..Default::default()
        };
        assert_eq!(a.title(), "Edit cli.rs");
        a.derived = "Listed files".into();
        assert_eq!(a.title(), "List files");
        a.said = Some("Fix the import".into());
        assert_eq!(a.title(), "Fix the import", "the agent's words stay");
        let denied = Activity {
            derived: "Ran tests".into(),
            not_run: true,
            ..Default::default()
        };
        assert_eq!(denied.title(), "Run tests");
    }

    #[test]
    fn an_unknown_check_never_marks_its_row_green() {
        let unknown = check(CheckWhat::Tests, "cargo test", Outcome::Unknown);
        assert_eq!(row_mark(false, Some(true), &unknown), RowMark::Unknown);
        let ok = check(CheckWhat::Tests, "cargo test", passed(2));
        assert_eq!(row_mark(false, Some(true), &ok), RowMark::Ok);
        let planned = Activity {
            planned_failure: true,
            ..check(CheckWhat::Tests, "cargo test", failed(1, 3))
        };
        assert_eq!(row_mark(false, Some(false), &planned), RowMark::Planned);
        assert_eq!(row_mark(true, None, &ok), RowMark::Running);
    }

    #[test]
    fn row_words_lead_with_the_description_and_trace_the_command() {
        let mut tests = tool(
            "1",
            Activity {
                said: Some("Run the CLI tests".into()),
                ..check(CheckWhat::Tests, "cargo test --bin fuxx cli::", passed(2))
            },
        );
        if let Item::Tool { detail, .. } = &mut tests {
            *detail = Some(ToolDetail::Command("cargo test --bin fuxx cli::".into()));
        }
        assert_eq!(
            row_words(&tests),
            RowWords {
                title: "Run the CLI tests".into(),
                trace: Some("cargo test --bin fuxx cli::".into()),
                chips: vec!["2 passed".into()],
            }
        );
        let write = tool(
            "2",
            touching(
                ToolKind::Edit,
                "/tmp/fx/code/my-app/src/link_route.rs",
                How::Changed {
                    plus: Some(41),
                    minus: Some(0),
                    at: None,
                },
            ),
        );
        assert_eq!(
            row_words(&write),
            RowWords {
                title: "Edited link_route.rs".into(),
                trace: None,
                chips: vec!["+41 −0".into()],
            }
        );
        let e0425 = Outcome::Failed {
            failed: None,
            of: None,
            first_error: Some("error[E0425]: cannot find function `call_within`".into()),
        };
        let build = tool("3", check(CheckWhat::Build, "cargo build", e0425));
        assert_eq!(row_words(&build).chips, ["build failed · E0425"]);
        let mut running = tool("4", check(CheckWhat::Tests, "cargo test", Outcome::Unknown));
        if let Item::Tool { running: r, ok, .. } = &mut running {
            *r = true;
            *ok = None;
        }
        assert_eq!(row_words(&running).title, "Running tests");
        assert!(row_words(&running).chips.is_empty());
        // A whole-file read says how many lines it read, as the row did before.
        let mut whole = read("5", "src/api.rs");
        if let Item::Tool { output, .. } = &mut whole {
            *output = Some("a\nb\nc".into());
        }
        assert_eq!(row_words(&whole).chips, ["3 lines"]);
    }

    #[test]
    fn a_column_s_tip_is_its_row_outcome_and_time() {
        let a = check(CheckWhat::Tests, "cargo test cli::", passed(2));
        assert_eq!(
            column_tip(&a, Some(Duration::from_secs(9))),
            "Ran tests cli:: · 2 passed · 0:09"
        );
        assert_eq!(column_tip(&a, None), "Ran tests cli:: · 2 passed");
    }

    #[test]
    fn header_counts_failed_calls_and_time_when_known() {
        assert_eq!(
            header(&counts(8, 5), Some(Duration::from_secs(112))),
            "8 actions · 5 files · 1:52"
        );
        assert_eq!(header(&counts(8, 5), None), "8 actions · 5 files");
        assert_eq!(
            header(&counts(8, 5), Some(Duration::from_millis(600))),
            "8 actions · 5 files",
            "no time under a second"
        );
        assert_eq!(counts(1, 1), "1 action · 1 file");
    }

    #[test]
    fn a_long_command_s_trace_is_capped() {
        let cmd = format!("python3 -c '{}'", "x = 1; ".repeat(400));
        let mut item = tool("1", Activity::default());
        if let Item::Tool { detail, .. } = &mut item {
            *detail = Some(ToolDetail::Command(cmd));
        }
        let trace = row_words(&item).trace.expect("a trace");
        assert_eq!(trace.chars().count(), TRACE_MAX);
        assert!(trace.ends_with('…'));
    }
}
