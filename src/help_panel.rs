//! M11d: the Help panel, a panel near the top of the window like the command palette, on one of
//! two pages. Keyboard Shortcuts (⌘/, or Help → Keyboard Shortcuts) lists every shortcut the
//! key-binding table has, by group, each name with its keys on the right; the fuxx Command
//! Reference (Help → fuxx Command Reference) lists `fuxx help`'s commands, each usage in the
//! terminal's font with what it does under it, then the usage text's closing notes.
//!
//! Read only: the box filters the list, ↑/↓ scroll it a row, and Enter does nothing. The box is
//! the sidebar's rename box (`NameInput`), as in the palette: its Escape (or losing focus) and
//! a click outside close the panel.

use std::path::{Path, PathBuf};

use gpui::{
    AnyElement, App, Context, Entity, EventEmitter, FocusHandle, Focusable, MouseDownEvent, Point,
    ScrollHandle, SharedString, Subscription, Window, actions, div, prelude::*, px,
};

use crate::appearance::Appearance;
use crate::help_commands::{CommandEntry, Reference, entry_matches, reference};
use crate::help_shortcuts::{ShortcutRow, binding_pairs, row_matches, shortcut_rows};
use crate::name_input::{NameInput, NameInputEvent};
use crate::overlay_panel;
use crate::switcher::{LIST_MAX_HEIGHT, ROW, ROW_GAP};
use crate::terminal_view::hsla;
use crate::theme::UiColors;
use crate::ui_tokens::text;
use crate::workspace::{Overlay, Workspace};

// Bound in main.rs with the "HelpPanel" key context; Escape is the box's own.
actions!(help_panel, [ScrollUp, ScrollDown]);

/// Wider than the old switcher: a shortcut row has its name on the left and its keys on the right.
const PANEL_WIDTH: f32 = 560.0;
/// The room between a shortcut's name and its keys.
const KEYS_GAP: f32 = 12.0;
/// A command's usage, in the terminal's font: a point under the names, as mono text runs wide.
const USAGE_SIZE: f32 = text::NAME - 1.0;
const HINT: &str = "Esc closes";

/// A group's heading: faint, small, with room above it unless it is the first line (the
/// command palette's headings too).
pub(crate) fn render_group(
    label: impl Into<SharedString>,
    first: bool,
    ui: &UiColors,
) -> AnyElement {
    div()
        .flex_none()
        .px(px(8.0))
        .pt(px(if first { 2.0 } else { 8.0 }))
        .pb(px(2.0))
        .text_size(px(text::META))
        .text_color(hsla(ui.text_faint))
        .child(label.into())
        .into_any_element()
}

/// Which list the panel shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HelpPage {
    Shortcuts,
    Commands,
}

impl HelpPage {
    fn placeholder(self) -> &'static str {
        match self {
            HelpPage::Shortcuts => "Search shortcuts",
            HelpPage::Commands => "Search commands",
        }
    }

    /// What the list says when the box's text finds nothing.
    fn nothing_found(self) -> &'static str {
        match self {
            HelpPage::Shortcuts => "No shortcut matches",
            HelpPage::Commands => "No command matches",
        }
    }
}

pub enum HelpPanelEvent {
    /// Escape, a click outside, or the box lost focus.
    Close,
}

/// The page's rows, built once as the panel opens.
enum Content {
    Shortcuts(Vec<ShortcutRow>),
    Commands(Reference),
}

/// One line of the list as shown: a group's heading, or a row under it.
enum Line<'a> {
    Group(&'static str),
    Shortcut(&'a ShortcutRow),
    Command(&'a CommandEntry),
}

pub struct HelpPanel {
    page: HelpPage,
    content: Content,
    input: Entity<NameInput>,
    /// The box's text as last filtered.
    query: String,
    scroll: ScrollHandle,
    _input_events: Subscription,
    _input_changes: Subscription,
}

impl EventEmitter<HelpPanelEvent> for HelpPanel {}

impl Focusable for HelpPanel {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.input.focus_handle(cx)
    }
}

/// Keyboard Shortcuts' rows: from fuxx's whole key-binding table, ⌘N and ⌘T (`new_bindings`)
/// first and ⌥⌘Q (`persistent_bindings`) last with persistent sessions on, as `main` binds them.
pub(crate) fn shortcut_list(persistent: bool) -> Vec<ShortcutRow> {
    let mut bindings = crate::new_bindings().to_vec();
    bindings.extend(crate::key_bindings());
    if persistent {
        bindings.extend(crate::persistent_bindings());
    }
    shortcut_rows(&binding_pairs(&bindings))
}

/// What a Help request for a page does to the panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HelpStep {
    /// Show the page (opened, or in place of the other one).
    Open,
    /// Close the panel.
    Close,
    /// Leave it showing that page, its box with the keyboard.
    Keep,
}

