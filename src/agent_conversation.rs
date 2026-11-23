//! The conversation model (M11aa): folds a provider's `Event`s into chat items and a status.
//! Pure and GPUI-free.

use crate::providers::{Event, LaneControls, LaneEnd, Past, TodoItem, ToolDetail};
use crate::status::Status;

pub const OUTPUT_KEEP_LINES: usize = 2_000;
pub const OUTPUT_KEEP_BYTES: usize = 256 * 1024;
/// The notice an agent tab shows once its conversation is opened in a terminal (R13): its own
/// process has stopped until you send from it again.
pub const PAUSED_NOTICE: &str = "Open in classic view — this tab is paused";

#[derive(Clone, Debug, PartialEq)]
pub enum Answer {
    Allowed,
    AlwaysAllowed,
    Denied(Option<String>),
    /// The turn ended (or the process died) before you answered.
    Expired,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    You {
        text: String,
        attachments: Vec<String>,
        /// Its id in the agent's record (batch 3, chore 2: `Provider::fresh_message_id`, or the
        /// history's), when the agent takes one: what Rewind and Fork name it by.
        id: Option<String>,
        /// The agent has read it (its echo, `Event::Taken`; anything in the history): only then
        /// can it be rewound or forked from.
        taken: bool,
    },
    Agent {
        message: String,
        text: String,
        done: bool,
        /// The tool call of the sub-agent that said it; `None` for the main agent.
        parent: Option<String>,
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
        /// The subagent (lane id) that asks; `None`: the main agent.
        by: Option<String>,
    },
    Notice(String),
    Error(String),
}

/// A subagent (M11ar): its steps are the items whose `parent` is its `id`.
#[derive(Clone, Debug, PartialEq)]
pub struct Lane {
    pub id: String,
    /// The lane that started it, when nested (drawn flat for now).
    pub parent: Option<String>,
    pub label: String,
    pub kind: String,
    pub task: String,
    pub model: Option<String>,
    pub status: LaneStatus,
    pub tokens: Option<u64>,
    pub tools: Option<u32>,
    pub last_tool: Option<String>,
    pub controls: LaneControls,
}

#[derive(Clone, Debug, PartialEq)]
pub enum LaneStatus {
    Working,
    /// It asked you something (a permission) and waits.
    Waiting,
    Done,
    Failed(String),
    Stopped,
}

impl LaneStatus {
    /// Still going (working or waiting on you).
    pub fn live(&self) -> bool {
        matches!(self, LaneStatus::Working | LaneStatus::Waiting)
    }
}

#[derive(Default)]
pub struct Conversation {
    items: Vec<Item>,
    /// Your latest message's index (the chat's latest-prompt strip, chore after M11ar): kept as
    /// items come, so nothing scans the conversation for it.
    latest_you: Option<usize>,
    /// The subagents, in start order.
    lanes: Vec<Lane>,
    /// Where each lane started (`items.len()` then), by the lanes' order: a cut keeps the lanes
    /// started before it (batch 3, chore 2).
    lane_starts: Vec<usize>,
    conversation: Option<String>,
    model: Option<String>,
    cost_usd: Option<f64>,
    context_pct: Option<u8>,
    /// Messages sent and not yet read. Claude says when it reads one (`Taken`, its echo): a
    /// note sent mid-turn may be read in that same turn, with no `result` of its own (your
    /// report, 2026-11-22: Main stayed "Cultivating…" and Esc stuck on "Stopping…"). An agent
    /// that doesn't say (agy) answers each with its own turn: a `TurnDone` counts one.
    outstanding: u32,
    /// The agent says when it reads a message (a `Taken` has come).
    echoes: bool,
    /// Main's own activity (its words, its tools, reading a message) since the last `TurnDone`;
    /// a subagent's steps don't count.
    busy: bool,
    ended: bool,
    /// A turn has finished since the last `you`.
    finished: bool,
}

/// What `Conversation::cut_before` cut (batch 3, chore 2): kept for Undo Rewind until the next
/// send.
#[derive(Clone, Debug, PartialEq)]
#[allow(dead_code)] // Batch 3 wave 2 (the Rewind/Fork UI) calls it.
pub struct Cut {
    items: Vec<Item>,
    lanes: Vec<Lane>,
    lane_starts: Vec<usize>,
    latest_you: Option<usize>,
}

#[allow(dead_code)] // Batch 3 wave 2 (the Rewind/Fork UI) calls it.
impl Cut {
    /// The cut items, your message first.
    pub fn items(&self) -> &[Item] {
        &self.items
    }

    /// Your message the cut was made before (its text and attachments go back in the box).
    pub fn message(&self) -> Option<&Item> {
        self.items.first()
    }
}

