//! The right sidebar's Changes panel (M11h, M11v, M11w: list, branch bar, staging, GRAPH) and
//! its Activity panel (M11h).

use super::right_sidebar::{files_fixture, right_panel_workspace};
use super::*;

/// This file, as `file!()` gives it (repo-relative under cargo): `registry::AREAS` pairs it with
/// `SCENES`, so `--changed` runs these scenes when this file changes.
pub(super) const FILE: &str = file!();

/// This area's scenes (`registry.rs` runs them in its `ORDER`).
pub(super) const SCENES: &[Scene] = &[
    Scene {
        name: "changes",
        run: changes_scenes,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/changes_panel.rs",
            "src/right_sidebar.rs",
            "src/git_status.rs",
            "src/git_stage.rs",
            "src/ssh_target.rs",
            "src/layout_model.rs",
            "src/file_tab.rs",
        ],
    },
    Scene {
        name: "changes-bar",
        run: changes_bar_scenes,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/changes_panel.rs",
            "src/right_sidebar.rs",
            "src/branch_list.rs",
            "src/branch_sync.rs",
            "src/git_status.rs",
        ],
    },
    Scene {
        name: "changes-staging",
        run: changes_staging_scenes,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/changes_panel.rs",
            "src/right_sidebar.rs",
            "src/git_status.rs",
            "src/git_stage.rs",
        ],
    },
    Scene {
        name: "changes-graph",
        run: changes_graph_scenes,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/changes_panel.rs",
            "src/graph_panel.rs",
            "src/git_graph.rs",
            "src/graph_layout.rs",
            "src/git_status.rs",
            "src/right_sidebar.rs",
        ],
    },
    Scene {
        name: "changes-sticky",
        run: changes_sticky_scenes,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/changes_panel.rs",
            "src/graph_panel.rs",
            "src/git_graph.rs",
            "src/graph_layout.rs",
            "src/git_status.rs",
            "src/right_sidebar.rs",
        ],
    },
    Scene {
        name: "activity",
        run: activity_scenes,
        themes: Themes::Both,
        // Serial: (c) nothing moves over 200 ms of real time.
        serial: true,
        kind: Kind::Visual,
        sources: &[
            "src/activity.rs",
            "src/activity_panel.rs",
            "src/right_sidebar.rs",
            "src/status.rs",
            "src/command_name.rs",
            "src/workspace_command.rs",
            "src/reduce_motion.rs",
        ],
    },
];

/// The GRAPH (M11w, note 74), in both themes, a clean repo: "No changes" on top, the GRAPH docked
/// at the room's bottom (2026-11-20): its header, then a merge (HEAD, `feat/x` and
/// `origin/feat/x`), its two parents on two lanes, their fork.
/// - `changes-graph`: the lanes in their colours, HEAD's ring, the pills; the GRAPH a third of the
///   room, at its bottom.
/// - `changes-graph-card`: the pointer on the merge: its card, with Open on GitHub.
/// - `changes-graph-closed`: closed, its header alone at the room's bottom.
///
/// Acting (Mocha): its divider dragged to the room's top holds it at half, to its bottom at a
/// quarter.
fn changes_graph_scenes() {
    use crate::changes_panel::{GIT_INDENT, GIT_ROW as ROW};
    use crate::git_graph::{Commit, Details, Ref};
    use crate::git_status::ChangeList;
    use crate::graph_panel::{GRAPH_DEFAULT, GRAPH_MAX, GRAPH_MIN};
    use crate::right_sidebar::Panel;
    use alacritty_terminal::vte::ansi::Rgb;
    use std::cell::RefCell;
    use std::rc::Rc;

    let dims = size(px(1100.0), px(560.0));
    let window_right = 1100.0_f32;
    let sidebar_left = right_panel_left(window_right, 300.0);
    let root = files_fixture();
    let commit = |hash: &str, parents: &[&str], subject: &str, refs: Vec<Ref>, head: bool| Commit {
        hash: hash.to_string(),
        parents: parents.iter().map(|p| p.to_string()).collect(),
        author: "Andri Ferinata".to_string(),
        time: 1_794_455_520,
        refs,
        subject: subject.to_string(),
        head,
    };
    let commits = vec![
        commit(
            "aaaaaaa1",
            &["bbbbbbb2", "ccccccc3"],
            "Merge pull request #786 from acme/release",
            vec![
                Ref::Head("feat/x".into()),
                Ref::Remote("origin/feat/x".into()),
            ],
            true,
        ),
        commit(
            "bbbbbbb2",
            &["ddddddd4"],
            "Complete the swagger information",
            vec![],
            false,
        ),
        commit(
            "ccccccc3",
            &["ddddddd4"],
            "Conductor: auto flow until qa agent",
            vec![],
            false,
        ),
        commit(
            "ddddddd4",
            &[],
            "chore(release): 1.192.0",
            vec![Ref::Tag("v1.192.0".into())],
            false,
        ),
    ];
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let ui = theme.ui();
        let inks = crate::graph_panel::lane_inks(&theme);
        for (scene, card, open) in [
            ("changes-graph", false, true),
            ("changes-graph-card", true, true),
            ("changes-graph-closed", false, false),
        ] {
            let n = format!("{scene}-{slug}");
            let slot: Slot = Rc::new(RefCell::new(None));
            let build = {
                let (root, commits) = (root.clone(), commits.clone());
                right_panel_workspace(
                    theme,
                    root.clone(),
                    300.0,
                    Panel::Changes,
                    slot.clone(),
                    move |ws| {
                        let changes = &mut ws.right_sidebar.changes;
                        changes.show_for_shots(root.clone(), true, Some(ChangeList::default()));
                        // GRAPH starts closed (M11x, your call): these scenes open it.
                        changes.graph.open = open;
                        changes.graph.apply(
                            "shot".into(),
                            commits.clone(),
                            Some("git@github.com:acme/api.git".into()),
                        );
                        if card {
                            changes.graph.hover = Some((0, true));
                            changes.graph.details = Some((
                            "aaaaaaa1".into(),
                            Some(Details {
                                body: "Merge pull request #786 from acme/release\n\nComplete the swagger information".into(),
                                files: 9,
                                insertions: 774,
                                deletions: 65,
                            }),
                        ));
                        }
                    },
                )
            };
            // The room the changes list and the GRAPH share, as drawn: (top, height).
            let room: Rc<std::cell::Cell<(f32, f32)>> = Rc::default();
            let seen = room.clone();
            let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                cx.update_window(window, |_, window, _| window.refresh())
                    .expect("refresh");
                cx.run_until_parked();
                let ws = slotted(&slot);
                let b = cx
                    .read(|cx| ws.read(cx).right_sidebar.changes.graph.room.get())
                    .expect("the room was drawn");
                seen.set((f32::from(b.top()), f32::from(b.size.height)));
                slot.borrow_mut().take();
            };
            let path = shoot_with(scene, slug, &theme, dims, build.clone(), act.clone());
            println!("ui-shots: {}", path.display());
            let shot = render_with(&theme, dims, build, act);
            let near = |p: [u8; 3], c: Rgb, by: u8| {
                p.iter()
                    .zip([c.r, c.g, c.b])
                    .all(|(x, y)| x.abs_diff(y) <= by)
            };
            let has = |x0: f32, x1: f32, y0: f32, y1: f32, c: Rgb, by: u8| {
                let (w, h) = (((x1 - x0) * 2.0) as usize, ((y1 - y0) * 2.0) as usize);
                (0..h).any(|dy| {
                    (0..w).any(|dx| {
                        near(
                            pixel_at(&shot, dims, x0 + dx as f32 * 0.5, y0 + dy as f32 * 0.5),
                            c,
                            by,
                        )
                    })
                })
            };
            let (room_top, room_h) = room.get();
            let room_bottom = room_top + room_h;
            // Open: a third of the room at its bottom (its 1 pt top edge, then its header); closed,
            // its header alone.
            let section_top = if open {
                room_bottom - room_h * GRAPH_DEFAULT
            } else {
                room_bottom - ROW - 1.0
            };
            let edge = (0..8).map(|k| section_top - 2.0 + k as f32 * 0.5).any(|y| {
                near(
                    pixel_at(&shot, dims, sidebar_left + 150.0, y),
                    ui.divider,
                    6,
                )
            });
            assert!(
                edge,
                "{n}: the GRAPH's top edge at {section_top} (room {room_top}..{room_bottom})"
            );
            assert!(
                has(
                    sidebar_left + 12.0,
                    sidebar_left + 290.0,
                    section_top + 1.0,
                    section_top + 1.0 + ROW,
                    ui.text_faint,
                    16
                ),
                "{n}: the GRAPH header"
            );
            if !open {
                continue;
            }
            // Its commits under its header; the lanes in the first 24 pt past the rows' indent.
            let commits_top = section_top + 1.0 + ROW;
            let lanes = |c: Rgb| {
                has(
                    sidebar_left + 12.0 + GIT_INDENT,
                    sidebar_left + 36.0 + GIT_INDENT,
                    commits_top,
                    // (Its list shows what fits above the room's bottom.)
                    (commits_top + 4.0 * ROW).min(room_bottom - 1.0),
                    c,
                    16,
                )
            };
            assert!(lanes(inks[0]), "{n}: the first lane's colour");
            assert!(lanes(inks[1]), "{n}: the merged lane's colour");
            // The card: to the left of the panel, on `panel`, with its accent link.
            let card_area = |c: Rgb| {
                has(
                    sidebar_left - 300.0,
                    sidebar_left - 20.0,
                    (commits_top - 200.0).max(0.0),
                    (commits_top + 200.0).min(559.0),
                    c,
                    6,
                )
            };
            assert_eq!(card_area(ui.accent), card, "{n}: the card's links");
        }
    }

    // Acting (Mocha): the divider dragged past the room's top and bottom holds at its bounds.
    let theme = shot_theme("Catppuccin Mocha");
    let slot: Slot = Rc::new(RefCell::new(None));
    let build = right_panel_workspace(theme, root.clone(), 300.0, Panel::Changes, slot.clone(), {
        let root = root.clone();
        move |ws| {
            let changes = &mut ws.right_sidebar.changes;
            changes.show_for_shots(root.clone(), true, Some(ChangeList::default()));
            changes.graph.open = true;
        }
    });
    let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
        cx.update_window(window, |_, window, _| window.refresh())
            .expect("refresh");
        cx.run_until_parked();
        let ws = slotted(&slot);
        let room = cx
            .read(|cx| ws.read(cx).right_sidebar.changes.graph.room.get())
            .expect("the room was drawn");
        let fraction = |cx: &mut VisualTestAppContext| {
            cx.read(|cx| ws.read(cx).right_sidebar.changes.graph.fraction)
        };
        for (y, want) in [
            (f32::from(room.top()) - 50.0, GRAPH_MAX),
            (f32::from(room.bottom()) + 50.0, GRAPH_MIN),
        ] {
            cx.update(|cx| ws.update(cx, |ws, cx| ws.resize_graph(y, cx)));
            assert_eq!(fraction(cx), want, "changes-graph drag to {y}");
        }
        slot.borrow_mut().take();
    };
    render_with(&theme, dims, build, act);
}

