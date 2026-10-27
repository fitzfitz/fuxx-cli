//! M10a: reopening fuxx with the background service. Its live sessions are shown again in the
//! places the sessions file gives them (matched by `daemon_id`); saved sessions that no longer
//! run come back through M7 restore; live ones the file doesn't know are added to the sidebar.
//! Pure and GPUI-free, so the rules are unit-tested; listing and attaching are the caller's.

use std::path::{Path, PathBuf};

use crate::groups;
use crate::mux_proto::SessionInfo;
use crate::presets::Preset;
use crate::restore::{self, Launch, RestoreMode, SavedSession, SavedState};
use crate::split_tree::{self, Node};
use crate::status::Status;

/// A session the background service runs now, as its `List` gives it.
#[derive(Clone, Debug, PartialEq)]
pub struct LiveInfo {
    pub id: u64,
    pub name: Option<String>,
    pub project: Option<PathBuf>,
    /// The preset's name.
    pub preset: Option<String>,
    pub title: Option<String>,
    pub status: Status,
    pub note: Option<String>,
    pub progress: Option<(u8, Option<String>)>,
}

impl LiveInfo {
    /// `None` for a session that has ended (it is reaped soon after).
    pub fn from_list(info: &SessionInfo) -> Option<Self> {
        Some(Self {
            id: info.id,
            name: Some(info.name.clone()).filter(|n| !n.is_empty()),
            project: info.project.as_ref().map(PathBuf::from),
            preset: info.preset.clone(),
            title: info.title.clone(),
            status: Status::parse(&info.status).unwrap_or(Status::Idle),
            note: info.note.clone(),
            progress: info
                .progress
                .as_ref()
                .map(|p| (p.percent.min(100), p.label.clone())),
        })
        .filter(|_| info.alive)
    }
}

/// A live session to show again (never started anew), with what the sidebar shows of it.
#[derive(Clone, Debug, PartialEq)]
pub struct Adopted {
    pub id: u64,
    pub saved: SavedSession,
    pub title: Option<String>,
    pub status: Status,
    pub note: Option<String>,
    pub progress: Option<(u8, Option<String>)>,
}

/// One session of the reopened sidebar.
#[derive(Clone, Debug, PartialEq)]
pub enum Slot {
    Live(Adopted),
    /// Saved, but no longer running: started again (M7 restore).
    Restore(Launch),
}

/// The reopened workspace: the sessions in sidebar order and their layouts (leaves are slot
/// indices; every slot is in exactly one).
#[derive(Clone, Debug, PartialEq)]
pub struct Placement {
    pub slots: Vec<Slot>,
    pub layouts: Vec<Node<usize>>,
    pub collapsed: Vec<PathBuf>,
    /// The active slot (0 when there are none).
    pub active: usize,
}

