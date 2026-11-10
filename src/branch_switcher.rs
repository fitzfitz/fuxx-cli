//! M11o: the branch switcher (note 66), a panel where the ⌘K palette opens: the repo's local
//! branches (marked current, ahead/behind, local only, upstream gone), then origin's that
//! aren't local, under a box that filters them and offers a typed name to check out from
//! origin. A branch the switch would collide with (uncommitted files it also changes) is
//! disabled with "conflict x file(s)". Choosing a branch asks first, every time (§4): the
//! confirmation step says what runs in the folder and what comes along; only its Switch runs
//! `branch_git::switch`, and only git's yes closes the panel (`Switched`). git's no is shown
//! under the box, on the list, and the list is read again (§6).
//!
//! Everything git does runs on the background executor: one read as it opens, the automatic
//! fetch (and the Fetch button's) with a read after it, the conflict counts (only with a dirty
//! work tree, only for the rows in view, at most `COUNTS_AT_ONCE` at a time), a chosen row's
//! count when it hadn't landed, a typed name's fetch, and the switch. Each task holds only a
//! weak handle: once the switcher is closed or replaced, its answers are dropped and nothing new
//! starts. The box is the palette's (`NameInput`): on the list its Escape, losing focus or a
//! click outside closes the switcher; on the confirmation Escape goes back to the list. While
//! git works on a choice (`busy`) every key and click is ignored, Escape too: a switch can't be
//! called back half way.

use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::ops::RangeInclusive;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use gpui::{
    AnyElement, App, Bounds, ClickEvent, Context, Entity, EventEmitter, FocusHandle, Focusable,
    FontWeight, MouseDownEvent, Pixels, ScrollHandle, Subscription, Window, actions, canvas, div,
    prelude::*, px,
};

use crate::appearance::Appearance;
use crate::branch_git::{self, FetchMode, Snapshot};
use crate::branch_list::{self, Mark, Row, Target};
use crate::git_branch::branch_at;
use crate::help_panel::render_group;
use crate::host_picker::move_selection;
use crate::name_input::{NameInput, NameInputEvent};
use crate::overlay_panel;
use crate::palette::{PANEL_WIDTH, item_frame, item_row, list_max_height, name_cell};
use crate::procs::ProgramState;
use crate::sidebar::{home_relative, tabular};
use crate::switcher::{ROW, ROW_GAP};
use crate::terminal_view::hsla;
use crate::theme::UiColors;
use crate::ui_tokens::text;
use crate::workspace::{Overlay, Workspace};

// Bound in main.rs with the "BranchSwitcher" key context; Enter and Escape are the box's own.
actions!(branch_switcher, [SelectPrev, SelectNext]);

const PLACEHOLDER: &str = "type a branch name\u{2026}";
const ORIGIN_HEADING: &str = "ORIGIN";
const FETCH: &str = "Fetch";
const FETCHING: &str = "Fetching\u{2026}";
/// How many conflict counts run at once.
const COUNTS_AT_ONCE: usize = 4;
/// The Fetch button's height (the palette's chips'), and the confirmation's buttons'.
const BUTTON: f32 = 22.0;
const CANCEL: &str = "Cancel";
const SWITCH: &str = "Switch";
const COUNTING: &str = "Counting conflicts\u{2026}";
const SWITCHING: &str = "Switching\u{2026}";

/// The repo a switcher opens on: git, the terminal's folder in it, and that folder as the
/// sidebar shows it (home-relative).
pub struct RepoAt {
    pub git: PathBuf,
    pub dir: PathBuf,
    pub label: String,
}

pub enum BranchSwitcherEvent {
    /// The repo at `root` is on another branch now.
    Switched { root: PathBuf },
    /// Escape, a click outside, the box lost focus, or the folder is no repo any more.
    Cancel,
}

/// A line of the list: a branch's row (a place in `rows`), the ORIGIN heading, or the typed
/// name to check out from origin.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Line {
    Row(usize),
    OriginHeading,
    Typed(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum FetchState {
    Idle,
    Running,
    /// git's first line.
    Failed(String),
}

/// What the panel shows under its box: the list, or the confirmation of one branch.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Step {
    List,
    Confirm(Confirm),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Confirm {
    target: Target,
    /// `Switch to <name>?`, then its warnings (`confirm_lines`), or its conflict.
    lines: Vec<String>,
    /// Not for a typed branch that would conflict (only Cancel then).
    can_switch: bool,
    /// Reached from the typed name's row (a conflict then shows here, not on a row).
    typed: bool,
}

/// A typed name's fetch, and what came of it.
enum Typed {
    /// On origin now: the repo read again (none if that failed) and the branch's conflict
    /// count (none: unknown).
    Found {
        name: String,
        snapshot: Option<Box<Snapshot>>,
        count: Option<usize>,
    },
    NotFound(String),
    /// git's first line.
    Failed(String),
}

/// The typed name's row.
fn typed_text(name: &str) -> String {
    format!("Check out {name} from origin")
}

fn not_found(name: &str) -> String {
    format!("Branch {name} not found")
}

fn looking_for(name: &str) -> String {
    format!("Looking for {name} on origin\u{2026}")
}

/// The confirmation of `target`: with a conflict (a typed branch's), its title and the count,
/// and no Switch; else its title and warnings, `carried` = the dirty files the target doesn't
/// change (all of them while its count is unknown: git has the last word).
fn confirmation(
    target: Target,
    conflict: Option<usize>,
    programs: &[String],
    dirty: usize,
    orphans: usize,
) -> Confirm {
    let name = target.name();
    let (lines, can_switch) = match conflict {
        Some(n) if n > 0 => {
            let mut lines = branch_list::confirm_lines(name, &[], 0, 0);
            lines.push(branch_list::conflict_text(n));
            (lines, false)
        }
        _ => {
            let carried = dirty.saturating_sub(conflict.unwrap_or(0));
            (
                branch_list::confirm_lines(name, programs, carried, orphans),
                true,
            )
        }
    };
    Confirm {
        target,
        lines,
        can_switch,
        typed: false,
    }
}

/// Whether `folder` is inside the repo whose top folder is `root` (as git gives it, the real
/// path): by its path, or by its real one (a folder reached through a symlink, like macOS's
/// `/var` for `/private/var`).
fn in_repo(folder: &Path, root: &Path) -> bool {
    folder.starts_with(root)
        || std::fs::canonicalize(folder).is_ok_and(|real| real.starts_with(root))
}

fn fetch_failed(why: &str) -> String {
    format!("Fetch failed: {why}")
}

/// A folder's own name (`/` for the disk's top).
fn folder_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "/".into())
}

/// A row's text at its right: its conflict count while it has one, else its mark.
fn right_text(row: &Row) -> Option<String> {
    match row.conflict.filter(|&n| n > 0) {
        Some(n) => Some(branch_list::conflict_text(n)),
        None => branch_list::mark_text(&row.mark),
    }
}

/// The lines in view before the list has been drawn: as many as fit in its tallest.
fn unlaid_view() -> RangeInclusive<usize> {
    let fit = (list_max_height(f32::MAX) / (ROW + ROW_GAP)).ceil() as usize;
    0..=fit.saturating_sub(1)
}

/// The branches to count next: the rows on lines `view` (not the current branch, not the
/// heading or the typed name) not `asked` yet, top first, at most `room`.
fn to_count(
    lines: &[Line],
    rows: &[Row],
    view: RangeInclusive<usize>,
    asked: &HashSet<String>,
    room: usize,
) -> Vec<Target> {
    lines
        .iter()
        .enumerate()
        .filter(|(at, _)| view.contains(at))
        .filter_map(|(_, line)| match line {
            Line::Row(i) => Some(&rows[*i]),
            Line::OriginHeading | Line::Typed(_) => None,
        })
        .filter(|r| r.mark != Mark::Current && !asked.contains(r.target.name()))
        .map(|r| r.target.clone())
        .take(room)
        .collect()
}