/// Sticky headers (your ask: "make other accordion sticky"): 30 changed files, the list scrolled 8
/// rows down: CHANGES' header stays pinned over the list's top, its hairline under it. (The GRAPH
/// has its own list, docked at the bottom, since 2026-11-20; closed here, so the list has room.)
fn changes_sticky_scenes() {
    use crate::changes_panel::GIT_ROW as ROW;
    use crate::git_status::{Change, ChangeList, Counts, Mark};
    use crate::right_sidebar::Panel;
    use crate::ui_tokens::size::{HAIRLINE, TAB_BAR};
    use alacritty_terminal::vte::ansi::Rgb;
    use std::cell::RefCell;
    use std::rc::Rc;

    let dims = size(px(1100.0), px(560.0));
    let window_right = 1100.0_f32;
    let sidebar_left = right_panel_left(window_right, 300.0);
    let list_top = CARD_INSET + HAIRLINE + TAB_BAR + crate::changes_panel::HEADER;
    let root = files_fixture();
    let list = ChangeList {
        entries: (0..30)
            .map(|i| Change {
                path: std::path::PathBuf::from(format!("src/file_{i:02}.rs")),
                mark: Mark::Modified,
                counts: Counts::Lines {
                    added: i + 1,
                    removed: 1,
                },
                folder: false,
                staged: false,
                untracked: false,
            })
            .collect(),
        more: 0,
        counts_failed: false,
    };
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let ui = theme.ui();
        let n = format!("changes-sticky-{slug}");
        let slot: Slot = Rc::new(RefCell::new(None));
        let build = {
            let (root, list) = (root.clone(), list.clone());
            right_panel_workspace(
                theme,
                root.clone(),
                300.0,
                Panel::Changes,
                slot.clone(),
                move |ws| {
                    let changes = &mut ws.right_sidebar.changes;
                    changes.show_for_shots(root.clone(), true, Some(list.clone()));
                },
            )
        };
        let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
            cx.update_window(window, |_, window, _| window.refresh())
                .expect("refresh");
            cx.run_until_parked();
            let ws = slotted(&slot);
            cx.update_window(window, |_, window, cx| {
                ws.update(cx, |ws, _| {
                    ws.right_sidebar
                        .changes
                        .scroll_offset_for_shots(point(px(0.0), px(-8.0 * ROW)))
                });
                window.refresh();
            })
            .expect("scroll");
            cx.run_until_parked();
            cx.update_window(window, |_, window, _| window.refresh())
                .expect("refresh");
            cx.run_until_parked();
            slot.borrow_mut().take();
        };
        let path = shoot_with(
            "changes-sticky",
            slug,
            &theme,
            dims,
            build.clone(),
            act.clone(),
        );
        println!("ui-shots: {}", path.display());
        let shot = render_with(&theme, dims, build, act);
        let near = |p: [u8; 3], c: Rgb, by: u8| {
            p.iter()
                .zip([c.r, c.g, c.b])
                .all(|(x, y)| x.abs_diff(y) <= by)
        };
        let has = |x0: f32, x1: f32, y0: f32, y1: f32, c: Rgb, by: u8| {
            let (w, h) = (((x1 - x0) * 2.0) as usize, ((y1 - y0) * 2.0) as usize);
            (0..h).any(|dy| {
                (0..w).any(|dx| {
                    near(
                        pixel_at(&shot, dims, x0 + dx as f32 * 0.5, y0 + dy as f32 * 0.5),
                        c,
                        by,
                    )
                })
            })
        };
        // The pinned header in the list's first row, though its row has scrolled away: the
        // `CHANGES` words (faint) where an entry's name would be; no entry's mark letter (an
        // `M` in its blue) in that row; the header's hairline under it.
        let first = (list_top + 2.0, list_top + ROW - 2.0);
        assert!(
            has(
                sidebar_left + 38.0,
                sidebar_left + 100.0,
                first.0,
                first.1,
                ui.text_faint,
                16
            ),
            "{n}: CHANGES' header is pinned at the top"
        );
        let m = crate::right_sidebar::mark_color(Mark::Modified, &theme);
        assert!(
            !has(
                sidebar_left + 50.0,
                sidebar_left + 70.0,
                first.0,
                first.1,
                m,
                24
            ),
            "{n}: no entry under the pinned header's words"
        );
        assert!(
            has(
                sidebar_left + 20.0,
                sidebar_left + 280.0,
                list_top + ROW - 1.0,
                list_top + ROW + 1.0,
                ui.divider,
                6
            ),
            "{n}: the pinned header's hairline"
        );
        // And the rows under it are entries (scrolled: its own row is gone).
        assert!(
            has(
                sidebar_left + 50.0,
                sidebar_left + 70.0,
                list_top + ROW + 2.0,
                list_top + 2.0 * ROW - 2.0,
                m,
                24
            ),
            "{n}: an entry right under the pinned header"
        );
    }
}

