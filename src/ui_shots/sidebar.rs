//! The left sidebar: its marks, rows, selection, button and the full sidebar.

use super::panes::settle_slides;
use super::*;

/// This file, as `file!()` gives it (repo-relative under cargo): `registry::AREAS` pairs it with
/// `SCENES`, so `--changed` runs these scenes when this file changes.
pub(super) const FILE: &str = file!();

/// This area's scenes (`registry.rs` runs them in its `ORDER`).
pub(super) const SCENES: &[Scene] = &[
    Scene {
        name: "sidebar-marks",
        run: sidebar_marks,
        themes: Themes::One,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/sidebar.rs",
            "src/status_mark.rs",
            "src/mark_view.rs",
            "src/procs.rs",
        ],
    },
    Scene {
        name: "sidebar",
        run: sidebar_scenes,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/sidebar.rs",
            "src/drag_gap.rs",
            "src/groups.rs",
            "src/workspace.rs",
            // `providers::all()`: the + menu's badges (sidebar-menu).
            "src/providers/mod.rs",
        ],
    },
    Scene {
        name: "sidebar-quiet",
        run: sidebar_quiet_scenes,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/sidebar.rs",
            "src/status_mark.rs",
            "src/groups.rs",
            "src/workspace.rs",
        ],
    },
    Scene {
        name: "sidebar-rows",
        run: sidebar_row_scenes,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &["src/sidebar.rs", "src/groups.rs", "src/git_branch.rs"],
    },
    Scene {
        name: "sidebar-selection",
        run: sidebar_selection_scenes,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &["src/sidebar.rs", "src/status_mark.rs"],
    },
    Scene {
        name: "sidebar-button",
        run: sidebar_button_scenes,
        themes: Themes::Both,
        // Serial: (c) a shot mid-slide, the slide set 60 ms back by the real clock.
        serial: true,
        kind: Kind::Visual,
        sources: &[
            "src/sidebar.rs",
            "src/sidebar_toggle.rs",
            "src/title_area.rs",
        ],
    },
];

/// The real sidebar with the new marks: sessions in every status across two groups.
fn sidebar_marks() {
    let dims = size(px(480.0), px(320.0));
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let build = move |window: &mut Window, cx: &mut App| -> AnyView {
            cx.set_global(shot_appearance(theme, cx));
            cx.new(|cx| {
                let mut ws = Workspace::for_shots(window, cx);
                let api = std::path::Path::new("/Users/me/code/api");
                let web = std::path::Path::new("/Users/me/code/web");
                let sessions = [
                    ("claude", api, Status::Working),
                    ("codex", api, Status::Waiting),
                    ("tests", api, Status::Done),
                    ("shell", api, Status::Idle),
                    ("agent", web, Status::Working),
                    ("server", web, Status::Idle),
                ];
                for (i, (name, project, status)) in sessions.into_iter().enumerate() {
                    ws.open_shot_session(name, project, SHELL_TEXT, None, window, cx);
                    ws.sessions.get_mut(i).expect("just opened").status = status;
                }
                // Idle (M11i): server runs a program (a filled dot), shell is at its prompt (a
                // ring).
                let id = |i: usize| ws.sessions.items()[i].id;
                let (shell, server) = (id(3), id(5));
                ws.programs
                    .set_for_shots(server, ProgramState::Running("npm run dev".into()));
                ws.programs.set_for_shots(shell, ProgramState::Prompt);
                // The hint closed, so every row (and both idle marks) shows.
                ws.sidebar.hint_dismissed = true;
                ws
            })
            .into()
        };
        let path = shoot("sidebar-marks", slug, &theme, dims, build);
        println!("ui-shots: {}", path.display());
    }
}

/// The question the sidebar must find in `CLAUDE_PROMPT`.
pub(super) const CLAUDE_QUESTION: &str = "Do you want to edit auth.rs?";

/// The sidebar scene's eight sessions in two groups: in api a waiting one (with a boxed
/// question), a working one with a script's progress, a done one and an idle one (split
/// together, the idle one active), and one with a long title; web closed, holding a waiting,
/// an idle and a done one.
pub(super) const EIGHT: [ShotSession; 8] = [
    shot("claude", API, Status::Waiting, 4 * 60 + 12, CLAUDE_PROMPT),
    shot("codex", API, Status::Working, 32, AGENT_TEXT),
    shot("tests", API, Status::Done, 12 * 60, SHELL_TEXT),
    shot("shell", API, Status::Idle, 0, SHELL_TEXT),
    shot(
        "✳ Refactor the auth middleware to use the new session store",
        API,
        Status::Working,
        2 * 3600 + 300,
        AGENT_TEXT,
    ),
    shot("agent", WEB, Status::Waiting, 95, SHELL_TEXT),
    shot("server", WEB, Status::Idle, 0, SHELL_TEXT),
    shot("build", WEB, Status::Done, 40, SHELL_TEXT),
];

/// codex (session 2) as a script would mark it: a progress bar with a label, and a note.
pub(super) fn with_progress(ws: &mut Workspace) {
    let codex = ws.sessions.get_mut(1).expect("codex");
    codex.progress = Some((60, Some("migrate".into())));
    codex.note = Some("3 of 5 tables".into());
}

/// The window position of sidebar row `row`'s centre, `dy` below it.
pub(super) fn row_point(
    cx: &mut VisualTestAppContext,
    ws: &gpui::Entity<Workspace>,
    row: usize,
    dy: f32,
) -> gpui::Point<Pixels> {
    cx.read(|cx| {
        let scroll = &ws.read(cx).sidebar.scroll;
        let b = scroll.bounds_for_item(row).expect("the row was laid out");
        let c = b.center();
        point(c.x, c.y + scroll.offset().y + px(dy))
    })
}

