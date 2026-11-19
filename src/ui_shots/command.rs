//! Which command runs at a shell prompt (M11q): the marks the parser applied reach the
//! workspace as `Session::command`, name its pane, settle its status and word its banner.

use super::*;
use crate::terminal_view::TerminalEvent;
use crate::workspace_ssh::Held;

/// This file, as `file!()` gives it (repo-relative under cargo): `registry::AREAS` pairs it with
/// `SCENES`, so `--changed` runs these scenes when this file changes.
pub(super) const FILE: &str = file!();

/// This area's scenes (`registry.rs` runs them in its `ORDER`).
pub(super) const SCENES: &[Scene] = &[
    Scene {
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
    },
    Scene {
        name: "command-settles",
        run: command_settles,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: &[
            "src/status.rs",
            "src/command_name.rs",
            "src/terminal_view.rs",
            "src/workspace_command.rs",
            "src/notifier.rs",
            "src/notification_list.rs",
            "src/sidebar.rs",
        ],
    },
    Scene {
        name: "command-name",
        run: command_name,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/command_name.rs",
            "src/groups.rs",
            "src/workspace.rs",
            "src/workspace_command.rs",
            "src/sidebar.rs",
            "src/tab_bar.rs",
            "src/switcher.rs",
        ],
    },
    Scene {
        name: "command-banner",
        run: command_banner,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: &[
            "src/notifier.rs",
            "src/command_name.rs",
            "src/status.rs",
            "src/terminal_view.rs",
            "src/workspace.rs",
            "src/workspace_command.rs",
            "src/workspace_api.rs",
            "src/api_events.rs",
            "src/notification_list.rs",
        ],
    },
    Scene {
        name: "command-bell-failed",
        run: command_bell_failed,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/title_area.rs",
            "src/notification_list.rs",
            "src/notifier.rs",
            "src/workspace.rs",
            "src/workspace_command.rs",
            "src/ui_tokens.rs",
        ],
    },
];

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

/// An end mark alone: the command printed nothing.
const END_MARK: &str = "\x1b]133;D;0\x07";

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

/// `view`'s program prints `line` (and its terminal echoes it) until `seen` holds; panics, naming
/// `what`, when it hasn't after 20 s.
fn prints(
    cx: &mut VisualTestAppContext,
    view: &gpui::Entity<crate::terminal_view::TerminalView>,
    line: &[u8],
    what: &str,
    seen: &dyn Fn(&VisualTestAppContext) -> bool,
) {
    use std::time::{Duration, Instant};

    let started = Instant::now();
    while !seen(cx) {
        assert!(
            started.elapsed() < Duration::from_secs(20),
            "{what} never came"
        );
        crate::ui_shots::prof::sleep(Duration::from_millis(50));
        cx.read(|cx| view.read(cx).write_to_program(line));
        cx.run_until_parked();
    }
}

/// A command's end mark settles its terminal's status at once (M11q Task 5), on `api`, which is
/// not the active terminal (so a Done there is news). Its `cat` echoes what it is sent: that is
/// the command's output, by the app's own path (the pty, `TermEvent::Output`, the tracker).
/// - typed into, a start mark, output until it is Working; a `D;0` mark: Done with the next
///   frame, long before the silence guess could say so (its ticker never runs here: the
///   harness's clock stands still), and its row's mark is Done's;
/// - the prompt drawn after it, and more output: still Done, and the 🔔 list holds one entry,
///   about `api`;
/// - typed into again (Idle), then a start and an end mark with no output in between and under
///   3 s: Idle, and no new entry.
///
/// The end mark's Done is told with a banner however short the run (Task 7): kept in
/// `shot_banners`, never posted (`Workspace::post_banner`). Panics unless all of that holds.
fn command_settles() {
    use crate::grid::TextScope;
    use crate::sidebar::SidebarRow;
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::time::Instant;

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
            ws
        });
        *keep.borrow_mut() = Some(ws.clone());
        ws.into()
    };
    let acting = slot.clone();
    let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
        let n = "command-settles";
        let ws = slotted(&acting);
        let i = cx
            .read(|cx| ws.read(cx).sessions.position(|s| s.label() == "api"))
            .unwrap_or_else(|| panic!("{n}: a session named api"));
        let view = cx.read(|cx| ws.read(cx).sessions.items()[i].view.clone());
        let id = cx.read(|cx| ws.read(cx).sessions.items()[i].id);
        let status =
            |cx: &VisualTestAppContext| cx.read(|cx| ws.read(cx).sessions.items()[i].status);
        let entries = |cx: &VisualTestAppContext| {
            cx.read(|cx| ws.read(cx).notifications.newest_first().count())
        };
        let prints = |cx: &mut VisualTestAppContext,
                      line: &[u8],
                      what: &str,
                      seen: &dyn Fn(&VisualTestAppContext) -> bool| {
            prints(cx, &view, line, &format!("{n}: {what}"), seen)
        };

        assert_ne!(
            cx.read(|cx| ws.read(cx).sessions.active_index()),
            Some(i),
            "{n}: api is not the active terminal"
        );
        assert_eq!(status(cx), Status::Idle, "{n}: idle at first");

        // Typed into, the command starts, and its output makes it Working.
        cx.update(|cx| {
            view.update(cx, |view, cx| {
                view.type_bytes(b"x", cx);
                view.shot_output(&started(Some("pnpm build")), cx);
            })
        });
        prints(cx, b"building\n", "Working", &|cx| {
            status(cx) == Status::Working
        });
        let working_since = Instant::now();
        assert_eq!(entries(cx), 0, "{n}: nothing in the list while it works");

        // The end mark: Done with the next frame.
        cx.update(|cx| view.update(cx, |view, cx| view.shot_output(ENDED, cx)));
        settle_frames(cx, window);
        assert_eq!(
            status(cx),
            Status::Done,
            "{n}: the end mark settles it at once"
        );
        assert!(
            working_since.elapsed() < crate::status::IDLE_AFTER,
            "{n}: Done before the silence guess could say so"
        );
        let row = cx
            .read(|cx| {
                ws.read(cx)
                    .rows_in_view(Instant::now())
                    .into_iter()
                    .find_map(|row| match row {
                        SidebarRow::Workspace(w) if w.subject == Some(i) => Some(w),
                        _ => None,
                    })
            })
            .unwrap_or_else(|| panic!("{n}: api's row"));
        assert_eq!(row.status, Status::Done, "{n}: the row's mark is Done's");

        // The prompt after it, and what the program still prints: no work, no second Done.
        cx.update(|cx| view.update(cx, |view, cx| view.shot_output(PROMPT, cx)));
        prints(cx, b"later\n", "the later output", &|cx| {
            cx.read(|cx| view.read(cx).text(TextScope::Screen).contains("later"))
        });
        settle_frames(cx, window);
        assert_eq!(status(cx), Status::Done, "{n}: the prompt is not work");
        let listed: Vec<_> = cx.read(|cx| {
            (ws.read(cx).notifications.newest_first())
                .map(|e| (e.terminal, e.status))
                .collect()
        });
        assert_eq!(
            listed,
            [(Some(id), Some(Status::Done))],
            "{n}: one entry, about api's Done"
        );

        // A quick command that prints nothing, typed into an Idle terminal: Idle, no entry.
        cx.update(|cx| {
            view.update(cx, |view, cx| {
                view.type_bytes(b"y", cx);
                view.shot_output(&started(Some("true")), cx);
                view.shot_output(END_MARK, cx);
            })
        });
        settle_frames(cx, window);
        assert_eq!(status(cx), Status::Idle, "{n}: a quick silent command");
        assert_eq!(entries(cx), 1, "{n}: and no new entry");

        drop(ws);
        acting.borrow_mut().take();
    };
    let path = shoot_with(
        "command-settles",
        "mocha",
        &theme,
        size(px(960.0), px(360.0)),
        build,
        act,
    );
    println!("{}", path.display());
}

