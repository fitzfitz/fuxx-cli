//! The agent tab's top bar (chores after M11ab, the lead designer's spec A §C): the status pill,
//! the conversation's cost and its live context meter. Pure: the words, colours and sizes the
//! bar draws with; `agent_tab::top_line` draws them.

use alacritty_terminal::vte::ansi::Rgb;

use crate::status::Status;
use crate::theme::{Theme, mix};
use crate::ui_tokens::{TEXT_MIN, ensure_contrast};

/// Cubic ease-out: fast, then settling (the bar's blends and the meter's glide).
pub fn ease_out_cubic(t: f32) -> f32 {
    1.0 - (1.0 - t.clamp(0.0, 1.0)).powi(3)
}

/// The meter's size (spec A §C): 56 × 4, radius 2.
pub const METER_W: f32 = 56.0;
pub const METER_H: f32 = 4.0;

/// How strongly a pill takes its state's colour over the page: lighter on a light page, so the
/// word keeps its hue there.
fn pill_tint(theme: &Theme) -> f32 {
    if theme.is_dark() { 0.16 } else { 0.12 }
}

/// How full the context is, as the meter colours it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Band {
    /// Under 70 %: the accent.
    Ok,
    /// 70–89 %.
    Caution,
    /// 90 % and up.
    Critical,
}

pub fn band(pct: u8) -> Band {
    match pct {
        0..70 => Band::Ok,
        70..90 => Band::Caution,
        _ => Band::Critical,
    }
}

/// The meter's filled width for `pct` of `width`: at least 2 px once any is used.
pub fn meter_fill(pct: u8, width: f32) -> f32 {
    if pct == 0 {
        return 0.0;
    }
    (width * f32::from(pct.min(100)) / 100.0).max(2.0)
}

/// What the last finished turn added to the cost (`+$0.42`): only a rise from a known cost.
pub fn cost_delta(before: Option<f64>, now: Option<f64>) -> Option<String> {
    let (before, now) = (before?, now?);
    let cents = ((now - before) * 100.0).round();
    (cents >= 1.0).then(|| format!("+${:.2}", cents / 100.0))
}

/// What a narrow bar keeps (spec A §C): the delta from 480 px, the word "Context" from 420, the
/// meter from 320; the percent always.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Trim {
    pub delta: bool,
    pub label: bool,
    pub meter: bool,
}

pub fn trim(width: f32) -> Trim {
    Trim {
        delta: width >= 480.0,
        label: width >= 420.0,
        meter: width >= 320.0,
    }
}

/// The pill's mark: a status mark on the shared clock, or Done's still check.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PillMark {
    Status(Status),
    DoneCheck,
}

/// The status pill: its word, fill (none: Idle) and the word's colour, and its mark's.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pill {
    pub word: &'static str,
    pub fill: Option<Rgb>,
    pub word_ink: Rgb,
    pub mark: PillMark,
    /// The check's colour (Done); the moving marks colour themselves from the theme.
    pub mark_ink: Rgb,
}

/// The pill for `status` (`stopping`: a stop asked and not done yet), per spec A §C's table.
pub fn pill(status: Status, stopping: bool, theme: &Theme) -> Pill {
    let ui = theme.ui();
    let page = theme.background;
    let neutral = mix(page, theme.foreground, 0.06);
    let quiet = |fill: Rgb| ensure_contrast(ui.text_dim, fill, TEXT_MIN);
    let tinted = |ring: Rgb| {
        let fill = mix(page, ring, pill_tint(theme));
        (Some(fill), ensure_contrast(ring, fill, TEXT_MIN))
    };
    let (word, (fill, word_ink), mark) = match (status, stopping) {
        (Status::Working | Status::Waiting, true) => (
            "Stopping…",
            (Some(neutral), quiet(neutral)),
            PillMark::Status(Status::Working),
        ),
        (Status::Working, false) => (
            "Working",
            tinted(ui.ring_working),
            PillMark::Status(Status::Working),
        ),
        (Status::Waiting, false) => (
            "Needs input",
            tinted(ui.ring_waiting),
            PillMark::Status(Status::Waiting),
        ),
        (Status::Done, _) => ("Done", (Some(neutral), quiet(neutral)), PillMark::DoneCheck),
        (Status::Idle, _) => ("Idle", (None, quiet(page)), PillMark::Status(Status::Idle)),
    };
    Pill {
        word,
        fill,
        word_ink,
        mark,
        mark_ink: quiet(fill.unwrap_or(page)),
    }
}

