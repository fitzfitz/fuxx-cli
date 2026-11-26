//! Claude's background shells (chore 16, note 93): a `run_in_background` Bash call is a
//! `local_bash` task with `is_backgrounded: true` (a foreground Bash is one too, not
//! backgrounded); its tool_result names the file its output goes to; `task_notification` ends
//! it. Read into the provider-neutral `Event::ShellStarted` / `Event::ShellEnded`.

use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use crate::providers::{Event, ShellEnd};

/// The request id of ■ on shell `id` (`stop_task`, as a subagent's stop; recorded in the
/// background-shells fixture: answered `success`, then a `stopped` notification).
pub(super) fn stop_request(id: &str) -> String {
    format!("{STOP_PREFIX}{id}")
}

pub(super) const STOP_PREFIX: &str = "fuxx-stop-shell-";

/// The words before the output file in a background Bash's tool_result (Claude Code 2.1.331:
/// "Command running in background with ID: b1. Output is being written to: <path>. You will be
/// notified when it completes. …").
const OUTPUT_SAYS: &str = "Output is being written to: ";

/// A shell started and not yet told: its result (naming the file) comes next.
struct Held {
    id: String,
    description: String,
    command: String,
}

/// What the translator keeps of the background shells.
#[derive(Default)]
pub(super) struct Shells {
    /// Background Bash calls by id: their command.
    commands: HashMap<String, String>,
    /// Started, by their call's id, until its result.
    held: HashMap<String, Held>,
    /// Told, by task id, until they end.
    live: HashSet<String>,
}

impl Shells {
    /// A tool call started: a Bash with `run_in_background` keeps its command.
    pub(super) fn call(&mut self, id: &str, name: &str, input: &Value) {
        if name == "Bash" && input["run_in_background"] == true {
            let command = input["command"].as_str().unwrap_or_default();
            self.commands.insert(id.to_string(), command.to_string());
        }
    }

    /// A `task_started`: true when it is a background shell (held for its call's result).
    pub(super) fn started(&mut self, v: &Value) -> bool {
        if v["task_type"] != "local_bash" || v["is_backgrounded"] != true {
            return false;
        }
        let (Some(id), Some(call)) = (v["task_id"].as_str(), v["tool_use_id"].as_str()) else {
            return false;
        };
        let held = Held {
            id: id.to_string(),
            description: v["description"].as_str().unwrap_or_default().to_string(),
            command: self.commands.remove(call).unwrap_or_default(),
        };
        self.held.insert(call.to_string(), held);
        true
    }

    /// Tool call `call`'s result: a held shell is told, with the file its result names.
    pub(super) fn result(&mut self, call: &str, text: &str) -> Option<Event> {
        let held = self.held.remove(call)?;
        Some(self.tell(call, held, output_named(text)))
    }

    /// A `task_notification`: a shell's end (told first if its result never came); nothing for
    /// any other task.
    pub(super) fn ended(&mut self, v: &Value) -> Vec<Event> {
        let Some(id) = v["task_id"].as_str() else {
            return Vec::new();
        };
        let output = v["output_file"]
            .as_str()
            .filter(|p| !p.is_empty())
            .map(PathBuf::from);
        let mut out = Vec::new();
        let held = self
            .held
            .iter()
            .find(|(_, h)| h.id == id)
            .map(|(call, _)| call.clone());
        if let Some(call) = held
            && let Some(h) = self.held.remove(&call)
        {
            out.push(self.tell(&call, h, output.clone()));
        }
        if !self.live.remove(id) {
            return out;
        }
        out.push(Event::ShellEnded {
            id: id.to_string(),
            end: end_of(v),
            output,
        });
        out
    }

    fn tell(&mut self, call: &str, h: Held, output: Option<PathBuf>) -> Event {
        self.live.insert(h.id.clone());
        Event::ShellStarted {
            id: h.id,
            call: call.to_string(),
            description: h.description,
            command: h.command,
            output,
        }
    }
}

