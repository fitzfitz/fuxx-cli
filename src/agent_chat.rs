//! The agent chat's layout rules (chores after M11ab, the lead designer's spec B-B): which items
//! are drawn as rows, the gaps between them, how a run of tool rows groups, where the avatar
//! goes, and when the "Thinking…" line shows. Pure; `agent_tab` draws with them.

use std::hash::{Hash, Hasher};

use crate::agent_conversation::Item;
use crate::agent_lanes::View;
use crate::agent_tool_modal::parent_of;
pub use crate::ui_tokens::chat;

/// The items drawn as rows in `view`, in order (M11ar): Main's — everything but a lane's steps
/// (a step whose lane's call is gone stays) — or a lane's own steps. One pass. A relay to a
/// lane (the lanes chore) is in Main's rows and its lane's: there the last row until it
/// settles, then where it settled (it never moves after).
pub fn rows_of(items: &[Item], view: &View, lanes: &[&str]) -> Vec<usize> {
    #[cfg(test)]
    ROWS_OF_CALLS.with(|n| n.set(n.get() + 1));
    let tools: std::collections::HashSet<&str> = items
        .iter()
        .filter_map(|i| match i {
            Item::Tool { id, .. } => Some(id.as_str()),
            _ => None,
        })
        .collect();
    // A step is its lane's only when that lane exists (a Task whose subagent never became a
    // lane keeps its steps in Main).
    let lane_of = |p: &str| tools.contains(p) && lanes.contains(&p);
    let mut rows: Vec<usize> = Vec::new();
    let mut asks: Vec<usize> = Vec::new();
    // A lane's relays: settled ones by where they settled, waiting ones last.
    let mut settled: Vec<(usize, usize)> = Vec::new();
    let mut waiting: Vec<usize> = Vec::new();
    for (ix, item) in items.iter().enumerate() {
        match (view, item) {
            (View::Lane(lane), Item::You { to: Some(r), .. }) => {
                if r.lane == *lane && !r.about {
                    match r.settled_at {
                        Some(at) => settled.push((at, ix)),
                        None => waiting.push(ix),
                    }
                }
            }
            // A question waits in whichever view is shown, at its end (M11ar: Enter answers it
            // there); once answered it stays in Main's and its own lane's.
            (View::Lane(lane), Item::Permission { answer, by, .. }) => {
                if by.as_deref() == Some(lane.as_str()) {
                    rows.push(ix);
                } else if answer.is_none() {
                    asks.push(ix);
                }
            }
            // The agent's own question (batch 3, chore 8): as a permission.
            (View::Lane(lane), Item::Question { reply, by, .. }) => {
                if by.as_deref() == Some(lane.as_str()) {
                    rows.push(ix);
                } else if reply.is_none() {
                    asks.push(ix);
                }
            }
            (View::Main, item) => {
                if in_main(item, &lane_of) {
                    rows.push(ix);
                }
            }
            (View::Lane(lane), item) => {
                if parent_of(item) == Some(lane.as_str()) {
                    rows.push(ix);
                }
            }
        }
    }
    // Each settled relay goes before the first row that came after it settled.
    if !settled.is_empty() {
        let mut keyed: Vec<((usize, u8), usize)> = rows.iter().map(|&r| ((r, 1), r)).collect();
        keyed.extend(settled.into_iter().map(|(at, ix)| ((at, 0), ix)));
        keyed.sort_by_key(|&(key, _)| key);
        rows = keyed.into_iter().map(|(_, ix)| ix).collect();
    }
    rows.extend(waiting);
    rows.extend(asks);
    rows
}

// How many times `rows_of` ran on this thread (the perf chore's work-count tests).
#[cfg(test)]
thread_local! {
    pub static ROWS_OF_CALLS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Whether item `item` is a row of Main's (`rows_of`'s rule, one item at a time): `live` says
/// which tool calls have a lane (and are there).
pub fn in_main(item: &Item, live: &dyn Fn(&str) -> bool) -> bool {
    parent_of(item).is_none_or(|p| !live(p))
}

