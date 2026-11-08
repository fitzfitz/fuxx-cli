//! M11: getting around. ⇧⌘U takes you to the terminal that has waited longest for you (each
//! press the next one: its workspace, its pane, its tab); ⌘P opens the switcher, a panel near
//! the top of the window: a filter box over every workspace (M11b), most urgent first, each row
//! with its most urgent terminal's mark, its number, name (and its colour's dot), group and age,
//! and a waiting one's question under it. Enter goes to that terminal's pane.
//!
//! The box is the sidebar's rename box (`NameInput`), as in the host picker: its Enter goes to
//! the selected row, its Escape (or losing focus) closes the switcher.

use std::cmp::Ordering;
use std::time::Instant;

use gpui::{
    AnyElement, App, ClickEvent, Context, Entity, EventEmitter, FocusHandle, Focusable, FontWeight,
    MouseDownEvent, ScrollHandle, Subscription, Window, actions, div, prelude::*, px,
};

use crate::appearance::Appearance;
use crate::groups::{self, GroupKey};
use crate::host_picker::move_selection;
use crate::layout_model::WorkspaceId;
use crate::name_input::{NameInput, NameInputEvent};
use crate::overlay_panel;
use crate::sidebar::{MARK_SLOT, color_dot, mark_size, mark_slot, tabular};
use crate::status::{self, Status};
use crate::status_mark;
use crate::tab_color::TabColor;
use crate::terminal_view::hsla;
use crate::ui_tokens::size::{SIDEBAR_ROW, WAITING_ROW};
use crate::ui_tokens::text;
use crate::workspace::{Overlay, Workspace};

// Bound in main.rs with the "Switcher" key context; Enter and Escape are the box's own.
actions!(switcher, [SelectPrev, SelectNext]);

/// Wider than the host picker: a row holds a name, its group and a question.
const PANEL_WIDTH: f32 = 520.0;
/// The space between rows (the Help panel's too).
pub(crate) const ROW_GAP: f32 = 2.0;
/// Eight rows, two of them waiting ones, show before the list scrolls (the Help panel's list is
/// as tall).
pub(crate) const LIST_MAX_HEIGHT: f32 =
    8.0 * (SIDEBAR_ROW + ROW_GAP) + 2.0 * (WAITING_ROW - SIDEBAR_ROW);
/// The `#n` column, wide enough for two digits.
const NUMBER_WIDTH: f32 = 26.0;
const MARK_GAP: f32 = 8.0;
const NUMBER_GAP: f32 = 4.0;
/// A long group label is cut short so the name keeps its room.
const GROUP_MAX_WIDTH: f32 = 160.0;
/// The question line starts under the name.
const QUESTION_INDENT: f32 = MARK_SLOT + MARK_GAP + NUMBER_WIDTH + NUMBER_GAP;

const PLACEHOLDER: &str = "Go to a workspace \u{2014} number, name, group or question";
const HINT: &str = "Enter go \u{b7} \u{2191}\u{2193} choose \u{b7} Esc close \u{b7} \u{21e7}\u{2318}U next that needs you";

/// What the switcher shows of one workspace, in sidebar order (its number is its place + 1).
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

/// Whether session `number` matches the switcher's filter: the number itself ("3" or "#3"), or
/// the text anywhere in its name, group or question, in any case. A blank filter matches all.
pub fn matches(number: usize, entry: &Entry, query: &str) -> bool {
    let query = query.trim();
    if query.is_empty() || query.strip_prefix('#').unwrap_or(query) == number.to_string() {
        return true;
    }
    let needle = query.to_lowercase();
    [
        Some(entry.label.as_str()),
        entry.group.as_deref(),
        entry.question.as_deref(),
    ]
    .into_iter()
    .flatten()
    .any(|text| text.to_lowercase().contains(&needle))
}

/// The switcher's rows: the sessions (by place in `entries`) matching `query`, most urgent first.
pub fn filter_rows(entries: &[Entry], query: &str) -> Vec<usize> {
    let facts: Vec<(Status, Instant)> = entries.iter().map(|e| (e.status, e.since)).collect();
    urgency_order(&facts)
        .into_iter()
        .filter(|&i| matches(i + 1, &entries[i], query))
        .collect()
}

