//! A pane tab's right-click menu (M11b): Close Tab, Close Other Tabs in Pane, Rename Tab…, for
//! a browser tab Pop Out, and for an agent tab Open in Classic View (M11aa; Show Classic View
//! while that terminal is open). Close Tab ends the tab at once (as its × and ⌘W do); Close
//! Other Tabs is a bulk close, so it asks first in a native sheet; Rename Tab… names the tab's
//! terminal (as `session.rename` does), or an agent tab; Pop Out moves a browser tab's page into
//! a window of its own (M10c's).
//!
//! `closes`, `enabled` and `confirm_text` are the pure model; the rest opens, draws and acts on
//! the menu. Like the row menu, it is one of the overlays (one open at a time): it opens at the
//! pointer and holds the keyboard so Escape closes it, and a click anywhere else closes it too.

use gpui::{
    AnyElement, ClickEvent, Context, FocusHandle, MouseButton, MouseDownEvent, Pixels, Point,
    PromptLevel, Window, anchored, div, prelude::*, px,
};

use crate::layout_model::{Pane, TabId, TabKind};
use crate::overlay_panel;
use crate::status::Status;
use crate::tab_menu::{self, Dismiss};
use crate::terminal_view::hsla;
use crate::theme::UiColors;
use crate::ui_tokens::text;
use crate::workspace::{Overlay, Workspace};

/// The menu's width and an item's height (as the row menu's).
const MENU_WIDTH: f32 = 224.0;
const ITEM: f32 = 26.0;
/// How far from the window's edges the menu keeps.
const EDGE_MARGIN: f32 = 8.0;

/// An open tab menu: whose tab, where it opened, and the focus it holds.
pub(crate) struct PaneTabMenu {
    pub tab: TabId,
    at: Point<Pixels>,
    focus: FocusHandle,
}

/// What the menu can do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaneTabAction {
    Close,
    CloseOthers,
    Rename,
    PopOut,
    /// An agent tab's conversation in the agent's own UI, in a terminal tab (R13).
    OpenClassic,
}

/// The tabs `action` from `tab`'s menu closes, in bar order: itself, or every other tab of its
/// pane; none for Rename (or a tab not in `pane`).
pub fn closes(action: PaneTabAction, pane: &Pane, tab: TabId) -> Vec<TabId> {
    if !pane.tabs.iter().any(|t| t.id == tab) {
        return Vec::new();
    }
    match action {
        PaneTabAction::Close => vec![tab],
        PaneTabAction::CloseOthers => pane
            .tabs
            .iter()
            .map(|t| t.id)
            .filter(|&id| id != tab)
            .collect(),
        PaneTabAction::Rename | PaneTabAction::PopOut | PaneTabAction::OpenClassic => Vec::new(),
    }
}

/// Whether `action` can be chosen from `tab`'s menu: Close Other Tabs needs another tab in the
/// pane; Rename Tab… needs a terminal or an agent tab to name; Pop Out, a browser tab (so a
/// file tab has neither); Open in Classic View, an agent tab with a conversation.
pub fn enabled(action: PaneTabAction, pane: &Pane, tab: TabId) -> bool {
    match action {
        PaneTabAction::Close => pane.tabs.iter().any(|t| t.id == tab),
        PaneTabAction::CloseOthers => !closes(action, pane, tab).is_empty(),
        PaneTabAction::Rename => pane
            .tabs
            .iter()
            .any(|t| t.id == tab && matches!(t.kind, TabKind::Terminal(_) | TabKind::Agent { .. })),
        PaneTabAction::PopOut => pane
            .tabs
            .iter()
            .any(|t| t.id == tab && matches!(t.kind, TabKind::Browser { .. })),
        PaneTabAction::OpenClassic => pane.tabs.iter().any(|t| {
            t.id == tab
                && matches!(
                    t.kind,
                    TabKind::Agent {
                        conversation: Some(_),
                        ..
                    }
                )
        }),
    }
}

/// The sheet Close Other Tabs asks with: closing `count` tabs, `busy` of whose terminals have
/// an agent working or waiting.
pub fn confirm_text(count: usize, busy: usize) -> (String, Option<String>) {
    let tabs = if count == 1 { "tab" } else { "tabs" };
    (
        format!("Close {count} other {tabs}?"),
        tab_menu::busy_line(busy),
    )
}

impl Workspace {
    /// Right-click on a pane tab: its menu opens at `at` (the pointer) and takes the keyboard.
    pub(crate) fn open_pane_tab_menu(
        &mut self,
        tab: TabId,
        at: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_overlays(Overlay::PaneTabMenu);
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        self.pane_tab_menu = Some(PaneTabMenu { tab, at, focus });
        cx.notify();
    }

