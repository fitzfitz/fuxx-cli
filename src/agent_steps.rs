//! The step model (the agent activity chore): a narration line and the run of tool calls after
//! it, as the activity view draws it. Pure and GPUI-free.

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};

use crate::agent_activity::{
    Activity, Check, CheckWhat, How, Outcome, ToolKind, Touch, file_name, of,
};
use crate::agent_chat::{Shape, group_shape_at};
use crate::agent_conversation::{Conversation, Item};
use crate::agent_step_words::{
    checks_failing, checks_summary, column_tip, commands_summary, counts, file_summary,
};

/// Each tool call's live start and finish (None while it runs), by its id. History never
/// enters it, so restored calls have no times.
pub type ToolTimes = HashMap<String, (Instant, Option<Instant>)>;

/// A step of this many calls or more draws as tracks; fewer, as readable rows.
pub const TRACKS_FROM: usize = 3;
/// The columns shown (the latest calls); "+M earlier" opens the rest.
pub const LATEST: usize = 24;
/// More file tracks than this: the top `MAX_TRACKS - 1`, then "+N files".
pub const MAX_TRACKS: usize = 6;

/// A tool row that stands alone and breaks a step: a subagent's lane row, or Main's call that
/// passed a relay on ("Passed on to …").
pub fn stands_alone(conversation: &Conversation, item: &Item) -> bool {
    let Item::Tool { id, .. } = item else {
        return false;
    };
    conversation.lane(id).is_some()
        || conversation
            .passed_on(id)
            .and_then(|l| conversation.lane(l))
            .is_some()
}

/// How a tool row draws: a readable row with its shape in its drawn group, a step's tracks (its first row, holding
/// the step's calls as item indexes), or inside its step's tracks (drawn by the first row).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Role {
    Row(Shape),
    Tracks(Vec<usize>),
    Inside,
}

/// Row `k`'s role (None: not a tool row). `fits`: the chat column is wide enough for tracks.
/// (The tab builds every row's at once: `roles_of`.)
#[cfg(test)]
pub fn role_at(
    items: &[Item],
    rows: &[usize],
    k: usize,
    alone: &dyn Fn(&Item) -> bool,
    fits: bool,
) -> Option<Role> {
    roles_of(items, rows, alone, fits).swap_remove(k)
}

/// Every row's role, in one pass (`agent_tab::sync_rows`).
pub fn roles_of(
    items: &[Item],
    rows: &[usize],
    alone: &dyn Fn(&Item) -> bool,
    fits: bool,
) -> Vec<Option<Role>> {
    let shapes: Vec<Option<Shape>> = (0..rows.len())
        .map(|k| group_shape_at(items, rows, k, alone))
        .collect();
    let mut roles: Vec<Option<Role>> = shapes.iter().map(|s| s.map(Role::Row)).collect();
    if !fits {
        return roles;
    }
    let mut k = 0;
    while k < rows.len() {
        if shapes[k] != Some(Shape::First) {
            k += 1;
            continue;
        }
        let mut end = k;
        while matches!(shapes.get(end + 1), Some(Some(Shape::Middle | Shape::Last))) {
            end += 1;
            if shapes[end] == Some(Shape::Last) {
                break;
            }
        }
        if end + 1 - k >= TRACKS_FROM {
            roles[k] = Some(Role::Tracks(rows[k..=end].to_vec()));
            for role in &mut roles[k + 1..=end] {
                *role = Some(Role::Inside);
            }
        }
        k = end + 1;
    }
    // Drawn, the readable rows (a lane's or "Passed on" row too: the subagent row chore's look)
    // group with their neighbours; a step's tracks are a group of their own.
    let readable = |k: Option<usize>| {
        k.and_then(|k| Some((rows.get(k)?, roles[k].as_ref()?)))
            .is_some_and(|(&i, role)| {
                matches!(items[i], Item::Tool { .. }) && matches!(role, Role::Row(_))
            })
    };
    let drawn: Vec<Option<Shape>> = (0..rows.len())
        .map(|k| {
            readable(Some(k)).then(
                || match (readable(k.checked_sub(1)), readable(Some(k + 1))) {
                    (false, false) => Shape::Solo,
                    (false, true) => Shape::First,
                    (true, true) => Shape::Middle,
                    (true, false) => Shape::Last,
                },
            )
        })
        .collect();
    for (role, shape) in roles.iter_mut().zip(drawn) {
        if let (Some(Role::Row(s)), Some(shape)) = (role.as_mut(), shape) {
            *s = shape;
        }
    }
    roles
}

