//! The agent chat's layout rules (chores after M11ab, the lead designer's spec B-B): which items
//! are drawn as rows, the gaps between them, how a run of tool rows groups, where the avatar
//! goes, and when the "Thinking…" line shows. Pure; `agent_tab` draws with them.

use std::hash::{Hash, Hasher};

use crate::agent_conversation::Item;
use crate::agent_lanes::View;
use crate::agent_tool_modal::parent_of;
pub use crate::ui_tokens::chat;

/// The items drawn as rows in `view`, in order (M11ar): Main's — everything but a lane's steps
/// (a step whose lane's call is gone stays) — or a lane's own steps. One pass.
pub fn rows_of(items: &[Item], view: &View) -> Vec<usize> {
    let tools: std::collections::HashSet<&str> = items
        .iter()
        .filter_map(|i| match i {
            Item::Tool { id, .. } => Some(id.as_str()),
            _ => None,
        })
        .collect();
    (0..items.len())
        .filter(|&ix| match (view, &items[ix]) {
            // A question waits in whichever view is shown (M11ar): Enter answers it there; once
            // answered it stays in Main's and its own lane's.
            (View::Lane(lane), Item::Permission { answer, by, .. }) => {
                answer.is_none() || by.as_deref() == Some(lane.as_str())
            }
            (View::Main, item) => parent_of(item).is_none_or(|p| !tools.contains(p)),
            (View::Lane(lane), item) => parent_of(item) == Some(lane.as_str()),
        })
        .collect()
}

/// An item said by the agent (its avatar's column), not you or the tab.
pub fn agent_side(item: &Item) -> bool {
    matches!(
        item,
        Item::Agent { .. } | Item::Tool { .. } | Item::Todo(_) | Item::Permission { .. }
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

/// Row `k` of `rows` (item indexes) in its run of tool rows.
pub fn group_shape_at(items: &[Item], rows: &[usize], k: usize) -> Option<Shape> {
    let is_tool = |k: Option<usize>| {
        k.and_then(|k| rows.get(k))
            .is_some_and(|&i| matches!(items[i], Item::Tool { .. }))
    };
    if !is_tool(Some(k)) {
        return None;
    }
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

/// Row `k`'s print: its item and which item it is, its shape, whether it is the last row and
/// carries the thinking line — what makes it laid out again (`agent_tab::sync_rows`).
pub fn row_print_at(items: &[Item], rows: &[usize], k: usize, thinking: bool) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    rows[k].hash(&mut h);
    crate::agent_tab::fingerprint(&items[rows[k]]).hash(&mut h);
    group_shape_at(items, rows, k).hash(&mut h);
    let last = k + 1 == rows.len();
    last.hash(&mut h);
    (last && thinking).hash(&mut h);
    h.finish()
}

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
            Item::Agent { done: false, .. } | Item::Permission { answer: None, .. }
        )
}

/// Main's rows (the helpers below as before M11ar, for the tests and callers by item index).
fn main_rows(items: &[Item]) -> Vec<usize> {
    rows_of(items, &View::Main)
}

pub fn group_shape(items: &[Item], ix: usize) -> Option<Shape> {
    let rows = main_rows(items);
    group_shape_at(items, &rows, rows.iter().position(|&r| r == ix)?)
}

/// The gap above row `ix`, from the row before it (spec B-B's table).
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
        Item::Permission { answer: None, .. } => 16.0,
        _ => 2.0,
    }
}

/// Whether the "Thinking…" line shows under Main's last row (all tools count, as before).
pub fn shows_thinking(working: bool, items: &[Item]) -> bool {
    let rows = main_rows(items);
    let Some(&last) = rows.last() else {
        return false;
    };
    working
        && !items
            .iter()
            .any(|i| matches!(i, Item::Tool { running: true, .. }))
        && !matches!(
            items[last],
            Item::Agent { done: false, .. } | Item::Permission { answer: None, .. }
        )
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
        Some(k) => row_print_at(items, &rows, k, thinking),
        None => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_conversation::{Answer, Item};

    fn you() -> Item {
        Item::You {
            text: "hi".into(),
            attachments: Vec::new(),
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
        }
    }

    fn asking(answered: bool) -> Item {
        Item::Permission {
            id: "p".into(),
            tool: "Bash".into(),
            what: "ls".into(),
            detail: String::new(),
            always: None,
            answer: answered.then_some(Answer::Allowed),
            by: None,
        }
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
            rows_of(&items, &View::Main),
            [0, 1, 3, 5],
            "a lane's steps aren't Main's rows; a step whose parent is gone is"
        );
        assert_eq!(rows_of(&items, &View::Lane("agent1".into())), [2, 4]);
        assert!(rows_of(&items, &View::Lane("nobody".into())).is_empty());
        // The helpers read the view's rows: in the lane, r2 follows r1 as one group.
        let lane = rows_of(&items, &View::Lane("agent1".into()));
        assert_eq!(group_shape_at(&items, &lane, 0), Some(Shape::First));
        assert_eq!(group_shape_at(&items, &lane, 1), Some(Shape::Last));
        assert_eq!(gap_before_at(&items, &lane, 0), chat::FIRST_TOP);
        assert!(shows_avatar_at(&items, &lane, 0));
        // A row's print knows which item it draws.
        let main = rows_of(&items, &View::Main);
        assert_ne!(
            row_print_at(&items, &main, 1, false),
            row_print_at(&items, &main, 2, false)
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
        assert_eq!(group_shape(&items, 0), None, "not a tool");
        assert_eq!(group_shape(&items, 1), Some(Shape::First));
        assert_eq!(
            group_shape(&items, 2),
            Some(Shape::Middle),
            "its step doesn't break it"
        );
        assert_eq!(group_shape(&items, 4), Some(Shape::Last));
        assert_eq!(group_shape(&items, 6), Some(Shape::Solo));
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

    #[test]
    fn thinking_shows_while_working_with_nothing_else_moving() {
        assert!(shows_thinking(true, &[you()]));
        assert!(shows_thinking(true, &[you(), tool("a", None, false)]));
        assert!(shows_thinking(true, &[you(), said(true)]));
        assert!(!shows_thinking(false, &[you()]), "not working");
        assert!(!shows_thinking(true, &[you(), said(false)]), "text streams");
        assert!(
            !shows_thinking(true, &[you(), tool("a", None, true)]),
            "a tool runs"
        );
        assert!(
            !shows_thinking(true, &[you(), asking(false)]),
            "it asks you"
        );
        assert!(!shows_thinking(true, &[]));
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
}
