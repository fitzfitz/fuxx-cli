//! The title area (M11b): the selected workspace's name at the window's top, and the 🔔 of
//! this run's notifications (`notification_list`; a dot while some are unseen), which lives in
//! the sidebar's footer beside the Preferences gear, and in the window's band with the sidebar
//! hidden (M11m).
//!
//! With the sidebar shown the pane tab bars are the window's top (they drag it) and the selected
//! row, filled with the accent, already names the workspace, so no band is added over the
//! content to say it again and the terminals keep that height. The sidebar's band holds the
//! app's title, "the Fuxx?", in its middle (M11m), where it fits (`shows_title`), and the
//! sidebar button at its right edge (`sidebar_toggle`).
//!
//! With the sidebar hidden (M11m) the window's band is the title bar: the name centred on the
//! window (drawn at its root over the panes' band and the right sidebar's), and [🔔] [⚙]
//! [sidebar] at its far right (`band_buttons`), the 🔔 and ⚙ doing what the footer's do.
//!
//! The popover is one of the overlays (one open at a time): it opens upward from the footer's
//! 🔔, over the sidebar with its right edge the gear's, or, with the sidebar hidden, drops down
//! from the band's 🔔 with its right edge the band's; every sidebar toggle closes it (its 🔔
//! goes), giving the keyboard back to the terminal. The 🔔 stays lit on its fill while it shows
//! (as the + with its menu). It holds the keyboard so Escape closes it, and a click anywhere
//! else, or a shortcut that moves the keyboard (`close_menus`), closes it too. Clicking an entry
//! whose terminal is still open shows that terminal (its workspace, pane and tab) as ⇧⌘U does;
//! a closed one's entry is greyed and does nothing. The dot follows the list, which changes
//! only when a notification is raised or the popover opens: no timer.
//!
//! `footer_buttons`, `band_buttons`, `popover_anchor`, `list_room`, `entry_top`, the band's
//! `band_popover_anchor`, `band_popover_width`, `band_list_room` and `band_entry_top`,
//! `name_inset` and `shows_title` are the pure geometry.

use std::time::Instant;

use gpui::{
    Anchor, AnyElement, AnyView, ClickEvent, Context, Div, FontWeight, MouseButton, MouseDownEvent,
    Window, anchored, div, point, prelude::*, px,
};

use crate::notification_list::Entry;
use crate::overlay_panel;
use crate::preferences;
use crate::sidebar::{FOOTER, MARK_SLOT, ROW_INSET, mark_size, mark_slot, tabular};
use crate::sidebar_toggle::{self, BAND_EDGE, HIT, Tip, band_button};
use crate::status::{self, Status};
use crate::status_mark;
use crate::tab_bar::{Glyph, footer_glyph};
use crate::tab_menu::Dismiss;
use crate::terminal_view::hsla;
use crate::theme::UiColors;
use crate::ui_tokens::size::{STATUS_BAR, TOP_BAND};
use crate::ui_tokens::text;
use crate::workspace::{Overlay, Workspace};

