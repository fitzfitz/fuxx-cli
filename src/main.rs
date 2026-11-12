// fuxx — a native GPU terminal for running AI coding agents.
// Many sessions in one window, each with a live status ring and background notifications.

mod activity;
mod activity_panel;
mod adopt;
mod agent_detect;
mod api;
mod api_client;
mod api_events;
mod api_server;
mod app_icon;
mod app_log;
mod appearance;
mod block_view;
mod blocks;
mod branch_git;
mod branch_list;
mod branch_switcher;
mod branch_sync;
mod browser;
mod browser_history;
mod browser_js;
mod browser_tab;
mod browser_url;
mod changes_panel;
mod cli;
mod colors;
mod cursor_look;
mod daemon;
mod daemon_client;
mod daemon_session;
mod detector;
mod diff_view;
mod drag_gap;
mod file_tab;
mod file_tree;
mod fonts;
mod git_branch;
mod git_graph;
mod git_stage;
mod git_status;
mod graph_layout;
mod graph_panel;
mod grid;
mod groups;
mod help_commands;
mod help_menu;
mod help_panel;
mod help_shortcuts;
mod highlight;
mod host_picker;
mod keys;
mod layout_model;
mod links;
mod mcp;
mod mouse;
#[allow(dead_code)] // used by the daemon and client from M10a Task 3/4
mod mux_proto;
mod name_input;
mod native_notify;
mod notification_list;
mod notifier;
mod overlay_panel;
mod palette;
mod palette_files;
mod palette_items;
mod palette_match;
mod pane_actions;
mod pane_browser;
mod pane_tab_menu;
mod panes;
mod plugin_fetch;
mod plugin_manifest;
mod plugin_source;
mod plugin_store;
mod plugin_themes;
mod plugins;
mod preferences;
mod preferences_plugins;
mod prefs;
mod presets;
mod private_socket;
mod procs;
mod pty;
mod reduce_motion;
mod restore;
mod right_sidebar;
mod session_list;
mod settings;
mod shell_integration;
mod shell_marks;
mod sidebar;
mod sidebar_toggle;
mod sounds;
mod split_tree;
mod ssh_control;
mod ssh_hosts;
mod ssh_target;
mod status;
mod status_bar;
mod status_mark;
mod switcher;
mod tab_bar;
mod tab_color;
mod tab_menu;
mod terminal;
mod terminal_view;
mod theme;
mod title_area;
#[cfg(feature = "ui-shots")]
mod ui_shots;
mod ui_tokens;
mod update_release;
mod updater;
mod vt_snapshot;
mod welcome;
mod welcome_items;
mod workspace;
mod workspace_api;
mod workspace_browser;
mod workspace_daemon;
mod workspace_ssh;

use std::io::{IsTerminal, Read, Write};
use std::path::{Path, PathBuf};

use futures::channel::mpsc::UnboundedReceiver;
use gpui::{
    App, Bounds, Global, KeyBinding, Menu, MenuItem, OsAction, PromptLevel, QuitMode,
    TitlebarOptions, WindowBounds, WindowOptions, actions, point, prelude::*, px, size,
};
use gpui_platform::application;
use serde_json::{Value, json};

use api_client::ClientError;
use cli::Cli;
use help_panel::HelpPage;
use prefs::FontStep;
use ui_tokens::size::TRAFFIC_LIGHTS;

use terminal_view::{
    Copy, CopyCommandOutput, NextCommand, Paste, PrevCommand, ScrollPageDown, ScrollPageUp,
};
use workspace::{
    CloseSession, EqualizePanes, FocusPaneDown, FocusPaneLeft, FocusPaneRight, FocusPaneUp,
    LaunchPreset1, LaunchPreset2, LaunchPreset3, LaunchPreset4, LaunchPreset5, LaunchPreset6,
    LaunchPreset7, LaunchPreset8, LaunchPreset9, MoveSessionDown, MoveSessionUp,
    NewSessionInFolder, NewSshSession, NewTab, NewWorkspace, NextNeedingYou, NextPane, NextSession,
    NextTab, OpenBrowser, OpenPalette, OpenSwitcher, PrevPane, PrevSession, PrevTab, RenameSession,
    SelectSession1, SelectSession2, SelectSession3, SelectSession4, SelectSession5, SelectSession6,
    SelectSession7, SelectSession8, SelectSession9, SplitDown, SplitRight, ToggleRightSidebar,
    ToggleSidebar, Workspace,
};

actions!(
    fuxx,
    [
        Quit,
        OpenPreferences,
        QuitAndEndSessions,
        Hide,
        CheckForUpdates
    ]
);

// The Help menu (M11d): ⌘/ opens Keyboard Shortcuts; the other items have no key.
actions!(
    help,
    [
        ShowWelcome,
        ShowShortcuts,
        ShowCommands,
        ShowNotificationStatus,
        ShowWelcomeTips,
        ReportIssue,
        ShowLogs
    ]
);

// The View menu's font size items (every window: they change the setting) and the Window menu's
// (the key fuxx window).
actions!(view, [Bigger, Smaller, ActualSize]);
actions!(window, [Minimize, Zoom]);

// The Edit menu's Cut and Select All. Nothing in fuxx handles them: their items work only
// through their macOS selectors (`cut:`, `selectAll:`), which a browser's web view answers.
actions!(edit, [Cut, SelectAll]);

/// The Edit menu: Cut, Copy, Paste and Select All, each also its standard macOS selector, sent
/// to the key window's first responder. In a browser window, the web view takes them. In
/// fuxx's window, GPUI handles the keys first (the terminal's ⌘C and ⌘V, as before), and the
/// items do what their GPUI action does there: Copy and Paste the terminal's, the others
/// nothing. Undo and Redo aren't here: GPUI sends them to itself, never to the web view, so
/// the browser window takes ⌘Z and ⇧⌘Z itself.
fn edit_menu() -> Menu {
    Menu::new("Edit").items([
        MenuItem::os_action("Cut", Cut, OsAction::Cut),
        MenuItem::os_action("Copy", Copy, OsAction::Copy),
        MenuItem::os_action("Paste", Paste, OsAction::Paste),
        MenuItem::os_action("Select All", SelectAll, OsAction::SelectAll),
    ])
}

/// ⌘T (a new tab in the focused pane) and ⌘N (a new workspace).
pub(crate) fn new_bindings() -> [KeyBinding; 2] {
    [
        KeyBinding::new("cmd-t", NewTab, None),
        KeyBinding::new("cmd-n", NewWorkspace, None),
    ]
}

