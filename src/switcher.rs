//! M11: getting around. ⇧⌘U takes you to the terminal that has waited longest for you (each
//! press the next one: its workspace, its pane, its tab). What the ⌘P list knows of a workspace
//! (`Entry`, most urgent first by `urgency_order`) and how it draws one (`workspace_row`) live
//! here; since M11j the list itself is the command palette's (`palette.rs`), which ⌘P opens on
//! its Workspaces scope.

use std::cmp::Ordering;
use std::ops::Range;
use std::time::Instant;

use gpui::{
    App, Context, Div, FontWeight, HighlightStyle, StyledText, Window, div, prelude::*, px,
};

use crate::groups::{self, GroupKey};
use crate::sidebar::{MARK_SLOT, color_dot, mark_size, mark_slot, tabular};
use crate::status::{self, Status};
use crate::status_mark;
use crate::tab_color::TabColor;
use crate::terminal_view::hsla;
use crate::theme::UiColors;
use crate::ui_tokens::text;
use crate::workspace::Workspace;

/// The space between rows (the Help panel's and the palette's too).
pub(crate) const ROW_GAP: f32 = 2.0;
/// A row, and a waiting one with its question line (the Help panel's rows are `ROW` tall too).
/// The sidebar's rows were these heights until M11i made them compact; the palette keeps them.
pub(crate) const ROW: f32 = 28.0;
pub(crate) const WAITING_ROW: f32 = 42.0;
/// Eight rows, two of them waiting ones, show before the list scrolls (the Help panel's list is
/// as tall).
pub(crate) const LIST_MAX_HEIGHT: f32 = 8.0 * (ROW + ROW_GAP) + 2.0 * (WAITING_ROW - ROW);
/// The `#n` column, wide enough for two digits.
const NUMBER_WIDTH: f32 = 26.0;
const MARK_GAP: f32 = 8.0;
const NUMBER_GAP: f32 = 4.0;
/// A long group label is cut short so the name keeps its room.
const GROUP_MAX_WIDTH: f32 = 160.0;
/// The question line starts under the name.
const QUESTION_INDENT: f32 = MARK_SLOT + MARK_GAP + NUMBER_WIDTH + NUMBER_GAP;

/// What the ⌘P list shows of one workspace, in sidebar order (its number is its place + 1).
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    /// Its workspace's id (`WorkspaceId`'s number): where a row goes, while the list changes.
    pub id: u64,
    pub label: String,
    /// Its group as the sidebar labels it; none when ungrouped.
    pub group: Option<String>,
    pub status: Status,
    /// When `status` last changed.
    pub since: Instant,
    /// While Waiting: the screen line that asked.
    pub question: Option<String>,
    pub color: Option<TabColor>,
}

/// How urgent a status is: the lower, the sooner it's listed.
fn rank(status: Status) -> u8 {
    match status {
        Status::Waiting => 0,
        Status::Done => 1,
        Status::Working => 2,
        Status::Idle => 3,
    }
}

/// The sessions (each one's status and when it began) in order of urgency: Waiting, the longest
/// waiting first; Done, the longest finished first; then Working and Idle; ties by number.
pub fn urgency_order(sessions: &[(Status, Instant)]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..sessions.len()).collect();
    order.sort_by(|&a, &b| {
        let ((status_a, since_a), (status_b, since_b)) = (sessions[a], sessions[b]);
        let by_age = match status_a {
            Status::Waiting | Status::Done => since_a.cmp(&since_b),
            Status::Working | Status::Idle => Ordering::Equal,
        };
        rank(status_a)
            .cmp(&rank(status_b))
            .then(by_age)
            .then(a.cmp(&b))
    });
    order
}

/// Where ⇧⌘U goes from `current`: the sessions that need you (Waiting, then Done, as in
/// `urgency_order`), the one after `current` there, round to the first; the first if `current`
/// isn't one of them. None when nothing needs you.
pub fn next_needing_you(sessions: &[(Status, Instant)], current: Option<usize>) -> Option<usize> {
    let queue: Vec<usize> = urgency_order(sessions)
        .into_iter()
        .filter(|&i| matches!(sessions[i].0, Status::Waiting | Status::Done))
        .collect();
    if queue.is_empty() {
        return None;
    }
    let next = current
        .and_then(|c| queue.iter().position(|&i| i == c))
        .map_or(0, |at| (at + 1) % queue.len());
    Some(queue[next])
}