/// The sidebar, from real workspaces in both themes:
/// - `sidebar`: eight sessions in two groups (waiting with a boxed question, working with a
///   script's progress, done and idle split together in one workspace (M11b: one row, its two
///   panes on screen with their tab bars), a long title, a closed group) and the first-run hint;
///   `sidebar-narrow` and `sidebar-wide` at 180 and 400 px.
/// - `sidebar-hover`: the pointer over the closed group's header (its "+" shows).
/// - `sidebar-menu`: the footer's + menu open.
/// - `sidebar-drag`: codex's row dragged down its group, its copy right over the gap it opens
///   between shell and the long one, once the rows have slid (M11b Task 7b): tests and shell slid
///   up by codex's height (and the gap), the copy (as tall as codex's row) filling the gap, claude
///   and the rows below still, no insertion line.
/// - `sidebar-scroll`: 24 sessions, the last one active, so the list scrolls to it.
///
/// Panics unless the question was read off the boxed prompt, a waiting row and a plain one are
/// both 32 pt (M11i: two lines), the active row is scrolled into view and
/// rows scrolled away don't count as on screen, and the dragged row's run slid as described.
fn sidebar_scenes() {
    use gpui::{Modifiers, MouseButton};
    use std::cell::RefCell;
    use std::rc::Rc;

    let dims = size(px(960.0), px(600.0));
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        for (scene, width, dims) in [
            ("sidebar", 240.0, dims),
            ("sidebar-narrow", 180.0, size(px(720.0), px(600.0))),
            ("sidebar-wide", 400.0, dims),
        ] {
            let slot: Slot = Rc::new(RefCell::new(None));
            let keep = slot.clone();
            let build = move |window: &mut Window, cx: &mut App| -> AnyView {
                cx.set_global(sidebar_appearance(theme, cx));
                let ws = cx.new(|cx| {
                    let mut ws = sidebar_workspace(&EIGHT, Some(3), 3, &[WEB], window, cx);
                    ws.sidebar.width = width;
                    with_progress(&mut ws);
                    ws
                });
                *keep.borrow_mut() = Some(ws.clone());
                ws.into()
            };
            let check = move |cx: &mut VisualTestAppContext, _: AnyWindowHandle| {
                let ws = slotted(&slot);
                cx.read(|cx| {
                    let ws = ws.read(cx);
                    let question = ws.sessions.items()[0].question.as_deref();
                    assert_eq!(question, Some(CLAUDE_QUESTION), "{scene}-{slug}: question");
                    // Rows: api header, its four workspaces (tests and shell split in one),
                    // web's header.
                    let height = |i: usize| {
                        let b = ws.sidebar.scroll.bounds_for_item(i).expect("laid out");
                        f32::from(b.size.height)
                    };
                    // Every row has one line under its name (M11r): two lines, 52 pt.
                    assert_eq!(height(1), 52.0, "{scene}-{slug}: waiting row");
                    assert_eq!(height(3), 52.0, "{scene}-{slug}: the split workspace's row");
                    assert_eq!(ws.sidebar.scroll.children_count(), 6, "{scene}-{slug}");
                    // The split workspace is on screen: two panes, each with its tab.
                    assert_eq!(ws.tab_sessions().len(), 2, "{scene}-{slug}: tabs");
                });
            };
            let path = shoot_with(scene, slug, &theme, dims, build, check);
            println!("ui-shots: {}", path.display());
        }

        // The pointer over web's (closed) header; the + menu open; a row mid-drag.
        for scene in [
            "sidebar-hover",
            "sidebar-menu",
            "sidebar-drag",
            "sidebar-drag-out",
            "sidebar-drop",
        ] {
            let slot: Slot = Rc::new(RefCell::new(None));
            let keep = slot.clone();
            let build = move |window: &mut Window, cx: &mut App| -> AnyView {
                cx.set_global(sidebar_appearance(theme, cx));
                let ws = cx.new(|cx| {
                    let mut ws = sidebar_workspace(&EIGHT, None, 3, &[WEB], window, cx);
                    ws.sidebar.hint_dismissed = true;
                    with_progress(&mut ws);
                    ws
                });
                *keep.borrow_mut() = Some(ws.clone());
                ws.into()
            };
            // The drag's rows (claude, codex, tests, shell, the long one) as laid out, for the
            // checks.
            let rows: Rc<RefCell<Vec<gpui::Bounds<Pixels>>>> = Rc::default();
            let seen = rows.clone();
            let acting = slot.clone();
            let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let ws = slotted(&acting);
                let none = Modifiers::none();
                match scene {
                    "sidebar-hover" => {
                        let at = row_point(cx, &ws, 6, 0.0);
                        cx.simulate_mouse_move(window, at, None, none);
                    }
                    "sidebar-menu" => {
                        // The footer's + (an icon now, no "New" label), at its left, over the
                        // status bar (M11i).
                        let bar = ui_tokens::size::STATUS_BAR;
                        let at = point(px(18.0), dims.height - px(bar + 18.0));
                        cx.simulate_click(window, at, none);
                        assert!(cx.read(|cx| ws.read(cx).sidebar.new_menu), "menu opens");
                    }
                    "sidebar-drag" => {
                        // codex (row 2, three lines tall) down past tests and shell: its copy right
                        // over the gap between shell and the long one, as it opens with codex
                        // taken out (the rows after it moved up by its room: its height and the
                        // gap); the gap ends where shell did.
                        let bounds: Vec<_> = (1..=5)
                            .map(|i| {
                                cx.read(|cx| {
                                    let scroll = &ws.read(cx).sidebar.scroll;
                                    let mut b = scroll.bounds_for_item(i).expect("laid out");
                                    b.origin.y += scroll.offset().y;
                                    b
                                })
                            })
                            .collect();
                        let (codex, shell) = (bounds[1], bounds[3]);
                        // Past the long one's middle (as laid out): the gap after it (M11r: codex,
                        // two lines, is taller than the long one, one line in api's group).
                        let centre = shell.center().y + px(DRAG_PAST);
                        // Taken at its middle, 8 px down (GPUI's drag starts past its threshold):
                        // the pointer is 8 px below the copy's centre.
                        let from = codex.center();
                        let to = point(from.x, centre + px(8.0));
                        cx.simulate_mouse_down(window, from, MouseButton::Left, none);
                        let left = MouseButton::Left;
                        cx.simulate_mouse_move(window, from + point(px(0.0), px(8.0)), left, none);
                        cx.simulate_mouse_move(window, to, left, none);
                        // Before the long one (item 5), after claude, tests and shell: slot 3 of
                        // api's group.
                        let at = cx.read(|cx| ws.read(cx).sidebar.drop_at.clone());
                        let at = at.map(|(at, group, slot)| (at, group.is_some(), slot));
                        assert_eq!(
                            at,
                            Some((5, true, 3)),
                            "the gap is between shell and the long one"
                        );
                        settle_slides(cx, window);
                        *seen.borrow_mut() = bounds;
                    }
                    "sidebar-drag-out" => {
                        // codex taken up into the title band: out of every group, at the top;
                        // api's header and claude slide down under the gap (2026-11-10).
                        let (claude, codex) = cx.read(|cx| {
                            let scroll = &ws.read(cx).sidebar.scroll;
                            let at = |i| scroll.bounds_for_item(i).expect("laid out");
                            (at(1), at(2))
                        });
                        let from = codex.center();
                        let left = MouseButton::Left;
                        cx.simulate_mouse_down(window, from, left, none);
                        cx.simulate_mouse_move(window, from + point(px(0.0), px(8.0)), left, none);
                        cx.simulate_mouse_move(window, point(from.x, px(12.0)), left, none);
                        let at = cx.read(|cx| ws.read(cx).sidebar.drop_at.clone());
                        assert_eq!(at, Some((0, None, 0)), "out of every group, first");
                        settle_slides(cx, window);
                        *seen.borrow_mut() = vec![claude, codex];
                    }
                    "sidebar-drop" => {
                        // codex dropped out of every group, then into web's (closed) group, then
                        // back into api's: where it shows and what is recorded (2026-11-10).
                        use crate::groups::GroupKey;
                        use crate::layout_model::Placed;
                        let left = MouseButton::Left;
                        let codex = cx.read(|cx| {
                            let ws = ws.read(cx);
                            let i = ws
                                .sessions
                                .position(|s| s.label() == "codex")
                                .expect("codex");
                            ws.workspace_of(i).expect("its workspace")
                        });
                        let item = |cx: &mut VisualTestAppContext| {
                            cx.read(|cx| {
                                let ws = ws.read(cx);
                                let rows = ws.rows_in_view(std::time::Instant::now());
                                let i = rows
                                    .iter()
                                    .position(|r| matches!(r, crate::sidebar::SidebarRow::Workspace(w) if w.workspace == codex))
                                    .expect("codex's row");
                                ws.sidebar.scroll.bounds_for_item(i).expect("laid out")
                            })
                        };
                        let drop_at = |cx: &mut VisualTestAppContext, y: Pixels| {
                            let from = item(cx).center();
                            cx.simulate_mouse_down(window, from, left, none);
                            cx.simulate_mouse_move(
                                window,
                                from + point(px(0.0), px(8.0)),
                                left,
                                none,
                            );
                            cx.simulate_mouse_move(window, point(from.x, y), left, none);
                            cx.simulate_mouse_up(window, point(from.x, y), left, none);
                            settle_slides(cx, window);
                        };
                        let state = |cx: &mut VisualTestAppContext| {
                            cx.read(|cx| {
                                let ws = ws.read(cx);
                                let placed =
                                    ws.model.workspace(codex).and_then(|w| w.placed.clone());
                                (placed, ws.workspace_group(codex))
                            })
                        };
                        // Into the title band: out of every group.
                        drop_at(cx, px(12.0));
                        assert_eq!(state(cx), (Some(Placed::Ungrouped), None), "out");
                        // Down past web's header (the list's last item): into web's group.
                        let bottom = cx.read(|cx| ws.read(cx).sidebar.scroll.bounds().bottom());
                        drop_at(cx, bottom - px(6.0));
                        let web = GroupKey::Folder(WEB.into());
                        assert_eq!(
                            state(cx),
                            (Some(Placed::Folder(WEB.into())), Some(web)),
                            "into web's group"
                        );
                        // (Web is closed in this scene: open it, so codex's row shows.)
                        let web = GroupKey::Folder(WEB.into());
                        cx.update(|cx| ws.update(cx, |ws, cx| ws.toggle_group(&web, cx)));
                        settle_slides(cx, window);
                        // Back between claude and tests (under api's header): its own group,
                        // nothing recorded.
                        let tests = cx.read(|cx| {
                            let ws = ws.read(cx);
                            let i = ws.sessions.position(|s| s.label() == "tests").expect("tests");
                            let tests = ws.workspace_of(i).expect("its workspace");
                            let rows = ws.rows_in_view(std::time::Instant::now());
                            let i = rows
                                .iter()
                                .position(|r| matches!(r, crate::sidebar::SidebarRow::Workspace(w) if w.workspace == tests))
                                .expect("tests' row");
                            ws.sidebar.scroll.bounds_for_item(i).expect("laid out")
                        });
                        drop_at(cx, tests.top() + px(6.0));
                        let api = GroupKey::Folder(API.into());
                        assert_eq!(state(cx), (None, Some(api)), "back in api's group");
                    }
                    _ => unreachable!("{scene}"),
                }
                // Not kept past its window (the act runs twice for the drag).
                acting.borrow_mut().take();
            };
            let path = shoot_with(scene, slug, &theme, dims, build.clone(), act.clone());
            println!("ui-shots: {}", path.display());
            if scene == "sidebar-drag-out" {
                // claude slid down into codex's place, drawn over it (codex's own card goes to
                // its gap, not left where it was: 2026-11-10).
                let shot = render_with(&theme, dims, build, act);
                let n = format!("{scene}-{slug}");
                let ui = theme.ui();
                let rows = rows.borrow().clone();
                let (claude, codex) = (rows[0], rows[1]);
                let slid = claude.center().y + (codex.top() - claude.top());
                let p = pixel_at(
                    &shot,
                    dims,
                    // Between its avatar and its name (M11r).
                    f32::from(claude.left()) + 42.0,
                    f32::from(slid),
                );
                assert!(
                    near(p, ui.waiting_tint),
                    "{n}: claude slid down, seen: {p:?}"
                );
                continue;
            }
            if scene != "sidebar-drag" {
                continue;
            }
            let shot = render_with(&theme, dims, build, act);
            let n = format!("{scene}-{slug}");
            let ui = theme.ui();
            let is = |p: [u8; 3], c: alacritty_terminal::vte::ansi::Rgb| {
                p.iter()
                    .zip([c.r, c.g, c.b])
                    .all(|(a, b)| a.abs_diff(b) <= 3)
            };
            let rows = rows.borrow().clone();
            let (claude, codex, tests, shell) = (rows[0], rows[1], rows[2], rows[3]);
            let room = f32::from(tests.top() - codex.top());
            let x = f32::from(shell.left()) + 10.0;
            let px_at = |y: Pixels| pixel_at(&shot, dims, x, f32::from(y));
            // shell (selected: the card's selection, in api's card) slid up by codex's room; the
            // gap below it,
            // down to where shell ended, is filled by codex's copy (as tall as codex's row), top
            // to bottom: no empty band, no line.
            let up = px_at(shell.top() - px(room) + px(3.0));
            // (Selected: the card's neutral lift, M11r.)
            assert!(is(up, ui.card_lift), "{n}: shell slid up: {up:?}");
            // The copy where the pointer holds it, over the gap (as far as it reaches into it).
            let centre = shell.center().y + px(DRAG_PAST);
            let gap_top = shell.bottom() - codex.size.height;
            let copy_top = centre - codex.size.height / 2.0;
            for y in [
                gap_top.max(copy_top) + px(3.0),
                centre,
                shell.bottom() - px(3.0),
            ] {
                let p = px_at(y);
                assert!(
                    !is(p, ui.card) && !is(p, ui.card_selected),
                    "{n}: the copy in the gap at {y:?}: {p:?}"
                );
            }
            // claude, above codex, stays (tinted: it's waiting).
            let still = px_at(claude.center().y);
            assert!(is(still, ui.waiting_tint), "{n}: claude stays: {still:?}");
        }

        // More sessions than fit: the last one is active, so the list scrolls to it.
        let statuses = [Status::Working, Status::Idle, Status::Done, Status::Waiting];
        let names = [
            "claude", "codex", "tests", "shell", "server", "agent", "docs", "lint",
        ];
        let many: Vec<ShotSession> = [API, WEB, "/Users/me/code/infra"]
            .iter()
            .flat_map(|project| {
                names.iter().enumerate().map(|(i, name)| {
                    shot(name, project, statuses[i % 4], 60 * i as u64, SHELL_TEXT)
                })
            })
            .collect();
        let many: &'static [ShotSession] = Box::leak(many.into_boxed_slice());
        let slot: Slot = Rc::new(RefCell::new(None));
        let keep = slot.clone();
        let build = move |window: &mut Window, cx: &mut App| -> AnyView {
            cx.set_global(sidebar_appearance(theme, cx));
            let ws = cx.new(|cx| {
                let mut ws = sidebar_workspace(many, None, many.len() - 1, &[], window, cx);
                ws.sidebar.hint_dismissed = true;
                ws
            });
            *keep.borrow_mut() = Some(ws.clone());
            ws.into()
        };
        let check = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
            // A second frame: the first laid the rows out, this one scrolls to the active one.
            cx.update_window(window, |_, window, _| window.refresh())
                .expect("refresh");
            cx.run_until_parked();
            let ws = slotted(&slot);
            cx.read(|cx| {
                let ws = ws.read(cx);
                let offset = f32::from(ws.sidebar.scroll.offset().y);
                assert!(
                    offset < 0.0,
                    "sidebar-scroll-{slug}: scrolled to the active row"
                );
                let rows = ws.sidebar.scroll.children_count();
                let on_screen = ws.visible_statuses().len();
                assert!(
                    on_screen < rows,
                    "sidebar-scroll-{slug}: {on_screen} of {rows} rows count as on screen"
                );
            });
        };
        let tall = size(px(960.0), px(520.0));
        let path = shoot_with("sidebar-scroll", slug, &theme, tall, build, check);
        println!("ui-shots: {}", path.display());
    }
}

