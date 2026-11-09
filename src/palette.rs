//! M11j: the ⌘K command palette, a panel near the top of the window: one box over every action,
//! workspace, file and setting. Chips under the box narrow it to one kind (Tab and ⇧Tab move
//! between them; on All, a query starting with `>` searches Actions); the list under them is
//! `palette_items::compose`'s, rebuilt on each keystroke and each time the Workspace draws (so
//! the workspaces' marks move and the order follows their statuses while it's open), the
//! selection kept on its item. ⌘P opens it on Workspaces: the old ⌘P list.
//!
//! The box is the sidebar's rename box (`NameInput`), as in the Help panel: its Enter does the
//! chosen row, its Escape (or losing focus, or a click outside) closes the palette. A pick
//! closes the palette, gives the keyboard back to the focused tab, and only then runs (an
//! action is dispatched from there, so it lands where its key would).

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use alacritty_terminal::vte::ansi::Rgb;
use gpui::{
    AnyElement, App, Bounds, ClickEvent, Context, Div, Entity, EventEmitter, FocusHandle,
    Focusable, FontWeight, HighlightStyle, MouseDownEvent, Pixels, ScrollHandle, StyledText,
    Subscription, Task, Window, actions, canvas, div, prelude::*, px,
};

use crate::appearance::Appearance;
use crate::help_panel::render_group;
use crate::help_shortcuts::binding_pairs;
use crate::host_picker::move_selection;
use crate::layout_model::{FileMode, TabKind, WorkspaceId};
use crate::name_input::{NameInput, NameInputEvent};
use crate::overlay_panel;
use crate::palette_files::{CachedFiles, rank_files};
use crate::palette_items::{
    ActionRow, COMMANDS, FILE_RESULTS, FileHit, Files, Focused, HINT, Key, Line, NO_FOLDER, Recent,
    Run, SSH_NOTE, Scope, Sources, Step, action_rows, compose, more_label, palette_step,
    parse_query, selectable,
};
use crate::palette_match::byte_ranges;
use crate::preferences::PaletteSetting;
use crate::right_sidebar::{FileKind, file_kind, kind_ink};
use crate::sidebar::{MARK_SLOT, tabular};
use crate::status::Status;
use crate::switcher::{Entry, ROW, ROW_GAP, workspace_row};
use crate::tab_bar::{Glyph, glyph};
use crate::terminal_view::hsla;
use crate::theme::{Theme, UiColors, readable};
use crate::ui_tokens::{MARK_MIN, text};
use crate::workspace::{Overlay, Workspace};

// Bound in main.rs with the "Palette" key context; Enter and Escape are the box's own.
actions!(palette, [SelectPrev, SelectNext, NextScope, PrevScope]);

/// Wider than the Help panel: a row holds a name, a folder or keys.
const PANEL_WIDTH: f32 = 600.0;
/// A scope chip's height.
const CHIP: f32 = 22.0;
/// The list's tallest; it also keeps `LIST_BOTTOM` from the window's bottom, but is never
/// shorter than `LIST_MIN`.
const LIST_MAX: f32 = 420.0;
const LIST_MIN: f32 = 120.0;
const LIST_BOTTOM: f32 = 200.0;
/// The room between a row's icon slot and its name.
const SLOT_GAP: f32 = 8.0;

/// How tall the list may grow in a window `window_h` tall.
fn list_max_height(window_h: f32) -> f32 {
    (window_h - LIST_BOTTOM).clamp(LIST_MIN, LIST_MAX)
}

/// A file row's icon tint: the Files panel's (`right_sidebar::kind_ink`), pushed (hue kept) to
/// `MARK_MIN` on the palette's surfaces too.
pub(crate) fn palette_file_ink(kind: FileKind, theme: &Theme) -> Rgb {
    let ui = theme.ui();
    readable(
        kind_ink(kind, theme, &ui),
        &[ui.panel, ui.panel_hover, ui.row_active],
        MARK_MIN,
    )
}

