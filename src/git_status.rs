//! Git and folder lookups for the right sidebar and file tabs (M11g): the repo root, the status
//! marks, the ignored files, a file's changed lines against HEAD, and a folder's listing.
//!
//! git runs the way `git_branch` runs it (`run_git`: no prompt, no index lock, 2 s cap, no
//! terminal). These are blocking: callers run them on the background executor. Read-only.
//!
//! Status marks (porcelain v1, `-z`; X is the index column, Y the work tree's): `M` or `T` in
//! either column is Modified; `A` or `??` is Added; `D` is Deleted; `R` or `C` is Added for the
//! new path (the old path of a rename is not marked). A merge conflict (a `U` in either column,
//! or `AA`/`DD`) is Modified: a change in progress. Anything else is Modified; `!!` is skipped.
//!
//! Paths are returned as git prints them (`-z`, so never quoted), not normalised: on macOS git
//! stores and prints precomposed (NFC) names (`core.precomposeunicode`), so a caller comparing
//! them with on-disk names (which may be decomposed) should normalise both.

use std::collections::{HashMap, HashSet};
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use crate::git_branch::{Ran, run_git, run_git_bytes, run_git_outcome, run_git_stdin_outcome};
use crate::highlight::{PLAIN_ABOVE, REFUSE_ABOVE, Viewable, classify};

/// A file's git state, as the tree marks it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mark {
    Modified,
    Added,
    Deleted,
}

/// A line's state against HEAD, as the change bar shows it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LineChange {
    Added,
    Changed,
    /// Lines were removed just after this (1-based) line; 0 is above the first line.
    DeletedAfter,
}

/// A folder's entry, for the tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DirEntry {
    pub name: String,
    pub is_dir: bool,
}

/// The repo's top folder holding `dir`, or none outside a repo (or when git fails or is slow).
pub fn repo_root(git: &Path, dir: &Path) -> Option<PathBuf> {
    let out = run_git(git, dir, &["rev-parse", "--show-toplevel"])?;
    let top = out.trim_end_matches('\n');
    (!top.is_empty()).then(|| PathBuf::from(top))
}

/// `repo_root`, telling "not a repo" (git says so: `Some(None)`) from no answer (git failed,
/// took longer than the cap, or couldn't run: None, to ask again later).
pub fn repo_root_at(git: &Path, dir: &Path) -> Option<Option<PathBuf>> {
    match run_git_outcome(git, dir, &["rev-parse", "--show-toplevel"]) {
        Ran::Exited(Some(0), out) => {
            let top = out.trim_end_matches('\n');
            Some((!top.is_empty()).then(|| PathBuf::from(top)))
        }
        // git's "fatal: not a git repository".
        Ran::Exited(Some(128), _) => Some(None),
        _ => None,
    }
}

/// The size in bytes of `rel`'s blob in HEAD (`git cat-file -s HEAD:<rel>`): `Some(None)` when
/// it isn't there (untracked, or no commits yet), None when git failed or took longer than the
/// cap. Blocking.
pub fn head_size(git: &Path, root: &Path, rel: &Path) -> Option<Option<u64>> {
    let object = format!("HEAD:{}", rel.to_string_lossy());
    match run_git_outcome(git, root, &["cat-file", "-s", &object]) {
        Ran::Exited(Some(0), out) => out.trim().parse().ok().map(Some),
        Ran::Exited(Some(_), _) => Some(None),
        _ => None,
    }
}

/// `git status --porcelain=v1 -z`'s entries (`XY path\0`; a rename or copy is followed by its
/// old path, which is skipped) as marks; see the module note for the mapping.
pub fn parse_porcelain(z: &[u8]) -> Vec<(PathBuf, Mark)> {
    let mut out = Vec::new();
    let mut entries = z.split(|&b| b == 0);
    while let Some(entry) = entries.next() {
        if entry.len() < 4 || entry[2] != b' ' {
            continue;
        }
        let (x, y) = (entry[0], entry[1]);
        let moved = matches!(x, b'R' | b'C') || matches!(y, b'R' | b'C');
        if moved {
            entries.next();
        }
        let mark = match (x, y) {
            (b'!', b'!') => continue,
            (b'?', b'?') => Mark::Added,
            _ if moved => Mark::Added,
            (b'U', _) | (_, b'U') | (b'A', b'A') | (b'D', b'D') => Mark::Modified,
            (b'D', _) | (_, b'D') => Mark::Deleted,
            (b'A', _) => Mark::Added,
            _ => Mark::Modified,
        };
        out.push((PathBuf::from(OsStr::from_bytes(&entry[3..])), mark));
    }
    out
}

/// The repo's changed files (paths relative to `root`, a folder of untracked files as one
/// entry); the caller marks their parent folders. None when git fails or takes longer than the
/// cap: no answer, not "nothing changed" (the caller keeps the marks it has).
pub fn status(git: &Path, root: &Path) -> Option<HashMap<PathBuf, Mark>> {
    run_git(git, root, &["status", "--porcelain=v1", "-z"])
        .map(|out| parse_porcelain(out.as_bytes()).into_iter().collect())
}

