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

/// A name that may go to git as a branch: not empty, not an option (`-f`), not `@` (git's
/// HEAD, which `check-ref-format --branch` lets through), and one git itself accepts
/// (`check-ref-format --branch`, which also expands `@{-1}`: so it must print back exactly the
/// name).
fn valid_branch(git: &Path, root: &Path, name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('-')
        && name != "@"
        && run_git(git, root, &["check-ref-format", "--branch", name])
            .is_some_and(|out| out.trim_end_matches('\n') == name)
}

/// Fetch one branch from origin (a typed name that isn't in the list): `Ok(true)` if fetched,
/// `Ok(false)` if the name is invalid or origin doesn't have it.
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

/// Switch to `target`. The only command that changes the work tree is
/// `switch --no-guess <name>` to a local branch that exists (no `-c`, no `-C`, no force, no
/// guessing a branch from origin's): git refuses it whole, before touching the tree, or does
/// it. A name that could be read as an option is refused here (branch names from refs can't
/// start with `-`).
///
/// An origin branch is first made into a local one, as `--track` would make it, by commands
/// that never touch the tree: `branch --no-track <name> refs/remotes/origin/<name>` (which
/// refuses cleanly if a local branch has the name, or clashes with it as a folder:
/// `feature` against `feature/x`), then its upstream (`branch.<name>.remote` = origin,
/// `branch.<name>.merge` = `refs/heads/<name>`), which works whatever origin's fetch refspecs
/// are. Then the switch. If the upstream can't be written, or git refuses the switch, what was
/// just made is taken back (`unmake`): the repo is as it was, and git's first refusal is the
/// answer.
pub fn switch(git: &Path, root: &Path, target: &Target) -> Result<(), String> {
    let name = target.name();
    if name.starts_with('-') {
        return Err("not a branch name".to_string());
    }
    let switch_to =
        || run_git_checked(git, root, &["switch", "--no-guess", name], SWITCH_CAP, &[]).map(|_| ());
    if matches!(target, Target::Local(_)) {
        return switch_to();
    }
    let local = format!("refs/heads/{name}");
    let remote = format!("refs/remotes/origin/{name}");
    run_git_checked(
        git,
        root,
        &["branch", "--no-track", name, &remote],
        SWITCH_CAP,
        &[],
    )?;
    // The commit it was made at: only a branch still there is taken back.
    let Some(made_at) = run_git(git, root, &["rev-parse", "--verify", "-q", &local])
        .map(|oid| oid.trim().to_string())
    else {
        return Err("git could not read the branch it made".to_string());
    };
    let mut written = Vec::new();
    for (key, value) in [("remote", "origin"), ("merge", local.as_str())] {
        let key = format!("branch.{name}.{key}");
        let before = run_git(git, root, &["config", "--get", &key]).map(|v| v.trim().to_string());
        match run_git_checked(git, root, &["config", &key, value], SWITCH_CAP, &[]) {
            Ok(_) => written.push((key, before)),
            Err(e) => {
                unmake(git, root, name, &made_at, &written);
                return Err(e);
            }
        }
    }
    switch_to().inspect_err(|_| unmake(git, root, name, &made_at, &written))
}