/// What the palette knows of the files (`palette_files`): a note in their place, a first
/// listing running, a failed one, or the listed paths (from `root`; `more`: how many past the
/// cap).
#[derive(Clone, Debug, PartialEq)]
pub enum FilesData {
    Note(&'static str),
    Listing,
    Failed,
    Ready {
        root: PathBuf,
        paths: Arc<Vec<String>>,
        more: usize,
        /// The listing came from the walk (not git).
        walked: bool,
    },
}

/// What the palette has of the followed terminal's files, from its folder (and whether it is an
/// SSH terminal's) and that folder's last listing: no folder, a note; an SSH terminal, its note;
/// nothing listed yet (the first listing running or about to), `Listing`; the last listing
/// failed, `Failed`; else its paths.
pub(crate) fn files_data(
    folder: Option<(PathBuf, bool)>,
    cached: Option<&CachedFiles>,
) -> FilesData {
    match folder {
        None => FilesData::Note(NO_FOLDER),
        Some((_, true)) => FilesData::Note(SSH_NOTE),
        Some((_, false)) => match cached {
            None => FilesData::Listing,
            Some(c) if c.failed => FilesData::Failed,
            Some(c) => FilesData::Ready {
                root: c.root.clone(),
                paths: c.paths.clone(),
                more: c.more,
                walked: c.walked,
            },
        },
    }
}

/// A ranking's answer, kept only while it is for the newest query (`current`).
fn fresh_answer(
    current: &AtomicU64,
    asked: u64,
    answer: Option<(Vec<FileHit>, usize)>,
) -> Option<(u64, Vec<FileHit>, usize)> {
    let (hits, total) = answer?;
    (current.load(Ordering::Relaxed) == asked).then_some((asked, hits, total))
}

/// A file row's two parts from its path from the root: the name and the folder it is in, each
/// with its own matched chars.
fn file_parts(rel: &str, positions: &[usize]) -> (String, Vec<usize>, String, Vec<usize>) {
    let (folder, name) = rel.rsplit_once('/').unwrap_or(("", rel));
    // The name starts this many chars in (past the folder and its `/`).
    let start = rel.chars().count() - name.chars().count();
    let in_name = positions
        .iter()
        .filter(|&&p| p >= start)
        .map(|&p| p - start)
        .collect();
    let folder_len = folder.chars().count();
    let in_folder = positions
        .iter()
        .copied()
        .filter(|&p| p < folder_len)
        .collect();
    (name.to_string(), in_name, folder.to_string(), in_folder)
}

impl FilesData {
    /// Whether `other` is the same, the listed paths compared by pointer (each listing is a new
    /// `Arc`): the Workspace compares on every draw.
    fn same(&self, other: &FilesData) -> bool {
        match (self, other) {
            (
                FilesData::Ready {
                    root,
                    paths,
                    more,
                    walked,
                },
                FilesData::Ready {
                    root: root_b,
                    paths: paths_b,
                    more: more_b,
                    walked: walked_b,
                },
            ) => {
                Arc::ptr_eq(paths, paths_b)
                    && root == root_b
                    && more == more_b
                    && walked == walked_b
            }
            (FilesData::Note(a), FilesData::Note(b)) => a == b,
            (FilesData::Listing, FilesData::Listing) | (FilesData::Failed, FilesData::Failed) => {
                true
            }
            _ => false,
        }
    }

    /// The root of a listing; none for the rest.
    fn root(&self) -> Option<&Path> {
        match self {
            FilesData::Ready { root, .. } => Some(root),
            _ => None,
        }
    }

    /// Whether `path` is in the listing (a Recent file still there). The listings are sorted.
    fn lists(&self, path: &Path) -> bool {
        let FilesData::Ready { root, paths, .. } = self else {
            return false;
        };
        let Some(rel) = path.strip_prefix(root).ok().and_then(|r| r.to_str()) else {
            return false;
        };
        paths.binary_search_by(|p| p.as_str().cmp(rel)).is_ok()
    }

    /// As `compose` takes it, with the ranking's last `hits` (its generation, the best ones and
    /// how many matched).
    fn as_files<'a>(&'a self, hits: Option<&'a (u64, Vec<FileHit>, usize)>) -> Files<'a> {
        match self {
            FilesData::Note(note) => Files::Note(note),
            FilesData::Listing => Files::Listing,
            FilesData::Failed => Files::Failed,
            FilesData::Ready {
                root, more, walked, ..
            } => Files::Ready {
                root,
                root_name: root.file_name().and_then(|n| n.to_str()).unwrap_or(""),
                hits: hits.map_or(&[], |(_, hits, _)| hits.as_slice()),
                total: hits.map_or(0, |(_, _, total)| *total),
                capped: *more > 0,
                walked: *walked,
            },
        }
    }
}

/// What the palette lists from, as the Workspace has it now: the workspaces in display order,
/// the actions that apply where the focus was, the settings, the files, and the marks' clock.
pub struct PaletteData {
    pub workspaces: Vec<Entry>,
    pub actions: Vec<ActionRow>,
    pub settings: Vec<PaletteSetting>,
    pub files: FilesData,
    pub phase: f32,
}

pub enum PaletteEvent {
    /// Do this row (never a `… more` row: the palette handles those itself).
    Pick(Key),
    /// It shows files now (the scope or the query changed): list them if they are due.
    WantFiles,
    /// Escape, a click outside, or the box lost focus.
    Cancel,
}

/// The ⌘K panel.
pub struct Palette {
    input: Entity<NameInput>,
    /// The box's text as last listed.
    query: String,
    /// The chosen chip (a `>` query on All shows Actions: `scope`).
    base: Scope,
    data: PaletteData,
    /// The Workspace's, as it was when the palette opened (a pick closes it).
    recent: Recent,
    lines: Vec<Line>,
    /// The places in `lines` of the rows that can be chosen.
    rows: Vec<usize>,
    /// The chosen one, a place in `rows`.
    selected: usize,
    /// It was moved (↑↓) since the list last went back to its first row: a ranking's answer
    /// then keeps it on its item; else the answer's best row is chosen.
    moved: bool,
    scroll: ScrollHandle,
    /// The files' ranking: bumped for each new one (a running one stops when it sees it moved).
    generation: Arc<AtomicU64>,
    /// The last ranking's answer: its generation, the best `FILE_RESULTS` and how many matched.
    /// Kept while a newer one runs (so the rows don't blink), dropped when files can't show.
    hits: Option<(u64, Vec<FileHit>, usize)>,
    /// What the running or last ranking was asked: the query and the listing.
    ranked: Option<(String, Arc<Vec<String>>)>,
    ranking: Option<Task<()>>,
    /// Where the panel and its chips were last drawn (the UI screenshots read them).
    panel_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    chip_bounds: Rc<RefCell<[Option<Bounds<Pixels>>; 5]>>,
    _input_events: Subscription,
    _input_changes: Subscription,
}

impl EventEmitter<PaletteEvent> for Palette {}

impl Focusable for Palette {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.input.focus_handle(cx)
    }
}

