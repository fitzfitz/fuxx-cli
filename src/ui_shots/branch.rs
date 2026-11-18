//! The branch switcher (M11o): seeded, its confirmation, on a real repo, and the ways to open it.

use super::*;

/// This file, as `file!()` gives it (repo-relative under cargo): `registry::AREAS` pairs it with
/// `SCENES`, so `--changed` runs these scenes when this file changes.
pub(super) const FILE: &str = file!();

/// This area's scenes (`registry.rs` runs them in its `ORDER`).
pub(super) const SCENES: &[Scene] = &[
    Scene {
        name: "branch-switcher",
        run: branch_switcher_scene,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/branch_switcher.rs",
            "src/branch_list.rs",
            "src/branch_git.rs",
            "src/overlay_panel.rs",
            "src/palette.rs",
            "src/palette_items.rs",
        ],
    },
    Scene {
        name: "branch-switcher-confirm",
        run: branch_switcher_confirm_scene,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/branch_switcher.rs",
            "src/branch_list.rs",
            "src/branch_git.rs",
            "src/overlay_panel.rs",
            "src/procs.rs",
        ],
    },
    Scene {
        name: "branch-switcher-real",
        run: branch_switcher_real_scene,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: &[
            "src/branch_switcher.rs",
            "src/branch_git.rs",
            "src/branch_list.rs",
            "src/git_branch.rs",
        ],
    },
    Scene {
        name: "branch-switcher-open",
        run: branch_switcher_open_scene,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: &[
            "src/branch_switcher.rs",
            "src/palette.rs",
            "src/palette_items.rs",
            "src/sidebar.rs",
            "src/status_bar.rs",
            "src/git_branch.rs",
            "src/procs.rs",
        ],
    },
];

/// The open branch switcher; none while it's closed.
fn branch_switcher_of(
    cx: &mut VisualTestAppContext,
    ws: &gpui::Entity<Workspace>,
) -> Option<gpui::Entity<crate::branch_switcher::BranchSwitcher>> {
    cx.read(|cx| ws.read(cx).branch_switcher.clone())
}

/// Open the switcher `make` builds as the Workspace opens one, then let it take the keyboard.
fn open_switcher_with(
    cx: &mut VisualTestAppContext,
    window: AnyWindowHandle,
    ws: &gpui::Entity<Workspace>,
    make: impl FnOnce(
        &mut Window,
        &mut Context<crate::branch_switcher::BranchSwitcher>,
    ) -> crate::branch_switcher::BranchSwitcher,
) {
    cx.update_window(window, |_, window, cx| {
        ws.update(cx, |ws, cx| {
            let sw = cx.new(|cx| make(window, cx));
            ws.show_branch_switcher(sw, window, cx);
        })
    })
    .expect("open the branch switcher");
    settle(cx, window);
}