/// What asking for `asked` does, with `shown` the page the panel shows (none: closed) and
/// `window_was_key` whether the main window was already the key window. The shown page asked
/// for again closes (⌘/ twice), but only there: from Preferences or a popped-out page, ⌘/ means
/// "show me the shortcuts", so the panel stays.
pub fn help_step(shown: Option<HelpPage>, asked: HelpPage, window_was_key: bool) -> HelpStep {
    match (shown == Some(asked), window_was_key) {
        (false, _) => HelpStep::Open,
        (true, true) => HelpStep::Close,
        (true, false) => HelpStep::Keep,
    }
}

/// The running fuxx's path, for the MCP setup row (as `fuxx` when it can't be found).
fn this_exe() -> PathBuf {
    std::env::current_exe().unwrap_or_else(|_| Path::new("fuxx").to_path_buf())
}

impl HelpPanel {
    /// The panel on `page`, its box focused. `persistent`: persistent sessions are on, so ⌥⌘Q
    /// is bound (and listed).
    pub fn new(
        page: HelpPage,
        persistent: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let input = cx.new(|cx| NameInput::new(String::new(), window, cx));
        let input_events = cx.subscribe_in(&input, window, |_, _, event, _, cx| match event {
            // Read only: Enter does nothing.
            NameInputEvent::Confirm(_) => {}
            NameInputEvent::Cancel => cx.emit(HelpPanelEvent::Close),
        });
        let input_changes = cx.observe(&input, |this, _, cx| this.query_changed(cx));
        input.focus_handle(cx).focus(window, cx);
        let content = match page {
            HelpPage::Shortcuts => Content::Shortcuts(shortcut_list(persistent)),
            HelpPage::Commands => Content::Commands(reference(&this_exe())),
        };
        Self {
            page,
            content,
            input,
            query: String::new(),
            scroll: ScrollHandle::new(),
            _input_events: input_events,
            _input_changes: input_changes,
        }
    }

    pub fn page(&self) -> HelpPage {
        self.page
    }

    /// How many rows the box's text leaves (group headings not counted), for the shots.
    #[cfg_attr(not(feature = "ui-shots"), allow(dead_code))]
    pub fn shown_rows(&self) -> usize {
        self.lines()
            .iter()
            .filter(|line| !matches!(line, Line::Group(_)))
            .count()
    }

    /// How far the list is scrolled (0 at the top, negative below it), for the shots.
    #[cfg(feature = "ui-shots")]
    pub fn scroll_offset(&self) -> gpui::Pixels {
        self.scroll.offset().y
    }