/// A lane's rows by `rows_of`'s rule, from what the conversation keeps (the perf chore: no
/// scan): `own`, its items in order (its words, steps and resumes, and what it asked);
/// `relays`, your messages relayed to any lane; `asks`, the open asks of every lane, in order.
pub fn lane_rows(
    items: &[Item],
    lane: &str,
    own: impl Iterator<Item = usize>,
    relays: impl Iterator<Item = usize>,
    asks: impl Iterator<Item = usize>,
) -> Vec<usize> {
    let mut settled: Vec<(usize, usize)> = Vec::new();
    let mut waiting: Vec<usize> = Vec::new();
    for ix in relays {
        if let Item::You { to: Some(r), .. } = &items[ix]
            && r.lane == lane
            && !r.about
        {
            match r.settled_at {
                Some(at) => settled.push((at, ix)),
                None => waiting.push(ix),
            }
        }
    }
    // Each settled relay goes before the first row that came after it settled (ties: in item
    // order, as a stable sort leaves them).
    settled.sort_by_key(|&(at, _)| at);
    let mut rows: Vec<usize> = Vec::new();
    let mut next = settled.into_iter().peekable();
    for ix in own {
        while let Some(&(_, relay)) = next.peek().filter(|&&(at, _)| at <= ix) {
            rows.push(relay);
            next.next();
        }
        rows.push(ix);
    }
    rows.extend(next.map(|(_, relay)| relay));
    rows.extend(waiting);
    let by = |ix: usize| match &items[ix] {
        Item::Permission { by, .. } | Item::Question { by, .. } => by.as_deref(),
        _ => None,
    };
    rows.extend(asks.filter(|&ix| by(ix) != Some(lane)));
    rows
}

/// An item said by the agent (its avatar's column), not you or the tab.
pub fn agent_side(item: &Item) -> bool {
    matches!(
        item,
        Item::Agent { .. }
            | Item::Tool { .. }
            | Item::Todo(_)
            | Item::Permission { .. }
            | Item::Question { .. }
    )
}

/// The line under the last row while the turn works and nothing else moves (your word,
/// 2026-11-21: "thinking change to cultivating…").
pub const THINKING_WORD: &str = "Cultivating…";

/// A tool row's place in its run of tool rows (spec B-B): alone, first, between, last.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Shape {
    Solo,
    First,
    Middle,
    Last,
}

/// Row `k` of `rows` (item indexes) in its run of tool rows. A row that stands `alone` (a
/// subagent's lane row, a "Passed on" row: the agent activity chore) is Solo and breaks the run.
pub fn group_shape_at(
    items: &[Item],
    rows: &[usize],
    k: usize,
    alone: &dyn Fn(&Item) -> bool,
) -> Option<Shape> {
    let tool = |k: Option<usize>| {
        k.and_then(|k| rows.get(k))
            .map(|&i| &items[i])
            .filter(|i| matches!(i, Item::Tool { .. }))
    };
    if alone(tool(Some(k))?) {
        return Some(Shape::Solo);
    }
    let is_tool = |k: Option<usize>| tool(k).is_some_and(|i| !alone(i));
    Some(match (is_tool(k.checked_sub(1)), is_tool(Some(k + 1))) {
        (false, false) => Shape::Solo,
        (false, true) => Shape::First,
        (true, true) => Shape::Middle,
        (true, false) => Shape::Last,
    })
}

/// The gap above row `k`, from the row before it (spec B-B's table).
pub fn gap_before_at(items: &[Item], rows: &[usize], k: usize) -> f32 {
    let Some(prev) = k.checked_sub(1).map(|p| &items[rows[p]]) else {
        return chat::FIRST_TOP;
    };
    let item = &items[rows[k]];
    let text = |i: &Item| matches!(i, Item::Agent { .. });
    let tool = |i: &Item| matches!(i, Item::Tool { .. });
    match (prev, item) {
        (_, Item::You { .. }) => chat::GAP_TURN,
        (Item::You { .. }, i) if agent_side(i) => chat::GAP_TURN,
        (_, Item::Notice(_) | Item::Error(_)) => chat::GAP_NOTICE,
        (p, i) if tool(p) && tool(i) => chat::GAP_STACK,
        (p, i) if (text(p) && tool(i)) || (tool(p) && text(i)) => chat::GAP_TEXT_TOOL,
        _ => chat::GAP_ITEM,
    }
}

/// Whether row `k` carries the avatar: the first of a run of the agent's rows.
pub fn shows_avatar_at(items: &[Item], rows: &[usize], k: usize) -> bool {
    agent_side(&items[rows[k]])
        && !k
            .checked_sub(1)
            .is_some_and(|p| agent_side(&items[rows[p]]))
}

