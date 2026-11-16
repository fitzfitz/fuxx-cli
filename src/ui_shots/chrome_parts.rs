//! The chrome cache (M11p): which cached chrome parts a breath, a notify or a pane change redraws,
//! the kill switch, and that what is shown is always current.

use super::window::shot_notifications;
use super::*;

/// This file, as `file!()` gives it (repo-relative under cargo): `registry::AREAS` pairs it with
/// `SCENES`, so `--changed` runs these scenes when this file changes.
pub(super) const FILE: &str = file!();

/// This area's scenes (`registry.rs` runs them in its `ORDER`).
pub(super) const SCENES: &[Scene] = &[
    Scene {
        name: "chrome-breath-skips-parts",
        run: chrome_breath_skips_parts,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: &[
            "src/chrome_cache.rs",
            "src/sidebar.rs",
            "src/status_bar.rs",
            "src/tab_bar.rs",
            "src/title_area.rs",
        ],
    },
    Scene {
        name: "chrome-notify-redraws-parts",
        run: chrome_notify_redraws_parts,
        themes: Themes::One,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/chrome_cache.rs",
            "src/sidebar.rs",
            "src/status_bar.rs",
            "src/tab_bar.rs",
            "src/title_area.rs",
            "src/sidebar_toggle.rs",
            "src/notification_list.rs",
        ],
    },
    Scene {
        name: "chrome-pane-churn",
        run: chrome_pane_churn,
        themes: Themes::One,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/chrome_cache.rs",
            "src/panes.rs",
            "src/split_tree.rs",
            "src/sidebar.rs",
            "src/status_bar.rs",
            "src/tab_bar.rs",
            "src/title_area.rs",
        ],
    },
    Scene {
        name: "chrome-kill-switch",
        run: chrome_kill_switch,
        themes: Themes::One,
        // Serial: (a) a child fuxx (FUXX_CHROME_CACHE=off).
        serial: true,
        kind: Kind::State,
        sources: &[
            "src/chrome_cache.rs",
            "src/sidebar.rs",
            "src/status_bar.rs",
            "src/tab_bar.rs",
            "src/title_area.rs",
        ],
    },
    Scene {
        name: "chrome-pixels-same",
        run: chrome_pixels_same,
        themes: Themes::One,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/chrome_cache.rs",
            "src/sidebar.rs",
            "src/status_bar.rs",
            "src/tab_bar.rs",
            "src/title_area.rs",
        ],
    },
    Scene {
        name: "chrome-ages-current",
        run: chrome_ages_current,
        themes: Themes::One,
        // Serial: (c) "59s" read within 400 ms by the real clock.
        serial: true,
        kind: Kind::Visual,
        sources: &[
            "src/chrome_cache.rs",
            "src/sidebar.rs",
            "src/status_bar.rs",
            "src/status.rs",
        ],
    },
    Scene {
        name: "chrome-working-marks",
        run: chrome_working_marks,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: &[
            "src/chrome_cache.rs",
            "src/mark_view.rs",
            "src/status_mark.rs",
            "src/sidebar.rs",
            "src/status_bar.rs",
            "src/tab_bar.rs",
            "src/title_area.rs",
        ],
    },
    Scene {
        name: "chrome-folder-polled",
        run: chrome_folder_polled,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: &[
            "src/chrome_cache.rs",
            "src/git_branch.rs",
            "src/sidebar.rs",
            "src/status_bar.rs",
        ],
    },
    Scene {
        name: "chrome-followed",
        run: chrome_followed,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: &[
            "src/chrome_cache.rs",
            "src/status_bar.rs",
            "src/right_sidebar.rs",
        ],
    },
    Scene {
        name: "chrome-title-once",
        run: chrome_title_once,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: &[
            "src/chrome_cache.rs",
            "src/title_area.rs",
            "src/workspace.rs",
        ],
    },
    Scene {
        name: "chrome-rows-once",
        run: chrome_rows_once,
        themes: Themes::One,
        // Serial: (c) "59s" read within 400 ms by the real clock.
        serial: true,
        kind: Kind::Visual,
        sources: &["src/chrome_cache.rs", "src/sidebar.rs"],
    },
];

/// How many times `c` rendered over `act` and one explicit draw: reset the counters, `act`,
/// draw, read. A notify inside `act` already makes the test platform draw a frame when its
/// effects flush, so a notifying `act` plus the explicit draw is `FRAMES_PER_TICK` frames.
fn draws_of(
    c: Counted,
    cx: &mut VisualTestAppContext,
    window: AnyWindowHandle,
    act: impl FnOnce(&mut VisualTestAppContext),
) -> u64 {
    chrome_cache::reset();
    act(cx);
    draw(cx, window);
    chrome_cache::get(c)
}

/// Panics (`what`) unless the window as it was last drawn (no draw now) is what a full redraw
/// draws. On a failure both are kept as `stale-{now,fresh}` (in `target/ui-shots/failed/<scene>/`).
fn assert_shown_current(cx: &mut VisualTestAppContext, window: AnyWindowHandle, what: &str) {
    let image = cx.capture_screenshot(window).expect("capture the window");
    let shown = Shot {
        width: image.width(),
        height: image.height(),
        rgba: image.into_raw(),
    };
    let fresh = frame_fresh(cx, window);
    let differ = bytes_differ(&shown, &fresh);
    if differ > 0 {
        keep_shot("stale-now", &shown);
        keep_shot("stale-fresh", &fresh);
    }
    assert_eq!(
        differ, 0,
        "{what}: {differ} bytes differ from a full redraw (stale)"
    );
}

