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
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
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
    Animation, AnimationExt, AnyElement, App, Context, Entity, EventEmitter, FocusHandle,
    Focusable, FontWeight, IntoElement, ListAlignment, ListState, MouseButton, Render,
    SharedString, Stateful, Subscription, Task, WeakEntity, Window, actions, div, list, prelude::*,
    px, relative,
};

use crate::agent_box::{
    AREA_X, AtRow, BoxMenu, Change, PLACEHOLDER, SLASH_LIMIT, SlashRow, Step, at_rows, slash_query,
    slash_rows, step_for, switched_notice,
};
#[path = "agent_rows.rs"]
mod rows;

#[path = "agent_lanes_view.rs"]
mod lanes_view;

#[path = "agent_lane_row.rs"]
mod lane_row;

#[path = "agent_tracks.rs"]
pub(crate) mod tracks;
pub(crate) use lanes_view::{BAR_FADE, BoxMode};

#[path = "agent_latest.rs"]
mod latest;

#[path = "agent_rewind.rs"]
mod rewind;

#[path = "agent_pills.rs"]
pub(crate) mod pills;

#[path = "agent_paste.rs"]
pub(crate) mod paste;

#[path = "agent_cancel.rs"]
mod cancel;

#[path = "agent_ask_view.rs"]
mod ask_view;

#[path = "agent_plan_card.rs"]
mod plan_card;

#[path = "agent_shells_view.rs"]
mod shells_view;
#[cfg(feature = "ui-shots")]
pub(crate) use ask_view::ShotAsk;
#[cfg(feature = "ui-shots")]
pub(crate) use cancel::CANCEL_LATE;
#[cfg(feature = "ui-shots")]
pub(crate) use latest::STRIP_TIP;
#[cfg(feature = "ui-shots")]
pub(crate) use rewind::STOPPED_NOTICE;
pub use rewind::{ForkSeed, Seed};
pub(crate) use rewind::{fork_tip, fork_title};

use crate::agent_conversation::{Answer, Conversation, Item};
use crate::agent_markdown::{Md, link_bare_urls, parse};
use crate::agent_process::{self, AgentProcess, ProcessEvent};
use crate::agent_tool_modal::ToolModal;
use crate::appearance::Appearance;
use crate::block_view::BlockDrag;
use crate::code_ask::{CodeRef, agent_path};
use crate::composer::{Composer, ComposerEvent};
use crate::diff_view::{RowKind, parse_diff};
use crate::file_tab::{DiffRowColours, diff_row_colours};
use crate::icons::{Icon, icon};
use crate::mark_view::{MarkClock, MarkView};
use crate::md_preview::{self, LinkTarget};
use crate::palette_items::FileHit;
use crate::providers::{
    Action, Applies, BoxCommand, ChangeKind, Event, ImageLimit, ModelChoice, Past, Picture,
    Provider, Start, TodoState, ToolDetail, Translator,
};
use crate::status::Status;
use crate::status_mark;
use crate::terminal_view::hsla;
use crate::theme::{Theme, UiColors, mix, readable};
use crate::ui_tokens::{MARK_MIN, QUIET_LINE_MIN, TEXT_MIN, ensure_contrast, text};
use crate::workspace_agent::Holder;

// ↑ / ↓ in the `@` file list and the `/` list (bound in main.rs to "AtPopup > Composer", so they
// reach the list, not the caret, only while it is open).
actions!(
    agent_tab,
    [
        AtUp,
        AtDown,
        ModalBack,
        ModalPrev,
        ModalNext,
        CloseImage,
        PrevLane,
        NextLane,
        CycleMode,
        HistoryUp,
        HistoryDown,
        // ⌥⌘P: the plan's tasks (the 2026-11-24 chores).
        TogglePlan,
        // The rewind pick (batch 3, chore 2; bound in main.rs to "RewindPick > Composer").
        PickUp,
        PickDown,
        PickRewind,
        PickFork,
        PickCopy,
        PickLeave,
        // The palette's "Rewind Last Prompt".
        RewindLastPrompt,
        // The agent's question card (batch 3, chore 8; bound in main.rs to "AgentAsk >
        // Composer": only while it has the keys and the box is empty).
        AskUp,
        AskDown,
        AskPrev,
        AskNext,
        AskSpace,
        AskEnter,
        AskToBox,
        AskSkip,
        AskPick1,
        AskPick2,
        AskPick3,
        AskPick4,
        AskPick5,
        AskPick6,
        AskPick7,
        AskPick8,
        AskPick9,
        // A busy step's tracks (the agent activity chore; bound in main.rs to "AgentTracks":
        // only while a step's group has the keyboard).
        TrackPrev,
        TrackNext,
        TrackOpen,
        TrackLeave,
        // ⌥↑ in the box: the latest busy step's tracks take the keyboard (bound in main.rs to
        // "AgentTab > Composer").
        FocusTracks,
        // The folder picker (the 2026-11-25 chores; bound in main.rs to "FolderPicker >
        // NameInput": ↑/↓ move, ⌘1…⌘8 open a Recent row).
        FolderUp,
        FolderDown,
        FolderPick1,
        FolderPick2,
        FolderPick3,
        FolderPick4,
        FolderPick5,
        FolderPick6,
        FolderPick7,
        FolderPick8
    ]
);

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
/// The provider's logo in the avatar's slot (spec B-A).
const LOGO: f32 = crate::agent_chat::chat::LOGO;
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
    /// The conversation's id: when your first message goes to a new conversation started under
    /// an id fuxx chose, when the agent reports it, and again whenever it changes (an agent's
    /// `/clear` may go on under a new id).
    Conversation(String),
    /// The model, effort and mode it now starts with (the box's choice, once taken): saved
    /// with the tab.
    Settings {
        model: Option<String>,
        effort: Option<String>,
        mode: Option<String>,
    },
    /// The folder it now runs in (the box's folder chip: chosen before the first message, or
    /// the conversation moved): saved as the tab's folder.
    Folder(PathBuf),
    /// The branch chip's click: open the branch switcher on the box's folder (the agent keeps
    /// running: a branch switch never restarts it).
    OpenBranchSwitcher(PathBuf),
    /// "New conversation" in that folder (the folder chip, after the first message): a new
    /// agent tab of the same agent there, beside this one in its pane.
    NewConversationIn(PathBuf),
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
    /// A tool call that writes files started (M11af): `call` is its tool id (unique only within
    /// this conversation); `paths` are canonical, relative ones joined to the tab's cwd.
    Edit { call: String, paths: Vec<PathBuf> },
    /// That tool call finished.
    EditDone { call: String, ok: bool },
    /// An edit finished OK and the agent had said why (live events only, M11ag): the files it
    /// wrote and its note. After the `EditDone`.
    EditNoted {
        paths: Vec<PathBuf>,
        note: crate::edit_notes::EditNote,
    },
    /// A command block dropped on the tab's body (Task 11): the Workspace reads its output and
    /// attaches it (`attach_block`), as for a drop on the tab in a bar.
    BlockDropped(BlockDrag),
    /// Fork (batch 3, chore 2): open a new tab beside this one from this seed.
    Fork(Seed),
    /// A fork's first row: its source's name clicked (show that tab).
    ShowForkSource,
    /// A right-click (or ⌃-click) in the chat (batch 3, chore 4): fuxx's menu at window point
    /// `at`, for the chat's selection and the link under the pointer.
    ContextMenu {
        at: gpui::Point<gpui::Pixels>,
        selection: Option<String>,
        link: Option<String>,
    },
    /// The box's right-click or ⇧F10: its Cut / Copy / Paste / Select All menu at `at`.
    BoxMenu {
        at: gpui::Point<gpui::Pixels>,
        has_sel: bool,
    },
    /// A web or mail link clicked in the chat (wave 3): opened where "Open links in" says.
    OpenLink(String),
    /// A file to open in a file tab: a link to it in the chat, a path pill or its sent chip.
    OpenFile(PathBuf),
    /// A changed file in the turn ledger's popover (the agent activity chore): its diff tab.
    OpenDiff(PathBuf),
    /// A background shell ended (chore 16): `words` as "Build · done in 6:40" / "Build ·
    /// failed". The Workspace tells it (the 🔔 list, a banner) unless you are looking at the tab.
    ShellEnded { words: String, failed: bool },
    /// A folder's pill or chip: shown in the Files tree.
    RevealInFiles(PathBuf),
}

/// What the tab remembers to tell why an edit was made: the turn's last text per speaker
/// (`None` is the main agent, `Some(call)` the sub-agent of that tool call), and the notes of
/// edits started and not finished, by call id.
#[derive(Default)]
pub(crate) struct Reasons {
    texts: HashMap<Option<String>, (u64, Arc<str>)>,
    next_id: u64,
    started: HashMap<String, (Vec<PathBuf>, crate::edit_notes::EditNote)>,
}

impl Reasons {
    /// The turn is over, however it ended: its texts are no one's reason. `next_id` goes on, so
    /// ids stay unique.
    fn end_turn(&mut self) {
        self.texts.clear();
    }
}

/// What an agent event tells the edit ledger: a tool call that writes files (`writes`, resolved
/// against the agent's `cwd`) starts an edit; the finish of one that did ends it. `open` holds
/// the calls started and not finished.
fn edit_events(
    event: &Event,
    cwd: &Path,
    home: Option<&Path>,
    open: &mut HashSet<String>,
    reasons: &mut Reasons,
) -> Vec<AgentTabEvent> {
    match event {
        Event::MessageDone { text, parent, .. } => {
            if let Some(reason) = crate::edit_notes::reason_text(text) {
                reasons.next_id += 1;
                reasons
                    .texts
                    .insert(parent.clone(), (reasons.next_id, reason));
            }
            Vec::new()
        }
        Event::TurnDone { .. } => {
            reasons.end_turn();
            Vec::new()
        }
        Event::ToolStarted {
            id,
            writes,
            texts,
            parent,
            ..
        } => {
            let paths: Vec<PathBuf> = writes
                .iter()
                .filter_map(|w| crate::edit_ledger::resolve_write(w, cwd, home))
                .collect();
            if paths.is_empty() {
                return Vec::new();
            }
            open.insert(id.clone());
            if let Some((reason_id, reason)) = reasons.texts.get(parent) {
                let note = crate::edit_notes::EditNote {
                    reason: reason.clone(),
                    reason_id: *reason_id,
                    texts: texts.clone(),
                };
                reasons.started.insert(id.clone(), (paths.clone(), note));
            }
            vec![AgentTabEvent::Edit {
                call: id.clone(),
                paths,
            }]
        }
        Event::ToolFinished { id, ok, .. } if open.remove(id) => {
            let mut told = vec![AgentTabEvent::EditDone {
                call: id.clone(),
                ok: *ok,
            }];
            if let Some((paths, note)) = reasons.started.remove(id)
                && *ok
            {
                told.push(AgentTabEvent::EditNoted { paths, note });
            }
            told
        }
        _ => Vec::new(),
    }
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
/// The tool modal's open motion (its panel's, its scrim's) while it is fresh; none once it has
/// settled. Never a second animation after the first: one under a new id starts again at 0 and
/// drew the modal see-through for a frame (your report, 2026-11-22: "blinking").
pub fn modal_motion(fresh: bool) -> Option<(Duration, Duration)> {
    fresh.then(|| (Duration::from_millis(180), Duration::from_millis(140)))
}

pub fn resume_failed(started: bool, resume: bool) -> bool {
    resume && !started
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
        Answer::Withdrawn => "Not answered".into(),
    }
}

/// How often the branch chip reads the branch at most (the 2026-11-25 chores).
const BRANCH_REREAD: Duration = Duration::from_millis(500);

/// Whether `event` may follow a branch switch (a tool call's end, a turn's end): the chip
/// reads the branch again (the 2026-11-25 chores: "branch on outer chatbox is not align").
pub fn rereads_branch(event: &Event) -> bool {
    matches!(event, Event::ToolFinished { .. } | Event::TurnDone { .. })
}

/// An answered plan card's line (the 2026-11-25 chores).
pub fn plan_answer_line(answer: &Answer) -> String {
    match answer {
        Answer::AlwaysAllowed => "Approved \u{b7} auto-accept edits \u{2713}".into(),
        Answer::Allowed => "Approved \u{2713}".into(),
        Answer::Denied(Some(note)) => format!("Kept planning: {note}"),
        Answer::Denied(None) => "Kept planning".into(),
        other => answer_line(other),
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

/// What an `@` pick of `row` puts in the box: a file's path (see `inserted_path`), or `@name` for
/// a sub-agent or skill, and a space. None for a heading, or a row nothing is at.
pub fn at_insertion(
    row: &AtRow,
    files: &[FileHit],
    commands: &[crate::providers::AgentCommand],
    root: &Path,
    cwd: &Path,
) -> Option<String> {
    match row {
        AtRow::Heading(_) => None,
        AtRow::File(i) => Some(format!(
            "{} ",
            inserted_path(root, cwd, &files.get(*i)?.rel)
        )),
        AtRow::Named { ix, .. } => Some(format!("@{} ", commands.get(*ix)?.name)),
    }
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

/// What goes with your next message, shown as a chip in the box until it is sent: a command
/// block's text, a file's path, or an image (with the path it is at, when it has one).
#[derive(Clone, Debug, PartialEq)]
pub enum Attachment {
    Text(String),
    File(PathBuf),
    /// Code asked about from a file tab (M11ag): sent as its path and lines, then the code.
    Code(CodeRef),
    Image {
        name: String,
        picture: Picture,
        /// Where the image is: its file, or the temp file a pasted one was written to when
        /// the agent can't take it in the message (it is told this path instead).
        path: Option<PathBuf>,
        /// A pasted one: it sits in the text as `[name]` (a pill), not as a chip.
        inline: bool,
    },
    /// Text quoted from elsewhere (batch 3: wave 3's Add to Chat), with where it came from
    /// (`from`: "Terminal api", "Claude"): sent as a Markdown block quote under a line naming
    /// its source.
    Quote {
        text: String,
        from: String,
    },
    /// A path pill (batch 3, chore 5). Attached (the Files menu's Add to Chat), it goes into
    /// the text as a pill, never a chip; sent, it is in the text already, and its message's row
    /// lists it as a chip by `label` (its name, its parent too when two names clash).
    PathPill {
        path: PathBuf,
        is_dir: bool,
        label: String,
    },
}

impl Attachment {
    /// What its chip says after the glyph (and what the conversation's row for the message
    /// lists): a file's or an image's name; a command block's whole text (rows show its first
    /// line).
    pub fn label(&self) -> String {
        match self {
            Attachment::Text(text) => text.clone(),
            Attachment::File(path) => file_name(path),
            Attachment::Code(code) => code.label(),
            Attachment::Image { name, .. } => name.clone(),
            Attachment::Quote { text, .. } => quote_label(text),
            Attachment::PathPill { label, .. } => label.clone(),
        }
    }

    /// What hovering its chip says: a code chip's full path.
    pub fn tip(&self) -> Option<String> {
        match self {
            Attachment::Code(code) => Some(code.path.display().to_string()),
            Attachment::Quote { from, .. } => Some(format!("Quoted from {from}")),
            Attachment::PathPill { path, .. } => Some(path.display().to_string()),
            _ => None,
        }
    }

    /// The glyph before its chip's words.
    pub fn glyph(&self) -> &'static str {
        match self {
            Attachment::Image { .. } => "\u{1f5bc}",
            Attachment::Code(_) => "</>",
            Attachment::Quote { .. } => "\u{275d}",
            _ => "\u{1f4ce}",
        }
    }
}

/// A quote chip's words: the quote's first line with words, at most 40 characters (then "…").
fn quote_label(text: &str) -> String {
    let line = text
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("");
    if line.chars().count() <= 40 {
        line.to_string()
    } else {
        line.chars().take(39).collect::<String>() + "…"
    }
}

/// A quote as sent: a line naming where it came from, then the text as a Markdown block quote
/// (a blank line kept as a bare `>`).
fn quote_wire(text: &str, from: &str) -> String {
    let quoted: Vec<String> = text
        .lines()
        .map(|l| {
            if l.trim().is_empty() {
                ">".to_string()
            } else {
                format!("> {l}")
            }
        })
        .collect();
    format!("From {from}:\n{}", quoted.join("\n"))
}

/// The last part of `path` (the whole of it when it has none).
fn file_name(path: &Path) -> String {
    path.file_name()
        .unwrap_or(path.as_os_str())
        .to_string_lossy()
        .into_owned()
}

/// The type an image file's extension says (png, jpeg/jpg, gif, webp); None for any other file.
pub fn media_type_for(path: &Path) -> Option<&'static str> {
    match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        _ => None,
    }
}

/// A pasted image's name: "Image #1", "Image #2" … in the order pasted in this tab (your words,
/// 2026-11-22: "can we make the name like "Image #1", Image #2, and so on?").
pub fn pasted_name(n: usize) -> String {
    format!("Image #{n}")
}

/// The file extension for an image of `media_type` (a pasted one written for an agent that is
/// told a path).
pub fn ext_for(media_type: &str) -> &'static str {
    match media_type {
        "image/jpeg" => "jpg",
        "image/gif" => "gif",
        "image/webp" => "webp",
        _ => "png",
    }
}

/// GPUI's format for an image of `media_type` (the viewer draws its bytes).
pub fn image_format(media_type: &str) -> gpui::ImageFormat {
    match media_type {
        "image/jpeg" => gpui::ImageFormat::Jpeg,
        "image/gif" => gpui::ImageFormat::Gif,
        "image/webp" => gpui::ImageFormat::Webp,
        _ => gpui::ImageFormat::Png,
    }
}

/// `bytes` as a picture to send, or the notice saying it is too big for the agent called
/// `label` (checked before it is encoded).
pub fn picture(
    name: &str,
    media_type: &'static str,
    bytes: Vec<u8>,
    limit: ImageLimit,
    label: &str,
) -> Result<Picture, String> {
    if !limit.fits(bytes.len()) {
        return Err(limit.refusal(label, name));
    }
    Ok(Picture::new(media_type, bytes))
}

/// An image file's bytes, read no further than `limit` allows (a device, a pipe or a growing
/// file can't be read without end): more than that is the same notice as a big picture. Reads
/// a file: call it off the UI thread.
pub fn read_image(
    path: &Path,
    name: &str,
    limit: ImageLimit,
    label: &str,
) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .and_then(|f| f.take(limit.raw_max() as u64 + 1).read_to_end(&mut bytes))
        .map_err(|e| format!("Couldn't read {name}: {e}"))?;
    if !limit.fits(bytes.len()) {
        return Err(limit.refusal(label, name));
    }
    Ok(bytes)
}

/// Remove the files in `dir` (pasted images written for an agent told a path) last changed
/// more than `age` before `now`; how many went. A folder that isn't there has none. Reads and
/// removes files: call it off the UI thread.
pub fn clean_pasted(dir: &Path, age: Duration, now: std::time::SystemTime) -> usize {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    entries
        .flatten()
        .filter(|e| {
            e.metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| now.duration_since(t).ok())
                .is_some_and(|old| old > age)
        })
        .filter(|e| std::fs::remove_file(e.path()).is_ok())
        .count()
}

/// At start: pasted images older than a day are removed from the temp folder.
pub fn clean_old_pasted() {
    let day = Duration::from_secs(24 * 60 * 60);
    clean_pasted(
        &std::env::temp_dir().join(PASTED_DIR),
        day,
        std::time::SystemTime::now(),
    );
}

/// Write a pasted `picture` to a new file in `dir` (made if needed), for an agent that is
/// told a path: `<dir>/<uuid>.<ext>`. Reads and writes a file: call it off the UI thread.
pub fn write_pasted(dir: &Path, picture: &Picture, ext: &str) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let id = crate::providers::new_uuid().ok_or_else(|| std::io::Error::other("no random id"))?;
    let path = dir.join(format!("{id}.{ext}"));
    std::fs::write(&path, &picture.bytes)?;
    Ok(path)
}

/// Take attachment `i` off the list waiting for your next message (its chip's ×); false when
/// there is none at `i` (nothing changes).
pub fn remove_attachment<T>(list: &mut Vec<T>, i: usize) -> bool {
    if i >= list.len() {
        return false;
    }
    list.remove(i);
    true
}

/// The folder in the temp folder that pasted images are written to (for an agent told a path).
const PASTED_DIR: &str = "fuxx-images";

/// Where an image chip's bytes come from.
enum ImageFrom {
    File(PathBuf),
    Clipboard(Vec<u8>),
    /// A TIFF or BMP image from the clipboard, made PNG on the way in.
    ClipboardToPng(Vec<u8>),
}

/// What is sent as text for a message with attachments: the text, then each attachment (a
/// command block's text, a file's path, an image's path). With `images_inline` the agent takes
/// images in the message itself, so they are not in the text. A code chip says its path from
/// the agent's folder `cwd` (else whole), its lines, and the code.
pub fn wire_text(
    text: &str,
    attachments: &[Attachment],
    images_inline: bool,
    cwd: &Path,
) -> String {
    let mut out = text.to_string();
    for a in attachments {
        let said = match a {
            Attachment::Text(text) => text.clone(),
            Attachment::File(path) => path.to_string_lossy().into_owned(),
            Attachment::Code(code) => code.text(&agent_path(&code.path, code.real.as_deref(), cwd)),
            Attachment::Quote { text, from } => quote_wire(text, from),
            Attachment::Image {
                path, inline, name, ..
            } if !images_inline => match path {
                // A pasted one: its marker, so "[Image #1]" in the text finds its file.
                Some(path) if *inline => format!("{}: {}", marker(name), path.to_string_lossy()),
                Some(path) => path.to_string_lossy().into_owned(),
                None => continue,
            },
            Attachment::Image { .. } => continue,
            // In the text already, as the box showed it.
            Attachment::PathPill { .. } => continue,
        };
        if !out.is_empty() {
            out.push_str("\n\n");
        }
        out.push_str(&said);
    }
    out
}

/// Whether `new` may join the waiting `list`: the same code (file and lines) twice is one chip.
pub(crate) fn attachable(list: &[Attachment], new: &Attachment) -> bool {
    let Attachment::Code(code) = new else {
        return true;
    };
    !list
        .iter()
        .any(|a| matches!(a, Attachment::Code(c) if c.same_place(code)))
}

/// What the conversation's row for a message lists of its attachments.
fn labels(attachments: &[Attachment]) -> Vec<String> {
    // A pasted image is a pill in the text, not a chip under it.
    attachments
        .iter()
        .filter(|a| !matches!(a, Attachment::Image { inline: true, .. }))
        .map(Attachment::label)
        .collect()
}

/// The images among `attachments`, for an agent that takes them in the message.
fn pictures(attachments: &[Attachment]) -> Vec<Picture> {
    attachments
        .iter()
        .filter_map(|a| match a {
            Attachment::Image {
                picture,
                inline,
                name,
                ..
            } => Some(Picture {
                label: inline.then(|| Arc::from(marker(name))),
                ..picture.clone()
            }),
            _ => None,
        })
        .collect()
}

/// Where `text` names the pasted images in `names` (`[Image #1]` …), in order, with each name.
pub fn marker_ranges<'a>(
    text: &str,
    names: impl IntoIterator<Item = &'a String>,
) -> Vec<(std::ops::Range<usize>, String)> {
    let mut found: Vec<(std::ops::Range<usize>, String)> = names
        .into_iter()
        .flat_map(|name| {
            let mark = marker(name);
            text.match_indices(mark.as_str())
                .map(|(i, m)| (i..i + m.len(), name.clone()))
                .collect::<Vec<_>>()
        })
        .collect();
    found.sort_by_key(|(r, _)| r.start);
    found
}

/// How a pasted image sits in the text: `[Image #1]` (drawn as a pill).
pub fn marker(name: &str) -> String {
    format!("[{name}]")
}

/// The attachments a message really takes: a pasted image whose pill was deleted from `text`
/// goes (you took it out).
pub fn kept(text: &str, attachments: Vec<Attachment>) -> Vec<Attachment> {
    attachments
        .into_iter()
        .filter(|a| match a {
            Attachment::Image {
                inline: true, name, ..
            } => text.contains(&marker(name)),
            _ => true,
        })
        .collect()
}

