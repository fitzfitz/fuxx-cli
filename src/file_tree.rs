//! The Files panel's tree (M11g): what's listed, what's open, what's selected, and the git
//! marks, as rows to draw. Pure: the panel (`right_sidebar`) lists folders and runs git off the
//! UI thread and hands the results in.
//!
//! - Rooted at the repo's top folder, or outside a repo at the terminal's folder; paths are
//!   relative to the root (`""` is the root, always open, never a row).
//! - Lazy: a folder's children are rows only once it's been listed (`expand`); closing it keeps
//!   the listing, so opening it again shows them at once. Each listing is capped (`CAP`, by the
//!   panel, after hiding ignored entries: `shown_entries`); a capped one ends in a `… N more` row.
//! - Folders first, then files, each by name in any case.
//! - Marks (`git status`): on a file, and on each folder holding marked files (the same mark, or
//!   Modified when they differ). A file under an untracked folder (one entry, `new/`) is Added.
//!   A deleted file, gone from the disk, still shows as a row in its listed folder.
//! - The filter: rows whose name holds the text in any case, with their folders, from every
//!   folder listed so far (open or not).
//! - A nested repo or submodule (a folder holding a `.git`, `set_nested`) is marked by its own
//!   state only; what's inside it isn't this repo's to mark.
//! - Every change that can change the rows moves the tree's `generation` on; `RowCache` builds
//!   the rows again only then (or for another filter), not on every frame.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::git_status::{DirEntry, Mark};

/// The most entries a folder lists; past them, a `… N more` row.
pub const CAP: usize = 10_000;

/// One row of the tree as drawn.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    /// The entry's path from the root; for a `… N more` row, its folder's.
    pub rel: PathBuf,
    /// 0 for the root's entries.
    pub depth: usize,
    pub is_dir: bool,
    /// A folder showing its children.
    pub expanded: bool,
    pub mark: Option<Mark>,
    /// A `… N more` row: how many entries its folder left out.
    pub more: Option<usize>,
}

/// What the panel does after a key or a click.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Want {
    /// List this folder (it was opened).
    List(PathBuf),
    /// Open this file in a tab.
    Open(PathBuf),
}

/// A folder's listing: its shown entries and how many it left out.
#[derive(PartialEq)]
struct Listing {
    entries: Vec<DirEntry>,
    more: usize,
}

/// A generation no tree has had: unique across trees, so rows cached for one tree are never
/// taken for another's.
fn next_generation() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

/// A tree's rows for a filter, built again only when the tree's generation or the filter
/// changed (the selection isn't in them).
#[derive(Default)]
pub struct RowCache {
    key: Option<(u64, String)>,
    rows: Rc<Vec<Row>>,
    builds: usize,
}

impl RowCache {
    pub fn rows(&mut self, tree: &FileTree, filter: &str) -> Rc<Vec<Row>> {
        let fresh = self
            .key
            .as_ref()
            .is_some_and(|(generation, f)| *generation == tree.generation && f == filter);
        if !fresh {
            self.rows = Rc::new(tree.rows(filter));
            self.key = Some((tree.generation, filter.to_string()));
            self.builds += 1;
        }
        self.rows.clone()
    }

    /// How many times the rows were built.
    #[cfg(test)]
    pub fn builds(&self) -> usize {
        self.builds
    }
}

/// Folders first, then files, each by name in any case (then as written, for a stable order).
pub fn sort_entries(entries: &mut [DirEntry]) {
    entries.sort_by_cached_key(|e| (!e.is_dir, e.name.to_lowercase(), e.name.clone()));
}

pub struct FileTree {
    root: PathBuf,
    listings: HashMap<PathBuf, Listing>,
    /// The open folders (never the root, which always is).
    expanded: HashSet<PathBuf>,
    /// git's marks by path, and each folder's from what it holds.
    marks: HashMap<PathBuf, Mark>,
    folder_marks: HashMap<PathBuf, Mark>,
    /// The deleted files (gone from the disk) by folder: rows all the same.
    deleted: HashMap<PathBuf, Vec<PathBuf>>,
    /// Paths git ignores, never shown.
    ignored: HashSet<PathBuf>,
    /// Nested repos and submodules, by the folder they're in.
    nested: HashMap<PathBuf, HashSet<PathBuf>>,
    selected: Option<PathBuf>,
    /// Where the selection was last moved to among the rows: where a move starts when the
    /// selected row has gone and none of its folders shows.
    selected_at: Option<usize>,
    /// Moved on by every change to what the rows show (`RowCache`).
    generation: u64,
}

/// `p` rebuilt from its parts, so `new/` (as git prints a folder) and `new` are one key.
fn key(p: &Path) -> PathBuf {
    p.components().collect()
}

/// A folder's mark from two of its files': the same, or Modified when they differ.
fn combine(a: Mark, b: Mark) -> Mark {
    if a == b { a } else { Mark::Modified }
}

/// `name` precomposed (NFC), as git prints names on macOS; a name read from the disk may be
/// decomposed. ASCII is left as it is. (Foundation's own normalising, in a pool of its own: it
/// runs on the background executor's threads.)
pub fn nfc(name: &str) -> String {
    if name.is_ascii() {
        return name.to_string();
    }
    objc2::rc::autoreleasepool(|_| {
        objc2_foundation::NSString::from_str(name)
            .precomposedStringWithCanonicalMapping()
            .to_string()
    })
}

