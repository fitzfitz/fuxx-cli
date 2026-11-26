//! The perf chore's tests: work counts on a long conversation (deterministic, not timing), no
//! text hashing or `rows_of` on Main's hot path, and incremental rows equal to a full rebuild's
//! over random edits.

use super::*;
use crate::agent_chat::ROWS_OF_CALLS;
use crate::agent_conversation::Answer;
use crate::agent_long_chat::{conversation, delta};
use crate::agent_tab::FINGERPRINTS;
use crate::providers::{Event, LaneControls, LaneEnd};

const CWD: &str = "/tmp/fx/code/my-app";

fn sync(cache: &mut ViewCache, c: &mut Conversation, view: &View) -> usize {
    let dirty = c.take_dirty();
    cache.sync(c, view, dirty, true, Path::new(CWD))
}

/// A cache built from nothing for `c` as it is now.
fn fresh(c: &Conversation, view: &View, fits: bool) -> ViewCache {
    let mut cache = ViewCache::default();
    cache.sync(c, view, None, fits, Path::new(CWD));
    cache
}

fn same(a: &ViewCache, b: &ViewCache, what: &str) {
    assert_eq!(a.order, b.order, "order after {what}");
    assert_eq!(a.roles, b.roles, "roles after {what}");
    assert_eq!(a.thinking, b.thinking, "thinking after {what}");
    assert_eq!(a.open, b.open, "open failures after {what}");
    assert_eq!(a.prints, b.prints, "prints after {what}");
}

fn tool(id: &str, parent: Option<&str>) -> Event {
    Event::ToolStarted {
        id: id.into(),
        name: "Bash".into(),
        summary: "ls".into(),
        parent: parent.map(String::from),
        detail: None,
        writes: Vec::new(),
        texts: Vec::new(),
    }
}

fn finished(id: &str) -> Event {
    Event::ToolFinished {
        id: id.into(),
        ok: true,
        summary: "ok".into(),
        output: None,
    }
}

fn long() -> Conversation {
    let mut c = conversation(10_000);
    c.you("go on".into(), Vec::new());
    c
}

#[test]
fn streaming_into_the_last_message_derives_a_row_or_two() {
    let mut c = long();
    let mut cache = ViewCache::default();
    sync(&mut cache, &mut c, &View::Main);
    assert!(cache.derived() > 10_000, "the first sync builds every row");
    c.apply(delta("live", "Hello"));
    sync(&mut cache, &mut c, &View::Main);
    assert!(cache.derived() <= 4, "a new message: {}", cache.derived());
    FINGERPRINTS.with(|n| n.set(0));
    ROWS_OF_CALLS.with(|n| n.set(0));
    for i in 0..100 {
        c.apply(delta("live", &format!(" word {i}")));
        sync(&mut cache, &mut c, &View::Main);
        assert!(cache.derived() <= 4, "delta {i}: {} rows", cache.derived());
    }
    assert_eq!(FINGERPRINTS.with(|n| n.get()), 0, "no item's text hashed");
    assert_eq!(
        ROWS_OF_CALLS.with(|n| n.get()),
        0,
        "Main's order extended, not found again"
    );
    same(&cache, &fresh(&c, &View::Main, true), "100 deltas");
}

#[test]
fn appending_an_item_derives_a_few_rows() {
    let mut c = long();
    let mut cache = ViewCache::default();
    sync(&mut cache, &mut c, &View::Main);
    c.apply(Event::MessageDone {
        message: "live".into(),
        text: "Done.".into(),
        parent: None,
    });
    sync(&mut cache, &mut c, &View::Main);
    assert!(cache.derived() <= 4, "a message: {}", cache.derived());
    c.apply(tool("new1", None));
    sync(&mut cache, &mut c, &View::Main);
    assert!(cache.derived() <= 4, "a call: {}", cache.derived());
    c.apply(finished("new1"));
    sync(&mut cache, &mut c, &View::Main);
    assert!(cache.derived() <= 4, "its finish: {}", cache.derived());
    same(&cache, &fresh(&c, &View::Main, true), "appends");
}

#[test]
fn nothing_changed_derives_nothing() {
    let mut c = long();
    let mut cache = ViewCache::default();
    sync(&mut cache, &mut c, &View::Main);
    sync(&mut cache, &mut c, &View::Main);
    assert_eq!(cache.derived(), 0);
}