/// A bead's shape: what the call did to that track's file (or how a check went).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Glyph {
    Read,
    Search,
    Edit,
    Script,
    Passed,
    Failed,
    Planned,
    /// A check whose outcome is unknown.
    Check,
    /// A call with no file that isn't a check (the Commands track).
    Command,
    /// A real check failure a later pass of the same kind and scope cleared: the light ✕.
    Cleared,
    Running,
}

impl Glyph {
    /// Which bead wins when one call touches a file twice.
    fn rank(self) -> u8 {
        match self {
            Glyph::Read | Glyph::Check | Glyph::Command => 0,
            Glyph::Search => 1,
            Glyph::Script => 2,
            Glyph::Edit | Glyph::Passed | Glyph::Planned => 3,
            Glyph::Failed | Glyph::Cleared => 4,
            Glyph::Running => 5,
        }
    }
}

/// A finished check run's glyph: ✓ passed, ○ a planned failure, ✖ a real one, ◌ unknown (the
/// checks track and the popover's strips).
pub fn check_glyph(outcome: &Outcome, planned: bool) -> Glyph {
    match outcome {
        Outcome::Passed { .. } => Glyph::Passed,
        Outcome::Failed { .. } if planned => Glyph::Planned,
        Outcome::Failed { .. } => Glyph::Failed,
        Outcome::Unknown => Glyph::Check,
    }
}

/// One call's mark on a track. `failed`: the small red ✕ (the call itself failed); `dim`: it
/// didn't run (denied, stopped).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Bead {
    pub glyph: Glyph,
    pub failed: bool,
    pub dim: bool,
}

/// One call, in time order: its item, id, hover words, and whether it runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Column {
    pub ix: usize,
    pub id: String,
    pub tip: String,
    pub running: bool,
}

/// A file's track (or the checks or Commands track): its label, full path, beads by column,
/// summary, whether it says a failure (a check still failing for real, a file whose every touch
/// failed, a command that failed) and whether its file changed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Track {
    pub label: String,
    pub path: String,
    pub beads: Vec<(usize, Bead)>,
    pub summary: String,
    pub failing: bool,
    pub changed: bool,
}

/// A step's tracks: built once when its calls change (`agent_tab::sync_rows`), only read when
/// drawn.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tracks {
    /// "8 actions · 5 files".
    pub counts: String,
    /// The calls that themselves failed (the header's "N failed").
    pub failed: usize,
    /// The first start and the last finish (None while one runs); None: a time is unknown.
    pub span: Option<(Instant, Option<Instant>)>,
    /// Every call, in time order.
    pub columns: Vec<Column>,
    /// Columns before the latest `LATEST`.
    pub earlier: usize,
    pub checks: Option<Track>,
    /// Every file track, ordered: changed first (by first change), then the rest.
    pub files: Vec<Track>,
    /// The calls with no file that aren't checks (`git status`, `ls`), drawn last.
    pub commands: Option<Track>,
    /// The step's latest real check failure still open: its first error line.
    pub first_error: Option<String>,
}

impl Tracks {
    /// The columns shown: the latest `LATEST`, or all.
    pub fn shown_columns(&self, all: bool) -> std::ops::Range<usize> {
        if all {
            0..self.columns.len()
        } else {
            self.earlier..self.columns.len()
        }
    }

    /// The file tracks shown, and how many the "+N files" track holds.
    pub fn shown_files(&self, all: bool) -> (&[Track], usize) {
        if all || self.files.len() <= MAX_TRACKS {
            (&self.files, 0)
        } else {
            let shown = MAX_TRACKS - 1;
            (&self.files[..shown], self.files.len() - shown)
        }
    }
}

/// How long a call took (None: running, or its times unknown).
fn took(times: &ToolTimes, id: &str) -> Option<Duration> {
    let (start, end) = times.get(id)?;
    Some(end.as_ref()?.saturating_duration_since(*start))
}

/// A file being gathered into its track.
struct FileAcc {
    path: String,
    beads: Vec<(usize, Bead)>,
    touches: Vec<(Touch, bool, ToolKind)>,
    first_change: Option<usize>,
    first_touch: usize,
    /// Every call touching it failed.
    all_failed: bool,
}

/// Put `bead` in column `col`, keeping the stronger one when the call is there already.
fn add_bead(beads: &mut Vec<(usize, Bead)>, col: usize, bead: Bead) {
    match beads.last_mut() {
        Some((c, b)) if *c == col => {
            if bead.glyph.rank() > b.glyph.rank() {
                *b = bead;
            }
        }
        _ => beads.push((col, bead)),
    }
}