/// What the bar remembers between draws to animate its changes (spec A §C): each animation is
/// keyed by a counter of its changes, so the tab's 1 s redraw never starts one again.
#[derive(Clone, Debug, Default)]
pub struct TopState {
    /// The pill's state (status, stopping) and the one before it, which its blend starts from.
    pill_now: Option<(Status, bool)>,
    pub pill_from: Option<(Status, bool)>,
    pub pill_key: u64,
    /// The cost last seen, and what the last turn added (cleared when the next starts working).
    cost_seen: Option<f64>,
    pub delta: Option<String>,
    pub delta_key: u64,
    /// The context last seen and the one before it, which the meter glides from.
    context_now: Option<u8>,
    pub context_from: Option<u8>,
    pub context_key: u64,
}

impl TopState {
    /// The tab's state now: `status`, a stop asked (`stopping`), its cost and context.
    pub fn observe(
        &mut self,
        status: Status,
        stopping: bool,
        cost: Option<f64>,
        context: Option<u8>,
    ) {
        let state = (status, stopping);
        if self.pill_now != Some(state) {
            if self.pill_now.is_some() {
                self.pill_from = self.pill_now;
                self.pill_key += 1;
            }
            if status == Status::Working && self.pill_now.is_none_or(|(s, _)| s != Status::Working)
            {
                self.delta = None;
            }
            self.pill_now = Some(state);
        }
        if cost != self.cost_seen {
            if let Some(delta) = cost_delta(self.cost_seen, cost) {
                self.delta = Some(delta);
                self.delta_key += 1;
            }
            self.cost_seen = cost;
        }
        if context != self.context_now {
            self.context_from = self.context_now;
            self.context_now = context;
            self.context_key += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_tab::Colors;
    use crate::ui_tokens::{MARK_MIN, contrast};

    #[test]
    fn the_meter_bands_at_70_and_90() {
        assert_eq!(band(0), Band::Ok);
        assert_eq!(band(69), Band::Ok);
        assert_eq!(band(70), Band::Caution);
        assert_eq!(band(89), Band::Caution);
        assert_eq!(band(90), Band::Critical);
        assert_eq!(band(100), Band::Critical);
    }

    #[test]
    fn the_meter_fills_its_share_and_shows_any_use() {
        assert_eq!(meter_fill(0, 56.0), 0.0);
        assert_eq!(meter_fill(1, 56.0), 2.0);
        assert_eq!(meter_fill(50, 56.0), 28.0);
        assert_eq!(meter_fill(100, 56.0), 56.0);
    }

    #[test]
    fn the_delta_shows_only_a_rise_from_a_known_cost() {
        assert_eq!(cost_delta(None, Some(1.0)), None);
        assert_eq!(
            cost_delta(Some(3.57), Some(3.99)).as_deref(),
            Some("+$0.42")
        );
        assert_eq!(cost_delta(Some(1.0), Some(1.0)), None);
        assert_eq!(cost_delta(Some(2.0), Some(1.0)), None);
        assert_eq!(cost_delta(Some(1.0), None), None);
    }

    #[test]
    fn narrow_bars_drop_the_delta_then_the_label_then_the_meter() {
        let all = Trim {
            delta: true,
            label: true,
            meter: true,
        };
        assert_eq!(trim(500.0), all);
        assert_eq!(trim(480.0), all);
        assert_eq!(
            trim(450.0),
            Trim {
                delta: false,
                ..all
            }
        );
        assert_eq!(
            trim(400.0),
            Trim {
                delta: false,
                label: false,
                meter: true
            }
        );
        assert_eq!(
            trim(300.0),
            Trim {
                delta: false,
                label: false,
                meter: false
            }
        );
    }

    #[test]
    fn the_pill_says_the_state_in_the_bar_s_words() {
        let theme = crate::theme::Theme::default();
        assert_eq!(pill(Status::Working, false, &theme).word, "Working");
        assert_eq!(pill(Status::Waiting, false, &theme).word, "Needs input");
        assert_eq!(pill(Status::Done, false, &theme).word, "Done");
        assert_eq!(pill(Status::Idle, false, &theme).word, "Idle");
        let stopping = pill(Status::Working, true, &theme);
        assert_eq!(stopping.word, "Stopping…");
        assert_eq!(stopping.mark, PillMark::Status(Status::Working));
        assert_eq!(pill(Status::Done, false, &theme).mark, PillMark::DoneCheck);
        assert_eq!(pill(Status::Idle, false, &theme).fill, None);
        assert!(pill(Status::Working, false, &theme).fill.is_some());
    }

    /// Spec A §D: every word on its pill, the meter on the page and its track, the tool name and
    /// the hovered chevron on a hovered row, in every theme.
    #[test]
    fn the_bar_and_rows_meet_the_floors_in_every_theme() {
        for (name, theme) in crate::theme::every_theme() {
            let c = Colors::of(&theme);
            for (status, stopping) in [
                (Status::Working, false),
                (Status::Waiting, false),
                (Status::Done, false),
                (Status::Idle, false),
                (Status::Working, true),
            ] {
                let p = pill(status, stopping, &theme);
                let on = p.fill.unwrap_or(c.page);
                let got = contrast(p.word_ink, on);
                assert!(got >= TEXT_MIN - 0.01, "{name}: {} {got:.2}", p.word);
            }
            for (what, fill) in [
                ("ok", c.meter_ok),
                ("caution", c.meter_caution),
                ("critical", c.meter_critical),
            ] {
                for (on, bg) in [("page", c.page), ("track", c.meter_track)] {
                    let got = contrast(fill, bg);
                    assert!(
                        got >= MARK_MIN - 0.01,
                        "{name}: meter {what} on {on} {got:.2}"
                    );
                }
            }
            for (what, fg, on) in [
                ("tool name on a hovered row", c.tool_name, c.row_hover),
                ("tool name on a row", c.tool_name, c.panel),
                ("hovered chevron", c.panel_text, c.row_hover),
                ("caution percent", c.pct_caution, c.page),
                ("critical percent", c.pct_critical, c.page),
            ] {
                let got = contrast(fg, on);
                assert!(got >= TEXT_MIN - 0.01, "{name}: {what} {got:.2}");
            }
        }
    }

    #[test]
    fn a_turn_that_raises_the_cost_shows_what_it_added_until_the_next_starts() {
        let mut top = TopState::default();
        top.observe(Status::Working, false, None, None);
        top.observe(Status::Done, false, Some(3.57), None);
        assert_eq!(top.delta, None, "the first cost has nothing to add to");
        top.observe(Status::Working, false, Some(3.57), None);
        top.observe(Status::Done, false, Some(3.99), None);
        assert_eq!(top.delta.as_deref(), Some("+$0.42"));
        let shown = top.delta_key;
        top.observe(Status::Done, false, Some(3.99), None);
        assert_eq!(
            top.delta_key, shown,
            "nothing new: its fade isn't started again"
        );
        top.observe(Status::Working, false, Some(3.99), None);
        assert_eq!(top.delta, None, "the next turn's start clears it");
    }

    #[test]
    fn each_new_context_value_moves_the_meter_from_the_last() {
        let mut top = TopState::default();
        top.observe(Status::Working, false, None, Some(20));
        let first = top.context_key;
        assert_eq!(top.context_from, None, "the first value appears, no glide");
        top.observe(Status::Working, false, None, Some(20));
        assert_eq!(top.context_key, first, "the same value: no glide");
        top.observe(Status::Working, false, None, Some(30));
        assert_eq!(top.context_key, first + 1);
        assert_eq!(top.context_from, Some(20));
    }

    #[test]
    fn a_change_of_state_keys_the_pill_s_blend() {
        let mut top = TopState::default();
        top.observe(Status::Idle, false, None, None);
        let k = top.pill_key;
        top.observe(Status::Idle, false, None, None);
        assert_eq!(top.pill_key, k);
        top.observe(Status::Working, false, None, None);
        assert_eq!(top.pill_key, k + 1);
        assert_eq!(top.pill_from, Some((Status::Idle, false)));
        top.observe(Status::Working, true, None, None);
        assert_eq!(top.pill_key, k + 2, "a stop asked is a new state too");
    }
}