/// How far past the long one's middle `sidebar-drag` holds codex's copy.
const DRAG_PAST: f32 = 4.0;

const INFRA: &str = "/Users/me/code/infra";

const PAYMENTS: &str = "/Users/me/code/a-very-long-project-folder/services/payments";

const SCRATCH: &str = "/tmp/scratch";

const LONG_BRANCH: &str = "feature/a-really-long-branch-name-for-the-payments-service";

/// The rows scene's sessions: deploy and claude (waiting) to be pinned; codex in api; payments
/// (to be selected) with a long folder; a long name in web; notes outside home.
const ROWS: [ShotSession; 6] = [
    shot("deploy", INFRA, Status::Idle, 0, SHELL_TEXT),
    shot("claude", API, Status::Waiting, 4 * 60 + 12, CLAUDE_PROMPT),
    shot("codex", API, Status::Working, 32, AGENT_TEXT),
    shot("payments", PAYMENTS, Status::Idle, 0, SHELL_TEXT),
    shot(
        "✳ Refactor the auth middleware to use the new session store",
        WEB,
        Status::Done,
        40,
        SHELL_TEXT,
    ),
    shot("notes", SCRATCH, Status::Idle, 0, SHELL_TEXT),
];

/// The sidebar rows (M11b; compact in M11i), from real workspaces in both themes, at 240 pt
/// (`sidebar-rows`) and 180 pt (`sidebar-rows-narrow`): deploy and claude pinned (📌, above
/// every group header), claude waiting with "Needs input · question" under its name ("Needs
/// input" alone at 180 pt); notes in no group, a gap under the pinned rows; each row in a repo
/// with its branch on one line and its `~/folder` under it (2026-11-10); payments selected (the
/// selection's tint and the accent bar, its text in `text`), its long branch cut at its end, its
/// long folder at its start (keeping `payments`); the long name cut; notes in /tmp (no `~`, not
/// in a repo: no branch line).
///
/// Panics unless the pinned rows lead, the rows carry their branch and folder, the rows in a
/// repo are 68 pt (three lines) and claude (waiting) and notes 52 pt (two), and the selected row
/// is tinted to its left edge (no accent bar since 2026-11-10) with its name in `text`.
fn sidebar_row_scenes() {
    use std::cell::{Cell, RefCell};
    use std::path::Path;
    use std::rc::Rc;
    use std::time::Instant;

    use crate::sidebar::SidebarRow;

    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let ui = theme.ui();
        for (scene, width) in [("sidebar-rows", 240.0), ("sidebar-rows-narrow", 180.0)] {
            let dims = size(px(width + 300.0), px(720.0));
            let slot: Slot = Rc::new(RefCell::new(None));
            let keep = slot.clone();
            let build = move |window: &mut Window, cx: &mut App| -> AnyView {
                cx.set_global(sidebar_appearance(theme, cx));
                let ws = cx.new(|cx| {
                    let mut ws = sidebar_workspace(&ROWS, None, 3, &[], window, cx);
                    ws.sidebar.width = width;
                    ws.sidebar.hint_dismissed = true;
                    for i in [0, 1] {
                        let pinned = ws.workspace_of(i).expect("its workspace");
                        ws.model.set_pinned(pinned, true);
                    }
                    // notes in no group (its folder as found, not where it started): it leads
                    // the unpinned rows, a gap under the pinned ones.
                    let notes = ws.sessions.get_mut(5).expect("notes");
                    notes.project = None;
                    let id = notes.id;
                    let found = Some(SCRATCH.into());
                    ws.places.set_folder(id, found, Instant::now());
                    ws.sync();
                    for (folder, branch) in [
                        (INFRA, "main"),
                        (API, "feature/rows"),
                        (PAYMENTS, LONG_BRANCH),
                        (WEB, "main"),
                    ] {
                        ws.places.set_branch(Path::new(folder), Some(branch.into()));
                    }
                    ws
                });
                *keep.borrow_mut() = Some(ws.clone());
                ws.into()
            };
            // The selected row's bounds, for the pixel checks.
            let selected = Rc::new(Cell::new(None));
            let found = selected.clone();
            let act = move |cx: &mut VisualTestAppContext, _: AnyWindowHandle| {
                let ws = slotted(&slot);
                cx.read(|cx| {
                    let ws = ws.read(cx);
                    let n = format!("{scene}-{slug}");
                    let rows = ws.rows_in_view(Instant::now());
                    let shape: Vec<(String, bool, Option<String>, Option<String>)> = rows
                        .iter()
                        .map(|r| match r {
                            SidebarRow::Header(h) => (format!("[{}]", h.label), false, None, None),
                            SidebarRow::Workspace(w) => (
                                w.label.clone(),
                                w.pinned,
                                w.branch.clone(),
                                w.folder.clone(),
                            ),
                        })
                        .collect();
                    let row = |label: &str, pinned, branch: Option<&str>, folder: Option<&str>| {
                        (
                            label.to_string(),
                            pinned,
                            branch.map(String::from),
                            folder.map(String::from),
                        )
                    };
                    let header = |label: &str| row(label, false, None, None);
                    let long_folder = "~/code/a-very-long-project-folder/services/payments";
                    assert_eq!(
                        shape,
                        [
                            row("deploy", true, Some("main"), Some("~/code/infra")),
                            row("claude", true, Some("feature/rows"), Some("~/code/api")),
                            row("notes", false, None, Some("/tmp/scratch")),
                            header("[api]"),
                            row("codex", false, Some("feature/rows"), Some("~/code/api")),
                            header("[payments]"),
                            row("payments", false, Some(LONG_BRANCH), Some(long_folder)),
                            header("[web]"),
                            row(ROWS[4].name, false, Some("main"), Some("~/code/web")),
                        ],
                        "{n}"
                    );
                    let active = rows
                        .iter()
                        .position(|r| matches!(r, SidebarRow::Workspace(w) if w.active))
                        .expect("a selected row");
                    assert_eq!(active, 6, "{n}: payments is selected");
                    // A group's gap between the pinned rows and notes.
                    let bounds = |i: usize| ws.sidebar.scroll.bounds_for_item(i).expect("laid out");
                    let gap = f32::from(bounds(2).top() - bounds(1).bottom());
                    assert_eq!(gap, 10.0, "{n}: the gap under the pinned rows");
                    // Room under the title band: the first row starts 10 pt into the list.
                    let list_top = f32::from(ws.sidebar.scroll.bounds().top());
                    let first_top = f32::from(bounds(0).top());
                    assert_eq!(first_top - list_top, 10.0, "{n}: the gap under the title");
                    // Every row has one line under its name (M11r): out of a group the branch and
                    // the folder share it; 52 pt each.
                    for (i, h) in [
                        (0, 52.0),
                        (1, 52.0),
                        (2, 52.0),
                        (4, 52.0),
                        (6, 52.0),
                        (8, 52.0),
                    ] {
                        let seen = f32::from(bounds(i).size.height);
                        assert_eq!(seen, h, "{n}: row {i}'s height");
                    }
                    let b = ws.sidebar.scroll.bounds_for_item(active).expect("laid out");
                    found.set(Some((
                        f32::from(b.left()),
                        f32::from(b.right()),
                        f32::from(b.top()),
                        f32::from(b.size.height),
                    )));
                });
                // Not kept past the window: a handle left over is reported as a leak.
                slot.borrow_mut().take();
            };
            let path = shoot_with(scene, slug, &theme, dims, build.clone(), act.clone());
            println!("ui-shots: {}", path.display());
            let shot = render_with(&theme, dims, build, act);
            let (left, right, top, height) = selected.get().expect("the selected row's bounds");
            let n = format!("{scene}-{slug}");
            check_selected(&shot, dims, &ui, &n, (left, right, top, height));
        }
    }
}

