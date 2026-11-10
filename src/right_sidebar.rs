//! The right sidebar (M11g): one per window, on the right of the panes, showing the focused
//! terminal's files.
//!
//! It opens and closes with ⌥⌘\, View → Toggle Right Sidebar, or the icon at the far right of
//! every pane's tab bar (`tab_bar::BarButton::RightSidebar`, lit while it's open), sliding like
//! the left sidebar (`sidebar_toggle::Slide`, not with Reduce motion on). It is a raised card
//! (M11i, `panes::card`) 6 pt from the window's top, right and bottom edges and from the pane
//! cards; the gap at its left resizes it (220–560 pt, 300 by default: the card's own width).
//! Open state and width live in `sessions.json`. A narrow window draws it narrower
//! (`drawn_width`) so the pane card keeps 200 pt.
//!
//! It follows the focused terminal (with a browser or file tab focused, the last terminal it
//! followed; none yet, as after a relaunch, the focused workspace's first terminal): that
//! terminal's folder, found like a sidebar row's (`git_branch::refresh_places`: its OSC 7
//! report, else the kernel's answer, else the folder it started in; an SSH terminal's only as
//! reported). The lookup runs off the UI thread, on a focus change and every 2 s while the
//! sidebar can be seen (`tick_right_sidebar`, which stops once it can't).
//!
//! M11h: the card's top strip, level with the pane tab bars and drawn like them, holds the
//! panel tabs, Files · Changes · Activity (`Panel`; the last one shown is saved). Only the shown
//! panel does any work: Files lists and marks, Changes (`changes_panel`) asks git, Activity
//! asks nothing; switching brings the new one up to date at once, and leaving Files while it has
//! the keyboard gives the keyboard to the focused tab (`show_right_panel`).
//!
//! The Files panel: the tree of that folder's
//! repo (`file_tree::FileTree`), rooted at the repo's top folder with the terminal's folder
//! open and selected, else at the folder itself. Folders are listed one at a time as they open,
//! each with one `git check-ignore` batch; marks come from `git status`. All of it runs on the
//! background executor, again when the sidebar is shown, when the terminal or its folder
//! changes, and every 2 s while it can be seen (`back_off`: longer, up to 30 s, while git status
//! fails or hits its cap; a failed git answer keeps the marks and ignored files shown). Above it
//! (M11i) the root header: the root's name (a click folds the whole tree away, remembered per
//! root for the run), collapse all, and the root's path, its start cut to fit; then a filter
//! box; ↑ ↓ → ← Return and Esc in it (key context `FileTree`); a click on a file opens it in a
//! file tab. Rows (M11i) have type icons (`file_kind`, tinted by `kind_color`) and indent
//! guides (`guides`). The panel has its own focus: closing the sidebar while it has the
//! keyboard gives the keyboard to the focused tab. An SSH terminal's files aren't shown.
//!
//! `Panel`, `clamp`, `drawn_width`, `width_for_edge`, `resolve_folder`, `lookup_due`, `back_off`,
//! `file_kind`, `kind_color`, `guides` and `root_path_line` are pure and tested, as is the tree.

#[cfg(feature = "ui-shots")]
use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

use alacritty_terminal::vte::ansi::Rgb;
use gpui::{
    AnyElement, AnyView, App, ClickEvent, Context, DispatchPhase, Entity, FocusHandle, Focusable,
    FontWeight, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ScrollStrategy,
    SharedString, Subscription, TextStyle, TruncateFrom, UniformListScrollHandle, Window, actions,
    canvas, div, prelude::*, px, uniform_list,
};

#[cfg(feature = "ui-shots")]
use gpui::{Bounds, Pixels};

use crate::appearance::Appearance;
use crate::changes_panel::ChangesPanel;
use crate::file_tree::{self, FileTree, Row, RowCache, TreeMemory, Want};
use crate::git_branch::{find_git, is_this_mac, process_cwd};
use crate::git_status::{self, DirEntry, Mark};
use crate::name_input::{NameInput, NameInputEvent};
use crate::overlay_panel;
use crate::panes::{self, PaneColors};
use crate::reduce_motion::WindowVisible;
use crate::restore::SavedState;
use crate::sidebar_toggle::{self, Slide, Tip};
use crate::tab_bar::{self, Glyph, glyph};
use crate::terminal_view::hsla;
use crate::theme::{Theme, UiColors, readable};
use crate::ui_tokens::size::{
    CARD_GAP, CARD_RADIUS, DIVIDER_ACTIVE, FOCUS_RING, RIGHT_SIDEBAR_DEFAULT_WIDTH,
    RIGHT_SIDEBAR_MAX_WIDTH, RIGHT_SIDEBAR_MIN_WIDTH, TAB_BAR, TAB_TOP_GAP, TOP_BAND,
};
use crate::ui_tokens::{MARK_MIN, text};
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
/// and the shown panel (Files or Changes) refreshed.
pub const REFRESH: Duration = Duration::from_secs(2);
/// The panel tabs in the card's strip (M11h): the first's room from the strip's left, the
/// room between them, each one's side padding; their top corners are the pane tabs'.
const PANEL_TABS_LEFT: f32 = crate::ui_tokens::size::TABS_LEFT;
const PANEL_TAB_GAP: f32 = 2.0;
const PANEL_TAB_PAD: f32 = 10.0;
/// A tree row's height.
pub(crate) const ROW: f32 = 22.0;
/// The band holding the filter box, under the header.
pub(crate) const FILTER_BAND: f32 = 34.0;
/// How far each level of the tree is indented (M11i: 14, was 12).
const INDENT: f32 = 14.0;
/// A row's room left of its top level's chevron column.
const ROW_LEFT: f32 = 8.0;
/// The chevron column, then the icon and the gap after it (M11i).
const CHEVRON: f32 = 12.0;
const ICON: f32 = 12.0;
const ICON_GAP: f32 = 6.0;
/// The root header above the filter (M11i): its name line and its path line.
pub(crate) const ROOT_HEADER: f32 = 40.0;
pub(crate) const ROOT_LINE: f32 = 20.0;
pub(crate) const PATH_LINE: f32 = 14.0;
pub(crate) const ROOT_PAD_LEFT: f32 = 10.0;
pub(crate) const ROOT_PAD_RIGHT: f32 = 6.0;
/// Between the root's chevron and its name.
const ROOT_CHEVRON_GAP: f32 = 4.0;
const COLLAPSE_ALL_TIP: &str = "Collapse all folders";
const FILTER_PLACEHOLDER: &str = "Filter files";
const SSH_NOTE: &str = "Files aren't shown for SSH terminals.";
pub(crate) const NO_FOLDER: &str = "No terminal folder to show.";
const NO_MATCH: &str = "No file matches.";

/// A panel the right sidebar shows (M11h), chosen with the tabs in its strip.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Panel {
    #[default]
    Files,
    Changes,
    Activity,
}

impl Panel {
    /// Every panel, in the strip's order.
    pub const ALL: [Panel; 3] = [Panel::Files, Panel::Changes, Panel::Activity];

    /// Its tab's words.
    pub fn label(self) -> &'static str {
        match self {
            Panel::Files => "Files",
            Panel::Changes => "Changes",
            Panel::Activity => "Activity",
        }
    }

    /// How the sessions file names it (`right_sidebar_panel`).
    pub fn key(self) -> &'static str {
        match self {
            Panel::Files => "files",
            Panel::Changes => "changes",
            Panel::Activity => "activity",
        }
    }

    /// The panel saved as `key`; missing or unknown: Files.
    pub fn from_key(key: Option<&str>) -> Panel {
        Panel::ALL
            .into_iter()
            .find(|p| Some(p.key()) == key)
            .unwrap_or_default()
    }
}

/// A mark's colour on the Files tree: the theme's ANSI blue (M), green (A) or red (D), pushed
/// to read as status marks do (`on_rows`).
// (The tree draws from `RowInks`; the tests and the screenshots read it one colour at a time.)
#[cfg(any(test, feature = "ui-shots"))]
pub(crate) fn mark_color(mark: Mark, theme: &Theme) -> Rgb {
    mark_ink(mark, theme, &theme.ui())
}

