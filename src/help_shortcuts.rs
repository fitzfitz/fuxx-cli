// The Keyboard Shortcuts list: what fuxx's key-binding table says, in groups, ready to show.
// Pure: it reads the bindings and builds text. The panel that shows it is elsewhere.

use gpui::KeyBinding;

/// The panel's sections, in the order they appear.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ShortcutGroup {
    Workspaces,
    Tabs,
    Panes,
    Terminal,
    BrowserPages,
    WindowAndApp,
}

impl ShortcutGroup {
    /// The section's heading.
    pub fn label(self) -> &'static str {
        match self {
            ShortcutGroup::Workspaces => "Workspaces",
            ShortcutGroup::Tabs => "Tabs",
            ShortcutGroup::Panes => "Panes",
            ShortcutGroup::Terminal => "Terminal",
            ShortcutGroup::BrowserPages => "Browser pages",
            ShortcutGroup::WindowAndApp => "Window & app",
        }
    }
}

/// One line of the list: what a shortcut does, and its keys as they are written on a Mac.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShortcutRow {
    pub group: ShortcutGroup,
    pub name: &'static str,
    pub keys: String,
}

/// What the list does with a bound action.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Describe {
    /// Shown, in this group, under this name.
    Listed(ShortcutGroup, &'static str),
    /// Bound, but not worth a line (a text box's own keys, a menu's, a picker's).
    Unlisted,
    /// Not known here. A test fails on this, so a new binding gets a decision.
    Unknown,
}

