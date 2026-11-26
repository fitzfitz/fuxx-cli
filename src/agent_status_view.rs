//! The chat's status row over the box (the 2026-11-24 chores: "change the Cultivating loading on
//! the chat to be the status states"; chore 22: one row): its mark in the avatar column, the
//! line in the text column (cut with "…" when room runs short), and pinned to the row's right
//! the ledger's chips, `⧉ 4 changed` and `✓ checks`, each opening its part of the ledger's
//! popover. Always in view (not a row of the chat, so a scrolled chat still shows it). The
//! words are `agent_status_line`'s.

use gpui::{
    AnyElement, AppContext, ClickEvent, Context, InteractiveElement, IntoElement, ParentElement,
    SharedString, StatefulInteractiveElement, Styled, div, prelude::FluentBuilder, px,
};

use super::{AgentTab, BLACK, Colors, MARK};
use crate::agent_box::BoxMenu;
use crate::agent_box::menus::MenuPlace;
use crate::agent_status_line::{Chip, ChipKind, ChipState};
use crate::appearance::Appearance;
use crate::mark_view::MarkView;
use crate::status::Status;
use crate::terminal_view::hsla;
use crate::ui_tokens::text;

impl AgentTab {
    /// The status row, while the status line shows (`status_line_shown`).
    pub(super) fn status_strip(
        &mut self,
        c: &Colors,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        use crate::ui_tokens::chat;
        let theme = cx.global::<Appearance>().theme;
        let (main, chips) = self.status_line_now(&theme)?;
        let mark: AnyElement = match self.shown_status() {
            Status::Waiting => div()
                .size(px(6.))
                .rounded_full()
                .bg(hsla(c.ask_edge))
                .into_any_element(),
            Status::Done => crate::status_mark::done_check(c.dim, px(12.)).into_any_element(),
            Status::Working | Status::Idle => self.thinking_mark.clone().into_any_element(),
        };
        let chips = (!chips.is_empty()).then(|| self.status_chips(&chips, c, cx));
        let line = div()
            .w_full()
            .max_w(px(chat::COLUMN_MAX))
            .flex()
            .items_center()
            .gap(px(chat::AVATAR_GAP))
            .pt(px(6.))
            .pb(px(4.))
            .text_size(px(text::META))
            .child(
                div()
                    .flex_none()
                    .w(px(chat::AVATAR))
                    .h(px(18.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(mark),
            )
            .child(
                self.marked(
                    "status-line",
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .h(px(18.))
                        .flex()
                        .items_center()
                        .text_color(hsla(c.dim))
                        .child(div().truncate().child(SharedString::from(main))),
                ),
            )
            .children(chips);
        Some(
            div()
                .flex_none()
                .w_full()
                .px(px(crate::agent_box::AREA_X))
                .flex()
                .justify_center()
                .child(line)
                .into_any_element(),
        )
    }

    /// The chips on the row's right: quiet (dim, as the time), the checks' red while one fails,
    /// its arc while one runs; the ledger's popover hangs from them while open.
    fn status_chips(&mut self, chips: &[Chip], c: &Colors, cx: &mut Context<Self>) -> AnyElement {
        let running = chips.iter().any(|ch| ch.state == ChipState::Running);
        if running && self.checks_mark.is_none() {
            let clock = self.clock.clone();
            self.checks_mark =
                Some(cx.new(|cx| MarkView::new(&clock, Status::Working, px(MARK), BLACK, cx)));
        } else if !running {
            self.checks_mark = None;
        }
        let els: Vec<AnyElement> = chips
            .iter()
            .map(|chip| self.status_chip(chip, c, cx))
            .collect();
        let open = matches!(self.menu, Some(BoxMenu::Ledger));
        let popover = open.then(|| {
            let menu = self.ledger_menu(cx);
            self.menu_frame_at(
                Some(menu),
                None,
                crate::agent_box::LEDGER_MENU_WIDTH,
                MenuPlace::Right(0.),
                cx,
            )
        });
        self.marked(
            "status-chips",
            div()
                .relative()
                .flex_none()
                .flex()
                .items_center()
                .gap(px(4.))
                .children(els)
                .children(popover),
        )
        .into_any_element()
    }

    /// One chip: its glyph and words; a click opens its part of the ledger's popover, or shuts
    /// it (not the click whose press just shut it).
    fn status_chip(&self, chip: &Chip, c: &Colors, cx: &mut Context<Self>) -> AnyElement {
        use crate::icons::{Icon, icon};
        let failed = chip.state == ChipState::Failed;
        let ink = if failed { c.ledger_failing } else { c.dim };
        let glyph: AnyElement = match chip.state {
            ChipState::Quiet => icon(Icon::Copy, px(12.), hsla(ink)).into_any_element(),
            ChipState::Passed => icon(Icon::Check, px(12.), hsla(ink)).into_any_element(),
            ChipState::Ran => icon(Icon::Circle, px(10.), hsla(ink)).into_any_element(),
            ChipState::Failed => icon(Icon::MarkCross, px(12.), hsla(ink)).into_any_element(),
            ChipState::Running => match &self.checks_mark {
                Some(mark) => mark.clone().into_any_element(),
                None => icon(Icon::Circle, px(10.), hsla(ink)).into_any_element(),
            },
        };
        let (id, part) = match chip.kind {
            ChipKind::Changed => ("agent-status-changed", "status-changed"),
            ChipKind::Checks => ("agent-status-checks", "status-checks"),
        };
        let kind = chip.kind;
        let lit = matches!(self.menu, Some(BoxMenu::Ledger)) && self.ledger_part == Some(kind);
        let (hover_bg, hover_ink) = (c.pill_hover, c.text);
        self.marked(
            part,
            div()
                .id(id)
                .flex()
                .items_center()
                .gap(px(4.))
                .h(px(20.))
                .px(px(6.))
                .rounded(px(6.))
                .font_features(crate::sidebar::tabular())
                .text_color(hsla(ink))
                .cursor_pointer()
                .hover(move |s| s.bg(hsla(hover_bg)).text_color(hsla(hover_ink)))
                .when(lit, |d| d.bg(hsla(hover_bg)))
                .child(glyph)
                .child(SharedString::from(chip.text.clone()))
                .on_click(cx.listener(move |this, event: &ClickEvent, _, cx| {
                    let pressed = match event {
                        ClickEvent::Mouse(click) => Some(click.down.position),
                        _ => None,
                    };
                    // This press already shut the open popover (from outside it): that
                    // chip's own click leaves it shut; the other chip's opens its part.
                    let shut_at = this.chip_closed_at.take();
                    let shut_own = this.chip_closed_menu.take() == Some(BoxMenu::Ledger)
                        && this.ledger_part == Some(kind);
                    if pressed.is_none() || shut_at != pressed || !shut_own {
                        this.ledger_clicked(kind, cx)
                    }
                })),
        )
        .into_any_element()
    }

    /// A chip's click: the ledger's popover opens with its part (the files, or the checks), or
    /// shuts when it shows that part already.
    pub(super) fn ledger_clicked(&mut self, kind: ChipKind, cx: &mut Context<Self>) {
        if matches!(self.menu, Some(BoxMenu::Ledger)) && self.ledger_part == Some(kind) {
            self.close_menu(cx);
        } else {
            self.ledger_part = Some(kind);
            self.open_menu(BoxMenu::Ledger, cx);
        }
    }
}