/// Every key binding but ⌘T / ⌘N (`new_bindings`). Order matters where two bindings of a key
/// are equally deep in the focused element's key contexts: the later one wins. So a browser
/// tab's page keys come after fuxx's own.
pub(crate) fn key_bindings() -> Vec<KeyBinding> {
    let mut bindings = vec![
        KeyBinding::new("cmd-q", Quit, None),
        KeyBinding::new("cmd-,", OpenPreferences, None),
        KeyBinding::new("cmd-o", NewSessionInFolder, None),
        KeyBinding::new("cmd-shift-s", NewSshSession, None),
        KeyBinding::new("cmd-shift-b", OpenBrowser, None),
        KeyBinding::new("cmd-w", CloseSession, None),
        KeyBinding::new("cmd-r", RenameSession, None),
        // macOS reports cmd-shift-] as key "}" (shift folded into the character).
        KeyBinding::new("cmd-}", NextSession, None),
        KeyBinding::new("cmd-{", PrevSession, None),
        KeyBinding::new("cmd-1", SelectSession1, None),
        KeyBinding::new("cmd-2", SelectSession2, None),
        KeyBinding::new("cmd-3", SelectSession3, None),
        KeyBinding::new("cmd-4", SelectSession4, None),
        KeyBinding::new("cmd-5", SelectSession5, None),
        KeyBinding::new("cmd-6", SelectSession6, None),
        KeyBinding::new("cmd-7", SelectSession7, None),
        KeyBinding::new("cmd-8", SelectSession8, None),
        KeyBinding::new("cmd-9", SelectSession9, None),
        KeyBinding::new("ctrl-cmd-1", LaunchPreset1, None),
        KeyBinding::new("ctrl-cmd-2", LaunchPreset2, None),
        KeyBinding::new("ctrl-cmd-3", LaunchPreset3, None),
        KeyBinding::new("ctrl-cmd-4", LaunchPreset4, None),
        KeyBinding::new("ctrl-cmd-5", LaunchPreset5, None),
        KeyBinding::new("ctrl-cmd-6", LaunchPreset6, None),
        KeyBinding::new("ctrl-cmd-7", LaunchPreset7, None),
        KeyBinding::new("ctrl-cmd-8", LaunchPreset8, None),
        KeyBinding::new("ctrl-cmd-9", LaunchPreset9, None),
        KeyBinding::new("cmd-d", SplitRight, None),
        KeyBinding::new("cmd-shift-d", SplitDown, None),
        KeyBinding::new("cmd-alt-left", FocusPaneLeft, None),
        KeyBinding::new("cmd-alt-right", FocusPaneRight, None),
        KeyBinding::new("cmd-alt-up", FocusPaneUp, None),
        KeyBinding::new("cmd-alt-down", FocusPaneDown, None),
        KeyBinding::new("cmd-[", PrevPane, None),
        KeyBinding::new("cmd-]", NextPane, None),
        KeyBinding::new("ctrl-cmd-=", EqualizePanes, None),
        KeyBinding::new("cmd-\\", ToggleSidebar, None),
        KeyBinding::new("alt-cmd-\\", ToggleRightSidebar, None),
        KeyBinding::new("cmd-shift-u", NextNeedingYou, None),
        KeyBinding::new("cmd-k", OpenPalette, None),
        KeyBinding::new("cmd-p", OpenSwitcher, None),
        KeyBinding::new("cmd-/", ShowShortcuts, None),
        // The first binding is the one a menu shows: ⌘=, with ⌘+ (shift-=) as well.
        KeyBinding::new("cmd-=", Bigger, None),
        KeyBinding::new("cmd-+", Bigger, None),
        KeyBinding::new("cmd--", Smaller, None),
        KeyBinding::new("cmd-0", ActualSize, None),
        KeyBinding::new("cmd-h", Hide, None),
        KeyBinding::new("cmd-m", Minimize, None),
        // Terminal pane only. Bindings run before the pane's own key handler.
        KeyBinding::new("cmd-c", Copy, Some("Terminal")),
        KeyBinding::new("cmd-v", Paste, Some("Terminal")),
        KeyBinding::new("shift-pageup", ScrollPageUp, Some("Terminal")),
        KeyBinding::new("cmd-up", ScrollPageUp, Some("Terminal")),
        KeyBinding::new("shift-pagedown", ScrollPageDown, Some("Terminal")),
        KeyBinding::new("cmd-down", ScrollPageDown, Some("Terminal")),
        // Command blocks (M11l): jump between commands, copy the output.
        KeyBinding::new("cmd-shift-up", PrevCommand, Some("Terminal")),
        KeyBinding::new("cmd-shift-down", NextCommand, Some("Terminal")),
        KeyBinding::new("cmd-shift-c", CopyCommandOutput, Some("Terminal")),
        // ⌃Tab / ⌃⇧Tab: the focused pane's next / previous tab. Taken from the terminal
        // program (it got nothing for them before: `keys` sends no bytes for ⌃Tab), as
        // terminals with tabs do.
        KeyBinding::new("ctrl-tab", NextTab, Some("Terminal")),
        KeyBinding::new("ctrl-shift-tab", PrevTab, Some("Terminal")),
        // A browser tab: ⌘L to its address field, ⌃Tab / ⌃⇧Tab as in a terminal; ↑ / ↓
        // move through the field's suggestions, and an address is pasted.
        KeyBinding::new("cmd-l", browser_tab::FocusAddress, Some("BrowserTab")),
        KeyBinding::new("ctrl-tab", NextTab, Some("BrowserTab")),
        KeyBinding::new("ctrl-shift-tab", PrevTab, Some("BrowserTab")),
        // A file tab: ⌘F finds in the file, ⌘G / ⇧⌘G step through the matches, Esc closes the
        // find bar (in its box, the box's Esc does), ⌘C copies the selection, ⌃Tab / ⌃⇧Tab as
        // in a terminal. Its own keys, so terminals and browser tabs keep theirs.
        KeyBinding::new("cmd-f", file_tab::Find, Some("FileTab")),
        KeyBinding::new("cmd-g", file_tab::FindNext, Some("FileTab")),
        KeyBinding::new("cmd-shift-g", file_tab::FindPrev, Some("FileTab")),
        KeyBinding::new("escape", file_tab::CloseFind, Some("FileTab")),
        KeyBinding::new("cmd-c", Copy, Some("FileTab")),
        KeyBinding::new("ctrl-tab", NextTab, Some("FileTab")),
        KeyBinding::new("ctrl-shift-tab", PrevTab, Some("FileTab")),
        KeyBinding::new("up", browser_tab::SuggestPrev, Some("AddressField")),
        KeyBinding::new("down", browser_tab::SuggestNext, Some("AddressField")),
        KeyBinding::new("cmd-v", name_input::Paste, Some("AddressField > NameInput")),
        // The sidebar's rename box only; other keys type into it.
        KeyBinding::new("backspace", name_input::Backspace, Some("NameInput")),
        KeyBinding::new("cmd-backspace", name_input::Clear, Some("NameInput")),
        KeyBinding::new("enter", name_input::Confirm, Some("NameInput")),
        // The caret (2026-11-10): ← → by character, ⌥ by word, ⌘ (or Home / End) to an end,
        // ⇧ selecting; ⌦ erases forward, ⌘A selects all.
        KeyBinding::new("left", name_input::MoveLeft, Some("NameInput")),
        KeyBinding::new("right", name_input::MoveRight, Some("NameInput")),
        KeyBinding::new("alt-left", name_input::WordLeft, Some("NameInput")),
        KeyBinding::new("alt-right", name_input::WordRight, Some("NameInput")),
        KeyBinding::new("cmd-left", name_input::LineStart, Some("NameInput")),
        KeyBinding::new("cmd-right", name_input::LineEnd, Some("NameInput")),
        KeyBinding::new("home", name_input::LineStart, Some("NameInput")),
        KeyBinding::new("end", name_input::LineEnd, Some("NameInput")),
        KeyBinding::new("shift-left", name_input::SelectLeft, Some("NameInput")),
        KeyBinding::new("shift-right", name_input::SelectRight, Some("NameInput")),
        KeyBinding::new(
            "alt-shift-left",
            name_input::SelectWordLeft,
            Some("NameInput"),
        ),
        KeyBinding::new(
            "alt-shift-right",
            name_input::SelectWordRight,
            Some("NameInput"),
        ),
        KeyBinding::new(
            "cmd-shift-left",
            name_input::SelectToStart,
            Some("NameInput"),
        ),
        KeyBinding::new(
            "cmd-shift-right",
            name_input::SelectToEnd,
            Some("NameInput"),
        ),
        KeyBinding::new("delete", name_input::Delete, Some("NameInput")),
        KeyBinding::new("cmd-a", name_input::SelectAll, Some("NameInput")),
        KeyBinding::new("escape", name_input::Cancel, Some("NameInput")),
        // The SSH host picker: its box is a NameInput (Enter, Escape above); these keys
        // move through the hosts and choose.
        KeyBinding::new("up", host_picker::SelectPrev, Some("HostPicker")),
        KeyBinding::new("down", host_picker::SelectNext, Some("HostPicker")),
        KeyBinding::new("tab", host_picker::ToggleTmux, Some("HostPicker")),
        KeyBinding::new("alt-enter", host_picker::OpenWithTmux, Some("HostPicker")),
        // The command palette: its box is a NameInput too; these keys move through its list
        // and change its scope.
        KeyBinding::new("up", palette::SelectPrev, Some("Palette")),
        KeyBinding::new("down", palette::SelectNext, Some("Palette")),
        KeyBinding::new("tab", palette::NextScope, Some("Palette")),
        KeyBinding::new("shift-tab", palette::PrevScope, Some("Palette")),
        KeyBinding::new("cmd-v", name_input::Paste, Some("Palette > NameInput")),
        // The branch switcher: its box is a NameInput too; these keys move through its list.
        KeyBinding::new("up", branch_switcher::SelectPrev, Some("BranchSwitcher")),
        KeyBinding::new("down", branch_switcher::SelectNext, Some("BranchSwitcher")),
        KeyBinding::new(
            "cmd-v",
            name_input::Paste,
            Some("BranchSwitcher > NameInput"),
        ),
        // The Help panel: its box is a NameInput too; these keys scroll its list a row.
        KeyBinding::new("up", help_panel::ScrollUp, Some("HelpPanel")),
        KeyBinding::new("down", help_panel::ScrollDown, Some("HelpPanel")),
        // The right sidebar's Files panel (its tree and its filter box, a NameInput whose Enter
        // and Escape are above): move, open and close folders, open a file, back to the pane.
        // ← → only in the tree: from the box they'd change it out of sight.
        KeyBinding::new("up", right_sidebar::SelectPrev, Some("FileTree")),
        KeyBinding::new("down", right_sidebar::SelectNext, Some("FileTree")),
        KeyBinding::new(
            "right",
            right_sidebar::ExpandSelected,
            Some("FileTree && !NameInput"),
        ),
        KeyBinding::new(
            "left",
            right_sidebar::CollapseSelected,
            Some("FileTree && !NameInput"),
        ),
        KeyBinding::new("enter", right_sidebar::OpenSelected, Some("FileTree")),
        KeyBinding::new("escape", right_sidebar::BackToPane, Some("FileTree")),
        // A sidebar row's tab menu: Escape closes it.
        KeyBinding::new("escape", tab_menu::Dismiss, Some("TabMenu")),
        // The browser's address box: its box is a NameInput too; an address is pasted.
        KeyBinding::new("cmd-v", name_input::Paste, Some("AddressBox > NameInput")),
        // Settings' boxes (the Sound links, M11s).
        KeyBinding::new("cmd-v", name_input::Paste, Some("Preferences > NameInput")),
        // Only so the Edit menu shows ⌘X and ⌘A: no element in fuxx has this context, so
        // these keys never reach the actions (see `edit_menu`).
        KeyBinding::new("cmd-x", Cut, Some("EditMenuOnly")),
        KeyBinding::new("cmd-a", SelectAll, Some("EditMenuOnly")),
        // The Edit menu doesn't supply "Emoji & Symbols", so bind its shortcut here.
        KeyBinding::new(
            "ctrl-cmd-space",
            name_input::ShowCharacterPalette,
            Some("NameInput"),
        ),
    ];
    bindings.extend(browser_tab::page_bindings());
    bindings
}

