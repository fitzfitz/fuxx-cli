//! Session restore: the `session-restore` setting and the saved-sessions file format.
//! Pure and GPUI-free, so the rules are unit-tested; reading and writing the file is the caller's.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::presets::Preset;
use crate::settings::Entry;
use crate::split_tree::Node;
use crate::ssh_target::SshTarget;

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
    /// The background service's id for it (persistence on), so a reopen finds it running.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub daemon_id: Option<u64>,
    /// An SSH session's host and tmux choice (M10b); a restore connects again.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh: Option<SshTarget>,
}

/// Everything saved about the workspace.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedState {
    pub version: u32,
    #[serde(default)]
    pub active: Option<usize>,
    #[serde(default)]
    pub collapsed: Vec<PathBuf>,
    /// The SSH hosts whose groups are closed (M10b).
    #[serde(default)]
    pub collapsed_hosts: Vec<String>,
    #[serde(default)]
    pub sessions: Vec<SavedSession>,
    /// One split layout per session (leaves are pane indices); empty means all single panes.
    #[serde(default)]
    pub layouts: Vec<Node<usize>>,
    /// The background service the sessions' `daemon_id`s belong to (its random instance id):
    /// ids from another one mean nothing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub daemon_instance: Option<u64>,
    /// The sidebar's width as you left it (M11); none: the default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sidebar_width: Option<f32>,
    /// The sidebar was hidden (⌘\).
    #[serde(default)]
    pub sidebar_hidden: bool,
    /// The first-run hint card was closed, for good.
    #[serde(default)]
    pub hint_dismissed: bool,
}

