//! The step model (the agent activity chore): a narration line and the run of tool calls after
//! it, as the activity view draws it. Pure and GPUI-free.

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};

use crate::agent_activity::{Activity, CheckWhat, How, Outcome, ToolKind, Touch, file_name, of};
use crate::agent_activity_checks::error_code;
use crate::agent_chat::{Shape, group_shape_at};
use crate::agent_conversation::{Conversation, Item};
use crate::agent_lanes::clock;
use crate::providers::ToolDetail;

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

/// How a tool row draws: a readable row in its group, a step's tracks (its first row, holding
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
    Running,
}

impl Glyph {
    /// Which bead wins when one call touches a file twice.
    fn rank(self) -> u8 {
        match self {
            Glyph::Read | Glyph::Check => 0,
            Glyph::Search => 1,
            Glyph::Script => 2,
            Glyph::Edit | Glyph::Passed | Glyph::Planned => 3,
            Glyph::Failed => 4,
            Glyph::Running => 5,
        }
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

/// A file's track (or the checks track): its label, full path, beads by column, and summary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Track {
    pub label: String,
    pub path: String,
    pub beads: Vec<(usize, Bead)>,
    pub summary: String,
}

/// A step's tracks: built once when its calls change (`agent_tab::sync_rows`), only read when
/// drawn.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tracks {
    /// "8 actions · 5 files · 1 failed".
    pub counts: String,
    /// The first start and the last finish (None while one runs); None: a time is unknown.
    pub span: Option<(Instant, Option<Instant>)>,
    /// Every call, in time order.
    pub columns: Vec<Column>,
    /// Columns before the latest `LATEST`.
    pub earlier: usize,
    pub checks: Option<Track>,
    /// Every file track, ordered: changed first (by first change), then the rest.
    pub files: Vec<Track>,
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
        if let Some(check) = &a.check {
            let glyph = match (&check.outcome, *running) {
                (_, true) => Glyph::Running,
                (Outcome::Passed { .. }, _) => Glyph::Passed,
                (Outcome::Failed { .. }, _) if a.planned_failure => Glyph::Planned,
                (Outcome::Failed { .. }, _) => Glyph::Failed,
                (Outcome::Unknown, _) => Glyph::Check,
            };
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
            if matches!(t.how, How::Changed { .. }) && !call_failed && !dim && !*running {
                f.first_change.get_or_insert(col);
            }
        }
        if a.check.is_some() {
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
                path: f.path,
                beads: f.beads,
            }
        })
        .collect();
    let checks = (!checks.is_empty()).then(|| Track {
        label: "Checks".into(),
        path: String::new(),
        beads: checks,
        summary: checks_summary(&runs.iter().map(|r| r.as_ref()).collect::<Vec<_>>()),
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
        counts: counts(columns.len(), files.len(), failed),
        span: span.filter(|_| timed),
        earlier: columns.len().saturating_sub(LATEST),
        columns,
        checks,
        files,
        first_error,
    }
}

