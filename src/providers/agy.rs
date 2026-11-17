//! Antigravity's (`agy`) `--output-format stream-json` / `--input-format stream-json` wire format
//! (recorded fixtures: `fixtures/agy-*.jsonl`, agy 1.5.7) to fuxx events and back.

use super::*;
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::path::Path;

pub struct Agy;
pub static AGY: Agy = Agy;

static INFO: ProviderInfo = ProviderInfo {
    name: "agy",
    label: "Antigravity",
    badge: "AG",
    badge_rgb: 0x2d4f8a,
    // The recordings show no way to stop a turn on stdin.
    interrupts: false,
    modes: &["accept-edits", "auto", "plan"],
    default_mode: Some("accept-edits"),
};

/// Flags fuxx sets itself (or never passes), with whether each takes a value: an `extra_args`
/// entry naming one is dropped (with its value). agy's `-p` / `--print` / `--prompt` take the
/// prompt as their value (a bare `-p` swallows the next word, found in Task 0); `-c` is
/// `--continue`'s alias. `--dangerously-skip-permissions` is never passed: agy can't ask fuxx
/// before a tool, so its mode is the only guard.
const OWNED_FLAGS: &[(&str, bool)] = &[
    ("-p", true),
    ("--print", true),
    ("--prompt", true),
    ("--input-format", true),
    ("--output-format", true),
    ("--conversation", true),
    ("--continue", false),
    ("-c", false),
    ("--mode", true),
    ("--dangerously-skip-permissions", false),
];

/// The flag that names the conversation an `agy` works on.
const CONVERSATION_FLAGS: &[&str] = &["--conversation"];

impl Provider for Agy {
    fn info(&self) -> &'static ProviderInfo {
        &INFO
    }

    /// `agy -p= …` (an empty prompt: print mode, the turns from stdin), in `start.mode` if agy
    /// has it, else its default.
    fn argv(&self, start: &Start) -> Vec<String> {
        let mode = start
            .mode
            .as_deref()
            .filter(|m| INFO.modes.contains(m))
            .or(INFO.default_mode)
            .unwrap_or("accept-edits");
        let mut a: Vec<String> = [
            "agy",
            "-p=",
            "--input-format",
            "stream-json",
            "--output-format",
            "stream-json",
            "--mode",
            mode,
        ]
        .map(String::from)
        .to_vec();
        if let Some(id) = &start.resume {
            a.extend(["--conversation".to_string(), id.clone()]);
        }
        a.extend(pass_through(&start.extra_args, OWNED_FLAGS));
        a
    }

    fn classic_argv(&self, conversation: &str) -> Vec<String> {
        ["agy", "--conversation", conversation]
            .map(String::from)
            .to_vec()
    }

    /// A native binary (`~/.local/bin/agy`): `ps` shows its arguments as given.
    fn program(&self) -> &'static str {
        "agy"
    }

    /// `--conversation <id>` or `--conversation=<id>`, the id a UUID (`uuid_shaped`), as agy's
    /// conversation ids are (`init.conversation_id`).
    fn conversation_in_args(&self, args: &str) -> Option<String> {
        conversation_after(args, CONVERSATION_FLAGS, uuid_shaped)
    }

    fn translator(&self) -> Box<dyn Translator> {
        Box::new(AgyTranslator::default())
    }

    /// Given a conversation it doesn't have, agy warns `conversation "<id>" not found` on
    /// stderr, exits 0 if its input ends, and otherwise goes on in a new conversation with a
    /// new `init.conversation_id` (`fixtures/agy-bad-resume.jsonl`).
    fn unknown_conversation(&self, said: &str) -> bool {
        said.lines().any(|l| {
            let l = l.trim();
            l.contains("conversation \"") && l.ends_with("\" not found")
        })
    }
}

