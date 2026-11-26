//! Antigravity's (`agy`) `--output-format stream-json` / `--input-format stream-json` wire format
//! (recorded fixtures: `fixtures/agy-*.jsonl`, agy 1.5.7) to fuxx events and back.

use super::*;
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet, VecDeque};
use std::ffi::OsStr;
use std::path::Path;

mod activity;

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
    // M11ap: shown beside "N% used", so "… Remaining" would contradict it.
    let name = s(&b["name"]).unwrap_or_default();
    let name = name
        .strip_suffix(" Remaining")
        .map_or(name.clone(), String::from);
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

/// The id agy's main transcript is followed by (`Event::Follow` with `main`): its questions
/// are only there (stdout shows an `ask_question` step as a bare `step_type: "unknown"`).
pub const MAIN_FOLLOW: &str = "agy-main";

/// The mode agy runs in when it refuses the one asked ("continuing in the default mode"
/// names none): Accept edits, the closest of the modes fuxx offers to agy's own default (its
/// `init` says `request-review`: it edits, and asks before anything riskier).
const DEFAULT_RUNNING: &str = "accept-edits";

/// Where agy keeps its data: `$ANTIGRAVITY_GEMINI_DIR` (`env`), else `~/.gemini`.
pub fn gemini_dir(env: Option<&OsStr>, home: Option<&OsStr>) -> Option<PathBuf> {
    match env.filter(|e| !e.is_empty()) {
        Some(dir) => Some(PathBuf::from(dir)),
        None => home
            .filter(|h| !h.is_empty())
            .map(|h| Path::new(h).join(".gemini")),
    }
}

/// Conversation `conversation`'s main transcript under agy's data folder: the same layout as a
/// subagent's `log_uri`.
pub fn transcript_path(gemini_dir: &Path, conversation: &str) -> PathBuf {
    gemini_dir
        .join("antigravity-cli/brain")
        .join(conversation)
        .join(".system_generated/logs/transcript.jsonl")
}

/// This process's agy data folder (agy inherits fuxx's environment). Tests and scenes never
/// read your own: a scrubbed home (`/Users/me`, as the fixtures' paths) stands in.
fn here_gemini_dir() -> Option<PathBuf> {
    if cfg!(any(test, feature = "ui-shots")) {
        return gemini_dir(None, Some(OsStr::new("/Users/me")));
    }
    gemini_dir(
        std::env::var_os("ANTIGRAVITY_GEMINI_DIR").as_deref(),
        std::env::var_os("HOME").as_deref(),
    )
}

/// agy refusing a mode for this account ("--mode auto is not available … continuing in the
/// default mode"): the mode asked, and the one it runs in instead (one of agy's modes it
/// names after "continuing in", else `DEFAULT_RUNNING`). None for any other line.
pub fn mode_refusal(line: &str) -> Option<(String, String)> {
    let known = |w: &str| INFO.modes.iter().any(|m| m.id == w);
    let at = line.find("--mode")?;
    let rest = &line[at + "--mode".len()..];
    if !rest.contains("not available") {
        return None;
    }
    let asked: String = rest
        .trim_start_matches([' ', '=', '"', '\''])
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '-')
        .collect();
    if !known(&asked) {
        return None;
    }
    let running = line
        .split_once("continuing in")
        .and_then(|(_, after)| {
            after
                .split(|c: char| !(c.is_ascii_alphanumeric() || c == '-'))
                .find(|w| known(w) && *w != asked)
        })
        .unwrap_or(DEFAULT_RUNNING);
    Some((asked, running.to_string()))
}

/// An `ask_question` call's questions: `args.questions` is JSON text (`arg`); an entry with no
/// question is left out, and an option is a string or a `{label, description}` object.
fn read_questions(args: &Value) -> Vec<Question> {
    let list = arg(&args["questions"]);
    list.as_array()
        .into_iter()
        .flatten()
        .filter_map(|q| {
            let text = s(&q["question"]).filter(|t| !t.trim().is_empty())?;
            let options = q["options"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|o| match o {
                    Value::String(label) => Some(QOption {
                        label: label.clone(),
                        description: String::new(),
                        preview: None,
                    }),
                    Value::Object(_) => Some(QOption {
                        label: s(&o["label"]).filter(|l| !l.trim().is_empty())?,
                        description: s(&o["description"]).unwrap_or_default(),
                        preview: None,
                    }),
                    _ => None,
                })
                .collect();
            Some(Question {
                header: String::new(),
                text,
                options,
                multi: q["is_multi_select"].as_bool().unwrap_or(false),
            })
        })
        .collect()
}

/// A tool result's answer lines (`A1: Vitest`, `A2: User Skipped`): whether there are any,
/// and whether one says you skipped.
fn answer_lines(content: &str) -> (bool, bool) {
    let answers: Vec<&str> = content
        .lines()
        .filter_map(|l| {
            let rest = l.strip_prefix('A')?;
            let digits = rest.len() - rest.trim_start_matches(|c: char| c.is_ascii_digit()).len();
            (digits > 0)
                .then(|| rest[digits..].strip_prefix(": "))
                .flatten()
        })
        .collect();
    let skipped = answers.iter().any(|a| a.trim() == "User Skipped");
    (!answers.is_empty(), skipped)
}

/// One `ask_question` call read from the main transcript: its step, its card's id and its
/// questions.
struct Ask {
    step: i64,
    id: String,
    questions: Vec<Question>,
}

pub struct Agy;
pub static AGY: Agy = Agy;

