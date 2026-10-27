//! Session restore: the `session-restore` setting and the saved-sessions file format.
//! Pure and GPUI-free, so the rules are unit-tested; reading and writing the file is the caller's.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::presets::Preset;
use crate::settings::Entry;
use crate::split_tree::Node;

/// How much of the last run comes back at startup.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum RestoreMode {
    Off,
    Layout,
    #[default]
    Agents,
}

impl RestoreMode {
    /// `off` / `layout` / `agents`, trimmed, any case.
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "off" => Some(Self::Off),
            "layout" => Some(Self::Layout),
            "agents" => Some(Self::Agents),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Layout => "layout",
            Self::Agents => "agents",
        }
    }

    /// The last valid `session-restore` entry wins; an empty value resets to the default. Returns
    /// the mode and a warning per invalid value (the previous value stays).
    pub fn from_entries(entries: &[Entry]) -> (Self, Vec<String>) {
        let mut mode = Self::default();
        let mut warnings = Vec::new();
        for entry in entries.iter().filter(|e| e.key == "session-restore") {
            if entry.value.is_empty() {
                mode = Self::default();
            } else if let Some(parsed) = Self::parse(&entry.value) {
                mode = parsed;
            } else {
                warnings.push(format!(
                    "session-restore \"{}\": expected agents, layout or off",
                    entry.value
                ));
            }
        }
        (mode, warnings)
    }
}

/// The file format version; a file with any other version is ignored.
pub const VERSION: u32 = 1;

/// One session as saved: enough to reopen it.
#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
pub struct SavedSession {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub project: Option<PathBuf>,
    #[serde(default)]
    pub preset: Option<Preset>,
}

/// Everything saved about the workspace.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedState {
    pub version: u32,
    #[serde(default)]
    pub active: Option<usize>,
    #[serde(default)]
    pub collapsed: Vec<PathBuf>,
    #[serde(default)]
    pub sessions: Vec<SavedSession>,
    /// One split layout per session (leaves are pane indices); empty means all single panes.
    #[serde(default)]
    pub layouts: Vec<Node<usize>>,
}

/// Pretty JSON with a trailing newline.
pub fn encode(state: &SavedState) -> String {
    let mut text = serde_json::to_string_pretty(state).unwrap_or_default();
    text.push('\n');
    text
}

/// `None` for bad JSON or a different `version`.
pub fn decode(text: &str) -> Option<SavedState> {
    serde_json::from_str::<SavedState>(text)
        .ok()
        .filter(|s| s.version == VERSION)
}

pub fn state_path(home: &Path) -> PathBuf {
    home.join("Library/Application Support/fuxx/sessions.json")
}

/// A session to open: the saved description and whether to run its preset command.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Launch {
    pub saved: SavedSession,
    pub run_preset: bool,
}

/// What to open at startup: nothing, every session as a plain shell, or every session with its
/// agent started again.
pub fn launches(state: &SavedState, mode: RestoreMode) -> Vec<Launch> {
    if mode == RestoreMode::Off {
        return Vec::new();
    }
    state
        .sessions
        .iter()
        .map(|s| Launch {
            saved: s.clone(),
            run_preset: mode == RestoreMode::Agents && s.preset.is_some(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::split_tree::Axis;

    fn r(value: &str) -> Entry {
        Entry {
            key: "session-restore".into(),
            value: value.into(),
        }
    }
    fn sample() -> SavedState {
        SavedState {
            version: VERSION,
            active: Some(1),
            collapsed: vec![PathBuf::from("/w/api")],
            sessions: vec![
                SavedSession::default(),
                SavedSession {
                    name: Some("🖕 api".into()),
                    project: Some(PathBuf::from("/w/api")),
                    preset: Some(Preset {
                        name: "claude".into(),
                        command: "claude".into(),
                    }),
                },
            ],
            layouts: vec![
                Node::Leaf(0),
                Node::Split {
                    axis: Axis::Row,
                    ratio: 0.5,
                    first: Box::new(Node::Leaf(0)),
                    second: Box::new(Node::Leaf(1)),
                },
            ],
        }
    }

    #[test]
    fn restore_mode_parses_and_defaults_to_agents() {
        assert_eq!(RestoreMode::parse(" Layout "), Some(RestoreMode::Layout));
        assert_eq!(RestoreMode::parse("nope"), None);
        assert_eq!(RestoreMode::from_entries(&[]).0, RestoreMode::Agents);
        assert_eq!(RestoreMode::from_entries(&[r("off")]).0, RestoreMode::Off);
        assert_eq!(
            RestoreMode::from_entries(&[r("off"), r("")]).0,
            RestoreMode::Agents
        );
        let (mode, warnings) = RestoreMode::from_entries(&[r("layout"), r("bogus")]);
        assert_eq!((mode, warnings.len()), (RestoreMode::Layout, 1));
        assert_eq!(RestoreMode::Off.as_str(), "off");
    }

    #[test]
    fn layouts_round_trip_and_default_to_none() {
        let mut s = sample();
        s.layouts = vec![Node::Split {
            axis: Axis::Column,
            ratio: 0.3,
            first: Box::new(Node::Leaf(0)),
            second: Box::new(Node::Leaf(1)),
        }];
        assert_eq!(decode(&encode(&s)), Some(s));
        assert_eq!(decode("{\"version\": 1}").unwrap().layouts, vec![]);
    }

    #[test]
    fn encode_then_decode_round_trips() {
        assert_eq!(decode(&encode(&sample())), Some(sample()));
    }

    #[test]
    fn decode_rejects_garbage_and_other_versions() {
        assert_eq!(decode(""), None);
        assert_eq!(decode("{\"version\": 1, \"sessions\": [tru"), None);
        assert_eq!(decode("{\"version\": 2}"), None);
    }

    #[test]
    fn decode_fills_missing_fields() {
        let state = decode("{\"version\": 1, \"sessions\": [{}]}").unwrap();
        assert_eq!(state.active, None);
        assert_eq!(state.sessions, vec![SavedSession::default()]);
    }

    #[test]
    fn launches_follow_the_mode() {
        let s = sample();
        assert!(launches(&s, RestoreMode::Off).is_empty());
        let layout = launches(&s, RestoreMode::Layout);
        assert_eq!(layout.len(), 2);
        assert!(layout.iter().all(|l| !l.run_preset));
        let agents = launches(&s, RestoreMode::Agents);
        assert_eq!(
            agents.iter().map(|l| l.run_preset).collect::<Vec<_>>(),
            [false, true]
        );
        assert_eq!(agents[1].saved, s.sessions[1]);
    }

    #[test]
    fn state_path_is_in_application_support() {
        assert_eq!(
            state_path(Path::new("/Users/me")),
            PathBuf::from("/Users/me/Library/Application Support/fuxx/sessions.json")
        );
    }
}
