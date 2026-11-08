//! The ⌘K palette's Files source: the followed terminal's files, listed with git (or, outside a
//! repo, a bounded walk) and ranked for the query, all off the UI thread (M11j).
#![allow(dead_code)] // used by the palette (M11j task 5)

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use crate::file_tree::nfc;
use crate::git_branch::{Ran, find_git, run_git_bytes};
use crate::palette_items::FileHit;
use crate::palette_match::fuzzy_path;
use crate::right_sidebar::find_root;

/// The most files kept from a listing; `Listed::more` counts the rest.
pub const FILES_CAP: usize = 50_000;
/// A walk (outside a repo) stops at this many files, this many folders deep, or this long.
pub const WALK_FILES: usize = 20_000;
pub const WALK_DEPTH: usize = 12;
pub const WALK_TIME: Duration = Duration::from_secs(2);
/// A listing is asked again once it is this old.
pub const FRESH: Duration = Duration::from_secs(10);
/// `rank_files` looks at the shared generation every this many paths.
pub const CANCEL_EVERY: usize = 1_024;
/// The folders (listings) kept; the oldest goes first.
const KEPT: usize = 4;

/// A folder's files from its root, sorted, and how many more there were past the cap.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Listed {
    pub paths: Vec<String>,
    pub more: usize,
}

/// `git ls-files -z` output: NUL-separated, empty pieces skipped, lossy UTF-8, NFC.
pub fn parse_z(bytes: &[u8]) -> Vec<String> {
    bytes
        .split(|b| *b == 0)
        .filter(|piece| !piece.is_empty())
        .map(|piece| nfc(&String::from_utf8_lossy(piece)))
        .collect()
}

/// The repo at `root`'s files: tracked and untracked-not-ignored, minus the deleted ones;
/// sorted, at most `FILES_CAP`. Blocking. None when either git call fails or hits its cap.
pub fn list_repo(git: &Path, root: &Path) -> Option<Listed> {
    list_repo_with(git, root, FILES_CAP)
}

pub(crate) fn list_repo_with(git: &Path, root: &Path, cap: usize) -> Option<Listed> {
    let out = |args: &[&str]| match run_git_bytes(git, root, args) {
        Ran::Exited(Some(0), bytes) => Some(bytes),
        _ => None,
    };
    let all = out(&[
        "ls-files",
        "-z",
        "--cached",
        "--others",
        "--exclude-standard",
    ])?;
    let deleted: HashSet<String> = parse_z(&out(&["ls-files", "-z", "--deleted"])?)
        .into_iter()
        .collect();
    let mut paths: Vec<String> = parse_z(&all)
        .into_iter()
        .filter(|p| !deleted.contains(p))
        .collect();
    paths.sort_unstable();
    paths.dedup();
    let more = paths.len().saturating_sub(cap);
    paths.truncate(cap);
    Some(Listed { paths, more })
}

/// A breadth-first walk of `root` for a folder that isn't a repo: dot-folders skipped, symlinked
/// folders not followed (so no loops), and stopped at `max_files` files, `max_depth` folders deep
/// or `max_time`. `more` is non-zero when files were left out by the file or time cap.
pub fn walk(root: &Path, max_files: usize, max_depth: usize, max_time: Duration) -> Listed {
    let started = Instant::now();
    let mut paths: Vec<String> = Vec::new();
    let mut more = 0;
    let mut queue: VecDeque<(PathBuf, String, usize)> = VecDeque::new();
    queue.push_back((root.to_path_buf(), String::new(), 0));
    while let Some((dir, rel, depth)) = queue.pop_front() {
        if started.elapsed() >= max_time {
            more = more.max(1);
            break;
        }
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            let name = nfc(&entry.file_name().to_string_lossy());
            let is_file = if kind.is_dir() {
                if depth < max_depth && !name.starts_with('.') {
                    queue.push_back((entry.path(), format!("{rel}{name}/"), depth + 1));
                }
                false
            } else if kind.is_symlink() {
                // A link to a file is a file; a link to a folder is never entered.
                std::fs::metadata(entry.path()).is_ok_and(|m| m.is_file())
            } else {
                kind.is_file()
            };
            if !is_file {
                continue;
            }
            if paths.len() >= max_files {
                more += 1;
            } else {
                paths.push(format!("{rel}{name}"));
            }
        }
        if paths.len() >= max_files {
            // The rest of this folder was counted; folders still queued hold files we skip.
            if !queue.is_empty() {
                more = more.max(1);
            }
            break;
        }
    }
    paths.sort_unstable();
    paths.dedup();
    Listed { paths, more }
}

