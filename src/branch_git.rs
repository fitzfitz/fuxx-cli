//! The branch switcher's git (note 66 §2, §5, §6): one read of the repo's branches and dirt,
//! the files a switch would touch, the auto/button fetch, and the switch itself. The first
//! fuxx feature that changes a user's repo, so every command is spelled out and a name typed
//! by the user is never put where git could read it as an option.

use crate::branch_list::{Head, LocalRef, RemoteRef, Target, parse_refs};
use crate::git_branch::{FETCH_CAP, SWITCH_CAP, run_git, run_git_bytes, run_git_checked};
use crate::git_status::parse_porcelain;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Everything the modal shows, read in one go.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    /// The repo's top folder; every path below is relative to it.
    pub root: PathBuf,
    pub head: Head,
    pub local: Vec<LocalRef>,
    pub origin: Vec<RemoteRef>,
    /// Local branches with no upstream but a same-named origin ref: (ahead, behind) of it.
    pub pushed: HashMap<String, (u32, u32)>,
    pub has_origin: bool,
    /// Changed or untracked paths, root-relative.
    pub dirty: HashSet<PathBuf>,
    /// Commits only HEAD reaches; counted only when detached.
    pub orphans: usize,
}

const REFS_FORMAT: &str =
    "--format=%(refname)%00%(upstream)%00%(upstream:track)%00%(committerdate:unix)";

/// `None` outside a repo (or when git fails).
pub fn read(git: &Path, dir: &Path) -> Option<Snapshot> {
    let root =
        PathBuf::from(run_git(git, dir, &["rev-parse", "--show-toplevel"])?.trim_end_matches('\n'));
    let head = match run_git(git, &root, &["symbolic-ref", "-q", "--short", "HEAD"]) {
        Some(name) => {
            let name = name.trim().to_string();
            if run_git(git, &root, &["rev-parse", "-q", "--verify", "HEAD"]).is_some() {
                Head::Branch(name)
            } else {
                Head::Unborn(name)
            }
        }
        None => Head::Detached(
            run_git(git, &root, &["rev-parse", "--short", "HEAD"])?
                .trim()
                .to_string(),
        ),
    };
    let refs = run_git(
        git,
        &root,
        &[
            "for-each-ref",
            REFS_FORMAT,
            "refs/heads",
            "refs/remotes/origin",
        ],
    )?;
    let (local, origin) = parse_refs(&refs);
    let mut pushed = HashMap::new();
    for branch in local.iter().filter(|b| b.upstream.is_none()) {
        if origin.iter().any(|o| o.name == branch.name) {
            let range = format!("refs/heads/{0}...refs/remotes/origin/{0}", branch.name);
            if let Some(counts) =
                run_git(git, &root, &["rev-list", "--left-right", "--count", &range])
            {
                let mut n = counts.split_whitespace().map(|n| n.parse::<u32>());
                if let (Some(Ok(ahead)), Some(Ok(behind))) = (n.next(), n.next()) {
                    pushed.insert(branch.name.clone(), (ahead, behind));
                }
            }
        }
    }
    let has_origin = run_git(git, &root, &["remote"])?
        .lines()
        .any(|r| r == "origin");
    // At the root, so every path is root-relative whatever folder `dir` is in; `-uall` so a
    // file in a new folder is listed itself, not as `folder/`; `--no-renames` so a staged rename
    // lists its old path too (git refuses a switch that would touch it).
    let dirty = run_git_bytes(
        git,
        &root,
        &["status", "--porcelain=v1", "-z", "-uall", "--no-renames"],
    )
    .success()
    .map(|z| parse_porcelain(&z).into_iter().map(|(p, _)| p).collect())?;
    let orphans = if matches!(head, Head::Detached(_)) {
        run_git(
            git,
            &root,
            &[
                "rev-list",
                "--count",
                "HEAD",
                "--not",
                "--branches",
                "--remotes",
            ],
        )
        .and_then(|n| n.trim().parse().ok())
        .unwrap_or(0)
    } else {
        0
    };
    Some(Snapshot {
        root,
        head,
        local,
        origin,
        pushed,
        has_origin,
        dirty,
        orphans,
    })
}

/// The files that differ between HEAD and `target` (root-relative): where a switch could
/// collide with the working tree's changes. `None` if git fails.
pub fn changed(git: &Path, root: &Path, target: &Target) -> Option<HashSet<PathBuf>> {
    let reference = match target {
        Target::Local(name) => format!("refs/heads/{name}"),
        Target::Origin(name) => format!("refs/remotes/origin/{name}"),
    };
    let out = run_git_bytes(
        git,
        root,
        &[
            "diff",
            "--name-only",
            "-z",
            "--no-renames",
            "HEAD",
            &reference,
            "--",
        ],
    )
    .success()?;
    Some(
        out.split(|&b| b == 0)
            .filter(|p| !p.is_empty())
            .map(|p| {
                use std::os::unix::ffi::OsStrExt;
                PathBuf::from(std::ffi::OsStr::from_bytes(p))
            })
            .collect(),
    )
}

