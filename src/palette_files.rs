//! The ⌘K palette's Files source: the followed terminal's files, listed with git (or, outside a
//! repo, a bounded walk) and ranked for the query, all off the UI thread (M11j).

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use crate::file_tree::nfc;
use crate::git_branch::{Ran, find_git, run_git_bytes};
use crate::palette_items::FileHit;
use crate::palette_match::fuzzy_path;
use crate::right_sidebar::{back_off, find_root};

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
    /// It came from the walk (not git), so a cut-short one says so differently.
    pub walked: bool,
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
    let deleted = out(&["ls-files", "-z", "--deleted"])?;
    Some(assemble(&all, &deleted, cap))
}

/// The two `ls-files -z` outputs as a listing: the deleted subtracted, deduped (a conflicted
/// path is listed once per stage), sorted, cut at `cap`.
fn assemble(all: &[u8], deleted: &[u8], cap: usize) -> Listed {
    let deleted: HashSet<String> = parse_z(deleted).into_iter().collect();
    let mut paths: Vec<String> = parse_z(all)
        .into_iter()
        .filter(|p| !deleted.contains(p))
        .collect();
    paths.sort_unstable();
    paths.dedup();
    let more = paths.len().saturating_sub(cap);
    paths.truncate(cap);
    Listed {
        paths,
        more,
        walked: false,
    }
}

/// A breadth-first walk of `root` for a folder that isn't a repo: dot-folders skipped (and, in
/// the home folder and at `/`, the folders `skipped` names), symlinked folders not followed (so
/// no loops), and stopped at `max_files` files, `max_depth` folders deep or `max_time`. `more` is
/// non-zero when files were left out by the file or time cap.
pub fn walk(root: &Path, max_files: usize, max_depth: usize, max_time: Duration) -> Listed {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    walk_with_home(root, home.as_deref(), max_files, max_depth, max_time)
}

/// The home folder's folders the walk never enters: Library, and those macOS asks permission
/// to read (a walk into them would put its dialog up, in fuxx's name).
const HOME_SKIPPED: [&str; 7] = [
    "Library",
    "Desktop",
    "Documents",
    "Downloads",
    "Pictures",
    "Movies",
    "Music",
];
/// The disk's (`/`) folders the walk never enters: the system's, other disks', devices.
const ROOT_SKIPPED: [&str; 8] = [
    "Volumes",
    "System",
    "dev",
    "private",
    "Library",
    "Applications",
    "cores",
    "opt",
];

/// Whether the walk skips folder `name` in the folder it is reading: the home folder
/// (`at_home`) or the disk's top (`at_fs_root`). A terminal inside one of them is still walked:
/// its root is that folder, not home.
fn skipped(name: &std::ffi::OsStr, at_home: bool, at_fs_root: bool) -> bool {
    (at_home && HOME_SKIPPED.iter().any(|s| name == *s))
        || (at_fs_root && ROOT_SKIPPED.iter().any(|s| name == *s))
}

/// How many entries the walk reads between looks at the clock.
const WALK_CLOCK_EVERY: usize = 256;

