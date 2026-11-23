//! Rewind and Fork in the agent tab (batch 3, chore 2): which of your messages offer what, the
//! words, the confirm rule, the Esc Esc pick. Pure parts first; the tab's flows after them.

use std::time::{Duration, Instant};

use futures::channel::oneshot;
use futures::future::{self, Either};

use super::*;
use crate::agent_conversation::{Conversation, Cut, Item};
use crate::providers::{Branch, Branching, FileRewind};

/// The notice a stopped turn leaves (the provider's, and Force stop's).
pub(crate) const STOPPED_NOTICE: &str = "Stopped";
/// The link that follows it while the message before it can be rewound.
pub(crate) const STOPPED_LINK: &str = "Rewind to edit your message";
/// The notice a rewind leaves (its Undo follows it until the next send).
pub(crate) const REWOUND_NOTICE: &str = "Rewound";
/// The empty box's words after a stop.
pub(crate) const PICK_PLACEHOLDER: &str = "Esc twice to rewind";
/// Esc twice within this enters the rewind pick.
pub(crate) const DOUBLE_ESC: Duration = Duration::from_millis(400);
/// How long the agent has to answer a file rewind (its dry run, or the rewind itself).
pub(crate) const FILES_WAIT: Duration = Duration::from_secs(10);
/// A fork's first row (drawn with the source's name: `fork_notice`).
pub(crate) const FORKED_NOTICE: &str = "Forked";

/// A hover action on your message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Hover {
    Copy,
    Rewind,
    Fork,
    /// Your text back in the box, the chat left as it is (an agent that can't go back).
    Edit,
    /// × on a message the agent hasn't read yet (batch 3, wave 3): taken back.
    Cancel,
}

/// Message `item`'s id when it can be rewound or forked from: yours, read by the agent, named.
pub(crate) fn branch_id(item: &Item) -> Option<&str> {
    match item {
        Item::You {
            id: Some(id),
            taken: true,
            ..
        } => Some(id),
        _ => None,
    }
}

/// The hover actions on `item`: Copy on any message of yours; Rewind and Fork as the agent can,
/// once the message is read; Edit instead when the agent can't go back at all.
pub(crate) fn hover_actions(item: &Item, b: Branching) -> Vec<Hover> {
    if !matches!(item, Item::You { .. }) {
        return Vec::new();
    }
    let mut out = vec![Hover::Copy];
    if !b.any() {
        out.push(Hover::Edit);
        return out;
    }
    if branch_id(item).is_some() {
        if b.rewind {
            out.push(Hover::Rewind);
        }
        if b.fork {
            out.push(Hover::Fork);
        }
    }
    out
}

/// A hover action's tooltip. `files`: the agent can undo file changes; `edit`: the provider's own
/// words for Edit.
pub(crate) fn hover_tip(h: Hover, files: bool, edit: Option<&str>) -> String {
    match h {
        Hover::Copy => "Copy".into(),
        Hover::Rewind if files => "Rewind chat and files to here".into(),
        Hover::Rewind => "Rewind the chat to here (files stay as they are)".into(),
        Hover::Fork => "Fork from here into a new tab".into(),
        Hover::Edit => edit
            .unwrap_or("Put this message back in the box")
            .to_string(),
        Hover::Cancel => super::cancel::CANCEL_TIP.into(),
    }
}

/// The "Stopped" notice that carries "Rewind to edit your message": the last one after your
/// latest message, while that message can be rewound (until the next send makes another latest).
pub(crate) fn stopped_link(items: &[Item], latest: Option<usize>, b: Branching) -> Option<usize> {
    let you = latest?;
    if !b.rewind || branch_id(items.get(you)?).is_none() {
        return None;
    }
    (you + 1..items.len())
        .rev()
        .find(|&i| matches!(&items[i], Item::Notice(t) if t == STOPPED_NOTICE))
}

/// The message the latest-prompt strip's Rewind button rewinds (the opens fixed, 2026-11-23):
/// the one the "Stopped" notice offers (`stopped_link`), in Main's view (`main`) of a tab not
/// paused for its classic view (that process holds the conversation).
pub(crate) fn strip_rewind(
    items: &[Item],
    latest: Option<usize>,
    b: Branching,
    main: bool,
    paused: bool,
) -> Option<usize> {
    if !main || paused {
        return None;
    }
    stopped_link(items, latest, b).and(latest)
}

/// A fork's title: the source's with " (fork)"; a fork of a fork counts on ("(fork 2)", …).
pub(crate) fn fork_title(source: &str) -> String {
    if let Some(base) = source.strip_suffix(" (fork)") {
        return format!("{base} (fork 2)");
    }
    if let Some(rest) = source.strip_suffix(')')
        && let Some((base, n)) = rest.rsplit_once(" (fork ")
        && let Ok(n) = n.parse::<u32>()
    {
        return format!("{base} (fork {})", n + 1);
    }
    format!("{source} (fork)")
}

/// A fork's tooltip line: where it came from, and the message it was forked at.
pub(crate) fn fork_tip(source: &str, text: &str) -> String {
    let line = text
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
        .trim();
    let short: String = line.chars().take(40).collect();
    let more = if line.chars().count() > 40 { "…" } else { "" };
    format!("Forked from {source} at \u{201c}{short}{more}\u{201d}")
}

/// The fork's warning under its first row.
pub(crate) fn shares_folder(source: &str) -> String {
    format!("Shares the folder with {source}: both agents can edit the same files.")
}

/// The native sheet a rewind asks first: its words, its buttons, and which button undoes the
/// files too (none: it can't) and which rewinds the chat only. The last button cancels.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Ask {
    pub title: String,
    pub detail: String,
    pub buttons: Vec<&'static str>,
    pub undo: Option<usize>,
    pub chat: usize,
}

const ASK_TITLE: &str = "Rewind to this message?";

/// Whether the agent's answer says files changed after the message.
fn changed(o: &FileRewind) -> bool {
    !o.files.is_empty() || o.insertions.unwrap_or(0) + o.deletions.unwrap_or(0) > 0
}

