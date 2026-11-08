//! Pane tab bars (M11b): a 26 px bar on top of every pane (even a lone one, as cmux does), in
//! place of M11's pane headers. Each tab shows an icon (a terminal's `>_`, a browser's globe), its
//! title, and for a terminal whose agent is Working, Waiting or Done its status mark in the
//! icon's place; × closes it (shown on hover). The active tab is filled and, in the focused pane,
//! has an accent line along its top; a waiting tab is tinted. Too many tabs shrink to a minimum
//! width, then the row scrolls sideways. On the right, pinned: new terminal tab, new browser tab
//! split right, split down.
//!
//! A tab can be dragged (M11b Task 7): within its bar or into another pane's, or onto a sidebar
//! row (into that workspace's focused pane, at the end). It is picked up (Task 7b): the pointer
//! is a closed hand, the tab stays where it was grabbed, and the other tabs slide aside to open a
//! gap where it would land (`drag_gap`).
//!
//! `bar_tabs`, `icon`, `tip`, `look`, `tab_offsets`, `gap_for_slot` and `BarButton` are the pure
//! model; the rest draws the bar and acts on its clicks and drags.

use std::time::{Duration, Instant};

use alacritty_terminal::vte::ansi::Rgb;
use gpui::{
    AnyElement, AnyView, App, ClickEvent, Context, CursorStyle, Div, DragMoveEvent, Focusable,
    MouseButton, MouseDownEvent, PathBuilder, Pixels, Point, ScrollHandle, SharedString, Window,
    canvas, div, point, prelude::*, px,
};

use crate::appearance::Appearance;
use crate::drag_gap::{self, Slid};
use crate::layout_model::{Pane, PaneId, TabDrop, TabId, TabKind};
use crate::restore::{Launch, SavedSession};
use crate::sidebar::{mark_size, mark_slot};
use crate::sidebar_toggle::Tip;
use crate::split_tree::Axis;
use crate::status::{self, Status};
use crate::status_mark;
use crate::terminal_view::hsla;
use crate::theme::UiColors;
use crate::ui_tokens::size::TAB_BAR;
use crate::ui_tokens::text;
use crate::workspace::{At, Renaming, Workspace};
use crate::workspace_ssh::Place;

/// A tab's width: it shrinks from the most toward the least as tabs are added, then the row
/// scrolls.
pub const TAB_MIN_WIDTH: f32 = 90.0;
pub const TAB_MAX_WIDTH: f32 = 200.0;
/// A browser tab's title until it shows a page.
pub const BROWSER_TITLE: &str = "New tab";

/// What a tab shows left of its title.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabIcon {
    Terminal,
    Globe,
    /// A terminal's status mark, while it is anything but Idle.
    Mark(Status),
}

/// What the bar knows of a terminal: its name (as the sidebar used to show it), its status and
/// for how long it has had it.
pub struct TerminalFacts {
    pub label: SharedString,
    pub status: Status,
    pub age: Duration,
}

/// One tab as the bar draws it.
#[derive(Clone, Debug, PartialEq)]
pub struct BarTab {
    pub tab: TabId,
    /// The terminal it shows (`sN`); none for a browser tab.
    pub terminal: Option<u64>,
    pub title: SharedString,
    pub icon: TabIcon,
    /// Idle for a browser tab.
    pub status: Status,
    pub active: bool,
    /// Its tooltip: the title, and for a busy terminal its status and for how long.
    pub tip: String,
}

/// The icon for a tab of `kind` whose terminal (if any) is `status`.
pub fn icon(kind: &TabKind, status: Status) -> TabIcon {
    match kind {
        TabKind::Browser { .. } => TabIcon::Globe,
        TabKind::Terminal(_) if status == Status::Idle => TabIcon::Terminal,
        TabKind::Terminal(_) => TabIcon::Mark(status),
    }
}

/// A tab's tooltip: `title`, then while not Idle its status and age ("claude — waiting 4m").
pub fn tip(title: &str, status: Status, age: Duration) -> String {
    match status {
        Status::Idle => title.to_string(),
        _ => format!("{title} — {} {}", status.as_str(), status::age_label(age)),
    }
}

/// `pane`'s tabs in bar order. `facts` tells about a terminal; one it doesn't know (never, while
/// tabs and sessions are in step) is drawn as an idle "Terminal". `page` gives a browser tab's
/// title (none: "New tab").
pub fn bar_tabs(
    pane: &Pane,
    facts: impl Fn(u64) -> Option<TerminalFacts>,
    page: impl Fn(TabId) -> Option<SharedString>,
) -> Vec<BarTab> {
    pane.tabs
        .iter()
        .map(|t| {
            let (terminal, title, status, age): (_, SharedString, _, _) = match t.kind {
                TabKind::Terminal(id) => match facts(id) {
                    Some(f) => (Some(id), f.label, f.status, f.age),
                    None => (Some(id), "Terminal".into(), Status::Idle, Duration::ZERO),
                },
                TabKind::Browser { .. } => {
                    let title = page(t.id).unwrap_or_else(|| BROWSER_TITLE.into());
                    (None, title, Status::Idle, Duration::ZERO)
                }
            };
            BarTab {
                tab: t.id,
                terminal,
                icon: icon(&t.kind, status),
                tip: tip(&title, status, age),
                title,
                status,
                active: t.id == pane.active,
            }
        })
        .collect()
}

/// Where a dragged tab would land, as one bar sees it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Landing {
    /// In this bar, at this slot (its index once it is out of its own bar: `move_tab`'s).
    Here(usize),
    /// In another bar, or a sidebar row's workspace.
    Elsewhere,
    /// Nowhere: a drop would put it back.
    Nowhere,
}

/// How far each of a bar's `count` tabs is drawn from its place during a drag of a tab `width`
/// wide, at `own` in this bar (none: from another): a gap opens where it would land; with it
/// elsewhere, its own bar closes up; landing nowhere, the gap is back at its own place.
pub fn tab_offsets(count: usize, own: Option<usize>, landing: Landing, width: f32) -> Vec<f32> {
    let gap = match landing {
        Landing::Here(slot) => Some(slot),
        Landing::Elsewhere => None,
        Landing::Nowhere => own,
    };
    drag_gap::gap_offsets(count, own, gap, width)
}

/// The `TabDrop::Bar` gap (counted with the dragged tab in the bar, at `own`; none: it comes from
/// another) for landing slot `slot` (counted without it).
pub fn gap_for_slot(slot: usize, own: Option<usize>) -> usize {
    // Past its own place, one more: it is still there.
    match own {
        Some(own) if slot >= own => slot + 1,
        _ => slot,
    }
}