/// A sidebar row as laid out: (left, right, top, height), in points.
type RowBox = (f32, f32, f32, f32);

/// A sidebar row's line 1 (18 pt) and the lines under it (16 pt each), centred in the row
/// (M11i; a third line in a repo, and roomier, since 2026-11-10).
const ROW_LINE_ONE: f32 = 18.0;

const ROW_LINE_TWO: f32 = 16.0;

const ROW_NAME_GAP: f32 = 4.0;

/// The middle of line 1 and of line 2 of a sidebar row `height` tall whose top is at `top`.
fn row_lines(top: f32, height: f32) -> (f32, f32) {
    use crate::ui_tokens::size::{SIDEBAR_ROW, SIDEBAR_ROW_THREE};
    let under = match height {
        h if h >= SIDEBAR_ROW_THREE => 2.0,
        h if h > SIDEBAR_ROW => 1.0,
        _ => 0.0,
    };
    // The name, 4 pt, then the lines under it (2026-11-10).
    let gap = if under > 0.0 { ROW_NAME_GAP } else { 0.0 };
    let content = ROW_LINE_ONE + gap + under * ROW_LINE_TWO;
    let first = top + (height - content) / 2.0;
    (
        first + ROW_LINE_ONE / 2.0,
        first + ROW_LINE_ONE + gap + ROW_LINE_TWO / 2.0,
    )
}