/// Place the live sessions (`live`, of the service with random id `instance`) by the saved
/// state: a saved session whose `daemon_id` is live in the same service is shown there, with the service's name, note, progress, title and status where it has
/// them; the others are started again per `mode` (`off`: not at all); live sessions the file
/// doesn't know join their group (or the ungrouped run) as layouts of their own. Live ones are
/// always shown, whatever the mode: hiding them would leave processes running unseen.
pub fn place(
    saved: &SavedState,
    live: &[LiveInfo],
    instance: Option<u64>,
    mode: RestoreMode,
    presets: &[Preset],
) -> Placement {
    // Ids are only the same sessions under the same service (a new one numbers from 1 again).
    let same_service = saved.daemon_instance.is_some() && saved.daemon_instance == instance;
    let mut shown: Vec<u64> = Vec::new();
    let mut slots: Vec<(Origin, Slot)> = Vec::new();
    for (i, s) in saved.sessions.iter().enumerate() {
        // A live session is shown once: a second entry with its id (a damaged file) is dead.
        let found = s
            .daemon_id
            .filter(|id| same_service && !shown.contains(id))
            .and_then(|id| live.iter().find(|l| l.id == id));
        let slot = match found {
            Some(info) => {
                shown.push(info.id);
                Some(Slot::Live(adopted(s, info)))
            }
            None => restore::launch(s, mode, presets).map(Slot::Restore),
        };
        if let Some(slot) = slot {
            slots.push((Origin::Saved(i), slot));
        }
    }
    let placed = |i: usize| slots.iter().any(|(o, _)| *o == Origin::Saved(i));
    let mut layouts: Vec<Node<Origin>> = saved
        .layouts
        .iter()
        .filter_map(|t| keep_panes(t, placed))
        .map(|t| t.map(&Origin::Saved))
        .collect();
    for (k, info) in live.iter().filter(|l| !shown.contains(&l.id)).enumerate() {
        let projects: Vec<Option<&Path>> = slots.iter().map(|(_, s)| s.project()).collect();
        let at = groups::insert_index(&projects, info.project.as_deref());
        slots.insert(at, (Origin::New(k), Slot::Live(newcomer(info, presets))));
        layouts.push(Node::Leaf(Origin::New(k)));
    }
    let index_of = |o: Origin| slots.iter().position(|(x, _)| *x == o);
    let trees = layouts
        .iter()
        .map(|t| t.map(&|o| index_of(o).unwrap_or(usize::MAX)))
        .collect();
    let (layouts, _) = split_tree::sanitize(trees, slots.len());
    let active = saved.active.and_then(|i| index_of(Origin::Saved(i)));
    let slots: Vec<Slot> = slots.into_iter().map(|(_, s)| s).collect();
    Placement {
        active: active.or_else(|| first_live(&slots)).unwrap_or(0),
        slots,
        layouts,
        collapsed: saved.collapsed.clone(),
    }
}

impl Placement {
    /// Without the slots `gone` (e.g. sessions that couldn't start): their panes' neighbours
    /// take their space, and the active one falls back if it went.
    pub fn without(self, gone: &[usize]) -> Placement {
        let kept: Vec<usize> = (0..self.slots.len())
            .filter(|i| !gone.contains(i))
            .collect();
        let new_index = |i: usize| kept.iter().position(|&k| k == i);
        let trees = self
            .layouts
            .iter()
            .filter_map(|t| keep_panes(t, |i| new_index(i).is_some()))
            .map(|t| t.map(&|i| new_index(i).unwrap_or(usize::MAX)))
            .collect();
        let (layouts, _) = split_tree::sanitize(trees, kept.len());
        let active = new_index(self.active);
        let slots: Vec<Slot> = self
            .slots
            .into_iter()
            .enumerate()
            .filter(|(i, _)| !gone.contains(i))
            .map(|(_, s)| s)
            .collect();
        Placement {
            active: active.or_else(|| first_live(&slots)).unwrap_or(0),
            slots,
            layouts,
            collapsed: self.collapsed,
        }
    }
}

impl Slot {
    fn project(&self) -> Option<&Path> {
        match self {
            Slot::Live(a) => a.saved.project.as_deref(),
            Slot::Restore(l) => l.saved.project.as_deref(),
        }
    }
}

/// Where a slot came from, so the layouts follow it while the sidebar order is settled.
#[derive(Clone, Copy, PartialEq)]
enum Origin {
    Saved(usize),
    New(usize),
}

/// Saved session `s`, running as `info`: the service's facts where it has them.
fn adopted(s: &SavedSession, info: &LiveInfo) -> Adopted {
    Adopted {
        id: info.id,
        saved: SavedSession {
            name: info.name.clone().or_else(|| s.name.clone()),
            project: s.project.clone().or_else(|| info.project.clone()),
            preset: s.preset.clone(),
            daemon_id: Some(info.id),
        },
        title: info.title.clone(),
        status: info.status,
        note: info.note.clone(),
        progress: info.progress.clone(),
    }
}

/// A live session the file doesn't know (e.g. started by a script while the window was
/// closed). Its preset is today's of that name; a removed one keeps just its name, for the label.
fn newcomer(info: &LiveInfo, presets: &[Preset]) -> Adopted {
    let preset = info.preset.as_ref().map(|name| {
        presets
            .iter()
            .find(|p| p.name == *name)
            .cloned()
            .unwrap_or_else(|| Preset {
                name: name.clone(),
                command: String::new(),
            })
    });
    Adopted {
        id: info.id,
        saved: SavedSession {
            name: info.name.clone(),
            project: info.project.clone(),
            preset,
            daemon_id: Some(info.id),
        },
        title: info.title.clone(),
        status: info.status,
        note: info.note.clone(),
        progress: info.progress.clone(),
    }
}

