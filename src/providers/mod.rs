//! Agent providers (M11aa): fuxx's own event language for an agent conversation, and the
//! translators that turn one agent's wire format into it. Pure and GPUI-free, and it knows no
//! agent by name here: each agent lives in its own file.

use std::path::PathBuf;

pub mod claude;

/// What fuxx shows and decides about a provider without starting it.
pub struct ProviderInfo {
    /// Matches a preset name.
    pub name: &'static str,
    pub label: &'static str,
    /// The two-letter tab badge.
    pub badge: &'static str,
    pub badge_rgb: u32,
    /// Whether the agent asks fuxx (not the user's terminal) before using a tool.
    #[allow(dead_code)] // M11aa Task 12 (agy: false); nothing reads it yet
    pub routes_permissions: bool,
    pub modes: &'static [&'static str],
    pub default_mode: Option<&'static str>,
}

/// How to start (or resume) a conversation.
#[derive(Clone, Debug, PartialEq)]
pub struct Start {
    pub cwd: PathBuf,
    pub resume: Option<String>,
    pub mode: Option<String>,
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
    /// The model chip's text for the model the agent reports (`model`): as it is, unless the
    /// provider knows a shorter name for it.
    fn model_label(&self, model: &str) -> String {
        model.to_string()
    }
    /// What was said before in a resumed conversation (empty unless the provider can read it).
    /// Reads a file: call it off the UI thread.
    fn history(&self, _start: &Start) -> Vec<Past> {
        Vec::new()
    }
    fn translator(&self) -> Box<dyn Translator>;
    /// Whether what a resumed run said on its way out (its stderr tail, or an error row) means
    /// the conversation to resume doesn't exist: only then does the tab start a new one.
    fn unknown_conversation(&self, _said: &str) -> bool {
        false
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

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Send { text: String },
    Allow { id: String },
    AlwaysAllow { id: String },
    Deny { id: String, reason: Option<String> },
    Interrupt,
}

/// Every provider fuxx can run an agent tab with.
pub fn all() -> &'static [&'static dyn Provider] {
    static ALL: [&dyn Provider; 1] = [&claude::CLAUDE];
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
        routes_permissions: false,
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
    fn a_model_is_labelled_as_it_is_unless_the_provider_knows_better() {
        assert_eq!(Plain.model_label("claude-opus-5-5"), "claude-opus-5-5");
        assert_eq!(Plain.model_label("gemini-3"), "gemini-3");
    }

    #[test]
    fn by_name_finds_claude_and_nothing_else() {
        assert_eq!(by_name("claude").map(|p| p.info().name), Some("claude"));
        assert!(by_name("opencode").is_none());
    }
}