    /// The rows the box's text finds, each group's heading over its first one.
    fn lines(&self) -> Vec<Line<'_>> {
        let query = self.query.trim();
        // Each row found, with its group's heading (the rows come in group order).
        let found: Vec<(&'static str, Line<'_>)> = match &self.content {
            Content::Shortcuts(rows) => rows
                .iter()
                .filter(|r| row_matches(r, query))
                .map(|r| (r.group.label(), Line::Shortcut(r)))
                .collect(),
            Content::Commands(reference) => reference
                .entries
                .iter()
                .filter(|e| entry_matches(e, query))
                .map(|e| (e.group, Line::Command(e)))
                .collect(),
        };
        let mut lines = Vec::new();
        let mut group = None;
        for (label, line) in found {
            if group != Some(label) {
                group = Some(label);
                lines.push(Line::Group(label));
            }
            lines.push(line);
        }
        lines
    }

    /// The box changed (typing, or the IME's composition): filter again if its text did, from
    /// the top of the list.
    fn query_changed(&mut self, cx: &mut Context<Self>) {
        let query = self.input.read(cx).text().to_string();
        if query != self.query {
            self.query = query;
            self.scroll.set_offset(Point::default());
            cx.notify();
        }
    }

    /// ↑/↓: the line above or below the top one to the top of the list. ↑ with the top line cut
    /// off (scrolled by the mouse) brings that line in whole first. Nothing while nothing is
    /// found: the handle still counts the rows last drawn, and a scroll queued now would win
    /// over the return to the top once the box is cleared.
    fn scroll_by(&mut self, delta: i32, cx: &mut Context<Self>) {
        let count = self.scroll.children_count();
        if count == 0 || self.lines().is_empty() {
            return;
        }
        let (top, cut) = self.scroll.logical_scroll_top();
        let to = if delta > 0 {
            (top + 1).min(count - 1)
        } else if cut < px(0.0) {
            top
        } else {
            top.saturating_sub(1)
        };
        self.scroll.scroll_to_top_of_item(to);
        cx.notify();
    }

    /// A shortcut: what it does on the left, its keys on the right, dimmer.
    fn render_shortcut(row: &ShortcutRow, ui: &UiColors) -> AnyElement {
        div()
            .flex_none()
            .flex()
            .items_center()
            .h(px(ROW))
            .px(px(8.0))
            .text_size(px(text::NAME))
            .child(div().flex_1().min_w(px(0.0)).truncate().child(row.name))
            .child(
                div()
                    .flex_none()
                    .ml(px(KEYS_GAP))
                    .text_color(hsla(ui.text_dim))
                    .child(row.keys.clone()),
            )
            .into_any_element()
    }

    /// A command: its usage line(s) in the terminal's font, what it does under them, dimmer.
    fn render_command(entry: &CommandEntry, ui: &UiColors, mono: gpui::SharedString) -> AnyElement {
        div()
            .flex_none()
            .flex()
            .flex_col()
            .px(px(8.0))
            .py(px(4.0))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .font_family(mono)
                    .text_size(px(USAGE_SIZE))
                    .children(entry.usage.iter().map(|line| div().child(line.clone()))),
            )
            .child(
                div()
                    .mt(px(2.0))
                    .text_size(px(text::META))
                    .text_color(hsla(ui.text_dim))
                    .child(entry.description.clone()),
            )
            .into_any_element()
    }
}