/// A folder's files as asked for, with the root they are relative to.
#[derive(Clone, Debug, PartialEq)]
pub struct FilesAnswer {
    pub folder: PathBuf,
    pub root: PathBuf,
    /// None: it is a repo and git gave no (complete) answer.
    pub listed: Option<Listed>,
}

/// `folder`'s files: its root by `find_root`, then git in a repo, else the walk. Blocking. None
/// when `find_root` had no answer.
pub fn list_files(folder: &Path) -> Option<FilesAnswer> {
    let (root, in_repo, _) = find_root(folder)?;
    let listed = if in_repo {
        find_git().and_then(|git| list_repo(&git, &root))
    } else {
        Some(walk(&root, WALK_FILES, WALK_DEPTH, WALK_TIME))
    };
    Some(FilesAnswer {
        folder: folder.to_path_buf(),
        root,
        listed,
    })
}

/// A folder's last listing.
#[derive(Clone, Debug)]
pub struct CachedFiles {
    pub root: PathBuf,
    pub paths: Arc<Vec<String>>,
    pub more: usize,
    pub failed: bool,
    pub listed_at: Instant,
}

/// The listings a window has, one running at a time.
#[derive(Default)]
pub struct PaletteFiles {
    listings: HashMap<PathBuf, CachedFiles>,
    running: bool,
}

impl PaletteFiles {
    /// Whether to list `folder` now: nothing running, and no listing, a failed one, or one
    /// older than `FRESH`.
    pub fn due(&self, folder: &Path, now: Instant) -> bool {
        !self.running
            && self
                .listings
                .get(folder)
                .is_none_or(|c| c.failed || now.saturating_duration_since(c.listed_at) >= FRESH)
    }

    pub fn started(&mut self, _folder: &Path) {
        self.running = true;
    }

    pub fn finished(&mut self, folder: &Path, answer: Option<FilesAnswer>, now: Instant) {
        self.running = false;
        let cached = match answer {
            Some(FilesAnswer {
                root,
                listed: Some(l),
                ..
            }) => CachedFiles {
                root,
                paths: Arc::new(l.paths),
                more: l.more,
                failed: false,
                listed_at: now,
            },
            other => CachedFiles {
                root: other.map(|a| a.root).unwrap_or_default(),
                paths: Arc::new(Vec::new()),
                more: 0,
                failed: true,
                listed_at: now,
            },
        };
        if self.listings.len() >= KEPT && !self.listings.contains_key(folder) {
            let oldest = self
                .listings
                .iter()
                .min_by_key(|(_, c)| c.listed_at)
                .map(|(f, _)| f.clone());
            if let Some(oldest) = oldest {
                self.listings.remove(&oldest);
            }
        }
        self.listings.insert(folder.to_path_buf(), cached);
    }

    pub fn get(&self, folder: &Path) -> Option<&CachedFiles> {
        self.listings.get(folder)
    }

    pub fn running(&self) -> bool {
        self.running
    }

    /// The screenshots' listing: `paths` under `root`, as if just listed.
    #[cfg(feature = "ui-shots")]
    pub fn set_for_shots(&mut self, folder: PathBuf, root: PathBuf, paths: Vec<String>) {
        self.listings.insert(
            folder,
            CachedFiles {
                root,
                paths: Arc::new(paths),
                more: 0,
                failed: false,
                listed_at: Instant::now(),
            },
        );
    }
}

