//! The tool detail modal (chores after M11ab, the lead designer's spec A §B): a tool row's chevron
//! opens it over the agent tab, instead of the row expanding. Its state is a stack of tool ids
//! (a sub-agent's step opens inside it, Back returns), walked with ↑/↓ among the level's tools.

use crate::agent_conversation::Item;
use crate::providers::ToolDetail;

/// The open modal: the tools shown, outermost first (the last is on screen); `opened` keys its
/// open animation; `follow`: its body sticks to the bottom as a running tool's output grows.
#[derive(Clone, Debug, PartialEq)]
pub struct ToolModal {
    pub stack: Vec<String>,
    pub opened: u64,
    pub follow: bool,
    /// The shown tool was running at the last draw; `finished` counts its finishes seen while
    /// open (the header's check pops once, keyed by it).
    was_running: Option<bool>,
    pub finished: u64,
    /// The body's height to scroll at its last measurement (none before its first layout).
    last_max: Option<f32>,
}

impl ToolModal {
    pub fn new(stack: Vec<String>, opened: u64) -> Self {
        Self {
            stack,
            opened,
            follow: true,
            was_running: None,
            finished: 0,
            last_max: None,
        }
    }

    /// The body measured at a draw (`laid_out`: it has been; scrolled to `offset`, of `max`):
    /// whether to scroll it to the bottom now. The first layout only decides whether it follows
    /// (a body all in view does; a long one opens at its top); after that it follows its growth
    /// while it was near the bottom (`follows`).
    pub fn measured(&mut self, laid_out: bool, offset: f32, max: f32) -> bool {
        if !laid_out {
            return false;
        }
        let scroll = match self.last_max {
            None => {
                self.follow = near_bottom(offset, max);
                false
            }
            Some(last) => {
                let grew = max > last + 0.5;
                self.follow = follows(self.follow, grew, near_bottom(offset, last));
                self.follow && grew
            }
        };
        self.last_max = Some(max);
        scroll
    }

    /// The shown tool is `running` at this draw: whether it just finished (it ran at the last).
    pub fn saw(&mut self, running: bool) -> bool {
        let finished = self.was_running == Some(true) && !running;
        self.was_running = Some(running);
        if finished {
            self.finished += 1;
        }
        finished
    }

    /// Another tool on screen (↑/↓, a step, Back): it follows its bottom, nothing to pop.
    pub fn switched(&mut self) {
        self.follow = true;
        self.was_running = None;
        self.last_max = None;
    }

    /// The tool on screen.
    pub fn shown(&self) -> Option<&str> {
        self.stack.last().map(String::as_str)
    }

    /// The level the shown tool is in: the step it opened from, or the conversation (`None`).
    pub fn parent(&self) -> Option<&str> {
        self.stack
            .len()
            .checked_sub(2)
            .map(|i| self.stack[i].as_str())
    }

    /// Keep what is still in `items`: the stack up to its first vanished tool. Whether anything
    /// is left (false: close it).
    pub fn prune(&mut self, items: &[Item]) -> bool {
        let kept = self
            .stack
            .iter()
            .take_while(|id| index_of(items, id).is_some())
            .count();
        self.stack.resize(kept, String::new());
        !self.stack.is_empty()
    }
}

/// The modal's width in a pane `pane_w` wide (spec A §B): 24 px in from each side up to 720;
/// 8 in a pane under 360.
pub fn modal_width(pane_w: f32) -> f32 {
    if pane_w < 360.0 {
        (pane_w - 16.0).max(0.0)
    } else {
        (pane_w - 48.0).min(720.0)
    }
}

/// How close to the bottom still counts as at it.
const FOLLOW_SLACK: f32 = 24.0;

/// Whether a body scrolled to `offset_y` (0 to −`max_y`) is at its bottom.
pub fn near_bottom(offset_y: f32, max_y: f32) -> bool {
    max_y + offset_y <= FOLLOW_SLACK
}

/// Whether the body follows its bottom now: through growth it keeps what it did (`was`); else it
/// does while it is near the bottom (scrolled up, it stops; back down, it follows again).
pub fn follows(was: bool, grew: bool, near: bool) -> bool {
    if grew { was } else { near }
}

/// The header's right side (spec A §B).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Right {
    /// Running: how long, tabular.
    Elapsed(String),
    /// An edit: lines added and removed.
    Diff(usize, usize),
    /// Failed: the result, without the row's "✗".
    Failed(String),
    Plain(String),
}

/// The header's right side for a tool `running`, finished `ok`, with this `result` (the row's),
/// `diff` counts (added, removed) and `elapsed` label.
pub fn right_of(
    running: bool,
    ok: Option<bool>,
    result: &str,
    diff: Option<(usize, usize)>,
    elapsed: &str,
) -> Right {
    if running {
        return Right::Elapsed(elapsed.into());
    }
    if ok == Some(false) {
        let bare = result.trim_start_matches('✗').trim_start();
        return Right::Failed(bare.into());
    }
    match diff {
        Some((a, r)) => Right::Diff(a, r),
        None => Right::Plain(result.into()),
    }
}