/// A step's tracks from its calls (item indexes, in order).
pub fn tracks_of(
    items: &[Item],
    calls: &[usize],
    open: &HashSet<usize>,
    times: &ToolTimes,
    cwd: &Path,
) -> Tracks {
    let mut columns = Vec::with_capacity(calls.len());
    let mut files: Vec<FileAcc> = Vec::new();
    let mut by_key: HashMap<PathBuf, usize> = HashMap::new();
    let mut checks: Vec<(usize, Bead)> = Vec::new();
    // Each finished check's column, key and whether it passed (for the cleared failures).
    let mut check_runs: Vec<(usize, (CheckWhat, String), bool)> = Vec::new();
    let mut commands: Vec<(usize, Bead)> = Vec::new();
    let mut commands_failed = 0;
    let mut runs: Vec<Cow<'_, Activity>> = Vec::new();
    let mut failed = 0;
    let mut span: Option<(Instant, Option<Instant>)> = None;
    let mut timed = true;
    for (col, &ix) in calls.iter().enumerate() {
        let item = &items[ix];
        let Item::Tool {
            id, running, ok, ..
        } = item
        else {
            continue;
        };
        let a = of(item).unwrap_or_default();
        match times.get(id) {
            Some(&(start, end)) => {
                let (first, last) = span.get_or_insert((start, end));
                *first = (*first).min(start);
                *last = match (*last, end, *running) {
                    (_, _, true) | (None, _, _) | (_, None, _) => None,
                    (Some(a), Some(b), false) => Some(a.max(b)),
                };
            }
            None => timed = false,
        }
        columns.push(Column {
            ix,
            id: id.clone(),
            tip: column_tip(&a, took(times, id)),
            running: *running,
        });
        let dim = a.not_run || (!*running && ok.is_none());
        let call_failed = a.failed.is_some() && !a.not_run;
        failed += call_failed as usize;
        if a.check.is_none() && a.touches.is_empty() {
            commands_failed += call_failed as usize;
            let glyph = match (*running, call_failed) {
                (true, _) => Glyph::Running,
                (_, true) => Glyph::Failed,
                _ => Glyph::Command,
            };
            add_bead(
                &mut commands,
                col,
                Bead {
                    glyph,
                    failed: false,
                    dim,
                },
            );
        }
        if let Some(check) = &a.check {
            let glyph = if *running {
                Glyph::Running
            } else {
                check_glyph(&check.outcome, a.planned_failure)
            };
            if !*running {
                let passed = matches!(check.outcome, Outcome::Passed { .. });
                check_runs.push((col, check_key(check, cwd), passed));
            }
            add_bead(
                &mut checks,
                col,
                Bead {
                    glyph,
                    failed: false,
                    dim,
                },
            );
        }
        for t in &a.touches {
            let key = file_key(&t.path, cwd);
            let at = *by_key.entry(key.clone()).or_insert_with(|| {
                files.push(FileAcc {
                    path: key.to_string_lossy().into_owned(),
                    beads: Vec::new(),
                    touches: Vec::new(),
                    first_change: None,
                    first_touch: col,
                    all_failed: true,
                });
                files.len() - 1
            });
            let f = &mut files[at];
            let glyph = match (&t.how, *running) {
                (_, true) => Glyph::Running,
                (How::Read { .. }, _) => Glyph::Read,
                (How::Searched { .. }, _) => Glyph::Search,
                (How::Changed { .. }, _) => Glyph::Edit,
                (How::ScriptTouched, _) => Glyph::Script,
            };
            add_bead(
                &mut f.beads,
                col,
                Bead {
                    glyph,
                    failed: call_failed,
                    dim,
                },
            );
            f.touches.push((t.clone(), call_failed || dim, a.kind));
            f.all_failed &= call_failed;
            if matches!(t.how, How::Changed { .. }) && !call_failed && !dim && !*running {
                f.first_change.get_or_insert(col);
            }
        }
        // Only finished checks have an outcome to say.
        if a.check.is_some() && !*running {
            runs.push(a);
        }
    }
    files.sort_by_key(|f| match f.first_change {
        Some(c) => (0, c),
        None => (1, f.first_touch),
    });
    let files: Vec<Track> = files
        .into_iter()
        .map(|f| {
            let refs: Vec<(&Touch, bool, ToolKind)> =
                f.touches.iter().map(|(t, x, k)| (t, *x, *k)).collect();
            Track {
                label: file_name(&f.path).to_string(),
                summary: file_summary(&refs),
                failing: f.all_failed,
                changed: f.first_change.is_some(),
                path: f.path,
                beads: f.beads,
            }
        })
        .collect();
    // A real failure a later pass of the same kind and scope cleared: the light ✕ (the history
    // stays, the alarm goes).
    for (col, bead) in &mut checks {
        if bead.glyph != Glyph::Failed {
            continue;
        }
        let Some((_, key, _)) = check_runs.iter().find(|(c, ..)| c == col) else {
            continue;
        };
        if check_runs
            .iter()
            .any(|(c, k, passed)| c > col && k == key && *passed)
        {
            bead.glyph = Glyph::Cleared;
        }
    }
    let runs: Vec<&Activity> = runs.iter().map(|r| r.as_ref()).collect();
    let checks = (!checks.is_empty()).then(|| Track {
        label: "Checks".into(),
        path: String::new(),
        beads: checks,
        summary: if runs.is_empty() {
            "running…".into()
        } else {
            checks_summary(&runs, cwd)
        },
        failing: checks_failing(&runs, cwd),
        changed: false,
    });
    let commands = (!commands.is_empty()).then(|| Track {
        label: "Commands".into(),
        path: String::new(),
        summary: commands_summary(commands.len(), commands_failed),
        failing: commands_failed > 0,
        changed: false,
        beads: commands,
    });
    let first_error = calls
        .iter()
        .rev()
        .filter(|ix| open.contains(ix))
        .find_map(|&ix| match of(&items[ix])?.check.as_ref()?.outcome {
            Outcome::Failed {
                ref first_error, ..
            } => first_error.clone(),
            _ => None,
        });
    Tracks {
        counts: counts(columns.len(), files.len()),
        failed,
        span: span.filter(|_| timed),
        earlier: columns.len().saturating_sub(LATEST),
        columns,
        checks,
        files,
        commands,
        first_error,
    }
}