/// "3 files (+12 −4)": how many files and lines, as far as the answer says.
fn what_changed(o: &FileRewind) -> String {
    let files = match o.files.len() {
        0 => "files".to_string(),
        1 => "1 file".to_string(),
        n => format!("{n} files"),
    };
    match (o.insertions, o.deletions) {
        (Some(a), Some(d)) => format!("{files} after it (+{a} \u{2212}{d})"),
        _ => format!("{files} after it"),
    }
}

/// Whether a rewind asks first, and what (batch 3, chore 2's confirm rule). `files`: the agent
/// can undo file changes at all; `answer`: its dry run's answer (None: none came); `live`: its
/// process runs (None then means it didn't answer in time; without a process there was no dry
/// run, and the undo is the one-off).
/// - No files changed (or the agent can't tell, or can't undo files at all): no sheet.
/// - Files changed and it can undo them: Rewind and Undo Changes · Rewind Chat Only · Cancel.
/// - Files changed and it can't: honest words, Rewind Chat · Cancel.
/// - No process to ask: the files may have changed, so it asks as if it can undo them.
pub(crate) fn confirm_for(files: bool, answer: Option<&FileRewind>, live: bool) -> Option<Ask> {
    if !files {
        return None;
    }
    let undo = |detail: String| Ask {
        title: ASK_TITLE.into(),
        detail,
        buttons: vec!["Rewind and Undo Changes", "Rewind Chat Only", "Cancel"],
        undo: Some(0),
        chat: 1,
    };
    match answer {
        Some(o) if !changed(o) => None,
        Some(o) if o.can_rewind => Some(undo(format!(
            "The agent changed {}. Undo those changes too, or keep them and rewind only the chat.",
            what_changed(o)
        ))),
        Some(o) => Some(Ask {
            title: ASK_TITLE.into(),
            detail: format!(
                "The agent changed {}. It can't undo file changes from fuxx, so they stay as they \
                 are. You can revert them in Changes.",
                what_changed(o)
            ),
            buttons: vec!["Rewind Chat", "Cancel"],
            undo: None,
            chat: 0,
        }),
        None if live => None,
        None => Some(undo(
            "The agent may have changed files after it. Undo those changes too, or keep them and \
             rewind only the chat."
                .into(),
        )),
    }
}

/// The newest message of yours that can be rewound (the pick starts there).
pub(crate) fn pick_start(items: &[Item]) -> Option<usize> {
    items.iter().rposition(|i| branch_id(i).is_some())
}

/// The next message of yours that can be rewound from `from`, older (`up`) or newer; None past
/// the ends.
pub(crate) fn pick_step(items: &[Item], from: usize, up: bool) -> Option<usize> {
    if up {
        items[..from.min(items.len())]
            .iter()
            .rposition(|i| branch_id(i).is_some())
    } else {
        (from + 1..items.len()).find(|&i| branch_id(&items[i]).is_some())
    }
}

/// Esc at `now`, the last at `last`: the second of two within `DOUBLE_ESC`.
pub(crate) fn double_esc(last: Option<Instant>, now: Instant) -> bool {
    last.is_some_and(|t| now.saturating_duration_since(t) <= DOUBLE_ESC)
}

/// What the tab keeps for Rewind and Fork.
#[derive(Default)]
pub(crate) struct State {
    /// The rewind (or fork) under way: stopping the turn, reading the branch point, asking.
    task: Option<Task<()>>,
    /// The flow waiting for the turn to end (Rewind stops a turn first).
    turn_done: Option<oneshot::Sender<()>>,
    /// The flow waiting for the agent's answer to a file rewind of a message (`dry_run` or not).
    files: Option<(String, bool, oneshot::Sender<FileRewind>)>,
    /// The next start branches off a conversation (`Start::branch`), once.
    pub(super) branch_next: Option<Branch>,
    /// The last rewind, for Undo (until the next send) and for a branch that fails to start.
    undo: Option<Undo>,
    /// The Esc Esc pick: your message lit (its item).
    pub(crate) pick: Option<usize>,
    last_esc: Option<Instant>,
    /// What each message you sent here carried, by its id (a rewind puts it back in the box).
    sent: HashMap<String, Vec<Attachment>>,
    /// A fork's first row: its source's name, and whether that tab is still open.
    pub(super) fork: Option<ForkNote>,
    /// Scenes: the sheet a rewind asked (its words), and the button they answer it with
    /// (no native sheet is shown).
    #[cfg(feature = "ui-shots")]
    pub(crate) shot_asked: Option<Ask>,
    #[cfg(feature = "ui-shots")]
    pub(crate) shot_answer: Option<usize>,
    /// Scenes: the folder a transcript is read under (never your home).
    #[cfg(feature = "ui-shots")]
    pub(crate) shot_home: Option<std::path::PathBuf>,
    /// Scenes: where each hover action was laid out ("rewind:3").
    #[cfg(feature = "ui-shots")]
    pub(crate) shot_buttons: Rc<std::cell::RefCell<HashMap<String, gpui::Bounds<gpui::Pixels>>>>,
}

/// The last rewind: the conversation it branched off, what it cut, where its notice is, and
/// whether Undo is still offered (until the next send).
struct Undo {
    from: String,
    cut: Cut,
    notice: usize,
    undoable: bool,
}

/// A fork's first row.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ForkNote {
    pub source: String,
    pub open: bool,
}

/// What a fork starts from (sent to the Workspace, which opens its tab beside the source).
pub struct ForkSeed {
    pub(crate) conversation: Conversation,
    pub(crate) branch: Branch,
    pub(crate) text: String,
    pub(crate) attachments: Vec<Attachment>,
    pub(crate) images: HashMap<String, Picture>,
    pub(crate) pasted: usize,
}

/// A `ForkSeed` in an event (taken once by the Workspace).
#[derive(Clone)]
pub struct Seed(pub Rc<std::cell::RefCell<Option<ForkSeed>>>);

impl PartialEq for Seed {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl std::fmt::Debug for Seed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Seed")
    }
}

impl AgentTab {
    /// What its agent can do to go back (nothing without one).
    pub(crate) fn branching(&self) -> Branching {
        self.provider.map(|p| p.branching()).unwrap_or_default()
    }