/// The footer's buttons (the +, the 🔔, the gear): each one's side, and the space between them.
pub(crate) const FOOTER_BUTTON: f32 = 28.0;
pub(crate) const FOOTER_GAP: f32 = 4.0;
/// The space between the workspace's name and what is beside it in the collapsed band (the
/// lights, the band's buttons), and the least room the name is drawn in (less would show little
/// more than an ellipsis).
const NAME_GAP: f32 = 12.0;
const NAME_MIN: f32 = 60.0;
/// The app's title, drawn in the open sidebar's band (M11m), the least room it is drawn in
/// (centred on the sidebar's middle), and the least space between it and the sidebar button.
pub(crate) const TITLE: &str = "the Fuxx?";
pub(crate) const TITLE_ROOM: f32 = 64.0;
/// `TITLE_GAP` never binds at today's widths (the lights' clearance always does first): it is
/// the safeguard should `TITLE_ROOM` grow past 100.
pub(crate) const TITLE_GAP: f32 = 8.0;
/// The Preferences gear's tooltip (the footer's, and the collapsed band's).
pub(crate) const PREFERENCES_TIP: &str = "Preferences (⌘,)";
/// The 🔔's tooltip (the footer's, and the collapsed band's).
pub(crate) const NOTIFICATIONS_TIP: &str = "Notifications";
/// The collapsed band's buttons (M11m), [🔔] [⚙] [sidebar] at the window's right: the space
/// between them, and the group's width from the 🔔's left edge to the window's right edge.
pub(crate) const BAND_GAP: f32 = 4.0;
pub(crate) const BAND_GROUP: f32 = BAND_EDGE + 3.0 * HIT + 2.0 * BAND_GAP;
/// The band 🔔's and ⚙'s hover groups.
pub(crate) const BAND_BELL_GROUP: &str = "band-notifications";
pub(crate) const BAND_PREFERENCES_GROUP: &str = "band-preferences";
/// The unseen dot, at the 🔔's top right, and the ring of the footer's colour that keeps it apart
/// from the 🔔 (inside `DOT`).
pub(crate) const DOT: f32 = 10.0;
pub(crate) const DOT_INSET: f32 = 1.5;
/// The popover: its widest and narrowest (the room in the default sidebar), how far above the
/// footer it opens, how far from the window's edges it keeps, and the most its list may take
/// (the rest scrolls).
const POPOVER_WIDTH: f32 = 340.0;
const POPOVER_MIN: f32 = 228.0;
const POPOVER_DROP: f32 = 4.0;
pub(crate) const EDGE_MARGIN: f32 = 8.0;
const LIST_MAX: f32 = 420.0;
/// The popover's inside: its padding (inside a 1 px edge), the space between its header and
/// list, and the header's height.
const PADDING: f32 = 4.0;
const HEADER: f32 = 18.0;
/// An entry: its first line (the mark is centred on it), the line under it, the space above
/// and below them, and the space between entries.
const LINE: f32 = 18.0;
const SUB: f32 = 15.0;
const ENTRY_PAD: f32 = 5.0;
const ENTRY_GAP: f32 = 2.0;
const ENTRY: f32 = ENTRY_PAD + LINE + SUB + ENTRY_PAD;
/// A script's notification's mark (it has no status).
const SCRIPT_DOT: f32 = 6.0;
/// The footer's 🔔 button's hover group, and the footer icons' box and line width (the same for
/// the +, the 🔔 and the gear).
pub(crate) const BELL_GROUP: &str = "notifications-button";
pub(crate) const PREFERENCES_GROUP: &str = "sidebar-preferences";
const ICON: f32 = 16.0;
const STROKE: f32 = 1.5;

/// The popover's height around its list: its edge, padding, header and the gap under it.
const PANEL_CHROME: f32 = 2.0 + 2.0 * PADDING + HEADER + PADDING;

/// The left edges, in the sidebar `width` wide, of the footer's buttons: the + at the footer's
/// left inset, the gear at the right (the same distance from the sidebar's edge, which has no
/// border since M11i), the 🔔 beside it.
pub(crate) fn footer_buttons(width: f32) -> [f32; 3] {
    let gear = width - ROW_INSET - FOOTER_BUTTON;
    [ROW_INSET, gear - FOOTER_GAP - FOOTER_BUTTON, gear]
}

/// The popover's width over a sidebar `sidebar_width` wide: the footer's, inside its insets,
/// from `POPOVER_MIN` to `POPOVER_WIDTH`.
pub(crate) fn popover_width(sidebar_width: f32) -> f32 {
    (sidebar_width - 2.0 * ROW_INSET).clamp(POPOVER_MIN, POPOVER_WIDTH)
}

/// The popover's bottom left corner in the window, just above the footer. It hangs from the
/// footer's right-hand buttons (the 🔔 and the gear): its right edge is the gear's, so it sits
/// over the sidebar, unless that is too narrow for it; then it starts at the footer's left
/// inset, as the + menu does.
pub(crate) fn popover_anchor(sidebar_width: f32, viewport_height: f32) -> (f32, f32) {
    let right = footer_buttons(sidebar_width)[2] + FOOTER_BUTTON;
    let left = (right - popover_width(sidebar_width)).max(ROW_INSET);
    (left, viewport_height - STATUS_BAR - FOOTER - POPOVER_DROP)
}

/// The most the list may take in a window `viewport_height` high: up to `LIST_MAX`, and short
/// enough that the whole panel stays under the top band (where the traffic lights are).
pub(crate) fn list_room(viewport_height: f32) -> f32 {
    let above = viewport_height - STATUS_BAR - FOOTER - POPOVER_DROP - TOP_BAND - PANEL_CHROME;
    above.clamp(0.0, LIST_MAX)
}

/// Entry `i`'s top in the window (the list not scrolled) of `count` entries, the popover open
/// above the footer: the list's foot is one padding and the edge above the anchor.
#[cfg_attr(not(feature = "ui-shots"), allow(dead_code))]
pub(crate) fn entry_top(i: usize, count: usize, sidebar_width: f32, viewport_height: f32) -> f32 {
    let (_, y) = popover_anchor(sidebar_width, viewport_height);
    let content = count as f32 * ENTRY + count.saturating_sub(1) as f32 * ENTRY_GAP;
    let list = content.min(list_room(viewport_height));
    y - 1.0 - PADDING - list + i as f32 * (ENTRY + ENTRY_GAP)
}

