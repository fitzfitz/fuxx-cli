//! The Changes tab's branch bar (M11v, note 73): where the followed terminal's branch stands
//! against origin, read from local git only (no network), the one action worth a button
//! (Pull, Push or Publish) and the words the bar says.
//!
//! `read` and `run` are blocking (background work); everything else is pure and tested.
//!
//! The actions never lose work: Pull is fast-forward only, Push and Publish never force, and
//! no command takes a typed name (Publish pushes `HEAD`). Git's refusal is the answer.

use crate::branch_git::{FetchMode, fetch};
use crate::branch_list::{Head, Track, parse_track};
use crate::git_branch::{FETCH_CAP, SWITCH_CAP, run_git, run_git_checked_all};
use std::path::Path;
use std::time::SystemTime;

/// The branch's upstream, as git tracks it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Upstream {
    /// No upstream set.
    None,
    /// Set to a branch that isn't origin's (`upstream/main`, a local branch): the bar offers
    /// nothing for it (only origin is in M11v).
    Other(String),
    /// Set, but origin's branch is gone.
    Gone,
    Track {
        ahead: u32,
        behind: u32,
    },
}

/// Where the branch stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sync {
    pub head: Head,
    pub upstream: Upstream,
    pub has_origin: bool,
    /// When origin was last fetched here (`FETCH_HEAD`'s time); none: never.
    pub fetched: Option<SystemTime>,
}

/// What a bar button does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Fetch,
    Pull(u32),
    Push(u32),
    Publish,
}

/// The line's colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Faint,
    Good,
    Warn,
    Bad,
}

/// After this long since the last fetch, showing a repo fetches it once (`FetchMode::Auto`).
pub const AUTO_FETCH_AFTER: u64 = 300;

/// The upstream from `for-each-ref`'s `%(upstream)` and `%(upstream:track)` fields.
pub fn upstream_of(upstream: &str, track: &str) -> Upstream {
    let upstream = upstream.trim();
    if upstream.is_empty() {
        return Upstream::None;
    }
    if !upstream.starts_with("refs/remotes/origin/") {
        let short = upstream
            .strip_prefix("refs/remotes/")
            .or_else(|| upstream.strip_prefix("refs/heads/"))
            .unwrap_or(upstream);
        return Upstream::Other(short.to_string());
    }
    match parse_track(track) {
        Track::Gone => Upstream::Gone,
        Track::Diverged { ahead, behind } => Upstream::Track { ahead, behind },
        Track::Even | Track::None => Upstream::Track {
            ahead: 0,
            behind: 0,
        },
    }
}

/// The one accent button: Pull when only behind, Push when only ahead, Publish for a branch
/// with no upstream (origin there); none otherwise (up to date, diverged, gone, detached).
pub fn action(sync: &Sync) -> Option<Action> {
    if !matches!(sync.head, Head::Branch(_)) {
        return None;
    }
    match sync.upstream {
        Upstream::Track { ahead: 0, behind } if behind > 0 => Some(Action::Pull(behind)),
        Upstream::Track { ahead, behind: 0 } if ahead > 0 => Some(Action::Push(ahead)),
        Upstream::None if sync.has_origin => Some(Action::Publish),
        _ => None,
    }
}

/// Whether Fetch shows: only with an `origin` remote.
pub fn shows_fetch(sync: &Sync) -> bool {
    sync.has_origin
}

/// The line under the branch, and its colour.
pub fn line(sync: &Sync, now: SystemTime) -> (String, Tone) {
    let fetched = |words: String| match sync.fetched {
        Some(at) => format!("{words} \u{b7} fetched {}", ago_label(at, now)),
        None => words,
    };
    if matches!(sync.head, Head::Detached(_)) {
        return ("Detached HEAD".into(), Tone::Faint);
    }
    if !sync.has_origin {
        return ("No remote named origin".into(), Tone::Faint);
    }
    match &sync.upstream {
        Upstream::Other(name) => (format!("Tracks {name} \u{b7} not origin"), Tone::Faint),
        Upstream::None => ("Local only \u{b7} not on origin".into(), Tone::Faint),
        Upstream::Gone => (fetched("Upstream gone".into()), Tone::Faint),
        Upstream::Track {
            ahead: 0,
            behind: 0,
        } => (fetched("\u{2713} Up to date".into()), Tone::Good),
        Upstream::Track { ahead: 0, behind } => {
            (fetched(format!("\u{2193}{behind} behind")), Tone::Faint)
        }
        Upstream::Track { ahead, behind: 0 } => {
            (fetched(format!("\u{2191}{ahead} ahead")), Tone::Faint)
        }
        Upstream::Track { ahead, behind } => (
            format!(
                "\u{26a0} \u{2193}{behind} \u{2191}{ahead} diverged \u{b7} merge or rebase in the terminal"
            ),
            Tone::Warn,
        ),
    }
}

