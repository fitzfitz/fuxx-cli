//! The sidebar button (M11): a sidebar-outline icon in the top band, just right of the window's
//! traffic lights, that hides or shows the sidebar like ⌘\. It sits in the same spot whether
//! the sidebar is shown or hidden. With motion on, the sidebar slides in or out over 160 ms
//! (`shown_fraction`); the pane column takes its final width at once, so the terminals re-fit
//! one time and not on every frame of the slide.

use std::time::{Duration, Instant};

use gpui::{
    AnyElement, AnyView, Context, Div, MouseButton, MouseDownEvent, SharedString, Window, div,
    prelude::*, px,
};

use crate::appearance::Appearance;
use crate::terminal_view::hsla;
use crate::theme::UiColors;
use crate::ui_tokens::size::TOP_BAND;
use crate::workspace::Workspace;

/// How long the slide takes.
pub const SLIDE: Duration = Duration::from_millis(160);
/// The hit area's side, and its corner radius.
const HIT: f32 = 24.0;
const HIT_RADIUS: f32 = 6.0;
/// The icon: a 16 × 13 rounded outline with a divider a third of the way in.
const ICON: (f32, f32) = (16.0, 13.0);
const STROKE: f32 = 1.5;
const ICON_RADIUS: f32 = 3.0;
/// The button's left edge: just right of the green button (windowed), or where the traffic
/// lights would start (full screen).
const X_WINDOWED: f32 = 80.0;
const X_FULL_SCREEN: f32 = 13.0;

/// A slide in progress: towards shown or hidden, since when, and from how much shown (a
/// slide turned back half way starts where the other one had got to).
#[derive(Clone, Copy)]
pub struct Slide {
    pub showing: bool,
    pub since: Instant,
    pub from: f32,
}

impl Slide {
    /// A slide towards `showing` starting now, from where `current` (the slide under way, if
    /// any) has got to: fully in or out when there's none.
    pub fn start(showing: bool, current: Option<Slide>) -> Self {
        let from = current.map_or(if showing { 0.0 } else { 1.0 }, |s| s.shown());
        Slide {
            showing,
            since: Instant::now(),
            from,
        }
    }

    /// How much of the sidebar shows now.
    pub fn shown(&self) -> f32 {
        shown_fraction(self.since.elapsed(), self.from, self.showing)
    }
}

/// The button's top left in the window: x just right of the traffic lights (13 in full screen),
/// y so the hit area is centred in the top band, on the lights.
pub fn button_origin(full_screen: bool) -> (f32, f32) {
    let x = if full_screen {
        X_FULL_SCREEN
    } else {
        X_WINDOWED
    };
    (x, (TOP_BAND - HIT) / 2.0)
}

/// How much of the sidebar's width is on screen `elapsed` into a slide from `from` towards
/// `showing` (0 to 1), easing out: fast at first, settling at the end.
pub fn shown_fraction(elapsed: Duration, from: f32, showing: bool) -> f32 {
    let t = (elapsed.as_secs_f32() / SLIDE.as_secs_f32()).clamp(0.0, 1.0);
    let eased = 1.0 - (1.0 - t).powi(3);
    let to = if showing { 1.0 } else { 0.0 };
    from + (to - from) * eased
}

/// The tooltip's text for the sidebar being `hidden` or not.
pub fn tooltip_text(hidden: bool) -> &'static str {
    if hidden {
        "Show sidebar (⌘\\)"
    } else {
        "Hide sidebar (⌘\\)"
    }
}

/// The tooltip's view.
pub(crate) struct Tip(pub SharedString);

impl Render for Tip {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ui = cx.global::<Appearance>().theme.ui();
        div()
            .px(px(8.0))
            .py(px(4.0))
            .rounded(px(6.0))
            .bg(hsla(ui.panel))
            .border_1()
            .border_color(hsla(ui.panel_border))
            .text_size(px(crate::ui_tokens::text::META))
            .text_color(hsla(ui.text))
            .child(self.0.clone())
    }
}