/// How a tab is drawn.
#[derive(Debug, PartialEq)]
pub struct Look {
    /// Its fill; none: the bar's own.
    pub fill: Option<Rgb>,
    /// Its fill under the pointer; none: no change.
    pub hover: Option<Rgb>,
    pub text: Rgb,
    /// The accent line along its top edge: the active tab of the focused pane.
    pub top_accent: bool,
}

/// The look of a tab with `status`, `active` in its pane or not, in the focused pane or not. The
/// bar is the sidebar's colour; the active tab is raised to the panel colour with full text, the
/// others dim. A waiting tab is tinted, active or not (the active one a step lighter).
pub fn look(ui: &UiColors, status: Status, active: bool, pane_focused: bool) -> Look {
    let waiting = status == Status::Waiting;
    let (fill, hover) = match (active, waiting) {
        (true, false) => (Some(ui.panel), None),
        (true, true) => (Some(ui.waiting_hover), None),
        (false, false) => (None, Some(ui.row_hover)),
        (false, true) => (Some(ui.waiting_tint), Some(ui.waiting_hover)),
    };
    Look {
        fill,
        hover,
        text: if active { ui.text } else { ui.text_dim },
        top_accent: active && pane_focused,
    }
}

/// The bar's buttons, right to left of the tabs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BarButton {
    NewTerminal,
    NewBrowser,
    SplitRight,
    SplitDown,
}

impl BarButton {
    pub const ALL: [BarButton; 4] = [
        BarButton::NewTerminal,
        BarButton::NewBrowser,
        BarButton::SplitRight,
        BarButton::SplitDown,
    ];

    /// Whether it can be clicked (all can, since browser tabs exist).
    pub fn enabled(self) -> bool {
        true
    }

    /// Its tooltip.
    pub fn tooltip(self) -> &'static str {
        match self {
            BarButton::NewTerminal => "New terminal tab",
            BarButton::NewBrowser => "New browser tab",
            BarButton::SplitRight => "Split right",
            BarButton::SplitDown => "Split down",
        }
    }
}

/// What a pane tab carries while it's dragged.
pub(crate) struct DraggedTab {
    pub tab: TabId,
}

/// A pane tab's drag as it started: the tab, where in it the pointer took it (GPUI's cursor
/// offset), and its width.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TabGrab {
    pub tab: TabId,
    pub at: Point<Pixels>,
    pub width: f32,
}

/// The tab that follows the pointer during a drag: a chip as big as the tab, with its icon (or
/// mark) and title, drawn where the tab was grabbed (the pointer stays on the spot it took).
struct TabChip {
    title: SharedString,
    icon: TabIcon,
    width: f32,
}

impl Render for TabChip {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ui = cx.global::<Appearance>().theme.ui();
        let icon: AnyElement = match self.icon {
            TabIcon::Mark(status) => {
                status_mark::mark_on(status, &ui, 0.0, None, px(mark_size(status)), ui.panel)
                    .into_any_element()
            }
            TabIcon::Globe => glyph(Glyph::Globe, ui.text).into_any_element(),
            TabIcon::Terminal => glyph(Glyph::Terminal, ui.text).into_any_element(),
        };
        div()
            .flex()
            .flex_row()
            .items_center()
            .w(px(self.width))
            .h(px(TAB_BAR))
            .pl(px(TAB_PAD_LEFT))
            .pr(px(TAB_PAD_RIGHT + CLOSE))
            .gap(px(TAB_GAP))
            .rounded(px(4.0))
            .opacity(0.95)
            .shadow_md()
            .bg(hsla(ui.panel))
            .border_1()
            .border_color(hsla(ui.panel_border))
            .text_size(px(text::NAME))
            .text_color(hsla(ui.text))
            .child(mark_slot(icon))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .truncate()
                    .child(self.title.clone()),
            )
    }
}

/// A tab's side padding and the gap between its parts; the × button's side.
const TAB_PAD_LEFT: f32 = 8.0;
const TAB_PAD_RIGHT: f32 = 4.0;
const TAB_GAP: f32 = 6.0;
const CLOSE: f32 = 16.0;
/// The accent line along the active tab's top (as thick as an active divider).
const ACCENT: f32 = 2.0;
/// A button, its icon box, and the gap between buttons and around them.
const BUTTON: f32 = 20.0;
const ICON: f32 = 12.0;
const BUTTON_GAP: f32 = 2.0;
const BUTTONS_PAD: f32 = 4.0;
/// The icons' line width.
const STROKE: f32 = 1.25;

