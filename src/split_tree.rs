//! Nested binary split tree (like Ghostty/iTerm panes). Pure and GPUI-free, so the rules are unit-tested.

use serde::{Deserialize, Serialize};

/// `Row` = side by side (split right), `Column` = stacked (split down).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Axis {
    Row,
    Column,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // used from M8 Task 4
pub enum Dir {
    Left,
    Right,
    Up,
    Down,
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[allow(dead_code)] // used from M8 Task 4
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

#[allow(dead_code)] // used from M8 Task 4
pub const MIN_RATIO: f32 = 0.1;
#[allow(dead_code)] // used from M8 Task 4
pub const MAX_RATIO: f32 = 0.9;

/// Slack when comparing rect edges.
#[allow(dead_code)] // used from M8 Task 4
const EPS: f32 = 1e-4;

/// A leaf (one pane) or a split of two subtrees; `ratio` is the share the first child gets.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Node<T> {
    Leaf(T),
    Split {
        axis: Axis,
        ratio: f32,
        first: Box<Node<T>>,
        second: Box<Node<T>>,
    },
}

/// Clamp into `MIN_RATIO..=MAX_RATIO`; NaN becomes an even split.
#[allow(dead_code)] // used from M8 Task 4
fn clamp_ratio(ratio: f32) -> f32 {
    if ratio.is_nan() {
        0.5
    } else {
        ratio.clamp(MIN_RATIO, MAX_RATIO)
    }
}

/// The areas of a split's two children.
#[allow(dead_code)] // used from M8 Task 4
fn child_rects(axis: Axis, ratio: f32, area: Rect) -> (Rect, Rect) {
    match axis {
        Axis::Row => {
            let w = area.w * ratio;
            (
                Rect { w, ..area },
                Rect {
                    x: area.x + w,
                    w: area.w - w,
                    ..area
                },
            )
        }
        Axis::Column => {
            let h = area.h * ratio;
            (
                Rect { h, ..area },
                Rect {
                    y: area.y + h,
                    h: area.h - h,
                    ..area
                },
            )
        }
    }
}

impl<T: Copy + PartialEq> Node<T> {
    /// Leaves in reading order (first before second, recursively).
    pub fn leaves(&self) -> Vec<T> {
        match self {
            Node::Leaf(id) => vec![*id],
            Node::Split { first, second, .. } => {
                let mut out = first.leaves();
                out.extend(second.leaves());
                out
            }
        }
    }

    pub fn contains(&self, id: T) -> bool {
        match self {
            Node::Leaf(leaf) => *leaf == id,
            Node::Split { first, second, .. } => first.contains(id) || second.contains(id),
        }
    }

    /// Replace the `target` leaf by a split of it and `new`; false if `target` is absent.
    pub fn split(&mut self, target: T, new: T, axis: Axis) -> bool {
        match self {
            Node::Leaf(leaf) => {
                if *leaf != target {
                    return false;
                }
                *self = Node::Split {
                    axis,
                    ratio: 0.5,
                    first: Box::new(Node::Leaf(target)),
                    second: Box::new(Node::Leaf(new)),
                };
                true
            }
            Node::Split { first, second, .. } => {
                first.split(target, new, axis) || second.split(target, new, axis)
            }
        }
    }

    /// The tree without `target`; its sibling takes the parent's place. `None` if the tree was
    /// only that leaf; unchanged if `target` is absent.
    pub fn remove(self, target: T) -> Option<Node<T>> {
        match self {
            Node::Leaf(leaf) => (leaf != target).then_some(Node::Leaf(leaf)),
            Node::Split {
                axis,
                ratio,
                first,
                second,
            } => {
                let (first, second) = (*first, *second);
                if first.contains(target) {
                    match first.remove(target) {
                        None => Some(second),
                        Some(first) => Some(Node::Split {
                            axis,
                            ratio,
                            first: Box::new(first),
                            second: Box::new(second),
                        }),
                    }
                } else if second.contains(target) {
                    match second.remove(target) {
                        None => Some(first),
                        Some(second) => Some(Node::Split {
                            axis,
                            ratio,
                            first: Box::new(first),
                            second: Box::new(second),
                        }),
                    }
                } else {
                    Some(Node::Split {
                        axis,
                        ratio,
                        first: Box::new(first),
                        second: Box::new(second),
                    })
                }
            }
        }
    }