/// Who may ask the user for credentials: `Auto` (on opening the modal) must never open a
/// window; `Button` (the user asked) may let a credential helper open its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FetchMode {
    Auto,
    Button,
}

impl FetchMode {
    fn env(self) -> &'static [(&'static str, &'static str)] {
        match self {
            FetchMode::Auto => &[
                ("GCM_INTERACTIVE", "never"),
                ("GIT_ASKPASS", "/usr/bin/false"),
                ("SSH_ASKPASS_REQUIRE", "never"),
            ],
            FetchMode::Button => &[],
        }
    }
}

/// `fetch --prune origin`.
pub fn fetch(git: &Path, root: &Path, mode: FetchMode) -> Result<(), String> {
    run_git_checked(
        git,
        root,
        &["fetch", "--prune", "origin"],
        FETCH_CAP,
        mode.env(),
    )
    .map(|_| ())
}

/// A name that may go to git as a branch: not empty, not an option (`-f`), and one git itself
/// accepts (`check-ref-format --branch`, which also expands `@{-1}`: so it must print back
/// exactly the name).
fn valid_branch(git: &Path, root: &Path, name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('-')
        && run_git(git, root, &["check-ref-format", "--branch", name])
            .is_some_and(|out| out.trim_end_matches('\n') == name)
}

/// Fetch one branch from origin (a typed name that isn't in the list): `Ok(true)` if fetched,
/// `Ok(false)` if the name is invalid or origin doesn't have it.
#[allow(dead_code)] // the typed name's row (M11o Task 4)
pub fn fetch_branch(git: &Path, root: &Path, name: &str, mode: FetchMode) -> Result<bool, String> {
    if !valid_branch(git, root, name) {
        return Ok(false);
    }
    let spec = format!("refs/heads/{name}:refs/remotes/origin/{name}");
    match run_git_checked(
        git,
        root,
        &["fetch", "origin", &spec],
        FETCH_CAP,
        mode.env(),
    ) {
        Ok(_) => Ok(true),
        Err(e) if e.contains("couldn't find remote ref") => Ok(false),
        Err(e) => Err(e),
    }
}