/// The key of a row that can be chosen.
fn line_key(line: &Line) -> Option<Key> {
    match line {
        Line::Item { key, .. } => Some(key.clone()),
        Line::More { kind, .. } => Some(Key::More(*kind)),
        Line::Header(_) | Line::Note(_) => None,
    }
}

impl Palette {
    pub fn new(
        scope: Scope,
        data: PaletteData,
        recent: Recent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let input = cx.new(|cx| NameInput::new(String::new(), window, cx));
        let input_events = cx.subscribe_in(&input, window, |this, _, event, _, cx| match event {
            NameInputEvent::Confirm(_) => this.pick(this.selected, cx),
            NameInputEvent::Cancel => cx.emit(PaletteEvent::Cancel),
        });
        let input_changes = cx.observe(&input, |this, _, cx| this.query_changed(cx));
        let mut this = Self {
            input,
            query: String::new(),
            base: scope,
            data,
            recent,
            lines: Vec::new(),
            rows: Vec::new(),
            selected: 0,
            moved: false,
            scroll: ScrollHandle::new(),
            generation: Arc::default(),
            hits: None,
            ranked: None,
            ranking: None,
            panel_bounds: Rc::default(),
            chip_bounds: Rc::default(),
            _input_events: input_events,
            _input_changes: input_changes,
        };
        this.relist(false);
        this
    }

    /// The scope shown: the chosen chip's, or Actions for a `>` query on All.
    pub fn scope(&self) -> Scope {
        parse_query(self.base, &self.query).0
    }

    /// Show `scope` (a chip, Tab, ⌘K or ⌘P, a `… more` row), the text kept, the first row
    /// chosen.
    pub fn set_scope(&mut self, scope: Scope, cx: &mut Context<Self>) {
        self.base = scope;
        self.relist(false);
        self.rank_files_for(cx);
        self.want_files(cx);
        cx.notify();
    }

    /// Whether the scope shown can list files (All, without `>`, or Files).
    fn shows_files(&self) -> bool {
        matches!(self.scope(), Scope::All | Scope::Files)
    }

    /// Ask the Workspace to list the files (if due) while they can show.
    fn want_files(&self, cx: &mut Context<Self>) {
        if self.shows_files() {
            cx.emit(PaletteEvent::WantFiles);
        }
    }

    /// Rank the listed files for the query on the background executor, when the query or the
    /// listing changed and the scope shows files; a newer ranking stops an older one, and an
    /// older one's answer is dropped. With no query, no listing or a scope without files, the
    /// hits go.
    fn rank_files_for(&mut self, cx: &mut Context<Self>) {
        let query = parse_query(self.base, &self.query).1.trim().to_string();
        let paths = match &self.data.files {
            FilesData::Ready { paths, .. } if self.shows_files() && !query.is_empty() => {
                paths.clone()
            }
            _ => {
                if self.ranked.take().is_some() || self.hits.is_some() {
                    self.generation.fetch_add(1, Ordering::Relaxed);
                    self.hits = None;
                    self.ranking = None;
                }
                return;
            }
        };
        if self
            .ranked
            .as_ref()
            .is_some_and(|(q, p)| *q == query && Arc::ptr_eq(p, &paths))
        {
            return;
        }
        let asked = self.generation.fetch_add(1, Ordering::Relaxed) + 1;
        self.ranked = Some((query.clone(), paths.clone()));
        let current = self.generation.clone();
        let ranking = cx.background_executor().spawn(async move {
            let answer = rank_files(&paths, &query, FILE_RESULTS, asked, &current);
            (answer, current)
        });
        self.ranking = Some(cx.spawn(async move |this, cx| {
            let (answer, current) = ranking.await;
            let Some(hits) = fresh_answer(&current, asked, answer) else {
                return;
            };
            let _ = this.update(cx, |this, cx| {
                this.hits = Some(hits);
                // Rows chosen from an older answer aren't kept: the best of this one is.
                this.relist(this.moved);
                cx.notify();
            });
        }));
    }

    /// What can change while it's open, as it is now (each time the Workspace draws): the
    /// workspaces, the files and the marks' clock. The actions and the settings stay as they
    /// were at open. The chosen row stays on its item while it is still listed; else the first
    /// is chosen.
    /// The files are compared by their listing's pointer (`FilesData::same`), so this stays
    /// cheap with 50,000 paths; a new listing is ranked again.
    pub fn refresh(
        &mut self,
        workspaces: Vec<Entry>,
        files: FilesData,
        phase: f32,
        cx: &mut Context<Self>,
    ) {
        self.data.phase = phase;
        let files_changed = !files.same(&self.data.files);
        if workspaces != self.data.workspaces || files_changed {
            if files_changed && files.root() != self.data.files.root() {
                // Hits from another folder's listing name files that aren't there.
                self.hits = None;
            }
            self.data.workspaces = workspaces;
            self.data.files = files;
            self.relist(true);
            if files_changed {
                self.rank_files_for(cx);
            }
        }
    }

    /// The workspaces listed, in order (for the marks' ticker and the age refresh).
    pub fn shown_workspaces(&self) -> impl Iterator<Item = &Entry> {
        self.lines.iter().filter_map(|line| match line {
            Line::Item {
                key: Key::Workspace(id),
                ..
            } => self.data.workspaces.iter().find(|e| e.id == *id),
            _ => None,
        })
    }

