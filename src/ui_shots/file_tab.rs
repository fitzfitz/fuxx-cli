//! The file tab (M11g, M11ae): the tab in a pane, its viewer, file bar, drag-select, overlay
//! scrollbar, and its Diff mode (M11h).

use super::branch::{scratch_git, settle_until};
use super::*;

/// This area's scenes (`mod.rs` runs them in its `ORDER`).
pub(super) const SCENES: &[Scene] = &[
    Scene {
        name: "file-tab",
        run: file_tab_scenes,
        themes: Themes::Both,
        serial: true,
        kind: Kind::Visual,
        sources: &[
            "src/file_tab.rs",
            "src/layout_model.rs",
            "src/tab_bar.rs",
            "src/highlight.rs",
            "src/git_status.rs",
            "src/git_branch.rs",
            "src/scrollbar.rs",
            "src/name_input.rs",
        ],
    },
    Scene {
        name: "file-diff",
        run: file_diff_scenes,
        themes: Themes::Both,
        serial: true,
        kind: Kind::Visual,
        sources: &[
            "src/file_tab.rs",
            "src/diff_view.rs",
            "src/git_status.rs",
            "src/highlight.rs",
            "src/layout_model.rs",
            "src/right_sidebar.rs",
            "src/file_tree.rs",
            "src/name_input.rs",
        ],
    },
    Scene {
        name: "file-tab-scrollbar",
        run: file_tab_scrollbar_scenes,
        themes: Themes::One,
        serial: true,
        kind: Kind::State,
        sources: &["src/file_tab.rs", "src/scrollbar.rs"],
    },
];

/// A file tab (M11g) in a pane beside a terminal's tab: opened twice for one file it is one tab,
/// shown, with the keyboard; its bar tab draws the file glyph; closing it leaves the terminal,
/// which takes the keyboard back. Its file isn't on this Mac, so it shows "This file no longer
/// exists", and its Close button closes the tab. Shot once per theme (opened). Then the viewer
/// itself (`file_view_scenes`).
fn file_tab_scenes() {
    use crate::layout_model::TabKind;
    use crate::tab_bar::look;
    use gpui::Focusable;
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    let dims = size(px(900.0), px(400.0));
    let file = PathBuf::from(format!("{API}/src/main.rs"));
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let ui = theme.ui();
        let tab_at: Rc<Cell<Option<gpui::Bounds<Pixels>>>> = Rc::default();
        let scene = |close: bool| {
            let slot: Slot = Rc::new(RefCell::new(None));
            let keep = slot.clone();
            let build = move |window: &mut Window, cx: &mut App| -> AnyView {
                cx.set_global(shot_appearance(theme, cx));
                let ws = cx.new(|cx| {
                    let mut ws = Workspace::for_shots(window, cx);
                    ws.sidebar.hint_dismissed = true;
                    let project = std::path::Path::new(API);
                    ws.open_shot_session("api", project, SHELL_TEXT, None, window, cx);
                    ws
                });
                *keep.borrow_mut() = Some(ws.clone());
                ws.into()
            };
            let file = file.clone();
            let seen = tab_at.clone();
            let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let ws = slotted(&slot);
                let n = format!("file-tab-{slug}");
                for _ in 0..2 {
                    cx.update_window(window, |_, window, cx| {
                        ws.update(cx, |ws, cx| {
                            ws.open_file_tab(
                                file.clone(),
                                crate::layout_model::FileMode::Plain,
                                window,
                                cx,
                            )
                        })
                    })
                    .expect("open the file");
                    cx.run_until_parked();
                }
                let pane = cx.read(|cx| ws.read(cx).model.focused_pane().expect("a pane"));
                let (kinds, file_tab) = cx.read(|cx| {
                    let ws = ws.read(cx);
                    let p = ws.model.pane(pane).expect("known");
                    let kinds: Vec<TabKind> = p.tabs.iter().map(|t| t.kind.clone()).collect();
                    assert_eq!(ws.model.focused_tab(), Some(p.tabs[1].id), "{n}: shown");
                    assert_eq!(ws.file_tabs.len(), 1, "{n}: one view");
                    assert_eq!(ws.focused_terminal(), None, "{n}: no terminal focused");
                    (kinds, p.tabs[1].id)
                });
                assert!(matches!(kinds[0], TabKind::Terminal(_)), "{n}");
                assert_eq!(
                    kinds[1..],
                    [TabKind::File {
                        path: file.clone(),
                        mode: crate::layout_model::FileMode::Plain,
                    }],
                    "{n}: one file tab for the file opened twice"
                );
                let keys = cx
                    .update_window(window, |_, window, cx| {
                        let ws = ws.read(cx);
                        let view = ws.focused_file_tab().expect("the file tab's view");
                        view.read(cx).focus_handle(cx).is_focused(window)
                    })
                    .expect("read");
                assert!(keys, "{n}: the file tab has the keyboard");
                seen.set(tab_bounds(cx, &ws, pane, 1));
                if close {
                    cx.update_window(window, |_, window, cx| {
                        ws.update(cx, |ws, cx| ws.close_tab(file_tab, window, cx))
                    })
                    .expect("close the file tab");
                    cx.run_until_parked();
                    cx.update_window(window, |_, window, cx| {
                        let ws = ws.read(cx);
                        let p = ws.model.pane(pane).expect("the pane stays");
                        assert_eq!(p.tabs.len(), 1, "{n}: the terminal's tab is left");
                        assert!(ws.file_tabs.is_empty(), "{n}: the view went");
                        let s = ws.sessions.active().expect("the terminal is active again");
                        assert_eq!(ws.focused_terminal(), Some(s.id), "{n}");
                        let keys = s.view.focus_handle(cx).is_focused(window);
                        assert!(keys, "{n}: the terminal has the keyboard back");
                    })
                    .expect("read");
                }
                slot.borrow_mut().take();
            };
            (build, act)
        };
        let (build, act) = scene(false);
        let path = shoot_with("file-tab", slug, &theme, dims, build.clone(), act.clone());
        println!("ui-shots: {}", path.display());
        // Its bar tab (active, in the focused pane) draws the glyph in the tab's text colour.
        let shot = render_with(&theme, dims, build, act);
        let tab = tab_at.get().expect("the file tab was laid out");
        let text = look(&ui, Status::Idle, true, true).text;
        let near = |p: [u8; 3]| {
            p.iter()
                .zip([text.r, text.g, text.b])
                .all(|(a, b)| a.abs_diff(b) <= 60)
        };
        let (x0, y) = (f32::from(tab.left()) + 8.0, f32::from(tab.center().y));
        let glyph = (0..12).any(|dx| {
            (-6..=6).any(|dy| near(pixel_at(&shot, dims, x0 + dx as f32, y + dy as f32)))
        });
        assert!(glyph, "file-tab-{slug}: the file glyph is drawn");
        // Closing it (not shot).
        let (build, act) = scene(true);
        render_with(&theme, dims, build, act);
        // Its file is gone (no such file here): its Close button closes the tab (not shot).
        let slot: Slot = Rc::new(RefCell::new(None));
        let keep = slot.clone();
        let build = move |window: &mut Window, cx: &mut App| -> AnyView {
            cx.set_global(shot_appearance(theme, cx));
            let ws = cx.new(|cx| {
                let mut ws = Workspace::for_shots(window, cx);
                ws.sidebar.hint_dismissed = true;
                let project = std::path::Path::new(API);
                ws.open_shot_session("api", project, SHELL_TEXT, None, window, cx);
                ws
            });
            *keep.borrow_mut() = Some(ws.clone());
            ws.into()
        };
        let file = file.clone();
        let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
            let ws = slotted(&slot);
            let n = format!("file-tab-close-{slug}");
            cx.update_window(window, |_, window, cx| {
                ws.update(cx, |ws, cx| {
                    ws.open_file_tab(
                        file.clone(),
                        crate::layout_model::FileMode::Plain,
                        window,
                        cx,
                    )
                })
            })
            .expect("open the file");
            cx.run_until_parked();
            cx.update_window(window, |_, window, _| window.refresh())
                .expect("refresh");
            cx.run_until_parked();
            let close = cx.read(|cx| {
                let view = ws.read(cx).focused_file_tab().expect("the view").clone();
                assert_eq!(view.read(cx).state_name(), "missing", "{n}");
                view.read(cx).close_bounds().expect("its Close button")
            });
            cx.simulate_click(window, close.center(), gpui::Modifiers::none());
            cx.read(|cx| {
                let ws = ws.read(cx);
                assert!(ws.file_tabs.is_empty(), "{n}: the tab closed");
                let pane = ws.model.focused_pane().expect("a pane");
                assert_eq!(ws.model.pane(pane).expect("known").tabs.len(), 1, "{n}");
            });
            slot.borrow_mut().take();
        };
        render_with(&theme, dims, build, act);
        println!("ui-shots: file-tab-close-{slug} ok");
    }
    file_view_scenes();
    file_header_scenes();
    file_drag_select_scenes();
}

