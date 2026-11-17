//! The agent tab (M11aa Task 7): a chat with an agent fuxx runs itself, in its stream-json mode
//! (`providers`), instead of in a terminal. The agent's process (`agent_process`) is read on a
//! task; each line goes through the provider's translator into `Event`s, which the
//! `Conversation` folds into items; the items are drawn as a GPUI `list` (only the rows in sight
//! are laid out), and the `Composer` below them sends messages, answers permission prompts and
//! stops the turn.
//!
//! Rules it keeps (the plan's Global Constraints and Refresh calls):
//! - Nothing is emitted from `new` or `render` (R8): the history, the process and every event
//!   come in on spawned tasks, and `on_events` (or a person's action) emits what changed.
//! - Every change it shows ends in `cx.notify()` on the tab itself: the Workspace draws it cached.
//! - Messages are drawn by `md_preview::column` (R7: Mermaid shows as code).
//! - The process is kept as an `Arc<AgentProcess>` (Ruling 23) and its end is reported to the
//!   `Conversation` exactly once (`ProcessEnd`).
//!
//! `ProcessEnd`, `resume_failed` and the small text helpers are pure and tested.

use std::cell::Cell;
use std::collections::HashSet;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use alacritty_terminal::vte::ansi::Rgb;
use futures::StreamExt;
use futures::channel::mpsc;
use gpui::{
    AnyElement, App, Context, Entity, EventEmitter, FocusHandle, Focusable, FontWeight,
    IntoElement, ListAlignment, ListState, MouseButton, Render, SharedString, Stateful,
    Subscription, Task, WeakEntity, Window, actions, div, list, prelude::*, px, relative,
};

use crate::agent_conversation::{Answer, Conversation, Item};
use crate::agent_markdown::{Md, parse};
use crate::agent_process::{self, AgentProcess, ProcessEvent};
use crate::appearance::Appearance;
use crate::block_view::BlockDrag;
use crate::composer::{Composer, ComposerEvent};
use crate::diff_view::{RowKind, parse_diff};
use crate::file_tab::{DiffRowColours, diff_row_colours};
use crate::md_preview::{self, LinkTarget};
use crate::palette_items::FileHit;
use crate::providers::{Action, Event, Past, Provider, Start, TodoState, ToolDetail, Translator};
use crate::status::Status;
use crate::terminal_view::hsla;
use crate::theme::{Theme, UiColors, mix};
use crate::ui_tokens::{TEXT_MIN, ensure_contrast, text};
use crate::workspace_agent::Holder;

// ↑ / ↓ in the `@` file list (bound in main.rs to "AtPopup > Composer", so they reach the
// list, not the caret, only while it is open).
actions!(agent_tab, [AtUp, AtDown]);

/// After Stop, how long the tab waits for the turn to end before offering **Force stop**
/// (an interrupt during a running tool may not act until the tool ends: Ruling 7).
pub const STOP_FORCE_AFTER: Duration = Duration::from_secs(10);
/// After the process ends, how long to wait for `Exited` and its stderr tail (a straggler
/// holding stdout can hold `Exited` back; Ruling 17) before reporting the end without it.
pub const END_WAIT: Duration = Duration::from_secs(1);

/// The notice a failed resume leaves (Review Focus 4).
pub const BAD_RESUME_NOTICE: &str = "Couldn't resume that conversation; this is a new one";
/// The notice of a resumed conversation whose history couldn't be read (call 7).
pub const RESTORED_NOTICE: &str = "Conversation restored";

/// The most `@` file suggestions shown.
const AT_LIMIT: usize = 8;
/// A chat row's text, and the avatar's side.
const BODY: f32 = 13.0;
const AVATAR: f32 = 22.0;
const AVATAR_GAP: f32 = 9.0;
/// The most lines of a permission's detail shown on its card.
const DETAIL_LINES: usize = 12;

/// What the tab tells the Workspace (Task 8 subscribes; Task 9 draws status and question).
#[derive(Clone, Debug, PartialEq)]
pub enum AgentTabEvent {
    /// The tab's status changed. `live: false` when the change is not news: history replayed
    /// into a resumed tab, and (Task 8, R13) the pause before "Open in classic view" — no
    /// banner, no pop.
    Status { status: Status, live: bool },
    /// "⟨Label⟩ wants to run: ⟨what⟩" while a permission waits, else None.
    Question(Option<String>),
    /// The first line of your first message (once).
    Title(String),
    /// The conversation's id, when the agent first reports it (once).
    Conversation(String),
    /// The error tab's Close.
    Close,
    /// "Open in classic view": the command line that resumes it in a terminal.
    OpenClassic(Vec<String>),
    /// A message sent from a tab paused for "Open in classic view" (R13): the Workspace answers
    /// with `resume_after_classic` (it knows whether the classic terminal still runs it).
    ResumeAsked(String),
    /// The agent couldn't start, or its process ended with an error (a non-zero exit, a
    /// signal): its error row's words, for the tab's Activity (spec §1.5). Before the `Status`
    /// it brings.
    Failed(String),
    /// A command block dropped on the tab's body (Task 11): the Workspace reads its output and
    /// attaches it (`attach_block`), as for a drop on the tab in a bar.
    BlockDropped(BlockDrag),
}

/// Reports the process's end exactly once (`Conversation::process_ended` isn't idempotent):
/// `Ended` (the child reaped) starts the `END_WAIT` timer; `Exited` (stdout drained, with the
/// stderr tail) reports it, or the timer does, with no tail, if `Exited` is late.
#[derive(Default)]
pub(crate) struct ProcessEnd {
    /// The exit code `Ended` gave, once it came.
    ended: Option<Option<i32>>,
    reported: bool,
}

impl ProcessEnd {
    /// The child was reaped: true the first time (start the `END_WAIT` timer).
    pub fn ended(&mut self, code: Option<i32>) -> bool {
        if self.reported || self.ended.is_some() {
            return false;
        }
        self.ended = Some(code);
        true
    }

    /// stdout is drained: the end to report, unless it was reported already.
    pub fn exited(&mut self, code: Option<i32>, tail: String) -> Option<(Option<i32>, String)> {
        if self.reported {
            return None;
        }
        self.reported = true;
        Some((code, tail))
    }

    /// The `END_WAIT` timer fired before `Exited`: the end to report, without a tail.
    pub fn waited(&mut self) -> Option<(Option<i32>, String)> {
        match self.ended {
            Some(code) if !self.reported => {
                self.reported = true;
                Some((code, String::new()))
            }
            _ => None,
        }
    }
}

/// A run asked to resume a conversation ended before the agent said it had started one: the
/// resume failed (a deleted transcript, an unknown id).
pub fn resume_failed(started: bool, resume: bool) -> bool {
    resume && !started
}

/// The status in the tab's top line.
pub fn status_word(status: Status) -> &'static str {
    match status {
        Status::Working => "working",
        Status::Waiting => "needs you",
        Status::Done => "done",
        Status::Idle => "idle",
    }
}

/// The model chip's text: as `provider` labels `model` (no provider: as it is).
pub fn model_chip(provider: Option<&dyn Provider>, model: &str) -> String {
    provider.map_or_else(|| model.to_string(), |p| p.model_label(model))
}

/// The cost chip's text.
pub fn cost_chip(usd: f64) -> String {
    format!("${usd:.2}")
}

/// A running tool's elapsed time: `4s`, `1m 05s`.
pub fn elapsed_label(d: Duration) -> String {
    let s = d.as_secs();
    if s < 60 {
        format!("{s}s")
    } else {
        format!("{}m {:02}s", s / 60, s % 60)
    }
}

/// A read's result: how many lines it read.
pub fn lines_label(n: usize) -> String {
    if n == 1 {
        "1 line".into()
    } else {
        format!("{n} lines")
    }
}

/// An answered permission card's line.
pub fn answer_line(answer: &Answer) -> String {
    match answer {
        Answer::Allowed => "Allowed".into(),
        Answer::AlwaysAllowed => "Always allowed".into(),
        Answer::Denied(Some(why)) => format!("Denied: {why}"),
        Answer::Denied(None) => "Denied".into(),
        Answer::Expired => "Expired".into(),
    }
}

/// The text of the `@` query: what follows the `@` at byte `at` up to the caret, while that is
/// still one word after an `@`; else None (the list closes).
pub fn at_query(text: &str, at: usize, cursor: usize) -> Option<&str> {
    if cursor <= at || !text.get(at..)?.starts_with('@') {
        return None;
    }
    let query = text.get(at + 1..cursor)?;
    (!query.contains(char::is_whitespace)).then_some(query)
}

/// The path an `@` pick inserts: relative to the agent's folder when the file is in it, else
/// whole.
pub fn inserted_path(root: &Path, cwd: &Path, rel: &str) -> String {
    let full = root.join(rel);
    match full.strip_prefix(cwd) {
        Ok(inside) if !inside.as_os_str().is_empty() => inside.to_string_lossy().into_owned(),
        _ => full.to_string_lossy().into_owned(),
    }
}

/// Take attachment `i` off the list waiting for your next message (its chip's ×); false when
/// there is none at `i` (nothing changes).
pub fn remove_attachment(list: &mut Vec<String>, i: usize) -> bool {
    if i >= list.len() {
        return false;
    }
    list.remove(i);
    true
}

/// What is sent for a message with attachments: the text, then each attachment.
pub fn wire_text(text: &str, attachments: &[String]) -> String {
    let mut out = text.to_string();
    for a in attachments {
        if !out.is_empty() {
            out.push_str("\n\n");
        }
        out.push_str(a);
    }
    out
}

/// What an item looks like, as a number: a row is laid out again when its number changes.
fn fingerprint(item: &Item) -> u64 {
    let mut h = DefaultHasher::new();
    std::mem::discriminant(item).hash(&mut h);
    match item {
        Item::You { text, attachments } => (text, attachments).hash(&mut h),
        Item::Agent { text, done, .. } => (text, done).hash(&mut h),
        Item::Tool {
            running,
            ok,
            result,
            output,
            dropped_lines,
            ..
        } => (
            running,
            ok,
            result,
            output.as_ref().map(String::len),
            dropped_lines,
        )
            .hash(&mut h),
        Item::Todo(list) => {
            for t in list {
                (&t.text, t.state as u8).hash(&mut h);
            }
        }
        Item::Permission { answer, .. } => format!("{answer:?}").hash(&mut h),
        Item::Notice(t) | Item::Error(t) => t.hash(&mut h),
    }
    h.finish()
}

/// The rows to lay out again after the items changed from `old` to `new` (fingerprints): each
/// changed row still there, and how the count changed (rows added at the end or dropped from it).
pub fn changed_rows(old: &[u64], new: &[u64]) -> Vec<usize> {
    old.iter()
        .zip(new)
        .enumerate()
        .filter(|(_, (a, b))| a != b)
        .map(|(i, _)| i)
        .collect()
}

/// The tool call an item belongs to (a sub-agent's), if any.
fn parent_of(item: &Item) -> Option<&str> {
    match item {
        Item::Agent { parent, .. } | Item::Tool { parent, .. } => parent.as_deref(),
        _ => None,
    }
}