    /// The chosen row's key; none when nothing can be chosen.
    pub fn selected_key(&self) -> Option<&Key> {
        let line = self.lines.get(*self.rows.get(self.selected)?)?;
        match line {
            Line::Item { key, .. } => Some(key),
            // `line_key` makes a key of a `… more` row; here it has to be borrowed.
            Line::More { kind, .. } => MORE_KEYS.iter().find(|k| k.kind() == *kind),
            Line::Header(_) | Line::Note(_) => None,
        }
    }

    /// The box changed (typing, or the IME's composition): list again if its text did, from
    /// the first row.
    fn query_changed(&mut self, cx: &mut Context<Self>) {
        let query = self.input.read(cx).text().to_string();
        if query != self.query {
            self.query = query;
            self.rank_files_for(cx);
            self.relist(false);
            self.want_files(cx);
            cx.notify();
        }
    }

    /// Rebuild the lines. `keep`: the chosen item stays chosen if it is still listed; else (or
    /// not `keep`) the first row is chosen and the list goes back to its top.
    fn relist(&mut self, keep: bool) {
        let before = self.selected_key().cloned().filter(|_| keep);
        let (scope, query) = parse_query(self.base, &self.query);
        // A Recent file is shown while its listing still has it; compose checks the rest.
        let files = &self.data.files;
        let valid = |key: &Key| match key {
            Key::File(path) => files.lists(path),
            _ => true,
        };
        self.lines = compose(&Sources {
            scope,
            query,
            workspaces: &self.data.workspaces,
            actions: &self.data.actions,
            settings: &self.data.settings,
            files: self.data.files.as_files(self.hits.as_ref()),
            recent: &self.recent,
            valid: &valid,
        });
        self.rows = selectable(&self.lines);
        let kept = before.and_then(|key| {
            self.rows
                .iter()
                .position(|&at| line_key(&self.lines[at]).as_ref() == Some(&key))
        });
        match kept {
            Some(at) => {
                if at != self.selected {
                    self.selected = at;
                    self.scroll.scroll_to_item(self.rows[at]);
                }
            }
            None => {
                self.selected = 0;
                self.moved = false;
                self.scroll.scroll_to_item(0);
            }
        }
    }

    fn select_by(&mut self, delta: i32, cx: &mut Context<Self>) {
        self.selected = move_selection(self.selected, self.rows.len(), delta);
        self.moved = true;
        if let Some(&at) = self.rows.get(self.selected) {
            self.scroll.scroll_to_item(at);
        }
        cx.notify();
    }

    /// Do row `at` (a place in `rows`; nothing if there is none, e.g. nothing matches): a `…
    /// more` row shows its kind's scope; anything else goes to the Workspace.
    fn pick(&mut self, at: usize, cx: &mut Context<Self>) {
        let Some(key) = self
            .rows
            .get(at)
            .and_then(|&line| line_key(&self.lines[line]))
        else {
            return;
        };
        match key {
            Key::More(kind) => self.set_scope(Scope::of(kind), cx),
            key => cx.emit(PaletteEvent::Pick(key)),
        }
    }

    /// The name of an item's row and where its matched letters are.
    fn item_name(&self, key: &Key) -> String {
        match key {
            Key::Workspace(id) => self
                .data
                .workspaces
                .iter()
                .find(|e| e.id == *id)
                .map(|e| e.label.clone())
                .unwrap_or_default(),
            Key::Action(_) | Key::Preset(_) => self
                .data
                .actions
                .iter()
                .find(|r| r.key == *key)
                .map(|r| r.name.clone())
                .unwrap_or_default(),
            Key::Setting(name) => name.to_string(),
            Key::File(path) => path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            Key::More(_) => String::new(),
        }
    }