/// `mark_color` with the theme's chrome colours (`ui`) already worked out.
pub(crate) fn mark_ink(mark: Mark, theme: &Theme, ui: &UiColors) -> Rgb {
    let ansi = match mark {
        Mark::Modified => theme.palette[4],
        Mark::Added => theme.palette[2],
        Mark::Deleted => theme.palette[1],
    };
    on_rows(ansi, ui)
}

/// `colour` pushed (hue kept) to `MARK_MIN` on each fill a Files row has (M11i: the card, and
/// `card_hover` / `card_selected` under a hovered or selected row), so a git mark or a type
/// tint reads on all of them.
fn on_rows(colour: Rgb, ui: &UiColors) -> Rgb {
    readable(
        colour,
        &[ui.card, ui.card_hover, ui.card_selected],
        MARK_MIN,
    )
}

/// What a file's icon is tinted by (M11i): one of the theme's ANSI colours 1–6, or `Plain`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileKind {
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    Plain,
}

impl FileKind {
    /// Every kind, in order (`RowInks` indexes its tints by it).
    pub const ALL: [FileKind; 7] = [
        FileKind::Red,
        FileKind::Green,
        FileKind::Yellow,
        FileKind::Blue,
        FileKind::Magenta,
        FileKind::Cyan,
        FileKind::Plain,
    ];
}

/// A file's kind from its extension, in any case (note 60 §4's table). Dotfiles, names
/// without an extension and anything not in the table are `Plain`; only the last extension
/// counts (`archive.tar.gz` is a `gz`).
pub fn file_kind(name: &str) -> FileKind {
    use FileKind::*;
    if name.starts_with('.') {
        return Plain;
    }
    let Some((_, ext)) = name.rsplit_once('.') else {
        return Plain;
    };
    match ext.to_ascii_lowercase().as_str() {
        "rs" | "rb" | "swift" | "scala" | "html" | "htm" => Red,
        "sh" | "bash" | "zsh" | "fish" | "vue" | "lua" => Green,
        "js" | "jsx" | "mjs" | "cjs" | "py" => Yellow,
        "ts" | "tsx" | "mts" | "cts" | "c" | "h" | "cpp" | "hpp" | "cc" | "css" | "scss"
        | "java" | "kt" => Blue,
        "png" | "jpg" | "jpeg" | "gif" | "svg" | "webp" | "ico" | "pdf" => Magenta,
        "go" | "json" | "jsonc" | "yaml" | "yml" | "toml" | "xml" | "sql" => Cyan,
        _ => Plain,
    }
}

/// A file icon's tint: the theme's ANSI colour for its kind, pushed (hue kept) to `MARK_MIN`
/// on the card and the row fills (`on_rows`); `Plain` is the dim text colour.
// (The tree draws from `RowInks`; the tests and the screenshots read it one colour at a time.)
#[cfg(any(test, feature = "ui-shots"))]
pub fn kind_color(kind: FileKind, theme: &Theme) -> Rgb {
    kind_ink(kind, theme, &theme.ui())
}

/// `kind_color` with the theme's chrome colours (`ui`) already worked out.
pub(crate) fn kind_ink(kind: FileKind, theme: &Theme, ui: &UiColors) -> Rgb {
    let ansi = match kind {
        FileKind::Red => 1,
        FileKind::Green => 2,
        FileKind::Yellow => 3,
        FileKind::Blue => 4,
        FileKind::Magenta => 5,
        FileKind::Cyan => 6,
        FileKind::Plain => return ui.text_dim,
    };
    on_rows(theme.palette[ansi], ui)
}

/// Where a row `depth` levels down draws its indent guides: the x (from the row's left) of
/// each level above it, at the centre of that level's chevron column.
pub fn guides(depth: usize) -> Vec<f32> {
    (0..depth)
        .map(|d| ROW_LEFT + d as f32 * INDENT + CHEVRON / 2.0)
        .collect()
}

/// The root's path as the header's second line shows it: home-relative; if it doesn't `fit`,
/// its start cut at a folder boundary (`…/proj/fuxx-cli`, as many whole folders as fit); when
/// not even `…/<name>` fits, cut by characters, never shorter than the folder's own name.
pub fn root_path_line(root: &Path, home: Option<&Path>, fits: impl Fn(&str) -> bool) -> String {
    let path = crate::sidebar::home_relative(root, home);
    if fits(&path) {
        return path;
    }
    let length = path.chars().count();
    // From the most folders kept to the fewest; only cuts shorter than the path (not `~`'s or
    // the leading `/`'s).
    let cuts = path
        .match_indices('/')
        .map(|(at, _)| format!("\u{2026}{}", &path[at..]))
        .filter(|cut| cut.chars().count() < length);
    let mut last = None;
    for cut in cuts {
        if fits(&cut) {
            return cut;
        }
        last = Some(cut);
    }
    match last {
        // Not even `…/<name>`: by characters (`fit_place`, its own name kept).
        Some(_) => crate::sidebar::fit_place(None, Some(&path), fits),
        None => path,
    }
}

/// The colours the tree's rows are drawn in, worked out once per frame (not per row): the
/// chrome's, and each type tint and git mark's (`kind_color`, `mark_color`).
struct RowInks {
    ui: UiColors,
    kinds: [Rgb; 7],
    marks: [Rgb; 3],
}

impl RowInks {
    /// Worked out from one `theme.ui()` (not one per tint and mark).
    fn of(theme: &Theme) -> Self {
        let ui = theme.ui();
        Self {
            kinds: FileKind::ALL.map(|k| kind_ink(k, theme, &ui)),
            marks: [Mark::Modified, Mark::Added, Mark::Deleted].map(|m| mark_ink(m, theme, &ui)),
            ui,
        }
    }

    fn kind(&self, kind: FileKind) -> Rgb {
        self.kinds[kind as usize]
    }

    fn mark(&self, mark: Mark) -> Rgb {
        let at = match mark {
            Mark::Modified => 0,
            Mark::Added => 1,
            Mark::Deleted => 2,
        };
        self.marks[at]
    }
}

fn mark_letter(mark: Mark) -> &'static str {
    match mark {
        Mark::Modified => "M",
        Mark::Added => "A",
        Mark::Deleted => "D",
    }
}

/// Tree row `i`: indented by its depth (an indent guide for each level above it), a folder's
/// ▸ / ▾, its icon (a folder, open while expanded, in the dim text colour; a file's page tinted
/// by its kind), its name and its mark (both in the mark's colour), filled while `selected`; a
/// click on it is `files_click`. A `… N more` row is only its text (and its guides).
fn file_row(
    i: usize,
    row: &Row,
    selected: Option<&Path>,
    inks: &RowInks,
    cx: &mut Context<Workspace>,
) -> AnyElement {
    let ui = inks.ui;
    let left = ROW_LEFT + row.depth as f32 * INDENT;
    // 1 pt lines the row's full height, so they join from row to row.
    let lines = guides(row.depth).into_iter().map(|x| {
        div()
            .absolute()
            .top_0()
            .left(px(x - 0.5))
            .w(px(1.0))
            .h_full()
            .bg(hsla(ui.guide))
    });
    let base = div()
        .id(("file-row", i))
        .relative()
        .flex()
        .items_center()
        .h(px(ROW))
        .w_full()
        .pl(px(left))
        .pr(px(10.0))
        .text_size(px(text::NAME))
        .children(lines);
    if let Some(more) = row.more {
        return base
            // Under the names: past the chevron column, the icon and its gap.
            .pl(px(left + CHEVRON + ICON + ICON_GAP))
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
    let ink = row.mark.map_or(ui.text, |m| inks.mark(m));
    let chevron = match (row.is_dir, row.expanded) {
        (true, true) => "\u{25be}",
        (true, false) => "\u{25b8}",
        (false, _) => "",
    };
    let icon = match (row.is_dir, row.expanded) {
        (true, true) => glyph(Glyph::FolderOpen, ui.text_dim),
        (true, false) => glyph(Glyph::Folder, ui.text_dim),
        (false, _) => glyph(Glyph::File, inks.kind(file_kind(&name))),
    };
    let (rel, is_dir) = (row.rel.clone(), row.is_dir);
    let hover = ui.card_hover;
    base.when(chosen, |r| r.bg(hsla(ui.card_selected)))
        .when(!chosen, |r| r.hover(move |s| s.bg(hsla(hover))))
        .cursor_pointer()
        .child(
            div()
                .flex_none()
                .w(px(CHEVRON))
                .text_size(px(text::META))
                .text_color(hsla(ui.text_dim))
                .child(chevron),
        )
        .child(
            div()
                .flex_none()
                .size(px(ICON))
                .mr(px(ICON_GAP))
                .child(icon),
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
        .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
            // A folder's double-click: the terminal goes there (its first click opened it).
            if is_dir && event.click_count() == 2 {
                this.files_cd(&rel, cx);
            } else {
                this.files_click(&rel, is_dir, window, cx)
            }
        }))
        .into_any_element()
}

