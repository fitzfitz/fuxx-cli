//! Subagent lanes for the agent tab's UI (M11ar): which view is shown, the pills' labels, and
//! stepping between views. Pure and GPUI-free; the lanes themselves are the conversation's.

use crate::agent_conversation::Lane;

/// What the agent tab's chat shows: the main agent's conversation, or one subagent's.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub enum View {
    #[default]
    Main,
    Lane(String),
}

/// A pill's label: `label`, cut to `max` characters with an ellipsis.
pub fn pill_label(label: &str, max: usize) -> String {
    if label.chars().count() <= max {
        return label.to_string();
    }
    let kept: String = label.chars().take(max.saturating_sub(1)).collect();
    format!("{}…", kept.trim_end())
}

/// Every lane's pill label, in order; a label already used gets " 2", " 3" ….
pub fn pill_labels(lanes: &[Lane], max: usize) -> Vec<String> {
    let mut seen: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    lanes
        .iter()
        .map(|l| {
            let n = seen.entry(l.label.clone()).or_insert(0);
            *n += 1;
            let label = if *n == 1 {
                l.label.clone()
            } else {
                format!("{} {n}", l.label)
            };
            pill_label(&label, max)
        })
        .collect()
}

/// The view `dir` pills from `view` (Main first, then the lanes; no wrap). A lane no longer
/// there goes back to Main.
pub fn step(view: &View, lanes: &[Lane], dir: i32) -> View {
    let at = match view {
        View::Main => 0,
        View::Lane(id) => match lanes.iter().position(|l| &l.id == id) {
            Some(i) => i + 1,
            None => return View::Main,
        },
    };
    let to = (at as i64 + dir as i64).clamp(0, lanes.len() as i64) as usize;
    match to {
        0 => View::Main,
        n => View::Lane(lanes[n - 1].id.clone()),
    }
}

/// The gap between pills, and the "+N" pill's width.
pub const PILL_GAP: f32 = 8.0;
pub const MORE_W: f32 = 26.0;

/// Which pills (by index, in order) fit in `avail` with these `widths`, and how many are left
/// behind "+N". Main (index 0) and the `active` pill always show when anything is hidden.
pub fn fit(widths: &[f32], avail: f32, active: usize) -> (Vec<usize>, usize) {
    let n = widths.len();
    let total: f32 = widths.iter().sum::<f32>() + PILL_GAP * n.saturating_sub(1) as f32;
    if total <= avail {
        return ((0..n).collect(), 0);
    }
    let room = |shown: &[usize], plus: Option<usize>| {
        let pills: f32 = shown.iter().chain(plus.iter()).map(|&i| widths[i]).sum();
        let count = shown.len() + plus.iter().count();
        pills + PILL_GAP * count as f32 + MORE_W <= avail
    };
    let mut shown: Vec<usize> = Vec::new();
    for i in 0..n {
        if !room(&shown, Some(i)) {
            break;
        }
        shown.push(i);
    }
    if active < n && !shown.contains(&active) {
        while shown.len() > 1 && !room(&shown, Some(active)) {
            shown.pop();
        }
        shown.push(active);
    }
    shown.sort_unstable();
    let hidden = n - shown.len();
    (shown, hidden)
}

/// How long a lane has run: to its end, else to `now`; none without a start (a resumed lane).
pub fn elapsed(
    started: Option<std::time::Instant>,
    ended: Option<std::time::Instant>,
    now: std::time::Instant,
) -> Option<std::time::Duration> {
    let start = started?;
    Some(ended.unwrap_or(now).saturating_duration_since(start))
}

/// Whether a lane's header offers Stop: it is still going and its agent can stop it alone.
pub fn shows_stop(lane: &Lane) -> bool {
    lane.controls.stop && lane.status.live()
}

/// "1 step", "14 steps".
pub fn steps_label(n: usize) -> String {
    match n {
        0 => "No steps".into(),
        1 => "1 step".into(),
        n => format!("{n} steps"),
    }
}

