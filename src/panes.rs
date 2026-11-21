//! Drawing a split layout inside the one floating card (M11al §3.3; the workspace draws the
//! card, `card`): nested flex rows/columns of panes, each its tab bar over its shown tab, with
//! a 1 px `card_border` hairline between them. In a split the unfocused panes are veiled (no
//! focus ring: a ring inside the card reads as a nested box). A 6 px grab strip centred on each
//! hairline resizes it, lit by the 2 px resize line in its middle while hovered or dragged.
//! Stateless; the tree, the tabs, the hover and the drag live in the workspace.

use std::rc::Rc;

use gpui::{
    AnyElement, App, BoxShadow, Div, Hsla, MouseButton, MouseDownEvent, Window, div, point,
    prelude::*, px, relative,
};

use crate::layout_model::PaneId;
use crate::split_tree::{Axis, Node};
use crate::terminal_view::hsla;
use crate::theme::UiColors;
use crate::ui_tokens::size::{
    CARD_RADIUS, CARD_SHADOW_BLUR, CARD_SHADOW_Y, DIVIDER_ACTIVE, HAIRLINE, UNFOCUSED_VEIL,
};

/// Called with a pane when it is clicked.
pub type SelectPane = Rc<dyn Fn(PaneId, &mut Window, &mut App)>;

/// Called with a split's path (from the root; `true` = second child) when its line is pressed.
pub type StartDrag = Rc<dyn Fn(Vec<bool>, &mut Window, &mut App)>;

/// Called with a split's path when the pointer enters (`true`) or leaves its line's grab strip.
pub type HoverDivider = Rc<dyn Fn(Vec<bool>, bool, &mut Window, &mut App)>;

/// Width of the invisible grab strip centred on each split's hairline (M11al: as M11i's 6 pt
/// gap was), and on the right sidebar's (§6.7).
pub(crate) const GRAB: f32 = 6.0;

/// The colors a layout is drawn with.
#[derive(Clone, Copy)]
pub struct PaneColors {
    /// Laid over an unfocused pane (the theme background, partly transparent).
    pub dim: Hsla,
    /// A line under the pointer or being dragged.
    pub accent: Hsla,
    /// The card's fill (the terminal background), and its 1 px edge and the hairlines between
    /// panes.
    pub card: Hsla,
    pub card_border: Hsla,
    /// The card's outer edge (the canvas colour, 2026-11-20), and the window's edge.
    pub rim: Hsla,
    pub window_edge: Hsla,
    /// The card's shadow: `shadow` at `shadow_alpha` (0-1).
    pub shadow: Hsla,
    pub shadow_alpha: f32,
}

/// The line on a resize edge under the pointer or being dragged (a pane split's, either
/// sidebar's): faint text at half strength, a quiet grey (M11x, your ask: the accent was "ugly").
pub fn resize_line(ui: &UiColors) -> Hsla {
    Hsla {
        a: 0.5,
        ..hsla(ui.text_faint)
    }
}

impl PaneColors {
    /// The layout's colours from the theme's UI colours.
    pub fn of(ui: &UiColors) -> PaneColors {
        PaneColors {
            dim: Hsla {
                a: UNFOCUSED_VEIL,
                ..hsla(ui.card)
            },
            accent: resize_line(ui),
            card: hsla(ui.card),
            card_border: hsla(ui.card_border),
            rim: hsla(ui.sidebar_background),
            window_edge: hsla(ui.window_edge),
            shadow: hsla(ui.shadow),
            shadow_alpha: ui.card_shadow_alpha,
        }
    }
}

/// The floating card (M11al §3.1; the one the panes sit in, and until it moves in, the right
/// sidebar's): the card's fill, `CARD_RADIUS` corners, a `HAIRLINE` edge in the canvas colour
/// (since 2026-11-20, your words: "not adding an outer border to the inner floating window"), and
/// its soft shadow (`CARD_SHADOW_Y` down, `CARD_SHADOW_BLUR` blurred). No light line along its top
/// (part 8 of the chores, your words: "the top inset is ugly").
/// Its content is clipped to the inside of its edge (a rectangle: content that reaches a corner
/// rounds its own background, see note 60 §5).
pub fn card(colors: &PaneColors) -> Div {
    div()
        .bg(colors.card)
        .rounded(px(CARD_RADIUS))
        .border(px(HAIRLINE))
        .border_color(colors.rim)
        .shadow(vec![BoxShadow {
            color: Hsla {
                a: colors.shadow_alpha,
                ..colors.shadow
            },
            offset: point(px(0.0), px(CARD_SHADOW_Y)),
            blur_radius: px(CARD_SHADOW_BLUR),
            spread_radius: px(0.0),
            inset: false,
        }])
        .overflow_hidden()
}