/// An item said by the agent (its avatar column), not you or the tab.
fn agent_side(item: &Item) -> bool {
    matches!(
        item,
        Item::Agent { .. } | Item::Tool { .. } | Item::Todo(_) | Item::Permission { .. }
    )
}

/// The colours the tab draws with, from the theme's `UiColors` (text held to `TEXT_MIN` on what
/// it is drawn on).
#[derive(Clone, Copy, Debug)]
pub struct Colors {
    pub page: Rgb,
    pub text: Rgb,
    pub dim: Rgb,
    /// On a tool row or chip (`panel`).
    pub panel: Rgb,
    pub panel_border: Rgb,
    /// A hovered button on a chip (an attachment's ×).
    pub panel_hover: Rgb,
    pub panel_text: Rgb,
    pub panel_dim: Rgb,
    pub tool_name: Rgb,
    pub ok: Rgb,
    pub failed_panel: Rgb,
    /// Your message's card.
    pub card: Rgb,
    pub card_border: Rgb,
    pub card_text: Rgb,
    pub card_dim: Rgb,
    /// An error row's text.
    pub failed: Rgb,
    /// The permission card: its edge, fill, heading and quiet text.
    pub ask_edge: Rgb,
    pub ask_fill: Rgb,
    pub ask_head: Rgb,
    pub ask_dim: Rgb,
    pub accent: Rgb,
    pub on_accent: Rgb,
    pub todo_active: Rgb,
    /// Code (what a tool runs, its output) on the page.
    pub code_fill: Rgb,
    pub code_text: Rgb,
    pub code_dim: Rgb,
}

impl Colors {
    pub fn of(theme: &Theme) -> Colors {
        let ui: UiColors = theme.ui();
        let page = theme.background;
        let fit = |c: Rgb, on: Rgb| ensure_contrast(c, on, TEXT_MIN);
        let ask_fill = mix(page, ui.ring_waiting, 0.07);
        let code_fill = mix(page, ui.text, 0.06);
        Colors {
            page,
            text: fit(ui.text, page),
            dim: fit(ui.text_dim, page),
            panel: ui.panel,
            panel_border: ui.panel_border,
            panel_hover: ui.panel_hover,
            panel_text: fit(ui.text, ui.panel),
            panel_dim: fit(ui.text_dim, ui.panel),
            tool_name: fit(ui.accent, ui.panel),
            ok: fit(ui.block_ok, ui.panel),
            failed_panel: fit(ui.block_failed, ui.panel),
            card: ui.card,
            card_border: ui.card_border,
            card_text: fit(ui.text, ui.card),
            card_dim: fit(ui.text_dim, ui.card),
            failed: fit(ui.block_failed, page),
            ask_edge: ui.ring_waiting,
            ask_fill,
            ask_head: fit(ui.ring_waiting, ask_fill),
            ask_dim: fit(ui.text_dim, ask_fill),
            accent: ui.accent,
            on_accent: fit(ui.on_accent, ui.accent),
            todo_active: fit(ui.ring_working, ui.panel),
            code_fill,
            code_text: fit(ui.text, code_fill),
            code_dim: fit(ui.text_dim, code_fill),
        }
    }
}

/// One item's row as laid out last: its fingerprint, and (an agent message) its blocks, parsed
/// again only when its text changes.
struct Row {
    print: u64,
    blocks: Option<Rc<Vec<Md>>>,
}

/// This run of the process: what it was asked to resume, whether it said it started, where its
/// rows begin, and what was sent during it (sent again if the resume fails).
#[derive(Default)]
struct Run {
    resume: Option<String>,
    started: bool,
    first_item: usize,
    sends: Vec<(String, Vec<String>)>,
}

/// The `@` file list: where the `@` is, the query after it, the ranked hits, the selected one.
struct AtList {
    at: usize,
    query: String,
    hits: Vec<FileHit>,
    selected: usize,
}

/// The agent's folder's files for `@`: listed once per tab (off the UI thread), then kept.
struct Files {
    root: PathBuf,
    paths: Arc<Vec<String>>,
}

/// What the emitted events last said.
#[derive(Default)]
struct Said {
    status: Option<Status>,
    question: Option<String>,
    title: bool,
    conversation: bool,
}

pub struct AgentTab {
    provider: Option<&'static dyn Provider>,
    provider_name: String,
    start: Start,
    composer: Entity<Composer>,
    conversation: Conversation,
    /// The login shell's `PATH`, read once the tab opens.
    path: Option<String>,
    translator: Option<Box<dyn Translator>>,
    process: Option<Arc<AgentProcess>>,
    /// Every process this tab started that isn't reaped yet (an ended one dies within
    /// `TERM_GRACE`): its own, never a holder of its conversation (R13).
    started: Vec<Arc<AgentProcess>>,
    /// Reads the process's events; dropped to stop listening (a replaced or ended process).
    pump: Option<Task<()>>,
    end: ProcessEnd,
    /// The process has ended (a send starts a new one).
    exited: bool,
    /// History is being read: messages wait for it and the process (shown as waiting, under
    /// the conversation; sent after the history).
    booting: bool,
    /// The process has ended and its end isn't reported yet (≤ `END_WAIT`): messages wait.
    ending: bool,
    /// Its process was stopped for "Open in classic view" (R13): a send asks the Workspace first
    /// (`ResumeAsked`).
    paused: bool,
    queued: Vec<(String, Vec<String>)>,
    run: Run,
    /// Scenes: no process is ever started.
    no_process: bool,
    list: ListState,
    rows: Vec<Row>,
    /// Tool rows opened by a click (by tool id).
    expanded: HashSet<String>,
    /// When each running tool started (for its elapsed time).
    tool_started: std::collections::HashMap<String, Instant>,
    said: Said,
    /// Stop was pressed: the turn hasn't ended yet. `force`: `STOP_FORCE_AFTER` has passed.
    stopping: Option<Instant>,
    force: bool,
    attachments: Vec<String>,
    files: Option<Files>,
    listing: bool,
    at: Option<AtList>,
    rank_generation: Arc<AtomicU64>,
    ticker: Option<Task<()>>,
    end_timer: Option<Task<()>>,
    force_timer: Option<Task<()>>,
    _boot: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
    /// The command line of the last start (scenes check a restart resumes).
    #[cfg(feature = "ui-shots")]
    last_argv: Vec<String>,
    /// How many item rows the list built (scenes check only the rows in sight are laid out).
    #[cfg(feature = "ui-shots")]
    pub rows_built: Rc<Cell<usize>>,
    /// Where each waiting attachment's × was laid out (scenes click it).
    #[cfg(feature = "ui-shots")]
    removes: Rc<std::cell::RefCell<Vec<Option<gpui::Bounds<gpui::Pixels>>>>>,
}

impl EventEmitter<AgentTabEvent> for AgentTab {}

impl Focusable for AgentTab {
    /// The composer: the tab takes the keyboard there.
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.composer.read(cx).focus_handle(cx)
    }
}

impl AgentTab {
    /// A tab for `provider` (None: no agent by that name; the tab shows one error row). Starts
    /// reading the history and then the process on a task; emits nothing (R8). A conversation
    /// to resume is checked first against `classic` (its classic view, from before a restart).
    pub fn new(
        provider: Option<&'static dyn Provider>,
        provider_name: String,
        start: Start,
        classic: crate::workspace_agent::ClassicProbe,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut this = Self::base(provider, provider_name, start, window, cx);
        match provider {
            None => {
                let name = this.provider_name.clone();
                this.conversation
                    .failed_to_start(&format!("No agent called {name}"));
                this.sync_rows();
                // Not news: what the tab shows from the start.
                this.said.status = Some(this.conversation.status());
            }
            Some(provider) => {
                this.booting = true;
                let start = this.start.clone();
                this._boot = Some(cx.spawn(async move |this, cx| {
                    let (path, past, holder) = cx
                        .background_executor()
                        .spawn(async move {
                            let path = crate::workspace_ssh::login_env().and_then(|env| {
                                env.iter()
                                    .find(|(k, _)| k == "PATH")
                                    .map(|(_, v)| v.clone())
                            });
                            // A conversation to resume that another process holds (its classic
                            // view, still running after a restart): the tab won't start a second
                            // writer (R13).
                            let holder = match &start.resume {
                                Some(id) => crate::workspace_agent::conversation_holder(
                                    crate::procs::ps_table().as_deref(),
                                    provider,
                                    id,
                                    &[],
                                    &classic(),
                                ),
                                None => Holder::Free,
                            };
                            (path, provider.history(&start), holder)
                        })
                        .await;
                    this.update(cx, |this, cx| this.booted(path, past, holder, cx))
                        .ok();
                }));
            }
        }
        this
    }

    /// A tab with no process (scenes feed it events through `shot_feed`).
    #[cfg(feature = "ui-shots")]
    pub fn for_shots(
        provider: &'static dyn Provider,
        start: Start,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let name = provider.info().name.to_string();
        let mut this = Self::base(Some(provider), name, start, window, cx);
        this.no_process = true;
        this.translator = Some(provider.translator());
        this
    }

    /// Decoded events, into the same path a process line takes (R12).
    #[cfg(feature = "ui-shots")]
    pub fn shot_feed(&mut self, events: Vec<Event>, cx: &mut Context<Self>) {
        self.on_events(events, cx);
    }

    /// A process event, into the same path the process's take.
    #[cfg(feature = "ui-shots")]
    pub fn shot_process(&mut self, event: ProcessEvent, cx: &mut Context<Self>) {
        self.on_process(event, cx);
    }

    /// History, into the same path the boot task takes.
    #[cfg(feature = "ui-shots")]
    pub fn shot_history(&mut self, past: Vec<Past>, cx: &mut Context<Self>) {
        self.booted(None, past, Holder::Free, cx);
    }

    /// History, into the boot path, with what the boot's `ps` check found.
    #[cfg(feature = "ui-shots")]
    pub fn shot_history_held(&mut self, past: Vec<Past>, holder: Holder, cx: &mut Context<Self>) {
        self.booted(None, past, holder, cx);
    }

    /// The `@` list as if `@` and `query` were typed, over `paths` (from `root`).
    #[cfg(feature = "ui-shots")]
    pub fn shot_at(
        &mut self,
        root: PathBuf,
        paths: Vec<String>,
        query: &str,
        cx: &mut Context<Self>,
    ) {
        self.files = Some(Files {
            root,
            paths: Arc::new(paths),
        });
        let text = format!("@{query}");
        self.composer
            .update(cx, |c, cx| c.set_text(text.clone(), cx));
        self.at = Some(AtList {
            at: 0,
            query: String::new(),
            hits: Vec::new(),
            selected: 0,
        });
        self.composer_changed(cx);
    }

    /// A plain process in place of the agent (a scene's `sleep`, to see closing end it).
    #[cfg(feature = "ui-shots")]
    pub fn shot_spawn(&mut self, argv: Vec<String>, cx: &mut Context<Self>) {
        self.listen(&argv, cx);
    }

    /// Open or close tool row `ix`, as its click does.
    #[cfg(feature = "ui-shots")]
    pub fn shot_toggle(&mut self, ix: usize, cx: &mut Context<Self>) {
        self.toggle(ix, cx);
    }

    #[cfg(feature = "ui-shots")]
    pub fn shot_items(&self) -> &[Item] {
        self.conversation.items()
    }

