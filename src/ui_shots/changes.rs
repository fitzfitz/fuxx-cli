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
            "src/reduce_motion.rs",
        ],
    },
];

/// The GRAPH (M11w, note 74), in both themes, a clean repo: "No changes", the GRAPH header,
/// then a merge (HEAD, `feat/x` and `origin/feat/x`), its two parents on two lanes, their fork.
/// - `changes-graph`: the lanes in their colours, HEAD's ring, the pills.
/// - `changes-graph-card`: the pointer on the merge: its card, with Open on GitHub.
fn changes_graph_scenes() {
    use crate::git_graph::{Commit, Details, Ref};
    use crate::git_status::ChangeList;
    use crate::right_sidebar::{Panel, ROW};
    use crate::ui_tokens::size::{HAIRLINE, TAB_BAR};
    use alacritty_terminal::vte::ansi::Rgb;
    use std::cell::RefCell;
    use std::rc::Rc;

    let dims = size(px(1100.0), px(560.0));
    let window_right = 1100.0_f32;
    let sidebar_left = window_right - RIGHT_CARD_GAP - 300.0 + HAIRLINE;
    let list_top = CARD_INSET + HAIRLINE + TAB_BAR + crate::changes_panel::HEADER;
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
        for card in [false, true] {
            let scene = if card {
                "changes-graph-card"
            } else {
                "changes-graph"
            };
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
                        changes.graph.open = true;
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
            let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                cx.update_window(window, |_, window, _| window.refresh())
                    .expect("refresh");
                cx.run_until_parked();
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
            // Rows: 0 "No changes", 1 GRAPH, 2–5 the commits; the lanes in the first 24 pt.
            let commits_top = list_top + 2.0 * ROW;
            let lanes = |c: Rgb| {
                has(
                    sidebar_left + 12.0,
                    sidebar_left + 36.0,
                    commits_top,
                    commits_top + 4.0 * ROW,
                    c,
                    16,
                )
            };
            assert!(lanes(inks[0]), "{n}: the first lane's colour");
            assert!(lanes(inks[1]), "{n}: the merged lane's colour");
            assert!(
                has(
                    sidebar_left + 12.0,
                    sidebar_left + 290.0,
                    list_top + ROW,
                    list_top + 2.0 * ROW,
                    ui.text_faint,
                    16
                ),
                "{n}: the GRAPH header"
            );
            // The card: to the left of the panel, on `panel`, with its accent link.
            let card_area = |c: Rgb| {
                has(
                    sidebar_left - 300.0,
                    sidebar_left - 20.0,
                    commits_top,
                    commits_top + 200.0,
                    c,
                    6,
                )
            };
            assert_eq!(card_area(ui.accent), card, "{n}: the card's links");
        }
    }
}

