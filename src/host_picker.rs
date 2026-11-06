//! M10b: the host picker (File → New SSH Workspace…, ⇧⌘S). A panel near the top of the window:
//! a box to type a host, the hosts on offer (recent first, then `~/.ssh/config`'s), and the
//! "Keep running on the host (tmux)" option. Typing filters the hosts and offers the typed
//! text as a host of its own. The config hosts' `ssh -G` runs off the UI thread as the picker
//! opens, one host after another; git-only hosts drop out as their answers arrive.
//!
//! The box is the sidebar's rename box (`NameInput`), so typing, accents and IME work the
//! same: its Enter opens the selected row, its Escape (or losing focus) closes the picker.

use std::time::Duration;

use gpui::{
    App, ClickEvent, Context, Entity, EventEmitter, FocusHandle, Focusable, MouseDownEvent,
    ScrollHandle, Subscription, Task, Window, actions, div, prelude::*, px,
};

use crate::appearance::Appearance;
use crate::name_input::{NameInput, NameInputEvent};
use crate::overlay_panel;
use crate::ssh_hosts::{self, Recent};
use crate::ssh_target::{self, Effective, SshTarget};
use crate::terminal_view::hsla;
use crate::ui_tokens::text;

// Bound in main.rs with the "HostPicker" key context; Enter and Escape are the box's own.
actions!(
    host_picker,
    [SelectPrev, SelectNext, ToggleTmux, OpenWithTmux]
);

/// At most this long for each config host's `ssh -G`.
const RESOLVE_WAIT: Duration = Duration::from_secs(2);

const PANEL_WIDTH: f32 = 480.0;
const ROW_HEIGHT: f32 = 28.0;
/// Rows shown before the list scrolls.
const VISIBLE_ROWS: f32 = 8.0;

const PLACEHOLDER: &str = "Host \u{2014} type or pick";
const HINT: &str = "Enter open \u{b7} \u{2325}Enter with tmux \u{b7} Tab toggle \u{b7} Esc close";

/// A row the picker offers: a known host (recent or from `~/.ssh/config`), or the typed text
/// as a host ("Connect to …").
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PickerRow {
    Host(SshTarget),
    Typed(String),
}

/// The rows whose host contains `query` (any case), in order. A query that is a valid host
/// and not exactly one of theirs comes last, as a host of its own; a blank query is no filter.
pub fn filter_rows(rows: &[SshTarget], query: &str) -> Vec<PickerRow> {
    let query = query.trim();
    let needle = query.to_lowercase();
    let mut shown: Vec<PickerRow> = rows
        .iter()
        .filter(|t| t.host.to_lowercase().contains(&needle))
        .cloned()
        .map(PickerRow::Host)
        .collect();
    if !query.is_empty()
        && let Ok(host) = ssh_target::validate_host(query)
        && !rows.iter().any(|t| t.host == host)
    {
        shown.push(PickerRow::Typed(host));
    }
    shown
}

/// The row `delta` steps from `current` in a list of `len`, wrapping round both ends.
pub fn move_selection(current: usize, len: usize, delta: i32) -> usize {
    if len == 0 {
        return 0;
    }
    (current as i64 + i64::from(delta)).rem_euclid(len as i64) as usize
}

/// Whether the option shows ticked: as the user set it (`chosen`), else as the row was last
/// opened (a recent host carries its own choice), else off.
pub fn tmux_for(row: Option<&PickerRow>, chosen: Option<bool>) -> bool {
    chosen.unwrap_or(matches!(row, Some(PickerRow::Host(t)) if t.tmux))
}

/// What opening `row` connects to: its host, with tmux if the option says so or `with_tmux`
/// (⌥Enter). The session makes its own tmux name.
pub fn chosen_target(row: &PickerRow, chosen: Option<bool>, with_tmux: bool) -> SshTarget {
    let host = match row {
        PickerRow::Host(t) => t.host.clone(),
        PickerRow::Typed(host) => host.clone(),
    };
    SshTarget {
        host,
        tmux: with_tmux || tmux_for(Some(row), chosen),
        tmux_name: None,
    }
}

