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
    /// How a sidebar row's line two names it, glyph then name ("✳ Claude", M11al §2.4).
    pub who: (&'static str, &'static str),
    pub badge_rgb: u32,
    /// Its logo, drawn in place of the badge (spec B-A); none keeps the letters.
    pub logo: Option<crate::logos::Logo>,
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

/// A command the box answers with its own menu instead of sending it (`Provider::own_command`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoxCommand {
    Usage,
    Model,
    Effort,
}

/// A short-lived process that answers part of the catalog: its command line, the lines to write
/// to its stdin, (when it keeps running) the stdout line that is the answer, and the environment
/// it runs with besides the login one (none for most).
#[derive(Clone, Debug)]
pub struct Probe {
    pub argv: Vec<String>,
    pub stdin: Vec<String>,
    pub until: Option<fn(&str) -> bool>,
    pub env: Vec<(String, String)>,
}

/// What a provider can do to go back in a conversation (batch 3, chore 2): cut it before one of
/// your messages in place (`rewind`), start a new conversation from that point (`fork`), and
/// undo the file changes made after it (`files`, through the agent's own checkpoints).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[allow(dead_code)] // Batch 3 wave 2 (the Rewind/Fork UI) calls it.
pub struct Branching {
    pub rewind: bool,
    pub fork: bool,
    pub files: bool,
}

impl Branching {
    /// Whether it can go back at all.
    #[allow(dead_code)] // Batch 3 wave 2 (the Rewind/Fork UI) calls it.
    pub fn any(self) -> bool {
        self.rewind || self.fork
    }
}

/// Where a new conversation branches off an old one (`Start::branch`): conversation `from`, kept
/// up to and including chain entry `at` (`BranchPoint::parent`); `at` None (the first message
/// cut): nothing kept, a fresh conversation. The new one starts under `Start::new_id`.
#[derive(Clone, Debug, PartialEq)]
pub struct Branch {
    pub from: String,
    pub at: Option<String>,
}

/// One of your messages in its conversation's record: its id, and the entry before it (where a
/// branch that cuts the message resumes; None for the first).
#[derive(Clone, Debug, PartialEq)]
#[allow(dead_code)] // Batch 3 wave 2 (the Rewind/Fork UI) calls it.
pub struct BranchPoint {
    pub message: String,
    pub parent: Option<String>,
}