/// ⌥⌘Q, "Quit fuxx and End All Sessions": bound only with persistent sessions on (`main`), and
/// listed in Keyboard Shortcuts only then.
pub(crate) fn persistent_bindings() -> [KeyBinding; 1] {
    [KeyBinding::new("alt-cmd-q", QuitAndEndSessions, None)]
}

/// The File menu's first two items; the menu shows each one's binding.
fn new_items() -> [MenuItem; 2] {
    [
        MenuItem::action("New Workspace", NewWorkspace),
        MenuItem::action("New Tab", NewTab),
    ]
}

/// The File menu: new workspaces and tabs (and each agent preset's), the workspace's own items,
/// going places, the browser, closing, Preferences.
fn file_menu(preset_items: Vec<MenuItem>) -> Menu {
    let [new_workspace, new_tab] = new_items();
    Menu::new("File").items([
        new_workspace,
        new_tab,
        MenuItem::action("New Workspace in Folder\u{2026}", NewSessionInFolder),
        MenuItem::action("New SSH Workspace\u{2026}", NewSshSession),
        MenuItem::submenu(Menu::new("New Agent Workspace").items(preset_items)),
        MenuItem::action("Rename Workspace\u{2026}", RenameSession),
        MenuItem::action("Move Workspace Up", MoveSessionUp),
        MenuItem::action("Move Workspace Down", MoveSessionDown),
        MenuItem::separator(),
        MenuItem::action("Command Palette\u{2026}", OpenPalette),
        MenuItem::action("Go to Workspace\u{2026}", OpenSwitcher),
        MenuItem::action("Go to What Needs You", NextNeedingYou),
        MenuItem::separator(),
        MenuItem::action("Open Browser\u{2026}", OpenBrowser),
        MenuItem::separator(),
        MenuItem::action("Close Tab", CloseSession),
        MenuItem::separator(),
        MenuItem::action("Preferences\u{2026}", OpenPreferences),
    ])
}

/// The View menu: the sidebar, and the font size (written to the settings file, as Preferences
/// does).
fn view_menu() -> Menu {
    Menu::new("View").items([
        MenuItem::action("Toggle Sidebar", ToggleSidebar),
        MenuItem::action("Toggle Right Sidebar", ToggleRightSidebar),
        MenuItem::separator(),
        MenuItem::action("Bigger", Bigger),
        MenuItem::action("Smaller", Smaller),
        MenuItem::action("Actual Size", ActualSize),
    ])
}

/// The Window menu: Minimize and Zoom, for the key fuxx window.
fn window_menu() -> Menu {
    Menu::new("Window").items([
        MenuItem::action("Minimize", Minimize),
        MenuItem::action("Zoom", Zoom),
    ])
}

/// Run `f` on the key fuxx window. Deferred: a menu action runs while that window is checked out
/// of the app, so it can't be updated until the action is over (see `preferences`).
fn with_key_window(cx: &mut App, f: fn(&mut gpui::Window)) {
    cx.defer(move |cx| {
        if let Some(window) = cx.active_window() {
            window.update(cx, |_, window, _| f(window)).ok();
        }
    });
}

/// Run `f` on the main fuxx window a Help item goes to (`help_menu::help_target`), brought to
/// the front if another window (Preferences, a popped-out page) is key; nothing when there is
/// none. `f` is told whether that window was already key. Deferred, as `with_key_window` is.
pub(crate) fn with_help_window(
    cx: &mut App,
    f: impl FnOnce(&mut Workspace, bool, &mut gpui::Window, &mut gpui::Context<Workspace>) + 'static,
) {
    cx.defer(move |cx| {
        let key = cx.active_window();
        let windows = cx.windows();
        let facts: Vec<(bool, bool)> = windows
            .iter()
            .map(|w| (w.downcast::<Workspace>().is_some(), Some(*w) == key))
            .collect();
        let Some(at) = help_menu::help_target(&facts) else {
            return;
        };
        let Some(handle) = windows[at].downcast::<Workspace>() else {
            return;
        };
        let was_key = facts[at].1;
        handle
            .update(cx, |workspace, window, cx| {
                if !was_key {
                    window.activate_window();
                }
                f(workspace, was_key, window, cx)
            })
            .ok();
    });
}

/// Notifications Status…'s sheet (`help_menu::status_message`) on the main fuxx window, for what
/// macOS said; its Open System Settings opens fuxx's page there.
fn show_notification_status(
    bundled: bool,
    permission: Option<native_notify::Permission>,
    cx: &mut App,
) {
    let message = help_menu::status_message(bundled, permission);
    with_help_window(cx, move |_, _, window, cx| {
        let answer = window.prompt(
            PromptLevel::Info,
            message.title,
            Some(message.detail),
            message.buttons,
            cx,
        );
        let settings = message.settings_button;
        cx.spawn(async move |_, cx| {
            if settings.is_some() && answer.await.ok() == settings {
                cx.update(|cx| cx.open_url(help_menu::NOTIFICATION_SETTINGS_URL));
            }
        })
        .detach();
    });
}

/// The File-menu item for preset `n` (0-based), bound to its `LaunchPresetN`.
fn preset_item(n: usize, name: &str) -> Option<MenuItem> {
    let name = name.to_string();
    Some(match n {
        0 => MenuItem::action(name, LaunchPreset1),
        1 => MenuItem::action(name, LaunchPreset2),
        2 => MenuItem::action(name, LaunchPreset3),
        3 => MenuItem::action(name, LaunchPreset4),
        4 => MenuItem::action(name, LaunchPreset5),
        5 => MenuItem::action(name, LaunchPreset6),
        6 => MenuItem::action(name, LaunchPreset7),
        7 => MenuItem::action(name, LaunchPreset8),
        8 => MenuItem::action(name, LaunchPreset9),
        _ => return None,
    })
}

