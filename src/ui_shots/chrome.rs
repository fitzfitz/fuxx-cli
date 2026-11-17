//! M11p Task 5: every source of change reaches its cached chrome part (the user's rule: every
//! chrome part always shows current data; every source of change gets a test). One scene per
//! dependency group of note 77, plus the two frames a part asks for itself.
//!
//! Each step changes its source the way the app does (the code path the app runs, not a field
//! poked), then lets frames come only as the app would get them (`settle_frames`: what a notify
//! draws, then each frame something asked for; no explicit draw, no notify of the scene's own),
//! and checks the window as shown is what a full redraw draws (no part stale) and, where the
//! source changes what is seen, that it differs from before.
//!
//! `FUXX_SHOTS_KEEP_GOING=1`: a stale step is reported and the scene goes on, failing at its end
//! (to see every step's outcome under one deliberate break of a source).

use std::path::PathBuf;
use std::time::{Duration, Instant};

use gpui::{
    AnyWindowHandle, App, Context, Entity, Modifiers, MouseButton, Pixels, Point,
    VisualTestAppContext, Window, point, px,
};

use super::*;
use crate::split_tree::Axis;
use crate::status::Status;
use crate::terminal_view::TerminalEvent;
use crate::workspace::Workspace;

/// This file, as `file!()` gives it (repo-relative under cargo): `registry::AREAS` pairs it with
/// `SCENES`, so `--changed` runs these scenes when this file changes.
pub(super) const FILE: &str = file!();

/// This area's scenes (`registry.rs` runs them in its `ORDER`).
pub(super) const SCENES: &[Scene] = &[
    Scene {
        name: "chrome-status-elsewhere",
        run: chrome_status_elsewhere,
        themes: Themes::One,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/chrome_cache.rs",
            "src/sidebar.rs",
            "src/tab_bar.rs",
            "src/status_mark.rs",
            "src/status.rs",
        ],
    },
    Scene {
        name: "chrome-model-edits",
        run: chrome_model_edits,
        themes: Themes::One,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/chrome_cache.rs",
            "src/layout_model.rs",
            "src/sidebar.rs",
            "src/tab_bar.rs",
            "src/tab_menu.rs",
            "src/title_area.rs",
        ],
    },
    Scene {
        name: "chrome-notifications",
        run: chrome_notifications,
        themes: Themes::One,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/chrome_cache.rs",
            "src/notification_list.rs",
            "src/title_area.rs",
            "src/sidebar.rs",
        ],
    },
    Scene {
        name: "chrome-places-programs-ports",
        run: chrome_places_programs_ports,
        themes: Themes::One,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/chrome_cache.rs",
            "src/git_branch.rs",
            "src/procs.rs",
            "src/sidebar.rs",
            "src/status_bar.rs",
        ],
    },
    Scene {
        name: "chrome-globals",
        run: chrome_globals,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/chrome_cache.rs",
            "src/appearance.rs",
            "src/reduce_motion.rs",
            "src/updater.rs",
            "src/update_release.rs",
            "src/sidebar.rs",
            "src/status_bar.rs",
            "src/tab_bar.rs",
            "src/title_area.rs",
        ],
    },
    Scene {
        name: "chrome-resize-title",
        run: chrome_resize_title,
        themes: Themes::One,
        serial: false,
        kind: Kind::Visual,
        sources: &["src/chrome_cache.rs", "src/title_area.rs", "src/sidebar.rs"],
    },
    Scene {
        name: "chrome-drag-cancel",
        run: chrome_drag_cancel,
        themes: Themes::One,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/chrome_cache.rs",
            "src/drag_gap.rs",
            "src/sidebar.rs",
            "src/tab_bar.rs",
        ],
    },
    Scene {
        name: "chrome-renaming",
        run: chrome_renaming,
        themes: Themes::One,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/chrome_cache.rs",
            "src/name_input.rs",
            "src/sidebar.rs",
            "src/tab_bar.rs",
            "src/tab_menu.rs",
        ],
    },
    Scene {
        name: "chrome-agent-tabs",
        run: chrome_agent_tabs,
        themes: Themes::One,
        // Serial: (c) the first send is held only while the classic view opened less than
        // `CLASSIC_START` (5 s of real time) before it; a slowed process could pass that.
        serial: true,
        kind: Kind::Visual,
        sources: &[
            "src/chrome_cache.rs",
            "src/workspace_agent.rs",
            "src/agent_tab.rs",
            "src/agent_conversation.rs",
            "src/composer.rs",
            "src/layout_model.rs",
            "src/pane_actions.rs",
            "src/name_input.rs",
            "src/procs.rs",
            "src/sidebar.rs",
            "src/tab_bar.rs",
            "src/tab_menu.rs",
            "src/providers/mod.rs",
            "src/providers/claude.rs",
            "src/providers/fixtures/claude-text.jsonl",
        ],
    },
    Scene {
        name: "chrome-agent-status",
        run: chrome_agent_status,
        themes: Themes::One,
        // Serial: (c) its ages under a minute turn every second while a check runs.
        serial: true,
        kind: Kind::Visual,
        sources: &[
            "src/chrome_cache.rs",
            "src/workspace_agent.rs",
            "src/agent_tab.rs",
            "src/agent_conversation.rs",
            "src/agent_process.rs",
            "src/activity.rs",
            "src/activity_panel.rs",
            "src/mark_view.rs",
            "src/status_mark.rs",
            "src/notifier.rs",
            "src/notification_list.rs",
            "src/title_area.rs",
            "src/groups.rs",
            "src/sidebar.rs",
            "src/tab_bar.rs",
            "src/providers/mod.rs",
            "src/providers/claude.rs",
        ],
    },
    Scene {
        name: "chrome-agent-age",
        run: chrome_agent_age,
        themes: Themes::One,
        // Serial: (c) "59s" read, then turned, by the real clock within 500 ms.
        serial: true,
        kind: Kind::Visual,
        sources: &[
            "src/chrome_cache.rs",
            "src/workspace_agent.rs",
            "src/agent_tab.rs",
            "src/agent_conversation.rs",
            "src/activity_panel.rs",
            "src/right_sidebar.rs",
            "src/status_bar.rs",
            "src/sidebar.rs",
            "src/status.rs",
            "src/providers/mod.rs",
            "src/providers/claude.rs",
        ],
    },
    Scene {
        name: "chrome-new-bar-reveal",
        run: chrome_new_bar_reveal,
        themes: Themes::One,
        serial: false,
        kind: Kind::Visual,
        sources: &["src/chrome_cache.rs", "src/tab_bar.rs"],
    },
    Scene {
        name: "chrome-session-facts",
        run: chrome_session_facts,
        themes: Themes::One,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/chrome_cache.rs",
            "src/sidebar.rs",
            "src/tab_bar.rs",
            "src/terminal_view.rs",
            "src/ssh_target.rs",
        ],
    },
    Scene {
        name: "chrome-bars-and-edges",
        run: chrome_bars_and_edges,
        themes: Themes::One,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/chrome_cache.rs",
            "src/sidebar.rs",
            "src/tab_bar.rs",
            "src/sidebar_toggle.rs",
            "src/right_sidebar.rs",
            "src/title_area.rs",
        ],
    },
    Scene {
        name: "chrome-sidebar-scroll",
        run: chrome_sidebar_scroll,
        themes: Themes::One,
        serial: false,
        kind: Kind::Visual,
        sources: &["src/chrome_cache.rs", "src/sidebar.rs"],
    },
    Scene {
        name: "chrome-sidebar-header-mark",
        run: chrome_sidebar_header_mark,
        themes: Themes::One,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/chrome_cache.rs",
            "src/sidebar.rs",
            "src/groups.rs",
            "src/status_mark.rs",
        ],
    },
    Scene {
        name: "chrome-row-sources",
        run: chrome_row_sources,
        themes: Themes::One,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/chrome_cache.rs",
            "src/sidebar.rs",
            "src/git_branch.rs",
            "src/ssh_target.rs",
            "src/restore.rs",
        ],
    },
    Scene {
        name: "chrome-file-tab-toolbar",
        run: chrome_file_tab_toolbar,
        themes: Themes::One,
        serial: false,
        kind: Kind::Visual,
        sources: &["src/chrome_cache.rs", "src/file_tab.rs", "src/tab_bar.rs"],
    },
];

/// Frames as the app gets them, and nothing else (`settle_frames`, with no cap on how many: an
/// animation asks for one each frame until it ends, by the real clock): the effects settle (a
/// notify draws), then each frame something asked for. No explicit draw.
pub(super) fn settle_frames(cx: &mut VisualTestAppContext, window: AnyWindowHandle) {
    let started = Instant::now();
    cx.run_until_parked();
    while next_frame(cx, window) {
        cx.run_until_parked();
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "frames are still asked for after 10 s"
        );
    }
}

/// The AppKit window titled `title` (the scene names its window so; the harness has no other
/// way to it).
fn ns_window(title: &str) -> objc2::rc::Retained<objc2_app_kit::NSWindow> {
    use objc2::MainThreadMarker;
    use objc2_app_kit::NSApplication;

    let mtm = MainThreadMarker::new().expect("the main thread");
    NSApplication::sharedApplication(mtm)
        .windows()
        .to_vec()
        .into_iter()
        .find(|w| w.title().to_string() == title)
        .unwrap_or_else(|| panic!("a window titled {title}"))
}

/// Session `name`'s place in the list.
fn index_of(cx: &VisualTestAppContext, ws: &Entity<Workspace>, name: &str) -> usize {
    cx.read(|cx| ws.read(cx).sessions.position(|s| s.label() == name))
        .unwrap_or_else(|| panic!("a session named {name}"))
}

/// Session `name`'s terminal reports `status` as its status tracker would (`shot_status`: the
/// view's `TerminalEvent::StatusChanged`, which the workspace hears in `on_terminal_event`).
fn report(cx: &mut VisualTestAppContext, ws: &Entity<Workspace>, name: &str, status: Status) {
    let i = index_of(cx, ws, name);
    let view = cx.read(|cx| ws.read(cx).sessions.items()[i].view.clone());
    cx.update(|cx| view.update(cx, |view, cx| view.shot_status(status, cx)));
}

/// Session `name`'s status dated minutes back (M11p Task 6, review): its age label turns once a
/// minute, not every second, so a step's check never lands on a turn (a turn the step's own
/// wait drew would repair a stale part before the check). It mutates `status_since` outside
/// the app's own paths (nothing in the app dates a status back), so it must notify the
/// workspace itself: without the notify the rows built after the report show the new date only
/// if the report's own path happens to notify (tried: chrome-notifications' steps show a part
/// stale without it). So in a step's `act` it can mask a missing notify of the source; the
/// callers there are `report`s, whose path (`on_terminal_event`) is what those steps test
/// elsewhere (chrome-status-elsewhere).
fn minutes_old(cx: &mut VisualTestAppContext, ws: &Entity<Workspace>, name: &str) {
    let i = index_of(cx, ws, name);
    cx.update(|cx| {
        ws.update(cx, |ws, cx| {
            let s = ws.sessions.get_mut(i).expect("a session");
            s.status_since = Instant::now() - Duration::from_secs(5 * 60 + 10);
            cx.notify();
        })
    });
}

/// Act on the workspace in the window (the app's own functions, as its handlers call them).
fn on_ws(
    cx: &mut VisualTestAppContext,
    window: AnyWindowHandle,
    ws: &Entity<Workspace>,
    f: impl FnOnce(&mut Workspace, &mut Window, &mut Context<Workspace>),
) {
    cx.update_window(window, |_, window, cx| {
        ws.update(cx, |ws, cx| f(ws, window, cx))
    })
    .expect("act on the workspace");
}

/// api and tests split side by side in one workspace (shown, api focused), and agent in a
/// workspace of its own (not shown), in another group.
fn elsewhere_window() -> (ShotApp, AnyWindowHandle, Entity<Workspace>) {
    super::chrome_parts::chrome_window(|ws, window, cx| {
        let api = std::path::Path::new(super::API);
        let web = std::path::Path::new(super::WEB);
        ws.open_shot_session("api", api, super::SHELL_TEXT, None, window, cx);
        ws.open_shot_session("tests", api, super::SHELL_TEXT, Some(Axis::Row), window, cx);
        ws.open_shot_session("agent", web, super::SHELL_TEXT, None, window, cx);
        let first = ws.sessions.position(|s| s.label() == "api").expect("api");
        ws.select(first, window, cx);
    })
}

/// Review Focus 2: a terminal's status changing where you aren't looking reaches the sidebar
/// (its row's mark, ring, age and the line under its name) and, for a pane of the shown
/// workspace, its tab's mark; each by the status's own path (`on_terminal_event`), no other
/// redraw. The marks' ticker turns a working mark on its own. A Waiting status (which would post a real banner from there) by `set_status` without
/// the notification, as an SSH hold does; it is the same function.
///
/// Panics unless after each the window shows what a full redraw does, and it changed.
pub(super) fn chrome_status_elsewhere() {
    let n = "chrome-status-elsewhere";
    let (mut cx, window, ws) = elsewhere_window();
    let mut steps = Steps::new(n, &mut cx, window, &ws);
    let ws = &ws;
    steps.step(
        &mut cx,
        window,
        "agent (elsewhere) works",
        Seen::Changes,
        |cx| report(cx, ws, "agent", Status::Working),
    );
    steps.step(
        &mut cx,
        window,
        "agent is done (a short run)",
        Seen::Changes,
        |cx| report(cx, ws, "agent", Status::Done),
    );
    steps.step(&mut cx, window, "agent waits", Seen::Changes, |cx| {
        let i = index_of(cx, ws, "agent");
        cx.update(|cx| ws.update(cx, |ws, cx| ws.set_status(i, Status::Waiting, false, cx)));
    });
    steps.step(&mut cx, window, "agent idle", Seen::Changes, |cx| {
        report(cx, ws, "agent", Status::Idle)
    });
    steps.step(
        &mut cx,
        window,
        "tests (not focused) works",
        Seen::Changes,
        |cx| report(cx, ws, "tests", Status::Working),
    );
    steps.step(
        &mut cx,
        window,
        "the marks' ticker turns tests's tab mark",
        Seen::Changes,
        |cx| {
            for _ in 0..3 {
                cx.advance_clock(crate::status_mark::TICK);
                cx.run_until_parked();
            }
        },
    );
    steps.step(&mut cx, window, "tests is done", Seen::Changes, |cx| {
        report(cx, ws, "tests", Status::Done)
    });
    let unseen = cx.read(|cx| ws.read(cx).notifications.unseen());
    assert_eq!(
        unseen, 2,
        "{n}: the two Done ones are in the 🔔 list, unseen"
    );
    steps.done();
}