/// The agent's answer to `Action::RewindFiles` (Claude's `rewind_files`): whether it can (or, not
/// a dry run, did) put the files back as they were before the message, why not, and what it
/// changes. `files` empty when it doesn't say.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FileRewind {
    pub can_rewind: bool,
    pub error: Option<String>,
    pub files: Vec<String>,
    pub insertions: Option<u64>,
    pub deletions: Option<u64>,
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
    /// How much of it is used, when the agent says (Claude's stream does: M11ap, live usage).
    pub used_pct: Option<u8>,
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
    /// Start a new conversation branched off an old one (batch 3, chore 2: Rewind and Fork),
    /// under `new_id`; it wins over `resume`. None: as `resume` and `new_id` say.
    pub branch: Option<Branch>,
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
    /// The biggest image it takes: by default 5 MB of the file (an agent told the image's path).
    fn image_limit(&self) -> ImageLimit {
        ImageLimit::Raw(5 * 1024 * 1024)
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
    /// The command `typed` (the box's whole text) names, when fuxx answers it with its own menu
    /// rather than sending it. None: it is sent to the agent as text.
    fn own_command(&self, _typed: &str) -> Option<BoxCommand> {
        None
    }
    /// The command that asks the agent, once, for a commit message from `prompt` (M11aq's ✦),
    /// its answer on stdout as text; none for an agent that can't.
    fn message_probe(&self, _prompt: &str) -> Option<Probe> {
        None
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
    /// The folders the agent itself knows (its own list of projects), under your `home`.
    /// Reads files: call it off the UI thread. Default: none.
    fn known_folders(&self, _home: &Path) -> Vec<PathBuf> {
        Vec::new()
    }
    /// The flags that ask the agent to guess your next message after each turn
    /// (`Event::Suggestion`), added while fuxx's "Prompt suggestions" is on. Default: none (the
    /// agent can't).
    fn suggestion_flags(&self) -> Vec<String> {
        Vec::new()
    }
    /// The environment set on the agent's own process (besides the login one): what it always
    /// needs (batch 3, chore 2: Claude's file checkpoints), and with `suggestions` (fuxx's
    /// "Prompt suggestions" on) what its guesses need. Default: none.
    fn process_env(&self, _suggestions: bool) -> Vec<(String, String)> {
        Vec::new()
    }
    /// What it can do to go back in a conversation (batch 3, chore 2). Default: nothing.
    #[allow(dead_code)] // Batch 3 wave 2 (the Rewind/Fork UI) calls it.
    fn branching(&self) -> Branching {
        Branching::default()
    }
    /// An id for a message about to be sent (`Action::Send`'s `id`), when the agent takes one and
    /// names the message by it after (its echo, `Event::Taken`; its record, `Past::You`). None:
    /// the agent names messages its own way, or not at all.
    fn fresh_message_id(&self) -> Option<String> {
        None
    }
    /// Where message `message` sits in the conversation `start` resumes (`Start::resume`), under
    /// your `home`: for a `Branch` that cuts it. Reads a file: call it off the UI thread. Err: the
    /// line to show. Default: it can't.
    #[allow(dead_code)] // Batch 3 wave 2 (the Rewind/Fork UI) calls it.
    fn branch_point(
        &self,
        _home: &Path,
        _start: &Start,
        _message: &str,
    ) -> Result<BranchPoint, String> {
        Err("This agent can't go back in a conversation".into())
    }
    /// A one-off process that undoes the file changes made since message `message` in the
    /// conversation `start` resumes, for when its own process isn't running (the live way is
    /// `Action::RewindFiles`). None: it can't (or `start` names no conversation of its shape).
    #[allow(dead_code)] // Batch 3 wave 2 (the Rewind/Fork UI) calls it.
    fn rewind_files_probe(&self, _start: &Start, _message: &str) -> Option<Probe> {
        None
    }
    /// Whether it can take back a message of yours it hasn't read yet (batch 3, wave 3:
    /// `Action::CancelQueued`, answered by `Event::QueuedCancelled`). Default: it can't (a
    /// message fuxx still holds can always be dropped).
    fn cancels_queued(&self) -> bool {
        false
    }
    /// The Edit hover action's tooltip on your message, for an agent that can't go back from
    /// fuxx (batch 3, chore 2): where its own rewind is. None: the generic words.
    fn edit_tip(&self) -> Option<&'static str> {
        None
    }
    /// Whether Main can pass your message to a running subagent (`Action::Relay`; the lanes
    /// chore). Default: it can't (the box in a lane talks to Main).
    fn relays_to_subagents(&self) -> bool {
        false
    }
    /// Whether a subagent's `kind` names nothing in particular (the subagent row chore: its
    /// row's tooltip then says just "Subagent"). Default: only an empty one.
    fn generic_lane_kind(&self, kind: &str) -> bool {
        kind.trim().is_empty()
    }
    /// Whether the agent's questions (`Event::Question`) are answered by an ordinary message
    /// rather than `Action::Answer`: it asked, didn't wait, and went on (agy's print mode skips
    /// its own `ask_question`; 2026-11-24). The card then sends your picks as a message, the tab
    /// doesn't wait on it, and Skip only closes it. Default: it waits for `Action::Answer`.
    fn questions_by_message(&self) -> bool {
        false
    }
    /// The card's footer line in that mode: why the agent went on, in its own words. None: no
    /// line.
    fn question_note(&self) -> Option<&'static str> {
        None
    }
}

pub trait Translator: Send {
    /// Lines written before the first message.
    fn hello(&mut self) -> Vec<String>;
    /// One stdout line to zero or more events.
    fn decode(&mut self, line: &str) -> Vec<Event>;
    /// One action to the lines to write.
    fn encode(&mut self, action: &Action) -> Vec<String>;
    /// One line of a file the translator asked to have followed (`Event::Follow`) for subagent
    /// `lane`, to events (M11ar: agy's subagent transcripts). None by default.
    fn decode_followed(&mut self, _lane: &str, _line: &str) -> Vec<Event> {
        Vec::new()
    }
    /// One stderr line of the agent's process, to events (agy says there that it refused a
    /// mode: `Event::ModeRefused`). None by default.
    fn decode_stderr(&mut self, _line: &str) -> Vec<Event> {
        Vec::new()
    }
}

