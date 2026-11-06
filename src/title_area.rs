//! The title area (M11b): the selected workspace's name at the content's top, and the 🔔 of
//! this run's notifications (`notification_list`; a dot while some are unseen), which lives in
//! the sidebar's footer beside the Preferences gear (so it goes with the footer when the
//! sidebar is hidden).
//!
//! The name is drawn in the top band, centred over the content, wherever the content has one:
//! with the sidebar hidden (the band then runs over the panes). With the sidebar shown the pane
//! tab bars are the window's top (they drag it) and the selected row, filled with the accent,
//! already names the workspace, so no band is added to say it again and the terminals keep that
//! height. The band's one button is the sidebar's (`sidebar_toggle`).
//!
//! The popover is one of the overlays (one open at a time): it opens upward from the 🔔, holds
//! the keyboard so Escape closes it, and a click anywhere else closes it too. Clicking an entry
//! whose terminal is still open shows that terminal (its workspace, pane and tab) as ⇧⌘U does;
//! a closed one's entry is greyed and does nothing. The dot follows the list, which changes
//! only when a notification is raised or the popover opens: no timer.
//!
//! `footer_buttons`, `popover_anchor`, `list_room`, `entry_top` and `name_inset` are the pure
//! geometry.

use std::time::Instant;

use gpui::{
    Anchor, AnyElement, ClickEvent, Context, Div, FontWeight, MouseButton, MouseDownEvent, Window,
    anchored, div, point, prelude::*, px,
};

use crate::notification_list::Entry;
use crate::overlay_panel;
use crate::sidebar::{FOOTER, MARK_SLOT, ROW_INSET, mark_size, mark_slot, tabular};
use crate::sidebar_toggle::{self, HIT};
use crate::status::{self, Status};
use crate::status_mark;
use crate::tab_menu::Dismiss;
use crate::terminal_view::hsla;
use crate::theme::UiColors;
use crate::ui_tokens::size::TOP_BAND;
use crate::ui_tokens::text;
use crate::workspace::{Overlay, Workspace};

/// The footer's buttons (the +, the 🔔, the gear): each one's side, and the space between them.
pub(crate) const FOOTER_BUTTON: f32 = 24.0;
pub(crate) const FOOTER_GAP: f32 = 4.0;
/// The space between the sidebar button and the workspace's name, and the least room the name is
/// drawn in (less would show little more than an ellipsis).
const NAME_GAP: f32 = 12.0;
const NAME_MIN: f32 = 60.0;
/// The unseen dot, at the 🔔's top right, and the ring of the footer's colour that keeps it apart
/// from the 🔔 (inside `DOT`).
pub(crate) const DOT: f32 = 10.0;
pub(crate) const DOT_INSET: f32 = 1.5;
/// The popover: its width, how far above the footer it opens, how far from the window's edges
/// it keeps, and the most its list may take (the rest scrolls).
const POPOVER_WIDTH: f32 = 340.0;
const POPOVER_DROP: f32 = 4.0;
const EDGE_MARGIN: f32 = 8.0;
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
/// The footer's 🔔 button's hover group, and the 🔔's box (drawn as an outline, `bell_icon`).
pub(crate) const BELL_GROUP: &str = "notifications-button";
const BELL: f32 = 14.0;
/// The + drawn as two strokes, like the sidebar button's outline.
const PLUS: f32 = 11.0;
const STROKE: f32 = 1.5;

/// The popover's height around its list: its edge, padding, header and the gap under it.
const PANEL_CHROME: f32 = 2.0 + 2.0 * PADDING + HEADER + PADDING;

/// The left edges, in the sidebar `width` wide, of the footer's buttons: the + at the footer's
/// left inset, the gear at the right, the 🔔 beside it.
pub(crate) fn footer_buttons(width: f32) -> [f32; 3] {
    let gear = width - ROW_INSET - FOOTER_BUTTON;
    [ROW_INSET, gear - FOOTER_GAP - FOOTER_BUTTON, gear]
}

/// The popover's bottom left corner in the window: over the 🔔, just above the footer.
pub(crate) fn popover_anchor(sidebar_width: f32, viewport_height: f32) -> (f32, f32) {
    let bell = footer_buttons(sidebar_width)[1];
    (bell, viewport_height - FOOTER - POPOVER_DROP)
}