/// The model's edits reach the sidebar, the tab bars and the title: a terminal renamed (as
/// `session.rename`), its workspace moved down (the menu's Move Workspace Down), a row's menu
/// opened, a colour and a pin chosen from it, a group closed (its header's click), a
/// workspace opened and one closed.
///
/// Panics unless after each the window shows what a full redraw does, and it changed.
pub(super) fn chrome_model_edits() {
    use crate::groups::GroupKey;
    use crate::tab_color::TabColor;
    use crate::tab_menu::TabAction;
    use crate::workspace::MoveSessionDown;

    let n = "chrome-model-edits";
    let (mut cx, window, ws) = super::chrome_parts::chrome_window(|ws, window, cx| {
        let api = std::path::Path::new(super::API);
        let web = std::path::Path::new(super::WEB);
        ws.open_shot_session("api", api, super::SHELL_TEXT, None, window, cx);
        ws.open_shot_session("tests", api, super::SHELL_TEXT, None, window, cx);
        ws.open_shot_session("web", web, super::SHELL_TEXT, None, window, cx);
        let first = ws.sessions.position(|s| s.label() == "api").expect("api");
        ws.select(first, window, cx);
    });
    let mut steps = Steps::new(n, &mut cx, window, &ws);
    let ws = &ws;
    let workspace = |cx: &mut VisualTestAppContext, name: &str| {
        let i = index_of(cx, ws, name);
        cx.read(|cx| ws.read(cx).workspace_of(i))
            .expect("its workspace")
    };
    steps.step(&mut cx, window, "api renamed", Seen::Changes, |cx| {
        let i = index_of(cx, ws, "api");
        cx.update(|cx| ws.update(cx, |ws, cx| ws.rename_at(i, "server", cx)));
    });
    let order = |cx: &mut VisualTestAppContext| cx.read(|cx| ws.read(cx).model.display_order());
    let before = order(&mut cx);
    steps.step(
        &mut cx,
        window,
        "its workspace moved down",
        Seen::Changes,
        |cx| cx.dispatch_action(window, MoveSessionDown),
    );
    assert_ne!(order(&mut cx), before, "{n}: the order changed");
    let tests = workspace(&mut cx, "tests");
    steps.step(
        &mut cx,
        window,
        "tests's row menu opened",
        Seen::Changes,
        |cx| {
            on_ws(cx, window, ws, |ws, window, cx| {
                ws.open_tab_menu(tests, point(px(120.0), px(120.0)), window, cx)
            })
        },
    );
    steps.step(&mut cx, window, "a colour chosen", Seen::Changes, |cx| {
        on_ws(cx, window, ws, |ws, window, cx| {
            ws.choose(TabAction::Color(Some(TabColor::Blue)), window, cx)
        })
    });
    let web = workspace(&mut cx, "web");
    on_ws(&mut cx, window, ws, |ws, window, cx| {
        ws.open_tab_menu(web, point(px(120.0), px(120.0)), window, cx)
    });
    steps.step(&mut cx, window, "web pinned", Seen::Changes, |cx| {
        on_ws(cx, window, ws, |ws, window, cx| {
            ws.choose(TabAction::Pin(true), window, cx)
        })
    });
    let key = GroupKey::Folder(super::API.into());
    steps.step(&mut cx, window, "api's group closed", Seen::Changes, |cx| {
        cx.update(|cx| ws.update(cx, |ws, cx| ws.toggle_group(&key, cx)))
    });
    steps.step(&mut cx, window, "a workspace opened", Seen::Changes, |cx| {
        on_ws(cx, window, ws, |ws, window, cx| {
            let web = std::path::Path::new(super::WEB);
            ws.open_shot_session("new", web, super::SHELL_TEXT, None, window, cx)
        })
    });
    let new = workspace(&mut cx, "new");
    steps.step(&mut cx, window, "it closed", Seen::Changes, |cx| {
        on_ws(cx, window, ws, |ws, window, cx| {
            ws.close_workspace(new, window, cx)
        })
    });
    steps.done();
}

/// Notifications reach the footer's 🔔 and, with the sidebar hidden, the title band's: a
/// terminal elsewhere is done after a short run (listed, unseen, no banner), the 🔔 clicked
/// open (all seen), the popover closed; then hidden, another one.
///
/// Panics unless after each the window shows what a full redraw does, and it changed.
pub(super) fn chrome_notifications() {
    use crate::title_area::{FOOTER_BUTTON, footer_buttons};
    use crate::ui_tokens::size::STATUS_BAR;

    let n = "chrome-notifications";
    let (mut cx, window, ws) = elsewhere_window();
    let ws = &ws;
    // Animations off: no marks' ticker, whose frames redraw the window's uncached root (the
    // popover) on their own and would hide a missing redraw (M11p Task 6, review).
    cx.update(|cx| {
        use gpui::BorrowAppContext;
        cx.update_global::<crate::appearance::Appearance, _>(|a, _| a.animations = false)
    });
    report(&mut cx, ws, "agent", Status::Working);
    report(&mut cx, ws, "tests", Status::Working);
    minutes_old(&mut cx, ws, "agent");
    minutes_old(&mut cx, ws, "tests");
    let mut steps = Steps::new(n, &mut cx, window, ws);
    let unseen = |cx: &mut VisualTestAppContext| cx.read(|cx| ws.read(cx).notifications.unseen());
    steps.step(&mut cx, window, "agent done: unseen", Seen::Changes, |cx| {
        report(cx, ws, "agent", Status::Done);
        minutes_old(cx, ws, "agent");
    });
    assert_eq!(unseen(&mut cx), 1, "{n}: one unseen");
    let (width, height) = cx
        .update_window(window, |_, window, cx| {
            let size = window.viewport_size();
            (ws.read(cx).sidebar.width, f32::from(size.height))
        })
        .expect("the window");
    let [_, bell_x, _] = footer_buttons(width);
    let bell = point(
        px(bell_x + FOOTER_BUTTON / 2.0),
        px(height - STATUS_BAR - crate::sidebar::FOOTER / 2.0),
    );
    steps.step(
        &mut cx,
        window,
        "the 🔔 clicked open",
        Seen::Changes,
        |cx| cx.simulate_click(window, bell, Modifiers::none()),
    );
    assert!(
        cx.read(|cx| ws.read(cx).notifications_popover.is_some()),
        "{n}: the popover opened"
    );
    assert_eq!(unseen(&mut cx), 0, "{n}: all seen");
    steps.step(&mut cx, window, "the popover closed", Seen::Changes, |cx| {
        on_ws(cx, window, ws, |ws, window, cx| {
            ws.dismiss_notifications(window, cx)
        })
    });
    // The sidebar hidden: the title band's 🔔.
    on_ws(&mut cx, window, ws, |ws, window, cx| {
        ws.toggle_sidebar(window, cx)
    });
    let start = Instant::now();
    while cx.read(|cx| ws.read(cx).slide.is_some()) {
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "{n}: the slide ends"
        );
        crate::ui_shots::prof::sleep(Duration::from_millis(16));
        settle_frames(&mut cx, window);
    }
    steps.step(
        &mut cx,
        window,
        "hidden: tests done: unseen",
        Seen::Changes,
        |cx| {
            report(cx, ws, "tests", Status::Done);
            minutes_old(cx, ws, "tests");
        },
    );
    assert_eq!(unseen(&mut cx), 1, "{n}: one unseen again");
    steps.done();
}