/// What the host can do to one subagent, and how its agent tells of it, as the agent says when
/// it starts (M11ar; the lanes chore, 2026-11-24).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LaneControls {
    /// Stop this subagent alone (`Action::StopSubagent`).
    pub stop: bool,
    /// A message can reach it while it runs (`Action::Relay`, through Main): Claude's
    /// background subagents. A foreground one runs inside Main's step: Main can't pass a note
    /// on until it ends.
    pub message: bool,
    /// A step under it after its end means it works again (agy's woken subagents, whose
    /// transcript is the only sign). Claude says a resume itself (`Event::SubagentResumed`):
    /// a late step never reopens its lane.
    pub woken_by_steps: bool,
}

/// How a subagent ended.
#[derive(Clone, Debug, PartialEq)]
pub enum LaneEnd {
    Done,
    Failed(String),
    Stopped,
}

/// One piece of a resumed conversation's history: something you said (with its id in the
/// agent's record, when it has one: batch 3, chore 2), or an event of the agent's.
#[derive(Clone, Debug, PartialEq)]
pub enum Past {
    You {
        text: String,
        id: Option<String>,
    },
    /// A message of yours fuxx asked Main to pass to subagent `to` (a lane), or (`about`) to
    /// answer about it; `text` is your words, never the frame round them (the lanes chore).
    Relay {
        text: String,
        id: Option<String>,
        to: String,
        about: bool,
    },
    Event(Event),
}

#[cfg(any(test, feature = "ui-shots"))]
impl Past {
    /// Something you said, with no id.
    pub fn you(text: impl Into<String>) -> Past {
        Past::You {
            text: text.into(),
            id: None,
        }
    }
}