/// The branch switcher (M11o Task 3) over `switcher_workspace` (shell's terminal focused), in
/// both themes: `branch-switcher`, opened seeded on `switcher_snapshot` with feature/old
/// conflicting (3 files), then ↓ ↓ ↑ (on spike) and a click on Fetch (`Fetching…` beside it).
///
/// Panics unless: the panel is where the ⌘K palette is (same left, top and width); the title
/// names the repo; the lines are main (current), feature/rows (↑2 ↓1), spike (local only),
/// feature/old (conflict 3 file(s)), ORIGIN, fix-login; feature/rows is chosen first; ↓ goes to
/// spike, ↓ again skips feature/old (and the ORIGIN heading) to fix-login, ↑ skips it back to
/// spike; feature/old's text is drawn in `text_faint` (none in `text`) while spike's name is in
/// `text`; a click on Fetch leaves it open with the box's keyboard and shows `Fetching…` (a
/// seeded switcher runs no git). And (not shot): typing `zzz` leaves exactly one row, `Check out zzz from origin`;
/// Escape closes it, the active terminal with the keyboard; a switcher closed, or replaced,
/// before its read lands drops that read (no panic, nothing reopens, the replacement keeps its
/// own list) and is gone.
fn branch_switcher_scene() {
    use crate::branch_switcher::BranchSwitcher;
    use crate::palette_items::Scope;
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    type Seen = (gpui::Bounds<Pixels>, gpui::Bounds<Pixels>);
    let dims = size(px(960.0), px(600.0));
    let want_lines = [
        "main · current",
        "feature/rows · ↑2 ↓1",
        "spike · local only",
        "feature/old · conflict 3 file(s)",
        "ORIGIN",
        "fix-login",
    ];
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let ui = theme.ui();
        for shot_pass in [true, false] {
            let slot: Slot = Rc::new(RefCell::new(None));
            let build = palette_build(theme, slot.clone());
            let seen: Rc<Cell<Option<Seen>>> = Rc::default();
            let found = seen.clone();
            let n = format!("branch-switcher-{slug}");
            let seeded = |window: &mut Window, cx: &mut Context<BranchSwitcher>| {
                let conflicts = [("feature/old".to_string(), 3)].into_iter().collect();
                BranchSwitcher::seeded(
                    switcher_repo(),
                    vec![],
                    switcher_snapshot(),
                    conflicts,
                    None,
                    window,
                    cx,
                )
            };
            let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let ws = slotted(&slot);
                let lines = |cx: &mut VisualTestAppContext| {
                    let sw = branch_switcher_of(cx, &ws).expect("the switcher is open");
                    cx.read(|cx| sw.read(cx).line_texts())
                };
                let chosen = |cx: &mut VisualTestAppContext| {
                    let sw = branch_switcher_of(cx, &ws).expect("the switcher is open");
                    cx.read(|cx| sw.read(cx).selected_label())
                };
                if shot_pass {
                    // Where the palette's panel is drawn.
                    open_palette_on(cx, window, &ws, Scope::All);
                    let palette = palette_of(cx, &ws).expect("the palette is open");
                    let at = cx
                        .read(|cx| palette.read(cx).panel_bounds())
                        .expect("the palette is drawn");
                    press_keys(cx, window, "escape");
                    assert!(palette_of(cx, &ws).is_none(), "{n}: the palette closed");

                    open_switcher_with(cx, window, &ws, seeded);
                    let sw = branch_switcher_of(cx, &ws).expect("the switcher is open");
                    let (title, label, panel) = cx.read(|cx| {
                        let s = sw.read(cx);
                        (s.title(), s.label().to_string(), s.panel_bounds())
                    });
                    assert_eq!(title, "Switch branch · api", "{n}: the title");
                    assert_eq!(label, "~/code/api", "{n}: the folder");
                    let panel = panel.expect("the switcher is drawn");
                    for (what, a, b) in [
                        ("left", panel.left(), at.left()),
                        ("top", panel.top(), at.top()),
                        ("width", panel.size.width, at.size.width),
                    ] {
                        let (a, b) = (f32::from(a), f32::from(b));
                        assert!(
                            (a - b).abs() < 0.5,
                            "{n}: its {what} is {a}, the palette's {b}"
                        );
                    }
                    assert_eq!(lines(cx), want_lines, "{n}: the lines");
                    assert_eq!(chosen(cx).as_deref(), Some("feature/rows"), "{n}: first");
                    press_keys(cx, window, "down");
                    assert_eq!(chosen(cx).as_deref(), Some("spike"), "{n}: ↓");
                    press_keys(cx, window, "down");
                    assert_eq!(
                        chosen(cx).as_deref(),
                        Some("fix-login"),
                        "{n}: ↓ skips feature/old"
                    );
                    press_keys(cx, window, "up");
                    assert_eq!(chosen(cx).as_deref(), Some("spike"), "{n}: ↑ skips it back");
                    // A click on Fetch: the box mustn't lose the keyboard (that closes it).
                    let fetch = cx
                        .read(|cx| sw.read(cx).fetch_bounds())
                        .expect("the Fetch button is drawn");
                    cx.simulate_click(window, fetch.center(), gpui::Modifiers::none());
                    settle(cx, window);
                    assert!(
                        branch_switcher_of(cx, &ws).as_ref() == Some(&sw),
                        "{n}: a click on Fetch leaves it open"
                    );
                    let focused = cx
                        .update_window(window, |_, window, cx| {
                            use gpui::Focusable;
                            sw.focus_handle(cx).is_focused(window)
                        })
                        .expect("read the focus");
                    assert!(focused, "{n}: the box keeps the keyboard");
                    assert_eq!(
                        cx.read(|cx| sw.read(cx).fetch_status()).as_deref(),
                        Some("Fetching\u{2026}"),
                        "{n}: Fetching… beside the button"
                    );
                    found.set(cx.read(|cx| {
                        let s = sw.read(cx);
                        Some((s.line_bounds(3)?, s.line_bounds(2)?))
                    }));
                } else {
                    open_switcher_with(cx, window, &ws, seeded);
                    cx.simulate_input(window, "zzz");
                    settle(cx, window);
                    assert_eq!(
                        lines(cx),
                        ["Check out zzz from origin"],
                        "{n}: zzz offers only the typed name"
                    );
                    assert_eq!(
                        chosen(cx).as_deref(),
                        Some("Check out zzz from origin"),
                        "{n}: the typed row can be chosen"
                    );
                    press_keys(cx, window, "escape");
                    assert!(branch_switcher_of(cx, &ws).is_none(), "{n}: Escape closes");
                    assert!(
                        active_terminal_focused(cx, window, &ws),
                        "{n}: the terminal has the keyboard back"
                    );

                    // A read that lands after its switcher closed: dropped.
                    let ran = Arc::new(AtomicBool::new(false));
                    let read_ran = ran.clone();
                    let gone = cx
                        .update_window(window, |_, window, cx| {
                            ws.update(cx, |ws, cx| {
                                let sw = cx.new(|cx| {
                                    BranchSwitcher::with_read(
                                        switcher_repo(),
                                        vec![],
                                        move || {
                                            read_ran.store(true, Ordering::SeqCst);
                                            Some(switcher_snapshot())
                                        },
                                        window,
                                        cx,
                                    )
                                });
                                ws.show_branch_switcher(sw.clone(), window, cx);
                                // Closed before any task has run.
                                ws.branch_switcher = None;
                                ws.focus_active(window, cx);
                                sw.downgrade()
                            })
                        })
                        .expect("open and close");
                    settle(cx, window);
                    assert!(ran.load(Ordering::SeqCst), "{n}: the read ran");
                    assert!(
                        branch_switcher_of(cx, &ws).is_none(),
                        "{n}: a late read reopens nothing"
                    );
                    assert!(gone.upgrade().is_none(), "{n}: the closed switcher is gone");
                    assert!(
                        active_terminal_focused(cx, window, &ws),
                        "{n}: the terminal keeps the keyboard"
                    );

                    // A read that lands after its switcher was replaced: dropped too.
                    let ran = Arc::new(AtomicBool::new(false));
                    let read_ran = ran.clone();
                    let old = cx
                        .update_window(window, |_, window, cx| {
                            ws.update(cx, |ws, cx| {
                                let old = cx.new(|cx| {
                                    BranchSwitcher::with_read(
                                        switcher_repo(),
                                        vec![],
                                        move || {
                                            read_ran.store(true, Ordering::SeqCst);
                                            let mut other = switcher_snapshot();
                                            other.local.clear();
                                            other.origin.clear();
                                            Some(other)
                                        },
                                        window,
                                        cx,
                                    )
                                });
                                ws.show_branch_switcher(old.clone(), window, cx);
                                old.downgrade()
                            })
                        })
                        .expect("open the first");
                    open_switcher_with(cx, window, &ws, seeded);
                    assert!(ran.load(Ordering::SeqCst), "{n}: the first's read ran");
                    assert!(
                        old.upgrade().is_none(),
                        "{n}: the replaced switcher is gone"
                    );
                    assert_eq!(lines(cx), want_lines, "{n}: the new one keeps its list");
                    press_keys(cx, window, "escape");
                }
                slot.borrow_mut().take();
            };
            let shot = render_with(&theme, dims, build, act);
            if !shot_pass {
                continue;
            }
            save_shot("branch-switcher", slug, &shot);
            let (old, spike) = seen.get().expect("drawn");
            // Glyph pixels exactly in a colour: about as many per point as the scale (the shot's
            // pixels per point), so the least is 5 at 1x and 10 at 2x.
            let least = (5.0 * shot.width as f32 / f32::from(dims.width)).round() as usize;
            let faint = count_along(
                &shot,
                dims,
                ui.text_faint,
                f32::from(old.center().y),
                f32::from(old.left()) + 20.0,
                f32::from(old.right()) - 4.0,
            );
            let bright = count_along(
                &shot,
                dims,
                ui.text,
                f32::from(old.center().y),
                f32::from(old.left()) + 20.0,
                f32::from(old.right()) - 4.0,
            );
            assert!(
                faint >= least && bright == 0,
                "{slug}: feature/old is faint ({faint} faint, {bright} in text, {least} needed)"
            );
            let spike_text = count_along(
                &shot,
                dims,
                ui.text,
                f32::from(spike.center().y),
                f32::from(spike.left()) + 20.0,
                f32::from(spike.right()) - 4.0,
            );
            assert!(
                spike_text >= least,
                "{slug}: spike's name is in text ({spike_text}, {least} needed)"
            );
        }
    }
}

