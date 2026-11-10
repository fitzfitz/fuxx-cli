//! M11o: the branch switcher (note 66 §1-§3, §5), a panel where the ⌘K palette opens: the
//! repo's local branches (marked current, ahead/behind, local only, upstream gone), then
//! origin's that aren't local, under a box that filters them and offers a typed name to check
//! out from origin. A branch the switch would collide with (uncommitted files it also changes)
//! is disabled with "conflict x file(s)".
//!
//! Everything git does runs on the background executor: one read as it opens, the automatic
//! fetch (and the Fetch button's) with a read after it, and the conflict counts (only with a
//! dirty work tree, at most `COUNTS_AT_ONCE` at a time, rows in view first). Each task holds
//! only a weak handle: once the switcher is closed or replaced, its answers are dropped and
//! nothing new starts. The box is the palette's (`NameInput`): its Escape, losing focus or a
//! click outside closes the switcher.

use std::cell::Cell;
use std::collections::{HashMap, HashSet};
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
use crate::help_panel::render_group;
use crate::host_picker::move_selection;
use crate::name_input::{NameInput, NameInputEvent};
use crate::overlay_panel;
use crate::palette::{PANEL_WIDTH, item_frame, item_row, list_max_height, name_cell};
use crate::sidebar::{home_relative, tabular};
use crate::switcher::ROW_GAP;
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
/// The Fetch button's height (the palette's chips').
const BUTTON: f32 = 22.0;

/// The repo a switcher opens on: git, the terminal's folder in it, and that folder as the
/// sidebar shows it (home-relative).
pub struct RepoAt {
    pub git: PathBuf,
    pub dir: PathBuf,
    pub label: String,
}

pub enum BranchSwitcherEvent {
    /// The repo at `root` is on another branch now.
    #[allow(dead_code)] // emitted by the switch (M11o Task 4)
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

/// The typed name's row.
fn typed_text(name: &str) -> String {
    format!("Check out {name} from origin")
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

/// The ⌘K-like panel that switches a repo's branch.
pub struct BranchSwitcher {
    repo: RepoAt,
    /// The programs running in this repo's terminals (the confirmation's warning).
    #[allow(dead_code)] // read by the confirmation (M11o Task 4)
    programs: Vec<String>,
    /// git may run (a seeded switcher, for the screenshots, runs none).
    live: bool,
    input: Entity<NameInput>,
    /// The box's text as last listed.
    query: String,
    /// The last read; none until the first lands.
    snapshot: Option<Snapshot>,
    /// The snapshot's dirty files, shared with the counts.
    dirty: Arc<HashSet<PathBuf>>,
    /// Counted conflicts by branch name (a name missing: not counted, or unknown).
    conflicts: HashMap<String, usize>,
    rows: Vec<Row>,
    lines: Vec<Line>,
    /// The places in `lines` of the rows that can be chosen.
    selectable: Vec<usize>,
    /// The chosen one, a place in `selectable`.
    selected: usize,
    fetch: FetchState,
    /// Bumped with each read: a count asked of an older one is dropped.
    generation: u64,
    /// This read's branches still to count.
    to_count: Vec<Target>,
    /// Counts running (of any read).
    counting: usize,
    scroll: ScrollHandle,
    /// Where the panel was last drawn (the UI screenshots read it).
    panel_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    _input_events: Subscription,
    _input_changes: Subscription,
}

impl EventEmitter<BranchSwitcherEvent> for BranchSwitcher {}

impl Focusable for BranchSwitcher {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.input.focus_handle(cx)
    }
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
        let input_events = cx.subscribe_in(&input, window, |_, _, event, _, cx| match event {
            // Choosing a row is the switch's (M11o Task 4).
            NameInputEvent::Confirm(_) => {}
            NameInputEvent::Cancel => cx.emit(BranchSwitcherEvent::Cancel),
        });
        let input_changes = cx.observe(&input, |this, _, cx| this.query_changed(cx));
        Self {
            repo,
            programs,
            live,
            input,
            query: String::new(),
            snapshot: None,
            dirty: Arc::default(),
            conflicts: HashMap::new(),
            rows: Vec::new(),
            lines: Vec::new(),
            selectable: Vec::new(),
            selected: 0,
            fetch: FetchState::Idle,
            generation: 0,
            to_count: Vec::new(),
            counting: 0,
            scroll: ScrollHandle::new(),
            panel_bounds: Rc::default(),
            _input_events: input_events,
            _input_changes: input_changes,
        }
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
        if fetch {
            self.start_fetch(FetchMode::Auto, cx);
        }
        self.count_conflicts(cx);
        cx.notify();
    }

