//! The GRAPH's git (M11w, note 74): the commits to draw, a commit's details for the hover
//! card, and its GitHub link. Every read is `run_git` (off the UI thread, capped, never
//! prompting); `read` and `details` are blocking.

use crate::git_branch::run_git;
use std::path::Path;

/// Which commits are drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Scope {
    /// The current branch, its upstream and origin's default branch.
    #[default]
    Auto,
    /// Every local and remote branch.
    All,
    /// The current branch only.
    Current,
}

/// A name pointing at a commit, as the row's pills show it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ref {
    /// The branch checked out (`HEAD -> main`).
    Head(String),
    Local(String),
    /// An origin (or other remote) branch, `origin/main`.
    Remote(String),
    Tag(String),
}

/// One commit, as the row draws it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Commit {
    pub hash: String,
    pub parents: Vec<String>,
    pub author: String,
    /// Committed at, seconds since the epoch.
    pub time: i64,
    pub refs: Vec<Ref>,
    pub subject: String,
    /// HEAD is here (a branch checked out here, or a detached HEAD).
    pub head: bool,
}

/// What the hover card adds: the whole message and what the commit changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Details {
    pub body: String,
    pub files: u32,
    pub insertions: u32,
    pub deletions: u32,
}

/// `%D`'s refs with full names (`HEAD -> refs/heads/main, refs/remotes/origin/main, tag:
/// refs/tags/v1`) as pills with short names, in its order; a remote's `HEAD` and a bare `HEAD`
/// (detached) are left out (the bool says HEAD is here).
pub fn parse_refs(decoration: &str) -> (Vec<Ref>, bool) {
    let mut head = false;
    let mut refs = Vec::new();
    for part in decoration
        .split(", ")
        .map(str::trim)
        .filter(|p| !p.is_empty())
    {
        if part == "HEAD" {
            head = true;
        } else if let Some(branch) = part.strip_prefix("HEAD -> ") {
            head = true;
            let name = branch.strip_prefix("refs/heads/").unwrap_or(branch);
            refs.push(Ref::Head(name.to_string()));
        } else if let Some(tag) = part.strip_prefix("tag: ") {
            refs.push(Ref::Tag(
                tag.strip_prefix("refs/tags/").unwrap_or(tag).to_string(),
            ));
        } else if let Some(local) = part.strip_prefix("refs/heads/") {
            refs.push(Ref::Local(local.to_string()));
        } else if let Some(remote) = part.strip_prefix("refs/remotes/")
            && !remote.ends_with("/HEAD")
        {
            refs.push(Ref::Remote(remote.to_string()));
        }
    }
    (refs, head)
}

/// `git show --shortstat`'s line (` 9 files changed, 774 insertions(+), 65 deletions(-)`) as
/// (files, insertions, deletions); a missing part is 0.
pub fn parse_shortstat(line: &str) -> (u32, u32, u32) {
    let (mut files, mut ins, mut del) = (0, 0, 0);
    for part in line.split(',') {
        let mut words = part.split_whitespace();
        let (Some(n), Some(what)) = (words.next(), words.next()) else {
            continue;
        };
        let Ok(n) = n.parse::<u32>() else {
            continue;
        };
        if what.starts_with("file") {
            files = n;
        } else if what.starts_with("insertion") {
            ins = n;
        } else if what.starts_with("deletion") {
            del = n;
        }
    }
    (files, ins, del)
}

/// The commit's page on GitHub, when `remote` (origin's URL) is a GitHub one:
/// `git@github.com:o/r.git`, `ssh://git@github.com/o/r`, `https://github.com/o/r(.git)`.
pub fn github_commit_url(remote: &str, hash: &str) -> Option<String> {
    let remote = remote.trim();
    let path = [
        "git@github.com:",
        "ssh://git@github.com/",
        "https://github.com/",
        "http://github.com/",
    ]
    .iter()
    .find_map(|prefix| remote.strip_prefix(prefix))?;
    let path = path.trim_end_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    let mut parts = path.split('/');
    let (Some(owner), Some(repo), None) = (parts.next(), parts.next(), parts.next()) else {
        return None;
    };
    if owner.is_empty() || repo.is_empty() || !hash.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    Some(format!("https://github.com/{owner}/{repo}/commit/{hash}"))
}