/// Which of `rel` (paths relative to `root`) git ignores, in one `check-ignore --stdin` call
/// (`ignored_answer`). None when git fails or takes longer than the cap.
pub fn ignored(git: &Path, root: &Path, rel: &[PathBuf]) -> Option<HashSet<PathBuf>> {
    if rel.is_empty() {
        return Some(HashSet::new());
    }
    let mut input = Vec::new();
    for path in rel {
        input.extend_from_slice(path.as_os_str().as_bytes());
        input.push(0);
    }
    ignored_answer(run_git_stdin_outcome(
        git,
        root,
        &["check-ignore", "-z", "--stdin"],
        &input,
    ))
}

/// `check-ignore -z`'s answer: exit 0 lists the ignored paths; exit 1 is its normal "none of
/// them are"; anything else (an error, the cap, a signal) is no answer.
pub fn ignored_answer(ran: Ran) -> Option<HashSet<PathBuf>> {
    match ran {
        Ran::Exited(Some(0), out) => Some(
            out.split('\0')
                .filter(|path| !path.is_empty())
                .map(PathBuf::from)
                .collect(),
        ),
        Ran::Exited(Some(1), _) => Some(HashSet::new()),
        _ => None,
    }
}

/// A yes-or-no question to git (`rev-parse --verify -q`, `ls-files --error-unmatch`): exit 0
/// is yes, 1 is no; anything else (an error, the cap) is no answer.
fn yes_or_no(ran: Ran) -> Option<bool> {
    match ran {
        Ran::Exited(Some(0), _) => Some(true),
        Ran::Exited(Some(1), _) => Some(false),
        _ => None,
    }
}

/// A `-U0` diff's hunks as per-line changes (1-based new line numbers). For `@@ -a,b +c,d @@`
/// (a count left out is 1): b = 0 is d Added lines from c; d = 0 is a deletion after line c;
/// else min(b, d) Changed lines from c, then the rest of d Added.
pub fn parse_hunks(diff: &str) -> Vec<(usize, LineChange)> {
    fn range(part: &str) -> Option<(usize, usize)> {
        let part = part.get(1..)?;
        match part.split_once(',') {
            Some((start, count)) => Some((start.parse().ok()?, count.parse().ok()?)),
            None => Some((part.parse().ok()?, 1)),
        }
    }
    let mut out = Vec::new();
    for line in diff.lines().filter(|l| l.starts_with("@@ ")) {
        let mut parts = line.split(' ').skip(1);
        let (Some(old), Some(new)) = (parts.next(), parts.next()) else {
            continue;
        };
        let (Some((_, b)), Some((c, d))) = (range(old), range(new)) else {
            continue;
        };
        if d == 0 {
            out.push((c, LineChange::DeletedAfter));
            continue;
        }
        let changed = b.min(d);
        for i in 0..d {
            let kind = if i < changed {
                LineChange::Changed
            } else {
                LineChange::Added
            };
            out.push((c + i, kind));
        }
    }
    out
}

/// Whether `root` is a work tree (else None), has a `HEAD`, and tracks `rel` (in the index, or
/// in HEAD: a staged deletion): the questions
/// `line_changes` and `file_diff` share. None too when git fails or takes longer than the cap.
/// Names are literal, not pathspecs (`*`, `[`, a leading `:`).
fn head_and_tracked(git: &Path, root: &Path, rel: &Path) -> Option<(bool, bool)> {
    let inside = run_git(git, root, &["rev-parse", "--is-inside-work-tree"])?;
    (inside.trim() == "true").then_some(())?;
    let path = rel.to_string_lossy();
    let has_head = yes_or_no(run_git_outcome(
        git,
        root,
        &["rev-parse", "--verify", "-q", "HEAD"],
    ))?;
    let in_index = yes_or_no(run_git_outcome(
        git,
        root,
        &[
            "--literal-pathspecs",
            "ls-files",
            "--error-unmatch",
            "--",
            &path,
        ],
    ))?;
    // A staged deletion (`git rm`, a `git mv`'s old path) is out of the index but still in
    // HEAD: tracked, so its diff is its old lines, all removed.
    let tracked = in_index || (has_head && head_size(git, root, rel)?.is_some());
    Some((has_head, tracked))
}

/// `rel`'s (relative to `root`) lines changed on disk against HEAD. An untracked file, or any
/// file in a repo with no commits, has every line Added (only a plain file, at most
/// `REFUSE_ABOVE` bytes: else no bars). None outside a repo, or when git fails or takes longer
/// than the cap: no answer, not "nothing changed" (the caller keeps the bars it has).
pub fn line_changes(git: &Path, root: &Path, rel: &Path) -> Option<Vec<(usize, LineChange)>> {
    let (has_head, tracked) = head_and_tracked(git, root, rel)?;
    let path = rel.to_string_lossy();
    if !has_head || !tracked {
        return Some(all_added(&root.join(rel)));
    }
    let diff = run_git(
        git,
        root,
        &[
            "--literal-pathspecs",
            "diff",
            "HEAD",
            "-U0",
            "--no-color",
            "--no-ext-diff",
            "--",
            &path,
        ],
    )?;
    Some(parse_hunks(&diff))
}

/// What git says about one file's change against the last commit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FileDiff {
    /// `git diff HEAD`'s output (empty: the file is unchanged).
    Unified(String),
    /// Untracked, or no commits yet: the whole file is new (the caller reads it from disk).
    AllAdded,
    Binary,
}