/// M11p Task 4, fix round 1 (Ruling 8): ages never go stale with the chrome cached, with
/// nothing breathing and frames only as the app would get them (`settle_frames`).
/// - Done and Idle terminals only, animations off: as an age label turns, the row and the
///   status bar show it; the tooltip of a Done tab that is neither its row's subject nor the
///   status bar's terminal (so no age timer covers it) says its age as it opens, not as the
///   bar was last drawn.
/// - The last Working terminal turns Done (the marks' ticker stops on its next tick): the
///   row's and the status bar's age still turn on time.
///
/// Panics unless all of that holds.
fn chrome_ages_current() {
    use std::time::{Duration, Instant};

    use crate::sidebar::SidebarRow;

    let n = "chrome-ages-current";
    let api = std::path::Path::new(API);
    let web = std::path::Path::new(WEB);

    // Done and Idle only, animations off.
    let (mut cx, window, ws) = chrome_window(move |ws, window, cx| {
        ws.open_shot_session("api", api, SHELL_TEXT, None, window, cx);
        ws.open_shot_session("tests", api, SHELL_TEXT, Some(Axis::Row), window, cx);
        ws.open_shot_session("shell", web, SHELL_TEXT, None, window, cx);
        let first = ws.workspace_of(0).expect("api's workspace");
        ws.select_workspace(first, window, cx);
    });
    cx.update(|cx| cx.update_global::<Appearance, _>(|a, _| a.animations = false));
    let since = |cx: &mut VisualTestAppContext, i: usize, ago: Duration| {
        cx.update(|cx| {
            ws.update(cx, |ws, cx| {
                let s = ws.sessions.get_mut(i).expect("a session");
                s.status = Status::Done;
                s.status_since = Instant::now() - ago;
                cx.notify();
            })
        })
    };
    since(&mut cx, 0, Duration::from_secs(5 * 60 + 10));
    since(&mut cx, 1, Duration::from_secs(5 * 60 + 10));
    settle_frames(&mut cx, window);
    // The row's subject (S, also focused: the status bar's) and the other Done one (O).
    let subject = |cx: &mut VisualTestAppContext| {
        cx.read(|cx| {
            let ws = ws.read(cx);
            let api_ws = ws.workspace_of(0).expect("api's workspace");
            ws.rows_in_view(Instant::now())
                .into_iter()
                .find_map(|r| match r {
                    SidebarRow::Workspace(w) if w.workspace == api_ws => w.subject,
                    _ => None,
                })
                .expect("api's row has a subject")
        })
    };
    let s = subject(&mut cx);
    let o = 1 - s;
    cx.update_window(window, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            let id = ws.sessions.items()[s].id;
            let tab = ws.model.terminal_tab(id).expect("S's tab");
            let (_, pane) = ws.model.locate(tab).expect("S's pane");
            ws.select_pane(pane, window, cx);
        })
    })
    .expect("focus S");
    since(&mut cx, o, Duration::from_millis(59_600));
    settle_frames(&mut cx, window);
    assert_eq!(subject(&mut cx), s, "{n}: the subject stays");
    let followed = cx.read(|cx| {
        let ws = ws.read(cx);
        ws.right_sidebar.followed == Some(ws.sessions.items()[s].id)
    });
    assert!(followed, "{n}: the status bar follows S");
    assert_shown_current(&mut cx, window, &format!("{n}: done, before"));
    // O's age turns ("59s" to "1m") with nothing notified.
    prof::sleep(Duration::from_millis(500));
    cx.advance_clock(Duration::from_secs(2));
    settle_frames(&mut cx, window);
    // O's tab's tooltip, opened now (before any full redraw, which would draw its bar anew).
    let at = cx.read(|cx| {
        let ws = ws.read(cx);
        let id = ws.sessions.items()[o].id;
        let tab = ws.model.terminal_tab(id).expect("O's tab");
        let (_, pane) = ws.model.locate(tab).expect("O's pane");
        let scroll = ws
            .tab_scroll
            .borrow()
            .get(&pane)
            .expect("its row")
            .0
            .clone();
        scroll.bounds_for_item(0).expect("laid out").center()
    });
    if let Ok(mut shown) = crate::sidebar_toggle::SHOWN_TIP.lock() {
        *shown = None;
    }
    cx.simulate_mouse_move(window, at, None, gpui::Modifiers::none());
    cx.advance_clock(Duration::from_secs(1));
    settle_frames(&mut cx, window);
    draw(&mut cx, window);
    let tip = crate::sidebar_toggle::SHOWN_TIP
        .lock()
        .ok()
        .and_then(|t| t.clone());
    println!("ui-shots: {n}: O's tooltip {tip:?}");
    assert_eq!(
        tip.as_deref(),
        Some("tests — done 1m"),
        "{n}: O's tooltip says its age as it opens"
    );
    assert_shown_current(&mut cx, window, &format!("{n}: done, O's age turned"));

    // The last Working terminal turns Done: the ticker stops; the ages still turn.
    let (mut cx, window, ws) = chrome_window(move |ws, window, cx| {
        ws.open_shot_session("api", api, SHELL_TEXT, None, window, cx);
        ws.open_shot_session("shell", web, SHELL_TEXT, None, window, cx);
        let first = ws.workspace_of(0).expect("api's workspace");
        ws.select_workspace(first, window, cx);
    });
    let set = |cx: &mut VisualTestAppContext, status: Status, ago: Duration| {
        cx.update(|cx| {
            ws.update(cx, |ws, cx| {
                let s = ws.sessions.get_mut(0).expect("api");
                s.status = status;
                s.status_since = Instant::now() - ago;
                cx.notify();
            })
        })
    };
    set(&mut cx, Status::Working, Duration::from_secs(1));
    settle_frames(&mut cx, window);
    cx.advance_clock(Duration::from_millis(250));
    settle_frames(&mut cx, window);
    assert!(
        cx.read(|cx| ws.read(cx).marks_ticking),
        "{n}: the marks tick while api works"
    );
    set(&mut cx, Status::Done, Duration::from_millis(59_600));
    settle_frames(&mut cx, window);
    cx.advance_clock(Duration::from_millis(250));
    settle_frames(&mut cx, window);
    assert!(
        !cx.read(|cx| ws.read(cx).marks_ticking),
        "{n}: the ticker stopped"
    );
    // (No check here: a full redraw would run `Workspace::render`, which starts the age timer.)
    prof::sleep(Duration::from_millis(500));
    cx.advance_clock(Duration::from_secs(2));
    settle_frames(&mut cx, window);
    assert_shown_current(&mut cx, window, &format!("{n}: done, its age turned"));

    // Ages turn while the marks tick (M11p Task 6: a tick notifies only the marks' clock, so it
    // redraws no age, and the sidebar, with no moving mark, isn't drawn by it). tests is Done
    // 59.6 s and focused (the status bar's), next to api Working (its tab's mark ticks).
    let (mut cx, window, ws) = chrome_window(move |ws, window, cx| {
        ws.open_shot_session("api", api, SHELL_TEXT, None, window, cx);
        ws.open_shot_session("tests", api, SHELL_TEXT, Some(Axis::Row), window, cx);
        ws.open_shot_session("shell", web, SHELL_TEXT, None, window, cx);
        let first = ws.workspace_of(0).expect("api's workspace");
        ws.select_workspace(first, window, cx);
    });
    cx.update(|cx| {
        ws.update(cx, |ws, cx| {
            let now = Instant::now();
            for (i, status, ago) in [
                (0, Status::Working, Duration::from_secs(5 * 60 + 10)),
                (1, Status::Done, Duration::from_millis(59_600)),
            ] {
                let s = ws.sessions.get_mut(i).expect("a session");
                s.status = status;
                s.status_since = now - ago;
            }
            cx.notify();
        })
    });
    cx.update_window(window, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            let id = ws.sessions.items()[1].id;
            let tab = ws.model.terminal_tab(id).expect("tests' tab");
            let (_, pane) = ws.model.locate(tab).expect("tests' pane");
            ws.select_pane(pane, window, cx);
        })
    })
    .expect("focus tests");
    settle_frames(&mut cx, window);
    cx.advance_clock(status_mark::TICK);
    settle_frames(&mut cx, window);
    let ticking = |cx: &mut VisualTestAppContext| cx.read(|cx| ws.read(cx).marks_ticking);
    assert!(ticking(&mut cx), "{n}: the marks tick while api works");
    let followed = cx.read(|cx| {
        let ws = ws.read(cx);
        ws.right_sidebar.followed == Some(ws.sessions.items()[1].id)
    });
    assert!(followed, "{n}: the status bar follows tests");
    prof::sleep(Duration::from_millis(500));
    cx.advance_clock(Duration::from_secs(2));
    settle_frames(&mut cx, window);
    assert!(ticking(&mut cx), "{n}: the marks still tick");
    assert_shown_current(&mut cx, window, &format!("{n}: ticking, its age turned"));
}