/// Row `k`'s print: which item it is and that item's version (`Conversation::version`: never
/// its text, the perf chore), its shape, whether it is the last row and carries the thinking
/// line — what makes it laid out again (`agent_view_cache`).
pub fn row_print_at(
    items: &[Item],
    version: &dyn Fn(usize) -> u64,
    rows: &[usize],
    k: usize,
    thinking: bool,
    alone: &dyn Fn(&Item) -> bool,
) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    rows[k].hash(&mut h);
    version(rows[k]).hash(&mut h);
    group_shape_at(items, rows, k, alone).hash(&mut h);
    let last = k + 1 == rows.len();
    last.hash(&mut h);
    (last && thinking).hash(&mut h);
    h.finish()
}

#[cfg(test)]
/// Whether the "Thinking…" line shows under the last of `rows`: the turn works and nothing
/// else there moves (no text streaming, no tool running, no question waiting).
pub fn shows_thinking_in(working: bool, items: &[Item], rows: &[usize]) -> bool {
    let Some(&last) = rows.last() else {
        return false;
    };
    working
        && !rows
            .iter()
            .any(|&i| matches!(items[i], Item::Tool { running: true, .. }))
        && !matches!(
            items[last],
            Item::Agent { done: false, .. }
                | Item::Permission { answer: None, .. }
                | Item::Question { reply: None, .. }
        )
}

/// Main's rows (the helpers below as before M11ar, for the tests and callers by item index).
fn main_rows(items: &[Item]) -> Vec<usize> {
    rows_of(items, &View::Main, &[])
}

pub fn group_shape(items: &[Item], ix: usize, alone: &dyn Fn(&Item) -> bool) -> Option<Shape> {
    let rows = main_rows(items);
    group_shape_at(items, &rows, rows.iter().position(|&r| r == ix)?, alone)
}

/// The gap above row `ix`, from the row before it (spec B-B's table).
/// The thinking line's opacity at `t` (0..1) of its fade: nothing while short gaps pass
/// (`THINKING_HOLD`), then eased in, and exactly full for its last few percent, so a cached view
/// that keeps a frame from the fade's tail shows what a full redraw does.
pub fn thinking_opacity(t: f32) -> f32 {
    let t = ((t - chat::THINKING_HOLD) / (1. - chat::THINKING_HOLD)).clamp(0., 1.);
    let o = crate::agent_top::ease_out_cubic(t);
    if o >= 0.99 { 1.0 } else { o }
}

#[cfg(test)]
pub fn gap_before(items: &[Item], ix: usize) -> f32 {
    let rows = main_rows(items);
    rows.iter()
        .position(|&r| r == ix)
        .map_or(chat::FIRST_TOP, |k| gap_before_at(items, &rows, k))
}

/// Whether row `ix` carries the avatar: the first of a run of the agent's rows.
#[cfg(test)]
pub fn shows_avatar(items: &[Item], ix: usize) -> bool {
    let rows = main_rows(items);
    rows.iter()
        .position(|&r| r == ix)
        .is_some_and(|k| shows_avatar_at(items, &rows, k))
}

/// How far down the avatar sits, so it centres on the item's first line.
pub fn avatar_top(item: &Item) -> f32 {
    match item {
        Item::Tool { .. } => 6.0,
        Item::Todo(_) => 11.0,
        Item::Permission { answer: None, .. } | Item::Question { reply: None, .. } => 16.0,
        _ => 2.0,
    }
}

/// Whether the "Thinking…" line shows under Main's last row: never since the 2026-11-24 chores
/// (the status strip over the box says it, always in view). The tab reads
/// `ViewCache::thinking_now` (no scan); the rule stays here for its tests.
#[cfg(test)]
pub fn shows_thinking(_working: bool, _items: &[Item]) -> bool {
    false
}

/// Whether a tool row says its summary (not when it only repeats the name, or is empty).
pub fn summary_shown(name: &str, summary: &str) -> bool {
    !summary.is_empty() && summary != name
}

/// Row `ix`'s print in Main.
#[cfg(test)]
pub fn row_print(items: &[Item], ix: usize, thinking: bool) -> u64 {
    let rows = main_rows(items);
    match rows.iter().position(|&r| r == ix) {
        Some(k) => row_print_at(
            items,
            &|ix| crate::agent_tab::fingerprint(&items[ix]),
            &rows,
            k,
            thinking,
            &|_| false,
        ),
        None => 0,
    }
}

