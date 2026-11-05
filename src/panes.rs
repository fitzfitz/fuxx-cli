//! Drawing a split layout: nested flex rows/columns of terminals (each under its header, when it
//! has one) with a 1 px line between panes, the unfocused ones dimmed, each line with a grab
//! strip for resizing that lights it up as a 2 px accent line while hovered or dragged.
//! Stateless; the tree, the sessions, the hover and the drag live in the workspace.

use std::rc::Rc;

use gpui::{
    AnyElement, App, EntityId, Hsla, MouseButton, MouseDownEvent, Window, div, prelude::*, px,
    relative,
};

use crate::pane_header;
use crate::split_tree::{Axis, Node};
use crate::ui_tokens::size::DIVIDER_ACTIVE;

/// Called with a pane's session (by its view) when the pane is clicked.
pub type SelectPane = Rc<dyn Fn(EntityId, &mut Window, &mut App)>;

/// Called with a split's path (from the root; `true` = second child) when its line is pressed.
pub type StartDrag = Rc<dyn Fn(Vec<bool>, &mut Window, &mut App)>;

/// Called with a split's path when the pointer enters (`true`) or leaves its line's grab strip.
pub type HoverDivider = Rc<dyn Fn(Vec<bool>, bool, &mut Window, &mut App)>;

/// Width of the invisible grab strip centred on each line (it covers 3 px of either pane).
const GRAB: f32 = 6.0;

/// The colors a layout is drawn with.
#[derive(Clone, Copy)]
pub struct PaneColors {
    /// Laid over an unfocused pane (the theme background, partly transparent).
    pub dim: Hsla,
    pub divider: Hsla,
    /// A line under the pointer or being dragged.
    pub accent: Hsla,
}

/// What every pane of one layout is drawn with.
pub struct Panes<'a> {
    /// The focused pane.
    pub active: EntityId,
    /// Each pane's terminal, as the element to draw (the workspace decides how it's embedded).
    pub view: &'a dyn Fn(EntityId) -> Option<AnyElement>,
    /// Each pane's header, above its terminal (none for a layout that isn't split).
    pub header: &'a dyn Fn(EntityId) -> Option<AnyElement>,
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

/// `tree` filling its parent.
pub fn render_layout(tree: &Node<EntityId>, panes: &Panes) -> AnyElement {
    render_node(tree, Vec::new(), panes)
}

/// `node`, reached from the root by `path`.
fn render_node(node: &Node<EntityId>, path: Vec<bool>, panes: &Panes) -> AnyElement {
    match node {
        Node::Leaf(id) => {
            let id = *id;
            let select = panes.select.clone();
            div()
                .flex()
                .flex_col()
                .size_full()
                // Capture phase: runs before the terminal's own mouse-down (which still runs).
                // It covers the header too, so clicking that focuses the pane.
                .capture_any_mouse_down(move |_: &MouseDownEvent, window, cx| {
                    select(id, window, cx)
                })
                .children((panes.header)(id))
                .child(
                    // The terminal's area: what's left under the header.
                    div()
                        .relative()
                        .flex_1()
                        .min_h(px(0.0))
                        .w_full()
                        .children((panes.view)(id))
                        // No listeners, so no hitbox: clicks go through to the terminal underneath.
                        .when(id != panes.active, |pane| {
                            pane.child(
                                div()
                                    .absolute()
                                    .top_0()
                                    .left_0()
                                    .size_full()
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
            // Each side takes its share; they may shrink (in proportion) to make room for the line.
            let side = |node: &Node<EntityId>, share: f32, to_second: bool| {
                div()
                    .flex_basis(relative(share))
                    .flex_grow_0()
                    .flex_shrink_1()
                    .overflow_hidden()
                    .map(|d| if row { d.h_full() } else { d.w_full() })
                    .child(render_node(node, child_path(to_second), panes))
            };
            let line = div().flex_none().bg(panes.colors.divider).map(|d| {
                if row {
                    d.w(px(1.0)).h_full()
                } else {
                    d.h(px(1.0)).w_full()
                }
            });
            // Last child, so it sits above both panes' terminals. It blocks their clicks under it
            // (so pressing it doesn't select text or switch panes) but lets scrolling through.
            let start_drag = panes.start_drag.clone();
            let here = path.clone();
            let hover = panes.hover.clone();
            let hover_path = path.clone();
            let active = pane_header::divider_active(&path, panes.hovered, panes.dragging);
            // The accent line is drawn in the grab strip (the last child, so over both panes),
            // centred on the 1 px line: the line sits `ratio` px short of the split's own
            // proportion, since the sides shrink by it to make room.
            let shift = 0.5 - *ratio;
            let accent = div().absolute().bg(panes.colors.accent).map(|d| {
                let from = GRAB / 2.0 + shift - DIVIDER_ACTIVE / 2.0;
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
                .overflow_hidden()
                .child(side(first, *ratio, false))
                .child(line)
                .child(side(second, 1.0 - *ratio, true))
                .child(grab)
                .into_any_element()
        }
    }
}