/// Sticky headers (your ask: "make other accordion sticky"): 60 commits, scrolled 8 rows down:
/// the GRAPH's header stays pinned over the list's top, its hairline under it.
fn changes_sticky_scenes() {
    use crate::git_graph::Commit;
    use crate::git_status::ChangeList;
    use crate::right_sidebar::{Panel, ROW};
    use crate::ui_tokens::size::{HAIRLINE, TAB_BAR};
    use alacritty_terminal::vte::ansi::Rgb;
    use std::cell::RefCell;
    use std::rc::Rc;

    let dims = size(px(1100.0), px(560.0));
    let window_right = 1100.0_f32;
    let sidebar_left = window_right - RIGHT_CARD_GAP - 300.0 + HAIRLINE;
    let list_top = CARD_INSET + HAIRLINE + TAB_BAR + crate::changes_panel::HEADER;
    let root = files_fixture();
    let commits: Vec<Commit> = (0..60)
        .map(|i| Commit {
            hash: format!("{i:07x}"),
            parents: if i < 59 {
                vec![format!("{:07x}", i + 1)]
            } else {
                vec![]
            },
            author: "Ayu".into(),
            time: 1_794_455_520 - i * 600,
            refs: vec![],
            subject: format!("Commit number {i}"),
            head: i == 0,
        })
        .collect();
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let ui = theme.ui();
        let n = format!("changes-sticky-{slug}");
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
                    changes.graph.open = true;
                    changes.graph.apply("shot".into(), commits.clone(), None);
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
        // The pinned header: `GRAPH` (faint) in the list's first row, though its row has
        // scrolled away.
        assert!(
            has(
                sidebar_left + 28.0,
                sidebar_left + 90.0,
                list_top + 2.0,
                list_top + ROW - 2.0,
                ui.text_faint,
                16
            ),
            "{n}: the GRAPH header is pinned at the top"
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
    use crate::right_sidebar::{Panel, ROW};
    use crate::ui_tokens::size::{HAIRLINE, TAB_BAR};
    use alacritty_terminal::vte::ansi::Rgb;
    use gpui::Modifiers;
    use std::cell::RefCell;
    use std::rc::Rc;

    let dims = size(px(1100.0), px(560.0));
    let window_right = 1100.0_f32;
    let sidebar_left = window_right - RIGHT_CARD_GAP - 300.0 + HAIRLINE;
    let sidebar_right = window_right - RIGHT_CARD_GAP - HAIRLINE;
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
    let row_top = |r: usize| list_top + r as f32 * ROW;
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
                    let on = point(px(sidebar_left + 15.0), px(row_top(1) + ROW / 2.0));
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
                    sidebar_left + 10.0,
                    sidebar_left + 30.0,
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
                    // The bar's green third, then its track.
                    let bar_y = header_top + crate::changes_panel::HEADER / 2.0 + 9.0;
                    assert!(
                        has(
                            sidebar_left + 14.0,
                            sidebar_left + 60.0,
                            bar_y - 3.0,
                            bar_y + 3.0,
                            green,
                            16
                        ),
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
                    assert!(
                        has(
                            sidebar_left + 8.0,
                            sidebar_left + 26.0,
                            header_top + 4.0,
                            header_top + 28.0,
                            green,
                            16
                        ),
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
    use crate::ui_tokens::size::HAIRLINE;
    use alacritty_terminal::vte::ansi::Rgb;
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::time::{Duration, SystemTime};

    let dims = size(px(1100.0), px(560.0));
    let window_right = 1100.0_f32;
    let sidebar_left = window_right - RIGHT_CARD_GAP - 300.0 + HAIRLINE;
    let sidebar_right = window_right - RIGHT_CARD_GAP - HAIRLINE;
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
    // (slug, sync, busy, error, tooltip shown, accent button expected)
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
            // The bar is the card's last 56 pt (476 to 532 in a 560 pt window); its buttons in
            // its right 80 pt; the tooltip hangs just above them, where no row is.
            let (bar_top, bar_bottom) = (476.0, 532.0);
            let accented = has(
                sidebar_right - 80.0,
                sidebar_right,
                bar_top,
                bar_bottom,
                ui.accent,
                10,
            );
            if busy.is_some() {
                // Dimmed: not the full accent.
                assert!(!accented, "{n}: the accent dims while it works");
            } else {
                assert_eq!(accented, accent, "{n}: the accent button");
            }
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
///   row in neither; the hovered row (the fifth) `card_hover`, the others the card.
/// - `changes-empty-clean`: an empty list in a repo: the header (name only), then
///   "No changes since the last commit."; `changes-empty-norepo`: not a repo: "Not a git
///   repository." and no header; `changes-empty-ssh`: an SSH terminal followed: "Changes
///   aren't shown for SSH terminals.", no header.
/// - `changes-click`: a click on the M row opens src/main.rs in the focused pane in Diff mode
///   (the model's tab), and that row is drawn `card_selected`.
fn changes_scenes() {
    use crate::changes_panel::{HEADER, NO_CHANGES, NO_REPO, SSH_NOTE, totals};
    use crate::git_status::{Change, ChangeList, Counts, Mark};
    use crate::layout_model::{FileMode, TabKind};
    use crate::right_sidebar::{Panel, ROW, mark_color};
    use crate::ui_tokens::size::{HAIRLINE, TAB_BAR};
    use alacritty_terminal::vte::ansi::Rgb;
    use gpui::{Focusable, Modifiers};
    use std::cell::RefCell;
    use std::rc::Rc;

    let dims = size(px(1100.0), px(560.0));
    let window_right = 1100.0_f32;
    let sidebar_left = window_right - RIGHT_CARD_GAP - 300.0 + HAIRLINE;
    let sidebar_right = window_right - RIGHT_CARD_GAP - HAIRLINE;
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
        let line_top = header_top + (HEADER - 28.0) / 2.0;
        let (left, right) = (sidebar_left + 10.0, sidebar_right - 6.0);
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
                line_top + 24.0,
                line_top + 29.0,
                ui.divider,
                6
            ),
            "{n}: the progress bar's track"
        );
        // Each row's letter in its mark's colour, and in no other mark's.
        // Part 2: after the row's circle (14 pt and a 6 pt gap).
        let letter = |i: usize, c: Rgb| in_row(i, sidebar_left + 28.0, sidebar_left + 40.0, c);
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
        // `… 3 more`, faint, under the names; then the GRAPH's header (M11w; no commits here:
        // the fixture isn't a git repo), and nothing after it.
        let more = |i: usize, c: Rgb| in_row(i, sidebar_left + 26.0, sidebar_left + 120.0, c);
        assert!(more(6, ui.text_faint), "{n}: the more row");
        assert!(more(7, ui.text_faint), "{n}: the GRAPH header after it");
        assert!(
            !more(8, ui.text_faint),
            "{n}: nothing after the GRAPH header"
        );
        // The hovered row filled `card_hover`, the one above it the card (part 4: a pill 6 pt
        // in from the side, so read 4 pt inside it).
        let fill = |i: usize| at(sidebar_left + 10.0, row_mid(i));
        assert!(
            near(fill(4), ui.card_hover, 3),
            "{n}: hovered {:?}",
            fill(4)
        );
        assert!(near(fill(3), ui.card, 3), "{n}: not hovered {:?}", fill(3));

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
                    slot.borrow_mut().take();
                }
            };
            let path = shoot_with(&scene, slug, &theme, dims, build.clone(), act.clone());
            println!("ui-shots: {}", path.display());
            let shot = render_with(&theme, dims, build, act);
            let at = |x: f32, y: f32| pixel_at(&shot, dims, x, y);
            // Tight: Latte's dim and full text colours are close.
            let has = |y0: f32, y1: f32, c: Rgb| {
                (0..((y1 - y0) * 2.0) as usize).any(|dy| {
                    (0..400).any(|dx| {
                        let (x, y) = (sidebar_left + 2.0 + dx as f32 * 0.5, y0 + dy as f32 * 0.5);
                        near(at(x, y), c, 6)
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
        // Part 4: the selection is a pill 6 pt in from the side: read 4 pt inside it.
        let fill = |i: usize| pixel_at(&shot, dims, sidebar_left + 10.0, row_mid(i));
        assert!(
            near(fill(0), ui.card_selected, 3),
            "{n}: the M row is selected: {:?}",
            fill(0)
        );
        assert!(
            near(fill(1), ui.card, 3),
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
    use crate::activity_panel::{NO_ACTIVITY, NO_TERMINAL, RowMark};
    use crate::right_sidebar::{HEADER, ROW};
    use crate::ui_tokens::size::{HAIRLINE, TAB_BAR};
    use alacritty_terminal::vte::ansi::Rgb;
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::time::{Duration, Instant};

    let dims = size(px(1100.0), px(560.0));
    let window_right = 1100.0_f32;
    let sidebar_left = window_right - RIGHT_CARD_GAP - 300.0 + HAIRLINE;
    let sidebar_right = window_right - RIGHT_CARD_GAP - HAIRLINE;
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
                frame(cx, window);
                cx.read(|cx| {
                    let ws = ws.read(cx);
                    let api = &ws.sessions.items()[0];
                    let kinds: Vec<_> = api.activity.newest_first().map(|e| &e.kind).collect();
                    let count = |f: fn(&Kind) -> bool| kinds.iter().filter(|k| f(k)).count();
                    assert_eq!(count(|k| matches!(k, Kind::Status { .. })), 2, "{n}");
                    assert_eq!(count(|k| matches!(k, Kind::Note(_))), 1, "{n}");
                    assert_eq!(count(|k| matches!(k, Kind::Progress(..))), 1, "{n}");
                    let rows = ws.activity_rows();
                    let words: Vec<_> = rows.iter().map(|r| r.text.as_str()).collect();
                    assert_eq!(
                        words,
                        ["Progress 20%", "a", "Done", "Cultivating"],
                        "{n}: newest first"
                    );
                    // Part 4 (timeline): how long it worked moves to Done's second line.
                    assert_eq!(
                        rows[2].detail.as_deref(),
                        Some("Worked 11 min"),
                        "{n}: Done says how long it worked"
                    );
                    let marks: Vec<_> = rows.iter().map(|r| r.mark).collect();
                    assert_eq!(
                        marks,
                        [
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
        for (what, c) in [
            ("progress", ui.accent),
            ("a note", ui.text_faint),
            ("Done", ui.ring_done),
            ("Cultivating", ui.ring_working),
        ] {
            assert!(rail(c), "{n}: {what}'s dot on the rail");
        }
        assert!(!rail(ui.ring_waiting), "{n}: no waiting dot");
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
                    6
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