#[test]
fn a_view_shown_again_or_a_rewind_rebuilds_once() {
    let mut c = long();
    let mut cache = ViewCache::default();
    sync(&mut cache, &mut c, &View::Main);
    let rows = cache.order().len();
    cache.invalidate();
    sync(&mut cache, &mut c, &View::Main);
    assert_eq!(cache.derived(), rows, "shown again: every row once");
    sync(&mut cache, &mut c, &View::Main);
    assert_eq!(cache.derived(), 0, "then nothing");
    let you = c.latest_you().unwrap();
    let cut = c.cut_before(you).expect("a message of yours");
    sync(&mut cache, &mut c, &View::Main);
    assert_eq!(
        cache.derived(),
        cache.order().len(),
        "a rewind: every row once"
    );
    same(&cache, &fresh(&c, &View::Main, true), "the rewind");
    c.apply(delta("after", "x"));
    sync(&mut cache, &mut c, &View::Main);
    assert!(cache.derived() <= 4);
    c.restore(cut);
    sync(&mut cache, &mut c, &View::Main);
    assert_eq!(cache.derived(), cache.order().len(), "Undo: every row once");
    same(&cache, &fresh(&c, &View::Main, true), "the restore");
}

#[test]
fn the_tracks_fit_changing_rebuilds() {
    let mut c = long();
    let mut cache = ViewCache::default();
    sync(&mut cache, &mut c, &View::Main);
    let dirty = c.take_dirty();
    cache.sync(&c, &View::Main, dirty, false, Path::new(CWD));
    assert_eq!(cache.derived(), cache.order().len());
    same(&cache, &fresh(&c, &View::Main, false), "narrow");
}

/// A live turn of `calls` finished calls after a short history, in runs of five after a line
/// of narration, every tenth a check run.
fn long_turn(calls: usize) -> Conversation {
    use crate::agent_activity::CheckWhat;
    use crate::agent_steps::tests::{check, failed, passed};
    let mut c = conversation(300);
    c.you("a long turn".into(), Vec::new());
    for k in 0..calls {
        if k % 5 == 0 {
            c.apply(Event::MessageDone {
                message: format!("n{k}"),
                text: "Next I check the callers.".into(),
                parent: None,
            });
        }
        let id = format!("k{k}");
        c.apply(tool(&id, None));
        c.apply(finished(&id));
        if k % 10 == 0 {
            let outcome = if k % 20 == 0 { failed(1, 3) } else { passed(3) };
            c.apply(Event::Activity {
                id,
                activity: check(CheckWhat::Tests, &format!("cargo test t{}", k % 7), outcome),
            });
        }
    }
    c
}

/// The review's hot paths: in a turn of 3,000 calls, a call's start finds the open failures
/// from the change on (not the whole turn) and the ledger adds only the newest calls.
#[test]
fn a_call_in_a_long_turn_adds_only_the_newest_to_the_failures_and_the_ledger() {
    let cwd = Path::new(CWD);
    let times = crate::agent_steps::ToolTimes::new();
    let mut c = long_turn(3_000);
    let mut cache = ViewCache::default();
    let mut ledger = crate::agent_ledger::LedgerCache::default();
    let dirty = c.take_dirty();
    cache.sync(&c, &View::Main, dirty, true, cwd);
    ledger.sync(&c, &View::Main, dirty, &times, cwd);
    for k in 0..20 {
        if k % 4 == 0 {
            c.apply(Event::MessageDone {
                message: format!("xn{k}"),
                text: "Then the next file.".into(),
                parent: None,
            });
        }
        let id = format!("x{k}");
        c.apply(tool(&id, None));
        c.apply(finished(&id));
        let dirty = c.take_dirty();
        cache.sync(&c, &View::Main, dirty, true, cwd);
        let built = ledger.sync(&c, &View::Main, dirty, &times, cwd);
        assert!(
            ledger.added() <= 2 * 16,
            "call {k}: {} calls added",
            ledger.added()
        );
        assert_eq!(
            built.expect("a call changes it"),
            crate::agent_ledger::ledger_of(&c, &View::Main, &times, cwd)
        );
        // The new rows, and the run of calls before them (it was last).
        assert!(cache.derived() <= 8, "call {k}: {} rows", cache.derived());
    }
    assert_eq!(
        cache.open,
        crate::agent_steps::open_failures(c.items(), c.turn_start(), cwd)
    );
}

/// A fixed-seed generator (no crate for it).
struct Seed(u64);

impl Seed {
    fn next(&mut self, n: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.0 >> 33) as usize) % n
    }
}

