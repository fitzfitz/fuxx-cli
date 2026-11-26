//! The turn ledger popover's files (the 2026-11-25 chores, mockup A: "make it 20x better"): what
//! git and the disk say of the files a turn touched (`read_facts`, run off the UI thread), and
//! the rows the popover draws, grouped by folder (pure). A path only named in a script's text
//! (`icons/shapes/x.svg` written by a script run in `assets/`) is no file here, so it is dropped:
//! each file shows once, and folders never do.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::agent_ledger::{Changed, Ledger};

/// How a file stands against the last commit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileStatus {
    Added,
    Modified,
    Deleted,
    /// There, and git says nothing of it (or no git to ask): a file only read.
    Unchanged,
}

/// What git and the disk say of a path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileFacts {
    pub status: FileStatus,
    pub plus: Option<u32>,
    pub minus: Option<u32>,
    pub is_file: bool,
}

/// How the turn touched a file: changed it (its calls), a script did, or it only read it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum How {
    Changed,
    Script,
    Read,
}

/// A file's row: its name, how the turn touched it, its status and its +/− (none: not known).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileRow {
    pub path: PathBuf,
    pub name: String,
    pub how: How,
    pub status: FileStatus,
    pub plus: Option<u32>,
    pub minus: Option<u32>,
}

/// The popover's files (chore 36): every file the turn touched, under its folder.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FilesList {
    pub inside: Vec<Group>,
}

/// The rows under one folder (relative to the tab's folder; `""`: its top).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Group {
    pub folder: String,
    pub rows: Vec<FileRow>,
}

/// What git and the disk say of `paths` (absolute, under `root`, the tab's folder). Blocking:
/// run off the UI thread. A path with nothing there and nothing in git gets no facts; outside a
/// repo (or without git) a file there is `Modified` with no counts. A new file's `+` is its lines.
pub fn read_facts(root: &Path, paths: &[PathBuf]) -> HashMap<PathBuf, FileFacts> {
    let in_git = crate::git_branch::find_git().and_then(|git| git_facts(&git, root, paths));
    let (status, counts) = in_git.unwrap_or_default();
    let mut out = HashMap::new();
    for path in paths {
        let meta = std::fs::metadata(path).ok();
        let is_file = meta.as_ref().is_some_and(|m| m.is_file());
        let status = match status.get(path) {
            Some(s) => *s,
            None if meta.is_some() => FileStatus::Unchanged,
            None => continue,
        };
        let (plus, minus) = match (counts.get(path), status) {
            (Some(&(p, m)), _) => (p, m),
            (None, FileStatus::Added) if is_file => (line_count(path), Some(0)),
            _ => (None, None),
        };
        out.insert(
            path.clone(),
            FileFacts {
                status,
                plus,
                minus,
                is_file,
            },
        );
    }
    out
}

type Statuses = HashMap<PathBuf, FileStatus>;
type Counts = HashMap<PathBuf, (Option<u32>, Option<u32>)>;

/// Git's word on `paths`: each one's status (`status --porcelain`) and +/− against `HEAD`
/// (`diff --numstat`; none before the first commit). None: `root` is in no repo.
fn git_facts(git: &Path, root: &Path, paths: &[PathBuf]) -> Option<(Statuses, Counts)> {
    use crate::git_branch::run_git;
    // Git names paths from the repo's top; `root` is `prefix` below it.
    let prefix = run_git(git, root, &["rev-parse", "--show-prefix"])?;
    let prefix = prefix.trim_end_matches('\n');
    let specs: Vec<String> = paths
        .iter()
        .filter_map(|p| p.strip_prefix(root).ok())
        .map(|p| p.to_string_lossy().into_owned())
        .filter(|p| !p.is_empty())
        .collect();
    if specs.is_empty() {
        return Some(Default::default());
    }
    let back = |from_top: &str| from_top.strip_prefix(prefix).map(|rest| root.join(rest));
    let with = |head: &[&str]| -> Vec<String> {
        head.iter()
            .map(|s| s.to_string())
            .chain(["--".to_string()])
            .chain(specs.iter().cloned())
            .collect()
    };
    let args = with(&["status", "--porcelain=v1", "-z", "--untracked-files=all"]);
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let mut statuses = Statuses::new();
    if let Some(out) = run_git(git, root, &args) {
        let mut entries = out.split('\0');
        while let Some(entry) = entries.next() {
            if entry.len() < 4 {
                continue;
            }
            let (xy, path) = entry.split_at(3);
            let xy = xy.trim_end();
            // A rename's old path follows it.
            if xy.starts_with('R') || xy.starts_with('C') {
                entries.next();
            }
            let status = if xy == "??" || xy.contains('A') {
                FileStatus::Added
            } else if xy.contains('D') {
                FileStatus::Deleted
            } else {
                FileStatus::Modified
            };
            if let Some(path) = back(path) {
                statuses.insert(path, status);
            }
        }
    }
    let args = with(&["diff", "--numstat", "-z", "HEAD"]);
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let mut counts = Counts::new();
    if let Some(out) = run_git(git, root, &args) {
        for entry in out.split('\0') {
            let mut parts = entry.splitn(3, '\t');
            let (Some(plus), Some(minus), Some(path)) = (parts.next(), parts.next(), parts.next())
            else {
                continue;
            };
            if let Some(path) = back(path) {
                counts.insert(path, (plus.parse().ok(), minus.parse().ok()));
            }
        }
    }
    Some((statuses, counts))
}

