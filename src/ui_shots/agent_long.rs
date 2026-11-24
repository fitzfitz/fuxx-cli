//! A long chat (the agent tab's perf chore, 2026-11-24): about 3,000 rows of turns, tool runs,
//! lanes and markdown, then a live reply streaming. It draws, scrolls to the top and back, and
//! logs each frame's time (indicative: the machine and the run vary).

use std::time::{Duration, Instant};

use gpui::{AnyWindowHandle, Entity, ListOffset, VisualTestAppContext, px};

use super::agent::{feed, scene_sized};
use super::{Kind, Scene, Themes, frame_fresh};
use crate::agent_long_chat::{delta, history};
use crate::agent_tab::AgentTab;

pub(super) const FILE: &str = file!();

const SOURCES: &[&str] = &[
    "src/agent_tab.rs",
    "src/agent_rows.rs",
    "src/agent_chat.rs",
    "src/agent_view_cache.rs",
    "src/agent_items.rs",
    "src/agent_conversation.rs",
    "src/agent_long_chat.rs",
];

pub(super) const SCENES: &[Scene] = &[Scene {
    name: "agent-long-chat",
    run: agent_long_chat,
    themes: Themes::One,
    serial: false,
    kind: Kind::State,
    sources: SOURCES,
}];

const ROWS: usize = 3_000;

pub(super) fn agent_long_chat() {
    scene_sized("agent-long-chat", (1100.0, 900.0), check);
}

/// One frame drawn, timed.
fn timed(cx: &mut VisualTestAppContext, window: AnyWindowHandle) -> Duration {
    let t = Instant::now();
    let _ = frame_fresh(cx, window);
    t.elapsed()
}

fn check(n: &str, cx: &mut VisualTestAppContext, window: AnyWindowHandle, tab: &Entity<AgentTab>) {
    // An empty tab's frame first: what drawing and reading a frame back costs here at all.
    let empty = timed(cx, window);
    feed(cx, tab, |t, cx| t.shot_history(history(ROWS), cx));
    let rows = cx.read(|cx| tab.read(cx).shot_items().len());
    assert!(rows >= ROWS, "{n}: {rows} items restored");
    let first = timed(cx, window);
    feed(cx, tab, |t, cx| t.send("Keep going".into(), cx));
    // A reply streaming: a frame after each piece.
    let mut streaming = Vec::new();
    for i in 0..10 {
        feed(cx, tab, |t, cx| {
            t.shot_feed(
                vec![delta("live", &format!("piece {i} of the reply. "))],
                cx,
            )
        });
        streaming.push(timed(cx, window));
    }
    let list = cx.read(|cx| tab.read(cx).shot_list());
    let end = list.item_count();
    assert!(end > ROWS / 2, "{n}: {end} rows in the list");
    // To the top and back.
    list.scroll_to(ListOffset {
        item_ix: 0,
        offset_in_item: px(0.),
    });
    let top = timed(cx, window);
    assert_eq!(list.logical_scroll_top().item_ix, 0, "{n}: at the top");
    list.scroll_to(ListOffset {
        item_ix: end - 1,
        offset_in_item: px(0.),
    });
    let back = timed(cx, window);
    assert!(
        list.logical_scroll_top().item_ix > end / 2,
        "{n}: back at the end"
    );
    let worst = streaming.iter().max().copied().unwrap_or_default();
    eprintln!(
        "{n}: {rows} items, {end} rows; empty tab's frame {empty:?}, first frame {first:?}, \
         streaming frames worst {worst:?} (all {streaming:?}), top {top:?}, back {back:?}"
    );
}
