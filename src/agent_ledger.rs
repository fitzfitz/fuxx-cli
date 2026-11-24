//! The turn ledger (the agent activity chore): what the current turn is running, which files it
//! changed and how its checks went, for the agent box's folder tab (after the status pill) and
//! its popover. Pure and GPUI-free; rebuilt when events come, never per frame.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::agent_activity::{CheckWhat, How, Outcome, of};
use crate::agent_conversation::{Conversation, Item};
use crate::agent_lanes::{View, clock};
pub use crate::agent_steps::ToolTimes;
use crate::agent_steps::{file_key, stands_alone, turn_start};

/// The call running now (the latest started), its words, when it started (None: unknown) and
/// how many others run with it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Running {
    pub id: String,
    pub title: String,
    pub started: Option<Instant>,
    pub more: usize,
}

/// A file the turn changed, its +/− summed over the counts known.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Changed {
    pub path: PathBuf,
    pub plus: Option<u32>,
    pub minus: Option<u32>,
}

/// A check the turn ran (item `ix`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckRun {
    pub ix: usize,
    pub what: CheckWhat,
    pub scope: String,
    pub outcome: Outcome,
    pub planned: bool,
}

/// The current turn's ledger.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ledger {
    pub running: Option<Running>,
    /// By successful calls, one per file, +/− summed over known counts.
    pub changed: Vec<Changed>,
    /// Touched by scripts only (never counted as changed).
    pub scripted: Vec<PathBuf>,
    /// Read or searched only.
    pub read: Vec<PathBuf>,
    pub checks: Vec<CheckRun>,
    /// Passed runs, and all runs but planned failures and unknown outcomes.
    pub passed: usize,
    pub total: usize,
    /// The latest check of a kind failed for real and no pass of the same kind and scope has
    /// come since.
    pub failing: Option<CheckWhat>,
}

/// Which part of the folder tab's ledger.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PartKind {
    Running,
    Changed,
    Checks,
}

/// One part of the ledger as the folder tab says it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Part {
    pub kind: PartKind,
    pub text: String,
}

/// A character's width at the pill's 12 pt medium, for fitting.
pub const CHAR_W: f32 = 7.0;
/// A pill's icon, padding and gap.
pub const PILL_EXTRA: f32 = 34.0;
/// Between the parts (and after the status word).
pub const SEP: &str = " · ";