/// M11ae Task 5: drag-select past the text's edge scrolls, a tab on its own in a window, in both
/// themes (Wrap off, as the scenes' appearance has it):
/// - `file-tab-drag-select`: a 400-line file; a press on line 3, the pointer 60 pt below the
///   text, then 300 ms with no moves: the auto-scroll runs, the text has scrolled and the
///   selection's moving end is at or past the last line in sight at the start (and still in
///   sight); 300 pt below, each step scrolls further; the release ends the task within 50 ms
///   and the scroll stays. Then from the middle to above the text: it scrolls back up, the
///   moving end in sight. Shot at the end: that second selection, the thumb showing.
/// - `file-tab-drag-select-wide` (asserts only): a 3,000-character line, the pointer right of
///   the text: it scrolls sideways and the selection grows along the line; then over the line
///   numbers (left of the text): it scrolls back left.
/// - `file-tab-drag-select-mode` (asserts only): a switch to Diff mid-drag ends the task.
/// - `file-tab-drag-select-hidden` (asserts only): the tab goes off screen mid-drag (`set_shown`,
///   as the workspace says each frame): the task ends within one step and the scroll stays.
///   Then the pointer in the right edge's zone (the thumb shows, hovered) leaves the window
///   (`MouseExitEvent`, no last move): 1.5 s later the thumb is gone.
fn file_drag_select_scenes() {
    use crate::file_tab::FileTab;
    use crate::layout_model::FileMode;
    use gpui::{Modifiers, MouseButton};
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::time::Duration;

    let dir = std::env::temp_dir().join("fuxx-ui-shots-file-tab-drag-select");
    std::fs::create_dir_all(&dir).expect("the fixture folder");
    let dir = std::fs::canonicalize(&dir).unwrap_or(dir);
    let long = dir.join("long.rs");
    let text: String = (1..=400)
        .map(|i| format!("let line_{i} = {i};\n"))
        .collect();
    std::fs::write(&long, text).expect("long.rs");
    let wide = dir.join("wide.rs");
    let wide_text = format!("// one wide line\nlet wide = \"{}\";\n", "x".repeat(3_000));
    std::fs::write(&wide, wide_text).expect("wide.rs");

    type TabSlot = Rc<RefCell<Option<gpui::Entity<FileTab>>>>;
    let dims = size(px(760.0), px(460.0));
    let none = Modifiers::none();
    let left = Some(MouseButton::Left);
    let frame = |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
        cx.update_window(window, |_, window, _| window.refresh())
            .expect("refresh");
        cx.run_until_parked();
    };
    let ms = Duration::from_millis;
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let build_for = |path: PathBuf, slot: TabSlot| {
            move |window: &mut Window, cx: &mut App| -> AnyView {
                cx.set_global(shot_appearance(theme, cx));
                let tab = cx.new(|cx| {
                    let mut tab = FileTab::new(path.clone(), FileMode::Plain, window, cx);
                    tab.frozen = true;
                    tab
                });
                *slot.borrow_mut() = Some(tab.clone());
                tab.into()
            }
        };

        // The 400-line file: below the text, held still, further out, released; then above.
        let n = format!("file-tab-drag-select-{slug}");
        let slot: TabSlot = Rc::default();
        let build = build_for(long.clone(), slot.clone());
        let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
            let tab = slot.borrow().clone().expect("the file tab");
            frame(cx, window);
            let look = |cx: &mut VisualTestAppContext| {
                cx.read(|cx| {
                    let t = tab.read(cx);
                    (t.autoscrolling(), t.scroll(), t.selection())
                })
            };
            let g = cx.read(|cx| tab.read(cx).geom()).expect("drawn");
            let (lh, height) = (g.line_height, g.bounds.size.height);
            let x = g.bounds.left() + g.text_left + px(20.0);
            let last_in_sight = ((height - px(1.0)) / lh).floor() as usize;
            let line3 = point(x, g.bounds.top() + lh * 2.5);
            cx.simulate_mouse_down(window, line3, MouseButton::Left, none);
            let near = point(x, g.bounds.bottom() + px(60.0));
            cx.simulate_mouse_move(window, near, left, none);
            // Real time, as the scrollbar's fade reads the clock; the test clock for the steps.
            prof::sleep(ms(300));
            cx.advance_clock(ms(300));
            frame(cx, window);
            let (on, scroll, selection) = look(cx);
            assert!(on, "{n}: no auto-scroll 300 ms after leaving the text");
            assert!(scroll.y > px(0.0), "{n}: the text didn't scroll");
            let (anchor, head) = selection.expect("a selection");
            assert_eq!(anchor.line, 2, "{n}: the press was on line 3");
            assert!(
                head.line >= last_in_sight,
                "{n}: the moving end {head:?} is above the last line in sight at the start \
                 ({last_in_sight})"
            );
            let bottom_line = ((scroll.y + height - px(1.0)) / lh).floor() as usize;
            assert!(
                head.line <= bottom_line,
                "{n}: the moving end {head:?} went out of sight (last in sight {bottom_line})"
            );
            // Each step's scroll, 3 steps at 60 pt out, then 3 at 300 pt.
            let per_step = |cx: &mut VisualTestAppContext| {
                let from = look(cx).1.y;
                cx.advance_clock(ms(48));
                (look(cx).1.y - from) / 3.0
            };
            let slow = per_step(cx);
            let far = point(x, g.bounds.bottom() + px(300.0));
            cx.simulate_mouse_move(window, far, left, none);
            let fast = per_step(cx);
            assert!(
                slow > px(0.0) && fast > slow,
                "{n}: {fast:?} a step 300 pt out, {slow:?} 60 pt out"
            );
            // The release: the task ends within 50 ms, the scroll stays.
            cx.simulate_mouse_up(window, far, MouseButton::Left, none);
            let held = look(cx).1;
            cx.advance_clock(ms(50));
            frame(cx, window);
            let (on, scroll, _) = look(cx);
            assert!(!on, "{n}: the auto-scroll runs 50 ms after the release");
            assert_eq!(scroll, held, "{n}: the scroll moved after the release");
            assert!(
                !cx.read(|cx| tab.read(cx).selected_text()).is_empty(),
                "{n}: nothing selected"
            );

            // Above the text: it scrolls back up.
            let middle = point(x, g.bounds.center().y);
            cx.simulate_mouse_down(window, middle, MouseButton::Left, none);
            let above = point(x, g.bounds.top() - px(60.0));
            cx.simulate_mouse_move(window, above, left, none);
            cx.advance_clock(ms(160));
            let (on, scroll, selection) = look(cx);
            assert!(on, "{n}: no auto-scroll above the text");
            assert!(scroll.y < held.y, "{n}: didn't scroll up ({scroll:?})");
            let (_, head) = selection.expect("a selection");
            let top_line = (scroll.y / lh).floor() as usize;
            assert!(
                head.line >= top_line,
                "{n}: the moving end {head:?} went above the text (first in sight {top_line})"
            );
            cx.simulate_mouse_up(window, above, MouseButton::Left, none);
            cx.advance_clock(ms(50));
            assert!(!look(cx).0, "{n}: the auto-scroll runs after the release");
            slot.borrow_mut().take();
        };
        let path = shoot_with("file-tab-drag-select", slug, &theme, dims, build, act);
        println!("ui-shots: {}", path.display());

        // A 3,000-character line, the pointer right of the text: it scrolls sideways.
        let n = format!("file-tab-drag-select-wide-{slug}");
        let slot: TabSlot = Rc::default();
        let build = build_for(wide.clone(), slot.clone());
        let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
            let tab = slot.borrow().clone().expect("the file tab");
            frame(cx, window);
            let g = cx.read(|cx| tab.read(cx).geom()).expect("drawn");
            let y = g.bounds.top() + g.line_height * 1.5;
            let start = point(g.bounds.left() + g.text_left + px(20.0), y);
            cx.simulate_mouse_down(window, start, MouseButton::Left, none);
            let right = point(g.bounds.right() + px(60.0), y);
            cx.simulate_mouse_move(window, right, left, none);
            let edge = cx
                .read(|cx| tab.read(cx).selection())
                .expect("a selection")
                .1;
            cx.advance_clock(ms(300));
            frame(cx, window);
            let (on, scroll, selection) = cx.read(|cx| {
                let t = tab.read(cx);
                (t.autoscrolling(), t.scroll(), t.selection())
            });
            assert!(on, "{n}: no auto-scroll right of the text");
            assert!(scroll.x > px(0.0), "{n}: didn't scroll sideways");
            assert_eq!(scroll.y, px(0.0), "{n}: scrolled down (2 lines fit)");
            let (_, head) = selection.expect("a selection");
            assert!(
                head.line == 1 && head.col > edge.col,
                "{n}: the moving end {head:?} didn't follow the scroll from {edge:?}"
            );
            // Over the line numbers: left of the text, so it scrolls back left.
            let numbers = point(g.bounds.left() + g.text_left / 2.0, y);
            cx.simulate_mouse_move(window, numbers, left, none);
            cx.advance_clock(ms(160));
            let (on, back) = cx.read(|cx| {
                let t = tab.read(cx);
                (t.autoscrolling(), t.scroll())
            });
            assert!(on, "{n}: no auto-scroll over the line numbers");
            assert!(
                back.x < scroll.x,
                "{n}: didn't scroll left over the numbers ({back:?} from {scroll:?})"
            );
            cx.simulate_mouse_up(window, numbers, MouseButton::Left, none);
            cx.advance_clock(ms(50));
            assert!(
                !cx.read(|cx| tab.read(cx).autoscrolling()),
                "{n}: the auto-scroll runs after the release"
            );
            slot.borrow_mut().take();
        };
        render_with(&theme, dims, build, act);
        println!("ui-shots: file-tab-drag-select-wide-{slug} ok");

        // A switch to Diff mid-drag: the task ends.
        let n = format!("file-tab-drag-select-mode-{slug}");
        let slot: TabSlot = Rc::default();
        let build = build_for(long.clone(), slot.clone());
        let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
            let tab = slot.borrow().clone().expect("the file tab");
            frame(cx, window);
            let g = cx.read(|cx| tab.read(cx).geom()).expect("drawn");
            let x = g.bounds.left() + g.text_left + px(20.0);
            let press = point(x, g.bounds.top() + g.line_height * 2.5);
            cx.simulate_mouse_down(window, press, MouseButton::Left, none);
            let below = point(x, g.bounds.bottom() + px(60.0));
            cx.simulate_mouse_move(window, below, left, none);
            cx.advance_clock(ms(100));
            assert!(
                cx.read(|cx| tab.read(cx).autoscrolling()),
                "{n}: no auto-scroll below the text"
            );
            cx.update(|cx| tab.update(cx, |t, cx| t.choose_mode_for_shots(FileMode::Diff, cx)));
            cx.advance_clock(ms(50));
            assert!(
                !cx.read(|cx| tab.read(cx).autoscrolling()),
                "{n}: the auto-scroll runs after the switch to Diff"
            );
            cx.simulate_mouse_up(window, below, MouseButton::Left, none);
            slot.borrow_mut().take();
        };
        render_with(&theme, dims, build, act);
        println!("ui-shots: file-tab-drag-select-mode-{slug} ok");

        // Off screen mid-drag: the task ends; then the pointer leaves the window from the edge.
        let n = format!("file-tab-drag-select-hidden-{slug}");
        let slot: TabSlot = Rc::default();
        let build = build_for(long.clone(), slot.clone());
        let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
            let tab = slot.borrow().clone().expect("the file tab");
            frame(cx, window);
            cx.update(|cx| tab.update(cx, |t, cx| t.set_shown(true, cx)));
            let g = cx.read(|cx| tab.read(cx).geom()).expect("drawn");
            let x = g.bounds.left() + g.text_left + px(20.0);
            let press = point(x, g.bounds.top() + g.line_height * 2.5);
            cx.simulate_mouse_down(window, press, MouseButton::Left, none);
            let below = point(x, g.bounds.bottom() + px(60.0));
            cx.simulate_mouse_move(window, below, left, none);
            cx.advance_clock(ms(100));
            assert!(
                cx.read(|cx| tab.read(cx).autoscrolling()),
                "{n}: no auto-scroll below the text"
            );
            cx.update(|cx| tab.update(cx, |t, cx| t.set_shown(false, cx)));
            cx.advance_clock(ms(16));
            let held = cx.read(|cx| {
                let t = tab.read(cx);
                assert!(
                    !t.autoscrolling(),
                    "{n}: the auto-scroll runs a step after the tab went off screen"
                );
                t.scroll()
            });
            cx.advance_clock(ms(100));
            assert_eq!(
                cx.read(|cx| tab.read(cx).scroll()),
                held,
                "{n}: the scroll moved off screen"
            );
            // The release never comes (the listeners went with the canvas): shown again, a move
            // with the button up starts nothing.
            cx.update(|cx| tab.update(cx, |t, cx| t.set_shown(true, cx)));
            cx.simulate_mouse_move(window, below, None, none);
            cx.advance_clock(ms(100));
            assert!(
                !cx.read(|cx| tab.read(cx).autoscrolling()),
                "{n}: the auto-scroll started again"
            );

            // The pointer in the right edge's zone: the thumb shows, hovered; then it leaves
            // the window with no last move.
            let edge = point(g.bounds.right() - px(4.0), g.bounds.center().y);
            cx.simulate_mouse_move(window, edge, None, none);
            frame(cx, window);
            assert!(
                cx.read(|cx| tab.read(cx).scrollbar_shown()),
                "{n}: no thumb with the pointer at the edge"
            );
            cx.simulate_event(
                window,
                gpui::MouseExitEvent {
                    position: point(g.bounds.right() + px(2.0), g.bounds.center().y),
                    pressed_button: None,
                    modifiers: none,
                },
            );
            // Real time for the fade (it reads `Instant`), the test clock for its task's steps.
            prof::sleep(ms(1500));
            cx.advance_clock(ms(1500));
            frame(cx, window);
            assert!(
                !cx.read(|cx| tab.read(cx).scrollbar_shown()),
                "{n}: the thumb stays after the pointer left the window"
            );
            slot.borrow_mut().take();
        };
        render_with(&theme, dims, build, act);
        println!("ui-shots: file-tab-drag-select-hidden-{slug} ok");
    }
    std::fs::remove_dir_all(&dir).ok();
}