/// A file's key: its path joined to the folder when relative, with `.` and `..` resolved, so
/// `src/api.rs` and `/…/my-app/src/api.rs` are one file.
pub fn file_key(path: &str, cwd: &Path) -> PathBuf {
    let joined = cwd.join(path);
    let mut out = PathBuf::new();
    for part in joined.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}

/// A check's key (kind, scope), with each `cd <folder>` in its scope read against the tab's
/// folder: a `cd` to the folder itself is left out (`cd /…/my-app && cargo test` is `cargo
/// test`), any other is kept relative to it.
pub fn check_key(check: &Check, cwd: &Path) -> (CheckWhat, String) {
    let mut here = cwd.to_path_buf();
    let mut parts: Vec<String> = Vec::new();
    for part in check.scope.split(" && ") {
        let Some(to) = part.strip_prefix("cd ") else {
            parts.push(part.to_string());
            continue;
        };
        here = file_key(to, &here);
        if here != cwd {
            let shown = here.strip_prefix(cwd).unwrap_or(&here);
            parts.push(format!("cd {}", shown.display()));
        }
    }
    (check.what, parts.join(" && "))
}

/// Where the current turn starts: the index after your latest message (0: none). The tab asks
/// `Conversation::turn_start` (kept, no scan); this is its rule.
#[cfg(test)]
pub fn turn_start(items: &[Item]) -> usize {
    items
        .iter()
        .rposition(|i| matches!(i, Item::You { .. }))
        .map_or(0, |ix| ix + 1)
}

/// The tool items from `from` whose check failed for real and that no later pass of the same
/// kind and scope has cleared. The tab keeps the turn's check runs (`check_marks`) instead.
#[cfg(test)]
pub fn open_failures(items: &[Item], from: usize, cwd: &Path) -> HashSet<usize> {
    open_of(&check_marks(items, from..items.len(), cwd))
}

/// A check run that moves the open failures: its item, its kind and scope, and whether it
/// passed (else it failed for real).
pub type CheckMark = (usize, (CheckWhat, String), bool);

/// The check runs in `range` that move the open failures, in order (the tab keeps them and
/// adds only what changed: the perf chore).
pub fn check_marks(items: &[Item], range: std::ops::Range<usize>, cwd: &Path) -> Vec<CheckMark> {
    range
        .filter_map(|ix| {
            let a = of(&items[ix])?;
            let check = a.check.as_ref()?;
            let passed = match check.outcome {
                Outcome::Passed { .. } => true,
                Outcome::Failed { .. } if a.real_failure() => false,
                _ => return None,
            };
            Some((ix, check_key(check, cwd), passed))
        })
        .collect()
}

/// The open failures `marks` leave: each real failure no later pass of its kind and scope
/// cleared.
pub fn open_of(marks: &[CheckMark]) -> HashSet<usize> {
    let mut open: HashMap<&(CheckWhat, String), Vec<usize>> = HashMap::new();
    for (ix, key, passed) in marks {
        if *passed {
            open.remove(key);
        } else {
            open.entry(key).or_default().push(*ix);
        }
    }
    open.into_values().flatten().collect()
}

