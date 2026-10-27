// fuxx — a native GPU terminal for running AI coding agents.
// Many sessions in one window, each with a live status ring and background notifications.

#[allow(dead_code)] // wired up by the socket server (M9 Task 5)
mod api;
mod api_client;
mod api_server;
mod appearance;
mod cli;
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
mod panes;
mod preferences;
mod prefs;
mod presets;
mod pty;
mod restore;
mod session_list;
mod settings;
mod split_tree;
mod status;
mod terminal;
mod terminal_view;
mod theme;
mod workspace;

use std::io::{IsTerminal, Read, Write};
use std::path::PathBuf;

use gpui::{
    App, Bounds, Global, KeyBinding, Menu, MenuItem, QuitMode, TitlebarOptions, WindowBounds,
    WindowOptions, actions, prelude::*, px, size,
};
use gpui_platform::application;
use serde_json::{Value, json};

use api::ApiError;
use api_client::ClientError;
use cli::Cli;

use terminal_view::{Copy, Paste, ScrollPageDown, ScrollPageUp};
use workspace::{
    CloseSession, EqualizePanes, FocusPaneDown, FocusPaneLeft, FocusPaneRight, FocusPaneUp,
    LaunchPreset1, LaunchPreset2, LaunchPreset3, LaunchPreset4, LaunchPreset5, LaunchPreset6,
    LaunchPreset7, LaunchPreset8, LaunchPreset9, MoveSessionDown, MoveSessionUp, NewSession,
    NewSessionInFolder, NextPane, NextSession, PrevPane, PrevSession, RenameSession,
    SelectSession1, SelectSession2, SelectSession3, SelectSession4, SelectSession5, SelectSession6,
    SelectSession7, SelectSession8, SelectSession9, SplitDown, SplitRight, Workspace,
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

/// `$HOME`, unless unset or empty.
fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .filter(|h| !h.is_empty())
        .map(PathBuf::from)
}

/// Print an error the way every `fuxx` command does and return its exit code.
fn fail(message: &str, code: i32) -> i32 {
    eprintln!("fuxx: {message}");
    code
}

fn client_failure(err: ClientError) -> i32 {
    match err {
        ClientError::Unreachable(message) => fail(&message, 3),
        ClientError::Api { code, message } => fail(&message, cli::exit_code(&code)),
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
    let env_socket = std::env::var("FUXX_SOCKET").ok();
    let Some(path) = api_client::client_socket_path(env_socket.as_deref(), home_dir().as_deref())
    else {
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

/// Start the scripting socket if the settings allow it. Any failure is one log line, and the
/// app runs without scripting.
fn start_scripting(cx: &mut App) {
    if !cx.global::<appearance::Appearance>().scripting {
        eprintln!("[fuxx] scripting: off (scripting = off in the settings)");
        return;
    }
    let Some(path) = home_dir().map(|home| api_server::socket_path(&home)) else {
        eprintln!("[fuxx] scripting: $HOME is unset; running without it");
        return;
    };
    let (guard, requests) = match api_server::start(&path) {
        Ok(started) => started,
        Err(e) => {
            eprintln!("[fuxx] scripting: {e}; running without it");
            return;
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

    // TEMPORARY (M9 Task 5 replaces this with the workspace handling requests): answer pings
    // so `fuxx ping` works, and everything else with `unavailable`.
    std::thread::spawn(move || {
        for incoming in futures::executor::block_on_stream(requests) {
            let result = if incoming.request.method == "system.ping" {
                Ok(json!({ "pong": true }))
            } else {
                Err(ApiError::unavailable(
                    "this build of fuxx doesn't handle that request yet",
                ))
            };
            incoming
                .conn
                .send_line(&api::reply(incoming.request.id, result));
        }
    });
}

fn main() {
    // `fuxx <command>` talks to the running app and exits; only no command opens the window.
    let args: Vec<String> = std::env::args().skip(1).collect();
    let caller = std::env::var("FUXX_SESSION")
        .ok()
        .and_then(|s| s.strip_prefix('s')?.parse().ok());
    match cli::parse(&args, caller) {
        Ok(Cli::Gui) => {}
        Ok(Cli::Help(usage)) => {
            print!("{usage}");
            std::process::exit(0);
        }
        Ok(Cli::Mcp) => {
            // M9 Task 8.
            std::process::exit(fail("mcp: not yet available in this build", 2));
        }
        Ok(Cli::Call {
            request,
            json,
            stream,
        }) => std::process::exit(run_command(&args[0], request, json, stream)),
        // Already worded `fuxx <command>: …`.
        Err(message) => {
            eprintln!("{message}");
            std::process::exit(2);
        }
    }

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
                KeyBinding::new("cmd-d", SplitRight, None),
                KeyBinding::new("cmd-shift-d", SplitDown, None),
                KeyBinding::new("cmd-alt-left", FocusPaneLeft, None),
                KeyBinding::new("cmd-alt-right", FocusPaneRight, None),
                KeyBinding::new("cmd-alt-up", FocusPaneUp, None),
                KeyBinding::new("cmd-alt-down", FocusPaneDown, None),
                KeyBinding::new("cmd-[", PrevPane, None),
                KeyBinding::new("cmd-]", NextPane, None),
                KeyBinding::new("ctrl-cmd-=", EqualizePanes, None),
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
            start_scripting(cx);
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
