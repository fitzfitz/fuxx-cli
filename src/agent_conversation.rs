//! The conversation model (M11aa): folds a provider's `Event`s into chat items and a status.
//! Pure and GPUI-free.

use crate::agent_activity::{Activity, announces_failure};
use crate::providers::{
    Event, LaneControls, LaneEnd, Past, QAnswer, Question, TodoItem, ToolDetail,
};
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
    /// The agent stopped waiting before you answered (it withdrew the ask).
    Withdrawn,
}

/// How a question of the agent's ended (batch 3, chore 8).
#[derive(Clone, Debug, PartialEq)]
pub enum Reply {
    /// Your answers, one per question (empty: answered elsewhere, or in a past session).
    Answered(Vec<QAnswer>),
    Skipped,
    /// The turn ended (or the process died, or its tool failed) before you answered.
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
        /// For a subagent (the lanes chore): Main is to pass it on, or answer about it.
        to: Option<Relay>,
    },
    /// Subagent `lane` runs again (the lanes chore): its last run `ended` so (a divider in its
    /// lane), and what Main `said` to it; `run` is the run it starts (2 for the first resume).
    Resumed {
        lane: String,
        ended: LaneStatus,
        said: Option<String>,
        run: u32,
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
        /// What the call does (the agent activity chore): its provider's, set at its start and
        /// replaced at its finish; None until the provider sends one.
        activity: Option<Box<Activity>>,
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
        /// The plan it asks to approve (the 2026-11-25 chores); its card shows it.
        plan: Option<Box<crate::providers::PlanDraft>>,
    },
    /// The agent asks you to choose (batch 3, chore 8): open while `reply` is None.
    Question {
        id: String,
        questions: Vec<Question>,
        reply: Option<Reply>,
        /// The subagent (lane id) that asks; `None`: the main agent.
        by: Option<String>,
    },
    Notice(String),
    Error(String),
}

/// A message of yours for a subagent (the lanes chore, 2026-11-24): sent to Main, which passes
/// it on word for word (a relay), or answers about it (`about`, a lane it must not resume).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Relay {
    pub lane: String,
    pub about: bool,
    pub state: RelayState,
    /// Its place in its lane's rows once it settled (`items.len()` then): from then on it
    /// never moves; until then it is the lane's last row.
    pub settled_at: Option<usize>,
}

/// Where a relay is (`Relay::state`); read with `Item::You::taken` (held or taken).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum RelayState {
    /// Sent to Main, not passed on yet.
    Sent,
    /// Main's SendMessage to its subagent was seen.
    Delivered,
    /// Main's turn ended without passing it on.
    NotPassed,
    /// Its subagent finished before Main passed it on.
    LaneEnded,
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
    /// How many times it has started: 1, and one more for each resume (the lanes chore).
    pub runs: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub enum LaneStatus {
    Working,
    /// It asked you something (a permission) and waits.
    Waiting,
    Done,
    Failed(String),
    Stopped,
    /// Its source stopped with no end said (the agent exited, a history cut off): it ended
    /// without a report (the lanes chore; never left spinning).
    Ended,
}

impl LaneStatus {
    /// Still going (working or waiting on you).
    pub fn live(&self) -> bool {
        matches!(self, LaneStatus::Working | LaneStatus::Waiting)
    }
}

#[derive(Clone, Default)]
pub struct Conversation {
    /// Changed only through `Items`' methods: each change is noted (the perf chore).
    items: crate::agent_items::Items,
    /// Your latest message's index (the chat's latest-prompt strip, chore after M11ar): kept as
    /// items come, so nothing scans the conversation for it.
    latest_you: Option<usize>,
    /// The agent's todo list's index (it is updated in place), for the plan tab (the 2026-11-24
    /// chores): read without a scan.
    todo_at: Option<usize>,
    /// The open questions' indexes, oldest first (batch 3, chore 8): kept as items come and
    /// rebuilt after anything moves them, so `status` scans nothing for them.
    open_asks: Vec<usize>,
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
    /// Main's tool calls that delivered a relay, and the lane each went to (the lanes chore).
    passed_on: std::collections::HashMap<String, String>,
    /// The agent's questions are answered by a message (`Provider::questions_by_message`:
    /// agy, 2026-11-24): it went on without waiting, so an open one waits on nothing, and
    /// only you close it (an answer, a skip, another message).
    by_message: bool,
    /// Where the restored history ends (`items.len()` after `past`; 0: none restored): the
    /// agent activity chore's ledger shows only for a live turn.
    live_from: usize,
    /// Bumped whenever the lanes or the relays passed on change (the perf chore): which rows
    /// each view has and which tool rows stand alone follow them, so the tab's caches start
    /// again; the pill labels too.
    shape: u64,
    /// The background shells (chore 16): the shells tab and its popover.
    shells: crate::agent_shells::Shells,
}

/// What `Conversation::cut_before` cut (batch 3, chore 2): kept for Undo Rewind until the next
/// send.
#[derive(Clone, Debug, PartialEq)]
#[allow(dead_code)] // Batch 3 wave 2 (the Rewind/Fork UI) calls it.
pub struct Cut {
    /// Where it was cut (`restore` drops what came after it first).
    at: usize,
    items: Vec<Item>,
    lanes: Vec<Lane>,
    lane_starts: Vec<usize>,
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

    /// The agent's questions are answered by a message (`by_message`).
    pub fn set_questions_by_message(&mut self, on: bool) {
        self.by_message = on;
    }

    /// The open questions are answered by a message (the card sends your picks as one).
    pub fn questions_by_message(&self) -> bool {
        self.by_message
    }

