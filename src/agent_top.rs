//! The agent tab's top bar (chores after M11ab, the lead designer's spec A §C): the status pill,
//! the conversation's cost and its live context meter. Pure: the words, colours and sizes the
//! bar draws with; `agent_tab::top_line` draws them.

use std::time::Instant;

use alacritty_terminal::vte::ansi::Rgb;

use crate::status::Status;
use crate::theme::{Theme, mix};
use crate::ui_tokens::{TEXT_MIN, ensure_contrast};

/// Whether a change made `at` is still within its animation's `duration` at `now` (else it is
/// drawn at its end).
pub fn animating(at: Option<Instant>, now: Instant, duration: std::time::Duration) -> bool {
    at.is_some_and(|at| now.saturating_duration_since(at) < duration)
}

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
        // Your word (2026-11-21): "working change to cultivating…".
        (Status::Working, false) => (
            "Cultivating…",
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
    /// When each last changed: its animation plays only while it is new (`animating`), so a
    /// tab shown again draws them at their ends (the review's finding 4).
    pub pill_at: Option<Instant>,
    pub delta_at: Option<Instant>,
    pub context_at: Option<Instant>,
    /// When the cost and the context first showed (their groups fade in then).
    pub cost_shown_at: Option<Instant>,
    pub context_shown_at: Option<Instant>,
    /// The cost last seen, and what the last turn added (cleared when the next starts working).
    cost_seen: Option<f64>,
    pub delta: Option<String>,
    pub delta_key: u64,
    /// The context last seen and the one before it, which the meter glides from.
    context_now: Option<u8>,
    pub context_from: Option<u8>,
    pub context_key: u64,
    /// When the current Working stretch began, and how long the last one took (the status
    /// line's `✓ Done · 0:42`).
    work_began: Option<Instant>,
    pub took: Option<std::time::Duration>,
}

/// The clock's warning dot (the 2026-11-24 chores): the context is past 80 %.
pub fn context_warning(pct: Option<u8>) -> bool {
    pct.is_some_and(|p| p > 80)
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
        self.observe_at(status, stopping, cost, context, Instant::now());
    }

    /// `observe`, the changes stamped `now`.
    pub fn observe_at(
        &mut self,
        status: Status,
        stopping: bool,
        cost: Option<f64>,
        context: Option<u8>,
        now: Instant,
    ) {
        let state = (status, stopping);
        if self.pill_now != Some(state) {
            if self.pill_now.is_some() {
                self.pill_from = self.pill_now;
                self.pill_key += 1;
                self.pill_at = Some(now);
            }
            let was_working = self.pill_now.is_some_and(|(s, _)| s == Status::Working);
            if status == Status::Working && !was_working {
                self.delta = None;
                self.work_began = Some(now);
                self.took = None;
            }
            if status != Status::Working && was_working {
                self.took = self
                    .work_began
                    .take()
                    .map(|t| now.saturating_duration_since(t));
            }
            self.pill_now = Some(state);
        }
        if cost != self.cost_seen {
            if cost.is_none() {
                self.delta = None;
            }
            if self.cost_seen.is_none() {
                self.cost_shown_at = Some(now);
            }
            if let Some(delta) = cost_delta(self.cost_seen, cost) {
                self.delta = Some(delta);
                self.delta_key += 1;
                self.delta_at = Some(now);
            }
            self.cost_seen = cost;
        }
        if context != self.context_now {
            if self.context_now.is_none() {
                self.context_shown_at = Some(now);
            }
            self.context_from = self.context_now;
            self.context_now = context;
            self.context_key += 1;
            self.context_at = Some(now);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_tab::Colors;
    use crate::ui_tokens::{MARK_MIN, contrast};
    use std::time::Duration;

    #[test]
    fn context_warning_past_80_percent() {
        assert!(!context_warning(None));
        assert!(!context_warning(Some(80)));
        assert!(context_warning(Some(81)));
    }

    #[test]
    fn a_turn_records_how_long_it_worked() {
        let t0 = Instant::now();
        let mut top = TopState::default();
        top.observe_at(Status::Working, false, None, None, t0);
        assert_eq!(top.took, None);
        top.observe_at(
            Status::Working,
            false,
            None,
            None,
            t0 + Duration::from_secs(10),
        );
        top.observe_at(
            Status::Done,
            false,
            None,
            None,
            t0 + Duration::from_secs(42),
        );
        assert_eq!(top.took, Some(Duration::from_secs(42)));
        top.observe_at(
            Status::Working,
            false,
            None,
            None,
            t0 + Duration::from_secs(50),
        );
        assert_eq!(top.took, None, "a new turn clears it");
    }

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
        // Your word (2026-11-21): "working change to cultivating…".
        assert_eq!(pill(Status::Working, false, &theme).word, "Cultivating…");
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

    /// `/clear`: the cost goes, and what the last turn added with it.
    #[test]
    fn a_cleared_cost_takes_its_delta_with_it() {
        let mut top = TopState::default();
        top.observe(Status::Done, false, Some(1.0), None);
        top.observe(Status::Done, false, Some(1.5), None);
        assert!(top.delta.is_some());
        top.observe(Status::Done, false, None, None);
        assert_eq!(top.delta, None);
    }

    /// The review's finding 4: a change's animation plays once, when it happens; drawn again
    /// later (the tab shown again) it is drawn at its end.
    #[test]
    fn a_change_animates_only_while_it_is_new() {
        use std::time::{Duration, Instant};
        let t0 = Instant::now();
        let mut top = TopState::default();
        top.observe_at(Status::Idle, false, None, None, t0);
        top.observe_at(Status::Working, false, None, Some(20), t0);
        let ms = Duration::from_millis;
        assert!(animating(top.pill_at, t0 + ms(100), ms(200)), "mid-blend");
        assert!(
            !animating(top.pill_at, t0 + ms(250), ms(200)),
            "over: its end"
        );
        assert!(!animating(None, t0, ms(200)), "no change: nothing to play");
        assert_eq!(top.context_at, Some(t0));
        top.observe_at(Status::Working, false, None, Some(20), t0 + ms(900));
        assert_eq!(top.context_at, Some(t0), "the same value: no new glide");
    }
}