/// `git log`'s revisions for `scope`: `upstream` (the branch's, if any) and `default`
/// (origin's default branch, if known) join HEAD for Auto.
pub fn revisions(scope: Scope, upstream: Option<&str>, default: Option<&str>) -> Vec<String> {
    match scope {
        Scope::Current => vec!["HEAD".to_string()],
        Scope::All => vec!["--branches".to_string(), "--remotes".to_string()],
        Scope::Auto => {
            let mut revs = vec!["HEAD".to_string()];
            for r in [upstream, default].into_iter().flatten() {
                if !r.starts_with('-') && !revs.iter().any(|x| x == r) {
                    revs.push(r.to_string());
                }
            }
            revs
        }
    }
}

/// Up to `limit` commits for `scope`, newest first in topological order (none before the first
/// commit). None when git fails.
pub fn read(git: &Path, root: &Path, scope: Scope, limit: usize) -> Option<Vec<Commit>> {
    if run_git(git, root, &["rev-parse", "-q", "--verify", "HEAD"]).is_none() {
        return Some(Vec::new());
    }
    let short = |args: &[&str]| {
        run_git(git, root, args)
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    };
    let upstream = short(&["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"]);
    let default = short(&["rev-parse", "--abbrev-ref", "origin/HEAD"]);
    let revs = revisions(scope, upstream.as_deref(), default.as_deref());
    let count = limit.to_string();
    let mut args = vec![
        "log",
        "--topo-order",
        "--decorate=full",
        "--format=%H%x00%P%x00%an%x00%at%x00%D%x00%s%x1e",
        "-n",
        &count,
    ];
    args.extend(revs.iter().map(String::as_str));
    args.push("--");
    let out = run_git(git, root, &args)?;
    Some(
        out.split('\x1e')
            .map(|r| r.trim_start_matches('\n'))
            .filter(|r| !r.is_empty())
            .filter_map(|record| {
                let mut f = record.split('\0');
                let hash = f.next()?.to_string();
                let parents = f.next()?.split_whitespace().map(String::from).collect();
                let author = f.next()?.to_string();
                let time = f.next()?.parse().unwrap_or(0);
                let (refs, head) = parse_refs(f.next()?);
                let subject = f.next().unwrap_or("").to_string();
                Some(Commit {
                    hash,
                    parents,
                    author,
                    time,
                    refs,
                    subject,
                    head,
                })
            })
            .collect(),
    )
}