/// The Changes tab grouped (M11v part 2, layout C, note 73), in both themes:
/// - `changes-staging-mixed`: STAGED (2, green ✓ circles) over CHANGES (4, empty circles);
///   `2 of 6 staged` with a green third of the bar; the pointer on `12 - Scope.md`'s row: ↺
///   where its counts were.
/// - `changes-staging-dialog`: the revert dialog over the tab, its red Revert button.
/// - `changes-staging-all`: everything staged: the header's circle filled, no CHANGES divider.
fn changes_staging_scenes() {
    use crate::git_status::{Change, ChangeList, Counts, Mark};
    use crate::right_sidebar::{EDGE, Panel};
    use crate::ui_tokens::size::{HAIRLINE, TAB_BAR};
    use alacritty_terminal::vte::ansi::Rgb;
    use gpui::Modifiers;
    use std::cell::RefCell;
    use std::rc::Rc;

    let dims = size(px(1100.0), px(560.0));
    let window_right = 1100.0_f32;
    let sidebar_left = right_panel_left(window_right, 300.0);
    let sidebar_right = right_panel_right(window_right);
    let header_top = CARD_INSET + HAIRLINE + TAB_BAR;
    let list_top = header_top + crate::changes_panel::HEADER;
    let root = files_fixture();
    let lines = |added, removed| Counts::Lines { added, removed };
    let entry = |path: &str, mark, counts, staged| Change {
        path: PathBuf::from(path),
        mark,
        counts,
        folder: false,
        staged,
        untracked: mark == Mark::Added && !staged,
    };
    let mixed = ChangeList {
        entries: vec![
            entry(
                "docs/73 - Milestone 11v.md",
                Mark::Added,
                lines(119, 0),
                true,
            ),
            entry("docs/M11v Plan.md", Mark::Added, lines(98, 0), true),
            entry(".impeccable/config.json", Mark::Added, lines(62, 0), false),
            entry("docs/12 - Scope.md", Mark::Modified, lines(7, 0), false),
            entry(
                "src/changes_panel.rs",
                Mark::Modified,
                lines(544, 20),
                false,
            ),
            entry("src/sidebar.rs", Mark::Modified, lines(6, 84), false),
        ],
        more: 0,
        counts_failed: false,
    };
    let all = ChangeList {
        entries: mixed.entries[..2].to_vec(),
        ..mixed.clone()
    };
    // Rows: 0 STAGED, 1–2 staged, 3 CHANGES, 4–7 the rest; Scope.md is row 5 (entry 3).
    // The Git tab's rows (2026-11-20: 24 pt, indented `GIT_INDENT`).
    use crate::changes_panel::{GIT_INDENT, GIT_ROW as ROW};
    let row_top = |r: usize| list_top + r as f32 * ROW;
    // A row's circle: from the content edge (`PILL_INSET` + `PILL_PAD`) and the indent, 12 pt.
    let circle_left = sidebar_left + 22.0 + GIT_INDENT;
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let ui = theme.ui();
        let (green, _) = crate::changes_panel::count_inks(&theme);
        let (_, _, red) = crate::changes_panel::bar_inks(&theme);
        for state in ["mixed", "dialog", "all"] {
            let n = format!("changes-staging-{state}-{slug}");
            let list = if state == "all" {
                all.clone()
            } else {
                mixed.clone()
            };
            let slot: Slot = Rc::new(RefCell::new(None));
            let build = {
                let root = root.clone();
                right_panel_workspace(
                    theme,
                    root.clone(),
                    300.0,
                    Panel::Changes,
                    slot.clone(),
                    move |ws| {
                        let changes = &mut ws.right_sidebar.changes;
                        changes.show_for_shots(root.clone(), true, Some(list.clone()));
                        if state == "dialog" {
                            changes.ask_revert_for_shots(3);
                        }
                    },
                )
            };
            let n_act = n.clone();
            let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                cx.update_window(window, |_, window, _| window.refresh())
                    .expect("refresh");
                cx.run_until_parked();
                if state == "dialog" {
                    // Review: a click outside the dialog, over a staged row's circle, only
                    // closes it: nothing under the dim layer is clicked. (Not for the shot: the
                    // dialog opens again after.)
                    let ws = slotted(&slot);
                    let on = point(px(circle_left + 6.0), px(row_top(1) + ROW / 2.0));
                    cx.simulate_click(window, on, Modifiers::none());
                    cx.run_until_parked();
                    let (open, working) = cx.read(|cx| {
                        ws.read(cx)
                            .right_sidebar
                            .changes
                            .dialog_and_work_for_shots()
                    });
                    assert!(!open, "{n_act}: a click outside closes the dialog");
                    assert!(!working, "{n_act}: and clicks nothing under it");
                    cx.update_window(window, |_, _, cx| {
                        ws.update(cx, |ws, _| ws.right_sidebar.changes.ask_revert_for_shots(3))
                    })
                    .expect("open it again");
                    cx.update_window(window, |_, window, _| window.refresh())
                        .expect("refresh");
                    cx.run_until_parked();
                }
                if state == "mixed" {
                    let on = point(px(sidebar_left + 150.0), px(row_top(5) + ROW / 2.0));
                    cx.simulate_mouse_move(window, on, None, Modifiers::none());
                    cx.update_window(window, |_, window, _| window.refresh())
                        .expect("refresh");
                    cx.run_until_parked();
                }
                slot.borrow_mut().take();
            };
            let path = shoot_with(
                "changes-staging",
                &format!("{state}-{slug}"),
                &theme,
                dims,
                build.clone(),
                act.clone(),
            );
            println!("ui-shots: {}", path.display());
            let shot = render_with(&theme, dims, build, act);
            let near = |p: [u8; 3], c: Rgb, by: u8| {
                p.iter()
                    .zip([c.r, c.g, c.b])
                    .all(|(x, y)| x.abs_diff(y) <= by)
            };
            let has = |x0: f32, x1: f32, y0: f32, y1: f32, c: Rgb, by: u8| {
                let (w, h) = (((x1 - x0) * 2.0) as usize, ((y1 - y0) * 2.0) as usize);
                (0..h).any(|dy| {
                    (0..w).any(|dx| {
                        near(
                            pixel_at(&shot, dims, x0 + dx as f32 * 0.5, y0 + dy as f32 * 0.5),
                            c,
                            by,
                        )
                    })
                })
            };
            let circle_green = |r: usize| {
                has(
                    circle_left - 2.0,
                    circle_left + 14.0,
                    row_top(r) + 2.0,
                    row_top(r) + ROW - 2.0,
                    green,
                    16,
                )
            };
            match state {
                "mixed" => {
                    assert!(
                        circle_green(1) && circle_green(2),
                        "{n}: staged rows' circles are green"
                    );
                    for r in 4..8 {
                        assert!(!circle_green(r), "{n}: row {r}'s circle is empty");
                    }
                    // The bar's green third, then its track: in the plain header row (the
                    // designer's redesign), its line (20), a 5 pt gap and the bar (3) centred.
                    let card_mid = header_top + crate::changes_panel::HEADER / 2.0;
                    let bar_y = card_mid + 12.5;
                    let inner = sidebar_left + EDGE;
                    assert!(
                        has(inner, inner + 46.0, bar_y - 3.0, bar_y + 3.0, green, 16),
                        "{n}: the progress bar's green share"
                    );
                    // The hovered row's ↺ replaced its `+7`.
                    assert!(
                        !has(
                            sidebar_right - 70.0,
                            sidebar_right - 4.0,
                            row_top(5) + 2.0,
                            row_top(5) + ROW - 2.0,
                            green,
                            16
                        ),
                        "{n}: the hovered row's counts are hidden"
                    );
                    assert!(
                        has(
                            sidebar_right - 32.0,
                            sidebar_right - 6.0,
                            row_top(5) + 2.0,
                            row_top(5) + ROW - 2.0,
                            ui.text_dim,
                            12
                        ),
                        "{n}: ↺ shows on the hovered row"
                    );
                    assert!(
                        has(
                            sidebar_right - 70.0,
                            sidebar_right - 4.0,
                            row_top(6) + 2.0,
                            row_top(6) + ROW - 2.0,
                            green,
                            16
                        ),
                        "{n}: the next row's counts show"
                    );
                }
                "dialog" => {
                    assert!(
                        has(
                            sidebar_left + 20.0,
                            sidebar_right - 10.0,
                            150.0,
                            420.0,
                            red,
                            12
                        ),
                        "{n}: the red Revert button"
                    );
                }
                _ => {
                    // All staged: the header's circle is filled.
                    let inner = sidebar_left + EDGE;
                    let line_top = header_top + (crate::changes_panel::HEADER - 28.0) / 2.0;
                    assert!(
                        has(inner, inner + 16.0, line_top, line_top + 20.0, green, 16),
                        "{n}: the header's circle is filled"
                    );
                }
            }
        }
    }
}