    /// Each leaf's area within `area`, in reading order.
    #[allow(dead_code)] // used from M8 Task 4
    pub fn rects(&self, area: Rect) -> Vec<(T, Rect)> {
        match self {
            Node::Leaf(id) => vec![(*id, area)],
            Node::Split {
                axis,
                ratio,
                first,
                second,
            } => {
                let (a, b) = child_rects(*axis, *ratio, area);
                let mut out = first.rects(a);
                out.extend(second.rects(b));
                out
            }
        }
    }

    /// The axis and area of the split reached by `path` (`false` = first child, `true` = second).
    #[allow(dead_code)] // used from M8 Task 4
    pub fn split_rect(&self, path: &[bool], area: Rect) -> Option<(Axis, Rect)> {
        match self {
            Node::Leaf(_) => None,
            Node::Split {
                axis,
                ratio,
                first,
                second,
            } => match path.split_first() {
                None => Some((*axis, area)),
                Some((&to_second, rest)) => {
                    let (a, b) = child_rects(*axis, *ratio, area);
                    if to_second {
                        second.split_rect(rest, b)
                    } else {
                        first.split_rect(rest, a)
                    }
                }
            },
        }
    }

    /// Set the ratio of the split reached by `path`, clamped; false if `path` ends elsewhere.
    #[allow(dead_code)] // used from M8 Task 4
    pub fn set_ratio(&mut self, path: &[bool], ratio: f32) -> bool {
        match self {
            Node::Leaf(_) => false,
            Node::Split {
                ratio: own,
                first,
                second,
                ..
            } => match path.split_first() {
                None => {
                    *own = clamp_ratio(ratio);
                    true
                }
                Some((&to_second, rest)) => {
                    if to_second {
                        second.set_ratio(rest, ratio)
                    } else {
                        first.set_ratio(rest, ratio)
                    }
                }
            },
        }
    }

    /// The leaf next to `from` in direction `dir`, judged on the rects of a unit square: it lies
    /// wholly on that side and overlaps on the other axis. Nearest wins, then biggest overlap,
    /// then reading order.
    #[allow(dead_code)] // used from M8 Task 4
    pub fn neighbor(&self, from: T, dir: Dir) -> Option<T> {
        let rects = self.rects(Rect {
            x: 0.0,
            y: 0.0,
            w: 1.0,
            h: 1.0,
        });
        let f = rects.iter().find(|(id, _)| *id == from)?.1;
        let mut best: Option<(T, f32, f32)> = None; // (id, gap, overlap)
        for &(id, c) in &rects {
            if id == from {
                continue;
            }
            let (gap, overlap) = match dir {
                Dir::Left => (f.x - (c.x + c.w), span_overlap(f.y, f.h, c.y, c.h)),
                Dir::Right => (c.x - (f.x + f.w), span_overlap(f.y, f.h, c.y, c.h)),
                Dir::Up => (f.y - (c.y + c.h), span_overlap(f.x, f.w, c.x, c.w)),
                Dir::Down => (c.y - (f.y + f.h), span_overlap(f.x, f.w, c.x, c.w)),
            };
            if gap < -EPS || overlap <= EPS {
                continue;
            }
            let better = match best {
                None => true,
                Some((_, bg, bo)) => {
                    gap < bg - EPS || ((gap - bg).abs() <= EPS && overlap > bo + EPS)
                }
            };
            if better {
                best = Some((id, gap, overlap));
            }
        }
        best.map(|(id, ..)| id)
    }

    /// The next (or previous) leaf in reading order, wrapping; `None` if `from` is absent or alone.
    pub fn cycle(&self, from: T, forward: bool) -> Option<T> {
        let leaves = self.leaves();
        if leaves.len() < 2 {
            return None;
        }
        let at = leaves.iter().position(|id| *id == from)?;
        let n = leaves.len();
        Some(
            leaves[if forward {
                (at + 1) % n
            } else {
                (at + n - 1) % n
            }],
        )
    }