/// A step's print: its calls (and their versions), which of them hold an open failure, and
/// whether tracks fit — what makes it built again.
pub fn step_print(
    version: &dyn Fn(usize) -> u64,
    calls: &[usize],
    open: &HashSet<usize>,
    fits: bool,
) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for &ix in calls {
        ix.hash(&mut h);
        version(ix).hash(&mut h);
        open.contains(&ix).hash(&mut h);
    }
    fits.hash(&mut h);
    h.finish()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::agent_activity::{Activity, Check, CheckWhat, How, Outcome, ToolKind, Touch};
    use crate::agent_chat::{Shape, group_shape_at};
    use crate::agent_conversation::Item;
    use std::collections::HashSet;
    use std::path::Path;

    pub(crate) const CWD: &str = "/tmp/fx/code/my-app";

    pub(crate) fn tool(id: &str, activity: Activity) -> Item {
        Item::Tool {
            id: id.into(),
            name: "Bash".into(),
            summary: String::new(),
            parent: None,
            running: false,
            ok: Some(activity.failed.is_none()),
            result: String::new(),
            detail: None,
            output: None,
            dropped_lines: 0,
            activity: Some(Box::new(activity)),
        }
    }

    pub(crate) fn touching(kind: ToolKind, path: &str, how: How) -> Activity {
        let touches = vec![Touch {
            path: path.into(),
            how,
        }];
        Activity {
            derived: crate::agent_activity::derived_words(kind, &touches, None, ""),
            kind,
            touches,
            ..Default::default()
        }
    }

    pub(crate) fn read(id: &str, path: &str) -> Item {
        tool(
            id,
            touching(ToolKind::Read, path, How::Read { lines: None }),
        )
    }

    pub(crate) fn edit(id: &str, path: &str) -> Item {
        let how = How::Changed {
            plus: Some(1),
            minus: Some(0),
            at: None,
        };
        tool(id, touching(ToolKind::Edit, path, how))
    }

    pub(crate) fn check(what: CheckWhat, scope: &str, outcome: Outcome) -> Activity {
        let check = Check {
            what,
            scope: scope.into(),
            outcome,
        };
        Activity {
            kind: ToolKind::Execute,
            derived: crate::agent_activity::derived_words(ToolKind::Execute, &[], Some(&check), ""),
            check: Some(check),
            ..Default::default()
        }
    }

    pub(crate) fn run_check(id: &str, what: CheckWhat, scope: &str, outcome: Outcome) -> Item {
        tool(id, check(what, scope, outcome))
    }

    pub(crate) fn failed(n: u32, of: u32) -> Outcome {
        Outcome::Failed {
            failed: Some(n),
            of: Some(of),
            first_error: None,
        }
    }

    pub(crate) fn passed(n: u32) -> Outcome {
        Outcome::Passed { n: Some(n) }
    }

    pub(crate) fn lane_row(id: &str) -> Item {
        tool(id, Activity::default())
    }

    pub(crate) fn all(items: &[Item]) -> Vec<usize> {
        (0..items.len()).collect()
    }

    pub(crate) fn tracks(items: &[Item]) -> Tracks {
        tracks_of(
            items,
            &all(items),
            &HashSet::new(),
            &ToolTimes::new(),
            Path::new(CWD),
        )
    }

    #[test]
    fn three_plain_calls_are_tracks_two_are_rows() {
        let two = [read("a", "src/a.rs"), read("b", "src/b.rs")];
        let none = |_: &Item| false;
        assert_eq!(
            role_at(&two, &[0, 1], 0, &none, true),
            Some(Role::Row(Shape::First))
        );
        let three = [
            read("a", "src/a.rs"),
            read("b", "src/b.rs"),
            edit("c", "src/c.rs"),
        ];
        assert_eq!(
            role_at(&three, &[0, 1, 2], 0, &none, true),
            Some(Role::Tracks(vec![0, 1, 2]))
        );
        assert_eq!(
            role_at(&three, &[0, 1, 2], 2, &none, true),
            Some(Role::Inside)
        );
        assert_eq!(
            role_at(&three, &[0, 1, 2], 1, &none, false),
            Some(Role::Row(Shape::Middle))
        ); // narrow
    }

    #[test]
    fn lane_and_passed_on_rows_break_a_track_group() {
        let items = [
            read("a", "src/a.rs"),
            read("b", "src/b.rs"),
            lane_row("lane"),
            read("c", "src/c.rs"),
            read("d", "src/d.rs"),
            edit("e", "src/e.rs"),
        ];
        let alone = |i: &Item| matches!(i, Item::Tool { id, .. } if id == "lane");
        let rows = all(&items);
        let roles: Vec<Option<Role>> = (0..6)
            .map(|k| role_at(&items, &rows, k, &alone, true))
            .collect();
        // The lane row breaks the step (tracks start after it) but, drawn, stays in its
        // readable rows' group (the subagent row chore's look); the group ends at the tracks.
        assert_eq!(
            roles,
            [
                Some(Role::Row(Shape::First)),
                Some(Role::Row(Shape::Middle)),
                Some(Role::Row(Shape::Last)),
                Some(Role::Tracks(vec![3, 4, 5])),
                Some(Role::Inside),
                Some(Role::Inside),
            ]
        );
        // Two short steps either side of a lane row: one readable group.
        let short = [
            read("a", "src/a.rs"),
            lane_row("lane"),
            read("c", "src/c.rs"),
        ];
        let roles: Vec<Option<Role>> = roles_of(&short, &all(&short), &alone, true);
        assert_eq!(
            roles,
            [
                Some(Role::Row(Shape::First)),
                Some(Role::Row(Shape::Middle)),
                Some(Role::Row(Shape::Last)),
            ]
        );
        assert_eq!(group_shape_at(&items, &rows, 2, &alone), Some(Shape::Solo));
        assert_eq!(group_shape_at(&items, &rows, 3, &alone), Some(Shape::First));
    }

    #[test]
    fn files_order_changed_first_then_read() {
        let items = [
            read("1", "src/a.rs"),
            read("2", "src/b.rs"),
            edit("3", "src/c.rs"),
            tool(
                "4",
                touching(
                    ToolKind::Search,
                    "src/d.rs",
                    How::Searched { matches: None },
                ),
            ),
            edit("5", "src/a.rs"),
        ];
        let labels: Vec<String> = tracks(&items).files.into_iter().map(|t| t.label).collect();
        assert_eq!(labels, ["c.rs", "a.rs", "b.rs", "d.rs"]);
    }

    #[test]
    fn a_call_touching_two_files_beads_both_in_its_column() {
        let r = crate::agent_activity_command::read_command("mv src/a.rs src/b.rs");
        let mv = Activity {
            kind: r.kind,
            derived: r.derived,
            touches: r.touches,
            ..Default::default()
        };
        let t = tracks(&[tool("1", mv), read("2", "src/c.rs"), read("3", "src/c.rs")]);
        for label in ["a.rs", "b.rs"] {
            let track = t.files.iter().find(|f| f.label == label).unwrap();
            assert!(matches!(
                track.beads[0],
                (
                    0,
                    Bead {
                        glyph: Glyph::Edit,
                        ..
                    }
                )
            ));
        }
    }

    #[test]
    fn more_than_six_files_show_five_and_plus_n() {
        let eight: Vec<Item> = (0..8)
            .map(|n| read(&n.to_string(), &format!("src/f{n}.rs")))
            .collect();
        let t = tracks(&eight);
        assert_eq!(
            (t.shown_files(false).0.len(), t.shown_files(false).1),
            (5, 3)
        );
        assert_eq!((t.shown_files(true).0.len(), t.shown_files(true).1), (8, 0));
        let t = tracks(&eight[..6]);
        assert_eq!(
            (t.shown_files(false).0.len(), t.shown_files(false).1),
            (6, 0)
        );
    }

    #[test]
    fn sixty_calls_show_the_latest_24_and_plus_36_earlier() {
        let items: Vec<Item> = (0..60)
            .map(|n| read(&n.to_string(), ["src/a.rs", "src/b.rs", "src/c.rs"][n % 3]))
            .collect();
        let t = tracks(&items);
        assert_eq!(t.columns.len(), 60);
        assert_eq!(t.earlier, 36);
        assert_eq!(t.shown_columns(false), 36..60);
        assert_eq!(t.shown_columns(true), 0..60);
        assert_eq!(t.counts, "60 actions · 3 files");
    }

    #[test]
    fn a_failed_call_beads_its_kind_with_a_mark_and_counts_in_the_header() {
        let bad = Activity {
            failed: Some("String to replace not found in file.".into()),
            ..touching(
                ToolKind::Edit,
                "src/b.rs",
                How::Changed {
                    plus: Some(1),
                    minus: Some(1),
                    at: None,
                },
            )
        };
        let t = tracks(&[read("1", "src/a.rs"), tool("2", bad), read("3", "src/c.rs")]);
        let b = t.files.iter().find(|f| f.label == "b.rs").unwrap();
        assert_eq!(
            b.beads,
            vec![(
                1,
                Bead {
                    glyph: Glyph::Edit,
                    failed: true,
                    dim: false
                }
            )]
        );
        assert_eq!(b.summary, "edit failed");
        assert!(b.failing, "an edit that failed reads as a failure");
        assert_eq!((t.counts.as_str(), t.failed), ("3 actions · 3 files", 1));
        // A denied call and a stopped one: dim, not counted failed.
        let denied = Activity {
            not_run: true,
            failed: Some("Not now".into()),
            ..touching(
                ToolKind::Edit,
                "src/d.rs",
                How::Changed {
                    plus: None,
                    minus: None,
                    at: None,
                },
            )
        };
        let mut stopped = read("5", "src/e.rs");
        if let Item::Tool { ok, .. } = &mut stopped {
            *ok = None;
        }
        let t = tracks(&[read("1", "src/a.rs"), tool("4", denied), stopped]);
        assert_eq!((t.counts.as_str(), t.failed), ("3 actions · 3 files", 0));
        for label in ["d.rs", "e.rs"] {
            let f = t.files.iter().find(|f| f.label == label).unwrap();
            assert!(f.beads[0].1.dim, "{label}");
        }
    }

    #[test]
    fn calls_with_no_file_that_arent_checks_bead_a_last_commands_track() {
        let status = Activity {
            kind: ToolKind::Execute,
            derived: "Ran git status".into(),
            ..Default::default()
        };
        let broke = Activity {
            failed: Some("fatal: not a git repository".into()),
            ..status.clone()
        };
        let t = tracks(&[read("1", "src/a.rs"), tool("2", status), tool("3", broke)]);
        let commands = t.commands.expect("a commands track");
        assert_eq!(commands.label, "Commands");
        assert_eq!(
            commands
                .beads
                .iter()
                .map(|(c, b)| (*c, b.glyph))
                .collect::<Vec<_>>(),
            vec![(1, Glyph::Command), (2, Glyph::Failed)]
        );
        assert_eq!(commands.summary, "2 commands · 1 failed");
        assert!(commands.failing);
        assert!(
            tracks(&[read("1", "src/a.rs"), read("2", "src/b.rs")])
                .commands
                .is_none()
        );
    }

    #[test]
    fn a_track_knows_if_its_file_changed_and_the_checks_if_one_still_fails() {
        let t = tracks(&[
            read("1", "src/a.rs"),
            edit("2", "src/b.rs"),
            run_check("3", CheckWhat::Tests, "cargo test", failed(1, 3)),
        ]);
        let label = |l: &str| t.files.iter().find(|f| f.label == l).unwrap();
        assert!(!label("a.rs").changed && label("b.rs").changed);
        assert!(t.checks.as_ref().unwrap().failing);
        let fixed = tracks(&[
            run_check("1", CheckWhat::Tests, "cargo test", failed(1, 3)),
            edit("2", "src/b.rs"),
            run_check("3", CheckWhat::Tests, "cargo test", passed(3)),
        ]);
        assert!(!fixed.checks.unwrap().failing);
    }

    #[test]
    fn a_failure_a_later_pass_cleared_draws_as_the_light_cross() {
        let t = tracks(&[
            run_check("1", CheckWhat::Tests, "cargo test", failed(1, 3)),
            run_check("2", CheckWhat::Lint, "cargo clippy", failed(1, 1)),
            edit("3", "src/b.rs"),
            run_check("4", CheckWhat::Tests, "cargo test", passed(3)),
        ]);
        let glyphs: Vec<Glyph> = t
            .checks
            .unwrap()
            .beads
            .iter()
            .map(|(_, b)| b.glyph)
            .collect();
        assert_eq!(
            glyphs,
            [Glyph::Cleared, Glyph::Failed, Glyph::Passed],
            "the tests' failure was cleared, the lint's still stands"
        );
    }

    #[test]
    fn the_running_call_beads_an_arc_on_its_tracks() {
        let mut running = edit("2", "src/api.rs");
        if let Item::Tool { running: r, ok, .. } = &mut running {
            *r = true;
            *ok = None;
        }
        let t = tracks(&[read("1", "src/a.rs"), read("3", "src/b.rs"), running]);
        assert!(t.columns[2].running);
        let api = t.files.iter().find(|f| f.label == "api.rs").unwrap();
        assert_eq!(api.beads[0].1.glyph, Glyph::Running);
        // A check still running: its track says so (not "ran").
        let mut tests = run_check("4", CheckWhat::Tests, "cargo test", Outcome::Unknown);
        if let Item::Tool { running: r, ok, .. } = &mut tests {
            *r = true;
            *ok = None;
        }
        let t = tracks(&[read("1", "src/a.rs"), read("3", "src/b.rs"), tests]);
        assert_eq!(t.checks.unwrap().summary, "running…");
        assert_eq!(t.span, None, "no times known");
    }

    #[test]
    fn a_real_failure_stays_until_the_same_kind_and_scope_passes() {
        let mut items = vec![
            read("0", "src/a.rs"),
            run_check("1", CheckWhat::Tests, "cargo test cli::", failed(1, 3)),
            run_check(
                "2",
                CheckWhat::Lint,
                "cargo clippy",
                Outcome::Passed { n: None },
            ),
            run_check("3", CheckWhat::Tests, "cargo test api::", passed(2)),
        ];
        assert_eq!(open_failures(&items, 0, Path::new(CWD)), HashSet::from([1]));
        items.push(run_check(
            "4",
            CheckWhat::Tests,
            "cargo test cli::",
            passed(3),
        ));
        assert!(open_failures(&items, 0, Path::new(CWD)).is_empty());
    }

    #[test]
    fn a_planned_failure_is_never_open() {
        let planned = Activity {
            planned_failure: true,
            ..check(CheckWhat::Tests, "cargo test cli::", failed(1, 3))
        };
        assert!(open_failures(&[tool("1", planned)], 0, Path::new(CWD)).is_empty());
    }

    #[test]
    fn open_failures_count_only_from_the_turn_start() {
        let items = vec![
            run_check("1", CheckWhat::Tests, "cargo test", failed(1, 3)),
            Item::You {
                text: "go on".into(),
                attachments: Vec::new(),
                id: None,
                taken: true,
                to: None,
            },
            read("2", "src/a.rs"),
        ];
        assert_eq!(turn_start(&items), 2);
        assert_eq!(turn_start(&items[..1]), 0);
        assert!(open_failures(&items, turn_start(&items), Path::new(CWD)).is_empty());
        assert_eq!(open_failures(&items, 0, Path::new(CWD)), HashSet::from([0]));
    }

    #[test]
    fn a_cd_to_the_folder_is_the_same_scope_and_a_cd_elsewhere_is_not() {
        let cwd = Path::new(CWD);
        let key = |scope: &str| {
            check_key(
                &Check {
                    what: CheckWhat::Tests,
                    scope: scope.into(),
                    outcome: Outcome::Unknown,
                },
                cwd,
            )
        };
        assert_eq!(
            key("cd /tmp/fx/code/my-app && cargo test"),
            key("cargo test")
        );
        assert_eq!(
            key("cd /tmp/fx/code/my-app/crates/a && cargo test"),
            key("cd crates/a && cargo test")
        );
        assert_ne!(key("cd crates/a && cargo test"), key("cargo test"));
        assert_ne!(
            key("cd crates/a && cargo test"),
            key("cd crates/b && cargo test")
        );
        // A pass in the folder by its absolute path clears the plain run's failure.
        let items = vec![
            run_check("1", CheckWhat::Tests, "cargo test", failed(1, 3)),
            run_check(
                "2",
                CheckWhat::Tests,
                "cd /tmp/fx/code/my-app && cargo test",
                passed(3),
            ),
        ];
        assert!(open_failures(&items, 0, cwd).is_empty());
    }

    #[test]
    fn file_keys_join_relative_paths_to_the_folder() {
        let cwd = Path::new(CWD);
        assert_eq!(
            file_key("src/api.rs", cwd),
            file_key("/tmp/fx/code/my-app/src/api.rs", cwd)
        );
        assert_eq!(file_key("./src/api.rs", cwd), file_key("src/api.rs", cwd));
    }

    #[test]
    fn the_step_print_changes_when_a_call_its_clearing_or_the_width_changes() {
        let mut items = vec![
            read("1", "src/a.rs"),
            run_check("2", CheckWhat::Tests, "cargo test", failed(1, 3)),
            edit("3", "src/c.rs"),
        ];
        let calls = [0, 1, 2];
        let open = HashSet::from([1]);
        // A call's version: here its content's fingerprint (the tab uses `Conversation::version`).
        let print = |items: &[Item], open: &HashSet<usize>, fits: bool| {
            step_print(
                &|ix| crate::agent_tab::fingerprint(&items[ix]),
                &calls,
                open,
                fits,
            )
        };
        let p = print(&items, &open, true);
        assert_eq!(p, print(&items, &open, true));
        assert_ne!(p, print(&items, &HashSet::new(), true));
        assert_ne!(p, print(&items, &open, false));
        if let Item::Tool { ok, .. } = &mut items[2] {
            *ok = Some(false);
        }
        assert_ne!(p, print(&items, &open, true));
    }
}