/// The Changes tab's branch bar (M11v, note 73), in each state, in both themes:
/// - `changes-bar-even`: up to date: the green line, ↻ only (no accent button).
/// - `changes-bar-behind`: ↓2 behind: the accent Pull button; its tooltip "Pull 2 commits from
///   origin" above it.
/// - `changes-bar-ahead`, `changes-bar-diverged` (no accent button), `changes-bar-local`
///   (Publish), `changes-bar-working` (dimmed, "Pulling 2 commits…"), `changes-bar-failed`
///   (git's line in red).
fn changes_bar_scenes() {
    use crate::branch_list::Head;
    use crate::branch_sync::{Action, Sync, Upstream};
    use crate::git_status::{Change, ChangeList, Counts, Mark};
    use crate::right_sidebar::Panel;
    use alacritty_terminal::vte::ansi::Rgb;
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::time::{Duration, SystemTime};

    let dims = size(px(1100.0), px(560.0));
    let window_right = 1100.0_f32;
    let sidebar_left = right_panel_left(window_right, 300.0);
    let sidebar_right = right_panel_right(window_right);
    let root = files_fixture();
    let list = ChangeList {
        entries: vec![Change {
            path: "CLAUDE.md".into(),
            mark: Mark::Modified,
            counts: Counts::Lines {
                added: 1,
                removed: 1,
            },
            folder: false,
            staged: false,
            untracked: false,
        }],
        more: 0,
        counts_failed: false,
    };
    let at = |upstream, has_origin, head: Head| Sync {
        head,
        upstream,
        has_origin,
        fetched: Some(SystemTime::now() - Duration::from_secs(180)),
        base: None,
    };
    let branch = || Head::Branch("fix/hdw-approval-flow".into());
    let track = |ahead, behind| Upstream::Track { ahead, behind };
    // (slug, sync, busy, error, tooltip shown, an action button beside Fetch expected)
    let states: Vec<(&str, Sync, Option<Action>, Option<String>, bool, bool)> = vec![
        (
            "even",
            at(track(0, 0), true, branch()),
            None,
            None,
            false,
            false,
        ),
        (
            "behind",
            at(track(0, 2), true, branch()),
            None,
            None,
            true,
            true,
        ),
        (
            "ahead",
            at(track(1, 0), true, branch()),
            None,
            None,
            false,
            true,
        ),
        (
            "diverged",
            at(track(1, 2), true, branch()),
            None,
            None,
            false,
            false,
        ),
        (
            "local",
            at(Upstream::None, true, Head::Branch("feat/breadcrumb".into())),
            None,
            None,
            false,
            true,
        ),
        (
            "working",
            at(track(0, 2), true, branch()),
            Some(Action::Pull(2)),
            None,
            false,
            true,
        ),
        (
            "failed",
            at(track(0, 2), true, branch()),
            None,
            Some(
                "Pull failed: Your local changes to the following files would be overwritten"
                    .into(),
            ),
            false,
            true,
        ),
    ];
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let ui = theme.ui();
        for (state, sync, busy, error, tip, accent) in states.clone() {
            let n = format!("changes-bar-{state}-{slug}");
            let slot: Slot = Rc::new(RefCell::new(None));
            let build = {
                let (root, list) = (root.clone(), list.clone());
                right_panel_workspace(
                    theme,
                    root.clone(),
                    300.0,
                    Panel::Changes,
                    slot.clone(),
                    move |ws| {
                        ws.right_sidebar.changes.show_for_shots(
                            root.clone(),
                            true,
                            Some(list.clone()),
                        );
                        ws.right_sidebar.changes.show_bar_for_shots(
                            sync.clone(),
                            busy,
                            error.clone(),
                            tip,
                        );
                    },
                )
            };
            let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                cx.update_window(window, |_, window, _| window.refresh())
                    .expect("refresh");
                cx.run_until_parked();
                slot.borrow_mut().take();
            };
            let path = shoot_with(
                "changes-bar",
                &format!("{state}-{slug}"),
                &theme,
                dims,
                build.clone(),
                act.clone(),
            );
            println!("ui-shots: {}", path.display());
            let shot = render_with(&theme, dims, build, act);
            let near = |p: [u8; 3], c: Rgb, by: u8| {
                p.iter()
                    .zip([c.r, c.g, c.b])
                    .all(|(x, y)| x.abs_diff(y) <= by)
            };
            let has = |x0: f32, x1: f32, y0: f32, y1: f32, c: Rgb, by: u8| {
                let (w, h) = (((x1 - x0) * 2.0) as usize, ((y1 - y0) * 2.0) as usize);
                (0..h).any(|dy| {
                    (0..w).any(|dx| {
                        near(
                            pixel_at(&shot, dims, x0 + dx as f32 * 0.5, y0 + dy as f32 * 0.5),
                            c,
                            by,
                        )
                    })
                })
            };
            // The bar is the card's last stretch (to 532 in a 560 pt window): from the card's
            // bottom edge (`card_edge` since 2026-11-20) up to its top, the `card_border` line,
            // both found by scanning up the panel's left edge (M11al Task 10: the
            // bar is `BAR_PAD_Y` + its two lines or the 34 pt buttons + `BAR_PAD_Y`, taller than 56);
            // its buttons in its right 80 pt; the tooltip hangs just above them, where no row is.
            let probe = |y: f32, c: Rgb| near(pixel_at(&shot, dims, sidebar_left + 4.0, y), c, 3);
            let bar_bottom = (0..40)
                .map(|i| 540.0 - i as f32 * 0.5)
                .find(|&y| probe(y, ui.sidebar_background))
                .expect("the card's bottom edge");
            let bar_top = (0..460)
                .map(|i| bar_bottom - 2.0 - i as f32 * 0.5)
                .find(|&y| probe(y, ui.card_border))
                .expect("the bar's top line");
            assert!(
                bar_bottom - bar_top >= 63.0 - 1.0,
                "{n}: the bar is at least 2 x 14 + 34 pt tall plus its line: {bar_top}"
            );
            // M11al §6.6: the buttons are 34 pt squares in a `card_border` edge, the icons
            // `text_dim` (no accent fill any more); the one action's button is the second.
            // Count the edges along the bar's middle: two a button, `BAR_BUTTON` apart (34, less
            // the line's own width). The icon set's thin strokes (2026-11-20) pass through
            // `card_border`'s shade where they cross the line, so only runs with a partner a
            // button's width on count as edges. (Dimmed while it works: the edge blends with the
            // fill, so it isn't counted then.)
            let mid = (bar_top + bar_bottom) / 2.0;
            let mut starts = vec![];
            let mut inside = false;
            for i in 0..200 {
                let x = sidebar_right - 100.0 + i as f32 * 0.5;
                let on = near(pixel_at(&shot, dims, x, mid), ui.card_border, 3);
                if on && !inside {
                    starts.push(x);
                }
                inside = on;
            }
            let buttons: Vec<(f32, f32)> = starts
                .iter()
                .filter_map(|&a| {
                    let b = starts.iter().find(|&&b| (b - a - 33.0).abs() <= 1.0)?;
                    Some((a, *b))
                })
                .collect();
            if busy.is_none() {
                assert_eq!(
                    buttons.len(),
                    if accent { 2 } else { 1 },
                    "{n}: the buttons' edges, 34 pt apart ({starts:?})"
                );
            }
            assert!(
                !has(
                    sidebar_right - 100.0,
                    sidebar_right,
                    bar_top + 4.0,
                    bar_bottom - 4.0,
                    ui.accent,
                    10
                ),
                "{n}: no accent fill on the buttons"
            );
            let tip_shown = has(
                sidebar_right - 200.0,
                sidebar_right,
                bar_top - 24.0,
                bar_top,
                ui.text,
                24,
            );
            assert_eq!(tip_shown, tip, "{n}: the tooltip above the button");
            // The branch's name, in the text colour, at the bar's left.
            assert!(
                has(
                    sidebar_left + 8.0,
                    sidebar_left + 160.0,
                    bar_top,
                    bar_bottom,
                    ui.text,
                    24
                ),
                "{n}: the branch"
            );
        }
    }
}