/// One random edit of `c`, of every kind the rows depend on. `lanes`: the lanes started, the
/// last the live one.
fn edit(
    seed: &mut Seed,
    c: &mut Conversation,
    n: usize,
    lanes: &mut Vec<String>,
    cuts: &mut Vec<Cut>,
) {
    use crate::agent_activity::CheckWhat;
    use crate::agent_conversation::Reply;
    use crate::agent_steps::tests::{check, failed, passed};
    use crate::providers::{TodoItem, TodoState};
    let live = lanes.last().cloned();
    let any_lane = (!lanes.is_empty()).then(|| lanes[seed.next(lanes.len())].clone());
    let ask = |seed: &mut Seed| match seed.next(3) {
        0 => None,
        1 => live.clone(),
        _ => any_lane.clone(),
    };
    match seed.next(35) {
        0 => c.you(format!("message {n}"), Vec::new()),
        1 => c.you_as(format!("message {n}"), Vec::new(), Some(format!("y{n}"))),
        2 | 3 => c.apply(delta(&format!("m{}", n / 7), "more words ")),
        4 => c.apply(Event::MessageDone {
            message: format!("m{}", n / 7),
            text: "said".into(),
            parent: None,
        }),
        5 | 6 => c.apply(tool(&format!("t{n}"), None)),
        7 | 8 => {
            let running: Vec<usize> = c.running_tools().iter().copied().collect();
            if !running.is_empty() {
                let ix = running[seed.next(running.len())];
                if let Item::Tool { id, .. } = &c.items()[ix] {
                    let id = id.clone();
                    c.apply(finished(&id));
                }
            }
        }
        9 => c.apply(Event::TurnDone {
            ok: seed.next(4) != 0,
            message: Some("the turn failed".into()),
        }),
        10 => {
            // A lane: its call first, or (a third of the time) after it started; nested
            // under the live lane now and then.
            let id = format!("agent{n}");
            let parent = live.clone().filter(|_| seed.next(4) == 0);
            let before = seed.next(3) == 0;
            if !before {
                c.apply(tool(&id, parent.as_deref()));
            }
            c.apply(Event::SubagentStarted {
                id: id.clone(),
                parent: parent.clone(),
                label: "Explore".into(),
                kind: "Explore".into(),
                task: "look".into(),
                model: None,
                controls: LaneControls::default(),
            });
            if before {
                c.apply(delta(&format!("m{n}"), "between"));
                c.apply(tool(&id, parent.as_deref()));
            }
            lanes.push(id);
        }
        11 | 12 => {
            if let Some(l) = &live {
                c.apply(tool(&format!("{l}s{n}"), Some(l)));
            }
        }
        13 => {
            if let Some(l) = &live {
                c.apply(Event::TextDelta {
                    message: format!("{l}m{}", n / 5),
                    text: "lane words ".into(),
                    parent: Some(l.clone()),
                });
            }
        }
        14 => {
            if let Some(l) = &live {
                c.apply(Event::MessageDone {
                    message: format!("{l}m{}", n / 5),
                    text: "lane said".into(),
                    parent: Some(l.clone()),
                });
            }
        }
        15 => {
            if let Some(l) = &any_lane {
                c.apply(Event::SubagentEnded {
                    id: l.clone(),
                    end: LaneEnd::Done,
                });
                if seed.next(2) == 0 {
                    c.apply(Event::SubagentResumed {
                        id: l.clone(),
                        controls: LaneControls::default(),
                        said: None,
                    });
                }
            }
        }
        16 => {
            if let Some(l) = &any_lane {
                let about = seed.next(3) == 0;
                c.relay("pass it on".into(), Some(format!("r{n}")), l.clone(), about);
                if seed.next(2) == 0 {
                    c.apply(tool(&format!("send{n}"), None));
                    c.apply(Event::PassedOn {
                        call: format!("send{n}"),
                        lane: l.clone(),
                    });
                }
            }
        }
        17 => {
            let by = ask(seed);
            c.apply(Event::Permission {
                id: format!("p{n}"),
                tool: "Bash".into(),
                what: "rm".into(),
                detail: String::new(),
                always: None,
                about: None,
                by,
                plan: None,
            });
        }
        18 => {
            // An open permission answered, or withdrawn.
            let open: Vec<usize> = c.open_permissions().iter().copied().collect();
            if !open.is_empty()
                && let Item::Permission { id, .. } = &c.items()[open[seed.next(open.len())]]
            {
                let id = id.clone();
                if seed.next(2) == 0 {
                    c.answered(&id, Answer::Allowed);
                } else {
                    c.apply(Event::AskWithdrawn { id });
                }
            }
        }
        19 => {
            let by = ask(seed);
            c.apply(Event::Question {
                id: format!("q{n}"),
                questions: Vec::new(),
                by,
            });
        }
        20 => {
            // An open question replied, skipped, withdrawn or closed by its tool's result.
            let open = c.open_asks().to_vec();
            if !open.is_empty()
                && let Item::Question { id, .. } = &c.items()[open[seed.next(open.len())]]
            {
                let id = id.clone();
                match seed.next(4) {
                    0 => c.reply(&id, Reply::Answered(Vec::new())),
                    1 => c.reply(&id, Reply::Skipped),
                    2 => c.apply(Event::AskWithdrawn { id }),
                    _ => c.apply(finished(&id)),
                }
            }
        }
        21 => {
            let id = (seed.next(2) == 0).then(|| format!("y{}", n.saturating_sub(seed.next(20))));
            c.apply(Event::Taken { id });
        }
        22 => {
            let id = format!("y{}", n.saturating_sub(seed.next(20)));
            c.cancel_unread(&id);
        }
        23 => {
            let len = c.items().len();
            if len > 40 && seed.next(3) == 0 {
                c.keep_first(len - 1 - seed.next(10));
                lanes.clear();
            }
        }
        24 => {
            if seed.next(8) == 0 {
                c.apply(Event::Cleared);
                lanes.clear();
                cuts.clear();
            }
        }
        25 => {
            if seed.next(2) == 0 {
                c.paused();
            } else {
                c.process_gone();
            }
        }
        26 => c.apply(Event::Todo(vec![TodoItem {
            text: format!("step {n}"),
            state: [TodoState::Done, TodoState::Active, TodoState::Pending][seed.next(3)],
        }])),
        27 => c.replace_notice(
            &format!("note {}", n.saturating_sub(seed.next(30))),
            "replaced",
        ),
        28 => {
            let yours: Vec<usize> = (0..c.items().len())
                .filter(|&ix| matches!(c.items()[ix], Item::You { .. }))
                .collect();
            if yours.len() > 3 {
                let ix = yours[yours.len() - 1 - seed.next(3)];
                if let Some(cut) = c.cut_before(ix) {
                    cuts.push(cut);
                    lanes.retain(|l| c.lane(l).is_some());
                }
            }
        }
        29 => {
            if let Some(cut) = cuts.pop() {
                c.restore(cut);
                *lanes = c.lanes().iter().map(|l| l.id.clone()).collect();
            }
        }
        30 => {
            // A check run that fails or passes: what a step's tracks mark open.
            let id = format!("check{n}");
            c.apply(tool(&id, None));
            c.apply(finished(&id));
            let outcome = if seed.next(2) == 0 {
                failed(1, 3)
            } else {
                passed(3)
            };
            let scope = format!("cargo test {}", seed.next(3));
            c.apply(Event::Activity {
                id,
                activity: check(CheckWhat::Tests, &scope, outcome),
            });
        }
        31 => {
            // A late activity on any call, however old (a check's result arrives late).
            let calls: Vec<usize> = (0..c.items().len())
                .filter(|&ix| matches!(c.items()[ix], Item::Tool { .. }))
                .collect();
            if !calls.is_empty()
                && let Item::Tool { id, .. } = &c.items()[calls[seed.next(calls.len())]]
            {
                let id = id.clone();
                let outcome = if seed.next(2) == 0 {
                    failed(2, 5)
                } else {
                    passed(5)
                };
                c.apply(Event::Activity {
                    id,
                    activity: check(CheckWhat::Tests, "cargo test 1", outcome),
                });
            }
        }
        _ => c.apply(Event::Notice(format!("note {n}"))),
    }
}