/// The window's corner radius on macOS 26 (measured from a screenshot: 14 pt).
pub(crate) const WINDOW_RADIUS: f32 = 14.0;

/// The window edge's width in points: one screen pixel (part 8 of the chores), just the pixel
/// macOS's own light rim lies over, so no darker pixel shows inside it on a Retina screen.
pub(crate) fn window_edge_width(scale: f32) -> f32 {
    1.0 / scale.max(1.0)
}

/// The window's outermost edge: a one-pixel line in `window_edge` along the window's four
/// straight sides, drawn over the window's content at its root, right under macOS's own light
/// rim, which adds light over it (black on a dark theme: the rim at its darkest; part 8 of the
/// chores, your words: "almost the same as the outer background"). It stops `WINDOW_RADIUS`
/// short of each corner: macOS's corner is a smooth curve, not a circle, so a circular corner
/// of it showed inside the rim as "a slight border or shadow" on the 4 corners (your report,
/// 2026-11-21); the corners are macOS's own. Left out in full screen. It takes no clicks.
pub fn window_edge(colors: &PaneColors, width: f32) -> Div {
    let (r, w, ink) = (px(WINDOW_RADIUS), px(width), colors.window_edge);
    let line = || div().absolute().bg(ink);
    div()
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .child(line().top_0().left(r).right(r).h(w))
        .child(line().bottom_0().left(r).right(r).h(w))
        .child(line().left_0().top(r).bottom(r).w(w))
        .child(line().right_0().top(r).bottom(r).w(w))
}

/// What every pane of one layout is drawn with.
pub struct Panes<'a> {
    /// The focused pane.
    pub active: PaneId,
    /// Each pane's shown tab, as the element to draw (the workspace decides how it's embedded).
    pub view: &'a dyn Fn(PaneId) -> Option<AnyElement>,
    /// Each pane's tab bar, above its shown tab.
    pub bar: &'a dyn Fn(PaneId) -> Option<AnyElement>,
    pub select: SelectPane,
    pub start_drag: StartDrag,
    pub hover: HoverDivider,
    /// The path of the split whose line the pointer is over, and of the one being dragged.
    pub hovered: Option<&'a [bool]>,
    pub dragging: Option<&'a [bool]>,
    pub colors: PaneColors,
}

/// A number for the split at `path`, to identify its grab strip (a leading 1 keeps `[false]` and
/// `[false, false]` apart).
fn path_key(path: &[bool]) -> usize {
    path.iter()
        .fold(1, |key, &second| key << 1 | usize::from(second))
}

/// Whether the divider of the split at `path` is drawn as the accent line: the pointer is over
/// it, or it is the one being dragged.
pub fn divider_active(path: &[bool], hovered: Option<&[bool]>, dragging: Option<&[bool]>) -> bool {
    hovered == Some(path) || dragging == Some(path)
}

/// How far from the split's `ratio` of its width its grab strip (`grab` wide) starts, so that
/// it is centred on the hairline (`line` wide): the sides give up the line in proportion to
/// their shares, so the line starts at `ratio * (width - line)`, and the strip `(grab - line)
/// / 2` before that.
pub fn grab_shift(ratio: f32, line: f32, grab: f32) -> f32 {
    -line * ratio - (grab - line) / 2.0
}

/// Where the 2 px resize line starts in a split's grab strip (`grab` wide): its middle, so over
/// the hairline's.
pub fn accent_offset(grab: f32) -> f32 {
    (grab - DIVIDER_ACTIVE) / 2.0
}