impl Default for SavedState {
    /// An empty workspace in this file version.
    fn default() -> Self {
        Self {
            version: VERSION,
            active: None,
            collapsed: Vec::new(),
            collapsed_hosts: Vec::new(),
            sessions: Vec::new(),
            layouts: Vec::new(),
            daemon_instance: None,
            sidebar_width: None,
            sidebar_hidden: false,
            hint_dismissed: false,
        }
    }
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
/// agent started again. A session's preset is looked up by name in `presets` (today's settings),
/// so an edited preset runs its new command and a removed one leaves a plain shell.
pub fn launches(state: &SavedState, mode: RestoreMode, presets: &[Preset]) -> Vec<Launch> {
    state
        .sessions
        .iter()
        .filter_map(|s| launch(s, mode, presets))
        .collect()
}

/// One saved session's `launches` entry (`None` when the mode is off).
pub fn launch(saved: &SavedSession, mode: RestoreMode, presets: &[Preset]) -> Option<Launch> {
    if mode == RestoreMode::Off {
        return None;
    }
    let preset = saved
        .preset
        .as_ref()
        .and_then(|p| presets.iter().find(|today| today.name == p.name))
        .cloned();
    Some(Launch {
        run_preset: mode == RestoreMode::Agents && preset.is_some(),
        saved: SavedSession {
            preset,
            ..saved.clone()
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presets::builtins;
    use crate::split_tree::Axis;
    use crate::ssh_target::SshTarget;

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
            collapsed_hosts: Vec::new(),
            sessions: vec![
                SavedSession::default(),
                SavedSession {
                    name: Some("🖕 api".into()),
                    project: Some(PathBuf::from("/w/api")),
                    preset: Some(Preset {
                        name: "claude".into(),
                        command: "claude".into(),
                    }),
                    daemon_id: Some(4),
                    ssh: None,
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
            daemon_instance: Some(u64::MAX),
            ..SavedState::default()
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
    fn a_daemon_id_round_trips_and_is_absent_from_older_files() {
        let state = decode(&encode(&sample())).unwrap();
        assert_eq!(state.sessions[1].daemon_id, Some(4));
        assert_eq!(state.daemon_instance, Some(u64::MAX));
        assert_eq!(decode("{\"version\": 1}").unwrap().daemon_instance, None);
        assert_eq!(state.sessions[0].daemon_id, None);
        // A file from before persistence: no field, so none.
        let old = decode("{\"version\": 1, \"sessions\": [{\"name\": \"a\"}]}").unwrap();
        assert_eq!(old.sessions[0].daemon_id, None);
        // Not written for a session the service doesn't run.
        assert!(!encode(&sample()).contains("\"daemon_id\": null"));
    }

    #[test]
    fn ui_state_round_trips_and_old_files_load() {
        let state = SavedState {
            sidebar_width: Some(312.0),
            sidebar_hidden: true,
            hint_dismissed: true,
            ..sample()
        };
        assert_eq!(decode(&encode(&state)), Some(state));
        // A file from before M11: the sidebar at its default width, shown, with the hint.
        let old = decode(r#"{"version":1,"sessions":[{}]}"#).expect("an old file loads");
        assert_eq!(old.sidebar_width, None);
        assert!(!old.sidebar_hidden);
        assert!(!old.hint_dismissed);
        assert_eq!(old.sessions.len(), 1);
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
        assert!(launches(&s, RestoreMode::Off, &builtins()).is_empty());
        let layout = launches(&s, RestoreMode::Layout, &builtins());
        assert_eq!(layout.len(), 2);
        assert!(layout.iter().all(|l| !l.run_preset));
        let agents = launches(&s, RestoreMode::Agents, &builtins());
        assert_eq!(
            agents.iter().map(|l| l.run_preset).collect::<Vec<_>>(),
            [false, true]
        );
        assert_eq!(agents[1].saved, s.sessions[1]);
    }

    #[test]
    fn launches_use_the_current_preset_by_name() {
        let edited = Preset {
            name: "claude".into(),
            command: "claude --continue".into(),
        };
        let agents = launches(
            &sample(),
            RestoreMode::Agents,
            std::slice::from_ref(&edited),
        );
        assert_eq!(agents[1].saved.preset, Some(edited));
        assert!(agents[1].run_preset);
        // A preset that's no longer in the settings comes back as a plain shell.
        let gone = launches(&sample(), RestoreMode::Agents, &[]);
        assert_eq!(gone[1].saved.preset, None);
        assert!(!gone[1].run_preset);
    }

    fn devbox(tmux: bool) -> SshTarget {
        SshTarget {
            host: "devbox".into(),
            tmux,
            tmux_name: tmux.then(|| "devbox-0a0b0c".into()),
        }
    }

    #[test]
    fn saved_ssh_round_trips_and_old_files_load() {
        let mut s = sample();
        s.sessions[0].ssh = Some(devbox(true));
        s.collapsed_hosts = vec!["devbox".into()];
        assert_eq!(decode(&encode(&s)), Some(s));
        // Not written for a local session.
        assert!(!encode(&sample()).contains("\"ssh\""));
        // A file from before SSH sessions: none, and no collapsed hosts.
        let old = decode("{\"version\": 1, \"sessions\": [{\"name\": \"a\"}]}").unwrap();
        assert_eq!(old.sessions[0].ssh, None);
        assert!(old.collapsed_hosts.is_empty());
        // tmux left out reads as off.
        let bare = decode(r#"{"version": 1, "sessions": [{"ssh": {"host": "devbox"}}]}"#).unwrap();
        assert_eq!(bare.sessions[0].ssh, Some(devbox(false)));
    }

    #[test]
    fn restore_reopens_ssh_in_layout_and_agents_not_off() {
        let mut s = sample();
        s.sessions[0].ssh = Some(devbox(true));
        assert!(launches(&s, RestoreMode::Off, &builtins()).is_empty());
        for mode in [RestoreMode::Layout, RestoreMode::Agents] {
            let l = launches(&s, mode, &builtins());
            assert_eq!(l[0].saved.ssh, Some(devbox(true)), "{mode:?}");
            // Connecting is the place, not a program: nothing is typed in.
            assert!(!l[0].run_preset, "{mode:?}");
            assert_eq!(l[1].saved.ssh, None, "{mode:?}");
        }
    }

    #[test]
    fn state_path_is_in_application_support() {
        assert_eq!(
            state_path(Path::new("/Users/me")),
            PathBuf::from("/Users/me/Library/Application Support/fuxx/sessions.json")
        );
    }
}
