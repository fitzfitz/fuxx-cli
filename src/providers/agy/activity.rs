//! agy's tool calls as activities (the agent activity chore): its steps carry only paths and
//! command lines (no edit text, so no +/−), its exit code sits in a header at the start of a
//! command's output, and a subagent's transcript gives each call its `toolSummary`. agy-only:
//! the generic readers are `agent_activity*`.

use serde_json::Value;

use crate::agent_activity::{
    Activity, Check, Finish, How, Outcome, ToolKind, Touch, derived_words, finished,
};
use crate::agent_activity_command::read_command;

/// A call between its start and its result: the activity as planned, and whether a check's
/// output goes into a pipe.
pub(super) struct Call {
    start: Activity,
    exit_hidden: bool,
}

/// A call's activity at its start (`said`: a lane's `toolSummary`), and what its finish needs.
pub(super) fn call_started(name: &str, params: &Value, said: Option<String>) -> (Activity, Call) {
    let start = activity_at_start(name, params, said);
    let exit_hidden = params["CommandLine"]
        .as_str()
        .is_some_and(|c| !read_command(c).exit_trusted);
    (start.clone(), Call { start, exit_hidden })
}

/// A call's final activity: `exit` from the command's header, `error` agy's own error text.
pub(super) fn call_finished(
    call: Call,
    ok: bool,
    exit: Option<i32>,
    output: &str,
    error: Option<String>,
) -> Activity {
    finished(
        call.start,
        Finish {
            ok,
            exit,
            output,
            error,
            not_run: false,
            exit_hidden: call.exit_hidden,
        },
    )
}

/// What a call does, from its parameters alone. agy's edits carry only `Path` (no text, so no
/// +/−); its reads no line range.
pub(super) fn activity_at_start(name: &str, params: &Value, said: Option<String>) -> Activity {
    let path = params["Path"].as_str().unwrap_or("").to_string();
    let make = |kind: ToolKind, how: Option<How>, object: &str| {
        let touches: Vec<Touch> = how
            .into_iter()
            .map(|how| Touch {
                path: path.clone(),
                how,
            })
            .collect();
        Activity {
            derived: derived_words(kind, &touches, None, object),
            kind,
            touches,
            ..Default::default()
        }
    };
    let mut activity = match name {
        "read_file" => make(ToolKind::Read, Some(How::Read { lines: None }), ""),
        "edit_file" | "write_file" => make(
            ToolKind::Edit,
            Some(How::Changed {
                plus: None,
                minus: None,
                at: None,
            }),
            "",
        ),
        "run_command" => {
            let r = read_command(params["CommandLine"].as_str().unwrap_or(""));
            Activity {
                kind: r.kind,
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
        _ => make(ToolKind::Other, None, name),
    };
    activity.said = said.filter(|s| !s.trim().is_empty());
    activity
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_activity::{Activity, How};
    use crate::providers::agy::{AGY, AgyTranslator, command_output};
    use crate::providers::{Event, Provider, Translator};
    use serde_json::json;

    const TOOLS: &str = include_str!("../fixtures/agy-tools.jsonl");
    const ASK_TRANSCRIPT: &str = include_str!("../fixtures/agy-ask-transcript.jsonl");

    fn activities(ev: &[Event]) -> Vec<(String, Activity)> {
        ev.iter()
            .filter_map(|e| match e {
                Event::Activity { id, activity } => Some((id.clone(), activity.clone())),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn agy_command_exit_is_the_header_s_code() {
        assert_eq!(
            command_output(
                "Created At: x\nCompleted At: x\nCommand exited with code 1.\n\nCommand output:\nboom\n"
            ),
            ("boom\n".into(), Some(1))
        );
        assert_eq!(command_output("plain"), ("plain".into(), None));
    }

    #[test]
    fn agy_steps_fill_activities_without_counts() {
        let mut t = AGY.translator();
        let ev: Vec<Event> = TOOLS.lines().flat_map(|l| t.decode(l)).collect();
        let started = |name: &str| {
            ev.iter()
                .find_map(|e| match e {
                    Event::ToolStarted { id, name: n, .. } if n == name => Some(id.clone()),
                    _ => None,
                })
                .expect(name)
        };
        let last = |id: &str| {
            activities(&ev)
                .into_iter()
                .rev()
                .find(|(i, _)| i == id)
                .map(|(_, a)| a)
                .expect(id)
        };
        assert_eq!(
            last(&started("read_file")).touches[0].how,
            How::Read { lines: None }
        );
        let changed = How::Changed {
            plus: None,
            minus: None,
            at: None,
        };
        assert_eq!(last(&started("edit_file")).touches[0].how, changed);
        assert_eq!(last(&started("write_file")).touches[0].how, changed);
        let ls = last(&started("run_command"));
        assert_eq!(ls, {
            let r = crate::agent_activity_command::read_command("ls");
            Activity {
                kind: r.kind,
                derived: r.derived,
                touches: r.touches,
                exit: Some(0),
                ..Default::default()
            }
        });
        assert!(activities(&ev).iter().all(|(_, a)| a.said.is_none()));
    }

    #[test]
    fn an_agy_lane_s_tool_summary_is_its_words() {
        let mut t = AgyTranslator::default();
        let line = ASK_TRANSCRIPT
            .lines()
            .find(|l| l.contains("toolSummary"))
            .unwrap();
        let ev = t.decode_followed("lane-1", line);
        let (_, activity) = activities(&ev).pop().expect("an activity");
        assert_eq!(
            activity.said.as_deref(),
            Some("List workspace directory contents")
        );
    }

    #[test]
    fn an_agy_lane_s_failing_command_is_not_ok() {
        let mut t = AgyTranslator::default();
        let call = json!({"step_index": 3, "type": "PLANNER_RESPONSE", "tool_calls": [
            {"name": "run_command", "args": {"CommandLine": "\"cargo build\""}}]});
        let result = json!({"step_index": 4, "type": "GENERIC",
            "content": "Created At: x\nCompleted At: x\nCommand exited with code 1.\n\nCommand output:\nerror: could not compile `my-app`\n"});
        t.decode_followed("lane-1", &call.to_string());
        let ev = t.decode_followed("lane-1", &result.to_string());
        assert!(
            ev.iter()
                .any(|e| matches!(e, Event::ToolFinished { ok: false, .. }))
        );
        let (_, activity) = activities(&ev).pop().expect("an activity");
        assert_eq!(activity.exit, Some(1));
    }

    #[test]
    fn agy_activities_at_start() {
        let read = activity_at_start(
            "read_file",
            &json!({"Path": "/tmp/fx/code/my-app/src/api.rs"}),
            None,
        );
        assert_eq!(read.derived, "Read api.rs");
        let other = activity_at_start("browser", &json!({}), Some("Open the docs".into()));
        assert_eq!(other.title(), "Open the docs");
    }
}
