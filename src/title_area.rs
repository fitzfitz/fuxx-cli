//! The title area (M11b): right of the sidebar button in the top band, a 🔔 that opens this
//! run's notifications (`notification_list`; a dot while some are unseen) and a + that makes a
//! new workspace (⌘N); and the selected workspace's name at the content's top.
//!
//! The name is drawn in the top band, centred over the content, wherever the content has one:
//! with the sidebar hidden (the band then runs over the panes). With the sidebar shown the pane
//! tab bars are the window's top (they drag it) and the selected row, filled with the accent,
//! already names the workspace, so no band is added to say it again and the terminals keep that
//! height.
//!
//! The popover is one of the overlays (one open at a time): it holds the keyboard so Escape
//! closes it, and a click anywhere else closes it too. Clicking an entry whose terminal is still
//! open shows that terminal (its workspace, pane and tab) as ⇧⌘U does; a closed one's entry is
//! greyed and does nothing. The dot follows the list, which changes only when a notification is
//! raised or the popover opens: no timer.
//!
//! `bell_origin`, `plus_origin` and `name_inset` are the pure geometry.

use std::time::Instant;

use gpui::{
    AnyElement, AnyView, ClickEvent, Context, Div, FontWeight, MouseButton, MouseDownEvent,
    Stateful, Window, anchored, div, point, prelude::*, px,
};

use crate::appearance::Appearance;
use crate::notification_list::Entry;
use crate::overlay_panel;
use crate::sidebar::{MARK_SLOT, mark_size, mark_slot, tabular};
use crate::sidebar_toggle::{self, HIT, HIT_RADIUS, Tip};
use crate::status::{self, Status};
use crate::status_mark;
use crate::tab_menu::Dismiss;
use crate::terminal_view::hsla;
use crate::theme::UiColors;
use crate::ui_tokens::size::TOP_BAND;
use crate::ui_tokens::text;
use crate::workspace::{Overlay, Workspace};

/// The space between the band's buttons.
const GAP: f32 = 4.0;
/// The space between the last button and the workspace's name, and the least room the name is
/// drawn in (less would show little more than an ellipsis).
const NAME_GAP: f32 = 12.0;
const NAME_MIN: f32 = 60.0;
/// The unseen dot, at the 🔔's top right, and the ring of the band's colour that keeps it apart
/// from the 🔔 (inside `DOT`).
pub(crate) const DOT: f32 = 10.0;
pub(crate) const DOT_INSET: f32 = 1.5;
/// The popover: its width, how far below the buttons it opens, how far from the window's edges
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
/// The 🔔's box (drawn as an outline, `bell_icon`).
const BELL: f32 = 14.0;
/// The + drawn as two strokes, like the sidebar button's outline.
const PLUS: f32 = 11.0;
const STROKE: f32 = 1.5;

/// The 🔔's top left in the window: right of the sidebar button, on the same line.
pub fn bell_origin(full_screen: bool) -> (f32, f32) {
    let (x, y) = sidebar_toggle::button_origin(full_screen);
    (x + HIT + GAP, y)
}

/// The +'s top left: right of the 🔔.
pub fn plus_origin(full_screen: bool) -> (f32, f32) {
    let (x, y) = bell_origin(full_screen);
    (x + HIT + GAP, y)
}

/// Entry `i`'s top in the window (the list not scrolled), under the 🔔 opened windowed.
#[cfg_attr(not(feature = "ui-shots"), allow(dead_code))]
pub(crate) fn entry_top(i: usize) -> f32 {
    let (_, y) = bell_origin(false);
    let list = y + HIT + POPOVER_DROP + 1.0 + PADDING + HEADER + PADDING;
    list + i as f32 * (ENTRY + ENTRY_GAP)
}

