//! Drawing a split layout: nested flex rows/columns of panes, each a raised card (M11i: its
//! tab bar and shown tab on the card's fill, rounded, edged and shadowed) with a 6 pt gap of
//! canvas between cards. In a split, the focused pane's edge is the accent (the focus ring)
//! and the unfocused panes are dimmed. Each gap is the grab strip for resizing, lit by a 2 pt
//! accent line in its middle while hovered or dragged. Stateless; the tree, the tabs, the
//! hover and the drag live in the workspace.

use std::rc::Rc;

use gpui::{
    AnyElement, App, BoxShadow, Div, Hsla, MouseButton, MouseDownEvent, Window, div, point,
    prelude::*, px, relative,
};

use crate::layout_model::PaneId;
use crate::split_tree::{Axis, Node};
use crate::terminal_view::hsla;
use crate::theme::UiColors;
use crate::ui_tokens::size::{CARD_GAP, CARD_RADIUS, DIVIDER_ACTIVE, FOCUS_RING, UNFOCUSED_VEIL};

/// Called with a pane when it is clicked.
pub type SelectPane = Rc<dyn Fn(PaneId, &mut Window, &mut App)>;

/// Called with a split's path (from the root; `true` = second child) when its line is pressed.
pub type StartDrag = Rc<dyn Fn(Vec<bool>, &mut Window, &mut App)>;

/// Called with a split's path when the pointer enters (`true`) or leaves its line's grab strip.
pub type HoverDivider = Rc<dyn Fn(Vec<bool>, bool, &mut Window, &mut App)>;

/// Width of the invisible grab strip centred on each split's ratio: the gap between its cards.
const GRAB: f32 = CARD_GAP;

/// A card's shadow (M11i): straight down this far, blurred this much.
const CARD_SHADOW_Y: f32 = 1.0;
const CARD_SHADOW_BLUR: f32 = 6.0;

/// The colors a layout is drawn with.
#[derive(Clone, Copy)]
pub struct PaneColors {
    /// Laid over an unfocused pane (the theme background, partly transparent).
    pub dim: Hsla,
    /// A line under the pointer or being dragged.
    pub accent: Hsla,
    /// A card's fill (the terminal background), its 1 pt edge, and the focused pane's edge in
    /// a split.
    pub card: Hsla,
    pub card_border: Hsla,
    pub focus: Hsla,
    /// The cards' shadow: `shadow` at `shadow_alpha` (0-1).
    pub shadow: Hsla,
    pub shadow_alpha: f32,
}

impl PaneColors {
    /// The layout's colours from the theme's UI colours.
    pub fn of(ui: &UiColors) -> PaneColors {
        PaneColors {
            dim: Hsla {
                a: UNFOCUSED_VEIL,
                ..hsla(ui.card)
            },
            accent: hsla(ui.accent),
            card: hsla(ui.card),
            card_border: hsla(ui.card_border),
            focus: hsla(ui.accent),
            shadow: hsla(ui.shadow),
            shadow_alpha: ui.card_shadow_alpha,
        }
    }
}