/// `rel`'s (relative to `root`) whole diff against HEAD with `context` lines around each hunk.
/// Blocking: call it from background work. None outside a repo, or when git fails or takes
/// longer than the cap (no answer, not "unchanged"). Non-UTF-8 text is decoded lossily. A `Binary files …` line with no hunk is
/// `Binary`.
pub fn file_diff(git: &Path, root: &Path, rel: &Path, context: usize) -> Option<FileDiff> {
    let (has_head, tracked) = head_and_tracked(git, root, rel)?;
    if !has_head || !tracked {
        return Some(FileDiff::AllAdded);
    }
    let path = rel.to_string_lossy();
    let unified = format!("-U{context}");
    // Bytes, decoded lossily: a Latin-1 byte in the file must not hide its whole diff.
    // `--no-textconv`: the rows are the file's own lines, not a converter's text.
    let out = run_git_bytes(
        git,
        root,
        &[
            "--literal-pathspecs",
            "diff",
            "HEAD",
            &unified,
            "--no-color",
            "--no-ext-diff",
            "--no-textconv",
            "--",
            &path,
        ],
    )
    .success()?;
    let out = String::from_utf8_lossy(&out).into_owned();
    let hunks = out.lines().any(|l| l.starts_with("@@ "));
    if !hunks && out.lines().any(|l| l.starts_with("Binary files ")) {
        return Some(FileDiff::Binary);
    }
    Some(FileDiff::Unified(out))
}

/// Every line of the file at `path` Added: only a plain file (never a FIFO or a device, which
/// could block), read through a bound; past `REFUSE_ABOVE` bytes, or unreadable, no bars.
fn all_added(path: &Path) -> Vec<(usize, LineChange)> {
    use std::io::Read;
    if !path.is_file() {
        return Vec::new();
    }
    let mut bytes = Vec::new();
    let read = std::fs::File::open(path)
        .and_then(|file| file.take(REFUSE_ABOVE + 1).read_to_end(&mut bytes));
    if read.is_err() || bytes.len() as u64 > REFUSE_ABOVE {
        return Vec::new();
    }
    let lines = bytes.iter().filter(|&&b| b == b'\n').count()
        + usize::from(bytes.last().is_some_and(|&b| b != b'\n'));
    (1..=lines).map(|n| (n, LineChange::Added)).collect()
}

/// A changed file's line counts.
#[allow(dead_code)] // wired by the Changes panel (M11h task 4)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Counts {
    Lines {
        added: u32,
        removed: u32,
    },
    Binary,
    /// Not counted: too large, not a plain file, past the per-refresh budget, or git said nothing.
    Unknown,
}

/// A file changed since the last commit (`path` relative to the repo root).
#[allow(dead_code)] // wired by the Changes panel (M11h task 4)
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    pub path: PathBuf,
    pub mark: Mark,
    pub counts: Counts,
}

/// The changed files, sorted, at most `CHANGES_CAP`; `more` is how many were left out.
#[allow(dead_code)] // wired by the Changes panel (M11h task 4)
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ChangeList {
    pub entries: Vec<Change>,
    pub more: usize,
}

/// Most rows a `ChangeList` holds.
#[allow(dead_code)] // wired by the Changes panel (M11h task 4)
pub const CHANGES_CAP: usize = 1_000;
/// Most files counted from disk (untracked, or before the first commit) per `changes` call.
#[allow(dead_code)] // wired by the Changes panel (M11h task 4)
pub const COUNT_FILES_MAX: usize = 200;

