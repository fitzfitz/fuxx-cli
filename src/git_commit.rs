//! The Changes tab's commit (M11aq, note 92): commit the staged files, say what stops a commit
//! (a merge or rebase in progress), and the staged diff an agent writes the message from (✦),
//! without files that look like secrets. Blocking: background work.
//!
//! Never `--no-verify`, `--amend` or force: a hook that refuses is the commit refused, its output
//! kept for you.

// Wired in by M11aq Task 3 (the commit line) and Task 4 (✦); until then only the tests call it.
#![allow(dead_code)]

use std::path::Path;
use std::time::Duration;

use crate::git_branch::{Ran, run_capped_streams, run_git, run_git_bytes};

/// The most of the staged diff an agent is sent (✦).
pub const DIFF_CAP: usize = 24 * 1024;
/// How long a commit may take: its hooks may lint or test.
const COMMIT_CAP: Duration = Duration::from_secs(600);

/// Commit the staged files with `message` (one argument: quotes, newlines, a leading `-` are
/// kept as written). Err: git's whole output (a hook's log, or why there was nothing to commit).
pub fn commit(git: &Path, root: &Path, message: &str) -> Result<(), String> {
    let mut command = std::process::Command::new(git);
    command
        .arg("-C")
        .arg(root)
        .args(["commit", "-m", message])
        .env("GIT_TERMINAL_PROMPT", "0");
    match run_capped_streams(command, None, COMMIT_CAP, true) {
        Ran::Exited(Some(0), _) => Ok(()),
        // A hook writes to stderr (git sends its stdout there too); "nothing to commit" comes on
        // stdout: both, as they came.
        Ran::Exited(_, (out, err)) => {
            let said = [err, out]
                .iter()
                .map(|b| String::from_utf8_lossy(b).trim().to_string())
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join("\n");
            Err(if said.is_empty() {
                "git failed".into()
            } else {
                said
            })
        }
        Ran::TimedOut => Err("the commit took too long (its hooks ran past 10 minutes)".into()),
        Ran::Failed => Err("git could not run".into()),
    }
}

/// What stops a commit now: a merge, rebase, cherry-pick or revert in progress (its name).
pub fn in_progress(git: &Path, root: &Path) -> Option<&'static str> {
    [
        ("MERGE_HEAD", "a merge"),
        ("rebase-merge", "a rebase"),
        ("rebase-apply", "a rebase"),
        ("CHERRY_PICK_HEAD", "a cherry-pick"),
        ("REVERT_HEAD", "a revert"),
    ]
    .into_iter()
    .find(|(name, _)| {
        run_git(git, root, &["rev-parse", "--git-path", name]).is_some_and(|path| {
            let path = Path::new(path.trim());
            root.join(path).exists()
        })
    })
    .map(|(_, what)| what)
}

/// The staged diff an agent is sent: without files that look like secrets (named in `skipped`),
/// cut at a line end to stay within the cap (`cut`).
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct StagedDiff {
    pub text: String,
    pub skipped: Vec<String>,
    pub cut: bool,
}

/// The staged diff in `root`, at most `cap` bytes.
pub fn staged_diff(git: &Path, root: &Path, cap: usize) -> Result<StagedDiff, String> {
    let names = run_git(git, root, &["diff", "--cached", "--name-only", "-z"])
        .ok_or("git could not list the staged files")?;
    let (skipped, sent): (Vec<&str>, Vec<&str>) = names
        .split('\0')
        .filter(|n| !n.is_empty())
        .partition(|n| is_secret(n));
    let mut diff = StagedDiff {
        skipped: skipped.iter().map(|s| s.to_string()).collect(),
        ..StagedDiff::default()
    };
    if sent.is_empty() {
        return Ok(diff);
    }
    let mut args = vec![
        "--literal-pathspecs",
        "diff",
        "--cached",
        "--no-color",
        "--no-ext-diff",
        "--",
    ];
    args.extend(sent);
    let text = run_git_bytes(git, root, &args)
        .success()
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .ok_or("git could not read the staged changes")?;
    if text.len() > cap {
        let end = text[..cap].rfind('\n').map_or(0, |i| i + 1);
        diff.text = text[..end].to_string();
        diff.cut = true;
    } else {
        diff.text = text;
    }
    Ok(diff)
}