/// Where a terminal is and what runs there, found by the app's own lookups on a real shell: a
/// `cd` into a git repository (told by OSC 7, then its branch looked up), a program started
/// (the `ps` lookup), its listening port (the `lsof` lookup): the sidebar's row and the status
/// bar.
///
/// Panics unless each is found and, after each, the window shows what a full redraw does, and
/// it changed.
pub(super) fn chrome_places_programs_ports() {
    use crate::procs::ProgramState;
    use crate::workspace::{At, PROGRAMS_REFRESH};

    let n = "chrome-places-programs-ports";
    let dir = std::env::temp_dir().join(format!("fuxx-{n}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    for sub in ["a", "b"] {
        std::fs::create_dir_all(dir.join(sub)).expect("the fixture's folders");
    }
    let dir = dir.canonicalize().expect("the fixture's folder");
    // Declared first, so it goes last, also on a failed check: the shell's programs and the
    // folders go with it.
    let fixture = Fixture {
        dir: dir.clone(),
        shell: std::cell::Cell::new(None),
    };
    let (a, b) = (dir.join("a"), dir.join("b"));
    let git = |args: &[&str]| {
        let ok = std::process::Command::new("/usr/bin/git")
            .args(["-C", &b.display().to_string()])
            .args(["-c", "user.name=fuxx", "-c", "user.email=fuxx@example.com"])
            .args(args)
            .output()
            .is_ok_and(|o| o.status.success());
        assert!(ok, "{n}: git {args:?}");
    };
    git(&["init", "-q", "-b", "m11p-branch"]);
    git(&["commit", "-q", "--allow-empty", "-m", "fixture"]);
    // A shell with job control (so a program it runs is its terminal's foreground job), no rc
    // files, that waits for a line before each step: the cd (and its OSC 7, as shell
    // integration says it), then a program, which waits for a line before it listens on a port.
    let script = r#"read -r _; cd "$0/b" && printf '\033]7;file://%s\a' "$PWD"; read -r _; /usr/bin/perl -MIO::Socket::INET -e '<STDIN>; my $s = IO::Socket::INET->new(Listen => 5, LocalAddr => "127.0.0.1", LocalPort => 0, Proto => "tcp") or die; sleep 600'"#;
    let program = [
        "/usr/bin/env",
        "BASH_SILENCE_DEPRECATION_WARNING=1",
        "/bin/bash",
        "--norc",
        "--noprofile",
        "-i",
        "-c",
        script,
    ]
    .iter()
    .map(|s| s.to_string())
    .chain([dir.display().to_string()])
    .collect::<Vec<_>>();
    let start = a.clone();
    let (mut cx, window, ws) = super::chrome_parts::chrome_window(move |ws, window, cx| {
        // Looked up for real here.
        ws.places.frozen = false;
        ws.programs.frozen = false;
        ws.ports.frozen = false;
        let api = std::path::Path::new(super::API);
        ws.open_shot_session("api", api, super::SHELL_TEXT, None, window, cx);
        ws.open_shot_program(
            Some("shell"),
            None,
            program,
            Some(&start),
            "",
            At::New,
            window,
            cx,
        );
    });
    let ws = &ws;
    let id = cx.read(|cx| ws.read(cx).focused_terminal().expect("the shell"));
    let view = cx.read(|cx| {
        let ws = ws.read(cx);
        ws.sessions.items()[ws.index_of(id).expect("its session")]
            .view
            .clone()
    });
    fixture
        .shell
        .set(cx.read(|cx| view.read(cx).working_folder().1));
    // Run the app's timers and lookups until `found` holds (the shell and its program run on
    // their own time).
    let until = |cx: &mut VisualTestAppContext, what: &str, found: &dyn Fn(&App) -> bool| {
        let started = Instant::now();
        while !cx.read(|cx| found(cx)) {
            assert!(
                started.elapsed() < Duration::from_secs(20),
                "{n}: {what} wasn't found"
            );
            crate::ui_shots::prof::sleep(Duration::from_millis(50));
            cx.advance_clock(PROGRAMS_REFRESH);
            cx.run_until_parked();
        }
    };
    until(&mut cx, "the shell's first folder", &|cx| {
        ws.read(cx).places.folder(id) == Some(a.as_path())
    });
    let mut steps = Steps::new(n, &mut cx, window, ws);
    let line = |cx: &mut VisualTestAppContext| cx.read(|cx| view.read(cx).write_to_program(b"\n"));
    steps.step(
        &mut cx,
        window,
        "cd into a git repository",
        Seen::Changes,
        |cx| {
            line(cx);
            until(cx, "the new folder and its branch", &|cx| {
                let ws = ws.read(cx);
                ws.places.folder(id) == Some(b.as_path())
                    && ws.places.branch(&b) == Some("m11p-branch")
            });
        },
    );
    let facts = cx.read(|cx| ws.read(cx).status_bar_facts()).expect("facts");
    assert_eq!(
        facts.folder.as_deref(),
        Some(b.as_path()),
        "{n}: the bar's folder"
    );
    assert_eq!(
        facts.branch.as_deref(),
        Some("m11p-branch"),
        "{n}: the bar's branch"
    );
    steps.step(&mut cx, window, "a program starts", Seen::Changes, |cx| {
        line(cx);
        until(cx, "the program", &|cx| {
            matches!(ws.read(cx).programs.state(id), ProgramState::Running(_))
        });
    });
    println!(
        "ui-shots: {n}: running {:?}",
        cx.read(|cx| ws.read(cx).programs.state(id))
    );
    assert!(
        cx.read(|cx| ws.read(cx).ports.of(id).is_empty()),
        "{n}: no port yet"
    );
    steps.step(
        &mut cx,
        window,
        "it listens on a port",
        Seen::Changes,
        |cx| {
            line(cx);
            until(cx, "its port", &|cx| !ws.read(cx).ports.of(id).is_empty())
        },
    );
    let facts = cx.read(|cx| ws.read(cx).status_bar_facts()).expect("facts");
    println!(
        "ui-shots: {n}: the bar: {:?}, ports {:?}",
        facts.text, facts.ports
    );
    assert!(facts.running, "{n}: the bar says a program runs");
    steps.done();
}

/// `chrome-places-programs-ports`' fixture, cleared when dropped (a failed check too): every
/// process under its shell (the program would outlive the terminal by a hang-up it may not
/// get), then its folders.
struct Fixture {
    dir: PathBuf,
    shell: std::cell::Cell<Option<u32>>,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if let Some((shell, table)) = self.shell.get().zip(crate::procs::ps_table()) {
            for pid in crate::procs::descendants(&table, shell) {
                let _ = std::process::Command::new("/bin/kill")
                    .arg(pid.to_string())
                    .status();
            }
        }
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// The globals the chrome reads: the theme switched (the `Appearance` set anew, as a settings
/// reload sets it), macOS's Reduce motion on and off, the window hidden and shown again (its
/// occlusion), and an update found (the status bar's pill).
///
/// Panics unless after each the window shows what a full redraw does (and, where it should
/// look different, it changed).
pub(super) fn chrome_globals() {
    use crate::reduce_motion::{
        SystemReduceMotion, WindowVisible, reduce_motion_changed, visibility_changed,
    };
    use crate::update_release::CheckOutcome;

    let n = "chrome-globals";
    let (mut cx, window, ws) = elsewhere_window();
    let ws = &ws;
    cx.update(|cx| {
        cx.set_global(SystemReduceMotion::for_shots(false));
        cx.set_global(WindowVisible::for_shots(true));
    });
    // tests works in a pane in front (its tab's mark moves; since M11p Task 6 a working row
    // elsewhere, a still badge, doesn't run the marks' ticker).
    report(&mut cx, ws, "tests", Status::Working);
    minutes_old(&mut cx, ws, "tests");
    let mut steps = Steps::new(n, &mut cx, window, ws);
    steps.step(
        &mut cx,
        window,
        "theme switched to Latte",
        Seen::Changes,
        |cx| {
            let latte = super::shot_theme("Catppuccin Latte");
            cx.update(|cx| {
                let appearance = super::shot_appearance(latte, cx);
                cx.set_global(appearance)
            });
        },
    );
    let ticking = |cx: &mut VisualTestAppContext| cx.read(|cx| ws.read(cx).marks_ticking);
    assert!(ticking(&mut cx), "{n}: tests' mark moves");
    steps.step(&mut cx, window, "Reduce motion on", Seen::Maybe, |cx| {
        cx.update(|cx| reduce_motion_changed(true, cx));
        // The ticker's next tick finds nothing moving and stops.
        cx.advance_clock(crate::status_mark::TICK);
    });
    assert!(!ticking(&mut cx), "{n}: the marks stopped");
    steps.step(&mut cx, window, "Reduce motion off", Seen::Maybe, |cx| {
        cx.update(|cx| reduce_motion_changed(false, cx))
    });
    assert!(ticking(&mut cx), "{n}: the marks move again");
    // (Hidden, nothing turns the ages, by design: none may turn during the step.)
    steps.clear_of_age_turns(&mut cx, window, Duration::from_millis(600));
    steps.step(&mut cx, window, "the window hidden", Seen::Maybe, |cx| {
        cx.update(|cx| visibility_changed(false, cx));
        // The ticker's next tick finds the window unseen and stops.
        cx.advance_clock(crate::status_mark::TICK);
    });
    assert!(!ticking(&mut cx), "{n}: hidden, the marks stopped");
    steps.step(
        &mut cx,
        window,
        "the window shown again",
        Seen::Maybe,
        |cx| cx.update(|cx| visibility_changed(true, cx)),
    );
    // Shown again, it was redrawn (that redraw starts the ticker again).
    assert!(ticking(&mut cx), "{n}: shown again, the marks move");
    steps.step(&mut cx, window, "an update found", Seen::Changes, |cx| {
        let release = super::shot_release();
        cx.update(|cx| crate::updater::found(CheckOutcome::Newer(release), false, cx));
    });
    steps.done();
}

/// Review Focus 3: the window resized across the width where the open sidebar's band hides the
/// app's title (`shows_title`), and back (GPUI redraws the whole window on a resize). Full
/// screen isn't driven here (an off-screen window can't go full screen without taking over a
/// real screen); going in or out of it resizes the window's view the same way (AppKit sets its
/// frame size, GPUI's resize callback redraws the whole window), so it takes this path.
///
/// Panics unless the title hides and shows again and, after each, the window shows what a full
/// redraw does.
pub(super) fn chrome_resize_title() {
    use crate::title_area::shows_title;

    let n = "chrome-resize-title";
    let (mut cx, window, ws) = super::chrome_parts::chrome_window(|ws, window, cx| {
        let api = std::path::Path::new(super::API);
        ws.open_shot_session("api", api, super::SHELL_TEXT, None, window, cx);
    });
    let width = cx.read(|cx| ws.read(cx).sidebar.width);
    let viewport = |cx: &mut VisualTestAppContext| {
        cx.update_window(window, |_, window, _| window.viewport_size())
            .expect("the window's size")
    };
    let start = viewport(&mut cx);
    let shows = |w: f32| shows_title(width, w, false);
    assert!(
        shows(f32::from(start.width)),
        "{n}: the title shows at first"
    );
    let narrow = (0..f32::from(start.width) as i32)
        .rev()
        .map(|w| w as f32)
        .find(|w| !shows(*w))
        .expect("a width that hides the title");
    println!(
        "ui-shots: {n}: sidebar {width}, the title hides below {}",
        narrow + 1.0
    );
    // Named, to be found among AppKit's windows (the workspace sets its title only when it
    // changes, so this one stays).
    let title = format!("{n}-{}", std::process::id());
    cx.update_window(window, |_, window, _| window.set_window_title(&title))
        .expect("name the window");
    let ns = ns_window(&title);
    let mut steps = Steps::new(n, &mut cx, window, &ws);
    for (what, w) in [
        ("narrowed past it", narrow),
        ("widened back", f32::from(start.width)),
    ] {
        steps.step(&mut cx, window, what, Seen::Changes, |_| {
            // As a drag of the window's edge does it: AppKit sizes the window, which tells GPUI
            // (its view's new size), and GPUI redraws the whole window.
            let height = f64::from(f32::from(start.height));
            ns.setContentSize(objc2_foundation::NSSize::new(f64::from(w), height));
        });
        let now = viewport(&mut cx);
        assert_eq!(f32::from(now.width), w, "{n}: {what}: resized");
        assert_eq!(
            shows(w),
            w > narrow,
            "{n}: {what}: the title shows only when wide"
        );
    }
    // The sidebar hidden: the title band's workspace name has its own threshold (`shows_name`).
    on_ws(&mut cx, window, &ws, |ws, window, cx| {
        ws.toggle_sidebar(window, cx)
    });
    let started = Instant::now();
    while cx.read(|cx| ws.read(cx).slide.is_some()) {
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "{n}: the slide ends"
        );
        crate::ui_shots::prof::sleep(Duration::from_millis(16));
        settle_frames(&mut cx, window);
    }
    let named = |w: f32| crate::title_area::shows_name(w, false);
    let nameless = (0..f32::from(start.width) as i32)
        .rev()
        .map(|w| w as f32)
        .find(|w| !named(*w))
        .expect("a width that hides the name");
    for (what, w) in [
        ("hidden: narrowed past the name's", nameless),
        ("hidden: widened back", f32::from(start.width)),
    ] {
        steps.step(&mut cx, window, what, Seen::Changes, |_| {
            let height = f64::from(f32::from(start.height));
            ns.setContentSize(objc2_foundation::NSSize::new(f64::from(w), height));
        });
        assert_eq!(
            named(w),
            w > nameless,
            "{n}: {what}: the name shows only when wide"
        );
    }
    steps.done();
}

/// Workspace A: api, tests and build as tabs of one pane, web split right of it; B (docs) and
/// C (notes) below it in the same group. api focused.
fn drag_window() -> (ShotApp, AnyWindowHandle, Entity<Workspace>) {
    super::chrome_parts::chrome_window(|ws, window, cx| {
        let api = std::path::Path::new(super::API);
        ws.open_shot_session("api", api, super::SHELL_TEXT, None, window, cx);
        ws.open_shot_tab("tests", api, super::SHELL_TEXT, window, cx);
        ws.open_shot_tab("build", api, super::SHELL_TEXT, window, cx);
        ws.open_shot_session("web", api, super::SHELL_TEXT, Some(Axis::Row), window, cx);
        ws.open_shot_session("docs", api, super::SHELL_TEXT, None, window, cx);
        ws.open_shot_session("notes", api, super::SHELL_TEXT, None, window, cx);
        let first = ws.sessions.position(|s| s.label() == "api").expect("api");
        ws.select(first, window, cx);
    })
}

/// Press on `from`, move past GPUI's drag threshold, then to `to` (still held), and hold still
/// while what makes way slides (by the real clock).
fn drag_to(
    cx: &mut VisualTestAppContext,
    window: AnyWindowHandle,
    from: Point<Pixels>,
    to: Point<Pixels>,
) {
    let left = MouseButton::Left;
    cx.simulate_mouse_down(window, from, left, Modifiers::none());
    cx.simulate_mouse_move(
        window,
        from + point(px(0.0), px(8.0)),
        left,
        Modifiers::none(),
    );
    cx.simulate_mouse_move(window, to, left, Modifiers::none());
    crate::ui_shots::prof::sleep(crate::drag_gap::SLIDE + Duration::from_millis(30));
}

/// A point over the right pane's terminal (no bar, no row).
fn over_terminal(cx: &mut VisualTestAppContext, ws: &Entity<Workspace>) -> Point<Pixels> {
    let panes = super::shown_panes(cx, ws);
    let bar = super::tab_bounds(cx, ws, panes[1], 0).expect("web's tab");
    point(bar.left() + px(40.0), bar.bottom() + px(200.0))
}

/// Whether anything of a drag is left: its source, landing, gap, or slides.
fn drag_left(ws: &Workspace) -> bool {
    ws.tab_drag.get().is_some()
        || ws.tab_drop.is_some()
        || ws.sidebar.drag_source.get().is_some()
        || ws.sidebar.drop_slot.is_some()
        || ws.sidebar.drop_at.is_some()
        || !ws.drag_slides.borrow().is_empty()
}

/// Review Focus 4: a drag's indicators and slides, and their clearing as it ends: a sidebar row
/// dragged down its run (the rows below slide up), Esc (a row's drag goes on: only a tab's has
/// Esc), released over a terminal; a tab dragged over the other pane's bar (its tab slides
/// right), Esc; dragged there again and released over a terminal. Frames only as the app gets
/// them.
///
/// Panics unless the drags show and each end leaves nothing behind, and after each step the
/// window shows what a full redraw does.
pub(super) fn chrome_drag_cancel() {
    let n = "chrome-drag-cancel";
    let (mut cx, window, ws) = drag_window();
    let ws = &ws;
    let mut steps = Steps::new(n, &mut cx, window, ws);
    let order = cx.read(|cx| ws.read(cx).model.display_order());
    // Row 1 is A's (under its group's header), row 3 C's.
    let (row_a, row_c) = (
        super::sidebar::row_point(&mut cx, ws, 1, 0.0),
        super::sidebar::row_point(&mut cx, ws, 3, 0.0),
    );
    steps.step(
        &mut cx,
        window,
        "a row dragged down its run",
        Seen::Changes,
        |cx| drag_to(cx, window, row_a, row_c),
    );
    let (slot, at, source) = cx.read(|cx| {
        let s = &ws.read(cx).sidebar;
        (s.drop_slot, s.drop_at.clone(), s.drag_source.get())
    });
    println!("ui-shots: {n}: the row's drag: slot {slot:?}, at {at:?}, source {source:?}");
    assert!(source.is_some(), "{n}: a row is dragged");
    assert!(slot.is_some() || at.is_some(), "{n}: the row's gap is open");
    steps.step(
        &mut cx,
        window,
        "Esc (the row drag goes on)",
        Seen::Maybe,
        |cx| cx.simulate_keystrokes(window, "escape"),
    );
    let terminal = over_terminal(&mut cx, ws);
    steps.step(
        &mut cx,
        window,
        "the row released over a terminal",
        Seen::Changes,
        |cx| cx.simulate_mouse_up(window, terminal, MouseButton::Left, Modifiers::none()),
    );
    assert!(
        !cx.read(|cx| cx.has_active_drag()),
        "{n}: the row's drag ended"
    );
    assert!(
        !cx.read(|cx| drag_left(ws.read(cx))),
        "{n}: nothing of the row's drag left"
    );
    assert_eq!(
        cx.read(|cx| ws.read(cx).model.display_order()),
        order,
        "{n}: released elsewhere, the rows stay"
    );

    let panes = super::shown_panes(&mut cx, ws);
    let tests = super::tab_bounds(&mut cx, ws, panes[0], 1).expect("tests's tab");
    let web = super::tab_bounds(&mut cx, ws, panes[1], 0).expect("web's tab");
    let over_web = point(web.left() + px(5.0), web.center().y);
    let dragged = |cx: &mut VisualTestAppContext| {
        drag_to(cx, window, tests.center(), over_web);
        let drop = cx.read(|cx| ws.read(cx).tab_drop);
        assert!(drop.is_some(), "{n}: tests lands in web's bar");
    };
    steps.step(
        &mut cx,
        window,
        "a tab dragged over the other bar",
        Seen::Changes,
        dragged,
    );
    steps.step(&mut cx, window, "Esc", Seen::Changes, |cx| {
        cx.simulate_keystrokes(window, "escape")
    });
    assert!(
        !cx.read(|cx| cx.has_active_drag()),
        "{n}: Esc ended the tab's drag"
    );
    assert!(
        !cx.read(|cx| drag_left(ws.read(cx))),
        "{n}: nothing of it left"
    );
    // (The button is still down after Esc: let it go where it is, as a hand would.)
    cx.simulate_mouse_up(window, over_web, MouseButton::Left, Modifiers::none());
    steps.step(
        &mut cx,
        window,
        "dragged there again",
        Seen::Changes,
        dragged,
    );
    let terminal = over_terminal(&mut cx, ws);
    steps.step(
        &mut cx,
        window,
        "the tab released over a terminal",
        Seen::Changes,
        |cx| cx.simulate_mouse_up(window, terminal, MouseButton::Left, Modifiers::none()),
    );
    assert!(
        !cx.read(|cx| drag_left(ws.read(cx))),
        "{n}: nothing of the tab's drag left"
    );
    let tabs = cx.read(|cx| ws.read(cx).model.pane(panes[0]).map(|p| p.tabs.len()));
    assert_eq!(tabs, Some(3), "{n}: released elsewhere, the tab stays");
    steps.done();
}

/// Renaming in place: the rename box opened in a workspace's row (its menu's Rename…), typed
/// into, confirmed (Enter); then in a tab (Rename Tab…), the same.
///
/// Panics unless the names change and after each step the window shows what a full redraw
/// does, and it changed.
pub(super) fn chrome_renaming() {
    let n = "chrome-renaming";
    let (mut cx, window, ws) = elsewhere_window();
    let ws = &ws;
    cx.update(|cx| {
        cx.bind_keys([gpui::KeyBinding::new(
            "enter",
            crate::name_input::Confirm,
            Some("NameInput"),
        )])
    });
    let mut steps = Steps::new(n, &mut cx, window, ws);
    let agent = index_of(&cx, ws, "agent");
    let row = cx
        .read(|cx| ws.read(cx).workspace_of(agent))
        .expect("agent's workspace");
    steps.step(
        &mut cx,
        window,
        "a row's rename box opened",
        Seen::Changes,
        |cx| {
            on_ws(cx, window, ws, |ws, window, cx| {
                ws.start_rename(row, window, cx)
            })
        },
    );
    steps.step(&mut cx, window, "typed into", Seen::Changes, |cx| {
        cx.simulate_input(window, "deploy")
    });
    steps.step(&mut cx, window, "confirmed", Seen::Changes, |cx| {
        cx.simulate_keystrokes(window, "enter")
    });
    let label = cx.read(|cx| ws.read(cx).workspace_label(row).to_string());
    assert_eq!(label, "deploy", "{n}: the workspace's new name");
    let tests = index_of(&cx, ws, "tests");
    let id = cx.read(|cx| ws.read(cx).sessions.items()[tests].id);
    steps.step(
        &mut cx,
        window,
        "a tab's rename box opened",
        Seen::Changes,
        |cx| {
            on_ws(cx, window, ws, |ws, window, cx| {
                ws.start_tab_rename(id, window, cx)
            })
        },
    );
    steps.step(&mut cx, window, "typed into", Seen::Changes, |cx| {
        cx.simulate_input(window, "unit")
    });
    steps.step(&mut cx, window, "confirmed", Seen::Changes, |cx| {
        cx.simulate_keystrokes(window, "enter")
    });
    let label = cx.read(|cx| ws.read(cx).terminal_label(id).to_string());
    assert_eq!(label, "unit", "{n}: the terminal's new name");
    steps.done();
}

/// M11aa Task 8: agent tabs' chrome sources, each by the app's own path. An agent tab split
/// beside api (`agent_beside`, the provider preset launch's `then` hook's body: a new pane with
/// a CC tab; the row keeps api's name, R11); the ⋯ menu's New Claude Tab (in tests's pane); the
/// first message titling the tab (`AgentTab::send` → `Title` → `on_agent_event`); its rename box
/// (as `chrome-renaming`); Open in Classic View (`open_classic`, the tab Done: no sheet; a
/// terminal tab joins its pane); closing it (`close_tab`, no sheet). No agent runs: the tabs are
/// made with no process and fed decoded events (R12); the classic view's terminal is a quiet
/// `cat` (`quiet_opens`).
///
/// Panics unless after each step the window shows what a full redraw does, and it changed.
pub(super) fn chrome_agent_tabs() {
    use crate::layout_model::TabKind;
    use crate::pane_actions::PaneAction;
    let n = "chrome-agent-tabs";
    let (mut cx, window, ws) = elsewhere_window();
    let ws = &ws;
    cx.update(|cx| {
        cx.bind_keys([gpui::KeyBinding::new(
            "enter",
            crate::name_input::Confirm,
            Some("NameInput"),
        )])
    });
    let mut steps = Steps::new(n, &mut cx, window, ws);
    let api = index_of(&cx, ws, "api");
    let (api_id, row) = cx.read(|cx| {
        let ws = ws.read(cx);
        (
            ws.sessions.items()[api].id,
            ws.workspace_of(api).expect("api's workspace"),
        )
    });
    steps.step(
        &mut cx,
        window,
        "an agent tab beside api",
        Seen::Changes,
        |cx| {
            on_ws(cx, window, ws, |ws, window, cx| {
                ws.agent_beside(api_id, "claude", window, cx)
            })
        },
    );
    let (tab, view) = cx
        .read(|cx| ws.read(cx).focused_agent_tab())
        .expect("the agent tab, focused, its view made");
    cx.read(|cx| {
        let ws = ws.read(cx);
        assert_eq!(
            ws.workspace_label(row).as_ref(),
            "api",
            "{n}: api is named: its row too"
        );
        assert_eq!(ws.agents[&tab].id, 1, "{n}: a1");
        let (_, pane) = ws.model.locate(tab).expect("placed");
        assert_eq!(
            ws.model.pane(pane).map(|p| p.tabs.len()),
            Some(1),
            "{n}: its own pane"
        );
    });
    let tests = index_of(&cx, ws, "tests");
    let tests_pane = cx
        .read(|cx| ws.read(cx).pane_of(tests))
        .expect("tests's pane");
    steps.step(
        &mut cx,
        window,
        "the pane menu adds a Claude tab",
        Seen::Changes,
        |cx| {
            on_ws(cx, window, ws, |ws, window, cx| {
                ws.run_pane_action(tests_pane, PaneAction::NewAgent("claude"), window, cx)
            })
        },
    );
    let tabs = cx.read(|cx| ws.read(cx).model.pane(tests_pane).map(|p| p.tabs.len()));
    assert_eq!(tabs, Some(2), "{n}: tests's pane has its Claude tab");
    assert_eq!(
        cx.read(|cx| ws.read(cx).agents.len()),
        2,
        "{n}: two agent tabs"
    );
    steps.step(
        &mut cx,
        window,
        "the first message titles the tab",
        Seen::Changes,
        |cx| cx.update(|cx| view.update(cx, |t, cx| t.send("Fix the date check".into(), cx))),
    );
    let title = |cx: &VisualTestAppContext| {
        cx.read(
            |cx| match ws.read(cx).model.tab(tab).map(|t| t.kind.clone()) {
                Some(TabKind::Agent { title, .. }) => title,
                _ => None,
            },
        )
    };
    assert_eq!(
        title(&cx).as_deref(),
        Some("Fix the date check"),
        "{n}: titled"
    );
    // Its turn ends and it reports its conversation (nothing the chrome shows changes: the
    // id is saved with the tab).
    steps.step(&mut cx, window, "its turn ends", Seen::Maybe, |cx| {
        let mut t = crate::providers::claude::CLAUDE.translator();
        use crate::providers::Provider as _;
        let events: Vec<_> = include_str!("../providers/fixtures/claude-text.jsonl")
            .lines()
            .flat_map(|l| t.decode(l))
            .collect();
        cx.update(|cx| view.update(cx, |t, cx| t.shot_feed(events, cx)));
    });
    let conversation = cx.read(
        |cx| match ws.read(cx).model.tab(tab).map(|t| t.kind.clone()) {
            Some(TabKind::Agent { conversation, .. }) => conversation,
            _ => None,
        },
    );
    assert!(
        conversation.is_some(),
        "{n}: its conversation is saved with the tab"
    );
    steps.step(
        &mut cx,
        window,
        "rename the agent tab: its box",
        Seen::Changes,
        |cx| {
            on_ws(cx, window, ws, |ws, window, cx| {
                ws.start_agent_rename(tab, window, cx)
            })
        },
    );
    steps.step(&mut cx, window, "typed into", Seen::Changes, |cx| {
        cx.simulate_input(window, "dates")
    });
    steps.step(&mut cx, window, "confirmed", Seen::Changes, |cx| {
        cx.simulate_keystrokes(window, "enter")
    });
    assert_eq!(title(&cx).as_deref(), Some("dates"), "{n}: renamed");
    steps.step(
        &mut cx,
        window,
        "open it in classic view",
        Seen::Changes,
        |cx| {
            on_ws(cx, window, ws, |ws, window, cx| {
                ws.open_classic(tab, window, cx)
            })
        },
    );
    cx.read(|cx| {
        let ws = ws.read(cx);
        let (_, pane) = ws.model.locate(tab).expect("still there");
        let kinds: Vec<bool> = ws
            .model
            .pane(pane)
            .expect("its pane")
            .tabs
            .iter()
            .map(|t| matches!(t.kind, TabKind::Terminal(_)))
            .collect();
        assert_eq!(kinds, [false, true], "{n}: a terminal tab joined its pane");
        let classic = ws
            .classic_of
            .get(&tab)
            .expect("the classic view's terminal noted")
            .0;
        assert_eq!(
            ws.model.agent_classic(tab),
            None,
            "{n}: a local classic view ends with fuxx: not saved with the tab"
        );
        let s = ws
            .sessions
            .items()
            .iter()
            .find(|s| s.id == classic)
            .expect("it runs");
        let preset = s.preset.as_ref().expect("its preset");
        assert_eq!(preset.name, "claude", "{n}");
        assert!(
            preset.command.starts_with("claude --resume "),
            "{n}: {}",
            preset.command
        );
        let t = view.read(cx);
        assert!(t.shot_paused(), "{n}: the tab is paused");
        assert_eq!(t.status(), Status::Done, "{n}");
        assert_eq!(
            t.shot_items().last(),
            Some(&crate::agent_conversation::Item::Notice(
                crate::agent_conversation::PAUSED_NOTICE.into()
            )),
            "{n}"
        );
    });
    // R13: sending from the paused tab while its classic terminal still runs `claude` warns and
    // keeps the text (never a second writer); once it doesn't, the tab resumes and sends.
    let classic = cx.read(|cx| ws.read(cx).classic_of[&tab].0);
    let composer = cx.read(|cx| view.read(cx).shot_composer());
    let send_again = |cx: &mut VisualTestAppContext| {
        cx.update(|cx| {
            composer.update(cx, |c, cx| c.set_text("and the tests".into(), cx));
            view.update(cx, |t, cx| t.send("and the tests".into(), cx));
        });
        cx.run_until_parked();
    };
    // What holds the conversation, as one `ps` would say (the scene's tables: `shot_ps`).
    let set_ps = |cx: &mut VisualTestAppContext, table: Option<Vec<crate::procs::Proc>>| {
        cx.update(|cx| ws.update(cx, |ws, _| ws.shot_ps = Some(table)));
    };
    let you = |cx: &VisualTestAppContext| {
        cx.read(|cx| {
            view.read(cx)
                .shot_items()
                .iter()
                .filter(|i| matches!(i, crate::agent_conversation::Item::You { .. }))
                .count()
        })
    };
    let held = |cx: &VisualTestAppContext, why: &str| -> String {
        cx.read(|cx| {
            let t = view.read(cx);
            assert!(t.shot_paused(), "{n}: {why}: still paused");
            assert_eq!(
                composer.read(cx).text(),
                "and the tests",
                "{n}: {why}: the text stays"
            );
            match t.shot_items().last().cloned() {
                Some(crate::agent_conversation::Item::Notice(text)) => text,
                last => panic!("{n}: {why}: a notice, not {last:?}"),
            }
        })
    };
    let before = you(&cx);
    // Sent at once: the classic terminal's `claude` may not have started yet (`ps` finds
    // nothing): still held.
    send_again(&mut cx);
    let text = held(&cx, "the classic view just opened");
    assert!(
        text.starts_with("Claude is still running this conversation in ")
            && text.ends_with(" — quit it there first"),
        "{n}: {text}"
    );
    assert_eq!(you(&cx), before, "{n}: nothing sent");
    // Past its start: a `claude --resume <id>` in a background-service terminal (no shell pid
    // of fuxx's) holds it.
    cx.update(|cx| {
        ws.update(cx, |ws, _| {
            if let Some(entry) = ws.classic_of.get_mut(&tab) {
                entry.1 = Instant::now() - Duration::from_secs(10);
            }
        })
    });
    let id = conversation.clone().expect("its conversation");
    let holder = format!("  4242     1  4242  4242 S+   claude --resume {id}\n");
    set_ps(&mut cx, Some(crate::procs::parse_ps(&holder)));
    send_again(&mut cx);
    let text = held(&cx, "a claude --resume holds it");
    assert!(text.contains("still running"), "{n}: {text}");
    assert_eq!(you(&cx), before, "{n}: nothing sent");
    // `ps` failed: unknown, so no second writer either.
    set_ps(&mut cx, None);
    send_again(&mut cx);
    let text = held(&cx, "ps failed");
    assert!(
        text.starts_with("Couldn't check whether Claude"),
        "{n}: {text}"
    );
    assert_eq!(you(&cx), before, "{n}: nothing sent");
    // Nothing holds it: the tab resumes and sends, once even for Enter pressed twice (the
    // second comes while the first's check runs; the box still has the text then).
    set_ps(&mut cx, Some(Vec::new()));
    cx.update(|cx| {
        composer.update(cx, |c, cx| c.set_text("and the tests".into(), cx));
        view.update(cx, |t, cx| t.send("and the tests".into(), cx));
        view.update(cx, |t, cx| t.send("and the tests".into(), cx));
    });
    cx.run_until_parked();
    cx.read(|cx| {
        let t = view.read(cx);
        assert!(!t.shot_paused(), "{n}: resumed");
        assert!(
            t.shot_last_argv().windows(2).any(|w| w[0] == "--resume"),
            "{n}: restarted with --resume: {:?}",
            t.shot_last_argv()
        );
        assert_eq!(composer.read(cx).text(), "", "{n}: sent, the box cleared");
        assert_eq!(t.status(), Status::Working, "{n}");
    });
    assert_eq!(you(&cx), before + 1, "{n}: sent once");
    // While its classic view is open, "Open in classic view" shows that terminal: no second.
    let classic_tabs = |cx: &VisualTestAppContext| {
        cx.read(|cx| {
            let ws = ws.read(cx);
            let (_, pane) = ws.model.locate(tab).expect("placed");
            ws.model.pane(pane).map_or(0, |p| p.tabs.len())
        })
    };
    let tabs_before = classic_tabs(&cx);
    on_ws(&mut cx, window, ws, |ws, window, cx| {
        ws.select_tab(tab, window, cx);
        ws.open_classic(tab, window, cx);
    });
    cx.run_until_parked();
    assert_eq!(
        classic_tabs(&cx),
        tabs_before,
        "{n}: no second classic view"
    );
    let shown = cx.read(|cx| ws.read(cx).focused_terminal());
    assert_eq!(shown, Some(classic), "{n}: its classic view is shown");
    // That turn ends: the tab is Done, so closing it asks nothing.
    cx.update(|cx| {
        view.update(cx, |t, cx| {
            t.shot_feed(
                vec![crate::providers::Event::TurnDone {
                    ok: true,
                    message: None,
                }],
                cx,
            )
        })
    });
    cx.run_until_parked();
    // Its classic terminal no longer noted (as after a restart): Open in classic view opens
    // another. Asked twice, and a send, while it opens (its `ps` and the reap still to come):
    // one terminal, and the send is held (the tab's `claude -p --resume` would be a second
    // writer beside the classic view's `claude --resume`).
    cx.update(|cx| {
        ws.update(cx, |ws, _| {
            ws.classic_of.remove(&tab);
        })
    });
    let tabs_before = classic_tabs(&cx);
    let before = you(&cx);
    let argv_before = cx.read(|cx| view.read(cx).shot_last_argv());
    on_ws(&mut cx, window, ws, |ws, window, cx| {
        ws.open_classic(tab, window, cx);
        ws.open_classic(tab, window, cx);
        composer.update(cx, |c, cx| c.set_text("and the tests".into(), cx));
        view.update(cx, |t, cx| t.send("and the tests".into(), cx));
    });
    cx.run_until_parked();
    let text = held(&cx, "sent while the classic view opens");
    assert!(
        text.starts_with("Claude is still running this conversation in "),
        "{n}: {text}"
    );
    assert_eq!(you(&cx), before, "{n}: nothing sent while it opens");
    assert_eq!(
        cx.read(|cx| view.read(cx).shot_last_argv()),
        argv_before,
        "{n}: no process started while it opens"
    );
    assert_eq!(
        classic_tabs(&cx),
        tabs_before + 1,
        "{n}: one classic view opened"
    );
    let reopened = cx.read(|cx| ws.read(cx).classic_of.get(&tab).map(|c| c.0));
    assert!(
        reopened.is_some_and(|id| id != classic),
        "{n}: the new classic view noted"
    );
    // Its classic terminal no longer noted again, and nothing holds it: a send starts its
    // check, then Open in classic view while that check runs does nothing (the check could
    // answer Free and resume the tab beside a classic view opened meanwhile).
    cx.update(|cx| {
        ws.update(cx, |ws, _| {
            ws.classic_of.remove(&tab);
            ws.shot_ps = Some(Some(Vec::new()));
        })
    });
    let tabs_before = classic_tabs(&cx);
    let before = you(&cx);
    cx.update(|cx| {
        composer.update(cx, |c, cx| c.set_text("and the tests".into(), cx));
        view.update(cx, |t, cx| t.send("and the tests".into(), cx));
    });
    on_ws(&mut cx, window, ws, |ws, window, cx| {
        ws.open_classic(tab, window, cx)
    });
    cx.run_until_parked();
    assert_eq!(
        classic_tabs(&cx),
        tabs_before,
        "{n}: no classic view opened while a send's check runs"
    );
    cx.read(|cx| {
        let t = view.read(cx);
        assert!(!t.shot_paused(), "{n}: resumed");
        assert!(
            t.shot_items()
                .contains(&crate::agent_conversation::Item::Notice(
                    crate::workspace_agent::CHECKING_NOTICE.into()
                )),
            "{n}: the ignored open says why"
        );
    });
    assert_eq!(you(&cx), before + 1, "{n}: that send went once");
    cx.update(|cx| {
        view.update(cx, |t, cx| {
            t.shot_feed(
                vec![crate::providers::Event::TurnDone {
                    ok: true,
                    message: None,
                }],
                cx,
            )
        })
    });
    cx.run_until_parked();
    // A classic view saved from before a restart, in a background-service terminal this run
    // can't see (the service couldn't be reached at launch), whose id a local terminal of this
    // run shares (the first classic view's). An npm `claude` there shows as `claude` alone: it
    // still holds the conversation. Open in classic view opens none beside it, and a send
    // doesn't resume the tab.
    cx.update(|cx| {
        ws.update(cx, |ws, _| {
            ws.classic_of.remove(&tab);
            assert!(ws.model.set_agent_classic(tab, Some(classic)));
            let npm = "  4242     1  4242  4242 S+   claude\n";
            ws.shot_ps = Some(Some(crate::procs::parse_ps(npm)));
        })
    });
    let tabs_before = classic_tabs(&cx);
    let before = you(&cx);
    on_ws(&mut cx, window, ws, |ws, window, cx| {
        ws.open_classic(tab, window, cx)
    });
    cx.run_until_parked();
    assert_eq!(
        classic_tabs(&cx),
        tabs_before,
        "{n}: no second classic view beside a saved one this run can't see"
    );
    send_again(&mut cx);
    let text = held(&cx, "a saved classic view this run can't see");
    assert!(
        text.ends_with("in another terminal — quit it there first"),
        "{n}: {text}"
    );
    assert_eq!(you(&cx), before, "{n}: nothing sent");
    // The local terminal sharing its number closes: the saved one is not forgotten.
    on_ws(&mut cx, window, ws, |ws, window, cx| {
        if let Some(index) = ws.index_of(classic) {
            ws.close_at(index, window, cx);
        }
        ws.select_tab(tab, window, cx);
    });
    cx.run_until_parked();
    cx.read(|cx| {
        let ws = ws.read(cx);
        assert_eq!(ws.index_of(classic), None, "{n}: that terminal closed");
        assert_eq!(
            ws.model.agent_classic(tab),
            Some(classic),
            "{n}: a local terminal never clears a saved service session"
        );
    });
    steps.step(
        &mut cx,
        window,
        "close the agent tab",
        Seen::Changes,
        |cx| {
            on_ws(cx, window, ws, |ws, window, cx| {
                ws.close_tab(tab, window, cx)
            })
        },
    );
    cx.read(|cx| {
        let ws = ws.read(cx);
        assert!(ws.model.tab(tab).is_none(), "{n}: the tab went");
        assert!(!ws.agents.contains_key(&tab), "{n}: its slot went");
        assert!(
            !ws.classic_of.contains_key(&tab),
            "{n}: its classic note went"
        );
    });
    drop(view);
    steps.done();
}

/// An agent tab of the scene's workspace: its id and its view.
type AgentAt = (
    crate::layout_model::TabId,
    Entity<crate::agent_tab::AgentTab>,
);

/// `elsewhere_window` with a Claude tab split beside api (shown) and one beside agent (not
/// shown), api's terminal focused again; each titled as a rename would title it (so a first
/// message's own title, which notifies, can't stand in for its status's notify). No agent runs:
/// the tabs are fed decoded events (R12). The context, window, workspace, api's Claude, agent's.
fn claude_beside_api_and_agent() -> (
    ShotApp,
    AnyWindowHandle,
    Entity<Workspace>,
    AgentAt,
    AgentAt,
) {
    use crate::layout_model::TabKind;
    let (mut cx, window, ws) = elsewhere_window();
    let id = |cx: &VisualTestAppContext, name: &str| {
        let i = index_of(cx, &ws, name);
        cx.read(|cx| ws.read(cx).sessions.items()[i].id)
    };
    let (api, agent) = (id(&cx, "api"), id(&cx, "agent"));
    on_ws(&mut cx, window, &ws, |ws, window, cx| {
        ws.agent_beside(api, "claude", window, cx);
        ws.agent_beside(agent, "claude", window, cx);
        let first = ws.sessions.position(|s| s.label() == "api").expect("api");
        ws.select(first, window, cx);
    });
    settle_frames(&mut cx, window);
    let claude = |cx: &mut VisualTestAppContext, name: &str, title: &str| -> AgentAt {
        let i = index_of(cx, &ws, name);
        cx.update(|cx| {
            ws.update(cx, |ws, _| {
                let row = ws.workspace_of(i).expect("its workspace");
                let tab = *ws.model.agent_tabs(row).first().expect("its Claude tab");
                if let Some(TabKind::Agent {
                    provider,
                    cwd,
                    conversation,
                    mode,
                    ..
                }) = ws.model.tab(tab).map(|t| t.kind.clone())
                {
                    let title = Some(title.to_string());
                    let kind = TabKind::Agent {
                        provider,
                        cwd,
                        conversation,
                        mode,
                        title,
                    };
                    ws.model.set_kind(tab, kind);
                }
                (tab, ws.agents[&tab].view.clone())
            })
        })
    };
    let api_claude = claude(&mut cx, "api", "dates");
    let agent_claude = claude(&mut cx, "agent", "build");
    settle_frames(&mut cx, window);
    (cx, window, ws, api_claude, agent_claude)
}

/// Send `text` from agent tab `view` (its composer's Send): its turn starts (Working).
fn agent_sends(
    cx: &mut VisualTestAppContext,
    view: &Entity<crate::agent_tab::AgentTab>,
    text: &str,
) {
    cx.update(|cx| view.update(cx, |t, cx| t.send(text.into(), cx)));
}

/// Agent tab `view` fed `events` as decoded from its process (R12).
fn agent_hears(
    cx: &mut VisualTestAppContext,
    view: &Entity<crate::agent_tab::AgentTab>,
    events: Vec<crate::providers::Event>,
) {
    cx.update(|cx| view.update(cx, |t, cx| t.shot_feed(events, cx)));
}

/// M11aa Task 9: an agent tab's status reaches the chrome as a terminal's does, each by the
/// app's own path (`send`, `shot_feed`'s decoded events, `answer`: the tab's `Status` /
/// `Question` events, heard in `on_agent_event`, `set_agent_status`): agent's Claude (not
/// shown) works (its row's badge and age), api's Claude works (its tab's mark, a view the
/// marks' ticker turns), agent's Claude asks (its row red, the question under its name, a 🔔
/// entry and a banner, kept by the scene), it is allowed (Working), api's Claude is done (its
/// tab's mark pops, a 🔔 entry, no banner for a short run); then api's group is closed and its
/// Claude works there (the header's rolled-up mark).
///
/// Panics unless after each step the window shows what a full redraw does, and it changed.
pub(super) fn chrome_agent_status() {
    use crate::agent_conversation::Answer;
    use crate::groups::GroupKey;
    use crate::providers::Event;
    use crate::sidebar::{SidebarRow, Sub};
    let n = "chrome-agent-status";
    let (mut cx, window, ws, (api_tab, api_claude), (agent_tab, agent_claude)) =
        claude_beside_api_and_agent();
    let ws = &ws;
    let mut steps = Steps::new(n, &mut cx, window, ws);
    // The row agent's Claude is in, as the sidebar shows it.
    let agent_row = |cx: &VisualTestAppContext| {
        cx.read(|cx| {
            let ws = ws.read(cx);
            let (row, _) = ws.model.locate(agent_tab).expect("placed");
            ws.rows_in_view(Instant::now())
                .into_iter()
                .find_map(|r| match r {
                    SidebarRow::Workspace(w) if w.workspace == row => Some(w),
                    _ => None,
                })
                .expect("agent's row")
        })
    };
    steps.step(
        &mut cx,
        window,
        "agent's Claude works (elsewhere)",
        Seen::Changes,
        |cx| agent_sends(cx, &agent_claude, "Check the build"),
    );
    let row = agent_row(&cx);
    assert_eq!(
        (row.agent, row.status, row.age.is_some()),
        (Some(agent_tab), Status::Working, true),
        "{n}: agent's row is about its Claude"
    );
    steps.step(&mut cx, window, "api's Claude works", Seen::Changes, |cx| {
        agent_sends(cx, &api_claude, "Fix the date check")
    });
    let on_screen = cx.read(|cx| ws.read(cx).marks_on_screen());
    let key = cx.read(|cx| ws.read(cx).agents[&api_tab].id);
    assert!(
        on_screen.contains(&((crate::sidebar::AGENT_MARK, key), Status::Working)),
        "{n}: api's Claude's mark is on screen: {on_screen:?}"
    );
    steps.step(
        &mut cx,
        window,
        "the ticker turns api's Claude mark",
        Seen::Changes,
        |cx| {
            for _ in 0..3 {
                cx.advance_clock(crate::status_mark::TICK);
                cx.run_until_parked();
            }
        },
    );
    let unseen = |cx: &VisualTestAppContext| cx.read(|cx| ws.read(cx).notifications.unseen());
    let before = unseen(&cx);
    steps.step(
        &mut cx,
        window,
        "agent's Claude asks",
        Seen::Changes,
        |cx| {
            let ask = Event::Permission {
                id: "p1".into(),
                tool: "Bash".into(),
                what: "ls".into(),
                detail: "ls".into(),
                always: None,
            };
            agent_hears(cx, &agent_claude, vec![ask]);
        },
    );
    let row = agent_row(&cx);
    assert_eq!(
        (row.status, row.sub),
        (
            Status::Waiting,
            Some(Sub::Question("Claude wants to run: ls".into()))
        ),
        "{n}: agent's row waits, its question under its name"
    );
    let tip = cx.read(|cx| ws.read(cx).workspace_tip(row.workspace));
    assert!(
        tip.as_deref()
            .is_some_and(|t| t.ends_with("\nClaude wants to run: ls")),
        "{n}: its row's tooltip has the whole question: {tip:?}"
    );
    assert_eq!(unseen(&cx), before + 1, "{n}: a 🔔 entry for it");
    let banners = cx.read(|cx| ws.read(cx).shot_banners.clone().unwrap_or_default());
    match banners.as_slice() {
        [(_, subtitle, body)] => {
            assert_eq!(body, "Claude wants to run: ls", "{n}: the banner's body");
            assert!(subtitle.ends_with("Claude"), "{n}: who asks: {subtitle}");
        }
        other => panic!("{n}: one banner, not {other:?}"),
    }
    steps.step(&mut cx, window, "it is allowed", Seen::Changes, |cx| {
        cx.update(|cx| {
            agent_claude.update(cx, |t, cx| t.answer(Answer::Allowed, cx).expect("waiting"))
        })
    });
    assert_eq!(
        agent_row(&cx).status,
        Status::Working,
        "{n}: allowed: works"
    );
    let before = unseen(&cx);
    steps.step(
        &mut cx,
        window,
        "api's Claude is done",
        Seen::Changes,
        |cx| {
            let done = Event::TurnDone {
                ok: true,
                message: None,
            };
            agent_hears(cx, &api_claude, vec![done]);
        },
    );
    cx.read(|cx| {
        let ws = ws.read(cx);
        let slot = &ws.agents[&api_tab];
        assert_eq!(slot.status, Status::Done, "{n}");
        assert!(
            slot.pop.is_some(),
            "{n}: its Done pops (its row is on screen)"
        );
        assert_eq!(slot.activity.len(), 2, "{n}: its Activity: Working, Done");
    });
    assert_eq!(unseen(&cx), before + 1, "{n}: a 🔔 entry for it");
    let banners = cx.read(|cx| ws.read(cx).shot_banners.clone().unwrap_or_default());
    assert_eq!(banners.len(), 1, "{n}: a short run makes no banner");
    let key = GroupKey::Folder(super::API.into());
    steps.step(&mut cx, window, "api's group closed", Seen::Changes, |cx| {
        cx.update(|cx| ws.update(cx, |ws, cx| ws.toggle_group(&key, cx)))
    });
    steps.step(
        &mut cx,
        window,
        "api's Claude works in its closed group",
        Seen::Maybe,
        |cx| agent_sends(cx, &api_claude, "and the tests"),
    );
    steps.step(
        &mut cx,
        window,
        "agent's Claude asks again",
        Seen::Changes,
        |cx| {
            let ask = Event::Permission {
                id: "p2".into(),
                tool: "Bash".into(),
                what: "npm test".into(),
                detail: "npm test".into(),
                always: None,
            };
            agent_hears(cx, &agent_claude, vec![ask]);
        },
    );
    let row = agent_row(&cx);
    steps.step(
        &mut cx,
        window,
        "its row's Open goes to it",
        Seen::Changes,
        |cx| {
            on_ws(cx, window, ws, |ws, window, cx| {
                ws.open_waiting(&row, window, cx)
            })
        },
    );
    cx.read(|cx| {
        let ws = ws.read(cx);
        let focused = ws.focused_agent_tab().map(|(tab, _)| tab);
        assert_eq!(focused, Some(agent_tab), "{n}: Open focused agent's Claude");
        // The Activity panel follows the focused agent tab: its title, its status, its log.
        let (label, _, status) = ws.activity_header().expect("an Activity subject");
        assert_eq!(
            (label.as_ref(), status),
            ("build", Status::Waiting),
            "{n}: the Activity header"
        );
        assert_eq!(
            ws.activity_rows().len(),
            4,
            "{n}: its log: works, waits, works, waits"
        );
    });
    // Spec §1.5: an error or a non-zero exit is Done, an error row and an Activity note.
    let newest_note = |cx: &VisualTestAppContext, tab| {
        cx.read(|cx| {
            let ws = ws.read(cx);
            match ws.agents[&tab]
                .activity
                .newest_first()
                .nth(1)
                .map(|e| &e.kind)
            {
                Some(crate::activity::Kind::Note(note)) => Some(note.clone()),
                _ => None,
            }
        })
    };
    steps.step(
        &mut cx,
        window,
        "agent's Claude exits with an error",
        Seen::Changes,
        |cx| {
            use crate::agent_process::ProcessEvent;
            cx.update(|cx| {
                agent_claude.update(cx, |t, cx| {
                    t.shot_process(ProcessEvent::Ended { code: Some(1) }, cx);
                    let stderr_tail = "boom".into();
                    t.shot_process(
                        ProcessEvent::Exited {
                            code: Some(1),
                            stderr_tail,
                        },
                        cx,
                    );
                })
            })
        },
    );
    assert_eq!(
        newest_note(&cx, agent_tab).as_deref(),
        Some("The agent exited (code 1): boom"),
        "{n}: a note under its Done"
    );
    let rows = cx.read(|cx| ws.read(cx).activity_rows());
    assert_eq!(rows.len(), 6, "{n}: the note and Done in the panel's log");
    steps.step(
        &mut cx,
        window,
        "api's Claude can't start",
        Seen::Changes,
        |cx| {
            use crate::agent_process::ProcessEvent;
            let why = "No such file or directory".into();
            cx.update(|cx| {
                api_claude.update(cx, |t, cx| {
                    t.shot_process(ProcessEvent::SpawnFailed(why), cx)
                })
            })
        },
    );
    assert_eq!(
        newest_note(&cx, api_tab).as_deref(),
        Some("Couldn't start Claude: No such file or directory"),
        "{n}: a note under its Done"
    );
    steps.done();
}

/// M11aa Task 9: an agent tab's age on its row turns on the ages' own timer, as a terminal's
/// does (`chrome-ages-current`): agent's Claude (not shown) works, dated 59.6 s back by the
/// scenes' hook (which notifies); then nothing but the timer: its row's "59s" turns "1m", and
/// the window shows what a full redraw does. Nothing else moves (the marks don't tick: no
/// moving mark is on screen). Then the same for the Activity header's age while that Claude
/// tab, done, is focused (fix round 1).
///
/// Panics unless all of that holds.
pub(super) fn chrome_agent_age() {
    use crate::sidebar::SidebarRow;
    let n = "chrome-agent-age";
    let (mut cx, window, ws, _, (agent_tab, agent_claude)) = claude_beside_api_and_agent();
    let ws = &ws;
    agent_sends(&mut cx, &agent_claude, "Check the build");
    settle_frames(&mut cx, window);
    cx.update(|cx| {
        ws.update(cx, |ws, cx| {
            ws.shot_agent_since(agent_tab, Duration::from_millis(59_600), cx)
        })
    });
    settle_frames(&mut cx, window);
    let age = |cx: &VisualTestAppContext| {
        cx.read(|cx| {
            let ws = ws.read(cx);
            let (row, _) = ws.model.locate(agent_tab).expect("placed");
            ws.rows(Instant::now())
                .iter()
                .find_map(|r| match r {
                    SidebarRow::Workspace(w) if w.workspace == row => Some(w.clone()),
                    _ => None,
                })
                .and_then(|w| w.agent.and(w.age))
        })
    };
    assert_eq!(age(&cx).as_deref(), Some("59s"), "{n}: its row's age");
    assert!(
        !cx.read(|cx| ws.read(cx).marks_ticking),
        "{n}: no mark ticks"
    );
    let current = |cx: &mut VisualTestAppContext, what: &str| {
        let now = shown(cx, window);
        let fresh = frame_fresh(cx, window);
        let stale = bytes_differ(&now, &fresh);
        if stale > 0 {
            keep_shot(&format!("{n}-stale-now"), &now);
            keep_shot(&format!("{n}-stale-fresh"), &fresh);
        }
        assert_eq!(
            stale, 0,
            "{n}: {what}: {stale} bytes differ from a full redraw (stale)"
        );
    };
    current(&mut cx, "dated");
    let before = shown(&mut cx, window);
    // The age turns ("59s" to "1m") with nothing notified: only its timer redraws it.
    crate::ui_shots::prof::sleep(Duration::from_millis(500));
    cx.advance_clock(Duration::from_secs(2));
    settle_frames(&mut cx, window);
    assert_eq!(age(&cx).as_deref(), Some("1m"), "{n}: its row's age turned");
    let after = shown(&mut cx, window);
    assert!(
        bytes_differ(&before, &after) > 0,
        "{n}: the turned age is drawn"
    );
    current(&mut cx, "its age turned");
    // The Activity header's age (R9: the status bar keeps its terminal): agent's Claude done,
    // focused, the Activity panel shown and the sidebar hidden (no row's age times it), dated
    // 59.6 s back; its "Done · 59s" turns "Done · 1m" on the timer alone.
    agent_hears(
        &mut cx,
        &agent_claude,
        vec![crate::providers::Event::TurnDone {
            ok: true,
            message: None,
        }],
    );
    on_ws(&mut cx, window, ws, |ws, window, cx| {
        ws.select_tab(agent_tab, window, cx);
        ws.sidebar.hidden = true;
        ws.right_sidebar.hidden = false;
        ws.right_sidebar.panel = crate::right_sidebar::Panel::Activity;
        cx.notify();
    });
    settle_frames(&mut cx, window);
    cx.update(|cx| {
        ws.update(cx, |ws, cx| {
            ws.shot_agent_since(agent_tab, Duration::from_millis(59_600), cx)
        })
    });
    settle_frames(&mut cx, window);
    let header = |cx: &VisualTestAppContext| {
        cx.read(|cx| {
            ws.read(cx)
                .activity_header()
                .map(|(_, words, status)| (words, status))
        })
    };
    let done = |words: &str| Some((words.to_string(), Status::Done));
    assert_eq!(header(&cx), done("Done · 59s"), "{n}: the header");
    assert!(
        !cx.read(|cx| ws.read(cx).marks_ticking),
        "{n}: no mark ticks"
    );
    current(&mut cx, "the header dated");
    let before = shown(&mut cx, window);
    crate::ui_shots::prof::sleep(Duration::from_millis(500));
    cx.advance_clock(Duration::from_secs(2));
    settle_frames(&mut cx, window);
    assert_eq!(
        header(&cx),
        done("Done · 1m"),
        "{n}: the header's age turned"
    );
    let after = shown(&mut cx, window);
    assert!(
        bytes_differ(&before, &after) > 0,
        "{n}: the header's turned age is drawn"
    );
    current(&mut cx, "the header's age turned");
    println!("ui-shots: {n}: its row's and the Activity header's ages turned on their timer");
}

/// A tab bar's first frame: a workspace never shown before, its one pane holding more tabs than
/// its bar shows, the last one shown. Its new bar isn't laid out on that frame, so it can only
/// scroll to the shown tab on the next, which the bar asks for itself (`render_chrome`). Frames
/// only as the app gets them.
///
/// Panics unless the shown tab is in its row and the window shows what a full redraw does.
pub(super) fn chrome_new_bar_reveal() {
    let n = "chrome-new-bar-reveal";
    let (mut cx, window, ws) = super::chrome_parts::chrome_window(|ws, window, cx| {
        let web = std::path::Path::new(super::WEB);
        let api = std::path::Path::new(super::API);
        ws.open_shot_session("many", web, super::SHELL_TEXT, None, window, cx);
        for i in 1..16 {
            ws.open_shot_tab(&format!("tab {i}"), web, super::SHELL_TEXT, window, cx);
        }
        ws.open_shot_session("api", api, super::SHELL_TEXT, None, window, cx);
    });
    let ws = &ws;
    let mut steps = Steps::new(n, &mut cx, window, ws);
    steps.step(
        &mut cx,
        window,
        "a workspace shown the first time",
        Seen::Changes,
        |cx| {
            on_ws(cx, window, ws, |ws, window, cx| {
                let many = ws.workspace_of(0).expect("many's workspace");
                ws.select_workspace(many, window, cx);
            })
        },
    );
    let (row, left, width) = cx.read(|cx| {
        let ws = ws.read(cx);
        let pane = ws.model.focused_pane().expect("many's pane");
        let p = ws.model.pane(pane).expect("the pane");
        let last = p.tabs.len() - 1;
        assert_eq!(p.tabs[last].id, p.active, "{n}: the last tab is shown");
        let scroll = ws
            .tab_scroll
            .borrow()
            .get(&pane)
            .expect("its row")
            .0
            .clone();
        let item = scroll.bounds_for_item(last).expect("laid out");
        (
            scroll.bounds(),
            item.origin.x + scroll.offset().x,
            item.size.width,
        )
    });
    println!("ui-shots: {n}: the row {row:?}, its shown tab drawn at x {left:?}");
    assert!(
        left >= row.left() - px(1.0) && left + width <= row.right() + px(1.0),
        "{n}: the shown tab (x {left:?}, {width:?} wide) is in its row {row:?}"
    );
    steps.done();
}

/// Session `name`'s terminal emits `event` (as its view does from what the program wrote or the
/// connection did; the workspace hears it in `on_terminal_event`).
fn emit(cx: &mut VisualTestAppContext, ws: &Entity<Workspace>, name: &str, event: TerminalEvent) {
    let i = index_of(cx, ws, name);
    let view = cx.read(|cx| ws.read(cx).sessions.items()[i].view.clone());
    cx.update(|cx| view.update(cx, |_, cx| cx.emit(event)));
}

/// The rest of what a session shows in the sidebar and its tab: a script's note and progress
/// (`session.note`, `session.progress`), the program's title (OSC 0/2), the background
/// service's connection lost, the sidebar's notice (an SSH session that couldn't start), and
/// its tips card shown again (Help → Show Tips Card).
///
/// Panics unless after each the window shows what a full redraw does, and it changed.
pub(super) fn chrome_session_facts() {
    let n = "chrome-session-facts";
    let (mut cx, window, ws) = super::chrome_parts::chrome_window(|ws, window, cx| {
        let api = std::path::Path::new(super::API);
        let web = std::path::Path::new(super::WEB);
        ws.open_shot_session("api", api, super::SHELL_TEXT, None, window, cx);
        ws.open_shot_session("agent", web, super::SHELL_TEXT, None, window, cx);
        // One with no name of yours: its row and tab say its program's title.
        ws.open_unnamed_shot_session(Some(web), super::SHELL_TEXT, window, cx);
        let first = ws.sessions.position(|s| s.label() == "api").expect("api");
        ws.select(first, window, cx);
        ws.sidebar.hint_dismissed = true;
    });
    let ws = &ws;
    let unnamed = cx.read(|cx| {
        let ws = ws.read(cx);
        ws.sessions.items()[ws.sessions.position(|s| s.name.is_none()).expect("unnamed")].id
    });
    let mut steps = Steps::new(n, &mut cx, window, ws);
    steps.step(&mut cx, window, "agent's note", Seen::Changes, |cx| {
        let i = index_of(cx, ws, "agent");
        cx.update(|cx| {
            ws.update(cx, |ws, cx| {
                ws.set_note(i, Some("reviewing the diff".into()), cx)
            })
        });
    });
    steps.step(&mut cx, window, "agent's progress", Seen::Changes, |cx| {
        let i = index_of(cx, ws, "agent");
        cx.update(|cx| {
            ws.update(cx, |ws, cx| {
                ws.set_progress(i, Some((40, Some("tests".into()))), cx)
            })
        });
    });
    steps.step(
        &mut cx,
        window,
        "a program's title (OSC 2)",
        Seen::Changes,
        |cx| {
            let i = cx
                .read(|cx| ws.read(cx).index_of(unnamed))
                .expect("its session");
            let view = cx.read(|cx| ws.read(cx).sessions.items()[i].view.clone());
            let title = TerminalEvent::TitleChanged(Some("✳ cargo test".into()));
            cx.update(|cx| view.update(cx, |_, cx| cx.emit(title)));
        },
    );
    steps.step(
        &mut cx,
        window,
        "agent's connection lost",
        Seen::Changes,
        |cx| emit(cx, ws, "agent", TerminalEvent::DaemonLost),
    );
    steps.step(&mut cx, window, "a notice", Seen::Changes, |cx| {
        cx.update(|cx| ws.update(cx, |ws, cx| ws.ssh_failed("host unreachable", cx)))
    });
    steps.step(
        &mut cx,
        window,
        "the tips card back (Help)",
        Seen::Changes,
        |cx| {
            on_ws(cx, window, ws, |ws, window, cx| {
                ws.show_welcome_tips(window, cx)
            })
        },
    );
    steps.done();
}

/// The sidebar's own state and each pane's bar: the sidebar's edge dragged wider, its + menu
/// opened and closed (clicks), the other pane focused (a click on its tab), the next tab shown
/// (⌃Tab), a bar's row scrolled (the wheel), a pane's ⋯ menu opened and closed (Esc), the
/// right sidebar opened (⌥⌘\), and, with the sidebar hidden, the title band's name after its
/// terminal is renamed.
///
/// Panics unless after each the window shows what a full redraw does, and it changed.
pub(super) fn chrome_bars_and_edges() {
    use crate::workspace::{NextTab, ToggleRightSidebar};
    use gpui::{ScrollDelta, ScrollWheelEvent, TouchPhase};

    let n = "chrome-bars-and-edges";
    let (mut cx, window, ws) = super::chrome_parts::chrome_window(|ws, window, cx| {
        let api = std::path::Path::new(super::API);
        ws.open_shot_session("api", api, super::SHELL_TEXT, None, window, cx);
        for i in 1..12 {
            ws.open_shot_tab(&format!("tab {i}"), api, super::SHELL_TEXT, window, cx);
        }
        // Another folder, so the status bar says which pane has the focus.
        let web = std::path::Path::new(super::WEB);
        ws.open_shot_session("web", web, super::SHELL_TEXT, Some(Axis::Row), window, cx);
        ws.open_shot_tab("docs", web, super::SHELL_TEXT, window, cx);
    });
    let ws = &ws;
    cx.update(|cx| {
        cx.bind_keys([
            gpui::KeyBinding::new("ctrl-tab", NextTab, Some("Terminal")),
            gpui::KeyBinding::new("escape", crate::tab_menu::Dismiss, Some("TabMenu")),
        ])
    });
    let mut steps = Steps::new(n, &mut cx, window, ws);
    let none = Modifiers::none();
    let left = MouseButton::Left;
    let (width, height) = cx
        .update_window(window, |_, window, cx| {
            (
                ws.read(cx).sidebar.width,
                f32::from(window.viewport_size().height),
            )
        })
        .expect("the window");
    steps.step(
        &mut cx,
        window,
        "the sidebar's edge dragged wider (held)",
        Seen::Changes,
        |cx| {
            let edge = point(px(width - 1.0), px(height / 2.0));
            cx.simulate_mouse_down(window, edge, left, none);
            cx.simulate_mouse_move(window, edge + point(px(60.0), px(0.0)), left, none);
        },
    );
    // (Released, the edge's line stays lit: the pointer is still on it.)
    steps.step(&mut cx, window, "released", Seen::Maybe, |cx| {
        let at = point(px(width + 59.0), px(height / 2.0));
        cx.simulate_mouse_up(window, at, left, none);
    });
    let wider = cx.read(|cx| ws.read(cx).sidebar.width);
    assert!(
        wider > width,
        "{n}: the sidebar is wider ({width} to {wider})"
    );
    let plus = point(
        px(crate::sidebar::ROW_INSET + 20.0),
        px(height - crate::ui_tokens::size::STATUS_BAR - crate::sidebar::FOOTER / 2.0),
    );
    steps.step(&mut cx, window, "the + menu opened", Seen::Changes, |cx| {
        cx.simulate_mouse_move(window, plus, None, none);
        cx.simulate_click(window, plus, none)
    });
    assert!(
        cx.read(|cx| ws.read(cx).sidebar.new_menu),
        "{n}: the + menu is open"
    );
    steps.step(&mut cx, window, "the + menu closed", Seen::Changes, |cx| {
        cx.simulate_click(window, plus, none)
    });
    let panes = super::shown_panes(&mut cx, ws);
    let other = cx.read(|cx| {
        let focused = ws.read(cx).model.focused_pane();
        panes.iter().copied().find(|p| Some(*p) != focused)
    });
    let other = other.expect("the other pane");
    // Its shown tab (the row is scrolled to it).
    let shown = cx.read(|cx| {
        let p = ws.read(cx).model.pane(other).expect("the pane");
        p.tabs
            .iter()
            .position(|t| t.id == p.active)
            .expect("its shown tab")
    });
    let tab = super::tab_bounds(&mut cx, ws, other, shown).expect("its shown tab");
    steps.step(
        &mut cx,
        window,
        "the other pane focused",
        Seen::Changes,
        |cx| {
            // (The pointer gets there first, as a hand's does: GPUI moves hovers on moves.)
            cx.simulate_mouse_move(window, tab.center(), None, none);
            cx.simulate_click(window, tab.center(), none)
        },
    );
    assert_eq!(
        cx.read(|cx| ws.read(cx).model.focused_pane()),
        Some(other),
        "{n}: focused"
    );
    steps.step(
        &mut cx,
        window,
        "its next tab (⌃Tab)",
        Seen::Changes,
        |cx| cx.simulate_keystrokes(window, "ctrl-tab"),
    );
    let many = panes
        .iter()
        .copied()
        .max_by_key(|p| cx.read(|cx| ws.read(cx).model.pane(*p).map_or(0, |p| p.tabs.len())))
        .expect("api's pane");
    let shown = cx.read(|cx| {
        let p = ws.read(cx).model.pane(many).expect("the pane");
        p.tabs
            .iter()
            .position(|t| t.id == p.active)
            .expect("its shown tab")
    });
    let on_row = super::tab_bounds(&mut cx, ws, many, shown).expect("its shown tab");
    let offset = |cx: &mut VisualTestAppContext| {
        cx.read(|cx| {
            let scrolls = ws.read(cx).tab_scroll.borrow();
            scrolls.get(&many).map(|(s, _)| s.offset().x)
        })
    };
    let before = offset(&mut cx);
    steps.step(
        &mut cx,
        window,
        "api's bar scrolled (the wheel)",
        Seen::Changes,
        |cx| {
            cx.simulate_mouse_move(window, on_row.center(), None, none);
            cx.simulate_event(
                window,
                ScrollWheelEvent {
                    position: on_row.center(),
                    delta: ScrollDelta::Pixels(point(px(200.0), px(0.0))),
                    modifiers: none,
                    touch_phase: TouchPhase::Moved,
                },
            )
        },
    );
    assert_ne!(offset(&mut cx), before, "{n}: the row scrolled");
    steps.step(&mut cx, window, "a ⋯ menu opened", Seen::Changes, |cx| {
        on_ws(cx, window, ws, |ws, window, cx| {
            ws.open_pane_actions(other, point(px(800.0), px(40.0)), window, cx)
        })
    });
    steps.step(&mut cx, window, "Esc closes it", Seen::Changes, |cx| {
        cx.simulate_keystrokes(window, "escape")
    });
    assert!(
        cx.read(|cx| ws.read(cx).pane_actions.is_none()),
        "{n}: closed"
    );
    steps.step(
        &mut cx,
        window,
        "the right sidebar opened",
        Seen::Changes,
        |cx| {
            cx.dispatch_action(window, ToggleRightSidebar);
            // Its slide, by the real clock.
            crate::ui_shots::prof::sleep(Duration::from_millis(400));
        },
    );
    on_ws(&mut cx, window, ws, |ws, window, cx| {
        ws.toggle_sidebar(window, cx)
    });
    let start = Instant::now();
    while cx.read(|cx| ws.read(cx).slide.is_some()) {
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "{n}: the slide ends"
        );
        crate::ui_shots::prof::sleep(Duration::from_millis(16));
        settle_frames(&mut cx, window);
    }
    steps.step(
        &mut cx,
        window,
        "hidden: its terminal renamed",
        Seen::Changes,
        |cx| {
            // (`session.rename`; the workspace has no name of its own, so it takes its terminal's.)
            let i = cx
                .read(|cx| {
                    let ws = ws.read(cx);
                    ws.focused_terminal().and_then(|id| ws.index_of(id))
                })
                .expect("the focused terminal");
            cx.update(|cx| ws.update(cx, |ws, cx| ws.rename_at(i, "renamed", cx)));
        },
    );
    steps.done();
}

