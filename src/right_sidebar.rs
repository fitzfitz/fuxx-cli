//! The right sidebar (M11g): one per window, on the right of the panes, showing the focused
//! terminal's files.
//!
//! It opens and closes with ⌥⌘\, View → Toggle Right Sidebar, or the icon at the far right of
//! every pane's tab bar (`tab_bar::BarButton::RightSidebar`, lit while it's open), sliding like
//! the left sidebar (`sidebar_toggle::Slide`, not with Reduce motion on). Its left edge resizes
//! it (220–560 pt, 300 by default). Open state and width live in `sessions.json`. A narrow
//! window draws it narrower (`drawn_width`) so the panes keep 200 pt.
//!
//! It follows the focused terminal (with a browser or file tab focused, the last terminal it
//! followed): that terminal's folder, found like a sidebar row's (`git_branch::refresh_places`:
//! its OSC 7 report, else the kernel's answer, else the folder it started in; an SSH terminal's
//! only as reported). The lookup runs off the UI thread, on a focus change and every 2 s while
//! the sidebar can be seen (`tick_right_sidebar`, which stops once it can't).
//!
//! The Files panel (under a "Files" header level with the tab bars): the tree of that folder's
//! repo (`file_tree::FileTree`), rooted at the repo's top folder with the terminal's folder
//! open and selected, else at the folder itself. Folders are listed one at a time as they open,
//! each with one `git check-ignore` batch; marks come from `git status`. All of it runs on the
//! background executor, again when the sidebar is shown, when the terminal or its folder
//! changes, and every 2 s while it can be seen. A filter box above it; ↑ ↓ → ← Return and Esc
//! in it (key context `FileTree`); a click on a file opens it in a file tab. The panel has its
//! own focus: closing the sidebar while it has the keyboard gives the keyboard to the focused
//! tab. An SSH terminal's files aren't shown.
//!
//! `clamp`, `drawn_width`, `width_for_edge`, `resolve_folder` and `lookup_due` are pure and
//! tested, as is the tree.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

use alacritty_terminal::vte::ansi::Rgb;
use gpui::{
    AnyElement, App, ClickEvent, Context, DispatchPhase, Entity, FocusHandle, Focusable,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ScrollStrategy, Subscription,
    UniformListScrollHandle, Window, actions, canvas, div, prelude::*, px, uniform_list,
};

use crate::appearance::Appearance;
use crate::file_tree::{self, FileTree, Row, RowCache, Want};
use crate::git_branch::{find_git, is_this_mac, process_cwd};
use crate::git_status::{self, DirEntry, Mark};
use crate::name_input::{NameInput, NameInputEvent};
use crate::overlay_panel;
use crate::reduce_motion::WindowVisible;
use crate::restore::SavedState;
use crate::sidebar_toggle::{self, Slide};
use crate::terminal_view::hsla;
use crate::theme::{Theme, UiColors};
use crate::ui_tokens::size::{
    DIVIDER_ACTIVE, RIGHT_SIDEBAR_DEFAULT_WIDTH, RIGHT_SIDEBAR_MAX_WIDTH, RIGHT_SIDEBAR_MIN_WIDTH,
    TAB_BAR,
};
use crate::ui_tokens::{MARK_MIN, ensure_contrast, text};
use crate::workspace::Workspace;

// Bound in main.rs with the "FileTree" key context (the panel, its filter box included).
actions!(
    file_tree,
    [
        SelectPrev,
        SelectNext,
        ExpandSelected,
        CollapseSelected,
        OpenSelected,
        BackToPane
    ]
);

/// How often the followed terminal's folder is looked up again while the sidebar can be seen,
/// and the Files panel refreshed.
pub const REFRESH: Duration = Duration::from_secs(2);
/// The strip along its left edge that resizes it.
const RESIZE_STRIP: f32 = 4.0;
/// The panel's name, in its header.
const PANEL: &str = "Files";
/// A tree row's height.
pub(crate) const ROW: f32 = 22.0;
/// The band holding the filter box, under the header.
pub(crate) const FILTER_BAND: f32 = 34.0;
/// How far each level of the tree is indented.
const INDENT: f32 = 12.0;
const FILTER_PLACEHOLDER: &str = "Filter files";
const SSH_NOTE: &str = "Files aren't shown for SSH terminals.";
const NO_FOLDER: &str = "No terminal folder to show.";
const NO_MATCH: &str = "No file matches.";

/// A mark's colour on the sidebar: the theme's ANSI blue (M), green (A) or red (D), pushed to
/// read on the sidebar's background as status marks do.
pub(crate) fn mark_color(mark: Mark, theme: &Theme) -> Rgb {
    let ansi = match mark {
        Mark::Modified => theme.palette[4],
        Mark::Added => theme.palette[2],
        Mark::Deleted => theme.palette[1],
    };
    ensure_contrast(ansi, theme.ui().sidebar_background, MARK_MIN)
}

fn mark_letter(mark: Mark) -> &'static str {
    match mark {
        Mark::Modified => "M",
        Mark::Added => "A",
        Mark::Deleted => "D",
    }
}

