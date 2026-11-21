//! The status bar's usage strip (M11ap, the lead designer's spec): per agent its logo, how much
//! of its tightest limit is used, and when that resets. Pure: what the strip and its tooltip
//! say; `status_bar` draws it.

use crate::agent_catalog::{Catalogs, Lookup};
use crate::logos::Logo;
use crate::providers::{Reset, UsageGroup, UsageLimit};

/// What the strip shows for one agent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StripValue {
    /// Its tightest limit: how much is used, and how long until it resets (none: unknown, or
    /// said in words).
    Used { used: u8, reset_in: Option<i64> },
    /// Read for the first time, nothing to show yet.
    Reading,
}

/// One agent in the strip.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StripAgent {
    pub name: &'static str,
    pub label: &'static str,
    pub logo: Option<Logo>,
    pub value: StripValue,
}

/// The agent's tightest limit in `groups` at `now_unix`: the most used, then the soonest reset;
/// with how long until it resets.
pub fn tightest(groups: &[UsageGroup], now_unix: i64) -> Option<(u8, Option<i64>)> {
    groups
        .iter()
        .flat_map(|g| &g.limits)
        .filter_map(|l| match l {
            UsageLimit::Read {
                left_pct, resets, ..
            } => {
                let reset_in = match resets {
                    Some(Reset::At(at)) => Some((*at - now_unix).max(0)),
                    _ => None,
                };
                Some((100 - (*left_pct).min(100), reset_in))
            }
            UsageLimit::Text(_) => None,
        })
        .min_by_key(|&(used, reset_in)| (std::cmp::Reverse(used), reset_in.unwrap_or(i64::MAX)))
}

/// `secs` until a reset in its largest unit: `5d`, `23h`, `51m`, `1m`, `now`.
pub fn short_reset(secs: i64) -> String {
    match secs {
        s if s <= 0 => "now".into(),
        s if s >= 86_400 => format!("{}d", s / 86_400),
        s if s >= 3_600 => format!("{}h", s / 3_600),
        s => format!("{}m", (s / 60).max(1)),
    }
}

/// The agents the strip shows, in a fixed order (Claude, then Antigravity): those with a
/// reading (or a live window), and those being read for the first time. One never read (its
/// read failed: most likely not installed) isn't shown.
pub fn strip_agents(catalogs: Option<&Catalogs>, now_unix: i64) -> Vec<StripAgent> {
    let Some(c) = catalogs else {
        return Vec::new();
    };
    crate::providers::all()
        .iter()
        .filter(|p| p.usage_probe().is_some())
        .filter_map(|p| {
            let info = p.info();
            let value = match c.live_usage_at(info.name, now_unix) {
                Some(groups) => {
                    let (used, reset_in) = tightest(&groups, now_unix)?;
                    StripValue::Used { used, reset_in }
                }
                None if matches!(c.usage_last(info.name), Some(Lookup::Loading)) => {
                    StripValue::Reading
                }
                None => return None,
            };
            Some(StripAgent {
                name: info.name,
                label: info.label,
                logo: info.logo,
                value,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn read(name: &str, left: u8, resets: Option<Reset>) -> UsageLimit {
        UsageLimit::Read {
            name: name.into(),
            left_pct: left,
            resets,
        }
    }

    fn one(limits: Vec<UsageLimit>) -> Vec<UsageGroup> {
        vec![UsageGroup {
            name: String::new(),
            about: None,
            limits,
        }]
    }

    #[test]
    fn the_tightest_limit_is_the_most_used_then_the_soonest_reset() {
        let now = 1_000;
        let groups = one(vec![
            read("Session", 60, Some(Reset::At(now + 7_200))),
            read("Week", 23, Some(Reset::At(now + 86_400))),
            read("Fable", 23, Some(Reset::At(now + 3_600))),
            UsageLimit::Text("Couldn't read".into()),
        ]);
        assert_eq!(
            tightest(&groups, now),
            Some((77, Some(3_600))),
            "77% used, the sooner"
        );
        let words = one(vec![read(
            "Session",
            10,
            Some(Reset::Words("Nov 17".into())),
        )]);
        assert_eq!(
            tightest(&words, now),
            Some((90, None)),
            "a reset in words: none shown"
        );
        assert_eq!(tightest(&one(vec![]), now), None);
    }

    #[test]
    fn a_reset_says_its_largest_unit() {
        assert_eq!(short_reset(0), "now");
        assert_eq!(short_reset(59), "1m");
        assert_eq!(short_reset(51 * 60), "51m");
        assert_eq!(short_reset(23 * 3_600 + 60), "23h");
        assert_eq!(short_reset(5 * 86_400 + 12 * 3_600), "5d");
    }

    #[test]
    fn the_strip_shows_agents_read_or_being_read() {
        let now = Instant::now();
        let mut c = Catalogs::default();
        c.set_usage(
            "claude",
            Lookup::Ready(one(vec![read("Session", 23, Some(Reset::At(4_600)))])),
            now,
        );
        c.set_usage("agy", Lookup::Failed("agy: not found".into()), now);
        let agents = strip_agents(Some(&c), 1_000);
        assert_eq!(
            agents.len(),
            1,
            "a never-read agent (not installed) isn't shown"
        );
        assert_eq!(agents[0].label, "Claude");
        assert_eq!(
            agents[0].value,
            StripValue::Used {
                used: 77,
                reset_in: Some(3_600)
            }
        );
        // Being read for the first time: shown, waiting.
        c.set_usage("agy", Lookup::Loading, now);
        let agents = strip_agents(Some(&c), 1_000);
        assert_eq!(agents[1].value, StripValue::Reading);
        assert!(strip_agents(None, 1_000).is_empty());
    }
}