/// Set the menu bar. `keep_running`: `None` with persistence off; else how many sessions keep
/// running after Quit, which the Quit item says, and "Quit fuxx and End All Sessions" is there.
/// Called again when that number changes (GPUI rebuilds the whole bar). After the key bindings:
/// a menu shows the shortcut bound when the menus are set.
pub(crate) fn set_menus(cx: &App, keep_running: Option<usize>) {
    let preset_items: Vec<MenuItem> = cx
        .global::<appearance::Appearance>()
        .presets
        .iter()
        .enumerate()
        .filter_map(|(n, preset)| preset_item(n, &preset.name))
        .collect();
    cx.set_menus(menu_bar(app_items(keep_running), preset_items));
    help_menu::mark_help_menu();
}

/// The fuxx menu: Check for Updates…, Hide fuxx, then Quit (see `set_menus` for `keep_running`).
fn app_items(keep_running: Option<usize>) -> Vec<MenuItem> {
    let mut items = vec![
        MenuItem::action("Check for Updates\u{2026}", CheckForUpdates),
        MenuItem::separator(),
        MenuItem::action("Hide fuxx", Hide),
        MenuItem::separator(),
        MenuItem::action(quit_title(keep_running.unwrap_or(0)), Quit),
    ];
    if keep_running.is_some() {
        items.push(MenuItem::action(
            "Quit fuxx and End All Sessions",
            QuitAndEndSessions,
        ));
    }
    items
}

/// The menu bar, left to right: fuxx · File · Edit · Pane · View · Window · Help.
fn menu_bar(app_items: Vec<MenuItem>, preset_items: Vec<MenuItem>) -> [Menu; 7] {
    [
        Menu::new("fuxx").items(app_items),
        file_menu(preset_items),
        edit_menu(),
        Menu::new("Pane").items([
            MenuItem::action("Split Right", SplitRight),
            MenuItem::action("Split Down", SplitDown),
            MenuItem::separator(),
            MenuItem::action("Previous Pane", PrevPane),
            MenuItem::action("Next Pane", NextPane),
            MenuItem::action("Focus Pane Left", FocusPaneLeft),
            MenuItem::action("Focus Pane Right", FocusPaneRight),
            MenuItem::action("Focus Pane Up", FocusPaneUp),
            MenuItem::action("Focus Pane Down", FocusPaneDown),
            MenuItem::separator(),
            MenuItem::action("Equalize Panes", EqualizePanes),
        ]),
        view_menu(),
        window_menu(),
        help_menu::menu(),
    ]
}

/// Every action item's action name in the menu bar, submenus included (each agent preset's item
/// as if all nine were set, "Quit fuxx and End All Sessions" as with persistent sessions on): what
/// the palette's parity test checks against.
#[cfg(test)]
pub(crate) fn menu_action_names() -> Vec<String> {
    fn walk(items: &[MenuItem], names: &mut Vec<String>) {
        for item in items {
            match item {
                MenuItem::Action { action, .. } => names.push(action.name().to_string()),
                MenuItem::Submenu(menu) => walk(&menu.items, names),
                MenuItem::Separator | MenuItem::SystemMenu(_) => {}
            }
        }
    }
    let presets = (0..9).filter_map(|n| preset_item(n, "preset")).collect();
    let mut names = Vec::new();
    for menu in menu_bar(app_items(Some(0)), presets) {
        walk(&menu.items, &mut names);
    }
    names
}

/// `$HOME`, unless unset or empty.
fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .filter(|h| !h.is_empty())
        .map(PathBuf::from)
}

/// `sw_vers -productVersion`, trimmed; `None` if it fails or says nothing. Blocks: call it off the
/// UI thread.
fn macos_version() -> Option<String> {
    let out = std::process::Command::new("sw_vers")
        .arg("-productVersion")
        .output()
        .ok()
        .filter(|o| o.status.success())?;
    let version = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!version.is_empty()).then_some(version)
}

/// Print an error the way every `fuxx` command does and return its exit code.
fn fail(message: &str, code: i32) -> i32 {
    eprintln!("fuxx: {}", cli::plain_text(message));
    code
}

fn client_failure(err: ClientError) -> i32 {
    match err {
        ClientError::Unreachable(message) => fail(&message, 3),
        ClientError::Api { code, message } => fail(&message, cli::exit_code(&code)),
    }
}

/// The socket `fuxx <command>` and `fuxx mcp` talk to.
fn client_socket() -> Option<PathBuf> {
    let env_socket = std::env::var("FUXX_SOCKET").ok();
    api_client::client_socket_path(env_socket.as_deref(), home_dir().as_deref())
}

/// `fuxx daemon ...` on the daemon socket (`FUXX_DAEMON_SOCKET`, else under `$HOME`).
fn run_daemon(command: cli::DaemonCommand) -> i32 {
    let env_socket = std::env::var("FUXX_DAEMON_SOCKET").ok();
    let Some(path) = daemon::client_socket_path(env_socket.as_deref(), home_dir().as_deref())
    else {
        return fail(
            "$HOME and FUXX_DAEMON_SOCKET are unset, so the daemon socket can't be found",
            3,
        );
    };
    match command {
        cli::DaemonCommand::Run => daemon::run_command(&path),
        cli::DaemonCommand::Status => daemon::status_command(&path),
        cli::DaemonCommand::Stop { force } => daemon::stop_command(&path, force),
    }
}

/// Run one `fuxx <command>` against the running app; returns the exit code.
fn run_command(command: &str, mut request: api::Request, json: bool, stream: bool) -> i32 {
    if command == "paste" {
        let mut stdin = std::io::stdin();
        if stdin.is_terminal() {
            return fail("pipe text into fuxx paste (e.g. `pbpaste | fuxx paste`)", 2);
        }
        let mut text = String::new();
        if let Err(e) = stdin.read_to_string(&mut text) {
            return fail(&format!("couldn't read standard input: {e}"), 2);
        }
        request.params["text"] = json!(text);
    }
    // The app doesn't know this shell's folder, so a relative `--folder` (or screenshot path)
    // is resolved here.
    let path_key = match command {
        "new" => Some("folder"),
        "browser screenshot" => Some("path"),
        _ => None,
    };
    if let Some(key) = path_key
        && let Some(path) = request.params[key].as_str()
        && Path::new(path).is_relative()
        && let Ok(cwd) = std::env::current_dir()
    {
        request.params[key] = json!(cwd.join(path).display().to_string());
    }
    let Some(path) = client_socket() else {
        return fail(
            "$HOME and FUXX_SOCKET are unset, so the fuxx socket can't be found",
            3,
        );
    };
    let mut out = std::io::stdout().lock();
    let mut print = |text: String| -> bool {
        text.is_empty() || writeln!(out, "{text}").and_then(|_| out.flush()).is_ok()
    };

    if !stream {
        return match api_client::call(&path, &request) {
            Ok(result) => {
                print(cli::render(command, &result, json));
                0
            }
            Err(err) => client_failure(err),
        };
    }
    // `events`: every event line until stopped. `wait`: until its one reply line.
    let mut outcome: Option<Result<Value, ClientError>> = None;
    let streamed = api_client::stream(&path, &request, |line| {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            return true;
        };
        let is_reply = value.get("result").is_some() || value.get("error").is_some();
        if !is_reply {
            return print(cli::render(command, &value, json));
        }
        match api_client::parse_reply(line) {
            // The subscription's own acknowledgement: keep reading events.
            Ok(_) if command == "events" => true,
            reply => {
                outcome = Some(reply);
                false
            }
        }
    });
    match (streamed, outcome) {
        (Err(err), _) => client_failure(err),
        (Ok(()), Some(Ok(result))) => {
            print(cli::render(command, &result, json));
            0
        }
        (Ok(()), Some(Err(err))) => client_failure(err),
        // `events` ends when fuxx quits; `wait` must have had its reply first.
        (Ok(()), None) if command == "events" => 0,
        (Ok(()), None) => fail("fuxx closed the connection without replying", 3),
    }
}

