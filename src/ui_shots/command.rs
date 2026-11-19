//! Which command runs at a shell prompt (M11q): the marks the parser applied reach the
//! workspace as `Session::command`.

use super::*;
use crate::terminal_view::TerminalEvent;
use crate::workspace_ssh::Held;

/// This file, as `file!()` gives it (repo-relative under cargo): `registry::AREAS` pairs it with
/// `SCENES`, so `--changed` runs these scenes when this file changes.
pub(super) const FILE: &str = file!();

/// This area's scenes (`registry.rs` runs them in its `ORDER`).
pub(super) const SCENES: &[Scene] = &[Scene {
    name: "command-reaches-workspace",
    run: command_reaches_workspace,
    themes: Themes::One,
    serial: false,
    kind: Kind::State,
    sources: &[
        "src/command_name.rs",
        "src/blocks.rs",
        "src/terminal.rs",
        "src/terminal_view.rs",
        "src/workspace_command.rs",
        "src/workspace_ssh.rs",
        "src/workspace_daemon.rs",
    ],
}];

const PROMPT: &str = "\x1b]133;A\x07$ \x1b]133;B\x07";

/// A start mark for `cmd` (percent-encoded as the shell integration does), none for no text.
fn started(cmd: Option<&str>) -> String {
    match cmd {
        Some(cmd) => {
            let url: String = cmd
                .bytes()
                .map(|b| match b {
                    b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' => {
                        (b as char).to_string()
                    }
                    _ => format!("%{b:02X}"),
                })
                .collect();
            format!("{PROMPT}run\r\n\x1b]133;C;cmdline_url={url}\x07")
        }
        None => format!("{PROMPT}run\r\n\x1b]133;C\x07"),
    }
}

const ENDED: &str = "done\r\n\x1b]133;D;0\x07";

/// The marks of a program's output reach the workspace the way the app takes them (`shot_output`:
/// the screen, then each mark as the view's `on_term_event` does), and `Session::command` follows:
/// - a start with `API_KEY=sk-123 npm run deploy --token abc` names `npm run deploy`;
/// - an end mark clears it; a start with no command text has none while it runs, also after a
///   start that had one; a prompt mark clears it;
/// - a held (SSH) pane's name is cleared, and a start or an end after it changes nothing;
/// - a lost background-service connection clears it.
///
/// Panics unless all of that holds.
fn command_reaches_workspace() {
    use std::cell::RefCell;
    use std::rc::Rc;

    let theme = shot_theme("Catppuccin Mocha");
    let slot: Slot = Rc::new(RefCell::new(None));
    let keep = slot.clone();
    let build = move |window: &mut Window, cx: &mut App| -> AnyView {
        cx.set_global(shot_appearance(theme, cx));
        let ws = cx.new(|cx| {
            let mut ws = Workspace::for_shots(window, cx);
            ws.sidebar.hint_dismissed = true;
            let api = std::path::Path::new(API);
            ws.open_shot_session("api", api, SHELL_TEXT, None, window, cx);
            ws.open_shot_session("web", api, SHELL_TEXT, None, window, cx);
            let i = ws.sessions.position(|s| s.label() == "api").expect("api");
            // A reconnect's drop is not news: the hold posts no banner and no sound.
            ws.sessions.get_mut(i).expect("opened").reconnected = true;
            ws.sessions.get_mut(i).expect("opened").ssh = Some(crate::ssh_target::SshTarget {
                host: "devbox".into(),
                tmux: false,
                tmux_name: None,
            });
            ws
        });
        *keep.borrow_mut() = Some(ws.clone());
        ws.into()
    };
    let acting = slot.clone();
    let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
        let ws = slotted(&acting);
        let index = |cx: &VisualTestAppContext, name: &str| {
            cx.read(|cx| ws.read(cx).sessions.position(|s| s.label() == name))
                .unwrap_or_else(|| panic!("command-reaches-workspace: a session named {name}"))
        };
        let command = |cx: &VisualTestAppContext, name: &str| -> Option<String> {
            let i = index(cx, name);
            cx.read(|cx| ws.read(cx).sessions.items()[i].command.clone())
                .map(|c| c.to_string())
        };
        let output = |cx: &mut VisualTestAppContext, name: &str, text: &str| {
            let i = index(cx, name);
            let view = cx.read(|cx| ws.read(cx).sessions.items()[i].view.clone());
            cx.update(|cx| view.update(cx, |view, cx| view.shot_output(text, cx)));
        };
        let n = "command-reaches-workspace";
        let deploy = started(Some("API_KEY=sk-123 npm run deploy --token abc"));

        assert_eq!(command(cx, "api"), None, "{n}: nothing runs at first");
        output(cx, "api", &deploy);
        assert_eq!(
            command(cx, "api").as_deref(),
            Some("npm run deploy"),
            "{n}: a start names the short form, never the line"
        );
        output(cx, "api", ENDED);
        assert_eq!(command(cx, "api"), None, "{n}: an end mark clears it");

        output(cx, "api", &deploy);
        output(cx, "api", &started(None));
        assert_eq!(
            command(cx, "api"),
            None,
            "{n}: a start with no command text has no name while it runs"
        );
        output(cx, "api", ENDED);

        output(cx, "api", &deploy);
        output(cx, "api", PROMPT);
        assert_eq!(command(cx, "api"), None, "{n}: a prompt mark clears it");

        // Held: the name is cleared, and what follows changes nothing.
        output(cx, "api", &deploy);
        assert!(command(cx, "api").is_some(), "{n}: running before the hold");
        let i = index(cx, "api");
        cx.update_window(window, |_, window, cx| {
            ws.update(cx, |ws, cx| ws.hold(i, Held::Dropped, window, cx))
        })
        .expect("the window");
        assert_eq!(command(cx, "api"), None, "{n}: a held pane has no name");
        output(cx, "api", ENDED);
        assert_eq!(command(cx, "api"), None, "{n}: an end after the hold");
        output(cx, "api", &deploy);
        assert_eq!(command(cx, "api"), None, "{n}: a start after the hold");

        // The connection to the service lost.
        output(cx, "web", &deploy);
        assert!(command(cx, "web").is_some(), "{n}: web runs before");
        let j = index(cx, "web");
        let view = cx.read(|cx| ws.read(cx).sessions.items()[j].view.clone());
        cx.update(|cx| view.update(cx, |_, cx| cx.emit(TerminalEvent::DaemonLost)));
        assert_eq!(command(cx, "web"), None, "{n}: a lost connection clears it");

        drop(ws);
        acting.borrow_mut().take();
    };
    let path = shoot_with(
        "command-reaches-workspace",
        "mocha",
        &theme,
        size(px(960.0), px(360.0)),
        build,
        act,
    );
    println!("{}", path.display());
}