    /// Remember what message `id` carried (a rewind puts it back); Undo is offered no more, and
    /// once the branch has started, a failed rewind has nothing to fall back to.
    pub(super) fn rewind_sent(&mut self, id: Option<&str>, attachments: &[Attachment]) {
        if let Some(id) = id
            && !attachments.is_empty()
        {
            self.rw.sent.insert(id.to_string(), attachments.to_vec());
        }
        self.rw.pick = None;
        if let Some(undo) = &mut self.rw.undo {
            undo.undoable = false;
        }
        if self.run.started {
            self.rw.undo = None;
        }
    }

    /// An event the rewind flow waits on (before the conversation applies it).
    pub(super) fn rewind_event(&mut self, event: &Event) {
        match event {
            Event::TurnDone { .. } => {
                if let Some(tx) = self.rw.turn_done.take() {
                    tx.send(()).ok();
                }
            }
            Event::FilesRewound {
                message,
                dry_run,
                outcome,
            } => {
                if self
                    .rw
                    .files
                    .as_ref()
                    .is_some_and(|(m, d, _)| m == message && d == dry_run)
                    && let Some((_, _, tx)) = self.rw.files.take()
                {
                    tx.send(outcome.clone()).ok();
                }
            }
            // The branch started and a message went to it: no going back to the old one.
            Event::Started { .. } if self.rw.undo.as_ref().is_some_and(|u| !u.undoable) => {
                self.rw.undo = None;
            }
            _ => {}
        }
    }

    /// Message `ix`'s text, and what it carried when it was sent here.
    fn message_at(&self, ix: usize) -> Option<(String, Vec<Attachment>)> {
        let Some(Item::You { text, id, .. }) = self.conversation.items().get(ix) else {
            return None;
        };
        let attachments = id
            .as_ref()
            .and_then(|id| self.rw.sent.get(id).cloned())
            .unwrap_or_default();
        Some((text.clone(), attachments))
    }

    /// Copy: your message's text.
    pub(crate) fn copy_message(&self, ix: usize, cx: &mut Context<Self>) {
        if let Some((text, _)) = self.message_at(ix) {
            cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
        }
    }

    /// Edit (an agent that can't go back): the message back in the box, the chat left as it is.
    pub(crate) fn edit_message(&mut self, ix: usize, cx: &mut Context<Self>) {
        if let Some((text, attachments)) = self.message_at(ix) {
            self.put_back(text, attachments, cx);
        }
    }

    /// `text` and `attachments` in the box, in place of what it held.
    fn put_back(&mut self, text: String, attachments: Vec<Attachment>, cx: &mut Context<Self>) {
        for a in &attachments {
            if let Attachment::Image {
                name,
                picture,
                inline,
                ..
            } = a
            {
                self.images.insert(name.clone(), picture.clone());
                let mark = marker(name);
                if *inline && !self.pills.contains(&mark) {
                    self.pills.push(mark);
                }
            }
        }
        // Its path pills (batch 3, chore 5) back as pills, not chips.
        self.restore_pills(&attachments);
        self.attachments = attachments
            .into_iter()
            .filter(|a| !matches!(a, Attachment::PathPill { .. }))
            .collect();
        let atoms = self.box_atoms();
        self.composer.update(cx, |c, cx| {
            c.set_atoms(atoms, cx);
            c.set_text(text, cx);
        });
        cx.notify();
    }

    /// Where a transcript is read: your home (a scene's own folder in scenes).
    fn branch_home(&self) -> Option<std::path::PathBuf> {
        #[cfg(feature = "ui-shots")]
        if self.no_process {
            return self.rw.shot_home.clone();
        }
        crate::workspace::home_dir()
    }

    /// Ask the agent to rewind the files changed since `message` (`dry_run`: only say what would
    /// change); its answer comes on the receiver (`rewind_event`).
    fn ask_files(&mut self, message: &str, dry_run: bool) -> oneshot::Receiver<FileRewind> {
        let (tx, rx) = oneshot::channel();
        self.rw.files = Some((message.to_string(), dry_run, tx));
        self.write(&Action::RewindFiles {
            message: message.to_string(),
            dry_run,
        });
        rx
    }