/// What a pane's name reads, in each place that shows it, for session `id`.
struct Names {
    tab: String,
    tab_tip: String,
    row: String,
    who: String,
    line_two: String,
    switcher: String,
}

/// Every word the chrome shows about the command line: all rows' words (name, line two, tip),
/// every workspace's tabs (title, tooltip), the window title and the switcher's rows.
fn chrome_words(cx: &VisualTestAppContext, ws: &gpui::Entity<Workspace>) -> Vec<String> {
    use crate::sidebar::SidebarRow;
    use std::time::Instant;

    cx.read(|cx| {
        let ws = ws.read(cx);
        let now = Instant::now();
        let mut words = Vec::new();
        for row in ws.rows(now).to_vec() {
            if let SidebarRow::Workspace(w) = row {
                words.push(w.label.clone());
                words.extend(crate::sidebar::place_lines(&w));
                words.extend(w.title.clone());
                words.push(w.who.label());
                words.extend(ws.workspace_tip(w.workspace));
            }
        }
        for w in ws.model.display_order() {
            let pane = ws.model.workspace(w).and_then(|w| ws.model.pane(w.focused));
            for tab in pane.map(|p| ws.pane_bar_tabs(p, now)).unwrap_or_default() {
                words.push(tab.title.to_string());
                words.push(tab.tip);
            }
        }
        words.extend(ws.shot_window_title().map(String::from));
        words.extend(ws.switcher_entries(cx).into_iter().map(|e| e.label));
        words
    })
}

