//! The turn ledger (the agent activity chore): what the current turn is running, which files it
//! changed and how its checks went, for the agent box's folder tab (after the status pill) and
//! its popover. Pure and GPUI-free; rebuilt when events come, never per frame.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::agent_activity::{CheckWhat, How, Outcome, of};
use crate::agent_activity_checks::error_code;
use crate::agent_conversation::{Conversation, Item};
use crate::agent_lanes::View;
pub use crate::agent_steps::ToolTimes;
use crate::agent_steps::{file_key, stands_alone};

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

/// One check scope's runs this turn (each outcome, and whether it was a planned failure), in
/// order; `scope` as its latest run wrote it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scope {
    pub what: CheckWhat,
    pub scope: String,
    pub runs: Vec<(Outcome, bool)>,
}

impl Scope {
    /// Its latest run failed for real.
    pub fn failing(&self) -> bool {
        matches!(self.runs.last(), Some((Outcome::Failed { .. }, false)))
    }

    /// Its latest run passed.
    pub fn passing(&self) -> bool {
        matches!(self.runs.last(), Some((Outcome::Passed { .. }, _)))
    }
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
    /// The checks by scope (`check_key`), in the order of their first run.
    pub scopes: Vec<Scope>,
    /// Scopes whose latest run passed, and all scopes (the popover's "P/T passed").
    pub passed: usize,
    pub total: usize,
    /// The latest check of a kind failed for real and no pass of the same kind and scope has
    /// come since.
    pub failing: Option<CheckWhat>,
    /// A check runs now (one of the running calls is one: chore 22's arc).
    pub checking: bool,
}

impl Ledger {
    /// How many files the turn changed under `root`, the tab's folder (chore 36: an agent's log
    /// in /tmp isn't the project's).
    pub fn changed_in(&self, root: &Path) -> usize {
        self.changed
            .iter()
            .filter(|c| c.path.starts_with(root))
            .count()
    }
}

/// Between the parts (and after the status word).
pub const SEP: &str = " · ";

/// The ledger of `view`'s current turn: Main's covers every call of the turn (its own and every
/// subagent's), a lane's its own calls since its latest run began. None: no live turn (nothing
/// said yet, or the turn is restored history). Built from nothing: `LedgerCache`'s rule, which
/// the tab uses.
#[cfg(test)]
pub fn ledger_of(
    conversation: &Conversation,
    view: &View,
    times: &ToolTimes,
    cwd: &Path,
) -> Option<Ledger> {
    let from = calls_from(conversation, view)?;
    let mut fold = Fold::default();
    for ix in calls(conversation, view, from..conversation.items().len()) {
        fold.add(conversation.items(), ix, times, cwd);
    }
    Some(fold.finish(conversation.items()))
}

/// Where `view`'s calls begin: Main's turn, or the lane's latest run (found through the
/// lane's own items, not a scan); None: no live turn.
fn calls_from(conversation: &Conversation, view: &View) -> Option<usize> {
    let latest = conversation.latest_you()?;
    if latest < conversation.live_from() {
        return None;
    }
    let items = conversation.items();
    Some(match view {
        View::Main => conversation.turn_start(),
        View::Lane(lane) => conversation
            .under(lane)
            .and_then(|under| {
                under
                    .iter()
                    .rev()
                    .find(|&&ix| matches!(&items[ix], Item::Resumed { .. }))
            })
            .map_or(0, |ix| ix + 1),
    })
}

/// `view`'s calls in `range`: Main's every call (not one standing alone), a lane's its own
/// (through the lane's items).
fn calls<'a>(
    conversation: &'a Conversation,
    view: &'a View,
    range: std::ops::Range<usize>,
) -> Box<dyn Iterator<Item = usize> + 'a> {
    let items = conversation.items();
    match view {
        View::Main => Box::new(range.filter(move |&ix| {
            matches!(items[ix], Item::Tool { .. }) && !stands_alone(conversation, &items[ix])
        })),
        View::Lane(lane) => Box::new(
            conversation
                .under(lane)
                .into_iter()
                .flat_map(move |under| under.range(range.clone()).copied())
                .filter(move |&ix| matches!(items[ix], Item::Tool { .. })),
        ),
    }
}

/// The ledger's sums as its calls are added in order (`ledger_of`; `LedgerCache` keeps them up
/// to a point and adds only what came after).
#[derive(Clone, Default)]
struct Fold {
    changed: Vec<Changed>,
    scripted: Vec<PathBuf>,
    read: Vec<PathBuf>,
    checks: Vec<CheckRun>,
    scopes: Vec<((CheckWhat, String), Scope)>,
    open: HashMap<(CheckWhat, String), usize>,
    running: Vec<(usize, String, Option<Instant>)>,
}

