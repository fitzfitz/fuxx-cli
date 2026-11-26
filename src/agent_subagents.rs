//! The subagents corner tab (chore 19, note 93): `🤖 4/6` beside the shells and plan tabs, and
//! its popover's list. Pure: the view (`agent_subagents_view`) hands it the lanes.

use crate::agent_conversation::{Lane, LaneStatus};

/// The tab's ink: peach while one runs, green once all finished, red while one needs you.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabTone {
    Running,
    Done,
    NeedsYou,
}

/// Whether the chat shows the running mark (chore 31: then it is the only one that spins):
/// Main's status line (`status_line_shown`, chore 17), or a subagent's own view while it is live
/// (`lane_live`: its header and rows say it).
pub fn chat_mark_shown(status_line: bool, lane_live: Option<bool>) -> bool {
    status_line || lane_live == Some(true)
}

/// Whether the folder tab's status mark hides (chores 17 and 31): while the chat shows the
/// running mark, except a wait on you (`waiting`) in a subagent's view, where nothing else says
/// it (Main's status line, which would, isn't there).
pub fn folder_mark_hidden(status_line: bool, lane_live: Option<bool>, waiting: bool) -> bool {
    status_line || (lane_live == Some(true) && !waiting)
}

/// Whether the tab spins beside its count: a subagent runs and the chat shows no running mark.
pub fn tab_spins(lanes: &[Lane], chat_mark: bool) -> bool {
    !chat_mark && lanes.iter().any(|l| l.status == LaneStatus::Working)
}

/// Finished (in any way) and all.
pub fn counts(lanes: &[Lane]) -> (usize, usize) {
    let done = lanes.iter().filter(|l| !l.status.live()).count();
    (done, lanes.len())
}

/// The tab's words: `4/6`, or `6/6 ✓` once all finished.
pub fn tab_words(lanes: &[Lane]) -> String {
    match counts(lanes) {
        (done, all) if done == all => format!("{done}/{all} \u{2713}"),
        (done, all) => format!("{done}/{all}"),
    }
}

/// The tab's tone: one waiting on you wins, then one running.
pub fn tab_tone(lanes: &[Lane]) -> TabTone {
    if lanes.iter().any(|l| l.status == LaneStatus::Waiting) {
        TabTone::NeedsYou
    } else if lanes.iter().any(|l| l.status.live()) {
        TabTone::Running
    } else {
        TabTone::Done
    }
}

/// The popover's header: "2 of 6 done · 3 running · 1 needs you" (no zero parts after the first).
pub fn header(lanes: &[Lane]) -> String {
    let (done, all) = counts(lanes);
    let count = |s: LaneStatus| lanes.iter().filter(|l| l.status == s).count();
    let (running, waiting) = (count(LaneStatus::Working), count(LaneStatus::Waiting));
    [
        Some(format!("{done} of {all} done")),
        (running > 0).then(|| format!("{running} running")),
        (waiting > 0).then(|| format!("{waiting} needs you")),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" \u{b7} ")
}

/// The popover's rows, as indexes into `lanes` (start order): the ones that need you, then the
/// running ones, then the finished ones; newest first in each.
pub fn order(lanes: &[Lane]) -> Vec<usize> {
    let rank = |s: &LaneStatus| match s {
        LaneStatus::Waiting => 0,
        LaneStatus::Working => 1,
        _ => 2,
    };
    let mut out: Vec<usize> = (0..lanes.len()).rev().collect();
    // A stable sort keeps newest first inside each group.
    out.sort_by_key(|&i| rank(&lanes[i].status));
    out
}

