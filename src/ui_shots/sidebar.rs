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
        name: "sidebar-band",
        run: sidebar_band_scenes,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/sidebar.rs",
            "src/title_area.rs",
            "src/sidebar_toggle.rs",
            "src/ui_tokens.rs",
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
        name: "sidebar-comfortable",
        run: sidebar_comfortable_scenes,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/sidebar.rs",
            "src/ui_tokens.rs",
            "src/groups.rs",
            "src/providers/mod.rs",
            "src/providers/claude.rs",
        ],
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
/// both `SIDEBAR_ROW_TWO` (two lines, 56 pt since M11al), the active row is scrolled into view and
/// rows scrolled away don't count as on screen, and the dragged row's run slid as described.
fn sidebar_scenes() {
    use gpui::{Modifiers, MouseButton};
    use std::cell::RefCell;
    use std::rc::Rc;

    let dims = size(px(960.0), px(600.0));
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        for (scene, width, dims) in [
            ("sidebar", ui_tokens::size::SIDEBAR_DEFAULT_WIDTH, dims),
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
                    // Every row has one line under its name (M11r): two lines, 56 pt (M11al).
                    let two = ui_tokens::size::SIDEBAR_ROW_TWO;
                    assert_eq!(height(1), two, "{scene}-{slug}: waiting row");
                    assert_eq!(height(3), two, "{scene}-{slug}: the split workspace's row");
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
                        // codex (row 2, two lines tall) down past tests and shell: its copy right
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
                // Between its avatar and its name (M11r; M11al's roomier avatar).
                use crate::sidebar::{AVATAR, AVATAR_GAP, ROW_PAD};
                let between = ROW_PAD + AVATAR + AVATAR_GAP / 2.0;
                let p = pixel_at(
                    &shot,
                    dims,
                    f32::from(claude.left()) + between,
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
            // shell (selected) slid up by codex's room; the gap below it, down to where shell
            // ended, is filled by codex's copy (as tall as codex's row), top to bottom: no empty
            // band, no line.
            let up = px_at(shell.top() - px(room) + px(3.0));
            // (Selected: the accent's soft tint on the canvas, M11al; no group card.)
            assert!(is(up, ui.row_selected), "{n}: shell slid up: {up:?}");
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
                    !is(p, ui.sidebar_background) && !is(p, ui.row_selected),
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

/// The sidebar rows (M11b; compact in M11i; 56 pt in M11al), from real workspaces in both
/// themes, at 290 pt (`sidebar-rows`) and 180 pt (`sidebar-rows-narrow`): deploy and claude
/// pinned (📌, above every group heading), claude waiting with its question under its name
/// ("Needs input" alone at 180 pt); notes in no group, a gap under the pinned rows; payments
/// selected (the selection's tint and the accent bar at the sidebar's edge, its text in
/// `text`); the long name cut; notes in /tmp (no `~`).
///
/// Panics unless the pinned rows lead, the rows still know their folder (the row shows it as
/// the last part only: line two says who and where, M11al §2.4), every row is `SIDEBAR_ROW_TWO`
/// and every heading `GROUP_HEADER` tall, and the selected row is tinted to its left edge with the
/// accent bar beside it and its name in `text` (`check_selected`).
fn sidebar_row_scenes() {
    use std::cell::{Cell, RefCell};
    use std::path::Path;
    use std::rc::Rc;
    use std::time::Instant;

    use crate::sidebar::SidebarRow;

    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let ui = theme.ui();
        for (scene, width) in [
            ("sidebar-rows", ui_tokens::size::SIDEBAR_DEFAULT_WIDTH),
            ("sidebar-rows-narrow", 180.0),
        ] {
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
                    let shape: Vec<(String, bool, Option<String>)> = rows
                        .iter()
                        .map(|r| match r {
                            SidebarRow::Header(h) => (format!("[{}]", h.label), false, None),
                            SidebarRow::Workspace(w) => {
                                (w.label.clone(), w.pinned, w.folder.clone())
                            }
                        })
                        .collect();
                    let row = |label: &str, pinned, folder: Option<&str>| {
                        (label.to_string(), pinned, folder.map(String::from))
                    };
                    let header = |label: &str| row(label, false, None);
                    let long_folder = "~/code/a-very-long-project-folder/services/payments";
                    assert_eq!(
                        shape,
                        [
                            row("deploy", true, Some("~/code/infra")),
                            row("claude", true, Some("~/code/api")),
                            row("notes", false, Some("/tmp/scratch")),
                            header("[api]"),
                            row("codex", false, Some("~/code/api")),
                            header("[payments]"),
                            row("payments", false, Some(long_folder)),
                            header("[web]"),
                            row(ROWS[4].name, false, Some("~/code/web")),
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
                    // Every row has one line under its name (M11r): who and where (M11al, no
                    // branch), `SIDEBAR_ROW_TWO` (56) each; headings `GROUP_HEADER`.
                    let two = ui_tokens::size::SIDEBAR_ROW_TWO;
                    for i in [0, 1, 2, 4, 6, 8] {
                        let seen = f32::from(bounds(i).size.height);
                        assert_eq!(seen, two, "{n}: row {i}'s height");
                    }
                    for i in [3, 5, 7] {
                        let seen = f32::from(bounds(i).size.height);
                        let want = ui_tokens::size::GROUP_HEADER;
                        assert_eq!(seen, want, "{n}: heading {i}'s height");
                    }
                    // Line two says who and where: the folder's last part (an SSH-less row in a
                    // repo leaves its branch out).
                    let said: Vec<String> = rows
                        .iter()
                        .filter_map(|r| match r {
                            SidebarRow::Workspace(w) => crate::sidebar::place_lines(w).pop(),
                            SidebarRow::Header(_) => None,
                        })
                        .collect();
                    let local = crate::sidebar::local_shell();
                    let shell = |place: &str| format!("\u{203a}_ {local} \u{b7} {place}");
                    assert_eq!(said[0], shell("infra"), "{n}: deploy");
                    assert_eq!(said[1], CLAUDE_QUESTION, "{n}: claude waits");
                    assert_eq!(said[2], shell("scratch"), "{n}: notes");
                    assert_eq!(said[4], shell("payments"), "{n}: payments");
                    assert!(
                        said.iter().all(|line| !line.contains("feature/")),
                        "{n}: no branch on a row: {said:?}"
                    );
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

/// The middle of line 1 (the name, `NAME_LINE`) and of line 2 (`SUB_LINE`, `NAME_GAP` under
/// it) of a sidebar row `height` tall whose top is at `top`: every row has its line two (M11r),
/// the two centred in the row.
fn row_lines(top: f32, height: f32) -> (f32, f32) {
    use crate::sidebar::{NAME_GAP, NAME_LINE, SUB_LINE};
    let content = NAME_LINE + NAME_GAP + SUB_LINE;
    let first = top + (height - content) / 2.0;
    (
        first + NAME_LINE / 2.0,
        first + NAME_LINE + NAME_GAP + SUB_LINE / 2.0,
    )
}

/// The selected row (M11i; M11al §2.3) at `(left, right, top, height)`: the canvas between
/// the sidebar's edge and the row, a `SELECTED_BAR` accent bar at the sidebar's very left edge
/// (stopping `SELECTED_BAR_INSET` short of the row's top), `row_selected` from the row's own left
/// edge to its right, and its name in `text`, never `on_accent`.
fn check_selected(
    shot: &Shot,
    dims: Size<Pixels>,
    ui: &UiColors,
    n: &str,
    (left, right, top, height): RowBox,
) {
    use crate::sidebar::{ROW_INSET, SELECTED_BAR, SELECTED_BAR_INSET};
    let middle = top + height / 2.0;
    let edge = left - ROW_INSET;
    let bar = pixel_at(shot, dims, edge + SELECTED_BAR / 2.0, middle);
    assert!(near(bar, ui.accent), "{n}: the accent bar: {bar:?}");
    let above = pixel_at(
        shot,
        dims,
        edge + SELECTED_BAR / 2.0,
        top + SELECTED_BAR_INSET - 3.0,
    );
    assert!(
        near(above, ui.sidebar_background),
        "{n}: the bar stops short of the row's top: {above:?}"
    );
    let beside = pixel_at(shot, dims, (edge + SELECTED_BAR + left) / 2.0 + 1.0, middle);
    assert!(
        near(beside, ui.sidebar_background),
        "{n}: the canvas between the bar and the row: {beside:?}"
    );
    for (what, x) in [
        ("left edge", left + 1.0),
        ("padding", left + 5.0),
        ("right padding", right - 4.0),
    ] {
        let tint = pixel_at(shot, dims, x, middle);
        assert!(
            near(tint, ui.row_selected),
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

/// The roomier rows (M11al Task 8, spec §2.3–2.5), from real workspaces in both themes, at the
/// default width (`sidebar-comfortable`) and at 180 (`sidebar-comfortable-narrow`): in api,
/// `api` working with Claude in front (selected), `server` idle running npm (hovered), `shell`
/// at its prompt; in payments a long name running a long program; web closed with one waiting.
///
/// Panics unless every row is `SIDEBAR_ROW_TWO` (56) tall and every heading `GROUP_HEADER`; line
/// two says who and where ("✳ Claude · api", "› npm · api", "›_ <shell> · api"); each badge's
/// ring takes its row's fill (the canvas, `row_hover` hovered, `row_selected` selected); the
/// selected row has its accent bar at the sidebar's left edge (`check_selected`); the age shows
/// top right at the default width and not at 180; the headings are text on the canvas (no card)
/// in `text_faint`, the closed one showing "1 waiting" and no avatars; and at 180 the long row's
/// name and line two stop inside its right padding (cut, nothing past it).
fn sidebar_comfortable_scenes() {
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::time::Instant;

    use gpui::Modifiers;

    use crate::sidebar::{
        AVATAR, BADGE, BADGE_OUT, BADGE_RING, HEADING_ABOVE, ROW_PAD, SidebarRow, place_lines,
    };
    use crate::ui_tokens::size::{GROUP_HEADER, SIDEBAR_ROW_TWO};

    const LONG: &str = "a-really-long-workspace-name-for-a-narrow-sidebar";
    const LONG_PROGRAM: &str = "a-very-long-program-name-that-runs";
    const COMFORTABLE: [ShotSession; 5] = [
        shot("api", API, Status::Working, 2 * 60, AGENT_TEXT),
        shot("server", API, Status::Idle, 0, SHELL_TEXT),
        shot("shell", API, Status::Idle, 0, SHELL_TEXT),
        shot(LONG, PAYMENTS, Status::Idle, 0, SHELL_TEXT),
        shot("fix", WEB, Status::Waiting, 3 * 3600, SHELL_TEXT),
    ];
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let ui = theme.ui();
        for (scene, width) in [
            (
                "sidebar-comfortable",
                ui_tokens::size::SIDEBAR_DEFAULT_WIDTH,
            ),
            (
                "sidebar-comfortable-narrow",
                ui_tokens::size::SIDEBAR_MIN_WIDTH,
            ),
        ] {
            let n = format!("{scene}-{slug}");
            let dims = size(px(width + 400.0), px(640.0));
            let slot: Slot = Rc::new(RefCell::new(None));
            let keep = slot.clone();
            let named = n.clone();
            let build = move |window: &mut Window, cx: &mut App| -> AnyView {
                cx.set_global(sidebar_appearance(theme, cx));
                let ws = cx.new(|cx| {
                    let mut ws = sidebar_workspace(&COMFORTABLE, None, 0, &[WEB], window, cx);
                    ws.sidebar.width = width;
                    ws.sidebar.hint_dismissed = true;
                    let id = |i: usize| ws.sessions.items()[i].id;
                    let (api, server, shell, long) = (id(0), id(1), id(2), id(3));
                    // Claude in front in api's terminal (its preset by name: an agent fuxx has
                    // a provider for); npm in server's; the prompt in shell's.
                    ws.programs
                        .set_for_shots(api, ProgramState::Running("claude".into()));
                    ws.programs
                        .set_for_shots(server, ProgramState::Running("npm".into()));
                    ws.programs.set_for_shots(shell, ProgramState::Prompt);
                    ws.programs
                        .set_for_shots(long, ProgramState::Running(LONG_PROGRAM.into()));
                    assert!(ws.note_agents(cx), "{named}: claude runs in api");
                    ws
                });
                *keep.borrow_mut() = Some(ws.clone());
                ws.into()
            };
            // Each item's bounds, and what each row's line two says.
            type Laid = Vec<(Option<String>, gpui::Bounds<Pixels>)>;
            let laid: Rc<RefCell<Laid>> = Rc::default();
            let seen = laid.clone();
            let acting = slot.clone();
            let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let ws = slotted(&acting);
                let items: Laid = cx.read(|cx| {
                    let ws = ws.read(cx);
                    let rows = ws.rows_in_view(Instant::now());
                    rows.iter()
                        .enumerate()
                        .map(|(i, row)| {
                            let line = match row {
                                SidebarRow::Workspace(w) => place_lines(w).pop(),
                                SidebarRow::Header(_) => None,
                            };
                            let b = ws.sidebar.scroll.bounds_for_item(i).expect("laid out");
                            (line, b)
                        })
                        .collect()
                });
                // The pointer over server's row (item 2: api's heading is item 0).
                let at = row_point(cx, &ws, 2, 0.0);
                cx.simulate_mouse_move(window, at, None, Modifiers::none());
                *seen.borrow_mut() = items;
                acting.borrow_mut().take();
            };
            let path = shoot_with(scene, slug, &theme, dims, build.clone(), act.clone());
            println!("ui-shots: {}", path.display());
            let shot = render_with(&theme, dims, build, act);
            let items = laid.borrow().clone();
            // [api] api server shell [payments] long [web]
            assert_eq!(items.len(), 7, "{n}: {items:?}");
            let lines: Vec<Option<&str>> = items.iter().map(|(l, _)| l.as_deref()).collect();
            let local = crate::sidebar::local_shell();
            let shell = format!("\u{203a}_ {local} \u{b7} api");
            let long = format!("\u{203a} {LONG_PROGRAM} \u{b7} payments");
            assert_eq!(
                lines,
                [
                    None,
                    Some("\u{2733} Claude \u{b7} api"),
                    Some("\u{203a} npm \u{b7} api"),
                    Some(shell.as_str()),
                    None,
                    Some(long.as_str()),
                    None,
                ],
                "{n}: who and where"
            );
            let span = |i: usize| {
                let b = items[i].1;
                (
                    f32::from(b.left()),
                    f32::from(b.right()),
                    f32::from(b.top()),
                    f32::from(b.size.height),
                )
            };
            for i in [1, 2, 3, 5] {
                assert_eq!(span(i).3, SIDEBAR_ROW_TWO, "{n}: row {i} is 56");
            }
            for i in [0, 4, 6] {
                assert_eq!(span(i).3, GROUP_HEADER, "{n}: heading {i}");
            }
            // The selected row: its tint, the accent bar at the sidebar's edge, its name.
            check_selected(&shot, dims, &ui, &n, span(1));
            // Each badge's ring is its row's fill where it lies over the avatar: above and left
            // of the mark, halfway through the ring.
            let ring = |(left, _, top, height): RowBox| {
                let half = BADGE / 2.0 + BADGE_RING;
                let (cx, cy) = (
                    left + ROW_PAD + AVATAR + BADGE_OUT - half,
                    top + (height + AVATAR) / 2.0 + BADGE_OUT - half,
                );
                let mid = BADGE / 2.0 + BADGE_RING / 2.0;
                [
                    pixel_at(&shot, dims, cx, cy - mid),
                    pixel_at(&shot, dims, cx - mid, cy),
                ]
            };
            for (i, fill, what) in [
                (1, ui.row_selected, "selected"),
                (2, ui.row_hover, "hovered"),
                (3, ui.sidebar_background, "plain"),
            ] {
                for p in ring(span(i)) {
                    assert!(near(p, fill), "{n}: the {what} row's ring: {p:?}");
                }
            }
            // The hovered ring took the hover's fill: it is not the canvas (a ring that kept
            // the plain row's fill would pass a loose check, `row_hover` being close to it).
            for p in ring(span(2)) {
                assert!(
                    !near(p, ui.sidebar_background),
                    "{n}: the hovered row's ring is still the canvas: {p:?}"
                );
            }
            // The age (api's "2m"), top right, at the default width only: right of the name's
            // short words, before the right padding.
            let (_, right, top, height) = span(1);
            let line_one = row_lines(top, height).0;
            let aged = (right as usize - ROW_PAD as usize - 24..right as usize - ROW_PAD as usize)
                .filter(|&x| gap(pixel_at(&shot, dims, x as f32, line_one), ui.row_selected) > 48)
                .count();
            if width >= ui_tokens::size::SIDEBAR_DEFAULT_WIDTH {
                assert!(aged > 0, "{n}: api's age shows top right");
            } else {
                assert_eq!(aged, 0, "{n}: no age at 180");
            }
            // The headings: text on the canvas, no card: the room above the words is the canvas
            // across the sidebar; the words in `text_faint`.
            for i in [0, 4] {
                let (left, right, top, _) = span(i);
                let above = top + HEADING_ABOVE / 2.0;
                let carded = ((left + 2.0) as usize..(right - 2.0) as usize)
                    .step_by(6)
                    .filter(|&x| {
                        !near(
                            pixel_at(&shot, dims, x as f32, above),
                            ui.sidebar_background,
                        )
                    })
                    .count();
                assert_eq!(carded, 0, "{n}: heading {i} has no card");
                let words = count_box(
                    &shot,
                    dims,
                    ui.text_faint,
                    20,
                    (left, right),
                    (top + HEADING_ABOVE, top + GROUP_HEADER),
                );
                assert!(words > 0, "{n}: heading {i}'s words in text_faint");
            }
            // web, closed: its status ("1 waiting" on the waiting tint), no avatars.
            let (left, right, top, height) = span(6);
            let (x, y) = ((left, right), (top + HEADING_ABOVE - 4.0, top + height));
            let pill = count_box(&shot, dims, ui.waiting_tint, 3, x, y);
            assert!(pill > 0, "{n}: web shows how many wait");
            // (A square of an avatar's colour, not a stray pixel of the pill's anti-aliased
            // words that happens to match one.)
            for (fill, _, _) in ui.avatars {
                let faces = solid_count(&shot, dims, fill, x, y);
                assert_eq!(faces, 0, "{n}: no avatars on web's heading");
            }
            // The long row: its name and line two stop inside its right padding.
            let (left, right, top, height) = span(5);
            let (one, two) = row_lines(top, height);
            for y in [one, two] {
                let past = ((right - ROW_PAD + 2.0) as usize..(right - 2.0) as usize)
                    .filter(|&x| !near(pixel_at(&shot, dims, x as f32, y), ui.sidebar_background))
                    .count();
                assert_eq!(
                    past, 0,
                    "{n}: the long row's text stops in its padding at {y}"
                );
            }
            // And starts after the badge: nothing between the badge's ring and the text.
            let after = left + ROW_PAD + AVATAR + BADGE_OUT + 1.0;
            let gap_x = left + ROW_PAD + AVATAR + crate::sidebar::AVATAR_GAP - 1.0;
            let mid = top + height / 2.0;
            for x in [after, gap_x] {
                let p = pixel_at(&shot, dims, x, mid);
                assert!(
                    near(p, ui.sidebar_background),
                    "{n}: nothing between the badge and the text at {x}: {p:?}"
                );
            }
        }
    }
    who_follows_the_active_tab();
}

/// Task 8b (the user's ruling): a row's who follows its workspace's active tab, with the chrome
/// cached (M11p). One workspace: a terminal running npm and Claude's tab split beside it (as a
/// provider preset launches it), Claude's tab focused. Each step (focus the terminal, then the
/// Claude tab) must change what's shown and leave the window as a full redraw would draw it
/// (`Steps`: the cached sidebar was drawn again), and the row's line two must follow.
fn who_follows_the_active_tab() {
    use crate::layout_model::TabId;
    use crate::sidebar::{SidebarRow, place_lines};

    let n = "sidebar-comfortable";
    let (mut cx, window, ws) = super::chrome_parts::chrome_window(|ws, window, cx| {
        ws.sidebar.hint_dismissed = true;
        let api = std::path::Path::new(API);
        ws.open_shot_session("dev", api, SHELL_TEXT, None, window, cx);
        let id = ws.sessions.items()[0].id;
        ws.programs
            .set_for_shots(id, ProgramState::Running("npm".into()));
        ws.agent_beside(id, "claude", window, cx);
    });
    let ws = &ws;
    let said = |cx: &mut VisualTestAppContext| {
        cx.read(|cx| {
            let rows = ws.read(cx).rows(std::time::Instant::now());
            rows.iter()
                .find_map(|r| match r {
                    SidebarRow::Workspace(w) => place_lines(w).pop(),
                    SidebarRow::Header(_) => None,
                })
                .expect("dev's row")
        })
    };
    let (terminal, agent): (TabId, TabId) = cx.read(|cx| {
        let ws = ws.read(cx);
        let id = ws.sessions.items()[0].id;
        let w = ws.model.focused_workspace().expect("a workspace");
        let agent = ws.model.agent_tabs(w)[0];
        (ws.model.terminal_tab(id).expect("its tab"), agent)
    });
    let claude = "\u{2733} Claude \u{b7} api";
    assert_eq!(said(&mut cx), claude, "{n}: Claude's tab is active");
    let mut steps = Steps::new(n, &mut cx, window, ws);
    let select = |tab: TabId| {
        move |cx: &mut VisualTestAppContext| {
            cx.update_window(window, |_, window, cx| {
                ws.update(cx, |ws, cx| ws.select_tab(tab, window, cx))
            })
            .expect("select the tab");
        }
    };
    steps.step(
        &mut cx,
        window,
        "the terminal running npm focused",
        Seen::Changes,
        select(terminal),
    );
    assert_eq!(
        said(&mut cx),
        "\u{203a} npm \u{b7} api",
        "{n}: npm's terminal is active"
    );
    steps.step(
        &mut cx,
        window,
        "Claude's tab focused again",
        Seen::Changes,
        select(agent),
    );
    assert_eq!(said(&mut cx), claude, "{n}: Claude's tab is active again");
    steps.done();
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
    // 440 pt over the old sidebar chrome and rows: all five rows keep their room.
    let dims = size(
        px(540.0),
        px(440.0 + SIDEBAR_CHROME_GROWTH + sidebar_list_growth(5.0, 1.0)),
    );
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
            // The status badge (M11r; M11al §2.3): over the avatar's bottom-right corner (the
            // avatar `ROW_PAD` in from the row's left, centred in it; the badge's ring
            // `BADGE_OUT` past it).
            let badge_at = |(left, _, top, height): RowBox| {
                use crate::sidebar::{AVATAR, BADGE, BADGE_OUT, BADGE_RING, ROW_PAD};
                let half = BADGE / 2.0 + BADGE_RING;
                let (x, y) = (
                    left + ROW_PAD + AVATAR + BADGE_OUT - half,
                    top + (height + AVATAR) / 2.0 + BADGE_OUT - half,
                );
                (x - half, x + half, x, y)
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
            // On the canvas (M11al: no group card): `row_hover`, in its padding.
            let hover = pixel_at(&shot, dims, left + 6.0, top + height / 2.0);
            assert!(near(hover, ui.row_hover), "{n}: docs hovered: {hover:?}");
            let shows = right_end(rows[4], ui.row_hover);
            assert!(shows > 0, "{n}: docs shows its shortcut");
            let hides = right_end(rows[2], ui.sidebar_background);
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
/// Panics unless the icon is drawn at the sidebar band's right edge shown (M11m) and just past
/// the traffic lights hidden (2026-11-20, over the card's top-left bar), the hover paints the
/// hit area, and mid-slide the sidebar has left the middle of the window (it is only partly
/// out).
fn sidebar_button_scenes() {
    use crate::sidebar_toggle::{BUTTON_Y, Slide, open_button_x};
    use crate::title_area::band_buttons;
    use crate::ui_tokens::size::SIDEBAR_DEFAULT_WIDTH;
    use gpui::Modifiers;
    use std::time::{Duration, Instant};

    // Tall enough that the pane's terminal still shows above the status bar (M11i).
    let dims = size(px(360.0), px(140.0));
    // Shown, the button is at the sidebar band's right edge (M11m); hidden, the first of the
    // collapsed sidebar's buttons, by the lights.
    let (bx, by) = (open_button_x(SIDEBAR_DEFAULT_WIDTH), BUTTON_Y);
    let hx = band_buttons(false)[0];
    // The hit area's left margin (inside it, outside the icon), and the icon's left edge
    // (Lucide's panel-left, 16 in the 24 hit: its frame's line 6 in).
    let (margin, edge) = (point(bx + 1.5, by + 12.0), point(bx + 6.0, by + 12.0));
    let hidden_edge = point(hx + 6.0, by + 12.0);
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
        // The icon's left edge is not what is under it: shown, the band at the sidebar band's
        // right edge; hidden, the card's top-left bar by the lights (`band_buttons`).
        assert_ne!(
            at("shown", edge),
            bg(ui.sidebar_background),
            "{slug}: icon shown"
        );
        let p = at("hidden", hidden_edge);
        assert!(
            !near_by(p, theme.background, 1),
            "{slug}: icon hidden: {p:?}"
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
        // terminal's colour; under the `CARD_INSET` strip since 2026-11-20, its room by the
        // lights empty there).
        let bar_above_tab = ui_tokens::size::CARD_INSET
            + ui_tokens::size::HAIRLINE
            + (ui_tokens::size::TAB_BAR - crate::tab_bar::TAB) / 4.0;
        let (inside, beyond) = (point(30.0, 58.0), point(150.0, bar_above_tab));
        let p = at("sliding", beyond);
        assert!(near_by(p, theme.background, 1), "{slug}: slid out: {p:?}");
        assert_ne!(
            at("sliding", inside),
            bg(theme.background),
            "{slug}: still partly in"
        );
    }
}

/// How many 3 × 3 pt squares (at pixel steps) in the box (`x0..x1`, `y0..y1`, points) are all
/// within 3 of `c`: a filled shape of that colour, not anti-aliased text that touches it.
fn solid_count(
    shot: &Shot,
    dims: Size<Pixels>,
    c: alacritty_terminal::vte::ansi::Rgb,
    (x0, x1): (f32, f32),
    (y0, y1): (f32, f32),
) -> usize {
    let scale = shot.width as f32 / f32::from(dims.width);
    let side = (3.0 * scale) as usize;
    let at = |x: usize, y: usize| {
        let i = (y * shot.width as usize + x) * 4;
        near_by([shot.rgba[i], shot.rgba[i + 1], shot.rgba[i + 2]], c, 3)
    };
    let (xs, ys) = (
        (x0 * scale) as usize..(x1 * scale) as usize - side,
        (y0 * scale) as usize..(y1 * scale) as usize - side,
    );
    ys.flat_map(|y| xs.clone().map(move |x| (x, y)))
        .filter(|&(x, y)| (0..side).all(|dy| (0..side).all(|dx| at(x + dx, y + dy))))
        .count()
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
    // 460 pt over the old 22 pt status bar and the old sidebar chrome: the rows keep their room
    // over M11al's roomier ones.
    let dims = size(
        px(520.0),
        px(460.0
            + (crate::ui_tokens::size::STATUS_BAR - 22.0)
            + SIDEBAR_CHROME_GROWTH
            + sidebar_list_growth(5.0, 3.0)),
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
        // tests' avatar and its badge: the row's padding, avatar and the badge past it (its name
        // starts after them).
        let ((left, _), (top, bottom)) = span(tests);
        use crate::sidebar::{AVATAR, BADGE_OUT, ROW_PAD};
        let face = (
            (left, left + ROW_PAD + AVATAR + BADGE_OUT + 1.0),
            (top, bottom),
        );
        let green = count_box(&shot, dims, ui.ring_done, 3, face.0, face.1);
        assert_eq!(green, 0, "{n}: no green dot on the done row");
        // The check's stroke is anti-aliased: near `text_dim`. Looked for in the badge's
        // corner only, past the avatar's letters (`text_dim` too since 2026-11-20).
        let corner = (
            (left + ROW_PAD + AVATAR - 10.0, face.0.1),
            ((top + bottom) / 2.0 + AVATAR / 2.0 - 10.0, bottom),
        );
        let tick = count_box(&shot, dims, ui.text_dim, 20, corner.0, corner.1);
        assert!(tick > 0, "{n}: the done row's muted check");
        // The closed headers' right side: past their names, before the count and chevron.
        let closed = |b: gpui::Bounds<Pixels>| {
            let ((_, right), (top, bottom)) = span(b);
            ((right - 80.0, right - 34.0), (top, bottom))
        };
        let (x, y) = closed(web);
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

/// The sidebar's search box and footer (M11al Task 7, spec 2.2 and 2.6), in both themes, in a
/// new window (the default width): the search box `SEARCH` tall under the band at 4 / 14 / 10
/// margins, `raised` fill, a 1 px `card_border` edge; the footer `FOOTER` tall with its 1 px
/// top line, the "New Fuxxpace" button 38 tall (a `card_border` edge, no fill) reaching the
/// bell's box a gap away; no line down the sidebar's right edge.
///
/// Panics unless a new window's sidebar is `SIDEBAR_DEFAULT_WIDTH` wide, the list starts 10 pt
/// under the search box, and those edges and fills are read off the pixels.
fn sidebar_band_scenes() {
    use std::cell::RefCell;
    use std::rc::Rc;

    use crate::sidebar::{FOOTER, SEARCH, SEARCH_BOTTOM, SEARCH_SIDE, SEARCH_TOP};
    use crate::title_area::{FOOTER_BUTTON, FOOTER_GAP, FOOTER_SIDE, footer_buttons};
    use ui_tokens::size::{SIDEBAR_DEFAULT_WIDTH, STATUS_BAR, TOP_BAND};

    let dims = size(px(960.0), px(600.0));
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let ui = theme.ui();
        let n = format!("sidebar-band-{slug}");
        let slot: Slot = Rc::new(RefCell::new(None));
        let keep = slot.clone();
        let build = move |window: &mut Window, cx: &mut App| -> AnyView {
            cx.set_global(sidebar_appearance(theme, cx));
            let ws = cx.new(|cx| {
                let mut ws = sidebar_workspace(&EIGHT, Some(3), 3, &[WEB], window, cx);
                ws.sidebar.hint_dismissed = true;
                ws
            });
            *keep.borrow_mut() = Some(ws.clone());
            ws.into()
        };
        let list_top = Rc::new(std::cell::Cell::new(0.0_f32));
        let width = Rc::new(std::cell::Cell::new(0.0_f32));
        let (found_top, found_width) = (list_top.clone(), width.clone());
        let act = move |cx: &mut VisualTestAppContext, _: AnyWindowHandle| {
            let ws = slotted(&slot);
            cx.read(|cx| {
                let ws = ws.read(cx);
                found_top.set(f32::from(ws.sidebar.scroll.bounds().top()));
                found_width.set(ws.sidebar.width);
            });
            slot.borrow_mut().take();
        };
        let shot = render_with(&theme, dims, build, act);
        let w = SIDEBAR_DEFAULT_WIDTH;
        assert_eq!(w, 290.0, "{n}: the default width");
        assert_eq!(width.get(), w, "{n}: a new window's sidebar width");
        let at = |x: f32, y: f32| pixel_at(&shot, dims, x, y);
        let (edge, fill, side) = (ui.card_border, ui.raised, ui.sidebar_background);

        // The search box: SEARCH tall, SEARCH_TOP under the band, SEARCH_SIDE from the sides.
        let top = TOP_BAND + SEARCH_TOP;
        let mid = top + SEARCH / 2.0;
        assert_eq!(SEARCH, 38.0, "{n}: the search box's height");
        assert_eq!(
            list_top.get(),
            top + SEARCH + SEARCH_BOTTOM,
            "{n}: the list starts under the box's bottom margin"
        );
        assert!(near(at(SEARCH_SIDE + 0.5, mid), edge), "{n}: left edge");
        assert!(
            near(at(w - SEARCH_SIDE - 0.5, mid), edge),
            "{n}: right edge"
        );
        assert!(near(at(w / 2.0, top + 0.5), edge), "{n}: top edge");
        assert!(
            near(at(w / 2.0, top + SEARCH - 0.5), edge),
            "{n}: bottom edge"
        );
        assert!(near(at(SEARCH_SIDE + 6.0, mid), fill), "{n}: raised fill");
        assert!(near(at(SEARCH_SIDE - 2.0, mid), side), "{n}: margin, left");
        assert!(near(at(w / 2.0, top - 2.0), side), "{n}: margin, above");
        assert!(
            near(at(w / 2.0, top + SEARCH + 2.0), side),
            "{n}: margin, below"
        );

        // The footer: FOOTER tall above the status bar, its line on top, the button 38 tall
        // with 12 above and below, from FOOTER_SIDE to a FOOTER_GAP short of the bell's box.
        let foot = dims.height.as_f32() - STATUS_BAR - FOOTER;
        assert_eq!(FOOTER, 62.0, "{n}: the footer's height");
        assert!(
            near(at(w / 2.0, foot + 0.5), edge),
            "{n}: the footer's top line"
        );
        assert!(near(at(5.0, foot + 3.0), side), "{n}: under the line");
        let [_, bell, _] = footer_buttons(w);
        assert_eq!(FOOTER_BUTTON, 34.0, "{n}: the bell's box");
        let right = bell - FOOTER_GAP;
        let (button_top, button_mid) = (foot + 12.0, foot + FOOTER / 2.0);
        assert!(
            near(at(FOOTER_SIDE + 0.5, button_mid), edge),
            "{n}: button, left"
        );
        assert!(
            near(at(right - 0.5, button_mid), edge),
            "{n}: button, right"
        );
        assert!(
            near(at(right + 2.0, button_mid), side),
            "{n}: gap to the bell"
        );
        assert!(near(at(60.0, button_top + 0.5), edge), "{n}: button, top");
        assert!(
            near(at(60.0, button_top + 37.5), edge),
            "{n}: button, bottom"
        );
        assert!(
            near(at(60.0, button_top - 1.5), side),
            "{n}: 12 above the button"
        );
        assert!(
            near(at(60.0, button_top + 39.5), side),
            "{n}: 12 below the button"
        );
        // No edge line down the sidebar's right: the card's edge and shadow separate them.
        assert!(
            !near(at(w - 0.5, TOP_BAND + 200.0), edge),
            "{n}: no line on the sidebar's right edge"
        );
        println!("ui-shots: {}", keep_shot(&n, &shot).display());
    }
}
