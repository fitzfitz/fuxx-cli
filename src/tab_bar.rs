//! Pane tab bars (M11b): a bar on top of every pane (even a lone one, as cmux does), in place of
//! M11's pane headers. Since M11al (style E, "minimal") the bar is `TAB_BAR` (52 pt) with no
//! fill of its own (the card shows through) and a 1 pt `card_border` line along its bottom. Its
//! tabs float in its middle as pills (chore 2026-11-20), `TAB` tall with round ends, each as
//! wide as its title: each shows an icon tile (a terminal's `>_`, a browser's globe, a file's
//! page, an agent's provider letters), its title, and after it a small status dot while its
//! terminal or agent is Working, Waiting or Done; × closes it (shown on hover). The shown tab
//! has the accent wash (`tab_active`; in an unfocused split pane a neutral one, `card_lift`);
//! the others are quiet pills (`card_hover`), a waiting one tinted. Too many tabs shrink to a
//! minimum width, then the row scrolls sideways. + (a new terminal tab) follows the last tab;
//! on the right, pinned: ⋯ (the pane's actions, `pane_actions`) and the right sidebar's (M11g;
//! lit while it's open).
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
    Focusable, FontWeight, MouseButton, MouseDownEvent, Pixels, Point, ScrollHandle, SharedString,
    Window, canvas, div, prelude::*, px, relative,
};

use crate::appearance::Appearance;
use crate::block_view::BlockDrag;
use crate::chrome_cache::BarPlace;
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
use crate::ui_tokens::size::{CARD_INSET, HAIRLINE, TAB_BAR, TABS_LEFT};
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
        /// Its provider's logo (spec B-A): drawn in its own colours in place of the letters.
        logo: Option<crate::logos::Logo>,
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
    pub logo: Option<crate::logos::Logo>,
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
            let logo = crate::providers::by_name(provider).and_then(|p| p.info().logo);
            TabIcon::Agent { badge, rgb, logo }
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
                    logo: f.logo,
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
/// (M11al §3.3, §4.2; pills since 2026-11-20). The shown tab has full text and a wash: the
/// accent's (`tab_active`) in the focused pane, a neutral one (`card_lift`) in another (no
/// per-pane ring shows the focus now). The others are quiet pills (`card_hover`, `card_lift`
/// under the pointer) with faint text; a waiting one keeps its tint.
pub fn look(ui: &UiColors, status: Status, active: bool, pane_focused: bool) -> Look {
    let waiting = status == Status::Waiting;
    let (fill, hover) = match (active, waiting) {
        (true, _) if pane_focused => (Some(ui.tab_active), None),
        (true, _) => (Some(ui.card_lift), None),
        (false, false) => (Some(ui.tab_idle), Some(ui.tab_idle_hover)),
        (false, true) => (Some(ui.waiting_tint), Some(ui.waiting_hover)),
    };
    Look {
        fill,
        hover,
        // Inactive: quieter text (your words, 2026-11-22); a waiting one keeps faint text, it
        // asks for you.
        text: match (active, waiting) {
            (true, _) => ui.text,
            (false, false) => ui.tab_idle_text,
            (false, true) => ui.text_faint,
        },
    }
}

/// A tab's width for a title `title` wide (with a status dot after it or not): its tile, title
/// and ×, `PART_GAP` apart, inside `TAB_PAD`; held between `TAB_MIN_WIDTH` and `TAB_MAX_WIDTH`.
pub fn natural_width(title: f32, dot: bool) -> f32 {
    // The room a status mark kept before it moved into the ×'s slot; kept as it was so the
    // tabs' widths don't move with the mark's size (it is drawn in the `CLOSE` slot now).
    const DOT_ROOM: f32 = 8.0;
    let dot = if dot { PART_GAP + DOT_ROOM } else { 0.0 };
    (2.0 * TAB_PAD + TILE + 2.0 * PART_GAP + CLOSE + title + dot)
        .clamp(TAB_MIN_WIDTH, TAB_MAX_WIDTH)
}