/// The icon: an outline with a divider, both in the current text colour of the `group`.
fn icon(ui: UiColors) -> Div {
    let outline = |mut d: Div| {
        d.style().border_widths.top = Some(px(STROKE).into());
        d.style().border_widths.bottom = Some(px(STROKE).into());
        d.style().border_widths.left = Some(px(STROKE).into());
        d.style().border_widths.right = Some(px(STROKE).into());
        d
    };
    let divider = div()
        .absolute()
        .top_0()
        .bottom_0()
        .left(px(ICON.0 / 3.0 - STROKE / 2.0))
        .w(px(STROKE))
        .bg(hsla(ui.text_dim))
        .group_hover("sidebar-toggle", move |s| s.bg(hsla(ui.text)));
    outline(div())
        .relative()
        .w(px(ICON.0))
        .h(px(ICON.1))
        .rounded(px(ICON_RADIUS))
        .border_color(hsla(ui.text_dim))
        .group_hover("sidebar-toggle", move |s| s.border_color(hsla(ui.text)))
        .child(divider)
}

impl Workspace {
    /// The sidebar button, absolutely placed over the top band (and over the sliding sidebar).
    pub(crate) fn render_sidebar_button(
        &self,
        ui: UiColors,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (x, y) = button_origin(window.is_fullscreen());
        let hidden = self.sidebar.hidden;
        div()
            .id("sidebar-toggle")
            .group("sidebar-toggle")
            .absolute()
            .left(px(x))
            .top(px(y))
            .size(px(HIT))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(HIT_RADIUS))
            .hover(move |s| s.bg(hsla(ui.panel_hover)))
            .child(icon(ui))
            .occlude()
            // The band under it starts a window drag on a press; this press is the button's.
            .on_mouse_down(MouseButton::Left, |_: &MouseDownEvent, _, cx| {
                cx.stop_propagation()
            })
            .on_click(cx.listener(|this, _, window, cx| this.toggle_sidebar(window, cx)))
            .tooltip(move |_, cx| -> AnyView {
                cx.new(|_| Tip(tooltip_text(hidden).into())).into()
            })
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_button_sits_right_of_the_traffic_lights_and_moves_to_the_edge_in_full_screen() {
        let (x, y) = button_origin(false);
        assert_eq!(x, 80.0);
        // Centred on the lights: the band's middle.
        assert_eq!(y + HIT / 2.0, TOP_BAND / 2.0);
        assert_eq!(button_origin(true), (13.0, y));
    }

    #[test]
    fn the_slide_eases_out_and_ends_on_the_target() {
        let at = |ms: u64, showing| {
            let from = if showing { 0.0 } else { 1.0 };
            shown_fraction(Duration::from_millis(ms), from, showing)
        };
        assert_eq!(at(0, true), 0.0);
        assert_eq!(at(0, false), 1.0);
        assert_eq!(at(160, true), 1.0);
        assert_eq!(at(160, false), 0.0);
        assert_eq!(at(400, true), 1.0, "stays at the end");
        // Ease-out: past half way by half the time, and always rising.
        assert!(at(80, true) > 0.5);
        assert!(at(40, true) < at(80, true) && at(80, true) < at(120, true));
        // Showing and hiding mirror each other.
        assert!((at(50, true) + at(50, false) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn a_slide_turned_back_half_way_starts_where_the_other_had_got_to() {
        // Hiding, 60 ms in, then shown again: the new slide starts at that fraction, no jump.
        let hiding = Slide {
            showing: false,
            since: Instant::now() - Duration::from_millis(60),
            from: 1.0,
        };
        let now = hiding.shown();
        assert!(now > 0.0 && now < 1.0, "{now}");
        let back = Slide::start(true, Some(hiding));
        assert!((back.from - now).abs() < 0.02, "{} vs {now}", back.from);
        let at = |ms: u64| shown_fraction(Duration::from_millis(ms), 0.4, true);
        assert_eq!(at(0), 0.4);
        assert_eq!(at(160), 1.0);
        assert!(at(40) > 0.4 && at(40) < at(80) && at(80) < 1.0);
        // Hiding from part way, too.
        assert_eq!(shown_fraction(Duration::ZERO, 0.7, false), 0.7);
        assert_eq!(shown_fraction(Duration::from_millis(160), 0.7, false), 0.0);
        // No slide under way: from fully out (showing) or fully in (hiding).
        assert_eq!(Slide::start(true, None).from, 0.0);
        assert_eq!(Slide::start(false, None).from, 1.0);
    }

    #[test]
    fn the_tooltip_names_what_a_click_does() {
        assert_eq!(tooltip_text(false), "Hide sidebar (⌘\\)");
        assert_eq!(tooltip_text(true), "Show sidebar (⌘\\)");
    }
}
