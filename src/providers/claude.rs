//! Claude Code's `--output-format stream-json` / `--input-format stream-json` wire format
//! (recorded fixtures: `fixtures/claude-*.jsonl`, Claude Code 2.1.323) to fuxx events and back.

use super::*;
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
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
    default_mode: Some("auto"),
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
    // Branching (batch 3, chore 2): fuxx's own when it rewinds or forks.
    ("--fork-session", false),
    ("--resume-session-at", true),
    ("--resume-drops-turn", true),
    ("--rewind-files", true),
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
            // Each message echoed when Claude reads it (`Event::Taken`): a note sent mid-turn
            // is read in that turn, with no `result` of its own (your report, 2026-11-22).
            "--replay-user-messages",
            "--permission-prompt-tool",
            "stdio",
        ]
        .map(String::from)
        .to_vec();
        match (&start.branch, &start.resume, &start.new_id) {
            // Rewind and Fork (batch 3, chore 2): the old conversation resumed up to the entry
            // before the cut message, as a new session under fuxx's id (`--fork-session`): the
            // old transcript is never touched (Claude Code 2.1.330's `--help`).
            (Some(Branch { from, at: Some(at) }), _, new_id) => {
                a.extend(["--resume".to_string(), from.clone()]);
                a.extend(["--resume-session-at".to_string(), at.clone()]);
                a.push("--fork-session".to_string());
                if let Some(id) = new_id {
                    a.extend(["--session-id".to_string(), id.clone()]);
                }
            }
            // The first message cut: nothing to keep, a fresh conversation.
            (Some(Branch { at: None, .. }), _, Some(id)) => {
                a.extend(["--session-id".to_string(), id.clone()])
            }
            (Some(Branch { at: None, .. }), _, None) => {}
            (None, Some(id), _) => a.extend(["--resume".to_string(), id.clone()]),
            // A new conversation under the id fuxx chose: init reports that same id (checked
            // with Claude Code 2.1.326; an id already used is refused).
            (None, None, Some(id)) => a.extend(["--session-id".to_string(), id.clone()]),
            (None, None, None) => {}
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
            branch: None,
        };
        vec![Probe {
            argv: self.argv(&start),
            stdin: vec![initialize_line()],
            until: Some(is_initialize_answer),
            env: Vec::new(),
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

    /// Claude guesses your next message after a turn when asked by this flag...
    fn suggestion_flags(&self) -> Vec<String> {
        vec!["--prompt-suggestions".into(), "true".into()]
    }

    /// File checkpoints, always: stream-json keeps none unless this is set (Claude Code
    /// 2.1.330's binary; `CLAUDE_CODE_DISABLE_FILE_CHECKPOINTING` still vetoes it), and Rewind's
    /// file undo needs them (batch 3, chore 2). With suggestions on, the variable without which
    /// it sent one rarely in recordings (Claude Code 2.1.330).
    fn process_env(&self, suggestions: bool) -> Vec<(String, String)> {
        let mut env = vec![(CHECKPOINTS.0.into(), CHECKPOINTS.1.into())];
        if suggestions {
            env.push(("CLAUDE_CODE_ENABLE_PROMPT_SUGGESTION".into(), "1".into()));
        }
        env
    }

    /// Rewind and Fork through `--resume-session-at` and `--fork-session`, files through its
    /// checkpoints (batch 3, chore 2).
    fn branching(&self) -> Branching {
        Branching {
            rewind: true,
            fork: true,
            files: true,
        }
    }

    /// A message not read yet goes back with `cancel_async_message` (Claude Code 2.1.330's
    /// print mode; batch 3, wave 3).
    fn cancels_queued(&self) -> bool {
        true
    }

    /// The `uuid` of a stream-json `user` line is fuxx's to choose: the echo and the transcript
    /// carry it (batch 3, chore 2).
    fn fresh_message_id(&self) -> Option<String> {
        new_uuid()
    }

    /// The message's `parentUuid` in its transcript (any chain entry: `--resume-session-at`
    /// keeps up to and including it).
    fn branch_point(
        &self,
        home: &Path,
        start: &Start,
        message: &str,
    ) -> Result<BranchPoint, String> {
        let text = transcript_text(home, start)
            .ok_or_else(|| "Couldn't read the conversation's transcript".to_string())?;
        branch_point_in(&text, message)
    }

    /// `claude -p --resume S --rewind-files <message>`: puts the files back and exits, no
    /// prompt needed (its `--help`); with checkpoints on, as the tab's process ran.
    fn rewind_files_probe(&self, start: &Start, message: &str) -> Option<Probe> {
        let id = start.resume.as_ref().filter(|id| uuid_shaped(id))?;
        if !uuid_shaped(message) {
            return None;
        }
        Some(Probe {
            argv: ["claude", "-p", "--resume", id, "--rewind-files", message]
                .map(String::from)
                .to_vec(),
            stdin: Vec::new(),
            until: None,
            env: self.process_env(false),
        })
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

    /// ✦ (M11aq): once, as text, on the quick model, not kept as a session.
    fn message_probe(&self, prompt: &str) -> Option<Probe> {
        Some(Probe {
            argv: [
                "claude",
                "-p",
                prompt,
                "--output-format",
                "text",
                "--model",
                "haiku",
                "--no-session-persistence",
                // No built-in tools: it only writes the message, never runs anything.
                "--tools",
                "",
            ]
            .map(String::from)
            .to_vec(),
            stdin: Vec::new(),
            until: None,
            env: Vec::new(),
        })
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
            env: Vec::new(),
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
    transcript_text(home, start)
        .map(|text| transcript_events(&text))
        .unwrap_or_default()
}

/// The transcript of the conversation `start` resumes, as `history_in` finds it (under its own
/// folder's entry, else any folder's); None to resume, an id not of a session id's shape, or
/// nothing readable.
fn transcript_text(home: &Path, start: &Start) -> Option<String> {
    // The id comes from the sessions file: only a session id's shape is joined into a path.
    let id = start.resume.as_ref().filter(|id| uuid_shaped(id))?;
    let cwd = std::fs::canonicalize(&start.cwd).unwrap_or_else(|_| start.cwd.clone());
    let here = transcript_path(home, &cwd, id);
    std::fs::read_to_string(&here).ok().or_else(|| {
        let name = here.file_name()?;
        std::fs::read_dir(home.join(".claude").join("projects"))
            .ok()?
            .flatten()
            .find_map(|entry| std::fs::read_to_string(entry.path().join(name)).ok())
    })
}

/// Message `message`'s place in transcript `text`: its `user` line (one you said, not a tool's
/// result) and that line's `parentUuid` (none for the first).
#[allow(dead_code)] // Batch 3 wave 2 (the Rewind/Fork UI) calls it.
fn branch_point_in(text: &str, message: &str) -> Result<BranchPoint, String> {
    text.lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .find(|v| v["uuid"] == message)
        .filter(|v| v["type"] == "user" && said_text(&v["message"]["content"]).is_some())
        .map(|v| BranchPoint {
            message: message.to_string(),
            parent: s(&v["parentUuid"]),
        })
        .ok_or_else(|| "That message isn't in the conversation's transcript".to_string())
}

/// The variable that turns Claude's file checkpoints on in stream-json, and its value.
const CHECKPOINTS: (&str, &str) = ("CLAUDE_CODE_ENABLE_SDK_FILE_CHECKPOINTING", "true");

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
    // Past subagents (M11ar): each Agent call is a lane, ended by its result.
    let mut agents: HashSet<String> = HashSet::new();
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
                    for r in content.as_array().into_iter().flatten() {
                        if let Some(id) = r["tool_use_id"].as_str()
                            && agents.remove(id)
                        {
                            let end = if r["is_error"] == true {
                                LaneEnd::Failed("It failed".into())
                            } else {
                                LaneEnd::Done
                            };
                            out.push(Past::Event(Event::SubagentEnded {
                                id: id.to_string(),
                                end,
                            }));
                        }
                    }
                } else if let Some(past) = said_text(content)
                    .as_deref()
                    .and_then(|t| user_said(t, s(&v["uuid"])))
                {
                    out.push(past);
                }
            }
            Some("assistant") => {
                out.extend(translator.assistant(&v).into_iter().map(Past::Event));
                for b in v["message"]["content"].as_array().into_iter().flatten() {
                    if b["type"] == "tool_use"
                        && matches!(b["name"].as_str(), Some("Agent" | "Task"))
                        && let Some(id) = b["id"].as_str()
                    {
                        agents.insert(id.to_string());
                        out.push(Past::Event(past_lane(id, &b["input"])));
                    }
                }
            }
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

/// What a `user` line's text was in the conversation: you (with the line's `uuid`, `id`), unless
/// it is one of Claude's own wrappers — a slash command (shown as you typed it), its output (a
/// notice), an interrupt ("Stopped"), or another `<…>` wrapper (left out).
fn user_said(text: &str, id: Option<String>) -> Option<Past> {
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
        return Some(Past::You { text: typed, id });
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
    (!wrapper && !t.is_empty()).then(|| Past::You {
        text: text.to_string(),
        id,
    })
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
    /// File rewinds asked and not answered yet (batch 3, chore 2), by request id
    /// (`fuxx-rewind-<n>`): the message and whether it was a dry run.
    rewinds: HashMap<String, (String, bool)>,
    /// `cancel_async_message` requests asked and not yet answered: request id → message id.
    cancels: HashMap<String, String>,
    /// Subagents (M11ar): Claude's task id → the lane (its Agent call's id), and back.
    lanes: HashMap<String, String>,
    tasks: HashMap<String, String>,
    /// The foreground subagents still going (a background one may outlive the turn).
    foreground: HashSet<String>,
}

/// Claude's own subagent types: a description names one of these better than its type does.
const BUILT_IN_AGENTS: [&str; 3] = ["general-purpose", "Explore", "Plan"];

/// A subagent's label: its custom type, else what it was asked to do.
fn lane_label(kind: &str, description: &str) -> String {
    if kind.is_empty() || BUILT_IN_AGENTS.contains(&kind) {
        description.to_string()
    } else {
        kind.to_string()
    }
}

/// A past Agent call (a resumed conversation's) as a lane, by the live rule.
fn past_lane(id: &str, input: &Value) -> Event {
    let kind = input["subagent_type"].as_str().unwrap_or("");
    let description = input["description"].as_str().unwrap_or("");
    Event::SubagentStarted {
        id: id.to_string(),
        parent: None,
        label: lane_label(kind, description),
        kind: kind.to_string(),
        task: input["prompt"].as_str().unwrap_or(description).to_string(),
        model: None,
        controls: LaneControls::default(),
    }
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
    // A pasted image's marker just before it: "[Image #1]" in the text means this picture.
    let mut blocks: Vec<Value> = images
        .iter()
        .flat_map(|p| {
            let label = p
                .label
                .as_deref()
                .map(|l| json!({"type": "text", "text": l}));
            label.into_iter().chain([json!({"type": "image",
            "source": {"type": "base64", "media_type": p.media_type, "data": &*p.data}})])
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

    /// The answer to a file rewind fuxx asked (`rewind_files`): `{canRewind, error?,
    /// filesChanged?, insertions?, deletions?}` (Claude Code 2.1.330's schema), or an error.
    fn rewound(&mut self, v: &Value) -> Vec<Event> {
        let response = &v["response"];
        let Some((message, dry_run)) = response["request_id"]
            .as_str()
            .and_then(|id| self.rewinds.remove(id))
        else {
            return Vec::new();
        };
        let outcome = if response["subtype"] == "success" {
            let body = &response["response"];
            FileRewind {
                can_rewind: body["canRewind"] == true,
                error: s(&body["error"]),
                files: body["filesChanged"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(s)
                    .collect(),
                insertions: body["insertions"].as_u64(),
                deletions: body["deletions"].as_u64(),
            }
        } else {
            FileRewind {
                error: Some(s(&response["error"]).unwrap_or_else(|| "Claude refused it".into())),
                ..FileRewind::default()
            }
        };
        vec![Event::FilesRewound {
            message,
            dry_run,
            outcome,
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
            // Subagents (M11ar): a `local_agent` task is one (a subagent's own Bash is a
            // `local_bash` task: not a lane).
            Some("task_started") if v["task_type"] == "local_agent" => {
                let (Some(task), Some(lane)) = (s(&v["task_id"]), s(&v["tool_use_id"])) else {
                    return Vec::new();
                };
                self.lanes.insert(task.clone(), lane.clone());
                self.tasks.insert(lane.clone(), task);
                if v["is_backgrounded"] != true {
                    self.foreground.insert(lane.clone());
                }
                let kind = s(&v["subagent_type"]).unwrap_or_default();
                let description = s(&v["description"]).unwrap_or_default();
                vec![Event::SubagentStarted {
                    id: lane,
                    parent: s(&v["parent_task_id"]).and_then(|p| self.lanes.get(&p).cloned()),
                    label: lane_label(&kind, &description),
                    kind,
                    task: s(&v["prompt"]).unwrap_or(description),
                    model: None,
                    controls: LaneControls { stop: true },
                }]
            }
            Some("task_progress") => match s(&v["task_id"]).and_then(|t| self.lanes.get(&t)) {
                Some(lane) => vec![Event::SubagentProgress {
                    id: lane.clone(),
                    tokens: v["usage"]["total_tokens"].as_u64(),
                    tools: v["usage"]["tool_uses"].as_u64().map(|n| n as u32),
                    last_tool: s(&v["last_tool_name"]),
                }],
                None => Vec::new(),
            },
            Some("task_notification") => match s(&v["task_id"]).and_then(|t| self.lanes.get(&t)) {
                Some(lane) => {
                    self.foreground.remove(lane);
                    let end = match v["status"].as_str() {
                        Some("failed") => {
                            LaneEnd::Failed(s(&v["summary"]).unwrap_or_else(|| "It failed".into()))
                        }
                        Some("killed") | Some("stopped") => LaneEnd::Stopped,
                        _ => LaneEnd::Done,
                    };
                    vec![Event::SubagentEnded {
                        id: lane.clone(),
                        end,
                    }]
                }
                None => Vec::new(),
            },
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
        // A foreground subagent can't outlive the turn: one still open was cut off (an
        // interrupt). A background one goes on to its own `task_notification`.
        let mut cut: Vec<String> = self.foreground.drain().collect();
        cut.sort();
        out.extend(cut.into_iter().map(|id| Event::SubagentEnded {
            id,
            end: LaneEnd::Stopped,
        }));
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
            // A subagent's request names it (`agent_id` is its task id).
            by: s(&req["agent_id"]).and_then(|a| self.lanes.get(&a).cloned()),
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
            // A refused stop of one subagent (M11ar): said, not swallowed.
            Some("control_response")
                if v["response"]["request_id"]
                    .as_str()
                    .is_some_and(|r| r.starts_with("fuxx-stop-")) =>
            {
                match v["response"]["subtype"].as_str() {
                    Some("error") => vec![Event::Notice(format!(
                        "Couldn't stop the subagent: {}",
                        s(&v["response"]["error"]).unwrap_or_else(|| "no reason given".into())
                    ))],
                    _ => Vec::new(),
                }
            }
            Some("control_response")
                if v["response"]["request_id"]
                    .as_str()
                    .is_some_and(|r| self.rewinds.contains_key(r)) =>
            {
                self.rewound(&v)
            }
            // The answer to a cancel fuxx asked: `{cancelled, message_uuid?}`; an error (an
            // older Claude) is not cancelled.
            Some("control_response")
                if v["response"]["request_id"]
                    .as_str()
                    .is_some_and(|r| self.cancels.contains_key(r)) =>
            {
                let response = &v["response"];
                let rid = response["request_id"].as_str().unwrap_or_default();
                let Some(id) = self.cancels.remove(rid) else {
                    return Vec::new();
                };
                let cancelled =
                    response["subtype"] == "success" && response["response"]["cancelled"] == true;
                vec![Event::QueuedCancelled { id, cancelled }]
            }
            Some("control_response") => self.answered(&v),
            Some("stream_event") => self.stream_event(&v),
            Some("assistant") => self.assistant(&v),
            // An echo of a message of yours (`--replay-user-messages`): it was read.
            Some("user")
                if v["isReplay"] == true && said_text(&v["message"]["content"]).is_some() =>
            {
                vec![Event::Taken { id: s(&v["uuid"]) }]
            }
            Some("user") => self.user(&v),
            Some("result") => self.result(&v),
            Some("rate_limit_event") => rate_limit(&v),
            // `/clear`: then a new `system/init` under a new session id (Claude Code 2.1.330).
            Some("conversation_reset") => vec![Event::Cleared],
            // A guess at your next message, about a second after a turn's `result` (with
            // `--prompt-suggestions true`; Claude Code 2.1.330). Not every turn has one.
            Some("prompt_suggestion") => v["suggestion"]
                .as_str()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| vec![Event::Suggestion(s.to_string())])
                .unwrap_or_default(),
            Some("control_request") if v["request"]["subtype"] == "can_use_tool" => {
                self.permission(&v)
            }
            _ => Vec::new(),
        }
    }

    fn encode(&mut self, action: &Action) -> Vec<String> {
        match action {
            // One subagent alone (M11ar), by its task id.
            Action::StopSubagent { lane } => match self.tasks.get(lane) {
                Some(task) => vec![
                    json!({"type": "control_request", "request_id": format!("fuxx-stop-{task}"),
                    "request": {"subtype": "stop_task", "task_id": task}})
                    .to_string(),
                ],
                None => Vec::new(),
            },
            Action::Send { text, images, id } => {
                let mut line = json!({"type": "user",
                    "message": {"role": "user", "content": content(text, images)}});
                // fuxx's id for it (batch 3, chore 2): the echo and the transcript carry it.
                if let Some(id) = id {
                    line["uuid"] = json!(id);
                }
                vec![line.to_string()]
            }
            Action::RewindFiles { message, dry_run } => {
                self.sets += 1;
                let rid = format!("fuxx-rewind-{}", self.sets);
                let mut request = json!({"subtype": "rewind_files", "user_message_id": message});
                if *dry_run {
                    request["dry_run"] = json!(true);
                }
                self.rewinds
                    .insert(rid.clone(), (message.clone(), *dry_run));
                vec![
                    json!({"type": "control_request", "request_id": rid, "request": request})
                        .to_string(),
                ]
            }
            Action::CancelQueued { id } => {
                self.sets += 1;
                let rid = format!("fuxx-cancel-{}", self.sets);
                self.cancels.insert(rid.clone(), id.clone());
                vec![
                    json!({"type": "control_request", "request_id": rid,
                    "request": {"subtype": "cancel_async_message", "message_uuid": id}})
                    .to_string(),
                ]
            }
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

    // ---- M11ar: subagents ----

    const SUBAGENTS: &str = include_str!("fixtures/claude-subagents.jsonl");
    const SUB_STOP: &str = include_str!("fixtures/claude-subagent-stop.jsonl");
    const SUB_ASK: &str = include_str!("fixtures/claude-subagent-ask.jsonl");

    fn started_of(ev: &[Event]) -> Vec<(String, String, String, String, bool)> {
        ev.iter()
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
                    controls.stop,
                )),
                _ => None,
            })
            .collect()
    }

    fn conversation_of(text: &str) -> crate::agent_conversation::Conversation {
        let mut c = crate::agent_conversation::Conversation::new();
        c.you("go".into(), Vec::new());
        for e in decode_all(&CLAUDE, text) {
            c.apply(e);
        }
        c
    }

    #[test]
    fn task_started_becomes_a_lane() {
        let started = started_of(&decode_all(&CLAUDE, SUBAGENTS));
        assert_eq!(started.len(), 2, "two subagents: {started:?}");
        let (id, label, kind, task, stop) = &started[0];
        assert_eq!(
            id, "toolu_01A6dTWvtAdh452CDXHMraUZ",
            "keyed by the Agent call"
        );
        // A built-in type isn't a name: the description is.
        assert_eq!(label, "Read a.txt and report contents");
        assert_eq!(kind, "general-purpose");
        assert!(task.contains("a.txt"), "the task is its prompt: {task}");
        assert!(stop, "Claude can stop one");
        // A custom type is the name.
        let line = json!({"type":"system","subtype":"task_started","task_id":"t9",
            "tool_use_id":"toolu_x","description":"Review the diff","subagent_type":"code-reviewer",
            "task_type":"local_agent","prompt":"Review it"})
        .to_string();
        let ev = CLAUDE.translator().decode(&line);
        assert_eq!(started_of(&ev)[0].1, "code-reviewer");
        // A subagent's own Bash task (`local_bash`) is no lane.
        assert_eq!(started_of(&decode_all(&CLAUDE, SUB_STOP)).len(), 1);
    }

    #[test]
    fn task_progress_becomes_progress() {
        let ev = decode_all(&CLAUDE, SUBAGENTS);
        let first = ev.iter().find_map(|e| match e {
            Event::SubagentProgress {
                id,
                tokens,
                tools,
                last_tool,
            } if id == "toolu_01A6dTWvtAdh452CDXHMraUZ" => {
                Some((*tokens, *tools, last_tool.clone()))
            }
            _ => None,
        });
        assert_eq!(first, Some((Some(25_997), Some(1), Some("Read".into()))));
    }

    #[test]
    fn task_notification_ends_the_lane() {
        let end = |status: &str| {
            let mut t = CLAUDE.translator();
            t.decode(
                &json!({"type":"system","subtype":"task_started","task_id":"t1",
                "tool_use_id":"toolu_1","description":"d","subagent_type":"general-purpose",
                "task_type":"local_agent","prompt":"p"})
                .to_string(),
            );
            t.decode(
                &json!({"type":"system","subtype":"task_notification","task_id":"t1",
                "tool_use_id":"toolu_1","status":status,"summary":"no such file"})
                .to_string(),
            )
            .into_iter()
            .find_map(|e| match e {
                Event::SubagentEnded { id, end } if id == "toolu_1" => Some(end),
                _ => None,
            })
        };
        assert_eq!(end("completed"), Some(LaneEnd::Done));
        assert_eq!(end("failed"), Some(LaneEnd::Failed("no such file".into())));
        assert_eq!(end("killed"), Some(LaneEnd::Stopped));
        assert_eq!(end("stopped"), Some(LaneEnd::Stopped));
    }

    #[test]
    fn two_subagents_keep_their_own_steps() {
        let c = conversation_of(SUBAGENTS);
        let lanes = c.lanes();
        assert_eq!(lanes.len(), 2);
        for lane in lanes {
            assert_eq!(
                c.lane_steps(&lane.id),
                1,
                "{}: its own Read only",
                lane.label
            );
            assert_eq!(
                lane.status,
                crate::agent_conversation::LaneStatus::Done,
                "{}",
                lane.label
            );
        }
    }

    #[test]
    fn a_background_lane_ends_after_the_turn() {
        use crate::agent_conversation::LaneStatus;
        let mut c = crate::agent_conversation::Conversation::new();
        let mut t = CLAUDE.translator();
        let feed = |t: &mut Box<dyn Translator>,
                    c: &mut crate::agent_conversation::Conversation,
                    v: Value| {
            for e in t.decode(&v.to_string()) {
                c.apply(e);
            }
        };
        feed(
            &mut t,
            &mut c,
            json!({"type":"system","subtype":"task_started","task_id":"bg1",
            "tool_use_id":"toolu_bg","description":"Watch the build","subagent_type":"general-purpose",
            "task_type":"local_agent","is_backgrounded":true,"prompt":"watch"}),
        );
        feed(
            &mut t,
            &mut c,
            json!({"type":"result","subtype":"success","is_error":false,
            "result":"ok","session_id":"s"}),
        );
        assert_eq!(c.lane("toolu_bg").unwrap().status, LaneStatus::Working);
        feed(
            &mut t,
            &mut c,
            json!({"type":"system","subtype":"task_notification","task_id":"bg1",
            "tool_use_id":"toolu_bg","status":"completed","summary":"built"}),
        );
        assert_eq!(c.lane("toolu_bg").unwrap().status, LaneStatus::Done);
    }

    #[test]
    fn a_foreground_lane_ends_with_the_turn() {
        // Review 5: a foreground subagent can't outlive Main's `result`; a background one can.
        let mut t = CLAUDE.translator();
        for (task, lane, bg) in [("f1", "toolu_f", false), ("b1", "toolu_b", true)] {
            t.decode(
                &json!({"type":"system","subtype":"task_started","task_id":task,
                "tool_use_id":lane,"description":"d","subagent_type":"general-purpose",
                "task_type":"local_agent","is_backgrounded":bg,"prompt":"p"})
                .to_string(),
            );
        }
        let ev = t.decode(
            &json!({"type":"result","subtype":"error_during_execution","is_error":true,
            "session_id":"s"})
            .to_string(),
        );
        let ended: Vec<&str> = ev
            .iter()
            .filter_map(|e| match e {
                Event::SubagentEnded {
                    id,
                    end: LaneEnd::Stopped,
                } => Some(id.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(ended, ["toolu_f"]);
    }

    #[test]
    fn a_subagents_permission_says_who() {
        let ev = decode_all(&CLAUDE, SUB_ASK);
        let lane = started_of(&ev)[0].0.clone();
        let by = ev.iter().find_map(|e| match e {
            Event::Permission { by, .. } => Some(by.clone()),
            _ => None,
        });
        assert_eq!(by, Some(Some(lane)), "agent_id is the lane's task_id");
    }

    #[test]
    fn a_message_read_is_taken() {
        // `--replay-user-messages`: Claude echoes each message when it reads it (recorded
        // 2026-11-22: a note sent mid-turn came back inside that turn, one `result` for both).
        let start = Start {
            cwd: PathBuf::from("/tmp/fx"),
            resume: None,
            new_id: None,
            mode: None,
            extra_args: Vec::new(),
            model: None,
            effort: None,
            branch: None,
        };
        assert!(
            CLAUDE
                .argv(&start)
                .iter()
                .any(|a| a == "--replay-user-messages")
        );
        let echo = json!({"type":"user","isReplay":true,
            "message":{"role":"user","content":"Also: what is 2+2?"},"session_id":"s"})
        .to_string();
        assert_eq!(
            CLAUDE.translator().decode(&echo),
            [Event::Taken { id: None }]
        );
        // A tool's result isn't a message of yours.
        let result = json!({"type":"user","message":{"role":"user","content":[
            {"type":"tool_result","tool_use_id":"t","content":"ok"}]}})
        .to_string();
        assert!(
            !CLAUDE
                .translator()
                .decode(&result)
                .contains(&Event::Taken { id: None })
        );
    }

    #[test]
    fn a_refused_stop_says_so() {
        let mut t = CLAUDE.translator();
        let ev = t.decode(
            &json!({"type":"control_response","response":{"subtype":"error",
            "request_id":"fuxx-stop-a4dfa9abed5c81e02","error":"No such task"}})
            .to_string(),
        );
        assert_eq!(
            ev,
            [Event::Notice(
                "Couldn't stop the subagent: No such task".into()
            )]
        );
    }

    #[test]
    fn stop_subagent_writes_stop_task() {
        let mut t = CLAUDE.translator();
        let mut lane = None;
        for l in SUBAGENTS.lines() {
            for e in t.decode(l) {
                if let Event::SubagentStarted { id, .. } = e {
                    lane.get_or_insert(id);
                }
            }
        }
        let out = t.encode(&Action::StopSubagent {
            lane: lane.unwrap(),
        });
        assert_eq!(out.len(), 1);
        let v: Value = serde_json::from_str(&out[0]).unwrap();
        assert_eq!(v["type"], "control_request");
        assert_eq!(v["request"]["subtype"], "stop_task");
        assert_eq!(v["request"]["task_id"], "a4dfa9abed5c81e02");
        assert!(
            t.encode(&Action::StopSubagent {
                lane: "unknown".into()
            })
            .is_empty(),
            "an unknown lane writes nothing"
        );
    }

    #[test]
    fn the_stop_fixture_ends_one_lane_only() {
        use crate::agent_conversation::LaneStatus;
        let c = conversation_of(SUB_STOP);
        assert_eq!(c.lanes().len(), 1);
        assert_eq!(c.lanes()[0].status, LaneStatus::Stopped);
        assert!(
            c.items().iter().any(|i| matches!(
                i,
                crate::agent_conversation::Item::Agent { parent: None, .. }
            )),
            "Main went on and answered"
        );
    }

    #[test]
    fn history_restores_lanes_by_the_live_rule() {
        let agent = json!({"type":"assistant","message":{"id":"m1","role":"assistant","content":[
            {"type":"tool_use","id":"toolu_a","name":"Agent","input":{
                "description":"Find the config loader","subagent_type":"Explore",
                "prompt":"Find where config is loaded"}}]}});
        let side = json!({"type":"assistant","isSidechain":true,"message":{"id":"m2",
            "role":"assistant","content":[{"type":"text","text":"inside"}]}});
        let done = json!({"type":"user","message":{"role":"user","content":[
            {"type":"tool_result","tool_use_id":"toolu_a","content":"found it"}]}});
        let text = [agent, side, done]
            .iter()
            .map(Value::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        let mut c = crate::agent_conversation::Conversation::new();
        c.past(transcript_events(&text));
        let lanes = c.lanes();
        assert_eq!(lanes.len(), 1);
        assert_eq!(lanes[0].id, "toolu_a");
        assert_eq!(
            lanes[0].label, "Find the config loader",
            "Explore is a built-in"
        );
        assert_eq!(lanes[0].kind, "Explore");
        assert_eq!(lanes[0].task, "Find where config is loaded");
        assert_eq!(lanes[0].status, crate::agent_conversation::LaneStatus::Done);
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
            ..
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
                Past::You {
                    text: "Say hello in one word".into(),
                    id: Some("9ae3b190-4bf7-404f-ac4d-4678b6fd9c06".into()),
                },
                Past::Event(Event::MessageDone {
                    message: "msg_011Cgz3BeeVjP3aWM4pjPKbD".into(),
                    text: "Hello!".into(),
                    parent: None,
                }),
                Past::You {
                    text: "What single word did I ask you to say earlier? Answer in one word."
                        .into(),
                    id: Some("4d02f791-3abf-497c-b3fe-3503c2c8f4b2".into()),
                },
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
        assert_eq!(past[0], Past::you("fix it"));
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
                Past::you("look at\nthis"),
                Past::you("/compact"),
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
            branch: None,
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
            id: None,
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
            id: None,
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
            id: None,
        });
        let v: Value = serde_json::from_str(&out[0]).unwrap();
        assert_eq!(v["message"]["content"].as_array().map(Vec::len), Some(1));
    }

    #[test]
    fn a_pasted_images_marker_goes_just_before_it() {
        let picture = Picture {
            label: Some(Arc::from("[Image #1]")),
            ..Picture::new("image/png", b"x".to_vec())
        };
        let out = CLAUDE.translator().encode(&Action::Send {
            text: "what is in [Image #1]?".into(),
            images: vec![picture],
            id: None,
        });
        let v: Value = serde_json::from_str(&out[0]).unwrap();
        let blocks = v["message"]["content"].as_array().unwrap();
        assert_eq!(blocks[0], json!({"type": "text", "text": "[Image #1]"}));
        assert_eq!(blocks[1]["type"], "image");
        assert_eq!(
            blocks[2],
            json!({"type": "text", "text": "what is in [Image #1]?"})
        );
    }

    #[test]
    fn encode_uses_the_pictures_data_as_it_is() {
        // The base64 is made once, off the UI thread, when the picture is built; `encode`
        // (on the UI thread) only copies it.
        let picture = Picture {
            media_type: "image/png",
            bytes: Arc::from(&b"foo"[..]),
            data: Arc::from("VERBATIM"),
            label: None,
        };
        let out = CLAUDE.translator().encode(&Action::Send {
            text: String::new(),
            images: vec![picture],
            id: None,
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
            id: None,
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
            branch: None,
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
            branch: None,
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
            branch: None,
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
        assert_eq!(i.default_mode, Some("auto"), "the user's call: Auto");
    }

    // ---- Prompt suggestions (a chore after M11ar) ----

    const SUGGESTION: &str = include_str!("fixtures/claude-prompt-suggestion.jsonl");

    /// Claude's `prompt_suggestion` lines, after each turn's `result`, are the main
    /// conversation's suggestions (their `uuid` and `session_id` ignored).
    #[test]
    fn prompt_suggestion_decodes_to_a_suggestion() {
        let suggestions: Vec<String> = decode_all(&CLAUDE, SUGGESTION)
            .into_iter()
            .filter_map(|e| match e {
                Event::Suggestion(s) => Some(s),
                _ => None,
            })
            .collect();
        assert_eq!(
            suggestions,
            ["go ahead and create it", "and the third step?"]
        );
        let mut t = CLAUDE.translator();
        let padded = json!({"type": "prompt_suggestion", "suggestion": "  run the tests \n"});
        assert_eq!(
            t.decode(&padded.to_string()),
            [Event::Suggestion("run the tests".into())]
        );
    }

    #[test]
    fn a_blank_suggestion_is_nothing() {
        let mut t = CLAUDE.translator();
        for line in [
            json!({"type": "prompt_suggestion", "suggestion": "   "}),
            json!({"type": "prompt_suggestion", "suggestion": ""}),
            json!({"type": "prompt_suggestion"}),
            json!({"type": "prompt_suggestion", "suggestion": 4}),
        ] {
            assert!(t.decode(&line.to_string()).is_empty(), "{line}");
        }
    }

    /// Claude is asked for suggestions by a flag, and sends them only with its env var set.
    #[test]
    fn claude_asks_for_suggestions_by_flag_and_env() {
        assert_eq!(CLAUDE.suggestion_flags(), ["--prompt-suggestions", "true"]);
        let suggest = (
            "CLAUDE_CODE_ENABLE_PROMPT_SUGGESTION".to_string(),
            "1".to_string(),
        );
        assert!(CLAUDE.process_env(true).contains(&suggest));
        assert!(!CLAUDE.process_env(false).contains(&suggest));
    }

    #[test]
    fn argv_passes_the_auto_mode() {
        let s = Start {
            cwd: "/tmp".into(),
            resume: None,
            new_id: None,
            mode: Some("auto".into()),
            model: None,
            effort: None,
            extra_args: vec![],
            branch: None,
        };
        let a = CLAUDE.argv(&s);
        assert!(
            a.windows(2).any(|w| w == ["--permission-mode", "auto"]),
            "{a:?}"
        );
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
            branch: None,
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
        let home = crate::test_temp::dir(&format!("fuxx-claude-home-{name}"));
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
            branch: None,
        };
        let past = history_in(&home, &start);
        assert_eq!(
            past.first(),
            Some(&Past::You {
                text: "Say hello in one word".into(),
                id: Some("9ae3b190-4bf7-404f-ac4d-4678b6fd9c06".into()),
            }),
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
            branch: None,
        };
        for id in ["../escape", "../../escape", "a/../../escape", ""] {
            assert!(history_in(&home, &start(id)).is_empty(), "{id:?}");
        }
        let _ = std::fs::remove_dir_all(&home);
    }
}

/// Batch 3, chore 2 (Rewind and Fork, the provider core): message ids, the forked resume, the
/// branch point from the transcript, file checkpoints and their rewind (Claude Code 2.1.330's
/// `--help` and its binary's strings; no model called).
#[cfg(test)]
mod branch_tests {
    use super::*;
    use serde_json::{Value, json};

    fn start(resume: Option<&str>, new_id: Option<&str>, branch: Option<Branch>) -> Start {
        Start {
            cwd: PathBuf::from("/tmp/fx"),
            resume: resume.map(String::from),
            new_id: new_id.map(String::from),
            mode: None,
            model: None,
            effort: None,
            extra_args: Vec::new(),
            branch,
        }
    }

    const S: &str = "6a1c234d-7469-40ef-b093-c74ad8006c28";
    const N: &str = "11111111-2222-4333-8444-555555555555";
    const P: &str = "c6775dd7-1a5a-4e4a-975c-aab3efe12bf8";
    const U: &str = "9ae3b190-4bf7-404f-ac4d-4678b6fd9c06";

    #[test]
    fn claude_can_rewind_fork_and_undo_files() {
        assert_eq!(
            CLAUDE.branching(),
            Branching {
                rewind: true,
                fork: true,
                files: true
            }
        );
        let id = CLAUDE.fresh_message_id().expect("Claude takes fuxx's ids");
        assert!(uuid_shaped(&id), "{id}");
    }

    #[test]
    fn a_send_carries_its_message_id_and_the_echo_names_it() {
        let mut t = CLAUDE.translator();
        let out = t.encode(&Action::Send {
            text: "hi".into(),
            images: Vec::new(),
            id: Some(U.into()),
        });
        let v: Value = serde_json::from_str(&out[0]).unwrap();
        assert_eq!(
            v,
            json!({"type":"user","uuid":U,"message":{"role":"user","content":"hi"}})
        );
        // No id: the line as before.
        let out = t.encode(&Action::Send {
            text: "hi".into(),
            images: Vec::new(),
            id: None,
        });
        let v: Value = serde_json::from_str(&out[0]).unwrap();
        assert!(v.get("uuid").is_none(), "{v}");
        let echo = json!({"type":"user","isReplay":true,"uuid":U,
            "message":{"role":"user","content":"hi"},"session_id":"s"})
        .to_string();
        assert_eq!(
            CLAUDE.translator().decode(&echo),
            [Event::Taken { id: Some(U.into()) }]
        );
    }

    #[test]
    fn history_keeps_each_messages_id() {
        let past = transcript_events(include_str!("fixtures/claude-transcript.jsonl"));
        let yours: Vec<(String, Option<String>)> = past
            .into_iter()
            .filter_map(|p| match p {
                Past::You { text, id } => Some((text, id)),
                Past::Event(_) => None,
            })
            .collect();
        assert_eq!(yours[0], ("Say hello in one word".into(), Some(U.into())));
        assert_eq!(
            yours[1].1.as_deref(),
            Some("4d02f791-3abf-497c-b3fe-3503c2c8f4b2")
        );
    }

    /// Rewind and Fork both start a forked session (the old transcript untouched, so Undo
    /// Rewind can go back to it): resume S at the cut message's parent, under fuxx's new id.
    #[test]
    fn a_branch_resumes_at_its_point_and_forks_under_the_new_id() {
        let from = |at: Option<&str>| Branch {
            from: S.into(),
            at: at.map(String::from),
        };
        let argv = CLAUDE.argv(&start(Some(S), Some(N), Some(from(Some(P)))));
        let tail: Vec<&str> = argv.iter().map(String::as_str).collect();
        let at = |flag: &str| tail.iter().position(|a| *a == flag);
        let after = |flag: &str| at(flag).map(|i| tail[i + 1]);
        assert_eq!(after("--resume"), Some(S));
        assert_eq!(after("--resume-session-at"), Some(P));
        assert!(at("--fork-session").is_some(), "{tail:?}");
        assert_eq!(after("--session-id"), Some(N));
        assert_eq!(
            tail.iter().filter(|a| **a == "--resume").count(),
            1,
            "one resume: {tail:?}"
        );
        // The first message cut: nothing to keep, a fresh conversation under N.
        let argv = CLAUDE.argv(&start(Some(S), Some(N), Some(from(None))));
        assert!(
            !argv
                .iter()
                .any(|a| a == "--resume" || a == "--fork-session")
        );
        let i = argv.iter().position(|a| a == "--session-id").unwrap();
        assert_eq!(argv[i + 1], N);
        // A preset's own branch flags are fuxx's: dropped.
        let mut s = start(Some(S), None, None);
        s.extra_args = [
            "--fork-session",
            "--resume-session-at",
            "x",
            "--rewind-files",
            "y",
        ]
        .map(String::from)
        .to_vec();
        let argv = CLAUDE.argv(&s);
        for flag in ["--fork-session", "--resume-session-at", "--rewind-files"] {
            assert!(!argv.iter().any(|a| a == flag), "{flag}: {argv:?}");
        }
    }

    fn write_transcript(home: &Path, cwd: &Path, id: &str, text: &str) {
        let at = transcript_path(home, &std::fs::canonicalize(cwd).unwrap(), id);
        std::fs::create_dir_all(at.parent().unwrap()).unwrap();
        std::fs::write(at, text).unwrap();
    }

    /// The point to resume at is the cut message's `parentUuid` (any chain entry); the first
    /// message has none. Read off the UI thread, from the transcript `history` reads.
    #[test]
    fn the_branch_point_is_the_messages_parent_in_the_transcript() {
        let home = crate::test_temp::dir("fuxx-claude-branch-point");
        let _ = std::fs::remove_dir_all(&home);
        let cwd = home.join("code/my-app");
        std::fs::create_dir_all(&cwd).unwrap();
        let lines = [
            json!({"type":"user","uuid":"a","parentUuid":null,
                "message":{"role":"user","content":"first"}}),
            json!({"type":"assistant","uuid":"b","parentUuid":"a",
                "message":{"id":"m","role":"assistant","content":[{"type":"text","text":"ok"}]}}),
            json!({"type":"system","uuid":"c","parentUuid":"b","subtype":"x"}),
            json!({"type":"user","uuid":"d","parentUuid":"c",
                "message":{"role":"user","content":"second"}}),
        ]
        .map(|v| v.to_string())
        .join("\n");
        write_transcript(&home, &cwd, S, &lines);
        let mut s = start(Some(S), None, None);
        s.cwd = cwd.clone();
        assert_eq!(
            CLAUDE.branch_point(&home, &s, "d"),
            Ok(BranchPoint {
                message: "d".into(),
                parent: Some("c".into())
            })
        );
        assert_eq!(
            CLAUDE.branch_point(&home, &s, "a"),
            Ok(BranchPoint {
                message: "a".into(),
                parent: None
            })
        );
        // Not a message of yours, or not there at all: said.
        assert!(CLAUDE.branch_point(&home, &s, "b").is_err());
        assert!(CLAUDE.branch_point(&home, &s, "zz").is_err());
        // No conversation, or not a session id: nothing read.
        assert!(
            CLAUDE
                .branch_point(&home, &start(None, None, None), "d")
                .is_err()
        );
        let mut odd = s.clone();
        odd.resume = Some("../x".into());
        assert!(CLAUDE.branch_point(&home, &odd, "d").is_err());
        let _ = std::fs::remove_dir_all(&home);
    }

    /// Claude keeps file checkpoints in stream-json only with this set (its binary:
    /// `CLAUDE_CODE_ENABLE_SDK_FILE_CHECKPOINTING`, vetoed by `…_DISABLE_FILE_CHECKPOINTING`);
    /// the suggestion variable joins it while suggestions are on.
    #[test]
    fn claude_runs_with_file_checkpoints_on() {
        let on = (
            "CLAUDE_CODE_ENABLE_SDK_FILE_CHECKPOINTING".to_string(),
            "true".to_string(),
        );
        let suggest = (
            "CLAUDE_CODE_ENABLE_PROMPT_SUGGESTION".to_string(),
            "1".to_string(),
        );
        assert_eq!(CLAUDE.process_env(false), std::slice::from_ref(&on));
        assert_eq!(CLAUDE.process_env(true), [on, suggest]);
    }

    /// Batch 3, wave 3: × on a message Claude hasn't read asks it to drop it from its queue
    /// (`cancel_async_message`, Claude Code 2.1.330); its answer says whether it went.
    #[test]
    fn cancelling_a_queued_message_asks_claude_and_its_answer_decodes() {
        let mut t = CLAUDE.translator();
        let out = t.encode(&Action::CancelQueued { id: U.into() });
        let v: Value = serde_json::from_str(&out[0]).unwrap();
        assert_eq!(v["type"], "control_request");
        assert_eq!(
            v["request"],
            json!({"subtype":"cancel_async_message","message_uuid":U})
        );
        let rid = v["request_id"].as_str().unwrap().to_string();
        assert!(rid.starts_with("fuxx-cancel-"), "{rid}");
        let gone = json!({"type":"control_response","response":{"subtype":"success",
            "request_id":rid,"response":{"cancelled":true,"message_uuid":U}}})
        .to_string();
        assert_eq!(
            t.decode(&gone),
            [Event::QueuedCancelled {
                id: U.into(),
                cancelled: true
            }]
        );
        assert!(t.decode(&gone).is_empty(), "answered once");
        // Already taken in: not cancelled. A refusal (an older Claude) is not either.
        let out = t.encode(&Action::CancelQueued { id: U.into() });
        let rid = serde_json::from_str::<Value>(&out[0]).unwrap()["request_id"]
            .as_str()
            .unwrap()
            .to_string();
        let taken = json!({"type":"control_response","response":{"subtype":"success",
            "request_id":rid,"response":{"cancelled":false}}})
        .to_string();
        assert_eq!(
            t.decode(&taken),
            [Event::QueuedCancelled {
                id: U.into(),
                cancelled: false
            }]
        );
        let out = t.encode(&Action::CancelQueued { id: U.into() });
        let rid = serde_json::from_str::<Value>(&out[0]).unwrap()["request_id"]
            .as_str()
            .unwrap()
            .to_string();
        let refused = json!({"type":"control_response","response":{"subtype":"error",
            "request_id":rid,"error":"Unsupported control request subtype"}})
        .to_string();
        assert_eq!(
            t.decode(&refused),
            [Event::QueuedCancelled {
                id: U.into(),
                cancelled: false
            }]
        );
    }

    #[test]
    fn rewind_files_asks_claude_and_its_answer_decodes() {
        let mut t = CLAUDE.translator();
        let out = t.encode(&Action::RewindFiles {
            message: U.into(),
            dry_run: true,
        });
        let v: Value = serde_json::from_str(&out[0]).unwrap();
        assert_eq!(v["type"], "control_request");
        assert_eq!(
            v["request"],
            json!({"subtype":"rewind_files","user_message_id":U,"dry_run":true})
        );
        let rid = v["request_id"].as_str().unwrap().to_string();
        assert!(rid.starts_with("fuxx-rewind-"), "{rid}");
        let answer = json!({"type":"control_response","response":{"subtype":"success",
            "request_id":rid,"response":{"canRewind":true,"filesChanged":["src/a.rs","b.md"],
            "insertions":3,"deletions":1}}})
        .to_string();
        assert_eq!(
            t.decode(&answer),
            [Event::FilesRewound {
                message: U.into(),
                dry_run: true,
                outcome: FileRewind {
                    can_rewind: true,
                    error: None,
                    files: vec!["src/a.rs".into(), "b.md".into()],
                    insertions: Some(3),
                    deletions: Some(1),
                },
            }]
        );
        // Asked again for real: refused by Claude (an error answer).
        let out = t.encode(&Action::RewindFiles {
            message: U.into(),
            dry_run: false,
        });
        let v: Value = serde_json::from_str(&out[0]).unwrap();
        assert!(
            v["request"].get("dry_run").is_none(),
            "only a dry run says so"
        );
        let rid = v["request_id"].as_str().unwrap().to_string();
        let refused = json!({"type":"control_response","response":{"subtype":"error",
            "request_id":rid,"error":"No file checkpoint for this message"}})
        .to_string();
        let ev = t.decode(&refused);
        assert!(
            matches!(&ev[..], [Event::FilesRewound { dry_run: false, outcome, .. }]
                if !outcome.can_rewind
                    && outcome.error.as_deref() == Some("No file checkpoint for this message")),
            "{ev:?}"
        );
        // Answered once: a repeat is nobody's.
        assert!(t.decode(&refused).is_empty());
    }

    /// The process isn't running: a one-off `--rewind-files` on the conversation (it needs no
    /// prompt and exits), with checkpoints on.
    #[test]
    fn the_one_off_rewind_needs_no_prompt() {
        let probe = CLAUDE
            .rewind_files_probe(&start(Some(S), None, None), U)
            .expect("Claude can");
        assert_eq!(
            probe.argv,
            ["claude", "-p", "--resume", S, "--rewind-files", U]
        );
        assert!(probe.stdin.is_empty());
        assert_eq!(probe.env, CLAUDE.process_env(false));
        assert!(
            CLAUDE
                .rewind_files_probe(&start(None, None, None), U)
                .is_none()
        );
        assert!(
            CLAUDE
                .rewind_files_probe(&start(Some("../x"), None, None), U)
                .is_none()
        );
        assert!(
            CLAUDE
                .rewind_files_probe(&start(Some(S), None, None), "-rm")
                .is_none(),
            "only an id"
        );
    }
}