/// The output file a background Bash's result names.
fn output_named(text: &str) -> Option<PathBuf> {
    let rest = &text[text.find(OUTPUT_SAYS)? + OUTPUT_SAYS.len()..];
    let end = rest
        .find(".output")
        .map(|i| i + ".output".len())
        .or_else(|| rest.find(". "))
        .unwrap_or(rest.len());
    let path = rest[..end].trim();
    (!path.is_empty()).then(|| PathBuf::from(path))
}

/// How a notification says it ended: `completed` with a non-zero exit in its summary
/// ("… completed (exit code 1)") failed.
fn end_of(v: &Value) -> ShellEnd {
    match v["status"].as_str() {
        Some("failed") => ShellEnd::Failed,
        Some("killed") | Some("stopped") => ShellEnd::Stopped,
        _ => match exit_code(v["summary"].as_str().unwrap_or_default()) {
            Some(code) if code != 0 => ShellEnd::Failed,
            _ => ShellEnd::Done,
        },
    }
}

fn exit_code(summary: &str) -> Option<i64> {
    let rest = &summary[summary.rfind("(exit code ")? + "(exit code ".len()..];
    rest[..rest.find(')')?].trim().parse().ok()
}

#[cfg(test)]
mod tests {
    use crate::providers::claude::CLAUDE;
    use crate::providers::{Action, Event, Provider, ShellEnd};
    use serde_json::{Value, json};
    use std::path::PathBuf;

    const SHELLS: &str = include_str!("../fixtures/claude-background-shells.jsonl");
    const QUEUE: &str = include_str!("../fixtures/claude-queue.jsonl");

    fn decode_all(text: &str) -> Vec<Event> {
        let mut t = CLAUDE.translator();
        text.lines().flat_map(|l| t.decode(l)).collect()
    }

    fn shell_events(ev: &[Event]) -> Vec<&Event> {
        ev.iter()
            .filter(|e| matches!(e, Event::ShellStarted { .. } | Event::ShellEnded { .. }))
            .collect()
    }

    #[test]
    fn background_shells_fixture() {
        let ev = decode_all(SHELLS);
        let shells = shell_events(&ev);
        let dir = "/private/tmp/claude-502/-tmp-fx/1bcf2865-482a-4bd6-9e13-f6e08c448d88/tasks";
        assert_eq!(
            shells,
            [
                &Event::ShellStarted {
                    id: "bgx0c802g".into(),
                    call: "toolu_01TemeDhovkWgPbSZF6sQprD".into(),
                    description: "Sleep for two minutes in Python".into(),
                    command: "python3 -c 'import time; time.sleep(120)'".into(),
                    output: Some(PathBuf::from(format!("{dir}/bgx0c802g.output"))),
                },
                &Event::ShellStarted {
                    id: "bscwsxv03".into(),
                    call: "toolu_011VXxGjWBqzMsjqBd4Q8ebD".into(),
                    description: "Print three steps with pauses".into(),
                    command: "for i in 1 2 3; do echo step $i; sleep 3; done".into(),
                    output: Some(PathBuf::from(format!("{dir}/bscwsxv03.output"))),
                },
                &Event::ShellEnded {
                    id: "bscwsxv03".into(),
                    end: ShellEnd::Done,
                    output: Some(PathBuf::from(format!("{dir}/bscwsxv03.output"))),
                },
                &Event::ShellEnded {
                    id: "bgx0c802g".into(),
                    end: ShellEnd::Stopped,
                    output: Some(PathBuf::from(format!("{dir}/bgx0c802g.output"))),
                },
            ]
        );
        // Neither is a subagent's lane.
        assert!(
            !ev.iter()
                .any(|e| matches!(e, Event::SubagentStarted { .. }))
        );
    }

    #[test]
    fn a_foreground_bash_task_is_no_shell() {
        assert!(shell_events(&decode_all(QUEUE)).is_empty());
    }