/// A folder's listing as the background job hands it to the tree.
struct Listed {
    /// Shown entries, sorted, at most `file_tree::CAP`.
    entries: Vec<DirEntry>,
    more: usize,
    /// What git ignores in it (paths from the root); none: git didn't answer (the folder's last
    /// answer stays, and filtered these entries).
    ignored: Option<HashSet<PathBuf>>,
    /// Its folders holding a `.git`: nested repos and submodules (paths from the root).
    nested: HashSet<PathBuf>,
}

/// Folder `rel`'s listing (from the `root`) as the tree shows it, read off the UI thread: its
/// entries with names precomposed (as git prints them) and sorted; in a repo, what git ignores
/// in it (one batch) and which of its folders are nested repos; then `file_tree::shown_entries`
/// (the ignored and hidden left out before the cap). When git doesn't answer (slow or failing),
/// the folder's last answer (`last`) filters it, so `target/` doesn't show until the next one.
fn list_folder(
    git: Option<&Path>,
    root: &Path,
    rel: &Path,
    in_repo: bool,
    last: Option<HashSet<PathBuf>>,
) -> Listed {
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
        None => Some(HashSet::new()),
    };
    let filter = ignored.as_ref().or(last.as_ref());
    let none = HashSet::new();
    let (entries, more) = file_tree::shown_entries(rel, entries, filter.unwrap_or(&none), in_repo);
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
/// whether it's a repo, and the folder from the root (to open and select). None when git gave
/// no answer (`root_from`). No git at all: not a repo.
pub(crate) fn find_root(folder: &Path) -> Option<(PathBuf, bool, Option<PathBuf>)> {
    let real = std::fs::canonicalize(folder).unwrap_or_else(|_| folder.to_path_buf());
    let top = match find_git() {
        Some(git) => git_status::repo_root_at(&git, &real),
        None => Some(None),
    };
    root_from(&real, top)
}

/// What a root lookup found for a folder whose real path is `real`, from git's answer
/// (`git_status::repo_root_at`): a repo's top folder (by its real path), or "not a repo" (the
/// folder is the root); with whether it's a repo and the folder from the root. None: no answer
/// (git failed, or took longer than the cap), so the folder stays un-rooted and is asked again
/// later (`needs_root`), not taken for "not a repo" until it moves.
pub(crate) fn root_from(
    real: &Path,
    top: Option<Option<PathBuf>>,
) -> Option<(PathBuf, bool, Option<PathBuf>)> {
    let top = top?;
    let in_repo = top.is_some();
    let root = top
        .map(|t| std::fs::canonicalize(&t).unwrap_or(t))
        .unwrap_or_else(|| real.to_path_buf());
    let revealed = real.strip_prefix(&root).ok().map(Path::to_path_buf);
    Some((root, in_repo, revealed))
}

/// Whether a refresh (or a root lookup asked again) that last started at `last` (none: never)
/// is due `now`, `wait` after it.
pub(crate) fn due(last: Option<Instant>, wait: Duration, now: Instant) -> bool {
    last.is_none_or(|at| now.saturating_duration_since(at) >= wait)
}

/// The Files panel: its tree and what it was made from, its filter box, focus and scroll, and
/// the background work under way.
pub(crate) struct FilesPanel {
    pub(crate) tree: Option<FileTree>,
    /// The tree's root is a repo's.
    in_repo: bool,
    /// The terminal folder the tree was last made for.
    folder: Option<PathBuf>,
    /// Each root's open folders and whether it was folded away, for the run.
    memory: HashMap<PathBuf, TreeMemory>,
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
    /// How long after it the next one starts: `REFRESH`, longer while git status fails or is
    /// slow (`back_off`).
    wait: Duration,
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
            wait: REFRESH,
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
    /// keeps its tree as it is; another remembers this one's open folders (and whether it was
    /// folded away) and starts afresh (as that root was left, if it was shown before). Work
    /// under way for the old root no longer holds the new one up (its results are dropped).
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

    /// No tree (an SSH terminal's, or no folder): this one's memory is kept, and nothing under
    /// way for it holds a new one up.
    fn forget(&mut self) {
        if let Some(old) = self.tree.take() {
            self.memory
                .insert(old.root().to_path_buf(), old.expanded_set());
        }
        self.ignored.clear();
        self.listing.clear();
        self.refreshing = false;
        self.wait = REFRESH;
    }

    /// Whether `folder`'s root is looked up `now`: a folder other than the last one at once;
    /// the same one again only when its lookup had no answer (no tree, nothing under way) and
    /// its wait is over.
    fn needs_root(&self, folder: &Path, now: Instant) -> bool {
        self.folder.as_deref() != Some(folder)
            || (self.tree.is_none() && !self.rooting && due(self.refreshed, self.wait, now))
    }

    /// The root lookup had no answer at `now`: un-rooted (no tree, not the last folder's either),
    /// asked again after a longer wait (`back_off`).
    fn unanswered(&mut self, now: Instant) {
        let wait = back_off(self.wait, false);
        self.forget();
        self.rooting = false;
        self.refreshed = Some(now);
        self.wait = wait;
    }

    /// Folders' listings for the tree at `root` (dropped if the tree has moved on). A folder
    /// git didn't answer for keeps what it ignored last.
    fn listed(&mut self, root: &Path, listings: Vec<(PathBuf, Listed)>) {
        let Some(tree) = self.tree.as_mut().filter(|t| t.root() == root) else {
            return;
        };
        for (rel, listed) in listings {
            self.listing.remove(&rel);
            tree.relist(rel.clone(), listed.entries, listed.more);
            tree.set_nested(&rel, listed.nested);
            if let Some(ignored) = listed.ignored {
                self.ignored.insert(rel, ignored);
            }
        }
        // Once for them all.
        tree.set_ignored(self.ignored.values().flatten().cloned().collect());
    }

    /// A key or click on the tree (`key`, with the filter's text): what it asked for, and the
    /// folders to list off the UI thread. A folder the tree asks for comes with the folders
    /// under it not listed yet; one opened from its kept listing shows that at once (the tree
    /// asks for nothing) and is listed again all the same, so what changed in it while it was
    /// closed shows without waiting for the next refresh (2 s, or up to 30 s backed off).
    fn act(
        &mut self,
        key: impl FnOnce(&mut FileTree, &str) -> Option<Want>,
    ) -> (Option<Want>, Vec<PathBuf>) {
        let Some(tree) = self.tree.as_mut() else {
            return (None, Vec::new());
        };
        let before = tree.expanded_set().expanded;
        let want = key(tree, &self.query);
        let mut list = Vec::new();
        if let Some(Want::List(rel)) = &want {
            list.push(rel.clone());
            list.extend(tree.wanted().into_iter().filter(|f| f != rel));
        }
        let mut opened: Vec<PathBuf> = tree
            .expanded_set()
            .expanded
            .into_iter()
            .filter(|f| !before.contains(f) && !list.contains(f))
            .collect();
        opened.sort();
        list.extend(opened);
        (want, list)
    }

    /// What git last said it ignores in each of `folders`, for listing them again.
    fn last_ignored(&self, folders: &[PathBuf]) -> HashMap<PathBuf, HashSet<PathBuf>> {
        folders
            .iter()
            .filter_map(|rel| Some((rel.clone(), self.ignored.get(rel)?.clone())))
            .collect()
    }