/// The ⌘K-like panel that switches a repo's branch.
pub struct BranchSwitcher {
    repo: RepoAt,
    /// The programs running in this repo's terminals (the confirmation's warning).
    programs: Vec<String>,
    /// The programs running in any terminal (folder, label): those in this repo (known once it
    /// is read) are named too.
    running: Vec<(PathBuf, String)>,
    /// Those of `running` inside the repo as last read (looked up once per read, not per frame).
    here: Vec<String>,
    /// git may run (a seeded switcher, for the screenshots, runs none).
    live: bool,
    /// A seeded switcher's switch: git's message to fail with, or none to succeed.
    fail_switch: Option<String>,
    step: Step,
    /// What git is doing for a choice (counting, fetching a typed name, switching): every key
    /// and click is ignored meanwhile.
    busy: Option<String>,
    /// git's refusal, or a typed name not found, under the box (until the next choice or
    /// typing).
    note: Option<String>,
    input: Entity<NameInput>,
    /// The box's text as last listed.
    query: String,
    /// The last read; none until the first lands.
    snapshot: Option<Snapshot>,
    /// The snapshot's dirty files, shared with the counts.
    dirty: Arc<HashSet<PathBuf>>,
    /// Counted conflicts by branch name (a name missing: not counted, or unknown). A count
    /// of an older read stays shown until this read's lands.
    conflicts: HashMap<String, usize>,
    /// The names whose count in `conflicts` is of the current read.
    fresh: HashSet<String>,
    /// A typed name waiting for the running fetch to land before its own fetch starts.
    pending_typed: Option<String>,
    rows: Vec<Row>,
    lines: Vec<Line>,
    /// Each line's matched letters (a row's name, for the box's text).
    matches: Vec<Vec<usize>>,
    /// Each line's place in `selectable` (none: it can't be chosen).
    places: Vec<Option<usize>>,
    /// The places in `lines` of the rows that can be chosen.
    selectable: Vec<usize>,
    /// The chosen one, a place in `selectable`.
    selected: usize,
    fetch: FetchState,
    /// Bumped with each read: a count asked of an older one is dropped.
    generation: u64,
    /// The branches whose count this read has asked for (running or answered).
    asked: HashSet<String>,
    /// Counts running (of any read).
    counting: usize,
    scroll: ScrollHandle,
    /// Where the panel and the Fetch button were last drawn (the UI screenshots read them).
    panel_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    fetch_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    /// The confirmation's buttons and the line under the box, where last drawn.
    cancel_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    switch_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    note_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    _input_events: Subscription,
    _input_changes: Subscription,
}

impl EventEmitter<BranchSwitcherEvent> for BranchSwitcher {}

impl Focusable for BranchSwitcher {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.input.focus_handle(cx)
    }
}

/// The labels of the programs running in `running` (folder, label) whose folder is in the repo
/// at `root`.
fn programs_under(running: &[(PathBuf, String)], root: &Path) -> Vec<String> {
    running
        .iter()
        .filter(|(folder, _)| in_repo(folder, root))
        .map(|(_, label)| label.clone())
        .collect()
}

impl BranchSwitcher {
    /// Open on `repo`: read it on the background executor, then fetch origin (if it has one)
    /// and read again after the fetch.
    pub fn new(
        repo: RepoAt,
        programs: Vec<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let (git, dir) = (repo.git.clone(), repo.dir.clone());
        Self::reading(
            repo,
            programs,
            move || branch_git::read(&git, &dir),
            window,
            cx,
        )
    }

    /// `new`, with `read` as its first read.
    fn reading(
        repo: RepoAt,
        programs: Vec<String>,
        read: impl FnOnce() -> Option<Snapshot> + Send + 'static,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut this = Self::make(repo, programs, true, window, cx);
        this.read_with(read, true, cx);
        this
    }

    fn make(
        repo: RepoAt,
        programs: Vec<String>,
        live: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let input = cx.new(|cx| NameInput::new(String::new(), window, cx));
        let input_events = cx.subscribe_in(&input, window, |this, input, event, window, cx| {
            match event {
                NameInputEvent::Confirm(_) => this.enter(cx),
                // Escape, or the box lost the keyboard (then the switcher closes, even from
                // the confirmation).
                NameInputEvent::Cancel => {
                    let blurred = !input.focus_handle(cx).is_focused(window);
                    this.escape(blurred, cx)
                }
            }
        });
        let input_changes = cx.observe(&input, |this, _, cx| this.query_changed(cx));
        Self {
            repo,
            programs,
            running: Vec::new(),
            here: Vec::new(),
            live,
            fail_switch: None,
            step: Step::List,
            busy: None,
            note: None,
            input,
            query: String::new(),
            snapshot: None,
            dirty: Arc::default(),
            conflicts: HashMap::new(),
            fresh: HashSet::new(),
            pending_typed: None,
            rows: Vec::new(),
            lines: Vec::new(),
            matches: Vec::new(),
            places: Vec::new(),
            selectable: Vec::new(),
            selected: 0,
            fetch: FetchState::Idle,
            generation: 0,
            asked: HashSet::new(),
            counting: 0,
            scroll: ScrollHandle::new(),
            panel_bounds: Rc::default(),
            fetch_bounds: Rc::default(),
            cancel_bounds: Rc::default(),
            switch_bounds: Rc::default(),
            note_bounds: Rc::default(),
            _input_events: input_events,
            _input_changes: input_changes,
        }
    }

    /// Name too the programs of `running` (folder, label) that run inside the repo (checked
    /// once its top folder is read, so it counts from the first read on).
    pub fn with_running(mut self, running: Vec<(PathBuf, String)>) -> Self {
        self.running = running;
        self
    }

    /// The programs running in this repo's terminals, for the confirmation's warning.
    fn programs_here(&self) -> Vec<String> {
        let mut programs = self.programs.clone();
        programs.extend(self.here.iter().cloned());
        programs
    }

    /// `Switch branch · <repo folder>` (the repo's name once it has been read).
    pub fn title(&self) -> String {
        match &self.snapshot {
            Some(s) => format!("Switch branch \u{b7} {}", folder_name(&s.root)),
            None => "Switch branch".into(),
        }
    }

    /// Run `read` on the background executor; its answer lands in `got_snapshot` unless the
    /// switcher is gone by then. `first`: the read as it opens (the automatic fetch follows).
    fn read_with(
        &mut self,
        read: impl FnOnce() -> Option<Snapshot> + Send + 'static,
        first: bool,
        cx: &mut Context<Self>,
    ) {
        let task = cx.background_executor().spawn(async move { read() });
        cx.spawn(async move |this, cx| {
            let snapshot = task.await;
            let _ = this.update(cx, |this, cx| this.got_snapshot(snapshot, first, cx));
        })
        .detach();
    }

