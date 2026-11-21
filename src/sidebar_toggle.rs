//! The sidebar button (M11): a sidebar-outline icon in the top band that hides or shows the
//! sidebar like ⌘\. With the sidebar shown it sits at the right edge of the sidebar's band, over
//! the footer gear (M11m), and rides the sidebar when it slides; hidden, it is the first of the
//! collapsed sidebar's buttons, just past the traffic lights (`title_area::band_buttons`). With motion
//! on, the sidebar slides in or out over 160 ms (`shown_fraction`); the pane column takes its
//! final width at once, so the terminals re-fit one time and not on every frame of the slide.

use std::time::{Duration, Instant};

use gpui::{
    AnyElement, AnyView, Context, Div, MouseButton, MouseDownEvent, SharedString, Stateful, Window,
    div, prelude::*, px,
};

use crate::appearance::Appearance;
use crate::terminal_view::hsla;
use crate::theme::UiColors;
use crate::ui_tokens::size::{CHROME_LINE, LIGHT, LIGHT_PITCH, TRAFFIC_LIGHTS};
use crate::workspace::Workspace;

/// How long the slide takes.
pub const SLIDE: Duration = Duration::from_millis(160);
/// The hit area's side, and its corner radius.
pub(crate) const HIT: f32 = 24.0;
pub(crate) const HIT_RADIUS: f32 = 6.0;
/// The icon's side (the icon set's panel-left since 2026-11-20; a drawn 15 × 12 outline before).
const ICON: f32 = 16.0;
/// How far the collapsed title band's buttons keep from the window's right edge (and the
/// open button, in a window narrower than its sidebar).
pub(crate) const BAND_EDGE: f32 = 8.0;
/// How far the open sidebar's button keeps from its band's right edge: its centre is then the
/// footer gear's (`OPEN_EDGE + HIT / 2 == FOOTER_SIDE + FOOTER_BUTTON / 2`; M11al).
pub(crate) const OPEN_EDGE: f32 = 12.0;
/// The button's top: its hit area centred on the window's line (`CHROME_LINE`), with the
/// traffic lights.
pub(crate) const BUTTON_Y: f32 = CHROME_LINE - HIT / 2.0;
/// The clear space between the green light's edge and the next icon (your call, 2026-11-20:
/// "add more acceptable gaps").
pub(crate) const LIGHTS_GAP: f32 = 12.0;
/// Where the traffic lights' area ends, the left edge of a button after them: its icon
/// `LIGHTS_GAP` past the green light (windowed), or the band's left margin (full screen, which
/// has no lights).
const LIGHTS_CLEAR: f32 =
    TRAFFIC_LIGHTS.0 + 2.0 * LIGHT_PITCH + LIGHT + LIGHTS_GAP - (HIT - ICON) / 2.0;
const LIGHTS_CLEAR_FULL_SCREEN: f32 = 13.0;

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

/// The first x clear of the traffic lights: 84 windowed, 13 in full screen.
pub fn lights_clear(full_screen: bool) -> f32 {
    if full_screen {
        LIGHTS_CLEAR_FULL_SCREEN
    } else {
        LIGHTS_CLEAR
    }
}

/// The open sidebar's button's left edge in a sidebar `sidebar_width` wide: at the band's right
/// edge, `OPEN_EDGE` in.
pub fn open_button_x(sidebar_width: f32) -> f32 {
    sidebar_width - OPEN_EDGE - HIT
}

/// Where the open sidebar's button is drawn in a window `window_width` wide: at the sidebar
/// band's right edge (`open_button_x`), or, in a window narrower than the sidebar (which isn't
/// clamped to it), `BAND_EDGE` in from the window's right edge so it can still be clicked.
pub fn open_button_at(sidebar_width: f32, window_width: f32) -> f32 {
    open_button_x(sidebar_width).min(window_width - BAND_EDGE - HIT)
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

/// ui-shots only: the text of the tooltip drawn last, for the scenes to read.
#[cfg(feature = "ui-shots")]
pub(crate) static SHOWN_TIP: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

impl Render for Tip {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        #[cfg(feature = "ui-shots")]
        if let Ok(mut shown) = SHOWN_TIP.lock() {
            *shown = Some(self.0.to_string());
        }
        let ui = cx.global::<Appearance>().theme.ui();
        tip_box(self.0.clone(), &ui)
    }
}

/// A tooltip's box with `text` (also drawn in place, as the Changes bar's tooltips above
/// their buttons are, M11v).
pub(crate) fn tip_box(text: SharedString, ui: &UiColors) -> Div {
    div()
        .px(px(8.0))
        .py(px(4.0))
        .rounded(px(6.0))
        .bg(hsla(ui.panel))
        .border_1()
        .border_color(hsla(ui.panel_border))
        .text_size(px(crate::ui_tokens::text::META))
        .text_color(hsla(ui.text))
        .whitespace_nowrap()
        .child(text)
}