/// Tree row `i`: indented by its depth, a folder's ▸ / ▾, its name and its mark (both in the
/// mark's colour), filled while `selected`; a click on it is `files_click`. A `… N more` row is
/// only its text.
fn file_row(
    i: usize,
    row: &Row,
    selected: Option<&Path>,
    theme: &Theme,
    cx: &mut Context<Workspace>,
) -> AnyElement {
    let ui = theme.ui();
    let base = div()
        .id(("file-row", i))
        .flex()
        .items_center()
        .h(px(ROW))
        .w_full()
        .pl(px(8.0 + row.depth as f32 * INDENT))
        .pr(px(10.0))
        .text_size(px(text::NAME));
    if let Some(more) = row.more {
        return base
            .pl(px(8.0 + row.depth as f32 * INDENT + INDENT))
            .text_color(hsla(ui.text_faint))
            .child(format!("\u{2026} {more} more"))
            .into_any_element();
    }
    let chosen = selected == Some(row.rel.as_path());
    let name = row
        .rel
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let ink = row.mark.map_or(ui.text, |m| mark_color(m, theme));
    let chevron = match (row.is_dir, row.expanded) {
        (true, true) => "\u{25be}",
        (true, false) => "\u{25b8}",
        (false, _) => "",
    };
    let (rel, is_dir) = (row.rel.clone(), row.is_dir);
    let hover = ui.row_hover;
    base.when(chosen, |r| r.bg(hsla(ui.row_active)))
        .when(!chosen, |r| r.hover(move |s| s.bg(hsla(hover))))
        .cursor_pointer()
        .child(
            div()
                .flex_none()
                .w(px(INDENT))
                .text_size(px(text::META))
                .text_color(hsla(ui.text_dim))
                .child(chevron),
        )
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .truncate()
                .text_color(hsla(ink))
                .child(name),
        )
        .children(row.mark.map(|m| {
            div()
                .flex_none()
                .ml(px(6.0))
                .text_size(px(text::META))
                .text_color(hsla(ink))
                .child(mark_letter(m))
        }))
        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
            this.files_click(&rel, is_dir, window, cx)
        }))
        .into_any_element()
}

/// A folder's listing as the background job hands it to the tree.
struct Listed {
    /// Shown entries, sorted, at most `file_tree::CAP`.
    entries: Vec<DirEntry>,
    more: usize,
    /// What git ignores in it (paths from the root).
    ignored: HashSet<PathBuf>,
    /// Its folders holding a `.git`: nested repos and submodules (paths from the root).
    nested: HashSet<PathBuf>,
}

/// Folder `rel`'s listing (from the `root`) as the tree shows it, read off the UI thread: its
/// entries with names precomposed (as git prints them) and sorted; in a repo, what git ignores
/// in it (one batch) and which of its folders are nested repos; then `file_tree::shown_entries`
/// (the ignored and hidden left out before the cap).
fn list_folder(git: Option<&Path>, root: &Path, rel: &Path, in_repo: bool) -> Listed {
    let dir = root.join(rel);
    let (entries, _) = git_status::list_dir(&dir, usize::MAX);
    let mut entries: Vec<DirEntry> = entries
        .into_iter()
        .map(|e| DirEntry {
            name: file_tree::nfc(&e.name),
            is_dir: e.is_dir,
        })
        .collect();
    file_tree::sort_entries(&mut entries);
    let ignored = match git.filter(|_| in_repo) {
        Some(git) => {
            let paths: Vec<PathBuf> = entries.iter().map(|e| rel.join(&e.name)).collect();
            git_status::ignored(git, root, &paths)
        }
        None => HashSet::new(),
    };
    let (entries, more) = file_tree::shown_entries(rel, entries, &ignored, in_repo);
    let nested = entries
        .iter()
        .filter(|e| in_repo && e.is_dir && dir.join(&e.name).join(".git").exists())
        .map(|e| rel.join(&e.name))
        .collect();
    Listed {
        entries,
        more,
        ignored,
        nested,
    }
}

/// The tree's root for a terminal in `folder` (off the UI thread): the repo's top folder, else
/// the folder; both by their real paths (git prints them so: `/private/var`, not `/var`). Also
/// whether it's a repo, and the folder from the root (to open and select).
fn find_root(folder: &Path) -> (PathBuf, bool, Option<PathBuf>) {
    let real = std::fs::canonicalize(folder).unwrap_or_else(|_| folder.to_path_buf());
    let top = find_git().and_then(|git| git_status::repo_root(&git, &real));
    let in_repo = top.is_some();
    let root = top
        .map(|t| std::fs::canonicalize(&t).unwrap_or(t))
        .unwrap_or_else(|| real.clone());
    let revealed = real.strip_prefix(&root).ok().map(Path::to_path_buf);
    (root, in_repo, revealed)
}

/// The Files panel: its tree and what it was made from, its filter box, focus and scroll, and
/// the background work under way.
pub(crate) struct FilesPanel {
    pub(crate) tree: Option<FileTree>,
    /// The tree's root is a repo's.
    in_repo: bool,
    /// The terminal folder the tree was last made for.
    folder: Option<PathBuf>,
    /// Each root's open folders, for the run.
    memory: HashMap<PathBuf, HashSet<PathBuf>>,
    /// What git ignores in each listed folder (all of it goes to the tree).
    ignored: HashMap<PathBuf, HashSet<PathBuf>>,
    /// The rows drawn, built again only when the tree or the filter changes.
    pub(crate) rows: Rc<Vec<Row>>,
    cache: RowCache,
    pub(crate) input: Option<Entity<NameInput>>,
    /// The panel's own focus (filter box and tree): the tree's keys, and what closing the
    /// sidebar checks.
    pub(crate) focus: Option<FocusHandle>,
    /// The filter box's text as last read.
    query: String,
    scroll: UniformListScrollHandle,
    /// Scroll the selected row into view once it's drawn (a new tree, a key).
    reveal: Option<ScrollStrategy>,
    /// A root lookup, a refresh, and folders being listed, under way.
    rooting: bool,
    refreshing: bool,
    listing: HashSet<PathBuf>,
    /// When the last refresh started; none: refresh now.
    refreshed: Option<Instant>,
    _subscriptions: Vec<Subscription>,
}

impl FilesPanel {
    pub(crate) fn new() -> Self {
        Self {
            tree: None,
            in_repo: false,
            folder: None,
            memory: HashMap::new(),
            ignored: HashMap::new(),
            rows: Rc::default(),
            cache: RowCache::default(),
            input: None,
            focus: None,
            query: String::new(),
            scroll: UniformListScrollHandle::new(),
            reveal: None,
            rooting: false,
            refreshing: false,
            listing: HashSet::new(),
            refreshed: None,
            _subscriptions: Vec::new(),
        }
    }