// How many items `fingerprint` hashed on this thread (the perf chore's work-count tests).
#[cfg(test)]
thread_local! {
    pub static FINGERPRINTS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
/// What an item looks like, as a number (its text hashed whole: never on the hot path since
/// the perf chore, where rows go by `Conversation::version`).
pub(crate) fn fingerprint(item: &Item) -> u64 {
    FINGERPRINTS.with(|n| n.set(n.get() + 1));
    let mut h = DefaultHasher::new();
    std::mem::discriminant(item).hash(&mut h);
    match item {
        Item::You {
            text,
            attachments,
            taken,
            to,
            ..
        } => (text, attachments, taken, to).hash(&mut h),
        Item::Resumed { ended, said, .. } => (format!("{ended:?}"), said).hash(&mut h),
        Item::Agent { text, done, .. } => (text, done).hash(&mut h),
        Item::Tool {
            running,
            ok,
            result,
            output,
            dropped_lines,
            activity,
            ..
        } => (
            running,
            ok,
            result,
            output.as_ref().map(String::len),
            dropped_lines,
            activity,
        )
            .hash(&mut h),
        Item::Todo(list) => {
            for t in list {
                (&t.text, t.state as u8).hash(&mut h);
            }
        }
        Item::Permission { answer, .. } => format!("{answer:?}").hash(&mut h),
        Item::Question { reply, .. } => format!("{reply:?}").hash(&mut h),
        Item::Notice(t) | Item::Error(t) => t.hash(&mut h),
    }
    h.finish()
}

/// The command line with the provider's suggestion flags (`Provider::suggestion_flags`) added
/// while "Prompt suggestions" is `on`; a command line that names the flag already (a preset's)
/// is left as it is: the preset wins.
pub fn with_suggestions(mut argv: Vec<String>, flags: &[String], on: bool) -> Vec<String> {
    let Some(flag) = flags.first().filter(|_| on) else {
        return argv;
    };
    let named = argv.iter().any(|a| {
        a == flag
            || a.strip_prefix(flag.as_str())
                .is_some_and(|r| r.starts_with('='))
    });
    if !named {
        argv.extend(flags.iter().cloned());
    }
    argv
}

/// Whether a suggestion arriving now is kept: only while Main is idle (no turn running, no
/// permission waiting) with nothing queued to send; otherwise it is stale.
pub fn keeps_suggestion(status: Status, queued: bool) -> bool {
    !matches!(status, Status::Working | Status::Waiting) && !queued
}

/// The faint guess the empty box shows (`Composer::set_ghost`): the suggestion, only in Main's
/// view while it is idle and no permission waits.
pub fn ghost_for(
    suggestion: Option<&str>,
    main_view: bool,
    permission: bool,
    status: Status,
) -> Option<String> {
    suggestion
        .filter(|_| main_view && !permission && keeps_suggestion(status, false))
        .map(str::to_string)
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
    pub failed_panel: Rgb,
    /// An error row's text.
    pub failed: Rgb,
    /// The permission card: its edge, fill, heading and quiet text.
    pub ask_edge: Rgb,
    pub ask_fill: Rgb,
    pub ask_head: Rgb,
    pub ask_dim: Rgb,
    /// The question card (batch 3, chore 8): the light on its lit row (and the current step
    /// chip), its words, its filled digit mark and the digit on it, an unfilled mark's ring,
    /// and the answered line's check.
    pub ask_cursor: Rgb,
    pub ask_text: Rgb,
    pub ask_mark: Rgb,
    pub ask_mark_ink: Rgb,
    pub ask_key_edge: Rgb,
    pub ask_done: Rgb,
    /// The design review: the "Recommended" tag (text, so it meets the text floor), the
    /// preview well's fill on the card and its code.
    pub ask_tag: Rgb,
    pub ask_well: Rgb,
    pub ask_well_text: Rgb,
    pub accent: Rgb,
    pub on_accent: Rgb,
    pub todo_active: Rgb,
    /// Code (what a tool runs, its output) on the page.
    pub code_fill: Rgb,
    pub code_text: Rgb,
    /// The agent box you type in (M11ak; the folder tab's inner box since the chores).
    pub box_fill: Rgb,
    /// The folder-tab composer (chores: as the shell tab's): its outer box and edge, and a pill
    /// under the pointer (the pills have no fill at rest; `page` is behind it: the tab's
    /// concave corner).
    pub outer: Rgb,
    pub outer_edge: Rgb,
    pub pill_hover: Rgb,
    /// The box's controls under the pointer (`+`, the model and mode pills; no fill at rest
    /// since 2026-11-22): a step over the box (10 %).
    pub control_hover: Rgb,
    /// Stop's circle while the agent works (your words, 2026-11-22: "make the background
    /// redish"): the theme's red over the outer ring, as strong as keeps its square showing; a
    /// step more under the pointer.
    pub stop_fill: Rgb,
    pub stop_hover: Rgb,
    /// The send circle, empty: a step over the outer ring it sits in (the lead designer's review,
    /// 2026-11-22: as a control it matched the ring in Latte and vanished).
    pub send_empty: Rgb,
    /// The send circle under the pointer, ready (the designer's details): the accent a step
    /// lighter (dark) or darker. (Empty, it is a control: `control`, its arrow `dim`.)
    pub send_hover: Rgb,
    /// The send circle, ready (your reference, 2026-11-22): the accent, its arrow on it — the
    /// one strong thing in the box.
    pub send_fill: Rgb,
    pub send_ink: Rgb,
    /// The inner box's lit edge (`UiColors::composer_glow_*`).
    pub glow_top: Rgb,
    pub glow_bottom: Rgb,
    /// The page is dark (the box's shadow is stronger there).
    pub page_is_dark: bool,
    /// A tool row under the pointer (spec A §A): the panel a step towards the foreground.
    pub row_hover: Rgb,
    /// The context meter (spec A §C): its track, and its fill under 70 %, to 89 % and from 90 %;
    /// the percent's colour in the last two.
    pub meter_track: Rgb,
    pub meter_ok: Rgb,
    pub meter_caution: Rgb,
    pub meter_critical: Rgb,
    pub pct_caution: Rgb,
    pub pct_critical: Rgb,
    /// The tool modal's scrim (spec A §B): the theme's shadow, at this strength.
    pub scrim: Rgb,
    pub scrim_alpha: f32,
    /// The chat (spec B-B). Your bubble: the box you typed it in (its inner fill and edge),
    /// its text, its quiet text and an attachment's chip.
    pub you_fill: Rgb,
    pub you_edge: Rgb,
    pub you_text: Rgb,
    pub you_dim: Rgb,
    pub chip_fill: Rgb,
    /// A queued message: the box's outer fill, quieter text.
    pub queued_fill: Rgb,
    pub queued_text: Rgb,
    /// A tool row's ok dot, the line between a group's rows, the todo meter's track.
    pub tool_ok: Rgb,
    pub tool_divider: Rgb,
    pub todo_track: Rgb,
    /// An error card: its fill, edge, octagon and text.
    pub error_fill: Rgb,
    pub error_edge: Rgb,
    pub error_mark: Rgb,
    pub error_text: Rgb,
    /// The latest-prompt strip (chore after M11ar): your message's words a step dimmer than in
    /// its bubble, and its leading mark, on the panel (and its hover).
    pub strip_text: Rgb,
    pub strip_mark: Rgb,
    /// Your message's hover actions under the pointer (batch 3, chore 2), and the rewind pick's
    /// ring.
    pub card_hover: Rgb,
    pub focus_ring: Rgb,
    /// The lanes chore (2026-11-24): the closing bar's words (name and verb), its quiet words,
    /// a failed lane's words and a done lane's check, on the box and a control under the
    /// pointer; the relay target pill's fill on the tab, its words and its quiet words.
    pub bar_text: Rgb,
    pub bar_dim: Rgb,
    pub bar_failed: Rgb,
    pub bar_ok: Rgb,
    /// "Ask Main about it"'s edge on the box (the lanes review, fix 5: `outer_edge` vanished
    /// in Mocha).
    pub bar_edge: Rgb,
    pub relay_fill: Rgb,
    pub relay_text: Rgb,
    pub relay_dim: Rgb,
    /// The subagent row chore (2026-11-24): a live subagent's robot (its row, the strip's
    /// group, its lane's header): of the theme's blue, magenta and cyan the hue farthest from
    /// the accent and the status colours (`agent_lanes::lane_hue`), else `panel_dim`; held to
    /// `MARK_MIN` on the rows' fills and the page.
    pub lane_ink: Rgb,
    /// The agent activity chore: an edit's ink in the tracks (its tile and band; orange, the
    /// 2026-11-24 chores), a read or search's (blue), and the keyboard cursor's ring on a column.
    pub bead_edit: Rgb,
    pub track_look: Rgb,
    pub track_cursor: Rgb,
    /// The ledger's "tests failing" in the folder tab, on the tab and on a pill under the
    /// pointer.
    pub ledger_failing: Rgb,
}

const BLACK: Rgb = Rgb { r: 0, g: 0, b: 0 };

/// The top bar's height (spec A §C); the tool modal's scrim starts under it.
const TOP_BAR: f32 = 32.0;

/// The status marks' size in the tab (the top bar's pill, the tool modal's header).
const MARK: f32 = 12.0;

impl Colors {
    pub fn of(theme: &Theme) -> Colors {
        let ui: UiColors = theme.ui();
        let page = theme.background;
        let fit = |c: Rgb, on: Rgb| ensure_contrast(c, on, TEXT_MIN);
        let ask_fill = mix(page, ui.ring_waiting, 0.07);
        // The question card's light (batch 3, chore 8; the lead designer): neutral, not the
        // waiting red, so it never reads as an alarm.
        // The design review: a touch warmer in a light theme (towards the waiting colour,
        // the floors kept by `readable` and the words' own).
        let lit = if theme.is_dark() {
            mix(ask_fill, theme.foreground, 0.07)
        } else {
            mix(
                mix(ask_fill, theme.foreground, 0.055),
                ui.ring_waiting,
                0.035,
            )
        };
        let ask_cursor = readable(lit, &[ask_fill], crate::ui_tokens::SOFT_LINE_MIN);
        let ask_well = if theme.is_dark() {
            mix(ask_fill, BLACK, 0.25)
        } else {
            mix(
                ask_fill,
                Rgb {
                    r: 255,
                    g: 255,
                    b: 255,
                },
                0.5,
            )
        };
        let ask_mark = readable(ui.accent, &[ask_fill, ask_cursor], MARK_MIN);
        let code_fill = mix(page, ui.text, 0.06);
        let row_hover = mix(ui.panel, theme.foreground, 0.04);
        let meter_track = readable(mix(page, theme.foreground, 0.12), &[page], QUIET_LINE_MIN);
        let on_meter = |c: Rgb| readable(c, &[page, meter_track], MARK_MIN);
        let you_fill = ui.composer_inner;
        let chip_fill = mix(you_fill, theme.foreground, 0.06);
        let error_fill = mix(page, ui.block_failed, 0.08);
        let control_hover = mix(ui.composer_inner, theme.foreground, 0.10);
        let on_box = [ui.composer_inner, control_hover];
        let relay_fill = readable(
            mix(
                ui.composer_outer,
                ui.accent,
                if theme.is_dark() { 0.12 } else { 0.08 },
            ),
            &[ui.composer_outer],
            crate::ui_tokens::SOFT_LINE_MIN,
        );
        let panel_dim = readable(ui.text_dim, &[ui.panel, row_hover], TEXT_MIN);
        let p = theme.palette;
        let lane_ink = readable(
            crate::agent_lanes::lane_hue(
                &[p[4], p[5], p[6]],
                &[ui.accent, ui.ring_working, ui.ring_waiting, ui.ring_done],
            )
            .unwrap_or(panel_dim),
            &[ui.panel, row_hover, ask_fill, ask_cursor, page],
            MARK_MIN,
        );
        Colors {
            page,
            text: fit(ui.text, page),
            dim: fit(ui.text_dim, page),
            panel: ui.panel,
            panel_border: ui.panel_border,
            panel_hover: ui.panel_hover,
            panel_text: readable(ui.text, &[ui.panel, row_hover], TEXT_MIN),
            panel_dim,
            tool_name: readable(ui.accent, &[ui.panel, row_hover], TEXT_MIN),
            failed_panel: readable(ui.block_failed, &[ui.panel, row_hover], TEXT_MIN),
            failed: fit(ui.block_failed, page),
            ask_edge: ui.ring_waiting,
            ask_fill,
            ask_head: readable(ui.ring_waiting, &[ask_fill, ask_cursor], TEXT_MIN),
            ask_dim: readable(ui.text_dim, &[ask_fill, ask_cursor], TEXT_MIN),
            ask_cursor,
            ask_text: readable(ui.text, &[ask_fill, ask_cursor], TEXT_MIN),
            ask_mark,
            ask_mark_ink: fit(ui.on_accent, ask_mark),
            ask_key_edge: readable(
                mix(ask_fill, theme.foreground, 0.22),
                &[ask_fill, ask_cursor],
                QUIET_LINE_MIN,
            ),
            ask_done: readable(ui.ring_done, &[page, ui.card_hover], MARK_MIN),
            ask_tag: readable(ui.accent, &[ask_fill, ask_cursor], TEXT_MIN),
            ask_well,
            ask_well_text: readable(ui.text, &[ask_well], TEXT_MIN),
            accent: ui.accent,
            on_accent: fit(ui.on_accent, ui.accent),
            todo_active: fit(ui.ring_working, ui.panel),
            code_fill,
            code_text: fit(ui.text, code_fill),
            // The shell composer's palette (chores: "more likely in terminal tab"): the box you
            // type in is its inner box, raised over its outer box.
            box_fill: ui.composer_inner,
            outer: ui.composer_outer,
            outer_edge: ui.composer_edge,
            pill_hover: ui.composer_pill_hover,
            control_hover,
            stop_fill: {
                let red = theme.palette[1];
                let dim = ui.text_dim;
                (0..=10)
                    .map(|k| mix(ui.composer_outer, red, 0.32 - 0.02 * k as f32))
                    .find(|&fill| {
                        crate::ui_tokens::contrast(dim, fill) >= crate::ui_tokens::MARK_MIN
                            && crate::ui_tokens::contrast(ui.text, fill)
                                >= crate::ui_tokens::MARK_MIN
                    })
                    .unwrap_or_else(|| mix(ui.composer_outer, red, 0.12))
            },
            stop_hover: mix(
                mix(ui.composer_outer, theme.palette[1], 0.32),
                theme.palette[1],
                0.10,
            ),
            send_empty: crate::theme::readable(
                mix(ui.composer_outer, theme.foreground, 0.08),
                &[ui.composer_outer],
                crate::ui_tokens::SOFT_LINE_MIN,
            ),
            // Your reference (2026-11-22): the accent's circle, its arrow on it.
            send_hover: mix(
                ui.accent,
                if theme.is_dark() {
                    theme.foreground
                } else {
                    page
                },
                0.12,
            ),
            send_fill: ui.accent,
            send_ink: fit(ui.on_accent, ui.accent),
            glow_top: ui.composer_glow_top,
            glow_bottom: ui.composer_glow_bottom,
            page_is_dark: theme.is_dark(),
            row_hover,
            meter_track,
            meter_ok: on_meter(ui.accent),
            meter_caution: on_meter(theme.palette[3]),
            meter_critical: on_meter(theme.palette[1]),
            pct_caution: fit(theme.palette[3], page),
            pct_critical: fit(theme.palette[1], page),
            scrim: ui.shadow,
            scrim_alpha: if theme.is_dark() { 0.35 } else { 0.25 },
            you_fill,
            you_edge: ui.composer_edge,
            you_text: fit(ui.text, you_fill),
            you_dim: readable(ui.text_dim, &[you_fill, chip_fill], TEXT_MIN),
            chip_fill,
            queued_fill: ui.composer_outer,
            queued_text: fit(ui.text_dim, ui.composer_outer),
            tool_ok: readable(ui.ring_done, &[ui.panel, row_hover], MARK_MIN),
            tool_divider: readable(
                mix(ui.panel, theme.foreground, 0.07),
                &[ui.panel],
                crate::ui_tokens::SOFT_LINE_MIN,
            ),
            todo_track: readable(
                mix(ui.panel, theme.foreground, 0.10),
                &[ui.panel],
                crate::ui_tokens::SOFT_LINE_MIN,
            ),
            error_fill,
            error_edge: readable(
                mix(page, ui.block_failed, 0.30),
                &[page, error_fill],
                QUIET_LINE_MIN,
            ),
            error_mark: readable(ui.block_failed, &[error_fill], MARK_MIN),
            error_text: fit(ui.text, error_fill),
            strip_text: readable(
                mix(ui.text, ui.text_dim, 0.4),
                &[ui.panel, ui.panel_hover],
                TEXT_MIN,
            ),
            strip_mark: readable(ui.text_dim, &[ui.panel, ui.panel_hover], MARK_MIN),
            card_hover: ui.card_hover,
            focus_ring: ui.focus_ring,
            bar_text: readable(ui.text, &on_box, TEXT_MIN),
            bar_dim: readable(ui.text_dim, &on_box, TEXT_MIN),
            bar_failed: readable(ui.block_failed, &on_box, TEXT_MIN),
            bar_ok: readable(ui.ring_done, &[ui.composer_inner], MARK_MIN),
            bar_edge: readable(
                mix(ui.composer_inner, theme.foreground, 0.20),
                &[ui.composer_inner],
                QUIET_LINE_MIN,
            ),
            relay_fill,
            relay_text: readable(ui.text, &[relay_fill], TEXT_MIN),
            relay_dim: readable(ui.text_dim, &[relay_fill], TEXT_MIN),
            lane_ink,
            bead_edit: readable(
                ui.tab_colors[crate::tab_color::TabColor::Orange as usize],
                &[ui.panel, row_hover],
                MARK_MIN,
            ),
            track_look: readable(
                ui.tab_colors[crate::tab_color::TabColor::Blue as usize],
                &[ui.panel, row_hover],
                MARK_MIN,
            ),
            track_cursor: readable(ui.accent, &[ui.panel, row_hover], MARK_MIN),
            ledger_failing: readable(
                ui.block_failed,
                &[ui.composer_outer, ui.composer_pill_hover],
                TEXT_MIN,
            ),
        }
    }
}

/// One item's row as laid out last: its fingerprint, and (an agent message) its blocks, parsed
/// again only when its text changes.
struct Row {
    print: u64,
    blocks: Option<Rc<Vec<Md>>>,
    /// A tool row's role in its step (the agent activity chore), a busy step's tracks on its
    /// first row, and a readable tool row's words: built when the row's print changes, only read
    /// when drawn.
    role: Option<crate::agent_steps::Role>,
    tracks: Option<Rc<crate::agent_steps::Tracks>>,
    words: Option<Rc<crate::agent_step_words::RowWords>>,
}

/// One view's list (M11ar: Main and each lane keep their own, so each keeps its scroll): its
/// rows and the item each row draws.
struct Pane {
    list: ListState,
    rows: Vec<Row>,
    /// The items its rows draw (not counting `head` and `tail`), each row's role and print,
    /// and whether the thinking line shows: derived from the conversation in `sync_rows`, only
    /// read when drawn (the perf chore).
    cache: crate::agent_view_cache::ViewCache,
    /// A lane's view (M11ar): its task as the first row ("from Main"), and how it ended as the
    /// last.
    head: bool,
    tail: bool,
    /// Its chat text's selection (chore after M11ar): each view keeps its own.
    select: Rc<crate::chat_select::ChatSelect>,
    /// The message its latest-prompt strip says (chore after M11ar): Main's your latest, a
    /// lane's its task; its row, and its one line, built once per message.
    latest: Option<latest::Latest>,
}

impl Pane {
    fn new() -> Self {
        let list = ListState::new(0, ListAlignment::Top, px(200.));
        list.set_follow_mode(gpui::FollowMode::Tail);
        Pane {
            list,
            rows: Vec::new(),
            cache: Default::default(),
            select: Rc::default(),
            head: false,
            tail: false,
            latest: None,
        }
    }
}

/// This run of the process: what it was asked to resume (until the agent says it started it),
/// the id fuxx chose for it (a new conversation: `Provider::fresh_conversation_id`), whether it
/// said it started, where its rows begin, and what was sent during it (sent again if the resume
/// fails).
#[derive(Default)]
struct Run {
    resume: Option<String>,
    fresh: Option<String>,
    /// It starts a branch of another conversation (batch 3, chore 2: `Start::branch`).
    branched: bool,
    started: bool,
    first_item: usize,
    sends: Vec<(String, Vec<Attachment>)>,
}

/// The `@` file list: where the `@` is, the query after it, the ranked hits, the selected one.
pub(crate) struct AtList {
    at: usize,
    pub(crate) query: String,
    pub(crate) hits: Vec<FileHit>,
    pub(crate) selected: usize,
}

/// The `/` list: the first word's text after the `/`, and the selected command (counted among
/// the rows that are commands, not the headings).
pub(crate) struct SlashList {
    pub(crate) query: String,
    pub(crate) selected: usize,
}

/// The agent's folder's files for `@`: listed once per tab (off the UI thread), then kept.
struct Files {
    root: PathBuf,
    paths: Arc<Vec<String>>,
}

/// Where the keyboard goes at the next draw (the tool modal opened or closed).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Refocus {
    Modal,
    Box,
    /// A step's tracks (the agent activity chore).
    Tracks,
}

/// What the emitted events last said.
#[derive(Default)]
struct Said {
    status: Option<Status>,
    question: Option<String>,
    title: bool,
    conversation: Option<String>,
}

/// Live changes asked of the running agent and not confirmed yet: the start flags each becomes
/// once the agent says it took it (`Event::Applied`).
#[derive(Default)]
struct Asked {
    model: Option<String>,
    effort: Option<String>,
    mode: Option<String>,
}

/// What the agent starts with, as saved with the tab: its flags and its folder.
struct Settings {
    model: Option<String>,
    effort: Option<String>,
    mode: Option<String>,
    cwd: PathBuf,
}

pub struct AgentTab {
    pub(crate) provider: Option<&'static dyn Provider>,
    provider_name: String,
    pub(crate) start: Start,
    pub(crate) composer: Entity<Composer>,
    pub(crate) conversation: Conversation,
    /// The login shell's `PATH`, read once the tab opens.
    path: Option<String>,
    translator: Option<Box<dyn Translator>>,
    process: Option<Arc<AgentProcess>>,
    /// Every process this tab started that isn't reaped yet (an ended one dies within
    /// `TERM_GRACE`): its own, never a holder of its conversation (R13).
    started: Vec<Arc<AgentProcess>>,
    /// Reads the process's events; dropped to stop listening (a replaced or ended process).
    pump: Option<Task<()>>,
    /// The subagent transcripts being followed, by lane (M11ar; dropped with the process).
    followers: HashMap<String, Task<()>>,
    /// Each live lane's mark (the Working arc, or Waiting), and Main's pill's.
    lane_marks: HashMap<String, Entity<MarkView>>,
    /// The pill strip's labels, kept until a lane starts or goes (the perf chore).
    pill_labels: crate::agent_lanes::PillLabels,
    main_mark: Option<Entity<MarkView>>,
    /// When each lane was first seen and when it ended (its elapsed time stops there); since
    /// the lanes chore, its current run's (a resume starts it again).
    lane_times: HashMap<String, (Instant, Option<Instant>)>,
    /// Each lane's finished runs' times, oldest first (a resume's divider says them).
    lane_runs: HashMap<String, Vec<Option<Duration>>>,
    /// When each lane's pill last flashed (a resume, a relay delivered, a draft retargeted).
    lane_flash: HashMap<String, Instant>,
    /// The closing bar opened into the box ("Ask Main about it", or typing), for this lane.
    pub(crate) bar_open: Option<String>,
    /// What the box's draft is for, set when it stops being empty in a lane.
    draft: Option<lanes_view::Draft>,
    /// When the draft's lane finished under it: an Enter this soon after only flashes.
    retargeted_at: Option<Instant>,
    /// How many times the box changed between the bar and the box (its cross-fade's key), and
    /// when last.
    pub(crate) bar_changes: u64,
    pub(crate) bar_changed_at: Option<Instant>,
    /// The box was the closing bar at the last look (`note_bar`).
    bar_was: bool,
    /// The "+N" list's running lanes when it opened (it regroups only on the next open).
    lanes_menu_running: HashSet<String>,
    /// The "+N" list of the pills that don't fit, open.
    lanes_menu: bool,
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
    /// Messages held for the process, and (the lanes chore) the subagent each is for.
    queued: Vec<(String, Vec<Attachment>, Option<lanes_view::Target>)>,
    run: Run,
    /// The agent's guess at your next message (`Event::Suggestion`), drawn faint in the empty
    /// box while Main is idle (`ghost_for`); Tab takes it. Never saved.
    pub(crate) suggestion: Option<String>,
    /// ↑/↓ browsing the prompt history from the box (`prompt_history::Recall`); dropped once the
    /// box holds anything but the prompt it put there.
    recall: Option<crate::prompt_history::Recall>,
    /// The id fuxx chose for a new conversation, taken as the tab's conversation at your first
    /// message, while the agent hasn't said it started under it: a restart then starts a new
    /// conversation again (under a new id: an agent may refuse an id used before) instead of
    /// resuming one that may never have been made.
    unconfirmed: Option<String>,
    /// Scenes: no process is ever started (nor a probe, nor a file read for the picker).
    pub(crate) no_process: bool,
    /// This tab's own process is to answer the catalog (`Event::Catalog`): the cache says
    /// "loading" until it does, and failed if the process ends first.
    catalog_pending: bool,
    /// Each view's list; `view` is the one shown (M11ar).
    panes: HashMap<crate::agent_lanes::View, Pane>,
    pub(crate) view: crate::agent_lanes::View,
    /// The tool detail modal, open (spec A §B); how many times one opened (its animation's key);
    /// its focus (key context `AgentToolModal`) and body's scroll; the keyboard to move at the
    /// next draw (a click or an action has no window: `render` does it).
    modal: Option<ToolModal>,
    modals_opened: u64,
    /// How many times the conversation was cleared (`/clear`), and when last: keys the emptied
    /// chat's fade, which plays only while the clear is new.
    cleared: u64,
    cleared_at: Option<Instant>,
    /// When the modal last opened: its open animation plays only then.
    modal_opened_at: Option<Instant>,
    modal_focus: FocusHandle,
    /// Chat selection (chore after M11ar): a drag is selecting, where the pointer last was, an
    /// auto-scroll running (the pointer above or below the chat), the chat's box as drawn.
    chat_selecting: bool,
    chat_drag_at: Option<gpui::Point<gpui::Pixels>>,
    chat_autoscrolling: bool,
    chat_bounds: Rc<Cell<Option<gpui::Bounds<gpui::Pixels>>>>,
    pub(crate) modal_scroll: gpui::ScrollHandle,
    refocus: Option<Refocus>,
    /// How many images were pasted in this tab (they are named "Image #N").
    pasted: usize,
    /// Their markers (`[Image #1]` …): the box draws them as pills that edit as one.
    pills: Vec<String>,
    /// The path pills in the box (batch 3, chore 5), by the text each sits in it as.
    file_pills: HashMap<String, pills::PillPath>,
    /// The path pills sent, by their chips' words: a chip clicked opens its file.
    sent_pills: HashMap<String, pills::PillPath>,
    /// Paths are being dragged over the tab's page (its drop states show).
    drop_over: bool,
    /// Messages the agent was asked to take back (batch 3, wave 3), not answered yet.
    cancelling: HashSet<String>,
    /// Every image attached in this tab, by its chip's name, so a chip (in the box or in a sent
    /// message) opens it in the viewer. Kept for the tab's life only.
    pub(crate) images: std::collections::HashMap<String, Picture>,
    /// The image open in the viewer: its name and its bytes as GPUI draws them.
    pub(crate) image_view: Option<(String, Arc<gpui::Image>)>,
    /// The model menu's "Older" group, opened (None: as it starts, closed; reset when the
    /// menu closes).
    pub(crate) models_older_open: Option<bool>,
    /// The model menu's pending swap of the effort panel to another hovered row (it waits
    /// `EFFORT_SWITCH_DELAY`; dropped to cancel), and the panel's bounds as last drawn (a press
    /// there is a pick, not outside the menu).
    pub(crate) effort_switch: Option<gpui::Task<()>>,
    /// The box's sweep (`agent_sweep`): when the last one was asked for (none once you type),
    /// and how many have been (each its own animation).
    pub(crate) sweep_at: Option<Instant>,
    pub(crate) sweeps: u64,
    /// The box's text when the sweep started: it ends only when that changes (the composer
    /// is observed on every redraw: its caret's breath, the keyboard coming back).
    sweep_text: String,
    pub(crate) effort_panel_at: std::rc::Rc<std::cell::Cell<Option<gpui::Bounds<gpui::Pixels>>>>,
    /// The top bar (spec A §C): what its animations start from (`agent_top::TopState`), its
    /// status mark and the clock that turns it (a still one of its own until the Workspace
    /// gives it the shared one, `use_clock`), and its width as last laid out (`agent_top::trim`).
    top: crate::agent_top::TopState,
    top_mark: Entity<MarkView>,
    /// The tool modal's header mark while its tool runs.
    modal_mark: Entity<MarkView>,
    /// The marks' clock (the Workspace's once `use_clock`); the Working arcs of the running
    /// tools' rows (made and dropped in `after_change`, by tool id) and of the "Cultivating…" line.
    clock: Entity<MarkClock>,
    tool_marks: std::collections::HashMap<String, Entity<MarkView>>,
    /// A Working arc for each running background shell's popover row (chore 16).
    shell_marks: std::collections::HashMap<String, Entity<MarkView>>,
    /// The open shells popover's reading of their last output lines (chore 16).
    shell_poll: Option<Task<()>>,
    thinking_mark: Entity<MarkView>,
    bar_width: Rc<Cell<f32>>,
    /// Whether the box's placeholder was last set for a tab under `NARROW` (a lane's relay
    /// placeholder is shorter there; the width is known only once laid out).
    placeholder_narrow: bool,
    /// The agent activity chore: whether the chat column held tracks at the last sync (a busy
    /// step falls back to rows under `TRACKS_NARROW`), the tracks' keyboard focus (key context
    /// `AgentTracks`) and cursor, and the steps opened to every column and to every file.
    tracks_fit: bool,
    tracks_focus: FocusHandle,
    track_cursor: Option<tracks::TrackCursor>,
    tracks_all: HashSet<String>,
    files_all: HashSet<String>,
    /// The shown view's turn ledger (the folder tab's words after the status, and its
    /// popover); None: no live turn.
    pub(crate) ledger: Option<crate::agent_ledger::Ledger>,
    /// The chat code block copied last and when (its ✓, the 2026-11-25 chores).
    code_copied: md_preview::ChatCopied,
    /// What git and the disk say of the turn's files (the 2026-11-25 chores), for the paths
    /// last asked about; read off the UI thread when the ledger's files change.
    pub(crate) ledger_facts: std::collections::HashMap<PathBuf, crate::changed_files::FileFacts>,
    ledger_facts_for: Vec<PathBuf>,
    ledger_facts_gen: u64,
    /// The ledger popover's folded folders (their header's click).
    pub(crate) ledger_folded: std::collections::HashSet<String>,
    /// When the branch was last read for the chip, and a read waiting for its turn.
    branch_read_at: Option<Instant>,
    branch_read_queued: bool,
    /// The plan cards opened whole (Show the whole plan), by their request's id (the
    /// 2026-11-25 chores).
    plan_open: std::collections::HashSet<String>,
    /// Keep planning… was pressed: the waiting plan's buttons step aside for your note.
    plan_note: bool,
    /// The waiting plan's footer, where it was last painted (the pinned bar's cue).
    plan_footer: plan_card::FooterSeen,
    /// The frame being drawn (the footer's place counts only from the frame just painted).
    plan_frame: Cell<u64>,
    /// Whether the box held text at the last draw (a plan card reads it as a note).
    box_text_nonempty: Cell<bool>,
    /// The ledger's kept sums (`agent_ledger::LedgerCache`).
    ledgers: crate::agent_ledger::LedgerCache,
    /// The "Stopped" notice that offers to rewind your latest message (`rewind::stopped_link`),
    /// found with the ledger, so a frame never looks for it.
    pub(crate) stopped: Option<usize>,
    /// A batch of the process's lines is being handled: the rows wait for its last
    /// (`on_process_batch`), and are owed a sync.
    rows_held: bool,
    rows_pending: bool,
    /// When each tool call started and finished, live only (its elapsed time; the agent
    /// activity chore's step times).
    tool_times: crate::agent_steps::ToolTimes,
    /// Tool calls that write files and haven't finished (reported to the edit ledger).
    open_edits: HashSet<String>,
    /// Why the edits were made (M11ag): see `Reasons`.
    reasons: Reasons,
    said: Said,
    /// Stop was pressed: the turn hasn't ended yet. `force`: `STOP_FORCE_AFTER` has passed.
    stopping: Option<Instant>,
    force: bool,
    pub(crate) attachments: Vec<Attachment>,
    /// The box's open menu (one at a time; Esc closes it).
    pub(crate) menu: Option<BoxMenu>,
    /// The folder picker's search and rows, while it is open.
    pub(crate) picker: Option<crate::agent_box::FolderPicker>,
    /// Where the press was that last closed the folder or model menu from outside it: that
    /// chip's click from the same press leaves it closed (not closed and opened again).
    pub(crate) chip_closed_at: Option<gpui::Point<gpui::Pixels>>,
    /// The usage gauge's panel opens on a rest over it (the 2026-11-24 chores: hover, not click).
    pub(crate) usage_hover: crate::hover_delay::HoverDelay,
    /// The clock's cost and context panel opens the same way.
    pub(crate) clock_hover: crate::hover_delay::HoverDelay,
    /// Plan progress (the 2026-11-24 chores): the burger tab's tasks, the checklist file they
    /// come from (none: the agent's own todo list), and whether to read them again.
    pub(crate) plan: Vec<crate::plan_progress::PlanTask>,
    /// The file, and whether the turn wrote it (a file only read counts only as a plan with
    /// `### Task N` headings).
    plan_file: Option<(PathBuf, bool)>,
    /// The `.md` files the turn wrote or read, newest first (written before read): where the
    /// plan is looked for before `plan_file`, the one shown (chore 14 of the 2026-11-25 batch).
    plan_turn_files: Vec<(PathBuf, bool)>,
    plan_stale: bool,
    /// A finished plan, hidden after your next message until its list changes.
    plan_done: Option<Vec<crate::plan_progress::PlanTask>>,
    /// Each read's number: an older read that lands late is dropped.
    plan_gen: u64,
    /// The branch checked out in the agent's folder (none outside a repo, or before it is read):
    /// looked up on the background executor, never in `render` (`read_branch`).
    pub(crate) branch: Option<String>,
    files: Option<Files>,
    pub(crate) listing: bool,
    pub(crate) at: Option<AtList>,
    /// The `/` list, while it is open.
    pub(crate) slash: Option<SlashList>,
    /// Esc closed the `/` list: it stays closed until the box leaves the `/` word.
    slash_dismissed: bool,
    pub(crate) slash_scroll: gpui::ScrollHandle,
    rank_generation: Arc<AtomicU64>,
    ticker: Option<Task<()>>,
    end_timer: Option<Task<()>>,
    force_timer: Option<Task<()>>,
    _boot: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
    /// Box changes waiting for the turn to end (one per kind of change, the latest); while one
    /// of them is a restart, what you send waits with them (`queued`), for the restarted agent.
    waiting: Vec<Change>,
    asked: Asked,
    /// The scripting socket and this tab's `aN` (`set_link_caller`): its process's programs
    /// open their pages beside it (`auto_open::agent_env`, `fuxx link open`).
    link_caller: Option<(PathBuf, u64)>,
    /// The command line of the last start (scenes check a restart resumes), and the environment
    /// it was given.
    #[cfg(feature = "ui-shots")]
    last_argv: Vec<String>,
    #[cfg(feature = "ui-shots")]
    last_env: Vec<(String, String)>,
    /// The lines a scene tab (no process) would have written to its agent.
    #[cfg(feature = "ui-shots")]
    written: Vec<String>,
    /// How many item rows the list built (scenes check only the rows in sight are laid out).
    #[cfg(feature = "ui-shots")]
    pub rows_built: Rc<Cell<usize>>,
    /// Where each waiting attachment's × was laid out (scenes click it).
    #[cfg(feature = "ui-shots")]
    pub(crate) removes: Rc<std::cell::RefCell<Vec<Option<gpui::Bounds<gpui::Pixels>>>>>,
    /// Where each part of the box was laid out (`shot_part`).
    #[cfg(feature = "ui-shots")]
    pub(crate) parts: crate::agent_box::Parts,
    /// Where each row of the open menu was laid out, by its words (`shot_menu_row`).
    #[cfg(feature = "ui-shots")]
    pub(crate) menu_rows: crate::agent_box::MenuRows,
    /// The latest-prompt strip (chore after M11ar): whether the last draw showed it, the jump's
    /// glide while it runs, and the jumped-to message's highlight.
    strip: latest::Strip,
    /// Rewind and Fork (batch 3, chore 2).
    pub(crate) rw: rewind::State,
    /// The agent's question card (batch 3, chore 8).
    pub(crate) ask: ask_view::AskState,
}

impl EventEmitter<AgentTabEvent> for AgentTab {}

impl Focusable for AgentTab {
    /// The composer: the tab takes the keyboard there — or the tool modal while it is open, so
    /// a Workspace giving the tab the keyboard back (after the palette, a tab switch) never
    /// puts Esc in the box, where it stops the agent (the review's finding 2).
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        // The image viewer holds it too (it shares the modal's handle: they never show together).
        if self.modal.is_some() || self.image_view.is_some() {
            return self.modal_focus.clone();
        }
        self.composer.read(cx).focus_handle(cx)
    }
}

impl AgentTab {
    /// A press in the chat (chore after M11ar): on text, a selection starts there (a double-click
    /// takes the word, a triple-click the paragraph, ⇧ extends the one there is); anywhere else,
    /// the selection goes. The box's own selection goes too, so ⌘C copies what you just picked.
    fn chat_press(
        &mut self,
        e: &gpui::MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use crate::chat_select::{Pos, Sel, word_at};
        // ⌃-click is a right-click on a Mac.
        if e.modifiers.control {
            self.chat_menu(e, window, cx);
            return;
        }
        let select = self.pane().select.clone();
        let Some(at) = select.pos_at(e.position, true) else {
            if select.sel().is_some() {
                select.set(None);
                cx.notify();
            }
            return;
        };
        self.composer.update(cx, |c, cx| c.collapse_selection(cx));
        let whole = |r: std::ops::Range<usize>| {
            Sel::new(
                Pos {
                    key: at.key,
                    ix: r.start,
                },
                Pos {
                    key: at.key,
                    ix: r.end,
                },
            )
        };
        let sel = match e.click_count {
            2 => select.text_of(at.key).map(|t| whole(word_at(&t, at.ix))),
            n if n >= 3 => select.text_of(at.key).map(|t| whole(0..t.len())),
            _ if e.modifiers.shift => select.sel().map(|s| Sel::new(s.anchor, at)),
            _ => None,
        }
        .unwrap_or(Sel::new(at, at));
        select.set(Some(sel));
        self.chat_selecting = e.click_count <= 1;
        cx.notify();
    }

