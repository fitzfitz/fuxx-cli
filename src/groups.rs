//! Project groups, session names and ordering. Pure and GPUI-free, so the rules are unit-tested.

use crate::status::Status;
use std::path::{Path, PathBuf};

/// The name a session shows when nothing better is known.
pub const DEFAULT_NAME: &str = "╭∩╮";

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

/// The program's title, when it says something `name` doesn't (shown under the name in a
/// tooltip); none when it is blank or the same.
pub fn extra_title<'a>(name: &str, title: Option<&'a str>) -> Option<&'a str> {
    title.filter(|t| !t.trim().is_empty() && t.trim() != name.trim())
}

/// What a sidebar group is for: a folder (M7), or a host (M10b SSH sessions).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum GroupKey {
    Folder(PathBuf),
    Host(String),
}

/// A session's group: its host if it is an SSH session, else its folder, else none.
pub fn key(host: Option<&str>, project: Option<&Path>) -> Option<GroupKey> {
    match (host, project) {
        (Some(host), _) => Some(GroupKey::Host(host.to_string())),
        (None, Some(project)) => Some(GroupKey::Folder(project.to_path_buf())),
        (None, None) => None,
    }
}

/// The collapsed groups as the sessions file keeps them (folders and hosts apart), joined.
pub fn collapsed_keys(folders: &[PathBuf], hosts: &[String]) -> Vec<GroupKey> {
    let folders = folders.iter().cloned().map(GroupKey::Folder);
    folders
        .chain(hosts.iter().cloned().map(GroupKey::Host))
        .collect()
}

fn last_component(path: &Path) -> Option<String> {
    path.file_name().map(|n| n.to_string_lossy().into_owned())
}

/// A host group's label is the host. A folder's is its name; when a different folder in `all`
/// shares it, the parent's name is appended.
pub fn group_label(key: &GroupKey, all: &[&GroupKey]) -> String {
    let project = match key {
        GroupKey::Host(host) => return host.clone(),
        GroupKey::Folder(project) => project.as_path(),
    };
    let Some(name) = last_component(project) else {
        return project.display().to_string();
    };
    let clash = all.iter().any(|k| match k {
        GroupKey::Folder(p) => p != project && last_component(p).as_deref() == Some(name.as_str()),
        GroupKey::Host(_) => false,
    });
    match project.parent().and_then(last_component) {
        Some(parent) if clash => format!("{name} ({parent})"),
        _ => name,
    }
}

/// Where a new session goes so groups stay together: ungrouped ones sit in the leading run,
/// a grouped one goes right after its group (or at the end when the group is new).
pub fn insert_index(keys: &[Option<&GroupKey>], key: Option<&GroupKey>) -> usize {
    match key {
        None => keys.iter().take_while(|k| k.is_none()).count(),
        Some(k) => keys
            .iter()
            .rposition(|q| *q == Some(k))
            .map_or(keys.len(), |i| i + 1),
    }
}

/// How much a status needs you, for roll-ups: Waiting > Done > Working > Idle.
pub fn urgency(status: Status) -> u8 {
    match status {
        Status::Idle => 0,
        Status::Working => 1,
        Status::Done => 2,
        Status::Waiting => 3,
    }
}

/// The most urgent status of a group: Waiting > Done > Working > Idle.
pub fn rolled_up(statuses: impl IntoIterator<Item = Status>) -> Status {
    statuses
        .into_iter()
        .max_by_key(|s| urgency(*s))
        .unwrap_or(Status::Idle)
}

/// One sidebar row: a group header, or a session (by index into the session list).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Row {
    Header {
        key: GroupKey,
        first: usize,
        len: usize,
        collapsed: bool,
    },
    Session {
        index: usize,
        grouped: bool,
    },
}