/// The new text of one edit, as the provider read it from the call (M11ag).
#[derive(Clone, Debug, PartialEq)]
pub enum EditText {
    /// Text the edit put in the file.
    Inserted(String),
    /// The call replaced the whole file.
    WholeFile,
    /// The edit only removed text (its new text is empty).
    Removed,
    /// The call writes its file, and the provider says there is no note for it (a notebook
    /// edit, an edit with no new text given). Not the same as no texts at all: that is an agent
    /// that gives none, whose edits get their notes the light way.
    NoNote,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// The agent cleared its conversation (Claude's `/clear`): what was said goes; a `Started`
    /// with the new id follows.
    Cleared,
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
        /// What the call put in those files, for finding its lines (M11ag); empty for a call
        /// with no text to hand over.
        texts: Vec<EditText>,
    },
    ToolFinished {
        id: String,
        ok: bool,
        summary: String,
        output: Option<String>,
    },
    /// What tool call `id` does (the agent activity chore): sent right after its `ToolStarted`
    /// (the call as planned) and right after its `ToolFinished` (the final one); history
    /// replays the same pairs.
    Activity {
        id: String,
        activity: crate::agent_activity::Activity,
    },
    Todo(Vec<TodoItem>),
    Permission {
        id: String,
        tool: String,
        what: String,
        detail: String,
        always: Option<String>,
        /// The subagent (lane id) that asks; `None`: the main agent.
        by: Option<String>,
        /// The agent's plan, when this asks to leave plan mode with it (the 2026-11-25 chores:
        /// Claude's `ExitPlanMode`): its card shows the plan, and `AlwaysAllow` approves it
        /// with edits accepted from then on.
        plan: Option<PlanDraft>,
    },
    /// The agent asks you to choose (batch 3, chore 8: Claude's `AskUserQuestion`), answered by
    /// `Action::Answer` (or skipped by `Action::Deny`). `id`: the tool call's, whose own
    /// `ToolFinished` closes it. Not a tool row.
    Question {
        id: String,
        questions: Vec<Question>,
        /// The subagent (lane id) that asks; `None`: the main agent.
        by: Option<String>,
    },
    /// The agent stopped waiting on its ask `id` (a permission's or a question's, as the card
    /// is keyed) before you answered: Claude's `control_cancel_request` ("Permission request
    /// withdrawn unanswered", a Stop or the session ending). The card settles at once.
    AskWithdrawn {
        id: String,
    },
    /// A past question's reply as its result says it (the design review of batch 3, chore 8):
    /// your answers, one per question in order (`None`: you skipped it). Restored history only;
    /// a question closed already keeps its reply.
    QuestionReplied {
        id: String,
        answers: Option<Vec<QAnswer>>,
    },
    /// A subagent started (M11ar), its steps hanging under the tool call `id` (their `parent`);
    /// `parent`: the lane that started it, when nested.
    SubagentStarted {
        id: String,
        parent: Option<String>,
        label: String,
        kind: String,
        task: String,
        model: Option<String>,
        controls: LaneControls,
    },
    /// What a subagent has done so far, as far as its agent says.
    SubagentProgress {
        id: String,
        tokens: Option<u64>,
        tools: Option<u32>,
        last_tool: Option<String>,
    },
    SubagentEnded {
        id: String,
        end: LaneEnd,
    },
    /// Subagent `id` (a lane that started before) runs again (the lanes chore: Claude re-sends
    /// `task_started` for the same task when Main's SendMessage resumes it, or it wakes):
    /// its lane goes on, with `controls` as they are now and what Main `said` to it.
    SubagentResumed {
        id: String,
        controls: LaneControls,
        said: Option<String>,
    },
    /// Main's tool call `call` sent subagent `lane` a message (Claude's SendMessage): a
    /// relay to it is delivered.
    PassedOn {
        call: String,
        lane: String,
    },
    /// The agent read one of your messages (Claude's `--replay-user-messages` echo): in the
    /// turn running, or starting its own. `id`: the message's (`Action::Send`'s), when it says.
    Taken {
        id: Option<String>,
    },
    /// The answer to `Action::CancelQueued` for message `id` (batch 3, wave 3): whether it
    /// went (false: the agent had taken it in already, or refused).
    QueuedCancelled {
        id: String,
        cancelled: bool,
    },
    /// The answer to `Action::RewindFiles` for message `message` (batch 3, chore 2).
    FilesRewound {
        message: String,
        dry_run: bool,
        outcome: FileRewind,
    },
    /// Read this file's new lines as subagent `id`'s, through the translator's
    /// `decode_followed` (agy writes a subagent's steps to its own transcript). `main`: the
    /// main conversation's own record (agy's questions are only there), read as often as a
    /// working lane's; `id` then names no lane.
    Follow {
        id: String,
        path: PathBuf,
        main: bool,
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
    /// The agent's guess at your next message, after a turn of the main conversation (Claude's
    /// `prompt_suggestion`); never empty.
    Suggestion(String),
    /// The names of the skills the agent has (the cache marks those commands `Skill`).
    Skills(Vec<String>),
    /// A usage limit changed state.
    RateLimit(RateLimit),
    /// The permission mode the agent says it runs in now, also one it changed to itself (a
    /// plan approved leaves plan mode): the tab follows it unless a change of yours is pending.
    Mode(String),
    /// The agent refused mode `asked` for this account and runs in `running` instead (agy:
    /// "--mode auto is not available … continuing in the default mode"): the tab follows
    /// `running`, says so once, and new tabs of this agent start in `running` from then on.
    ModeRefused {
        asked: String,
        running: String,
    },
    /// The agent answered a live change (`Action::SetModel` …): `ok` with the value it now
    /// uses, else not, `value` saying why.
    Applied {
        kind: ChangeKind,
        value: String,
        ok: bool,
    },
}

/// One question of an `Event::Question`: its chip (`header`, maybe empty), its words (`text`,
/// kept exactly: the answer is keyed by them), its options, and whether several may be picked.
#[derive(Clone, Debug, PartialEq)]
pub struct Question {
    pub header: String,
    pub text: String,
    pub options: Vec<QOption>,
    pub multi: bool,
}

/// One option of a `Question`: its label, a line saying what it means (maybe empty), and what
/// it would look like (`preview`, shown in a monospace well; single-select only).
#[derive(Clone, Debug, PartialEq)]
pub struct QOption {
    pub label: String,
    pub description: String,
    pub preview: Option<String>,
}

/// Your answer to one `Question`: the labels picked (in the options' order) and any words of
/// your own ("Other"). Both empty: not answered (left out of what is sent).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct QAnswer {
    pub labels: Vec<String>,
    pub other: Option<String>,
}

impl QAnswer {
    /// Nothing picked and no words of your own.
    pub fn is_empty(&self) -> bool {
        self.labels.is_empty() && self.other.as_deref().is_none_or(|o| o.trim().is_empty())
    }
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
    /// The marker the message's text names it by (`[Image #1]`, a pasted image), sent just
    /// before it so the agent knows which picture the text means; none for a file's.
    pub label: Option<Arc<str>>,
}

