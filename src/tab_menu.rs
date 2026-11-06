//! Sidebar rows as tabs (M11; M11b: a row is a workspace): a row's right-click menu (Close
//! Workspace, Close Other Workspaces, Close Workspaces Below, Show All Workspaces, Rename
//! Workspace…, Workspace Color) and the closes it starts. Closing one workspace ends all its
//! terminals and asks first when it has two or more (`asks_to_close`); closing several always
//! asks, in a native sheet.
//!
//! `closes`, `enabled`, `asks_to_close`, `confirm_text` and `confirm_workspace_text` are the pure
//! model; the rest opens, draws and acts on the menu. It is one of the overlays (one open at a time): it opens at the pointer, holds the
//! keyboard so Escape closes it, and a click anywhere else (or any choice) closes it too.

use gpui::{
    AnyElement, ClickEvent, Context, FocusHandle, MouseButton, MouseDownEvent, PathBuilder, Pixels,
    Point, PromptLevel, Window, actions, anchored, canvas, div, point, prelude::*, px,
};

use crate::layout_model::WorkspaceId;
use crate::overlay_panel;
use crate::status::Status;
use crate::tab_color::TabColor;
use crate::terminal_view::hsla;
use crate::theme::UiColors;
use crate::ui_tokens::text;
use crate::workspace::{Overlay, Workspace};

// Bound in main.rs with the "TabMenu" key context.
actions!(tab_menu, [Dismiss]);

/// The menu's width, an item's height, and the colour swatches (and the cell each sits in,
/// which rings the current one).
const MENU_WIDTH: f32 = 224.0;
const ITEM: f32 = 26.0;
const SWATCH: f32 = 16.0;
const SWATCH_CELL: f32 = 24.0;
const SWATCH_GAP: f32 = 6.0;
/// How far from the window's edges the menu keeps.
const EDGE_MARGIN: f32 = 8.0;

/// An open row menu: whose row (its workspace), where it opened, and the focus it holds.
pub(crate) struct TabMenu {
    pub workspace: WorkspaceId,
    at: Point<Pixels>,
    focus: FocusHandle,
}

/// What the menu can do to its workspace.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabAction {
    Close,
    CloseOthers,
    CloseToRight,
    ShowAll,
    Rename,
    Color(Option<TabColor>),
}

/// The workspaces (by place in the sidebar's `len`) that `action` from workspace `at`'s menu
/// closes: itself; every other one; or the rows shown below it (`shown`: the workspaces the
/// sidebar has a row for, in order; those folded in a closed group have none and stay). None for
/// the actions that close nothing.
pub fn closes(action: TabAction, shown: &[usize], len: usize, at: usize) -> Vec<usize> {
    match action {
        TabAction::Close => vec![at],
        TabAction::CloseOthers => (0..len).filter(|&i| i != at).collect(),
        TabAction::CloseToRight => match shown.iter().position(|&i| i == at) {
            Some(row) => shown[row + 1..].to_vec(),
            None => Vec::new(),
        },
        TabAction::ShowAll | TabAction::Rename | TabAction::Color(_) => Vec::new(),
    }
}

/// Whether `action` can be chosen from workspace `at`'s menu (of `len`, `shown` as in `closes`):
/// Close Other Workspaces needs another workspace, Close Workspaces Below a row below it.
pub fn enabled(action: TabAction, shown: &[usize], len: usize, at: usize) -> bool {
    match action {
        TabAction::CloseOthers | TabAction::CloseToRight => {
            !closes(action, shown, len, at).is_empty()
        }
        _ => true,
    }
}

/// Whether closing one workspace of `terminals` terminals asks first: with two or more (a
/// single tab or pane closes at once, as ⌘W does).
pub fn asks_to_close(terminals: usize) -> bool {
    terminals >= 2
}

/// The confirm sheet for closing `count` workspaces, `busy` of their terminals with an agent
/// still working or waiting: its message, and a line about the busy ones when there are any.
pub fn confirm_text(count: usize, busy: usize) -> (String, Option<String>) {
    let message = format!(
        "Close {count} {}?",
        plural(count, "workspace", "workspaces")
    );
    (message, busy_line(busy))
}

/// The confirm sheet for closing one workspace of `terminals` terminals, `busy` as in
/// `confirm_text`.
pub fn confirm_workspace_text(terminals: usize, busy: usize) -> (String, Option<String>) {
    let message = format!(
        "Close workspace with {terminals} {}?",
        plural(terminals, "terminal", "terminals")
    );
    (message, busy_line(busy))
}