/// A text file's lines (a new file's `+`); none for one that can't be read as text.
fn line_count(path: &Path) -> Option<u32> {
    let text = std::fs::read_to_string(path).ok()?;
    u32::try_from(text.lines().count()).ok()
}

/// The files `ledger`'s turn touched, as rows under their folders (each folder once, the top's
/// first, then case-blind; files by name): those it changed, those scripts touched and those it
/// only read, all in one grouping (chore 36: grouped apart, a folder came back once per kind).
pub fn files_list(ledger: &Ledger, facts: &HashMap<PathBuf, FileFacts>, cwd: &Path) -> FilesList {
    let (scripted, read) = (
        kept_paths(&ledger.scripted, facts),
        kept_paths(&ledger.read, facts),
    );
    let rows = changed_rows(&ledger.changed, facts)
        .chain(plain_rows(&scripted, facts, How::Script))
        .chain(plain_rows(&read, facts, How::Read));
    FilesList {
        inside: grouped(rows, cwd),
    }
}

/// The files the turn changed, as rows. The ledger's own +/− win; `facts` fill what it doesn't
/// know, and give each its status.
fn changed_rows<'a>(
    changed: &'a [Changed],
    facts: &'a HashMap<PathBuf, FileFacts>,
) -> impl Iterator<Item = FileRow> + 'a {
    // A folder a command wrote into (`cp x dist/`) is no file (the review's Minor 7).
    let file = |c: &&Changed| {
        facts
            .get(&c.path)
            .is_none_or(|f| f.is_file || f.status == FileStatus::Deleted)
    };
    changed.iter().filter(file).map(|c| {
        let fact = facts.get(&c.path);
        let known = c.plus.is_some() || c.minus.is_some();
        let (plus, minus) = match fact {
            Some(f) if !known => (f.plus, f.minus),
            _ => (c.plus, c.minus),
        };
        FileRow {
            path: c.path.clone(),
            name: file_name(&c.path),
            how: How::Changed,
            // The turn changed it: committed since, it still reads M.
            status: match fact.map(|f| f.status) {
                None | Some(FileStatus::Unchanged) => FileStatus::Modified,
                Some(s) => s,
            },
            plus,
            minus,
        }
    })
}

/// `paths` (touched by scripts, or read) that are files, or files gone: a folder, or a path
/// named in a script's text with nothing there, is dropped. In their order, each once.
pub fn kept_paths(paths: &[PathBuf], facts: &HashMap<PathBuf, FileFacts>) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    for p in paths {
        let keep = facts
            .get(p)
            .is_some_and(|f| f.is_file || f.status == FileStatus::Deleted);
        if keep && !out.contains(p) {
            out.push(p.clone());
        }
    }
    out
}

/// `paths` as rows touched `how`, with no counts.
fn plain_rows<'a>(
    paths: &'a [PathBuf],
    facts: &'a HashMap<PathBuf, FileFacts>,
    how: How,
) -> impl Iterator<Item = FileRow> + 'a {
    paths.iter().map(move |p| FileRow {
        path: p.clone(),
        name: file_name(p),
        how,
        status: facts.get(p).map_or(FileStatus::Unchanged, |f| f.status),
        plus: None,
        minus: None,
    })
}