/// `walk`, with `home` as the folder whose `HOME_SKIPPED` folders are skipped, whether the walk
/// starts there or reaches it from above (a walk of `/` or `/Users`).
pub(crate) fn walk_with_home(
    root: &Path,
    home: Option<&Path>,
    max_files: usize,
    max_depth: usize,
    max_time: Duration,
) -> Listed {
    let started = Instant::now();
    let real = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    // From the real root, every folder queued is a real path too (links aren't entered), so
    // the home folder is found by a plain comparison.
    let real_root = real(root);
    let real_home = home.map(real);
    let mut paths: Vec<String> = Vec::new();
    let mut more = 0;
    let mut seen = 0usize;
    let mut queue: VecDeque<(PathBuf, String, usize)> = VecDeque::new();
    queue.push_back((real_root, String::new(), 0));
    'dirs: while let Some((dir, rel, depth)) = queue.pop_front() {
        if started.elapsed() >= max_time {
            more = more.max(1);
            break;
        }
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        let at_home = real_home.as_deref() == Some(dir.as_path());
        let at_fs_root = depth == 0 && dir == Path::new("/");
        for entry in entries.flatten() {
            seen += 1;
            if seen.is_multiple_of(WALK_CLOCK_EVERY) && started.elapsed() >= max_time {
                more = more.max(1);
                break 'dirs;
            }
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            let raw = entry.file_name();
            let is_file = if kind.is_dir() {
                let hidden = raw.as_encoded_bytes().first() == Some(&b'.');
                if depth < max_depth && !hidden && !skipped(&raw, at_home, at_fs_root) {
                    let name = nfc(&raw.to_string_lossy());
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
                more = more.max(1);
                break 'dirs;
            }
            paths.push(format!("{rel}{}", nfc(&raw.to_string_lossy())));
        }
    }
    paths.sort_unstable();
    paths.dedup();
    Listed {
        paths,
        more,
        walked: true,
    }
}

/// The folder to list for a terminal: what it last reported itself (OSC 7, the freshest), else
/// where the places last found it, else the folder it started in (never for an SSH terminal:
/// that one is this Mac's); and whether it is an SSH terminal (its files aren't listed, but it
/// is still one).
pub fn choose_folder(
    reported: Option<PathBuf>,
    found: Option<&Path>,
    project: Option<&Path>,
    ssh: bool,
) -> Option<(PathBuf, bool)> {
    let reported = reported.or_else(|| found.map(Path::to_path_buf));
    if ssh {
        // The folder it started in is this Mac's, not the remote one's.
        return Some((reported.unwrap_or_default(), true));
    }
    reported
        .or_else(|| project.map(Path::to_path_buf))
        .map(|f| (f, false))
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
    /// The list came from the walk (not git).
    pub walked: bool,
    pub failed: bool,
    pub listed_at: Instant,
    /// How long after `listed_at` it is asked again: `FRESH`, or the back-off after a failure.
    pub wait: Duration,
}

/// The listings a window has, one running at a time.
#[derive(Default)]
pub struct PaletteFiles {
    listings: HashMap<PathBuf, CachedFiles>,
    running: bool,
    /// Nothing is listed (the screenshots give their listings themselves).
    pub frozen: bool,
}

impl PaletteFiles {
    /// Whether to list `folder` now: nothing running, and no listing (`get`), a failed one, or
    /// one older than `FRESH`.
    pub fn due(&self, folder: &Path, now: Instant) -> bool {
        !self.running
            && self
                .get(folder)
                .is_none_or(|c| now.saturating_duration_since(c.listed_at) >= c.wait)
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
                walked: l.walked,
                failed: false,
                listed_at: now,
                wait: FRESH,
            },
            other => {
                // 2 s after a first failure, then 4, 8, 16, 30 s.
                let wait = match self.listings.get(folder) {
                    Some(c) if c.failed => back_off(c.wait, false),
                    _ => back_off(Duration::from_secs(1), false),
                };
                CachedFiles {
                    root: other.map(|a| a.root).unwrap_or_default(),
                    paths: Arc::new(Vec::new()),
                    more: 0,
                    walked: false,
                    failed: true,
                    listed_at: now,
                    wait,
                }
            }
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

    /// `folder`'s listing: its own, else the freshest repo listing (git's, not failed) whose
    /// root it is inside (a `cd src` in a listed repo isn't listed again until that one is
    /// older than `FRESH`).
    pub fn get(&self, folder: &Path) -> Option<&CachedFiles> {
        self.listings.get(folder).or_else(|| {
            self.listings
                .values()
                .filter(|c| !c.failed && !c.walked && !c.root.as_os_str().is_empty())
                .filter(|c| folder.starts_with(&c.root))
                .max_by_key(|c| c.listed_at)
        })
    }