    /// Give every leaf an equal share: a child on the same axis counts as all its leaves along it.
    #[allow(dead_code)] // used from M8 Task 4
    pub fn equalize(&mut self) {
        if let Node::Split {
            axis,
            ratio,
            first,
            second,
        } = self
        {
            first.equalize();
            second.equalize();
            let (a, b) = (first.weight(*axis), second.weight(*axis));
            *ratio = a / (a + b);
        }
    }

    /// How many equal shares this node takes along `axis`.
    fn weight(&self, axis: Axis) -> f32 {
        match self {
            Node::Split {
                axis: own,
                first,
                second,
                ..
            } if *own == axis => first.weight(axis) + second.weight(axis),
            _ => 1.0,
        }
    }

    #[allow(dead_code)] // used from M8 Task 5
    pub fn map<U>(&self, f: &impl Fn(T) -> U) -> Node<U> {
        match self {
            Node::Leaf(id) => Node::Leaf(f(*id)),
            Node::Split {
                axis,
                ratio,
                first,
                second,
            } => Node::Split {
                axis: *axis,
                ratio: *ratio,
                first: Box::new(first.map(f)),
                second: Box::new(second.map(f)),
            },
        }
    }
}

/// Length of the overlap of two spans `[a, a + a_len]` and `[b, b + b_len]` (negative if apart).
#[allow(dead_code)] // used from M8 Task 4
fn span_overlap(a: f32, a_len: f32, b: f32, b_len: f32) -> f32 {
    (a + a_len).min(b + b_len) - a.max(b)
}

/// Clamp every ratio in place; true if any changed.
#[allow(dead_code)] // used from M8 Task 5
fn fix_ratios(node: &mut Node<usize>) -> bool {
    match node {
        Node::Leaf(_) => false,
        Node::Split {
            ratio,
            first,
            second,
            ..
        } => {
            let fixed = clamp_ratio(*ratio);
            let mut changed = fixed != *ratio;
            *ratio = fixed;
            changed |= fix_ratios(first);
            changed |= fix_ratios(second);
            changed
        }
    }
}