/// How many files the turn changed, and their +/− summed (unknown counts as 0).
pub fn total(groups: &[Group]) -> (usize, u32, u32) {
    groups
        .iter()
        .flat_map(|g| &g.rows)
        .filter(|r| r.how == How::Changed)
        .fold((0, 0, 0), |(n, p, m), r| {
            (n + 1, p + r.plus.unwrap_or(0), m + r.minus.unwrap_or(0))
        })
}

fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.to_string_lossy().into_owned(),
        |n| n.to_string_lossy().into_owned(),
    )
}

/// Rows under their folder relative to `cwd` (outside it: the folder as it is), each folder
/// once, in order case-blind with the top's (`""`) first, files by name.
fn grouped(rows: impl Iterator<Item = FileRow>, cwd: &Path) -> Vec<Group> {
    let mut by: std::collections::BTreeMap<(String, String), Vec<FileRow>> = Default::default();
    for row in rows {
        let parent = row.path.parent().unwrap_or(Path::new(""));
        let folder = parent
            .strip_prefix(cwd)
            .unwrap_or(parent)
            .to_string_lossy()
            .into_owned();
        by.entry((folder.to_lowercase(), folder))
            .or_default()
            .push(row);
    }
    by.into_iter()
        .map(|((_, folder), mut rows)| {
            rows.sort_by(|a, b| a.name.cmp(&b.name));
            Group { folder, rows }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process::Command;

    const CWD: &str = "/w";

    fn p(s: &str) -> PathBuf {
        Path::new(CWD).join(s)
    }

    fn changed(path: &str, plus: Option<u32>, minus: Option<u32>) -> Changed {
        Changed {
            path: p(path),
            plus,
            minus,
        }
    }

    fn fact(status: FileStatus, plus: Option<u32>, minus: Option<u32>) -> FileFacts {
        FileFacts {
            status,
            plus,
            minus,
            is_file: status != FileStatus::Deleted,
        }
    }

    fn changed_groups(
        c: &[Changed],
        facts: &HashMap<PathBuf, FileFacts>,
        cwd: &Path,
    ) -> Vec<Group> {
        grouped(changed_rows(c, facts), cwd)
    }

    fn plain_groups(
        paths: &[PathBuf],
        facts: &HashMap<PathBuf, FileFacts>,
        cwd: &Path,
    ) -> Vec<Group> {
        grouped(plain_rows(paths, facts, How::Read), cwd)
    }

    fn names(groups: &[Group]) -> Vec<(String, Vec<String>)> {
        groups
            .iter()
            .map(|g| {
                (
                    g.folder.clone(),
                    g.rows.iter().map(|r| r.name.clone()).collect(),
                )
            })
            .collect()
    }

    #[test]
    fn groups_sort_by_folder_and_name() {
        let c = [
            changed("src/b.rs", None, None),
            changed("assets/x.svg", None, None),
            changed("src/a.rs", None, None),
            changed("README.md", None, None),
        ];
        let g = changed_groups(&c, &HashMap::new(), Path::new(CWD));
        let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        assert_eq!(
            names(&g),
            [
                (String::new(), s(&["README.md"])),
                ("assets".into(), s(&["x.svg"])),
                ("src".into(), s(&["a.rs", "b.rs"])),
            ]
        );
    }

    #[test]
    fn ledger_counts_win_and_facts_fill_gaps() {
        let c = [
            changed("src/a.rs", Some(3), Some(1)),
            changed("src/b.rs", None, None),
        ];
        let facts = HashMap::from([
            (p("src/a.rs"), fact(FileStatus::Modified, Some(9), Some(9))),
            (p("src/b.rs"), fact(FileStatus::Modified, Some(4), Some(2))),
        ]);
        let g = changed_groups(&c, &facts, Path::new(CWD));
        let counts: Vec<_> = g[0].rows.iter().map(|r| (r.plus, r.minus)).collect();
        assert_eq!(counts, [(Some(3), Some(1)), (Some(4), Some(2))]);
    }

    /// Your screenshot (2026-11-25): `icons/shapes/x.svg` and `icons/lucide` beside the real
    /// `assets/icons/shapes/x.svg`.
    #[test]
    fn script_named_folders_and_missing_files_are_dropped() {
        let real = p("assets/icons/shapes/x.svg");
        let folder = p("icons/lucide");
        let missing = p("icons/shapes/x.svg");
        let facts = HashMap::from([
            (real.clone(), fact(FileStatus::Added, Some(4), Some(0))),
            (
                folder.clone(),
                FileFacts {
                    status: FileStatus::Modified,
                    plus: None,
                    minus: None,
                    is_file: false,
                },
            ),
        ]);
        assert_eq!(kept_paths(&[missing, folder, real.clone()], &facts), [real]);
    }

    #[test]
    fn a_deleted_file_is_kept() {
        let gone = p("old.rs");
        let facts = HashMap::from([(gone.clone(), fact(FileStatus::Deleted, None, None))]);
        assert_eq!(kept_paths(std::slice::from_ref(&gone), &facts), [gone]);
    }

    /// The review's Minor 7 (your screenshot's `icons/lucide`): a folder a command wrote into
    /// (`cp x dist/`) never shows as a changed file; a path not read yet stays.
    #[test]
    fn a_changed_folder_is_left_out() {
        let c = [changed("dist", None, None), changed("src/a.rs", None, None)];
        let facts = HashMap::from([(
            p("dist"),
            FileFacts {
                status: FileStatus::Unchanged,
                plus: None,
                minus: None,
                is_file: false,
            },
        )]);
        let g = changed_groups(&c, &facts, Path::new(CWD));
        assert_eq!(names(&g), [("src".into(), vec!["a.rs".to_string()])]);
    }

    /// The review's Important 4: a file git says nothing about is unchanged (a Read row gets no
    /// badge); one the turn changed still reads M.
    #[test]
    fn a_clean_file_is_unchanged_but_a_changed_one_reads_m() {
        let facts = HashMap::from([(p("src/a.rs"), fact(FileStatus::Unchanged, None, None))]);
        let g = changed_groups(&[changed("src/a.rs", None, None)], &facts, Path::new(CWD));
        assert_eq!(g[0].rows[0].status, FileStatus::Modified);
        let read = plain_groups(&[p("src/a.rs")], &facts, Path::new(CWD));
        assert_eq!(read[0].rows[0].status, FileStatus::Unchanged);
    }

    #[test]
    fn total_sums_rows() {
        let row = |plus, minus| FileRow {
            path: p("a"),
            name: "a".into(),
            how: How::Changed,
            status: FileStatus::Modified,
            plus,
            minus,
        };
        let g = [Group {
            folder: String::new(),
            rows: vec![row(Some(3), Some(1)), row(Some(4), None)],
        }];
        assert_eq!(total(&g), (2, 7, 1));
    }

    fn git_in(git: &Path, dir: &Path, args: &[&str]) {
        let out = Command::new(git)
            .arg("-C")
            .arg(dir)
            .args(["-c", "user.name=t", "-c", "user.email=t@t"])
            .args(["-c", "commit.gpgsign=false"])
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}");
    }

    fn fresh(name: &str) -> PathBuf {
        let dir = crate::test_temp::dir(name);
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn facts_in_a_repo() {
        let Some(git) = crate::git_branch::find_git() else {
            return;
        };
        let root = fresh("fuxx-test-changed-facts");
        git_in(&git, &root, &["init", "-q", "-b", "main"]);
        fs::write(root.join("a.txt"), "1\n2\n3\n").unwrap();
        fs::write(root.join("gone.txt"), "x\n").unwrap();
        git_in(&git, &root, &["add", "-A"]);
        git_in(&git, &root, &["commit", "-q", "-m", "first"]);
        fs::write(root.join("a.txt"), "1\nTWO\n3\n4\n").unwrap();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/new.txt"), "a\nb\nc\n").unwrap();
        fs::remove_file(root.join("gone.txt")).unwrap();
        fs::write(root.join("clean.txt"), "c\n").unwrap();
        git_in(&git, &root, &["add", "clean.txt"]);
        git_in(&git, &root, &["commit", "-q", "-m", "clean"]);
        let paths = [
            root.join("a.txt"),
            root.join("src/new.txt"),
            root.join("gone.txt"),
            root.join("clean.txt"),
        ];
        let f = read_facts(&root, &paths);
        assert_eq!(
            f[&paths[3]],
            fact(FileStatus::Unchanged, None, None),
            "git says nothing"
        );
        assert_eq!(f[&paths[0]], fact(FileStatus::Modified, Some(2), Some(1)));
        assert_eq!(f[&paths[1]], fact(FileStatus::Added, Some(3), Some(0)));
        assert_eq!(f[&paths[2]].status, FileStatus::Deleted);
        assert!(!f[&paths[2]].is_file);
    }

    #[test]
    fn facts_without_git() {
        let root = fresh("fuxx-test-changed-nogit");
        fs::write(root.join("a.txt"), "1\n").unwrap();
        fs::create_dir_all(root.join("dir")).unwrap();
        let paths = [
            root.join("a.txt"),
            root.join("dir"),
            root.join("missing.txt"),
        ];
        let f = read_facts(&root, &paths);
        assert_eq!(
            f[&paths[0]],
            fact(FileStatus::Unchanged, None, None),
            "nothing to tell"
        );
        assert!(!f[&paths[1]].is_file, "a folder is no file");
        assert!(!f.contains_key(&paths[2]), "nothing there, nothing known");
    }

    #[test]
    fn facts_without_head() {
        let Some(git) = crate::git_branch::find_git() else {
            return;
        };
        let root = fresh("fuxx-test-changed-nohead");
        git_in(&git, &root, &["init", "-q", "-b", "main"]);
        fs::write(root.join("a.txt"), "1\n2\n").unwrap();
        let paths = [root.join("a.txt")];
        let f = read_facts(&root, &paths);
        assert_eq!(f[&paths[0]], fact(FileStatus::Added, Some(2), Some(0)));
    }

    fn ledger(changed: &[&str], scripted: &[&str], read: &[&str]) -> Ledger {
        Ledger {
            running: None,
            changed: changed
                .iter()
                .map(|c| self::changed(c, None, None))
                .collect(),
            scripted: scripted.iter().map(|s| p(s)).collect(),
            read: read.iter().map(|r| p(r)).collect(),
            checks: Vec::new(),
            scopes: Vec::new(),
            passed: 0,
            total: 0,
            failing: None,
            checking: false,
        }
    }

    fn facts_of(paths: &[&str]) -> HashMap<PathBuf, FileFacts> {
        paths
            .iter()
            .map(|s| (p(s), fact(FileStatus::Modified, None, None)))
            .collect()
    }

    /// Chore 36, your picture: `src` (agent_tab.rs), `src/browser` (probe.rs), then `src`
    /// again (agent_box.rs, which a script edited): each kind of touch was grouped on its own.
    #[test]
    fn the_picture_groups_src_once() {
        let l = ledger(
            &["src/agent_tab.rs", "src/browser/probe.rs"],
            &["src/agent_box.rs"],
            &["src/agent_lanes.rs"],
        );
        let facts = facts_of(&["src/agent_box.rs", "src/agent_lanes.rs"]);
        let list = files_list(&l, &facts, Path::new(CWD));
        let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        assert_eq!(
            names(&list.inside),
            [
                (
                    "src".into(),
                    s(&["agent_box.rs", "agent_lanes.rs", "agent_tab.rs"])
                ),
                ("src/browser".into(), s(&["probe.rs"])),
            ]
        );
        let how: Vec<How> = list.inside[0].rows.iter().map(|r| r.how).collect();
        assert_eq!(how, [How::Script, How::Read, How::Changed]);
        // Only the changed count.
        assert_eq!(total(&list.inside).0, 2);
    }

    /// Folders sort case-blind (`assets` before `Docs`), the top's first.
    #[test]
    fn folders_sort_case_blind() {
        let l = ledger(&["Docs/a.md", "assets/x.svg", "README.md"], &[], &[]);
        let list = files_list(&l, &HashMap::new(), Path::new(CWD));
        let folders: Vec<&str> = list.inside.iter().map(|g| g.folder.as_str()).collect();
        assert_eq!(folders, ["", "assets", "Docs"]);
    }
}