/// A pane is named after the command running at its prompt (M11q Task 6), in a window with four
/// terminals in `fuxx-cli`: a plain one, one you named, one whose program set a title, one with an
/// agent seen in it. Each starts `API_KEY=sk-123 npm run deploy --token abc` (a start mark):
/// - the plain one's tab, row, window title (`fuxx — #1 npm run deploy`) and ⌘K row read
///   `npm run deploy`, and its row's line two starts `› npm run deploy`;
/// - the one you named keeps its name on its tab while its row's who reads the command; the
///   titled one keeps the title on its tab (its who still the command); the one with an agent
///   keeps `✳ Claude` as who;
/// - nothing the chrome shows (rows, tabs, tooltips, window title, ⌘K rows) holds `sk-123`,
///   `API_KEY`, `--token` or `abc`, though it does hold the short form;
/// - the end mark puts every name back.
///
/// Panics unless all of that holds.
fn command_name() {
    use crate::sidebar::SidebarRow;
    use crate::terminal_view::TerminalEvent;
    use crate::workspace::window_title;
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;
    use std::time::Instant;

    #[derive(Clone, Copy, Default)]
    struct Ids {
        plain: u64,
        titled: u64,
        agent: u64,
    }

    for &(slug, theme_name) in shot_themes() {
        let theme = shot_theme(theme_name);
        let slot: Slot = Rc::new(RefCell::new(None));
        let keep = slot.clone();
        let ids = Rc::new(Cell::new(Ids::default()));
        let set_ids = ids.clone();
        let build = move |window: &mut Window, cx: &mut App| -> AnyView {
            cx.set_global(shot_appearance(theme, cx));
            let ws = cx.new(|cx| {
                let mut ws = Workspace::for_shots(window, cx);
                ws.sidebar.hint_dismissed = true;
                let fuxx = Some(std::path::Path::new(super::naming::FUXX_CLI));
                let plain = ws.open_unnamed_shot_session(fuxx, SHELL_TEXT, window, cx);
                ws.open_shot_session(
                    "mine",
                    std::path::Path::new(super::naming::FUXX_CLI),
                    SHELL_TEXT,
                    None,
                    window,
                    cx,
                );
                let titled = ws.open_unnamed_shot_session(fuxx, SHELL_TEXT, window, cx);
                let agent = ws.open_unnamed_shot_session(fuxx, SHELL_TEXT, window, cx);
                set_ids.set(Ids {
                    plain,
                    titled,
                    agent,
                });
                ws.sync();
                ws.select(0, window, cx);
                ws
            });
            *keep.borrow_mut() = Some(ws.clone());
            ws.into()
        };
        let acting = slot.clone();
        let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
            let n = "command-name";
            let ws = slotted(&acting);
            let ids = ids.get();
            let mine = cx.read(|cx| {
                let w = ws.read(cx);
                w.sessions
                    .items()
                    .iter()
                    .find(|s| s.name.is_some())
                    .map(|s| s.id)
            });
            let mine = mine.unwrap_or_else(|| panic!("{n}: the named session"));
            let view_of = |cx: &VisualTestAppContext, id: u64| {
                cx.read(|cx| {
                    let ws = ws.read(cx);
                    let i = ws.index_of(id).expect("open");
                    ws.sessions.items()[i].view.clone()
                })
            };
            let output = |cx: &mut VisualTestAppContext, id: u64, text: &str| {
                let view = view_of(cx, id);
                cx.update(|cx| view.update(cx, |view, cx| view.shot_output(text, cx)));
                settle_frames(cx, window);
            };
            let names = |cx: &VisualTestAppContext, id: u64| -> Names {
                cx.read(|cx| {
                    let ws = ws.read(cx);
                    let now = Instant::now();
                    let i = ws.index_of(id).expect("open");
                    let w = ws.workspace_of(i).expect("its workspace");
                    let pane = ws.model.workspace(w).expect("it").focused;
                    let tab = ws
                        .pane_bar_tabs(ws.model.pane(pane).expect("its pane"), now)
                        .into_iter()
                        .find(|t| t.terminal == Some(id))
                        .expect("its tab");
                    let row = ws
                        .rows(now)
                        .to_vec()
                        .into_iter()
                        .find_map(|row| match row {
                            SidebarRow::Workspace(r) if r.workspace == w => Some(r),
                            _ => None,
                        })
                        .expect("its row");
                    let switcher = ws
                        .switcher_entries(cx)
                        .into_iter()
                        .find(|e| e.id == w.0)
                        .expect("its switcher row");
                    Names {
                        tab: tab.title.to_string(),
                        tab_tip: tab.tip,
                        row: row.label.clone(),
                        who: row.who.label(),
                        line_two: crate::sidebar::place_lines(&row).join(" "),
                        switcher: switcher.label,
                    }
                })
            };
            let number_of = |cx: &VisualTestAppContext, id: u64| {
                cx.read(|cx| {
                    let ws = ws.read(cx);
                    let w = ws.workspace_of(ws.index_of(id).expect("open")).expect("ws");
                    ws.workspace_number(w).expect("numbered")
                })
            };
            settle_frames(cx, window);
            let deploy = started(Some("API_KEY=sk-123 npm run deploy --token abc"));
            let short = "npm run deploy";

            // Before: the plain one's name, and its row and tab as they are.
            let before = names(cx, ids.plain);
            assert_eq!(number_of(cx, ids.plain), 1, "{n}: the plain one is #1");

            // The plain one: its short form names it everywhere.
            output(cx, ids.plain, &deploy);
            let now = names(cx, ids.plain);
            assert_eq!(now.tab, short, "{n}: the tab");
            assert_eq!(now.row, short, "{n}: the row's name (no name of yours)");
            assert_eq!(now.switcher, short, "{n}: the switcher's row");
            assert!(
                now.line_two.starts_with("\u{203a} npm run deploy"),
                "{n}: the row's line two starts with who: {:?}",
                now.line_two
            );
            assert_eq!(now.who, "\u{203a} npm run deploy", "{n}: who");
            let title = cx.read(|cx| ws.read(cx).shot_window_title().map(String::from));
            assert_eq!(
                title.as_deref(),
                Some(window_title(1, short).as_str()),
                "{n}: the window title"
            );
            assert_eq!(title.as_deref(), Some("fuxx \u{2014} #1 npm run deploy"));

            // The one you named keeps its name; the title's one its title; the agent's its glyph.
            output(cx, mine, &deploy);
            let named = names(cx, mine);
            assert_eq!(named.tab, "mine", "{n}: your name stays on its tab");
            assert_eq!(named.row, "mine", "{n}: and on its row");
            assert_eq!(
                named.who, "\u{203a} npm run deploy",
                "{n}: its who is the command"
            );

            let set_title = TerminalEvent::TitleChanged(Some("vim notes".into()));
            let view = view_of(cx, ids.titled);
            cx.update(|cx| view.update(cx, |_, cx| cx.emit(set_title)));
            output(cx, ids.titled, &deploy);
            let titled = names(cx, ids.titled);
            assert_eq!(
                titled.tab, "vim notes",
                "{n}: a program's title stays on its tab"
            );
            assert_eq!(
                titled.who, "\u{203a} npm run deploy",
                "{n}: its who is the command"
            );

            cx.update(|cx| {
                ws.update(cx, |ws, cx| {
                    ws.agents_now.insert(ids.agent, "claude".into());
                    cx.notify();
                })
            });
            output(cx, ids.agent, &deploy);
            let agent = names(cx, ids.agent);
            assert_eq!(
                agent.who, "\u{2733} Claude",
                "{n}: an agent in the terminal is who"
            );
            assert_eq!(agent.tab, short, "{n}: its tab still reads the command");

            // The probes: nothing the chrome shows holds more of the command line.
            let words = chrome_words(cx, &ws);
            assert!(
                words.iter().any(|w| w.contains(short)),
                "{n}: the probe reads the real words ({words:?})"
            );
            assert!(
                words.iter().any(|w| w.contains("vim notes")),
                "{n}: and the titled one's tab"
            );
            for secret in ["sk-123", "API_KEY", "--token", "abc"] {
                let leaks: Vec<_> = words.iter().filter(|w| w.contains(secret)).collect();
                assert!(leaks.is_empty(), "{n}: {secret} shown in {leaks:?}");
            }
            assert!(!now.tab_tip.contains("sk-123"), "{n}: the tooltip");

            // The end mark puts every name back.
            for id in [ids.plain, mine, ids.titled, ids.agent] {
                output(cx, id, ENDED);
            }
            let after = names(cx, ids.plain);
            assert_eq!(after.tab, before.tab, "{n}: the tab is back");
            assert_eq!(after.row, before.row, "{n}: the row is back");
            assert_eq!(after.who, before.who, "{n}: who is back");
            assert_eq!(after.switcher, before.switcher, "{n}: the switcher is back");
            assert_eq!(names(cx, mine).tab, "mine", "{n}: your name");
            assert_eq!(names(cx, mine).who, before.who, "{n}: its who is back");
            assert_eq!(names(cx, ids.titled).tab, "vim notes", "{n}: the title");
            assert_eq!(
                names(cx, ids.titled).who,
                before.who,
                "{n}: its who is back"
            );
            assert_ne!(names(cx, ids.agent).who, "\u{203a} npm run deploy");
            let title = cx.read(|cx| ws.read(cx).shot_window_title().map(String::from));
            assert_ne!(
                title.as_deref(),
                Some("fuxx \u{2014} #1 npm run deploy"),
                "{n}: title back"
            );

            drop(ws);
            acting.borrow_mut().take();
        };
        let path = shoot_with(
            "command-name",
            slug,
            &theme,
            size(px(960.0), px(420.0)),
            build,
            act,
        );
        println!("{}", path.display());
    }
}