/// agy's events: `init` (the conversation id), `step_update` (one step of a turn, `ACTIVE`
/// while it runs and `DONE` at its end) and `result` (the turn's end).
#[derive(Default)]
struct AgyTranslator {
    /// Each reply's text so far, by its message id (`<conversation>:<step_index>`).
    said: HashMap<String, String>,
    /// The tool steps started and not finished.
    running: HashSet<String>,
    /// The error rows of this turn (a `result` repeating one adds none).
    errors: Vec<String>,
}

fn s(v: &Value) -> Option<String> {
    v.as_str().map(str::to_string)
}

/// A step's id: `<conversation>:<step_index>` (step indexes go on across a resumed run).
fn step_id(step: &Value) -> String {
    format!(
        "{}:{}",
        step["conversation_id"].as_str().unwrap_or(""),
        step["step_index"]
    )
}

/// A tool step's one-line target: the file name, else the command's first line, else the
/// tool's name.
fn tool_summary(name: &str, params: &Value) -> String {
    if let Some(p) = params["Path"].as_str() {
        return Path::new(p)
            .file_name()
            .map_or_else(|| p.to_string(), |f| f.to_string_lossy().into_owned());
    }
    if let Some(c) = params["CommandLine"].as_str() {
        return first_line(c).to_string();
    }
    name.to_string()
}

/// A `run_command` step's output is agy's header (`Created At`, `Completed At`, `Command
/// exited with code N.`), then `Command output:` and what the command printed: what it
/// printed, and whether it exited 0 (None: no header).
fn command_output(output: &str) -> (String, Option<bool>) {
    let code = output.lines().find_map(|l| {
        l.strip_prefix("Command exited with code ")
            .map(|rest| rest.trim_end_matches('.').trim() == "0")
    });
    let printed = output
        .split_once("Command output:\n")
        .map_or(output, |(_, rest)| rest)
        .replace("\r\n", "\n");
    (printed, code)
}

impl AgyTranslator {
    fn step(&mut self, step: &Value) -> Vec<Event> {
        let done = step["state"] == "DONE";
        match step["step_type"].as_str() {
            Some("agent_response") => self.response(step, done),
            Some("tool") => self.tool(step, done),
            Some("error_message") => {
                // The recordings' error steps carry no text: the `result` brings it.
                let text = ["error", "message", "text_delta"]
                    .iter()
                    .find_map(|k| s(&step[k]).filter(|t| !t.trim().is_empty()));
                text.map(|t| self.error(t)).unwrap_or_default()
            }
            _ => Vec::new(),
        }
    }

    /// A reply streams as `text_delta`s of one step; its `DONE` ends it. A step that only plans
    /// a tool streams nothing and says nothing.
    fn response(&mut self, step: &Value, done: bool) -> Vec<Event> {
        let message = step_id(step);
        let mut out = Vec::new();
        if let Some(text) = s(&step["text_delta"]).filter(|t| !t.is_empty()) {
            self.said
                .entry(message.clone())
                .or_default()
                .push_str(&text);
            out.push(Event::TextDelta {
                message: message.clone(),
                text,
                parent: None,
            });
        }
        if done {
            let text = self.said.remove(&message).unwrap_or_default();
            let text = text.trim_end();
            if !text.is_empty() {
                out.push(Event::MessageDone {
                    message,
                    text: text.to_string(),
                    parent: None,
                });
            }
        }
        out
    }

    /// A tool is one step: `ACTIVE` starts it, `DONE` finishes it (a `DONE` alone does both).
    fn tool(&mut self, step: &Value, done: bool) -> Vec<Event> {
        let id = step_id(step);
        let info = &step["tool_info"];
        let name = s(&step["tool_name"])
            .or_else(|| s(&info["name"]))
            .unwrap_or_default();
        let params = &info["parameters"];
        let mut out = Vec::new();
        if self.running.insert(id.clone()) {
            out.push(Event::ToolStarted {
                id: id.clone(),
                summary: tool_summary(&name, params),
                detail: params["CommandLine"]
                    .as_str()
                    .map(|c| ToolDetail::Command(c.to_string())),
                parent: None,
                name: name.clone(),
            });
        }
        if done {
            self.running.remove(&id);
            let raw = s(&info["output"]).unwrap_or_default();
            let (output, exited_ok) = if name == "run_command" {
                command_output(&raw)
            } else {
                (raw, None)
            };
            let failed = info["error"].is_string() || step["error"].is_string();
            out.push(Event::ToolFinished {
                id,
                ok: !failed && exited_ok != Some(false),
                summary: short(&output, 60),
                output: (!output.trim().is_empty()).then_some(output),
            });
        }
        out
    }