/// M11p Task 6: with an agent working, the marks' 100 ms clock turns its marks without notifying
/// the workspace. api works in a pane split beside web (Done), focused, so the status bar
/// follows it; a second workspace (Idle) sits in the sidebar.
///
/// The limit (GPUI, pinned rev): a notify of a view marks every view above it dirty too
/// (`Window::mark_view_dirty`, window.rs:1814), and a dirty cached view is drawn again
/// (view.rs:390). So a part holding a moving mark (api's tab bar, the status bar) is still drawn
/// once per tick; every other part isn't (the title area, web's tab bar, the sidebar, whose rows
/// show still badges), and the rows aren't built again.
///
/// Panics unless, over 10 ticks: the title area and the sidebar aren't drawn, the tab bars are
/// drawn at most once a tick (api's only), the status bar at most once a tick, the rows aren't
/// built, and each moving mark view is drawn once a tick (10).
fn chrome_working_marks() {
    use std::time::{Duration, Instant};

    let n = "chrome-working-marks";
    let (mut cx, window, ws) = chrome_window(|ws, window, cx| {
        let api = std::path::Path::new(API);
        ws.open_shot_session("api", api, SHELL_TEXT, None, window, cx);
        ws.open_shot_session("web", api, SHELL_TEXT, Some(Axis::Row), window, cx);
        ws.open_shot_session(
            "shell",
            std::path::Path::new(WEB),
            SHELL_TEXT,
            None,
            window,
            cx,
        );
        let first = ws.workspace_of(0).expect("api's workspace");
        ws.select_workspace(first, window, cx);
    });
    cx.update(|cx| {
        ws.update(cx, |ws, cx| {
            // 5m10s: no age label turns during the ticks (an age turning redraws every part).
            for (i, status) in [(0, Status::Working), (1, Status::Done)] {
                let s = ws.sessions.get_mut(i).expect("a session");
                s.status = status;
                s.status_since = Instant::now() - Duration::from_secs(5 * 60 + 10);
            }
            cx.notify();
        })
    });
    cx.update_window(window, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            let id = ws.sessions.items()[0].id;
            let tab = ws.model.terminal_tab(id).expect("api's tab");
            let (_, pane) = ws.model.locate(tab).expect("api's pane");
            ws.select_pane(pane, window, cx);
        })
    })
    .expect("focus api");
    settle_frames(&mut cx, window);
    cx.advance_clock(status_mark::TICK);
    settle_frames(&mut cx, window);
    assert!(
        cx.read(|cx| ws.read(cx).marks_ticking),
        "{n}: the marks tick while api works"
    );
    let followed = cx.read(|cx| {
        let ws = ws.read(cx);
        ws.right_sidebar.followed == Some(ws.sessions.items()[0].id)
    });
    assert!(followed, "{n}: the status bar follows api");
    // Each moving mark's view on screen, and how often it was drawn so far.
    let moving = |cx: &mut VisualTestAppContext| {
        cx.read(|cx| {
            let ws = ws.read(cx);
            let marks = ws.marks.borrow();
            let mut views: Vec<_> = (ws.marks_on_screen().into_iter())
                .filter_map(|(key, _)| marks.get(&key).map(|v| (key, v.read(cx))))
                .filter(|(_, v)| v.moves())
                .map(|(key, v)| (key, v.renders))
                .collect();
            views.sort();
            views
        })
    };
    let before = moving(&mut cx);
    let api_id = cx.read(|cx| ws.read(cx).sessions.items()[0].id);
    let keys: Vec<_> = before.iter().map(|(key, _)| *key).collect();
    assert_eq!(
        keys,
        [("status-bar", api_id), ("tab", api_id)],
        "{n}: api's moving marks: the status bar's and its tab's"
    );
    chrome_cache::reset();
    for _ in 0..10 {
        cx.advance_clock(status_mark::TICK);
        settle_frames(&mut cx, window);
    }
    let frames = chrome_cache::get(Counted::Workspace);
    let parts = PARTS.map(|c| (c, chrome_cache::get(c)));
    let rows = chrome_cache::get(Counted::SidebarRows);
    println!(
        "ui-shots: {n}: 10 ticks: workspace {frames} frames; parts {parts:?}; rows built {rows}"
    );
    for c in [Counted::Title, Counted::Sidebar] {
        let count = chrome_cache::get(c);
        assert_eq!(count, 0, "{n}: {c:?} drawn {count} times in 10 ticks");
    }
    for c in [Counted::TabBar, Counted::StatusBar] {
        let count = chrome_cache::get(c);
        assert!(count <= 10, "{n}: {c:?} drawn {count} times in 10 ticks");
    }
    assert_eq!(rows, 0, "{n}: the rows built in 10 ticks");
    let after = moving(&mut cx);
    let drawn: Vec<_> = (before.iter().zip(&after))
        .map(|((key, was), (_, now))| (*key, now - was))
        .collect();
    println!("ui-shots: {n}: moving marks drawn {drawn:?}");
    for (key, count) in drawn {
        assert_eq!(
            count, 10,
            "{n}: mark {key:?} drawn {count} times in 10 ticks"
        );
    }

    // The status bar follows web (Done) now: api's mark there is no longer drawn. Its view is
    // kept, parked: not counted as on screen nor read each frame (its notifies redraw nothing).
    // Only api's tab moves; the status bar isn't drawn by the ticks.
    cx.update_window(window, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            let id = ws.sessions.items()[1].id;
            let tab = ws.model.terminal_tab(id).expect("web's tab");
            let (_, pane) = ws.model.locate(tab).expect("web's pane");
            ws.select_pane(pane, window, cx);
        })
    })
    .expect("focus web");
    settle_frames(&mut cx, window);
    let parked = cx.read(|cx| {
        let ws = ws.read(cx);
        let key = ("status-bar", ws.sessions.items()[0].id);
        let kept = ws.marks.borrow().contains_key(&key);
        let shown = ws.marks_on_screen().iter().any(|(k, _)| *k == key);
        (kept, shown)
    });
    assert_eq!(parked, (true, false), "{n}: api's status-bar mark parked");
    let before = moving(&mut cx);
    chrome_cache::reset();
    for _ in 0..10 {
        cx.advance_clock(status_mark::TICK);
        settle_frames(&mut cx, window);
    }
    let parts = PARTS.map(|c| (c, chrome_cache::get(c)));
    let after = moving(&mut cx);
    let drawn: Vec<_> = (before.iter().zip(&after))
        .map(|((key, was), (_, now))| (*key, now - was))
        .collect();
    println!("ui-shots: {n}: web followed: parts {parts:?}; moving marks drawn {drawn:?}");
    assert_eq!(
        chrome_cache::get(Counted::StatusBar),
        0,
        "{n}: web followed: the status bar drawn by the ticks"
    );
    let api_id = cx.read(|cx| ws.read(cx).sessions.items()[0].id);
    assert_eq!(
        drawn,
        [(("tab", api_id), 10)],
        "{n}: web followed: only api's tab mark moves, drawn each tick"
    );

    // An agent working in another workspace: its row shows a still badge, it isn't followed
    // and no tab of it is on screen, so nothing on screen moves: the clock stops and the
    // window draws nothing while time passes.
    let (mut cx, window, ws) = chrome_window(|ws, window, cx| {
        let api = std::path::Path::new(API);
        ws.open_shot_session("api", api, SHELL_TEXT, None, window, cx);
        ws.open_shot_session(
            "agent",
            std::path::Path::new(WEB),
            SHELL_TEXT,
            None,
            window,
            cx,
        );
        let first = ws.workspace_of(0).expect("api's workspace");
        ws.select_workspace(first, window, cx);
    });
    cx.update(|cx| {
        ws.update(cx, |ws, cx| {
            let s = ws.sessions.get_mut(1).expect("agent");
            s.status = Status::Working;
            s.status_since = Instant::now() - Duration::from_secs(5 * 60 + 10);
            cx.notify();
        })
    });
    settle_frames(&mut cx, window);
    cx.advance_clock(status_mark::TICK);
    settle_frames(&mut cx, window);
    let ticking = cx.read(|cx| ws.read(cx).marks_ticking);
    chrome_cache::reset();
    for _ in 0..10 {
        cx.advance_clock(status_mark::TICK);
        settle_frames(&mut cx, window);
    }
    let frames = chrome_cache::get(Counted::Workspace);
    println!("ui-shots: {n}: agent elsewhere: ticking {ticking}; {frames} frames in 10 ticks");
    assert!(!ticking, "{n}: agent elsewhere: the marks' clock stopped");
    assert_eq!(frames, 0, "{n}: agent elsewhere: frames drawn in 10 ticks");
}

