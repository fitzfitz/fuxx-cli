// fuxx — a native GPU terminal for running AI coding agents.
// Many sessions in one window, each with a live status ring and background notifications.

mod colors;
mod detector;
mod grid;
mod keys;
mod links;
mod mouse;
mod notifier;
mod pty;
mod session_list;
mod status;
mod terminal;
mod terminal_view;
mod workspace;

use gpui::{
    App, Bounds, KeyBinding, QuitMode, TitlebarOptions, WindowBounds, WindowOptions, actions,
    prelude::*, px, size,
};
use gpui_platform::application;

use terminal_view::{Copy, Paste, ScrollPageDown, ScrollPageUp};
use workspace::{
    CloseSession, NewSession, NextSession, PrevSession, SelectSession1, SelectSession2,
    SelectSession3, SelectSession4, SelectSession5, SelectSession6, SelectSession7, SelectSession8,
    SelectSession9, Workspace,
};

actions!(fuxx, [Quit]);

fn main() {
    // macOS apps normally keep running with no windows; a terminal should quit instead.
    application()
        .with_quit_mode(QuitMode::LastWindowClosed)
        .run(|cx: &mut App| {
            cx.bind_keys([
                KeyBinding::new("cmd-q", Quit, None),
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

            let bounds = Bounds::centered(None, size(px(1100.0), px(640.0)), cx);
            cx.open_window(
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
            cx.activate(true);
        });
}