/// The Changes panel (M11h Task 4) in the right sidebar, beside a terminal, in a 1100 × 560
/// window, in both themes, its list given (`show_for_shots`; nothing looked up):
/// - `changes`: six rows and 3 more for the fixture folder as a repo: src/main.rs (M, +12 −3),
///   src/new.rs (A, +4), old.rs (D, −9), docs/logo.png (A, binary), a file four folders down
///   (M, +2 −2) and a long name (A, not counted), then `… 3 more`. The header reads the repo's
///   name, `9 files` and the totals (+18 −14); each letter in its mark's colour (M blue, A
///   green, D red); `+12` in the added ink and `−3` in the removed one, a binary or uncounted
///   row in neither; the hovered row (the fifth) `side_hover`, the others `side`.
/// - `changes-empty-clean`: an empty list in a repo: the header (name only), then
///   "No changes since the last commit."; `changes-empty-norepo`: not a repo: no git tab in
///   the strip, the Files panel in place of Changes (your words: "hide git tab if the folder is
///   not git"); `changes-empty-ssh`: an SSH terminal followed: "Changes
///   aren't shown for SSH terminals.", no header.
/// - `changes-click`: a click on the M row opens src/main.rs in the focused pane in Diff mode
///   (the model's tab), and that row is drawn `side_selected`.
fn changes_scenes() {
    use crate::changes_panel::{GIT_INDENT, GIT_ROW as ROW};
    use crate::changes_panel::{HEADER, NO_CHANGES, NO_REPO, SSH_NOTE, totals};
    use crate::git_status::{Change, ChangeList, Counts, Mark};
    use crate::layout_model::{FileMode, TabKind};
    use crate::right_sidebar::{PILL_INSET, PILL_PAD, Panel, mark_color};
    use crate::ui_tokens::size::{HAIRLINE, TAB_BAR};
    use alacritty_terminal::vte::ansi::Rgb;
    use gpui::{Focusable, Modifiers};
    use std::cell::RefCell;
    use std::rc::Rc;

    let dims = size(px(1100.0), px(560.0));
    let window_right = 1100.0_f32;
    let sidebar_left = right_panel_left(window_right, 300.0);
    let sidebar_right = right_panel_right(window_right);
    let card_top = CARD_INSET + HAIRLINE;
    let header_top = card_top + TAB_BAR;
    let list_top = header_top + HEADER;
    // M11v part 2: the CHANGES divider takes the first row; the entries start under it.
    let rows_top = list_top + ROW;
    let root = files_fixture();
    let lines = |added, removed| Counts::Lines { added, removed };
    let change = |path: &str, mark, counts| Change {
        path: PathBuf::from(path),
        mark,
        counts,
        folder: false,
        staged: false,
        untracked: mark == Mark::Added,
    };
    let six = ChangeList {
        entries: vec![
            change("src/main.rs", Mark::Modified, lines(12, 3)),
            change("src/new.rs", Mark::Added, lines(4, 0)),
            change("old.rs", Mark::Deleted, lines(0, 9)),
            change("docs/logo.png", Mark::Added, Counts::Binary),
            change("src/ui/widgets/deep/panel.rs", Mark::Modified, lines(2, 2)),
            change(
                "a_very_long_file_name_that_goes_on_well_past_the_sidebar_edge.rs",
                Mark::Added,
                Counts::Unknown,
            ),
        ],
        more: 3,
        counts_failed: false,
    };
    let frame = |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
        cx.update_window(window, |_, window, _| window.refresh())
            .expect("refresh");
        cx.run_until_parked();
    };
    let row_mid = |i: usize| rows_top + (i as f32 + 0.5) * ROW;
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let ui = theme.ui();
        let (added, removed) = crate::changes_panel::count_inks(&theme);
        let (m, a, d) = (
            mark_color(Mark::Modified, &theme),
            mark_color(Mark::Added, &theme),
            mark_color(Mark::Deleted, &theme),
        );
        let near = |p: [u8; 3], c: Rgb, by: u8| {
            p.iter()
                .zip([c.r, c.g, c.b])
                .all(|(x, y)| x.abs_diff(y) <= by)
        };
        let build_for = |list: Option<ChangeList>, in_repo: bool, ssh: bool, slot: Slot| {
            let root_for = root.clone();
            right_panel_workspace(
                theme,
                root.clone(),
                300.0,
                Panel::Changes,
                slot,
                move |ws| {
                    ws.right_sidebar.changes.show_for_shots(
                        root_for.clone(),
                        in_repo,
                        list.clone(),
                    );
                    if ssh {
                        let i = ws.sessions.position(|s| s.label() == "api").expect("api");
                        ws.sessions.get_mut(i).expect("opened").ssh =
                            Some(crate::ssh_target::SshTarget {
                                host: "devbox".into(),
                                tmux: false,
                                tmux_name: None,
                            });
                    }
                },
            )
        };

        // The list.
        let n = format!("changes-{slug}");
        let slot: Slot = Rc::new(RefCell::new(None));
        let build = build_for(Some(six.clone()), true, false, slot.clone());
        let act = {
            let n = n.clone();
            move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let ws = slotted(&slot);
                frame(cx, window);
                let shown = cx.read(|cx| ws.read(cx).changes_note(cx));
                assert_eq!(shown, None, "{n}: rows, no note");
                // The pointer on the fifth row: its hover fill in the shot.
                let on = point(px(sidebar_left + 150.0), px(row_mid(4)));
                cx.simulate_mouse_move(window, on, None, Modifiers::none());
                frame(cx, window);
                slot.borrow_mut().take();
            }
        };
        let path = shoot_with("changes", slug, &theme, dims, build.clone(), act.clone());
        println!("ui-shots: {}", path.display());
        let shot = render_with(&theme, dims, build, act);
        let at = |x: f32, y: f32| pixel_at(&shot, dims, x, y);
        let has = |x0: f32, x1: f32, y0: f32, y1: f32, c: Rgb, by: u8| {
            let (w, h) = (((x1 - x0) * 2.0) as usize, ((y1 - y0) * 2.0) as usize);
            (0..h).any(|dy| {
                (0..w).any(|dx| near(at(x0 + dx as f32 * 0.5, y0 + dy as f32 * 0.5), c, by))
            })
        };
        let in_row = |i: usize, x0: f32, x1: f32, c: Rgb| {
            let y0 = rows_top + i as f32 * ROW;
            has(x0, x1, y0 + 2.0, y0 + ROW - 2.0, c, 24)
        };
        // The header (M11v part 2): the stage-all circle, the repo's name, `0 of 9 staged` at
        // the right (faint), and under them the progress bar's track.
        assert_eq!(totals(&six), (18, 14), "{n}: the totals");
        assert_eq!(crate::git_stage::staged_summary(&six), (0, 9));
        // In the plain header row (the designer's redesign): the line (20), a 5 pt gap and the
        // bar (3) centred; its words on the 12 pt edge.
        let line_top = header_top + (HEADER - 28.0) / 2.0;
        let inset = crate::right_sidebar::EDGE;
        let (left, right) = (sidebar_left + inset - 20.0, sidebar_right - inset);
        assert!(
            has(
                left + 20.0,
                left + 140.0,
                line_top,
                line_top + 20.0,
                ui.text,
                24
            ),
            "{n}: the repo's name"
        );
        assert!(
            has(
                right - 110.0,
                right,
                line_top,
                line_top + 20.0,
                ui.text_faint,
                24
            ),
            "{n}: `0 of 9 staged`, faint, at the right"
        );
        assert!(
            has(
                left + 40.0,
                right - 40.0,
                line_top + 25.0,
                line_top + 28.0,
                ui.progress_track,
                6
            ),
            "{n}: the progress bar's track"
        );
        // Each row's letter in its mark's colour, and in no other mark's.
        // Part 2: after the row's circle (12 pt since 2026-11-20, and a 6 pt gap) from its
        // content edge (`PILL_INSET` + `PILL_PAD`, M11al §6.5, and the Git tab's indent).
        let content = sidebar_left + PILL_INSET + PILL_PAD + GIT_INDENT;
        let letter = |i: usize, c: Rgb| in_row(i, content + 18.0, content + 30.0, c);
        for (i, mark) in [(0, m), (1, a), (2, d), (3, a), (4, m), (5, a)] {
            assert!(letter(i, mark), "{n}: row {i}'s letter in {mark:?}");
            for other in [m, a, d].into_iter().filter(|o| *o != mark) {
                assert!(!letter(i, other), "{n}: row {i}'s letter isn't {other:?}");
            }
        }
        // The counts at the right: `+12 −3` in both inks; `+4` added only; `−9` removed
        // only; `binary` faint in neither; an uncounted row has none.
        let counts =
            |i: usize, c: Rgb| in_row(i, sidebar_right - 8.0 - 60.0, sidebar_right - 8.0, c);
        for (i, want_added, want_removed) in [
            (0, true, true),
            (1, true, false),
            (2, false, true),
            (3, false, false),
            (5, false, false),
        ] {
            assert_eq!(counts(i, added), want_added, "{n}: row {i}'s added count");
            assert_eq!(
                counts(i, removed),
                want_removed,
                "{n}: row {i}'s removed count"
            );
        }
        assert!(counts(3, ui.text_faint), "{n}: `binary`, faint");
        // `… 3 more`, faint, under the names, and nothing after it: the GRAPH is docked at the
        // room's bottom since 2026-11-20 (`changes-graph` checks it there).
        let more = |i: usize, c: Rgb| in_row(i, content + 50.0, content + 140.0, c);
        assert!(more(6, ui.text_faint), "{n}: the more row");
        assert!(!more(7, ui.text_faint), "{n}: nothing after the more row");
        // The hovered row filled `side_hover`, the one above it the panel's `side` (part 4: a
        // pill `PILL_INSET` in from the side, so read 4 pt inside it; M11al §6.5).
        let fill = |i: usize| at(sidebar_left + PILL_INSET + 4.0, row_mid(i));
        assert!(
            near(fill(4), ui.side_hover, 3),
            "{n}: hovered {:?}",
            fill(4)
        );
        assert!(near(fill(3), ui.side, 3), "{n}: not hovered {:?}", fill(3));

        // The empty states.
        for (case, list, in_repo, ssh, note) in [
            (
                "clean",
                Some(ChangeList::default()),
                true,
                false,
                NO_CHANGES,
            ),
            ("norepo", None, false, false, NO_REPO),
            ("ssh", Some(six.clone()), true, true, SSH_NOTE),
        ] {
            let scene = format!("changes-empty-{case}");
            let n = format!("{scene}-{slug}");
            let slot: Slot = Rc::new(RefCell::new(None));
            let build = build_for(list, in_repo, ssh, slot.clone());
            let act = {
                let n = n.clone();
                move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                    let ws = slotted(&slot);
                    frame(cx, window);
                    let shown = cx.read(|cx| ws.read(cx).changes_note(cx));
                    assert_eq!(shown, Some(note), "{n}");
                    if case == "norepo" {
                        use crate::right_sidebar::Panel;
                        let (tabs, panel) = cx.read(|cx| ws.read(cx).right_panels_shown());
                        assert_eq!(tabs, vec![Panel::Files, Panel::Activity], "{n}: no git tab");
                        assert_eq!(panel, Panel::Files, "{n}: Files in place of Changes");
                    }
                    slot.borrow_mut().take();
                }
            };
            let path = shoot_with(&scene, slug, &theme, dims, build.clone(), act.clone());
            println!("ui-shots: {}", path.display());
            if case == "norepo" {
                // The Files panel shows: its own scenes check how it looks.
                continue;
            }
            let shot = render_with(&theme, dims, build, act);
            let at = |x: f32, y: f32| pixel_at(&shot, dims, x, y);
            // Tight: Latte's dim and full text colours are close (6 apart in a channel since the
            // dark-panel chore, so 3).
            let has = |y0: f32, y1: f32, c: Rgb| {
                (0..((y1 - y0) * 2.0) as usize).any(|dy| {
                    (0..400).any(|dx| {
                        let (x, y) = (sidebar_left + 2.0 + dx as f32 * 0.5, y0 + dy as f32 * 0.5);
                        near(at(x, y), c, 3)
                    })
                })
            };
            // The header (in a repo only), then the note's words, dim.
            let header = case == "clean";
            assert_eq!(
                has(header_top, header_top + 30.0, ui.text),
                header,
                "{n}: the header {}",
                if header { "shows" } else { "doesn't" }
            );
            let note_top = if header { list_top } else { header_top };
            assert!(
                has(note_top, note_top + 30.0, ui.text_dim),
                "{n}: the note's words"
            );
            // No rows: no mark letters.
            for c in [m, a, d] {
                assert!(
                    !has(note_top + 30.0, note_top + 120.0, c),
                    "{n}: no rows ({c:?})"
                );
            }
        }

        // A click on the M row: its diff, selected.
        let n = format!("changes-click-{slug}");
        let slot: Slot = Rc::new(RefCell::new(None));
        let build = build_for(Some(six.clone()), true, false, slot.clone());
        let act = {
            let (n, root) = (n.clone(), root.clone());
            move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let ws = slotted(&slot);
                frame(cx, window);
                let on = point(px(sidebar_left + 100.0), px(row_mid(0)));
                cx.simulate_click(window, on, Modifiers::none());
                frame(cx, window);
                cx.read(|cx| {
                    let ws = ws.read(cx);
                    let tab = ws.model.focused_tab().expect("a tab");
                    let kind = ws.model.tab(tab).expect("known").kind.clone();
                    assert_eq!(
                        kind,
                        TabKind::File {
                            path: root.join("src/main.rs"),
                            mode: FileMode::Diff
                        },
                        "{n}: the click opens src/main.rs's diff"
                    );
                    let view = ws.focused_file_tab().expect("its view");
                    assert_eq!(view.read(cx).mode(), FileMode::Diff, "{n}: the tab's mode");
                });
                let keys = cx
                    .update_window(window, |_, window, cx| {
                        let view = ws.read(cx).focused_file_tab().expect("its view");
                        view.focus_handle(cx).is_focused(window)
                    })
                    .expect("read");
                assert!(keys, "{n}: the diff tab has the keyboard");
                // The pointer away: no hover or tooltip in the shot.
                cx.simulate_mouse_move(
                    window,
                    point(px(500.0), px(300.0)),
                    None,
                    Modifiers::none(),
                );
                frame(cx, window);
                slot.borrow_mut().take();
            }
        };
        let path = shoot_with(
            "changes-click",
            slug,
            &theme,
            dims,
            build.clone(),
            act.clone(),
        );
        println!("ui-shots: {}", path.display());
        let shot = render_with(&theme, dims, build, act);
        // Part 4: the selection is a pill `PILL_INSET` in from the side: read 4 pt inside it.
        let fill = |i: usize| {
            pixel_at(
                &shot,
                dims,
                sidebar_left + crate::right_sidebar::PILL_INSET + 4.0,
                row_mid(i),
            )
        };
        assert!(
            near(fill(0), ui.side_selected, 3),
            "{n}: the M row is selected: {:?}",
            fill(0)
        );
        assert!(
            near(fill(1), ui.side, 3),
            "{n}: the next isn't: {:?}",
            fill(1)
        );
    }
}