    /// The pointer moved anywhere in the window: a selection's head follows it (held inside the
    /// chat's height); above or below the chat, the chat scrolls on its own.
    fn chat_drag(&mut self, e: &gpui::MouseMoveEvent, window: &mut Window, cx: &mut Context<Self>) {
        if !self.chat_selecting {
            return;
        }
        if e.pressed_button != Some(MouseButton::Left) {
            self.chat_selecting = false;
            return;
        }
        self.chat_drag_at = Some(e.position);
        self.chat_head(e.position, cx);
        let out = self
            .chat_bounds
            .get()
            .is_some_and(|b| e.position.y < b.top() || e.position.y > b.bottom());
        if out && !self.chat_autoscrolling {
            self.chat_autoscroll(window, cx);
        }
    }

    /// The selection's head to the text nearest `p` (held inside the chat's height).
    fn chat_head(&mut self, p: gpui::Point<gpui::Pixels>, cx: &mut Context<Self>) {
        let select = self.pane().select.clone();
        let p = match self.chat_bounds.get() {
            Some(b) => gpui::point(p.x, p.y.clamp(b.top() + px(1.), b.bottom() - px(1.))),
            None => p,
        };
        if let (Some(s), Some(head)) = (select.sel(), select.pos_at(p, false))
            && s.head != head
        {
            select.set(Some(crate::chat_select::Sel::new(s.anchor, head)));
            cx.notify();
        }
    }