impl Picture {
    pub fn new(media_type: &'static str, bytes: Vec<u8>) -> Self {
        let data = Arc::from(base64(&bytes));
        Picture {
            media_type,
            bytes: Arc::from(bytes),
            data,
            label: None,
        }
    }
}

/// The biggest image a provider takes, in bytes: of the file itself, or of its base64 (an
/// image sent in the message, whose limit is on the encoded size).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ImageLimit {
    Raw(usize),
    Encoded(usize),
}

impl ImageLimit {
    /// The biggest file that fits: base64 is 4 digits for every 3 bytes (or part of 3).
    pub fn raw_max(self) -> usize {
        match self {
            ImageLimit::Raw(max) => max,
            ImageLimit::Encoded(max) => max / 4 * 3,
        }
    }

    /// Whether an image of `len` bytes is taken (checked before it is encoded).
    pub fn fits(self, len: usize) -> bool {
        match self {
            ImageLimit::Raw(max) => len <= max,
            ImageLimit::Encoded(max) => len.div_ceil(3) * 4 <= max,
        }
    }

    /// The notice for image `name` that is too big for the agent called `label`: not attached.
    pub fn refusal(self, label: &str, name: &str) -> String {
        match self {
            ImageLimit::Raw(max) => format!("Image over {} MB: {name}", megabytes(max)),
            ImageLimit::Encoded(_) => format!(
                "Image too big for {label} ({} MB max): {name}",
                megabytes(self.raw_max())
            ),
        }
    }
}

/// `bytes` in MB (MiB), as few digits as it needs: `5`, `3.75`.
fn megabytes(bytes: usize) -> String {
    format!("{}", bytes as f64 / (1024.0 * 1024.0))
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

/// A plan the agent asks you to approve (the 2026-11-25 chores): its title (`plan_title`), its
/// markdown, and the file it was written to.
#[derive(Clone, Debug, PartialEq)]
pub struct PlanDraft {
    pub title: String,
    pub markdown: String,
    pub file: Option<std::path::PathBuf>,
}

/// A plan's title: its first `# ` heading, a leading "Plan:" taken off; else "Plan".
pub fn plan_title(markdown: &str) -> String {
    markdown
        .lines()
        .find_map(|l| l.strip_prefix("# "))
        .map(|t| {
            let t = t.trim();
            t.strip_prefix("Plan:").map_or(t, str::trim).to_string()
        })
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| "Plan".into())
}

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    /// A message; `images` go with it only for a provider that `sends_images` (the others get
    /// each image's path in `text`); `id` its id, from `Provider::fresh_message_id` (an agent
    /// that takes none ignores it).
    Send {
        text: String,
        images: Vec<Picture>,
        id: Option<String>,
    },
    Allow {
        id: String,
    },
    AlwaysAllow {
        id: String,
    },
    /// Your answers to `Event::Question` `id`, one per question in its order (batch 3, chore 8).
    /// Skipping it is `Deny`. A provider that asked no such question writes nothing.
    Answer {
        id: String,
        answers: Vec<QAnswer>,
    },
    Deny {
        id: String,
        reason: Option<String>,
    },
    Interrupt,
    /// Stop subagent `lane` alone (a lane whose `controls.stop` is on).
    StopSubagent {
        lane: String,
    },
    /// Your `text` for subagent `lane` (called `label`), sent to Main to pass on word for word
    /// (a provider that `relays_to_subagents`; the lanes chore): a message to Main in every
    /// other way (`id` as `Send`'s). Only the translator knows the lane's agent id.
    Relay {
        lane: String,
        label: String,
        text: String,
        id: Option<String>,
    },
    /// Your `text` to Main about subagent `lane` (called `label`), which it must not resume:
    /// finished, or (`live`) working inside Main's step.
    About {
        lane: String,
        label: String,
        text: String,
        id: Option<String>,
        live: bool,
    },
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
    /// Put the files back as they were before message `message` (`dry_run`: only say what would
    /// change); answered by `Event::FilesRewound`. A provider that can't writes nothing.
    #[allow(dead_code)] // Batch 3 wave 2 (the Rewind/Fork UI) calls it.
    RewindFiles {
        message: String,
        dry_run: bool,
    },
    /// Take back message `id`, sent but not read yet (batch 3, wave 3; a provider that
    /// `cancels_queued`); answered by `Event::QueuedCancelled`.
    CancelQueued {
        id: String,
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

/// The prompt for a commit message from the staged `diff` (M11aq's ✦): one imperative summary
/// line, the reply nothing but that line; what was left out (secret-looking files) or cut said.
pub fn commit_message_prompt(diff: &crate::git_commit::StagedDiff) -> String {
    let mut prompt = String::from(
        "Write a git commit message for the staged changes below. Reply with only the message: \
         one summary line in the imperative mood, at most 72 characters, no quotes, no code \
         fences, no explanation.\n",
    );
    if !diff.skipped.is_empty() {
        prompt.push_str(&format!(
            "(Also staged but not shown, as they may hold secrets: {}.)\n",
            diff.skipped.join(", ")
        ));
    }
    if diff.cut {
        prompt.push_str("(The diff was cut to fit; summarise what is shown.)\n");
    }
    prompt.push('\n');
    prompt.push_str(&diff.text);
    prompt
}

/// The message in an agent's answer: its first line with text, out of any code fence, without
/// quotes or a "Commit message:" label; none when nothing is left.
pub fn clean_message(raw: &str) -> Option<String> {
    let line = raw
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.starts_with("```"))?;
    let line = line
        .strip_prefix("Commit message:")
        .or_else(|| line.strip_prefix("commit message:"))
        .unwrap_or(line)
        .trim();
    let line = line
        .trim_matches(|c| c == '"' || c == '\'' || c == '`')
        .trim();
    (!line.is_empty()).then(|| line.to_string())
}

