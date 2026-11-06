//! Sidebar rows as tabs (M11): a row's right-click menu (Close Tab, Close Other Tabs, Close Tabs
//! to the Right, Show All Tabs, Rename Tab…, Tab Color) and the closes it starts. Closing one
//! session is like ⌘W; closing several always asks first, in a native sheet.
//!
//! `closes`, `enabled` and `confirm_text` are the pure model; the rest opens, draws and acts on
//! the menu. It is one of the overlays (one open at a time): it opens at the pointer, holds the
//! keyboard so Escape closes it, and a click anywhere else (or any choice) closes it too.

use gpui::{
    AnyElement, ClickEvent, Context, EntityId, FocusHandle, MouseButton, MouseDownEvent,
    PathBuilder, Pixels, Point, PromptLevel, Window, actions, anchored, canvas, div, point,
    prelude::*, px,
};

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

/// An open tab menu: whose row (by view), where it opened, and the focus it holds.
pub(crate) struct TabMenu {
    pub view: EntityId,
    at: Point<Pixels>,
    focus: FocusHandle,
}

/// What the menu can do to its session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabAction {
    Close,
    CloseOthers,
    CloseToRight,
    ShowAll,
    Rename,
    Color(Option<TabColor>),
}

/// The sessions (by place in the list of `len`) that `action` from session `at`'s menu closes:
/// itself; every other one; or the ones after it in the list (the rows below it). None for the
/// actions that close nothing.
pub fn closes(action: TabAction, len: usize, at: usize) -> Vec<usize> {
    match action {
        TabAction::Close => vec![at],
        TabAction::CloseOthers => (0..len).filter(|&i| i != at).collect(),
        TabAction::CloseToRight => (at + 1..len).collect(),
        TabAction::ShowAll | TabAction::Rename | TabAction::Color(_) => Vec::new(),
    }
}

/// Whether `action` can be chosen from session `at`'s menu (of `len` sessions): Close Other
/// Tabs needs another session, Close Tabs to the Right one below it.
pub fn enabled(action: TabAction, len: usize, at: usize) -> bool {
    match action {
        TabAction::CloseOthers | TabAction::CloseToRight => !closes(action, len, at).is_empty(),
        _ => true,
    }
}

/// The confirm sheet for closing `count` sessions, `busy` of them with an agent still working or
/// waiting: its message, and a line about the busy ones when there are any.
pub fn confirm_text(count: usize, busy: usize) -> (String, Option<String>) {
    let plural = |n: usize, one: &str, many: &str| {
        if n == 1 {
            one.to_string()
        } else {
            many.to_string()
        }
    };
    let message = format!("Close {count} {}?", plural(count, "session", "sessions"));
    let detail = (busy > 0).then(|| {
        let who = plural(busy, "agent is", "agents are");
        format!("{busy} {who} still working or waiting.")
    });
    (message, detail)
}

impl Workspace {
    /// Right-click on `view`'s row: its menu opens at `at` (the pointer) and takes the keyboard.
    pub(crate) fn open_tab_menu(
        &mut self,
        view: EntityId,
        at: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_overlays(Overlay::TabMenu);
        self.sidebar.new_menu = false;
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        self.sidebar.tab_menu = Some(TabMenu { view, at, focus });
        cx.notify();
    }