    /// The UI screenshots: show `tree` (listed, marked) as made for a terminal in a folder
    /// outside a repo; nothing is looked up there.
    #[cfg(feature = "ui-shots")]
    pub(crate) fn show_for_shots(&mut self, tree: FileTree) {
        self.folder = Some(tree.root().to_path_buf());
        self.tree = Some(tree);
        self.reveal = Some(ScrollStrategy::Center);
    }

    /// Re-root at `root` (a repo's, `in_repo`) for a terminal at `revealed` in it: the same root
    /// keeps its tree as it is; another remembers this one's open folders and starts afresh
    /// (with that root's, if it was shown before). Work under way for the old root no longer
    /// holds the new one up (its results are dropped).
    fn rooted(&mut self, root: PathBuf, in_repo: bool, revealed: Option<PathBuf>) {
        self.in_repo = in_repo;
        if self.tree.as_ref().is_some_and(|t| t.root() == root) {
            return;
        }
        self.forget();
        let mut tree = FileTree::new(root.clone(), revealed);
        if let Some(open) = self.memory.get(&root) {
            tree.restore_expanded(open.clone());
        }
        self.tree = Some(tree);
        self.reveal = Some(ScrollStrategy::Center);
    }

    /// No tree (an SSH terminal's, or no folder): this one's open folders are remembered, and
    /// nothing under way for it holds a new one up.
    fn forget(&mut self) {
        if let Some(old) = self.tree.take() {
            self.memory
                .insert(old.root().to_path_buf(), old.expanded_set());
        }
        self.ignored.clear();
        self.listing.clear();
        self.refreshing = false;
    }

    /// Folders' listings for the tree at `root` (dropped if the tree has moved on).
    fn listed(&mut self, root: &Path, listings: Vec<(PathBuf, Listed)>) {
        let Some(tree) = self.tree.as_mut().filter(|t| t.root() == root) else {
            return;
        };
        for (rel, listed) in listings {
            self.listing.remove(&rel);
            tree.relist(rel.clone(), listed.entries, listed.more);
            tree.set_nested(&rel, listed.nested);
            self.ignored.insert(rel, listed.ignored);
        }
        // Once for them all.
        tree.set_ignored(self.ignored.values().flatten().cloned().collect());
    }
}

/// A width it can take: within 220–560 pt (the default for a missing or odd one).
pub fn clamp(width: Option<f32>) -> f32 {
    match width {
        Some(w) if w.is_finite() => w.clamp(RIGHT_SIDEBAR_MIN_WIDTH, RIGHT_SIDEBAR_MAX_WIDTH),
        _ => RIGHT_SIDEBAR_DEFAULT_WIDTH,
    }
}

/// The narrowest the pane column gets for the right sidebar (there's no pane minimum elsewhere).
pub const PANE_COLUMN_MIN: f32 = 200.0;

/// How wide it is drawn: its `width`, less what a window `window_width` wide needs to keep the
/// pane column `PANE_COLUMN_MIN` wide beside a left sidebar `left` wide (0 when hidden); never
/// less than none. The saved width stays as chosen.
pub fn drawn_width(width: f32, window_width: f32, left: f32) -> f32 {
    width.min((window_width - left - PANE_COLUMN_MIN).max(0.0))
}

/// Its width with its left edge at `x` in a window `window_width` wide (it ends at the
/// window's right).
pub fn width_for_edge(window_width: f32, x: f32) -> f32 {
    clamp(Some(window_width - x))
}

/// A terminal's folder: what it `reported` (OSC 7, already checked to be about this Mac for a
/// local one), else for a local one the kernel's answer (`cwd`, asked only then) and then the
/// folder it started in (`project`). An SSH terminal's is only what it reported: the rest are
/// this Mac's, not the far machine's.
pub fn resolve_folder(
    reported: Option<PathBuf>,
    ssh: bool,
    cwd: impl FnOnce() -> Option<PathBuf>,
    project: Option<PathBuf>,
) -> Option<PathBuf> {
    if ssh {
        return reported;
    }
    reported.or_else(cwd).or(project)
}

/// Whether terminal `id`'s folder is looked up `now`, the last lookup having been `last` (the
/// terminal and when): for a terminal not looked up last, at once; else every `REFRESH`.
pub fn lookup_due(last: Option<(u64, Instant)>, id: u64, now: Instant) -> bool {
    match last {
        Some((of, at)) if of == id => now.saturating_duration_since(at) >= REFRESH,
        _ => true,
    }
}

/// The right sidebar's state: what's saved (closed, width) and what isn't.
pub struct RightSidebarState {
    pub hidden: bool,
    pub width: f32,
    /// Its left edge is being dragged.
    pub resizing: bool,
    /// It is sliding in or out; none when it isn't moving.
    pub slide: Option<Slide>,
    /// The terminal it follows (`sN`): the focused one, or the last one focused.
    pub(crate) followed: Option<u64>,
    /// The last folder found: for which terminal, the folder (none: not found) and whether that
    /// terminal is an SSH one.
    pub(crate) found: Option<(u64, Option<PathBuf>, bool)>,
    /// The last lookup started: for which terminal, and when.
    looked: Option<(u64, Instant)>,
    /// Whether the 2 s refresh is running (only while the sidebar can be seen).
    ticking: bool,
    /// The Files panel.
    pub(crate) files: FilesPanel,
}

impl RightSidebarState {
    /// Closed, at the default width.
    pub fn new() -> Self {
        Self {
            hidden: true,
            width: RIGHT_SIDEBAR_DEFAULT_WIDTH,
            resizing: false,
            slide: None,
            followed: None,
            found: None,
            looked: None,
            ticking: false,
            files: FilesPanel::new(),
        }
    }

    /// Take its open state and width from the sessions file.
    pub fn load(&mut self, saved: &SavedState) {
        self.hidden = saved.right_sidebar_hidden;
        self.width = clamp(saved.right_sidebar_width);
    }
}