    /// Your message (not the card's) while a question the agent didn't wait on is open: it is
    /// left not answered (`Reply::Expired`).
    fn passed_over(&mut self) {
        if !self.by_message {
            return;
        }
        let open: Vec<String> = self
            .open_asks
            .iter()
            .filter_map(|&ix| match &self.items[ix] {
                Item::Question { id, .. } => Some(id.clone()),
                _ => None,
            })
            .collect();
        for id in open {
            self.reply(&id, Reply::Expired);
        }
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
                self.shape += 1;
                self.items.clear();
                self.open_asks.clear();
                self.latest_you = None;
                self.lanes.clear();
                self.lane_starts.clear();
                self.passed_on.clear();
                self.cost_usd = None;
                self.context_pct = None;
                self.finished = false;
                self.live_from = 0;
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
                match self.open_text(&message) {
                    Some(ix) => {
                        if let Item::Agent { text: t, .. } = &mut *self.items.get_mut(ix) {
                            t.push_str(&text);
                        }
                    }
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
            } => match self.open_text(&message) {
                Some(ix) => {
                    if let Item::Agent { text: t, done, .. } = &mut *self.items.get_mut(ix) {
                        *t = text;
                        *done = true;
                    }
                }
                None => self.items.push(Item::Agent {
                    message,
                    text,
                    done: true,
                    parent,
                }),
            },
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
                    activity: None,
                })
            }
            Event::Activity { id, activity } => self.set_activity(&id, activity),
            Event::ToolFinished {
                id,
                ok,
                summary,
                output,
            } => {
                let call = self
                    .items
                    .iter()
                    .position(|i| matches!(i, Item::Tool { id: t, .. } if *t == id));
                if let Some(ix) = call {
                    if let Item::Tool {
                        running,
                        ok: tok,
                        result,
                        output: tout,
                        dropped_lines,
                        ..
                    } = &mut *self.items.get_mut(ix)
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
                    }
                    return;
                }
                // A question's own tool (batch 3, chore 8) has no row: its result closes it.
                let asked = self.open_asks.iter().copied().find(
                    |&ix| matches!(&self.items[ix], Item::Question { id: q, .. } if *q == id),
                );
                if asked.is_some() {
                    let reply = if ok {
                        Reply::Answered(Vec::new())
                    } else {
                        Reply::Expired
                    };
                    self.reply(&id, reply);
                }
            }
            // A restored question's reply (the design review): what you chose, or skipped.
            Event::QuestionReplied { id, answers } => {
                let open = self
                    .open_asks
                    .iter()
                    .any(|&ix| matches!(&self.items[ix], Item::Question { id: q, .. } if *q == id));
                if open {
                    let reply = match answers {
                        Some(a) => Reply::Answered(a),
                        None => Reply::Skipped,
                    };
                    self.reply(&id, reply);
                }
            }
            Event::ShellStarted {
                id,
                call,
                description,
                command,
                output,
            } => {
                self.touch_call(&call);
                self.shells.start(crate::agent_shells::Shell {
                    id,
                    call,
                    description,
                    command,
                    output,
                    started: std::time::Instant::now(),
                    ended: None,
                    last_line: None,
                });
            }
            Event::ShellEnded { id, end, output } => {
                let now = std::time::Instant::now();
                if let Some(call) = self
                    .shells
                    .end(&id, end, output, now)
                    .map(|s| s.call.clone())
                {
                    self.touch_call(&call);
                }
            }
            Event::Todo(list) => match self.items.iter().position(|i| matches!(i, Item::Todo(_))) {
                Some(ix) => {
                    *self.items.get_mut(ix) = Item::Todo(list);
                    self.todo_at = Some(ix);
                }
                None => {
                    self.todo_at = Some(self.items.len());
                    self.items.push(Item::Todo(list));
                }
            },
            Event::Permission {
                id,
                tool,
                what,
                detail,
                always,
                by,
                plan,
            } => {
                self.mark_waiting(by.as_deref());
                self.items.push(Item::Permission {
                    id,
                    tool,
                    what,
                    detail,
                    always,
                    answer: None,
                    by,
                    plan: plan.map(Box::new),
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
                    runs: 1,
                };
                self.shape += 1;
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
                    // A relay Main hadn't passed on yet came too late.
                    self.settle_relays(|r, _| r.lane == id, RelayState::LaneEnded);
                }
            }
            // It runs again (the lanes chore): in its own lane, its place and label kept, its
            // old end a divider there.
            Event::SubagentResumed { id, controls, said } => {
                if let Some(lane) = self.lane_mut(&id) {
                    let ended = std::mem::replace(&mut lane.status, LaneStatus::Working);
                    lane.controls = controls;
                    lane.runs += 1;
                    let run = lane.runs;
                    self.items.push(Item::Resumed {
                        lane: id,
                        ended,
                        said,
                        run,
                    });
                }
            }
            // Main messaged a subagent: the oldest relay to it waiting is delivered.
            Event::PassedOn { call, lane } => {
                let at = self.items.len();
                let waiting = self.items.iter().position(|i| {
                    matches!(i, Item::You { to: Some(r), .. }
                        if r.lane == lane && !r.about && r.state == RelayState::Sent)
                });
                if let Some(ix) = waiting {
                    if let Item::You { to: Some(r), .. } = &mut *self.items.get_mut(ix) {
                        r.state = RelayState::Delivered;
                        r.settled_at = Some(at);
                    }
                    self.passed_on.insert(call, lane);
                    self.shape += 1;
                }
            }
            Event::Question { id, questions, by } => {
                self.mark_waiting(by.as_deref());
                self.open_asks.push(self.items.len());
                self.items.push(Item::Question {
                    id,
                    questions,
                    reply: None,
                    by,
                })
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
            // The tab acts on it (`cancel_unread`).
            Event::QueuedCancelled { .. } => {}
            // The agent stopped waiting (Claude's `control_cancel_request`): the card settles
            // now, not at the turn's end. One answered already keeps its answer.
            Event::AskWithdrawn { id } => {
                let open_permission = self
                    .items
                    .iter()
                    .any(|i| matches!(i, Item::Permission { id: p, answer: None, .. } if *p == id));
                if open_permission {
                    self.answered(&id, Answer::Withdrawn);
                } else {
                    self.reply(&id, Reply::Expired);
                }
            }
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
                // A relay Main read in this turn (or any, when the turn failed: what was
                // queued is dropped) and didn't pass on.
                self.settle_relays(|_, taken| taken || !ok, RelayState::NotPassed);
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
            | Event::Mode(_)
            | Event::ModeRefused { .. } => {}
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

    /// Message `message`'s block still streaming (the newest), if any.
    fn open_text(&self, message: &str) -> Option<usize> {
        self.items.open_text(message)
    }

    pub fn you(&mut self, text: String, attachments: Vec<String>) {
        self.you_as(text, attachments, None);
    }

    /// `you`, the message sent under `id` (`Provider::fresh_message_id`; batch 3, chore 2).
    pub fn you_as(&mut self, text: String, attachments: Vec<String>, id: Option<String>) {
        self.you_to(text, attachments, id, None);
    }

    /// Your `text` for subagent `lane`, sent to Main (the lanes chore): to pass on, or
    /// (`about`) to answer about it. A message to Main in every other way.
    pub fn relay(&mut self, text: String, id: Option<String>, lane: String, about: bool) {
        let to = Relay {
            lane,
            about,
            state: RelayState::Sent,
            settled_at: None,
        };
        self.you_to(text, Vec::new(), id, Some(to));
    }

    fn you_to(
        &mut self,
        text: String,
        attachments: Vec<String>,
        id: Option<String>,
        to: Option<Relay>,
    ) {
        self.passed_over();
        self.shells.you_sent();
        self.latest_you = Some(self.items.len());
        self.items.push(Item::You {
            text,
            attachments,
            id,
            taken: false,
            to,
        });
        self.outstanding += 1;
        self.ended = false;
        self.finished = false;
    }

    /// Your message `id`, not read yet, cancelled (batch 3, wave 3: the agent dropped it from
    /// its queue, or fuxx never wrote it): it goes, and no turn is waited on for it. False (and
    /// nothing changes) when there is no such unread message.
    pub fn cancel_unread(&mut self, id: &str) -> bool {
        let Some(ix) = self
            .items
            .iter()
            .rposition(|i| matches!(i, Item::You { id: Some(x), taken: false, .. } if x == id))
        else {
            return false;
        };
        self.items.remove(ix);
        for at in &mut self.lane_starts {
            if *at > ix {
                *at -= 1;
            }
        }
        let after = |i: &Item| matches!(i, Item::You { to: Some(Relay { settled_at: Some(at), .. }), .. } if *at > ix);
        self.items.change_where(after, |i| {
            if let Item::You {
                to:
                    Some(Relay {
                        settled_at: Some(at),
                        ..
                    }),
                ..
            } = i
            {
                *at -= 1;
            }
        });
        self.rebuild_asks();
        self.latest_you = self
            .items
            .iter()
            .rposition(|i| matches!(i, Item::You { .. }));
        self.outstanding = self.outstanding.saturating_sub(1);
        true
    }

    /// Your message `id` was read (none said: the oldest not read yet). Looked for from the
    /// newest back when named (it is nearly always the last), from the oldest when not.
    fn mark_taken(&mut self, id: Option<&str>) {
        let unread = |i: &Item| matches!(i, Item::You { taken: false, .. });
        let found = match id {
            Some(id) => self
                .items
                .iter()
                .rposition(|i| matches!(i, Item::You { id: Some(x), .. } if x == id)),
            None => self.items.iter().position(unread),
        };
        if let Some(ix) = found
            && let Item::You { taken, .. } = &mut *self.items.get_mut(ix)
        {
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
                        to: None,
                    })
                }
                Past::Relay {
                    text,
                    id,
                    to,
                    about,
                } => {
                    self.relay(text, id, to, about);
                    self.mark_taken(None);
                }
                Past::Event(event) => self.apply(event),
            }
        }
        self.outstanding = 0;
        self.busy = false;
        // A relay never passed on in the history stays so.
        self.settle_relays(|_, _| true, RelayState::NotPassed);
        // A subagent with no end in the history was cut off (the app quit mid-run).
        self.stop_lanes(LaneStatus::Ended);
        self.settle();
        self.live_from = self.items.len();
    }

    /// Tool call `id`'s activity (the agent activity chore). A finished check that failed is
    /// "as planned" when the call's own words or the narration before its run announced it.
    fn set_activity(&mut self, id: &str, mut activity: Activity) {
        let Some(ix) = self
            .items
            .iter()
            .rposition(|i| matches!(i, Item::Tool { id: t, .. } if t == id))
        else {
            return;
        };
        let finished = matches!(&self.items[ix], Item::Tool { running: false, .. });
        if finished && activity.check_failed() {
            let announced = activity.said.as_deref().is_some_and(announces_failure)
                || self.narration_before(ix).is_some_and(announces_failure);
            // Only the first red run of a check in the run is the announced one: the same check
            // failing again after it is a real failure.
            let again = activity.check.as_ref().is_some_and(|check| {
                self.items[self.run_start(ix)..ix].iter().any(|i| match i {
                    Item::Tool {
                        activity: Some(a), ..
                    } => {
                        a.check_failed()
                            && a.check
                                .as_ref()
                                .is_some_and(|c| c.what == check.what && c.scope == check.scope)
                    }
                    _ => false,
                })
            });
            activity.planned_failure = activity.planned_failure || announced && !again;
        }
        if let Item::Tool { activity: slot, .. } = &mut *self.items.get_mut(ix) {
            *slot = Some(Box::new(activity));
        }
    }

    /// The agent's text just before the run of tool calls holding item `ix`, from the same
    /// agent (Main's, or the same lane's); None when your message comes first.
    pub fn narration_before(&self, ix: usize) -> Option<&str> {
        match self.run_head(ix)? {
            (at, true) => match &self.items[at] {
                Item::Agent { text, .. } => Some(text),
                _ => None,
            },
            _ => None,
        }
    }

    /// Where the run of tool calls holding item `ix` starts: just after its narration, your
    /// message or its lane's resume.
    fn run_start(&self, ix: usize) -> usize {
        self.run_head(ix).map_or(0, |(at, _)| at + 1)
    }

    /// The item that leads the run of tool calls holding item `ix`, and whether it is the
    /// agent's narration (else your message or the lane's resume); None for a non-tool or a run
    /// from the start.
    fn run_head(&self, ix: usize) -> Option<(usize, bool)> {
        let Some(Item::Tool { parent, .. }) = self.items.get(ix) else {
            return None;
        };
        self.items[..ix]
            .iter()
            .enumerate()
            .rev()
            .find_map(|(at, item)| match item {
                Item::Agent { parent: p, .. } if p == parent => Some((at, true)),
                Item::You { .. } => Some((at, false)),
                Item::Resumed { lane, .. } if parent.as_deref() == Some(lane.as_str()) => {
                    Some((at, false))
                }
                _ => None,
            })
    }

    /// Where the restored history ends (0: none restored).
    pub fn live_from(&self) -> usize {
        self.live_from
    }

    /// The agent couldn't be started: one error row, and the conversation is over (Done).
    pub fn failed_to_start(&mut self, message: &str) {
        self.ended = true;
        self.outstanding = 0;
        self.busy = false;
        self.stop_lanes(LaneStatus::Stopped);
        self.settle();
        self.items.push(Item::Error(message.to_string()));
    }

    /// Keep only the first `len` items (a resume that failed: what that run said goes, and a
    /// new conversation follows what came before). Nothing is left running or waiting.
    pub fn keep_first(&mut self, len: usize) {
        self.items.truncate(len);
        self.live_from = self.live_from.min(len);
        // Rare (a failed resume): the one still there is looked for once.
        if self.latest_you.is_some_and(|i| i >= len) {
            self.latest_you = self
                .items
                .iter()
                .rposition(|i| matches!(i, Item::You { .. }));
        }
        self.lanes.clear();
        self.lane_starts.clear();
        self.shape += 1;
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
        self.live_from = self.live_from.min(ix);
        self.shape += 1;
        let cut = Cut {
            at: ix,
            items: self.items.split_off(ix),
            lanes: self.lanes.split_off(keep),
            lane_starts: self.lane_starts.split_off(keep),
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

    /// Fork (batch 3, chore 2): a copy of this conversation cut before your message at `ix`,
    /// with `head` as its first row and no conversation id of its own (the fork's comes when it
    /// starts). None unless `ix` is a message of yours. This one is left as it is.
    pub fn forked_before(&self, ix: usize, head: Item) -> Option<Conversation> {
        let mut fork = self.clone();
        fork.cut_before(ix)?;
        fork.items.insert(0, head);
        for at in &mut fork.lane_starts {
            *at += 1;
        }
        fork.rebuild_asks();
        fork.latest_you = fork.latest_you.map(|i| i + 1);
        fork.conversation = None;
        fork.cost_usd = None;
        fork.context_pct = None;
        Some(fork)
    }

    /// Undo Rewind (or a rewind that failed): what `cut_before` cut, back after what it kept;
    /// what was added since the cut (its "Rewound" notice, a send that failed) goes first. Lanes
    /// started since go too.
    pub fn restore(&mut self, cut: Cut) {
        self.items.truncate(cut.at);
        let keep = self
            .lane_starts
            .iter()
            .take_while(|&&at| at <= cut.at)
            .count();
        self.lanes.truncate(keep);
        self.lane_starts.truncate(keep);
        self.items.extend(cut.items);
        self.lanes.extend(cut.lanes);
        self.lane_starts.extend(cut.lane_starts);
        self.shape += 1;
        // Found again, not taken from the cut: something before the cut may have gone since
        // (an unread message cancelled), and the turn starts after your latest (the perf
        // chore's property test found it).
        self.latest_you = self
            .items
            .iter()
            .rposition(|i| matches!(i, Item::You { .. }));
        self.rebuild_asks();
    }

    /// Notice `old` (its last one) says `new` instead, in its place; without one, `new` is added.
    pub fn replace_notice(&mut self, old: &str, new: &str) {
        let found = self
            .items
            .iter()
            .rposition(|i| matches!(i, Item::Notice(t) if t == old));
        match found {
            Some(ix) => *self.items.get_mut(ix) = Item::Notice(new.to_string()),
            None => self.items.push(Item::Notice(new.to_string())),
        }
    }

    pub fn answered(&mut self, id: &str, answer: Answer) {
        let mut asker = None;
        let found = self
            .items
            .iter()
            .position(|i| matches!(i, Item::Permission { id: pid, .. } if pid == id));
        if let Some(ix) = found
            && let Item::Permission { answer: a, by, .. } = &mut *self.items.get_mut(ix)
        {
            *a = Some(answer);
            asker = by.clone();
        }
        self.wake(asker.as_deref());
    }

    /// Question `id` answered, skipped or expired (batch 3, chore 8); one already closed keeps
    /// its reply.
    pub fn reply(&mut self, id: &str, reply: Reply) {
        let Some(k) = self
            .open_asks
            .iter()
            .position(|&ix| matches!(&self.items[ix], Item::Question { id: q, .. } if q == id))
        else {
            return;
        };
        let ix = self.open_asks.remove(k);
        let mut asker = None;
        if let Item::Question { reply: r, by, .. } = &mut *self.items.get_mut(ix) {
            *r = Some(reply);
            asker = by.clone();
        }
        self.wake(asker.as_deref());
    }

    /// Subagent `asker` works again once none of its questions (permissions or the agent's own
    /// questions) is open.
    fn wake(&mut self, asker: Option<&str>) {
        let Some(b) = asker else {
            return;
        };
        let still_asks = self.items.iter().any(|i| match i {
            Item::Permission {
                answer: None,
                by: Some(x),
                ..
            }
            | Item::Question {
                reply: None,
                by: Some(x),
                ..
            } => x == b,
            _ => false,
        });
        if !still_asks
            && let Some(lane) = self.lane_mut(b)
            && lane.status == LaneStatus::Waiting
        {
            lane.status = LaneStatus::Working;
        }
    }

    /// The open questions' indexes, from a scan: after anything moves items (rare).
    fn rebuild_asks(&mut self) {
        self.open_asks = self
            .items
            .iter()
            .enumerate()
            .filter(|(_, i)| matches!(i, Item::Question { reply: None, .. }))
            .map(|(ix, _)| ix)
            .collect();
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
        self.items.steps(id)
    }

    /// How many items hang under tool call `id` (`parent_of`: its subagent's words, steps and
    /// resumes).
    pub fn kids(&self, id: &str) -> usize {
        self.items.kids(id)
    }

    /// The items under tool call `id` (`parent_of`), in order.
    pub fn under(&self, id: &str) -> Option<&std::collections::BTreeSet<usize>> {
        self.items.under(id)
    }

    /// Your messages relayed to a lane, in order.
    pub fn relays(&self) -> &std::collections::BTreeSet<usize> {
        self.items.relays()
    }

    /// The permissions and questions lane `lane` asked, in order.
    pub fn asked_by(&self, lane: &str) -> Option<&std::collections::BTreeSet<usize>> {
        self.items.asked_by(lane)
    }

    /// The open questions' indexes, oldest first.
    pub fn open_asks(&self) -> &[usize] {
        &self.open_asks
    }

    /// The open permissions' indexes, oldest first.
    pub fn open_permissions(&self) -> &std::collections::BTreeSet<usize> {
        self.items.open_permissions()
    }

    /// Tool call `id`'s index (its first).
    pub fn tool_index(&self, id: &str) -> Option<usize> {
        self.items.tool_index(id)
    }

    /// There is a tool call `id`.
    pub fn has_tool(&self, id: &str) -> bool {
        self.items.has_tool(id)
    }

    /// The running tool calls' indexes, in order.
    pub fn running_tools(&self) -> &std::collections::BTreeSet<usize> {
        self.items.running()
    }

    /// Item `ix`'s version: bumped by every change to it (the perf chore).
    pub fn version(&self, ix: usize) -> u64 {
        self.items.version(ix)
    }

    /// The lowest item index changed since the last call (None: nothing changed): the tab
    /// derives its rows again from there.
    pub fn take_dirty(&mut self) -> Option<usize> {
        self.items.take_dirty()
    }

    /// Bumped when the lanes or the relays passed on change (`shape`).
    pub fn shape(&self) -> u64 {
        self.shape
    }

    /// Where the current turn starts: just after your latest message (0: none), as
    /// `agent_steps::turn_start` finds by a scan.
    pub fn turn_start(&self) -> usize {
        self.latest_you.map_or(0, |ix| ix + 1)
    }

    /// A step under a done lane whose agent wakes it by steps (agy): it works again. A stopped
    /// or failed one stays so (its lagging lines don't revive it), and so does any of Claude's
    /// (it says a resume itself: the lanes chore).
    fn reopen(&mut self, parent: Option<&str>) {
        if let Some(lane) = parent.and_then(|p| self.lane_mut(p))
            && lane.controls.woken_by_steps
            && lane.status == LaneStatus::Done
        {
            lane.status = LaneStatus::Working;
        }
    }

    /// Subagent `by` asks: it waits on you — only while it runs (an ask after its end changes
    /// nothing: the lanes chore).
    fn mark_waiting(&mut self, by: Option<&str>) {
        if let Some(lane) = by.and_then(|b| self.lane_mut(b))
            && lane.status.live()
        {
            lane.status = LaneStatus::Waiting;
        }
    }

    /// Each relay still waiting to be passed on that `which` picks (by it and whether Main has
    /// read it) settles `so`, where it is now.
    fn settle_relays(&mut self, which: impl Fn(&Relay, bool) -> bool, so: RelayState) {
        let at = self.items.len();
        let picked = |i: &Item| {
            matches!(i, Item::You { to: Some(r), taken, .. }
                if !r.about && r.state == RelayState::Sent && which(r, *taken))
        };
        self.items.change_where(picked, |i| {
            if let Item::You { to: Some(r), .. } = i {
                r.state = so.clone();
                r.settled_at = Some(at);
            }
        });
    }

    /// The lane Main's tool call `call` delivered a relay to (its row says so).
    pub fn passed_on(&self, call: &str) -> Option<&str> {
        self.passed_on.get(call).map(String::as_str)
    }

    /// Lane `id` ended: its steps stop running and its questions expire.
    fn settle_lane(&mut self, id: &str) {
        let under = |p: &Option<String>| p.as_deref() == Some(id);
        // Only what changes is touched (each change lays its row out again).
        let picked = |i: &Item| match i {
            Item::Tool {
                running: true,
                parent,
                ..
            } => under(parent),
            Item::Agent {
                done: false,
                parent,
                ..
            } => under(parent),
            Item::Permission {
                answer: None, by, ..
            }
            | Item::Question {
                reply: None, by, ..
            } => under(by),
            _ => false,
        };
        self.items.change_where(picked, |i| match i {
            Item::Tool { running, .. } => *running = false,
            Item::Agent { done, .. } => *done = true,
            Item::Permission { answer, .. } => *answer = Some(Answer::Expired),
            Item::Question { reply, .. } => *reply = Some(Reply::Expired),
            _ => {}
        });
        self.rebuild_asks();
    }

    /// The process was ended from here (Force stop, a restart for a change): its subagents
    /// stop, and nothing of theirs runs or waits on (no `process_ended` follows).
    pub fn process_gone(&mut self) {
        self.shells_gone();
        self.stop_lanes(LaneStatus::Stopped);
        self.settle();
    }

    /// The process is gone: every lane still going ends `so` (Stopped by fuxx, else Ended:
    /// no report came).
    fn stop_lanes(&mut self, so: LaneStatus) {
        for lane in &mut self.lanes {
            if lane.status.live() {
                lane.status = so.clone();
            }
        }
    }

    pub fn process_ended(&mut self, code: Option<i32>, stderr_tail: &str) {
        self.ended = true;
        self.outstanding = 0;
        self.busy = false;
        self.shells_gone();
        self.stop_lanes(LaneStatus::Ended);
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
        self.stop_lanes(LaneStatus::Stopped);
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
        let by_message = self.by_message;
        // Only what changes is touched (each change lays its row out again).
        let picked = |i: &Item| match i {
            Item::Tool {
                running: true,
                parent,
                ..
            } => !under_live(parent),
            Item::Agent {
                done: false,
                parent,
                ..
            } => !under_live(parent),
            Item::Permission {
                answer: None, by, ..
            } => !under_live(by),
            // One the agent didn't wait on stays open: only you close it.
            Item::Question {
                reply: None, by, ..
            } => !under_live(by) && !by_message,
            _ => false,
        };
        self.items.change_where(picked, |i| match i {
            Item::Tool { running, .. } => *running = false,
            Item::Agent { done, .. } => *done = true,
            Item::Permission { answer, .. } => *answer = Some(Answer::Expired),
            Item::Question { reply, .. } => *reply = Some(Reply::Expired),
            _ => {}
        });
        self.rebuild_asks();
    }

    pub fn items(&self) -> &[Item] {
        &self.items
    }

    /// Your latest message's index in `items`, if you said anything.
    pub fn latest_you(&self) -> Option<usize> {
        self.latest_you
    }

    /// The background shells (chore 16).
    pub fn shells(&self) -> &crate::agent_shells::Shells {
        &self.shells
    }

    /// The background shells, to note what their files say or that the popover was opened.
    pub fn shells_mut(&mut self) -> &mut crate::agent_shells::Shells {
        &mut self.shells
    }

    /// Scenes: the shells changed by `f` (dated back, given last lines), their rows drawn again.
    #[cfg(feature = "ui-shots")]
    pub fn shot_shells(&mut self, f: impl FnOnce(&mut Vec<crate::agent_shells::Shell>)) {
        f(self.shells.list_mut());
        let calls: Vec<String> = self
            .shells
            .list_mut()
            .iter()
            .map(|s| s.call.clone())
            .collect();
        for call in calls {
            self.touch_call(&call);
        }
    }

    /// The process is gone: its shells stopped, their rows drawn again.
    fn shells_gone(&mut self) {
        for call in self.shells.gone(std::time::Instant::now()) {
            self.touch_call(&call);
        }
    }

    /// Tool call `call`'s row is drawn again (its shell started or ended).
    fn touch_call(&mut self, call: &str) {
        if let Some(ix) = self.items.tool_index(call) {
            let _ = self.items.get_mut(ix);
        }
    }

    /// The agent's todo list, if it keeps one (the plan tab).
    pub fn todos(&self) -> Option<&[crate::providers::TodoItem]> {
        let at = self.todo_at?;
        match self.items.get(at) {
            Some(Item::Todo(list)) => Some(list),
            // An item before it was taken out (a cancelled message, a rewind): found again.
            _ => self.items.iter().rev().find_map(|i| match i {
                Item::Todo(list) => Some(list.as_slice()),
                _ => None,
            }),
        }
    }

    pub fn pending_permission(&self) -> Option<&Item> {
        let ix = *self.items.open_permissions().first()?;
        Some(&self.items[ix])
    }

    /// The oldest open question's index (batch 3, chore 8).
    pub fn pending_ask(&self) -> Option<usize> {
        self.open_asks.first().copied()
    }

    /// The oldest open ask of either kind (a permission or a question): the one the box and
    /// Enter answer.
    pub fn oldest_ask(&self) -> Option<usize> {
        let permission = self.items.open_permissions().first().copied();
        match (permission, self.pending_ask()) {
            (Some(p), Some(q)) => Some(p.min(q)),
            (p, q) => p.or(q),
        }
    }

    pub fn status(&self) -> Status {
        if self.ended {
            Status::Done
        } else if (!self.open_asks.is_empty() && !self.by_message)
            || self.pending_permission().is_some()
        {
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
            plan: None,
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

    /// Chore 16: a background shell is kept from its start to your next message after its
    /// end, and each change redraws the Bash row that started it.
    #[test]
    fn background_shells_are_kept_and_touch_their_row() {
        use crate::providers::ShellEnd;
        let mut c = Conversation::new();
        c.you("go".into(), Vec::new());
        c.apply(tool_started("t1"));
        // A background Bash's call ends at once ("Command running in background …").
        c.apply(Event::ToolFinished {
            id: "t1".into(),
            ok: true,
            summary: "Command running in background".into(),
            output: None,
        });
        let row = c.tool_index("t1").unwrap();
        let before = c.version(row);
        c.apply(Event::ShellStarted {
            id: "b1".into(),
            call: "t1".into(),
            description: "Run the tests".into(),
            command: "cargo test".into(),
            output: None,
        });
        assert_eq!(c.shells().counts(), (0, 1));
        assert!(c.shells().running_for_call("t1").is_some());
        assert!(c.version(row) > before, "its row says it runs");
        let started = c.version(row);
        c.apply(Event::ShellEnded {
            id: "b1".into(),
            end: ShellEnd::Done,
            output: None,
        });
        assert_eq!(c.shells().counts(), (1, 1));
        assert!(c.version(row) > started, "and that it ended");
        c.you("next".into(), Vec::new());
        assert!(c.shells().is_empty(), "gone with your next message");
        // The process gone: a running one is stopped.
        c.apply(Event::ShellStarted {
            id: "b2".into(),
            call: "t1".into(),
            description: "Serve".into(),
            command: "npm run dev".into(),
            output: None,
        });
        let running = c.version(row);
        c.process_gone();
        assert_eq!(c.shells().counts(), (1, 1));
        assert!(
            c.version(row) > running,
            "its row stops saying it runs (the review's finding 1)"
        );
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
            controls: LaneControls {
                stop: true,
                message: true,
                woken_by_steps: false,
            },
        }
    }

    /// An agy-like lane: woken by its steps.
    fn started_woken(id: &str, label: &str) -> Event {
        match started(id, label) {
            Event::SubagentStarted {
                id,
                parent,
                label,
                kind,
                task,
                model,
                ..
            } => Event::SubagentStarted {
                id,
                parent,
                label,
                kind,
                task,
                model,
                controls: LaneControls {
                    woken_by_steps: true,
                    ..LaneControls::default()
                },
            },
            other => other,
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

    /// The 2026-11-25 chores: a plan still waiting when the turn ends is expired, its plan kept
    /// (the card shows it, with no live buttons).
    #[test]
    fn a_plan_expires_with_the_turn() {
        let mut c = Conversation::new();
        c.you("plan it".into(), Vec::new());
        let plan = crate::providers::PlanDraft {
            title: "Retry".into(),
            markdown: "# Retry\n1. a".into(),
            file: None,
        };
        c.apply(Event::Permission {
            id: "p1".into(),
            tool: "ExitPlanMode".into(),
            what: String::new(),
            detail: String::new(),
            always: None,
            by: None,
            plan: Some(plan.clone()),
        });
        c.apply(Event::TurnDone {
            ok: true,
            message: None,
        });
        let card = c
            .items()
            .iter()
            .find(|i| matches!(i, Item::Permission { .. }))
            .cloned();
        let Some(Item::Permission {
            answer, plan: kept, ..
        }) = card
        else {
            panic!("no card");
        };
        assert_eq!(answer, Some(Answer::Expired));
        assert_eq!(kept.as_deref(), Some(&plan));
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
            plan: None,
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
            plan: None,
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
    fn agy_lanes_still_wake_on_steps() {
        let mut c = Conversation::new();
        c.apply(started_woken("t1", "Reader"));
        c.apply(Event::SubagentEnded {
            id: "t1".into(),
            end: LaneEnd::Done,
        });
        c.apply(sub_tool("r1", "t1"));
        assert_eq!(c.lane("t1").unwrap().status, LaneStatus::Working);
    }

    #[test]
    fn a_late_step_doesnt_reopen_a_claude_lane() {
        // The stuck lanes (the lanes chore): Claude's steps after the end never revive it;
        // only its resume does.
        let mut c = Conversation::new();
        c.apply(started("t1", "Reader"));
        c.apply(Event::SubagentEnded {
            id: "t1".into(),
            end: LaneEnd::Done,
        });
        c.apply(sub_tool("r1", "t1"));
        assert_eq!(c.lane("t1").unwrap().status, LaneStatus::Done);
        // A question after the end doesn't revive it either.
        c.apply(Event::Permission {
            id: "p1".into(),
            tool: "Bash".into(),
            what: "ls".into(),
            detail: String::new(),
            always: None,
            by: Some("t1".into()),
            plan: None,
        });
        assert_eq!(c.lane("t1").unwrap().status, LaneStatus::Done);
    }

    #[test]
    fn a_resume_reopens_any_ended_lane_and_keeps_its_place() {
        for end in [
            LaneEnd::Done,
            LaneEnd::Failed("no".into()),
            LaneEnd::Stopped,
        ] {
            let mut c = Conversation::new();
            c.apply(started("a", "Explore"));
            c.apply(started("b", "Tester"));
            c.apply(Event::SubagentEnded {
                id: "a".into(),
                end: end.clone(),
            });
            c.apply(Event::SubagentResumed {
                id: "a".into(),
                controls: LaneControls {
                    stop: true,
                    message: false,
                    woken_by_steps: false,
                },
                said: Some("Also the docs".into()),
            });
            let lane = &c.lanes()[0];
            assert_eq!(lane.id, "a", "{end:?}: its place");
            assert_eq!(lane.label, "Explore");
            assert_eq!(lane.status, LaneStatus::Working, "{end:?}");
            assert!(!lane.controls.message, "the resume's controls");
            assert_eq!(lane.runs, 2);
            // Its old end is a divider in its lane, with what Main said.
            let divider = c.items().iter().find_map(|i| match i {
                Item::Resumed {
                    lane, ended, said, ..
                } => Some((lane.clone(), ended.clone(), said.clone())),
                _ => None,
            });
            let was = match &end {
                LaneEnd::Done => LaneStatus::Done,
                LaneEnd::Failed(w) => LaneStatus::Failed(w.clone()),
                LaneEnd::Stopped => LaneStatus::Stopped,
            };
            assert_eq!(
                divider,
                Some(("a".into(), was, Some("Also the docs".into())))
            );
        }
        // An Ended lane (cut off) resumes too.
        let mut c = Conversation::new();
        c.past(vec![Past::Event(started("a", "Explore"))]);
        assert_eq!(c.lane("a").unwrap().status, LaneStatus::Ended);
        c.apply(Event::SubagentResumed {
            id: "a".into(),
            controls: LaneControls::default(),
            said: None,
        });
        assert_eq!(c.lane("a").unwrap().status, LaneStatus::Working);
    }

    fn relay_of(c: &Conversation) -> Vec<(String, bool, RelayState)> {
        c.items()
            .iter()
            .filter_map(|i| match i {
                Item::You {
                    text,
                    taken,
                    to: Some(r),
                    ..
                } => Some((text.clone(), *taken, r.state.clone())),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_relay_counts_as_a_message_to_main() {
        let mut c = Conversation::new();
        c.you_as("go".into(), Vec::new(), Some("u1".into()));
        c.apply(Event::Taken {
            id: Some("u1".into()),
        });
        c.apply(started("t1", "Explore"));
        c.apply(turn_done());
        assert_eq!(c.status(), Status::Done);
        c.relay(
            "Also the docs".into(),
            Some("u2".into()),
            "t1".into(),
            false,
        );
        assert_eq!(c.status(), Status::Working, "waiting on Main's turn");
        assert_eq!(c.latest_you(), Some(c.items().len() - 1));
        assert!(c.cancel_unread("u2"), "taken back like any message");
        assert_eq!(c.status(), Status::Done);
        c.relay(
            "Also the docs".into(),
            Some("u3".into()),
            "t1".into(),
            false,
        );
        c.apply(Event::Taken {
            id: Some("u3".into()),
        });
        assert_eq!(
            relay_of(&c),
            [("Also the docs".into(), true, RelayState::Sent)]
        );
        c.apply(Event::PassedOn {
            call: "toolu_s".into(),
            lane: "t1".into(),
        });
        assert_eq!(relay_of(&c)[0].2, RelayState::Delivered);
        assert_eq!(c.passed_on("toolu_s"), Some("t1"));
        c.apply(turn_done());
        assert_eq!(relay_of(&c)[0].2, RelayState::Delivered, "kept");
        assert_eq!(c.status(), Status::Done);
    }

    #[test]
    fn a_relay_not_passed_on_or_too_late_says_so() {
        let mut c = Conversation::new();
        c.apply(started("t1", "Explore"));
        c.apply(started("t2", "Tester"));
        c.relay("one".into(), Some("u1".into()), "t1".into(), false);
        c.relay("two".into(), Some("u2".into()), "t2".into(), false);
        c.apply(Event::Taken {
            id: Some("u1".into()),
        });
        // Explore finishes before Main passes "one" on; Main's turn ends.
        c.apply(Event::SubagentEnded {
            id: "t1".into(),
            end: LaneEnd::Done,
        });
        c.apply(Event::Taken {
            id: Some("u2".into()),
        });
        c.apply(turn_done());
        let states: Vec<RelayState> = relay_of(&c).into_iter().map(|r| r.2).collect();
        assert_eq!(states, [RelayState::LaneEnded, RelayState::NotPassed]);
    }

    #[test]
    fn a_relay_shows_in_main_and_its_lane() {
        use crate::agent_lanes::View;
        let mut c = Conversation::new();
        c.you("go".into(), Vec::new());
        c.apply(agent_call("t1"));
        c.apply(started("t1", "Explore"));
        c.apply(sub_tool("r1", "t1"));
        c.relay(
            "Also the docs".into(),
            Some("u2".into()),
            "t1".into(),
            false,
        );
        let relay = c.items().len() - 1;
        c.apply(sub_tool("r2", "t1"));
        let lanes = ["t1"];
        let main = crate::agent_chat::rows_of(c.items(), &View::Main, &lanes);
        assert!(main.contains(&relay), "your bubble in Main");
        let lane = crate::agent_chat::rows_of(c.items(), &View::Lane("t1".into()), &lanes);
        assert_eq!(
            lane.last(),
            Some(&relay),
            "last in its lane while undelivered"
        );
        assert_eq!(lane.len(), 3, "r1, r2 and the relay, once");
        c.apply(Event::PassedOn {
            call: "s1".into(),
            lane: "t1".into(),
        });
        c.apply(sub_tool("r3", "t1"));
        let lane = crate::agent_chat::rows_of(c.items(), &View::Lane("t1".into()), &lanes);
        assert_eq!(
            lane[lane.len() - 2],
            relay,
            "delivered: it stays where it was, r3 after it"
        );
        // An about message is Main's only.
        c.relay("What did it find?".into(), None, "t1".into(), true);
        let about = c.items().len() - 1;
        let lane = crate::agent_chat::rows_of(c.items(), &View::Lane("t1".into()), &lanes);
        assert!(!lane.contains(&about));
    }

    #[test]
    fn a_past_relay_is_delivered_or_not_passed() {
        let mut c = Conversation::new();
        c.past(vec![
            Past::Event(started("t1", "Explore")),
            Past::Relay {
                text: "one".into(),
                id: Some("u1".into()),
                to: "t1".into(),
                about: false,
            },
            Past::Event(Event::PassedOn {
                call: "s1".into(),
                lane: "t1".into(),
            }),
            Past::Relay {
                text: "two".into(),
                id: None,
                to: "t1".into(),
                about: false,
            },
        ]);
        let states: Vec<RelayState> = relay_of(&c).into_iter().map(|r| r.2).collect();
        assert_eq!(states, [RelayState::Delivered, RelayState::NotPassed]);
        assert!(relay_of(&c).iter().all(|r| r.1), "taken");
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
            // The agent exiting leaves its lanes without a report (the lanes chore); a pause
            // fuxx made stops them.
            let cut = if end == 0 {
                LaneStatus::Ended
            } else {
                LaneStatus::Stopped
            };
            assert_eq!(c.lane("w").unwrap().status, cut, "{end}");
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
            plan: None,
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
    fn a_resumed_lane_without_its_result_is_ended() {
        // Review 2: the app quit mid-subagent: no result in the transcript. The lanes chore:
        // it ended without a report (a hollow ring, never a spinner).
        let mut c = Conversation::new();
        c.past(vec![Past::Event(started("t1", "Reader"))]);
        assert_eq!(c.lane("t1").unwrap().status, LaneStatus::Ended);
        assert!(!c.lane("t1").unwrap().status.live());
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

    /// Batch 3, wave 3: a message not yet read can be cancelled: it goes (counted no more:
    /// the turn it would have started never comes); a read one, or one by another id, stays.
    #[test]
    fn an_unread_message_can_be_cancelled_and_a_read_one_cannot() {
        let mut c = Conversation::new();
        c.you_as("one".into(), Vec::new(), Some("u1".into()));
        c.apply(Event::Taken {
            id: Some("u1".into()),
        });
        c.apply(said("working"));
        c.you_as("note".into(), Vec::new(), Some("u2".into()));
        c.apply(said("still working"));
        assert!(!c.cancel_unread("u1"), "already read");
        assert!(!c.cancel_unread("nobody"));
        let before = c.items().len();
        assert!(c.cancel_unread("u2"));
        assert_eq!(c.items().len(), before - 1);
        assert!(
            !c.items()
                .iter()
                .any(|i| matches!(i, Item::You { text, .. } if text == "note"))
        );
        assert_eq!(c.latest_you(), Some(0));
        // Its turn never comes: the first one's end leaves nothing waiting.
        c.apply(turn_done());
        assert_eq!(c.status(), Status::Done);
        assert!(!c.cancel_unread("u2"), "once");
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

    /// Undo Rewind (and a rewind that failed) after something was added since the cut (the
    /// "Rewound" notice, a send that failed): what came after the cut goes, the cut comes back.
    #[test]
    fn restore_drops_what_came_after_the_cut() {
        let mut c = Conversation::new();
        c.you_as("one".into(), Vec::new(), Some("u1".into()));
        c.apply(turn_done());
        c.you_as("two".into(), Vec::new(), Some("u2".into()));
        c.apply(said("on it"));
        let whole = c.items().to_vec();
        let cut = c.cut_before(1).expect("two");
        c.apply(Event::Notice("Rewound".into()));
        c.you_as("again".into(), Vec::new(), Some("u3".into()));
        c.restore(cut);
        assert_eq!(c.items(), &whole[..]);
        assert_eq!(c.latest_you(), Some(1));
    }

    /// Fork (batch 3, chore 2): a copy of the conversation before your message, with no id of its
    /// own (the fork's comes when it starts), `head` as its first row, the source untouched.
    #[test]
    fn forked_before_copies_what_came_before_under_a_head() {
        let mut c = Conversation::new();
        c.apply(Event::Started {
            conversation: "S".into(),
            model: Some("m".into()),
        });
        c.you_as("one".into(), Vec::new(), Some("u1".into()));
        c.apply(agent_call("t1"));
        c.apply(started("t1", "Reader"));
        c.apply(turn_done());
        c.you_as("two".into(), Vec::new(), Some("u2".into()));
        c.apply(said("working"));
        let whole = c.items().to_vec();
        assert!(matches!(&whole[2], Item::You { text, .. } if text == "two"));
        let f = c
            .forked_before(2, Item::Notice("Forked".into()))
            .expect("a message of yours");
        assert_eq!(c.items(), &whole[..], "the source untouched");
        assert_eq!(f.items()[0], Item::Notice("Forked".into()));
        assert_eq!(&f.items()[1..], &whole[..2]);
        assert_eq!(f.latest_you(), Some(1));
        assert_eq!(f.conversation(), None);
        assert_eq!(f.lanes().len(), 1);
        assert_eq!(f.status(), Status::Done);
        // The lane's start moved with the head: a cut before "one" drops it.
        let mut f = f;
        let _ = f.cut_before(1).expect("one");
        assert!(f.lanes().is_empty());
        assert!(c.forked_before(1, Item::Notice("x".into())).is_none());
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
                plan: None,
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

    // ---- Batch 3, chore 8: the agent's question picker ----

    fn ask(id: &str, by: Option<&str>) -> Event {
        Event::Question {
            id: id.into(),
            questions: vec![crate::providers::Question {
                header: "Test runner".into(),
                text: "Which test runner?".into(),
                options: vec![crate::providers::QOption {
                    label: "Vitest".into(),
                    description: String::new(),
                    preview: None,
                }],
                multi: false,
            }],
            by: by.map(String::from),
        }
    }

    fn reply_of(c: &Conversation, id: &str) -> Option<Reply> {
        c.items().iter().find_map(|i| match i {
            Item::Question { id: q, reply, .. } if q == id => Some(reply.clone()),
            _ => None,
        })?
    }

    fn picked(label: &str) -> QAnswer {
        QAnswer {
            labels: vec![label.into()],
            other: None,
        }
    }

    /// 2026-11-24: agy's question came after it went on (print mode skips it): the tab doesn't
    /// wait on it, the turn's end leaves it open, and only you close it — your answer, a skip,
    /// or another message (not answered).
    #[test]
    fn a_question_answered_by_message_waits_on_nothing() {
        let mut c = Conversation::new();
        c.set_questions_by_message(true);
        assert!(c.questions_by_message());
        c.you("set up tests".into(), Vec::new());
        c.apply(Event::TurnDone {
            ok: true,
            message: None,
        });
        c.apply(ask("q1", None));
        assert_eq!(c.status(), Status::Done, "agy isn't blocked");
        assert_eq!(c.oldest_ask(), Some(1), "the card is still the box's");
        c.apply(Event::TurnDone {
            ok: true,
            message: None,
        });
        c.process_ended(Some(0), "");
        assert_eq!(reply_of(&c, "q1"), None, "only you close it");
        // The card's own message: answered first, then sent.
        c.reply("q1", Reply::Answered(vec![picked("Vitest")]));
        c.you("My answers to your questions: …".into(), Vec::new());
        assert!(matches!(reply_of(&c, "q1"), Some(Reply::Answered(_))));
        // Another message while one is open: not answered.
        c.apply(ask("q2", None));
        c.you("never mind, use Jest".into(), Vec::new());
        assert_eq!(reply_of(&c, "q2"), Some(Reply::Expired));
        assert_eq!(c.oldest_ask(), None);
        // Claude's questions are unchanged: they wait, and the turn's end expires them.
        let mut c = Conversation::new();
        c.you("go".into(), Vec::new());
        c.apply(ask("q1", None));
        c.you("more".into(), Vec::new());
        assert_eq!(reply_of(&c, "q1"), None, "a note mid-ask leaves it open");
        assert_eq!(c.status(), Status::Waiting);
    }

    #[test]
    fn a_question_waits_and_answering_works_again() {
        let mut c = Conversation::new();
        c.you("set up tests".into(), Vec::new());
        c.apply(ask("q1", None));
        assert_eq!(c.status(), Status::Waiting);
        assert_eq!(c.pending_ask(), Some(1));
        assert_eq!(c.oldest_ask(), Some(1));
        c.reply("q1", Reply::Answered(vec![picked("Vitest")]));
        assert_eq!(c.status(), Status::Working, "the turn goes on");
        assert_eq!(c.pending_ask(), None);
        assert_eq!(
            reply_of(&c, "q1"),
            Some(Reply::Answered(vec![picked("Vitest")]))
        );
        // Answered once: a second reply changes nothing.
        c.reply("q1", Reply::Skipped);
        assert!(matches!(reply_of(&c, "q1"), Some(Reply::Answered(_))));
    }

    /// The agent stopped waiting (Claude's `control_cancel_request`): the open card settles at
    /// once, not at the turn's end; an answered one keeps its answer.
    #[test]
    fn a_withdrawn_ask_settles_now_and_an_answered_one_keeps_its_answer() {
        let mut c = Conversation::new();
        c.you("go".into(), Vec::new());
        c.apply(started("t1", "Reader"));
        c.apply(ask("q1", Some("t1")));
        c.apply(Event::Permission {
            id: "p1".into(),
            tool: "Bash".into(),
            what: "npm test".into(),
            detail: String::new(),
            always: None,
            by: Some("t1".into()),
            plan: None,
        });
        c.apply(Event::Permission {
            id: "p2".into(),
            tool: "Bash".into(),
            what: "ls".into(),
            detail: String::new(),
            always: None,
            by: None,
            plan: None,
        });
        c.answered("p2", Answer::Allowed);
        c.apply(Event::AskWithdrawn { id: "q1".into() });
        assert_eq!(reply_of(&c, "q1"), Some(Reply::Expired));
        assert_eq!(c.pending_ask(), None);
        assert_eq!(
            c.lane("t1").unwrap().status,
            LaneStatus::Waiting,
            "p1 waits"
        );
        c.apply(Event::AskWithdrawn { id: "p1".into() });
        let answer_of = |c: &Conversation, id: &str| {
            c.items().iter().find_map(|i| match i {
                Item::Permission { id: p, answer, .. } if p == id => Some(answer.clone()),
                _ => None,
            })
        };
        assert_eq!(answer_of(&c, "p1"), Some(Some(Answer::Withdrawn)));
        assert_eq!(c.lane("t1").unwrap().status, LaneStatus::Working);
        assert_eq!(c.status(), Status::Working, "nothing waits");
        c.apply(Event::AskWithdrawn { id: "p2".into() });
        assert_eq!(answer_of(&c, "p2"), Some(Some(Answer::Allowed)));
    }

    #[test]
    fn a_subagents_question_makes_its_lane_wait() {
        let mut c = Conversation::new();
        c.you("go".into(), Vec::new());
        c.apply(started("t1", "Reader"));
        c.apply(ask("q1", Some("t1")));
        assert_eq!(c.lane("t1").unwrap().status, LaneStatus::Waiting);
        assert_eq!(c.status(), Status::Waiting);
        // A permission of its own as well: it waits until both are answered.
        c.apply(Event::Permission {
            id: "p1".into(),
            tool: "Bash".into(),
            what: "ls".into(),
            detail: String::new(),
            always: None,
            by: Some("t1".into()),
            plan: None,
        });
        c.reply("q1", Reply::Skipped);
        assert_eq!(c.lane("t1").unwrap().status, LaneStatus::Waiting);
        c.answered("p1", Answer::Allowed);
        assert_eq!(c.lane("t1").unwrap().status, LaneStatus::Working);
        // And the other way round: a permission answered while its question waits.
        c.apply(ask("q2", Some("t1")));
        c.apply(Event::Permission {
            id: "p2".into(),
            tool: "Bash".into(),
            what: "ls".into(),
            detail: String::new(),
            always: None,
            by: Some("t1".into()),
            plan: None,
        });
        c.answered("p2", Answer::Allowed);
        assert_eq!(c.lane("t1").unwrap().status, LaneStatus::Waiting);
        // Its lane's end expires it.
        c.apply(Event::SubagentEnded {
            id: "t1".into(),
            end: LaneEnd::Done,
        });
        assert_eq!(reply_of(&c, "q2"), Some(Reply::Expired));
        assert_eq!(c.pending_ask(), None);
    }

    #[test]
    fn turn_end_and_process_end_expire_it() {
        for end in 0..4 {
            let mut c = Conversation::new();
            c.you("x".into(), vec![]);
            c.apply(ask("q1", None));
            match end {
                0 => c.apply(turn_done()),
                1 => c.process_ended(Some(1), ""),
                2 => c.process_gone(),
                _ => c.paused(),
            }
            assert_eq!(reply_of(&c, "q1"), Some(Reply::Expired), "{end}");
            assert_eq!(c.pending_ask(), None, "{end}");
            assert_ne!(c.status(), Status::Waiting, "{end}");
        }
    }

    #[test]
    fn its_tool_result_closes_it() {
        let finished = |ok| Event::ToolFinished {
            id: "q1".into(),
            ok,
            summary: String::new(),
            output: None,
        };
        let mut c = Conversation::new();
        c.you("x".into(), vec![]);
        c.apply(ask("q1", None));
        c.apply(finished(true));
        assert_eq!(reply_of(&c, "q1"), Some(Reply::Answered(Vec::new())));
        assert_eq!(c.pending_ask(), None);
        // A failed result (an interrupt) leaves it not answered.
        let mut c = Conversation::new();
        c.apply(ask("q1", None));
        c.apply(finished(false));
        assert_eq!(reply_of(&c, "q1"), Some(Reply::Expired));
        // Answered here first: its result keeps what you chose.
        let mut c = Conversation::new();
        c.apply(ask("q1", None));
        c.reply("q1", Reply::Skipped);
        c.apply(finished(false));
        assert_eq!(reply_of(&c, "q1"), Some(Reply::Skipped));
    }

    /// The design review (fix 3): a restored result's reply closes the question with what you
    /// chose, or as skipped; one already closed keeps its reply.
    #[test]
    fn a_replayed_reply_closes_it_with_your_answers() {
        let answers = vec![crate::providers::QAnswer {
            labels: vec!["Vitest".into()],
            other: None,
        }];
        let mut c = Conversation::new();
        c.past(vec![
            Past::Event(ask("q1", None)),
            Past::Event(Event::QuestionReplied {
                id: "q1".into(),
                answers: Some(answers.clone()),
            }),
            Past::Event(ask("q2", None)),
            Past::Event(Event::QuestionReplied {
                id: "q2".into(),
                answers: None,
            }),
        ]);
        assert_eq!(reply_of(&c, "q1"), Some(Reply::Answered(answers)));
        assert_eq!(reply_of(&c, "q2"), Some(Reply::Skipped));
        assert_eq!(c.pending_ask(), None);
        let mut c = Conversation::new();
        c.apply(ask("q1", None));
        c.reply("q1", Reply::Expired);
        c.apply(Event::QuestionReplied {
            id: "q1".into(),
            answers: None,
        });
        assert_eq!(reply_of(&c, "q1"), Some(Reply::Expired));
    }

    #[test]
    fn history_never_leaves_one_open() {
        let mut c = Conversation::new();
        c.past(vec![
            Past::you("set up tests"),
            Past::Event(ask("q1", None)),
            Past::Event(Event::ToolFinished {
                id: "q1".into(),
                ok: true,
                summary: String::new(),
                output: None,
            }),
            Past::you("again"),
            Past::Event(ask("q2", None)),
        ]);
        assert_eq!(reply_of(&c, "q1"), Some(Reply::Answered(Vec::new())));
        assert_eq!(reply_of(&c, "q2"), Some(Reply::Expired));
        assert_eq!(c.pending_ask(), None);
        assert_eq!(c.status(), Status::Done);
    }

    #[test]
    fn oldest_open_ask_wins() {
        let mut c = Conversation::new();
        c.you("x".into(), vec![]);
        c.apply(ask("q1", None));
        c.apply(ask("q2", None));
        assert_eq!(c.pending_ask(), Some(1));
        c.reply("q1", Reply::Skipped);
        assert_eq!(c.pending_ask(), Some(2));
        // A permission asked before the question goes first.
        let mut c = Conversation::new();
        c.apply(permission("p1"));
        c.apply(ask("q1", None));
        assert_eq!(c.oldest_ask(), Some(0));
        c.answered("p1", Answer::Allowed);
        assert_eq!(c.oldest_ask(), Some(1));
    }

    /// The cache is what a full scan finds, after every way items move.
    #[test]
    fn open_asks_matches_a_full_scan() {
        fn scan(c: &Conversation) -> Vec<usize> {
            c.items()
                .iter()
                .enumerate()
                .filter(|(_, i)| matches!(i, Item::Question { reply: None, .. }))
                .map(|(ix, _)| ix)
                .collect()
        }
        let check = |c: &Conversation, what: &str| assert_eq!(c.open_asks, scan(c), "{what}");
        let mut c = Conversation::new();
        c.past(vec![Past::You {
            text: "one".into(),
            id: Some("m1".into()),
        }]);
        c.apply(ask("q1", None));
        check(&c, "asked");
        c.you_as("two".into(), Vec::new(), Some("m2".into()));
        c.you_as("three".into(), Vec::new(), Some("m3".into()));
        c.apply(ask("q2", None));
        check(&c, "asked again");
        assert!(c.cancel_unread("m2"));
        check(&c, "a message before it taken back");
        let cut = c.cut_before(2).expect("a message of yours");
        check(&c, "cut");
        c.restore(cut);
        check(&c, "restored");
        let fork = c.forked_before(2, Item::Notice("fork".into())).unwrap();
        check(&fork, "forked");
        c.keep_first(2);
        check(&c, "kept the first two");
        c.apply(ask("q3", None));
        c.apply(Event::Cleared);
        check(&c, "cleared");
        assert!(c.open_asks.is_empty());
    }

    // ---- the agent activity chore ----

    fn activity_of(derived: &str, check: Option<crate::agent_activity::Outcome>) -> Event {
        Event::Activity {
            id: "t1".into(),
            activity: Activity {
                derived: derived.into(),
                check: check.map(|outcome| crate::agent_activity::Check {
                    what: crate::agent_activity::CheckWhat::Tests,
                    scope: "cargo test".into(),
                    outcome,
                }),
                ..Default::default()
            },
        }
    }

    fn finished_ok(id: &str, ok: bool) -> Event {
        Event::ToolFinished {
            id: id.into(),
            ok,
            summary: String::new(),
            output: None,
        }
    }

    fn tool_activity<'a>(c: &'a Conversation, id: &str) -> Option<&'a Activity> {
        c.items().iter().find_map(|i| match i {
            Item::Tool {
                id: t, activity, ..
            } if t == id => activity.as_deref(),
            _ => None,
        })
    }

    fn failed_check() -> Option<crate::agent_activity::Outcome> {
        Some(crate::agent_activity::Outcome::Failed {
            failed: Some(1),
            of: Some(3),
            first_error: None,
        })
    }

    #[test]
    fn an_activity_is_stored_on_its_tool_and_replaced_at_its_finish() {
        let mut c = Conversation::new();
        c.apply(tool_started("t1"));
        assert!(tool_activity(&c, "t1").is_none());
        c.apply(activity_of("Reading api.rs", None));
        assert_eq!(tool_activity(&c, "t1").unwrap().derived, "Reading api.rs");
        c.apply(finished_ok("t1", true));
        c.apply(activity_of("Read api.rs", None));
        assert_eq!(tool_activity(&c, "t1").unwrap().derived, "Read api.rs");
    }

    #[test]
    fn a_failure_the_narration_announced_is_planned() {
        let run = |narration: Option<&str>, said: Option<&str>| {
            let mut c = Conversation::new();
            if let Some(n) = narration {
                c.apply(done("m1", n));
            }
            c.apply(tool_started("t1"));
            c.apply(finished_ok("t1", false));
            let Event::Activity { id, mut activity } = activity_of("Ran tests", failed_check())
            else {
                unreachable!()
            };
            activity.said = said.map(String::from);
            c.apply(Event::Activity { id, activity });
            tool_activity(&c, "t1").unwrap().planned_failure
        };
        assert!(run(Some("Write the failing test first (red first)."), None));
        assert!(!run(None, None));
        assert!(!run(Some("Run the tests."), None));
        assert!(run(None, Some("Run it, it should fail")));
    }

    #[test]
    fn only_the_first_red_run_of_a_check_is_planned() {
        let failed_run = |c: &mut Conversation, id: &str| {
            c.apply(tool_started(id));
            c.apply(finished_ok(id, false));
            let Event::Activity { activity, .. } = activity_of("Ran tests", failed_check()) else {
                unreachable!()
            };
            c.apply(Event::Activity {
                id: id.into(),
                activity,
            });
        };
        let mut c = Conversation::new();
        c.you("add retries".into(), vec![]);
        c.apply(done("m1", "Write the failing test first (red first)."));
        failed_run(&mut c, "t1");
        c.apply(tool_started("t2"));
        c.apply(finished_ok("t2", true));
        failed_run(&mut c, "t3");
        assert!(tool_activity(&c, "t1").unwrap().planned_failure);
        assert!(
            !tool_activity(&c, "t3").unwrap().planned_failure,
            "the same check failing again after a change is a real failure"
        );
        // A new narration starts a new run: its announced failure is planned again.
        c.apply(done("m2", "Now another failing test first (red first)."));
        failed_run(&mut c, "t4");
        assert!(tool_activity(&c, "t4").unwrap().planned_failure);
    }

    #[test]
    fn narration_is_the_text_just_before_the_run() {
        let mut c = Conversation::new();
        c.you("go".into(), vec![]);
        c.apply(done("m1", "A"));
        c.apply(tool_started("t1"));
        c.apply(tool_started("t2"));
        assert_eq!(c.narration_before(3), Some("A"));
        assert_eq!(c.narration_before(2), Some("A"));
        let mut c = Conversation::new();
        c.apply(done("m1", "A"));
        c.you("go".into(), vec![]);
        c.apply(tool_started("t1"));
        assert_eq!(c.narration_before(2), None);
        // A subagent's tool reads only its own lane's text.
        let mut c = Conversation::new();
        c.apply(done("m1", "Main says"));
        c.apply(Event::MessageDone {
            message: "m2".into(),
            text: "Lane says".into(),
            parent: Some("L".into()),
        });
        c.apply(done("m3", "Main again"));
        c.apply(Event::ToolStarted {
            id: "t1".into(),
            name: "Bash".into(),
            summary: "ls".into(),
            parent: Some("L".into()),
            detail: None,
            writes: Vec::new(),
            texts: Vec::new(),
        });
        assert_eq!(c.narration_before(3), Some("Lane says"));
    }

    #[test]
    fn live_from_is_where_the_history_ends() {
        let mut c = Conversation::new();
        assert_eq!(c.live_from(), 0);
        c.past(vec![
            Past::You {
                text: "hi".into(),
                id: None,
            },
            Past::Event(done("m1", "hello")),
        ]);
        assert_eq!(c.live_from(), c.items().len());
        c.apply(Event::Cleared);
        assert_eq!(c.live_from(), 0);
    }
}