    /// A read landed. None (the folder is no repo any more): the first read closes the
    /// switcher, a later one leaves the list as it was.
    fn got_snapshot(&mut self, snapshot: Option<Snapshot>, first: bool, cx: &mut Context<Self>) {
        let Some(snapshot) = snapshot else {
            if self.snapshot.is_none() {
                cx.emit(BranchSwitcherEvent::Cancel);
            }
            return;
        };
        let fetch = first && snapshot.has_origin;
        self.set_snapshot(snapshot);
        // A confirmation showing is rebuilt from the new read (what comes along, commits on no
        // branch, a conflict now); not while git works on it (a switch, a count: a count of the
        // old read is counted again when it lands).
        if self.busy.is_none()
            && let Step::Confirm(c) = &self.step
        {
            let (target, typed) = (c.target.clone(), c.typed);
            if self.listed(&target) {
                self.confirm_fresh(target, typed, cx);
            } else {
                self.step = Step::List;
            }
        }
        if fetch {
            self.start_fetch(FetchMode::Auto, cx);
        }
        // The counts start once the list is drawn (`render`): only rows in view are counted.
        cx.notify();
    }

    /// Show `snapshot`. Its branches are counted again (with a dirty tree) as they come into
    /// view: the counts of the branches still there stay shown until theirs land; a clean tree
    /// has none.
    fn set_snapshot(&mut self, snapshot: Snapshot) {
        self.generation += 1;
        self.here = programs_under(&self.running, &snapshot.root);
        self.dirty = Arc::new(snapshot.dirty.clone());
        if snapshot.dirty.is_empty() {
            self.conflicts.clear();
        } else {
            let names: HashSet<&str> = snapshot
                .local
                .iter()
                .map(|l| l.name.as_str())
                .chain(snapshot.origin.iter().map(|r| r.name.as_str()))
                .collect();
            self.conflicts
                .retain(|name, _| names.contains(name.as_str()));
        }
        self.snapshot = Some(snapshot);
        self.asked.clear();
        self.fresh.clear();
        self.rebuild_rows();
        self.relist(true);
    }

    fn rebuild_rows(&mut self) {
        let Some(s) = &self.snapshot else {
            return;
        };
        self.rows = branch_list::rows(&s.head, &s.local, &s.origin, &s.pushed, &self.conflicts);
    }

    /// Whether counts may be wanted: live, a dirty tree, room for one, and a branch not yet
    /// asked for (whether it is in view is `count_conflicts`'s to find).
    fn may_count(&self) -> bool {
        self.live
            && !self.dirty.is_empty()
            && self.counting < COUNTS_AT_ONCE
            && self
                .rows
                .iter()
                .any(|r| r.mark != Mark::Current && !self.asked.contains(r.target.name()))
    }

    /// The lines in view at the last draw (`unlaid_view` before the first).
    fn view(&self) -> RangeInclusive<usize> {
        match self.scroll.bounds_for_item(0) {
            Some(_) => self.scroll.top_item()..=self.scroll.bottom_item(),
            None => unlaid_view(),
        }
    }

    /// Count the conflicts of the branches in view not asked for yet, until `COUNTS_AT_ONCE`
    /// run. Each answer, a scroll, a move or a new filter (each draws the list, see `render`)
    /// counts the rows come into view.
    fn count_conflicts(&mut self, cx: &mut Context<Self>) {
        if !self.may_count() {
            return;
        }
        let Some(root) = self.snapshot.as_ref().map(|s| s.root.clone()) else {
            return;
        };
        let room = COUNTS_AT_ONCE - self.counting;
        for target in to_count(&self.lines, &self.rows, self.view(), &self.asked, room) {
            self.asked.insert(target.name().to_string());
            self.counting += 1;
            let (git, root, dirty) = (self.repo.git.clone(), root.clone(), self.dirty.clone());
            let generation = self.generation;
            let task = cx.background_executor().spawn(async move {
                let count = branch_git::changed(&git, &root, &target)
                    .map(|changed| branch_list::conflict_count(&dirty, &changed));
                (target, count)
            });
            cx.spawn(async move |this, cx| {
                let (target, count) = task.await;
                let _ = this.update(cx, |this, cx| this.counted(generation, target, count, cx));
            })
            .detach();
        }
    }

    /// A count landed: shown if it is of the current read; unknown (none) clears an older count.
    fn counted(
        &mut self,
        generation: u64,
        target: Target,
        count: Option<usize>,
        cx: &mut Context<Self>,
    ) {
        self.counting -= 1;
        if generation == self.generation {
            let name = target.name().to_string();
            let before = self.conflicts.get(&name).copied();
            match count {
                Some(count) => {
                    self.fresh.insert(name.clone());
                    self.conflicts.insert(name, count)
                }
                None => {
                    self.fresh.remove(&name);
                    self.conflicts.remove(&name)
                }
            };
            if before != count {
                self.rebuild_rows();
                self.relist(true);
            }
        }
        // Drawn again, the list counts the next rows in view (`render`).
        cx.notify();
    }