    /// A row of the list: its line (`at`), chosen or not.
    fn render_line(
        &self,
        at: usize,
        line: &Line,
        chosen: bool,
        ui: &UiColors,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = cx.global::<Appearance>().theme;
        let row = match line {
            Line::Header(title) => return render_group(title.clone(), at == 0, ui),
            Line::Note(note) => {
                return div()
                    .flex_none()
                    .px(px(8.0))
                    .py(px(4.0))
                    .text_size(px(text::NAME))
                    .text_color(hsla(ui.text_dim))
                    .child(note.clone())
                    .into_any_element();
            }
            Line::More { kind, count } => item_row(chosen, ui, None).child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .truncate()
                    .text_color(hsla(ui.text_dim))
                    .child(more_label(*kind, *count)),
            ),
            Line::Item {
                key: Key::Workspace(id),
                positions,
            } => {
                let Some((place, entry)) = self
                    .data
                    .workspaces
                    .iter()
                    .enumerate()
                    .find(|(_, e)| e.id == *id)
                else {
                    return div().into_any_element();
                };
                let highlights = byte_ranges(&entry.label, positions);
                workspace_row(entry, place + 1, chosen, &highlights, self.data.phase, ui)
            }
            // The file's icon in its kind's ink, its name, then (6 pt on) its folder, faint and
            // cut at its start.
            Line::Item {
                key: Key::File(path),
                positions,
            } => {
                let rel = self
                    .data
                    .files
                    .root()
                    .and_then(|root| path.strip_prefix(root).ok())
                    .unwrap_or(path)
                    .to_string_lossy()
                    .into_owned();
                let (name, name_at, folder, folder_at) = file_parts(&rel, positions);
                let ink = palette_file_ink(file_kind(&name), &theme);
                item_row(chosen, ui, Some(glyph(Glyph::File, ink)))
                    .child(name_cell(&name, &name_at, chosen).flex_initial())
                    .when(!folder.is_empty(), |row| {
                        row.child(
                            div()
                                .flex_1()
                                .min_w(px(0.0))
                                .ml(px(6.0))
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis_start()
                                .text_size(px(text::META))
                                .text_color(hsla(ui.text_faint))
                                .child(highlighted(&folder, &folder_at)),
                        )
                    })
            }
            Line::Item { key, positions } => {
                let name = self.item_name(key);
                let right = match key {
                    Key::Action(_) | Key::Preset(_) => self
                        .data
                        .actions
                        .iter()
                        .find(|r| r.key == *key)
                        .and_then(|r| r.keys.clone())
                        .map(|keys| (keys, ui.text_dim)),
                    Key::Setting(name) => self
                        .data
                        .settings
                        .iter()
                        .find(|s| s.name == *name)
                        .map(|s| (s.section.title().to_string(), ui.text_faint)),
                    _ => None,
                };
                let icon = matches!(key, Key::Setting(_)).then(|| glyph(Glyph::Gear, ui.text_dim));
                item_row(chosen, ui, icon)
                    .child(name_cell(&name, positions, chosen))
                    .when_some(right, |row, (right, color)| {
                        row.child(
                            div()
                                .flex_none()
                                .ml(px(8.0))
                                .text_size(px(text::META))
                                .font_features(tabular())
                                .text_color(hsla(color))
                                .child(right),
                        )
                    })
            }
        };
        let Some(place) = self.rows.iter().position(|&l| l == at) else {
            return row.into_any_element();
        };
        row.id(("palette-row", at))
            .cursor_pointer()
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.selected = place;
                this.pick(place, cx);
            }))
            .into_any_element()
    }

    /// A scope's chip: filled `row_active` while it's the scope shown; a click shows it.
    fn render_chip(&self, scope: Scope, cx: &mut Context<Self>, ui: &UiColors) -> AnyElement {
        let current = scope == self.scope();
        let hover = ui.panel_hover;
        let at = Scope::ALL.iter().position(|s| *s == scope).unwrap_or(0);
        let bounds = self.chip_bounds.clone();
        div()
            .id(("palette-chip", at))
            .relative()
            .flex_none()
            .flex()
            .items_center()
            .h(px(CHIP))
            .px(px(10.0))
            .rounded(px(5.0))
            .text_size(px(text::META))
            .when(current, |chip| {
                chip.bg(hsla(ui.row_active)).text_color(hsla(ui.text))
            })
            .when(!current, |chip| {
                chip.text_color(hsla(ui.text_dim))
                    .hover(move |s| s.bg(hsla(hover)))
            })
            .cursor_pointer()
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.set_scope(scope, cx)))
            .child(scope.label())
            .child(
                canvas(
                    move |b, _, _| bounds.borrow_mut()[at] = Some(b),
                    |_, _, _, _| {},
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
            .into_any_element()
    }
}

/// A `… more` row's key, to lend out (`Palette::selected_key`).
static MORE_KEYS: [Key; 4] = [
    Key::More(crate::palette_items::Kind::Workspace),
    Key::More(crate::palette_items::Kind::Action),
    Key::More(crate::palette_items::Kind::File),
    Key::More(crate::palette_items::Kind::Setting),
];

/// An item's row (not a workspace's): `ROW` tall, its icon slot (empty but for a file's or a
/// setting's icon, so the names line up), filled `row_active` while chosen, `panel_hover` under
/// the pointer.
fn item_row(chosen: bool, ui: &UiColors, icon: Option<Div>) -> Div {
    let hover = ui.panel_hover;
    div()
        .flex_none()
        .flex()
        .items_center()
        .h(px(ROW))
        .px(px(8.0))
        .rounded(px(6.0))
        .text_size(px(text::NAME))
        .when(chosen, |row| row.bg(hsla(ui.row_active)))
        .when(!chosen, |row| row.hover(move |s| s.bg(hsla(hover))))
        .child(
            div()
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .w(px(MARK_SLOT))
                .mr(px(SLOT_GAP))
                .children(icon),
        )
}

/// `text` with the chars at `positions` semibold.
fn highlighted(text: &str, positions: &[usize]) -> StyledText {
    let matched = HighlightStyle {
        font_weight: Some(FontWeight::SEMIBOLD),
        ..Default::default()
    };
    let ranges = byte_ranges(text, positions);
    StyledText::new(text.to_string()).with_highlights(ranges.into_iter().map(|r| (r, matched)))
}

/// A row's name, cut at its end, the matched letters (char `positions`) semibold; medium while
/// chosen.
fn name_cell(name: &str, positions: &[usize], chosen: bool) -> Div {
    div()
        .flex_1()
        .min_w(px(0.0))
        .truncate()
        .when(chosen, |cell| cell.font_weight(FontWeight::MEDIUM))
        .child(highlighted(name, positions))
}

/// The UI screenshots' look inside.
#[cfg(feature = "ui-shots")]
impl Palette {
    pub fn lines(&self) -> &[Line] {
        &self.lines
    }

    /// Where line `at` was last drawn.
    pub fn row_bounds(&self, at: usize) -> Option<Bounds<Pixels>> {
        self.scroll.bounds_for_item(at)
    }