pub enum HostPickerEvent {
    /// Open an SSH session to this host.
    Open(SshTarget),
    /// Escape, a click outside, or the box lost focus.
    Cancel,
}

pub struct HostPicker {
    input: Entity<NameInput>,
    /// The box's text as last filtered.
    query: String,
    recent: Vec<Recent>,
    /// The config hosts, each with its `ssh -G` answer once it has one.
    config: Vec<(String, Option<Effective>)>,
    rows: Vec<PickerRow>,
    selected: usize,
    /// The option as the user set it (Tab, or a click); none: it follows the selected row.
    tmux: Option<bool>,
    scroll: ScrollHandle,
    _input_events: Subscription,
    _input_changes: Subscription,
    /// Dropped with the picker, which stops the `ssh -G` runs still to come.
    _resolving: Task<()>,
}

impl EventEmitter<HostPickerEvent> for HostPicker {}

impl Focusable for HostPicker {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.input.focus_handle(cx)
    }
}

impl HostPicker {
    /// `recent`: the recent hosts, newest first; `config_hosts`: the hosts in `~/.ssh/config`.
    pub fn new(
        recent: Vec<Recent>,
        config_hosts: Vec<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let input = cx.new(|cx| NameInput::new(String::new(), window, cx));
        let input_events = cx.subscribe_in(&input, window, |this, _, event, _, cx| match event {
            NameInputEvent::Confirm(_) => this.open(this.selected, false, cx),
            NameInputEvent::Cancel => cx.emit(HostPickerEvent::Cancel),
        });
        let input_changes = cx.observe(&input, |this, _, cx| this.query_changed(cx));
        let resolving = Self::resolve_hosts(config_hosts.clone(), cx);
        let mut this = Self {
            input,
            query: String::new(),
            recent,
            config: config_hosts.into_iter().map(|h| (h, None)).collect(),
            rows: Vec::new(),
            selected: 0,
            tmux: None,
            scroll: ScrollHandle::new(),
            _input_events: input_events,
            _input_changes: input_changes,
            _resolving: resolving,
        };
        this.refilter(true);
        this
    }