impl Workspace {
    /// `pane`'s tab bar: its tabs (scrolling sideways when they don't fit) and, pinned on the
    /// right, its buttons. None for an unknown pane. `drags_window`: the bar is at the window's
    /// top edge, so its empty part (after the last tab) drags the window.
    pub(crate) fn render_tab_bar(
        &self,
        pane: PaneId,
        ui: UiColors,
        drags_window: bool,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let p = self.model.pane(pane)?;
        let focused = self.model.focused_pane() == Some(pane);
        let now = Instant::now();
        let tabs = bar_tabs(
            p,
            |id| {
                let s = self.sessions.items().iter().find(|s| s.id == id)?;
                Some(TerminalFacts {
                    label: s.label(),
                    status: s.status,
                    age: now.saturating_duration_since(s.status_since),
                })
            },
            |tab| self.browser_titles.get(&tab).map(|t| t.clone().into()),
        );
        let scroll = self.tab_row_scroll(pane, p.active, &tabs);
        // While a tab is dragged: where each tab is drawn, out of its way.
        let targets = match self.tab_grab(cx) {
            Some(grab) => {
                let own = tabs.iter().position(|t| t.tab == grab.tab);
                tab_offsets(tabs.len(), own, self.landing(grab.tab, pane), grab.width)
            }
            None => vec![0.0; tabs.len()],
        };
        let animate = !cx.reduce_motion();
        let row = div()
            .id(("pane-tabs", pane.0 as usize))
            .flex()
            .flex_row()
            .flex_1()
            .min_w(px(0.0))
            .h_full()
            .overflow_x_scroll()
            .track_scroll(&scroll)
            .children(
                tabs.into_iter()
                    .zip(targets)
                    .enumerate()
                    .map(|(i, (tab, target))| {
                        // As last laid out, for its drag chip.
                        let width = scroll
                            .bounds_for_item(i)
                            .map_or(TAB_MAX_WIDTH, |b| f32::from(b.size.width));
                        let offset = self.drag_slides.borrow_mut().offset(
                            Slid::Tab(tab.tab),
                            target,
                            now,
                            animate,
                        );
                        self.render_tab(tab, ui, focused, offset, width, cx)
                    }),
            )
            // What the tabs leave empty; it takes no space once they fill the row.
            .child({
                let filler = div()
                    .id(("pane-tabs-empty", pane.0 as usize))
                    .flex_1()
                    .min_w(px(0.0))
                    .h_full();
                if drags_window {
                    self.drag_window_by(filler, cx)
                } else {
                    filler
                }
            });
        let buttons = div()
            .flex()
            .flex_none()
            .flex_row()
            .items_center()
            .h_full()
            .px(px(BUTTONS_PAD))
            .gap(px(BUTTON_GAP))
            .children(BarButton::ALL.map(|b| render_button(pane, b, ui, cx)));
        Some(
            div()
                .flex()
                .flex_row()
                .flex_none()
                .w_full()
                .h(px(TAB_BAR))
                .bg(hsla(ui.sidebar_background))
                // A press on the bar's empty part focuses its pane (a tab's own press selects it;
                // one in a rename box stays there).
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _: &MouseDownEvent, window, cx| {
                        this.focus_pane_of_bar(pane, window, cx)
                    }),
                )
                // A dragged tab lands in the gap nearest the pointer: over a tab, the empty part
                // (the window-drag area: its handlers stop nothing) or the buttons (the end).
                .on_drag_move(
                    cx.listener(move |this, event: &DragMoveEvent<DraggedTab>, _, cx| {
                        this.drag_tab_over_bar(pane, event, cx)
                    }),
                )
                // Put back in its own bar where it was (the gap at its own place): shown, as a
                // click would. A click that moved past GPUI's 2 px drag threshold ends here, and
                // isn't lost.
                .on_drop(cx.listener(move |this, dragged: &DraggedTab, window, cx| {
                    let here = |at| matches!(at, TabDrop::Bar { pane: p, .. } if p == pane);
                    let tab = dragged.tab;
                    let home = this.model.locate(tab).is_some_and(|(_, p)| p == pane);
                    if !this.drop_tab(tab, here, window, cx) && home {
                        this.select_tab(tab, window, cx);
                    }
                }))
                .child(row)
                .child(buttons)
                .into_any_element(),
        )
    }

    /// `pane`'s tab row scroll, scrolled to its shown tab when that changed since the last frame
    /// (a click, ⌃Tab, a new tab, a script).
    fn tab_row_scroll(&self, pane: PaneId, shown: TabId, tabs: &[BarTab]) -> ScrollHandle {
        let mut scrolls = self.tab_scroll.borrow_mut();
        let (scroll, revealed) = scrolls
            .entry(pane)
            .or_insert_with(|| (ScrollHandle::new(), None));
        if *revealed != Some(shown) {
            if let Some(i) = tabs.iter().position(|t| t.tab == shown) {
                scroll.scroll_to_item(i);
            }
            // A row not laid out yet drops the request (GPUI learns it scrolls only then), so
            // it's asked again on the next frame.
            if scroll.bounds().size.width > px(0.0) {
                *revealed = Some(shown);
            }
        }
        scroll.clone()
    }

    /// One tab: its icon (or mark), its title (or the rename box), × on hover, the accent line
    /// along the top of the focused pane's active tab. It can be dragged (a press and a move of
    /// a few pixels; a click stays a click): its chip, `width` wide, is it meanwhile, and its
    /// place shows empty. It is drawn `offset` px right of its place (out of a dragged tab's way;
    /// its place in the row, which the drop rules measure, stays where it is).
    fn render_tab(
        &self,
        tab: BarTab,
        ui: UiColors,
        pane_focused: bool,
        offset: f32,
        width: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let look = look(&ui, tab.status, tab.active, pane_focused);
        let surface = look.fill.unwrap_or(ui.sidebar_background);
        let id = tab.tab;
        let group: SharedString = format!("pane-tab-{}", id.0).into();
        let session = tab
            .terminal
            .and_then(|t| self.sessions.items().iter().find(|s| s.id == t));
        let icon: AnyElement = match (tab.icon, session) {
            (TabIcon::Mark(_), Some(session)) => self.session_mark("tab", session, ui, surface),
            (TabIcon::Mark(status), None) => status_mark::mark_on(
                status,
                &ui,
                self.phase,
                None,
                px(mark_size(status)),
                surface,
            )
            .into_any_element(),
            (TabIcon::Globe, _) => glyph(Glyph::Globe, look.text).into_any_element(),
            (TabIcon::Terminal, _) => glyph(Glyph::Terminal, look.text).into_any_element(),
        };
        let rename_box = self
            .renaming
            .as_ref()
            .filter(|(r, _)| tab.terminal.is_some_and(|t| *r == Renaming::Terminal(t)))
            .map(|(_, input)| input.clone());
        let title: AnyElement = match rename_box {
            Some(input) => div()
                .flex()
                .flex_1()
                .min_w(px(0.0))
                .child(input)
                .into_any_element(),
            None => div()
                .flex_1()
                .min_w(px(0.0))
                .truncate()
                .child(tab.title.clone())
                .into_any_element(),
        };
        let close = div()
            .id(("pane-tab-close", id.0 as usize))
            .flex_none()
            .size(px(CLOSE))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(4.0))
            .text_size(px(14.0))
            .text_color(hsla(ui.text_dim))
            .hover(move |s| s.bg(hsla(ui.panel_hover)).text_color(hsla(ui.text)))
            .child("×")
            .invisible()
            .group_hover(group.clone(), |s| s.visible())
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                cx.stop_propagation(); // not also a click on the tab
                this.close_tab(id, window, cx)
            }));
        let terminal = tab.terminal;
        let tip = tab.tip.clone();
        let lifted = self.dragged_tab(cx) == Some(id);
        let (source, rows) = (self.tab_drag.clone(), self.sidebar.drag_source.clone());
        let (chip_title, chip_icon) = (tab.title.clone(), tab.icon);
        let tab = div()
            .id(("pane-tab", id.0 as usize))
            .group(group)
            .absolute()
            .top_0()
            .left(px(offset))
            .size_full()
            .flex()
            .flex_row()
            .items_center()
            .pl(px(TAB_PAD_LEFT))
            .pr(px(TAB_PAD_RIGHT))
            .gap(px(TAB_GAP))
            // The line on its right shows between tabs on the bar's colour only: a filled or
            // hovered tab is told apart by its fill (the line would be too faint on it).
            .border_r_1()
            .border_color(hsla(look.fill.unwrap_or(ui.sidebar_border)))
            .text_size(px(text::NAME))
            .text_color(hsla(look.text))
            .when_some(look.fill, |t, fill| t.bg(hsla(fill)))
            .when_some(look.hover, |t, hover| {
                t.hover(move |s| s.bg(hsla(hover)).border_color(hsla(hover)))
            })
            .child(mark_slot(icon))
            .child(title)
            .child(close)
            .when(look.top_accent, |t| {
                t.child(
                    div()
                        .absolute()
                        .top_0()
                        .left_0()
                        .w_full()
                        .h(px(ACCENT))
                        .bg(hsla(ui.accent)),
                )
            })
            // Its chip is it while it's dragged: its place shows empty.
            .when(lifted, |t| t.opacity(0.0))
            .on_drag(DraggedTab { tab: id }, move |dragged, at, window, cx| {
                source.set(Some(TabGrab {
                    tab: dragged.tab,
                    at,
                    width,
                }));
                rows.set(None); // not a row's drag any more
                hold_closed_hand(window, cx);
                cx.new(|_| TabChip {
                    title: chip_title.clone(),
                    icon: chip_icon,
                    width,
                })
            })
            // Its press is its own (not the bar's, which only focuses the pane).
            .on_mouse_down(MouseButton::Left, |_: &MouseDownEvent, _, cx| {
                cx.stop_propagation()
            })
            .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                this.select_tab(id, window, cx);
                if event.click_count() == 2
                    && let Some(terminal) = terminal
                {
                    this.start_tab_rename(terminal, window, cx);
                }
            }))
            // A middle-click closes it, like its ×.
            .on_aux_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                if event.is_middle_click() {
                    this.close_tab(id, window, cx);
                }
            }))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    this.open_pane_tab_menu(id, event.position, window, cx)
                }),
            )
            .tooltip(move |_, cx| -> AnyView { cx.new(|_| Tip(tip.clone().into())).into() });
        // Its place in the row: the size the row gives it, where the tab is drawn from.
        div()
            .relative()
            .flex_basis(px(TAB_MAX_WIDTH))
            .flex_shrink_1()
            .min_w(px(TAB_MIN_WIDTH))
            .max_w(px(TAB_MAX_WIDTH))
            .h_full()
            .child(tab)
            .into_any_element()
    }

    /// Where dragged tab `tab` would land, as `pane`'s bar sees it: in it, at the slot its gap
    /// shows (`Model::drop_place`'s index, so the drop puts it exactly there), elsewhere (another
    /// bar, a sidebar row), or nowhere (a drop would put it back).
    fn landing(&self, tab: TabId, pane: PaneId) -> Landing {
        let Some(drop) = self.tab_drop else {
            return Landing::Nowhere;
        };
        match (drop, self.model.drop_place(tab, drop)) {
            (_, None) => Landing::Nowhere,
            (TabDrop::Bar { pane: p, .. }, Some((_, index))) if p == pane => Landing::Here(index),
            _ => Landing::Elsewhere,
        }
    }

    /// The pointer moved during a tab drag: if it's over `pane`'s bar and dropping there would
    /// move the tab, it lands in the gap nearest the chip's centre, among the bar's other tabs as
    /// laid out with it taken out (`drag_gap::landing_slot`).
    fn drag_tab_over_bar(
        &mut self,
        pane: PaneId,
        event: &DragMoveEvent<DraggedTab>,
        cx: &mut Context<Self>,
    ) {
        let over = event.bounds.contains(&event.event.position);
        let ours = matches!(self.tab_drop, Some(TabDrop::Bar { pane: p, .. }) if p == pane);
        if !over && !ours {
            return; // not this bar's business
        }
        let tab = event.drag(cx).tab;
        let grab = self.tab_drag.get().filter(|grab| grab.tab == tab);
        let next = over
            .then_some(grab)
            .flatten()
            .map(|grab| {
                let centre = f32::from(event.event.position.x - grab.at.x) + grab.width / 2.0;
                let own = self
                    .model
                    .pane(pane)
                    .and_then(|p| p.tabs.iter().position(|t| t.id == tab));
                let slot = drag_gap::landing_slot(centre, &self.tab_edges(pane), own, grab.width);
                TabDrop::Bar {
                    pane,
                    gap: gap_for_slot(slot, own),
                }
            })
            .filter(|at| self.model.drop_place(tab, *at).is_some());
        if self.tab_drop != next {
            self.tab_drop = next;
            cx.notify();
        }
    }

    /// `pane`'s tabs' places' left and right edges as last laid out (its row scrolled, not slid),
    /// in bar order.
    fn tab_edges(&self, pane: PaneId) -> Vec<(f32, f32)> {
        let count = self.model.pane(pane).map_or(0, |p| p.tabs.len());
        let scrolls = self.tab_scroll.borrow();
        let Some((scroll, _)) = scrolls.get(&pane) else {
            return Vec::new();
        };
        let dx = f32::from(scroll.offset().x);
        (0..count)
            .map_while(|i| scroll.bounds_for_item(i))
            .map(|b| (f32::from(b.left()) + dx, f32::from(b.right()) + dx))
            .collect()
    }

    /// A press on a bar's empty part: its pane takes the focus (and the keyboard comes back
    /// from any box).
    fn focus_pane_of_bar(&mut self, pane: PaneId, window: &mut Window, cx: &mut Context<Self>) {
        if self.model.focus_pane(pane) {
            self.sync();
            self.activate(window, cx);
        }
    }

    /// Show `tab` (a click on it): it, its pane and its workspace get the focus.
    pub(crate) fn select_tab(&mut self, tab: TabId, window: &mut Window, cx: &mut Context<Self>) {
        if self.model.select_tab(tab) {
            self.sync();
            self.activate(window, cx);
        }
    }

    /// ⌃Tab / ⌃⇧Tab: the next (or previous) tab of the focused pane, wrapping.
    pub(crate) fn cycle_tab(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.model.cycle_tab(forward).is_some() {
            self.sync();
            self.activate(window, cx);
        }
    }

    /// Close `tab` (its ×, a middle-click, its menu, ⌘W, a workspace closing): a terminal ends;
    /// a browser tab's page goes with its view (`sync` drops it). Never asks. The pane's next
    /// tab shows, or the pane (then the workspace) goes with its last one; with nothing left,
    /// the window.
    pub(crate) fn close_tab(&mut self, tab: TabId, window: &mut Window, cx: &mut Context<Self>) {
        match self.model.tab(tab).map(|t| t.kind.clone()) {
            Some(TabKind::Terminal(id)) => {
                if let Some(i) = self.index_of(id) {
                    self.close_at(i, window, cx);
                }
            }
            Some(TabKind::Browser { .. }) => self.close_browser_tab(tab, true, window, cx),
            None => {}
        }
    }

    /// Close browser tab `tab`: its page leaves the window at once. `by_person`: the person
    /// closed it (its ×, ⌘W, a menu), and the focused tab takes the keyboard as after any close.
    /// Otherwise (an agent's `browser.close`) the person's focus, menus and boxes stay as they
    /// are, unless the keyboard was in the tab that went: then the pane's next tab takes it.
    pub(crate) fn close_browser_tab(
        &mut self,
        tab: TabId,
        by_person: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let had_keyboard = self
            .browser_tabs
            .get(&tab)
            .is_some_and(|v| v.read(cx).focus_handle(cx).contains_focused(window, cx));
        if self.model.close_tab(tab).is_none() {
            return;
        }
        if let Some(menu) = &self.pane_tab_menu
            && menu.tab == tab
        {
            self.pane_tab_menu = None;
        }
        self.sync();
        self.close_browser_pages(cx);
        self.save();
        if self.all_gone() {
            let conns = self.subscribers.clone();
            self.quit_after_flush(conns, None, window, cx);
            return;
        }
        if by_person {
            self.activate(window, cx);
            return;
        }
        if had_keyboard {
            self.focus_active(window, cx);
        }
        self.note_focus();
        cx.notify();
    }

    /// A bar's "new terminal tab": a plain terminal at the end of `pane`'s tabs, in the group of
    /// the terminal the pane shows; or, for an SSH one, another session to its host.
    pub(crate) fn new_terminal_tab(
        &mut self,
        pane: PaneId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let shown = self
            .pane_session(pane)
            .map(|s| (s.id, s.project.clone(), s.ssh.clone()));
        let (project, ssh) = match shown {
            Some((id, project, ssh)) => (project, ssh.zip(self.index_of(id))),
            None => (self.active_project(), None),
        };
        if let Some((target, of)) = ssh {
            self.open_ssh(target, Place::Tab { of }, window, cx);
            return;
        }
        let launch = Launch {
            saved: SavedSession {
                project,
                ..Default::default()
            },
            run_preset: false,
        };
        self.open_session_then(launch, At::Tab(pane), None, window, cx);
    }

    /// A bar's split buttons: `pane` takes the focus, then splits as ⌘D / ⇧⌘D do.
    fn split_pane(
        &mut self,
        pane: PaneId,
        axis: Axis,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.model.focus_pane(pane) {
            self.sync();
        }
        self.split(axis, window, cx);
    }
}