/// A tab's icon's opacity (spec B-C): quieter on an inactive tab on a dark theme (0.75), whole
/// when shown or under the pointer (`lit`), and always on a light theme (the logos are only
/// ~2.5:1 there already).
pub fn icon_opacity(lit: bool, dark: bool) -> f32 {
    if lit || !dark { 1.0 } else { 0.75 }
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

/// A tab's icon (M11al §4.3; since 2026-11-20 an outline icon of the set, no tile behind it:
/// "find icon with outline no backgrounds"): `TILE` square, in `tile_ink`; an agent's provider
/// letters (`TILE_GLYPH`) in its provider's colour.
fn tile(icon: TabIcon, ui: &UiColors, cx: &App) -> Div {
    let ink = tile_ink(ui, icon);
    let line = |i| crate::icons::icon(i, px(TILE), hsla(ink)).into_any_element();
    let inside = match icon {
        TabIcon::Terminal => line(crate::icons::Icon::Terminal),
        TabIcon::Globe => line(crate::icons::Icon::Globe),
        TabIcon::File => line(crate::icons::Icon::File),
        TabIcon::Agent {
            logo: Some(logo), ..
        } if let Some(image) = crate::logos::image(logo, TILE, cx) => {
            gpui::img(image).size(px(TILE)).into_any_element()
        }
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
    #[cfg(test)]
    pub const ALL: [BarButton; 3] = [
        BarButton::NewTerminal,
        BarButton::More,
        BarButton::RightSidebar,
    ];
    /// The ones pinned at the bar's right end; + follows the last tab (chore 2026-11-20).
    pub const PINNED: [BarButton; 2] = [BarButton::More, BarButton::RightSidebar];

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
            .child(tile(self.icon, &ui, cx))
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
///
/// Since the pill chore (2026-11-20, your reference: "a pill, not that big"): `TAB` 28 tall with
/// fully round ends, as wide as its title (`natural_width`), no separator line; `AFTER` is the
/// clear gap after each pill.
pub(crate) const TAB: f32 = 28.0;
pub(crate) const TAB_RADIUS: f32 = TAB / 2.0;
pub(crate) const TAB_PAD: f32 = 8.0;
pub(crate) const PART_GAP: f32 = 8.0;
pub(crate) const AFTER: f32 = 4.0;
/// The gap between the last tab's pill and the + after it.
pub(crate) const PLUS_GAP: f32 = 2.0;
pub(crate) const TILE: f32 = 14.0;
const TILE_GLYPH: f32 = 11.0;
/// A tab's status mark (your words, 2026-11-22: "the loading and the checklist is too tiny"):
/// the size of the box's own marks, in the × slot (`CLOSE`).
pub(crate) const STATUS_DOT: f32 = 12.0;
/// An inactive tab's status mark's opacity.
pub(crate) const INACTIVE_MARK: f32 = 0.2;
pub(crate) const CLOSE: f32 = 16.0;
/// A bar button, its glyph (drawn with the title area's line width) and the gap between
/// buttons and around them.
pub(crate) const BUTTON: f32 = 24.0;
pub(crate) const BUTTON_ICON: f32 = 14.0;
pub(crate) const BUTTON_GAP: f32 = 4.0;
pub(crate) const BUTTONS_PAD: f32 = 10.0;
/// The bars' line icons' box.
pub(crate) const ICON: f32 = 12.0;

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
                    logo: slot.logo,
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
    /// right, its buttons. None for an unknown pane. `place` (`BarPlace`): at the window's top
    /// edge its empty part (after the last tab) drags the window; the top-right bar alone has
    /// the right sidebar's button; the top-left one, with the sidebar hidden, starts with the
    /// room the traffic lights and the sidebar's buttons take (`title_area::band_reserve`),
    /// which drags the window too.
    pub(crate) fn render_tab_bar(
        &self,
        pane: PaneId,
        ui: UiColors,
        place: BarPlace,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        crate::chrome_cache::bump(crate::chrome_cache::Counted::TabBar);
        let p = self.model.pane(pane)?;
        let focused = self.model.focused_pane() == Some(pane);
        let now = Instant::now();
        let tabs = self.pane_bar_tabs(p, now);
        // Each pill as wide as its title (chore 2026-11-20): the title measured in the bar's
        // font at `text::UI` (a preview's in italics), a point spare so it never truncates.
        let font = window.text_style().font();
        let naturals: Vec<f32> = tabs
            .iter()
            .map(|t| {
                let mut font = font.clone();
                if t.preview {
                    font.style = gpui::FontStyle::Italic;
                }
                let run = gpui::TextRun {
                    len: t.title.len(),
                    font: font.clone(),
                    color: hsla(ui.text),
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                };
                let line =
                    window
                        .text_system()
                        .shape_line(t.title.clone(), px(text::UI), &[run], None);
                natural_width(f32::from(line.width()) + 1.0, t.status != Status::Idle)
            })
            .collect();
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
        // The tabs: as wide as they need (the sum of their pills), giving way to the bar's
        // width; past that they shrink, then the row scrolls sideways.
        let row = div()
            .id(("pane-tabs", pane.0 as usize))
            .flex()
            .flex_row()
            .items_center()
            .flex_initial()
            .min_w(px(0.0))
            .h_full()
            // The tabs float in the bar's middle (M11al); the first starts `TABS_LEFT` in from
            // the card's edge, as the right sidebar's panel tabs (under the lights, the room
            // before the row holds that gap).
            .when(!place.under_lights, |row| row.pl(px(TABS_LEFT)))
            .overflow_x_scroll()
            .track_scroll(&scroll)
            .children(tabs.into_iter().zip(targets).zip(naturals).enumerate().map(
                |(i, ((tab, target), natural))| {
                    // As last laid out, for its drag chip.
                    let width = scroll
                        .bounds_for_item(i)
                        .map_or(natural + AFTER, |b| f32::from(b.size.width));
                    let offset = self.drag_slides.borrow_mut().offset(
                        Slid::Tab(tab.tab),
                        target,
                        now,
                        animate,
                    );
                    self.render_tab(tab, ui, focused, offset, width, natural, cx)
                },
            ));
        let button = |b: BarButton, cx: &mut Context<Self>| {
            // The right sidebar's is lit while the sidebar is open; ⋯ while its menu is.
            let lit = match b {
                BarButton::NewTerminal => false,
                BarButton::More => self.pane_actions.as_ref().is_some_and(|m| m.pane == pane),
                BarButton::RightSidebar => !self.right_sidebar.hidden,
            };
            render_button(pane, b, lit, ui, cx)
        };
        // + right after the last tab (chore 2026-11-20: "the + is instead beside the last tab"),
        // staying in sight when the row scrolls.
        let plus = div()
            .flex()
            .flex_none()
            .items_center()
            .h_full()
            .pl(px(PLUS_GAP))
            .child(button(BarButton::NewTerminal, cx));
        // What the tabs and + leave empty; it takes no space once they fill the bar.
        let filler = div()
            .id(("pane-tabs-empty", pane.0 as usize))
            .flex_1()
            .min_w(px(0.0))
            .h_full();
        let filler = if place.drags_window {
            self.drag_window_by(filler, cx)
        } else {
            filler
        };
        // Under the lights: up to `band_reserve` from the window's left (the card starts
        // `CARD_INSET` in, inside its 1 pt edge; the row then has no `TABS_LEFT` of its own, so
        // the whole gap to the first tab drags the window).
        let reserve = place.under_lights.then(|| {
            let width =
                crate::title_area::band_reserve(window.is_fullscreen()) - CARD_INSET - HAIRLINE;
            let room = div()
                .id(("pane-tabs-lights", pane.0 as usize))
                .flex_none()
                .w(px(width))
                .h_full();
            if place.drags_window {
                self.drag_window_by(room, cx)
            } else {
                room
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
            .children(
                BarButton::PINNED
                    .into_iter()
                    .filter(|&b| b != BarButton::RightSidebar || place.top_right)
                    .map(|b| button(b, cx)),
            );
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
                .children(reserve)
                .child(row)
                .child(plus)
                .child(filler)
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
    /// hover; a `TAB` tall pill (chore 2026-11-20) floating in the bar's middle, `natural` wide
    /// (`natural_width`) unless the row is short of room; `AFTER` clear after it. It can be
    /// dragged (a press and a move of a few pixels; a click stays a click): its chip is it
    /// meanwhile, and its place shows empty. Its place is `width` wide as last laid out (the pill
    /// and its gap). It is drawn `offset` px right of its place (out of a dragged tab's way; its
    /// place in the row, which the drop rules measure, stays where it is).
    #[allow(clippy::too_many_arguments)]
    fn render_tab(
        &self,
        tab: BarTab,
        ui: UiColors,
        pane_focused: bool,
        offset: f32,
        width: f32,
        natural: f32,
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
                // Done: the check on a green disc (your words, 2026-11-22: "make the checklist
                // have backgrounds (green background)"), cut out in the tab's own fill.
                Status::Done => div()
                    .flex_none()
                    .size(px(STATUS_DOT))
                    .rounded_full()
                    .bg(hsla(ui.ring_done))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(status_mark::done_check(surface, px(STATUS_DOT - 2.0)))
                    .into_any_element(),
                _ => self.mark_view(place, key, status, px(STATUS_DOT), surface, cx),
            };
            Some(mark)
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
            .absolute()
            .top_0()
            .left_0()
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
        let dark = cx.global::<crate::appearance::Appearance>().theme.is_dark();
        let tile = tile(tab.icon, &ui, cx)
            .opacity(icon_opacity(shown, dark))
            .group_hover(group.clone(), |s| s.opacity(1.0));
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
            .group(group.clone())
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
            .child(part_gap())
            // The mark and × share one slot (your words, 2026-11-22: the mark "looks like
            // floating" beside the hidden ×): the mark shows, the × takes its place on hover.
            .child(
                div()
                    .relative()
                    .flex_none()
                    .size(px(CLOSE))
                    .children(dot.map(|dot| {
                        div()
                            .size_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            // An inactive tab's mark is almost clear (your words, 2026-11-22:
                            // "should be almost transparent (0.20)"); a waiting tab still has
                            // its tint.
                            .when(!tab.active, |d| d.opacity(INACTIVE_MARK))
                            .group_hover(group.clone(), |s| s.invisible())
                            .child(dot)
                    }))
                    .child(close),
            )
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
        // The clear gap after it (no separator line between pills).
        let gap = div().flex_none().w(px(AFTER)).h_full();
        // Its place in the row: the size the row gives it (the pill and its gap), `TAB` tall in
        // the bar's middle; the tab and its gap are drawn from it, slid by `offset`. Its chip is
        // it while it's dragged: its place shows empty. `natural` wide while the row has room;
        // short of room, at least `TAB_MIN_WIDTH` (then the row scrolls); but the shown tab, in a
        // row narrower than that, fits the row (its title gives way): its minimum is its
        // content's (a strut that wide), held to the row's width by its `max_w` (taffy's
        // automatic minimum size of a flex item, css-flexbox-1 §4.5, clamps the content size by
        // the max size).
        // (Its width, not a flex basis: the row is as wide as its content, and taffy sizes that
        // content from the places' widths; a basis alone left every place at its minimum.)
        let place = div()
            .relative()
            .w(px(natural + AFTER))
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
                    .child(gap),
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
        BarButton::NewTerminal => crate::icons::Icon::Plus,
        BarButton::More => crate::icons::Icon::Ellipsis,
        BarButton::RightSidebar => crate::icons::Icon::PanelRight,
    };
    let tip = button.tooltip();
    // Where it was last painted: the ⋯ menu opens under it.
    let placed: Rc<Cell<Option<Bounds<Pixels>>>> = Rc::default();
    let place = placed.clone();
    // Its icon of the set (chore 2026-11-20), in the hover colour while the pointer is on it.
    let icon = crate::icons::icon(kind, px(BUTTON_ICON), hsla(dim))
        .group_hover(group.clone(), move |s| s.text_color(hsla(lit)));
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

/// The bars' line icons (the tab bars', the browser and file tabs' bars, the sidebar's and the
/// panels'): the app's icon set, `icons::Icon` (Lucide outlines since 2026-11-20, your call:
/// "find icon with outline no backgrounds… we need to have icon sets"; drawn here before).
pub(crate) type Glyph = crate::icons::Icon;

/// `kind` in `color`, in a 12 px box.
pub(crate) fn glyph(kind: Glyph, color: Rgb) -> Div {
    footer_glyph(kind, color, ICON)
}

/// `kind` in `color`, in a `size` px box.
pub(crate) fn footer_glyph(kind: Glyph, color: Rgb, size: f32) -> Div {
    div()
        .flex_none()
        .size(px(size))
        .child(crate::icons::icon(kind, px(size), hsla(color)))
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
                logo: None,
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
    fn look_active_unfocused_is_card_lift() {
        let ui = ui();
        // An unfocused split pane's shown tab: a neutral wash a step above the other pills
        // (no per-pane ring, M11al §3.3; pills, chore 2026-11-20).
        for status in [Status::Idle, Status::Waiting] {
            assert_eq!(
                look(&ui, status, true, false),
                Look {
                    fill: Some(ui.card_lift),
                    hover: None,
                    text: ui.text,
                },
                "{status:?}"
            );
        }
    }

    #[test]
    fn look_inactive_is_a_quiet_pill_with_faint_text() {
        let ui = ui();
        // Every tab is a pill (chore 2026-11-20); the others quieter than the shown one (spec
        // B-C, "inactive tabs … slightly more darker"): `tab_idle`, `tab_idle_hover` under the
        // pointer, and faint text; in the focused pane or not.
        for focused in [true, false] {
            for status in [Status::Idle, Status::Working, Status::Done] {
                assert_eq!(
                    look(&ui, status, false, focused),
                    Look {
                        fill: Some(ui.tab_idle),
                        hover: Some(ui.tab_idle_hover),
                        text: ui.tab_idle_text,
                    },
                    "{status:?}, focused {focused}"
                );
            }
        }
    }

    /// Spec B-C: an inactive tab sits between the card and the shown tab, so the shown one
    /// stands out; its icon is quieter on dark themes until it is hovered.
    #[test]
    fn inactive_tabs_are_quieter_than_the_shown_one() {
        use crate::ui_tokens::{TEXT_MIN, contrast, relative_luminance};
        for (name, theme) in crate::theme::every_theme() {
            let ui = theme.ui();
            let l = relative_luminance;
            let (card, idle, hover, active) = (
                l(ui.card),
                l(ui.tab_idle),
                l(ui.tab_idle_hover),
                l(ui.tab_active),
            );
            let between = |x: f32| {
                (x - card) * (active - card) >= 0.0 && (x - card).abs() < (active - card).abs()
            };
            assert!(
                between(idle) && between(hover),
                "{name}: idle {idle} hover {hover}"
            );
            // Its title quieter than faint text (your words, 2026-11-22: "inactive tab make the
            // text more transparent"), still a mark's floor on its fill.
            for on in [ui.tab_idle, ui.tab_idle_hover] {
                let got = contrast(ui.tab_idle_text, on);
                assert!(
                    got >= crate::ui_tokens::MARK_MIN - 0.01,
                    "{name}: title {got:.2}"
                );
                assert!(
                    got < contrast(ui.text_faint, on),
                    "{name}: quieter than faint ({got:.2})"
                );
            }
            let dark = theme.is_dark();
            assert_eq!(
                icon_opacity(false, dark),
                if dark { 0.75 } else { 1.0 },
                "{name}"
            );
            assert_eq!(icon_opacity(true, dark), 1.0, "{name}");
        }
    }

    #[test]
    fn a_tab_is_a_small_pill() {
        // Your reference (2026-11-20): a pill, not that big.
        assert_eq!(TAB, 28.0);
        assert_eq!(TAB_RADIUS, TAB / 2.0);
    }

    #[test]
    fn a_tab_is_as_wide_as_its_title_between_the_bounds() {
        // The tile, the title, the ×, each a gap apart, inside the padding.
        let parts = 2.0 * TAB_PAD + TILE + 2.0 * PART_GAP + CLOSE;
        assert_eq!(natural_width(60.0, false), parts + 60.0);
        // A status dot takes its own gap.
        assert_eq!(natural_width(60.0, true), parts + 60.0 + PART_GAP + 8.0);
        // Never narrower than the least, nor wider than the most.
        assert_eq!(natural_width(1.0, false), TAB_MIN_WIDTH);
        assert_eq!(natural_width(900.0, true), TAB_MAX_WIDTH);
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
        // + follows the last tab; ⋯ and the right sidebar's stay pinned on the right.
        assert_eq!(
            BarButton::PINNED,
            [BarButton::More, BarButton::RightSidebar]
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
            logo: Some(crate::logos::Logo::Claude),
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
                rgb: 0x7a4a2a,
                logo: Some(crate::logos::Logo::Claude),
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
