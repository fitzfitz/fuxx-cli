//! One look for usage limits app-wide (M11ap, the lead designer's spec): the agent box's usage
//! panel and the status bar's usage popover draw their rows with these. Every figure is the
//! percent **used**, its bar and number coloured by band as the agent tab's context meter is
//! (under 70 normal, 70–89 caution, 90 and up critical).

use alacritty_terminal::vte::ansi::Rgb;
use gpui::{AnyElement, Div, SharedString, div, prelude::*, px, relative};

use crate::terminal_view::hsla;
use crate::theme::{Theme, UiColors, mix, readable};
use crate::ui_tokens::{MARK_MIN, QUIET_LINE_MIN, TEXT_MIN, text};

/// The colours of a usage bar and its number, by band (from the theme, held to their floors on
/// the panel and the bar's track).
#[derive(Clone, Copy, Debug)]
pub struct UsageInk {
    pub track: Rgb,
    pub bar_ok: Rgb,
    pub bar_caution: Rgb,
    pub bar_critical: Rgb,
    pub pct_ok: Rgb,
    pub pct_caution: Rgb,
    pub pct_critical: Rgb,
}

impl UsageInk {
    pub fn of(theme: &Theme) -> UsageInk {
        let ui = theme.ui();
        let panel = ui.panel;
        let track = readable(mix(panel, theme.foreground, 0.12), &[panel], QUIET_LINE_MIN);
        let bar = |c: Rgb| readable(c, &[panel, track], MARK_MIN);
        let pct = |c: Rgb| readable(c, &[panel], TEXT_MIN);
        UsageInk {
            track,
            bar_ok: bar(ui.accent),
            bar_caution: bar(theme.palette[3]),
            bar_critical: bar(theme.palette[1]),
            pct_ok: pct(ui.text_dim),
            pct_caution: pct(theme.palette[3]),
            pct_critical: pct(theme.palette[1]),
        }
    }

    /// The bar's and the number's colours for `used` percent (the context meter's bands).
    pub fn band(&self, used: u8) -> (Rgb, Rgb) {
        match crate::agent_top::band(used) {
            crate::agent_top::Band::Ok => (self.bar_ok, self.pct_ok),
            crate::agent_top::Band::Caution => (self.bar_caution, self.pct_caution),
            crate::agent_top::Band::Critical => (self.bar_critical, self.pct_critical),
        }
    }
}

/// `77% used`.
pub fn pct_text(used: u8) -> String {
    format!("{used}% used")
}

/// One limit: its name and `N% used` (in its band's colour), a full-width bar filled to the used
/// share (in its band's colour, on the track), `Resets …` under it, and the agent's note (its
/// waiting colour). The caller marks it (the scenes find it by name).
pub fn limit_row(
    name: String,
    used_pct: Option<u8>,
    resets: Option<String>,
    note: Option<String>,
    ui: &UiColors,
    ink: &UsageInk,
) -> Div {
    let small = |t: String, color| {
        div()
            .text_size(px(text::META))
            .text_color(hsla(color))
            .child(SharedString::from(t))
    };
    let dim = ui.text_dim;
    let top = div()
        .flex()
        .items_center()
        .gap(px(8.))
        .text_size(px(13.))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(SharedString::from(name)),
        )
        .children(used_pct.map(|used| {
            div()
                .flex_none()
                .font_features(crate::sidebar::tabular())
                .text_color(hsla(ink.band(used).1))
                .child(SharedString::from(pct_text(used)))
        }));
    let bar = used_pct.map(|used| {
        div()
            .w_full()
            .h(px(4.))
            .my(px(4.))
            .rounded(px(2.))
            .bg(hsla(ink.track))
            .child(
                div()
                    .h_full()
                    .w(relative(f32::from(used.min(100)) / 100.))
                    .rounded(px(2.))
                    .bg(hsla(ink.band(used).0)),
            )
    });
    div()
        .px(px(8.))
        .py(px(6.))
        .flex()
        .flex_col()
        .child(top)
        .children(bar)
        .children(resets.map(|t| small(t, dim)))
        .children(note.map(|t| small(t, ui.ring_waiting)))
}

/// A section heading: 24 tall, quiet (the designer's details).
pub fn heading(text: String, ui: &UiColors) -> AnyElement {
    div()
        .h(px(24.))
        .px(px(8.))
        .flex()
        .items_center()
        .text_size(px(text::META))
        .text_color(hsla(ui.text_dim))
        .overflow_hidden()
        .whitespace_nowrap()
        .text_ellipsis_start()
        .child(SharedString::from(text))
        .into_any_element()
}

