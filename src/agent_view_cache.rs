//! One view's rows as derived from the conversation (the agent tab's perf chore, 2026-11-24):
//! its order (`agent_chat::rows_of`), each row's role in its step and print, the open check
//! failures and whether the thinking line shows. Brought up to date after each change from the
//! lowest item that changed (`Conversation::take_dirty`), so a streamed delta derives a row or
//! two, not the whole conversation; the render path only reads it. Pure and GPUI-free.
//!
//! The contract: `sync` after every change to the conversation (or `invalidate`), before the
//! rows are read. It starts again from nothing (`full`) when the lanes or the relays passed on
//! changed (`Conversation::shape`), the tracks' fit changed, or it was invalidated (a view
//! shown again after changes it didn't follow).

use std::collections::{BTreeSet, HashSet};
use std::hash::{Hash, Hasher};
use std::path::Path;

use crate::agent_chat::{in_main, lane_rows, row_print_at, rows_of};
use crate::agent_conversation::{Conversation, Item};
use crate::agent_lanes::View;
use crate::agent_steps::{
    CheckMark, Role, check_marks, open_of, roles_of, stands_alone, step_print,
};
use crate::status::Status;

#[derive(Default)]
pub struct ViewCache {
    order: Vec<usize>,
    prints: Vec<u64>,
    roles: Vec<Option<Role>>,
    thinking: bool,
    /// The open check failures (`agent_steps::open_failures`) from `turn` on.
    open: HashSet<usize>,
    turn: usize,
    fits: bool,
    shape: u64,
    /// Whether each lane's tool call is there, by the lanes' order (a lane's steps leave Main
    /// only then).
    live: Vec<bool>,
    /// The turn's check runs that move the open failures (`agent_steps::check_marks`).
    marks: Vec<CheckMark>,
    /// Main's order is item order (binary-searchable); a lane's may not be.
    sorted: bool,
    valid: bool,
    /// Rows derived by the last `sync` (the work-count tests).
    derived: usize,
}

/// Two sorted index sets as one sorted list.
fn merged(a: Option<&BTreeSet<usize>>, b: Option<&BTreeSet<usize>>) -> Vec<usize> {
    let a: Vec<usize> = a.map(|s| s.iter().copied().collect()).unwrap_or_default();
    match b {
        Some(b) => merge_sorted(a, &b.iter().copied().collect::<Vec<_>>()),
        None => a,
    }
}

/// Two sorted lists as one.
fn merge_sorted(a: Vec<usize>, b: &[usize]) -> Vec<usize> {
    if b.is_empty() {
        return a;
    }
    let mut out = Vec::with_capacity(a.len() + b.len());
    let (mut i, mut j) = (0, 0);
    while i < a.len() || j < b.len() {
        if j == b.len() || (i < a.len() && a[i] <= b[j]) {
            out.push(a[i]);
            i += 1;
        } else {
            out.push(b[j]);
            j += 1;
        }
    }
    out
}

pub(crate) fn hash<T: Hash>(what: &T) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    what.hash(&mut h);
    h.finish()
}

impl ViewCache {
    /// Start again from nothing at the next `sync`.
    pub fn invalidate(&mut self) {
        self.valid = false;
    }

    /// The items its rows draw, in order.
    pub fn order(&self) -> &[usize] {
        &self.order
    }

    /// Row `k`'s print: what makes it laid out again.
    pub fn print(&self, k: usize) -> u64 {
        self.prints[k]
    }

    /// Row `k`'s role in its step (None: not a tool row).
    pub fn role(&self, k: usize) -> Option<&Role> {
        self.roles.get(k)?.as_ref()
    }

    /// The open check failures its steps' tracks mark.
    pub fn open(&self) -> &HashSet<usize> {
        &self.open
    }

    /// Rows derived by the last `sync`.
    #[cfg(test)]
    pub fn derived(&self) -> usize {
        self.derived
    }

    /// The row drawing item `ix`, if any.
    pub fn row_of(&self, ix: usize) -> Option<usize> {
        if self.sorted {
            self.order.binary_search(&ix).ok()
        } else {
            self.order.iter().position(|&i| i == ix)
        }
    }