/// The selected row (M11i) at `(left, right, top, height)`: the selection's tint right to its
/// left edge (the accent bar there was removed 2026-11-10, your call), and its name in `text`,
/// never `on_accent`.
fn check_selected(
    shot: &Shot,
    dims: Size<Pixels>,
    ui: &UiColors,
    n: &str,
    (left, right, top, height): RowBox,
) {
    let middle = top + height / 2.0;
    // In its group's card (2026-11-10): the card's colour in the card's 4 pt room at the row's
    // left, then the card's selection from the row's own left edge (where the bar was), in the
    // padding left of the mark, and in the right padding (a row in a repo has text across its
    // middle: the branch line).
    let edge = pixel_at(shot, dims, left + 2.0, middle);
    assert!(
        near(edge, ui.card),
        "{n}: the card beside the row: {edge:?}"
    );
    let (left, right) = (left + 4.0, right - 4.0);
    for (what, x) in [
        ("left edge", left + 1.0),
        ("padding", left + 5.0),
        ("right padding", right - 4.0),
    ] {
        let tint = pixel_at(shot, dims, x, middle);
        // The card's neutral lift (M11r, as the mockup), not the accent's tint.
        assert!(
            near(tint, ui.card_lift),
            "{n}: tint at its {what}: {tint:?}"
        );
    }
    let (line_one, _) = row_lines(top, height);
    let name = count_along(shot, dims, ui.text, line_one, left, right);
    assert!(name > 0, "{n}: the selected row's name in text");
    if !near([ui.text.r, ui.text.g, ui.text.b], ui.on_accent) {
        let ink = count_along(shot, dims, ui.on_accent, line_one, left, right);
        assert_eq!(ink, 0, "{n}: nothing on the selected row in on_accent");
    }
}