/// The icon: the icon set's panel-left, dim, full on hover of the button.
fn icon(ui: UiColors) -> gpui::Svg {
    crate::icons::icon(crate::icons::Icon::PanelLeft, px(ICON), hsla(ui.text_dim))
        .group_hover("sidebar-toggle", move |s| s.text_color(hsla(ui.text)))
}

/// A band button's shell, absolutely placed at (`x`, `BUTTON_Y`) in its parent: a `HIT` square
/// with `HIT_RADIUS` corners, its contents centred, `panel_hover` under the pointer. Its press is
/// its own: it occludes the band under it, which would start a window drag on a press. The
/// sidebar button and the collapsed band's 🔔 and ⚙ (M11m) share it.
pub(crate) fn band_button(
    id: &'static str,
    group: &'static str,
    x: f32,
    ui: UiColors,
) -> Stateful<Div> {
    div()
        .id(id)
        .group(group)
        .absolute()
        .left(px(x))
        .top(px(BUTTON_Y))
        .size(px(HIT))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(HIT_RADIUS))
        .hover(move |s| s.bg(hsla(ui.panel_hover)))
        .occlude()
        // The band under it starts a window drag on a press; this press is the button's.
        .on_mouse_down(MouseButton::Left, |_: &MouseDownEvent, _, cx| {
            cx.stop_propagation()
        })
}

impl Workspace {
    /// The sidebar button, absolutely placed at (`x`, `BUTTON_Y`) in its parent: the sidebar's
    /// band (at `open_button_x`), or the collapsed band's buttons (at `band_buttons(W)[2]`).
    pub(crate) fn render_sidebar_button(
        &self,
        ui: UiColors,
        x: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let hidden = self.sidebar.hidden;
        band_button("sidebar-toggle", "sidebar-toggle", x, ui)
            .child(icon(ui))
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
    fn the_open_button_sits_at_the_sidebars_right_edge_on_the_gears_centre() {
        use crate::title_area::{FOOTER_BUTTON, footer_buttons};
        for w in [180.0, 240.0, 260.0, 340.0, 400.0] {
            assert_eq!(
                open_button_x(w) + HIT + OPEN_EDGE,
                w,
                "width {w}: the band's edge"
            );
            assert_eq!(
                open_button_x(w) + HIT / 2.0,
                footer_buttons(w)[2] + FOOTER_BUTTON / 2.0,
                "width {w}: over the footer gear's centre"
            );
        }
        // Centred on the lights: the window's one line (critique issue 3).
        assert_eq!(BUTTON_Y + HIT / 2.0, CHROME_LINE);
        assert_eq!(lights_clear(false), 84.0);
        assert_eq!(lights_clear(true), 13.0);
    }

    #[test]
    fn the_open_button_stays_in_a_window_narrower_than_the_sidebar() {
        use crate::ui_tokens::size::{SIDEBAR_MAX_WIDTH, SIDEBAR_MIN_WIDTH};
        // As wide as the sidebar or wider: at the sidebar band's right edge, as ever.
        assert_eq!(open_button_at(240.0, 960.0), open_button_x(240.0));
        assert_eq!(open_button_at(240.0, 240.0), open_button_x(240.0));
        // Narrower: `BAND_EDGE` in from the window's right edge.
        assert_eq!(open_button_at(240.0, 200.0), 200.0 - BAND_EDGE - HIT);
        let mut s = SIDEBAR_MIN_WIDTH;
        while s <= SIDEBAR_MAX_WIDTH {
            for k in 0..=200 {
                let w = 60.0 + k as f32 * 2.0;
                let x = open_button_at(s, w);
                assert!(
                    x + HIT + BAND_EDGE <= w,
                    "sidebar {s}, window {w}: off the window"
                );
                assert!(
                    x <= open_button_x(s),
                    "sidebar {s}, window {w}: past the band's edge"
                );
            }
            s += 10.0;
        }
    }

    /// Your screenshot (2026-11-20, "fix alignment and add more acceptable gaps"): the button
    /// was tight on the green light. The lights on macOS 26 are 14 pt, 22 apart; the icon now
    /// starts `LIGHTS_GAP` past the green one's edge.
    #[test]
    fn the_band_button_keeps_a_clear_gap_after_the_lights() {
        use crate::ui_tokens::size::{LIGHT, LIGHT_PITCH, TRAFFIC_LIGHTS};
        let green_edge = TRAFFIC_LIGHTS.0 + 2.0 * LIGHT_PITCH + LIGHT;
        assert_eq!(green_edge, 76.0);
        let icon_left = lights_clear(false) + (HIT - ICON) / 2.0;
        assert_eq!(icon_left - green_edge, LIGHTS_GAP);
        assert_eq!(LIGHTS_GAP, 12.0);
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