    /// Whether a listing is running (the palette shows `Listing` for any folder not listed yet,
    /// so only the tests ask).
    #[cfg(test)]
    pub fn running(&self) -> bool {
        self.running
    }

    /// The screenshots' listing: `paths` under `root`, as if just listed.
    #[cfg(feature = "ui-shots")]
    pub fn set_for_shots(&mut self, folder: PathBuf, root: PathBuf, mut paths: Vec<String>) {
        // Sorted, as every listing is.
        paths.sort_unstable();
        self.listings.insert(
            folder,
            CachedFiles {
                root,
                paths: Arc::new(paths),
                more: 0,
                walked: false,
                failed: false,
                listed_at: Instant::now(),
                wait: FRESH,
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

    fn answer(folder: &Path, paths: &[&str]) -> FilesAnswer {
        FilesAnswer {
            folder: folder.to_path_buf(),
            root: folder.to_path_buf(),
            listed: Some(Listed {
                paths: paths.iter().map(|p| p.to_string()).collect(),
                more: 0,
                walked: false,
            }),
        }
    }

    #[test]
    fn palette_files_lists_once_and_freshens() {
        let folder = Path::new("/p");
        let mut files = PaletteFiles::default();
        let t = Instant::now();
        let secs = Duration::from_secs;
        assert!(files.due(folder, t));
        files.started(folder);
        assert!(!files.due(folder, t) && files.running());
        files.finished(folder, Some(answer(folder, &["a"])), t);
        assert!(!files.running());
        assert!(!files.get(folder).unwrap().failed);
        assert!(!files.due(folder, t + secs(9)));
        assert!(files.due(folder, t + secs(10)));
        // A failure backs off: 2 s, then 4 s.
        files.started(folder);
        files.finished(folder, None, t);
        assert!(files.get(folder).unwrap().failed);
        assert!(!files.due(folder, t));
        assert!(!files.due(folder, t + secs(1)));
        assert!(files.due(folder, t + secs(2)));
        files.started(folder);
        files.finished(folder, None, t);
        assert!(!files.due(folder, t + secs(3)));
        assert!(files.due(folder, t + secs(4)));
        // A success is back to FRESH.
        files.started(folder);
        files.finished(folder, Some(answer(folder, &["a"])), t);
        assert!(!files.due(folder, t + secs(9)));
        assert!(files.due(folder, t + secs(10)));
    }

    #[test]
    fn palette_files_keeps_each_folder_under_its_own_key() {
        let (a, b) = (Path::new("/a"), Path::new("/b"));
        let mut files = PaletteFiles::default();
        let t = Instant::now();
        files.started(a);
        files.finished(a, Some(answer(a, &["one"])), t);
        files.started(b);
        files.finished(b, Some(answer(b, &["two", "three"])), t);
        assert_eq!(*files.get(a).unwrap().paths, ["one"]);
        assert_eq!(files.get(a).unwrap().root, a);
        assert_eq!(*files.get(b).unwrap().paths, ["two", "three"]);
        assert!(files.get(Path::new("/c")).is_none());
        assert!(files.due(Path::new("/c"), t));
    }

    #[test]
    fn assemble_dedupes_subtracts_and_flags_the_source() {
        // A conflicted path is listed once per stage; a deleted one goes.
        let listed = assemble(b"b a a a gone ", b"gone ", 10);
        assert_eq!(listed.paths, ["a", "b"]);
        assert!(!listed.walked && listed.more == 0);
    }

    #[test]
    fn walk_is_breadth_first_and_flagged() {
        let root = temp("bfs");
        fs::create_dir_all(root.join("a")).unwrap();
        fs::create_dir_all(root.join("b/c")).unwrap();
        fs::write(root.join("a/f"), "").unwrap();
        fs::write(root.join("b/c/f"), "").unwrap();
        let listed = walk(&root, 1, 12, Duration::from_secs(2));
        assert_eq!(listed.paths, ["a/f"]);
        assert!(listed.more >= 1 && listed.walked);
        let all = walk(&root, 10, 12, Duration::from_secs(2));
        assert_eq!(all.more, 0);
    }

    #[test]
    fn walk_skips_library_only_in_the_home_folder() {
        let home = temp("home");
        fs::create_dir_all(home.join("Library")).unwrap();
        fs::write(home.join("Library/secret"), "").unwrap();
        fs::create_dir_all(home.join("Desktop")).unwrap();
        fs::write(home.join("Desktop/secret"), "").unwrap();
        fs::create_dir_all(home.join("proj/Library")).unwrap();
        fs::write(home.join("proj/Library/ok"), "").unwrap();
        fs::write(home.join("notes"), "").unwrap();
        let t = Duration::from_secs(2);
        let at_home = walk_with_home(&home, Some(&home), 100, 12, t);
        assert_eq!(at_home.paths, ["notes", "proj/Library/ok"]);
        let elsewhere = walk_with_home(&home, Some(Path::new("/nonexistent-home")), 100, 12, t);
        assert!(elsewhere.paths.contains(&"Library/secret".to_string()));
        assert!(elsewhere.paths.contains(&"Desktop/secret".to_string()));
    }

    /// The folders macOS asks permission for (and Library) are skipped in the home folder,
    /// whether the walk starts there or reaches it from above; a terminal inside one of them is
    /// still walked as its root, and the same names elsewhere are walked.
    #[test]
    fn walk_skips_the_protected_folders_in_the_home_folder() {
        let outer = temp("protected");
        let home = outer.join("me");
        let protected = [
            "Library",
            "Desktop",
            "Documents",
            "Downloads",
            "Pictures",
            "Movies",
            "Music",
        ];
        for name in protected {
            fs::create_dir_all(home.join(name)).unwrap();
            fs::write(home.join(name).join("secret"), "").unwrap();
            fs::create_dir_all(home.join("proj").join(name)).unwrap();
            fs::write(home.join("proj").join(name).join("ok"), "").unwrap();
        }
        fs::write(home.join("notes"), "").unwrap();
        let t = Duration::from_secs(2);
        let mut want: Vec<String> = protected.iter().map(|n| format!("proj/{n}/ok")).collect();
        want.insert(0, "notes".into());
        want.sort_unstable();
        let at_home = walk_with_home(&home, Some(&home), 100, 12, t);
        assert_eq!(at_home.paths, want);
        // Reached from above (a walk of the folder the home folder is in): skipped there too.
        let above = walk_with_home(&outer, Some(&home), 100, 12, t);
        assert!(
            above.paths.iter().all(|p| !p.ends_with("secret")),
            "{:?}",
            above.paths
        );
        assert!(above.paths.contains(&"me/proj/Desktop/ok".to_string()));
        // A terminal in ~/Documents: walked as its root.
        let inside = walk_with_home(&home.join("Documents"), Some(&home), 100, 12, t);
        assert_eq!(inside.paths, ["secret"]);
    }

    #[test]
    fn a_walk_of_the_disk_skips_the_system_folders() {
        for name in [
            "Volumes",
            "System",
            "dev",
            "private",
            "Library",
            "Applications",
            "cores",
            "opt",
        ] {
            assert!(skipped(name.as_ref(), false, true), "{name} at /");
            assert!(!skipped(name.as_ref(), false, false), "{name} elsewhere");
        }
        assert!(!skipped("Users".as_ref(), false, true));
        assert!(!skipped("usr".as_ref(), false, true));
        assert!(skipped("Desktop".as_ref(), true, false));
        assert!(!skipped("src".as_ref(), true, false));
    }

    fn git_answer(folder: &Path, root: &Path, walked: bool) -> FilesAnswer {
        FilesAnswer {
            folder: folder.to_path_buf(),
            root: root.to_path_buf(),
            listed: Some(Listed {
                paths: vec!["src/a.rs".into()],
                more: 0,
                walked,
            }),
        }
    }

    /// `cd src` inside a listed repo: its listing is reused (not listed again, so no
    /// "Listing files…") until it is older than `FRESH`.
    #[test]
    fn a_folder_inside_a_listed_repo_reuses_its_listing() {
        let (repo, src) = (Path::new("/r"), Path::new("/r/src/deep"));
        let mut files = PaletteFiles::default();
        let t = Instant::now();
        let secs = Duration::from_secs;
        files.started(repo);
        files.finished(repo, Some(git_answer(repo, repo, false)), t);
        let reused = files.get(src).expect("the repo's listing");
        assert_eq!(reused.root, repo);
        assert!(Arc::ptr_eq(&reused.paths, &files.get(repo).unwrap().paths));
        assert!(!files.due(src, t + secs(9)));
        assert!(files.due(src, t + secs(10)));
        // Listed for itself, its own listing is the one used.
        files.started(src);
        files.finished(src, Some(git_answer(src, Path::new("/r/src"), false)), t);
        assert_eq!(files.get(src).unwrap().root, Path::new("/r/src"));
        // Not a folder inside: a sibling whose name starts the same.
        assert!(files.get(Path::new("/rx")).is_none());
        // A walk (not a repo) or a failed listing isn't reused.
        let mut files = PaletteFiles::default();
        files.started(repo);
        files.finished(repo, Some(git_answer(repo, repo, true)), t);
        assert!(files.get(src).is_none() && files.due(src, t));
        let mut files = PaletteFiles::default();
        files.started(repo);
        files.finished(repo, None, t);
        assert!(files.get(src).is_none() && files.due(src, t));
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
    fn rank_files_breaks_ties_by_length_then_path() {
        let paths = strings(&["c/main.rs", "main.rsx", "b/main.rs", "main.rs", "a/main.rs"]);
        let current = AtomicU64::new(1);
        let (hits, total) = rank_files(&paths, "main", 10, 1, &current).unwrap();
        let rels: Vec<&str> = hits.iter().map(|h| h.rel.as_str()).collect();
        assert_eq!(hits[0].score, hits[1].score);
        assert_eq!(hits[2].score, hits[4].score);
        assert_eq!(
            rels,
            ["main.rs", "main.rsx", "a/main.rs", "b/main.rs", "c/main.rs"]
        );
        assert_eq!(total, 5);
    }

    #[test]
    fn the_folder_is_the_freshest_known() {
        let (osc, found, project) = (
            PathBuf::from("/now"),
            Path::new("/looked-up"),
            Path::new("/started"),
        );
        let pick = |r: Option<&PathBuf>, f, p| choose_folder(r.cloned(), f, p, false);
        assert_eq!(
            pick(Some(&osc), Some(found), Some(project)),
            Some((osc.clone(), false))
        );
        assert_eq!(
            pick(None, Some(found), Some(project)),
            Some((found.to_path_buf(), false))
        );
        assert_eq!(
            pick(None, None, Some(project)),
            Some((project.to_path_buf(), false))
        );
        assert_eq!(pick(None, None, None), None);
        // SSH: still an answer (its note), with no folder known.
        assert_eq!(
            choose_folder(None, None, Some(project), true),
            Some((PathBuf::new(), true))
        );
        assert_eq!(
            choose_folder(Some(osc.clone()), None, None, true),
            Some((osc, true))
        );
    }

    #[test]
    fn rank_files_stops_when_a_newer_query_starts() {
        let paths: Vec<String> = (0..10_000).map(|i| format!("dir/file{i}.rs")).collect();
        let current = AtomicU64::new(8);
        assert!(rank_files(&paths, "file", 200, 7, &current).is_none());
        assert!(rank_files(&paths, "file", 200, 8, &current).is_some());
    }
}