/// Two terminals split side by side (two tab bars) in one workspace, and a second workspace:
/// the chrome scenes' window with every part on screen.
fn chrome_split_window() -> (ShotApp, AnyWindowHandle, gpui::Entity<Workspace>) {
    chrome_window(|ws, window, cx| {
        let api = std::path::Path::new("/Users/me/code/api");
        ws.open_shot_session("api", api, SHELL_TEXT, None, window, cx);
        ws.open_shot_session("web", api, SHELL_TEXT, Some(Axis::Row), window, cx);
        ws.open_shot_session("docs", api, SHELL_TEXT, None, window, cx);
        let first = ws.workspace_of(0).expect("api's workspace");
        ws.select_workspace(first, window, cx);
    })
}

/// `breaths` breath ticks (the focused terminal notifies, as the glow does, then a draw):
/// each part's renders over them, and the workspace's.
fn breath_counts(
    cx: &mut VisualTestAppContext,
    window: AnyWindowHandle,
    ws: &gpui::Entity<Workspace>,
    breaths: u64,
) -> (u64, [(Counted, u64); 4]) {
    let view = focused_view(cx, ws);
    chrome_cache::reset();
    for _ in 0..breaths {
        cx.update(|cx| view.update(cx, |_, cx| cx.notify()));
        draw(cx, window);
    }
    (
        chrome_cache::get(Counted::Workspace),
        PARTS.map(|c| (c, chrome_cache::get(c))),
    )
}

/// M11p Task 4 (was Task 1's `chrome-counts-baseline`): a breath redraws the workspace (the
/// terminal's ancestor) but none of the chrome's cached parts, which reuse their last frame.
///
/// Panics unless 10 breath ticks draw the workspace 10 × `FRAMES_PER_TICK` times and each part
/// (sidebar, status bar, both tab bars, title area) not once; the sidebar's rows aren't built.
fn chrome_breath_skips_parts() {
    let n = "chrome-breath-skips-parts";
    let (mut cx, window, ws) = chrome_split_window();
    let (frames, parts) = breath_counts(&mut cx, window, &ws, 10);
    let rows = chrome_cache::get(Counted::SidebarRows);
    println!("ui-shots: {n}: workspace {frames} frames; parts {parts:?}; rows built {rows}");
    assert_eq!(frames, 10 * FRAMES_PER_TICK, "{n}: the breaths drew");
    for (c, count) in parts {
        assert_eq!(count, 0, "{n}: {c:?} rendered {count} times in 10 breaths");
    }
    assert_eq!(rows, SIDEBAR_ROWS_PER_FRAME, "{n}: the rows built");
}

/// M11p Task 4: one notify of the workspace redraws every part once (each observes it), and
/// the next frame, with nothing new, none. Then the title area (the sidebar hidden): its
/// name shows once the hide slide ends, its band buttons take their clicks, and the box it
/// fills takes none (the band under it still starts a window drag); the sidebar, shown again
/// by the band's button, is drawn as it is. After every step the frame is the one a full
/// redraw (every part drawn afresh) gives.
///
/// Panics unless all of that holds.
fn chrome_notify_redraws_parts() {
    use crate::sidebar_toggle::{BUTTON_Y, HIT, SLIDE};
    use crate::title_area::band_buttons;
    use gpui::Modifiers;

    let n = "chrome-notify-redraws-parts";
    let (mut cx, window, ws) = chrome_split_window();
    let frames = draws_of(Counted::Workspace, &mut cx, window, |cx| {
        cx.update(|cx| ws.update(cx, |_, cx| cx.notify()));
        cx.run_until_parked();
    });
    // (Read from the same notify and draw.)
    let parts = PARTS.map(|c| (c, chrome_cache::get(c)));
    println!("ui-shots: {n}: one notify: workspace {frames} frames; parts {parts:?}");
    assert_eq!(
        frames, FRAMES_PER_TICK,
        "{n}: the notify's frame and a draw"
    );
    for (c, count) in parts {
        let bars = if c == Counted::TabBar { 2 } else { 1 };
        assert_eq!(
            count, bars,
            "{n}: {c:?} rendered {count} times for one notify"
        );
    }
    assert_fresh(&mut cx, window, &format!("{n}: after a notify"));

    // The sidebar hidden (⌘\), sliding out: the name waits for the slide's end, then shows
    // with nothing notified (the workspace ends the slide while drawing).
    cx.update_window(window, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.toggle_sidebar(window, cx))
    })
    .expect("hide the sidebar");
    let start = std::time::Instant::now();
    while cx.read(|cx| ws.read(cx).slide.is_some()) {
        assert!(start.elapsed() < SLIDE * 20, "{n}: the slide ends");
        prof::sleep(std::time::Duration::from_millis(16));
        cx.run_until_parked();
        next_frame(&mut cx, window);
    }
    cx.run_until_parked();
    draw(&mut cx, window);
    assert_fresh(&mut cx, window, &format!("{n}: the hide slide ended"));

    // The band under the title's box: a press there starts a window drag.
    let width = f32::from(
        cx.update_window(window, |_, window, _| window.viewport_size().width)
            .expect("the window's width"),
    );
    let [bell_x, _, sidebar_x] = band_buttons(width);
    let band = point(px(width * 0.25), px(BUTTON_Y + HIT / 2.0));
    cx.simulate_mouse_down(window, band, gpui::MouseButton::Left, Modifiers::none());
    assert!(
        cx.read(|cx| ws.read(cx).band_pressed),
        "{n}: a press on the band under the title's box reaches the band"
    );
    cx.simulate_mouse_up(window, band, gpui::MouseButton::Left, Modifiers::none());
    // The 🔔: its popover.
    let bell = point(px(bell_x + HIT / 2.0), px(BUTTON_Y + HIT / 2.0));
    cx.simulate_click(window, bell, Modifiers::none());
    cx.run_until_parked();
    assert!(
        cx.read(|cx| ws.read(cx).notifications_popover.is_some()),
        "{n}: a click on the band's 🔔 opens its popover"
    );
    draw(&mut cx, window);
    assert_fresh(&mut cx, window, &format!("{n}: the popover open"));
    cx.update_window(window, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.dismiss_notifications(window, cx))
    })
    .expect("close the popover");
    cx.run_until_parked();
    // The sidebar button: the sidebar slides back in.
    let toggle = point(px(sidebar_x + HIT / 2.0), px(BUTTON_Y + HIT / 2.0));
    cx.simulate_click(window, toggle, Modifiers::none());
    assert!(
        !cx.read(|cx| ws.read(cx).sidebar.hidden),
        "{n}: a click on the band's sidebar button shows the sidebar"
    );
    let start = std::time::Instant::now();
    while cx.read(|cx| ws.read(cx).slide.is_some()) {
        assert!(start.elapsed() < SLIDE * 20, "{n}: the slide ends");
        prof::sleep(std::time::Duration::from_millis(16));
        cx.run_until_parked();
        next_frame(&mut cx, window);
    }
    cx.run_until_parked();
    draw(&mut cx, window);
    assert_fresh(&mut cx, window, &format!("{n}: the show slide ended"));

    // With the sidebar shown, the top tab bars sit under the title's box: a click on the
    // other pane's tab reaches it.
    let (other, tab) = cx.read(|cx| {
        let ws = ws.read(cx);
        let focused = ws.model.focused_pane().expect("a focused pane");
        let other = ws
            .shown_layout()
            .expect("a layout")
            .leaves()
            .into_iter()
            .find(|p| *p != focused)
            .expect("the other pane");
        let scroll = ws
            .tab_scroll
            .borrow()
            .get(&other)
            .expect("its row")
            .0
            .clone();
        (other, scroll.bounds_for_item(0).expect("its tab, laid out"))
    });
    let on = tab.center();
    assert!(
        f32::from(on.y) < crate::ui_tokens::size::TOP_BAND,
        "{n}: the tab ({on:?}) is under the title's box"
    );
    cx.simulate_click(window, on, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(
        cx.read(|cx| ws.read(cx).model.focused_pane()),
        Some(other),
        "{n}: a click on a top tab reaches it"
    );
    draw(&mut cx, window);
    assert_fresh(&mut cx, window, &format!("{n}: the other pane focused"));
}