/// The most characters the latest-prompt strip keeps of your message (chore after M11ar): it is
/// one line, cut again to its width as it is drawn; this only bounds what is shaped.
pub const STRIP_CHARS: usize = 200;

/// Your message as the latest-prompt strip says it: one line, its whitespace collapsed, cut
/// with "…" past `STRIP_CHARS`. A pasted image's `[Image #N]` stays as text (the icon set has
/// no image glyph). Built once per message, not per frame.
pub fn strip_line(text: &str) -> String {
    let mut line = String::new();
    let mut count = 0;
    for word in text.split_whitespace() {
        for ch in (!line.is_empty())
            .then_some(' ')
            .into_iter()
            .chain(word.chars())
        {
            if count == STRIP_CHARS {
                line.push('\u{2026}');
                return line;
            }
            line.push(ch);
            count += 1;
        }
    }
    line
}

/// Whether the latest-prompt strip shows: your message's row (`row`) is scrolled above the
/// view, judged from the list's scroll top (`top`: the first row in sight and how far into it;
/// `None` before the list is laid out). A message mostly scrolled off counts once what is left
/// of it (its measured `height` less the offset) fits under the strip (`cover`).
pub fn strip_shows(
    row: Option<usize>,
    top: Option<(usize, f32)>,
    height: Option<f32>,
    cover: f32,
) -> bool {
    let (Some(row), Some((ix, offset))) = (row, top) else {
        return false;
    };
    ix > row || (ix == row && height.is_some_and(|h| h - offset <= cover))
}

/// The jumped-to message's highlight: how long it lasts, and how much of that it holds full.
pub const FLASH_MS: u64 = 600;
pub const FLASH_HOLD: f32 = 0.3;

/// The highlight's strength at `t` (0..1 of `FLASH_MS`): full while held, then easing to none.
pub fn flash_strength(t: f32) -> f32 {
    if t <= FLASH_HOLD {
        return 1.0;
    }
    1.0 - crate::agent_top::ease_out_cubic((t - FLASH_HOLD) / (1.0 - FLASH_HOLD))
}

/// The jump's glide: how long it takes, and at most how many screens of it are seen (from
/// further down it starts that far from your message: the rest would only blur by).
pub const GLIDE_MS: u64 = 220;
pub const GLIDE_SCREENS: f32 = 1.5;

/// Where the glide to `to` starts, from `from` (scroll offsets in points), in a view `screen`
/// tall: at most `GLIDE_SCREENS` away, below it (a glide up) or above it (down).
pub fn glide_from(from: f32, to: f32, screen: f32) -> f32 {
    if from <= to {
        from.max(to - screen * GLIDE_SCREENS)
    } else {
        from.min(to + screen * GLIDE_SCREENS)
    }
}