/// The header: the counts, then the time when known.
pub fn header(counts: &str, took: Option<Duration>) -> String {
    match took {
        Some(d) => format!("{counts} · {}", clock(d)),
        None => counts.to_string(),
    }
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// "8 actions · 5 files", and " · N failed" when calls failed.
pub fn counts(actions: usize, files: usize, failed: usize) -> String {
    let mut out = format!(
        "{} · {}",
        plural(actions, "action", "actions"),
        plural(files, "file", "files")
    );
    if failed > 0 {
        out.push_str(&format!(" · {failed} failed"));
    }
    out
}

/// A file track's summary from its touches (each with whether that call failed or didn't run,
/// and the call's kind): "edited at :543 · +1 −0", "read 80–400, 78–120 · script 2×",
/// "touched by a script", "searched · 3 matches", "edit failed".
pub fn file_summary(touches: &[(&Touch, bool, ToolKind)]) -> String {
    let good: Vec<&(&Touch, bool, ToolKind)> =
        touches.iter().filter(|(_, failed, _)| !failed).collect();
    if good.is_empty() {
        let Some((t, _, kind)) = touches.first() else {
            return String::new();
        };
        let verb = match (&t.how, kind) {
            (How::Changed { .. }, ToolKind::Delete) => "delete",
            (How::Changed { .. }, ToolKind::Move) => "move",
            (How::Changed { .. }, _) => "edit",
            (How::Read { .. }, _) => "read",
            (How::Searched { .. }, _) => "search",
            (How::ScriptTouched, _) => "script",
        };
        return format!("{verb} failed");
    }
    let mut parts: Vec<String> = Vec::new();
    let changes: Vec<_> = good
        .iter()
        .filter(|(t, _, _)| matches!(t.how, How::Changed { .. }))
        .collect();
    if !changes.is_empty() {
        let verb = match changes[0].2 {
            ToolKind::Delete => "deleted",
            ToolKind::Move => "moved",
            _ => "edited",
        };
        let mut part = verb.to_string();
        let (mut plus, mut minus, mut known, mut at) = (0, 0, false, None);
        for (t, _, _) in &changes {
            if let How::Changed {
                plus: p,
                minus: m,
                at: a,
            } = &t.how
            {
                plus += p.unwrap_or(0);
                minus += m.unwrap_or(0);
                known |= p.is_some() || m.is_some();
                at = at.or(*a);
            }
        }
        if let Some(n) = at {
            part.push_str(&format!(" at :{n}"));
        }
        if known {
            part.push_str(&format!(" · +{plus} −{minus}"));
        }
        parts.push(part);
    }
    let reads: Vec<Option<(u32, u32)>> = good
        .iter()
        .filter_map(|(t, _, _)| match t.how {
            How::Read { lines } => Some(lines),
            _ => None,
        })
        .collect();
    if !reads.is_empty() {
        let mut ranges: Vec<(u32, u32)> = Vec::new();
        for r in reads.iter().flatten() {
            if !ranges.contains(r) {
                ranges.push(*r);
            }
        }
        let shown: Vec<String> = ranges
            .iter()
            .take(2)
            .map(|(a, b)| {
                if a == b {
                    a.to_string()
                } else {
                    format!("{a}–{b}")
                }
            })
            .collect();
        let mut part = "read".to_string();
        if !shown.is_empty() {
            part.push(' ');
            part.push_str(&shown.join(", "));
            if ranges.len() > 2 {
                part.push('…');
            }
        }
        parts.push(part);
    }
    let searches: Vec<Option<u32>> = good
        .iter()
        .filter_map(|(t, _, _)| match t.how {
            How::Searched { matches } => Some(matches),
            _ => None,
        })
        .collect();
    if !searches.is_empty() {
        let mut part = "searched".to_string();
        if searches.iter().any(Option::is_some) {
            let n: u32 = searches.iter().flatten().sum();
            part.push_str(&format!(" · {}", plural(n as usize, "match", "matches")));
        }
        parts.push(part);
    }
    let scripts = good
        .iter()
        .filter(|(t, _, _)| t.how == How::ScriptTouched)
        .count();
    if scripts > 0 {
        parts.push(match (parts.is_empty(), scripts) {
            (false, n) => format!("script {n}×"),
            (true, 1) => "touched by a script".into(),
            (true, n) => format!("touched by a script {n}×"),
        });
    }
    parts.join(" · ")
}

/// A check run or failed call's outcome in words: "2 passed", "build failed · E0425", "red, as
/// planned", "1 of 3 failed", "tests failed", "failed". None: nothing to say (running, unknown).
pub fn outcome_phrase(a: &Activity) -> Option<String> {
    let Some(check) = &a.check else {
        return (a.failed.is_some() && !a.not_run).then(|| "failed".to_string());
    };
    match &check.outcome {
        Outcome::Passed { n: Some(n) } => Some(format!("{n} passed")),
        Outcome::Passed { n: None } => Some("passed".into()),
        Outcome::Failed { .. } if a.planned_failure => Some("red, as planned".into()),
        Outcome::Failed {
            failed,
            of,
            first_error,
        } => {
            if let Some(code) = first_error.as_deref().and_then(error_code) {
                return Some(format!("build failed · {code}"));
            }
            Some(match (failed, of) {
                (Some(f), Some(of)) => format!("{f} of {of} failed"),
                (Some(f), None) => format!("{f} failed"),
                _ => format!("{} failed", what_word(check.what)),
            })
        }
        Outcome::Unknown => None,
    }
}

fn what_word(what: CheckWhat) -> &'static str {
    match what {
        CheckWhat::Tests => "tests",
        CheckWhat::Build => "build",
        CheckWhat::Lint => "lint",
    }
}