impl Fold {
    fn add(&mut self, items: &[Item], ix: usize, times: &ToolTimes, cwd: &Path) {
        let Item::Tool {
            id, running: on, ..
        } = &items[ix]
        else {
            return;
        };
        let Some(a) = of(&items[ix]) else { return };
        if *on {
            self.running
                .push((ix, id.clone(), times.get(id).map(|(start, _)| *start)));
            return;
        }
        if let Some(check) = &a.check {
            let key = crate::agent_steps::check_key(check, cwd);
            let run = (check.outcome.clone(), a.planned_failure);
            match self.scopes.iter_mut().find(|(k, _)| *k == key) {
                Some((_, s)) => {
                    s.scope = check.scope.clone();
                    s.runs.push(run);
                }
                None => self.scopes.push((
                    key.clone(),
                    Scope {
                        what: check.what,
                        scope: check.scope.clone(),
                        runs: vec![run],
                    },
                )),
            }
            match check.outcome {
                Outcome::Passed { .. } => {
                    self.open.remove(&key);
                }
                Outcome::Failed { .. } if a.real_failure() => {
                    self.open.insert(key, ix);
                }
                _ => {}
            }
            self.checks.push(CheckRun {
                ix,
                what: check.what,
                scope: check.scope.clone(),
                outcome: check.outcome.clone(),
                planned: a.planned_failure,
            });
        }
        if a.failed.is_some() || a.not_run {
            return;
        }
        for t in &a.touches {
            let key = file_key(&t.path, cwd);
            match t.how {
                How::Changed { plus, minus, .. } => {
                    match self.changed.iter_mut().find(|c| c.path == key) {
                        Some(c) => {
                            c.plus = add(c.plus, plus);
                            c.minus = add(c.minus, minus);
                        }
                        None => self.changed.push(Changed {
                            path: key,
                            plus,
                            minus,
                        }),
                    }
                }
                How::ScriptTouched => {
                    if !self.scripted.contains(&key) {
                        self.scripted.push(key);
                    }
                }
                How::Read { .. } | How::Searched { .. } => {
                    if !self.read.contains(&key) {
                        self.read.push(key);
                    }
                }
            }
        }
    }

    fn finish(self, items: &[Item]) -> Ledger {
        let Fold {
            changed,
            mut scripted,
            mut read,
            checks,
            scopes,
            open,
            running,
        } = self;
        scripted.retain(|p| !changed.iter().any(|c| &c.path == p));
        read.retain(|p| !changed.iter().any(|c| &c.path == p) && !scripted.contains(p));
        // By scope, as the popover's rows (chore 36: by run, "1/6 passed" sat over four rows).
        let passed = scopes.iter().filter(|(_, s)| s.passing()).count();
        let total = scopes.len();
        // The latest real failure's kind; a compile error is the build's, whatever ran it.
        let failing = open
            .iter()
            .max_by_key(|(_, ix)| **ix)
            .map(|((what, _), &ix)| {
                let compile = of(&items[ix])
                    .and_then(|a| match &a.check.as_ref()?.outcome {
                        Outcome::Failed { first_error, .. } => first_error.clone(),
                        _ => None,
                    })
                    .is_some_and(|e| error_code(&e).is_some());
                if compile { CheckWhat::Build } else { *what }
            });
        // A running call that is a check (its command is one: `cargo test`, a build).
        let checking = running
            .iter()
            .any(|(ix, _, _)| of(&items[*ix]).is_some_and(|a| a.check.is_some()));
        // The latest started runs now (else the last in order), the others "+N running".
        let lead = running
            .iter()
            .enumerate()
            .max_by_key(|(n, (_, _, started))| (*started, *n))
            .map(|(_, r)| r);
        let running = lead.map(|(ix, id, started)| Running {
            id: id.clone(),
            title: of(&items[*ix])
                .map(|a| a.running_title())
                .unwrap_or_default(),
            started: *started,
            more: running.len() - 1,
        });
        Ledger {
            running,
            changed,
            scripted,
            read,
            checks,
            scopes: scopes.into_iter().map(|(_, s)| s).collect(),
            passed,
            total,
            failing,
            checking,
        }
    }
}

/// How far behind the end the kept sums stop: a call's last activity comes just after it
/// finishes, so the newest calls are added again each time rather than kept.
const LAG: usize = 16;

/// What the kept sums are for: the view, the lanes' shape, your latest message, where the
/// restored history ends, and where the view's calls begin.
type LedgerKey = (View, u64, Option<usize>, usize, usize);

/// The shown view's ledger, kept (the perf chore): its sums up to a point (`at`: before the
/// first running call, and `LAG` behind the end), only what came after added again; built
/// from nothing when what it is for changes or something before `at` changed.
#[derive(Default)]
pub struct LedgerCache {
    key: Option<LedgerKey>,
    /// It has run once (`key` None then means no live turn).
    built: bool,
    at: usize,
    fold: Fold,
    /// Calls added by the last `sync` (the work-count tests).
    added: usize,
}

