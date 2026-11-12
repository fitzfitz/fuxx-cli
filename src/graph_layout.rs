//! The GRAPH's lanes (M11w, note 74): which column each commit sits in and how its row's
//! lines run, from the commits newest first with their parents. Pure; drawn by the panel.
//!
//! Walking down the list, a set of open lanes each waits for one commit. A commit takes the
//! lane that waits for it (else the first free one); lanes that also waited for it end in it
//! (a merge of lines); its first parent takes its lane on, or joins a lane already waiting for
//! that parent; each other parent joins such a lane or opens a new one.

/// One row's drawing, in lane columns (0 the leftmost; each at most `cap - 1`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// The commit's column.
    pub lane: usize,
    /// Lanes that come down from the row above into the commit's dot.
    pub into: Vec<usize>,
    /// Lanes that leave the dot for the row below (its parents' lanes).
    pub out: Vec<usize>,
    /// Lanes that pass straight through the row, untouched.
    pub through: Vec<usize>,
    /// More than one parent.
    pub merge: bool,
}

/// The rows for `commits` (each its id and its parents' ids, newest first). At most `cap`
/// columns are drawn: a lane past them is drawn in the last one.
pub fn layout(commits: &[(String, Vec<String>)], cap: usize) -> Vec<Row> {
    let last = cap.max(1) - 1;
    let clamp =
        |lanes: Vec<usize>| -> Vec<usize> { lanes.into_iter().map(|l| l.min(last)).collect() };
    // Each open lane: the id of the commit it waits for.
    let mut lanes: Vec<Option<&str>> = Vec::new();
    let mut rows = Vec::with_capacity(commits.len());
    for (id, parents) in commits {
        let before: Vec<bool> = lanes.iter().map(Option::is_some).collect();
        let into: Vec<usize> = (0..lanes.len())
            .filter(|&i| lanes[i] == Some(id.as_str()))
            .collect();
        let lane = into.first().copied().unwrap_or_else(|| free(&mut lanes));
        for &i in &into {
            lanes[i] = None;
        }
        let mut out = Vec::new();
        for (n, parent) in parents.iter().enumerate() {
            let waiting = lanes.iter().position(|w| *w == Some(parent.as_str()));
            let to = match waiting {
                Some(k) => k,
                None if n == 0 => {
                    lanes[lane] = Some(parent);
                    lane
                }
                None => {
                    let k = free(&mut lanes);
                    lanes[k] = Some(parent);
                    k
                }
            };
            out.push(to);
        }
        let through = (0..before.len())
            .filter(|&j| before[j] && !into.contains(&j) && lanes[j].is_some())
            .collect();
        while lanes.last() == Some(&None) {
            lanes.pop();
        }
        rows.push(Row {
            lane: lane.min(last),
            into: clamp(into),
            out: clamp(out),
            through: clamp(through),
            merge: parents.len() > 1,
        });
    }
    rows
}

/// The first free lane (a new one at the right if none is).
fn free(lanes: &mut Vec<Option<&str>>) -> usize {
    match lanes.iter().position(Option::is_none) {
        Some(i) => i,
        None => {
            lanes.push(None);
            lanes.len() - 1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(id: &str, parents: &[&str]) -> (String, Vec<String>) {
        (
            id.to_string(),
            parents.iter().map(|p| p.to_string()).collect(),
        )
    }

    fn row(lane: usize, into: &[usize], out: &[usize], through: &[usize], merge: bool) -> Row {
        Row {
            lane,
            into: into.to_vec(),
            out: out.to_vec(),
            through: through.to_vec(),
            merge,
        }
    }

    #[test]
    fn a_straight_history_is_one_lane() {
        let rows = layout(&[c("a", &["b"]), c("b", &["c"]), c("c", &[])], 8);
        assert_eq!(
            rows,
            vec![
                row(0, &[], &[0], &[], false),
                row(0, &[0], &[0], &[], false),
                row(0, &[0], &[], &[], false),
            ]
        );
    }

    /// M merges B into A; both come from C.
    #[test]
    fn a_fork_and_a_merge() {
        let rows = layout(
            &[
                c("m", &["a", "b"]),
                c("a", &["c"]),
                c("b", &["c"]),
                c("c", &[]),
            ],
            8,
        );
        assert_eq!(
            rows,
            vec![
                row(0, &[], &[0, 1], &[], true),
                row(0, &[0], &[0], &[1], false),
                // b's line curves back into c's lane.
                row(1, &[1], &[0], &[0], false),
                row(0, &[0], &[], &[], false),
            ]
        );
    }

    /// Two branch tips on one parent: the second joins the first's lane.
    #[test]
    fn two_tips_on_one_parent_meet() {
        let rows = layout(&[c("a", &["p"]), c("b", &["p"]), c("p", &[])], 8);
        assert_eq!(rows[1], row(1, &[], &[0], &[0], false));
        assert_eq!(rows[2], row(0, &[0], &[], &[], false));
    }

    #[test]
    fn an_octopus_merge_opens_a_lane_per_parent() {
        let rows = layout(
            &[
                c("m", &["a", "b", "x"]),
                c("a", &[]),
                c("b", &[]),
                c("x", &[]),
            ],
            8,
        );
        assert_eq!(rows[0], row(0, &[], &[0, 1, 2], &[], true));
        assert_eq!(rows[3].lane, 2);
    }

    /// The page ends before the parents: their lanes run off the bottom.
    #[test]
    fn parents_past_the_page_leave_their_lanes_open() {
        let rows = layout(&[c("a", &["b", "z"])], 8);
        assert_eq!(rows, vec![row(0, &[], &[0, 1], &[], true)]);
    }

    #[test]
    fn lanes_past_the_cap_share_the_last_column() {
        let rows = layout(
            &[
                c("m", &["a", "b", "x"]),
                c("a", &[]),
                c("b", &[]),
                c("x", &[]),
            ],
            2,
        );
        assert_eq!(rows[0].out, vec![0, 1, 1]);
        assert!(rows.iter().all(|r| r.lane < 2));
    }
}