/// Where the tool `id` is among `items`.
pub fn index_of(items: &[Item], id: &str) -> Option<usize> {
    items
        .iter()
        .position(|i| matches!(i, Item::Tool { id: t, .. } if t == id))
}

/// Whether the tool at `ix` has anything more to show (its row's chevron): a diff, a command,
/// output (not a read's, and not an edit's unless it failed). A sub-agent's steps are its
/// lane's (M11ar): its row opens the lane.
pub fn openable(items: &[Item], ix: usize) -> bool {
    let Some(Item::Tool {
        name,
        ok,
        detail,
        output,
        ..
    }) = items.get(ix)
    else {
        return false;
    };
    let diff = matches!(detail, Some(ToolDetail::Diff { .. }));
    diff || matches!(detail, Some(ToolDetail::Command(_)))
        || shows_output(name, output, detail, *ok)
}

/// Whether a tool's output is worth showing: not a read's, and not an edit's (its diff says it)
/// unless it failed.
pub fn shows_output(
    name: &str,
    output: &Option<String>,
    detail: &Option<ToolDetail>,
    ok: Option<bool>,
) -> bool {
    let diff = matches!(detail, Some(ToolDetail::Diff { .. }));
    name != "Read" && output.is_some() && (!diff || ok == Some(false))
}

/// The tool call an item belongs to (a sub-agent's), if any.
pub fn parent_of(item: &Item) -> Option<&str> {
    match item {
        Item::Agent { parent, .. } | Item::Tool { parent, .. } => parent.as_deref(),
        // A resume's divider is its lane's (the lanes chore).
        Item::Resumed { lane, .. } => Some(lane),
        _ => None,
    }
}

/// The openable tools at one level (`parent`: a step's, or the conversation's), in order.
pub fn siblings(items: &[Item], parent: Option<&str>) -> Vec<String> {
    items
        .iter()
        .enumerate()
        .filter(|(ix, i)| parent_of(i) == parent && openable(items, *ix))
        .filter_map(|(_, i)| match i {
            Item::Tool { id, .. } => Some(id.clone()),
            _ => None,
        })
        .collect()
}