    /// The sheet `ask` (a native one; a scene's answer in scenes): the button chosen.
    fn ask_sheet(
        &mut self,
        ask: &Ask,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> oneshot::Receiver<usize> {
        #[cfg(feature = "ui-shots")]
        if self.no_process {
            self.rw.shot_asked = Some(ask.clone());
            let (tx, rx) = oneshot::channel();
            if let Some(answer) = self.rw.shot_answer.take() {
                tx.send(answer).ok();
            }
            return rx;
        }
        let _ = &cx;
        window.prompt(
            gpui::PromptLevel::Warning,
            &ask.title,
            Some(&ask.detail),
            &ask.buttons,
            cx,
        )
    }

    /// The turn didn't end within `STOP_FORCE_AFTER` of a rewind's stop: the process ends (the
    /// branch starts a new one).
    fn end_turn_now(&mut self, cx: &mut Context<Self>) {
        self.stopping = None;
        self.force = false;
        self.force_timer = None;
        self.end_process();
        self.reasons.end_turn();
        self.conversation
            .apply(Event::Notice(STOPPED_NOTICE.into()));
        self.conversation.apply(Event::TurnDone {
            ok: false,
            message: None,
        });
        self.after_change(true, cx);
    }

    /// Rewind (batch 3, chore 2) to before your message `ix`: a turn running stops first (Force
    /// after `STOP_FORCE_AFTER`); where it sits in the agent's record is read off the UI thread;
    /// the agent says whether files changed after it (its dry run) and the confirm rule asks or
    /// not (`confirm_for`); the files are undone if you chose so; then the chat is cut there, its
    /// text and attachments back in the box, and the agent starts again on a branch of the
    /// conversation that ends before it (the old one is kept, for Undo).
    pub(crate) fn rewind_to(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(provider) = self.provider else {
            return;
        };
        let b = provider.branching();
        if !b.rewind || self.rw.task.is_some() || self.paused {
            return;
        }
        let Some(message) = self
            .conversation
            .items()
            .get(ix)
            .and_then(branch_id)
            .map(str::to_string)
        else {
            return;
        };
        let Some(from) = self.conversation_id() else {
            return;
        };
        self.rw.pick = None;
        let working = matches!(self.status(), Status::Working | Status::Waiting);
        let stopped = working.then(|| {
            let (tx, rx) = oneshot::channel();
            self.rw.turn_done = Some(tx);
            rx
        });
        if working {
            self.stop(cx);
        }
        let start = Start {
            resume: Some(from.clone()),
            branch: None,
            ..self.start.clone()
        };
        let home = self.branch_home();
        let path = self.path.clone();
        self.rw.task = Some(cx.spawn_in(window, async move |this, cx| {
            if let Some(rx) = stopped {
                let timer = cx.background_executor().timer(STOP_FORCE_AFTER);
                if let Either::Right(_) = future::select(rx, Box::pin(timer)).await {
                    this.update(cx, |this, cx| this.end_turn_now(cx)).ok();
                }
            }
            let point = {
                let (start, message) = (start.clone(), message.clone());
                cx.background_executor()
                    .spawn(async move {
                        let home = home.ok_or("Couldn't find your home folder")?;
                        provider.branch_point(&home, &start, &message)
                    })
                    .await
            };
            let point = match point {
                Ok(point) => point,
                Err(why) => {
                    this.update(cx, |this, cx| {
                        this.rewind_ended(Some(format!("Couldn't rewind: {why}")), cx)
                    })
                    .ok();
                    return;
                }
            };
            let live = this
                .update(cx, |this, _| this.process.is_some() || this.no_process)
                .unwrap_or(false);
            let answer = if b.files && live {
                let Ok(rx) = this.update(cx, |this, _| this.ask_files(&message, true)) else {
                    return;
                };
                let timer = cx.background_executor().timer(FILES_WAIT);
                match future::select(rx, Box::pin(timer)).await {
                    Either::Left((Ok(answer), _)) => Some(answer),
                    _ => None,
                }
            } else {
                None
            };
            let undo_files = match confirm_for(b.files, answer.as_ref(), live) {
                None => false,
                Some(ask) => {
                    let Ok(rx) =
                        this.update_in(cx, |this, window, cx| this.ask_sheet(&ask, window, cx))
                    else {
                        return;
                    };
                    match rx.await {
                        Ok(i) if Some(i) == ask.undo => true,
                        Ok(i) if i == ask.chat => false,
                        _ => {
                            this.update(cx, |this, cx| this.rewind_ended(None, cx)).ok();
                            return;
                        }
                    }
                }
            };
            let mut note = None;
            if undo_files && live {
                let Ok(rx) = this.update(cx, |this, _| this.ask_files(&message, false)) else {
                    return;
                };
                let timer = cx.background_executor().timer(FILES_WAIT);
                note = match future::select(rx, Box::pin(timer)).await {
                    Either::Left((Ok(done), _)) if done.can_rewind && done.error.is_none() => None,
                    Either::Left((Ok(done), _)) => Some(format!(
                        "Couldn't undo the file changes: {}",
                        done.error.unwrap_or_else(|| "the agent can't".into())
                    )),
                    _ => Some("Couldn't undo the file changes: the agent didn't answer".into()),
                };
            } else if undo_files {
                // No process to ask: the agent's one-off rewind.
                note = match provider.rewind_files_probe(&start, &message) {
                    Some(probe) => {
                        let cwd = start.cwd.clone();
                        cx.background_executor()
                            .spawn(async move {
                                crate::agent_catalog::run_probe(
                                    &probe,
                                    &cwd,
                                    path.as_deref(),
                                    crate::agent_catalog::PROBE_DEADLINE,
                                )
                                .err()
                                .map(|why| format!("Couldn't undo the file changes: {why}"))
                            })
                            .await
                    }
                    None => Some("Couldn't undo the file changes: the agent can't".into()),
                };
            }
            this.update(cx, |this, cx| {
                this.rewound(ix, &message, from, point.parent, note, cx)
            })
            .ok();
        }));
        cx.notify();
    }

    /// The flow ended without a rewind (cancelled, or `why` it couldn't).
    fn rewind_ended(&mut self, why: Option<String>, cx: &mut Context<Self>) {
        self.rw.task = None;
        self.rw.files = None;
        self.rw.turn_done = None;
        if let Some(why) = why {
            self.conversation.apply(Event::Notice(why));
            self.after_change(true, cx);
        }
        cx.notify();
    }

    /// The rewind itself: the chat cut before message `ix` (still `message`), its text back in
    /// the box, the "Rewound" notice (and `note`, a file undo that failed), and the agent started
    /// on a branch of conversation `from` kept up to `at`.
    fn rewound(
        &mut self,
        ix: usize,
        message: &str,
        from: String,
        at: Option<String>,
        note: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.rw.task = None;
        self.rw.files = None;
        if self.conversation.items().get(ix).and_then(branch_id) != Some(message) {
            return;
        }
        let Some((text, attachments)) = self.message_at(ix) else {
            return;
        };
        let Some(cut) = self.conversation.cut_before(ix) else {
            return;
        };
        let notice = self.conversation.items().len();
        self.conversation
            .apply(Event::Notice(REWOUND_NOTICE.into()));
        if let Some(note) = note {
            self.conversation.apply(Event::Notice(note));
        }
        self.rw.undo = Some(Undo {
            from: from.clone(),
            cut,
            notice,
            undoable: true,
        });
        self.rw.branch_next = Some(Branch { from, at });
        self.spawn_process(None, cx);
        self.put_back(text, attachments, cx);
        self.after_change(true, cx);
    }

    /// The "Rewound" notice's index while its Undo is offered.
    pub(super) fn undo_notice(&self) -> Option<usize> {
        self.rw
            .undo
            .as_ref()
            .filter(|u| u.undoable)
            .map(|u| u.notice)
    }

    /// Undo Rewind (until the next send): the chat as it was, and the agent on the old
    /// conversation again. The box empties if it still holds the rewound message.
    pub(crate) fn undo_rewind(&mut self, cx: &mut Context<Self>) {
        if self.undo_notice().is_none() {
            return;
        }
        let Some(undo) = self.rw.undo.take() else {
            return;
        };
        let text = match undo.cut.message() {
            Some(Item::You { text, .. }) => text.clone(),
            _ => String::new(),
        };
        self.go_back_to(undo, cx);
        if self.composer.read(cx).text() == text {
            self.attachments.clear();
            self.composer
                .update(cx, |c, cx| c.set_text(String::new(), cx));
        }
        self.after_change(true, cx);
    }

    /// The conversation `undo` left, back: its items, its id, its process.
    fn go_back_to(&mut self, undo: Undo, cx: &mut Context<Self>) {
        self.rw.branch_next = None;
        self.conversation.restore(undo.cut);
        self.conversation.apply(Event::Started {
            conversation: undo.from.clone(),
            model: None,
        });
        self.unconfirmed = None;
        self.start.resume = Some(undo.from.clone());
        self.spawn_process(Some(undo.from), cx);
    }

    /// The branch a rewind started ended before the agent said it started (`why`): the old
    /// conversation back (never a fresh one), and what was sent to the branch back in the box.
    /// False when this run was no rewind's.
    pub(super) fn rewind_failed(&mut self, why: &str, cx: &mut Context<Self>) -> bool {
        if !self.run.branched || self.run.started {
            return false;
        }
        let Some(undo) = self.rw.undo.take() else {
            return false;
        };
        let sends = std::mem::take(&mut self.run.sends);
        self.go_back_to(undo, cx);
        self.conversation
            .apply(Event::Notice(format!("Couldn't rewind: {why}")));
        if !sends.is_empty() {
            let text = sends
                .iter()
                .map(|(t, _)| t.as_str())
                .collect::<Vec<_>>()
                .join("\n\n");
            let attachments = sends.into_iter().flat_map(|(_, a)| a).collect();
            self.put_back(text, attachments, cx);
        }
        true
    }

    /// Fork (batch 3, chore 2) from before your message `ix`: where it sits is read off the UI
    /// thread, then the Workspace opens a new tab beside this one with the chat before it, the
    /// message in its box unsent, on a branch of this conversation. This tab is left as it is
    /// (a turn running goes on).
    pub(crate) fn fork_from(&mut self, ix: usize, cx: &mut Context<Self>) {
        let Some(provider) = self.provider else {
            return;
        };
        if !provider.branching().fork || self.rw.task.is_some() {
            return;
        }
        let Some(message) = self
            .conversation
            .items()
            .get(ix)
            .and_then(branch_id)
            .map(str::to_string)
        else {
            return;
        };
        let Some(from) = self.conversation_id() else {
            return;
        };
        self.rw.pick = None;
        let start = Start {
            resume: Some(from.clone()),
            branch: None,
            ..self.start.clone()
        };
        let home = self.branch_home();
        self.rw.task = Some(cx.spawn(async move |this, cx| {
            let point = {
                let message = message.clone();
                cx.background_executor()
                    .spawn(async move {
                        let home = home.ok_or("Couldn't find your home folder")?;
                        provider.branch_point(&home, &start, &message)
                    })
                    .await
            };
            this.update(cx, |this, cx| {
                this.rw.task = None;
                match point {
                    Ok(point) => this.forked(ix, &message, from, point.parent, cx),
                    Err(why) => this.rewind_ended(Some(format!("Couldn't fork: {why}")), cx),
                }
            })
            .ok();
        }));
        cx.notify();
    }

    /// The fork's seed, told to the Workspace.
    fn forked(
        &mut self,
        ix: usize,
        message: &str,
        from: String,
        at: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if self.conversation.items().get(ix).and_then(branch_id) != Some(message) {
            return;
        }
        let Some((text, attachments)) = self.message_at(ix) else {
            return;
        };
        let Some(conversation) = self
            .conversation
            .forked_before(ix, Item::Notice(FORKED_NOTICE.into()))
        else {
            return;
        };
        let images = attachments
            .iter()
            .filter_map(|a| match a {
                Attachment::Image { name, picture, .. } => Some((name.clone(), picture.clone())),
                _ => None,
            })
            .collect();
        let seed = ForkSeed {
            conversation,
            branch: Branch { from, at },
            text,
            attachments,
            images,
            pasted: self.pasted,
        };
        cx.emit(AgentTabEvent::Fork(Seed(Rc::new(std::cell::RefCell::new(
            Some(seed),
        )))));
    }

    /// A new fork tab takes its seed (before its history is read): the chat before the message,
    /// the message in the box, the branch its first start makes; `source` names the tab it came
    /// from.
    pub(crate) fn seed_fork(&mut self, seed: ForkSeed, source: String, cx: &mut Context<Self>) {
        self.conversation = seed.conversation;
        self.rw.branch_next = Some(seed.branch);
        self.rw.fork = Some(ForkNote { source, open: true });
        self.pasted = self.pasted.max(seed.pasted);
        self.images.extend(seed.images);
        self.put_back(seed.text, seed.attachments, cx);
        self.after_change(false, cx);
    }

    /// The fork's source tab closed: its name in the first row is no link any more.
    pub(crate) fn fork_source_closed(&mut self, cx: &mut Context<Self>) {
        if let Some(note) = &mut self.rw.fork {
            note.open = false;
            cx.notify();
        }
    }

    /// Esc in the box with nothing else to close: the second within `DOUBLE_ESC`, with the box
    /// empty and Main idle, enters the rewind pick on your newest message. True when Esc was
    /// the pick's (nothing to stop then).
    pub(super) fn esc_rewind(&mut self, cx: &mut Context<Self>) -> bool {
        let ready = self.branching().rewind
            && !matches!(self.status(), Status::Working | Status::Waiting)
            && self.view == crate::agent_lanes::View::Main
            && self.composer.read(cx).text().is_empty()
            && pick_start(self.conversation.items()).is_some();
        if !ready {
            self.rw.last_esc = None;
            return false;
        }
        let now = Instant::now();
        if double_esc(self.rw.last_esc, now) {
            self.rw.last_esc = None;
            self.pick_message(pick_start(self.conversation.items()), cx);
        } else {
            self.rw.last_esc = Some(now);
        }
        true
    }

    /// The pick on message `ix` (None: out of it), the chat following it.
    fn pick_message(&mut self, ix: Option<usize>, cx: &mut Context<Self>) {
        self.rw.pick = ix;
        if let Some(ix) = ix {
            let pane = self.pane();
            if let Ok(row) = pane.order.binary_search(&ix) {
                pane.list.scroll_to_reveal_item(row + pane.head as usize);
            }
        }
        cx.notify();
    }

    /// The box changed: typing leaves the pick (↩ sends again).
    pub(super) fn pick_typed(&mut self, cx: &mut Context<Self>) {
        if self.rw.pick.is_some() && !self.composer.read(cx).text().is_empty() {
            self.rw.pick = None;
            cx.notify();
        }
    }

    pub(super) fn pick_up(&mut self, _: &PickUp, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(from) = self.rw.pick {
            let to = pick_step(self.conversation.items(), from, true).unwrap_or(from);
            self.pick_message(Some(to), cx);
        }
    }

    pub(super) fn pick_down(&mut self, _: &PickDown, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(from) = self.rw.pick {
            let to = pick_step(self.conversation.items(), from, false).unwrap_or(from);
            self.pick_message(Some(to), cx);
        }
    }

    pub(super) fn pick_rewind(
        &mut self,
        _: &PickRewind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(ix) = self.rw.pick.take() {
            self.rewind_to(ix, window, cx);
        }
    }

    pub(super) fn pick_fork(&mut self, _: &PickFork, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(ix) = self.rw.pick.take() {
            self.fork_from(ix, cx);
        }
    }

    pub(super) fn pick_copy(&mut self, _: &PickCopy, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(ix) = self.rw.pick {
            self.copy_message(ix, cx);
        }
    }

    pub(super) fn pick_leave(&mut self, _: &PickLeave, _: &mut Window, cx: &mut Context<Self>) {
        self.pick_message(None, cx);
    }

    /// The palette's Rewind Last Prompt: your newest message that can be rewound.
    pub(super) fn rewind_last(
        &mut self,
        _: &RewindLastPrompt,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(ix) = pick_start(self.conversation.items()) {
            self.rewind_to(ix, window, cx);
        }
    }

    /// The message whose hover actions stay shown: the picked one, or after a stop the one the
    /// "Stopped" notice offers to rewind.
    pub(super) fn actions_kept(&self) -> Option<usize> {
        self.rw.pick.or_else(|| {
            stopped_link(
                self.conversation.items(),
                self.conversation.latest_you(),
                self.branching(),
            )
            .and(self.conversation.latest_you())
        })
    }
}

impl AgentTab {
    /// Scenes: `el` laid out under `label` (`State::shot_buttons`).
    fn shot_marked<E: ParentElement>(&self, label: String, el: E) -> E {
        #[cfg(feature = "ui-shots")]
        {
            let buttons = self.rw.shot_buttons.clone();
            el.child(
                gpui::canvas(
                    move |bounds, _, _| {
                        buttons.borrow_mut().insert(label.clone(), bounds);
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
        }
        #[cfg(not(feature = "ui-shots"))]
        {
            let _ = label;
            el
        }
    }

    /// Your message `ix`'s hover actions (Main only), left of its bubble and bottom-aligned,
    /// over the chat (nothing moves): shown while the pointer is on its row, and kept shown on
    /// the picked message and, after a stop, on the one to rewind.
    pub(super) fn you_actions(&self, ix: usize, item: &Item, look: &Look) -> Option<AnyElement> {
        use crate::agent_chat::chat;
        if self.view != crate::agent_lanes::View::Main {
            return None;
        }
        let b = self.branching();
        // A tab paused for its classic view can't rewind (that process holds the conversation).
        let mut actions: Vec<Hover> = hover_actions(item, b)
            .into_iter()
            .filter(|h| !(self.paused && *h == Hover::Rewind))
            .collect();
        // × while the agent hasn't read it and can take it back (batch 3, wave 3).
        if self.cancel_offered(item) {
            actions.push(Hover::Cancel);
        }
        if actions.is_empty() {
            return None;
        }
        let kept = self.actions_kept() == Some(ix);
        let c = look.c;
        let edit = self.provider.and_then(|p| p.edit_tip());
        let buttons: Vec<AnyElement> = actions
            .into_iter()
            .map(|h| {
                let (glyph, name) = match h {
                    Hover::Copy => (Icon::Copy, "copy"),
                    Hover::Rewind => (Icon::Revert, "rewind"),
                    Hover::Fork => (Icon::Fork, "fork"),
                    Hover::Edit => (Icon::FilePen, "edit"),
                    Hover::Cancel => (Icon::Stop, "cancel"),
                };
                let group = SharedString::from(format!("you-act-{name}-{ix}"));
                let tip = SharedString::from(hover_tip(h, b.files, edit));
                let view = look.view.clone();
                let button = div()
                    .id(group.clone())
                    .group(group.clone())
                    .flex_none()
                    .size(px(24.))
                    .rounded(px(chat::R_CHIP))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .hover(move |s| s.bg(hsla(c.card_hover)))
                    .tooltip(move |_, cx| {
                        cx.new(|_| crate::sidebar_toggle::Tip(tip.clone())).into()
                    })
                    .on_click(move |_, window, cx| {
                        view.update(cx, |this, cx| match h {
                            Hover::Copy => this.copy_message(ix, cx),
                            Hover::Rewind => this.rewind_to(ix, window, cx),
                            Hover::Fork => this.fork_from(ix, cx),
                            Hover::Edit => this.edit_message(ix, cx),
                            Hover::Cancel => this.cancel_message(ix, cx),
                        })
                        .ok();
                    })
                    .child(if h == Hover::Cancel {
                        // A × (the set has no cross icon), drawn as the chips' × are.
                        div()
                            .text_size(px(14.))
                            .text_color(hsla(c.dim))
                            .group_hover(group, move |s| s.text_color(hsla(c.text)))
                            .child("\u{d7}")
                            .into_any_element()
                    } else {
                        icon(glyph, px(14.), hsla(c.dim))
                            .group_hover(group, move |s| s.text_color(hsla(c.text)))
                            .into_any_element()
                    });
                self.shot_marked(format!("{name}:{ix}"), button)
                    .into_any_element()
            })
            .collect();
        Some(
            div()
                .absolute()
                .right_full()
                .bottom_0()
                .pr(px(6.))
                .flex()
                .flex_row()
                .gap(px(2.))
                .block_mouse_except_scroll()
                // Transparent, not hidden, at rest: a hidden element registers no group hover,
                // so the row's hover would never redraw it.
                .when(!kept, |d| {
                    d.opacity(0.).group_hover(YOU_ROW, |s| s.opacity(1.))
                })
                .children(buttons)
                .into_any_element(),
        )
    }

    /// Whether your message `ix` is the picked one (its bubble ringed).
    pub(super) fn picked(&self, ix: usize) -> bool {
        self.rw.pick == Some(ix)
    }

    /// A link in a notice's words (the link ink, underlined under the pointer).
    fn notice_link(
        &self,
        id: &'static str,
        ix: usize,
        label: impl Into<SharedString>,
        look: &Look,
        f: impl Fn(&mut AgentTab, &mut Window, &mut Context<AgentTab>) + 'static,
    ) -> AnyElement {
        let view = look.view.clone();
        let link = div()
            .id((id, ix))
            .text_color(hsla(look.draw.colours.link))
            .cursor_pointer()
            .hover(|s| s.underline())
            .on_click(move |_, window, cx| {
                view.update(cx, |this, cx| f(this, window, cx)).ok();
            })
            .child(label.into());
        self.shot_marked(format!("{id}:{ix}"), link)
            .into_any_element()
    }

    /// What follows notice `ix`'s words: after a stop, "· Rewind to edit your message"; after a
    /// rewind, "· Undo" (until the next send).
    pub(super) fn notice_action(&self, ix: usize, text: &str, look: &Look) -> Option<AnyElement> {
        let link = if text == STOPPED_NOTICE
            && stopped_link(
                self.conversation.items(),
                self.conversation.latest_you(),
                self.branching(),
            ) == Some(ix)
        {
            let you = self.conversation.latest_you()?;
            self.notice_link(
                "stopped-rewind",
                ix,
                STOPPED_LINK,
                look,
                move |this, w, cx| this.rewind_to(you, w, cx),
            )
        } else if text == REWOUND_NOTICE && self.undo_notice() == Some(ix) {
            self.notice_link("rewound-undo", ix, "Undo", look, |this, _, cx| {
                this.undo_rewind(cx)
            })
        } else {
            return None;
        };
        Some(
            div()
                .flex()
                .flex_row()
                .gap(px(4.))
                .child("\u{b7}")
                .child(link)
                .into_any_element(),
        )
    }

    /// A fork's first row (`FORKED_NOTICE` first in its chat): "Forked from ⟨source⟩" (the name
    /// shows that tab while it is open) and, under it, that both share the folder.
    pub(super) fn fork_row(&self, ix: usize, text: &str, look: &Look) -> Option<AnyElement> {
        let note = self
            .rw
            .fork
            .as_ref()
            .filter(|_| ix == 0 && text == FORKED_NOTICE)?;
        let c = look.c;
        let name: AnyElement = if note.open {
            self.notice_link("fork-source", ix, note.source.clone(), look, |_, _, cx| {
                cx.emit(AgentTabEvent::ShowForkSource)
            })
        } else {
            div()
                .text_color(hsla(c.text))
                .child(note.source.clone())
                .into_any_element()
        };
        Some(
            div()
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
                        .child(icon(Icon::Fork, px(12.), hsla(c.dim))),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .gap(px(4.))
                                .child("Forked from")
                                .child(name),
                        )
                        .child(SharedString::from(shares_folder(&note.source))),
                )
                .into_any_element(),
        )
    }
}

/// The group a row of yours is (its hover actions show on it).
pub(super) const YOU_ROW: &str = "agent-you-row";

#[cfg(test)]
mod tests {
    use super::*;