    fn error(&mut self, text: String) -> Vec<Event> {
        if self.errors.contains(&text) {
            return Vec::new();
        }
        self.errors.push(text.clone());
        vec![Event::Error(text)]
    }

    /// The turn's end: an error row (unless one said it already), its usage (agy reports
    /// tokens, no context window or cost), then the turn's outcome.
    fn result(&mut self, result: &Value) -> Vec<Event> {
        let ok = result["status"] == "SUCCESS";
        let message = s(&result["error"]).filter(|e| !e.trim().is_empty());
        let message = match (ok, message) {
            (false, None) => Some(s(&result["finished_reason"]).map_or_else(
                || "The turn failed".into(),
                |r| format!("The turn failed ({r})"),
            )),
            (_, m) => m,
        };
        let mut out = Vec::new();
        if let (false, Some(m)) = (ok, &message) {
            out.extend(self.error(m.clone()));
        }
        self.errors.clear();
        self.said.clear();
        self.running.clear();
        out.push(Event::Usage {
            context_pct: None,
            cost_usd: None,
        });
        out.push(Event::TurnDone { ok, message });
        out
    }
}

impl Translator for AgyTranslator {
    /// agy takes no request before the first message.
    fn hello(&mut self) -> Vec<String> {
        Vec::new()
    }