    /// A refresh's answer for the tree at `root`: the listings, and git status's marks (none:
    /// git failed or was slow, so the marks shown stay and the next refresh waits longer,
    /// `back_off`). False, and nothing done, when the tree has moved on to another root.
    fn apply_refresh(
        &mut self,
        root: &Path,
        listings: Vec<(PathBuf, Listed)>,
        marks: Option<HashMap<PathBuf, Mark>>,
    ) -> bool {
        // Another root's refresh may be under way since (`forget`): not this one's to end.
        let Some(tree) = self.tree.as_mut().filter(|t| t.root() == root) else {
            return false;
        };
        self.wait = back_off(self.wait, marks.is_some());
        if let Some(marks) = marks {
            tree.set_marks(marks);
        }
        self.refreshing = false;
        self.listed(root, listings);
        true
    }
}

/// A panel's note in place of its rows (an empty state): 13 pt, dim, 12 pt in from the left.
pub(crate) fn note(ui: &UiColors, text: &'static str) -> AnyElement {
    div()
        .px(px(12.0))
        .py(px(8.0))
        .text_size(px(text::NAME))
        .text_color(hsla(ui.text_dim))
        .child(text)
        .into_any_element()
}

/// The wait before the next refresh, after one that waited `wait`: `REFRESH` after a good git
/// status; after a failed or slow one, twice as long, up to 30 s (2 → 4 → 8 → 16 → 30 s).
pub fn back_off(wait: Duration, ok: bool) -> Duration {
    const MOST: Duration = Duration::from_secs(30);
    if ok { REFRESH } else { (wait * 2).min(MOST) }
}

/// A width it can take: within 220–560 pt (the default for a missing or odd one).
pub fn clamp(width: Option<f32>) -> f32 {
    match width {
        Some(w) if w.is_finite() => w.clamp(RIGHT_SIDEBAR_MIN_WIDTH, RIGHT_SIDEBAR_MAX_WIDTH),
        _ => RIGHT_SIDEBAR_DEFAULT_WIDTH,
    }
}

/// The narrowest the pane cards get for the right sidebar (there's no pane minimum elsewhere).
pub const PANE_COLUMN_MIN: f32 = 200.0;

/// How wide its card is drawn: its `width`, less what a window `window_width` wide needs to
/// keep the pane cards `PANE_COLUMN_MIN` wide beside a left sidebar `left` wide (0 when
/// hidden), with the three 6 pt gaps: before the pane cards (from the sidebar, or the window's
/// left edge), between them and the right card, and after it. Never less than none. The saved
/// width stays as chosen.
pub fn drawn_width(width: f32, window_width: f32, left: f32) -> f32 {
    width.min((window_width - left - PANE_COLUMN_MIN - 3.0 * CARD_GAP).max(0.0))
}

/// Its card's width with the pointer at `x`, in the middle of the gap at its left, in a window
/// `window_width` wide (the card ends 6 pt short of the window's right).
pub fn width_for_edge(window_width: f32, x: f32) -> f32 {
    clamp(Some(window_width - CARD_GAP - (x + CARD_GAP / 2.0)))
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
    /// The panel shown (M11h; saved).
    pub(crate) panel: Panel,
    /// The Files panel.
    pub(crate) files: FilesPanel,
    /// The Changes panel (M11h).
    pub(crate) changes: ChangesPanel,
    /// The Activity panel's scroll (M11h): back to the newest when it is shown again or
    /// follows another terminal.
    pub(crate) activity_scroll: UniformListScrollHandle,
    /// The panel tabs, as last drawn (the screenshots click them).
    #[cfg(feature = "ui-shots")]
    tabs_at: Rc<Cell<[Option<Bounds<Pixels>>; 3]>>,
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
            panel: Panel::Files,
            files: FilesPanel::new(),
            changes: ChangesPanel::new(),
            activity_scroll: UniformListScrollHandle::new(),
            #[cfg(feature = "ui-shots")]
            tabs_at: Rc::default(),
        }
    }

    /// Take its open state, width and panel from the sessions file.
    pub fn load(&mut self, saved: &SavedState) {
        self.hidden = saved.right_sidebar_hidden;
        self.width = clamp(saved.right_sidebar_width);
        self.panel = Panel::from_key(saved.right_sidebar_panel.as_deref());
    }

    /// The Files, Changes and Activity tabs, as last drawn.
    #[cfg(feature = "ui-shots")]
    pub(crate) fn panel_tab_bounds(&self) -> [Option<Bounds<Pixels>>; 3] {
        self.tabs_at.get()
    }

    /// The shown panel brings itself up to date at once (shown, another terminal); Activity
    /// starts again at its newest row.
    fn refresh_now(&mut self) {
        match self.panel {
            Panel::Files => self.files.refreshed = None,
            Panel::Changes => self.changes.refreshed = None,
            Panel::Activity => self.activity_scroll = UniformListScrollHandle::new(),
        }
    }
}