pub enum SwitcherEvent {
    /// Go to this workspace (by `Entry::id`).
    Jump(u64),
    /// Escape, a click outside, or the box lost focus.
    Cancel,
}

/// The ⌘P panel. Its sessions come from the Workspace, fresh each time it draws (`refresh`), so
/// the marks move and the order follows the statuses while it's open.
pub struct Switcher {
    input: Entity<NameInput>,
    /// The box's text as last filtered.
    query: String,
    entries: Vec<Entry>,
    /// The marks' clock (the Workspace's), so a working mark here turns with the sidebar's.
    phase: f32,
    /// The rows shown: places in `entries`, most urgent first.
    rows: Vec<usize>,
    selected: usize,
    scroll: ScrollHandle,
    _input_events: Subscription,
    _input_changes: Subscription,
}

impl EventEmitter<SwitcherEvent> for Switcher {}

impl Focusable for Switcher {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.input.focus_handle(cx)
    }
}

impl Switcher {
    pub fn new(
        entries: Vec<Entry>,
        phase: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let input = cx.new(|cx| NameInput::new(String::new(), window, cx));
        let input_events = cx.subscribe_in(&input, window, |this, _, event, _, cx| match event {
            NameInputEvent::Confirm(_) => this.jump(this.selected, cx),
            NameInputEvent::Cancel => cx.emit(SwitcherEvent::Cancel),
        });
        let input_changes = cx.observe(&input, |this, _, cx| this.query_changed(cx));
        let mut this = Self {
            input,
            query: String::new(),
            entries,
            phase,
            rows: Vec::new(),
            selected: 0,
            scroll: ScrollHandle::new(),
            _input_events: input_events,
            _input_changes: input_changes,
        };
        this.refilter(true);
        this
    }

    /// The sessions as they are now. The selection stays on its session if it is still shown.
    pub fn refresh(&mut self, entries: Vec<Entry>, phase: f32) {
        self.phase = phase;
        if entries != self.entries {
            self.entries = entries;
            self.refilter(false);
        }
    }

    /// The sessions shown, for the marks' ticker and the age refresh.
    pub fn shown(&self) -> impl Iterator<Item = &Entry> {
        self.rows.iter().map(|&i| &self.entries[i])
    }

    /// The box changed (typing, or the IME's composition): filter again if its text did.
    fn query_changed(&mut self, cx: &mut Context<Self>) {
        let query = self.input.read(cx).text().to_string();
        if query != self.query {
            self.query = query;
            self.refilter(true);
            cx.notify();
        }
    }

    /// Rebuild the rows. `from_top`: select the first; else keep the selected session.
    fn refilter(&mut self, from_top: bool) {
        let before = self.selected_id();
        self.rows = filter_rows(&self.entries, &self.query);
        let kept = before
            .filter(|_| !from_top)
            .and_then(|id| self.rows.iter().position(|&i| self.entries[i].id == id));
        let moved = kept != Some(self.selected);
        self.selected = kept.unwrap_or(0);
        if moved {
            self.scroll.scroll_to_item(self.selected);
        }
    }

    fn selected_id(&self) -> Option<u64> {
        self.rows.get(self.selected).map(|&i| self.entries[i].id)
    }

    fn select_by(&mut self, delta: i32, cx: &mut Context<Self>) {
        self.selected = move_selection(self.selected, self.rows.len(), delta);
        self.scroll.scroll_to_item(self.selected);
        cx.notify();
    }

    /// Go to row `at`'s session (nothing if there is none, e.g. nothing matches).
    fn jump(&mut self, at: usize, cx: &mut Context<Self>) {
        if let Some(&i) = self.rows.get(at) {
            cx.emit(SwitcherEvent::Jump(self.entries[i].id));
        }
    }

