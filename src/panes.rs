//! Drawing a split layout: nested flex rows/columns of terminals with a 1 px line between panes,
//! the unfocused ones dimmed, each line with a grab strip for resizing. Stateless; the tree, the
//! sessions and the drag live in the workspace.

use std::rc::Rc;

use gpui::{
    AnyElement, App, Entity, EntityId, Hsla, MouseButton, MouseDownEvent, Window, div, prelude::*,
    px, relative,
};

use crate::split_tree::{Axis, Node};
use crate::terminal_view::TerminalView;

/// Called with a pane's session (by its view) when the pane is clicked.
pub type SelectPane = Rc<dyn Fn(EntityId, &mut Window, &mut App)>;

/// Called with a split's path (from the root; `true` = second child) when its line is pressed.
pub type StartDrag = Rc<dyn Fn(Vec<bool>, &mut Window, &mut App)>;

/// Width of the invisible grab strip centred on each line (it covers 3 px of either pane).
const GRAB: f32 = 6.0;

/// The colors a layout is drawn with.
#[derive(Clone, Copy)]
pub struct PaneColors {
    /// Laid over an unfocused pane (the theme background, partly transparent).
    pub dim: Hsla,
    pub divider: Hsla,
}

/// What every pane of one layout is drawn with.
pub struct Panes<'a> {
    /// The focused pane.
    pub active: EntityId,
    /// Finds each pane's terminal.
    pub view: &'a dyn Fn(EntityId) -> Option<Entity<TerminalView>>,
    pub select: SelectPane,
    pub start_drag: StartDrag,
    pub colors: PaneColors,
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
                .relative()
                .size_full()
                // Capture phase: runs before the terminal's own mouse-down (which still runs).
                .capture_any_mouse_down(move |_: &MouseDownEvent, window, cx| {
                    select(id, window, cx)
                })
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
                })
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
            let grab = div()
                .absolute()
                .block_mouse_except_scroll()
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