/// What a step told: the banners it would have posted (kept, never posted:
/// `Workspace::post_banner`) as title, subtitle and body, its new 🔔 entries (title, message)
/// and its `notification` events (title, body), each oldest first.
#[derive(Debug, Default, PartialEq)]
struct Told {
    banners: Vec<(String, String, String)>,
    listed: Vec<(String, String)>,
    notified: Vec<(String, String)>,
}

impl Told {
    /// One notification about workspace `name` (its banner's subtitle `sub`): the title's
    /// `message`, the `body`, the 🔔 entry's words, and whether a banner went with it.
    fn one(name: &str, sub: &str, message: &str, body: &str, listed: &str, banner: bool) -> Told {
        let title = crate::notifier::banner_title(name, message);
        Told {
            banners: Vec::from_iter(banner.then(|| (title.clone(), sub.into(), body.into()))),
            listed: vec![(name.into(), listed.into())],
            notified: vec![(title, body.into())],
        }
    }

    /// Every word told.
    fn words(&self) -> Vec<&String> {
        let banners = self.banners.iter().flat_map(|(a, b, c)| [a, b, c]);
        let pairs = self.listed.iter().chain(&self.notified);
        banners.chain(pairs.flat_map(|(a, b)| [a, b])).collect()
    }
}

/// What was told since the last call (`listed`: how many 🔔 entries were counted by then).
fn told(
    cx: &mut VisualTestAppContext,
    ws: &gpui::Entity<Workspace>,
    listed: &std::cell::Cell<usize>,
) -> Told {
    cx.run_until_parked();
    let (banners, events) = cx.update(|cx| {
        ws.update(cx, |ws, _| {
            (
                std::mem::take(&mut ws.shot_banners),
                std::mem::take(&mut ws.shot_events),
            )
        })
    });
    let notified = events
        .iter()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("an event line"))
        .filter(|event| event["event"] == "notification")
        .map(|event| {
            let text = |key: &str| event[key].as_str().expect("its text").to_string();
            (text("title"), text("body"))
        })
        .collect();
    let all: Vec<(String, String)> = cx.read(|cx| {
        (ws.read(cx).notifications.newest_first())
            .map(|e| (e.title.clone(), e.message.clone()))
            .collect()
    });
    let new = all.len() - listed.replace(all.len());
    let mut fresh: Vec<_> = all.into_iter().take(new).collect();
    fresh.reverse();
    Told {
        banners,
        listed: fresh,
        notified,
    }
}

