//! Session restore: the `session-restore` setting and the saved-sessions file format.
//! Pure and GPUI-free, so the rules are unit-tested; reading and writing the file is the caller's.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::layout_model::{self, Model, SavedWorkspace};
use crate::presets::Preset;
use crate::settings::Entry;
use crate::split_tree::Node;
use crate::ssh_target::SshTarget;
use crate::tab_color::{self, TabColor};

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

/// The file format version written: 2 holds workspaces (M11b). Version 1 files (sessions and
/// their split layouts) still load; any other version is ignored.
pub const VERSION: u32 = layout_model::SAVED_VERSION;

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
    /// The colour given it from its row's menu (M11); a name not known here reads as none.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "tab_color::lenient"
    )]
    pub color: Option<TabColor>,
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
    /// Version 1: one split layout per session (leaves are session indices); empty means all
    /// single panes. Version 2 keeps `workspaces` instead.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub layouts: Vec<Node<usize>>,
    /// Version 2 (M11b): the sidebar's workspaces, their panes and tabs (a terminal tab names its
    /// session by its index in `sessions`).
    #[serde(default)]
    pub workspaces: Vec<SavedWorkspace>,
    /// The focused workspace, by its index in `workspaces`.
    #[serde(default)]
    pub active_workspace: Option<usize>,
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
    /// A browser window was opened with `browser-keep-data` on, so the saved (on-disk) browser
    /// data may hold something; cleared once it has been deleted.
    #[serde(default)]
    pub browser_data_kept: bool,
    /// The right sidebar was closed (M11g); a file from before it: closed.
    #[serde(default = "closed")]
    pub right_sidebar_hidden: bool,
    /// The right sidebar's width as you left it; none: the default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub right_sidebar_width: Option<f32>,
    /// The right sidebar's panel as you left it (M11h): `files`, `changes` or `activity`;
    /// missing or unknown: Files.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub right_sidebar_panel: Option<String>,
    /// The Welcome card has been shown (M11k); a file from before it: welcomed.
    #[serde(default = "already_welcomed")]
    pub welcomed: bool,
}

/// The right sidebar's state in a file without it: closed.
fn closed() -> bool {
    true
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
            workspaces: Vec::new(),
            active_workspace: None,
            daemon_instance: None,
            sidebar_width: None,
            sidebar_hidden: false,
            hint_dismissed: false,
            browser_data_kept: false,
            right_sidebar_hidden: true,
            right_sidebar_width: None,
            right_sidebar_panel: None,
            welcomed: false,
        }
    }
}

/// `welcomed` in a file without it: an existing user has no first run.
fn already_welcomed() -> bool {
    true
}

/// Whether the Welcome card is due: no readable record, or one that says it was never shown.
pub fn welcome_due(saved: Option<&SavedState>) -> bool {
    saved.is_none_or(|s| !s.welcomed)
}

/// Whether the file has anything to put back: a session, or a workspace (one of pages only
/// comes back with no terminal, M11b).
pub fn has_anything(state: &SavedState) -> bool {
    !state.sessions.is_empty() || !state.workspaces.is_empty()
}

/// Whether a restore shows anything: a terminal came back (`terminals_back`), or `model` (built
/// from the file) holds a workspace, as one of pages does with no terminal. When it doesn't, one
/// plain session opens instead.
pub fn shows_anything(model: &Model, terminals_back: bool) -> bool {
    terminals_back || !model.workspaces().is_empty()
}

/// Pretty JSON with a trailing newline.
pub fn encode(state: &SavedState) -> String {
    let mut text = serde_json::to_string_pretty(state).unwrap_or_default();
    text.push('\n');
    text
}