    fn decode(&mut self, line: &str) -> Vec<Event> {
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            return Vec::new();
        };
        match v["event"].as_str() {
            Some("init") => vec![Event::Started {
                conversation: s(&v["conversation_id"]).unwrap_or_default(),
                model: None,
            }],
            Some("step_update") if v["step_update"].is_object() => self.step(&v["step_update"]),
            Some("result") if v["result"].is_object() => self.result(&v["result"]),
            _ => Vec::new(),
        }
    }

    /// Each turn is one `{"event":"user"}` line (no `role`: agy rejects it). agy asks fuxx
    /// nothing (its prompts are not routed), so there is nothing to answer; and the recordings
    /// show no way to stop a turn, so `Interrupt` writes nothing (the tab's Force stop ends the
    /// process).
    fn encode(&mut self, action: &Action) -> Vec<String> {
        match action {
            Action::Send { text } => {
                vec![json!({"event": "user", "message": {"content": text}}).to_string()]
            }
            Action::Allow { .. }
            | Action::AlwaysAllow { .. }
            | Action::Deny { .. }
            | Action::Interrupt => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    use std::path::PathBuf;

    const TEXT: &str = include_str!("fixtures/agy-text.jsonl");
    const TOOLS: &str = include_str!("fixtures/agy-tools.jsonl");
    const RESUME: &str = include_str!("fixtures/agy-resume.jsonl");
    /// The Task 0 recording: the account was out of credits (429 RESOURCE_EXHAUSTED).
    const CREDITS: &str = include_str!("fixtures/agy-credits.jsonl");
    /// `--conversation 00000000-…`: agy warns on stderr and starts a new conversation.
    const BAD_RESUME: &str = include_str!("fixtures/agy-bad-resume.jsonl");
    const BAD_RESUME_STDERR: &str = include_str!("fixtures/agy-bad-resume.jsonl.stderr");

    fn decode_all(text: &str) -> Vec<Event> {
        let mut t = AGY.translator();
        text.lines().flat_map(|l| t.decode(l)).collect()
    }

    fn start(resume: Option<&str>, mode: Option<&str>, extra: &[&str]) -> Start {
        Start {
            cwd: PathBuf::from("/tmp/fx"),
            resume: resume.map(String::from),
            mode: mode.map(String::from),
            extra_args: extra.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn agy_text_fixture_streams_then_finishes() {
        let ev = decode_all(TEXT);
        assert_eq!(
            ev.first(),
            Some(&Event::Started {
                conversation: "6a1c234d-7469-40ef-b093-c74ad8006c28".into(),
                model: None,
            })
        );
        let message = "6a1c234d-7469-40ef-b093-c74ad8006c28:1";
        assert!(ev.contains(&Event::TextDelta {
            message: message.into(),
            text: "Hello".into(),
            parent: None,
        }));
        assert!(ev.contains(&Event::MessageDone {
            message: message.into(),
            text: "Hello".into(),
            parent: None,
        }));
        assert!(ev.iter().any(|e| matches!(e, Event::Usage { .. })));
        assert_eq!(
            ev.last(),
            Some(&Event::TurnDone {
                ok: true,
                message: None
            })
        );
        // Your own message (`user_input`) is the tab's to show, not the decoder's.
        assert!(!ev.iter().any(|e| matches!(e, Event::Error(_))));
    }

    #[test]
    fn agy_tools_fixture_pairs_tools() {
        let ev = decode_all(TOOLS);
        let started: Vec<(&str, &str, &str)> = ev
            .iter()
            .filter_map(|e| match e {
                Event::ToolStarted {
                    id, name, summary, ..
                } => Some((id.as_str(), name.as_str(), summary.as_str())),
                _ => None,
            })
            .collect();
        assert_eq!(
            started.iter().map(|s| s.1).collect::<Vec<_>>(),
            ["read_file", "edit_file", "run_command", "write_file"]
        );
        for (id, ..) in &started {
            let n = ev
                .iter()
                .filter(|e| matches!(e, Event::ToolFinished { id: f, .. } if f == id))
                .count();
            assert_eq!(n, 1, "tool {id} should finish exactly once");
        }
        assert_eq!(started[0].2, "notes.txt");
        assert_eq!(started[3].2, "task.md");
        assert!(ev.iter().any(|e| matches!(
            e,
            Event::ToolStarted { name, summary, detail: Some(ToolDetail::Command(c)), .. }
                if name == "run_command" && summary == "ls" && c == "ls"
        )));
        // `ls`: its output after agy's header, its exit code 0 → ok.
        assert!(ev.iter().any(|e| matches!(
            e,
            Event::ToolFinished { ok: true, summary, output: Some(o), .. }
                if summary == "notes.txt" && o.trim() == "notes.txt"
        )));
        // read_file reports a size, not the file.
        assert!(ev.iter().any(|e| matches!(
            e,
            Event::ToolFinished { ok: true, summary, .. } if summary == "1 lines, 6 bytes"
        )));
        // Steps that only plan a tool say nothing: one reply, the last.
        let done: Vec<&str> = ev
            .iter()
            .filter_map(|e| match e {
                Event::MessageDone { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(done.len(), 1, "{done:?}");
        assert!(done[0].starts_with("Done."), "{done:?}");
        assert!(matches!(ev.last(), Some(Event::TurnDone { ok: true, .. })));
    }

    #[test]
    fn a_failing_command_finishes_not_ok() {
        let mut t = AGY.translator();
        let line = |state: &str, output: Option<&str>| {
            let mut info = json!({"name": "run_command",
                "parameters": {"CommandLine": "false"}});
            if let Some(o) = output {
                info["output"] = json!(o);
            }
            json!({"event": "step_update", "step_update": {"conversation_id": "c",
                "step_index": 6, "state": state, "step_type": "tool",
                "tool_name": "run_command", "tool_info": info}})
            .to_string()
        };
        assert!(matches!(
            t.decode(&line("ACTIVE", None)).as_slice(),
            [Event::ToolStarted { id, .. }] if id == "c:6"
        ));
        let out =
            "Created At: x\nCompleted At: x\nCommand exited with code 1.\n\nCommand output:\n";
        assert!(matches!(
            t.decode(&line("DONE", Some(out))).as_slice(),
            [Event::ToolFinished { id, ok: false, .. }] if id == "c:6"
        ));
    }

    #[test]
    fn agy_errors_map_to_an_error_row() {
        let ev = decode_all(CREDITS);
        let why = "Your AI credits balance is too low to continue.";
        let errors: Vec<&Event> = ev.iter().filter(|e| matches!(e, Event::Error(_))).collect();
        assert_eq!(errors, [&Event::Error(why.into())]);
        let at = ev.iter().position(|e| matches!(e, Event::Error(_)));
        let done = ev.iter().position(|e| matches!(e, Event::TurnDone { .. }));
        assert!(at < done);
        assert_eq!(
            ev.last(),
            Some(&Event::TurnDone {
                ok: false,
                message: Some(why.into())
            })
        );
    }

    #[test]
    fn an_error_step_with_text_is_its_own_row() {
        let mut t = AGY.translator();
        let step = json!({"event": "step_update", "step_update": {"conversation_id": "c",
            "step_index": 1, "state": "DONE", "step_type": "error_message", "error": "boom"}});
        assert_eq!(t.decode(&step.to_string()), [Event::Error("boom".into())]);
        let result = json!({"event": "result", "result": {"conversation_id": "c",
            "status": "ERROR", "error": "boom"}});
        // The same text again isn't a second row (the turn's message carries it).
        assert_eq!(
            t.decode(&result.to_string()),
            [
                Event::Usage {
                    context_pct: None,
                    cost_usd: None
                },
                Event::TurnDone {
                    ok: false,
                    message: Some("boom".into())
                }
            ]
        );
    }

    #[test]
    fn agy_resume_continues_the_conversation() {
        let ev = decode_all(RESUME);
        assert!(matches!(ev.first(),
            Some(Event::Started { conversation, .. }) if conversation == "6a1c234d-7469-40ef-b093-c74ad8006c28"));
        assert!(
            ev.iter()
                .any(|e| matches!(e, Event::MessageDone { text, .. } if text == "Hello"))
        );
        assert!(matches!(ev.last(), Some(Event::TurnDone { ok: true, .. })));
    }

    #[test]
    fn every_agy_fixture_decodes_and_garbage_is_silent() {
        for f in [TEXT, TOOLS, RESUME, CREDITS, BAD_RESUME] {
            decode_all(f);
        }
        let mut t = AGY.translator();
        assert!(t.decode("not json").is_empty());
        assert!(t.decode("{}").is_empty());
        assert!(t.decode(r#"{"event":"step_update"}"#).is_empty());
    }

    #[test]
    fn agy_argv_has_mode_and_never_skips_permissions() {
        let head = [
            "agy",
            "-p=",
            "--input-format",
            "stream-json",
            "--output-format",
            "stream-json",
            "--mode",
        ];
        let a = AGY.argv(&start(None, None, &[]));
        assert_eq!(a[..7], head);
        assert_eq!(a[7..], ["accept-edits"]);
        assert_eq!(AGY.argv(&start(None, Some("plan"), &[]))[7], "plan");
        // A mode agy doesn't have: its default.
        assert_eq!(AGY.argv(&start(None, Some("yolo"), &[]))[7], "accept-edits");
        let extra = [
            "--dangerously-skip-permissions",
            "--model",
            "gemini-3",
            "--mode",
            "auto",
            "--mode=auto",
            "-p",
            "hi",
            "--print=x",
            "--input-format",
            "text",
            "--output-format=json",
            "--conversation",
            "abc",
            "--continue",
            "--effort=high",
        ];
        let a = AGY.argv(&start(None, None, &extra));
        assert!(!a.iter().any(|w| w == "--dangerously-skip-permissions"));
        assert_eq!(
            a[7..],
            ["accept-edits", "--model", "gemini-3", "--effort=high"]
        );
    }

    #[test]
    fn agy_resume_uses_conversation_flag() {
        let id = "6a1c234d-7469-40ef-b093-c74ad8006c28";
        let a = AGY.argv(&start(Some(id), Some("auto"), &["--model", "m"]));
        assert_eq!(a[7..], ["auto", "--conversation", id, "--model", "m"]);
        assert_eq!(AGY.classic_argv(id), ["agy", "--conversation", id]);
    }

    #[test]
    fn a_process_names_its_conversation_only_by_a_conversation_id() {
        let id = "6a1c234d-7469-40ef-b093-c74ad8006c28";
        assert_eq!(AGY.program(), "agy");
        for args in [
            format!("agy --conversation {id}"),
            format!("agy --conversation={id}"),
            format!(
                "/Users/me/.local/bin/agy -p= --input-format stream-json --mode plan --conversation {id}"
            ),
        ] {
            assert_eq!(
                AGY.conversation_in_args(&args).as_deref(),
                Some(id),
                "{args}"
            );
        }
        for args in [
            "agy",
            "agy -c",
            "agy --conversation",
            "agy --conversation abc",
        ] {
            assert_eq!(AGY.conversation_in_args(args), None, "{args}");
        }
        // Claude's flag names nothing for agy.
        assert_eq!(
            AGY.conversation_in_args(&format!("agy --resume {id}")),
            None
        );
        assert_eq!(AGY.model_label("gemini-3-pro"), "gemini-3-pro");
    }

    #[test]
    fn agy_sends_user_events_and_answers_nothing() {
        let mut t = AGY.translator();
        assert!(t.hello().is_empty());
        let sent = t.encode(&Action::Send { text: "hi".into() });
        assert_eq!(sent.len(), 1);
        let v: Value = serde_json::from_str(&sent[0]).unwrap();
        assert_eq!(v, json!({"event": "user", "message": {"content": "hi"}}));
        for action in [
            Action::Allow { id: "x".into() },
            Action::AlwaysAllow { id: "x".into() },
            Action::Deny {
                id: "x".into(),
                reason: None,
            },
            Action::Interrupt,
        ] {
            assert!(t.encode(&action).is_empty(), "{action:?}");
        }
    }

    #[test]
    fn a_dead_conversation_is_told_by_what_agy_said() {
        assert!(AGY.unknown_conversation(BAD_RESUME_STDERR));
        assert!(AGY.unknown_conversation(
            "warning: conversation \"6a1c234d-7469-40ef-b093-c74ad8006c28\" not found\n"
        ));
        // Other failures are not a dead conversation.
        assert!(!AGY.unknown_conversation(include_str!("fixtures/agy-credits.jsonl.stderr")));
        assert!(!AGY.unknown_conversation("error: file \"x\" not found"));
        assert!(!AGY.unknown_conversation(""));
        // It goes on in a new conversation, not the one asked for.
        let ev = decode_all(BAD_RESUME);
        assert!(
            matches!(ev.first(), Some(Event::Started { conversation, .. })
            if conversation != "00000000-0000-0000-0000-000000000000")
        );
        assert!(matches!(ev.last(), Some(Event::TurnDone { ok: true, .. })));
    }

    #[test]
    fn agy_info_is_antigravity() {
        let i = AGY.info();
        assert_eq!(
            (i.name, i.label, i.badge, i.badge_rgb),
            ("agy", "Antigravity", "AG", 0x2d4f8a)
        );
        assert!(!i.interrupts);
        assert_eq!(i.modes, ["accept-edits", "auto", "plan"]);
        assert_eq!(i.default_mode, Some("accept-edits"));
    }
}