/// The most the list may take in a window `viewport_height` high: up to `LIST_MAX`, and short
/// enough that the whole panel stays under the top band (where the traffic lights are).
pub(crate) fn list_room(viewport_height: f32) -> f32 {
    let above = viewport_height - FOOTER - POPOVER_DROP - TOP_BAND - PANEL_CHROME;
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

/// How far in from each side of the band the workspace's name keeps: clear of the sidebar
/// button on the left, and as far on the right so it stays centred.
pub fn name_inset(full_screen: bool) -> f32 {
    sidebar_toggle::button_origin(full_screen).0 + HIT + NAME_GAP
}

/// Whether a band `width` wide has room for the name between its insets.
pub fn shows_name(width: f32, full_screen: bool) -> bool {
    width - 2.0 * name_inset(full_screen) >= NAME_MIN
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
        self.sidebar.new_menu = false;
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

    /// The open popover above the footer's 🔔, with its click-away backdrop: this run's notifications,
    /// newest first, or a line saying there are none yet.
    pub(crate) fn render_notifications(
        &self,
        ui: UiColors,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let focus = self.notifications_popover.as_ref()?;
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
            // Up to the top band, at most `LIST_MAX`; the rest scrolls.
            div()
                .id("notifications-list")
                .flex()
                .flex_col()
                .gap(px(ENTRY_GAP))
                .max_h(px(list_room(window.viewport_size().height.as_f32())))
                .overflow_y_scroll()
                .children(rows)
                .into_any_element()
        };
        let panel = overlay_panel::shell(&ui, POPOVER_WIDTH)
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
        let (x, y) = popover_anchor(self.sidebar.width, window.viewport_size().height.as_f32());
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
                        .anchor(Anchor::BottomLeft)
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

    /// The selected workspace's name for the top band over the content (with the sidebar
    /// hidden): centred, clear of the buttons, cut with an ellipsis when the window is narrow
    /// (and left out when there's hardly room for one). It takes no clicks, so the band under
    /// it still drags the window.
    pub(crate) fn render_workspace_name(
        &self,
        ui: UiColors,
        window: &Window,
    ) -> Option<AnyElement> {
        let ws = self.model.focused_workspace()?;
        let full_screen = window.is_fullscreen();
        // The band runs the window's width (the sidebar is hidden).
        if !shows_name(window.viewport_size().width.as_f32(), full_screen) {
            return None;
        }
        let inset = name_inset(full_screen);
        Some(
            div()
                .absolute()
                .top_0()
                .bottom_0()
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
}

/// `status`'s mark, still, on the popover.
fn status_mark(status: Status, ui: UiColors) -> AnyElement {
    status_mark::mark_on(status, &ui, 0.0, None, px(mark_size(status)), ui.panel).into_any_element()
}

/// The 🔔 as an outline in the sidebar button's stroke: a dome open at the bottom, its rim and
/// clapper, in the dim text colour, the full one on hover.
pub(crate) fn bell_icon(ui: UiColors) -> Div {
    const GROUP: &str = BELL_GROUP;
    let (dim, full) = (hsla(ui.text_dim), hsla(ui.text));
    let mut dome = div()
        .absolute()
        .left(px(2.5))
        .top(px(1.0))
        .w(px(BELL - 5.0))
        .h(px(9.5))
        .rounded_t(px((BELL - 5.0) / 2.0))
        .border_color(dim)
        .group_hover(GROUP, move |s| s.border_color(full));
    dome.style().border_widths.top = Some(px(STROKE).into());
    dome.style().border_widths.left = Some(px(STROKE).into());
    dome.style().border_widths.right = Some(px(STROKE).into());
    let fill = || {
        div()
            .absolute()
            .bg(dim)
            .group_hover(GROUP, move |s| s.bg(full))
    };
    let rim = fill()
        .left(px(0.5))
        .top(px(9.5))
        .w(px(BELL - 1.0))
        .h(px(STROKE))
        .rounded(px(STROKE / 2.0));
    let clapper = fill()
        .left(px((BELL - 3.0) / 2.0))
        .top(px(11.5))
        .w(px(3.0))
        .h(px(2.0))
        .rounded_b(px(1.5));
    div()
        .relative()
        .size(px(BELL))
        .child(dome)
        .child(rim)
        .child(clapper)
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

/// The +: two strokes in the dim text colour, the full one on hover of `group` (or while `lit`).
pub(crate) fn plus_icon(ui: UiColors, group: &'static str, lit: bool) -> Div {
    let stroke = || {
        div()
            .absolute()
            .bg(hsla(if lit { ui.text } else { ui.text_dim }))
            .group_hover(group, move |s| s.bg(hsla(ui.text)))
    };
    let at = (PLUS - STROKE) / 2.0;
    div()
        .relative()
        .size(px(PLUS))
        .child(stroke().left_0().top(px(at)).w(px(PLUS)).h(px(STROKE)))
        .child(stroke().top_0().left(px(at)).h(px(PLUS)).w(px(STROKE)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sidebar::{FOOTER, ROW_INSET};
    use crate::sidebar_toggle::button_origin;
    use crate::ui_tokens::size::{SIDEBAR_MAX_WIDTH, SIDEBAR_MIN_WIDTH};

    #[test]
    fn the_footers_three_buttons_fit_the_narrowest_sidebar_and_keep_apart() {
        for width in [SIDEBAR_MIN_WIDTH, 260.0, SIDEBAR_MAX_WIDTH] {
            let [plus, bell, gear] = footer_buttons(width);
            assert_eq!(plus, ROW_INSET, "the + at the footer's left inset");
            assert_eq!(
                gear + FOOTER_BUTTON + ROW_INSET,
                width,
                "the gear at the right"
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
    }

    #[test]
    fn the_popover_opens_upward_from_the_bell_clear_of_the_band() {
        for height in [300.0, 480.0, 600.0, 1000.0] {
            let (x, y) = popover_anchor(260.0, height);
            assert_eq!(x, footer_buttons(260.0)[1], "over the bell");
            assert_eq!(y, height - FOOTER - POPOVER_DROP, "just above the footer");
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
    fn the_name_keeps_clear_of_the_sidebar_button() {
        for full_screen in [false, true] {
            let (x, _) = button_origin(full_screen);
            assert_eq!(name_inset(full_screen), x + HIT + NAME_GAP);
        }
    }

    #[test]
    fn a_band_too_narrow_for_the_name_leaves_it_out() {
        let inset = name_inset(false);
        assert!(shows_name(2.0 * inset + NAME_MIN, false));
        assert!(!shows_name(2.0 * inset + NAME_MIN - 1.0, false));
        assert!(!shows_name(160.0, false));
        assert!(shows_name(360.0, false), "room now the + and 🔔 have left");
        // Full screen's button sits further left: more room.
        assert!(shows_name(2.0 * inset + NAME_MIN - 1.0, true));
    }
}