/// `hash`'s whole message and what it changed. None when git can't read it (gone).
pub fn details(git: &Path, root: &Path, hash: &str) -> Option<Details> {
    if hash.is_empty() || !hash.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let body = run_git(git, root, &["show", "-s", "--format=%B", hash])?;
    // A merge's change is against its first parent (as the IDEs count it); a root commit's,
    // everything in it.
    let first = format!("{hash}^1");
    let stat = run_git(git, root, &["diff", "--shortstat", &first, hash])
        .or_else(|| run_git(git, root, &["show", "--shortstat", "--format=", hash]))
        .unwrap_or_default();
    let (files, insertions, deletions) = parse_shortstat(stat.trim());
    Some(Details {
        body: body.trim_end().to_string(),
        files,
        insertions,
        deletions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::process::Command;

    fn git_in(git: &Path, dir: &Path, args: &[&str]) {
        let out = Command::new(git)
            .arg("-C")
            .arg(dir)
            .args([
                "-c",
                "user.name=Ayu",
                "-c",
                "user.email=a@a",
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

    fn repo(name: &str, git: &Path) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("fuxx-git-graph-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        git_in(git, &root, &["init", "-q", "-b", "main"]);
        root
    }

    fn commit(git: &Path, root: &Path, file: &str, text: &str, msg: &str) {
        fs::write(root.join(file), text).unwrap();
        git_in(git, root, &["add", "-A"]);
        git_in(git, root, &["commit", "-q", "-m", msg]);
    }

    #[test]
    fn refs_read_as_pills() {
        // Full names (`--decorate=full`): a local `feature/x` and a remote `origin/x` can't be
        // told apart by their short names.
        let (refs, head) = parse_refs(
            "HEAD -> refs/heads/feature/x, refs/remotes/origin/feature/x, \
             refs/remotes/origin/HEAD, refs/heads/main, tag: refs/tags/v1.2",
        );
        assert!(head);
        assert_eq!(
            refs,
            vec![
                Ref::Head("feature/x".into()),
                Ref::Remote("origin/feature/x".into()),
                Ref::Local("main".into()),
                Ref::Tag("v1.2".into()),
            ]
        );
        assert_eq!(parse_refs(""), (vec![], false));
        assert_eq!(parse_refs("HEAD"), (vec![], true), "a detached HEAD");
    }

    #[test]
    fn shortstat_reads_its_counts() {
        assert_eq!(
            parse_shortstat(" 9 files changed, 774 insertions(+), 65 deletions(-)"),
            (9, 774, 65)
        );
        assert_eq!(
            parse_shortstat(" 1 file changed, 2 insertions(+)"),
            (1, 2, 0)
        );
        assert_eq!(
            parse_shortstat(" 1 file changed, 3 deletions(-)"),
            (1, 0, 3)
        );
        assert_eq!(parse_shortstat(""), (0, 0, 0));
    }

    #[test]
    fn github_links_from_origin() {
        let want = Some("https://github.com/hubexo/api/commit/ea3a07a".to_string());
        assert_eq!(
            github_commit_url("git@github.com:hubexo/api.git", "ea3a07a"),
            want
        );
        assert_eq!(
            github_commit_url("https://github.com/hubexo/api", "ea3a07a"),
            want
        );
        assert_eq!(
            github_commit_url("https://github.com/hubexo/api.git", "ea3a07a"),
            want
        );
        assert_eq!(
            github_commit_url("ssh://git@github.com/hubexo/api.git", "ea3a07a"),
            want
        );
        assert_eq!(
            github_commit_url("git@gitlab.com:hubexo/api.git", "ea3a07a"),
            None
        );
        assert_eq!(
            github_commit_url("git@gh-work:hubexo/api.git", "ea3a07a"),
            None,
            "an ssh alias"
        );
    }

    #[test]
    fn revisions_per_scope() {
        assert_eq!(
            revisions(Scope::Current, Some("origin/x"), Some("origin/main")),
            ["HEAD"]
        );
        assert_eq!(
            revisions(Scope::Auto, Some("origin/x"), Some("origin/main")),
            ["HEAD", "origin/x", "origin/main"]
        );
        assert_eq!(revisions(Scope::Auto, None, None), ["HEAD"]);
        assert_eq!(
            revisions(Scope::All, None, None),
            ["--branches", "--remotes"]
        );
    }

    #[test]
    fn read_and_details_from_a_real_repo() {
        let Some(git) = crate::git_branch::find_git() else {
            return;
        };
        let root = repo("read", &git);
        assert_eq!(
            read(&git, &root, Scope::Auto, 50),
            Some(vec![]),
            "no commits yet"
        );
        commit(&git, &root, "a.txt", "1\n", "first");
        git_in(&git, &root, &["switch", "-q", "-c", "side"]);
        commit(
            &git,
            &root,
            "b.txt",
            "1\n2\n",
            "on the side\n\nThe whole message.",
        );
        git_in(&git, &root, &["switch", "-q", "main"]);
        commit(&git, &root, "c.txt", "1\n", "on main");
        git_in(
            &git,
            &root,
            &["merge", "-q", "--no-ff", "-m", "merge side", "side"],
        );
        let all = read(&git, &root, Scope::All, 50).unwrap();
        assert_eq!(all.len(), 4);
        assert_eq!(all[0].subject, "merge side");
        assert_eq!(all[0].parents.len(), 2);
        assert!(all[0].head);
        assert_eq!(all[0].refs, vec![Ref::Head("main".into())]);
        assert_eq!(all[0].author, "Ayu");
        assert!(all[0].time > 0);
        let side = all.iter().find(|c| c.subject == "on the side").unwrap();
        assert_eq!(side.refs, vec![Ref::Local("side".into())]);
        assert_eq!(
            read(&git, &root, Scope::All, 2).unwrap().len(),
            2,
            "the limit"
        );
        let d = details(&git, &root, &side.hash).unwrap();
        assert_eq!(d.body.trim(), "on the side\n\nThe whole message.");
        assert_eq!((d.files, d.insertions, d.deletions), (1, 2, 0));
        assert_eq!(details(&git, &root, "0000000"), None);
    }
}
