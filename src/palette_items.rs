//! The ⌘K palette's keys, scopes and Recent list, its sources (every action, setting and
//! workspace) and `compose`, which turns them and the query into the list's lines (M11j). Pure.

use std::path::{Path, PathBuf};

use std::cmp::Reverse;

use crate::help_shortcuts::{ShortcutGroup, key_label, keys_for};
use crate::palette_match::{LOOSE, NUMBER, fuzzy, loose};
use crate::preferences::{PaletteSetting, Section};
use crate::switcher::{Entry, urgency_order};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Workspace,
    Action,
    File,
    Setting,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Key {
    Workspace(u64),
    Action(&'static str),
    Preset(usize),
    File(PathBuf),
    Setting(&'static str),
    More(Kind),
}

impl Key {
    pub fn kind(&self) -> Kind {
        match self {
            Key::Workspace(_) => Kind::Workspace,
            Key::Action(_) | Key::Preset(_) => Kind::Action,
            Key::File(_) => Kind::File,
            Key::Setting(_) => Kind::Setting,
            Key::More(k) => *k,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Scope {
    #[default]
    All,
    Actions,
    Workspaces,
    Files,
    Settings,
}

impl Scope {
    pub const ALL: [Scope; 5] = [
        Scope::All,
        Scope::Actions,
        Scope::Workspaces,
        Scope::Files,
        Scope::Settings,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Scope::All => "All",
            Scope::Actions => "Actions",
            Scope::Workspaces => "Workspaces",
            Scope::Files => "Files",
            Scope::Settings => "Settings",
        }
    }
    pub fn placeholder(self) -> &'static str {
        match self {
            Scope::All => "Search actions, workspaces, files and settings",
            Scope::Actions => "Run an action",
            Scope::Workspaces => "Go to a workspace \u{2014} number, name, group or question",
            Scope::Files => "Open a file",
            Scope::Settings => "Find a setting",
        }
    }
    fn index(self) -> usize {
        Scope::ALL.iter().position(|s| *s == self).unwrap_or(0)
    }
    pub fn next(self) -> Scope {
        Scope::ALL[(self.index() + 1) % Scope::ALL.len()]
    }
    pub fn prev(self) -> Scope {
        Scope::ALL[(self.index() + Scope::ALL.len() - 1) % Scope::ALL.len()]
    }
    pub fn kind(self) -> Option<Kind> {
        match self {
            Scope::All => None,
            Scope::Actions => Some(Kind::Action),
            Scope::Workspaces => Some(Kind::Workspace),
            Scope::Files => Some(Kind::File),
            Scope::Settings => Some(Kind::Setting),
        }
    }
    pub fn of(kind: Kind) -> Scope {
        match kind {
            Kind::Action => Scope::Actions,
            Kind::Workspace => Scope::Workspaces,
            Kind::File => Scope::Files,
            Kind::Setting => Scope::Settings,
        }
    }
}

/// On All only, a query starting with `>` searches Actions (the rest trimmed at its start).
pub fn parse_query(scope: Scope, text: &str) -> (Scope, &str) {
    match (scope, text.strip_prefix('>')) {
        (Scope::All, Some(rest)) => (Scope::Actions, rest.trim_start()),
        _ => (scope, text),
    }
}

/// The box's text once a scope is chosen (a chip, Tab, ⇧Tab, ⌘P, a `… more` row), when it
/// changes: a `>` typed on All was the Actions shortcut, so choosing a scope replaces it and the
/// `>` goes (with the spaces after it). None: the text is kept as typed.
pub fn text_for_scope(base: Scope, text: &str) -> Option<&str> {
    match (base, text.strip_prefix('>')) {
        (Scope::All, Some(rest)) => Some(rest.trim_start()),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Open,
    Close,
    Rescope,
}

/// Closed: open. Open on the asked scope: close. Open on another: rescope (text kept).
pub fn palette_step(open: Option<Scope>, asked: Scope) -> Step {
    match open {
        None => Step::Open,
        Some(s) if s == asked => Step::Close,
        Some(_) => Step::Rescope,
    }
}

pub const RECENT_MAX: usize = 20;
pub const RECENT_SHOWN: usize = 5;

#[derive(Clone, Default, Debug)]
pub struct Recent {
    keys: Vec<Key>,
}

impl Recent {
    pub fn push(&mut self, key: Key) {
        if matches!(key, Key::More(_)) {
            return;
        }
        self.keys.retain(|k| *k != key);
        self.keys.insert(0, key);
        self.keys.truncate(RECENT_MAX);
    }
    pub fn iter(&self) -> impl Iterator<Item = &Key> {
        self.keys.iter()
    }
    pub fn rank(&self, key: &Key) -> Option<usize> {
        self.keys.iter().position(|k| k == key)
    }
}

/// Where an action works: anywhere, or only with that kind of tab focused when the palette
/// opened (`Persistent`: anywhere, while persistent sessions are on).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    Anywhere,
    Terminal,
    File,
    Browser,
    TerminalOrFile,
    Persistent,
}

/// The kind of tab that had the focus when the palette opened.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Focused {
    Terminal,
    Browser,
    File,
    Nothing,
}

pub fn applies(place: Place, focused: Focused, persistent: bool) -> bool {
    match place {
        Place::Anywhere => true,
        Place::Terminal => focused == Focused::Terminal,
        Place::File => focused == Focused::File,
        Place::Browser => focused == Focused::Browser,
        Place::TerminalOrFile => matches!(focused, Focused::Terminal | Focused::File),
        Place::Persistent => persistent,
    }
}

/// How a command runs: its GPUI action, dispatched as its key would be, or New Browser Tab (the
/// ⋯ menu's method on the focused pane).
#[derive(Clone, Copy)]
pub enum Run {
    Action(fn() -> Box<dyn gpui::Action>),
    NewBrowserTab,
}

pub struct Command {
    pub name: &'static str,
    pub group: ShortcutGroup,
    pub keywords: &'static str,
    pub place: Place,
    pub run: Run,
}

fn act<A: gpui::Action + Default>() -> Box<dyn gpui::Action> {
    Box::new(A::default())
}

const fn cmd(
    name: &'static str,
    group: ShortcutGroup,
    keywords: &'static str,
    place: Place,
    run: Run,
) -> Command {
    Command {
        name,
        group,
        keywords,
        place,
        run,
    }
}

/// The agent presets' `Start` rows come right after this one.
const OPEN_BROWSER: &str = "Open Browser\u{2026}";

/// Every palette action, in its order (the agent presets' rows come from `action_rows`).
pub static COMMANDS: &[Command] = {
    use crate::browser_tab::{FocusAddress, GoBack, GoForward, Reload, ResetZoom, ZoomIn, ZoomOut};
    use crate::file_tab::{Find, FindNext, FindPrev};
    use crate::terminal_view::{
        Copy, CopyCommand, CopyCommandAndOutput, CopyCommandOutput, NextCommand, Paste,
        PrevCommand, RerunCommand, ScrollPageDown, ScrollPageUp,
    };
    use crate::workspace::*;
    use crate::{
        ActualSize, Bigger, CheckForUpdates, Hide, Minimize, OpenPreferences, Quit,
        QuitAndEndSessions, ReportIssue, ShowCommands, ShowLogs, ShowNotificationStatus,
        ShowShortcuts, ShowWelcome, ShowWelcomeTips, Smaller, Zoom,
    };
    use Place::{Anywhere as A, Browser as B, File as F, Persistent as P, Terminal as T};
    use ShortcutGroup::*;
    const TF: Place = Place::TerminalOrFile;
    &[
        cmd(
            "New Workspace",
            Workspaces,
            "",
            A,
            Run::Action(act::<NewWorkspace>),
        ),
        cmd(
            "New Workspace in Folder\u{2026}",
            Workspaces,
            "open folder",
            A,
            Run::Action(act::<NewSessionInFolder>),
        ),
        cmd(
            "New SSH Workspace\u{2026}",
            Workspaces,
            "ssh host remote",
            A,
            Run::Action(act::<NewSshSession>),
        ),
        cmd(
            "Rename Workspace\u{2026}",
            Workspaces,
            "",
            A,
            Run::Action(act::<RenameSession>),
        ),
        cmd(
            "Move Workspace Up",
            Workspaces,
            "",
            A,
            Run::Action(act::<MoveSessionUp>),
        ),
        cmd(
            "Move Workspace Down",
            Workspaces,
            "",
            A,
            Run::Action(act::<MoveSessionDown>),
        ),
        cmd(
            "Next Workspace",
            Workspaces,
            "",
            A,
            Run::Action(act::<NextSession>),
        ),
        cmd(
            "Previous Workspace",
            Workspaces,
            "",
            A,
            Run::Action(act::<PrevSession>),
        ),
        cmd(
            "Go to What Needs You",
            Workspaces,
            "waiting done urgent",
            A,
            Run::Action(act::<NextNeedingYou>),
        ),
        cmd(
            OPEN_BROWSER,
            Workspaces,
            "web url address",
            A,
            Run::Action(act::<OpenBrowser>),
        ),
        cmd("New Tab", Tabs, "", A, Run::Action(act::<NewTab>)),
        cmd("New Browser Tab", Tabs, "web", A, Run::NewBrowserTab),
        cmd("Close Tab", Tabs, "", A, Run::Action(act::<CloseSession>)),
        cmd("Next Tab", Tabs, "", A, Run::Action(act::<NextTab>)),
        cmd("Previous Tab", Tabs, "", A, Run::Action(act::<PrevTab>)),
        cmd(
            "Find in File\u{2026}",
            Tabs,
            "search",
            F,
            Run::Action(act::<Find>),
        ),
        cmd("Find Next", Tabs, "", F, Run::Action(act::<FindNext>)),
        cmd("Find Previous", Tabs, "", F, Run::Action(act::<FindPrev>)),
        cmd("Split Right", Panes, "", A, Run::Action(act::<SplitRight>)),
        cmd("Split Down", Panes, "", A, Run::Action(act::<SplitDown>)),
        cmd("Previous Pane", Panes, "", A, Run::Action(act::<PrevPane>)),
        cmd("Next Pane", Panes, "", A, Run::Action(act::<NextPane>)),
        cmd(
            "Focus Pane Left",
            Panes,
            "",
            A,
            Run::Action(act::<FocusPaneLeft>),
        ),
        cmd(
            "Focus Pane Right",
            Panes,
            "",
            A,
            Run::Action(act::<FocusPaneRight>),
        ),
        cmd(
            "Focus Pane Up",
            Panes,
            "",
            A,
            Run::Action(act::<FocusPaneUp>),
        ),
        cmd(
            "Focus Pane Down",
            Panes,
            "",
            A,
            Run::Action(act::<FocusPaneDown>),
        ),
        cmd(
            "Equalize Panes",
            Panes,
            "",
            A,
            Run::Action(act::<EqualizePanes>),
        ),
        cmd("Copy", Terminal, "", TF, Run::Action(act::<Copy>)),
        cmd("Paste", Terminal, "", T, Run::Action(act::<Paste>)),
        cmd(
            "Scroll Up a Page",
            Terminal,
            "",
            T,
            Run::Action(act::<ScrollPageUp>),
        ),
        cmd(
            "Scroll Down a Page",
            Terminal,
            "",
            T,
            Run::Action(act::<ScrollPageDown>),
        ),
        // Command blocks (M11l): the chosen block, else the newest finished one.
        cmd(
            "Previous Command",
            Terminal,
            "block prompt jump",
            T,
            Run::Action(act::<PrevCommand>),
        ),
        cmd(
            "Next Command",
            Terminal,
            "block prompt jump",
            T,
            Run::Action(act::<NextCommand>),
        ),
        cmd(
            "Copy Command Output",
            Terminal,
            "block copy",
            T,
            Run::Action(act::<CopyCommandOutput>),
        ),
        cmd(
            "Copy Command",
            Terminal,
            "block copy",
            T,
            Run::Action(act::<CopyCommand>),
        ),
        cmd(
            "Copy Command and Output",
            Terminal,
            "block copy",
            T,
            Run::Action(act::<CopyCommandAndOutput>),
        ),
        cmd(
            "Rerun Command",
            Terminal,
            "block run again",
            T,
            Run::Action(act::<RerunCommand>),
        ),
        cmd(
            "Go to Address Field",
            BrowserPages,
            "",
            B,
            Run::Action(act::<FocusAddress>),
        ),
        cmd(
            "Reload Page",
            BrowserPages,
            "",
            B,
            Run::Action(act::<Reload>),
        ),
        cmd("Page Back", BrowserPages, "", B, Run::Action(act::<GoBack>)),
        cmd(
            "Page Forward",
            BrowserPages,
            "",
            B,
            Run::Action(act::<GoForward>),
        ),
        cmd(
            "Zoom Page In",
            BrowserPages,
            "",
            B,
            Run::Action(act::<ZoomIn>),
        ),
        cmd(
            "Zoom Page Out",
            BrowserPages,
            "",
            B,
            Run::Action(act::<ZoomOut>),
        ),
        cmd(
            "Page Actual Size",
            BrowserPages,
            "",
            B,
            Run::Action(act::<ResetZoom>),
        ),
        cmd(
            "Toggle Sidebar",
            WindowAndApp,
            "",
            A,
            Run::Action(act::<ToggleSidebar>),
        ),
        cmd(
            "Toggle Right Sidebar",
            WindowAndApp,
            "",
            A,
            Run::Action(act::<ToggleRightSidebar>),
        ),
        cmd(
            "Bigger Text",
            WindowAndApp,
            "font size zoom",
            A,
            Run::Action(act::<Bigger>),
        ),
        cmd(
            "Smaller Text",
            WindowAndApp,
            "font size zoom",
            A,
            Run::Action(act::<Smaller>),
        ),
        cmd(
            "Actual Text Size",
            WindowAndApp,
            "font size zoom",
            A,
            Run::Action(act::<ActualSize>),
        ),
        cmd(
            "Minimize",
            WindowAndApp,
            "",
            A,
            Run::Action(act::<Minimize>),
        ),
        cmd("Zoom Window", WindowAndApp, "", A, Run::Action(act::<Zoom>)),
        cmd("Hide fuxx", WindowAndApp, "", A, Run::Action(act::<Hide>)),
        cmd(
            "Preferences\u{2026}",
            WindowAndApp,
            "settings",
            A,
            Run::Action(act::<OpenPreferences>),
        ),
        cmd(
            "Keyboard Shortcuts",
            WindowAndApp,
            "keys help",
            A,
            Run::Action(act::<ShowShortcuts>),
        ),
        cmd(
            "fuxx Command Reference",
            WindowAndApp,
            "cli help",
            A,
            Run::Action(act::<ShowCommands>),
        ),
        cmd(
            "Notifications Status\u{2026}",
            WindowAndApp,
            "",
            A,
            Run::Action(act::<ShowNotificationStatus>),
        ),
        cmd(
            "Welcome to fuxx",
            WindowAndApp,
            "first run start agent folder intro",
            A,
            Run::Action(act::<ShowWelcome>),
        ),
        cmd(
            "Show Tips Card",
            WindowAndApp,
            "welcome tips",
            A,
            Run::Action(act::<ShowWelcomeTips>),
        ),
        cmd(
            "Report an Issue\u{2026}",
            WindowAndApp,
            "bug",
            A,
            Run::Action(act::<ReportIssue>),
        ),
        cmd(
            "Show Logs",
            WindowAndApp,
            "",
            A,
            Run::Action(act::<ShowLogs>),
        ),
        cmd(
            "Check for Updates\u{2026}",
            WindowAndApp,
            "",
            A,
            Run::Action(act::<CheckForUpdates>),
        ),
        cmd("Quit fuxx", WindowAndApp, "", A, Run::Action(act::<Quit>)),
        cmd(
            "Quit fuxx and End All Sessions",
            WindowAndApp,
            "",
            P,
            Run::Action(act::<QuitAndEndSessions>),
        ),
    ]
};

/// Bound or menu actions left out of the palette on purpose: ⌘1–⌘9 (the workspaces are rows of
/// their own), each preset's key (a `Start` row per preset instead), the palette's own keys, and
/// Cut and Select All (nothing in fuxx's window answers them).
#[cfg(test)] // the parity test's
pub const NOT_IN_PALETTE: &[&str] = &[
    "workspace::SelectSession1",
    "workspace::SelectSession2",
    "workspace::SelectSession3",
    "workspace::SelectSession4",
    "workspace::SelectSession5",
    "workspace::SelectSession6",
    "workspace::SelectSession7",
    "workspace::SelectSession8",
    "workspace::SelectSession9",
    "workspace::LaunchPreset1",
    "workspace::LaunchPreset2",
    "workspace::LaunchPreset3",
    "workspace::LaunchPreset4",
    "workspace::LaunchPreset5",
    "workspace::LaunchPreset6",
    "workspace::LaunchPreset7",
    "workspace::LaunchPreset8",
    "workspace::LaunchPreset9",
    "workspace::OpenSwitcher",
    "workspace::OpenPalette",
    "edit::Cut",
    "edit::SelectAll",
];

/// ⋯ menu items (`pane_actions`) the palette lists under another name: the same action.
#[cfg(test)] // the parity test's
pub const PANE_MENU_AS: &[(&str, &str)] = &[("New Terminal Tab", "New Tab")];
/// ⋯ menu items left out of the palette on purpose (none: each is an entry).
#[cfg(test)] // the parity test's
pub const PANE_MENU_LEFT_OUT: &[&str] = &[];

/// One action as the palette lists it: `rest` is what a loose match searches (its keywords, its
/// group's label and its keys).
#[derive(Clone, Debug, PartialEq)]
pub struct ActionRow {
    pub key: Key,
    pub name: String,
    pub group: ShortcutGroup,
    pub keys: Option<String>,
    pub rest: String,
}

/// The actions that apply where the focus is, in table order, each agent preset's `Start` row
/// right after `Open Browser…`, keys from `pairs` (the binding table).
pub fn action_rows(
    focused: Focused,
    persistent: bool,
    presets: &[String],
    pairs: &[(String, String)],
) -> Vec<ActionRow> {
    let mut rows = Vec::new();
    for c in COMMANDS {
        if !applies(c.place, focused, persistent) {
            continue;
        }
        let keys = match c.run {
            Run::Action(make) => keys_for(pairs, make().name()),
            Run::NewBrowserTab => None,
        };
        rows.push(action_row(
            Key::Action(c.name),
            c.name.to_string(),
            c.group,
            c.keywords,
            keys,
        ));
        if c.name == OPEN_BROWSER {
            // Only the first nine presets have a key (⌃⌘1…⌃⌘9), and so a row.
            for (n, preset) in presets.iter().take(9).enumerate() {
                let keys = keys_for(pairs, &format!("workspace::LaunchPreset{}", n + 1));
                rows.push(action_row(
                    Key::Preset(n),
                    format!("Start {preset}"),
                    ShortcutGroup::Workspaces,
                    "agent preset",
                    keys,
                ));
            }
        }
    }
    rows
}

fn action_row(
    key: Key,
    name: String,
    group: ShortcutGroup,
    keywords: &str,
    keys: Option<String>,
) -> ActionRow {
    let rest = [keywords, group.label(), keys.as_deref().unwrap_or("")]
        .into_iter()
        .filter(|t| !t.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    ActionRow {
        key,
        name,
        group,
        keys,
        rest,
    }
}

pub const NOTHING: &str = "Nothing matches";
pub const LISTING: &str = "Listing files\u{2026}";
pub const SSH_NOTE: &str = "Files aren't searched for SSH terminals.";
pub const NO_FOLDER: &str = "No terminal folder to search.";
pub const FAILED: &str = "Files couldn't be listed.";
pub const CAPPED: &str = "Only the first 50,000 files are searched.";
/// The same, for a folder that isn't a repo (its walk is bounded sooner).
pub const CAPPED_WALK: &str = "Only part of this folder is searched.";
pub const HINT: &str =
    "Enter open \u{b7} \u{2191}\u{2193} choose \u{b7} Tab scope \u{b7} Esc close";
/// The most rows a section shows on All before its `… more` row.
pub const SECTION_CAP: usize = 5;
/// The most files the Files scope lists.
pub const FILE_RESULTS: usize = 200;

/// A file that matched the query: its path from the root, the matched chars, its score.
#[derive(Clone, Debug, PartialEq)]
pub struct FileHit {
    pub rel: String,
    pub positions: Vec<usize>,
    pub score: i32,
}

/// What the palette has of the files: a note in their place, a first listing running, a failed
/// one, or the ranked hits for the query (`total` matched; `capped`: the listing was cut short).
pub enum Files<'a> {
    Note(&'static str),
    Listing,
    Failed,
    Ready {
        root: &'a Path,
        root_name: &'a str,
        hits: &'a [FileHit],
        total: usize,
        capped: bool,
        /// The listing came from the walk (not git): a cut-short one says so differently.
        walked: bool,
    },
}

/// One line of the list. Items and `More` are selectable; headings and notes are not.
#[derive(Clone, Debug, PartialEq)]
pub enum Line {
    Header(String),
    Item { key: Key, positions: Vec<usize> },
    More { kind: Kind, count: usize },
    Note(String),
}

/// What `compose` lists from. `scope` and `query` have been through `parse_query`; the
/// workspaces are in display order (so `#n` is a workspace's place + 1). `valid` says whether a
/// Recent key still stands for something (it filters Recent only).
pub struct Sources<'a> {
    pub scope: Scope,
    pub query: &'a str,
    pub workspaces: &'a [Entry],
    pub actions: &'a [ActionRow],
    pub settings: &'a [PaletteSetting],
    pub files: Files<'a>,
    pub recent: &'a Recent,
    pub valid: &'a dyn Fn(&Key) -> bool,
}

/// One matching item before it becomes a line: `order` is its place in its kind's own order.
struct Found {
    key: Key,
    positions: Vec<usize>,
    score: i32,
    order: usize,
}

impl Found {
    fn line(self) -> Line {
        Line::Item {
            key: self.key,
            positions: self.positions,
        }
    }
}

/// How `query` matches an item: by its main text (fuzzy, highlighted); else loosely, every word
/// somewhere in the main text or `rest`, or the query typed as keys found in `keys` (`LOOSE`).
fn match_item(
    query: &str,
    main: &str,
    rest: &[&str],
    keys: Option<&str>,
) -> Option<(i32, Vec<usize>)> {
    if let Some(m) = fuzzy(query, main) {
        return Some((m.score, m.positions));
    }
    let mut texts = vec![main];
    texts.extend_from_slice(rest);
    let typed_keys = || {
        let typed = key_label(&query.split_whitespace().collect::<Vec<_>>().join("-"));
        keys.is_some_and(|k| k.to_lowercase().contains(&typed.to_lowercase()))
    };
    (loose(query, &texts) || typed_keys()).then(|| (LOOSE, Vec::new()))
}

/// Score first; while typing, recently picked next (newest first); then the kind's own order.
fn rank(found: &mut [Found], recent: Option<&Recent>) {
    found.sort_by_key(|f| {
        let picked = recent.and_then(|r| r.rank(&f.key)).unwrap_or(usize::MAX);
        (Reverse(f.score), picked, f.order)
    });
}

/// The workspaces that match, in urgency order: a workspace's number (`3`, `#3`) puts it first.
fn find_workspaces(s: &Sources) -> Vec<Found> {
    let facts: Vec<_> = s.workspaces.iter().map(|e| (e.status, e.since)).collect();
    let typed = s.query.trim();
    let number = typed.strip_prefix('#').unwrap_or(typed);
    urgency_order(&facts)
        .into_iter()
        .enumerate()
        .filter_map(|(order, i)| {
            let e = &s.workspaces[i];
            let (score, positions) = if !typed.is_empty() && number == (i + 1).to_string() {
                (NUMBER, Vec::new())
            } else {
                let rest: Vec<&str> = [e.group.as_deref(), e.question.as_deref()]
                    .into_iter()
                    .flatten()
                    .collect();
                match_item(s.query, &e.label, &rest, None)?
            };
            Some(Found {
                key: Key::Workspace(e.id),
                positions,
                score,
                order,
            })
        })
        .collect()
}

fn find_actions(s: &Sources) -> Vec<Found> {
    s.actions
        .iter()
        .enumerate()
        .filter_map(|(order, row)| {
            let (score, positions) =
                match_item(s.query, &row.name, &[&row.rest], row.keys.as_deref())?;
            Some(Found {
                key: row.key.clone(),
                positions,
                score,
                order,
            })
        })
        .collect()
}

fn find_settings(s: &Sources) -> Vec<Found> {
    s.settings
        .iter()
        .enumerate()
        .filter_map(|(order, setting)| {
            let mut rest = vec![setting.section.title()];
            rest.extend(&setting.keywords);
            let (score, positions) = match_item(s.query, setting.name, &rest, None)?;
            Some(Found {
                key: Key::Setting(setting.name),
                positions,
                score,
                order,
            })
        })
        .collect()
}

/// The files' hits as found items, keyed by their full path, in the ranking's order.
fn find_files(root: &Path, hits: &[FileHit]) -> Vec<Found> {
    hits.iter()
        .enumerate()
        .map(|(order, hit)| Found {
            key: Key::File(root.join(&hit.rel)),
            positions: hit.positions.clone(),
            score: hit.score,
            order,
        })
        .collect()
}

/// Up to `RECENT_SHOWN` Recent keys (of `kind`, or any) still valid and still in the sources (a
/// file's by `valid` alone: the hits are there only while typing).
fn recent_shown(s: &Sources, kind: Option<Kind>) -> Vec<Key> {
    let present = |key: &Key| match key {
        Key::Workspace(id) => s.workspaces.iter().any(|e| e.id == *id),
        Key::Action(_) | Key::Preset(_) => s.actions.iter().any(|r| r.key == *key),
        Key::Setting(name) => s.settings.iter().any(|x| x.name == *name),
        Key::File(_) => true,
        Key::More(_) => false,
    };
    s.recent
        .iter()
        .filter(|k| kind.is_none_or(|kind| k.kind() == kind))
        .filter(|k| (s.valid)(k) && present(k))
        .take(RECENT_SHOWN)
        .cloned()
        .collect()
}

fn item_line(key: Key) -> Line {
    Line::Item {
        key,
        positions: Vec::new(),
    }
}

fn push_recent(lines: &mut Vec<Line>, recent: &[Key]) {
    if !recent.is_empty() {
        lines.push(Line::Header("Recent".into()));
        lines.extend(recent.iter().cloned().map(item_line));
    }
}

/// A section on All: its heading, at most `SECTION_CAP` items, then `… N more` (`total`: how many
/// matched, the files' count past the hits included).
fn push_capped(lines: &mut Vec<Line>, title: String, kind: Kind, found: Vec<Found>, total: usize) {
    if found.is_empty() {
        return;
    }
    lines.push(Line::Header(title));
    let shown = found.len().min(SECTION_CAP);
    lines.extend(found.into_iter().take(SECTION_CAP).map(Found::line));
    if total > shown {
        lines.push(Line::More {
            kind,
            count: total - shown,
        });
    }
}

fn kind_title(kind: Kind, root_name: &str) -> String {
    match kind {
        Kind::Workspace => "Workspaces".into(),
        Kind::Action => "Actions".into(),
        Kind::File => format!("Files in {root_name}"),
        Kind::Setting => "Settings".into(),
    }
}

/// The list's lines for the sources and the query (the plan's Global Constraints, Lists).
pub fn compose(s: &Sources) -> Vec<Line> {
    let typing = !s.query.trim().is_empty();
    match (s.scope.kind(), typing) {
        (None, false) => compose_all_empty(s),
        (None, true) => compose_all_typed(s),
        (Some(kind), false) => compose_kind_empty(s, kind),
        (Some(kind), true) => compose_kind_typed(s, kind),
    }
}

/// Nothing typed on All: Recent, then Workspaces and Actions, each capped, Recent's items left
/// out of them.
fn compose_all_empty(s: &Sources) -> Vec<Line> {
    let recent = recent_shown(s, None);
    let mut lines = Vec::new();
    push_recent(&mut lines, &recent);
    for (kind, mut found) in [
        (Kind::Workspace, find_workspaces(s)),
        (Kind::Action, find_actions(s)),
    ] {
        found.retain(|f| !recent.contains(&f.key));
        rank(&mut found, None);
        let total = found.len();
        push_capped(&mut lines, kind_title(kind, ""), kind, found, total);
    }
    lines
}

/// Typing on All: a capped section per kind that matched, the best match's section first (ties:
/// Workspaces, Actions, Files, Settings). Files only once there are hits, the cap note after.
fn compose_all_typed(s: &Sources) -> Vec<Line> {
    let mut sections: Vec<(Kind, Vec<Found>, usize)> = Vec::new();
    for kind in [Kind::Workspace, Kind::Action, Kind::File, Kind::Setting] {
        let (mut found, total) = match (kind, &s.files) {
            (Kind::Workspace, _) => (find_workspaces(s), 0),
            (Kind::Action, _) => (find_actions(s), 0),
            (Kind::Setting, _) => (find_settings(s), 0),
            (
                Kind::File,
                Files::Ready {
                    root, hits, total, ..
                },
            ) => (find_files(root, hits), *total),
            (Kind::File, _) => continue,
        };
        if found.is_empty() {
            continue;
        }
        rank(&mut found, Some(s.recent));
        let total = total.max(found.len());
        sections.push((kind, found, total));
    }
    // A stable sort keeps the tie order above.
    sections.sort_by_key(|(_, found, _)| Reverse(found[0].score));
    let (root_name, capped) = match s.files {
        Files::Ready {
            root_name, capped, ..
        } => (root_name, capped),
        _ => ("", false),
    };
    let mut lines = Vec::new();
    for (kind, found, total) in sections {
        push_capped(&mut lines, kind_title(kind, root_name), kind, found, total);
        if kind == Kind::File && capped {
            lines.push(cap_note(walked(&s.files)));
        }
    }
    lines
}

/// Nothing typed on a narrowed scope: Recent of that kind, then all of it (Workspaces by
/// urgency, Actions under their groups, Settings under their sections), or the files' note.
/// Workspaces (what ⌘P opens on) has no Recent: it is exactly the old ⌘P list.
fn compose_kind_empty(s: &Sources, kind: Kind) -> Vec<Line> {
    let recent = match kind {
        Kind::Workspace => Vec::new(),
        _ => recent_shown(s, Some(kind)),
    };
    let mut lines = Vec::new();
    push_recent(&mut lines, &recent);
    match kind {
        Kind::Workspace => {
            let mut found = find_workspaces(s);
            found.retain(|f| !recent.contains(&f.key));
            if !found.is_empty() {
                lines.push(Line::Header(kind_title(kind, "")));
                lines.extend(found.into_iter().map(Found::line));
            }
        }
        Kind::Action => {
            let mut rows: Vec<&ActionRow> = s
                .actions
                .iter()
                .filter(|r| !recent.contains(&r.key))
                .collect();
            rows.sort_by_key(|r| r.group);
            push_grouped(
                &mut lines,
                rows.iter().map(|r| (r.group.label(), r.key.clone())),
            );
        }
        Kind::Setting => {
            let mut settings: Vec<&PaletteSetting> = s
                .settings
                .iter()
                .filter(|x| !recent.contains(&Key::Setting(x.name)))
                .collect();
            settings.sort_by_key(|x| Section::ALL.iter().position(|&sec| sec == x.section));
            push_grouped(
                &mut lines,
                settings
                    .iter()
                    .map(|x| (x.section.title(), Key::Setting(x.name))),
            );
        }
        Kind::File => match s.files {
            Files::Ready {
                root_name, capped, ..
            } => {
                lines.push(Line::Note(format!("Type to search files in {root_name}")));
                if capped {
                    lines.push(cap_note(walked(&s.files)));
                }
            }
            _ => lines.extend(files_note(&s.files)),
        },
    }
    lines
}

/// The items, with a heading each time their heading changes.
fn push_grouped(lines: &mut Vec<Line>, items: impl Iterator<Item = (&'static str, Key)>) {
    let mut heading = None;
    for (title, key) in items {
        if heading != Some(title) {
            lines.push(Line::Header(title.into()));
            heading = Some(title);
        }
        lines.push(item_line(key));
    }
}

/// What the Files scope says when there is nothing to rank.
fn files_note(files: &Files) -> Option<Line> {
    let note = match files {
        Files::Note(note) => note,
        Files::Listing => LISTING,
        Files::Failed => FAILED,
        Files::Ready { .. } => return None,
    };
    Some(Line::Note(note.to_string()))
}

/// Typing on a narrowed scope: every match under the kind's heading (files: the best
/// `FILE_RESULTS`, then how many more, then the cap note), or the files' note.
fn compose_kind_typed(s: &Sources, kind: Kind) -> Vec<Line> {
    let (mut found, root_name, total, capped) = match (kind, &s.files) {
        (Kind::Workspace, _) => (find_workspaces(s), "", 0, false),
        (Kind::Action, _) => (find_actions(s), "", 0, false),
        (Kind::Setting, _) => (find_settings(s), "", 0, false),
        (
            Kind::File,
            Files::Ready {
                root,
                root_name,
                hits,
                total,
                capped,
                ..
            },
        ) => (find_files(root, hits), *root_name, *total, *capped),
        (Kind::File, files) => return files_note(files).into_iter().collect(),
    };
    if found.is_empty() {
        // A capped listing explains a miss: the file may be past the cap.
        return if capped {
            vec![cap_note(walked(&s.files))]
        } else {
            Vec::new()
        };
    }
    rank(&mut found, Some(s.recent));
    if kind == Kind::File {
        found.truncate(FILE_RESULTS);
    }
    let shown = found.len();
    let mut lines = vec![Line::Header(kind_title(kind, root_name))];
    lines.extend(found.into_iter().map(Found::line));
    if total > shown {
        lines.push(Line::Note(more_label(Kind::File, total - shown)));
    }
    if capped {
        lines.push(cap_note(walked(&s.files)));
    }
    lines
}

/// Whether the files listed came from the walk (not git).
fn walked(files: &Files) -> bool {
    matches!(files, Files::Ready { walked: true, .. })
}

/// The note under a listing that was cut short: the walk's (outside a repo) or git's cap.
fn cap_note(walked: bool) -> Line {
    Line::Note(if walked { CAPPED_WALK } else { CAPPED }.into())
}

/// The places in `lines` of the rows that can be chosen (items and `… more`).
pub fn selectable(lines: &[Line]) -> Vec<usize> {
    lines
        .iter()
        .enumerate()
        .filter(|(_, line)| matches!(line, Line::Item { .. } | Line::More { .. }))
        .map(|(i, _)| i)
        .collect()
}

/// `… 3 more workspaces`; `… 1 more file`.
pub fn more_label(kind: Kind, n: usize) -> String {
    let noun = match kind {
        Kind::Workspace => "workspace",
        Kind::Action => "action",
        Kind::File => "file",
        Kind::Setting => "setting",
    };
    let plural = if n == 1 { "" } else { "s" };
    format!("\u{2026} {n} more {noun}{plural}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_scopes_cycle_round() {
        assert_eq!(Scope::All.next(), Scope::Actions);
        assert_eq!(Scope::Settings.next(), Scope::All);
        assert_eq!(Scope::All.prev(), Scope::Settings);
        assert_eq!(Scope::Actions.prev(), Scope::All);
        for s in Scope::ALL {
            assert_eq!(s.next().prev(), s);
            assert!(!s.label().is_empty());
            assert!(!s.placeholder().is_empty());
            if let Some(k) = s.kind() {
                assert_eq!(Scope::of(k), s);
            }
        }
        assert_eq!(Scope::All.kind(), None);
        assert_eq!(Scope::Files.label(), "Files");
        assert_eq!(Scope::Files.placeholder(), "Open a file");
        assert_eq!(
            Scope::Workspaces.placeholder(),
            "Go to a workspace \u{2014} number, name, group or question"
        );
        assert_eq!(
            parse_query(Scope::All, "> split"),
            (Scope::Actions, "split")
        );
        assert_eq!(
            parse_query(Scope::Workspaces, ">x"),
            (Scope::Workspaces, ">x")
        );
        assert_eq!(parse_query(Scope::All, "split"), (Scope::All, "split"));
    }

    /// Choosing a scope with `>` typed on All (Actions shown): the `>` was the Actions shortcut,
    /// so it goes (with the spaces after it) and the rest of the text stays; otherwise the
    /// text is kept as typed.
    #[test]
    fn choosing_a_scope_drops_the_actions_prefix() {
        assert_eq!(text_for_scope(Scope::All, "> split"), Some("split"));
        assert_eq!(text_for_scope(Scope::All, ">"), Some(""));
        assert_eq!(text_for_scope(Scope::All, "split"), None);
        assert_eq!(text_for_scope(Scope::All, " >split"), None);
        // On a narrowed scope `>` is just a letter.
        assert_eq!(text_for_scope(Scope::Workspaces, ">x"), None);
        assert_eq!(text_for_scope(Scope::Actions, ">x"), None);
    }

    #[test]
    fn palette_step_toggles_and_rescopes() {
        assert_eq!(palette_step(None, Scope::All), Step::Open);
        assert_eq!(palette_step(Some(Scope::All), Scope::All), Step::Close);
        assert_eq!(
            palette_step(Some(Scope::All), Scope::Workspaces),
            Step::Rescope
        );
        assert_eq!(palette_step(Some(Scope::Files), Scope::All), Step::Rescope);
    }

    #[test]
    fn palette_key_kinds() {
        assert_eq!(Key::Workspace(1).kind(), Kind::Workspace);
        assert_eq!(Key::Preset(0).kind(), Kind::Action);
        assert_eq!(Key::File("a".into()).kind(), Kind::File);
        assert_eq!(Key::Setting("x").kind(), Kind::Setting);
        assert_eq!(Key::More(Kind::File).kind(), Kind::File);
    }

    #[test]
    fn palette_recent_keeps_the_last_twenty_newest_first() {
        let mut r = Recent::default();
        for i in 0..25 {
            r.push(Key::Workspace(i));
        }
        let all: Vec<_> = r.iter().cloned().collect();
        assert_eq!(all.len(), RECENT_MAX);
        assert_eq!(all[0], Key::Workspace(24));
        assert_eq!(all[19], Key::Workspace(5));
        assert_eq!(r.rank(&Key::Workspace(24)), Some(0));
        assert_eq!(r.rank(&Key::Workspace(0)), None);
        r.push(Key::Workspace(10));
        assert_eq!(r.iter().next(), Some(&Key::Workspace(10)));
        assert_eq!(r.iter().count(), RECENT_MAX);
        assert_eq!(r.rank(&Key::Workspace(24)), Some(1));
        r.push(Key::More(Kind::File));
        assert_eq!(r.iter().count(), RECENT_MAX);
        assert_eq!(r.rank(&Key::More(Kind::File)), None);
        assert_eq!(r.iter().next(), Some(&Key::Workspace(10)));
    }

    use crate::help_shortcuts::{Describe, binding_pairs, describe};
    use crate::preferences::palette_settings;
    use crate::status::Status;
    use std::collections::HashSet;
    use std::time::{Duration, Instant};

    /// The whole binding table, as `main` binds it with persistent sessions on.
    fn real_pairs() -> Vec<(String, String)> {
        let mut bindings = crate::new_bindings().to_vec();
        bindings.extend(crate::key_bindings());
        bindings.extend(crate::persistent_bindings());
        binding_pairs(&bindings)
    }

    fn action_name(run: Run) -> Option<String> {
        match run {
            Run::Action(make) => Some(make().name().to_string()),
            Run::NewBrowserTab => None,
        }
    }

    #[test]
    fn every_bound_or_menu_action_is_in_the_palette_or_left_out() {
        let in_palette: Vec<String> = COMMANDS.iter().filter_map(|c| action_name(c.run)).collect();
        let decided = |action: &str| {
            in_palette.iter().any(|a| a == action) || NOT_IN_PALETTE.contains(&action)
        };
        for (action, keystroke) in real_pairs() {
            if matches!(describe(&action), Describe::Listed(..)) {
                assert!(decided(&action), "bound {action} ({keystroke})");
            }
        }
        let menu = crate::menu_action_names();
        // The walk reaches every menu, submenus included.
        for expected in [
            "fuxx::CheckForUpdates",
            "workspace::LaunchPreset9",
            "edit::Cut",
            "workspace::EqualizePanes",
            "window::Zoom",
            "help::ShowLogs",
            "fuxx::QuitAndEndSessions",
        ] {
            assert!(menu.iter().any(|m| m == expected), "{expected} not walked");
        }
        for action in &menu {
            assert!(decided(action), "menu {action}");
        }
        let mut names: Vec<&str> = COMMANDS.iter().map(|c| c.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), COMMANDS.len(), "COMMANDS names unique");
    }

    /// The ⋯ menu's items (`pane_actions`) are palette entries too: each by its own name, by
    /// the name the palette gives the same action (`PANE_MENU_AS`), or left out on purpose
    /// (`PANE_MENU_LEFT_OUT`).
    #[test]
    fn every_pane_menu_item_is_in_the_palette_or_left_out() {
        use crate::pane_actions::PaneAction;
        let in_palette = |name: &str| COMMANDS.iter().any(|c| c.name == name);
        for action in PaneAction::ALL {
            let label = action.label();
            let decided = in_palette(label)
                || PANE_MENU_AS
                    .iter()
                    .any(|(item, entry)| *item == label && in_palette(entry))
                || PANE_MENU_LEFT_OUT.contains(&label);
            assert!(decided, "⋯ menu {label}");
        }
        // Each alias names an item still in the menu.
        for (item, _) in PANE_MENU_AS {
            assert!(
                PaneAction::ALL.iter().any(|a| a.label() == *item),
                "{item} is no ⋯ item"
            );
        }
    }

    fn names(rows: &[ActionRow]) -> Vec<&str> {
        rows.iter().map(|r| r.name.as_str()).collect()
    }

    #[test]
    fn actions_apply_only_where_they_work() {
        let pairs = real_pairs();
        let terminal = action_rows(Focused::Terminal, false, &[], &pairs);
        let t = names(&terminal);
        assert!(t.contains(&"Copy") && t.contains(&"Paste"));
        assert!(!t.contains(&"Find in File\u{2026}"));
        assert!(!t.contains(&"Reload Page"));
        assert!(!t.contains(&"Quit fuxx and End All Sessions"));
        assert!(t.contains(&"Quit fuxx") && t.contains(&"New Browser Tab"));
        let file = action_rows(Focused::File, false, &[], &pairs);
        let f = names(&file);
        assert!(f.contains(&"Find in File\u{2026}") && f.contains(&"Copy"));
        assert!(!f.contains(&"Paste"));
        let browser = action_rows(Focused::Browser, true, &[], &pairs);
        let b = names(&browser);
        assert!(b.contains(&"Reload Page"));
        assert!(b.contains(&"Quit fuxx and End All Sessions"));
        assert!(!b.contains(&"Copy"));
        let nothing = action_rows(Focused::Nothing, false, &[], &pairs);
        let n = names(&nothing);
        assert!(!n.contains(&"Copy") && !n.contains(&"Find in File\u{2026}"));
        assert!(n.contains(&"Split Right"));
        assert!(applies(Place::Anywhere, Focused::Nothing, false));
        assert!(applies(Place::TerminalOrFile, Focused::File, false));
        assert!(!applies(Place::Persistent, Focused::Terminal, false));
    }

    #[test]
    fn action_rows_carry_keys_and_presets() {
        let pairs = real_pairs();
        let presets = ["Claude".to_string(), "Codex".to_string()];
        let rows = action_rows(Focused::Terminal, false, &presets, &pairs);
        let find = |name: &str| rows.iter().find(|r| r.name == name).expect(name);
        assert_eq!(
            find("Split Down").keys.as_deref(),
            Some("\u{21e7}\u{2318}D")
        );
        assert_eq!(
            find("Scroll Up a Page").keys.as_deref(),
            Some("\u{21e7}\u{21de} / \u{2318}\u{2191}")
        );
        assert_eq!(find("Move Workspace Up").keys, None);
        assert_eq!(find("Split Right").key, Key::Action("Split Right"));
        assert!(find("Split Right").rest.contains("Panes"));
        assert!(
            find("New Workspace in Folder\u{2026}")
                .rest
                .contains("open folder")
        );
        let at = rows
            .iter()
            .position(|r| r.name == "Open Browser\u{2026}")
            .expect("Open Browser");
        let claude = &rows[at + 1];
        assert_eq!(claude.name, "Start Claude");
        assert_eq!(claude.key, Key::Preset(0));
        assert_eq!(claude.keys.as_deref(), Some("\u{2303}\u{2318}1"));
        assert_eq!(claude.group, ShortcutGroup::Workspaces);
        assert!(claude.rest.contains("agent preset"));
        assert_eq!(rows[at + 2].name, "Start Codex");
        assert_eq!(rows[at + 2].key, Key::Preset(1));
        assert_eq!(rows[at + 3].name, "New Tab");
        // Only the first nine presets have a key, so only they have a row.
        let many: Vec<String> = (0..12).map(|n| format!("p{n}")).collect();
        let rows = action_rows(Focused::Nothing, false, &many, &pairs);
        assert_eq!(
            rows.iter()
                .filter(|r| matches!(r.key, Key::Preset(_)))
                .count(),
            9
        );
    }

    fn entry(now: Instant, id: u64, label: &str, group: &str, status: Status, secs: u64) -> Entry {
        Entry {
            id,
            label: label.into(),
            group: Some(group.into()),
            status,
            since: now - Duration::from_secs(secs),
            question: None,
            color: None,
        }
    }

    /// The switcher scenes' eight workspaces (ids 10–17 for #1–#8). Most urgent first: claude,
    /// agent, tests, build, codex, deploy, shell, logs.
    fn eight() -> Vec<Entry> {
        let now = Instant::now();
        let mut all = vec![
            entry(now, 10, "claude", "api", Status::Waiting, 252),
            entry(now, 11, "codex", "api", Status::Working, 32),
            entry(now, 12, "tests", "api", Status::Done, 720),
            entry(now, 13, "shell", "api", Status::Idle, 0),
            entry(now, 14, "agent", "web", Status::Waiting, 95),
            entry(now, 15, "build", "web", Status::Done, 40),
            entry(now, 16, "deploy", "build-box", Status::Working, 180),
            entry(now, 17, "logs", "build-box", Status::Idle, 0),
        ];
        all[0].question = Some("Do you want to make this edit to auth.rs?".into());
        all
    }

    const URGENT: [u64; 8] = [10, 14, 12, 15, 11, 16, 13, 17];

    fn yes(_: &Key) -> bool {
        true
    }

    /// Every action (each in its own place's rows), in table order.
    fn every_action() -> Vec<ActionRow> {
        let pairs = real_pairs();
        let mut all = action_rows(Focused::Terminal, true, &[], &pairs);
        for row in action_rows(Focused::Browser, true, &[], &pairs)
            .into_iter()
            .chain(action_rows(Focused::File, true, &[], &pairs))
        {
            if !all.iter().any(|r| r.key == row.key) {
                all.push(row);
            }
        }
        let place = |r: &ActionRow| COMMANDS.iter().position(|c| Key::Action(c.name) == r.key);
        all.sort_by_key(place);
        all
    }

    struct Fixture {
        workspaces: Vec<Entry>,
        actions: Vec<ActionRow>,
        settings: Vec<PaletteSetting>,
        recent: Recent,
    }

    impl Fixture {
        fn new() -> Self {
            Fixture {
                workspaces: eight(),
                actions: action_rows(Focused::Terminal, false, &[], &real_pairs()),
                settings: palette_settings(),
                recent: Recent::default(),
            }
        }
        fn sources<'a>(&'a self, scope: Scope, query: &'a str, files: Files<'a>) -> Sources<'a> {
            Sources {
                scope,
                query,
                workspaces: &self.workspaces,
                actions: &self.actions,
                settings: &self.settings,
                files,
                recent: &self.recent,
                valid: &yes,
            }
        }
        fn compose(&self, scope: Scope, query: &str) -> Vec<Line> {
            compose(&self.sources(scope, query, Files::Note(NO_FOLDER)))
        }
    }

    fn item(key: Key) -> Line {
        Line::Item {
            key,
            positions: Vec::new(),
        }
    }

    fn keys(lines: &[Line]) -> Vec<Key> {
        lines
            .iter()
            .filter_map(|l| match l {
                Line::Item { key, .. } => Some(key.clone()),
                _ => None,
            })
            .collect()
    }

    fn headers(lines: &[Line]) -> Vec<&str> {
        lines
            .iter()
            .filter_map(|l| match l {
                Line::Header(h) => Some(h.as_str()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn empty_all_shows_recent_workspaces_and_actions() {
        let mut f = Fixture::new();
        f.recent.push(Key::Workspace(12));
        f.recent.push(Key::Action("Split Right"));
        let n = f.actions.len();
        let lines = f.compose(Scope::All, "");
        let mut want = vec![
            Line::Header("Recent".into()),
            item(Key::Action("Split Right")),
            item(Key::Workspace(12)),
            Line::Header("Workspaces".into()),
        ];
        want.extend([10, 14, 15, 11, 16].map(|id| item(Key::Workspace(id))));
        want.push(Line::More {
            kind: Kind::Workspace,
            count: 2,
        });
        want.push(Line::Header("Actions".into()));
        want.extend(
            [
                "New Workspace",
                "New Workspace in Folder\u{2026}",
                "New SSH Workspace\u{2026}",
                "Rename Workspace\u{2026}",
                "Move Workspace Up",
            ]
            .map(|a| item(Key::Action(a))),
        );
        want.push(Line::More {
            kind: Kind::Action,
            count: n - 6,
        });
        assert_eq!(lines, want);

        // A Recent key that no longer stands for anything is skipped, and its item is back in
        // its section.
        let gone = |k: &Key| *k != Key::Workspace(12);
        let mut s = f.sources(Scope::All, "", Files::Note(NO_FOLDER));
        s.valid = &gone;
        let lines = compose(&s);
        assert_eq!(
            &lines[..3],
            &[
                Line::Header("Recent".into()),
                item(Key::Action("Split Right")),
                Line::Header("Workspaces".into()),
            ]
        );
        assert_eq!(keys(&lines)[1..6], [10, 14, 12, 15, 11].map(Key::Workspace));
    }

    #[test]
    fn typing_orders_sections_by_their_best_match() {
        let f = Fixture::new();
        let glow = f.compose(Scope::All, "glow");
        assert_eq!(glow[0], Line::Header("Settings".into()));
        assert!(matches!(
            &glow[1],
            Line::Item {
                key: Key::Setting("Cursor breathing glow"),
                ..
            }
        ));

        let split = f.compose(Scope::All, "split");
        assert_eq!(split[0], Line::Header("Actions".into()));
        assert_eq!(
            keys(&split)[..2],
            [Key::Action("Split Right"), Key::Action("Split Down")]
        );
        let Line::Item { positions, .. } = &split[1] else {
            panic!("an item")
        };
        assert_eq!(positions, &[0, 1, 2, 3, 4]);
        assert!(!headers(&split).contains(&"Recent"));

        let three = f.compose(Scope::All, "3");
        assert_eq!(three[0], Line::Header("Workspaces".into()));
        assert_eq!(three[1], item(Key::Workspace(12)));

        let root = Path::new("/w/fuxx-cli");
        let hits: Vec<FileHit> = (0..7)
            .map(|i| FileHit {
                rel: format!("src/main{i}.rs"),
                positions: vec![4, 5, 6, 7],
                score: 200 - i,
            })
            .collect();
        let files = Files::Ready {
            root,
            root_name: "fuxx-cli",
            hits: &hits,
            total: 7,
            capped: false,
            walked: false,
        };
        let main = compose(&f.sources(Scope::All, "main", files));
        let at = main
            .iter()
            .position(|l| *l == Line::Header("Files in fuxx-cli".into()))
            .expect("a Files section");
        let mut want: Vec<Line> = (0..5)
            .map(|i| Line::Item {
                key: Key::File(root.join(format!("src/main{i}.rs"))),
                positions: vec![4, 5, 6, 7],
            })
            .collect();
        want.push(Line::More {
            kind: Kind::File,
            count: 2,
        });
        assert_eq!(main[at + 1..at + 7], want[..]);
        // The best file outscores the rest, so Files leads.
        assert_eq!(at, 0);

        assert!(f.compose(Scope::All, "zzqqxx").is_empty());
        for files in [Files::Listing, Files::Note(SSH_NOTE), Files::Failed] {
            let lines = compose(&f.sources(Scope::All, "main", files));
            assert!(!headers(&lines).iter().any(|h| h.starts_with("Files")));
            assert!(!lines.iter().any(|l| matches!(l, Line::Note(_))));
        }
        // On All, the cap note follows the files.
        let capped = Files::Ready {
            root,
            root_name: "fuxx-cli",
            hits: &hits[..2],
            total: 2,
            capped: true,
            walked: false,
        };
        let lines = compose(&f.sources(Scope::All, "main", capped));
        assert_eq!(lines[3], Line::Note(CAPPED.into()));
    }

    #[test]
    fn narrowed_scopes_list_everything() {
        let mut f = Fixture::new();
        let ws = f.compose(Scope::Workspaces, "");
        assert_eq!(keys(&ws), URGENT.map(Key::Workspace));
        assert!(!ws.iter().any(|l| matches!(l, Line::More { .. })));

        f.actions = every_action();
        let actions = f.compose(Scope::Actions, "");
        assert_eq!(
            headers(&actions),
            [
                "Workspaces",
                "Tabs",
                "Panes",
                "Terminal",
                "Browser pages",
                "Window & app"
            ]
        );
        // Each row under its own group's heading, every action once.
        let mut heading = "";
        for line in &actions {
            match line {
                Line::Header(h) => heading = h,
                Line::Item { key, .. } => {
                    let row = f.actions.iter().find(|r| r.key == *key).expect("a row");
                    assert_eq!(row.group.label(), heading, "{}", row.name);
                }
                other => panic!("{other:?}"),
            }
        }
        assert_eq!(keys(&actions).len(), f.actions.len());

        let settings = f.compose(Scope::Settings, "");
        assert_eq!(
            headers(&settings),
            ["General", "Appearance", "Sessions", "Browser"]
        );
        assert_eq!(keys(&settings).len(), 17);

        let root = Path::new("/w/fuxx-cli");
        let ready = |hits, total, capped| Files::Ready {
            root,
            root_name: "fuxx-cli",
            hits,
            total,
            capped,
            walked: false,
        };
        assert_eq!(
            compose(&f.sources(Scope::Files, "", ready(&[], 0, false))),
            [Line::Note("Type to search files in fuxx-cli".into())]
        );
        let many: Vec<FileHit> = (0..250)
            .map(|i| FileHit {
                rel: format!("x{i}"),
                positions: vec![0],
                score: 40,
            })
            .collect();
        let lines = compose(&f.sources(Scope::Files, "x", ready(&many, 250, false)));
        assert_eq!(keys(&lines).len(), 200);
        assert_eq!(
            lines.last(),
            Some(&Line::Note("\u{2026} 50 more files".into()))
        );
        for scope_query in [(Scope::Files, ""), (Scope::Files, "x")] {
            let (scope, query) = scope_query;
            assert_eq!(
                compose(&f.sources(scope, query, Files::Note(SSH_NOTE))),
                [Line::Note(SSH_NOTE.into())]
            );
            assert_eq!(
                compose(&f.sources(scope, query, Files::Listing)),
                [Line::Note(LISTING.into())]
            );
            assert_eq!(
                compose(&f.sources(scope, query, Files::Failed)),
                [Line::Note(FAILED.into())]
            );
        }
        assert_eq!(
            compose(&f.sources(Scope::Files, "zzz", ready(&[], 0, true))),
            [Line::Note(CAPPED.into())]
        );
        assert!(compose(&f.sources(Scope::Files, "zzz", ready(&[], 0, false))).is_empty());
        let lines = compose(&f.sources(Scope::Files, "x", ready(&many[..3], 3, true)));
        assert_eq!(keys(&lines).len(), 3);
        assert_eq!(lines.last(), Some(&Line::Note(CAPPED.into())));

        // Nothing typed: Recent of that kind first, not repeated below; but Workspaces (⌘P) is
        // exactly the old list, with no Recent.
        f.recent.push(Key::Workspace(17));
        f.recent.push(Key::Action("Split Right"));
        let ws = f.compose(Scope::Workspaces, "");
        assert_eq!(ws[0], Line::Header("Workspaces".into()));
        assert!(!headers(&ws).contains(&"Recent"));
        assert_eq!(keys(&ws), URGENT.map(Key::Workspace));
        let actions = f.compose(Scope::Actions, "");
        assert_eq!(actions[0], Line::Header("Recent".into()));
        assert_eq!(actions[1], item(Key::Action("Split Right")));
        assert_eq!(
            keys(&actions)
                .iter()
                .filter(|k| **k == Key::Action("Split Right"))
                .count(),
            1
        );
        let file = Key::File(root.join("src/a.rs"));
        f.recent.push(file.clone());
        assert_eq!(
            compose(&f.sources(Scope::Files, "", ready(&[], 0, false))),
            [
                Line::Header("Recent".into()),
                item(file),
                Line::Note("Type to search files in fuxx-cli".into())
            ]
        );
    }

    #[test]
    fn a_cut_short_walk_says_only_part_is_searched() {
        let f = Fixture::new();
        let root = Path::new("/w/notes");
        let hits = [FileHit {
            rel: "main.md".into(),
            positions: vec![0, 1, 2, 3],
            score: 100,
        }];
        let walked = |hits, walked| Files::Ready {
            root,
            root_name: "notes",
            hits,
            total: hits.len(),
            capped: true,
            walked,
        };
        let walk_note = Line::Note(CAPPED_WALK.into());
        // Every place the cap note shows: Files empty, a miss, after the hits, and on All.
        let empty = compose(&f.sources(Scope::Files, "", walked(&[], true)));
        assert_eq!(empty.last(), Some(&walk_note));
        let miss = compose(&f.sources(Scope::Files, "zzz", walked(&[], true)));
        assert_eq!(miss, std::slice::from_ref(&walk_note));
        let found = compose(&f.sources(Scope::Files, "main", walked(&hits, true)));
        assert_eq!(found.last(), Some(&walk_note));
        let all = compose(&f.sources(Scope::All, "main", walked(&hits, true)));
        assert!(all.contains(&walk_note), "{all:?}");
        assert!(!all.contains(&Line::Note(CAPPED.into())));
        // git's cap keeps its own words.
        let git = compose(&f.sources(Scope::Files, "zzz", walked(&[], false)));
        assert_eq!(git, [Line::Note(CAPPED.into())]);
    }

    #[test]
    fn compose_keeps_keys_stable() {
        let mut f = Fixture::new();
        for (scope, query) in [
            (Scope::Workspaces, ""),
            (Scope::All, "a"),
            (Scope::Actions, ""),
        ] {
            let before: HashSet<Key> = keys(&f.compose(scope, query)).into_iter().collect();
            f.workspaces[7].status = Status::Waiting;
            let lines = f.compose(scope, query);
            let after: HashSet<Key> = keys(&lines).into_iter().collect();
            assert_eq!(before, after, "{scope:?} {query:?}");
            // Every selectable key appears once.
            assert_eq!(keys(&lines).len(), after.len());
            f.workspaces[7].status = Status::Idle;
        }
        // logs is now urgent, so it moved, but the same rows are there.
        f.workspaces[7].status = Status::Waiting;
        assert_eq!(
            keys(&f.compose(Scope::Workspaces, ""))[2],
            Key::Workspace(17)
        );

        let lines = vec![
            Line::Header("Recent".into()),
            item(Key::Workspace(1)),
            Line::Note("n".into()),
            Line::More {
                kind: Kind::File,
                count: 3,
            },
            item(Key::Setting("Theme")),
        ];
        assert_eq!(selectable(&lines), [1, 3, 4]);
    }

    #[test]
    fn welcome_is_found_in_the_palette() {
        let find = |name: &str| COMMANDS.iter().find(|c| c.name == name).expect(name);
        let welcome = find("Welcome to fuxx");
        let tips = find("Show Tips Card");
        for query in ["welcome", "first run", "intro", "start agent"] {
            assert!(
                match_item(query, welcome.name, &[welcome.keywords], None).is_some(),
                "{query}"
            );
        }
        assert!(match_item("welcome tips", tips.name, &[tips.keywords], None).is_some());
        for c in [welcome, tips] {
            assert_eq!(c.group, ShortcutGroup::WindowAndApp, "{}", c.name);
            assert_eq!(c.place, Place::Anywhere, "{}", c.name);
        }
        assert!(COMMANDS.iter().all(|c| c.name != "Show Welcome Tips"));
    }

    const BLOCK_COMMANDS: [&str; 6] = [
        "Previous Command",
        "Next Command",
        "Copy Command Output",
        "Copy Command",
        "Copy Command and Output",
        "Rerun Command",
    ];

    #[test]
    fn block_commands_are_found() {
        let f = Fixture::new();
        let found = |query: &str| keys(&f.compose(Scope::Actions, query));
        for query in ["command", "block"] {
            let got = found(query);
            for name in BLOCK_COMMANDS {
                assert!(
                    got.contains(&Key::Action(name)),
                    "{query:?} finds {name}: {got:?}"
                );
            }
        }
        assert!(found("run again").contains(&Key::Action("Rerun Command")));
        assert!(found("jump").contains(&Key::Action("Previous Command")));
        for name in BLOCK_COMMANDS {
            let c = COMMANDS.iter().find(|c| c.name == name).expect(name);
            assert_eq!(c.place, Place::Terminal, "{name}");
            assert_eq!(c.group, ShortcutGroup::Terminal, "{name}");
        }
        // The three with keys show them.
        let row = |name: &str| {
            f.actions
                .iter()
                .find(|r| r.name == name)
                .expect(name)
                .keys
                .clone()
        };
        assert_eq!(
            row("Previous Command").as_deref(),
            Some("\u{21e7}\u{2318}\u{2191}")
        );
        assert_eq!(
            row("Next Command").as_deref(),
            Some("\u{21e7}\u{2318}\u{2193}")
        );
        assert_eq!(
            row("Copy Command Output").as_deref(),
            Some("\u{21e7}\u{2318}C")
        );
        for name in ["Copy Command", "Copy Command and Output", "Rerun Command"] {
            assert_eq!(row(name), None, "{name} has no key");
        }
        // Not where there is no terminal.
        let file = action_rows(Focused::File, false, &[], &real_pairs());
        assert!(!names(&file).contains(&"Rerun Command"));
    }

    #[test]
    fn loose_and_number_matches() {
        let f = Fixture::new();
        let auth = f.compose(Scope::All, "auth.rs");
        assert_eq!(auth[0], Line::Header("Workspaces".into()));
        assert_eq!(auth[1], item(Key::Workspace(10)));
        // Keys typed as words find their action. (On All, the command-block rows, whose names
        // fuzzy-match "cmd d", fill the section first: Split Right is under its "… more".)
        let cmd = f.compose(Scope::Actions, "cmd d");
        assert!(cmd.contains(&item(Key::Action("Split Right"))));
        // On All it is still found, under the actions' "… more" (which opens that list).
        let all = f.compose(Scope::All, "cmd d");
        let shown = keys(&all);
        assert!(!shown.contains(&Key::Action("Split Right")), "{shown:?}");
        let matched = keys(&cmd).len();
        assert!(
            all.contains(&Line::More {
                kind: Kind::Action,
                count: matched - SECTION_CAP
            }),
            "{all:?}"
        );
        let four = f.compose(Scope::All, "#4");
        assert_eq!(keys(&four)[0], Key::Workspace(13));
        // A group finds its workspaces, most urgent first.
        let web = f.compose(Scope::Workspaces, "web");
        assert_eq!(keys(&web), [Key::Workspace(14), Key::Workspace(15)]);
        assert_eq!(more_label(Kind::File, 1), "\u{2026} 1 more file");
        assert_eq!(more_label(Kind::Workspace, 3), "\u{2026} 3 more workspaces");
        assert_eq!(more_label(Kind::Setting, 2), "\u{2026} 2 more settings");
        assert_eq!(more_label(Kind::Action, 1), "\u{2026} 1 more action");
    }
}