    /// Show `snapshot`. Its branches are counted again (with a dirty tree): the counts of the
    /// branches still there stay shown until theirs land; a clean tree has none.
    fn set_snapshot(&mut self, snapshot: Snapshot) {
        self.generation += 1;
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
        self.rebuild_rows();
        self.to_count = if self.live && !self.dirty.is_empty() {
            self.rows
                .iter()
                .filter(|r| r.mark != Mark::Current)
                .map(|r| r.target.clone())
                .collect()
        } else {
            Vec::new()
        };
        self.relist(true);
    }

    fn rebuild_rows(&mut self) {
        let Some(s) = &self.snapshot else {
            return;
        };
        self.rows = branch_list::rows(&s.head, &s.local, &s.origin, &s.pushed, &self.conflicts);
    }

    /// Start counting branches' conflicts until `COUNTS_AT_ONCE` run; each answer starts the
    /// next.
    fn count_conflicts(&mut self, cx: &mut Context<Self>) {
        let Some(root) = self.snapshot.as_ref().map(|s| s.root.clone()) else {
            return;
        };
        while self.counting < COUNTS_AT_ONCE {
            let Some(target) = self.next_to_count() else {
                break;
            };
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

    /// The next branch to count: the first one whose line was in view at the last draw, else
    /// the first in list order.
    fn next_to_count(&mut self) -> Option<Target> {
        if self.to_count.is_empty() {
            return None;
        }
        let in_view = self.scroll.bounds_for_item(0).map(|_| {
            let (top, bottom) = (self.scroll.top_item(), self.scroll.bottom_item());
            top..=bottom
        });
        let at = in_view
            .and_then(|view| {
                self.to_count.iter().position(|t| {
                    self.lines.iter().enumerate().any(|(at, line)| {
                        view.contains(&at)
                            && matches!(line, Line::Row(i) if self.rows[*i].target == *t)
                    })
                })
            })
            .unwrap_or(0);
        Some(self.to_count.remove(at))
    }

    /// A count landed: shown if it is of the current read (none: unknown, left unmarked).
    fn counted(
        &mut self,
        generation: u64,
        target: Target,
        count: Option<usize>,
        cx: &mut Context<Self>,
    ) {
        self.counting -= 1;
        if generation == self.generation
            && let Some(count) = count
        {
            self.conflicts.insert(target.name().to_string(), count);
            self.rebuild_rows();
            self.relist(true);
            cx.notify();
        }
        self.count_conflicts(cx);
    }

    /// Fetch origin in the background (one at a time), then read again.
    fn start_fetch(&mut self, mode: FetchMode, cx: &mut Context<Self>) {
        if !self.live || self.fetch == FetchState::Running {
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
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    /// The box changed: list again if its text did, from the first row.
    fn query_changed(&mut self, cx: &mut Context<Self>) {
        let query = self.input.read(cx).text().to_string();
        if query != self.query {
            self.query = query;
            self.relist(false);
            cx.notify();
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

    /// ↑ / ↓: the previous or next row that can be chosen (round the ends).
    fn select_by(&mut self, delta: i32, cx: &mut Context<Self>) {
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
        let place = self.selectable.iter().position(|&l| l == at);
        let chosen = place.is_some() && place == Some(self.selected);
        let query = self.query.trim();
        let row = match line {
            Line::OriginHeading => return render_group(ORIGIN_HEADING, at == 0, ui),
            Line::Typed(name) => {
                let text = typed_text(name);
                item_row(chosen, ui, None).child(name_cell(&text, &[], chosen))
            }
            Line::Row(i) => {
                let row = &self.rows[*i];
                let conflict = row.conflict.filter(|&n| n > 0);
                let positions = crate::palette_match::fuzzy(query, &row.label)
                    .map(|m| m.positions)
                    .unwrap_or_default();
                let icon = (row.mark == Mark::Current)
                    .then(|| div().text_color(hsla(ui.text_dim)).child("\u{2713}"));
                let right = match conflict {
                    Some(n) => Some(branch_list::conflict_text(n)),
                    None => branch_list::mark_text(&row.mark),
                };
                // Not clickable (the current branch, a conflict): no hover fill.
                let frame = if place.is_some() {
                    item_row(chosen, ui, icon)
                } else {
                    item_frame(chosen, ui, icon)
                };
                frame
                    .when(conflict.is_some(), |el| el.text_color(hsla(ui.text_faint)))
                    .child(name_cell(&row.label, &positions, chosen))
                    .when_some(right, |el, right| {
                        el.child(
                            div()
                                .flex_none()
                                .ml(px(8.0))
                                .text_size(px(text::META))
                                .font_features(tabular())
                                .text_color(hsla(if conflict.is_some() {
                                    ui.text_faint
                                } else {
                                    ui.text_dim
                                }))
                                .child(right),
                        )
                    })
            }
        };
        let Some(place) = place else {
            return row.into_any_element();
        };
        // Choosing (Enter, a click) is the switch's (M11o Task 4): a click only selects.
        row.id(("branch-row", at))
            .cursor_pointer()
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.selected = place;
                cx.notify();
            }))
            .into_any_element()
    }

    /// The Fetch button: dimmed and inert while a fetch runs.
    fn fetch_button(&self, ui: &UiColors, cx: &mut Context<Self>) -> AnyElement {
        let running = self.fetch == FetchState::Running;
        let hover = ui.panel_hover;
        div()
            .id("branch-fetch")
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
            .into_any_element()
    }
}

/// The UI screenshots' look inside.
#[cfg(feature = "ui-shots")]
impl BranchSwitcher {
    /// Open on `snapshot` as read, `conflicts` counted; no git runs (no fetch, no counts).
    pub fn seeded(
        repo: RepoAt,
        snapshot: Snapshot,
        conflicts: HashMap<String, usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut this = Self::make(repo, Vec::new(), false, window, cx);
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
                    let mark = match row.conflict.filter(|&n| n > 0) {
                        Some(n) => Some(branch_list::conflict_text(n)),
                        None => branch_list::mark_text(&row.mark),
                    };
                    match mark {
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
}

impl Render for BranchSwitcher {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ui = cx.global::<Appearance>().theme.ui();
        let rows: Vec<AnyElement> = self
            .lines
            .iter()
            .enumerate()
            .map(|(at, line)| self.render_line(at, line, &ui, cx))
            .collect();
        let has_origin = self.snapshot.as_ref().is_some_and(|s| s.has_origin);
        let status = match &self.fetch {
            FetchState::Idle => None,
            FetchState::Running => Some(FETCHING.to_string()),
            FetchState::Failed(why) => Some(fetch_failed(why)),
        };
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
            .on_mouse_down_out(
                cx.listener(|_, _: &MouseDownEvent, _, cx| cx.emit(BranchSwitcherEvent::Cancel)),
            )
            .child(header)
            .child(field)
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
    #[allow(dead_code)] // called from the row, the status bar and ⌘K (M11o Task 5)
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
        let switcher = cx.new(|cx| BranchSwitcher::new(repo, Vec::new(), window, cx));
        self.show_branch_switcher(switcher, window, cx);
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
        match event {
            BranchSwitcherEvent::Switched { .. } | BranchSwitcherEvent::Cancel => {
                self.branch_switcher = None;
                self.focus_active(window, cx);
                cx.notify();
            }
        }
    }
}