/// Whether `command` is a program on `path_env` (the login PATH; none: this process's).
pub fn on_path(command: &str, path_env: Option<&str>) -> bool {
    let path = match path_env {
        Some(p) => p.to_string(),
        None => std::env::var("PATH").unwrap_or_default(),
    };
    std::env::split_paths(&path)
        .filter(|dir| !dir.as_os_str().is_empty())
        .any(|dir| dir.join(command).is_file())
}

/// M11aq: the commit message an agent is asked for (✦), and what fuxx keeps of its answer.
#[cfg(test)]
mod commit_message_tests {
    use super::*;
    use crate::git_commit::StagedDiff;

    #[test]
    fn each_agent_writes_a_message_once_in_text() {
        let claude = claude::CLAUDE
            .message_probe("say it")
            .expect("claude writes");
        assert_eq!(
            claude.argv,
            [
                "claude",
                "-p",
                "say it",
                "--output-format",
                "text",
                "--model",
                "haiku",
                "--no-session-persistence",
                "--tools",
                ""
            ],
            "no tools: it only writes (the review: it never commits)"
        );
        let agy = agy::AGY.message_probe("say it").expect("agy writes");
        assert_eq!(agy.argv, ["agy", "-p", "say it", "--output-format", "text"]);
    }

    #[test]
    fn the_prompt_carries_the_diff_and_says_what_was_left_out() {
        let diff = StagedDiff {
            text: "+changed\n".into(),
            skipped: vec![".env".into()],
            cut: true,
        };
        let prompt = commit_message_prompt(&diff);
        assert!(prompt.contains("+changed"), "{prompt}");
        assert!(prompt.contains(".env"), "names what was left out: {prompt}");
        assert!(prompt.contains("72"), "one short line: {prompt}");
        assert!(
            prompt.to_lowercase().contains("cut"),
            "says it was cut: {prompt}"
        );
    }

    #[test]
    fn only_the_message_line_is_kept() {
        assert_eq!(
            clean_message("Add commit field\n").as_deref(),
            Some("Add commit field")
        );
        assert_eq!(
            clean_message("```\nFix the bar\n```").as_deref(),
            Some("Fix the bar"),
            "out of a fence"
        );
        assert_eq!(
            clean_message("\"Fix it\"").as_deref(),
            Some("Fix it"),
            "unquoted"
        );
        assert_eq!(
            clean_message("Commit message: Fix it").as_deref(),
            Some("Fix it"),
            "no label"
        );
        assert_eq!(
            clean_message("\n\nFirst line\n\nA body").as_deref(),
            Some("First line"),
            "the first line"
        );
        assert_eq!(clean_message("  \n```\n```\n"), None, "nothing");
    }