fn plural(n: usize, one: &str, many: &str) -> String {
    if n == 1 { one } else { many }.to_string()
}

/// The sheet's line about the busy agents, when there are any.
pub(crate) fn busy_line(busy: usize) -> Option<String> {
    (busy > 0).then(|| {
        let who = plural(busy, "agent is", "agents are");
        format!("{busy} {who} still working or waiting.")
    })
}

impl Workspace {
    /// Right-click on a workspace's row: its menu opens at `at` (the pointer) and takes the
    /// keyboard.
    pub(crate) fn open_tab_menu(
        &mut self,
        workspace: WorkspaceId,
        at: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_overlays(Overlay::TabMenu);
        self.sidebar.new_menu = false;
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        self.sidebar.tab_menu = Some(TabMenu {
            workspace,
            at,
            focus,
        });
        cx.notify();
    }

    /// Escape or a click outside: the menu goes and the typing returns to the terminal.
    fn dismiss_tab_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.sidebar.tab_menu.take().is_some() {
            self.focus_active(window, cx);
            cx.notify();
        }
    }

    /// A choice from the menu: it closes, then does `action` to its workspace.
    fn choose(&mut self, action: TabAction, window: &mut Window, cx: &mut Context<Self>) {
        let Some(menu) = self.sidebar.tab_menu.take() else {
            return;
        };
        cx.notify();
        let ws = menu.workspace;
        let order = self.model.display_order();
        let Some(at) = order.iter().position(|&w| w == ws) else {
            self.focus_active(window, cx);
            return;
        };
        match action {
            TabAction::Close => {
                self.focus_active(window, cx);
                self.close_workspace(ws, window, cx);
            }
            TabAction::CloseOthers | TabAction::CloseToRight => {
                self.focus_active(window, cx);
                let shown = self.workspaces_with_rows();
                let chosen = closes(action, &shown, order.len(), at)
                    .into_iter()
                    .filter_map(|i| order.get(i).copied())
                    .collect();
                self.confirm_close(chosen, window, cx);
            }
            TabAction::ShowAll => self.open_switcher(window, cx),
            TabAction::Rename => self.start_rename(ws, window, cx),
            TabAction::Color(color) => {
                self.model.set_color(ws, color);
                self.save();
                self.focus_active(window, cx);
            }
        }
    }

    /// Close a workspace (its ×, a middle-click, its menu's Close): every terminal in it ends.
    /// With two or more it asks first, in a native sheet; Cancel keeps it.
    pub(crate) fn close_workspace(
        &mut self,
        ws: WorkspaceId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let terminals = self.model.terminals(ws);
        if !asks_to_close(terminals.len()) {
            self.end_terminals(&terminals, window, cx);
            return;
        }
        let (message, detail) = confirm_workspace_text(terminals.len(), self.busy(&terminals));
        self.ask_then_end(message, detail, terminals, window, cx);
    }

    /// Close the workspaces `chosen`, once a native sheet has asked (always, for a bulk close
    /// from the menu). Cancel keeps them all.
    fn confirm_close(
        &mut self,
        chosen: Vec<WorkspaceId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if chosen.is_empty() {
            return;
        }
        let terminals: Vec<u64> = chosen
            .iter()
            .flat_map(|&ws| self.model.terminals(ws))
            .collect();
        let (message, detail) = confirm_text(chosen.len(), self.busy(&terminals));
        self.ask_then_end(message, detail, terminals, window, cx);
    }

    /// How many of `terminals` have an agent still working or waiting.
    fn busy(&self, terminals: &[u64]) -> usize {
        self.sessions
            .items()
            .iter()
            .filter(|s| terminals.contains(&s.id))
            .filter(|s| matches!(s.status, Status::Working | Status::Waiting))
            .count()
    }

    /// Ask (`message`, `detail`) in a native sheet; on Close, end `terminals`. By id, so the
    /// ones ended are the ones asked about, whatever moves meanwhile.
    fn ask_then_end(
        &mut self,
        message: String,
        detail: Option<String>,
        terminals: Vec<u64>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let answer = window.prompt(
            PromptLevel::Warning,
            &message,
            detail.as_deref(),
            &["Close", "Cancel"],
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            if answer.await != Ok(0) {
                return;
            }
            this.update_in(cx, |this, window, cx| {
                this.end_terminals(&terminals, window, cx)
            })
            .ok();
        })
        .detach();
    }

    /// End each of `terminals` as ⌘W would (those already gone are skipped).
    fn end_terminals(&mut self, terminals: &[u64], window: &mut Window, cx: &mut Context<Self>) {
        for &id in terminals {
            if let Some(i) = self.index_of(id) {
                self.close_at(i, window, cx);
            }
        }
    }

    /// The open tab menu, over the window at the pointer, with its click-away backdrop.
    pub(crate) fn render_tab_menu(
        &self,
        ui: UiColors,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let menu = self.sidebar.tab_menu.as_ref()?;
        if self.sidebar.hidden {
            return None;
        }
        let order = self.model.display_order();
        let at = order.iter().position(|&w| w == menu.workspace)?;
        let len = order.len();
        let shown = self.workspaces_with_rows();
        let current = self.model.workspace(menu.workspace)?.color;
        let hover = ui.panel_hover;
        let item = |n: usize, label: &'static str, action: TabAction| {
            let on = enabled(action, &shown, len, at);
            div()
                .id(("tab-menu", n))
                .flex()
                .items_center()
                .h(px(ITEM))
                .px(px(8.0))
                .rounded(px(5.0))
                .text_size(px(text::NAME))
                .text_color(hsla(if on { ui.text } else { ui.text_faint }))
                .child(label)
                .when(on, |item| {
                    item.cursor_pointer()
                        .hover(move |s| s.bg(hsla(hover)))
                        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                            this.choose(action, window, cx)
                        }))
                })
                .into_any_element()
        };
        let separator = || {
            div()
                .my(px(4.0))
                .mx(px(8.0))
                .h(px(1.0))
                .bg(hsla(ui.panel_border))
                .into_any_element()
        };
        let swatches = std::iter::once(None)
            .chain(TabColor::ALL.map(Some))
            .enumerate()
            .map(|(n, color)| {
                let chosen = color == current;
                let swatch = match color {
                    Some(c) => div()
                        .size(px(SWATCH))
                        .rounded_full()
                        .bg(hsla(ui.tab(c)))
                        .into_any_element(),
                    None => no_color(ui).into_any_element(),
                };
                div()
                    .id(("tab-color", n))
                    .flex_none()
                    .size(px(SWATCH_CELL))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .cursor_pointer()
                    .when(chosen, |cell| cell.border_2().border_color(hsla(ui.text)))
                    .when(!chosen, |cell| cell.hover(move |s| s.bg(hsla(hover))))
                    .child(swatch)
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        this.choose(TabAction::Color(color), window, cx)
                    }))
            });
        let panel = overlay_panel::shell(&ui, MENU_WIDTH)
            .p(px(4.0))
            .gap(px(0.0))
            .track_focus(&menu.focus)
            .key_context("TabMenu")
            .on_action(
                cx.listener(|this, _: &Dismiss, window, cx| this.dismiss_tab_menu(window, cx)),
            )
            // Its own clicks don't reach the backdrop under it.
            .on_mouse_down(MouseButton::Left, |_: &MouseDownEvent, _, cx| {
                cx.stop_propagation()
            })
            .on_mouse_down(MouseButton::Right, |_: &MouseDownEvent, _, cx| {
                cx.stop_propagation()
            })
            .child(item(0, "Close Workspace", TabAction::Close))
            .child(item(1, "Close Other Workspaces", TabAction::CloseOthers))
            .child(item(2, "Close Workspaces Below", TabAction::CloseToRight))
            .child(item(3, "Show All Workspaces", TabAction::ShowAll))
            .child(separator())
            .child(item(4, "Rename Workspace\u{2026}", TabAction::Rename))
            .child(separator())
            .child(
                div()
                    .px(px(8.0))
                    .pt(px(2.0))
                    .pb(px(4.0))
                    .text_size(px(text::META))
                    .text_color(hsla(ui.text_faint))
                    .child("Workspace Color"),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .px(px(6.0))
                    .pb(px(4.0))
                    .gap(px(SWATCH_GAP))
                    .children(swatches),
            );
        Some(
            div()
                .id("tab-menu-backdrop")
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .occlude()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _: &MouseDownEvent, window, cx| {
                        this.dismiss_tab_menu(window, cx)
                    }),
                )
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(|this, _: &MouseDownEvent, window, cx| {
                        this.dismiss_tab_menu(window, cx)
                    }),
                )
                .child(
                    anchored()
                        .position(menu.at)
                        .snap_to_window_with_margin(px(EDGE_MARGIN))
                        .child(panel),
                )
                .into_any_element(),
        )
    }
}