/// `tree` without the panes `keep` refuses (their neighbours take the space); `None` if none
/// is left.
fn keep_panes(tree: &Node<usize>, keep: impl Fn(usize) -> bool) -> Option<Node<usize>> {
    tree.leaves()
        .into_iter()
        .filter(|&i| !keep(i))
        .try_fold(tree.clone(), |t, i| t.remove(i))
}

fn first_live(slots: &[Slot]) -> Option<usize> {
    slots.iter().position(|s| matches!(s, Slot::Live(_)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mux_proto::{Dims, Progress};
    use crate::presets::builtins;
    use crate::restore::VERSION;
    use crate::split_tree::Axis;

    fn s(name: &str, id: Option<u64>) -> SavedSession {
        SavedSession {
            name: Some(name.into()),
            daemon_id: id,
            ..Default::default()
        }
    }
    fn in_project(mut s: SavedSession, project: &str) -> SavedSession {
        s.project = Some(project.into());
        s
    }
    fn live(id: u64) -> LiveInfo {
        LiveInfo {
            id,
            name: None,
            project: None,
            preset: None,
            title: None,
            status: Status::Idle,
            note: None,
            progress: None,
        }
    }
    fn state(sessions: Vec<SavedSession>, layouts: Vec<Node<usize>>, active: usize) -> SavedState {
        SavedState {
            version: VERSION,
            active: Some(active),
            collapsed: vec![PathBuf::from("/w/api")],
            sessions,
            layouts,
            daemon_instance: Some(INSTANCE),
        }
    }
    fn split(axis: Axis, first: Node<usize>, second: Node<usize>) -> Node<usize> {
        Node::Split {
            axis,
            ratio: 0.3,
            first: Box::new(first),
            second: Box::new(second),
        }
    }
    /// `s<id>` for a live slot, `+name` for one started again.
    fn shown(p: &Placement) -> Vec<String> {
        p.slots
            .iter()
            .map(|slot| match slot {
                Slot::Live(a) => format!("s{}", a.id),
                Slot::Restore(l) => format!("+{}", l.saved.name.as_deref().unwrap_or("?")),
            })
            .collect()
    }
    const INSTANCE: u64 = 0xfeed;
    fn place_all(saved: &SavedState, live: &[LiveInfo], mode: RestoreMode) -> Placement {
        place(saved, live, Some(INSTANCE), mode, &builtins())
    }

    #[test]
    fn ids_from_another_service_match_nothing() {
        let saved = state(
            vec![s("a", Some(1)), s("b", Some(2))],
            vec![split(Axis::Row, Node::Leaf(0), Node::Leaf(1))],
            1,
        );
        let live_now = [live(1), live(2)];
        // A new service (after a reboot) reuses the numbers: they are other sessions.
        let other = place(&saved, &live_now, Some(1), RestoreMode::Agents, &builtins());
        assert_eq!(shown(&other), ["+a", "+b", "s1", "s2"]);
        let off = place(&saved, &live_now, Some(1), RestoreMode::Off, &builtins());
        assert_eq!(shown(&off), ["s1", "s2"]);
        let Slot::Live(one) = &off.slots[0] else {
            panic!()
        };
        assert_eq!(one.saved.name, None, "not the saved session's name");
        // A file from before instance ids, or a service without them: no match either.
        let mut old = saved.clone();
        old.daemon_instance = None;
        let p = place(
            &old,
            &live_now,
            Some(INSTANCE),
            RestoreMode::Off,
            &builtins(),
        );
        assert_eq!(shown(&p), ["s1", "s2"]);
        assert!(matches!(&p.slots[0], Slot::Live(a) if a.saved.name.is_none()));
        let p = place(&saved, &live_now, None, RestoreMode::Off, &builtins());
        assert!(matches!(&p.slots[0], Slot::Live(a) if a.saved.name.is_none()));
        // The same service: matched.
        assert_eq!(
            shown(&place_all(&saved, &live_now, RestoreMode::Off)),
            ["s1", "s2"]
        );
        let Slot::Live(a) = &place_all(&saved, &live_now, RestoreMode::Off).slots[0] else {
            panic!()
        };
        assert_eq!(a.saved.name.as_deref(), Some("a"));
    }

    #[test]
    fn every_saved_session_still_running_is_shown_in_its_place() {
        let saved = state(
            vec![s("a", Some(1)), s("b", Some(2)), s("c", Some(3))],
            vec![
                split(Axis::Row, Node::Leaf(0), Node::Leaf(1)),
                Node::Leaf(2),
            ],
            2,
        );
        // The service lists them in its own order.
        let p = place_all(&saved, &[live(3), live(1), live(2)], RestoreMode::Agents);
        assert_eq!(shown(&p), ["s1", "s2", "s3"]);
        assert_eq!(p.layouts, saved.layouts);
        assert_eq!(p.active, 2);
        assert_eq!(p.collapsed, saved.collapsed);
        let Slot::Live(b) = &p.slots[1] else { panic!() };
        assert_eq!(b.saved.name.as_deref(), Some("b"));
        assert_eq!(b.saved.daemon_id, Some(2));
    }

    #[test]
    fn a_saved_session_no_longer_running_is_started_again_per_the_mode() {
        let claude = Preset {
            name: "claude".into(),
            command: "claude".into(),
        };
        let mut b = s("b", Some(2));
        b.preset = Some(claude.clone());
        // `c` is from a run without the service: it never had an id.
        let saved = state(vec![s("a", Some(1)), b, s("c", None)], vec![], 0);
        let agents = place_all(&saved, &[live(1)], RestoreMode::Agents);
        assert_eq!(shown(&agents), ["s1", "+b", "+c"]);
        let Slot::Restore(launch) = &agents.slots[1] else {
            panic!()
        };
        assert!(launch.run_preset, "agents re-runs the preset");
        assert_eq!(launch.saved.preset, Some(claude));
        let layout = place_all(&saved, &[live(1)], RestoreMode::Layout);
        assert_eq!(shown(&layout), ["s1", "+b", "+c"]);
        assert!(layout.slots.iter().all(|slot| match slot {
            Slot::Restore(l) => !l.run_preset,
            Slot::Live(_) => true,
        }));
    }

    #[test]
    fn live_sessions_the_file_does_not_know_join_their_group() {
        let saved = state(
            vec![
                s("free", Some(1)),
                in_project(s("api", Some(2)), "/w/api"),
                in_project(s("web", Some(3)), "/w/web"),
            ],
            vec![
                Node::Leaf(0),
                split(Axis::Row, Node::Leaf(1), Node::Leaf(2)),
            ],
            2,
        );
        let mut api = live(9);
        api.project = Some("/w/api".into());
        let mut new_group = live(11);
        new_group.project = Some("/w/new".into());
        let p = place_all(
            &saved,
            &[live(1), live(2), live(3), api, live(10), new_group],
            RestoreMode::Agents,
        );
        // After its group; ungrouped at the end of the leading run; a new group at the end.
        assert_eq!(shown(&p), ["s1", "s10", "s2", "s9", "s3", "s11"]);
        let Slot::Live(nine) = &p.slots[3] else {
            panic!()
        };
        assert_eq!(nine.saved.project, Some(PathBuf::from("/w/api")));
        assert_eq!(nine.saved.daemon_id, Some(9));
        // The saved split still holds api and web; each newcomer is a layout of its own.
        assert_eq!(
            p.layouts,
            vec![
                Node::Leaf(0),
                split(Axis::Row, Node::Leaf(2), Node::Leaf(4)),
                Node::Leaf(3),
                Node::Leaf(1),
                Node::Leaf(5),
            ]
        );
        // The active session (web) is still the active one.
        assert_eq!(p.active, 4);
    }

    #[test]
    fn a_layout_loses_a_dead_pane_and_keeps_the_rest() {
        let saved = state(
            vec![s("a", Some(1)), s("b", Some(2)), s("c", Some(3))],
            vec![split(
                Axis::Row,
                Node::Leaf(0),
                split(Axis::Column, Node::Leaf(1), Node::Leaf(2)),
            )],
            0,
        );
        let p = place_all(&saved, &[live(1), live(3)], RestoreMode::Off);
        assert_eq!(shown(&p), ["s1", "s3"]);
        // b's neighbour takes its space.
        assert_eq!(
            p.layouts,
            vec![split(Axis::Row, Node::Leaf(0), Node::Leaf(1))]
        );
    }

    #[test]
    fn broken_saved_layouts_are_sanitized() {
        let mut bad = split(Axis::Row, Node::Leaf(0), Node::Leaf(1));
        if let Node::Split { ratio, .. } = &mut bad {
            *ratio = 7.0;
        }
        let saved = state(
            vec![s("a", Some(1)), s("b", Some(2)), s("c", Some(3))],
            // A ratio out of range, then a repeat of a pane already shown.
            vec![bad, Node::Leaf(1)],
            0,
        );
        let p = place_all(&saved, &[live(1), live(2), live(3)], RestoreMode::Agents);
        let Node::Split { ratio, .. } = &p.layouts[0] else {
            panic!("{:?}", p.layouts)
        };
        assert!(*ratio <= crate::split_tree::MAX_RATIO);
        // Every session in exactly one layout.
        let mut leaves: Vec<usize> = p.layouts.iter().flat_map(|t| t.leaves()).collect();
        leaves.sort();
        assert_eq!(leaves, [0, 1, 2]);
    }

    #[test]
    fn the_active_session_is_kept_or_falls_back_to_the_first_live_one() {
        let two = || vec![s("a", None), s("b", Some(2)), s("c", Some(3))];
        // Kept, renumbered past a session that isn't coming back.
        let kept = place_all(
            &state(two(), vec![], 2),
            &[live(2), live(3)],
            RestoreMode::Off,
        );
        assert_eq!(
            (shown(&kept), kept.active),
            (vec!["s2".into(), "s3".into()], 1)
        );
        // Started again: still the active one.
        let again = place_all(
            &state(two(), vec![], 0),
            &[live(2), live(3)],
            RestoreMode::Agents,
        );
        assert_eq!(again.active, 0);
        // Gone (restore off): the first live one.
        let gone = place_all(
            &state(two(), vec![], 0),
            &[live(2), live(3)],
            RestoreMode::Off,
        );
        assert_eq!(gone.active, 0);
        // None saved (or out of range): the first live one, not the first restored.
        let mut none = state(two(), vec![], 0);
        none.active = None;
        let p = place_all(&none, &[live(3)], RestoreMode::Agents);
        assert_eq!(
            (shown(&p), p.active),
            (vec!["+a".into(), "+b".into(), "s3".into()], 2)
        );
        none.active = Some(99);
        assert_eq!(place_all(&none, &[live(3)], RestoreMode::Agents).active, 2);
        // Nothing live: the first.
        assert_eq!(place_all(&none, &[], RestoreMode::Agents).active, 0);
    }

    #[test]
    fn restore_off_starts_nothing_again_but_shows_every_live_session() {
        let saved = state(vec![s("a", Some(1)), s("b", Some(2))], vec![], 0);
        let p = place_all(&saved, &[live(2), live(7)], RestoreMode::Off);
        assert_eq!(shown(&p), ["s2", "s7"]);
        // No sessions file at all: still every live one.
        let empty = state(vec![], vec![], 0);
        assert_eq!(
            shown(&place_all(&empty, &[live(4), live(5)], RestoreMode::Off)),
            ["s4", "s5"]
        );
        assert!(place_all(&empty, &[], RestoreMode::Agents).slots.is_empty());
    }

    #[test]
    fn the_service_s_metadata_wins_where_it_has_it() {
        let claude = Preset {
            name: "claude".into(),
            command: "claude --continue".into(),
        };
        let mut b = in_project(s("old name", Some(2)), "/w/api");
        b.preset = Some(claude.clone());
        let saved = state(vec![b, s("kept name", Some(3))], vec![], 0);
        let mut two = live(2);
        two.name = Some("new name".into());
        two.title = Some("vim".into());
        two.status = Status::Waiting;
        two.note = Some("tests".into());
        two.progress = Some((40, Some("build".into())));
        let p = place_all(&saved, &[two, live(3)], RestoreMode::Agents);
        let Slot::Live(a) = &p.slots[0] else { panic!() };
        assert_eq!(a.saved.name.as_deref(), Some("new name"));
        assert_eq!(a.title.as_deref(), Some("vim"));
        assert_eq!(a.status, Status::Waiting);
        assert_eq!(a.note.as_deref(), Some("tests"));
        assert_eq!(a.progress, Some((40, Some("build".into()))));
        // What the service doesn't keep comes from the file.
        assert_eq!(a.saved.project, Some(PathBuf::from("/w/api")));
        assert_eq!(a.saved.preset, Some(claude));
        let Slot::Live(c) = &p.slots[1] else { panic!() };
        assert_eq!(c.saved.name.as_deref(), Some("kept name"));
    }

    #[test]
    fn a_newcomer_s_preset_is_today_s_by_name() {
        let mut known = live(5);
        known.preset = Some("claude".into());
        let mut unknown = live(6);
        unknown.preset = Some("gone".into());
        let empty = state(vec![], vec![], 0);
        let p = place_all(&empty, &[known, unknown], RestoreMode::Agents);
        let preset = |i: usize| match &p.slots[i] {
            Slot::Live(a) => a.saved.preset.clone(),
            Slot::Restore(_) => panic!(),
        };
        assert_eq!(
            preset(0),
            builtins().into_iter().find(|p| p.name == "claude")
        );
        // Not in the settings any more: its name still labels the row.
        assert_eq!(preset(1).map(|p| p.name), Some("gone".into()));
    }

    #[test]
    fn one_live_session_is_shown_once_even_if_the_file_names_it_twice() {
        let saved = state(vec![s("a", Some(1)), s("dup", Some(1))], vec![], 0);
        let p = place_all(&saved, &[live(1)], RestoreMode::Off);
        assert_eq!(shown(&p), ["s1"]);
        assert_eq!(p.layouts, vec![Node::Leaf(0)]);
    }

    #[test]
    fn slots_that_could_not_start_leave_the_rest_in_place() {
        let saved = state(
            vec![s("a", None), s("b", Some(2)), s("c", None)],
            vec![
                split(Axis::Row, Node::Leaf(0), Node::Leaf(1)),
                Node::Leaf(2),
            ],
            0,
        );
        let p = place_all(&saved, &[live(2)], RestoreMode::Agents);
        assert_eq!(shown(&p), ["+a", "s2", "+c"]);
        // `a`, the active one, didn't start.
        let p = p.without(&[0]);
        assert_eq!(shown(&p), ["s2", "+c"]);
        assert_eq!(p.layouts, vec![Node::Leaf(0), Node::Leaf(1)]);
        assert_eq!(p.active, 0);
        let none = p.without(&[0, 1]);
        assert!(none.slots.is_empty() && none.layouts.is_empty());
    }

    #[test]
    fn the_list_s_entries_become_live_infos() {
        let info = SessionInfo {
            id: 4,
            name: String::new(),
            project: Some("/w/api".into()),
            preset: Some("claude".into()),
            title: Some("t".into()),
            status: "waiting".into(),
            note: Some("n".into()),
            progress: Some(Progress {
                percent: 30,
                label: None,
            }),
            dims: Dims { cols: 80, rows: 24 },
            alive: true,
            created_at: 0,
        };
        let l = LiveInfo::from_list(&info).unwrap();
        assert_eq!(l.name, None, "empty means no name");
        assert_eq!(l.project, Some(PathBuf::from("/w/api")));
        assert_eq!(l.status, Status::Waiting);
        assert_eq!(l.progress, Some((30, None)));
        let named = SessionInfo {
            name: "api".into(),
            ..info.clone()
        };
        assert_eq!(
            LiveInfo::from_list(&named).unwrap().name.as_deref(),
            Some("api")
        );
        let ended = SessionInfo {
            alive: false,
            ..info
        };
        assert_eq!(LiveInfo::from_list(&ended), None);
    }
}