/// `switch <name>` or `switch --track origin/<name>`, never another flag. A local name that
/// could be read as an option is refused here (branch names from refs can't start with `-`).
#[allow(dead_code)] // the confirmation's Switch (M11o Task 4)
pub fn switch(git: &Path, root: &Path, target: &Target) -> Result<(), String> {
    if target.name().starts_with('-') {
        return Err("not a branch name".to_string());
    }
    let remote;
    let args: Vec<&str> = match target {
        Target::Local(name) => vec!["switch", name],
        Target::Origin(name) => {
            remote = format!("origin/{name}");
            vec!["switch", "--track", &remote]
        }
    };
    run_git_checked(git, root, &args, SWITCH_CAP, &[]).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::branch_list::Track;
    use crate::git_branch::find_git;
    use std::fs;
    use std::process::Command;

    fn temp(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("fuxx-branch-git-{name}-{}", std::process::id()));
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

    fn commit_file(git: &Path, root: &Path, file: &str, text: &str) {
        let path = root.join(file);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, text).unwrap();
        git_in(git, root, &["add", "-A"]);
        git_in(git, root, &["commit", "-q", "-m", file]);
    }

    /// `work` with `trunk` and `feature` (which changes `src/a.rs`), a bare local `origin`
    /// holding both, `feature` tracking it, and `only-on-origin` (origin only, not fetched).
    fn fixture(name: &str, git: &Path) -> (PathBuf, PathBuf) {
        let base = temp(name);
        let (work, origin) = (base.join("work"), base.join("origin.git"));
        fs::create_dir_all(&work).unwrap();
        git_in(
            git,
            &base,
            &["init", "-q", "--bare", "-b", "trunk", "origin.git"],
        );
        git_in(git, &work, &["init", "-q", "-b", "trunk"]);
        commit_file(git, &work, "src/a.rs", "one\n");
        git_in(git, &work, &["switch", "-q", "-c", "feature"]);
        commit_file(git, &work, "src/a.rs", "two\n");
        git_in(git, &work, &["switch", "-q", "trunk"]);
        git_in(
            git,
            &work,
            &["remote", "add", "origin", origin.to_str().unwrap()],
        );
        git_in(git, &work, &["push", "-q", "origin", "trunk"]);
        git_in(git, &work, &["push", "-q", "-u", "origin", "feature"]);
        git_in(git, &work, &["branch", "only-on-origin"]);
        git_in(git, &work, &["push", "-q", "origin", "only-on-origin"]);
        git_in(git, &work, &["branch", "-q", "-D", "only-on-origin"]);
        git_in(
            git,
            &work,
            &["update-ref", "-d", "refs/remotes/origin/only-on-origin"],
        );
        (work, origin)
    }

    fn head_name(git: &Path, root: &Path) -> String {
        let out = Command::new(git)
            .arg("-C")
            .arg(root)
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .output()
            .unwrap();
        String::from_utf8(out.stdout).unwrap().trim().to_string()
    }

    #[test]
    fn read_lists_branches_tracking_and_dirt() {
        let Some(git) = find_git() else {
            return println!("skipped: no git");
        };
        let (work, _) = fixture("read", &git);
        git_in(&git, &work, &["switch", "-q", "feature"]);
        commit_file(&git, &work, "src/b.rs", "x\n");
        fs::write(work.join("src/a.rs"), "changed\n").unwrap();
        fs::write(work.join("new.txt"), "n\n").unwrap();
        let snap = read(&git, &work).unwrap();
        assert_eq!(
            snap.root.canonicalize().unwrap(),
            work.canonicalize().unwrap()
        );
        assert_eq!(snap.head, Head::Branch("feature".into()));
        let feature = snap.local.iter().find(|b| b.name == "feature").unwrap();
        assert_eq!(feature.upstream.as_deref(), Some("origin/feature"));
        assert_eq!(
            feature.track,
            Track::Diverged {
                ahead: 1,
                behind: 0
            }
        );
        assert!(snap.has_origin);
        assert_eq!(snap.pushed.get("trunk"), Some(&(0, 0)));
        assert!(snap.dirty.contains(Path::new("new.txt")));
        assert!(snap.dirty.contains(Path::new("src/a.rs")));
        assert_eq!(snap.orphans, 0);
    }

    #[test]
    fn paths_are_repo_relative_from_a_subfolder() {
        let Some(git) = find_git() else {
            return println!("skipped: no git");
        };
        let (work, _) = fixture("subfolder", &git);
        fs::write(work.join("src/a.rs"), "changed\n").unwrap();
        let snap = read(&git, &work.join("src")).unwrap();
        assert_eq!(snap.dirty, HashSet::from([PathBuf::from("src/a.rs")]));
        let changed = changed(&git, &snap.root, &Target::Local("feature".into())).unwrap();
        assert_eq!(changed, HashSet::from([PathBuf::from("src/a.rs")]));
    }

    #[test]
    fn an_unborn_repo_and_no_origin() {
        let Some(git) = find_git() else {
            return println!("skipped: no git");
        };
        let dir = temp("unborn");
        git_in(&git, &dir, &["init", "-q", "-b", "trunk"]);
        let snap = read(&git, &dir).unwrap();
        assert_eq!(snap.head, Head::Unborn("trunk".into()));
        assert!(snap.local.is_empty() && snap.origin.is_empty());
        assert!(!snap.has_origin);
        assert_eq!(read(&git, &temp("not-a-repo")), None);
    }

    #[test]
    fn a_detached_head_counts_its_orphans() {
        let Some(git) = find_git() else {
            return println!("skipped: no git");
        };
        let (work, _) = fixture("detached", &git);
        git_in(&git, &work, &["switch", "-q", "--detach"]);
        commit_file(&git, &work, "d.txt", "d\n");
        let snap = read(&git, &work).unwrap();
        assert!(matches!(snap.head, Head::Detached(_)));
        assert_eq!(snap.orphans, 1);
    }

    #[test]
    fn fetch_prunes_and_fetch_branch_finds_or_not() {
        let Some(git) = find_git() else {
            return println!("skipped: no git");
        };
        let (work, origin) = fixture("fetch", &git);
        git_in(&git, &work, &["branch", "doomed"]);
        git_in(&git, &work, &["push", "-q", "origin", "doomed"]);
        git_in(&git, &origin, &["branch", "-q", "-D", "doomed"]);
        let has = |name: &str| {
            read(&git, &work)
                .unwrap()
                .origin
                .iter()
                .any(|o| o.name == name)
        };
        assert!(has("doomed"));
        assert_eq!(fetch(&git, &work, FetchMode::Auto), Ok(()));
        assert!(!has("doomed"));
        // The fetch brought every origin branch; drop this one's ref to fetch it alone.
        git_in(
            &git,
            &work,
            &["update-ref", "-d", "refs/remotes/origin/only-on-origin"],
        );
        assert!(!has("only-on-origin"));
        assert_eq!(
            fetch_branch(&git, &work, "only-on-origin", FetchMode::Auto),
            Ok(true)
        );
        assert!(has("only-on-origin"));
        assert_eq!(
            fetch_branch(&git, &work, "nope", FetchMode::Auto),
            Ok(false)
        );
        assert_eq!(fetch_branch(&git, &work, "-f", FetchMode::Auto), Ok(false));
        assert_eq!(
            fetch_branch(&git, &work, "a..b", FetchMode::Auto),
            Ok(false)
        );
        assert_eq!(fetch_branch(&git, &work, "", FetchMode::Button), Ok(false));
        for bad in ["@{-1}", "foo bar"] {
            assert!(!valid_branch(&git, &work, bad), "{bad}");
            assert_eq!(fetch_branch(&git, &work, bad, FetchMode::Auto), Ok(false));
        }
    }

    #[test]
    fn switch_moves_and_refuses_with_gits_message() {
        let Some(git) = find_git() else {
            return println!("skipped: no git");
        };
        let (work, _) = fixture("switch", &git);
        assert_eq!(
            switch(&git, &work, &Target::Local("feature".into())),
            Ok(())
        );
        assert_eq!(head_name(&git, &work), "feature");
        assert_eq!(
            fetch_branch(&git, &work, "only-on-origin", FetchMode::Auto),
            Ok(true)
        );
        assert_eq!(
            switch(&git, &work, &Target::Origin("only-on-origin".into())),
            Ok(())
        );
        assert_eq!(head_name(&git, &work), "only-on-origin");
        let snap = read(&git, &work).unwrap();
        let made = snap
            .local
            .iter()
            .find(|b| b.name == "only-on-origin")
            .unwrap();
        assert_eq!(made.upstream.as_deref(), Some("origin/only-on-origin"));
        assert_eq!(switch(&git, &work, &Target::Local("trunk".into())), Ok(()));
        fs::write(work.join("src/a.rs"), "mine\n").unwrap();
        let err = switch(&git, &work, &Target::Local("feature".into())).unwrap_err();
        assert!(err.contains("would be overwritten"), "{err}");
        assert_eq!(head_name(&git, &work), "trunk");
        assert_eq!(fs::read_to_string(work.join("src/a.rs")).unwrap(), "mine\n");
        assert!(switch(&git, &work, &Target::Local("-f".into())).is_err());
    }

    #[test]
    fn an_untracked_file_in_a_new_folder_is_dirty_and_collides() {
        let Some(git) = find_git() else {
            return println!("skipped: no git");
        };
        let (work, _) = fixture("newdir", &git);
        git_in(&git, &work, &["switch", "-q", "-c", "adds"]);
        commit_file(&git, &work, "newdir/x.rs", "theirs\n");
        git_in(&git, &work, &["switch", "-q", "trunk"]);
        fs::create_dir_all(work.join("newdir")).unwrap();
        fs::write(work.join("newdir/x.rs"), "mine\n").unwrap();
        let snap = read(&git, &work).unwrap();
        assert_eq!(snap.dirty, HashSet::from([PathBuf::from("newdir/x.rs")]));
        let changed = changed(&git, &snap.root, &Target::Local("adds".into())).unwrap();
        assert!(changed.contains(Path::new("newdir/x.rs")));
        assert_eq!(snap.dirty.intersection(&changed).count(), 1);
    }

    #[test]
    fn a_staged_rename_counts_both_paths_as_dirty() {
        let Some(git) = find_git() else {
            return println!("skipped: no git");
        };
        let (work, _) = fixture("rename", &git);
        git_in(&git, &work, &["mv", "src/a.rs", "src/b.rs"]);
        let snap = read(&git, &work).unwrap();
        assert!(snap.dirty.contains(Path::new("src/a.rs")));
        assert!(snap.dirty.contains(Path::new("src/b.rs")));
        let changed = changed(&git, &snap.root, &Target::Local("feature".into())).unwrap();
        assert!(changed.contains(Path::new("src/a.rs")));
        assert!(snap.dirty.intersection(&changed).count() >= 1);
    }

    #[test]
    fn no_origin_fetch_fails_cleanly() {
        let Some(git) = find_git() else {
            return println!("skipped: no git");
        };
        let dir = temp("no-origin");
        git_in(&git, &dir, &["init", "-q", "-b", "trunk"]);
        assert!(fetch(&git, &dir, FetchMode::Auto).is_err());
    }
}