    pub fn panel_bounds(&self) -> Option<Bounds<Pixels>> {
        self.panel_bounds.get()
    }

    pub fn chip_bounds(&self, scope: Scope) -> Option<Bounds<Pixels>> {
        let at = Scope::ALL.iter().position(|s| *s == scope)?;
        self.chip_bounds.borrow()[at]
    }

    /// The keys an action's row shows (by its name).
    pub fn action_keys(&self, name: &str) -> Option<String> {
        self.data
            .actions
            .iter()
            .find(|r| r.name == name)
            .and_then(|r| r.keys.clone())
    }
}

impl Render for Palette {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ui = cx.global::<Appearance>().theme.ui();
        let chosen_line = self.rows.get(self.selected).copied();
        let rows: Vec<AnyElement> = self
            .lines
            .iter()
            .enumerate()
            .map(|(at, line)| self.render_line(at, line, chosen_line == Some(at), &ui, cx))
            .collect();
        let list = if rows.is_empty() {
            div()
                .px(px(8.0))
                .py(px(4.0))
                .text_size(px(text::NAME))
                .text_color(hsla(ui.text_dim))
                .child(crate::palette_items::NOTHING)
                .into_any_element()
        } else {
            let max_h = list_max_height(f32::from(window.viewport_size().height));
            div()
                .id("palette-rows")
                .flex()
                .flex_col()
                .gap(px(ROW_GAP))
                .max_h(px(max_h))
                .overflow_y_scroll()
                .track_scroll(&self.scroll)
                .children(rows)
                .into_any_element()
        };
        let chips: Vec<AnyElement> = Scope::ALL
            .iter()
            .map(|&scope| self.render_chip(scope, cx, &ui))
            .collect();

        let empty = self.input.read(cx).text().is_empty();
        let field = overlay_panel::filter_box(&ui, &self.input, empty, self.base.placeholder());
        let bounds = self.panel_bounds.clone();

        overlay_panel::shell(&ui, PANEL_WIDTH)
            .relative()
            .key_context("Palette")
            .on_action(cx.listener(|this, _: &SelectPrev, _, cx| this.select_by(-1, cx)))
            .on_action(cx.listener(|this, _: &SelectNext, _, cx| this.select_by(1, cx)))
            .on_action(cx.listener(|this, _: &NextScope, _, cx| {
                let next = this.scope().next();
                this.set_scope(next, cx)
            }))
            .on_action(cx.listener(|this, _: &PrevScope, _, cx| {
                let prev = this.scope().prev();
                this.set_scope(prev, cx)
            }))
            // The terminal under the panel gets neither its clicks nor its hover.
            .occlude()
            .on_mouse_down_out(
                cx.listener(|_, _: &MouseDownEvent, _, cx| cx.emit(PaletteEvent::Cancel)),
            )
            .child(field)
            .child(div().flex().gap(px(4.0)).m(px(2.0)).children(chips))
            .child(list)
            .child(
                div()
                    .px(px(8.0))
                    .pt(px(5.0))
                    .border_t_1()
                    .border_color(hsla(ui.panel_border))
                    .text_size(px(text::META))
                    .text_color(hsla(ui.text_faint))
                    .child(HINT),
            )
            .child(
                canvas(move |b, _, _| bounds.set(Some(b)), |_, _, _, _| {})
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full(),
            )
    }
}

/// The keys' table as `main` binds it (persistent sessions' ⌥⌘Q only while they're on): where
/// an action row's keys come from, as the Help list's.
fn binding_table(persistent: bool) -> Vec<(String, String)> {
    let mut bindings = crate::new_bindings().to_vec();
    bindings.extend(crate::key_bindings());
    if persistent {
        bindings.extend(crate::persistent_bindings());
    }
    binding_pairs(&bindings)
}

impl Workspace {
    /// The kind of tab that has the focus (what the palette's actions apply to).
    fn focused_kind(&self) -> Focused {
        let tab = self.model.focused_tab().and_then(|t| self.model.tab(t));
        match tab.map(|t| &t.kind) {
            Some(TabKind::Terminal(_)) => Focused::Terminal,
            Some(TabKind::Browser { .. }) => Focused::Browser,
            Some(TabKind::File { .. }) => Focused::File,
            None => Focused::Nothing,
        }
    }

    /// The palette's sources as it opens: the actions that apply where the focus is now (the
    /// palette takes it next), and the rest as `refresh_palette` keeps giving them.
    fn palette_data(&self, actions: Vec<ActionRow>, cx: &App) -> PaletteData {
        PaletteData {
            workspaces: self.switcher_entries(cx),
            actions,
            settings: crate::preferences::palette_settings(),
            files: self.palette_files_data(cx),
            phase: self.phase,
        }
    }

    /// What the palette has of the followed terminal's files (`files_data`).
    fn palette_files_data(&self, cx: &App) -> FilesData {
        let folder = self.palette_folder(cx);
        let cached = folder
            .as_ref()
            .and_then(|(folder, _)| self.palette_files.get(folder));
        files_data(folder.clone(), cached)
    }

