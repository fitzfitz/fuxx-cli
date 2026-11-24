//! The step model's words (the agent activity chore): a step's header, a file track's and the
//! checks track's summaries, a call's outcome phrase and hover words, and a readable row's
//! title, trace and chips. Pure and GPUI-free; split out of `agent_steps` (the review's lean
//! files).

use std::time::Duration;

use crate::agent_activity::{Activity, How, Outcome, ToolKind, Touch, of, what_word};
use crate::agent_activity_checks::error_code;
use crate::agent_conversation::Item;
use crate::agent_lanes::clock;
use crate::providers::ToolDetail;

/// The longest command trace a readable row keeps, in characters (the row truncates it to its
/// width; a huge script needn't be carried whole).
pub const TRACE_MAX: usize = 160;

/// The header: the counts, then the time when known.
pub fn header(counts: &str, took: Option<Duration>) -> String {
    match took {
        Some(d) => format!("{counts} · {}", clock(d)),
        None => counts.to_string(),
    }
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// "8 actions · 5 files", and " · N failed" when calls failed.
pub fn counts(actions: usize, files: usize, failed: usize) -> String {
    let mut out = format!(
        "{} · {}",
        plural(actions, "action", "actions"),
        plural(files, "file", "files")
    );
    if failed > 0 {
        out.push_str(&format!(" · {failed} failed"));
    }
    out
}

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
    parts.join(" · ")
}

/// A check run or failed call's outcome in words: "2 passed", "build failed · E0425", "red, as
/// planned", "1 of 3 failed", "tests failed", "failed". None: nothing to say (running, unknown).
pub fn outcome_phrase(a: &Activity) -> Option<String> {
    let Some(check) = &a.check else {
        return (a.failed.is_some() && !a.not_run).then(|| "failed".to_string());
    };
    match &check.outcome {
        Outcome::Passed { n: Some(n) } => Some(format!("{n} passed")),
        Outcome::Passed { n: None } => Some("passed".into()),
        Outcome::Failed { .. } if a.planned_failure => Some("red, as planned".into()),
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

/// The checks track's summary: each run's outcome, a planned failure that a later pass of the
/// same check followed folded into "red → green · …", repeats collapsed, the last three kept.
pub fn checks_summary(runs: &[&Activity]) -> String {
    let same = |a: &Activity, b: &Activity| match (&a.check, &b.check) {
        (Some(x), Some(y)) => x.what == y.what && x.scope == y.scope,
        _ => false,
    };
    let passed = |a: &Activity| {
        matches!(
            a.check.as_ref().map(|c| &c.outcome),
            Some(Outcome::Passed { .. })
        )
    };
    let mut used = vec![false; runs.len()];
    let mut phrases: Vec<String> = Vec::new();
    for (i, a) in runs.iter().enumerate() {
        if used[i] {
            continue;
        }
        let green = (a.planned_failure && a.check_failed())
            .then(|| (i + 1..runs.len()).find(|&j| !used[j] && same(a, runs[j]) && passed(runs[j])))
            .flatten();
        if let Some(j) = green {
            used[j] = true;
            let pass = outcome_phrase(runs[j]).unwrap_or_else(|| "passed".into());
            phrases.push(format!("red → green · {pass}"));
            continue;
        }
        if let Some(p) = outcome_phrase(a) {
            phrases.push(p.replace("failed · ", "failed "));
        }
    }
    phrases.dedup();
    if phrases.is_empty() {
        return "ran".into();
    }
    if phrases.len() > 3 {
        let kept = phrases.split_off(phrases.len() - 3);
        return format!("… → {}", kept.join(" → "));
    }
    phrases.join(" → ")
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
            (a.said.is_some() || title != line).then(|| capped(line))
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

/// `line` cut to `TRACE_MAX` characters, an ellipsis last when cut.
fn capped(line: String) -> String {
    if line.chars().count() <= TRACE_MAX {
        return line;
    }
    let mut out: String = line.chars().take(TRACE_MAX - 1).collect();
    out.push('…');
    out
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
    use crate::agent_steps::tests::{check, failed, passed, read, tool, touching};
    use crate::providers::ToolDetail;
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

    #[test]
    fn checks_summaries() {
        let tests = |o| check(CheckWhat::Tests, "cargo test cli::", o);
        let planned = |o| Activity {
            planned_failure: true,
            ..tests(o)
        };
        let e0425 = Outcome::Failed {
            failed: None,
            of: None,
            first_error: Some("error[E0425]: cannot find function `call_within`".into()),
        };
        let build = check(CheckWhat::Build, "cargo build", e0425);
        assert_eq!(checks_summary(&[&tests(passed(2))]), "2 passed");
        assert_eq!(
            checks_summary(&[&tests(passed(2)), &build]),
            "2 passed → build failed E0425"
        );
        assert_eq!(checks_summary(&[&planned(failed(1, 3))]), "red, as planned");
        assert_eq!(
            checks_summary(&[&planned(failed(1, 3)), &tests(passed(2))]),
            "red → green · 2 passed"
        );
        assert_eq!(checks_summary(&[&tests(failed(1, 3))]), "1 of 3 failed");
        let four = [
            tests(failed(1, 3)),
            tests(passed(1)),
            tests(passed(2)),
            tests(passed(3)),
        ];
        assert_eq!(
            checks_summary(&four.iter().collect::<Vec<_>>()),
            "… → 1 passed → 2 passed → 3 passed"
        );
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
            header(&counts(8, 5, 0), Some(Duration::from_secs(112))),
            "8 actions · 5 files · 1:52"
        );
        assert_eq!(
            header(&counts(8, 5, 1), None),
            "8 actions · 5 files · 1 failed"
        );
        assert_eq!(counts(1, 1, 0), "1 action · 1 file");
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
