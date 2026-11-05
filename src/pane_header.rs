//! Pane headers (M11): when a session is split, each pane gets a 22 px strip above its terminal
//! with its status mark, number, name and how long it has been in that status. `shows` and
//! `look` are the pure model (when, and with which colours); `Workspace::pane_header` draws it,
//! and `divider_active` decides which divider is drawn as the 2 px accent line.

use std::time::Instant;

use alacritty_terminal::vte::ansi::Rgb;
use gpui::{AnyElement, EntityId, SharedString, div, prelude::*, px};

use crate::sidebar::{mark_slot, tabular};
use crate::status::{self, Status};
use crate::terminal_view::hsla;
use crate::theme::UiColors;
use crate::ui_tokens::size::PANE_HEADER;
use crate::ui_tokens::text;
use crate::workspace::Workspace;

/// Whether a layout of `panes` panes gives each one a header: only when there is a split.
pub fn shows(panes: usize) -> bool {
    panes > 1
}

/// How a header is drawn.
#[derive(Debug, PartialEq)]
pub struct Look {
    /// Name colour: full for the focused pane, dimmer for the others.
    pub text: Rgb,
    /// The strip's fill: the sidebar's, or the waiting tint for a pane waiting on you.
    pub background: Rgb,
    /// The accent line along the bottom edge: the focused pane only.
    pub underline: bool,
}

pub fn look(ui: &UiColors, status: Status, focused: bool) -> Look {
    Look {
        text: if focused { ui.text } else { ui.text_dim },
        background: if status == Status::Waiting {
            ui.waiting_tint
        } else {
            ui.sidebar_background
        },
        underline: focused,
    }
}

/// Whether the divider of the split at `path` is drawn as the accent line: the pointer is over
/// it, or it is the one being dragged.
pub fn divider_active(path: &[bool], hovered: Option<&[bool]>, dragging: Option<&[bool]>) -> bool {
    hovered == Some(path) || dragging == Some(path)
}

/// Gap between the header's parts, and its side padding.
const GAP: f32 = 6.0;
const PAD: f32 = 8.0;
/// The accent line's thickness (the focus line, as thick as an active divider).
const UNDERLINE: f32 = 2.0;

impl Workspace {
    /// The sessions of the layout on screen that get a header, in layout order (none unless
    /// it's split).
    pub(crate) fn header_sessions(&self) -> Vec<usize> {
        let Some(tree) = self.shown_layout() else {
            return Vec::new();
        };
        let leaves = tree.leaves();
        if !shows(leaves.len()) {
            return Vec::new();
        }
        leaves
            .into_iter()
            .filter_map(|id| self.sessions.position(|s| s.view.entity_id() == id))
            .collect()
    }

    /// The header of the pane showing `view`; `None` for a pane with none.
    pub(crate) fn pane_header(&self, view: EntityId, ui: UiColors) -> Option<AnyElement> {
        let index = self.sessions.position(|s| s.view.entity_id() == view)?;
        let session = &self.sessions.items()[index];
        let focused = self.sessions.active_index() == Some(index);
        let look = look(&ui, session.status, focused);
        let age: Option<SharedString> = (session.status != Status::Idle).then(|| {
            status::age_label(Instant::now().saturating_duration_since(session.status_since)).into()
        });
        let mark = self.session_mark("header", session, ui, look.background);
        Some(
            div()
                .relative()
                .flex()
                .flex_row()
                .items_center()
                .flex_none()
                .w_full()
                .h(px(PANE_HEADER))
                .px(px(PAD))
                .gap(px(GAP))
                .bg(hsla(look.background))
                .child(mark_slot(mark))
                .child(
                    div()
                        .flex_none()
                        .text_size(px(text::META))
                        .font_features(tabular())
                        .text_color(hsla(ui.text_faint))
                        .child(format!("#{}", index + 1)),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_size(px(text::NAME))
                        .text_color(hsla(look.text))
                        .child(session.label()),
                )
                .children(age.map(|age| {
                    div()
                        .flex_none()
                        .text_size(px(text::META))
                        .font_features(tabular())
                        .text_color(hsla(ui.text_faint))
                        .child(age)
                }))
                .when(look.underline, |header| {
                    header.child(
                        div()
                            .absolute()
                            .left_0()
                            .bottom_0()
                            .w_full()
                            .h(px(UNDERLINE))
                            .bg(hsla(ui.accent)),
                    )
                })
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Theme;

    fn ui() -> UiColors {
        Theme::default().ui()
    }

    #[test]
    fn header_shows_only_when_split() {
        assert!(!shows(0));
        assert!(!shows(1));
        assert!(shows(2));
        assert!(shows(3));
    }

    #[test]
    fn the_focused_pane_has_the_accent_and_full_text() {
        let ui = ui();
        let focused = look(&ui, Status::Idle, true);
        let other = look(&ui, Status::Idle, false);
        assert!(focused.underline && !other.underline);
        assert_eq!(focused.text, ui.text);
        assert_eq!(other.text, ui.text_dim);
        assert_eq!(other.background, ui.sidebar_background);
    }

    #[test]
    fn a_waiting_pane_is_tinted_focused_or_not() {
        let ui = ui();
        assert_eq!(look(&ui, Status::Waiting, true).background, ui.waiting_tint);
        assert_eq!(
            look(&ui, Status::Waiting, false).background,
            ui.waiting_tint
        );
        assert_eq!(
            look(&ui, Status::Working, false).background,
            ui.sidebar_background
        );
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