    fn you(id: Option<&str>, taken: bool) -> Item {
        Item::You {
            text: "fix it".into(),
            attachments: Vec::new(),
            id: id.map(str::to_string),
            taken,
            to: None,
        }
    }

    fn notice(t: &str) -> Item {
        Item::Notice(t.into())
    }

    const CLAUDE: Branching = Branching {
        rewind: true,
        fork: true,
        files: true,
    };
    const NONE: Branching = Branching {
        rewind: false,
        fork: false,
        files: false,
    };

    #[test]
    fn a_read_message_offers_copy_rewind_and_fork_and_an_unread_one_copy_only() {
        assert_eq!(
            hover_actions(&you(Some("u1"), true), CLAUDE),
            [Hover::Copy, Hover::Rewind, Hover::Fork]
        );
        assert_eq!(
            hover_actions(&you(Some("u1"), false), CLAUDE),
            [Hover::Copy],
            "not echoed yet"
        );
        assert_eq!(
            hover_actions(&you(None, true), CLAUDE),
            [Hover::Copy],
            "no id"
        );
        assert!(hover_actions(&notice("x"), CLAUDE).is_empty());
    }

    #[test]
    fn an_agent_that_cannot_go_back_offers_edit() {
        assert_eq!(
            hover_actions(&you(None, true), NONE),
            [Hover::Copy, Hover::Edit]
        );
        assert_eq!(
            hover_actions(&you(None, false), NONE),
            [Hover::Copy, Hover::Edit]
        );
    }