/// The compact rows' selection and marks (M11i), from real workspaces in both themes, all in
/// api: `api` selected, `server` idle running a program, `shell` idle at its prompt, `claude`
/// waiting, `docs` idle and hovered.
/// - `sidebar-selection`: api selected (the tint), the pointer over docs.
/// - `sidebar-selection-waiting`: claude selected instead: it takes the selection's tint and
///   keeps its waiting mark and "Needs input".
///
/// Panics unless the selected row has the selection's tint from its left edge and its name in `text` (not `on_accent`); a waiting selected row keeps its waiting
/// colour; the hovered row shows `row_hover` and its ⌘ shortcut while the others hide theirs;
/// and the idle marks are a filled `text_dim` dot (server) and a hollow `text_faint` ring
/// (shell).
fn sidebar_selection_scenes() {
    use gpui::Modifiers;
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    const SESSIONS: [ShotSession; 5] = [
        shot("api", API, Status::Idle, 0, SHELL_TEXT),
        shot("server", API, Status::Idle, 0, SHELL_TEXT),
        shot("shell", API, Status::Idle, 0, SHELL_TEXT),
        shot("claude", API, Status::Waiting, 4 * 60, CLAUDE_PROMPT),
        shot("docs", API, Status::Idle, 0, SHELL_TEXT),
    ];
    let dims = size(px(540.0), px(440.0));
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let ui = theme.ui();
        for (scene, active) in [("sidebar-selection", 0), ("sidebar-selection-waiting", 3)] {
            let slot: Slot = Rc::new(RefCell::new(None));
            let keep = slot.clone();
            let build = move |window: &mut Window, cx: &mut App| -> AnyView {
                cx.set_global(sidebar_appearance(theme, cx));
                let ws = cx.new(|cx| {
                    let mut ws = sidebar_workspace(&SESSIONS, None, active, &[], window, cx);
                    ws.sidebar.hint_dismissed = true;
                    let id = |i: usize| ws.sessions.items()[i].id;
                    let (api, server, shell, docs) = (id(0), id(1), id(2), id(4));
                    let running = ProgramState::Running("npm run dev".into());
                    ws.programs.set_for_shots(server, running);
                    for prompt in [api, shell, docs] {
                        ws.programs.set_for_shots(prompt, ProgramState::Prompt);
                    }
                    ws
                });
                *keep.borrow_mut() = Some(ws.clone());
                ws.into()
            };
            // Each row's (left, right, top, height): the header is item 0, then api..docs.
            let rows: Rc<Cell<[RowBox; 5]>> = Rc::default();
            let seen = rows.clone();
            let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let ws = slotted(&slot);
                let mut all = [(0.0, 0.0, 0.0, 0.0); 5];
                cx.read(|cx| {
                    let scroll = &ws.read(cx).sidebar.scroll;
                    for (i, row) in all.iter_mut().enumerate() {
                        let b = scroll.bounds_for_item(i + 1).expect("laid out");
                        *row = (
                            f32::from(b.left()),
                            f32::from(b.right()),
                            f32::from(b.top()),
                            f32::from(b.size.height),
                        );
                    }
                });
                seen.set(all);
                // The pointer over docs.
                let at = row_point(cx, &ws, 5, 0.0);
                cx.simulate_mouse_move(window, at, None, Modifiers::none());
                slot.borrow_mut().take();
            };
            let path = shoot_with(scene, slug, &theme, dims, build.clone(), act.clone());
            println!("ui-shots: {}", path.display());
            let shot = render_with(&theme, dims, build, act);
            let n = format!("{scene}-{slug}");
            let rows = rows.get();
            check_selected(&shot, dims, &ui, &n, rows[active]);
            // The status badge (M11r): over the avatar's bottom-right corner (the avatar 28 pt,
            // 12 pt in from the row's left, centred in it; the badge 14 pt, 3 pt past it).
            let badge_at = |(left, _, top, height): RowBox| {
                let (x, y) = (
                    left + 40.0 + 3.0 - 7.0,
                    top + (height + 28.0) / 2.0 + 3.0 - 7.0,
                );
                (x - 7.0, x + 7.0, x, y)
            };
            if active == 3 {
                // Selected and waiting: its mark and "Needs input" keep the waiting colour.
                let (x0, x1, _, y) = badge_at(rows[3]);
                let mark = count_along(&shot, dims, ui.ring_waiting, y, x0, x1);
                assert!(mark > 0, "{n}: the waiting mark on the selection");
                let (left, right, top, height) = rows[3];
                let two = row_lines(top, height).1;
                let words = count_along(&shot, dims, ui.ring_waiting, two, left, right);
                assert!(words > 0, "{n}: Needs input on the selection");
            }
            // Hovered docs: row_hover and its shortcut (⌘5) at the right; shell, not hovered,
            // shows no shortcut there.
            // (Small text at 1x is all smoothing: "something drawn" is a pixel well off the row.)
            let right_end = |(_, right, top, height): RowBox, surface| {
                let y = row_lines(top, height).0;
                (right as usize - 40..right as usize - 4)
                    .filter(|&x| gap(pixel_at(&shot, dims, x as f32, y), surface) > 48)
                    .count()
            };
            let (left, _, top, height) = rows[4];
            // In api's card: the card's hover, 4 pt in (2026-11-10).
            let hover = pixel_at(&shot, dims, left + 8.0, top + height / 2.0);
            assert!(near(hover, ui.card_hover), "{n}: docs hovered: {hover:?}");
            let shows = right_end(rows[4], ui.card_hover);
            assert!(shows > 0, "{n}: docs shows its shortcut");
            let hides = right_end(rows[2], ui.card);
            assert_eq!(hides, 0, "{n}: shell hides its shortcut");
            // Idle badges (M11r): server's program runs (a text_dim dot); shell is at its prompt
            // (a fainter, text_faint one).
            let (_, _, x, y) = badge_at(rows[1]);
            let dot = pixel_at(&shot, dims, x, y);
            assert!(near(dot, ui.text_dim), "{n}: server's dot: {dot:?}");
            let (_, _, x, y) = badge_at(rows[2]);
            let faint = pixel_at(&shot, dims, x, y);
            assert!(near(faint, ui.text_faint), "{n}: shell's dot: {faint:?}");
        }
    }
}