/// The scripting socket, kept for the app's lifetime; dropping it removes the socket file.
struct Scripting(#[allow(dead_code)] api_server::ServerGuard);

impl Global for Scripting {}

/// Start the scripting socket if the settings allow it; its path and requests are for the
/// workspace. Any failure is one log line, and the app runs without scripting.
fn start_scripting(cx: &mut App) -> Option<(PathBuf, UnboundedReceiver<api_server::Incoming>)> {
    if !cx.global::<appearance::Appearance>().scripting {
        eprintln!("[fuxx] scripting: off (scripting = off in the settings)");
        return None;
    }
    let Some(path) = home_dir().map(|home| api_server::socket_path(&home)) else {
        eprintln!("[fuxx] scripting: $HOME is unset; running without it");
        return None;
    };
    let (guard, requests) = match api_server::start(&path) {
        Ok(started) => started,
        Err(e) => {
            eprintln!("[fuxx] scripting: {e}; running without it");
            return None;
        }
    };
    eprintln!("[fuxx] scripting: listening on {}", path.display());
    cx.set_global(Scripting(guard));
    // GPUI doesn't drop globals on quit; take the guard so the socket file goes away.
    cx.on_app_quit(|cx| {
        drop(cx.remove_global::<Scripting>());
        async {}
    })
    .detach();
    Some((path, requests))
}

fn main() {
    // `fuxx <command>` talks to the running app and exits; only no command opens the window.
    let args: Vec<String> = std::env::args().skip(1).collect();
    // Dev-only (`ui-shots` feature): render the UI scenes off-screen to PNGs, then exit.
    #[cfg(feature = "ui-shots")]
    if args.first().map(String::as_str) == Some("__ui-shots") {
        ui_shots::run(args.get(1).map(String::as_str));
    }
    let caller = std::env::var("FUXX_SESSION")
        .ok()
        .and_then(|s| s.strip_prefix('s')?.parse().ok());
    match cli::parse(&args, caller) {
        Ok(Cli::Gui) => {}
        Ok(Cli::Help(usage)) => {
            print!("{usage}");
            std::process::exit(0);
        }
        Ok(Cli::BrowserProbe) => browser::run_probe(),
        Ok(Cli::PaneBrowserProbe) => pane_browser::run_probe(),
        Ok(Cli::Mcp) => std::process::exit(mcp::run(client_socket(), caller)),
        Ok(Cli::Daemon(command)) => std::process::exit(run_daemon(command)),
        Ok(Cli::Call {
            command,
            request,
            json,
            stream,
        }) => std::process::exit(run_command(command, request, json, stream)),
        // Already worded `fuxx <command>: …`.
        Err(message) => {
            eprintln!("{message}");
            std::process::exit(2);
        }
    }

    // From the app bundle with no terminal, the window's output goes to fuxx.log. Only here: not
    // for `fuxx <command>` or the daemon, which returned above.
    if let Some(home) = home_dir() {
        app_log::start(&home);
    }
    // Before any terminal starts (M11l): the zsh integration's scripts, written only when
    // missing or changed. A failure is logged and leaves it off.
    shell_integration::setup();
    // Clicks on fuxx's banners, heard from before the app runs (a click that opened fuxx comes
    // as it starts), wait here for the workspace (from the app bundle only).
    let (clicked, clicks) = futures::channel::mpsc::unbounded();
    native_notify::install(move |terminal| {
        let _ = clicked.unbounded_send(terminal);
    });

    // macOS apps normally keep running with no windows; a terminal should quit instead.
    application()
        .with_quit_mode(QuitMode::LastWindowClosed)
        .run(move |cx: &mut App| {
            cx.bind_keys(new_bindings());
            cx.bind_keys(key_bindings());
            // Read once, before any view exists: every view takes its theme and font from it, the
            // agent submenu is built from its presets, and `persistent-sessions` decides who
            // runs the sessions.
            cx.set_global(appearance::load(cx));
            // The key sounds (M11s): loaded now, and again whenever the settings change.
            sounds::sync(cx);
            cx.observe_global::<appearance::Appearance>(sounds::sync)
                .detach();
            // A `cargo run` build has no app package to take its Dock icon from.
            app_icon::show_in_dock();
            // Motion follows the `animations` setting and macOS Reduce motion from here on.
            reduce_motion::watch(cx);
            let persistent = cx.global::<appearance::Appearance>().persistent;
            if persistent {
                cx.bind_keys(persistent_bindings());
            }
            cx.on_action(|_: &OpenPreferences, cx: &mut App| preferences::open_or_activate(cx));
            cx.on_action(|_: &Bigger, cx: &mut App| {
                preferences::step_font_size(cx, FontStep::Bigger)
            });
            cx.on_action(|_: &Smaller, cx: &mut App| {
                preferences::step_font_size(cx, FontStep::Smaller)
            });
            cx.on_action(|_: &ActualSize, cx: &mut App| {
                preferences::step_font_size(cx, FontStep::ActualSize)
            });
            cx.on_action(|_: &Hide, cx: &mut App| cx.hide());
            // Checks now, from any build; always answers in a sheet on the main window.
            cx.on_action(|_: &CheckForUpdates, cx: &mut App| updater::check(true, cx));
            // Before any window: the sidebar reads it.
            cx.set_global(updater::Updates {
                state: updater::UpdateState::Idle,
            });
            // The Help panels go to the main fuxx window whatever window is key; asked for again,
            // the shown page closes only if that window was key.
            cx.on_action(|_: &ShowShortcuts, cx: &mut App| {
                with_help_window(cx, |workspace, was_key, window, cx| {
                    workspace.open_help(HelpPage::Shortcuts, was_key, window, cx)
                })
            });
            cx.on_action(|_: &ShowCommands, cx: &mut App| {
                with_help_window(cx, |workspace, was_key, window, cx| {
                    workspace.open_help(HelpPage::Commands, was_key, window, cx)
                })
            });
            // Read off the UI thread (it waits up to 2 s for macOS), then asked on the main
            // window in a sheet.
            cx.on_action(|_: &ShowNotificationStatus, cx: &mut App| {
                let read = cx
                    .background_executor()
                    .spawn(async { (native_notify::is_bundled(), native_notify::permission()) });
                cx.spawn(async move |cx| {
                    let (bundled, permission) = read.await;
                    cx.update(|cx| show_notification_status(bundled, permission, cx));
                })
                .detach();
            });
            // Neither needs a window. The macOS version is read off the UI thread.
            cx.on_action(|_: &ReportIssue, cx: &mut App| {
                let read = cx.background_executor().spawn(async { macos_version() });
                cx.spawn(async move |cx| {
                    let os = read.await;
                    cx.update(|cx| {
                        cx.open_url(&help_menu::issue_url(
                            env!("CARGO_PKG_VERSION"),
                            os.as_deref(),
                        ))
                    });
                })
                .detach();
            });
            cx.on_action(|_: &ShowLogs, cx: &mut App| {
                if let Some(home) = home_dir() {
                    let dir = daemon::logs_dir(&home);
                    if std::fs::create_dir_all(&dir).is_ok() {
                        cx.open_with_system(&dir);
                    }
                }
            });
            cx.on_action(|_: &ShowWelcome, cx: &mut App| {
                with_help_window(cx, |workspace, _, window, cx| {
                    workspace.open_welcome(welcome_items::Origin::Help, window, cx)
                })
            });
            cx.on_action(|_: &ShowWelcomeTips, cx: &mut App| {
                with_help_window(cx, |workspace, _, window, cx| {
                    workspace.show_welcome_tips(window, cx)
                })
            });
            cx.on_action(|_: &Minimize, cx: &mut App| {
                with_key_window(cx, |window| window.minimize_window())
            });
            cx.on_action(|_: &Zoom, cx: &mut App| {
                with_key_window(cx, |window| window.zoom_window())
            });
            let scripting = start_scripting(cx);
            set_menus(cx, persistent.then_some(0));

            let bounds = Bounds::centered(None, size(px(1280.0), px(800.0)), cx);
            let windows_before = reduce_motion::window_numbers();
            let (light_x, light_y) = TRAFFIC_LIGHTS;
            let main_window = cx
                .open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(bounds)),
                        // Integrated: no system titlebar; the traffic lights sit in the window's
                        // top band, which fuxx draws and moves the window by (see `Workspace`).
                        titlebar: Some(TitlebarOptions {
                            title: Some("fuxx".into()),
                            appears_transparent: true,
                            traffic_light_position: Some(point(px(light_x), px(light_y))),
                        }),
                        app_owns_titlebar_drag: true,
                        ..Default::default()
                    },
                    move |window, cx| {
                        cx.new(|cx| Workspace::new(scripting, clicks, persistent, window, cx))
                    },
                )
                .expect("failed to open the window");
            // The marks stop animating while this window can't be seen.
            reduce_motion::watch_window(&windows_before, cx);
            // Browser tabs' pages go in this window, over GPUI's view.
            match pane_browser::NativeWindow::find(&windows_before) {
                Some(native) => cx.set_global(native),
                None => eprintln!(
                    "[fuxx] browser: the main window wasn't found; browser tabs show no page"
                ),
            }
            // With persistence on, quitting (here, or by closing the window) only detaches: the
            // background service keeps the sessions running. Not while a quit that ends
            // sessions first is under way: that one finishes.
            cx.on_action(move |_: &Quit, cx: &mut App| {
                if main_window
                    .read(cx)
                    .is_ok_and(|workspace| workspace.quitting)
                {
                    eprintln!("[fuxx] already quitting: waiting for the sessions to end");
                } else {
                    cx.quit();
                }
            });
            // Ends every session (the background service's too), then quits.
            cx.on_action(move |_: &QuitAndEndSessions, cx: &mut App| {
                let ended =
                    main_window.update(cx, |workspace, _, cx| workspace.end_all_and_quit(cx));
                if ended.is_err() {
                    cx.quit();
                }
            });
            // The terminal window is the app: closing it quits, even with Preferences still open
            // (quitting on the last window closed alone would keep fuxx running then).
            let main_id = main_window.window_id();
            cx.on_window_closed(move |cx, id| {
                if id == main_id {
                    cx.quit();
                }
            })
            .detach();
            // A bundled fuxx asks GitHub for a newer release 10 s from now, then daily.
            updater::start_schedule(cx);
            plugins::init(cx);
            // Only now that the window has opened does an updated fuxx delete the app its
            // update replaced (`fuxx.app.previous`), so a release that crashes opening its
            // window leaves the old copy to open by hand. Its own thread, so a big folder
            // doesn't hold up the window. From the app bundle only, never `cargo run`.
            if native_notify::is_bundled()
                && let Some(app) = std::env::current_exe()
                    .ok()
                    .and_then(|exe| updater::self_update_target(&exe, |_| true))
            {
                std::thread::spawn(move || updater::clean_previous(&app));
            }
            cx.activate(true);
        });
}

