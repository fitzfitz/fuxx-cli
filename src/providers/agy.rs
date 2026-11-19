//! Antigravity's (`agy`) `--output-format stream-json` / `--input-format stream-json` wire format
//! (recorded fixtures: `fixtures/agy-*.jsonl`, agy 1.5.7) to fuxx events and back.

use super::*;
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// An RFC 3339 time (`2026-11-22T14:59:00Z`, `…+07:00`, fractions allowed) as unix seconds.
pub fn rfc3339_unix(text: &str) -> Option<i64> {
    let (date, rest) = text.split_once(['T', 't'])?;
    let mut d = date.split('-');
    let (year, month, day): (i64, i64, i64) = (
        d.next()?.parse().ok()?,
        d.next()?.parse().ok()?,
        d.next()?.parse().ok()?,
    );
    if d.next().is_some() || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    // The time ends at the zone: `Z`, or a sign and `HH:MM`.
    let zone_at = rest.find(['Z', 'z', '+', '-'])?;
    let (time, zone) = rest.split_at(zone_at);
    let offset = match zone {
        "Z" | "z" => 0,
        _ => {
            let sign = if zone.starts_with('-') { -1 } else { 1 };
            let (h, m) = zone[1..].split_once(':')?;
            sign * (h.parse::<i64>().ok()? * 3600 + m.parse::<i64>().ok()? * 60)
        }
    };
    // Fractions of a second are dropped.
    let mut t = time.split('.').next()?.split(':');
    let (h, m, s): (i64, i64, i64) = (
        t.next()?.parse().ok()?,
        t.next()?.parse().ok()?,
        t.next()?.parse().ok()?,
    );
    if t.next().is_some() || h > 23 || m > 59 || s > 60 {
        return None;
    }
    // Days since 1970-01-01 (the proleptic Gregorian calendar).
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(days * 86_400 + h * 3600 + m * 60 + s - offset)
}

const UNREADABLE_USAGE: &str = "Couldn't read usage";

/// A bucket as a limit: with no fraction left it is only its name (never a made-up number).
fn usage_bucket(b: &Value) -> UsageLimit {
    let name = s(&b["name"]).unwrap_or_default();
    match b["remaining_fraction"].as_f64() {
        Some(f) => UsageLimit::Read {
            name,
            left_pct: (f * 100.0).round().clamp(0.0, 100.0) as u8,
            resets: b["reset_time"]
                .as_str()
                .and_then(rfc3339_unix)
                .map(Reset::At),
        },
        None => UsageLimit::Text(name),
    }
}

pub struct Agy;
pub static AGY: Agy = Agy;