    /// Fetch origin in the background (one at a time), then read again.
    fn start_fetch(&mut self, mode: FetchMode, cx: &mut Context<Self>) {
        if self.fetch == FetchState::Running {
            return;
        }
        // A seeded switcher (the screenshots) runs no git: it only shows the fetch running.
        if !self.live {
            self.fetch = FetchState::Running;
            cx.notify();
            return;
        }
        let Some(root) = self
            .snapshot
            .as_ref()
            .filter(|s| s.has_origin)
            .map(|s| s.root.clone())
        else {
            return;
        };
        self.fetch = FetchState::Running;
        let git = self.repo.git.clone();
        let task = cx.background_executor().spawn({
            let root = root.clone();
            async move { branch_git::fetch(&git, &root, mode) }
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                this.fetch = match result {
                    Ok(()) => FetchState::Idle,
                    Err(why) => FetchState::Failed(why),
                };
                let git = this.repo.git.clone();
                this.read_with(move || branch_git::read(&git, &root), false, cx);
                // A typed name waited for this fetch: its own starts now.
                if let Some(name) = this.pending_typed.take() {
                    this.run_typed_fetch(name, cx);
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    /// The box changed: list again if its text did, from the first row (a note under the box
    /// goes). On the confirmation, or while git works on a choice, typing is undone.
    fn query_changed(&mut self, cx: &mut Context<Self>) {
        let query = self.input.read(cx).text().to_string();
        if query == self.query {
            return;
        }
        if self.step != Step::List || self.busy.is_some() {
            let back = self.query.clone();
            self.input.update(cx, |input, cx| {
                input.set_text(back);
                cx.notify();
            });
            return;
        }
        self.query = query;
        self.note = None;
        self.relist(false);
        cx.notify();
    }

    /// Enter: on the list, choose the chosen row; on the confirmation, Switch.
    fn enter(&mut self, cx: &mut Context<Self>) {
        if self.busy.is_some() {
            return;
        }
        match self.step {
            Step::List => self.choose(cx),
            Step::Confirm(_) => self.start_switch(cx),
        }
    }

    /// Escape (or Cancel): from the confirmation back to the list (the chosen row kept); on
    /// the list, close. `blurred`: the box lost the keyboard, which closes it from either.
    /// Nothing while git works on a choice.
    fn escape(&mut self, blurred: bool, cx: &mut Context<Self>) {
        if self.busy.is_some() {
            return;
        }
        match self.step {
            Step::Confirm(_) if !blurred => {
                self.step = Step::List;
                cx.notify();
            }
            _ => cx.emit(BranchSwitcherEvent::Cancel),
        }
    }

    /// The chosen row (Enter, a click): its confirmation, after what it needs first. A branch
    /// row's conflict count, if it hasn't landed with a dirty tree, is counted first (a
    /// conflict then disables the row, and the list stays); a typed name is fetched first.
    fn choose(&mut self, cx: &mut Context<Self>) {
        if self.busy.is_some() || self.step != Step::List {
            return;
        }
        let Some(line) = self
            .selectable
            .get(self.selected)
            .and_then(|&at| self.lines.get(at))
            .cloned()
        else {
            return;
        };
        self.note = None;
        match line {
            Line::Row(i) => {
                let target = self.rows[i].target.clone();
                self.confirm_fresh(target, false, cx);
            }
            Line::Typed(name) => self.fetch_typed(name, cx),
            Line::OriginHeading => {}
        }
        cx.notify();
    }

    /// Whether `target` is a row of the current read that could be switched to (not gone, not
    /// checked out meanwhile).
    fn listed(&self, target: &Target) -> bool {
        self.rows
            .iter()
            .any(|r| &r.target == target && r.mark != Mark::Current)
    }

    /// `target`'s confirmation from the current read: at once with a clean tree, or with its
    /// count of this read; else (a dirty tree, its count not of this read) after counting it
    /// (`count_then_confirm`). A seeded switcher counts nothing: its seeded count, or unknown.
    fn confirm_fresh(&mut self, target: Target, typed: bool, cx: &mut Context<Self>) {
        if self.dirty.is_empty() {
            self.confirm(target, Some(0), typed);
        } else if self.fresh.contains(target.name()) || !self.live {
            let count = self.conflicts.get(target.name()).copied();
            self.apply_count(target, count, typed);
        } else {
            self.count_then_confirm(target, typed, cx);
        }
    }

    /// A known count for `target`: a row's conflict disables it and leaves the confirmation
    /// for the list; else (none, unknown, or a typed branch's) its confirmation.
    fn apply_count(&mut self, target: Target, count: Option<usize>, typed: bool) {
        match count {
            Some(n) if n > 0 && !typed => {
                self.step = Step::List;
                self.rebuild_rows();
                self.relist(true);
            }
            _ => self.confirm(target, count, typed),
        }
    }

    /// Show the confirmation of `target` (`confirmation`).
    fn confirm(&mut self, target: Target, conflict: Option<usize>, typed: bool) {
        let orphans = self.snapshot.as_ref().map_or(0, |s| s.orphans);
        let confirm = confirmation(
            target,
            conflict,
            &self.programs_here(),
            self.dirty.len(),
            orphans,
        );
        self.step = Step::Confirm(Confirm { typed, ..confirm });
    }

    /// Count `target`'s conflict against this read's tree (busy meanwhile), then
    /// `counted_chosen`.
    fn count_then_confirm(&mut self, target: Target, typed: bool, cx: &mut Context<Self>) {
        let Some(root) = self.snapshot.as_ref().map(|s| s.root.clone()) else {
            return;
        };
        self.busy = Some(COUNTING.to_string());
        let (git, dirty) = (self.repo.git.clone(), self.dirty.clone());
        let generation = self.generation;
        let task = cx.background_executor().spawn(async move {
            let count = branch_git::changed(&git, &root, &target)
                .map(|changed| branch_list::conflict_count(&dirty, &changed));
            (target, count)
        });
        cx.spawn(async move |this, cx| {
            let (target, count) = task.await;
            let _ = this.update(cx, |this, cx| {
                this.counted_chosen(generation, target, typed, count, cx)
            });
        })
        .detach();
    }

    /// A chosen branch's count landed. Of an older read (one landed meanwhile): dropped, and
    /// the branch is looked at again on the new read (gone: back to the list). Of this read:
    /// kept, then a row's conflict disables it (the list stays); none, or unknown, opens its
    /// confirmation (git decides).
    fn counted_chosen(
        &mut self,
        generation: u64,
        target: Target,
        typed: bool,
        count: Option<usize>,
        cx: &mut Context<Self>,
    ) {
        self.busy = None;
        if generation != self.generation {
            if self.listed(&target) {
                self.confirm_fresh(target, typed, cx);
            } else {
                self.step = Step::List;
            }
            cx.notify();
            return;
        }
        if let Some(n) = count {
            self.conflicts.insert(target.name().to_string(), n);
            self.fresh.insert(target.name().to_string());
            self.rebuild_rows();
            self.relist(true);
        }
        self.apply_count(target, count, typed);
        cx.notify();
    }

    /// The typed name: fetch it from origin (busy meanwhile; the user asked, so a credential
    /// helper may open its window). Found: the repo read again and the branch's conflict
    /// counted, then its confirmation. Not on origin: `Branch <name> not found`, nothing made.
    /// While a fetch runs it waits for it to land first (the two would contend for origin's
    /// refs), busy and looking. A seeded switcher fetches nothing (nothing happens, unless it
    /// waits on its never-ending fetch).
    fn fetch_typed(&mut self, name: String, cx: &mut Context<Self>) {
        if self.busy.is_some() {
            return;
        }
        self.busy = Some(looking_for(&name));
        if self.fetch == FetchState::Running {
            self.pending_typed = Some(name);
            return;
        }
        self.run_typed_fetch(name, cx);
    }

    /// The typed name's fetch itself (`fetch_typed`), then `fetched_typed`.
    fn run_typed_fetch(&mut self, name: String, cx: &mut Context<Self>) {
        let root = self.snapshot.as_ref().map(|s| s.root.clone());
        let Some(root) = root.filter(|_| self.live) else {
            self.busy = None;
            return;
        };
        self.busy = Some(looking_for(&name));
        let (git, dir) = (self.repo.git.clone(), self.repo.dir.clone());
        let task = cx.background_executor().spawn(async move {
            match branch_git::fetch_branch(&git, &root, &name, FetchMode::Button) {
                Ok(true) => {
                    let snapshot = branch_git::read(&git, &dir);
                    let target = Target::Origin(name.clone());
                    let count = snapshot.as_ref().and_then(|s| {
                        if s.dirty.is_empty() {
                            return Some(0);
                        }
                        branch_git::changed(&git, &s.root, &target)
                            .map(|changed| branch_list::conflict_count(&s.dirty, &changed))
                    });
                    Typed::Found {
                        name,
                        snapshot: snapshot.map(Box::new),
                        count,
                    }
                }
                Ok(false) => Typed::NotFound(name),
                Err(why) => Typed::Failed(why),
            }
        });
        cx.spawn(async move |this, cx| {
            let typed = task.await;
            let _ = this.update(cx, |this, cx| this.fetched_typed(typed, cx));
        })
        .detach();
    }

    fn fetched_typed(&mut self, typed: Typed, cx: &mut Context<Self>) {
        self.busy = None;
        match typed {
            Typed::Found {
                name,
                snapshot,
                count,
            } => {
                let read = snapshot.is_some();
                if let Some(snapshot) = snapshot {
                    self.set_snapshot(*snapshot);
                }
                if let Some(n) = count {
                    self.conflicts.insert(name.clone(), n);
                    // Counted on the read that came with it, which is the current one now.
                    if read {
                        self.fresh.insert(name.clone());
                    }
                    self.rebuild_rows();
                    self.relist(true);
                }
                self.confirm(Target::Origin(name), count, true);
            }
            Typed::NotFound(name) => self.note = Some(not_found(&name)),
            Typed::Failed(why) => self.note = Some(fetch_failed(&why)),
        }
        cx.notify();
    }

    /// The confirmation's Switch: `branch_git::switch` on the background executor (busy
    /// meanwhile); a seeded switcher's stub instead (`fail_switch`).
    fn start_switch(&mut self, cx: &mut Context<Self>) {
        if self.busy.is_some() {
            return;
        }
        let Step::Confirm(confirm) = &self.step else {
            return;
        };
        if !confirm.can_switch {
            return;
        }
        let Some(root) = self.snapshot.as_ref().map(|s| s.root.clone()) else {
            return;
        };
        let target = confirm.target.clone();
        let job: Box<dyn FnOnce() -> Result<(), String> + Send> = if self.live {
            let (git, root) = (self.repo.git.clone(), root.clone());
            Box::new(move || branch_git::switch(&git, &root, &target))
        } else {
            let fail = self.fail_switch.clone();
            Box::new(move || fail.map_or(Ok(()), Err))
        };
        self.busy = Some(SWITCHING.to_string());
        let task = cx.background_executor().spawn(async move { job() });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| this.switched(root, result, cx));
        })
        .detach();
        cx.notify();
    }

    /// git answered the switch: yes closes the panel (`Switched`); no goes back to the list
    /// with git's message under the box, and the list is read again (the tree may have changed
    /// since the counts, and a refusing git can have touched it: see the M11o Task 4 report on
    /// `switch --track` in a single-branch clone).
    fn switched(&mut self, root: PathBuf, result: Result<(), String>, cx: &mut Context<Self>) {
        self.busy = None;
        match result {
            Ok(()) => cx.emit(BranchSwitcherEvent::Switched { root }),
            Err(message) => {
                self.step = Step::List;
                self.note = Some(message);
                self.read_again(cx);
            }
        }
        cx.notify();
    }

    /// Read the repo again (a seeded switcher shows its snapshot again, as a new read).
    fn read_again(&mut self, cx: &mut Context<Self>) {
        if self.live {
            let (git, dir) = (self.repo.git.clone(), self.repo.dir.clone());
            self.read_with(move || branch_git::read(&git, &dir), false, cx);
        } else if let Some(snapshot) = self.snapshot.clone() {
            self.set_snapshot(snapshot);
        }
    }

    /// What a selectable line chooses.
    fn line_target(&self, at: usize) -> Option<Target> {
        match self.lines.get(at)? {
            Line::Row(i) => Some(self.rows[*i].target.clone()),
            // No row has its name, so it can't be taken for one.
            Line::Typed(name) => Some(Target::Origin(name.clone())),
            Line::OriginHeading => None,
        }
    }

    /// Rebuild the lines for the box's text: the matching rows, the ORIGIN heading before the
    /// first origin one, and (with an origin) the typed name when no branch has it. `keep`: the
    /// chosen branch stays chosen while it can be (else the place stays); not `keep`, the first
    /// row is chosen and the list goes back to its top.
    fn relist(&mut self, keep: bool) {
        let before = self
            .selectable
            .get(self.selected)
            .and_then(|&at| self.line_target(at))
            .filter(|_| keep);
        let query = self.query.trim();
        let mut lines = Vec::new();
        for i in branch_list::visible(&self.rows, query) {
            if self.rows[i].origin_section && !lines.contains(&Line::OriginHeading) {
                lines.push(Line::OriginHeading);
            }
            lines.push(Line::Row(i));
        }
        let has_origin = self.snapshot.as_ref().is_some_and(|s| s.has_origin);
        if has_origin && let Some(name) = branch_list::typed_name(&self.rows, query) {
            lines.push(Line::Typed(name));
        }
        self.selectable = lines
            .iter()
            .enumerate()
            .filter(|(_, line)| match line {
                Line::Row(i) => branch_list::selectable(&self.rows[*i]),
                Line::Typed(_) => true,
                Line::OriginHeading => false,
            })
            .map(|(at, _)| at)
            .collect();
        self.places = vec![None; lines.len()];
        for (place, &at) in self.selectable.iter().enumerate() {
            self.places[at] = Some(place);
        }
        self.matches = lines
            .iter()
            .map(|line| match line {
                Line::Row(i) => crate::palette_match::fuzzy(query, &self.rows[*i].label)
                    .map(|m| m.positions)
                    .unwrap_or_default(),
                Line::OriginHeading | Line::Typed(_) => Vec::new(),
            })
            .collect();
        self.lines = lines;
        let kept = before.and_then(|target| {
            self.selectable
                .iter()
                .position(|&at| self.line_target(at).as_ref() == Some(&target))
        });
        match kept {
            Some(at) => self.selected = at,
            None if keep => {
                self.selected = self.selected.min(self.selectable.len().saturating_sub(1))
            }
            None => {
                self.selected = 0;
                self.scroll.scroll_to_item(0);
            }
        }
    }

    /// ↑ / ↓: the previous or next row that can be chosen (round the ends); only on the list.
    fn select_by(&mut self, delta: i32, cx: &mut Context<Self>) {
        if self.step != Step::List || self.busy.is_some() {
            return;
        }
        self.selected = move_selection(self.selected, self.selectable.len(), delta);
        if let Some(&at) = self.selectable.get(self.selected) {
            self.scroll.scroll_to_item(at);
        }
        cx.notify();
    }

    fn render_line(
        &self,
        at: usize,
        line: &Line,
        ui: &UiColors,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let place = self.places.get(at).copied().flatten();
        let chosen = place == Some(self.selected);
        let row = match line {
            Line::OriginHeading => return render_group(ORIGIN_HEADING, at == 0, ui),
            Line::Typed(name) => {
                let text = typed_text(name);
                item_row(chosen, ui, None).child(name_cell(&text, &[], chosen))
            }
            Line::Row(i) => {
                let row = &self.rows[*i];
                let conflict = row.conflict.filter(|&n| n > 0);
                let positions = self.matches.get(at).map(Vec::as_slice).unwrap_or(&[]);
                let icon = (row.mark == Mark::Current)
                    .then(|| div().text_color(hsla(ui.text_faint)).child("\u{2713}"));
                // Not clickable (the current branch, a conflict): no hover fill.
                let frame = if place.is_some() {
                    item_row(chosen, ui, icon)
                } else {
                    item_frame(chosen, ui, icon)
                };
                frame
                    .when(conflict.is_some(), |el| el.text_color(hsla(ui.text_faint)))
                    .child(name_cell(&row.label, positions, chosen))
                    .when_some(right_text(row), |el, right| {
                        el.child(
                            div()
                                .flex_none()
                                .ml(px(8.0))
                                .text_size(px(text::META))
                                .font_features(tabular())
                                .text_color(hsla(ui.text_faint))
                                .child(right),
                        )
                    })
            }
        };
        let Some(place) = place else {
            return row.into_any_element();
        };
        // A click chooses it, as Enter does.
        row.id(("branch-row", at))
            .cursor_pointer()
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                if this.busy.is_some() || this.step != Step::List {
                    return;
                }
                this.selected = place;
                this.choose(cx);
            }))
            .into_any_element()
    }

    /// `Fetching…` while a fetch runs, `Fetch failed: …` after one failed.
    fn status(&self) -> Option<String> {
        match &self.fetch {
            FetchState::Idle => None,
            FetchState::Running => Some(FETCHING.to_string()),
            FetchState::Failed(why) => Some(fetch_failed(why)),
        }
    }

    /// The Fetch button: dimmed and inert while a fetch runs, on the confirmation, or while git
    /// works on a choice.
    fn fetch_button(&self, ui: &UiColors, cx: &mut Context<Self>) -> AnyElement {
        let running =
            self.fetch == FetchState::Running || self.busy.is_some() || self.step != Step::List;
        let hover = ui.panel_hover;
        let bounds = self.fetch_bounds.clone();
        div()
            .id("branch-fetch")
            .relative()
            .flex()
            .flex_none()
            .items_center()
            .h(px(BUTTON))
            .px(px(10.0))
            .rounded(px(5.0))
            .border_1()
            .border_color(hsla(ui.panel_border))
            .text_size(px(text::META))
            .when(running, |b| b.text_color(hsla(ui.text_faint)))
            .when(!running, |b| {
                b.text_color(hsla(ui.text))
                    .cursor_pointer()
                    .hover(move |s| s.bg(hsla(hover)))
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.start_fetch(FetchMode::Button, cx)
                    }))
            })
            .child(FETCH)
            .child(
                canvas(move |b, _, _| bounds.set(Some(b)), |_, _, _, _| {})
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full(),
            )
            .into_any_element()
    }

    /// The line under the box: what git is doing for a choice (faint), else its refusal or a
    /// typed name not found (in the failure colour).
    fn note_line(&self, ui: &UiColors) -> Option<AnyElement> {
        let (text, colour) = match (&self.busy, &self.note) {
            (Some(busy), _) => (busy.clone(), ui.text_faint),
            (None, Some(note)) => (note.clone(), ui.ring_waiting),
            (None, None) => return None,
        };
        let bounds = self.note_bounds.clone();
        Some(
            div()
                .relative()
                .px(px(8.0))
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .text_size(px(text::META))
                .text_color(hsla(colour))
                .child(text)
                .child(
                    canvas(move |b, _, _| bounds.set(Some(b)), |_, _, _, _| {})
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full(),
                )
                .into_any_element(),
        )
    }

    /// The confirmation step: its title, its warnings (a running program's with ⚠), then
    /// Cancel and Switch (the default, filled with the accent; dimmed and inert for a typed
    /// branch that would conflict, and both while the switch runs).
    fn render_confirm(
        &self,
        confirm: &Confirm,
        ui: &UiColors,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let running = branch_list::running_line(&self.programs_here());
        let idle = self.busy.is_none();
        let mut lines = confirm.lines.iter();
        let title = lines.next().cloned().unwrap_or_default();
        let warnings = lines.map(|line| {
            let warn = running.as_deref() == Some(line.as_str());
            div()
                .flex()
                .gap(px(6.0))
                .text_color(hsla(ui.text_dim))
                .when(warn, |l| {
                    l.child(div().text_color(hsla(ui.ring_waiting)).child("\u{26a0}"))
                })
                .child(line.clone())
        });
        let cancel = self
            .confirm_button(CANCEL, false, idle, self.cancel_bounds.clone(), ui)
            .when(idle, |b| {
                b.on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.escape(false, cx)))
            });
        let can_switch = idle && confirm.can_switch;
        let switch = self
            .confirm_button(SWITCH, true, can_switch, self.switch_bounds.clone(), ui)
            .when(can_switch, |b| {
                b.on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.start_switch(cx)))
            });
        div()
            .flex()
            .flex_col()
            .gap(px(4.0))
            .px(px(8.0))
            .py(px(4.0))
            .child(div().font_weight(FontWeight::MEDIUM).child(title))
            .children(warnings)
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.0))
                    .pt(px(6.0))
                    .child(cancel)
                    .child(switch),
            )
            .into_any_element()
    }

    /// A confirmation button: `primary` filled with the accent; inert ones dimmed.
    fn confirm_button(
        &self,
        label: &'static str,
        primary: bool,
        enabled: bool,
        bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
        ui: &UiColors,
    ) -> gpui::Stateful<gpui::Div> {
        let hover = ui.panel_hover;
        div()
            .id(label)
            .relative()
            .flex()
            .flex_none()
            .items_center()
            .h(px(BUTTON))
            .px(px(12.0))
            .rounded(px(5.0))
            .text_size(px(text::META))
            .map(|b| match (primary, enabled) {
                (true, true) => b.bg(hsla(ui.accent)).text_color(hsla(ui.on_accent)),
                (false, true) => b
                    .border_1()
                    .border_color(hsla(ui.panel_border))
                    .text_color(hsla(ui.text))
                    .hover(move |s| s.bg(hsla(hover))),
                (_, false) => b
                    .border_1()
                    .border_color(hsla(ui.panel_border))
                    .text_color(hsla(ui.text_faint)),
            })
            .when(enabled, |b| b.cursor_pointer())
            .child(label)
            .child(
                canvas(move |b, _, _| bounds.set(Some(b)), |_, _, _, _| {})
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full(),
            )
    }
}

