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
use crate::agent_conversation::{Answer, Conversation, Item};
use crate::agent_markdown::{Md, parse};
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
actions!(agent_tab, [AtUp, AtDown, ModalBack, ModalPrev, ModalNext]);

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
pub fn resume_failed(started: bool, resume: bool) -> bool {
    resume && !started
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
        }
    }

    /// What hovering its chip says: a code chip's full path.
    pub fn tip(&self) -> Option<String> {
        match self {
            Attachment::Code(code) => Some(code.path.display().to_string()),
            _ => None,
        }
    }

    /// The glyph before its chip's words.
    pub fn glyph(&self) -> &'static str {
        match self {
            Attachment::Image { .. } => "\u{1f5bc}",
            Attachment::Code(_) => "</>",
            _ => "\u{1f4ce}",
        }
    }
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
            Attachment::Image { path, .. } if !images_inline => match path {
                Some(path) => path.to_string_lossy().into_owned(),
                None => continue,
            },
            Attachment::Image { .. } => continue,
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
    attachments.iter().map(Attachment::label).collect()
}

/// The images among `attachments`, for an agent that takes them in the message.
fn pictures(attachments: &[Attachment]) -> Vec<Picture> {
    attachments
        .iter()
        .filter_map(|a| match a {
            Attachment::Image { picture, .. } => Some(picture.clone()),
            _ => None,
        })
        .collect()
}

/// What an item looks like, as a number: a row is laid out again when its number changes.
pub(crate) fn fingerprint(item: &Item) -> u64 {
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
    pub accent: Rgb,
    pub on_accent: Rgb,
    pub todo_active: Rgb,
    /// Code (what a tool runs, its output) on the page.
    pub code_fill: Rgb,
    pub code_text: Rgb,
    /// The agent box you type in (M11ak; the folder tab's inner box since the chores).
    pub box_fill: Rgb,
    /// The folder-tab composer (chores: as the shell tab's): its outer box and edge, its pills
    /// and a pill under the pointer (`page` is behind it: the tab's concave corner).
    pub outer: Rgb,
    pub outer_edge: Rgb,
    pub pill: Rgb,
    pub pill_hover: Rgb,
    /// The box's controls (the designer's details): `+`, the model and mode pills and an idle
    /// send button, a step over the box (6 %), a little more under the pointer (10 %).
    pub control: Rgb,
    pub control_hover: Rgb,
    /// The send circle under the pointer, ready (the designer's details): the accent a step
    /// lighter (dark) or darker. (Empty, it is a control: `control`, its arrow `dim`.)
    pub send_hover: Rgb,
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
}

const WHITE: Rgb = Rgb {
    r: 255,
    g: 255,
    b: 255,
};
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
        let code_fill = mix(page, ui.text, 0.06);
        let row_hover = mix(ui.panel, theme.foreground, 0.04);
        let meter_track = readable(mix(page, theme.foreground, 0.12), &[page], QUIET_LINE_MIN);
        let on_meter = |c: Rgb| readable(c, &[page, meter_track], MARK_MIN);
        let you_fill = ui.composer_inner;
        let chip_fill = mix(you_fill, theme.foreground, 0.06);
        let error_fill = mix(page, ui.block_failed, 0.08);
        Colors {
            page,
            text: fit(ui.text, page),
            dim: fit(ui.text_dim, page),
            panel: ui.panel,
            panel_border: ui.panel_border,
            panel_hover: ui.panel_hover,
            panel_text: readable(ui.text, &[ui.panel, row_hover], TEXT_MIN),
            panel_dim: readable(ui.text_dim, &[ui.panel, row_hover], TEXT_MIN),
            tool_name: readable(ui.accent, &[ui.panel, row_hover], TEXT_MIN),
            failed_panel: readable(ui.block_failed, &[ui.panel, row_hover], TEXT_MIN),
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
            // The shell composer's palette (chores: "more likely in terminal tab"): the box you
            // type in is its inner box, raised over its outer box.
            box_fill: ui.composer_inner,
            outer: ui.composer_outer,
            outer_edge: ui.composer_edge,
            pill: ui.composer_inner,
            pill_hover: ui.composer_pill_hover,
            control: mix(ui.composer_inner, theme.foreground, 0.06),
            control_hover: mix(ui.composer_inner, theme.foreground, 0.10),
            send_hover: mix(ui.accent, if theme.is_dark() { WHITE } else { BLACK }, 0.10),
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
        }
    }
}

/// One item's row as laid out last: its fingerprint, and (an agent message) its blocks, parsed
/// again only when its text changes.
struct Row {
    print: u64,
    blocks: Option<Rc<Vec<Md>>>,
}

