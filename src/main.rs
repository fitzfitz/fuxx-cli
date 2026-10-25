// fuxx — a native GPU terminal for running AI coding agents.
// Milestone 1: one real terminal (alacritty_terminal + portable-pty) in a GPUI window.

mod colors;
mod grid;
mod keys;
mod pty;
mod terminal;
mod terminal_view;

use gpui::{
    App, Bounds, KeyBinding, QuitMode, TitlebarOptions, WindowBounds, WindowOptions, actions, px,
    size,
};
use gpui_platform::application;

use terminal_view::TerminalView;

actions!(fuxx, [Quit]);

fn main() {
    // macOS apps normally keep running with no windows; a terminal should quit instead.
    application()
        .with_quit_mode(QuitMode::LastWindowClosed)
        .run(|cx: &mut App| {
            cx.bind_keys([KeyBinding::new("cmd-q", Quit, None)]);
            cx.on_action(|_: &Quit, cx: &mut App| cx.quit());

            let bounds = Bounds::centered(None, size(px(900.0), px(560.0)), cx);
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    titlebar: Some(TitlebarOptions {
                        title: Some("fuxx".into()),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                |window, cx| TerminalView::spawn(window, cx).expect("failed to start the shell"),
            )
            .expect("failed to open the window");
            cx.activate(true);
        });
}