/// Whether `path` looks like a file of secrets (never sent to an agent).
pub fn is_secret(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path).to_ascii_lowercase();
    name.starts_with(".env")
        || ["id_rsa", "id_ed25519", "id_ecdsa", "id_dsa"]
            .iter()
            .any(|key| name.starts_with(key))
        || [".pem", ".key", ".p12", ".pfx"]
            .iter()
            .any(|ext| name.ends_with(ext))
        || name.contains("secret")
        || name.contains("credential")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::process::Command;

    fn temp(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("fuxx-git-commit-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn git_in(git: &Path, dir: &Path, args: &[&str]) -> String {
        let out = Command::new(git)
            .arg("-C")
            .arg(dir)
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    /// A repo with `a.txt` committed, its own identity and no signing (as yours might sign).
    fn repo(name: &str, git: &Path) -> PathBuf {
        let root = temp(name);
        git_in(git, &root, &["init", "-q", "-b", "main"]);
        for (k, v) in [
            ("user.name", "t"),
            ("user.email", "t@t"),
            ("commit.gpgsign", "false"),
            ("core.hooksPath", ".git/hooks"),
        ] {
            git_in(git, &root, &["config", k, v]);
        }
        fs::write(root.join("a.txt"), "1\n").unwrap();
        git_in(git, &root, &["add", "-A"]);
        git_in(git, &root, &["commit", "-q", "-m", "first"]);
        root
    }

    fn last_message(git: &Path, root: &Path) -> String {
        git_in(git, root, &["log", "-1", "--format=%B"])
            .trim_end()
            .to_string()
    }

    #[test]
    fn commit_takes_the_staged_files_only() {
        let Some(git) = crate::git_branch::find_git() else {
            return;
        };
        let root = repo("staged", &git);
        fs::write(root.join("a.txt"), "1\n2\n").unwrap();
        fs::write(root.join("b.txt"), "b\n").unwrap();
        git_in(&git, &root, &["add", "--", "b.txt"]);
        commit(&git, &root, "add b").expect("commits");
        assert_eq!(last_message(&git, &root), "add b");
        let shown = git_in(&git, &root, &["show", "--name-only", "--format=", "HEAD"]);
        assert_eq!(shown.trim(), "b.txt", "only what was staged");
        let left = git_in(&git, &root, &["status", "--porcelain"]);
        assert_eq!(left.trim(), "M a.txt", "the rest still changed");
    }

    #[test]
    fn a_message_with_quotes_and_a_dash_is_kept() {
        let Some(git) = crate::git_branch::find_git() else {
            return;
        };
        let root = repo("quotes", &git);
        fs::write(root.join("a.txt"), "2\n").unwrap();
        git_in(&git, &root, &["add", "-A"]);
        let message = "-x \"quoted\" 'and' $HOME\nsecond line";
        commit(&git, &root, message).expect("commits");
        assert_eq!(last_message(&git, &root), message);
    }

    #[test]
    fn a_failing_hook_returns_its_output() {
        let Some(git) = crate::git_branch::find_git() else {
            return;
        };
        let root = repo("hook", &git);
        let hook = root.join(".git/hooks/pre-commit");
        fs::write(&hook, "#!/bin/sh\necho nope-from-the-hook\nexit 1\n").unwrap();
        Command::new("chmod").arg("+x").arg(&hook).status().unwrap();
        fs::write(root.join("a.txt"), "2\n").unwrap();
        git_in(&git, &root, &["add", "-A"]);
        let err = commit(&git, &root, "refused").expect_err("the hook refuses");
        assert!(err.contains("nope-from-the-hook"), "its output: {err}");
        assert_eq!(last_message(&git, &root), "first", "nothing committed");
    }

    #[test]
    fn nothing_staged_is_refused_with_why() {
        let Some(git) = crate::git_branch::find_git() else {
            return;
        };
        let root = repo("empty", &git);
        let err = commit(&git, &root, "nothing").expect_err("nothing to commit");
        assert!(!err.trim().is_empty(), "says why");
    }

    #[test]
    fn a_merge_in_progress_is_named() {
        let Some(git) = crate::git_branch::find_git() else {
            return;
        };
        let root = repo("merge", &git);
        assert_eq!(in_progress(&git, &root), None, "a clean repo");
        git_in(&git, &root, &["checkout", "-q", "-b", "side"]);
        fs::write(root.join("a.txt"), "side\n").unwrap();
        git_in(&git, &root, &["commit", "-q", "-am", "side"]);
        git_in(&git, &root, &["checkout", "-q", "main"]);
        fs::write(root.join("a.txt"), "main\n").unwrap();
        git_in(&git, &root, &["commit", "-q", "-am", "main"]);
        // The conflicting merge stops half way (its failure is the point).
        let _ = Command::new(&git)
            .arg("-C")
            .arg(&root)
            .args(["merge", "side"])
            .output();
        assert_eq!(in_progress(&git, &root), Some("a merge"));
    }

    #[test]
    fn the_staged_diff_leaves_secrets_out_and_is_capped() {
        let Some(git) = crate::git_branch::find_git() else {
            return;
        };
        let root = repo("diff", &git);
        fs::write(root.join("a.txt"), "1\nchanged\n").unwrap();
        fs::write(root.join(".env"), "TOKEN=abc\n").unwrap();
        git_in(&git, &root, &["add", "-A"]);
        let diff = staged_diff(&git, &root, DIFF_CAP).expect("reads");
        assert!(diff.text.contains("+changed"), "{}", diff.text);
        assert!(!diff.text.contains("TOKEN"), "no secret sent");
        assert_eq!(diff.skipped, [".env"]);
        assert!(!diff.cut);
        let small = staged_diff(&git, &root, 40).expect("reads");
        assert!(small.cut && small.text.len() <= 40, "{small:?}");
        assert!(
            small.text.is_empty() || small.text.ends_with('\n'),
            "on a line end"
        );
    }

    #[test]
    fn secrets_are_recognised_by_name() {
        for yes in [
            ".env",
            "config/.env.local",
            "id_rsa",
            "keys/id_ed25519.pub",
            "a.pem",
            "server.key",
            "aws_credentials.json",
            "my-secret.txt",
        ] {
            assert!(is_secret(yes), "{yes}");
        }
        for no in ["src/env.rs", "README.md", "keyboard.rs"] {
            assert!(!is_secret(no), "{no}");
        }
    }
}