impl LedgerCache {
    /// The ledger after a change (`dirty`: the lowest item changed); None from the outer
    /// Option: nothing it reads changed (the agent's words streaming), the last one stands.
    pub fn sync(
        &mut self,
        conversation: &Conversation,
        view: &View,
        dirty: Option<usize>,
        times: &ToolTimes,
        cwd: &Path,
    ) -> Option<Option<Ledger>> {
        let items = conversation.items();
        let from = calls_from(conversation, view);
        let key = from.map(|from| {
            (
                view.clone(),
                conversation.shape(),
                conversation.latest_you(),
                conversation.live_from(),
                from,
            )
        });
        let same = self.built && self.key == key;
        let words_only = dirty.is_none_or(|d| {
            items[d.min(items.len())..]
                .iter()
                .all(|i| matches!(i, Item::Agent { .. }))
        });
        self.added = 0;
        if same && words_only {
            return None;
        }
        self.built = true;
        let Some(from) = from else {
            self.key = None;
            return Some(None);
        };
        if !same || dirty.is_some_and(|d| d < self.at) {
            self.key = key;
            self.at = from;
            self.fold = Fold::default();
        }
        // The kept sums move on over calls that are done, up to `LAG` behind the end.
        let target = items.len().saturating_sub(LAG).max(self.at);
        let mut stop = target;
        for ix in calls(conversation, view, self.at..target) {
            if matches!(items[ix], Item::Tool { running: true, .. }) {
                stop = ix;
                break;
            }
            self.fold.add(items, ix, times, cwd);
            self.added += 1;
        }
        self.at = stop;
        let mut fold = self.fold.clone();
        for ix in calls(conversation, view, self.at..items.len()) {
            fold.add(items, ix, times, cwd);
            self.added += 1;
        }
        Some(Some(fold.finish(items)))
    }

    /// Calls added by the last `sync`.
    #[cfg(test)]
    pub fn added(&self) -> usize {
        self.added
    }
}

/// Two counts summed: unknown only when both are.
fn add(a: Option<u32>, b: Option<u32>) -> Option<u32> {
    match (a, b) {
        (None, None) => None,
        _ => Some(a.unwrap_or(0) + b.unwrap_or(0)),
    }
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
        // P/T counts the scopes, as the popover's rows (chore 36: it counted every run): of
        // `cargo test`, `cli::` and `api::`, only the first's latest run passed.
        assert_eq!((l.passed, l.total, l.checks.len()), (1, 3, 5));
        assert!(l.checks[2].planned);
        // One group per scope, in the order first run, each with its runs.
        assert_eq!(
            l.scopes
                .iter()
                .map(|s| (s.scope.as_str(), s.runs.len()))
                .collect::<Vec<_>>(),
            [
                ("cargo test", 3),
                ("cargo test cli::", 1),
                ("cargo test api::", 1)
            ]
        );
        assert!(!l.scopes[0].failing() && l.scopes[1].failing() && !l.scopes[2].failing());
    }

    #[test]
    fn a_compile_error_under_cargo_test_is_the_build_failing() {
        let mut c = turn();
        let e0425 = Outcome::Failed {
            failed: None,
            of: None,
            first_error: Some("error[E0425]: cannot find function `call_within`".into()),
        };
        call(&mut c, "1", None, check("cargo test cli::", e0425));
        let l = main_of(&c);
        assert_eq!(l.failing, Some(CheckWhat::Build));
        assert_eq!(
            crate::agent_status_line::chips(&l, Path::new(CWD))
                .last()
                .map(|c| c.text.as_str()),
            Some("build")
        );
    }

    /// Chore 22: a running call that is a check makes the checks chip's arc; a finished one
    /// doesn't.
    #[test]
    fn a_running_check_is_checking() {
        let mut c = turn();
        started(
            &mut c,
            "1",
            None,
            &check("cargo test cli::", Outcome::Unknown),
        );
        assert!(main_of(&c).checking, "running");
        let mut done = turn();
        call(&mut done, "1", None, check("cargo test cli::", failed()));
        assert!(!main_of(&done).checking, "finished");
        let mut edit_only = turn();
        started(
            &mut edit_only,
            "1",
            None,
            &edit("src/a.rs", Some(1), Some(0)),
        );
        assert!(!main_of(&edit_only).checking, "not a check");
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

    /// Chore 36, your picture: four rows (`--no-run` and the build piped, no verdict; a test
    /// that failed then passed; one more) under "1/6 passed": the header counted runs.
    #[test]
    fn the_checks_header_counts_the_rows() {
        let mut c = turn();
        call(
            &mut c,
            "1",
            None,
            check("cargo test --bin fuxx --no-run", Outcome::Unknown),
        );
        let build = || Activity {
            check: Some(Check {
                what: CheckWhat::Build,
                scope: "cargo build --features ui-shots".into(),
                outcome: Outcome::Unknown,
            }),
            ..check("", Outcome::Unknown)
        };
        call(&mut c, "2", None, build());
        call(&mut c, "3", None, build());
        call(
            &mut c,
            "4",
            None,
            check("cargo test --bin fuxx the_task", failed()),
        );
        call(
            &mut c,
            "5",
            None,
            check(
                "cargo test --bin fuxx the_task",
                Outcome::Passed { n: Some(1) },
            ),
        );
        call(
            &mut c,
            "6",
            None,
            check("cargo clippy --all-targets", Outcome::Unknown),
        );
        let l = main_of(&c);
        assert_eq!(l.scopes.len(), 4);
        assert_eq!((l.passed, l.total), (1, 4), "as many as the rows");
    }
}