/// The checks track's summary: each run's outcome, a planned failure that a later pass of the
/// same check followed folded into "red → green · …", repeats collapsed, the last three kept.
pub fn checks_summary(runs: &[&Activity]) -> String {
    let same = |a: &Activity, b: &Activity| match (&a.check, &b.check) {
        (Some(x), Some(y)) => x.what == y.what && x.scope == y.scope,
        _ => false,
    };
    let passed = |a: &Activity| {
        matches!(
            a.check.as_ref().map(|c| &c.outcome),
            Some(Outcome::Passed { .. })
        )
    };
    let mut used = vec![false; runs.len()];
    let mut phrases: Vec<String> = Vec::new();
    for (i, a) in runs.iter().enumerate() {
        if used[i] {
            continue;
        }
        let green = (a.planned_failure && a.check_failed())
            .then(|| (i + 1..runs.len()).find(|&j| !used[j] && same(a, runs[j]) && passed(runs[j])))
            .flatten();
        if let Some(j) = green {
            used[j] = true;
            let pass = outcome_phrase(runs[j]).unwrap_or_else(|| "passed".into());
            phrases.push(format!("red → green · {pass}"));
            continue;
        }
        if let Some(p) = outcome_phrase(a) {
            phrases.push(p.replace("failed · ", "failed "));
        }
    }
    phrases.dedup();
    if phrases.is_empty() {
        return "ran".into();
    }
    if phrases.len() > 3 {
        let kept = phrases.split_off(phrases.len() - 3);
        return format!("… → {}", kept.join(" → "));
    }
    phrases.join(" → ")
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

/// Where the current turn starts: the index after your latest message (0: none).
pub fn turn_start(items: &[Item]) -> usize {
    items
        .iter()
        .rposition(|i| matches!(i, Item::You { .. }))
        .map_or(0, |ix| ix + 1)
}

/// The tool items from `from` whose check failed for real and that no later pass of the same
/// kind and scope has cleared.
pub fn open_failures(items: &[Item], from: usize) -> HashSet<usize> {
    let mut open: HashMap<(CheckWhat, String), Vec<usize>> = HashMap::new();
    for (ix, item) in items.iter().enumerate().skip(from) {
        let Some(a) = of(item) else { continue };
        let Some(check) = &a.check else { continue };
        let key = (check.what, check.scope.clone());
        match check.outcome {
            Outcome::Passed { .. } => {
                open.remove(&key);
            }
            Outcome::Failed { .. } if a.real_failure() => open.entry(key).or_default().push(ix),
            _ => {}
        }
    }
    open.into_values().flatten().collect()
}

/// A readable row's words: its title, a dim trace of its command, and outcome chips.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RowWords {
    pub title: String,
    pub trace: Option<String>,
    pub chips: Vec<String>,
}