/// The UI screenshots' look inside.
#[cfg(feature = "ui-shots")]
impl BranchSwitcher {
    /// Open on `snapshot` as read, `conflicts` counted, `programs` running; no git runs (no
    /// fetch, no counts, no typed name's fetch). Its Switch fails with `fail_switch`'s message,
    /// or (none) succeeds at once: `Switched`, nothing switched.
    pub fn seeded(
        repo: RepoAt,
        programs: Vec<String>,
        snapshot: Snapshot,
        conflicts: HashMap<String, usize>,
        fail_switch: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut this = Self::make(repo, programs, false, window, cx);
        this.fail_switch = fail_switch;
        this.conflicts = conflicts;
        this.set_snapshot(snapshot);
        this
    }

    /// `new`, with `read` in place of git's first read.
    pub fn with_read(
        repo: RepoAt,
        programs: Vec<String>,
        read: impl FnOnce() -> Option<Snapshot> + Send + 'static,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::reading(repo, programs, read, window, cx)
    }

    pub fn label(&self) -> &str {
        &self.repo.label
    }

    /// Each line as it reads: a row's name and its mark (` · ` between), the heading, the
    /// typed name's row.
    pub fn line_texts(&self) -> Vec<String> {
        self.lines
            .iter()
            .map(|line| match line {
                Line::Row(i) => {
                    let row = &self.rows[*i];
                    match right_text(row) {
                        Some(mark) => format!("{} \u{b7} {mark}", row.label),
                        None => row.label.clone(),
                    }
                }
                Line::OriginHeading => ORIGIN_HEADING.to_string(),
                Line::Typed(name) => typed_text(name),
            })
            .collect()
    }