    #[test]
    fn the_rewind_tip_says_whether_files_go_back_too() {
        assert_eq!(
            hover_tip(Hover::Rewind, true, None),
            "Rewind chat and files to here"
        );
        assert_eq!(
            hover_tip(Hover::Rewind, false, None),
            "Rewind the chat to here (files stay as they are)"
        );
        assert_eq!(
            hover_tip(Hover::Fork, true, None),
            "Fork from here into a new tab"
        );
        assert_eq!(
            hover_tip(Hover::Edit, false, Some("own words")),
            "own words"
        );
    }

    #[test]
    fn the_stopped_notice_links_to_rewind_until_the_next_send() {
        let items = vec![you(Some("u1"), true), notice("Hi"), notice(STOPPED_NOTICE)];
        assert_eq!(stopped_link(&items, Some(0), CLAUDE), Some(2));
        assert_eq!(stopped_link(&items, Some(0), NONE), None, "agy: no link");
        // The next message is the latest: the old stop links no more.
        let mut sent = items.clone();
        sent.push(you(Some("u2"), false));
        assert_eq!(stopped_link(&sent, Some(3), CLAUDE), None);
        // A stop before your message was read: nothing to rewind to.
        let unread = vec![you(Some("u1"), false), notice(STOPPED_NOTICE)];
        assert_eq!(stopped_link(&unread, Some(0), CLAUDE), None);
        assert_eq!(stopped_link(&items, None, CLAUDE), None);
    }