static INFO: ProviderInfo = ProviderInfo {
    name: "agy",
    label: "Antigravity",
    badge: "AG",
    who: ("A", "agy"),
    badge_rgb: 0x2d4f8a,
    // The recordings show no way to stop a turn on stdin.
    interrupts: false,
    modes: &[
        ModeChoice {
            id: "accept-edits",
            label: "Accept edits",
            about: "Edits files without asking; asks before anything riskier",
        },
        ModeChoice {
            id: "auto",
            label: "Auto",
            about: "Runs without asking",
        },
        ModeChoice {
            id: "plan",
            label: "Plan",
            about: "Plans first and asks for approval before changing anything",
        },
    ],
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
            .filter(|m| INFO.modes.iter().any(|x| x.id == *m))
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
        // `--model <id as `agy models` lists it>` alone: an id with an effort plus a different
        // `--effort` is refused (Task 0), so a chosen model drops the preset's `--effort` too,
        // and `start.effort` (already in the id: `model_flags`) is not passed.
        let mut owned = OWNED_FLAGS.to_vec();
        if let Some(model) = &start.model {
            owned.extend([("--model", true), ("--effort", true)]);
            a.extend(["--model".to_string(), model.clone()]);
        }
        a.extend(pass_through(&start.extra_args, &owned));
        a
    }

    /// The commands the box answers with its own menus (a whole line, no arguments).
    fn own_command(&self, typed: &str) -> Option<BoxCommand> {
        match typed.trim() {
            "/usage" => Some(BoxCommand::Usage),
            "/model" => Some(BoxCommand::Model),
            "/effort" => Some(BoxCommand::Effort),
            _ => None,
        }
    }

    /// `/usage` in print mode: about three seconds, no turn, no cost.
    fn usage_probe(&self) -> Option<Probe> {
        Some(Probe {
            argv: ["agy", "-p", "/usage", "--output-format", "json"]
                .map(String::from)
                .to_vec(),
            stdin: Vec::new(),
            until: None,
        })
    }

    /// `command.data.groups[].buckets[]`: each bucket's `remaining_fraction` is the share left.
    fn read_usage(&self, output: &str) -> Result<Vec<UsageGroup>, String> {
        let v: Value = serde_json::from_str(output).map_err(|_| UNREADABLE_USAGE.to_string())?;
        let groups = v["command"]["data"]["groups"]
            .as_array()
            .ok_or_else(|| UNREADABLE_USAGE.to_string())?;
        Ok(groups
            .iter()
            .map(|g| UsageGroup {
                name: s(&g["name"]).unwrap_or_default(),
                about: s(&g["description"]),
                limits: g["buckets"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(usage_bucket)
                    .collect(),
            })
            .collect())
    }

    /// `agy models` (about a second), then `/help` in print mode (about three).
    fn catalog_probes(&self) -> Vec<Probe> {
        let probe = |argv: &[&str]| Probe {
            argv: argv.iter().map(|w| w.to_string()).collect(),
            stdin: Vec::new(),
            until: None,
        };
        vec![
            probe(&["agy", "models"]),
            probe(&["agy", "-p", "/help", "--output-format", "json"]),
        ]
    }

    /// The models from `outputs[0]` (none is no catalog), the commands from `outputs[1]` (a
    /// failed `/help` leaves them out rather than the models).
    fn read_catalog(&self, outputs: &[String]) -> Result<Catalog, String> {
        let models = read_models(outputs.first().map_or("", String::as_str));
        let commands = outputs
            .get(1)
            .map(|o| read_commands(o))
            .transpose()
            .map(Option::unwrap_or_default);
        if models.is_empty() {
            return Err(commands
                .err()
                .unwrap_or_else(|| "agy lists no models".into()));
        }
        Ok(Catalog {
            models,
            commands: commands.unwrap_or_default(),
            ..Catalog::default()
        })
    }

    /// The effort's own id, alone (`--effort` is never passed: Task 0); a model with no such
    /// effort goes by its own id.
    fn model_flags(&self, model: &ModelChoice, effort: Option<&str>) -> (String, Option<String>) {
        let id = effort
            .and_then(|l| model.efforts.iter().find(|e| e.level == l))
            .and_then(|e| e.model_id.clone())
            .unwrap_or_else(|| model.id.clone());
        (id, None)
    }

    /// Every change restarts agy on the same `--conversation` with the new flags (Task 0: it
    /// carries on, and `--model` / `--mode` take); it has no live request on stdin.
    fn applies(&self, _change: ChangeKind) -> Applies {
        Applies::Restart
    }

    /// `--conversation <id>` from another folder carries the conversation on (Task 0,
    /// `agy-move.jsonl`), whatever its version.
    fn can_move(&self, _version: Option<&str>) -> bool {
        true
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

    /// A conversation id is a UUID (`init.conversation_id`). (agy picks a new conversation's
    /// id itself: `fresh_conversation_id` stays None.)
    fn is_conversation_id(&self, id: &str) -> bool {
        uuid_shaped(id)
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

/// The effort levels agy builds into a model id, in the order a menu lists them.
const EFFORT_ORDER: &[&str] = &["low", "medium", "high", "xhigh", "max"];

/// `agy models`' stdout (`id<TAB>Name (Effort)` per line) as models: the lines of one name are
/// one model whose efforts carry their own ids. A trailing parenthesis that names no effort
/// stays in the name; a blank or malformed line is skipped.
fn read_models(text: &str) -> Vec<ModelChoice> {
    let mut models: Vec<ModelChoice> = Vec::new();
    for line in text.lines() {
        let Some((id, name)) = line.split_once('\t') else {
            continue;
        };
        let (id, name) = (id.trim(), name.trim());
        if id.is_empty() || name.is_empty() {
            continue;
        }
        let effort = name
            .strip_suffix(')')
            .and_then(|n| n.rsplit_once(" ("))
            .and_then(|(base, level)| {
                let level = level.to_lowercase();
                EFFORT_ORDER
                    .contains(&level.as_str())
                    .then(|| (base.trim().to_string(), level))
            });
        let (label, effort) = match effort {
            Some((base, level)) => (base, Some(level)),
            None => (name.to_string(), None),
        };
        let at = match models.iter().position(|m| m.label == label) {
            Some(at) => at,
            None => {
                // The id without its effort suffix names the model as a whole.
                let base = effort
                    .as_deref()
                    .and_then(|l| id.strip_suffix(&format!("-{l}")))
                    .unwrap_or(id);
                models.push(ModelChoice {
                    id: base.to_string(),
                    label,
                    resolves_to: None,
                    efforts: Vec::new(),
                });
                models.len() - 1
            }
        };
        if let Some(level) = effort {
            models[at].efforts.push(EffortChoice {
                level,
                model_id: Some(id.to_string()),
            });
        }
    }
    for m in &mut models {
        m.efforts.sort_by_key(|e| {
            EFFORT_ORDER
                .iter()
                .position(|l| *l == e.level)
                .unwrap_or(usize::MAX)
        });
    }
    models
}

/// `agy -p /help --output-format json`'s output as commands (`command.data.commands[]`).
fn read_commands(text: &str) -> Result<Vec<AgentCommand>, String> {
    let v: Value = serde_json::from_str(text).map_err(|e| format!("/help is not JSON: {e}"))?;
    let list = v["command"]["data"]["commands"]
        .as_array()
        .ok_or("/help lists no commands")?;
    Ok(list
        .iter()
        .filter_map(|c| {
            Some(AgentCommand {
                name: s(&c["name"])?,
                about: s(&c["description"]).unwrap_or_default(),
                kind: CommandKind::Command,
                aliases: c["aliases"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(s)
                    .collect(),
            })
        })
        .collect())
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

/// The file an agy `edit_file` or `write_file` step writes: its `Path` parameter, as agy gave
/// it (it may start with `~`); none for any other tool.
fn tool_writes(name: &str, params: &Value) -> Vec<String> {
    match name {
        "edit_file" | "write_file" => s(&params["Path"]).into_iter().collect(),
        _ => Vec::new(),
    }
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
                writes: tool_writes(&name, params),
                // M11ag Task 0 checked every agy recording: edit_file / write_file carry only
                // `Path`, never the new text, so agy edits hand over none.
                texts: Vec::new(),
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
            // Its images reach it as paths in `text` (`sends_images` is false).
            Action::Send { text, .. } => {
                vec![json!({"event": "user", "message": {"content": text}}).to_string()]
            }
            Action::Allow { .. }
            | Action::AlwaysAllow { .. }
            | Action::Deny { .. }
            | Action::Interrupt
            | Action::SetModel { .. }
            | Action::SetEffort { .. }
            | Action::SetMode { .. } => Vec::new(),
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

    #[test]
    fn agys_image_limit_is_5_mb_of_the_file() {
        // agy is told the image's path, so the file's own size is what counts.
        assert_eq!(
            AGY.image_limit(),
            crate::providers::ImageLimit::Raw(5 * 1024 * 1024)
        );
    }

    fn decode_all(text: &str) -> Vec<Event> {
        let mut t = AGY.translator();
        text.lines().flat_map(|l| t.decode(l)).collect()
    }

    fn start(resume: Option<&str>, mode: Option<&str>, extra: &[&str]) -> Start {
        Start {
            cwd: PathBuf::from("/tmp/fx"),
            resume: resume.map(String::from),
            new_id: None,
            mode: mode.map(String::from),
            extra_args: extra.iter().map(|s| s.to_string()).collect(),
            model: None,
            effort: None,
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
    fn agy_edits_carry_no_text() {
        let ev = decode_all(TOOLS);
        let edit = ev.iter().find_map(|e| match e {
            Event::ToolStarted {
                name,
                writes,
                texts,
                ..
            } if name == "edit_file" => Some((writes, texts)),
            _ => None,
        });
        let (writes, texts) = edit.expect("an edit_file step");
        assert!(!writes.is_empty());
        assert!(texts.is_empty());
    }

    #[test]
    fn agy_tool_calls_name_written_files_when_they_say() {
        let ev = decode_all(TOOLS);
        let writes: Vec<(&str, &Vec<String>)> = ev
            .iter()
            .filter_map(|e| match e {
                Event::ToolStarted { name, writes, .. } => Some((name.as_str(), writes)),
                _ => None,
            })
            .collect();
        let of = |tool: &str| writes.iter().find(|w| w.0 == tool).map(|w| w.1.clone());
        assert_eq!(of("edit_file"), Some(vec!["/tmp/fx/notes.txt".to_string()]));
        assert_eq!(
            of("write_file"),
            Some(vec![
                "~/.gemini/antigravity-cli/brain/72e5eb35-46f6-4788-9c0f-f93c20324da4/task.md"
                    .to_string()
            ])
        );
        assert_eq!(of("read_file"), Some(Vec::new()));
        assert_eq!(of("run_command"), Some(Vec::new()));
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
        let sent = t.encode(&Action::Send {
            text: "hi".into(),
            images: Vec::new(),
        });
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
        assert_eq!(
            i.modes.iter().map(|m| m.id).collect::<Vec<_>>(),
            ["accept-edits", "auto", "plan"]
        );
        assert_eq!(i.default_mode, Some("accept-edits"));
    }

    /// agy can't be told the id of a new conversation (Ruling 40: its crash orphan is a limit),
    /// and only an id of its shape is ever typed into a shell (the classic view).
    #[test]
    fn agy_chooses_its_own_ids() {
        assert_eq!(AGY.fresh_conversation_id(), None);
        let s = Start {
            new_id: Some("6a1c234d-7469-40ef-b093-c74ad8006c28".into()),
            ..start(None, None, &[])
        };
        assert!(!AGY.argv(&s).iter().any(|w| w.contains("6a1c234d")));
        assert!(AGY.is_conversation_id("6a1c234d-7469-40ef-b093-c74ad8006c28"));
        assert!(!AGY.is_conversation_id("6a1c234d; open -a Calculator"));
    }

    const MODELS: &str = include_str!("fixtures/agy-models.txt");
    const HELP: &str = include_str!("fixtures/agy-help.json");

    #[test]
    fn agy_models_group_by_name_with_effort_ids() {
        let models = read_models(MODELS);
        let opus = models
            .iter()
            .find(|m| m.label == "Claude Opus 5.5")
            .expect("Claude Opus 5.5");
        let high = opus.efforts.iter().find(|e| e.level == "high").unwrap();
        assert_eq!(high.model_id.as_deref(), Some("claude-opus-5-5-high"));
        let levels: Vec<&str> = opus.efforts.iter().map(|e| e.level.as_str()).collect();
        assert_eq!(levels, ["low", "medium", "high"]);
        assert_eq!(opus.id, "claude-opus-5-5");
        // One model per name, however many lines.
        assert_eq!(
            models
                .iter()
                .filter(|m| m.label == "Claude Opus 5.5")
                .count(),
            1
        );
        // A parenthesis that isn't an effort stays in the name; no effort at all.
        let orchestrator = models
            .iter()
            .find(|m| m.label == "Orchestrator (Argon)")
            .unwrap();
        assert!(orchestrator.efforts.is_empty());
        assert_eq!(orchestrator.id, "gemini-pro-orchestrator-g1-pro");
        let hydrogen = models
            .iter()
            .find(|m| m.label == "Gemini 4 Hydrogen")
            .unwrap();
        assert!(hydrogen.efforts.iter().any(|e| e.level == "xhigh"));

        let plain = read_models("m-1\tPlain Model\n");
        assert_eq!(plain.len(), 1);
        assert_eq!(
            (plain[0].id.as_str(), plain[0].label.as_str()),
            ("m-1", "Plain Model")
        );
        assert!(plain[0].efforts.is_empty());
        // Blank and malformed lines (no tab, nothing after it) are skipped.
        assert_eq!(
            read_models("\nno tab here\nid-only\t\n\t\nm-2\tTwo\n").len(),
            1
        );
    }

    #[test]
    fn own_commands() {
        assert_eq!(AGY.own_command("/usage"), Some(BoxCommand::Usage));
        assert_eq!(AGY.own_command("/model"), Some(BoxCommand::Model));
        assert_eq!(AGY.own_command("/effort"), Some(BoxCommand::Effort));
        assert_eq!(AGY.own_command("/help"), None);
        assert_eq!(AGY.own_command("/usage now"), None);
    }

    #[test]
    fn agy_help_is_its_commands() {
        let c = AGY
            .read_catalog(&[MODELS.to_string(), HELP.to_string()])
            .unwrap();
        let help: Value = serde_json::from_str(HELP).unwrap();
        let listed = help["command"]["data"]["commands"].as_array().unwrap();
        assert_eq!(c.commands.len(), listed.len());
        assert!(c.commands.iter().all(|x| x.kind == CommandKind::Command));
        let config = c.commands.iter().find(|x| x.name == "config").unwrap();
        assert_eq!(config.aliases, ["settings"]);
        assert_eq!(config.about, "Open settings panel");
        assert!(!c.fallback_models);
        assert_eq!(c.mode, None);
        assert!(!c.models.is_empty());
        assert!(read_commands("not json").is_err());
        assert!(read_commands(r#"{"command":{}}"#).is_err());
        // Its probes: the model list, then `/help` in print mode.
        let probes = AGY.catalog_probes();
        assert_eq!(probes[0].argv, ["agy", "models"]);
        assert_eq!(
            probes[1].argv,
            ["agy", "-p", "/help", "--output-format", "json"]
        );
        assert!(
            probes
                .iter()
                .all(|p| p.stdin.is_empty() && p.until.is_none())
        );
    }

    #[test]
    fn agy_argv_carries_the_chosen_model_and_mode() {
        // Task 0: `--model <id as agy models lists it>` alone; an id with an effort plus a
        // different `--effort` is refused, so no `--effort` is passed.
        let mut s = start(None, Some("plan"), &["--model", "x", "--effort", "low"]);
        // Nothing chosen: a preset's flags pass through.
        let a = AGY.argv(&s);
        assert!(
            a.ends_with(&["--model", "x", "--effort", "low"].map(String::from)),
            "{a:?}"
        );
        s.model = Some("claude-opus-5-5-high".into());
        s.effort = Some("high".into());
        let a = AGY.argv(&s);
        assert_eq!(
            a.windows(2)
                .filter(|w| *w == ["--model", "claude-opus-5-5-high"])
                .count(),
            1,
            "{a:?}"
        );
        assert!(!a.iter().any(|w| w == "--effort" || w == "x"), "{a:?}");
        assert!(a.windows(2).any(|w| w == ["--mode", "plan"]));
        // The choice turns into those values by the effort's own id.
        let models = read_models(MODELS);
        let opus = models
            .iter()
            .find(|m| m.label == "Claude Opus 5.5")
            .unwrap();
        assert_eq!(
            AGY.model_flags(opus, Some("high")),
            ("claude-opus-5-5-high".to_string(), None)
        );
        assert_eq!(
            AGY.model_flags(opus, None),
            ("claude-opus-5-5".to_string(), None)
        );
    }

    #[test]
    fn agy_modes_keep_their_names() {
        let ids: Vec<&str> = AGY.info().modes.iter().map(|m| m.id).collect();
        assert_eq!(ids, ["accept-edits", "auto", "plan"]);
        assert!(
            AGY.info()
                .modes
                .iter()
                .all(|m| !m.label.is_empty() && !m.about.is_empty())
        );
    }

    const USAGE: &str = include_str!("fixtures/agy-usage.json");

    #[test]
    fn agy_usage_reads_groups_and_buckets() {
        let groups = AGY.read_usage(USAGE).unwrap();
        let v: Value = serde_json::from_str(USAGE).unwrap();
        let listed = v["command"]["data"]["groups"].as_array().unwrap();
        assert_eq!(groups.len(), listed.len());
        for (g, l) in groups.iter().zip(listed) {
            assert_eq!(g.name, l["name"].as_str().unwrap());
            assert_eq!(g.about.as_deref(), l["description"].as_str());
            let buckets = l["buckets"].as_array().unwrap();
            assert_eq!(g.limits.len(), buckets.len());
            for (limit, b) in g.limits.iter().zip(buckets) {
                let want = UsageLimit::Read {
                    name: b["name"].as_str().unwrap().into(),
                    left_pct: (b["remaining_fraction"].as_f64().unwrap() * 100.0).round() as u8,
                    resets: Some(Reset::At(
                        rfc3339_unix(b["reset_time"].as_str().unwrap()).unwrap(),
                    )),
                };
                assert_eq!(*limit, want);
            }
        }
        assert!(matches!(
            groups[0].limits[0],
            UsageLimit::Read { left_pct: 23, .. }
        ));
        // A bucket without a fraction is only its name; no number is made up.
        let odd =
            r#"{"command":{"data":{"groups":[{"name":"G","buckets":[{"name":"Mystery"}]}]}}}"#;
        let g = AGY.read_usage(odd).unwrap();
        assert_eq!(g[0].about, None);
        assert_eq!(g[0].limits, [UsageLimit::Text("Mystery".into())]);
        assert_eq!(
            AGY.read_usage("not json"),
            Err("Couldn't read usage".into())
        );
        assert!(AGY.read_usage(r#"{"command":{}}"#).is_err());
    }

    #[test]
    fn agy_usage_runs_in_print_mode() {
        let p = AGY.usage_probe().expect("agy has /usage");
        assert_eq!(p.argv, ["agy", "-p", "/usage", "--output-format", "json"]);
        assert!(p.stdin.is_empty() && p.until.is_none());
    }

    #[test]
    fn rfc3339_reads_offsets_and_fractions() {
        for t in [
            "2026-11-22T14:59:00Z",
            "2026-11-22T21:59:00+07:00",
            "2026-11-22T14:59:00.123456+00:00",
        ] {
            assert_eq!(rfc3339_unix(t), Some(1795359540), "{t}");
        }
        assert_eq!(rfc3339_unix("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(rfc3339_unix("2026-11-22T09:59:00-05:00"), Some(1795359540));
        for t in [
            "nonsense",
            "",
            "2026-13-22T14:59:00Z",
            "2026-11-22T14:59:00",
        ] {
            assert_eq!(rfc3339_unix(t), None, "{t}");
        }
    }

    #[test]
    fn agy_set_actions_write_nothing() {
        let mut t = AGY.translator();
        for a in [
            Action::SetModel {
                model: "gemini-3.8-flash-low".into(),
            },
            Action::SetEffort {
                effort: "low".into(),
            },
            Action::SetMode {
                mode: "plan".into(),
            },
        ] {
            assert!(t.encode(&a).is_empty(), "{a:?}");
        }
    }

    #[test]
    fn agy_applies_per_task_0() {
        for kind in [
            ChangeKind::Model,
            ChangeKind::Effort,
            ChangeKind::Mode,
            ChangeKind::Folder,
        ] {
            assert_eq!(AGY.applies(kind), Applies::Restart, "{kind:?}");
        }
    }

    /// Task 0: agy moves a conversation (same `--conversation` from the new folder); its
    /// version isn't read, and it knows no folders of its own.
    #[test]
    fn agy_moves_a_conversation_and_knows_no_folders() {
        assert!(AGY.can_move(None));
        assert!(AGY.version_probe().is_none());
        assert!(AGY.known_folders(Path::new("/nonexistent")).is_empty());
    }
}
