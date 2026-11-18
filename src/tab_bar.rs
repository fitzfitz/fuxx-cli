//! Pane tab bars (M11b): a bar on top of every pane (even a lone one, as cmux does), in place of
//! M11's pane headers. Since M11al (style E, "minimal") the bar is `TAB_BAR` (52 pt) with no
//! fill of its own (the card shows through) and a 1 pt `card_border` line along its bottom. Its
//! tabs float in its middle, `TAB` tall and rounded: each shows an icon tile (a terminal's `>_`,
//! a browser's globe, a file's page, an agent's provider letters), its title, and after it a
//! small status dot while its terminal or agent is Working, Waiting or Done; × closes it (shown
//! on hover). Thin separators stand between the tabs. The shown tab has the accent wash
//! (`tab_active`; in an unfocused split pane a neutral one, `card_hover`); a waiting one that
//! isn't shown is tinted. Too many tabs shrink to a minimum width, then the row scrolls
//! sideways. On the right, pinned: + (a new terminal tab), ⋯ (the pane's actions,
//! `pane_actions`) and the right sidebar's (M11g; lit while it's open).
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
use std::cell::Cell;
use std::rc::Rc;

use gpui::{
    AnyElement, AnyView, App, Bounds, ClickEvent, Context, CursorStyle, Div, DragMoveEvent,
    Focusable, FontWeight, MouseButton, MouseDownEvent, PathBuilder, Pixels, Point, ScrollHandle,
    SharedString, Window, canvas, div, point, prelude::*, px, relative,
};

use crate::appearance::Appearance;
use crate::block_view::BlockDrag;
use crate::drag_gap::{self, Slid};
use crate::layout_model::{Pane, PaneId, TabDrop, TabId, TabKind};
use crate::restore::{Launch, SavedSession};
use crate::sidebar::AGENT_MARK;
use crate::sidebar_toggle::Tip;
use crate::split_tree::Axis;
use crate::status::{self, Status};
use crate::status_mark;
use crate::terminal_view::hsla;
use crate::theme::UiColors;
use crate::ui_tokens::MARK_MIN;
use crate::ui_tokens::size::{HAIRLINE, TAB_BAR, TABS_LEFT};
use crate::ui_tokens::text;
use crate::workspace::{At, Renaming, Workspace};
use crate::workspace_ssh::Place;

/// A tab's width: it shrinks from the most toward the least as tabs are added, then the row
/// scrolls.
pub const TAB_MIN_WIDTH: f32 = 100.0;
pub const TAB_MAX_WIDTH: f32 = 220.0;
/// A browser tab's title until it shows a page.
pub const BROWSER_TITLE: &str = "New tab";

/// What a tab's icon tile shows, left of its title (M11al: whatever its status; the status dot
/// follows the title).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabIcon {
    Terminal,
    Globe,
    /// A file tab's (M11g): a page with a folded corner.
    File,
    /// An agent tab's (M11aa): its provider's badge, two letters in its colour.
    Agent {
        badge: &'static str,
        rgb: u32,
    },
}

/// What the bar knows of a terminal: its name (`Workspace::terminal_label`), the program's own
/// title (OSC; for the tooltip), its status and for how long it has had it.
pub struct TerminalFacts {
    pub label: SharedString,
    pub title: Option<SharedString>,
    pub status: Status,
    pub age: Duration,
}

/// What the bar knows of an agent tab (the Workspace's `AgentSlot`, R1): its provider's label
/// and badge, its status and for how long it has had it.
pub struct AgentFacts {
    pub label: SharedString,
    pub badge: &'static str,
    pub rgb: u32,
    pub status: Status,
    pub age: Duration,
}

/// One tab as the bar draws it.
#[derive(Clone, Debug, PartialEq)]
pub struct BarTab {
    pub tab: TabId,
    /// The terminal it shows (`sN`); none for a browser or file tab.
    pub terminal: Option<u64>,
    pub title: SharedString,
    pub icon: TabIcon,
    /// Idle for a browser or file tab.
    pub status: Status,
    pub active: bool,
    /// Its tooltip: the title, and for a busy terminal its status and for how long; a file
    /// tab's full path.
    pub tip: String,
    /// Its pane's preview tab (M11t): its title in italics.
    pub preview: bool,
}

/// The icon for a tab of `kind`.
pub fn icon(kind: &TabKind) -> TabIcon {
    match kind {
        TabKind::Browser { .. } => TabIcon::Globe,
        TabKind::File { .. } => TabIcon::File,
        TabKind::Agent { provider, .. } => {
            let (_, badge, rgb) = crate::workspace_agent::provider_look(provider);
            TabIcon::Agent { badge, rgb }
        }
        TabKind::Terminal(_) => TabIcon::Terminal,
    }
}

/// A tab's tooltip: `title`, then while not Idle its status and age ("claude — waiting 4m");
/// under it, the program's own title (`osc`) when it says something the name doesn't.
pub fn tip(title: &str, osc: Option<&str>, status: Status, age: Duration) -> String {
    let line = match status {
        Status::Idle => title.to_string(),
        _ => format!("{title} — {} {}", status.as_str(), status::age_label(age)),
    };
    match crate::groups::extra_title(title, osc) {
        Some(osc) => format!("{line}\n{osc}"),
        None => line,
    }
}

/// `pane`'s tabs in bar order. `facts` tells about a terminal; one it doesn't know (never, while
/// tabs and sessions are in step) is drawn as an idle "Terminal". `page` gives a browser tab's
/// title (none: "New tab"). A file tab's title is its file's name, its tooltip the full path.
/// `agent` tells about an agent tab: its title is yours or its first message's, else its
/// provider's label; its icon its provider's badge.
pub fn bar_tabs(
    pane: &Pane,
    facts: impl Fn(u64) -> Option<TerminalFacts>,
    page: impl Fn(TabId) -> Option<SharedString>,
    agent: impl Fn(TabId) -> Option<AgentFacts>,
) -> Vec<BarTab> {
    pane.tabs
        .iter()
        .map(|t| {
            let idle = (Status::Idle, Duration::ZERO);
            let (terminal, title, osc, (status, age)): (_, SharedString, _, _) = match &t.kind {
                TabKind::Terminal(id) => match facts(*id) {
                    Some(f) => (Some(*id), f.label, f.title, (f.status, f.age)),
                    None => (Some(*id), "Terminal".into(), None, idle),
                },
                TabKind::Browser { .. } => {
                    let title = page(t.id).unwrap_or_else(|| BROWSER_TITLE.into());
                    (None, title, None, idle)
                }
                TabKind::File { path, .. } => (None, file_title(path), None, idle),
                TabKind::Agent {
                    provider, title, ..
                } => {
                    let facts = agent(t.id);
                    let label = facts.as_ref().map_or_else(
                        || crate::workspace_agent::provider_look(provider).0,
                        |f| f.label.clone(),
                    );
                    let title = title.clone().map_or(label, SharedString::from);
                    let status = facts.as_ref().map_or(idle, |f| (f.status, f.age));
                    (None, title, None, status)
                }
            };
            let tip = match &t.kind {
                TabKind::File { path, .. } => path.display().to_string(),
                _ => tip(&title, osc.as_deref(), status, age),
            };
            // An agent tab's badge: its slot's (the provider's).
            let icon = match (&t.kind, agent(t.id)) {
                (TabKind::Agent { .. }, Some(f)) => TabIcon::Agent {
                    badge: f.badge,
                    rgb: f.rgb,
                },
                _ => icon(&t.kind),
            };
            BarTab {
                tab: t.id,
                terminal,
                icon,
                tip,
                title,
                status,
                active: t.id == pane.active,
                preview: false,
            }
        })
        .collect()
}