    /// The chosen row's name (the typed name's row: its whole text).
    pub fn selected_label(&self) -> Option<String> {
        match self.lines.get(*self.selectable.get(self.selected)?)? {
            Line::Row(i) => Some(self.rows[*i].label.clone()),
            Line::Typed(name) => Some(typed_text(name)),
            Line::OriginHeading => None,
        }
    }

    /// Where line `at` was last drawn.
    pub fn line_bounds(&self, at: usize) -> Option<Bounds<Pixels>> {
        self.scroll.bounds_for_item(at)
    }

    pub fn panel_bounds(&self) -> Option<Bounds<Pixels>> {
        self.panel_bounds.get()
    }

    pub fn fetch_bounds(&self) -> Option<Bounds<Pixels>> {
        self.fetch_bounds.get()
    }

    /// The text beside the Fetch button, if any.
    pub fn fetch_status(&self) -> Option<String> {
        self.status()
    }

    /// The confirmation's lines while it shows; none on the list.
    pub fn confirm_texts(&self) -> Option<Vec<String>> {
        match &self.step {
            Step::Confirm(c) => Some(c.lines.clone()),
            Step::List => None,
        }
    }

    /// The confirmation's buttons and whether each can be pressed; none on the list.
    pub fn buttons(&self) -> Vec<(&'static str, bool)> {
        match &self.step {
            Step::Confirm(c) => {
                let idle = self.busy.is_none();
                vec![(CANCEL, idle), (SWITCH, idle && c.can_switch)]
            }
            Step::List => Vec::new(),
        }
    }

    /// git's refusal or a typed name not found, under the box.
    pub fn note(&self) -> Option<String> {
        self.note.clone()
    }

    /// Whether git is working on a choice (input ignored).
    pub fn busy(&self) -> bool {
        self.busy.is_some()
    }