    /// Escape or a click outside: the menu goes and the typing returns to the terminal.
    fn dismiss_tab_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.sidebar.tab_menu.take().is_some() {
            self.focus_active(window, cx);
            cx.notify();
        }
    }

    /// A choice from the menu: it closes, then does `action` to its session.
    fn choose(&mut self, action: TabAction, window: &mut Window, cx: &mut Context<Self>) {
        let Some(menu) = self.sidebar.tab_menu.take() else {
            return;
        };
        cx.notify();
        let Some(at) = self.sessions.position(|s| s.view.entity_id() == menu.view) else {
            self.focus_active(window, cx);
            return;
        };
        let len = self.sessions.items().len();
        match action {
            TabAction::Close => self.close_at(at, window, cx),
            TabAction::CloseOthers | TabAction::CloseToRight => {
                self.focus_active(window, cx);
                self.confirm_close(closes(action, len, at), window, cx);
            }
            TabAction::ShowAll => self.open_switcher(window, cx),
            TabAction::Rename => self.start_rename(at, window, cx),
            TabAction::Color(color) => {
                if let Some(session) = self.sessions.get_mut(at) {
                    session.color = color;
                }
                self.save();
                self.focus_active(window, cx);
            }
        }
    }

    /// Close the sessions at `indices`, once a native sheet has asked (always, for more than
    /// one from the menu). Cancel keeps them all.
    fn confirm_close(&mut self, indices: Vec<usize>, window: &mut Window, cx: &mut Context<Self>) {
        let chosen: Vec<_> = indices
            .iter()
            .filter_map(|&i| self.sessions.items().get(i))
            .collect();
        if chosen.is_empty() {
            return;
        }
        let busy = chosen
            .iter()
            .filter(|s| matches!(s.status, Status::Working | Status::Waiting))
            .count();
        // By view, so the ones closed are the ones asked about, whatever moves meanwhile.
        let views: Vec<EntityId> = chosen.iter().map(|s| s.view.entity_id()).collect();
        let (message, detail) = confirm_text(views.len(), busy);
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
                for view in views {
                    this.close_view(view, window, cx);
                }
            })
            .ok();
        })
        .detach();
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
        let at = self
            .sessions
            .position(|s| s.view.entity_id() == menu.view)?;
        let len = self.sessions.items().len();
        let current = self.sessions.items()[at].color;
        let hover = ui.panel_hover;
        let item = |n: usize, label: &'static str, action: TabAction| {
            let on = enabled(action, len, at);
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
            .child(item(0, "Close Tab", TabAction::Close))
            .child(item(1, "Close Other Tabs", TabAction::CloseOthers))
            .child(item(2, "Close Tabs to the Right", TabAction::CloseToRight))
            .child(item(3, "Show All Tabs", TabAction::ShowAll))
            .child(separator())
            .child(item(4, "Rename Tab\u{2026}", TabAction::Rename))
            .child(separator())
            .child(
                div()
                    .px(px(8.0))
                    .pt(px(2.0))
                    .pb(px(4.0))
                    .text_size(px(text::META))
                    .text_color(hsla(ui.text_faint))
                    .child("Tab Color"),
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

    #[test]
    fn closes_pick_the_right_sessions() {
        // Five sessions, the menu on the second (index 1).
        assert_eq!(closes(Close, 5, 1), [1]);
        assert_eq!(closes(CloseOthers, 5, 1), [0, 2, 3, 4]);
        assert_eq!(closes(CloseToRight, 5, 1), [2, 3, 4]);
        // The last one has nothing to its right; the only one, no others.
        assert!(closes(CloseToRight, 5, 4).is_empty());
        assert!(closes(CloseOthers, 1, 0).is_empty());
        // The rest close nothing.
        for action in [ShowAll, Rename, Color(None), Color(Some(TabColor::Red))] {
            assert!(closes(action, 5, 1).is_empty(), "{action:?}");
        }
    }

    #[test]
    fn bulk_closes_are_disabled_when_they_would_close_nothing() {
        assert!(enabled(CloseOthers, 2, 0));
        assert!(!enabled(CloseOthers, 1, 0));
        assert!(enabled(CloseToRight, 3, 1));
        assert!(!enabled(CloseToRight, 3, 2));
        for action in [Close, ShowAll, Rename, Color(None)] {
            assert!(enabled(action, 1, 0), "{action:?}");
        }
        // Enabled exactly when it closes something.
        for (len, at) in [(1, 0), (2, 0), (2, 1), (5, 2), (5, 4)] {
            for action in [CloseOthers, CloseToRight] {
                assert_eq!(
                    enabled(action, len, at),
                    !closes(action, len, at).is_empty()
                );
            }
        }
    }

    #[test]
    fn the_confirm_says_how_many_and_who_is_busy() {
        assert_eq!(confirm_text(3, 0), ("Close 3 sessions?".to_string(), None));
        assert_eq!(
            confirm_text(4, 2),
            (
                "Close 4 sessions?".to_string(),
                Some("2 agents are still working or waiting.".to_string())
            )
        );
        assert_eq!(
            confirm_text(1, 1),
            (
                "Close 1 session?".to_string(),
                Some("1 agent is still working or waiting.".to_string())
            )
        );
    }
}