/// M11ae's file bar (option B), on a scratch repo with real git, a tab on its own:
/// - `file-tab-header` (both themes): `src/a.rs`, committed then edited (+2 −1): its name, the
///   chip `M · +2 −1`, and `<repo> / src · Rust · 4 lines · edited just now`.
/// - `file-tab-header-narrow` (both themes): the repo's README.md in a 320 pt window: no chip
///   (unchanged), File | Diff | Preview, Wrap and Open in Editor all inside the pane.
///
/// Not shot: an untracked file (`A · +3`); a file outside any repo (no chip, its folder); a
/// line appended to a.rs and read again (the line count and the counts follow, the counts
/// after the look-up's wait); a.rs opened in Diff mode (`file-tab-header-diff`: the chip
/// `M · +2 −1` from the diff's rows, with no switch to File; reverted to as committed and read
/// again: no chip); new.rs in Diff mode (`A · +3`).
fn file_header_scenes() {
    use crate::file_tab::FileTab;
    use crate::layout_model::FileMode;
    use std::cell::RefCell;
    use std::rc::Rc;

    let Some(git) = crate::git_branch::find_git() else {
        println!("ui-shots: file-tab-header skipped (no git on this Mac)");
        return;
    };
    let base = std::env::temp_dir().join(format!("fuxx-ui-shots-header-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let repo = base.join("header-repo");
    std::fs::create_dir_all(repo.join("src")).expect("make the repo");
    let repo = std::fs::canonicalize(&repo).unwrap_or(repo);
    scratch_git(&git, &repo, &["init", "-q", "-b", "main"]);
    let a_rs = repo.join("src/a.rs");
    std::fs::write(&a_rs, "fn a() {}\nfn b() {}\nfn c() {}\n").expect("a.rs");
    let readme = repo.join("README.md");
    std::fs::write(&readme, "# Read me\n\nSome *words*.\n").expect("README.md");
    // Deep enough that its Where is cut in a 480 pt pane while the rest fits.
    let deep_dir = repo.join("src/some/deeper/folders/for/the/where/part");
    std::fs::create_dir_all(&deep_dir).expect("the deep folders");
    let deep_rs = deep_dir.join("b.rs");
    std::fs::write(&deep_rs, "fn b() {}\n").expect("b.rs");
    scratch_git(&git, &repo, &["add", "."]);
    scratch_git(&git, &repo, &["commit", "-q", "-m", "one"]);
    // Line 2 changed (−1 +1) and a line added (+1).
    std::fs::write(&a_rs, "fn a() {}\nfn bee() {}\nfn c() {}\nfn d() {}\n").expect("edit a.rs");
    let new_rs = repo.join("src/new.rs");
    std::fs::write(&new_rs, "fn one() {}\nfn two() {}\nfn three() {}\n").expect("new.rs");
    let loose = base.join("loose");
    std::fs::create_dir_all(&loose).expect("a folder outside any repo");
    let notes = loose.join("notes.txt");
    std::fs::write(&notes, "a note\n").expect("notes.txt");
    let repo_name = repo
        .file_name()
        .expect("the repo's name")
        .to_string_lossy()
        .into_owned();

    type TabSlot = Rc<RefCell<Option<gpui::Entity<FileTab>>>>;
    type Words = (String, Option<String>, String);
    let tab_of = |slot: &TabSlot| slot.borrow().clone().expect("the file tab");
    let build_for = |theme: Theme, path: PathBuf, slot: TabSlot| {
        move |window: &mut Window, cx: &mut App| -> AnyView {
            cx.set_global(shot_appearance(theme, cx));
            // Not frozen: git is asked for real.
            let tab = cx.new(|cx| FileTab::new(path.clone(), FileMode::Plain, window, cx));
            *slot.borrow_mut() = Some(tab.clone());
            tab.into()
        }
    };
    let words = |cx: &mut VisualTestAppContext, tab: &gpui::Entity<FileTab>| -> Words {
        cx.read(|cx| tab.read(cx).header_words())
    };
    // Frames until the bar says `want`. git and the colouring answer on the background
    // executor; the harness's clock is moved on with the real one, so a look-up waiting for its
    // turn (2 s after the last) runs.
    let wait_for = move |cx: &mut VisualTestAppContext,
                         window: AnyWindowHandle,
                         tab: &gpui::Entity<FileTab>,
                         want: &Words,
                         n: &str| {
        let said = settle_until(cx, window, 20, |cx| {
            cx.advance_clock(std::time::Duration::from_millis(50));
            words(cx, tab) == *want
        });
        assert!(said, "{n}: the bar says {:?}, not {want:?}", words(cx, tab));
    };
    let words_of = |name: &str, chip: Option<&str>, meta: String| -> Words {
        (name.to_string(), chip.map(str::to_string), meta)
    };

    for (i, &(slug, name)) in shot_themes().iter().enumerate() {
        let theme = shot_theme(name);

        // A changed file in a repo.
        let n = format!("file-tab-header-{slug}");
        let slot: TabSlot = Rc::default();
        let build = build_for(theme, a_rs.clone(), slot.clone());
        let want = words_of(
            "a.rs",
            Some("M \u{b7} +2 \u{2212}1"),
            format!("{repo_name} / src \u{b7} Rust \u{b7} 4 lines \u{b7} edited just now"),
        );
        let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
            let tab = tab_of(&slot);
            wait_for(cx, window, &tab, &want, &n);
            slot.borrow_mut().take();
        };
        let path = shoot_with(
            "file-tab-header",
            slug,
            &theme,
            size(px(760.0), px(160.0)),
            build,
            act,
        );
        println!("ui-shots: {}", path.display());

        // A middling pane: Where is cut (narrower than in a wide pane, down to its `…`), the
        // parts after it whole (as wide as in a wide pane) and short of the controls.
        let meta_at =
            |width: f32, act_on: Rc<std::cell::Cell<Option<[gpui::Bounds<Pixels>; 3]>>>| {
                let slot: TabSlot = Rc::default();
                let build = build_for(theme, deep_rs.clone(), slot.clone());
                let want = words_of(
                    "b.rs",
                    None,
                    format!(
                        "{repo_name} / src / some / deeper / folders / for / the / where / part \
                     \u{b7} Rust \u{b7} 1 line \u{b7} edited just now"
                    ),
                );
                let n = format!("file-tab-header-mid-{slug} at {width}");
                let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                    let tab = tab_of(&slot);
                    wait_for(cx, window, &tab, &want, &n);
                    let diff_drawn = |cx: &mut VisualTestAppContext| {
                        cx.read(|cx| tab.read(cx).toggle_bounds()[1]).is_some()
                    };
                    assert!(settle_until(cx, window, 20, diff_drawn), "{n}: File | Diff");
                    let (meta, controls) = cx.read(|cx| {
                        let t = tab.read(cx);
                        (t.meta_bounds(), t.controls_bounds().expect("the controls"))
                    });
                    let [Some(place), Some(rest)] = meta else {
                        panic!("{n}: Where and the rest are drawn: {meta:?}");
                    };
                    act_on.set(Some([place, rest, controls]));
                    slot.borrow_mut().take();
                };
                (build, act)
            };
        let wide: Rc<std::cell::Cell<Option<[gpui::Bounds<Pixels>; 3]>>> = Rc::default();
        let (build, act) = meta_at(1100.0, wide.clone());
        render_with(&theme, size(px(1100.0), px(160.0)), build, act);
        let mid: Rc<std::cell::Cell<Option<[gpui::Bounds<Pixels>; 3]>>> = Rc::default();
        let (build, act) = meta_at(480.0, mid.clone());
        let path = shoot_with(
            "file-tab-header-mid",
            slug,
            &theme,
            size(px(480.0), px(160.0)),
            build,
            act,
        );
        println!("ui-shots: {}", path.display());
        let n = format!("file-tab-header-mid-{slug}");
        let [wide_place, wide_rest, _] = wide.get().expect("the wide pane");
        let [place, rest, controls] = mid.get().expect("the middling pane");
        assert!(
            place.size.width < wide_place.size.width,
            "{n}: Where is cut ({place:?}; whole, {wide_place:?})"
        );
        assert!(
            place.size.width >= px(ui_tokens::text::META),
            "{n}: Where keeps room for its `…` ({place:?})"
        );
        assert!(
            (rest.size.width - wide_rest.size.width).abs() < px(0.5),
            "{n}: the rest whole ({rest:?}; wide, {wide_rest:?})"
        );
        assert!(
            rest.left() >= place.right() && rest.right() <= controls.left(),
            "{n}: the rest {rest:?} after Where {place:?}, short of the controls {controls:?}"
        );

        // Narrow: the controls stay inside the pane; Where is still its `…`.
        let n = format!("file-tab-header-narrow-{slug}");
        let width = 320.0;
        let slot: TabSlot = Rc::default();
        let build = build_for(theme, readme.clone(), slot.clone());
        let want = words_of(
            "README.md",
            None,
            format!("{repo_name} \u{b7} Markdown \u{b7} 3 lines \u{b7} edited just now"),
        );
        let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
            let tab = tab_of(&slot);
            wait_for(cx, window, &tab, &want, &n);
            let all = |cx: &mut VisualTestAppContext| {
                cx.read(|cx| tab.read(cx).toggle_bounds())
                    .iter()
                    .all(Option::is_some)
            };
            assert!(
                settle_until(cx, window, 20, all),
                "{n}: File, Diff and Preview are drawn"
            );
            let (segments, controls) = cx.read(|cx| {
                let t = tab.read(cx);
                (
                    t.toggle_bounds(),
                    t.controls_bounds().expect("the controls"),
                )
            });
            let inside = |b: gpui::Bounds<Pixels>| {
                b.left() >= px(0.0) && b.right() <= px(width) && b.size.width > px(0.0)
            };
            assert!(
                inside(controls),
                "{n}: the controls {controls:?} in the pane"
            );
            let place = cx.read(|cx| tab.read(cx).meta_bounds()[0]).expect("Where");
            assert!(
                place.size.width >= px(ui_tokens::text::META),
                "{n}: Where never vanishes, its `…` stays ({place:?})"
            );
            for b in segments.into_iter().flatten() {
                assert!(inside(b), "{n}: a segment {b:?} in the pane");
                assert!(
                    b.left() >= controls.left() && b.right() <= controls.right(),
                    "{n}: the segment {b:?} with the controls {controls:?}"
                );
            }
            slot.borrow_mut().take();
        };
        let path = shoot_with(
            "file-tab-header-narrow",
            slug,
            &theme,
            size(px(width), px(160.0)),
            build,
            act,
        );
        println!("ui-shots: {}", path.display());
        if i > 0 {
            continue;
        }

        // Untracked: every line added.
        let n = "file-tab-header-untracked";
        let slot: TabSlot = Rc::default();
        let build = build_for(theme, new_rs.clone(), slot.clone());
        let want = words_of(
            "new.rs",
            Some("A \u{b7} +3"),
            format!("{repo_name} / src \u{b7} Rust \u{b7} 3 lines \u{b7} edited just now"),
        );
        render_with(
            &theme,
            size(px(760.0), px(160.0)),
            build,
            move |cx, window| {
                let tab = tab_of(&slot);
                wait_for(cx, window, &tab, &want, n);
                slot.borrow_mut().take();
            },
        );
        println!("ui-shots: {n} ok");

        // Outside any repo: no chip, its folder (under your home, as `~/…`).
        let n = "file-tab-header-outside";
        let home = crate::workspace::home_dir();
        let place = match home.as_deref().and_then(|h| loose.strip_prefix(h).ok()) {
            Some(rest) => format!("~/{}", rest.display()),
            None => loose.display().to_string(),
        };
        let slot: TabSlot = Rc::default();
        let build = build_for(theme, notes.clone(), slot.clone());
        let want = words_of(
            "notes.txt",
            None,
            format!("{place} \u{b7} Plain Text \u{b7} 1 line \u{b7} edited just now"),
        );
        render_with(
            &theme,
            size(px(760.0), px(160.0)),
            build,
            move |cx, window| {
                let tab = tab_of(&slot);
                wait_for(cx, window, &tab, &want, n);
                let drawn = cx.read(|cx| tab.read(cx).toggle_bounds());
                assert_eq!(drawn, [None; 3], "{n}: one view, no switch");
                slot.borrow_mut().take();
            },
        );
        println!("ui-shots: {n} ok");

        // Opened in Diff mode (as from a Changes-panel row, or restored): the chip comes from
        // the diff's rows; reverted, it goes.
        let n = "file-tab-header-diff";
        let slot: TabSlot = Rc::default();
        let diff_build = |path: PathBuf, slot: TabSlot| {
            move |window: &mut Window, cx: &mut App| -> AnyView {
                cx.set_global(shot_appearance(theme, cx));
                let tab = cx.new(|cx| FileTab::new(path.clone(), FileMode::Diff, window, cx));
                *slot.borrow_mut() = Some(tab.clone());
                tab.into()
            }
        };
        let build = diff_build(a_rs.clone(), slot.clone());
        let file = a_rs.clone();
        let (changed, reverted) = (
            words_of(
                "a.rs",
                Some("M \u{b7} +2 \u{2212}1"),
                format!("{repo_name} / src \u{b7} Rust \u{b7} 4 lines \u{b7} edited just now"),
            ),
            words_of(
                "a.rs",
                None,
                format!("{repo_name} / src \u{b7} Rust \u{b7} 3 lines \u{b7} edited just now"),
            ),
        );
        render_with(
            &theme,
            size(px(760.0), px(160.0)),
            build,
            move |cx, window| {
                let tab = tab_of(&slot);
                wait_for(cx, window, &tab, &changed, n);
                assert_eq!(
                    cx.read(|cx| tab.read(cx).mode()),
                    FileMode::Diff,
                    "{n}: still in Diff mode"
                );
                std::fs::write(&file, "fn a() {}\nfn b() {}\nfn c() {}\n").expect("revert a.rs");
                cx.update(|cx| tab.update(cx, |t, cx| t.check_disk(cx)));
                wait_for(cx, window, &tab, &reverted, n);
                slot.borrow_mut().take();
            },
        );
        std::fs::write(&a_rs, "fn a() {}\nfn bee() {}\nfn c() {}\nfn d() {}\n").expect("a.rs");
        println!("ui-shots: {n} ok");
        let n = "file-tab-header-diff-untracked";
        let slot: TabSlot = Rc::default();
        let build = diff_build(new_rs.clone(), slot.clone());
        let want = words_of(
            "new.rs",
            Some("A \u{b7} +3"),
            format!("{repo_name} / src \u{b7} Rust \u{b7} 3 lines \u{b7} edited just now"),
        );
        render_with(
            &theme,
            size(px(760.0), px(160.0)),
            build,
            move |cx, window| {
                let tab = tab_of(&slot);
                wait_for(cx, window, &tab, &want, n);
                slot.borrow_mut().take();
            },
        );
        println!("ui-shots: {n} ok");

        // Read again after a line is appended: the line count at once, the counts after git.
        let n = "file-tab-header-reload";
        let slot: TabSlot = Rc::default();
        let build = build_for(theme, a_rs.clone(), slot.clone());
        let file = a_rs.clone();
        let (before, after) = (
            words_of(
                "a.rs",
                Some("M \u{b7} +2 \u{2212}1"),
                format!("{repo_name} / src \u{b7} Rust \u{b7} 4 lines \u{b7} edited just now"),
            ),
            words_of(
                "a.rs",
                Some("M \u{b7} +3 \u{2212}1"),
                format!("{repo_name} / src \u{b7} Rust \u{b7} 5 lines \u{b7} edited just now"),
            ),
        );
        render_with(
            &theme,
            size(px(760.0), px(160.0)),
            build,
            move |cx, window| {
                let tab = tab_of(&slot);
                wait_for(cx, window, &tab, &before, n);
                let mut text = std::fs::read_to_string(&file).expect("a.rs");
                text.push_str("fn e() {}\n");
                std::fs::write(&file, text).expect("append to a.rs");
                cx.update(|cx| tab.update(cx, |t, cx| t.check_disk(cx)));
                wait_for(cx, window, &tab, &after, n);
                slot.borrow_mut().take();
            },
        );
        println!("ui-shots: {n} ok");
        // Back as committed and edited, for the other theme's shots.
        std::fs::write(&a_rs, "fn a() {}\nfn bee() {}\nfn c() {}\nfn d() {}\n").expect("a.rs");
    }
    std::fs::remove_dir_all(&base).ok();
}