/// A drag just starting (from `on_drag`) shows the closed hand anywhere in the window until it
/// ends: GPUI takes a drag's cursor from the dragged element's own, which would also be its
/// hover cursor (a tab's is the arrow), so it is set on the drag once GPUI has made it (right
/// after `on_drag` returns). While a drag has a cursor, every element GPUI paints asks for it
/// window-wide, over any element's own (a terminal's I-beam).
pub(crate) fn hold_closed_hand(window: &Window, cx: &mut App) {
    window.defer(cx, |window, cx| {
        cx.set_active_drag_cursor_style(CursorStyle::ClosedHand, window);
    });
}

/// One of a bar's buttons: its icon, dim until hovered; a disabled one stays faint and only
/// says why.
fn render_button(
    pane: PaneId,
    button: BarButton,
    ui: UiColors,
    cx: &mut Context<Workspace>,
) -> Div {
    let on = button.enabled();
    let group: SharedString = format!("tab-bar-button-{}-{}", pane.0, button as usize).into();
    let (dim, lit) = if on {
        (ui.text_dim, ui.text)
    } else {
        (ui.text_faint, ui.text_faint)
    };
    let kind = match button {
        BarButton::NewTerminal => Glyph::Terminal,
        BarButton::NewBrowser => Glyph::Globe,
        BarButton::SplitRight => Glyph::SplitRight,
        BarButton::SplitDown => Glyph::SplitDown,
    };
    let tip = button.tooltip();
    let icon = div()
        .relative()
        .size(px(ICON))
        .child(
            glyph(kind, dim)
                .absolute()
                .group_hover(group.clone(), |s| s.invisible()),
        )
        .child(
            glyph(kind, lit)
                .absolute()
                .invisible()
                .group_hover(group.clone(), |s| s.visible()),
        );
    div().child(
        div()
            .id(SharedString::from(format!("{group}-hit")))
            .group(group)
            .flex()
            .items_center()
            .justify_center()
            .size(px(BUTTON))
            .rounded(px(4.0))
            .when(on, |b| {
                b.cursor_pointer()
                    .hover(move |s| s.bg(hsla(ui.row_hover)))
                    .on_click(
                        cx.listener(move |this, _: &ClickEvent, window, cx| match button {
                            BarButton::NewTerminal => this.new_terminal_tab(pane, window, cx),
                            BarButton::NewBrowser => this.new_browser_tab(pane, window, cx),
                            BarButton::SplitRight => this.split_pane(pane, Axis::Row, window, cx),
                            BarButton::SplitDown => this.split_pane(pane, Axis::Column, window, cx),
                        }),
                    )
            })
            // Its press is its own (not the bar's, which focuses the pane first).
            .on_mouse_down(MouseButton::Left, |_: &MouseDownEvent, _, cx| {
                cx.stop_propagation()
            })
            .tooltip(move |_, cx| -> AnyView { cx.new(|_| Tip(tip.into())).into() })
            .child(icon),
    )
}