/// The sidebar button in the top band, as small windows of the top left corner, in both themes:
/// `sidebar-button-shown`, `-hidden`, `-hover` (the pointer over it) and `-sliding` (a hide
/// 60 ms in, the sidebar partly out and over the pane column).
///
/// Panics unless the icon is drawn at the sidebar band's right edge shown (M11m) and at the
/// window's right hidden (the collapsed band's last button), the hover paints the hit area, and
/// mid-slide the sidebar has left the middle of the window (it is only partly out).
fn sidebar_button_scenes() {
    use crate::sidebar_toggle::{BUTTON_Y, Slide, open_button_x};
    use crate::title_area::band_buttons;
    use crate::ui_tokens::size::SIDEBAR_DEFAULT_WIDTH;
    use gpui::Modifiers;
    use std::time::{Duration, Instant};

    // Tall enough that the pane's terminal still shows above the status bar (M11i).
    let dims = size(px(360.0), px(140.0));
    // Shown, the button is at the sidebar band's right edge (M11m); hidden, it is the collapsed
    // band's last button, at the window's right.
    let (bx, by) = (open_button_x(SIDEBAR_DEFAULT_WIDTH), BUTTON_Y);
    let hx = band_buttons(f32::from(dims.width))[2];
    // The hit area's left margin (inside it, outside the icon), and the icon's left edge.
    let (margin, edge) = (point(bx + 1.5, by + 12.0), point(bx + 4.75, by + 12.0));
    let hidden_edge = point(hx + 4.75, by + 12.0);
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let ui = theme.ui();
        let mut shots = Vec::new();
        for scene in ["shown", "hidden", "hover", "sliding"] {
            let build = move |window: &mut Window, cx: &mut App| -> AnyView {
                cx.set_global(sidebar_appearance(theme, cx));
                let ws = cx.new(|cx| {
                    let mut ws = sidebar_workspace(&EIGHT, None, 0, &[], window, cx);
                    ws.sidebar.hint_dismissed = true;
                    ws.sidebar.hidden = scene == "hidden" || scene == "sliding";
                    if scene == "sliding" {
                        ws.slide = Some(Slide {
                            showing: false,
                            since: Instant::now() - Duration::from_millis(60),
                            from: 1.0,
                        });
                    }
                    ws
                });
                ws.into()
            };
            let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                if scene == "hover" {
                    let at = point(px(bx + 12.0), px(by + 12.0));
                    cx.simulate_mouse_move(window, at, None, Modifiers::none());
                }
            };
            let shot = render_with(&theme, dims, build, act);
            let path = keep_shot(&format!("sidebar-button-{scene}-{slug}"), &shot);
            println!("ui-shots: {}", path.display());
            shots.push((scene, shot));
        }
        let at = |scene: &str, p: gpui::Point<f32>| {
            let (_, shot) = shots.iter().find(|(s, _)| *s == scene).expect("shot");
            pixel_at(shot, dims, p.x, p.y)
        };
        let bg = |rgb: alacritty_terminal::vte::ansi::Rgb| [rgb.r, rgb.g, rgb.b];
        // The icon's left edge is not the band's background: shown, at the sidebar band's
        // right edge; hidden, at the window's right (`band_buttons`).
        assert_ne!(
            at("shown", edge),
            bg(ui.sidebar_background),
            "{slug}: icon shown"
        );
        // (With the sidebar hidden the band is the canvas, the sidebar's colour, since M11i.)
        assert_ne!(
            at("hidden", hidden_edge),
            bg(ui.sidebar_background),
            "{slug}: icon hidden"
        );
        // Hover: the hit area is panel_hover, not the sidebar's colour.
        assert_eq!(
            at("shown", margin),
            bg(ui.sidebar_background),
            "{slug}: not hovered"
        );
        assert_eq!(at("hover", margin), bg(ui.panel_hover), "{slug}: hovered");
        // Inside: on the search button (M11r; the cards below it are the terminal's colour).
        // Beyond it: the pane's tab bar above its tab (M11al: no fill, the card, which is the
        // terminal's colour; the 52 pt bar fills this short window's card under the band).
        let bar_above_tab = ui_tokens::size::TOP_BAND
            + ui_tokens::size::HAIRLINE
            + (ui_tokens::size::TAB_BAR - crate::tab_bar::TAB) / 4.0;
        let (inside, beyond) = (point(30.0, 58.0), point(150.0, bar_above_tab));
        assert_eq!(
            at("sliding", beyond),
            bg(theme.background),
            "{slug}: slid out"
        );
        assert_ne!(
            at("sliding", inside),
            bg(theme.background),
            "{slug}: still partly in"
        );
    }
}

/// How many pixels in the box (`x0..x1`, `y0..y1`, points) are within `tol` of `c`.
fn count_box(
    shot: &Shot,
    dims: Size<Pixels>,
    c: alacritty_terminal::vte::ansi::Rgb,
    tol: u8,
    (x0, x1): (f32, f32),
    (y0, y1): (f32, f32),
) -> usize {
    let scale = shot.width as f32 / f32::from(dims.width);
    let mut count = 0;
    for y in (y0 * scale) as usize..(y1 * scale) as usize {
        for x in (x0 * scale) as usize..(x1 * scale) as usize {
            let i = (y * shot.width as usize + x) * 4;
            if near_by([shot.rgba[i], shot.rgba[i + 1], shot.rgba[i + 2]], c, tol) {
                count += 1;
            }
        }
    }
    count
}