/// Where an action goes in the list. `action` is GPUI's `Action::name()`.
pub fn describe(action: &str) -> Describe {
    use Describe::{Listed, Unlisted};
    use ShortcutGroup::*;
    match action {
        "workspace::NewWorkspace" => Listed(Workspaces, "New Workspace"),
        "workspace::NewSessionInFolder" => Listed(Workspaces, "New Workspace in Folder\u{2026}"),
        "workspace::NewSshSession" => Listed(Workspaces, "New SSH Workspace\u{2026}"),
        "workspace::RenameSession" => Listed(Workspaces, "Rename Workspace\u{2026}"),
        "workspace::OpenSwitcher" => Listed(Workspaces, "Go to Session\u{2026}"),
        "workspace::NextNeedingYou" => Listed(Workspaces, "Go to What Needs You"),
        "workspace::OpenBrowser" => Listed(Workspaces, "Open Browser\u{2026}"),
        "workspace::NextSession" => Listed(Workspaces, "Next workspace"),
        "workspace::PrevSession" => Listed(Workspaces, "Previous workspace"),
        "workspace::SelectSession1"
        | "workspace::SelectSession2"
        | "workspace::SelectSession3"
        | "workspace::SelectSession4"
        | "workspace::SelectSession5"
        | "workspace::SelectSession6"
        | "workspace::SelectSession7"
        | "workspace::SelectSession8"
        | "workspace::SelectSession9" => Listed(Workspaces, "Go to workspace 1\u{2013}9"),
        "workspace::LaunchPreset1"
        | "workspace::LaunchPreset2"
        | "workspace::LaunchPreset3"
        | "workspace::LaunchPreset4"
        | "workspace::LaunchPreset5"
        | "workspace::LaunchPreset6"
        | "workspace::LaunchPreset7"
        | "workspace::LaunchPreset8"
        | "workspace::LaunchPreset9" => Listed(Workspaces, "Start agent preset 1\u{2013}9"),

        "workspace::NewTab" => Listed(Tabs, "New Tab"),
        "workspace::CloseSession" => Listed(Tabs, "Close Tab"),
        "workspace::NextTab" => Listed(Tabs, "Next tab"),
        "workspace::PrevTab" => Listed(Tabs, "Previous tab"),

        "workspace::SplitRight" => Listed(Panes, "Split Right"),
        "workspace::SplitDown" => Listed(Panes, "Split Down"),
        "workspace::PrevPane" => Listed(Panes, "Previous Pane"),
        "workspace::NextPane" => Listed(Panes, "Next Pane"),
        "workspace::FocusPaneLeft" => Listed(Panes, "Focus Pane Left"),
        "workspace::FocusPaneRight" => Listed(Panes, "Focus Pane Right"),
        "workspace::FocusPaneUp" => Listed(Panes, "Focus Pane Up"),
        "workspace::FocusPaneDown" => Listed(Panes, "Focus Pane Down"),
        "workspace::EqualizePanes" => Listed(Panes, "Equalize Panes"),

        "terminal::Copy" => Listed(Terminal, "Copy"),
        "terminal::Paste" => Listed(Terminal, "Paste"),
        "terminal::ScrollPageUp" => Listed(Terminal, "Scroll up a page"),
        "terminal::ScrollPageDown" => Listed(Terminal, "Scroll down a page"),

        "browser_tab::FocusAddress" => Listed(BrowserPages, "Address field"),
        "browser_tab::Reload" => Listed(BrowserPages, "Reload"),
        "browser_tab::GoBack" => Listed(BrowserPages, "Back"),
        "browser_tab::GoForward" => Listed(BrowserPages, "Forward"),
        "browser_tab::ZoomIn" => Listed(BrowserPages, "Zoom in"),
        "browser_tab::ZoomOut" => Listed(BrowserPages, "Zoom out"),
        "browser_tab::ResetZoom" => Listed(BrowserPages, "Actual size"),

        "fuxx::Quit" => Listed(WindowAndApp, "Quit fuxx"),
        "fuxx::QuitAndEndSessions" => Listed(WindowAndApp, "Quit fuxx and End All Sessions"),
        "fuxx::OpenPreferences" => Listed(WindowAndApp, "Preferences\u{2026}"),
        "fuxx::Hide" => Listed(WindowAndApp, "Hide fuxx"),
        "window::Minimize" => Listed(WindowAndApp, "Minimize"),
        "workspace::ToggleSidebar" => Listed(WindowAndApp, "Toggle Sidebar"),
        "view::Bigger" => Listed(WindowAndApp, "Bigger"),
        "view::Smaller" => Listed(WindowAndApp, "Smaller"),
        "view::ActualSize" => Listed(WindowAndApp, "Actual Size"),
        "help::ShowShortcuts" => Listed(WindowAndApp, "Keyboard Shortcuts"),

        "browser_tab::SuggestPrev"
        | "browser_tab::SuggestNext"
        | "tab_menu::Dismiss"
        | "edit::Cut"
        | "edit::SelectAll"
        | "help_panel::ScrollUp"
        | "help_panel::ScrollDown" => Unlisted,
        other if other.starts_with("name_input::") => Unlisted,
        other if other.starts_with("host_picker::") => Unlisted,
        other if other.starts_with("switcher::") => Unlisted,
        _ => Describe::Unknown,
    }
}

/// A keystroke as a Mac writes it: modifiers in the order ⌃⌥⇧⌘, then the key. `cmd-shift-d` is
/// `⇧⌘D`. A key macOS reports with shift folded in (`}`, `{`) is shown as its shift-key pair.
pub fn key_label(keystroke: &str) -> String {
    let (mut ctrl, mut alt, mut shift, mut cmd) = (false, false, false, false);
    let mut rest = keystroke;
    loop {
        if let Some(r) = rest.strip_prefix("ctrl-") {
            ctrl = true;
            rest = r;
        } else if let Some(r) = rest.strip_prefix("alt-") {
            alt = true;
            rest = r;
        } else if let Some(r) = rest.strip_prefix("shift-") {
            shift = true;
            rest = r;
        } else if let Some(r) = rest.strip_prefix("cmd-") {
            cmd = true;
            rest = r;
        } else {
            break;
        }
    }
    let key = match rest {
        "}" => {
            shift = true;
            "]".to_string()
        }
        "{" => {
            shift = true;
            "[".to_string()
        }
        "left" => "\u{2190}".to_string(),
        "right" => "\u{2192}".to_string(),
        "up" => "\u{2191}".to_string(),
        "down" => "\u{2193}".to_string(),
        "pageup" => "\u{21de}".to_string(),
        "pagedown" => "\u{21df}".to_string(),
        "tab" => "\u{21e5}".to_string(),
        "enter" => "\u{21a9}".to_string(),
        "escape" => "\u{238b}".to_string(),
        "backspace" => "\u{232b}".to_string(),
        "space" => "Space".to_string(),
        other => other.to_uppercase(),
    };
    let mut label = String::new();
    for (on, symbol) in [
        (ctrl, '\u{2303}'),
        (alt, '\u{2325}'),
        (shift, '\u{21e7}'),
        (cmd, '\u{2318}'),
    ] {
        if on {
            label.push(symbol);
        }
    }
    label + &key
}

