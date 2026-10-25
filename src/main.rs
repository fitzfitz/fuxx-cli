mod colors;
mod keys;

// fuxx — Milestone 0: prove the GPUI toolchain by opening a blank window.
// The real terminal (alacritty_terminal in a GPUI view) arrives in Milestone 1.

use gpui::{
    App, Bounds, Context, KeyBinding, QuitMode, SharedString, TitlebarOptions, Window,
    WindowBounds, WindowOptions, actions, div, prelude::*, px, rgb, size,
};
use gpui_platform::application;

actions!(fuxx, [Quit]);

struct Fuxx {
    text: SharedString,
}

impl Render for Fuxx {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .size_full()
            .justify_center()
            .items_center()
            .bg(rgb(0x1e1e1e))
            .text_xl()
            .text_color(rgb(0xf0f0f0))
            .child(format!("{} — M0: GPUI window is alive", self.text))
    }
}

fn main() {
    // macOS apps normally keep running with no windows; a terminal should quit instead.
    application()
        .with_quit_mode(QuitMode::LastWindowClosed)
        .run(|cx: &mut App| {
            // GPUI ships no default key bindings: cmd-q does nothing until we bind it.
            cx.bind_keys([KeyBinding::new("cmd-q", Quit, None)]);
            cx.on_action(|_: &Quit, cx: &mut App| cx.quit());

            let bounds = Bounds::centered(None, size(px(800.0), px(520.0)), cx);
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    titlebar: Some(TitlebarOptions {
                        title: Some("fuxx".into()),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                |_, cx| {
                    cx.new(|_| Fuxx {
                        text: "fuxx".into(),
                    })
                },
            )
            .expect("failed to open the window");
            cx.activate(true);
        });
}