/// A screen asking a question one line long on the terminal, wrapped in the Activity card.
const ASKING_TEXT: &str =
    "~/code/api $ ./deploy\r\nRun the database migration on production now? (y/n) ";

/// The Activity panel's window (M11h Task 6): the right sidebar open on Activity, 300 pt wide,
/// with terminals `sessions` (name, screen; none: no terminal to follow), the first focused;
/// Reduce motion as `still`.
fn activity_workspace(
    theme: Theme,
    sessions: &'static [(&'static str, &'static str)],
    still: bool,
    slot: Slot,
) -> impl FnOnce(&mut Window, &mut App) -> AnyView + Clone + 'static {
    move |window: &mut Window, cx: &mut App| -> AnyView {
        cx.set_global(shot_appearance(theme, cx));
        cx.bind_keys(crate::key_bindings());
        cx.set_reduce_motion(still);
        let ws = cx.new(|cx| {
            let mut ws = Workspace::for_shots(window, cx);
            ws.sidebar.hint_dismissed = true;
            ws.right_sidebar.hidden = false;
            ws.right_sidebar.width = 300.0;
            ws.right_sidebar.panel = crate::right_sidebar::Panel::Activity;
            for (name, text) in sessions {
                let at = std::path::Path::new(API);
                ws.open_shot_session(name, at, text, None, window, cx);
            }
            if !sessions.is_empty() {
                ws.select(0, window, cx);
            }
            ws
        });
        *slot.borrow_mut() = Some(ws.clone());
        ws.into()
    }
}

