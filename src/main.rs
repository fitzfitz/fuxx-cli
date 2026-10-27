// fuxx — a native GPU terminal for running AI coding agents.
// Many sessions in one window, each with a live status ring and background notifications.

mod appearance;
mod colors;
mod detector;
mod fonts;
mod grid;
mod groups;
mod keys;
mod links;
mod mouse;
mod name_input;
mod notifier;
mod preferences;
mod prefs;
mod presets;
mod pty;
mod restore;
mod session_list;
mod settings;
#[allow(dead_code)] // used from M8 Task 3
mod split_tree;
mod status;
mod terminal;
mod terminal_view;
mod theme;
mod workspace;

use gpui::{
    App, Bounds, KeyBinding, Menu, MenuItem, QuitMode, TitlebarOptions, WindowBounds,
    WindowOptions, actions, prelude::*, px, size,
};
use gpui_platform::application;

use terminal_view::{Copy, Paste, ScrollPageDown, ScrollPageUp};
use workspace::{
    CloseSession, LaunchPreset1, LaunchPreset2, LaunchPreset3, LaunchPreset4, LaunchPreset5,
    LaunchPreset6, LaunchPreset7, LaunchPreset8, LaunchPreset9, MoveSessionDown, MoveSessionUp,
    NewSession, NewSessionInFolder, NextSession, PrevSession, RenameSession, SelectSession1,
    SelectSession2, SelectSession3, SelectSession4, SelectSession5, SelectSession6, SelectSession7,
    SelectSession8, SelectSession9, Workspace,
};

actions!(fuxx, [Quit, OpenPreferences]);

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

fn main() {
    // macOS apps normally keep running with no windows; a terminal should quit instead.
    application()
        .with_quit_mode(QuitMode::LastWindowClosed)
        .run(|cx: &mut App| {
            cx.bind_keys([
                KeyBinding::new("cmd-q", Quit, None),
                KeyBinding::new("cmd-,", OpenPreferences, None),
                KeyBinding::new("cmd-t", NewSession, None),
                KeyBinding::new("cmd-o", NewSessionInFolder, None),
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
                // Terminal pane only. Bindings run before the pane's own key handler.
                KeyBinding::new("cmd-c", Copy, Some("Terminal")),
                KeyBinding::new("cmd-v", Paste, Some("Terminal")),
                KeyBinding::new("shift-pageup", ScrollPageUp, Some("Terminal")),
                KeyBinding::new("cmd-up", ScrollPageUp, Some("Terminal")),
                KeyBinding::new("shift-pagedown", ScrollPageDown, Some("Terminal")),
                KeyBinding::new("cmd-down", ScrollPageDown, Some("Terminal")),
                // The sidebar's rename box only; other keys type into it.
                KeyBinding::new("backspace", name_input::Backspace, Some("NameInput")),
                KeyBinding::new("cmd-backspace", name_input::Clear, Some("NameInput")),
                KeyBinding::new("enter", name_input::Confirm, Some("NameInput")),
                KeyBinding::new("escape", name_input::Cancel, Some("NameInput")),
                // There's no Edit menu to supply "Emoji & Symbols", so bind its shortcut here.
                KeyBinding::new(
                    "ctrl-cmd-space",
                    name_input::ShowCharacterPalette,
                    Some("NameInput"),
                ),
            ]);
            cx.on_action(|_: &Quit, cx: &mut App| cx.quit());
            cx.on_action(|_: &OpenPreferences, cx: &mut App| preferences::open_or_activate(cx));
            // Read once, before any view exists: every view takes its theme and font from it, and
            // the agent submenu is built from its presets.
            let appearance = appearance::load(cx);
            let preset_items: Vec<MenuItem> = appearance
                .presets
                .iter()
                .enumerate()
                .filter_map(|(n, preset)| preset_item(n, &preset.name))
                .collect();
            cx.set_global(appearance);
            // After the key bindings: a menu shows the shortcut bound when the menus are set.
            cx.set_menus([
                Menu::new("fuxx").items([MenuItem::action("Quit fuxx", Quit)]),
                Menu::new("File").items([
                    MenuItem::action("New Session", NewSession),
                    MenuItem::action("New Session in Folder\u{2026}", NewSessionInFolder),
                    MenuItem::submenu(Menu::new("New Agent Session").items(preset_items)),
                    MenuItem::action("Rename Session\u{2026}", RenameSession),
                    MenuItem::action("Move Session Up", MoveSessionUp),
                    MenuItem::action("Move Session Down", MoveSessionDown),
                    MenuItem::separator(),
                    MenuItem::action("Close Session", CloseSession),
                    MenuItem::separator(),
                    MenuItem::action("Preferences\u{2026}", OpenPreferences),
                ]),
            ]);

            let bounds = Bounds::centered(None, size(px(1100.0), px(640.0)), cx);
            let main_window = cx
                .open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(bounds)),
                        titlebar: Some(TitlebarOptions {
                            title: Some("fuxx".into()),
                            ..Default::default()
                        }),
                        ..Default::default()
                    },
                    |window, cx| cx.new(|cx| Workspace::new(window, cx)),
                )
                .expect("failed to open the window");
            // The terminal window is the app: closing it quits, even with Preferences still open
            // (quitting on the last window closed alone would keep fuxx running then).
            let main_id = main_window.window_id();
            cx.on_window_closed(move |cx, id| {
                if id == main_id {
                    cx.quit();
                }
            })
            .detach();
            cx.activate(true);
        });
}