/// Folder `rel`'s `entries` as the tree shows them: without `.git`, what git ignores
/// (`ignored`, paths from the root) and, outside a repo, dot-folders; then at most `CAP`, with
/// how many were left out. Hidden ones go before the cap, so they never take its places.
pub fn shown_entries(
    rel: &Path,
    entries: Vec<DirEntry>,
    ignored: &HashSet<PathBuf>,
    in_repo: bool,
) -> (Vec<DirEntry>, usize) {
    let mut shown: Vec<DirEntry> = entries
        .into_iter()
        .filter(|e| e.name != ".git")
        .filter(|e| in_repo || !(e.is_dir && e.name.starts_with('.')))
        .filter(|e| !ignored.contains(&rel.join(&e.name)))
        .collect();
    let more = shown.len().saturating_sub(CAP);
    shown.truncate(CAP);
    (shown, more)
}

impl FileTree {
    /// A tree at `root`, nothing listed yet. `revealed` (from the root): the terminal's folder,
    /// opened with its folders and selected.
    pub fn new(root: PathBuf, revealed: Option<PathBuf>) -> Self {
        let revealed = revealed
            .map(|r| key(&r))
            .filter(|r| !r.as_os_str().is_empty());
        let expanded = revealed
            .iter()
            .flat_map(|r| r.ancestors())
            .filter(|a| !a.as_os_str().is_empty())
            .map(Path::to_path_buf)
            .collect();
        Self {
            root,
            listings: HashMap::new(),
            expanded,
            marks: HashMap::new(),
            folder_marks: HashMap::new(),
            deleted: HashMap::new(),
            ignored: HashSet::new(),
            nested: HashMap::new(),
            selected: revealed,
            selected_at: None,
            generation: next_generation(),
        }
    }