/// The popover's width from the collapsed band's 🔔 in a window `window_width` wide: inside the
/// window's margins, from `POPOVER_MIN` to `POPOVER_WIDTH`.
pub(crate) fn band_popover_width(window_width: f32) -> f32 {
    (window_width - 2.0 * EDGE_MARGIN).clamp(POPOVER_MIN, POPOVER_WIDTH)
}

/// The popover's top right corner from the collapsed band's 🔔: just under the band, its right
/// edge the band's (`BAND_EDGE` from the window's), so it drops down over the band's buttons.
pub(crate) fn band_popover_anchor(window_width: f32) -> (f32, f32) {
    (window_width - BAND_EDGE, TOP_BAND + POPOVER_DROP)
}

/// The most the list may take in the popover from the band, in a window `viewport_height` high:
/// up to `LIST_MAX`, and short enough that the whole panel stops `EDGE_MARGIN` above the status
/// bar.
pub(crate) fn band_list_room(viewport_height: f32) -> f32 {
    let below = viewport_height - TOP_BAND - POPOVER_DROP - PANEL_CHROME - STATUS_BAR - EDGE_MARGIN;
    below.clamp(0.0, LIST_MAX)
}

/// Entry `i`'s top in the window (the list not scrolled), the popover dropped from the band:
/// counted down from its top, under its edge, padding, header and the gap under it.
#[cfg_attr(not(feature = "ui-shots"), allow(dead_code))]
pub(crate) fn band_entry_top(i: usize) -> f32 {
    TOP_BAND + POPOVER_DROP + 1.0 + PADDING + HEADER + PADDING + i as f32 * (ENTRY + ENTRY_GAP)
}

/// The left edges, in a window `window_width` wide, of the collapsed band's buttons (🔔, ⚙,
/// sidebar): the sidebar button `BAND_EDGE` from the window's right edge (as the open one is
/// from the sidebar's), the others `BAND_GAP` apart to its left.
pub fn band_buttons(window_width: f32) -> [f32; 3] {
    let sidebar = window_width - BAND_EDGE - HIT;
    let gear = sidebar - BAND_GAP - HIT;
    [gear - BAND_GAP - HIT, gear, sidebar]
}

/// How far in from each side of the window the workspace's name keeps in the collapsed band:
/// clear of the traffic lights on the left and of the band's buttons on the right, the same on
/// both sides so it stays centred on the window (100 pt, windowed and full screen alike).
pub fn name_inset(full_screen: bool) -> f32 {
    sidebar_toggle::lights_clear(full_screen).max(BAND_GROUP) + NAME_GAP
}

/// Whether a window `width` wide has room for the name between its insets.
pub fn shows_name(width: f32, full_screen: bool) -> bool {
    width - 2.0 * name_inset(full_screen) >= NAME_MIN
}

/// Whether the open sidebar's band, `sidebar_width` wide in a window `window_width` wide, has
/// room for the title: a `TITLE_ROOM` box centred on its middle clear of the traffic lights on
/// the left and `TITLE_GAP` short of the sidebar button on the right (where it is drawn,
/// `open_button_at`: brought in from the band's edge in a window narrower than the sidebar). It
/// is drawn whole or not at all.
pub fn shows_title(sidebar_width: f32, window_width: f32, full_screen: bool) -> bool {
    let mid = sidebar_width / 2.0;
    mid - TITLE_ROOM / 2.0 >= sidebar_toggle::lights_clear(full_screen)
        && mid + TITLE_ROOM / 2.0 + TITLE_GAP
            <= sidebar_toggle::open_button_at(sidebar_width, window_width)
}

impl Workspace {
    /// A notification was raised: it is listed (seen at once while the popover is open).
    pub(crate) fn note_notification(&mut self, entry: Entry) {
        self.notifications.push(entry);
        if self.notifications_popover.is_some() {
            self.notifications.mark_seen();
        }
    }

    /// The 🔔: its popover opens with the keyboard, and everything in it counts as seen.
    pub(crate) fn open_notifications(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_overlays(Overlay::Notifications);
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        self.notifications_popover = Some(focus);
        self.notifications.mark_seen();
        cx.notify();
    }

    /// Escape or a click outside: the popover goes and the typing returns to the focused tab.
    fn dismiss_notifications(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.notifications_popover.take().is_some() {
            self.focus_active(window, cx);
            cx.notify();
        }
    }

    /// A click on an entry: the popover goes and terminal `sN` is shown (its workspace, pane
    /// and tab) with the keyboard, as ⇧⌘U shows one. Nothing more if it has closed since.
    pub(crate) fn jump_to_notification(
        &mut self,
        terminal: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.notifications_popover = None;
        if let Some(index) = self.index_of(terminal) {
            self.select(index, window, cx);
        }
        // Already the active one: it still takes the keyboard back from the popover.
        self.focus_active(window, cx);
        cx.notify();
    }