/// The fuxx menu's Quit item: with persistence on, how many sessions keep running after it.
fn quit_title(keep_running: usize) -> String {
    match keep_running {
        0 => "Quit fuxx".into(),
        1 => "Quit fuxx (1 session keeps running)".into(),
        n => format!("Quit fuxx ({n} sessions keep running)"),
    }
}

#[cfg(test)]
mod tests {
    use gpui::{MenuItem, OsAction};

    use super::{
        ActualSize, Bigger, CheckForUpdates, ReportIssue, ShowCommands, ShowLogs,
        ShowNotificationStatus, ShowShortcuts, ShowWelcome, ShowWelcomeTips, Smaller, app_items,
        edit_menu, file_menu, key_bindings, menu_bar, new_bindings, new_items, quit_title,
        view_menu, window_menu,
    };
    use crate::browser_tab::{GoBack, GoForward, Reload, ResetZoom, ZoomIn, ZoomOut};
    use crate::terminal_view::{Copy, Paste};
    use crate::workspace::{
        CloseSession, EqualizePanes, NewTab, NewWorkspace, NextNeedingYou, NextPane, NextTab,
        OpenPalette, OpenSwitcher, PrevPane, PrevTab, RenameSession, SelectSession1,
        SelectSession9, SplitDown, SplitRight, ToggleRightSidebar, ToggleSidebar,
    };
    use gpui::{Action, KeyContext, Keymap, Keystroke};

    #[test]
    fn the_edit_menu_hands_its_items_to_the_os() {
        let menu = edit_menu();
        assert_eq!(menu.name.as_ref(), "Edit");
        let items: Vec<_> = menu
            .items
            .iter()
            .map(|item| match item {
                MenuItem::Action {
                    name,
                    action,
                    os_action,
                    ..
                } => (name.to_string(), action.boxed_clone(), *os_action),
                _ => panic!("only actions"),
            })
            .collect();
        let names: Vec<&str> = items.iter().map(|(n, _, _)| n.as_str()).collect();
        assert_eq!(names, ["Cut", "Copy", "Paste", "Select All"]);
        let os: Vec<Option<OsAction>> = items.iter().map(|(_, _, o)| *o).collect();
        assert!(
            os == [
                Some(OsAction::Cut),
                Some(OsAction::Copy),
                Some(OsAction::Paste),
                Some(OsAction::SelectAll)
            ]
        );
        // The terminal's own ⌘C and ⌘V stay what they were.
        assert!(items[1].1.partial_eq(&Copy));
        assert!(items[2].1.partial_eq(&Paste));
    }

    /// Each item's name.
    fn names(menu: &gpui::Menu) -> Vec<String> {
        menu.items
            .iter()
            .filter_map(|item| match item {
                MenuItem::Action { name, .. } => Some(name.to_string()),
                _ => None,
            })
            .collect()
    }

    /// Each item's name, "\u{2014}" for a separator.
    fn names_and_separators(items: &[MenuItem]) -> Vec<String> {
        items
            .iter()
            .map(|item| match item {
                MenuItem::Action { name, .. } => name.to_string(),
                MenuItem::Separator => "\u{2014}".to_string(),
                _ => panic!("only actions and separators"),
            })
            .collect()
    }

    #[test]
    fn the_fuxx_menu_checks_for_updates_above_hide() {
        let sep = "\u{2014}";
        assert_eq!(
            names_and_separators(&app_items(None)),
            [
                "Check for Updates\u{2026}",
                sep,
                "Hide fuxx",
                sep,
                "Quit fuxx"
            ]
        );
        assert_eq!(
            names_and_separators(&app_items(Some(2))),
            [
                "Check for Updates\u{2026}",
                sep,
                "Hide fuxx",
                sep,
                "Quit fuxx (2 sessions keep running)",
                "Quit fuxx and End All Sessions"
            ]
        );
        let MenuItem::Action { action, .. } = &app_items(None)[0] else {
            panic!("an action first");
        };
        assert!(action.partial_eq(&CheckForUpdates));
    }

    #[test]
    fn the_view_and_window_menus() {
        assert_eq!(
            names(&view_menu()),
            [
                "Toggle Sidebar",
                "Toggle Right Sidebar",
                "Bigger",
                "Smaller",
                "Actual Size"
            ]
        );
        assert_eq!(names(&window_menu()), ["Minimize", "Zoom"]);
    }

    #[test]
    fn the_help_menu_comes_last_with_its_ten_items() {
        let bar = menu_bar(vec![], vec![]);
        let menus: Vec<&str> = bar.iter().map(|m| m.name.as_ref()).collect();
        assert_eq!(
            menus,
            ["fuxx", "File", "Edit", "Pane", "View", "Window", "Help"]
        );
        let help = crate::help_menu::menu();
        assert_eq!(help.name.as_ref(), "Help");
        // Each item's name and action ("—" for a separator).
        let actions: [Box<dyn Action>; 7] = [
            Box::new(ShowShortcuts),
            Box::new(ShowCommands),
            Box::new(ShowNotificationStatus),
            Box::new(ShowWelcomeTips),
            Box::new(ReportIssue),
            Box::new(ShowLogs),
            Box::new(ShowWelcome),
        ];
        let items: Vec<(String, Option<usize>)> = help
            .items
            .iter()
            .map(|item| match item {
                MenuItem::Action { name, action, .. } => (
                    name.to_string(),
                    actions.iter().position(|a| action.partial_eq(&**a)),
                ),
                MenuItem::Separator => ("\u{2014}".to_string(), None),
                _ => panic!("only actions and separators"),
            })
            .collect();
        let want: Vec<(String, Option<usize>)> = [
            ("Welcome to fuxx", Some(6)),
            ("\u{2014}", None),
            ("Keyboard Shortcuts", Some(0)),
            ("fuxx Command Reference", Some(1)),
            ("\u{2014}", None),
            ("Notifications Status\u{2026}", Some(2)),
            ("Show Tips Card", Some(3)),
            ("\u{2014}", None),
            ("Report an Issue\u{2026}", Some(4)),
            ("Show Logs", Some(5)),
        ]
        .into_iter()
        .map(|(name, at)| (name.to_string(), at))
        .collect();
        assert_eq!(items, want);
    }

    #[test]
    fn up_and_down_scroll_the_help_panel_from_its_box() {
        let keymap = keymap();
        let stack = &["Workspace", "HelpPanel", "NameInput"][..];
        let up = runs(&keymap, "up", stack);
        assert!(
            up.partial_eq(&crate::help_panel::ScrollUp),
            "up: {}",
            up.name()
        );
        let down = runs(&keymap, "down", stack);
        assert!(
            down.partial_eq(&crate::help_panel::ScrollDown),
            "down: {}",
            down.name()
        );
    }