    /// Up to date with `c` as shown in `view`; `dirty`: the lowest item changed since the last
    /// sync (`Conversation::take_dirty`). Returns the first row whose print may have changed
    /// (the rows from there on were derived again; the order's length otherwise).
    pub fn sync(
        &mut self,
        c: &Conversation,
        view: &View,
        dirty: Option<usize>,
        fits: bool,
        cwd: &Path,
    ) -> usize {
        let items = c.items();
        // Which lanes' calls are there: a lane started before its call takes its steps out of
        // Main only once the call comes (no new lane, so no new shape).
        let shape = c.shape();
        let lanes = c.lanes();
        let live_changed = self.live.len() != lanes.len()
            || lanes
                .iter()
                .zip(&self.live)
                .any(|(l, &was)| c.has_tool(&l.id) != was);
        let sorted = matches!(view, View::Main);
        let full = !self.valid
            || self.fits != fits
            || self.shape != shape
            || live_changed
            || self.sorted != sorted;
        if live_changed {
            self.live = lanes.iter().map(|l| c.has_tool(&l.id)).collect();
        }
        let d = if full {
            0
        } else {
            dirty.unwrap_or(items.len()).min(items.len())
        };
        let old_len = if full { 0 } else { self.order.len() };
        // The order, and the rows that kept their item (`moved`: the first that didn't) and the
        // first whose item changed in place (`touched`).
        let (moved, mut touched) = match view {
            // Main's items before `d` keep their rows; the rest are found again in place.
            View::Main if !full => {
                let kept = self.order.partition_point(|&i| i < d);
                let was = self.order.split_off(kept);
                let lane_of = |p: &str| {
                    lanes
                        .iter()
                        .zip(&self.live)
                        .any(|(l, &live)| live && l.id == p)
                };
                self.order
                    .extend((d..items.len()).filter(|&ix| in_main(&items[ix], &lane_of)));
                let same = was
                    .iter()
                    .zip(&self.order[kept..])
                    .take_while(|(a, b)| a == b)
                    .count();
                (kept + same, kept)
            }
            View::Main => {
                let ids: Vec<&str> = lanes.iter().map(|l| l.id.as_str()).collect();
                self.order = rows_of(items, view, &ids);
                (0, 0)
            }
            // A lane's from what the conversation keeps: its own items, the relays, the open
            // asks (no scan of the conversation).
            View::Lane(id) => {
                let own = merged(c.under(id), c.asked_by(id));
                let asks = merge_sorted(merged(Some(c.open_permissions()), None), c.open_asks());
                let order = lane_rows(
                    items,
                    id,
                    own.into_iter(),
                    c.relays().iter().copied(),
                    asks.into_iter(),
                );
                let same = if full {
                    0
                } else {
                    self.order
                        .iter()
                        .zip(&order)
                        .take_while(|(a, b)| a == b)
                        .count()
                };
                self.order = order;
                let touched = self.order[..same]
                    .iter()
                    .position(|&i| i >= d)
                    .unwrap_or(same);
                (same, touched)
            }
        };
        let len = self.order.len();
        // The open failures: what a step's tracks mark, kept as the turn's check runs, those
        // from the change on found again. Only a change that isn't the agent's words moves them.
        let turn = c.turn_start();
        if full || turn != self.turn || items[d..].iter().any(|i| !matches!(i, Item::Agent { .. }))
        {
            if full || turn != self.turn {
                self.marks = check_marks(items, turn..items.len(), cwd);
            } else {
                let from = d.max(turn);
                self.marks.retain(|&(ix, _, _)| ix < from);
                self.marks
                    .extend(check_marks(items, from..items.len(), cwd));
            }
            let open = open_of(&self.marks);
            if let Some(&m) = open.symmetric_difference(&self.open).min() {
                let k = if sorted {
                    self.order.partition_point(|&i| i < m)
                } else {
                    self.order
                        .iter()
                        .position(|i| open.contains(i) != self.open.contains(i))
                        .unwrap_or(len)
                };
                touched = touched.min(k);
            }
            self.open = open;
            self.turn = turn;
        }
        let thinking = self.thinking_now(c, view);
        if thinking != self.thinking && len > 0 {
            touched = touched.min(len - 1);
        }
        self.thinking = thinking;
        // A row that came, went or moved changes the one before it too (its shape, or that
        // it is last).
        let mut start = touched.min(len);
        if moved < len || len != old_len {
            start = start.min(moved.min(len).saturating_sub(1));
        }
        if full {
            start = 0;
        }
        // From the start of its run of tool rows: a step's role and print hold the whole run.
        let tool = |k: usize| matches!(items[self.order[k]], Item::Tool { .. });
        while start > 0 && start < len && tool(start - 1) && tool(start) {
            start -= 1;
        }
        self.roles.truncate(start);
        self.prints.truncate(start);
        if start < len {
            let alone = |i: &Item| stands_alone(c, i);
            let version = |ix: usize| c.version(ix);
            self.roles
                .extend(roles_of(items, &self.order[start..], &alone, fits));
            for k in start..len {
                let row = row_print_at(items, &version, &self.order, k, thinking, &alone);
                let step = match &self.roles[k] {
                    Some(Role::Tracks(calls)) => step_print(&version, calls, &self.open, fits),
                    _ => 0,
                };
                self.prints.push(hash(&(row, &self.roles[k], step)));
            }
        }
        self.derived = len - start;
        self.fits = fits;
        self.shape = shape;
        self.sorted = sorted;
        self.valid = true;
        start
    }

    /// Whether the thinking line shows: the view works and nothing else there moves (no text
    /// streaming, no tool running, no question waiting). Main's counts every tool and its last
    /// item (as before the lanes: `agent_chat::shows_thinking`); a lane's its own rows.
    /// Read from what the conversation keeps (its status, its running calls) and this view's
    /// last row: no scan (the render path asks it for the last row).
    pub fn thinking_now(&self, c: &Conversation, view: &View) -> bool {
        let items = c.items();
        let (working, running, last) = match view {
            View::Main => (
                c.status() == Status::Working,
                !c.running_tools().is_empty(),
                items.last(),
            ),
            View::Lane(id) => (
                c.lane(id).is_some_and(|l| l.status.live()),
                c.running_tools()
                    .iter()
                    .any(|&ix| matches!(&items[ix], Item::Tool { parent: Some(p), .. } if p == id)),
                self.order.last().map(|&ix| &items[ix]),
            ),
        };
        working
            && !running
            && last.is_some_and(|last| {
                !matches!(
                    last,
                    Item::Agent { done: false, .. }
                        | Item::Permission { answer: None, .. }
                        | Item::Question { reply: None, .. }
                )
            })
    }
}

#[cfg(test)]
mod tests;