    /// ⌘K (`All`), ⌘P (`Workspaces`) or the row menu's Show All Workspaces: open the palette on
    /// `scope`; open on it already, close it; open on another, show `scope`, the text kept
    /// (`palette_step`). Any other box over the window closes first.
    pub(crate) fn open_palette(
        &mut self,
        scope: Scope,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let open = self.palette.as_ref().map(|p| p.read(cx).base);
        match palette_step(open, scope) {
            Step::Close => {
                self.palette = None;
                self.focus_active(window, cx);
            }
            Step::Rescope => {
                if let Some(palette) = self.palette.clone() {
                    palette.update(cx, |p, cx| p.set_scope(scope, cx));
                    palette.focus_handle(cx).focus(window, cx);
                }
            }
            Step::Open => {
                self.close_overlays(Overlay::Palette);
                // On All or Files, the listing starts now, so it's there once something is typed.
                if matches!(scope, Scope::All | Scope::Files) {
                    self.list_palette_files(cx);
                }
                let presets: Vec<String> = cx
                    .global::<Appearance>()
                    .presets
                    .iter()
                    .map(|p| p.name.clone())
                    .collect();
                let actions = action_rows(
                    self.focused_kind(),
                    self.persistent,
                    &presets,
                    &binding_table(self.persistent),
                );
                let data = self.palette_data(actions, cx);
                let recent = self.palette_recent.clone();
                let palette = cx.new(|cx| Palette::new(scope, data, recent, window, cx));
                cx.subscribe_in(&palette, window, Self::on_palette_event)
                    .detach();
                palette.focus_handle(cx).focus(window, cx);
                self.palette = Some(palette);
            }
        }
        cx.notify();
    }

    /// The palette picked a row or was closed.
    fn on_palette_event(
        &mut self,
        palette: &Entity<Palette>,
        event: &PaletteEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // A palette already closed or replaced can still report losing focus: ignore it.
        if self.palette.as_ref() != Some(palette) {
            return;
        }
        match event {
            PaletteEvent::Pick(key) => self.run_pick(key.clone(), window, cx),
            PaletteEvent::WantFiles => self.list_palette_files(cx),
            PaletteEvent::Cancel => {
                self.palette = None;
                self.focus_active(window, cx);
                cx.notify();
            }
        }
    }

    /// Do a pick: the palette closes and the keyboard goes back to the focused tab, then the
    /// pick runs from there (an action is dispatched as its key would be, so it lands where the
    /// key would); it goes to the front of Recent. A `… more` row shows its scope instead, the
    /// palette left open.
    pub(crate) fn run_pick(&mut self, key: Key, window: &mut Window, cx: &mut Context<Self>) {
        if let Key::More(kind) = key {
            if let Some(palette) = &self.palette {
                palette.update(cx, |p, cx| p.set_scope(Scope::of(kind), cx));
            }
            return;
        }
        self.palette = None;
        self.focus_active(window, cx);
        self.palette_recent.push(key.clone());
        match key {
            Key::Action(name) => match COMMANDS.iter().find(|c| c.name == name).map(|c| c.run) {
                Some(Run::Action(make)) => window.dispatch_action(make(), cx),
                Some(Run::NewBrowserTab) => {
                    if let Some(pane) = self.model.focused_pane() {
                        self.new_browser_tab(pane, window, cx);
                    }
                }
                None => {}
            },
            Key::Preset(n) => self.launch_preset(n, window, cx),
            // As ⌘P always did: its most urgent terminal (its pane and tab shown), else the
            // workspace as it was (one of browser or file tabs only).
            Key::Workspace(id) => {
                let ws = WorkspaceId(id);
                match self.rollup_session(ws) {
                    Some(index) => self.select(index, window, cx),
                    None => self.select_workspace(ws, window, cx),
                }
                self.focus_active(window, cx);
            }
            Key::File(path) => self.open_file_tab(path, FileMode::Plain, window, cx),
            // Preferences, on that setting's section.
            Key::Setting(name) => {
                let settings = crate::preferences::palette_settings();
                if let Some(setting) = settings.iter().find(|s| s.name == name) {
                    crate::preferences::open_at(setting.section, cx);
                }
            }
            Key::More(_) => {}
        }
        cx.notify();
    }

    /// Give the open palette what changes while it's open (each time the Workspace draws).
    pub(crate) fn refresh_palette(&self, cx: &mut Context<Self>) {
        if let Some(palette) = &self.palette {
            let (workspaces, files) = (self.switcher_entries(cx), self.palette_files_data(cx));
            let phase = self.phase;
            palette.update(cx, |p, cx| p.refresh(workspaces, files, phase, cx));
        }
    }

