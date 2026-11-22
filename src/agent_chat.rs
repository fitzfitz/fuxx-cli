//! The agent chat's layout rules (chores after M11ab, the lead designer's spec B-B): which items
//! are drawn as rows, the gaps between them, how a run of tool rows groups, where the avatar
//! goes, and when the "Thinking…" line shows. Pure; `agent_tab` draws with them.

use std::hash::{Hash, Hasher};

use crate::agent_conversation::Item;
use crate::agent_tool_modal::{index_of, parent_of};
pub use crate::ui_tokens::chat;

/// Whether item `ix` is a row of its own (a sub-agent's items are inside their tool's).
pub fn is_row(items: &[Item], ix: usize) -> bool {
    items
        .get(ix)
        .is_some_and(|i| parent_of(i).is_none_or(|p| index_of(items, p).is_none()))
}

/// The row before `ix`, and after it.
fn prev_row(items: &[Item], ix: usize) -> Option<usize> {
    (0..ix).rev().find(|&i| is_row(items, i))
}

fn next_row(items: &[Item], ix: usize) -> Option<usize> {
    (ix + 1..items.len()).find(|&i| is_row(items, i))
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

pub fn group_shape(items: &[Item], ix: usize) -> Option<Shape> {
    let is_tool = |i: Option<usize>| i.is_some_and(|i| matches!(items[i], Item::Tool { .. }));
    if !is_row(items, ix) || !is_tool(Some(ix)) {
        return None;
    }
    Some(
        match (is_tool(prev_row(items, ix)), is_tool(next_row(items, ix))) {
            (false, false) => Shape::Solo,
            (false, true) => Shape::First,
            (true, true) => Shape::Middle,
            (true, false) => Shape::Last,
        },
    )
}

/// The gap above row `ix`, from the row before it (spec B-B's table).
pub fn gap_before(items: &[Item], ix: usize) -> f32 {
    let Some(prev) = prev_row(items, ix).map(|p| &items[p]) else {
        return chat::FIRST_TOP;
    };
    let item = &items[ix];
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

/// Whether row `ix` carries the avatar: the first of a run of the agent's rows.
pub fn shows_avatar(items: &[Item], ix: usize) -> bool {
    agent_side(&items[ix]) && !prev_row(items, ix).is_some_and(|p| agent_side(&items[p]))
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

/// Whether the "Thinking…" line shows under the last row: the turn works and nothing else on
/// screen moves (no text streaming, no tool running, no question waiting).
pub fn shows_thinking(working: bool, items: &[Item]) -> bool {
    let Some(last) = (0..items.len()).rev().find(|&i| is_row(items, i)) else {
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

/// Row `ix`'s print: its item's, its shape, whether it is the last row and carries the
/// thinking line — what makes it laid out again (`agent_tab::sync_rows`).
pub fn row_print(items: &[Item], ix: usize, thinking: bool) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    crate::agent_tab::fingerprint(&items[ix]).hash(&mut h);
    group_shape(items, ix).hash(&mut h);
    let last = next_row(items, ix).is_none();
    last.hash(&mut h);
    (last && thinking).hash(&mut h);
    h.finish()
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