/// The file tab's viewer (M11g Task 6), a tab on its own in a window, in both themes:
/// - `file-tab-view`: a Rust fixture in colours with its line numbers; change bars from a given
///   list (lines 3–4 added, green; line 7 changed, blue; a red ▸ under line 10 for lines
///   deleted) with each changed line tinted 12 %; ⌘F "fn": "1 of N", the matches tinted, the
///   current one stronger; lines 2–3 selected with the mouse (⌘C copies them).
/// - `file-tab-long`: a 1,200-character line (with two-byte characters: drawn from its
///   checkpoints) scrolled sideways by the wheel (its end shows), and a ▸ for lines deleted above
///   the first, drawn whole under the top edge.
/// - `file-tab-end`: 80 long lines, all changed and found ("x"), scrolled to the end: the
///   bottom room (as tall as the card's corner radius) stays the background, so no tint, match
///   or selection reaches the card's rounded bottom corners.
/// - `file-tab-missing`, `file-tab-refuse`, `file-tab-plain`: a file that is gone (its path, a
///   Close button that asks to close the tab), a binary file (Show in Finder), a file over 2 MB
///   (plain text, "Too large to colour").
///
/// - `file-tab-minified`: a 1.5 MB one-line file, ⌘F "e": "1 of 10,000+", and a frame stays
///   quick.
///
/// Not shot: a reload with new text (the file rewritten, the disk checked again) keeps the
/// scroll; a shrink to 10 lines and a growth back keep the view at the top; the file removed
/// with the find bar open closes it, ⌘F then does nothing, and the tab has the keyboard. Also prints how long colouring a 5,000-line Rust file takes (run with `--release`
/// for the real number).
fn file_view_scenes() {
    use crate::file_tab::{FileTab, FileTabEvent, MISSING, REFUSED, TOO_LARGE};
    use crate::git_status::LineChange;
    use crate::highlight::Highlighter;
    use crate::ui_tokens::{MARK_MIN, ensure_contrast};
    use alacritty_terminal::vte::ansi::Rgb;
    use gpui::{Focusable, Modifiers, MouseButton, ScrollDelta, ScrollWheelEvent, TouchPhase};
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    let dir = std::env::temp_dir().join("fuxx-ui-shots-file-tab");
    std::fs::create_dir_all(&dir).expect("the fixture folder");
    let dir = std::fs::canonicalize(&dir).unwrap_or(dir);
    let view_rs = dir.join("view.rs");
    std::fs::write(&view_rs, FILE_VIEW_FIXTURE).expect("view.rs");
    let long_rs = dir.join("long.rs");
    let long_line = Rc::new(format!("let long = \"{}END\";", "abcdéfghij".repeat(120)));
    std::fs::write(
        &long_rs,
        format!("// one long line\n{long_line}\nfn after() {{}}\n"),
    )
    .expect("long.rs");
    // 80 lines, each running far past the right edge (for `file-tab-end`).
    let end_rs = dir.join("end.rs");
    let end_lines = 80;
    let end_text: String = (0..end_lines)
        .map(|i| format!("let line_{i:02} = \"{}\";\n", "x".repeat(200)))
        .collect();
    std::fs::write(&end_rs, end_text).expect("end.rs");
    let missing = dir.join("gone.rs");
    let _ = std::fs::remove_file(&missing);
    let binary = dir.join("image.bin");
    std::fs::write(&binary, b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR").expect("image.bin");
    let big = dir.join("big.log");
    let row = "2026-11-08 12:00:00 INFO request served in 12 ms, all fine\n";
    std::fs::write(&big, row.repeat(2 * 1024 * 1024 / row.len() + 100)).expect("big.log");
    let reload_rs = dir.join("reload.rs");
    let minified = dir.join("min.js");
    std::fs::write(&minified, "var e=1;".repeat(1_500_000 / 8)).expect("min.js");

    type TabSlot = Rc<RefCell<Option<gpui::Entity<FileTab>>>>;
    let tab_of = |slot: &TabSlot| slot.borrow().clone().expect("the file tab");
    let dims = size(px(760.0), px(460.0));
    let fixture_lines: Rc<Vec<&str>> = Rc::new(FILE_VIEW_FIXTURE.lines().collect());
    let fn_count: usize = fixture_lines
        .iter()
        .map(|l| l.to_lowercase().matches("fn").count())
        .sum();
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let ui = theme.ui();
        let near = |p: [u8; 3], c: Rgb, by: u8| {
            p.iter()
                .zip([c.r, c.g, c.b])
                .all(|(a, b)| a.abs_diff(b) <= by)
        };
        let build_for = |path: PathBuf, slot: TabSlot| {
            move |window: &mut Window, cx: &mut App| -> AnyView {
                cx.set_global(shot_appearance(theme, cx));
                cx.bind_keys(crate::key_bindings());
                let tab = cx.new(|cx| {
                    let mut tab = FileTab::new(
                        path.clone(),
                        crate::layout_model::FileMode::Plain,
                        window,
                        cx,
                    );
                    tab.frozen = true;
                    tab
                });
                *slot.borrow_mut() = Some(tab.clone());
                tab.into()
            }
        };
        let frame = |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
            cx.update_window(window, |_, window, _| window.refresh())
                .expect("refresh");
            cx.run_until_parked();
        };

        // The view: colours, numbers, change bars and tints, find, a selection and ⌘C.
        let n = format!("file-tab-view-{slug}");
        let slot: TabSlot = Rc::default();
        let build = build_for(view_rs.clone(), slot.clone());
        let copied: Rc<RefCell<Option<String>>> = Rc::default();
        let seen = copied.clone();
        let geom_seen: Rc<Cell<Option<crate::file_tab::Geom>>> = Rc::default();
        let keep_geom = geom_seen.clone();
        let act = {
            let n = n.clone();
            let slot = slot.clone();
            let fixture_lines = fixture_lines.clone();
            move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let tab = tab_of(&slot);
                frame(cx, window);
                cx.read(|cx| {
                    let t = tab.read(cx);
                    assert_eq!(t.state_name(), "text", "{n}");
                    let doc = t.doc().expect("its text");
                    assert_eq!(doc.lines.len(), fixture_lines.len(), "{n}");
                    assert!(doc.spans.is_some(), "{n}: coloured");
                });
                cx.update(|cx| {
                    tab.update(cx, |t, cx| {
                        t.set_changes(
                            vec![
                                (3, LineChange::Added),
                                (4, LineChange::Added),
                                (7, LineChange::Changed),
                                (10, LineChange::DeletedAfter),
                            ],
                            cx,
                        )
                    })
                });
                cx.update_window(window, |_, window, cx| {
                    tab.read(cx).focus_handle(cx).focus(window, cx)
                })
                .expect("focus");
                cx.simulate_keystrokes(window, "cmd-f");
                cx.simulate_input(window, "fn");
                frame(cx, window);
                let label = cx.read(|cx| tab.read(cx).find_label());
                assert_eq!(label, format!("1 of {fn_count}"), "{n}: ⌘F fn");
                // Lines 2–3 (0-based 1–2), from line 2's start to line 3's 5th byte.
                let g = cx.read(|cx| tab.read(cx).geom()).expect("drawn");
                let at = |line: usize, x: f32| {
                    point(
                        g.bounds.left() + g.text_left + px(x),
                        g.bounds.top() + g.line_height * (line as f32 + 0.5),
                    )
                };
                let cw = f32::from(g.cell_width);
                cx.simulate_mouse_down(window, at(1, 0.5), MouseButton::Left, Modifiers::none());
                cx.simulate_mouse_move(
                    window,
                    at(2, cw * 5.0 + 1.0),
                    MouseButton::Left,
                    Modifiers::none(),
                );
                cx.simulate_mouse_up(
                    window,
                    at(2, cw * 5.0 + 1.0),
                    MouseButton::Left,
                    Modifiers::none(),
                );
                cx.simulate_keystrokes(window, "cmd-c");
                *seen.borrow_mut() = cx.read_from_clipboard().and_then(|c| c.text());
                frame(cx, window);
                keep_geom.set(cx.read(|cx| tab.read(cx).geom()));
                slot.borrow_mut().take();
            }
        };
        let path = shoot_with(
            "file-tab-view",
            slug,
            &theme,
            dims,
            build.clone(),
            act.clone(),
        );
        println!("ui-shots: {}", path.display());
        let shot = render_with(&theme, dims, build, act);
        let expect = format!("{}\n{}", fixture_lines[1], &fixture_lines[2][..5]);
        assert_eq!(copied.borrow().as_deref(), Some(expect.as_str()), "{n}: ⌘C");
        let g = geom_seen.get().expect("drawn");
        let at = |x: Pixels, y: Pixels| pixel_at(&shot, dims, f32::from(x), f32::from(y));
        let row_y = |line: usize| g.bounds.top() + g.line_height * (line as f32 + 0.5);
        let fit = |c: Rgb| ensure_contrast(c, theme.background, MARK_MIN);
        let (green, blue, red) = (
            fit(theme.palette[2]),
            fit(theme.palette[4]),
            fit(theme.palette[1]),
        );
        let bar_x = g.bounds.left() + g.bar_left + px(1.5);
        // The bars: 0-based lines 2 and 3 green, 6 blue; none on line 0.
        for (line, c) in [(2, green), (3, green), (6, blue)] {
            let p = at(bar_x, row_y(line));
            assert!(near(p, c, 12), "{n}: line {line}'s bar {p:?}, not {c:?}");
        }
        let p = at(bar_x, row_y(0));
        assert!(near(p, theme.background, 6), "{n}: no bar on line 0: {p:?}");
        // The tint at the far right of a changed line: its colour at 12 % over the background.
        let right = g.bounds.right() - px(6.0);
        let tint = crate::theme::mix(theme.background, green, 0.12);
        let p = at(right, row_y(2));
        assert!(near(p, tint, 6), "{n}: the tint {p:?}, not {tint:?}");
        assert!(
            near(at(right, row_y(0)), theme.background, 4),
            "{n}: untinted"
        );
        // The ▸ under line 10 (the edge above 0-based line 10), red.
        let edge = g.bounds.top() + g.line_height * 10.0;
        let marked = (-6..=6).any(|dy| {
            (-6..=10).any(|dx| near(at(bar_x + px(dx as f32), edge + px(dy as f32)), red, 40))
        });
        assert!(marked, "{n}: the deletion mark");
        // The line numbers, faint, right-aligned before the bar.
        let numbers = (0..(f32::from(g.numbers_right) as i32)).any(|dx| {
            (-5..=5).any(|dy| {
                near(
                    at(g.bounds.left() + px(dx as f32), row_y(0) + px(dy as f32)),
                    ui.text_faint,
                    40,
                )
            })
        });
        assert!(numbers, "{n}: line numbers in text_faint");
        // Several syntax colours in the text.
        let inked = |c: Rgb| {
            (0..12).any(|line| {
                (0..400).any(|dx| {
                    near(
                        at(g.bounds.left() + g.text_left + px(dx as f32), row_y(line)),
                        c,
                        30,
                    )
                })
            })
        };
        let keyword = fit(theme.palette[5]);
        let string = fit(theme.palette[2]);
        let comment = fit(theme.palette[8]);
        for (what, c) in [
            ("keyword", keyword),
            ("string", string),
            ("comment", comment),
        ] {
            assert!(inked(c), "{n}: a {what} in {c:?}");
        }

        // Wrap on (M11x part 2): the long line takes more rows than there are lines, the wheel
        // doesn't scroll sideways, and the Wrap button is filled.
        let n = format!("file-tab-wrap-{slug}");
        let slot: TabSlot = Rc::default();
        let build = build_for(long_rs.clone(), slot.clone());
        let act = {
            let n = n.clone();
            move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let tab = tab_of(&slot);
                cx.update(|cx| cx.global_mut::<Appearance>().file_wrap = true);
                frame(cx, window);
                frame(cx, window);
                let g = cx.read(|cx| tab.read(cx).geom()).expect("drawn");
                cx.simulate_event(
                    window,
                    ScrollWheelEvent {
                        position: g.bounds.center(),
                        delta: ScrollDelta::Pixels(point(px(-5000.0), px(0.0))),
                        modifiers: Modifiers::none(),
                        touch_phase: TouchPhase::Moved,
                    },
                );
                frame(cx, window);
                let (scroll, rows, lines) = cx.read(|cx| {
                    let t = tab.read(cx);
                    (t.scroll(), t.wrapped_rows(), t.line_count())
                });
                assert_eq!(scroll.x, px(0.0), "{n}: no sideways scroll");
                let rows = rows.expect("wrapped");
                assert!(rows > lines, "{n}: {rows} rows for {lines} lines");
                slot.borrow_mut().take();
            }
        };
        let path = shoot_with("file-tab-wrap", slug, &theme, dims, build, act);
        println!("ui-shots: {}", path.display());

        // A long line, scrolled sideways by the wheel: its end shows.
        let n = format!("file-tab-long-{slug}");
        let long_geom: Rc<Cell<Option<crate::file_tab::Geom>>> = Rc::default();
        let keep_long = long_geom.clone();
        let slot: TabSlot = Rc::default();
        let build = build_for(long_rs.clone(), slot.clone());
        let act = {
            let n = n.clone();
            let long_line = long_line.clone();
            let long_geom = keep_long.clone();
            move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let tab = tab_of(&slot);
                frame(cx, window);
                let g = cx.read(|cx| tab.read(cx).geom()).expect("drawn");
                let over = g.bounds.center();
                for _ in 0..3 {
                    cx.simulate_event(
                        window,
                        ScrollWheelEvent {
                            position: over,
                            delta: ScrollDelta::Pixels(point(px(-5000.0), px(0.0))),
                            modifiers: Modifiers::none(),
                            touch_phase: TouchPhase::Moved,
                        },
                    );
                }
                frame(cx, window);
                // Lines deleted above the first: its ▸ on the top edge, drawn whole.
                cx.update(|cx| {
                    tab.update(cx, |t, cx| {
                        t.set_changes(vec![(0, LineChange::DeletedAfter)], cx)
                    })
                });
                frame(cx, window);
                let (scroll, g) = cx.read(|cx| (tab.read(cx).scroll(), tab.read(cx).geom()));
                let g = g.expect("drawn");
                long_geom.set(Some(g));
                assert!(scroll.x > px(0.0), "{n}: scrolled sideways");
                assert_eq!(scroll.y, px(0.0), "{n}");
                // As far as it goes: the line's end at the right edge.
                let widest = g.text_left + g.cell_width * long_line.chars().count() as f32;
                assert!(
                    scroll.x + g.bounds.size.width >= widest,
                    "{n}: the end shows ({scroll:?})"
                );
                slot.borrow_mut().take();
            }
        };
        let path = shoot_with(
            "file-tab-long",
            slug,
            &theme,
            dims,
            build.clone(),
            act.clone(),
        );
        println!("ui-shots: {}", path.display());
        let shot = render_with(&theme, dims, build, act);
        let g = long_geom.get().expect("drawn");
        let red = fit(theme.palette[1]);
        let bar_x = g.bounds.left() + g.bar_left + px(1.5);
        let whole = (-6..=10).any(|dx| {
            (0..=6).any(|dy| {
                let y = g.bounds.top() + g.line_height * 0.4 + px(dy as f32 * 0.6);
                let x = bar_x + px(dx as f32);
                near(pixel_at(&shot, dims, f32::from(x), f32::from(y)), red, 40)
            })
        });
        assert!(
            whole,
            "{n}: the top edge's ▸ is drawn whole, under the edge"
        );

        // Scrolled to its end, every line changed (tinted the full width) and found: nothing
        // is painted in the bottom room, where the card's bottom corners are (Task 4 review, M2).
        let n = format!("file-tab-end-{slug}");
        let slot: TabSlot = Rc::default();
        let build = build_for(end_rs.clone(), slot.clone());
        let act = {
            let n = n.clone();
            move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let tab = tab_of(&slot);
                frame(cx, window);
                cx.update(|cx| {
                    tab.update(cx, |t, cx| {
                        // Lines counted from 1, as git does.
                        let all = (1..=end_lines).map(|i| (i, LineChange::Changed));
                        t.set_changes(all.collect(), cx)
                    })
                });
                cx.update_window(window, |_, window, cx| {
                    tab.read(cx).focus_handle(cx).focus(window, cx)
                })
                .expect("focus");
                cx.simulate_keystrokes(window, "cmd-f");
                cx.simulate_input(window, "x");
                frame(cx, window);
                let g = cx.read(|cx| tab.read(cx).geom()).expect("drawn");
                for _ in 0..3 {
                    cx.simulate_event(
                        window,
                        ScrollWheelEvent {
                            position: g.bounds.center(),
                            delta: ScrollDelta::Pixels(point(px(0.0), px(-50_000.0))),
                            modifiers: Modifiers::none(),
                            touch_phase: TouchPhase::Moved,
                        },
                    );
                }
                frame(cx, window);
                let (scroll, g) = cx.read(|cx| (tab.read(cx).scroll(), tab.read(cx).geom()));
                let g = g.expect("drawn");
                assert!(scroll.y > px(0.0), "{n}: scrolled down");
                assert_eq!(scroll.y, g.max_scroll.y, "{n}: to the end");
                slot.borrow_mut().take();
            }
        };
        let path = shoot_with(
            "file-tab-end",
            slug,
            &theme,
            dims,
            build.clone(),
            act.clone(),
        );
        println!("ui-shots: {}", path.display());
        let shot = render_with(&theme, dims, build, act);
        let (w, h) = (f32::from(dims.width), f32::from(dims.height));
        let tint = crate::theme::mix(theme.background, fit(theme.palette[4]), 0.12);
        // The last line, tinted, above the room ...
        let room = ui_tokens::size::CARD_RADIUS;
        let p = pixel_at(&shot, dims, w - 2.0, h - room - 2.0);
        assert!(
            near(p, tint, 4) && !near(p, theme.background, 4),
            "{n}: the last line is tinted: {p:?}, not {tint:?}"
        );
        // ... and the room under it, across the width, the background.
        for x in [1.0, 2.0, 6.0, w / 2.0, w - 6.0, w - 2.0, w - 1.0] {
            for y in [h - 1.0, h - 2.0, h - room / 2.0] {
                let p = pixel_at(&shot, dims, x, y);
                assert!(
                    near(p, theme.background, 4),
                    "{n}: nothing in the bottom room at ({x}, {y}): {p:?}"
                );
            }
        }

        // The states: missing (and its Close), refused, plain.
        for (scene, file, state, words) in [
            ("file-tab-missing", missing.clone(), "missing", MISSING),
            ("file-tab-refuse", binary.clone(), "refuse", REFUSED),
            ("file-tab-plain", big.clone(), "plain", TOO_LARGE),
        ] {
            let n = format!("{scene}-{slug}");
            let slot: TabSlot = Rc::default();
            let build = build_for(file, slot.clone());
            let closed = Rc::new(Cell::new(false));
            let asked = closed.clone();
            let act = {
                let n = n.clone();
                move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                    let tab = tab_of(&slot);
                    frame(cx, window);
                    cx.read(|cx| {
                        let t = tab.read(cx);
                        assert_eq!(t.state_name(), state, "{n}");
                        assert_eq!(t.note(), Some(words), "{n}");
                        if state == "plain" {
                            assert!(t.doc().expect("its text").spans.is_none(), "{n}");
                        }
                    });
                    if state == "missing" {
                        let flag = asked.clone();
                        cx.update(|cx| {
                            cx.subscribe(&tab, move |_, e: &FileTabEvent, _| {
                                flag.set(matches!(e, FileTabEvent::Close))
                            })
                            .detach()
                        });
                        let b = cx.read(|cx| tab.read(cx).close_bounds()).expect("Close");
                        cx.simulate_click(window, b.center(), Modifiers::none());
                    }
                    slot.borrow_mut().take();
                }
            };
            let path = shoot_with(scene, slug, &theme, dims, build, act);
            println!("ui-shots: {}", path.display());
            if state == "missing" {
                assert!(closed.get(), "{n}: Close asks to close the tab");
            }
        }

        // A reload with new text keeps the scroll (not shot).
        let n = format!("file-tab-reload-{slug}");
        let numbered = |count: usize, word: &str| -> String {
            (1..=count)
                .map(|i| format!("let {word}{i} = {i};\n"))
                .collect()
        };
        std::fs::write(&reload_rs, numbered(200, "a")).expect("reload.rs");
        let slot: TabSlot = Rc::default();
        let build = build_for(reload_rs.clone(), slot.clone());
        let file = reload_rs.clone();
        let label = n.clone();
        let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
            let tab = tab_of(&slot);
            frame(cx, window);
            let g = cx.read(|cx| tab.read(cx).geom()).expect("drawn");
            cx.update(|cx| {
                tab.update(cx, |t, cx| {
                    t.set_scroll(point(px(0.0), g.line_height * 40.0), cx)
                })
            });
            frame(cx, window);
            let (before, version) = cx.read(|cx| {
                (
                    tab.read(cx).scroll(),
                    tab.read(cx).doc().expect("text").version,
                )
            });
            assert_eq!(before.y, g.line_height * 40.0, "{n}: scrolled");
            std::fs::write(&file, numbered(220, "b")).expect("rewrite reload.rs");
            cx.update(|cx| tab.update(cx, |t, cx| t.check_disk(cx)));
            frame(cx, window);
            cx.read(|cx| {
                let t = tab.read(cx);
                let doc = t.doc().expect("text");
                assert!(doc.version > version, "{n}: reloaded");
                assert_eq!(doc.lines.len(), 220, "{n}: the new text");
                assert!(doc.text.starts_with("let b1"), "{n}");
                assert_eq!(t.scroll(), before, "{n}: the scroll kept");
            });
            // It shrinks to 10 lines (the view to the top), then grows again: it stays at the top.
            for (count, word) in [(10, "c"), (220, "d")] {
                std::fs::write(&file, numbered(count, word)).expect("rewrite reload.rs");
                cx.update(|cx| tab.update(cx, |t, cx| t.check_disk(cx)));
                frame(cx, window);
                let (lines, scroll) = cx.read(|cx| {
                    let t = tab.read(cx);
                    (t.doc().expect("text").lines.len(), t.scroll())
                });
                assert_eq!(lines, count, "{n}");
                assert_eq!(scroll.y, px(0.0), "{n}: {count} lines, at the top");
            }
            // ⌘F with the file there; then it goes: the bar closes, ⌘F does nothing, the tab has
            // the keyboard.
            cx.update_window(window, |_, window, cx| {
                tab.read(cx).focus_handle(cx).focus(window, cx)
            })
            .expect("focus");
            cx.simulate_keystrokes(window, "cmd-f");
            cx.simulate_input(window, "let");
            frame(cx, window);
            assert_eq!(cx.read(|cx| tab.read(cx).find_label()), "1 of 220", "{n}");
            // Scrolled away from the current match, the file changes: the view stays put.
            cx.update(|cx| {
                tab.update(cx, |t, cx| {
                    t.set_scroll(point(px(0.0), g.line_height * 100.0), cx)
                })
            });
            frame(cx, window);
            let away = cx.read(|cx| tab.read(cx).scroll());
            assert_eq!(away.y, g.line_height * 100.0, "{n}: scrolled away");
            std::fs::write(&file, numbered(230, "e")).expect("rewrite reload.rs");
            cx.update(|cx| tab.update(cx, |t, cx| t.check_disk(cx)));
            frame(cx, window);
            let (label, scroll) = cx.read(|cx| (tab.read(cx).find_label(), tab.read(cx).scroll()));
            assert_eq!(label, "1 of 230", "{n}: found again in the new text");
            assert_eq!(
                scroll, away,
                "{n}: a reload with the find bar open keeps the scroll"
            );
            std::fs::remove_file(&file).expect("remove reload.rs");
            cx.update(|cx| tab.update(cx, |t, cx| t.check_disk(cx)));
            frame(cx, window);
            cx.simulate_keystrokes(window, "cmd-f");
            frame(cx, window);
            let (state, open, keys) = cx
                .update_window(window, |_, window, cx| {
                    let t = tab.read(cx);
                    (
                        t.state_name(),
                        t.find_open(),
                        t.focus_handle(cx).is_focused(window),
                    )
                })
                .expect("read");
            assert_eq!(state, "missing", "{n}");
            assert!(!open, "{n}: no find bar for a missing file");
            assert!(keys, "{n}: the tab has the keyboard back");
            slot.borrow_mut().take();
        };
        render_with(&theme, dims, build, act);
        println!("ui-shots: {label} ok");

        // A minified file, one 1.5 MB line, and ⌘F "e": capped at 10,000, and drawing it stays
        // quick (only the matches in sight are drawn).
        let n = format!("file-tab-minified-{slug}");
        let slot: TabSlot = Rc::default();
        let build = build_for(minified.clone(), slot.clone());
        let act = {
            let n = n.clone();
            move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let tab = tab_of(&slot);
                frame(cx, window);
                assert_eq!(cx.read(|cx| tab.read(cx).state_name()), "plain", "{n}");
                cx.update_window(window, |_, window, cx| {
                    tab.read(cx).focus_handle(cx).focus(window, cx)
                })
                .expect("focus");
                cx.simulate_keystrokes(window, "cmd-f");
                cx.simulate_input(window, "e");
                frame(cx, window);
                let label = cx.read(|cx| tab.read(cx).find_label());
                assert_eq!(label, "1 of 10,000+", "{n}");
                let started = std::time::Instant::now();
                for _ in 0..5 {
                    cx.update_window(window, |_, window, cx| {
                        window.refresh();
                        window.draw(cx).clear();
                    })
                    .expect("draw");
                }
                let each = started.elapsed() / 5;
                println!("ui-shots: {n}: a frame in {} ms", each.as_millis());
                assert!(
                    each < std::time::Duration::from_secs(1),
                    "{n}: {each:?} a frame"
                );
                slot.borrow_mut().take();
            }
        };
        let path = shoot_with("file-tab-minified", slug, &theme, dims, build, act);
        println!("ui-shots: {}", path.display());
    }

    // How long colouring a 5,000-line Rust file takes (the loading of the syntaxes apart).
    let theme = shot_theme("Catppuccin Mocha");
    let started = std::time::Instant::now();
    let h = Highlighter::load(&theme);
    let loaded = started.elapsed();
    let text: String = FILE_VIEW_FIXTURE
        .lines()
        .cycle()
        .take(5000)
        .map(|l| format!("{l}\n"))
        .collect();
    let started = std::time::Instant::now();
    let lines = h.highlight(std::path::Path::new("big.rs"), &text);
    let took = started.elapsed();
    assert_eq!(lines.len(), 5000);
    println!(
        "ui-shots: file-tab highlight 5000 lines: {} ms (syntaxes loaded in {} ms; {} build)",
        took.as_millis(),
        loaded.as_millis(),
        if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        }
    );
}