    #[test]
    fn no_fixture_names_a_real_home() {
        // Recordings are scrubbed (`scripts/record-agent-fixture.py`): a home folder shows as
        // `~` or `/Users/me`, never someone's.
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/providers/fixtures");
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            let text = std::fs::read_to_string(&path).unwrap_or_default();
            for (i, _) in text.match_indices("/Users/") {
                let rest = &text[i + "/Users/".len()..];
                assert!(
                    rest.starts_with("me/") || rest.starts_with("me\"") || rest.starts_with("me "),
                    "{}: a real home folder at byte {i}",
                    path.display()
                );
            }
        }
    }

    #[test]
    fn a_program_is_found_on_the_path() {
        assert!(on_path("sh", Some("/usr/bin:/bin")));
        assert!(!on_path("fuxx-no-such-program", Some("/usr/bin:/bin")));
        assert!(!on_path("sh", Some("")), "nowhere to look");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A plan card's title (the 2026-11-25 chores): its first `#` heading, less a leading
    /// "Plan:", else "Plan".
    #[test]
    fn plan_title_falls_back() {
        assert_eq!(plan_title("# Plan: Retry\n\n## Steps"), "Retry");
        assert_eq!(plan_title("# Retry"), "Retry");
        assert_eq!(plan_title("intro\n# Later"), "Later");
        assert_eq!(plan_title("no heading"), "Plan");
        assert_eq!(plan_title(""), "Plan");
        assert_eq!(plan_title("## Only a second level"), "Plan");
    }

    /// A provider that overrides nothing it needn't.
    struct Plain;
    struct Silent;
    static PLAIN_INFO: ProviderInfo = ProviderInfo {
        name: "plain",
        label: "Plain",
        badge: "PL",
        who: ("P", "plain"),
        badge_rgb: 0,
        logo: None,
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
    fn a_provider_takes_images_up_to_5_mb_raw_unless_it_says_otherwise() {
        assert_eq!(Plain.image_limit(), ImageLimit::Raw(5 * 1024 * 1024));
    }

    #[test]
    fn an_encoded_limit_is_checked_on_the_base64_size() {
        let five = 5 * 1024 * 1024;
        let encoded = ImageLimit::Encoded(five);
        assert_eq!(encoded.raw_max(), 3_932_160);
        assert_eq!(base64(&vec![0; encoded.raw_max()]).len(), five);
        assert!(encoded.fits(3_932_160));
        assert!(!encoded.fits(3_932_161), "4 more digits: over 5 MB");
        assert_eq!(
            encoded.refusal("Claude", "a.png"),
            "Image too big for Claude (3.75 MB max): a.png"
        );
        let raw = ImageLimit::Raw(five);
        assert_eq!(raw.raw_max(), five);
        assert!(raw.fits(five) && !raw.fits(five + 1));
        assert_eq!(raw.refusal("agy", "a.png"), "Image over 5 MB: a.png");
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

    /// Batch 3, chore 2: a provider that says nothing can't branch, mints no message ids, sets
    /// no environment and has no one-off file rewind.
    /// Batch 3, wave 3: only a provider that says so can take back a message it hasn't read.
    #[test]
    fn a_provider_cant_cancel_a_queued_message_unless_it_says() {
        assert!(!Plain.cancels_queued());
        assert!(!agy::AGY.cancels_queued());
        assert!(claude::CLAUDE.cancels_queued());
    }

    #[test]
    fn a_provider_cant_branch_unless_it_says() {
        assert_eq!(Plain.branching(), Branching::default());
        assert!(!Plain.branching().any());
        assert_eq!(Plain.fresh_message_id(), None);
        assert!(Plain.process_env(true).is_empty());
        let start = Start {
            cwd: PathBuf::from("/tmp"),
            resume: Some("x".into()),
            new_id: None,
            mode: None,
            model: None,
            effort: None,
            extra_args: Vec::new(),
            branch: None,
        };
        assert!(Plain.branch_point(Path::new("/tmp"), &start, "m").is_err());
        assert!(Plain.rewind_files_probe(&start, "m").is_none());
        assert_eq!(Plain.edit_tip(), None);
    }

    #[test]
    fn by_name_finds_claude_and_agy_and_nothing_else() {
        assert_eq!(by_name("claude").map(|p| p.info().name), Some("claude"));
        assert_eq!(by_name("agy").map(|p| p.info().label), Some("Antigravity"));
        assert!(by_name("opencode").is_none());
    }
}