    /// Run `ssh -G` for each config host in turn, off the UI thread, updating the rows as each
    /// answers. Stops when the picker is gone.
    fn resolve_hosts(hosts: Vec<String>, cx: &mut Context<Self>) -> Task<()> {
        cx.spawn(async move |this, cx| {
            for host in hosts {
                let asked = host.clone();
                let effective = cx
                    .background_spawn(async move { ssh_hosts::resolve(&asked, RESOLVE_WAIT) })
                    .await;
                let shown = this.update(cx, |this, cx| {
                    if let Some(entry) = this.config.iter_mut().find(|(h, _)| *h == host) {
                        entry.1 = effective;
                    }
                    this.refilter(false);
                    cx.notify();
                });
                if shown.is_err() {
                    break;
                }
            }
        })
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

    /// Rebuild the rows. `from_top`: select the first; else keep the selected row if it is
    /// still there (an `ssh -G` answer can take one away).
    fn refilter(&mut self, from_top: bool) {
        let before = self.rows.get(self.selected).cloned();
        let hosts = ssh_hosts::picker_rows(&self.recent, &self.config);
        self.rows = filter_rows(&hosts, &self.query);
        let last = self.rows.len().saturating_sub(1);
        self.selected = match before {
            Some(row) if !from_top => self
                .rows
                .iter()
                .position(|r| *r == row)
                .unwrap_or(self.selected.min(last)),
            _ => 0,
        };
        self.scroll.scroll_to_item(self.selected);
    }

    fn select_by(&mut self, delta: i32, cx: &mut Context<Self>) {
        self.selected = move_selection(self.selected, self.rows.len(), delta);
        self.scroll.scroll_to_item(self.selected);
        cx.notify();
    }

    fn toggle_tmux(&mut self, cx: &mut Context<Self>) {
        self.tmux = Some(!tmux_for(self.rows.get(self.selected), self.tmux));
        cx.notify();
    }

    /// Open row `index` (nothing if there is none, e.g. an empty list).
    fn open(&mut self, index: usize, with_tmux: bool, cx: &mut Context<Self>) {
        if let Some(row) = self.rows.get(index) {
            cx.emit(HostPickerEvent::Open(chosen_target(
                row, self.tmux, with_tmux,
            )));
        }
    }

    fn is_recent(&self, host: &str) -> bool {
        self.recent.iter().any(|r| r.target.host == host)
    }

    /// Why the list is empty: the typed text isn't a host, or there is nothing yet.
    fn empty_line(&self) -> String {
        match ssh_target::validate_host(&self.query) {
            Err(why) if !self.query.trim().is_empty() => why,
            _ => "Type a host: user@host, or an alias from ~/.ssh/config".to_string(),
        }
    }
}

impl Render for HostPicker {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ui = cx.global::<Appearance>().theme.ui();
        let rows: Vec<_> = self
            .rows
            .iter()
            .enumerate()
            .map(|(i, row)| {
                let (label, tag) = match row {
                    PickerRow::Host(t) => {
                        let tag = match (self.is_recent(&t.host), t.tmux) {
                            (true, true) => Some("recent \u{b7} tmux"),
                            (true, false) => Some("recent"),
                            (false, _) => None,
                        };
                        (t.host.clone(), tag)
                    }
                    PickerRow::Typed(host) => (format!("Connect to {host}"), None),
                };
                div()
                    .id(("host-row", i))
                    .flex_none()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .h(px(ROW_HEIGHT))
                    .px_2()
                    .rounded(px(6.0))
                    .when(i == self.selected, |r| r.bg(hsla(ui.row_active)))
                    .hover(|s| s.bg(hsla(ui.panel_hover)))
                    .cursor_pointer()
                    .on_click(
                        cx.listener(move |this, _: &ClickEvent, _, cx| this.open(i, false, cx)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child(label),
                    )
                    .when_some(tag, |r, tag| {
                        r.child(
                            div()
                                .text_size(px(text::META))
                                .text_color(hsla(ui.text_dim))
                                .child(tag),
                        )
                    })
            })
            .collect();
        let list = if rows.is_empty() {
            div()
                .px_2()
                .py_1()
                .text_color(hsla(ui.text_dim))
                .child(self.empty_line())
                .into_any_element()
        } else {
            div()
                .id("host-rows")
                .flex()
                .flex_col()
                .max_h(px(ROW_HEIGHT * VISIBLE_ROWS))
                .overflow_y_scroll()
                .track_scroll(&self.scroll)
                .children(rows)
                .into_any_element()
        };

        let empty = self.input.read(cx).text().is_empty();
        let field = overlay_panel::filter_box(&ui, &self.input, empty, PLACEHOLDER);

        let tmux = tmux_for(self.rows.get(self.selected), self.tmux);
        let checkbox = div()
            .flex()
            .items_center()
            .justify_center()
            .flex_none()
            .size(px(14.0))
            .rounded_sm()
            .border_1()
            .border_color(hsla(if tmux { ui.text } else { ui.text_dim }))
            .text_size(px(text::META))
            .when(tmux, |b| b.child("\u{2713}"));
        let option = div()
            .id("tmux-option")
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .px_2()
            .py_1()
            .rounded(px(6.0))
            .hover(|s| s.bg(hsla(ui.panel_hover)))
            .cursor_pointer()
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_tmux(cx)))
            .child(checkbox)
            .child("Keep running on the host (tmux)");

