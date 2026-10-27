// fuxx — a native GPU terminal for running AI coding agents.
// Many sessions in one window, each with a live status ring and background notifications.

mod appearance;
mod colors;
mod detector;
mod fonts;
mod grid;
mod keys;
mod links;
mod mouse;
mod notifier;
mod preferences;
mod prefs;
mod pty;
mod session_list;
mod settings;
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
    CloseSession, NewSession, NextSession, PrevSession, SelectSession1, SelectSession2,
    SelectSession3, SelectSession4, SelectSession5, SelectSession6, SelectSession7, SelectSession8,
    SelectSession9, Workspace,
};

actions!(fuxx, [Quit, OpenPreferences]);

fn main() {
    // macOS apps normally keep running with no windows; a terminal should quit instead.
    application()
        .with_quit_mode(QuitMode::LastWindowClosed)
        .run(|cx: &mut App| {
            cx.bind_keys([
                KeyBinding::new("cmd-q", Quit, None),
                KeyBinding::new("cmd-,", OpenPreferences, None),
                KeyBinding::new("cmd-t", NewSession, None),
                KeyBinding::new("cmd-w", CloseSession, None),
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
                // Terminal pane only. Bindings run before the pane's own key handler.
                KeyBinding::new("cmd-c", Copy, Some("Terminal")),
                KeyBinding::new("cmd-v", Paste, Some("Terminal")),
                KeyBinding::new("shift-pageup", ScrollPageUp, Some("Terminal")),
                KeyBinding::new("cmd-up", ScrollPageUp, Some("Terminal")),
                KeyBinding::new("shift-pagedown", ScrollPageDown, Some("Terminal")),
                KeyBinding::new("cmd-down", ScrollPageDown, Some("Terminal")),
            ]);
            cx.on_action(|_: &Quit, cx: &mut App| cx.quit());
            cx.on_action(|_: &OpenPreferences, cx: &mut App| preferences::open_or_activate(cx));
            // After the key bindings: a menu shows the shortcut bound when the menus are set.
            cx.set_menus([
                Menu::new("fuxx").items([MenuItem::action("Quit fuxx", Quit)]),
                Menu::new("File").items([
                    MenuItem::action("New Session", NewSession),
                    MenuItem::action("Close Session", CloseSession),
                    MenuItem::separator(),
                    MenuItem::action("Preferences\u{2026}", OpenPreferences),
                ]),
            ]);

            // Read once, before any view exists: every view takes its theme and font from it.
            let appearance = appearance::load(cx);
            cx.set_global(appearance);

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
