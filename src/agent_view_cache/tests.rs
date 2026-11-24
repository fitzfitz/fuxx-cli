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

/// Random edits of every kind the rows depend on; after each, the incremental caches (Main and
/// a lane) equal ones built from nothing, and the conversation's kept indexes equal a scan.
#[test]
fn incremental_rows_equal_a_full_rebuild_over_random_edits() {
    let mut seed = Seed(0x5eed);
    let mut c = conversation(300);
    let mut main = ViewCache::default();
    let mut lane = ViewCache::default();
    let mut cuts = Vec::new();
    let mut n = 0;
    let mut live_lane: Option<String> = None;
    let lane_view = |l: &Option<String>| View::Lane(l.clone().unwrap_or_else(|| "lane3".into()));
    for step in 0..1_500 {
        n += 1;
        let what = seed.next(17);
        match what {
            0 => c.you(format!("message {n}"), Vec::new()),
            1 | 2 => c.apply(delta(&format!("m{}", n / 7), "more words ")),
            3 => c.apply(Event::MessageDone {
                message: format!("m{}", n / 7),
                text: "said".into(),
                parent: None,
            }),
            4 => c.apply(tool(&format!("t{n}"), None)),
            5 => {
                let running: Vec<usize> = c.running_tools().iter().copied().collect();
                if !running.is_empty() {
                    let ix = running[seed.next(running.len())];
                    if let Item::Tool { id, .. } = &c.items()[ix] {
                        let id = id.clone();
                        c.apply(finished(&id));
                    }
                }
            }
            6 => c.apply(Event::TurnDone {
                ok: true,
                message: None,
            }),
            7 => {
                let id = format!("agent{n}");
                c.apply(tool(&id, None));
                c.apply(Event::SubagentStarted {
                    id: id.clone(),
                    parent: None,
                    label: "Explore".into(),
                    kind: "Explore".into(),
                    task: "look".into(),
                    model: None,
                    controls: LaneControls::default(),
                });
                live_lane = Some(id);
            }
            8 | 9 => {
                if let Some(l) = &live_lane {
                    c.apply(tool(&format!("{l}s{n}"), Some(l)));
                }
            }
            10 => {
                if let Some(l) = live_lane.take() {
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
                        live_lane = Some(l);
                    }
                }
            }
            11 => {
                if let Some(l) = &live_lane {
                    c.relay("pass it on".into(), None, l.clone(), false);
                    c.apply(tool(&format!("send{n}"), None));
                    c.apply(Event::PassedOn {
                        call: format!("send{n}"),
                        lane: l.clone(),
                    });
                }
            }
            12 => {
                c.apply(Event::Permission {
                    id: format!("p{n}"),
                    tool: "Bash".into(),
                    what: "rm".into(),
                    detail: String::new(),
                    always: None,
                    by: live_lane.clone(),
                });
                if seed.next(2) == 0 {
                    c.answered(&format!("p{n}"), Answer::Allowed);
                }
            }
            13 => {
                let yours: Vec<usize> = (0..c.items().len())
                    .filter(|&ix| matches!(c.items()[ix], Item::You { .. }))
                    .collect();
                if yours.len() > 3 {
                    let ix = yours[yours.len() - 1 - seed.next(3)];
                    if let Some(cut) = c.cut_before(ix) {
                        cuts.push(cut);
                        live_lane = None;
                    }
                }
            }
            14 => {
                if let Some(cut) = cuts.pop() {
                    c.restore(cut);
                }
            }
            15 => {
                // A check run that fails or passes: what a step's tracks mark open.
                use crate::agent_activity::CheckWhat;
                use crate::agent_steps::tests::{check, failed, passed};
                let id = format!("check{n}");
                c.apply(tool(&id, None));
                c.apply(finished(&id));
                let outcome = if seed.next(2) == 0 {
                    failed(1, 3)
                } else {
                    passed(3)
                };
                c.apply(Event::Activity {
                    id,
                    activity: check(CheckWhat::Tests, "cargo test", outcome),
                });
            }
            _ => c.apply(Event::Notice(format!("note {n}"))),
        }
        let dirty = c.take_dirty();
        let fits = step % 97 != 0;
        let lv = lane_view(&live_lane);
        main.sync(&c, &View::Main, dirty, fits, Path::new(CWD));
        lane.sync(&c, &lv, dirty, fits, Path::new(CWD));
        let what = format!("step {step} (edit {what})");
        same(&main, &fresh(&c, &View::Main, fits), &what);
        same(&lane, &fresh(&c, &lv, fits), &what);
        assert_eq!(
            c.turn_start(),
            crate::agent_steps::turn_start(c.items()),
            "{what}"
        );
        let running: Vec<usize> = (0..c.items().len())
            .filter(|&ix| matches!(c.items()[ix], Item::Tool { running: true, .. }))
            .collect();
        assert_eq!(
            c.running_tools().iter().copied().collect::<Vec<_>>(),
            running,
            "{what}"
        );
        assert_eq!(
            c.pending_permission().cloned(),
            c.items()
                .iter()
                .find(|i| matches!(i, Item::Permission { answer: None, .. }))
                .cloned(),
            "{what}"
        );
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