    /// Row `at`: mark, `#n`, name and its tab colour, group, and how long in its status; a
    /// waiting one's question under them.
    fn render_row(&self, at: usize, now: Instant, cx: &mut Context<Self>) -> AnyElement {
        let ui = cx.global::<Appearance>().theme.ui();
        let i = self.rows[at];
        let entry = &self.entries[i];
        let chosen = at == self.selected;
        let fill = if chosen { ui.row_active } else { ui.panel };
        let hover = ui.panel_hover;
        let waiting = entry.status == Status::Waiting;
        let age = (entry.status != Status::Idle)
            .then(|| status::age_label(now.saturating_duration_since(entry.since)));
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
            self.phase,
            None,
            px(mark_size(entry.status)),
            fill,
        );
        let line = div()
            .flex()
            .items_center()
            .child(mark_slot(mark))
            .child(
                meta(ui.text_faint)
                    .w(px(NUMBER_WIDTH))
                    .ml(px(MARK_GAP))
                    .mr(px(NUMBER_GAP))
                    .child(format!("#{}", i + 1)),
            )
            .child(
                div()
                    .min_w(px(0.0))
                    .truncate()
                    .when(chosen, |name| name.font_weight(FontWeight::MEDIUM))
                    .child(entry.label.clone()),
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
            .id(("switcher-row", entry.id))
            .flex_none()
            .flex()
            .flex_col()
            .justify_center()
            .h(px(if waiting { WAITING_ROW } else { SIDEBAR_ROW }))
            .px(px(8.0))
            .rounded(px(6.0))
            .text_size(px(text::NAME))
            .when(chosen, |row| row.bg(hsla(ui.row_active)))
            .when(!chosen, |row| row.hover(move |s| s.bg(hsla(hover))))
            .cursor_pointer()
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.jump(at, cx)))
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
            .into_any_element()
    }
}

impl Render for Switcher {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ui = cx.global::<Appearance>().theme.ui();
        let now = Instant::now();
        let rows: Vec<_> = (0..self.rows.len())
            .map(|at| self.render_row(at, now, cx))
            .collect();
        let list = if rows.is_empty() {
            div()
                .px(px(8.0))
                .py(px(4.0))
                .text_size(px(text::NAME))
                .text_color(hsla(ui.text_dim))
                .child("No session matches")
                .into_any_element()
        } else {
            div()
                .id("switcher-rows")
                .flex()
                .flex_col()
                .gap(px(ROW_GAP))
                .max_h(px(LIST_MAX_HEIGHT))
                .overflow_y_scroll()
                .track_scroll(&self.scroll)
                .children(rows)
                .into_any_element()
        };

        let empty = self.input.read(cx).text().is_empty();
        let field = overlay_panel::filter_box(&ui, &self.input, empty, PLACEHOLDER);

        overlay_panel::shell(&ui, PANEL_WIDTH)
            .key_context("Switcher")
            .on_action(cx.listener(|this, _: &SelectPrev, _, cx| this.select_by(-1, cx)))
            .on_action(cx.listener(|this, _: &SelectNext, _, cx| this.select_by(1, cx)))
            // The terminal under the panel gets neither its clicks nor its hover.
            .occlude()
            .on_mouse_down_out(
                cx.listener(|_, _: &MouseDownEvent, _, cx| cx.emit(SwitcherEvent::Cancel)),
            )
            .child(field)
            .child(list)
            .child(
                div()
                    .px(px(8.0))
                    .pt(px(5.0))
                    .border_t_1()
                    .border_color(hsla(ui.panel_border))
                    .text_size(px(text::META))
                    .text_color(hsla(ui.text_faint))
                    .child(HINT),
            )
    }
}