/// The Activity panel (M11h Task 6) in the right sidebar, in a 1100 × 560 window, both themes:
/// - `activity`: api driven as the app drives it: Working, 11 min later Working again (no
///   change) and Done, the note `a` twice, progress 10 then 20 %. Its log: 2 status entries,
///   1 note, 1 progress; its rows newest first `Progress 20%`, `a`, `Done — 11 min` (how long
///   it worked: `status_since` aged before the change, so `lasted` is read before the reset),
///   `Working`; the header `api` and `Done · 4m`. In the shot: the rows' marks in that order
///   (two faint dots, Done's, Working's), nothing after them; the name in `text`, the status
///   words `text_dim`. Then (not shot) the focus to web: its empty log, `No activity yet.`.
/// - `activity-waiting`: api Waiting on a question long enough to wrap: the header's words
///   `Needs input · 2m` in `ring_waiting`; the card 6 pt in, filled `waiting_tint`, at least two
///   lines of question tall, `Needs input` in it `ring_waiting`, the question in `text`.
/// - `activity-empty-none`: a terminal with no entries: `No activity yet.` under its header;
///   `activity-empty-unfollowed`: no terminal: `No terminal to follow.` and no header. Then (not
///   shot) a busy log with Reduce motion on, and again with it off: two frames 200 ms apart are
///   the same in the panel (nothing in it moves).
fn activity_scenes() {
    use crate::activity::Kind;
    use crate::activity_panel::{DOT, EVENT_BELOW, RAIL, TITLE_LINE};
    use crate::activity_panel::{NO_ACTIVITY, NO_TERMINAL, RowMark};
    use crate::command_name::{Change, Ending, Finished};
    use crate::right_sidebar::{HEADER, ROW};
    use crate::ui_tokens::size::{HAIRLINE, TAB_BAR};
    use alacritty_terminal::vte::ansi::Rgb;
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::time::{Duration, Instant};

    let dims = size(px(1100.0), px(560.0));
    let window_right = 1100.0_f32;
    let sidebar_left = right_panel_left(window_right, 300.0);
    let sidebar_right = right_panel_right(window_right);
    let card_top = CARD_INSET + HAIRLINE;
    let header_top = card_top + TAB_BAR;
    let list_top = header_top + HEADER;
    let frame = |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
        cx.update_window(window, |_, window, _| window.refresh())
            .expect("refresh");
        cx.run_until_parked();
    };
    let ago = |secs: u64| Instant::now() - Duration::from_secs(secs);
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let ui = theme.ui();
        let near = |p: [u8; 3], c: Rgb, by: u8| {
            p.iter()
                .zip([c.r, c.g, c.b])
                .all(|(x, y)| x.abs_diff(y) <= by)
        };
        // Whether any pixel in the box is `c` (within `by`).
        let has = |shot: &Shot, x0: f32, x1: f32, y0: f32, y1: f32, c: Rgb, by: u8| {
            let (w, h) = (((x1 - x0) * 2.0) as usize, ((y1 - y0) * 2.0) as usize);
            (0..h).any(|dy| {
                (0..w).any(|dx| {
                    let p = pixel_at(shot, dims, x0 + dx as f32 * 0.5, y0 + dy as f32 * 0.5);
                    near(p, c, by)
                })
            })
        };
        let marks = [ui.ring_working, ui.ring_waiting, ui.ring_done];

        // The timeline.
        let n = format!("activity-{slug}");
        let slot: Slot = Rc::new(RefCell::new(None));
        let build = activity_workspace(
            theme,
            &[("api", SHELL_TEXT), ("web", SHELL_TEXT)],
            true,
            slot.clone(),
        );
        let act = {
            let (n, slot) = (n.clone(), slot.clone());
            move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let ws = slotted(&slot);
                ws.update(cx, |ws, cx| {
                    let i = ws.sessions.position(|s| s.label() == "api").expect("api");
                    ws.set_status(i, Status::Working, false, cx);
                    // It works 11 min: aged before the next change (read as `lasted`).
                    ws.sessions.get_mut(i).expect("api").status_since = ago(11 * 60 + 5);
                    ws.set_status(i, Status::Working, false, cx);
                    ws.set_status(i, Status::Done, false, cx);
                    ws.set_note(i, Some("a".into()), cx);
                    ws.set_note(i, Some("a".into()), cx);
                    ws.set_progress(i, Some((10, None)), cx);
                    ws.set_progress(i, Some((20, None)), cx);
                    let s = ws.sessions.get_mut(i).expect("api");
                    s.status_since = ago(4 * 60 + 5);
                    s.pop = None;
                });
                // M11q: three endings on api's terminal, as its view's end marks make them: a
                // failed run, a stopped one and a quick `ls` (no entry).
                let api_view = cx.read(|cx| {
                    let ws = ws.read(cx);
                    let i = ws.sessions.position(|s| s.label() == "api").expect("api");
                    ws.sessions.items()[i].view.clone()
                });
                for (short, exit, ran) in
                    [("cargo test", 101, 40), ("pnpm dev", 130, 30), ("ls", 0, 1)]
                {
                    let finished = Finished {
                        short: Some(short.into()),
                        ending: Ending::of(Some(exit)),
                        ran: Duration::from_secs(ran),
                    };
                    let ended =
                        crate::terminal_view::TerminalEvent::Command(Change::Ended(finished));
                    cx.update(|cx| api_view.update(cx, |_, cx| cx.emit(ended)));
                    cx.run_until_parked();
                }
                frame(cx, window);
                cx.read(|cx| {
                    let ws = ws.read(cx);
                    let api = &ws.sessions.items()[0];
                    let kinds: Vec<_> = api.activity.newest_first().map(|e| &e.kind).collect();
                    let count = |f: fn(&Kind) -> bool| kinds.iter().filter(|k| f(k)).count();
                    assert_eq!(count(|k| matches!(k, Kind::Status { .. })), 2, "{n}");
                    assert_eq!(count(|k| matches!(k, Kind::Note(_))), 1, "{n}");
                    assert_eq!(count(|k| matches!(k, Kind::Progress(..))), 1, "{n}");
                    assert_eq!(
                        count(|k| matches!(k, Kind::Command { .. })),
                        2,
                        "{n}: not ls"
                    );
                    let rows = ws.activity_rows();
                    let words: Vec<_> = rows.iter().map(|r| r.text.as_str()).collect();
                    assert_eq!(
                        words,
                        [
                            "pnpm dev \u{2014} Stopped after 30s",
                            "cargo test \u{2014} Failed (exit 101) after 40s",
                            "Progress 20%",
                            "a",
                            "Done",
                            "Cultivating"
                        ],
                        "{n}: newest first"
                    );
                    // Part 4 (timeline): how long it worked moves to Done's second line.
                    assert_eq!(
                        rows[4].detail.as_deref(),
                        Some("Worked 11 min"),
                        "{n}: Done says how long it worked"
                    );
                    let marks: Vec<_> = rows.iter().map(|r| r.mark).collect();
                    assert_eq!(
                        marks,
                        [
                            RowMark::Command(Ending::Stopped),
                            RowMark::Command(Ending::Failed(101)),
                            RowMark::Progress,
                            RowMark::Note,
                            RowMark::Status(Status::Done),
                            RowMark::Status(Status::Working)
                        ],
                        "{n}"
                    );
                    let (label, words, status) = ws.activity_header().expect("followed");
                    assert_eq!(
                        (label.as_ref(), words.as_str(), status),
                        ("api", "Done \u{b7} 4m", Status::Done),
                        "{n}: the header"
                    );
                    assert_eq!(ws.activity_note(), None, "{n}: rows, no note");
                });
                slot.borrow_mut().take();
            }
        };
        let path = shoot_with("activity", slug, &theme, dims, build.clone(), act.clone());
        println!("ui-shots: {}", path.display());
        let shot = render_with(&theme, dims, build, act);
        // Part 4, the timeline: each event's dot in its colour on the rail (12 pt in, 14 pt
        // wide): progress the accent, a note faint, Done and Cultivating their status colours;
        // the times faint at the right; nothing in the waiting colour.
        let list = (list_top, list_top + 300.0);
        let rail = |c: Rgb| {
            has(
                &shot,
                sidebar_left + 12.0,
                sidebar_left + 26.0,
                list.0,
                list.1,
                c,
                16,
            )
        };
        // M11q: the stopped run is the top event (faint dot), the failed run the next: its dot
        // (the failure colour) is found on the rail, and the stopped one's lies one event's
        // height (title line and room under) above it.
        let rail_x = (
            sidebar_left + 12.0 + RAIL / 2.0 - 2.0,
            sidebar_left + 12.0 + RAIL / 2.0 + 2.0,
        );
        let failed_y = (0..240)
            .map(|i| list_top + i as f32 * 0.5)
            .find(|&y| has(&shot, rail_x.0, rail_x.1, y, y + 0.5, ui.block_failed, 16))
            .unwrap_or_else(|| panic!("{n}: the failed run's dot, in the failure colour"));
        let stopped_y = failed_y - (TITLE_LINE + EVENT_BELOW);
        assert!(stopped_y >= list_top, "{n}: the stopped run's row is above");
        let mid = |y: f32| (y + DOT / 2.0 - 1.0, y + DOT / 2.0 + 1.0);
        let (s0, s1) = mid(stopped_y);
        assert!(
            has(&shot, rail_x.0, rail_x.1, s0, s1, ui.text_faint, 16)
                && !has(&shot, rail_x.0, rail_x.1, s0, s1, ui.block_failed, 16),
            "{n}: the stopped run's dot is the faint one"
        );
        let (f0, f1) = mid(failed_y);
        assert!(
            !has(&shot, rail_x.0, rail_x.1, f0, f1, ui.text_faint, 16),
            "{n}: the failed run's dot is not the faint one"
        );
        for (what, c) in [
            ("progress", ui.accent),
            ("a note", ui.text_faint),
            ("Done", ui.ring_done),
            ("Cultivating", ui.ring_working),
        ] {
            assert!(rail(c), "{n}: {what}'s dot on the rail");
        }
        // No waiting dot: the failure colour is a red too, so the failed run's own dot is left
        // out of the look (the rail above it and below it).
        let (rail_l, rail_r) = (sidebar_left + 12.0, sidebar_left + 26.0);
        assert!(
            !has(
                &shot,
                rail_l,
                rail_r,
                list.0,
                failed_y - 1.0,
                ui.ring_waiting,
                16
            ) && !has(
                &shot,
                rail_l,
                rail_r,
                failed_y + DOT + 1.0,
                list.1,
                ui.ring_waiting,
                16
            ),
            "{n}: no waiting dot"
        );
        assert!(
            has(
                &shot,
                sidebar_right - 50.0,
                sidebar_right - 8.0,
                list.0,
                list.1,
                ui.text_faint,
                12
            ),
            "{n}: the times at the right"
        );
        // The header: the name in `text` on the 12 pt edge; the status as a pill (its tint and
        // its words in Done's colour), never the waiting colour.
        let line_top = header_top + (HEADER - 20.0) / 2.0;
        let left = sidebar_left + 12.0;
        assert!(
            has(
                &shot,
                left,
                left + 60.0,
                line_top,
                line_top + 20.0,
                ui.text,
                12
            ),
            "{n}: the name"
        );
        let (fill, ink) = crate::activity_panel::pill_inks(Status::Done, &ui);
        let pill = |c: Rgb| {
            has(
                &shot,
                left + 40.0,
                sidebar_right - 8.0,
                header_top,
                header_top + HEADER,
                c,
                8,
            )
        };
        assert!(pill(fill) && pill(ink), "{n}: the Done pill");
        assert!(!pill(ui.ring_waiting), "{n}: not in the waiting colour");

        // The focus to web (not shot): its own log, empty.
        let n = format!("activity-switch-{slug}");
        let slot: Slot = Rc::new(RefCell::new(None));
        let build = activity_workspace(
            theme,
            &[("api", SHELL_TEXT), ("web", SHELL_TEXT)],
            true,
            slot.clone(),
        );
        let act = {
            let (n, slot) = (n.clone(), slot.clone());
            move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let ws = slotted(&slot);
                ws.update(cx, |ws, cx| {
                    let i = ws.sessions.position(|s| s.label() == "api").expect("api");
                    ws.set_status(i, Status::Working, false, cx);
                    ws.set_note(i, Some("busy".into()), cx);
                });
                frame(cx, window);
                assert_eq!(
                    cx.read(|cx| ws.read(cx).activity_rows().len()),
                    2,
                    "{n}: api's"
                );
                cx.update_window(window, |_, window, cx| {
                    ws.update(cx, |ws, cx| {
                        let i = ws.sessions.position(|s| s.label() == "web").expect("web");
                        ws.select(i, window, cx);
                    })
                })
                .expect("focus web");
                frame(cx, window);
                cx.read(|cx| {
                    let ws = ws.read(cx);
                    let web = ws.sessions.items().iter().find(|s| s.label() == "web");
                    assert_eq!(
                        ws.right_sidebar.followed,
                        web.map(|s| s.id),
                        "{n}: follows web"
                    );
                    assert_eq!(
                        ws.activity_note(),
                        Some(NO_ACTIVITY),
                        "{n}: web's empty log"
                    );
                    assert!(ws.activity_rows().is_empty(), "{n}");
                    let (label, _, _) = ws.activity_header().expect("followed");
                    assert_eq!(label.as_ref(), "web", "{n}: its header");
                });
                slot.borrow_mut().take();
            }
        };
        let shot = render_with(&theme, dims, build, act);
        assert!(
            has(
                &shot,
                sidebar_left + 2.0,
                sidebar_right,
                list_top,
                list_top + 30.0,
                ui.text_dim,
                6
            ),
            "{n}: the note's words"
        );
        for c in marks {
            assert!(
                !has(
                    &shot,
                    sidebar_left + 12.0,
                    sidebar_left + 26.0,
                    list_top,
                    list_top + 4.0 * ROW,
                    c,
                    24
                ),
                "{n}: no events ({c:?})"
            );
        }
        println!("ui-shots: {n} ok");

        // Waiting, on a question that wraps.
        let n = format!("activity-waiting-{slug}");
        let slot: Slot = Rc::new(RefCell::new(None));
        let build = activity_workspace(theme, &[("api", ASKING_TEXT)], true, slot.clone());
        let act = {
            let (n, slot) = (n.clone(), slot.clone());
            move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let ws = slotted(&slot);
                ws.update(cx, |ws, cx| {
                    ws.set_status(0, Status::Working, false, cx);
                    ws.set_status(0, Status::Waiting, false, cx);
                    ws.sessions.get_mut(0).expect("api").status_since = ago(2 * 60 + 5);
                });
                frame(cx, window);
                cx.read(|cx| {
                    let ws = ws.read(cx);
                    let question = ws.sessions.items()[0].question.clone();
                    assert_eq!(
                        question.as_deref(),
                        Some("Run the database migration on production now? (y/n)"),
                        "{n}: read off the screen"
                    );
                    let (_, words, status) = ws.activity_header().expect("followed");
                    assert_eq!(
                        (words.as_str(), status),
                        ("Needs input \u{b7} 2m", Status::Waiting),
                        "{n}"
                    );
                    let tip = ws.activity_rows()[0].tip.clone();
                    assert_eq!(
                        tip.as_deref(),
                        Some("Needs input\nRun the database migration on production now? (y/n)"),
                        "{n}: the entry keeps its question"
                    );
                });
                slot.borrow_mut().take();
            }
        };
        let path = shoot_with(
            "activity-waiting",
            slug,
            &theme,
            dims,
            build.clone(),
            act.clone(),
        );
        println!("ui-shots: {}", path.display());
        let shot = render_with(&theme, dims, build, act);
        // Part 4: the header's pill in the waiting colours; the newest event, Needs input, a red
        // dot on the rail with its question whole in a tinted box under it (at least two lines,
        // not the whole panel).
        let left = sidebar_left + 12.0;
        let (fill, ink) = crate::activity_panel::pill_inks(Status::Waiting, &ui);
        let pill = |c: Rgb| {
            has(
                &shot,
                left + 40.0,
                sidebar_right - 8.0,
                header_top,
                header_top + HEADER,
                c,
                8,
            )
        };
        assert!(
            pill(fill) && pill(ink),
            "{n}: the header's pill in the waiting colours"
        );
        assert!(
            has(
                &shot,
                sidebar_left + 12.0,
                sidebar_left + 26.0,
                list_top,
                list_top + 80.0,
                ui.ring_waiting,
                16
            ),
            "{n}: the newest event's dot is Waiting's"
        );
        let (box_left, box_right) = (sidebar_left + 12.0 + 14.0 + 10.0, sidebar_right - 50.0);
        let tint_at = |y: f32| {
            (0..((box_right - box_left) as usize)).any(|dx| {
                near(
                    pixel_at(&shot, dims, box_left + dx as f32, y),
                    ui.waiting_tint,
                    3,
                )
            })
        };
        let rows: Vec<f32> = (0..600)
            .map(|dy| list_top + dy as f32 * 0.5)
            .filter(|&y| tint_at(y))
            .collect();
        let tall = rows.len() as f32 * 0.5;
        println!("ui-shots: {n}: the question's box is {tall} pt tall");
        assert!(
            tall >= 40.0,
            "{n}: two lines of question at least: {tall} pt"
        );
        assert!(tall < 120.0, "{n}: not the whole panel: {tall} pt");
        let (top, bottom) = (rows[0], rows[rows.len() - 1]);
        assert!(
            has(&shot, box_left, box_right, top, bottom, ui.text, 12),
            "{n}: the question in the box"
        );

        // Empty.
        for case in ["none", "unfollowed"] {
            let scene = format!("activity-empty-{case}");
            let n = format!("{scene}-{slug}");
            let slot: Slot = Rc::new(RefCell::new(None));
            let sessions: &'static [(&str, &str)] = match case {
                "none" => &[("api", SHELL_TEXT)],
                _ => &[],
            };
            let build = activity_workspace(theme, sessions, true, slot.clone());
            let act = {
                let (n, slot) = (n.clone(), slot.clone());
                move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                    let ws = slotted(&slot);
                    frame(cx, window);
                    cx.read(|cx| {
                        let ws = ws.read(cx);
                        let (note, header) = match case {
                            "none" => (NO_ACTIVITY, true),
                            _ => (NO_TERMINAL, false),
                        };
                        assert_eq!(ws.activity_note(), Some(note), "{n}");
                        assert_eq!(ws.activity_header().is_some(), header, "{n}: the header");
                    });
                    slot.borrow_mut().take();
                }
            };
            let path = shoot_with(&scene, slug, &theme, dims, build.clone(), act.clone());
            println!("ui-shots: {}", path.display());
            let shot = render_with(&theme, dims, build, act);
            let header = case == "none";
            assert_eq!(
                has(
                    &shot,
                    sidebar_left + 2.0,
                    sidebar_right,
                    header_top,
                    header_top + 30.0,
                    ui.text,
                    3
                ),
                header,
                "{n}: the header {}",
                if header { "shows" } else { "doesn't" }
            );
            let note_top = if header { list_top } else { header_top };
            assert!(
                has(
                    &shot,
                    sidebar_left + 2.0,
                    sidebar_right,
                    note_top,
                    note_top + 30.0,
                    ui.text_dim,
                    6
                ),
                "{n}: the note's words"
            );
        }

        // Nothing moves: two frames 200 ms apart, the same in the panel.
        for still in [true, false] {
            let n = format!("activity-still-{slug}-{still}");
            let slot: Slot = Rc::new(RefCell::new(None));
            let build = activity_workspace(
                theme,
                &[("api", ASKING_TEXT), ("web", SHELL_TEXT)],
                still,
                slot.clone(),
            );
            let act = {
                let n = n.clone();
                move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                    let ws = slotted(&slot);
                    ws.update(cx, |ws, cx| {
                        ws.set_status(0, Status::Working, false, cx);
                        ws.set_note(0, Some("migrating".into()), cx);
                        ws.set_status(0, Status::Waiting, false, cx);
                        ws.set_status(0, Status::Working, false, cx);
                        ws.set_status(0, Status::Done, false, cx);
                        ws.set_status(0, Status::Working, false, cx);
                        ws.set_status(0, Status::Waiting, false, cx);
                        ws.sessions.get_mut(0).expect("api").status_since = ago(4 * 60 + 30);
                    });
                    frame(cx, window);
                    let grab = |cx: &mut VisualTestAppContext| {
                        let image = cx.capture_screenshot(window).expect("capture");
                        let (w, raw) = (image.width() as usize, image.as_raw().to_vec());
                        let scale = w as f32 / f32::from(dims.width);
                        let (x0, x1) = (
                            (sidebar_left * scale) as usize,
                            (sidebar_right * scale) as usize,
                        );
                        let (y0, y1) = (
                            (header_top * scale) as usize,
                            ((f32::from(dims.height) - 20.0) * scale) as usize,
                        );
                        (y0..y1)
                            .flat_map(|y| raw[(y * w + x0) * 4..(y * w + x1) * 4].to_vec())
                            .collect::<Vec<u8>>()
                    };
                    let before = grab(cx);
                    prof::sleep(Duration::from_millis(200));
                    // Drawn again for certain (not a cached frame).
                    ws.update(cx, |_, cx| cx.notify());
                    frame(cx, window);
                    let after = grab(cx);
                    assert!(before == after, "{n}: the panel moved in 200 ms");
                    assert_eq!(cx.read(|cx| ws.read(cx).activity_rows().len()), 7, "{n}");
                    slot.borrow_mut().take();
                }
            };
            render_with(&theme, dims, build, act);
            println!("ui-shots: {n} ok");
        }
    }
}