/// Make loaded trees match `count` sessions: drop trees with an out-of-range or repeated index,
/// clamp ratios, then add a leaf for every session no kept tree shows. The bool is true when a
/// tree was dropped or a ratio fixed.
#[allow(dead_code)] // used from M8 Task 5
pub fn sanitize(trees: Vec<Node<usize>>, count: usize) -> (Vec<Node<usize>>, bool) {
    let mut used = vec![false; count];
    let mut kept = Vec::new();
    let mut changed = false;
    for mut tree in trees {
        let leaves = tree.leaves();
        let valid = leaves.iter().all(|&i| i < count && !used[i])
            && leaves
                .iter()
                .enumerate()
                .all(|(n, i)| !leaves[..n].contains(i));
        if !valid {
            changed = true;
            continue;
        }
        for i in leaves {
            used[i] = true;
        }
        changed |= fix_ratios(&mut tree);
        kept.push(tree);
    }
    for (i, taken) in used.into_iter().enumerate() {
        if !taken {
            kept.push(Node::Leaf(i));
        }
    }
    (kept, changed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn three() -> Node<u32> {
        // 1 | (2 over 3)
        let mut t = Node::Leaf(1);
        assert!(t.split(1, 2, Axis::Row));
        assert!(t.split(2, 3, Axis::Column));
        t
    }
    const AREA: Rect = Rect {
        x: 0.0,
        y: 0.0,
        w: 100.0,
        h: 50.0,
    };

    #[test]
    fn split_keeps_reading_order() {
        let mut t = three();
        assert_eq!(t.leaves(), [1, 2, 3]);
        assert!(!t.split(9, 4, Axis::Row));
        assert!(t.contains(3) && !t.contains(9));
    }

    #[test]
    fn remove_gives_the_space_to_the_sibling() {
        let t = three().remove(2).unwrap();
        assert_eq!(
            t,
            Node::Split {
                axis: Axis::Row,
                ratio: 0.5,
                first: Box::new(Node::Leaf(1)),
                second: Box::new(Node::Leaf(3))
            }
        );
        assert_eq!(Node::Leaf(1).remove(1), None);
        assert_eq!(three().remove(7), Some(three()));
    }

    #[test]
    fn rects_follow_the_ratios() {
        assert_eq!(
            three().rects(AREA),
            vec![
                (
                    1,
                    Rect {
                        x: 0.0,
                        y: 0.0,
                        w: 50.0,
                        h: 50.0
                    }
                ),
                (
                    2,
                    Rect {
                        x: 50.0,
                        y: 0.0,
                        w: 50.0,
                        h: 25.0
                    }
                ),
                (
                    3,
                    Rect {
                        x: 50.0,
                        y: 25.0,
                        w: 50.0,
                        h: 25.0
                    }
                ),
            ]
        );
        assert_eq!(
            three().split_rect(&[true], AREA),
            Some((
                Axis::Column,
                Rect {
                    x: 50.0,
                    y: 0.0,
                    w: 50.0,
                    h: 50.0
                }
            ))
        );
        assert_eq!(three().split_rect(&[false], AREA), None);
    }

    #[test]
    fn neighbor_by_direction() {
        let t = three();
        assert_eq!(t.neighbor(2, Dir::Left), Some(1));
        assert_eq!(t.neighbor(3, Dir::Left), Some(1));
        assert_eq!(t.neighbor(1, Dir::Right), Some(2)); // tie on gap and overlap: reading order
        assert_eq!(t.neighbor(2, Dir::Down), Some(3));
        assert_eq!(t.neighbor(3, Dir::Up), Some(2));
        assert_eq!(t.neighbor(1, Dir::Up), None);
    }

    #[test]
    fn cycle_wraps() {
        assert_eq!(three().cycle(3, true), Some(1));
        assert_eq!(three().cycle(1, false), Some(3));
        assert_eq!(Node::Leaf(1).cycle(1, true), None);
    }

    #[test]
    fn set_ratio_clamps() {
        let mut t = three();
        assert!(t.set_ratio(&[], 0.05));
        assert!(t.set_ratio(&[true], 0.95));
        assert!(!t.set_ratio(&[false], 0.5));
        let Node::Split { ratio, second, .. } = &t else {
            panic!()
        };
        assert_eq!(*ratio, MIN_RATIO);
        assert!(matches!(**second, Node::Split { ratio, .. } if ratio == MAX_RATIO));
        assert!(t.set_ratio(&[], f32::NAN));
        assert!(matches!(t, Node::Split { ratio, .. } if ratio == 0.5));
    }

    #[test]
    fn equalize_counts_leaves_along_the_axis() {
        let mut t = three(); // the Column child counts as 1 across a Row
        t.set_ratio(&[], 0.8);
        t.equalize();
        assert!(matches!(t, Node::Split { ratio, .. } if ratio == 0.5));
        let mut row3 = Node::Leaf(1);
        row3.split(1, 2, Axis::Row);
        row3.split(2, 3, Axis::Row); // 1 | 2 | 3
        row3.equalize();
        let Node::Split { ratio, second, .. } = &row3 else {
            panic!()
        };
        assert!((ratio - 1.0 / 3.0).abs() < 1e-6);
        assert!(matches!(**second, Node::Split { ratio, .. } if ratio == 0.5));
    }

    #[test]
    fn sanitize_drops_bad_trees_and_fixes_ratios() {
        let leaf = |i| Box::new(Node::Leaf(i));
        let ok = Node::Split {
            axis: Axis::Row,
            ratio: 2.0,
            first: leaf(0),
            second: leaf(1),
        };
        let out_of_range = Node::Split {
            axis: Axis::Row,
            ratio: 0.5,
            first: leaf(2),
            second: leaf(7),
        };
        let repeated = Node::Leaf(1);
        let (trees, changed) = sanitize(vec![ok, out_of_range, repeated], 3);
        assert!(changed);
        assert_eq!(
            trees,
            vec![
                Node::Split {
                    axis: Axis::Row,
                    ratio: MAX_RATIO,
                    first: leaf(0),
                    second: leaf(1)
                },
                Node::Leaf(2),
            ]
        );
        assert_eq!(
            sanitize(vec![Node::Leaf(0)], 1),
            (vec![Node::Leaf(0)], false)
        );
    }
}
