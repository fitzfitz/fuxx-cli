//! The look shared by the panels that open at the top of the window (the command palette, the
//! host picker, the address box and the Help panel): one surface, edge, shadow and radius, and one filter box
//! with its placeholder. The box's own focus edge is the `NameInput`'s.

use gpui::{
    AnyElement, BoxShadow, Div, Entity, Hsla, IntoElement, ParentElement, Styled, div, point,
    prelude::*, px,
};

use crate::name_input::NameInput;
use crate::terminal_view::hsla;
use crate::theme::UiColors;
use crate::ui_tokens::text;

/// The panel itself: `width` wide (less in a narrow window), raised above the window. Children
/// are the filter box (`filter_box`), then the rest.
pub fn shell(ui: &UiColors, width: f32) -> Div {
    div()
        .flex()
        .flex_col()
        .w(px(width))
        .max_w_full()
        .p(px(6.0))
        .gap(px(6.0))
        .rounded(px(10.0))
        .bg(hsla(ui.panel))
        .border_1()
        .border_color(hsla(ui.panel_border))
        .shadow(vec![BoxShadow {
            color: Hsla {
                a: 0.3,
                ..hsla(ui.shadow)
            },
            offset: point(px(0.0), px(8.0)),
            blur_radius: px(24.0),
            spread_radius: px(0.0),
            inset: false,
        }])
        .text_size(px(text::NAME))
        .text_color(hsla(ui.text))
}

/// The filter box, a little in from the panel's edge, with `placeholder` laid over it while it
/// is `empty` (the caret stays at its start, as in any macOS field).
pub fn filter_box(
    ui: &UiColors,
    input: &Entity<NameInput>,
    empty: bool,
    placeholder: &'static str,
) -> AnyElement {
    div()
        .relative()
        .flex()
        .m(px(2.0))
        .text_size(px(text::NAME))
        .child(input.clone())
        .when(empty, |field| {
            field.child(
                div()
                    .absolute()
                    // Inside the box's 1 px border and 4 px padding, and clear of the caret at
                    // the start.
                    .top(px(1.0))
                    .left(px(7.0))
                    .text_color(hsla(ui.text_dim))
                    .child(placeholder),
            )
        })
        .into_any_element()
}
