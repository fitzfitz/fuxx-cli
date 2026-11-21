//! Claude Code's `--output-format stream-json` / `--input-format stream-json` wire format
//! (recorded fixtures: `fixtures/claude-*.jsonl`, Claude Code 2.1.323) to fuxx events and back.

use super::*;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub struct Claude;
pub static CLAUDE: Claude = Claude;

static INFO: ProviderInfo = ProviderInfo {
    name: "claude",
    label: "Claude",
    badge: "CC",
    who: ("\u{2733}", "Claude"),
    badge_rgb: 0x7a4a2a,
    logo: Some(crate::logos::Logo::Claude),
    interrupts: true,
    modes: &[
        ModeChoice {
            id: "default",
            label: "Ask before edits",
            about: "Asks before it edits files or runs commands",
        },
        ModeChoice {
            id: "acceptEdits",
            label: "Accept edits",
            about: "Edits files without asking; asks before running commands",
        },
        ModeChoice {
            id: "plan",
            label: "Plan",
            about: "Plans first and asks for approval before changing anything",
        },
        ModeChoice {
            id: "auto",
            label: "Auto",
            about: "Decides for itself which actions need asking",
        },
    ],
    default_mode: None,
};

/// The id the start-up `initialize` request carries, and its answer's `request_id`.
const INIT_ID: &str = "fuxx-init";

/// The `initialize` request line.
fn initialize_line() -> String {
    json!({"type": "control_request", "request_id": INIT_ID,
        "request": {"subtype": "initialize", "hooks": null}})
    .to_string()
}

/// Whether `line` is the answer to `initialize`.
fn is_initialize_answer(line: &str) -> bool {
    serde_json::from_str::<Value>(line).is_ok_and(|v| is_init_answer(&v))
}

fn is_init_answer(v: &Value) -> bool {
    v["type"] == "control_response"
        && v["response"]["request_id"] == INIT_ID
        && v["response"]["subtype"] == "success"
}

/// The aliases `--model` takes, as a menu of last resort: the order the design lists them.
const FALLBACK_ALIASES: &[(&str, &str)] = &[
    ("opus", "Opus"),
    ("sonnet", "Sonnet"),
    ("haiku", "Haiku"),
    ("fable", "Fable"),
];

/// The effort levels Claude Code 2.1.326 lists for its current model aliases (`initialize`).
const FALLBACK_EFFORTS: &[&str] = &["low", "medium", "high", "xhigh", "max"];

/// The catalog an `initialize` answer's `response` holds. `models` missing or empty: the
/// aliases stand in (`fallback_models`). `commands` come first, then the `agents`.
fn catalog_of(response: &Value) -> Catalog {
    let mut models: Vec<ModelChoice> = response["models"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| {
            let id = s(&m["value"])?;
            Some(ModelChoice {
                label: s(&m["displayName"]).unwrap_or_else(|| id.clone()),
                resolves_to: s(&m["resolvedModel"]),
                efforts: m["supportedEffortLevels"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|l| {
                        Some(EffortChoice {
                            level: s(l)?,
                            model_id: None,
                        })
                    })
                    .collect(),
                id,
            })
        })
        .collect();
    let fallback_models = models.is_empty();
    if fallback_models {
        models = CLAUDE.fallback_models();
    }
    let named = |list: &Value, kind: CommandKind| -> Vec<AgentCommand> {
        list.as_array()
            .into_iter()
            .flatten()
            .filter_map(|c| {
                Some(AgentCommand {
                    name: s(&c["name"])?,
                    about: s(&c["description"]).unwrap_or_default(),
                    kind,
                    aliases: c["aliases"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(s)
                        .collect(),
                })
            })
            .collect()
    };
    let mut commands = named(&response["commands"], CommandKind::Command);
    commands.extend(named(&response["agents"], CommandKind::Agent));
    Catalog {
        models,
        fallback_models,
        commands,
        mode: s(&response["current_permission_mode"]),
    }
}

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
    ("--session-id", true),
    ("--continue", false),
    ("-c", false),
];

/// The flags that name the conversation a `claude` works on.
const CONVERSATION_FLAGS: &[&str] = &["--resume", "-r", "--session-id"];

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
        match (&start.resume, &start.new_id) {
            (Some(id), _) => a.extend(["--resume".to_string(), id.clone()]),
            // A new conversation under the id fuxx chose: init reports that same id (checked
            // with Claude Code 2.1.326; an id already used is refused).
            (None, Some(id)) => a.extend(["--session-id".to_string(), id.clone()]),
            (None, None) => {}
        }
        // What was chosen replaces the same flag in the preset; what wasn't, passes through.
        let mut owned = OWNED_FLAGS.to_vec();
        for (flag, value) in [
            ("--model", &start.model),
            ("--effort", &start.effort),
            ("--permission-mode", &start.mode),
        ] {
            if let Some(value) = value {
                owned.push((flag, true));
                a.extend([flag.to_string(), value.clone()]);
            }
        }
        a.extend(pass_through(&start.extra_args, &owned));
        a
    }

    /// The stream-json process without a conversation, asked `initialize`: it answers in about
    /// half a second and leaves no transcript (Task 0).
    fn catalog_probes(&self) -> Vec<Probe> {
        let start = Start {
            cwd: PathBuf::from("."),
            resume: None,
            new_id: None,
            mode: None,
            model: None,
            effort: None,
            extra_args: Vec::new(),
        };
        vec![Probe {
            argv: self.argv(&start),
            stdin: vec![initialize_line()],
            until: Some(is_initialize_answer),
        }]
    }

    /// The tab's own process answers `initialize` (its `hello`): `Event::Catalog`.
    fn catalog_on_start(&self) -> bool {
        true
    }

    fn read_catalog(&self, outputs: &[String]) -> Result<Catalog, String> {
        outputs
            .iter()
            .flat_map(|o| o.lines())
            .filter_map(|l| serde_json::from_str::<Value>(l).ok())
            .find(is_init_answer)
            .map(|v| catalog_of(&v["response"]["response"]))
            .ok_or_else(|| "no answer to initialize".to_string())
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

    /// `/usage` in print mode ($0, no turn); `--no-session-persistence` keeps it from leaving a
    /// transcript behind (Task 0).
    fn usage_probe(&self) -> Option<Probe> {
        Some(Probe {
            argv: [
                "claude",
                "-p",
                "/usage",
                "--output-format",
                "json",
                "--no-session-persistence",
            ]
            .map(String::from)
            .to_vec(),
            stdin: Vec::new(),
            until: None,
        })
    }

    /// The result's text lines (`Current session: 13% used · resets Nov 17 at 8:10pm (UTC)`),
    /// as one nameless group. A line that is no `name: value` (the header, "What's contributing
    /// to your usage?" and what explains it) is no limit and is dropped; a limit line it can't
    /// read stays as text.
    fn read_usage(&self, output: &str) -> Result<Vec<UsageGroup>, String> {
        let text = serde_json::from_str::<Value>(output)
            .ok()
            .filter(|v| !v["is_error"].as_bool().unwrap_or(false))
            .and_then(|v| s(&v["result"]))
            .ok_or_else(|| "Couldn't read usage".to_string())?;
        Ok(vec![UsageGroup {
            name: String::new(),
            about: None,
            limits: text
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(usage_line)
                .filter(|l| match l {
                    UsageLimit::Text(text) => text.contains(": "),
                    UsageLimit::Read { .. } => true,
                })
                .collect(),
        }])
    }

    fn fallback_models(&self) -> Vec<ModelChoice> {
        FALLBACK_ALIASES
            .iter()
            .map(|(id, label)| ModelChoice {
                id: id.to_string(),
                label: label.to_string(),
                resolves_to: None,
                // Haiku takes no effort (Claude Code lists none for it).
                efforts: if *id == "haiku" {
                    Vec::new()
                } else {
                    FALLBACK_EFFORTS
                        .iter()
                        .map(|l| EffortChoice {
                            level: l.to_string(),
                            model_id: None,
                        })
                        .collect()
                },
            })
            .collect()
    }

    /// Model, effort and mode switch live on the running process (`set_model`,
    /// `apply_flag_settings`, `set_permission_mode`: Task 0, before the first message and
    /// mid-conversation); a folder is a restart (`--resume` from the new folder).
    fn applies(&self, change: ChangeKind) -> Applies {
        match change {
            ChangeKind::Model | ChangeKind::Effort | ChangeKind::Mode => Applies::Live,
            ChangeKind::Folder => Applies::Restart,
        }
    }

    /// A session id is a UUID.
    fn is_conversation_id(&self, id: &str) -> bool {
        uuid_shaped(id)
    }

    /// Claude takes `--session-id <uuid>` for a new conversation, so fuxx picks one.
    fn fresh_conversation_id(&self) -> Option<String> {
        new_uuid()
    }

    fn classic_argv(&self, conversation: &str) -> Vec<String> {
        ["claude", "--resume", conversation]
            .map(String::from)
            .to_vec()
    }

    fn program(&self) -> &'static str {
        "claude"
    }

    /// `--resume <id>`, `-r <id>` or `--session-id <id>` (or `=<id>`), the id a session id
    /// (a UUID: `uuid_shaped`); any other word after one of those (a word of a prompt `ps`
    /// joined in, the `--resume` picker's search) names nothing, and the search goes on.
    /// An npm install's `claude` (node) sets `process.title = "claude"`, which writes over its
    /// arguments: `ps` then shows `claude` alone, and this says None.
    fn conversation_in_args(&self, args: &str) -> Option<String> {
        conversation_after(args, CONVERSATION_FLAGS, uuid_shaped)
    }

    /// `claude-opus-5-5` → `opus`: the family, the word after the `claude-` prefix.
    fn model_label(&self, model: &str) -> String {
        match model.strip_prefix("claude-") {
            Some(rest) => rest.split('-').next().unwrap_or(rest).to_string(),
            None => model.to_string(),
        }
    }

    /// `initialize`'s `default` entry ("Default (recommended)") stands for whichever model
    /// Claude picks; an alias such as `opus` names the same model by its own name.
    fn stands_for_default(&self, id: &str) -> bool {
        id == "default"
    }

    fn translator(&self) -> Box<dyn Translator> {
        Box::new(ClaudeTranslator::default())
    }

    /// Image content blocks in a user message (Task 0: `claude-image.jsonl`).
    fn sends_images(&self) -> bool {
        true
    }

    /// The API's per-image limit, 5 MB, is on the base64 it is sent as (Anthropic's vision
    /// docs), so a file of 3.75 MB is the most it takes (R8).
    fn image_limit(&self) -> ImageLimit {
        ImageLimit::Encoded(5 * 1024 * 1024)
    }

    /// "No conversation found with session ID: …" (on stderr and in the `result`'s errors).
    fn unknown_conversation(&self, said: &str) -> bool {
        said.contains("No conversation found")
    }

    /// `--resume` doesn't replay on stdout (Task 0): the turns come from Claude's transcript of
    /// the conversation. None to resume, no home, or a missing or unreadable file: none.
    fn history(&self, start: &Start) -> Vec<Past> {
        match std::env::var_os("HOME") {
            Some(home) => history_in(Path::new(&home), start),
            None => Vec::new(),
        }
    }

    /// The `projects` keys of `~/.claude.json` (only read; missing or unreadable: none).
    fn known_folders(&self, home: &Path) -> Vec<PathBuf> {
        std::fs::read_to_string(home.join(".claude.json"))
            .map(|text| claude_json_projects(&text))
            .unwrap_or_default()
    }
}