    #[test]
    fn command_slash_shows_the_shortcuts_wherever_the_focus_is() {
        let keymap = keymap();
        for stack in [
            TERMINAL,
            RENAME_BOX,
            WORKSPACE,
            PAGE,
            ADDRESS_FIELD,
            &["Workspace", "HelpPanel", "NameInput"][..],
        ] {
            let got = runs(&keymap, "cmd-/", stack);
            assert!(
                got.partial_eq(&ShowShortcuts),
                "cmd-/ in {stack:?}: {}",
                got.name()
            );
        }
    }

    #[test]
    fn command_t_is_a_new_tab_and_command_n_a_new_workspace() {
        let bindings = new_bindings();
        let of = |keys: &str| {
            let typed = [Keystroke::parse(keys).unwrap()];
            let hits: Vec<_> = bindings
                .iter()
                .filter(|b| b.match_keystrokes(&typed) == Some(false))
                .collect();
            assert_eq!(hits.len(), 1, "{keys}");
            hits[0].action().boxed_clone()
        };
        assert!(of("cmd-t").partial_eq(&NewTab));
        assert!(of("cmd-n").partial_eq(&NewWorkspace));
    }

    /// fuxx's keymap, bound as `main` binds it.
    fn keymap() -> Keymap {
        let mut bindings = new_bindings().to_vec();
        bindings.extend(key_bindings());
        Keymap::new(bindings)
    }

    /// The action GPUI runs for `keys` when what has the focus sits in key contexts `stack`
    /// (outermost first): the first binding it finds (each one found is handled where these
    /// keys go, so the first is the one that runs).
    fn runs(keymap: &Keymap, keys: &str, stack: &[&str]) -> Box<dyn Action> {
        let contexts: Vec<KeyContext> = stack
            .iter()
            .map(|c| KeyContext::parse(c).unwrap())
            .collect();
        let (found, _) = keymap.bindings_for_input(&[Keystroke::parse(keys).unwrap()], &contexts);
        found
            .first()
            .unwrap_or_else(|| panic!("{keys} in {stack:?}: nothing bound"))
            .action()
            .boxed_clone()
    }

    /// Where the focus can be: a browser tab (its page has the keyboard, or the tab itself), its
    /// address field, a terminal, the sidebar's rename box, and none of those.
    const PAGE: &[&str] = &["Workspace", "BrowserTab"];
    const ADDRESS_FIELD: &[&str] = &["Workspace", "BrowserTab", "AddressField", "NameInput"];
    const TERMINAL: &[&str] = &["Workspace", "Terminal"];
    const RENAME_BOX: &[&str] = &["Workspace", "NameInput"];
    const WORKSPACE: &[&str] = &["Workspace"];

    #[test]
    fn browser_keys_act_on_the_page_only_while_a_browser_tab_has_the_focus() {
        let keymap = keymap();
        let keys: [(&str, Box<dyn Action>, Box<dyn Action>); 7] = [
            ("cmd-r", Box::new(Reload), Box::new(RenameSession)),
            ("cmd-[", Box::new(GoBack), Box::new(PrevPane)),
            ("cmd-]", Box::new(GoForward), Box::new(NextPane)),
            ("cmd-=", Box::new(ZoomIn), Box::new(Bigger)),
            ("cmd-+", Box::new(ZoomIn), Box::new(Bigger)),
            ("cmd--", Box::new(ZoomOut), Box::new(Smaller)),
            ("cmd-0", Box::new(ResetZoom), Box::new(ActualSize)),
        ];
        for (key, page, fuxx) in keys {
            for stack in [PAGE, ADDRESS_FIELD] {
                let got = runs(&keymap, key, stack);
                assert!(got.partial_eq(&*page), "{key} in {stack:?}: {}", got.name());
            }
            for stack in [TERMINAL, RENAME_BOX, WORKSPACE] {
                let got = runs(&keymap, key, stack);
                assert!(got.partial_eq(&*fuxx), "{key} in {stack:?}: {}", got.name());
            }
        }
    }

    #[test]
    fn fuxx_keys_still_reach_fuxx_from_a_browser_tab() {
        let keymap = keymap();
        let keys: [(&str, Box<dyn Action>); 12] = [
            ("cmd-w", Box::new(CloseSession)),
            ("cmd-t", Box::new(NewTab)),
            ("cmd-n", Box::new(NewWorkspace)),
            ("cmd-d", Box::new(SplitRight)),
            ("cmd-shift-d", Box::new(SplitDown)),
            ("cmd-1", Box::new(SelectSession1)),
            ("cmd-9", Box::new(SelectSession9)),
            ("cmd-p", Box::new(OpenSwitcher)),
            ("cmd-k", Box::new(OpenPalette)),
            ("cmd-shift-u", Box::new(NextNeedingYou)),
            ("ctrl-cmd-=", Box::new(EqualizePanes)),
            ("ctrl-tab", Box::new(NextTab)),
        ];
        for (key, fuxx) in keys {
            for stack in [PAGE, ADDRESS_FIELD] {
                let got = runs(&keymap, key, stack);
                assert!(got.partial_eq(&*fuxx), "{key} in {stack:?}: {}", got.name());
            }
        }
        assert!(runs(&keymap, "ctrl-shift-tab", PAGE).partial_eq(&PrevTab));
        // And from a file tab (its own key context), ⌘K and ⌘P still open the palette.
        let file_tab: &[&str] = &["Workspace", "FileTab"];
        assert!(runs(&keymap, "cmd-k", file_tab).partial_eq(&OpenPalette));
        assert!(runs(&keymap, "cmd-p", file_tab).partial_eq(&OpenSwitcher));
    }

    #[test]
    fn shift_command_arrows_jump_between_commands_in_a_terminal() {
        use crate::terminal_view::{
            CopyCommandOutput, NextCommand, PrevCommand, ScrollPageDown, ScrollPageUp,
        };
        let keymap = keymap();
        let keys: [(&str, Box<dyn Action>); 5] = [
            ("cmd-shift-up", Box::new(PrevCommand)),
            ("cmd-shift-down", Box::new(NextCommand)),
            ("cmd-shift-c", Box::new(CopyCommandOutput)),
            // ⌘↑ / ⌘↓ stay page scroll.
            ("cmd-up", Box::new(ScrollPageUp)),
            ("cmd-down", Box::new(ScrollPageDown)),
        ];
        for (key, action) in keys {
            let got = runs(&keymap, key, TERMINAL);
            assert!(got.partial_eq(&*action), "{key}: {}", got.name());
        }
        // Only in a terminal: elsewhere ⇧⌘C is nothing of the terminal's.
        let file_tab: &[&str] = &["Workspace", "FileTab"];
        for stack in [file_tab, PAGE, WORKSPACE] {
            for key in ["cmd-shift-up", "cmd-shift-down", "cmd-shift-c"] {
                let contexts: Vec<KeyContext> = stack
                    .iter()
                    .map(|c| KeyContext::parse(c).unwrap())
                    .collect();
                let got = keymap.bindings_for_input(&[Keystroke::parse(key).unwrap()], &contexts);
                assert!(
                    got.0
                        .iter()
                        .all(|b| !b.action().name().starts_with("terminal::")),
                    "{key} in {stack:?}"
                );
            }
        }
    }

    #[test]
    fn command_v_pastes_into_the_palette_box() {
        let keymap = keymap();
        let palette_box: &[&str] = &["Workspace", "Palette", "NameInput"];
        let got = runs(&keymap, "cmd-v", palette_box);
        assert!(
            got.partial_eq(&crate::name_input::Paste),
            "cmd-v in the palette's box: {}",
            got.name()
        );
        // A terminal's ⌘V is still the terminal's.
        assert!(runs(&keymap, "cmd-v", TERMINAL).partial_eq(&Paste));
    }

    /// Settings' boxes (the Sound links, M11s) take ⌘V (your report, 2026-11-11: "textfield di
    /// settings > sound, tidak bisa paste").
    #[test]
    fn command_v_pastes_into_a_settings_box() {
        let keymap = keymap();
        let settings_box: &[&str] = &["Preferences", "NameInput"];
        assert!(runs(&keymap, "cmd-v", settings_box).partial_eq(&crate::name_input::Paste));
    }