    /// The latest-prompt strip's Rewind (the opens fixed, 2026-11-23): after a stop, in Main,
    /// for the message the "Stopped" notice offers, while the provider can rewind and the
    /// message was read; gone after the next send, in a lane's view and in a paused tab.
    #[test]
    fn the_strip_offers_rewind_while_the_stopped_notice_does() {
        let items = vec![you(Some("u1"), true), notice("Hi"), notice(STOPPED_NOTICE)];
        assert_eq!(strip_rewind(&items, Some(0), CLAUDE, true, false), Some(0));
        assert_eq!(
            strip_rewind(&items, Some(0), NONE, true, false),
            None,
            "agy"
        );
        assert_eq!(
            strip_rewind(&items, Some(0), CLAUDE, false, false),
            None,
            "a lane's view"
        );
        assert_eq!(
            strip_rewind(&items, Some(0), CLAUDE, true, true),
            None,
            "a paused tab"
        );
        let mut sent = items.clone();
        sent.push(you(Some("u2"), false));
        assert_eq!(strip_rewind(&sent, Some(3), CLAUDE, true, false), None);
        let unread = vec![you(Some("u1"), false), notice(STOPPED_NOTICE)];
        assert_eq!(strip_rewind(&unread, Some(0), CLAUDE, true, false), None);
    }