/// The best `limit` of `paths` for `query` (score desc, then shorter path, then by path) and how
/// many matched in all. None when `current` is no longer `generation` (a newer query started),
/// checked every `CANCEL_EVERY` paths. A blank query matches nothing.
pub fn rank_files(
    paths: &[String],
    query: &str,
    limit: usize,
    generation: u64,
    current: &AtomicU64,
) -> Option<(Vec<FileHit>, usize)> {
    if query.trim().is_empty() {
        return Some((Vec::new(), 0));
    }
    let mut matched: Vec<(i32, usize)> = Vec::new();
    for (i, path) in paths.iter().enumerate() {
        if i.is_multiple_of(CANCEL_EVERY) && current.load(Ordering::Relaxed) != generation {
            return None;
        }
        if let Some(m) = fuzzy_path(query, path) {
            matched.push((m.score, i));
        }
    }
    let total = matched.len();
    matched.sort_by(|(sa, a), (sb, b)| {
        sb.cmp(sa)
            .then_with(|| paths[*a].chars().count().cmp(&paths[*b].chars().count()))
            .then_with(|| paths[*a].cmp(&paths[*b]))
    });
    matched.truncate(limit);
    let hits = matched
        .into_iter()
        .filter_map(|(_, i)| {
            fuzzy_path(query, &paths[i]).map(|m| FileHit {
                rel: paths[i].clone(),
                positions: m.positions,
                score: m.score,
            })
        })
        .collect();
    Some((hits, total))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process::Command;

    fn temp(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("fuxx-palette-files-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn git_in(git: &Path, root: &Path, args: &[&str]) {
        let ok = Command::new(git)
            .arg("-C")
            .arg(root)
            .args(["-c", "user.name=t", "-c", "user.email=t@t"])
            .args(["-c", "commit.gpgsign=false"])
            .args(args)
            .status()
            .unwrap()
            .success();
        assert!(ok, "git {args:?}");
    }

    #[test]
    fn parse_z_splits_and_normalises() {
        assert_eq!(parse_z(b"a.rs\0dir/b c.txt\0\0"), ["a.rs", "dir/b c.txt"]);
        assert_eq!(parse_z("u\u{308}.txt\0".as_bytes()), ["\u{fc}.txt"]);
    }

    #[test]
    fn list_repo_end_to_end() {
        let Some(git) = find_git() else {
            return println!("skipped: no git");
        };
        let root = temp("repo");
        git_in(&git, &root, &["init", "-q"]);
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("a.rs"), "a").unwrap();
        fs::write(root.join("src/b.rs"), "b").unwrap();
        fs::write(root.join("gone.rs"), "g").unwrap();
        git_in(&git, &root, &["add", "-A"]);
        git_in(&git, &root, &["commit", "-q", "-m", "first"]);
        fs::remove_file(root.join("gone.rs")).unwrap();
        fs::create_dir_all(root.join("new dir")).unwrap();
        fs::write(root.join("new dir/\u{fc}.txt"), "u").unwrap();
        fs::write(root.join(".gitignore"), "target/\n").unwrap();
        fs::create_dir_all(root.join("target")).unwrap();
        fs::write(root.join("target/x"), "x").unwrap();
        fs::write(root.join("tab\tname.txt"), "t").unwrap();
        let listed = list_repo(&git, &root).unwrap();
        // `.gitignore` is untracked and listed too.
        assert_eq!(
            listed.paths,
            [
                ".gitignore",
                "a.rs",
                "new dir/\u{fc}.txt",
                "src/b.rs",
                "tab\tname.txt"
            ]
        );
        assert_eq!(listed.more, 0);
        let plain = temp("plain");
        assert_eq!(list_repo(&git, &plain), None);
    }

    #[test]
    fn list_repo_caps() {
        let Some(git) = find_git() else {
            return println!("skipped: no git");
        };
        let root = temp("caps");
        git_in(&git, &root, &["init", "-q"]);
        for i in 0..1_005 {
            fs::write(root.join(format!("f{i:04}")), "").unwrap();
        }
        let listed = list_repo_with(&git, &root, 1_000).unwrap();
        assert_eq!(listed.paths.len(), 1_000);
        assert_eq!(listed.more, 5);
    }

    #[test]
    fn walk_stops_at_its_caps() {
        let root = temp("walk");
        fs::create_dir_all(root.join(".hidden")).unwrap();
        fs::write(root.join(".hidden/x"), "").unwrap();
        fs::create_dir_all(root.join(".git")).unwrap();
        fs::write(root.join(".git/y"), "").unwrap();
        let real = temp("walk-target");
        fs::write(real.join("through"), "").unwrap();
        std::os::unix::fs::symlink(&real, root.join("link")).unwrap();
        // A link back up: a loop if it were followed.
        std::os::unix::fs::symlink(&root, root.join("loop")).unwrap();
        let mut dir = root.clone();
        for level in 0..15 {
            dir = dir.join(format!("d{level}"));
            fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join("f1"), "").unwrap();
            fs::write(dir.join("f2"), "").unwrap();
        }
        let all = walk(&root, 20_000, 12, Duration::from_secs(2));
        assert!(
            all.paths
                .iter()
                .all(|p| !p.starts_with(".hidden") && !p.starts_with(".git"))
        );
        assert!(all.paths.iter().all(|p| !p.contains("through")));
        assert!(all.paths.iter().all(|p| !p.starts_with("link/")));
        assert!(all.paths.iter().all(|p| !p.starts_with("loop/")));
        assert!(all.paths.iter().all(|p| p.matches('/').count() <= 12));
        let few = walk(&root, 10, 12, Duration::from_secs(2));
        assert_eq!(few.paths.len(), 10);
        assert!(few.more >= 1);
        let shallow = walk(&root, 20_000, 3, Duration::from_secs(2));
        assert!(!shallow.paths.is_empty());
        assert!(shallow.paths.iter().all(|p| p.matches('/').count() <= 3));
        let none = walk(&root, 20_000, 12, Duration::ZERO);
        assert!(none.paths.is_empty() && none.more >= 1);
    }

    #[test]
    fn palette_files_lists_once_and_freshens() {
        let folder = Path::new("/p");
        let mut files = PaletteFiles::default();
        let t = Instant::now();
        assert!(files.due(folder, t));
        files.started(folder);
        assert!(!files.due(folder, t) && files.running());
        let answer = FilesAnswer {
            folder: folder.to_path_buf(),
            root: PathBuf::from("/p"),
            listed: Some(Listed {
                paths: vec!["a".into()],
                more: 0,
            }),
        };
        files.finished(folder, Some(answer), t);
        assert!(!files.running());
        assert!(!files.get(folder).unwrap().failed);
        assert!(!files.due(folder, t + Duration::from_secs(9)));
        assert!(files.due(folder, t + Duration::from_secs(10)));
        files.started(folder);
        files.finished(folder, None, t);
        assert!(files.get(folder).unwrap().failed);
        assert!(files.due(folder, t));
    }

    fn strings(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn rank_files_orders_and_counts() {
        let paths = strings(&[
            "src/domain/mod.rs",
            "docs/maintenance.md",
            "src/main.rs",
            "README",
        ]);
        let current = AtomicU64::new(1);
        let (hits, total) = rank_files(&paths, "main", 2, 1, &current).unwrap();
        let rels: Vec<&str> = hits.iter().map(|h| h.rel.as_str()).collect();
        assert_eq!(rels, ["src/main.rs", "docs/maintenance.md"]);
        assert_eq!(total, 3);
    }

    #[test]
    fn rank_files_stops_when_a_newer_query_starts() {
        let paths: Vec<String> = (0..10_000).map(|i| format!("dir/file{i}.rs")).collect();
        let current = AtomicU64::new(8);
        assert!(rank_files(&paths, "file", 200, 7, &current).is_none());
        assert!(rank_files(&paths, "file", 200, 8, &current).is_some());
    }
}
