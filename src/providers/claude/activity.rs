//! Claude's tool calls as activities (the agent activity chore): what each call reads, searches
//! or changes from its input, and how it ended from its result (`Exit code N`,
//! `<tool_use_error>`). Claude-only: the generic readers are `agent_activity*`.

use serde_json::Value;

use crate::agent_activity::{
    Activity, Check, Finish, How, Outcome, ToolKind, Touch, derived_words, finished,
};
use crate::agent_activity_command::{looks_like_path, read_command};

/// A call between its start and its result: the activity as planned, whether a check's output
/// goes into a pipe, and a Grep's input (its files are read from its result).
pub(super) struct Call {
    start: Activity,
    piped: bool,
    grep: Option<Value>,
}

/// A call's activity at its start, and what its finish needs.
pub(super) fn call_started(name: &str, input: &Value) -> (Activity, Call) {
    let start = activity_at_start(name, input);
    let piped = name == "Bash"
        && input["command"]
            .as_str()
            .is_some_and(|c| read_command(c).piped);
    let grep = (name == "Grep").then(|| input.clone());
    (start.clone(), Call { start, piped, grep })
}

/// A call's final activity from its result's text (`ok`: not `is_error`); `not_run`: the
/// permission was denied.
pub(super) fn call_finished(call: Call, ok: bool, text: &str, not_run: bool) -> Activity {
    let Call {
        mut start,
        piped,
        grep,
    } = call;
    if let Some(input) = grep.filter(|_| ok) {
        let found = grep_touches(&input, text);
        if !found.is_empty() {
            start.touches = found;
            let pattern = input["pattern"].as_str().unwrap_or("");
            start.derived = derived_words(ToolKind::Search, &start.touches, None, pattern);
        }
    }
    finished(
        start,
        Finish {
            ok,
            exit: exit_code(text),
            output: text,
            error: error_line(text).filter(|_| !ok),
            not_run,
            piped,
        },
    )
}

/// The number of lines in an edit's text (none for an empty one).
fn lines_in(text: Option<&str>) -> u32 {
    text.map_or(0, |t| t.lines().count() as u32)
}

/// What a call does, from its input alone.
pub(super) fn activity_at_start(name: &str, input: &Value) -> Activity {
    let text = |k: &str| input[k].as_str().unwrap_or("");
    let path = text("file_path").to_string();
    let changed = |plus: Option<u32>, minus: Option<u32>| How::Changed {
        plus,
        minus,
        at: None,
    };
    let make = |kind: ToolKind, touches: Vec<Touch>, object: &str| Activity {
        derived: derived_words(kind, &touches, None, object),
        kind,
        touches,
        ..Default::default()
    };
    let one = |how: How| {
        vec![Touch {
            path: path.clone(),
            how,
        }]
    };
    match name {
        "Read" => {
            let (offset, limit) = (input["offset"].as_u64(), input["limit"].as_u64());
            let lines = match (offset, limit) {
                (Some(o), Some(l)) if l > 0 => Some((o as u32, (o + l - 1) as u32)),
                (None, Some(l)) if l > 0 => Some((1, l as u32)),
                _ => None,
            };
            make(ToolKind::Read, one(How::Read { lines }), "")
        }
        "Edit" => make(
            ToolKind::Edit,
            one(changed(
                Some(lines_in(input["new_string"].as_str())),
                Some(lines_in(input["old_string"].as_str())),
            )),
            "",
        ),
        "MultiEdit" => {
            let edits = input["edits"]
                .as_array()
                .map(Vec::as_slice)
                .unwrap_or_default();
            let sum = |k: &str| edits.iter().map(|e| lines_in(e[k].as_str())).sum();
            make(
                ToolKind::Edit,
                one(changed(Some(sum("new_string")), Some(sum("old_string")))),
                "",
            )
        }
        "Write" => make(
            ToolKind::Edit,
            one(changed(Some(lines_in(input["content"].as_str())), None)),
            "",
        ),
        "NotebookEdit" => {
            let touches = vec![Touch {
                path: text("notebook_path").to_string(),
                how: changed(None, None),
            }];
            make(ToolKind::Edit, touches, "")
        }
        "Grep" => {
            let touches = Some(text("path"))
                .filter(|p| looks_like_path(p))
                .map(|p| Touch {
                    path: p.to_string(),
                    how: How::Searched { matches: None },
                })
                .into_iter()
                .collect();
            make(ToolKind::Search, touches, text("pattern"))
        }
        "Glob" => Activity {
            kind: ToolKind::Search,
            derived: format!("Listed {}", text("pattern")),
            ..Default::default()
        },
        "WebFetch" => make(ToolKind::Fetch, Vec::new(), text("url")),
        "WebSearch" => Activity {
            kind: ToolKind::Fetch,
            derived: format!("Searched the web for {}", text("query")),
            ..Default::default()
        },
        "Bash" => {
            let r = read_command(text("command"));
            Activity {
                kind: r.kind,
                said: input["description"]
                    .as_str()
                    .filter(|d| !d.trim().is_empty())
                    .map(String::from),
                derived: r.derived,
                touches: r.touches,
                check: r.check.map(|(what, scope)| Check {
                    what,
                    scope,
                    outcome: Outcome::Unknown,
                }),
                ..Default::default()
            }
        }
        // Task / Agent / SendMessage are lane and "passed on" rows; anything else is named.
        _ => make(
            ToolKind::Other,
            Vec::new(),
            input["description"].as_str().unwrap_or(name),
        ),
    }
}