/// `just now`, `N min ago`, `N h ago`, `N d ago`.
pub fn ago_label(fetched: SystemTime, now: SystemTime) -> String {
    let secs = now.duration_since(fetched).map_or(0, |d| d.as_secs());
    match secs {
        0..60 => "just now".into(),
        60..3600 => format!("{} min ago", secs / 60),
        3600..86_400 => format!("{} h ago", secs / 3600),
        _ => format!("{} d ago", secs / 86_400),
    }
}

/// `1 commit`, `2 commits`.
fn commits(n: u32) -> String {
    if n == 1 {
        "1 commit".into()
    } else {
        format!("{n} commits")
    }
}

/// A button's tooltip: what a click does.
pub fn tip(action: Action, branch: &str) -> String {
    match action {
        Action::Fetch => "Fetch from origin".into(),
        Action::Pull(n) => format!("Pull {} from origin", commits(n)),
        Action::Push(n) => format!("Push {} to origin", commits(n)),
        Action::Publish => format!("Publish {branch} to origin"),
    }
}

/// The line while `action` runs.
pub fn working_text(action: Action) -> String {
    match action {
        Action::Fetch => "Fetching\u{2026}".into(),
        Action::Pull(n) => format!("Pulling {}\u{2026}", commits(n)),
        Action::Push(n) => format!("Pushing {}\u{2026}", commits(n)),
        Action::Publish => "Publishing\u{2026}".into(),
    }
}

/// The line after `action` failed with git's first line `err`.
pub fn failed_text(action: Action, err: &str) -> String {
    let verb = match action {
        Action::Fetch => "Fetch",
        Action::Pull(_) => "Pull",
        Action::Push(_) => "Push",
        Action::Publish => "Publish",
    };
    let err = err
        .strip_prefix("error: ")
        .or_else(|| err.strip_prefix("fatal: "))
        .unwrap_or(err);
    format!("{verb} failed: {err}")
}

/// Whether showing a repo fetched at `fetched` should fetch it: never fetched, or longer ago
/// than `AUTO_FETCH_AFTER`.
pub fn auto_fetch_due(fetched: Option<SystemTime>, now: SystemTime) -> bool {
    fetched.is_none_or(|at| {
        now.duration_since(at)
            .is_ok_and(|d| d.as_secs() > AUTO_FETCH_AFTER)
    })
}

/// Where the branch at `root` stands. Blocking; local git only. None when git fails.
pub fn read(git: &Path, root: &Path) -> Option<Sync> {
    // The full ref, not `--short`: with a tag named like the branch, `--short` says
    // `heads/<name>`.
    let head = match run_git(git, root, &["symbolic-ref", "-q", "HEAD"]) {
        Some(full) => {
            let full = full.trim();
            let name = full.strip_prefix("refs/heads/").unwrap_or(full).to_string();
            if run_git(git, root, &["rev-parse", "-q", "--verify", "HEAD"]).is_some() {
                Head::Branch(name)
            } else {
                Head::Unborn(name)
            }
        }
        None => Head::Detached(
            run_git(git, root, &["rev-parse", "--short", "HEAD"])?
                .trim()
                .to_string(),
        ),
    };
    let upstream = match &head {
        Head::Branch(name) => {
            let refname = format!("refs/heads/{name}");
            let out = run_git(
                git,
                root,
                &[
                    "for-each-ref",
                    "--format=%(upstream)%00%(upstream:track)",
                    &refname,
                ],
            )?;
            let mut fields = out.trim_end_matches('\n').split('\0');
            upstream_of(fields.next().unwrap_or(""), fields.next().unwrap_or(""))
        }
        _ => Upstream::None,
    };
    let has_origin = run_git(git, root, &["remote"])?
        .lines()
        .any(|r| r == "origin");
    let fetched = run_git(git, root, &["rev-parse", "--git-path", "FETCH_HEAD"])
        .map(|p| root.join(p.trim_end_matches('\n')))
        .and_then(|p| std::fs::metadata(p).ok())
        .and_then(|m| m.modified().ok());
    Some(Sync {
        head,
        upstream,
        has_origin,
        fetched,
    })
}

