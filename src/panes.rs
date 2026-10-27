//! Drawing a split layout: nested flex rows/columns of terminals with a 1 px line between panes,
//! the unfocused ones dimmed. Stateless; the tree and the sessions live in the workspace.

use std::rc::Rc;

use gpui::{
    AnyElement, App, Entity, EntityId, Hsla, MouseDownEvent, Window, div, prelude::*, px, relative,
};

use crate::split_tree::{Axis, Node};
use crate::terminal_view::TerminalView;

/// Called with a pane's session (by its view) when the pane is clicked.
pub type SelectPane = Rc<dyn Fn(EntityId, &mut Window, &mut App)>;

/// The colors a layout is drawn with.
#[derive(Clone, Copy)]
pub struct PaneColors {
    /// Laid over an unfocused pane (the theme background, partly transparent).
    pub dim: Hsla,
    pub divider: Hsla,
}

/// `tree` filling its parent; `active` is the focused pane, `view` finds each pane's terminal.
pub fn render_layout(
    tree: &Node<EntityId>,
    active: EntityId,
    view: &dyn Fn(EntityId) -> Option<Entity<TerminalView>>,
    select: &SelectPane,
    colors: PaneColors,
) -> AnyElement {
    match tree {
        Node::Leaf(id) => {
            let id = *id;
            let select = select.clone();
            div()
                .relative()
                .size_full()
                // Capture phase: runs before the terminal's own mouse-down (which still runs).
                .capture_any_mouse_down(move |_: &MouseDownEvent, window, cx| {
                    select(id, window, cx)
                })
                .children(view(id))
                // No listeners, so no hitbox: clicks go through to the terminal underneath.
                .when(id != active, |pane| {
                    pane.child(div().absolute().top_0().left_0().size_full().bg(colors.dim))
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
            // Each side takes its share; they may shrink (in proportion) to make room for the line.
            let side = |node: &Node<EntityId>, share: f32| {
                div()
                    .flex_basis(relative(share))
                    .flex_grow_0()
                    .flex_shrink_1()
                    .overflow_hidden()
                    .map(|d| if row { d.h_full() } else { d.w_full() })
                    .child(render_layout(node, active, view, select, colors))
            };
            let line = div().flex_none().bg(colors.divider).map(|d| {
                if row {
                    d.w(px(1.0)).h_full()
                } else {
                    d.h(px(1.0)).w_full()
                }
            });
            div()
                .flex()
                .map(|d| if row { d.flex_row() } else { d.flex_col() })
                .size_full()
                .overflow_hidden()
                .child(side(first, *ratio))
                .child(line)
                .child(side(second, 1.0 - *ratio))
                .into_any_element()
        }
    }
}
