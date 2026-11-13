#![allow(dead_code)] // consumers arrive in M11aa Tasks 7–11; remove when wired
//! The conversation model (M11aa): folds a provider's `Event`s into chat items and a status.
//! Pure and GPUI-free.

use crate::providers::{Event, TodoItem, ToolDetail};
use crate::status::Status;

pub const OUTPUT_KEEP_LINES: usize = 2_000;
pub const OUTPUT_KEEP_BYTES: usize = 256 * 1024;

#[derive(Clone, Debug, PartialEq)]
pub enum Answer {
    Allowed,
    AlwaysAllowed,
    Denied(Option<String>),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    You {
        text: String,
        attachments: Vec<String>,
    },
    Agent {
        message: String,
        text: String,
        done: bool,
    },
    Tool {
        id: String,
        name: String,
        summary: String,
        parent: Option<String>,
        running: bool,
        ok: Option<bool>,
        result: String,
        detail: Option<ToolDetail>,
        output: Option<String>,
        dropped_lines: usize,
    },
    Todo(Vec<TodoItem>),
    Permission {
        id: String,
        tool: String,
        what: String,
        detail: String,
        always: Option<String>,
        answer: Option<Answer>,
    },
    Notice(String),
    Error(String),
}

#[derive(Default)]
pub struct Conversation {
    items: Vec<Item>,
    conversation: Option<String>,
    model: Option<String>,
    cost_usd: Option<f64>,
    context_pct: Option<u8>,
    turn_running: bool,
    ended: bool,
    /// A turn has finished since the last `you`.
    finished: bool,
}

impl Conversation {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn apply(&mut self, event: Event) {
        match event {
            Event::Started {
                conversation,
                model,
            } => {
                self.conversation = Some(conversation);
                if model.is_some() {
                    self.model = model;
                }
            }
            Event::TextDelta { message, text } => {
                let found = self.items.iter_mut().find_map(|i| match i {
                    Item::Agent {
                        message: m,
                        text: t,
                        ..
                    } if *m == message => Some(t),
                    _ => None,
                });
                match found {
                    Some(t) => t.push_str(&text),
                    None => self.items.push(Item::Agent {
                        message,
                        text,
                        done: false,
                    }),
                }
            }
            Event::MessageDone { message, text } => {
                let found = self.items.iter_mut().find_map(|i| match i {
                    Item::Agent {
                        message: m,
                        text: t,
                        done,
                    } if *m == message => Some((t, done)),
                    _ => None,
                });
                match found {
                    Some((t, done)) => {
                        *t = text;
                        *done = true;
                    }
                    None => self.items.push(Item::Agent {
                        message,
                        text,
                        done: true,
                    }),
                }
            }
            Event::ToolStarted {
                id,
                name,
                summary,
                parent,
                detail,
            } => self.items.push(Item::Tool {
                id,
                name,
                summary,
                parent,
                running: true,
                ok: None,
                result: String::new(),
                detail,
                output: None,
                dropped_lines: 0,
            }),
            Event::ToolFinished {
                id,
                ok,
                summary,
                output,
            } => {
                for item in &mut self.items {
                    if let Item::Tool {
                        id: tid,
                        running,
                        ok: tok,
                        result,
                        output: tout,
                        dropped_lines,
                        ..
                    } = item
                        && *tid == id
                    {
                        *running = false;
                        *tok = Some(ok);
                        *result = summary;
                        let (kept, dropped) = match output {
                            Some(o) => {
                                let (k, d) = cap_output(&o);
                                (Some(k), d)
                            }
                            None => (None, 0),
                        };
                        *tout = kept;
                        *dropped_lines = dropped;
                        break;
                    }
                }
            }
            Event::Todo(list) => match self.items.iter_mut().find(|i| matches!(i, Item::Todo(_))) {
                Some(item) => *item = Item::Todo(list),
                None => self.items.push(Item::Todo(list)),
            },
            Event::Permission {
                id,
                tool,
                what,
                detail,
                always,
            } => self.items.push(Item::Permission {
                id,
                tool,
                what,
                detail,
                always,
                answer: None,
            }),
            Event::Usage {
                context_pct,
                cost_usd,
            } => {
                if context_pct.is_some() {
                    self.context_pct = context_pct;
                }
                if cost_usd.is_some() {
                    self.cost_usd = cost_usd;
                }
            }
            Event::TurnDone { ok, message } => {
                self.turn_running = false;
                self.finished = true;
                if !ok && let Some(text) = message {
                    // The translator reports one failure as `Error` then `TurnDone`.
                    let dup = matches!(self.items.last(), Some(Item::Error(e)) if *e == text);
                    if !dup {
                        self.items.push(Item::Error(text));
                    }
                }
            }
            Event::Notice(text) => self.items.push(Item::Notice(text)),
            Event::Error(text) => self.items.push(Item::Error(text)),
        }
    }

