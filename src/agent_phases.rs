//! A busy step's phases (the 2026-11-24 chores, your pick "C2 with C1's hover"): its columns, one
//! call each in time order, fall under bands saying what the agent was doing — Looking around
//! (reads, searches), Ran (commands), Changing (edits), Checking (tests and builds) — so the
//! icons read as a story. Pure: built from a step's `Tracks`.

use std::ops::Range;

use crate::agent_steps::{Glyph, Tracks};

/// What the agent was doing in a run of calls.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Phase {
    Looking,
    Ran,
    Changing,
    Checking,
}

impl Phase {
    /// The band's words.
    pub fn word(self) -> &'static str {
        match self {
            Phase::Looking => "Looking around",
            Phase::Ran => "Ran",
            Phase::Changing => "Changing",
            Phase::Checking => "Checking",
        }
    }

    /// Which phase a column takes when its call touches several tracks.
    fn weight(self) -> u8 {
        match self {
            Phase::Looking => 0,
            Phase::Ran => 1,
            Phase::Changing => 2,
            Phase::Checking => 3,
        }
    }
}

/// The phase a bead's call belongs to. A running call is "Ran" until its kind shows.
pub fn phase_of(g: Glyph) -> Phase {
    match g {
        Glyph::Read | Glyph::Search => Phase::Looking,
        Glyph::Command | Glyph::Script | Glyph::Running => Phase::Ran,
        Glyph::Edit => Phase::Changing,
        Glyph::Passed | Glyph::Failed | Glyph::Planned | Glyph::Check | Glyph::Cleared => {
            Phase::Checking
        }
    }
}

/// Each column's phase: the weightiest of its beads' (a call with no bead: Ran).
pub fn column_phases(tracks: &Tracks) -> Vec<Phase> {
    let mut phases: Vec<Option<Phase>> = vec![None; tracks.columns.len()];
    let all = tracks
        .checks
        .iter()
        .chain(&tracks.files)
        .chain(&tracks.commands);
    for track in all {
        for &(col, bead) in &track.beads {
            let p = phase_of(bead.glyph);
            if let Some(slot) = phases.get_mut(col)
                && slot.is_none_or(|q| p.weight() > q.weight())
            {
                *slot = Some(p);
            }
        }
    }
    phases
        .into_iter()
        .map(|p| p.unwrap_or(Phase::Ran))
        .collect()
}

/// The bands over the columns: one per run of columns of one phase.
pub fn bands(phases: &[Phase]) -> Vec<(Phase, Range<usize>)> {
    let mut out: Vec<(Phase, Range<usize>)> = Vec::new();
    for (i, &p) in phases.iter().enumerate() {
        match out.last_mut() {
            Some((q, run)) if *q == p => run.end = i + 1,
            _ => out.push((p, i..i + 1)),
        }
    }
    out
}

/// A column's hover words: its number (1st call: #1) and what the call did.
pub fn column_tip(col: usize, tip: &str) -> String {
    format!("#{} · {tip}", col + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_and_searches_are_looking_around() {
        assert_eq!(phase_of(Glyph::Read), Phase::Looking);
        assert_eq!(phase_of(Glyph::Search), Phase::Looking);
        assert_eq!(Phase::Looking.word(), "Looking around");
    }

    #[test]
    fn commands_ran() {
        assert_eq!(phase_of(Glyph::Command), Phase::Ran);
        assert_eq!(phase_of(Glyph::Script), Phase::Ran);
        assert_eq!(phase_of(Glyph::Running), Phase::Ran);
    }

    #[test]
    fn edits_are_changing() {
        assert_eq!(phase_of(Glyph::Edit), Phase::Changing);
    }

    #[test]
    fn checks_are_checking() {
        for g in [
            Glyph::Passed,
            Glyph::Failed,
            Glyph::Planned,
            Glyph::Check,
            Glyph::Cleared,
        ] {
            assert_eq!(phase_of(g), Phase::Checking, "{g:?}");
        }
    }

    #[test]
    fn bands_merge_runs_of_one_phase() {
        use Phase::*;
        assert_eq!(
            bands(&[Looking, Looking, Ran, Changing, Changing]),
            [(Looking, 0..2), (Ran, 2..3), (Changing, 3..5)]
        );
        assert_eq!(bands(&[]), []);
        assert_eq!(
            bands(&[Looking, Ran, Looking]),
            [(Looking, 0..1), (Ran, 1..2), (Looking, 2..3)]
        );
    }

    #[test]
    fn the_column_tip_names_the_action() {
        assert_eq!(
            column_tip(4, "Read tab_bar.rs, lines 1280–1296"),
            "#5 · Read tab_bar.rs, lines 1280–1296"
        );
    }
}