/// M11ae Task 3: the file tab's overlay scrollbar, a tab on its own in a window, in Mocha (both
/// themes with `--all-themes`):
/// - `file-tab-scrollbar`: a 400-line file. No thumb before any scroll; one wheel step shows it
///   in the text's right 12 pt, its top as far down its track as the scroll is down the text;
///   1.5 s later (real time: the fade reads the clock) it is gone and its fade task has ended.
///   A press below the thumb centres it on the press and selects nothing; the thumb dragged to
///   below the text scrolls to the end. Shot then: the thumb at the bottom right.
/// - `file-tab-scrollbar-wide`: Wrap off, a 3,000-character line, the wheel sideways: a thumb
///   along the bottom (in the text's bottom 12 pt, right of the numbers), none at the right.
/// - `file-tab-scrollbar-short`: a 10-line file: the wheel and the pointer at the right edge
///   show nothing, and a press in that edge's 12 pt starts a selection.
fn file_tab_scrollbar_scenes() {
    use crate::file_tab::FileTab;
    use crate::scrollbar::Axis;
    use gpui::{Modifiers, MouseButton, ScrollDelta, ScrollWheelEvent, TouchPhase};
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::time::Duration;

    let dir = std::env::temp_dir().join("fuxx-ui-shots-file-tab-scrollbar");
    std::fs::create_dir_all(&dir).expect("the fixture folder");
    let dir = std::fs::canonicalize(&dir).unwrap_or(dir);
    let lines =
        |n: usize| -> String { (1..=n).map(|i| format!("let line_{i} = {i};\n")).collect() };
    let long = dir.join("long.rs");
    std::fs::write(&long, lines(400)).expect("long.rs");
    let wide = dir.join("wide.rs");
    let wide_text = format!("// one wide line\nlet wide = \"{}\";\n", "x".repeat(3_000));
    std::fs::write(&wide, wide_text).expect("wide.rs");
    let short = dir.join("short.rs");
    std::fs::write(&short, lines(10)).expect("short.rs");

    type TabSlot = Rc<RefCell<Option<gpui::Entity<FileTab>>>>;
    let dims = size(px(760.0), px(460.0));
    let none = Modifiers::none();
    let left = Some(MouseButton::Left);
    let frame = |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
        cx.update_window(window, |_, window, _| window.refresh())
            .expect("refresh");
        cx.run_until_parked();
    };
    let wheel = |cx: &mut VisualTestAppContext, window, at, delta| {
        cx.simulate_event(
            window,
            ScrollWheelEvent {
                position: at,
                delta,
                modifiers: none,
                touch_phase: TouchPhase::Moved,
            },
        );
    };
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let build_for = |path: PathBuf, slot: TabSlot| {
            move |window: &mut Window, cx: &mut App| -> AnyView {
                cx.set_global(shot_appearance(theme, cx));
                let tab = cx.new(|cx| {
                    let mut tab = FileTab::new(
                        path.clone(),
                        crate::layout_model::FileMode::Plain,
                        window,
                        cx,
                    );
                    tab.frozen = true;
                    tab
                });
                *slot.borrow_mut() = Some(tab.clone());
                tab.into()
            }
        };

        // The 400-line file: shown by a scroll, faded, a press below the thumb, a drag.
        let n = format!("file-tab-scrollbar-{slug}");
        let slot: TabSlot = Rc::default();
        let build = build_for(long.clone(), slot.clone());
        let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
            let tab = slot.borrow().clone().expect("the file tab");
            frame(cx, window);
            let look = |cx: &mut VisualTestAppContext| {
                cx.read(|cx| {
                    let t = tab.read(cx);
                    let g = t.geom().expect("drawn");
                    let thumb = t.thumb_bounds(Axis::Vertical);
                    (
                        t.scrollbar_shown(),
                        t.scrollbar_animating(),
                        thumb,
                        t.scroll(),
                        g,
                    )
                })
            };
            let (shown, _, thumb, _, g) = look(cx);
            assert!(!shown, "{n}: shown before any scroll");
            assert!(thumb.is_some(), "{n}: 400 lines overflow");
            wheel(
                cx,
                window,
                g.bounds.center(),
                ScrollDelta::Lines(point(0.0, -3.0)),
            );
            frame(cx, window);
            let (shown, _, thumb, scroll, g) = look(cx);
            let thumb = thumb.expect("a thumb");
            assert!(scroll.y > px(0.0), "{n}: one wheel step scrolls");
            assert!(shown, "{n}: not shown after a scroll");
            assert!(
                thumb.left() >= g.bounds.right() - px(12.0) && thumb.right() <= g.bounds.right(),
                "{n}: the thumb {thumb:?} is not in the text's right 12 pt ({:?})",
                g.bounds
            );
            let down = (thumb.top() - g.bounds.top()) / (g.bounds.size.height - thumb.size.height);
            let scrolled = scroll.y / g.max_scroll.y;
            assert!(
                (down - scrolled).abs() < 0.01,
                "{n}: the thumb {down} down its track, the text {scrolled} down"
            );
            // Real time for the fade (it reads `Instant`), the test clock for its task's steps.
            prof::sleep(Duration::from_millis(1500));
            cx.advance_clock(Duration::from_millis(1500));
            frame(cx, window);
            let (shown, animating, ..) = look(cx);
            assert!(!shown, "{n}: still shown 1.5 s after the scroll");
            assert!(!animating, "{n}: the fade task still runs");

            // A press below the thumb, in the right edge's zone: the thumb centres on it.
            let at = point(
                g.bounds.right() - px(6.0),
                g.bounds.top() + g.bounds.size.height * 0.6,
            );
            cx.simulate_mouse_down(window, at, MouseButton::Left, none);
            cx.simulate_mouse_up(window, at, MouseButton::Left, none);
            frame(cx, window);
            let (shown, _, thumb, _, _) = look(cx);
            let thumb = thumb.expect("a thumb");
            assert!(shown, "{n}: not shown after a jump");
            assert!(
                (thumb.center().y - at.y).abs() < px(0.5),
                "{n}: the thumb {thumb:?} is not centred on the press at {at:?}"
            );
            let selected = cx.read(|cx| tab.read(cx).selected_text());
            assert_eq!(selected, "", "{n}: a press in the edge zone selected");

            // The thumb dragged to below the text: the end.
            let grab = thumb.center();
            cx.simulate_mouse_down(window, grab, MouseButton::Left, none);
            let below = point(grab.x, g.bounds.bottom() + px(40.0));
            cx.simulate_mouse_move(window, below, left, none);
            cx.simulate_mouse_up(window, below, MouseButton::Left, none);
            frame(cx, window);
            let (shown, _, thumb, scroll, g) = look(cx);
            assert_eq!(scroll.y, g.max_scroll.y, "{n}: dragged to the end");
            let thumb = thumb.expect("a thumb");
            assert!(
                (thumb.bottom() - g.bounds.bottom()).abs() < px(0.5),
                "{n}: the thumb {thumb:?} at the track's end"
            );
            assert!(shown, "{n}: not shown after a drag");
            let selected = cx.read(|cx| tab.read(cx).selected_text());
            assert_eq!(selected, "", "{n}: a thumb drag selected");
            slot.borrow_mut().take();
        };
        let path = shoot_with("file-tab-scrollbar", slug, &theme, dims, build, act);
        println!("ui-shots: {}", path.display());

        // Wrap off, a 3,000-character line, scrolled sideways: the bottom thumb.
        let n = format!("file-tab-scrollbar-wide-{slug}");
        let slot: TabSlot = Rc::default();
        let build = build_for(wide.clone(), slot.clone());
        let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
            let tab = slot.borrow().clone().expect("the file tab");
            frame(cx, window);
            let g = cx.read(|cx| tab.read(cx).geom()).expect("drawn");
            let sideways = ScrollDelta::Pixels(point(px(-400.0), px(0.0)));
            wheel(cx, window, g.bounds.center(), sideways);
            frame(cx, window);
            let (shown, thumb, right, scroll, g) = cx.read(|cx| {
                let t = tab.read(cx);
                let g = t.geom().expect("drawn");
                let (h, v) = (
                    t.thumb_bounds(Axis::Horizontal),
                    t.thumb_bounds(Axis::Vertical),
                );
                (t.scrollbar_shown(), h, v, t.scroll(), g)
            });
            assert!(scroll.x > px(0.0), "{n}: scrolled sideways");
            assert!(shown, "{n}: not shown after a sideways scroll");
            assert_eq!(right, None, "{n}: a thumb at the right for 2 lines");
            let thumb = thumb.expect("a bottom thumb");
            assert!(
                thumb.top() >= g.bounds.bottom() - px(12.0)
                    && thumb.bottom() <= g.bounds.bottom()
                    && thumb.left() > g.bounds.left() + g.text_left,
                "{n}: the thumb {thumb:?} is not in the text's bottom 12 pt ({:?})",
                g.bounds
            );
            slot.borrow_mut().take();
        };
        let path = shoot_with("file-tab-scrollbar-wide", slug, &theme, dims, build, act);
        println!("ui-shots: {}", path.display());

        // A 10-line file: nothing to scroll, so no bar, and the edge selects as anywhere.
        let n = format!("file-tab-scrollbar-short-{slug}");
        let slot: TabSlot = Rc::default();
        let build = build_for(short.clone(), slot.clone());
        let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
            let tab = slot.borrow().clone().expect("the file tab");
            frame(cx, window);
            let g = cx.read(|cx| tab.read(cx).geom()).expect("drawn");
            wheel(
                cx,
                window,
                g.bounds.center(),
                ScrollDelta::Lines(point(0.0, -3.0)),
            );
            let edge = point(
                g.bounds.right() - px(4.0),
                g.bounds.top() + g.line_height * 1.5,
            );
            cx.simulate_mouse_move(window, edge, None, none);
            frame(cx, window);
            let (shown, thumb) = cx.read(|cx| {
                let t = tab.read(cx);
                (t.scrollbar_shown(), t.thumb_bounds(Axis::Vertical))
            });
            assert!(!shown, "{n}: shown for a text that fits");
            assert_eq!(thumb, None, "{n}: a thumb for a text that fits");
            cx.simulate_mouse_down(window, edge, MouseButton::Left, none);
            let to = point(
                g.bounds.left() + g.text_left + px(2.0),
                g.bounds.top() + g.line_height * 3.5,
            );
            cx.simulate_mouse_move(window, to, left, none);
            cx.simulate_mouse_up(window, to, MouseButton::Left, none);
            frame(cx, window);
            let selected = cx.read(|cx| tab.read(cx).selected_text());
            assert!(
                !selected.is_empty(),
                "{n}: a press at the edge didn't select"
            );
            slot.borrow_mut().take();
        };
        let path = shoot_with("file-tab-scrollbar-short", slug, &theme, dims, build, act);
        println!("ui-shots: {}", path.display());
    }
    std::fs::remove_dir_all(&dir).ok();
}