/// The ledger of `view`'s current turn: Main's covers every call of the turn (its own and every
/// subagent's), a lane's its own calls since its latest run began. None: no live turn (nothing
/// said yet, or the turn is restored history).
pub fn ledger_of(
    conversation: &Conversation,
    view: &View,
    times: &ToolTimes,
    cwd: &Path,
) -> Option<Ledger> {
    let latest = conversation.latest_you()?;
    if latest < conversation.live_from() {
        return None;
    }
    let items = conversation.items();
    let calls: Vec<usize> = match view {
        View::Main => (turn_start(items)..items.len())
            .filter(|&ix| {
                matches!(items[ix], Item::Tool { .. }) && !stands_alone(conversation, &items[ix])
            })
            .collect(),
        View::Lane(lane) => {
            let from = items
                .iter()
                .rposition(|i| matches!(i, Item::Resumed { lane: l, .. } if l == lane))
                .map_or(0, |ix| ix + 1);
            (from..items.len())
                .filter(|&ix| matches!(&items[ix], Item::Tool { parent: Some(p), .. } if p == lane))
                .collect()
        }
    };
    let mut changed: Vec<Changed> = Vec::new();
    let mut scripted: Vec<PathBuf> = Vec::new();
    let mut read: Vec<PathBuf> = Vec::new();
    let mut checks: Vec<CheckRun> = Vec::new();
    let mut open: HashMap<(CheckWhat, String), usize> = HashMap::new();
    let mut running: Vec<(usize, &str, Option<Instant>)> = Vec::new();
    for &ix in &calls {
        let Item::Tool {
            id, running: on, ..
        } = &items[ix]
        else {
            continue;
        };
        let Some(a) = of(&items[ix]) else { continue };
        if *on {
            running.push((ix, id.as_str(), times.get(id).map(|(start, _)| *start)));
            continue;
        }
        if let Some(check) = &a.check {
            let key = (check.what, check.scope.clone());
            match check.outcome {
                Outcome::Passed { .. } => {
                    open.remove(&key);
                }
                Outcome::Failed { .. } if a.real_failure() => {
                    open.insert(key, ix);
                }
                _ => {}
            }
            checks.push(CheckRun {
                ix,
                what: check.what,
                scope: check.scope.clone(),
                outcome: check.outcome.clone(),
                planned: a.planned_failure,
            });
        }
        if a.failed.is_some() || a.not_run {
            continue;
        }
        for t in &a.touches {
            let key = file_key(&t.path, cwd);
            match t.how {
                How::Changed { plus, minus, .. } => {
                    match changed.iter_mut().find(|c| c.path == key) {
                        Some(c) => {
                            c.plus = add(c.plus, plus);
                            c.minus = add(c.minus, minus);
                        }
                        None => changed.push(Changed {
                            path: key,
                            plus,
                            minus,
                        }),
                    }
                }
                How::ScriptTouched => {
                    if !scripted.contains(&key) {
                        scripted.push(key);
                    }
                }
                How::Read { .. } | How::Searched { .. } => {
                    if !read.contains(&key) {
                        read.push(key);
                    }
                }
            }
        }
    }
    scripted.retain(|p| !changed.iter().any(|c| &c.path == p));
    read.retain(|p| !changed.iter().any(|c| &c.path == p) && !scripted.contains(p));
    let passed = checks
        .iter()
        .filter(|r| matches!(r.outcome, Outcome::Passed { .. }))
        .count();
    let total = checks
        .iter()
        .filter(|r| !r.planned && r.outcome != Outcome::Unknown)
        .count();
    let failing = open
        .iter()
        .max_by_key(|(_, ix)| **ix)
        .map(|((what, _), _)| *what);
    // The latest started runs now (else the last in order), the others "+N running".
    let lead = running
        .iter()
        .enumerate()
        .max_by_key(|(n, (_, _, started))| (*started, *n))
        .map(|(_, r)| *r);
    let running = lead.map(|(ix, id, started)| Running {
        id: id.to_string(),
        title: of(&items[ix])
            .map(|a| a.running_title())
            .unwrap_or_default(),
        started,
        more: running.len() - 1,
    });
    Some(Ledger {
        running,
        changed,
        scripted,
        read,
        checks,
        passed,
        total,
        failing,
    })
}

/// Two counts summed: unknown only when both are.
fn add(a: Option<u32>, b: Option<u32>) -> Option<u32> {
    match (a, b) {
        (None, None) => None,
        _ => Some(a.unwrap_or(0) + b.unwrap_or(0)),
    }
}

/// The folder tab's parts: the running call and its time ("+N running"), the files changed,
/// and the checks ("checks P/T", or "tests failing" while one fails for real).
pub fn parts(ledger: &Ledger, now: Instant) -> Vec<Part> {
    let mut out = Vec::new();
    if let Some(r) = &ledger.running {
        let mut text = r.title.clone();
        if let Some(started) = r.started {
            text.push(' ');
            text.push_str(&clock(now.saturating_duration_since(started)));
        }
        if r.more > 0 {
            text.push_str(&format!("{SEP}+{} running", r.more));
        }
        out.push(Part {
            kind: PartKind::Running,
            text,
        });
    }
    if !ledger.changed.is_empty() {
        out.push(Part {
            kind: PartKind::Changed,
            text: format!("{} changed", ledger.changed.len()),
        });
    }
    let checks = match ledger.failing {
        Some(CheckWhat::Tests) => Some("tests failing".to_string()),
        Some(CheckWhat::Build) => Some("build failing".to_string()),
        Some(CheckWhat::Lint) => Some("lint failing".to_string()),
        None => (ledger.total > 0).then(|| format!("checks {}/{}", ledger.passed, ledger.total)),
    };
    out.extend(checks.map(|text| Part {
        kind: PartKind::Checks,
        text,
    }));
    out
}