    /// The open popover, with its click-away backdrop: this run's notifications, newest first, or
    /// a line saying there are none yet. Above the footer's 🔔 with the sidebar shown; dropped
    /// down from the band's 🔔 with it hidden (M11m).
    pub(crate) fn render_notifications(
        &self,
        ui: UiColors,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let focus = self.notifications_popover.as_ref()?;
        let viewport = window.viewport_size();
        let (width, height) = (viewport.width.as_f32(), viewport.height.as_f32());
        // From the collapsed band's 🔔 it drops down; from the footer's it opens upward.
        let from_band = self.sidebar.hidden;
        let (panel_width, list_room) = if from_band {
            (band_popover_width(width), band_list_room(height))
        } else {
            (popover_width(self.sidebar.width), list_room(height))
        };
        let now = Instant::now();
        let rows: Vec<AnyElement> = self
            .notifications
            .shown(|id| self.index_of(id).is_some())
            .into_iter()
            .enumerate()
            .map(|(i, (entry, open))| self.render_notification(i, entry, open, now, ui, cx))
            .collect();
        let header = div()
            .flex()
            .items_center()
            .h(px(HEADER))
            .px(px(8.0))
            .text_size(px(text::META))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(hsla(ui.text_dim))
            .child("Notifications");
        let body = if rows.is_empty() {
            div()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .px(px(8.0))
                .pb(px(8.0))
                .child(
                    div()
                        .text_color(hsla(ui.text_dim))
                        .child("No notifications yet"),
                )
                .child(
                    div()
                        .text_size(px(text::META))
                        .text_color(hsla(ui.text_faint))
                        .child(
                            "A terminal that finishes or needs your input while you aren\u{2019}t \
                             looking shows up here.",
                        ),
                )
                .into_any_element()
        } else {
            // Up to the top band (down to the status bar from the band), at most `LIST_MAX`;
            // the rest scrolls.
            div()
                .id("notifications-list")
                .flex()
                .flex_col()
                .gap(px(ENTRY_GAP))
                .max_h(px(list_room))
                .overflow_y_scroll()
                .children(rows)
                .into_any_element()
        };
        let panel = overlay_panel::shell(&ui, panel_width)
            .p(px(PADDING))
            .gap(px(PADDING))
            .track_focus(focus)
            .key_context("TabMenu")
            .on_action(
                cx.listener(|this, _: &Dismiss, window, cx| this.dismiss_notifications(window, cx)),
            )
            // Its own clicks don't reach the backdrop under it.
            .on_mouse_down(MouseButton::Left, |_: &MouseDownEvent, _, cx| {
                cx.stop_propagation()
            })
            .on_mouse_down(MouseButton::Right, |_: &MouseDownEvent, _, cx| {
                cx.stop_propagation()
            })
            .child(header)
            .child(body);
        let (anchor, (x, y)) = if from_band {
            (Anchor::TopRight, band_popover_anchor(width))
        } else {
            (
                Anchor::BottomLeft,
                popover_anchor(self.sidebar.width, height),
            )
        };
        Some(
            div()
                .id("notifications-backdrop")
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .occlude()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _: &MouseDownEvent, window, cx| {
                        this.dismiss_notifications(window, cx)
                    }),
                )
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(|this, _: &MouseDownEvent, window, cx| {
                        this.dismiss_notifications(window, cx)
                    }),
                )
                .child(
                    anchored()
                        .anchor(anchor)
                        .position(point(px(x), px(y)))
                        .snap_to_window_with_margin(px(EDGE_MARGIN))
                        .child(panel),
                )
                .into_any_element(),
        )
    }

    /// One entry: its mark, its workspace's name (a script's: its title) and how long ago, and
    /// what it said under them. One whose terminal has closed is greyed (an idle mark, faint
    /// text, "closed") and can't be clicked; a script's from outside fuxx's terminals has
    /// nowhere to go, so it can't be clicked either.
    fn render_notification(
        &self,
        i: usize,
        entry: &Entry,
        open: bool,
        now: Instant,
        ui: UiColors,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let closed = entry.terminal.is_some() && !open;
        let mark = match (entry.status, closed) {
            (_, true) => status_mark(Status::Idle, ui),
            (Some(status), false) => status_mark(status, ui),
            // A script's: a plain dot.
            (None, false) => div()
                .size(px(SCRIPT_DOT))
                .rounded_full()
                .bg(hsla(ui.text_dim))
                .into_any_element(),
        };
        // A status's: the name of the workspace its terminal is in now (renamed, or the
        // terminal moved), while it's open; else as it was. A script's: its own title.
        let name = entry
            .status
            .and(entry.terminal)
            .and_then(|id| self.index_of(id))
            .and_then(|i| self.workspace_of(i))
            .map_or_else(|| entry.title.clone(), |ws| self.workspace_label(ws).into());
        let age = status::age_label(now.saturating_duration_since(entry.at));
        let age = if closed {
            format!("closed \u{b7} {age}")
        } else {
            age
        };
        let line = div()
            .flex()
            .items_center()
            .h(px(LINE))
            .gap(px(8.0))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .truncate()
                    .when(!closed, |name| name.font_weight(FontWeight::SEMIBOLD))
                    .text_color(hsla(if closed { ui.text_faint } else { ui.text }))
                    .child(name),
            )
            .child(
                div()
                    .flex_none()
                    .text_size(px(text::META))
                    .font_features(tabular())
                    .text_color(hsla(ui.text_faint))
                    .child(age),
            );
        let message = div()
            .h(px(SUB))
            .line_height(px(SUB))
            .min_w(px(0.0))
            .truncate()
            .text_size(px(text::META))
            .text_color(hsla(if closed { ui.text_faint } else { ui.text_dim }))
            .child(entry.message.clone());
        let hover = ui.panel_hover;
        div()
            .id(("notification", i))
            .flex_none()
            .flex()
            .items_start()
            .gap(px(8.0))
            .h(px(ENTRY))
            .px(px(8.0))
            .py(px(ENTRY_PAD))
            .rounded(px(6.0))
            .child(
                div()
                    .flex_none()
                    .pt(px((LINE - MARK_SLOT) / 2.0))
                    .child(mark_slot(mark)),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w(px(0.0))
                    .child(line)
                    .child(message),
            )
            .when_some(entry.terminal.filter(|_| open), |row, terminal| {
                row.cursor_pointer()
                    .hover(move |s| s.bg(hsla(hover)))
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        this.jump_to_notification(terminal, window, cx)
                    }))
            })
            .into_any_element()
    }

    /// The app's title for the open sidebar's band: centred on the band's full width (the
    /// sidebar's middle), vertically centred, never cut; `None` where it doesn't fit
    /// (`shows_title`). It takes no clicks, so the band under it still drags the window and a
    /// double-click still zooms.
    pub(crate) fn render_app_title(&self, ui: UiColors, window: &Window) -> Option<AnyElement> {
        let width = window.viewport_size().width.as_f32();
        if !shows_title(self.sidebar.width, width, window.is_fullscreen()) {
            return None;
        }
        Some(
            div()
                .absolute()
                .top_0()
                .bottom_0()
                .left_0()
                .right_0()
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .flex_none()
                        .whitespace_nowrap()
                        .text_size(px(text::NAME))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(hsla(ui.text_dim))
                        .child(TITLE),
                )
                .into_any_element(),
        )
    }

    /// The selected workspace's name for the collapsed band, placed at the window's root over
    /// both bands (the panes' and the right sidebar's): centred on the window between its
    /// insets (`name_inset`), cut with an ellipsis when the window is narrow (and left out when
    /// there's hardly room for one). It takes no clicks, so the bands under it still drag the
    /// window and a double-click still zooms.
    pub(crate) fn render_workspace_name(
        &self,
        ui: UiColors,
        window: &Window,
    ) -> Option<AnyElement> {
        let ws = self.model.focused_workspace()?;
        let full_screen = window.is_fullscreen();
        if !shows_name(window.viewport_size().width.as_f32(), full_screen) {
            return None;
        }
        let inset = name_inset(full_screen);
        Some(
            div()
                .absolute()
                .top_0()
                .h(px(TOP_BAND))
                .left(px(inset))
                .right(px(inset))
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .min_w(px(0.0))
                        .truncate()
                        .text_size(px(text::NAME))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(hsla(ui.text))
                        .child(self.workspace_label(ws)),
                )
                .into_any_element(),
        )
    }

    /// The collapsed band's buttons at the window's far right (M11m): [🔔] [⚙] [sidebar] at
    /// `band_buttons`, in the sidebar button's look. The 🔔 opens the notifications popover (lit
    /// on its fill while that shows, with the unseen dot while some are unseen), the ⚙ opens
    /// Preferences (as ⌘,), the sidebar button shows the sidebar. Each one's press is its own: it
    /// never drags the window. `None` while the sidebar is shown (its footer has the 🔔 and ⚙,
    /// its band the sidebar button).
    pub(crate) fn render_band_buttons(
        &self,
        ui: UiColors,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.sidebar.hidden {
            return None;
        }
        let [bell_x, gear_x, sidebar_x] = band_buttons(window.viewport_size().width.as_f32());
        let popover = self.notifications_popover.is_some();
        let unseen = self.notifications.unseen() > 0;
        let bell = band_button("band-notifications", BAND_BELL_GROUP, bell_x, ui)
            .when(popover, |b| b.bg(hsla(ui.panel_hover)))
            .child(footer_icon(Glyph::Bell, ui, BAND_BELL_GROUP, popover))
            .when(unseen, |b| b.child(unseen_dot(ui)))
            .tooltip(|_, cx| -> AnyView { cx.new(|_| Tip(NOTIFICATIONS_TIP.into())).into() })
            .on_click(
                cx.listener(|this, _: &ClickEvent, window, cx| this.open_notifications(window, cx)),
            );
        let gear = band_button("band-preferences", BAND_PREFERENCES_GROUP, gear_x, ui)
            .child(footer_icon(Glyph::Gear, ui, BAND_PREFERENCES_GROUP, false))
            .tooltip(|_, cx| -> AnyView { cx.new(|_| Tip(PREFERENCES_TIP.into())).into() })
            .on_click(|_: &ClickEvent, _, cx| preferences::open_or_activate(cx));
        Some(
            div()
                .absolute()
                .top_0()
                .left_0()
                .w_full()
                .h(px(TOP_BAND))
                .child(bell)
                .child(gear)
                .child(self.render_sidebar_button(ui, sidebar_x, cx))
                .into_any_element(),
        )
    }
}