/// A file tab's title: its file's name (the whole path when it has none, as `/`).
fn file_title(path: &std::path::Path) -> SharedString {
    path.file_name()
        .unwrap_or(path.as_os_str())
        .to_string_lossy()
        .into_owned()
        .into()
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
    /// Its fill; none: the card shows through.
    pub fill: Option<Rgb>,
    /// Its fill under the pointer; none: no change.
    pub hover: Option<Rgb>,
    pub text: Rgb,
}

/// The look of a tab with `status`, `active` in its pane or not, in the focused pane or not
/// (M11al §3.3, §4.2). The shown tab has full text and a wash: the accent's (`tab_active`) in
/// the focused pane, a neutral one (`card_hover`) in another (no per-pane ring shows the focus
/// now). The others have no fill (the card shows through), faint text and the card's hover
/// shade; a waiting one keeps its tint.
pub fn look(ui: &UiColors, status: Status, active: bool, pane_focused: bool) -> Look {
    let waiting = status == Status::Waiting;
    let (fill, hover) = match (active, waiting) {
        (true, _) if pane_focused => (Some(ui.tab_active), None),
        (true, _) => (Some(ui.card_hover), None),
        (false, false) => (None, Some(ui.card_hover)),
        (false, true) => (Some(ui.waiting_tint), Some(ui.waiting_hover)),
    };
    Look {
        fill,
        hover,
        text: if active { ui.text } else { ui.text_faint },
    }
}

/// The colour of the glyph in a tab's icon tile (M11al §4.3): a terminal's and a browser's the
/// accent, a file's `text_dim`, an agent's its provider's colour, held to `MARK_MIN` on the tile.
pub fn tile_ink(ui: &UiColors, icon: TabIcon) -> Rgb {
    match icon {
        TabIcon::Terminal | TabIcon::Globe => ui.accent,
        TabIcon::File => ui.text_dim,
        TabIcon::Agent { rgb, .. } => {
            let provider = Rgb {
                r: (rgb >> 16) as u8,
                g: (rgb >> 8) as u8,
                b: rgb as u8,
            };
            crate::theme::readable(provider, &[ui.card_lift], MARK_MIN)
        }
    }
}

/// A tab's icon tile (M11al §4.3): `TILE` square, `TILE_RADIUS` corners, `card_lift`, its glyph
/// (`TILE_GLYPH`; an agent's provider letters) in the middle in `tile_ink`.
fn tile(icon: TabIcon, ui: &UiColors) -> Div {
    let ink = tile_ink(ui, icon);
    let line = |kind| footer_glyph(kind, ink, TILE_GLYPH, STROKE).into_any_element();
    let inside = match icon {
        TabIcon::Terminal => line(Glyph::Terminal),
        TabIcon::Globe => line(Glyph::Globe),
        TabIcon::File => line(Glyph::File),
        TabIcon::Agent { badge, .. } => div()
            .text_size(px(TILE_GLYPH))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(hsla(ink))
            .child(badge)
            .into_any_element(),
    };
    div()
        .flex_none()
        .size(px(TILE))
        .rounded(px(TILE_RADIUS))
        .bg(hsla(ui.card_lift))
        .flex()
        .items_center()
        .justify_center()
        .child(inside)
}

/// The bar's buttons, left to right after the tabs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BarButton {
    NewTerminal,
    /// Opens the pane's actions menu (`pane_actions`); lit while it's open.
    More,
    /// Opens or closes the right sidebar (M11g), at the far right; lit while it's open.
    RightSidebar,
}

impl BarButton {
    pub const ALL: [BarButton; 3] = [
        BarButton::NewTerminal,
        BarButton::More,
        BarButton::RightSidebar,
    ];

    /// Whether it can be clicked (all can, since browser tabs exist).
    pub fn enabled(self) -> bool {
        true
    }

    /// Its tooltip.
    pub fn tooltip(self) -> &'static str {
        match self {
            BarButton::NewTerminal => "New terminal tab (⌘T)",
            BarButton::More => "More",
            BarButton::RightSidebar => "Toggle right sidebar (⌥⌘\\)",
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

/// The tab that follows the pointer during a drag (M11al §4.7): a chip as big as the tab
/// (`TAB` tall, `TAB_RADIUS` corners), with its tile and title on `panel`, edged and shadowed,
/// drawn where the tab was grabbed (the pointer stays on the spot it took).
struct TabChip {
    title: SharedString,
    icon: TabIcon,
    width: f32,
}

impl Render for TabChip {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ui = cx.global::<Appearance>().theme.ui();
        div()
            .flex()
            .flex_row()
            .items_center()
            .w(px(self.width))
            .h(px(TAB))
            .pl(px(TAB_PAD))
            .pr(px(TAB_PAD + CLOSE))
            .gap(px(PART_GAP))
            .rounded(px(TAB_RADIUS))
            .opacity(0.95)
            .shadow_md()
            .bg(hsla(ui.panel))
            .border_1()
            .border_color(hsla(ui.panel_border))
            .text_size(px(text::UI))
            .text_color(hsla(ui.text))
            .child(tile(self.icon, &ui))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .truncate()
                    .child(self.title.clone()),
            )
    }
}

/// M11al's minimal tabs (E, §4): a tab's height, corners, side padding and the gap between its
/// parts; the separator after each tab but the last (1 × 16 `card_border`, `SEPARATOR_ROOM`
/// clear on each side), which its place in the row holds (`AFTER`); the icon tile, its corners
/// and its glyph; the status dot after the title; the ×.
pub(crate) const TAB: f32 = 34.0;
pub(crate) const TAB_RADIUS: f32 = 8.0;
pub(crate) const TAB_PAD: f32 = 12.0;
pub(crate) const PART_GAP: f32 = 8.0;
pub(crate) const SEPARATOR: (f32, f32) = (1.0, 16.0);
pub(crate) const SEPARATOR_ROOM: f32 = 4.0;
const AFTER: f32 = 2.0 * SEPARATOR_ROOM + SEPARATOR.0;
pub(crate) const TILE: f32 = 20.0;
const TILE_RADIUS: f32 = 6.0;
const TILE_GLYPH: f32 = 11.5;
pub(crate) const STATUS_DOT: f32 = 8.0;
pub(crate) const CLOSE: f32 = 16.0;
/// A bar button, its glyph (drawn with the title area's line width) and the gap between
/// buttons and around them.
pub(crate) const BUTTON: f32 = 28.0;
pub(crate) const BUTTON_ICON: f32 = 16.0;
const BUTTON_STROKE: f32 = 1.5;
pub(crate) const BUTTON_GAP: f32 = 4.0;
pub(crate) const BUTTONS_PAD: f32 = 10.0;
/// The line icons' drawing box (12 units, scaled to the box they are drawn in) and their line
/// width in it.
pub(crate) const ICON: f32 = 12.0;
const STROKE: f32 = 1.25;

