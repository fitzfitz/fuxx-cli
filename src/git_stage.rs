//! The Changes tab's file actions (M11v part 2, note 73): stage, unstage and revert, and the
//! words the panel says about them. `run` is blocking (background work).
//!
//! Every path goes after `--` with `--literal-pathspecs` (a name like `-f` or `*` is a file,
//! never an option or a pattern). Revert is the one action that loses work: the panel asks
//! first (`revert_words`, `revert_all_words`).

use crate::branch_sync::refusal;
use crate::git_branch::{FETCH_CAP, run_git, run_git_checked_all};
use crate::git_status::{ChangeList, Counts};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// What to do to which files (paths relative to the repo's top folder).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileWork {
    Stage(Vec<PathBuf>),
    Unstage(Vec<PathBuf>),
    /// `tracked` go back to their staged (else committed) state; `untracked` are deleted.
    Revert {
        tracked: Vec<PathBuf>,
        untracked: Vec<PathBuf>,
    },
}

/// The bar's line while `work` runs.
pub fn working_text(work: &FileWork) -> &'static str {
    match work {
        FileWork::Stage(_) => "Staging\u{2026}",
        FileWork::Unstage(_) => "Unstaging\u{2026}",
        FileWork::Revert { .. } => "Reverting\u{2026}",
    }
}

/// The verb for a failure: `Stage failed: …`.
pub fn verb(work: &FileWork) -> &'static str {
    match work {
        FileWork::Stage(_) => "Stage",
        FileWork::Unstage(_) => "Unstage",
        FileWork::Revert { .. } => "Revert",
    }
}

/// `1 file`, `2 files`.
fn files(n: usize) -> String {
    if n == 1 {
        "1 file".to_string()
    } else {
        format!("{n} files")
    }
}

const NO_UNDO: &str = "This can't be undone.";

/// The dialog's title, body and button for reverting one file: `name`, its unstaged `counts`,
/// `untracked` (a new file: deleted).
pub fn revert_words(name: &str, counts: Counts, untracked: bool) -> (String, String, &'static str) {
    if untracked {
        return (
            format!("Delete {name}?"),
            format!("It's a new file git doesn't know yet: it is deleted. {NO_UNDO}"),
            "Delete",
        );
    }
    let what = match counts {
        Counts::Lines { added, removed: 0 } if added > 0 => format!("Its {added} added line(s)"),
        Counts::Lines { added: 0, removed } if removed > 0 => {
            format!("Its {removed} removed line(s)")
        }
        Counts::Lines { added, removed } if added > 0 => {
            format!("Its {added} added and {removed} removed line(s)")
        }
        _ => "Its changes".to_string(),
    };
    (
        format!("Revert {name}?"),
        format!("{what} since it was last staged or committed are thrown away. {NO_UNDO}"),
        "Revert",
    )
}

/// The dialog's words for reverting every unstaged file: `tracked` go back, `untracked` are
/// deleted.
pub fn revert_all_words(tracked: usize, untracked: usize) -> (String, String, &'static str) {
    let back = format!("{tracked} changed file(s) go back to their last staged or committed state");
    let gone = format!("{untracked} new file(s) are deleted");
    let body = match (tracked, untracked) {
        (0, _) => format!("{gone}. {NO_UNDO}"),
        (_, 0) => format!("{back}. {NO_UNDO}"),
        _ => format!("{back} and {gone}. {NO_UNDO}"),
    };
    (
        format!("Revert {}?", files(tracked + untracked)),
        body,
        "Revert all",
    )
}

/// (files with a staged part, files) in `list`: a file staged and edited again counts once;
/// the files past the list (`more`) count as files.
pub fn staged_summary(list: &ChangeList) -> (usize, usize) {
    let staged: HashSet<&PathBuf> = list
        .entries
        .iter()
        .filter(|c| c.staged)
        .map(|c| &c.path)
        .collect();
    let all: HashSet<&PathBuf> = list.entries.iter().map(|c| &c.path).collect();
    (staged.len(), all.len() + list.more)
}