/// A row's right-side words for the popover: the chat row's (`agent_lanes::lane_row_words`),
/// with one waiting on you saying "Needs you".
pub fn row_words(words: &str, status: &LaneStatus) -> String {
    match status {
        LaneStatus::Waiting => words.replacen("Needs input", "Needs you", 1),
        _ => words.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::LaneControls;

    fn lane(id: &str, status: LaneStatus) -> Lane {
        Lane {
            id: id.into(),
            parent: None,
            label: id.into(),
            kind: "Explore".into(),
            task: String::new(),
            model: None,
            status,
            tokens: None,
            tools: None,
            last_tool: None,
            controls: LaneControls::default(),
            runs: 1,
        }
    }

    /// Chore 31: one running mark at a time. The chat's (Main's status line, or a live
    /// subagent's own view) is it while it shows; the tab spins only without it, and only while
    /// one runs; the relay chip's mark stays still beside it.
    #[test]
    fn only_the_chat_mark_spins() {
        assert!(chat_mark_shown(true, None), "Main's status line");
        assert!(!chat_mark_shown(false, None));
        assert!(chat_mark_shown(false, Some(true)), "a live subagent's view");
        assert!(!chat_mark_shown(false, Some(false)), "a finished one's");
        let running = [lane("a", LaneStatus::Working)];
        assert!(!tab_spins(&running, true));
        assert!(tab_spins(&running, false));
        assert!(!tab_spins(&[lane("a", LaneStatus::Done)], false));
        assert!(
            !tab_spins(&[lane("a", LaneStatus::Waiting)], false),
            "waits: red, still"
        );
    }

    /// The review of chores 24–31: in a subagent's view the folder tab's mark is the only sign
    /// that Main (or another subagent) waits on you, so it hides there only while work runs;
    /// in Main the status line carries the question (chore 17).
    #[test]
    fn the_folder_mark_keeps_a_wait_in_a_subagent_view() {
        assert!(
            folder_mark_hidden(true, None, true),
            "Main: the line says it"
        );
        assert!(
            folder_mark_hidden(false, Some(true), false),
            "a live view, working"
        );
        assert!(
            !folder_mark_hidden(false, Some(true), true),
            "a live view, one waits"
        );
        assert!(
            !folder_mark_hidden(false, Some(false), false),
            "a finished one's view"
        );
        assert!(!folder_mark_hidden(false, None, false));
    }

    fn six() -> Vec<Lane> {
        vec![
            lane("a", LaneStatus::Done),
            lane("b", LaneStatus::Working),
            lane("c", LaneStatus::Failed("boom".into())),
            lane("d", LaneStatus::Waiting),
            lane("e", LaneStatus::Stopped),
            lane("f", LaneStatus::Working),
        ]
    }

    #[test]
    fn the_tab_counts_finished_of_all() {
        assert_eq!(tab_words(&six()), "3/6");
        let done = vec![lane("a", LaneStatus::Done), lane("b", LaneStatus::Ended)];
        assert_eq!(tab_words(&done), "2/2 \u{2713}");
    }

    #[test]
    fn needing_you_beats_running_beats_done() {
        assert_eq!(tab_tone(&six()), TabTone::NeedsYou);
        let running = vec![lane("a", LaneStatus::Done), lane("b", LaneStatus::Working)];
        assert_eq!(tab_tone(&running), TabTone::Running);
        let done = vec![
            lane("a", LaneStatus::Done),
            lane("b", LaneStatus::Failed(String::new())),
        ];
        assert_eq!(tab_tone(&done), TabTone::Done);
    }

    #[test]
    fn the_header_says_done_running_and_needs_you() {
        assert_eq!(
            header(&six()),
            "3 of 6 done \u{b7} 2 running \u{b7} 1 needs you"
        );
        let done = vec![lane("a", LaneStatus::Done)];
        assert_eq!(header(&done), "1 of 1 done");
    }

    #[test]
    fn rows_put_you_first_then_running_then_finished_newest_first() {
        // d waits; f and b run (f newer); e, c, a finished (newest first).
        assert_eq!(order(&six()), [3, 5, 1, 4, 2, 0]);
    }

    #[test]
    fn a_waiting_row_says_needs_you() {
        assert_eq!(
            row_words("Needs input \u{b7} 1:02", &LaneStatus::Waiting),
            "Needs you \u{b7} 1:02"
        );
        assert_eq!(
            row_words("Running Read \u{b7} 0:12", &LaneStatus::Working),
            "Running Read \u{b7} 0:12"
        );
    }
}
