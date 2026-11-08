//! Git and folder lookups for the right sidebar and file tabs (M11g): the repo root, the status
//! marks, the ignored files, a file's changed lines against HEAD, and a folder's listing.
//!
//! git runs the way `git_branch` runs it (`run_git`: no prompt, no index lock, 2 s cap, no
//! terminal). These are blocking: callers run them on the background executor. Read-only.
//!
//! Status marks (porcelain v1, `-z`; X is the index column, Y the work tree's): `M` or `T` in
//! either column is Modified; `A` or `??` is Added; `D` is Deleted; `R` or `C` is Added for the
//! new path (the old path of a rename is not marked). Anything else (`U` conflicts, `!!`) is
//! Modified if it is a tracked change, and `!!` is skipped.

use std::collections::{HashMap, HashSet};
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use crate::git_branch::{run_git, run_git_stdin};

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
            (b'D', _) | (_, b'D') => Mark::Deleted,
            (b'A', _) => Mark::Added,
            _ => Mark::Modified,
        };
        out.push((PathBuf::from(OsStr::from_bytes(&entry[3..])), mark));
    }
    out
}

/// The repo's changed files (paths relative to `root`, a folder of untracked files as one
/// entry); the caller marks their parent folders. Empty when git fails.
pub fn status(git: &Path, root: &Path) -> HashMap<PathBuf, Mark> {
    run_git(git, root, &["status", "--porcelain=v1", "-z"])
        .map(|out| parse_porcelain(out.as_bytes()).into_iter().collect())
        .unwrap_or_default()
}

/// Which of `rel` (paths relative to `root`) git ignores, in one `check-ignore --stdin` call.
/// Empty when none are (git exits 1 then), when git fails, or when the list is too long to send.
pub fn ignored(git: &Path, root: &Path, rel: &[PathBuf]) -> HashSet<PathBuf> {
    if rel.is_empty() {
        return HashSet::new();
    }
    let mut input = Vec::new();
    for path in rel {
        input.extend_from_slice(path.as_os_str().as_bytes());
        input.push(0);
    }
    run_git_stdin(git, root, &["check-ignore", "-z", "--stdin"], &input)
        .map(|out| {
            out.split('\0')
                .filter(|path| !path.is_empty())
                .map(PathBuf::from)
                .collect()
        })
        .unwrap_or_default()
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

/// `rel`'s (relative to `root`) lines changed on disk against HEAD. None outside a repo; an
/// untracked file, or any file in a repo with no commits, has every line Added.
pub fn line_changes(git: &Path, root: &Path, rel: &Path) -> Option<Vec<(usize, LineChange)>> {
    run_git(git, root, &["rev-parse", "--is-inside-work-tree"])?;
    let path = rel.to_string_lossy();
    let has_head = run_git(git, root, &["rev-parse", "--verify", "-q", "HEAD"]).is_some();
    let tracked = run_git(git, root, &["ls-files", "--error-unmatch", "--", &path]).is_some();
    if !has_head || !tracked {
        let bytes = std::fs::read(root.join(rel)).unwrap_or_default();
        let lines = bytes.iter().filter(|&&b| b == b'\n').count()
            + usize::from(bytes.last().is_some_and(|&b| b != b'\n'));
        return Some((1..=lines).map(|n| (n, LineChange::Added)).collect());
    }
    let diff = run_git(
        git,
        root,
        &[
            "diff",
            "HEAD",
            "-U0",
            "--no-color",
            "--no-ext-diff",
            "--",
            &path,
        ],
    );
    Some(parse_hunks(&diff.unwrap_or_default()))
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

        let marks = status(&git, &root);
        assert_eq!(marks.get(&p("a.txt")), Some(&Mark::Modified));
        assert_eq!(marks.get(&p("new.txt")), Some(&Mark::Added));

        let ign = ignored(&git, &root, &[p("target"), p("a.txt"), p("new.txt")]);
        assert_eq!(ign, HashSet::from([p("target")]));
        assert!(ignored(&git, &root, &[p("a.txt")]).is_empty());

        assert_eq!(
            line_changes(&git, &root, Path::new("a.txt")),
            Some(vec![(2, LineChange::Changed)])
        );
        let new = line_changes(&git, &root, Path::new("new.txt")).unwrap();
        assert_eq!(new, vec![(1, LineChange::Added), (2, LineChange::Added)]);
    }

    fn temp(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("fuxx-git-status-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }
}
