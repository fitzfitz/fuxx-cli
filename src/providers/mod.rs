//! Agent providers (M11aa): fuxx's own event language for an agent conversation, and the
//! translators that turn one agent's wire format into it. Pure and GPUI-free, and it knows no
//! agent by name here: each agent lives in its own file.

use std::path::{Path, PathBuf};
use std::sync::Arc;

pub mod agy;
pub mod claude;

/// What fuxx shows and decides about a provider without starting it.
pub struct ProviderInfo {
    /// Matches a preset name.
    pub name: &'static str,
    pub label: &'static str,
    /// The two-letter tab badge.
    pub badge: &'static str,
    pub badge_rgb: u32,
    /// Whether a turn can be stopped by asking the agent (`Action::Interrupt`); if not, Stop
    /// offers Force stop (ending the process) at once.
    pub interrupts: bool,
    pub modes: &'static [ModeChoice],
    pub default_mode: Option<&'static str>,
}

/// One permission mode a provider offers: the id it is passed as, the name the menu shows, and
/// a line saying what it does.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ModeChoice {
    pub id: &'static str,
    pub label: &'static str,
    pub about: &'static str,
}

/// What a provider can say about itself without a conversation: its models (with their effort
/// levels), its slash commands and the mode it runs in.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Catalog {
    pub models: Vec<ModelChoice>,
    /// The provider's built-in list stood in (it gave none): the menu says "(default list)".
    pub fallback_models: bool,
    pub commands: Vec<AgentCommand>,
    /// The mode the agent says it runs in.
    pub mode: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ModelChoice {
    /// What the provider takes as the model (`--model <id>`).
    pub id: String,
    pub label: String,
    /// The full model name an alias stands for, when the provider says.
    pub resolves_to: Option<String>,
    pub efforts: Vec<EffortChoice>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EffortChoice {
    pub level: String,
    /// The model id that carries this effort, when the provider builds the effort into the id.
    pub model_id: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandKind {
    Command,
    Skill,
    Agent,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AgentCommand {
    pub name: String,
    pub about: String,
    pub kind: CommandKind,
    pub aliases: Vec<String>,
}

/// A short-lived process that answers part of the catalog: its command line, the lines to write
/// to its stdin, and (when it keeps running) the stdout line that is the answer.
#[derive(Clone, Debug)]
pub struct Probe {
    pub argv: Vec<String>,
    pub stdin: Vec<String>,
    pub until: Option<fn(&str) -> bool>,
}

/// One group of usage limits (agy names its groups; Claude's lines make one nameless group).
#[derive(Clone, Debug, PartialEq)]
pub struct UsageGroup {
    pub name: String,
    pub about: Option<String>,
    pub limits: Vec<UsageLimit>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum UsageLimit {
    /// A limit fuxx could read: how much is left, and when it resets.
    Read {
        name: String,
        left_pct: u8,
        resets: Option<Reset>,
    },
    /// A line fuxx couldn't read, kept as the agent wrote it (never turned into a number).
    Text(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Reset {
    /// Unix seconds.
    At(i64),
    /// The agent's own words (`Nov 17 at 8:09pm`).
    Words(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LimitState {
    Fine,
    Close,
    Reached,
}

/// A change in a limit's state the agent announced while it ran.
#[derive(Clone, Debug, PartialEq)]
pub struct RateLimit {
    /// The limit's name, as its usage listing shows it.
    pub limit: String,
    pub state: LimitState,
    pub resets_at: Option<i64>,
}

/// What a change in the agent box changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ChangeKind {
    Model,
    Effort,
    Mode,
    Folder,
}

/// How a provider takes a change (`Provider::applies`): in place on its running process, by
/// starting it again on the same conversation, or only in a new conversation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Applies {
    Live,
    Restart,
    /// No provider answers it today (Task 0: both carry a conversation on); the box's menus
    /// then say "applies to a new conversation".
    #[allow(dead_code)]
    NextConversation,
}

/// How to start (or resume) a conversation.
#[derive(Clone, Debug, PartialEq)]
pub struct Start {
    pub cwd: PathBuf,
    pub resume: Option<String>,
    /// A new conversation's id, chosen by fuxx (`Provider::fresh_conversation_id`): the
    /// provider starts it under that id when `resume` is None.
    pub new_id: Option<String>,
    pub mode: Option<String>,
    /// The model and effort chosen (as `Provider::model_flags` gives them); None: the agent's own.
    pub model: Option<String>,
    pub effort: Option<String>,
    pub extra_args: Vec<String>,
}

pub trait Provider: Sync {
    fn info(&self) -> &'static ProviderInfo;
    /// The full command line, program first.
    fn argv(&self, start: &Start) -> Vec<String>;
    /// The command line that opens a conversation in the agent's own terminal UI.
    fn classic_argv(&self, conversation: &str) -> Vec<String>;
    /// The name its processes run as (`procs::program_name` of their command line).
    fn program(&self) -> &'static str;
    /// The conversation a running process of this agent works on, as `ps` shows its command
    /// line (`args`): Some only when its arguments name one by an id of this agent's shape.
    /// None is not "another conversation": the arguments may not say (or may be hidden).
    fn conversation_in_args(&self, _args: &str) -> Option<String> {
        None
    }
    /// Whether `id` has the shape of this agent's conversation ids: only such an id is typed
    /// into a shell (`classic_argv`, the classic view). None is, unless the provider says.
    fn is_conversation_id(&self, _id: &str) -> bool {
        false
    }
    /// An id for a new conversation, when the agent can be told to start one under it (R13,
    /// Ruling 40: `ps` then names the conversation from its first moment, so a process left
    /// running after a crash is seen as its holder). None: the agent picks its own id.
    fn fresh_conversation_id(&self) -> Option<String> {
        None
    }
    /// The model chip's text for the model the agent reports (`model`): as it is, unless the
    /// provider knows a shorter name for it.
    fn model_label(&self, model: &str) -> String {
        model.to_string()
    }
    /// Whether catalog model `id` stands for "the agent's own default" rather than naming a
    /// model (the chip then names the model it stands for by another entry).
    fn stands_for_default(&self, _id: &str) -> bool {
        false
    }
    /// What was said before in a resumed conversation (empty unless the provider can read it).
    /// Reads a file: call it off the UI thread.
    fn history(&self, _start: &Start) -> Vec<Past> {
        Vec::new()
    }
    fn translator(&self) -> Box<dyn Translator>;
    /// Whether the agent takes an image in the message itself (`Action::Send`'s `images`); if
    /// not, an image goes as its path in the text.
    fn sends_images(&self) -> bool {
        false
    }
    /// The short processes that answer the catalog, in the order `read_catalog` takes their
    /// outputs. None: the provider can't be asked.
    fn catalog_probes(&self) -> Vec<Probe> {
        Vec::new()
    }
    /// Its own process's start answers the catalog (`Event::Catalog`), so no probe runs while
    /// it runs.
    fn catalog_on_start(&self) -> bool {
        false
    }
    /// The catalog from the probes' outputs (stdout of each, in order).
    fn read_catalog(&self, _outputs: &[String]) -> Result<Catalog, String> {
        Err("no list".into())
    }
    /// The models to offer when the provider gave none.
    fn fallback_models(&self) -> Vec<ModelChoice> {
        Vec::new()
    }
    /// A model choice and an effort level to the `(model, effort)` values to start with
    /// (`Start::model`, `Start::effort`). Default: the model's id and the effort as chosen.
    fn model_flags(&self, model: &ModelChoice, effort: Option<&str>) -> (String, Option<String>) {
        (model.id.clone(), effort.map(String::from))
    }
    /// The short process that prints the agent's usage limits ($0, outside any conversation).
    /// None: the agent has no usage command.
    fn usage_probe(&self) -> Option<Probe> {
        None
    }
    /// The usage limits from the probe's stdout; Err is the line to show.
    fn read_usage(&self, _output: &str) -> Result<Vec<UsageGroup>, String> {
        Err("Usage isn't available".into())
    }
    /// Whether what a resumed run said on its way out (its stderr tail, or an error row) means
    /// the conversation to resume doesn't exist: only then does the tab start a new one.
    fn unknown_conversation(&self, _said: &str) -> bool {
        false
    }
    /// How a change of `change` reaches a running conversation. Default: by a restart on the
    /// same conversation (the new flags).
    fn applies(&self, _change: ChangeKind) -> Applies {
        Applies::Restart
    }
    /// The short process that prints the agent's version (read once per app run, when the
    /// folder picker first opens). None: its version doesn't matter.
    fn version_probe(&self) -> Option<Probe> {
        None
    }
    /// Whether a conversation can be moved to another folder (restarted there on the same
    /// conversation), the agent's `version` as its probe printed it. Default: no (only a new
    /// conversation in that folder).
    fn can_move(&self, _version: Option<&str>) -> bool {
        false
    }
    /// The folders the agent itself knows (its own list of projects), under your `home`.
    /// Reads files: call it off the UI thread. Default: none.
    fn known_folders(&self, _home: &Path) -> Vec<PathBuf> {
        Vec::new()
    }
}

pub trait Translator: Send {
    /// Lines written before the first message.
    fn hello(&mut self) -> Vec<String>;
    /// One stdout line to zero or more events.
    fn decode(&mut self, line: &str) -> Vec<Event>;
    /// One action to the lines to write.
    fn encode(&mut self, action: &Action) -> Vec<String>;
}

/// One piece of a resumed conversation's history: something you said, or an event of the agent's.
#[derive(Clone, Debug, PartialEq)]
pub enum Past {
    You(String),
    Event(Event),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Started {
        conversation: String,
        model: Option<String>,
    },
    TextDelta {
        message: String,
        text: String,
        /// The tool call of the sub-agent that said it; `None` for the main agent.
        parent: Option<String>,
    },
    MessageDone {
        message: String,
        text: String,
        parent: Option<String>,
    },
    ToolStarted {
        id: String,
        name: String,
        summary: String,
        parent: Option<String>,
        detail: Option<ToolDetail>,
        /// The files the call writes, as the provider read them (absolute, or relative to the
        /// agent's cwd); empty for a call that writes none.
        writes: Vec<String>,
    },
    ToolFinished {
        id: String,
        ok: bool,
        summary: String,
        output: Option<String>,
    },
    Todo(Vec<TodoItem>),
    Permission {
        id: String,
        tool: String,
        what: String,
        detail: String,
        always: Option<String>,
    },
    Usage {
        context_pct: Option<u8>,
        cost_usd: Option<f64>,
    },
    TurnDone {
        ok: bool,
        message: Option<String>,
    },
    Notice(String),
    Error(String),
    /// The provider's own start answered the catalog.
    Catalog(Catalog),
    /// The names of the skills the agent has (the cache marks those commands `Skill`).
    Skills(Vec<String>),
    /// A usage limit changed state.
    RateLimit(RateLimit),
    /// The agent answered a live change (`Action::SetModel` …): `ok` with the value it now
    /// uses, else not, `value` saying why.
    Applied {
        kind: ChangeKind,
        value: String,
        ok: bool,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum ToolDetail {
    Diff { path: String, unified: String },
    Command(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct TodoItem {
    pub text: String,
    pub state: TodoState,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TodoState {
    Done,
    Active,
    Pending,
}

/// An image to send with a message: its type (`image/png`), its bytes and the same as base64
/// (made once by `new`, which is for a background thread: encoding up to 5 MB is not for the
/// UI thread, so a translator only copies `data`).
#[derive(Clone, Debug, PartialEq)]
pub struct Picture {
    pub media_type: &'static str,
    pub bytes: Arc<[u8]>,
    pub data: Arc<str>,
}

impl Picture {
    pub fn new(media_type: &'static str, bytes: Vec<u8>) -> Self {
        let data = Arc::from(base64(&bytes));
        Picture {
            media_type,
            bytes: Arc::from(bytes),
            data,
        }
    }
}

/// `bytes` as standard base64 (RFC 4648, padded).
pub fn base64(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, b)| n | u32::from(*b) << (16 - 8 * i));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(DIGITS[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    /// A message; `images` go with it only for a provider that `sends_images` (the others get
    /// each image's path in `text`).
    Send {
        text: String,
        images: Vec<Picture>,
    },
    Allow {
        id: String,
    },
    AlwaysAllow {
        id: String,
    },
    Deny {
        id: String,
        reason: Option<String>,
    },
    Interrupt,
    /// Live changes (`Applies::Live`); a provider that can't take one writes nothing.
    SetModel {
        model: String,
    },
    SetEffort {
        effort: String,
    },
    SetMode {
        mode: String,
    },
}

/// `extra` without the flags a provider sets itself (`owned`: each flag, and whether it takes a
/// value): a word naming one is dropped, with its value.
fn pass_through(extra: &[String], owned: &[(&str, bool)]) -> Vec<String> {
    let mut out = Vec::new();
    let mut skip_value = false;
    for arg in extra {
        if std::mem::take(&mut skip_value) {
            continue;
        }
        let name = arg.split_once('=').map_or(arg.as_str(), |(n, _)| n);
        match owned.iter().find(|(f, _)| *f == name) {
            // `--flag=value` carries its value; `--flag value` borrows the next word.
            Some((_, takes_value)) => skip_value = *takes_value && name == arg,
            None => out.push(arg.clone()),
        }
    }
    out
}

/// Whether `word` has a UUID's shape (8-4-4-4-12 hex digits), as conversation ids have.
fn uuid_shaped(word: &str) -> bool {
    let groups: Vec<&str> = word.split('-').collect();
    groups.len() == 5
        && groups
            .iter()
            .zip([8, 4, 4, 4, 12])
            .all(|(g, n)| g.len() == n && g.chars().all(|c| c.is_ascii_hexdigit()))
}

/// A random (version 4) UUID, lower-case, from the system's random source; None if it can't
/// be read.
pub(crate) fn new_uuid() -> Option<String> {
    use std::io::Read;
    let mut b = [0u8; 16];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut b))
        .ok()?;
    b[6] = (b[6] & 0x0f) | 0x40; // version 4
    b[8] = (b[8] & 0x3f) | 0x80; // the RFC 4122 variant
    let hex: String = b.iter().map(|x| format!("{x:02x}")).collect();
    Some(format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    ))
}

/// The conversation a command line (`ps`'s `args`) names after one of `flags` (`--flag <id>` or
/// `--flag=<id>`), the id of the provider's shape (`is_id`); any other word after one (a word of
/// a prompt `ps` joined in, a search) names nothing, and the search goes on.
fn conversation_after(args: &str, flags: &[&str], is_id: fn(&str) -> bool) -> Option<String> {
    let mut words = args.split_whitespace().peekable();
    while let Some(word) = words.next() {
        let value = match word.split_once('=') {
            Some((flag, value)) if flags.contains(&flag) => Some(value),
            _ if flags.contains(&word) => words.peek().copied(),
            _ => None,
        };
        if let Some(id) = value.filter(|v| is_id(v)) {
            return Some(id.to_string());
        }
    }
    None
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

/// Every provider fuxx can run an agent tab with.
pub fn all() -> &'static [&'static dyn Provider] {
    static ALL: [&dyn Provider; 2] = [&claude::CLAUDE, &agy::AGY];
    &ALL
}

pub fn by_name(name: &str) -> Option<&'static dyn Provider> {
    all().iter().copied().find(|p| p.info().name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A provider that overrides nothing it needn't.
    struct Plain;
    struct Silent;
    static PLAIN_INFO: ProviderInfo = ProviderInfo {
        name: "plain",
        label: "Plain",
        badge: "PL",
        badge_rgb: 0,
        interrupts: false,
        modes: &[],
        default_mode: None,
    };
    impl Translator for Silent {
        fn hello(&mut self) -> Vec<String> {
            Vec::new()
        }
        fn decode(&mut self, _line: &str) -> Vec<Event> {
            Vec::new()
        }
        fn encode(&mut self, _action: &Action) -> Vec<String> {
            Vec::new()
        }
    }
    impl Provider for Plain {
        fn info(&self) -> &'static ProviderInfo {
            &PLAIN_INFO
        }
        fn argv(&self, _start: &Start) -> Vec<String> {
            Vec::new()
        }
        fn classic_argv(&self, _conversation: &str) -> Vec<String> {
            Vec::new()
        }
        fn program(&self) -> &'static str {
            "plain"
        }
        fn translator(&self) -> Box<dyn Translator> {
            Box::new(Silent)
        }
    }

    #[test]
    fn a_conversation_is_named_by_the_providers_id_shape() {
        let args = "agent --id abc --id 6a1c234d-7469-40ef-b093-c74ad8006c28";
        assert_eq!(
            conversation_after(args, &["--id"], uuid_shaped).as_deref(),
            Some("6a1c234d-7469-40ef-b093-c74ad8006c28")
        );
        assert_eq!(
            conversation_after(args, &["--id"], |w| w == "abc").as_deref(),
            Some("abc")
        );
        assert_eq!(conversation_after(args, &["--other"], uuid_shaped), None);
    }

    #[test]
    fn a_new_id_is_a_random_v4_uuid() {
        let a = new_uuid().expect("16 random bytes");
        let b = new_uuid().expect("16 random bytes");
        assert!(uuid_shaped(&a), "{a}");
        assert_ne!(a, b);
        assert_eq!(&a[14..15], "4", "version 4: {a}");
        assert!("89ab".contains(&a[19..20]), "the RFC 4122 variant: {a}");
        assert_eq!(a, a.to_lowercase());
    }

    /// A provider that says nothing of its ids: no id is chosen for it, and none of its ids is
    /// typed into a shell (fail closed).
    #[test]
    fn a_provider_s_ids_are_its_own_unless_it_says() {
        assert_eq!(Plain.fresh_conversation_id(), None);
        assert!(!Plain.is_conversation_id("6a1c234d-7469-40ef-b093-c74ad8006c28"));
    }

    #[test]
    fn a_model_is_labelled_as_it_is_unless_the_provider_knows_better() {
        assert_eq!(Plain.model_label("claude-opus-5-5"), "claude-opus-5-5");
        assert_eq!(Plain.model_label("gemini-3"), "gemini-3");
    }

    #[test]
    fn by_name_finds_claude_and_agy_and_nothing_else() {
        assert_eq!(by_name("claude").map(|p| p.info().name), Some("claude"));
        assert_eq!(by_name("agy").map(|p| p.info().label), Some("Antigravity"));
        assert!(by_name("opencode").is_none());
    }
}