/// The tool ↑ (`dir` −1) or ↓ (+1) of the shown one, among its level's; none past the ends.
pub fn step(items: &[Item], modal: &ToolModal, dir: i8) -> Option<String> {
    let shown = modal.shown()?;
    // The shown tool's own level: its parent call (a step opened from a lane's view has no
    // parent in the stack), else the stack's.
    let own = index_of(items, shown).and_then(|ix| parent_of(&items[ix]));
    let level = siblings(items, own.or(modal.parent()));
    let at = level.iter().position(|id| id == shown)?;
    let to = at.checked_add_signed(isize::from(dir))?;
    level.get(to).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_conversation::Item;
    use crate::providers::ToolDetail;

    fn tool(id: &str, name: &str, parent: Option<&str>, detail: Option<ToolDetail>) -> Item {
        Item::Tool {
            id: id.into(),
            name: name.into(),
            summary: String::new(),
            parent: parent.map(String::from),
            running: false,
            ok: Some(true),
            result: String::new(),
            detail,
            output: Some("out".into()),
            dropped_lines: 0,
        }
    }

    fn bash(id: &str, parent: Option<&str>) -> Item {
        tool(id, "Bash", parent, Some(ToolDetail::Command("ls".into())))
    }

    /// A read (nothing to show), two Bash, a Task with two steps, another Bash.
    fn items() -> Vec<Item> {
        vec![
            tool("r", "Read", None, None),
            bash("a", None),
            bash("b", None),
            tool("task", "Task", None, None),
            bash("t1", Some("task")),
            bash("t2", Some("task")),
            bash("c", None),
        ]
    }

    fn open(stack: &[&str]) -> ToolModal {
        ToolModal::new(stack.iter().map(|s| s.to_string()).collect(), 0)
    }

    #[test]
    fn a_read_opens_nothing_and_a_task_opens_its_steps() {
        let items = items();
        assert!(!openable(&items, 0), "a read shows nothing more");
        assert!(openable(&items, 1));
        assert!(openable(&items, 3), "a task with output");
    }

    #[test]
    fn a_subagent_row_opens_its_lane_not_the_modal() {
        // M11ar: a task's steps are its lane's (its row opens the lane), not the modal's.
        let mut task = tool("task", "Agent", None, None);
        if let Item::Tool { output, .. } = &mut task {
            *output = None;
        }
        let items = vec![task, bash("t1", Some("task"))];
        assert!(!openable(&items, 0));
    }

    #[test]
    fn siblings_are_the_level_s_openable_tools() {
        let items = items();
        assert_eq!(siblings(&items, None), ["a", "b", "task", "c"]);
        assert_eq!(siblings(&items, Some("task")), ["t1", "t2"]);
    }

    #[test]
    fn a_lane_steps_window_walks_its_own_steps() {
        // M11ar review: a step opened from a lane's view (its stack is just the step) walks the
        // lane's steps, not Main's tools.
        let items = items();
        assert_eq!(step(&items, &open(&["t1"]), 1).as_deref(), Some("t2"));
        assert_eq!(step(&items, &open(&["t2"]), -1).as_deref(), Some("t1"));
    }

    #[test]
    fn up_and_down_stop_at_the_ends() {
        let items = items();
        assert_eq!(step(&items, &open(&["a"]), -1), None);
        assert_eq!(step(&items, &open(&["a"]), 1).as_deref(), Some("b"));
        assert_eq!(step(&items, &open(&["c"]), 1), None);
        assert_eq!(step(&items, &open(&["c"]), -1).as_deref(), Some("task"));
        // Inside a step: its siblings, not the conversation's.
        assert_eq!(
            step(&items, &open(&["task", "t1"]), 1).as_deref(),
            Some("t2")
        );
        assert_eq!(step(&items, &open(&["task", "t2"]), 1), None);
    }

    #[test]
    fn a_vanished_tool_closes_or_backs_out_of_the_modal() {
        let items = items();
        let mut m = open(&["task", "t2"]);
        assert!(m.prune(&items));
        assert_eq!(m.shown(), Some("t2"));
        let without_step: Vec<Item> = items.iter().filter(|i| !is(i, "t2")).cloned().collect();
        assert!(m.prune(&without_step));
        assert_eq!(m.shown(), Some("task"), "back to the step's parent");
        assert!(!m.prune(&[]), "the conversation emptied: closed");
    }

    fn is(item: &Item, id: &str) -> bool {
        matches!(item, Item::Tool { id: i, .. } if i == id)
    }

    #[test]
    fn the_modal_is_720_wide_at_most_and_hugs_narrow_panes() {
        assert_eq!(modal_width(1000.0), 720.0);
        assert_eq!(modal_width(600.0), 552.0);
        assert_eq!(modal_width(340.0), 324.0);
    }

    #[test]
    fn the_body_follows_the_bottom_until_you_scroll_up() {
        // Output grew: it keeps following if it was.
        assert!(follows(true, true, false));
        assert!(!follows(false, true, false));
        // Nothing grew: following is being near the bottom (24 px).
        assert!(!follows(true, false, false));
        assert!(follows(false, false, true));
        assert!(near_bottom(-100.0, 110.0));
        assert!(!near_bottom(-50.0, 110.0));
    }

    #[test]
    fn the_header_says_elapsed_lines_changed_or_the_failure() {
        assert_eq!(
            right_of(true, None, "", None, "31s"),
            Right::Elapsed("31s".into())
        );
        assert_eq!(
            right_of(false, Some(true), "updated", Some((12, 3)), ""),
            Right::Diff(12, 3)
        );
        assert_eq!(
            right_of(false, Some(false), "✗ exit 1", Some((1, 1)), ""),
            Right::Failed("exit 1".into()),
            "a failure says so, without the ✗ (the mark carries it)"
        );
        assert_eq!(
            right_of(false, Some(true), "ok", None, ""),
            Right::Plain("ok".into())
        );
    }

    #[test]
    fn a_tool_finishing_while_shown_pops_once() {
        let mut m = open(&["a"]);
        assert!(!m.saw(true), "running");
        assert!(m.saw(false), "it finished while open");
        assert!(!m.saw(false), "only once");
        let mut done = open(&["b"]);
        assert!(!done.saw(false), "opened finished: no pop");
    }

    /// The review's finding 3: a long tool opens at its top. The body's first measurements
    /// (before it is laid out, then its first layout) are not growth; it follows the bottom
    /// only from there, while near it.
    #[test]
    fn a_long_tool_opens_at_its_top_and_follows_only_from_the_bottom() {
        let mut m = open(&["a"]);
        assert!(!m.measured(false, 0.0, 0.0), "not laid out yet: nothing");
        assert!(
            !m.measured(true, 0.0, 800.0),
            "first layout: no jump to the bottom"
        );
        assert!(!m.follow, "at the top of a long body: not following");
        assert!(!m.measured(true, 0.0, 900.0), "it grows: still at the top");
        // Scrolled to the bottom, it follows the growth.
        assert!(!m.measured(true, -900.0, 900.0));
        assert!(
            m.measured(true, -900.0, 950.0),
            "grew while at the bottom: follow"
        );
        // A short body (all in view) follows from the start.
        let mut short = open(&["b"]);
        assert!(!short.measured(true, 0.0, 10.0));
        assert!(short.follow);
        assert!(short.measured(true, 0.0, 60.0));
    }
}
