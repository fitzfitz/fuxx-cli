//! The Welcome card's rules (M11k): its rows, which one is chosen, what a pick opens and what
//! it replaces, and its words. Pure: the card (`welcome.rs`) draws and wires them.

use std::path::Path;

use crate::agent_detect::Found;
use crate::presets::Preset;
use crate::restore::{Launch, SavedSession};

pub const TITLE: &str = "Welcome to fuxx";
pub const SUBTITLE: &str = "A terminal for running AI coding agents.";
pub const HEADING_FOLDER: &str = "Pick a folder";
pub const HEADING_AGENT: &str = "Start an agent";
pub const HEADING_SHORTCUTS: &str = "Three shortcuts";
pub const CHOOSE: &str = "Choose\u{2026}";
pub const PLAIN_TERMINAL: &str = "Plain terminal";
pub const NO_PRESETS: &str = "No agent presets set.";
pub const HINT: &str = "Enter start \u{b7} \u{2191}\u{2193} choose \u{b7} Esc skip";
pub const SKIP: &str = "Skip";
pub const OPEN_TERMINAL: &str = "Open Terminal";

/// Each shortcut: the action's name (its key is read from the bindings) and its line.
pub const SHORTCUTS: [(&str, &str); 3] = [
    (
        "workspace::OpenPalette",
        "Command palette \u{2014} do anything",
    ),
    ("workspace::NewWorkspace", "New workspace for another agent"),
    (
        "workspace::NextNeedingYou",
        "Go to the agent that needs you",
    ),
];

/// Why the card is open.
#[allow(dead_code)] // made by the first run and Help (M11k tasks 3 and 4)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    /// The first run, over the terminal fuxx opened by itself (its id).
    FirstRun { over: u64 },
    /// Opened from the Help menu.
    Help,
}

/// What the card starts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Choice {
    Preset(Preset),
    Plain,
}

/// The card's rows, in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Row {
    Folder,
    Preset(usize),
    Plain,
}

/// Folder, one row per preset, then the plain terminal.
pub fn rows(presets: usize) -> Vec<Row> {
    let mut rows = vec![Row::Folder];
    rows.extend((0..presets).map(Row::Preset));
    rows.push(Row::Plain);
    rows
}

/// The next row down or up from `at`, round (a row not in `rows` counts as the first).
pub fn step(rows: &[Row], at: Row, down: bool) -> Row {
    let Some(first) = rows.first().copied() else {
        return at;
    };
    let Some(i) = rows.iter().position(|r| *r == at) else {
        return first;
    };
    let n = rows.len();
    rows[if down { (i + 1) % n } else { (i + n - 1) % n }]
}

/// The row chosen at first (and while detection hasn't been overruled by a key): the first
/// preset found; else the plain terminal when every preset is missing (or there are none);
/// else the first preset.
pub fn default_row(found: &[Found]) -> Row {
    if let Some(i) = found.iter().position(|f| *f == Found::Yes) {
        Row::Preset(i)
    } else if found.iter().all(|f| *f == Found::No) {
        Row::Plain
    } else {
        Row::Preset(0)
    }
}

/// The session `choice` opens in `folder`; home is no project.
#[allow(dead_code)] // the pick (M11k task 3)
pub fn start_launch(folder: &Path, home: Option<&Path>, choice: &Choice) -> Launch {
    let project = (home != Some(folder)).then(|| folder.to_path_buf());
    let (preset, run_preset) = match choice {
        Choice::Preset(p) => (Some(p.clone()), true),
        Choice::Plain => (None, false),
    };
    Launch {
        saved: SavedSession {
            project,
            preset,
            ..SavedSession::default()
        },
        run_preset,
    }
}

/// What a pick does.
#[allow(dead_code)] // the pick (M11k task 3)
#[derive(Debug, PartialEq, Eq)]
pub enum Effect {
    /// Nothing to open: the terminal already there is what was asked for.
    JustClose,
    /// Open the launch; close the terminal `close_after` once the new one exists.
    Open { close_after: Option<u64> },
}

/// A plain terminal in home on the first run is what is already open. Otherwise a pick opens
/// a new session, and on the first run replaces the first terminal if it is still alive and
/// alone (`over_alone`).
#[allow(dead_code)] // the pick (M11k task 3)
pub fn pick_effect(
    origin: Origin,
    over_alone: bool,
    choice: &Choice,
    folder_is_home: bool,
) -> Effect {
    match origin {
        Origin::FirstRun { .. } if *choice == Choice::Plain && folder_is_home => Effect::JustClose,
        Origin::FirstRun { over } if over_alone => Effect::Open {
            close_after: Some(over),
        },
        _ => Effect::Open { close_after: None },
    }
}