/// Panics (`what`) unless the window as drawn now is what a full redraw draws: no part stale.
/// On a failure both are kept as `stale-{now,fresh}` (in `target/ui-shots/failed/<scene>/`).
fn assert_fresh(cx: &mut VisualTestAppContext, window: AnyWindowHandle, what: &str) {
    let now = frame_now(cx, window);
    let fresh = frame_fresh(cx, window);
    let differ = bytes_differ(&now, &fresh);
    if differ > 0 {
        keep_shot("stale-now", &now);
        keep_shot("stale-fresh", &fresh);
    }
    assert_eq!(
        differ, 0,
        "{what}: {differ} bytes differ from a full redraw (a part is stale)"
    );
}

/// M11p Task 4, Review Focus 5: a pane split, closed or moved: one tab-bar part per pane on
/// screen, keyed by (pane, whether its bar drags the window); a gone pane's part is dropped,
/// and a pane that comes to the top edge gets a part that drags the window.
///
/// Panics unless after each step the parts are exactly the screen's keys, and the frame is a
/// full redraw's.
fn chrome_pane_churn() {
    use std::collections::HashSet;

    let n = "chrome-pane-churn";
    let (mut cx, window, ws) = chrome_window(|ws, window, cx| {
        let api = std::path::Path::new("/Users/me/code/api");
        let web = std::path::Path::new("/Users/me/code/web");
        // A workspace not shown yet whose one pane holds more tabs than its bar shows, the
        // last one shown.
        ws.open_shot_session("many", web, SHELL_TEXT, None, window, cx);
        for i in 1..16 {
            ws.open_shot_tab(&format!("tab {i}"), web, SHELL_TEXT, window, cx);
        }
        ws.open_shot_session("a", api, SHELL_TEXT, None, window, cx);
    });
    let check = |cx: &mut VisualTestAppContext, step: &str, panes: usize| {
        cx.run_until_parked();
        draw(cx, window);
        let (parts, live) = cx
            .update_window(window, |_, window, cx| {
                let ws = ws.read(cx);
                let parts: HashSet<_> = ws.chrome.tab_bars.keys().copied().collect();
                let live: HashSet<_> = ws.tab_bar_keys(window).into_iter().collect();
                (parts, live)
            })
            .expect("read the parts");
        println!("ui-shots: {n}: {step}: parts {parts:?}");
        assert_eq!(live.len(), panes, "{n}: {step}: panes on screen");
        assert_eq!(parts, live, "{n}: {step}: one part per pane on screen");
        assert_fresh(cx, window, &format!("{n}: {step}"));
    };
    let act = |cx: &mut VisualTestAppContext,
               f: &dyn Fn(&mut Workspace, &mut Window, &mut Context<Workspace>)| {
        cx.update_window(window, |_, window, cx| {
            ws.update(cx, |ws, cx| f(ws, window, cx))
        })
        .expect("act on the workspace");
    };
    let pane_of = |cx: &mut VisualTestAppContext| cx.read(|cx| ws.read(cx).model.focused_pane());
    check(&mut cx, "one pane", 1);
    let a = pane_of(&mut cx).expect("a's pane");
    // [a | b]
    // (A silent `cat`, as `split` would open a shell, whose prompt would come in meanwhile.)
    let api = std::path::Path::new("/Users/me/code/api");
    act(&mut cx, &move |ws, w, cx| {
        ws.open_shot_session("b", api, SHELL_TEXT, Some(Axis::Row), w, cx)
    });
    check(&mut cx, "split right", 2);
    let b = pane_of(&mut cx).expect("b's pane");
    // [a / c | b]: c under a, not at the top edge.
    act(&mut cx, &move |ws, w, cx| ws.select_pane(a, w, cx));
    act(&mut cx, &move |ws, w, cx| {
        ws.open_shot_session("c", api, SHELL_TEXT, Some(Axis::Column), w, cx)
    });
    check(&mut cx, "split a down", 3);
    let c = pane_of(&mut cx).expect("c's pane");
    let top = |cx: &mut VisualTestAppContext, pane| {
        cx.update_window(window, |_, window, cx| {
            ws.read(cx)
                .tab_bar_keys(window)
                .into_iter()
                .find(|(p, _)| *p == pane)
                .map(|(_, top)| top)
        })
        .expect("read the keys")
    };
    assert_eq!(top(&mut cx, c), Some(false), "{n}: c is under a");
    // a's tab moved into b: a goes, and c comes up to the top edge.
    act(&mut cx, &move |ws, w, cx| {
        let tab = ws.model.pane(a).expect("a").tabs[0].id;
        ws.move_tab(tab, b, 0, w, cx);
    });
    check(&mut cx, "a's tab moved into b", 2);
    assert_eq!(top(&mut cx, c), Some(true), "{n}: c is at the top now");
    // b closed (its two tabs).
    act(&mut cx, &move |ws, w, cx| {
        for tab in ws
            .model
            .pane(b)
            .expect("b")
            .tabs
            .iter()
            .map(|t| t.id)
            .collect::<Vec<_>>()
        {
            ws.close_tab(tab, w, cx);
        }
    });
    check(&mut cx, "b closed", 1);

    // The other workspace, shown for the first time: its new bar, not laid out on its first
    // frame, scrolls to its shown tab (the last) on the frame after, which it asks for itself.
    act(&mut cx, &|ws, w, cx| {
        let many = ws.workspace_of(0).expect("many's workspace");
        ws.select_workspace(many, w, cx);
    });
    cx.run_until_parked();
    while next_frame(&mut cx, window) {
        cx.run_until_parked();
    }
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
        // Where it's drawn: its place in the row moved by the row's scroll.
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
    assert_fresh(
        &mut cx,
        window,
        &format!("{n}: a workspace shown the first time"),
    );
}

