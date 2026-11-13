#![allow(dead_code)] // consumers arrive in M11aa Tasks 7–11; remove when wired
//! Claude Code's `--output-format stream-json` / `--input-format stream-json` wire format
//! (recorded fixtures: `fixtures/claude-*.jsonl`, Claude Code 2.1.323) to fuxx events and back.

use super::*;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::Path;

pub struct Claude;
pub static CLAUDE: Claude = Claude;

static INFO: ProviderInfo = ProviderInfo {
    name: "claude",
    label: "Claude",
    badge: "CC",
    badge_rgb: 0x7a4a2a,
    routes_permissions: true,
    modes: &[],
    default_mode: None,
};

/// Flags fuxx sets itself, with whether each takes a value: an `extra_args` entry naming one
/// is dropped (with its value).
const OWNED_FLAGS: &[(&str, bool)] = &[
    ("-p", false),
    ("--print", false),
    ("--input-format", true),
    ("--output-format", true),
    ("--permission-prompt-tool", true),
    ("--permission-prompts", true),
    ("--resume", true),
    ("-r", true),
    ("--continue", false),
    ("-c", false),
];

fn pass_through(extra: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    let mut skip_value = false;
    for arg in extra {
        if std::mem::take(&mut skip_value) {
            continue;
        }
        let name = arg.split_once('=').map_or(arg.as_str(), |(n, _)| n);
        match OWNED_FLAGS.iter().find(|(f, _)| *f == name) {
            // `--flag=value` carries its value; `--flag value` borrows the next word.
            Some((_, takes_value)) => skip_value = *takes_value && name == arg,
            None => out.push(arg.clone()),
        }
    }
    out
}

impl Provider for Claude {
    fn info(&self) -> &'static ProviderInfo {
        &INFO
    }

    fn argv(&self, start: &Start) -> Vec<String> {
        let mut a: Vec<String> = [
            "claude",
            "-p",
            "--input-format",
            "stream-json",
            "--output-format",
            "stream-json",
            "--verbose",
            "--include-partial-messages",
            "--permission-prompt-tool",
            "stdio",
        ]
        .map(String::from)
        .to_vec();
        if let Some(id) = &start.resume {
            a.extend(["--resume".to_string(), id.clone()]);
        }
        a.extend(pass_through(&start.extra_args));
        a
    }

    fn classic_argv(&self, conversation: &str) -> Vec<String> {
        ["claude", "--resume", conversation]
            .map(String::from)
            .to_vec()
    }

    fn translator(&self) -> Box<dyn Translator> {
        Box::new(ClaudeTranslator::default())
    }
}

#[derive(Default)]
struct ClaudeTranslator {
    /// Open permission requests by request id: the tool input and the suggestions to send back.
    pending: HashMap<String, (Value, Value)>,
    /// The model named by `system/init`, whose `modelUsage` entry sizes the context.
    model: Option<String>,
}

fn s(v: &Value) -> Option<String> {
    v.as_str().map(str::to_string)
}

fn first_line(text: &str) -> &str {
    text.lines().next().unwrap_or("")
}

/// `text`'s first line, cut to `max` characters with an ellipsis.
fn short(text: &str, max: usize) -> String {
    let line = first_line(text.trim_start());
    if line.chars().count() <= max {
        line.to_string()
    } else {
        let mut t: String = line.chars().take(max - 1).collect();
        t.push('…');
        t
    }
}

/// A tool call's one-line target: the file name, else the command's first line, else the
/// pattern, else the tool's name.
fn tool_summary(name: &str, input: &Value) -> String {
    if let Some(p) = input["file_path"].as_str() {
        return Path::new(p)
            .file_name()
            .map_or_else(|| p.to_string(), |f| f.to_string_lossy().into_owned());
    }
    if let Some(c) = input["command"].as_str() {
        return first_line(c).to_string();
    }
    if let Some(p) = input["pattern"].as_str() {
        return p.to_string();
    }
    name.to_string()
}