impl Workspace {
    /// ⌥⌘\, View → Toggle Right Sidebar, a pane's icon: open or close the right sidebar. It
    /// slides unless Reduce motion is on (a slide under way turns back from where it got to).
    /// The keyboard stays where it is, unless the Files panel (its filter box or tree) has it:
    /// closing then gives it to the focused tab. (Asked of the panel's own focus, as last drawn:
    /// `Window::focused` keeps answering with a handle no longer drawn, so it can't tell.)
    /// Shown again, the panel refreshes.
    pub(crate) fn toggle_right_sidebar(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let state = &mut self.right_sidebar;
        state.hidden = !state.hidden;
        state.resizing = false;
        let current = state.slide;
        state.slide = (!cx.reduce_motion()).then(|| Slide::start(!state.hidden, current));
        if state.hidden {
            let panel = state.files.focus.as_ref();
            if panel.is_some_and(|f| f.contains_focused(window, cx)) {
                self.focus_active(window, cx);
            }
        } else {
            state.files.refreshed = None;
        }
        self.save();
        cx.notify();
    }

    /// A pane's right-sidebar icon: that pane takes the focus, then the sidebar opens or closes.
    pub(crate) fn right_sidebar_button(
        &mut self,
        pane: crate::layout_model::PaneId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.model.focus_pane(pane) {
            self.sync();
        }
        self.activate(window, cx);
        self.toggle_right_sidebar(window, cx);
    }

    /// The folder the right sidebar shows, and whether its terminal is an SSH one: the followed
    /// terminal's, as last looked up; before the first lookup for it, what is known without
    /// asking the kernel (its report, else the folder it started in). None with no terminal
    /// followed yet, or none found.
    pub(crate) fn right_sidebar_folder(&self, cx: &App) -> Option<(PathBuf, bool)> {
        let id = self.right_sidebar.followed?;
        if let Some((of, folder, ssh)) = &self.right_sidebar.found
            && *of == id
        {
            return folder.clone().map(|f| (f, *ssh));
        }
        let session = self.sessions.items().iter().find(|s| s.id == id)?;
        let ssh = session.ssh.is_some();
        let (reported, _) = self.reported_folder(id, cx)?;
        resolve_folder(reported, ssh, || None, session.project.clone()).map(|f| (f, ssh))
    }

    /// Terminal `id`'s OSC 7 report (counted for a local terminal only when it is about this
    /// Mac) and its shell's pid; none for an unknown terminal.
    fn reported_folder(&self, id: u64, cx: &App) -> Option<(Option<PathBuf>, Option<u32>)> {
        let session = self.sessions.items().iter().find(|s| s.id == id)?;
        let ssh = session.ssh.is_some();
        let (reported, pid) = session.view.read(cx).working_folder();
        let hostname = self.places.hostname.as_deref();
        let reported = reported
            .filter(|r| ssh || is_this_mac(&r.host, hostname))
            .map(|r| r.path);
        Some((reported, pid))
    }

    /// Whether the followed terminal's folder can be looked up: the sidebar is open, the window
    /// can be seen, and nothing is frozen (the UI screenshots).
    fn right_sidebar_wanted(&self, cx: &App) -> bool {
        let seen = cx.try_global::<WindowVisible>().is_none_or(|w| w.visible);
        !self.right_sidebar.hidden && seen && !self.places.frozen
    }

    /// Follow the focused terminal (a browser or file tab focused: keep the last one), look its
    /// folder up where due (`lookup_due`: at once for a newly followed terminal, else every 2 s)
    /// and bring the Files panel up to date. `render` calls it on every frame, the 2 s refresh
    /// between frames.
    pub(crate) fn refresh_right_sidebar(&mut self, cx: &mut Context<Self>) {
        if let Some(id) = self.focused_terminal()
            && self.right_sidebar.followed.replace(id) != Some(id)
        {
            // Another terminal: its files at once.
            self.right_sidebar.files.refreshed = None;
        }
        if !self.right_sidebar_wanted(cx) {
            return;
        }
        self.look_up_right_sidebar_folder(cx);
        self.refresh_files(cx);
    }