/// M11p Task 4: `FUXX_CHROME_CACHE=off` draws every part inline, as before M11p: a breath
/// renders each part once per frame (and no part is made). This scene runs the check in a
/// child process with the variable set (it is read once per process); in this process (the
/// cache on) the same breaths render no part.
///
/// Panics unless both hold.
fn chrome_kill_switch() {
    let n = "chrome-kill-switch";
    let (mut cx, window, ws) = chrome_split_window();
    let (frames, parts) = breath_counts(&mut cx, window, &ws, 10);
    let made = cx.read(|cx| {
        let c = &ws.read(cx).chrome;
        [
            c.sidebar.is_some(),
            c.status_bar.is_some(),
            c.title.is_some(),
        ]
    });
    if chrome_cache::enabled() {
        println!("ui-shots: {n}: cache on: workspace {frames} frames; parts {parts:?}");
        assert_eq!(made, [true; 3], "{n}: the parts are made");
        for (c, count) in parts {
            assert_eq!(count, 0, "{n}: cache on: {c:?} rendered {count} times");
        }
        let spawned = Instant::now();
        let child = child_run(n)
            .env("FUXX_CHROME_CACHE", "off")
            .status()
            .expect("run the scene with the cache off");
        prof::add(prof::CHILD, spawned.elapsed());
        assert!(child.success(), "{n}: with FUXX_CHROME_CACHE=off: {child}");
    } else {
        println!("ui-shots: {n}: cache off: workspace {frames} frames; parts {parts:?}");
        assert_eq!(made, [false; 3], "{n}: no part is made with the cache off");
        assert_eq!(frames, 10 * FRAMES_PER_TICK, "{n}: the breaths drew");
        for (c, count) in parts {
            let bars = if c == Counted::TabBar { 2 } else { 1 };
            assert_eq!(
                count,
                bars * frames,
                "{n}: cache off: {c:?} rendered {count} times in {frames} frames"
            );
        }
    }
}

/// The chrome drawn uncached while this lives (`chrome_cache::shots_off`); back as the
/// environment says when it drops, a failed check's unwind too.
struct CacheOff;

impl CacheOff {
    fn on() -> Self {
        chrome_cache::shots_off::set(true);
        CacheOff
    }
}

impl Drop for CacheOff {
    fn drop(&mut self) {
        chrome_cache::shots_off::set(false);
    }
}

/// M11p Task 4: a busy window (six workspaces in two groups, a split, waiting, working and done
/// terminals, notifications, an unseen one) looks the same drawn from the cached parts as drawn
/// with the cache off, and drawn afresh; with the sidebar shown and hidden (the title area).
///
/// Panics unless the frames are identical byte for byte, and the cached frame was drawn with
/// no part rendering (so it was the parts' reused frame).
fn chrome_pixels_same() {
    const BUSY: [ShotSession; 7] = [
        shot("claude", API, Status::Waiting, 4 * 60 + 12, CLAUDE_PROMPT),
        shot("codex", API, Status::Working, 3 * 60, AGENT_TEXT),
        shot("tests", API, Status::Done, 12 * 60, SHELL_TEXT),
        shot("shell", API, Status::Idle, 0, SHELL_TEXT),
        shot("agent", WEB, Status::Waiting, 5 * 60, SHELL_TEXT),
        shot("server", WEB, Status::Idle, 0, SHELL_TEXT),
        shot("build", WEB, Status::Done, 40 * 60, SHELL_TEXT),
    ];
    let n = "chrome-pixels-same";
    let (mut cx, window, ws) = chrome_window_with(|window, cx| {
        // codex's workspace shown: its Working mark in its tab and the status bar.
        let mut ws = sidebar_workspace(&BUSY, Some(3), 1, &[], window, cx);
        ws.sidebar.hint_dismissed = true;
        shot_notifications(&mut ws);
        ws
    });
    // A Working mark mid-beat (M11p Task 6: it is its own view, turned by the clock): its arc
    // has turned 144°, as the cached frame, the uncached one and a full redraw all draw it.
    let clock = cx.read(|cx| ws.read(cx).clock.clone());
    cx.update(|cx| {
        clock.update(cx, |clock, cx| {
            clock.phase = 0.1;
            cx.notify();
        })
    });
    cx.run_until_parked();
    draw(&mut cx, window);
    let moving = cx.read(|cx| {
        let ws = ws.read(cx);
        let marks = ws.marks.borrow();
        (ws.marks_on_screen().into_iter())
            .filter(|(key, _)| marks.get(key).is_some_and(|v| v.read(cx).moves()))
            .map(|(key, _)| key)
            .collect::<Vec<_>>()
    });
    println!("ui-shots: {n}: moving marks {moving:?}");
    assert!(
        ["tab", "status-bar"]
            .iter()
            .all(|place| moving.iter().any(|(p, _)| p == place)),
        "{n}: codex's Working mark is a view in its tab and the status bar"
    );
    for hidden in [false, true] {
        let step = if hidden {
            "sidebar hidden"
        } else {
            "sidebar shown"
        };
        if hidden {
            cx.update(|cx| {
                ws.update(cx, |ws, cx| {
                    ws.sidebar.hidden = true;
                    cx.notify();
                })
            });
            cx.run_until_parked();
            draw(&mut cx, window);
        }
        let phase = |cx: &mut VisualTestAppContext| cx.read(|cx| ws.read(cx).phase(cx));
        let at = phase(&mut cx);
        // A breath: the parts' reused frame.
        let view = focused_view(&mut cx, &ws);
        chrome_cache::reset();
        cx.update(|cx| view.update(cx, |_, cx| cx.notify()));
        let cached = frame_now(&mut cx, window);
        let parts = PARTS.map(|c| (c, chrome_cache::get(c)));
        println!("ui-shots: {n}: {step}: parts rendered for the cached frame {parts:?}");
        for (c, count) in parts {
            assert_eq!(count, 0, "{n}: {step}: {c:?} rendered for the cached frame");
        }
        let off = {
            let _off = CacheOff::on();
            frame_fresh(&mut cx, window)
        };
        let fresh = frame_fresh(&mut cx, window);
        assert_eq!(
            phase(&mut cx),
            at,
            "{n}: {step}: the marks didn't move meanwhile"
        );
        let slug = if hidden { "hidden" } else { "shown" };
        keep_shot(&format!("{n}-{slug}-mocha"), &cached);
        keep_shot(&format!("{n}-{slug}-off-mocha"), &off);
        assert_eq!(
            bytes_differ(&cached, &off),
            0,
            "{n}: {step}: cached vs FUXX_CHROME_CACHE=off"
        );
        assert_eq!(
            bytes_differ(&cached, &fresh),
            0,
            "{n}: {step}: cached vs drawn afresh"
        );
    }
}

/// How many times `sidebar_rows` runs per workspace render on a breath tick: none since Task 3
/// (the rows are kept until the workspace notifies or an age turns; it was 3 before).
const SIDEBAR_ROWS_PER_FRAME: u64 = 0;

/// Frames a breath-like tick draws in `draws_of`: the platform's own draw when the notify's
/// effects flush, plus the explicit one.
const FRAMES_PER_TICK: u64 = 2;

/// An off-screen 900 x 500 window over `Workspace::for_shots` as `build` sets it up, drawn
/// twice and settled: the context, the window and the workspace (the chrome scenes of M11p).
pub(super) fn chrome_window(
    build: impl FnOnce(&mut Workspace, &mut Window, &mut Context<Workspace>) + 'static,
) -> (ShotApp, AnyWindowHandle, gpui::Entity<Workspace>) {
    chrome_window_with(|window, cx| {
        let mut ws = Workspace::for_shots(window, cx);
        build(&mut ws, window, cx);
        ws
    })
}