/// A tool row's words (approach A): the agent's own words else fuxx's (the present tense while
/// it runs), the command as a trace, and chips ("+41 −0", "3 matches", "2 passed", "failed").
pub fn row_words(item: &Item) -> RowWords {
    let Item::Tool {
        running,
        detail,
        output,
        ..
    } = item
    else {
        return RowWords {
            title: String::new(),
            trace: None,
            chips: Vec::new(),
        };
    };
    let a = of(item).unwrap_or_default();
    let title = if *running {
        a.running_title()
    } else {
        a.title().to_string()
    };
    let trace = match detail {
        Some(ToolDetail::Command(cmd)) => {
            let line = cmd.split_whitespace().collect::<Vec<_>>().join(" ");
            (a.said.is_some() || title != line).then_some(line)
        }
        _ => None,
    };
    let mut chips = Vec::new();
    if !*running {
        let (mut plus, mut minus, mut known) = (0, 0, false);
        for t in a.changed() {
            if let How::Changed {
                plus: p, minus: m, ..
            } = t.how
            {
                plus += p.unwrap_or(0);
                minus += m.unwrap_or(0);
                known |= p.is_some() || m.is_some();
            }
        }
        if known {
            chips.push(format!("+{plus} −{minus}"));
        }
        let matches: Vec<u32> = a
            .touches
            .iter()
            .filter_map(|t| match t.how {
                How::Searched { matches } => matches,
                _ => None,
            })
            .collect();
        if !matches.is_empty() && a.failed.is_none() {
            chips.push(plural(
                matches.iter().sum::<u32>() as usize,
                "match",
                "matches",
            ));
        }
        // A whole-file read: how many lines it read.
        let whole_read = matches!(
            a.touches.as_slice(),
            [Touch {
                how: How::Read { lines: None },
                ..
            }]
        );
        if let (true, None, Some(out)) = (whole_read, &a.failed, output) {
            chips.push(crate::agent_tab::lines_label(out.lines().count()));
        }
        chips.extend(outcome_phrase(&a));
    }
    RowWords {
        title,
        trace,
        chips,
    }
}

/// A column's hover words: the call's title, its outcome and how long it took.
pub fn column_tip(a: &Activity, took: Option<Duration>) -> String {
    let mut parts = vec![a.title().to_string()];
    parts.extend(outcome_phrase(a));
    parts.extend(took.map(clock));
    parts.join(" · ")
}

