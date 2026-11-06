//! Pane tab bars (M11b): a 26 px bar on top of every pane (even a lone one, as cmux does), in
//! place of M11's pane headers. Each tab shows an icon (a terminal's `>_`, a browser's globe), its
//! title, and for a terminal whose agent is Working, Waiting or Done its status mark in the
//! icon's place; × closes it (shown on hover). The active tab is filled and, in the focused pane,
//! has an accent line along its top; a waiting tab is tinted. Too many tabs shrink to a minimum
//! width, then the row scrolls sideways. On the right, pinned: new terminal tab, new browser tab
//! split right, split down.
//!
//! `bar_tabs`, `icon`, `tip`, `look` and `BarButton` are the pure model; the rest draws the bar
//! and acts on its clicks.

use std::time::{Duration, Instant};

use alacritty_terminal::vte::ansi::Rgb;
use gpui::{
    AnyElement, AnyView, ClickEvent, Context, Div, Focusable, MouseButton, MouseDownEvent,
    PathBuilder, Pixels, Point, ScrollHandle, SharedString, Window, canvas, div, point, prelude::*,
    px,
};

use crate::layout_model::{Pane, PaneId, TabId, TabKind};
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
                    .map(|tab| self.render_tab(tab, ui, focused, cx)),
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
    /// along the top of the focused pane's active tab.
    fn render_tab(
        &self,
        tab: BarTab,
        ui: UiColors,
        pane_focused: bool,
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
        div()
            .id(("pane-tab", id.0 as usize))
            .group(group)
            .relative()
            .flex()
            .flex_row()
            .items_center()
            .flex_basis(px(TAB_MAX_WIDTH))
            .flex_shrink_1()
            .min_w(px(TAB_MIN_WIDTH))
            .max_w(px(TAB_MAX_WIDTH))
            .h_full()
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
            .tooltip(move |_, cx| -> AnyView { cx.new(|_| Tip(tip.clone().into())).into() })
            .into_any_element()
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
    fn new_terminal_tab(&mut self, pane: PaneId, window: &mut Window, cx: &mut Context<Self>) {
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
}

/// `kind` in `color`, in a 12 px box.
pub(crate) fn glyph(kind: Glyph, color: Rgb) -> Div {
    glyph_with(kind, color, STROKE)
}

/// `glyph` with lines `stroke` px wide (in the 12 px box).
pub(crate) fn glyph_with(kind: Glyph, color: Rgb, stroke: f32) -> Div {
    div().flex_none().size(px(ICON)).child(
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