/// The folders `~/.claude.json` lists as projects (its `projects` object's keys); none from
/// anything unreadable.
pub fn claude_json_projects(text: &str) -> Vec<PathBuf> {
    serde_json::from_str::<Value>(text)
        .ok()
        .and_then(|v| {
            v["projects"]
                .as_object()
                .map(|projects| projects.keys().map(PathBuf::from).collect())
        })
        .unwrap_or_default()
}

/// The history of the conversation `start` resumes, from its transcript under `home`. Claude
/// keeps a transcript under the folder the conversation started in, also after it moved (Task
/// 0: `--resume` from another folder writes on in the first folder's entry): so when it isn't
/// under `start`'s folder, it is looked for under every folder's entry (an id is unique). An id
/// not of a session id's shape (a path, `..`) reads nothing.
fn history_in(home: &Path, start: &Start) -> Vec<Past> {
    // The id comes from the sessions file: only a session id's shape is joined into a path.
    let Some(id) = start.resume.as_ref().filter(|id| uuid_shaped(id)) else {
        return Vec::new();
    };
    let cwd = std::fs::canonicalize(&start.cwd).unwrap_or_else(|_| start.cwd.clone());
    let here = transcript_path(home, &cwd, id);
    let text = std::fs::read_to_string(&here).ok().or_else(|| {
        let name = here.file_name()?;
        std::fs::read_dir(home.join(".claude").join("projects"))
            .ok()?
            .flatten()
            .find_map(|entry| std::fs::read_to_string(entry.path().join(name)).ok())
    });
    text.map(|text| transcript_events(&text))
        .unwrap_or_default()
}

/// Where Claude keeps conversation `id` begun in folder `cwd` (its realpath: the caller resolves
/// it): `<home>/.claude/projects/<cwd with / and . turned into ->/<id>.jsonl` (M11aa Task 0).
pub fn transcript_path(home: &Path, cwd: &Path, id: &str) -> PathBuf {
    let mangled: String = cwd
        .to_string_lossy()
        .chars()
        .map(|c| if c == '/' || c == '.' { '-' } else { c })
        .collect();
    home.join(".claude")
        .join("projects")
        .join(mangled)
        .join(format!("{id}.jsonl"))
}

/// A transcript's turns as history (call 7). Its `assistant` and `user` lines are the stream's
/// own shapes (checked against `fixtures/claude-transcript.jsonl`), so a fresh translator decodes
/// them; a `user` line whose content is a string is something you said. Lines Claude adds for
/// itself (`isMeta`), a sub-agent's own thread (`isSidechain`) and every other kind of line
/// (attachments, request records, costs) are skipped, as is anything that isn't JSON.
pub fn transcript_events(text: &str) -> Vec<Past> {
    let translator = ClaudeTranslator::default();
    let mut out = Vec::new();
    for line in text.lines() {
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if v["isMeta"] == true || v["isSidechain"] == true || v["isCompactSummary"] == true {
            continue;
        }
        match v["type"].as_str() {
            Some("user") => {
                let content = &v["message"]["content"];
                let results = content
                    .as_array()
                    .is_some_and(|a| a.iter().any(|i| i["type"] == "tool_result"));
                if results {
                    out.extend(translator.user(&v).into_iter().map(Past::Event));
                } else if let Some(past) = said_text(content).as_deref().and_then(user_said) {
                    out.push(past);
                }
            }
            Some("assistant") => out.extend(translator.assistant(&v).into_iter().map(Past::Event)),
            _ => {}
        }
    }
    out
}

/// A `user` line's text: the string, or its text blocks (an image or a pasted block among them
/// left out) one per line.
fn said_text(content: &Value) -> Option<String> {
    match content {
        Value::String(t) => Some(t.clone()),
        Value::Array(items) => {
            let texts: Vec<&str> = items.iter().filter_map(|i| i["text"].as_str()).collect();
            (!texts.is_empty()).then(|| texts.join("\n"))
        }
        _ => None,
    }
}

/// The text between `<tag>` and `</tag>` in `text`.
fn inside<'a>(text: &'a str, tag: &str) -> Option<&'a str> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = text.find(&open)? + open.len();
    let end = start + text[start..].find(&close)?;
    Some(&text[start..end])
}

/// What a `user` line's text was in the conversation: you, unless it is one of Claude's own
/// wrappers — a slash command (shown as you typed it), its output (a notice), an interrupt
/// ("Stopped"), or another `<…>` wrapper (left out).
fn user_said(text: &str) -> Option<Past> {
    let t = text.trim();
    if t.starts_with("[Request interrupted by user") {
        return Some(Past::Event(Event::Notice("Stopped".into())));
    }
    if t.starts_with("<command-name>") {
        let name = inside(t, "command-name")?.trim();
        let args = inside(t, "command-args").unwrap_or("").trim();
        let typed = if args.is_empty() {
            name.to_string()
        } else {
            format!("{name} {args}")
        };
        return Some(Past::You(typed));
    }
    if t.starts_with("<local-command-stdout>") {
        let out = inside(t, "local-command-stdout")?.trim();
        return (!out.is_empty()).then(|| Past::Event(Event::Notice(out.to_string())));
    }
    let wrapper = t
        .strip_prefix('<')
        .and_then(|rest| rest.split_once('>'))
        .is_some_and(|(tag, _)| {
            !tag.is_empty() && tag.chars().all(|c| c.is_ascii_lowercase() || c == '-')
        });
    (!wrapper && !t.is_empty()).then(|| Past::You(text.to_string()))
}

#[derive(Default)]
struct ClaudeTranslator {
    /// Open permission requests by request id: the tool input and the suggestions to send back.
    pending: HashMap<String, (Value, Value)>,
    /// The model named by `system/init`, whose `modelUsage` entry sizes the context.
    model: Option<String>,
    /// The context window the last finished turn reported (for when no model is named).
    window: Option<u64>,
    /// The live changes asked and not answered yet, by request id (`fuxx-set-<n>`).
    asked: HashMap<String, Asked>,
    /// The last `fuxx-set-<n>` written.
    sets: u32,
}

/// A live change asked of Claude (Task 0's requests), and what was asked for.
enum Asked {
    /// `set_model`: answered with the resolved id.
    Model(String),
    /// `set_permission_mode`: answered with the mode.
    Mode(String),
    /// `apply_flag_settings` with `effortLevel`: answered with no body; `read_back` is the
    /// `get_settings` asked after it.
    Effort { read_back: String },
    /// That `get_settings`: `applied.effort` is the effort now in use.
    ReadEffort(String),
}