    #[test]
    fn the_branch_switchers_keys_work_in_its_box() {
        let keymap = keymap();
        let switcher_box: &[&str] = &["Workspace", "BranchSwitcher", "NameInput"];
        assert!(runs(&keymap, "up", switcher_box).partial_eq(&crate::branch_switcher::SelectPrev));
        assert!(
            runs(&keymap, "down", switcher_box).partial_eq(&crate::branch_switcher::SelectNext)
        );
        assert!(runs(&keymap, "cmd-v", switcher_box).partial_eq(&crate::name_input::Paste));
        assert!(runs(&keymap, "enter", switcher_box).partial_eq(&crate::name_input::Confirm));
        assert!(runs(&keymap, "escape", switcher_box).partial_eq(&crate::name_input::Cancel));
    }

    #[test]
    fn option_command_backslash_toggles_the_right_sidebar_wherever_the_focus_is() {
        let keymap = keymap();
        // A terminal, a browser page, a file tab (M11g; its key context is "FileTab").
        for stack in [TERMINAL, PAGE, &["Workspace", "FileTab"][..]] {
            let got = runs(&keymap, "alt-cmd-\\", stack);
            assert!(
                got.partial_eq(&ToggleRightSidebar),
                "alt-cmd-\\ in {stack:?}: {}",
                got.name()
            );
        }
        // ⌘\ is still the left sidebar's.
        assert!(runs(&keymap, "cmd-\\", TERMINAL).partial_eq(&ToggleSidebar));
    }

    #[test]
    fn the_files_panel_keys_work_in_its_tree_and_its_filter_box() {
        use crate::name_input;
        use crate::right_sidebar::{
            BackToPane, CollapseSelected, ExpandSelected, OpenSelected, SelectNext, SelectPrev,
        };
        let keymap = keymap();
        let tree: &[&str] = &["Workspace", "FileTree"];
        let filter_box: &[&str] = &["Workspace", "FileTree", "NameInput"];
        let keys: [(&str, Box<dyn Action>); 6] = [
            ("up", Box::new(SelectPrev)),
            ("down", Box::new(SelectNext)),
            ("right", Box::new(ExpandSelected)),
            ("left", Box::new(CollapseSelected)),
            ("enter", Box::new(OpenSelected)),
            ("escape", Box::new(BackToPane)),
        ];
        for (key, action) in &keys {
            let got = runs(&keymap, key, tree);
            assert!(
                got.partial_eq(action.as_ref()),
                "{key} in the tree: {}",
                got.name()
            );
        }
        // In the box, ↑ ↓ still move through the rows; Return and Escape are the box's (it
        // opens the selected row, or gives the keyboard back to the pane).
        assert!(runs(&keymap, "up", filter_box).partial_eq(&SelectPrev));
        assert!(runs(&keymap, "down", filter_box).partial_eq(&SelectNext));
        assert!(runs(&keymap, "enter", filter_box).partial_eq(&name_input::Confirm));
        assert!(runs(&keymap, "escape", filter_box).partial_eq(&name_input::Cancel));
        let bound = |key: &str, stack: &[&str]| {
            let contexts: Vec<KeyContext> = stack
                .iter()
                .map(|c| KeyContext::parse(c).unwrap())
                .collect();
            let (found, _) =
                keymap.bindings_for_input(&[Keystroke::parse(key).unwrap()], &contexts);
            !found.is_empty()
        };
        // ← → don't open or close folders from the box (a filter's rows show every match's
        // folder open, so it would change the tree out of sight): they move its caret.
        assert!(runs(&keymap, "left", filter_box).partial_eq(&name_input::MoveLeft));
        assert!(runs(&keymap, "right", filter_box).partial_eq(&name_input::MoveRight));
        let _ = bound;
        // Not in a terminal.
        assert!(!bound("up", TERMINAL), "↑ stays the terminal's");
    }

    #[test]
    fn the_file_tab_keys_work_in_the_tab_and_its_find_box() {
        use crate::file_tab::{CloseFind, Find, FindNext, FindPrev};
        use crate::name_input;
        let keymap = keymap();
        let tab: &[&str] = &["Workspace", "FileTab"];
        let find_box: &[&str] = &["Workspace", "FileTab", "NameInput"];
        let keys: [(&str, Box<dyn Action>); 6] = [
            ("cmd-f", Box::new(Find)),
            ("cmd-g", Box::new(FindNext)),
            ("cmd-shift-g", Box::new(FindPrev)),
            ("cmd-c", Box::new(Copy)),
            ("ctrl-tab", Box::new(NextTab)),
            ("ctrl-shift-tab", Box::new(PrevTab)),
        ];
        for (key, action) in &keys {
            for stack in [tab, find_box] {
                let got = runs(&keymap, key, stack);
                assert!(
                    got.partial_eq(action.as_ref()),
                    "{key} in {stack:?}: {}",
                    got.name()
                );
            }
        }
        // Esc closes the find bar from the tab; in the box it is the box's (which closes it).
        assert!(runs(&keymap, "escape", tab).partial_eq(&CloseFind));
        assert!(runs(&keymap, "escape", find_box).partial_eq(&name_input::Cancel));
        // ⌘F, ⌘G stay out of terminals and browser tabs.
        let bound = |key: &str, stack: &[&str]| {
            let contexts: Vec<KeyContext> = stack
                .iter()
                .map(|c| KeyContext::parse(c).unwrap())
                .collect();
            let (found, _) =
                keymap.bindings_for_input(&[Keystroke::parse(key).unwrap()], &contexts);
            !found.is_empty()
        };
        for key in ["cmd-f", "cmd-g", "cmd-shift-g"] {
            assert!(!bound(key, TERMINAL), "{key} in a terminal");
            assert!(!bound(key, PAGE), "{key} in a browser tab");
        }
    }

    #[test]
    fn the_file_menu_offers_a_new_workspace_and_a_new_tab() {
        let items = new_items();
        let named: Vec<(String, bool, bool)> = items
            .iter()
            .map(|item| match item {
                MenuItem::Action { name, action, .. } => (
                    name.to_string(),
                    action.partial_eq(&NewWorkspace),
                    action.partial_eq(&NewTab),
                ),
                _ => panic!("only actions"),
            })
            .collect();
        assert_eq!(
            named,
            [
                ("New Workspace".to_string(), true, false),
                ("New Tab".to_string(), false, true)
            ]
        );
    }

    #[test]
    fn every_new_item_in_the_file_menu_says_what_it_makes() {
        let menu = file_menu(vec![]);
        let named: Vec<String> = menu
            .items
            .iter()
            .filter_map(|item| match item {
                MenuItem::Action { name, .. } => Some(name.to_string()),
                MenuItem::Submenu(menu) => Some(format!("{} ▸", menu.name)),
                _ => None,
            })
            .take(5)
            .collect();
        assert_eq!(
            named,
            [
                "New Workspace",
                "New Tab",
                "New Workspace in Folder…",
                "New SSH Workspace…",
                "New Agent Workspace ▸"
            ]
        );
    }

    #[test]
    fn the_file_menu_goes_to_the_palette_then_a_workspace_then_what_needs_you() {
        let menu = file_menu(vec![]);
        let named: Vec<(String, Box<dyn Action>)> = menu
            .items
            .iter()
            .filter_map(|item| match item {
                MenuItem::Action { name, action, .. } => {
                    Some((name.to_string(), action.boxed_clone()))
                }
                _ => None,
            })
            .collect();
        let at = named
            .iter()
            .position(|(name, _)| name == "Command Palette\u{2026}")
            .expect("Command Palette…");
        assert!(named[at].1.partial_eq(&OpenPalette));
        assert_eq!(named[at + 1].0, "Go to Workspace\u{2026}");
        assert!(named[at + 1].1.partial_eq(&OpenSwitcher));
        assert_eq!(named[at + 2].0, "Go to What Needs You");
        assert!(named[at + 2].1.partial_eq(&NextNeedingYou));
    }

    #[test]
    fn the_quit_item_says_how_many_sessions_keep_running() {
        assert_eq!(quit_title(0), "Quit fuxx");
        assert_eq!(quit_title(1), "Quit fuxx (1 session keeps running)");
        assert_eq!(quit_title(4), "Quit fuxx (4 sessions keep running)");
    }
}