/// How far in from each side of the band the workspace's name keeps: clear of the buttons on
/// the left, and as far on the right so it stays centred.
pub fn name_inset(full_screen: bool) -> f32 {
    plus_origin(full_screen).0 + HIT + NAME_GAP
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

    /// The 🔔 (with its dot while some are unseen) and the +, absolutely placed over the top
    /// band right of the sidebar button, in the same spot whether the sidebar is shown or not.
    pub(crate) fn render_title_buttons(
        &self,
        ui: UiColors,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> [AnyElement; 2] {
        let full_screen = window.is_fullscreen();
        let unseen = self.notifications.unseen() > 0;
        // The band under the buttons: the sidebar's top, or (hidden) the content's.
        let band = if self.sidebar.hidden {
            cx.global::<Appearance>().theme.background
        } else {
            ui.sidebar_background
        };
        let bell = band_button("notifications-button", bell_origin(full_screen), ui)
            .child(bell_icon(ui))
            .when(unseen, |bell| {
                bell.child(
                    div()
                        .absolute()
                        .top(px(DOT_INSET))
                        .right(px(DOT_INSET))
                        .size(px(DOT))
                        .rounded_full()
                        .border_2()
                        .border_color(hsla(band))
                        .bg(hsla(ui.accent)),
                )
            })
            .on_click(cx.listener(|this, _, window, cx| this.open_notifications(window, cx)))
            .tooltip(|_, cx| -> AnyView { cx.new(|_| Tip("Notifications".into())).into() });
        let plus = band_button("new-workspace-button", plus_origin(full_screen), ui)
            .child(plus_icon(ui))
            .on_click(cx.listener(|this, _, window, cx| this.new_session(window, cx)))
            .tooltip(|_, cx| -> AnyView { cx.new(|_| Tip("New Workspace (⌘N)".into())).into() });
        [bell.into_any_element(), plus.into_any_element()]
    }

    /// The open popover under the 🔔, with its click-away backdrop: this run's notifications,
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
            // Down to near the window's foot, at most `LIST_MAX`; the rest scrolls.
            let room = window.viewport_size().height.as_f32() - TOP_BAND - 80.0;
            div()
                .id("notifications-list")
                .flex()
                .flex_col()
                .gap(px(ENTRY_GAP))
                .max_h(px(room.clamp(120.0, LIST_MAX)))
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
        let (x, y) = bell_origin(window.is_fullscreen());
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
                        .position(point(px(x), px(y + HIT + POPOVER_DROP)))
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

/// A button in the top band at `origin`: a hover fill, and a press that stays its own (the band
/// under it would start a window drag).
fn band_button(id: &'static str, (x, y): (f32, f32), ui: UiColors) -> Stateful<Div> {
    div()
        .id(id)
        .group(id)
        .absolute()
        .left(px(x))
        .top(px(y))
        .size(px(HIT))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(HIT_RADIUS))
        .hover(move |s| s.bg(hsla(ui.panel_hover)))
        .occlude()
        .on_mouse_down(MouseButton::Left, |_: &MouseDownEvent, _, cx| {
            cx.stop_propagation()
        })
}

/// The 🔔 as an outline in the sidebar button's stroke: a dome open at the bottom, its rim and
/// clapper, in the dim text colour, the full one on hover.
fn bell_icon(ui: UiColors) -> Div {
    const GROUP: &str = "notifications-button";
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

/// The +: two strokes in the dim text colour, the full one on hover.
fn plus_icon(ui: UiColors) -> Div {
    let stroke = || {
        div()
            .absolute()
            .bg(hsla(ui.text_dim))
            .group_hover("new-workspace-button", move |s| s.bg(hsla(ui.text)))
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
    use crate::sidebar_toggle::button_origin;
    use crate::ui_tokens::size::SIDEBAR_MIN_WIDTH;

    #[test]
    fn the_bell_and_the_plus_follow_the_sidebar_button_on_its_line() {
        for full_screen in [false, true] {
            let (sx, sy) = button_origin(full_screen);
            let (bx, by) = bell_origin(full_screen);
            let (px, py) = plus_origin(full_screen);
            assert_eq!((by, py), (sy, sy), "one line, centred in the band");
            assert_eq!(bx, sx + HIT + GAP);
            assert_eq!(px, bx + HIT + GAP);
            // All three fit over the narrowest sidebar, so they never hang off it.
            assert!(px + HIT <= SIDEBAR_MIN_WIDTH, "full screen {full_screen}");
        }
        // In full screen they move left with the sidebar button.
        assert!(bell_origin(true).0 < bell_origin(false).0);
    }

    #[test]
    fn the_name_keeps_clear_of_the_buttons() {
        for full_screen in [false, true] {
            let (px, _) = plus_origin(full_screen);
            assert_eq!(name_inset(full_screen), px + HIT + NAME_GAP);
        }
    }

    #[test]
    fn a_band_too_narrow_for_the_name_leaves_it_out() {
        let inset = name_inset(false);
        assert!(shows_name(2.0 * inset + NAME_MIN, false));
        assert!(!shows_name(2.0 * inset + NAME_MIN - 1.0, false));
        assert!(
            !shows_name(360.0, false),
            "the sidebar button scene's window"
        );
        assert!(shows_name(560.0, false));
        // Full screen's buttons sit further left: more room.
        assert!(shows_name(2.0 * inset + NAME_MIN - 1.0, true));
    }
}