fn s(v: &Value) -> Option<String> {
    v.as_str().map(str::to_string)
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

/// One hunk replacing `old` with `new`, with the header `diff_view::parse_diff` reads (the
/// numbers are counts from 1: an edit's own place in the file isn't known).
fn diff_body(old: &str, new: &str) -> String {
    let mut out = format!(
        "@@ -1,{} +1,{} @@\n",
        old.lines().count(),
        new.lines().count()
    );
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

/// The files a tool call writes: the path of an `Edit`, `Write` or `MultiEdit` (`file_path`) or
/// a `NotebookEdit` (`notebook_path`); none for any other tool, or while the input is empty.
fn tool_writes(name: &str, input: &Value) -> Vec<String> {
    let field = match name {
        "Edit" | "Write" | "MultiEdit" => "file_path",
        "NotebookEdit" => "notebook_path",
        _ => return Vec::new(),
    };
    s(&input[field]).into_iter().collect()
}

/// The new text each edit puts in its file, for the rest of fuxx to find the edit's lines
/// (M11ag). `replace_all` changes nothing: the first match is where the note goes. A call that
/// writes a file and has no text to find a note by (a `NotebookEdit`; an `Edit` or `MultiEdit`
/// with no `new_string`) says so with `NoNote`: no texts at all would read as an agent that
/// gives none.
fn tool_texts(name: &str, input: &Value) -> Vec<EditText> {
    let one = |v: &Value| match v["new_string"].as_str() {
        Some("") => Some(EditText::Removed),
        Some(t) => Some(EditText::Inserted(t.to_string())),
        None => None,
    };
    let texts: Vec<EditText> = match name {
        "Edit" => one(input).into_iter().collect(),
        "MultiEdit" => input["edits"]
            .as_array()
            .map(|a| a.iter().filter_map(one).collect())
            .unwrap_or_default(),
        "Write" => return vec![EditText::WholeFile],
        "NotebookEdit" => Vec::new(),
        _ => return Vec::new(),
    };
    if texts.is_empty() {
        vec![EditText::NoNote]
    } else {
        texts
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

/// The "always" button's whole label for a permission suggestion: Write and Edit offer a mode
/// for the whole session (Ruling 10), Bash a rule for that command.
fn describe_suggestion(sug: &Value) -> String {
    match sug["type"].as_str() {
        Some("setMode") => match sug["mode"].as_str() {
            Some("acceptEdits") => "Allow all edits this session".to_string(),
            Some(m) => format!("Switch to {m} mode this session"),
            None => "Always allow".to_string(),
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
            if rules.is_empty() {
                "Always allow".to_string()
            } else {
                format!("Always allow {}", rules.join(", "))
            }
        }
        Some(_) | None => "Always allow".to_string(),
    }
}

/// Context windows learned from finished turns, by model, for every tab: a tab's first turn on a
/// model another tab already ran is live from its first call.
static WINDOWS: std::sync::LazyLock<std::sync::Mutex<HashMap<String, u64>>> =
    std::sync::LazyLock::new(Default::default);

/// The context window a `result` event reports: the session model's (`model`, from init), else
/// the largest one reported.
fn result_window(result: &Value, model: Option<&str>) -> Option<u64> {
    let usage = result["modelUsage"].as_object()?;
    model
        .and_then(|m| usage.get(m))
        .and_then(|m| m["contextWindow"].as_u64())
        .or_else(|| {
            usage
                .values()
                .filter_map(|m| m["contextWindow"].as_u64())
                .max()
        })
        .filter(|w| *w > 0)
}

/// The tokens a model call read: its input, cached and newly cached.
fn tokens_in(usage: &Value) -> u64 {
    [
        "input_tokens",
        "cache_read_input_tokens",
        "cache_creation_input_tokens",
    ]
    .iter()
    .map(|k| usage[*k].as_u64().unwrap_or(0))
    .sum()
}

/// `used` as a whole percent of `window`, at most 100.
fn share(used: u64, window: u64) -> u8 {
    (used * 100 / window).min(100) as u8
}

/// The share of the context window the last model call used, from a `result` event.
fn context_pct(result: &Value, model: Option<&str>) -> Option<u8> {
    let window = result_window(result, model)?;
    let last = result["usage"]["iterations"].as_array()?.last()?;
    Some(share(tokens_in(last), window))
}

/// `name: N% used · resets Nov 17 at 8:09pm (UTC)` as a limit with the percent left
/// (the `· resets …` part may be missing); any other line stays text.
fn usage_line(line: &str) -> UsageLimit {
    let read = || {
        let (name, rest) = line.split_once(": ")?;
        let (used, rest) = rest.split_once("% used")?;
        let used: u8 = used.trim().parse().ok().filter(|u| *u <= 100)?;
        let resets = match rest.trim().strip_prefix("· resets ") {
            Some(when) => Some(Reset::Words(strip_zone(when.trim()).to_string())),
            None if rest.trim().is_empty() => None,
            None => return None,
        };
        Some(UsageLimit::Read {
            name: name.to_string(),
            left_pct: 100 - used,
            resets,
        })
    };
    read().unwrap_or_else(|| UsageLimit::Text(line.to_string()))
}

/// `Nov 17 at 8:09pm (UTC)` without its trailing zone.
fn strip_zone(when: &str) -> &str {
    match when.rsplit_once(" (") {
        Some((time, zone)) if zone.ends_with(')') => time,
        _ => when,
    }
}

/// A `rate_limit_event`'s limit as `/usage` names it (any other type keeps its raw name).
fn limit_name(kind: &str) -> String {
    match kind {
        "five_hour" => "Current session",
        "seven_day" => "Current week (all models)",
        other => other,
    }
    .to_string()
}

fn rate_limit(v: &Value) -> Vec<Event> {
    let info = &v["rate_limit_info"];
    let Some(kind) = info["rateLimitType"].as_str() else {
        return Vec::new();
    };
    let Some(mut first) = window_limit(kind, info) else {
        return Vec::new();
    };
    // An ordinary ("allowed") line says its own window's use only in `unifiedWindows` (the
    // review's finding 1): take it, and its reset, from there.
    if let Some(own) = info["unifiedWindows"]
        .get(kind)
        .and_then(|w| window_limit(kind, w))
    {
        first.used_pct = first.used_pct.or(own.used_pct);
        first.resets_at = first.resets_at.or(own.resets_at);
    }
    // M11ap: the other windows the line carries (`unifiedWindows`), each with its own status,
    // use and reset: Claude's usage, live while it works.
    let others = info["unifiedWindows"]
        .as_object()
        .into_iter()
        .flatten()
        .filter(|(name, _)| name.as_str() != kind)
        .filter_map(|(name, w)| window_limit(name, w));
    std::iter::once(first)
        .chain(others)
        .map(Event::RateLimit)
        .collect()
}

/// One rate-limit window (`five_hour`, `seven_day`, …): its status, reset and use.
fn window_limit(kind: &str, w: &Value) -> Option<RateLimit> {
    let state = match w["status"].as_str() {
        Some("allowed") => LimitState::Fine,
        Some("allowed_warning") => LimitState::Close,
        Some("rejected") => LimitState::Reached,
        _ => return None,
    };
    Some(RateLimit {
        limit: limit_name(kind),
        state,
        resets_at: w["resetsAt"].as_i64(),
        used_pct: w["utilization"]
            .as_f64()
            .map(|u| (u * 100.0).round().clamp(0.0, 100.0) as u8),
    })
}

/// A message's `content`: the text as it is, or with images a block each, then the text block
/// (none when there is no text: the API refuses an empty one).
fn content(text: &str, images: &[Picture]) -> Value {
    if images.is_empty() {
        return json!(text);
    }
    let mut blocks: Vec<Value> = images
        .iter()
        .map(|p| {
            json!({"type": "image",
            "source": {"type": "base64", "media_type": p.media_type, "data": &*p.data}})
        })
        .collect();
    if !text.is_empty() {
        blocks.push(json!({"type": "text", "text": text}));
    }
    Value::Array(blocks)
}

fn control_response(request_id: &str, response: Value) -> String {
    json!({"type": "control_response",
        "response": {"subtype": "success", "request_id": request_id, "response": response}})
    .to_string()
}

impl ClaudeTranslator {
    /// The next request id for a live change.
    fn set_id(&mut self) -> String {
        self.sets += 1;
        format!("fuxx-set-{}", self.sets)
    }

    /// A live change's request under `id`, kept as `asked` until its answer.
    fn ask(&mut self, id: String, request: Value, asked: Asked) -> String {
        let line =
            json!({"type": "control_request", "request_id": id, "request": request}).to_string();
        self.asked.insert(id, asked);
        line
    }

    /// The answer to a live change fuxx asked (any other control response is not one).
    fn answered(&mut self, v: &Value) -> Vec<Event> {
        let response = &v["response"];
        let Some(asked) = response["request_id"]
            .as_str()
            .and_then(|id| self.asked.remove(id))
        else {
            return Vec::new();
        };
        let kind = match &asked {
            Asked::Model(_) => ChangeKind::Model,
            Asked::Mode(_) => ChangeKind::Mode,
            Asked::Effort { .. } | Asked::ReadEffort(_) => ChangeKind::Effort,
        };
        if response["subtype"] != "success" {
            if let Asked::Effort { read_back } = &asked {
                // Its read-back would only say the effort that stayed.
                self.asked.remove(read_back);
            }
            let why = s(&response["error"]).unwrap_or_else(|| "Claude refused it".into());
            return vec![Event::Applied {
                kind,
                value: why,
                ok: false,
            }];
        }
        let body = &response["response"];
        let value = match asked {
            Asked::Model(asked) => {
                let model = s(&body["model"]).unwrap_or(asked);
                // The context is measured against the model it now runs.
                self.model = Some(model.clone());
                model
            }
            Asked::Mode(asked) => s(&body["mode"]).unwrap_or(asked),
            Asked::Effort { .. } => return Vec::new(),
            Asked::ReadEffort(asked) => s(&body["applied"]["effort"]).unwrap_or(asked),
        };
        vec![Event::Applied {
            kind,
            value,
            ok: true,
        }]
    }

    /// The session model's context window, as a finished turn (in any tab) reported it; with no
    /// model named, the last turn's.
    fn known_window(&self) -> Option<u64> {
        match &self.model {
            Some(m) => WINDOWS.lock().ok()?.get(m).copied(),
            None => self.window,
        }
    }

    fn assistant(&self, v: &Value) -> Vec<Event> {
        let message = s(&v["message"]["id"]).unwrap_or_default();
        let parent = s(&v["parent_tool_use_id"]);
        let mut out = Vec::new();
        // Each main-thread call says how much context it read: the meter moves during the turn.
        // (A sub-agent's calls run in their own context.)
        if parent.is_none()
            && let Some(window) = self.known_window()
            && v["message"]["usage"].is_object()
        {
            out.push(Event::Usage {
                context_pct: Some(share(tokens_in(&v["message"]["usage"]), window)),
                cost_usd: None,
            });
        }
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
                            writes: tool_writes(&name, input),
                            texts: tool_texts(&name, input),
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
                let mut out = vec![Event::Started {
                    conversation: s(&v["session_id"]).unwrap_or_default(),
                    model: s(&v["model"]),
                }];
                if let Some(skills) = v["skills"].as_array() {
                    out.push(Event::Skills(skills.iter().filter_map(s).collect()));
                }
                out.extend(s(&v["permissionMode"]).map(Event::Mode));
                out
            }
            // The mode it runs in, also one it changed to itself (R9); a status without one
            // (`requesting`, …) says nothing.
            Some("status") => s(&v["permissionMode"])
                .map(Event::Mode)
                .into_iter()
                .collect(),
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

    fn result(&mut self, v: &Value) -> Vec<Event> {
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
        if let Some(window) = result_window(v, self.model.as_deref()) {
            self.window = Some(window);
            if let (Some(model), Ok(mut known)) = (&self.model, WINDOWS.lock()) {
                known.insert(model.clone(), window);
            }
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
        vec![initialize_line()]
    }

    fn decode(&mut self, line: &str) -> Vec<Event> {
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            return Vec::new();
        };
        match v["type"].as_str() {
            Some("system") => self.system(&v),
            Some("control_response") if is_init_answer(&v) => {
                vec![Event::Catalog(catalog_of(&v["response"]["response"]))]
            }
            Some("control_response") => self.answered(&v),
            Some("stream_event") => self.stream_event(&v),
            Some("assistant") => self.assistant(&v),
            Some("user") => self.user(&v),
            Some("result") => self.result(&v),
            Some("rate_limit_event") => rate_limit(&v),
            // `/clear`: then a new `system/init` under a new session id (Claude Code 2.1.330).
            Some("conversation_reset") => vec![Event::Cleared],
            Some("control_request") if v["request"]["subtype"] == "can_use_tool" => {
                self.permission(&v)
            }
            _ => Vec::new(),
        }
    }

    fn encode(&mut self, action: &Action) -> Vec<String> {
        match action {
            Action::Send { text, images } => vec![
                json!({"type": "user",
                "message": {"role": "user", "content": content(text, images)}})
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
            Action::SetModel { model } => {
                let id = self.set_id();
                let request = json!({"subtype": "set_model", "model": model});
                vec![self.ask(id, request, Asked::Model(model.clone()))]
            }
            Action::SetMode { mode } => {
                let id = self.set_id();
                let request = json!({"subtype": "set_permission_mode", "mode": mode});
                vec![self.ask(id, request, Asked::Mode(mode.clone()))]
            }
            // The SDK's `applyFlagSettings`; its answer has no body, so `get_settings`, asked
            // right after it, reads the effort back (Task 0).
            Action::SetEffort { effort } => {
                let (apply_id, read_id) = (self.set_id(), self.set_id());
                let apply = self.ask(
                    apply_id,
                    json!({"subtype": "apply_flag_settings", "settings": {"effortLevel": effort}}),
                    Asked::Effort {
                        read_back: read_id.clone(),
                    },
                );
                let read = self.ask(
                    read_id,
                    json!({"subtype": "get_settings"}),
                    Asked::ReadEffort(effort.clone()),
                );
                vec![apply, read]
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

    /// Spec B-A: the tab and the chat draw Claude's logo, not "CC".
    #[test]
    fn claude_and_agy_have_their_logos() {
        use crate::logos::Logo;
        assert_eq!(CLAUDE.info().logo, Some(Logo::Claude));
        assert_eq!(
            crate::providers::agy::AGY.info().logo,
            Some(Logo::Antigravity)
        );
    }

    #[test]
    fn own_commands() {
        assert_eq!(CLAUDE.own_command("/usage"), Some(BoxCommand::Usage));
        assert_eq!(CLAUDE.own_command("/model"), Some(BoxCommand::Model));
        assert_eq!(CLAUDE.own_command("/effort"), Some(BoxCommand::Effort));
        assert_eq!(CLAUDE.own_command(" /usage \n"), Some(BoxCommand::Usage));
        // Anything else (or a command with arguments) is sent as text.
        assert_eq!(CLAUDE.own_command("/clear"), None);
        assert_eq!(CLAUDE.own_command("/model sonnet"), None);
        assert_eq!(CLAUDE.own_command("usage"), None);
    }

    fn lines_of(text: &str) -> Vec<Value> {
        text.lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }

    #[test]
    fn text_fixture_streams_then_finishes() {
        let ev = decode_all(&CLAUDE, include_str!("fixtures/claude-text.jsonl"));
        // The tab's own `initialize` answer (a `Catalog`) comes first, then `Started`.
        assert!(matches!(ev.first(), Some(Event::Catalog(_))));
        assert!(ev.iter().any(|e| matches!(e, Event::Started { .. })));
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
        // The turn's last Usage carries the recorded cost and a context percentage from the
        // last call (any before it are the live, cost-free readings).
        let usage = ev.iter().rev().find_map(|e| match e {
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
            ev.iter().find(|e| matches!(e, Event::Started { .. })),
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

    #[test]
    fn tool_texts_of_edit_multiedit_write_and_notebook() {
        let ins = |t: &str| EditText::Inserted(t.to_string());
        assert_eq!(
            tool_texts("Edit", &json!({"old_string": "a", "new_string": "b"})),
            vec![ins("b")]
        );
        assert_eq!(
            tool_texts(
                "Edit",
                &json!({"old_string": "a", "new_string": "", "replace_all": true})
            ),
            vec![EditText::Removed]
        );
        assert_eq!(
            tool_texts(
                "MultiEdit",
                &json!({"edits": [
                    {"old_string": "a", "new_string": "b"},
                    {"old_string": "c", "new_string": ""},
                ]})
            ),
            vec![ins("b"), EditText::Removed]
        );
        assert_eq!(
            tool_texts("Write", &json!({"content": "x"})),
            vec![EditText::WholeFile]
        );
        // A notebook edit, and an edit with no new text given, write their file with no note:
        // said so, not left as "no texts" (an agent that gives none gets the light way).
        assert_eq!(
            tool_texts("NotebookEdit", &json!({"new_source": "x"})),
            vec![EditText::NoNote]
        );
        assert_eq!(
            tool_texts("Edit", &json!({"old_string": "a"})),
            vec![EditText::NoNote]
        );
        assert_eq!(tool_texts("Edit", &json!({})), vec![EditText::NoNote]);
        assert_eq!(
            tool_texts("MultiEdit", &json!({"edits": [{"old_string": "a"}]})),
            vec![EditText::NoNote]
        );
        assert_eq!(tool_texts("MultiEdit", &json!({})), vec![EditText::NoNote]);
        assert!(tool_texts("Bash", &json!({"command": "ls"})).is_empty());
    }

    #[test]
    fn claude_tools_fixture_edits_carry_their_new_text() {
        let tools = decode_all(&CLAUDE, include_str!("fixtures/claude-tools.jsonl"));
        let texts: Vec<Vec<EditText>> = tools
            .iter()
            .filter_map(|e| match e {
                Event::ToolStarted { name, texts, .. } if name == "Edit" => Some(texts.clone()),
                _ => None,
            })
            .collect();
        assert!(
            texts.contains(&vec![EditText::Inserted("beta".to_string())]),
            "{texts:?}"
        );
    }

    #[test]
    fn edit_tools_name_the_files_they_write() {
        let writes_of = |ev: &[Event], tool: &str| -> Vec<Vec<String>> {
            ev.iter()
                .filter_map(|e| match e {
                    Event::ToolStarted { name, writes, .. } if name == tool => Some(writes.clone()),
                    _ => None,
                })
                .collect()
        };
        let tools = decode_all(&CLAUDE, include_str!("fixtures/claude-tools.jsonl"));
        // The recording carries each Edit twice: once with the input still empty (nothing to
        // name), once whole.
        let edits = writes_of(&tools, "Edit");
        assert!(
            edits.contains(&vec!["/tmp/fx/notes.txt".to_string()]),
            "{edits:?}"
        );
        assert!(
            edits
                .iter()
                .all(|w| w.is_empty() || w == &["/tmp/fx/notes.txt"])
        );
        assert!(writes_of(&tools, "Read").iter().all(|w| w.is_empty()));
        assert!(writes_of(&tools, "Bash").iter().all(|w| w.is_empty()));
        let allow = decode_all(
            &CLAUDE,
            include_str!("fixtures/claude-permission-allow.jsonl"),
        );
        let wrote: Vec<Vec<String>> = writes_of(&allow, "Write")
            .into_iter()
            .filter(|w| !w.is_empty())
            .collect();
        assert!(!wrote.is_empty());
        assert!(
            wrote
                .iter()
                .all(|w| w.len() == 1 && w[0].ends_with("hello.txt"))
        );
        let inline = |name: &str, input: Value| {
            let line = json!({"type":"assistant","parent_tool_use_id":null,"message":{"id":"m",
                "content":[{"type":"tool_use","id":"toolu_i","name":name,"input":input}]}})
            .to_string();
            writes_of(&CLAUDE.translator().decode(&line), name)
        };
        assert_eq!(
            inline("MultiEdit", json!({"file_path":"/tmp/fx/a.rs","edits":[]})),
            [["/tmp/fx/a.rs"]]
        );
        assert_eq!(
            inline("NotebookEdit", json!({"notebook_path":"/tmp/fx/n.ipynb"})),
            [["/tmp/fx/n.ipynb"]]
        );
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
                writes: Vec::new(),
                texts: Vec::new(),
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
        assert_eq!(always.as_deref(), Some("Allow all edits this session"));
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
            Some("Always allow Bash(python3 -c 'import time; time.sleep(60)')")
        );
    }

    #[test]
    fn history_reads_the_transcript() {
        let past = transcript_events(include_str!("fixtures/claude-transcript.jsonl"));
        assert_eq!(
            past,
            vec![
                Past::You("Say hello in one word".into()),
                Past::Event(Event::MessageDone {
                    message: "msg_011Cgz3BeeVjP3aWM4pjPKbD".into(),
                    text: "Hello!".into(),
                    parent: None,
                }),
                Past::You(
                    "What single word did I ask you to say earlier? Answer in one word.".into()
                ),
                Past::Event(Event::MessageDone {
                    message: "msg_011Cgz3JJVyRZykwmNZqqJjR".into(),
                    text: "Hello".into(),
                    parent: None,
                }),
            ]
        );
    }

    /// A transcript holds tool calls in the stream's shapes, and lines that are not turns.
    #[test]
    fn history_keeps_tools_and_skips_meta_and_sidechain_lines() {
        let lines = [
            json!({"type":"user","message":{"role":"user","content":"fix it"}}),
            json!({"type":"user","isMeta":true,"message":{"role":"user","content":"<caveat>"}}),
            json!({"type":"assistant","message":{"id":"m1","content":[
                {"type":"tool_use","id":"t1","name":"Bash","input":{"command":"ls"}}]}}),
            json!({"type":"user","message":{"role":"user","content":[
                {"type":"tool_result","tool_use_id":"t1","content":"a.txt","is_error":false}]}}),
            json!({"type":"assistant","isSidechain":true,"message":{"id":"m2","content":[
                {"type":"text","text":"sub-agent"}]}}),
            json!({"type":"attachment","attachment":{}}),
        ];
        let text: String = lines.iter().map(|l| l.to_string() + "\n").collect();
        let past = transcript_events(&format!("{text}not json\n"));
        assert_eq!(past.len(), 3, "{past:?}");
        assert_eq!(past[0], Past::You("fix it".into()));
        assert!(matches!(&past[1], Past::Event(Event::ToolStarted { id, .. }) if id == "t1"));
        assert!(
            matches!(&past[2], Past::Event(Event::ToolFinished { id, ok: true, .. }) if id == "t1")
        );
    }

    /// Review fix (Minor 5): a `user` line of text blocks is what you said; Claude's own command
    /// wrappers are not you: a slash command shows as you typed it, its output as a notice, an
    /// interrupt as "Stopped", the rest of `<…>` wrappers not at all.
    #[test]
    fn history_reads_text_blocks_and_command_wrappers() {
        let user =
            |content: Value| json!({"type":"user","message":{"role":"user","content":content}});
        let lines = [
            user(
                json!([{"type":"text","text":"look at"},{"type":"image","source":{}},{"type":"text","text":"this"}]),
            ),
            user(json!(
                "<command-name>/compact</command-name>\n<command-message>compact</command-message>\n<command-args></command-args>"
            )),
            user(json!(
                "<local-command-stdout>Compacted (ctrl+o to see full summary)</local-command-stdout>"
            )),
            user(json!(
                "<local-command-caveat>Caveat: the messages below were generated by the user</local-command-caveat>"
            )),
            user(json!([{"type":"text","text":"[Request interrupted by user]"}])),
            json!({"type":"user","isCompactSummary":true,"message":{"role":"user","content":"This session is being continued"}}),
        ];
        let text: String = lines.iter().map(|l| l.to_string() + "\n").collect();
        assert_eq!(
            transcript_events(&text),
            vec![
                Past::You("look at\nthis".into()),
                Past::You("/compact".into()),
                Past::Event(Event::Notice(
                    "Compacted (ctrl+o to see full summary)".into()
                )),
                Past::Event(Event::Notice("Stopped".into())),
            ]
        );
    }

    /// Review fix (Minor 1): only "no such conversation" makes a failed resume a new one.
    #[test]
    fn an_unknown_conversation_is_told_by_what_claude_said() {
        assert!(
            CLAUDE.unknown_conversation(include_str!("fixtures/claude-bad-resume.jsonl.stderr"))
        );
        assert!(!CLAUDE.unknown_conversation("Invalid API key · Please run /login"));
        assert!(!CLAUDE.unknown_conversation(""));
    }

    #[test]
    fn transcript_path_mangles_the_cwd() {
        assert_eq!(
            transcript_path(
                Path::new("/Users/me"),
                Path::new("/Users/me/k.p/app"),
                "id1"
            ),
            PathBuf::from("/Users/me/.claude/projects/-Users-me-k-p-app/id1.jsonl")
        );
    }

    #[test]
    fn a_missing_transcript_is_no_history() {
        let start = Start {
            cwd: PathBuf::from("/nonexistent/fuxx-test"),
            resume: Some("no-such-id".into()),
            new_id: None,
            mode: None,
            extra_args: Vec::new(),
            model: None,
            effort: None,
        };
        // A made-up home (never the real one: a miss looks under every folder's entry).
        let home = scratch_home("missing");
        assert!(history_in(&home, &start).is_empty());
        let fresh = Start {
            resume: None,
            ..start
        };
        assert!(history_in(&home, &fresh).is_empty());
        let _ = std::fs::remove_dir_all(&home);
    }

    /// An edit's diff is one hunk `diff_view::parse_diff` reads (it needs `@@ -a +c @@`).
    #[test]
    fn an_edit_diff_parses_as_a_hunk() {
        let ev = decode_all(&CLAUDE, include_str!("fixtures/claude-tools.jsonl"));
        let unified = ev
            .iter()
            .find_map(|e| match e {
                Event::ToolStarted {
                    detail: Some(ToolDetail::Diff { unified, .. }),
                    ..
                } => Some(unified.clone()),
                _ => None,
            })
            .expect("an Edit");
        let parsed = crate::diff_view::parse_diff(&unified);
        use crate::diff_view::RowKind;
        let kinds: Vec<RowKind> = parsed.rows.iter().map(|r| r.kind).collect();
        assert_eq!(kinds, [RowKind::Removed, RowKind::Added], "{unified}");
        assert_eq!(parsed.text, "alpha\nbeta");
    }

    #[test]
    fn send_and_interrupt_encode() {
        let mut t = CLAUDE.translator();
        let out = t.encode(&Action::Send {
            text: "hi".into(),
            images: Vec::new(),
        });
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
    fn base64_vectors() {
        // RFC 4648's test vectors, and the empty input.
        for (plain, coded) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(base64(plain.as_bytes()), coded, "{plain:?}");
        }
        assert_eq!(base64(&[0xfb, 0xff, 0xfe]), "+//+", "the + and / digits");
    }

    #[test]
    fn claudes_image_limit_is_5_mb_after_base64() {
        // Anthropic's vision docs: the per-image 5 MB limit is on the base64-encoded image.
        assert_eq!(
            CLAUDE.image_limit(),
            crate::providers::ImageLimit::Encoded(5 * 1024 * 1024)
        );
    }

    #[test]
    fn a_send_with_an_image_is_a_content_array() {
        assert!(CLAUDE.sends_images());
        let mut t = CLAUDE.translator();
        let picture = Picture::new("image/png", b"foo".to_vec());
        let out = t.encode(&Action::Send {
            text: "what colour".into(),
            images: vec![picture.clone(), picture],
        });
        let v: Value = serde_json::from_str(&out[0]).unwrap();
        let image = json!({"type":"image","source":{"type":"base64","media_type":"image/png","data":"Zm9v"}});
        assert_eq!(
            v,
            json!({"type":"user","message":{"role":"user","content":[
                image.clone(), image, {"type":"text","text":"what colour"}]}})
        );
        // An image alone has no (empty) text block: the API refuses one.
        let out = t.encode(&Action::Send {
            text: String::new(),
            images: vec![Picture::new("image/png", b"foo".to_vec())],
        });
        let v: Value = serde_json::from_str(&out[0]).unwrap();
        assert_eq!(v["message"]["content"].as_array().map(Vec::len), Some(1));
    }

    #[test]
    fn encode_uses_the_pictures_data_as_it_is() {
        // The base64 is made once, off the UI thread, when the picture is built; `encode`
        // (on the UI thread) only copies it.
        let picture = Picture {
            media_type: "image/png",
            bytes: Arc::from(&b"foo"[..]),
            data: Arc::from("VERBATIM"),
        };
        let out = CLAUDE.translator().encode(&Action::Send {
            text: String::new(),
            images: vec![picture],
        });
        let v: Value = serde_json::from_str(&out[0]).unwrap();
        assert_eq!(v["message"]["content"][0]["source"]["data"], "VERBATIM");
        assert_eq!(
            Picture::new("image/png", b"foo".to_vec()).data.as_ref(),
            "Zm9v"
        );
    }

    #[test]
    fn a_send_without_images_is_as_before() {
        let mut t = CLAUDE.translator();
        let out = t.encode(&Action::Send {
            text: "hi".into(),
            images: Vec::new(),
        });
        let v: Value = serde_json::from_str(&out[0]).unwrap();
        assert_eq!(v["message"]["content"], "hi", "a plain string, not blocks");
        assert!(
            !crate::providers::agy::AGY.sends_images(),
            "agy gets a path"
        );
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

    /// Recorded with Claude Code 2.1.330 (`/clear` in stream-json): a `conversation_reset` line,
    /// then hooks and a new `system/init` under a new session id. The reset empties the tab.
    #[test]
    fn a_conversation_reset_clears_the_tab() {
        let line = json!({"type":"conversation_reset","session_id":"s1"}).to_string();
        assert_eq!(CLAUDE.translator().decode(&line), vec![Event::Cleared]);
    }

    /// The context share an `assistant` line reports (`None` when it reports none).
    fn live_pct(t: &mut Box<dyn Translator>, line: &str) -> Option<u8> {
        t.decode(line).into_iter().find_map(|e| match e {
            Event::Usage {
                context_pct,
                cost_usd: None,
            } => context_pct,
            _ => None,
        })
    }

    /// A main-thread `assistant` line for `model` with these input-side token counts.
    fn assistant_usage(
        model: &str,
        parent: Option<&str>,
        input: u64,
        read: u64,
        made: u64,
    ) -> String {
        json!({"type":"assistant","parent_tool_use_id":parent,"message":{"id":"m","model":model,
            "content":[{"type":"text","text":"hi"}],
            "usage":{"input_tokens":input,"cache_read_input_tokens":read,
                "cache_creation_input_tokens":made,"output_tokens":5}}})
        .to_string()
    }

    /// A finished turn that sizes `model`'s context window.
    fn sized(model: &str, window: u64) -> String {
        json!({"type":"result","subtype":"success","is_error":false,"result":"ok",
            "modelUsage":{model:{"contextWindow":window}},
            "usage":{"iterations":[{"input_tokens":1}]}})
        .to_string()
    }

    /// Synthetic: the context moves with each model call, not only when the turn ends.
    #[test]
    fn context_is_live_once_the_window_is_known() {
        let mut t = CLAUDE.translator();
        let init = json!({"type":"system","subtype":"init","session_id":"s","model":"live-a"});
        t.decode(&init.to_string());
        // Before any turn has sized the window: nothing to measure against.
        assert_eq!(
            live_pct(&mut t, &assistant_usage("live-a", None, 2, 30_000, 10_000)),
            None
        );
        t.decode(&sized("live-a", 200_000));
        // 2 + 30k + 10k of 200k: 20 %, with no cost (that comes with the turn's end).
        assert_eq!(
            live_pct(&mut t, &assistant_usage("live-a", None, 2, 30_000, 10_000)),
            Some(20)
        );
        assert_eq!(
            live_pct(&mut t, &assistant_usage("live-a", None, 2, 60_000, 0)),
            Some(30)
        );
    }

    /// Synthetic: a sub-agent's calls run in their own context; they don't move the meter.
    #[test]
    fn subagent_calls_leave_the_context_alone() {
        let mut t = CLAUDE.translator();
        let init = json!({"type":"system","subtype":"init","session_id":"s","model":"live-b"});
        t.decode(&init.to_string());
        t.decode(&sized("live-b", 100_000));
        let line = assistant_usage("live-b", Some("toolu_p"), 2, 50_000, 0);
        assert_eq!(live_pct(&mut t, &line), None);
    }

    /// Synthetic: a model's window, once learned, holds for every tab running it.
    #[test]
    fn another_tab_on_the_same_model_is_live_from_its_first_call() {
        let mut first = CLAUDE.translator();
        let init = json!({"type":"system","subtype":"init","session_id":"s","model":"live-c"});
        first.decode(&init.to_string());
        first.decode(&sized("live-c", 1_000_000));
        let mut second = CLAUDE.translator();
        second.decode(&init.to_string());
        assert_eq!(
            live_pct(&mut second, &assistant_usage("live-c", None, 0, 250_000, 0)),
            Some(25)
        );
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
            new_id: None,
            mode: None,
            extra_args: vec![
                "--model".into(),
                "sonnet".into(),
                "-p".into(),
                "--resume".into(),
                "x".into(),
            ],
            model: None,
            effort: None,
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
            new_id: None,
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
            model: None,
            effort: None,
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
    fn a_process_names_its_conversation_only_by_a_session_id() {
        const ID: &str = "0b3c4a5e-1f2d-4e6a-9b8c-7d6e5f4a3b2c";
        let named = |args: &str| CLAUDE.conversation_in_args(args);
        assert_eq!(named(&format!("claude --resume {ID}")).as_deref(), Some(ID));
        assert_eq!(named(&format!("claude -r {ID}")).as_deref(), Some(ID));
        assert_eq!(named(&format!("claude --resume={ID}")).as_deref(), Some(ID));
        assert_eq!(
            named(&format!(
                "claude -p --input-format stream-json --session-id {ID}"
            ))
            .as_deref(),
            Some(ID)
        );
        // A prompt's words, the picker's search, a bare picker, npm's title alone: none.
        assert_eq!(named("claude -c fix the grep -r foo call"), None);
        assert_eq!(named("claude --resume searchterm"), None);
        assert_eq!(named("claude --resume=searchterm"), None);
        assert_eq!(named("claude --resume"), None);
        assert_eq!(named("claude"), None);
        // A prompt word first, the id after it.
        assert_eq!(
            named(&format!("claude -r foo --resume {ID}")).as_deref(),
            Some(ID)
        );
        assert_eq!(CLAUDE.program(), "claude");
    }

    #[test]
    fn the_model_label_is_its_family() {
        assert_eq!(CLAUDE.model_label("claude-opus-5-5"), "opus");
        assert_eq!(CLAUDE.model_label("claude-sonnet-4-5-20250929"), "sonnet");
        assert_eq!(CLAUDE.model_label("gemini-3"), "gemini-3");
    }

    #[test]
    fn classic_argv_resumes_the_conversation() {
        assert_eq!(CLAUDE.classic_argv("abc"), ["claude", "--resume", "abc"]);
    }

    /// Ruling 40: a new conversation starts under an id fuxx chose (`--session-id`), so `ps`
    /// names it from the first moment (a crash's orphan still holds it, R13).
    #[test]
    fn a_new_conversation_starts_under_the_id_fuxx_chose() {
        let id = CLAUDE
            .fresh_conversation_id()
            .expect("Claude takes a chosen id");
        assert!(uuid_shaped(&id), "{id}");
        assert_ne!(
            CLAUDE.fresh_conversation_id(),
            Some(id.clone()),
            "a new one each time"
        );
        let fresh = Start {
            cwd: "/tmp".into(),
            resume: None,
            new_id: Some(id.clone()),
            mode: None,
            extra_args: ["--session-id", "mine", "--model", "sonnet"]
                .map(String::from)
                .to_vec(),
            model: None,
            effort: None,
        };
        let a = CLAUDE.argv(&fresh);
        assert!(
            a.windows(2).any(|w| w == ["--session-id", id.as_str()]),
            "{a:?}"
        );
        assert_eq!(
            a.iter().filter(|w| *w == "--session-id").count(),
            1,
            "{a:?}"
        );
        assert!(
            !a.contains(&"mine".to_string()),
            "a preset's own id is dropped: {a:?}"
        );
        assert_eq!(CLAUDE.conversation_in_args(&a.join(" ")), Some(id.clone()));
        // Resuming, the chosen id plays no part.
        let resumed = Start {
            resume: Some("abc".into()),
            ..fresh
        };
        let a = CLAUDE.argv(&resumed);
        assert!(a.windows(2).any(|w| w == ["--resume", "abc"]));
        assert!(!a.contains(&"--session-id".to_string()), "{a:?}");
    }

    /// Only an id of Claude's shape is ever typed into a shell (the classic view).
    #[test]
    fn a_conversation_id_is_a_session_id() {
        assert!(CLAUDE.is_conversation_id("0b3c4a5e-1f2d-4e6a-9b8c-7d6e5f4a3b2c"));
        assert!(!CLAUDE.is_conversation_id("abc"));
        assert!(!CLAUDE.is_conversation_id("0b3c4a5e-1f2d-4e6a-9b8c-7d6e5f4a3b2c; rm -rf ~"));
        assert!(!CLAUDE.is_conversation_id(""));
    }

    const SLASH: &str = include_str!("fixtures/claude-slash.jsonl");
    const INITIALIZE: &str = include_str!("fixtures/claude-initialize.jsonl");

    fn the_answer(text: &str) -> String {
        text.lines()
            .find(|l| l.contains("fuxx-init") && l.contains("control_response"))
            .expect("the initialize answer")
            .to_string()
    }

    #[test]
    fn the_initialize_answer_is_the_catalog() {
        // `claude-slash.jsonl` (its line 9) holds the tab's own answer.
        let c = CLAUDE.read_catalog(&[the_answer(SLASH)]).unwrap();
        let ids: Vec<&str> = c.models.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids, ["default", "opus", "fable"]);
        let labels: Vec<&str> = c.models.iter().map(|m| m.label.as_str()).collect();
        assert_eq!(labels, ["Default (recommended)", "Opus 5.5", "Fable 5.2"]);
        assert_eq!(c.models[0].resolves_to.as_deref(), Some("claude-opus-5-5"));
        assert_eq!(c.models[1].resolves_to.as_deref(), Some("claude-opus-5-5"));
        for m in &c.models {
            let levels: Vec<&str> = m.efforts.iter().map(|e| e.level.as_str()).collect();
            assert_eq!(levels, ["low", "medium", "high", "xhigh", "max"]);
            assert!(m.efforts.iter().all(|e| e.model_id.is_none()));
        }
        // Task 0's recording of this answer has one skill and one sub-agent (the made-up ones).
        let kinds: Vec<(&str, CommandKind)> = c
            .commands
            .iter()
            .map(|x| (x.name.as_str(), x.kind))
            .collect();
        assert_eq!(
            kinds,
            [
                ("example-skill", CommandKind::Command),
                ("example-agent", CommandKind::Agent)
            ]
        );
        assert_eq!(c.mode.as_deref(), Some("default"));
        assert!(!c.fallback_models);

        // The start-up probe's own recording: all 14 models, Claude's commands, then its agents.
        let c = CLAUDE.read_catalog(&[the_answer(INITIALIZE)]).unwrap();
        assert_eq!(c.models.len(), 14);
        let haiku = c
            .models
            .iter()
            .find(|m| m.id == "claude-haiku-4-5-20251001");
        assert!(haiku.is_some_and(|m| m.efforts.is_empty()), "no effort");
        assert_eq!(c.commands.len(), 9);
        assert_eq!(c.commands[0].name, "clear");
        assert_eq!(c.commands[0].aliases, ["reset", "new"]);
        assert_eq!(c.commands[0].kind, CommandKind::Command);
        assert_eq!(c.commands[6].name, "Explore");
        assert_eq!(c.commands[6].kind, CommandKind::Agent);
        assert!(c.commands[6].about.starts_with("Fast read-only search"));
    }

    #[test]
    fn the_fallback_list_gives_haiku_no_efforts() {
        let models = CLAUDE.fallback_models();
        let efforts = |id: &str| -> Vec<String> {
            models
                .iter()
                .find(|m| m.id == id)
                .unwrap_or_else(|| panic!("{id} is in the list"))
                .efforts
                .iter()
                .map(|e| e.level.clone())
                .collect()
        };
        assert!(efforts("haiku").is_empty(), "Haiku takes no effort");
        assert_eq!(efforts("opus"), FALLBACK_EFFORTS);
        assert_eq!(efforts("sonnet"), FALLBACK_EFFORTS);
    }

    #[test]
    fn no_models_falls_back_to_the_aliases() {
        let line = json!({"type": "control_response", "response": {"subtype": "success",
            "request_id": "fuxx-init", "response": {"commands": [], "agents": []}}});
        let c = CLAUDE.read_catalog(&[line.to_string()]).unwrap();
        let ids: Vec<&str> = c.models.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids, ["opus", "sonnet", "haiku", "fable"]);
        assert!(c.fallback_models);
        assert_eq!(c.models, CLAUDE.fallback_models());
        // An output without the answer line is no catalog.
        let other = r#"{"type":"system","subtype":"hook_started"}"#;
        assert!(CLAUDE.read_catalog(&[other.to_string()]).is_err());
        assert!(CLAUDE.read_catalog(&[]).is_err());
    }

    #[test]
    fn the_tabs_own_initialize_answer_is_a_catalog_event() {
        let ev = decode_all(&CLAUDE, SLASH);
        let catalogs: Vec<&Catalog> = ev
            .iter()
            .filter_map(|e| match e {
                Event::Catalog(c) => Some(c),
                _ => None,
            })
            .collect();
        assert_eq!(catalogs.len(), 1);
        assert_eq!(catalogs[0].models.len(), 3);
        let skills = ev.iter().find_map(|e| match e {
            Event::Skills(s) => Some(s),
            _ => None,
        });
        assert_eq!(skills.map(|s| s[0].as_str()), Some("example-skill"));
    }

    #[test]
    fn claude_catalog_comes_with_its_start() {
        assert!(CLAUDE.catalog_on_start());
        assert!(!crate::providers::agy::AGY.catalog_on_start());
        // Its probe is the stream-json process without a conversation, asked `initialize`.
        let probes = CLAUDE.catalog_probes();
        assert_eq!(probes.len(), 1);
        let p = &probes[0];
        assert_eq!(p.argv[0], "claude");
        assert!(
            !p.argv
                .iter()
                .any(|w| w == "--resume" || w == "--session-id")
        );
        assert_eq!(p.stdin.len(), 1);
        assert!(p.stdin[0].contains(r#""subtype":"initialize""#));
        let until = p.until.expect("it keeps running until answered");
        assert!(until(&the_answer(SLASH)));
        assert!(!until(r#"{"type":"system","subtype":"init"}"#));
    }

    #[test]
    fn claude_modes_are_its_four() {
        let i = CLAUDE.info();
        let ids: Vec<&str> = i.modes.iter().map(|m| m.id).collect();
        assert_eq!(ids, ["default", "acceptEdits", "plan", "auto"]);
        let labels: Vec<&str> = i.modes.iter().map(|m| m.label).collect();
        assert_eq!(labels, ["Ask before edits", "Accept edits", "Plan", "Auto"]);
        assert!(i.modes.iter().all(|m| !m.about.is_empty()));
        assert_eq!(i.default_mode, None);
    }

    #[test]
    fn argv_passes_model_effort_and_mode_only_when_chosen() {
        let mut s = Start {
            cwd: "/tmp".into(),
            resume: None,
            new_id: None,
            mode: None,
            model: None,
            effort: None,
            extra_args: vec!["--model".into(), "x".into()],
        };
        let a = CLAUDE.argv(&s);
        for flag in ["--effort", "--permission-mode"] {
            assert!(!a.iter().any(|w| w == flag), "{flag}: {a:?}");
        }
        assert!(
            a.ends_with(&["--model".to_string(), "x".to_string()]),
            "{a:?}"
        );
        s.model = Some("sonnet".into());
        s.effort = Some("low".into());
        s.mode = Some("plan".into());
        let a = CLAUDE.argv(&s);
        for pair in [
            ["--model", "sonnet"],
            ["--effort", "low"],
            ["--permission-mode", "plan"],
        ] {
            assert_eq!(
                a.windows(2).filter(|w| *w == pair).count(),
                1,
                "{pair:?}: {a:?}"
            );
        }
        assert!(
            !a.contains(&"x".to_string()),
            "the preset's --model is dropped: {a:?}"
        );
    }

    const USAGE: &str = include_str!("fixtures/claude-usage.json");

    fn usage_json(result: &str) -> String {
        json!({"type": "result", "result": result}).to_string()
    }

    fn read(name: &str, left_pct: u8, resets: &str) -> UsageLimit {
        UsageLimit::Read {
            name: name.into(),
            left_pct,
            resets: Some(Reset::Words(resets.into())),
        }
    }

    #[test]
    fn usage_lines_read_as_percent_left() {
        let text = "Current session: 0% used · resets Nov 17 at 8:09pm (UTC)\n\
            Current week (all models): 37% used · resets Nov 22 at 9:59pm (UTC)";
        let groups = CLAUDE.read_usage(&usage_json(text)).unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].name, "");
        assert_eq!(groups[0].about, None);
        assert_eq!(
            groups[0].limits,
            [
                read("Current session", 100, "Nov 17 at 8:09pm"),
                read("Current week (all models)", 63, "Nov 22 at 9:59pm"),
            ]
        );
        let fixture = CLAUDE.read_usage(USAGE).unwrap();
        let limits = &fixture[0].limits;
        assert!(limits.iter().any(|l| matches!(l, UsageLimit::Read { .. })));
        assert!(limits.contains(&read("Current week (Fable)", 100, "Nov 22 at 10pm")));
    }

    #[test]
    fn an_unreadable_usage_line_is_kept_as_text() {
        let groups = CLAUDE
            .read_usage(&usage_json(
                "Extra usage: off\n\n  \nCurrent session: 5% used",
            ))
            .unwrap();
        assert_eq!(
            groups[0].limits,
            [
                UsageLimit::Text("Extra usage: off".into()),
                UsageLimit::Read {
                    name: "Current session".into(),
                    left_pct: 95,
                    resets: None
                },
            ]
        );
    }

    #[test]
    fn usage_drops_the_header_and_the_explanations_but_keeps_a_limit_it_cannot_read() {
        let text = "You are currently using your subscription to power your Claude Code usage\n\n\
            Current session: 13% used \u{b7} resets Nov 17 at 8:10pm (UTC)\n\
            Extra usage: off\n\
            Current week: unavailable\n\
            What's contributing to your usage?\n\
            Most of it came from long sessions with a lot of context.";
        let limits = &CLAUDE.read_usage(&usage_json(text)).unwrap()[0].limits;
        assert_eq!(
            limits,
            &[
                read("Current session", 87, "Nov 17 at 8:10pm"),
                UsageLimit::Text("Extra usage: off".into()),
                UsageLimit::Text("Current week: unavailable".into()),
            ]
        );
        let fixture = CLAUDE.read_usage(USAGE).unwrap();
        assert!(
            fixture[0]
                .limits
                .iter()
                .all(|l| matches!(l, UsageLimit::Read { .. })),
            "the fixture's header is no limit: {:?}",
            fixture[0].limits
        );
    }

    #[test]
    fn garbage_usage_is_an_error() {
        assert_eq!(
            CLAUDE.read_usage("not json"),
            Err("Couldn't read usage".into())
        );
        assert!(CLAUDE.read_usage(r#"{"type":"result"}"#).is_err());
        assert!(CLAUDE.read_usage("").is_err());
    }

    #[test]
    fn claude_usage_runs_without_a_conversation_or_a_transcript() {
        let p = CLAUDE.usage_probe().expect("claude has /usage");
        assert_eq!(
            p.argv,
            [
                "claude",
                "-p",
                "/usage",
                "--output-format",
                "json",
                "--no-session-persistence"
            ]
        );
        assert!(p.stdin.is_empty() && p.until.is_none());
    }

    #[test]
    fn rate_limit_events_become_rate_limits() {
        let limits: Vec<RateLimit> = decode_all(&CLAUDE, SLASH)
            .into_iter()
            .filter_map(|e| match e {
                Event::RateLimit(r) => Some(r),
                _ => None,
            })
            .collect();
        // M11ap: each line's own window, then the others it carries (`unifiedWindows`), each
        // with how much is used — live usage while Claude works, no `/usage` needed.
        let limit = |name: &str, state, resets_at, used| RateLimit {
            limit: String::from(name),
            state,
            resets_at: Some(resets_at),
            used_pct: Some(used),
        };
        let (week, session) = ("Current week (all models)", "Current session");
        assert_eq!(
            limits,
            [
                limit(week, LimitState::Close, 1794754800, 78),
                limit(session, LimitState::Fine, 1794535800, 90),
                limit(session, LimitState::Close, 1794535800, 90),
                limit(week, LimitState::Close, 1794754800, 78),
            ]
        );
    }

    /// The review's finding 1: an ordinary ("allowed") line has no top-level `utilization`;
    /// its own window's use is in `unifiedWindows` — the live session must still come through.
    #[test]
    fn an_allowed_line_takes_its_own_windows_use_from_unified_windows() {
        let line = json!({"type": "rate_limit_event", "rate_limit_info": {
            "status": "allowed", "resetsAt": 1794921000, "rateLimitType": "five_hour",
            "unifiedWindows": {
                "five_hour": {"utilization": 0.12, "resetsAt": 1794921000, "status": "allowed"},
                "seven_day": {"utilization": 0.39, "resetsAt": 1795359600, "status": "allowed"}}}})
        .to_string();
        let used: Vec<(String, Option<u8>)> = CLAUDE
            .translator()
            .decode(&line)
            .into_iter()
            .filter_map(|e| match e {
                Event::RateLimit(r) => Some((r.limit, r.used_pct)),
                _ => None,
            })
            .collect();
        assert_eq!(
            used,
            [
                ("Current session".to_string(), Some(12)),
                ("Current week (all models)".to_string(), Some(39)),
            ]
        );
    }

    #[test]
    fn a_rate_limit_status_maps_to_a_state_and_unknown_types_keep_their_name() {
        let one = |status: &str, kind: &str| {
            let line = json!({"type": "rate_limit_event",
                "rate_limit_info": {"status": status, "rateLimitType": kind}})
            .to_string();
            CLAUDE.translator().decode(&line)
        };
        let state = |ev: Vec<Event>| match ev.as_slice() {
            [Event::RateLimit(r)] => (r.limit.clone(), r.state, r.resets_at),
            other => panic!("{other:?}"),
        };
        assert_eq!(
            state(one("allowed", "five_hour")),
            ("Current session".into(), LimitState::Fine, None)
        );
        assert_eq!(
            state(one("rejected", "opus_week")),
            ("opus_week".into(), LimitState::Reached, None)
        );
        assert!(one("brand_new", "five_hour").is_empty());
    }

    /// The `Applied` events among `events`.
    fn applied(events: Vec<Event>) -> Vec<(ChangeKind, String, bool)> {
        events
            .into_iter()
            .filter_map(|e| match e {
                Event::Applied { kind, value, ok } => Some((kind, value, ok)),
                _ => None,
            })
            .collect()
    }

    /// `text` with each recorded request id the recorder chose turned into the one fuxx writes.
    fn renamed(text: &str, ids: &[(&str, &str)]) -> String {
        ids.iter().fold(text.to_string(), |t, (from, to)| {
            t.replace(&format!("\"{from}\""), &format!("\"{to}\""))
        })
    }

    #[test]
    fn set_requests_encode_as_recorded() {
        let mut t = CLAUDE.translator();
        let lines = |t: &mut Box<dyn Translator>, a: Action| -> Vec<Value> {
            t.encode(&a)
                .iter()
                .map(|l| serde_json::from_str(l).unwrap())
                .collect()
        };
        let request = |id: &str, request: Value| json!({"type": "control_request", "request_id": id, "request": request});
        assert_eq!(
            lines(
                &mut t,
                Action::SetModel {
                    model: "sonnet".into()
                }
            ),
            [request(
                "fuxx-set-1",
                json!({"subtype": "set_model", "model": "sonnet"})
            )]
        );
        // The effort, then `get_settings` to read it back (its answer has no body).
        assert_eq!(
            lines(
                &mut t,
                Action::SetEffort {
                    effort: "low".into()
                }
            ),
            [
                request(
                    "fuxx-set-2",
                    json!({"subtype": "apply_flag_settings", "settings": {"effortLevel": "low"}})
                ),
                request("fuxx-set-3", json!({"subtype": "get_settings"})),
            ]
        );
        assert_eq!(
            lines(
                &mut t,
                Action::SetMode {
                    mode: "plan".into()
                }
            ),
            [request(
                "fuxx-set-4",
                json!({"subtype": "set_permission_mode", "mode": "plan"})
            )]
        );
    }

    #[test]
    fn set_answers_decode_to_applied() {
        use ChangeKind::*;
        let mut t = CLAUDE.translator();
        t.encode(&Action::SetModel {
            model: "sonnet".into(),
        });
        t.encode(&Action::SetModel {
            model: "opus".into(),
        });
        let text = include_str!("fixtures/claude-set-model.jsonl");
        let events = text.lines().flat_map(|l| t.decode(l)).collect();
        assert_eq!(
            applied(events),
            [
                (Model, "claude-sonnet-5-5".to_string(), true),
                (Model, "claude-opus-5-5".to_string(), true),
            ],
            "the resolved ids, before the first message and mid-conversation"
        );

        let mut t = CLAUDE.translator();
        t.encode(&Action::SetEffort {
            effort: "low".into(),
        });
        t.encode(&Action::SetEffort {
            effort: "medium".into(),
        });
        let text = renamed(
            include_str!("fixtures/claude-set-effort.jsonl"),
            &[
                ("fuxx-effort-1", "fuxx-set-1"),
                ("fuxx-settings-2", "fuxx-set-2"),
                ("fuxx-effort-2", "fuxx-set-3"),
                ("fuxx-settings-4", "fuxx-set-4"),
            ],
        );
        let events = text.lines().flat_map(|l| t.decode(l)).collect();
        assert_eq!(
            applied(events),
            [
                (Effort, "low".to_string(), true),
                (Effort, "medium".to_string(), true),
            ],
            "read back by get_settings; the recorder's other reads are not fuxx's"
        );

        let mut t = CLAUDE.translator();
        t.encode(&Action::SetMode {
            mode: "plan".into(),
        });
        t.encode(&Action::SetMode {
            mode: "acceptEdits".into(),
        });
        let text = renamed(
            include_str!("fixtures/claude-set-mode.jsonl"),
            &[("fuxx-mode-1", "fuxx-set-1"), ("fuxx-mode-2", "fuxx-set-2")],
        );
        let events = text.lines().flat_map(|l| t.decode(l)).collect();
        assert_eq!(
            applied(events),
            [
                (Mode, "plan".to_string(), true),
                (Mode, "acceptEdits".to_string(), true),
            ]
        );
    }

    #[test]
    fn the_mode_claude_reports_decodes_to_a_mode_event() {
        // `system/status` and `system/init` carry the mode Claude runs in, also when it changes
        // it itself (a plan approved leaves plan mode), R9.
        let mut t = CLAUDE.translator();
        let modes: Vec<String> = include_str!("fixtures/claude-set-mode.jsonl")
            .lines()
            .flat_map(|l| t.decode(l))
            .filter_map(|e| match e {
                Event::Mode(mode) => Some(mode),
                _ => None,
            })
            .collect();
        assert_eq!(modes, ["plan", "plan", "acceptEdits", "acceptEdits"]);
        let status = json!({"type":"system","subtype":"status","status":null,
            "permissionMode":"default"})
        .to_string();
        assert_eq!(t.decode(&status), [Event::Mode("default".into())]);
    }

    #[test]
    fn a_refused_set_decodes_to_not_applied() {
        use ChangeKind::*;
        let error = |id: &str, why: &str| {
            json!({"type": "control_response",
                "response": {"subtype": "error", "request_id": id, "error": why}})
            .to_string()
        };
        let mut t = CLAUDE.translator();
        t.encode(&Action::SetModel {
            model: "nope".into(),
        });
        assert_eq!(
            applied(t.decode(&error("fuxx-set-1", "Unknown model: nope"))),
            [(Model, "Unknown model: nope".to_string(), false)]
        );
        // A refused effort: its read-back then says nothing.
        t.encode(&Action::SetEffort {
            effort: "max".into(),
        });
        assert_eq!(
            applied(t.decode(&error("fuxx-set-2", "Not supported"))),
            [(Effort, "Not supported".to_string(), false)]
        );
        let read_back = json!({"type": "control_response", "response": {"subtype": "success",
            "request_id": "fuxx-set-3", "response": {"applied": {"effort": "high"}}}})
        .to_string();
        assert!(applied(t.decode(&read_back)).is_empty());
        // An answer to nothing fuxx asked is no change.
        assert!(applied(t.decode(&error("fuxx-set-9", "?"))).is_empty());
        let other = json!({"type": "control_response", "response": {"subtype": "success",
            "request_id": "fuxx-set-9", "response": {"model": "x"}}})
        .to_string();
        assert!(applied(t.decode(&other)).is_empty());
    }

    /// A live switch to another model: the context is measured against the new model's window.
    #[test]
    fn a_switched_model_sizes_the_context() {
        let init =
            json!({"type":"system","subtype":"init","session_id":"s","model":"big"}).to_string();
        let result = json!({"type":"result","subtype":"success","is_error":false,"result":"ok",
            "modelUsage":{"small":{"contextWindow":200000},"big":{"contextWindow":1000000}},
            "usage":{"iterations":[{"input_tokens":100000}]}})
        .to_string();
        let mut t = CLAUDE.translator();
        t.decode(&init);
        t.encode(&Action::SetModel {
            model: "small-alias".into(),
        });
        let answer = json!({"type": "control_response", "response": {"subtype": "success",
            "request_id": "fuxx-set-1", "response": {"model": "small"}}})
        .to_string();
        t.decode(&answer);
        let pct = t.decode(&result).into_iter().find_map(|e| match e {
            Event::Usage { context_pct, .. } => context_pct,
            _ => None,
        });
        assert_eq!(pct, Some(50), "100k of small's 200k, not big's 1M");
    }

    #[test]
    fn claude_applies_per_task_0() {
        assert_eq!(CLAUDE.applies(ChangeKind::Model), Applies::Live);
        assert_eq!(CLAUDE.applies(ChangeKind::Effort), Applies::Live);
        assert_eq!(CLAUDE.applies(ChangeKind::Mode), Applies::Live);
        assert_eq!(CLAUDE.applies(ChangeKind::Folder), Applies::Restart);
    }

    #[test]
    fn claude_json_projects_are_its_project_keys() {
        let text = json!({
            "numStartups": 3,
            "projects": {
                "/tmp/fx/code/my-app": {"allowedTools": []},
                "/tmp/fx/code/scratch": {}
            }
        })
        .to_string();
        let mut found = claude_json_projects(&text);
        found.sort();
        assert_eq!(
            found,
            ["/tmp/fx/code/my-app", "/tmp/fx/code/scratch"].map(PathBuf::from)
        );
        assert!(claude_json_projects("not json").is_empty());
        assert!(claude_json_projects(r#"{"projects": []}"#).is_empty());
        assert!(claude_json_projects("{}").is_empty());
        // Read from the home given (a made-up one); a missing file is none.
        let home = scratch_home("known");
        std::fs::write(home.join(".claude.json"), &text).unwrap();
        assert_eq!(CLAUDE.known_folders(&home).len(), 2);
        std::fs::remove_file(home.join(".claude.json")).unwrap();
        assert!(CLAUDE.known_folders(&home).is_empty());
        let _ = std::fs::remove_dir_all(&home);
    }

    /// An empty made-up home folder for one test.
    fn scratch_home(name: &str) -> PathBuf {
        let home =
            std::env::temp_dir().join(format!("fuxx-claude-home-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        home
    }

    /// Task 0 (`claude-move.jsonl`): after `--resume` from another folder, the transcript stays
    /// under the folder the conversation started in. A tab now in the new folder still reads it.
    #[test]
    fn history_finds_a_moved_conversation() {
        let home = scratch_home("moved");
        let first = home.join("code/scratch");
        let moved = first.join("moved");
        std::fs::create_dir_all(&moved).unwrap();
        let id = "6a1c234d-7469-40ef-b093-c74ad8006c28";
        let first_real = std::fs::canonicalize(&first).unwrap();
        let at = transcript_path(&home, &first_real, id);
        std::fs::create_dir_all(at.parent().unwrap()).unwrap();
        std::fs::write(&at, include_str!("fixtures/claude-transcript.jsonl")).unwrap();
        // The new folder's entry has only an empty `memory/` (as Task 0 found).
        let moved_real = std::fs::canonicalize(&moved).unwrap();
        let new_entry = transcript_path(&home, &moved_real, id);
        std::fs::create_dir_all(new_entry.parent().unwrap().join("memory")).unwrap();
        let start = Start {
            cwd: moved.clone(),
            resume: Some(id.into()),
            new_id: None,
            mode: None,
            model: None,
            effort: None,
            extra_args: Vec::new(),
        };
        let past = history_in(&home, &start);
        assert_eq!(
            past.first(),
            Some(&Past::You("Say hello in one word".into())),
            "read from the first folder's entry: {past:?}"
        );
        assert_eq!(past.len(), 4);
        // Another id is found nowhere.
        let other = Start {
            resume: Some("00000000-0000-4000-8000-000000000000".into()),
            ..start
        };
        assert!(history_in(&home, &other).is_empty());
        let _ = std::fs::remove_dir_all(&home);
    }

    /// The id comes from the sessions file: one that isn't a session id (a path, `..`) is never
    /// joined into a path, so nothing outside a transcript is read.
    #[test]
    fn history_reads_only_a_session_id_s_transcript() {
        let home = scratch_home("escape");
        let projects = home.join(".claude/projects");
        let folder = home.join("code");
        std::fs::create_dir_all(&folder).unwrap();
        let entry = transcript_path(&home, &std::fs::canonicalize(&folder).unwrap(), "x");
        std::fs::create_dir_all(entry.parent().unwrap()).unwrap();
        // Where `../escape` from the folder's entry, or from any entry, would lead.
        let transcript = include_str!("fixtures/claude-transcript.jsonl");
        std::fs::write(projects.join("escape.jsonl"), transcript).unwrap();
        std::fs::write(home.join(".claude/escape.jsonl"), transcript).unwrap();
        let start = |id: &str| Start {
            cwd: folder.clone(),
            resume: Some(id.into()),
            new_id: None,
            mode: None,
            model: None,
            effort: None,
            extra_args: Vec::new(),
        };
        for id in ["../escape", "../../escape", "a/../../escape", ""] {
            assert!(history_in(&home, &start(id)).is_empty(), "{id:?}");
        }
        let _ = std::fs::remove_dir_all(&home);
    }
}