/// The diff fixture: `FILE_VIEW_FIXTURE` with line 7 changed (row 6 removed, row 7 added) and
/// a line added after line 18 (row 19), as git prints it with the whole file as context.
fn file_diff_fixture() -> String {
    let old: Vec<&str> = FILE_VIEW_FIXTURE.lines().collect();
    let mut u = format!(
        "diff --git a/view.rs b/view.rs\n--- a/view.rs\n+++ b/view.rs\n@@ -1,{} +1,{} @@\n",
        old.len(),
        old.len() + 1
    );
    for (i, line) in old.iter().enumerate() {
        if i == 6 {
            u.push_str(&format!("-{line}\n+const NAME: &str = \"fuxx diff\";\n"));
            continue;
        }
        u.push_str(&format!(" {line}\n"));
        if i == 17 {
            u.push_str("+    pub fn port(&self) -> u16 { self.port }\n");
        }
    }
    u
}

/// The file tab's diff mode (M11h Task 3), in both themes:
/// - `file-diff`: a tab on the `view.rs` fixture in Diff mode, given `file_diff_fixture`: its
///   rows coloured; a removed row tinted red at 12 % with a `−` in the sign column, an added row
///   green with a `+`, an unchanged row untinted; the old number column of an added row and the
///   new number column of a removed row hold no number (the other one does); opened with its
///   first change 3 rows from the top; ⌘F "fn" counts the diff text's matches; two rows dragged
///   over and ⌘C copy their text without signs or numbers.
/// - `file-diff-states-*`: `same` ("No changes since the last commit."), `binary` ("This file is
///   binary: no diff to show."), `norepo` (a file outside any repo, looked up for real: "Not a
///   git repository."), `missing-rows` (a deleted file's removed rows show, not "This file no
///   longer exists"), `missing-same` (a deleted file with nothing to show: that note).
/// - `file-diff-toggle`: a workspace with the Files panel; `open_file_tab(view.rs, Diff)`: the
///   bar's view switch with Diff `card_selected` and File `panel` (no Preview: not Markdown).
///   Not shot: File and Diff clicked both ways, each mode keeping its own scroll; a click on
///   File (the tab and the model Plain, the same tab); a click on line 7's change bar (Diff,
///   its change scrolled into view, the model Diff); a Files click on view.rs (the same tab,
///   Plain).
fn file_diff_scenes() {
    use crate::diff_view::{BINARY, NO_CHANGES, NO_REPO, RowKind, first_change, reveal_row};
    use crate::file_tab::{FileTab, MISSING};
    use crate::git_status::LineChange;
    use crate::layout_model::{FileMode, TabKind};
    use crate::right_sidebar::{FILTER_BAND, HEADER, LIST_TOP, ROW};
    use crate::ui_tokens::size::{FOCUS_RING, TAB_BAR};
    use crate::ui_tokens::{MARK_MIN, ensure_contrast};
    use alacritty_terminal::vte::ansi::Rgb;
    use gpui::{Focusable, Modifiers, MouseButton};
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    let dir = std::env::temp_dir().join("fuxx-ui-shots-file-diff");
    std::fs::create_dir_all(dir.join("src")).expect("the fixture folder");
    let dir = std::fs::canonicalize(&dir).unwrap_or(dir);
    let view_rs = dir.join("src/view.rs");
    std::fs::write(&view_rs, FILE_VIEW_FIXTURE).expect("view.rs");
    let missing = dir.join("src/gone.rs");
    let _ = std::fs::remove_file(&missing);
    let unified = Rc::new(file_diff_fixture());
    // The fixture's rows (as `parse_diff` makes them): 6 removed, 7 and 19 added, 9 unchanged.
    let (removed, added, same, added_late) = (6usize, 7usize, 9usize, 19usize);

    type TabSlot = Rc<RefCell<Option<gpui::Entity<FileTab>>>>;
    let tab_of = |slot: &TabSlot| slot.borrow().clone().expect("the file tab");
    let dims = size(px(760.0), px(460.0));
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let ui = theme.ui();
        let near = |p: [u8; 3], c: Rgb, by: u8| {
            p.iter()
                .zip([c.r, c.g, c.b])
                .all(|(a, b)| a.abs_diff(b) <= by)
        };
        let fit = |c: Rgb| ensure_contrast(c, theme.background, MARK_MIN);
        let (green, red) = (fit(theme.palette[2]), fit(theme.palette[1]));
        let build_for = |path: PathBuf, slot: TabSlot, frozen: bool| {
            move |window: &mut Window, cx: &mut App| -> AnyView {
                cx.set_global(shot_appearance(theme, cx));
                cx.bind_keys(crate::key_bindings());
                let tab = cx.new(|cx| {
                    let mut tab = FileTab::new(path.clone(), FileMode::Diff, window, cx);
                    tab.frozen = frozen;
                    tab
                });
                *slot.borrow_mut() = Some(tab.clone());
                tab.into()
            }
        };
        let frame = |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
            cx.update_window(window, |_, window, _| window.refresh())
                .expect("refresh");
            cx.run_until_parked();
        };

        // The diff: rows, tints, signs, two number columns, the first change near the top,
        // find and copy.
        let n = format!("file-diff-{slug}");
        let slot: TabSlot = Rc::default();
        let build = build_for(view_rs.clone(), slot.clone(), true);
        let copied: Rc<RefCell<Option<String>>> = Rc::default();
        let seen = copied.clone();
        let geom_seen: Rc<Cell<Option<crate::file_tab::Geom>>> = Rc::default();
        let keep_geom = geom_seen.clone();
        let texts: Rc<RefCell<Vec<String>>> = Rc::default();
        let keep_texts = texts.clone();
        let act = {
            let n = n.clone();
            let unified = unified.clone();
            move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let tab = tab_of(&slot);
                frame(cx, window);
                assert_eq!(
                    cx.read(|cx| tab.read(cx).diff_state_name()),
                    "waiting",
                    "{n}"
                );
                cx.update(|cx| tab.update(cx, |t, cx| t.set_diff_for_shots(&unified, cx)));
                frame(cx, window);
                let (first, lines, fn_count) = cx.read(|cx| {
                    let t = tab.read(cx);
                    assert_eq!(t.mode(), FileMode::Diff, "{n}");
                    assert_eq!(t.diff_state_name(), "rows", "{n}");
                    let d = t.diff_doc().expect("its rows");
                    assert!(d.doc.spans.is_some(), "{n}: coloured");
                    assert_eq!(d.rows[removed].kind, RowKind::Removed, "{n}");
                    assert_eq!(d.rows[added].kind, RowKind::Added, "{n}");
                    assert_eq!(d.rows[added_late].kind, RowKind::Added, "{n}");
                    assert_eq!(d.rows[same].kind, RowKind::Same, "{n}");
                    assert_eq!(d.doc.lines.len(), d.rows.len(), "{n}: a line per row");
                    let lines: Vec<String> = (0..d.rows.len())
                        .map(|i| d.doc.line(i).to_string())
                        .collect();
                    let fns = lines.iter().map(|l| l.to_lowercase().matches("fn").count());
                    (first_change(&d.rows), lines.clone(), fns.sum::<usize>())
                });
                *keep_texts.borrow_mut() = lines;
                let g = cx.read(|cx| tab.read(cx).geom()).expect("drawn");
                let first = first.expect("a change");
                assert_eq!(
                    g.scroll.y,
                    g.line_height * first.saturating_sub(3) as f32,
                    "{n}: opened with its first change 3 rows from the top"
                );
                cx.update_window(window, |_, window, cx| {
                    tab.read(cx).focus_handle(cx).focus(window, cx)
                })
                .expect("focus");
                cx.simulate_keystrokes(window, "cmd-f");
                cx.simulate_input(window, "fn");
                frame(cx, window);
                let label = cx.read(|cx| tab.read(cx).find_label());
                assert_eq!(label, format!("1 of {fn_count}"), "{n}: ⌘F fn in the diff");
                // From the removed row's start to the added row's 5th byte.
                let g = cx.read(|cx| tab.read(cx).geom()).expect("drawn");
                let at = |row: usize, x: f32| {
                    point(
                        g.bounds.left() + g.text_left + px(x),
                        g.bounds.top() + g.line_height * (row as f32 + 0.5) - g.scroll.y,
                    )
                };
                let cw = f32::from(g.cell_width);
                let end = at(added, cw * 5.0 + 1.0);
                cx.simulate_mouse_down(
                    window,
                    at(removed, 0.5),
                    MouseButton::Left,
                    Modifiers::none(),
                );
                cx.simulate_mouse_move(window, end, MouseButton::Left, Modifiers::none());
                cx.simulate_mouse_up(window, end, MouseButton::Left, Modifiers::none());
                cx.simulate_keystrokes(window, "cmd-c");
                *seen.borrow_mut() = cx.read_from_clipboard().and_then(|c| c.text());
                frame(cx, window);
                keep_geom.set(cx.read(|cx| tab.read(cx).geom()));
                slot.borrow_mut().take();
            }
        };
        let path = shoot_with("file-diff", slug, &theme, dims, build.clone(), act.clone());
        println!("ui-shots: {}", path.display());
        let shot = render_with(&theme, dims, build, act);
        let texts = texts.borrow();
        let expect = format!("{}\n{}", texts[removed], &texts[added][..5]);
        assert_eq!(
            copied.borrow().as_deref(),
            Some(expect.as_str()),
            "{n}: ⌘C, no signs"
        );
        let g = geom_seen.get().expect("drawn");
        let at = |x: Pixels, y: Pixels| pixel_at(&shot, dims, f32::from(x), f32::from(y));
        let row_y = |row: usize| g.bounds.top() + g.line_height * (row as f32 + 0.5) - g.scroll.y;
        // The tints, 2 pt inside the right edge (right of the scrollbar's thumb, which the
        // reveal shows, M11ae).
        let right = g.bounds.right() - px(2.0);
        let red_tint = crate::theme::mix(theme.background, red, 0.12);
        let green_tint = crate::theme::mix(theme.background, green, 0.12);
        for (row, tint, what) in [
            (removed, red_tint, "removed"),
            (added, green_tint, "added"),
            (added_late, green_tint, "added"),
            (same, theme.background, "unchanged"),
        ] {
            let p = at(right, row_y(row));
            assert!(
                near(p, tint, 5),
                "{n}: the {what} row's tint {p:?}, not {tint:?}"
            );
        }
        // An unchanged row's colours dimmed (`diff_view::dim`), an added row's not: "pub" is a
        // keyword on both.
        let keyword = fit(theme.palette[5]);
        let dimmed = crate::diff_view::dim(keyword, theme.background);
        let ink_in = |row: usize, c: Rgb, by: u8| {
            (0..300).any(|dx| {
                (-4..=4).any(|dy| {
                    let x = g.bounds.left() + g.text_left + px(dx as f32);
                    near(at(x, row_y(row) + px(dy as f32)), c, by)
                })
            })
        };
        assert!(
            ink_in(same, dimmed, 24),
            "{n}: an unchanged row's keyword in {dimmed:?}"
        );
        assert!(
            ink_in(added_late, keyword, 30),
            "{n}: an added row's keyword in {keyword:?}"
        );
        if !near([dimmed.r, dimmed.g, dimmed.b], keyword, 40) {
            assert!(
                !ink_in(same, keyword, 16),
                "{n}: an unchanged row's keyword not at full colour"
            );
        }
        // The signs, in the one-cell sign column.
        let sign_in = |row: usize, c: Rgb| {
            let top = row_y(row) - g.line_height / 2.0;
            (0..(f32::from(g.cell_width) as i32)).any(|dx| {
                (2..(f32::from(g.line_height) as i32 - 2)).any(|dy| {
                    near(
                        at(
                            g.bounds.left() + g.bar_left + px(dx as f32),
                            top + px(dy as f32),
                        ),
                        c,
                        40,
                    )
                })
            })
        };
        assert!(sign_in(removed, red), "{n}: the − in red");
        assert!(sign_in(added, green), "{n}: the + in green");
        assert!(
            !sign_in(same, red) && !sign_in(same, green),
            "{n}: no sign on an unchanged row"
        );
        // The number columns: (left, right) edges.
        let old_right = g.old_numbers_right.expect("two number columns");
        let old_col = (px(12.0), old_right);
        let new_col = (old_right + px(8.0), g.numbers_right);
        let inked = |row: usize, (l, r): (Pixels, Pixels), base: Rgb| {
            let top = row_y(row) - g.line_height / 2.0;
            (f32::from(l) as i32..f32::from(r) as i32).any(|x| {
                (2..(f32::from(g.line_height) as i32 - 2)).any(|dy| {
                    !near(
                        at(g.bounds.left() + px(x as f32), top + px(dy as f32)),
                        base,
                        6,
                    )
                })
            })
        };
        assert!(
            !inked(added, old_col, green_tint),
            "{n}: no old number on an added row"
        );
        assert!(
            inked(added, new_col, green_tint),
            "{n}: the added row's new number"
        );
        assert!(
            !inked(removed, new_col, red_tint),
            "{n}: no new number on a removed row"
        );
        assert!(
            inked(removed, old_col, red_tint),
            "{n}: the removed row's old number"
        );
        assert!(
            inked(same, old_col, theme.background) && inked(same, new_col, theme.background),
            "{n}: both numbers on an unchanged row"
        );

        // The states: a note instead of rows, or a deleted file's rows.
        let states = [
            ("same", view_rs.clone(), Some(""), "same", Some(NO_CHANGES)),
            (
                "binary",
                view_rs.clone(),
                Some("diff --git a/v b/v\nindex 1..2 100644\nBinary files a/v and b/v differ\n"),
                "binary",
                Some(BINARY),
            ),
            // A file outside any repo, looked up for real (not frozen).
            (
                "norepo",
                dir.join("src/view.rs"),
                None,
                "norepo",
                Some(NO_REPO),
            ),
            (
                "missing-rows",
                missing.clone(),
                Some("@@ -1,2 +0,0 @@\n-fn old() {}\n-// gone\n"),
                "rows",
                None,
            ),
            (
                "missing-same",
                missing.clone(),
                Some(""),
                "same",
                Some(MISSING),
            ),
        ];
        for (case, file, given, state, note) in states {
            let n = format!("file-diff-states-{case}-{slug}");
            let slot: TabSlot = Rc::default();
            let build = build_for(file, slot.clone(), given.is_some());
            let act = {
                let n = n.clone();
                move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                    let tab = tab_of(&slot);
                    frame(cx, window);
                    if let Some(u) = given {
                        cx.update(|cx| tab.update(cx, |t, cx| t.set_diff_for_shots(u, cx)));
                    }
                    frame(cx, window);
                    cx.read(|cx| {
                        let t = tab.read(cx);
                        assert_eq!(t.diff_state_name(), state, "{n}");
                        assert_eq!(t.shown_note(), note, "{n}");
                        if case.starts_with("missing") {
                            assert_eq!(t.state_name(), "missing", "{n}");
                        }
                    });
                    slot.borrow_mut().take();
                }
            };
            let path = shoot_with(
                &format!("file-diff-states-{case}"),
                slug,
                &theme,
                dims,
                build.clone(),
                act.clone(),
            );
            println!("ui-shots: {}", path.display());
            // What the body shows: the note's words in the middle, or the removed rows' tint.
            let shot = render_with(&theme, dims, build, act);
            let (w, h) = (f32::from(dims.width), f32::from(dims.height));
            let middle = (0..(w as i32)).step_by(2).any(|x| {
                // The body's middle, under the toolbar.
                (-40..=40).any(|dy| {
                    near(
                        pixel_at(&shot, dims, x as f32, h / 2.0 + dy as f32),
                        ui.text,
                        40,
                    )
                })
            });
            if note.is_some() {
                assert!(middle, "{n}: the note's words in the middle");
            } else {
                let red_tint = crate::theme::mix(theme.background, red, 0.12);
                let p = pixel_at(&shot, dims, w - 6.0, 70.0);
                assert!(
                    near(p, red_tint, 5),
                    "{n}: a removed row {p:?}, not {red_tint:?}"
                );
            }
        }

        // The toggle, in a workspace with the Files panel (as the files scenes lay it out).
        let window_dims = size(px(1100.0), px(560.0));
        let sidebar_left = 1100.0 - GAP - 300.0 + FOCUS_RING;
        let list_top = GAP + FOCUS_RING + TAB_BAR + HEADER + FILTER_BAND + LIST_TOP;
        let build_ws = |slot: Slot| {
            let dir = dir.clone();
            move |window: &mut Window, cx: &mut App| -> AnyView {
                use crate::file_tree::{CAP, FileTree};
                use crate::git_status::list_dir;
                cx.set_global(shot_appearance(theme, cx));
                cx.bind_keys(crate::key_bindings());
                let ws = cx.new(|cx| {
                    let mut ws = Workspace::for_shots(window, cx);
                    ws.sidebar.hint_dismissed = true;
                    ws.right_sidebar.hidden = false;
                    let src = dir.join("src");
                    ws.open_shot_session("api", &src, SHELL_TEXT, None, window, cx);
                    let mut tree = FileTree::new(dir.clone(), Some(PathBuf::from("src")));
                    for rel in tree.wanted() {
                        let (entries, more) = list_dir(&dir.join(&rel), CAP);
                        tree.expand(rel, entries, more);
                    }
                    ws.right_sidebar.files.show_for_shots(tree);
                    ws
                });
                *slot.borrow_mut() = Some(ws.clone());
                ws.into()
            }
        };
        let open_diff = {
            let view_rs = view_rs.clone();
            let unified = unified.clone();
            move |cx: &mut VisualTestAppContext,
                  window: AnyWindowHandle,
                  ws: &gpui::Entity<Workspace>| {
                cx.update_window(window, |_, window, cx| {
                    ws.update(cx, |ws, cx| {
                        ws.open_file_tab(view_rs.clone(), FileMode::Diff, window, cx)
                    })
                })
                .expect("open the diff");
                frame(cx, window);
                let view = cx.read(|cx| ws.read(cx).focused_file_tab().expect("its view").clone());
                cx.update(|cx| view.update(cx, |t, cx| t.set_diff_for_shots(&unified, cx)));
                frame(cx, window);
                view
            }
        };
        let n = format!("file-diff-toggle-{slug}");
        let toggle: Rc<Cell<[Option<gpui::Bounds<Pixels>>; 3]>> = Rc::default();
        let slot: Slot = Rc::new(RefCell::new(None));
        let build = build_ws(slot.clone());
        let act = {
            let n = n.clone();
            let toggle = toggle.clone();
            let view_rs = view_rs.clone();
            let open_diff = open_diff.clone();
            move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let ws = slotted(&slot);
                frame(cx, window);
                let view = open_diff(cx, window, &ws);
                cx.read(|cx| {
                    let ws = ws.read(cx);
                    let tab = ws.model.focused_tab().expect("a tab");
                    let kind = ws.model.tab(tab).expect("known").kind.clone();
                    assert_eq!(
                        kind,
                        TabKind::File {
                            path: view_rs.clone(),
                            mode: FileMode::Diff
                        },
                        "{n}"
                    );
                    assert_eq!(view.read(cx).diff_state_name(), "rows", "{n}");
                });
                toggle.set(cx.read(|cx| view.read(cx).toggle_bounds()));
                slot.borrow_mut().take();
            }
        };
        let path = shoot_with(
            "file-diff-toggle",
            slug,
            &theme,
            window_dims,
            build.clone(),
            act.clone(),
        );
        println!("ui-shots: {}", path.display());
        let shot = render_with(&theme, window_dims, build, act);
        let [file_seg, diff_seg, preview_seg] = toggle.get();
        let (file_seg, diff_seg) = (file_seg.expect("File drawn"), diff_seg.expect("Diff drawn"));
        assert!(preview_seg.is_none(), "{n}: no Preview for a .rs file");
        // Inside each segment's padding (no words there).
        let fill_of = |b: gpui::Bounds<Pixels>| {
            pixel_at(
                &shot,
                window_dims,
                f32::from(b.left()) + 5.0,
                f32::from(b.center().y),
            )
        };
        let p = fill_of(diff_seg);
        assert!(
            near(p, ui.card_selected, 3),
            "{n}: Diff is current {p:?}, not {:?}",
            ui.card_selected
        );
        let p = fill_of(file_seg);
        assert!(
            near(p, ui.panel, 3),
            "{n}: File is not {p:?}, not {:?}",
            ui.panel
        );
        assert!(
            file_seg.right() <= diff_seg.left() + px(1.0),
            "{n}: File, then Diff"
        );

        // Clicks: File, a change bar, a Files row (not shot).
        let slot: Slot = Rc::new(RefCell::new(None));
        let build = build_ws(slot.clone());
        let act = {
            let n = format!("file-diff-toggle-clicks-{slug}");
            let view_rs = view_rs.clone();
            move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let ws = slotted(&slot);
                frame(cx, window);
                let view = open_diff(cx, window, &ws);
                let tab = cx.read(|cx| ws.read(cx).model.focused_tab().expect("a tab"));
                let modes = |cx: &mut VisualTestAppContext| {
                    cx.read(|cx| {
                        let ws = ws.read(cx);
                        let model = match &ws.model.tab(tab).expect("the same tab").kind {
                            TabKind::File { mode, .. } => *mode,
                            other => panic!("not a file tab: {other:?}"),
                        };
                        assert_eq!(ws.model.focused_tab(), Some(tab), "the same tab");
                        assert_eq!(ws.file_tabs.len(), 1, "one view");
                        (view.read(cx).mode(), model)
                    })
                };
                // Each mode keeps its own scroll: the diff scrolled, File (scrolled too), Diff
                // again, File again, by the toggle both ways.
                cx.update(|cx| view.update(cx, |t, cx| t.pretend_in_repo_for_shots(cx)));
                frame(cx, window);
                let lh = cx
                    .read(|cx| view.read(cx).geom())
                    .expect("drawn")
                    .line_height;
                let segment = |cx: &mut VisualTestAppContext, i: usize| {
                    cx.read(|cx| view.read(cx).toggle_bounds()[i])
                        .expect("the toggle")
                        .center()
                };
                let scroll = |cx: &mut VisualTestAppContext| cx.read(|cx| view.read(cx).scroll());
                cx.update(|cx| view.update(cx, |t, cx| t.set_scroll(point(px(0.0), lh * 5.0), cx)));
                frame(cx, window);
                let diff_at = scroll(cx);
                assert_eq!(diff_at.y, lh * 5.0, "{n}: the diff scrolled");
                let at = segment(cx, 0);
                cx.simulate_click(window, at, Modifiers::none());
                frame(cx, window);
                assert_eq!(modes(cx), (FileMode::Plain, FileMode::Plain), "{n}: File");
                assert_eq!(scroll(cx).y, px(0.0), "{n}: the file's own scroll");
                cx.update(|cx| view.update(cx, |t, cx| t.set_scroll(point(px(0.0), lh * 2.0), cx)));
                frame(cx, window);
                let at = segment(cx, 1);
                cx.simulate_click(window, at, Modifiers::none());
                frame(cx, window);
                assert_eq!(modes(cx), (FileMode::Diff, FileMode::Diff), "{n}: Diff");
                assert_eq!(scroll(cx), diff_at, "{n}: the diff's scroll kept");
                let at = segment(cx, 0);
                cx.simulate_click(window, at, Modifiers::none());
                frame(cx, window);
                assert_eq!(scroll(cx).y, lh * 2.0, "{n}: the file's scroll kept");
                let at = segment(cx, 1);
                cx.simulate_click(window, at, Modifiers::none());
                frame(cx, window);
                assert_eq!(modes(cx), (FileMode::Diff, FileMode::Diff), "{n}: Diff");
                // Scrolled to the diff's end, away from its changes.
                cx.update(|cx| {
                    view.update(cx, |t, cx| t.set_scroll(point(px(0.0), px(1.0e5)), cx))
                });
                frame(cx, window);
                let file_seg = cx
                    .read(|cx| view.read(cx).toggle_bounds()[0])
                    .expect("File");
                cx.simulate_click(window, file_seg.center(), Modifiers::none());
                frame(cx, window);
                assert_eq!(modes(cx), (FileMode::Plain, FileMode::Plain), "{n}: File");
                // Line 7's change bar: the diff, at that change.
                cx.update(|cx| {
                    view.update(cx, |t, cx| {
                        t.set_changes(vec![(7, LineChange::Changed)], cx)
                    })
                });
                frame(cx, window);
                let g = cx.read(|cx| view.read(cx).geom()).expect("drawn");
                let bar = point(
                    g.bounds.left() + g.bar_left + px(1.5),
                    g.bounds.top() + g.line_height * 6.5 - g.scroll.y,
                );
                cx.simulate_click(window, bar, Modifiers::none());
                frame(cx, window);
                assert_eq!(
                    modes(cx),
                    (FileMode::Diff, FileMode::Diff),
                    "{n}: a bar's click"
                );
                let (row, g) = cx.read(|cx| {
                    let t = view.read(cx);
                    let rows = &t.diff_doc().expect("its rows").rows;
                    (
                        reveal_row(rows, (7, LineChange::Changed)).expect("line 7's row"),
                        t.geom(),
                    )
                });
                let g = g.expect("drawn");
                let (top, h) = (g.line_height * row as f32, g.bounds.size.height);
                assert!(
                    top >= g.scroll.y && top + g.line_height <= g.scroll.y + h,
                    "{n}: row {row} in view ({:?})",
                    g.scroll
                );
                // A Files click on the same file: the same tab, Plain.
                let row_at = |i: usize| {
                    point(
                        px(sidebar_left + 80.0),
                        px(list_top + (i as f32 + 0.5) * ROW),
                    )
                };
                let rows = cx.read(|cx| {
                    let files = &ws.read(cx).right_sidebar.files;
                    files
                        .rows
                        .iter()
                        .map(|r| r.rel.display().to_string())
                        .collect::<Vec<_>>()
                });
                let i = rows
                    .iter()
                    .position(|r| r == "src/view.rs")
                    .expect("view.rs listed");
                cx.simulate_click(window, row_at(i), Modifiers::none());
                frame(cx, window);
                assert_eq!(
                    modes(cx),
                    (FileMode::Plain, FileMode::Plain),
                    "{n}: a Files click"
                );
                let path = cx.read(
                    |cx| match &ws.read(cx).model.tab(tab).expect("known").kind {
                        TabKind::File { path, .. } => path.clone(),
                        _ => unreachable!(),
                    },
                );
                assert_eq!(path, view_rs, "{n}");
                slot.borrow_mut().take();
            }
        };
        render_with(&theme, window_dims, build, act);
        println!("ui-shots: file-diff-toggle-clicks-{slug} ok");
    }
}

/// The file tab scenes' Rust fixture: comments, strings, numbers, keywords, types, functions,
/// and "fn" in several places (one in upper case).
const FILE_VIEW_FIXTURE: &str = r#"// A small server, for the file tab's shots.
use std::collections::HashMap;
use std::time::Duration;

/// How long a request may take.
const TIMEOUT: Duration = Duration::from_secs(30);
const NAME: &str = "fuxx";

pub struct Server {
    routes: HashMap<String, Route>,
    port: u16,
}

impl Server {
    pub fn new(port: u16) -> Self {
        Self { routes: HashMap::new(), port }
    }

    /// Add a route; FN-style handlers only.
    pub fn route(&mut self, path: &str, handler: fn() -> String) {
        let route = Route { handler, hits: 0 };
        self.routes.insert(path.to_string(), route);
    }
}

struct Route {
	handler: fn() -> String,
	hits: u64,
}

fn main() {
    let mut server = Server::new(8080);
    server.route("/health", || "ok".to_string());
    println!("listening on {} (timeout {:?})", server.port, TIMEOUT);
}
"#;