    /// Selecting with the pointer above or below the chat: it scrolls every frame towards the
    /// pointer (faster the farther it is) until the drag ends, the pointer comes back, or the
    /// chat's end is reached. One task at a time.
    fn chat_autoscroll(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.chat_autoscrolling = true;
        cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(16))
                    .await;
                let go =
                    cx.update(|_, cx| this.update(cx, |this, cx| this.chat_autoscroll_step(cx)));
                if !matches!(go, Ok(Ok(true))) {
                    break;
                }
            }
            cx.update(|_, cx| this.update(cx, |this, _| this.chat_autoscrolling = false))
                .ok();
        })
        .detach();
    }

    fn chat_autoscroll_step(&mut self, cx: &mut Context<Self>) -> bool {
        let (true, Some(p), Some(b)) = (
            self.chat_selecting,
            self.chat_drag_at,
            self.chat_bounds.get(),
        ) else {
            return false;
        };
        let push = if p.y < b.top() {
            -(b.top() - p.y).clamp(px(4.), px(40.))
        } else if p.y > b.bottom() {
            (p.y - b.bottom()).clamp(px(4.), px(40.))
        } else {
            return false;
        };
        let list = self.pane().list.clone();
        let before = list.logical_scroll_top();
        list.scroll_by(push);
        let after = list.logical_scroll_top();
        self.chat_head(p, cx);
        cx.notify();
        before.item_ix != after.item_ix || before.offset_in_item != after.offset_in_item
    }

    /// ⌘C with nothing selected in the box: the chat's selection.
    fn copy_chat(&mut self, cx: &mut Context<Self>) {
        if let Some(text) = self.pane().select.copied() {
            cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
        }
    }

    /// ⌘A with nothing typed: the whole chat (as far as it has been drawn).
    fn select_all_chat(&mut self, cx: &mut Context<Self>) {
        self.pane().select.select_all();
        cx.notify();
    }

    /// The chat menu's Select All (wave 3).
    pub(crate) fn select_all_in_chat(&mut self, cx: &mut Context<Self>) {
        self.select_all_chat(cx);
    }

    /// A right-click (or ⌃-click) in the chat (batch 3, chore 4): fuxx's menu, for the chat's
    /// selection (kept) and the link under the pointer.
    fn chat_menu(&mut self, e: &gpui::MouseDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let select = self.pane().select.clone();
        // A web or mail link's items only (a file link is opened by a click).
        let link = select.link_at(e.position).and_then(|url| {
            match md_preview::link_target(&self.start.cwd, &url) {
                LinkTarget::Web(url) => Some(url),
                _ => None,
            }
        });
        let selection = select.copied();
        cx.emit(AgentTabEvent::ContextMenu {
            at: e.position,
            selection,
            link,
        });
    }

    /// The box menu's choice (wave 3), as its keys do in the box.
    pub(crate) fn box_menu_action(
        &mut self,
        action: crate::context_menu::MenuAction,
        cx: &mut Context<Self>,
    ) {
        use crate::context_menu::MenuAction;
        self.composer.update(cx, |c, cx| match action {
            MenuAction::Cut => c.menu_cut(cx),
            MenuAction::Copy => c.menu_copy(cx),
            MenuAction::Paste => c.menu_paste(cx),
            MenuAction::SelectAll => c.menu_select_all(cx),
            _ => {}
        });
    }

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
                this.said.status = Some(this.shown_status());
            }
            Some(provider) => {
                this.booting = true;
                this.read_branch(cx);
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

    /// The background shells changed by `f` (chore 16's scenes date them back).
    #[cfg(feature = "ui-shots")]
    pub fn shot_shells(
        &mut self,
        f: impl FnOnce(&mut Vec<crate::agent_shells::Shell>),
        cx: &mut Context<Self>,
    ) {
        self.conversation.shot_shells(f);
        self.after_change(true, cx);
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
        self.listen(&argv, &[], cx);
    }

    /// Open tool row `ix`'s detail, as its click does.
    #[cfg(feature = "ui-shots")]
    pub fn shot_open_tool(&mut self, ix: usize, cx: &mut Context<Self>) {
        if let Some(Item::Tool { id, .. }) = self.conversation.items().get(ix) {
            let id = id.clone();
            self.open_tool(id, cx);
        }
    }

    /// The tool modal's Esc, ↑ and ↓, as their keys.
    #[cfg(feature = "ui-shots")]
    pub fn shot_modal_key(&mut self, key: &str, cx: &mut Context<Self>) {
        match key {
            "escape" => self.back_out(cx),
            "up" => self.walk(-1, cx),
            "down" => self.walk(1, cx),
            _ => {}
        }
    }

    /// The mode `id` chosen from the box's menu, as its row's click does.
    #[cfg(feature = "ui-shots")]
    pub fn shot_choose_mode(&mut self, id: &str, cx: &mut Context<Self>) {
        self.choose_mode(id, cx);
    }

    #[cfg(feature = "ui-shots")]
    pub fn shot_items(&self) -> &[Item] {
        self.conversation.items()
    }

    /// The chat text painted last frame: each piece's text and box (chore after M11ar).
    #[cfg(feature = "ui-shots")]
    pub fn shot_text_boxes(&self) -> Vec<(String, gpui::Bounds<gpui::Pixels>)> {
        let select = &self.pane().select;
        select
            .painted_boxes()
            .into_iter()
            .map(|(k, b)| {
                (
                    select.text_of(k).map(|t| t.to_string()).unwrap_or_default(),
                    b,
                )
            })
            .collect()
    }

    #[cfg(feature = "ui-shots")]
    pub fn shot_list(&self) -> ListState {
        self.pane().list.clone()
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

    /// Start the agent again on its conversation, as a restart does (a scene starts nothing:
    /// the command line is only built).
    #[cfg(feature = "ui-shots")]
    pub fn shot_respawn(&mut self, cx: &mut Context<Self>) {
        let resume = self.conversation_id();
        self.spawn_process(resume, cx);
        self.after_change(true, cx);
    }

    /// While this fuxx serves `socket`, its processes (from the next start) get it and this
    /// tab's id `aN`, with the auto-open shims: a program they run that opens a browser by
    /// itself lands beside this tab. Set by the workspace before the first start.
    pub(crate) fn set_link_caller(&mut self, socket: Option<PathBuf>, id: u64) {
        self.link_caller = socket.map(|socket| (socket, id));
    }

    /// The environment the last (re)start added (prompt suggestions' switch).
    #[cfg(feature = "ui-shots")]
    pub fn shot_last_env(&self) -> Vec<(String, String)> {
        self.last_env.clone()
    }

    /// The lines written to the agent (a scene tab's, kept by `write`).
    #[cfg(feature = "ui-shots")]
    pub fn shot_written(&self) -> Vec<String> {
        self.written.clone()
    }

    /// The conversation the tab resumes when its process starts again.
    #[cfg(feature = "ui-shots")]
    pub fn shot_resume(&self) -> Option<String> {
        self.start.resume.clone()
    }

    /// Whether tool `id`'s detail is on screen.
    #[cfg(feature = "ui-shots")]
    pub fn shot_open(&self, id: &str) -> bool {
        self.modal.as_ref().and_then(ToolModal::shown) == Some(id)
    }

    /// The Stop / Force stop button, as its click.
    #[cfg(feature = "ui-shots")]
    pub fn shot_press_stop(&mut self, cx: &mut Context<Self>) {
        self.stop_pressed(cx);
    }

    /// Force stop, as its button does once offered.
    #[cfg(feature = "ui-shots")]
    pub fn shot_force_stop(&mut self, cx: &mut Context<Self>) {
        self.force_stop(cx);
    }

    /// The catalog the box's chips and menus read, as if this tab's agent had answered it.
    #[cfg(feature = "ui-shots")]
    pub fn shot_catalog(
        &mut self,
        found: crate::agent_catalog::Lookup<crate::providers::Catalog>,
        cx: &mut Context<Self>,
    ) {
        if let Some(provider) = self.provider {
            crate::agent_catalog::store_catalog(provider.info().name, &self.start.cwd, found, cx);
        }
        cx.notify();
    }

    /// What the box's chips and the top line say.
    #[cfg(feature = "ui-shots")]
    pub fn shot_box(&self, cx: &App) -> crate::agent_box::BoxTexts {
        self.box_texts(cx)
    }

    /// The chat's status line and its counts line, as drawn (built here for the scenes, not on
    /// every frame with the box's texts).
    #[cfg(feature = "ui-shots")]
    pub fn shot_status_line(&self, cx: &App) -> Option<(String, Option<String>)> {
        self.status_line_now(&cx.global::<Appearance>().theme)
    }

    /// Where a part of the box was last laid out: `folder`, `box`, `attachments`, `attach`,
    /// `model`, `mode`, `send`, `menu`, `usage`.
    #[cfg(feature = "ui-shots")]
    pub fn shot_part(&self, part: &str) -> Option<gpui::Bounds<gpui::Pixels>> {
        self.parts.borrow().get(part).copied()
    }

    /// The box's open menu.
    #[cfg(feature = "ui-shots")]
    pub fn shot_menu(&self) -> Option<BoxMenu> {
        self.menu.clone()
    }

    /// Whether the `@` file list is open.
    pub(crate) fn at_open(&self) -> bool {
        self.at.is_some()
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
        let composer = cx.new(|cx| {
            let mut composer = Composer::new(PLACEHOLDER.into(), window, cx);
            // The box draws its edge and fill.
            composer.set_bare(true, cx);
            composer
        });
        let panes = HashMap::from([(crate::agent_lanes::View::Main, Pane::new())]);
        let subscriptions = vec![
            // The question card's light goes out while the window isn't the key window (the
            // design review of batch 3, chore 8): a change of key window draws it again.
            cx.observe_window_activation(window, |this, window, cx| {
                if this.asking() {
                    cx.notify();
                }
                // Back to the front: a branch switched outside fuxx shows on the chip.
                if window.is_window_active() {
                    this.read_branch_soon(cx);
                }
            }),
            cx.subscribe_in(&composer, window, Self::on_composer),
            cx.observe(&composer, |this, _, cx| this.composer_changed(cx)),
            // The box's menus draw from the cache: a probe finishing redraws the tab (and the
            // folder picker's lists are made again).
            cx.observe_global::<crate::agent_catalog::Catalogs>(|this, cx| {
                this.refresh_picker(cx);
                cx.notify()
            }),
            cx.observe_global::<crate::folder_picker::KnownFolders>(|this, cx| {
                this.refresh_picker(cx)
            }),
            // Closed before its process answered the catalog: the menu mustn't load forever.
            cx.on_release(|this, cx| {
                if let (true, Some(provider)) = (this.catalog_pending, this.provider) {
                    crate::agent_catalog::fail_catalog_if_loading(
                        provider.info().name,
                        &this.start.cwd,
                        "its tab closed before it answered",
                        cx,
                    );
                }
            }),
        ];
        let clock = cx.new(|_| MarkClock { phase: 0.0 });
        let top_mark = cx.new(|cx| MarkView::new(&clock, Status::Idle, px(MARK), BLACK, cx));
        let modal_mark = cx.new(|cx| MarkView::new(&clock, Status::Working, px(MARK), BLACK, cx));
        let thinking_mark =
            cx.new(|cx| MarkView::new(&clock, Status::Working, px(MARK), BLACK, cx));
        Self {
            provider,
            provider_name,
            start,
            composer,
            conversation: {
                let mut c = Conversation::new();
                c.set_questions_by_message(provider.is_some_and(|p| p.questions_by_message()));
                c
            },
            path: None,
            translator: None,
            process: None,
            started: Vec::new(),
            pump: None,
            followers: HashMap::new(),
            lane_marks: HashMap::new(),
            pill_labels: Default::default(),
            lane_runs: HashMap::new(),
            lane_flash: HashMap::new(),
            bar_open: None,
            draft: None,
            retargeted_at: None,
            bar_changes: 0,
            bar_changed_at: None,
            bar_was: false,
            lanes_menu_running: HashSet::new(),
            main_mark: None,
            lane_times: HashMap::new(),
            lanes_menu: false,
            end: ProcessEnd::default(),
            exited: false,
            booting: false,
            ending: false,
            paused: false,
            queued: Vec::new(),
            run: Run::default(),
            suggestion: None,
            recall: None,
            unconfirmed: None,
            no_process: false,
            catalog_pending: false,
            panes,
            view: crate::agent_lanes::View::Main,
            modal: None,
            modals_opened: 0,
            cleared: 0,
            cleared_at: None,
            modal_opened_at: None,
            modal_focus: cx.focus_handle(),
            chat_selecting: false,
            chat_drag_at: None,
            chat_autoscrolling: false,
            chat_bounds: Rc::default(),
            modal_scroll: gpui::ScrollHandle::new(),
            refocus: None,
            pasted: 0,
            pills: Vec::new(),
            file_pills: HashMap::new(),
            sent_pills: HashMap::new(),
            drop_over: false,
            cancelling: HashSet::new(),
            images: std::collections::HashMap::new(),
            image_view: None,
            models_older_open: None,
            effort_switch: None,
            sweep_at: None,
            sweeps: 0,
            sweep_text: String::new(),
            effort_panel_at: Default::default(),
            top: crate::agent_top::TopState::default(),
            top_mark,
            modal_mark,
            clock,
            tool_marks: std::collections::HashMap::new(),
            shell_marks: std::collections::HashMap::new(),
            shell_poll: None,
            thinking_mark,
            bar_width: Rc::new(Cell::new(f32::MAX)),
            placeholder_narrow: false,
            tracks_fit: true,
            tracks_focus: cx.focus_handle(),
            track_cursor: None,
            tracks_all: HashSet::new(),
            files_all: HashSet::new(),
            ledger: None,
            code_copied: Rc::default(),
            ledger_facts: Default::default(),
            ledger_facts_for: Vec::new(),
            ledger_facts_gen: 0,
            ledger_folded: Default::default(),
            branch_read_at: None,
            branch_read_queued: false,
            plan_open: Default::default(),
            plan_note: false,
            plan_footer: Rc::default(),
            plan_frame: Cell::new(0),
            box_text_nonempty: Cell::new(false),
            ledgers: Default::default(),
            stopped: None,
            rows_held: false,
            rows_pending: false,
            tool_times: crate::agent_steps::ToolTimes::new(),
            open_edits: HashSet::new(),
            reasons: Reasons::default(),
            said: Said::default(),
            stopping: None,
            force: false,
            attachments: Vec::new(),
            menu: None,
            picker: None,
            chip_closed_at: None,
            usage_hover: Default::default(),
            clock_hover: Default::default(),
            plan: Vec::new(),
            plan_file: None,
            plan_turn_files: Vec::new(),
            plan_stale: false,
            plan_done: None,
            plan_gen: 0,
            branch: None,
            files: None,
            listing: false,
            at: None,
            slash: None,
            slash_dismissed: false,
            slash_scroll: gpui::ScrollHandle::new(),
            rank_generation: Arc::new(AtomicU64::new(0)),
            ticker: None,
            end_timer: None,
            force_timer: None,
            _boot: None,
            _subscriptions: subscriptions,
            waiting: Vec::new(),
            asked: Asked::default(),
            link_caller: None,
            #[cfg(feature = "ui-shots")]
            last_argv: Vec::new(),
            #[cfg(feature = "ui-shots")]
            last_env: Vec::new(),
            #[cfg(feature = "ui-shots")]
            written: Vec::new(),
            #[cfg(feature = "ui-shots")]
            rows_built: Rc::new(Cell::new(0)),
            #[cfg(feature = "ui-shots")]
            removes: Rc::default(),
            #[cfg(feature = "ui-shots")]
            parts: Rc::default(),
            #[cfg(feature = "ui-shots")]
            menu_rows: Rc::default(),
            strip: latest::Strip::default(),
            rw: rewind::State::default(),
            ask: ask_view::AskState::default(),
        }
    }

    pub fn status(&self) -> Status {
        self.conversation.status()
    }

    /// What the tab shows (its mark, the sidebar, the status pill, closing's question): Working
    /// while a subagent still runs after Main finished. `status` stays Main's own (Stop, the
    /// box's notes, sending).
    pub fn shown_status(&self) -> Status {
        crate::agent_lanes::shown_status(self.status(), self.conversation.lanes())
    }

    /// "⟨Label⟩ wants to run: ⟨what⟩" while a permission waits; "⟨Label⟩ asks: ⟨question⟩"
    /// while the agent's own question is the oldest thing waiting (batch 3, chore 8).
    pub fn question(&self) -> Option<String> {
        if let Some(line) = self.ask_question_line() {
            return Some(line);
        }
        match self.conversation.pending_permission() {
            Some(Item::Permission { plan: Some(_), .. }) => {
                Some(format!("{}'s plan is ready", self.label()))
            }
            Some(Item::Permission { what, .. }) => {
                Some(format!("{} wants to run: {what}", self.label()))
            }
            _ => None,
        }
    }

    /// Its agent's questions are answered by a message (`Provider::questions_by_message`): an
    /// open one doesn't make the tab wait.
    pub fn questions_by_message(&self) -> bool {
        self.conversation.questions_by_message()
    }

    /// The mode the agent starts with: chosen, or the one it last reported (R9).
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

    /// The run ended (or was stopped): its open edits end as not ok, so one that never finished
    /// can't claim later changes.
    fn abandon_edits(&mut self, cx: &mut Context<Self>) {
        self.reasons.started.clear();
        self.reasons.end_turn();
        for call in std::mem::take(&mut self.open_edits) {
            cx.emit(AgentTabEvent::EditDone { call, ok: false });
        }
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
        self.abandon_edits(cx);
        self.start_with_waiting(false, cx);
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
        for (text, attachments, _) in queued {
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
    /// `--resume`, the box cleared unless you edited it during the check), or a notice saying
    /// why not with `text` left in the box: never a second writer of one conversation, nor a
    /// start without an answer.
    pub fn resume_after_classic(
        &mut self,
        text: String,
        holder: Holder,
        terminal: Option<SharedString>,
        cx: &mut Context<Self>,
    ) {
        use crate::workspace_agent::{AfterClassic, after_classic, held_notice};
        let sent = text.clone();
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
        // What was typed while the check ran stays.
        if self.composer.read(cx).text() == sent {
            self.composer
                .update(cx, |c, cx| c.set_text(String::new(), cx));
        }
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
            // No process of its own: the probe answers the catalog.
            // (A scene tab never probes.)
            if let Some(provider) = self.provider.filter(|_| !self.no_process) {
                crate::agent_catalog::load_catalog(provider, self.start.cwd.clone(), cx);
            }
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
        for (text, attachments, to) in std::mem::take(&mut self.queued) {
            self.say_to(text, attachments, to);
        }
    }

    /// Show what you said and send it to the running process. The first message of a new
    /// conversation started under an id fuxx chose makes that id the tab's conversation at once
    /// (told, so saved: a process left running by a crash is found holding it, R13).
    fn say(&mut self, text: String, attachments: Vec<Attachment>) {
        self.say_to(text, attachments, None);
    }

    /// `say`, for a subagent when `to` names one (the lanes chore): a message to Main all the
    /// same — framed for it to pass on (or answer about), your words in the bubble.
    fn say_to(
        &mut self,
        text: String,
        attachments: Vec<Attachment>,
        to: Option<lanes_view::Target>,
    ) {
        let attachments = kept(&text, attachments);
        if let Some(id) = self.run.fresh.clone()
            && !self.run.started
            && self.conversation.conversation() != Some(id.as_str())
        {
            self.conversation.apply(Event::Started {
                conversation: id.clone(),
                model: None,
            });
            self.unconfirmed = Some(id);
        }
        let inline = self.provider.is_some_and(|p| p.sends_images());
        let wire = wire_text(&text, &attachments, inline, &self.start.cwd);
        let images = if inline {
            pictures(&attachments)
        } else {
            Vec::new()
        };
        // Its id, when the agent takes one (batch 3, chore 2): Rewind and Fork name it by it.
        let id = self.provider.and_then(|p| p.fresh_message_id());
        self.rewind_sent(id.as_deref(), &attachments);
        if let Some(to) = to {
            self.conversation
                .relay(text.clone(), id.clone(), to.lane.clone(), to.about);
            self.run.sends.push((text, attachments));
            self.write(&to.action(wire, id));
            return;
        }
        self.conversation
            .you_as(text.clone(), labels(&attachments), id.clone());
        self.run.sends.push((text, attachments));
        self.write(&Action::Send {
            text: wire,
            images,
            id,
        });
    }

    /// Start (or start again) the agent, resuming `resume`; with none, a new conversation (under
    /// an id fuxx chooses, if the provider takes one).
    fn spawn_process(&mut self, resume: Option<String>, cx: &mut Context<Self>) {
        self.end_process();
        // Live changes the old process never answered: the new one starts with them.
        let asked = std::mem::take(&mut self.asked);
        let before = self.settings();
        if asked.model.is_some() {
            self.start.model = asked.model;
        }
        if asked.effort.is_some() {
            self.start.effort = asked.effort;
        }
        if asked.mode.is_some() {
            self.start.mode = asked.mode;
        }
        self.tell_settings(before, cx);
        // A process ended and not yet reported (Force stop within `END_WAIT`) never will be:
        // sends go to this one.
        self.ending = false;
        // A chosen id the agent never said it took may never have been made: a new one again.
        // A branch (Rewind, Fork: batch 3, chore 2) starts a new conversation off an old one.
        let branch = self.rw.branch_next.take();
        let resume = resume
            .filter(|id| self.unconfirmed.as_ref() != Some(id))
            .filter(|_| branch.is_none());
        let fresh = match (&resume, self.provider) {
            (None, Some(provider)) => provider.fresh_conversation_id(),
            _ => None,
        };
        self.run = Run {
            resume: resume.clone(),
            fresh: fresh.clone(),
            branched: branch.is_some(),
            started: false,
            first_item: self.conversation.items().len(),
            sends: Vec::new(),
        };
        self.end = ProcessEnd::default();
        self.exited = false;
        // A guess from the last process is about what it said: gone with it.
        self.suggestion = None;
        let Some(provider) = self.provider else {
            return;
        };
        let start = Start {
            resume,
            new_id: fresh,
            branch,
            ..self.start.clone()
        };
        // "Prompt suggestions" (on by default), read at each start: a change applies to agents
        // started after it.
        let suggest = cx
            .try_global::<Appearance>()
            .is_none_or(|a| a.agent_suggestions);
        let argv = with_suggestions(provider.argv(&start), &provider.suggestion_flags(), suggest);
        let env = provider.process_env(suggest);
        #[cfg(feature = "ui-shots")]
        {
            self.last_argv = argv.clone();
            self.last_env = env.clone();
        }
        if self.no_process {
            return;
        }
        // fuxx's own (socket, tab, shims: `auto_open`), the shims first on the login `PATH`.
        let mut env = env;
        if let Some((socket, id)) = &self.link_caller {
            let inherited = std::env::var_os("PATH");
            let path = self
                .path
                .as_deref()
                .map(std::ffi::OsStr::new)
                .or(inherited.as_deref());
            env.extend(crate::auto_open::agent_env(
                Some(socket),
                *id,
                crate::auto_open::shims(),
                path,
            ));
        }
        let process = self.listen(&argv, &env, cx);
        self.ask_catalog(provider, cx);
        let mut translator = provider.translator();
        for line in translator.hello() {
            process.send(line);
        }
        self.translator = Some(translator);
    }

    /// The catalog for the box's menus: from this tab's own process when the agent answers it at
    /// start (marked loading until it does), else from a probe.
    fn ask_catalog(&mut self, provider: &'static dyn Provider, cx: &mut Context<Self>) {
        let agent = provider.info().name;
        if provider.catalog_on_start() {
            let cwd = self.start.cwd.clone();
            // Still waiting on this process's answer even when the entry already says loading
            // (a restart, or another tab asked first): if it ends unanswered, the entry fails.
            self.catalog_pending = true;
            cx.update_default_global(|c: &mut crate::agent_catalog::Catalogs, _| {
                c.begin_catalog(agent, &cwd);
            });
        } else {
            crate::agent_catalog::load_catalog(provider, self.start.cwd.clone(), cx);
        }
    }

    /// The process ended without answering the catalog: say so, so the menu stops loading.
    fn catalog_unanswered(&mut self, cx: &mut Context<Self>) {
        if !std::mem::take(&mut self.catalog_pending) {
            return;
        }
        if let Some(provider) = self.provider {
            crate::agent_catalog::fail_catalog_if_loading(
                provider.info().name,
                &self.start.cwd,
                "it ended without answering",
                cx,
            );
        }
    }

    /// Start `argv` in the agent's folder and read its events on a task (`on_process`).
    fn listen(
        &mut self,
        argv: &[String],
        env: &[(String, String)],
        cx: &mut Context<Self>,
    ) -> Arc<AgentProcess> {
        let (tx, mut rx) = mpsc::unbounded();
        let process = Arc::new(agent_process::spawn(
            argv,
            &self.start.cwd,
            self.path.as_deref(),
            env,
            tx,
        ));
        self.process = Some(process.clone());
        self.started.retain(|p| !p.reaped());
        self.started.push(process.clone());
        self.pump = Some(cx.spawn(async move |this, cx| {
            while let Some(event) = rx.next().await {
                // The event and whatever else is already read, as one batch: one row sync for a
                // burst (the perf chore).
                let batch = crate::agent_process::drain_ready(
                    event,
                    &mut rx,
                    crate::agent_process::PUMP_BATCH,
                );
                let full = batch.len() == crate::agent_process::PUMP_BATCH;
                if this
                    .update(cx, |this, cx| this.on_process_batch(batch, cx))
                    .is_err()
                {
                    break;
                }
                // A flood: let the window draw before the next batch.
                if full {
                    cx.background_executor().timer(Duration::ZERO).await;
                }
            }
        }));
        process
    }

    /// Stop listening to the process and end it (its group is signalled; `Drop` would too).
    fn end_process(&mut self) {
        self.pump = None;
        self.followers.clear();
        self.end_timer = None;
        if let Some(process) = self.process.take() {
            process.kill();
            // Its subagents go with it (no `process_ended` comes for a process ended here).
            self.conversation.process_gone();
        }
    }

    fn write(&mut self, action: &Action) {
        if let (Some(translator), Some(process)) = (self.translator.as_mut(), &self.process) {
            for line in translator.encode(action) {
                process.send(line);
            }
        }
        // A scene tab has no process: what it would write is kept.
        #[cfg(feature = "ui-shots")]
        if self.no_process
            && let Some(translator) = self.translator.as_mut()
        {
            self.written.extend(translator.encode(action));
        }
    }

    /// A choice made in the box (a model and effort, a mode, a folder). Never sends a message.
    /// The provider says how it applies (`Provider::applies`); `step_for` says when: live on
    /// the running agent, by a restart on the same conversation, after the current turn, or at
    /// its next start. Choosing what it already runs with changes nothing (and drops a waiting
    /// change of that kind).
    pub fn choose(&mut self, change: Change, cx: &mut Context<Self>) {
        let Some(provider) = self.provider else {
            return;
        };
        self.waiting
            .retain(|w| std::mem::discriminant(w) != std::mem::discriminant(&change));
        if self.kinds(provider, &change, cx).is_empty() {
            cx.notify();
            return;
        }
        self.apply_changes(vec![change], cx);
    }

    /// The model change waiting for the turn to end, if there is one (`choose` keeps one).
    pub(crate) fn waiting_model(&self) -> Option<&Change> {
        self.waiting
            .iter()
            .rev()
            .find(|c| matches!(c, Change::Model { .. }))
    }

    /// The mode change waiting for the turn to end, if there is one.
    pub(crate) fn waiting_mode(&self) -> Option<&str> {
        self.waiting.iter().rev().find_map(|c| match c {
            Change::Mode(id) => Some(id.as_str()),
            _ => None,
        })
    }

    /// The mode chosen: one waiting for the turn (shown at once, your words: "Make sure I can
    /// choose mode mid run"), else the start flag's.
    pub(crate) fn mode_chosen(&self) -> Option<&str> {
        self.waiting_mode().or(self.start.mode.as_deref())
    }

    /// The mode asked of the running agent that it hasn't answered yet (⇧Tab moves on from it).
    pub(crate) fn mode_asked(&self) -> Option<&str> {
        self.asked.mode.as_deref()
    }

    /// The latest change waiting for the turn to end.
    #[allow(dead_code)] // read by scenes
    pub fn waiting_change(&self) -> Option<&Change> {
        self.waiting.last()
    }

    /// Events read together (a burst): each handled as if alone, in order, but the rows synced
    /// once for a run of lines (the perf chore). Nothing outside the tab sees it in between: it
    /// is one update; anything that isn't a line finds the rows synced first.
    fn on_process_batch(&mut self, batch: Vec<ProcessEvent>, cx: &mut Context<Self>) {
        let n = batch.len();
        for (i, event) in batch.into_iter().enumerate() {
            let line = matches!(event, ProcessEvent::Line(_) | ProcessEvent::ErrLine(_));
            if !line {
                self.flush_rows();
            }
            self.rows_held = line && i + 1 < n;
            self.on_process(event, cx);
        }
        self.rows_held = false;
        self.flush_rows();
    }

    /// The rows synced, if a batch held them back.
    fn flush_rows(&mut self) {
        if self.rows_pending {
            self.sync_rows();
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
            ProcessEvent::ErrLine(line) => {
                let events = match self.translator.as_mut() {
                    Some(t) => t.decode_stderr(&line),
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
                self.catalog_unanswered(cx);
                self.exited = true;
                self.ending = false;
                self.process = None;
                self.abandon_edits(cx);
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
        self.catalog_unanswered(cx);
        // Its turn is over: what waited for it is how the next start runs.
        self.start_with_waiting(false, cx);
        self.exited = true;
        self.ending = false;
        self.process = None;
        self.pump = None;
        self.followers.clear();
        self.stopping = None;
        self.abandon_edits(cx);
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
        let last_words = said
            .iter()
            .rev()
            .map(|s| s.trim())
            .find(|s| !s.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| match code {
                Some(c) => format!("it exited (code {c})"),
                None => "it was terminated".into(),
            });
        if self.rewind_failed(&last_words, cx) {
            // A rewind's branch that never started: the old conversation is back (never a fresh
            // one: batch 3, chore 2).
        } else if failed_resume && !unknown {
            // It couldn't start (login, network, a crash): the conversation is still there, and
            // the tab keeps its id. One error row says why.
            let sends = std::mem::take(&mut self.run.sends);
            self.conversation.keep_first(self.run.first_item);
            for (text, attachments) in sends {
                self.conversation.you(text, labels(&attachments));
            }
            let label = self.label().to_string();
            let why = format!("Couldn't start {label}: {last_words}");
            self.conversation.failed_to_start(&why);
            cx.emit(AgentTabEvent::Failed(why));
            // What waited for this run goes back into the box: you send it again.
            self.unqueue(cx);
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
        let mut turn_ended = false;
        for event in events {
            let home = crate::workspace::home_dir();
            for told in edit_events(
                &event,
                &self.start.cwd,
                home.as_deref(),
                &mut self.open_edits,
                &mut self.reasons,
            ) {
                cx.emit(told);
            }
            self.rewind_event(&event);
            // A `git switch` by the agent shows on the branch chip (the 2026-11-25 chores).
            if rereads_branch(&event) {
                self.read_branch_soon(cx);
            }
            match &event {
                Event::Started { conversation, .. } => {
                    self.run.started = true;
                    self.unconfirmed = None;
                    // Asked to resume one conversation, the agent began another: the one asked
                    // for is gone, and this is a new one (the id it reported is kept).
                    if self.run.resume.as_ref().is_some_and(|r| r != conversation) {
                        self.start.resume = None;
                        self.conversation
                            .replace_notice(RESTORED_NOTICE, BAD_RESUME_NOTICE);
                    }
                    // The resume is settled: a later id in this run (an agent's `/clear`) is the
                    // conversation going on under it, followed (`after_change`), not a failure.
                    self.run.resume = None;
                }
                Event::ToolStarted { id, .. } => {
                    self.tool_times.insert(id.clone(), (Instant::now(), None));
                }
                Event::ToolFinished { id, .. } => {
                    if let Some((_, end)) = self.tool_times.get_mut(id) {
                        end.get_or_insert_with(Instant::now);
                    }
                }
                Event::Follow { id, path, main } => {
                    self.follow(id.clone(), path.clone(), *main, cx)
                }
                Event::ModeRefused { asked, running } => self.mode_refused(asked, running, cx),
                // Batch 3, wave 3: the answer to × on a message not read yet.
                Event::QueuedCancelled { id, cancelled } => {
                    self.queued_cancelled(id, *cancelled, cx)
                }
                Event::SubagentStarted { id, .. } => {
                    self.lane_times.insert(id.clone(), (Instant::now(), None));
                }
                Event::SubagentEnded { id, .. } => {
                    if let Some((_, ended)) = self.lane_times.get_mut(id) {
                        ended.get_or_insert_with(Instant::now);
                    }
                }
                // The lanes chore: a resume keeps the last run's time for its divider, starts
                // the clock again and flashes the pill.
                Event::SubagentResumed { id, .. } => {
                    let now = Instant::now();
                    let took = self.lane_elapsed(id);
                    self.lane_runs.entry(id.clone()).or_default().push(took);
                    self.lane_times.insert(id.clone(), (now, None));
                    self.lane_flash.insert(id.clone(), now);
                }
                Event::PassedOn { lane, .. } => {
                    self.lane_flash.insert(lane.clone(), Instant::now());
                }
                // `/clear`: the chat empties (the conversation drops its items) and fades back in
                // empty, keyed by how many times it was cleared.
                Event::Cleared => {
                    self.suggestion = None;
                    // The reset settles a resume: the new id after it is the cleared conversation
                    // going on, not a failed resume (your report, 2026-11-22: the first `/clear`
                    // after a restart said "Couldn't resume…").
                    self.run.resume = None;
                    self.tool_times.clear();
                    self.track_cursor = None;
                    self.tracks_all.clear();
                    self.files_all.clear();
                    self.followers.clear();
                    self.lane_times.clear();
                    self.lane_runs.clear();
                    self.lane_flash.clear();
                    self.cleared += 1;
                    self.cleared_at = Some(Instant::now());
                }
                Event::Catalog(catalog) => {
                    self.catalog_pending = false;
                    if let Some(provider) = self.provider {
                        crate::agent_catalog::store_catalog(
                            provider.info().name,
                            &self.start.cwd,
                            crate::agent_catalog::Lookup::Ready(catalog.clone()),
                            cx,
                        );
                    }
                }
                Event::Skills(skills) => {
                    if let Some(provider) = self.provider {
                        let (agent, cwd) = (provider.info().name, self.start.cwd.clone());
                        cx.update_default_global(|c: &mut crate::agent_catalog::Catalogs, _| {
                            c.mark_skills(agent, &cwd, skills)
                        });
                    }
                }
                Event::RateLimit(limit) => {
                    if let Some(provider) = self.provider {
                        let agent = provider.info().name;
                        cx.update_default_global(|c: &mut crate::agent_catalog::Catalogs, _| {
                            c.note_limit(agent, limit.clone())
                        });
                    }
                }
                Event::TurnDone { .. } => {
                    self.stopping = None;
                    self.force = false;
                    self.force_timer = None;
                    turn_ended = true;
                }
                Event::Applied { kind, value, ok } => self.applied(*kind, value, *ok, cx),
                Event::Mode(mode) => self.reported_mode(mode, cx),
                // Kept only while Main is idle with nothing waiting to go: else it is stale.
                Event::Suggestion(text) => {
                    self.suggestion = keeps_suggestion(self.status(), !self.queued.is_empty())
                        .then(|| text.clone());
                }
                _ => {}
            }
            let shell_ending = self.shell_ending(&event);
            self.conversation.apply(event);
            if let Some(id) = shell_ending {
                self.tell_shell_end(&id, cx);
            }
            // Main at work again (a turn starting, a message read, a permission asked): the
            // guess is out of date.
            if matches!(self.status(), Status::Working | Status::Waiting) {
                self.suggestion = None;
            }
        }
        if turn_ended {
            self.settle_waiting(cx);
        }
        self.after_change(true, cx);
    }

    /// After the conversation changed: rows, events, placeholder, the elapsed ticker, a redraw.
    fn after_change(&mut self, live: bool, cx: &mut Context<Self>) {
        // A Working arc for each running tool's row (spec B-B), none for the others.
        // The conversation keeps the running calls (the perf chore): no scan.
        let items = self.conversation.items();
        // A call whose background shell still runs keeps its arc too (chore 16).
        let shell_calls = self
            .conversation
            .shells()
            .rows(Instant::now())
            .into_iter()
            .filter(|s| s.running())
            .map(|s| s.call.clone());
        let running: Vec<String> = self
            .conversation
            .running_tools()
            .iter()
            .filter_map(|&ix| match &items[ix] {
                Item::Tool { id, .. } => Some(id.clone()),
                _ => None,
            })
            .chain(shell_calls)
            .collect();
        self.tool_marks.retain(|id, _| running.contains(id));
        for id in running {
            let clock = self.clock.clone();
            self.tool_marks.entry(id).or_insert_with(|| {
                cx.new(|cx| MarkView::new(&clock, Status::Working, px(MARK), BLACK, cx))
            });
        }
        self.sync_lane_marks(cx);
        self.sync_shell_marks(cx);
        self.close_empty_shells(cx);
        self.sync_draft(cx);
        if self.rows_held {
            self.rows_pending = true;
        } else {
            self.sync_rows();
        }
        self.sync_ask(cx);
        // The modal's tool gone (a restart, a new conversation): back out to what is left.
        if let Some(modal) = &mut self.modal
            && !modal.prune(self.conversation.items())
        {
            self.close_modal(cx);
        }
        let question = self.question();
        if question != self.said.question {
            self.said.question = question.clone();
            cx.emit(AgentTabEvent::Question(question));
        }
        let status = self.shown_status();
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
        if let Some(id) = self.conversation.conversation()
            && self.said.conversation.as_deref() != Some(id)
        {
            self.said.conversation = Some(id.to_string());
            cx.emit(AgentTabEvent::Conversation(id.to_string()));
        }
        self.update_placeholder(cx);
        self.keep_ticking(cx);
        cx.notify();
    }

    /// Lay out again the rows whose prints changed (the view's cache derives them again from
    /// the lowest item changed: the perf chore), add the new ones at the end, drop any gone.
    fn sync_rows(&mut self) {
        use crate::agent_lanes::View;
        use crate::agent_view_cache::hash as print;
        self.rows_pending = false;
        let dirty = self.conversation.take_dirty();
        let view = self.view.clone();
        // A view not shown follows no change: it starts again when shown.
        if dirty.is_some() {
            for (v, pane) in self.panes.iter_mut() {
                if *v != view {
                    pane.cache.invalidate();
                }
            }
        }
        let fits = tracks::fits_tracks(self.bar_width.get());
        self.tracks_fit = fits;
        let lane = match &view {
            View::Lane(id) => self.conversation.lane(id).cloned(),
            View::Main => None,
        };
        let head = lane.is_some();
        let tail = lane.as_ref().is_some_and(|l| !l.status.live());
        let head_print = lane.as_ref().map(|l| print(&("head", &l.task)));
        let tail_print = lane.as_ref().map(|l| {
            let time = self.lane_elapsed(&l.id).map(|d| d.as_secs());
            print(&("tail", format!("{:?}", l.status), time))
        });
        let pane = self.panes.entry(view.clone()).or_insert_with(Pane::new);
        let from = pane
            .cache
            .sync(&self.conversation, &view, dirty, fits, &self.start.cwd);
        let items = self.conversation.items();
        let cache = &pane.cache;
        let order = cache.order();
        let lane_rows = head as usize;
        let total = lane_rows + order.len() + tail as usize;
        let new = |k: usize| match k.checked_sub(lane_rows) {
            None => head_print.unwrap_or_default(),
            Some(j) if j < order.len() => cache.print(j),
            Some(_) => tail_print.unwrap_or_default(),
        };
        let old = pane.rows.len();
        // The rows that may have changed: the head, and from the cache's first on.
        let mut dirty_rows: Vec<usize> = (0..lane_rows)
            .chain(lane_rows + from..total)
            .filter(|&k| k < old && pane.rows[k].print != new(k))
            .collect();
        let block = |k: usize| {
            k.checked_sub(lane_rows)
                .and_then(|j| order.get(j))
                .and_then(|&i| blocks_of(&items[i]))
        };
        let (times, cwd) = (&self.tool_times, &self.start.cwd);
        let role = |k: usize| {
            k.checked_sub(lane_rows)
                .and_then(|j| cache.role(j).cloned())
        };
        let tracks = |role: &Option<crate::agent_steps::Role>| match role {
            Some(crate::agent_steps::Role::Tracks(calls)) => Some(Rc::new(
                crate::agent_steps::tracks_of(items, calls, cache.open(), times, cwd),
            )),
            _ => None,
        };
        let words = |k: usize, role: &Option<crate::agent_steps::Role>| match role {
            Some(crate::agent_steps::Role::Row(_)) => k
                .checked_sub(lane_rows)
                .and_then(|j| order.get(j))
                .map(|&i| Rc::new(crate::agent_step_words::row_words(&items[i]))),
            _ => None,
        };
        if total < old {
            pane.list.splice(total..old, 0);
            pane.rows.drain(total..);
        }
        dirty_rows.retain(|&k| k < total);
        for &k in &dirty_rows {
            pane.list.splice(k..k + 1, 1);
            let role = role(k);
            pane.rows[k] = Row {
                print: new(k),
                blocks: block(k),
                tracks: tracks(&role),
                words: words(k, &role),
                role,
            };
        }
        if total > old {
            pane.list.splice(old..old, total - old);
            for k in old..total {
                let role = role(k);
                pane.rows.push(Row {
                    print: new(k),
                    blocks: block(k),
                    tracks: tracks(&role),
                    words: words(k, &role),
                    role,
                });
            }
        }
        // The strip's message: Main's your latest (cached by the conversation, its row found
        // in the sorted rows), a lane's its task (its first row).
        let said = match &lane {
            Some(l) => Some((0, l.task.as_str())),
            None => self.conversation.latest_you().and_then(|ix| {
                let k = cache.row_of(ix)?;
                match &items[ix] {
                    Item::You { text, .. } => Some((k, text.as_str())),
                    _ => None,
                }
            }),
        };
        pane.latest = latest::Latest::update(pane.latest.take(), said);
        pane.head = head;
        pane.tail = tail;
        // The turn ledger (the agent activity chore): kept, never per frame, and not touched
        // while only the agent's words stream; the "Stopped" link goes with it.
        if let Some(ledger) = self.ledgers.sync(
            &self.conversation,
            &view,
            dirty,
            &self.tool_times,
            &self.start.cwd,
        ) {
            // A checklist the turn wrote or read is the plan (newest first); the agent's own
            // todo list wins.
            if let Some(l) = ledger.as_ref() {
                self.plan_turn_files = plan_files_of(l, &self.start.cwd);
            }
            self.plan_stale = true;
            self.ledger = ledger;
            self.stopped =
                rewind::stopped_link(items, self.conversation.latest_you(), self.branching());
        }
    }

    /// The shown view's list.
    fn pane(&self) -> &Pane {
        self.panes
            .get(&self.view)
            .or_else(|| self.panes.get(&crate::agent_lanes::View::Main))
            .expect("Main's pane is made with the tab")
    }

    /// Item `ix`'s parsed text, from its row `k` in the shown view (no search: the perf chore);
    /// else parsed now.
    pub(super) fn blocks_at(&self, k: usize, ix: usize) -> Option<Rc<Vec<Md>>> {
        let pane = self.pane();
        match pane.cache.order().get(k) {
            Some(&i) if i == ix => pane
                .rows
                .get(k + pane.head as usize)
                .and_then(|r| r.blocks.clone()),
            _ => blocks_of(&self.conversation.items()[ix]),
        }
    }

    /// While a tool runs, its elapsed time turns each second.
    fn keep_ticking(&mut self, cx: &mut Context<Self>) {
        let running = !self.conversation.running_tools().is_empty()
            || self.conversation.lanes().iter().any(|l| l.status.live())
            || self.shells_running();
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
                        // The running calls and the live subagents' calls, from what the
                        // conversation keeps (no scan: the perf chore).
                        let c = &this.conversation;
                        let live = c
                            .lanes()
                            .iter()
                            .filter(|l| l.status.live())
                            .filter_map(|l| c.tool_index(&l.id));
                        // A background shell's Bash row turns its time too (chore 16).
                        let shells: Vec<usize> = c
                            .shells()
                            .rows(Instant::now())
                            .into_iter()
                            .filter(|s| s.running())
                            .filter_map(|s| c.tool_index(&s.call))
                            .collect();
                        let ticking: Vec<usize> = c
                            .running_tools()
                            .iter()
                            .copied()
                            .chain(live)
                            .chain(shells.iter().copied())
                            .collect();
                        let any = !ticking.is_empty()
                            || c.lanes().iter().any(|l| l.status.live())
                            || this.shells_running();
                        // The shown view's rows of them say their time again.
                        let pane = this.pane();
                        let mut rows: Vec<usize> = ticking
                            .iter()
                            .filter_map(|&i| pane.cache.row_of(i))
                            .collect();
                        rows.sort_unstable();
                        rows.dedup();
                        for k in rows {
                            let k = k + pane.head as usize;
                            pane.list.splice(k..k + 1, 1);
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

    pub(super) fn update_placeholder(&mut self, cx: &mut Context<Self>) {
        let label = self.label().to_string();
        let text: SharedString = if let Some(other) = self.ask_placeholder() {
            // The box is the question's Other (batch 3, chore 8).
            other.into()
        } else if let Some(words) = self.lane_placeholder(cx) {
            // A lane's box (the lanes chore): to its subagent via Main, or to Main about it.
            words.into()
        } else if self.drop_over {
            // Files dragged over the tab (batch 3, chore 5).
            pills::DROP_PLACEHOLDER.into()
        } else if self.waiting_plan().is_some() {
            format!("Tell {label} what to change in the plan\u{2026} (optional)").into()
        } else if self.conversation.pending_permission().is_some() {
            "Deny and say why… (optional)".into()
        } else if self.status() == Status::Working {
            format!("Add a note. {label} reads it after this step…").into()
        } else if rewind::stopped_link(
            self.conversation.items(),
            self.conversation.latest_you(),
            self.branching(),
        )
        .is_some()
        {
            // After a stop (batch 3, chore 2).
            rewind::PICK_PLACEHOLDER.into()
        } else {
            PLACEHOLDER.into()
        };
        let ghost = ghost_for(
            self.suggestion.as_deref(),
            self.view == crate::agent_lanes::View::Main,
            self.conversation.pending_permission().is_some() || self.asking(),
            self.status(),
        );
        self.composer.update(cx, |c, cx| {
            c.set_placeholder(text, cx);
            c.set_ghost(ghost.map(SharedString::from), cx);
        });
    }

    /// Send `text` (with the attachments waiting) to the agent; mid-turn it goes at once (the
    /// agent queues it). An ended process is started again (resuming the conversation).
    pub fn send(&mut self, text: String, cx: &mut Context<Self>) {
        let attachments = std::mem::take(&mut self.attachments);
        if self.provider.is_none() || (text.trim().is_empty() && attachments.is_empty()) {
            self.attachments = attachments;
            return;
        }
        // What you send answers any guess.
        self.suggestion = None;
        // The box talks to Main: your message shows there (M11ar).
        if self.view != crate::agent_lanes::View::Main {
            self.show_view(crate::agent_lanes::View::Main, cx);
        }
        if self.paused {
            // The Workspace decides (R13): `resume_after_classic`.
            self.attachments = attachments;
            cx.emit(AgentTabEvent::ResumeAsked(text));
            return;
        }
        // Its path pills, as chips under it (batch 3, chore 5).
        let mut attachments = attachments;
        attachments.extend(self.take_sent_pills(&text));
        self.sync_box_atoms(cx);
        // Kept for ↑ in this folder's boxes: what goes to the agent (a message, a note, a
        // command it runs), never a command fuxx answers itself.
        if self.provider.and_then(|p| p.own_command(&text)).is_none() {
            let folder = self.history_folder();
            let write = !self.no_process;
            crate::prompt_history::PromptHistory::record(&folder, &text, write, cx);
        }
        self.recall = None;
        if self.booting || self.ending || self.restart_waits(cx) {
            // Held, shown as waiting under the conversation, until the process is there (or,
            // with a restart waiting for the turn, until it is done: it would lose what was
            // written to the process before it). A live change holds nothing back (R5).
            self.queued.push((text, attachments, None));
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

    /// Something to send with the next message (a command's output, a file, an image), shown
    /// as a chip in the box until it is sent. Only kept: it never sends, and never restarts a
    /// paused tab (R13); an error tab (no agent) has nothing to send it to.
    pub fn attach(&mut self, attachment: Attachment, cx: &mut Context<Self>) {
        if self.provider.is_none() {
            return;
        }
        // A path pill goes into the text (batch 3, chore 5), never a chip.
        if let Attachment::PathPill { path, is_dir, .. } = attachment {
            self.add_path_pills(vec![pills::PillPath { path, is_dir }], false, cx);
            return;
        }
        if !attachable(&self.attachments, &attachment) {
            return;
        }
        if let Attachment::Image {
            name,
            picture,
            inline,
            ..
        } = &attachment
        {
            self.images.insert(name.clone(), picture.clone());
            // A pasted image goes into the text where the caret is, as a pill.
            if *inline {
                let mark = marker(name);
                self.pills.push(mark.clone());
                let atoms = self.box_atoms();
                self.composer.update(cx, |c, cx| {
                    c.set_atoms(atoms, cx);
                    c.insert(&mark, cx);
                });
            }
        }
        self.attachments.push(attachment);
        cx.notify();
    }

    /// An image chip pressed (in the box or a sent message): the image in the viewer, over the
    /// tab, with the keyboard (Esc closes). Not one this tab has: nothing.
    pub fn view_image(&mut self, name: &str, cx: &mut Context<Self>) {
        let Some(picture) = self.images.get(name) else {
            return;
        };
        let image =
            gpui::Image::from_bytes(image_format(picture.media_type), picture.bytes.to_vec());
        self.modal = None;
        self.image_view = Some((name.to_string(), Arc::new(image)));
        self.refocus = Some(Refocus::Modal);
        cx.notify();
    }

    /// Esc, ×, a click on the scrim: the viewer closes, the keyboard back in the box.
    pub fn close_image(&mut self, cx: &mut Context<Self>) {
        if self.image_view.take().is_some() {
            self.refocus = Some(Refocus::Box);
            cx.notify();
        }
    }

    fn close_image_action(&mut self, _: &CloseImage, _: &mut Window, cx: &mut Context<Self>) {
        self.close_image(cx);
    }

    /// `paths` (from `File…`) as file chips: each is sent as its path, for the agent to read.
    pub fn attach_files(&mut self, paths: Vec<PathBuf>, cx: &mut Context<Self>) {
        for path in paths {
            self.attach(Attachment::File(path), cx);
        }
    }

    /// The image files among `paths` (from `Image…`) as image chips, read off the UI thread;
    /// any other file is left out.
    pub fn attach_images(&mut self, paths: Vec<PathBuf>, cx: &mut Context<Self>) {
        for path in paths {
            if let Some(media_type) = media_type_for(&path) {
                let name = file_name(&path);
                self.add_image(name, media_type, ImageFrom::File(path), false, cx);
            }
        }
    }

    /// An image file dropped from Finder (batch 3, chore 5): an image pill in the text, as a
    /// pasted one, read from its file.
    pub(crate) fn add_inline_image(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let Some(media_type) = media_type_for(&path) else {
            return;
        };
        self.pasted += 1;
        let name = pasted_name(self.pasted);
        self.add_image(name, media_type, ImageFrom::File(path), true, cx);
    }

    /// An image chip once its bytes are in: they are read, checked against the provider's
    /// `image_limit` and, for a pasted image with an agent that is told a path, written to the
    /// temp folder, all off the UI thread. Too big (or unreadable): no chip, a notice says so.
    fn add_image(
        &mut self,
        name: String,
        media_type: &'static str,
        from: ImageFrom,
        inline: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(provider) = self.provider else {
            return;
        };
        let by_path = !provider.sends_images();
        let (limit, label) = (provider.image_limit(), provider.info().label);
        let dir = std::env::temp_dir().join(PASTED_DIR);
        // A pasted (or Finder-dropped) one goes into the text as a pill.
        let job = cx.background_spawn(async move {
            let (bytes, path) = match from {
                ImageFrom::File(path) => {
                    let bytes = read_image(&path, &name, limit, label)?;
                    (bytes, Some(path))
                }
                ImageFrom::Clipboard(bytes) => (bytes, None),
                ImageFrom::ClipboardToPng(bytes) => (paste::to_png(&bytes)?, None),
            };
            let picture = picture(&name, media_type, bytes, limit, label)?;
            let path = match path {
                Some(path) => Some(path),
                None if by_path => Some(
                    write_pasted(&dir, &picture, ext_for(media_type))
                        .map_err(|e| format!("Couldn't attach {name}: {e}"))?,
                ),
                None => None,
            };
            Ok(Attachment::Image {
                name,
                picture,
                path,
                inline,
            })
        });
        cx.spawn(async move |this, cx| {
            let done = job.await;
            this.update(cx, |this, cx| match done {
                Ok(image) => this.attach(image, cx),
                Err(note) => this.notice(note, cx),
            })
            .ok();
        })
        .detach();
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
    pub fn shot_attachments(&self) -> &[Attachment] {
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
            Answer::Expired | Answer::Withdrawn => Action::Deny {
                id: id.clone(),
                reason: None,
            },
        };
        self.write(&action);
        self.conversation.answered(&id, answer);
        self.plan_note = false;
        self.after_change(true, cx);
        Ok(())
    }

    /// Stop the turn: an interrupt; the button reads "Stopping…" until the turn ends, and
    /// offers Force stop after `STOP_FORCE_AFTER`. An agent that can't be interrupted
    /// (`ProviderInfo::interrupts`) is offered Force stop at once.
    pub fn stop(&mut self, cx: &mut Context<Self>) {
        if !matches!(self.status(), Status::Working | Status::Waiting) || self.stopping.is_some() {
            return;
        }
        self.stopping = Some(Instant::now());
        self.abandon_edits(cx);
        if !self.provider.is_some_and(|p| p.info().interrupts) {
            self.force = true;
            self.force_timer = None;
            cx.notify();
            return;
        }
        self.write(&Action::Interrupt);
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
    pub(crate) fn stop_pressed(&mut self, cx: &mut Context<Self>) {
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
        self.reasons.end_turn();
        self.conversation.apply(Event::Notice("Stopped".into()));
        self.conversation.apply(Event::TurnDone {
            ok: false,
            message: None,
        });
        // The turn is over: what waited for it starts with the new process, and what you sent
        // meanwhile goes to it.
        let held = !self.waiting.is_empty();
        self.start_with_waiting(true, cx);
        self.spawn_process(resume, cx);
        if held {
            self.flush_queued();
        }
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
                // A list with a command to pick: Return picks it. With none (a complete line
                // that names nothing listed), Return sends as ever.
                if self.slash.is_some() && self.pick_slash(None, cx) {
                    return;
                }
                // A list still loading is a line in progress, not a complete one: Return does not
                // send the half-typed `/xx`. One that could not be read sends as typed (R10).
                if self.slash_list_pending(cx) {
                    return;
                }
                self.submit(text.clone(), composer, cx);
            }
            ComposerEvent::Stop => {
                // The modal open with the box focused anyway: Esc backs out of it, never stops.
                if self.modal.is_some() {
                    self.back_out(cx);
                } else if self.menu.is_some() {
                    self.close_menu(cx);
                } else if self.at.take().is_some() {
                    cx.notify();
                } else if self.slash.take().is_some() {
                    self.slash_dismissed = true;
                    cx.notify();
                } else if self.ask_take_back(cx) {
                    // A question waits: Esc goes back to its card, never stops the agent.
                } else if self.plan_noting(!self.composer.read(cx).text().is_empty()) {
                    // Writing a note for a plan (Keep planning..., or typing): Esc brings its
                    // buttons back and empties the box; it never stops the agent (the review's
                    // Important 1).
                    self.plan_note = false;
                    self.composer
                        .update(cx, |c, cx| c.set_text(String::new(), cx));
                    self.relayout_plan();
                    cx.notify();
                } else if !self.esc_rewind(cx) {
                    self.stop(cx);
                }
            }
            ComposerEvent::AtTyped => self.open_at(cx),
            ComposerEvent::Complete => self.complete_slash(cx),
            ComposerEvent::PastePaths(paths) => self.paste_paths(paths.clone(), cx),
            ComposerEvent::PasteImage(image) => self.attach_pasted_image(image.clone(), cx),
            ComposerEvent::PasteNothing => self.notice(paste::NOTHING_TO_PASTE.into(), cx),
            ComposerEvent::OpenAtom(mark) => {
                // A path pill opens its file (or shows its folder); a pasted image its viewer.
                if let Some(pill) = self.file_pills.get(mark).cloned() {
                    self.open_pill(pill, cx);
                    return;
                }
                let name = mark
                    .trim_start_matches('[')
                    .trim_end_matches(']')
                    .to_string();
                self.view_image(&name, cx);
            }
            ComposerEvent::ContextMenu { at, has_sel } => cx.emit(AgentTabEvent::BoxMenu {
                at: *at,
                has_sel: *has_sel,
            }),
            ComposerEvent::BackspaceEmpty => self.backspace_empty(cx),
        }
    }

    /// The box's send (➤), as Enter on the box.
    pub(crate) fn send_pressed(&mut self, cx: &mut Context<Self>) {
        let composer = self.composer.clone();
        let text = composer.read(cx).text().to_string();
        self.submit(text, &composer, cx);
    }

    /// What Enter or ➤ does with the box's `text`: with a permission waiting, empty allows and
    /// text denies with it as the reason; else it is sent. The box is cleared once it is used.
    fn submit(&mut self, text: String, composer: &Entity<Composer>, cx: &mut Context<Self>) {
        // A plan all done shows `6/6 ✓` until your next message (the 2026-11-24 chores).
        let (done, all) = crate::plan_progress::counts(&self.plan);
        if all > 0 && done == all {
            self.plan_done = Some(std::mem::take(&mut self.plan));
        }
        if self.asking() {
            // The agent's question waits first: the box is its Other (batch 3, chore 8).
            self.ask_submit(&text, cx);
            return;
        }
        if self.waiting_plan().is_some() {
            // A plan waits (the 2026-11-25 chores): empty Enter approves it (unless you are
            // writing a note), text keeps it planning with your note.
            self.suggestion = None;
            if let Some(answer) = plan_card::plan_submit(&text, self.plan_noting(false)) {
                self.answer(answer, cx).ok();
                composer.update(cx, |c, cx| c.set_text(String::new(), cx));
            }
            return;
        }
        if self.conversation.pending_permission().is_some() {
            self.suggestion = None;
            let answer = if text.trim().is_empty() {
                Answer::Allowed
            } else {
                Answer::Denied(Some(text.trim().to_string()))
            };
            self.answer(answer, cx).ok();
        } else if let Some(command) = self.provider.and_then(|p| p.own_command(&text)) {
            // What fuxx answers itself opens its menu: nothing is sent.
            self.open_box_command(command, cx);
        } else if let Some(sent) = self.lane_submit(&text, cx) {
            // A lane's box (the lanes chore): a relay, a message about it, Back to Main, or
            // nothing; the box keeps its text unless it went.
            if !sent {
                return;
            }
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

    /// Open `menu` over its chip, in place of any other (Esc closes it).
    pub fn open_menu(&mut self, menu: BoxMenu, cx: &mut Context<Self>) {
        // A hover panel replaced by another menu loses its pointer without a leave: it waits for
        // a fresh rest (the review's finding 8).
        if menu != BoxMenu::Usage {
            self.usage_hover.close();
        }
        if menu != BoxMenu::Clock {
            self.clock_hover.close();
        }
        self.at = None;
        self.slash = None;
        self.picker = None;
        self.menu = Some(menu);
        #[cfg(feature = "ui-shots")]
        self.menu_rows.borrow_mut().clear();
        cx.notify();
    }

    /// Close the box's menu (a click outside it).
    pub(crate) fn close_menu(&mut self, cx: &mut Context<Self>) {
        self.picker = None;
        self.models_older_open = None;
        self.effort_switch = None;
        self.effort_panel_at.set(None);
        // A hover-opened panel closed another way waits for a fresh rest.
        self.usage_hover.close();
        self.clock_hover.close();
        #[cfg(feature = "ui-shots")]
        self.menu_rows.borrow_mut().clear();
        if self.menu.take().is_some() {
            cx.notify();
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
        self.slash_scroll.scroll_to_item(0);
        // The agents and skills come from the catalog: a list not read yet (or that failed) is
        // asked for again, as the `/` list does.
        if let Some(provider) = self.provider
            && !self.no_process
        {
            crate::agent_catalog::load_catalog(provider, self.start.cwd.clone(), cx);
        }
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
        // Typing ends the box's sweep (the designer's spec: it never runs while you type); a
        // redraw of the box (its caret, focus) doesn't (your report, 2026-11-22: going back to
        // the tab, it was ended at once).
        if self.sweep_at.is_some() && self.composer.read(cx).text() != self.sweep_text {
            self.sweep_at = None;
        }
        self.pick_typed(cx);
        self.ask_box_changed(cx);
        let typed = !self.composer.read(cx).text().is_empty();
        if typed != self.box_text_nonempty.get() {
            self.box_text_nonempty.set(typed);
            self.relayout_plan();
        }
        self.lane_box_changed(cx);
        self.slash_follow(cx);
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
                    at.selected = 0;
                }
                self.slash_scroll.scroll_to_item(0);
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
                    this.slash_scroll.scroll_to_item(0);
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    /// The rows of the open `@` list: the ranked files, and the catalog's agents and skills (none
    /// while it isn't read).
    pub(crate) fn at_rows_now(&self, cx: &App) -> Vec<AtRow> {
        self.at
            .as_ref()
            .map_or_else(Vec::new, |at| self.at_rows_from(at, cx))
    }

    /// How many rows of the `@` list a key can pick (its files, agents and skills).
    fn at_pick_count(&self, cx: &App) -> usize {
        self.at_rows_now(cx)
            .iter()
            .filter(|r| !matches!(r, AtRow::Heading(_)))
            .count()
    }

    /// Put the picked row (`index` among the rows that can be picked, else the selected one) in
    /// place of `@query`: a file's path, or `@name` for an agent or a skill. Nothing is sent.
    pub(crate) fn pick_at(&mut self, index: Option<usize>, cx: &mut Context<Self>) {
        let Some(at) = &self.at else {
            return;
        };
        // Resolve the row first: a pick that is not there leaves the list as it is.
        let pick = index.unwrap_or(at.selected);
        let Some(row) = self
            .at_rows_now(cx)
            .into_iter()
            .filter(|r| !matches!(r, AtRow::Heading(_)))
            .nth(pick)
        else {
            return;
        };
        let commands = match self.catalog_lookup(cx) {
            crate::agent_catalog::Lookup::Ready(catalog) => catalog.commands,
            _ => Vec::new(),
        };
        // Agents and skills need no file listing (the hits are empty until it is read).
        let root = self
            .files
            .as_ref()
            .map_or_else(|| self.start.cwd.clone(), |f| f.root.clone());
        let inserted = at_insertion(&row, &at.hits, &commands, &root, &self.start.cwd);
        let (from, end) = (at.at, at.at + 1 + at.query.len());
        self.at = None;
        if let Some(text) = inserted {
            self.composer.update(cx, |c, cx| {
                c.replace(from..end, &text, cx);
            });
        }
        cx.notify();
    }

    /// `at_rows_now` for an `@` list that is no longer `self.at` (the one a pick has taken).
    fn at_rows_from(&self, at: &AtList, cx: &App) -> Vec<AtRow> {
        match self.catalog_lookup(cx) {
            crate::agent_catalog::Lookup::Ready(catalog) => {
                at_rows(&at.hits, &catalog.commands, &at.query)
            }
            _ => at_rows(&at.hits, &[], &at.query),
        }
    }

    /// The `/` list follows the box: it opens on a `/` at its start, filters as the first word
    /// grows or shrinks, and closes when the caret leaves that word (or the box's first word is
    /// no longer a `/` one). Esc keeps it closed until then. It stays out of the way of an open
    /// menu, the `@` list and a permission that waits for its answer.
    fn slash_follow(&mut self, cx: &mut Context<Self>) {
        // A question waits (batch 3, chore 8): a `/` is answer text, so the list stays shut.
        if self.asking() {
            if self.slash.take().is_some() {
                cx.notify();
            }
            return;
        }
        let composer = self.composer.read(cx);
        let query = slash_query(composer.text(), composer.cursor()).map(str::to_string);
        let Some(query) = query else {
            self.slash_dismissed = false;
            if self.slash.take().is_some() {
                cx.notify();
            }
            return;
        };
        let allowed = self.provider.is_some()
            && self.menu.is_none()
            && self.at.is_none()
            && !self.slash_dismissed
            && self.conversation.pending_permission().is_none();
        if !allowed {
            if self.slash.take().is_some() {
                cx.notify();
            }
            return;
        }
        match &mut self.slash {
            Some(list) if list.query == query => {}
            Some(list) => {
                list.query = query;
                list.selected = 0;
                self.slash_scroll.scroll_to_item(0);
                cx.notify();
            }
            None => {
                self.slash = Some(SlashList { query, selected: 0 });
                self.slash_scroll.scroll_to_item(0);
                // As the model menu: a list that could not be read is asked for again.
                if let Some(provider) = self.provider
                    && !self.no_process
                {
                    crate::agent_catalog::load_catalog(provider, self.start.cwd.clone(), cx);
                }
                cx.notify();
            }
        }
    }

    /// The `/` list's commands, in the order they are drawn: the places in the catalog's list.
    pub(crate) fn slash_picks(&self, cx: &App) -> Vec<usize> {
        self.slash_rows_now(cx)
            .into_iter()
            .filter_map(|row| match row {
                SlashRow::Item { ix, .. } => Some(ix),
                SlashRow::Heading(_) => None,
            })
            .collect()
    }

    /// The rows of the open `/` list: none while the catalog isn't read.
    pub(crate) fn slash_rows_now(&self, cx: &App) -> Vec<SlashRow> {
        let Some(list) = &self.slash else {
            return Vec::new();
        };
        match self.catalog_lookup(cx) {
            crate::agent_catalog::Lookup::Ready(catalog) => {
                slash_rows(&catalog.commands, &list.query, SLASH_LIMIT)
            }
            _ => Vec::new(),
        }
    }

    /// Whether the `/` list has a command a key could pick (the keys ↑ ↓ Tab are the list's
    /// only then).
    pub(crate) fn slash_has_picks(&self, cx: &App) -> bool {
        self.slash.is_some() && !self.slash_picks(cx).is_empty()
    }

    /// The command of the `/` list at pick `index` (else the selected one).
    fn slash_command(
        &self,
        index: Option<usize>,
        cx: &App,
    ) -> Option<crate::providers::AgentCommand> {
        let list = self.slash.as_ref()?;
        let picks = self.slash_picks(cx);
        let want = index.unwrap_or(list.selected.min(picks.len().saturating_sub(1)));
        let at = *picks.get(want)?;
        match self.catalog_lookup(cx) {
            crate::agent_catalog::Lookup::Ready(catalog) => catalog.commands.get(at).cloned(),
            _ => None,
        }
    }

    /// Where the first word of the box ends (a byte offset).
    fn first_word_end(&self, cx: &App) -> usize {
        let text = self.composer.read(cx).text();
        text.find(char::is_whitespace).unwrap_or(text.len())
    }

    /// Return or a click on a command of the `/` list: `/name ` goes in place of the first word
    /// (nothing is sent). A command fuxx answers with its own menu opens that menu instead,
    /// and the box is cleared. False: there was nothing to pick.
    pub(crate) fn pick_slash(&mut self, index: Option<usize>, cx: &mut Context<Self>) -> bool {
        let Some(command) = self.slash_command(index, cx) else {
            return false;
        };
        let typed = format!("/{}", command.name);
        // Its own menu opens only when the command is all the box holds: with more after it
        // (the caret was inside the first word), the name goes in as text and the rest stays.
        let end = self.first_word_end(cx);
        let alone = self.composer.read(cx).text()[end..].trim().is_empty();
        let own = self.provider.and_then(|p| p.own_command(&typed));
        if let Some(own) = own.filter(|_| alone) {
            self.slash = None;
            self.composer
                .update(cx, |c, cx| c.set_text(String::new(), cx));
            self.open_box_command(own, cx);
        } else {
            self.insert_command(&typed, cx);
        }
        true
    }

    /// Tab: the selected command's `/name ` goes in the box, as typed (nothing opens).
    fn complete_slash(&mut self, cx: &mut Context<Self>) {
        if let Some(command) = self.slash_command(None, cx) {
            self.insert_command(&format!("/{}", command.name), cx);
        }
    }

    /// Whether the `/` list is open while its commands still load (`Loading…` in place of its
    /// rows).
    fn slash_list_pending(&self, cx: &App) -> bool {
        self.slash.is_some()
            && matches!(
                self.catalog_lookup(cx),
                crate::agent_catalog::Lookup::Loading
            )
    }

    /// `typed` and a space in place of the box's first word (the space is there already when
    /// the word had text after it); the list closes (the caret is past the word).
    fn insert_command(&mut self, typed: &str, cx: &mut Context<Self>) {
        let end = self.first_word_end(cx);
        let spaced = self.composer.read(cx).text()[end..].starts_with(char::is_whitespace);
        self.slash = None;
        let put = if spaced {
            typed.to_string()
        } else {
            format!("{typed} ")
        };
        self.composer
            .update(cx, |c, cx| c.replace(0..end, &put, cx));
        cx.notify();
    }

    /// What `command` opens: View Usage, the model menu, or the model menu with the running
    /// model's efforts beside it.
    fn open_box_command(&mut self, command: BoxCommand, cx: &mut Context<Self>) {
        match command {
            BoxCommand::Usage => self.open_usage(cx),
            BoxCommand::Model => self.open_model_menu(None, cx),
            BoxCommand::Effort => {
                let efforts_of = self.running_model_place(cx);
                self.open_model_menu(efforts_of, cx);
            }
        }
    }

    fn at_up(&mut self, _: &AtUp, _: &mut Window, cx: &mut Context<Self>) {
        if self.slash.is_some() {
            if let Some(list) = &mut self.slash {
                list.selected = list.selected.saturating_sub(1);
                let selected = list.selected;
                self.scroll_slash_to(selected, cx);
            }
            return;
        }
        if let Some(at) = &mut self.at {
            at.selected = at.selected.saturating_sub(1);
            let selected = at.selected;
            self.scroll_at_to(selected, cx);
        }
    }

    /// What ↑ walks: this tab's own prompts, newest first, then its folder's stored ones
    /// (none until the file is read), without repeats.
    fn history_entries(&self, cx: &App) -> Vec<String> {
        let own: Vec<String> = self
            .conversation
            .items()
            .iter()
            .filter_map(|i| match i {
                Item::You { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect();
        let folder = crate::prompt_history::PromptHistory::prompts(&self.history_folder(), cx);
        let folder: Vec<&str> = folder.iter().map(String::as_str).collect();
        crate::prompt_history::entries(&own, &folder)
    }

    /// The folder prompt history is kept under: the agent's.
    fn history_folder(&self) -> String {
        self.start.cwd.to_string_lossy().into_owned()
    }

    /// The box's text is the recalled prompt still: browsing goes on. Edited, it is over.
    fn recall_current(&mut self, cx: &App) -> Option<&mut crate::prompt_history::Recall> {
        let text = self.composer.read(cx).text();
        if !self.recall.as_ref().is_some_and(|r| r.browsing(text)) {
            self.recall = None;
        }
        self.recall.as_mut()
    }

    /// A recalled prompt in the box. One starting with `/` keeps the `/` list shut (as Esc
    /// would), so ↑ / ↓ go on through the history instead of into the list.
    fn show_recalled(&mut self, text: String, cx: &mut Context<Self>) {
        self.slash_dismissed = true;
        self.composer.update(cx, |c, cx| c.recall_text(text, cx));
    }

    /// ↑ (⌃P) in the box: on its first row, with nothing selected or being composed, the older
    /// prompt, the caret at its end; otherwise the caret moves up as before. The lists over the
    /// box keep ↑ (their binding wins), and a waiting permission keeps the box for its answer.
    pub(crate) fn history_up(&mut self, _: &HistoryUp, _: &mut Window, cx: &mut Context<Self>) {
        // A question waits: ↑ in the empty box goes back to its card (batch 3, chore 8).
        if self.composer.read(cx).text().is_empty() && self.ask_take_back(cx) {
            return;
        }
        let composer = self.composer.read(cx);
        let recalls = composer.on_first_row()
            && !composer.has_selection()
            && !composer.is_composing()
            && !self.at_open()
            && !self.slash_has_picks(cx)
            && self.conversation.pending_permission().is_none()
            && !self.asking();
        if !recalls {
            self.composer.update(cx, |c, cx| c.move_up(cx));
            return;
        }
        let shown = match self.recall_current(cx) {
            Some(recall) => recall.older().map(str::to_string),
            None => {
                let draft = self.composer.read(cx).text().to_string();
                self.recall =
                    crate::prompt_history::Recall::begin(self.history_entries(cx), &draft);
                self.recall.as_ref().map(|r| r.shown().to_string())
            }
        };
        match shown {
            Some(text) => self.show_recalled(text, cx),
            // The oldest already (or nothing to recall): the caret moves as before.
            None => self.composer.update(cx, |c, cx| c.move_up(cx)),
        }
    }

    /// ↓ (⌃N): while browsing, on the last row, the newer prompt, and past the newest the draft
    /// you had; otherwise the caret moves down as before.
    pub(crate) fn history_down(&mut self, _: &HistoryDown, _: &mut Window, cx: &mut Context<Self>) {
        let composer = self.composer.read(cx);
        let walks = composer.on_last_row()
            && !composer.has_selection()
            && !composer.is_composing()
            && !self.at_open()
            && !self.slash_has_picks(cx);
        let next = match self.recall_current(cx) {
            Some(recall) if walks => Some(recall.newer()),
            _ => None,
        };
        match next {
            Some(crate::prompt_history::Newer::Show(text)) => self.show_recalled(text, cx),
            Some(crate::prompt_history::Newer::Draft(draft)) => {
                self.recall = None;
                // Your own words again: a `/` you were typing gets its list back.
                self.slash_dismissed = false;
                self.composer.update(cx, |c, cx| c.recall_text(draft, cx));
            }
            None => self.composer.update(cx, |c, cx| c.move_down(cx)),
        }
    }

    fn at_down(&mut self, _: &AtDown, _: &mut Window, cx: &mut Context<Self>) {
        if self.slash.is_some() {
            let last = self.slash_picks(cx).len().saturating_sub(1);
            if let Some(list) = &mut self.slash {
                list.selected = (list.selected + 1).min(last);
                let selected = list.selected;
                self.scroll_slash_to(selected, cx);
            }
            return;
        }
        let last = self.at_pick_count(cx).saturating_sub(1);
        if let Some(at) = &mut self.at {
            at.selected = (at.selected + 1).min(last);
            let selected = at.selected;
            self.scroll_at_to(selected, cx);
        }
    }

    /// Keep pick `selected` of the `@` list in view (the lists never show together, so they
    /// share a scroll).
    fn scroll_at_to(&mut self, selected: usize, cx: &mut Context<Self>) {
        let row = self
            .at_rows_now(cx)
            .iter()
            .enumerate()
            .filter(|(_, r)| !matches!(r, AtRow::Heading(_)))
            .nth(selected)
            .map(|(i, _)| i);
        if let Some(row) = row {
            self.slash_scroll.scroll_to_item(row);
        }
        cx.notify();
    }

    /// Keep pick `selected` of the `/` list in view (and drawn as selected).
    fn scroll_slash_to(&mut self, selected: usize, cx: &mut Context<Self>) {
        let row = self
            .slash_rows_now(cx)
            .iter()
            .enumerate()
            .filter(|(_, r)| matches!(r, SlashRow::Item { .. }))
            .nth(selected)
            .map(|(i, _)| i);
        if let Some(row) = row {
            self.slash_scroll.scroll_to_item(row);
        }
        cx.notify();
    }

    /// A tool row's click (spec A §B): its detail in the modal, which takes the keyboard.
    fn open_tool(&mut self, id: String, cx: &mut Context<Self>) {
        // A step's row inside the modal: it opens in the modal's place.
        let items = self.conversation.items();
        let parent = crate::agent_tool_modal::index_of(items, &id)
            .and_then(|ix| crate::agent_tool_modal::parent_of(&items[ix]).map(String::from));
        if let (Some(modal), Some(parent)) = (&self.modal, parent)
            && modal.shown() == Some(parent.as_str())
        {
            self.open_step(id, cx);
            return;
        }
        self.modals_opened += 1;
        self.modal_opened_at = Some(Instant::now());
        self.modal = Some(ToolModal::new(vec![id], self.modals_opened));
        self.modal_scroll = gpui::ScrollHandle::new();
        self.refocus = Some(Refocus::Modal);
        cx.notify();
    }

    /// A step's click inside the modal: shown in its place, Back returns.
    fn open_step(&mut self, id: String, cx: &mut Context<Self>) {
        if let Some(modal) = &mut self.modal {
            modal.stack.push(id);
            modal.switched();
            self.modal_scroll = gpui::ScrollHandle::new();
            cx.notify();
        }
    }

    /// ✕, a click on the scrim: closed, the keyboard back in the box.
    fn close_modal(&mut self, cx: &mut Context<Self>) {
        if self.modal.take().is_some() {
            self.refocus = Some(Refocus::Box);
            cx.notify();
        }
    }

    /// Esc: back out of a step, else close.
    fn back_out(&mut self, cx: &mut Context<Self>) {
        match &mut self.modal {
            Some(modal) if modal.stack.len() > 1 => {
                modal.stack.pop();
                modal.switched();
                self.modal_scroll = gpui::ScrollHandle::new();
                cx.notify();
            }
            _ => self.close_modal(cx),
        }
    }

    /// ↑/↓: the level's previous or next tool, none past the ends.
    fn walk(&mut self, dir: i8, cx: &mut Context<Self>) {
        let Some(modal) = &self.modal else {
            return;
        };
        if let Some(to) = crate::agent_tool_modal::step(self.conversation.items(), modal, dir)
            && let Some(modal) = &mut self.modal
        {
            if let Some(last) = modal.stack.last_mut() {
                *last = to;
            }
            modal.switched();
            self.modal_scroll = gpui::ScrollHandle::new();
            cx.notify();
        }
    }

    fn modal_back(&mut self, _: &ModalBack, _: &mut Window, cx: &mut Context<Self>) {
        self.back_out(cx);
    }

    fn modal_prev(&mut self, _: &ModalPrev, _: &mut Window, cx: &mut Context<Self>) {
        self.walk(-1, cx);
    }

    fn modal_next(&mut self, _: &ModalNext, _: &mut Window, cx: &mut Context<Self>) {
        self.walk(1, cx);
    }

    /// The process runs (a scene tab's "runs" until it ends: it has none).
    fn running(&self) -> bool {
        self.process.is_some() || (self.no_process && !self.exited && !self.paused)
    }

    /// A turn hasn't ended (it works, or waits for your answer).
    fn working(&self) -> bool {
        matches!(self.status(), Status::Working | Status::Waiting)
    }

    /// `id` as a model choice: from the catalog the menus show, else the provider's own list,
    /// else just the id.
    pub(crate) fn model_choice(
        &self,
        provider: &'static dyn Provider,
        id: &str,
        cx: &App,
    ) -> ModelChoice {
        use crate::agent_catalog::{Catalogs, Lookup};
        let listed = cx.try_global::<Catalogs>().and_then(|c| {
            match c.catalog(provider.info().name, &self.start.cwd) {
                Some(Lookup::Ready(catalog)) => catalog.models.iter().find(|m| m.id == id).cloned(),
                _ => None,
            }
        });
        listed
            .or_else(|| provider.fallback_models().into_iter().find(|m| m.id == id))
            .unwrap_or_else(|| ModelChoice {
                id: id.to_string(),
                label: id.to_string(),
                resolves_to: None,
                efforts: Vec::new(),
            })
    }

    /// A model change's start flags (`Provider::model_flags`).
    pub(crate) fn model_flags(
        &self,
        provider: &'static dyn Provider,
        id: &str,
        effort: Option<&str>,
        cx: &App,
    ) -> (String, Option<String>) {
        provider.model_flags(&self.model_choice(provider, id, cx), effort)
    }

    /// What `change` changes from how the agent runs now (empty: nothing). A new model is
    /// `Model`, a new effort `Effort` (both, for a model with a new effort).
    fn kinds(&self, provider: &'static dyn Provider, change: &Change, cx: &App) -> Vec<ChangeKind> {
        match change {
            Change::Model { id, effort } => {
                let (model, effort) = self.model_flags(provider, id, effort.as_deref(), cx);
                let mut kinds = Vec::new();
                if self.start.model.as_deref() != Some(model.as_str()) {
                    kinds.push(ChangeKind::Model);
                }
                if effort.is_some() && self.start.effort != effort {
                    kinds.push(ChangeKind::Effort);
                }
                kinds
            }
            Change::Mode(mode) if self.start.mode.as_deref() != Some(mode.as_str()) => {
                vec![ChangeKind::Mode]
            }
            Change::Folder(folder) if *folder != self.start.cwd => vec![ChangeKind::Folder],
            _ => Vec::new(),
        }
    }

    /// How `kinds` reach the agent: the strictest of the provider's answers (a model and an
    /// effort are live only if both are). A folder is never live: it is where the process runs.
    fn applies(provider: &'static dyn Provider, kinds: &[ChangeKind]) -> Applies {
        let rank = |a: Applies| match a {
            Applies::Live => 0,
            Applies::Restart => 1,
            Applies::NextConversation => 2,
        };
        kinds
            .iter()
            .map(|&k| match provider.applies(k) {
                Applies::Live if k == ChangeKind::Folder => Applies::Restart,
                a => a,
            })
            .max_by_key(|&a| rank(a))
            .unwrap_or(Applies::Live)
    }

    /// Carry out `changes` now, each by its step: asked live, into the start flags (with a
    /// restart on the same conversation for those that need one), or left waiting for the turn.
    fn apply_changes(&mut self, changes: Vec<Change>, cx: &mut Context<Self>) {
        let Some(provider) = self.provider else {
            return;
        };
        let before = self.settings();
        let mut restart = Vec::new();
        for change in changes {
            let kinds = self.kinds(provider, &change, cx);
            if kinds.is_empty() {
                continue;
            }
            let applies = Self::applies(provider, &kinds);
            match step_for(applies, self.running(), self.spoken(), self.working()) {
                Step::Live => self.ask_live(provider, &change, &kinds, cx),
                Step::AfterTurn => self.waiting.push(change),
                Step::NextStart => self.set_start(provider, &change, cx),
                Step::RestartNow => {
                    self.set_start(provider, &change, cx);
                    restart.push(change);
                }
            }
        }
        let moved = self.tell_settings(before, cx);
        if !restart.is_empty() {
            self.restart_with(provider, &restart, cx);
        }
        if moved {
            self.folder_changed(cx);
        }
        cx.notify();
    }

    /// Ask the running agent to take `change` (its `kinds`) in place.
    fn ask_live(
        &mut self,
        provider: &'static dyn Provider,
        change: &Change,
        kinds: &[ChangeKind],
        cx: &App,
    ) {
        match change {
            Change::Model { id, effort } => {
                let (model, effort) = self.model_flags(provider, id, effort.as_deref(), cx);
                if kinds.contains(&ChangeKind::Model) {
                    self.write(&Action::SetModel {
                        model: model.clone(),
                    });
                    self.asked.model = Some(model);
                }
                if let Some(effort) = effort.filter(|_| kinds.contains(&ChangeKind::Effort)) {
                    self.write(&Action::SetEffort {
                        effort: effort.clone(),
                    });
                    self.asked.effort = Some(effort);
                }
            }
            Change::Mode(mode) => {
                self.write(&Action::SetMode { mode: mode.clone() });
                self.asked.mode = Some(mode.clone());
            }
            Change::Folder(_) => {}
        }
    }

    /// `change` into the flags the agent starts with.
    fn set_start(&mut self, provider: &'static dyn Provider, change: &Change, cx: &App) {
        match change {
            Change::Model { id, effort } => {
                let (model, effort) = self.model_flags(provider, id, effort.as_deref(), cx);
                self.start.model = Some(model);
                if effort.is_some() {
                    self.start.effort = effort;
                }
            }
            Change::Mode(mode) => self.start.mode = Some(mode.clone()),
            Change::Folder(folder) => self.start.cwd = folder.clone(),
        }
    }

    /// Start the agent again with what `changes` set, on the same conversation once you have
    /// spoken (one quiet row each), else as it was to start. Not running: its next start takes
    /// them. Queued text is kept (as Force stop's restart).
    fn restart_with(
        &mut self,
        provider: &'static dyn Provider,
        changes: &[Change],
        cx: &mut Context<Self>,
    ) {
        let spoken = self.spoken();
        if spoken {
            for change in changes {
                let notice = self.switched(provider, change, cx);
                self.conversation.apply(Event::Notice(notice));
            }
            self.after_change(true, cx);
        }
        if self.running() {
            let resume = if spoken {
                self.conversation_id()
            } else {
                self.start.resume.clone()
            };
            self.spawn_process(resume, cx);
        }
    }

    /// The row a restart for `change` leaves.
    fn switched(&self, provider: &'static dyn Provider, change: &Change, cx: &App) -> String {
        let model = match change {
            Change::Model { id, .. } => Some(self.model_choice(provider, id, cx).label),
            _ => None,
        };
        let mode = match change {
            Change::Mode(id) => provider
                .info()
                .modes
                .iter()
                .find(|m| m.id == id)
                .map(|m| m.label),
            _ => None,
        };
        let home = crate::workspace::home_dir();
        switched_notice(change, model.as_deref(), mode, home.as_deref())
    }

    /// A change waiting for the turn will restart the agent (not a live change).
    fn restart_waits(&self, cx: &App) -> bool {
        let Some(provider) = self.provider else {
            return false;
        };
        self.waiting.iter().any(|change| {
            let kinds = self.kinds(provider, change, cx);
            !kinds.is_empty() && Self::applies(provider, &kinds) != Applies::Live
        })
    }

    /// The turn is over: what waited for it is carried out, then what you sent meanwhile goes
    /// to the agent as it now is.
    fn settle_waiting(&mut self, cx: &mut Context<Self>) {
        if self.working() {
            return;
        }
        if !self.waiting.is_empty() {
            let changes = std::mem::take(&mut self.waiting);
            self.apply_changes(changes, cx);
        }
        // Also when the change was dropped meanwhile (chosen back): nothing stays held.
        if !self.booting && !self.ending && self.running() && !self.queued.is_empty() {
            self.flush_queued();
            self.after_change(true, cx);
        }
    }

    /// The process is gone (ended, stopped, paused) with changes waiting: its next start takes
    /// them. `notice`: it starts again now, so each leaves its row.
    fn start_with_waiting(&mut self, notice: bool, cx: &mut Context<Self>) {
        let Some(provider) = self.provider else {
            return;
        };
        let before = self.settings();
        for change in std::mem::take(&mut self.waiting) {
            if self.kinds(provider, &change, cx).is_empty() {
                continue;
            }
            self.set_start(provider, &change, cx);
            if notice && self.spoken() {
                let text = self.switched(provider, &change, cx);
                self.conversation.apply(Event::Notice(text));
            }
        }
        if self.tell_settings(before, cx) {
            self.folder_changed(cx);
        }
    }

    fn settings(&self) -> Settings {
        Settings {
            model: self.start.model.clone(),
            effort: self.start.effort.clone(),
            mode: self.start.mode.clone(),
            cwd: self.start.cwd.clone(),
        }
    }

    /// Tell the Workspace what the agent starts with now, if it changed since `before` (saved
    /// with the tab): its flags, and its folder. True when the folder changed: the caller reads
    /// the new folder's lists once its restart (if any) is under way (`read_folder_catalog`).
    fn tell_settings(&mut self, before: Settings, cx: &mut Context<Self>) -> bool {
        let now = self.settings();
        if (&now.model, &now.effort, &now.mode) != (&before.model, &before.effort, &before.mode) {
            cx.emit(AgentTabEvent::Settings {
                model: now.model,
                effort: now.effort,
                mode: now.mode,
            });
        }
        if now.cwd == before.cwd {
            return false;
        }
        // The old folder's list, still waited for from this tab's process, never comes now.
        if std::mem::take(&mut self.catalog_pending)
            && let Some(provider) = self.provider
        {
            crate::agent_catalog::fail_catalog_if_loading(
                provider.info().name,
                &before.cwd,
                "its tab moved to another folder",
                cx,
            );
        }
        cx.emit(AgentTabEvent::Folder(now.cwd));
        true
    }

    /// The agent is in another folder now: its menus' lists and its branch are read again.
    fn folder_changed(&mut self, cx: &mut Context<Self>) {
        self.read_folder_catalog(cx);
        self.branch = None;
        self.read_branch(cx);
    }

    /// Look the folder's branch up again (`git_branch::branch_at`, on the background executor):
    /// at boot, when the folder changes, and when the branch switcher reports a change
    /// (`branch_changed`). An answer for a folder the agent has left is dropped. A scene tab
    /// reads no disk (`shot_place` sets the branch).
    fn read_branch(&mut self, cx: &mut Context<Self>) {
        if self.no_process {
            return;
        }
        let folder = self.start.cwd.clone();
        cx.spawn(async move |this, cx| {
            let at = folder.clone();
            let found = cx
                .background_executor()
                .spawn(async move {
                    let git = crate::git_branch::find_git()?;
                    crate::git_branch::branch_at(&git, &at)
                })
                .await;
            this.update(cx, |this, cx| {
                if this.start.cwd == folder && this.branch != found {
                    this.branch = found;
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    /// Read the branch again soon (the 2026-11-25 chores: the chip went stale after a `git
    /// switch` by the agent or in another terminal): now, unless it was read less than
    /// `BRANCH_REREAD` ago, then once when that has passed.
    fn read_branch_soon(&mut self, cx: &mut Context<Self>) {
        let now = Instant::now();
        let wait = self
            .branch_read_at
            .map(|at| BRANCH_REREAD.saturating_sub(now.saturating_duration_since(at)))
            .unwrap_or_default();
        if wait.is_zero() {
            self.branch_read_at = Some(now);
            self.read_branch(cx);
        } else if !self.branch_read_queued {
            self.branch_read_queued = true;
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(wait).await;
                this.update(cx, |this, cx| {
                    this.branch_read_queued = false;
                    this.branch_read_at = Some(Instant::now());
                    this.read_branch(cx);
                })
                .ok();
            })
            .detach();
        }
    }

    /// The branch switcher switched the repo this agent's folder is in: read its branch again.
    /// Never restarts the agent.
    pub(crate) fn branch_changed(&mut self, cx: &mut Context<Self>) {
        self.read_branch(cx);
    }

    /// The folder the agent runs in.
    pub(crate) fn folder(&self) -> &std::path::Path {
        &self.start.cwd
    }

    /// The menus' lists for the folder the agent is in now (R4): a restart already asked its
    /// own process (the entry says loading); else a probe reads them. A scene tab never probes:
    /// its entry only says the list is being read.
    fn read_folder_catalog(&mut self, cx: &mut Context<Self>) {
        let Some(provider) = self.provider else {
            return;
        };
        let cwd = self.start.cwd.clone();
        if self.no_process {
            cx.update_default_global(|c: &mut crate::agent_catalog::Catalogs, _| {
                c.begin_catalog(provider.info().name, &cwd);
            });
        } else {
            crate::agent_catalog::load_catalog(provider, cwd, cx);
        }
    }

    /// The agent answered a live change: taken, it is a start flag from now on; refused, one
    /// row says why and nothing changes.
    fn applied(&mut self, kind: ChangeKind, value: &str, ok: bool, cx: &mut Context<Self>) {
        let asked = match kind {
            ChangeKind::Model => self.asked.model.take(),
            ChangeKind::Effort => self.asked.effort.take(),
            ChangeKind::Mode => self.asked.mode.take(),
            ChangeKind::Folder => None,
        };
        if !ok {
            self.conversation
                .apply(Event::Notice(format!("Couldn't switch: {value}")));
            return;
        }
        let before = self.settings();
        match kind {
            // The flag as chosen (an alias stays an alias); the agent names the model it is.
            ChangeKind::Model => self.start.model = asked.or(self.start.model.take()),
            ChangeKind::Effort => self.start.effort = Some(value.to_string()),
            ChangeKind::Mode => self.start.mode = Some(value.to_string()),
            ChangeKind::Folder => {}
        }
        self.tell_settings(before, cx);
    }

    /// The agent says the mode it runs in, also one it changed to itself (a plan approved
    /// leaves plan mode, R9). With no change of yours pending (asked, or waiting for the turn)
    /// it is the mode the tab starts with from now on (the chip, the ✓ and the saved tab follow
    /// it); the mode the chip already shows changes nothing.
    fn reported_mode(&mut self, mode: &str, cx: &mut Context<Self>) {
        let pending =
            self.asked.mode.is_some() || self.waiting.iter().any(|c| matches!(c, Change::Mode(_)));
        let Some(provider) = self.provider.filter(|_| !pending) else {
            return;
        };
        let shown = crate::agent_box::current_mode(
            self.start.mode.as_deref(),
            self.catalog(cx).and_then(|c| c.mode.as_deref()),
            provider.info().default_mode,
        );
        if shown == Some(mode) {
            return;
        }
        let before = self.settings();
        self.start.mode = Some(mode.to_string());
        self.tell_settings(before, cx);
    }

    /// The agent refused mode `asked` for this account and runs in `running` (agy,
    /// 2026-11-24): the chip follows `running`, a quiet notice says so, and this Mac's new tabs
    /// of the agent start in `running` from now on (`mode_memory`).
    fn mode_refused(&mut self, asked: &str, running: &str, cx: &mut Context<Self>) {
        let Some(provider) = self.provider else {
            return;
        };
        self.reported_mode(running, cx);
        let info = provider.info();
        let label = |id: &str| {
            info.modes
                .iter()
                .find(|m| m.id == id)
                .map_or_else(|| id.to_string(), |m| m.label.to_string())
        };
        let notice = crate::mode_memory::refused_notice(&label(asked), info.who.1, &label(running));
        self.conversation.apply(Event::Notice(notice));
        crate::mode_memory::remember(cx, info.name, asked, running);
    }

    /// You have sent a message in this tab.
    pub(crate) fn spoken(&self) -> bool {
        self.conversation
            .items()
            .iter()
            .any(|i| matches!(i, Item::You { .. }))
    }

    fn open_classic(&mut self, cx: &mut Context<Self>) {
        if let (Some(provider), Some(id)) = (self.provider, self.conversation_id()) {
            cx.emit(AgentTabEvent::OpenClassic(provider.classic_argv(&id)));
        }
    }

    /// A link in a message (wave 3): the web and mail go where "Open links in" says (the
    /// workspace's `open_link`); a file in the agent's folder (or a whole path) opens in a
    /// file tab, if it is one; anything else does nothing.
    fn follow_link(&mut self, url: &str, cx: &mut Context<Self>) {
        match md_preview::link_target(&self.start.cwd, url) {
            LinkTarget::Web(url) => cx.emit(AgentTabEvent::OpenLink(url)),
            LinkTarget::File(path, _) if path.is_file() => cx.emit(AgentTabEvent::OpenFile(path)),
            _ => {}
        }
    }
}

/// An agent message's blocks.
fn blocks_of(item: &Item) -> Option<Rc<Vec<Md>>> {
    match item {
        // Batch 3, chore 6: its bare addresses are links too.
        Item::Agent { text, .. } => {
            let mut blocks = parse(text);
            link_bare_urls(&mut blocks);
            Some(Rc::new(blocks))
        }
        // A plan to approve (the 2026-11-25 chores): its card renders it.
        Item::Permission {
            plan: Some(plan), ..
        } => {
            let mut blocks = parse(&plan.markdown);
            // Its `#` heading is the card's title already.
            if let Some(at) = blocks.iter().position(|b| matches!(b, Md::Heading(1, _))) {
                blocks.remove(at);
            }
            link_bare_urls(&mut blocks);
            Some(Rc::new(blocks))
        }
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
    /// The provider's logo (spec B-A), ready to draw: the avatar draws it bare, in its own
    /// colours.
    logo_image: Option<Arc<gpui::RenderImage>>,
    draw: md_preview::Draw,
    diff: DiffRowColours,
    view: WeakEntity<AgentTab>,
    now: Instant,
}

/// A small button: `primary` filled with the accent.
pub(crate) fn button(
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
    // Spec B-B: 28 high, 14 in from its sides, r6.
    div()
        .id(id.into())
        .h(px(28.))
        .px(px(14.))
        .flex()
        .items_center()
        .rounded(px(6.))
        .border_1()
        .border_color(hsla(edge))
        .bg(hsla(fill))
        .text_color(hsla(ink))
        .when(primary, |d| d.font_weight(FontWeight::SEMIBOLD))
        .cursor_pointer()
        .child(label.into())
}

/// A chip with an edge (an attachment's).
pub(crate) fn chip(text: impl Into<SharedString>, dim: bool, c: &Colors) -> gpui::Div {
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
    /// The image viewer (your words, 2026-11-22: "I want it to be viewable (modal to view the
    /// image)"): the tool modal's scrim below the top bar, the image fitted in it whole, its name
    /// and × above it. A click on the scrim, ×, or Esc closes it.
    fn image_layer(&self, look: &Look) -> Option<AnyElement> {
        let (name, image) = self.image_view.clone()?;
        let c = look.c;
        let view = look.view.clone();
        let close = view.clone();
        let header = div()
            .w_full()
            .flex()
            .items_center()
            .gap(px(8.))
            .pb(px(8.))
            .text_size(px(text::META))
            .text_color(hsla(c.panel_text))
            .child(div().flex_1().child(SharedString::from(name)))
            .child(
                div()
                    .id("agent-image-close")
                    .size(px(22.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(6.))
                    .cursor_pointer()
                    .text_color(hsla(c.panel_dim))
                    .hover(move |s| s.bg(hsla(c.panel_hover)).text_color(hsla(c.panel_text)))
                    .child("\u{d7}")
                    .on_click(on_tab(&view, |this, cx| this.close_image(cx))),
            );
        let picture = div()
            .flex_1()
            .min_h(px(0.))
            .w_full()
            .flex()
            .items_center()
            .justify_center()
            .child(
                gpui::img(image)
                    .max_w_full()
                    .max_h_full()
                    .object_fit(gpui::ObjectFit::Contain)
                    .rounded(px(6.)),
            );
        let panel = self.marked(
            "image-view",
            div()
                .id("agent-image-panel")
                .w_full()
                .max_w(px(crate::ui_tokens::chat::COLUMN_MAX + 160.))
                .h_full()
                .flex()
                .flex_col()
                .p(px(12.))
                .rounded(px(12.))
                .bg(hsla(c.panel))
                .border_1()
                .border_color(hsla(c.panel_border))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(header)
                .child(picture),
        );
        let scrim = gpui::Hsla {
            a: c.scrim_alpha,
            ..hsla(c.scrim)
        };
        Some(
            div()
                .id("agent-image-view")
                .key_context("AgentImageView")
                .track_focus(&self.modal_focus)
                .absolute()
                .top(px(TOP_BAR))
                .left_0()
                .right_0()
                .bottom_0()
                .rounded_b(px(crate::ui_tokens::size::CARD_RADIUS))
                .p(px(24.))
                .flex()
                .flex_col()
                .items_center()
                .bg(scrim)
                .occlude()
                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                    cx.stop_propagation();
                    close.update(cx, |this, cx| this.close_image(cx)).ok();
                })
                .child(panel)
                .into_any_element(),
        )
    }

    /// The tool modal (spec A §B): a scrim over the tab below its top bar (a click on it closes),
    /// and the shown tool's detail — its header, then its command, changes, output and steps,
    /// in one scroll. It holds the keyboard (key context `AgentToolModal`: Esc, ↑, ↓).
    fn modal_layer(&mut self, look: &Rc<Look>, cx: &mut Context<Self>) -> AnyElement {
        use crate::agent_tool_modal::{
            Right, index_of, modal_width, parent_of as parent_id, right_of,
        };
        let c = look.c;
        let items = self.conversation.items();
        let Some(modal) = self.modal.as_mut() else {
            return div().into_any_element();
        };
        let Some(ix) = modal.shown().and_then(|id| index_of(items, id)) else {
            return div().into_any_element();
        };
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
        } = &items[ix]
        else {
            return div().into_any_element();
        };
        modal.saw(*running);
        let (opened, finished, in_step) = (modal.opened, modal.finished, modal.stack.len() > 1);
        // Its open animation plays when it opens, not each time the tab is shown again.
        let fresh =
            crate::agent_top::animating(self.modal_opened_at, look.now, Duration::from_millis(180));
        // Output growing keeps the body at its bottom while it follows (spec A §B).
        let (offset, max) = (
            f32::from(self.modal_scroll.offset().y),
            f32::from(self.modal_scroll.max_offset().y),
        );
        let laid_out = self.modal_scroll.bounds().size.height > px(0.);
        if modal.measured(laid_out, offset, max) {
            self.modal_scroll.scroll_to_bottom();
        }
        let parent_name = parent_id(&items[ix])
            .and_then(|p| index_of(items, p))
            .and_then(|p| match &items[p] {
                Item::Tool { name, .. } => Some(name.clone()),
                _ => None,
            });
        let diff = match detail {
            Some(ToolDetail::Diff { unified, .. }) => Some(parse_diff(unified)),
            _ => None,
        };
        let counts = diff.as_ref().map(|d| {
            let n = |k| d.rows.iter().filter(|r| r.kind == k).count();
            (n(RowKind::Added), n(RowKind::Removed))
        });
        let elapsed = self.tool_times.get(id).map_or(String::new(), |(t, _)| {
            crate::agent_lanes::clock(look.now.saturating_duration_since(*t))
        });
        let children: Vec<usize> = items
            .iter()
            .enumerate()
            .filter(|(_, i)| parent_id(i) == Some(id.as_str()))
            .map(|(i, _)| i)
            .collect();
        let view = look.view.clone();

        // The header's mark: running, the Working arc; done, the muted check (popping once if
        // it finished while open); failed, the octagon.
        let mark: AnyElement = if *running {
            self.modal_mark
                .update(cx, |m, _| m.set(Status::Working, px(MARK), c.panel));
            self.modal_mark.clone().into_any_element()
        } else if *ok == Some(false) {
            icon(Icon::Octagon, px(MARK), hsla(c.failed_panel)).into_any_element()
        } else {
            let ink = c.panel_dim;
            let slot = div()
                .flex_none()
                .size(px(MARK))
                .flex()
                .items_center()
                .justify_center();
            if finished > 0 {
                slot.with_animation(
                    ("agent-modal-done", finished),
                    Animation::new(status_mark::POP),
                    move |slot, t| {
                        slot.child(status_mark::done_check(
                            ink,
                            px(MARK * status_mark::pop_curve(t)),
                        ))
                    },
                )
                .into_any_element()
            } else {
                slot.child(status_mark::done_check(ink, px(MARK)))
                    .into_any_element()
            }
        };
        let right: AnyElement = match right_of(*running, *ok, result, counts, &elapsed) {
            Right::Elapsed(t) | Right::Plain(t) => div()
                .text_color(hsla(c.panel_dim))
                .child(SharedString::from(t))
                .into_any_element(),
            Right::Failed(t) => div()
                .text_color(hsla(c.failed_panel))
                .child(SharedString::from(t))
                .into_any_element(),
            Right::Diff(a, r) => div()
                .flex()
                .gap(px(6.))
                .child(
                    div()
                        .text_color(hsla(look.diff.added_ink))
                        .child(SharedString::from(format!("+{a}"))),
                )
                .child(
                    div()
                        .text_color(hsla(look.diff.removed_ink))
                        .child(SharedString::from(format!("−{r}"))),
                )
                .into_any_element(),
        };
        let square = |id: &'static str, glyph: Icon, size: f32, ink: Rgb| {
            let (hover_fill, hover_ink) = (hsla(c.panel_hover), hsla(c.panel_text));
            div()
                .id(id)
                .flex_none()
                .size(px(28.))
                .rounded(px(8.))
                .flex()
                .items_center()
                .justify_center()
                .group(id)
                .cursor_pointer()
                .hover(move |s| s.bg(hover_fill))
                .child(
                    icon(glyph, px(size), hsla(ink))
                        .group_hover(id, move |s| s.text_color(hover_ink)),
                )
        };
        let back = in_step.then(|| {
            let view = view.clone();
            square("agent-modal-back", Icon::Back, 14., c.panel_dim).on_click(move |_, _, cx| {
                cx.stop_propagation();
                view.update(cx, |this, cx| this.back_out(cx)).ok();
            })
        });
        let close = {
            let view = view.clone();
            square("agent-modal-close", Icon::Stop, 15., c.panel_dim).on_click(move |_, _, cx| {
                cx.stop_propagation();
                view.update(cx, |this, cx| this.close_modal(cx)).ok();
            })
        };
        let steps = (!children.is_empty()).then(|| format!("{} steps", children.len()));
        let header = div()
            .flex_none()
            .h(px(44.))
            .pl(px(14.))
            .pr(px(8.))
            .flex()
            .items_center()
            .gap(px(10.))
            .border_b_1()
            .border_color(hsla(c.panel_border))
            .text_size(px(text::UI))
            .children(back)
            .child(mark)
            .child(
                div()
                    .flex_none()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(hsla(c.tool_name))
                    .child(SharedString::from(name.clone())),
            )
            // One line: "Task › " (in a step) and the summary, which truncates.
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .flex()
                    .items_center()
                    .text_color(hsla(c.panel_text))
                    .when_some(parent_name.filter(|_| in_step), |d, p| {
                        d.child(
                            div()
                                .flex_none()
                                .whitespace_nowrap()
                                .text_color(hsla(c.panel_dim))
                                .child(SharedString::from(format!("{p} › "))),
                        )
                    })
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .truncate()
                            .child(SharedString::from(summary.clone())),
                    ),
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
                    .max_w(relative(0.35))
                    .truncate()
                    .text_size(px(text::META))
                    .font_features(crate::sidebar::tabular())
                    .child(right),
            )
            .child(close);

        // The sections, each in a labelled well on the page's colour (spec A §B).
        let well = || {
            div()
                .w_full()
                .rounded(px(8.))
                .border_1()
                .border_color(hsla(c.panel_border))
                .bg(hsla(c.page))
                .font_family(look.mono.clone())
                .text_size(px(12.))
                .line_height(relative(1.45))
                .overflow_hidden()
        };
        let section = |label: &'static str, body: AnyElement| {
            div()
                .flex()
                .flex_col()
                .gap(px(6.))
                .child(
                    div()
                        .text_size(px(text::KEYS))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(hsla(c.panel_dim))
                        .child(label),
                )
                .child(body)
                .into_any_element()
        };
        let mut sections: Vec<AnyElement> = Vec::new();
        if let Some(ToolDetail::Command(command)) = detail {
            sections.push(section(
                "Command",
                well()
                    .px(px(12.))
                    .py(px(10.))
                    .flex()
                    .child(div().flex_none().text_color(hsla(c.dim)).child("$ "))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .text_color(hsla(c.text))
                            .child(SharedString::from(command.clone())),
                    )
                    .into_any_element(),
            ));
        }
        if let Some(d) = &diff {
            sections.push(section(
                "Changes",
                well().child(self.diff_block(d, look)).into_any_element(),
            ));
        }
        let shows_output = crate::agent_tool_modal::shows_output(name, output, detail, *ok);
        if shows_output || (*running && output.is_none() && diff.is_none()) {
            let body =
                match output.as_ref().filter(|_| shows_output) {
                    Some(out) => well()
                        .px(px(12.))
                        .py(px(10.))
                        .flex()
                        .flex_col()
                        .when(*dropped_lines > 0, |d| {
                            d.child(div().text_color(hsla(c.dim)).child(SharedString::from(
                                format!("… {dropped_lines} earlier lines"),
                            )))
                        })
                        .child(
                            div()
                                .text_color(hsla(c.text))
                                .child(SharedString::from(out.clone())),
                        ),
                    None => well()
                        .px(px(12.))
                        .py(px(10.))
                        .text_color(hsla(c.dim))
                        .child("No output yet"),
                };
            sections.push(section("Output", body.into_any_element()));
        }
        let body = div()
            .id(("agent-modal-body", opened))
            .flex_1()
            .min_h(px(0.))
            .overflow_y_scroll()
            .track_scroll(&self.modal_scroll)
            .p(px(16.))
            .flex()
            .flex_col()
            .gap(px(12.))
            .children(sections);
        let panel = div()
            .id("agent-modal-panel")
            .w(px(modal_width(self.bar_width.get())))
            .max_h_full()
            .flex()
            .flex_col()
            .rounded(px(12.))
            .bg(hsla(c.panel))
            .border_1()
            .border_color(hsla(c.panel_border))
            .shadow(vec![gpui::BoxShadow {
                color: gpui::Hsla {
                    a: 0.3,
                    ..hsla(c.scrim)
                },
                offset: gpui::point(px(0.), px(8.)),
                blur_radius: px(24.),
                spread_radius: px(0.),
                inset: false,
            }])
            .text_color(hsla(c.panel_text))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(header)
            .child(body);
        let motion = modal_motion(fresh);
        let panel = self.marked("modal", panel);
        let panel = match motion {
            Some((open, _)) => panel
                .with_animation(
                    ("agent-modal-open", opened),
                    Animation::new(open).with_easing(crate::agent_top::ease_out_cubic),
                    |d, t| d.opacity(t).mt(px(8. * (1. - t))),
                )
                .into_any_element(),
            None => panel.into_any_element(),
        };
        let scrim = gpui::Hsla {
            a: c.scrim_alpha,
            ..hsla(c.scrim)
        };
        let close_view = view.clone();
        let scrim_box = div()
            .id("agent-tool-modal")
            .key_context("AgentToolModal")
            .track_focus(&self.modal_focus)
            .absolute()
            .top(px(TOP_BAR))
            .left_0()
            .right_0()
            .bottom_0()
            .rounded_b(px(crate::ui_tokens::size::CARD_RADIUS))
            .pt(px(16.))
            .pb(px(24.))
            .flex()
            .flex_col()
            .items_center()
            // Nothing under the scrim is pressed, hovered or scrolled (the review's finding 1:
            // a click over Stop must close the modal and stop nothing).
            .occlude()
            .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                cx.stop_propagation();
                close_view.update(cx, |this, cx| this.close_modal(cx)).ok();
            })
            .child(panel);
        match motion {
            Some((_, fade)) => scrim_box
                .with_animation(
                    ("agent-modal-scrim", opened),
                    Animation::new(fade).with_easing(crate::agent_top::ease_out_cubic),
                    move |d, t| {
                        d.bg(gpui::Hsla {
                            a: scrim.a * t,
                            ..scrim
                        })
                    },
                )
                .into_any_element(),
            None => scrim_box.bg(scrim).into_any_element(),
        }
    }

    /// Turn the top bar's mark with `clock` (the Workspace's, shared by every mark on screen, so
    /// they all turn together and only the marks redraw on its ticks).
    pub fn use_clock(&mut self, clock: &Entity<MarkClock>, cx: &mut Context<Self>) {
        self.top_mark = cx.new(|cx| MarkView::new(clock, Status::Idle, px(MARK), BLACK, cx));
        self.modal_mark = cx.new(|cx| MarkView::new(clock, Status::Working, px(MARK), BLACK, cx));
        self.thinking_mark =
            cx.new(|cx| MarkView::new(clock, Status::Working, px(MARK), BLACK, cx));
        self.clock = clock.clone();
        self.tool_marks.clear();
        self.shell_marks.clear();
    }

    /// The pane's top bar (spec A §C): the status pill on the left; on the right what the
    /// conversation cost (and what its last turn added) and how full its context is, live.
    /// The tab was just shown (the Workspace tells it: first shown, back from another tab, in a
    /// new split): the box's sweep runs once, unless Settings turns it off (`agent-sweep`).
    pub(crate) fn shown(&mut self, cx: &mut Context<Self>) {
        if !cx.global::<Appearance>().agent_sweep {
            return;
        }
        self.sweep_at = Some(Instant::now());
        self.sweep_text = self.composer.read(cx).text().to_string();
        self.sweeps += 1;
        cx.notify();
    }

    /// How many sweeps the tab has run (UI screenshots).
    #[cfg(feature = "ui-shots")]
    pub fn shot_sweeps(&self) -> u64 {
        self.sweeps
    }

    /// The agent's status in the box's tab, before the folder, as a shell's running command is
    /// (your words, 2026-11-22: "like terminal when doing like npm run dev (in process)"): its
    /// mark and word, no fill (the tab's pills have none), the word's colour blending on a
    /// change; nothing while idle. Reads `self.top`, which `top_line` brought up to date this
    /// frame (it is drawn first).
    pub(crate) fn status_pill(
        &self,
        theme: &Theme,
        c: &Colors,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        use crate::agent_top::{PillMark, animating, ease_out_cubic, pill};
        let (status, stopping) = (self.shown_status(), self.stopping.is_some());
        if status == Status::Idle && !stopping {
            return None;
        }
        let clock_now = Instant::now();
        let top = self.top.clone();
        let on = c.outer;
        let ink = |p: &crate::agent_top::Pill| {
            crate::ui_tokens::ensure_contrast(p.word_ink, on, crate::ui_tokens::TEXT_MIN)
        };
        let now = pill(status, stopping, theme);
        let to_ink = ink(&now);
        // Reduce motion or Animations off: the new ink at once (the lanes review: a still shot
        // caught "Done" mid-blend, in Cultivating's peach).
        let still = cx.reduce_motion() || !cx.global::<Appearance>().animations;
        let from_ink = top
            .pill_from
            .map(|(s, st)| ink(&pill(s, st, theme)))
            .filter(|_| !still && animating(top.pill_at, clock_now, Duration::from_millis(200)));
        let mark: AnyElement = match now.mark {
            PillMark::Status(s) => {
                self.top_mark.update(cx, |m, _| m.set(s, px(MARK), on));
                self.top_mark.clone().into_any_element()
            }
            PillMark::DoneCheck => {
                let check =
                    crate::ui_tokens::ensure_contrast(now.mark_ink, on, crate::ui_tokens::MARK_MIN);
                let slot = div()
                    .flex_none()
                    .size(px(MARK))
                    .flex()
                    .items_center()
                    .justify_center();
                if !still && animating(top.pill_at, clock_now, status_mark::POP) {
                    slot.with_animation(
                        ("agent-pill-done", top.pill_key),
                        Animation::new(status_mark::POP),
                        move |slot, t| {
                            slot.child(status_mark::done_check(
                                check,
                                px(MARK * status_mark::pop_curve(t)),
                            ))
                        },
                    )
                    .into_any_element()
                } else {
                    slot.child(status_mark::done_check(check, px(MARK)))
                        .into_any_element()
                }
            }
        };
        let el = div()
            .id("agent-status")
            .flex()
            .flex_none()
            .items_center()
            .gap(px(5.))
            .h(px(crate::card_view::PILL_H))
            // Its right side flush (the 2026-11-24 chores: "the gap, make it smaller"): the folder
            // chip's own padding is the gap.
            .pl(px(8.))
            .text_size(px(12.))
            .font_weight(FontWeight::MEDIUM)
            .text_color(hsla(to_ink))
            // Its mark alone (the 2026-11-24 chores: the words ran past the tab's edge; they are
            // the chat's status line now, `status_strip`); its word is the tip.
            .child(mark)
            .tooltip({
                let word = self.status_word(theme);
                move |_, cx| {
                    cx.new(|_| crate::sidebar_toggle::Tip(word.clone().into()))
                        .into()
                }
            });
        let el = match from_ink {
            Some(from) => el
                .with_animation(
                    ("agent-pill", top.pill_key),
                    Animation::new(Duration::from_millis(200)).with_easing(ease_out_cubic),
                    move |d, t| d.text_color(hsla(mix(from, to_ink, t))),
                )
                .into_any_element(),
            None => el.into_any_element(),
        };
        Some(
            self.marked("status", div().flex_none().child(el))
                .into_any_element(),
        )
    }

    /// The status pill's word: the status's, or the subagents' while Main has finished and
    /// they haven't.
    fn status_word(&self, theme: &Theme) -> String {
        let stopping = self.stopping.is_some();
        let now = crate::agent_top::pill(self.shown_status(), stopping, theme);
        match (
            self.status(),
            crate::agent_lanes::running_word(self.conversation.lanes()),
        ) {
            (Status::Done | Status::Idle, Some(word)) if !stopping => word,
            _ => now.word.to_string(),
        }
    }

    /// The chat's status line and its counts line (`agent_status_line`), as of now: in Main,
    /// while a turn works or waits on you, and after a turn this tab saw end (until the next).
    pub(crate) fn status_line_now(&self, theme: &Theme) -> Option<(String, Option<String>)> {
        use crate::agent_status_line::{LineState, status_line};
        let status = self.shown_status();
        let live = match status {
            Status::Working | Status::Waiting => true,
            // A turn this tab saw (its ledger), not a conversation only restored.
            Status::Done => self.ledger.is_some(),
            Status::Idle => self.stopping.is_some(),
        };
        if !live || self.view != crate::agent_lanes::View::Main {
            return None;
        }
        let now = Instant::now();
        let parts = self
            .ledger
            .as_ref()
            .map(|l| crate::agent_ledger::parts(l, now))
            .unwrap_or_default();
        let word = self.status_word(theme);
        let question = self.question();
        Some(status_line(&LineState {
            word: &word,
            status,
            question: question.as_deref(),
            task: crate::plan_progress::building(&self.plan),
            running: self.ledger.as_ref().and_then(|l| l.running.as_ref()),
            parts: &parts,
            took: self.top.took,
            now,
        }))
    }

    /// The status line over the box (the 2026-11-24 chores: "change the Cultivating loading on
    /// the chat to be the status states"): its mark in the avatar column, the line in the text
    /// column, and under it the dim counts that open the ledger's popover. Always in view (not
    /// a row of the chat, so a scrolled chat still shows it).
    fn status_strip(&self, c: &Colors, cx: &mut Context<Self>) -> Option<AnyElement> {
        use crate::ui_tokens::chat;
        let theme = cx.global::<Appearance>().theme;
        let (main, counts) = self.status_line_now(&theme)?;
        let mark: AnyElement = match self.shown_status() {
            Status::Waiting => div()
                .size(px(6.))
                .rounded_full()
                .bg(hsla(c.ask_edge))
                .into_any_element(),
            Status::Done => status_mark::done_check(c.dim, px(12.)).into_any_element(),
            Status::Working | Status::Idle => self.thinking_mark.clone().into_any_element(),
        };
        let failing = self.ledger.as_ref().is_some_and(|l| l.failing.is_some());
        let hover = c.text;
        let counts = counts.map(|text| {
            self.marked(
                "status-counts",
                div()
                    .id("agent-status-counts")
                    .truncate()
                    .text_color(hsla(if failing { c.ledger_failing } else { c.dim }))
                    .cursor_pointer()
                    .hover(move |s| s.text_color(hsla(hover)))
                    .on_click(cx.listener(|this, event: &gpui::ClickEvent, _, cx| {
                        let pressed = match event {
                            gpui::ClickEvent::Mouse(click) => Some(click.down.position),
                            _ => None,
                        };
                        // This press already shut the open popover (from outside it).
                        let shut_at = this.chip_closed_at.take();
                        if pressed.is_none() || shut_at != pressed {
                            this.ledger_clicked(cx)
                        }
                    }))
                    .child(SharedString::from(text)),
            )
        });
        let line = div()
            .w_full()
            .max_w(px(chat::COLUMN_MAX))
            .flex()
            .gap(px(chat::AVATAR_GAP))
            .pt(px(6.))
            .pb(px(4.))
            .text_size(px(text::META))
            .child(
                div()
                    .flex_none()
                    .w(px(chat::AVATAR))
                    .h(px(18.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(mark),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .flex()
                    .flex_col()
                    .gap(px(1.))
                    .child(
                        self.marked(
                            "status-line",
                            div()
                                .h(px(18.))
                                .flex()
                                .items_center()
                                .text_color(hsla(c.dim))
                                .child(div().truncate().child(SharedString::from(main))),
                        ),
                    )
                    .children(counts),
            );
        Some(
            div()
                .flex_none()
                .w_full()
                .px(px(crate::agent_box::AREA_X))
                .flex()
                .justify_center()
                .child(line)
                .into_any_element(),
        )
    }

    /// What git and the disk say of the turn's files, read again off the UI thread when the
    /// ledger's paths (changed, scripted, read) differ from those last asked about.
    fn refresh_ledger_facts(&mut self, cx: &mut Context<Self>) {
        let paths: Vec<PathBuf> = self.ledger.as_ref().map_or_else(Vec::new, |l| {
            l.changed
                .iter()
                .map(|c| c.path.clone())
                .chain(l.scripted.iter().cloned())
                .chain(l.read.iter().cloned())
                .collect()
        });
        if paths == self.ledger_facts_for {
            return;
        }
        self.ledger_facts_for = paths.clone();
        self.ledger_facts_gen += 1;
        let generation = self.ledger_facts_gen;
        let root = self.start.cwd.clone();
        let read = cx
            .background_executor()
            .spawn(async move { crate::changed_files::read_facts(&root, &paths) });
        cx.spawn(async move |this, cx| {
            let facts = read.await;
            this.update(cx, |this, cx| {
                if generation == this.ledger_facts_gen {
                    this.ledger_facts = facts;
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    /// The plan's tasks again, when the turn moved on (`plan_stale`): the agent's todo list, else
    /// its checklist file, read with the branch's latest commits off the UI thread.
    fn refresh_plan(&mut self, cx: &mut Context<Self>) {
        if !self.plan_stale {
            return;
        }
        self.plan_stale = false;
        let todos = self.conversation.todos().map(<[_]>::to_vec);
        // The turn's files, then the plan shown (chore 14 of the 2026-11-25 batch).
        let files =
            crate::plan_progress::plan_candidates(&self.plan_turn_files, self.plan_file.as_ref());
        if todos.is_none() && files.is_empty() {
            return;
        }
        self.plan_gen += 1;
        let generation = self.plan_gen;
        let cwd = self.start.cwd.clone();
        let from_file = todos.is_none();
        let read = cx.background_executor().spawn(async move {
            use crate::plan_progress::{commits_since, has_task_headings};
            if !from_file {
                return None;
            }
            // The first file that is a plan (`first_plan`): a note with no tasks is passed over.
            let texts: Vec<(PathBuf, bool, String)> = files
                .into_iter()
                .filter_map(|(p, w)| Some((p.clone(), w, std::fs::read_to_string(&p).ok()?)))
                .collect();
            let pick = crate::plan_progress::first_plan(
                &texts
                    .iter()
                    .map(|(_, w, t)| (*w, t.as_str()))
                    .collect::<Vec<_>>(),
            )?;
            let (path, written, text) = texts.into_iter().nth(pick)?;
            let headed = has_task_headings(&text);
            // Commits name the tasks of a headed plan only, and only those made since it was
            // first there.
            let commits = if headed {
                // From the plan's own repo (chore 15: a plan in another repo than the tab's).
                let repo = path.parent().map_or(cwd.clone(), Path::to_path_buf);
                commits_since(&recent_commits(&repo), plan_since(&repo, &path))
            } else {
                Vec::new()
            };
            Some((text, commits, (path, written)))
        });
        cx.spawn(async move |this, cx| {
            let read = read.await;
            this.update(cx, |this, cx| {
                use crate::plan_progress::{from_checklist, from_todos, match_commits};
                if generation != this.plan_gen {
                    return;
                }
                let tasks = match (&todos, read) {
                    (Some(list), _) => from_todos(list),
                    (None, Some((md, commits, file))) => {
                        // The plan shown from now on, until a turn's own file is one.
                        this.plan_file = Some(file);
                        let mut tasks = from_checklist(&md);
                        // Commits saying the task's planned message count too (chore 15).
                        crate::plan_progress::match_planned(
                            &mut tasks,
                            &commits,
                            &crate::plan_progress::planned_subjects(&md),
                        );
                        match_commits(&mut tasks, &commits);
                        tasks
                    }
                    (None, None) => Vec::new(),
                };
                // A finished plan stays hidden after your next message, until its list changes.
                let tasks = crate::plan_progress::after_done(&mut this.plan_done, tasks);
                if tasks != this.plan {
                    this.plan = tasks;
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    /// ⌥⌘P and the burger tab's click: the plan's tasks open, or shut.
    pub(crate) fn toggle_plan(&mut self, _: &TogglePlan, _: &mut Window, cx: &mut Context<Self>) {
        if matches!(self.menu, Some(crate::agent_box::BoxMenu::Plan)) {
            self.close_menu(cx);
        } else if crate::plan_progress::shows(&self.plan) {
            self.open_menu(crate::agent_box::BoxMenu::Plan, cx);
        }
    }

    /// The counts line's click: the ledger's popover opens (over the status mark), or shuts.
    fn ledger_clicked(&mut self, cx: &mut Context<Self>) {
        if matches!(self.menu, Some(crate::agent_box::BoxMenu::Ledger)) {
            self.close_menu(cx);
        } else {
            self.open_menu(crate::agent_box::BoxMenu::Ledger, cx);
        }
    }

    /// The shown view's turn ledger (the scenes read it).
    #[cfg(feature = "ui-shots")]
    #[allow(dead_code)] // the scenes read the drawn parts (`BoxTexts.ledger`) instead.
    pub fn shot_ledger(&self) -> Option<crate::agent_ledger::Ledger> {
        self.ledger.clone()
    }

    fn top_line(&mut self) -> AnyElement {
        use crate::agent_top::trim;
        let status = self.shown_status();
        let stopping = self.stopping.is_some();
        let cost = self.conversation.cost_usd();
        let context = self.conversation.context_pct();
        self.top.observe(status, stopping, cost, context);
        let width = self.bar_width.clone();
        let measure = gpui::canvas(
            move |bounds, window, _| {
                let w = f32::from(bounds.size.width);
                if trim(width.replace(w)) != trim(w) {
                    window.refresh();
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full();
        div()
            .relative()
            .flex_none()
            .w_full()
            .h(px(TOP_BAR))
            .px(px(AREA_X))
            .flex()
            .items_center()
            .text_size(px(text::META))
            .font_features(crate::sidebar::tabular())
            .child(measure)
            // Its status moved to the box's tab (2026-11-22), its cost and context behind the
            // box's clock (the 2026-11-24 chores): the bar is the chat's top margin now, and
            // measures the tab's width.
            .into_any_element()
    }

    /// The cost and the context (`+$1.31 $314.65  Context ▬ 69%`), as the clock's panel shows
    /// them (the 2026-11-24 chores; the top bar's before); none with neither known.
    pub(crate) fn cost_context(&self, c: &Colors) -> Option<AnyElement> {
        use crate::agent_top::{
            Band, METER_H, METER_W, Trim, animating, band, ease_out_cubic, meter_fill,
        };
        let clock_now = Instant::now();
        let ms = Duration::from_millis;
        let cost = self.conversation.cost_usd();
        let context = self.conversation.context_pct();
        if cost.is_none() && context.is_none() {
            return None;
        }
        let top = self.top.clone();
        let shown = Trim {
            delta: true,
            label: true,
            meter: true,
        };
        let cost_group = cost.map(|usd| {
            let fresh = animating(top.delta_at, clock_now, ms(200));
            let delta = top.delta.clone().filter(|_| shown.delta).map(|d| {
                let el = div().text_color(hsla(c.dim)).child(SharedString::from(d));
                if fresh {
                    el.with_animation(
                        ("agent-delta", top.delta_key),
                        Animation::new(ms(200)),
                        |d, t| d.opacity(t),
                    )
                    .into_any_element()
                } else {
                    el.into_any_element()
                }
            });
            let appearing = animating(top.cost_shown_at, clock_now, ms(200));
            div()
                .id("agent-cost")
                .flex()
                .items_center()
                .gap(px(6.))
                .children(delta)
                .child(
                    div()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(hsla(c.text))
                        .child(SharedString::from(format!("${usd:.2}"))),
                )
                .tooltip(|_, cx| {
                    cx.new(|_| {
                        crate::sidebar_toggle::Tip(
                            "Conversation cost · updates when each turn ends".into(),
                        )
                    })
                    .into()
                })
                .with_animation(
                    ("agent-cost-in", u64::from(appearing)),
                    Animation::new(ms(if appearing { 200 } else { 1 })),
                    |d, t| d.opacity(t),
                )
        });
        let context_group = context.map(|pct| {
            let (fill, pct_ink) = match band(pct) {
                Band::Ok => (c.meter_ok, c.dim),
                Band::Caution => (c.meter_caution, c.pct_caution),
                Band::Critical => (c.meter_critical, c.pct_critical),
            };
            let to_w = meter_fill(pct, METER_W);
            let from_w = top
                .context_from
                .filter(|_| animating(top.context_at, clock_now, ms(400)))
                .map(|p| meter_fill(p, METER_W));
            let appearing = animating(top.context_shown_at, clock_now, ms(200));
            let bar = div()
                .absolute()
                .left_0()
                .top_0()
                .h(px(METER_H))
                .rounded(px(METER_H / 2.))
                .bg(hsla(fill));
            let bar = match from_w {
                Some(from_w) => bar
                    .with_animation(
                        ("agent-meter", top.context_key),
                        Animation::new(Duration::from_millis(400)).with_easing(ease_out_cubic),
                        move |d, t| d.w(px(from_w + (to_w - from_w) * t)),
                    )
                    .into_any_element(),
                None => bar.w(px(to_w)).into_any_element(),
            };
            let meter = shown.meter.then(|| {
                div()
                    .relative()
                    .flex_none()
                    .w(px(METER_W))
                    .h(px(METER_H))
                    .rounded(px(METER_H / 2.))
                    .bg(hsla(c.meter_track))
                    .child(bar)
            });
            div()
                .id("agent-context")
                .flex()
                .items_center()
                .gap(px(8.))
                .when(shown.label, |d| {
                    d.child(div().text_color(hsla(c.dim)).child("Context"))
                })
                .children(meter)
                .child(
                    div()
                        .min_w(px(32.))
                        .flex()
                        .justify_end()
                        .text_color(hsla(pct_ink))
                        .child(SharedString::from(format!("{pct}%"))),
                )
                .tooltip(|_, cx| {
                    cx.new(|_| {
                        crate::sidebar_toggle::Tip(
                            "Context window used · updates as the agent works".into(),
                        )
                    })
                    .into()
                })
                .with_animation(
                    ("agent-context-in", u64::from(appearing)),
                    Animation::new(ms(if appearing { 200 } else { 1 })),
                    |d, t| d.opacity(t),
                )
        });
        Some(
            div()
                .flex()
                .items_center()
                .text_size(px(text::META))
                .font_features(crate::sidebar::tabular())
                .text_color(hsla(c.dim))
                .children(cost_group)
                .when(cost.is_some() && context.is_some(), |d| {
                    d.child(div().w(px(16.)))
                })
                .children(context_group.map(|g| self.marked("context", g)))
                .into_any_element(),
        )
    }

    /// Messages waiting for the process, under the conversation, and why they wait.
    fn pending(&self, c: &Colors, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.queued.is_empty() {
            return None;
        }
        Some(
            div()
                .flex_none()
                .w_full()
                .px(px(AREA_X))
                .pb(px(10.))
                .flex()
                .flex_col()
                .items_end()
                .gap(px(6.))
                .text_size(px(BODY))
                // Spec B-B: your bubble, quieter (the box's outer fill), waiting to go; × left of
                // it takes it back before it is ever written (batch 3, wave 3).
                .children(self.queued.iter().enumerate().map(|(i, (text, _, _))| {
                    let (dim, ink, hover) = (c.dim, c.text, c.card_hover);
                    div()
                        .max_w(relative(0.78))
                        .flex()
                        .flex_row()
                        .items_end()
                        .gap(px(6.))
                        .child(
                            div()
                                .id(("queued-cancel", i))
                                .flex_none()
                                .size(px(24.))
                                .rounded(px(crate::agent_chat::chat::R_CHIP))
                                .flex()
                                .items_center()
                                .justify_center()
                                .cursor_pointer()
                                .text_size(px(14.))
                                .text_color(hsla(dim))
                                .hover(move |s| s.bg(hsla(hover)).text_color(hsla(ink)))
                                .tooltip(|_, cx| {
                                    cx.new(|_| {
                                        crate::sidebar_toggle::Tip(cancel::CANCEL_TIP.into())
                                    })
                                    .into()
                                })
                                .on_click(
                                    cx.listener(move |this, _, _, cx| this.cancel_held(i, cx)),
                                )
                                .child("\u{d7}"),
                        )
                        .child(
                            div()
                                .min_w_0()
                                .px(px(14.))
                                .py(px(9.))
                                .rounded(px(crate::agent_chat::chat::R_BUBBLE))
                                .rounded_br(px(crate::agent_chat::chat::R_TAIL))
                                .bg(hsla(c.queued_fill))
                                .border_1()
                                .border_color(hsla(c.you_edge))
                                .text_color(hsla(c.queued_text))
                                .child(SharedString::from(text.clone())),
                        )
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

    /// Nothing said yet (spec B-B): the agent's logo, what to do, and where, a little above the
    /// middle.
    fn empty_state(&self, c: &Colors, cx: &App) -> AnyElement {
        let home = crate::workspace::home_dir();
        let place = crate::sidebar::home_relative(&self.start.cwd, home.as_deref());
        let info = self.provider.map(|p| p.info());
        let mark: AnyElement = match info
            .and_then(|i| i.logo)
            .and_then(|logo| crate::logos::image(logo, 40., cx))
        {
            Some(logo) => gpui::img(logo).size(px(40.)).into_any_element(),
            None => {
                let rgb = info.map_or(0x6b6b6b, |i| i.badge_rgb);
                div()
                    .size(px(40.))
                    .rounded(px(10.))
                    .bg(hsla(Rgb {
                        r: (rgb >> 16) as u8,
                        g: (rgb >> 8) as u8,
                        b: rgb as u8,
                    }))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(16.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(gpui::white())
                    .child(info.map_or("?", |i| i.badge))
                    .into_any_element()
            }
        };
        div()
            .flex_1()
            .w_full()
            .pb(px(48.))
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .child(mark)
            .child(
                div()
                    .mt(px(16.))
                    .text_size(px(15.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(hsla(c.text))
                    .child(SharedString::from(format!(
                        "Ask {} to do something",
                        self.label()
                    ))),
            )
            .child(
                div()
                    .mt(px(6.))
                    .font_family(cx.global::<Appearance>().font.family.clone())
                    .text_size(px(12.5))
                    .text_color(hsla(c.dim))
                    .child(SharedString::from(place)),
            )
            .into_any_element()
    }
}

impl Render for AgentTab {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.refresh_plan(cx);
        self.refresh_ledger_facts(cx);
        self.plan_frame.set(self.plan_frame.get() + 1);
        let appearance = cx.global::<Appearance>();
        let theme = appearance.theme;
        let mono = appearance.font.family.clone();
        let c = Colors::of(&theme);
        let narrow = self.bar_width.get() < lanes_view::NARROW;
        if narrow != self.placeholder_narrow {
            self.placeholder_narrow = narrow;
            self.update_placeholder(cx);
        }
        // The chat column crossed `TRACKS_NARROW`: busy steps change between tracks and rows.
        if tracks::fits_tracks(self.bar_width.get()) != self.tracks_fit {
            self.sync_rows();
        }
        let empty = self.conversation.items().is_empty();
        let info = self.provider.map(|p| p.info());
        let view = cx.entity().downgrade();
        let link_view = view.clone();
        // The hover actions and notice links are laid out again (scenes find them by name).
        #[cfg(feature = "ui-shots")]
        self.rw.shot_buttons.borrow_mut().clear();
        // Chat text registers again as it paints (chore after M11ar).
        let select = self.pane().select.clone();
        select.begin_frame();
        // Rows a `/clear` (or a restart) took away: what they showed, and a selection in them, go.
        let rows = self.pane().list.item_count();
        select.forget_from(rows);
        if select
            .sel()
            .is_some_and(|s| s.anchor.key.row >= rows || s.head.key.row >= rows)
        {
            select.set(None);
        }
        let look = Rc::new(Look {
            c,
            mono: mono.clone(),
            label: self.label().to_string().into(),
            badge: info.map_or("?", |i| i.badge).into(),
            badge_rgb: info.map_or(0x6b6b6b, |i| i.badge_rgb),
            logo_image: info
                .and_then(|i| i.logo)
                .and_then(|logo| crate::logos::image(logo, LOGO, cx)),
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
                sizes: md_preview::Sizes::CHAT,
                reader: None,
                select: Some((select.clone(), hsla(theme.ui().selection))),
                copied: Some(self.code_copied.clone()),
            },
            diff: diff_row_colours(&theme, theme.background),
            view: view.clone(),
            now: Instant::now(),
        });
        #[cfg(feature = "ui-shots")]
        let built = self.rows_built.clone();
        let pending = self.pending(&c, cx);
        // The agent's question docked over the chat's bottom while its card is below (batch 3,
        // chore 8): decided before the strip, which makes room for it.
        self.ask_frame(window, cx);
        let dock = if empty {
            None
        } else {
            self.ask_dock(&look, cx)
                .or_else(|| self.plan_bar(&look, cx))
        };
        // Your latest message pinned over the chat while it is scrolled above (chore after
        // M11ar).
        let latest = if empty {
            None
        } else {
            self.latest_layer(&c, window, cx)
        };
        let stream = if empty && self.queued.is_empty() {
            self.empty_state(&c, cx)
        } else if empty {
            div().flex_1().into_any_element()
        } else {
            let look = look.clone();
            let chat_view = cx.entity().downgrade();
            let chat_bounds = self.chat_bounds.clone();
            div()
                .flex_1()
                .min_h(px(0.))
                .w_full()
                .relative()
                .on_mouse_down(MouseButton::Left, cx.listener(Self::chat_press))
                .on_mouse_down(MouseButton::Right, cx.listener(Self::chat_menu))
                .child(
                    list(self.pane().list.clone(), move |ix, _, cx| {
                        #[cfg(feature = "ui-shots")]
                        built.set(built.get() + 1);
                        match view.upgrade() {
                            Some(tab) => tab.read(cx).item_row(ix, &look),
                            None => div().into_any_element(),
                        }
                    })
                    .size_full(),
                )
                // A selection's drag keeps going outside the chat; registered for this frame.
                .child(
                    gpui::canvas(
                        move |b, _, _| chat_bounds.set(Some(b)),
                        move |_, _, window, _| {
                            let v = chat_view.clone();
                            window.on_mouse_event(
                                move |e: &gpui::MouseMoveEvent, phase, window, cx| {
                                    if phase == gpui::DispatchPhase::Bubble {
                                        v.update(cx, |this, cx| this.chat_drag(e, window, cx)).ok();
                                    }
                                },
                            );
                            let v = chat_view.clone();
                            window.on_mouse_event(move |_: &gpui::MouseUpEvent, phase, _, cx| {
                                if phase == gpui::DispatchPhase::Bubble {
                                    v.update(cx, |this, _| this.chat_selecting = false).ok();
                                }
                            });
                        },
                    )
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full(),
                )
                .children(latest)
                .children(dock)
                .into_any_element()
        };
        // A command block dragged over the tab tints it (GPUI's element-local style: nothing
        // else changes during a drag-over); dropped, the Workspace attaches it (Task 11).
        let drop_fill = theme.ui().drop_card;
        // A drag that ended elsewhere (or was given up) leaves no drop state behind.
        if self.drop_over && !cx.has_active_drag() {
            self.drop_over = false;
            self.update_placeholder(cx);
        }
        let dropping = self.dropping(cx);
        // The keyboard moves where a click or an action asked (they have no window).
        match self.refocus.take() {
            Some(Refocus::Modal) => window.focus(&self.modal_focus, cx),
            Some(Refocus::Tracks) => window.focus(&self.tracks_focus, cx),
            // Back to the box only if the modal had the keyboard: a modal closed because its tool
            // went (a restart) while you type elsewhere takes nothing from you.
            Some(Refocus::Box)
                if self.modal_focus.is_focused(window) || self.tracks_focus.is_focused(window) =>
            {
                let to = self.composer.read(cx).focus_handle(cx);
                window.focus(&to, cx);
            }
            Some(Refocus::Box) => {}
            None => {}
        }
        let modal = self.modal.is_some().then(|| self.modal_layer(&look, cx));
        let image_view = self.image_layer(&look);
        let asking = self.ask_keys_on(cx);
        div()
            // The rewind pick's keys win in the box while it is on (batch 3, chore 2); the
            // question card's while it has them (chore 8).
            .key_context(if self.rw.pick.is_some() {
                "AgentTab RewindPick"
            } else if asking {
                "AgentTab AgentAsk"
            } else {
                "AgentTab"
            })
            .on_action(cx.listener(Self::track_prev))
            .on_action(cx.listener(Self::track_next))
            .on_action(cx.listener(Self::track_open))
            .on_action(cx.listener(Self::track_leave))
            .on_action(cx.listener(Self::focus_tracks))
            .on_action(cx.listener(Self::toggle_plan))
            .on_action(cx.listener(Self::ask_up))
            .on_action(cx.listener(Self::ask_down))
            .on_action(cx.listener(Self::ask_prev))
            .on_action(cx.listener(Self::ask_next))
            .on_action(cx.listener(Self::ask_space))
            .on_action(cx.listener(Self::ask_enter))
            .on_action(cx.listener(Self::ask_to_box))
            .on_action(cx.listener(Self::ask_skip))
            .on_action(cx.listener(|this, _: &AskPick1, _, cx| this.ask_pick(1, cx)))
            .on_action(cx.listener(|this, _: &AskPick2, _, cx| this.ask_pick(2, cx)))
            .on_action(cx.listener(|this, _: &AskPick3, _, cx| this.ask_pick(3, cx)))
            .on_action(cx.listener(|this, _: &AskPick4, _, cx| this.ask_pick(4, cx)))
            .on_action(cx.listener(|this, _: &AskPick5, _, cx| this.ask_pick(5, cx)))
            .on_action(cx.listener(|this, _: &AskPick6, _, cx| this.ask_pick(6, cx)))
            .on_action(cx.listener(|this, _: &AskPick7, _, cx| this.ask_pick(7, cx)))
            .on_action(cx.listener(|this, _: &AskPick8, _, cx| this.ask_pick(8, cx)))
            .on_action(cx.listener(|this, _: &AskPick9, _, cx| this.ask_pick(9, cx)))
            .on_action(cx.listener(Self::at_up))
            .on_action(cx.listener(Self::at_down))
            .on_action(cx.listener(Self::modal_back))
            .on_action(cx.listener(Self::modal_prev))
            .on_action(cx.listener(Self::modal_next))
            .on_action(cx.listener(Self::close_image_action))
            .on_action(cx.listener(Self::prev_lane))
            .on_action(cx.listener(Self::next_lane))
            .on_action(cx.listener(Self::cycle_mode))
            .on_action(cx.listener(Self::history_up))
            .on_action(cx.listener(Self::history_down))
            .on_action(cx.listener(Self::pick_up))
            .on_action(cx.listener(Self::pick_down))
            .on_action(cx.listener(Self::pick_rewind))
            .on_action(cx.listener(Self::pick_fork))
            .on_action(cx.listener(Self::pick_copy))
            .on_action(cx.listener(Self::pick_leave))
            .on_action(cx.listener(Self::rewind_last))
            // ⌘C / ⌘A reach here when the box has nothing to copy or select (chore after M11ar).
            .on_action(cx.listener(|this, _: &crate::composer::Copy, _, cx| this.copy_chat(cx)))
            .on_action(cx.listener(|this, _: &crate::terminal_view::Copy, _, cx| {
                match this.composer.read(cx).selected_text() {
                    Some(t) => cx.write_to_clipboard(gpui::ClipboardItem::new_string(t)),
                    None => this.copy_chat(cx),
                }
            }))
            .on_action(
                cx.listener(|this, _: &crate::composer::SelectAll, _, cx| this.select_all_chat(cx)),
            )
            .on_action(cx.listener(|this, _: &crate::SelectAll, _, cx| this.select_all_chat(cx)))
            .size_full()
            .flex()
            .flex_col()
            .bg(hsla(c.page))
            // Its page reaches the card's bottom: rounded there as the terminal is, so nothing
            // square shows past the card's corner (GPUI's clip is a rectangle; M11al §3.4).
            .rounded_b(px(crate::ui_tokens::size::CARD_RADIUS))
            .when(self.provider.is_some(), |d| {
                d.drag_over::<BlockDrag>(move |s, _, _, _| s.bg(hsla(drop_fill)))
                    .on_drop(cx.listener(|_, drag: &BlockDrag, _, cx| {
                        cx.emit(AgentTabEvent::BlockDropped(drag.clone()))
                    }))
                    // Files from the tree or Finder (batch 3, chore 5): pills in the box. While
                    // they are over the page it is tinted, the box ringed.
                    .when(dropping, |d| d.bg(hsla(drop_fill)))
                    .on_drag_move(cx.listener(
                        |this, e: &gpui::DragMoveEvent<pills::DraggedPaths>, _, cx| {
                            this.set_drop_over(e.bounds.contains(&e.event.position), cx)
                        },
                    ))
                    .on_drag_move(cx.listener(
                        |this, e: &gpui::DragMoveEvent<gpui::ExternalPaths>, _, cx| {
                            this.set_drop_over(e.bounds.contains(&e.event.position), cx)
                        },
                    ))
                    .on_drop(cx.listener(|this, drag: &pills::DraggedPaths, window, cx| {
                        this.drop_on_tab(pills::DropPaths::Tree(drag.paths.clone()), window, cx)
                    }))
                    .on_drop(
                        cx.listener(|this, paths: &gpui::ExternalPaths, window, cx| {
                            let paths = paths.paths().to_vec();
                            this.drop_on_tab(pills::DropPaths::External(paths), window, cx)
                        }),
                    )
            })
            .text_color(hsla(c.text))
            .child(self.top_line())
            .children(self.lane_strip(&look, cx))
            .children(self.lane_header(&look, cx))
            .child(
                if crate::agent_top::animating(
                    self.cleared_at,
                    Instant::now(),
                    Duration::from_millis(220),
                ) {
                    // After `/clear` the emptied chat fades in: it reads as cleared, not swapped.
                    div()
                        .flex_1()
                        .min_h(px(0.))
                        .w_full()
                        .flex()
                        .flex_col()
                        .child(stream)
                        .with_animation(
                            ("agent-cleared", self.cleared),
                            Animation::new(Duration::from_millis(220))
                                .with_easing(crate::agent_top::ease_out_cubic),
                            |d, t| d.opacity(t),
                        )
                        .into_any_element()
                } else {
                    stream
                },
            )
            .children(self.status_strip(&c, cx))
            .children(pending)
            // An error tab (no such agent) has nothing to send to.
            .when(self.provider.is_some(), |d| {
                d.child(self.box_area(&c, mono, window, cx))
            })
            .children(modal)
            .children(image_view)
    }
}

/// The `.md` files the turn wrote (newest first), then those it only read (newest first),
/// each once, as absolute paths; `true`: written.
fn plan_files_of(ledger: &crate::agent_ledger::Ledger, cwd: &Path) -> Vec<(PathBuf, bool)> {
    let md = |p: &Path| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("md"));
    let abs = |p: &Path| {
        if p.is_absolute() {
            p.to_path_buf()
        } else {
            cwd.join(p)
        }
    };
    let written = ledger.changed.iter().rev().map(|c| (abs(&c.path), true));
    let read = ledger.read.iter().rev().map(|p| (abs(p), false));
    let mut out: Vec<(PathBuf, bool)> = Vec::new();
    for (p, w) in written.chain(read) {
        if md(&p) && !out.iter().any(|(q, _)| *q == p) {
            out.push((p, w));
        }
    }
    out
}

/// When a plan file was first there, in seconds since the epoch: the commit that added it, else
/// the file's own creation (a new, uncommitted plan). Run off the UI thread.
fn plan_since(cwd: &Path, file: &Path) -> i64 {
    let added = std::process::Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(["log", "--diff-filter=A", "--format=%ct", "--"])
        .arg(file)
        .output()
        .ok()
        .and_then(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .last()
                .and_then(|l| l.trim().parse::<i64>().ok())
        });
    let created = std::fs::metadata(file)
        .and_then(|m| m.created())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64);
    added.into_iter().chain(created).min().unwrap_or(i64::MAX)
}

/// The branch's latest commits (short hash, time and subject), for marking a plan's tasks done;
/// none outside a repo. Run off the UI thread.
fn recent_commits(cwd: &Path) -> Vec<(String, String, i64)> {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(["log", "-n", "50", "--format=%h%x09%ct%x09%s"])
        .output();
    let Ok(out) = out else {
        return Vec::new();
    };
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| {
            let mut parts = l.splitn(3, '\t');
            let hash = parts.next()?;
            let at = parts.next()?.parse::<i64>().ok()?;
            Some((hash.to_string(), parts.next()?.to_string(), at))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- Prompt suggestions (a chore after M11ar) ----

    fn words(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn with_suggestions_adds_the_flags_only_when_on() {
        let argv = words(&["claude", "-p"]);
        let flags = words(&["--prompt-suggestions", "true"]);
        assert_eq!(
            with_suggestions(argv.clone(), &flags, true),
            words(&["claude", "-p", "--prompt-suggestions", "true"])
        );
        assert_eq!(with_suggestions(argv.clone(), &flags, false), argv);
        // An agent with no such flags (agy) gets nothing.
        assert_eq!(with_suggestions(argv.clone(), &[], true), argv);
    }

    /// A preset that names the flag itself wins: nothing is added.
    #[test]
    fn with_suggestions_leaves_a_preset_that_names_the_flag() {
        let flags = words(&["--prompt-suggestions", "true"]);
        let preset = words(&["claude", "-p", "--prompt-suggestions", "false"]);
        assert_eq!(with_suggestions(preset.clone(), &flags, true), preset);
        let joined = words(&["claude", "--prompt-suggestions=false"]);
        assert_eq!(with_suggestions(joined.clone(), &flags, true), joined);
    }

    /// A suggestion is kept only while Main is idle with nothing waiting to go.
    #[test]
    fn keeps_suggestion_only_when_idle() {
        assert!(keeps_suggestion(Status::Done, false));
        assert!(keeps_suggestion(Status::Idle, false));
        assert!(!keeps_suggestion(Status::Working, false), "a turn runs");
        assert!(
            !keeps_suggestion(Status::Waiting, false),
            "a permission waits"
        );
        assert!(!keeps_suggestion(Status::Done, true), "a message is queued");
    }

    #[test]
    fn ghost_for_shows_only_in_the_idle_main_view() {
        let s = Some("go ahead and create it");
        let ghost = |s, main, permission, status| ghost_for(s, main, permission, status);
        assert_eq!(
            ghost(s, true, false, Status::Done).as_deref(),
            Some("go ahead and create it")
        );
        assert_eq!(ghost(None, true, false, Status::Done), None);
        assert_eq!(ghost(s, false, false, Status::Done), None, "a lane view");
        assert_eq!(ghost(s, true, true, Status::Waiting), None, "a permission");
        assert_eq!(ghost(s, true, false, Status::Working), None, "a turn runs");
    }

    #[test]
    fn pasted_images_are_numbered_and_drawn_as_they_are() {
        assert_eq!(pasted_name(1), "Image #1");
        assert_eq!(pasted_name(12), "Image #12");
        assert_eq!(ext_for("image/jpeg"), "jpg", "a pasted JPEG's file for agy");
        assert_eq!(ext_for("image/webp"), "webp");
        assert_eq!(ext_for("image/png"), "png");
        assert!(matches!(image_format("image/gif"), gpui::ImageFormat::Gif));
        assert!(matches!(image_format("image/png"), gpui::ImageFormat::Png));
    }

    /// Your report (2026-11-22: "it first show blinking first then show the modal"): the open
    /// motion runs only while the modal is fresh. After that nothing animates it — a new
    /// animation (its id changed) starts again at 0 and drew the modal see-through for a frame.
    #[test]
    fn the_modal_moves_only_while_fresh() {
        assert_eq!(
            modal_motion(true),
            Some((Duration::from_millis(180), Duration::from_millis(140)))
        );
        assert_eq!(modal_motion(false), None, "settled: drawn as it is");
    }
    use crate::code_ask::CodeLines;
    use crate::providers::Picture;
    use crate::ui_tokens::contrast;

    #[test]
    fn an_agent_pick_inserts_at_name() {
        use crate::agent_box::AtRow;
        use crate::providers::{AgentCommand, CommandKind};
        let file = FileHit {
            rel: "src/a.rs".into(),
            positions: Vec::new(),
            score: 0,
        };
        let named = |name: &str, kind| AgentCommand {
            name: name.into(),
            about: String::new(),
            kind,
            aliases: Vec::new(),
        };
        let commands = vec![
            named("example-skill", CommandKind::Skill),
            named("example-agent", CommandKind::Agent),
        ];
        let root = Path::new("/tmp/fx");
        let insert =
            |row: AtRow| at_insertion(&row, std::slice::from_ref(&file), &commands, root, root);
        assert_eq!(insert(AtRow::File(0)).as_deref(), Some("src/a.rs "));
        let named_row = |ix| AtRow::Named {
            ix,
            positions: Vec::new(),
        };
        assert_eq!(insert(named_row(1)).as_deref(), Some("@example-agent "));
        assert_eq!(insert(named_row(0)).as_deref(), Some("@example-skill "));
        assert_eq!(insert(named_row(9)), None, "no such command");
        assert_eq!(insert(AtRow::Heading("Files")), None);
    }

    #[test]
    fn an_edit_tool_call_emits_edit_with_paths_joined_to_cwd() {
        let mut open = HashSet::new();
        let cwd = Path::new("/tmp/fx");
        let started = Event::ToolStarted {
            id: "t1".into(),
            name: "Write".into(),
            summary: "hello.txt".into(),
            parent: None,
            detail: None,
            writes: vec!["hello.txt".into(), String::new()],
            texts: Vec::new(),
        };
        assert_eq!(
            edit_events(&started, cwd, None, &mut open, &mut Reasons::default()),
            vec![AgentTabEvent::Edit {
                call: "t1".into(),
                paths: vec![PathBuf::from("/tmp/fx/hello.txt")],
            }]
        );
        let other = Event::ToolFinished {
            id: "t0".into(),
            ok: true,
            summary: String::new(),
            output: None,
        };
        assert!(edit_events(&other, cwd, None, &mut open, &mut Reasons::default()).is_empty());
        let finished = Event::ToolFinished {
            id: "t1".into(),
            ok: true,
            summary: String::new(),
            output: None,
        };
        assert_eq!(
            edit_events(&finished, cwd, None, &mut open, &mut Reasons::default()),
            vec![AgentTabEvent::EditDone {
                call: "t1".into(),
                ok: true
            }]
        );
        assert!(open.is_empty());
        // A call that writes nothing says nothing.
        let read = Event::ToolStarted {
            id: "t2".into(),
            name: "Read".into(),
            summary: String::new(),
            parent: None,
            detail: None,
            writes: vec![],
            texts: Vec::new(),
        };
        assert!(edit_events(&read, cwd, None, &mut open, &mut Reasons::default()).is_empty());
    }

    fn said(text: &str, parent: Option<&str>) -> Event {
        Event::MessageDone {
            message: "m".into(),
            text: text.into(),
            parent: parent.map(String::from),
        }
    }

    fn edit_call(id: &str, parent: Option<&str>) -> Event {
        Event::ToolStarted {
            id: id.into(),
            name: "Edit".into(),
            summary: "a.rs".into(),
            parent: parent.map(String::from),
            detail: None,
            writes: vec!["a.rs".into()],
            texts: vec![crate::providers::EditText::Inserted("x".into())],
        }
    }

    fn finish(id: &str, ok: bool) -> Event {
        Event::ToolFinished {
            id: id.into(),
            ok,
            summary: String::new(),
            output: None,
        }
    }

    /// Feeds events through `edit_events` as the tab does; what it told, in order.
    fn told_by(events: &[Event]) -> Vec<AgentTabEvent> {
        let (mut open, mut reasons) = (HashSet::new(), Reasons::default());
        let cwd = Path::new("/tmp/fx");
        events
            .iter()
            .flat_map(|e| edit_events(e, cwd, None, &mut open, &mut reasons))
            .collect()
    }

    fn noted(told: &[AgentTabEvent]) -> Vec<&crate::edit_notes::EditNote> {
        told.iter()
            .filter_map(|t| match t {
                AgentTabEvent::EditNoted { note, .. } => Some(note),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_reason_is_the_last_text_before_the_edit() {
        let told = told_by(&[
            said("reuse an open tab", None),
            edit_call("t1", None),
            finish("t1", true),
        ]);
        let notes = noted(&told);
        assert_eq!(notes.len(), 1);
        assert_eq!(&*notes[0].reason, "reuse an open tab");
        assert_eq!(
            notes[0].texts,
            vec![crate::providers::EditText::Inserted("x".into())]
        );
        assert!(matches!(
            told[told.len() - 2],
            AgentTabEvent::EditDone { .. }
        ));
    }

    #[test]
    fn an_earlier_text_in_the_turn_counts() {
        let bash = Event::ToolStarted {
            id: "b1".into(),
            name: "Bash".into(),
            summary: "ls".into(),
            parent: None,
            detail: None,
            writes: vec![],
            texts: vec![],
        };
        let told = told_by(&[
            said("first look around", None),
            bash,
            finish("b1", true),
            edit_call("t1", None),
            finish("t1", true),
        ]);
        let notes = noted(&told);
        assert_eq!(notes.len(), 1);
        assert_eq!(&*notes[0].reason, "first look around");
    }

    #[test]
    fn an_edit_with_no_text_before_it_in_the_turn_gets_no_note() {
        let done = Event::TurnDone {
            ok: true,
            message: None,
        };
        let told = told_by(&[
            said("last turn's words", None),
            done,
            edit_call("t1", None),
            finish("t1", true),
        ]);
        assert!(noted(&told).is_empty());
        assert!(
            told.iter()
                .any(|t| matches!(t, AgentTabEvent::EditDone { ok: true, .. }))
        );
    }

    #[test]
    fn a_sub_agent_edit_takes_its_own_text() {
        let told = told_by(&[
            said("main words", None),
            said("sub words", Some("t0")),
            edit_call("t1", Some("t0")),
            finish("t1", true),
            edit_call("t2", None),
            finish("t2", true),
        ]);
        let notes = noted(&told);
        assert_eq!(notes.len(), 2);
        assert_eq!(&*notes[0].reason, "sub words");
        assert_eq!(&*notes[1].reason, "main words");
        assert_ne!(notes[0].reason_id, notes[1].reason_id);
    }

    #[test]
    fn a_turn_ended_without_turn_done_leaves_no_reason() {
        // A force stop or a crash ends the turn with no `TurnDone` event: `end_turn` is what
        // they call.
        let (mut open, mut reasons) = (HashSet::new(), Reasons::default());
        let cwd = Path::new("/tmp/fx");
        edit_events(&said("old turn", None), cwd, None, &mut open, &mut reasons);
        reasons.end_turn();
        let told: Vec<_> = [edit_call("t1", None), finish("t1", true)]
            .iter()
            .flat_map(|e| edit_events(e, cwd, None, &mut open, &mut reasons))
            .collect();
        assert!(noted(&told).is_empty());
    }

    #[test]
    fn a_failed_edit_gives_no_note() {
        let told = told_by(&[
            said("why", None),
            edit_call("t1", None),
            finish("t1", false),
        ]);
        assert!(noted(&told).is_empty());
        assert!(
            told.iter()
                .any(|t| matches!(t, AgentTabEvent::EditDone { ok: false, .. }))
        );
    }

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

    /// The 2026-11-25 chores ("branch on outer chatbox is not align"): the branch is read again
    /// after the agent's tool calls finish and at a turn's end, not while words stream.
    #[test]
    fn the_branch_is_read_again_after_tools_and_turns() {
        assert!(rereads_branch(&Event::ToolFinished {
            id: "t".into(),
            ok: true,
            summary: String::new(),
            output: None,
        }));
        assert!(rereads_branch(&Event::TurnDone {
            ok: true,
            message: None,
        }));
        assert!(!rereads_branch(&Event::TextDelta {
            message: "m".into(),
            text: "hi".into(),
            parent: None,
        }));
    }

    /// The 2026-11-25 chores: an answered plan says how it went.
    #[test]
    fn a_plan_answer_says_how_it_went() {
        assert_eq!(
            plan_answer_line(&Answer::AlwaysAllowed),
            "Approved \u{b7} auto-accept edits \u{2713}"
        );
        assert_eq!(plan_answer_line(&Answer::Allowed), "Approved \u{2713}");
        assert_eq!(
            plan_answer_line(&Answer::Denied(Some("split it".into()))),
            "Kept planning: split it"
        );
        assert_eq!(plan_answer_line(&Answer::Denied(None)), "Kept planning");
        assert_eq!(plan_answer_line(&Answer::Expired), "Expired");
    }

    #[test]
    fn words_and_chips() {
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

    static CWD_STR: &str = "/p/fuxx-cli";

    fn code(first: usize, last: usize) -> CodeRef {
        CodeRef {
            path: PathBuf::from("/p/fuxx-cli/src/workspace.rs"),
            real: None,
            lines: CodeLines::Lines { first, last },
            fence: Some("rust".into()),
            body: "if let Some(tab) = pane.find(&path) {\n    return self.show(tab);\n}".into(),
        }
    }

    #[test]
    fn wire_text_gives_code_with_its_path_from_the_agent_folder() {
        let list = [Attachment::Code(code(43, 45))];
        assert_eq!(
            wire_text("why?", &list, false, Path::new(CWD_STR)),
            "why?\n\nsrc/workspace.rs:43-45\n```rust\nif let Some(tab) = pane.find(&path) {\n    return self.show(tab);\n}\n```"
        );
        // A folder that doesn't hold the file gets the whole path.
        assert!(
            wire_text("", &list, false, Path::new("/q"))
                .starts_with("/p/fuxx-cli/src/workspace.rs:43-45\n")
        );
    }

    #[test]
    fn the_same_code_twice_is_one_chip() {
        let a = Attachment::Code(code(43, 45));
        assert!(attachable(&[], &a));
        assert!(!attachable(
            std::slice::from_ref(&a),
            &Attachment::Code(code(43, 45))
        ));
        assert!(attachable(
            std::slice::from_ref(&a),
            &Attachment::Code(code(43, 46))
        ));
        assert!(attachable(
            std::slice::from_ref(&a),
            &Attachment::File(PathBuf::from("/p/fuxx-cli/src/workspace.rs"))
        ));
    }

    #[test]
    fn the_code_chip_says_its_lines_and_tips_its_path() {
        let a = Attachment::Code(code(43, 45));
        assert_eq!(a.label(), "workspace.rs:43\u{2013}45");
        assert_eq!(a.glyph(), "</>");
        assert_eq!(a.tip().as_deref(), Some("/p/fuxx-cli/src/workspace.rs"));
        assert_eq!(Attachment::Text("x".into()).tip(), None);
    }

    fn pasted(n: usize, path: Option<&str>) -> Attachment {
        Attachment::Image {
            name: pasted_name(n),
            picture: Picture::new("image/png", b"x".to_vec()),
            path: path.map(PathBuf::from),
            inline: true,
        }
    }

    #[test]
    fn a_pasted_image_is_its_marker_in_the_text() {
        assert_eq!(marker("Image #1"), "[Image #1]");
        let text = "compare [Image #1] with [Image #2]";
        let names = vec!["Image #2".to_string(), "Image #1".to_string()];
        assert_eq!(
            marker_ranges(text, &names),
            vec![
                (8..18, "Image #1".to_string()),
                (24..34, "Image #2".to_string())
            ]
        );
        // A pill deleted from the text: that image isn't sent; a file's chip still is.
        let sent = kept(
            "only [Image #2]",
            vec![pasted(1, None), pasted(2, None), image("a.png", None)],
        );
        let names: Vec<String> = sent.iter().map(Attachment::label).collect();
        assert_eq!(names, ["Image #2", "a.png"]);
        // No chip under your message for it: it's in the text.
        assert_eq!(labels(&sent), ["a.png"]);
        // Claude: the picture carries its marker; agy: the text names its file by it.
        assert_eq!(pictures(&sent)[0].label.as_deref(), Some("[Image #2]"));
        assert_eq!(pictures(&sent)[1].label, None);
        let agy = wire_text(
            "look at [Image #1]",
            &[pasted(1, Some("/tmp/fuxx-images/a.png"))],
            false,
            Path::new("/"),
        );
        assert_eq!(
            agy,
            "look at [Image #1]\n\n[Image #1]: /tmp/fuxx-images/a.png"
        );
    }

    fn image(name: &str, path: Option<&str>) -> Attachment {
        Attachment::Image {
            name: name.into(),
            picture: Picture::new("image/png", b"x".to_vec()),
            path: path.map(PathBuf::from),
            inline: false,
        }
    }

    #[test]
    fn attachments_follow_the_text() {
        let text = |s: &str| Attachment::Text(s.into());
        assert_eq!(wire_text("fix", &[], false, Path::new(CWD_STR)), "fix");
        assert_eq!(
            wire_text("fix", &[text("out")], false, Path::new(CWD_STR)),
            "fix\n\nout"
        );
        assert_eq!(
            wire_text("", &[text("out")], false, Path::new(CWD_STR)),
            "out"
        );
    }

    #[test]
    fn wire_text_puts_paths_after_the_text() {
        let list = [
            Attachment::File(PathBuf::from("/p/notes.txt")),
            image("shot.png", Some("/p/shot.png")),
            Attachment::Text("out".into()),
        ];
        // An agent that takes no image in the message gets its path, as a file's.
        assert_eq!(
            wire_text("look", &list, false, Path::new(CWD_STR)),
            "look\n\n/p/notes.txt\n\n/p/shot.png\n\nout"
        );
        // One that does gets the image apart (`Action::Send`'s images): not in the text.
        assert_eq!(
            wire_text("look", &list, true, Path::new(CWD_STR)),
            "look\n\n/p/notes.txt\n\nout"
        );
        assert_eq!(
            wire_text(
                "",
                &[image("a.png", Some("/p/a.png"))],
                true,
                Path::new(CWD_STR)
            ),
            ""
        );
        // Without a path (nothing wrote one) there is nothing to say.
        assert_eq!(
            wire_text("hi", &[image("a.png", None)], false, Path::new(CWD_STR)),
            "hi"
        );
    }

    /// Batch 3, chore 5: a sent path pill is already in the text (as typed), so the wire adds
    /// nothing for it; the message's row lists it as a chip by its words.
    #[test]
    fn a_sent_path_pill_is_in_the_text_and_listed_as_a_chip() {
        let pill = Attachment::PathPill {
            path: PathBuf::from("/p/my-app/src/main.rs"),
            is_dir: false,
            label: "main.rs".into(),
        };
        assert_eq!(
            wire_text(
                "look at src/main.rs",
                std::slice::from_ref(&pill),
                false,
                Path::new(CWD_STR)
            ),
            "look at src/main.rs"
        );
        assert_eq!(labels(std::slice::from_ref(&pill)), ["main.rs"]);
        assert_eq!(pill.tip().as_deref(), Some("/p/my-app/src/main.rs"));
    }

    #[test]
    fn labels_and_glyphs() {
        assert_eq!(
            Attachment::File(PathBuf::from("/p/notes.txt")).label(),
            "notes.txt"
        );
        assert_eq!(image("shot.png", None).label(), "shot.png");
        assert_eq!(Attachment::Text("a\nb".into()).label(), "a\nb");
        assert_eq!(image("shot.png", None).glyph(), "\u{1f5bc}");
        assert_eq!(Attachment::File(PathBuf::from("/p/a")).glyph(), "\u{1f4ce}");
    }

    /// Batch 3, chore 2's core for wave 3's Add to Chat: a quote's chip says its first line
    /// (cut at 40), its tip where it came from, and it goes as a Markdown block quote under a
    /// line naming its source, after your text.
    #[test]
    fn a_quote_is_labelled_tipped_and_sent_as_a_block_quote() {
        let quote = Attachment::Quote {
            text: "error: the build failed\n\n  at src/main.rs:4".into(),
            from: "Terminal api".into(),
        };
        assert_eq!(quote.label(), "error: the build failed");
        assert_eq!(quote.tip().as_deref(), Some("Quoted from Terminal api"));
        assert_eq!(quote.glyph(), "\u{275d}");
        let long = Attachment::Quote {
            text: "x".repeat(60),
            from: "Claude".into(),
        };
        assert_eq!(long.label(), format!("{}…", "x".repeat(39)));
        assert_eq!(
            wire_text("why?", &[quote], true, Path::new(CWD_STR)),
            "why?\n\nFrom Terminal api:\n> error: the build failed\n>\n>   at src/main.rs:4"
        );
        // It is listed under the message like any chip.
        assert_eq!(labels(&[long]), [format!("{}…", "x".repeat(39))]);
    }

    #[test]
    fn media_types_by_extension() {
        for (name, want) in [
            ("a.png", Some("image/png")),
            ("a.PNG", Some("image/png")),
            ("a.jpg", Some("image/jpeg")),
            ("a.jpeg", Some("image/jpeg")),
            ("a.gif", Some("image/gif")),
            ("a.webp", Some("image/webp")),
            ("a.svg", None),
            ("a.txt", None),
            ("png", None),
            ("a.png.bak", None),
        ] {
            assert_eq!(media_type_for(Path::new(name)), want, "{name}");
        }
    }

    /// 5 MB (MiB): agy's raw limit, and Claude's after base64 (the API's per-image limit).
    const FIVE_MB: usize = 5 * 1024 * 1024;

    #[test]
    fn an_image_told_as_a_path_is_refused_over_5_mb_raw() {
        let raw = ImageLimit::Raw(FIVE_MB);
        let ok = picture("big.png", "image/png", vec![0; FIVE_MB], raw, "agy")
            .expect("exactly 5 MB is taken");
        assert_eq!(ok.bytes.len(), FIVE_MB);
        assert_eq!(
            picture("big.png", "image/png", vec![0; FIVE_MB + 1], raw, "agy"),
            Err("Image over 5 MB: big.png".to_string())
        );
    }

    #[test]
    fn an_image_sent_inline_is_refused_when_its_base64_is_over_5_mb() {
        let encoded = ImageLimit::Encoded(FIVE_MB);
        // 3,932,160 bytes are exactly 5 MB as base64; one more byte is 4 more digits.
        let ok = picture(
            "big.png",
            "image/png",
            vec![0; 3_932_160],
            encoded,
            "Claude",
        )
        .expect("exactly 5 MB once encoded is taken");
        assert_eq!(ok.data.len(), FIVE_MB);
        assert_eq!(
            picture(
                "big.png",
                "image/png",
                vec![0; 3_932_161],
                encoded,
                "Claude"
            ),
            Err("Image too big for Claude (3.75 MB max): big.png".to_string())
        );
    }

    #[test]
    fn an_image_file_is_read_up_to_the_limit_only() {
        let dir = crate::test_temp::dir("fuxx-test-read");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let make = |name: &str, len: usize| {
            let path = dir.join(name);
            std::fs::File::create(&path)
                .unwrap()
                .set_len(len as u64)
                .unwrap();
            path
        };
        let raw = ImageLimit::Raw(FIVE_MB);
        let exact = read_image(&make("a.png", FIVE_MB), "a.png", raw, "agy").expect("exactly 5 MB");
        assert_eq!(exact.len(), FIVE_MB);
        assert_eq!(
            read_image(&make("b.png", FIVE_MB + 1), "b.png", raw, "agy"),
            Err("Image over 5 MB: b.png".to_string())
        );
        let encoded = ImageLimit::Encoded(FIVE_MB);
        let exact = read_image(&make("c.png", 3_932_160), "c.png", encoded, "Claude")
            .expect("exactly 5 MB once encoded");
        assert_eq!(exact.len(), 3_932_160);
        assert_eq!(
            read_image(&make("d.png", 3_932_161), "d.png", encoded, "Claude"),
            Err("Image too big for Claude (3.75 MB max): d.png".to_string())
        );
        assert!(read_image(&dir.join("none.png"), "none.png", raw, "agy").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn old_pasted_images_are_removed_and_new_ones_kept() {
        let dir = crate::test_temp::dir("fuxx-test-clean");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let old = dir.join("old.png");
        let new = dir.join("new.png");
        std::fs::write(&old, b"x").unwrap();
        std::fs::write(&new, b"x").unwrap();
        let day = Duration::from_secs(24 * 60 * 60);
        let now = std::time::SystemTime::now();
        // Judged at a time 25 h on: both are old; at 1 h on: neither is.
        assert_eq!(
            clean_pasted(&dir, day, now + Duration::from_secs(60 * 60)),
            0
        );
        assert!(old.exists() && new.exists());
        std::fs::File::options()
            .write(true)
            .open(&old)
            .unwrap()
            .set_modified(now - Duration::from_secs(25 * 60 * 60))
            .unwrap();
        assert_eq!(clean_pasted(&dir, day, now), 1);
        assert!(!old.exists() && new.exists());
        // A folder that isn't there is nothing to clean.
        assert_eq!(clean_pasted(&dir.join("none"), day, now), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_pasted_image_goes_to_a_new_file_in_the_temp_folder() {
        let dir = crate::test_temp::dir("fuxx-test-paste");
        let _ = std::fs::remove_dir_all(&dir);
        let picture = Picture::new("image/png", b"png bytes".to_vec());
        let a = write_pasted(&dir, &picture, "png").expect("written");
        let b = write_pasted(&dir, &picture, "png").expect("written");
        assert_ne!(a, b, "each paste its own file");
        assert!(a.starts_with(&dir) && a.is_absolute());
        assert_eq!(a.extension().and_then(|e| e.to_str()), Some("png"));
        assert_eq!(std::fs::read(&a).unwrap(), b"png bytes");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_tool_s_print_changes_with_its_activity() {
        let a = Item::Tool {
            id: "t1".into(),
            name: "Read".into(),
            summary: "api.rs".into(),
            parent: None,
            running: false,
            ok: Some(true),
            result: String::new(),
            detail: None,
            output: None,
            dropped_lines: 0,
            activity: None,
        };
        let mut b = a.clone();
        if let Item::Tool { activity, .. } = &mut b {
            *activity = Some(Box::new(crate::agent_activity::Activity {
                derived: "Read api.rs".into(),
                ..Default::default()
            }));
        }
        assert_ne!(fingerprint(&a), fingerprint(&b));
    }

    /// The subagent row chore: the robot's ink stands at least 25 degrees from the accent and
    /// the three status colours, or is the dim fallback.
    #[test]
    fn the_lane_ink_stands_apart_or_falls_back_to_dim() {
        use crate::ui_tokens::hue_of;
        for (name, theme) in crate::theme::every_theme() {
            let c = Colors::of(&theme);
            let ui = theme.ui();
            let p = theme.palette;
            let avoid = [ui.accent, ui.ring_working, ui.ring_waiting, ui.ring_done];
            let floors = [c.panel, c.row_hover, c.ask_fill, c.ask_cursor, c.page];
            match crate::agent_lanes::lane_hue(&[p[4], p[5], p[6]], &avoid) {
                Some(_) => {
                    let h = hue_of(c.lane_ink).expect("a hue");
                    for a in avoid.iter().filter_map(|&a| hue_of(a)) {
                        let d = (h - a).abs() % 360.0;
                        let d = d.min(360.0 - d);
                        // The floor's nudge may move it a degree.
                        assert!(
                            d >= crate::agent_lanes::LANE_HUE_MIN - 1.0,
                            "{name}: {d:.1} degrees"
                        );
                    }
                }
                None => assert_eq!(
                    c.lane_ink,
                    readable(c.panel_dim, &floors, MARK_MIN),
                    "{name}: the fallback"
                ),
            }
        }
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
                ("tool name", c.tool_name, c.panel),
                ("failed", c.failed, c.page),
                ("failed on panel", c.failed_panel, c.panel),
                ("ask head", c.ask_head, c.ask_fill),
                ("ask dim", c.ask_dim, c.ask_fill),
                ("on accent", c.on_accent, c.accent),
                ("code", c.code_text, c.code_fill),
                ("todo active", c.todo_active, c.panel),
                // The composer's controls and pills (the designer's details): their words in
                // `dim` on the control and pill fills.
                ("dim on a control under the pointer", c.dim, c.control_hover),
                (
                    "text on a control under the pointer",
                    c.text,
                    c.control_hover,
                ),
                // The tab's pills have no fill (2026-11-22): their words on the tab.
                ("dim on the tab", c.dim, c.outer),
                ("text on a pill under the pointer", c.text, c.pill_hover),
            ] {
                let got = contrast(fg, on);
                assert!(got >= TEXT_MIN - 0.01, "{name}: {what} {got:.2}");
            }
            // The question card (batch 3, chore 8; the lead designer's tokens): its words on
            // its fill and on the light, its marks, the digit on a filled mark, the answered
            // check on the page and on a hovered line.
            for (what, fg, on, floor) in [
                ("ask head", c.ask_head, c.ask_fill, TEXT_MIN),
                ("ask head lit", c.ask_head, c.ask_cursor, TEXT_MIN),
                ("ask dim lit", c.ask_dim, c.ask_cursor, TEXT_MIN),
                ("ask text", c.ask_text, c.ask_fill, TEXT_MIN),
                ("ask text lit", c.ask_text, c.ask_cursor, TEXT_MIN),
                ("ask mark", c.ask_mark, c.ask_fill, MARK_MIN),
                ("ask mark lit", c.ask_mark, c.ask_cursor, MARK_MIN),
                ("ask mark ink", c.ask_mark_ink, c.ask_mark, TEXT_MIN),
                ("ask key edge", c.ask_key_edge, c.ask_fill, QUIET_LINE_MIN),
                (
                    "ask key edge lit",
                    c.ask_key_edge,
                    c.ask_cursor,
                    QUIET_LINE_MIN,
                ),
                (
                    "ask light",
                    c.ask_cursor,
                    c.ask_fill,
                    crate::ui_tokens::SOFT_LINE_MIN,
                ),
                // The design review: the "Recommended" tag reads as text (Latte's was 3.3:1),
                // and the preview well's code on the well.
                ("ask tag", c.ask_tag, c.ask_fill, TEXT_MIN),
                ("ask tag lit", c.ask_tag, c.ask_cursor, TEXT_MIN),
                ("ask well text", c.ask_well_text, c.ask_well, TEXT_MIN),
                ("ask done", c.ask_done, c.page, MARK_MIN),
                ("ask done hovered", c.ask_done, c.card_hover, MARK_MIN),
                // The lanes chore: the closing bar's words and marks on the box (and a
                // control under the pointer), the relay pill's words on its fill and its fill
                // on the tab.
                ("bar text", c.bar_text, c.box_fill, TEXT_MIN),
                ("bar text hovered", c.bar_text, c.control_hover, TEXT_MIN),
                ("bar dim", c.bar_dim, c.box_fill, TEXT_MIN),
                ("bar dim hovered", c.bar_dim, c.control_hover, TEXT_MIN),
                ("bar failed", c.bar_failed, c.box_fill, TEXT_MIN),
                (
                    "bar failed hovered",
                    c.bar_failed,
                    c.control_hover,
                    TEXT_MIN,
                ),
                ("bar ok", c.bar_ok, c.box_fill, MARK_MIN),
                // The subagent row chore: the robot on a row (at rest, hovered, waiting, lit)
                // and on the page (the strip, a lane's header).
                ("lane ink", c.lane_ink, c.panel, MARK_MIN),
                ("lane ink hovered", c.lane_ink, c.row_hover, MARK_MIN),
                ("lane ink waiting", c.lane_ink, c.ask_fill, MARK_MIN),
                ("lane ink waiting lit", c.lane_ink, c.ask_cursor, MARK_MIN),
                ("lane ink on the page", c.lane_ink, c.page, MARK_MIN),
                // The agent activity chore: an edit's ink (at rest and in a hovered column) and
                // the cursor's ring.
                ("bead edit", c.bead_edit, c.panel, MARK_MIN),
                ("track look", c.track_look, c.panel, MARK_MIN),
                ("bead edit hovered", c.bead_edit, c.row_hover, MARK_MIN),
                ("passed bead hovered", c.tool_ok, c.row_hover, MARK_MIN),
                ("track cursor", c.track_cursor, c.panel, MARK_MIN),
                ("ledger failing", c.ledger_failing, c.outer, TEXT_MIN),
                (
                    "ledger failing on a pill under the pointer",
                    c.ledger_failing,
                    c.pill_hover,
                    TEXT_MIN,
                ),
                // The lanes review, fix 5: "Ask Main about it" has an edge in every theme.
                ("bar edge", c.bar_edge, c.box_fill, QUIET_LINE_MIN),
                ("relay text", c.relay_text, c.relay_fill, TEXT_MIN),
                ("relay dim", c.relay_dim, c.relay_fill, TEXT_MIN),
                (
                    "relay fill",
                    c.relay_fill,
                    c.outer,
                    crate::ui_tokens::SOFT_LINE_MIN,
                ),
            ] {
                let got = contrast(fg, on);
                assert!(got >= floor - 0.01, "{name}: {what} {got:.2}");
            }
            // Stop's circle reddish while the agent works (your words, 2026-11-22: "make the
            // background redish"): it reads apart from its ring, and its square on it.
            assert!(
                c.stop_fill.r > c.stop_fill.g && c.stop_fill.r > c.stop_fill.b,
                "{name}: reddish {:?}",
                c.stop_fill
            );
            let ring = contrast(c.stop_fill, c.outer);
            assert!(
                ring >= crate::ui_tokens::SOFT_LINE_MIN - 0.01,
                "{name}: stop on its ring {ring:.2}"
            );
            for (what, square) in [("text", c.text), ("dim", c.dim)] {
                let got = contrast(square, c.stop_fill);
                assert!(
                    got >= crate::ui_tokens::MARK_MIN - 0.01,
                    "{name}: stop's {what} square {got:.2}"
                );
            }
            // The empty send circle shows on the ring it sits in, its arrow on it (the lead
            // designer's review, 2026-11-22).
            let ring = contrast(c.send_empty, c.outer);
            assert!(
                ring >= crate::ui_tokens::SOFT_LINE_MIN - 0.01,
                "{name}: the empty send on its ring {ring:.2}"
            );
            let arrow = contrast(c.dim, c.send_empty);
            assert!(
                arrow >= crate::ui_tokens::MARK_MIN - 0.01,
                "{name}: the empty send's arrow {arrow:.2}"
            );
            // The chat (spec B-B): your bubble and its chips, a queued one, an error card's text;
            // a failed result on a hovered row.
            for (what, fg, on) in [
                ("you", c.you_text, c.you_fill),
                ("you dim", c.you_dim, c.you_fill),
                ("chip", c.you_dim, c.chip_fill),
                ("queued", c.queued_text, c.queued_fill),
                ("error", c.error_text, c.error_fill),
                ("failed on a hovered row", c.failed_panel, c.row_hover),
                ("dim on a hovered row", c.panel_dim, c.row_hover),
            ] {
                let got = contrast(fg, on);
                assert!(got >= TEXT_MIN - 0.01, "{name}: {what} {got:.2}");
            }
            // Marks: the ok dot on a row and a hovered one, the error's octagon.
            let mark = crate::ui_tokens::MARK_MIN - 0.01;
            for (what, fg, on) in [
                ("ok dot", c.tool_ok, c.panel),
                ("ok dot hovered", c.tool_ok, c.row_hover),
                ("error mark", c.error_mark, c.error_fill),
            ] {
                let got = contrast(fg, on);
                assert!(got >= mark, "{name}: {what} {got:.2}");
            }
            // The send circle (the designer's details): ready, the arrow on the accent (text's
            // floor, "on accent" above) and under the pointer, a mark; empty, a control ("dim on
            // a control" above).
            // Your reference (2026-11-22): send is the accent's circle with its arrow.
            let got = contrast(c.send_ink, c.send_fill);
            assert!(
                got >= crate::ui_tokens::MARK_MIN - 0.01,
                "{name}: send's arrow {got:.2}"
            );
            let got = contrast(c.send_ink, c.send_hover);
            assert!(
                got >= crate::ui_tokens::MARK_MIN - 0.01,
                "{name}: send hover {got:.2}"
            );
            // Raw text_dim, as the brief asks: on panel and on card.
            let ui = theme.ui();
            for (on, bg) in [("panel", ui.panel), ("card", ui.card)] {
                let got = contrast(ui.text_dim, bg);
                assert!(got >= TEXT_MIN - 0.01, "{name}: text_dim on {on} {got:.2}");
            }
        }
    }
}
