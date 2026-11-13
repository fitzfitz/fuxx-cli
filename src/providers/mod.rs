#![allow(dead_code)] // consumers arrive in M11aa Tasks 7–11; remove when wired
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
    /// Past events of a resumed conversation (empty unless the provider can read them).
    fn history(&self, _start: &Start) -> Vec<Event> {
        Vec::new()
    }
    fn translator(&self) -> Box<dyn Translator>;
}

pub trait Translator: Send {
    /// Lines written before the first message.
    fn hello(&mut self) -> Vec<String>;
    /// One stdout line to zero or more events.
    fn decode(&mut self, line: &str) -> Vec<Event>;
    /// One action to the lines to write.
    fn encode(&mut self, action: &Action) -> Vec<String>;
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

    #[test]
    fn by_name_finds_claude_and_nothing_else() {
        assert_eq!(by_name("claude").map(|p| p.info().name), Some("claude"));
        assert!(by_name("opencode").is_none());
    }
}