/// A raised card (M11i; each pane, the right sidebar): the card's fill, `CARD_RADIUS`
/// corners, a `FOCUS_RING` edge (the accent while `focused_ring`, else `card_border`) drawn
/// over its content, and its shadow. Its content is clipped to the inside of its edge (a
/// rectangle: content that reaches a corner rounds its own background, see note 60 §5).
pub fn card(colors: &PaneColors, focused_ring: bool) -> Div {
    div()
        .bg(colors.card)
        .rounded(px(CARD_RADIUS))
        .border(px(FOCUS_RING))
        .border_color(if focused_ring {
            colors.focus
        } else {
            colors.card_border
        })
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

/// Where the 2 pt accent line starts in a split's grab strip (`gap` wide, centred on the
/// split's `ratio` of its width): the middle of the gap between the two cards. The sides give
/// up the gap in proportion to their shares, so the gap's middle sits `gap * (0.5 - ratio)`
/// past the strip's middle.
pub fn accent_offset(ratio: f32, gap: f32) -> f32 {
    gap / 2.0 + gap * (0.5 - ratio) - DIVIDER_ACTIVE / 2.0
}

/// `tree` filling its parent. In a split, the focused pane's card has the focus ring.
pub fn render_layout(tree: &Node<PaneId>, panes: &Panes) -> AnyElement {
    let ring = matches!(tree, Node::Split { .. });
    render_node(tree, Vec::new(), ring, panes)
}

/// `node`, reached from the root by `path`; `ring`: the focused pane's card is ringed.
fn render_node(node: &Node<PaneId>, path: Vec<bool>, ring: bool, panes: &Panes) -> AnyElement {
    match node {
        Node::Leaf(id) => {
            let id = *id;
            let select = panes.select.clone();
            card(&panes.colors, ring && id == panes.active)
                .flex()
                .flex_col()
                .size_full()
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
                        // underneath. Rounded at the bottom like the card.
                        .when(id != panes.active, |pane| {
                            pane.child(
                                div()
                                    .absolute()
                                    .top_0()
                                    .left_0()
                                    .size_full()
                                    .rounded_b(px(CARD_RADIUS))
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
            // Each side takes its share; they shrink (in proportion) to make room for the gap.
            // Not clipped, so the cards' shadows show in the gaps; a side may still shrink
            // below its content (`min_*` 0).
            let side = |node: &Node<PaneId>, share: f32, to_second: bool| {
                div()
                    .flex_basis(relative(share))
                    .flex_grow_0()
                    .flex_shrink_1()
                    .min_w(px(0.0))
                    .min_h(px(0.0))
                    .map(|d| if row { d.h_full() } else { d.w_full() })
                    .child(render_node(node, child_path(to_second), ring, panes))
            };
            // The gap between the two cards: the canvas shows through.
            let line = div().flex_none().map(|d| {
                if row {
                    d.w(px(CARD_GAP)).h_full()
                } else {
                    d.h(px(CARD_GAP)).w_full()
                }
            });
            // Last child, so it sits above both panes' terminals. It blocks their clicks under it
            // (so pressing it doesn't select text or switch panes) but lets scrolling through.
            let start_drag = panes.start_drag.clone();
            let here = path.clone();
            let hover = panes.hover.clone();
            let hover_path = path.clone();
            let active = divider_active(&path, panes.hovered, panes.dragging);
            // The accent line is drawn in the grab strip (the last child, so over both panes),
            // in the middle of the gap (`accent_offset`).
            let accent = div().absolute().bg(panes.colors.accent).map(|d| {
                let from = accent_offset(*ratio, CARD_GAP);
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
                            .ml(px(-GRAB / 2.0))
                            .cursor_col_resize()
                    } else {
                        d.left_0()
                            .w_full()
                            .h(px(GRAB))
                            .top(relative(*ratio))
                            .mt(px(-GRAB / 2.0))
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

    /// The gap between the two cards in a split's grab strip coordinates: the strip is `gap`
    /// wide and centred on the split's `ratio`; the first side gives up `ratio * gap` of its
    /// share to the gap.
    fn gap_in_strip(ratio: f32, gap: f32) -> (f32, f32) {
        let start = gap / 2.0 - ratio * gap;
        (start, start + gap)
    }

    #[test]
    fn the_accent_line_is_centred_in_the_gap() {
        // An even split: the strip is the gap, and the 2 pt line sits 2 pt in.
        assert_eq!(accent_offset(0.5, 6.0), 2.0);
        assert_eq!(gap_in_strip(0.5, 6.0), (0.0, 6.0));
        for ratio in [0.1, 0.25, 0.5, 0.75, 0.9] {
            let (from, to) = gap_in_strip(ratio, 6.0);
            let line = accent_offset(ratio, 6.0);
            assert!(
                line >= from && line + DIVIDER_ACTIVE <= to,
                "{ratio}: {line}"
            );
            // Centred: as much gap on either side of it.
            let (before, after) = (line - from, to - line - DIVIDER_ACTIVE);
            assert!(
                (before - after).abs() < 1e-4,
                "{ratio}: {before} vs {after}"
            );
        }
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