/// The small line icons the tab bars (and the browser tabs' bars) draw.
#[derive(Clone, Copy)]
pub(crate) enum Glyph {
    /// `>_`
    Terminal,
    /// A circle with a meridian and the equator.
    Globe,
    /// A box split by a line down its middle, and by one across it.
    SplitRight,
    SplitDown,
    /// Chevrons left and right.
    Back,
    Forward,
    /// Three quarters of a circle with an arrowhead.
    Reload,
    /// A cross.
    Stop,
    /// A magnifying glass.
    Search,
    /// The sidebar footer's three (drawn in a 16 px box, `footer_glyph`): a plus, a bell (a
    /// dome open at the bottom, its rim and clapper) and a gear (a ring with teeth around a hub).
    Plus,
    Bell,
    Gear,
    /// The Update Available button's (a 16 px box, as the footer's): a package, an isometric box
    /// (its outline, the edges meeting at its front corner) with a tape line across its top.
    Package,
}

/// `kind` in `color`, in a 12 px box.
pub(crate) fn glyph(kind: Glyph, color: Rgb) -> Div {
    glyph_with(kind, color, STROKE)
}

/// `glyph` with lines `stroke` px wide (in the 12 px box).
pub(crate) fn glyph_with(kind: Glyph, color: Rgb, stroke: f32) -> Div {
    glyph_sized(kind, color, stroke, ICON)
}