/// The ways to open the branch switcher (M11o Task 5), over `switcher_workspace` with claude (its
/// first, with a program running in it), tests and shell (the focused one) pointed at a real scratch
/// repo (`main` and `feature`, reached through `/var`, which git sees as `/private/var`), in the
/// first theme: `branch-switcher-open`, the status bar's branch clicked, `feature` chosen, the
/// confirmation up.
///
/// Panics unless:
/// - outside a repo, ⌘K for `switch branch` lists no `Switch branch…`;
/// - the rows of claude, tests and shell show `main`; the SSH rows (deploy, logs) show no branch, and
///   opening the switcher for an SSH terminal opens nothing;
/// - a click on tests' row's branch opens the switcher (`Switch branch · repo`, read from the
///   repo) and leaves the selected workspace as it was (not a row click, no drag); Escape closes it;
/// - ⌘K with `switch branch` typed lists `Switch branch…`; Enter opens the switcher for the
///   focused terminal;
/// - a click on the status bar's branch opens it; choosing `feature` shows `Switch to feature?`
///   and `claude is running in this folder` (the program in claude's terminal, found in the
///   repo by its real path).
fn branch_switcher_open_scene() {
    use crate::palette_items::{Key, Line, Scope};
    use crate::sidebar::SidebarRow;
    use crate::status_bar::{BRANCH_GAP, BRANCH_GLYPH, ITEM_GAP, MARK, MARK_GAP, PAD};
    use crate::ui_tokens::size::STATUS_BAR;
    use gpui::{FontWeight, Modifiers};
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::time::Instant;

    let Some(git) = crate::git_branch::find_git() else {
        println!("ui-shots: branch-switcher-open skipped (no git on this Mac)");
        return;
    };
    let base =
        std::env::temp_dir().join(format!("fuxx-ui-shots-branch-open-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let repo = base.join("repo");
    std::fs::create_dir_all(&repo).expect("make the repo");
    scratch_git(&git, &repo, &["init", "-q", "-b", "main"]);
    std::fs::write(repo.join("a.txt"), "a\n").unwrap();
    scratch_git(&git, &repo, &["add", "."]);
    scratch_git(&git, &repo, &["commit", "-q", "-m", "one"]);
    scratch_git(&git, &repo, &["branch", "feature"]);
    // A second repo, for the terminal whose row is clicked (each opener must use its own).
    let other = base.join("other");
    std::fs::create_dir_all(&other).expect("make the second repo");
    scratch_git(&git, &other, &["init", "-q", "-b", "main"]);
    std::fs::write(other.join("b.txt"), "b\n").unwrap();
    scratch_git(&git, &other, &["add", "."]);
    scratch_git(&git, &other, &["commit", "-q", "-m", "one"]);

    let n = "branch-switcher-open";
    let (slug, name) = shot_themes()[0];
    let theme = shot_theme(name);
    let dims = size(px(960.0), px(600.0));
    let slot: Slot = Rc::new(RefCell::new(None));
    let build = palette_build(theme, slot.clone());
    let (folder, folder2) = (repo.clone(), other.clone());
    let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
        let ws = slotted(&slot);
        let is_listed = |cx: &mut VisualTestAppContext| {
            let palette = palette_of(cx, &ws).expect("the palette is open");
            cx.read(|cx| {
                palette.read(cx).lines().iter().any(|l| {
                    matches!(l, Line::Item { key: Key::Action(a), .. } if *a == "Switch branch\u{2026}")
                })
            })
        };
        let ask_palette = |cx: &mut VisualTestAppContext| {
            open_palette_on(cx, window, &ws, Scope::Actions);
            cx.simulate_input(window, "switch branch");
            settle(cx, window);
        };

        // Outside a repo, ⌘K has no such command.
        ask_palette(cx);
        assert!(!is_listed(cx), "{n}: no command outside a repo");
        press_keys(cx, window, "escape");

        // claude (a program runs in it) and shell (focused) are in the repo, by its path
        // through `/var`.
        let (claude, tests, shell, ssh) = cx.read(|cx| {
            let ids: Vec<u64> = ws.read(cx).sessions.items().iter().map(|s| s.id).collect();
            (ids[0], ids[2], ids[3], vec![ids[6], ids[7]])
        });
        cx.update_window(window, |_, _, cx| {
            ws.update(cx, |ws, cx| {
                for (id, at) in [(claude, &folder), (tests, &folder2), (shell, &folder)] {
                    ws.places.set_folder(id, Some(at.clone()), Instant::now());
                }
                ws.places.set_branch(&folder, Some("main".into()));
                ws.places.set_branch(&folder2, Some("main".into()));
                ws.programs
                    .set_for_shots(claude, ProgramState::Running("claude".into()));
                ws.sync();
                // As the lookups that find these do (M11p: the rows are kept until a notify).
                cx.notify();
            })
        })
        .expect("point the terminals at the repo");
        settle(cx, window);

        // The rows: an SSH row has its host (no row shows a branch any more).
        let rows = cx.read(|cx| ws.read(cx).rows_in_view(Instant::now()));
        let at = rows
            .iter()
            .position(|r| matches!(r, SidebarRow::Workspace(w) if w.label == "tests"))
            .expect("tests' row");
        for (i, r) in rows.iter().enumerate() {
            let SidebarRow::Workspace(w) = r else {
                continue;
            };
            if matches!(w.label.as_str(), "deploy" | "logs") {
                assert!(w.folder.is_some() && i > 0, "{n}: an SSH row has its host");
            }
        }
        for id in ssh {
            cx.update_window(window, |_, window, cx| {
                ws.update(cx, |ws, cx| ws.open_branch_switcher(id, window, cx))
            })
            .expect("ask for an SSH terminal's");
            settle(cx, window);
            assert!(
                branch_switcher_of(cx, &ws).is_none(),
                "{n}: nothing opens for an SSH terminal"
            );
        }

        // The switcher for shell's terminal, as the Changes bar's branch opens it (M11v: the
        // sidebar's group header no longer has a branch); the selection (shell) stays. The rows
        // say who and where, never the branch (M11al §2.4: it left the rows).
        let selected = cx.read(|cx| ws.read(cx).model.focused_workspace());
        if let SidebarRow::Workspace(w) = &rows[at] {
            assert!(
                matches!(
                    crate::sidebar::line_two(w),
                    Some(crate::sidebar::LineTwo::WhoWhere { .. })
                ),
                "{n}: tests' row says who and where, no branch"
            );
        }
        cx.update_window(window, |_, window, cx| {
            ws.update(cx, |ws, cx| ws.open_branch_switcher(shell, window, cx))
        })
        .expect("open shell's switcher");
        settle(cx, window);
        let sw = branch_switcher_of(cx, &ws).expect("the bar opens it");
        assert!(
            settle_until(cx, window, 10, |cx| cx
                .read(|cx| sw.read(cx).title() == "Switch branch \u{b7} repo")),
            "{n}: the repo is read: {}",
            cx.read(|cx| sw.read(cx).title())
        );
        assert_eq!(
            cx.read(|cx| ws.read(cx).model.focused_workspace()),
            selected,
            "{n}: opening it did not select another row"
        );
        press_keys(cx, window, "escape");
        assert!(branch_switcher_of(cx, &ws).is_none(), "{n}: Escape closes");

        // ⌘K.
        ask_palette(cx);
        assert!(is_listed(cx), "{n}: the command is listed in a repo");
        press_keys(cx, window, "enter");
        let sw = branch_switcher_of(cx, &ws).expect("the command opens it");
        assert!(
            settle_until(cx, window, 10, |cx| cx
                .read(|cx| sw.read(cx).title() == "Switch branch \u{b7} repo")),
            "{n}: ⌘K's switcher reads the repo"
        );
        press_keys(cx, window, "escape");
        assert!(branch_switcher_of(cx, &ws).is_none(), "{n}: Escape closes");

        // The status bar's branch (shell's, the focused one): after the mark, the word and the
        // glyph.
        let facts = cx
            .read(|cx| ws.read(cx).status_bar_facts())
            .expect("the bar's facts");
        assert_eq!(
            facts.branch.as_deref(),
            Some("main"),
            "{n}: the bar's branch"
        );
        let word = cx
            .update_window(window, |_, window, _| {
                crate::status_bar::text_width(&facts.text, FontWeight::NORMAL, false, window)
            })
            .expect("measure the word");
        let x = PAD + MARK + MARK_GAP + word + ITEM_GAP + BRANCH_GLYPH + BRANCH_GAP + 6.0;
        let y = f32::from(dims.height) - STATUS_BAR / 2.0;
        cx.simulate_click(window, gpui::point(px(x), px(y)), Modifiers::none());
        settle(cx, window);
        let sw = branch_switcher_of(cx, &ws).expect("a click on the bar's branch opens it");
        assert!(
            settle_until(cx, window, 10, |cx| cx.read(|cx| sw
                .read(cx)
                .line_texts()
                .contains(&"feature \u{b7} local only".to_string()))),
            "{n}: the repo is read: {:?}",
            cx.read(|cx| sw.read(cx).line_texts())
        );
        assert_eq!(
            cx.read(|cx| sw.read(cx).title()),
            "Switch branch \u{b7} repo",
            "{n}: the bar opens the followed terminal's repo"
        );
        for _ in 0..4 {
            if cx.read(|cx| sw.read(cx).selected_label()).as_deref() == Some("feature") {
                break;
            }
            press_keys(cx, window, "down");
        }
        press_keys(cx, window, "enter");
        assert_eq!(
            cx.read(|cx| sw.read(cx).confirm_texts()),
            Some(vec![
                "Switch to feature?".to_string(),
                "claude is running in this folder".to_string()
            ]),
            "{n}: the program in claude's terminal is named"
        );
        slot.borrow_mut().take();
    };
    let shot = render_with(&theme, dims, build, act);
    save_shot(n, slug, &shot);
    let _ = std::fs::remove_dir_all(&base);
    println!("ui-shots: {n} ok");
}

/// What git says when a switch would overwrite uncommitted work (the confirm scene's stub).
const SWITCH_REFUSED: &str =
    "error: Your local changes to the following files would be overwritten by checkout:";

/// Draw frames until `done` holds, at most `secs` seconds (real git answers from the background
/// executor's threads).
pub(super) fn settle_until(
    cx: &mut VisualTestAppContext,
    window: AnyWindowHandle,
    secs: u64,
    mut done: impl FnMut(&mut VisualTestAppContext) -> bool,
) -> bool {
    let start = std::time::Instant::now();
    loop {
        settle(cx, window);
        if done(cx) {
            return true;
        }
        if start.elapsed() > std::time::Duration::from_secs(secs) {
            return false;
        }
        prof::sleep(std::time::Duration::from_millis(20));
    }
}

/// The branch switcher's confirmation (M11o Task 4) over `switcher_workspace`, in both themes,
/// seeded on `switcher_snapshot` with a second dirty file (README.md), Claude Code running in
/// the folder, feature/old conflicting (3 files) and feature/rows counted clean (0):
/// - `branch-switcher-confirm`: Enter on feature/rows (chosen first): the step reads exactly
///   `Switch to feature/rows?`, `Claude Code is running in this folder`, `2 uncommitted
///   file(s) come along`, over `Cancel` and `Switch` (both enabled; Switch filled with the
///   accent).
/// - `branch-switcher-confirm-refused`: from there Escape (back to the list, still open,
///   feature/rows still chosen), then Enter, Enter with the switch stubbed to fail: the list
///   again (same lines, feature/rows chosen), git's message under the box in `ring_waiting`.
///
/// And (not shot): on the confirmation ↑/↓ and typing change nothing; a click on Cancel goes
/// back to the list; a click on spike's row opens spike's confirmation (Claude Code running, 2
/// files carried: spike is counted clean); Enter with the switch stubbed to succeed closes the
/// switcher (`Switched`, no git run), the active terminal has the keyboard back.
fn branch_switcher_confirm_scene() {
    use crate::branch_switcher::BranchSwitcher;
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    let dims = size(px(960.0), px(600.0));
    let want_lines = [
        "main · current",
        "feature/rows · ↑2 ↓1",
        "spike · local only",
        "feature/old · conflict 3 file(s)",
        "ORIGIN",
        "fix-login",
    ];
    let want_confirm = [
        "Switch to feature/rows?",
        "Claude Code is running in this folder",
        "2 uncommitted file(s) come along",
    ];
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let ui = theme.ui();
        for pass in ["confirm", "refused", "checks"] {
            let slot: Slot = Rc::new(RefCell::new(None));
            let build = palette_build(theme, slot.clone());
            let seen: Rc<Cell<Option<gpui::Bounds<Pixels>>>> = Rc::default();
            let found = seen.clone();
            let n = format!("branch-switcher-confirm-{pass}-{slug}");
            let seeded = |fail: Option<&'static str>| {
                move |window: &mut Window, cx: &mut Context<BranchSwitcher>| {
                    let mut snapshot = switcher_snapshot();
                    snapshot.dirty.insert(PathBuf::from("README.md"));
                    let conflicts = [
                        ("feature/old".to_string(), 3),
                        ("feature/rows".to_string(), 0),
                        ("spike".to_string(), 0),
                    ]
                    .into_iter()
                    .collect();
                    BranchSwitcher::seeded(
                        switcher_repo(),
                        vec!["Claude Code".into()],
                        snapshot,
                        conflicts,
                        fail.map(String::from),
                        window,
                        cx,
                    )
                }
            };
            let act = {
                let n = n.clone();
                move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                    let ws = slotted(&slot);
                    let sw = |cx: &mut VisualTestAppContext| {
                        branch_switcher_of(cx, &ws).expect("the switcher is open")
                    };
                    let confirm = |cx: &mut VisualTestAppContext| {
                        let s = sw(cx);
                        cx.read(|cx| s.read(cx).confirm_texts())
                    };
                    let buttons = |cx: &mut VisualTestAppContext| {
                        let s = sw(cx);
                        cx.read(|cx| s.read(cx).buttons())
                    };
                    let lines = |cx: &mut VisualTestAppContext| {
                        let s = sw(cx);
                        cx.read(|cx| s.read(cx).line_texts())
                    };
                    let chosen = |cx: &mut VisualTestAppContext| {
                        let s = sw(cx);
                        cx.read(|cx| s.read(cx).selected_label())
                    };
                    let note = |cx: &mut VisualTestAppContext| {
                        let s = sw(cx);
                        cx.read(|cx| s.read(cx).note())
                    };
                    let fail = (pass == "refused").then_some(SWITCH_REFUSED);
                    open_switcher_with(cx, window, &ws, seeded(fail));
                    assert_eq!(lines(cx), want_lines, "{n}: the lines");
                    assert_eq!(chosen(cx).as_deref(), Some("feature/rows"), "{n}: first");
                    assert_eq!(confirm(cx), None, "{n}: on the list");
                    press_keys(cx, window, "enter");
                    assert_eq!(
                        confirm(cx).as_deref(),
                        Some(&want_confirm.map(String::from)[..]),
                        "{n}: Enter opens the confirmation"
                    );
                    assert_eq!(
                        buttons(cx),
                        [("Cancel", true), ("Switch", true)],
                        "{n}: its buttons"
                    );
                    match pass {
                        "confirm" => {
                            let s = sw(cx);
                            found.set(cx.read(|cx| s.read(cx).switch_bounds()));
                        }
                        "refused" => {
                            press_keys(cx, window, "escape");
                            assert!(
                                branch_switcher_of(cx, &ws).is_some(),
                                "{n}: Escape on the confirmation leaves it open"
                            );
                            assert_eq!(confirm(cx), None, "{n}: Escape goes back to the list");
                            assert_eq!(chosen(cx).as_deref(), Some("feature/rows"), "{n}: kept");
                            press_keys(cx, window, "enter");
                            assert!(confirm(cx).is_some(), "{n}: Enter opens it again");
                            // Switch (without running the executor: the stub's answer waits),
                            // then the terminal takes the keyboard while git works.
                            cx.dispatch_keystroke(
                                window,
                                gpui::Keystroke::parse("enter").expect("a keystroke"),
                            );
                            let s = sw(cx);
                            cx.update_window(window, |_, window, cx| {
                                ws.update(cx, |ws, cx| ws.focus_active(window, cx));
                                s.update(cx, |s, cx| s.report_blur(cx));
                            })
                            .expect("the terminal takes the keyboard");
                            assert!(
                                active_terminal_focused(cx, window, &ws),
                                "{n}: the terminal has the keyboard while git works"
                            );
                            assert!(
                                branch_switcher_of(cx, &ws).is_some()
                                    && cx.read(|cx| s.read(cx).busy()),
                                "{n}: losing the keyboard while busy leaves it open, busy"
                            );
                            assert!(
                                settle_until(cx, window, 3, |cx| !cx.read(|cx| s.read(cx).busy())),
                                "{n}: the switch ends"
                            );
                            let boxed = cx
                                .update_window(window, |_, window, cx| {
                                    use gpui::Focusable;
                                    s.focus_handle(cx).is_focused(window)
                                })
                                .expect("read the focus");
                            assert!(boxed, "{n}: its box has the keyboard back");
                            assert_eq!(confirm(cx), None, "{n}: a refusal goes back to the list");
                            assert_eq!(
                                note(cx).as_deref(),
                                Some(SWITCH_REFUSED),
                                "{n}: git's message under the box"
                            );
                            assert_eq!(lines(cx), want_lines, "{n}: the lines again");
                            assert_eq!(chosen(cx).as_deref(), Some("feature/rows"), "{n}: kept");
                            found.set(cx.read(|cx| s.read(cx).note_bounds()));
                        }
                        _ => {
                            // On the confirmation, ↑/↓ and typing change nothing.
                            press_keys(cx, window, "down");
                            cx.simulate_input(window, "zz");
                            settle(cx, window);
                            assert_eq!(
                                confirm(cx).as_deref(),
                                Some(&want_confirm.map(String::from)[..]),
                                "{n}: ↓ and typing leave the confirmation"
                            );
                            let s = sw(cx);
                            assert_eq!(
                                cx.read(|cx| s.read(cx).query_text(cx)),
                                "",
                                "{n}: typing is ignored on it"
                            );
                            // Fetch can't be pressed on the confirmation.
                            let fetch = cx
                                .read(|cx| s.read(cx).fetch_bounds())
                                .expect("Fetch is drawn");
                            cx.simulate_click(window, fetch.center(), gpui::Modifiers::none());
                            settle(cx, window);
                            assert_eq!(
                                cx.read(|cx| s.read(cx).fetch_status()),
                                None,
                                "{n}: Fetch is inert on the confirmation"
                            );
                            // A click on Cancel: back to the list.
                            let cancel = cx
                                .read(|cx| s.read(cx).cancel_bounds())
                                .expect("Cancel is drawn");
                            cx.simulate_click(window, cancel.center(), gpui::Modifiers::none());
                            settle(cx, window);
                            assert!(branch_switcher_of(cx, &ws).is_some(), "{n}: still open");
                            assert_eq!(confirm(cx), None, "{n}: Cancel goes back to the list");
                            assert_eq!(chosen(cx).as_deref(), Some("feature/rows"), "{n}: kept");
                            // A click on spike's row: its confirmation.
                            let spike = cx
                                .read(|cx| s.read(cx).line_bounds(2))
                                .expect("spike is drawn");
                            cx.simulate_click(window, spike.center(), gpui::Modifiers::none());
                            settle(cx, window);
                            assert_eq!(
                                confirm(cx).as_deref(),
                                Some(
                                    &[
                                        "Switch to spike?",
                                        "Claude Code is running in this folder",
                                        "2 uncommitted file(s) come along",
                                    ]
                                    .map(String::from)[..]
                                ),
                                "{n}: a click on spike opens its confirmation"
                            );
                            // Enter: the stub switches; the switcher closes.
                            press_keys(cx, window, "enter");
                            assert!(
                                settle_until(cx, window, 3, |cx| branch_switcher_of(cx, &ws)
                                    .is_none()),
                                "{n}: a switch closes it"
                            );
                            assert!(
                                active_terminal_focused(cx, window, &ws),
                                "{n}: the terminal has the keyboard back"
                            );
                            // A typed name chosen while a fetch runs waits for it (Busy, looking).
                            open_switcher_with(cx, window, &ws, seeded(None));
                            let s = sw(cx);
                            let fetch = cx
                                .read(|cx| s.read(cx).fetch_bounds())
                                .expect("Fetch is drawn");
                            cx.simulate_click(window, fetch.center(), gpui::Modifiers::none());
                            settle(cx, window);
                            cx.simulate_input(window, "zzz");
                            settle(cx, window);
                            press_keys(cx, window, "enter");
                            assert_eq!(
                                cx.read(|cx| s.read(cx).busy_text()).as_deref(),
                                Some("Looking for zzz on origin\u{2026}"),
                                "{n}: the typed name waits for the running fetch"
                            );
                            assert_eq!(
                                cx.read(|cx| s.read(cx).fetch_status()).as_deref(),
                                Some("Fetching\u{2026}"),
                                "{n}: the fetch still runs"
                            );
                            // Busy: no opener replaces it (the row's or the bar's branch, the
                            // command), and ⌘K opens nothing over it.
                            let folder = PathBuf::from("/Users/me/scratch/repo");
                            cx.update_window(window, |_, window, cx| {
                                ws.update(cx, |ws, cx| {
                                    let id = ws.sessions.active().expect("an active terminal").id;
                                    ws.places.set_folder(
                                        id,
                                        Some(folder.clone()),
                                        std::time::Instant::now(),
                                    );
                                    ws.places.set_branch(&folder, Some("main".into()));
                                    assert!(ws.switchable_folder(id).is_some());
                                    ws.open_branch_switcher(id, window, cx);
                                })
                            })
                            .expect("ask for another switcher");
                            settle(cx, window);
                            assert_eq!(
                                branch_switcher_of(cx, &ws).as_ref(),
                                Some(&s),
                                "{n}: a busy switcher isn't replaced"
                            );
                            press_keys(cx, window, "cmd-k");
                            assert!(palette_of(cx, &ws).is_none(), "{n}: no palette over it");
                            assert_eq!(
                                branch_switcher_of(cx, &ws).as_ref(),
                                Some(&s),
                                "{n}: still the busy one"
                            );
                        }
                    }
                    slot.borrow_mut().take();
                }
            };
            let shot = render_with(&theme, dims, build, act);
            if pass == "checks" {
                println!("ui-shots: {n} ok");
                continue;
            }
            let at = seen.get().expect("drawn");
            if pass == "confirm" {
                save_shot("branch-switcher-confirm", slug, &shot);
                let fill = count_along(
                    &shot,
                    dims,
                    ui.accent,
                    f32::from(at.top()) + 3.0,
                    f32::from(at.left()) + 6.0,
                    f32::from(at.right()) - 6.0,
                );
                let wide = f32::from(at.size.width) - 12.0;
                assert!(
                    fill as f32 >= wide * 0.8,
                    "{n}: Switch is filled with the accent ({fill} of {wide})"
                );
            } else {
                save_shot("branch-switcher-confirm-refused", slug, &shot);
                let red = count_along(
                    &shot,
                    dims,
                    ui.ring_waiting,
                    f32::from(at.center().y),
                    f32::from(at.left()),
                    f32::from(at.right()),
                );
                assert!(red >= 10, "{n}: git's message is in ring_waiting ({red})");
            }
            println!("ui-shots: {n} ok");
        }
    }
}