    pub fn you(&mut self, text: String, attachments: Vec<String>) {
        self.items.push(Item::You { text, attachments });
        self.turn_running = true;
        self.finished = false;
    }

    pub fn answered(&mut self, id: &str, answer: Answer) {
        for item in &mut self.items {
            if let Item::Permission {
                id: pid, answer: a, ..
            } = item
                && pid == id
            {
                *a = Some(answer);
                return;
            }
        }
    }

    pub fn process_ended(&mut self, code: Option<i32>, stderr_tail: &str) {
        self.ended = true;
        self.turn_running = false;
        let tail = stderr_tail.trim();
        let what = match code {
            Some(c) => format!("The agent exited (code {c})"),
            None => "The agent was terminated".to_string(),
        };
        if code == Some(0) && tail.is_empty() {
            self.items.push(Item::Notice("The agent exited".into()));
        } else if tail.is_empty() {
            self.items.push(Item::Error(what));
        } else {
            self.items.push(Item::Error(format!("{what}: {tail}")));
        }
    }

    pub fn items(&self) -> &[Item] {
        &self.items
    }

    pub fn pending_permission(&self) -> Option<&Item> {
        self.items
            .iter()
            .find(|i| matches!(i, Item::Permission { answer: None, .. }))
    }

    pub fn status(&self) -> Status {
        if self.ended {
            Status::Done
        } else if self.pending_permission().is_some() {
            Status::Waiting
        } else if self.turn_running {
            Status::Working
        } else if self.finished {
            Status::Done
        } else {
            Status::Idle
        }
    }

    pub fn conversation(&self) -> Option<&str> {
        self.conversation.as_deref()
    }

    pub fn model(&self) -> Option<&str> {
        self.model.as_deref()
    }

    pub fn cost_usd(&self) -> Option<f64> {
        self.cost_usd
    }

    pub fn context_pct(&self) -> Option<u8> {
        self.context_pct
    }

    /// The first line of the first thing you said: at most 40 characters, then "…".
    pub fn title(&self) -> Option<String> {
        let text = self.items.iter().find_map(|i| match i {
            Item::You { text, .. } => Some(text),
            _ => None,
        })?;
        let line = text.lines().next()?.trim();
        if line.is_empty() {
            return None;
        }
        if line.chars().count() > 40 {
            Some(line.chars().take(40).collect::<String>() + "…")
        } else {
            Some(line.to_string())
        }
    }
}