/// The banner, the 🔔 entry and the `notification` event say which command ended and how, once
/// per ending (M11q Task 7), in a window you are at, with three terminals in workspaces of their
/// own: `front` (the one you look at), `fuxx-cli` and `tests` (in the background). A command
/// starts by its start mark (the parser's short form); it ends by the view's own `Command`
/// event with a chosen `ran` (the clock can't be turned), or by a real end mark where the
/// length doesn't matter. Nothing is posted: the banners are read from `shot_banners`.
/// - `sleep 8; true` ends with exit 0 after 8 s: one banner `fuxx-cli — sleep 8 …`, `#2`,
///   `Done in 8s`; one entry `sleep 8 … · Done in 8s`; one event with the banner's title and
///   body; its tracker's own Done after it tells nothing;
/// - exit 3 after 4 s: `Failed (exit 3) after 4s`; exit 130 on `front`: Done, and nothing told;
/// - a command that asks (Waiting: its banner names it), then ends: one more banner, the ending;
/// - `API_KEY=sk-123 npm run deploy --token abc`, a real end mark: one banner, and nothing told
///   holds `sk-123`, `API_KEY`, `--token` or `abc`;
/// - `claude` works 4 min and goes quiet (the silence guess's Done): one banner
///   `fuxx-cli — claude`, `Worked for 4m`; its end mark then: no second banner, one more entry
///   and event; a second end mark, and another start and end with no keystroke: nothing more;
/// - the same having worked 5 s (under `QUIET_UNDER`): the silence Done is listed with no
///   banner, the end mark posts the one banner;
/// - output, a Done by silence (a banner), more output (Working again), the end mark: one
///   banner for the ending; its tracker's Done after it, and a background service's Working and
///   Done again after the prompt: no banner more;
/// - no marks at all (bash): Done after 20 s of work is today's banner, `fuxx-cli — Finished`;
/// - on `tests`, by the app's own path (typed into, a start mark, output until it works, typed
///   ahead, the end mark, the next start mark), the view tells its owner in this order: the
///   command started, Working, the command ended, Done, Idle, the next command started; and
///   that ending posts one banner.
///
/// Panics unless all of that holds.
fn command_banner() {
    use crate::command_name::{Change, Ending, Finished};
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;
    use std::time::{Duration, Instant};

    const BACK: &str = "fuxx-cli";
    const SUB: &str = "#2";

    let theme = shot_theme("Catppuccin Mocha");
    let slot: Slot = Rc::new(RefCell::new(None));
    let keep = slot.clone();
    let build = move |window: &mut Window, cx: &mut App| -> AnyView {
        cx.set_global(shot_appearance(theme, cx));
        let ws = cx.new(|cx| {
            let mut ws = Workspace::for_shots(window, cx);
            ws.sidebar.hint_dismissed = true;
            let folder = std::path::Path::new(super::naming::FUXX_CLI);
            for name in ["front", BACK, "tests"] {
                ws.open_shot_session(name, folder, SHELL_TEXT, None, window, cx);
            }
            let front = ws.sessions.position(|s| s.label() == "front");
            ws.select(front.expect("front"), window, cx);
            // You are at the window (an off-screen one is never active): its active terminal
            // is the one you look at, the others are in the background.
            ws.window_active = true;
            ws
        });
        *keep.borrow_mut() = Some(ws.clone());
        ws.into()
    };
    let acting = slot.clone();
    let act = move |cx: &mut VisualTestAppContext, _: AnyWindowHandle| {
        let n = "command-banner";
        let ws = slotted(&acting);
        let index = |cx: &VisualTestAppContext, name: &str| {
            cx.read(|cx| {
                ws.read(cx)
                    .sessions
                    .position(|s| s.name.as_deref() == Some(name))
            })
            .unwrap_or_else(|| panic!("{n}: a session named {name}"))
        };
        let view_of = |cx: &VisualTestAppContext, name: &str| {
            let i = index(cx, name);
            cx.read(|cx| ws.read(cx).sessions.items()[i].view.clone())
        };
        let status = |cx: &VisualTestAppContext, name: &str| {
            let i = index(cx, name);
            cx.read(|cx| ws.read(cx).sessions.items()[i].status)
        };
        let output = |cx: &mut VisualTestAppContext, name: &str, text: &str| {
            let view = view_of(cx, name);
            cx.update(|cx| view.update(cx, |view, cx| view.shot_output(text, cx)));
        };
        // The status its tracker (or the background service) found.
        let reports = |cx: &mut VisualTestAppContext, name: &str, status: Status| {
            let view = view_of(cx, name);
            cx.update(|cx| view.update(cx, |view, cx| view.shot_status(status, cx)));
        };
        // Typed into (what it showed is seen: Idle), then `line` starts, if any.
        let starts = |cx: &mut VisualTestAppContext, name: &str, line: Option<&str>| {
            reports(cx, name, Status::Idle);
            if let Some(line) = line {
                output(cx, name, &started(Some(line)));
            }
        };
        // Its output made it work, `secs` ago, and then it went quiet: the silence guess's Done.
        let worked_then_quiet = |cx: &mut VisualTestAppContext, name: &str, secs: u64| {
            reports(cx, name, Status::Working);
            let (i, view) = (index(cx, name), view_of(cx, name));
            cx.update(|cx| {
                ws.update(cx, |ws, _| {
                    let s = ws.sessions.get_mut(i).expect("open");
                    s.status_since = Instant::now() - Duration::from_secs(secs);
                });
                view.update(cx, |view, cx| view.shot_status(Status::Done, cx));
            });
        };
        // The running command ends after `ran` seconds: the view's event, as its end mark makes
        // it, with the short form the parser gave.
        let ends = |cx: &mut VisualTestAppContext, name: &str, exit: Option<i32>, ran: u64| {
            let (i, view) = (index(cx, name), view_of(cx, name));
            let short = cx.read(|cx| ws.read(cx).sessions.items()[i].command.clone());
            let finished = Finished {
                short: short.map(|s| s.to_string()),
                ending: Ending::of(exit),
                ran: Duration::from_secs(ran),
            };
            let ended = TerminalEvent::Command(Change::Ended(finished));
            cx.update(|cx| view.update(cx, |_, cx| cx.emit(ended)));
        };
        let listed = Cell::new(0);
        let told = |cx: &mut VisualTestAppContext| told(cx, &ws, &listed);
        let nothing = Told::default();

        let (front, back) = (index(cx, "front"), index(cx, BACK));
        cx.read(|cx| {
            let ws = ws.read(cx);
            assert_eq!(ws.sessions.active_index(), Some(front), "{n}: front is it");
            assert!(ws.window_active, "{n}: you are at the window");
            let number = ws.workspace_of(back).and_then(|w| ws.workspace_number(w));
            assert_eq!(number, Some(2), "{n}: {BACK} is #2");
        });

        // `sleep 8 …`: silent, so Idle until its end mark.
        starts(cx, BACK, Some("sleep 8; true"));
        assert_eq!(told(cx), nothing, "{n}: a start tells nothing");
        ends(cx, BACK, Some(0), 8);
        assert_eq!(
            told(cx),
            Told::one(
                BACK,
                SUB,
                "sleep 8 \u{2026}",
                "Done in 8s",
                "sleep 8 \u{2026} \u{b7} Done in 8s",
                true
            ),
            "{n}: one banner, one entry, one event for the ending"
        );
        assert_eq!(status(cx, BACK), Status::Done, "{n}: and it is Done");
        reports(cx, BACK, Status::Done);
        assert_eq!(told(cx), nothing, "{n}: its tracker's own Done is no news");

        // A failure; and a stop on the terminal you look at.
        starts(cx, BACK, Some("cargo test --release"));
        ends(cx, BACK, Some(3), 4);
        let failed = "Failed (exit 3) after 4s";
        assert_eq!(
            told(cx),
            Told::one(
                BACK,
                SUB,
                "cargo test",
                failed,
                &format!("cargo test \u{b7} {failed}"),
                true
            ),
            "{n}: a failed command"
        );
        starts(cx, "front", Some("pnpm dev"));
        ends(cx, "front", Some(130), 3840);
        assert_eq!(status(cx, "front"), Status::Done, "{n}: front settled");
        assert_eq!(told(cx), nothing, "{n}: nothing about what you look at");

        // A command that asks, then ends: Waiting settles too (no Working stretch ends there).
        starts(cx, BACK, Some("cargo publish"));
        reports(cx, BACK, Status::Waiting);
        let asked = told(cx);
        assert_eq!(asked.banners.len(), 1, "{n}: the question's banner");
        assert_eq!(
            asked.banners[0].0, "fuxx-cli \u{2014} cargo publish",
            "{n}: the question's banner names the command"
        );
        ends(cx, BACK, Some(0), 12);
        assert_eq!(
            told(cx),
            Told::one(
                BACK,
                SUB,
                "cargo publish",
                "Done in 12s",
                "cargo publish \u{b7} Done in 12s",
                true
            ),
            "{n}: a waiting command's end"
        );

        // The token line, by a real end mark: only its short form is told.
        starts(cx, BACK, Some("API_KEY=sk-123 npm run deploy --token abc"));
        reports(cx, BACK, Status::Working);
        output(cx, BACK, ENDED);
        let deployed = told(cx);
        assert_eq!(deployed.banners.len(), 1, "{n}: {deployed:?}");
        assert_eq!(
            deployed.banners[0].0, "fuxx-cli \u{2014} npm run deploy",
            "{n}: the short form is the banner's message"
        );
        assert!(
            deployed.banners[0].2.starts_with("Done in "),
            "{n}: {deployed:?}"
        );
        assert_eq!(
            (deployed.listed.len(), deployed.notified.len()),
            (1, 1),
            "{n}: {deployed:?}"
        );
        for secret in ["sk-123", "API_KEY", "--token", "abc"] {
            let leaks: Vec<_> = (deployed.words().into_iter())
                .filter(|w| w.contains(secret))
                .collect();
            assert!(leaks.is_empty(), "{n}: {secret} told in {leaks:?}");
        }

        // An agent started at the prompt: the silence guess's Done has the banner, the end mark
        // after it only the list.
        starts(cx, BACK, Some("claude"));
        worked_then_quiet(cx, BACK, 4 * 60 + 10);
        assert_eq!(
            told(cx),
            Told::one(BACK, SUB, "claude", "Worked for 4m", "Worked for 4m", true),
            "{n}: the silence guess names the running command"
        );
        ends(cx, BACK, Some(0), 5 * 60);
        assert_eq!(
            told(cx),
            Told::one(
                BACK,
                SUB,
                "claude",
                "Done in 5m",
                "claude \u{b7} Done in 5m",
                false
            ),
            "{n}: this Done had its banner: the ending is listed, not bannered again"
        );
        // The view still holds the command (its end above was the event alone): a real end mark.
        output(cx, BACK, END_MARK);
        assert_eq!(told(cx), nothing, "{n}: a second end mark");
        output(cx, BACK, &started(Some("claude")));
        ends(cx, BACK, Some(0), 60);
        assert_eq!(told(cx), nothing, "{n}: marks alone, no keystroke");

        // The same under QUIET_UNDER: the silence Done has no banner, so the end mark's goes out.
        starts(cx, BACK, Some("claude"));
        worked_then_quiet(cx, BACK, 5);
        assert_eq!(
            told(cx),
            Told::one(BACK, SUB, "claude", "Worked for 5s", "Worked for 5s", false),
            "{n}: a short run is listed without a banner"
        );
        ends(cx, BACK, Some(0), 6);
        assert_eq!(
            told(cx),
            Told::one(
                BACK,
                SUB,
                "claude",
                "Done in 6s",
                "claude \u{b7} Done in 6s",
                true
            ),
            "{n}: the ending's banner, this Done having had none"
        );
        output(cx, BACK, &started(Some("claude")));
        ends(cx, BACK, Some(0), 60);
        assert_eq!(told(cx), nothing, "{n}: and no more after it");

        // Quiet, then output again, then the end.
        starts(cx, BACK, Some("pnpm build"));
        worked_then_quiet(cx, BACK, 20);
        assert_eq!(
            told(cx),
            Told::one(
                BACK,
                SUB,
                "pnpm build",
                "Worked for 20s",
                "Worked for 20s",
                true
            ),
            "{n}: quiet for a while"
        );
        reports(cx, BACK, Status::Working);
        ends(cx, BACK, Some(0), 40);
        assert_eq!(
            told(cx),
            Told::one(
                BACK,
                SUB,
                "pnpm build",
                "Done in 40s",
                "pnpm build \u{b7} Done in 40s",
                true
            ),
            "{n}: it worked again, so its end is news with a banner"
        );
        reports(cx, BACK, Status::Done);
        assert_eq!(told(cx), nothing, "{n}: its tracker's Done after the mark");
        // A background service that doesn't know the mark yet: the prompt is work to it.
        reports(cx, BACK, Status::Working);
        reports(cx, BACK, Status::Done);
        let prompt = told(cx);
        assert!(prompt.banners.is_empty(), "{n}: no banner for the prompt");

        // No marks at all: today's words.
        starts(cx, BACK, None);
        worked_then_quiet(cx, BACK, 20);
        assert_eq!(
            told(cx),
            Told::one(
                BACK,
                SUB,
                "Finished",
                "Worked for 20s",
                "Worked for 20s",
                true
            ),
            "{n}: a terminal with no marks"
        );

        // The order the view tells its owner in, by the app's own path.
        let view = view_of(cx, "tests");
        let heard: Rc<RefCell<Vec<String>>> = Rc::default();
        let hears = heard.clone();
        let _heard = cx.update(|cx| {
            cx.subscribe(&view, move |_, event: &TerminalEvent, _| {
                let said = match event {
                    TerminalEvent::StatusChanged(status) => format!("status {}", status.as_str()),
                    TerminalEvent::Command(Change::Started(short)) => {
                        format!("started {}", short.as_deref().unwrap_or("-"))
                    }
                    TerminalEvent::Command(Change::Ended(fin)) => format!("ended {}", fin.name()),
                    TerminalEvent::Command(Change::Cleared) => "cleared".to_string(),
                    _ => return,
                };
                hears.borrow_mut().push(said);
            })
        });
        cx.update(|cx| {
            view.update(cx, |view, cx| {
                view.type_bytes(b"x", cx);
                view.shot_output(&started(Some("pnpm build")), cx);
            })
        });
        prints(cx, &view, b"building\n", &format!("{n}: Working"), &|cx| {
            status(cx, "tests") == Status::Working
        });
        // Typed ahead while it runs, its end mark, and the next command's start mark.
        cx.update(|cx| {
            view.update(cx, |view, cx| {
                view.type_bytes(b"y", cx);
                view.shot_output(ENDED, cx);
            })
        });
        cx.update(|cx| {
            view.update(cx, |view, cx| {
                view.shot_output(&started(Some("cargo test")), cx)
            })
        });
        assert_eq!(
            *heard.borrow(),
            [
                "started pnpm build",
                "status working",
                "ended pnpm build",
                "status done",
                "status idle",
                "started cargo test",
            ],
            "{n}: an end's command comes before its Done, a start's status before its command"
        );
        let ended = told(cx);
        assert_eq!(ended.banners.len(), 1, "{n}: one banner: {ended:?}");
        assert_eq!(ended.banners[0].0, "tests \u{2014} pnpm build", "{n}");
        assert!(ended.banners[0].2.starts_with("Done in "), "{n}: {ended:?}");
        assert_eq!(
            (ended.listed.len(), ended.notified.len()),
            (1, 1),
            "{n}: {ended:?}"
        );

        drop(ws);
        acting.borrow_mut().take();
    };
    let path = shoot_with(
        "command-banner",
        "mocha",
        &theme,
        size(px(960.0), px(360.0)),
        build,
        act,
    );
    println!("{}", path.display());
}