/// The sidebar rows: leading ungrouped sessions, then each group's run under a header
/// (its sessions are omitted when the group is collapsed).
pub fn rows(keys: &[Option<&GroupKey>], collapsed: &[GroupKey]) -> Vec<Row> {
    let mut out = Vec::new();
    let lead = keys.iter().take_while(|k| k.is_none()).count();
    out.extend((0..lead).map(|index| Row::Session {
        index,
        grouped: false,
    }));
    let mut i = lead;
    while i < keys.len() {
        let Some(key) = keys[i] else {
            // Ungrouped session after a group: not produced by insert_index; show it plainly.
            out.push(Row::Session {
                index: i,
                grouped: false,
            });
            i += 1;
            continue;
        };
        let len = keys[i..].iter().take_while(|k| **k == Some(key)).count();
        let is_collapsed = collapsed.contains(key);
        out.push(Row::Header {
            key: key.clone(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_title_shows_in_a_tooltip_only_when_it_says_more() {
        assert_eq!(
            extra_title("api", Some("me@mac:~/fuxx-cli")),
            Some("me@mac:~/fuxx-cli")
        );
        assert_eq!(extra_title("fuxx-cli", Some("fuxx-cli")), None);
        assert_eq!(extra_title("fuxx-cli", Some("  ")), None);
        assert_eq!(extra_title("fuxx-cli", None), None);
    }

    #[test]
    fn auto_name_prefers_user_then_title_then_preset() {
        assert_eq!(auto_name(Some("api"), Some("vim"), Some("claude")), "api");
        assert_eq!(auto_name(Some("  "), Some(" vim "), Some("claude")), "vim");
        assert_eq!(auto_name(None, None, Some("claude")), "claude");
        assert_eq!(auto_name(None, None, None), "╭∩╮");
    }

    fn folder(path: &str) -> GroupKey {
        GroupKey::Folder(path.into())
    }

    #[test]
    fn group_label_adds_the_parent_only_on_a_clash() {
        let (a, b, c) = (
            folder("/w/work/api"),
            folder("/w/home/api"),
            folder("/w/web"),
        );
        assert_eq!(group_label(&a, &[&a, &b, &c]), "api (work)");
        assert_eq!(group_label(&c, &[&a, &b, &c]), "web");
        assert_eq!(group_label(&a, &[&a, &a]), "api");
    }

    #[test]
    fn insert_index_keeps_groups_together() {
        let (a, b) = (folder("/a"), folder("/b"));
        let p = [None, Some(&a), Some(&a), Some(&b)];
        assert_eq!(insert_index(&p, None), 1);
        assert_eq!(insert_index(&p, Some(&a)), 3);
        assert_eq!(insert_index(&p, Some(&b)), 4);
        assert_eq!(insert_index(&p, Some(&folder("/c"))), 4);
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
        let (a, b) = (folder("/a"), folder("/b"));
        let p = [None, Some(&a), Some(&a), Some(&b)];
        assert_eq!(
            rows(&p, std::slice::from_ref(&a)),
            vec![
                Row::Session {
                    index: 0,
                    grouped: false
                },
                Row::Header {
                    key: a.clone(),
                    first: 1,
                    len: 2,
                    collapsed: true
                },
                Row::Header {
                    key: b.clone(),
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
    fn groups_key_by_folder_or_host() {
        let api = GroupKey::Folder("/w/work/api".into());
        let other_api = GroupKey::Folder("/w/home/api".into());
        let dev = GroupKey::Host("devbox".into());
        let host_api = GroupKey::Host("api".into());
        // A host group is labelled with the host; only folders clash with folders.
        assert_eq!(group_label(&dev, &[&api, &dev]), "devbox");
        assert_eq!(group_label(&api, &[&api, &host_api]), "api");
        assert_eq!(group_label(&api, &[&api, &other_api]), "api (work)");
        // The host wins: an SSH session has no folder group.
        assert_eq!(
            key(Some("devbox"), Some(Path::new("/w/work/api"))),
            Some(dev.clone())
        );
        assert_eq!(key(None, Some(Path::new("/w/work/api"))), Some(api.clone()));
        assert_eq!(key(None, None), None);
        // Ungrouped first, then the groups in first-seen order.
        let keys = [None, Some(&api), Some(&dev), Some(&dev)];
        assert_eq!(insert_index(&keys, None), 1);
        assert_eq!(insert_index(&keys, Some(&api)), 2);
        assert_eq!(insert_index(&keys, Some(&dev)), 4);
        assert_eq!(insert_index(&keys, Some(&GroupKey::Host("new".into()))), 4);
        // A collapsed host hides its sessions.
        assert_eq!(
            rows(&keys, std::slice::from_ref(&dev)),
            vec![
                Row::Session {
                    index: 0,
                    grouped: false
                },
                Row::Header {
                    key: api.clone(),
                    first: 1,
                    len: 1,
                    collapsed: false
                },
                Row::Session {
                    index: 1,
                    grouped: true
                },
                Row::Header {
                    key: dev.clone(),
                    first: 2,
                    len: 2,
                    collapsed: true
                },
            ]
        );
        // A folder and a host of the same name are different groups.
        let same_name = GroupKey::Folder("api".into());
        assert_ne!(same_name, host_api);
        assert_eq!(keys[2], keys[3]);
        // The sessions file keeps collapsed folders and hosts apart; they join back in order.
        assert_eq!(
            collapsed_keys(&[PathBuf::from("/w/work/api")], &["devbox".to_string()]),
            vec![api, dev]
        );
    }
}