use crate::agent_conversation::Cut;

/// Each item's fingerprint and version.
fn marks(c: &Conversation) -> Vec<(u64, u64)> {
    (0..c.items().len())
        .map(|ix| (crate::agent_tab::fingerprint(&c.items()[ix]), c.version(ix)))
        .collect()
}

/// Random edits of every kind; after each, checks that don't share the cache's code: what
/// changed is at or after the noted index and has a new version (fingerprints), each view's
/// order is `rows_of`'s, the thinking line is `shows_thinking`'s, the open failures a full
/// recompute's; and the incremental caches (Main and every lane at once) equal ones built
/// from nothing.
#[test]
fn incremental_rows_equal_a_full_rebuild_over_random_edits() {
    use crate::agent_chat::{shows_thinking, shows_thinking_in};
    use crate::agent_steps::{open_failures, turn_start};
    for run in 0..6u64 {
        let mut seed = Seed(0x5eed + run * 7919);
        let mut c = conversation(120);
        let mut main = ViewCache::default();
        let mut views: std::collections::HashMap<String, ViewCache> = Default::default();
        let mut cuts = Vec::new();
        let mut lanes: Vec<String> = Vec::new();
        let mut before = marks(&c);
        let mut ledgers: std::collections::HashMap<
            View,
            (
                crate::agent_ledger::LedgerCache,
                Option<crate::agent_ledger::Ledger>,
            ),
        > = Default::default();
        let times = crate::agent_steps::ToolTimes::new();
        c.take_dirty();
        for step in 0..1_200 {
            edit(
                &mut seed,
                &mut c,
                run as usize * 10_000 + step,
                &mut lanes,
                &mut cuts,
            );
            let what = format!("seed {run}, step {step}");
            let dirty = c.take_dirty();
            let now = marks(&c);
            let low = dirty.unwrap_or(now.len().min(before.len()));
            assert!(
                dirty.is_some() || now.len() == before.len(),
                "{what}: items came or went unnoted"
            );
            for ix in 0..now.len().min(before.len()) {
                if ix < low {
                    assert_eq!(
                        now[ix], before[ix],
                        "{what}: item {ix} changed under the noted {low}"
                    );
                } else if now[ix].0 != before[ix].0 {
                    assert_ne!(
                        now[ix].1, before[ix].1,
                        "{what}: item {ix} changed, same version"
                    );
                }
            }
            before = now;
            let fits = step % 97 != 0;
            let cwd = Path::new(CWD);
            let items = c.items();
            main.sync(&c, &View::Main, dirty, fits, cwd);
            let ids: Vec<&str> = c.lanes().iter().map(|l| l.id.as_str()).collect();
            assert_eq!(
                main.order,
                rows_of(items, &View::Main, &ids),
                "{what}: Main's order"
            );
            assert_eq!(
                main.thinking,
                shows_thinking(c.status() == crate::status::Status::Working, items),
                "{what}: Main's thinking"
            );
            assert_eq!(
                main.open,
                open_failures(items, turn_start(items), cwd),
                "{what}: open failures"
            );
            same(&main, &fresh(&c, &View::Main, fits), &what);
            // The kept ledger equals one built from nothing, for Main and the live lane.
            let shown: Vec<View> = std::iter::once(View::Main)
                .chain(lanes.last().map(|l| View::Lane(l.clone())))
                .collect();
            // A view not shown this time starts again when shown (the tab keeps one, by view).
            ledgers.retain(|v, _| shown.contains(v));
            for view in shown {
                let (cache, last) = ledgers.entry(view.clone()).or_default();
                if let Some(ledger) = cache.sync(&c, &view, dirty, &times, cwd) {
                    *last = ledger;
                }
                assert_eq!(
                    *last,
                    crate::agent_ledger::ledger_of(&c, &view, &times, cwd),
                    "{what}: {view:?}'s ledger"
                );
            }
            for id in c.lanes().iter().map(|l| l.id.clone()) {
                let view = View::Lane(id.clone());
                let cache = views.entry(id.clone()).or_default();
                cache.sync(&c, &view, dirty, fits, cwd);
                assert_eq!(
                    cache.order,
                    rows_of(items, &view, &ids),
                    "{what}: lane {id}'s order"
                );
                let live = c.lane(&id).is_some_and(|l| l.status.live());
                assert_eq!(
                    cache.thinking,
                    shows_thinking_in(live, items, &cache.order),
                    "{what}: lane {id}'s thinking"
                );
                same(cache, &fresh(&c, &view, fits), &what);
            }
            views.retain(|id, _| c.lane(id).is_some());
            assert_eq!(c.turn_start(), turn_start(items), "{what}");
            let running: Vec<usize> = (0..items.len())
                .filter(|&ix| matches!(items[ix], Item::Tool { running: true, .. }))
                .collect();
            assert_eq!(
                c.running_tools().iter().copied().collect::<Vec<_>>(),
                running,
                "{what}"
            );
            assert_eq!(
                c.pending_permission().cloned(),
                items
                    .iter()
                    .find(|i| matches!(i, Item::Permission { answer: None, .. }))
                    .cloned(),
                "{what}"
            );
        }
    }
}