/// `None` for bad JSON or a version this fuxx doesn't read (1 and 2 it does).
pub fn decode(text: &str) -> Option<SavedState> {
    serde_json::from_str::<SavedState>(text)
        .ok()
        .filter(|s| (1..=VERSION).contains(&s.version))
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
    use crate::layout_model::{SavedPane, SavedTab, SavedWorkspace, TabKind};
    use crate::presets::builtins;
    use crate::split_tree::Axis;
    use crate::ssh_target::SshTarget;

    /// A version-2 file of two workspaces: "build" shows saved session 0 (a terminal), "docs"
    /// a page; "build" was focused.
    fn a_terminal_and_pages() -> SavedState {
        let workspace = |name: &str, tab| SavedWorkspace {
            name: Some(name.into()),
            color: None,
            pinned: false,
            placed: None,
            layout: Node::Leaf(SavedPane {
                tabs: vec![tab],
                active: 0,
            }),
            focused: 0,
        };
        SavedState {
            sessions: vec![SavedSession::default()],
            workspaces: vec![
                workspace("build", SavedTab::Terminal { session: 0 }),
                workspace(
                    "docs",
                    SavedTab::Browser {
                        url: Some("https://docs.rs/gpui".into()),
                    },
                ),
            ],
            active_workspace: Some(0),
            ..SavedState::default()
        }
    }

    /// The one workspace `model` shows, by name, and its focused tab's kind.
    fn shown(model: &Model) -> Vec<(Option<String>, TabKind)> {
        model
            .workspaces()
            .iter()
            .map(|w| {
                let pane = model.pane(w.focused).expect("its pane");
                let tab = model.tab(pane.active).expect("its tab");
                (w.name.clone(), tab.kind.clone())
            })
            .collect()
    }

    #[test]
    fn a_file_of_pages_only_comes_back_with_its_urls() {
        // The terminal tab closed before quitting, the page stayed: no session is saved.
        let mut state = a_terminal_and_pages();
        state.sessions.clear();
        state.workspaces.remove(0);
        state.active_workspace = Some(0);
        let state = decode(&encode(&state)).expect("it loads");
        assert!(has_anything(&state), "a workspace of pages is something");
        let model = Model::from_state(&state, &[]);
        model.check().unwrap();
        assert!(shows_anything(&model, false), "no plain session instead");
        let page = TabKind::Browser {
            url: Some("https://docs.rs/gpui".into()),
        };
        assert_eq!(shown(&model), [(Some("docs".into()), page.clone())]);
        assert_eq!(model.tab(model.focused_tab().unwrap()).unwrap().kind, page);
    }

    #[test]
    fn a_dead_terminals_workspace_goes_and_the_pages_one_comes_back() {
        let state = a_terminal_and_pages();
        assert!(has_anything(&state));
        // Session 0 didn't start again: its workspace goes, the focus goes to the pages'.
        let model = Model::from_state(&state, &[None]);
        model.check().unwrap();
        assert!(shows_anything(&model, false), "no plain session instead");
        let page = TabKind::Browser {
            url: Some("https://docs.rs/gpui".into()),
        };
        assert_eq!(shown(&model), [(Some("docs".into()), page.clone())]);
        assert_eq!(model.tab(model.focused_tab().unwrap()).unwrap().kind, page);
        // A file with nothing in it shows nothing: then one plain session opens.
        assert!(!has_anything(&SavedState::default()));
        assert!(!shows_anything(
            &Model::from_state(&SavedState::default(), &[]),
            false
        ));
    }

    #[test]
    fn a_file_of_file_tabs_only_comes_back_with_its_paths() {
        // The terminal tab closed before quitting, a file tab stayed: no session is saved.
        let mut state = a_terminal_and_pages();
        state.sessions.clear();
        state.workspaces.remove(0);
        if let Node::Leaf(pane) = &mut state.workspaces[0].layout {
            pane.tabs = vec![SavedTab::File {
                path: "/w/api/src/main.rs".into(),
                mode: crate::layout_model::FileMode::Plain,
            }];
        }
        state.active_workspace = Some(0);
        let text = encode(&state);
        assert!(text.contains("\"kind\": \"file\""), "{text}");
        let state = decode(&text).expect("it loads");
        assert!(has_anything(&state));
        let model = Model::from_state(&state, &[]);
        model.check().unwrap();
        assert!(shows_anything(&model, false), "no plain session instead");
        let file = TabKind::File {
            path: "/w/api/src/main.rs".into(),
            mode: crate::layout_model::FileMode::Plain,
        };
        assert_eq!(shown(&model), [(Some("docs".into()), file.clone())]);
        assert_eq!(model.tab(model.focused_tab().unwrap()).unwrap().kind, file);
    }

    fn r(value: &str) -> Entry {
        Entry {
            key: "session-restore".into(),
            value: value.into(),
        }
    }
    /// A version-1 file (sessions and their layouts).
    fn sample() -> SavedState {
        SavedState {
            version: 1,
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
                    color: Some(TabColor::Teal),
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
        assert_eq!(decode("{\"version\": 3}"), None);
        assert_eq!(decode("{\"version\": 0}"), None);
    }

    #[test]
    fn files_hold_workspaces_now_and_older_ones_still_load() {
        assert_eq!(VERSION, 2);
        assert_eq!(SavedState::default().version, 2);
        // Version 2: workspaces, panes and tabs, and which workspace is focused.
        let state = SavedState {
            workspaces: vec![SavedWorkspace {
                name: Some("api".into()),
                color: Some(TabColor::Teal),
                pinned: true,
                placed: None,
                layout: Node::Leaf(SavedPane {
                    tabs: vec![SavedTab::Terminal { session: 1 }],
                    active: 0,
                }),
                focused: 0,
            }],
            active_workspace: Some(0),
            layouts: Vec::new(),
            version: VERSION,
            ..sample()
        };
        let text = encode(&state);
        assert_eq!(decode(&text), Some(state));
        assert!(text.contains("\"version\": 2"), "{text}");
        // A version-2 file has no old layouts.
        assert!(!text.contains("\"layouts\""), "{text}");
        // Version 1 still loads, with no workspaces (its layouts are read instead).
        let old = decode(r#"{"version": 1, "sessions": [{}], "layouts": [{"leaf": 0}]}"#).unwrap();
        assert_eq!(old.version, 1);
        assert!(old.workspaces.is_empty() && old.active_workspace.is_none());
        assert_eq!(old.layouts, vec![Node::Leaf(0)]);
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
    fn browser_data_flag_round_trips_and_old_files_load() {
        let state = SavedState {
            browser_data_kept: true,
            ..sample()
        };
        assert_eq!(decode(&encode(&state)), Some(state));
        let old = decode(r#"{"version":1,"sessions":[{}]}"#).expect("an old file loads");
        assert!(!old.browser_data_kept);
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
    fn the_right_sidebar_round_trips_and_a_file_without_it_starts_it_closed() {
        let state = SavedState {
            right_sidebar_hidden: false,
            right_sidebar_width: Some(410.0),
            ..sample()
        };
        assert_eq!(decode(&encode(&state)), Some(state));
        // A file from before M11g: the right sidebar closed, at its default width.
        let old = decode(r#"{"version":2,"sessions":[{}]}"#).expect("an old file loads");
        assert!(old.right_sidebar_hidden);
        assert_eq!(old.right_sidebar_width, None);
        assert!(SavedState::default().right_sidebar_hidden);
    }

    #[test]
    fn the_right_sidebar_panel_round_trips_and_an_old_file_shows_files() {
        let state = SavedState {
            right_sidebar_panel: Some("changes".into()),
            ..sample()
        };
        let text = encode(&state);
        assert!(text.contains("\"right_sidebar_panel\""), "{text}");
        assert_eq!(decode(&text), Some(state));
        // A file from before M11h: no panel, so Files; and none isn't written.
        let old = decode(r#"{"version":2,"sessions":[{}]}"#).expect("an old file loads");
        assert_eq!(old.right_sidebar_panel, None);
        assert_eq!(
            crate::right_sidebar::Panel::from_key(old.right_sidebar_panel.as_deref()),
            crate::right_sidebar::Panel::Files
        );
        assert!(!encode(&SavedState::default()).contains("right_sidebar_panel"));
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
    fn a_tab_color_round_trips_and_old_or_odd_files_load() {
        let state = decode(&encode(&sample())).unwrap();
        assert_eq!(state.sessions[1].color, Some(TabColor::Teal));
        assert!(encode(&sample()).contains("\"color\": \"teal\""));
        // None isn't written.
        assert_eq!(state.sessions[0].color, None);
        assert!(!encode(&SavedState::default()).contains("color"));
        // A file from before tab colours: none.
        let old = decode(r#"{"version": 1, "sessions": [{"name": "a"}]}"#).unwrap();
        assert_eq!(old.sessions[0].color, None);
        // A colour this fuxx doesn't know, or not a name: no colour, and the rest still loads.
        let odd = decode(
            r#"{"version": 1, "sessions": [{"name": "a", "color": "brown"}, {"color": 3}]}"#,
        )
        .expect("still loads");
        assert_eq!(odd.sessions[0].name.as_deref(), Some("a"));
        assert_eq!((odd.sessions[0].color, odd.sessions[1].color), (None, None));
        // Restored as saved.
        let back = launches(&sample(), RestoreMode::Layout, &builtins());
        assert_eq!(back[1].saved.color, Some(TabColor::Teal));
    }

    #[test]
    fn state_path_is_in_application_support() {
        assert_eq!(
            state_path(Path::new("/Users/me")),
            PathBuf::from("/Users/me/Library/Application Support/fuxx/sessions.json")
        );
    }

    #[test]
    fn an_older_sessions_file_counts_as_welcomed() {
        assert!(decode(r#"{"version":2}"#).unwrap().welcomed);
    }

    #[test]
    fn a_fresh_state_is_not_welcomed() {
        assert!(!SavedState::default().welcomed);
    }

    #[test]
    fn welcomed_survives_a_round_trip() {
        let state = SavedState {
            welcomed: false,
            ..Default::default()
        };
        assert!(!decode(&encode(&state)).unwrap().welcomed);
        let state = SavedState {
            welcomed: true,
            ..Default::default()
        };
        assert!(decode(&encode(&state)).unwrap().welcomed);
    }

    #[test]
    fn welcome_due_only_without_a_record() {
        assert!(welcome_due(None));
        assert!(welcome_due(Some(&SavedState::default())));
        assert!(!welcome_due(Some(&SavedState {
            welcomed: true,
            ..Default::default()
        })));
    }
}