impl Render for HelpPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let appearance = cx.global::<Appearance>();
        let ui = appearance.theme.ui();
        let mono = appearance.font.family.clone();
        let lines: Vec<AnyElement> = self
            .lines()
            .iter()
            .enumerate()
            .map(|(at, line)| match line {
                Line::Group(label) => render_group(*label, at == 0, &ui),
                Line::Shortcut(row) => Self::render_shortcut(row, &ui),
                Line::Command(entry) => Self::render_command(entry, &ui, mono.clone()),
            })
            .collect();
        let list = if lines.is_empty() {
            div()
                .px(px(8.0))
                .py(px(4.0))
                .text_size(px(text::NAME))
                .text_color(hsla(ui.text_dim))
                .child(self.page.nothing_found())
                .into_any_element()
        } else {
            div()
                .id("help-rows")
                .flex()
                .flex_col()
                .gap(px(ROW_GAP))
                .max_h(px(LIST_MAX_HEIGHT))
                .overflow_y_scroll()
                .track_scroll(&self.scroll)
                .children(lines)
                .into_any_element()
        };
        // The usage text's closing notes (`--json`, what TARGET is, the exit codes).
        let footer = match &self.content {
            Content::Commands(reference) => reference.footer.clone(),
            Content::Shortcuts(_) => Vec::new(),
        };

        let empty = self.input.read(cx).text().is_empty();
        let field = overlay_panel::filter_box(&ui, &self.input, empty, self.page.placeholder());

        overlay_panel::shell(&ui, PANEL_WIDTH)
            .key_context("HelpPanel")
            .on_action(cx.listener(|this, _: &ScrollUp, _, cx| this.scroll_by(-1, cx)))
            .on_action(cx.listener(|this, _: &ScrollDown, _, cx| this.scroll_by(1, cx)))
            // The terminal under the panel gets neither its clicks nor its hover.
            .occlude()
            .on_mouse_down_out(
                cx.listener(|_, _: &MouseDownEvent, _, cx| cx.emit(HelpPanelEvent::Close)),
            )
            .child(field)
            .child(list)
            .when(!footer.is_empty(), |panel| {
                panel.child(
                    div()
                        .flex()
                        .flex_col()
                        .px(px(8.0))
                        .pt(px(5.0))
                        .border_t_1()
                        .border_color(hsla(ui.panel_border))
                        .text_size(px(text::META))
                        .text_color(hsla(ui.text_faint))
                        .children(footer.into_iter().map(|line| div().child(line))),
                )
            })
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
    /// ⌘/ or a Help item: the Help panel on `page` (`help_step`). Open on that page already, it
    /// closes when this window was key (⌘/ again) and stays otherwise, its box given the
    /// keyboard; open on the other, the new page takes its place. Any other box over the window
    /// closes first.
    pub(crate) fn open_help(
        &mut self,
        page: HelpPage,
        window_was_key: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let shown = self.help.as_ref().map(|h| h.read(cx).page());
        match help_step(shown, page, window_was_key) {
            HelpStep::Close => {
                self.help = None;
                self.focus_active(window, cx);
                cx.notify();
                return;
            }
            HelpStep::Keep => {
                if let Some(help) = &self.help {
                    help.focus_handle(cx).focus(window, cx);
                }
                return;
            }
            HelpStep::Open => {}
        }
        self.close_overlays(Overlay::Help);
        let persistent = self.persistent;
        let help = cx.new(|cx| HelpPanel::new(page, persistent, window, cx));
        cx.subscribe_in(&help, window, Self::on_help_event).detach();
        self.help = Some(help);
        cx.notify();
    }

    /// The panel was closed: it goes and the typing returns to the focused tab.
    fn on_help_event(
        &mut self,
        help: &Entity<HelpPanel>,
        event: &HelpPanelEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // A panel already closed or replaced can still report losing focus: ignore it.
        if self.help.as_ref() != Some(help) {
            return;
        }
        let HelpPanelEvent::Close = event;
        self.help = None;
        self.focus_active(window, cx);
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::{HelpPage, HelpStep, help_step, shortcut_list};
    use crate::help_shortcuts::ShortcutGroup;

    /// (name, keys) of each row in `group`, in order, with persistent sessions off.
    fn rows_of(group: ShortcutGroup) -> Vec<(&'static str, String)> {
        rows_of_with(group, false)
    }

    fn rows_of_with(group: ShortcutGroup, persistent: bool) -> Vec<(&'static str, String)> {
        shortcut_list(persistent)
            .into_iter()
            .filter(|r| r.group == group)
            .map(|r| (r.name, r.keys))
            .collect()
    }

    #[test]
    fn the_list_starts_with_new_workspace_and_new_tab() {
        let workspaces = rows_of(ShortcutGroup::Workspaces);
        assert_eq!(workspaces[0], ("New Workspace", "\u{2318}N".to_string()));
        let tabs = rows_of(ShortcutGroup::Tabs);
        assert_eq!(tabs[0], ("New Tab", "\u{2318}T".to_string()));
    }

    #[test]
    fn command_slash_is_listed_under_window_and_app() {
        assert!(
            rows_of(ShortcutGroup::WindowAndApp)
                .contains(&("Keyboard Shortcuts", "\u{2318}/".to_string()))
        );
    }

    #[test]
    fn quit_and_end_all_sessions_is_listed_only_while_it_is_bound() {
        let row = (
            "Quit fuxx and End All Sessions",
            "\u{2325}\u{2318}Q".to_string(),
        );
        assert!(rows_of_with(ShortcutGroup::WindowAndApp, true).contains(&row));
        assert!(!rows_of_with(ShortcutGroup::WindowAndApp, false).contains(&row));
    }

    #[test]
    fn asking_again_closes_only_from_the_key_main_window() {
        use HelpPage::{Commands, Shortcuts};
        // The main window was key: the shown page toggles closed, another one replaces it.
        assert_eq!(help_step(None, Shortcuts, true), HelpStep::Open);
        assert_eq!(help_step(Some(Shortcuts), Shortcuts, true), HelpStep::Close);
        assert_eq!(help_step(Some(Commands), Shortcuts, true), HelpStep::Open);
        assert_eq!(help_step(Some(Commands), Commands, true), HelpStep::Close);
        // From Preferences or a popped-out page: shown, never closed.
        assert_eq!(help_step(None, Shortcuts, false), HelpStep::Open);
        assert_eq!(help_step(Some(Shortcuts), Shortcuts, false), HelpStep::Keep);
        assert_eq!(help_step(Some(Commands), Shortcuts, false), HelpStep::Open);
        assert_eq!(help_step(Some(Commands), Commands, false), HelpStep::Keep);
    }
}