/// An agent's usage rows (`agent_box::usage_rows`) from what fuxx knows: its live usage (the
/// last good reading, freshened by what the agent said since), else what the reading is doing;
/// kept numbers whose refresh failed say so, and how old they are. `available`: an agent with no
/// reading of its own (named in its note). The agent box's panel and the status bar's detail
/// both show these.
pub fn agent_rows(
    agent: &str,
    available: Option<&str>,
    catalogs: Option<&crate::agent_catalog::Catalogs>,
    now_unix: i64,
    utc_offset: i64,
) -> Vec<crate::agent_box::UsageRow> {
    use crate::agent_catalog::Lookup;
    let live = catalogs
        .and_then(|c| c.live_usage(agent))
        .map(Lookup::Ready);
    let found = live
        .as_ref()
        .or_else(|| catalogs.and_then(|c| c.usage_last(agent)));
    let limits = catalogs.map_or(&[][..], |c| c.limits(agent));
    let mut rows = crate::agent_box::usage_rows(found, limits, available, now_unix, utc_offset);
    let failed = matches!(
        catalogs.and_then(|c| c.usage_last(agent)),
        Some(Lookup::Failed(_))
    );
    let ago = catalogs.and_then(|c| c.usage_ready_at(agent)).map(|at| {
        std::time::Instant::now()
            .saturating_duration_since(at)
            .as_secs()
    });
    if live.is_some()
        && let Some(note) = crate::agent_catalog::stale_note(failed, ago)
    {
        rows.push(crate::agent_box::UsageRow::Quiet(note));
    }
    rows
}

/// `rows` drawn: headings, quiet lines, the agent's own words, and each limit (`limit_row`,
/// passed to `mark` with its name: the agent box marks them for its scenes).
pub fn limit_items(
    rows: Vec<crate::agent_box::UsageRow>,
    ui: &UiColors,
    ink: &UsageInk,
    mark: impl Fn(String, Div) -> AnyElement,
) -> Vec<AnyElement> {
    use crate::agent_box::UsageRow;
    rows.into_iter()
        .map(|row| match row {
            UsageRow::Heading(name) => heading(name, ui),
            UsageRow::Quiet(text) => quiet(text, ui),
            UsageRow::Text(text) => div()
                .px(px(8.))
                .py(px(4.))
                .text_color(hsla(ui.text_dim))
                .child(SharedString::from(text))
                .into_any_element(),
            UsageRow::Limit {
                name,
                used_pct,
                resets,
                note,
            } => mark(
                name.clone(),
                limit_row(name, used_pct, resets, note, ui, ink),
            ),
        })
        .collect()
}

/// The usage panel, one look app-wide (your words, 2026-11-22: "why can't we just reuse it"):
/// the overlay shell, `header` (the status bar's agent name) over `items`, which scroll past
/// `USAGE_MAX_H`.
pub fn limits_panel(
    id: &'static str,
    header: Option<AnyElement>,
    items: Vec<AnyElement>,
    ui: &UiColors,
) -> gpui::Div {
    crate::overlay_panel::shell(ui, crate::agent_box::USAGE_WIDTH)
        .p(px(4.))
        .gap(px(0.))
        .children(header)
        .child(
            div()
                .id(id)
                .flex()
                .flex_col()
                .gap(px(4.))
                .max_h(px(crate::agent_box::USAGE_MAX_H))
                .overflow_y_scroll()
                .children(items),
        )
}

/// A quiet line in a list (no row to pick).
pub fn quiet(text: impl Into<SharedString>, ui: &UiColors) -> AnyElement {
    div()
        .h(px(32.))
        .px(px(26.))
        .flex()
        .items_center()
        .text_color(hsla(ui.text_dim))
        .child(text.into())
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui_tokens::contrast;

    #[test]
    fn a_limit_says_how_much_is_used() {
        assert_eq!(pct_text(77), "77% used");
        assert_eq!(pct_text(0), "0% used");
    }

    #[test]
    fn bars_and_numbers_meet_their_floors_in_every_theme() {
        for (name, theme) in crate::theme::every_theme() {
            let ui = theme.ui();
            let ink = UsageInk::of(&theme);
            let got = contrast(ink.track, ui.panel);
            assert!(got >= QUIET_LINE_MIN - 0.01, "{name}: track {got:.2}");
            for used in [10, 75, 95] {
                let (bar, pct) = ink.band(used);
                for (what, fg, on, min) in [
                    ("bar on panel", bar, ui.panel, MARK_MIN),
                    ("bar on track", bar, ink.track, MARK_MIN),
                    ("percent on panel", pct, ui.panel, TEXT_MIN),
                ] {
                    let got = contrast(fg, on);
                    assert!(got >= min - 0.01, "{name}: {used}% {what} {got:.2}");
                }
            }
        }
    }
}