static INFO: ProviderInfo = ProviderInfo {
    name: "agy",
    label: "Antigravity",
    badge: "AG",
    who: ("A", "agy"),
    badge_rgb: 0x2d4f8a,
    logo: Some(crate::logos::Logo::Antigravity),
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
    default_mode: Some("auto"),
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
            .unwrap_or("auto");
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

    /// ✦ (M11aq): once, as text.
    fn message_probe(&self, prompt: &str) -> Option<Probe> {
        Some(Probe {
            argv: ["agy", "-p", prompt, "--output-format", "text"]
                .map(String::from)
                .to_vec(),
            stdin: Vec::new(),
            until: None,
            env: Vec::new(),
        })
    }

    /// `/usage` in print mode: about three seconds, no turn, no cost.
    fn usage_probe(&self) -> Option<Probe> {
        Some(Probe {
            argv: ["agy", "-p", "/usage", "--output-format", "json"]
                .map(String::from)
                .to_vec(),
            stdin: Vec::new(),
            until: None,
            env: Vec::new(),
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
            env: Vec::new(),
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

    /// Headless agy can't rewind or fork (its TUI's `/rewind` only; batch 3, chore 2's research).
    fn edit_tip(&self) -> Option<&'static str> {
        Some(
            "Put this message back in the box (to rewind the chat, type /rewind in Open in \
             Classic View)",
        )
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
        Box::new(AgyTranslator {
            gemini_dir: here_gemini_dir(),
            ..AgyTranslator::default()
        })
    }

    /// Print mode skips `ask_question` ("User Skipped") and goes on: the question is read from
    /// the main transcript, and answered by a message (2026-11-24, tested: agy takes it).
    fn questions_by_message(&self) -> bool {
        true
    }

    fn question_note(&self) -> Option<&'static str> {
        Some(
            "agy's print mode skips questions, so it went on without one. Your picks go back as \
             a new message.",
        )
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
    /// The agent activity chore: each tool call (a step's, or a lane's) between its start and
    /// its result.
    activities: HashMap<String, activity::Call>,
    /// The error rows of this turn (a `result` repeating one adds none).
    errors: Vec<String>,
    /// This conversation's id (a subagent reports to it).
    conversation: Option<String>,
    /// Subagents (M11ar) started and not yet reported, by lane id.
    unreported: HashSet<String>,
    /// Each lane's tool calls waiting for their result (a transcript's `GENERIC` line).
    calls: HashMap<String, Vec<String>>,
    /// agy's data folder (`gemini_dir`): where the main transcript is; None: not followed.
    gemini_dir: Option<PathBuf>,
    /// The conversation whose main transcript is followed (a new one, after `/clear`, gets
    /// its own `Follow`).
    followed: Option<String>,
    /// The step index of the first stdout `step_update` this process saw: the transcript's
    /// older steps (a resumed conversation's) never ask again.
    first_step: Option<i64>,
    /// The main transcript's `ask_question` calls waiting for their result, oldest first.
    asks: VecDeque<Ask>,
    /// Skipped asks read before the first step said where this run starts.
    held: Vec<Ask>,
    /// The question cards sent, so none shows twice.
    shown: HashSet<String>,
    /// A mode refusal was reported (it is said once).
    refused: bool,
}

/// The file a subagent's `log_uri` names, if it is a local `file://` one (percent-decoded).
pub fn followed_path(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    if rest.is_empty() {
        return None;
    }
    let bytes = rest.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(b) = u8::from_str_radix(&rest[i + 1..i + 3], 16)
        {
            out.push(b);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    Some(PathBuf::from(String::from_utf8_lossy(&out).into_owned()))
}

/// A transcript's tool argument: agy writes each as JSON text (`"\"a.txt\""`); its value.
fn arg(v: &Value) -> Value {
    match v.as_str().map(serde_json::from_str::<Value>) {
        Some(Ok(decoded)) => decoded,
        _ => v.clone(),
    }
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
/// printed, and its exit code (None: no header).
fn command_output(output: &str) -> (String, Option<i32>) {
    let code = output.lines().find_map(|l| {
        l.strip_prefix("Command exited with code ")
            .and_then(|rest| rest.trim_end_matches('.').trim().parse().ok())
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
        if let Some(c) = s(&step["conversation_id"]) {
            self.conversation = Some(c);
        }
        match step["step_type"].as_str() {
            Some("agent_response") => self.response(step, done),
            Some("subagent") => self.subagent(step, done),
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
            // Main's stream has no `toolSummary`: fuxx's own words.
            let (planned, call) = activity::call_started(&name, params, None);
            self.activities.insert(id.clone(), call);
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
            out.push(Event::Activity {
                id: id.clone(),
                activity: planned,
            });
        }
        if done {
            self.running.remove(&id);
            let raw = s(&info["output"]).unwrap_or_default();
            let (output, exit) = if name == "run_command" {
                command_output(&raw)
            } else {
                (raw, None)
            };
            let error = s(&info["error"]).or_else(|| s(&step["error"]));
            let ok = error.is_none() && exit.is_none_or(|c| c == 0);
            let done = self
                .activities
                .remove(&id)
                .map(|call| activity::call_finished(call, ok, exit, &output, error));
            out.push(Event::ToolFinished {
                id: id.clone(),
                ok,
                summary: short(&output, 60),
                output: (!output.trim().is_empty()).then_some(output),
            });
            out.extend(done.map(|activity| Event::Activity { id, activity }));
        }
        out
    }

    /// A `subagent` step (M11ar): per entry a row and a lane (`<step id>#<n>`); `DONE` means
    /// spawned — the row finishes and its transcript is followed (it runs on, in the
    /// background, and reports to this conversation).
    fn subagent(&mut self, step: &Value, done: bool) -> Vec<Event> {
        let mut out = Vec::new();
        let subs = step["subagent_info"]["subagents"].as_array();
        for (n, sub) in subs.into_iter().flatten().enumerate() {
            let lane = format!("{}#{n}", step_id(step));
            let role = s(&sub["role"]).unwrap_or_else(|| "Subagent".into());
            if self.running.insert(lane.clone()) {
                self.unreported.insert(lane.clone());
                out.push(Event::ToolStarted {
                    id: lane.clone(),
                    name: "Subagent".into(),
                    summary: role.clone(),
                    parent: None,
                    detail: None,
                    writes: Vec::new(),
                    texts: Vec::new(),
                });
                out.push(Event::Activity {
                    id: lane.clone(),
                    activity: activity::activity_at_start("Subagent", &Value::Null, None),
                });
                out.push(Event::SubagentStarted {
                    id: lane.clone(),
                    parent: None,
                    label: role,
                    kind: s(&sub["type_name"]).unwrap_or_default(),
                    task: s(&sub["initial_prompt"]).unwrap_or_default(),
                    model: None,
                    // Its transcript's steps are the only sign it woke (the lanes chore).
                    controls: LaneControls {
                        woken_by_steps: true,
                        ..LaneControls::default()
                    },
                });
            }
            if done {
                self.running.remove(&lane);
                out.push(Event::ToolFinished {
                    id: lane.clone(),
                    ok: true,
                    summary: String::new(),
                    output: None,
                });
                out.push(Event::Activity {
                    id: lane.clone(),
                    activity: activity::activity_at_start("Subagent", &Value::Null, None),
                });
                if let Some(path) = sub["log_uri"].as_str().and_then(followed_path) {
                    out.push(Event::Follow {
                        id: lane,
                        path,
                        main: false,
                    });
                }
            }
        }
        out
    }

    /// `init` (2026-11-24): follow the conversation's main transcript, once per conversation
    /// id; a new id (`/clear`) is a new transcript whose steps count from its own start.
    fn follow_main(&mut self, conversation: &str) -> Vec<Event> {
        if self.followed.as_deref() == Some(conversation) || !uuid_shaped(conversation) {
            return Vec::new();
        }
        let Some(dir) = &self.gemini_dir else {
            return Vec::new();
        };
        let path = transcript_path(dir, conversation);
        self.followed = Some(conversation.to_string());
        self.first_step = None;
        self.asks.clear();
        self.held.clear();
        vec![Event::Follow {
            id: MAIN_FOLLOW.to_string(),
            path,
            main: true,
        }]
    }

    /// A stdout step: the first says where this run starts; the skipped asks held until then
    /// show if they are this run's.
    fn note_step(&mut self, step: &Value) -> Vec<Event> {
        let Some(at) = step["step_index"].as_i64() else {
            return Vec::new();
        };
        if self.first_step.is_some() {
            return Vec::new();
        }
        self.first_step = Some(at);
        std::mem::take(&mut self.held)
            .into_iter()
            .flat_map(|ask| self.show(ask))
            .collect()
    }

    /// A skipped ask's card, if it is this run's and not shown yet; held while the run's
    /// first step isn't known.
    fn show(&mut self, ask: Ask) -> Vec<Event> {
        let Some(first) = self.first_step else {
            self.held.push(ask);
            return Vec::new();
        };
        if ask.step < first || ask.questions.is_empty() || !self.shown.insert(ask.id.clone()) {
            return Vec::new();
        }
        vec![Event::Question {
            id: ask.id,
            questions: ask.questions,
            by: None,
        }]
    }

    /// A line of the main transcript: only question cards (stdout gives the rows). An
    /// `ask_question` call waits for its result; a result with answer lines settles the oldest,
    /// and one you skipped (print mode always does) becomes its card.
    fn main_line(&mut self, line: &str) -> Vec<Event> {
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            return Vec::new();
        };
        let Some(step) = v["step_index"].as_i64() else {
            return Vec::new();
        };
        match v["type"].as_str() {
            Some("PLANNER_RESPONSE") => {
                let conversation = self.followed.clone().unwrap_or_default();
                for (n, call) in v["tool_calls"].as_array().into_iter().flatten().enumerate() {
                    if call["name"] == "ask_question" {
                        self.asks.push_back(Ask {
                            step,
                            id: format!("ask:{conversation}:{step}:{n}"),
                            questions: read_questions(&call["args"]),
                        });
                    }
                }
                Vec::new()
            }
            Some("GENERIC") => {
                let (answered, skipped) = answer_lines(v["content"].as_str().unwrap_or(""));
                if !answered {
                    return Vec::new();
                }
                match self.asks.pop_front() {
                    Some(ask) if skipped => self.show(ask),
                    _ => Vec::new(),
                }
            }
            _ => Vec::new(),
        }
    }

    /// agy refused a mode (said once).
    fn refusal(&mut self, line: &str) -> Vec<Event> {
        if self.refused {
            return Vec::new();
        }
        match mode_refusal(line) {
            Some((asked, running)) => {
                self.refused = true;
                vec![Event::ModeRefused { asked, running }]
            }
            None => Vec::new(),
        }
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
        // `-p` waits for its subagents: one that hasn't reported by now was cut off.
        for lane in std::mem::take(&mut self.unreported) {
            out.push(Event::SubagentEnded {
                id: lane,
                end: LaneEnd::Stopped,
            });
        }
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
            // agy's words outside its events (a refused mode may come this way too).
            return self.refusal(line);
        };
        match v["event"].as_str() {
            Some("init") => {
                let conversation = s(&v["conversation_id"]).unwrap_or_default();
                let mut out = vec![Event::Started {
                    conversation: conversation.clone(),
                    model: None,
                }];
                out.extend(self.follow_main(&conversation));
                out
            }
            Some("step_update") if v["step_update"].is_object() => {
                let mut out = self.step(&v["step_update"]);
                out.extend(self.note_step(&v["step_update"]));
                out
            }
            Some("result") if v["result"].is_object() => self.result(&v["result"]),
            _ => Vec::new(),
        }
    }

    /// A line of a subagent's transcript (M11ar): its words, its tool calls, their results, and
    /// its report to this conversation (which ends it).
    fn decode_followed(&mut self, lane: &str, line: &str) -> Vec<Event> {
        if lane == MAIN_FOLLOW {
            return self.main_line(line);
        }
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            return Vec::new();
        };
        let at = &v["step_index"];
        let mut out = Vec::new();
        match v["type"].as_str() {
            Some("PLANNER_RESPONSE") => {
                if let Some(text) = s(&v["content"]).filter(|t| !t.trim().is_empty()) {
                    out.push(Event::MessageDone {
                        message: format!("{lane}-{at}"),
                        text: text.trim_end().to_string(),
                        parent: Some(lane.to_string()),
                    });
                }
                for (n, call) in v["tool_calls"].as_array().into_iter().flatten().enumerate() {
                    let name = s(&call["name"]).unwrap_or_default();
                    let args: serde_json::Map<String, Value> = call["args"]
                        .as_object()
                        .map(|m| m.iter().map(|(k, a)| (k.clone(), arg(a))).collect())
                        .unwrap_or_default();
                    let args = Value::Object(args);
                    let id = format!("{lane}-{at}-{n}");
                    self.calls
                        .entry(lane.to_string())
                        .or_default()
                        .push(id.clone());
                    // A lane's calls carry the agent's own words (`toolSummary`).
                    let said = s(&args["toolSummary"]);
                    let (planned, call) = activity::call_started(&name, &args, said);
                    self.activities.insert(id.clone(), call);
                    out.push(Event::ToolStarted {
                        id: id.clone(),
                        summary: tool_summary(&name, &args),
                        detail: args["CommandLine"]
                            .as_str()
                            .map(|c| ToolDetail::Command(c.to_string())),
                        parent: Some(lane.to_string()),
                        writes: Vec::new(),
                        texts: Vec::new(),
                        name: name.clone(),
                    });
                    out.push(Event::Activity {
                        id: id.clone(),
                        activity: planned,
                    });
                    let to_us = self
                        .conversation
                        .as_deref()
                        .is_some_and(|c| args["Recipient"].as_str() == Some(c));
                    if name == "send_message" && to_us {
                        self.unreported.remove(lane);
                        out.push(Event::SubagentEnded {
                            id: lane.to_string(),
                            end: LaneEnd::Done,
                        });
                    }
                }
            }
            // A tool's result: it finishes the calls the step before made.
            Some("GENERIC") => {
                let raw = s(&v["content"]).unwrap_or_default();
                // A command's header says how it exited (the agent activity chore).
                let (printed, exit) = command_output(&raw);
                let ok = exit.is_none_or(|c| c == 0);
                let output: String = raw
                    .lines()
                    .filter(|l| !l.starts_with("Created At:") && !l.starts_with("Completed At:"))
                    .collect::<Vec<_>>()
                    .join("\n");
                let ids = self.calls.remove(lane).unwrap_or_default();
                // The exit header is one command's: the step's first command call's alone.
                let own = ids
                    .iter()
                    .position(|id| self.activities.get(id).is_some_and(|c| c.command()))
                    .filter(|_| exit.is_some());
                for (n, id) in ids.into_iter().enumerate() {
                    let (ok, exit) = match own {
                        Some(at) if at != n => (true, None),
                        _ => (ok, exit),
                    };
                    let done = self
                        .activities
                        .remove(&id)
                        .map(|call| activity::call_finished(call, ok, exit, &printed, None));
                    out.push(Event::ToolFinished {
                        id: id.clone(),
                        ok,
                        summary: short(output.trim(), 60),
                        output: (!output.trim().is_empty()).then(|| output.clone()),
                    });
                    out.extend(done.map(|activity| Event::Activity { id, activity }));
                }
            }
            _ => {}
        }
        out
    }

    /// agy warns on stderr (a refused mode).
    fn decode_stderr(&mut self, line: &str) -> Vec<Event> {
        self.refusal(line)
    }

    /// Each turn is one `{"event":"user"}` line (no `role`: agy rejects it). agy asks fuxx
    /// nothing (its prompts are not routed), so there is nothing to answer; and the recordings
    /// show no way to stop a turn, so `Interrupt` writes nothing (the tab's Force stop ends the
    /// process).
    fn encode(&mut self, action: &Action) -> Vec<String> {
        match action {
            // agy can't stop one subagent from outside (its lanes never offer it).
            Action::StopSubagent { .. } => Vec::new(),
            // agy says no background shells (its `run_command` goes async on its own).
            Action::StopShell { .. } => Vec::new(),
            // Its images reach it as paths in `text` (`sends_images` is false).
            Action::Send { text, .. } => {
                vec![json!({"event": "user", "message": {"content": text}}).to_string()]
            }
            // The closing bar's "Ask Main about it" (a finished lane): an ordinary turn that
            // names the subagent (agy has no history fuxx reads back, so no tag to parse).
            Action::About {
                label, text, live, ..
            } => {
                let state = if *live {
                    "which is still working"
                } else {
                    "which has finished"
                };
                let label = label.replace('"', "'").replace('\n', " ");
                let content = format!("About your subagent \"{label}\", {state}:\n\n{text}");
                vec![json!({"event": "user", "message": {"content": content}}).to_string()]
            }
            Action::Allow { .. }
            | Action::AlwaysAllow { .. }
            | Action::AllowInto { .. }
            | Action::Deny { .. }
            // Print mode answers its own `ask_question` (skipped) and has no channel for
            // yours: its cards are answered by an ordinary message (`questions_by_message`).
            | Action::Answer { .. }
            | Action::Interrupt
            | Action::SetModel { .. }
            | Action::SetEffort { .. }
            | Action::SetMode { .. }
            // Headless agy can't rewind (batch 3, chore 2: `branching` says so), nor take back
            // a message it was given (wave 3: `cancels_queued` says so).
            | Action::RewindFiles { .. }
            | Action::CancelQueued { .. }
            // Main can't pass a message on (`relays_to_subagents` is false): fuxx never asks.
            | Action::Relay { .. } => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    use std::path::PathBuf;

    /// Batch 3, chore 2: agy can't go back from fuxx (its rewind is its own terminal UI's): no
    /// branching, and Edit's tooltip points at `/rewind` there.
    #[test]
    fn agy_cannot_branch_and_points_edit_at_its_own_rewind() {
        assert!(!AGY.branching().any());
        let tip = AGY.edit_tip().expect("its own words");
        assert!(tip.contains("/rewind"), "{tip}");
        assert!(tip.contains("Classic View"), "{tip}");
    }

    // ---- M11ar: subagents ----

    const SUBS: &str = include_str!("fixtures/agy-subagents.jsonl");
    const SUB_A: &str = include_str!("fixtures/agy-subagents-transcript-a.jsonl");
    const SUB_B: &str = include_str!("fixtures/agy-subagents-transcript-b.jsonl");

    /// The parent's stream through one translator; then each lane's transcript through it.
    fn with_transcripts() -> (Box<dyn Translator>, Vec<Event>) {
        let mut t = AGY.translator();
        let mut out: Vec<Event> = SUBS.lines().flat_map(|l| t.decode(l)).collect();
        let lanes: Vec<String> = out
            .iter()
            .filter_map(|e| match e {
                Event::Follow {
                    id, main: false, ..
                } => Some(id.clone()),
                _ => None,
            })
            .collect();
        for (lane, file) in lanes.iter().zip([SUB_A, SUB_B]) {
            for line in file.lines() {
                out.extend(t.decode_followed(lane, line));
            }
        }
        (t, out)
    }

    /// agy has no prompt suggestions: no flag, no env.
    #[test]
    fn agy_has_no_suggestions() {
        assert!(AGY.suggestion_flags().is_empty());
        assert!(AGY.process_env(true).is_empty());
    }

    /// Batch 3, chore 2: headless agy has no rewind or fork (its terminal UI's `/rewind` only),
    /// takes no message ids, and a send with one writes the same line.
    #[test]
    fn agy_reports_no_branching() {
        assert_eq!(AGY.branching(), Branching::default());
        assert_eq!(AGY.fresh_message_id(), None);
        let send = |id: Option<&str>| {
            AGY.translator().encode(&Action::Send {
                text: "hi".into(),
                images: Vec::new(),
                id: id.map(String::from),
            })
        };
        assert_eq!(
            send(Some("6a1c234d-7469-40ef-b093-c74ad8006c28")),
            send(None)
        );
    }
    /// The lanes review, fix 1: a finished agy lane gets the closing bar, whose "Ask Main about
    /// it" sends your words to Main as an ordinary turn naming the subagent (agy has no relay).
    #[test]
    fn a_message_about_a_finished_subagent_goes_to_main() {
        let about = AGY.translator().encode(&Action::About {
            lane: "lane-1".into(),
            label: "Explore".into(),
            text: "What did it find?".into(),
            id: None,
            live: false,
        });
        assert_eq!(about.len(), 1, "one user line");
        let v: Value = serde_json::from_str(&about[0]).unwrap();
        assert_eq!(v["event"], "user");
        let content = v["message"]["content"].as_str().unwrap();
        assert!(content.contains("subagent \"Explore\""), "{content}");
        assert!(content.contains("finished"), "{content}");
        assert!(content.ends_with("\n\nWhat did it find?"), "{content}");
        // A relay still writes nothing: agy's Main can't pass a message on.
        let relay = AGY.translator().encode(&Action::Relay {
            lane: "lane-1".into(),
            label: "Explore".into(),
            text: "hi".into(),
            id: None,
        });
        assert!(relay.is_empty());
    }

    #[test]
    fn a_subagent_step_is_a_row_and_a_lane() {
        let ev = decode_all(SUBS);
        let started: Vec<_> = ev
            .iter()
            .filter_map(|e| match e {
                Event::SubagentStarted {
                    id,
                    label,
                    kind,
                    task,
                    controls,
                    ..
                } => Some((
                    id.clone(),
                    label.clone(),
                    kind.clone(),
                    task.clone(),
                    *controls,
                )),
                _ => None,
            })
            .collect();
        assert_eq!(started.len(), 2, "two subagents, two lanes");
        let (id, label, kind, task, controls) = &started[0];
        assert!(id.ends_with(":2#0"), "the step's id and its place: {id}");
        assert_eq!(label, "a.txt Reader", "its role");
        assert_eq!(kind, "research");
        assert!(task.contains("a.txt"));
        assert!(!controls.stop && !controls.message, "agy offers no control");
        assert!(controls.woken_by_steps, "its steps say it woke");
        // Its row: an ordinary tool row, started and finished.
        assert!(
            ev.iter()
                .any(|e| matches!(e, Event::ToolStarted { id: t, name, .. }
            if t == id && name == "Subagent"))
        );
        assert!(
            ev.iter()
                .any(|e| matches!(e, Event::ToolFinished { id: t, .. } if t == id))
        );
    }

    #[test]
    fn spawned_starts_following() {
        let ev = decode_all(SUBS);
        let follows: Vec<_> = ev
            .iter()
            .filter_map(|e| match e {
                Event::Follow {
                    id,
                    path,
                    main: false,
                } => Some((id.clone(), path.clone())),
                _ => None,
            })
            .collect();
        assert_eq!(follows.len(), 2);
        assert!(
            follows[0]
                .1
                .ends_with(".system_generated/logs/transcript.jsonl")
        );
        assert!(
            follows[0].1.to_string_lossy().contains("60e6d6ec"),
            "its own file"
        );
    }

    #[test]
    fn two_subagents_are_two_lanes() {
        let (_, ev) = with_transcripts();
        let mut c = crate::agent_conversation::Conversation::new();
        c.you("go".into(), Vec::new());
        for e in ev {
            c.apply(e);
        }
        assert_eq!(c.lanes().len(), 2);
        for lane in c.lanes() {
            assert_eq!(
                c.lane_steps(&lane.id),
                2,
                "{}: read_file, send_message",
                lane.label
            );
        }
    }

    #[test]
    fn a_transcript_line_becomes_the_lanes_steps() {
        let (_, ev) = with_transcripts();
        let lane = ev
            .iter()
            .find_map(|e| match e {
                Event::Follow {
                    id, main: false, ..
                } => Some(id.clone()),
                _ => None,
            })
            .unwrap();
        let read = ev.iter().find_map(|e| match e {
            Event::ToolStarted {
                name,
                summary,
                parent: Some(p),
                ..
            } if *p == lane && name == "read_file" => Some(summary.clone()),
            _ => None,
        });
        assert_eq!(read.as_deref(), Some("a.txt"), "its args decoded");
        assert!(
            ev.iter().any(|e| matches!(e, Event::ToolFinished { id, .. }
            if id.starts_with(&format!("{lane}-1-")))),
            "its result finishes it"
        );
        assert!(
            ev.iter().any(
                |e| matches!(e, Event::MessageDone { text, parent: Some(p), .. }
            if *p == lane && text.contains("Reported"))
            ),
            "its words"
        );
    }

    #[test]
    fn the_report_to_the_parent_ends_the_lane() {
        let (_, ev) = with_transcripts();
        let done = ev
            .iter()
            .filter(|e| {
                matches!(
                    e,
                    Event::SubagentEnded {
                        end: LaneEnd::Done,
                        ..
                    }
                )
            })
            .count();
        assert_eq!(done, 2, "each reported to the parent");
    }

    #[test]
    fn a_bad_line_is_skipped() {
        let mut t = AGY.translator();
        assert!(t.decode_followed("l", "not json").is_empty());
        assert!(
            t.decode_followed("l", "{\"type\":\"SYSTEM_MESSAGE\"}")
                .is_empty()
        );
    }

    #[test]
    fn the_turns_end_stops_unreported_lanes() {
        // The parent's stream alone (no transcript read): at its `result` both are cut off.
        let ev = decode_all(SUBS);
        let stopped = ev
            .iter()
            .filter(|e| {
                matches!(
                    e,
                    Event::SubagentEnded {
                        end: LaneEnd::Stopped,
                        ..
                    }
                )
            })
            .count();
        assert_eq!(stopped, 2);
        // With their reports read first, none is.
        let mut t = AGY.translator();
        let lines: Vec<&str> = SUBS.lines().collect();
        let (body, last) = lines.split_at(lines.len() - 1);
        let mut lanes = Vec::new();
        for l in body {
            for e in t.decode(l) {
                if let Event::Follow {
                    id, main: false, ..
                } = e
                {
                    lanes.push(id);
                }
            }
        }
        for (lane, file) in lanes.iter().zip([SUB_A, SUB_B]) {
            for line in file.lines() {
                t.decode_followed(lane, line);
            }
        }
        let end = t.decode(last[0]);
        assert!(
            !end.iter().any(|e| matches!(e, Event::SubagentEnded { .. })),
            "{end:?}"
        );
    }

    #[test]
    fn only_file_uris_are_followed() {
        assert_eq!(
            followed_path("file:///Users/me/.gemini/a%20b/transcript.jsonl"),
            Some(PathBuf::from("/Users/me/.gemini/a b/transcript.jsonl"))
        );
        assert_eq!(followed_path("https://example.com/t.jsonl"), None);
        assert_eq!(followed_path(""), None);
    }

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
            branch: None,
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
            [Event::ToolStarted { id, .. }, Event::Activity { .. }] if id == "c:6"
        ));
        let out =
            "Created At: x\nCompleted At: x\nCommand exited with code 1.\n\nCommand output:\n";
        assert!(matches!(
            t.decode(&line("DONE", Some(out))).as_slice(),
            [Event::ToolFinished { id, ok: false, .. }, Event::Activity { .. }] if id == "c:6"
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
        assert_eq!(a[7..], ["auto"]);
        assert_eq!(AGY.argv(&start(None, Some("plan"), &[]))[7], "plan");
        // A mode agy doesn't have: its default.
        assert_eq!(AGY.argv(&start(None, Some("yolo"), &[]))[7], "auto");
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
        assert_eq!(a[7..], ["auto", "--model", "gemini-3", "--effort=high"]);
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
            id: None,
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
        assert_eq!(i.default_mode, Some("auto"));
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
                    // Shown beside "N% used": agy's " Remaining" goes (M11ap).
                    name: b["name"]
                        .as_str()
                        .unwrap()
                        .trim_end_matches(" Remaining")
                        .into(),
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

    /// agy knows no folders of its own (moving a conversation is gone since the folder is fixed
    /// after the first message).
    #[test]
    fn agy_knows_no_folders() {
        assert!(AGY.known_folders(Path::new("/nonexistent")).is_empty());
    }

    /// M11ap: next to "N% used", a bucket's name loses agy's trailing " Remaining".
    #[test]
    fn bucket_names_drop_remaining() {
        let b = serde_json::json!({"name": "Weekly Limit Remaining", "remaining_fraction": 0.45});
        assert!(
            matches!(usage_bucket(&b), UsageLimit::Read { name, .. } if name == "Weekly Limit")
        );
    }

    // ---- 2026-11-24: agy's skipped questions, answered by a message ----

    const ASK: &str = include_str!("fixtures/agy-ask.jsonl");
    const ASK_TRANSCRIPT: &str = include_str!("fixtures/agy-ask-transcript.jsonl");
    const ASK_CONVERSATION: &str = "00000000-0000-0000-0000-000000000000";

    /// A translator reading agy's data under `dir`.
    fn translator_in(dir: &str) -> AgyTranslator {
        AgyTranslator {
            gemini_dir: Some(PathBuf::from(dir)),
            ..AgyTranslator::default()
        }
    }

    /// The first turn of the ask recording: its stdout, then (as the follower reads it later)
    /// its main transcript's lines up to that turn's end.
    fn asked_once(t: &mut AgyTranslator) -> Vec<Event> {
        let mut out: Vec<Event> = ASK.lines().flat_map(|l| t.decode(l)).collect();
        for line in ASK_TRANSCRIPT.lines().take(7) {
            out.extend(t.decode_followed(MAIN_FOLLOW, line));
        }
        out
    }

    fn questions_of(ev: &[Event]) -> Vec<(String, Vec<Question>)> {
        ev.iter()
            .filter_map(|e| match e {
                Event::Question { id, questions, by } => {
                    assert_eq!(*by, None, "Main asks");
                    Some((id.clone(), questions.clone()))
                }
                _ => None,
            })
            .collect()
    }

    #[test]
    fn agy_init_follows_the_main_transcript() {
        let mut t = translator_in("/Users/me/.gemini");
        let init = ASK.lines().next().unwrap();
        let ev = t.decode(init);
        assert!(matches!(&ev[0], Event::Started { conversation, .. }
            if conversation == ASK_CONVERSATION));
        assert_eq!(
            ev[1],
            Event::Follow {
                id: MAIN_FOLLOW.into(),
                path: PathBuf::from(format!(
                    "/Users/me/.gemini/antigravity-cli/brain/{ASK_CONVERSATION}/\
                     .system_generated/logs/transcript.jsonl"
                )),
                main: true,
            }
        );
        // Once per conversation: the same id again (a restart's init) follows nothing new.
        assert_eq!(t.decode(init).len(), 1);
        // A new id (`/clear`): its own transcript.
        let other = init.replace(ASK_CONVERSATION, "6a1c234d-7469-40ef-b093-c74ad8006c28");
        assert!(t.decode(&other).iter().any(|e| matches!(e,
            Event::Follow { path, main: true, .. }
                if path.to_string_lossy().contains("6a1c234d"))));
        // An id that isn't a UUID is never made into a path.
        let mut t = translator_in("/Users/me/.gemini");
        let odd = init.replace(ASK_CONVERSATION, "../../etc");
        assert_eq!(t.decode(&odd).len(), 1);
        // Where agy keeps its data: `$ANTIGRAVITY_GEMINI_DIR`, else `~/.gemini`.
        assert_eq!(
            gemini_dir(Some(OsStr::new("/tmp/g")), Some(OsStr::new("/Users/me"))),
            Some(PathBuf::from("/tmp/g"))
        );
        assert_eq!(
            gemini_dir(Some(OsStr::new("")), Some(OsStr::new("/Users/me"))),
            Some(PathBuf::from("/Users/me/.gemini"))
        );
        assert_eq!(gemini_dir(None, None), None);
        assert_eq!(
            transcript_path(Path::new("/tmp/g"), "c"),
            PathBuf::from("/tmp/g/antigravity-cli/brain/c/.system_generated/logs/transcript.jsonl")
        );
        // Tests never point at your own data.
        let mut t = AGY.translator();
        assert!(t.decode(init).iter().any(|e| matches!(e,
            Event::Follow { path, .. } if path.starts_with("/Users/me/.gemini"))));
    }

    #[test]
    fn agy_skipped_ask_question_becomes_a_question_card() {
        let mut t = translator_in("/Users/me/.gemini");
        let asked = questions_of(&asked_once(&mut t));
        assert_eq!(asked.len(), 1, "{asked:?}");
        let (id, questions) = &asked[0];
        assert_eq!(id, &format!("ask:{ASK_CONVERSATION}:3:0"));
        assert_eq!(
            questions,
            &[Question {
                header: String::new(),
                text: "Which test runner should code/my-app use?".into(),
                options: [
                    "(Recommended) Vitest",
                    "Jest",
                    "Node.js built-in test runner (node:test)"
                ]
                .map(|l| QOption {
                    label: l.into(),
                    description: String::new(),
                    preview: None,
                })
                .to_vec(),
                multi: false,
            }]
        );
        // Read again (a follower started over): never twice.
        for line in ASK_TRANSCRIPT.lines().take(7) {
            assert!(questions_of(&t.decode_followed(MAIN_FOLLOW, line)).is_empty());
        }
        // agy answers nothing for it: the answer is a message.
        assert!(AGY.questions_by_message());
        assert!(
            AGY.question_note()
                .is_some_and(|n| n.contains("new message"))
        );
        let answer = Action::Answer {
            id: id.clone(),
            answers: Vec::new(),
        };
        assert!(t.encode(&answer).is_empty());
    }

    #[test]
    fn agy_main_transcript_adds_no_rows() {
        let mut t = translator_in("/Users/me/.gemini");
        ASK.lines().for_each(|l| {
            t.decode(l);
        });
        for line in ASK_TRANSCRIPT.lines() {
            for e in t.decode_followed(MAIN_FOLLOW, line) {
                assert!(matches!(e, Event::Question { .. }), "{e:?}");
            }
        }
    }

    #[test]
    fn agy_resumed_run_ignores_old_questions() {
        // A resumed run (`--conversation`): its first stdout step is the 7th; the transcript's
        // older ask was the last run's.
        let mut t = translator_in("/Users/me/.gemini");
        t.decode(ASK.lines().next().unwrap());
        // The follower may read the old lines before the run's first step comes.
        let mut ev = Vec::new();
        for line in ASK_TRANSCRIPT.lines().take(7) {
            ev.extend(t.decode_followed(MAIN_FOLLOW, line));
        }
        let step = json!({"event": "step_update", "step_update": {"conversation_id":
            ASK_CONVERSATION, "step_index": 7, "state": "DONE", "step_type": "user_input"}});
        ev.extend(t.decode(&step.to_string()));
        for line in ASK_TRANSCRIPT.lines().skip(7) {
            ev.extend(t.decode_followed(MAIN_FOLLOW, line));
        }
        assert!(questions_of(&ev).is_empty(), "{ev:?}");
        // Held the other way round, this run's own ask still shows once its step is known.
        let mut t = translator_in("/Users/me/.gemini");
        t.decode(ASK.lines().next().unwrap());
        let mut ev = Vec::new();
        for line in ASK_TRANSCRIPT.lines().take(7) {
            ev.extend(t.decode_followed(MAIN_FOLLOW, line));
        }
        assert!(
            questions_of(&ev).is_empty(),
            "held until the run's first step"
        );
        let first = ASK.lines().nth(1).unwrap();
        assert_eq!(questions_of(&t.decode(first)).len(), 1);
    }

    #[test]
    fn agy_answered_ask_shows_no_card() {
        let mut t = translator_in("/Users/me/.gemini");
        ASK.lines().for_each(|l| {
            t.decode(l);
        });
        let lines: Vec<String> = ASK_TRANSCRIPT
            .lines()
            .take(7)
            .map(|l| l.replace("A1: User Skipped", "A1: Vitest"))
            .collect();
        for line in &lines {
            assert!(questions_of(&t.decode_followed(MAIN_FOLLOW, line)).is_empty());
        }
        // A tool result without answer lines settles no ask.
        let (any, skipped) = answer_lines("Created At: x\nCommand exited with code 0.");
        assert!(!any && !skipped);
        assert_eq!(answer_lines("A1: Vitest\nA2: User Skipped"), (true, true));
        assert_eq!(answer_lines("Q1: x\nA: y\nAx: z"), (false, false));
    }

    #[test]
    fn agy_multi_select_and_option_objects() {
        let questions = json!([
            {"is_multi_select": true, "question": "Which checks?",
             "options": [{"label": "Lint", "description": "clippy"}, "Format", 7, {"x": 1}]},
            {"question": "  ", "options": ["a"]},
        ])
        .to_string();
        let mut t = translator_in("/Users/me/.gemini");
        t.decode(ASK.lines().next().unwrap());
        t.decode(ASK.lines().nth(1).unwrap());
        let call = json!({"step_index": 3, "type": "PLANNER_RESPONSE", "tool_calls": [
            {"name": "run_command", "args": {}},
            {"name": "ask_question", "args": {"questions": questions}}]});
        let result = json!({"step_index": 4, "type": "GENERIC",
            "content": "Created At: x\nA1: User Skipped"});
        assert!(t.decode_followed(MAIN_FOLLOW, &call.to_string()).is_empty());
        let asked = questions_of(&t.decode_followed(MAIN_FOLLOW, &result.to_string()));
        assert_eq!(asked.len(), 1);
        let (id, q) = &asked[0];
        assert!(id.ends_with(":3:1"), "the call's place in its step: {id}");
        assert_eq!(q.len(), 1, "the empty question is left out");
        assert!(q[0].multi);
        assert_eq!(
            q[0].options
                .iter()
                .map(|o| (o.label.as_str(), o.description.as_str()))
                .collect::<Vec<_>>(),
            [("Lint", "clippy"), ("Format", "")]
        );
    }

    #[test]
    fn agy_answers_questions_by_message() {
        // The card's message, sent as an ordinary turn: the line agy took in the recording.
        let mut t = translator_in("/Users/me/.gemini");
        let (_, questions) = questions_of(&asked_once(&mut t)).remove(0);
        let text = crate::agent_ask::answer_message(
            &questions,
            &[QAnswer {
                labels: vec!["(Recommended) Vitest".into()],
                other: None,
            }],
        )
        .expect("an answer");
        let said = serde_json::from_str::<Value>(ASK_TRANSCRIPT.lines().nth(7).unwrap()).unwrap();
        let said = said["content"].as_str().unwrap();
        assert!(said.contains(&text), "{text:?} in {said:?}");
        let sent = t.encode(&Action::Send {
            text: text.clone(),
            images: Vec::new(),
            id: None,
        });
        assert_eq!(
            serde_json::from_str::<Value>(&sent[0]).unwrap(),
            json!({"event": "user", "message": {"content": text}})
        );
    }

    #[test]
    fn agy_refused_mode_is_its_real_mode() {
        let line = "warning: --mode auto is not available for this account; continuing in the \
                    default mode";
        assert_eq!(
            mode_refusal(line),
            Some(("auto".into(), "accept-edits".into()))
        );
        assert_eq!(
            mode_refusal("--mode=auto is not available, continuing in plan mode"),
            Some(("auto".into(), "plan".into()))
        );
        assert_eq!(mode_refusal("--mode yolo is not available"), None);
        assert_eq!(mode_refusal("warning: conversation \"x\" not found"), None);
        // On stderr, or as a line of its own on stdout; said once.
        let mut t = AGY.translator();
        assert_eq!(
            t.decode_stderr(line),
            [Event::ModeRefused {
                asked: "auto".into(),
                running: "accept-edits".into()
            }]
        );
        assert!(t.decode_stderr(line).is_empty());
        assert!(t.decode(line).is_empty());
        let mut t = AGY.translator();
        assert_eq!(t.decode(line).len(), 1);
        assert!(t.decode_stderr("anything else").is_empty());
    }
}