/// A failing Bash result's code: its first line is `Exit code N`.
pub(super) fn exit_code(text: &str) -> Option<i32> {
    text.lines()
        .next()?
        .trim()
        .strip_prefix("Exit code ")?
        .trim()
        .parse()
        .ok()
}

/// A result's first error line: its first non-empty line, with Claude's `<tool_use_error>`
/// tags and the `Exit code N` line left out.
pub(super) fn error_line(text: &str) -> Option<String> {
    text.lines()
        .map(|l| {
            l.replace("<tool_use_error>", "")
                .replace("</tool_use_error>", "")
        })
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .find(|l| exit_code(l).is_none())
}

/// The files a Grep's result names, by its output mode: `content` (`path:line:text`, counted
/// per file), `count` (`path:N`) or `files_with_matches` (a path per line, the default).
pub(super) fn grep_touches(input: &Value, output: &str) -> Vec<Touch> {
    let mode = input["output_mode"]
        .as_str()
        .unwrap_or("files_with_matches");
    let mut found: Vec<(String, Option<u32>)> = Vec::new();
    let mut bump = |path: &str, by: Option<u32>| match found.iter_mut().find(|(p, _)| p == path) {
        Some((_, n)) => *n = n.zip(by).map(|(a, b)| a + b),
        None => found.push((path.to_string(), by)),
    };
    for line in output.lines().map(str::trim).filter(|l| !l.is_empty()) {
        match mode {
            "content" | "count" => {
                let Some((path, rest)) = line.split_once(':') else {
                    continue;
                };
                if !looks_like_path(path) {
                    continue;
                }
                let n = if mode == "count" {
                    rest.trim().parse().ok()
                } else {
                    Some(1)
                };
                bump(path, n);
            }
            _ if looks_like_path(line) => bump(line, None),
            _ => {}
        }
    }
    if found.is_empty() && mode == "content" {
        let path = input["path"].as_str().unwrap_or("");
        let hits = output.lines().filter(|l| !l.trim().is_empty()).count() as u32;
        if looks_like_path(path) && hits > 0 {
            found.push((path.to_string(), Some(hits)));
        }
    }
    found
        .into_iter()
        .map(|(path, matches)| Touch {
            path,
            how: How::Searched { matches },
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_activity::{Activity, Check, CheckWhat, How, Outcome, ToolKind, Touch};
    use crate::providers::claude::{CLAUDE, transcript_events};
    use crate::providers::{Action, Event, Past, Provider};
    use serde_json::json;

    const ACTIVITY: &str = include_str!("../fixtures/claude-activity.jsonl");

    fn decode_all(p: &dyn Provider, text: &str) -> Vec<Event> {
        let mut t = p.translator();
        text.lines().flat_map(|l| t.decode(l)).collect()
    }

    fn activity_of(ev: &[Event], id: &str) -> Activity {
        ev.iter()
            .rev()
            .find_map(|e| match e {
                Event::Activity { id: i, activity } if i == id => Some(activity.clone()),
                _ => None,
            })
            .expect(id)
    }

    #[test]
    fn the_activity_fixture_says_it_is_synthetic() {
        assert!(
            ACTIVITY
                .lines()
                .next()
                .unwrap()
                .contains("\"synthetic\": true")
        );
    }

    #[test]
    fn every_tool_gets_an_activity_after_its_start_and_after_its_finish() {
        let ev = decode_all(&CLAUDE, ACTIVITY);
        for id in (1..=8).map(|n| format!("toolu_act_{n}")) {
            let at = |pred: &dyn Fn(&Event) -> bool| ev.iter().position(pred).expect(&id);
            let started = at(&|e| matches!(e, Event::ToolStarted { id: i, .. } if *i == id));
            let finished = at(&|e| matches!(e, Event::ToolFinished { id: i, .. } if *i == id));
            assert!(matches!(&ev[started + 1], Event::Activity { id: i, .. } if *i == id));
            assert!(matches!(&ev[finished + 1], Event::Activity { id: i, .. } if *i == id));
        }
    }

    #[test]
    fn bash_reads_its_description_command_and_exit() {
        let a = activity_of(&decode_all(&CLAUDE, ACTIVITY), "toolu_act_5");
        assert_eq!(a.said.as_deref(), Some("Run the CLI tests"));
        assert!(matches!(
            a.check,
            Some(Check {
                what: CheckWhat::Tests,
                outcome: Outcome::Failed {
                    failed: Some(1),
                    of: Some(3),
                    ..
                },
                ..
            })
        ));
        assert_eq!(
            exit_code("Exit code 2\nls: nope: No such file or directory"),
            Some(2)
        );
        assert_eq!(exit_code("total 8"), None);
        let ls = activity_of(&decode_all(&CLAUDE, ACTIVITY), "toolu_act_7");
        assert_eq!(
            (ls.exit, ls.failed.as_deref()),
            (Some(2), Some("ls: nope: No such file or directory"))
        );
    }

    #[test]
    fn read_edit_write_grep_and_glob_fill_touches() {
        let ev = decode_all(&CLAUDE, ACTIVITY);
        assert_eq!(
            activity_of(&ev, "toolu_act_1").touches[0].how,
            How::Read {
                lines: Some((80, 400))
            }
        );
        assert_eq!(
            activity_of(&ev, "toolu_act_2").touches[0].how,
            How::ScriptTouched
        );
        assert_eq!(
            activity_of(&ev, "toolu_act_3").touches[0].how,
            How::Changed {
                plus: Some(2),
                minus: Some(1),
                at: None
            }
        );
        assert_eq!(
            activity_of(&ev, "toolu_act_4").touches,
            vec![Touch {
                path: "src/help_commands.rs".into(),
                how: How::Searched { matches: Some(3) }
            }]
        );
        let write = activity_at_start(
            "Write",
            &json!({"file_path": "/tmp/fx/code/my-app/src/link_route.rs", "content": "a\n".repeat(41)}),
        );
        assert_eq!(
            write.touches[0].how,
            How::Changed {
                plus: Some(41),
                minus: None,
                at: None
            }
        );
        let glob = activity_at_start("Glob", &json!({"pattern": "**/*.rs"}));
        assert_eq!(
            (glob.kind, glob.touches.len(), glob.derived.as_str()),
            (ToolKind::Search, 0, "Listed **/*.rs")
        );
        let files = grep_touches(
            &json!({"pattern": "METHODS", "output_mode": "files_with_matches"}),
            "Found 2 files\nsrc/a.rs\nsrc/b.rs",
        );
        assert_eq!(files.len(), 2);
    }

    #[test]
    fn a_failed_edit_touches_nothing_changed() {
        let a = activity_of(&decode_all(&CLAUDE, ACTIVITY), "toolu_act_6");
        assert_eq!(
            a.failed.as_deref(),
            Some("String to replace not found in file.")
        );
        assert_eq!(a.changed().count(), 0);
        assert!(a.real_failure());
    }

    #[test]
    fn a_denied_call_is_not_run() {
        // The deny fixture: decode up to its can_use_tool line, encode Action::Deny for it,
        // then decode the rest.
        let text = include_str!("../fixtures/claude-permission-deny.jsonl");
        let mut t = CLAUDE.translator();
        let mut events = Vec::new();
        let mut write_call_id = String::new();
        for line in text.lines() {
            let v: serde_json::Value = serde_json::from_str(line).unwrap();
            events.extend(t.decode(line));
            if v["request"]["subtype"] == "can_use_tool" {
                write_call_id = v["request"]["tool_use_id"].as_str().unwrap().to_string();
                let id = v["request_id"].as_str().unwrap().to_string();
                assert!(
                    !t.encode(&Action::Deny {
                        id,
                        reason: Some("Not now".into())
                    })
                    .is_empty()
                );
            }
        }
        let a = activity_of(&events, &write_call_id);
        assert!(a.not_run && !a.real_failure());
        assert_eq!(a.changed().count(), 0);
    }

    #[test]
    fn history_rebuilds_the_same_activities() {
        let transcript: String = ACTIVITY
            .lines()
            .filter(|l| l.contains("\"type\": \"assistant\"") || l.contains("\"type\": \"user\""))
            .collect::<Vec<_>>()
            .join("\n");
        let live = decode_all(&CLAUDE, ACTIVITY);
        let past: Vec<Event> = transcript_events(&transcript)
            .into_iter()
            .filter_map(|p| match p {
                Past::Event(e) => Some(e),
                _ => None,
            })
            .collect();
        for n in 1..=8 {
            let id = format!("toolu_act_{n}");
            assert_eq!(activity_of(&live, &id), activity_of(&past, &id), "{id}");
        }
    }
}