fn diff_body(old: &str, new: &str) -> String {
    let mut out = String::from("@@\n");
    for (sign, text) in [('-', old), ('+', new)] {
        for line in text.lines() {
            out.push(sign);
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

fn tool_detail(name: &str, input: &Value) -> Option<ToolDetail> {
    let path = s(&input["file_path"]).unwrap_or_default();
    match name {
        "Edit" => Some(ToolDetail::Diff {
            unified: diff_body(
                input["old_string"].as_str().unwrap_or(""),
                input["new_string"].as_str().unwrap_or(""),
            ),
            path,
        }),
        "Write" => Some(ToolDetail::Diff {
            unified: diff_body("", input["content"].as_str().unwrap_or("")),
            path,
        }),
        "Bash" => s(&input["command"]).map(ToolDetail::Command),
        _ => None,
    }
}

fn todos(input: &Value) -> Vec<TodoItem> {
    input["todos"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|t| TodoItem {
                    text: s(&t["content"]).unwrap_or_default(),
                    state: match t["status"].as_str() {
                        Some("completed") => TodoState::Done,
                        Some("in_progress") => TodoState::Active,
                        _ => TodoState::Pending,
                    },
                })
                .collect()
        })
        .unwrap_or_default()
}

/// A tool result's text: the string, or the `text` items of an array joined by newlines.
fn result_text(content: &Value) -> String {
    match content {
        Value::String(t) => t.clone(),
        Value::Array(items) => items
            .iter()
            .filter_map(|i| i["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

/// A permission suggestion in words, for the "Always allow …" button.
fn describe_suggestion(sug: &Value) -> String {
    match sug["type"].as_str() {
        Some("setMode") => match sug["mode"].as_str() {
            Some("acceptEdits") => "accept edits for this session".to_string(),
            Some(m) => format!("{m} mode for this session"),
            None => "change the mode".to_string(),
        },
        Some("addRules") => {
            let rules: Vec<String> = sug["rules"]
                .as_array()
                .map(|r| {
                    r.iter()
                        .map(|r| {
                            let tool = r["toolName"].as_str().unwrap_or("");
                            match r["ruleContent"].as_str() {
                                Some(c) => format!("{tool}({c})"),
                                None => tool.to_string(),
                            }
                        })
                        .collect()
                })
                .unwrap_or_default();
            rules.join(", ")
        }
        Some(other) => other.to_string(),
        None => "this".to_string(),
    }
}

/// The share of the context window the last model call used, from a `result` event.
/// The window is the session model's (`model`, from init), else the largest one reported.
fn context_pct(result: &Value, model: Option<&str>) -> Option<u8> {
    let usage = result["modelUsage"].as_object()?;
    let window = model
        .and_then(|m| usage.get(m))
        .and_then(|m| m["contextWindow"].as_u64())
        .or_else(|| {
            usage
                .values()
                .filter_map(|m| m["contextWindow"].as_u64())
                .max()
        })?;
    let last = result["usage"]["iterations"].as_array()?.last()?;
    let used: u64 = [
        "input_tokens",
        "cache_read_input_tokens",
        "cache_creation_input_tokens",
    ]
    .iter()
    .map(|k| last[*k].as_u64().unwrap_or(0))
    .sum();
    (window > 0).then(|| (used * 100 / window).min(100) as u8)
}

fn control_response(request_id: &str, response: Value) -> String {
    json!({"type": "control_response",
        "response": {"subtype": "success", "request_id": request_id, "response": response}})
    .to_string()
}

impl ClaudeTranslator {
    fn assistant(&self, v: &Value) -> Vec<Event> {
        let message = s(&v["message"]["id"]).unwrap_or_default();
        let parent = s(&v["parent_tool_use_id"]);
        let mut out = Vec::new();
        for item in v["message"]["content"].as_array().into_iter().flatten() {
            match item["type"].as_str() {
                Some("text") => {
                    let text = item["text"].as_str().unwrap_or("");
                    if !text.is_empty() {
                        out.push(Event::MessageDone {
                            message: message.clone(),
                            text: text.into(),
                            parent: parent.clone(),
                        });
                    }
                }
                Some("tool_use") => {
                    let name = s(&item["name"]).unwrap_or_default();
                    let input = &item["input"];
                    if name == "TodoWrite" {
                        out.push(Event::Todo(todos(input)));
                    } else {
                        out.push(Event::ToolStarted {
                            id: s(&item["id"]).unwrap_or_default(),
                            summary: tool_summary(&name, input),
                            detail: tool_detail(&name, input),
                            parent: parent.clone(),
                            name,
                        });
                    }
                }
                _ => {}
            }
        }
        out
    }

    fn user(&self, v: &Value) -> Vec<Event> {
        let Some(items) = v["message"]["content"].as_array() else {
            return Vec::new();
        };
        items
            .iter()
            .filter(|i| i["type"] == "tool_result")
            .map(|i| {
                let text = result_text(&i["content"]);
                Event::ToolFinished {
                    id: s(&i["tool_use_id"]).unwrap_or_default(),
                    ok: !i["is_error"].as_bool().unwrap_or(false),
                    summary: short(&text, 60),
                    output: (!text.is_empty()).then_some(text),
                }
            })
            .collect()
    }

    fn system(&mut self, v: &Value) -> Vec<Event> {
        match v["subtype"].as_str() {
            Some("init") => {
                self.model = s(&v["model"]);
                vec![Event::Started {
                    conversation: s(&v["session_id"]).unwrap_or_default(),
                    model: s(&v["model"]),
                }]
            }
            Some("permission_denied") => s(&v["message"])
                .or_else(|| s(&v["tool_name"]).map(|t| format!("{t} was denied")))
                .map(Event::Notice)
                .into_iter()
                .collect(),
            Some("hook_response") => match v["exit_code"].as_i64() {
                Some(code) if code != 0 => {
                    let name = v["hook_name"].as_str().unwrap_or("hook");
                    let why = [&v["stderr"], &v["output"]]
                        .iter()
                        .filter_map(|f| f.as_str())
                        .find(|t| !t.trim().is_empty())
                        .map(|t| short(t, 120))
                        .unwrap_or_else(|| format!("exit {code}"));
                    vec![Event::Notice(format!("{name}: {why}"))]
                }
                _ => Vec::new(),
            },
            _ => Vec::new(),
        }
    }

    fn stream_event(&self, v: &Value) -> Vec<Event> {
        let delta = &v["event"]["delta"];
        if v["event"]["type"] != "content_block_delta" || delta["type"] != "text_delta" {
            return Vec::new();
        }
        vec![Event::TextDelta {
            message: s(&v["api_message_id"]).unwrap_or_default(),
            text: s(&delta["text"]).unwrap_or_default(),
            parent: s(&v["parent_tool_use_id"]),
        }]
    }

    fn result(&self, v: &Value) -> Vec<Event> {
        let is_error = v["is_error"].as_bool().unwrap_or(false);
        let errors: Vec<&str> = v["errors"]
            .as_array()
            .map(|e| e.iter().filter_map(|m| m.as_str()).collect())
            .unwrap_or_default();
        let aborted = v["terminal_reason"]
            .as_str()
            .is_some_and(|r| r.starts_with("aborted"));
        let mut out = Vec::new();
        let mut message = s(&v["result"]).filter(|m| !m.is_empty());
        if aborted {
            // Stop: the `errors` are an internal diagnostic, not for the user.
            out.push(Event::Notice("Stopped".into()));
            message = None;
        } else if is_error && !errors.is_empty() {
            let joined = errors.join("; ");
            out.push(Event::Error(joined.clone()));
            message = Some(joined);
        } else if is_error && message.is_none() {
            // A failure with no text (e.g. `error_max_turns`): say which kind it was.
            message = Some(s(&v["subtype"]).unwrap_or_else(|| "The turn failed".into()));
        }
        out.push(Event::Usage {
            context_pct: context_pct(v, self.model.as_deref()),
            cost_usd: v["total_cost_usd"].as_f64(),
        });
        out.push(Event::TurnDone {
            ok: !is_error,
            message,
        });
        out
    }

    fn permission(&mut self, v: &Value) -> Vec<Event> {
        let req = &v["request"];
        let Some(id) = s(&v["request_id"]) else {
            return Vec::new();
        };
        let name = req["tool_name"].as_str().unwrap_or("");
        let input = req["input"].clone();
        let suggestions = req["permission_suggestions"].clone();
        let what = if name == "Bash" {
            s(&input["command"])
        } else {
            None
        }
        .or_else(|| s(&req["description"]))
        .unwrap_or_else(|| tool_summary(name, &input));
        let event = Event::Permission {
            id: id.clone(),
            tool: s(&req["display_name"]).unwrap_or_else(|| name.to_string()),
            what,
            detail: serde_json::to_string_pretty(&input).unwrap_or_default(),
            always: suggestions
                .as_array()
                .and_then(|a| a.first())
                .map(describe_suggestion),
        };
        self.pending.insert(id, (input, suggestions));
        vec![event]
    }
}

impl Translator for ClaudeTranslator {
    fn hello(&mut self) -> Vec<String> {
        vec![
            json!({"type": "control_request", "request_id": "fuxx-init",
            "request": {"subtype": "initialize", "hooks": null}})
            .to_string(),
        ]
    }

    fn decode(&mut self, line: &str) -> Vec<Event> {
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            return Vec::new();
        };
        match v["type"].as_str() {
            Some("system") => self.system(&v),
            Some("stream_event") => self.stream_event(&v),
            Some("assistant") => self.assistant(&v),
            Some("user") => self.user(&v),
            Some("result") => self.result(&v),
            Some("control_request") if v["request"]["subtype"] == "can_use_tool" => {
                self.permission(&v)
            }
            _ => Vec::new(),
        }
    }

    fn encode(&mut self, action: &Action) -> Vec<String> {
        match action {
            Action::Send { text } => vec![
                json!({"type": "user",
                "message": {"role": "user", "content": text}})
                .to_string(),
            ],
            Action::Interrupt => vec![
                json!({"type": "control_request", "request_id": "fuxx-stop",
                "request": {"subtype": "interrupt"}})
                .to_string(),
            ],
            Action::Allow { id } | Action::AlwaysAllow { id } => {
                let Some((input, suggestions)) = self.pending.remove(id) else {
                    return Vec::new();
                };
                let mut response = json!({"behavior": "allow", "updatedInput": input});
                if matches!(action, Action::AlwaysAllow { .. }) {
                    response["updatedPermissions"] = suggestions;
                }
                vec![control_response(id, response)]
            }
            Action::Deny { id, reason } => {
                if self.pending.remove(id).is_none() {
                    return Vec::new();
                }
                let message = reason.as_deref().unwrap_or("The user denied this.");
                vec![control_response(
                    id,
                    json!({"behavior": "deny", "message": message}),
                )]
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn decode_all(p: &dyn Provider, text: &str) -> Vec<Event> {
        let mut t = p.translator();
        text.lines().flat_map(|l| t.decode(l)).collect()
    }

    fn lines_of(text: &str) -> Vec<Value> {
        text.lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }

    #[test]
    fn text_fixture_streams_then_finishes() {
        let ev = decode_all(&CLAUDE, include_str!("fixtures/claude-text.jsonl"));
        assert!(matches!(ev.first(), Some(Event::Started { .. })));
        assert!(ev.iter().any(|e| matches!(e, Event::TextDelta { .. })));
        assert!(matches!(ev.last(), Some(Event::TurnDone { ok: true, .. })));
        // The delta and the finished message share the API message id.
        let delta = ev.iter().find_map(|e| match e {
            Event::TextDelta { message, text, .. } => Some((message.clone(), text.clone())),
            _ => None,
        });
        let done = ev.iter().find_map(|e| match e {
            Event::MessageDone { message, text, .. } => Some((message.clone(), text.clone())),
            _ => None,
        });
        let (dm, dt) = delta.unwrap();
        let (fm, ft) = done.unwrap();
        assert!(dm.starts_with("msg_"));
        assert_eq!(dm, fm);
        assert_eq!(dt, "Hello!");
        assert_eq!(ft, "Hello!");
        // Usage carries the recorded cost and a context percentage from the last call.
        let usage = ev.iter().find_map(|e| match e {
            Event::Usage {
                context_pct,
                cost_usd,
            } => Some((*context_pct, *cost_usd)),
            _ => None,
        });
        let (pct, cost) = usage.unwrap();
        assert!(pct.is_some());
        assert!((cost.unwrap() - 0.2116182).abs() < 1e-9);
    }

    #[test]
    fn started_carries_session_and_model() {
        let ev = decode_all(&CLAUDE, include_str!("fixtures/claude-text.jsonl"));
        assert_eq!(
            ev.first(),
            Some(&Event::Started {
                conversation: "ed50a2b7-6678-4a7c-9ed9-cfa63240ae19".into(),
                model: Some("claude-opus-5-5".into()),
            })
        );
    }

    #[test]
    fn tools_fixture_pairs_every_start_with_a_finish() {
        let ev = decode_all(&CLAUDE, include_str!("fixtures/claude-tools.jsonl"));
        let started: Vec<&str> = ev
            .iter()
            .filter_map(|e| match e {
                Event::ToolStarted { id, .. } => Some(id.as_str()),
                _ => None,
            })
            .collect();
        assert!(started.len() >= 4);
        for id in &started {
            let n = ev
                .iter()
                .filter(|e| matches!(e, Event::ToolFinished { id: f, .. } if f == id))
                .count();
            assert_eq!(n, 1, "tool {id} should finish exactly once");
        }
        assert!(ev.iter().any(|e| matches!(
            e,
            Event::ToolStarted { name, detail: Some(ToolDetail::Diff { path, unified }), .. }
                if name == "Edit" && path == "/tmp/fx/notes.txt"
                    && unified.contains("-alpha") && unified.contains("+beta")
        )));
        assert!(ev.iter().any(|e| matches!(
            e,
            Event::ToolStarted { name, summary, detail: Some(ToolDetail::Command(c)), .. }
                if name == "Bash" && summary.starts_with("ls") && c == "ls"
        )));
        assert!(ev.iter().any(|e| matches!(
            e,
            Event::ToolStarted { name, summary, .. } if name == "Read" && summary == "notes.txt"
        )));
    }

    #[test]
    fn a_hook_blocked_tool_finishes_not_ok() {
        let ev = decode_all(&CLAUDE, include_str!("fixtures/claude-tools.jsonl"));
        let blocked = ev.iter().find_map(|e| match e {
            Event::ToolFinished {
                id,
                ok: false,
                summary,
                output,
            } => Some((id.clone(), summary.clone(), output.clone())),
            _ => None,
        });
        let (id, summary, output) = blocked.expect("a ToolFinished with ok: false");
        assert_eq!(id, "toolu_01K4WPuXdknzAP9dxJkA4stp");
        assert!(summary.starts_with("PreToolUse:Edit hook error"));
        assert!(summary.chars().count() <= 60);
        assert!(output.unwrap().contains("Fact-Forcing Gate"));
        // A normal result is ok and its summary is the first line.
        assert!(ev.iter().any(|e| matches!(
            e,
            Event::ToolFinished { ok: true, summary, .. } if summary == "notes.txt"
        )));
    }

    #[test]
    fn deny_fixture_maps_the_denial_to_a_failed_tool() {
        let ev = decode_all(
            &CLAUDE,
            include_str!("fixtures/claude-permission-deny.jsonl"),
        );
        assert!(ev.iter().any(|e| matches!(
            e,
            Event::ToolFinished { ok: false, summary, .. } if summary == "Not now"
        )));
    }

    #[test]
    fn write_maps_to_an_all_added_diff() {
        let ev = decode_all(
            &CLAUDE,
            include_str!("fixtures/claude-permission-allow.jsonl"),
        );
        assert!(ev.iter().any(|e| matches!(
            e,
            Event::ToolStarted { name, detail: Some(ToolDetail::Diff { path, unified }), .. }
                if name == "Write" && path.ends_with("hello.txt")
                    && unified.contains("+hi") && !unified.contains("\n-")
        )));
    }

    /// Synthetic: this Claude build offers no TodoWrite, so no fixture has one. The shape is
    /// the documented `input.todos[{content, status}]`.
    #[test]
    fn todo_write_maps_to_a_todo_event() {
        let line = json!({"type":"assistant","parent_tool_use_id":null,"message":{"id":"msg_x",
            "content":[{"type":"tool_use","id":"toolu_t","name":"TodoWrite","input":{"todos":[
                {"content":"Plan","status":"completed","activeForm":"Planning"},
                {"content":"Build","status":"in_progress","activeForm":"Building"},
                {"content":"Ship","status":"pending","activeForm":"Shipping"}]}}]}})
        .to_string();
        let ev = CLAUDE.translator().decode(&line);
        let item = |t: &str, s| TodoItem {
            text: t.into(),
            state: s,
        };
        assert_eq!(
            ev,
            vec![Event::Todo(vec![
                item("Plan", TodoState::Done),
                item("Build", TodoState::Active),
                item("Ship", TodoState::Pending),
            ])]
        );
    }

    #[test]
    fn a_subagent_tool_keeps_its_parent() {
        let line = json!({"type":"assistant","parent_tool_use_id":"toolu_parent","message":{"id":"m",
            "content":[{"type":"tool_use","id":"toolu_c","name":"Grep","input":{"pattern":"foo"}}]}})
        .to_string();
        assert_eq!(
            CLAUDE.translator().decode(&line),
            vec![Event::ToolStarted {
                id: "toolu_c".into(),
                name: "Grep".into(),
                summary: "foo".into(),
                parent: Some("toolu_parent".into()),
                detail: None,
            }]
        );
    }

    fn permission_line() -> (String, Value) {
        let v = lines_of(include_str!("fixtures/claude-permission-allow.jsonl"))
            .into_iter()
            .find(|v| v["request"]["subtype"] == "can_use_tool")
            .unwrap();
        (v.to_string(), v)
    }

    #[test]
    fn permission_request_decodes_to_a_card() {
        let (line, req) = permission_line();
        let ev = CLAUDE.translator().decode(&line);
        assert_eq!(ev.len(), 1);
        let Event::Permission {
            id,
            tool,
            what,
            detail,
            always,
        } = &ev[0]
        else {
            panic!("not a Permission: {ev:?}");
        };
        assert_eq!(id, req["request_id"].as_str().unwrap());
        assert_eq!(tool, "Write");
        assert_eq!(what, "hello.txt");
        assert!(detail.contains("\"file_path\"") && detail.contains('\n'));
        assert_eq!(always.as_deref(), Some("accept edits for this session"));
    }

    #[test]
    fn permission_request_round_trips() {
        let (line, req) = permission_line();
        let rid = req["request_id"].as_str().unwrap().to_string();
        let mut t = CLAUDE.translator();
        t.decode(&line);

        let out = t.encode(&Action::Allow { id: rid.clone() });
        assert_eq!(out.len(), 1);
        let v: Value = serde_json::from_str(&out[0]).unwrap();
        assert_eq!(v["type"], "control_response");
        assert_eq!(v["response"]["subtype"], "success");
        assert_eq!(v["response"]["request_id"], rid.as_str());
        assert_eq!(v["response"]["response"]["behavior"], "allow");
        assert_eq!(
            v["response"]["response"]["updatedInput"],
            req["request"]["input"]
        );
        assert!(
            v["response"]["response"]
                .get("updatedPermissions")
                .is_none()
        );

        // An answered request is forgotten, so each action below decodes the request afresh.
        t.decode(&line);
        let out = t.encode(&Action::AlwaysAllow { id: rid.clone() });
        let v: Value = serde_json::from_str(&out[0]).unwrap();
        assert_eq!(v["response"]["response"]["behavior"], "allow");
        assert_eq!(
            v["response"]["response"]["updatedPermissions"],
            req["request"]["permission_suggestions"]
        );

        t.decode(&line);
        let out = t.encode(&Action::Deny {
            id: rid.clone(),
            reason: Some("Not now".into()),
        });
        let v: Value = serde_json::from_str(&out[0]).unwrap();
        assert_eq!(v["response"]["response"]["behavior"], "deny");
        assert_eq!(v["response"]["response"]["message"], "Not now");

        t.decode(&line);
        let out = t.encode(&Action::Deny {
            id: rid,
            reason: None,
        });
        let v: Value = serde_json::from_str(&out[0]).unwrap();
        assert_eq!(
            v["response"]["response"]["message"],
            "The user denied this."
        );
    }

    #[test]
    fn an_unknown_permission_id_encodes_nothing() {
        let mut t = CLAUDE.translator();
        assert!(t.encode(&Action::Allow { id: "nope".into() }).is_empty());
    }

    #[test]
    fn bash_permission_shows_the_command_and_the_rule() {
        let v = lines_of(include_str!("fixtures/claude-interrupt.jsonl"))
            .into_iter()
            .find(|v| v["request"]["subtype"] == "can_use_tool")
            .unwrap();
        let ev = CLAUDE.translator().decode(&v.to_string());
        let Event::Permission {
            tool, what, always, ..
        } = &ev[0]
        else {
            panic!("{ev:?}")
        };
        assert_eq!(tool, "Bash");
        assert_eq!(what, "python3 -c 'import time; time.sleep(60)'");
        assert_eq!(
            always.as_deref(),
            Some("Bash(python3 -c 'import time; time.sleep(60)')")
        );
    }

    #[test]
    fn send_and_interrupt_encode() {
        let mut t = CLAUDE.translator();
        let out = t.encode(&Action::Send { text: "hi".into() });
        let v: Value = serde_json::from_str(&out[0]).unwrap();
        assert_eq!(
            v,
            json!({"type":"user","message":{"role":"user","content":"hi"}})
        );
        let out = t.encode(&Action::Interrupt);
        let v: Value = serde_json::from_str(&out[0]).unwrap();
        assert_eq!(v["type"], "control_request");
        assert_eq!(v["request"]["subtype"], "interrupt");
    }

    #[test]
    fn decode_ignores_unknown_and_garbage() {
        let mut t = CLAUDE.translator();
        assert!(t.decode("not json").is_empty());
        assert!(t.decode("").is_empty());
        assert!(t.decode(r#"{"type":"brand_new_thing"}"#).is_empty());
        assert!(t.decode(r#"{"type":"assistant"}"#).is_empty());
        assert!(
            t.decode(r#"{"type":"system","subtype":"status"}"#)
                .is_empty()
        );
        assert!(t.decode(r#"{"type":"rate_limit_event"}"#).is_empty());
    }

    #[test]
    fn thinking_and_json_deltas_are_silent() {
        let mut t = CLAUDE.translator();
        for d in [
            json!({"type":"thinking_delta","thinking":"hm"}),
            json!({"type":"signature_delta","signature":""}),
            json!({"type":"input_json_delta","partial_json":"{"}),
        ] {
            let line = json!({"type":"stream_event","api_message_id":"msg_1",
                "event":{"type":"content_block_delta","index":0,"delta":d}})
            .to_string();
            assert!(t.decode(&line).is_empty());
        }
        let line = json!({"type":"assistant","message":{"id":"m","content":[
            {"type":"thinking","thinking":"x","signature":""}]}})
        .to_string();
        assert!(t.decode(&line).is_empty());
    }

    #[test]
    fn a_failing_hook_is_a_notice_and_a_clean_one_is_silent() {
        let mut t = CLAUDE.translator();
        let bad = json!({"type":"system","subtype":"hook_response","hook_name":"PreToolUse:Bash",
            "exit_code":2,"stderr":"blocked by policy","output":"","outcome":"error"})
        .to_string();
        let ev = t.decode(&bad);
        assert!(
            matches!(&ev[..], [Event::Notice(m)] if m.contains("PreToolUse:Bash") && m.contains("blocked by policy"))
        );
        let ok = decode_all(&CLAUDE, include_str!("fixtures/claude-text.jsonl"));
        assert!(!ok.iter().any(|e| matches!(e, Event::Notice(_))));
    }

    #[test]
    fn permission_denied_is_a_notice() {
        // Synthetic: no fixture has `system/permission_denied` (the field name is a guess).
        let line =
            json!({"type":"system","subtype":"permission_denied","message":"Bash was denied"})
                .to_string();
        assert_eq!(
            CLAUDE.translator().decode(&line),
            vec![Event::Notice("Bash was denied".into())]
        );
    }

    #[test]
    fn resume_failure_maps_to_an_error() {
        let ev = decode_all(&CLAUDE, include_str!("fixtures/claude-bad-resume.jsonl"));
        assert!(ev.iter().any(|e| matches!(
            e,
            Event::Error(m) if m.contains("No conversation found")
        )));
        assert!(matches!(ev.last(), Some(Event::TurnDone { ok: false, .. })));
    }

    #[test]
    fn a_stop_is_a_notice_not_a_diagnostic() {
        let ev = decode_all(
            &CLAUDE,
            include_str!("fixtures/claude-interrupt-text.jsonl"),
        );
        assert!(ev.contains(&Event::Notice("Stopped".into())));
        assert!(!ev.iter().any(|e| matches!(e, Event::Error(_))));
        assert!(!format!("{ev:?}").contains("ede_diagnostic"));
        assert!(matches!(
            ev.last(),
            Some(Event::TurnDone {
                ok: false,
                message: None
            })
        ));
    }

    /// Synthetic: no fixture has a failed turn without text (e.g. max turns).
    #[test]
    fn a_failure_without_text_falls_back_to_the_subtype() {
        let line = json!({"type":"result","subtype":"error_max_turns","is_error":true,
            "result":null,"terminal_reason":"max_turns","total_cost_usd":0.1})
        .to_string();
        let ev = CLAUDE.translator().decode(&line);
        assert!(matches!(
            ev.last(),
            Some(Event::TurnDone { ok: false, message: Some(m) }) if m == "error_max_turns"
        ));
    }

    /// Synthetic: a two-model `result` (the fixtures have one model each).
    #[test]
    fn context_uses_the_session_models_window() {
        let init =
            json!({"type":"system","subtype":"init","session_id":"s","model":"big"}).to_string();
        let result = json!({"type":"result","subtype":"success","is_error":false,"result":"ok",
            "modelUsage":{"small":{"contextWindow":200000},"big":{"contextWindow":1000000}},
            "usage":{"iterations":[{"input_tokens":100000}]}})
        .to_string();
        let pct = |t: &mut Box<dyn Translator>| {
            t.decode(&result).into_iter().find_map(|e| match e {
                Event::Usage { context_pct, .. } => context_pct,
                _ => None,
            })
        };
        let mut t = CLAUDE.translator();
        t.decode(&init);
        assert_eq!(pct(&mut t), Some(10)); // 100k of big's 1M, not small's 200k
        // Without init: the largest window.
        assert_eq!(pct(&mut CLAUDE.translator()), Some(10));
        // Init names the small one: 100k of 200k.
        let mut t = CLAUDE.translator();
        t.decode(&init.replace("big", "small"));
        assert_eq!(pct(&mut t), Some(50));
    }

    /// Synthetic: sub-agent lines carry `parent_tool_use_id` on stream and assistant lines.
    #[test]
    fn subagent_text_keeps_its_parent() {
        let mut t = CLAUDE.translator();
        let delta =
            json!({"type":"stream_event","parent_tool_use_id":"toolu_p","api_message_id":"m",
            "event":{"type":"content_block_delta","index":0,
                "delta":{"type":"text_delta","text":"hi"}}})
            .to_string();
        let done = json!({"type":"assistant","parent_tool_use_id":"toolu_p","message":{"id":"m",
            "content":[{"type":"text","text":"hi"}]}})
        .to_string();
        assert_eq!(
            t.decode(&delta),
            vec![Event::TextDelta {
                message: "m".into(),
                text: "hi".into(),
                parent: Some("toolu_p".into())
            }]
        );
        assert_eq!(
            t.decode(&done),
            vec![Event::MessageDone {
                message: "m".into(),
                text: "hi".into(),
                parent: Some("toolu_p".into())
            }]
        );
    }

    #[test]
    fn interrupt_fixture_ends_in_a_failed_turn() {
        let ev = decode_all(
            &CLAUDE,
            include_str!("fixtures/claude-interrupt-text.jsonl"),
        );
        assert!(matches!(ev.last(), Some(Event::TurnDone { ok: false, .. })));
    }

    #[test]
    fn every_fixture_decodes_without_panic() {
        for text in [
            include_str!("fixtures/claude-queue.jsonl"),
            include_str!("fixtures/claude-slash.jsonl"),
            include_str!("fixtures/claude-resume.jsonl"),
            include_str!("fixtures/claude-permission-always.jsonl"),
        ] {
            let ev = decode_all(&CLAUDE, text);
            assert!(matches!(ev.last(), Some(Event::TurnDone { .. })));
        }
    }

    #[test]
    fn hello_is_the_initialize_request() {
        let mut t = CLAUDE.translator();
        let out = t.hello();
        assert_eq!(out.len(), 1);
        let v: Value = serde_json::from_str(&out[0]).unwrap();
        assert_eq!(v["type"], "control_request");
        assert_eq!(v["request"]["subtype"], "initialize");
    }

    #[test]
    fn argv_drops_flags_fuxx_owns() {
        let s = Start {
            cwd: "/tmp".into(),
            resume: None,
            mode: None,
            extra_args: vec![
                "--model".into(),
                "sonnet".into(),
                "-p".into(),
                "--resume".into(),
                "x".into(),
            ],
        };
        let a = CLAUDE.argv(&s);
        assert!(a.ends_with(&["--model".to_string(), "sonnet".to_string()]));
        assert_eq!(a.iter().filter(|w| *w == "-p").count(), 1);
        assert!(!a.contains(&"x".to_string()));
        assert_eq!(a[0], "claude");
        assert!(
            a.windows(2)
                .any(|w| w == ["--permission-prompt-tool", "stdio"])
        );
    }

    #[test]
    fn argv_drops_every_owned_flag_and_value_and_adds_resume() {
        let s = Start {
            cwd: "/tmp".into(),
            resume: Some("abc".into()),
            mode: None,
            extra_args: [
                "--output-format",
                "text",
                "--input-format=text",
                "--print",
                "-c",
                "--continue",
                "-r",
                "zzz",
                "--permission-prompts",
                "none",
                "--permission-prompt-tool",
                "other",
                "--verbose-ish",
                "keep",
            ]
            .map(String::from)
            .to_vec(),
        };
        let a = CLAUDE.argv(&s);
        assert!(a.windows(2).any(|w| w == ["--resume", "abc"]));
        for gone in [
            "text",
            "-c",
            "--continue",
            "-r",
            "zzz",
            "none",
            "other",
            "--print",
        ] {
            assert!(
                !a.contains(&gone.to_string()),
                "{gone} should be dropped: {a:?}"
            );
        }
        assert_eq!(a.iter().filter(|w| *w == "--output-format").count(), 1);
        assert_eq!(
            a.iter()
                .filter(|w| *w == "--permission-prompt-tool")
                .count(),
            1
        );
        assert!(a.ends_with(&["--verbose-ish".to_string(), "keep".to_string()]));
    }

    #[test]
    fn classic_argv_resumes_the_conversation() {
        assert_eq!(CLAUDE.classic_argv("abc"), ["claude", "--resume", "abc"]);
    }
}