/// Workspace `number`'s row (`entry`): its mark, `#n`, name (the matched letters, at the byte
/// ranges `highlights`, semibold) and its tab colour, group, and how long in its status; a
/// waiting one's question under them. `chosen`: filled `row_active`, the name medium; else
/// `panel_hover` under the pointer. `phase` is the marks' clock (the Workspace's), so a working
/// mark turns with the sidebar's.
pub(crate) fn workspace_row(
    entry: &Entry,
    number: usize,
    chosen: bool,
    highlights: &[Range<usize>],
    phase: f32,
    ui: &UiColors,
) -> Div {
    let ui = *ui;
    let fill = if chosen { ui.row_active } else { ui.panel };
    let hover = ui.panel_hover;
    let waiting = entry.status == Status::Waiting;
    let age = (entry.status != Status::Idle)
        .then(|| status::age_label(Instant::now().saturating_duration_since(entry.since)));
    let meta = |color| {
        div()
            .flex_none()
            .text_size(px(text::META))
            .font_features(tabular())
            .text_color(hsla(color))
    };
    let mark = status_mark::mark_on(
        entry.status,
        &ui,
        phase,
        None,
        px(mark_size(entry.status)),
        fill,
    );
    let matched = HighlightStyle {
        font_weight: Some(FontWeight::SEMIBOLD),
        ..Default::default()
    };
    let name = StyledText::new(entry.label.clone())
        .with_highlights(highlights.iter().map(|r| (r.clone(), matched)));
    let line = div()
        .flex()
        .items_center()
        .child(mark_slot(mark))
        .child(
            meta(ui.text_faint)
                .w(px(NUMBER_WIDTH))
                .ml(px(MARK_GAP))
                .mr(px(NUMBER_GAP))
                .child(format!("#{number}")),
        )
        .child(
            div()
                .min_w(px(0.0))
                .truncate()
                .when(chosen, |name| name.font_weight(FontWeight::MEDIUM))
                .child(name),
        )
        .when_some(entry.color, |line, color| {
            line.child(color_dot(color, ui).ml(px(6.0)))
        })
        .when_some(entry.group.clone(), |line, group| {
            line.child(
                meta(ui.text_faint)
                    .max_w(px(GROUP_MAX_WIDTH))
                    .ml(px(8.0))
                    .truncate()
                    .child(group),
            )
        })
        .child(div().flex_1())
        .when_some(age, |line, age| {
            line.child(meta(ui.text_faint).ml(px(8.0)).child(age))
        });
    // A waiting row says what it asks (or that it needs you), as the sidebar does.
    let question = waiting.then(|| match &entry.question {
        Some(q) => (q.clone(), ui.text_dim),
        None => ("Needs your input".to_string(), ui.text_faint),
    });
    div()
        .flex_none()
        .flex()
        .flex_col()
        .justify_center()
        .h(px(if waiting { WAITING_ROW } else { ROW }))
        .px(px(8.0))
        .rounded(px(6.0))
        .text_size(px(text::NAME))
        .when(chosen, |row| row.bg(hsla(ui.row_active)))
        .when(!chosen, |row| row.hover(move |s| s.bg(hsla(hover))))
        .child(line)
        .when_some(question, |row, (question, color)| {
            row.child(
                div()
                    .pl(px(QUESTION_INDENT))
                    .text_size(px(text::META))
                    .text_color(hsla(color))
                    .truncate()
                    .child(question),
            )
        })
}