impl Workspace {
    /// ⌥⌘\, View → Toggle Right Sidebar, a pane's icon: open or close the right sidebar. It
    /// slides unless Reduce motion is on (a slide under way turns back from where it got to).
    /// The keyboard stays where it is, unless the Files panel (its filter box or tree) has it:
    /// closing then gives it to the focused tab. (Asked of the panel's own focus, as last drawn:
    /// `Window::focused` keeps answering with a handle no longer drawn, so it can't tell.)
    /// Shown again, the shown panel refreshes.
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
            state.refresh_now();
        }
        self.save();
        cx.notify();
    }

    /// A panel tab's click: show `panel` (saved), brought up to date at once. Leaving Files
    /// while its filter box or tree has the keyboard gives the keyboard to the focused tab (as
    /// closing the sidebar does): Changes and Activity take no keys.
    pub(crate) fn show_right_panel(
        &mut self,
        panel: Panel,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let state = &mut self.right_sidebar;
        if state.panel == panel {
            return;
        }
        let files_keys = state.panel == Panel::Files
            && state
                .files
                .focus
                .as_ref()
                .is_some_and(|f| f.contains_focused(window, cx));
        state.panel = panel;
        state.refresh_now();
        if files_keys {
            self.focus_active(window, cx);
        }
        self.save();
        self.refresh_right_sidebar(cx);
        cx.notify();
    }

    /// Whether `panel` may do its work now: it is the one shown, in a sidebar that can be seen.
    pub(crate) fn right_panel_works(&self, panel: Panel, cx: &App) -> bool {
        self.right_sidebar.panel == panel && self.right_sidebar_wanted(cx)
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
    pub(crate) fn reported_folder(
        &self,
        id: u64,
        cx: &App,
    ) -> Option<(Option<PathBuf>, Option<u32>)> {
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
    /// and bring the shown panel up to date (only that one: Files or Changes; Activity asks
    /// nothing). `render` calls it on every frame, the 2 s refresh between frames.
    pub(crate) fn refresh_right_sidebar(&mut self, cx: &mut Context<Self>) {
        // None followed yet (relaunched with a file or browser tab focused): the focused
        // workspace's first terminal, else the active one.
        let id = self.focused_terminal().or_else(|| {
            self.right_sidebar
                .followed
                .is_none()
                .then(|| self.first_terminal_to_follow())
                .flatten()
        });
        if let Some(id) = id
            && self.right_sidebar.followed.replace(id) != Some(id)
        {
            // Another terminal: its files or changes at once.
            self.right_sidebar.refresh_now();
        }
        if !self.right_sidebar_wanted(cx) {
            return;
        }
        self.look_up_right_sidebar_folder(cx);
        match self.right_sidebar.panel {
            Panel::Files => self.refresh_files(cx),
            Panel::Changes => self.refresh_changes(cx),
            Panel::Activity => {}
        }
    }

    /// The terminal to follow when none has been: the focused workspace's first, else the
    /// active session's.
    fn first_terminal_to_follow(&self) -> Option<u64> {
        let workspace = self.model.focused_workspace();
        workspace
            .and_then(|w| self.model.terminals(w).first().copied())
            .or_else(|| self.sessions.active().map(|s| s.id))
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
        // A window too narrow for it beside the panes: no card at all (not an empty stub).
        let width = self.right_sidebar_drawn_width(window);
        if width <= 0.0 {
            return (None, None);
        }
        self.ensure_files_panel(window, cx);
        self.update_file_rows();
        let node = self.render_right_sidebar(ui, width, window, cx);
        match slide {
            Some(shown) => {
                // The card and the gap at its right go out of sight together.
                let over = div()
                    .absolute()
                    .top_0()
                    .right(px(-(1.0 - shown) * (width + CARD_GAP)))
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

    /// The right sidebar: a card `width` wide (M11i), with the 6 pt gap at its right and the
    /// one at its bottom. Above it, with the left sidebar hidden, the window's top band (as over
    /// the panes, so its header lines up with their tab bars), else the 6 pt strip that drags
    /// the window. In the card: the panel tabs on a strip drawn like the pane tab bars (M11h;
    /// its empty part drags the window), then the shown panel. The gap at its left resizes it.
    fn render_right_sidebar(
        &self,
        ui: UiColors,
        width: f32,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (band, above) = if self.sidebar.hidden {
            (
                self.render_top_band("right-sidebar-top", None, window, cx),
                TOP_BAND,
            )
        } else {
            (
                Some(self.render_drag_strip("right-sidebar-strip", window, cx)),
                CARD_GAP,
            )
        };
        // What the tabs leave empty: it drags the window (not in full screen).
        let empty = div()
            .id("right-sidebar-band-empty")
            .flex_1()
            .min_w(px(0.0))
            .h_full();
        let empty = if window.is_fullscreen() {
            empty
        } else {
            self.drag_window_by(empty, cx)
        };
        let strip = div()
            .id("right-sidebar-band")
            .flex()
            .flex_row()
            .flex_none()
            .w_full()
            .h(px(TAB_BAR))
            .bg(hsla(ui.tab_bar))
            // The card's top, rounded as the inside of its edge (as a pane's tab bar); its
            // tabs clipped to the strip's bounds (GPUI's mask is a rectangle, not the rounded
            // corners), so a narrow card never lets a tab spill past the strip.
            .rounded_t(px(CARD_RADIUS - FOCUS_RING))
            .overflow_hidden()
            // The tabs float 4 pt under its top, flush with its bottom.
            .pt(px(TAB_TOP_GAP))
            .pl(px(PANEL_TABS_LEFT))
            .gap(px(PANEL_TAB_GAP))
            .children(Panel::ALL.map(|p| self.render_panel_tab(p, ui, cx)))
            .child(empty);
        let body = match self.right_sidebar.panel {
            Panel::Files => self.render_files_panel(ui, width, cx),
            Panel::Changes => self.render_changes_panel(ui, width, cx),
            Panel::Activity => self.render_activity_panel(ui, width, cx),
        };
        let card = panes::card(&PaneColors::of(&ui), false)
            .flex()
            .flex_col()
            .flex_1()
            .min_h(px(0.0))
            .mr(px(CARD_GAP))
            .mb(px(CARD_GAP))
            .child(strip)
            .child(body);
        div()
            .id("right-sidebar")
            .relative()
            .flex()
            .flex_col()
            .flex_none()
            .w(px(width + CARD_GAP))
            .h_full()
            .children(band)
            .child(card)
            .child(self.render_right_resize_edge(ui, above, cx))
            .into_any_element()
    }

    /// `panel`'s tab in the strip (M11h), drawn as a pane's tab: 13 pt words, 10 pt each side,
    /// top corners rounded; the shown panel's filled with the card (merging into it) in the
    /// text colour, the others unfilled, dim, `tab_hover` on hover. A click shows its panel.
    fn render_panel_tab(&self, panel: Panel, ui: UiColors, cx: &mut Context<Self>) -> AnyElement {
        let active = self.right_sidebar.panel == panel;
        let i = panel as usize;
        let tab = div()
            .id(("right-panel-tab", i))
            .relative()
            .flex()
            .flex_none()
            .items_center()
            .h_full()
            .px(px(PANEL_TAB_PAD))
            .rounded_t(px(tab_bar::TAB_RADIUS))
            .text_size(px(text::NAME))
            .text_color(hsla(if active { ui.text } else { ui.text_dim }))
            .when(active, |t| t.bg(hsla(ui.card)))
            .when(!active, |t| {
                t.cursor_pointer().hover(move |s| s.bg(hsla(ui.tab_hover)))
            })
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                this.show_right_panel(panel, window, cx)
            }))
            .child(panel.label());
        // Where it is (the screenshots click it).
        #[cfg(feature = "ui-shots")]
        let tab = {
            let at = self.right_sidebar.tabs_at.clone();
            tab.child(
                canvas(
                    move |b, _, _| {
                        let mut all = at.get();
                        all[i] = Some(b);
                        at.set(all);
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
        };
        tab.into_any_element()
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
    pub(crate) fn followed_ssh(&self) -> bool {
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
        let now = Instant::now();
        if files.needs_root(&folder, now) {
            files.folder = Some(folder.clone());
            files.rooting = true;
            cx.spawn(async move |this, cx| {
                let asked = folder.clone();
                let found = cx
                    .background_executor()
                    .spawn(async move { find_root(&asked) })
                    .await;
                this.update(cx, |this, cx| {
                    let files = &mut this.right_sidebar.files;
                    // The terminal has moved on since: its own lookup is under way.
                    if files.folder.as_ref() != Some(&folder) {
                        return;
                    }
                    // No answer from git: un-rooted, asked again on a later tick.
                    let Some((root, in_repo, revealed)) = found else {
                        files.unanswered(Instant::now());
                        cx.notify();
                        return;
                    };
                    files.rooting = false;
                    files.rooted(root, in_repo, revealed);
                    files.refreshed = None;
                    // Not while another panel shows: it refreshes when shown again.
                    if this.right_panel_works(Panel::Files, cx) {
                        this.refresh_files(cx);
                    }
                    cx.notify();
                })
                .ok();
            })
            .detach();
            return;
        }
        if files.rooting || files.refreshing || !due(files.refreshed, files.wait, now) {
            return;
        }
        let Some(tree) = files.tree.as_ref() else {
            return;
        };
        // While the root is folded away (no rows), its open folders are still listed and
        // marked: unfolding then shows a fresh tree at once, not one as stale as the fold.
        let (root, in_repo, folders) = (
            tree.root().to_path_buf(),
            files.in_repo,
            tree.open_folders(),
        );
        let mut last = files.last_ignored(&folders);
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
                            let was = last.remove(&rel);
                            let listing = list_folder(git.as_deref(), &at, &rel, in_repo, was);
                            (rel, listing)
                        })
                        .collect();
                    // Outside a repo (or with no git): nothing to mark, an answer all the same.
                    let marks = match git {
                        Some(git) => git_status::status(&git, &at),
                        None => Some(HashMap::new()),
                    };
                    (listings, marks)
                })
                .await;
            this.update(cx, |this, cx| {
                let files = &mut this.right_sidebar.files;
                let before = files.tree.as_ref().map(FileTree::generation);
                if files.apply_refresh(&root, listings, marks)
                    && files.tree.as_ref().map(FileTree::generation) != before
                {
                    cx.notify();
                }
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
        let mut last = files.last_ignored(&rels);
        cx.spawn(async move |this, cx| {
            let at = root.clone();
            let listings: Vec<_> = cx
                .background_executor()
                .spawn(async move {
                    let git = in_repo.then(find_git).flatten();
                    rels.into_iter()
                        .map(|rel| {
                            let was = last.remove(&rel);
                            let listing = list_folder(git.as_deref(), &at, &rel, in_repo, was);
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

    /// A key or click on the tree (`FilesPanel::act`): list the folders it opened, off the UI
    /// thread, or open a file in a tab in the focused pane (selected if that pane already has
    /// it).
    fn files_act(
        &mut self,
        key: impl FnOnce(&mut FileTree, &str) -> Option<Want>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.right_sidebar.files.tree.is_none() {
            return;
        }
        let (want, list) = self.right_sidebar.files.act(key);
        self.files_list(list, cx);
        if let Some(Want::Open(rel)) = want {
            let Some(root) = self.right_sidebar.files.tree.as_ref().map(|t| t.root()) else {
                return;
            };
            let path = root.join(rel);
            self.open_file_tab(path, crate::layout_model::FileMode::Plain, window, cx);
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
        self.files_act(key, window, cx);
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
        let key = |tree: &mut FileTree, _: &str| {
            tree.select(rel);
            if is_dir {
                tree.toggle(rel)
            } else {
                Some(Want::Open(rel.to_path_buf()))
            }
        };
        self.files_act(key, window, cx);
    }

    /// A double-click on folder `rel`: the followed terminal `cd`s there, if it is a local
    /// one at an empty prompt (2026-11-10, your call). Nothing otherwise.
    fn files_cd(&mut self, rel: &Path, cx: &mut Context<Self>) {
        let Some(root) = self
            .right_sidebar
            .files
            .tree
            .as_ref()
            .map(|t| t.root().to_path_buf())
        else {
            return;
        };
        let Some(id) = self.right_sidebar.followed else {
            return;
        };
        let Some(session) = self.sessions.items().iter().find(|s| s.id == id) else {
            return;
        };
        if session.ssh.is_some() {
            return; // the tree is this Mac's; the terminal is elsewhere
        }
        let folder = root.join(rel);
        session.view.update(cx, |view, cx| view.cd_to(&folder, cx));
    }

    /// A click on the root header's name line: the root folds away (only the header shows) or
    /// comes back, remembered for the root.
    fn files_toggle_root(&mut self, cx: &mut Context<Self>) {
        let files = &mut self.right_sidebar.files;
        if let Some(tree) = files.tree.as_mut() {
            tree.set_root_collapsed(!tree.root_collapsed());
            files.reveal = Some(ScrollStrategy::Nearest);
            cx.notify();
        }
    }

    /// The header's collapse all: every folder closes, the selection goes to its top-level
    /// folder; the filter stays.
    fn files_collapse_all(&mut self, cx: &mut Context<Self>) {
        let files = &mut self.right_sidebar.files;
        if let Some(tree) = files.tree.as_mut() {
            tree.collapse_all();
            files.reveal = Some(ScrollStrategy::Nearest);
            cx.notify();
        }
    }

    /// The root header (M11i), for a card `width` wide: ▾ (▸ while folded away), the root
    /// folder's name and, at the right, collapse all; under them its path, home-relative, its
    /// start cut to fit (the whole path in its tooltip). A click on the name line folds the
    /// root away or brings it back.
    fn render_root_header(
        &self,
        tree: &FileTree,
        ui: UiColors,
        width: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let root = tree.root();
        let name = root
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| root.display().to_string());
        let chevron = if tree.root_collapsed() {
            "\u{25b8}"
        } else {
            "\u{25be}"
        };
        let room = width - 2.0 * FOCUS_RING - ROOT_PAD_LEFT - ROOT_PAD_RIGHT;
        let wrapper = std::cell::RefCell::new(
            cx.text_system()
                .line_wrapper(TextStyle::default().font(), px(text::META)),
        );
        // As the drawn line decides whether to cut itself (`truncate`).
        let fits = |s: &str| {
            wrapper
                .borrow_mut()
                .should_truncate_line(s, px(room), "\u{2026}", TruncateFrom::End)
                .is_none()
        };
        let path = root_path_line(root, self.places.home.as_deref(), fits);
        let full: SharedString = root.display().to_string().into();
        let group = "files-collapse-all";
        let collapse_all = div()
            .id(group)
            .group(group)
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .relative()
            .size(px(tab_bar::BUTTON))
            .rounded(px(4.0))
            .cursor_pointer()
            .hover(move |s| s.bg(hsla(ui.card_hover)))
            // Its press and click are its own, not the name line's.
            .on_mouse_down(MouseButton::Left, |_: &MouseDownEvent, _, cx| {
                cx.stop_propagation()
            })
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                this.files_collapse_all(cx)
            }))
            .tooltip(|_, cx| -> AnyView { cx.new(|_| Tip(COLLAPSE_ALL_TIP.into())).into() })
            .child(
                glyph(Glyph::CollapseAll, ui.text_dim)
                    .absolute()
                    .group_hover(group, |s| s.invisible()),
            )
            .child(
                glyph(Glyph::CollapseAll, ui.text)
                    .absolute()
                    .invisible()
                    .group_hover(group, |s| s.visible()),
            );
        let line = div()
            .id("files-root")
            .flex()
            .items_center()
            .h(px(ROOT_LINE))
            .w_full()
            .cursor_pointer()
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.files_toggle_root(cx)))
            .child(
                div()
                    .flex_none()
                    .mr(px(ROOT_CHEVRON_GAP))
                    .text_size(px(text::META))
                    .text_color(hsla(ui.text_dim))
                    .child(chevron),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .truncate()
                    .text_size(px(text::NAME))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(hsla(ui.text))
                    .child(name),
            )
            .child(collapse_all);
        let path_line = div()
            .id("files-root-path")
            .h(px(PATH_LINE))
            .w_full()
            .truncate()
            .text_size(px(text::META))
            .text_color(hsla(ui.text_faint))
            .tooltip(move |_, cx| -> AnyView { cx.new(|_| Tip(full.clone())).into() })
            .child(path);
        div()
            .flex_none()
            .flex()
            .flex_col()
            .justify_center()
            .h(px(ROOT_HEADER))
            .w_full()
            .pl(px(ROOT_PAD_LEFT))
            .pr(px(ROOT_PAD_RIGHT))
            .child(line)
            .child(path_line)
            .into_any_element()
    }

    /// The Files panel under the strip: the root header, the filter box, then the tree
    /// (or what stands in for it: an SSH terminal's note, no folder, no match); with the root
    /// folded away, the header alone. It has its own focus, and the tree's keys (key context
    /// `FileTree`, the box included: ↑ ↓ move through the rows while typing).
    fn render_files_panel(&self, ui: UiColors, width: f32, cx: &mut Context<Self>) -> AnyElement {
        let files = &self.right_sidebar.files;
        let note = |text: &'static str| note(&ui, text);
        let ssh = self.followed_ssh();
        let tree = files.tree.as_ref().filter(|_| !ssh);
        let folded = tree.is_some_and(FileTree::root_collapsed);
        let header = tree.map(|tree| self.render_root_header(tree, ui, width, cx));
        let body = if ssh {
            Some(note(SSH_NOTE))
        } else if folded {
            None
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
                    let inks = RowInks::of(&cx.global::<Appearance>().theme);
                    let files = &this.right_sidebar.files;
                    let selected = files.tree.as_ref().and_then(|t| t.selected());
                    range
                        .filter_map(|i| files.rows.get(i).map(|row| (i, row)))
                        .map(|(i, row)| file_row(i, row, selected, &inks, cx))
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
            .filter(|_| !ssh && files.tree.is_some() && !folded)
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
            .children(header)
            .children(filter)
            .child(div().flex_1().min_h(px(0.0)).w_full().children(body));
        match &files.focus {
            Some(focus) => panel.track_focus(focus).into_any_element(),
            None => panel.into_any_element(),
        }
    }

    /// The 6 pt gap left of the right sidebar's card, beside it from `top` down (M11i): drag it
    /// to resize the sidebar. A 2 px accent line shows in its middle on hover and while
    /// dragging; while dragging, the whole window follows the mouse until release (as the left
    /// sidebar's edge does).
    fn render_right_resize_edge(
        &self,
        ui: UiColors,
        top: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
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
            .top(px(top))
            .bottom(px(CARD_GAP))
            .left(px(-CARD_GAP))
            .w(px(CARD_GAP))
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
                    .left(px((CARD_GAP - DIVIDER_ACTIVE) / 2.0))
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
    fn panels_are_files_changes_activity_and_load_by_key() {
        assert_eq!(Panel::ALL, [Panel::Files, Panel::Changes, Panel::Activity]);
        let labels = Panel::ALL.map(Panel::label);
        assert_eq!(labels, ["Files", "Changes", "Activity"]);
        let keys = Panel::ALL.map(Panel::key);
        assert_eq!(keys, ["files", "changes", "activity"]);
        for panel in Panel::ALL {
            assert_eq!(Panel::from_key(Some(panel.key())), panel);
        }
        assert_eq!(Panel::from_key(Some("changes")), Panel::Changes);
        // Missing or unknown (a newer or hand-edited file): Files.
        assert_eq!(Panel::from_key(None), Panel::Files);
        assert_eq!(Panel::from_key(Some("bogus")), Panel::Files);
        assert_eq!(Panel::from_key(Some("Changes")), Panel::Files);
        assert_eq!(Panel::default(), Panel::Files);
        // Loaded with the rest of the sidebar's state.
        let mut state = RightSidebarState::new();
        let saved = decode(r#"{"version":2,"right_sidebar_panel":"activity"}"#).expect("loads");
        state.load(&saved);
        assert_eq!(state.panel, Panel::Activity);
        state.load(&decode(r#"{"version":2}"#).expect("loads"));
        assert_eq!(state.panel, Panel::Files);
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
        // M11i: the pointer is in the middle of the 6 pt gap left of the card, and the card
        // ends 6 pt short of the window's right. In a 1280 pt window, the pointer at x = 971
        // makes it 300 wide (971 + 3 to 1274); 80 left, 80 wider.
        assert_eq!(width_for_edge(1280.0, 971.0), 300.0);
        assert_eq!(width_for_edge(1280.0, 891.0), 380.0);
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
        // 700 wide with the left sidebar's 240: the panes keep 200 and the three 6 pt gaps
        // (sidebar → pane card → right card → window edge) take 18, so 242 is drawn.
        assert_eq!(drawn_width(300.0, 700.0, 240.0), 242.0);
        // Without the left sidebar (the window's edge → pane card gap instead): 500 wide
        // leaves it 282.
        assert_eq!(drawn_width(300.0, 518.0, 0.0), 300.0);
        assert_eq!(drawn_width(300.0, 500.0, 0.0), 282.0);
        assert_eq!(drawn_width(560.0, 500.0, 0.0), 282.0);
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

    #[test]
    fn a_failed_status_backs_off_to_30_s_and_a_good_one_resets_it() {
        let mut wait = REFRESH;
        let mut seen = Vec::new();
        for _ in 0..6 {
            wait = back_off(wait, false);
            seen.push(wait.as_secs());
        }
        assert_eq!(seen, [4, 8, 16, 30, 30, 30]);
        assert_eq!(back_off(wait, true), REFRESH);
        assert_eq!(back_off(REFRESH, true), REFRESH);
    }

    fn entry(name: &str, is_dir: bool) -> DirEntry {
        DirEntry {
            name: name.into(),
            is_dir,
        }
    }

    fn mark_of(files: &FilesPanel, rel: &str) -> Option<Mark> {
        let tree = files.tree.as_ref().unwrap();
        tree.rows("")
            .into_iter()
            .find(|r| r.rel == Path::new(rel))
            .and_then(|r| r.mark)
    }

    fn shown(files: &FilesPanel) -> Vec<String> {
        let tree = files.tree.as_ref().unwrap();
        tree.rows("")
            .into_iter()
            .map(|r| r.rel.display().to_string())
            .collect()
    }

    /// git's answer to "which repo is this folder in": a top folder (a repo's root), "not a
    /// repo" (exit 128, or no git: the folder is the root), or no answer (failed, slower than
    /// the cap): no root, to ask again later.
    #[test]
    fn a_root_is_a_repo_s_top_else_the_folder_and_no_answer_is_none() {
        let real = Path::new("/w/app/src");
        assert_eq!(
            root_from(real, Some(Some(PathBuf::from("/w/app")))),
            Some((PathBuf::from("/w/app"), true, Some(PathBuf::from("src"))))
        );
        assert_eq!(
            root_from(real, Some(None)),
            Some((real.to_path_buf(), false, Some(PathBuf::new())))
        );
        assert_eq!(root_from(real, None), None);
    }

    /// A folder's root lookup that got no answer leaves it un-rooted: it is asked again on a
    /// later tick, each time waiting longer (`back_off`: 4, 8 … s); another folder at once.
    #[test]
    fn files_asks_for_an_unanswered_root_again_backing_off() {
        let folder = Path::new("/w/app/src");
        let s = Duration::from_secs;
        let now = Instant::now();
        let mut files = FilesPanel::new();
        assert!(files.needs_root(folder, now), "a new folder at once");
        files.folder = Some(folder.to_path_buf());
        files.rooting = true;
        assert!(!files.needs_root(folder, now), "under way");
        files.unanswered(now);
        assert!(!files.rooting);
        assert!(files.tree.is_none(), "un-rooted");
        assert!(!files.needs_root(folder, now + s(3)));
        assert!(
            files.needs_root(folder, now + s(4)),
            "asked again after 4 s"
        );
        files.rooting = true;
        files.unanswered(now + s(4));
        assert!(!files.needs_root(folder, now + s(11)));
        assert!(files.needs_root(folder, now + s(12)), "then after 8 s");
        assert!(files.needs_root(Path::new("/w/other"), now + s(2)));
        // Answered: rooted, not asked again for this folder.
        files.rooted(PathBuf::from("/w/app"), true, Some(PathBuf::from("src")));
        assert!(!files.needs_root(folder, now + s(60)));
    }

    #[test]
    fn a_failed_git_answer_keeps_the_last_marks_and_ignored_files() {
        let root = PathBuf::from("/repo");
        let mut files = FilesPanel::new();
        files.rooted(root.clone(), true, None);
        let good = Listed {
            entries: vec![entry("a.rs", false)],
            more: 0,
            ignored: Some(HashSet::from([PathBuf::from("target")])),
            nested: HashSet::new(),
        };
        let marks = HashMap::from([(PathBuf::from("a.rs"), Mark::Modified)]);
        files.refreshing = true;
        assert!(files.apply_refresh(&root, vec![(PathBuf::new(), good)], Some(marks)));
        assert_eq!(mark_of(&files, "a.rs"), Some(Mark::Modified));
        assert_eq!(files.wait, REFRESH);
        // git timed out on both: the listing shows `target` (unfiltered), status said nothing.
        let failed = Listed {
            entries: vec![entry("target", true), entry("a.rs", false)],
            more: 0,
            ignored: None,
            nested: HashSet::new(),
        };
        files.refreshing = true;
        assert!(files.apply_refresh(&root, vec![(PathBuf::new(), failed)], None));
        assert!(!files.refreshing);
        assert_eq!(mark_of(&files, "a.rs"), Some(Mark::Modified), "marks kept");
        assert_eq!(shown(&files), ["a.rs"], "target stays hidden");
        assert_eq!(files.wait, Duration::from_secs(4), "backed off");
        // A good answer again: applied, and the wait is 2 s again.
        assert!(files.apply_refresh(&root, Vec::new(), Some(HashMap::new())));
        assert_eq!(mark_of(&files, "a.rs"), None);
        assert_eq!(files.wait, REFRESH);
        // Another root's answer is dropped.
        assert!(!files.apply_refresh(Path::new("/other"), Vec::new(), None));
        assert_eq!(files.wait, REFRESH);
    }

    /// Opening a folder by click, → or Return shows its kept listing at once and lists it
    /// again in the background (the tree asks for nothing: Task 6 review), so a file made in
    /// it while it was closed shows.
    #[test]
    fn opening_a_folder_with_a_kept_listing_lists_it_again() {
        let mut files = FilesPanel::new();
        files.rooted(PathBuf::from("/repo"), true, None);
        let tree = files.tree.as_mut().unwrap();
        tree.relist(
            PathBuf::new(),
            vec![entry("docs", true), entry("src", true)],
            0,
        );
        tree.toggle(Path::new("src"));
        tree.relist(PathBuf::from("src"), vec![entry("main.rs", false)], 0);
        tree.collapse(Path::new("src"));
        // Opened again (a click): no Want, its rows at once, and a re-list of it.
        let (want, list) = files.act(|tree, _| tree.toggle(Path::new("src")));
        assert_eq!(want, None);
        assert_eq!(list, vec![PathBuf::from("src")]);
        assert_eq!(shown(&files), ["docs", "src", "src/main.rs"]);
        // Closed (Return): nothing to list.
        let (want, list) = files.act(|tree, _| tree.toggle(Path::new("src")));
        assert_eq!((want, list), (None, vec![]));
        // → on it: opened again, listed again.
        files.tree.as_mut().unwrap().select(Path::new("src"));
        let (_, list) = files.act(|tree, query| tree.expand_selected(query));
        assert_eq!(list, vec![PathBuf::from("src")]);
        // A folder never listed: the tree asks, and it is listed once.
        let (want, list) = files.act(|tree, _| tree.toggle(Path::new("docs")));
        assert_eq!(want, Some(Want::List(PathBuf::from("docs"))));
        assert_eq!(list, vec![PathBuf::from("docs")]);
        // A move opens nothing: nothing to list.
        let (want, list) = files.act(|tree, query| {
            tree.select_next(query);
            None
        });
        assert_eq!((want, list), (None, vec![]));
    }

    #[test]
    fn a_collapsed_root_is_remembered_across_a_re_root() {
        let (repo, other) = (PathBuf::from("/repo"), PathBuf::from("/other"));
        let mut files = FilesPanel::new();
        files.rooted(repo.clone(), true, Some(PathBuf::from("src")));
        files.tree.as_mut().unwrap().set_root_collapsed(true);
        // A cd elsewhere: that root starts open.
        files.rooted(other.clone(), true, None);
        assert!(!files.tree.as_ref().unwrap().root_collapsed());
        // Back: collapsed as it was left, its folders remembered too.
        files.rooted(repo, true, None);
        let tree = files.tree.as_ref().unwrap();
        assert!(tree.root_collapsed());
        assert!(tree.expanded_set().expanded.contains(Path::new("src")));
        assert!(tree.rows("").is_empty());
        // And the other root comes back open.
        files.rooted(other, true, None);
        assert!(!files.tree.as_ref().unwrap().root_collapsed());
    }

    #[test]
    fn a_file_s_kind_is_its_extension_s_in_any_case() {
        use FileKind::*;
        let table: [(FileKind, &[&str]); 6] = [
            (Red, &["rs", "rb", "swift", "scala", "html", "htm"]),
            (Green, &["sh", "bash", "zsh", "fish", "vue", "lua"]),
            (Yellow, &["js", "jsx", "mjs", "cjs", "py"]),
            (
                Blue,
                &[
                    "ts", "tsx", "mts", "cts", "c", "h", "cpp", "hpp", "cc", "css", "scss", "java",
                    "kt",
                ],
            ),
            (
                Magenta,
                &["png", "jpg", "jpeg", "gif", "svg", "webp", "ico", "pdf"],
            ),
            (
                Cyan,
                &["go", "json", "jsonc", "yaml", "yml", "toml", "xml", "sql"],
            ),
        ];
        for (kind, extensions) in table {
            for ext in extensions {
                assert_eq!(file_kind(&format!("a.{ext}")), kind, "a.{ext}");
                let upper = format!("A.{}", ext.to_uppercase());
                assert_eq!(file_kind(&upper), kind, "{upper}");
            }
        }
        for plain in [
            "README.md",
            "notes.txt",
            "Cargo.lock",
            "Cargo.LOCK",
            ".gitignore",
            ".env",
            "Makefile",
            "archive.tar.gz",
            "a.unknown",
            "trailing.",
        ] {
            assert_eq!(file_kind(plain), Plain, "{plain}");
        }
        assert_eq!(file_kind("main.RS"), Red);
    }

    /// Every colour a Files row draws over its fills (`card`, hovered, selected) reaches
    /// `MARK_MIN` on each, in every built-in theme (Task 1 review: the tints and git marks on
    /// `card_hover` and `card_selected`).
    #[test]
    fn type_tints_and_marks_read_on_every_files_row_fill() {
        use crate::ui_tokens::contrast;
        let mut failures = vec![];
        for (name, theme) in crate::theme::every_theme() {
            let ui = theme.ui();
            let fills = [
                ("card", ui.card),
                ("card_hover", ui.card_hover),
                ("card_selected", ui.card_selected),
            ];
            let tints = FileKind::ALL.map(|k| (format!("{k:?}"), kind_color(k, &theme)));
            let marks = [Mark::Modified, Mark::Added, Mark::Deleted]
                .map(|m| (format!("{m:?}"), mark_color(m, &theme)));
            for (what, ink) in tints.iter().chain(&marks) {
                for (fill_name, fill) in fills {
                    let ratio = contrast(*ink, fill);
                    if ratio < MARK_MIN {
                        failures.push(format!("{name}: {what} on {fill_name} is {ratio:.2}"));
                    }
                }
            }
            // Plain files are drawn in the dim text colour; the tints keep their ANSI hue.
            assert_eq!(kind_color(FileKind::Plain, &theme), ui.text_dim, "{name}");
        }
        assert!(failures.is_empty(), "\n{}", failures.join("\n"));
    }

    #[test]
    fn the_rows_inks_are_the_tints_and_marks() {
        for (name, theme) in crate::theme::every_theme() {
            let inks = RowInks::of(&theme);
            for kind in FileKind::ALL {
                assert_eq!(
                    inks.kind(kind),
                    kind_color(kind, &theme),
                    "{name}: {kind:?}"
                );
            }
            for mark in [Mark::Modified, Mark::Added, Mark::Deleted] {
                assert_eq!(
                    inks.mark(mark),
                    mark_color(mark, &theme),
                    "{name}: {mark:?}"
                );
            }
        }
    }

    #[test]
    fn a_tint_is_its_ansi_colour_when_it_already_reads() {
        // Mocha's red on its own background is far past 3:1: drawn as the theme has it.
        let (_, theme) = crate::theme::every_theme()
            .into_iter()
            .find(|(name, _)| *name == "Catppuccin Mocha")
            .expect("a built-in theme");
        assert_eq!(kind_color(FileKind::Red, &theme), theme.palette[1]);
        assert_eq!(kind_color(FileKind::Cyan, &theme), theme.palette[6]);
    }

    #[test]
    fn guides_sit_at_each_ancestor_level_s_chevron_centre() {
        assert!(guides(0).is_empty(), "none at the top level");
        assert_eq!(guides(1), vec![14.0]);
        assert_eq!(guides(3), vec![14.0, 28.0, 42.0]);
    }

    #[test]
    fn the_root_s_path_is_home_relative_and_cut_at_its_start() {
        let home = Some(Path::new("/Users/me"));
        let root = Path::new("/Users/me/Kerja/proj/fuxx-cli");
        let all = |_: &str| true;
        assert_eq!(root_path_line(root, home, all), "~/Kerja/proj/fuxx-cli");
        // Room for 17 characters: its start cut at a folder boundary (not `…ja/proj/fuxx-cli`,
        // which a cut by characters would give).
        let room = |n: usize| move |s: &str| s.chars().count() <= n;
        assert_eq!(root_path_line(root, home, room(17)), "…/proj/fuxx-cli");
        assert_eq!(root_path_line(root, home, room(14)), "…/fuxx-cli");
        assert_eq!(root_path_line(root, home, room(10)), "…/fuxx-cli");
        // No whole folder fits: cut by characters, never shorter than the folder's own name.
        assert_eq!(root_path_line(root, home, room(9)), "…fuxx-cli");
        let none = |_: &str| false;
        assert_eq!(root_path_line(root, home, none), "…fuxx-cli");
        // A full path outside the home folder is cut the same way.
        let long =
            Path::new("/private/var/folders/5n96dq4xpg82l70gnjh0000gp/T/fuxx-ui-shots-files");
        assert_eq!(
            root_path_line(long, home, room(30)),
            "…/T/fuxx-ui-shots-files"
        );
        // Outside the home folder: the whole path; the home folder itself: ~.
        assert_eq!(root_path_line(Path::new("/tmp/x"), home, all), "/tmp/x");
        assert_eq!(root_path_line(Path::new("/Users/me"), home, all), "~");
    }
}