/// The sidebar's list scrolled and revealed: 16 workspaces, more than the list shows. A row out
/// of view starts working (nothing to see yet); the list scrolled to it with the wheel shows it
/// as it is now; ⌘1 chooses the first workspace, whose row is out of view, and the list scrolls
/// it into view (`note_active` asks, `render_sidebar` scrolls to it).
///
/// Panics unless the list scrolls, the chosen row is in view, and after each step the window
/// shows what a full redraw does.
pub(super) fn chrome_sidebar_scroll() {
    use crate::workspace::SelectSession1;
    use gpui::{KeyBinding, ScrollDelta, ScrollWheelEvent, TouchPhase};

    let n = "chrome-sidebar-scroll";
    let (mut cx, window, ws) = super::chrome_parts::chrome_window(|ws, window, cx| {
        let api = std::path::Path::new(super::API);
        for i in 0..16 {
            ws.open_shot_session(&format!("w{i}"), api, super::SHELL_TEXT, None, window, cx);
        }
        let first = ws.sessions.position(|s| s.label() == "w0").expect("w0");
        ws.select(first, window, cx);
    });
    let ws = &ws;
    cx.update(|cx| cx.bind_keys([KeyBinding::new("cmd-1", SelectSession1, None)]));
    let mut steps = Steps::new(n, &mut cx, window, ws);
    // The list's viewport, and whether row `i` (its group's header is row 0) lies inside it.
    let in_view = |cx: &mut VisualTestAppContext, i: usize| {
        cx.read(|cx| {
            let scroll = &ws.read(cx).sidebar.scroll;
            let view = scroll.bounds();
            let mut row = scroll.bounds_for_item(i).expect("laid out");
            row.origin.y += scroll.offset().y;
            row.top() >= view.top() - px(1.0) && row.bottom() <= view.bottom() + px(1.0)
        })
    };
    let last = cx.read(|cx| ws.read(cx).model.workspaces().len());
    assert!(in_view(&mut cx, 1), "{n}: the first row shows");
    assert!(!in_view(&mut cx, last), "{n}: the last row is out of view");
    steps.step(
        &mut cx,
        window,
        "a row out of view works",
        Seen::Maybe,
        |cx| report(cx, ws, "w15", Status::Working),
    );
    let list = super::sidebar::row_point(&mut cx, ws, 1, 0.0);
    steps.step(
        &mut cx,
        window,
        "the list scrolled to it (the wheel)",
        Seen::Changes,
        |cx| {
            cx.simulate_mouse_move(window, list, None, Modifiers::none());
            cx.simulate_event(
                window,
                ScrollWheelEvent {
                    position: list,
                    delta: ScrollDelta::Pixels(point(px(0.0), px(-2000.0))),
                    modifiers: Modifiers::none(),
                    touch_phase: TouchPhase::Moved,
                },
            )
        },
    );
    assert!(in_view(&mut cx, last), "{n}: the last row is in view");
    assert!(!in_view(&mut cx, 1), "{n}: the first row is out of view");
    steps.step(
        &mut cx,
        window,
        "⌘1: its row revealed",
        Seen::Changes,
        |cx| cx.simulate_keystrokes(window, "cmd-1"),
    );
    assert!(in_view(&mut cx, 1), "{n}: the chosen row is in view");
    steps.done();
}