/// Run `work` at `root`: `Ok` when git did it, else the line saying why it refused.
pub fn run(git: &Path, root: &Path, work: &FileWork) -> Result<(), String> {
    // `head` before `--`, then `paths` (none: nothing to do; `add -A --` alone would stage
    // everything).
    let on = |head: &[&str], paths: &[PathBuf]| -> Result<(), String> {
        if paths.is_empty() {
            return Ok(());
        }
        let names: Vec<String> = paths
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        let mut args = vec!["--literal-pathspecs"];
        args.extend_from_slice(head);
        args.push("--");
        args.extend(names.iter().map(String::as_str));
        run_git_checked_all(git, root, &args, FETCH_CAP, &[])
            .map(|_| ())
            .map_err(|stderr| refusal(&stderr))
    };
    match work {
        FileWork::Stage(paths) => on(&["add", "-A"], paths),
        FileWork::Unstage(paths) => {
            // Before the first commit there is no HEAD to restore the index from.
            if run_git(git, root, &["rev-parse", "-q", "--verify", "HEAD"]).is_some() {
                on(&["restore", "--staged"], paths)
            } else {
                on(&["rm", "--cached", "-q", "-r"], paths)
            }
        }
        FileWork::Revert { tracked, untracked } => {
            on(&["restore", "--worktree"], tracked)?;
            on(&["clean", "-f", "-q"], untracked)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git_status::{Change, Mark, changes};
    use std::fs;
    use std::process::Command;

    fn temp(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("fuxx-git-stage-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn git_in(git: &Path, dir: &Path, args: &[&str]) {
        let out = Command::new(git)
            .arg("-C")
            .arg(dir)
            .args([
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@t",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    /// A repo with `a.txt` (`1 2 3`) committed.
    fn repo(name: &str, git: &Path) -> PathBuf {
        let root = temp(name);
        git_in(git, &root, &["init", "-q", "-b", "main"]);
        fs::write(root.join("a.txt"), "1\n2\n3\n").unwrap();
        git_in(git, &root, &["add", "-A"]);
        git_in(git, &root, &["commit", "-q", "-m", "first"]);
        root
    }

    /// (path, staged) of each entry.
    fn sides(git: &Path, root: &Path) -> Vec<(String, bool)> {
        changes(git, root)
            .unwrap()
            .entries
            .into_iter()
            .map(|c| (c.path.to_string_lossy().into_owned(), c.staged))
            .collect()
    }

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    #[test]
    fn stage_then_unstage() {
        let Some(git) = crate::git_branch::find_git() else {
            return;
        };
        let root = repo("stage", &git);
        fs::write(root.join("a.txt"), "1\nTWO\n3\n").unwrap();
        fs::write(root.join("new.txt"), "x\n").unwrap();
        run(
            &git,
            &root,
            &FileWork::Stage(vec![p("a.txt"), p("new.txt")]),
        )
        .unwrap();
        assert_eq!(
            sides(&git, &root),
            vec![("a.txt".into(), true), ("new.txt".into(), true)]
        );
        run(&git, &root, &FileWork::Unstage(vec![p("a.txt")])).unwrap();
        assert_eq!(
            sides(&git, &root),
            vec![("new.txt".into(), true), ("a.txt".into(), false)]
        );
    }

    #[test]
    fn a_deleted_file_stages_and_unstages() {
        let Some(git) = crate::git_branch::find_git() else {
            return;
        };
        let root = repo("deleted", &git);
        fs::remove_file(root.join("a.txt")).unwrap();
        run(&git, &root, &FileWork::Stage(vec![p("a.txt")])).unwrap();
        assert_eq!(sides(&git, &root), vec![("a.txt".into(), true)]);
        run(&git, &root, &FileWork::Unstage(vec![p("a.txt")])).unwrap();
        assert_eq!(sides(&git, &root), vec![("a.txt".into(), false)]);
    }

    #[test]
    fn unstage_before_the_first_commit() {
        let Some(git) = crate::git_branch::find_git() else {
            return;
        };
        let root = temp("fresh");
        git_in(&git, &root, &["init", "-q"]);
        fs::write(root.join("x.txt"), "x\n").unwrap();
        run(&git, &root, &FileWork::Stage(vec![p("x.txt")])).unwrap();
        assert_eq!(sides(&git, &root), vec![("x.txt".into(), true)]);
        run(&git, &root, &FileWork::Unstage(vec![p("x.txt")])).unwrap();
        assert_eq!(sides(&git, &root), vec![("x.txt".into(), false)]);
        assert!(root.join("x.txt").exists(), "the file stays on disk");
    }

    #[test]
    fn revert_keeps_the_staged_part() {
        let Some(git) = crate::git_branch::find_git() else {
            return;
        };
        let root = repo("revert", &git);
        fs::write(root.join("a.txt"), "1\nTWO\n3\n").unwrap();
        git_in(&git, &root, &["add", "a.txt"]);
        fs::write(root.join("a.txt"), "1\nTWO\n3\nmore\n").unwrap();
        let work = FileWork::Revert {
            tracked: vec![p("a.txt")],
            untracked: vec![],
        };
        run(&git, &root, &work).unwrap();
        assert_eq!(
            fs::read_to_string(root.join("a.txt")).unwrap(),
            "1\nTWO\n3\n"
        );
        assert_eq!(sides(&git, &root), vec![("a.txt".into(), true)]);
    }

    #[test]
    fn revert_deletes_a_new_file_and_brings_back_a_deleted_one() {
        let Some(git) = crate::git_branch::find_git() else {
            return;
        };
        let root = repo("revert-new", &git);
        fs::write(root.join("new.txt"), "x\n").unwrap();
        fs::remove_file(root.join("a.txt")).unwrap();
        let work = FileWork::Revert {
            tracked: vec![p("a.txt")],
            untracked: vec![p("new.txt")],
        };
        run(&git, &root, &work).unwrap();
        assert!(!root.join("new.txt").exists());
        assert_eq!(fs::read_to_string(root.join("a.txt")).unwrap(), "1\n2\n3\n");
        assert!(sides(&git, &root).is_empty());
    }

    /// Names git could read as an option or a pattern are files.
    #[test]
    fn paths_with_spaces_a_leading_dash_and_a_star() {
        let Some(git) = crate::git_branch::find_git() else {
            return;
        };
        let root = repo("odd", &git);
        for name in ["-f", "sp ace.txt", "*"] {
            fs::write(root.join(name), "x\n").unwrap();
        }
        fs::write(root.join("keep.txt"), "x\n").unwrap();
        run(
            &git,
            &root,
            &FileWork::Stage(vec![p("-f"), p("sp ace.txt"), p("*")]),
        )
        .unwrap();
        let got = sides(&git, &root);
        assert!(got.contains(&("-f".into(), true)), "{got:?}");
        assert!(got.contains(&("sp ace.txt".into(), true)), "{got:?}");
        assert!(got.contains(&("*".into(), true)), "{got:?}");
        assert!(
            got.contains(&("keep.txt".into(), false)),
            "`*` matched nothing else: {got:?}"
        );
        run(&git, &root, &FileWork::Unstage(vec![p("*")])).unwrap();
        let work = FileWork::Revert {
            tracked: vec![],
            untracked: vec![p("*")],
        };
        run(&git, &root, &work).unwrap();
        assert!(!root.join("*").exists(), "the file named `*` is deleted");
        assert!(
            root.join("keep.txt").exists(),
            "and nothing a pattern would match"
        );
    }

    #[test]
    fn revert_words_say_what_is_lost() {
        let lines = |added, removed| Counts::Lines { added, removed };
        assert_eq!(
            revert_words("60 - Progress Log.md", lines(12, 1), false),
            (
                "Revert 60 - Progress Log.md?".into(),
                "Its 12 added and 1 removed line(s) since it was last staged or committed are \
                 thrown away. This can't be undone."
                    .into(),
                "Revert"
            )
        );
        assert_eq!(
            revert_words("a.md", lines(7, 0), false).1,
            "Its 7 added line(s) since it was last staged or committed are thrown away. This \
             can't be undone.",
            "a zero side is left out"
        );
        assert!(
            revert_words("a.md", lines(0, 4), false)
                .1
                .starts_with("Its 4 removed line(s) since")
        );
        let new = revert_words("new.txt", lines(3, 0), true);
        assert_eq!(new.0, "Delete new.txt?");
        assert_eq!(
            new.1,
            "It's a new file git doesn't know yet: it is deleted. This can't be undone."
        );
        assert_eq!(new.2, "Delete");
        assert_eq!(
            revert_words("x.bin", Counts::Binary, false).1,
            "Its changes since it was last staged or committed are thrown away. This can't be \
             undone."
        );
        assert_eq!(
            revert_all_words(15, 2),
            (
                "Revert 17 files?".into(),
                "15 changed file(s) go back to their last staged or committed state and 2 new \
                 file(s) are deleted. This can't be undone."
                    .into(),
                "Revert all"
            )
        );
        assert_eq!(
            revert_all_words(3, 0).1,
            "3 changed file(s) go back to their last staged or committed state. This can't be \
             undone."
        );
        assert_eq!(revert_all_words(1, 0).0, "Revert 1 file?");
    }

    #[test]
    fn staged_summary_counts_files_not_parts() {
        let entry = |path: &str, staged| Change {
            path: p(path),
            mark: Mark::Modified,
            counts: Counts::Unknown,
            folder: false,
            staged,
            untracked: false,
        };
        let list = ChangeList {
            entries: vec![
                entry("a", true),
                entry("b", true),
                entry("a", false),
                entry("c", false),
            ],
            more: 0,
            counts_failed: false,
        };
        assert_eq!(staged_summary(&list), (2, 3));
        let long = ChangeList { more: 5, ..list };
        assert_eq!(
            staged_summary(&long),
            (2, 8),
            "the files past the list count too"
        );
    }

    #[test]
    fn working_words_and_verbs() {
        let one = vec![p("a")];
        assert_eq!(
            working_text(&FileWork::Stage(one.clone())),
            "Staging\u{2026}"
        );
        assert_eq!(
            working_text(&FileWork::Unstage(one.clone())),
            "Unstaging\u{2026}"
        );
        let revert = FileWork::Revert {
            tracked: one,
            untracked: vec![],
        };
        assert_eq!(working_text(&revert), "Reverting\u{2026}");
        assert_eq!(verb(&revert), "Revert");
    }
}