/// `chrome_window` over the workspace `make` makes.
fn chrome_window_with(
    make: impl FnOnce(&mut Window, &mut Context<Workspace>) -> Workspace + 'static,
) -> (ShotApp, AnyWindowHandle, gpui::Entity<Workspace>) {
    use std::cell::RefCell;
    use std::rc::Rc;

    let theme = shot_theme("Catppuccin Mocha");
    let dims = size(px(900.0), px(500.0));
    let mut cx = ShotApp::new();
    let slot = Rc::new(RefCell::new(None));
    let window = {
        let slot = slot.clone();
        cx.open_offscreen_window(dims, move |window, cx| {
            cx.set_global(shot_appearance(theme, cx));
            let ws = cx.new(|cx| make(window, cx));
            *slot.borrow_mut() = Some(ws.clone());
            cx.new(|_| Root {
                theme,
                scene: ws.into(),
            })
        })
        .expect("open an off-screen window")
    };
    cx.run_until_parked();
    draw(&mut cx, window.into());
    draw(&mut cx, window.into());
    cx.run_until_parked();
    let ws = slot.borrow().clone().expect("the workspace");
    (cx, window.into(), ws)
}

/// The focused terminal's view (what a breath tick notifies).
fn focused_view(
    cx: &mut VisualTestAppContext,
    ws: &gpui::Entity<Workspace>,
) -> gpui::Entity<crate::terminal_view::TerminalView> {
    cx.read(|cx| {
        let ws = ws.read(cx);
        let id = ws.focused_terminal().expect("a focused terminal");
        let i = ws.index_of(id).expect("its session");
        ws.sessions.items()[i].view.clone()
    })
}

/// M11p Task 2, Review Focus 1: a plain `cd` with no OSC 7 (a shell that changes folder and
/// says nothing; only polling finds it) reaches the sidebar row and the status bar on its own:
/// the 1 s folder check runs on its timer and redraws, with nothing else redrawing (no breath,
/// no terminal output, no other notify). The 5 s wait for a folder's next lookup is skipped by
/// dating the last lookup 5 s back (a fixture: the time passing with nothing drawn).
///
/// Panics unless an idle second draws nothing (no other redraw source), and the second after
/// the `cd` draws on its own with the new folder on the row and the bar.
fn chrome_folder_polled() {
    use std::path::{Path, PathBuf};
    use std::time::{Duration, Instant};

    use crate::git_branch::{CHECK, REFRESH, process_cwd};
    use crate::sidebar::SidebarRow;

    let n = "chrome-folder-polled";
    let dir = std::env::temp_dir().join(format!("fuxx-{n}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    for sub in ["a", "b"] {
        std::fs::create_dir_all(dir.join(sub)).expect("the fixture's folders");
    }
    let dir = dir.canonicalize().expect("the fixture's folder");
    let (a, b) = (dir.join("a"), dir.join("b"));
    // The shell starts in a, waits for `go`, then `cd`s to b and becomes cat (same pid): no
    // output, no OSC 7.
    let script = r#"while [ ! -e "$0/go" ]; do sleep 0.05; done; cd "$0/b" && exec /bin/cat"#;
    let program = vec![
        "/bin/sh".to_string(),
        "-c".to_string(),
        script.to_string(),
        dir.display().to_string(),
    ];
    let start = a.clone();
    let (mut cx, window, ws) = chrome_window(move |ws, window, cx| {
        // Looked up for real, this once.
        ws.places.frozen = false;
        let at = crate::workspace::At::New;
        ws.open_shot_program(
            Some("shell"),
            None,
            program,
            Some(&start),
            "",
            at,
            window,
            cx,
        );
    });
    let id = cx.read(|cx| ws.read(cx).focused_terminal().expect("the shell"));
    // What the row and the bar show (as the workspace would draw them now).
    let shown = |cx: &mut VisualTestAppContext| -> (Option<String>, Option<PathBuf>) {
        cx.read(|cx| {
            let ws = ws.read(cx);
            let row = ws
                .rows_in_view(Instant::now())
                .into_iter()
                .find_map(|r| match r {
                    SidebarRow::Workspace(w) => Some(w.folder),
                    SidebarRow::Header(_) => None,
                })
                .flatten();
            let bar = ws.status_bar_facts().and_then(|f| f.folder);
            (row, bar)
        })
    };
    let as_row = |p: &Path| Some(p.display().to_string());
    assert_eq!(
        shown(&mut cx),
        (as_row(&a), Some(a.clone())),
        "{n}: the shell's first folder, looked up"
    );

    // An idle second: the check runs and finds nothing new; nothing draws.
    chrome_cache::reset();
    cx.advance_clock(CHECK);
    cx.run_until_parked();
    assert_eq!(
        chrome_cache::get(Counted::Workspace),
        0,
        "{n}: an idle second draws nothing (no other redraw source)"
    );

    // The cd.
    std::fs::write(dir.join("go"), "").expect("go");
    let pid = cx
        .read(|cx| {
            ws.read(cx).sessions.items()[0]
                .view
                .read(cx)
                .working_folder()
                .1
        })
        .expect("the shell's pid");
    let moved = Instant::now();
    while process_cwd(pid).as_deref() != Some(b.as_path()) {
        assert!(
            moved.elapsed() < Duration::from_secs(5),
            "{n}: the shell moved to b"
        );
        prof::sleep(Duration::from_millis(20));
    }
    // Its last lookup 5 s back: due at the next check (what 5 s with nothing drawn would do).
    cx.update(|cx| {
        ws.update(cx, |ws, _| {
            let last = Instant::now() - REFRESH;
            ws.places.set_folder(id, Some(a.clone()), last);
        })
    });
    chrome_cache::reset();
    cx.advance_clock(CHECK);
    cx.run_until_parked();
    let drawn = [Counted::Workspace, Counted::Sidebar, Counted::StatusBar]
        .map(|c| (c, chrome_cache::get(c)));
    println!("ui-shots: {n}: drawn on their own after the cd: {drawn:?}");
    for (c, count) in drawn {
        assert!(
            count >= 1,
            "{n}: {c:?} drew on its own after the cd (the check's redraw)"
        );
    }
    assert_eq!(
        shown(&mut cx),
        (as_row(&b), Some(b.clone())),
        "{n}: the row and the bar show the new folder"
    );
    // And an explicit draw changes nothing.
    draw(&mut cx, window);
    cx.run_until_parked();
    assert_eq!(
        shown(&mut cx),
        (as_row(&b), Some(b.clone())),
        "{n}: still b after a draw"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// M11p Task 2: the terminal the status bar follows changes outside drawing. Focusing another
/// workspace (its row, ⌘N: `select_workspace`) follows its terminal before the frame is drawn;
/// a focus change by any other route (the model moved, the workspace notified) is followed
/// after the next frame (`Workspace::render`'s deferred check), which redraws.
///
/// Panics unless the bar describes api's terminal before the first frame after its row is
/// chosen, and, by the other route, describes web's after a second frame drawn on its own.
fn chrome_followed() {
    let n = "chrome-followed";
    let (mut cx, window, ws) = chrome_window(|ws, window, cx| {
        let api = std::path::Path::new("/Users/me/code/api");
        let web = std::path::Path::new("/Users/me/code/web");
        ws.open_shot_session("api", api, SHELL_TEXT, None, window, cx);
        ws.open_shot_session("web", web, SHELL_TEXT, None, window, cx);
    });
    let (api, web) = cx.read(|cx| {
        let ws = ws.read(cx);
        let id = |i: usize| ws.sessions.items()[i].id;
        (id(0), id(1))
    });
    let followed = |cx: &mut VisualTestAppContext| cx.read(|cx| ws.read(cx).right_sidebar.followed);
    assert_eq!(
        followed(&mut cx),
        Some(web),
        "{n}: web (opened last) followed"
    );

    // Its row: followed as the focus moves, before anything is drawn.
    let before_draw = cx
        .update_window(window, |_, window, cx| {
            ws.update(cx, |ws, cx| {
                let shown = ws.workspace_of(0).expect("api's workspace");
                ws.select_workspace(shown, window, cx);
                ws.right_sidebar.followed
            })
        })
        .expect("choose api's row");
    assert_eq!(
        before_draw,
        Some(api),
        "{n}: api followed as its row is chosen, before the frame"
    );
    cx.run_until_parked();
    draw(&mut cx, window);
    assert_eq!(followed(&mut cx), Some(api), "{n}: and after the frame");

    // Any other route: the frame that shows the change follows it after drawing, and redraws.
    chrome_cache::reset();
    cx.update(|cx| {
        ws.update(cx, |ws, cx| {
            let shown = ws.workspace_of(1).expect("web's workspace");
            ws.model.focus_workspace(shown);
            ws.sync();
            cx.notify();
        })
    });
    cx.run_until_parked();
    let bars = chrome_cache::get(Counted::StatusBar);
    println!("ui-shots: {n}: status bar frames after a bare focus change: {bars}");
    assert_eq!(followed(&mut cx), Some(web), "{n}: web followed");
    assert_eq!(
        bars, 2,
        "{n}: the frame that showed the change, then the one the deferred follow drew"
    );
}

/// M11p Task 2 (Goal 6): the window title is set only when it changes, not on every frame.
///
/// Panics unless 10 breath ticks (the focused terminal notifies, then a draw) set it at most
/// once, and choosing another workspace's row sets it exactly once more.
fn chrome_title_once() {
    let n = "chrome-title-once";
    let (mut cx, window, ws) = chrome_window(|ws, window, cx| {
        let api = std::path::Path::new("/Users/me/code/api");
        ws.open_shot_session("api", api, SHELL_TEXT, None, window, cx);
        ws.open_shot_session("web", api, SHELL_TEXT, None, window, cx);
    });
    let view = focused_view(&mut cx, &ws);
    chrome_cache::reset();
    for _ in 0..10 {
        cx.update(|cx| view.update(cx, |_, cx| cx.notify()));
        draw(&mut cx, window);
    }
    let breaths = chrome_cache::get(Counted::WindowTitle);
    let frames = chrome_cache::get(Counted::Workspace);
    println!("ui-shots: {n}: title set {breaths} times over {frames} breath frames");
    assert!(frames >= 10, "{n}: the breaths drew ({frames} frames)");
    assert!(
        breaths <= 1,
        "{n}: the title set {breaths} times in 10 breaths"
    );

    chrome_cache::reset();
    cx.update_window(window, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            let other = ws.workspace_of(0).expect("api's workspace");
            ws.select_workspace(other, window, cx);
        })
    })
    .expect("choose api's row");
    cx.run_until_parked();
    draw(&mut cx, window);
    draw(&mut cx, window);
    assert_eq!(
        chrome_cache::get(Counted::WindowTitle),
        1,
        "{n}: another workspace's title, set once"
    );
}

