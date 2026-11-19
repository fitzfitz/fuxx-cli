//! Which command runs at a shell prompt (M11q): the marks the parser applied reach the
//! workspace as `Session::command`.

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
/// The Working stretch is far under `notifier::QUIET_UNDER`, so the Done is in the 🔔 list and
/// posts no banner. Panics unless all of that holds.
fn command_settles() {
    use crate::grid::TextScope;
    use crate::sidebar::SidebarRow;
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::time::{Duration, Instant};

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
        // The program prints `line` (and its terminal echoes it), until `seen` holds.
        let prints = |cx: &mut VisualTestAppContext,
                      line: &[u8],
                      what: &str,
                      seen: &dyn Fn(&VisualTestAppContext) -> bool| {
            let started = Instant::now();
            while !seen(cx) {
                assert!(
                    started.elapsed() < Duration::from_secs(20),
                    "{n}: {what} never came"
                );
                crate::ui_shots::prof::sleep(Duration::from_millis(50));
                cx.read(|cx| view.read(cx).write_to_program(line));
                cx.run_until_parked();
            }
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
