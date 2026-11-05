// fuxx — a native GPU terminal for running AI coding agents.
// Many sessions in one window, each with a live status ring and background notifications.

mod adopt;
mod api;
mod api_client;
mod api_events;
mod api_server;
mod appearance;
mod cli;
mod colors;
mod daemon;
mod daemon_client;
mod daemon_session;
mod detector;
mod fonts;
mod grid;
mod groups;
mod host_picker;
mod keys;
mod links;
mod mcp;
mod mouse;
#[allow(dead_code)] // used by the daemon and client from M10a Task 3/4
mod mux_proto;
mod name_input;
mod notifier;
mod panes;
mod preferences;
mod prefs;
mod presets;
mod private_socket;
mod pty;
mod restore;
mod session_list;
mod settings;
mod split_tree;
mod ssh_control;
mod ssh_hosts;
mod ssh_target;
mod status;
mod terminal;
mod terminal_view;
mod theme;
mod vt_snapshot;
mod workspace;
mod workspace_api;
mod workspace_daemon;
mod workspace_ssh;

use std::io::{IsTerminal, Read, Write};
use std::path::{Path, PathBuf};

use futures::channel::mpsc::UnboundedReceiver;
use gpui::{
    App, Bounds, Global, KeyBinding, Menu, MenuItem, QuitMode, TitlebarOptions, WindowBounds,
    WindowOptions, actions, prelude::*, px, size,
};
use gpui_platform::application;
use serde_json::{Value, json};

use api_client::ClientError;
use cli::Cli;

use terminal_view::{Copy, Paste, ScrollPageDown, ScrollPageUp};
use workspace::{
    CloseSession, EqualizePanes, FocusPaneDown, FocusPaneLeft, FocusPaneRight, FocusPaneUp,
    LaunchPreset1, LaunchPreset2, LaunchPreset3, LaunchPreset4, LaunchPreset5, LaunchPreset6,
    LaunchPreset7, LaunchPreset8, LaunchPreset9, MoveSessionDown, MoveSessionUp, NewSession,
    NewSessionInFolder, NewSshSession, NextPane, NextSession, PrevPane, PrevSession, RenameSession,
    SelectSession1, SelectSession2, SelectSession3, SelectSession4, SelectSession5, SelectSession6,
    SelectSession7, SelectSession8, SelectSession9, SplitDown, SplitRight, Workspace,
};

actions!(fuxx, [Quit, OpenPreferences, QuitAndEndSessions]);

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
    let mut app_items = vec![MenuItem::action(
        quit_title(keep_running.unwrap_or(0)),
        Quit,
    )];
    if keep_running.is_some() {
        app_items.push(MenuItem::action(
            "Quit fuxx and End All Sessions",
            QuitAndEndSessions,
        ));
    }
    cx.set_menus([
        Menu::new("fuxx").items(app_items),
        Menu::new("File").items([
            MenuItem::action("New Session", NewSession),
            MenuItem::action("New Session in Folder\u{2026}", NewSessionInFolder),
            MenuItem::action("New SSH Session\u{2026}", NewSshSession),
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
    // The app doesn't know this shell's folder, so a relative `--folder` is resolved here.
    if command == "new"
        && let Some(folder) = request.params["folder"].as_str()
        && Path::new(folder).is_relative()
        && let Ok(cwd) = std::env::current_dir()
    {
        request.params["folder"] = json!(cwd.join(folder).display().to_string());
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
    let caller = std::env::var("FUXX_SESSION")
        .ok()
        .and_then(|s| s.strip_prefix('s')?.parse().ok());
    match cli::parse(&args, caller) {
        Ok(Cli::Gui) => {}
        Ok(Cli::Help(usage)) => {
            print!("{usage}");
            std::process::exit(0);
        }
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

    // macOS apps normally keep running with no windows; a terminal should quit instead.
    application()
        .with_quit_mode(QuitMode::LastWindowClosed)
        .run(move |cx: &mut App| {
            cx.bind_keys([
                KeyBinding::new("cmd-q", Quit, None),
                KeyBinding::new("cmd-,", OpenPreferences, None),
                KeyBinding::new("cmd-t", NewSession, None),
                KeyBinding::new("cmd-o", NewSessionInFolder, None),
                KeyBinding::new("cmd-shift-s", NewSshSession, None),
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
                // The SSH host picker: its box is a NameInput (Enter, Escape above); these keys
                // move through the hosts and choose.
                KeyBinding::new("up", host_picker::SelectPrev, Some("HostPicker")),
                KeyBinding::new("down", host_picker::SelectNext, Some("HostPicker")),
                KeyBinding::new("tab", host_picker::ToggleTmux, Some("HostPicker")),
                KeyBinding::new("alt-enter", host_picker::OpenWithTmux, Some("HostPicker")),
                // There's no Edit menu to supply "Emoji & Symbols", so bind its shortcut here.
                KeyBinding::new(
                    "ctrl-cmd-space",
                    name_input::ShowCharacterPalette,
                    Some("NameInput"),
                ),
            ]);
            // Read once, before any view exists: every view takes its theme and font from it, the
            // agent submenu is built from its presets, and `persistent-sessions` decides who
            // runs the sessions.
            cx.set_global(appearance::load(cx));
            let persistent = cx.global::<appearance::Appearance>().persistent;
            if persistent {
                cx.bind_keys([KeyBinding::new("alt-cmd-q", QuitAndEndSessions, None)]);
            }
            cx.on_action(|_: &OpenPreferences, cx: &mut App| preferences::open_or_activate(cx));
            let scripting = start_scripting(cx);
            set_menus(cx, persistent.then_some(0));

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
                    move |window, cx| {
                        cx.new(|cx| Workspace::new(scripting, persistent, window, cx))
                    },
                )
                .expect("failed to open the window");
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
    use super::quit_title;

    #[test]
    fn the_quit_item_says_how_many_sessions_keep_running() {
        assert_eq!(quit_title(0), "Quit fuxx");
        assert_eq!(quit_title(1), "Quit fuxx (1 session keeps running)");
        assert_eq!(quit_title(4), "Quit fuxx (4 sessions keep running)");
    }
}