    #[cfg(feature = "ui-shots")]
    pub fn shot_list(&self) -> ListState {
        self.list.clone()
    }

    #[cfg(feature = "ui-shots")]
    pub fn shot_composer(&self) -> Entity<Composer> {
        self.composer.clone()
    }

    /// The Stop button's label, if it shows one.
    #[cfg(feature = "ui-shots")]
    pub fn shot_stop_label(&self) -> Option<&'static str> {
        self.stop_label()
    }

    /// The tab as it is while its history and the login `PATH` are read (`new`'s boot task).
    #[cfg(feature = "ui-shots")]
    pub fn shot_boot(&mut self) {
        self.booting = true;
    }

    /// Messages waiting for the process.
    #[cfg(feature = "ui-shots")]
    pub fn shot_pending(&self) -> usize {
        self.queued.len()
    }

    /// The command line the last (re)start used (a scene starts nothing: it is only built).
    #[cfg(feature = "ui-shots")]
    pub fn shot_last_argv(&self) -> Vec<String> {
        self.last_argv.clone()
    }

    /// The conversation the tab resumes when its process starts again.
    #[cfg(feature = "ui-shots")]
    pub fn shot_resume(&self) -> Option<String> {
        self.start.resume.clone()
    }

    /// Whether tool `id`'s row is open.
    #[cfg(feature = "ui-shots")]
    pub fn shot_open(&self, id: &str) -> bool {
        self.expanded.contains(id)
    }

    /// The Stop / Force stop button, as its click.
    #[cfg(feature = "ui-shots")]
    pub fn shot_press_stop(&mut self, cx: &mut Context<Self>) {
        self.stop_pressed(cx);
    }

    /// Whether the `@` list has hits (scenes wait for the ranking).
    #[cfg(feature = "ui-shots")]
    pub fn at_hits(&self) -> usize {
        self.at.as_ref().map_or(0, |a| a.hits.len())
    }

    fn base(
        provider: Option<&'static dyn Provider>,
        provider_name: String,
        start: Start,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let label = provider.map_or(provider_name.as_str(), |p| p.info().label);
        let placeholder: SharedString = format!("Message {label}…").into();
        let composer = cx.new(|cx| Composer::new(placeholder, window, cx));
        let list = ListState::new(0, ListAlignment::Top, px(200.));
        list.set_follow_mode(gpui::FollowMode::Tail);
        let subscriptions = vec![
            cx.subscribe_in(&composer, window, Self::on_composer),
            cx.observe(&composer, |this, _, cx| this.composer_changed(cx)),
        ];
        Self {
            provider,
            provider_name,
            start,
            composer,
            conversation: Conversation::new(),
            path: None,
            translator: None,
            process: None,
            started: Vec::new(),
            pump: None,
            end: ProcessEnd::default(),
            exited: false,
            booting: false,
            ending: false,
            paused: false,
            queued: Vec::new(),
            run: Run::default(),
            no_process: false,
            list,
            rows: Vec::new(),
            expanded: HashSet::new(),
            tool_started: std::collections::HashMap::new(),
            said: Said::default(),
            stopping: None,
            force: false,
            attachments: Vec::new(),
            files: None,
            listing: false,
            at: None,
            rank_generation: Arc::new(AtomicU64::new(0)),
            ticker: None,
            end_timer: None,
            force_timer: None,
            _boot: None,
            _subscriptions: subscriptions,
            #[cfg(feature = "ui-shots")]
            last_argv: Vec::new(),
            #[cfg(feature = "ui-shots")]
            rows_built: Rc::new(Cell::new(0)),
            #[cfg(feature = "ui-shots")]
            removes: Rc::default(),
        }
    }

    pub fn status(&self) -> Status {
        self.conversation.status()
    }

    /// "⟨Label⟩ wants to run: ⟨what⟩" while a permission waits.
    pub fn question(&self) -> Option<String> {
        match self.conversation.pending_permission() {
            Some(Item::Permission { what, .. }) => {
                Some(format!("{} wants to run: {what}", self.label()))
            }
            _ => None,
        }
    }

    /// The mode the agent runs in (agy's chip; none for Claude).
    #[allow(dead_code)] // M11aa Task 12 (agy's mode, saved with the tab: call 6)
    pub fn mode(&self) -> Option<&str> {
        self.start.mode.as_deref()
    }

    /// The process, for quitting (`agent_process::end_all`).
    #[cfg(feature = "ui-shots")] // the scenes' (the app's own use `own_pids` / `all_reaped`)
    pub fn process(&self) -> Option<Arc<AgentProcess>> {
        self.process.clone()
    }

    /// Stop listening and end the process (closing the tab): no respawn.
    pub fn end(&mut self) {
        self.end_process();
    }

    /// "Open in classic view" (R13): stop the process (not an exit: no exit row; nothing that
    /// was booting or waiting goes on), one notice, Done (`live: false`: no banner). It comes
    /// back only through `resume_after_classic`.
    pub fn pause_for_classic(&mut self, cx: &mut Context<Self>) {
        self._boot = None;
        self.booting = false;
        self.ending = false;
        self.stopping = None;
        self.force = false;
        self.force_timer = None;
        self.end_process();
        self.paused = true;
        self.unqueue(cx);
        self.conversation.paused();
        self.after_change(false, cx);
    }

    /// Messages held for the process go back into the box (and their attachments onto the
    /// next message), so none is sent later behind your back; you send them again yourself.
    fn unqueue(&mut self, cx: &mut Context<Self>) {
        let queued = std::mem::take(&mut self.queued);
        if queued.is_empty() {
            return;
        }
        let mut texts: Vec<String> = Vec::new();
        for (text, attachments) in queued {
            texts.push(text);
            self.attachments.extend(attachments);
        }
        let typed = self.composer.read(cx).text().to_string();
        if !typed.trim().is_empty() {
            texts.push(typed);
        }
        let text = texts.join("\n\n");
        self.composer.update(cx, |c, cx| c.set_text(text, cx));
    }

    /// The pids of the processes this tab started that may still run (their groups: each is
    /// its group's leader).
    pub fn own_pids(&self) -> Vec<u32> {
        self.started
            .iter()
            .chain(&self.process)
            .filter(|p| !p.reaped())
            .filter_map(|p| p.pid())
            .map(|pid| pid as u32)
            .collect()
    }

    /// Whether every process this tab started has been reaped (none still writes).
    pub fn all_reaped(&self) -> bool {
        self.started.iter().chain(&self.process).all(|p| p.reaped())
    }

    /// Another process holds the conversation (or that couldn't be checked): one notice says so
    /// (`terminal`: where, when known); the tab stays paused.
    pub fn held(&mut self, holder: &Holder, terminal: Option<&str>, cx: &mut Context<Self>) {
        let notice = crate::workspace_agent::held_notice(self.label(), holder, terminal);
        self.conversation.apply(Event::Notice(notice));
        self.after_change(true, cx);
    }

    /// Its own process didn't stop in time, so its classic view didn't open: one notice says so;
    /// the tab stays paused.
    pub fn classic_not_opened(&mut self, cx: &mut Context<Self>) {
        let notice = crate::workspace_agent::not_stopped_notice(self.label());
        self.conversation.apply(Event::Notice(notice));
        self.after_change(true, cx);
    }

    /// One notice row (`text`), e.g. why something asked of the tab didn't happen yet.
    pub fn notice(&mut self, text: String, cx: &mut Context<Self>) {
        self.conversation.apply(Event::Notice(text));
        self.after_change(true, cx);
    }

    /// The conversation it resumes: the one the agent reported, else the one it was opened on.
    pub fn resume_id(&self) -> Option<String> {
        self.conversation_id()
    }

    /// A message from the paused tab (`ResumeAsked`), now that the Workspace knows whether
    /// another process holds the conversation (`holder`, from a fresh `ps`; `terminal`: the
    /// classic view's name, while it is open). Sent (the process started again with
    /// `--resume`, the box cleared), or a notice saying why not with `text` left in the box:
    /// never a second writer of one conversation, nor a start without an answer.
    pub fn resume_after_classic(
        &mut self,
        text: String,
        holder: Holder,
        terminal: Option<SharedString>,
        cx: &mut Context<Self>,
    ) {
        use crate::workspace_agent::{AfterClassic, after_classic, held_notice};
        match after_classic(self.paused, holder != Holder::Free) {
            AfterClassic::Warn => {
                let notice = held_notice(self.label(), &holder, terminal.as_deref());
                self.conversation.apply(Event::Notice(notice));
                self.after_change(true, cx);
                return;
            }
            AfterClassic::Respawn => {
                self.paused = false;
                let resume = self.conversation_id();
                self.spawn_process(resume, cx);
                self.send(text, cx);
            }
            AfterClassic::Send => self.send(text, cx),
        }
        self.composer
            .update(cx, |c, cx| c.set_text(String::new(), cx));
    }

    /// Whether the tab is paused for "Open in classic view".
    #[cfg(feature = "ui-shots")]
    pub fn shot_paused(&self) -> bool {
        self.paused
    }

    fn label(&self) -> &str {
        self.provider
            .map_or(self.provider_name.as_str(), |p| p.info().label)
    }

    /// The conversation's id: the one the agent reported, else the one being resumed.
    fn conversation_id(&self) -> Option<String> {
        self.conversation
            .conversation()
            .map(str::to_string)
            .or_else(|| self.start.resume.clone())
    }

    /// The history is read and the login `PATH` known: show what came before, start the process,
    /// and send what was typed meanwhile.
    fn booted(
        &mut self,
        path: Option<String>,
        past: Vec<Past>,
        holder: Holder,
        cx: &mut Context<Self>,
    ) {
        self.path = path;
        let none = past.is_empty();
        self.conversation.past(past);
        if self.start.resume.is_some() && none {
            self.conversation
                .apply(Event::Notice(RESTORED_NOTICE.into()));
        }
        self.booting = false;
        if crate::workspace_agent::starts_paused(self.start.resume.is_some(), &holder) {
            // Another process holds the conversation (R13): paused, as after "Open in classic
            // view"; a send checks again.
            self.paused = true;
            self.unqueue(cx);
            let notice = crate::workspace_agent::held_notice(self.label(), &holder, None);
            self.conversation.paused_with(&notice);
            self.after_change(false, cx);
            return;
        }
        self.after_change(false, cx);
        let resume = self.start.resume.clone();
        self.spawn_process(resume, cx);
        if !self.queued.is_empty() {
            self.flush_queued();
            self.after_change(true, cx);
        }
    }

    /// Send what waited for the process (booting, or the last one ending).
    fn flush_queued(&mut self) {
        for (text, attachments) in std::mem::take(&mut self.queued) {
            self.say(text, attachments);
        }
    }

    /// Show what you said and send it to the running process.
    fn say(&mut self, text: String, attachments: Vec<String>) {
        let wire = wire_text(&text, &attachments);
        self.conversation.you(text.clone(), attachments.clone());
        self.run.sends.push((text, attachments));
        self.write(&Action::Send { text: wire });
    }

    /// Start (or start again) the agent, resuming `resume`.
    fn spawn_process(&mut self, resume: Option<String>, cx: &mut Context<Self>) {
        self.end_process();
        self.run = Run {
            resume: resume.clone(),
            started: false,
            first_item: self.conversation.items().len(),
            sends: Vec::new(),
        };
        self.end = ProcessEnd::default();
        self.exited = false;
        let Some(provider) = self.provider else {
            return;
        };
        let start = Start {
            resume,
            ..self.start.clone()
        };
        let argv = provider.argv(&start);
        #[cfg(feature = "ui-shots")]
        {
            self.last_argv = argv.clone();
        }
        if self.no_process {
            return;
        }
        let process = self.listen(&argv, cx);
        let mut translator = provider.translator();
        for line in translator.hello() {
            process.send(line);
        }
        self.translator = Some(translator);
    }

    /// Start `argv` in the agent's folder and read its events on a task (`on_process`).
    fn listen(&mut self, argv: &[String], cx: &mut Context<Self>) -> Arc<AgentProcess> {
        let (tx, mut rx) = mpsc::unbounded();
        let process = Arc::new(agent_process::spawn(
            argv,
            &self.start.cwd,
            self.path.as_deref(),
            tx,
        ));
        self.process = Some(process.clone());
        self.started.retain(|p| !p.reaped());
        self.started.push(process.clone());
        self.pump = Some(cx.spawn(async move |this, cx| {
            while let Some(event) = rx.next().await {
                if this
                    .update(cx, |this, cx| this.on_process(event, cx))
                    .is_err()
                {
                    break;
                }
            }
        }));
        process
    }

    /// Stop listening to the process and end it (its group is signalled; `Drop` would too).
    fn end_process(&mut self) {
        self.pump = None;
        self.end_timer = None;
        if let Some(process) = self.process.take() {
            process.kill();
        }
    }

    fn write(&mut self, action: &Action) {
        if let (Some(translator), Some(process)) = (self.translator.as_mut(), &self.process) {
            for line in translator.encode(action) {
                process.send(line);
            }
        }
    }

    fn on_process(&mut self, event: ProcessEvent, cx: &mut Context<Self>) {
        match event {
            ProcessEvent::Line(line) => {
                let events = match self.translator.as_mut() {
                    Some(t) => t.decode(&line),
                    None => Vec::new(),
                };
                if !events.is_empty() {
                    self.on_events(events, cx);
                }
            }
            ProcessEvent::Ended { code } => {
                // The process is gone: nothing more can be written to it; a message sent now
                // waits for the end to be reported and goes to the restarted process.
                self.ending = true;
                self.process = None;
                if self.end.ended(code) {
                    self.end_timer = Some(cx.spawn(async move |this, cx| {
                        cx.background_executor().timer(END_WAIT).await;
                        this.update(cx, |this, cx| {
                            if let Some((code, tail)) = this.end.waited() {
                                this.finish(code, tail, cx);
                            }
                        })
                        .ok();
                    }));
                }
            }
            ProcessEvent::Exited { code, stderr_tail } => {
                if let Some((code, tail)) = self.end.exited(code, stderr_tail) {
                    self.end_timer = None;
                    self.finish(code, tail, cx);
                }
            }
            ProcessEvent::SpawnFailed(message) => {
                self.exited = true;
                self.ending = false;
                self.process = None;
                let label = self.label().to_string();
                let why = format!("Couldn't start {label}: {message}");
                self.conversation.failed_to_start(&why);
                cx.emit(AgentTabEvent::Failed(why));
                self.after_change(true, cx);
            }
        }
    }

    /// The process is gone (reported once). A resume that never started is a new conversation
    /// (one notice, the process again without `--resume`, what you sent sent again); else the
    /// conversation hears it ended.
    fn finish(&mut self, code: Option<i32>, tail: String, cx: &mut Context<Self>) {
        self.exited = true;
        self.ending = false;
        self.process = None;
        self.pump = None;
        self.stopping = None;
        // What the run said on its way out: its stderr tail and its error rows.
        let said: Vec<String> = self.conversation.items()[self.run.first_item..]
            .iter()
            .filter_map(|i| match i {
                Item::Error(e) => Some(e.clone()),
                _ => None,
            })
            .chain(std::iter::once(tail.clone()))
            .collect();
        let unknown = self
            .provider
            .is_some_and(|p| said.iter().any(|s| p.unknown_conversation(s)));
        let failed_resume = resume_failed(self.run.started, self.run.resume.is_some());
        if failed_resume && !unknown {
            // It couldn't start (login, network, a crash): the conversation is still there, and
            // the tab keeps its id. One error row says why.
            let sends = std::mem::take(&mut self.run.sends);
            self.conversation.keep_first(self.run.first_item);
            for (text, attachments) in sends {
                self.conversation.you(text, attachments);
            }
            let why = said
                .iter()
                .rev()
                .map(|s| s.trim())
                .find(|s| !s.is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| match code {
                    Some(c) => format!("it exited (code {c})"),
                    None => "it was terminated".into(),
                });
            let label = self.label().to_string();
            let why = format!("Couldn't start {label}: {why}");
            self.conversation.failed_to_start(&why);
            cx.emit(AgentTabEvent::Failed(why));
        } else if failed_resume {
            let sends = std::mem::take(&mut self.run.sends);
            // The "restored" notice said there was a conversation; there isn't.
            let mut keep = self.run.first_item;
            if keep > 0
                && matches!(&self.conversation.items()[keep - 1], Item::Notice(t) if t == RESTORED_NOTICE)
            {
                keep -= 1;
            }
            self.conversation.keep_first(keep);
            self.conversation
                .apply(Event::Notice(BAD_RESUME_NOTICE.into()));
            self.start.resume = None;
            self.spawn_process(None, cx);
            for (text, attachments) in sends {
                self.say(text, attachments);
            }
            self.flush_queued();
        } else {
            self.conversation.process_ended(code, &tail);
            // Not a clean exit: its error row's words.
            if let Some(Item::Error(why)) = self.conversation.items().last()
                && code != Some(0)
            {
                cx.emit(AgentTabEvent::Failed(why.clone()));
            }
            if !self.queued.is_empty() {
                let resume = self.conversation_id();
                self.spawn_process(resume, cx);
                self.flush_queued();
            }
        }
        self.after_change(true, cx);
    }

    /// Events from the agent (or a scene): applied, the changed rows laid out again, then what
    /// changed told — `Question`, `Status` (after it, so a Waiting banner has its question),
    /// `Title`, `Conversation` — and the tab redrawn.
    fn on_events(&mut self, events: Vec<Event>, cx: &mut Context<Self>) {
        for event in events {
            match &event {
                Event::Started { .. } => self.run.started = true,
                Event::ToolStarted { id, .. } => {
                    self.tool_started.insert(id.clone(), Instant::now());
                }
                Event::TurnDone { .. } => {
                    self.stopping = None;
                    self.force = false;
                    self.force_timer = None;
                }
                _ => {}
            }
            self.conversation.apply(event);
        }
        self.after_change(true, cx);
    }

    /// After the conversation changed: rows, events, placeholder, the elapsed ticker, a redraw.
    fn after_change(&mut self, live: bool, cx: &mut Context<Self>) {
        self.sync_rows();
        let question = self.question();
        if question != self.said.question {
            self.said.question = question.clone();
            cx.emit(AgentTabEvent::Question(question));
        }
        let status = self.conversation.status();
        if self.said.status != Some(status) {
            self.said.status = Some(status);
            cx.emit(AgentTabEvent::Status { status, live });
        }
        if !self.said.title
            && let Some(title) = self.conversation.title()
        {
            self.said.title = true;
            cx.emit(AgentTabEvent::Title(title));
        }
        if !self.said.conversation
            && let Some(id) = self.conversation.conversation()
        {
            self.said.conversation = true;
            cx.emit(AgentTabEvent::Conversation(id.to_string()));
        }
        self.update_placeholder(cx);
        self.keep_ticking(cx);
        cx.notify();
    }

    /// Lay out again the rows whose items changed (and a sub-agent's parent row), add the new
    /// ones at the end, drop any gone from it.
    fn sync_rows(&mut self) {
        let items = self.conversation.items();
        let new: Vec<u64> = items.iter().map(fingerprint).collect();
        let old: Vec<u64> = self.rows.iter().map(|r| r.print).collect();
        let mut dirty = changed_rows(&old, &new);
        // A sub-agent's item is drawn inside its parent's row.
        let touched: Vec<usize> = dirty.iter().copied().chain(old.len()..new.len()).collect();
        for i in touched {
            if let Some(parent) = items.get(i).and_then(parent_of)
                && let Some(p) = items
                    .iter()
                    .position(|t| matches!(t, Item::Tool { id, .. } if id == parent))
                && p < old.len().min(new.len())
                && !dirty.contains(&p)
            {
                dirty.push(p);
            }
        }
        if new.len() < old.len() {
            self.list.splice(new.len()..old.len(), 0);
            self.rows.drain(new.len()..);
        }
        for &i in &dirty {
            if i < new.len() {
                self.list.splice(i..i + 1, 1);
                self.rows[i] = Row {
                    print: new[i],
                    blocks: blocks_of(&items[i]),
                };
            }
        }
        if new.len() > old.len() {
            self.list
                .splice(old.len()..old.len(), new.len() - old.len());
            for i in old.len()..new.len() {
                self.rows.push(Row {
                    print: new[i],
                    blocks: blocks_of(&items[i]),
                });
            }
        }
    }

    /// While a tool runs, its elapsed time turns each second.
    fn keep_ticking(&mut self, cx: &mut Context<Self>) {
        let running = self
            .conversation
            .items()
            .iter()
            .any(|i| matches!(i, Item::Tool { running: true, .. }));
        if !running {
            self.ticker = None;
            return;
        }
        if self.ticker.is_some() {
            return;
        }
        self.ticker = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                let more = this
                    .update(cx, |this, cx| {
                        let mut any = false;
                        for (i, item) in this.conversation.items().iter().enumerate() {
                            if matches!(item, Item::Tool { running: true, .. }) {
                                this.list.splice(i..i + 1, 1);
                                any = true;
                            }
                        }
                        if any {
                            cx.notify();
                        } else {
                            this.ticker = None;
                        }
                        any
                    })
                    .unwrap_or(false);
                if !more {
                    break;
                }
            }
        }));
    }

    fn update_placeholder(&mut self, cx: &mut Context<Self>) {
        let label = self.label().to_string();
        let text: SharedString = if self.conversation.pending_permission().is_some() {
            "Deny and say why… (optional)".into()
        } else if self.status() == Status::Working {
            format!("Add a note. {label} reads it after this step…").into()
        } else {
            format!("Message {label}…").into()
        };
        self.composer
            .update(cx, |c, cx| c.set_placeholder(text, cx));
    }

    /// Send `text` (with the attachments waiting) to the agent; mid-turn it goes at once (the
    /// agent queues it). An ended process is started again (resuming the conversation).
    pub fn send(&mut self, text: String, cx: &mut Context<Self>) {
        let attachments = std::mem::take(&mut self.attachments);
        if self.provider.is_none() || (text.trim().is_empty() && attachments.is_empty()) {
            self.attachments = attachments;
            return;
        }
        if self.paused {
            // The Workspace decides (R13): `resume_after_classic`.
            self.attachments = attachments;
            cx.emit(AgentTabEvent::ResumeAsked(text));
            return;
        }
        if self.booting || self.ending {
            // Held, shown as waiting under the conversation, until the process is there.
            self.queued.push((text, attachments));
            cx.notify();
            return;
        }
        if self.process.is_none() && !self.no_process {
            let resume = self.conversation_id();
            self.spawn_process(resume, cx);
        }
        self.flush_queued();
        self.say(text, attachments);
        self.after_change(true, cx);
    }

    /// Something to send with the next message (Task 11: a command's output), shown as a chip
    /// under the composer until it is sent. Only kept: it never sends, and never restarts a
    /// paused tab (R13); an error tab (no agent) has nothing to send it to.
    pub fn attach(&mut self, attachment: String, cx: &mut Context<Self>) {
        if self.provider.is_none() {
            return;
        }
        self.attachments.push(attachment);
        cx.notify();
    }

    /// The × on a waiting attachment's chip: it goes, and is never sent (nothing else changes:
    /// nothing is sent or started).
    pub fn remove_attachment_at(&mut self, i: usize, cx: &mut Context<Self>) {
        if remove_attachment(&mut self.attachments, i) {
            cx.notify();
        }
    }

    /// Where waiting attachment `i`'s × was last laid out.
    #[cfg(feature = "ui-shots")]
    pub fn shot_remove_bounds(&self, i: usize) -> Option<gpui::Bounds<gpui::Pixels>> {
        self.removes.borrow().get(i).copied().flatten()
    }

    /// The attachments waiting for your next message.
    #[cfg(feature = "ui-shots")]
    pub fn shot_attachments(&self) -> &[String] {
        &self.attachments
    }

    /// Answer the waiting permission.
    pub fn answer(&mut self, answer: Answer, cx: &mut Context<Self>) -> Result<(), &'static str> {
        let Some(Item::Permission { id, .. }) = self.conversation.pending_permission() else {
            return Err("not waiting");
        };
        let id = id.clone();
        let action = match &answer {
            Answer::Allowed => Action::Allow { id: id.clone() },
            Answer::AlwaysAllowed => Action::AlwaysAllow { id: id.clone() },
            Answer::Denied(reason) => Action::Deny {
                id: id.clone(),
                reason: reason.clone(),
            },
            Answer::Expired => Action::Deny {
                id: id.clone(),
                reason: None,
            },
        };
        self.write(&action);
        self.conversation.answered(&id, answer);
        self.after_change(true, cx);
        Ok(())
    }

    /// Stop the turn: an interrupt; the button reads "Stopping…" until the turn ends, and
    /// offers Force stop after `STOP_FORCE_AFTER`.
    pub fn stop(&mut self, cx: &mut Context<Self>) {
        if !matches!(self.status(), Status::Working | Status::Waiting) || self.stopping.is_some() {
            return;
        }
        self.write(&Action::Interrupt);
        self.stopping = Some(Instant::now());
        self.force = false;
        self.force_timer = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(STOP_FORCE_AFTER).await;
            this.update(cx, |this, cx| {
                if this.stopping.is_some() {
                    this.force = true;
                    cx.notify();
                }
            })
            .ok();
        }));
        cx.notify();
    }

    /// The right button: Stop, or Force stop once it is offered.
    fn stop_pressed(&mut self, cx: &mut Context<Self>) {
        if self.force {
            self.force_stop(cx);
        } else {
            self.stop(cx);
        }
    }

    /// Force stop: end the process, then start it again resuming the conversation.
    fn force_stop(&mut self, cx: &mut Context<Self>) {
        self.stopping = None;
        self.force = false;
        self.force_timer = None;
        let resume = self.conversation_id();
        self.end_process();
        self.conversation.apply(Event::Notice("Stopped".into()));
        self.conversation.apply(Event::TurnDone {
            ok: false,
            message: None,
        });
        self.spawn_process(resume, cx);
        self.after_change(true, cx);
    }

    /// Enter: picks the `@` file; with a permission waiting, empty allows and text denies with
    /// it as the reason; else sends.
    fn on_composer(
        &mut self,
        composer: &Entity<Composer>,
        event: &ComposerEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            ComposerEvent::Send(text) => {
                if self.at.is_some() {
                    self.pick_at(None, cx);
                    return;
                }
                let text = text.clone();
                if self.conversation.pending_permission().is_some() {
                    let answer = if text.trim().is_empty() {
                        Answer::Allowed
                    } else {
                        Answer::Denied(Some(text.trim().to_string()))
                    };
                    self.answer(answer, cx).ok();
                } else if text.trim().is_empty() && self.attachments.is_empty() {
                    return;
                } else if self.paused {
                    // The box keeps the text until it is sent (`resume_after_classic`).
                    self.send(text, cx);
                    return;
                } else {
                    self.send(text, cx);
                }
                composer.update(cx, |c, cx| c.set_text(String::new(), cx));
            }
            ComposerEvent::Stop => {
                if self.at.take().is_some() {
                    cx.notify();
                } else {
                    self.stop(cx);
                }
            }
            ComposerEvent::AtTyped => self.open_at(cx),
        }
    }

    /// An `@` at a word's start: the file list opens (listing the files the first time).
    fn open_at(&mut self, cx: &mut Context<Self>) {
        let cursor = self.composer.read(cx).cursor();
        self.at = Some(AtList {
            at: cursor.saturating_sub(1),
            query: String::new(),
            hits: Vec::new(),
            selected: 0,
        });
        if self.files.is_none() && !self.listing {
            self.listing = true;
            let cwd = self.start.cwd.clone();
            cx.spawn(async move |this, cx| {
                let answer = cx
                    .background_executor()
                    .spawn(async move { crate::palette_files::list_files(&cwd) })
                    .await;
                this.update(cx, |this, cx| {
                    this.listing = false;
                    let (root, paths) = match answer {
                        Some(a) => (a.root, a.listed.map(|l| l.paths).unwrap_or_default()),
                        None => (this.start.cwd.clone(), Vec::new()),
                    };
                    this.files = Some(Files {
                        root,
                        paths: Arc::new(paths),
                    });
                    this.rank(cx);
                    cx.notify();
                })
                .ok();
            })
            .detach();
        }
        cx.notify();
    }

    /// The composer changed: the `@` list follows its query, or closes.
    fn composer_changed(&mut self, cx: &mut Context<Self>) {
        let Some(at) = &self.at else {
            return;
        };
        let composer = self.composer.read(cx);
        let query = at_query(composer.text(), at.at, composer.cursor()).map(str::to_string);
        match query {
            None => {
                self.at = None;
                cx.notify();
            }
            Some(q) if q != at.query => {
                if let Some(at) = &mut self.at {
                    at.query = q;
                }
                self.rank(cx);
            }
            Some(_) => {}
        }
    }

    /// Rank the files for the `@` query, off the UI thread (a newer query cancels it).
    fn rank(&mut self, cx: &mut Context<Self>) {
        let (Some(at), Some(files)) = (&self.at, &self.files) else {
            return;
        };
        let generation = self.rank_generation.fetch_add(1, Ordering::SeqCst) + 1;
        let current = self.rank_generation.clone();
        let paths = files.paths.clone();
        let query = at.query.clone();
        cx.spawn(async move |this, cx| {
            let ranked = cx
                .background_executor()
                .spawn(async move {
                    crate::palette_files::rank_files(&paths, &query, AT_LIMIT, generation, &current)
                })
                .await;
            let Some((hits, _)) = ranked else {
                return;
            };
            this.update(cx, |this, cx| {
                if this.rank_generation.load(Ordering::SeqCst) != generation {
                    return;
                }
                if let Some(at) = &mut this.at {
                    at.hits = hits;
                    at.selected = 0;
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    /// Put the picked file (`index`, else the selected one) in place of `@query`.
    fn pick_at(&mut self, index: Option<usize>, cx: &mut Context<Self>) {
        let Some(at) = self.at.take() else {
            return;
        };
        let pick = index.unwrap_or(at.selected);
        if let (Some(hit), Some(files)) = (at.hits.get(pick), &self.files) {
            let path = inserted_path(&files.root, &self.start.cwd, &hit.rel);
            let end = at.at + 1 + at.query.len();
            self.composer.update(cx, |c, cx| {
                c.replace(at.at..end, &format!("{path} "), cx);
            });
        }
        cx.notify();
    }

    fn at_up(&mut self, _: &AtUp, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(at) = &mut self.at {
            at.selected = at.selected.saturating_sub(1);
            cx.notify();
        }
    }

    fn at_down(&mut self, _: &AtDown, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(at) = &mut self.at {
            at.selected = (at.selected + 1).min(at.hits.len().saturating_sub(1));
            cx.notify();
        }
    }

    /// A tool row's click: open or close its detail.
    fn toggle(&mut self, ix: usize, cx: &mut Context<Self>) {
        let Some(Item::Tool { id, .. }) = self.conversation.items().get(ix) else {
            return;
        };
        if !self.expanded.remove(id) {
            self.expanded.insert(id.clone());
        }
        self.list.splice(ix..ix + 1, 1);
        cx.notify();
    }

    /// You have sent a message in this tab.
    fn spoken(&self) -> bool {
        self.conversation
            .items()
            .iter()
            .any(|i| matches!(i, Item::You { .. }))
    }

    /// The mode chip's click (call 6): before the first message, the agent's next mode (its
    /// process starts again in it); after it, the chip is read-only.
    fn next_mode(&mut self, cx: &mut Context<Self>) {
        let Some(provider) = self.provider else {
            return;
        };
        let modes = provider.info().modes;
        if modes.is_empty() || self.spoken() {
            return;
        }
        let at = self
            .start
            .mode
            .as_deref()
            .and_then(|m| modes.iter().position(|x| *x == m));
        let next = modes[at.map_or(0, |i| (i + 1) % modes.len())];
        self.start.mode = Some(next.to_string());
        if self.process.is_some() {
            let resume = self.start.resume.clone();
            self.spawn_process(resume, cx);
        }
        cx.notify();
    }

    fn open_classic(&mut self, cx: &mut Context<Self>) {
        if let (Some(provider), Some(id)) = (self.provider, self.conversation_id()) {
            cx.emit(AgentTabEvent::OpenClassic(provider.classic_argv(&id)));
        }
    }

    /// A link in a message: the web opens in the browser; anything else does nothing.
    fn follow_link(&mut self, url: &str, cx: &mut Context<Self>) {
        if let LinkTarget::Web(url) = md_preview::link_target(&self.start.cwd, url) {
            cx.open_url(&url);
        }
    }
}

/// An agent message's blocks.
fn blocks_of(item: &Item) -> Option<Rc<Vec<Md>>> {
    match item {
        Item::Agent { text, .. } => Some(Rc::new(parse(text))),
        _ => None,
    }
}

/// What drawing a row needs.
struct Look {
    c: Colors,
    mono: SharedString,
    label: SharedString,
    badge: SharedString,
    badge_rgb: u32,
    draw: md_preview::Draw,
    diff: DiffRowColours,
    view: WeakEntity<AgentTab>,
    now: Instant,
}

/// A small button: `primary` filled with the accent.
fn button(
    id: impl Into<gpui::ElementId>,
    label: impl Into<SharedString>,
    primary: bool,
    c: &Colors,
) -> Stateful<gpui::Div> {
    let (fill, ink, edge) = if primary {
        (c.accent, c.on_accent, c.accent)
    } else {
        (c.panel, c.panel_text, c.panel_border)
    };
    div()
        .id(id.into())
        .px(px(12.))
        .py(px(3.))
        .rounded(px(6.))
        .border_1()
        .border_color(hsla(edge))
        .bg(hsla(fill))
        .text_color(hsla(ink))
        .when(primary, |d| d.font_weight(FontWeight::SEMIBOLD))
        .cursor_pointer()
        .child(label.into())
}

/// A chip on the composer.
fn chip(text: impl Into<SharedString>, dim: bool, c: &Colors) -> gpui::Div {
    div()
        .px(px(10.))
        .py(px(2.))
        .rounded(px(999.))
        .border_1()
        .border_color(hsla(c.panel_border))
        .bg(hsla(c.panel))
        .text_size(px(text::META))
        .text_color(hsla(if dim { c.panel_dim } else { c.panel_text }))
        .child(text.into())
}

/// An action on the tab, from a row's element (it has no `Context` of its own).
fn on_tab(
    view: &WeakEntity<AgentTab>,
    f: impl Fn(&mut AgentTab, &mut Context<AgentTab>) + 'static,
) -> impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static {
    let view = view.clone();
    move |_, _, cx| {
        view.update(cx, |this, cx| f(this, cx)).ok();
    }
}

impl AgentTab {
    /// Item `ix`'s row of the list.
    fn item_row(&self, ix: usize, look: &Look) -> AnyElement {
        let items = self.conversation.items();
        let Some(item) = items.get(ix) else {
            return div().into_any_element();
        };
        // A sub-agent's items are drawn inside their parent's row.
        if let Some(parent) = parent_of(item)
            && items
                .iter()
                .any(|t| matches!(t, Item::Tool { id, .. } if id == parent))
        {
            return div().into_any_element();
        }
        let prev = items[..ix].iter().rev().find(|i| {
            parent_of(i).is_none_or(|p| {
                !items
                    .iter()
                    .any(|t| matches!(t, Item::Tool { id, .. } if id == p))
            })
        });
        let starts_group = !(agent_side(item) && prev.is_some_and(agent_side));
        let top = match (prev, starts_group) {
            (None, _) => 14.,
            (Some(_), true) => 14.,
            (Some(_), false) => 6.,
        };
        let last = ix + 1 == items.len();
        let body = self.item_body(ix, item, 0, look);
        let row = div()
            .w_full()
            .px(px(16.))
            .pt(px(top))
            .when(last, |d| d.pb(px(14.)))
            .text_size(px(BODY))
            .line_height(relative(1.45));
        let row = if agent_side(item) {
            let gutter = if starts_group {
                div()
                    .flex_none()
                    .size(px(AVATAR))
                    .rounded(px(6.))
                    .bg(hsla(Rgb {
                        r: (look.badge_rgb >> 16) as u8,
                        g: (look.badge_rgb >> 8) as u8,
                        b: look.badge_rgb as u8,
                    }))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(text::META))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(gpui::white())
                    .child(look.badge.clone())
            } else {
                div().flex_none().w(px(AVATAR))
            };
            row.flex()
                .flex_row()
                .gap(px(AVATAR_GAP))
                .child(gutter)
                .child(div().flex_1().min_w(px(0.)).child(body))
        } else {
            row.child(body)
        };
        row.into_any_element()
    }

    /// An item's own look (inside a row, or nested `depth` deep under a tool).
    fn item_body(&self, ix: usize, item: &Item, depth: usize, look: &Look) -> AnyElement {
        let c = &look.c;
        match item {
            Item::You { text, attachments } => div()
                .w_full()
                .flex()
                .justify_end()
                .child(
                    div()
                        .max_w(relative(0.8))
                        .px(px(12.))
                        .py(px(8.))
                        .rounded(px(12.))
                        .bg(hsla(c.card))
                        .border_1()
                        .border_color(hsla(c.card_border))
                        .text_color(hsla(c.card_text))
                        .flex()
                        .flex_col()
                        .gap(px(5.))
                        .child(SharedString::from(text.clone()))
                        .children(attachments.iter().map(|a| {
                            div()
                                .px(px(8.))
                                .py(px(2.))
                                .rounded(px(6.))
                                .border_1()
                                .border_color(hsla(c.card_border))
                                .text_size(px(text::META))
                                .text_color(hsla(c.card_dim))
                                .child(SharedString::from(format!(
                                    "📎 {}",
                                    a.lines().next().unwrap_or("")
                                )))
                        })),
                )
                .into_any_element(),
            Item::Agent { .. } => {
                let blocks = self
                    .rows
                    .get(ix)
                    .and_then(|r| r.blocks.clone())
                    .unwrap_or_default();
                div()
                    .w_full()
                    .text_color(hsla(c.text))
                    .when(depth > 0, |d| d.text_color(hsla(c.dim)))
                    .child(md_preview::column(&blocks, ix, &look.draw))
                    .into_any_element()
            }
            Item::Tool { .. } => self.tool(ix, item, depth, look),
            Item::Todo(list) => div()
                .w_full()
                .px(px(10.))
                .py(px(8.))
                .rounded(px(7.))
                .bg(hsla(c.panel))
                .border_1()
                .border_color(hsla(c.panel_border))
                .text_size(px(BODY - 1.))
                .flex()
                .flex_col()
                .gap(px(1.))
                .children(list.iter().map(|t| {
                    let (mark, ink) = match t.state {
                        TodoState::Done => ("✓", c.ok),
                        TodoState::Active => ("◐", c.todo_active),
                        TodoState::Pending => ("○", c.panel_dim),
                    };
                    div()
                        .text_color(hsla(ink))
                        .child(SharedString::from(format!("{mark} {}", t.text)))
                }))
                .into_any_element(),
            Item::Permission {
                tool,
                what,
                detail,
                always,
                answer,
                ..
            } => match answer {
                Some(answer) => div()
                    .w_full()
                    .text_color(hsla(c.dim))
                    .text_size(px(BODY - 1.))
                    .child(SharedString::from(format!(
                        "{} · {tool} {what}",
                        answer_line(answer)
                    )))
                    .into_any_element(),
                None => self.permission_card(tool, what, detail, always.as_deref(), look),
            },
            Item::Notice(t) => div()
                .w_full()
                .text_size(px(BODY - 1.))
                .text_color(hsla(c.dim))
                .child(SharedString::from(t.clone()))
                .into_any_element(),
            Item::Error(t) => {
                let classic = self.provider.is_some() && self.conversation_id().is_some();
                let closable = self.provider.is_none();
                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .child(
                        div()
                            .text_color(hsla(c.failed))
                            .child(SharedString::from(t.clone())),
                    )
                    .when(classic || closable, |d| {
                        d.child(
                            div()
                                .flex()
                                .gap(px(6.))
                                .text_size(px(text::META + 0.5))
                                .when(classic, |d| {
                                    d.child(
                                        button(
                                            ("agent-classic", ix),
                                            "Open in classic view",
                                            false,
                                            c,
                                        )
                                        .on_click(on_tab(&look.view, |this, cx| {
                                            this.open_classic(cx)
                                        })),
                                    )
                                })
                                .when(closable, |d| {
                                    d.child(
                                        button(("agent-close", ix), "Close", false, c).on_click(
                                            on_tab(&look.view, |_, cx| {
                                                cx.emit(AgentTabEvent::Close)
                                            }),
                                        ),
                                    )
                                }),
                        )
                    })
                    .into_any_element()
            }
        }
    }

    /// A tool call's row: name, target, result at the right (a spinner and the time while it
    /// runs); a click opens its diff, its command and output, or its sub-agent's steps.
    fn tool(&self, ix: usize, item: &Item, depth: usize, look: &Look) -> AnyElement {
        let Item::Tool {
            id,
            name,
            summary,
            running,
            ok,
            result,
            detail,
            output,
            dropped_lines,
            ..
        } = item
        else {
            return div().into_any_element();
        };
        let c = &look.c;
        let items = self.conversation.items();
        let children: Vec<usize> = items
            .iter()
            .enumerate()
            .filter(|(_, i)| parent_of(i) == Some(id.as_str()))
            .map(|(i, _)| i)
            .collect();
        let diff = match detail {
            Some(ToolDetail::Diff { unified, .. }) => Some(parse_diff(unified)),
            _ => None,
        };
        // A read expands to nothing (AI's call); an edit to its diff (its "updated" line says
        // nothing more), unless it failed.
        let shows_output =
            name != "Read" && output.is_some() && (diff.is_none() || *ok == Some(false));
        let openable = diff.is_some()
            || matches!(detail, Some(ToolDetail::Command(_)))
            || shows_output
            || !children.is_empty();
        let open = openable && self.expanded.contains(id);
        let right: (String, Rgb) = if *running {
            let since = self
                .tool_started
                .get(id)
                .map_or(Duration::ZERO, |t| look.now.saturating_duration_since(*t));
            let spin = ["◐", "◓", "◑", "◒"][(since.as_secs() % 4) as usize];
            (format!("{spin} {}", elapsed_label(since)), c.panel_dim)
        } else if *ok == Some(false) {
            (format!("✗ {result}"), c.failed_panel)
        } else if let Some(d) = &diff {
            let added = d.rows.iter().filter(|r| r.kind == RowKind::Added).count();
            let removed = d.rows.iter().filter(|r| r.kind == RowKind::Removed).count();
            (format!("+{added} −{removed}"), c.panel_dim)
        } else if let (true, Some(out)) = (name == "Read", output) {
            (lines_label(out.lines().count()), c.panel_dim)
        } else {
            (result.clone(), c.panel_dim)
        };
        let steps = (!children.is_empty()).then(|| format!("{} steps", children.len()));
        let head = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(8.))
            .min_w(px(0.))
            .child(
                div()
                    .flex_none()
                    .text_color(hsla(c.tool_name))
                    .font_weight(FontWeight::MEDIUM)
                    .child(SharedString::from(name.clone())),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .truncate()
                    .text_color(hsla(c.panel_text))
                    .child(SharedString::from(summary.clone())),
            )
            .when_some(steps, |d, s| {
                d.child(
                    div()
                        .flex_none()
                        .text_color(hsla(c.panel_dim))
                        .child(SharedString::from(s)),
                )
            })
            .child(
                div()
                    .flex_none()
                    .max_w(relative(0.45))
                    .truncate()
                    .text_color(hsla(right.1))
                    .child(SharedString::from(right.0)),
            )
            .when(openable, |d| {
                d.child(
                    div()
                        .flex_none()
                        .text_color(hsla(c.panel_dim))
                        .child(if open { "▾" } else { "▸" }),
                )
            });
        let mut tool = div()
            .id(("agent-tool", ix))
            .w_full()
            .px(px(10.))
            .py(px(6.))
            .rounded(px(7.))
            .bg(hsla(c.panel))
            .border_1()
            .border_color(hsla(c.panel_border))
            .text_size(px(BODY - 1.))
            .flex()
            .flex_col()
            .gap(px(6.));
        // Only the header toggles: a click in the diff or the output (to select it) doesn't.
        let head = if openable {
            let view = look.view.clone();
            head.cursor_pointer()
                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                    cx.stop_propagation();
                    view.update(cx, |this, cx| this.toggle(ix, cx)).ok();
                })
        } else {
            head
        };
        tool = tool.child(head);
        if open {
            if let Some(d) = &diff {
                tool = tool.child(self.diff_block(d, look));
            }
            if let Some(ToolDetail::Command(command)) = detail {
                tool = tool.child(
                    div()
                        .font_family(look.mono.clone())
                        .text_color(hsla(c.panel_dim))
                        .child(SharedString::from(format!("$ {command}"))),
                );
            }
            if shows_output && let Some(out) = output {
                tool = tool.child(
                    div()
                        .w_full()
                        .px(px(10.))
                        .py(px(6.))
                        .rounded(px(5.))
                        .bg(hsla(c.code_fill))
                        .font_family(look.mono.clone())
                        .text_size(px(BODY - 1.5))
                        .flex()
                        .flex_col()
                        .when(*dropped_lines > 0, |d| {
                            d.child(div().text_color(hsla(c.code_dim)).child(SharedString::from(
                                format!("… {dropped_lines} earlier lines"),
                            )))
                        })
                        .child(
                            div()
                                .text_color(hsla(c.code_text))
                                .child(SharedString::from(out.clone())),
                        ),
                );
            }
            if !children.is_empty() {
                tool = tool.child(
                    div()
                        .pl(px(10.))
                        .border_l_1()
                        .border_color(hsla(c.panel_border))
                        .flex()
                        .flex_col()
                        .gap(px(6.))
                        .children(
                            children
                                .iter()
                                .map(|&i| self.item_body(i, &items[i], depth + 1, look)),
                        ),
                );
            }
        }
        tool.into_any_element()
    }

    /// An edit's diff, coloured as the file tab colours its diff rows.
    fn diff_block(&self, d: &crate::diff_view::Parsed, look: &Look) -> AnyElement {
        let c = &look.c;
        let lines: Vec<&str> = d.text.split('\n').collect();
        div()
            .w_full()
            .py(px(4.))
            .rounded(px(5.))
            .bg(hsla(c.page))
            .font_family(look.mono.clone())
            .text_size(px(BODY - 1.5))
            .flex()
            .flex_col()
            .children(d.rows.iter().zip(lines).map(|(row, line)| {
                let (fill, sign, ink) = match row.kind {
                    RowKind::Added => (Some(look.diff.added_fill), "+", look.diff.added_ink),
                    RowKind::Removed => (Some(look.diff.removed_fill), "−", look.diff.removed_ink),
                    RowKind::Same | RowKind::Gap => (None, " ", c.dim),
                };
                div()
                    .flex()
                    .px(px(8.))
                    .when_some(fill, |d, f| d.bg(hsla(f)))
                    .child(
                        div()
                            .flex_none()
                            .w(px(14.))
                            .text_color(hsla(ink))
                            .child(sign),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .text_color(hsla(c.text))
                            .child(SharedString::from(line.to_string())),
                    )
            }))
            .into_any_element()
    }

    /// The open permission card: what the agent wants to run, and Allow / always / Deny.
    fn permission_card(
        &self,
        tool: &str,
        what: &str,
        detail: &str,
        always: Option<&str>,
        look: &Look,
    ) -> AnyElement {
        let c = &look.c;
        let mut shown: Vec<&str> = detail.lines().take(DETAIL_LINES).collect();
        if detail.lines().count() > DETAIL_LINES {
            shown.push("…");
        }
        let label = look.label.clone();
        div()
            .w_full()
            .px(px(12.))
            .py(px(10.))
            .rounded(px(9.))
            .border_1()
            .border_color(hsla(c.ask_edge))
            .bg(hsla(c.ask_fill))
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(
                div()
                    .text_color(hsla(c.ask_head))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(SharedString::from(format!("{label} wants to run {tool}"))),
            )
            .child(
                div()
                    .w_full()
                    .px(px(10.))
                    .py(px(8.))
                    .rounded(px(5.))
                    .bg(hsla(c.code_fill))
                    .font_family(look.mono.clone())
                    .text_color(hsla(c.code_text))
                    .child(SharedString::from(what.to_string())),
            )
            .when(!detail.trim().is_empty(), |d| {
                d.child(
                    div()
                        .font_family(look.mono.clone())
                        .text_size(px(text::META))
                        .text_color(hsla(c.ask_dim))
                        .child(SharedString::from(shown.join("\n"))),
                )
            })
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(6.))
                    .text_size(px(text::META + 0.5))
                    .child(button("agent-allow", "Allow  ⏎", true, c).on_click(on_tab(
                        &look.view,
                        |this, cx| {
                            this.answer(Answer::Allowed, cx).ok();
                        },
                    )))
                    .when_some(always.map(str::to_string), |d, always| {
                        d.child(button("agent-always", always, false, c).on_click(on_tab(
                            &look.view,
                            |this, cx| {
                                this.answer(Answer::AlwaysAllowed, cx).ok();
                            },
                        )))
                    })
                    .child(button("agent-deny", "Deny", false, c).on_click(on_tab(
                        &look.view,
                        |this, cx| {
                            this.answer(Answer::Denied(None), cx).ok();
                        },
                    ))),
            )
            .child(
                div()
                    .text_size(px(text::META))
                    .text_color(hsla(c.ask_dim))
                    .child(SharedString::from(format!(
                        "Or type in the box below to tell {label} what to do instead."
                    ))),
            )
            .into_any_element()
    }

    /// The right button: Stop while the turn runs, "Stopping…" until it ends, then Force stop.
    fn stop_label(&self) -> Option<&'static str> {
        match (self.stopping.is_some(), self.force) {
            (true, true) => Some("Force stop"),
            (true, false) => Some("Stopping…"),
            (false, _) if self.status() == Status::Working => Some("■ Stop  esc"),
            _ => None,
        }
    }

    fn top_line(&self, c: &Colors) -> AnyElement {
        let context = self
            .conversation
            .context_pct()
            .map(|p| format!("context {p}%"));
        div()
            .flex_none()
            .w_full()
            .h(px(26.))
            .px(px(16.))
            .flex()
            .items_center()
            .text_size(px(text::META))
            .text_color(hsla(c.dim))
            .child(SharedString::from(status_word(self.status())))
            .child(div().flex_1())
            .when_some(context, |d, t| d.child(SharedString::from(t)))
            .into_any_element()
    }

    /// Messages waiting for the process, under the conversation, and why they wait.
    fn pending(&self, c: &Colors) -> Option<AnyElement> {
        if self.queued.is_empty() {
            return None;
        }
        Some(
            div()
                .flex_none()
                .w_full()
                .px(px(16.))
                .pb(px(10.))
                .flex()
                .flex_col()
                .items_end()
                .gap(px(6.))
                .text_size(px(BODY))
                .children(self.queued.iter().map(|(text, _)| {
                    div()
                        .max_w(relative(0.8))
                        .px(px(12.))
                        .py(px(8.))
                        .rounded(px(12.))
                        .bg(hsla(c.card))
                        .border_1()
                        .border_color(hsla(c.card_border))
                        .text_color(hsla(c.card_dim))
                        .child(SharedString::from(text.clone()))
                }))
                .child(
                    div()
                        .text_size(px(text::META))
                        .text_color(hsla(c.dim))
                        .child(SharedString::from(format!("Starting {}…", self.label()))),
                )
                .into_any_element(),
        )
    }

    fn empty_state(&self, c: &Colors) -> AnyElement {
        let home = crate::workspace::home_dir();
        let place = crate::sidebar::home_relative(&self.start.cwd, home.as_deref());
        div()
            .flex_1()
            .w_full()
            .flex()
            .items_center()
            .justify_center()
            .text_color(hsla(c.dim))
            .child(SharedString::from(format!(
                "Ask {} to do something in {place}",
                self.label()
            )))
            .into_any_element()
    }

    fn composer_area(&self, c: &Colors, mono: SharedString, cx: &mut Context<Self>) -> AnyElement {
        let mode = self
            .provider
            .filter(|p| !p.info().modes.is_empty())
            .and(self.start.mode.clone());
        let model = self
            .conversation
            .model()
            .map(|m| model_chip(self.provider, m));
        let cost = self.conversation.cost_usd().map(cost_chip);
        let stop_label = self.stop_label();
        let at = self.at.as_ref().map(|at| {
            let rows: Vec<AnyElement> = at
                .hits
                .iter()
                .enumerate()
                .map(|(i, hit)| {
                    div()
                        .id(("agent-at", i))
                        .px(px(8.))
                        .py(px(3.))
                        .rounded(px(4.))
                        .when(i == at.selected, |d| d.bg(hsla(c.code_fill)))
                        .text_color(hsla(c.panel_text))
                        .cursor_pointer()
                        .on_click(cx.listener(move |this, _, _, cx| this.pick_at(Some(i), cx)))
                        .child(SharedString::from(hit.rel.clone()))
                        .into_any_element()
                })
                .collect();
            let note = if self.listing {
                Some("Listing files…")
            } else if at.query.is_empty() {
                Some("Type to find a file")
            } else if at.hits.is_empty() {
                Some("No file matches")
            } else {
                None
            };
            div()
                .w_full()
                .mb(px(8.))
                .p(px(4.))
                .rounded(px(7.))
                .bg(hsla(c.panel))
                .border_1()
                .border_color(hsla(c.panel_border))
                .font_family(mono.clone())
                .text_size(px(text::META + 0.5))
                .flex()
                .flex_col()
                .children(rows)
                .when_some(note, |d, n| {
                    d.child(
                        div()
                            .px(px(8.))
                            .py(px(3.))
                            .text_color(hsla(c.panel_dim))
                            .child(n),
                    )
                })
        });
        div()
            .flex_none()
            .w_full()
            .px(px(14.))
            .pt(px(10.))
            .pb(px(10.))
            .border_t_1()
            .border_color(hsla(c.card_border))
            .flex()
            .flex_col()
            .children(at)
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(6.))
                    .mb(px(8.))
                    .when_some(mode, |d, m| {
                        let fixed = self.spoken();
                        d.child(
                            div()
                                .id("agent-mode")
                                .when(!fixed, |d| {
                                    d.cursor_pointer()
                                        .on_click(cx.listener(|this, _, _, cx| this.next_mode(cx)))
                                })
                                .child(chip(m, false, c)),
                        )
                    })
                    .when_some(model, |d, m| d.child(chip(m, false, c)))
                    .when_some(cost, |d, m| d.child(chip(m, true, c))),
            )
            .child(
                div()
                    .when(self.at.is_some(), |d| d.key_context("AtPopup"))
                    .flex()
                    .items_end()
                    .gap(px(8.))
                    .font_family(mono)
                    .child(div().flex_1().min_w(px(0.)).child(self.composer.clone()))
                    .child(match stop_label {
                        Some(label) => button("agent-stop", label, false, c)
                            .text_size(px(text::META))
                            .on_click(cx.listener(|this, _, _, cx| this.stop_pressed(cx)))
                            .into_any_element(),
                        None => div()
                            .px(px(8.))
                            .py(px(3.))
                            .text_size(px(text::META))
                            .text_color(hsla(c.dim))
                            .child("⏎")
                            .into_any_element(),
                    }),
            )
            // What goes with your next message (a dropped command block, Task 11), under the
            // box until it is sent; its × takes it off.
            .when(!self.attachments.is_empty(), |d| {
                d.child(div().mt(px(6.)).flex().flex_wrap().gap(px(6.)).children(
                    self.attachments.iter().enumerate().map(|(i, a)| {
                        let (dim, text, hover) = (c.panel_dim, c.panel_text, c.panel_hover);
                        let remove = div()
                            .id(("attachment-remove", i))
                            .size(px(14.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(3.))
                            .text_color(hsla(dim))
                            .cursor_pointer()
                            .hover(move |s| s.bg(hsla(hover)).text_color(hsla(text)))
                            .child("×")
                            .tooltip(|_, cx| -> gpui::AnyView {
                                cx.new(|_| crate::sidebar_toggle::Tip("Remove".into()))
                                    .into()
                            })
                            .on_click(
                                cx.listener(move |this, _, _, cx| this.remove_attachment_at(i, cx)),
                            );
                        #[cfg(feature = "ui-shots")]
                        let remove = {
                            let removes = self.removes.clone();
                            removes.borrow_mut().resize(self.attachments.len(), None);
                            remove.child(
                                gpui::canvas(
                                    move |b, _, _| {
                                        if let Some(slot) = removes.borrow_mut().get_mut(i) {
                                            *slot = Some(b);
                                        }
                                    },
                                    |_, _, _, _| {},
                                )
                                .absolute()
                                .top_0()
                                .left_0()
                                .size_full(),
                            )
                        };
                        chip(format!("📎 {}", a.lines().next().unwrap_or("")), true, c)
                            .pr(px(4.))
                            .flex()
                            .items_center()
                            .gap(px(4.))
                            .child(remove)
                    }),
                ))
            })
            .child(
                div()
                    .mt(px(6.))
                    .flex()
                    .gap(px(12.))
                    .text_size(px(text::META))
                    .text_color(hsla(c.dim))
                    .child("/ commands")
                    .child("@ files"),
            )
            .into_any_element()
    }
}