/// M11p Task 3: the sidebar's rows are built once per change, not several times per frame.
/// Ten explicit draws with nothing changed (no notify) build them at most once in all. A
/// change the workspace notifies (a workspace renamed) is built once and drawn; an age label
/// turning ("59s" to "1m") with nothing notified is built again by the next frame, so the
/// labels stay true.
///
/// Panics unless all of that holds.
fn chrome_rows_once() {
    use std::time::{Duration, Instant};

    use crate::sidebar::SidebarRow;

    let n = "chrome-rows-once";
    let (mut cx, window, ws) = chrome_window(|ws, window, cx| {
        let api = std::path::Path::new("/Users/me/code/api");
        ws.open_shot_session("api", api, SHELL_TEXT, None, window, cx);
        ws.open_shot_session("web", api, SHELL_TEXT, None, window, cx);
    });
    // The rows as the workspace has them now (its cache: no build when nothing changed).
    let rows =
        |cx: &mut VisualTestAppContext| cx.read(|cx| ws.read(cx).rows_in_view(Instant::now()));

    chrome_cache::reset();
    for _ in 0..10 {
        draw(&mut cx, window);
    }
    let built = chrome_cache::get(Counted::SidebarRows);
    let frames = chrome_cache::get(Counted::Workspace);
    println!("ui-shots: {n}: rows built {built} times over {frames} frames with no change");
    assert!(frames >= 10, "{n}: the draws drew ({frames} frames)");
    assert!(
        built <= 1,
        "{n}: rows built {built} times in 10 unchanged frames"
    );

    // A rename the workspace notifies: built once, and the row says it.
    let api = cx.read(|cx| ws.read(cx).workspace_of(0).expect("api's workspace"));
    chrome_cache::reset();
    cx.update(|cx| {
        ws.update(cx, |ws, cx| {
            ws.model.set_name(api, Some("renamed".into()));
            cx.notify();
        })
    });
    cx.run_until_parked();
    draw(&mut cx, window);
    let built = chrome_cache::get(Counted::SidebarRows);
    println!("ui-shots: {n}: rows built {built} times for a rename");
    assert_eq!(built, 1, "{n}: a rename builds the rows once");
    let named = rows(&mut cx).into_iter().any(
        |row| matches!(row, SidebarRow::Workspace(w) if w.workspace == api && w.label == "renamed"),
    );
    assert!(named, "{n}: the renamed row shows its new name");

    // An age about to turn: api working for 59.7 s ("59s"), told by a notify.
    cx.update(|cx| {
        ws.update(cx, |ws, cx| {
            let s = ws.sessions.get_mut(0).expect("api's session");
            s.status = Status::Working;
            s.status_since = Instant::now() - Duration::from_millis(59_700);
            cx.notify();
        })
    });
    cx.run_until_parked();
    draw(&mut cx, window);
    let age = |cx: &mut VisualTestAppContext| {
        rows(cx).into_iter().find_map(|row| match row {
            SidebarRow::Workspace(w) if w.workspace == api => w.age,
            _ => None,
        })
    };
    assert_eq!(
        age(&mut cx).as_deref(),
        Some("59s"),
        "{n}: api's age before it turns"
    );
    // Past the minute: the age's own redraw (its timer, `refresh_ages`; or, as here, the
    // moving marks' ticker) builds the rows again ("1m") and the sidebar draws them (M11p
    // Task 4: a frame with no notify reuses the cached sidebar, so the redraw is what turns it).
    // (The labels read the real clock, the timers the harness's.)
    prof::sleep(Duration::from_millis(500));
    chrome_cache::reset();
    cx.advance_clock(Duration::from_millis(500));
    cx.run_until_parked();
    draw(&mut cx, window);
    let built = chrome_cache::get(Counted::SidebarRows);
    let drawn = chrome_cache::get(Counted::Sidebar);
    println!("ui-shots: {n}: rows built {built} times as an age turned, the sidebar drawn {drawn}");
    assert!(built >= 1, "{n}: an age turning builds the rows");
    assert!(drawn >= 1, "{n}: the sidebar is drawn as an age turns");
    assert_fresh(&mut cx, window, &format!("{n}: the turned age"));
    assert_eq!(age(&mut cx).as_deref(), Some("1m"), "{n}: api's age turned");
}