    #[test]
    fn a_fork_is_titled_after_its_source_and_counts_on() {
        assert_eq!(fork_title("Fix the tests"), "Fix the tests (fork)");
        assert_eq!(fork_title("Fix the tests (fork)"), "Fix the tests (fork 2)");
        assert_eq!(
            fork_title("Fix the tests (fork 2)"),
            "Fix the tests (fork 3)"
        );
        assert_eq!(fork_title("Odd (fork x)"), "Odd (fork x) (fork)");
    }

    #[test]
    fn a_fork_tip_names_its_source_and_message() {
        assert_eq!(
            fork_tip("Claude", "\nadd a test\nmore"),
            "Forked from Claude at \u{201c}add a test\u{201d}"
        );
        let long = "a".repeat(50);
        assert_eq!(
            fork_tip("Claude", &long),
            format!("Forked from Claude at \u{201c}{}…\u{201d}", "a".repeat(40))
        );
        assert_eq!(
            shares_folder("Claude"),
            "Shares the folder with Claude: both agents can edit the same files."
        );
    }

    fn answer(can: bool, files: &[&str], lines: Option<(u64, u64)>) -> FileRewind {
        FileRewind {
            can_rewind: can,
            error: None,
            files: files.iter().map(|f| f.to_string()).collect(),
            insertions: lines.map(|l| l.0),
            deletions: lines.map(|l| l.1),
        }
    }

    #[test]
    fn no_file_changed_rewinds_in_one_click() {
        assert_eq!(
            confirm_for(true, Some(&answer(true, &[], None)), true),
            None
        );
        assert_eq!(
            confirm_for(true, Some(&answer(false, &[], None)), true),
            None,
            "it can't tell: nothing to undo"
        );
        assert_eq!(confirm_for(true, None, true), None, "no answer in time");
        assert_eq!(
            confirm_for(false, None, false),
            None,
            "an agent that can't undo files never asks"
        );
    }

    #[test]
    fn changed_files_the_agent_can_undo_ask_with_three_buttons() {
        let ask = confirm_for(
            true,
            Some(&answer(true, &["a.rs", "b.rs"], Some((12, 4)))),
            true,
        )
        .expect("a sheet");
        assert_eq!(ask.title, "Rewind to this message?");
        assert_eq!(
            ask.detail,
            "The agent changed 2 files after it (+12 \u{2212}4). Undo those changes too, or keep \
             them and rewind only the chat."
        );
        assert_eq!(
            ask.buttons,
            ["Rewind and Undo Changes", "Rewind Chat Only", "Cancel"]
        );
        assert_eq!((ask.undo, ask.chat), (Some(0), 1));
        let one = confirm_for(true, Some(&answer(true, &["a.rs"], None)), true).unwrap();
        assert!(one.detail.starts_with("The agent changed 1 file after it."));
    }

    #[test]
    fn changed_files_the_agent_cannot_undo_say_so() {
        let ask = confirm_for(true, Some(&answer(false, &["a.rs"], Some((1, 0)))), true)
            .expect("a sheet");
        assert_eq!(
            ask.detail,
            "The agent changed 1 file after it (+1 \u{2212}0). It can't undo file changes from \
             fuxx, so they stay as they are. You can revert them in Changes."
        );
        assert_eq!(ask.buttons, ["Rewind Chat", "Cancel"]);
        assert_eq!((ask.undo, ask.chat), (None, 0));
    }

    #[test]
    fn without_a_process_the_files_may_have_changed() {
        let ask = confirm_for(true, None, false).expect("a sheet");
        assert!(
            ask.detail
                .starts_with("The agent may have changed files after it.")
        );
        assert_eq!(ask.undo, Some(0));
    }

    #[test]
    fn the_pick_moves_between_messages_that_can_be_rewound() {
        let items = vec![
            you(Some("u1"), true),
            notice("a"),
            you(Some("u2"), false),
            you(Some("u3"), true),
            notice("b"),
        ];
        assert_eq!(pick_start(&items), Some(3));
        assert_eq!(pick_step(&items, 3, true), Some(0), "skips the unread one");
        assert_eq!(pick_step(&items, 0, true), None);
        assert_eq!(pick_step(&items, 0, false), Some(3));
        assert_eq!(pick_step(&items, 3, false), None);
        assert_eq!(pick_start(&[notice("a")]), None);
    }

    #[test]
    fn esc_twice_is_two_within_400_ms() {
        let t = Instant::now();
        assert!(double_esc(Some(t), t + Duration::from_millis(400)));
        assert!(!double_esc(Some(t), t + Duration::from_millis(401)));
        assert!(!double_esc(None, t));
    }
}