/// Each binding's action name and keystroke text (`cmd-shift-d`; a chord's keystrokes spaced).
pub fn binding_pairs(bindings: &[KeyBinding]) -> Vec<(String, String)> {
    bindings
        .iter()
        .map(|binding| {
            let keys: Vec<String> = binding.keystrokes().iter().map(|k| k.unparse()).collect();
            (binding.action().name().to_string(), keys.join(" "))
        })
        .collect()
}

/// The list: only `Listed` actions, groups in enum order, lines in the order first bound, an
/// action's keys joined (the same line for several actions, as the nine workspace keys are).
pub fn shortcut_rows(pairs: &[(String, String)]) -> Vec<ShortcutRow> {
    // (group, name, keystroke texts) in first-seen order.
    let mut merged: Vec<(ShortcutGroup, &'static str, Vec<&str>)> = Vec::new();
    for (action, keystroke) in pairs {
        let Describe::Listed(group, name) = describe(action) else {
            continue;
        };
        match merged.iter_mut().find(|m| m.0 == group && m.1 == name) {
            Some(entry) => {
                if !entry.2.contains(&keystroke.as_str()) {
                    entry.2.push(keystroke);
                }
            }
            None => merged.push((group, name, vec![keystroke])),
        }
    }
    // A stable sort keeps first-seen order within a group.
    merged.sort_by_key(|m| m.0);
    merged
        .into_iter()
        .map(|(group, name, keystrokes)| {
            let labels: Vec<String> = keystrokes.iter().map(|k| key_label(k)).collect();
            let keys = match labels.as_slice() {
                [first, .., last] if labels.len() >= 3 => format!("{first} \u{2013} {last}"),
                _ => labels.join(" / "),
            };
            ShortcutRow { group, name, keys }
        })
        .collect()
}

/// Whether a search box's text finds a row: in its name, its group's label or its keys, ignoring case. Keys
/// may be typed as the symbols (`⌘d`) or as words (`cmd d`, `cmd-d`).
pub fn row_matches(row: &ShortcutRow, query: &str) -> bool {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return true;
    }
    let keys = row.keys.to_lowercase();
    row.name.to_lowercase().contains(&query)
        || row.group.label().to_lowercase().contains(&query)
        || keys.contains(&query)
        || keys.contains(&key_label(&query.replace(' ', "-")).to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs(list: &[(&str, &str)]) -> Vec<(String, String)> {
        list.iter()
            .map(|(a, k)| (a.to_string(), k.to_string()))
            .collect()
    }

    #[test]
    fn key_labels_follow_mac_order_and_symbols() {
        for (keystroke, label) in [
            ("cmd-shift-d", "\u{21e7}\u{2318}D"),
            ("ctrl-cmd-=", "\u{2303}\u{2318}="),
            ("cmd-alt-left", "\u{2325}\u{2318}\u{2190}"),
            ("cmd-}", "\u{21e7}\u{2318}]"),
            ("cmd-{", "\u{21e7}\u{2318}["),
            ("shift-pageup", "\u{21e7}\u{21de}"),
            ("ctrl-shift-tab", "\u{2303}\u{21e7}\u{21e5}"),
            ("cmd-\\", "\u{2318}\\"),
            ("cmd-/", "\u{2318}/"),
            ("cmd--", "\u{2318}-"),
        ] {
            assert_eq!(key_label(keystroke), label, "{keystroke}");
        }
    }

    #[test]
    fn the_nine_workspace_keys_are_one_row() {
        let list: Vec<(String, String)> = (1..=9)
            .map(|n| (format!("workspace::SelectSession{n}"), format!("cmd-{n}")))
            .collect();
        let rows = shortcut_rows(&list);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name, "Go to workspace 1\u{2013}9");
        assert_eq!(rows[0].keys, "\u{2318}1 \u{2013} \u{2318}9");
    }

    #[test]
    fn two_keys_for_one_action_are_joined_with_a_slash() {
        let rows = shortcut_rows(&pairs(&[
            ("view::Bigger", "cmd-="),
            ("view::Bigger", "cmd-+"),
        ]));
        assert_eq!(rows[0].keys, "\u{2318}= / \u{2318}+");
    }

    #[test]
    fn a_terminal_binding_is_in_the_terminal_group() {
        let rows = shortcut_rows(&pairs(&[("terminal::Copy", "cmd-c")]));
        assert_eq!(rows[0].group, ShortcutGroup::Terminal);
        assert_eq!(rows[0].name, "Copy");
    }

    #[test]
    fn unlisted_actions_are_left_out_and_repeats_are_merged() {
        let rows = shortcut_rows(&pairs(&[
            ("name_input::Confirm", "enter"),
            ("workspace::NextTab", "ctrl-tab"),
            ("workspace::NextTab", "ctrl-tab"),
        ]));
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].keys, "\u{2303}\u{21e5}");
    }

    #[test]
    fn rows_are_found_by_name_or_keys() {
        let rows = shortcut_rows(&pairs(&[
            ("workspace::SplitRight", "cmd-d"),
            ("workspace::NewTab", "cmd-t"),
        ]));
        let split = rows.iter().find(|r| r.name == "Split Right").unwrap();
        assert!(row_matches(split, "split"));
        assert!(row_matches(split, "\u{2318}d"));
        assert!(row_matches(split, "cmd d"));
        assert!(row_matches(split, "cmd-d"));
        assert!(row_matches(split, ""));
        assert!(!row_matches(split, "zzz"));
    }

    #[test]
    fn rows_are_found_by_group() {
        let rows = shortcut_rows(&pairs(&[
            ("terminal::Copy", "cmd-c"),
            ("browser_tab::Reload", "cmd-r"),
        ]));
        let copy = rows.iter().find(|r| r.name == "Copy").unwrap();
        let reload = rows.iter().find(|r| r.name == "Reload").unwrap();
        assert!(row_matches(copy, "Terminal"));
        assert!(!row_matches(reload, "terminal"));
        assert!(row_matches(reload, "browser pages"));
        assert_eq!(ShortcutGroup::WindowAndApp.label(), "Window & app");
    }

    #[test]
    fn every_binding_is_listed_or_marked_unlisted() {
        let mut bindings = crate::key_bindings();
        bindings.extend(crate::persistent_bindings());
        bindings.extend(crate::new_bindings());
        for (action, keystroke) in binding_pairs(&bindings) {
            assert_ne!(
                describe(&action),
                Describe::Unknown,
                "{action} ({keystroke})"
            );
        }
    }

    #[test]
    fn groups_come_in_order() {
        let rows = shortcut_rows(&binding_pairs(&crate::key_bindings()));
        assert!(rows.windows(2).all(|w| w[0].group <= w[1].group));
        for group in [
            ShortcutGroup::Workspaces,
            ShortcutGroup::Tabs,
            ShortcutGroup::Panes,
            ShortcutGroup::Terminal,
            ShortcutGroup::BrowserPages,
            ShortcutGroup::WindowAndApp,
        ] {
            assert!(rows.iter().any(|r| r.group == group), "{group:?} is empty");
        }
    }
}