/// `1:02`.
pub fn clock(d: std::time::Duration) -> String {
    let s = d.as_secs();
    format!("{}:{:02}", s / 60, s % 60)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_conversation::{Lane, LaneStatus};
    use crate::providers::LaneControls;
    use std::time::{Duration, Instant};

    fn lane(id: &str, label: &str) -> Lane {
        Lane {
            id: id.into(),
            parent: None,
            label: label.into(),
            kind: "Explore".into(),
            task: String::new(),
            model: None,
            status: LaneStatus::Working,
            tokens: None,
            tools: None,
            last_tool: None,
            controls: LaneControls::default(),
        }
    }

    #[test]
    fn pills_that_fit_show_the_rest_behind_plus_n() {
        // Main (60) and three lanes (50 each), 8 apart; "+N" takes 32.
        let widths = [60.0, 50.0, 50.0, 50.0];
        assert_eq!(fit(&widths, 400.0, 0), (vec![0, 1, 2, 3], 0), "all fit");
        let (shown, hidden) = fit(&widths, 210.0, 0);
        assert_eq!(shown, [0, 1, 2], "as many as fit with room for +N");
        assert_eq!(hidden, 1);
        // The active pill is always shown (in place of the last that fit).
        let (shown, hidden) = fit(&widths, 210.0, 3);
        assert!(shown.contains(&3) && shown.contains(&0), "{shown:?}");
        assert_eq!(hidden, 1);
        assert_eq!(fit(&[], 100.0, 0), (vec![], 0));
    }

    #[test]
    fn elapsed_stops_when_the_lane_ends() {
        let t0 = Instant::now();
        let t1 = t0 + Duration::from_secs(62);
        let later = t1 + Duration::from_secs(100);
        assert_eq!(elapsed(Some(t0), None, t1), Some(Duration::from_secs(62)));
        assert_eq!(
            elapsed(Some(t0), Some(t1), later),
            Some(Duration::from_secs(62)),
            "it stops at the end"
        );
        assert_eq!(elapsed(None, None, later), None, "no start: no time");
        assert_eq!(clock(Duration::from_secs(62)), "1:02");
        assert_eq!(clock(Duration::from_secs(5)), "0:05");
    }

    #[test]
    fn stop_shows_only_when_the_lane_can() {
        let mut working = lane("a", "A");
        working.controls.stop = true;
        assert!(shows_stop(&working), "Claude's, working");
        let mut waiting = working.clone();
        waiting.status = LaneStatus::Waiting;
        assert!(shows_stop(&waiting));
        let mut done = working.clone();
        done.status = LaneStatus::Done;
        assert!(!shows_stop(&done), "nothing to stop");
        assert!(!shows_stop(&lane("b", "B")), "agy's: no control");
    }

    #[test]
    fn steps_say_one_or_many() {
        assert_eq!(steps_label(1), "1 step");
        assert_eq!(steps_label(14), "14 steps");
        assert_eq!(steps_label(0), "No steps");
    }

    #[test]
    fn a_pill_label_is_cut_with_an_ellipsis() {
        assert_eq!(pill_label("Workspace File Reader", 12), "Workspace F…");
        assert_eq!(pill_label("Reader", 12), "Reader");
        assert_eq!(pill_label("Reader", 6), "Reader", "exactly the limit");
    }

    #[test]
    fn repeated_labels_are_numbered() {
        let lanes = [
            lane("a", "Reader"),
            lane("b", "Reader"),
            lane("c", "Tester"),
        ];
        assert_eq!(pill_labels(&lanes, 20), ["Reader", "Reader 2", "Tester"]);
    }

    #[test]
    fn stepping_goes_through_main_then_the_lanes() {
        let lanes = [lane("a", "A"), lane("b", "B")];
        assert_eq!(step(&View::Main, &lanes, 1), View::Lane("a".into()));
        assert_eq!(step(&View::Main, &lanes, -1), View::Main, "Main is first");
        assert_eq!(step(&View::Lane("a".into()), &lanes, -1), View::Main);
        assert_eq!(
            step(&View::Lane("b".into()), &lanes, 1),
            View::Lane("b".into()),
            "no wrap"
        );
        assert_eq!(step(&View::Main, &[], 1), View::Main, "no lanes");
        assert_eq!(
            step(&View::Lane("gone".into()), &lanes, 1),
            View::Main,
            "a lane that went: back to Main"
        );
    }
}