/// The 🔔 list shows a failed command in the failure colour (M11q Task 8). Two commands end in
/// the background (nothing is posted: `post_banner` records), `cargo test` failed (exit 101, 40 s)
/// and `pnpm build` done (3 min 12 s), and the open popover lists, newest first, `pnpm build ·
/// Done in 3m 12s` and `cargo test · Failed (exit 101) after 40s`, each under its workspace's name:
/// - the entries carry `failed` (the second only), and the list holds nothing else;
/// - a pixel of the failed message's text is nearer the failure ink than the usual one, the
///   other's the reverse, both read in the popover's own pixels.
///
/// Panics unless all of that holds.
fn command_bell_failed() {
    use crate::command_name::{Change, Ending, Finished};
    use crate::ui_tokens::size::{SIDEBAR_DEFAULT_WIDTH, STATUS_BAR};
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::time::Duration;

    const BACK: &str = "fuxx-cli";
    const FAILED: &str = "cargo test \u{b7} Failed (exit 101) after 40s";
    const DONE: &str = "pnpm build \u{b7} Done in 3m 12s";
    // The popover's entries: their height pieces (title_area's `ENTRY_PAD`, `LINE`, `SUB`).
    const PAD: f32 = 5.0;
    const LINE: f32 = 18.0;
    const SUB: f32 = 15.0;

    for &(slug, theme_name) in shot_themes() {
        let theme = shot_theme(theme_name);
        let ui = theme.ui();
        let dims = size(px(960.0), px(480.0));
        let slot: Slot = Rc::new(RefCell::new(None));
        let keep = slot.clone();
        let build = move |window: &mut Window, cx: &mut App| -> AnyView {
            cx.set_global(shot_appearance(theme, cx));
            let ws = cx.new(|cx| {
                let mut ws = Workspace::for_shots(window, cx);
                ws.sidebar.hint_dismissed = true;
                ws.sidebar.width = SIDEBAR_DEFAULT_WIDTH;
                let folder = std::path::Path::new(super::naming::FUXX_CLI);
                for name in ["front", BACK, "tests"] {
                    ws.open_shot_session(name, folder, SHELL_TEXT, None, window, cx);
                }
                let front = ws.sessions.position(|s| s.label() == "front");
                ws.select(front.expect("front"), window, cx);
                ws.window_active = true;
                ws
            });
            *keep.borrow_mut() = Some(ws.clone());
            ws.into()
        };
        let acting = slot.clone();
        let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
            let n = format!("command-bell-failed-{slug}");
            let ws = slotted(&acting);
            let view_of = |cx: &VisualTestAppContext, name: &str| {
                cx.read(|cx| {
                    let ws = ws.read(cx);
                    let i = ws
                        .sessions
                        .position(|s| s.name.as_deref() == Some(name))
                        .unwrap_or_else(|| panic!("{n}: a session named {name}"));
                    ws.sessions.items()[i].view.clone()
                })
            };
            // A command starts (typed into, a start mark) and ends after `ran` seconds: the
            // view's own event, as its end mark makes it.
            let runs =
                |cx: &mut VisualTestAppContext, name: &str, line: &str, exit: i32, ran: u64| {
                    let view = view_of(cx, name);
                    cx.update(|cx| {
                        view.update(cx, |view, cx| {
                            view.shot_status(Status::Idle, cx);
                            view.shot_output(&started(Some(line)), cx);
                        })
                    });
                    cx.run_until_parked();
                    let short = line
                        .split_whitespace()
                        .take(2)
                        .collect::<Vec<_>>()
                        .join(" ");
                    let finished = Finished {
                        short: Some(short),
                        ending: Ending::of(Some(exit)),
                        ran: Duration::from_secs(ran),
                    };
                    let ended = TerminalEvent::Command(Change::Ended(finished));
                    cx.update(|cx| view.update(cx, |_, cx| cx.emit(ended)));
                    cx.run_until_parked();
                };
            runs(cx, BACK, "cargo test", 101, 40);
            runs(cx, "tests", "pnpm build", 0, 192);
            cx.update_window(window, |_, window, cx| {
                ws.update(cx, |ws, cx| ws.open_notifications(window, cx))
            })
            .expect("open the popover");
            cx.run_until_parked();

            // The entries, newest first: the build, then the failed test run.
            let listed: Vec<(String, String, bool)> = cx.read(|cx| {
                (ws.read(cx).notifications.newest_first())
                    .map(|e| (e.title.clone(), e.message.clone(), e.failed))
                    .collect()
            });
            assert_eq!(
                listed,
                [
                    ("tests".to_string(), DONE.to_string(), false),
                    (BACK.to_string(), FAILED.to_string(), true),
                ],
                "{n}: the list"
            );
            let banners = cx.read(|cx| ws.read(cx).shot_banners.len());
            assert_eq!(banners, 2, "{n}: two recorded banners, none posted");
            drop(ws);
            acting.borrow_mut().take();
        };
        let shot = render_with(&theme, dims, build, act);
        let path = keep_shot(&format!("command-bell-failed-{slug}"), &shot);
        println!("{}", path.display());

        // The pixels: each entry's message line, its ink nearest the failure colour or the usual.
        let n = format!("command-bell-failed-{slug}");
        let failed_ink = crate::ui_tokens::ensure_contrast(
            ui.block_failed,
            ui.panel,
            crate::ui_tokens::TEXT_MIN,
        );
        let (left, _) = crate::title_area::popover_anchor(SIDEBAR_DEFAULT_WIDTH, 480.0, STATUS_BAR);
        let width = crate::title_area::popover_width(SIDEBAR_DEFAULT_WIDTH);
        // The most inked pixel of entry `i`'s message line (the furthest from the popover's fill).
        let message_ink = |i: usize| -> [u8; 3] {
            let top = crate::title_area::entry_top(i, 2, SIDEBAR_DEFAULT_WIDTH, 480.0) + PAD + LINE;
            let mut best = pixel_at(&shot, dims, left + 40.0, top + 2.0);
            let mut y = top + 1.0;
            while y < top + SUB - 1.0 {
                let mut x = left + 34.0;
                while x < left + width - 12.0 {
                    let p = pixel_at(&shot, dims, x, y);
                    if gap(p, ui.panel) > gap(best, ui.panel) {
                        best = p;
                    }
                    x += 0.5;
                }
                y += 0.5;
            }
            best
        };
        let (first, second) = (message_ink(0), message_ink(1));
        assert!(
            gap(second, ui.panel) > 40,
            "{n}: the failed message is drawn ({second:?})"
        );
        assert!(
            gap(first, ui.panel) > 40,
            "{n}: the done message is drawn ({first:?})"
        );
        assert!(
            gap(second, failed_ink) < gap(second, ui.text_dim),
            "{n}: the failed message is in the failure ink ({second:?})"
        );
        assert!(
            gap(first, ui.text_dim) < gap(first, failed_ink),
            "{n}: the done message is in the usual ink ({first:?})"
        );
    }
}