/// The sidebar with nothing running (chore, 2026-11-17: "nothings running but sidebar is
/// clored like somehting is actively running"), in both themes (`sidebar-quiet`): in api, codex
/// working (its bright dot and halo), tests done and a terminal whose `claude` ended (preset
/// `claude`, its title blanked as Claude Code leaves it, now at its prompt); web closed with a
/// done and an idle one; infra closed, all idle.
///
/// Panics unless the ended agent's row keeps `claude` at the first reading at its prompt and is
/// named as a fresh shell's at the second ("New Fuxxpace", like the unnamed shell beside it, its
/// preset gone) while a named one keeps its name; the done row's
/// badge is a muted ✓ (`text_dim`), no `ring_done` green; web's closed header shows a muted ✓
/// and no small avatars; infra's closed header shows nothing after its name.
fn sidebar_quiet_scenes() {
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::time::Instant;

    use crate::groups::NEW_WORKSPACE_NAME;
    use crate::sidebar::SidebarRow;

    const QUIET: [ShotSession; 5] = [
        shot("codex", API, Status::Working, 32, AGENT_TEXT),
        shot("tests", API, Status::Done, 12 * 60, SHELL_TEXT),
        shot("build", WEB, Status::Done, 40, SHELL_TEXT),
        shot("server", WEB, Status::Idle, 0, SHELL_TEXT),
        shot("deploy", INFRA, Status::Idle, 0, SHELL_TEXT),
    ];
    // 460 pt over the old 22 pt status bar: the rows keep their room over M11al's roomier one.
    let dims = size(
        px(520.0),
        px(460.0 + (crate::ui_tokens::size::STATUS_BAR - 22.0)),
    );
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let ui = theme.ui();
        let n = format!("sidebar-quiet-{slug}");
        // Where tests' row, web's header and infra's header were laid out.
        let boxes: Rc<RefCell<Vec<gpui::Bounds<Pixels>>>> = Rc::default();
        let seen = boxes.clone();
        let slot: Slot = Rc::new(RefCell::new(None));
        let keep = slot.clone();
        let named = n.clone();
        let build = move |window: &mut Window, cx: &mut App| -> AnyView {
            cx.set_global(sidebar_appearance(theme, cx));
            let ws = cx.new(|cx| {
                let mut ws = sidebar_workspace(&QUIET, None, 0, &[WEB, INFRA], window, cx);
                ws.sidebar.hint_dismissed = true;
                let api = std::path::Path::new(API);
                // A terminal `claude` ran in (restored as its preset, M11u) and one that never
                // ran an agent: both unnamed.
                let ended = ws.open_unnamed_shot_session(Some(api), SHELL_TEXT, window, cx);
                let fresh = ws.open_unnamed_shot_session(Some(api), SHELL_TEXT, window, cx);
                let claude = crate::presets::builtins()
                    .into_iter()
                    .find(|p| p.name == "claude")
                    .expect("the claude preset");
                let label_of = |ws: &Workspace, id: u64| {
                    let i = ws.sessions.position(|s| s.id == id).expect("opened");
                    let w = ws.workspace_of(i).expect("its workspace");
                    ws.workspace_label(w).to_string()
                };
                let tests = ws.sessions.items()[1].id;
                for id in [ended, tests] {
                    let i = ws.sessions.position(|s| s.id == id).expect("opened");
                    let s = ws.sessions.get_mut(i).expect("opened");
                    s.preset = Some(claude.clone());
                    s.title = Some(String::new().into());
                    ws.programs
                        .set_for_shots(id, ProgramState::Running("claude".into()));
                }
                assert!(ws.note_agents(cx), "{named}: claude runs");
                assert_eq!(label_of(&ws, ended), "claude", "{named}: while it runs");
                // It quits: the shell is in front again.
                for id in [ended, tests] {
                    ws.programs.set_for_shots(id, ProgramState::Prompt);
                }
                // One reading isn't an end (a looping preset's moment at the prompt); the next
                // confirms it.
                ws.note_agents(cx);
                assert_eq!(label_of(&ws, ended), "claude", "{named}: one reading");
                assert!(ws.note_agents(cx), "{named}: claude ended");
                assert_eq!(
                    label_of(&ws, ended),
                    NEW_WORKSPACE_NAME,
                    "{named}: a shell again"
                );
                assert_eq!(
                    label_of(&ws, ended),
                    label_of(&ws, fresh),
                    "{named}: as a fresh one"
                );
                let i = ws.sessions.position(|s| s.id == ended).expect("opened");
                assert_eq!(
                    ws.sessions.items()[i].preset,
                    None,
                    "{named}: its preset went"
                );
                // A name of yours stays.
                assert_eq!(
                    label_of(&ws, tests),
                    "tests",
                    "{named}: a named row keeps it"
                );
                let i = ws.sessions.position(|s| s.id == ended).expect("opened");
                ws.set_status(i, Status::Done, false, cx);
                ws.sessions.get_mut(i).expect("opened").pop = None;
                ws
            });
            *keep.borrow_mut() = Some(ws.clone());
            ws.into()
        };
        let act = move |cx: &mut VisualTestAppContext, _: AnyWindowHandle| {
            let ws = slotted(&slot);
            cx.read(|cx| {
                let ws = ws.read(cx);
                let rows = ws.rows_in_view(Instant::now());
                let at = |want: &dyn Fn(&SidebarRow) -> bool| {
                    let i = rows.iter().position(want).expect("the row");
                    ws.sidebar.scroll.bounds_for_item(i).expect("laid out")
                };
                let header = |label: &'static str| {
                    move |r: &SidebarRow| matches!(r, SidebarRow::Header(h) if h.label == label)
                };
                *seen.borrow_mut() = vec![
                    at(&|r| matches!(r, SidebarRow::Workspace(w) if w.label == "tests")),
                    at(&header("web")),
                    at(&header("infra")),
                ];
            });
            slot.borrow_mut().take();
        };
        let shot = render_with(&theme, dims, build, act);
        save_shot("sidebar-quiet", slug, &shot);
        let boxes = boxes.borrow().clone();
        let (tests, web, infra) = (boxes[0], boxes[1], boxes[2]);
        let span = |b: gpui::Bounds<Pixels>| {
            (
                (f32::from(b.left()), f32::from(b.right())),
                (f32::from(b.top()), f32::from(b.bottom())),
            )
        };
        // tests' avatar and its badge: the row's first 50 pt (its name starts after them).
        let ((left, _), (top, bottom)) = span(tests);
        let face = ((left, left + 50.0), (top, bottom));
        let green = count_box(&shot, dims, ui.ring_done, 3, face.0, face.1);
        assert_eq!(green, 0, "{n}: no green dot on the done row");
        // The check's stroke is anti-aliased: near `text_dim`.
        let tick = count_box(&shot, dims, ui.text_dim, 20, face.0, face.1);
        assert!(tick > 0, "{n}: the done row's muted check");
        // The closed headers' right side: past their names, before the count and chevron.
        let closed = |b: gpui::Bounds<Pixels>| {
            let ((_, right), (top, bottom)) = span(b);
            ((right - 80.0, right - 34.0), (top, bottom))
        };
        let (x, y) = closed(web);
        for (fill, _, _) in ui.avatars {
            let faces = count_box(&shot, dims, fill, 3, x, y);
            assert_eq!(faces, 0, "{n}: no small avatars on web's header");
        }
        assert_eq!(
            count_box(&shot, dims, ui.ring_done, 3, x, y),
            0,
            "{n}: web: no green"
        );
        let tick = count_box(&shot, dims, ui.text_dim, 20, x, y);
        assert!(tick > 0, "{n}: web's header shows its muted check");
        let (x, y) = closed(infra);
        let tick = count_box(&shot, dims, ui.text_dim, 20, x, y);
        assert_eq!(
            tick, 0,
            "{n}: infra (all idle) shows nothing after its name"
        );
    }
}