impl Conversation {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn apply(&mut self, event: Event) {
        // Main working: its own words or tools (not a subagent's). Not `Started`: agy says
        // `init` as it starts up, before anything is sent; a turn is counted by what was sent.
        if matches!(
            &event,
            Event::TextDelta { parent: None, .. }
                | Event::MessageDone { parent: None, .. }
                | Event::ToolStarted { parent: None, .. }
        ) {
            self.busy = true;
        }
        match event {
            Event::Cleared => {
                self.items.clear();
                self.latest_you = None;
                self.lanes.clear();
                self.lane_starts.clear();
                self.cost_usd = None;
                self.context_pct = None;
                self.finished = false;
            }
            Event::Started {
                conversation,
                model,
            } => {
                self.conversation = Some(conversation);
                if model.is_some() {
                    self.model = model;
                }
            }
            Event::TextDelta {
                message,
                text,
                parent,
            } => {
                // A finished block is closed: a later block of the same message starts anew.
                let found = self.items.iter_mut().rev().find_map(|i| match i {
                    Item::Agent {
                        message: m,
                        text: t,
                        done: false,
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
                        parent,
                    }),
                }
            }
            Event::MessageDone {
                message,
                text,
                parent,
            } => {
                let found = self.items.iter_mut().rev().find_map(|i| match i {
                    Item::Agent {
                        message: m,
                        text: t,
                        done: done @ false,
                        ..
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
                        parent,
                    }),
                }
            }
            Event::ToolStarted {
                id,
                name,
                summary,
                parent,
                detail,
                writes: _,
                texts: _,
            } => {
                self.reopen(parent.as_deref());
                self.items.push(Item::Tool {
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
                })
            }
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
                by,
            } => {
                if let Some(lane) = by.as_deref().and_then(|b| self.lane_mut(b)) {
                    lane.status = LaneStatus::Waiting;
                }
                self.items.push(Item::Permission {
                    id,
                    tool,
                    what,
                    detail,
                    always,
                    answer: None,
                    by,
                })
            }
            Event::SubagentStarted {
                id,
                parent,
                label,
                kind,
                task,
                model,
                controls,
            } => {
                let lane = Lane {
                    id: id.clone(),
                    parent,
                    label,
                    kind,
                    task,
                    model,
                    status: LaneStatus::Working,
                    tokens: None,
                    tools: None,
                    last_tool: None,
                    controls,
                };
                match self.lane_mut(&id) {
                    Some(old) => *old = lane,
                    None => {
                        self.lanes.push(lane);
                        self.lane_starts.push(self.items.len());
                    }
                }
            }
            Event::SubagentProgress {
                id,
                tokens,
                tools,
                last_tool,
            } => {
                if let Some(lane) = self.lane_mut(&id) {
                    if tokens.is_some() {
                        lane.tokens = tokens;
                    }
                    if tools.is_some() {
                        lane.tools = tools;
                    }
                    if last_tool.is_some() {
                        lane.last_tool = last_tool;
                    }
                }
            }
            Event::SubagentEnded { id, end } => {
                if let Some(lane) = self.lane_mut(&id) {
                    // A late report wins over a stop (agy's transcript can lag a poll).
                    let keep = matches!(lane.status, LaneStatus::Done | LaneStatus::Failed(_))
                        && end == LaneEnd::Stopped;
                    if !keep {
                        lane.status = match end {
                            LaneEnd::Done => LaneStatus::Done,
                            LaneEnd::Failed(why) => LaneStatus::Failed(why),
                            LaneEnd::Stopped => LaneStatus::Stopped,
                        };
                    }
                    self.settle_lane(&id);
                }
            }
            // The tab follows the file; nothing to show yet.
            Event::Follow { .. } => {}
            // The agent read one of your messages (in this turn, or starting its own): the one
            // of that id, or (none said) the oldest not read yet.
            Event::Taken { id } => {
                self.echoes = true;
                self.outstanding = self.outstanding.saturating_sub(1);
                self.busy = true;
                self.mark_taken(id.as_deref());
            }
            // The tab acts on a file rewind's answer (batch 3, chore 2); nothing to show here.
            Event::FilesRewound { .. } => {}
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
                // An agent that says when it reads each message (`Taken`: Claude's echo) has
                // counted already; one that doesn't answers each with its own turn (agy). A
                // failed or stopped turn drops what was queued behind it.
                self.outstanding = if !ok {
                    0
                } else if self.echoes {
                    self.outstanding
                } else {
                    self.outstanding.saturating_sub(1)
                };
                self.busy = false;
                self.finished = true;
                self.settle();
                if !ok && let Some(text) = message {
                    // The translator reports one failure as `Error` then `TurnDone`.
                    let dup = matches!(self.items.last(), Some(Item::Error(e)) if *e == text);
                    if !dup {
                        self.items.push(Item::Error(text));
                    }
                }
            }
            Event::Notice(text) => self.items.push(Item::Notice(text)),
            // The catalog, the skills, the limits and the mode are the tab's, not the
            // conversation's.
            Event::Catalog(_)
            | Event::Suggestion(_)
            | Event::Skills(_)
            | Event::RateLimit(_)
            | Event::Mode(_) => {}
            // A live model switch the agent confirmed: the id it now runs.
            Event::Applied {
                kind: crate::providers::ChangeKind::Model,
                value,
                ok: true,
            } => self.model = Some(value),
            Event::Applied { .. } => {}
            Event::Error(text) => self.items.push(Item::Error(text)),
        }
    }

    pub fn you(&mut self, text: String, attachments: Vec<String>) {
        self.you_as(text, attachments, None);
    }

    /// `you`, the message sent under `id` (`Provider::fresh_message_id`; batch 3, chore 2).
    pub fn you_as(&mut self, text: String, attachments: Vec<String>, id: Option<String>) {
        self.latest_you = Some(self.items.len());
        self.items.push(Item::You {
            text,
            attachments,
            id,
            taken: false,
        });
        self.outstanding += 1;
        self.ended = false;
        self.finished = false;
    }

    /// Your message `id` was read (none said: the oldest not read yet). Looked for from the
    /// newest back when named (it is nearly always the last), from the oldest when not.
    fn mark_taken(&mut self, id: Option<&str>) {
        let unread = |i: &Item| matches!(i, Item::You { taken: false, .. });
        let found = match id {
            Some(id) => self
                .items
                .iter_mut()
                .rev()
                .find(|i| matches!(i, Item::You { id: Some(x), .. } if x == id)),
            None => self.items.iter_mut().find(|i| unread(i)),
        };
        if let Some(Item::You { taken, .. }) = found {
            *taken = true;
        }
    }

    /// A resumed conversation's history: what you said (shown, but not counted as a send
    /// waiting for its turn) and the agent's events, then settled (nothing in it runs or waits):
    /// it reads Done when anything was said.
    pub fn past(&mut self, past: Vec<Past>) {
        for piece in past {
            match piece {
                Past::You { text, id } => {
                    self.latest_you = Some(self.items.len());
                    self.items.push(Item::You {
                        text,
                        attachments: Vec::new(),
                        id,
                        taken: true,
                    })
                }
                Past::Event(event) => self.apply(event),
            }
        }
        self.outstanding = 0;
        self.busy = false;
        // A subagent with no end in the history was cut off (the app quit mid-run).
        self.stop_lanes();
        self.settle();
    }

    /// The agent couldn't be started: one error row, and the conversation is over (Done).
    pub fn failed_to_start(&mut self, message: &str) {
        self.ended = true;
        self.outstanding = 0;
        self.busy = false;
        self.stop_lanes();
        self.settle();
        self.items.push(Item::Error(message.to_string()));
    }

    /// Keep only the first `len` items (a resume that failed: what that run said goes, and a
    /// new conversation follows what came before). Nothing is left running or waiting.
    pub fn keep_first(&mut self, len: usize) {
        self.items.truncate(len);
        // Rare (a failed resume): the one still there is looked for once.
        if self.latest_you.is_some_and(|i| i >= len) {
            self.latest_you = self
                .items
                .iter()
                .rposition(|i| matches!(i, Item::You { .. }));
        }
        self.lanes.clear();
        self.lane_starts.clear();
        self.outstanding = 0;
        self.busy = false;
        self.ended = false;
        self.settle();
    }

    /// Rewind (batch 3, chore 2): the conversation cut before your message at `ix` — it and
    /// everything after it go, handed back as a `Cut` (for Undo Rewind, `restore`); the lanes
    /// started before it stay; nothing is left running or waiting. None (nothing cut) unless
    /// `ix` is a message of yours.
    #[allow(dead_code)] // Batch 3 wave 2 (the Rewind/Fork UI) calls it.
    pub fn cut_before(&mut self, ix: usize) -> Option<Cut> {
        if !matches!(self.items.get(ix), Some(Item::You { .. })) {
            return None;
        }
        // A lane's start is how many items there were then: one at `ix` or before began before
        // your message at `ix` was added.
        let keep = self.lane_starts.iter().take_while(|&&at| at <= ix).count();
        let cut = Cut {
            items: self.items.split_off(ix),
            lanes: self.lanes.split_off(keep),
            lane_starts: self.lane_starts.split_off(keep),
            latest_you: self.latest_you,
        };
        // Once, on a cut: the message before it.
        self.latest_you = self
            .items
            .iter()
            .rposition(|i| matches!(i, Item::You { .. }));
        self.outstanding = 0;
        self.busy = false;
        self.ended = false;
        self.finished = false;
        self.settle();
        Some(cut)
    }

    /// Undo Rewind: what `cut_before` cut, back after what it kept (nothing may have been added
    /// since: the tab keeps a cut only until the next send).
    #[allow(dead_code)] // Batch 3 wave 2 (the Rewind/Fork UI) calls it.
    pub fn restore(&mut self, cut: Cut) {
        self.items.extend(cut.items);
        self.lanes.extend(cut.lanes);
        self.lane_starts.extend(cut.lane_starts);
        self.latest_you = cut.latest_you;
    }

    /// Notice `old` (its last one) says `new` instead, in its place; without one, `new` is added.
    pub fn replace_notice(&mut self, old: &str, new: &str) {
        let found = self
            .items
            .iter_mut()
            .rev()
            .find(|i| matches!(i, Item::Notice(t) if t == old));
        match found {
            Some(item) => *item = Item::Notice(new.to_string()),
            None => self.items.push(Item::Notice(new.to_string())),
        }
    }

    pub fn answered(&mut self, id: &str, answer: Answer) {
        let mut asker = None;
        for item in &mut self.items {
            if let Item::Permission {
                id: pid,
                answer: a,
                by,
                ..
            } = item
                && pid == id
            {
                *a = Some(answer);
                asker = by.clone();
                break;
            }
        }
        // It works again once none of its questions is open.
        let still_asks = asker.as_deref().is_some_and(|b| {
            self.items
                .iter()
                .any(|i| matches!(i, Item::Permission { answer: None, by: Some(x), .. } if x == b))
        });
        if !still_asks
            && let Some(lane) = asker.as_deref().and_then(|b| self.lane_mut(b))
            && lane.status == LaneStatus::Waiting
        {
            lane.status = LaneStatus::Working;
        }
    }

    /// The subagents, in start order.
    pub fn lanes(&self) -> &[Lane] {
        &self.lanes
    }

    pub fn lane(&self, id: &str) -> Option<&Lane> {
        self.lanes.iter().find(|l| l.id == id)
    }

    fn lane_mut(&mut self, id: &str) -> Option<&mut Lane> {
        self.lanes.iter_mut().find(|l| l.id == id)
    }

    /// How many tool calls lane `id` has made.
    pub fn lane_steps(&self, id: &str) -> usize {
        self.items
            .iter()
            .filter(|i| matches!(i, Item::Tool { parent: Some(p), .. } if p == id))
            .count()
    }

    /// A step under a done lane: it works again (a woken subagent). A stopped or failed one
    /// stays so (its lagging lines don't revive it).
    fn reopen(&mut self, parent: Option<&str>) {
        if let Some(lane) = parent.and_then(|p| self.lane_mut(p))
            && lane.status == LaneStatus::Done
        {
            lane.status = LaneStatus::Working;
        }
    }

    /// Lane `id` ended: its steps stop running and its questions expire.
    fn settle_lane(&mut self, id: &str) {
        for item in &mut self.items {
            match item {
                Item::Tool {
                    running,
                    parent: Some(p),
                    ..
                } if p == id => *running = false,
                Item::Agent {
                    done,
                    parent: Some(p),
                    ..
                } if p == id => *done = true,
                Item::Permission {
                    answer,
                    by: Some(b),
                    ..
                } if b == id && answer.is_none() => *answer = Some(Answer::Expired),
                _ => {}
            }
        }
    }

    /// The process was ended from here (Force stop, a restart for a change): its subagents
    /// stop, and nothing of theirs runs or waits on (no `process_ended` follows).
    pub fn process_gone(&mut self) {
        self.stop_lanes();
        self.settle();
    }

    /// The process is gone: every lane still going stops.
    fn stop_lanes(&mut self) {
        for lane in &mut self.lanes {
            if lane.status.live() {
                lane.status = LaneStatus::Stopped;
            }
        }
    }

    pub fn process_ended(&mut self, code: Option<i32>, stderr_tail: &str) {
        self.ended = true;
        self.outstanding = 0;
        self.busy = false;
        self.stop_lanes();
        self.settle();
        let tail = stderr_tail.trim();
        let what = match code {
            Some(c) => format!("The agent exited (code {c})"),
            None => "The agent was terminated".to_string(),
        };
        if code == Some(0) {
            // A clean exit is not an error, whatever it wrote to stderr.
            self.items.push(Item::Notice("The agent exited".into()));
        } else if tail.is_empty() {
            self.items.push(Item::Error(what));
        } else {
            self.items.push(Item::Error(format!("{what}: {tail}")));
        }
    }

    /// The tab's process was stopped for "Open in classic view" (R13): nothing runs or waits,
    /// one notice says so, and it reads Done. Not an exit: no error row.
    pub fn paused(&mut self) {
        self.paused_with(PAUSED_NOTICE);
    }

    /// `paused` with its own notice (a resumed tab that won't start because another process
    /// holds its conversation, R13).
    pub fn paused_with(&mut self, notice: &str) {
        self.ended = true;
        self.outstanding = 0;
        self.busy = false;
        self.stop_lanes();
        self.settle();
        self.items.push(Item::Notice(notice.into()));
    }

    /// A turn is over: nothing is still running and no question is still open — except under
    /// a lane still going (a background subagent outlives Main's turn; its own end settles it).
    fn settle(&mut self) {
        let live: Vec<String> = self
            .lanes
            .iter()
            .filter(|l| l.status.live())
            .map(|l| l.id.clone())
            .collect();
        let under_live = |p: &Option<String>| p.as_ref().is_some_and(|p| live.contains(p));
        for item in &mut self.items {
            match item {
                Item::Tool {
                    running, parent, ..
                } if !under_live(parent) => *running = false,
                Item::Agent { done, parent, .. } if !under_live(parent) => *done = true,
                Item::Permission { answer, by, .. } if answer.is_none() && !under_live(by) => {
                    *answer = Some(Answer::Expired)
                }
                _ => {}
            }
        }
    }

    pub fn items(&self) -> &[Item] {
        &self.items
    }

    /// Your latest message's index in `items`, if you said anything.
    pub fn latest_you(&self) -> Option<usize> {
        self.latest_you
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
        } else if self.outstanding > 0 || self.busy {
            Status::Working
        } else if self.finished || !self.items.is_empty() {
            // Anything said (even replayed history) with no turn running reads Done.
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
    while start + 1 < total && bytes > OUTPUT_KEEP_BYTES {
        bytes = bytes.saturating_sub(lines[start].len() + 1);
        start += 1;
    }
    let mut kept = lines[start..].join("\n");
    if kept.len() > OUTPUT_KEEP_BYTES {
        // One line alone is over budget: keep its tail, cut forward to a char boundary.
        let mut cut = kept.len() - OUTPUT_KEEP_BYTES;
        while !kept.is_char_boundary(cut) {
            cut += 1;
        }
        kept.drain(..cut);
    }
    (kept, start)
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
            by: None,
        }
    }

    fn delta(message: &str, text: &str) -> Event {
        Event::TextDelta {
            message: message.into(),
            text: text.into(),
            parent: None,
        }
    }

    fn done(message: &str, text: &str) -> Event {
        Event::MessageDone {
            message: message.into(),
            text: text.into(),
            parent: None,
        }
    }

    fn decode(fixture: &str) -> Vec<Event> {
        let mut t = crate::providers::claude::CLAUDE.translator();
        fixture.lines().flat_map(|l| t.decode(l)).collect()
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
            writes: Vec::new(),
            texts: Vec::new(),
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
    fn a_notice_is_replaced_in_its_place() {
        let mut c = Conversation::new();
        c.apply(Event::Notice("Conversation restored".into()));
        c.you("hi".into(), Vec::new());
        c.replace_notice("Conversation restored", "a new one");
        assert!(matches!(c.items(), [Item::Notice(t), Item::You { .. }] if t == "a new one"));
        // None to replace: added.
        c.replace_notice("Conversation restored", "again");
        assert!(matches!(c.items().last(), Some(Item::Notice(t)) if t == "again"));
    }

    #[test]
    fn deltas_build_one_message() {
        let mut c = Conversation::new();
        c.apply(delta("m1", "Hel"));
        c.apply(delta("m1", "lo"));
        assert_eq!(c.items().len(), 1);
        assert!(matches!(&c.items()[0],
            Item::Agent { text, done: false, .. } if text == "Hello"));
        c.apply(done("m1", "Hello!"));
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
    fn one_huge_line_keeps_its_tail() {
        for line in ["x".repeat(1_000_000), "é".repeat(500_000)] {
            let mut c = Conversation::new();
            c.apply(tool_started("t1"));
            c.apply(Event::ToolFinished {
                id: "t1".into(),
                ok: true,
                summary: String::new(),
                output: Some(line.clone()),
            });
            match &c.items()[0] {
                Item::Tool {
                    output: Some(o),
                    dropped_lines,
                    ..
                } => {
                    assert!(!o.is_empty());
                    assert!(o.len() <= OUTPUT_KEEP_BYTES);
                    assert!(o.len() > OUTPUT_KEEP_BYTES - 4);
                    assert!(line.ends_with(o.as_str()));
                    assert_eq!(*dropped_lines, 0);
                }
                other => panic!("unexpected {other:?}"),
            }
        }
    }

    #[test]
    fn turn_end_settles_tools_and_questions() {
        for end in 0..2 {
            let mut c = Conversation::new();
            c.you("x".into(), vec![]);
            c.apply(tool_started("t1"));
            c.apply(permission("p1"));
            assert_eq!(c.status(), Status::Waiting);
            if end == 0 {
                c.apply(Event::TurnDone {
                    ok: false,
                    message: None,
                });
            } else {
                c.process_ended(None, "");
            }
            assert_eq!(c.status(), Status::Done);
            assert!(c.pending_permission().is_none());
            assert!(c.items().iter().any(|i| matches!(
                i,
                Item::Permission {
                    answer: Some(Answer::Expired),
                    ..
                }
            )));
            assert!(c.items().iter().any(|i| matches!(
                i,
                Item::Tool {
                    running: false,
                    ok: None,
                    ..
                }
            )));
        }
    }

    #[test]
    fn past_turns_read_done_not_working() {
        let mut c = Conversation::new();
        c.past(vec![
            Past::you("hi"),
            Past::Event(Event::MessageDone {
                message: "m".into(),
                text: "Hello".into(),
                parent: None,
            }),
        ]);
        assert_eq!(c.status(), Status::Done);
        assert_eq!(c.items().len(), 2);
        assert!(matches!(&c.items()[0], Item::You { text, .. } if text == "hi"));
        // A new turn after it is one send: its TurnDone reads Done again.
        c.you("next".into(), vec![]);
        assert_eq!(c.status(), Status::Working);
        c.apply(Event::TurnDone {
            ok: true,
            message: None,
        });
        assert_eq!(c.status(), Status::Done);
    }

    #[test]
    fn past_with_an_open_tool_settles_it() {
        let mut c = Conversation::new();
        c.past(vec![Past::Event(tool_started("t1"))]);
        assert!(matches!(&c.items()[0], Item::Tool { running: false, .. }));
        assert_eq!(c.status(), Status::Done);
        let mut empty = Conversation::new();
        empty.past(Vec::new());
        assert_eq!(empty.status(), Status::Idle, "nothing past: nothing said");
    }

    /// A failed resume drops what its run said; nothing is left running or waiting.
    #[test]
    fn keep_first_drops_the_rest_and_settles() {
        let mut c = Conversation::new();
        c.apply(Event::Notice("before".into()));
        c.you("x".into(), vec![]);
        c.apply(permission("p1"));
        c.apply(Event::Error("No conversation found".into()));
        c.keep_first(1);
        assert!(matches!(c.items(), [Item::Notice(n)] if n == "before"));
        assert_eq!(c.status(), Status::Done);
        assert!(c.pending_permission().is_none());
        c.you("again".into(), vec![]);
        assert_eq!(c.status(), Status::Working);
    }

    #[test]
    fn spawn_failure_is_one_error_and_done() {
        let mut c = Conversation::new();
        c.failed_to_start("No such file");
        assert_eq!(c.status(), Status::Done);
        assert!(matches!(c.items(), [Item::Error(e)] if e.contains("No such file")));
    }

    #[test]
    fn a_turn_main_starts_itself_works_until_its_result() {
        // A turn the agent starts on its own (a background subagent waking it) works until its
        // `result`; history replays through `past`, which settles.
        let mut c = Conversation::new();
        assert_eq!(c.status(), Status::Idle);
        c.apply(delta("m", "old"));
        assert_eq!(c.status(), Status::Working);
        c.apply(Event::TurnDone {
            ok: true,
            message: None,
        });
        assert_eq!(c.status(), Status::Done);
        c.process_ended(Some(1), "bye");
        c.you("again".into(), vec![]);
        assert_eq!(c.status(), Status::Working);
    }

    #[test]
    fn clean_exit_is_never_an_error() {
        let mut c = Conversation::new();
        c.process_ended(Some(0), "some warning");
        assert!(!c.items().iter().any(|i| matches!(i, Item::Error(_))));
        assert_eq!(c.status(), Status::Done);
    }

    #[test]
    fn pausing_is_one_notice_and_done() {
        let mut c = Conversation::new();
        c.you("x".into(), vec![]);
        c.apply(permission("p1"));
        assert_eq!(c.status(), Status::Waiting);
        c.paused();
        assert!(c.items().iter().any(|i| matches!(
            i,
            Item::Permission {
                answer: Some(Answer::Expired),
                ..
            }
        )));
        assert_eq!(
            c.items().last(),
            Some(&Item::Notice(PAUSED_NOTICE.to_string()))
        );
        assert!(!c.items().iter().any(|i| matches!(i, Item::Error(_))));
        assert_eq!(
            c.items()
                .iter()
                .filter(|i| matches!(i, Item::Notice(_)))
                .count(),
            1
        );
        assert_eq!(c.status(), Status::Done);
        assert!(c.pending_permission().is_none());
        // Sending again starts a turn.
        c.you("y".into(), vec![]);
        assert_eq!(c.status(), Status::Working);
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

    /// agy says `init` the moment it starts, before anything is sent (your report, 2026-11-23:
    /// a new agent tab read "Cultivating…" with nothing running).
    #[test]
    fn an_agent_starting_up_is_idle_until_something_is_sent() {
        let mut c = Conversation::new();
        c.apply(Event::Started {
            conversation: "abc".into(),
            model: None,
        });
        assert_eq!(c.status(), Status::Idle);
        c.you("hi".into(), vec![]);
        assert_eq!(c.status(), Status::Working);
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

    #[test]
    fn a_queued_second_message_keeps_working_until_its_own_result() {
        // claude-queue.jsonl (recorded without Claude's echo): two messages sent mid-turn, two
        // `result`s — each `TurnDone` counts one.
        let mut c = Conversation::new();
        c.you("A".into(), vec![]);
        c.you("B".into(), vec![]);
        let mut results = 0;
        for e in decode(include_str!("providers/fixtures/claude-queue.jsonl")) {
            let is_done = matches!(e, Event::TurnDone { .. });
            c.apply(e);
            if is_done {
                results += 1;
                let want = if results == 1 {
                    Status::Working
                } else {
                    Status::Done
                };
                assert_eq!(c.status(), want, "after result {results}");
            }
        }
        assert_eq!(results, 2);
    }

    #[test]
    fn an_interrupt_drops_queued_turns_so_status_does_not_stick() {
        let mut c = Conversation::new();
        c.you("A".into(), vec![]);
        c.you("B".into(), vec![]);
        for e in decode(include_str!(
            "providers/fixtures/claude-interrupt-text.jsonl"
        )) {
            c.apply(e);
        }
        assert_eq!(c.status(), Status::Done);
        // A crash with sends outstanding also clears them.
        let mut d = Conversation::new();
        d.you("A".into(), vec![]);
        d.you("B".into(), vec![]);
        d.process_ended(Some(1), "");
        d.you("C".into(), vec![]);
        d.apply(Event::TurnDone {
            ok: true,
            message: None,
        });
        assert_eq!(d.status(), Status::Done);
    }

    fn started(id: &str, label: &str) -> Event {
        Event::SubagentStarted {
            id: id.into(),
            parent: None,
            label: label.into(),
            kind: "Explore".into(),
            task: "Find the config loader".into(),
            model: None,
            controls: LaneControls { stop: true },
        }
    }

    fn sub_tool(id: &str, parent: &str) -> Event {
        Event::ToolStarted {
            id: id.into(),
            name: "Read".into(),
            summary: "src/cfg.rs".into(),
            parent: Some(parent.into()),
            detail: None,
            writes: Vec::new(),
            texts: Vec::new(),
        }
    }

    #[test]
    fn a_subagent_becomes_a_lane() {
        let mut c = Conversation::new();
        c.apply(started("t1", "Reader"));
        c.apply(started("t2", "Tester"));
        let lanes = c.lanes();
        assert_eq!(lanes.len(), 2, "in start order");
        assert_eq!(lanes[0].id, "t1");
        assert_eq!(lanes[0].label, "Reader");
        assert_eq!(lanes[0].kind, "Explore");
        assert_eq!(lanes[0].task, "Find the config loader");
        assert!(lanes[0].controls.stop);
        assert_eq!(lanes[0].status, LaneStatus::Working);
        assert_eq!(c.lane("t2").map(|l| l.label.as_str()), Some("Tester"));
        // Started again (a resend): still one lane.
        c.apply(started("t1", "Reader"));
        assert_eq!(c.lanes().len(), 2);
    }

    #[test]
    fn progress_fills_the_lanes_stats() {
        let mut c = Conversation::new();
        c.apply(started("t1", "Reader"));
        c.apply(Event::SubagentProgress {
            id: "t1".into(),
            tokens: Some(18_200),
            tools: Some(12),
            last_tool: Some("Grep".into()),
        });
        let lane = c.lane("t1").unwrap();
        assert_eq!(lane.tokens, Some(18_200));
        assert_eq!(lane.tools, Some(12));
        assert_eq!(lane.last_tool.as_deref(), Some("Grep"));
    }

    #[test]
    fn lane_steps_counts_its_tools() {
        let mut c = Conversation::new();
        c.apply(started("t1", "Reader"));
        c.apply(sub_tool("r1", "t1"));
        c.apply(sub_tool("r2", "t1"));
        c.apply(sub_tool("r3", "other"));
        assert_eq!(c.lane_steps("t1"), 2);
        assert_eq!(c.lane_steps("nobody"), 0);
    }

    #[test]
    fn a_late_done_replaces_stopped() {
        let mut c = Conversation::new();
        c.apply(started("t1", "Reader"));
        c.apply(Event::SubagentEnded {
            id: "t1".into(),
            end: LaneEnd::Stopped,
        });
        c.apply(Event::SubagentEnded {
            id: "t1".into(),
            end: LaneEnd::Done,
        });
        assert_eq!(c.lane("t1").unwrap().status, LaneStatus::Done);
    }

    #[test]
    fn clearing_drops_the_lanes() {
        let mut c = Conversation::new();
        c.apply(started("t1", "Reader"));
        c.apply(Event::Cleared);
        assert!(c.lanes().is_empty(), "/clear");
        c.apply(started("t2", "Tester"));
        c.keep_first(0);
        assert!(c.lanes().is_empty(), "a failed resume");
    }

    #[test]
    fn settling_a_turn_leaves_a_working_lanes_steps() {
        let mut c = Conversation::new();
        c.you("go".into(), Vec::new());
        c.apply(started("t1", "Reader"));
        c.apply(sub_tool("r1", "t1"));
        c.apply(Event::Permission {
            id: "p1".into(),
            tool: "Bash".into(),
            what: "ls".into(),
            detail: String::new(),
            always: None,
            by: Some("t1".into()),
        });
        c.apply(Event::TurnDone {
            ok: true,
            message: None,
        });
        let running = |c: &Conversation| {
            c.items()
                .iter()
                .any(|i| matches!(i, Item::Tool { id, running: true, .. } if id == "r1"))
        };
        let open = |c: &Conversation| {
            c.items()
                .iter()
                .any(|i| matches!(i, Item::Permission { answer: None, .. }))
        };
        assert!(
            running(&c),
            "a background lane's step goes on after Main's turn"
        );
        assert!(open(&c), "and its question stays open");
        c.apply(Event::SubagentEnded {
            id: "t1".into(),
            end: LaneEnd::Done,
        });
        assert!(!running(&c) && !open(&c), "they settle when the lane ends");
    }

    #[test]
    fn an_end_sets_done_failed_or_stopped() {
        let mut c = Conversation::new();
        for id in ["a", "b", "c"] {
            c.apply(started(id, id));
        }
        c.apply(Event::SubagentEnded {
            id: "a".into(),
            end: LaneEnd::Done,
        });
        c.apply(Event::SubagentEnded {
            id: "b".into(),
            end: LaneEnd::Failed("no such file".into()),
        });
        c.apply(Event::SubagentEnded {
            id: "c".into(),
            end: LaneEnd::Stopped,
        });
        assert_eq!(c.lane("a").unwrap().status, LaneStatus::Done);
        assert_eq!(
            c.lane("b").unwrap().status,
            LaneStatus::Failed("no such file".into())
        );
        assert_eq!(c.lane("c").unwrap().status, LaneStatus::Stopped);
    }

    #[test]
    fn a_lanes_permission_makes_it_waiting() {
        let mut c = Conversation::new();
        c.apply(started("t1", "Reader"));
        c.apply(Event::Permission {
            id: "p1".into(),
            tool: "Bash".into(),
            what: "ls".into(),
            detail: String::new(),
            always: None,
            by: Some("t1".into()),
        });
        assert_eq!(c.lane("t1").unwrap().status, LaneStatus::Waiting);
        assert!(
            matches!(c.items().last(), Some(Item::Permission { by: Some(b), .. }) if b == "t1"),
            "the card knows who asks"
        );
        c.answered("p1", Answer::Allowed);
        assert_eq!(c.lane("t1").unwrap().status, LaneStatus::Working);
    }

    #[test]
    fn a_step_after_done_reopens_the_lane() {
        let mut c = Conversation::new();
        c.apply(started("t1", "Reader"));
        c.apply(Event::SubagentEnded {
            id: "t1".into(),
            end: LaneEnd::Done,
        });
        c.apply(sub_tool("r1", "t1"));
        assert_eq!(c.lane("t1").unwrap().status, LaneStatus::Working);
    }

    #[test]
    fn process_end_stops_working_lanes() {
        for end in 0..3 {
            let mut c = Conversation::new();
            c.apply(started("w", "Working"));
            c.apply(started("d", "Done"));
            c.apply(Event::SubagentEnded {
                id: "d".into(),
                end: LaneEnd::Done,
            });
            match end {
                0 => c.process_ended(Some(1), ""),
                1 => c.paused(),
                _ => c.paused_with("held"),
            }
            assert_eq!(c.lane("w").unwrap().status, LaneStatus::Stopped, "{end}");
            assert_eq!(c.lane("d").unwrap().status, LaneStatus::Done, "{end}");
        }
    }

    #[test]
    fn a_gone_process_stops_its_lanes_and_settles_them() {
        // Review 1: Force stop or a restart (no `process_ended`): nothing of its lanes spins or
        // waits on.
        let mut c = Conversation::new();
        c.you("go".into(), Vec::new());
        c.apply(started("t1", "Reader"));
        c.apply(sub_tool("r1", "t1"));
        c.apply(Event::Permission {
            id: "p1".into(),
            tool: "Bash".into(),
            what: "ls".into(),
            detail: String::new(),
            always: None,
            by: Some("t1".into()),
        });
        c.process_gone();
        assert_eq!(c.lane("t1").unwrap().status, LaneStatus::Stopped);
        assert!(c.pending_permission().is_none(), "its question expired");
        assert!(
            !c.items()
                .iter()
                .any(|i| matches!(i, Item::Tool { running: true, .. }))
        );
    }

    #[test]
    fn a_resumed_lane_without_its_result_is_stopped() {
        // Review 2: the app quit mid-subagent: no result in the transcript.
        let mut c = Conversation::new();
        c.past(vec![Past::Event(started("t1", "Reader"))]);
        assert_eq!(c.lane("t1").unwrap().status, LaneStatus::Stopped);
    }

    /// Batch 3, chore 2: a message sent under an id keeps it; the agent's echo of that id marks
    /// it read (only then can it be rewound or forked from); an echo with no id marks the oldest
    /// unread one.
    #[test]
    fn a_message_keeps_its_id_and_its_echo_marks_it_taken() {
        let mut c = Conversation::new();
        c.you_as("first".into(), Vec::new(), Some("u1".into()));
        c.you_as("second".into(), Vec::new(), Some("u2".into()));
        let taken = |c: &Conversation| -> Vec<(Option<String>, bool)> {
            c.items()
                .iter()
                .filter_map(|i| match i {
                    Item::You { id, taken, .. } => Some((id.clone(), *taken)),
                    _ => None,
                })
                .collect()
        };
        assert_eq!(
            taken(&c),
            [(Some("u1".into()), false), (Some("u2".into()), false)]
        );
        c.apply(Event::Taken {
            id: Some("u2".into()),
        });
        assert_eq!(
            taken(&c),
            [(Some("u1".into()), false), (Some("u2".into()), true)]
        );
        c.apply(Event::Taken { id: None });
        assert_eq!(
            taken(&c),
            [(Some("u1".into()), true), (Some("u2".into()), true)]
        );
        // The plain `you` has no id (an agent that takes none).
        c.you("third".into(), Vec::new());
        assert!(matches!(
            c.items().last(),
            Some(Item::You {
                id: None,
                taken: false,
                ..
            })
        ));
    }

    #[test]
    fn history_messages_are_taken_with_their_ids() {
        let mut c = Conversation::new();
        c.past(vec![
            Past::You {
                text: "old".into(),
                id: Some("h1".into()),
            },
            Past::Event(said("ok")),
        ]);
        assert!(matches!(
            &c.items()[0],
            Item::You { text, id: Some(id), taken: true, .. } if text == "old" && id == "h1"
        ));
    }

    fn agent_call(id: &str) -> Event {
        Event::ToolStarted {
            id: id.into(),
            name: "Agent".into(),
            summary: "Explore".into(),
            parent: None,
            detail: None,
            writes: Vec::new(),
            texts: Vec::new(),
        }
    }

    /// Batch 3, chore 2: Rewind cuts the conversation before one of your messages: it and all
    /// after it go (handed back, for Undo Rewind), the lanes started before it stay, your
    /// latest message is the one before, and nothing runs or waits.
    #[test]
    fn cut_before_drops_the_message_and_what_came_after() {
        let mut c = Conversation::new();
        c.you_as("one".into(), Vec::new(), Some("u1".into()));
        c.apply(agent_call("t1"));
        c.apply(started("t1", "Reader"));
        c.apply(turn_done());
        c.you_as("two".into(), vec!["a.rs".into()], Some("u2".into()));
        c.apply(agent_call("t2"));
        c.apply(started("t2", "Tester"));
        c.apply(said("working"));
        let ix = c.latest_you().expect("two");
        assert_eq!(ix, 2);
        let before = c.items()[..ix].to_vec();
        let cut = c.cut_before(ix).expect("a message of yours");
        assert_eq!(c.items(), &before[..]);
        assert_eq!(c.latest_you(), Some(0));
        assert_eq!(
            c.lanes().iter().map(|l| l.id.as_str()).collect::<Vec<_>>(),
            ["t1"]
        );
        assert_eq!(c.status(), Status::Done, "nothing runs on");
        assert!(matches!(
            cut.message(),
            Some(Item::You { text, attachments, .. }) if text == "two" && attachments == &["a.rs"]
        ));
        assert_eq!(cut.items().len(), 3);
        // Undo Rewind: all of it back, as it was.
        c.restore(cut);
        assert_eq!(c.items().len(), 5);
        assert_eq!(c.latest_you(), Some(2));
        assert_eq!(c.lanes().len(), 2);
        // Only a message of yours is a place to cut.
        assert!(c.cut_before(1).is_none());
        assert!(c.cut_before(99).is_none());
        assert_eq!(c.items().len(), 5, "nothing cut");
        // Cutting the first message leaves nothing: Idle.
        let _ = c.cut_before(0).expect("one");
        assert!(c.items().is_empty() && c.lanes().is_empty());
        assert_eq!((c.latest_you(), c.status()), (None, Status::Idle));
    }

    fn said(text: &str) -> Event {
        Event::TextDelta {
            message: "m2".into(),
            text: text.into(),
            parent: None,
        }
    }

    fn turn_done() -> Event {
        Event::TurnDone {
            ok: true,
            message: None,
        }
    }

    #[test]
    fn a_note_read_in_the_same_turn_leaves_main_done() {
        // Your report (2026-11-22): a message sent mid-turn read in that same turn ("Claude
        // reads it after this step") gets no `result` of its own: Main must not stay Working.
        let mut c = Conversation::new();
        c.you("first".into(), Vec::new());
        c.apply(Event::Taken { id: None });
        c.you("a note".into(), Vec::new());
        c.apply(Event::Taken { id: None });
        c.apply(turn_done());
        assert_eq!(c.status(), Status::Done);
    }

    #[test]
    fn a_queued_message_works_again_when_its_turn_starts() {
        let mut c = Conversation::new();
        c.you("first".into(), Vec::new());
        c.apply(Event::Taken { id: None });
        c.you("queued".into(), Vec::new());
        c.apply(turn_done());
        assert_eq!(
            c.status(),
            Status::Working,
            "not read yet: its turn is to come"
        );
        c.apply(Event::Taken { id: None });
        c.apply(said("on it"));
        c.apply(turn_done());
        assert_eq!(c.status(), Status::Done);
    }

    #[test]
    fn a_lanes_steps_after_main_finished_dont_make_main_work() {
        let mut c = Conversation::new();
        c.you("go".into(), Vec::new());
        c.apply(started("bg", "Watch"));
        c.apply(turn_done());
        c.apply(sub_tool("r1", "bg"));
        assert_eq!(c.status(), Status::Done, "Main's own status");
    }

    #[test]
    fn a_lane_with_another_open_question_still_waits() {
        let mut c = Conversation::new();
        c.apply(started("t1", "Reader"));
        for id in ["p1", "p2"] {
            c.apply(Event::Permission {
                id: id.into(),
                tool: "Bash".into(),
                what: "ls".into(),
                detail: String::new(),
                always: None,
                by: Some("t1".into()),
            });
        }
        c.answered("p1", Answer::Allowed);
        assert_eq!(
            c.lane("t1").unwrap().status,
            LaneStatus::Waiting,
            "p2 still open"
        );
        c.answered("p2", Answer::Allowed);
        assert_eq!(c.lane("t1").unwrap().status, LaneStatus::Working);
    }

    #[test]
    fn a_late_step_doesnt_revive_a_stopped_lane() {
        // Review 3: a stopped lane's lagging lines; a done one woken again does reopen.
        let mut c = Conversation::new();
        c.apply(started("t1", "Reader"));
        c.apply(Event::SubagentEnded {
            id: "t1".into(),
            end: LaneEnd::Stopped,
        });
        c.apply(sub_tool("late", "t1"));
        assert_eq!(c.lane("t1").unwrap().status, LaneStatus::Stopped);
    }

    #[test]
    fn subagent_text_keeps_its_parent() {
        let mut c = Conversation::new();
        c.apply(Event::TextDelta {
            message: "m".into(),
            text: "hi".into(),
            parent: Some("toolu_p".into()),
        });
        c.apply(Event::MessageDone {
            message: "m".into(),
            text: "hi!".into(),
            parent: Some("toolu_p".into()),
        });
        c.apply(done("main", "ok"));
        assert!(matches!(&c.items()[0],
            Item::Agent { parent: Some(p), text, done: true, .. } if p == "toolu_p" && text == "hi!"));
        assert!(matches!(&c.items()[1], Item::Agent { parent: None, .. }));
    }

    #[test]
    fn a_second_text_block_does_not_overwrite_the_first() {
        let mut c = Conversation::new();
        c.apply(delta("m", "one"));
        c.apply(done("m", "one"));
        c.apply(delta("m", "two"));
        c.apply(done("m", "two"));
        // Without streaming, a second finished block also stands alone.
        c.apply(done("n", "a"));
        c.apply(done("n", "b"));
        let texts: Vec<&str> = c
            .items()
            .iter()
            .filter_map(|i| match i {
                Item::Agent { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(texts, ["one", "two", "a", "b"]);
    }

    #[test]
    fn a_turn_end_finishes_a_half_streamed_message() {
        let mut c = Conversation::new();
        c.you("x".into(), vec![]);
        c.apply(delta("m", "partial"));
        c.apply(Event::TurnDone {
            ok: false,
            message: None,
        });
        assert!(matches!(&c.items()[1], Item::Agent { done: true, .. }));
    }

    /// `/clear` (Claude's `conversation_reset`): what was said, its cost and context go; the
    /// conversation id stays until the agent reports the new one.
    #[test]
    fn cleared_empties_the_conversation() {
        let mut c = Conversation::new();
        c.apply(Event::Started {
            conversation: "old".into(),
            model: Some("m".into()),
        });
        c.you("hello".into(), Vec::new());
        c.apply(Event::Notice("note".into()));
        c.apply(Event::Usage {
            context_pct: Some(40),
            cost_usd: Some(1.5),
        });
        c.apply(Event::Cleared);
        assert!(c.items().is_empty());
        assert_eq!(c.cost_usd(), None);
        assert_eq!(c.context_pct(), None);
        assert_eq!(c.conversation(), Some("old"));
    }

    /// Your latest message is kept by index as items come (the chat's latest-prompt strip,
    /// chore after M11ar): no scan of the conversation per frame.
    #[test]
    fn the_latest_you_follows_what_is_pushed() {
        let mut c = Conversation::new();
        assert_eq!(c.latest_you(), None, "nothing said yet");
        c.apply(Event::Notice("note".into()));
        assert_eq!(c.latest_you(), None, "a notice is not you");
        c.you("first".into(), Vec::new());
        assert_eq!(c.latest_you(), Some(1));
        c.apply(said("reply"));
        c.apply(turn_done());
        assert_eq!(c.latest_you(), Some(1), "the agent's reply leaves it");
        c.you("second".into(), Vec::new());
        assert_eq!(c.latest_you(), Some(3));
        // A failed resume keeps only what came before it: back to the one still there.
        c.keep_first(3);
        assert_eq!(c.latest_you(), Some(1));
        c.keep_first(1);
        assert_eq!(c.latest_you(), None);
        // History: its last message of yours.
        c.past(vec![Past::you("old one"), Past::you("old two")]);
        assert_eq!(c.latest_you(), Some(2));
        c.apply(Event::Cleared);
        assert_eq!(c.latest_you(), None, "`/clear` takes it with the rest");
    }
}