/// The "no colour" swatch: an outlined circle with a slash through it.
fn no_color(ui: UiColors) -> impl IntoElement {
    let color = ui.text_dim;
    div()
        .size(px(SWATCH))
        .rounded_full()
        .border_1()
        .border_color(hsla(color))
        .child(
            canvas(
                |_, _, _| (),
                move |bounds, _, window, _| {
                    // Corner to corner of the circle's inside, bottom left to top right.
                    let inset = bounds.size.width * 0.15;
                    let (o, s) = (bounds.origin, bounds.size);
                    let mut path = PathBuilder::stroke(px(1.0));
                    path.move_to(point(o.x + inset, o.y + s.height - inset));
                    path.line_to(point(o.x + s.width - inset, o.y + inset));
                    if let Ok(path) = path.build() {
                        window.paint_path(path, hsla(color));
                    }
                },
            )
            .size_full(),
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use TabAction::*;

    /// Every one of `n` sessions has a row (no group closed).
    fn all(n: usize) -> Vec<usize> {
        (0..n).collect()
    }

    #[test]
    fn closes_pick_the_right_sessions() {
        // Five sessions, the menu on the second (index 1).
        assert_eq!(closes(Close, &all(5), 5, 1), [1]);
        assert_eq!(closes(CloseOthers, &all(5), 5, 1), [0, 2, 3, 4]);
        assert_eq!(closes(CloseToRight, &all(5), 5, 1), [2, 3, 4]);
        // The last one has nothing to its right; the only one, no others.
        assert!(closes(CloseToRight, &all(5), 5, 4).is_empty());
        assert!(closes(CloseOthers, &all(1), 1, 0).is_empty());
        // The rest close nothing.
        for action in [ShowAll, Rename, Color(None), Color(Some(TabColor::Red))] {
            assert!(closes(action, &all(5), 5, 1).is_empty(), "{action:?}");
        }
    }

    #[test]
    fn close_to_the_right_leaves_a_closed_group_below_alone() {
        // Seven sessions; 3..6 are in a closed group below, so only 0, 1, 2 and 6 have rows.
        let shown = [0, 1, 2, 6];
        assert_eq!(closes(CloseToRight, &shown, 7, 0), [1, 2, 6]);
        assert_eq!(closes(CloseToRight, &shown, 7, 2), [6]);
        // Only the closed group below: nothing to close, so it's disabled.
        let shown = [0, 1, 2];
        assert!(closes(CloseToRight, &shown, 7, 2).is_empty());
        assert!(!enabled(CloseToRight, &shown, 7, 2));
        // Close Other Tabs still means every other session, folded or not.
        assert_eq!(closes(CloseOthers, &shown, 7, 2), [0, 1, 3, 4, 5, 6]);
    }

    #[test]
    fn bulk_closes_are_disabled_when_they_would_close_nothing() {
        assert!(enabled(CloseOthers, &all(2), 2, 0));
        assert!(!enabled(CloseOthers, &all(1), 1, 0));
        assert!(enabled(CloseToRight, &all(3), 3, 1));
        assert!(!enabled(CloseToRight, &all(3), 3, 2));
        for action in [Close, ShowAll, Rename, Color(None)] {
            assert!(enabled(action, &all(1), 1, 0), "{action:?}");
        }
        // Enabled exactly when it closes something.
        for (len, at) in [(1, 0), (2, 0), (2, 1), (5, 2), (5, 4)] {
            for action in [CloseOthers, CloseToRight] {
                assert_eq!(
                    enabled(action, &all(len), len, at),
                    !closes(action, &all(len), len, at).is_empty()
                );
            }
        }
    }

    #[test]
    fn the_confirm_says_how_many_and_who_is_busy() {
        assert_eq!(
            confirm_text(3, 0),
            ("Close 3 workspaces?".to_string(), None)
        );
        assert_eq!(
            confirm_text(4, 2),
            (
                "Close 4 workspaces?".to_string(),
                Some("2 agents are still working or waiting.".to_string())
            )
        );
        assert_eq!(
            confirm_text(1, 1),
            (
                "Close 1 workspace?".to_string(),
                Some("1 agent is still working or waiting.".to_string())
            )
        );
    }

    #[test]
    fn closing_a_workspace_asks_only_with_two_or_more_terminals() {
        assert!(!asks_to_close(0));
        assert!(!asks_to_close(1));
        assert!(asks_to_close(2));
        assert!(asks_to_close(7));
        assert_eq!(
            confirm_workspace_text(3, 0),
            ("Close workspace with 3 terminals?".to_string(), None)
        );
        assert_eq!(
            confirm_workspace_text(2, 1),
            (
                "Close workspace with 2 terminals?".to_string(),
                Some("1 agent is still working or waiting.".to_string())
            )
        );
    }
}
