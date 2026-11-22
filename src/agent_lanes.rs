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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_conversation::{Lane, LaneStatus};
    use crate::providers::LaneControls;

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