    /// Look the followed terminal's folder up, where due, off the UI thread.
    fn look_up_right_sidebar_folder(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.right_sidebar.followed else {
            return;
        };
        let now = Instant::now();
        if !lookup_due(self.right_sidebar.looked, id, now) {
            return;
        }
        let Some((reported, pid)) = self.reported_folder(id, cx) else {
            return;
        };
        let Some(session) = self.sessions.items().iter().find(|s| s.id == id) else {
            return;
        };
        let (ssh, project) = (session.ssh.is_some(), session.project.clone());
        self.right_sidebar.looked = Some((id, now));
        cx.spawn(async move |this, cx| {
            // The kernel lookup (`proc_pidinfo`) off the UI thread.
            let folder = cx
                .background_executor()
                .spawn(async move {
                    resolve_folder(reported, ssh, || pid.and_then(process_cwd), project)
                })
                .await;
            this.update(cx, |this, cx| {
                // An answer for a terminal no longer followed is dropped.
                let found = Some((id, folder, ssh));
                if this.right_sidebar.followed == Some(id) && this.right_sidebar.found != found {
                    this.right_sidebar.found = found;
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    /// Start the 2 s refresh unless it's running or not wanted (`right_sidebar_wanted`); it
    /// stops once it isn't, so a closed sidebar or a hidden window is never woken. `render`
    /// calls it, so opening the sidebar or showing the window again starts it.
    pub(crate) fn tick_right_sidebar(&mut self, cx: &mut Context<Self>) {
        if self.right_sidebar.ticking || !self.right_sidebar_wanted(cx) {
            return;
        }
        self.right_sidebar.ticking = true;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(REFRESH).await;
                let wanted = this
                    .update(cx, |this, cx| {
                        let wanted = this.right_sidebar_wanted(cx);
                        this.right_sidebar.ticking = wanted;
                        if wanted {
                            this.refresh_right_sidebar(cx);
                        }
                        wanted
                    })
                    .unwrap_or(false);
                if !wanted {
                    break;
                }
            }
        })
        .detach();
    }

    /// The right sidebar and how it sits this frame: laid out beside the pane column, or (while
    /// sliding) laid over it from the right; none when it's closed and still. Ends a slide past
    /// its time; until then every frame asks for the next. Also: how much of it shows while
    /// sliding.
    pub(crate) fn right_sidebar_layout(
        &mut self,
        ui: UiColors,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> (Option<AnyElement>, Option<AnyElement>) {
        self.right_sidebar.slide = self
            .right_sidebar
            .slide
            .filter(|s| s.since.elapsed() < sidebar_toggle::SLIDE);
        let slide = self.right_sidebar.slide.map(|s| s.shown());
        if slide.is_some() {
            window.request_animation_frame();
        }
        if self.right_sidebar.hidden && slide.is_none() {
            return (None, None);
        }
        self.ensure_files_panel(window, cx);
        self.update_file_rows();
        let width = self.right_sidebar_drawn_width(window);
        let node = self.render_right_sidebar(ui, width, window, cx);
        match slide {
            Some(shown) => {
                let over = div()
                    .absolute()
                    .top_0()
                    .right(px(-(1.0 - shown) * width))
                    .h_full()
                    .child(node)
                    .into_any_element();
                (None, Some(over))
            }
            None => (Some(node), None),
        }
    }

    /// How wide the right sidebar is drawn this frame: as chosen, unless the window is too narrow
    /// to leave the panes `PANE_COLUMN_MIN` beside it and the left sidebar (`drawn_width`).
    pub(crate) fn right_sidebar_drawn_width(&self, window: &Window) -> f32 {
        let left = if self.sidebar.hidden {
            0.0
        } else {
            self.sidebar.width
        };
        let window_width = f32::from(window.viewport_size().width);
        drawn_width(self.right_sidebar.width, window_width, left)
    }

    /// The right sidebar, `width` wide: with the left sidebar hidden, the window's top band
    /// (as over the panes, so its header lines up with their tab bars); its panel's name on a
    /// strip as tall as the tab bars (the window can be dragged by it); the Files panel; and its
    /// left edge, which resizes it.
    fn render_right_sidebar(
        &self,
        ui: UiColors,
        width: f32,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let band = self
            .sidebar
            .hidden
            .then(|| self.render_top_band("right-sidebar-top", None, window, cx))
            .flatten();
        let strip = div()
            .id("right-sidebar-band")
            .flex()
            .flex_none()
            .items_center()
            .w_full()
            .h(px(TAB_BAR))
            .px(px(10.0))
            .border_b_1()
            .border_color(hsla(ui.sidebar_border))
            .text_size(px(text::NAME))
            .text_color(hsla(ui.text))
            .child(PANEL);
        let strip = if window.is_fullscreen() {
            strip
        } else {
            self.drag_window_by(strip, cx)
        };
        div()
            .id("right-sidebar")
            .relative()
            .flex()
            .flex_col()
            .flex_none()
            .w(px(width))
            .h_full()
            .bg(hsla(ui.sidebar_background))
            .border_l_1()
            .border_color(hsla(ui.sidebar_border))
            .children(band)
            .child(strip)
            .child(self.render_files_panel(ui, cx))
            .child(self.render_right_resize_edge(ui, cx))
            .into_any_element()
    }

    /// The Files panel's focus and filter box, made once (they need a window).
    fn ensure_files_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.right_sidebar.files.focus.is_none() {
            self.right_sidebar.files.focus = Some(cx.focus_handle());
        }
        if self.right_sidebar.files.input.is_some() {
            return;
        }
        let input = cx.new(|cx| NameInput::new(String::new(), window, cx));
        let events = cx.subscribe_in(&input, window, |this, input, event, window, cx| {
            match event {
                NameInputEvent::Confirm(_) => this.files_open_selected(window, cx),
                // Esc while typing (not the box losing focus): back to the pane.
                NameInputEvent::Cancel => {
                    if input.read(cx).focus_handle(cx).is_focused(window) {
                        this.focus_active(window, cx);
                    }
                }
            }
        });
        let changes = cx.observe(&input, |this, input, cx| {
            let text = input.read(cx).text().to_string();
            this.files_query_changed(text, cx);
        });
        let files = &mut self.right_sidebar.files;
        files.input = Some(input);
        files._subscriptions = vec![events, changes];
    }

    /// The filter box's text changed: the selection stays if it still shows, else the first
    /// matching row.
    fn files_query_changed(&mut self, text: String, cx: &mut Context<Self>) {
        let files = &mut self.right_sidebar.files;
        if text == files.query {
            return;
        }
        files.query = text;
        if let Some(tree) = files.tree.as_mut() {
            let shown = tree
                .selected()
                .is_some_and(|s| tree.rows(&files.query).iter().any(|r| r.rel == s));
            if !shown {
                tree.select_next(&files.query);
            }
            files.reveal = Some(ScrollStrategy::Nearest);
        }
        cx.notify();
    }

    /// This frame's rows, and the selected one scrolled into view where asked (once it shows).
    fn update_file_rows(&mut self) {
        let files = &mut self.right_sidebar.files;
        let Some(tree) = files.tree.as_ref() else {
            files.rows = Rc::default();
            return;
        };
        // Built again only when the tree or the filter changed: not for a terminal's redraw.
        files.rows = files.cache.rows(tree, &files.query);
        let Some(strategy) = files.reveal else {
            return;
        };
        let at = tree.selected().and_then(|s| {
            files
                .rows
                .iter()
                .position(|r| r.more.is_none() && r.rel == s)
        });
        if let Some(at) = at {
            files.scroll.scroll_to_item(at, strategy);
            files.reveal = None;
        } else if !files.rows.is_empty() && tree.wanted().is_empty() {
            files.reveal = None; // it won't show
        }
    }

    /// Whether the followed terminal is an SSH one.
    fn followed_ssh(&self) -> bool {
        let id = self.right_sidebar.followed;
        self.sessions
            .items()
            .iter()
            .any(|s| Some(s.id) == id && s.ssh.is_some())
    }

    /// Bring the Files panel up to date (only while the sidebar can be seen): a new terminal
    /// folder looks its root up (a new root re-roots the tree); otherwise, every 2 s (at once
    /// when asked: shown, another terminal), the open folders are listed again and the marks
    /// read. All of it off the UI thread; an SSH terminal's files aren't looked at.
    fn refresh_files(&mut self, cx: &mut Context<Self>) {
        let folder = self.right_sidebar_folder(cx);
        let files = &mut self.right_sidebar.files;
        // An SSH terminal, or none with a folder: no tree, so no key or click acts on the last
        // one's (its note shows instead).
        let Some((folder, false)) = folder else {
            if files.tree.is_some() || files.folder.is_some() {
                files.forget();
                files.folder = None;
                cx.notify();
            }
            return;
        };
        if files.folder.as_ref() != Some(&folder) {
            files.folder = Some(folder.clone());
            files.rooting = true;
            cx.spawn(async move |this, cx| {
                let asked = folder.clone();
                let (root, in_repo, revealed) = cx
                    .background_executor()
                    .spawn(async move { find_root(&asked) })
                    .await;
                this.update(cx, |this, cx| {
                    let files = &mut this.right_sidebar.files;
                    // The terminal has moved on since: its own lookup is under way.
                    if files.folder.as_ref() != Some(&folder) {
                        return;
                    }
                    files.rooting = false;
                    files.rooted(root, in_repo, revealed);
                    files.refreshed = None;
                    this.refresh_files(cx);
                    cx.notify();
                })
                .ok();
            })
            .detach();
            return;
        }
        let now = Instant::now();
        let due = files
            .refreshed
            .is_none_or(|at| now.saturating_duration_since(at) >= REFRESH);
        if files.rooting || files.refreshing || !due {
            return;
        }
        let Some(tree) = files.tree.as_ref() else {
            return;
        };
        let (root, in_repo, folders) = (
            tree.root().to_path_buf(),
            files.in_repo,
            tree.open_folders(),
        );
        files.refreshing = true;
        files.refreshed = Some(now);
        cx.spawn(async move |this, cx| {
            let at = root.clone();
            let (listings, marks) = cx
                .background_executor()
                .spawn(async move {
                    let git = in_repo.then(find_git).flatten();
                    let listings: Vec<_> = folders
                        .into_iter()
                        .map(|rel| {
                            let listing = list_folder(git.as_deref(), &at, &rel, in_repo);
                            (rel, listing)
                        })
                        .collect();
                    let marks = git
                        .map(|git| git_status::status(&git, &at))
                        .unwrap_or_default();
                    (listings, marks)
                })
                .await;
            this.update(cx, |this, cx| {
                let files = &mut this.right_sidebar.files;
                // Another root's refresh may be under way since (`forget`): not this one's to end.
                let Some(tree) = files.tree.as_mut().filter(|t| t.root() == root) else {
                    return;
                };
                tree.set_marks(marks);
                files.refreshing = false;
                files.listed(&root, listings);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// List folders `rels` of the tree (just opened), off the UI thread; one already being
    /// listed isn't asked for again.
    fn files_list(&mut self, rels: Vec<PathBuf>, cx: &mut Context<Self>) {
        let files = &mut self.right_sidebar.files;
        let Some(tree) = files.tree.as_ref() else {
            return;
        };
        let (root, in_repo) = (tree.root().to_path_buf(), files.in_repo);
        let rels: Vec<PathBuf> = rels
            .into_iter()
            .filter(|rel| files.listing.insert(rel.clone()))
            .collect();
        if rels.is_empty() {
            return;
        }
        cx.spawn(async move |this, cx| {
            let at = root.clone();
            let listings: Vec<_> = cx
                .background_executor()
                .spawn(async move {
                    let git = in_repo.then(find_git).flatten();
                    rels.into_iter()
                        .map(|rel| {
                            let listing = list_folder(git.as_deref(), &at, &rel, in_repo);
                            (rel, listing)
                        })
                        .collect()
                })
                .await;
            this.update(cx, |this, cx| {
                this.right_sidebar.files.listed(&root, listings);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Do what a key or click asked of the tree: list a folder just opened, or open a file in a
    /// tab in the focused pane (selected if that pane already has it).
    fn files_want(&mut self, want: Option<Want>, window: &mut Window, cx: &mut Context<Self>) {
        match want {
            // With it, the folders inside it remembered open and not listed yet.
            Some(Want::List(rel)) => {
                let mut rels = vec![rel];
                if let Some(tree) = &self.right_sidebar.files.tree {
                    rels.extend(tree.wanted());
                }
                self.files_list(rels, cx)
            }
            Some(Want::Open(rel)) => {
                let Some(root) = self.right_sidebar.files.tree.as_ref().map(|t| t.root()) else {
                    return;
                };
                let path = root.join(rel);
                self.open_file_tab(path, window, cx);
            }
            None => {}
        }
        self.right_sidebar.files.reveal = Some(ScrollStrategy::Nearest);
        cx.notify();
    }

    /// A tree key: move the selection, open or close, or open a file.
    fn files_key(
        &mut self,
        key: fn(&mut FileTree, &str) -> Option<Want>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let files = &mut self.right_sidebar.files;
        let Some(tree) = files.tree.as_mut() else {
            return;
        };
        let want = key(tree, &files.query);
        self.files_want(want, window, cx);
    }

    /// Return (in the tree or the filter box): open the selected file, or open or close the
    /// selected folder.
    fn files_open_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.files_key(|tree, query| tree.toggle_selected(query), window, cx);
    }

    /// A click on a row: it's selected; a folder opens or closes, a file opens in a tab.
    fn files_click(
        &mut self,
        rel: &Path,
        is_dir: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(tree) = self.right_sidebar.files.tree.as_mut() else {
            return;
        };
        tree.select(rel);
        let want = if is_dir {
            tree.toggle(rel)
        } else {
            Some(Want::Open(rel.to_path_buf()))
        };
        self.files_want(want, window, cx);
    }

    /// The Files panel under the header: the filter box, then the tree (or what stands in for
    /// it: an SSH terminal's note, no folder, no match). It has its own focus, and the tree's
    /// keys (key context `FileTree`, the box included: ↑ ↓ move through the rows while typing).
    fn render_files_panel(&self, ui: UiColors, cx: &mut Context<Self>) -> AnyElement {
        let files = &self.right_sidebar.files;
        let note = |text: &'static str| {
            div()
                .px(px(12.0))
                .py(px(8.0))
                .text_size(px(text::NAME))
                .text_color(hsla(ui.text_dim))
                .child(text)
                .into_any_element()
        };
        let ssh = self.followed_ssh();
        let body = if ssh {
            Some(note(SSH_NOTE))
        } else if files.tree.is_none() {
            // Nothing to look at; else its tree is on its way.
            self.right_sidebar_folder(cx)
                .is_none()
                .then(|| note(NO_FOLDER))
        } else if files.rows.is_empty() && !files.query.trim().is_empty() {
            Some(note(NO_MATCH))
        } else {
            let list = uniform_list(
                "file-rows",
                files.rows.len(),
                cx.processor(|this, range: std::ops::Range<usize>, _window, cx| {
                    let theme = cx.global::<Appearance>().theme;
                    let files = &this.right_sidebar.files;
                    let selected = files.tree.as_ref().and_then(|t| t.selected());
                    range
                        .filter_map(|i| files.rows.get(i).map(|row| (i, row)))
                        .map(|(i, row)| file_row(i, row, selected, &theme, cx))
                        .collect::<Vec<_>>()
                }),
            )
            .track_scroll(&files.scroll)
            .size_full();
            Some(list.into_any_element())
        };
        let filter = files
            .input
            .clone()
            .filter(|_| !ssh && files.tree.is_some())
            .map(|input| {
                let empty = input.read(cx).text().is_empty();
                let focus_box = input.clone();
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .h(px(FILTER_BAND))
                    .px(px(6.0))
                    // The box keeps its clicks from reaching whatever holds it, so it takes the
                    // keyboard here, before they arrive.
                    .capture_any_mouse_down(move |_, window, cx| {
                        let focus = focus_box.read(cx).focus_handle(cx);
                        focus.focus(window, cx);
                    })
                    .child(div().flex_1().child(overlay_panel::filter_box(
                        &ui,
                        &input,
                        empty,
                        FILTER_PLACEHOLDER,
                    )))
            });
        let panel = div()
            .id("files-panel")
            .key_context("FileTree")
            .flex()
            .flex_col()
            .flex_1()
            .min_h(px(0.0))
            .w_full()
            .on_action(cx.listener(|this, _: &SelectPrev, window, cx| {
                this.files_key(
                    |tree, query| {
                        tree.select_prev(query);
                        None
                    },
                    window,
                    cx,
                )
            }))
            .on_action(cx.listener(|this, _: &SelectNext, window, cx| {
                this.files_key(
                    |tree, query| {
                        tree.select_next(query);
                        None
                    },
                    window,
                    cx,
                )
            }))
            .on_action(cx.listener(|this, _: &ExpandSelected, window, cx| {
                this.files_key(|tree, query| tree.expand_selected(query), window, cx)
            }))
            .on_action(cx.listener(|this, _: &CollapseSelected, window, cx| {
                this.files_key(
                    |tree, query| {
                        tree.collapse_selected(query);
                        None
                    },
                    window,
                    cx,
                )
            }))
            .on_action(cx.listener(|this, _: &OpenSelected, window, cx| {
                this.files_open_selected(window, cx)
            }))
            .on_action(
                cx.listener(|this, _: &BackToPane, window, cx| this.focus_active(window, cx)),
            )
            .children(filter)
            .child(div().flex_1().min_h(px(0.0)).w_full().children(body));
        match &files.focus {
            Some(focus) => panel.track_focus(focus).into_any_element(),
            None => panel.into_any_element(),
        }
    }

    /// The strip along the right sidebar's left edge: drag it to resize the sidebar. A 2 px
    /// accent line shows on hover and while dragging; while dragging, the whole window follows
    /// the mouse until release (as the left sidebar's edge does).
    fn render_right_resize_edge(&self, ui: UiColors, cx: &mut Context<Self>) -> AnyElement {
        let resizing = self.right_sidebar.resizing;
        let this = cx.entity().downgrade();
        let follow = canvas(
            |_, _, _| (),
            move |_, _, window, _| {
                if !resizing {
                    return;
                }
                // Window-wide, not tied to the strip, so a fast move off it keeps resizing.
                let moved = this.clone();
                window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                    if phase != DispatchPhase::Capture {
                        return;
                    }
                    let window_width = f32::from(window.viewport_size().width);
                    moved
                        .update(cx, |this, cx| {
                            // Released where no mouse-up reached us (e.g. outside the window).
                            if event.dragging() {
                                let x = f32::from(event.position.x);
                                this.resize_right_sidebar(window_width, x, cx)
                            } else {
                                this.end_right_resize(cx)
                            }
                        })
                        .ok();
                });
                let released = this.clone();
                window.on_mouse_event(move |event: &MouseUpEvent, phase, _, cx| {
                    if phase == DispatchPhase::Capture && event.button == MouseButton::Left {
                        released
                            .update(cx, |this, cx| this.end_right_resize(cx))
                            .ok();
                    }
                });
            },
        )
        .absolute()
        .size_0();
        div()
            .id("right-sidebar-resize")
            .group("right-sidebar-edge")
            .absolute()
            .top_0()
            .bottom_0()
            .left(px(-1.0))
            .w(px(RESIZE_STRIP))
            .cursor_col_resize()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _: &MouseDownEvent, _, cx| {
                    cx.stop_propagation(); // not a click in the sidebar
                    this.right_sidebar.resizing = true;
                    cx.notify();
                }),
            )
            .child(
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left(px(1.0))
                    .w(px(DIVIDER_ACTIVE))
                    .bg(hsla(ui.accent))
                    .when(!resizing, |line| {
                        line.invisible()
                            .group_hover("right-sidebar-edge", |style| style.visible())
                    }),
            )
            .child(follow)
            .into_any_element()
    }

    /// The right sidebar's left edge follows the mouse (it ends at the window's right).
    pub(crate) fn resize_right_sidebar(
        &mut self,
        window_width: f32,
        x: f32,
        cx: &mut Context<Self>,
    ) {
        let width = width_for_edge(window_width, x);
        if width != self.right_sidebar.width {
            self.right_sidebar.width = width;
            cx.notify();
        }
    }

    fn end_right_resize(&mut self, cx: &mut Context<Self>) {
        if std::mem::take(&mut self.right_sidebar.resizing) {
            self.save(); // once per drag, not on every move
            cx.notify();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::restore::decode;
    use std::path::{Path, PathBuf};

    #[test]
    fn a_file_without_the_right_sidebar_opens_it_closed_and_300_wide() {
        let saved = decode(r#"{"version":2,"sessions":[{}]}"#).expect("loads");
        let mut state = RightSidebarState::new();
        state.hidden = false;
        state.width = 410.0;
        state.load(&saved);
        assert!(state.hidden);
        assert_eq!(state.width, 300.0);
        // As saved: shown, and a width out of range comes back in it.
        let saved =
            decode(r#"{"version":2,"right_sidebar_hidden":false,"right_sidebar_width":1200.0}"#)
                .expect("loads");
        state.load(&saved);
        assert!(!state.hidden);
        assert_eq!(state.width, 560.0);
    }

    #[test]
    fn its_width_stays_between_220_and_560() {
        assert_eq!(clamp(Some(100.0)), 220.0);
        assert_eq!(clamp(Some(900.0)), 560.0);
        assert_eq!(clamp(Some(333.0)), 333.0);
        assert_eq!(clamp(None), 300.0);
        assert_eq!(clamp(Some(f32::NAN)), 300.0);
    }

    #[test]
    fn its_left_edge_follows_the_pointer() {
        // In a 1280 pt window, the edge at x = 980 makes it 300 wide; 80 left, 80 wider.
        assert_eq!(width_for_edge(1280.0, 980.0), 300.0);
        assert_eq!(width_for_edge(1280.0, 900.0), 380.0);
        // Never past its bounds.
        assert_eq!(width_for_edge(1280.0, 1270.0), 220.0);
        assert_eq!(width_for_edge(1280.0, 10.0), 560.0);
    }

    #[test]
    fn a_local_terminal_folder_is_its_report_else_the_kernels_else_where_it_started() {
        let report = Some(PathBuf::from("/a/reported"));
        let cwd = || Some(PathBuf::from("/a/cwd"));
        let project = Some(PathBuf::from("/a/project"));
        let got = resolve_folder(report.clone(), false, cwd, project.clone());
        assert_eq!(got.as_deref(), Some(Path::new("/a/reported")));
        let got = resolve_folder(None, false, cwd, project.clone());
        assert_eq!(got.as_deref(), Some(Path::new("/a/cwd")));
        let got = resolve_folder(None, false, || None, project.clone());
        assert_eq!(got.as_deref(), Some(Path::new("/a/project")));
        assert_eq!(resolve_folder(None, false, || None, None), None);
    }

    #[test]
    fn an_ssh_terminal_folder_is_only_what_it_reported() {
        // The kernel's answer and the start folder are this Mac's, not the far machine's.
        let asked = std::cell::Cell::new(false);
        let cwd = || {
            asked.set(true);
            Some(PathBuf::from("/local/cwd"))
        };
        let project = Some(PathBuf::from("/local/project"));
        assert_eq!(resolve_folder(None, true, cwd, project.clone()), None);
        assert!(!asked.get(), "no kernel lookup for an ssh terminal");
        let got = resolve_folder(Some(PathBuf::from("/srv/app")), true, || None, project);
        assert_eq!(got.as_deref(), Some(Path::new("/srv/app")));
    }

    #[test]
    fn a_narrow_window_keeps_the_panes_200_wide() {
        // Room for it: drawn as wide as saved.
        assert_eq!(drawn_width(300.0, 1280.0, 240.0), 300.0);
        // 700 wide with the left sidebar's 240: the panes keep 200, so 260 is drawn.
        assert_eq!(drawn_width(300.0, 700.0, 240.0), 260.0);
        // Without the left sidebar: 500 wide leaves it 300.
        assert_eq!(drawn_width(300.0, 500.0, 0.0), 300.0);
        assert_eq!(drawn_width(560.0, 500.0, 0.0), 300.0);
        // Too narrow for any of it: none drawn, never less.
        assert_eq!(drawn_width(300.0, 300.0, 240.0), 0.0);
        assert_eq!(PANE_COLUMN_MIN, 200.0);
    }

    #[test]
    fn it_refreshes_on_a_new_terminal_or_every_two_seconds() {
        use std::time::{Duration, Instant};
        let now = Instant::now();
        // Never looked up, or for another terminal: now.
        assert!(lookup_due(None, 7, now));
        assert!(lookup_due(Some((6, now)), 7, now));
        // The same terminal: once 2 s have passed.
        assert!(!lookup_due(
            Some((7, now)),
            7,
            now + Duration::from_millis(1999)
        ));
        assert!(lookup_due(Some((7, now)), 7, now + REFRESH));
    }
}