/// Keep the last `OUTPUT_KEEP_LINES` lines and at most `OUTPUT_KEEP_BYTES` bytes (whole lines);
/// also report how many lines were dropped.
fn cap_output(output: &str) -> (String, usize) {
    let lines: Vec<&str> = output.lines().collect();
    let total = lines.len();
    let mut start = total.saturating_sub(OUTPUT_KEEP_LINES);
    // Drop whole lines from the front until the rest fits in the byte budget.
    let mut bytes: usize = lines[start..].iter().map(|l| l.len() + 1).sum::<usize>();
    bytes = bytes.saturating_sub(1);
    while start < total && bytes > OUTPUT_KEEP_BYTES {
        bytes = bytes.saturating_sub(lines[start].len() + 1);
        start += 1;
    }
    (lines[start..].join("\n"), start)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::{Provider, TodoState};

    fn permission(id: &str) -> Event {
        Event::Permission {
            id: id.into(),
            tool: "Bash".into(),
            what: "run ls".into(),
            detail: "ls".into(),
            always: None,
        }
    }

    fn delta(message: &str, text: &str) -> Event {
        Event::TextDelta {
            message: message.into(),
            text: text.into(),
        }
    }

    fn todo(text: &str) -> Event {
        Event::Todo(vec![TodoItem {
            text: text.into(),
            state: TodoState::Active,
        }])
    }

    fn tool_started(id: &str) -> Event {
        Event::ToolStarted {
            id: id.into(),
            name: "Bash".into(),
            summary: "ls".into(),
            parent: None,
            detail: None,
        }
    }

    #[test]
    fn status_follows_the_turn() {
        let mut c = Conversation::new();
        assert_eq!(c.status(), Status::Idle);
        c.you("hi".into(), vec![]);
        assert_eq!(c.status(), Status::Working);
        c.apply(permission("p1"));
        assert_eq!(c.status(), Status::Waiting);
        assert!(c.pending_permission().is_some());
        c.answered("p1", Answer::Allowed);
        assert_eq!(c.status(), Status::Working);
        assert!(c.pending_permission().is_none());
        c.apply(Event::TurnDone {
            ok: true,
            message: None,
        });
        assert_eq!(c.status(), Status::Done);
        c.you("again".into(), vec![]);
        assert_eq!(c.status(), Status::Working);
    }

    #[test]
    fn deltas_build_one_message() {
        let mut c = Conversation::new();
        c.apply(delta("m1", "Hel"));
        c.apply(delta("m1", "lo"));
        assert_eq!(c.items().len(), 1);
        assert!(matches!(&c.items()[0],
            Item::Agent { text, done: false, .. } if text == "Hello"));
        c.apply(Event::MessageDone {
            message: "m1".into(),
            text: "Hello!".into(),
        });
        assert_eq!(c.items().len(), 1);
        assert!(matches!(&c.items()[0],
            Item::Agent { text, done: true, .. } if text == "Hello!"));
    }

    #[test]
    fn todo_updates_in_place() {
        let mut c = Conversation::new();
        c.apply(todo("first"));
        c.apply(delta("m", "x"));
        c.apply(todo("second"));
        assert_eq!(c.items().len(), 2);
        assert!(matches!(&c.items()[0], Item::Todo(t) if t.len() == 1 && t[0].text == "second"));
    }

    #[test]
    fn tool_output_is_capped() {
        let mut c = Conversation::new();
        c.apply(tool_started("t1"));
        let out: Vec<String> = (0..100_000).map(|i| format!("line {i}")).collect();
        c.apply(Event::ToolFinished {
            id: "t1".into(),
            ok: true,
            summary: "done".into(),
            output: Some(out.join("\n")),
        });
        match &c.items()[0] {
            Item::Tool {
                output: Some(o),
                dropped_lines,
                running: false,
                ok: Some(true),
                result,
                ..
            } => {
                assert_eq!(o.lines().count(), OUTPUT_KEEP_LINES);
                assert!(o.ends_with("line 99999"));
                assert!(o.starts_with("line 98000"));
                assert_eq!(*dropped_lines, 98_000);
                assert_eq!(result, "done");
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn tool_output_is_capped_by_bytes() {
        let mut c = Conversation::new();
        c.apply(tool_started("t1"));
        let long = "é".repeat(1000);
        let out = vec![long; 600].join("\n"); // 600 lines, ~1.2 MB
        c.apply(Event::ToolFinished {
            id: "t1".into(),
            ok: true,
            summary: String::new(),
            output: Some(out),
        });
        match &c.items()[0] {
            Item::Tool {
                output: Some(o),
                dropped_lines,
                ..
            } => {
                assert!(o.len() <= OUTPUT_KEEP_BYTES);
                assert_eq!(o.lines().count() + dropped_lines, 600);
                assert!(*dropped_lines > 0);
                assert!(o.lines().all(|l| l.chars().count() == 1000));
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn crash_is_done_with_an_error() {
        let mut c = Conversation::new();
        c.you("x".into(), vec![]);
        c.process_ended(Some(1), "not logged in");
        assert_eq!(c.status(), Status::Done);
        assert!(matches!(c.items().last(), Some(Item::Error(e)) if e.contains("not logged in")));
    }

    #[test]
    fn failed_turn_shows_one_error() {
        let mut c = Conversation::new();
        c.you("x".into(), vec![]);
        c.apply(Event::Error("boom".into()));
        c.apply(Event::TurnDone {
            ok: false,
            message: Some("boom".into()),
        });
        let errors = c
            .items()
            .iter()
            .filter(|i| matches!(i, Item::Error(_)))
            .count();
        assert_eq!(errors, 1);
        assert_eq!(c.status(), Status::Done);
        // A failure with a different message still shows.
        c.apply(Event::TurnDone {
            ok: false,
            message: Some("other".into()),
        });
        assert!(matches!(c.items().last(), Some(Item::Error(e)) if e == "other"));
    }

    #[test]
    fn title_counts_characters() {
        let mut c = Conversation::new();
        assert_eq!(c.title(), None);
        c.you("é".repeat(50) + "\nsecond line", vec![]);
        assert_eq!(c.title(), Some("é".repeat(40) + "…"));
        let mut d = Conversation::new();
        d.you("short\nmore".into(), vec![]);
        assert_eq!(d.title(), Some("short".into()));
    }

    #[test]
    fn started_and_usage_are_recorded() {
        let mut c = Conversation::new();
        c.apply(Event::Started {
            conversation: "abc".into(),
            model: Some("m".into()),
        });
        c.apply(Event::Usage {
            context_pct: Some(12),
            cost_usd: Some(0.5),
        });
        assert_eq!(c.conversation(), Some("abc"));
        assert_eq!(c.model(), Some("m"));
        assert_eq!(c.context_pct(), Some(12));
        assert_eq!(c.cost_usd(), Some(0.5));
    }

    #[test]
    fn replays_the_tools_fixture() {
        let mut t = crate::providers::claude::CLAUDE.translator();
        let mut c = Conversation::new();
        let text = include_str!("providers/fixtures/claude-tools.jsonl");
        for e in text.lines().flat_map(|l| t.decode(l)) {
            c.apply(e);
        }
        let tools: Vec<_> = c
            .items()
            .iter()
            .filter(|i| matches!(i, Item::Tool { .. }))
            .collect();
        assert!(!tools.is_empty());
        assert!(
            tools
                .iter()
                .all(|i| matches!(i, Item::Tool { running: false, .. }))
        );
    }
}