/// This run of the process: what it was asked to resume (until the agent says it started it),
/// the id fuxx chose for it (a new conversation: `Provider::fresh_conversation_id`), whether it
/// said it started, where its rows begin, and what was sent during it (sent again if the resume
/// fails).
#[derive(Default)]
struct Run {
    resume: Option<String>,
    fresh: Option<String>,
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
    queued: Vec<(String, Vec<Attachment>)>,
    run: Run,
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
    list: ListState,
    rows: Vec<Row>,
    /// The tool detail modal, open (spec A §B); how many times one opened (its animation's key);
    /// its focus (key context `AgentToolModal`) and body's scroll; the keyboard to move at the
    /// next draw (a click or an action has no window: `render` does it).
    modal: Option<ToolModal>,
    modals_opened: u64,
    /// How many times the conversation was cleared (`/clear`): keys the emptied chat's fade.
    cleared: u64,
    modal_focus: FocusHandle,
    pub(crate) modal_scroll: gpui::ScrollHandle,
    refocus: Option<Refocus>,
    /// The top bar (spec A §C): what its animations start from (`agent_top::TopState`), its
    /// status mark and the clock that turns it (a still one of its own until the Workspace
    /// gives it the shared one, `use_clock`), and its width as last laid out (`agent_top::trim`).
    top: crate::agent_top::TopState,
    top_mark: Entity<MarkView>,
    /// The tool modal's header mark while its tool runs.
    modal_mark: Entity<MarkView>,
    /// The marks' clock (the Workspace's once `use_clock`); the Working arcs of the running
    /// tools' rows (made and dropped in `after_change`, by tool id) and of the "Thinking…" line.
    clock: Entity<MarkClock>,
    tool_marks: std::collections::HashMap<String, Entity<MarkView>>,
    thinking_mark: Entity<MarkView>,
    bar_width: Rc<Cell<f32>>,
    /// When each running tool started (for its elapsed time).
    tool_started: std::collections::HashMap<String, Instant>,
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
    /// The command line of the last start (scenes check a restart resumes).
    #[cfg(feature = "ui-shots")]
    last_argv: Vec<String>,
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
        let list = ListState::new(0, ListAlignment::Top, px(200.));
        list.set_follow_mode(gpui::FollowMode::Tail);
        let subscriptions = vec![
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
            unconfirmed: None,
            no_process: false,
            catalog_pending: false,
            list,
            rows: Vec::new(),
            modal: None,
            modals_opened: 0,
            cleared: 0,
            modal_focus: cx.focus_handle(),
            modal_scroll: gpui::ScrollHandle::new(),
            refocus: None,
            top: crate::agent_top::TopState::default(),
            top_mark,
            modal_mark,
            clock,
            tool_marks: std::collections::HashMap::new(),
            thinking_mark,
            bar_width: Rc::new(Cell::new(f32::MAX)),
            tool_started: std::collections::HashMap::new(),
            open_edits: HashSet::new(),
            reasons: Reasons::default(),
            said: Said::default(),
            stopping: None,
            force: false,
            attachments: Vec::new(),
            menu: None,
            picker: None,
            chip_closed_at: None,
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
            #[cfg(feature = "ui-shots")]
            last_argv: Vec::new(),
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
        for (text, attachments) in std::mem::take(&mut self.queued) {
            self.say(text, attachments);
        }
    }