impl Workspace {
    /// Tab `tab`'s tooltip at `now` (its age then); none for a tab gone.
    pub(crate) fn tab_tip(&self, tab: TabId, now: Instant) -> Option<String> {
        let (_, pane) = self.model.locate(tab)?;
        let p = self.model.pane(pane)?;
        self.pane_bar_tabs(p, now)
            .into_iter()
            .find(|t| t.tab == tab)
            .map(|t| t.tip)
    }

    /// Pane `p`'s tabs as its bar draws them `now`: each terminal by its name
    /// (`terminal_label`), a browser tab by its page's title, a file tab by its file's name.
    pub(crate) fn pane_bar_tabs(&self, p: &Pane, now: Instant) -> Vec<BarTab> {
        bar_tabs(
            p,
            |id| {
                let s = self.sessions.items().iter().find(|s| s.id == id)?;
                Some(TerminalFacts {
                    label: self.terminal_label(id),
                    title: s.title.clone(),
                    status: s.status,
                    age: now.saturating_duration_since(s.status_since),
                })
            },
            |tab| self.browser_titles.get(&tab).map(|t| t.clone().into()),
            |tab| {
                let slot = self.agents.get(&tab)?;
                Some(AgentFacts {
                    label: slot.label.clone(),
                    badge: slot.badge,
                    rgb: slot.badge_rgb,
                    status: slot.status,
                    age: now.saturating_duration_since(slot.since),
                })
            },
        )
        .into_iter()
        .map(|mut t| {
            t.preview = self.model.is_preview(t.tab);
            t
        })
        .collect()
    }

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
        crate::chrome_cache::bump(crate::chrome_cache::Counted::TabBar);
        let p = self.model.pane(pane)?;
        let focused = self.model.focused_pane() == Some(pane);
        let now = Instant::now();
        let tabs = self.pane_bar_tabs(p, now);
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
        let count = tabs.len();
        let row = div()
            .id(("pane-tabs", pane.0 as usize))
            .flex()
            .flex_row()
            .items_center()
            .flex_1()
            .min_w(px(0.0))
            .h_full()
            // The tabs float in the bar's middle (M11al); the first starts `TABS_LEFT` in from
            // the card's edge, as the right sidebar's panel tabs.
            .pl(px(TABS_LEFT))
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
                            .map_or(TAB_MAX_WIDTH + AFTER, |b| f32::from(b.size.width));
                        let offset = self.drag_slides.borrow_mut().offset(
                            Slid::Tab(tab.tab),
                            target,
                            now,
                            animate,
                        );
                        let last = i + 1 == count;
                        self.render_tab(tab, ui, focused, offset, width, last, cx)
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
            .children(BarButton::ALL.map(|b| {
                // The right sidebar's is lit while the sidebar is open; ⋯ while its menu is.
                let lit = match b {
                    BarButton::NewTerminal => false,
                    BarButton::More => self.pane_actions.as_ref().is_some_and(|m| m.pane == pane),
                    BarButton::RightSidebar => !self.right_sidebar.hidden,
                };
                render_button(pane, b, lit, ui, cx)
            }));
        Some(
            div()
                .flex()
                .flex_row()
                .flex_none()
                .w_full()
                .h(px(TAB_BAR))
                // No fill (M11al §4.1): the card shows through, so the bar needs no rounding at
                // the card's corner either.
                .relative()
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
                // Its 1 pt `card_border` line along its bottom, over the bar's last row of
                // pixels (out of the layout, so the tabs stay in the bar's middle).
                .child(
                    div()
                        .absolute()
                        .bottom_0()
                        .left_0()
                        .w_full()
                        .h(px(HAIRLINE))
                        .bg(hsla(ui.card_border)),
                )
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

    /// One tab (M11al §4.2): its icon tile, its title (or the rename box), its status dot, × on
    /// hover; `TAB` tall, rounded, floating in the bar's middle; after it (unless `last`) a
    /// separator. It can be dragged (a press and a move of a few pixels; a click stays a click):
    /// its chip is it meanwhile, and its place shows empty. Its place is `width` wide (the tab and
    /// the separator's room, `AFTER`). It is drawn `offset` px right of its place (out of a
    /// dragged tab's way; its place in the row, which the drop rules measure, stays where it is).
    #[allow(clippy::too_many_arguments)]
    fn render_tab(
        &self,
        tab: BarTab,
        ui: UiColors,
        pane_focused: bool,
        offset: f32,
        width: f32,
        last: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let look = look(&ui, tab.status, tab.active, pane_focused);
        let shown = tab.active;
        let surface = look.fill.unwrap_or(ui.card);
        let id = tab.tab;
        let group: SharedString = format!("pane-tab-{}", id.0).into();
        let session = tab
            .terminal
            .and_then(|t| self.sessions.items().iter().find(|s| s.id == t));
        // Its status dot after its title (M11al §4.4), unless Idle: a terminal's (its `sN`) or
        // an agent tab's (R2: `AGENT_MARK`, its `aN`). Working's turning arc and Waiting's
        // breathing dot are their own views, which the marks' clock turns (M11p); Done is the
        // muted ✓ in `text_dim` (drawn still, as the sidebar's).
        let marked = match (session, self.agents.get(&tab.tab)) {
            (Some(s), _) => Some(("tab", s.id, s.status)),
            (None, Some(slot)) => Some((AGENT_MARK, slot.id, slot.status)),
            (None, None) => None,
        };
        let dot = marked.and_then(|(place, key, status)| {
            let mark = match status {
                Status::Idle => return None,
                Status::Done => {
                    status_mark::done_check(ui.text_dim, px(STATUS_DOT)).into_any_element()
                }
                _ => self.mark_view(place, key, status, px(STATUS_DOT), surface, cx),
            };
            Some(
                div()
                    .flex_none()
                    .size(px(STATUS_DOT))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(mark),
            )
        });
        let rename_box = self
            .renaming
            .as_ref()
            .filter(|(r, _)| {
                tab.terminal.is_some_and(|t| *r == Renaming::Terminal(t))
                    || *r == Renaming::Agent(tab.tab)
            })
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
                // A preview (M11t): in italics, until kept.
                .when(tab.preview, |t| t.italic())
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
        // The tooltip says the tab's age as it opens, not as the bar was last drawn (M11p: a
        // cached bar's frame, and its tooltip, can be older than the age it shows).
        let tip = tab.tip.clone();
        let workspace = cx.entity().downgrade();
        let lifted = self.dragged_tab(cx) == Some(id);
        let (source, rows) = (self.tab_drag.clone(), self.sidebar.drag_source.clone());
        let (chip_title, chip_icon) = (tab.title.clone(), tab.icon);
        let attachable = self.agents.get(&id).is_some_and(|slot| slot.attachable);
        let tile = tile(tab.icon, &ui);
        // The gap between two parts: `PART_GAP`, giving way (down to none) only once the title
        // is gone, so a shown tab squeezed into a narrow pane keeps its tile, dot and × whole.
        let part_gap = || {
            div()
                .flex_basis(px(PART_GAP))
                .flex_shrink_1()
                .min_w(px(0.0))
        };
        let tab = div()
            .id(("pane-tab", id.0 as usize))
            .group(group)
            .flex_1()
            .min_w(px(0.0))
            .h_full()
            .flex()
            .flex_row()
            .items_center()
            .px(px(TAB_PAD))
            .rounded(px(TAB_RADIUS))
            .text_size(px(text::UI))
            .text_color(hsla(look.text))
            .when_some(look.fill, |t, fill| t.bg(hsla(fill)))
            .when_some(look.hover, |t, hover| t.hover(move |s| s.bg(hsla(hover))))
            .child(tile)
            .child(part_gap())
            .child(title)
            .children(
                dot.map(|dot| [part_gap().into_any_element(), dot.into_any_element()])
                    .into_iter()
                    .flatten(),
            )
            .child(part_gap())
            .child(close)
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
                    width: width - AFTER,
                })
            })
            // Its press is its own (not the bar's, which only focuses the pane).
            .on_mouse_down(MouseButton::Left, |_: &MouseDownEvent, _, cx| {
                cx.stop_propagation()
            })
            .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                this.select_tab(id, window, cx);
                if event.click_count() == 2 {
                    match terminal {
                        Some(terminal) => this.start_tab_rename(terminal, window, cx),
                        // A preview's double click keeps it (M11t).
                        None => this.pin_tab(id, cx),
                    }
                }
            }))
            // An agent tab takes a command block dropped on it (M11aa Task 11), tinted toward the
            // accent while one is over it (GPUI's element-local style: no Workspace field changes
            // in a drag-over); dropped, the block is attached to its next message and the tab
            // shown.
            .when(attachable, |t| {
                t.drag_over::<BlockDrag>(move |s, _, _, _| s.bg(hsla(ui.drop_tab)))
                    .on_drop(cx.listener(move |this, drag: &BlockDrag, window, cx| {
                        this.attach_block(id, drag, cx);
                        this.select_tab(id, window, cx);
                    }))
            })
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
            .tooltip(move |_, cx| -> AnyView {
                let now = workspace
                    .upgrade()
                    .and_then(|ws| ws.read(cx).tab_tip(id, Instant::now()))
                    .unwrap_or_else(|| tip.clone());
                cx.new(|_| Tip(now.into())).into()
            });
        // The separator after it (M11al §4.2): 1 × 16 `card_border` in the middle of its room.
        let separator = div()
            .flex_none()
            .w(px(AFTER))
            .h_full()
            .flex()
            .items_center()
            .justify_center()
            .when(!last, |room| {
                room.child(
                    div()
                        .w(px(SEPARATOR.0))
                        .h(px(SEPARATOR.1))
                        .bg(hsla(ui.card_border)),
                )
            });
        // Its place in the row: the size the row gives it (the tab and the separator's room),
        // `TAB` tall in the bar's middle; the tab and its separator are drawn from it, slid by
        // `offset`. Its chip is it while it's dragged: its place shows empty. At least
        // `TAB_MIN_WIDTH` (then the row scrolls); but the shown tab, in a row narrower than that,
        // fits the row (its title gives way): its minimum is its content's (a strut that wide),
        // held to the row's width by its `max_w` (taffy's automatic minimum size of a flex item,
        // css-flexbox-1 §4.5, clamps the content size by the max size).
        let place = div()
            .relative()
            .flex_basis(px(TAB_MAX_WIDTH + AFTER))
            .flex_shrink_1()
            .h(px(TAB));
        let place = if shown {
            place
                .max_w(relative(1.0))
                .child(div().w(px(TAB_MIN_WIDTH + AFTER)).h(px(0.0)))
        } else {
            place
                .min_w(px(TAB_MIN_WIDTH + AFTER))
                .max_w(px(TAB_MAX_WIDTH + AFTER))
        };
        place
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left(px(offset))
                    .size_full()
                    .flex()
                    .flex_row()
                    .when(lifted, |t| t.opacity(0.0))
                    .child(tab)
                    .child(separator),
            )
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
    /// a browser tab's page goes with its view (`sync` drops it); a file tab just goes (the file
    /// is only read). Never asks. The pane's next
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
            Some(TabKind::File { .. }) => self.close_file_tab(tab, window, cx),
            // An agent tab: asks first while it works or waits for you (M11aa).
            Some(TabKind::Agent { .. }) => self.close_agent_tab(tab, true, window, cx),
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
    /// With `follow`, it starts where the pane's terminal (else the active one) is now; without,
    /// fresh in the workspace's folder (⇧⌘T).
    pub(crate) fn new_terminal_tab(
        &mut self,
        pane: PaneId,
        follow: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let shown = self.pane_session(pane).map(|s| {
            (
                s.id,
                s.project.clone(),
                s.ssh.clone(),
                follow.then(|| self.folder_now(s)).flatten(),
            )
        });
        let (project, ssh, cwd) = match shown {
            Some((id, project, ssh, cwd)) => (project, ssh.zip(self.index_of(id)), cwd),
            None => (
                self.active_project(),
                None,
                follow.then(|| self.active_folder_now()).flatten(),
            ),
        };
        if let Some((target, of)) = ssh {
            self.open_ssh(target, Place::Tab { of }, window, cx);
            return;
        }
        let launch = Launch {
            saved: SavedSession {
                project,
                cwd,
                ..Default::default()
            },
            run_preset: false,
        };
        self.open_session_then(launch, At::Tab(pane), None, window, cx);
    }

    /// Split `pane` (its ⋯ menu): it takes the focus, then splits as ⌘D / ⇧⌘D do.
    pub(crate) fn split_pane(
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

/// One of a bar's buttons (M11al §4.6: `BUTTON` square, its `BUTTON_ICON` glyph): its icon
/// faint until hovered; a disabled one stays faint and only says why. A `lit` one (the right
/// sidebar's while it's open, ⋯ while its menu is) is drawn as hovered all the time: its icon
/// in the text colour on the card's hover shade. ⋯ opens its menu
/// under itself, right edges together (it knows where it is from its last paint).
fn render_button(
    pane: PaneId,
    button: BarButton,
    lit_up: bool,
    ui: UiColors,
    cx: &mut Context<Workspace>,
) -> Div {
    let on = button.enabled();
    let group: SharedString = format!("tab-bar-button-{}-{}", pane.0, button as usize).into();
    let (dim, lit) = match (on, lit_up) {
        (true, true) => (ui.text, ui.text),
        (true, false) => (ui.text_faint, ui.text),
        (false, _) => (ui.text_faint, ui.text_faint),
    };
    let kind = match button {
        BarButton::NewTerminal => Glyph::Plus,
        BarButton::More => Glyph::More,
        BarButton::RightSidebar => Glyph::RightSidebar,
    };
    let tip = button.tooltip();
    // Where it was last painted: the ⋯ menu opens under it.
    let placed: Rc<Cell<Option<Bounds<Pixels>>>> = Rc::default();
    let place = placed.clone();
    let icon = div()
        .relative()
        .size(px(BUTTON_ICON))
        .child(
            footer_glyph(kind, dim, BUTTON_ICON, BUTTON_STROKE)
                .absolute()
                .group_hover(group.clone(), |s| s.invisible()),
        )
        .child(
            footer_glyph(kind, lit, BUTTON_ICON, BUTTON_STROKE)
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
            .relative()
            .size(px(BUTTON))
            .rounded(px(4.0))
            .when(on && lit_up, |b| b.bg(hsla(ui.card_hover)))
            .when(on, |b| {
                b.cursor_pointer()
                    .hover(move |s| s.bg(hsla(ui.card_hover)))
                    .on_click(
                        cx.listener(move |this, _: &ClickEvent, window, cx| match button {
                            BarButton::NewTerminal => this.new_terminal_tab(pane, true, window, cx),
                            BarButton::More => {
                                if let Some(b) = placed.get() {
                                    this.open_pane_actions(pane, b.bottom_right(), window, cx)
                                }
                            }
                            BarButton::RightSidebar => this.right_sidebar_button(pane, window, cx),
                        }),
                    )
            })
            .when(button == BarButton::More, |b| {
                b.child(
                    canvas(move |bounds, _, _| place.set(Some(bounds)), |_, _, _, _| {})
                        .absolute()
                        .size_full(),
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
    /// A page with its top right corner folded over.
    File,
    /// Three dots in a row (a pane bar's ⋯, M11i).
    More,
    /// The right sidebar's: a box with its right side filled.
    RightSidebar,
    /// Chevrons left and right.
    Back,
    Forward,
    /// Three quarters of a circle with an arrowhead.
    Reload,
    /// A cross.
    Stop,
    /// A magnifying glass.
    Search,
    /// The sidebar footer's three (drawn in a 16 px box, `footer_glyph`): a plus (also a pane
    /// bar's +, in 12), a bell (a dome open at the bottom, its rim and clapper) and a gear (a
    /// ring with teeth around a hub).
    Plus,
    Bell,
    Gear,
    /// The update pill's (a 12 px box in the status bar, M11i): a package, an isometric box
    /// (its outline, the edges meeting at its front corner) with a tape line across its top.
    Package,
    /// The Files tree's (M11i): a folder (a box with a tab on its top left) and an open one
    /// (its back, and its front flap leaning out); the header's collapse all, two chevrons
    /// pointing up.
    Folder,
    FolderOpen,
    CollapseAll,
    /// The status bar's branch (M11i): a small fork, a trunk with a commit at each end and a
    /// branch leaving it for a third.
    Branch,
    /// The Changes bar's actions (M11v): an arrow down onto a line (Pull), an arrow up from a
    /// line (Push), and a branch with an arrow going up beside it (Publish). Fetch is `Reload`.
    Pull,
    Push,
    Publish,
    /// The Changes rows' revert (M11v part 2): an arrow back to the left, turning down and back
    /// under itself (a U-turn).
    Revert,
    /// M11v part 4: the thin line chevrons that replace the ▸ / ▾ triangles (› closed, ⌄ open).
    ChevronRight,
    ChevronDown,
    /// The file bar's (M11ae): Wrap, three lines, the last ending in a return arrow (down from
    /// the right, back to the left); Open in Editor, a box with an arrow leaving its top right.
    Wrap,
    OpenExternal,
    /// The file bar's jumps between changes (M11ah): an arrow up and an arrow down, a shaft and
    /// a head with no line.
    ChangeUp,
    ChangeDown,
    /// The file bar's Side by side (M11ah): a box split in two columns.
    SideBySide,
    /// A code block's Copy in the Markdown reader (M11ai): two sheets, the front one whole, the
    /// back one showing at its top right; and Copied's tick.
    Copy,
    Check,
    /// The narrow "On this page" button (M11ai Task 9): three lines, the middle one indented
    /// (an outline's subheading).
    Outline,
    /// The Markdown reader's callouts (M11ai): a circle with an i (Note), a bulb (Tip), a speech
    /// box with a ! (Important), a triangle with a ! (Warning) and an octagon with a ! (Caution;
    /// `Stop` is a plain cross).
    Info,
    Bulb,
    Report,
    Alert,
    Octagon,
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
                // A ring and a filled dot (callouts' icons), centre and radius in the 12 box.
                let ring = |c: (f32, f32), r: f32| {
                    let mut e = stroke();
                    let radii = point(px(r * u), px(r * u));
                    e.move_to(p(c.0, c.1 - r));
                    e.arc_to(radii, px(0.0), false, true, p(c.0, c.1 + r));
                    e.arc_to(radii, px(0.0), false, true, p(c.0, c.1 - r));
                    e.close();
                    e
                };
                let dot = |c: (f32, f32)| {
                    let r = 0.9;
                    let mut d = PathBuilder::fill();
                    let radii = point(px(r * u), px(r * u));
                    d.move_to(p(c.0, c.1 - r));
                    d.arc_to(radii, px(0.0), false, true, p(c.0, c.1 + r));
                    d.arc_to(radii, px(0.0), false, true, p(c.0, c.1 - r));
                    d.close();
                    d
                };
                // A line from `a` to `b`.
                let bar = |a: (f32, f32), b: (f32, f32)| {
                    let mut l = stroke();
                    l.move_to(p(a.0, a.1));
                    l.line_to(p(b.0, b.1));
                    l
                };
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
                    Glyph::File => {
                        // The page's outline, its top right corner cut; the fold's two edges.
                        let (left, right, top, bottom, fold) = (2.5, 9.5, 1.0, 11.0, 3.0);
                        let mut page = stroke();
                        page.add_polygon(
                            &[
                                p(left, top),
                                p(right - fold, top),
                                p(right, top + fold),
                                p(right, bottom),
                                p(left, bottom),
                            ],
                            true,
                        );
                        paths.push(page);
                        let mut corner = stroke();
                        corner.move_to(p(right - fold, top));
                        corner.line_to(p(right - fold, top + fold));
                        corner.line_to(p(right, top + fold));
                        paths.push(corner);
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
                    Glyph::RightSidebar => {
                        // The split buttons' frame, and its right third filled.
                        let (left, right, top, bottom, side) = (1.0, 11.0, 1.5, 10.5, 7.5);
                        let mut frame = stroke();
                        frame.add_polygon(
                            &[
                                p(left, top),
                                p(right, top),
                                p(right, bottom),
                                p(left, bottom),
                            ],
                            true,
                        );
                        paths.push(frame);
                        let mut filled = PathBuilder::fill();
                        filled.add_polygon(
                            &[
                                p(side, top),
                                p(right, top),
                                p(right, bottom),
                                p(side, bottom),
                            ],
                            true,
                        );
                        paths.push(filled);
                    }
                    Glyph::Folder => {
                        let mut folder = stroke();
                        folder.add_polygon(
                            &[
                                p(1.0, 2.0),
                                p(4.5, 2.0),
                                p(5.75, 3.5),
                                p(11.0, 3.5),
                                p(11.0, 10.0),
                                p(1.0, 10.0),
                            ],
                            true,
                        );
                        paths.push(folder);
                    }
                    Glyph::FolderOpen => {
                        // The back: up the left side, over the tab, down to where the flap
                        // starts.
                        let mut back = stroke();
                        back.move_to(p(1.0, 10.0));
                        back.line_to(p(1.0, 2.0));
                        back.line_to(p(4.5, 2.0));
                        back.line_to(p(5.75, 3.5));
                        back.line_to(p(9.5, 3.5));
                        back.line_to(p(9.5, 5.5));
                        paths.push(back);
                        let mut flap = stroke();
                        flap.add_polygon(
                            &[p(1.0, 10.0), p(3.0, 5.5), p(11.5, 5.5), p(9.5, 10.0)],
                            true,
                        );
                        paths.push(flap);
                    }
                    Glyph::Branch => {
                        let ring = |c: (f32, f32)| {
                            let r = 1.6;
                            let mut e = stroke();
                            let radii = point(px(r * u), px(r * u));
                            e.move_to(p(c.0, c.1 - r));
                            e.arc_to(radii, px(0.0), false, true, p(c.0, c.1 + r));
                            e.arc_to(radii, px(0.0), false, true, p(c.0, c.1 - r));
                            e.close();
                            e
                        };
                        let (top, bottom, side) = ((3.5, 2.2), (3.5, 9.8), (8.5, 3.6));
                        paths.extend([ring(top), ring(bottom), ring(side)]);
                        let mut trunk = stroke();
                        trunk.move_to(p(top.0, top.1 + 1.6));
                        trunk.line_to(p(bottom.0, bottom.1 - 1.6));
                        paths.push(trunk);
                        // From under the side commit, bending down into the trunk.
                        let mut branch = stroke();
                        branch.move_to(p(side.0, side.1 + 1.6));
                        branch.curve_to(p(top.0, 8.0), p(side.0, 8.0));
                        paths.push(branch);
                    }
                    Glyph::ChevronRight => {
                        let mut chevron = stroke();
                        chevron.move_to(p(4.5, 2.5));
                        chevron.line_to(p(8.0, 6.0));
                        chevron.line_to(p(4.5, 9.5));
                        paths.push(chevron);
                    }
                    Glyph::ChevronDown => {
                        let mut chevron = stroke();
                        chevron.move_to(p(2.5, 4.5));
                        chevron.line_to(p(6.0, 8.0));
                        chevron.line_to(p(9.5, 4.5));
                        paths.push(chevron);
                    }
                    Glyph::CollapseAll => {
                        for y in [1.5, 6.0] {
                            let mut chevron = stroke();
                            chevron.move_to(p(2.5, y + 4.0));
                            chevron.line_to(p(6.0, y + 0.5));
                            chevron.line_to(p(9.5, y + 4.0));
                            paths.push(chevron);
                        }
                    }
                    Glyph::Pull | Glyph::Push => {
                        // The shaft and its head point down (Pull) or up (Push); the line is
                        // where it lands (below) or leaves from (above).
                        let down = matches!(kind, Glyph::Pull);
                        let (from, to, rule) = if down {
                            (1.5, 8.0, 10.5)
                        } else {
                            (10.5, 4.0, 1.5)
                        };
                        let mut shaft = stroke();
                        shaft.move_to(p(6.0, from));
                        shaft.line_to(p(6.0, to));
                        paths.push(shaft);
                        let back = if down { to - 3.0 } else { to + 3.0 };
                        let mut head = stroke();
                        head.move_to(p(3.0, back));
                        head.line_to(p(6.0, to));
                        head.line_to(p(9.0, back));
                        paths.push(head);
                        let mut line = stroke();
                        line.move_to(p(2.0, rule));
                        line.line_to(p(10.0, rule));
                        paths.push(line);
                    }
                    Glyph::ChangeUp | Glyph::ChangeDown => {
                        let up = matches!(kind, Glyph::ChangeUp);
                        let (from, to) = if up { (10.0, 2.0) } else { (2.0, 10.0) };
                        let back = if up { to + 3.0 } else { to - 3.0 };
                        let mut shaft = stroke();
                        shaft.move_to(p(6.0, from));
                        shaft.line_to(p(6.0, to));
                        paths.push(shaft);
                        let mut head = stroke();
                        head.move_to(p(3.0, back));
                        head.line_to(p(6.0, to));
                        head.line_to(p(9.0, back));
                        paths.push(head);
                    }
                    Glyph::Publish => {
                        // Part 6 (your pick, C): a branch (two commits on a trunk) and, beside
                        // it, an arrow going up: "send this branch up".
                        let r = 1.2;
                        for (cx_, cy) in [(3.4, 2.6), (3.4, 9.4)] {
                            let mut dot = stroke();
                            let radii = point(px(r * u), px(r * u));
                            dot.move_to(p(cx_, cy - r));
                            dot.arc_to(radii, px(0.0), false, true, p(cx_, cy + r));
                            dot.arc_to(radii, px(0.0), false, true, p(cx_, cy - r));
                            paths.push(dot);
                        }
                        let mut trunk = stroke();
                        trunk.move_to(p(3.4, 3.8));
                        trunk.line_to(p(3.4, 8.2));
                        paths.push(trunk);
                        let mut shaft = stroke();
                        shaft.move_to(p(8.6, 10.0));
                        shaft.line_to(p(8.6, 3.6));
                        paths.push(shaft);
                        let mut head = stroke();
                        head.move_to(p(6.7, 5.5));
                        head.line_to(p(8.6, 3.6));
                        head.line_to(p(10.5, 5.5));
                        paths.push(head);
                    }
                    Glyph::Revert => {
                        let mut head = stroke();
                        head.move_to(p(4.8, 1.8));
                        head.line_to(p(2.2, 4.4));
                        head.line_to(p(4.8, 7.0));
                        paths.push(head);
                        let mut shaft = stroke();
                        shaft.move_to(p(2.2, 4.4));
                        shaft.line_to(p(7.2, 4.4));
                        let r = point(px(3.0 * u), px(3.0 * u));
                        shaft.arc_to(r, px(0.0), false, true, p(7.2, 10.4));
                        shaft.line_to(p(4.5, 10.4));
                        paths.push(shaft);
                    }
                    Glyph::Wrap => {
                        for (y, right) in [(2.0, 10.5), (6.0, 7.5), (10.0, 4.0)] {
                            let mut line = stroke();
                            line.move_to(p(1.5, y));
                            line.line_to(p(right, y));
                            paths.push(line);
                        }
                        // The return: down from beside the middle line, back left along the
                        // last, its head pointing left.
                        let mut back = stroke();
                        back.move_to(p(10.5, 6.0));
                        back.line_to(p(10.5, 10.0));
                        back.line_to(p(6.5, 10.0));
                        paths.push(back);
                        let mut head = stroke();
                        head.move_to(p(8.3, 8.2));
                        head.line_to(p(6.5, 10.0));
                        head.line_to(p(8.3, 11.8));
                        paths.push(head);
                    }
                    Glyph::Copy => {
                        let mut front = stroke();
                        front.add_polygon(
                            &[p(1.5, 4.0), p(8.0, 4.0), p(8.0, 10.5), p(1.5, 10.5)],
                            true,
                        );
                        paths.push(front);
                        // The back sheet: its edges not behind the front one.
                        let mut back = stroke();
                        back.move_to(p(4.0, 4.0));
                        back.line_to(p(4.0, 1.5));
                        back.line_to(p(10.5, 1.5));
                        back.line_to(p(10.5, 8.0));
                        back.line_to(p(8.0, 8.0));
                        paths.push(back);
                    }
                    Glyph::Outline => {
                        for (y, left) in [(2.5, 1.5), (6.0, 4.0), (9.5, 1.5)] {
                            paths.push(bar((left, y), (10.5, y)));
                        }
                    }
                    Glyph::Check => {
                        let mut tick = stroke();
                        tick.move_to(p(1.5, 6.5));
                        tick.line_to(p(4.5, 9.5));
                        tick.line_to(p(10.5, 2.5));
                        paths.push(tick);
                    }
                    Glyph::Info => {
                        paths.push(ring((6.0, 6.0), 5.0));
                        paths.push(dot((6.0, 3.7)));
                        paths.push(bar((6.0, 5.6), (6.0, 8.9)));
                    }
                    Glyph::Bulb => {
                        paths.push(ring((6.0, 4.7), 3.5));
                        let mut neck = stroke();
                        neck.move_to(p(4.9, 7.6));
                        neck.line_to(p(4.9, 9.6));
                        neck.line_to(p(7.1, 9.6));
                        neck.line_to(p(7.1, 7.6));
                        paths.push(neck);
                        paths.push(bar((5.0, 11.2), (7.0, 11.2)));
                    }
                    Glyph::Report => {
                        let mut speech = stroke();
                        speech.add_polygon(
                            &[
                                p(1.5, 1.5),
                                p(10.5, 1.5),
                                p(10.5, 8.5),
                                p(7.0, 8.5),
                                p(4.5, 10.8),
                                p(4.5, 8.5),
                                p(1.5, 8.5),
                            ],
                            true,
                        );
                        paths.push(speech);
                        paths.push(bar((6.0, 3.3), (6.0, 5.4)));
                        paths.push(dot((6.0, 7.0)));
                    }
                    Glyph::Alert => {
                        let mut tri = stroke();
                        tri.add_polygon(&[p(6.0, 1.3), p(11.0, 10.3), p(1.0, 10.3)], true);
                        paths.push(tri);
                        paths.push(bar((6.0, 4.6), (6.0, 7.0)));
                        paths.push(dot((6.0, 8.7)));
                    }
                    Glyph::Octagon => {
                        let mut oct = stroke();
                        oct.add_polygon(
                            &[
                                p(4.0, 1.0),
                                p(8.0, 1.0),
                                p(11.0, 4.0),
                                p(11.0, 8.0),
                                p(8.0, 11.0),
                                p(4.0, 11.0),
                                p(1.0, 8.0),
                                p(1.0, 4.0),
                            ],
                            true,
                        );
                        paths.push(oct);
                        paths.push(bar((6.0, 3.4), (6.0, 6.5)));
                        paths.push(dot((6.0, 8.3)));
                    }
                    Glyph::SideBySide => {
                        let mut frame = stroke();
                        frame.move_to(p(1.5, 2.0));
                        frame.line_to(p(10.5, 2.0));
                        frame.line_to(p(10.5, 10.0));
                        frame.line_to(p(1.5, 10.0));
                        frame.line_to(p(1.5, 2.0));
                        paths.push(frame);
                        let mut middle = stroke();
                        middle.move_to(p(6.0, 2.0));
                        middle.line_to(p(6.0, 10.0));
                        paths.push(middle);
                    }
                    Glyph::OpenExternal => {
                        // The box, open at its top right corner.
                        let mut frame = stroke();
                        frame.move_to(p(5.5, 2.0));
                        frame.line_to(p(2.0, 2.0));
                        frame.line_to(p(2.0, 10.0));
                        frame.line_to(p(10.0, 10.0));
                        frame.line_to(p(10.0, 6.5));
                        paths.push(frame);
                        // The arrow, from inside the box out past that corner.
                        let mut shaft = stroke();
                        shaft.move_to(p(6.0, 6.0));
                        shaft.line_to(p(10.5, 1.5));
                        paths.push(shaft);
                        let mut head = stroke();
                        head.move_to(p(7.0, 1.5));
                        head.line_to(p(10.5, 1.5));
                        head.line_to(p(10.5, 5.0));
                        paths.push(head);
                    }
                    Glyph::More => {
                        // Filled dots, as wide as the lines are thick and a little more.
                        let r = 1.1;
                        for x in [2.0, 6.0, 10.0] {
                            let mut dot = PathBuilder::fill();
                            let radii = point(px(r * u), px(r * u));
                            dot.move_to(p(x, 6.0 - r));
                            dot.arc_to(radii, px(0.0), false, true, p(x, 6.0 + r));
                            dot.arc_to(radii, px(0.0), false, true, p(x, 6.0 - r));
                            dot.close();
                            paths.push(dot);
                        }
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
            title: None,
            status,
            age: Duration::from_secs(secs),
        })
    }

    #[test]
    fn a_terminal_keeps_its_icon_whatever_its_status_a_browser_a_globe() {
        // M11al: the status mark no longer takes the icon's place (it follows the title), so
        // the icon depends on the kind only (and a busy terminal's tab keeps `>_`, see
        // `bar_tabs_follow_the_pane_in_order_with_titles_icons_and_the_active_one`).
        assert_eq!(icon(&TabKind::Terminal(1)), TabIcon::Terminal);
        assert_eq!(icon(&TabKind::Browser { url: None }), TabIcon::Globe);
    }

    #[test]
    fn the_tooltip_adds_status_and_age_while_busy() {
        assert_eq!(
            tip("shell", None, Status::Idle, Duration::from_secs(900)),
            "shell"
        );
        assert_eq!(
            tip(
                "claude",
                None,
                Status::Waiting,
                Duration::from_secs(4 * 60 + 12)
            ),
            "claude — waiting 4m"
        );
        assert_eq!(
            tip("codex", None, Status::Working, Duration::from_secs(32)),
            "codex — working 32s"
        );
    }

    #[test]
    fn the_tooltip_adds_the_program_s_title_on_a_second_line_when_it_says_more() {
        let idle = Duration::ZERO;
        assert_eq!(
            tip("fuxx-cli", Some("me@mac:~/fuxx-cli"), Status::Idle, idle),
            "fuxx-cli\nme@mac:~/fuxx-cli"
        );
        assert_eq!(
            tip(
                "api",
                Some("✳ Fix the tests"),
                Status::Working,
                Duration::from_secs(32)
            ),
            "api — working 32s\n✳ Fix the tests"
        );
        // The same as the name, or blank: nothing more.
        assert_eq!(tip("vim", Some("vim"), Status::Idle, idle), "vim");
        assert_eq!(tip("vim", Some(" "), Status::Idle, idle), "vim");
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
                TabIcon::Terminal,
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
    fn a_terminal_tab_s_tooltip_carries_its_program_s_title() {
        let pane = Pane {
            id: crate::layout_model::PaneId(1),
            tabs: vec![crate::layout_model::Tab {
                id: TabId(10),
                kind: TabKind::Terminal(1),
            }],
            active: TabId(10),
        };
        let facts = |_| {
            Some(TerminalFacts {
                label: "fuxx-cli".into(),
                title: Some("me@mac:~/fuxx-cli".into()),
                status: Status::Idle,
                age: Duration::ZERO,
            })
        };
        let tabs = bar_tabs(&pane, facts, |_| None, |_| None);
        assert_eq!(tabs[0].title.as_ref(), "fuxx-cli");
        assert_eq!(tabs[0].tip, "fuxx-cli\nme@mac:~/fuxx-cli");
    }

    #[test]
    fn tile_glyphs_take_their_kind_s_colour() {
        // M11al §4.3: a terminal's and a browser's glyph in the accent, a file's in `text_dim`,
        // an agent's in its provider's colour, held to the marks' floor on the tile.
        for (name, theme) in crate::theme::every_theme() {
            let ui = theme.ui();
            assert_eq!(tile_ink(&ui, TabIcon::Terminal), ui.accent, "{name}");
            assert_eq!(tile_ink(&ui, TabIcon::Globe), ui.accent, "{name}");
            assert_eq!(tile_ink(&ui, TabIcon::File), ui.text_dim, "{name}");
            let agent = TabIcon::Agent {
                badge: "CC",
                rgb: 0x7a4a2a,
            };
            let ink = tile_ink(&ui, agent);
            let floor = crate::ui_tokens::MARK_MIN;
            assert!(
                crate::ui_tokens::contrast(ink, ui.card_lift) >= floor,
                "{name}: {ink:?} on the tile"
            );
            let brown = Rgb {
                r: 0x7a,
                g: 0x4a,
                b: 0x2a,
            };
            assert_eq!(
                ink,
                crate::theme::readable(brown, &[ui.card_lift], floor),
                "{name}"
            );
        }
    }

    #[test]
    fn look_active_focused_is_tab_active() {
        let ui = ui();
        // M11al: the focused pane's shown tab has the accent wash, full text, no hover change.
        for status in [Status::Idle, Status::Working, Status::Waiting, Status::Done] {
            assert_eq!(
                look(&ui, status, true, true),
                Look {
                    fill: Some(ui.tab_active),
                    hover: None,
                    text: ui.text,
                },
                "{status:?}"
            );
        }
    }

    #[test]
    fn look_active_unfocused_is_card_hover() {
        let ui = ui();
        // An unfocused split pane's shown tab: a neutral wash (no per-pane ring, M11al §3.3).
        for status in [Status::Idle, Status::Waiting] {
            assert_eq!(
                look(&ui, status, true, false),
                Look {
                    fill: Some(ui.card_hover),
                    hover: None,
                    text: ui.text,
                },
                "{status:?}"
            );
        }
    }

    #[test]
    fn look_inactive_has_no_fill_and_faint_text() {
        let ui = ui();
        // No fill (the card shows through), faint text, the card's hover shade; in the focused
        // pane or not.
        for focused in [true, false] {
            for status in [Status::Idle, Status::Working, Status::Done] {
                assert_eq!(
                    look(&ui, status, false, focused),
                    Look {
                        fill: None,
                        hover: Some(ui.card_hover),
                        text: ui.text_faint,
                    },
                    "{status:?}, focused {focused}"
                );
            }
        }
    }

    #[test]
    fn look_waiting_inactive_keeps_its_tint() {
        let ui = ui();
        for focused in [true, false] {
            assert_eq!(
                look(&ui, Status::Waiting, false, focused),
                Look {
                    fill: Some(ui.waiting_tint),
                    hover: Some(ui.waiting_hover),
                    text: ui.text_faint,
                },
                "focused {focused}"
            );
        }
    }

    #[test]
    fn the_buttons_are_plus_more_and_the_right_sidebar() {
        assert_eq!(
            BarButton::ALL,
            [
                BarButton::NewTerminal,
                BarButton::More,
                BarButton::RightSidebar
            ]
        );
        assert_eq!(
            BarButton::ALL.map(BarButton::tooltip),
            [
                "New terminal tab (⌘T)",
                "More",
                "Toggle right sidebar (⌥⌘\\)"
            ]
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
    fn a_file_tab_shows_its_file_name_and_glyph_and_its_path_in_the_tooltip() {
        let file = TabKind::File {
            path: "/w/api/src/main.rs".into(),
            mode: crate::layout_model::FileMode::Plain,
        };
        assert_eq!(icon(&file), TabIcon::File);
        let pane = Pane {
            id: crate::layout_model::PaneId(1),
            tabs: vec![
                crate::layout_model::Tab {
                    id: TabId(10),
                    kind: TabKind::Terminal(1),
                },
                crate::layout_model::Tab {
                    id: TabId(11),
                    kind: file,
                },
            ],
            active: TabId(11),
        };
        let tabs = bar_tabs(
            &pane,
            |_| terminal("shell", Status::Idle, 0),
            |_| None,
            |_| None,
        );
        assert_eq!(tabs[1].title.as_ref(), "main.rs");
        assert_eq!(tabs[1].icon, TabIcon::File);
        assert_eq!(tabs[1].tip, "/w/api/src/main.rs");
        assert_eq!(tabs[1].terminal, None);
        assert_eq!(tabs[1].status, Status::Idle);
        assert!(tabs[1].active);
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
            |_| None,
        );
        let titles: Vec<&str> = tabs.iter().map(|t| t.title.as_ref()).collect();
        assert_eq!(titles, [BROWSER_TITLE, "Docs.rs"]);
        assert_eq!(tabs[1].tip, "Docs.rs");
    }

    #[test]
    fn an_agent_tab_shows_its_badge_title_and_tip() {
        let pane = Pane {
            id: crate::layout_model::PaneId(1),
            tabs: vec![
                crate::layout_model::Tab {
                    id: TabId(20),
                    kind: TabKind::Agent {
                        provider: "claude".into(),
                        cwd: "/w".into(),
                        conversation: None,
                        mode: None,
                        title: Some("fix tests".into()),
                        model: None,
                        effort: None,
                    },
                },
                crate::layout_model::Tab {
                    id: TabId(21),
                    kind: TabKind::Agent {
                        provider: "claude".into(),
                        cwd: "/w".into(),
                        conversation: None,
                        mode: None,
                        title: None,
                        model: None,
                        effort: None,
                    },
                },
            ],
            active: TabId(20),
        };
        let facts = |status| AgentFacts {
            label: "Claude".into(),
            badge: "CC",
            rgb: 0x7a4a2a,
            status,
            age: Duration::from_secs(4 * 60),
        };
        let tabs = bar_tabs(
            &pane,
            |_| None,
            |_| None,
            |tab| {
                Some(facts(if tab == TabId(20) {
                    Status::Waiting
                } else {
                    Status::Idle
                }))
            },
        );
        assert_eq!(
            tabs[0].icon,
            TabIcon::Agent {
                badge: "CC",
                rgb: 0x7a4a2a
            }
        );
        assert_eq!(tabs[0].title.as_ref(), "fix tests");
        assert_eq!(tabs[0].tip, "fix tests — waiting 4m");
        assert_eq!(tabs[0].status, Status::Waiting);
        assert_eq!(tabs[0].terminal, None);
        // Untitled: its provider's label.
        assert_eq!(tabs[1].title.as_ref(), "Claude");
        assert_eq!(tabs[1].tip, "Claude");
    }
}