    /// Escape or a click outside: the menu goes and the typing returns to the terminal.
    fn dismiss_pane_tab_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.pane_tab_menu.take().is_some() {
            self.focus_active(window, cx);
            cx.notify();
        }
    }

    /// A choice from the menu: it closes, then does `action` to its tab.
    fn choose_for_tab(
        &mut self,
        action: PaneTabAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(menu) = self.pane_tab_menu.take() else {
            return;
        };
        cx.notify();
        let tab = menu.tab;
        let Some(pane) = self
            .model
            .locate(tab)
            .and_then(|(_, pane)| self.model.pane(pane))
            .cloned()
        else {
            self.focus_active(window, cx);
            return;
        };
        match action {
            PaneTabAction::Close => {
                self.focus_active(window, cx);
                self.close_tab(tab, window, cx);
            }
            PaneTabAction::CloseOthers => {
                self.focus_active(window, cx);
                self.confirm_close_tabs(closes(action, &pane, tab), window, cx);
            }
            PaneTabAction::Rename => match self.terminal_of(tab) {
                Some(id) => self.start_tab_rename(id, window, cx),
                None if self.agents.contains_key(&tab) => self.start_agent_rename(tab, window, cx),
                None => self.focus_active(window, cx),
            },
            PaneTabAction::OpenClassic => {
                self.focus_active(window, cx);
                self.open_classic(tab, window, cx);
            }
            PaneTabAction::PopOut => {
                self.focus_active(window, cx);
                self.pop_out(tab, window, cx);
            }
        }
    }

    /// Close `tabs` once a native sheet has asked (always: it is a bulk close). Cancel keeps
    /// them all. By id, so the ones closed are the ones asked about, whatever moves meanwhile.
    fn confirm_close_tabs(
        &mut self,
        tabs: Vec<TabId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if tabs.is_empty() {
            return;
        }
        let busy = tabs
            .iter()
            .filter_map(|&t| self.terminal_of(t))
            .filter_map(|id| self.index_of(id))
            .filter(|&i| {
                matches!(
                    self.sessions.items()[i].status,
                    Status::Working | Status::Waiting
                )
            })
            .count()
            + tabs.iter().filter(|&&t| self.agent_busy(t, cx)).count();
        let (message, detail) = confirm_text(tabs.len(), busy);
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
                // Asked already: an agent tab closes without asking again.
                for tab in tabs {
                    if this.agents.contains_key(&tab) {
                        this.close_agent_tab(tab, false, window, cx);
                    } else {
                        this.close_tab(tab, window, cx);
                    }
                }
            })
            .ok();
        })
        .detach();
    }

    /// The open tab menu, over the window at the pointer, with its click-away backdrop.
    pub(crate) fn render_pane_tab_menu(
        &self,
        ui: UiColors,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let menu = self.pane_tab_menu.as_ref()?;
        let (_, pane) = self.model.locate(menu.tab)?;
        let pane = self.model.pane(pane)?;
        let is_agent = pane
            .tabs
            .iter()
            .any(|t| t.id == menu.tab && matches!(t.kind, TabKind::Agent { .. }));
        let classic_open = self
            .classic_seen(menu.tab, cx)
            .1
            .is_some_and(|id| self.index_of(id).is_some());
        let hover = ui.panel_hover;
        let item = |n: usize, label: &'static str, action: PaneTabAction| {
            let on = enabled(action, pane, menu.tab);
            div()
                .id(("pane-tab-menu", n))
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
                            this.choose_for_tab(action, window, cx)
                        }))
                })
                .into_any_element()
        };
        let separator = div()
            .my(px(4.0))
            .mx(px(8.0))
            .h(px(1.0))
            .bg(hsla(ui.panel_border));
        let panel = overlay_panel::shell(&ui, MENU_WIDTH)
            .p(px(4.0))
            .gap(px(0.0))
            .track_focus(&menu.focus)
            .key_context("TabMenu")
            .on_action(
                cx.listener(|this, _: &Dismiss, window, cx| this.dismiss_pane_tab_menu(window, cx)),
            )
            // Its own clicks don't reach the backdrop under it.
            .on_mouse_down(MouseButton::Left, |_: &MouseDownEvent, _, cx| {
                cx.stop_propagation()
            })
            .on_mouse_down(MouseButton::Right, |_: &MouseDownEvent, _, cx| {
                cx.stop_propagation()
            })
            .child(item(0, "Close Tab", PaneTabAction::Close))
            .child(item(
                1,
                "Close Other Tabs in Pane",
                PaneTabAction::CloseOthers,
            ))
            .child(separator)
            .child(item(2, "Rename Tab\u{2026}", PaneTabAction::Rename))
            .when(enabled(PaneTabAction::PopOut, pane, menu.tab), |panel| {
                panel.child(item(3, "Pop Out", PaneTabAction::PopOut))
            })
            // Its classic view open already: the item shows that terminal (R13: never a second).
            .when(is_agent, |panel| {
                let label = if classic_open {
                    "Show Classic View"
                } else {
                    "Open in Classic View"
                };
                panel.child(item(4, label, PaneTabAction::OpenClassic))
            });
        Some(
            div()
                .id("pane-tab-menu-backdrop")
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .occlude()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _: &MouseDownEvent, window, cx| {
                        this.dismiss_pane_tab_menu(window, cx)
                    }),
                )
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(|this, _: &MouseDownEvent, window, cx| {
                        this.dismiss_pane_tab_menu(window, cx)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout_model::{PaneId, Tab};
    use PaneTabAction::*;

    /// A pane of `kinds`, tab ids 1, 2, …, the first one active.
    fn pane(kinds: &[TabKind]) -> Pane {
        Pane {
            id: PaneId(100),
            tabs: kinds
                .iter()
                .enumerate()
                .map(|(i, kind)| Tab {
                    id: TabId(i as u64 + 1),
                    kind: kind.clone(),
                })
                .collect(),
            active: TabId(1),
        }
    }

    fn browser() -> TabKind {
        TabKind::Browser { url: None }
    }

    #[test]
    fn close_closes_the_tab_and_close_others_the_rest_of_its_pane() {
        let p = pane(&[
            TabKind::Terminal(1),
            browser(),
            TabKind::Terminal(2),
            TabKind::Terminal(3),
        ]);
        assert_eq!(closes(Close, &p, TabId(3)), [TabId(3)]);
        assert_eq!(
            closes(CloseOthers, &p, TabId(3)),
            [TabId(1), TabId(2), TabId(4)]
        );
        assert!(closes(Rename, &p, TabId(3)).is_empty());
        // A tab not in the pane closes nothing.
        assert!(closes(Close, &p, TabId(9)).is_empty());
        assert!(closes(CloseOthers, &p, TabId(9)).is_empty());
    }

    #[test]
    fn a_file_tab_closes_but_is_neither_renamed_nor_popped_out() {
        let file = TabKind::File {
            path: "/w/a.rs".into(),
            mode: crate::layout_model::FileMode::Plain,
        };
        let p = pane(&[file, TabKind::Terminal(1)]);
        assert!(enabled(Close, &p, TabId(1)));
        assert!(enabled(CloseOthers, &p, TabId(1)));
        assert!(!enabled(Rename, &p, TabId(1)));
        assert!(!enabled(PopOut, &p, TabId(1)));
        assert_eq!(closes(CloseOthers, &p, TabId(1)), [TabId(2)]);
    }

    #[test]
    fn close_others_needs_another_tab_and_rename_a_terminal() {
        let one = pane(&[TabKind::Terminal(1)]);
        assert!(enabled(Close, &one, TabId(1)));
        assert!(!enabled(CloseOthers, &one, TabId(1)));
        assert!(enabled(Rename, &one, TabId(1)));
        let two = pane(&[TabKind::Terminal(1), browser()]);
        assert!(enabled(CloseOthers, &two, TabId(1)));
        assert!(enabled(CloseOthers, &two, TabId(2)));
        // A browser tab has no terminal to name (its title is the page's).
        assert!(!enabled(Rename, &two, TabId(2)));
        assert!(enabled(Close, &two, TabId(2)));
    }

    #[test]
    fn only_a_browser_tab_pops_out_and_that_closes_nothing_else() {
        let two = pane(&[TabKind::Terminal(1), browser()]);
        assert!(enabled(PopOut, &two, TabId(2)));
        assert!(!enabled(PopOut, &two, TabId(1)));
        assert!(!enabled(PopOut, &two, TabId(9)));
        assert!(closes(PopOut, &two, TabId(2)).is_empty());
    }

    #[test]
    fn the_sheet_says_how_many_tabs_and_who_is_busy() {
        assert_eq!(confirm_text(1, 0), ("Close 1 other tab?".to_string(), None));
        assert_eq!(
            confirm_text(3, 2),
            (
                "Close 3 other tabs?".to_string(),
                Some("2 agents are still working or waiting.".to_string())
            )
        );
    }

    #[test]
    fn an_agent_tab_renames_and_opens_classic_only_with_a_conversation() {
        let agent = |conversation: Option<&str>| TabKind::Agent {
            provider: "claude".into(),
            cwd: "/w".into(),
            conversation: conversation.map(Into::into),
            mode: None,
            title: None,
        };
        let p = pane(&[agent(Some("c1")), agent(None), TabKind::Terminal(1)]);
        assert!(enabled(Rename, &p, TabId(1)));
        assert!(enabled(Rename, &p, TabId(2)));
        assert!(enabled(OpenClassic, &p, TabId(1)));
        assert!(!enabled(OpenClassic, &p, TabId(2)));
        assert!(!enabled(OpenClassic, &p, TabId(3)));
        assert!(!enabled(PopOut, &p, TabId(1)));
        assert!(enabled(Close, &p, TabId(1)));
        assert!(closes(OpenClassic, &p, TabId(1)).is_empty());
    }
}