    /// The rows may have changed.
    fn changed(&mut self) {
        self.generation = next_generation();
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Moved on by every change to what the rows show: a caller can tell whether an update
    /// changed anything to draw.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Folder `rel` is open, listing `entries` (`more` left out). (The panel opens folders with
    /// `toggle` and hands listings in with `relist`, so one closed while it was listed stays
    /// closed.)
    /// `entries` are sorted here (`sort_entries`).
    #[cfg_attr(not(any(test, feature = "ui-shots")), allow(dead_code))]
    pub fn expand(&mut self, rel: PathBuf, mut entries: Vec<DirEntry>, more: usize) {
        let rel = key(&rel);
        if !rel.as_os_str().is_empty() && self.expanded.insert(rel.clone()) {
            self.changed();
        }
        sort_entries(&mut entries);
        self.relist(rel, entries, more);
    }

    /// Folder `rel`'s listing again (a refresh), in the order given (the panel sorts it off the
    /// UI thread): open or closed as it was. The same listing changes nothing.
    pub fn relist(&mut self, rel: PathBuf, entries: Vec<DirEntry>, more: usize) {
        let listing = Listing { entries, more };
        let rel = key(&rel);
        if self.listings.get(&rel) != Some(&listing) {
            self.listings.insert(rel, listing);
            self.changed();
        }
    }

    /// Close folder `rel`; its listing is kept for opening it again.
    pub fn collapse(&mut self, rel: &Path) {
        if self.expanded.remove(&key(rel)) {
            self.changed();
        }
    }

    /// Open folder `rel` if closed (asking for its listing, which also catches a kept one up),
    /// else close it.
    pub fn toggle(&mut self, rel: &Path) -> Option<Want> {
        let rel = key(rel);
        if rel.as_os_str().is_empty() {
            return None;
        }
        self.changed();
        if self.expanded.remove(&rel) {
            return None;
        }
        self.expanded.insert(rel.clone());
        Some(Want::List(rel))
    }

    /// git's marks (paths from the root, as `git_status::status` gives them). The same marks
    /// change nothing.
    pub fn set_marks(&mut self, marks: HashMap<PathBuf, Mark>) {
        let marks: HashMap<PathBuf, Mark> = marks.into_iter().map(|(p, m)| (key(&p), m)).collect();
        if marks == self.marks {
            return;
        }
        self.marks = marks;
        self.folder_marks.clear();
        self.deleted.clear();
        for (path, &mark) in &self.marks {
            if mark == Mark::Deleted
                && let Some(folder) = path.parent()
            {
                let files = self.deleted.entry(folder.to_path_buf()).or_default();
                files.push(path.clone());
            }
            for folder in path.ancestors().skip(1) {
                if folder.as_os_str().is_empty() {
                    break;
                }
                let entry = self
                    .folder_marks
                    .entry(folder.to_path_buf())
                    .or_insert(mark);
                *entry = combine(*entry, mark);
            }
        }
        self.changed();
    }

    /// Paths (from the root) git ignores: never rows, whatever a listing says. The same set
    /// changes nothing.
    pub fn set_ignored(&mut self, ignored: HashSet<PathBuf>) {
        let ignored: HashSet<PathBuf> = ignored.iter().map(|p| key(p)).collect();
        if ignored != self.ignored {
            self.ignored = ignored;
            self.changed();
        }
    }

    /// The nested repos and submodules (folders holding a `.git`, paths from the root) directly
    /// in `folder`, as its listing found them.
    pub fn set_nested(&mut self, folder: &Path, repos: HashSet<PathBuf>) {
        let folder = key(folder);
        let repos: HashSet<PathBuf> = repos.iter().map(|p| key(p)).collect();
        let before = self.nested.get(&folder);
        if before.map_or(repos.is_empty(), |b| *b == repos) {
            return;
        }
        if repos.is_empty() {
            self.nested.remove(&folder);
        } else {
            self.nested.insert(folder, repos);
        }
        self.changed();
    }

    /// Whether `rel` is a nested repo or submodule.
    fn is_nested(&self, rel: &Path) -> bool {
        rel.parent()
            .and_then(|folder| self.nested.get(folder))
            .is_some_and(|repos| repos.contains(rel))
    }

    /// The open folders, for remembering this root's (`restore_expanded`).
    pub fn expanded_set(&self) -> HashSet<PathBuf> {
        self.expanded.clone()
    }

    /// Open what was open when this root was last shown.
    pub fn restore_expanded(&mut self, expanded: HashSet<PathBuf>) {
        let before = self.expanded.len();
        self.expanded.extend(expanded);
        if self.expanded.len() != before {
            self.changed();
        }
    }

    /// The folders whose children can be seen: the root, and each open folder whose folders are
    /// all open, outermost first (by path).
    pub fn open_folders(&self) -> Vec<PathBuf> {
        let mut open: Vec<PathBuf> = self
            .expanded
            .iter()
            .filter(|f| {
                f.ancestors()
                    .skip(1)
                    .filter(|a| !a.as_os_str().is_empty())
                    .all(|a| self.expanded.contains(a))
            })
            .cloned()
            .collect();
        open.push(PathBuf::new());
        open.sort();
        open
    }

    /// The open folders not listed yet: to list.
    pub fn wanted(&self) -> Vec<PathBuf> {
        self.open_folders()
            .into_iter()
            .filter(|f| !self.listings.contains_key(f))
            .collect()
    }

    pub fn selected(&self) -> Option<&Path> {
        self.selected.as_deref()
    }

    pub fn select(&mut self, rel: &Path) {
        self.selected = Some(key(rel));
        self.selected_at = None;
    }

    /// The row after the selected one (the first with none); the last stays put.
    pub fn select_next(&mut self, filter: &str) {
        self.select_by(filter, 1);
    }

    /// The row before the selected one (the first with none); the first stays put.
    pub fn select_prev(&mut self, filter: &str) {
        self.select_by(filter, -1);
    }

    /// Every row in turn but the `… N more` ones, which aren't entries.
    fn select_by(&mut self, filter: &str, delta: isize) {
        let rows: Vec<Row> = self
            .rows(filter)
            .into_iter()
            .filter(|r| r.more.is_none())
            .collect();
        if rows.is_empty() {
            return;
        }
        let at = self
            .selected
            .as_ref()
            .and_then(|s| rows.iter().position(|r| &r.rel == s));
        let last = rows.len() - 1;
        let to = match at {
            Some(at) => at.saturating_add_signed(delta).min(last),
            // Its row has gone (its folder closed, the file deleted, the filter): its nearest
            // folder that shows, else the row now where it was; the top only with neither.
            None => self
                .selected
                .iter()
                .flat_map(|s| s.ancestors().skip(1))
                .find_map(|a| rows.iter().position(|r| r.rel == a))
                .or(self.selected_at.map(|i| i.min(last)))
                .unwrap_or(0),
        };
        self.selected = Some(rows[to].rel.clone());
        self.selected_at = Some(to);
    }

    /// The selected row as shown with `filter`.
    fn selected_row(&self, filter: &str) -> Option<Row> {
        let selected = self.selected.as_ref()?;
        self.rows(filter)
            .into_iter()
            .find(|r| r.more.is_none() && &r.rel == selected)
    }

    /// Return: open the selected file, or open or close the selected folder.
    pub fn toggle_selected(&mut self, filter: &str) -> Option<Want> {
        let row = self.selected_row(filter)?;
        if row.is_dir {
            self.toggle(&row.rel)
        } else {
            Some(Want::Open(row.rel))
        }
    }

    /// →: a closed folder opens; an open one selects its first row.
    pub fn expand_selected(&mut self, filter: &str) -> Option<Want> {
        let row = self.selected_row(filter)?;
        if !row.is_dir {
            return None;
        }
        if !row.expanded {
            return self.toggle(&row.rel);
        }
        let rows = self.rows(filter);
        let at = rows
            .iter()
            .position(|r| r.rel == row.rel && r.more.is_none())?;
        if let Some(child) = rows
            .get(at + 1)
            .filter(|r| r.depth > row.depth && r.more.is_none())
        {
            self.selected = Some(child.rel.clone());
        }
        None
    }

    /// ←: an open folder closes; anything else selects its folder (none for the root's own).
    pub fn collapse_selected(&mut self, filter: &str) {
        let Some(row) = self.selected_row(filter) else {
            return;
        };
        if row.is_dir && row.expanded && self.expanded.contains(&row.rel) {
            self.collapse(&row.rel);
            return;
        }
        if let Some(parent) = row.rel.parent().filter(|p| !p.as_os_str().is_empty()) {
            self.selected = Some(parent.to_path_buf());
        }
    }

    /// The rows to draw: the open tree, or with a `filter` (any case, blank for none) the
    /// matching entries and their folders.
    pub fn rows(&self, filter: &str) -> Vec<Row> {
        let needle = filter.trim().to_lowercase();
        let mut out = Vec::new();
        if needle.is_empty() {
            self.walk(Path::new(""), 0, &mut out);
        } else {
            self.matching(Path::new(""), 0, &needle, &mut out);
        }
        out
    }

    /// Folder `dir`'s children as (path, is a folder): its listing without the ignored, plus
    /// its deleted files (gone from the disk), in order. None when it isn't listed.
    fn children(&self, dir: &Path) -> Option<Vec<(PathBuf, bool)>> {
        let listing = self.listings.get(dir)?;
        let mut children: Vec<(PathBuf, bool)> = listing
            .entries
            .iter()
            .map(|e| (dir.join(&e.name), e.is_dir))
            .filter(|(rel, _)| !self.ignored.contains(rel))
            .collect();
        let deleted: Vec<(PathBuf, bool)> = self
            .deleted
            .get(dir)
            .into_iter()
            .flatten()
            .filter(|path| !children.iter().any(|(rel, _)| rel == *path))
            .map(|path| (path.clone(), false))
            .collect();
        if !deleted.is_empty() {
            children.extend(deleted);
            children.sort_by_cached_key(|(rel, is_dir)| {
                let name = rel.file_name().unwrap_or_default().to_string_lossy();
                (!is_dir, name.to_lowercase(), name.into_owned())
            });
        }
        Some(children)
    }

    /// Entry `rel`'s mark: its own; a folder's from what it holds; else Added when it is in an
    /// untracked folder (git lists only the folder, `new/`). None inside a nested repo or
    /// submodule: its own state isn't this repo's (`set_nested`), and a modified submodule's
    /// ` M sub` isn't passed down.
    fn mark_of(&self, rel: &Path, is_dir: bool) -> Option<Mark> {
        let mut folders = rel.ancestors().skip(1);
        if folders.clone().any(|a| self.is_nested(a)) {
            return None;
        }
        let own = self
            .marks
            .get(rel)
            .or_else(|| is_dir.then(|| self.folder_marks.get(rel)).flatten());
        own.copied().or_else(|| {
            folders
                .find_map(|a| self.marks.get(a))
                .filter(|&&m| m == Mark::Added)
                .copied()
        })
    }

    fn row(&self, rel: PathBuf, depth: usize, is_dir: bool, expanded: bool) -> Row {
        let mark = self.mark_of(&rel, is_dir);
        Row {
            rel,
            depth,
            is_dir,
            expanded,
            mark,
            more: None,
        }
    }

    fn walk(&self, dir: &Path, depth: usize, out: &mut Vec<Row>) {
        let Some(children) = self.children(dir) else {
            return;
        };
        for (rel, is_dir) in children {
            let open = is_dir && self.expanded.contains(&rel);
            out.push(self.row(rel.clone(), depth, is_dir, open));
            if open {
                self.walk(&rel, depth + 1, out);
            }
        }
        let more = self.listings.get(dir).map_or(0, |l| l.more);
        if more > 0 {
            out.push(Row {
                rel: dir.to_path_buf(),
                depth,
                is_dir: false,
                expanded: false,
                mark: None,
                more: Some(more),
            });
        }
    }

    /// `dir`'s matching rows into `out`; whether there were any.
    fn matching(&self, dir: &Path, depth: usize, needle: &str, out: &mut Vec<Row>) -> bool {
        let Some(children) = self.children(dir) else {
            return false;
        };
        let start = out.len();
        for (rel, is_dir) in children {
            let name = rel.file_name().unwrap_or_default().to_string_lossy();
            let hit = name.to_lowercase().contains(needle);
            if !is_dir {
                if hit {
                    out.push(self.row(rel, depth, false, false));
                }
                continue;
            }
            let mut inner = Vec::new();
            let held = self.matching(&rel, depth + 1, needle, &mut inner);
            if hit || held {
                out.push(self.row(rel, depth, true, held));
                out.extend(inner);
            }
        }
        out.len() > start
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git_status::{DirEntry, Mark};
    use std::collections::{HashMap, HashSet};
    use std::path::{Path, PathBuf};
    use std::rc::Rc;

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    fn dir(name: &str) -> DirEntry {
        DirEntry {
            name: name.into(),
            is_dir: true,
        }
    }

    fn file(name: &str) -> DirEntry {
        DirEntry {
            name: name.into(),
            is_dir: false,
        }
    }

    /// The rows as (rel, depth) for a quick look.
    fn shape(rows: &[Row]) -> Vec<(String, usize)> {
        rows.iter()
            .map(|r| match r.more {
                Some(n) => (format!("… {n} more"), r.depth),
                None => (r.rel.display().to_string(), r.depth),
            })
            .collect()
    }

    /// A repo: src/ (main.rs, util/ (a.rs)), docs/, README.md, Cargo.toml; root and src listed.
    fn tree() -> FileTree {
        let mut t = FileTree::new(p("/repo"), None);
        // Given out of order: `expand` sorts folders first, then by name in any case (the
        // panel's listings come sorted from the background; `relist` keeps their order).
        t.expand(
            p(""),
            vec![
                file("README.md"),
                dir("src"),
                file("Cargo.toml"),
                dir("docs"),
            ],
            0,
        );
        t.expand(p("src"), vec![file("main.rs"), dir("util")], 0);
        t
    }

    #[test]
    fn rows_order_and_depth() {
        let t = tree();
        assert_eq!(
            shape(&t.rows("")),
            vec![
                ("docs".into(), 0),
                ("src".into(), 0),
                ("src/util".into(), 1),
                ("src/main.rs".into(), 1),
                ("Cargo.toml".into(), 0),
                ("README.md".into(), 0),
            ]
        );
        let rows = t.rows("");
        assert!(rows[1].is_dir && rows[1].expanded, "src is open");
        assert!(rows[0].is_dir && !rows[0].expanded, "docs is closed");
        assert!(!rows[3].is_dir && rows[3].more.is_none());
    }

    #[test]
    fn an_unexpanded_folder_has_no_children_listed() {
        let mut t = tree();
        // util has never been expanded: nothing under it, and nothing to list for it yet.
        let rows = t.rows("");
        assert!(
            !rows
                .iter()
                .any(|r| r.rel.parent() == Some(Path::new("src/util")))
        );
        assert!(!t.wanted().contains(&p("src/util")));
        // Expanding it asks for its listing; until that comes, it's open and empty.
        assert_eq!(
            t.toggle(Path::new("src/util")),
            Some(Want::List(p("src/util")))
        );
        assert_eq!(t.wanted(), vec![p("src/util")]);
        assert!(
            !t.rows("")
                .iter()
                .any(|r| r.rel.parent() == Some(Path::new("src/util")))
        );
        t.expand(p("src/util"), vec![file("a.rs")], 0);
        assert!(t.wanted().is_empty());
        assert!(t.rows("").iter().any(|r| r.rel == p("src/util/a.rs")));
    }

    #[test]
    fn marks_propagate_to_parents() {
        let mut t = tree();
        t.expand(p("src/util"), vec![file("a.rs"), file("b.rs")], 0);
        t.set_marks(HashMap::from([
            (p("src/util/a.rs"), Mark::Added),
            (p("src/util/b.rs"), Mark::Added),
            (p("src/main.rs"), Mark::Modified),
            (p("README.md"), Mark::Modified),
        ]));
        let mark = |rel: &str| {
            t.rows("")
                .into_iter()
                .find(|r| r.rel == p(rel))
                .and_then(|r| r.mark)
        };
        assert_eq!(mark("src/util/a.rs"), Some(Mark::Added));
        assert_eq!(mark("src/main.rs"), Some(Mark::Modified));
        // util holds only added files: added; src holds an added and a modified one: modified.
        assert_eq!(mark("src/util"), Some(Mark::Added));
        assert_eq!(mark("src"), Some(Mark::Modified));
        assert_eq!(mark("docs"), None);
        assert_eq!(mark("Cargo.toml"), None);
        // An untracked folder is one entry (`new/`, as git prints it): it and what's in it.
        let mut t = tree();
        t.expand(p("docs"), vec![dir("new")], 0);
        t.expand(p("docs/new"), vec![file("x.md")], 0);
        t.set_marks(HashMap::from([(p("docs/new/"), Mark::Added)]));
        let rows = t.rows("");
        let mark = |rel: &str| rows.iter().find(|r| r.rel == p(rel)).and_then(|r| r.mark);
        assert_eq!(mark("docs"), Some(Mark::Added));
        assert_eq!(mark("docs/new"), Some(Mark::Added));
        assert_eq!(mark("docs/new/x.md"), Some(Mark::Added));
    }

    #[test]
    fn a_deleted_file_shows_in_its_listed_folder() {
        let mut t = tree();
        // Gone from the disk, so not in the listing: still a row, in its place, marked D.
        t.set_marks(HashMap::from([
            (p("src/lib.rs"), Mark::Deleted),
            (p("docs/gone/old.md"), Mark::Deleted),
        ]));
        let rows = t.rows("");
        assert_eq!(
            shape(&rows),
            vec![
                ("docs".into(), 0),
                ("src".into(), 0),
                ("src/util".into(), 1),
                ("src/lib.rs".into(), 1),
                ("src/main.rs".into(), 1),
                ("Cargo.toml".into(), 0),
                ("README.md".into(), 0),
            ]
        );
        assert_eq!(rows[3].mark, Some(Mark::Deleted));
        assert!(!rows[3].is_dir);
        // Its folders are marked: src (holding only it) and docs (closed) as deleted.
        assert_eq!(rows[1].mark, Some(Mark::Deleted));
        assert_eq!(rows[0].mark, Some(Mark::Deleted));
    }

    #[test]
    fn the_filter_keeps_matching_files_plus_their_ancestors() {
        let mut t = tree();
        t.expand(
            p("src/util"),
            vec![file("Main_helpers.rs"), file("b.rs")],
            0,
        );
        // A folder listed once and then closed is still searched.
        t.collapse(Path::new("src/util"));
        assert_eq!(
            shape(&t.rows("MAIN")),
            vec![
                ("src".into(), 0),
                ("src/util".into(), 1),
                ("src/util/Main_helpers.rs".into(), 2),
                ("src/main.rs".into(), 1),
            ]
        );
        assert!(
            t.rows("MAIN")
                .iter()
                .filter(|r| r.is_dir)
                .all(|r| r.expanded)
        );
        // A folder's own name matches too; no match leaves nothing; blank is the tree.
        assert_eq!(shape(&t.rows("doc")), vec![("docs".into(), 0)]);
        assert!(t.rows("zzz").is_empty());
        assert_eq!(t.rows("  "), t.rows(""));
    }

    #[test]
    fn the_more_row() {
        let mut t = FileTree::new(p("/big"), None);
        t.expand(p(""), vec![dir("node_modules"), file("a.js")], 0);
        t.expand(
            p("node_modules"),
            vec![dir("left-pad"), file("x.js")],
            12_345,
        );
        let rows = t.rows("");
        assert_eq!(
            shape(&rows),
            vec![
                ("node_modules".into(), 0),
                ("node_modules/left-pad".into(), 1),
                ("node_modules/x.js".into(), 1),
                ("… 12345 more".into(), 1),
                ("a.js".into(), 0),
            ]
        );
        assert!(!rows[3].is_dir);
        // Not in a filter's rows.
        assert!(t.rows("x").iter().all(|r| r.more.is_none()));
    }

    #[test]
    fn keyboard_moves_skip_nothing_and_clamp_at_the_ends() {
        let mut t = FileTree::new(p("/big"), None);
        t.expand(p(""), vec![dir("lib"), file("a.js"), file("b.js")], 0);
        t.expand(p("lib"), vec![file("x.js")], 3);
        let shown: Vec<PathBuf> = t
            .rows("")
            .into_iter()
            .filter(|r| r.more.is_none())
            .map(|r| r.rel)
            .collect();
        assert_eq!(shown, [p("lib"), p("lib/x.js"), p("a.js"), p("b.js")]);
        // Nothing selected: down starts at the top. Then every row in turn (the `… more` row
        // is not one to select), stopping at the last.
        assert_eq!(t.selected(), None);
        let mut seen = Vec::new();
        for _ in 0..6 {
            t.select_next("");
            seen.push(t.selected().map(Path::to_path_buf).expect("selected"));
        }
        assert_eq!(
            seen,
            [
                p("lib"),
                p("lib/x.js"),
                p("a.js"),
                p("b.js"),
                p("b.js"),
                p("b.js")
            ]
        );
        for _ in 0..5 {
            t.select_prev("");
        }
        assert_eq!(t.selected(), Some(Path::new("lib")), "stops at the first");
        // Within a filter's rows.
        t.select_next("js");
        assert_eq!(t.selected(), Some(Path::new("lib/x.js")));
        t.select_next("js");
        assert_eq!(t.selected(), Some(Path::new("a.js")));
    }

    #[test]
    fn return_opens_a_file_and_toggles_a_folder() {
        let mut t = tree();
        t.select(Path::new("src/main.rs"));
        assert_eq!(t.toggle_selected(""), Some(Want::Open(p("src/main.rs"))));
        t.select(Path::new("src"));
        assert_eq!(t.toggle_selected(""), None, "closed");
        assert!(!t.rows("").iter().any(|r| r.rel == p("src/main.rs")));
        assert_eq!(t.toggle_selected(""), Some(Want::List(p("src"))), "open");
        assert!(t.rows("").iter().any(|r| r.rel == p("src/main.rs")));
    }

    #[test]
    fn right_opens_then_goes_in_and_left_closes_then_goes_up() {
        let mut t = tree();
        t.select(Path::new("docs"));
        // →: a closed folder opens (asking for its listing).
        assert_eq!(t.expand_selected(""), Some(Want::List(p("docs"))));
        t.expand(p("docs"), vec![file("guide.md")], 0);
        // → on an open folder: its first row.
        assert_eq!(t.expand_selected(""), None);
        assert_eq!(t.selected(), Some(Path::new("docs/guide.md")));
        // → on a file: nothing.
        assert_eq!(t.expand_selected(""), None);
        assert_eq!(t.selected(), Some(Path::new("docs/guide.md")));
        // ← on a file: its folder; ← on an open folder: it closes; on a top-level one: stays.
        t.collapse_selected("");
        assert_eq!(t.selected(), Some(Path::new("docs")));
        t.collapse_selected("");
        assert!(!t.expanded_set().contains(Path::new("docs")));
        t.collapse_selected("");
        assert_eq!(t.selected(), Some(Path::new("docs")));
    }

    #[test]
    fn collapse_keeps_the_children_for_a_fast_re_expand() {
        let mut t = tree();
        t.collapse(Path::new("src"));
        assert_eq!(
            shape(&t.rows("")),
            vec![
                ("docs".into(), 0),
                ("src".into(), 0),
                ("Cargo.toml".into(), 0),
                ("README.md".into(), 0),
            ]
        );
        // Opened again: its children at once, from what was listed (a fresh listing is asked
        // for too, to catch up).
        assert_eq!(t.toggle(Path::new("src")), Some(Want::List(p("src"))));
        let rows = t.rows("");
        assert!(rows.iter().any(|r| r.rel == p("src/main.rs")));
        assert!(t.wanted().is_empty(), "nothing missing");
    }

    #[test]
    fn a_new_tree_opens_and_selects_the_terminals_folder() {
        let t = FileTree::new(p("/repo"), Some(p("src/util")));
        assert_eq!(t.root(), Path::new("/repo"));
        assert_eq!(t.selected(), Some(Path::new("src/util")));
        assert_eq!(
            t.expanded_set(),
            HashSet::from([p("src"), p("src/util")]),
            "it and its folders"
        );
        // What to list, outermost first: the root, then down to it.
        assert_eq!(t.wanted(), vec![p(""), p("src"), p("src/util")]);
        assert_eq!(t.open_folders(), vec![p(""), p("src"), p("src/util")]);
    }

    #[test]
    fn re_rooting_resets_the_memory_restores() {
        let mut memory: HashMap<PathBuf, HashSet<PathBuf>> = HashMap::new();
        let mut t = tree();
        t.toggle(Path::new("docs"));
        t.expand(p("docs"), vec![], 0);
        // A cd into another repo: a new tree, nothing open but its root.
        memory.insert(t.root().to_path_buf(), t.expanded_set());
        let mut t = FileTree::new(p("/other"), None);
        assert!(t.expanded_set().is_empty());
        assert!(t.rows("").is_empty(), "nothing listed yet");
        assert_eq!(t.wanted(), vec![p("")]);
        // Back: what was open is open again, and listed afresh.
        memory.insert(t.root().to_path_buf(), t.expanded_set());
        t = FileTree::new(p("/repo"), None);
        t.restore_expanded(memory[Path::new("/repo")].clone());
        assert_eq!(t.expanded_set(), HashSet::from([p("src"), p("docs")]));
        assert_eq!(t.wanted(), vec![p(""), p("docs"), p("src")]);
    }

    #[test]
    fn open_folders_skip_those_inside_a_closed_one() {
        let mut t = tree();
        t.toggle(Path::new("src/util"));
        t.expand(p("src/util"), vec![], 0);
        t.collapse(Path::new("src"));
        // util is remembered open but can't be seen: not listed again.
        assert!(t.expanded_set().contains(Path::new("src/util")));
        assert_eq!(t.open_folders(), vec![p("")]);
    }

    #[test]
    fn ignored_paths_are_hidden() {
        let mut t = tree();
        t.set_ignored(HashSet::from([p("docs"), p("src/main.rs")]));
        let rows: Vec<PathBuf> = t.rows("").into_iter().map(|r| r.rel).collect();
        assert_eq!(
            rows,
            [p("src"), p("src/util"), p("Cargo.toml"), p("README.md")]
        );
    }

    #[test]
    fn ignored_entries_are_dropped_before_the_cap() {
        let mut entries: Vec<DirEntry> = (0..CAP + 5).map(|i| file(&format!("f{i:05}"))).collect();
        entries.insert(0, dir("target"));
        entries.insert(0, dir(".cache"));
        let ignored = HashSet::from([p("sub/target"), p("sub/f00000")]);
        let (shown, more) = shown_entries(Path::new("sub"), entries, &ignored, true);
        // Two ignored, so CAP + 5 + 2 − 2 left: CAP shown, 5 more.
        assert_eq!(shown.len(), CAP);
        assert_eq!(more, 5);
        assert_eq!(shown[0].name, ".cache", "a dot-folder shows in a repo");
        assert_eq!(shown[1].name, "f00001");
    }

    #[test]
    fn outside_a_repo_dot_folders_are_hidden() {
        let entries = vec![dir(".venv"), dir("src"), file(".env"), file("a.py")];
        let (shown, more) = shown_entries(Path::new(""), entries, &HashSet::new(), false);
        let names: Vec<&str> = shown.iter().map(|e| e.name.as_str()).collect();
        // Dot files show; dot-folders (`.venv`) don't.
        assert_eq!(names, ["src", ".env", "a.py"]);
        assert_eq!(more, 0);
    }

    #[test]
    fn names_are_compared_precomposed() {
        // "é" written decomposed (e + U+0301), as some Macs store it, and precomposed, as git
        // prints it.
        assert_eq!(nfc("caf\u{65}\u{301}.txt"), "caf\u{e9}.txt");
        assert_eq!(nfc("plain.txt"), "plain.txt");
    }

    #[test]
    fn rows_are_built_again_only_when_the_tree_or_the_filter_changes() {
        let mut t = tree();
        let mut cache = RowCache::default();
        let first = cache.rows(&t, "");
        assert!(Rc::ptr_eq(&first, &cache.rows(&t, "")), "nothing changed");
        assert_eq!(cache.builds(), 1);
        // The selection isn't in the rows.
        t.select(Path::new("src/main.rs"));
        t.select_next("");
        assert!(Rc::ptr_eq(&first, &cache.rows(&t, "")));
        // A refresh that finds the same: still the same rows.
        t.relist(p("src"), vec![dir("util"), file("main.rs")], 0);
        t.set_marks(HashMap::new());
        t.set_ignored(HashSet::new());
        t.set_nested(Path::new("src"), HashSet::new());
        t.restore_expanded(HashSet::from([p("src")]));
        t.collapse(Path::new("docs")); // already closed
        assert!(
            Rc::ptr_eq(&first, &cache.rows(&t, "")),
            "a refresh with no change"
        );
        assert_eq!(cache.builds(), 1);
        // Each change builds them again: another filter, and each change to the tree.
        cache.rows(&t, "main");
        assert_eq!(cache.builds(), 2);
        let changes: [&dyn Fn(&mut FileTree); 7] = [
            &|t| {
                t.toggle(Path::new("docs"));
            },
            &|t| t.collapse(Path::new("docs")),
            &|t| t.relist(p("src"), vec![file("main.rs")], 0),
            &|t| t.set_marks(HashMap::from([(p("src/main.rs"), Mark::Modified)])),
            &|t| t.set_ignored(HashSet::from([p("README.md")])),
            &|t| t.restore_expanded(HashSet::from([p("docs")])),
            &|t| t.set_nested(Path::new(""), HashSet::from([p("docs")])),
        ];
        for (i, change) in changes.iter().enumerate() {
            let before = cache.rows(&t, "");
            change(&mut t);
            let after = cache.rows(&t, "");
            assert!(!Rc::ptr_eq(&before, &after), "change {i} builds them again");
        }
        // A new tree (another root) never takes an old tree's rows.
        let other = FileTree::new(p("/other"), None);
        assert!(cache.rows(&other, "").is_empty());
    }

    #[test]
    fn a_submodule_or_nested_repo_marks_only_itself() {
        let mut t = tree();
        t.expand(p("docs"), vec![dir("sub"), dir("nest"), dir("plain")], 0);
        t.expand(p("docs/sub"), vec![file("x.rs")], 0);
        t.expand(p("docs/nest"), vec![file("y.rs")], 0);
        t.expand(p("docs/plain"), vec![file("z.md")], 0);
        // docs/sub is a submodule (` M docs/sub`), docs/nest an untracked repo inside this one
        // (`?? docs/nest/`, as git prints a plain untracked folder too): both hold a `.git`.
        t.set_nested(
            Path::new("docs"),
            HashSet::from([p("docs/sub"), p("docs/nest")]),
        );
        t.set_marks(HashMap::from([
            (p("docs/sub"), Mark::Modified),
            (p("docs/nest/"), Mark::Added),
            (p("docs/plain/"), Mark::Added),
        ]));
        let rows = t.rows("");
        let mark = |rel: &str| rows.iter().find(|r| r.rel == p(rel)).and_then(|r| r.mark);
        assert_eq!(mark("docs/sub"), Some(Mark::Modified), "its own state");
        assert_eq!(mark("docs/sub/x.rs"), None, "its contents: not this repo's");
        assert_eq!(mark("docs/nest"), Some(Mark::Added));
        assert_eq!(mark("docs/nest/y.rs"), None);
        assert_eq!(
            mark("docs/plain/z.md"),
            Some(Mark::Added),
            "an untracked folder's"
        );
        // Only an untracked folder passes its mark down: a modified one (not known to be a
        // repo) doesn't.
        let mut t = tree();
        t.expand(p("docs"), vec![dir("sub")], 0);
        t.expand(p("docs/sub"), vec![file("x.rs")], 0);
        t.set_marks(HashMap::from([(p("docs/sub"), Mark::Modified)]));
        let rows = t.rows("");
        assert_eq!(
            rows.iter()
                .find(|r| r.rel == p("docs/sub/x.rs"))
                .and_then(|r| r.mark),
            None
        );
    }

    #[test]
    fn the_selection_stays_near_when_its_row_goes() {
        let mut t = FileTree::new(p("/big"), None);
        let root = vec![
            dir("app"),
            dir("lib"),
            file("a.js"),
            file("b.js"),
            file("c.js"),
        ];
        t.expand(p(""), root, 0);
        t.expand(p("lib"), vec![file("x.js")], 0);
        for _ in 0..3 {
            t.select_next("");
        }
        assert_eq!(t.selected(), Some(Path::new("lib/x.js")));
        // Its folder closed: the next move lands on that folder, not the top.
        t.collapse(Path::new("lib"));
        t.select_next("");
        assert_eq!(t.selected(), Some(Path::new("lib")));
        // b.js (the fourth row) gone from the disk: a move lands where it was.
        t.select_next("");
        t.select_next("");
        assert_eq!(t.selected(), Some(Path::new("b.js")));
        t.relist(
            p(""),
            vec![dir("app"), dir("lib"), file("a.js"), file("c.js")],
            0,
        );
        t.select_prev("");
        assert_eq!(
            t.selected(),
            Some(Path::new("c.js")),
            "the row now in its place"
        );
    }

    #[test]
    fn relist_on_a_closed_folder_keeps_it_closed() {
        let mut t = tree();
        t.collapse(Path::new("src"));
        t.relist(
            p("src"),
            vec![dir("util"), file("main.rs"), file("lib.rs")],
            0,
        );
        assert!(!t.expanded_set().contains(Path::new("src")));
        assert!(
            !t.rows("")
                .iter()
                .any(|r| r.rel.starts_with("src") && r.depth > 0)
        );
    }

    #[test]
    fn reopening_a_folder_wants_its_remembered_open_folders_too() {
        let mut t = tree();
        // util opened, then src closed before util's listing came.
        t.toggle(Path::new("src/util"));
        t.collapse(Path::new("src"));
        assert!(t.wanted().is_empty(), "util can't be seen");
        assert_eq!(t.toggle(Path::new("src")), Some(Want::List(p("src"))));
        assert_eq!(t.wanted(), vec![p("src/util")], "listed with it");
    }

    #[test]
    fn sorting_entries_puts_folders_first_then_names_in_any_case() {
        let mut entries = vec![file("b.rs"), dir("Zed"), file("A.rs"), dir("api")];
        sort_entries(&mut entries);
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["api", "Zed", "A.rs", "b.rs"]);
    }
}
