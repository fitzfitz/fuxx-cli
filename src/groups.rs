//! Project groups, session names and ordering. Pure and GPUI-free, so the rules are unit-tested.

use crate::status::Status;
use std::path::{Path, PathBuf};

/// The name a session shows when nothing better is known.
pub const DEFAULT_NAME: &str = "🖕";

/// First of user name, terminal title, preset name that is not blank (trimmed), else the default.
pub fn auto_name(user: Option<&str>, title: Option<&str>, preset: Option<&str>) -> String {
    [user, title, preset]
        .into_iter()
        .flatten()
        .map(str::trim)
        .find(|s| !s.is_empty())
        .unwrap_or(DEFAULT_NAME)
        .to_string()
}

fn last_component(path: &Path) -> Option<String> {
    path.file_name().map(|n| n.to_string_lossy().into_owned())
}

/// The folder's name; when a different path in `all` shares it, the parent's name is appended.
pub fn group_label(project: &Path, all: &[&Path]) -> String {
    let Some(name) = last_component(project) else {
        return project.display().to_string();
    };
    let clash = all
        .iter()
        .any(|p| *p != project && last_component(p).as_deref() == Some(name.as_str()));
    match project.parent().and_then(last_component) {
        Some(parent) if clash => format!("{name} ({parent})"),
        _ => name,
    }
}

/// Where a new session goes so groups stay together: ungrouped ones sit in the leading run,
/// a grouped one goes right after its group (or at the end when the group is new).
pub fn insert_index(projects: &[Option<&Path>], project: Option<&Path>) -> usize {
    match project {
        None => projects.iter().take_while(|p| p.is_none()).count(),
        Some(p) => projects
            .iter()
            .rposition(|q| *q == Some(p))
            .map_or(projects.len(), |i| i + 1),
    }
}

/// The most urgent status of a group: Waiting > Done > Working > Idle.
pub fn rolled_up(statuses: impl IntoIterator<Item = Status>) -> Status {
    let rank = |s: Status| match s {
        Status::Idle => 0,
        Status::Working => 1,
        Status::Done => 2,
        Status::Waiting => 3,
    };
    statuses
        .into_iter()
        .max_by_key(|s| rank(*s))
        .unwrap_or(Status::Idle)
}

/// One sidebar row: a project header, or a session (by index into the session list).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Row {
    Header {
        project: PathBuf,
        first: usize,
        len: usize,
        collapsed: bool,
    },
    Session {
        index: usize,
        grouped: bool,
    },
}

/// The sidebar rows: leading ungrouped sessions, then each project run under a header
/// (its sessions are omitted when the project is collapsed).
pub fn rows(projects: &[Option<&Path>], collapsed: &[PathBuf]) -> Vec<Row> {
    let mut out = Vec::new();
    let lead = projects.iter().take_while(|p| p.is_none()).count();
    out.extend((0..lead).map(|index| Row::Session {
        index,
        grouped: false,
    }));
    let mut i = lead;
    while i < projects.len() {
        let Some(project) = projects[i] else {
            // Ungrouped session after a group: not produced by insert_index; show it plainly.
            out.push(Row::Session {
                index: i,
                grouped: false,
            });
            i += 1;
            continue;
        };
        let len = projects[i..]
            .iter()
            .take_while(|p| **p == Some(project))
            .count();
        let is_collapsed = collapsed.iter().any(|c| c == project);
        out.push(Row::Header {
            project: project.to_path_buf(),
            first: i,
            len,
            collapsed: is_collapsed,
        });
        if !is_collapsed {
            out.extend((i..i + len).map(|index| Row::Session {
                index,
                grouped: true,
            }));
        }
        i += len;
    }
    out
}

/// True when both indexes exist and belong to the same project (or are both ungrouped).
pub fn same_group(projects: &[Option<&Path>], a: usize, b: usize) -> bool {
    a < projects.len() && b < projects.len() && projects[a] == projects[b]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_name_prefers_user_then_title_then_preset() {
        assert_eq!(auto_name(Some("api"), Some("vim"), Some("claude")), "api");
        assert_eq!(auto_name(Some("  "), Some(" vim "), Some("claude")), "vim");
        assert_eq!(auto_name(None, None, Some("claude")), "claude");
        assert_eq!(auto_name(None, None, None), "🖕");
    }

    #[test]
    fn group_label_adds_the_parent_only_on_a_clash() {
        let (a, b, c) = (
            Path::new("/w/work/api"),
            Path::new("/w/home/api"),
            Path::new("/w/web"),
        );
        assert_eq!(group_label(a, &[a, b, c]), "api (work)");
        assert_eq!(group_label(c, &[a, b, c]), "web");
        assert_eq!(group_label(a, &[a, a]), "api");
    }

    #[test]
    fn insert_index_keeps_groups_together() {
        let (a, b) = (Path::new("/a"), Path::new("/b"));
        let p = [None, Some(a), Some(a), Some(b)];
        assert_eq!(insert_index(&p, None), 1);
        assert_eq!(insert_index(&p, Some(a)), 3);
        assert_eq!(insert_index(&p, Some(b)), 4);
        assert_eq!(insert_index(&p, Some(Path::new("/c"))), 4);
        assert_eq!(insert_index(&[], None), 0);
    }

    #[test]
    fn rolled_up_picks_the_most_urgent() {
        assert_eq!(
            rolled_up([Status::Working, Status::Done, Status::Idle]),
            Status::Done
        );
        assert_eq!(rolled_up([Status::Done, Status::Waiting]), Status::Waiting);
        assert_eq!(rolled_up([Status::Idle, Status::Working]), Status::Working);
        assert_eq!(rolled_up([]), Status::Idle);
    }

    #[test]
    fn rows_hide_collapsed_groups() {
        let (a, b) = (Path::new("/a"), Path::new("/b"));
        let p = [None, Some(a), Some(a), Some(b)];
        assert_eq!(
            rows(&p, &[a.to_path_buf()]),
            vec![
                Row::Session {
                    index: 0,
                    grouped: false
                },
                Row::Header {
                    project: a.into(),
                    first: 1,
                    len: 2,
                    collapsed: true
                },
                Row::Header {
                    project: b.into(),
                    first: 3,
                    len: 1,
                    collapsed: false
                },
                Row::Session {
                    index: 3,
                    grouped: true
                },
            ]
        );
    }

    #[test]
    fn same_group_rules() {
        let (a, b) = (Path::new("/a"), Path::new("/b"));
        let p = [None, None, Some(a), Some(a), Some(b)];
        assert!(same_group(&p, 0, 1));
        assert!(same_group(&p, 2, 3));
        assert!(!same_group(&p, 1, 2));
        assert!(!same_group(&p, 3, 4));
        assert!(!same_group(&p, 3, 9));
    }
}
