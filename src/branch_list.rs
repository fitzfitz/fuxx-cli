#![allow(dead_code)] // consumed by the later M11o tasks (the switcher modal)

//! The branch switcher's pure model (note 66 §2-§3): parse `git for-each-ref`, build and filter
//! the rows, and the copy shown in the modal. No git, no GPUI.

use crate::palette_match;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Head {
    Branch(String),
    /// Short sha.
    Detached(String),
    /// Branch name of a repo with no commits yet.
    Unborn(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Track {
    None,
    Even,
    Diverged { ahead: u32, behind: u32 },
    Gone,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalRef {
    pub name: String,
    /// e.g. `origin/foo`.
    pub upstream: Option<String>,
    pub track: Track,
    pub date: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteRef {
    /// Without the `origin/` prefix.
    pub name: String,
    pub date: i64,
}

fn parse_track(s: &str) -> Track {
    let inner = s.trim().trim_start_matches('[').trim_end_matches(']');
    if inner.is_empty() {
        return Track::Even;
    }
    if inner == "gone" {
        return Track::Gone;
    }
    let (mut ahead, mut behind) = (0, 0);
    for part in inner.split(',') {
        let mut w = part.split_whitespace();
        match (w.next(), w.next().and_then(|n| n.parse::<u32>().ok())) {
            (Some("ahead"), Some(n)) => ahead = n,
            (Some("behind"), Some(n)) => behind = n,
            _ => {}
        }
    }
    if ahead == 0 && behind == 0 {
        Track::Even
    } else {
        Track::Diverged { ahead, behind }
    }
}

/// Parse `git for-each-ref --format='%(refname)%00%(upstream)%00%(upstream:track)%00%(committerdate:unix)'
/// refs/heads refs/remotes/origin`. `refs/remotes/origin/HEAD` is dropped.
pub fn parse_refs(out: &str) -> (Vec<LocalRef>, Vec<RemoteRef>) {
    let (mut locals, mut remotes) = (Vec::new(), Vec::new());
    for line in out.lines() {
        let mut f = line.split('\0');
        let (Some(refname), Some(upstream), Some(track), Some(date)) =
            (f.next(), f.next(), f.next(), f.next())
        else {
            continue;
        };
        let date = date.trim().parse::<i64>().unwrap_or(0);
        if let Some(name) = refname.strip_prefix("refs/heads/") {
            let (upstream, track) = if upstream.is_empty() {
                (None, Track::None)
            } else {
                let short = upstream.strip_prefix("refs/remotes/").unwrap_or(upstream);
                (Some(short.to_string()), parse_track(track))
            };
            locals.push(LocalRef {
                name: name.into(),
                upstream,
                track,
                date,
            });
        } else if let Some(name) = refname.strip_prefix("refs/remotes/origin/")
            && name != "HEAD"
        {
            remotes.push(RemoteRef {
                name: name.into(),
                date,
            });
        }
    }
    (locals, remotes)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Local(String),
    Origin(String),
}

impl Target {
    pub fn name(&self) -> &str {
        match self {
            Target::Local(n) | Target::Origin(n) => n,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mark {
    Current,
    Even,
    Diverged { ahead: u32, behind: u32 },
    LocalOnly,
    UpstreamGone,
    None,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub target: Target,
    pub label: String,
    pub mark: Mark,
    /// Files the switch would clobber (`Some(n)` only when the branch has a conflict entry).
    pub conflict: Option<usize>,
    pub origin_section: bool,
}

fn diverged_or_even(ahead: u32, behind: u32) -> Mark {
    if ahead == 0 && behind == 0 {
        Mark::Even
    } else {
        Mark::Diverged { ahead, behind }
    }
}

/// The list: the current row first, local branches by date (newest first), then the ORIGIN
/// rows (those with no same-named local branch) by date. `pushed` is ahead/behind for local
/// branches with no upstream but a same-named `origin/<name>`.
pub fn rows(
    head: &Head,
    local: &[LocalRef],
    origin: &[RemoteRef],
    pushed: &HashMap<String, (u32, u32)>,
    conflicts: &HashMap<String, usize>,
) -> Vec<Row> {
    let mut out = Vec::new();
    let current_name = match head {
        Head::Branch(n) | Head::Unborn(n) => {
            out.push(Row {
                target: Target::Local(n.clone()),
                label: n.clone(),
                mark: Mark::Current,
                conflict: None,
                origin_section: false,
            });
            Some(n.as_str())
        }
        Head::Detached(sha) => {
            out.push(Row {
                target: Target::Local(sha.clone()),
                label: format!("detached at {sha}"),
                mark: Mark::Current,
                conflict: None,
                origin_section: false,
            });
            None
        }
    };
    let mut locals: Vec<&LocalRef> = local
        .iter()
        .filter(|l| Some(l.name.as_str()) != current_name)
        .collect();
    locals.sort_by_key(|l| std::cmp::Reverse(l.date));
    for l in locals {
        let mark = match &l.track {
            Track::Even => Mark::Even,
            Track::Diverged { ahead, behind } => Mark::Diverged {
                ahead: *ahead,
                behind: *behind,
            },
            Track::Gone => Mark::UpstreamGone,
            Track::None => match pushed.get(&l.name) {
                Some(&(a, b)) => diverged_or_even(a, b),
                None => Mark::LocalOnly,
            },
        };
        out.push(Row {
            target: Target::Local(l.name.clone()),
            label: l.name.clone(),
            mark,
            conflict: conflicts.get(&l.name).copied(),
            origin_section: false,
        });
    }
    let have: HashSet<&str> = local
        .iter()
        .map(|l| l.name.as_str())
        .chain(current_name)
        .collect();
    let mut remotes: Vec<&RemoteRef> = origin
        .iter()
        .filter(|r| !have.contains(r.name.as_str()))
        .collect();
    remotes.sort_by_key(|r| std::cmp::Reverse(r.date));
    for r in remotes {
        out.push(Row {
            target: Target::Origin(r.name.clone()),
            label: r.name.clone(),
            mark: Mark::None,
            conflict: conflicts.get(&r.name).copied(),
            origin_section: true,
        });
    }
    out
}

/// Indices of the rows whose label fuzzy-matches `query`, list order kept.
pub fn visible(rows: &[Row], query: &str) -> Vec<usize> {
    rows.iter()
        .enumerate()
        .filter(|(_, r)| palette_match::fuzzy(query, &r.label).is_some())
        .map(|(i, _)| i)
        .collect()
}

/// The trimmed query, when non-empty and equal to no row's name (offer to create it).
pub fn typed_name(rows: &[Row], query: &str) -> Option<String> {
    let q = query.trim();
    (!q.is_empty() && !rows.iter().any(|r| r.target.name() == q)).then(|| q.to_string())
}

pub fn selectable(row: &Row) -> bool {
    row.mark != Mark::Current && row.conflict.unwrap_or(0) == 0
}

/// Uncommitted files that the target branch also changes.
pub fn conflict_count(dirty: &HashSet<PathBuf>, changed: &HashSet<PathBuf>) -> usize {
    dirty.intersection(changed).count()
}

pub fn mark_text(mark: &Mark) -> Option<String> {
    match mark {
        Mark::Current => Some("current".into()),
        Mark::Even => Some("✓".into()),
        Mark::Diverged { ahead, behind } => {
            let mut parts = Vec::new();
            if *ahead > 0 {
                parts.push(format!("↑{ahead}"));
            }
            if *behind > 0 {
                parts.push(format!("↓{behind}"));
            }
            Some(parts.join(" "))
        }
        Mark::LocalOnly => Some("local only".into()),
        Mark::UpstreamGone => Some("upstream gone".into()),
        Mark::None => None,
    }
}

pub fn conflict_text(n: usize) -> String {
    format!("conflict {n} file(s)")
}

pub fn running_line(programs: &[String]) -> Option<String> {
    match programs {
        [] => None,
        [one] => Some(format!("{one} is running in this folder")),
        many => Some(format!(
            "{} programs are running in this folder",
            many.len()
        )),
    }
}

/// The confirmation: the title, then each warning that applies.
pub fn confirm_lines(
    name: &str,
    programs: &[String],
    carried: usize,
    orphans: usize,
) -> Vec<String> {
    let mut lines = vec![format!("Switch to {name}?")];
    lines.extend(running_line(programs));
    if carried > 0 {
        lines.push(format!("{carried} uncommitted file(s) come along"));
    }
    if orphans > 0 {
        lines.push(format!(
            "{orphans} commit(s) on no branch: after switching they are only in the reflog"
        ));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, HashSet};
    use std::path::PathBuf;

    fn local(name: &str, up: Option<&str>, track: Track, date: i64) -> LocalRef {
        LocalRef {
            name: name.into(),
            upstream: up.map(String::from),
            track,
            date,
        }
    }
    fn remote(name: &str, date: i64) -> RemoteRef {
        RemoteRef {
            name: name.into(),
            date,
        }
    }
    fn labels(rows: &[Row]) -> Vec<&str> {
        rows.iter().map(|r| r.label.as_str()).collect()
    }
    fn sample() -> (Vec<LocalRef>, Vec<RemoteRef>) {
        (
            vec![
                local("main", Some("origin/main"), Track::Even, 100),
                local("spike", None, Track::None, 150),
                local(
                    "feature/rows",
                    Some("origin/feature/rows"),
                    Track::Diverged {
                        ahead: 2,
                        behind: 1,
                    },
                    300,
                ),
            ],
            vec![
                remote("main", 100),
                remote("feature/rows", 300),
                remote("fix-login", 400),
                remote("old-one", 50),
            ],
        )
    }

    #[test]
    fn refs_parse_with_their_tracking() {
        let out = "refs/heads/main\0refs/remotes/origin/main\0\x001700000300\n\
                   refs/heads/feature/rows\0refs/remotes/origin/feature/rows\0[ahead 2, behind 1]\x001700000200\n\
                   refs/heads/spike\0\0\x001700000100\n\
                   refs/heads/fix/cache\0refs/remotes/origin/fix/cache\0[gone]\x001700000050\n\
                   refs/remotes/origin/HEAD\0\0\x001700000300\n\
                   refs/remotes/origin/main\0\0\x001700000300\n\
                   refs/remotes/origin/feature/rows\0\0\x001700000200\n\
                   refs/remotes/origin/fix/cache\0\0\x001700000050\n\
                   refs/remotes/origin/fix-login\0\0\x001700000400\n";
        let (local, origin) = parse_refs(out);
        assert_eq!(
            local.iter().map(|l| l.name.as_str()).collect::<Vec<_>>(),
            ["main", "feature/rows", "spike", "fix/cache"]
        );
        assert_eq!(local[0].track, Track::Even);
        assert_eq!(
            local[1].track,
            Track::Diverged {
                ahead: 2,
                behind: 1
            }
        );
        assert_eq!(
            (local[2].upstream.as_deref(), &local[2].track),
            (None, &Track::None)
        );
        assert_eq!(local[3].track, Track::Gone);
        assert_eq!(
            origin.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(),
            ["main", "feature/rows", "fix/cache", "fix-login"]
        );
    }

    #[test]
    fn rows_put_the_current_first_then_local_then_origin_by_date() {
        let (l, o) = sample();
        let rows = rows(
            &Head::Branch("main".into()),
            &l,
            &o,
            &HashMap::new(),
            &HashMap::new(),
        );
        assert_eq!(
            labels(&rows),
            ["main", "feature/rows", "spike", "fix-login", "old-one"]
        );
        assert_eq!(rows[0].mark, Mark::Current);
        assert_eq!(
            rows[1].mark,
            Mark::Diverged {
                ahead: 2,
                behind: 1
            }
        );
        assert_eq!(rows[2].mark, Mark::LocalOnly);
        assert!(!rows[2].origin_section);
        assert!(rows[3].origin_section && rows[4].origin_section);
        assert_eq!(rows[3].target, Target::Origin("fix-login".into()));
        assert_eq!(rows[1].target, Target::Local("feature/rows".into()));
    }

    #[test]
    fn a_branch_pushed_without_an_upstream_is_on_origin() {
        let l = vec![
            local("main", None, Track::None, 1),
            local("spike", None, Track::None, 2),
        ];
        let o = vec![remote("spike", 2)];
        let pushed = HashMap::from([("spike".to_string(), (1u32, 0u32))]);
        let rows = rows(
            &Head::Branch("main".into()),
            &l,
            &o,
            &pushed,
            &HashMap::new(),
        );
        let spike = rows.iter().find(|r| r.label == "spike").unwrap();
        assert_eq!(
            spike.mark,
            Mark::Diverged {
                ahead: 1,
                behind: 0
            }
        );
        assert!(!rows.iter().any(|r| r.origin_section));
    }

    #[test]
    fn detached_and_unborn_heads() {
        let (l, o) = sample();
        let rows_d = rows(
            &Head::Detached("abc1234".into()),
            &l,
            &o,
            &HashMap::new(),
            &HashMap::new(),
        );
        assert_eq!(rows_d[0].label, "detached at abc1234");
        assert_eq!(rows_d[0].mark, Mark::Current);
        assert!(!selectable(&rows_d[0]));
        assert!(
            rows_d
                .iter()
                .filter(|r| !r.origin_section)
                .skip(1)
                .all(selectable)
        );
        assert_eq!(rows_d.iter().filter(|r| !r.origin_section).count(), 4);

        let rows_u = rows(
            &Head::Unborn("trunk".into()),
            &[],
            &[],
            &HashMap::new(),
            &HashMap::new(),
        );
        assert_eq!(labels(&rows_u), ["trunk"]);
        assert_eq!(rows_u[0].mark, Mark::Current);
        assert!(!rows_u.iter().any(|r| r.origin_section));
    }

    #[test]
    fn conflicts_disable_a_row_and_count_the_overlap() {
        let dirty: HashSet<PathBuf> = ["src/a.rs", "notes.txt", "new.rs"]
            .map(PathBuf::from)
            .into();
        let changed: HashSet<PathBuf> =
            ["src/a.rs", "new.rs", "src/b.rs"].map(PathBuf::from).into();
        assert_eq!(conflict_count(&dirty, &changed), 2);
        let l = vec![
            local("main", None, Track::None, 2),
            local("feature/old", None, Track::None, 1),
        ];
        let conflicts = HashMap::from([("feature/old".to_string(), 3usize)]);
        let rows = rows(
            &Head::Branch("main".into()),
            &l,
            &[],
            &HashMap::new(),
            &conflicts,
        );
        let old = rows.iter().find(|r| r.label == "feature/old").unwrap();
        assert_eq!(old.conflict, Some(3));
        assert!(!selectable(old));
        assert_eq!(conflict_text(3), "conflict 3 file(s)");
    }

    #[test]
    fn the_box_filters_and_offers_a_typed_name() {
        let l = vec![
            local("main", None, Track::None, 2),
            local("feature/rows", None, Track::None, 1),
        ];
        let o = vec![remote("fix-login", 5)];
        let rows = rows(
            &Head::Branch("main".into()),
            &l,
            &o,
            &HashMap::new(),
            &HashMap::new(),
        );
        let idx = rows.iter().position(|r| r.label == "feature/rows").unwrap();
        assert_eq!(visible(&rows, "rows"), [idx]);
        assert_eq!(visible(&rows, "").len(), 3);
        assert_eq!(typed_name(&rows, "rows").as_deref(), Some("rows"));
        assert_eq!(typed_name(&rows, "fix-login"), None);
        assert_eq!(typed_name(&rows, "  "), None);
    }

    #[test]
    fn marks_and_confirmation_copy() {
        assert_eq!(
            mark_text(&Mark::Diverged {
                ahead: 2,
                behind: 1
            })
            .as_deref(),
            Some("↑2 ↓1")
        );
        assert_eq!(
            mark_text(&Mark::Diverged {
                ahead: 2,
                behind: 0
            })
            .as_deref(),
            Some("↑2")
        );
        assert_eq!(mark_text(&Mark::Even).as_deref(), Some("✓"));
        assert_eq!(mark_text(&Mark::LocalOnly).as_deref(), Some("local only"));
        assert_eq!(
            mark_text(&Mark::UpstreamGone).as_deref(),
            Some("upstream gone")
        );
        assert_eq!(mark_text(&Mark::Current).as_deref(), Some("current"));
        assert_eq!(mark_text(&Mark::None), None);
        assert_eq!(
            running_line(&["Claude Code".into()]).as_deref(),
            Some("Claude Code is running in this folder")
        );
        assert_eq!(
            running_line(&["a".into(), "b".into()]).as_deref(),
            Some("2 programs are running in this folder")
        );
        assert_eq!(running_line(&[]), None);
        assert_eq!(
            confirm_lines("feature/rows", &["Claude Code".into()], 3, 2),
            [
                "Switch to feature/rows?",
                "Claude Code is running in this folder",
                "3 uncommitted file(s) come along",
                "2 commit(s) on no branch: after switching they are only in the reflog"
            ]
        );
        assert_eq!(confirm_lines("main", &[], 0, 0), ["Switch to main?"]);
    }
}