impl Workspace {
    /// Every workspace as the ⌘P list shows it, in sidebar order (so its `#n` is its ⌘N): its
    /// name, group and colour, and the rest from its most urgent terminal. One of browser tabs
    /// only is idle since its first tab opened.
    pub(crate) fn switcher_entries(&self, cx: &App) -> Vec<Entry> {
        let keys = self.sidebar_groups();
        let all: Vec<&GroupKey> = keys.iter().flatten().collect();
        self.model
            .display_order()
            .into_iter()
            .map(|ws| {
                let (status, since, question) = match self.rollup_session(ws) {
                    Some(i) => {
                        let s = &self.sessions.items()[i];
                        (s.status, s.status_since, s.question.clone())
                    }
                    None => {
                        let tabs = self.model.tabs_of(ws);
                        let pages = tabs
                            .iter()
                            .filter_map(|t| self.browser_tabs.get(t))
                            .map(|v| v.read(cx).since);
                        let files = tabs
                            .iter()
                            .filter_map(|t| self.file_tabs.get(t))
                            .map(|v| v.read(cx).since);
                        let opened = pages.chain(files).min();
                        (Status::Idle, opened.unwrap_or(self.started), None)
                    }
                };
                Entry {
                    id: ws.0,
                    label: self.workspace_label(ws).to_string(),
                    group: self
                        .workspace_group(ws)
                        .map(|k| groups::group_label(&k, &all)),
                    status,
                    since,
                    question,
                    color: self.model.workspace(ws).and_then(|w| w.color),
                }
            })
            .collect()
    }

    /// ⇧⌘U: go to the terminal that has waited longest for you (its workspace, pane and tab),
    /// the next one on each press (`next_needing_you`). When nothing needs you, nothing
    /// happens.
    pub(crate) fn jump_to_needing_you(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let facts: Vec<(Status, Instant)> = self
            .sessions
            .items()
            .iter()
            .map(|s| (s.status, s.status_since))
            .collect();
        if let Some(index) = next_needing_you(&facts, self.sessions.active_index()) {
            self.select(index, window, cx);
            // Already the active one: it still takes the keyboard back (from a rename box, say),
            // so the menus and the popover close as on any other move.
            self.close_menus();
            self.focus_active(window, cx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::next_needing_you;
    use crate::status::Status;
    use std::time::{Duration, Instant};

    /// A session that has been `status` for `secs` seconds.
    fn at(now: Instant, status: Status, secs: u64) -> (Status, Instant) {
        (status, now - Duration::from_secs(secs))
    }

    #[test]
    fn urgency_order() {
        let now = Instant::now();
        let sessions = [
            at(now, Status::Idle, 900),     // #1
            at(now, Status::Working, 10),   // #2
            at(now, Status::Waiting, 30),   // #3
            at(now, Status::Done, 60),      // #4
            at(now, Status::Waiting, 300),  // #5: waited longest
            at(now, Status::Done, 600),     // #6: finished longest ago
            at(now, Status::Working, 5000), // #7
            at(now, Status::Idle, 1),       // #8
        ];
        // Waiting (longest first), Done (longest first), then Working and Idle by number.
        assert_eq!(super::urgency_order(&sessions), [4, 2, 5, 3, 1, 6, 0, 7]);
        // The same age: by number.
        let tied = [at(now, Status::Waiting, 9), at(now, Status::Waiting, 9)];
        assert_eq!(super::urgency_order(&tied), [0, 1]);
        assert!(super::urgency_order(&[]).is_empty());
    }

    #[test]
    fn next_needing_you_cycles() {
        let now = Instant::now();
        let sessions = [
            at(now, Status::Working, 10), // #1
            at(now, Status::Done, 60),    // #2
            at(now, Status::Waiting, 30), // #3
            at(now, Status::Waiting, 90), // #4: waited longest
            at(now, Status::Idle, 0),     // #5
        ];
        // From a session that doesn't need you: the one that waited longest.
        assert_eq!(next_needing_you(&sessions, Some(0)), Some(3));
        assert_eq!(next_needing_you(&sessions, None), Some(3));
        // Each press the next: the other waiting one, then the done one, then round again.
        assert_eq!(next_needing_you(&sessions, Some(3)), Some(2));
        assert_eq!(next_needing_you(&sessions, Some(2)), Some(1));
        assert_eq!(next_needing_you(&sessions, Some(1)), Some(3));
        // Only one: it stays there.
        let one = [at(now, Status::Idle, 0), at(now, Status::Done, 5)];
        assert_eq!(next_needing_you(&one, Some(1)), Some(1));
        // Nothing needs you: nowhere.
        let calm = [at(now, Status::Working, 1), at(now, Status::Idle, 1)];
        assert_eq!(next_needing_you(&calm, Some(0)), None);
        assert_eq!(next_needing_you(&[], None), None);
    }
}