/// A closed group's header mark scrolled into view: a group closed, its header (below the 16
/// rows of another group, out of view) carrying a Working mark. While the header is out of view
/// no mark is on screen and the marks' ticker doesn't run; the list scrolled with the wheel
/// brings the header in, and its mark is on screen and the ticker runs.
///
/// Panics unless the header's mark is off screen before the scroll and on screen, with the
/// ticker running, after it, and after each step the window shows what a full redraw does.
pub(super) fn chrome_sidebar_header_mark() {
    use crate::groups::GroupKey;
    use crate::reduce_motion::{SystemReduceMotion, WindowVisible};
    use gpui::{ScrollDelta, ScrollWheelEvent, TouchPhase};

    let n = "chrome-sidebar-header-mark";
    let (mut cx, window, ws) = super::chrome_parts::chrome_window(|ws, window, cx| {
        let api = std::path::Path::new(super::API);
        for i in 0..16 {
            ws.open_shot_session(&format!("w{i}"), api, super::SHELL_TEXT, None, window, cx);
        }
        let web = std::path::Path::new(super::WEB);
        ws.open_shot_session("far", web, super::SHELL_TEXT, None, window, cx);
        let first = ws.sessions.position(|s| s.label() == "w0").expect("w0");
        ws.select(first, window, cx);
        ws.toggle_group(&GroupKey::Folder(super::WEB.into()), cx);
    });
    let ws = &ws;
    cx.update(|cx| {
        use gpui::BorrowAppContext;
        cx.set_global(SystemReduceMotion::for_shots(false));
        cx.set_global(WindowVisible::for_shots(true));
        cx.update_global::<crate::appearance::Appearance, _>(|a, _| a.animations = true)
    });
    let mut steps = Steps::new(n, &mut cx, window, ws);
    let marks = |cx: &mut VisualTestAppContext| cx.read(|cx| ws.read(cx).marks_on_screen().len());
    let ticking = |cx: &mut VisualTestAppContext| cx.read(|cx| ws.read(cx).marks_ticking);
    steps.step(
        &mut cx,
        window,
        "the closed group's terminal works, its header out of view",
        Seen::Maybe,
        |cx| {
            report(cx, ws, "far", Status::Working);
            minutes_old(cx, ws, "far");
        },
    );
    assert_eq!(marks(&mut cx), 0, "{n}: its header's mark is off screen");
    assert!(!ticking(&mut cx), "{n}: nothing on screen moves");
    let list = super::sidebar::row_point(&mut cx, ws, 1, 0.0);
    steps.step(
        &mut cx,
        window,
        "the list scrolled to the header (the wheel)",
        Seen::Changes,
        |cx| {
            cx.simulate_mouse_move(window, list, None, Modifiers::none());
            cx.simulate_event(
                window,
                ScrollWheelEvent {
                    position: list,
                    delta: ScrollDelta::Pixels(point(px(0.0), px(-2000.0))),
                    modifiers: Modifiers::none(),
                    touch_phase: TouchPhase::Moved,
                },
            )
        },
    );
    assert_eq!(marks(&mut cx), 1, "{n}: the header's mark is on screen");
    assert!(ticking(&mut cx), "{n}: the marks' ticker runs");
    steps.done();
}