/// `git -C dir args…` for the scratch repos, hermetic (no user config, no signing, no hooks
/// from a global hooks path): its stdout. Panics if git fails.
pub(super) fn scratch_git(git: &std::path::Path, dir: &std::path::Path, args: &[&str]) -> String {
    let out = std::process::Command::new(git)
        .arg("-C")
        .arg(dir)
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .expect("run git");
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// The branch switcher on a real repo (M11o Task 4, not shot), in a scratch folder under the
/// system's temp folder: `origin` (trunk, other, feature/typed) and a single-branch clone of it
/// (trunk only from origin) with a local branch `other` that changes `a.txt`. The active
/// terminal's folder is the clone, its branch `trunk`, the places unfrozen just for the switch.
///
/// Panics unless:
/// - with `a.txt` changed and twenty branches `b00`…`b19` listed above `other` (so it is out of
///   view, never counted): choosing `other` while a read of a clean tree lands drops that
///   count (never shown) and counts again on the new read (`Switch to other?`, nothing
///   carried); with the dirty read back, choosing `other` counts it first (busy meanwhile),
///   then ends on the list with `other · conflict 1 file(s)` (disabled), no confirmation,
///   nothing switched;
/// - with `a.txt` back and an untracked `notes.txt`, choosing `other` shows `Switch to other?`
///   and `1 uncommitted file(s) come along`; a read landing with a second untracked file
///   rebuilds it (`2 uncommitted…`), and again without it (`1…`); Enter switches (git says `other`), `notes.txt` is
///   still there, the switcher closes, and the active terminal's row has `other` at once (no
///   5 s wait);
/// - typing `nope`, Enter: `Branch nope not found` under the box, still `other`, no `nope`
///   branch or origin ref made;
/// - typing `feature/typed`, Enter: fetched, `Switch to feature/typed?`; Enter: on this
///   single-branch clone it is made a local branch first (`branch --no-track`, the upstream
///   written), then switched to: git on `feature/typed`, only `notes.txt` uncommitted,
///   `branch.feature/typed.merge` set;
/// - with the clone's refspec widened (`remote set-branches origin '*'`) and `feature/late` made
///   on origin after the switcher's automatic fetch: typing it, Enter: fetched, `Switch to
///   feature/late?`; Enter switches (git says `feature/late`, tracking `origin/feature/late`).
fn branch_switcher_real_scene() {
    use crate::branch_switcher::{BranchSwitcher, RepoAt};
    use std::cell::RefCell;
    use std::rc::Rc;

    let Some(git) = crate::git_branch::find_git() else {
        println!("ui-shots: branch-switcher-real skipped (no git on this Mac)");
        return;
    };
    let base =
        std::env::temp_dir().join(format!("fuxx-ui-shots-branch-real-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let origin = base.join("origin");
    let clone = base.join("clone");
    std::fs::create_dir_all(&origin).expect("make origin");
    scratch_git(&git, &origin, &["init", "-q", "-b", "trunk"]);
    std::fs::write(origin.join("a.txt"), "a\n").unwrap();
    scratch_git(&git, &origin, &["add", "."]);
    scratch_git(&git, &origin, &["commit", "-q", "-m", "one"]);
    scratch_git(&git, &origin, &["switch", "-q", "-c", "feature/typed"]);
    std::fs::write(origin.join("b.txt"), "b\n").unwrap();
    scratch_git(&git, &origin, &["add", "."]);
    scratch_git(&git, &origin, &["commit", "-q", "-m", "typed"]);
    scratch_git(&git, &origin, &["switch", "-q", "trunk"]);
    scratch_git(
        &git,
        &base,
        &[
            "clone",
            "-q",
            "--single-branch",
            "-b",
            "trunk",
            origin.to_str().unwrap(),
            "clone",
        ],
    );
    scratch_git(&git, &clone, &["switch", "-q", "-c", "other"]);
    std::fs::write(clone.join("a.txt"), "other\n").unwrap();
    scratch_git(&git, &clone, &["commit", "-q", "-am", "other"]);
    scratch_git(&git, &clone, &["switch", "-q", "trunk"]);
    let current = |git: &std::path::Path| scratch_git(git, &clone, &["branch", "--show-current"]);

    let theme = shot_theme(shot_themes()[0].1);
    let dims = size(px(960.0), px(600.0));
    let slot: Slot = Rc::new(RefCell::new(None));
    let build = palette_build(theme, slot.clone());
    let (git2, clone2) = (git.clone(), clone.clone());
    let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
        let (git, clone) = (git2, clone2);
        let n = "branch-switcher-real";
        let ws = slotted(&slot);
        let open = |cx: &mut VisualTestAppContext, ws: &gpui::Entity<Workspace>| {
            let repo = RepoAt {
                git: git.clone(),
                dir: clone.clone(),
                label: "~/scratch/clone".into(),
            };
            open_switcher_with(cx, window, ws, |window, cx| {
                BranchSwitcher::new(repo, vec![], window, cx)
            });
            // Its read and the automatic fetch (from the folder next door) land.
            let sw = branch_switcher_of(cx, ws).expect("the switcher is open");
            assert!(
                settle_until(cx, window, 10, |cx| cx.read(|cx| {
                    let s = sw.read(cx);
                    s.line_texts().iter().any(|l| l.starts_with("other"))
                        && s.fetch_status().is_none()
                })),
                "{n}: the list is read"
            );
            sw
        };
        let choose =
            |cx: &mut VisualTestAppContext, sw: &gpui::Entity<BranchSwitcher>, label: &str| {
                for _ in 0..8 {
                    if cx.read(|cx| sw.read(cx).selected_label()).as_deref() == Some(label) {
                        break;
                    }
                    press_keys(cx, window, "down");
                }
                assert_eq!(
                    cx.read(|cx| sw.read(cx).selected_label()).as_deref(),
                    Some(label),
                    "{n}: {label} chosen"
                );
                press_keys(cx, window, "enter");
                assert!(
                    settle_until(cx, window, 10, |cx| !cx.read(|cx| sw.read(cx).busy())),
                    "{n}: choosing {label} ends"
                );
            };

        // A conflicting change, and other out of view (b00…b19 on its commit list first: same
        // date, by name): chosen before any count, it is counted first, then disabled.
        for i in 0..20 {
            scratch_git(&git, &clone, &["branch", &format!("b{i:02}"), "other"]);
        }
        std::fs::write(clone.join("a.txt"), "mine\n").unwrap();
        let sw = open(cx, &ws);
        assert_eq!(
            cx.read(|cx| sw.read(cx).line_texts())
                .last()
                .map(String::as_str),
            Some("other · local only"),
            "{n}: other is last, out of view and not counted"
        );
        // A read lands (here: the tree seen clean) while other's count runs: that count is of
        // the old tree, so it's dropped (not shown) and other is counted again on the new one
        // (clean: nothing to count, its confirmation at once, nothing carried).
        let mut clean = crate::branch_git::read(&git, &clone).expect("read the clone");
        clean.dirty.clear();
        let (chose, busy) = cx
            .update_window(window, |_, _, cx| {
                sw.update(cx, |s, cx| {
                    let chose = s.choose_label("other", cx);
                    let busy = s.busy();
                    s.land_read(clean, cx);
                    (chose, busy)
                })
            })
            .expect("choose other, then a read lands");
        assert!(chose && busy, "{n}: other is being counted");
        assert!(
            settle_until(cx, window, 10, |cx| !cx.read(|cx| sw.read(cx).busy())),
            "{n}: the stale count lands"
        );
        assert_eq!(
            cx.read(|cx| sw.read(cx).confirm_texts()),
            Some(vec!["Switch to other?".to_string()]),
            "{n}: counted again on the newer (clean) read: its confirmation, nothing carried"
        );
        press_keys(cx, window, "escape");
        assert_eq!(
            cx.read(|cx| sw.read(cx).line_texts())
                .last()
                .map(String::as_str),
            Some("other · local only"),
            "{n}: the stale count was never shown"
        );
        // The real (dirty) tree again.
        let dirty = crate::branch_git::read(&git, &clone).expect("read the clone");
        cx.update_window(window, |_, _, cx| {
            sw.update(cx, |s, cx| s.land_read(dirty, cx))
        })
        .expect("land the dirty read");
        settle(cx, window);
        let (chose, busy) = cx
            .update_window(window, |_, _, cx| {
                sw.update(cx, |s, cx| {
                    let chose = s.choose_label("other", cx);
                    (chose, s.busy())
                })
            })
            .expect("choose other");
        assert!(chose && busy, "{n}: choosing other counts it first (busy)");
        assert!(
            settle_until(cx, window, 10, |cx| !cx.read(|cx| sw.read(cx).busy())),
            "{n}: the count ends"
        );
        assert_eq!(
            cx.read(|cx| sw.read(cx).confirm_texts()),
            None,
            "{n}: no confirmation"
        );
        assert!(
            settle_until(cx, window, 10, |cx| cx
                .read(|cx| sw.read(cx).line_texts())
                .contains(&"other · conflict 1 file(s)".to_string())),
            "{n}: other conflicts: {:?}",
            cx.read(|cx| sw.read(cx).line_texts())
        );
        assert_eq!(current(&git), "trunk", "{n}: nothing switched");
        press_keys(cx, window, "escape");
        assert!(branch_switcher_of(cx, &ws).is_none(), "{n}: closed");
        for i in 0..20 {
            scratch_git(&git, &clone, &["branch", "-q", "-D", &format!("b{i:02}")]);
        }

        // No conflict: a carried file, the confirmation, the switch, the row at once.
        std::fs::write(clone.join("a.txt"), "a\n").unwrap();
        std::fs::write(clone.join("notes.txt"), "notes\n").unwrap();
        cx.update_window(window, |_, _, cx| {
            ws.update(cx, |ws, _| {
                let id = ws.sessions.active().expect("an active terminal").id;
                ws.places
                    .set_folder(id, Some(clone.clone()), std::time::Instant::now());
                ws.places.set_branch(&clone, Some("trunk".into()));
            })
        })
        .expect("point the terminal at the clone");
        let sw = open(cx, &ws);
        choose(cx, &sw, "other");
        assert_eq!(
            cx.read(|cx| sw.read(cx).confirm_texts()),
            Some(vec![
                "Switch to other?".to_string(),
                "1 uncommitted file(s) come along".to_string()
            ]),
            "{n}: other's confirmation"
        );
        // A read lands while it shows: rebuilt from it (counted again: the tree is dirty).
        let land = |cx: &mut VisualTestAppContext| {
            let read = crate::branch_git::read(&git, &clone).expect("read the clone");
            cx.update_window(window, |_, _, cx| {
                sw.update(cx, |s, cx| s.land_read(read, cx))
            })
            .expect("land a read");
            assert!(
                settle_until(cx, window, 10, |cx| !cx.read(|cx| sw.read(cx).busy())),
                "{n}: the recount ends"
            );
            cx.read(|cx| sw.read(cx).confirm_texts())
        };
        std::fs::write(clone.join("notes2.txt"), "more\n").unwrap();
        assert_eq!(
            land(cx),
            Some(vec![
                "Switch to other?".to_string(),
                "2 uncommitted file(s) come along".to_string()
            ]),
            "{n}: a newer read: two files come along now"
        );
        std::fs::remove_file(clone.join("notes2.txt")).unwrap();
        assert_eq!(
            land(cx),
            Some(vec![
                "Switch to other?".to_string(),
                "1 uncommitted file(s) come along".to_string()
            ]),
            "{n}: and one again"
        );
        cx.update_window(window, |_, _, cx| {
            ws.update(cx, |ws, _| ws.places.frozen = false)
        })
        .expect("set the places");
        press_keys(cx, window, "enter");
        assert!(
            settle_until(cx, window, 10, |cx| branch_switcher_of(cx, &ws).is_none()),
            "{n}: the switch closes it"
        );
        assert_eq!(current(&git), "other", "{n}: git is on other");
        assert!(
            clone.join("notes.txt").exists(),
            "{n}: notes.txt came along"
        );
        assert!(
            settle_until(cx, window, 3, |cx| cx
                .read(|cx| ws.read(cx).places.branch(&clone).map(String::from))
                .as_deref()
                == Some("other")),
            "{n}: the row has the new branch at once"
        );
        cx.update_window(window, |_, _, cx| {
            ws.update(cx, |ws, _| ws.places.frozen = true)
        })
        .expect("set the places");

        // A typed name origin doesn't have: not found, nothing made.
        let sw = open(cx, &ws);
        cx.simulate_input(window, "nope");
        settle(cx, window);
        choose(cx, &sw, "Check out nope from origin");
        assert_eq!(
            cx.read(|cx| sw.read(cx).note()).as_deref(),
            Some("Branch nope not found"),
            "{n}: not found"
        );
        assert_eq!(
            cx.read(|cx| sw.read(cx).confirm_texts()),
            None,
            "{n}: on the list"
        );
        assert_eq!(current(&git), "other", "{n}: still on other");
        let refs = scratch_git(&git, &clone, &["for-each-ref", "--format=%(refname)"]);
        assert!(!refs.contains("nope"), "{n}: no nope ref: {refs}");
        press_keys(cx, window, "escape");

        // A typed name origin has, not fetched by this single-branch clone (its refspec brings
        // trunk only): fetched alone, made a local branch with origin's as its upstream (as
        // every origin branch is, whatever the refspec), then switched to.
        let sw = open(cx, &ws);
        cx.simulate_input(window, "feature/typed");
        settle(cx, window);
        choose(cx, &sw, "Check out feature/typed from origin");
        assert_eq!(
            cx.read(|cx| sw.read(cx).confirm_texts()),
            Some(vec![
                "Switch to feature/typed?".to_string(),
                "1 uncommitted file(s) come along".to_string()
            ]),
            "{n}: feature/typed's confirmation"
        );
        press_keys(cx, window, "enter");
        assert!(
            settle_until(cx, window, 10, |cx| branch_switcher_of(cx, &ws).is_none()),
            "{n}: the switch closes it ({:?})",
            cx.read(|cx| sw.read(cx).note())
        );
        assert_eq!(
            current(&git),
            "feature/typed",
            "{n}: git is on feature/typed"
        );
        assert_eq!(
            scratch_git(&git, &clone, &["status", "--porcelain=v1"]),
            "?? notes.txt",
            "{n}: a clean switch, notes.txt came along"
        );
        assert_eq!(
            scratch_git(&git, &clone, &["config", "branch.feature/typed.merge"]),
            "refs/heads/feature/typed",
            "{n}: its upstream set"
        );
        println!("ui-shots: {n}: narrow clone switched to {}", current(&git));

        // A normal refspec, and a branch made on origin after the automatic fetch (so it has
        // no row): fetched, confirmed, switched, tracking origin's.
        scratch_git(&git, &clone, &["remote", "set-branches", "origin", "*"]);
        let sw = open(cx, &ws);
        scratch_git(&git, &origin, &["branch", "feature/late"]);
        cx.simulate_input(window, "feature/late");
        settle(cx, window);
        choose(cx, &sw, "Check out feature/late from origin");
        assert_eq!(
            cx.read(|cx| sw.read(cx).confirm_texts()),
            Some(vec![
                "Switch to feature/late?".to_string(),
                "1 uncommitted file(s) come along".to_string()
            ]),
            "{n}: feature/late's confirmation"
        );
        press_keys(cx, window, "enter");
        assert!(
            settle_until(cx, window, 10, |cx| branch_switcher_of(cx, &ws).is_none()),
            "{n}: the switch closes it ({:?})",
            cx.read(|cx| sw.read(cx).note())
        );
        assert_eq!(current(&git), "feature/late", "{n}: git is on feature/late");
        assert_eq!(
            scratch_git(
                &git,
                &clone,
                &["rev-parse", "--abbrev-ref", "feature/late@{upstream}"]
            ),
            "origin/feature/late",
            "{n}: tracking origin's"
        );
        assert!(
            clone.join("notes.txt").exists(),
            "{n}: notes.txt still there"
        );
        println!(
            "ui-shots: {n}: git -C {} branch --show-current = {}",
            clone.display(),
            current(&git)
        );
        slot.borrow_mut().take();
    };
    let _ = render_with(&theme, dims, build, act);
    let _ = std::fs::remove_dir_all(&base);
}