    /// The lines of one background shell: its call, its start and (without the tool_result) its
    /// notification saying `status` and `summary`.
    fn one_shell(t: &mut Box<dyn crate::providers::Translator>, result: bool) -> Vec<Event> {
        let lines = [
            json!({"type":"assistant","message":{"id":"m1","content":[{"type":"tool_use",
                "id":"toolu_a","name":"Bash","input":{"command":"cargo test",
                "description":"Run the tests","run_in_background":true}}]}}),
            json!({"type":"system","subtype":"task_started","task_id":"b1",
                "tool_use_id":"toolu_a","description":"Run the tests","is_backgrounded":true,
                "task_type":"local_bash"}),
        ];
        let mut ev: Vec<Event> = lines
            .iter()
            .flat_map(|l| t.decode(&l.to_string()))
            .collect();
        if result {
            ev.extend(t.decode(
                &json!({"type":"user","message":{"role":"user","content":[{"type":"tool_result",
                "tool_use_id":"toolu_a","content":"Command running in background with ID: b1. \
                Output is being written to: /tmp/fx/tasks/b1.output. You will be notified when \
                it completes.","is_error":false}]}})
                .to_string(),
            ));
        }
        ev
    }

    fn notify(status: &str, summary: &str, output: &str) -> String {
        json!({"type":"system","subtype":"task_notification","task_id":"b1",
            "tool_use_id":"toolu_a","status":status,"output_file":output,"summary":summary})
        .to_string()
    }

    #[test]
    fn a_nonzero_exit_is_a_failed_shell() {
        let mut t = CLAUDE.translator();
        one_shell(&mut t, true);
        let ev = t.decode(&notify(
            "completed",
            "Background command \"Run the tests\" completed (exit code 101)",
            "/tmp/fx/tasks/b1.output",
        ));
        assert!(matches!(
            ev.as_slice(),
            [Event::ShellEnded {
                end: ShellEnd::Failed,
                ..
            }]
        ));
        let mut t = CLAUDE.translator();
        one_shell(&mut t, true);
        let ev = t.decode(&notify("failed", "Run the tests", ""));
        assert!(matches!(
            ev.as_slice(),
            [Event::ShellEnded {
                end: ShellEnd::Failed,
                // No file said: the state keeps the one its start named.
                output: None,
                ..
            }]
        ));
    }

    #[test]
    fn a_notification_before_the_result_still_starts_and_ends_it() {
        let mut t = CLAUDE.translator();
        let started = one_shell(&mut t, false);
        assert!(shell_events(&started).is_empty(), "held for its result");
        let ev = t.decode(&notify("completed", "done", "/tmp/fx/tasks/b1.output"));
        let out = Some(PathBuf::from("/tmp/fx/tasks/b1.output"));
        assert_eq!(
            ev,
            [
                Event::ShellStarted {
                    id: "b1".into(),
                    call: "toolu_a".into(),
                    description: "Run the tests".into(),
                    command: "cargo test".into(),
                    output: out.clone(),
                },
                Event::ShellEnded {
                    id: "b1".into(),
                    end: ShellEnd::Done,
                    output: out,
                },
            ]
        );
        // A notification for a task that was no shell says nothing.
        assert!(t.decode(&notify("completed", "done", "")).is_empty());
    }

    #[test]
    fn stop_shell_writes_stop_task() {
        let mut t = CLAUDE.translator();
        let out = t.encode(&Action::StopShell { id: "b1".into() });
        assert_eq!(out.len(), 1);
        let v: Value = serde_json::from_str(&out[0]).unwrap();
        assert_eq!(v["type"], "control_request");
        assert_eq!(v["request"]["subtype"], "stop_task");
        assert_eq!(v["request"]["task_id"], "b1");
        // A refused one is said as the shell's.
        let ev = t.decode(
            &json!({"type":"control_response","response":{"subtype":"error",
            "request_id": v["request_id"], "error":"No such task"}})
            .to_string(),
        );
        assert_eq!(
            ev,
            [Event::Notice(
                "Couldn't stop the shell: No such task".into()
            )]
        );
    }
}