/// Take back the branch `switch` just made (and the upstream keys it wrote, to their values
/// before): the branch is deleted only while it still points at `made_at` (git checks, in the
/// same step) and HEAD isn't on it; nothing else is ever deleted.
fn unmake(
    git: &Path,
    root: &Path,
    name: &str,
    made_at: &str,
    written: &[(String, Option<String>)],
) {
    let local = format!("refs/heads/{name}");
    let on_it = run_git(git, root, &["symbolic-ref", "-q", "HEAD"])
        .is_some_and(|head| head.trim() == local);
    if on_it {
        return;
    }
    for (key, before) in written.iter().rev() {
        let args: Vec<&str> = match before {
            Some(value) => vec!["config", key, value],
            None => vec!["config", "--unset", key],
        };
        let _ = run_git_checked(git, root, &args, SWITCH_CAP, &[]);
    }
    let _ = run_git_checked(
        git,
        root,
        &["update-ref", "-d", &local, made_at],
        SWITCH_CAP,
        &[],
    );
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

    /// git's stdout in `dir`, trimmed (empty if it failed).
    fn out(git: &Path, dir: &Path, args: &[&str]) -> String {
        let out = Command::new(git)
            .arg("-C")
            .arg(dir)
            .args(args)
            .output()
            .unwrap();
        String::from_utf8(out.stdout).unwrap().trim().to_string()
    }

    /// What a refused switch must leave as it was: HEAD, the index and work tree (status),
    /// the local branches, and the `branch.*` config.
    fn repo_state(git: &Path, dir: &Path) -> [String; 4] {
        [
            out(git, dir, &["rev-parse", "--symbolic-full-name", "HEAD"]),
            out(git, dir, &["status", "--porcelain=v1", "-uall"]),
            out(
                git,
                dir,
                &["for-each-ref", "--format=%(refname)", "refs/heads"],
            ),
            out(git, dir, &["config", "--get-regexp", "^branch\\."]),
        ]
    }

    #[test]
    fn an_origin_switch_that_clashes_with_a_local_branch_name_changes_nothing() {
        let Some(git) = find_git() else {
            return println!("skipped: no git");
        };
        let (work, _) = fixture("clash", &git);
        // Origin has `feature`; locally there is `feature/x` (and no `feature`).
        git_in(&git, &work, &["branch", "-q", "-D", "feature"]);
        git_in(&git, &work, &["branch", "feature/x"]);
        // Origin has `release/2.0`; locally there is `release`.
        git_in(
            &git,
            &work,
            &["branch", "release/2.0", "refs/heads/feature/x"],
        );
        git_in(&git, &work, &["push", "-q", "origin", "release/2.0"]);
        git_in(&git, &work, &["branch", "-q", "-D", "release/2.0"]);
        git_in(&git, &work, &["branch", "release"]);
        // The user's own changes (none the targets touch), which must stay exactly as they are.
        fs::write(work.join("notes.txt"), "mine\n").unwrap();
        git_in(&git, &work, &["add", "notes.txt"]);
        fs::write(work.join("todo.txt"), "mine\n").unwrap();
        let before = repo_state(&git, &work);
        for name in ["feature", "release/2.0"] {
            let err = switch(&git, &work, &Target::Origin(name.into())).unwrap_err();
            assert!(err.contains("cannot lock ref"), "{name}: {err}");
            assert_eq!(repo_state(&git, &work), before, "{name}: nothing changed");
            assert_eq!(fs::read_to_string(work.join("src/a.rs")).unwrap(), "one\n");
        }
    }

    #[test]
    fn an_origin_switch_refused_for_overlap_leaves_no_branch_behind() {
        let Some(git) = find_git() else {
            return println!("skipped: no git");
        };
        let (work, _) = fixture("overlap", &git);
        git_in(&git, &work, &["branch", "-q", "-D", "feature"]);
        fs::write(work.join("src/a.rs"), "mine\n").unwrap();
        let before = repo_state(&git, &work);
        let err = switch(&git, &work, &Target::Origin("feature".into())).unwrap_err();
        assert!(err.contains("would be overwritten"), "{err}");
        assert_eq!(
            repo_state(&git, &work),
            before,
            "no feature, no config left"
        );
        assert_eq!(fs::read_to_string(work.join("src/a.rs")).unwrap(), "mine\n");
    }

    #[test]
    fn an_origin_switch_whose_upstream_cant_be_written_takes_its_branch_back() {
        let Some(git) = find_git() else {
            return println!("skipped: no git");
        };
        let (work, _) = fixture("config-fails", &git);
        git_in(&git, &work, &["branch", "-q", "-D", "feature"]);
        // Two values: git won't overwrite them with one (after `remote` was written).
        for value in ["refs/heads/a", "refs/heads/b"] {
            git_in(
                &git,
                &work,
                &["config", "--add", "branch.feature.merge", value],
            );
        }
        let before = repo_state(&git, &work);
        let err = switch(&git, &work, &Target::Origin("feature".into())).unwrap_err();
        assert!(err.contains("multiple values"), "{err}");
        assert_eq!(
            repo_state(&git, &work),
            before,
            "no feature; remote unset again, merge's values kept"
        );
    }

    #[test]
    fn an_origin_switch_works_whatever_origins_fetch_refspec() {
        let Some(git) = find_git() else {
            return println!("skipped: no git");
        };
        let (work, origin) = fixture("refspecs", &git);
        let base = work.parent().unwrap().to_path_buf();
        // No `:` (fetches without storing; git takes no glob without one), and one storing
        // under another remote's folder.
        for (clone, spec) in [
            ("no-colon", "+refs/heads/feature"),
            ("elsewhere", "+refs/heads/*:refs/remotes/upstream/*"),
        ] {
            git_in(
                &git,
                &base,
                &[
                    "clone",
                    "-q",
                    "-b",
                    "trunk",
                    origin.to_str().unwrap(),
                    clone,
                ],
            );
            let dir = base.join(clone);
            git_in(&git, &dir, &["config", "remote.origin.fetch", spec]);
            git_in(
                &git,
                &dir,
                &["update-ref", "-d", "refs/remotes/origin/feature"],
            );
            fs::write(dir.join("notes.txt"), "mine\n").unwrap();
            assert_eq!(
                fetch_branch(&git, &dir, "feature", FetchMode::Auto),
                Ok(true),
                "{clone}"
            );
            let before = repo_state(&git, &dir);
            match switch(&git, &dir, &Target::Origin("feature".into())) {
                Ok(()) => {
                    assert_eq!(head_name(&git, &dir), "feature", "{clone}");
                    assert_eq!(
                        out(&git, &dir, &["status", "--porcelain"]),
                        "?? notes.txt",
                        "{clone}: a clean switch"
                    );
                    assert_eq!(
                        out(&git, &dir, &["config", "branch.feature.merge"]),
                        "refs/heads/feature",
                        "{clone}"
                    );
                }
                Err(e) => assert_eq!(repo_state(&git, &dir), before, "{clone}: {e}"),
            }
        }
    }

    #[test]
    fn an_origin_switch_names_origins_ref_not_a_local_branch_called_origin_slash() {
        let Some(git) = find_git() else {
            return println!("skipped: no git");
        };
        let (work, _) = fixture("ambiguous", &git);
        git_in(&git, &work, &["branch", "-q", "-D", "feature"]);
        // A local branch literally named `origin/feature`, on trunk.
        git_in(&git, &work, &["branch", "origin/feature", "trunk"]);
        assert_eq!(
            switch(&git, &work, &Target::Origin("feature".into())),
            Ok(())
        );
        assert_eq!(head_name(&git, &work), "feature");
        assert_eq!(
            out(&git, &work, &["rev-parse", "refs/heads/feature"]),
            out(&git, &work, &["rev-parse", "refs/remotes/origin/feature"]),
            "made from origin's feature"
        );
    }

    #[test]
    fn a_local_switch_never_guesses_a_vanished_branch_from_origin() {
        let Some(git) = find_git() else {
            return println!("skipped: no git");
        };
        let (work, _) = fixture("no-guess", &git);
        // Listed as local, gone by the time the user confirms; origin still has it.
        git_in(&git, &work, &["branch", "-q", "-D", "feature"]);
        let before = repo_state(&git, &work);
        assert!(switch(&git, &work, &Target::Local("feature".into())).is_err());
        assert_eq!(
            repo_state(&git, &work),
            before,
            "no branch made by guessing"
        );
    }

    #[test]
    fn at_is_no_branch_name() {
        let Some(git) = find_git() else {
            return println!("skipped: no git");
        };
        let (work, _) = fixture("at", &git);
        assert!(!valid_branch(&git, &work, "@"));
        assert_eq!(fetch_branch(&git, &work, "@", FetchMode::Auto), Ok(false));
    }

    #[test]
    fn switch_tracks_an_origin_branch_a_narrow_clone_fetched_alone() {
        let Some(git) = find_git() else {
            return println!("skipped: no git");
        };
        let (work, origin) = fixture("narrow", &git);
        let base = work.parent().unwrap().to_path_buf();
        git_in(
            &git,
            &base,
            &[
                "clone",
                "-q",
                "--single-branch",
                "-b",
                "trunk",
                origin.to_str().unwrap(),
                "narrow",
            ],
        );
        let narrow = base.join("narrow");
        assert_eq!(
            fetch_branch(&git, &narrow, "feature", FetchMode::Auto),
            Ok(true)
        );
        assert_eq!(
            switch(&git, &narrow, &Target::Origin("feature".into())),
            Ok(())
        );
        assert_eq!(head_name(&git, &narrow), "feature");
        let run = |args: &[&str]| {
            let out = Command::new(&git)
                .arg("-C")
                .arg(&narrow)
                .args(args)
                .output()
                .unwrap();
            String::from_utf8(out.stdout).unwrap().trim().to_string()
        };
        assert_eq!(run(&["status", "--porcelain"]), "", "a clean switch");
        assert_eq!(
            run(&["config", "branch.feature.merge"]),
            "refs/heads/feature"
        );
        assert_eq!(run(&["config", "branch.feature.remote"]), "origin");
        // git maps an upstream through the fetch refspec, which a narrow clone's doesn't
        // cover: the branch counts as on origin by name instead (even with it).
        let snap = read(&git, &narrow).unwrap();
        assert!(snap.local.iter().any(|b| b.name == "feature"));
        assert_eq!(snap.pushed.get("feature"), Some(&(0, 0)));
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