/// A step's print: its calls (and their prints), which of them hold an open failure, and
/// whether tracks fit — what makes it built again.
pub fn step_print(items: &[Item], calls: &[usize], open: &HashSet<usize>, fits: bool) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for &ix in calls {
        ix.hash(&mut h);
        crate::agent_tab::fingerprint(&items[ix]).hash(&mut h);
        open.contains(&ix).hash(&mut h);
    }
    fits.hash(&mut h);
    h.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_activity::{Activity, Check, CheckWhat, How, Outcome, ToolKind, Touch};
    use crate::agent_chat::{Shape, group_shape_at};
    use crate::agent_conversation::Item;
    use crate::providers::ToolDetail;
    use std::collections::HashSet;
    use std::path::Path;
    use std::time::Duration;

    const CWD: &str = "/tmp/fx/code/my-app";

    fn tool(id: &str, activity: Activity) -> Item {
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

    fn touching(kind: ToolKind, path: &str, how: How) -> Activity {
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

    fn read(id: &str, path: &str) -> Item {
        tool(
            id,
            touching(ToolKind::Read, path, How::Read { lines: None }),
        )
    }

    fn edit(id: &str, path: &str) -> Item {
        let how = How::Changed {
            plus: Some(1),
            minus: Some(0),
            at: None,
        };
        tool(id, touching(ToolKind::Edit, path, how))
    }

    fn check(what: CheckWhat, scope: &str, outcome: Outcome) -> Activity {
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

    fn run_check(id: &str, what: CheckWhat, scope: &str, outcome: Outcome) -> Item {
        tool(id, check(what, scope, outcome))
    }

    fn failed(n: u32, of: u32) -> Outcome {
        Outcome::Failed {
            failed: Some(n),
            of: Some(of),
            first_error: None,
        }
    }

    fn passed(n: u32) -> Outcome {
        Outcome::Passed { n: Some(n) }
    }

    fn lane_row(id: &str) -> Item {
        tool(id, Activity::default())
    }

    fn all(items: &[Item]) -> Vec<usize> {
        (0..items.len()).collect()
    }

    fn tracks(items: &[Item]) -> Tracks {
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
        assert_eq!(
            roles,
            [
                Some(Role::Row(Shape::First)),
                Some(Role::Row(Shape::Last)),
                Some(Role::Row(Shape::Solo)),
                Some(Role::Tracks(vec![3, 4, 5])),
                Some(Role::Inside),
                Some(Role::Inside),
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
    fn file_summaries() {
        let ch = |at, p, m| Touch {
            path: "src/api.rs".into(),
            how: How::Changed {
                plus: p,
                minus: m,
                at,
            },
        };
        let e = ToolKind::Edit;
        assert_eq!(
            file_summary(&[(&ch(Some(543), Some(1), Some(0)), false, e)]),
            "edited at :543 · +1 −0"
        );
        let r = |a, b| Touch {
            path: "src/api_client.rs".into(),
            how: How::Read {
                lines: Some((a, b)),
            },
        };
        let script = Touch {
            path: "src/api_client.rs".into(),
            how: How::ScriptTouched,
        };
        let x = ToolKind::Execute;
        assert_eq!(
            file_summary(&[
                (&r(80, 400), false, ToolKind::Read),
                (&r(78, 120), false, ToolKind::Read),
                (&script, false, x),
                (&script, false, x),
            ]),
            "read 80–400, 78–120 · script 2×"
        );
        assert_eq!(file_summary(&[(&script, false, x)]), "touched by a script");
        let searched = Touch {
            path: "src/help_commands.rs".into(),
            how: How::Searched { matches: Some(3) },
        };
        assert_eq!(
            file_summary(&[(&searched, false, ToolKind::Search)]),
            "searched · 3 matches"
        );
        assert_eq!(
            file_summary(&[(&ch(None, Some(1), Some(1)), true, e)]),
            "edit failed"
        );
        assert_eq!(
            file_summary(&[(&ch(None, None, None), false, ToolKind::Delete)]),
            "deleted"
        );
    }

    #[test]
    fn checks_summaries() {
        let tests = |o| check(CheckWhat::Tests, "cargo test cli::", o);
        let planned = |o| Activity {
            planned_failure: true,
            ..tests(o)
        };
        let e0425 = Outcome::Failed {
            failed: None,
            of: None,
            first_error: Some("error[E0425]: cannot find function `call_within`".into()),
        };
        let build = check(CheckWhat::Build, "cargo build", e0425);
        assert_eq!(checks_summary(&[&tests(passed(2))]), "2 passed");
        assert_eq!(
            checks_summary(&[&tests(passed(2)), &build]),
            "2 passed → build failed E0425"
        );
        assert_eq!(checks_summary(&[&planned(failed(1, 3))]), "red, as planned");
        assert_eq!(
            checks_summary(&[&planned(failed(1, 3)), &tests(passed(2))]),
            "red → green · 2 passed"
        );
        assert_eq!(checks_summary(&[&tests(failed(1, 3))]), "1 of 3 failed");
        let four = [
            tests(failed(1, 3)),
            tests(passed(1)),
            tests(passed(2)),
            tests(passed(3)),
        ];
        assert_eq!(
            checks_summary(&four.iter().collect::<Vec<_>>()),
            "… → 1 passed → 2 passed → 3 passed"
        );
    }

    #[test]
    fn header_counts_failed_calls_and_time_when_known() {
        assert_eq!(
            header(&counts(8, 5, 0), Some(Duration::from_secs(112))),
            "8 actions · 5 files · 1:52"
        );
        assert_eq!(
            header(&counts(8, 5, 1), None),
            "8 actions · 5 files · 1 failed"
        );
        assert_eq!(counts(1, 1, 0), "1 action · 1 file");
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
        assert_eq!(t.counts, "3 actions · 3 files · 1 failed");
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
        assert_eq!(t.counts, "3 actions · 3 files");
        for label in ["d.rs", "e.rs"] {
            let f = t.files.iter().find(|f| f.label == label).unwrap();
            assert!(f.beads[0].1.dim, "{label}");
        }
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
        assert_eq!(open_failures(&items, 0), HashSet::from([1]));
        items.push(run_check(
            "4",
            CheckWhat::Tests,
            "cargo test cli::",
            passed(3),
        ));
        assert!(open_failures(&items, 0).is_empty());
    }

    #[test]
    fn a_planned_failure_is_never_open() {
        let planned = Activity {
            planned_failure: true,
            ..check(CheckWhat::Tests, "cargo test cli::", failed(1, 3))
        };
        assert!(open_failures(&[tool("1", planned)], 0).is_empty());
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
        assert!(open_failures(&items, turn_start(&items)).is_empty());
        assert_eq!(open_failures(&items, 0), HashSet::from([0]));
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
    fn row_words_lead_with_the_description_and_trace_the_command() {
        let mut tests = tool(
            "1",
            Activity {
                said: Some("Run the CLI tests".into()),
                ..check(CheckWhat::Tests, "cargo test --bin fuxx cli::", passed(2))
            },
        );
        if let Item::Tool { detail, .. } = &mut tests {
            *detail = Some(ToolDetail::Command("cargo test --bin fuxx cli::".into()));
        }
        assert_eq!(
            row_words(&tests),
            RowWords {
                title: "Run the CLI tests".into(),
                trace: Some("cargo test --bin fuxx cli::".into()),
                chips: vec!["2 passed".into()],
            }
        );
        let write = tool(
            "2",
            touching(
                ToolKind::Edit,
                "/tmp/fx/code/my-app/src/link_route.rs",
                How::Changed {
                    plus: Some(41),
                    minus: Some(0),
                    at: None,
                },
            ),
        );
        assert_eq!(
            row_words(&write),
            RowWords {
                title: "Edited link_route.rs".into(),
                trace: None,
                chips: vec!["+41 −0".into()],
            }
        );
        let e0425 = Outcome::Failed {
            failed: None,
            of: None,
            first_error: Some("error[E0425]: cannot find function `call_within`".into()),
        };
        let build = tool("3", check(CheckWhat::Build, "cargo build", e0425));
        assert_eq!(row_words(&build).chips, ["build failed · E0425"]);
        let mut running = tool("4", check(CheckWhat::Tests, "cargo test", Outcome::Unknown));
        if let Item::Tool { running: r, ok, .. } = &mut running {
            *r = true;
            *ok = None;
        }
        assert_eq!(row_words(&running).title, "Running tests");
        assert!(row_words(&running).chips.is_empty());
        // A whole-file read says how many lines it read, as the row did before.
        let mut whole = read("5", "src/api.rs");
        if let Item::Tool { output, .. } = &mut whole {
            *output = Some("a\nb\nc".into());
        }
        assert_eq!(row_words(&whole).chips, ["3 lines"]);
    }

    #[test]
    fn a_column_s_tip_is_its_row_outcome_and_time() {
        let a = check(CheckWhat::Tests, "cargo test cli::", passed(2));
        assert_eq!(
            column_tip(&a, Some(Duration::from_secs(9))),
            "Ran tests cli:: · 2 passed · 0:09"
        );
        assert_eq!(column_tip(&a, None), "Ran tests cli:: · 2 passed");
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
        let p = step_print(&items, &calls, &open, true);
        assert_eq!(p, step_print(&items, &calls, &open, true));
        assert_ne!(p, step_print(&items, &calls, &HashSet::new(), true));
        assert_ne!(p, step_print(&items, &calls, &open, false));
        if let Item::Tool { ok, .. } = &mut items[2] {
            *ok = Some(false);
        }
        assert_ne!(p, step_print(&items, &calls, &open, true));
    }
}