/// A row's other sources: a waiting terminal's question (read off its screen by `set_status`;
/// without the banner, which would be a real one), the terminal a row is named after and reads
/// its branch from (`named_terminal` / `branch_terminal`: the workspace's focused pane's,
/// changed by Focus Pane Right), an SSH session's host, and a terminal's project
/// (its folder and group).
///
/// An SSH session can't connect here: it opens at `open_ssh`'s last step (`open_now` with the
/// session's `ssh` target), its program a silent `cat` instead of the `ssh` that would reach a
/// host. A project, the same way as a folder's New Workspace opens one (`open_in`), its
/// program a `cat` instead of a login shell (whose prompt would come in meanwhile).
///
/// Panics unless each shows as said and after each step the window shows what a full redraw
/// does, and it changed.
pub(super) fn chrome_row_sources() {
    use crate::restore::{Launch, SavedSession};
    use crate::ssh_target::SshTarget;
    use crate::workspace::{At, FocusPaneRight};

    let n = "chrome-row-sources";
    let (mut cx, window, ws) = super::chrome_parts::chrome_window(|ws, window, cx| {
        let api = std::path::Path::new(super::API);
        let web = std::path::Path::new(super::WEB);
        ws.open_shot_session("api", api, super::SHELL_TEXT, None, window, cx);
        ws.open_shot_session("tests", web, super::SHELL_TEXT, Some(Axis::Row), window, cx);
        ws.open_shot_session("agent", web, super::CLAUDE_PROMPT, None, window, cx);
        let first = ws.sessions.position(|s| s.label() == "api").expect("api");
        ws.select(first, window, cx);
        // Where each is and the branch there, as the places last found them (the scenes look
        // nothing up).
        let now = Instant::now();
        for (name, folder, branch) in [("api", api, "main"), ("tests", web, "feature")] {
            let i = ws.sessions.position(|s| s.label() == name).expect("opened");
            let id = ws.sessions.items()[i].id;
            ws.places.set_folder(id, Some(folder.to_path_buf()), now);
            ws.places.set_branch(folder, Some(branch.into()));
        }
    });
    let ws = &ws;
    let mut steps = Steps::new(n, &mut cx, window, ws);
    let question = |cx: &mut VisualTestAppContext| {
        let i = index_of(cx, ws, "agent");
        cx.read(|cx| ws.read(cx).sessions.items()[i].question.clone())
    };
    steps.step(
        &mut cx,
        window,
        "agent waits: its question",
        Seen::Changes,
        |cx| {
            let i = index_of(cx, ws, "agent");
            cx.update(|cx| ws.update(cx, |ws, cx| ws.set_status(i, Status::Waiting, false, cx)));
        },
    );
    assert_eq!(
        question(&mut cx).as_deref(),
        Some(super::sidebar::CLAUDE_QUESTION),
        "{n}: the question read off its screen"
    );
    // Its age turns every second from here; date it minutes back so no turn lands in a check.
    minutes_old(&mut cx, ws, "agent");
    // api's workspace: named after, and its branch read from, its focused pane's terminal.
    let row = |cx: &mut VisualTestAppContext| {
        cx.read(|cx| {
            let ws = ws.read(cx);
            let i = ws.sessions.position(|s| s.label() == "api").expect("api");
            let w = ws.workspace_of(i).expect("api's workspace");
            let named = ws
                .named_terminal(w)
                .map(|id| ws.terminal_label(id).to_string());
            let branch = ws
                .rows_in_view(Instant::now())
                .into_iter()
                .find_map(|r| match r {
                    crate::sidebar::SidebarRow::Workspace(r) if r.workspace == w => {
                        Some((r.label.to_string(), r.branch.clone()))
                    }
                    _ => None,
                });
            (named, branch)
        })
    };
    let before = row(&mut cx);
    assert_eq!(before.0.as_deref(), Some("api"), "{n}: named after api");
    steps.step(
        &mut cx,
        window,
        "the right pane focused: the row's name and branch follow",
        Seen::Changes,
        |cx| cx.dispatch_action(window, FocusPaneRight),
    );
    let after = row(&mut cx);
    println!("ui-shots: {n}: api's row {before:?} -> {after:?}");
    assert_eq!(after.0.as_deref(), Some("tests"), "{n}: named after tests");
    assert_eq!(
        after.1.as_ref().and_then(|(_, b)| b.as_deref()),
        Some("feature"),
        "{n}: tests's branch"
    );
    let open = |cx: &mut VisualTestAppContext, saved: SavedSession| {
        on_ws(cx, window, ws, |ws, window, cx| {
            let launch = Launch {
                saved,
                run_preset: false,
            };
            let cat = Some(vec!["/bin/cat".to_string()]);
            ws.open_now(launch, At::New, cat, None, window, cx)
        })
    };
    steps.step(
        &mut cx,
        window,
        "an SSH session opened",
        Seen::Changes,
        |cx| {
            let target = SshTarget {
                host: "build.example".into(),
                tmux: false,
                tmux_name: None,
            };
            open(
                cx,
                SavedSession {
                    ssh: Some(target),
                    ..Default::default()
                },
            )
        },
    );
    let facts = cx.read(|cx| ws.read(cx).status_bar_facts()).expect("facts");
    assert!(facts.ssh, "{n}: the bar's terminal is an SSH one");
    assert_eq!(
        facts.branch.as_deref(),
        Some("ssh build.example"),
        "{n}: the bar says its host"
    );
    let docs = std::path::PathBuf::from("/Users/me/code/docs");
    steps.step(
        &mut cx,
        window,
        "a terminal opened in another project",
        Seen::Changes,
        |cx| {
            open(
                cx,
                SavedSession {
                    project: Some(docs.clone()),
                    ..Default::default()
                },
            )
        },
    );
    let facts = cx.read(|cx| ws.read(cx).status_bar_facts()).expect("facts");
    assert_eq!(
        facts.folder.as_deref(),
        Some(docs.as_path()),
        "{n}: its folder"
    );
    steps.done();
}