    pub fn switch_bounds(&self) -> Option<Bounds<Pixels>> {
        self.switch_bounds.get()
    }

    pub fn cancel_bounds(&self) -> Option<Bounds<Pixels>> {
        self.cancel_bounds.get()
    }

    pub fn note_bounds(&self) -> Option<Bounds<Pixels>> {
        self.note_bounds.get()
    }

    /// Choose the row named `label` as Enter would, without drawing first (so a row out of
    /// view, not counted, is chosen before its count). False if no such row can be chosen.
    pub fn choose_label(&mut self, label: &str, cx: &mut Context<Self>) -> bool {
        let Some(place) = self.selectable.iter().position(
            |&at| matches!(self.lines.get(at), Some(Line::Row(i)) if self.rows[*i].label == label),
        ) else {
            return false;
        };
        self.selected = place;
        self.choose(cx);
        true
    }

    /// What git is doing for a choice, as the line under the box says it.
    pub fn busy_text(&self) -> Option<String> {
        self.busy.clone()
    }

    /// Land `snapshot` as a later read does (one from the background, e.g. after a fetch).
    pub fn land_read(&mut self, snapshot: Snapshot, cx: &mut Context<Self>) {
        self.got_snapshot(Some(snapshot), false, cx);
    }

    /// The box's text.
    pub fn query_text(&self, cx: &App) -> String {
        self.input.read(cx).text().to_string()
    }
}

impl Render for BranchSwitcher {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ui = cx.global::<Appearance>().theme.ui();
        let confirm = match &self.step {
            Step::Confirm(c) => Some(self.render_confirm(c, &ui, cx)),
            Step::List => None,
        };
        let rows: Vec<AnyElement> = match confirm {
            Some(_) => Vec::new(),
            None => self
                .lines
                .iter()
                .enumerate()
                .map(|(at, line)| self.render_line(at, line, &ui, cx))
                .collect(),
        };
        // After this frame is laid out (deferred past the draw), count the rows it shows.
        if confirm.is_none() && self.may_count() {
            cx.defer_in(window, |this, _, cx| this.count_conflicts(cx));
        }
        let has_origin = self.snapshot.as_ref().is_some_and(|s| s.has_origin);
        let status = self.status();
        let title = div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .truncate()
                    .font_weight(FontWeight::MEDIUM)
                    .child(self.title()),
            )
            .when(has_origin, |t| t.child(self.fetch_button(&ui, cx)))
            .when_some(status.filter(|_| has_origin), |t, status| {
                t.child(
                    div()
                        .flex_initial()
                        .min_w(px(0.0))
                        .max_w(px(PANEL_WIDTH / 2.0))
                        .truncate()
                        .text_size(px(text::META))
                        .text_color(hsla(ui.text_faint))
                        .child(status),
                )
            });
        let header = div()
            .flex()
            .flex_col()
            .gap(px(2.0))
            .px(px(8.0))
            .pt(px(2.0))
            .child(title)
            .child(
                div()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis_start()
                    .text_size(px(text::META))
                    .text_color(hsla(ui.text_faint))
                    .child(self.repo.label.clone()),
            );
        let empty = self.input.read(cx).text().is_empty();
        let field = overlay_panel::filter_box(&ui, &self.input, empty, PLACEHOLDER);
        let max_h = list_max_height(f32::from(window.viewport_size().height));
        let bounds = self.panel_bounds.clone();

        overlay_panel::shell(&ui, PANEL_WIDTH)
            .relative()
            .key_context("BranchSwitcher")
            .on_action(cx.listener(|this, _: &SelectPrev, _, cx| this.select_by(-1, cx)))
            .on_action(cx.listener(|this, _: &SelectNext, _, cx| this.select_by(1, cx)))
            // The terminal under the panel gets neither its clicks nor its hover.
            .occlude()
            // A click outside closes it, except while git works on a choice.
            .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                if this.busy.is_none() {
                    cx.emit(BranchSwitcherEvent::Cancel)
                }
            }))
            .child(header)
            .child(field)
            .when_some(self.note_line(&ui), |panel, note| panel.child(note))
            .when_some(confirm, |panel, confirm| panel.child(confirm))
            .when(!rows.is_empty(), |panel| {
                panel.child(
                    div()
                        .id("branch-rows")
                        .flex()
                        .flex_col()
                        .gap(px(ROW_GAP))
                        .max_h(px(max_h))
                        .overflow_y_scroll()
                        .track_scroll(&self.scroll)
                        .children(rows),
                )
            })
            .child(
                canvas(move |b, _, _| bounds.set(Some(b)), |_, _, _, _| {})
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full(),
            )
    }
}

impl Workspace {
    /// Open the branch switcher on terminal `terminal`'s repo: its folder as the sidebar finds
    /// it (as last looked up, else the one it started in). Nothing opens for an SSH terminal,
    /// one with no folder, no git on this Mac, or a folder with no branch (not a repo).
    pub(crate) fn open_branch_switcher(
        &mut self,
        terminal: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = self.sessions.items().iter().find(|s| s.id == terminal) else {
            return;
        };
        if session.ssh.is_some() {
            return;
        }
        let Some(folder) = self
            .places
            .folder(terminal)
            .map(Path::to_path_buf)
            .or_else(|| session.project.clone())
        else {
            return;
        };
        if self.places.branch(&folder).is_none() {
            return;
        }
        let Some(git) = self.places.git() else {
            return;
        };
        let label = home_relative(&folder, self.places.home.as_deref());
        let repo = RepoAt {
            git,
            dir: folder,
            label,
        };
        let running = self.running_programs();
        let switcher =
            cx.new(|cx| BranchSwitcher::new(repo, Vec::new(), window, cx).with_running(running));
        self.show_branch_switcher(switcher, window, cx);
    }

    /// Each local terminal that runs a program: its folder (as last looked up, else the one it
    /// started in) and what to call the program: the terminal's title, else the program's name,
    /// else `a program`. Which are inside the repo is told once its top folder is read (the
    /// switcher's `programs_here`): no git, and no file system, on this thread.
    fn running_programs(&self) -> Vec<(PathBuf, String)> {
        self.sessions
            .items()
            .iter()
            .filter(|s| s.ssh.is_none())
            .filter_map(|s| {
                let ProgramState::Running(name) = self.programs.state(s.id) else {
                    return None;
                };
                let folder = self
                    .places
                    .folder(s.id)
                    .or(s.project.as_deref())?
                    .to_path_buf();
                let label = match &s.title {
                    Some(title) if !title.is_empty() => title.to_string(),
                    _ if !name.is_empty() => name,
                    _ => "a program".to_string(),
                };
                Some((folder, label))
            })
            .collect()
    }

    /// Show `switcher` over the window, any other box closed first, with the keyboard.
    pub(crate) fn show_branch_switcher(
        &mut self,
        switcher: Entity<BranchSwitcher>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_overlays(Overlay::BranchSwitcher);
        cx.subscribe_in(&switcher, window, Self::on_branch_switcher_event)
            .detach();
        switcher.focus_handle(cx).focus(window, cx);
        self.branch_switcher = Some(switcher);
        cx.notify();
    }

    /// The switcher switched or was closed.
    fn on_branch_switcher_event(
        &mut self,
        switcher: &Entity<BranchSwitcher>,
        event: &BranchSwitcherEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // A switcher already closed or replaced can still report losing focus: ignore it.
        if self.branch_switcher.as_ref() != Some(switcher) {
            return;
        }
        self.branch_switcher = None;
        self.focus_active(window, cx);
        if let BranchSwitcherEvent::Switched { root } = event {
            self.refresh_branches_in(root.clone(), cx);
        }
        cx.notify();
    }