impl Workspace {
    /// Every workspace as the switcher shows it, in sidebar order (so its `#n` is its ⌘N): its
    /// name, group and colour, and the rest from its most urgent terminal. One of browser tabs
    /// only is idle since its first tab opened.
    fn switcher_entries(&self, cx: &App) -> Vec<Entry> {
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

    /// ⌘P: open the switcher (or, open already, give it the keyboard back).
    pub(crate) fn open_switcher(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(switcher) = &self.switcher {
            switcher.focus_handle(cx).focus(window, cx);
            return;
        }
        self.close_overlays(Overlay::Switcher);
        let (entries, phase) = (self.switcher_entries(cx), self.phase);
        let switcher = cx.new(|cx| Switcher::new(entries, phase, window, cx));
        cx.subscribe_in(&switcher, window, Self::on_switcher_event)
            .detach();
        switcher.focus_handle(cx).focus(window, cx);
        self.switcher = Some(switcher);
        cx.notify();
    }

    /// The switcher chose a workspace (its most urgent terminal becomes the active one: its
    /// pane and tab are shown; one of browser tabs only shows as it was) or was closed; either
    /// way it goes and the typing returns to the focused tab.
    fn on_switcher_event(
        &mut self,
        switcher: &Entity<Switcher>,
        event: &SwitcherEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // A switcher already closed can still report losing focus: ignore it.
        if self.switcher.as_ref() != Some(switcher) {
            return;
        }
        self.switcher = None;
        if let SwitcherEvent::Jump(id) = event {
            let ws = WorkspaceId(*id);
            match self.rollup_session(ws) {
                Some(index) => self.select(index, window, cx),
                None => self.select_workspace(ws, window, cx),
            }
        }
        self.focus_active(window, cx);
        cx.notify();
    }

    /// Give the open switcher the sessions as they are now (each time the Workspace draws).
    pub(crate) fn refresh_switcher(&self, cx: &mut Context<Self>) {
        if let Some(switcher) = &self.switcher {
            let (entries, phase) = (self.switcher_entries(cx), self.phase);
            switcher.update(cx, |switcher, _| switcher.refresh(entries, phase));
        }
    }

    /// The open switcher's rows (each one's status and since when); none while it's closed.
    pub(crate) fn switcher_shown(&self, cx: &App) -> Vec<(Status, Instant)> {
        self.switcher
            .as_ref()
            .map(|s| s.read(cx).shown().map(|e| (e.status, e.since)).collect())
            .unwrap_or_default()
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
    use super::{Entry, filter_rows, matches, next_needing_you};
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

    fn entry(id: u64, label: &str, group: Option<&str>, status: Status, secs: u64) -> Entry {
        Entry {
            id,
            label: label.into(),
            group: group.map(Into::into),
            status,
            since: Instant::now() - Duration::from_secs(secs),
            question: None,
            color: None,
        }
    }

    #[test]
    fn switcher_filters() {
        let mut claude = entry(7, "claude", Some("api"), Status::Waiting, 60);
        claude.question = Some("Do you want to make this edit to auth.rs?".into());
        let entries = [
            entry(1, "shell", None, Status::Idle, 0),               // #1
            entry(2, "codex", Some("api"), Status::Working, 5),     // #2
            claude.clone(),                                         // #3
            entry(4, "tests", Some("web"), Status::Done, 30),       // #4
            entry(5, "agent", Some("build-box"), Status::Idle, 0),  // #5
            entry(6, "Server 12", Some("web"), Status::Working, 1), // #6
        ];
        // No filter: every session, most urgent first.
        assert_eq!(filter_rows(&entries, ""), [2, 3, 1, 5, 0, 4]);
        assert_eq!(filter_rows(&entries, "  "), [2, 3, 1, 5, 0, 4]);
        // Its number, with or without "#": that session only.
        assert_eq!(filter_rows(&entries, "3"), [2]);
        assert_eq!(filter_rows(&entries, "#4"), [3]);
        // The name, any case.
        assert_eq!(filter_rows(&entries, "SERV"), [5]);
        // A group: its sessions, still by urgency.
        assert_eq!(filter_rows(&entries, "api"), [2, 1]);
        assert_eq!(filter_rows(&entries, "build"), [4]);
        // The question.
        assert_eq!(filter_rows(&entries, "auth.rs"), [2]);
        // Nothing.
        assert!(filter_rows(&entries, "zzz").is_empty());
        // `matches` on its own.
        assert!(matches(3, &claude, "edit"));
        assert!(matches(3, &claude, " #3 "));
        assert!(!matches(3, &claude, "web"));
    }
}