        overlay_panel::shell(&ui, PANEL_WIDTH)
            .key_context("HostPicker")
            .on_action(cx.listener(|this, _: &SelectPrev, _, cx| this.select_by(-1, cx)))
            .on_action(cx.listener(|this, _: &SelectNext, _, cx| this.select_by(1, cx)))
            .on_action(cx.listener(|this, _: &ToggleTmux, _, cx| this.toggle_tmux(cx)))
            .on_action(
                cx.listener(|this, _: &OpenWithTmux, _, cx| this.open(this.selected, true, cx)),
            )
            // The terminal under the panel gets neither its clicks nor its hover.
            .occlude()
            .on_mouse_down_out(
                cx.listener(|_, _: &MouseDownEvent, _, cx| cx.emit(HostPickerEvent::Cancel)),
            )
            .child(field)
            .child(list)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .pt_1()
                    .border_t_1()
                    .border_color(hsla(ui.panel_border))
                    .child(option)
                    .child(
                        div()
                            .px_2()
                            .text_size(px(text::META))
                            .text_color(hsla(ui.text_dim))
                            .child(HINT),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::{PickerRow, chosen_target, filter_rows, move_selection, tmux_for};
    use crate::ssh_target::SshTarget;

    fn host(name: &str, tmux: bool) -> SshTarget {
        SshTarget {
            host: name.to_string(),
            tmux,
            tmux_name: None,
        }
    }

    #[test]
    fn filter_rows_matches_and_offers_typed_host() {
        let rows = [
            host("build-box", true),
            host("Prod-Web", false),
            host("dev", false),
        ];
        // No query: every row, in order, and nothing typed.
        assert_eq!(
            filter_rows(&rows, ""),
            rows.iter()
                .cloned()
                .map(PickerRow::Host)
                .collect::<Vec<_>>()
        );
        // Case-insensitive substring; the typed host comes last.
        assert_eq!(
            filter_rows(&rows, "WEB"),
            [
                PickerRow::Host(host("Prod-Web", false)),
                PickerRow::Typed("WEB".to_string()),
            ]
        );
        // Exactly a row's host: no typed row.
        assert_eq!(
            filter_rows(&rows, "dev"),
            [PickerRow::Host(host("dev", false))]
        );
        // Nothing matches: only the typed host, trimmed.
        assert_eq!(
            filter_rows(&rows, " me@10.0.0.7 "),
            [PickerRow::Typed("me@10.0.0.7".to_string())]
        );
    }

    #[test]
    fn filter_rows_never_offers_an_invalid_host() {
        let rows = [host("dev", false)];
        assert!(filter_rows(&rows, "-oProxyCommand=x").is_empty());
        assert!(filter_rows(&rows, "a b").is_empty());
        assert!(filter_rows(&rows, "   ").len() == 1, "blank is no query");
        assert_eq!(
            filter_rows(&rows, "de\tv"),
            Vec::<PickerRow>::new(),
            "a control character"
        );
    }

    #[test]
    fn selection_wraps() {
        assert_eq!(move_selection(0, 3, 1), 1);
        assert_eq!(move_selection(2, 3, 1), 0);
        assert_eq!(move_selection(0, 3, -1), 2);
        assert_eq!(move_selection(1, 3, -1), 0);
        assert_eq!(
            move_selection(5, 3, 0),
            2,
            "a stale index is brought back in range"
        );
        assert_eq!(move_selection(0, 0, 1), 0);
        assert_eq!(move_selection(0, 0, -1), 0);
    }

    #[test]
    fn a_recent_row_s_tmux_choice_yields_to_the_option() {
        let recent = PickerRow::Host(host("box", true));
        let plain = PickerRow::Host(host("dev", false));
        let typed = PickerRow::Typed("me@new".to_string());
        // Untouched, the option follows the row.
        assert!(tmux_for(Some(&recent), None));
        assert!(!tmux_for(Some(&plain), None));
        assert!(!tmux_for(Some(&typed), None));
        assert!(!tmux_for(None, None));
        // Set by the user, it wins.
        assert!(!tmux_for(Some(&recent), Some(false)));
        assert!(tmux_for(Some(&typed), Some(true)));
        // ⌥Enter always asks for tmux; the session makes its own tmux name.
        assert_eq!(
            chosen_target(&recent, Some(false), false),
            host("box", false)
        );
        assert_eq!(chosen_target(&plain, None, true), host("dev", true));
        assert_eq!(chosen_target(&typed, None, false), host("me@new", false));
    }
}
