// fuxx — a native GPU terminal for running AI coding agents.
// Many sessions in one window, each with a live status ring and background notifications.

mod adopt;
mod api;
mod api_client;
mod api_events;
mod api_server;
mod appearance;
mod browser;
mod browser_history;
mod browser_js;
mod browser_tab;
mod browser_url;
mod cli;
mod colors;
mod daemon;
mod daemon_client;
mod daemon_session;
mod detector;
mod fonts;
mod git_branch;
mod grid;
mod groups;
mod host_picker;
mod keys;
mod layout_model;
mod links;
mod mcp;
mod mouse;
#[allow(dead_code)] // used by the daemon and client from M10a Task 3/4
mod mux_proto;
mod name_input;
mod notification_list;
mod notifier;
mod overlay_panel;
mod pane_browser;
mod pane_tab_menu;
mod panes;
mod preferences;
mod prefs;
mod presets;
mod private_socket;
mod pty;
mod reduce_motion;
mod restore;
mod session_list;
mod settings;
mod sidebar;
mod sidebar_toggle;
mod split_tree;
mod ssh_control;
mod ssh_hosts;
mod ssh_target;
mod status;
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
mod vt_snapshot;
mod workspace;
mod workspace_api;
mod workspace_browser;
mod workspace_daemon;
mod workspace_ssh;

use std::io::{IsTerminal, Read, Write};
use std::path::{Path, PathBuf};

use futures::channel::mpsc::UnboundedReceiver;
use gpui::{
    App, Bounds, Global, KeyBinding, Menu, MenuItem, OsAction, QuitMode, TitlebarOptions,
    WindowBounds, WindowOptions, actions, point, prelude::*, px, size,
};
use gpui_platform::application;
use serde_json::{Value, json};

use api_client::ClientError;
use cli::Cli;
use prefs::FontStep;
use ui_tokens::size::TRAFFIC_LIGHTS;

use terminal_view::{Copy, Paste, ScrollPageDown, ScrollPageUp};
use workspace::{
    CloseSession, EqualizePanes, FocusPaneDown, FocusPaneLeft, FocusPaneRight, FocusPaneUp,
    LaunchPreset1, LaunchPreset2, LaunchPreset3, LaunchPreset4, LaunchPreset5, LaunchPreset6,
    LaunchPreset7, LaunchPreset8, LaunchPreset9, MoveSessionDown, MoveSessionUp,
    NewSessionInFolder, NewSshSession, NewTab, NewWorkspace, NextNeedingYou, NextPane, NextSession,
    NextTab, OpenBrowser, OpenSwitcher, PrevPane, PrevSession, PrevTab, RenameSession,
    SelectSession1, SelectSession2, SelectSession3, SelectSession4, SelectSession5, SelectSession6,
    SelectSession7, SelectSession8, SelectSession9, SplitDown, SplitRight, ToggleSidebar,
    Workspace,
};

actions!(fuxx, [Quit, OpenPreferences, QuitAndEndSessions, Hide]);

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
fn new_bindings() -> [KeyBinding; 2] {
    [
        KeyBinding::new("cmd-t", NewTab, None),
        KeyBinding::new("cmd-n", NewWorkspace, None),
    ]
}

/// Every key binding but ⌘T / ⌘N (`new_bindings`). Order matters where two bindings of a key
/// are equally deep in the focused element's key contexts: the later one wins. So a browser
/// tab's page keys come after fuxx's own.
fn key_bindings() -> Vec<KeyBinding> {
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
        KeyBinding::new("cmd-shift-u", NextNeedingYou, None),
        KeyBinding::new("cmd-p", OpenSwitcher, None),
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
        KeyBinding::new("up", browser_tab::SuggestPrev, Some("AddressField")),
        KeyBinding::new("down", browser_tab::SuggestNext, Some("AddressField")),
        KeyBinding::new("cmd-v", name_input::Paste, Some("AddressField > NameInput")),
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
        // The session switcher: its box is a NameInput too; these keys move through it.
        KeyBinding::new("up", switcher::SelectPrev, Some("Switcher")),
        KeyBinding::new("down", switcher::SelectNext, Some("Switcher")),
        // A sidebar row's tab menu: Escape closes it.
        KeyBinding::new("escape", tab_menu::Dismiss, Some("TabMenu")),
        // The browser's address box: its box is a NameInput too; an address is pasted.
        KeyBinding::new("cmd-v", name_input::Paste, Some("AddressBox > NameInput")),
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
        MenuItem::action("Go to Session\u{2026}", OpenSwitcher),
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
    let mut app_items = vec![
        MenuItem::action("Hide fuxx", Hide),
        MenuItem::separator(),
        MenuItem::action(quit_title(keep_running.unwrap_or(0)), Quit),
    ];
    if keep_running.is_some() {
        app_items.push(MenuItem::action(
            "Quit fuxx and End All Sessions",
            QuitAndEndSessions,
        ));
    }
    cx.set_menus([
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
            // Motion follows the `animations` setting and macOS Reduce motion from here on.
            reduce_motion::watch(cx);
            let persistent = cx.global::<appearance::Appearance>().persistent;
            if persistent {
                cx.bind_keys([KeyBinding::new("alt-cmd-q", QuitAndEndSessions, None)]);
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
                        cx.new(|cx| Workspace::new(scripting, persistent, window, cx))
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
        ActualSize, Bigger, Smaller, edit_menu, file_menu, key_bindings, new_bindings, new_items,
        quit_title, view_menu, window_menu,
    };
    use crate::browser_tab::{GoBack, GoForward, Reload, ResetZoom, ZoomIn, ZoomOut};
    use crate::terminal_view::{Copy, Paste};
    use crate::workspace::{
        CloseSession, EqualizePanes, NewTab, NewWorkspace, NextNeedingYou, NextPane, NextTab,
        OpenSwitcher, PrevPane, PrevTab, RenameSession, SelectSession1, SelectSession9, SplitDown,
        SplitRight,
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

    #[test]
    fn the_view_and_window_menus() {
        assert_eq!(
            names(&view_menu()),
            ["Toggle Sidebar", "Bigger", "Smaller", "Actual Size"]
        );
        assert_eq!(names(&window_menu()), ["Minimize", "Zoom"]);
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
        let keys: [(&str, Box<dyn Action>); 11] = [
            ("cmd-w", Box::new(CloseSession)),
            ("cmd-t", Box::new(NewTab)),
            ("cmd-n", Box::new(NewWorkspace)),
            ("cmd-d", Box::new(SplitRight)),
            ("cmd-shift-d", Box::new(SplitDown)),
            ("cmd-1", Box::new(SelectSession1)),
            ("cmd-9", Box::new(SelectSession9)),
            ("cmd-p", Box::new(OpenSwitcher)),
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
    fn the_quit_item_says_how_many_sessions_keep_running() {
        assert_eq!(quit_title(0), "Quit fuxx");
        assert_eq!(quit_title(1), "Quit fuxx (1 session keeps running)");
        assert_eq!(quit_title(4), "Quit fuxx (4 sessions keep running)");
    }
}