    /// Show what you said and send it to the running process. The first message of a new
    /// conversation started under an id fuxx chose makes that id the tab's conversation at once
    /// (told, so saved: a process left running by a crash is found holding it, R13).
    fn say(&mut self, text: String, attachments: Vec<Attachment>) {
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
        self.conversation.you(text.clone(), labels(&attachments));
        self.run.sends.push((text, attachments));
        self.write(&Action::Send { text: wire, images });
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
        let resume = resume.filter(|id| self.unconfirmed.as_ref() != Some(id));
        let fresh = match (&resume, self.provider) {
            (None, Some(provider)) => provider.fresh_conversation_id(),
            _ => None,
        };
        self.run = Run {
            resume: resume.clone(),
            fresh: fresh.clone(),
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
            new_id: fresh,
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

    /// The latest change waiting for the turn to end.
    #[allow(dead_code)] // read by scenes
    pub fn waiting_change(&self) -> Option<&Change> {
        self.waiting.last()
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
        if failed_resume && !unknown {
            // It couldn't start (login, network, a crash): the conversation is still there, and
            // the tab keeps its id. One error row says why.
            let sends = std::mem::take(&mut self.run.sends);
            self.conversation.keep_first(self.run.first_item);
            for (text, attachments) in sends {
                self.conversation.you(text, labels(&attachments));
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
                    self.tool_started.insert(id.clone(), Instant::now());
                }
                // `/clear`: the chat empties (the conversation drops its items) and fades back in
                // empty, keyed by how many times it was cleared.
                Event::Cleared => {
                    self.tool_started.clear();
                    self.cleared += 1;
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
                _ => {}
            }
            self.conversation.apply(event);
        }
        if turn_ended {
            self.settle_waiting(cx);
        }
        self.after_change(true, cx);
    }

    /// After the conversation changed: rows, events, placeholder, the elapsed ticker, a redraw.
    fn after_change(&mut self, live: bool, cx: &mut Context<Self>) {
        // A Working arc for each running tool's row (spec B-B), none for the others.
        let running: Vec<String> = self
            .conversation
            .items()
            .iter()
            .filter_map(|i| match i {
                Item::Tool {
                    id, running: true, ..
                } => Some(id.clone()),
                _ => None,
            })
            .collect();
        self.tool_marks.retain(|id, _| running.contains(id));
        for id in running {
            let clock = self.clock.clone();
            self.tool_marks.entry(id).or_insert_with(|| {
                cx.new(|cx| MarkView::new(&clock, Status::Working, px(MARK), BLACK, cx))
            });
        }
        self.sync_rows();
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

    /// Lay out again the rows whose items changed (and a sub-agent's parent row), add the new
    /// ones at the end, drop any gone from it.
    fn sync_rows(&mut self) {
        let items = self.conversation.items();
        // A row's print holds its shape in its tool group and whether it carries the thinking
        // line (spec B-B): a neighbour arriving lays it out again.
        let thinking = crate::agent_chat::shows_thinking(self.status() == Status::Working, items);
        let new: Vec<u64> = (0..items.len())
            .map(|i| crate::agent_chat::row_print(items, i, thinking))
            .collect();
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
            PLACEHOLDER.into()
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
        if self.booting || self.ending || self.restart_waits(cx) {
            // Held, shown as waiting under the conversation, until the process is there (or,
            // with a restart waiting for the turn, until it is done: it would lose what was
            // written to the process before it). A live change holds nothing back (R5).
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

    /// Something to send with the next message (a command's output, a file, an image), shown
    /// as a chip in the box until it is sent. Only kept: it never sends, and never restarts a
    /// paused tab (R13); an error tab (no agent) has nothing to send it to.
    pub fn attach(&mut self, attachment: Attachment, cx: &mut Context<Self>) {
        if self.provider.is_none() {
            return;
        }
        if !attachable(&self.attachments, &attachment) {
            return;
        }
        self.attachments.push(attachment);
        cx.notify();
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
                self.add_image(name, media_type, ImageFrom::File(path), cx);
            }
        }
    }

    /// Files dropped on the box: an image file becomes an image chip, any other a file chip.
    pub fn attach_dropped(&mut self, paths: Vec<PathBuf>, cx: &mut Context<Self>) {
        let (images, files) = paths.into_iter().partition(|p| media_type_for(p).is_some());
        self.attach_images(images, cx);
        self.attach_files(files, cx);
    }

    /// An image pasted from the clipboard, as an image chip. A format the agents don't take
    /// (not png, jpeg, gif or webp) is left out.
    pub fn attach_pasted_image(&mut self, image: gpui::Image, cx: &mut Context<Self>) {
        let media_type = match image.format {
            gpui::ImageFormat::Png => "image/png",
            gpui::ImageFormat::Jpeg => "image/jpeg",
            gpui::ImageFormat::Gif => "image/gif",
            gpui::ImageFormat::Webp => "image/webp",
            _ => return,
        };
        let name = format!("pasted-image.{}", image.format.extension());
        self.add_image(name, media_type, ImageFrom::Clipboard(image.bytes), cx);
    }

    /// An image chip once its bytes are in: they are read, checked against the provider's
    /// `image_limit` and, for a pasted image with an agent that is told a path, written to the
    /// temp folder, all off the UI thread. Too big (or unreadable): no chip, a notice says so.
    fn add_image(
        &mut self,
        name: String,
        media_type: &'static str,
        from: ImageFrom,
        cx: &mut Context<Self>,
    ) {
        let Some(provider) = self.provider else {
            return;
        };
        let by_path = !provider.sends_images();
        let (limit, label) = (provider.image_limit(), provider.info().label);
        let dir = std::env::temp_dir().join(PASTED_DIR);
        let job = cx.background_spawn(async move {
            let (bytes, path) = match from {
                ImageFrom::File(path) => {
                    let bytes = read_image(&path, &name, limit, label)?;
                    (bytes, Some(path))
                }
                ImageFrom::Clipboard(bytes) => (bytes, None),
            };
            let picture = picture(&name, media_type, bytes, limit, label)?;
            let path = match path {
                Some(path) => Some(path),
                None if by_path => {
                    let ext = name.rsplit('.').next().unwrap_or("png");
                    Some(
                        write_pasted(&dir, &picture, ext)
                            .map_err(|e| format!("Couldn't attach {name}: {e}"))?,
                    )
                }
                None => None,
            };
            Ok(Attachment::Image {
                name,
                picture,
                path,
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
                if self.menu.is_some() {
                    self.close_menu(cx);
                } else if self.at.take().is_some() {
                    cx.notify();
                } else if self.slash.take().is_some() {
                    self.slash_dismissed = true;
                    cx.notify();
                } else {
                    self.stop(cx);
                }
            }
            ComposerEvent::AtTyped => self.open_at(cx),
            ComposerEvent::Complete => self.complete_slash(cx),
            ComposerEvent::PasteImage(image) => self.attach_pasted_image(image.clone(), cx),
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
        if self.conversation.pending_permission().is_some() {
            let answer = if text.trim().is_empty() {
                Answer::Allowed
            } else {
                Answer::Denied(Some(text.trim().to_string()))
            };
            self.answer(answer, cx).ok();
        } else if let Some(command) = self.provider.and_then(|p| p.own_command(&text)) {
            // What fuxx answers itself opens its menu: nothing is sent.
            self.open_box_command(command, cx);
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
    /// Item `ix`'s row of the list.
    fn item_row(&self, ix: usize, look: &Look) -> AnyElement {
        use crate::agent_chat::{
            agent_side, avatar_top, chat, gap_before, is_row, shows_avatar, shows_thinking,
        };
        let items = self.conversation.items();
        // A sub-agent's items are drawn in their tool's detail (the modal), not as rows.
        if !is_row(items, ix) {
            return div().into_any_element();
        }
        let item = &items[ix];
        let c = &look.c;
        let last = (ix + 1..items.len()).all(|i| !is_row(items, i));
        let thinking = last && shows_thinking(self.status() == Status::Working, items);
        let body = self.item_body(ix, item, 0, look);
        let text_column = chat::AVATAR + chat::AVATAR_GAP;
        let column = div().w_full().max_w(px(chat::COLUMN_MAX));
        let column = if agent_side(item) {
            let gutter = div()
                .flex_none()
                .w(px(chat::AVATAR))
                .when(shows_avatar(items, ix), |d| {
                    d.pt(px(avatar_top(item))).child(self.avatar(look))
                });
            column
                .flex()
                .flex_row()
                .gap(px(chat::AVATAR_GAP))
                .child(self.marked("column", gutter))
                .child(div().flex_1().min_w(px(0.)).child(body))
        } else if matches!(item, Item::Notice(_) | Item::Error(_)) {
            column.pl(px(text_column)).child(body)
        } else {
            column.child(body)
        };
        // "Thinking…" under the last row while the turn works and nothing else moves (spec
        // B-B): after your message with the avatar, else in the text column; fading in late,
        // so short gaps never flash it.
        let thinking = thinking.then(|| {
            let after_you = matches!(item, Item::You { .. });
            let gap = match item {
                Item::You { .. } => chat::GAP_TURN,
                Item::Tool { .. } => chat::GAP_TEXT_TOOL,
                _ => chat::GAP_ITEM,
            };
            let slot = div()
                .flex_none()
                .w(px(chat::AVATAR))
                .flex()
                .justify_center()
                .when(after_you, |d| d.child(self.avatar(look)));
            div()
                .w_full()
                .max_w(px(chat::COLUMN_MAX))
                .pt(px(gap))
                .h(px(gap + chat::THINKING_H))
                .flex()
                .items_center()
                .gap(px(chat::AVATAR_GAP))
                .child(slot)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .text_size(px(text::META))
                        .text_color(hsla(c.dim))
                        .child(self.thinking_mark.clone())
                        .child("Thinking…"),
                )
                .with_animation(
                    ("agent-thinking", ix),
                    Animation::new(Duration::from_millis(chat::THINKING_FADE_MS)),
                    |d, t| {
                        let t = ((t - chat::THINKING_HOLD) / (1. - chat::THINKING_HOLD)).max(0.);
                        d.opacity(crate::agent_top::ease_out_cubic(t))
                    },
                )
        });
        div()
            .w_full()
            .px(px(AREA_X))
            .pt(px(gap_before(items, ix)))
            .when(last, |d| d.pb(px(chat::LAST_BOTTOM)))
            .text_size(px(chat::BODY))
            .line_height(relative(chat::BODY_LINE))
            .child(column)
            .children(thinking)
            .into_any_element()
    }

    /// The agent's avatar (spec B-A): its logo bare, else its letters on its colour.
    fn avatar(&self, look: &Look) -> AnyElement {
        use crate::agent_chat::chat;
        match look.logo_image.clone() {
            Some(logo) => div()
                .flex_none()
                .size(px(chat::AVATAR))
                .flex()
                .items_center()
                .justify_center()
                .child(gpui::img(logo).size(px(chat::LOGO)))
                .into_any_element(),
            None => div()
                .flex_none()
                .size(px(chat::AVATAR))
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
                .into_any_element(),
        }
    }

    /// An item's own look (inside a row, or nested `depth` deep under a tool).
    fn item_body(&self, ix: usize, item: &Item, depth: usize, look: &Look) -> AnyElement {
        use crate::agent_chat::chat;
        let c = &look.c;
        match item {
            // Your message (spec B-B): a bubble in the colours of the box you typed it in, its
            // tail at the bottom right; attachments as chips under the text.
            Item::You { text, attachments } => div()
                .w_full()
                .flex()
                .justify_end()
                .child(
                    div().w(relative(0.78)).flex().justify_end().child(
                        div()
                            .max_w(px(520.))
                            .px(px(14.))
                            .py(px(9.))
                            .rounded(px(chat::R_BUBBLE))
                            .rounded_br(px(chat::R_TAIL))
                            .bg(hsla(c.you_fill))
                            .border_1()
                            .border_color(hsla(c.you_edge))
                            .text_color(hsla(c.you_text))
                            .flex()
                            .flex_col()
                            .gap(px(6.))
                            .child(SharedString::from(text.clone()))
                            .when(!attachments.is_empty(), |d| {
                                d.child(div().flex().flex_wrap().gap(px(6.)).children(
                                    attachments.iter().map(|a| {
                                        div()
                                            .h(px(22.))
                                            .px(px(8.))
                                            .rounded(px(chat::R_CHIP))
                                            .bg(hsla(c.chip_fill))
                                            .flex()
                                            .items_center()
                                            .gap(px(4.))
                                            .text_size(px(text::META))
                                            .text_color(hsla(c.you_dim))
                                            .child(icon(Icon::Paperclip, px(12.), hsla(c.you_dim)))
                                            .child(SharedString::from(
                                                a.lines().next().unwrap_or("").to_string(),
                                            ))
                                    }),
                                ))
                            }),
                    ),
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
                    .max_w(px(chat::COLUMN_MAX - 32.))
                    .text_color(hsla(c.text))
                    .when(depth > 0, |d| d.text_color(hsla(c.dim)))
                    .child(md_preview::column(&blocks, ix, &look.draw))
                    .into_any_element()
            }
            Item::Tool { .. } => self.tool(ix, item, look),
            // The todo list (spec B-B): a card with its count and a still meter, then the tasks.
            Item::Todo(list) => {
                let done = list.iter().filter(|t| t.state == TodoState::Done).count();
                let meter = crate::agent_top::meter_fill(
                    (done * 100).checked_div(list.len()).unwrap_or(0) as u8,
                    crate::agent_top::METER_W,
                );
                div()
                    .w_full()
                    .px(px(14.))
                    .py(px(10.))
                    .rounded(px(chat::R_CARD))
                    .bg(hsla(c.panel))
                    .border_1()
                    .border_color(hsla(c.panel_border))
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .h(px(20.))
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .text_size(px(text::META))
                            .child(icon(Icon::ListChecks, px(12.), hsla(c.panel_dim)))
                            .child(
                                div()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(hsla(c.panel_text))
                                    .child("Tasks"),
                            )
                            .child(div().flex_1())
                            .child(
                                div()
                                    .font_features(crate::sidebar::tabular())
                                    .text_color(hsla(c.panel_dim))
                                    .child(SharedString::from(format!("{done} of {}", list.len()))),
                            )
                            .child(
                                div()
                                    .ml(px(2.))
                                    .relative()
                                    .w(px(crate::agent_top::METER_W))
                                    .h(px(crate::agent_top::METER_H))
                                    .rounded(px(2.))
                                    .bg(hsla(c.todo_track))
                                    .child(
                                        div()
                                            .absolute()
                                            .left_0()
                                            .top_0()
                                            .h_full()
                                            .w(px(meter))
                                            .rounded(px(2.))
                                            .bg(hsla(c.tool_ok)),
                                    ),
                            ),
                    )
                    .child(div().my(px(8.)).h(px(1.)).bg(hsla(c.tool_divider)))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(4.))
                            .text_size(px(13.))
                            .line_height(relative(1.45))
                            .children(list.iter().map(|t| {
                                let (glyph, ink, words, medium) = match t.state {
                                    TodoState::Done => (Icon::Check, c.tool_ok, c.panel_dim, false),
                                    TodoState::Active => {
                                        (Icon::CircleDot, c.todo_active, c.panel_text, true)
                                    }
                                    TodoState::Pending => {
                                        (Icon::Circle, c.panel_dim, c.panel_text, false)
                                    }
                                };
                                div()
                                    .flex()
                                    .gap(px(8.))
                                    .child(
                                        div()
                                            .flex_none()
                                            .w(px(14.))
                                            .h(px(13. * 1.45))
                                            .flex()
                                            .items_center()
                                            .child(icon(glyph, px(12.), hsla(ink))),
                                    )
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w(px(0.))
                                            .text_color(hsla(words))
                                            .when(medium, |d| d.font_weight(FontWeight::MEDIUM))
                                            .child(SharedString::from(t.text.clone())),
                                    )
                            })),
                    )
                    .into_any_element()
            }
            Item::Permission {
                tool,
                what,
                detail,
                always,
                answer,
                ..
            } => {
                match answer {
                    // Answered (spec B-B): one quiet line, a check or a cross before it.
                    Some(answer) => {
                        let allowed = matches!(answer, Answer::Allowed | Answer::AlwaysAllowed);
                        let (glyph, ink) = if allowed {
                            (Icon::Check, c.tool_ok)
                        } else {
                            (Icon::Stop, c.failed)
                        };
                        div()
                            .w_full()
                            .h(px(22.))
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .text_size(px(text::META))
                            .text_color(hsla(c.dim))
                            .child(icon(glyph, px(12.), hsla(ink)))
                            .child(div().flex_1().min_w(px(0.)).truncate().child(
                                SharedString::from(format!(
                                    "{} · {tool} {what}",
                                    answer_line(answer)
                                )),
                            ))
                            .into_any_element()
                    }
                    None => self.permission_card(tool, what, detail, always.as_deref(), look),
                }
            }
            // A notice (spec B-B): an info mark and the words, quiet, on the text's column.
            Item::Notice(t) => div()
                .w_full()
                .flex()
                .gap(px(6.))
                .text_size(px(text::META))
                .text_color(hsla(c.dim))
                .child(
                    div()
                        .flex_none()
                        .h(px(text::META * 1.5))
                        .flex()
                        .items_center()
                        .child(icon(Icon::Info, px(12.), hsla(c.dim))),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .child(SharedString::from(t.clone())),
                )
                .into_any_element(),
            Item::Error(t) => {
                let classic = self.provider.is_some() && self.conversation_id().is_some();
                let closable = self.provider.is_none();
                // An error (spec B-B): a card washed with the failure's colour, its octagon,
                // the message in the body's colour (readable), its buttons under it.
                div()
                    .w_full()
                    .px(px(14.))
                    .py(px(12.))
                    .rounded(px(chat::R_CARD))
                    .bg(hsla(c.error_fill))
                    .border_1()
                    .border_color(hsla(c.error_edge))
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .child(
                        div()
                            .flex()
                            .gap(px(8.))
                            .child(
                                div()
                                    .flex_none()
                                    .h(px(chat::BODY * chat::BODY_LINE))
                                    .flex()
                                    .items_center()
                                    .child(icon(Icon::Octagon, px(14.), hsla(c.error_mark))),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.))
                                    .text_color(hsla(c.error_text))
                                    .child(SharedString::from(t.clone())),
                            ),
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

    /// A tool call's row (spec B-B): its status (an arc while it runs, a dot once done), name,
    /// target and result, and a chevron when it has more to show; a run of tool rows draws as one
    /// group (`agent_chat::group_shape`). A click opens its detail in the modal (spec A).
    fn tool(&self, ix: usize, item: &Item, look: &Look) -> AnyElement {
        use crate::agent_chat::{Shape, chat, group_shape, summary_shown};
        let Item::Tool {
            id,
            name,
            summary,
            running,
            ok,
            result,
            detail,
            output,
            ..
        } = item
        else {
            return div().into_any_element();
        };
        let c = &look.c;
        let items = self.conversation.items();
        let steps = items
            .iter()
            .filter(|i| parent_of(i) == Some(id.as_str()))
            .count();
        let diff = match detail {
            Some(ToolDetail::Diff { unified, .. }) => Some(parse_diff(unified)),
            _ => None,
        };
        let openable = crate::agent_tool_modal::openable(items, ix);
        let right: (String, Rgb) = if *running {
            let since = self
                .tool_started
                .get(id)
                .map_or(Duration::ZERO, |t| look.now.saturating_duration_since(*t));
            (elapsed_label(since), c.panel_dim)
        } else if *ok == Some(false) {
            let bare = result.trim_start_matches('✗').trim_start();
            (bare.to_string(), c.failed_panel)
        } else if let Some(d) = &diff {
            let added = d.rows.iter().filter(|r| r.kind == RowKind::Added).count();
            let removed = d.rows.iter().filter(|r| r.kind == RowKind::Removed).count();
            (format!("+{added} −{removed}"), c.panel_dim)
        } else if let (true, Some(out)) = (name == "Read", output) {
            (lines_label(out.lines().count()), c.panel_dim)
        } else {
            (result.clone(), c.panel_dim)
        };
        // Its status: running, the Working arc; ok, a dot; failed, a red dot; else a ring.
        let dot = |fill: Rgb| {
            div()
                .size(px(chat::TOOL_DOT))
                .rounded_full()
                .bg(hsla(fill))
                .into_any_element()
        };
        let status: AnyElement = match (running, ok) {
            (true, _) => match self.tool_marks.get(id) {
                Some(mark) => mark.clone().into_any_element(),
                None => dot(c.todo_active),
            },
            (false, Some(true)) => dot(c.tool_ok),
            (false, Some(false)) => dot(c.failed_panel),
            (false, None) => div()
                .size(px(chat::TOOL_DOT))
                .rounded_full()
                .border_1()
                .border_color(hsla(c.panel_dim))
                .into_any_element(),
        };
        let chevron_slot = div()
            .flex_none()
            .size(px(crate::right_sidebar::CHEVRON))
            .when(openable, |d| {
                let (rest, hover) = (hsla(c.panel_dim), hsla(c.panel_text));
                d.child(
                    icon(Icon::ChevronRight, px(crate::right_sidebar::CHEVRON), rest)
                        .group_hover("agent-tool-row", move |s| s.text_color(hover)),
                )
            });
        let shape = group_shape(items, ix).unwrap_or(Shape::Solo);
        let (top, bottom) = match shape {
            Shape::Solo => (true, true),
            Shape::First => (true, false),
            Shape::Middle => (false, false),
            Shape::Last => (false, true),
        };
        let row_hover = hsla(c.row_hover);
        let tool = div()
            .id(("agent-tool", ix))
            .group("agent-tool-row")
            .relative()
            .w_full()
            .h(px(chat::TOOL_ROW))
            .px(px(chat::TOOL_PAD_X))
            .flex()
            .items_center()
            .gap(px(chat::TOOL_GAP))
            .bg(hsla(c.panel))
            .border_color(hsla(c.panel_border))
            .border_l_1()
            .border_r_1()
            .when(top, |d| d.border_t_1().rounded_t(px(chat::R_GROUP)))
            .when(bottom, |d| d.border_b_1().rounded_b(px(chat::R_GROUP)))
            .text_size(px(text::META))
            .child(
                div()
                    .flex_none()
                    .size(px(chat::TOOL_STATUS))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(status),
            )
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
                    .when(summary_shown(name, summary), |d| {
                        d.child(SharedString::from(summary.clone()))
                    }),
            )
            .when(steps > 0, |d| {
                d.child(
                    div()
                        .flex_none()
                        .text_color(hsla(c.panel_dim))
                        .child(SharedString::from(format!("{steps} steps"))),
                )
            })
            .child(
                div()
                    .flex_none()
                    .max_w(relative(0.4))
                    .truncate()
                    .font_features(crate::sidebar::tabular())
                    .text_color(hsla(right.1))
                    .child(SharedString::from(right.0)),
            )
            .child(chevron_slot)
            // The line between a group's rows, from the name's column (spec B-B).
            .when(!bottom, |d| {
                d.child(
                    div()
                        .absolute()
                        .bottom_0()
                        .left(px(chat::TOOL_DIVIDER_INSET))
                        .right_0()
                        .h(px(1.))
                        .bg(hsla(c.tool_divider)),
                )
            });
        // The whole row opens the tool's detail (spec A §A): a modal, never inline.
        let tool = if openable {
            let view = look.view.clone();
            let id = id.clone();
            tool.cursor_pointer()
                .hover(move |s| s.bg(row_hover))
                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                    cx.stop_propagation();
                    let id = id.clone();
                    view.update(cx, |this, cx| this.open_tool(id, cx)).ok();
                })
        } else {
            tool
        };
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
        // Spec B-B: a card (r12), a shield before its heading, the command in a well.
        div()
            .w_full()
            .px(px(14.))
            .py(px(12.))
            .rounded(px(crate::agent_chat::chat::R_CARD))
            .border_1()
            .border_color(hsla(c.ask_edge))
            .bg(hsla(c.ask_fill))
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .text_color(hsla(c.ask_head))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(icon(Icon::Shield, px(14.), hsla(c.ask_head)))
                    .child(SharedString::from(format!("{label} wants to run {tool}"))),
            )
            .child(
                div()
                    .w_full()
                    .px(px(12.))
                    .py(px(10.))
                    .rounded(px(crate::agent_chat::chat::R_GROUP))
                    .bg(hsla(c.code_fill))
                    .font_family(look.mono.clone())
                    .text_size(px(12.5))
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
    pub(crate) fn stop_label(&self) -> Option<&'static str> {
        match (self.stopping.is_some(), self.force) {
            (true, true) => Some("Force stop"),
            (true, false) => Some("Stopping…"),
            (false, _) if self.status() == Status::Working => Some("■ Stop  esc"),
            _ => None,
        }
    }

    /// The tool modal (spec A §B): a scrim over the tab below its top bar (a click on it closes),
    /// and the shown tool's detail — its header, then its command, changes, output and steps,
    /// in one scroll. It holds the keyboard (key context `AgentToolModal`: Esc, ↑, ↓).
    fn modal_layer(&mut self, look: &Rc<Look>, cx: &mut Context<Self>) -> AnyElement {
        use crate::agent_tool_modal::{
            Right, follows, index_of, modal_width, near_bottom, parent_of as parent_id, right_of,
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
        // Output growing keeps the body at its bottom while it follows (spec A §B).
        let (offset, max) = (
            f32::from(self.modal_scroll.offset().y),
            f32::from(self.modal_scroll.max_offset().y),
        );
        let grew = max > modal.last_max + 0.5;
        modal.follow = follows(modal.follow, grew, near_bottom(offset, modal.last_max));
        modal.last_max = max;
        if modal.follow && grew {
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
        let elapsed = self.tool_started.get(id).map_or(String::new(), |t| {
            elapsed_label(look.now.saturating_duration_since(*t))
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
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .truncate()
                    .text_color(hsla(c.panel_text))
                    .when_some(parent_name.filter(|_| in_step), |d, p| {
                        d.child(
                            div()
                                .flex_none()
                                .text_color(hsla(c.panel_dim))
                                .child(SharedString::from(format!("{p} › "))),
                        )
                    })
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
        let shows_output =
            name != "Read" && output.is_some() && (diff.is_none() || *ok == Some(false));
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
        if !children.is_empty() {
            sections.push(section(
                "Steps",
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .font_family(look.mono.clone())
                    .children(
                        children
                            .iter()
                            .map(|&i| self.item_body(i, &items[i], 1, look)),
                    )
                    .into_any_element(),
            ));
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
            .child(body)
            .with_animation(
                ("agent-modal-open", opened),
                Animation::new(Duration::from_millis(180))
                    .with_easing(crate::agent_top::ease_out_cubic),
                |d, t| d.opacity(t).mt(px(8. * (1. - t))),
            );
        let scrim = gpui::Hsla {
            a: c.scrim_alpha,
            ..hsla(c.scrim)
        };
        let close_view = view.clone();
        div()
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
            .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                close_view.update(cx, |this, cx| this.close_modal(cx)).ok();
            })
            .child(panel)
            .with_animation(
                ("agent-modal-scrim", opened),
                Animation::new(Duration::from_millis(140))
                    .with_easing(crate::agent_top::ease_out_cubic),
                move |d, t| {
                    d.bg(gpui::Hsla {
                        a: scrim.a * t,
                        ..scrim
                    })
                },
            )
            .into_any_element()
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
    }

    /// The pane's top bar (spec A §C): the status pill on the left; on the right what the
    /// conversation cost (and what its last turn added) and how full its context is, live.
    fn top_line(&mut self, theme: &Theme, c: &Colors, cx: &mut Context<Self>) -> AnyElement {
        use crate::agent_top::{
            Band, METER_H, METER_W, PillMark, band, ease_out_cubic, meter_fill, pill, trim,
        };
        let status = self.status();
        let stopping = self.stopping.is_some();
        let cost = self.conversation.cost_usd();
        let context = self.conversation.context_pct();
        self.top.observe(status, stopping, cost, context);
        let top = self.top.clone();
        let now = pill(status, stopping, theme);
        let from = top.pill_from.map(|(s, st)| pill(s, st, theme));
        let fill_of = |p: &crate::agent_top::Pill| p.fill.unwrap_or(c.page);
        let blend = from.map(|f| (fill_of(&f), f.word_ink));
        let (to_fill, to_ink) = (fill_of(&now), now.word_ink);
        let mark: AnyElement = match now.mark {
            PillMark::Status(s) => {
                let fill = to_fill;
                self.top_mark.update(cx, |m, _| m.set(s, px(MARK), fill));
                self.top_mark.clone().into_any_element()
            }
            PillMark::DoneCheck => {
                let ink = now.mark_ink;
                let slot = div()
                    .flex_none()
                    .size(px(MARK))
                    .flex()
                    .items_center()
                    .justify_center();
                if top.pill_from.is_some() {
                    slot.with_animation(
                        ("agent-pill-done", top.pill_key),
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
            }
        };
        let pill_el = div()
            .flex_none()
            .h(px(22.))
            .ml(px(-8.))
            .pl(px(8.))
            .pr(px(10.))
            .rounded(px(11.))
            .flex()
            .items_center()
            .gap(px(6.))
            .font_weight(FontWeight::MEDIUM)
            .bg(hsla(to_fill))
            .text_color(hsla(to_ink))
            .child(mark)
            .child(SharedString::from(now.word));
        let pill_el = match blend {
            Some((from_fill, from_ink)) => pill_el
                .with_animation(
                    ("agent-pill", top.pill_key),
                    Animation::new(Duration::from_millis(200)).with_easing(ease_out_cubic),
                    move |d, t| {
                        d.bg(hsla(mix(from_fill, to_fill, t)))
                            .text_color(hsla(mix(from_ink, to_ink, t)))
                    },
                )
                .into_any_element(),
            None => pill_el.into_any_element(),
        };
        let shown = trim(self.bar_width.get());
        let cost_group = cost.map(|usd| {
            let delta = top.delta.clone().filter(|_| shown.delta).map(|d| {
                div()
                    .text_color(hsla(c.dim))
                    .child(SharedString::from(d))
                    .with_animation(
                        ("agent-delta", top.delta_key),
                        Animation::new(Duration::from_millis(200)),
                        |d, t| d.opacity(t),
                    )
            });
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
                    "agent-cost-in",
                    Animation::new(Duration::from_millis(200)),
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
            let from_w = top.context_from.map(|p| meter_fill(p, METER_W));
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
                    "agent-context-in",
                    Animation::new(Duration::from_millis(200)),
                    |d, t| d.opacity(t),
                )
        });
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
            .text_color(hsla(c.dim))
            .child(measure)
            .child(self.marked("status", div().flex_none().child(pill_el)))
            .child(div().flex_1())
            .children(cost_group)
            .when(cost.is_some() && context.is_some(), |d| {
                d.child(div().w(px(16.)))
            })
            .children(context_group.map(|g| self.marked("usage", g)))
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
                .px(px(AREA_X))
                .pb(px(10.))
                .flex()
                .flex_col()
                .items_end()
                .gap(px(6.))
                .text_size(px(BODY))
                // Spec B-B: your bubble, quieter (the box's outer fill), waiting to go.
                .children(self.queued.iter().map(|(text, _)| {
                    div()
                        .max_w(relative(0.78))
                        .px(px(14.))
                        .py(px(9.))
                        .rounded(px(crate::agent_chat::chat::R_BUBBLE))
                        .rounded_br(px(crate::agent_chat::chat::R_TAIL))
                        .bg(hsla(c.queued_fill))
                        .border_1()
                        .border_color(hsla(c.you_edge))
                        .text_color(hsla(c.queued_text))
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
            },
            diff: diff_row_colours(&theme, theme.background),
            view: view.clone(),
            now: Instant::now(),
        });
        #[cfg(feature = "ui-shots")]
        let built = self.rows_built.clone();
        let pending = self.pending(&c);
        let stream = if empty && self.queued.is_empty() {
            self.empty_state(&c, cx)
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
        // The keyboard moves where a click or an action asked (they have no window).
        match self.refocus.take() {
            Some(Refocus::Modal) => window.focus(&self.modal_focus, cx),
            Some(Refocus::Box) => {
                let to = self.composer.read(cx).focus_handle(cx);
                window.focus(&to, cx);
            }
            None => {}
        }
        let modal = self.modal.is_some().then(|| self.modal_layer(&look, cx));
        div()
            .key_context("AgentTab")
            .on_action(cx.listener(Self::at_up))
            .on_action(cx.listener(Self::at_down))
            .on_action(cx.listener(Self::modal_back))
            .on_action(cx.listener(Self::modal_prev))
            .on_action(cx.listener(Self::modal_next))
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
            })
            .text_color(hsla(c.text))
            .child(self.top_line(&theme, &c, cx))
            .child(if self.cleared > 0 {
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
            })
            .children(pending)
            // An error tab (no such agent) has nothing to send to.
            .when(self.provider.is_some(), |d| {
                d.child(self.box_area(&c, mono, window, cx))
            })
            .children(modal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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

    #[test]
    fn words_and_chips() {
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

    fn image(name: &str, path: Option<&str>) -> Attachment {
        Attachment::Image {
            name: name.into(),
            picture: Picture::new("image/png", b"x".to_vec()),
            path: path.map(PathBuf::from),
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
        let dir = std::env::temp_dir().join(format!("fuxx-test-read-{}", std::process::id()));
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
        let dir = std::env::temp_dir().join(format!("fuxx-test-clean-{}", std::process::id()));
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
        let dir = std::env::temp_dir().join(format!("fuxx-test-paste-{}", std::process::id()));
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
                ("dim on a control", c.dim, c.control),
                (
                    "text on a control under the pointer",
                    c.text,
                    c.control_hover,
                ),
                ("dim on a pill", c.dim, c.pill),
            ] {
                let got = contrast(fg, on);
                assert!(got >= TEXT_MIN - 0.01, "{name}: {what} {got:.2}");
            }
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
            let got = contrast(c.on_accent, c.send_hover);
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