/// A file tab's own toolbar reaches the tab bar (M11t's preview tab, M11ac's Preview): a
/// preview tab (its title in italics) is kept by turning its Preview on, another by choosing
/// Diff, each by its view switch segment's click handler (M11ae); the tab bar then draws
/// the title upright. The handlers are called without the mouse: a click's own whole-window
/// redraws (a tooltip it dismisses, GPUI's input modality) would hide a stale tab bar, and
/// they don't come with every click.
///
/// Panics unless after each the window shows what a full redraw does, and it changed.
pub(super) fn chrome_file_tab_toolbar() {
    use crate::layout_model::FileMode;

    let n = "chrome-file-tab-toolbar";
    let dir = std::env::temp_dir().join(format!("fuxx-ui-shots-{n}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("the scene's folder");
    let (readme, notes) = (dir.join("README.md"), dir.join("notes.md"));
    std::fs::write(&readme, "# Read me\n\nSome *words*.\n").expect("README.md");
    std::fs::write(&notes, "# Notes\n\n- one\n- two\n").expect("notes.md");
    let (mut cx, window, ws) = {
        let (dir, readme) = (dir.clone(), readme.clone());
        super::chrome_parts::chrome_window(move |ws, window, cx| {
            ws.open_shot_session("api", &dir, super::SHELL_TEXT, None, window, cx);
            ws.preview_file_tab(readme, FileMode::Plain, window, cx);
        })
    };
    let ws = &ws;
    let mut steps = Steps::new(n, &mut cx, window, ws);
    let focused = |cx: &mut VisualTestAppContext| {
        cx.read(|cx| {
            let ws = ws.read(cx);
            let tab = ws.model.focused_tab().expect("a tab");
            let view = ws.focused_file_tab().expect("a file tab").clone();
            (tab, view, ws.model.is_preview(tab))
        })
    };
    let kept =
        |cx: &mut VisualTestAppContext, tab| !cx.read(|cx| ws.read(cx).model.is_preview(tab));
    let (tab, view, preview) = focused(&mut cx);
    assert!(preview, "{n}: README.md opened as a preview tab");
    steps.step(&mut cx, window, "Preview turned on", Seen::Changes, |cx| {
        cx.update_window(window, |_, window, cx| {
            view.update(cx, |t, cx| t.choose_preview_for_shots(true, window, cx))
        })
        .expect("the window")
    });
    assert!(kept(&mut cx, tab), "{n}: kept by Preview");
    on_ws(&mut cx, window, ws, |ws, window, cx| {
        ws.preview_file_tab(notes, FileMode::Plain, window, cx)
    });
    settle_frames(&mut cx, window);
    let (tab, view, preview) = focused(&mut cx);
    assert!(preview, "{n}: notes.md opened as a preview tab");
    cx.update(|cx| view.update(cx, |t, cx| t.pretend_in_repo_for_shots(cx)));
    settle_frames(&mut cx, window);
    steps.step(&mut cx, window, "Diff chosen", Seen::Changes, |cx| {
        cx.update(|cx| view.update(cx, |t, cx| t.choose_mode_for_shots(FileMode::Diff, cx)))
    });
    assert!(kept(&mut cx, tab), "{n}: kept by the view switch");
    steps.done();
    std::fs::remove_dir_all(&dir).ok();
}