/// `status`'s mark, still, on the popover.
fn status_mark(status: Status, ui: UiColors) -> AnyElement {
    status_mark::mark_on(status, &ui, 0.0, None, px(mark_size(status)), ui.panel).into_any_element()
}

/// One of the footer's icons (`kind`: the +, the 🔔 or the gear), a line icon in the tab bars'
/// way: the dim text colour, the full one on hover of `group` (or while `lit`: its menu or
/// popover is open). A drawn path can't take a hover colour, so a dim copy and a full one are
/// stacked and the group shows one.
pub(crate) fn footer_icon(kind: Glyph, ui: UiColors, group: &'static str, lit: bool) -> Div {
    let dim = if lit { ui.text } else { ui.text_dim };
    div()
        .relative()
        .size(px(ICON))
        .child(
            footer_glyph(kind, dim, ICON, STROKE)
                .absolute()
                .group_hover(group, |s| s.invisible()),
        )
        .child(
            footer_glyph(kind, ui.text, ICON, STROKE)
                .absolute()
                .invisible()
                .group_hover(group, |s| s.visible()),
        )
}

/// The unseen dot for the 🔔's top right, ringed in the sidebar's colour.
pub(crate) fn unseen_dot(ui: UiColors) -> Div {
    div()
        .absolute()
        .top(px(DOT_INSET))
        .right(px(DOT_INSET))
        .size(px(DOT))
        .rounded_full()
        .border_2()
        .border_color(hsla(ui.sidebar_background))
        .bg(hsla(ui.accent))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sidebar::{FOOTER, ROW_INSET};
    use crate::sidebar_toggle::{lights_clear, open_button_x};
    use crate::ui_tokens::size::{SIDEBAR_MAX_WIDTH, SIDEBAR_MIN_WIDTH};

    #[test]
    fn the_footers_three_buttons_fit_the_narrowest_sidebar_and_keep_apart() {
        for width in [SIDEBAR_MIN_WIDTH, 240.0, 260.0, 340.0, SIDEBAR_MAX_WIDTH] {
            let [plus, bell, gear] = footer_buttons(width);
            assert_eq!(plus, ROW_INSET, "the + at the footer's left inset");
            assert_eq!(
                gear + FOOTER_BUTTON + ROW_INSET,
                width,
                "the gear at the footer's right inset (the sidebar has no edge since M11i)"
            );
            assert_eq!(
                plus,
                width - (gear + FOOTER_BUTTON),
                "width {width}: the + and the gear mirror each other"
            );
            assert_eq!(
                bell + FOOTER_BUTTON + FOOTER_GAP,
                gear,
                "the bell beside the gear"
            );
            assert!(
                plus + FOOTER_BUTTON + FOOTER_GAP <= bell,
                "width {width}: no overlap"
            );
        }
        assert_eq!(FOOTER_BUTTON, 28.0);
        assert_eq!(FOOTER_GAP, 4.0);
    }

    #[test]
    fn the_popover_hangs_from_the_footers_right_buttons_over_the_sidebar() {
        use crate::ui_tokens::size::SIDEBAR_DEFAULT_WIDTH;
        for width in [
            SIDEBAR_MIN_WIDTH,
            SIDEBAR_DEFAULT_WIDTH,
            260.0,
            SIDEBAR_MAX_WIDTH,
        ] {
            let (x, _) = popover_anchor(width, 600.0);
            let (w, right) = (popover_width(width), x + popover_width(width));
            let [_, bell, gear] = footer_buttons(width);
            assert!(
                (POPOVER_MIN..=POPOVER_WIDTH).contains(&w),
                "width {width}: {w} wide"
            );
            assert!(
                x <= bell && bell + FOOTER_BUTTON <= right,
                "width {width}: over the bell"
            );
            if width - 2.0 * ROW_INSET >= POPOVER_MIN {
                // Its right edge is the gear's, and it stays over the sidebar.
                assert_eq!(
                    right,
                    gear + FOOTER_BUTTON,
                    "width {width}: the gear's edge"
                );
                assert!(x >= ROW_INSET, "width {width}: inside the sidebar");
            } else {
                // Narrower than the panel's least: from the footer's left inset, as the + menu.
                assert_eq!((x, w), (ROW_INSET, POPOVER_MIN), "width {width}");
            }
        }
        // At the default width it fills the sidebar inside the footer's insets.
        assert_eq!(
            popover_width(SIDEBAR_DEFAULT_WIDTH),
            SIDEBAR_DEFAULT_WIDTH - 2.0 * ROW_INSET
        );
    }

    #[test]
    fn the_popover_opens_upward_from_the_footer_clear_of_the_band() {
        for height in [300.0, 480.0, 600.0, 1000.0] {
            let (_, y) = popover_anchor(260.0, height);
            // The footer sits on the status bar (M11i).
            assert_eq!(
                y,
                height - STATUS_BAR - FOOTER - POPOVER_DROP,
                "just above the footer"
            );
            // The fullest list: its panel (header, padding and edge included) stays under
            // the top band, where the traffic lights are.
            let panel = PANEL_CHROME + list_room(height);
            assert!(
                y - panel >= TOP_BAND,
                "height {height}: {} vs {TOP_BAND}",
                y - panel
            );
        }
    }

    #[test]
    fn an_entrys_place_is_counted_up_from_the_footer() {
        let (_, y) = popover_anchor(260.0, 600.0);
        // The last of three ends one padding and the edge above the anchor.
        let last = entry_top(2, 3, 260.0, 600.0);
        assert_eq!(last + ENTRY, y - 1.0 - PADDING);
        assert_eq!(entry_top(1, 3, 260.0, 600.0) + ENTRY + ENTRY_GAP, last);
        // A list that scrolls fills `list_room`, whatever its length.
        let top = entry_top(0, 50, 260.0, 600.0);
        assert_eq!(top, y - 1.0 - PADDING - list_room(600.0));
    }

    #[test]
    fn the_band_buttons_line_up_bell_gear_sidebar_at_the_windows_right() {
        use crate::sidebar_toggle::BAND_EDGE;
        assert_eq!(band_buttons(960.0), [872.0, 900.0, 928.0]);
        for w in [240.0, 360.0, 760.0, 1600.0] {
            let b = band_buttons(w);
            assert_eq!(
                b[2] + HIT + BAND_EDGE,
                w,
                "width {w}: the sidebar button's edge"
            );
            assert_eq!(b[1] + HIT + BAND_GAP, b[2], "width {w}: the gear beside it");
            assert_eq!(
                b[0] + HIT + BAND_GAP,
                b[1],
                "width {w}: the bell beside the gear"
            );
            assert_eq!(w - b[0], BAND_GROUP, "width {w}: the group's width");
        }
        // The same distance from the window's edge as the open button keeps from the
        // sidebar's, which lines it up over the footer gear.
        assert_eq!(BAND_EDGE + HIT / 2.0, ROW_INSET + FOOTER_BUTTON / 2.0);
        assert_eq!(BAND_GROUP, 88.0);
    }

    #[test]
    fn the_band_popover_drops_from_the_band_inside_the_window() {
        use crate::sidebar_toggle::BAND_EDGE;
        // 300 pt: the window's margins bind the width (284 pt, under `POPOVER_WIDTH`).
        for w in [300.0, 360.0, 760.0, 960.0, 1600.0] {
            let (r, t) = band_popover_anchor(w);
            assert_eq!(r, w - BAND_EDGE, "width {w}: its right edge the band's");
            assert_eq!(t, TOP_BAND + POPOVER_DROP, "width {w}: just under the band");
            let pw = band_popover_width(w);
            assert!(
                (POPOVER_MIN..=POPOVER_WIDTH).contains(&pw),
                "width {w}: {pw} wide"
            );
            assert!(r - pw >= EDGE_MARGIN, "width {w}: inside the window");
            assert!(
                r - pw <= band_buttons(w)[0],
                "width {w}: it spans the 🔔 ({} vs {})",
                r - pw,
                band_buttons(w)[0]
            );
        }
        assert_eq!(band_popover_width(1600.0), POPOVER_WIDTH);
        assert_eq!(band_popover_width(300.0), 300.0 - 2.0 * EDGE_MARGIN);
    }

    #[test]
    fn the_band_popovers_list_stops_above_the_status_bar() {
        for h in [300.0, 480.0, 600.0, 1000.0] {
            let (_, t) = band_popover_anchor(960.0);
            assert!(
                t + PANEL_CHROME + band_list_room(h) <= h - STATUS_BAR - EDGE_MARGIN,
                "height {h}: {} vs {}",
                t + PANEL_CHROME + band_list_room(h),
                h - STATUS_BAR - EDGE_MARGIN
            );
            assert!(
                band_list_room(h) <= LIST_MAX,
                "height {h}: at most LIST_MAX"
            );
        }
        assert_eq!(band_list_room(100.0), 0.0);
    }

    #[test]
    fn a_band_entrys_place_is_counted_down_from_the_band() {
        assert_eq!(
            band_entry_top(0),
            TOP_BAND + POPOVER_DROP + 1.0 + PADDING + HEADER + PADDING
        );
        assert_eq!(band_entry_top(1), band_entry_top(0) + ENTRY + ENTRY_GAP);
    }

    #[test]
    fn the_name_keeps_clear_of_the_lights_and_the_band_buttons() {
        assert_eq!(name_inset(false), 100.0);
        assert_eq!(name_inset(true), 100.0);
        for fs in [false, true] {
            assert!(
                name_inset(fs) >= lights_clear(fs) + NAME_GAP,
                "full screen {fs}: clear of the lights"
            );
            assert!(
                name_inset(fs) >= BAND_GROUP + NAME_GAP,
                "full screen {fs}: clear of the band buttons"
            );
        }
    }

    #[test]
    fn the_title_shows_where_it_fits() {
        assert!(
            shows_title(240.0, 960.0, false),
            "the default sidebar shows it"
        );
        assert!(
            !shows_title(180.0, 960.0, false),
            "too narrow beside the lights"
        );
        assert!(shows_title(180.0, 960.0, true), "no lights in full screen");
        let t = 2.0 * (80.0 + TITLE_ROOM / 2.0);
        assert!(shows_title(t, 960.0, false), "at the threshold {t}");
        assert!(
            !shows_title(t - 0.5, 960.0, false),
            "just under the threshold {t}"
        );
        assert_eq!(TITLE, "the Fuxx?");
    }

    #[test]
    fn the_title_never_meets_the_lights_or_the_button() {
        for fs in [false, true] {
            let mut was = false;
            for k in 0..=440 {
                let w = 180.0 + k as f32 * 0.5;
                let shown = shows_title(w, w, fs);
                assert!(
                    shown || !was,
                    "full screen {fs}: shown, then not at {w} (not monotonic)"
                );
                was = shown;
                if shown {
                    assert!(
                        w / 2.0 - TITLE_ROOM / 2.0 >= lights_clear(fs),
                        "full screen {fs}, width {w}: over the lights"
                    );
                    assert!(
                        w / 2.0 + TITLE_ROOM / 2.0 + TITLE_GAP <= open_button_x(w),
                        "full screen {fs}, width {w}: into the button"
                    );
                }
            }
        }
    }

    #[test]
    fn in_a_window_narrower_than_the_sidebar_the_title_keeps_clear_of_the_button() {
        use crate::sidebar_toggle::open_button_at;
        use crate::ui_tokens::size::{SIDEBAR_MAX_WIDTH, SIDEBAR_MIN_WIDTH};
        // 240 pt sidebar, 200 pt window: the button comes in to 168, the title (88..152, 8 short
        // of it) still fits; at 180 pt the button (148) would meet it, so it goes.
        assert!(shows_title(240.0, 200.0, false));
        assert!(!shows_title(240.0, 180.0, false));
        for fs in [false, true] {
            let mut s = SIDEBAR_MIN_WIDTH;
            while s <= SIDEBAR_MAX_WIDTH {
                for k in 0..=220 {
                    let w = 60.0 + k as f32 * 2.0;
                    if shows_title(s, w, fs) {
                        let (left, right) =
                            (s / 2.0 - TITLE_ROOM / 2.0, s / 2.0 + TITLE_ROOM / 2.0);
                        assert!(
                            left >= lights_clear(fs),
                            "full screen {fs}, sidebar {s}, window {w}: over the lights"
                        );
                        assert!(
                            right + TITLE_GAP <= open_button_at(s, w),
                            "full screen {fs}, sidebar {s}, window {w}: into the button"
                        );
                    }
                    // A window at least as wide as the sidebar changes nothing.
                    if w >= s {
                        assert_eq!(shows_title(s, w, fs), shows_title(s, s, fs));
                    }
                }
                s += 5.0;
            }
        }
    }

    #[test]
    fn a_window_too_narrow_for_the_name_leaves_it_out() {
        for fs in [false, true] {
            assert!(shows_name(260.0, fs), "full screen {fs}: 260 pt shows it");
            assert!(!shows_name(259.5, fs), "full screen {fs}: 259.5 pt doesn't");
        }
    }
}