/// `kind` in `color`, in a `size` px box, the line `stroke` px wide at that size.
pub(crate) fn footer_glyph(kind: Glyph, color: Rgb, size: f32, stroke: f32) -> Div {
    glyph_sized(kind, color, stroke * ICON / size, size)
}

/// `glyph_with` in a `size` px box (the 12-unit drawing scaled up to it).
fn glyph_sized(kind: Glyph, color: Rgb, stroke: f32, size: f32) -> Div {
    div().flex_none().size(px(size)).child(
        canvas(
            |_, _, _| (),
            move |bounds, _, window, _| {
                let (o, size) = (bounds.origin, bounds.size.width);
                // `x`, `y` in a 12-unit box.
                let u = f32::from(size) / ICON;
                let p =
                    |x: f32, y: f32| -> Point<Pixels> { point(o.x + px(x * u), o.y + px(y * u)) };
                let mut paths: Vec<PathBuilder> = Vec::new();
                let stroke = || PathBuilder::stroke(px(stroke * u));
                match kind {
                    Glyph::Terminal => {
                        let mut chevron = stroke();
                        chevron.move_to(p(1.5, 2.5));
                        chevron.line_to(p(5.5, 6.0));
                        chevron.line_to(p(1.5, 9.5));
                        paths.push(chevron);
                        let mut cursor = stroke();
                        cursor.move_to(p(6.5, 10.0));
                        cursor.line_to(p(11.0, 10.0));
                        paths.push(cursor);
                    }
                    Glyph::Globe => {
                        let (c, r) = ((6.0, 6.0), 5.0);
                        let ellipse = |rx: f32| {
                            let mut e = stroke();
                            let radii = point(px(rx * u), px(r * u));
                            e.move_to(p(c.0, c.1 - r));
                            e.arc_to(radii, px(0.0), false, true, p(c.0, c.1 + r));
                            e.arc_to(radii, px(0.0), false, true, p(c.0, c.1 - r));
                            e.close();
                            e
                        };
                        paths.push(ellipse(r));
                        paths.push(ellipse(2.2));
                        let mut equator = stroke();
                        equator.move_to(p(c.0 - r, c.1));
                        equator.line_to(p(c.0 + r, c.1));
                        paths.push(equator);
                    }
                    Glyph::Back | Glyph::Forward => {
                        let (tip, wing) = if matches!(kind, Glyph::Back) {
                            (3.5, 8.0)
                        } else {
                            (8.5, 4.0)
                        };
                        let mut chevron = stroke();
                        chevron.move_to(p(wing, 1.5));
                        chevron.line_to(p(tip, 6.0));
                        chevron.line_to(p(wing, 10.5));
                        paths.push(chevron);
                    }
                    Glyph::Reload => {
                        let (c, r) = ((6.0, 6.0), 4.5);
                        let mut arc = stroke();
                        arc.move_to(p(c.0 + r, c.1));
                        let radii = point(px(r * u), px(r * u));
                        arc.arc_to(radii, px(0.0), true, true, p(c.0, c.1 - r));
                        paths.push(arc);
                        let mut head = stroke();
                        head.move_to(p(c.0 - 1.8, c.1 - r - 1.8));
                        head.line_to(p(c.0, c.1 - r));
                        head.line_to(p(c.0 - 1.8, c.1 - r + 1.8));
                        paths.push(head);
                    }
                    Glyph::Stop => {
                        for (a, b) in [((2.5, 2.5), (9.5, 9.5)), ((9.5, 2.5), (2.5, 9.5))] {
                            let mut line = stroke();
                            line.move_to(p(a.0, a.1));
                            line.line_to(p(b.0, b.1));
                            paths.push(line);
                        }
                    }
                    Glyph::Search => {
                        let (c, r) = ((5.0, 5.0), 3.5);
                        let mut lens = stroke();
                        let radii = point(px(r * u), px(r * u));
                        lens.move_to(p(c.0, c.1 - r));
                        lens.arc_to(radii, px(0.0), false, true, p(c.0, c.1 + r));
                        lens.arc_to(radii, px(0.0), false, true, p(c.0, c.1 - r));
                        lens.close();
                        paths.push(lens);
                        let mut handle = stroke();
                        handle.move_to(p(7.6, 7.6));
                        handle.line_to(p(10.8, 10.8));
                        paths.push(handle);
                    }
                    Glyph::Plus => {
                        for (a, b) in [((0.5, 6.0), (11.5, 6.0)), ((6.0, 0.5), (6.0, 11.5))] {
                            let mut line = stroke();
                            line.move_to(p(a.0, a.1));
                            line.line_to(p(b.0, b.1));
                            paths.push(line);
                        }
                    }
                    Glyph::Bell => {
                        // The dome and its sides, then the rim across their feet, the clapper
                        // under it.
                        let (c, r, foot) = ((6.0, 4.5), 3.8, 9.3);
                        let mut body = stroke();
                        body.move_to(p(c.0 - r, foot));
                        body.line_to(p(c.0 - r, c.1));
                        let radii = point(px(r * u), px(r * u));
                        body.arc_to(radii, px(0.0), false, true, p(c.0 + r, c.1));
                        body.line_to(p(c.0 + r, foot));
                        paths.push(body);
                        let mut rim = stroke();
                        rim.move_to(p(0.9, foot));
                        rim.line_to(p(11.1, foot));
                        paths.push(rim);
                        let mut clapper = stroke();
                        clapper.move_to(p(4.8, 11.2));
                        clapper.line_to(p(7.2, 11.2));
                        paths.push(clapper);
                    }
                    Glyph::Gear => {
                        let c = (6.0, 6.0);
                        let circle = |r: f32| {
                            let mut e = stroke();
                            let radii = point(px(r * u), px(r * u));
                            e.move_to(p(c.0, c.1 - r));
                            e.arc_to(radii, px(0.0), false, true, p(c.0, c.1 + r));
                            e.arc_to(radii, px(0.0), false, true, p(c.0, c.1 - r));
                            e.close();
                            e
                        };
                        // The ring, the hub, and eight teeth between them and out past the ring.
                        paths.push(circle(3.7));
                        paths.push(circle(1.5));
                        for k in 0..8 {
                            let a = k as f32 * std::f32::consts::FRAC_PI_4;
                            let (dx, dy) = (a.cos(), a.sin());
                            let mut tooth = stroke();
                            tooth.move_to(p(c.0 + dx * 3.7, c.1 + dy * 3.7));
                            tooth.line_to(p(c.0 + dx * 5.5, c.1 + dy * 5.5));
                            paths.push(tooth);
                        }
                    }
                    Glyph::Package => {
                        // The box's six outer corners, clockwise from the top; its front corner.
                        let (top, right_up, right_down) = ((6.0, 0.75), (11.25, 3.4), (11.25, 8.6));
                        let (bottom, left_down, left_up) = ((6.0, 11.25), (0.75, 8.6), (0.75, 3.4));
                        let front = (6.0, 6.0);
                        let mut outline = stroke();
                        outline.add_polygon(
                            &[top, right_up, right_down, bottom, left_down, left_up]
                                .map(|(x, y)| p(x, y)),
                            true,
                        );
                        paths.push(outline);
                        // The top face's front edges, and the edge down the front corner.
                        let mut edges = stroke();
                        edges.move_to(p(left_up.0, left_up.1));
                        edges.line_to(p(front.0, front.1));
                        edges.line_to(p(right_up.0, right_up.1));
                        edges.move_to(p(front.0, front.1));
                        edges.line_to(p(bottom.0, bottom.1));
                        paths.push(edges);
                        // The tape: from the middle of the back left edge to the middle of the
                        // front right one.
                        let mid =
                            |a: (f32, f32), b: (f32, f32)| ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0);
                        let (from, to) = (mid(top, left_up), mid(front, right_up));
                        let mut tape = stroke();
                        tape.move_to(p(from.0, from.1));
                        tape.line_to(p(to.0, to.1));
                        paths.push(tape);
                    }
                    Glyph::SplitRight | Glyph::SplitDown => {
                        let mut frame = stroke();
                        frame.add_polygon(
                            &[p(1.0, 1.5), p(11.0, 1.5), p(11.0, 10.5), p(1.0, 10.5)],
                            true,
                        );
                        paths.push(frame);
                        let mut line = stroke();
                        if matches!(kind, Glyph::SplitRight) {
                            line.move_to(p(6.0, 1.5));
                            line.line_to(p(6.0, 10.5));
                        } else {
                            line.move_to(p(1.0, 6.0));
                            line.line_to(p(11.0, 6.0));
                        }
                        paths.push(line);
                    }
                }
                for path in paths {
                    if let Ok(path) = path.build() {
                        window.paint_path(path, hsla(color));
                    }
                }
            },
        )
        .size_full(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Theme;

    fn ui() -> UiColors {
        Theme::default().ui()
    }

    fn terminal(label: &str, status: Status, secs: u64) -> Option<TerminalFacts> {
        Some(TerminalFacts {
            label: label.to_string().into(),
            status,
            age: Duration::from_secs(secs),
        })
    }

    #[test]
    fn a_busy_terminal_shows_its_mark_an_idle_one_its_icon_a_browser_a_globe() {
        let t = TabKind::Terminal(1);
        assert_eq!(icon(&t, Status::Idle), TabIcon::Terminal);
        for s in [Status::Working, Status::Waiting, Status::Done] {
            assert_eq!(icon(&t, s), TabIcon::Mark(s));
        }
        let b = TabKind::Browser { url: None };
        assert_eq!(icon(&b, Status::Idle), TabIcon::Globe);
        assert_eq!(icon(&b, Status::Waiting), TabIcon::Globe);
    }

    #[test]
    fn the_tooltip_adds_status_and_age_while_busy() {
        assert_eq!(
            tip("shell", Status::Idle, Duration::from_secs(900)),
            "shell"
        );
        assert_eq!(
            tip("claude", Status::Waiting, Duration::from_secs(4 * 60 + 12)),
            "claude — waiting 4m"
        );
        assert_eq!(
            tip("codex", Status::Working, Duration::from_secs(32)),
            "codex — working 32s"
        );
    }

    #[test]
    fn bar_tabs_follow_the_pane_in_order_with_titles_icons_and_the_active_one() {
        let pane = Pane {
            id: crate::layout_model::PaneId(1),
            tabs: vec![
                crate::layout_model::Tab {
                    id: TabId(10),
                    kind: TabKind::Terminal(1),
                },
                crate::layout_model::Tab {
                    id: TabId(11),
                    kind: TabKind::Browser { url: None },
                },
                crate::layout_model::Tab {
                    id: TabId(12),
                    kind: TabKind::Terminal(2),
                },
                crate::layout_model::Tab {
                    id: TabId(13),
                    kind: TabKind::Terminal(9),
                },
            ],
            active: TabId(12),
        };
        let tabs = bar_tabs(
            &pane,
            |id| match id {
                1 => terminal("shell", Status::Idle, 0),
                2 => terminal("claude", Status::Waiting, 95),
                _ => None,
            },
            |_| None,
        );
        let ids: Vec<TabId> = tabs.iter().map(|t| t.tab).collect();
        assert_eq!(ids, [TabId(10), TabId(11), TabId(12), TabId(13)]);
        let titles: Vec<&str> = tabs.iter().map(|t| t.title.as_ref()).collect();
        assert_eq!(titles, ["shell", BROWSER_TITLE, "claude", "Terminal"]);
        let icons: Vec<TabIcon> = tabs.iter().map(|t| t.icon).collect();
        assert_eq!(
            icons,
            [
                TabIcon::Terminal,
                TabIcon::Globe,
                TabIcon::Mark(Status::Waiting),
                TabIcon::Terminal
            ]
        );
        let active: Vec<bool> = tabs.iter().map(|t| t.active).collect();
        assert_eq!(active, [false, false, true, false]);
        assert_eq!(tabs[0].terminal, Some(1));
        assert_eq!(tabs[1].terminal, None);
        assert_eq!(tabs[1].status, Status::Idle);
        assert_eq!(tabs[2].tip, "claude — waiting 1m");
        assert_eq!(tabs[3].tip, "Terminal");
    }

    #[test]
    fn the_active_tab_is_raised_and_lit_only_in_the_focused_pane() {
        let ui = ui();
        let active = look(&ui, Status::Idle, true, true);
        assert_eq!(active.fill, Some(ui.panel));
        assert_eq!(active.text, ui.text);
        assert_eq!(active.hover, None);
        assert!(active.top_accent);
        // The same tab in a pane without the focus: still raised, no accent line.
        let elsewhere = look(&ui, Status::Idle, true, false);
        assert_eq!(elsewhere.fill, Some(ui.panel));
        assert!(!elsewhere.top_accent);
        // An inactive tab: the bar's colour, dim text, a hover fill; never the accent.
        for focused in [true, false] {
            let other = look(&ui, Status::Working, false, focused);
            assert_eq!(other.fill, None);
            assert_eq!(other.text, ui.text_dim);
            assert_eq!(other.hover, Some(ui.row_hover));
            assert!(!other.top_accent);
        }
    }

    #[test]
    fn a_waiting_tab_is_tinted_active_or_not() {
        let ui = ui();
        let other = look(&ui, Status::Waiting, false, true);
        assert_eq!(other.fill, Some(ui.waiting_tint));
        assert_eq!(other.hover, Some(ui.waiting_hover));
        assert_eq!(other.text, ui.text_dim);
        let active = look(&ui, Status::Waiting, true, true);
        assert_eq!(active.fill, Some(ui.waiting_hover));
        assert_eq!(active.text, ui.text);
        assert!(active.top_accent);
    }

    #[test]
    fn the_buttons_are_in_order_and_all_enabled() {
        assert_eq!(
            BarButton::ALL.map(BarButton::tooltip),
            [
                "New terminal tab",
                "New browser tab",
                "Split right",
                "Split down"
            ]
        );
        assert_eq!(
            BarButton::ALL.map(BarButton::enabled),
            [true, true, true, true]
        );
    }

    #[test]
    fn the_gap_shows_where_a_dragged_tab_would_land() {
        // codex (2 of 4, 120 px) dragged along its own bar: the gap moves, the tabs between
        // slide aside.
        let own = Some(2);
        assert_eq!(
            tab_offsets(4, own, Landing::Here(1), 120.0),
            [0.0, 120.0, 0.0, 0.0]
        );
        assert_eq!(
            tab_offsets(4, own, Landing::Here(3), 120.0),
            [0.0, 0.0, 0.0, -120.0]
        );
        // Over another pane's bar, the gap opens there (before its second tab) ...
        assert_eq!(
            tab_offsets(3, None, Landing::Here(1), 120.0),
            [0.0, 120.0, 120.0]
        );
        // ... and its own bar closes up; as over a sidebar row that takes it.
        assert_eq!(
            tab_offsets(4, own, Landing::Elsewhere, 120.0),
            [0.0, 0.0, 0.0, -120.0]
        );
        // Over nothing that takes it: the gap is back at its own place.
        assert_eq!(tab_offsets(4, own, Landing::Nowhere, 120.0), [0.0; 4]);
        // A bar it isn't over and didn't come from: still.
        assert_eq!(tab_offsets(3, None, Landing::Elsewhere, 120.0), [0.0; 3]);
        assert_eq!(tab_offsets(3, None, Landing::Nowhere, 120.0), [0.0; 3]);
    }

    #[test]
    fn a_drop_puts_the_tab_exactly_where_the_gap_was() {
        use crate::layout_model::{Model, Tab};
        use crate::split_tree::Axis;
        let term = |id| TabKind::Terminal(id);
        let mut m = Model::new();
        let a = m.new_workspace(0, term(1)).unwrap();
        for id in 2..=4 {
            m.add_tab(a.pane, term(id)).unwrap();
        }
        let b = m.split(a.pane, Axis::Row, term(5)).unwrap();
        m.add_tab(b.pane, term(6)).unwrap();
        let ids = |m: &Model, pane| -> Vec<TabId> {
            m.pane(pane)
                .unwrap()
                .tabs
                .iter()
                .map(|t: &Tab| t.id)
                .collect()
        };
        let left = ids(&m, a.pane);
        // Every tab of the left bar to every slot of its own bar: it ends up at that slot (its
        // own slot: nothing to do, a click), and the others keep their order.
        for (at, &tab) in left.iter().enumerate() {
            for slot in 0..left.len() {
                let gap = gap_for_slot(slot, Some(at));
                let drop = TabDrop::Bar { pane: a.pane, gap };
                let mut moved = m.clone();
                match m.drop_place(tab, drop) {
                    None => assert_eq!(slot, at, "{at} to {slot}"),
                    Some((pane, index)) => {
                        assert_eq!((pane, index), (a.pane, slot), "{at} to {slot}");
                        moved.move_tab(tab, pane, index).unwrap();
                        let after = ids(&moved, a.pane);
                        assert_eq!(after[slot], tab, "{at} to {slot}");
                        let mut others = left.clone();
                        others.retain(|t| *t != tab);
                        let mut rest = after.clone();
                        rest.retain(|t| *t != tab);
                        assert_eq!(rest, others, "{at} to {slot}");
                    }
                }
            }
        }
        // Into the other bar: the slot is the index there.
        let right = ids(&m, b.pane);
        for slot in 0..=right.len() {
            let drop = TabDrop::Bar {
                pane: b.pane,
                gap: gap_for_slot(slot, None),
            };
            let (pane, index) = m.drop_place(left[1], drop).unwrap();
            assert_eq!((pane, index), (b.pane, slot));
            let mut moved = m.clone();
            moved.move_tab(left[1], pane, index).unwrap();
            assert_eq!(ids(&moved, b.pane)[slot], left[1]);
        }
    }

    #[test]
    fn a_browser_tab_shows_its_page_s_title() {
        let pane = Pane {
            id: crate::layout_model::PaneId(1),
            tabs: vec![
                crate::layout_model::Tab {
                    id: TabId(10),
                    kind: TabKind::Browser { url: None },
                },
                crate::layout_model::Tab {
                    id: TabId(11),
                    kind: TabKind::Browser {
                        url: Some("https://docs.rs/".into()),
                    },
                },
            ],
            active: TabId(11),
        };
        let tabs = bar_tabs(
            &pane,
            |_| None,
            |tab| (tab == TabId(11)).then(|| "Docs.rs".into()),
        );
        let titles: Vec<&str> = tabs.iter().map(|t| t.title.as_ref()).collect();
        assert_eq!(titles, [BROWSER_TITLE, "Docs.rs"]);
        assert_eq!(tabs[1].tip, "Docs.rs");
    }
}