/// The glide's offset at `t` (0..1 of `GLIDE_MS`), eased out.
pub fn glide_at(from: f32, to: f32, t: f32) -> f32 {
    from + (to - from) * crate::agent_top::ease_out_cubic(t)
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_thinking_line_fades_in_late_and_lands_on_full() {
        assert_eq!(thinking_opacity(0.0), 0.0);
        assert_eq!(
            thinking_opacity(chat::THINKING_HOLD * 0.9),
            0.0,
            "held while short gaps pass"
        );
        let mid = thinking_opacity(0.7);
        assert!(mid > 0.0 && mid < 1.0, "easing in: {mid}");
        // The fade's tail is exactly full: a frame kept from it (a cached view) equals the last.
        assert_eq!(thinking_opacity(0.95), 1.0);
        assert_eq!(thinking_opacity(1.0), 1.0);
    }

    use super::*;
    use crate::agent_conversation::{Answer, Item};

    fn you() -> Item {
        Item::You {
            text: "hi".into(),
            attachments: Vec::new(),
            id: None,
            taken: false,
            to: None,
        }
    }

    fn said(done: bool) -> Item {
        Item::Agent {
            message: "m".into(),
            text: "text".into(),
            done,
            parent: None,
        }
    }

    fn tool(id: &str, parent: Option<&str>, running: bool) -> Item {
        Item::Tool {
            id: id.into(),
            name: "Bash".into(),
            summary: "ls".into(),
            parent: parent.map(String::from),
            running,
            ok: (!running).then_some(true),
            result: String::new(),
            detail: None,
            output: None,
            dropped_lines: 0,
            activity: None,
        }
    }

    fn asking(answered: bool) -> Item {
        Item::Permission {
            id: "p".into(),
            tool: "Bash".into(),
            what: "ls".into(),
            detail: String::new(),
            always: None,
            about: None,
            answer: answered.then_some(Answer::Allowed),
            by: None,
            plan: None,
        }
    }

    fn question(by: Option<&str>, open: bool) -> Item {
        Item::Question {
            id: "q".into(),
            questions: Vec::new(),
            reply: (!open).then_some(crate::agent_conversation::Reply::Skipped),
            by: by.map(String::from),
        }
    }

    /// Batch 3, chore 8: an open question waits at the end of whichever view is shown, like a
    /// permission; once answered it stays in Main's and its own lane's. It is the agent's, and
    /// nothing else moves while it waits.
    #[test]
    fn an_open_question_shows_in_every_view_at_the_end() {
        use crate::agent_lanes::View;
        let lane = View::Lane("agent1".into());
        let items = [
            you(),
            tool("agent1", None, true),
            question(None, true),
            tool("r1", Some("agent1"), false),
        ];
        assert_eq!(rows_of(&items, &View::Main, &["agent1"]), [0, 1, 2]);
        assert_eq!(
            rows_of(&items, &lane, &["agent1"]),
            [3, 2],
            "the open one last"
        );
        let answered = [
            items[0].clone(),
            items[1].clone(),
            question(None, false),
            items[3].clone(),
        ];
        assert_eq!(
            rows_of(&answered, &lane, &["agent1"]),
            [3],
            "answered: Main's only"
        );
        let own = [items[1].clone(), question(Some("agent1"), false)];
        assert_eq!(
            rows_of(&own, &lane, &["agent1"]),
            [1],
            "its own lane keeps it"
        );
        assert!(agent_side(&question(None, true)));
        assert_eq!(avatar_top(&question(None, true)), 16.0);
        assert_eq!(avatar_top(&question(None, false)), 2.0);
        assert!(!shows_thinking(true, &[you(), question(None, true)]));
        assert!(!shows_thinking_in(
            true,
            &[you(), question(None, true)],
            &[0, 1]
        ));
        assert!(!shows_thinking(true, &[you(), question(None, false)]));
    }

    #[test]
    fn each_view_has_its_own_rows() {
        use crate::agent_lanes::View;
        let items = [
            you(),
            tool("agent1", None, true),
            tool("r1", Some("agent1"), false),
            said(true),
            tool("r2", Some("agent1"), true),
            tool("orphan", Some("gone"), false),
        ];
        assert_eq!(
            rows_of(&items, &View::Main, &["agent1"]),
            [0, 1, 3, 5],
            "a lane's steps aren't Main's rows; a step whose parent is gone is"
        );
        // Review 8: a Task whose subagent never became a lane keeps its steps in Main.
        assert_eq!(rows_of(&items, &View::Main, &[]), [0, 1, 2, 3, 4, 5]);
        // Review 6: a question from elsewhere sits at the end of the shown lane.
        let mut with_ask = items.to_vec();
        with_ask.insert(3, asking(false));
        assert_eq!(
            rows_of(&with_ask, &View::Lane("agent1".into()), &["agent1"]),
            [2, 5, 3],
            "the open question last"
        );
        assert_eq!(
            rows_of(&items, &View::Lane("agent1".into()), &["agent1"]),
            [2, 4]
        );
        assert!(rows_of(&items, &View::Lane("nobody".into()), &["agent1"]).is_empty());
        // The helpers read the view's rows: in the lane, r2 follows r1 as one group.
        let lane = rows_of(&items, &View::Lane("agent1".into()), &["agent1"]);
        assert_eq!(
            group_shape_at(&items, &lane, 0, &|_| false),
            Some(Shape::First)
        );
        assert_eq!(
            group_shape_at(&items, &lane, 1, &|_| false),
            Some(Shape::Last)
        );
        assert_eq!(gap_before_at(&items, &lane, 0), chat::FIRST_TOP);
        assert!(shows_avatar_at(&items, &lane, 0));
        // A row's print knows which item it draws.
        let main = rows_of(&items, &View::Main, &["agent1"]);
        let v = |ix: usize| ix as u64;
        assert_ne!(
            row_print_at(&items, &v, &main, 1, false, &|_| false),
            row_print_at(&items, &v, &main, 2, false, &|_| false)
        );
    }

    #[test]
    fn a_run_of_tools_is_one_group_and_steps_are_skipped() {
        let items = vec![
            said(true),
            tool("a", None, false),
            tool("b", None, false),
            tool("s", Some("b"), false),
            tool("c", None, false),
            said(true),
            tool("d", None, false),
        ];
        assert_eq!(group_shape(&items, 0, &|_| false), None, "not a tool");
        assert_eq!(group_shape(&items, 1, &|_| false), Some(Shape::First));
        assert_eq!(
            group_shape(&items, 2, &|_| false),
            Some(Shape::Middle),
            "its step doesn't break it"
        );
        assert_eq!(group_shape(&items, 4, &|_| false), Some(Shape::Last));
        assert_eq!(group_shape(&items, 6, &|_| false), Some(Shape::Solo));
    }

    #[test]
    fn gaps_follow_the_item_before() {
        let items = vec![
            you(),
            said(true),
            tool("a", None, false),
            tool("b", None, false),
            said(true),
            said(true),
            Item::Notice("n".into()),
            you(),
            Item::Todo(Vec::new()),
        ];
        assert_eq!(gap_before(&items, 0), chat::FIRST_TOP);
        assert_eq!(gap_before(&items, 1), chat::GAP_TURN, "the reply after you");
        assert_eq!(gap_before(&items, 2), chat::GAP_TEXT_TOOL);
        assert_eq!(gap_before(&items, 3), chat::GAP_STACK, "tool after tool");
        assert_eq!(gap_before(&items, 4), chat::GAP_TEXT_TOOL);
        assert_eq!(gap_before(&items, 5), chat::GAP_ITEM, "text after text");
        assert_eq!(gap_before(&items, 6), chat::GAP_NOTICE);
        assert_eq!(gap_before(&items, 7), chat::GAP_TURN, "you");
        assert_eq!(
            gap_before(&items, 8),
            chat::GAP_TURN,
            "the first reply item after you"
        );
    }

    #[test]
    fn the_avatar_starts_each_run_of_the_agent_s_items() {
        let items = vec![
            said(true),
            tool("a", None, false),
            you(),
            tool("b", None, false),
        ];
        assert!(shows_avatar(&items, 0));
        assert!(!shows_avatar(&items, 1));
        assert!(!shows_avatar(&items, 2), "yours has none");
        assert!(shows_avatar(&items, 3));
        assert_eq!(avatar_top(&said(true)), 2.0);
        assert_eq!(avatar_top(&tool("x", None, false)), 6.0);
        assert_eq!(avatar_top(&Item::Todo(Vec::new())), 11.0);
        assert_eq!(avatar_top(&asking(false)), 16.0);
        assert_eq!(avatar_top(&asking(true)), 2.0);
    }

    /// Main draws no thinking line any more (the 2026-11-24 chores): the status strip over the
    /// box says what it does, whatever moves in the chat.
    #[test]
    fn main_has_no_thinking_line() {
        assert!(!shows_thinking(true, &[you()]));
        assert!(!shows_thinking(true, &[you(), tool("a", None, false)]));
        assert!(!shows_thinking(true, &[you(), said(true)]));
    }

    /// Your word (2026-11-21): "thinking change to cultivating…".
    #[test]
    fn the_waiting_line_says_cultivating() {
        assert_eq!(THINKING_WORD, "Cultivating…");
    }

    #[test]
    fn a_summary_equal_to_the_name_is_not_said_twice() {
        assert!(!summary_shown("Skill", "Skill"));
        assert!(!summary_shown("Skill", ""));
        assert!(summary_shown("Bash", "ls"));
    }

    #[test]
    fn a_row_is_laid_out_again_when_its_neighbours_change_it() {
        let one = vec![said(true), tool("a", None, false)];
        let two = vec![said(true), tool("a", None, false), tool("b", None, false)];
        // The tool's shape went Solo → First, and it is no longer last.
        assert_ne!(row_print(&one, 1, false), row_print(&two, 1, false));
        // The text row before it: unchanged.
        assert_eq!(row_print(&one, 0, false), row_print(&two, 0, false));
        // The thinking line joins the last row.
        assert_ne!(row_print(&one, 1, false), row_print(&one, 1, true));
    }

    // ---- The latest-prompt strip (chore after M11ar) ----

    #[test]
    fn the_strip_line_is_one_line_cut_with_an_ellipsis() {
        assert_eq!(
            strip_line("  fix the\n\n  build   in\tcode/my-app  "),
            "fix the build in code/my-app"
        );
        assert_eq!(strip_line(""), "");
        assert_eq!(strip_line(" \n\t "), "");
        let long = "word ".repeat(200);
        let line = strip_line(&long);
        assert!(line.ends_with('\u{2026}'), "cut: {line:?}");
        assert_eq!(line.chars().count(), STRIP_CHARS + 1);
        assert!(!line.contains("  "));
        // Cut on a character, never inside one.
        let wide = "é".repeat(STRIP_CHARS + 5);
        assert_eq!(strip_line(&wide).chars().count(), STRIP_CHARS + 1);
        // Exactly the limit: nothing cut.
        let exact = "a".repeat(STRIP_CHARS);
        assert_eq!(strip_line(&exact), exact);
    }

    /// fuxx's icon set has no image glyph: a pasted image's marker stays as its text.
    #[test]
    fn the_strip_line_keeps_image_markers_as_text() {
        assert_eq!(
            strip_line("look at [Image #1]\nand [Image #2] please"),
            "look at [Image #1] and [Image #2] please"
        );
    }

    #[test]
    fn the_strip_shows_only_while_your_message_is_above_the_view() {
        const COVER: f32 = 40.0;
        // No message of yours: never.
        assert!(!strip_shows(None, Some((9, 0.0)), None, COVER));
        // Not laid out yet: the scroll top isn't known.
        assert!(!strip_shows(Some(2), None, None, COVER));
        // It is the first row in sight, or under it: on screen.
        assert!(!strip_shows(Some(2), Some((2, 0.0)), Some(80.0), COVER));
        assert!(!strip_shows(Some(2), Some((1, 30.0)), None, COVER));
        // Scrolled past it: shown.
        assert!(strip_shows(Some(2), Some((3, 0.0)), None, COVER));
        // Mostly scrolled off: what is left of it sits under the strip.
        assert!(strip_shows(Some(2), Some((2, 41.0)), Some(80.0), COVER));
        assert!(!strip_shows(Some(2), Some((2, 39.0)), Some(80.0), COVER));
        // Its height not measured: kept hidden rather than guessed.
        assert!(!strip_shows(Some(2), Some((2, 41.0)), None, COVER));
    }

    /// The jumped-to message's highlight: full a moment, then gone, its last frame exactly none
    /// (a frame kept from it equals the message at rest).
    #[test]
    fn the_jump_highlight_holds_then_fades_to_nothing() {
        assert_eq!(flash_strength(0.0), 1.0);
        assert_eq!(flash_strength(FLASH_HOLD * 0.9), 1.0, "held first");
        let mid = flash_strength(0.7);
        assert!(mid > 0.0 && mid < 1.0, "fading: {mid}");
        assert_eq!(flash_strength(1.0), 0.0);
    }

    /// The jump glides the last stretch only: from far down it starts within a little over a
    /// screen of your message; a short way, it glides all of it.
    #[test]
    fn the_jump_glides_at_most_the_last_stretch() {
        assert_eq!(
            glide_from(5_000.0, 200.0, 400.0),
            200.0 + 400.0 * GLIDE_SCREENS
        );
        assert_eq!(glide_from(500.0, 200.0, 400.0), 500.0);
        let at = |t| glide_at(1_000.0, 200.0, t);
        assert_eq!(at(0.0), 1_000.0);
        assert_eq!(at(1.0), 200.0);
        assert!(at(0.5) < 600.0, "eases out: fast first");
    }

    /// Down to the chat's end ("Show in chat", the opens fixed, 2026-11-23): the same last
    /// stretch, from above.
    #[test]
    fn a_glide_down_starts_within_the_last_stretch_too() {
        assert_eq!(
            glide_from(0.0, 5_000.0, 400.0),
            5_000.0 - 400.0 * GLIDE_SCREENS
        );
        assert_eq!(glide_from(4_800.0, 5_000.0, 400.0), 4_800.0);
    }
}