    /// The open palette's workspace rows (each one's status and since when); none while it's
    /// closed.
    pub(crate) fn palette_shown(&self, cx: &App) -> Vec<(Status, Instant)> {
        self.palette
            .as_ref()
            .map(|p| {
                p.read(cx)
                    .shown_workspaces()
                    .map(|e| (e.status, e.since))
                    .collect()
            })
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::every_theme;
    use crate::ui_tokens::{TEXT_MIN, contrast};

    #[test]
    fn palette_inks_meet_the_floors() {
        let mut failures = Vec::new();
        for (name, theme) in every_theme() {
            let ui = theme.ui();
            let surfaces = [
                ("panel", ui.panel),
                ("panel_hover", ui.panel_hover),
                ("row_active", ui.row_active),
            ];
            for (bg_name, bg) in surfaces {
                for (fg_name, fg) in [
                    ("text", ui.text),
                    ("text_dim", ui.text_dim),
                    ("text_faint", ui.text_faint),
                ] {
                    let c = contrast(fg, bg);
                    if c < TEXT_MIN {
                        failures.push(format!("{name}: {fg_name} on {bg_name} {c:.2}"));
                    }
                }
                for kind in FileKind::ALL {
                    let c = contrast(palette_file_ink(kind, &theme), bg);
                    if c < MARK_MIN {
                        failures.push(format!("{name}: {kind:?} ink on {bg_name} {c:.2}"));
                    }
                }
            }
        }
        assert!(failures.is_empty(), "{failures:#?}");
    }

    fn cached(paths: &[&str], more: usize, walked: bool, failed: bool) -> CachedFiles {
        CachedFiles {
            root: PathBuf::from("/w/fuxx-cli"),
            paths: Arc::new(paths.iter().map(|p| p.to_string()).collect()),
            more,
            walked,
            failed,
            listed_at: Instant::now(),
            wait: crate::palette_files::FRESH,
        }
    }

    #[test]
    fn files_data_says_what_there_is() {
        use crate::palette_items::{NO_FOLDER, SSH_NOTE};
        let folder = || Some((PathBuf::from("/w/fuxx-cli/src"), false));
        let ok = cached(&["a.rs"], 0, false, false);
        assert_eq!(files_data(None, None), FilesData::Note(NO_FOLDER));
        assert_eq!(files_data(None, Some(&ok)), FilesData::Note(NO_FOLDER));
        // An SSH terminal: the note, whatever is cached.
        let ssh = Some((PathBuf::from("/srv"), true));
        assert_eq!(files_data(ssh, Some(&ok)), FilesData::Note(SSH_NOTE));
        // Nothing listed yet (a first listing running, or about to): Listing.
        assert_eq!(files_data(folder(), None), FilesData::Listing);
        let failed = cached(&[], 0, false, true);
        assert_eq!(files_data(folder(), Some(&failed)), FilesData::Failed);
        let walk = cached(&["a.rs", "b.rs"], 3, true, false);
        match files_data(folder(), Some(&walk)) {
            FilesData::Ready {
                root,
                paths,
                more,
                walked,
            } => {
                assert_eq!(root, PathBuf::from("/w/fuxx-cli"));
                assert!(Arc::ptr_eq(&paths, &walk.paths), "the listing's own paths");
                assert_eq!((more, walked), (3, true));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_capped_walk_tells_compose_so() {
        let data = FilesData::Ready {
            root: PathBuf::from("/w/notes"),
            paths: Arc::new(vec!["a".into()]),
            more: 5,
            walked: true,
        };
        match data.as_files(None) {
            Files::Ready { capped, walked, .. } => assert!(capped && walked),
            _ => panic!("ready"),
        }
        let hits = (
            1,
            vec![FileHit {
                rel: "a".into(),
                positions: vec![0],
                score: 40,
            }],
            9,
        );
        match data.as_files(Some(&hits)) {
            Files::Ready { hits, total, .. } => assert_eq!((hits.len(), total), (1, 9)),
            _ => panic!("ready"),
        }
    }

    #[test]
    fn files_data_compares_listings_by_pointer() {
        let ready = |paths: &Arc<Vec<String>>| FilesData::Ready {
            root: PathBuf::from("/w"),
            paths: paths.clone(),
            more: 0,
            walked: false,
        };
        let one = Arc::new(vec!["a".to_string()]);
        let twin = Arc::new(vec!["a".to_string()]);
        assert!(ready(&one).same(&ready(&one)));
        // A new listing (even with the same paths) is a change: no deep comparison.
        assert!(!ready(&one).same(&ready(&twin)));
        assert!(FilesData::Listing.same(&FilesData::Listing));
        assert!(!FilesData::Listing.same(&FilesData::Failed));
        assert!(!FilesData::Failed.same(&ready(&one)));
    }

    #[test]
    fn an_older_rankings_answer_is_dropped() {
        let current = AtomicU64::new(4);
        let answer = || Some((Vec::new(), 7));
        assert_eq!(
            fresh_answer(&current, 4, answer()),
            Some((4, Vec::new(), 7))
        );
        assert_eq!(fresh_answer(&current, 3, answer()), None);
        assert_eq!(fresh_answer(&current, 4, None), None);
    }

    #[test]
    fn a_file_row_splits_into_name_and_folder() {
        // "view" in src/view.rs: 4 5 6 7.
        let (name, at, folder, folder_at) = file_parts("src/view.rs", &[0, 4, 5, 6, 7]);
        assert_eq!((name.as_str(), folder.as_str()), ("view.rs", "src"));
        assert_eq!(at, [0, 1, 2, 3]);
        assert_eq!(folder_at, [0]);
        let (name, at, folder, _) = file_parts("README", &[0, 1]);
        assert_eq!((name.as_str(), folder.as_str()), ("README", ""));
        assert_eq!(at, [0, 1]);
        // Chars, not bytes.
        let (name, at, folder, _) = file_parts("d\u{fc}r/\u{e9}t\u{e9}.md", &[4, 5]);
        assert_eq!(
            (name.as_str(), folder.as_str()),
            ("\u{e9}t\u{e9}.md", "d\u{fc}r")
        );
        assert_eq!(at, [0, 1]);
    }

    #[test]
    fn the_list_is_at_most_420_and_keeps_200_from_the_bottom() {
        assert_eq!(list_max_height(1000.0), 420.0);
        assert_eq!(list_max_height(600.0), 400.0);
        assert_eq!(list_max_height(300.0), 120.0);
        assert_eq!(list_max_height(100.0), 120.0);
    }
}