/// Indicative only: one long-chat sync after a delta, timed on this machine (release:
/// `cargo test --release --bin fuxx sync_bench -- --ignored --nocapture`).
#[test]
#[ignore]
fn sync_bench_on_10k_items() {
    let mut c = long();
    c.apply(delta("live", "Hello"));
    let mut cache = ViewCache::default();
    let t = std::time::Instant::now();
    sync(&mut cache, &mut c, &View::Main);
    let first = t.elapsed();
    let runs = 200;
    let t = std::time::Instant::now();
    for i in 0..runs {
        c.apply(delta("live", &format!(" {i}")));
        sync(&mut cache, &mut c, &View::Main);
    }
    let each = t.elapsed() / runs;
    let t = std::time::Instant::now();
    for _ in 0..runs {
        std::hint::black_box(cache.thinking_now(&c, &View::Main));
    }
    let read = t.elapsed() / runs;
    eprintln!(
        "AFTER items={} full={first:?} delta_sync={each:?} thinking_read={read:?}",
        c.items().len()
    );
}

/// The review's hot paths, timed (indicative; release:
/// `cargo test --release --bin fuxx review_bench -- --ignored --nocapture`).
#[test]
#[ignore]
fn review_bench() {
    let cwd = Path::new(CWD);
    // (a), (b): a live turn of 3,000 calls, some of them check runs.
    let mut c = long_turn(3_000);
    let mut cache = ViewCache::default();
    sync(&mut cache, &mut c, &View::Main);
    let runs = 50u32;
    let t = std::time::Instant::now();
    for k in 0..runs {
        c.apply(tool(&format!("x{k}"), None));
        sync(&mut cache, &mut c, &View::Main);
    }
    let a = t.elapsed() / runs;
    let times = crate::agent_steps::ToolTimes::new();
    let mut ledger = crate::agent_ledger::LedgerCache::default();
    ledger.sync(&c, &View::Main, None, &times, cwd);
    let t = std::time::Instant::now();
    for k in 0..runs {
        c.apply(tool(&format!("y{k}"), None));
        let dirty = c.take_dirty();
        std::hint::black_box(ledger.sync(&c, &View::Main, dirty, &times, cwd));
    }
    let b = t.elapsed() / runs;
    // What the same turn cost before (each call recomputed both over the whole turn).
    let t = std::time::Instant::now();
    for _ in 0..runs {
        std::hint::black_box(crate::agent_steps::open_failures(
            c.items(),
            c.turn_start(),
            cwd,
        ));
    }
    let a0 = t.elapsed() / runs;
    let t = std::time::Instant::now();
    for _ in 0..runs {
        std::hint::black_box(crate::agent_ledger::ledger_of(&c, &View::Main, &times, cwd));
    }
    let b0 = t.elapsed() / runs;
    eprintln!("REVIEW-BEFORE a0(open_failures whole turn)={a0:?} b0(ledger_of whole turn)={b0:?}");
    // (c), (d): a live lane on 10,700 items.
    let mut c = long();
    c.apply(tool("big", None));
    c.apply(Event::SubagentStarted {
        id: "big".into(),
        parent: None,
        label: "Explore".into(),
        kind: "Explore".into(),
        task: "look".into(),
        model: None,
        controls: LaneControls::default(),
    });
    for s in 0..20 {
        c.apply(tool(&format!("bigs{s}"), Some("big")));
    }
    let lane = View::Lane("big".into());
    let mut cache = ViewCache::default();
    sync(&mut cache, &mut c, &lane);
    let t = std::time::Instant::now();
    for k in 0..runs {
        c.apply(Event::TextDelta {
            message: "lane-live".into(),
            text: format!(" {k}"),
            parent: Some("big".into()),
        });
        sync(&mut cache, &mut c, &lane);
    }
    let cc = t.elapsed() / runs;
    let mut ledger = crate::agent_ledger::LedgerCache::default();
    ledger.sync(&c, &lane, None, &times, cwd);
    let t = std::time::Instant::now();
    for k in 0..runs {
        c.apply(tool(&format!("bigx{k}"), Some("big")));
        let dirty = c.take_dirty();
        std::hint::black_box(ledger.sync(&c, &lane, dirty, &times, cwd));
    }
    let d = t.elapsed() / runs;
    eprintln!(
        "REVIEW a(open, per call)={a:?} b(ledger Main 3k)={b:?} c(lane delta)={cc:?} d(ledger lane 10.7k)={d:?}"
    );
}
