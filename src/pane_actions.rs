//! A pane's ⋯ menu (M11i): New Terminal Tab, New Browser Tab, Split Right, Split Down, each
//! with its key (as the menu bar shows it). It opens under the tab bar's ⋯, right edges
//! together, kept 8 pt from the window's edges; the ⋯ stays lit while it's open. A choice
//! focuses that pane first, then acts, as the bar's buttons do.
//!
//! `PaneAction` is the pure model; the rest opens, draws and acts on the menu. Like the tab
//! menus, it is one of the overlays (one open at a time): it holds the keyboard so Escape
//! closes it, and a click anywhere else closes it too.

use gpui::{
    Anchor, AnyElement, ClickEvent, Context, FocusHandle, MouseButton, MouseDownEvent, Pixels,
    Point, Window, anchored, div, prelude::*, px,
};

use crate::layout_model::PaneId;
use crate::overlay_panel;
use crate::split_tree::Axis;
use crate::tab_menu::Dismiss;
use crate::terminal_view::hsla;
use crate::theme::UiColors;
use crate::ui_tokens::text;
use crate::workspace::{Overlay, Workspace};

/// The menu's width and an item's height.
const MENU_WIDTH: f32 = 200.0;
const ITEM: f32 = 26.0;
/// How far from the window's edges the menu keeps.
const EDGE_MARGIN: f32 = 8.0;

/// What the menu does to its pane.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaneAction {
    NewTerminal,
    NewBrowser,
    SplitRight,
    SplitDown,
}

impl PaneAction {
    /// The menu's items, top to bottom.
    pub const ALL: [PaneAction; 4] = [
        PaneAction::NewTerminal,
        PaneAction::NewBrowser,
        PaneAction::SplitRight,
        PaneAction::SplitDown,
    ];

    /// Its item's name.
    pub fn label(self) -> &'static str {
        match self {
            PaneAction::NewTerminal => "New Terminal Tab",
            PaneAction::NewBrowser => "New Browser Tab",
            PaneAction::SplitRight => "Split Right",
            PaneAction::SplitDown => "Split Down",
        }
    }

    /// Its key, shown at the item's right (none: it has no key).
    pub fn keys(self) -> Option<&'static str> {
        match self {
            PaneAction::NewTerminal => Some("⌘T"),
            PaneAction::NewBrowser => None,
            PaneAction::SplitRight => Some("⌘D"),
            PaneAction::SplitDown => Some("⇧⌘D"),
        }
    }
}

/// An open ⋯ menu: whose pane, the point its top right corner hangs from (the ⋯'s bottom
/// right), and the focus it holds.
pub(crate) struct PaneActionsMenu {
    pub pane: PaneId,
    pub at: Point<Pixels>,
    focus: FocusHandle,
}

impl Workspace {
    /// A click on `pane`'s ⋯: its menu opens with its top right corner at `at` and takes the
    /// keyboard.
    pub(crate) fn open_pane_actions(
        &mut self,
        pane: PaneId,
        at: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_overlays(Overlay::PaneActions);
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        self.pane_actions = Some(PaneActionsMenu { pane, at, focus });
        cx.notify();
    }

    /// Escape or a click outside: the menu goes and the typing returns to the terminal.
    fn dismiss_pane_actions(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.pane_actions.take().is_some() {
            self.focus_active(window, cx);
            cx.notify();
        }
    }

    /// Do `action` to `pane` (a choice from its menu, which closes): the pane takes the focus
    /// first, as a click on its bar's buttons does.
    pub(crate) fn run_pane_action(
        &mut self,
        pane: PaneId,
        action: PaneAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.pane_actions = None;
        cx.notify();
        if self.model.pane(pane).is_none() {
            self.focus_active(window, cx);
            return;
        }
        if self.model.focus_pane(pane) {
            self.sync();
        }
        self.focus_active(window, cx);
        match action {
            PaneAction::NewTerminal => self.new_terminal_tab(pane, window, cx),
            PaneAction::NewBrowser => self.new_browser_tab(pane, window, cx),
            PaneAction::SplitRight => self.split_pane(pane, Axis::Row, window, cx),
            PaneAction::SplitDown => self.split_pane(pane, Axis::Column, window, cx),
        }
    }

    /// The open ⋯ menu, under its button, with its click-away backdrop.
    pub(crate) fn render_pane_actions(
        &self,
        ui: UiColors,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let menu = self.pane_actions.as_ref()?;
        self.model.pane(menu.pane)?;
        let pane = menu.pane;
        let hover = ui.panel_hover;
        let items = PaneAction::ALL.map(|action| {
            div()
                .id(("pane-actions", action as usize))
                .flex()
                .items_center()
                .justify_between()
                .h(px(ITEM))
                .px(px(8.0))
                .rounded(px(5.0))
                .text_size(px(text::NAME))
                .text_color(hsla(ui.text))
                .cursor_pointer()
                .hover(move |s| s.bg(hsla(hover)))
                .child(action.label())
                .children(
                    action
                        .keys()
                        .map(|keys| div().text_color(hsla(ui.text_dim)).child(keys)),
                )
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    this.run_pane_action(pane, action, window, cx)
                }))
        });
        let panel = overlay_panel::shell(&ui, MENU_WIDTH)
            .p(px(4.0))
            .gap(px(0.0))
            .track_focus(&menu.focus)
            .key_context("TabMenu")
            .on_action(
                cx.listener(|this, _: &Dismiss, window, cx| this.dismiss_pane_actions(window, cx)),
            )
            // Its own clicks don't reach the backdrop under it.
            .on_mouse_down(MouseButton::Left, |_: &MouseDownEvent, _, cx| {
                cx.stop_propagation()
            })
            .on_mouse_down(MouseButton::Right, |_: &MouseDownEvent, _, cx| {
                cx.stop_propagation()
            })
            .children(items);
        Some(
            div()
                .id("pane-actions-backdrop")
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .occlude()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _: &MouseDownEvent, window, cx| {
                        this.dismiss_pane_actions(window, cx)
                    }),
                )
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(|this, _: &MouseDownEvent, window, cx| {
                        this.dismiss_pane_actions(window, cx)
                    }),
                )
                .child(
                    anchored()
                        .anchor(Anchor::TopRight)
                        .position(menu.at)
                        .snap_to_window_with_margin(px(EDGE_MARGIN))
                        .child(panel),
                )
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::Overlay;

    #[test]
    fn the_menu_lists_the_pane_actions_with_their_keys() {
        assert_eq!(
            PaneAction::ALL,
            [
                PaneAction::NewTerminal,
                PaneAction::NewBrowser,
                PaneAction::SplitRight,
                PaneAction::SplitDown
            ]
        );
        assert_eq!(
            PaneAction::ALL.map(PaneAction::label),
            [
                "New Terminal Tab",
                "New Browser Tab",
                "Split Right",
                "Split Down"
            ]
        );
        assert_eq!(
            PaneAction::ALL.map(PaneAction::keys),
            [Some("⌘T"), None, Some("⌘D"), Some("⇧⌘D")]
        );
    }

    #[test]
    fn the_menu_is_one_of_the_overlays() {
        assert!(Overlay::ALL.contains(&Overlay::PaneActions));
    }
}