/// Run `action` at `root`: `Ok` when git did it, else git's first line. `mode` says who may
/// ask for credentials (`Auto`: nobody).
pub fn run(git: &Path, root: &Path, action: Action, mode: FetchMode) -> Result<(), String> {
    let (args, cap): (&[&str], _) = match action {
        Action::Fetch => return fetch(git, root, mode),
        // A switch-sized cap: a pull killed half way could leave a half-updated tree.
        Action::Pull(_) => (&["pull", "--ff-only", "--no-rebase"], SWITCH_CAP),
        Action::Push(_) => (&["push"], FETCH_CAP),
        Action::Publish => (&["push", "-u", "origin", "HEAD"], FETCH_CAP),
    };
    run_git_checked_all(git, root, args, cap, mode.env())
        .map(|_| ())
        .map_err(|stderr| refusal(&stderr))
}

/// The line of git's stderr that says why it refused: a ` ! [rejected]` ref line, else the
/// first `error:` or `fatal:` line, else the first line (a pull prints `From …` first, a push
/// `To …`).
pub fn refusal(stderr: &str) -> String {
    let lines: Vec<&str> = stderr
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    lines
        .iter()
        .find(|l| l.starts_with("! ["))
        .or_else(|| {
            lines
                .iter()
                .find(|l| l.starts_with("error:") || l.starts_with("fatal:"))
        })
        .or(lines.first())
        .map_or_else(|| "git failed".to_string(), |l| l.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn at(secs: u64) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000 + secs)
    }

    fn sync(upstream: Upstream) -> Sync {
        Sync {
            head: Head::Branch("fix/x".into()),
            upstream,
            has_origin: true,
            fetched: Some(at(0)),
        }
    }

    fn track(ahead: u32, behind: u32) -> Upstream {
        Upstream::Track { ahead, behind }
    }

    #[test]
    fn upstream_reads_git_words() {
        assert_eq!(upstream_of("", ""), Upstream::None);
        assert_eq!(upstream_of("refs/remotes/origin/x", ""), track(0, 0));
        assert_eq!(
            upstream_of("refs/remotes/origin/x", "[ahead 1]"),
            track(1, 0)
        );
        assert_eq!(
            upstream_of("refs/remotes/origin/x", "[behind 2]"),
            track(0, 2)
        );
        assert_eq!(
            upstream_of("refs/remotes/origin/x", "[ahead 1, behind 2]"),
            track(1, 2)
        );
        assert_eq!(
            upstream_of("refs/remotes/upstream/main", "[ahead 1]"),
            Upstream::Other("upstream/main".into()),
            "only origin's branches count"
        );
        assert_eq!(
            upstream_of("refs/heads/trunk", ""),
            Upstream::Other("trunk".into())
        );
        assert_eq!(
            upstream_of("refs/remotes/origin/x", "[gone]"),
            Upstream::Gone
        );
    }

    #[test]
    fn one_action_at_most() {
        assert_eq!(action(&sync(track(0, 2))), Some(Action::Pull(2)));
        assert_eq!(action(&sync(track(1, 0))), Some(Action::Push(1)));
        assert_eq!(action(&sync(track(1, 2))), None, "diverged");
        assert_eq!(action(&sync(track(0, 0))), None, "up to date");
        assert_eq!(action(&sync(Upstream::None)), Some(Action::Publish));
        let no_origin = Sync {
            has_origin: false,
            ..sync(Upstream::None)
        };
        assert_eq!(action(&no_origin), None);
        assert!(!shows_fetch(&no_origin));
        assert_eq!(action(&sync(Upstream::Gone)), None);
        assert_eq!(action(&sync(Upstream::Other("upstream/main".into()))), None);
        let detached = Sync {
            head: Head::Detached("a1b2c3d".into()),
            ..sync(Upstream::None)
        };
        assert_eq!(action(&detached), None);
        let unborn = Sync {
            head: Head::Unborn("main".into()),
            ..sync(Upstream::None)
        };
        assert_eq!(action(&unborn), None, "nothing to publish yet");
    }

    #[test]
    fn lines_read_as_the_spec() {
        let now = at(180);
        let l = |s: Sync| line(&s, now);
        assert_eq!(
            l(sync(track(0, 0))),
            (
                "\u{2713} Up to date \u{b7} fetched 3 min ago".into(),
                Tone::Good
            )
        );
        assert_eq!(
            l(sync(track(0, 2))),
            (
                "\u{2193}2 behind \u{b7} fetched 3 min ago".into(),
                Tone::Faint
            )
        );
        assert_eq!(
            l(sync(track(1, 0))),
            (
                "\u{2191}1 ahead \u{b7} fetched 3 min ago".into(),
                Tone::Faint
            )
        );
        assert_eq!(
            l(sync(track(1, 2))),
            (
                "\u{26a0} \u{2193}2 \u{2191}1 diverged \u{b7} merge or rebase in the terminal"
                    .into(),
                Tone::Warn
            )
        );
        assert_eq!(
            l(sync(Upstream::None)),
            ("Local only \u{b7} not on origin".into(), Tone::Faint)
        );
        assert_eq!(
            l(Sync {
                has_origin: false,
                ..sync(Upstream::None)
            }),
            ("No remote named origin".into(), Tone::Faint)
        );
        assert_eq!(
            l(sync(Upstream::Other("upstream/main".into()))),
            ("Tracks upstream/main \u{b7} not origin".into(), Tone::Faint)
        );
        assert_eq!(
            l(sync(Upstream::Gone)),
            ("Upstream gone \u{b7} fetched 3 min ago".into(), Tone::Faint)
        );
        assert_eq!(
            l(Sync {
                head: Head::Detached("a1b2c3d".into()),
                ..sync(Upstream::None)
            }),
            ("Detached HEAD".into(), Tone::Faint)
        );
        assert_eq!(
            l(Sync {
                fetched: None,
                ..sync(track(0, 2))
            }),
            ("\u{2193}2 behind".into(), Tone::Faint)
        );
    }

    #[test]
    fn ago_label_steps() {
        assert_eq!(ago_label(at(0), at(10)), "just now");
        assert_eq!(ago_label(at(0), at(180)), "3 min ago");
        assert_eq!(ago_label(at(0), at(2 * 3600)), "2 h ago");
        assert_eq!(ago_label(at(0), at(3 * 86_400)), "3 d ago");
        assert_eq!(
            ago_label(at(10), at(0)),
            "just now",
            "a clock that went back"
        );
    }

    #[test]
    fn tips_and_working_words() {
        assert_eq!(tip(Action::Fetch, "x"), "Fetch from origin");
        assert_eq!(tip(Action::Pull(1), "x"), "Pull 1 commit from origin");
        assert_eq!(tip(Action::Pull(2), "x"), "Pull 2 commits from origin");
        assert_eq!(tip(Action::Push(1), "x"), "Push 1 commit to origin");
        assert_eq!(tip(Action::Push(2), "x"), "Push 2 commits to origin");
        assert_eq!(tip(Action::Publish, "feat/x"), "Publish feat/x to origin");
        assert_eq!(working_text(Action::Fetch), "Fetching\u{2026}");
        assert_eq!(working_text(Action::Pull(2)), "Pulling 2 commits\u{2026}");
        assert_eq!(working_text(Action::Push(1)), "Pushing 1 commit\u{2026}");
        assert_eq!(working_text(Action::Publish), "Publishing\u{2026}");
        assert_eq!(
            failed_text(Action::Pull(2), "Not possible to fast-forward"),
            "Pull failed: Not possible to fast-forward"
        );
        assert_eq!(
            failed_text(Action::Fetch, "error: x"),
            "Fetch failed: x",
            "git's own `error: ` prefix is dropped"
        );
        assert_eq!(failed_text(Action::Publish, "x"), "Publish failed: x");
    }

    #[test]
    fn auto_fetch_due_after_five_minutes() {
        assert!(auto_fetch_due(None, at(0)));
        assert!(!auto_fetch_due(Some(at(0)), at(299)));
        assert!(auto_fetch_due(Some(at(0)), at(301)));
    }

    pub(crate) mod repo {
        use std::fs;
        use std::path::{Path, PathBuf};
        use std::process::Command;

        pub fn temp(name: &str) -> PathBuf {
            let dir = std::env::temp_dir()
                .join(format!("fuxx-branch-sync-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            dir
        }

        pub fn git_in(git: &Path, dir: &Path, args: &[&str]) -> String {
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
            String::from_utf8_lossy(&out.stdout).into_owned()
        }

        pub fn commit_file(git: &Path, root: &Path, file: &str, text: &str) {
            fs::write(root.join(file), text).unwrap();
            git_in(git, root, &["add", "-A"]);
            git_in(git, root, &["commit", "-q", "-m", file]);
        }

        /// A bare `origin.git` with `main`, `work` cloned from it (tracking `main`), and
        /// `other`, a second clone to change origin from.
        pub fn fixture(name: &str, git: &Path) -> (PathBuf, PathBuf) {
            let base = temp(name);
            git_in(
                git,
                &base,
                &["init", "-q", "--bare", "-b", "main", "origin.git"],
            );
            let seed = base.join("seed");
            fs::create_dir_all(&seed).unwrap();
            git_in(git, &seed, &["init", "-q", "-b", "main"]);
            commit_file(git, &seed, "a.txt", "one\n");
            git_in(git, &seed, &["remote", "add", "origin", "../origin.git"]);
            git_in(git, &seed, &["push", "-q", "-u", "origin", "main"]);
            git_in(git, &base, &["clone", "-q", "origin.git", "work"]);
            git_in(git, &base, &["clone", "-q", "origin.git", "other"]);
            (base.join("work"), base.join("other"))
        }
    }

    #[test]
    fn read_sees_a_real_repo() {
        let Some(git) = crate::git_branch::find_git() else {
            return;
        };
        let (work, _other) = repo::fixture("read", &git);
        let s = read(&git, &work).expect("a repo");
        assert_eq!(s.head, Head::Branch("main".into()));
        assert_eq!(s.upstream, track(0, 0));
        assert!(s.has_origin);
        assert_eq!(s.fetched, None, "a clone writes no FETCH_HEAD");

        repo::commit_file(&git, &work, "b.txt", "two\n");
        assert_eq!(read(&git, &work).unwrap().upstream, track(1, 0));

        repo::git_in(&git, &work, &["fetch", "-q", "origin"]);
        assert!(read(&git, &work).unwrap().fetched.is_some());

        repo::git_in(&git, &work, &["switch", "-q", "-c", "feat/new"]);
        assert_eq!(read(&git, &work).unwrap().upstream, Upstream::None);

        repo::git_in(&git, &work, &["switch", "-q", "--detach"]);
        assert!(matches!(read(&git, &work).unwrap().head, Head::Detached(_)));

        repo::git_in(&git, &work, &["switch", "-q", "main"]);
        repo::git_in(&git, &work, &["remote", "remove", "origin"]);
        let s = read(&git, &work).unwrap();
        assert!(!s.has_origin);
        assert_eq!(s.upstream, Upstream::None, "its upstream went with origin");
    }

    /// `other` pushes a commit changing `a.txt`; `work` fetches it (now 1 behind).
    fn origin_moves(git: &Path, work: &Path, other: &Path) {
        repo::commit_file(git, other, "a.txt", "from other\n");
        repo::git_in(git, other, &["push", "-q", "origin", "main"]);
        run(git, work, Action::Fetch, FetchMode::Auto).unwrap();
        assert_eq!(read(git, work).unwrap().upstream, track(0, 1));
    }

    #[test]
    fn pull_fast_forwards() {
        let Some(git) = crate::git_branch::find_git() else {
            return;
        };
        let (work, other) = repo::fixture("pull", &git);
        origin_moves(&git, &work, &other);
        run(&git, &work, Action::Pull(1), FetchMode::Auto).unwrap();
        assert_eq!(read(&git, &work).unwrap().upstream, track(0, 0));
        assert_eq!(
            std::fs::read_to_string(work.join("a.txt")).unwrap(),
            "from other\n"
        );
    }

    #[test]
    fn pull_refuses_to_overwrite_local_changes() {
        let Some(git) = crate::git_branch::find_git() else {
            return;
        };
        let (work, other) = repo::fixture("pull-dirty", &git);
        origin_moves(&git, &work, &other);
        std::fs::write(work.join("a.txt"), "mine, not committed\n").unwrap();
        let err = run(&git, &work, Action::Pull(1), FetchMode::Auto).unwrap_err();
        assert!(err.contains("overwritten"), "{err}");
        assert_eq!(
            std::fs::read_to_string(work.join("a.txt")).unwrap(),
            "mine, not committed\n",
            "the edit is untouched"
        );
        assert_eq!(read(&git, &work).unwrap().upstream, track(0, 1));
    }

    #[test]
    fn pull_refuses_when_diverged() {
        let Some(git) = crate::git_branch::find_git() else {
            return;
        };
        let (work, other) = repo::fixture("pull-diverged", &git);
        origin_moves(&git, &work, &other);
        repo::commit_file(&git, &work, "b.txt", "mine\n");
        let before = repo::git_in(&git, &work, &["rev-parse", "HEAD"]);
        assert!(run(&git, &work, Action::Pull(1), FetchMode::Auto).is_err());
        assert_eq!(repo::git_in(&git, &work, &["rev-parse", "HEAD"]), before);
    }

    #[test]
    fn push_sends_a_local_commit() {
        let Some(git) = crate::git_branch::find_git() else {
            return;
        };
        let (work, _other) = repo::fixture("push", &git);
        repo::commit_file(&git, &work, "b.txt", "mine\n");
        assert_eq!(read(&git, &work).unwrap().upstream, track(1, 0));
        run(&git, &work, Action::Push(1), FetchMode::Auto).unwrap();
        assert_eq!(read(&git, &work).unwrap().upstream, track(0, 0));
    }

    #[test]
    fn publish_makes_the_branch_on_origin() {
        let Some(git) = crate::git_branch::find_git() else {
            return;
        };
        let (work, _other) = repo::fixture("publish", &git);
        repo::git_in(&git, &work, &["switch", "-q", "-c", "feat/new"]);
        assert_eq!(action(&read(&git, &work).unwrap()), Some(Action::Publish));
        run(&git, &work, Action::Publish, FetchMode::Auto).unwrap();
        let remote = repo::git_in(&git, &work, &["ls-remote", "--heads", "origin"]);
        assert!(remote.contains("refs/heads/feat/new"), "{remote}");
        assert_eq!(read(&git, &work).unwrap().upstream, track(0, 0));
    }

    /// A tag with the branch's name (review): `--short` would say `heads/main`; the bar must
    /// still read `main` and its upstream.
    #[test]
    fn a_tag_named_like_the_branch_changes_nothing() {
        let Some(git) = crate::git_branch::find_git() else {
            return;
        };
        let (work, _other) = repo::fixture("tag", &git);
        repo::git_in(&git, &work, &["tag", "main"]);
        let s = read(&git, &work).unwrap();
        assert_eq!(s.head, Head::Branch("main".into()));
        assert_eq!(s.upstream, track(0, 0));
        assert_eq!(action(&s), None, "no Publish for a tracked branch");
    }

    /// A pull that fetches first prints `From …` before git's refusal (review): the refusal
    /// is what the bar says.
    #[test]
    fn pull_without_fetching_first_says_why_it_refused() {
        let Some(git) = crate::git_branch::find_git() else {
            return;
        };
        let (work, other) = repo::fixture("pull-nofetch", &git);
        repo::commit_file(&git, &other, "a.txt", "from other\n");
        repo::git_in(&git, &other, &["push", "-q", "origin", "main"]);
        std::fs::write(work.join("a.txt"), "mine\n").unwrap();
        let err = run(&git, &work, Action::Pull(1), FetchMode::Auto).unwrap_err();
        assert!(err.contains("overwritten"), "{err}");
    }

    /// A rejected push starts with `To …` (review): the bar says it was rejected.
    #[test]
    fn a_rejected_push_says_rejected() {
        let Some(git) = crate::git_branch::find_git() else {
            return;
        };
        let (work, other) = repo::fixture("push-rejected", &git);
        repo::commit_file(&git, &other, "a.txt", "from other\n");
        repo::git_in(&git, &other, &["push", "-q", "origin", "main"]);
        repo::commit_file(&git, &work, "b.txt", "mine\n");
        let err = run(&git, &work, Action::Push(1), FetchMode::Auto).unwrap_err();
        assert!(err.contains("rejected"), "{err}");
    }
}