    /// After a switch in the repo at `root`: look the branch up again now in every terminal's
    /// folder inside it (as the 5 s refresh does: `branch_at` on the background executor, then
    /// `set_branch`), so the sidebar rows and the status bar show the new branch without
    /// waiting. Not over SSH (the folder is on another machine), not while the places are
    /// frozen (the screenshots).
    fn refresh_branches_in(&mut self, root: PathBuf, cx: &mut Context<Self>) {
        if self.places.frozen {
            return;
        }
        let Some(git) = self.places.git() else {
            return;
        };
        let mut folders: Vec<PathBuf> = Vec::new();
        for session in self.sessions.items().iter().filter(|s| s.ssh.is_none()) {
            if let Some(folder) = self.places.folder(session.id)
                && !folders.iter().any(|f| f == folder)
            {
                folders.push(folder.to_path_buf());
            }
        }
        if folders.is_empty() {
            return;
        }
        cx.spawn(async move |this, cx| {
            let found: Vec<(PathBuf, Option<String>)> = cx
                .background_executor()
                .spawn(async move {
                    folders
                        .into_iter()
                        .filter(|folder| in_repo(folder, &root))
                        .map(|folder| {
                            let branch = branch_at(&git, &folder);
                            (folder, branch)
                        })
                        .collect()
                })
                .await;
            this.update(cx, |this, cx| {
                let mut changed = false;
                for (folder, branch) in found {
                    changed |= this.places.set_branch(&folder, branch);
                }
                if changed {
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::branch_list::Mark;

    fn row(name: &str, mark: Mark, origin: bool) -> Row {
        Row {
            target: if origin {
                Target::Origin(name.into())
            } else {
                Target::Local(name.into())
            },
            label: name.into(),
            mark,
            conflict: None,
            origin_section: origin,
        }
    }

    fn names(targets: &[Target]) -> Vec<&str> {
        targets.iter().map(Target::name).collect()
    }

    #[test]
    fn only_rows_in_view_are_counted_and_each_once() {
        let rows = vec![
            row("main", Mark::Current, false),
            row("a", Mark::LocalOnly, false),
            row("b", Mark::Even, false),
            row("c", Mark::Even, false),
            row("x", Mark::None, true),
        ];
        let lines = vec![
            Line::Row(0),
            Line::Row(1),
            Line::Row(2),
            Line::Row(3),
            Line::OriginHeading,
            Line::Row(4),
            Line::Typed("zzz".into()),
        ];
        let none = HashSet::new();
        // Lines 0-2 in view: the current branch is never counted.
        assert_eq!(names(&to_count(&lines, &rows, 0..=2, &none, 4)), ["a", "b"]);
        // Scrolled down: the heading and the typed name aren't branches to count.
        assert_eq!(names(&to_count(&lines, &rows, 3..=6, &none, 4)), ["c", "x"]);
        // Asked already (running or answered): not again.
        let asked: HashSet<String> = ["a".to_string()].into_iter().collect();
        assert_eq!(
            names(&to_count(&lines, &rows, 0..=6, &asked, 4)),
            ["b", "c", "x"]
        );
        // Room for one more count: one.
        assert_eq!(names(&to_count(&lines, &rows, 0..=6, &none, 1)), ["a"]);
        assert!(to_count(&lines, &rows, 0..=6, &none, 0).is_empty());
    }

    #[test]
    fn before_the_list_is_drawn_the_rows_that_fit_are_in_view() {
        // 420 pt at most, 30 pt a row (28 + 2): 14 lines.
        assert_eq!(unlaid_view(), 0..=13);
    }

    #[test]
    fn the_confirmation_warns_or_shows_a_typed_branchs_conflict() {
        let claude = vec!["Claude Code".to_string()];
        let local = || Target::Local("feature/rows".into());
        // Counted clean, 2 dirty files: both come along; Switch can be pressed.
        let c = confirmation(local(), Some(0), &claude, 2, 0);
        assert_eq!(
            c.lines,
            [
                "Switch to feature/rows?",
                "Claude Code is running in this folder",
                "2 uncommitted file(s) come along"
            ]
        );
        assert!(c.can_switch);
        // Unknown count: every dirty file is said to come along (git decides).
        assert_eq!(
            confirmation(local(), None, &[], 3, 0).lines,
            [
                "Switch to feature/rows?",
                "3 uncommitted file(s) come along"
            ]
        );
        // A clean tree and nothing running: the title alone, still asked.
        let c = confirmation(local(), Some(0), &[], 0, 0);
        assert_eq!(c.lines, ["Switch to feature/rows?"]);
        assert!(c.can_switch);
        // A typed branch that would conflict: its count in place of the warnings, no Switch.
        let c = confirmation(Target::Origin("fix".into()), Some(2), &claude, 5, 1);
        assert_eq!(c.lines, ["Switch to fix?", "conflict 2 file(s)"]);
        assert!(!c.can_switch);
        assert_eq!(c.target, Target::Origin("fix".into()));
    }

    #[test]
    fn the_confirmation_warns_of_commits_on_no_branch() {
        let c = confirmation(Target::Local("main".into()), Some(0), &[], 1, 2);
        assert_eq!(
            c.lines,
            [
                "Switch to main?",
                "1 uncommitted file(s) come along",
                "2 commit(s) on no branch: after switching they are only in the reflog"
            ]
        );
        assert!(c.can_switch);
    }

    #[test]
    fn a_folder_is_in_the_repo_by_its_path_or_its_real_path() {
        let base = std::env::temp_dir().join(format!("fuxx-in-repo-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let root = base.join("repo");
        std::fs::create_dir_all(root.join("src")).unwrap();
        let link = base.join("link");
        std::os::unix::fs::symlink(&root, &link).unwrap();
        let real = std::fs::canonicalize(&root).unwrap();
        // git gives the real path; the folder may be the same one by another way in.
        assert!(in_repo(&real.join("src"), &real));
        assert!(in_repo(&real, &real));
        assert!(in_repo(&link.join("src"), &real), "through a symlink");
        assert!(!in_repo(&base, &real), "above it");
        assert!(
            !in_repo(&base.join("repo-2"), &real),
            "a sibling with a longer name"
        );
        assert!(!in_repo(Path::new("/nonexistent/x"), &real));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn only_programs_in_folders_of_the_repo_are_named() {
        let running = vec![
            (PathBuf::from("/nonexistent/repo/src"), "claude".to_string()),
            (PathBuf::from("/nonexistent/repo-2"), "vim".to_string()),
            (PathBuf::from("/nonexistent/repo"), "a program".to_string()),
        ];
        assert_eq!(
            programs_under(&running, Path::new("/nonexistent/repo")),
            ["claude", "a program"]
        );
        assert!(programs_under(&[], Path::new("/nonexistent/repo")).is_empty());
    }

    #[test]
    fn a_rows_right_text_is_its_conflict_else_its_mark() {
        let mut r = row(
            "a",
            Mark::Diverged {
                ahead: 2,
                behind: 0,
            },
            false,
        );
        assert_eq!(right_text(&r).as_deref(), Some("\u{2191}2"));
        r.conflict = Some(0);
        assert_eq!(right_text(&r).as_deref(), Some("\u{2191}2"));
        r.conflict = Some(3);
        assert_eq!(right_text(&r).as_deref(), Some("conflict 3 file(s)"));
        assert_eq!(right_text(&row("x", Mark::None, true)), None);
    }
}