/// `git diff --numstat -z`'s entries (`added\tremoved\tpath\0`, split on the first two tabs
/// only, so a path may hold one) as counts; `-\t-` is a binary file. Malformed entries are
/// skipped.
#[allow(dead_code)] // wired by the Changes panel (M11h task 4)
pub fn parse_numstat(z: &[u8]) -> HashMap<PathBuf, Counts> {
    let mut out = HashMap::new();
    for entry in z.split(|&b| b == 0) {
        let mut parts = entry.splitn(3, |&b| b == b'\t');
        let (Some(added), Some(removed), Some(path)) = (parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        if path.is_empty() {
            continue;
        }
        let number = |b: &[u8]| std::str::from_utf8(b).ok()?.parse::<u32>().ok();
        let counts = if added == b"-" && removed == b"-" {
            Counts::Binary
        } else if let (Some(added), Some(removed)) = (number(added), number(removed)) {
            Counts::Lines { added, removed }
        } else {
            continue;
        };
        out.insert(PathBuf::from(OsStr::from_bytes(path)), counts);
    }
    out
}

/// The lines of the plain file at `path`, all counted as added (it is new). Blocking. Only a
/// plain file (never a FIFO or a device, which could block), read through a bound: `Binary`
/// with a NUL in its first 8 KB, `Unknown` past `PLAIN_ABOVE` bytes or when unreadable.
#[allow(dead_code)] // wired by the Changes panel (M11h task 4)
pub fn count_lines(path: &Path) -> Counts {
    use std::io::Read;
    if !path.is_file() {
        return Counts::Unknown;
    }
    let mut bytes = Vec::new();
    let read = std::fs::File::open(path)
        .and_then(|file| file.take(PLAIN_ABOVE + 1).read_to_end(&mut bytes));
    if read.is_err() {
        return Counts::Unknown;
    }
    match classify(bytes.len() as u64, &bytes[..bytes.len().min(8 * 1024)]) {
        Viewable::Refuse => Counts::Binary,
        Viewable::PlainTooLarge => Counts::Unknown,
        Viewable::Text => {
            let lines = bytes.iter().filter(|&&b| b == b'\n').count()
                + usize::from(bytes.last().is_some_and(|&b| b != b'\n'));
            Counts::Lines {
                added: u32::try_from(lines).unwrap_or(u32::MAX),
                removed: 0,
            }
        }
    }
}

/// The changes' counts: from `numstat` when it has the file; else, for an Added (untracked or
/// new) file or before the first commit, counted from disk by `count` (`COUNT_FILES_MAX` per
/// call, then `Unknown`); else `Unknown`. A failed `numstat` is None.
fn assemble(
    marks: Vec<(PathBuf, Mark)>,
    numstat: Option<HashMap<PathBuf, Counts>>,
    fresh: bool,
    mut count: impl FnMut(&Path) -> Counts,
) -> Vec<Change> {
    let mut counted = 0;
    marks
        .into_iter()
        .map(|(path, mark)| {
            let counts = match numstat.as_ref().and_then(|n| n.get(&path)) {
                Some(&counts) => counts,
                None if mark == Mark::Added || fresh => {
                    if counted < COUNT_FILES_MAX {
                        counted += 1;
                        count(&path)
                    } else {
                        Counts::Unknown
                    }
                }
                None => Counts::Unknown,
            };
            Change { path, mark, counts }
        })
        .collect()
}

/// The files changed since the last commit, with their line counts. Blocking: call it from
/// background work. None outside a repo, or when git fails or takes longer than the cap (no
/// answer, not "nothing changed"). A failed `numstat` with a good status leaves the tracked
/// entries' counts `Unknown`.
#[allow(dead_code)] // wired by the Changes panel (M11h task 4)
pub fn changes(git: &Path, root: &Path) -> Option<ChangeList> {
    let status = run_git(
        git,
        root,
        &[
            "status",
            "--porcelain=v1",
            "-z",
            "--untracked-files=all",
            "--no-renames",
        ],
    )?;
    let mut marks = parse_porcelain(status.as_bytes());
    marks.sort_by_cached_key(|(path, _)| (path.to_string_lossy().to_lowercase(), path.clone()));
    let more = marks.len().saturating_sub(CHANGES_CAP);
    marks.truncate(CHANGES_CAP);
    // `fresh`: no commits yet, so there is no diff and every file is new. An unknown HEAD
    // probe (git failed) is not fresh: it falls through to `diff HEAD`, which fails the same way
    // and leaves the tracked entries' counts `Unknown`.
    let fresh = match yes_or_no(run_git_outcome(
        git,
        root,
        &["rev-parse", "--verify", "-q", "HEAD"],
    )) {
        Some(has_head) => !has_head,
        None => false,
    };
    let numstat = if fresh {
        Some(HashMap::new())
    } else {
        run_git(
            git,
            root,
            &[
                "diff",
                "HEAD",
                "--numstat",
                "-z",
                "--no-renames",
                "--no-color",
                "--no-ext-diff",
            ],
        )
        .map(|out| parse_numstat(out.as_bytes()))
    };
    let entries = assemble(marks, numstat, fresh, |path| count_lines(&root.join(path)));
    Some(ChangeList { entries, more })
}

/// `dir`'s entries: folders first, then files, each case-insensitive by name; `.git` left out.
/// At most `cap` are returned; the second value is how many were left out. A symlink to a
/// folder counts as a folder. Unreadable: empty.
pub fn list_dir(dir: &Path, cap: usize) -> (Vec<DirEntry>, usize) {
    let Ok(read) = std::fs::read_dir(dir) else {
        return (Vec::new(), 0);
    };
    let mut entries: Vec<DirEntry> = read
        .filter_map(Result::ok)
        .filter(|e| e.file_name() != ".git")
        .map(|e| DirEntry {
            name: e.file_name().to_string_lossy().into_owned(),
            is_dir: e.file_type().is_ok_and(|t| t.is_dir())
                || (e.file_type().is_ok_and(|t| t.is_symlink()) && e.path().is_dir()),
        })
        .collect();
    entries.sort_by_cached_key(|e| (!e.is_dir, e.name.to_lowercase(), e.name.clone()));
    let more = entries.len().saturating_sub(cap);
    entries.truncate(cap);
    (entries, more)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git_branch::find_git;
    use std::fs;
    use std::process::Command;

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    #[test]
    fn porcelain_marks() {
        let z = b" M src/a.rs\0?? new file.txt\0 D gone.rs\0A  added.rs\0R  new.rs\0old.rs\0MM both.rs\0";
        assert_eq!(
            parse_porcelain(z),
            vec![
                (p("src/a.rs"), Mark::Modified),
                (p("new file.txt"), Mark::Added),
                (p("gone.rs"), Mark::Deleted),
                (p("added.rs"), Mark::Added),
                (p("new.rs"), Mark::Added),
                (p("both.rs"), Mark::Modified),
            ]
        );
    }

    #[test]
    fn hunk_headers() {
        use LineChange::*;
        assert_eq!(
            parse_hunks("@@ -3,0 +4,2 @@\n+a\n+b\n"),
            vec![(4, Added), (5, Added)]
        );
        assert_eq!(
            parse_hunks("@@ -5,2 +4,0 @@\n-a\n-b\n"),
            vec![(4, DeletedAfter)]
        );
        assert_eq!(
            parse_hunks("@@ -7 +7 @@ fn x()\n-a\n+b\n"),
            vec![(7, Changed)]
        );
        assert_eq!(
            parse_hunks("diff --git a/f b/f\n@@ -1,2 +1,3 @@\n@@ -9 +10,0 @@\n"),
            vec![(1, Changed), (2, Changed), (3, Added), (10, DeletedAfter)]
        );
    }

    #[test]
    fn list_dir_order_cap_and_git() {
        let dir = temp("list");
        for d in [".git", "zeta", "Alpha"] {
            fs::create_dir(dir.join(d)).unwrap();
        }
        for f in ["b.txt", "A.txt", "c.txt"] {
            fs::write(dir.join(f), "x").unwrap();
        }
        let (entries, more) = list_dir(&dir, 10);
        let names: Vec<_> = entries
            .iter()
            .map(|e| (e.name.as_str(), e.is_dir))
            .collect();
        assert_eq!(
            names,
            [
                ("Alpha", true),
                ("zeta", true),
                ("A.txt", false),
                ("b.txt", false),
                ("c.txt", false)
            ]
        );
        assert_eq!(more, 0);
        let big = temp("cap");
        for i in 0..12 {
            fs::write(big.join(format!("f{i:02}")), "x").unwrap();
        }
        let (entries, more) = list_dir(&big, 10);
        assert_eq!((entries.len(), more), (10, 2));
        assert_eq!(entries[0].name, "f00");
    }

    #[test]
    fn not_a_repo() {
        let Some(git) = find_git() else {
            return println!("skipped: no git");
        };
        let dir = temp("norepo");
        fs::write(dir.join("f.txt"), "x\n").unwrap();
        assert_eq!(repo_root(&git, &dir), None);
        assert_eq!(line_changes(&git, &dir, Path::new("f.txt")), None);
        // A failed status (exit 128) is no answer, not "nothing changed".
        assert_eq!(status(&git, &dir), None);
        assert_eq!(ignored(&git, &dir, &[p("f.txt")]), None);
    }

    #[test]
    fn check_ignore_exit_1_is_none_ignored_and_a_failure_is_no_answer() {
        assert_eq!(
            ignored_answer(Ran::Exited(Some(1), String::new())),
            Some(HashSet::new())
        );
        assert_eq!(
            ignored_answer(Ran::Exited(Some(0), "target\0a b.log\0".into())),
            Some(HashSet::from([p("target"), p("a b.log")]))
        );
        assert_eq!(ignored_answer(Ran::Exited(Some(128), String::new())), None);
        assert_eq!(ignored_answer(Ran::Exited(None, String::new())), None);
        assert_eq!(ignored_answer(Ran::TimedOut), None);
        assert_eq!(ignored_answer(Ran::Failed), None);
    }

    #[test]
    fn an_untracked_file_past_the_limit_has_no_bars() {
        let Some(git) = find_git() else {
            return println!("skipped: no git");
        };
        let root = repo("huge", &git);
        let huge = vec![b'\n'; REFUSE_ABOVE as usize + 1];
        fs::write(root.join("huge.txt"), huge).unwrap();
        assert_eq!(
            line_changes(&git, &root, Path::new("huge.txt")),
            Some(Vec::new())
        );
        // Not a plain file (a folder here): no bars, nothing read.
        fs::create_dir(root.join("sub")).unwrap();
        fs::write(root.join("sub/x"), "x\n").unwrap();
        assert_eq!(
            line_changes(&git, &root, Path::new("sub")),
            Some(Vec::new())
        );
    }

    #[test]
    fn temp_repo_end_to_end() {
        let Some(git) = find_git() else {
            return println!("skipped: no git");
        };
        let root = temp("repo");
        let run = |args: &[&str]| {
            let ok = Command::new(&git)
                .arg("-C")
                .arg(&root)
                .args([
                    "-c",
                    "user.name=t",
                    "-c",
                    "user.email=t@t",
                    "-c",
                    "commit.gpgsign=false",
                ])
                .args(args)
                .status()
                .unwrap()
                .success();
            assert!(ok, "git {args:?}");
        };
        run(&["init", "-q"]);
        fs::write(root.join(".gitignore"), "target/\n").unwrap();
        fs::write(root.join("a.txt"), "one\ntwo\nthree\n").unwrap();

        // No commits yet: every line of a file is Added.
        let none = line_changes(&git, &root, Path::new("a.txt")).unwrap();
        assert_eq!(none.len(), 3);
        assert!(none.iter().all(|(_, c)| *c == LineChange::Added));

        run(&["add", "-A"]);
        run(&["commit", "-q", "-m", "first"]);
        assert_eq!(repo_root(&git, &root), Some(root.canonicalize().unwrap()));

        fs::write(root.join("a.txt"), "one\nTWO\nthree\n").unwrap();
        fs::write(root.join("new.txt"), "x\ny\n").unwrap();
        fs::create_dir(root.join("target")).unwrap();
        fs::write(root.join("target/out"), "x").unwrap();

        let marks = status(&git, &root).unwrap();
        assert_eq!(marks.get(&p("a.txt")), Some(&Mark::Modified));
        assert_eq!(marks.get(&p("new.txt")), Some(&Mark::Added));

        let ign = ignored(&git, &root, &[p("target"), p("a.txt"), p("new.txt")]);
        assert_eq!(ign, Some(HashSet::from([p("target")])));
        // None ignored (git exits 1): an answer, empty.
        assert_eq!(ignored(&git, &root, &[p("a.txt")]), Some(HashSet::new()));

        assert_eq!(
            line_changes(&git, &root, Path::new("a.txt")),
            Some(vec![(2, LineChange::Changed)])
        );
        let new = line_changes(&git, &root, Path::new("new.txt")).unwrap();
        assert_eq!(new, vec![(1, LineChange::Added), (2, LineChange::Added)]);
    }

    #[test]
    fn conflicts_are_modified() {
        let z = b"UU c.rs\0AA d.rs\0DD e.rs\0DU f.rs\0UD g.rs\0";
        assert!(parse_porcelain(z).iter().all(|(_, m)| *m == Mark::Modified));
        assert_eq!(parse_porcelain(z).len(), 5);
    }

    /// A repo in a temp dir with `run` to call git in it.
    fn repo(name: &str, git: &Path) -> PathBuf {
        let root = temp(name);
        git_in(git, &root, &["init", "-q"]);
        root
    }

    fn git_in(git: &Path, root: &Path, args: &[&str]) {
        let ok = Command::new(git)
            .arg("-C")
            .arg(root)
            .args([
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@t",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(args)
            .status()
            .unwrap()
            .success();
        assert!(ok, "git {args:?}");
    }

    #[test]
    fn spaces_unicode_and_a_real_rename() {
        let Some(git) = find_git() else {
            return println!("skipped: no git");
        };
        let root = repo("names", &git);
        fs::write(root.join(".gitignore"), "*.log\n").unwrap();
        fs::write(root.join("old name.txt"), "a\nb\n").unwrap();
        git_in(&git, &root, &["add", "-A"]);
        git_in(&git, &root, &["commit", "-q", "-m", "first"]);
        git_in(&git, &root, &["mv", "old name.txt", "new n\u{e4}me.txt"]);
        fs::write(root.join("caf\u{e9} x.txt"), "x").unwrap();
        fs::write(root.join("caf\u{e9} x.log"), "x").unwrap();
        fs::write(root.join("sp ace.log"), "x").unwrap();
        let marks = status(&git, &root).unwrap();
        assert_eq!(marks.get(&p("new n\u{e4}me.txt")), Some(&Mark::Added));
        assert_eq!(marks.get(&p("caf\u{e9} x.txt")), Some(&Mark::Added));
        assert!(!marks.contains_key(&p("old name.txt")));
        let names = [p("caf\u{e9} x.log"), p("sp ace.log"), p("caf\u{e9} x.txt")];
        assert_eq!(
            ignored(&git, &root, &names),
            Some(HashSet::from([p("caf\u{e9} x.log"), p("sp ace.log")]))
        );
        // Glob characters in a name are literal.
        fs::write(root.join("a[1].txt"), "q\n").unwrap();
        assert_eq!(
            line_changes(&git, &root, Path::new("a[1].txt")),
            Some(vec![(1, LineChange::Added)])
        );
        // A tracked one: a glob would also match a1.txt.
        fs::write(root.join("a1.txt"), "p\nq\n").unwrap();
        git_in(&git, &root, &["add", "-A"]);
        git_in(&git, &root, &["commit", "-q", "-m", "second"]);
        fs::write(root.join("a[1].txt"), "Q\n").unwrap();
        fs::write(root.join("a1.txt"), "p\nq\nr\ns\n").unwrap();
        assert_eq!(
            line_changes(&git, &root, Path::new("a[1].txt")),
            Some(vec![(1, LineChange::Changed)])
        );
    }

    #[test]
    fn ignored_in_one_batch_at_any_size() {
        let Some(git) = find_git() else {
            return println!("skipped: no git");
        };
        let root = repo("many", &git);
        fs::write(root.join(".gitignore"), "*.log\n").unwrap();
        let names: Vec<PathBuf> = (0..3000)
            .map(|i| {
                let ext = if i % 3 == 0 { "log" } else { "txt" };
                p(&format!(
                    "a-rather-long-file-name-to-fill-the-pipe-{i:05}.{ext}"
                ))
            })
            .collect();
        for n in &names {
            fs::write(root.join(n), "x").unwrap();
        }
        let want: HashSet<PathBuf> = names
            .iter()
            .filter(|n| n.extension().unwrap() == "log")
            .cloned()
            .collect();
        assert!(names.len() * 60 > 100_000);
        assert_eq!(ignored(&git, &root, &names), Some(want));
    }

    #[test]
    fn numstat_counts_lines_and_binaries() {
        let lines = |added, removed| Counts::Lines { added, removed };
        assert_eq!(
            parse_numstat(b"3\t1\tsrc/a.rs\0-\t-\tlogo.png\0"),
            HashMap::from([
                (p("src/a.rs"), lines(3, 1)),
                (p("logo.png"), Counts::Binary)
            ])
        );
        assert_eq!(
            parse_numstat(b"0\t0\tmode.sh\0"),
            HashMap::from([(p("mode.sh"), lines(0, 0))])
        );
        assert!(parse_numstat(b"x\ty\0").is_empty());
        assert!(parse_numstat(b"").is_empty());
        assert_eq!(
            parse_numstat(b"1\t0\ta\tb.txt\0"),
            HashMap::from([(p("a\tb.txt"), lines(1, 0))])
        );
    }

    #[test]
    fn count_lines_reads_only_small_text_files() {
        let dir = temp("count");
        let lines = |n| Counts::Lines {
            added: n,
            removed: 0,
        };
        fs::write(dir.join("nl"), "a\nb\n").unwrap();
        fs::write(dir.join("no-nl"), "a\nb").unwrap();
        fs::write(dir.join("empty"), "").unwrap();
        fs::write(dir.join("nul"), b"ab\0cd\n").unwrap();
        fs::write(dir.join("big"), vec![b'a'; PLAIN_ABOVE as usize + 1]).unwrap();
        fs::write(dir.join("edge"), vec![b'\n'; PLAIN_ABOVE as usize]).unwrap();
        fs::create_dir(dir.join("folder")).unwrap();
        assert_eq!(count_lines(&dir.join("nl")), lines(2));
        assert_eq!(count_lines(&dir.join("no-nl")), lines(2));
        assert_eq!(count_lines(&dir.join("empty")), lines(0));
        assert_eq!(count_lines(&dir.join("nul")), Counts::Binary);
        assert_eq!(count_lines(&dir.join("big")), Counts::Unknown);
        assert_eq!(count_lines(&dir.join("edge")), lines(PLAIN_ABOVE as u32));
        assert_eq!(count_lines(&dir.join("folder")), Counts::Unknown);
        assert_eq!(count_lines(&dir.join("missing")), Counts::Unknown);
    }

    #[test]
    fn changes_end_to_end() {
        let Some(git) = find_git() else {
            return println!("skipped: no git");
        };
        let root = repo("changes", &git);
        fs::write(root.join("a.rs"), "one\ntwo\nthree\n").unwrap();
        fs::write(root.join("b.rs"), "x\ny\n").unwrap();
        fs::write(root.join("c.rs"), "q\n").unwrap();
        git_in(&git, &root, &["add", "-A"]);
        git_in(&git, &root, &["commit", "-q", "-m", "first"]);
        fs::write(root.join("a.rs"), "one\nTWO\nthree\n").unwrap();
        fs::remove_file(root.join("b.rs")).unwrap();
        git_in(&git, &root, &["mv", "c.rs", "d.rs"]);
        fs::create_dir(root.join("new dir")).unwrap();
        fs::write(root.join("new dir/\u{fc}.txt"), "1\n2\n").unwrap();
        fs::write(root.join("bin.dat"), b"a\0b").unwrap();
        let lines = |added, removed| Counts::Lines { added, removed };
        let change = |path: &str, mark, counts| Change {
            path: p(path),
            mark,
            counts,
        };
        let want = vec![
            change("a.rs", Mark::Modified, lines(1, 1)),
            change("b.rs", Mark::Deleted, lines(0, 2)),
            change("bin.dat", Mark::Added, Counts::Binary),
            change("c.rs", Mark::Deleted, lines(0, 1)),
            change("d.rs", Mark::Added, lines(1, 0)),
            change("new dir/\u{fc}.txt", Mark::Added, lines(2, 0)),
        ];
        let got = changes(&git, &root).unwrap();
        assert_eq!(got.entries, want);
        assert_eq!(got.more, 0);
        // Staged, the edit reads the same.
        git_in(&git, &root, &["add", "a.rs"]);
        assert_eq!(changes(&git, &root).unwrap().entries, want);
    }

    #[test]
    fn changes_cap_and_more() {
        let Some(git) = find_git() else {
            return println!("skipped: no git");
        };
        let root = repo("cap-changes", &git);
        git_in(
            &git,
            &root,
            &["commit", "-q", "--allow-empty", "-m", "first"],
        );
        for i in 0..CHANGES_CAP + 5 {
            fs::write(root.join(format!("f{i:04}.txt")), "x\n").unwrap();
        }
        let got = changes(&git, &root).unwrap();
        assert_eq!(got.entries.len(), CHANGES_CAP);
        assert_eq!(got.more, 5);
        assert_eq!(got.entries[0].path, p("f0000.txt"));
        let one = Counts::Lines {
            added: 1,
            removed: 0,
        };
        let (counted, rest) = got.entries.split_at(COUNT_FILES_MAX);
        assert!(counted.iter().all(|c| c.counts == one));
        assert!(rest.iter().all(|c| c.counts == Counts::Unknown));
    }

    #[test]
    fn changes_without_commits() {
        let Some(git) = find_git() else {
            return println!("skipped: no git");
        };
        let root = repo("changes-fresh", &git);
        fs::write(root.join("x.txt"), "1\n2\n").unwrap();
        git_in(&git, &root, &["add", "x.txt"]);
        fs::write(root.join("y.txt"), "q\n").unwrap();
        let lines = |added| Counts::Lines { added, removed: 0 };
        let got = changes(&git, &root).unwrap();
        assert_eq!(
            got.entries,
            vec![
                Change {
                    path: p("x.txt"),
                    mark: Mark::Added,
                    counts: lines(2)
                },
                Change {
                    path: p("y.txt"),
                    mark: Mark::Added,
                    counts: lines(1)
                },
            ]
        );
    }

    #[test]
    fn a_failed_numstat_counts_added_files_from_disk_and_leaves_the_rest_unknown() {
        let marks = vec![
            (p("a.txt"), Mark::Added),
            (p("m.txt"), Mark::Modified),
            (p("d.txt"), Mark::Deleted),
        ];
        let got = assemble(marks, None, false, |_| Counts::Lines {
            added: 7,
            removed: 0,
        });
        let counts: Vec<_> = got.iter().map(|c| c.counts).collect();
        assert_eq!(
            counts,
            vec![
                Counts::Lines {
                    added: 7,
                    removed: 0
                },
                Counts::Unknown,
                Counts::Unknown
            ]
        );
    }

    #[test]
    fn a_staged_new_file_counts_the_same_from_disk_as_git_does() {
        let Some(git) = find_git() else {
            return println!("skipped: no git");
        };
        let root = repo("staged-new", &git);
        git_in(
            &git,
            &root,
            &["commit", "-q", "--allow-empty", "-m", "first"],
        );
        fs::write(root.join("n.txt"), "1\n2\n3\n").unwrap();
        git_in(&git, &root, &["add", "n.txt"]);
        let numstat = run_git(
            &git,
            &root,
            &["diff", "HEAD", "--numstat", "-z", "--no-renames"],
        )
        .unwrap();
        assert_eq!(
            parse_numstat(numstat.as_bytes()).get(&p("n.txt")),
            Some(&count_lines(&root.join("n.txt")))
        );
    }

    #[test]
    fn a_merge_conflict_is_one_modified_row() {
        let Some(git) = find_git() else {
            return println!("skipped: no git");
        };
        let root = repo("conflict", &git);
        fs::write(root.join("c.txt"), "base\n").unwrap();
        git_in(&git, &root, &["add", "-A"]);
        git_in(&git, &root, &["commit", "-q", "-m", "base"]);
        git_in(&git, &root, &["checkout", "-q", "-b", "other"]);
        fs::write(root.join("c.txt"), "theirs\n").unwrap();
        git_in(&git, &root, &["commit", "-q", "-am", "theirs"]);
        git_in(&git, &root, &["checkout", "-q", "-"]);
        fs::write(root.join("c.txt"), "ours\n").unwrap();
        git_in(&git, &root, &["commit", "-q", "-am", "ours"]);
        // The merge itself fails (a conflict): that is the point.
        let merged = Command::new(&git)
            .arg("-C")
            .arg(&root)
            .args(["-c", "user.name=t", "-c", "user.email=t@t"])
            .args(["merge", "other"])
            .output()
            .unwrap();
        assert!(!merged.status.success());
        let status = run_git(&git, &root, &["status", "--porcelain=v1"]).unwrap();
        assert!(status.starts_with("UU c.txt"), "{status}");
        let got = changes(&git, &root).unwrap();
        assert_eq!(got.entries.len(), 1);
        assert_eq!(got.entries[0].path, p("c.txt"));
        assert_eq!(got.entries[0].mark, Mark::Modified);
    }

    #[test]
    fn changes_outside_a_repo_is_no_answer() {
        let Some(git) = find_git() else {
            return println!("skipped: no git");
        };
        assert_eq!(changes(&git, &temp("changes-norepo")), None);
    }

    fn temp(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("fuxx-git-status-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_staged_deletion_still_diffs_its_old_lines() {
        let Some(git) = find_git() else {
            return println!("skipped: no git");
        };
        let root = repo("staged-deletion", &git);
        fs::write(root.join("a.rs"), "one\ntwo\n").unwrap();
        fs::write(root.join("b.rs"), "x\n").unwrap();
        git_in(&git, &root, &["add", "-A"]);
        git_in(&git, &root, &["commit", "-q", "-m", "first"]);
        // `git rm`: gone from the disk and the index, still in HEAD.
        git_in(&git, &root, &["rm", "-q", "a.rs"]);
        // `git mv`: the old path is gone from the index too.
        git_in(&git, &root, &["mv", "b.rs", "c.rs"]);
        for (rel, lines) in [("a.rs", 2), ("b.rs", 1)] {
            let Some(FileDiff::Unified(u)) = file_diff(&git, &root, Path::new(rel), 1) else {
                panic!("{rel}: a unified diff");
            };
            let removed = u
                .lines()
                .filter(|l| l.starts_with('-') && !l.starts_with("---"));
            assert_eq!(removed.count(), lines, "{rel}: {u}");
            assert!(
                !u.lines()
                    .any(|l| l.starts_with('+') && !l.starts_with("+++")),
                "{rel}"
            );
        }
        // The new path of the move (staged, so in the index) is all new lines.
        let Some(FileDiff::Unified(u)) = file_diff(&git, &root, Path::new("c.rs"), 2) else {
            panic!("c.rs: a unified diff");
        };
        assert!(u.contains("\n@@ -0,0 +1 @@\n+x\n"), "{u}");
    }

    #[test]
    fn head_size_and_repo_root_tell_no_from_no_answer() {
        let Some(git) = find_git() else {
            return println!("skipped: no git");
        };
        let root = repo("head-size", &git);
        // No commits yet: nothing in HEAD.
        fs::write(root.join("a.rs"), "12345\n").unwrap();
        assert_eq!(head_size(&git, &root, Path::new("a.rs")), Some(None));
        git_in(&git, &root, &["add", "-A"]);
        git_in(&git, &root, &["commit", "-q", "-m", "first"]);
        assert_eq!(head_size(&git, &root, Path::new("a.rs")), Some(Some(6)));
        fs::write(root.join("new.rs"), "x\n").unwrap();
        assert_eq!(head_size(&git, &root, Path::new("new.rs")), Some(None));
        // A repo's top, or (git says so) not a repo; no git to run: no answer.
        let top = repo_root_at(&git, &root)
            .expect("an answer")
            .expect("a repo");
        assert_eq!(
            fs::canonicalize(top).unwrap(),
            fs::canonicalize(&root).unwrap()
        );
        assert_eq!(repo_root_at(&git, &temp("head-size-plain")), Some(None));
        assert_eq!(repo_root_at(Path::new("/no/such/git"), &root), None);
    }
}