/// How many of `parts` fit in `room` (cut from the right): each after a `SEP`; a leading
/// running call always stays.
pub fn fit(parts: &[Part], room: f32) -> usize {
    let mut chars = 0;
    let mut shown = 0;
    for p in parts {
        chars += SEP.chars().count() + p.text.chars().count();
        if chars as f32 * CHAR_W > room {
            break;
        }
        shown += 1;
    }
    let keep = parts.first().is_some_and(|p| p.kind == PartKind::Running) as usize;
    shown.max(keep)
}

/// The room the ledger has in the folder tab: the column less the status, folder and branch
/// pills.
pub fn room(column_w: f32, status_word: &str, folder: &str, branch: Option<&str>) -> f32 {
    let chars = status_word.chars().count()
        + folder.chars().count()
        + branch.map_or(0, |b| b.chars().count());
    let pills = 2 + branch.is_some() as usize;
    column_w - chars as f32 * CHAR_W - pills as f32 * PILL_EXTRA
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_activity::{Activity, Check, CheckWhat, How, Outcome, ToolKind, Touch};
    use crate::agent_conversation::Conversation;
    use crate::agent_lanes::View;
    use crate::providers::{Event, LaneControls, Past};
    use std::path::{Path, PathBuf};
    use std::time::{Duration, Instant};

    const CWD: &str = "/tmp/fx/code/my-app";

    fn cwd() -> &'static Path {
        Path::new(CWD)
    }

    fn started(c: &mut Conversation, id: &str, parent: Option<&str>, a: &Activity) {
        c.apply(Event::ToolStarted {
            id: id.into(),
            name: "Bash".into(),
            summary: String::new(),
            parent: parent.map(String::from),
            detail: None,
            writes: Vec::new(),
            texts: Vec::new(),
        });
        c.apply(Event::Activity {
            id: id.into(),
            activity: a.clone(),
        });
    }

    fn call(c: &mut Conversation, id: &str, parent: Option<&str>, a: Activity) {
        started(c, id, parent, &a);
        c.apply(Event::ToolFinished {
            id: id.into(),
            ok: a.failed.is_none(),
            summary: String::new(),
            output: None,
        });
        c.apply(Event::Activity {
            id: id.into(),
            activity: a,
        });
    }

    fn lane(c: &mut Conversation, id: &str) {
        started(c, id, None, &Activity::default());
        c.apply(Event::SubagentStarted {
            id: id.into(),
            parent: None,
            label: id.into(),
            kind: String::new(),
            task: "look".into(),
            model: None,
            controls: LaneControls::default(),
        });
    }

    fn touching(kind: ToolKind, path: &str, how: How) -> Activity {
        Activity {
            kind,
            touches: vec![Touch {
                path: path.into(),
                how,
            }],
            ..Default::default()
        }
    }

    fn edit(path: &str, plus: Option<u32>, minus: Option<u32>) -> Activity {
        touching(
            ToolKind::Edit,
            path,
            How::Changed {
                plus,
                minus,
                at: None,
            },
        )
    }

    fn check(scope: &str, outcome: Outcome) -> Activity {
        let what = if scope.contains("clippy") {
            CheckWhat::Lint
        } else {
            CheckWhat::Tests
        };
        Activity {
            kind: ToolKind::Execute,
            check: Some(Check {
                what,
                scope: scope.into(),
                outcome,
            }),
            ..Default::default()
        }
    }

    fn failed() -> Outcome {
        Outcome::Failed {
            failed: Some(1),
            of: Some(3),
            first_error: None,
        }
    }

    fn turn() -> Conversation {
        let mut c = Conversation::new();
        c.you("go".into(), vec![]);
        c
    }

    fn main_of(c: &Conversation) -> Ledger {
        ledger_of(c, &View::Main, &ToolTimes::new(), cwd()).expect("a live turn")
    }

    fn paths(changed: &[Changed]) -> Vec<PathBuf> {
        changed.iter().map(|c| c.path.clone()).collect()
    }

    #[test]
    fn main_s_ledger_covers_the_whole_tab_and_a_lane_only_itself() {
        let mut c = turn();
        call(&mut c, "1", None, edit("src/a.rs", Some(1), Some(0)));
        lane(&mut c, "L");
        call(&mut c, "2", Some("L"), edit("src/b.rs", Some(1), Some(0)));
        assert_eq!(
            paths(&main_of(&c).changed),
            [
                Path::new(CWD).join("src/a.rs"),
                Path::new(CWD).join("src/b.rs")
            ]
        );
        let l = ledger_of(&c, &View::Lane("L".into()), &ToolTimes::new(), cwd()).unwrap();
        assert_eq!(paths(&l.changed), [Path::new(CWD).join("src/b.rs")]);
    }

    #[test]
    fn parallel_subagents_editing_one_file_count_it_once() {
        let mut c = turn();
        lane(&mut c, "A");
        lane(&mut c, "B");
        call(
            &mut c,
            "1",
            Some("A"),
            edit("/tmp/fx/code/my-app/src/api.rs", Some(2), Some(1)),
        );
        let r = crate::agent_activity_command::read_command("sed -i '3s/a/b/' src/api.rs");
        let sed = Activity {
            kind: r.kind,
            touches: r.touches,
            ..Default::default()
        };
        call(&mut c, "2", Some("B"), sed);
        let times = ToolTimes::new();
        let main = ledger_of(&c, &View::Main, &times, cwd()).unwrap();
        assert_eq!(main.changed.len(), 1);
        assert_eq!(
            main.changed[0].path,
            PathBuf::from("/tmp/fx/code/my-app/src/api.rs")
        );
        assert_eq!(
            (main.changed[0].plus, main.changed[0].minus),
            (Some(2), Some(1))
        );
        assert_eq!(
            ledger_of(&c, &View::Lane("A".into()), &times, cwd())
                .unwrap()
                .changed
                .len(),
            1
        );
        assert_eq!(
            ledger_of(&c, &View::Lane("B".into()), &times, cwd())
                .unwrap()
                .changed
                .len(),
            1
        );
    }

    #[test]
    fn a_failed_edit_is_not_counted_changed() {
        let mut c = turn();
        let bad = Activity {
            failed: Some("String to replace not found in file.".into()),
            ..edit("src/cli.rs", Some(1), Some(1))
        };
        call(&mut c, "1", None, bad);
        let l = main_of(&c);
        assert!(l.changed.is_empty());
        assert!(l.read.is_empty());
    }

    #[test]
    fn script_touched_files_are_listed_apart_and_never_counted() {
        let mut c = turn();
        let r = crate::agent_activity_command::read_command(
            "python3 - <<'EOF'\nsrc = open('src/api_client.rs').read()\nEOF",
        );
        let script = Activity {
            kind: r.kind,
            touches: r.touches,
            ..Default::default()
        };
        call(&mut c, "1", None, script);
        let l = main_of(&c);
        assert!(l.changed.is_empty());
        assert_eq!(
            l.scripted,
            [PathBuf::from("/tmp/fx/code/my-app/src/api_client.rs")]
        );
    }

    #[test]
    fn checks_count_passed_over_runs_planned_and_unknown_apart() {
        let mut c = turn();
        let planned = Activity {
            planned_failure: true,
            ..check("cargo test api::", failed())
        };
        call(
            &mut c,
            "1",
            None,
            check("cargo test", Outcome::Passed { n: Some(2) }),
        );
        call(&mut c, "2", None, check("cargo test cli::", failed()));
        call(&mut c, "3", None, planned);
        call(&mut c, "4", None, check("cargo test", Outcome::Unknown));
        call(
            &mut c,
            "5",
            None,
            check("cargo test", Outcome::Passed { n: None }),
        );
        let l = main_of(&c);
        assert_eq!((l.passed, l.total, l.checks.len()), (2, 3, 5));
        assert!(l.checks[2].planned);
    }

    #[test]
    fn failing_holds_until_the_same_kind_and_scope_passes() {
        let mut c = turn();
        call(&mut c, "1", None, check("cargo test cli::", failed()));
        call(
            &mut c,
            "2",
            None,
            check("cargo clippy", Outcome::Passed { n: None }),
        );
        assert_eq!(main_of(&c).failing, Some(CheckWhat::Tests));
        call(
            &mut c,
            "3",
            None,
            check("cargo test cli::", Outcome::Passed { n: Some(3) }),
        );
        assert_eq!(main_of(&c).failing, None);
    }

    #[test]
    fn the_latest_running_call_leads_with_plus_n_running() {
        let mut c = turn();
        lane(&mut c, "A");
        let reading = touching(ToolKind::Read, "src/a.rs", How::Read { lines: None });
        started(&mut c, "1", Some("A"), &reading);
        let tests = check("cargo test", Outcome::Unknown);
        started(&mut c, "2", None, &tests);
        let now = Instant::now();
        let mut times = ToolTimes::new();
        times.insert("A".into(), (now - Duration::from_secs(30), None));
        times.insert("1".into(), (now - Duration::from_secs(20), None));
        times.insert("2".into(), (now - Duration::from_secs(5), None));
        let l = ledger_of(&c, &View::Main, &times, cwd()).unwrap();
        let running = l.running.expect("running");
        assert_eq!(running.title, tests.running_title());
        assert_eq!(
            running.more, 1,
            "the lane's row itself is never a running call"
        );
        assert_eq!(running.id, "2");
    }

    #[test]
    fn no_ledger_for_a_restored_turn() {
        let mut c = Conversation::new();
        c.past(vec![Past::You {
            text: "hi".into(),
            id: None,
        }]);
        call(&mut c, "1", None, edit("src/a.rs", Some(1), Some(0)));
        assert!(ledger_of(&c, &View::Main, &ToolTimes::new(), cwd()).is_none());
        c.you("again".into(), vec![]);
        call(&mut c, "2", None, edit("src/b.rs", Some(1), Some(0)));
        assert!(ledger_of(&c, &View::Main, &ToolTimes::new(), cwd()).is_some());
    }

    fn sample(now: Instant) -> Ledger {
        Ledger {
            running: Some(Running {
                id: "t".into(),
                title: "Running tests".into(),
                started: Some(now - Duration::from_secs(14)),
                more: 1,
            }),
            changed: (0..4)
                .map(|n| Changed {
                    path: PathBuf::from(format!("{CWD}/src/f{n}.rs")),
                    plus: None,
                    minus: None,
                })
                .collect(),
            scripted: Vec::new(),
            read: Vec::new(),
            checks: Vec::new(),
            passed: 2,
            total: 6,
            failing: None,
        }
    }

    fn texts(parts: &[Part]) -> Vec<&str> {
        parts.iter().map(|p| p.text.as_str()).collect()
    }

    #[test]
    fn parts_read_running_changed_checks() {
        let now = Instant::now();
        let mut l = sample(now);
        assert_eq!(
            texts(&parts(&l, now)),
            ["Running tests 0:14 · +1 running", "4 changed", "checks 2/6"]
        );
        l.failing = Some(CheckWhat::Tests);
        assert_eq!(parts(&l, now)[2].text, "tests failing");
        l.failing = Some(CheckWhat::Build);
        assert_eq!(parts(&l, now)[2].text, "build failing");
        let empty = Ledger {
            running: None,
            changed: Vec::new(),
            passed: 0,
            total: 0,
            failing: None,
            ..sample(now)
        };
        assert!(parts(&empty, now).is_empty());
    }

    #[test]
    fn it_trims_from_the_right_keeping_the_running_call() {
        let now = Instant::now();
        let p = parts(&sample(now), now);
        // 31, 9 and 10 characters; a fit counts the leading SEP and one SEP between parts:
        // 3 parts = (3 + 31 + 3 + 9 + 3 + 10) * CHAR_W = 413, 2 parts = 322, 1 part = 238.
        assert_eq!(fit(&p, 413.0), 3);
        assert_eq!(fit(&p, 412.0), 2);
        assert_eq!(fit(&p, 322.0), 2);
        assert_eq!(fit(&p, 321.0), 1);
        assert_eq!(fit(&p, 0.0), 1); // the running call is kept
        assert_eq!(fit(&p[1..], 0.0), 0);
    }

    #[test]
    fn the_room_is_the_column_less_the_status_folder_and_branch() {
        assert_eq!(
            room(760.0, "Done", "my-app", Some("main")),
            760.0 - (4.0 + 6.0 + 4.0) * CHAR_W - 3.0 * PILL_EXTRA
        );
    }
}