impl Render for AgentTab {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let appearance = cx.global::<Appearance>();
        let theme = appearance.theme;
        let mono = appearance.font.family.clone();
        let c = Colors::of(&theme);
        let empty = self.conversation.items().is_empty();
        let info = self.provider.map(|p| p.info());
        let view = cx.entity().downgrade();
        let link_view = view.clone();
        let look = Rc::new(Look {
            c,
            mono: mono.clone(),
            label: self.label().to_string().into(),
            badge: info.map_or("?", |i| i.badge).into(),
            badge_rgb: info.map_or(0x6b6b6b, |i| i.badge_rgb),
            draw: md_preview::Draw {
                colours: md_preview::Colours::of(&theme),
                code_font: mono.clone(),
                on_link: Rc::new(move |url, _, cx| {
                    link_view
                        .update(cx, |this, cx| this.follow_link(&url, cx))
                        .ok();
                }),
                links: Cell::new(0),
                mermaid: Rc::new(Vec::new()),
            },
            diff: diff_row_colours(&theme, theme.background),
            view: view.clone(),
            now: Instant::now(),
        });
        #[cfg(feature = "ui-shots")]
        let built = self.rows_built.clone();
        let pending = self.pending(&c);
        let stream = if empty && self.queued.is_empty() {
            self.empty_state(&c)
        } else if empty {
            div().flex_1().into_any_element()
        } else {
            let look = look.clone();
            div()
                .flex_1()
                .min_h(px(0.))
                .w_full()
                .child(
                    list(self.list.clone(), move |ix, _, cx| {
                        #[cfg(feature = "ui-shots")]
                        built.set(built.get() + 1);
                        match view.upgrade() {
                            Some(tab) => tab.read(cx).item_row(ix, &look),
                            None => div().into_any_element(),
                        }
                    })
                    .size_full(),
                )
                .into_any_element()
        };
        // A command block dragged over the tab tints it (GPUI's element-local style: nothing
        // else changes during a drag-over); dropped, the Workspace attaches it (Task 11).
        let drop_fill = theme.ui().drop_card;
        div()
            .key_context("AgentTab")
            .on_action(cx.listener(Self::at_up))
            .on_action(cx.listener(Self::at_down))
            .size_full()
            .flex()
            .flex_col()
            .bg(hsla(c.page))
            .when(self.provider.is_some(), |d| {
                d.drag_over::<BlockDrag>(move |s, _, _, _| s.bg(hsla(drop_fill)))
                    .on_drop(cx.listener(|_, drag: &BlockDrag, _, cx| {
                        cx.emit(AgentTabEvent::BlockDropped(drag.clone()))
                    }))
            })
            .text_color(hsla(c.text))
            .child(self.top_line(&c))
            .child(stream)
            .children(pending)
            // An error tab (no such agent) has nothing to send to.
            .when(self.provider.is_some(), |d| {
                d.child(self.composer_area(&c, mono, cx))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui_tokens::contrast;

    #[test]
    fn process_end_is_reported_once() {
        let mut e = ProcessEnd::default();
        assert!(e.ended(Some(1)));
        assert_eq!(
            e.exited(Some(1), "nope".into()),
            Some((Some(1), "nope".into()))
        );
        assert_eq!(e.waited(), None);
        assert!(!e.ended(Some(1)), "a second Ended starts no timer");
    }

    #[test]
    fn a_late_exited_after_the_wait_is_not_reported_again() {
        let mut e = ProcessEnd::default();
        assert_eq!(
            e.waited(),
            None,
            "no end yet: the timer has nothing to report"
        );
        assert!(e.ended(Some(2)));
        assert_eq!(e.waited(), Some((Some(2), String::new())));
        assert_eq!(e.exited(Some(2), "late".into()), None);
        // Exited with no Ended first (killed before it started): reported at once.
        let mut never = ProcessEnd::default();
        assert_eq!(
            never.exited(None, String::new()),
            Some((None, String::new()))
        );
        assert!(!never.ended(None));
    }

    #[test]
    fn bad_resume_becomes_one_notice() {
        assert!(resume_failed(false, true));
        assert!(!resume_failed(true, true));
        assert!(!resume_failed(false, false));
        assert!(!resume_failed(true, false));
    }

    #[test]
    fn words_and_chips() {
        assert_eq!(status_word(Status::Waiting), "needs you");
        assert_eq!(status_word(Status::Working), "working");
        // The provider labels its models (`Provider::model_label`); without one, as it is.
        assert_eq!(model_chip(None, "claude-opus-5-5"), "claude-opus-5-5");
        assert_eq!(cost_chip(0.2116182), "$0.21");
        assert_eq!(elapsed_label(Duration::from_secs(4)), "4s");
        assert_eq!(elapsed_label(Duration::from_secs(65)), "1m 05s");
        assert_eq!(lines_label(1), "1 line");
        assert_eq!(lines_label(84), "84 lines");
        assert_eq!(
            answer_line(&Answer::Denied(Some("no".into()))),
            "Denied: no"
        );
        assert_eq!(answer_line(&Answer::Expired), "Expired");
    }

    #[test]
    fn the_at_query_is_one_word_after_the_at() {
        assert_eq!(at_query("see @src/ma", 4, 11), Some("src/ma"));
        assert_eq!(at_query("see @", 4, 5), Some(""));
        assert_eq!(at_query("see @src x", 4, 10), None, "a space ends it");
        assert_eq!(at_query("see src", 4, 7), None, "the @ is gone");
        assert_eq!(at_query("see @", 4, 4), None, "the caret went before it");
    }

    #[test]
    fn a_picked_file_is_relative_to_the_agent_s_folder() {
        let root = Path::new("/w");
        assert_eq!(inserted_path(root, Path::new("/w"), "src/a.rs"), "src/a.rs");
        assert_eq!(inserted_path(root, Path::new("/w/src"), "src/a.rs"), "a.rs");
        assert_eq!(
            inserted_path(root, Path::new("/w/docs"), "src/a.rs"),
            "/w/src/a.rs"
        );
    }

    #[test]
    fn an_attachment_is_removed_by_its_place() {
        let mut list = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        assert!(remove_attachment(&mut list, 1));
        assert_eq!(list, ["a", "c"]);
        // Out of range (a chip from a frame before a send): nothing changes.
        assert!(!remove_attachment(&mut list, 2));
        assert_eq!(list, ["a", "c"]);
        assert!(remove_attachment(&mut list, 0));
        assert!(remove_attachment(&mut list, 0));
        assert!(list.is_empty());
        assert!(!remove_attachment(&mut list, 0));
    }

    #[test]
    fn attachments_follow_the_text() {
        assert_eq!(wire_text("fix", &[]), "fix");
        assert_eq!(wire_text("fix", &["out".into()]), "fix\n\nout");
        assert_eq!(wire_text("", &["out".into()]), "out");
    }

    #[test]
    fn changed_rows_are_the_ones_whose_print_moved() {
        assert_eq!(changed_rows(&[1, 2, 3], &[1, 9, 3, 4]), vec![1]);
        assert_eq!(changed_rows(&[1, 2], &[1]), Vec::<usize>::new());
        let a = Item::Agent {
            message: "m".into(),
            text: "Hel".into(),
            done: false,
            parent: None,
        };
        let mut b = a.clone();
        if let Item::Agent { text, .. } = &mut b {
            text.push_str("lo");
        }
        assert_ne!(fingerprint(&a), fingerprint(&b));
        assert_eq!(fingerprint(&a), fingerprint(&a.clone()));
    }

    /// Step 3: text on the tab's surfaces meets the text floor in every theme (`text_dim` on
    /// `panel` and on `card` included).
    #[test]
    fn the_tab_s_text_meets_the_floor_in_every_theme() {
        for (name, theme) in crate::theme::every_theme() {
            let c = Colors::of(&theme);
            for (what, fg, on) in [
                ("text", c.text, c.page),
                ("dim", c.dim, c.page),
                ("dim on panel", c.panel_dim, c.panel),
                ("dim on card", c.card_dim, c.card),
                ("text on card", c.card_text, c.card),
                ("tool name", c.tool_name, c.panel),
                ("ok", c.ok, c.panel),
                ("failed", c.failed, c.page),
                ("failed on panel", c.failed_panel, c.panel),
                ("ask head", c.ask_head, c.ask_fill),
                ("ask dim", c.ask_dim, c.ask_fill),
                ("on accent", c.on_accent, c.accent),
                ("code", c.code_text, c.code_fill),
                ("code dim", c.code_dim, c.code_fill),
                ("todo active", c.todo_active, c.panel),
            ] {
                let got = contrast(fg, on);
                assert!(got >= TEXT_MIN - 0.01, "{name}: {what} {got:.2}");
            }
            // Raw text_dim, as the brief asks: on panel and on card.
            let ui = theme.ui();
            for (on, bg) in [("panel", ui.panel), ("card", ui.card)] {
                let got = contrast(ui.text_dim, bg);
                assert!(got >= TEXT_MIN - 0.01, "{name}: text_dim on {on} {got:.2}");
            }
        }
    }
}