/// `tree` filling its parent (the card's inside).
pub fn render_layout(tree: &Node<PaneId>, panes: &Panes) -> AnyElement {
    render_node(tree, Vec::new(), panes)
}

/// `node`, reached from the root by `path`.
fn render_node(node: &Node<PaneId>, path: Vec<bool>, panes: &Panes) -> AnyElement {
    match node {
        Node::Leaf(id) => {
            let id = *id;
            let select = panes.select.clone();
            // Clipped as its card was (M11i): a narrow pane's content never paints into its
            // neighbour.
            div()
                .flex()
                .flex_col()
                .size_full()
                .overflow_hidden()
                .children((panes.bar)(id))
                .child(
                    // The terminal's area: what's left under the bar. (The bar handles its own
                    // clicks, so a click in a rename box there stays in it.)
                    div()
                        .relative()
                        .flex_1()
                        .min_h(px(0.0))
                        .w_full()
                        // Capture phase: runs before the terminal's own mouse-down (which still
                        // runs).
                        .capture_any_mouse_down(move |_: &MouseDownEvent, window, cx| {
                            select(id, window, cx)
                        })
                        .children((panes.view)(id))
                        // No listeners, so no hitbox: clicks go through to the terminal
                        // underneath. Rounded at the bottom as the inside of the card's edge,
                        // as the terminal is: a pane at the card's bottom corner keeps it inside
                        // the curve; elsewhere the corner shows the card, the terminal's colour.
                        .when(id != panes.active, |pane| {
                            pane.child(
                                div()
                                    .absolute()
                                    .top_0()
                                    .left_0()
                                    .size_full()
                                    .rounded_b(px(CARD_RADIUS - HAIRLINE))
                                    .bg(panes.colors.dim),
                            )
                        }),
                )
                .into_any_element()
        }
        Node::Split {
            axis,
            ratio,
            first,
            second,
        } => {
            let row = *axis == Axis::Row;
            let child_path = |to_second: bool| {
                let mut p = path.clone();
                p.push(to_second);
                p
            };
            // Each side takes its share; they shrink (in proportion) to make room for the
            // hairline. A side may shrink below its content (`min_*` 0).
            let side = |node: &Node<PaneId>, share: f32, to_second: bool| {
                div()
                    .flex_basis(relative(share))
                    .flex_grow_0()
                    .flex_shrink_1()
                    .min_w(px(0.0))
                    .min_h(px(0.0))
                    .map(|d| if row { d.h_full() } else { d.w_full() })
                    .child(render_node(node, child_path(to_second), panes))
            };
            // The hairline between the two panes (M11al §3.3).
            let line = div().flex_none().bg(panes.colors.card_border).map(|d| {
                if row {
                    d.w(px(HAIRLINE)).h_full()
                } else {
                    d.h(px(HAIRLINE)).w_full()
                }
            });
            // Last child, so it sits above both panes' terminals. It blocks their clicks under it
            // (so pressing it doesn't select text or switch panes) but lets scrolling through.
            let start_drag = panes.start_drag.clone();
            let here = path.clone();
            let hover = panes.hover.clone();
            let hover_path = path.clone();
            let active = divider_active(&path, panes.hovered, panes.dragging);
            // The resize line is drawn in the grab strip (the last child, so over both panes),
            // in its middle, over the hairline (`accent_offset`).
            let accent = div().absolute().bg(panes.colors.accent).map(|d| {
                let from = accent_offset(GRAB);
                if row {
                    d.top_0().h_full().w(px(DIVIDER_ACTIVE)).left(px(from))
                } else {
                    d.left_0().w_full().h(px(DIVIDER_ACTIVE)).top(px(from))
                }
            });
            let grab = div()
                .id(("divider", path_key(&path)))
                .absolute()
                .block_mouse_except_scroll()
                .on_hover(move |over: &bool, window, cx| {
                    hover(hover_path.clone(), *over, window, cx)
                })
                .when(active, |d| d.child(accent))
                .map(|d| {
                    if row {
                        d.top_0()
                            .h_full()
                            .w(px(GRAB))
                            .left(relative(*ratio))
                            .ml(px(grab_shift(*ratio, HAIRLINE, GRAB)))
                            .cursor_col_resize()
                    } else {
                        d.left_0()
                            .w_full()
                            .h(px(GRAB))
                            .top(relative(*ratio))
                            .mt(px(grab_shift(*ratio, HAIRLINE, GRAB)))
                            .cursor_row_resize()
                    }
                })
                .on_mouse_down(MouseButton::Left, move |_: &MouseDownEvent, window, cx| {
                    start_drag(here.clone(), window, cx)
                });
            div()
                .relative()
                .flex()
                .map(|d| if row { d.flex_row() } else { d.flex_col() })
                .size_full()
                .min_w(px(0.0))
                .min_h(px(0.0))
                .child(side(first, *ratio, false))
                .child(line)
                .child(side(second, 1.0 - *ratio, true))
                .child(grab)
                .into_any_element()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Part 8 of the chores: the window's edge is one screen pixel, the one macOS's rim covers.
    #[test]
    fn the_cards_corners_follow_the_windows() {
        // The dark-panel chore (your words: "the border radius on the floating panel, is not
        // matching the window (outter border) border radius"): 14 (the window's own) widened
        // the gap at the corner, the strict parallel curve (14 − 8 = 6) read too tight against
        // macOS's smooth corner; you picked 9 from renders side by side, then "add more border
        // radius": 11, the next of those renders.
        assert_eq!(CARD_RADIUS, 11.0);
        let parallel = WINDOW_RADIUS - crate::ui_tokens::size::CARD_INSET;
        assert!(parallel < CARD_RADIUS && CARD_RADIUS < WINDOW_RADIUS);
    }

    #[test]
    fn the_window_edge_is_one_screen_pixel() {
        assert_eq!(window_edge_width(1.0), 1.0);
        assert_eq!(window_edge_width(2.0), 0.5);
        assert_eq!(window_edge_width(0.0), 1.0, "no scale yet: a whole point");
    }

    #[test]
    fn the_grab_strip_is_centred_on_the_hairline_at_any_ratio() {
        // A split `w` wide: the sides take their shares and give up the 1 px hairline in
        // proportion (flex shrink), so the first side ends, and the line starts, at
        // `ratio * (w - line)`. The strip (`GRAB` wide) is placed at `ratio * w`, shifted by
        // `grab_shift`: its middle is the line's middle.
        let line = HAIRLINE;
        for w in [400.0_f32, 961.0] {
            for ratio in [0.1, 0.5, 0.9] {
                let line_middle = ratio * (w - line) + line / 2.0;
                let strip_start = ratio * w + grab_shift(ratio, line, GRAB);
                let strip_middle = strip_start + GRAB / 2.0;
                assert!(
                    (strip_middle - line_middle).abs() < 0.01,
                    "{w} at {ratio}: strip middle {strip_middle}, line middle {line_middle}"
                );
            }
        }
        assert_eq!(GRAB, 6.0, "the strip stays 6 px wide");
    }

    #[test]
    fn the_accent_line_is_centred_in_the_strip() {
        // The 2 px line sits as far from either side of the strip, so over the hairline's
        // middle, at any ratio.
        assert_eq!(accent_offset(6.0), 2.0);
        let line = accent_offset(GRAB);
        let (before, after) = (line, GRAB - line - DIVIDER_ACTIVE);
        assert!(line >= 0.0 && after >= 0.0, "{line}");
        assert!((before - after).abs() < 1e-4, "{before} vs {after}");
        // Its middle is the hairline's middle (the strip's middle).
        let hairline_middle = (GRAB - HAIRLINE) / 2.0 + HAIRLINE / 2.0;
        assert!((line + DIVIDER_ACTIVE / 2.0 - hairline_middle).abs() < 1e-4);
    }

    #[test]
    fn a_divider_lights_up_on_hover_or_drag_only_for_its_own_split() {
        let here = [true, false];
        let other = [true];
        assert!(!divider_active(&here, None, None));
        assert!(divider_active(&here, Some(&here), None));
        assert!(divider_active(&here, None, Some(&here)));
        assert!(!divider_active(&here, Some(&other), None));
        assert!(!divider_active(&here, None, Some(&[])));
        assert!(!divider_active(&[], Some(&here), Some(&other)));
    }
}