/// The primary button's label.
pub fn start_label(choice: &Choice) -> String {
    match choice {
        Choice::Preset(p) => format!("Start {}", p.name),
        Choice::Plain => OPEN_TERMINAL.to_string(),
    }
}

/// The word beside a preset row, if any.
pub fn status_word(found: Found) -> Option<&'static str> {
    match found {
        Found::Checking => Some("Checking\u{2026}"),
        Found::No => Some("Not found on your PATH"),
        Found::Yes | Found::Unknown => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn preset(name: &str) -> Preset {
        Preset {
            name: name.to_string(),
            command: name.to_string(),
        }
    }

    #[test]
    fn rows_are_folder_presets_then_plain() {
        assert_eq!(
            rows(3),
            vec![
                Row::Folder,
                Row::Preset(0),
                Row::Preset(1),
                Row::Preset(2),
                Row::Plain
            ]
        );
        assert_eq!(rows(0), vec![Row::Folder, Row::Plain]);
    }

    #[test]
    fn step_wraps() {
        let r = rows(2);
        assert_eq!(step(&r, Row::Plain, true), Row::Folder);
        assert_eq!(step(&r, Row::Folder, false), Row::Plain);
        assert_eq!(step(&r, Row::Folder, true), Row::Preset(0));
        assert_eq!(step(&r, Row::Preset(1), true), Row::Plain);
        assert_eq!(step(&r, Row::Preset(0), false), Row::Folder);
    }

    #[test]
    fn default_row_picks_the_first_found() {
        use Found::*;
        assert_eq!(default_row(&[Checking; 3]), Row::Preset(0));
        assert_eq!(default_row(&[No, Yes, Yes]), Row::Preset(1));
        assert_eq!(default_row(&[No, No]), Row::Plain);
        assert_eq!(default_row(&[Unknown, No]), Row::Preset(0));
        assert_eq!(default_row(&[]), Row::Plain);
    }

    #[test]
    fn start_launch_leaves_home_without_a_folder() {
        let home = PathBuf::from("/Users/me");
        let plain = start_launch(&home, Some(&home), &Choice::Plain);
        assert_eq!(plain.saved.project, None);
        assert_eq!(plain.saved.preset, None);
        assert!(!plain.run_preset);

        let codex = preset("codex");
        let app = start_launch(
            Path::new("/w/app"),
            Some(&home),
            &Choice::Preset(codex.clone()),
        );
        assert_eq!(app.saved.project, Some(PathBuf::from("/w/app")));
        assert_eq!(app.saved.preset, Some(codex));
        assert!(app.run_preset);

        // No home known: any folder is a project.
        let any = start_launch(Path::new("/Users/me"), None, &Choice::Plain);
        assert_eq!(any.saved.project, Some(PathBuf::from("/Users/me")));
    }

    #[test]
    fn pick_effect_replaces_only_an_alone_first_terminal() {
        let first = Origin::FirstRun { over: 1 };
        let agent = Choice::Preset(preset("claude"));
        let open = |close_after| Effect::Open { close_after };
        assert_eq!(
            pick_effect(first, true, &Choice::Plain, true),
            Effect::JustClose
        );
        assert_eq!(pick_effect(first, true, &agent, true), open(Some(1)));
        assert_eq!(
            pick_effect(first, true, &Choice::Plain, false),
            open(Some(1))
        );
        assert_eq!(pick_effect(first, false, &agent, false), open(None));
        assert_eq!(
            pick_effect(Origin::Help, true, &Choice::Plain, true),
            open(None)
        );
    }

    #[test]
    fn labels_follow_the_choice() {
        assert_eq!(
            start_label(&Choice::Preset(preset("claude"))),
            "Start claude"
        );
        assert_eq!(start_label(&Choice::Plain), "Open Terminal");
        assert_eq!(status_word(Found::Checking), Some("Checking\u{2026}"));
        assert_eq!(status_word(Found::No), Some("Not found on your PATH"));
        assert_eq!(status_word(Found::Yes), None);
        assert_eq!(status_word(Found::Unknown), None);
    }
}
