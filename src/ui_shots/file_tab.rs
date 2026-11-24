//! The file tab (M11g, M11ae): the tab in a pane, its viewer, file bar, drag-select, overlay
//! scrollbar, and its Diff mode (M11h).

use super::branch::{scratch_git, settle_until};
use super::*;

/// This file, as `file!()` gives it (repo-relative under cargo): `registry::AREAS` pairs it with
/// `SCENES`, so `--changed` runs these scenes when this file changes.
pub(super) const FILE: &str = file!();

/// This area's scenes (`registry.rs` runs them in its `ORDER`).
pub(super) const SCENES: &[Scene] = &[
    Scene {
        name: "file-tab",
        run: file_tab_scenes,
        themes: Themes::Both,
        serial: false,
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
        serial: false,
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
        // Serial: (c) the thumb still shown after a scroll (it fades 1 s on, real clock).
        serial: true,
        kind: Kind::State,
        sources: &["src/file_tab.rs", "src/scrollbar.rs"],
    },
    Scene {
        name: "file-live-glow",
        run: file_live_glow_scenes,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &["src/file_tab.rs", "src/live_lines.rs"],
    },
    Scene {
        name: "file-live-marks",
        run: file_live_marks_scenes,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &["src/file_tab.rs", "src/live_lines.rs"],
    },
    Scene {
        name: "file-live-none",
        run: file_live_none_scenes,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &["src/file_tab.rs", "src/live_lines.rs"],
    },
    Scene {
        name: "file-live-tooltip",
        run: file_live_tooltip_scenes,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/file_tab.rs",
            "src/live_lines.rs",
            "src/sidebar_toggle.rs",
        ],
    },
    Scene {
        name: "file-live-modes",
        run: file_live_modes_scenes,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/file_tab.rs",
            "src/live_lines.rs",
            "src/diff_view.rs",
            "src/md_preview.rs",
        ],
    },
    Scene {
        name: "file-follow-pill",
        run: file_follow_pill_scenes,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &["src/file_tab.rs", "src/live_lines.rs"],
    },
    Scene {
        name: "file-follow-scrolls",
        run: file_follow_scrolls_scenes,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: &["src/file_tab.rs", "src/live_lines.rs"],
    },
    Scene {
        name: "file-follow-wheel-stops",
        run: file_follow_wheel_stops_scenes,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: &["src/file_tab.rs", "src/live_lines.rs"],
    },
    Scene {
        name: "file-follow-mode",
        run: file_follow_mode_scenes,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: &["src/file_tab.rs", "src/live_lines.rs"],
    },
    Scene {
        name: "file-code-calm",
        run: file_code_calm_scenes,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &["src/file_tab.rs", "src/ui_tokens.rs"],
    },
    Scene {
        name: "file-code-guides",
        run: file_code_guides_scenes,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &["src/file_tab.rs", "src/code_outline.rs", "src/ui_tokens.rs"],
    },
    Scene {
        name: "file-code-bars",
        run: file_code_bars_scenes,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &["src/file_tab.rs", "src/ui_tokens.rs"],
    },
    Scene {
        name: "file-code-fold",
        run: file_code_fold_scenes,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/file_tab.rs",
            "src/code_outline.rs",
            "src/row_map.rs",
            "src/live_lines.rs",
            "src/ui_tokens.rs",
        ],
    },
    Scene {
        name: "file-code-nav",
        run: file_code_nav_scenes,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: &[
            "src/file_tab.rs",
            "src/row_map.rs",
            "src/tab_bar.rs",
            "src/main.rs",
            "src/ui_tokens.rs",
        ],
    },
    Scene {
        name: "file-code-fold-keys",
        run: file_code_fold_keys_scenes,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: &[
            "src/file_tab.rs",
            "src/code_outline.rs",
            "src/row_map.rs",
            "src/main.rs",
        ],
    },
    Scene {
        name: "file-code-sticky",
        run: file_code_sticky_scenes,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/file_tab.rs",
            "src/code_outline.rs",
            "src/row_map.rs",
            "src/ui_tokens.rs",
        ],
    },
    Scene {
        name: "file-code-lane",
        run: file_code_lane_scenes,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/file_tab.rs",
            "src/scrollbar.rs",
            "src/row_map.rs",
            "src/ui_tokens.rs",
        ],
    },
    Scene {
        name: "file-diff-cards",
        run: file_diff_cards_scenes,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/file_tab.rs",
            "src/diff_cards.rs",
            "src/diff_view.rs",
            "src/row_map.rs",
            "src/code_outline.rs",
            "src/scrollbar.rs",
            "src/ui_tokens.rs",
        ],
    },
    Scene {
        name: "file-diff-cards-fold",
        run: file_diff_cards_fold_scenes,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: &[
            "src/file_tab.rs",
            "src/diff_cards.rs",
            "src/row_map.rs",
            "src/main.rs",
            "src/palette_items.rs",
        ],
    },
    Scene {
        name: "file-diff-words",
        run: file_diff_words_scenes,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/file_tab.rs",
            "src/diff_cards.rs",
            "src/diff_view.rs",
            "src/row_map.rs",
            "src/ui_tokens.rs",
        ],
    },
    Scene {
        name: "file-diff-side",
        run: file_diff_side_scenes,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/file_tab.rs",
            "src/diff_cards.rs",
            "src/diff_view.rs",
            "src/row_map.rs",
            "src/tab_bar.rs",
            "src/ui_tokens.rs",
        ],
    },
    Scene {
        name: "file-diff-side-narrow",
        run: file_diff_side_narrow_scenes,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: &["src/file_tab.rs", "src/diff_cards.rs", "src/row_map.rs"],
    },
    Scene {
        name: "file-code-breadcrumb",
        run: file_code_breadcrumb_scenes,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/file_tab.rs",
            "src/code_outline.rs",
            "src/highlight.rs",
            "src/ui_tokens.rs",
        ],
    },
];

/// A file tab (M11g) in a pane beside a terminal's tab: opened twice for one file it is one tab,
/// shown, with the keyboard; its bar tab draws the file glyph; closing it leaves the terminal,
/// which takes the keyboard back. Its file isn't on this Mac, so it shows "This file no longer
/// exists", and its Close button closes the tab. Shot once per theme (opened). Then the viewer
/// itself (`file_view_scenes`).
fn file_tab_scenes() {
    use crate::layout_model::TabKind;
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
        // Its bar tab draws the glyph in its tile's ink (`tile_ink`: a file's `text_dim`; it
        // passed as the tab's text colour only while the two were close, before the palette
        // chore set dim text apart).
        let shot = render_with(&theme, dims, build, act);
        let tab = tab_at.get().expect("the file tab was laid out");
        let text = crate::tab_bar::tile_ink(&ui, crate::tab_bar::TabIcon::File);
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
        // Its outer bar (M11al §9) sits in the card: the card's fill, the card's hairline under
        // it, nothing drawn in the first 16 pt (the title starts there). `FILE_TOOLBAR` is 42.
        {
            use crate::ui_tokens::size::{HAIRLINE, SIDEBAR_DEFAULT_WIDTH, TAB_BAR};
            let pane_left = CARD_INSET + HAIRLINE + SIDEBAR_DEFAULT_WIDTH;
            let bar_top = CARD_INSET + HAIRLINE + TAB_BAR;
            let p = pixel_at(&shot, dims, pane_left + 8.0, bar_top + 21.0);
            assert!(
                near_by(p, ui.card, 3),
                "file-tab-{slug}: the file bar is {p:?}, not the card {:?}",
                ui.card
            );
            let p = pixel_at(&shot, dims, pane_left + 8.0, bar_top + 41.5);
            assert!(
                near_by(p, ui.card_border, 3),
                "file-tab-{slug}: the file bar's line is {p:?}, not {:?}",
                ui.card_border
            );
        }
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

    let dir = crate::test_temp::dir("fuxx-ui-shots-file-tab-drag-select");
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
    let base = crate::test_temp::dir("fuxx-ui-shots-header");
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
            // Its floor as laid out: one em, snapped to device pixels (12.5 is 12 at 1x).
            let floor = cx
                .update_window(window, |_, window, _| {
                    window.pixel_snap(px(ui_tokens::text::META))
                })
                .expect("the window");
            assert!(
                place.size.width >= floor,
                "{n}: Where never vanishes, its `…` stays ({place:?}, floor {floor:?})"
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

    let dir = crate::test_temp::dir("fuxx-ui-shots-file-tab");
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
        // Line 3 (0-based 2) is where the drag ended: the current line, its tint over the band.
        let band = crate::theme::mix(
            theme.background,
            theme.ui().text,
            crate::ui_tokens::code::CURRENT_ALPHA,
        );
        let tint = crate::theme::mix(band, green, 0.12);
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
        // The last line, tinted, above the room (left of the scrollbar's lane, which marks it) ...
        let room = ui_tokens::size::CARD_RADIUS;
        let p = pixel_at(&shot, dims, w - 14.0, h - room - 2.0);
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
    let lines = h.colour(std::path::Path::new("big.rs"), &text).spans;
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

    let dir = crate::test_temp::dir("fuxx-ui-shots-file-tab-scrollbar");
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
    use crate::ui_tokens::size::{HAIRLINE, TAB_BAR};
    use crate::ui_tokens::{MARK_MIN, ensure_contrast};
    use alacritty_terminal::vte::ansi::Rgb;
    use gpui::{Focusable, Modifiers, MouseButton};
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    let dir = crate::test_temp::dir("fuxx-ui-shots-file-diff");
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
    // M11ah: taller, for the cards' titles: the second card's added row stays in sight.
    let dims = size(px(760.0), px(560.0));
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
        // M11ah: each row's top down the text, read from the tab's row map (cards and their
        // titles move rows down).
        let tops: Rc<RefCell<Vec<Pixels>>> = Rc::default();
        let keep_tops = tops.clone();
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
                // M11ah: opened at its first card, its title at the top, the change 3 rows
                // under it.
                let (title, change) = cx.read(|cx| {
                    let t = tab.read(cx);
                    (t.title_top(0, cx), t.line_top(first, cx))
                });
                let (title, change) = (title.expect("a card"), change.expect("its row"));
                assert_eq!(g.scroll.y, title, "{n}: opened at its first card");
                assert_eq!(
                    change - title - px(crate::ui_tokens::code::CARD_TITLE),
                    g.line_height * 3.0,
                    "{n}: its first change 3 rows under the title"
                );
                let row_tops: Vec<Pixels> = cx.read(|cx| {
                    let t = tab.read(cx);
                    (0..=added_late)
                        .map(|r| t.line_top(r, cx).expect("shown"))
                        .collect()
                });
                *keep_tops.borrow_mut() = row_tops.clone();
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
                        g.bounds.top() + row_tops[row] + g.line_height / 2.0 - g.scroll.y,
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
        let tops = tops.borrow();
        let row_y = |row: usize| g.bounds.top() + tops[row] + g.line_height / 2.0 - g.scroll.y;
        // The tints, inside the card's right side (left of the scrollbar's thumb, which the
        // reveal shows, M11ae).
        let right = g.bounds.right() - px(crate::ui_tokens::code::CARD_INSET + 16.0);
        let red_tint = crate::theme::mix(theme.background, red, 0.12);
        let green_tint = crate::theme::mix(theme.background, green, 0.12);
        // The row the drag ended on is the current row: its tint over the band.
        let band = crate::theme::mix(
            theme.background,
            theme.ui().text,
            crate::ui_tokens::code::CURRENT_ALPHA,
        );
        let green_current = crate::theme::mix(band, green, 0.12);
        for (row, tint, what) in [
            (removed, red_tint, "removed"),
            (added, green_current, "added (current)"),
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
        let old_col = (
            px(crate::ui_tokens::code::CARD_INSET + crate::ui_tokens::code::GUTTER_LEFT),
            old_right,
        );
        let new_col = (
            old_right + px(crate::ui_tokens::code::NUMBER_GAP),
            g.numbers_right,
        );
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
            !inked(added, old_col, green_current),
            "{n}: no old number on an added row"
        );
        assert!(
            inked(added, new_col, green_current),
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
                // M11ah: inside its one card, under the card's title.
                let card = crate::ui_tokens::code::CARD_INSET + 16.0;
                let p = pixel_at(
                    &shot,
                    dims,
                    w - card,
                    70.0 + crate::ui_tokens::code::CARD_TITLE,
                );
                assert!(
                    near(p, red_tint, 5),
                    "{n}: a removed row {p:?}, not {red_tint:?}"
                );
            }
        }

        // The toggle, in a workspace with the Files panel (as the files scenes lay it out).
        let window_dims = size(px(1100.0), px(560.0));
        let sidebar_left = right_panel_left(1100.0, 300.0);
        let list_top = CARD_INSET + HAIRLINE + TAB_BAR + HEADER + FILTER_BAND + LIST_TOP;
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
                let (row, top, title, g) = cx.read(|cx| {
                    let t = view.read(cx);
                    let rows = &t.diff_doc().expect("its rows").rows;
                    let row = reveal_row(rows, (7, LineChange::Changed)).expect("line 7's row");
                    (row, t.line_top(row, cx), t.title_top(0, cx), t.geom())
                });
                let (g, top) = (g.expect("drawn"), top.expect("its row shown"));
                let h = g.bounds.size.height;
                // M11ah: at its card, the card's title at the top.
                assert_eq!(Some(g.scroll.y), title, "{n}: the bar's card at the top");
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

// M11af Task 6: the glow, the age words and the marks (a file tab's live changes).

/// Fourteen short lines, so a run's words have room after its first line.
const LIVE_FIXTURE: &str = "fn main() {
    let a = 1;
    let b = 2;
    let c = 3;
    let d = 4;
    let e = 5;
    let f = 6;
    let g = 7;
    let h = 8;
    let i = 9;
    let j = 10;
    let k = 11;
    let l = 12;
}
";

/// The scenes' clock: every age is counted back from it (frozen tabs never poll).
fn live_now() -> std::time::SystemTime {
    std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(2_000_000_000)
}

/// A record of `lines` lines, each `(block, who, sure, seconds ago)` applied in turn (lines
/// changed in place), as a read of the file would make it.
fn live_record(
    lines: usize,
    changes: &[(
        std::ops::Range<usize>,
        crate::live_lines::Who,
        crate::live_lines::Sure,
        u64,
    )],
) -> crate::live_lines::LineRecord {
    use crate::live_lines::{Compared, LineRecord, Mark};
    let mut record = LineRecord::baseline(lines);
    for (block, who, sure, ago) in changes {
        let c = Compared {
            new_of_old: (0..lines)
                .map(|i| (!block.contains(&i)).then_some(i))
                .collect(),
            blocks: vec![block.clone()],
        };
        let mark = Mark {
            who: who.clone(),
            sure: *sure,
            when: live_now() - std::time::Duration::from_secs(*ago),
        };
        record.apply(&c, lines, mark);
    }
    record
}

type LiveSlot = std::rc::Rc<std::cell::RefCell<Option<gpui::Entity<crate::file_tab::FileTab>>>>;
/// What a scene read of its tab: its geometry and the age words drawn (line, words).
type LiveSeen = Option<(crate::file_tab::Geom, Vec<(usize, String)>)>;

/// A frozen file tab on `path` alone in the window (kept in `slot`).
fn live_tab(
    theme: Theme,
    path: PathBuf,
    slot: LiveSlot,
) -> impl FnOnce(&mut Window, &mut App) -> AnyView {
    move |window: &mut Window, cx: &mut App| -> AnyView {
        cx.set_global(shot_appearance(theme, cx));
        cx.bind_keys(crate::key_bindings());
        let tab = cx.new(|cx| {
            let mut tab = crate::file_tab::FileTab::new(
                path,
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
}

fn live_frame(cx: &mut VisualTestAppContext, window: AnyWindowHandle) {
    cx.update_window(window, |_, window, _| window.refresh())
        .expect("refresh");
    cx.run_until_parked();
}

/// The fixture folder, with `name` written as `text`.
fn live_file(name: &str, text: &str) -> PathBuf {
    let dir = crate::test_temp::dir("fuxx-ui-shots-file-live");
    std::fs::create_dir_all(&dir).expect("the fixture folder");
    let dir = std::fs::canonicalize(&dir).unwrap_or(dir);
    let path = dir.join(name);
    std::fs::write(&path, text).expect("the fixture");
    path
}

fn live_near(p: [u8; 3], c: alacritty_terminal::vte::ansi::Rgb, by: u8) -> bool {
    p.iter()
        .zip([c.r, c.g, c.b])
        .all(|(a, b)| a.abs_diff(b) <= by)
}

/// The gutter's text edge of a file with blocks (`LIVE_FIXTURE`'s) with no marks' column:
/// `code::GUTTER_LEFT`, two digits, `NUMBER_GAP`, the fold column (M11ah), `BAR_WIDTH`,
/// `TEXT_GAP`.
fn plain_text_left(cell: f32) -> f32 {
    use crate::ui_tokens::code;
    code::GUTTER_LEFT
        + 2.0 * cell
        + code::NUMBER_GAP
        + code::FOLD_COLUMN
        + code::BAR_WIDTH
        + code::TEXT_GAP
}

/// `file-live-glow` (both themes): runs changed 2 s, 20 s, 40 s and 70 s ago (lines 2–3, 6,
/// 9–10 and 13): the first three tinted the warm accent at 20 %, 11 % and 5 % over the whole
/// row, the fourth (cooled) and the unchanged lines not; the words `just now`, `20 s ago` and
/// `40 s ago` once per run, in the accent, after its first line's end (none after its other
/// lines, none for the cooled run); the marks' column open (the text 16 pt further right).
fn file_live_glow_scenes() {
    use crate::file_tab::{GLOW, WARM_RGB};
    use crate::live_lines::{Sure, Who};
    use crate::theme::mix;
    use crate::ui_tokens::{TEXT_MIN, ensure_contrast};
    use std::cell::RefCell;
    use std::rc::Rc;

    let path = live_file("glow.rs", LIVE_FIXTURE);
    let dims = size(px(760.0), px(360.0));
    for &(slug, name) in shot_themes() {
        let n = format!("file-live-glow-{slug}");
        let theme = shot_theme(name);
        let slot: LiveSlot = Rc::default();
        let build = live_tab(theme, path.clone(), slot.clone());
        let seen: Rc<RefCell<LiveSeen>> = Rc::default();
        let keep = seen.clone();
        let widths: Rc<RefCell<Vec<usize>>> = Rc::default();
        let keep_widths = widths.clone();
        let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
            let tab = slot.borrow().clone().expect("the file tab");
            live_frame(cx, window);
            let record = live_record(
                14,
                &[
                    (1..3, Who::You, Sure::Exact, 2),
                    (5..6, Who::You, Sure::Exact, 20),
                    (8..10, Who::You, Sure::Exact, 40),
                    (12..13, Who::You, Sure::Exact, 70),
                ],
            );
            cx.update(|cx| tab.update(cx, |t, _| t.set_live_for_shot(record, live_now(), None)));
            live_frame(cx, window);
            *keep.borrow_mut() = cx.read(|cx| {
                let t = tab.read(cx);
                *keep_widths.borrow_mut() = t.doc().expect("its text").widths.to_vec();
                Some((t.geom().expect("drawn"), t.live_words()))
            });
        };
        let shot = render_with(&theme, dims, build, act);
        keep_shot(&n, &shot);
        let (g, words) = seen.borrow().clone().expect("read");
        let widths = widths.borrow().clone();
        let words: Vec<(usize, &str)> = words.iter().map(|(l, w)| (*l, w.as_str())).collect();
        assert_eq!(
            words,
            [(1, "just now"), (5, "20 s ago"), (8, "40 s ago")],
            "{n}: one age per warm run, at its first line"
        );
        let cw = f32::from(g.cell_width);
        assert!(g.marks_left.is_some(), "{n}: the marks' column is open");
        assert_eq!(
            f32::from(g.text_left),
            plain_text_left(cw) + 16.0,
            "{n}: the text 16 pt further right"
        );
        let bg = theme.background;
        let at = |x: f32, line: usize| {
            let y = f32::from(g.bounds.top() + g.line_height * (line as f32 + 0.5));
            pixel_at(&shot, dims, x, y)
        };
        let right = f32::from(g.bounds.right()) - 6.0;
        for (line, level) in [(1, 3), (2, 3), (5, 2), (8, 1), (9, 1)] {
            let want = mix(bg, WARM_RGB, GLOW[level - 1]);
            let p = at(right, line);
            println!("ui-shots: {n}: line {line} level {level}: {p:?} (want {want:?})");
            assert!(
                live_near(p, want, 3),
                "{n}: line {line}'s glow {p:?}, not {want:?}"
            );
            assert!(!live_near(p, bg, 1), "{n}: line {line} tinted");
        }
        for line in [0, 3, 4, 12] {
            let p = at(right, line);
            assert!(live_near(p, bg, 1), "{n}: line {line} untinted: {p:?}");
        }
        // The words: in the accent (readable on the background), after the line's end.
        let ink = ensure_contrast(WARM_RGB, bg, TEXT_MIN);
        let inked = |line: usize| {
            let from = f32::from(g.bounds.left() + g.text_left) + cw * widths[line] as f32 + 4.0;
            let y0 = f32::from(g.bounds.top() + g.line_height * line as f32);
            (0..140).any(|dx| {
                (2..(f32::from(g.line_height) as i32 - 2)).any(|dy| {
                    let p = pixel_at(&shot, dims, from + dx as f32 * 0.5, y0 + dy as f32);
                    live_near(p, ink, 48)
                })
            })
        };
        for line in [1, 5, 8] {
            assert!(inked(line), "{n}: the age words after line {line}");
        }
        for line in [0, 2, 9, 12] {
            assert!(!inked(line), "{n}: no words after line {line}");
        }
        println!("ui-shots: {n} ok");
    }
}

/// `file-live-marks` (both themes): four lines changed a while ago (nothing glows), each with
/// its mark in the column between the numbers and the bar: Claude's exact change a filled
/// circle in its colour with its letter; Claude's guessed one hollow (its ring in that colour);
/// codex's (a program fuxx has no colour for) hollow and grey; yours filled teal. Unmarked lines
/// leave the column empty.
fn file_live_marks_scenes() {
    use crate::edit_ledger::agent_mark_for;
    use crate::live_lines::{Sure, Who};
    use crate::tab_color::TabColor;
    use crate::ui_tokens::{MARK_MIN, ensure_contrast};
    use alacritty_terminal::vte::ansi::Rgb;
    use std::cell::Cell;
    use std::rc::Rc;

    let path = live_file("marks.rs", LIVE_FIXTURE);
    let dims = size(px(760.0), px(360.0));
    let claude = agent_mark_for("claude");
    let codex = agent_mark_for("codex");
    assert_eq!(codex.rgb, None, "codex has no colour of its own");
    for &(slug, name) in shot_themes() {
        let n = format!("file-live-marks-{slug}");
        let theme = shot_theme(name);
        let ui = theme.ui();
        let slot: LiveSlot = Rc::default();
        let build = live_tab(theme, path.clone(), slot.clone());
        let seen: Rc<Cell<Option<crate::file_tab::Geom>>> = Rc::default();
        let keep = seen.clone();
        let (claude_mark, codex) = (claude.clone(), codex.clone());
        let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
            let tab = slot.borrow().clone().expect("the file tab");
            live_frame(cx, window);
            let record = live_record(
                14,
                &[
                    (1..2, Who::Agent(claude_mark.clone()), Sure::Exact, 90),
                    (3..4, Who::Agent(claude_mark), Sure::Guess, 90),
                    (5..6, Who::Agent(codex), Sure::Guess, 90),
                    (7..8, Who::You, Sure::Exact, 90),
                ],
            );
            cx.update(|cx| tab.update(cx, |t, _| t.set_live_for_shot(record, live_now(), None)));
            live_frame(cx, window);
            keep.set(cx.read(|cx| tab.read(cx).geom()));
        };
        let shot = render_with(&theme, dims, build, act);
        keep_shot(&n, &shot);
        let g = seen.get().expect("drawn");
        let cw = f32::from(g.cell_width);
        assert_eq!(
            f32::from(g.text_left),
            plain_text_left(cw) + 16.0,
            "{n}: the marks' column, 16 pt"
        );
        let left = f32::from(g.bounds.left() + g.marks_left.expect("the marks' column"));
        assert!(
            left > f32::from(g.bounds.left() + g.numbers_right),
            "{n}: after the numbers"
        );
        assert!(
            left + 16.0 <= f32::from(g.bounds.left() + g.bar_left),
            "{n}: before the bar"
        );
        let bg = theme.background;
        let centre = |line: usize| {
            (
                left + 8.0,
                f32::from(g.bounds.top() + g.line_height * (line as f32 + 0.5)),
            )
        };
        let rgb = |c: u32| Rgb {
            r: (c >> 16) as u8,
            g: (c >> 8) as u8,
            b: c as u8,
        };
        let claude_ink = ensure_contrast(rgb(claude.rgb.expect("Claude's colour")), bg, MARK_MIN);
        let (teal, grey) = (ui.tab(TabColor::Teal), ui.tab(TabColor::Grey));
        // Inside the circle, left of its letter: the fill (filled) or the background (hollow).
        let inside = |line: usize| {
            let (x, y) = centre(line);
            pixel_at(&shot, dims, x - 4.0, y)
        };
        // Its ring (hollow): somewhere on the circle's left edge.
        let ringed = |line: usize, c: Rgb| {
            let (x, y) = centre(line);
            (0..6).any(|k| live_near(pixel_at(&shot, dims, x - 7.5 + k as f32 * 0.5, y), c, 48))
        };
        // Its letter: within a few points of the centre, something unlike the fill.
        let lettered = |line: usize, fill: Rgb| {
            let (x, y) = centre(line);
            (-6..=6).any(|dx| {
                (-6..=6).any(|dy| {
                    let p = pixel_at(&shot, dims, x + dx as f32 * 0.5, y + dy as f32 * 0.5);
                    !live_near(p, fill, 60)
                })
            })
        };
        for (line, what, fill) in [(1, "Claude, exact", claude_ink), (7, "yours", teal)] {
            let p = inside(line);
            assert!(
                live_near(p, fill, 12),
                "{n}: {what}: filled {p:?}, not {fill:?}"
            );
            assert!(lettered(line, fill), "{n}: {what}: its letter");
        }
        for (line, what, ring) in [(3, "Claude, guessed", claude_ink), (5, "codex", grey)] {
            let p = inside(line);
            assert!(live_near(p, bg, 6), "{n}: {what}: hollow {p:?}");
            assert!(ringed(line, ring), "{n}: {what}: its ring in {ring:?}");
            assert!(lettered(line, bg), "{n}: {what}: its letter");
        }
        for line in [0, 2, 4, 6] {
            let (x, y) = centre(line);
            let empty = (-12..=12)
                .all(|dx| live_near(pixel_at(&shot, dims, x + dx as f32 * 0.5, y), bg, 2));
            assert!(empty, "{n}: no mark on line {line}");
        }
        println!("ui-shots: {n} ok");
    }
}

/// `file-live-none` (both themes): a tab whose record holds no mark (given one, all lines
/// unmarked) has the same geometry and the same pixels as the plain file tab beside it (no
/// record given): no marks' column (the text where it was before M11af), no tint, no words.
fn file_live_none_scenes() {
    use crate::live_lines::LineRecord;
    use std::cell::RefCell;
    use std::rc::Rc;

    let path = live_file("none.rs", LIVE_FIXTURE);
    let dims = size(px(760.0), px(360.0));
    for &(slug, name) in shot_themes() {
        let n = format!("file-live-none-{slug}");
        let theme = shot_theme(name);
        let run = |given: bool| {
            let slot: LiveSlot = Rc::default();
            let build = live_tab(theme, path.clone(), slot.clone());
            let seen: Rc<RefCell<LiveSeen>> = Rc::default();
            let keep = seen.clone();
            let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
                let tab = slot.borrow().clone().expect("the file tab");
                live_frame(cx, window);
                if given {
                    cx.update(|cx| {
                        tab.update(cx, |t, _| {
                            t.set_live_for_shot(LineRecord::baseline(14), live_now(), None)
                        })
                    });
                    live_frame(cx, window);
                }
                *keep.borrow_mut() = cx.read(|cx| {
                    Some((
                        tab.read(cx).geom().expect("drawn"),
                        tab.read(cx).live_words(),
                    ))
                });
            };
            let shot = render_with(&theme, dims, build, act);
            let (g, words) = seen.borrow().clone().expect("read");
            (shot, g, words)
        };
        let (plain, gp, _) = run(false);
        let (none, gn, words) = run(true);
        keep_shot(&n, &none);
        assert!(words.is_empty(), "{n}: no words");
        assert_eq!(gn.marks_left, None, "{n}: no marks' column");
        assert_eq!(gp.marks_left, None, "{n}: none on a plain tab either");
        let cw = f32::from(gn.cell_width);
        assert_eq!(
            f32::from(gn.text_left),
            plain_text_left(cw),
            "{n}: the text as before"
        );
        assert_eq!(
            (gn.numbers_right, gn.bar_left, gn.text_left),
            (gp.numbers_right, gp.bar_left, gp.text_left),
            "{n}: the plain tab's geometry"
        );
        assert_eq!(
            (none.width, none.height),
            (plain.width, plain.height),
            "{n}"
        );
        let differ = none
            .rgba
            .chunks(4)
            .zip(plain.rgba.chunks(4))
            .filter(|(a, b)| a != b)
            .count();
        assert_eq!(differ, 0, "{n}: the plain tab's pixels");
        println!("ui-shots: {n} ok");
    }
}

/// `file-live-tooltip` (both themes): the pointer on a mark shows its tooltip, at once: on
/// yours (3 min ago) "you · 3 min ago"; off the marks, none; on Claude's guessed mark (12 s
/// ago) "probably claude · 12 s ago (it was working)", drawn under it (shot).
fn file_live_tooltip_scenes() {
    use crate::edit_ledger::agent_mark_for;
    use crate::live_lines::{Sure, Who};
    use gpui::Modifiers;
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    let path = live_file("tooltip.rs", LIVE_FIXTURE);
    let dims = size(px(760.0), px(360.0));
    for &(slug, name) in shot_themes() {
        let n = format!("file-live-tooltip-{slug}");
        let theme = shot_theme(name);
        let ui = theme.ui();
        let slot: LiveSlot = Rc::default();
        let build = live_tab(theme, path.clone(), slot.clone());
        let tips: Rc<RefCell<Vec<Option<String>>>> = Rc::default();
        let keep_tips = tips.clone();
        let seen: Rc<Cell<Option<crate::file_tab::Geom>>> = Rc::default();
        let keep = seen.clone();
        let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
            let tab = slot.borrow().clone().expect("the file tab");
            live_frame(cx, window);
            let record = live_record(
                14,
                &[
                    (3..4, Who::Agent(agent_mark_for("claude")), Sure::Guess, 12),
                    (6..7, Who::You, Sure::Exact, 180),
                ],
            );
            cx.update(|cx| tab.update(cx, |t, _| t.set_live_for_shot(record, live_now(), None)));
            live_frame(cx, window);
            let g = cx.read(|cx| tab.read(cx).geom()).expect("drawn");
            let mark = |line: usize| {
                point(
                    g.bounds.left() + g.marks_left.unwrap_or(g.numbers_right) + px(8.0),
                    g.bounds.top() + g.line_height * (line as f32 + 0.5),
                )
            };
            let text = point(g.bounds.left() + g.text_left + px(20.0), mark(6).y);
            for at in [mark(6), text, mark(3)] {
                cx.simulate_mouse_move(window, at, None, Modifiers::none());
                live_frame(cx, window);
                keep_tips
                    .borrow_mut()
                    .push(cx.read(|cx| tab.read(cx).mark_tip()));
            }
            keep.set(Some(g));
        };
        let shot = render_with(&theme, dims, build, act);
        keep_shot(&n, &shot);
        let tips = tips.borrow().clone();
        assert_eq!(
            tips,
            [
                Some("you \u{b7} 3 min ago".to_string()),
                None,
                Some("probably claude \u{b7} 12 s ago (it was working)".to_string()),
            ],
            "{n}: the tooltips"
        );
        // The tooltip under the mark: its words in the text colour across the gutter between
        // the marks and the text, under line 4 (nothing is drawn there without it).
        let g = seen.get().expect("drawn");
        let left = f32::from(g.bounds.left() + g.marks_left.expect("the marks' column"));
        let text_left = f32::from(g.bounds.left() + g.text_left);
        let top = f32::from(g.bounds.top() + g.line_height * 4.0);
        let inked = (0..(((text_left - left - 18.0) * 2.0) as i32))
            .flat_map(|dx| (0..(f32::from(g.line_height) as i32 * 4)).map(move |dy| (dx, dy)))
            .filter(|&(dx, dy)| {
                let p = pixel_at(
                    &shot,
                    dims,
                    left + 18.0 + dx as f32 * 0.5,
                    top + dy as f32 * 0.5,
                );
                live_near(p, ui.text, 40)
            })
            .count();
        println!("ui-shots: {n}: {inked} text pixels beside the mark");
        assert!(inked > 10, "{n}: the tooltip drawn under the mark");
        println!("ui-shots: {n} ok");
    }
}

/// `file-live-modes` (both themes): the glow and marks are File mode's only. A tab whose lines
/// glow, switched to Diff (given a diff): no marks' column, its unchanged rows untinted; a
/// Markdown tab whose lines glow, in Preview: the page untinted.
fn file_live_modes_scenes() {
    use crate::layout_model::FileMode;
    use crate::live_lines::{Sure, Who};
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    let view_rs = live_file("view.rs", FILE_VIEW_FIXTURE);
    let md = live_file(
        "notes.md",
        "# Notes\n\nA first paragraph.\n\nA second paragraph.\n\nA third paragraph.\n",
    );
    let unified = Rc::new(file_diff_fixture());
    let dims = size(px(760.0), px(360.0));
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let bg = theme.background;

        // Diff: the rows of lines 1–7 (from 0) glowed in File mode; none in Diff.
        let n = format!("file-live-modes-diff-{slug}");
        let slot: LiveSlot = Rc::default();
        let build = live_tab(theme, view_rs.clone(), slot.clone());
        let seen: Rc<Cell<Option<(crate::file_tab::Geom, bool)>>> = Rc::default();
        let keep = seen.clone();
        let diff = unified.clone();
        let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
            let tab = slot.borrow().clone().expect("the file tab");
            live_frame(cx, window);
            let lines = cx.read(|cx| tab.read(cx).doc().expect("its text").lines.len());
            let record = live_record(lines, &[(1..8, Who::You, Sure::Exact, 2)]);
            cx.update(|cx| tab.update(cx, |t, _| t.set_live_for_shot(record, live_now(), None)));
            live_frame(cx, window);
            let file_marks = cx.read(|cx| tab.read(cx).geom().expect("drawn").marks_left.is_some());
            cx.update(|cx| {
                tab.update(cx, |t, cx| {
                    t.choose_mode_for_shots(FileMode::Diff, cx);
                    t.set_diff_for_shots(&diff, cx);
                })
            });
            live_frame(cx, window);
            let t = cx.read(|cx| {
                let t = tab.read(cx);
                assert_eq!(t.mode(), FileMode::Diff);
                assert_eq!(t.diff_state_name(), "rows");
                t.geom().expect("drawn")
            });
            keep.set(Some((t, file_marks)));
        };
        let shot = render_with(&theme, dims, build, act);
        keep_shot(&n, &shot);
        let (g, file_marks) = seen.get().expect("drawn");
        assert!(file_marks, "{n}: File mode had the marks' column");
        assert_eq!(g.marks_left, None, "{n}: Diff has none");
        // Rows 1–5 are lines 1–5 unchanged (the first change, row 6, opens 3 rows down); right
        // of their text, clear of the scrollbar (the diff opened scrolled, so it shows).
        let right = f32::from(g.bounds.right()) - 40.0;
        for row in 3..=5 {
            let y = f32::from(g.bounds.top() + g.line_height * (row as f32 + 0.5) - g.scroll.y);
            let p = pixel_at(&shot, dims, right, y);
            assert!(live_near(p, bg, 1), "{n}: row {row} untinted: {p:?}");
        }
        println!("ui-shots: {n} ok");

        // Preview: the Markdown rendered, nothing tinted.
        let n = format!("file-live-modes-preview-{slug}");
        let slot: LiveSlot = Rc::default();
        let build = live_tab(theme, md.clone(), slot.clone());
        let shown: Rc<Cell<bool>> = Rc::default();
        let keep = shown.clone();
        let tipped: Rc<RefCell<Option<String>>> = Rc::default();
        let keep_tip = tipped.clone();
        let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
            let tab = slot.borrow().clone().expect("the file tab");
            live_frame(cx, window);
            let lines = cx.read(|cx| tab.read(cx).doc().expect("its text").lines.len());
            let record = live_record(lines, &[(0..lines, Who::You, Sure::Exact, 2)]);
            cx.update(|cx| tab.update(cx, |t, _| t.set_live_for_shot(record, live_now(), None)));
            cx.update_window(window, |_, window, cx| {
                tab.update(cx, |t, cx| t.choose_preview_for_shots(true, window, cx))
            })
            .expect("Preview");
            live_frame(cx, window);
            keep.set(cx.read(|cx| tab.read(cx).preview_shown()));
            *keep_tip.borrow_mut() = cx.read(|cx| tab.read(cx).mark_tip());
        };
        let shot = render_with(&theme, dims, build, act);
        keep_shot(&n, &shot);
        let (preview, tip) = (shown.get(), tipped.borrow().clone());
        assert!(preview, "{n}: the Preview shows");
        assert_eq!(tip, None, "{n}");
        // Down the page's right side (past the bar): the background, untinted.
        let x = f32::from(dims.width) - 30.0;
        for y in (60..340).step_by(4) {
            let p = pixel_at(&shot, dims, x, y as f32);
            assert!(live_near(p, bg, 1), "{n}: untinted at y {y}: {p:?}");
        }
        println!("ui-shots: {n} ok");
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

/// A file of `lines` numbered lines, for the Follow scenes.
fn follow_file(name: &str, lines: usize) -> PathBuf {
    let text: String = (0..lines).map(|i| format!("let v{i} = {i};\n")).collect();
    live_file(name, &text)
}

/// A frozen tab on `path` with an agent's (codex) change at `lines`, drawn, Follow as given.
fn follow_setup(
    cx: &mut VisualTestAppContext,
    window: AnyWindowHandle,
    tab: &gpui::Entity<crate::file_tab::FileTab>,
    total: usize,
    change: std::ops::Range<usize>,
    follow: bool,
) {
    use crate::live_lines::{Sure, Who};
    live_frame(cx, window);
    let record = live_record(
        total,
        &[(
            change,
            Who::Agent(crate::edit_ledger::agent_mark_for("codex")),
            Sure::Exact,
            2,
        )],
    );
    cx.update(|cx| {
        tab.update(cx, |t, _| {
            t.set_live_for_shot(record, live_now(), Some(follow))
        })
    });
    live_frame(cx, window);
}

/// `file-follow-pill` (both themes): once an agent (codex) has marked the file the bar has a
/// pill: "Follow codex" with no fill, then (a click) "Following codex" on the warm accent's
/// tint; a click turns it back. A file marked only by you has none.
fn file_follow_pill_scenes() {
    use crate::live_lines::{Sure, Who};
    use gpui::Modifiers;
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    let path = live_file("follow-pill.rs", LIVE_FIXTURE);
    let dims = size(px(760.0), px(360.0));
    for &(slug, name) in shot_themes() {
        let n = format!("file-follow-pill-{slug}");
        let theme = shot_theme(name);
        let panel = theme.ui().panel;
        let slot: LiveSlot = Rc::default();
        let build = live_tab(theme, path.clone(), slot.clone());
        type Seen = (Option<gpui::Bounds<gpui::Pixels>>, bool);
        let seen: Rc<RefCell<Vec<Seen>>> = Rc::default();
        let keep = seen.clone();
        let on_bounds: Rc<Cell<Option<gpui::Bounds<gpui::Pixels>>>> = Rc::default();
        let keep_on = on_bounds.clone();
        let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
            let tab = slot.borrow().clone().expect("the file tab");
            let look = |cx: &mut VisualTestAppContext| {
                live_frame(cx, window);
                cx.read(|cx| {
                    let t = tab.read(cx);
                    (t.follow_pill_bounds(), t.follow_on())
                })
            };
            live_frame(cx, window);
            // Marked by you only: no pill.
            let yours = live_record(14, &[(3..4, Who::You, Sure::Exact, 30)]);
            cx.update(|cx| tab.update(cx, |t, _| t.set_live_for_shot(yours, live_now(), None)));
            keep.borrow_mut().push(look(cx));
            follow_setup(cx, window, &tab, 14, 3..5, false);
            let off = look(cx);
            keep.borrow_mut().push(off);
            let at = off.0.expect("the pill").center();
            cx.simulate_click(window, at, Modifiers::none());
            let on = look(cx);
            keep.borrow_mut().push(on);
            keep_on.set(on.0);
            cx.simulate_click(window, on.0.expect("the pill").center(), Modifiers::none());
            keep.borrow_mut().push(look(cx));
            cx.simulate_click(window, at, Modifiers::none());
            look(cx);
        };
        let shot = render_with(&theme, dims, build, act);
        keep_shot(&n, &shot);
        let seen = seen.borrow().clone();
        assert_eq!(seen[0].0, None, "{n}: your marks alone: no pill");
        assert!(seen[1].0.is_some() && !seen[1].1, "{n}: off");
        assert!(seen[2].0.is_some() && seen[2].1, "{n}: a click turns it on");
        assert!(!seen[3].1, "{n}: a click turns it off");
        // The last state (on): the pill's left padding tinted, not the bar's own colour.
        let b = on_bounds.get().expect("the pill on");
        let (x, y) = (f32::from(b.left()) + 5.0, f32::from(b.center().y));
        let p = pixel_at(&shot, dims, x, y);
        assert!(
            !live_near(p, panel, 3),
            "{n}: the pill on is tinted: {p:?} on the bar {panel:?}"
        );
        // Its words are narrower than "Following codex" with its dot only when on.
        assert!(b.size.width > px(60.0), "{n}: the pill holds its words");
        println!("ui-shots: {n} ok");
    }
}

/// `file-follow-scrolls`: a change by an agent at line 400 of 600, Follow on: the view comes to
/// it, its first line `REVEAL_ABOVE` (3) rows from the top; in view already, the view stays;
/// your change, or Follow off, never moves it.
fn file_follow_scrolls_scenes() {
    use crate::live_lines::{Sure, Who};
    use std::cell::Cell;
    use std::rc::Rc;

    let path = follow_file("follow-scrolls.rs", 600);
    let dims = size(px(760.0), px(360.0));
    for &(slug, name) in shot_themes() {
        let n = format!("file-follow-scrolls-{slug}");
        let theme = shot_theme(name);
        let slot: LiveSlot = Rc::default();
        let build = live_tab(theme, path.clone(), slot.clone());
        let rows: Rc<Cell<[f32; 5]>> = Rc::default();
        let keep = rows.clone();
        let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
            let tab = slot.borrow().clone().expect("the file tab");
            follow_setup(cx, window, &tab, 600, 400..403, true);
            let row = |cx: &mut VisualTestAppContext| {
                live_frame(cx, window);
                cx.read(|cx| {
                    let t = tab.read(cx);
                    let g = t.geom().expect("drawn");
                    t.scroll().y / g.line_height
                })
            };
            let start = row(cx);
            cx.update(|cx| tab.update(cx, |t, cx| t.follow_for_shots(cx)));
            let followed = row(cx);
            let still_on = cx.read(|cx| tab.read(cx).follow_on());
            // In view now: no scroll.
            cx.update(|cx| tab.update(cx, |t, cx| t.follow_for_shots(cx)));
            let again = row(cx);
            // Your change far away (line 100): nothing moves.
            let yours = live_record(600, &[(100..101, Who::You, Sure::Exact, 2)]);
            cx.update(|cx| tab.update(cx, |t, _| t.set_live_for_shot(yours, live_now(), None)));
            cx.update(|cx| tab.update(cx, |t, cx| t.follow_for_shots(cx)));
            let yours_row = row(cx);
            keep.set([start, followed, again, yours_row, f32::from(still_on as u8)]);
        };
        let shot = render_with(&theme, dims, build, act);
        keep_shot(&n, &shot);
        let [start, followed, again, yours_row, on] = rows.get();
        assert_eq!(start, 0.0, "{n}: opens at the top");
        assert_eq!(followed, 397.0, "{n}: line 400 is 3 rows from the top");
        assert_eq!(on, 1.0, "{n}: Follow stays on");
        assert_eq!(again, 397.0, "{n}: in view: it stays");
        assert_eq!(yours_row, 397.0, "{n}: your change never moves the view");
        println!("ui-shots: {n} ok");
    }
}

/// `file-follow-wheel-stops`: Follow on; the wheel turns it off (the pill says Follow again); a
/// change by the agent then leaves the view; selecting text alone keeps it on.
fn file_follow_wheel_stops_scenes() {
    use gpui::{Modifiers, MouseButton, ScrollDelta, ScrollWheelEvent, TouchPhase};
    use std::cell::Cell;
    use std::rc::Rc;

    let path = follow_file("follow-wheel.rs", 600);
    let dims = size(px(760.0), px(360.0));
    for &(slug, name) in shot_themes() {
        let n = format!("file-follow-wheel-stops-{slug}");
        let theme = shot_theme(name);
        let slot: LiveSlot = Rc::default();
        let build = live_tab(theme, path.clone(), slot.clone());
        let seen: Rc<Cell<[f32; 4]>> = Rc::default();
        let keep = seen.clone();
        let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
            let tab = slot.borrow().clone().expect("the file tab");
            follow_setup(cx, window, &tab, 600, 400..403, true);
            let g = cx.read(|cx| tab.read(cx).geom()).expect("drawn");
            // Selecting text alone: still on.
            let from = point(
                g.bounds.left() + g.text_left + px(10.0),
                g.bounds.top() + px(20.0),
            );
            let to = point(from.x + px(60.0), from.y);
            cx.simulate_mouse_down(window, from, MouseButton::Left, Modifiers::none());
            cx.simulate_mouse_move(window, to, Some(MouseButton::Left), Modifiers::none());
            cx.simulate_mouse_up(window, to, MouseButton::Left, Modifiers::none());
            live_frame(cx, window);
            let selected = cx.read(|cx| {
                let t = tab.read(cx);
                (t.selection().is_some(), t.follow_on())
            });
            // The wheel: off.
            cx.simulate_event(
                window,
                ScrollWheelEvent {
                    position: g.bounds.center(),
                    delta: ScrollDelta::Pixels(point(px(0.0), px(-60.0))),
                    modifiers: Modifiers::none(),
                    touch_phase: TouchPhase::Moved,
                },
            );
            live_frame(cx, window);
            let (on, pill) = cx.read(|cx| {
                let t = tab.read(cx);
                (t.follow_on(), t.follow_pill_bounds().is_some())
            });
            let before = cx.read(|cx| tab.read(cx).scroll().y);
            cx.update(|cx| tab.update(cx, |t, cx| t.follow_for_shots(cx)));
            live_frame(cx, window);
            let after = cx.read(|cx| tab.read(cx).scroll().y);
            keep.set([
                f32::from(selected.0 as u8 * 2 + selected.1 as u8),
                f32::from(on as u8 * 2 + pill as u8),
                f32::from(before),
                f32::from(after),
            ]);
        };
        let shot = render_with(&theme, dims, build, act);
        keep_shot(&n, &shot);
        let [selected, wheel, before, after] = seen.get();
        assert_eq!(selected, 3.0, "{n}: selecting keeps Follow on");
        assert_eq!(wheel, 1.0, "{n}: the wheel turns it off, the pill stays");
        assert_eq!(
            before, after,
            "{n}: off, the agent's change leaves the view"
        );
        println!("ui-shots: {n} ok");
    }
}

/// `file-follow-mode` (mode_switch_keeps_marks_turns_follow_off): Follow on and a mark's tooltip
/// showing; Diff, then File again: the marks are kept, Follow is off, and the tooltip is gone.
/// The same for Preview (a Markdown file), where an agent's change never moves the File view.
fn file_follow_mode_scenes() {
    use crate::layout_model::FileMode;
    use gpui::Modifiers;
    use std::cell::Cell;
    use std::rc::Rc;

    let path = follow_file("follow-mode.rs", 40);
    let dims = size(px(760.0), px(360.0));
    for &(slug, name) in shot_themes() {
        let n = format!("file-follow-mode-{slug}");
        let theme = shot_theme(name);
        let slot: LiveSlot = Rc::default();
        let build = live_tab(theme, path.clone(), slot.clone());
        let seen: Rc<Cell<[u8; 4]>> = Rc::default();
        let keep = seen.clone();
        let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
            let tab = slot.borrow().clone().expect("the file tab");
            follow_setup(cx, window, &tab, 40, 3..5, true);
            let g = cx.read(|cx| tab.read(cx).geom()).expect("drawn");
            let on_mark = point(
                g.bounds.left() + g.marks_left.unwrap_or(g.numbers_right) + px(8.0),
                g.bounds.top() + g.line_height * 3.5,
            );
            cx.simulate_mouse_move(window, on_mark, None, Modifiers::none());
            live_frame(cx, window);
            let tipped = cx.read(|cx| tab.read(cx).mark_tip().is_some());
            cx.update(|cx| tab.update(cx, |t, cx| t.choose_mode_for_shots(FileMode::Diff, cx)));
            live_frame(cx, window);
            cx.update(|cx| tab.update(cx, |t, cx| t.choose_mode_for_shots(FileMode::Plain, cx)));
            live_frame(cx, window);
            let (follow, tip, marked) = cx.read(|cx| {
                let t = tab.read(cx);
                (t.follow_on(), t.mark_tip().is_some(), t.live_marks().any())
            });
            keep.set([tipped as u8, follow as u8, tip as u8, marked as u8]);
        };
        let shot = render_with(&theme, dims, build, act);
        keep_shot(&n, &shot);
        assert_eq!(seen.get(), [1, 0, 0, 1], "{n}: tip, follow, tip, marks");
        println!("ui-shots: {n} ok");
    }
    // Preview (a Markdown file): File → Preview → File keeps the marks and ends Follow and the
    // tooltip, as Diff does; and an agent's change while Preview shows never moves the hidden
    // File view (Follow on there or not).
    let md = live_file(
        "follow-mode.md",
        &(0..600)
            .map(|i| format!("line {i}\n\n"))
            .collect::<String>(),
    );
    for &(slug, name) in shot_themes() {
        let n = format!("file-follow-mode-preview-{slug}");
        let theme = shot_theme(name);
        let slot: LiveSlot = Rc::default();
        let build = live_tab(theme, md.clone(), slot.clone());
        let seen: Rc<Cell<[u8; 7]>> = Rc::default();
        let keep = seen.clone();
        let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
            let tab = slot.borrow().clone().expect("the file tab");
            let lines = 1200;
            follow_setup(cx, window, &tab, lines, 3..5, true);
            let g = cx.read(|cx| tab.read(cx).geom()).expect("drawn");
            let on_mark = point(
                g.bounds.left() + g.marks_left.unwrap_or(g.numbers_right) + px(8.0),
                g.bounds.top() + g.line_height * 3.5,
            );
            cx.simulate_mouse_move(window, on_mark, None, Modifiers::none());
            live_frame(cx, window);
            let tipped = cx.read(|cx| tab.read(cx).mark_tip().is_some());
            let preview = |cx: &mut VisualTestAppContext, on: bool| {
                cx.update_window(window, |_, window, cx| {
                    tab.update(cx, |t, cx| t.choose_preview_for_shots(on, window, cx))
                })
                .expect("the view switch");
                live_frame(cx, window);
            };
            preview(cx, true);
            let (shown, in_preview) = cx.read(|cx| {
                let t = tab.read(cx);
                (t.preview_shown(), t.follow_on())
            });
            preview(cx, false);
            let (follow, tip, marked) = cx.read(|cx| {
                let t = tab.read(cx);
                (t.follow_on(), t.mark_tip().is_some(), t.live_marks().any())
            });
            // Preview again, Follow on anyway, and the agent changes line 800: the File view
            // stays where it was.
            preview(cx, true);
            let before = cx.read(|cx| tab.read(cx).scroll().y);
            let record = live_record(
                lines,
                &[(
                    800..803,
                    crate::live_lines::Who::Agent(crate::edit_ledger::agent_mark_for("codex")),
                    crate::live_lines::Sure::Exact,
                    1,
                )],
            );
            cx.update(|cx| {
                tab.update(cx, |t, cx| {
                    t.set_live_for_shot(record, live_now(), Some(true));
                    t.follow_for_shots(cx);
                })
            });
            live_frame(cx, window);
            let stayed = cx.read(|cx| tab.read(cx).scroll().y) == before;
            keep.set([
                tipped as u8,
                shown as u8,
                in_preview as u8,
                follow as u8,
                tip as u8,
                marked as u8,
                stayed as u8,
            ]);
        };
        let shot = render_with(&theme, dims, build, act);
        keep_shot(&n, &shot);
        assert_eq!(
            seen.get(),
            [1, 1, 0, 0, 0, 1, 1],
            "{n}: tip, Preview shown, Follow in Preview, Follow back in File, tip, marks, \
             the File view stays"
        );
        println!("ui-shots: {n} ok");
    }
}

/// `file-code-calm` (both themes; M11ah Task 1): a 14-line Rust file with line 5 pressed. Rows
/// are the font's line height x 1.2 and the text sits at the new columns; the first row starts
/// `code::TOP_ROOM` under the body's top (under the breadcrumb, M11ah Task 6); line 5's row has the current band (`text` at 5 %
/// over the background, to the right edge) and rows 4 and 6 have none; line 5's number is
/// drawn in `text`, line 4's in the quiet ink (`text_dim`, held to 3:1).
fn file_code_calm_scenes() {
    use crate::ui_tokens::{MARK_MIN, code, contrast, ensure_contrast};
    use gpui::{Modifiers, MouseButton};
    use std::cell::RefCell;
    use std::rc::Rc;

    let path = live_file("calm.rs", LIVE_FIXTURE);
    let dims = size(px(760.0), px(360.0));
    for &(slug, name) in shot_themes() {
        let n = format!("file-code-calm-{slug}");
        let theme = shot_theme(name);
        let ui = theme.ui();
        let slot: LiveSlot = Rc::default();
        let build = live_tab(theme, path.clone(), slot.clone());
        let seen: Rc<RefCell<Option<(crate::file_tab::Geom, gpui::Pixels)>>> = Rc::default();
        let keep = seen.clone();
        let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
            let tab = slot.borrow().clone().expect("the file tab");
            live_frame(cx, window);
            let g = cx.read(|cx| tab.read(cx).geom()).expect("drawn");
            // Line 5 (0-based 4): a press without a drag.
            let at = point(
                g.bounds.left() + g.text_left + px(4.0),
                g.bounds.top() + g.line_height * 4.5,
            );
            let none = Modifiers::none();
            cx.simulate_mouse_down(window, at, MouseButton::Left, none);
            cx.simulate_mouse_up(window, at, MouseButton::Left, none);
            live_frame(cx, window);
            let line = cx
                .update_window(window, |_, window, cx| {
                    let m = crate::terminal_view::CellMetrics::measure(
                        cx.global::<crate::appearance::Appearance>(),
                        window,
                    );
                    m.line_height
                })
                .expect("metrics");
            let g = cx.read(|cx| tab.read(cx).geom()).expect("drawn");
            *keep.borrow_mut() = Some((g, line));
        };
        let shot = render_with(&theme, dims, build, act);
        keep_shot(&n, &shot);
        let (g, line) = seen.borrow().expect("read");
        assert_eq!(
            g.line_height,
            crate::file_tab::row_height(line),
            "{n}: a row is the line height x 1.2"
        );
        assert!(
            (f32::from(g.line_height) - f32::from(line) * 1.2).abs() < 0.01,
            "{n}: a row is {:?}, the line height {line:?} x 1.2",
            g.line_height
        );
        assert_eq!(
            f32::from(g.text_left),
            plain_text_left(f32::from(g.cell_width)),
            "{n}: the text at the new columns"
        );
        // The file bar is 42 pt tall; the file has blocks, so the breadcrumb's strip (M11ah Task
        // 6) is under it; the first row starts `TOP_ROOM` under that.
        assert_eq!(
            f32::from(g.bounds.top()),
            42.0 + code::BREADCRUMB + code::TOP_ROOM,
            "{n}: the first row's top"
        );
        let bg = theme.background;
        let band = crate::theme::mix(bg, ui.text, code::CURRENT_ALPHA);
        let right = f32::from(g.bounds.right()) - 3.0;
        let top = f32::from(g.bounds.top());
        let row = f32::from(g.line_height);
        let at = |r: usize| pixel_at(&shot, dims, right, top + row * (r as f32 + 0.5));
        assert!(live_near(at(4), band, 2), "{n}: line 5's band {:?}", at(4));
        assert!(live_near(at(3), bg, 1), "{n}: line 4 has none {:?}", at(3));
        assert!(live_near(at(5), bg, 1), "{n}: line 6 has none {:?}", at(5));
        // The band covers the whole row and no more.
        let edge = |y: f32| pixel_at(&shot, dims, right, y);
        assert!(
            live_near(edge(top + row * 4.0 + 1.0), band, 2),
            "{n}: its top"
        );
        assert!(
            live_near(edge(top + row * 5.0 - 1.0), band, 2),
            "{n}: its bottom"
        );
        assert!(
            live_near(edge(top + row * 4.0 - 1.0), bg, 1),
            "{n}: above it"
        );
        assert!(
            live_near(edge(top + row * 5.0 + 1.0), bg, 1),
            "{n}: below it"
        );
        // The numbers: the most inked pixel of each, against its row's background.
        let inkiest = |r: usize, back: [u8; 3]| {
            let (x1, y0) = (
                f32::from(g.bounds.left() + g.numbers_right),
                top + row * r as f32,
            );
            let cell = f32::from(g.cell_width);
            let mut best = (0, back);
            for dx in 0..(cell * 2.0 * 2.0) as i32 {
                for dy in 0..(row * 2.0) as i32 {
                    let p = pixel_at(
                        &shot,
                        dims,
                        x1 - cell * 2.0 + dx as f32 * 0.5,
                        y0 + dy as f32 * 0.5,
                    );
                    let d: i32 = (0..3).map(|k| (p[k] as i32 - back[k] as i32).abs()).sum();
                    if d > best.0 {
                        best = (d, p);
                    }
                }
            }
            best.1
        };
        let dist = |p: [u8; 3], c: alacritty_terminal::vte::ansi::Rgb| -> i32 {
            [(p[0], c.r), (p[1], c.g), (p[2], c.b)]
                .iter()
                .map(|&(a, b)| (a as i32 - b as i32).abs())
                .sum()
        };
        let quiet = ensure_contrast(ui.text_dim, bg, MARK_MIN);
        assert!(contrast(quiet, bg) >= MARK_MIN, "{n}: the quiet ink reads");
        let (five, four) = (inkiest(4, at(4)), inkiest(3, at(3)));
        assert!(
            dist(five, ui.text) <= 30,
            "{n}: line 5's number in text {five:?}"
        );
        assert!(
            dist(four, quiet) <= 30 && dist(four, quiet) < dist(four, ui.text),
            "{n}: line 4's number quiet {four:?}"
        );
        println!("ui-shots: {n} ok");
    }
}

/// A frozen tab on `path` shot in `theme` after `setup` (given the tab): the shot and the
/// geometry last drawn.
fn code_shot(
    theme: &Theme,
    path: &std::path::Path,
    dims: gpui::Size<gpui::Pixels>,
    setup: impl FnOnce(
        &mut VisualTestAppContext,
        AnyWindowHandle,
        &gpui::Entity<crate::file_tab::FileTab>,
    ),
) -> (Shot, crate::file_tab::Geom) {
    use std::cell::Cell;
    use std::rc::Rc;
    let slot: LiveSlot = Rc::default();
    let build = live_tab(*theme, path.to_path_buf(), slot.clone());
    let seen: Rc<Cell<Option<crate::file_tab::Geom>>> = Rc::default();
    let keep = seen.clone();
    let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
        let tab = slot.borrow().clone().expect("the file tab");
        live_frame(cx, window);
        setup(cx, window, &tab);
        live_frame(cx, window);
        keep.set(cx.read(|cx| tab.read(cx).geom()));
    };
    let shot = render_with(theme, dims, build, act);
    (shot, seen.get().expect("drawn"))
}

/// The fixture of the guide scenes: a block inside a block, a blank line inside the inner one.
const GUIDES_FIXTURE: &str = "fn main() {
    if ready {
        let a = 1;

        let b = 2;
    }
    let c = 3;
}
";

/// `file-code-guides` (both themes; M11ah Task 3): in a nested Rust file with line 3 (0-based
/// 2) pressed, a level-2 line has a 1 pt guide at cell 0 and at cell 4 (the step) in `text` at
/// 7 %, the pressed line's block (the `if`) drawing its own at 14 %; the blank line inside the
/// block has both; a line outside the `if` has none at cell 4. A wrapped line's continuation
/// rows (Wrap on) have them too; a flat text file none; Diff mode draws them over its rows.
fn file_code_guides_scenes() {
    use crate::ui_tokens::code;
    use gpui::{Modifiers, MouseButton};

    let nested = live_file("guides.rs", GUIDES_FIXTURE);
    let flat = live_file("guides-flat.txt", "one\ntwo\n\nthree\nfour\n");
    let long = live_file(
        "guides-wrap.rs",
        &format!(
            "fn main() {{\n    if ready {{\n        let a = 1; // {}end\n        let b = 2;\n    }}\n}}\n",
            " ".repeat(400)
        ),
    );
    let dims = size(px(760.0), px(360.0));
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let ui = theme.ui();
        let bg = theme.background;
        let faint = crate::theme::mix(bg, ui.text, code::GUIDE_ALPHA);
        let strong = crate::theme::mix(bg, ui.text, code::GUIDE_ACTIVE_ALPHA);
        // The guide's pixel at `cell`, `dy` points down row `row` (scroll counted).
        let guide = |shot: &Shot, g: &crate::file_tab::Geom, cell: usize, row: usize, dy: f32| {
            let x = f32::from(g.bounds.left() + g.text_left + g.cell_width * cell as f32) + 0.5;
            let y = f32::from(g.bounds.top() + g.line_height * row as f32 - g.scroll.y) + dy;
            pixel_at(shot, dims, x, y)
        };

        // File mode: line 3 pressed.
        let n = format!("file-code-guides-{slug}");
        let (shot, g) = code_shot(&theme, &nested, dims, |cx, window, tab| {
            let g = cx.read(|cx| tab.read(cx).geom()).expect("drawn");
            let at = point(
                g.bounds.left() + g.text_left + g.cell_width * 12.0,
                g.bounds.top() + g.line_height * 2.5,
            );
            let none = Modifiers::none();
            cx.simulate_mouse_down(window, at, MouseButton::Left, none);
            cx.simulate_mouse_up(window, at, MouseButton::Left, none);
        });
        keep_shot(&n, &shot);
        let lh = f32::from(g.line_height);
        for dy in [lh * 0.3, lh * 0.5, lh * 0.8] {
            // Row 4 (`let b`) and the blank row 3: cell 0 quiet, cell 4 the pressed block's.
            for row in [3, 4] {
                let (a, b) = (guide(&shot, &g, 0, row, dy), guide(&shot, &g, 4, row, dy));
                assert!(
                    live_near(a, faint, 3),
                    "{n}: row {row} cell 0 {a:?}, not {faint:?}"
                );
                assert!(
                    live_near(b, strong, 3),
                    "{n}: row {row} cell 4 {b:?}, not {strong:?}"
                );
            }
            // Row 5 (`    }`, the closer, level 1): cell 0 quiet.
            let a = guide(&shot, &g, 0, 5, dy);
            assert!(
                live_near(a, faint, 3),
                "{n}: row 5 cell 0 {a:?}, not {faint:?}"
            );
        }
        // Row 6 (`    let c`): cell 0 quiet; its cell 4 holds the text, no guide drawn there.
        let a = guide(&shot, &g, 0, 6, lh * 0.5);
        assert!(live_near(a, faint, 3), "{n}: row 6 cell 0 {a:?}");
        println!("ui-shots: {n} ok");

        // Wrap on: the long line's continuation rows (just spaces) have the guides.
        let n = format!("file-code-guides-wrap-{slug}");
        let (shot, g) = code_shot(&theme, &long, dims, |cx, _, _| {
            cx.update(|cx| cx.global_mut::<crate::appearance::Appearance>().file_wrap = true);
        });
        keep_shot(&n, &shot);
        let lh = f32::from(g.line_height);
        for row in [3, 4] {
            for cell in [0, 4] {
                let p = guide(&shot, &g, cell, row, lh * 0.5);
                assert!(
                    live_near(p, faint, 3),
                    "{n}: continuation row {row} cell {cell} {p:?}, not {faint:?}"
                );
            }
        }
        println!("ui-shots: {n} ok");

        // A flat text file: none, even through its blank line.
        let n = format!("file-code-guides-flat-{slug}");
        let (shot, g) = code_shot(&theme, &flat, dims, |_, _, _| {});
        keep_shot(&n, &shot);
        for cell in [0, 4] {
            let p = guide(&shot, &g, cell, 2, f32::from(g.line_height) * 0.5);
            assert!(live_near(p, bg, 1), "{n}: no guide at cell {cell} {p:?}");
        }
        println!("ui-shots: {n} ok");

        // Diff mode: the rows' own outline; the blank context row (3) has both.
        let n = format!("file-code-guides-diff-{slug}");
        let old: Vec<&str> = GUIDES_FIXTURE.lines().collect();
        let mut unified = format!(
            "diff --git a/guides.rs b/guides.rs\n--- a/guides.rs\n+++ b/guides.rs\n@@ -1,{} +1,{} @@\n",
            old.len() - 1,
            old.len()
        );
        for (i, line) in old.iter().enumerate() {
            let sign = if i == 2 { '+' } else { ' ' };
            unified.push_str(&format!("{sign}{line}\n"));
        }
        let (shot, g) = code_shot(&theme, &nested, dims, |cx, _, tab| {
            cx.update(|cx| {
                tab.update(cx, |t, cx| {
                    t.choose_mode_for_shots(crate::layout_model::FileMode::Diff, cx);
                    t.set_diff_for_shots(&unified, cx);
                })
            });
        });
        keep_shot(&n, &shot);
        // M11ah: the rows are in one card, under its title row.
        let title = crate::ui_tokens::code::CARD_TITLE;
        for cell in [0, 4] {
            let p = guide(&shot, &g, cell, 3, f32::from(g.line_height) * 0.5 + title);
            assert!(live_near(p, faint, 3), "{n}: diff row 3 cell {cell} {p:?}");
        }
        println!("ui-shots: {n} ok");
    }
}

/// `file-code-bars` (both themes; M11ah Task 3): three Added rows are one 3 pt bar with rounded
/// ends (its corner pixels show the row's tint, not the green, at 2x; at 1x as much of the green
/// as the curve covers, `corner_cover`; its middle the green); an Added
/// run touching a Changed run leaves 2 pt between them (1 pt of each row's tint); the deleted
/// lines' ▸ is still drawn; a press on any row of a run opens Diff.
fn file_code_bars_scenes() {
    use crate::git_status::LineChange;
    use crate::layout_model::FileMode;
    use crate::ui_tokens::code::BAR_WIDTH;
    use crate::ui_tokens::{MARK_MIN, ensure_contrast};
    use gpui::{Modifiers, MouseButton};

    let path = live_file("bars.rs", LIVE_FIXTURE);
    let dims = size(px(760.0), px(360.0));
    // Lines counted from 1, as git does: 3-5 added, 6 changed, lines deleted above line 12.
    let changes = || {
        vec![
            (3, LineChange::Added),
            (4, LineChange::Added),
            (5, LineChange::Added),
            (6, LineChange::Changed),
            (11, LineChange::DeletedAfter),
        ]
    };
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let bg = theme.background;
        let fit = |c: alacritty_terminal::vte::ansi::Rgb| ensure_contrast(c, bg, MARK_MIN);
        let (green, blue, red) = (
            fit(theme.palette[2]),
            fit(theme.palette[4]),
            fit(theme.palette[1]),
        );
        let (tint_green, tint_blue) = (
            crate::theme::mix(bg, green, 0.12),
            crate::theme::mix(bg, blue, 0.12),
        );
        let n = format!("file-code-bars-{slug}");
        let (shot, g) = code_shot(&theme, &path, dims, |cx, _, tab| {
            cx.update(|cx| tab.update(cx, |t, cx| t.set_changes(changes(), cx)));
        });
        keep_shot(&n, &shot);
        let (lh, top) = (f32::from(g.line_height), f32::from(g.bounds.top()));
        // The bar's edges as drawn: on device pixels (`device_snap`; at 1x a half point moves).
        let scale = scale_of(&shot, dims);
        let snap = |v: f32| device_snap(v, scale);
        let left = snap(f32::from(g.bounds.left() + g.bar_left));
        let at = |x: f32, y: f32| pixel_at(&shot, dims, x, y);
        let (y0, y1) = (snap(top + lh * 2.0), snap(top + lh * 5.0));
        // A corner pixel: the row's tint, with as much of the green as the round end covers.
        let corner = crate::theme::mix(tint_green, green, corner_cover(BAR_WIDTH / 2.0, scale));
        // The run's middle is the green, at its full width.
        for dx in [0.4, 1.5, 2.6] {
            let p = at(left + dx, y0 + lh * 1.5);
            assert!(
                live_near(p, green, 12),
                "{n}: middle at {dx}: {p:?}, not {green:?}"
            );
        }
        // Its top corners are cut round: the row's tint shows there, not the green.
        for x in [left + 0.25, left + 2.75] {
            let p = at(x, y0 + 0.25);
            assert!(
                live_near(p, corner, 10),
                "{n}: top corner {p:?}, not {corner:?}"
            );
        }
        let p = at(left + 1.5, y0 + 2.0);
        assert!(live_near(p, green, 12), "{n}: below the corner {p:?}");
        // The bottom end (touching the Changed run) is shortened by 1 pt and round.
        for x in [left + 0.25, left + 2.75] {
            let p = at(x, y1 - 1.25);
            assert!(
                live_near(p, corner, 10),
                "{n}: bottom corner {p:?}, not {corner:?}"
            );
        }
        let p = at(left + 1.5, y1 - 3.0);
        assert!(live_near(p, green, 12), "{n}: above the gap {p:?}");
        // The 2 pt gap: 1 pt under the Added run, 1 pt above the Changed one, each its row's tint.
        let (up, down) = (at(left + 1.5, y1 - 0.5), at(left + 1.5, y1 + 0.5));
        assert!(
            live_near(up, tint_green, 10),
            "{n}: gap above {up:?}, not {tint_green:?}"
        );
        assert!(
            live_near(down, tint_blue, 10),
            "{n}: gap below {down:?}, not {tint_blue:?}"
        );
        let p = at(left + 1.5, y1 + 3.0);
        assert!(
            live_near(p, blue, 12),
            "{n}: the Changed run {p:?}, not {blue:?}"
        );
        // The deleted-lines ▸ over the bar's place, on the edge above line 12 (0-based 11).
        let edge = top + lh * 11.0;
        let drawn = (-8..=8).any(|dx| {
            (-10..=10).any(|dy| {
                let p = at(left + 1.5 + dx as f32 * 0.5, edge + dy as f32 * lh / 20.0);
                live_near(p, red, 40)
            })
        });
        assert!(drawn, "{n}: the ▸ is still drawn");
        println!("ui-shots: {n} ok");

        // Wrap on: two wrapped Added lines are one bar over every row of both, continuation
        // rows and the seam between the lines included.
        let n = format!("file-code-bars-wrap-{slug}");
        let wide = live_file(
            "bars-wrap.rs",
            &format!("a\n// {0}end\n// {0}end\nb\n", " ".repeat(400)),
        );
        let rows = std::rc::Rc::new(std::cell::Cell::new(0));
        let seen = rows.clone();
        let (shot, g) = code_shot(&theme, &wide, dims, |cx, window, tab| {
            cx.update(|cx| cx.global_mut::<crate::appearance::Appearance>().file_wrap = true);
            live_frame(cx, window);
            cx.update(|cx| {
                tab.update(cx, |t, cx| {
                    t.set_changes(vec![(2, LineChange::Added), (3, LineChange::Added)], cx)
                })
            });
            live_frame(cx, window);
            seen.set(cx.read(|cx| tab.read(cx).wrapped_rows()).expect("wrapped"));
        });
        keep_shot(&n, &shot);
        let each = (rows.get() - 2) / 2;
        assert!(each >= 3, "{n}: each long line takes {each} rows");
        let (lh, top) = (f32::from(g.line_height), f32::from(g.bounds.top()));
        let x = f32::from(g.bounds.left() + g.bar_left) + 1.5;
        let seam = top + lh * (1 + each) as f32;
        for (what, y) in [
            ("a continuation row", top + lh * 2.5),
            ("above the seam", seam - 1.5),
            ("the seam", seam - 0.25),
            ("below the seam", seam + 1.5),
            ("the last row", top + lh * (1 + 2 * each) as f32 - lh * 0.5),
        ] {
            let p = pixel_at(&shot, dims, x, y);
            assert!(live_near(p, green, 12), "{n}: {what} {p:?}, not {green:?}");
        }
        let p = pixel_at(&shot, dims, x, top + lh * 0.5);
        assert!(live_near(p, bg, 6), "{n}: none on the unchanged line {p:?}");
        println!("ui-shots: {n} ok");

        // A press on any row of the run opens Diff.
        let n = format!("file-code-bars-press-{slug}");
        let (_, _) = code_shot(&theme, &path, dims, |cx, window, tab| {
            cx.update(|cx| {
                tab.update(cx, |t, cx| {
                    t.pretend_in_repo_for_shots(cx);
                    t.set_changes(changes(), cx);
                })
            });
            live_frame(cx, window);
            for row in 2..6 {
                let g = cx.read(|cx| tab.read(cx).geom()).expect("drawn");
                let at = point(
                    g.bounds.left() + g.bar_left + px(1.5),
                    g.bounds.top() + g.line_height * (row as f32 + 0.5) - g.scroll.y,
                );
                let none = Modifiers::none();
                cx.simulate_mouse_down(window, at, MouseButton::Left, none);
                cx.simulate_mouse_up(window, at, MouseButton::Left, none);
                live_frame(cx, window);
                let mode = cx.read(|cx| tab.read(cx).mode());
                assert_eq!(mode, FileMode::Diff, "{n}: a press on row {row} opens Diff");
                cx.update(|cx| {
                    tab.update(cx, |t, cx| t.choose_mode_for_shots(FileMode::Plain, cx))
                });
                live_frame(cx, window);
            }
        });
        println!("ui-shots: {n} ok");
    }
}

/// A pixel as a colour.
fn as_rgb([r, g, b]: [u8; 3]) -> alacritty_terminal::vte::ansi::Rgb {
    alacritty_terminal::vte::ansi::Rgb { r, g, b }
}

/// The fold scenes' file: `fn refresh` (0-based line 11) holds 23 lines and its `}` (12..36), so
/// folded it hides 24; blocks also start on lines 3, 7 and 37.
fn fold_fixture() -> String {
    let mut text = String::from(
        "// A file with a long function, for the fold scenes.\nuse std::fmt;\n\nstruct Session {\n    id: u32,\n}\n\nfn small() {\n    let a = 1;\n}\n\nfn refresh(s: &mut Session) {\n",
    );
    for k in 12..35 {
        text.push_str(&format!("    let v{k} = {k};\n"));
    }
    text.push_str("}\n\nfn after() {\n    let z = 0;\n}\n");
    text
}

/// `file-code-fold` (both themes; M11ah Task 5): `fn refresh` (line 12) folded, with a git change
/// (an added line 20) and an agent's warm lines (16, 40 s ago; 26, just now) inside: the fold
/// column, 14 pt, sits between the numbers and the marks' column; the folded line has its ▸
/// always, the open blocks' ▾ show only while the pointer is over the gutter; the pill comes
/// after the folded line's text; the numbers go 12 then 37; the folded line draws the hidden
/// change's bar, the warmest glow (just now's) and a mark.
fn file_code_fold_scenes() {
    use crate::file_tab::{GLOW, WARM_RGB};
    use crate::git_status::LineChange;
    use crate::live_lines::{Sure, Who};
    use crate::theme::mix;
    use crate::ui_tokens::{MARK_MIN, code, ensure_contrast};

    let path = live_file("fold.rs", &fold_fixture());
    let dims = size(px(760.0), px(400.0));
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let bg = theme.background;
        let green = ensure_contrast(theme.palette[2], bg, MARK_MIN);
        let rows = std::rc::Rc::new(std::cell::Cell::new([None; 3]));
        let seen = rows.clone();
        let shoot = |hover: bool| {
            let seen = seen.clone();
            code_shot(&theme, &path, dims, move |cx, window, tab| {
                let record = live_record(
                    40,
                    &[
                        (
                            15..16,
                            Who::Agent(crate::edit_ledger::agent_mark_for("codex")),
                            Sure::Exact,
                            40,
                        ),
                        (
                            25..26,
                            Who::Agent(crate::edit_ledger::agent_mark_for("codex")),
                            Sure::Exact,
                            2,
                        ),
                    ],
                );
                cx.update(|cx| {
                    tab.update(cx, |t, cx| {
                        t.set_live_for_shot(record, live_now(), None);
                        t.set_changes(vec![(20, LineChange::Added)], cx);
                        t.fold_for_shots(&[11], cx);
                    })
                });
                live_frame(cx, window);
                let g = cx.read(|cx| tab.read(cx).geom()).expect("drawn");
                // The pointer over the gutter (the numbers), or over the text.
                let x = if hover {
                    g.bounds.left() + g.numbers_right - px(4.0)
                } else {
                    g.bounds.left() + g.text_left + px(200.0)
                };
                let at = point(x, g.bounds.top() + g.line_height * 1.5);
                cx.simulate_mouse_move(window, at, None, gpui::Modifiers::none());
                live_frame(cx, window);
                seen.set(cx.read(|cx| {
                    let t = tab.read(cx);
                    [t.line_at_row(11), t.line_at_row(12), t.line_at_row(13)]
                }));
            })
        };
        let n = format!("file-code-fold-{slug}");
        let (shot, g) = shoot(false);
        keep_shot(&n, &shot);
        let lh = f32::from(g.line_height);
        let mid = |row: usize| f32::from(g.bounds.top()) + lh * (row as f32 + 0.5);
        let at = |x: f32, y: f32| pixel_at(&shot, dims, x, y);
        // The lines drawn: 12, then 37 (0-based 11, then 36).
        assert_eq!(
            rows.get(),
            [Some(11), Some(36), Some(37)],
            "{n}: the rows after the fold"
        );
        // The columns: numbers · fold (14 pt) · marks · bar · text.
        let fold_left = g.fold_left.expect("a fold column");
        let marks_left = g.marks_left.expect("the marks' column");
        assert_eq!(
            fold_left,
            g.numbers_right + px(code::NUMBER_GAP / 2.0),
            "{n}"
        );
        assert_eq!(marks_left, fold_left + px(code::FOLD_COLUMN), "{n}: 14 pt");
        assert!(
            g.bar_left >= marks_left + px(16.0),
            "{n}: the bar after the marks"
        );
        let column = |shot: &Shot, row: usize| {
            let x0 = f32::from(g.bounds.left() + fold_left);
            (2..26).any(|dx| {
                (-4..=4).any(|dy| {
                    let p = pixel_at(shot, dims, x0 + dx as f32 * 0.5, mid(row) + dy as f32);
                    let ground = pixel_at(shot, dims, x0 + 0.5, mid(row) - lh * 0.4);
                    !live_near(p, as_rgb(ground), 40)
                })
            })
        };
        assert!(column(&shot, 11), "{n}: ▸ on the folded line");
        for row in [3, 7, 13] {
            assert!(
                !column(&shot, row),
                "{n}: no ▾ on row {row} with the pointer away"
            );
        }
        // The folded line: the hidden added line's bar, the warmest glow, a mark.
        // (The row's glow is drawn over its bar, as on any warm changed line.)
        let lit_green = mix(green, WARM_RGB, GLOW[2]);
        let bar = at(f32::from(g.bounds.left() + g.bar_left) + 1.5, mid(11));
        assert!(
            live_near(bar, lit_green, 12),
            "{n}: the bar {bar:?}, not {lit_green:?}"
        );
        let right = f32::from(g.bounds.right()) - 30.0;
        let warm = mix(bg, WARM_RGB, GLOW[2]);
        let p = at(right, mid(11));
        assert!(live_near(p, warm, 3), "{n}: the glow {p:?}, not {warm:?}");
        assert!(live_near(at(right, mid(12)), bg, 1), "{n}: line 37 unlit");
        let marked = |row: usize| {
            let x = f32::from(g.bounds.left() + marks_left) + 8.0;
            let ground = at(x - 7.5, mid(row) - lh * 0.45);
            (-4..=4).any(|dy| !live_near(at(x, mid(row) + dy as f32), as_rgb(ground), 40))
        };
        assert!(marked(11), "{n}: the folded line's mark");
        assert!(!marked(12), "{n}: no mark on line 37");
        // The pill: after the folded line's text, its ground and its words.
        let text_end = f32::from(g.bounds.left() + g.text_left)
            + f32::from(g.cell_width) * "fn refresh(s: &mut Session) {".len() as f32;
        let ground = at(text_end + 2.0, mid(11));
        assert!(live_near(ground, warm, 3), "{n}: the gap before the pill");
        let pill = at(text_end + code::FOLD_PILL_GAP + 2.0, mid(11));
        assert!(!live_near(pill, warm, 1), "{n}: the pill's ground {pill:?}");
        let words = (0..120).any(|dx| {
            let x = text_end + code::FOLD_PILL_GAP + code::FOLD_PILL_PAD + dx as f32 * 0.5;
            !live_near(at(x, mid(11)), as_rgb(pill), 30)
        });
        assert!(words, "{n}: the pill's words");
        println!("ui-shots: {n} ok");

        // The pointer over the gutter: the open blocks show ▾ (faint); the folded line keeps ▸.
        let n = format!("file-code-fold-hover-{slug}");
        let (shot, _) = shoot(true);
        keep_shot(&n, &shot);
        for row in [3, 7, 11] {
            assert!(column(&shot, row), "{n}: a fold mark on row {row}");
        }
        assert!(!column(&shot, 4), "{n}: none inside a block");
        println!("ui-shots: {n} ok");
    }
}

/// `file-code-fold-keys` (M11ah Task 5): ⌥⌘[ on the current line folds its innermost block and
/// ⌥⌘] unfolds it; a press on ▸ or the pill unfolds, on ▾ folds; Fold All / Unfold All; a press
/// under a fold selects the line drawn there (Review Focus 5); Copy over a fold copies the hidden
/// lines; a find to a hidden match opens its fold; Follow (on) after an agent's change inside a
/// fold opens it and stays on; folding turns Follow off; a file with no blocks has no fold column.
fn file_code_fold_keys_scenes() {
    use crate::file_tab::{FoldAll, UnfoldAll};
    use crate::live_lines::{Sure, Who};
    use gpui::{Focusable, Modifiers, MouseButton};

    let path = live_file("fold-keys.rs", &fold_fixture());
    let flat = live_file("fold-flat.txt", "one\ntwo\nthree\n");
    let dims = size(px(760.0), px(400.0));
    let n = "file-code-fold-keys";
    for &(_, name) in shot_themes() {
        let theme = shot_theme(name);
        code_shot(&theme, &path, dims, |cx, window, tab| {
            let none = Modifiers::none();
            let look = |cx: &mut VisualTestAppContext| {
                live_frame(cx, window);
                cx.read(|cx| {
                    let t = tab.read(cx);
                    (t.folded_lines(), t.current_line(), t.geom().expect("drawn"))
                })
            };
            let row_y = |g: &crate::file_tab::Geom, row: usize| {
                g.bounds.top() + g.line_height * (row as f32 + 0.5) - g.scroll.y
            };
            let press = |cx: &mut VisualTestAppContext, at: gpui::Point<gpui::Pixels>| {
                cx.simulate_mouse_down(window, at, MouseButton::Left, none);
                cx.simulate_mouse_up(window, at, MouseButton::Left, none);
            };
            cx.update_window(window, |_, window, cx| {
                tab.read(cx).focus_handle(cx).focus(window, cx)
            })
            .expect("focus");
            let (_, _, g) = look(cx);
            // A press on line 13 (0-based 12), then ⌥⌘[: `fn refresh` folds, the current
            // line moves up to it; ⌥⌘] opens it.
            press(
                cx,
                point(g.bounds.left() + g.text_left + px(8.0), row_y(&g, 12)),
            );
            let (_, current, _) = look(cx);
            assert_eq!(current, Some(12), "{n}: the press's line is current");
            cx.simulate_keystrokes(window, "alt-cmd-[");
            let (folded, current, _) = look(cx);
            assert_eq!(folded, [11], "{n}: ⌥⌘[ folds the innermost block");
            assert_eq!(current, Some(11), "{n}: the current line moves to its fold");
            assert_eq!(cx.read(|cx| tab.read(cx).line_at_row(12)), Some(36), "{n}");
            cx.simulate_keystrokes(window, "alt-cmd-]");
            assert!(look(cx).0.is_empty(), "{n}: ⌥⌘] unfolds it");
            // The fold column: ▾ folds, ▸ unfolds.
            let (_, _, g) = look(cx);
            let fold_x = g.bounds.left() + g.fold_left.expect("a fold column") + px(7.0);
            press(cx, point(fold_x, row_y(&g, 11)));
            assert_eq!(look(cx).0, [11], "{n}: a press on ▾ folds");
            press(cx, point(fold_x, row_y(&g, 11)));
            assert!(look(cx).0.is_empty(), "{n}: a press on ▸ unfolds");
            // The pill unfolds.
            cx.simulate_keystrokes(window, "alt-cmd-[");
            let (folded, _, g) = look(cx);
            assert_eq!(folded, [11], "{n}");
            let text_end = g.bounds.left()
                + g.text_left
                + g.cell_width * "fn refresh(s: &mut Session) {".len() as f32;
            press(cx, point(text_end + px(16.0), row_y(&g, 11)));
            assert!(look(cx).0.is_empty(), "{n}: a press on the pill unfolds");
            // Review Focus 5: a press under a fold selects the line drawn there.
            cx.update(|cx| tab.update(cx, |t, cx| t.fold_for_shots(&[11], cx)));
            let (_, _, g) = look(cx);
            press(
                cx,
                point(g.bounds.left() + g.text_left + px(8.0), row_y(&g, 12)),
            );
            assert_eq!(look(cx).1, Some(36), "{n}: the line after the fold");
            // Copy over the fold copies the real lines, the hidden ones too.
            let from = point(g.bounds.left() + g.text_left + px(1.0), row_y(&g, 11));
            let to = point(g.bounds.left() + g.text_left + px(1.0), row_y(&g, 13));
            cx.simulate_mouse_down(window, from, MouseButton::Left, none);
            cx.simulate_mouse_move(window, to, MouseButton::Left, none);
            cx.simulate_mouse_up(window, to, MouseButton::Left, none);
            live_frame(cx, window);
            let copied = cx.read(|cx| tab.read(cx).selected_text());
            assert!(
                copied.starts_with("fn refresh(s: &mut Session) {\n    let v12 = 12;\n"),
                "{n}: the copy starts {:?}",
                copied.lines().take(2).collect::<Vec<_>>()
            );
            assert!(
                copied.contains("    let v34 = 34;\n}\n"),
                "{n}: and holds the rest"
            );
            // Fold All / Unfold All (the palette's).
            for (action, want) in [
                (
                    Box::new(FoldAll) as Box<dyn gpui::Action>,
                    vec![3, 7, 11, 37],
                ),
                (Box::new(UnfoldAll), vec![]),
            ] {
                cx.update_window(window, |_, window, cx| window.dispatch_action(action, cx))
                    .expect("dispatch");
                assert_eq!(look(cx).0, want, "{n}: Fold All / Unfold All");
            }
            // Find: a match inside a fold is counted, and stepping to it opens the fold.
            cx.update(|cx| tab.update(cx, |t, cx| t.fold_for_shots(&[11], cx)));
            live_frame(cx, window);
            cx.simulate_keystrokes(window, "cmd-f");
            cx.simulate_input(window, "v20 ");
            live_frame(cx, window);
            assert_eq!(cx.read(|cx| tab.read(cx).find_label()), "1 of 1", "{n}");
            assert!(look(cx).0.is_empty(), "{n}: the find opened the fold");
            cx.simulate_keystrokes(window, "escape");
            cx.update_window(window, |_, window, cx| {
                tab.read(cx).focus_handle(cx).focus(window, cx)
            })
            .expect("focus");
            // Follow on: an agent's change inside a fold opens it, and Follow stays on.
            cx.update(|cx| tab.update(cx, |t, cx| t.fold_for_shots(&[11], cx)));
            live_frame(cx, window);
            let codex = Who::Agent(crate::edit_ledger::agent_mark_for("codex"));
            let record = live_record(40, &[(19..20, codex, Sure::Exact, 2)]);
            cx.update(|cx| {
                tab.update(cx, |t, cx| {
                    t.set_live_for_shot(record, live_now(), Some(true));
                    t.follow_for_shots(cx);
                })
            });
            assert!(look(cx).0.is_empty(), "{n}: Follow opened the fold");
            assert!(
                cx.read(|cx| tab.read(cx).follow_on()),
                "{n}: Follow stays on"
            );
            // Folding is you moving the view: Follow turns off.
            cx.simulate_keystrokes(window, "alt-cmd-[");
            assert!(!look(cx).0.is_empty(), "{n}: folded");
            assert!(
                !cx.read(|cx| tab.read(cx).follow_on()),
                "{n}: folding ends Follow"
            );
            // Final review: Fold All leaves rows that fit the view (no scroll at all); a find
            // step into `fn refresh` opens its fold and the match lands on screen, not cut
            // short by the shorter rows' limit.
            cx.update_window(window, |_, window, cx| {
                window.dispatch_action(Box::new(FoldAll), cx)
            })
            .expect("dispatch");
            let (_, _, g) = look(cx);
            assert_eq!(g.max_scroll.y, px(0.0), "{n}: folded, it fits");
            cx.simulate_keystrokes(window, "cmd-f");
            cx.simulate_input(window, "v30 ");
            live_frame(cx, window);
            assert_eq!(cx.read(|cx| tab.read(cx).find_label()), "1 of 1", "{n}");
            let (folded, _, g) = look(cx);
            assert!(!folded.contains(&11), "{n}: the find opened the fold");
            let top = cx
                .read(|cx| tab.read(cx).line_top(30, cx))
                .expect("the match's line drawn");
            assert!(
                top >= g.scroll.y && top + g.line_height <= g.scroll.y + g.bounds.size.height,
                "{n}: the match on screen: its top {top:?}, the scroll {:?}, the view {:?}",
                g.scroll.y,
                g.bounds.size.height
            );
            cx.simulate_keystrokes(window, "escape");
        });
        // A file with no blocks: no fold column.
        let (_, g) = code_shot(&theme, &flat, dims, |_, _, _| {});
        assert_eq!(g.fold_left, None, "{n}: no blocks, no fold column");
    }
    println!("ui-shots: {n} ok");
}

/// `file-code-nav` (M11ah Task 8): the bar's previous / next change buttons (24 pt) show only when
/// the file has a change; ⌥↓ goes to the next change (its first line current, 3 rows from the
/// top, 4 under a sticky line), opening a fold that holds it; at the last change the button is
/// faint and ⌥↓ does nothing (no wrap-around); ⌥↑ goes back; a jump ends Follow; plain ↑ / ↓ and
/// ⌥↓ in the find box change nothing; the buttons click.
fn file_code_nav_scenes() {
    use crate::git_status::LineChange;
    use crate::live_lines::{Sure, Who};
    use crate::ui_tokens::code;
    use gpui::{Focusable, Modifiers, MouseButton};

    let path = live_file("nav.rs", &fold_fixture());
    let flat = live_file("nav-flat.txt", "one\ntwo\nthree\n");
    let dims = size(px(760.0), px(400.0));
    let n = "file-code-nav";
    let theme = shot_theme(shot_themes()[0].1);
    let ui = theme.ui();
    // 1-based, as git counts: lines 2, 21 (inside `fn refresh`) and 39 changed.
    let changes = vec![
        (2, LineChange::Changed),
        (21, LineChange::Added),
        (39, LineChange::Changed),
    ];
    let ends: std::rc::Rc<std::cell::Cell<[Option<gpui::Bounds<gpui::Pixels>>; 2]>> =
        std::rc::Rc::default();
    let ends_in = ends.clone();
    let (shot, _) = code_shot(&theme, &path, dims, |cx, window, tab| {
        cx.update(|cx| tab.update(cx, |t, cx| t.set_changes(changes, cx)));
        cx.update(|cx| tab.update(cx, |t, cx| t.fold_for_shots(&[11], cx)));
        live_frame(cx, window);
        cx.update_window(window, |_, window, cx| {
            tab.read(cx).focus_handle(cx).focus(window, cx)
        })
        .expect("focus");
        let look = |cx: &mut VisualTestAppContext| {
            live_frame(cx, window);
            cx.read(|cx| {
                let t = tab.read(cx);
                (
                    t.current_line(),
                    t.top_line_for_shots(),
                    t.folded_lines(),
                    t.change_nav_for_shots(),
                )
            })
        };
        // Both buttons, 24 pt; at the top with no current line: nothing before, a change after.
        let (current, top, _, nav) = look(cx);
        assert_eq!((current, top), (None, Some(0)), "{n}");
        assert_eq!(nav, Some((false, true)), "{n}: previous faint, next live");
        let [prev, next] = cx.read(|cx| tab.read(cx).nav_drawn());
        let (prev, next) = (prev.expect("previous drawn"), next.expect("next drawn"));
        for b in [prev, next] {
            assert_eq!(
                (f32::from(b.size.width), f32::from(b.size.height)),
                (code::NAV_BUTTON, code::NAV_BUTTON),
                "{n}: the buttons are 24 pt"
            );
        }
        assert!(prev.right() <= next.left(), "{n}: previous, then next");
        // ⌥↓: the first change; the fold holding the next one is still shut.
        cx.simulate_keystrokes(window, "alt-down");
        let (current, top, folded, nav) = look(cx);
        assert_eq!(current, Some(1), "{n}: the first change is current");
        assert_eq!(top, Some(0), "{n}: near the top it stays there");
        assert_eq!(folded, [11], "{n}");
        assert_eq!(nav, Some((false, true)), "{n}");
        // ⌥↓: a change inside the fold opens it; the line sits 4 rows down (3, and one more for
        // the sticky line `fn refresh` pins there).
        cx.simulate_keystrokes(window, "alt-down");
        let (current, top, folded, nav) = look(cx);
        assert_eq!(current, Some(20), "{n}");
        assert!(folded.is_empty(), "{n}: the fold opened");
        assert_eq!(
            top,
            Some(16),
            "{n}: 4 rows from the top under a sticky line"
        );
        assert_eq!(nav, Some((true, true)), "{n}");
        // ⌥↓ to the last change: next goes faint; again does nothing.
        cx.simulate_keystrokes(window, "alt-down");
        let (current, top, _, nav) = look(cx);
        assert_eq!(current, Some(38), "{n}: the last change");
        assert_eq!(
            nav,
            Some((true, false)),
            "{n}: next is faint at the last change"
        );
        cx.simulate_keystrokes(window, "alt-down");
        let (again, top_again, _, _) = look(cx);
        assert_eq!((again, top_again), (Some(38), top), "{n}: no wrap-around");
        // Plain arrows change nothing, as before.
        for key in ["up", "down"] {
            cx.simulate_keystrokes(window, key);
            let (c, t, _, _) = look(cx);
            assert_eq!((c, t), (Some(38), top), "{n}: plain {key} does nothing");
        }
        // ⌥↓ typed in the find box is the box's, not a jump.
        cx.simulate_keystrokes(window, "alt-up");
        assert_eq!(look(cx).0, Some(20), "{n}: ⌥↑ goes back");
        cx.simulate_keystrokes(window, "cmd-f");
        live_frame(cx, window);
        cx.simulate_keystrokes(window, "alt-down");
        assert_eq!(
            look(cx).0,
            Some(20),
            "{n}: not while the find box has the keys"
        );
        cx.simulate_keystrokes(window, "escape");
        cx.update_window(window, |_, window, cx| {
            tab.read(cx).focus_handle(cx).focus(window, cx)
        })
        .expect("focus");
        // ⌥↑ to the first change: previous goes faint; again does nothing.
        cx.simulate_keystrokes(window, "alt-up");
        let (current, top, _, nav) = look(cx);
        assert_eq!(current, Some(1), "{n}");
        assert_eq!(
            nav,
            Some((false, true)),
            "{n}: previous is faint at the first change"
        );
        cx.simulate_keystrokes(window, "alt-up");
        assert_eq!(look(cx).0, Some(1), "{n}: no wrap-around");
        assert_eq!(look(cx).1, top, "{n}");
        // The buttons click.
        let centre = |b: gpui::Bounds<gpui::Pixels>| b.center();
        let none = Modifiers::none();
        let [_, next] = cx.read(|cx| tab.read(cx).nav_drawn());
        let at = centre(next.expect("next drawn"));
        cx.simulate_mouse_down(window, at, MouseButton::Left, none);
        cx.simulate_mouse_up(window, at, MouseButton::Left, none);
        assert_eq!(look(cx).0, Some(20), "{n}: the next button jumps");
        // A jump ends Follow.
        let codex = Who::Agent(crate::edit_ledger::agent_mark_for("codex"));
        let record = live_record(40, &[(30..31, codex, Sure::Exact, 2)]);
        cx.update(|cx| {
            tab.update(cx, |t, _| {
                t.set_live_for_shot(record, live_now(), Some(true));
            })
        });
        assert!(cx.read(|cx| tab.read(cx).follow_on()), "{n}: Follow is on");
        cx.simulate_keystrokes(window, "alt-down");
        assert_eq!(look(cx).0, Some(38), "{n}");
        assert!(
            !cx.read(|cx| tab.read(cx).follow_on()),
            "{n}: a jump ends Follow"
        );
        ends_in.set(cx.read(|cx| tab.read(cx).nav_drawn()));
    });
    keep_shot(n, &shot);
    // At the last change, previous is live and next is faint (nearer `text_faint` than `text`).
    let [prev, next] = ends.get().map(|b| b.expect("drawn"));
    let dist = |p: [u8; 3], c: alacritty_terminal::vte::ansi::Rgb| {
        p.iter()
            .zip([c.r, c.g, c.b])
            .map(|(a, b)| u32::from(a.abs_diff(b)))
            .sum::<u32>()
    };
    let (live_ink, faint_ink) = (
        ink_in(&shot, dims, prev, [ui.panel.r, ui.panel.g, ui.panel.b]),
        ink_in(&shot, dims, next, [ui.panel.r, ui.panel.g, ui.panel.b]),
    );
    assert!(
        dist(live_ink, ui.text) < dist(live_ink, ui.text_faint),
        "{n}: previous is live"
    );
    assert!(
        dist(faint_ink, ui.text_faint) < dist(faint_ink, ui.text),
        "{n}: next is faint"
    );
    // A file with no change: no buttons.
    code_shot(&theme, &flat, dims, |cx, window, tab| {
        live_frame(cx, window);
        assert_eq!(
            cx.read(|cx| tab.read(cx).change_nav_for_shots()),
            None,
            "{n}"
        );
        assert_eq!(cx.read(|cx| tab.read(cx).nav_drawn()), [None, None], "{n}");
    });
    println!("ui-shots: {n} ok");
}

/// The sticky line and breadcrumb scenes' file: `impl Session` (0-based line 5) holds
/// `fn refresh` (line 6, its first line far wider than the view, in tall letters), whose 30 lines
/// (7..37) and `}` follow; then `fn small` (line 39). Then `impl Other` (line 44) holds a note
/// and `fn last` (line 46), whose 20 lines (47..67) follow.
fn sticky_fixture() -> String {
    let mut text = String::from(
        "// A session, for the sticky line and breadcrumb scenes.\nstruct Session {\n    id: u32,\n}\n\nimpl Session {\n",
    );
    text.push_str(&format!(
        "    fn refresh(&mut self, data: {}) {{\n",
        "H".repeat(200)
    ));
    for k in 7..37 {
        text.push_str(&format!("        let v{k} = {k};\n"));
    }
    text.push_str("    }\n\n    fn small(&self) {\n        let z = 0;\n    }\n}\n");
    text.push_str("\nimpl Other {\n    // A note.\n    fn last(&self) {\n");
    for k in 47..67 {
        text.push_str(&format!("        let w{k} = {k};\n"));
    }
    text.push_str("    }\n}\n");
    text
}

/// `sticky_fixture`'s line count.
const STICKY_LINES: usize = 69;

/// `file-code-sticky` (both themes; M11ah Task 6): scrolled into `fn refresh`, its first line is
/// pinned over the text's first row with its number, on the background, a 1 pt `divider` under
/// it; with Wrap on it is one row, cut with "…"; a press on it scrolls back (that line on top,
/// current, Follow off); Follow and a find step land below it (`REVEAL_ABOVE` rows under it), and
/// a match under it is scrolled clear of it. The line exactly at the top has none over it.
fn file_code_sticky_scenes() {
    use crate::ui_tokens::code;
    use gpui::{Focusable, Modifiers, MouseButton};

    let path = live_file("sticky.rs", &sticky_fixture());
    let dims = size(px(760.0), px(400.0));
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let ui = theme.ui();
        let bg = theme.background;
        let bg3 = [bg.r, bg.g, bg.b];
        // Scrolled so line 16 (0-based 15) is at the top.
        let n = format!("file-code-sticky-{slug}");
        let (shot, g) = code_shot(&theme, &path, dims, |cx, window, tab| {
            let g = cx.read(|cx| tab.read(cx).geom()).expect("drawn");
            // Line 16 (0-based 15, under the sticky line) and line 17 current: their band.
            cx.update(|cx| {
                tab.update(cx, |t, cx| {
                    t.set_scroll(point(px(0.0), g.line_height * 15.0), cx);
                    t.set_current_for_shots(Some(15), cx);
                })
            });
            live_frame(cx, window);
            assert_eq!(
                cx.read(|cx| tab.read(cx).sticky_line()),
                Some(6),
                "file-code-sticky: fn refresh pinned"
            );
        });
        keep_shot(&n, &shot);
        let lh = f32::from(g.line_height);
        let top = f32::from(g.bounds.top());
        let at = |x: f32, y: f32| pixel_at(&shot, dims, x, y);
        let cell = |k: usize| f32::from(g.bounds.left() + g.text_left + g.cell_width * k as f32);
        let row_box = |x0: f32, x1: f32, row: usize| {
            gpui::Bounds::from_corners(
                point(px(x0), px(top + lh * row as f32 + 2.0)),
                point(px(x1), px(top + lh * (row as f32 + 1.0) - 2.0)),
            )
        };
        // Its text: `fn` from cell 4 (line 16 has spaces there, its `let` at cell 8).
        let fn_ink = ink_in(&shot, dims, row_box(cell(4), cell(6), 0), bg3);
        assert!(
            !live_near(fn_ink, bg, 40),
            "{n}: `fn` drawn at cell 4 of row 0"
        );
        // Its number: 7, in the numbers' column.
        let right = f32::from(g.bounds.left() + g.numbers_right);
        let number = ink_in(
            &shot,
            dims,
            row_box(right - f32::from(g.cell_width), right, 0),
            bg3,
        );
        assert!(!live_near(number, bg, 40), "{n}: its number");
        // On the background (the far right of its row), a divider 1 pt under it.
        // On the background: the current line's band under it is covered.
        let gutter = f32::from(g.bounds.left()) + 4.0;
        let ground = at(gutter, top + lh * 0.5);
        assert!(
            live_near(ground, bg, 1),
            "{n}: on the background {ground:?}"
        );
        let far = f32::from(g.bounds.right()) - 30.0;
        let rule = at(far, top + lh + code::STICKY_RULE / 2.0);
        assert!(
            live_near(rule, ui.divider, 8),
            "{n}: the divider {rule:?}, not {:?}",
            ui.divider
        );
        assert!(
            live_near(at(far, top + lh + 4.0), bg, 2),
            "{n}: one point only"
        );
        println!("ui-shots: {n} ok");

        // Wrap on: one row, cut with "…" (dots on the baseline, where `H` would stand tall).
        let n = format!("file-code-sticky-wrap-{slug}");
        let (shot, g) = code_shot(&theme, &path, dims, |cx, window, tab| {
            cx.update(|cx| cx.global_mut::<crate::appearance::Appearance>().file_wrap = true);
            live_frame(cx, window);
            let g = cx.read(|cx| tab.read(cx).geom()).expect("drawn");
            cx.update(|cx| {
                tab.update(cx, |t, cx| {
                    t.set_scroll(point(px(0.0), g.line_height * 20.0), cx)
                })
            });
            live_frame(cx, window);
            assert_eq!(
                cx.read(|cx| tab.read(cx).sticky_line()),
                Some(6),
                "file-code-sticky-wrap"
            );
        });
        keep_shot(&n, &shot);
        let (lh, top) = (f32::from(g.line_height), f32::from(g.bounds.top()));
        let cw = f32::from(g.cell_width);
        let text_x = f32::from(g.bounds.left() + g.text_left);
        let columns = ((f32::from(g.bounds.size.width) - f32::from(g.text_left) - 24.0) / cw)
            .floor() as usize;
        let upper = |k: usize, row: usize| {
            gpui::Bounds::from_corners(
                point(
                    px(text_x + cw * k as f32 + 1.0),
                    px(top + lh * row as f32 + lh * 0.2),
                ),
                point(
                    px(text_x + cw * (k + 1) as f32 - 1.0),
                    px(top + lh * row as f32 + lh * 0.45),
                ),
            )
        };
        let lower = |k: usize, row: usize| {
            gpui::Bounds::from_corners(
                point(
                    px(text_x + cw * k as f32),
                    px(top + lh * row as f32 + lh * 0.55),
                ),
                point(
                    px(text_x + cw * (k + 1) as f32),
                    px(top + lh * row as f32 + lh * 0.85),
                ),
            )
        };
        let tall = ink_in(&shot, dims, upper(columns - 2, 0), bg3);
        assert!(
            !live_near(tall, bg, 40),
            "{n}: an H before the cut ({columns} cells)"
        );
        let dots_top = ink_in(&shot, dims, upper(columns - 1, 0), bg3);
        assert!(
            live_near(dots_top, bg, 30),
            "{n}: no H in the last cell {dots_top:?}"
        );
        let dots = ink_in(&shot, dims, lower(columns - 1, 0), bg3);
        assert!(!live_near(dots, bg, 40), "{n}: the … in the last cell");
        // One row: the row under it is another line's (its `let`, at cell 8; nothing at 4).
        let under = ink_in(&shot, dims, upper(4, 1), bg3);
        assert!(live_near(under, bg, 30), "{n}: one row only");
        println!("ui-shots: {n} ok");
    }

    // State: the press, Follow, and Find.
    let n = "file-code-sticky-state";
    let theme = shot_theme(shot_themes()[0].1);
    code_shot(&theme, &path, dims, |cx, window, tab| {
        let none = Modifiers::none();
        let read = |cx: &mut VisualTestAppContext| {
            live_frame(cx, window);
            cx.read(|cx| {
                let t = tab.read(cx);
                let g = t.geom().expect("drawn");
                let top = (f32::from(g.scroll.y) / f32::from(g.line_height)).round() as usize;
                (top, t.sticky_line(), t.current_line(), t.follow_on(), g)
            })
        };
        let scroll_to = |cx: &mut VisualTestAppContext, row: usize| {
            let g = cx.read(|cx| tab.read(cx).geom()).expect("drawn");
            cx.update(|cx| {
                tab.update(cx, |t, cx| {
                    t.set_scroll(point(px(0.0), g.line_height * row as f32), cx)
                })
            });
        };
        cx.update_window(window, |_, window, cx| {
            tab.read(cx).focus_handle(cx).focus(window, cx)
        })
        .expect("focus");
        live_frame(cx, window);
        // Exactly at a block's own first line it is not pinned (`impl Session` has nothing
        // above it; `fn refresh` has `impl Session` pinned); one point further, it is.
        scroll_to(cx, 5);
        assert_eq!(read(cx).1, None, "{n}: none at impl's own first line");
        scroll_to(cx, 6);
        assert_eq!(
            read(cx).1,
            Some(5),
            "{n}: at fn refresh's own first line, impl"
        );
        let g = read(cx).4;
        cx.update(|cx| {
            tab.update(cx, |t, cx| {
                t.set_scroll(point(px(0.0), g.line_height * 6.0 + px(2.0)), cx)
            })
        });
        assert_eq!(
            read(cx).1,
            Some(6),
            "{n}: pinned once its line scrolls above the top"
        );
        // A press on it: back to its line, on top and current; Follow off.
        scroll_to(cx, 15);
        let codex = crate::live_lines::Who::Agent(crate::edit_ledger::agent_mark_for("codex"));
        let record = live_record(
            STICKY_LINES,
            &[(33..34, codex.clone(), crate::live_lines::Sure::Exact, 2)],
        );
        cx.update(|cx| {
            tab.update(cx, |t, _| {
                t.set_live_for_shot(record, live_now(), Some(true))
            })
        });
        let (_, sticky, _, follow, g) = read(cx);
        assert_eq!((sticky, follow), (Some(6), true), "{n}");
        let on = point(
            g.bounds.left() + g.text_left + px(40.0),
            g.bounds.top() + g.line_height * 0.5,
        );
        cx.simulate_mouse_down(window, on, MouseButton::Left, none);
        cx.simulate_mouse_up(window, on, MouseButton::Left, none);
        // Fix round 1: its line is the first row seen, right under the sticky line pinned
        // there now (impl, which holds it), not covered by it.
        // (A sticky line would pin impl over it at the top: one row lower, impl's own line is
        // at the top, so nothing is pinned and its line is the first row under it.)
        let (top, sticky, current, follow, _) = read(cx);
        assert_eq!((top, sticky), (5, None), "{n}: impl's own line on top");
        let seen = cx.read(|cx| tab.read(cx).line_at_row(top + 1));
        assert_eq!(seen, Some(6), "{n}: its line right under it, seen");
        assert_eq!(current, Some(6), "{n}: and makes it current");
        assert!(!follow, "{n}: a press on it ends Follow");
        // In `fn last` (a note between it and `impl Other`): pressed, its line lands right
        // under `impl Other` pinned, seen (not covered), and current.
        scroll_to(cx, 52);
        assert_eq!(read(cx).1, Some(46), "{n}: fn last pinned");
        cx.simulate_mouse_down(window, on, MouseButton::Left, none);
        cx.simulate_mouse_up(window, on, MouseButton::Left, none);
        let (top, sticky, current, _, _) = read(cx);
        assert_eq!(sticky, Some(44), "{n}: impl Other pinned over the note");
        assert_eq!(top, 45, "{n}: the note under the sticky line");
        let seen = cx.read(|cx| tab.read(cx).line_at_row(top + 1));
        assert_eq!(seen, Some(46), "{n}: fn last right under the sticky line");
        assert_eq!(current, Some(46), "{n}: and current");
        // Follow: the agent's change (line 27, 0-based 26) out of view lands 3 rows under the
        // sticky line.
        scroll_to(cx, 0);
        cx.update(|cx| {
            tab.update(cx, |t, cx| {
                let record = live_record(
                    STICKY_LINES,
                    &[(26..27, codex.clone(), crate::live_lines::Sure::Exact, 2)],
                );
                t.set_live_for_shot(record, live_now(), Some(true));
                t.follow_for_shots(cx);
            })
        });
        let (top, sticky, _, follow, _) = read(cx);
        assert_eq!(sticky, Some(6), "{n}: Follow under the sticky line");
        assert_eq!(top, 26 - 4, "{n}: the change 3 rows below the sticky line");
        assert!(follow, "{n}: Follow stays on");
        // Find: a new query's match off screen lands 3 rows under the sticky line.
        scroll_to(cx, 0);
        cx.simulate_keystrokes(window, "cmd-f");
        cx.simulate_input(window, "v25 ");
        let (top, sticky, _, _, _) = read(cx);
        assert_eq!(cx.read(|cx| tab.read(cx).find_label()), "1 of 1", "{n}");
        assert_eq!(
            (top, sticky),
            (25 - 4, Some(6)),
            "{n}: the find lands below it"
        );
        // A match under the sticky line (its line at the top) is scrolled clear of it by a
        // step, which makes its line current.
        scroll_to(cx, 25);
        assert_eq!(read(cx).1, Some(6), "{n}");
        cx.simulate_keystrokes(window, "cmd-g");
        let (top, _, current, _, _) = read(cx);
        assert_eq!(
            (top, current),
            (25 - 4, Some(25)),
            "{n}: the step's match scrolled clear of the sticky line"
        );
        // Fix round 1: the rows reflow with no scroll of yours (Wrap on, then a bigger font,
        // then a narrower window): the line at the top stays, and the place with it.
        cx.simulate_keystrokes(window, "escape");
        cx.update(|cx| cx.global_mut::<crate::appearance::Appearance>().file_wrap = true);
        live_frame(cx, window);
        scroll_to(cx, 20);
        let place = |cx: &mut VisualTestAppContext| {
            live_frame(cx, window);
            cx.read(|cx| {
                let t = tab.read(cx);
                let g = t.geom().expect("drawn");
                (t.top_line_for_shots(), t.sticky_line(), t.crumb_words(), g)
            })
        };
        let (top, sticky, words, g0) = place(cx);
        assert_eq!(sticky, Some(6), "{n}: wrapped, inside fn refresh");
        assert!(top.is_some_and(|l| l > 6), "{n}: the top line {top:?}");
        cx.update(|cx| cx.global_mut::<crate::appearance::Appearance>().font_size *= 1.5);
        let (after, sticky_after, words_after, g1) = place(cx);
        assert!(g1.line_height > g0.line_height, "{n}: a bigger font");
        assert_eq!(after, top, "{n}: a font change keeps the top line");
        assert_eq!(
            (sticky_after, &words_after),
            (sticky, &words),
            "{n}: and the place"
        );
    });
    // A narrower pane, Wrap on (the rows cut at another width): the top line stays, and the place
    // with it. (The off-screen window keeps its size: the tab is narrowed in a host.)
    let slot: LiveSlot = std::rc::Rc::default();
    let host: std::rc::Rc<std::cell::RefCell<Option<gpui::Entity<NarrowHost>>>> =
        std::rc::Rc::default();
    let tab_build = live_tab(theme, path.clone(), slot.clone());
    let keep = host.clone();
    let build = move |window: &mut Window, cx: &mut App| -> AnyView {
        let tab = tab_build(window, cx);
        let h = cx.new(|_| NarrowHost { tab, width: 760.0 });
        *keep.borrow_mut() = Some(h.clone());
        h.into()
    };
    render_with(&theme, dims, build, move |cx, window| {
        let tab = slot.borrow_mut().take().expect("the file tab");
        let host = host.borrow_mut().take().expect("the host");
        cx.update(|cx| cx.global_mut::<crate::appearance::Appearance>().file_wrap = true);
        live_frame(cx, window);
        let g = cx.read(|cx| tab.read(cx).geom()).expect("drawn");
        cx.update(|cx| {
            tab.update(cx, |t, cx| {
                t.set_scroll(point(px(0.0), g.line_height * 20.0), cx)
            })
        });
        let place = |cx: &mut VisualTestAppContext| {
            live_frame(cx, window);
            cx.read(|cx| {
                let t = tab.read(cx);
                let g = t.geom().expect("drawn");
                (t.top_line_for_shots(), t.sticky_line(), t.crumb_words(), g)
            })
        };
        let (top, sticky, words, g0) = place(cx);
        assert_eq!(sticky, Some(6), "{n}: wrapped, inside fn refresh");
        cx.update(|cx| {
            host.update(cx, |h, cx| {
                h.width = 480.0;
                cx.notify();
            })
        });
        let (after, sticky_after, words_after, g1) = place(cx);
        assert!(
            g1.bounds.size.width < g0.bounds.size.width,
            "{n}: a narrower pane ({:?})",
            g1.bounds.size.width
        );
        assert_eq!(after, top, "{n}: a narrower pane keeps the top line");
        assert_eq!(
            (sticky_after, &words_after),
            (sticky, &words),
            "{n}: and the place"
        );
    });
    println!("ui-shots: {n} ok");
}

/// A file tab in a host `width` points wide (a pane narrowed; the scene changes it).
struct NarrowHost {
    tab: AnyView,
    width: f32,
}

impl gpui::Render for NarrowHost {
    fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl gpui::IntoElement {
        use gpui::{ParentElement, Styled};
        gpui::div()
            .h_full()
            .w(px(self.width))
            .child(self.tab.clone())
    }
}

/// `file-code-breadcrumb` (both themes; M11ah Task 6): a strip `code::BREADCRUMB` tall under the
/// file bar (under the find bar when open), its parts in `text_dim` and the last in `text`:
/// `sticky.rs › impl Session › fn refresh(…`; narrow, `… › fn refresh(…`; a press on a part puts
/// its line on top and current and ends Follow; the file's name goes to the top. No strip in Diff,
/// in Preview, or for a flat text file; the text's top is the same with names (colours) or none.
fn file_code_breadcrumb_scenes() {
    use crate::ui_tokens::code;
    use gpui::{Modifiers, MouseButton};

    let text = sticky_fixture();
    let path = live_file("sticky-crumbs.rs", &text);
    // The same blocks in a file nothing colours: no names.
    let unnamed = live_file("sticky-crumbs.zz", &text);
    let flat = live_file("crumbs-flat.txt", "one\ntwo\nthree\n");
    let md = live_file(
        "crumbs.md",
        "# Notes\n\nSome words.\n\n## More\n\nOther words.\n",
    );
    let dims = size(px(760.0), px(400.0));
    let bar = 42.0;
    let fn_words = "fn refresh(&mut self, data: HHHH\u{2026}";
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let ui = theme.ui();
        let bg3 = [theme.background.r, theme.background.g, theme.background.b];
        let n = format!("file-code-breadcrumb-{slug}");
        let drawn = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let keep = drawn.clone();
        let (shot, g) = code_shot(&theme, &path, dims, move |cx, window, tab| {
            let g = cx.read(|cx| tab.read(cx).geom()).expect("drawn");
            cx.update(|cx| {
                tab.update(cx, |t, cx| {
                    t.set_scroll(point(px(0.0), g.line_height * 15.0), cx)
                })
            });
            live_frame(cx, window);
            *keep.borrow_mut() = cx.read(|cx| tab.read(cx).crumbs_drawn());
            assert_eq!(
                cx.read(|cx| tab.read(cx).crumb_words()),
                ["sticky-crumbs.rs", "impl Session", fn_words],
                "file-code-breadcrumb: the path"
            );
        });
        keep_shot(&n, &shot);
        let parts = drawn.borrow().clone();
        let words: Vec<&str> = parts.iter().map(|(_, w)| w.as_str()).collect();
        assert_eq!(
            words,
            ["sticky-crumbs.rs", "impl Session", fn_words],
            "{n}: all fit"
        );
        // The strip: 26 pt under the bar, the text's room under it.
        let strip = parts[0].0;
        assert_eq!(f32::from(strip.top()), bar, "{n}: under the bar");
        assert_eq!(
            f32::from(strip.size.height),
            code::BREADCRUMB,
            "{n}: 26 pt tall"
        );
        assert_eq!(
            f32::from(g.bounds.top()),
            bar + code::BREADCRUMB + code::TOP_ROOM,
            "{n}: the text under the strip"
        );
        // Parts in text_dim, the last in text.
        let first = ink_in(&shot, dims, parts[0].0, bg3);
        let last = ink_in(&shot, dims, parts[2].0, bg3);
        assert!(
            live_near(first, ui.text_dim, 40),
            "{n}: the file's name {first:?}, not {:?}",
            ui.text_dim
        );
        assert!(
            live_near(last, ui.text, 40),
            "{n}: the last part {last:?}, not {:?}",
            ui.text
        );
        assert!(
            parts[0].0.right() < parts[1].0.left(),
            "{n}: a separator between"
        );
        println!("ui-shots: {n} ok");

        // Narrow: the outer parts fold into "…"; the innermost stays.
        let n = format!("file-code-breadcrumb-narrow-{slug}");
        let narrow = size(px(300.0), px(300.0));
        let drawn = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let keep = drawn.clone();
        let (shot, _) = code_shot(&theme, &path, narrow, move |cx, window, tab| {
            let g = cx.read(|cx| tab.read(cx).geom()).expect("drawn");
            cx.update(|cx| {
                tab.update(cx, |t, cx| {
                    t.set_scroll(point(px(0.0), g.line_height * 15.0), cx)
                })
            });
            live_frame(cx, window);
            *keep.borrow_mut() = cx.read(|cx| tab.read(cx).crumbs_drawn());
        });
        keep_shot(&n, &shot);
        let words: Vec<String> = drawn.borrow().iter().map(|(_, w)| w.clone()).collect();
        assert_eq!(words, ["\u{2026}", fn_words], "{n}: … › fn refresh");
        println!("ui-shots: {n} ok");
    }

    // State: presses, the find bar, other modes, other files.
    let n = "file-code-breadcrumb-state";
    let theme = shot_theme(shot_themes()[0].1);
    let (_, named) = code_shot(&theme, &path, dims, |cx, window, tab| {
        let none = Modifiers::none();
        let read = |cx: &mut VisualTestAppContext| {
            live_frame(cx, window);
            cx.read(|cx| {
                let t = tab.read(cx);
                let g = t.geom().expect("drawn");
                let top = (f32::from(g.scroll.y) / f32::from(g.line_height)).round() as usize;
                (top, t.current_line(), t.follow_on(), t.crumbs_drawn(), g)
            })
        };
        let press = |cx: &mut VisualTestAppContext, at: gpui::Point<gpui::Pixels>| {
            cx.simulate_mouse_down(window, at, MouseButton::Left, none);
            cx.simulate_mouse_up(window, at, MouseButton::Left, none);
        };
        let g = read(cx).4;
        cx.update(|cx| {
            tab.update(cx, |t, cx| {
                t.set_scroll(point(px(0.0), g.line_height * 15.0), cx)
            })
        });
        // The current line elsewhere (fn small's): the breadcrumb follows the top, not it.
        cx.update(|cx| tab.update(cx, |t, cx| t.set_current_for_shots(Some(40), cx)));
        let codex = crate::live_lines::Who::Agent(crate::edit_ledger::agent_mark_for("codex"));
        let record = live_record(
            STICKY_LINES,
            &[(33..34, codex, crate::live_lines::Sure::Exact, 2)],
        );
        cx.update(|cx| {
            tab.update(cx, |t, _| {
                t.set_live_for_shot(record, live_now(), Some(true))
            })
        });
        let (_, current, follow, parts, _) = read(cx);
        assert_eq!((current, follow), (Some(40), true), "{n}");
        assert_eq!(
            parts[1].1, "impl Session",
            "{n}: the top's path, not the current line's"
        );
        // A press on `impl Session`: its line on top, current; Follow off.
        press(cx, parts[1].0.center());
        let (top, current, follow, _, _) = read(cx);
        assert_eq!(
            (top, current, follow),
            (5, Some(5), false),
            "{n}: a part's press"
        );
        // Fix round 1: a nested part (`fn refresh`): its line right under the sticky line
        // pinned there (impl), current.
        cx.update(|cx| {
            tab.update(cx, |t, cx| {
                t.set_scroll(point(px(0.0), g.line_height * 15.0), cx)
            })
        });
        let parts = read(cx).3;
        press(cx, parts[2].0.center());
        let (top, current, _, _, _) = read(cx);
        assert_eq!(cx.read(|cx| tab.read(cx).sticky_line()), None, "{n}");
        assert_eq!((top, current), (5, Some(6)), "{n}: a nested part's press");
        assert_eq!(
            cx.read(|cx| tab.read(cx).line_at_row(top + 1)),
            Some(6),
            "{n}"
        );
        // `fn last` (a note between it and `impl Other`): right under `impl Other` pinned.
        cx.update(|cx| {
            tab.update(cx, |t, cx| {
                t.set_scroll(point(px(0.0), g.line_height * 52.0), cx)
            })
        });
        let parts = read(cx).3;
        assert_eq!(parts[2].1, "fn last(&self)", "{n}");
        press(cx, parts[2].0.center());
        let (top, current, _, _, _) = read(cx);
        assert_eq!(cx.read(|cx| tab.read(cx).sticky_line()), Some(44), "{n}");
        assert_eq!(
            (top, current),
            (45, Some(46)),
            "{n}: a nested part under the sticky line"
        );
        assert_eq!(
            cx.read(|cx| tab.read(cx).line_at_row(top + 1)),
            Some(46),
            "{n}"
        );
        // The file's name: the top.
        cx.update(|cx| {
            tab.update(cx, |t, cx| {
                t.set_scroll(point(px(0.0), g.line_height * 15.0), cx)
            })
        });
        let parts = read(cx).3;
        press(cx, parts[0].0.center());
        let (top, current, _, _, _) = read(cx);
        assert_eq!(
            (top, current),
            (0, Some(46)),
            "{n}: the file's name goes to the top"
        );
        // The find bar open: the strip under it.
        cx.simulate_keystrokes(window, "cmd-f");
        let (_, _, _, parts, g) = read(cx);
        let strip = parts[0].0;
        assert!(f32::from(strip.top()) > bar, "{n}: under the find bar");
        assert_eq!(
            g.bounds.top(),
            strip.bottom() + px(code::TOP_ROOM),
            "{n}: the text under the strip"
        );
        cx.simulate_keystrokes(window, "escape");
        // Diff mode: no strip.
        cx.update(|cx| {
            tab.update(cx, |t, cx| {
                t.pretend_in_repo_for_shots(cx);
                t.choose_mode_for_shots(crate::layout_model::FileMode::Diff, cx);
            })
        });
        assert!(
            read_drawn(cx, window, tab).is_empty(),
            "{n}: no strip in Diff"
        );
        cx.update(|cx| {
            tab.update(cx, |t, cx| {
                t.choose_mode_for_shots(crate::layout_model::FileMode::Plain, cx)
            })
        });
        assert!(
            !read_drawn(cx, window, tab).is_empty(),
            "{n}: back in File mode"
        );
    });
    // Names or none (the colours landed, or a file nothing colours): the text's top is the same.
    let (_, plain) = code_shot(&theme, &unnamed, dims, |_, _, _| {});
    assert_eq!(
        named.bounds.top(),
        plain.bounds.top(),
        "{n}: the text does not move for names"
    );
    // A flat text file: no strip.
    let (_, g) = code_shot(&theme, &flat, dims, |cx, window, tab| {
        assert!(
            read_drawn(cx, window, tab).is_empty(),
            "{n}: no strip for a flat file"
        );
    });
    assert_eq!(
        f32::from(g.bounds.top()),
        bar + code::TOP_ROOM,
        "{n}: flat: the text under the bar"
    );
    // Markdown: a strip in File mode, none in Preview.
    code_shot(&theme, &md, dims, |cx, window, tab| {
        assert!(
            !read_drawn(cx, window, tab).is_empty(),
            "{n}: Markdown headings in File mode"
        );
        cx.update_window(window, |_, window, cx| {
            tab.update(cx, |t, cx| t.choose_preview_for_shots(true, window, cx))
        })
        .expect("preview");
        assert!(
            read_drawn(cx, window, tab).is_empty(),
            "{n}: no strip in Preview"
        );
    });
    println!("ui-shots: {n} ok");
}

/// The breadcrumb's pieces drawn this frame.
fn read_drawn(
    cx: &mut VisualTestAppContext,
    window: AnyWindowHandle,
    tab: &gpui::Entity<crate::file_tab::FileTab>,
) -> Vec<(gpui::Bounds<gpui::Pixels>, String)> {
    live_frame(cx, window);
    cx.read(|cx| tab.read(cx).crumbs_drawn())
}

/// The lane scene's file: 100 four-line functions (400 lines), a "needle" on lines 100, 250 and
/// 330 (0-based 99, 249, 329).
fn lane_fixture() -> String {
    let mut lines: Vec<String> = (0..100)
        .flat_map(|k| {
            [
                format!("fn f{k}() {{"),
                "    let a = 1;".to_string(),
                "    let b = 2;".to_string(),
                "}".to_string(),
            ]
        })
        .collect();
    for at in [99, 249, 329] {
        lines[at] = "    let needle = 3;".to_string();
    }
    lines.join("\n") + "\n"
}

/// `file-code-lane` (both themes; M11ah Task 7): in a 400-line file with changes near the top,
/// the middle and the end and three find matches, the scrollbar's lane (4 pt, at the right
/// edge) carries a mark at each change's place in the bar colours (at least 2 pt) and an accent
/// tick at each match, the current one brighter; the marks are still drawn once the thumb has
/// faded; a fold above a change moves its mark up; a press on the lane jumps as a track press.
fn file_code_lane_scenes() {
    use crate::git_status::LineChange;
    use crate::scrollbar::Axis;
    use crate::theme::mix;
    use crate::ui_tokens::{MARK_MIN, ensure_contrast};
    use gpui::{Focusable, Modifiers, MouseButton, ScrollDelta, ScrollWheelEvent, TouchPhase};

    let path = live_file("lane.rs", &lane_fixture());
    let dims = size(px(760.0), px(400.0));
    // 1-based, as git counts: lines 30 and 390 added, 200 changed.
    let changes = || {
        vec![
            (30, LineChange::Added),
            (200, LineChange::Changed),
            (390, LineChange::Added),
        ]
    };
    let setup = |cx: &mut VisualTestAppContext,
                 window: AnyWindowHandle,
                 tab: &gpui::Entity<crate::file_tab::FileTab>| {
        cx.update(|cx| tab.update(cx, |t, cx| t.set_changes(changes(), cx)));
        live_frame(cx, window);
        cx.update_window(window, |_, window, cx| {
            tab.read(cx).focus_handle(cx).focus(window, cx)
        })
        .expect("focus");
        cx.simulate_keystrokes(window, "cmd-f");
        cx.simulate_input(window, "needle ");
        live_frame(cx, window);
        assert_eq!(
            cx.read(|cx| tab.read(cx).find_label()),
            "1 of 3",
            "file-code-lane: three matches"
        );
    };
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let bg = theme.background;
        let fit = |c: alacritty_terminal::vte::ansi::Rgb| ensure_contrast(c, bg, MARK_MIN);
        let (green, blue) = (fit(theme.palette[2]), fit(theme.palette[4]));
        let accent = theme.ui().accent;
        let (found, now) = (mix(bg, accent, 0.25), mix(bg, accent, 0.55));
        // The mark at `line` (0-based, `hidden` lines folded away above it): the lane's place
        // by the geometry, and the pixels there.
        let place = |g: &crate::file_tab::Geom, line: usize, hidden: usize| {
            let (viewport, content) = (
                f32::from(g.bounds.size.height),
                f32::from(g.bounds.size.height + g.max_scroll.y),
            );
            f32::from(g.bounds.top())
                + (line - hidden) as f32 * f32::from(g.line_height) * viewport / content
        };
        let lane_x = |g: &crate::file_tab::Geom| f32::from(g.bounds.right()) - 2.0;
        // Some pixel within the mark's first 2 pt (y..y+2) is `want`.
        let has = |shot: &Shot, g: &crate::file_tab::Geom, y: f32, want, by| {
            (0..6).any(|k| {
                live_near(
                    pixel_at(shot, dims, lane_x(g), y + 0.25 + k as f32 * 0.3),
                    want,
                    by,
                )
            })
        };

        // Marks at the changes and the matches, the first match current.
        let n = format!("file-code-lane-{slug}");
        let (shot, g) = code_shot(&theme, &path, dims, |cx, window, tab| {
            setup(cx, window, tab);
            let marks = cx.read(|cx| tab.read(cx).lane_for_shots());
            let lens: Vec<f32> = marks.iter().map(|m| m.len).collect();
            assert!(
                lens.iter().all(|l| *l >= 2.0),
                "{n}: every mark is at least 2 pt: {lens:?}"
            );
            assert_eq!(marks.len(), 6, "{n}: 3 changes and 3 matches: {marks:?}");
        });
        keep_shot(&n, &shot);
        for (line, colour) in [(29, green), (199, blue), (389, green)] {
            let y = place(&g, line, 0);
            assert!(
                has(&shot, &g, y, colour, 12),
                "{n}: change at {line} (y {y})"
            );
        }
        for (line, colour, what) in [
            (99, now, "current"),
            (249, found, "match"),
            (329, found, "match"),
        ] {
            let y = place(&g, line, 0);
            assert!(
                has(&shot, &g, y, colour, 8),
                "{n}: the {what} at {line} (y {y}), not {colour:?}"
            );
        }
        assert!(
            !has(&shot, &g, place(&g, 150, 0), green, 40)
                && live_near(pixel_at(&shot, dims, lane_x(&g), place(&g, 150, 0)), bg, 6),
            "{n}: nothing drawn between the marks"
        );
        println!("ui-shots: {n} ok");

        // The thumb faded: the marks stay.
        let n = format!("file-code-lane-faded-{slug}");
        let (shot, g) = code_shot(&theme, &path, dims, |cx, window, tab| {
            setup(cx, window, tab);
            // The find scrolled the text down to its match; back up, then a wheel step shows
            // the thumb, and 1.5 s later it has faded.
            cx.update(|cx| tab.update(cx, |t, cx| t.set_scroll(point(px(0.0), px(0.0)), cx)));
            live_frame(cx, window);
            let g = cx.read(|cx| tab.read(cx).geom()).expect("drawn");
            cx.simulate_event(
                window,
                ScrollWheelEvent {
                    position: g.bounds.center(),
                    delta: ScrollDelta::Lines(point(0.0, -1.0)),
                    modifiers: Modifiers::none(),
                    touch_phase: TouchPhase::Moved,
                },
            );
            live_frame(cx, window);
            assert!(cx.read(|cx| tab.read(cx).scrollbar_shown()), "{n}: shown");
            prof::sleep(std::time::Duration::from_millis(1500));
            cx.advance_clock(std::time::Duration::from_millis(1500));
            live_frame(cx, window);
            assert!(!cx.read(|cx| tab.read(cx).scrollbar_shown()), "{n}: faded");
        });
        keep_shot(&n, &shot);
        // Scrolled one step, the marks are where the geometry puts them still.
        for (line, colour) in [(29, green), (199, blue), (389, green)] {
            let y = place(&g, line, 0);
            assert!(
                has(&shot, &g, y, colour, 12),
                "{n}: change at {line} (y {y})"
            );
        }
        println!("ui-shots: {n} ok");

        // Three blocks (12 lines) folded above the first change: its mark moves up.
        let n = format!("file-code-lane-fold-{slug}");
        let (shot, g) = code_shot(&theme, &path, dims, |cx, window, tab| {
            cx.update(|cx| {
                tab.update(cx, |t, cx| {
                    t.set_changes(changes(), cx);
                    t.fold_for_shots(&[0, 4, 8], cx);
                })
            });
            live_frame(cx, window);
        });
        keep_shot(&n, &shot);
        let y = place(&g, 29, 9);
        assert!(
            has(&shot, &g, y, green, 12),
            "{n}: the mark moved up to y {y}"
        );
        println!("ui-shots: {n} ok");

        // Hidden matches and deletions tick at their fold's row: blocks at lines 5 and 97
        // (0-based 4 and 96) folded, lines deleted above 0-based line 6 (hidden in the first)
        // and above line 150 (shown), the needle on line 100 (hidden in the second, current).
        let n = format!("file-code-lane-hidden-{slug}");
        let red = fit(theme.palette[1]);
        let (shot, g) = code_shot(&theme, &path, dims, |cx, window, tab| {
            setup(cx, window, tab);
            cx.update(|cx| {
                tab.update(cx, |t, cx| {
                    t.set_changes(
                        vec![
                            (6, LineChange::DeletedAfter),
                            (150, LineChange::DeletedAfter),
                        ],
                        cx,
                    );
                    t.fold_for_shots(&[4, 96], cx);
                })
            });
            live_frame(cx, window);
        });
        keep_shot(&n, &shot);
        for (line, hidden, colour, what) in [
            (4, 0, red, "the hidden deletion, on its fold's row"),
            (150, 6, red, "the shown deletion"),
            (96, 3, now, "the hidden current match, on its fold's row"),
            (249, 6, found, "a shown match"),
        ] {
            let y = place(&g, line, hidden);
            assert!(has(&shot, &g, y, colour, 12), "{n}: {what} (y {y})");
        }
        println!("ui-shots: {n} ok");

        // A press on the lane (between marks) jumps as a track press does: the thumb centres on it.
        let n = format!("file-code-lane-press-{slug}");
        code_shot(&theme, &path, dims, |cx, window, tab| {
            setup(cx, window, tab);
            let g = cx.read(|cx| tab.read(cx).geom()).expect("drawn");
            let at = point(
                g.bounds.right() - px(2.0),
                g.bounds.top() + g.bounds.size.height * 0.7,
            );
            let none = Modifiers::none();
            cx.simulate_mouse_down(window, at, MouseButton::Left, none);
            cx.simulate_mouse_up(window, at, MouseButton::Left, none);
            live_frame(cx, window);
            let thumb = cx
                .read(|cx| tab.read(cx).thumb_bounds(Axis::Vertical))
                .expect("a thumb");
            assert!(
                (thumb.center().y - at.y).abs() < px(3.0),
                "{n}: the thumb {thumb:?} is not centred on the press {at:?}"
            );
            assert!(
                cx.read(|cx| tab.read(cx).selection()).is_none(),
                "{n}: a press on the lane selects nothing"
            );
        });
        println!("ui-shots: {n} ok");
    }
}

/// The cards' fixture (M11ah Task 9): the new file's text and the diff giving it (with the whole
/// file as context, as fuxx asks git). The old file: a comment, a blank line, `fn setup() {`
/// (line 3) with 41 lines (line 5 holds the only `needle`), `}`, a blank line,
/// `fn refresh(&mut self) {` (line 47) with 48 lines, `}`. The new one: line 20 changed, 30
/// lines added after line 70. Rows: 19 removed, 20 added, 71..101 added; cards on rows 16..24
/// and 68..104; stretches 0..16, 24..68 (44 lines) and 104..127.
fn cards_fixture() -> (String, String) {
    let mut old: Vec<String> = vec!["// Sessions, for the cards' shots.".into(), String::new()];
    old.push("fn setup() {".into());
    for k in 4..=44 {
        old.push(if k == 5 {
            "    let needle = 5;".into()
        } else {
            format!("    let a{k} = {k};")
        });
    }
    old.push("}".into());
    old.push(String::new());
    old.push("fn refresh(&mut self) {".into());
    for k in 48..=95 {
        old.push(format!("    let b{k} = {k};"));
    }
    old.push("}".into());
    let mut new_text = String::new();
    let mut u = format!(
        "diff --git a/cards.rs b/cards.rs\n--- a/cards.rs\n+++ b/cards.rs\n@@ -1,{} +1,{} @@\n",
        old.len(),
        old.len() + 30
    );
    for (i, line) in old.iter().enumerate() {
        if i == 19 {
            let changed = "    let a20 = 2000;";
            u.push_str(&format!("-{line}\n+{changed}\n"));
            new_text.push_str(&format!("{changed}\n"));
            continue;
        }
        u.push_str(&format!(" {line}\n"));
        new_text.push_str(&format!("{line}\n"));
        if i == 69 {
            for k in 0..30 {
                let added = format!("    let c{k} = {k};");
                u.push_str(&format!("+{added}\n"));
                new_text.push_str(&format!("{added}\n"));
            }
        }
    }
    (new_text, u)
}

/// A frozen tab on `path` shown in Diff mode with `unified` as its diff (drawn).
fn cards_setup(
    cx: &mut VisualTestAppContext,
    window: AnyWindowHandle,
    tab: &gpui::Entity<crate::file_tab::FileTab>,
    unified: &str,
) {
    cx.update(|cx| {
        tab.update(cx, |t, cx| {
            t.pretend_in_repo_for_shots(cx);
            t.choose_mode_for_shots(crate::layout_model::FileMode::Diff, cx);
            t.set_diff_for_shots(unified, cx);
        })
    });
    live_frame(cx, window);
    live_frame(cx, window);
}

/// `file-diff-cards` (both themes; M11ah Task 9): Diff mode's changes as cards: a rounded panel
/// 12 pt in from the body's sides with a 1 pt `divider` border; 10 pt between two cards; each
/// title row with the block's name in `text`, `+n` in green, `−n` in red and `lines a–b` on the
/// right in `text_dim`; a folded stretch's `⋯ 44 unchanged lines — click to show` centred in
/// `text_dim`, nothing in the gutter; the indent guides drawn inside the cards; the lane one mark
/// per card (blue for both, green for added). A new file is one card, `New file · +12`, with
/// nothing folded.
fn file_diff_cards_scenes() {
    use crate::diff_cards::CardKind;
    use crate::theme::mix;
    use crate::ui_tokens::code;
    use crate::ui_tokens::{MARK_MIN, ensure_contrast};
    use alacritty_terminal::vte::ansi::Rgb;

    let (text, unified) = cards_fixture();
    let path = live_file("cards.rs", &text);
    let fresh_text: String = (1..=12).map(|k| format!("let n{k} = {k};\n")).collect();
    let fresh_path = live_file("cards-new.rs", &fresh_text);
    let fresh_diff = format!(
        "diff --git a/cards-new.rs b/cards-new.rs\n@@ -0,0 +1,12 @@\n{}",
        fresh_text
            .lines()
            .map(|l| format!("+{l}\n"))
            .collect::<String>()
    );
    let gone_path = live_file("cards-gone.rs", "");
    let gone_diff = format!(
        "diff --git a/cards-gone.rs b/cards-gone.rs\n@@ -1,60 +0,0 @@\n{}",
        (1..=60)
            .map(|k| format!("-let g{k} = {k};\n"))
            .collect::<String>()
    );
    let dims = size(px(760.0), px(560.0));
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let ui = theme.ui();
        let bg = theme.background;
        let fit = |c: Rgb| ensure_contrast(c, bg, MARK_MIN);
        let (green, red, blue) = (
            fit(theme.palette[2]),
            fit(theme.palette[1]),
            fit(theme.palette[4]),
        );
        let dim = ensure_contrast(ui.text_dim, bg, MARK_MIN);
        let n = format!("file-diff-cards-{slug}");
        type Read = (Vec<Option<Pixels>>, Vec<Option<Pixels>>, Vec<String>, usize);
        let seen: std::rc::Rc<std::cell::RefCell<Option<Read>>> = std::rc::Rc::default();
        let keep = seen.clone();
        let (shot, g) = code_shot(&theme, &path, dims, |cx, window, tab| {
            cards_setup(cx, window, tab, &unified);
            let read = cx.read(|cx| {
                let t = tab.read(cx);
                let d = t.diff_doc().expect("its rows");
                let g = &d.grouped;
                assert_eq!(g.cards.len(), 2, "two cards");
                assert_eq!(g.cards[0].kind, CardKind::Both);
                assert_eq!(g.cards[1].kind, CardKind::Added);
                assert_eq!(
                    g.stretches
                        .iter()
                        .map(|s| s.rows.clone())
                        .collect::<Vec<_>>(),
                    [0..16, 24..68, 104..127]
                );
                let words: Vec<String> = d
                    .titles
                    .iter()
                    .map(|t| {
                        format!(
                            "{}{}{} {}",
                            t.name,
                            t.separator(),
                            [t.added_words(), t.removed_words()]
                                .into_iter()
                                .flatten()
                                .collect::<Vec<_>>()
                                .join(" "),
                            t.lines_words().unwrap_or_default()
                        )
                    })
                    .collect();
                (
                    (0..2).map(|k| t.title_top(k, cx)).collect(),
                    [16, 17, 23].map(|l| t.line_top(l, cx)).to_vec(),
                    words,
                    t.lane_for_shots().len(),
                )
            });
            *keep.borrow_mut() = Some(read);
        });
        keep_shot(&n, &shot);
        let (titles, lines, words, marks) = seen.borrow_mut().take().expect("read");
        assert_eq!(
            words,
            [
                "fn setup()  +1 \u{2212}1 lines 17\u{2013}23",
                "fn refresh(&mut self)  +30 lines 68\u{2013}103"
            ],
            "{n}: the titles"
        );
        assert_eq!(marks, 2, "{n}: one lane mark per card");
        let lh = g.line_height;
        let (title0, title1) = (titles[0].expect("card 0"), titles[1].expect("card 1"));
        let (first, row17, last) = (
            lines[0].expect("16"),
            lines[1].expect("17"),
            lines[2].expect("23"),
        );
        assert_eq!(
            first - title0,
            px(code::CARD_TITLE),
            "{n}: the title's height"
        );
        // Card 0's last row, the ⋯ row, 10 pt, then card 1's title.
        assert_eq!(
            title1,
            last + lh * 2.0 + px(code::CARD_GAP),
            "{n}: 10 pt between the cards"
        );
        assert_eq!(g.scroll.y, title0, "{n}: opened at its first card");
        let at = |x: Pixels, y: Pixels| pixel_at(&shot, dims, f32::from(x), f32::from(y));
        let y_of = |top: Pixels, h: Pixels| g.bounds.top() + top - g.scroll.y + h / 2.0;
        // The panel's border, 12 pt in from each side.
        let row_mid = y_of(row17, lh);
        let (left, right) = (
            g.bounds.left() + px(code::CARD_INSET),
            g.bounds.right() - px(code::CARD_INSET),
        );
        assert!(
            live_near(at(left + px(0.5), row_mid), ui.divider, 12),
            "{n}: the left border {:?}, not {:?}",
            at(left + px(0.5), row_mid),
            ui.divider
        );
        assert!(
            live_near(at(right - px(0.5), row_mid), ui.divider, 12),
            "{n}: the right border"
        );
        assert!(
            live_near(at(left - px(4.0), row_mid), bg, 4),
            "{n}: nothing outside the panel"
        );
        // The title: the name, +1 green, −1 red; the lines on the right in text_dim.
        let has = |x0: Pixels, x1: Pixels, y: Pixels, h: Pixels, c: Rgb, by: u8| {
            let (x0, x1) = (f32::from(x0) as i32, f32::from(x1) as i32);
            (x0..x1).any(|x| {
                (2..(f32::from(h) as i32 - 2))
                    .any(|dy| live_near(at(px(x as f32), y + px(dy as f32)), c, by))
            })
        };
        let title_y = g.bounds.top() + title0 - g.scroll.y;
        let title_h = px(code::CARD_TITLE);
        let text_x = left + px(code::CARD_BORDER + code::GUTTER_LEFT);
        assert!(
            has(text_x, text_x + px(90.0), title_y, title_h, ui.text, 30),
            "{n}: the name in text"
        );
        assert!(
            has(
                text_x + px(60.0),
                text_x + px(200.0),
                title_y,
                title_h,
                green,
                30
            ),
            "{n}: +1 in green"
        );
        assert!(
            has(
                text_x + px(60.0),
                text_x + px(220.0),
                title_y,
                title_h,
                red,
                30
            ),
            "{n}: \u{2212}1 in red"
        );
        assert!(
            has(
                right - px(120.0),
                right - px(17.0),
                title_y,
                title_h,
                dim,
                30
            ),
            "{n}: the lines in text_dim"
        );
        // The folded stretch's row, centred, in text_dim; nothing in the gutter.
        let fold_y = g.bounds.top() + last + lh - g.scroll.y;
        let mid = (left + right) / 2.0;
        assert!(
            has(mid - px(120.0), mid + px(120.0), fold_y, lh, dim, 30),
            "{n}: the unchanged lines' words, centred"
        );
        assert!(
            !has(g.bounds.left(), left + px(60.0), fold_y, lh, dim, 30),
            "{n}: nothing at the left"
        );
        // The guides inside a card: line 17 (in `fn setup`) at the text's edge, `text` at 7 %.
        let guide = mix(bg, ui.text, code::GUIDE_ALPHA);
        assert!(
            live_near(
                at(g.bounds.left() + g.text_left + px(0.5), row_mid),
                guide,
                4
            ),
            "{n}: a guide in the card {:?}, not {guide:?}",
            at(g.bounds.left() + g.text_left + px(0.5), row_mid)
        );
        // The lane: card 0 blue (both), card 1 green.
        let lane_x = f32::from(g.bounds.right()) - 2.0;
        let place = |top: Pixels| {
            let (view, content) = (
                f32::from(g.bounds.size.height),
                f32::from(g.bounds.size.height + g.max_scroll.y),
            );
            f32::from(g.bounds.top()) + f32::from(top) * view / content
        };
        let lane_has = |top: Pixels, c: Rgb| {
            (0..8).any(|k| {
                live_near(
                    pixel_at(&shot, dims, lane_x, place(top) + 0.25 + k as f32 * 0.3),
                    c,
                    12,
                )
            })
        };
        assert!(lane_has(title0, blue), "{n}: card 0's mark in blue");
        assert!(lane_has(title1, green), "{n}: card 1's mark in green");
        println!("ui-shots: {n} ok");

        // A deleted file (taller than the view): one card, its lane mark red.
        let n = format!("file-diff-cards-deleted-{slug}");
        let gone_diff = gone_diff.clone();
        let (shot, g) = code_shot(&theme, &gone_path, dims, |cx, window, tab| {
            cards_setup(cx, window, tab, &gone_diff);
            cx.read(|cx| {
                let d = tab.read(cx).diff_doc().expect("its rows");
                assert_eq!(d.grouped.cards.len(), 1, "one card");
                assert_eq!(d.grouped.cards[0].kind, CardKind::Removed);
            });
        });
        keep_shot(&n, &shot);
        let lane_x = f32::from(g.bounds.right()) - 2.0;
        assert!(
            (0..8).any(|k| live_near(
                pixel_at(
                    &shot,
                    dims,
                    lane_x,
                    f32::from(g.bounds.top()) + 0.25 + k as f32 * 0.3
                ),
                red,
                12
            )),
            "{n}: its lane mark in red"
        );
        println!("ui-shots: {n} ok");

        // Scrolled into card 1 (taller than the view): its title pinned at the top, on the
        // background, a `divider` line under it.
        let n = format!("file-diff-cards-sticky-{slug}");
        let (shot, g) = code_shot(&theme, &path, dims, |cx, window, tab| {
            cards_setup(cx, window, tab, &unified);
            let lh = cx
                .read(|cx| tab.read(cx).geom())
                .expect("drawn")
                .line_height;
            let y = title1 + lh * 10.0;
            cx.update(|cx| tab.update(cx, |t, cx| t.set_scroll(point(px(0.0), y), cx)));
            live_frame(cx, window);
            assert_eq!(cx.read(|cx| tab.read(cx).sticky_title()), Some(1), "pinned");
        });
        keep_shot(&n, &shot);
        let at = |x: Pixels, y: Pixels| pixel_at(&shot, dims, f32::from(x), f32::from(y));
        let top = g.bounds.top();
        let has = |x0: Pixels, x1: Pixels, c: Rgb| {
            let (x0, x1) = (f32::from(x0) as i32, f32::from(x1) as i32);
            (x0..x1).any(|x| {
                (2..(code::CARD_TITLE as i32 - 2))
                    .any(|dy| live_near(at(px(x as f32), top + px(dy as f32)), c, 30))
            })
        };
        assert!(
            has(text_x, text_x + px(90.0), ui.text),
            "{n}: the pinned name"
        );
        assert!(
            has(text_x + px(100.0), text_x + px(260.0), green),
            "{n}: the pinned +30"
        );
        let rule = top + px(code::CARD_TITLE - 0.5);
        assert!(
            live_near(at(mid, rule), ui.divider, 12),
            "{n}: the line under it {:?}",
            at(mid, rule)
        );
        println!("ui-shots: {n} ok");

        // A new file: one card, nothing folded.
        let n = format!("file-diff-cards-new-{slug}");
        let fresh_diff = fresh_diff.clone();
        let (shot, g) = code_shot(&theme, &fresh_path, dims, |cx, window, tab| {
            cards_setup(cx, window, tab, &fresh_diff);
            cx.read(|cx| {
                let d = tab.read(cx).diff_doc().expect("its rows");
                assert_eq!(d.grouped.cards.len(), 1, "one card");
                assert!(d.grouped.stretches.is_empty(), "nothing folded");
                let t = &d.titles[0];
                assert_eq!(
                    format!(
                        "{}{}{}",
                        t.name,
                        t.separator(),
                        t.added_words().unwrap_or_default()
                    ),
                    "New file \u{b7} +12"
                );
                assert_eq!(t.lines_words(), None);
            });
        });
        keep_shot(&n, &shot);
        let left = g.bounds.left() + px(code::CARD_INSET);
        // Fix round 1: the last row's tint keeps inside the card's rounded bottom corners.
        let bottom = g.bounds.top() + px(code::CARD_TITLE) + g.line_height * 12.0 - g.scroll.y;
        // The corner pixel itself: wholly outside a curve as small as the dark-panel chore's.
        let corner = pixel_at(&shot, dims, f32::from(left) + 0.5, f32::from(bottom) - 0.5);
        assert!(
            live_near(corner, bg, 4),
            "{n}: outside the bottom-left corner's arc {corner:?}, not {bg:?}"
        );
        let y = g.bounds.top() + px(code::CARD_TITLE) + g.line_height * 2.5 - g.scroll.y;
        assert!(
            live_near(
                pixel_at(&shot, dims, f32::from(left) + 0.5, f32::from(y)),
                ui.divider,
                12
            ),
            "{n}: its card's border"
        );
        println!("ui-shots: {n} ok");
    }
}

/// A unified diff of a file whose line `at` (0-based) went from `old` to `new`, the lines `before`
/// and `after` it unchanged; and the new file's text.
fn words_diff(before: &[&str], old: &str, new: &str, after: &[&str]) -> (String, String) {
    let mut u = format!(
        "diff --git a/words.rs b/words.rs\n--- a/words.rs\n+++ b/words.rs\n@@ -1,{n} +1,{n} @@\n",
        n = before.len() + after.len() + 1
    );
    let mut text = String::new();
    for l in before {
        u.push_str(&format!(" {l}\n"));
        text.push_str(&format!("{l}\n"));
    }
    u.push_str(&format!("-{old}\n+{new}\n"));
    text.push_str(&format!("{new}\n"));
    for l in after {
        u.push_str(&format!(" {l}\n"));
        text.push_str(&format!("{l}\n"));
    }
    (text, u)
}

/// `file-diff-words` (both themes; M11ah Task 10): in a card, the changed word of a replaced
/// line has a box in its row's red / green at about 35 % over the row's 12 % tint with 2 pt
/// corners, the rest of the row only the light tint (`let token = old();` -> `fresh();`: `old`
/// and `fresh`). With Wrap on, a word far along a long line is marked on the screen row it is
/// drawn on and on no other.
fn file_diff_words_scenes() {
    use crate::theme::mix;
    use crate::ui_tokens::code;
    use crate::ui_tokens::{MARK_MIN, ensure_contrast};

    let (text, unified) = words_diff(
        &[
            "// Sessions, for the words' shots.",
            "fn refresh(&mut self) {",
        ],
        "    let token = old();",
        "    let token = fresh();",
        &["    self.keep(token);", "}"],
    );
    let path = live_file("words.rs", &text);
    let lead = "    let message = \"";
    let long = |word: &str| {
        format!(
            "{lead}{}{word} {}\";",
            "word ".repeat(40),
            "tail ".repeat(10)
        )
    };
    let (long_text, long_unified) = words_diff(
        &["// A long line, wrapped."],
        &long("stale"),
        &long("fresh"),
        &["}"],
    );
    let long_path = live_file("words-wrap.rs", &long_text);
    let dims = size(px(760.0), px(420.0));
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let bg = theme.background;
        let fit = |c: alacritty_terminal::vte::ansi::Rgb| ensure_contrast(c, bg, MARK_MIN);
        let (green, red) = (fit(theme.palette[2]), fit(theme.palette[1]));
        let tint = |c| mix(bg, c, 0.12);
        let word = |c| mix(tint(c), c, code::WORD_ALPHA);
        let n = format!("file-diff-words-{slug}");
        let tops: std::rc::Rc<std::cell::RefCell<Vec<gpui::Pixels>>> = std::rc::Rc::default();
        let keep = tops.clone();
        let (shot, g) = code_shot(&theme, &path, dims, |cx, window, tab| {
            cards_setup(cx, window, tab, &unified);
            cx.read(|cx| {
                let t = tab.read(cx);
                let d = t.diff_doc().expect("its rows");
                let w = d.words.as_ref().expect("in time");
                assert_eq!(w[&2].len(), 1);
                assert_eq!((w[&2][0].start, w[&2][0].end), (16, 19), "old");
                assert_eq!(w[&3].len(), 1);
                assert_eq!((w[&3][0].start, w[&3][0].end), (16, 21), "fresh");
                assert_eq!(w.len(), 2, "only the pair");
                *keep.borrow_mut() = (0..4).map(|r| t.line_top(r, cx).expect("shown")).collect();
            });
        });
        keep_shot(&n, &shot);
        let tops = tops.borrow().clone();
        let at = |x: f32, y: f32| pixel_at(&shot, dims, x, y);
        let cw = f32::from(g.cell_width);
        let x_of = |cell: f32| f32::from(g.bounds.left() + g.text_left) + cw * cell;
        let top_of = |row: usize| f32::from(g.bounds.top() + tops[row] - g.scroll.y);
        for (row, c, from, to) in [(2, red, 16.0, 19.0), (3, green, 16.0, 21.0)] {
            let y = top_of(row) + 1.0;
            let inside = at(x_of((from + to) / 2.0), y);
            assert!(
                live_near(inside, word(c), 5),
                "{n}: row {row}'s word {inside:?}, not {:?}",
                word(c)
            );
            let before = at(x_of(from - 3.0), y);
            assert!(
                live_near(before, tint(c), 5),
                "{n}: row {row} before the word {before:?}, not {:?}",
                tint(c)
            );
            let after = at(x_of(to + 3.0), y);
            assert!(
                live_near(after, tint(c), 5),
                "{n}: row {row} after the word {after:?}, not {:?}",
                tint(c)
            );
            // 2 pt corners: the box's very corner pixel is not fully its colour.
            let corner = at(x_of(from) + 0.1, y - 0.9);
            let d = |a: [u8; 3], b: alacritty_terminal::vte::ansi::Rgb| {
                a.iter()
                    .zip([b.r, b.g, b.b])
                    .map(|(a, b)| a.abs_diff(b) as u32)
                    .sum::<u32>()
            };
            assert!(
                d(corner, tint(c)) < d(corner, word(c)),
                "{n}: row {row}'s corner {corner:?} is rounded"
            );
        }
        println!("ui-shots: {n} ok");

        // Wrap on: the word far along the long line, on the screen row it is drawn on.
        let n = format!("file-diff-words-wrap-{slug}");
        let rows: std::rc::Rc<std::cell::RefCell<(usize, gpui::Pixels)>> = std::rc::Rc::default();
        let keep = rows.clone();
        let (shot, g) = code_shot(&theme, &long_path, dims, |cx, window, tab| {
            cx.update(|cx| cx.global_mut::<crate::appearance::Appearance>().file_wrap = true);
            cards_setup(cx, window, tab, &long_unified);
            cx.read(|cx| {
                let t = tab.read(cx);
                let d = t.diff_doc().expect("its rows");
                let w = d.words.as_ref().expect("in time");
                let cell = lead.len() + "word ".repeat(40).len();
                assert_eq!(w[&1].len(), 1);
                assert_eq!((w[&1][0].start, w[&1][0].end), (cell, cell + 5), "stale");
                let removed = t.line_top(1, cx).expect("shown");
                let added = t.line_top(2, cx).expect("shown");
                let lh = t.geom().expect("drawn").line_height;
                let screen_rows = ((added - removed) / lh).round() as usize;
                assert!(screen_rows >= 3, "wrapped to {screen_rows} rows");
                *keep.borrow_mut() = (cell, removed);
            });
        });
        keep_shot(&n, &shot);
        let (cell, removed) = *rows.borrow();
        let lh = f32::from(g.line_height);
        let y0 = f32::from(g.bounds.top() + removed - g.scroll.y);
        let colour = word(red);
        let mut hits = Vec::new();
        for k in 0..4 {
            let y = y0 + lh * k as f32 + 1.0;
            let first = (0..((f32::from(g.bounds.size.width) - f32::from(g.text_left)) as i32))
                .find(|dx| {
                    live_near(
                        pixel_at(&shot, dims, x_of_left(&g) + *dx as f32, y),
                        colour,
                        5,
                    )
                });
            if let Some(dx) = first {
                hits.push((k, dx as f32 / f32::from(g.cell_width)));
            }
        }
        assert_eq!(hits.len(), 1, "{n}: the word on one screen row: {hits:?}");
        let (k, in_row) = hits[0];
        assert!(k > 0, "{n}: not on the line's first screen row");
        let columns = (cell as f32 - in_row.round()) / k as f32;
        assert!(
            columns.fract().abs() < 0.01 && columns > 20.0,
            "{n}: the word's cell {cell} on row {k} at {in_row}: columns {columns}"
        );
        println!("ui-shots: {n} ok");
    }
}

/// The text's left edge in points.
fn x_of_left(g: &crate::file_tab::Geom) -> f32 {
    f32::from(g.bounds.left() + g.text_left)
}

/// `file-diff-cards-fold` (M11ah Task 9): pressing a folded stretch shows it; pressing a title
/// closes the card to its title and opens it again; Show All Unchanged Lines shows every
/// stretch; leaving Diff and coming back folds them again; a new diff keeps a shown stretch that
/// starts on the same new line and folds one that no longer does; a card taller than the view
/// keeps its title pinned until it scrolls away, a press on the pinned title closing it; ⌥↓
/// goes card to card (the title at the top, its first changed row current) and stops at the
/// last; a change bar's press in File mode opens Diff at its card; a find step into a folded
/// stretch opens it; the lane has one mark per card.
fn file_diff_cards_fold_scenes() {
    use crate::file_tab::ShowAllUnchanged;
    use crate::git_status::LineChange;
    use crate::layout_model::FileMode;
    use crate::row_map::Row;
    use crate::ui_tokens::code;
    use gpui::{Focusable, Modifiers};

    let (text, unified) = cards_fixture();
    let path = live_file("cards-fold.rs", &text);
    let dims = size(px(760.0), px(460.0));
    let n = "file-diff-cards-fold";
    let theme = shot_theme(shot_themes()[0].1);
    let (shot, _) = code_shot(&theme, &path, dims, |cx, window, tab| {
        cards_setup(cx, window, tab, &unified);
        let folds =
            |cx: &mut VisualTestAppContext| cx.read(|cx| tab.read(cx).diff_folds_for_shots());
        let geom =
            |cx: &mut VisualTestAppContext| cx.read(|cx| tab.read(cx).geom()).expect("drawn");
        let scroll_to = |cx: &mut VisualTestAppContext, y: Pixels| {
            cx.update(|cx| tab.update(cx, |t, cx| t.set_scroll(point(px(0.0), y), cx)));
            live_frame(cx, window);
        };
        // A press at `top` (down the text) + `dy`, in the body's middle.
        let press = |cx: &mut VisualTestAppContext, top: Pixels, dy: Pixels| {
            let g = geom(cx);
            let p = point(
                g.bounds.left() + g.bounds.size.width / 2.0,
                g.bounds.top() + top - g.scroll.y + dy,
            );
            cx.simulate_click(window, p, Modifiers::none());
            live_frame(cx, window);
        };
        let lh = geom(cx).line_height;
        // The stretch above card 0 (rows 0..16, from new line 1): pressed, it shows.
        scroll_to(cx, px(0.0));
        assert_eq!(
            cx.read(|cx| tab.read(cx).row_kind_at(1.0, cx)),
            Some(Row::Unchanged { stretch: 0 }),
            "{n}: the first stretch folded"
        );
        press(cx, px(0.0), lh / 2.0);
        assert_eq!(folds(cx), (vec![1], vec![]), "{n}: the stretch shown");
        assert_eq!(
            cx.read(|cx| tab.read(cx).line_top(3, cx)),
            Some(lh * 3.0),
            "{n}: its lines drawn"
        );
        // Card 0's title: closed, then open again.
        let title0 = cx.read(|cx| tab.read(cx).title_top(0, cx)).expect("card 0");
        press(cx, title0, px(code::CARD_TITLE / 2.0));
        assert_eq!(
            folds(cx).1,
            vec![17],
            "{n}: card 0 closed (by its first new line)"
        );
        assert_eq!(
            cx.read(|cx| tab.read(cx).line_top(18, cx)),
            None,
            "{n}: its rows hidden"
        );
        press(cx, title0, px(code::CARD_TITLE / 2.0));
        assert!(folds(cx).1.is_empty(), "{n}: card 0 open again");
        assert!(cx.read(|cx| tab.read(cx).line_top(18, cx)).is_some());
        // Show All Unchanged Lines.
        cx.update_window(window, |_, window, cx| {
            tab.read(cx).focus_handle(cx).focus(window, cx);
            window.dispatch_action(Box::new(ShowAllUnchanged), cx);
        })
        .expect("the action");
        live_frame(cx, window);
        assert_eq!(folds(cx).0, vec![1, 24, 104], "{n}: every stretch shown");
        // Leaving Diff and coming back: folded again.
        cx.update(|cx| tab.update(cx, |t, cx| t.choose_mode_for_shots(FileMode::Plain, cx)));
        live_frame(cx, window);
        cx.update(|cx| tab.update(cx, |t, cx| t.choose_mode_for_shots(FileMode::Diff, cx)));
        live_frame(cx, window);
        assert_eq!(folds(cx), (vec![], vec![]), "{n}: folded again");
        // A new diff: the stretch from new line 24 kept (the same start), one gone folded.
        cx.update(|cx| tab.update(cx, |t, cx| t.show_stretch_for_shots(1, cx)));
        cx.update(|cx| tab.update(cx, |t, cx| t.show_stretch_for_shots(0, cx)));
        assert_eq!(folds(cx).0, vec![1, 24], "{n}: two shown");
        // The same diff with line 3 changed too: the first stretch is gone (card 0 grows up to
        // row 0), the one from new line 24 stays.
        let reread = unified.replacen(
            " fn setup() {\n",
            "-fn setup() {\n+fn setup() { // again\n",
            1,
        );
        cx.update(|cx| tab.update(cx, |t, cx| t.set_diff_for_shots(&reread, cx)));
        live_frame(cx, window);
        assert_eq!(
            folds(cx).0,
            vec![24],
            "{n}: the stretch on the same line kept"
        );
        // Back to the first diff, all folded.
        cx.update(|cx| tab.update(cx, |t, cx| t.choose_mode_for_shots(FileMode::Plain, cx)));
        cx.update(|cx| tab.update(cx, |t, cx| t.choose_mode_for_shots(FileMode::Diff, cx)));
        cx.update(|cx| tab.update(cx, |t, cx| t.set_diff_for_shots(&unified, cx)));
        live_frame(cx, window);
        // Card 1 is taller than the view: its title pinned while the view is inside it (the
        // stretch after it shown, so the view can go past it).
        cx.update(|cx| tab.update(cx, |t, cx| t.show_stretch_for_shots(2, cx)));
        live_frame(cx, window);
        let title1 = cx.read(|cx| tab.read(cx).title_top(1, cx)).expect("card 1");
        scroll_to(cx, title1 + lh * 10.0);
        assert_eq!(
            cx.read(|cx| tab.read(cx).sticky_title()),
            Some(1),
            "{n}: pinned"
        );
        scroll_to(cx, title1);
        assert_eq!(
            cx.read(|cx| tab.read(cx).sticky_title()),
            None,
            "{n}: at its own row"
        );
        let end = cx
            .read(|cx| tab.read(cx).line_top(103, cx))
            .expect("card 1's last row");
        scroll_to(cx, end + lh * 1.5);
        assert_eq!(
            cx.read(|cx| tab.read(cx).sticky_title()),
            None,
            "{n}: scrolled away"
        );
        // Fix round 1: the card's bottom less than a title's height below the top: not pinned
        // (the title would hang over what follows the card).
        scroll_to(cx, end + lh - px(code::CARD_TITLE / 2.0));
        assert_eq!(
            cx.read(|cx| tab.read(cx).sticky_title()),
            None,
            "{n}: not pinned past its card's end"
        );
        // A press on the pinned title closes the card, its title at the top.
        scroll_to(cx, title1 + lh * 10.0);
        let top = geom(cx).scroll.y;
        press(cx, top, px(code::CARD_TITLE / 2.0));
        assert_eq!(folds(cx).1, vec![68], "{n}: the pinned title's card closed");
        assert_eq!(geom(cx).scroll.y, title1, "{n}: its title at the top");
        press(cx, title1, px(code::CARD_TITLE / 2.0));
        assert!(folds(cx).1.is_empty(), "{n}: open again");
        // ⌥↓ from the top: card 0, card 1, then nothing.
        scroll_to(cx, px(0.0));
        cx.update(|cx| tab.update(cx, |t, cx| t.set_current_diff_for_shots(None, cx)));
        cx.update_window(window, |_, window, cx| {
            tab.read(cx).focus_handle(cx).focus(window, cx)
        })
        .expect("focus");
        for (card, row) in [(0, 19), (1, 71)] {
            cx.simulate_keystrokes(window, "alt-down");
            live_frame(cx, window);
            let title = cx
                .read(|cx| tab.read(cx).title_top(card, cx))
                .expect("a card");
            let g = geom(cx);
            let max = g.max_scroll.y;
            assert_eq!(
                g.scroll.y,
                title.min(max),
                "{n}: card {card}'s title at the top"
            );
            assert_eq!(
                cx.read(|cx| tab.read(cx).diff_current()),
                Some(row),
                "{n}: card {card}'s first change current"
            );
        }
        let before = geom(cx).scroll;
        cx.simulate_keystrokes(window, "alt-down");
        live_frame(cx, window);
        assert_eq!(geom(cx).scroll, before, "{n}: no wrap-around");
        assert_eq!(cx.read(|cx| tab.read(cx).diff_current()), Some(71));
        cx.simulate_keystrokes(window, "alt-up");
        live_frame(cx, window);
        assert_eq!(
            cx.read(|cx| tab.read(cx).diff_current()),
            Some(19),
            "{n}: ⌥↑ back"
        );
        // The lane: one mark per card.
        assert_eq!(
            cx.read(|cx| tab.read(cx).lane_for_shots().len()),
            2,
            "{n}: two marks"
        );
        // A find step into the first stretch opens it.
        cx.simulate_keystrokes(window, "cmd-f");
        cx.simulate_input(window, "needle");
        live_frame(cx, window);
        cx.simulate_keystrokes(window, "cmd-g");
        live_frame(cx, window);
        assert!(folds(cx).0.contains(&1), "{n}: the find opened its stretch");
        assert!(
            cx.read(|cx| tab.read(cx).line_top(4, cx)).is_some(),
            "{n}: the match's line drawn"
        );
        // A change bar's press in File mode: Diff at its card.
        cx.simulate_keystrokes(window, "escape");
        cx.update(|cx| tab.update(cx, |t, cx| t.choose_mode_for_shots(FileMode::Plain, cx)));
        cx.update(|cx| tab.update(cx, |t, cx| t.set_changes(vec![(72, LineChange::Added)], cx)));
        live_frame(cx, window);
        let line71 = cx
            .read(|cx| tab.read(cx).line_top(71, cx))
            .expect("line 72");
        cx.update(|cx| {
            tab.update(cx, |t, cx| {
                t.set_scroll(point(px(0.0), line71 - lh * 2.0), cx)
            })
        });
        live_frame(cx, window);
        let g = geom(cx);
        let bar = point(
            g.bounds.left() + g.bar_left + px(1.5),
            g.bounds.top() + line71 - g.scroll.y + lh / 2.0,
        );
        cx.simulate_click(window, bar, Modifiers::none());
        live_frame(cx, window);
        assert_eq!(
            cx.read(|cx| tab.read(cx).mode()),
            FileMode::Diff,
            "{n}: a bar's press"
        );
        let title1 = cx.read(|cx| tab.read(cx).title_top(1, cx)).expect("card 1");
        let g = geom(cx);
        assert_eq!(g.scroll.y, title1.min(g.max_scroll.y), "{n}: at its card");

        // Fix round 1: pinned inside card 1, then card 0's bar in File mode: Diff at card 0,
        // nothing pinned.
        cx.update(|cx| tab.update(cx, |t, cx| t.show_stretch_for_shots(2, cx)));
        live_frame(cx, window);
        let title1 = cx.read(|cx| tab.read(cx).title_top(1, cx)).expect("card 1");
        scroll_to(cx, title1 + lh * 10.0);
        assert_eq!(
            cx.read(|cx| tab.read(cx).sticky_title()),
            Some(1),
            "{n}: pinned"
        );
        cx.update(|cx| tab.update(cx, |t, cx| t.choose_mode_for_shots(FileMode::Plain, cx)));
        cx.update(|cx| {
            tab.update(cx, |t, cx| {
                t.set_changes(vec![(20, LineChange::Changed)], cx);
            })
        });
        live_frame(cx, window);
        let line19 = cx
            .read(|cx| tab.read(cx).line_top(19, cx))
            .expect("line 20");
        scroll_to(cx, line19 - lh * 2.0);
        let g = geom(cx);
        let bar = point(
            g.bounds.left() + g.bar_left + px(1.5),
            g.bounds.top() + line19 - g.scroll.y + lh / 2.0,
        );
        cx.simulate_click(window, bar, Modifiers::none());
        live_frame(cx, window);
        assert_eq!(
            cx.read(|cx| tab.read(cx).mode()),
            FileMode::Diff,
            "{n}: card 0's bar"
        );
        let title0 = cx.read(|cx| tab.read(cx).title_top(0, cx)).expect("card 0");
        assert_eq!(geom(cx).scroll.y, title0, "{n}: card 0's bar: at card 0");
        assert_eq!(
            cx.read(|cx| tab.read(cx).sticky_title()),
            None,
            "{n}: nothing pinned at card 0's title"
        );
        // Pinned deep in `fn refresh`'s card, a new diff adds a card above it: the pin names
        // the card under the top, `fn refresh`'s.
        cx.update(|cx| tab.update(cx, |t, cx| t.show_stretch_for_shots(2, cx)));
        live_frame(cx, window);
        let title1 = cx.read(|cx| tab.read(cx).title_top(1, cx)).expect("card 1");
        scroll_to(cx, title1 + lh * 20.0);
        assert_eq!(
            cx.read(|cx| tab.read(cx).sticky_title()),
            Some(1),
            "{n}: pinned"
        );
        let more = unified.replacen(
            "\n     let a40 = 40;\n",
            "\n-    let a40 = 40;\n+    let a40 = 4000;\n",
            1,
        );
        assert_ne!(more, unified, "{n}: the second change");
        cx.update(|cx| tab.update(cx, |t, cx| t.set_diff_for_shots(&more, cx)));
        live_frame(cx, window);
        let pinned = cx.read(|cx| {
            let t = tab.read(cx);
            let titles = &t.diff_doc().expect("rows").titles;
            assert_eq!(titles.len(), 3, "{n}: a third card");
            t.sticky_title().map(|k| titles[k].name.clone())
        });
        assert_eq!(
            pinned.as_deref(),
            Some("fn refresh(&mut self)"),
            "{n}: the pin follows its card"
        );

        // Fix round 1: Show All Unchanged Lines keeps the line at the top where it is.
        cx.update(|cx| tab.update(cx, |t, cx| t.set_diff_for_shots(&unified, cx)));
        cx.update(|cx| tab.update(cx, |t, cx| t.choose_mode_for_shots(FileMode::Plain, cx)));
        cx.update(|cx| tab.update(cx, |t, cx| t.choose_mode_for_shots(FileMode::Diff, cx)));
        live_frame(cx, window);
        let line80 = cx
            .read(|cx| tab.read(cx).line_top(80, cx))
            .expect("line 81");
        scroll_to(cx, line80);
        assert_eq!(geom(cx).scroll.y, line80, "{n}: line 81 at the top");
        cx.update_window(window, |_, window, cx| {
            window.dispatch_action(Box::new(ShowAllUnchanged), cx);
        })
        .expect("the action");
        live_frame(cx, window);
        assert_eq!(folds(cx).0, vec![1, 24, 104], "{n}: every stretch shown");
        assert_eq!(
            cx.read(|cx| tab.read(cx).line_top(80, cx)),
            Some(geom(cx).scroll.y),
            "{n}: Show All keeps the line at the top"
        );

        // Final review: a find step deep into the last folded stretch opens it and the match
        // lands on screen, not cut short by the shorter rows' limit.
        cx.update(|cx| tab.update(cx, |t, cx| t.choose_mode_for_shots(FileMode::Plain, cx)));
        cx.update(|cx| tab.update(cx, |t, cx| t.choose_mode_for_shots(FileMode::Diff, cx)));
        live_frame(cx, window);
        scroll_to(cx, px(0.0));
        assert_eq!(folds(cx), (vec![], vec![]), "{n}: all folded");
        cx.simulate_keystrokes(window, "cmd-f");
        cx.simulate_input(window, "b90 ");
        live_frame(cx, window);
        assert_eq!(cx.read(|cx| tab.read(cx).find_label()), "1 of 1", "{n}");
        assert!(
            folds(cx).0.contains(&104),
            "{n}: the find opened its stretch"
        );
        let row = cx.read(|cx| {
            let doc = &tab.read(cx).diff_doc().expect("rows").doc;
            (0..doc.lines.len())
                .find(|&r| doc.line(r).contains("b90 "))
                .expect("the match's row")
        });
        let top = cx
            .read(|cx| tab.read(cx).line_top(row, cx))
            .expect("the match's row drawn");
        let g = geom(cx);
        assert!(
            top >= g.scroll.y && top + lh <= g.scroll.y + g.bounds.size.height,
            "{n}: the match on screen: its top {top:?}, the scroll {:?}, the view {:?}",
            g.scroll.y,
            g.bounds.size.height
        );
        cx.simulate_keystrokes(window, "escape");
    });
    keep_shot(n, &shot);
    println!("ui-shots: {n} ok");
}

/// The side-by-side scenes' diff: 12 unchanged lines, `fn refresh`, two lines replaced by three,
/// two more, 12 unchanged: one card between two folded stretches. Its rows: 0 the comment,
/// 1–12, 13 `fn refresh`, 14–15 removed, 16–18 added, 19 `self.keep(token);`, 20 `}`, 21–32.
fn side_fixture() -> (String, String) {
    let mut before: Vec<String> = vec!["// Sessions, for the side-by-side shots.".into()];
    before.extend((1..=12).map(|k| format!("    let a{k} = {k};")));
    before.push("fn refresh(&mut self) {".into());
    let old = ["    let token = old();", "    let count = old_count();"];
    let new = [
        "    let token = fresh(count);",
        "    let count = fresh_count();",
        "    self.count += 1;",
    ];
    let mut after: Vec<String> = vec!["    self.keep(token);".into(), "}".into()];
    after.extend((1..=12).map(|k| format!("    let b{k} = {k};")));
    let mut u = format!(
        "diff --git a/side.rs b/side.rs\n--- a/side.rs\n+++ b/side.rs\n@@ -1,{} +1,{} @@\n",
        before.len() + old.len() + after.len(),
        before.len() + new.len() + after.len()
    );
    let mut text = String::new();
    for l in &before {
        u.push_str(&format!(" {l}\n"));
        text.push_str(&format!("{l}\n"));
    }
    for l in old {
        u.push_str(&format!("-{l}\n"));
    }
    for l in new {
        u.push_str(&format!("+{l}\n"));
        text.push_str(&format!("{l}\n"));
    }
    for l in &after {
        u.push_str(&format!(" {l}\n"));
        text.push_str(&format!("{l}\n"));
    }
    (text, u)
}

/// `file-diff-side` (both themes; M11ah Task 11): side by side on, the old file on the left
/// with its numbers and the new on the right with its, a 1 pt `divider` hairline between them
/// on the lines' rows (not across a title); the removed row level with its added one (red tint
/// left, green right); the extra added row facing a faintly shaded empty left half; an unchanged
/// row plain on both; each changed word marked on its half; the `⋯` row's words centred across
/// both halves; a find match on an unchanged line on both halves; the button filled.
fn file_diff_side_scenes() {
    use crate::row_map::Row;
    use crate::theme::mix;
    use crate::ui_tokens::code;
    use crate::ui_tokens::{MARK_MIN, ensure_contrast};
    use gpui::{Bounds, Focusable};

    let (text, unified) = side_fixture();
    let path = live_file("side.rs", &text);
    let dims = size(px(1240.0), px(620.0));
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let (bg, ui) = (theme.background, theme.ui());
        let fit = |c: alacritty_terminal::vte::ansi::Rgb| ensure_contrast(c, bg, MARK_MIN);
        let (green, red) = (fit(theme.palette[2]), fit(theme.palette[1]));
        let tint = |c| mix(bg, c, 0.12);
        let word = |c| mix(tint(c), c, code::WORD_ALPHA);
        let n = format!("file-diff-side-{slug}");
        type Seen = (
            Vec<Pixels>,
            (usize, usize),
            (usize, usize),
            Option<Bounds<Pixels>>,
        );
        let seen: std::rc::Rc<std::cell::RefCell<Option<Seen>>> = std::rc::Rc::default();
        let keep = seen.clone();
        let (shot, g) = code_shot(&theme, &path, dims, |cx, window, tab| {
            cards_setup(cx, window, tab, &unified);
            // On, as the bar's button turns it.
            let (_, _, _, button) = cx.read(|cx| tab.read(cx).side_button_for_shots());
            let button = button.expect("the button drawn in Diff mode");
            cx.simulate_click(window, button.center(), gpui::Modifiers::none());
            live_frame(cx, window);
            // A find match on an unchanged line.
            cx.update_window(window, |_, window, cx| {
                tab.read(cx).focus_handle(cx).focus(window, cx)
            })
            .expect("focus");
            cx.simulate_keystrokes(window, "cmd-f");
            cx.simulate_input(window, "keep");
            live_frame(cx, window);
            live_frame(cx, window);
            cx.read(|cx| {
                let t = tab.read(cx);
                let (chosen, on, _, _) = t.side_by_side_for_shots();
                assert!(chosen && on, "side by side on");
                assert_eq!(t.find_label(), "1 of 1");
                let d = t.diff_doc().expect("its rows");
                let w = d.words.as_ref().expect("in time");
                let (old, new) = (w[&14][0].clone(), w[&16][0].clone());
                let tops = (0..=21).map(|l| t.line_top(l, cx).unwrap_or_default());
                let (_, _, _, button) = t.side_button_for_shots();
                *keep.borrow_mut() = Some((
                    tops.collect(),
                    (old.start, old.end),
                    (new.start, new.end),
                    button,
                ));
                // The extra added row faces an empty old half.
                let map_row = (0..40).find(|&r| {
                    matches!(
                        t.row_for_shots(r),
                        Some(Row::Pair {
                            new: Some((18, 0)),
                            ..
                        })
                    )
                });
                let row = map_row.and_then(|r| t.row_for_shots(r));
                assert_eq!(
                    row,
                    Some(Row::Pair {
                        old: None,
                        new: Some((18, 0))
                    }),
                    "line 18 faces an empty half"
                );
            });
        });
        keep_shot(&n, &shot);
        let (tops, old_word, new_word, button) = seen.borrow().clone().expect("read");
        let shift = g.side.unwrap_or_else(|| panic!("{n}: drawn side by side"));
        let at = |x: Pixels, y: Pixels| pixel_at(&shot, dims, f32::from(x), f32::from(y));
        let left = g.bounds.left();
        let mid = left + px(code::CARD_INSET) + shift;
        let lh = g.line_height;
        let y_of = |line: usize| g.bounds.top() + tops[line] - g.scroll.y + lh / 2.0;
        let far = |a: [u8; 3], b: alacritty_terminal::vte::ansi::Rgb| {
            a.iter()
                .zip([b.r, b.g, b.b])
                .map(|(a, b)| u32::from(a.abs_diff(b)))
                .sum::<u32>()
        };
        let near = |p: [u8; 3], c, what: &str| {
            assert!(live_near(p, c, 5), "{n}: {what}: {p:?}, not {c:?}");
        };
        // The hairline, on an unchanged line's row and on a changed one's; not on the title.
        near(
            at(mid + px(0.5), y_of(19)),
            ui.divider,
            "the hairline (line 19)",
        );
        near(
            at(mid + px(0.5), y_of(14)),
            ui.divider,
            "the hairline (line 14)",
        );
        let title = g.bounds.top() + tops[11] - g.scroll.y - px(code::CARD_TITLE / 2.0);
        near(at(mid + px(0.5), title), bg, "no hairline across the title");
        // Fix round 1: the card's title spans both halves: its name and counts on the left,
        // `lines a–b` at the panel's right edge, past the middle.
        let title_ink = |x0: Pixels, x1: Pixels| {
            let b = Bounds::from_corners(
                gpui::point(x0, title - px(6.0)),
                gpui::point(x1, title + px(6.0)),
            );
            far(ink_in(&shot, dims, b, [bg.r, bg.g, bg.b]), bg) > 120
        };
        let panel_right = g.bounds.right() - px(code::CARD_INSET + 4.0);
        assert!(
            title_ink(left + px(code::CARD_INSET + 4.0), mid - px(40.0)),
            "{n}: the title's name on the left"
        );
        assert!(
            title_ink(mid + px(40.0), panel_right),
            "{n}: the title's lines at the right edge, across the new half"
        );
        // Level: the removed row red on the left, its added row green on the right.
        let right_end = g.bounds.right() - px(code::CARD_INSET + 20.0);
        near(at(mid - px(20.0), y_of(14)), tint(red), "removed, left");
        near(at(right_end, y_of(14)), tint(green), "added, right");
        // The extra added row: a faintly shaded empty left half.
        let empty = mix(bg, ui.text, code::SIDE_EMPTY_ALPHA);
        let shade = at(mid - px(20.0), y_of(18));
        near(shade, empty, "the empty half");
        assert!(
            far(shade, bg) >= 3,
            "{n}: the empty half is shaded: {shade:?}"
        );
        near(at(right_end, y_of(18)), tint(green), "the extra added row");
        // An unchanged row: plain on both.
        near(at(mid - px(20.0), y_of(19)), bg, "unchanged, left");
        near(at(right_end, y_of(19)), bg, "unchanged, right");
        // Numbers: the old on the left, the new on the right.
        let numbers = |dx: Pixels, line: usize, base: alacritty_terminal::vte::ansi::Rgb| {
            let x0 = left + px(code::CARD_INSET + code::GUTTER_LEFT) + dx;
            let b = Bounds::from_corners(
                gpui::point(x0, y_of(line) - lh / 2.0 + px(2.0)),
                gpui::point(left + g.numbers_right + dx, y_of(line) + lh / 2.0 - px(2.0)),
            );
            far(ink_in(&shot, dims, b, [base.r, base.g, base.b]), base) > 120
        };
        assert!(
            numbers(px(0.0), 14, tint(red)),
            "{n}: line 14's old number on the left"
        );
        assert!(
            numbers(shift, 14, tint(green)),
            "{n}: line 16's new number level with it on the right"
        );
        assert!(
            numbers(shift, 18, tint(green)),
            "{n}: line 18's new number on the right"
        );
        assert!(
            !numbers(px(0.0), 18, empty),
            "{n}: no number on the empty half"
        );
        // The changed words, each on its own half.
        let cw = g.cell_width;
        let word_x = |dx: Pixels, (a, b): (usize, usize)| {
            left + g.text_left + dx + cw * ((a + b) as f32 / 2.0)
        };
        // Above the glyphs: a row's top + 1.5.
        let edge = |line: usize| y_of(line) - lh / 2.0 + px(1.5);
        near(
            at(word_x(px(0.0), old_word), edge(14)),
            word(red),
            "the old word",
        );
        near(
            at(word_x(shift, new_word), edge(16)),
            word(green),
            "the new word",
        );
        // The `⋯` row's words centred across both halves (the first row).
        let row0 = g.bounds.top() - g.scroll.y;
        let centre = Bounds::from_corners(
            gpui::point(mid - px(40.0), row0 + px(2.0)),
            gpui::point(mid + px(40.0), row0 + lh - px(2.0)),
        );
        assert!(
            far(ink_in(&shot, dims, centre, [bg.r, bg.g, bg.b]), bg) > 120,
            "{n}: the ⋯ row's words across the middle"
        );
        // The match on line 19, on both halves alike.
        // `keep` at cells 9–13 of `    self.keep(token);`.
        let x = |dx: Pixels| left + g.text_left + dx + cw * 11.0;
        let (l, r) = (at(x(px(0.0)), edge(19)), at(x(shift), edge(19)));
        assert!(far(l, bg) > 30, "{n}: the match on the left: {l:?}");
        assert!(
            live_near(r, as_rgb(l), 4),
            "{n}: the match on the right {r:?} as the left {l:?}"
        );
        // The button filled.
        let b = button.expect("drawn");
        near(
            at(b.left() + px(3.0), b.top() + px(12.0)),
            ui.card_selected,
            "the button filled",
        );
        println!("ui-shots: {n} ok");
    }
}

/// `file-diff-side-narrow` (M11ah Task 11): off by default; the button turns side by side on
/// (`FileTabEvent::SideBySide` for the workspace to keep per tab); below 60 cells a half the
/// view is unified, the button faint and deaf with the tooltip `Too narrow for side by side`;
/// wide again, side by side comes back. A drag stays in the half it started in and Copy gives
/// that half; the current row is on the half pressed (an empty half: that side's nearest
/// line); find steps row by row, left before right.
fn file_diff_side_narrow_scenes() {
    use crate::file_tab::{FileTab, FileTabEvent, TIP_SIDE_BY_SIDE, TIP_SIDE_TOO_NARROW};
    use crate::row_map::{Half, Row};
    use gpui::{Entity, Focusable, Modifiers, MouseButton};
    use std::cell::RefCell;
    use std::rc::Rc;

    /// The tab in a pane `width` wide (the window stays as it is).
    struct SideHost {
        tab: Entity<FileTab>,
        width: Pixels,
    }
    impl Render for SideHost {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
                .size_full()
                .flex()
                .flex_row()
                .child(div().h_full().w(self.width).flex().child(self.tab.clone()))
        }
    }

    let (text, unified) = side_fixture();
    let path = live_file("side-narrow.rs", &text);
    let dims = size(px(1240.0), px(620.0));
    let n = "file-diff-side-narrow";
    let theme = shot_theme(shot_themes()[0].1);
    type Slot = Rc<RefCell<Option<(Entity<FileTab>, Entity<SideHost>)>>>;
    let slot: Slot = Rc::default();
    let events: Rc<RefCell<Vec<bool>>> = Rc::default();
    let build = {
        let (slot, events) = (slot.clone(), events.clone());
        move |window: &mut Window, cx: &mut App| -> AnyView {
            cx.set_global(shot_appearance(theme, cx));
            cx.bind_keys(crate::key_bindings());
            let tab = cx.new(|cx| {
                let mut tab = FileTab::new(path, crate::layout_model::FileMode::Plain, window, cx);
                tab.frozen = true;
                tab
            });
            cx.subscribe(&tab, move |_, e: &FileTabEvent, _| {
                if let FileTabEvent::SideBySide(on) = e {
                    events.borrow_mut().push(*on);
                }
            })
            .detach();
            let host = cx.new(|_| SideHost {
                tab: tab.clone(),
                width: px(1240.0),
            });
            *slot.borrow_mut() = Some((tab, host.clone()));
            host.into()
        }
    };
    let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
        let (tab, host) = slot.borrow().clone().expect("the tab");
        live_frame(cx, window);
        cards_setup(cx, window, &tab, &unified);
        let look =
            |cx: &mut VisualTestAppContext| cx.read(|cx| tab.read(cx).side_by_side_for_shots());
        let button =
            |cx: &mut VisualTestAppContext| cx.read(|cx| tab.read(cx).side_button_for_shots());
        let width = |cx: &mut VisualTestAppContext, w: f32| {
            cx.update(|cx| {
                host.update(cx, |h, cx| {
                    h.width = px(w);
                    cx.notify();
                })
            });
            live_frame(cx, window);
            live_frame(cx, window);
        };
        // Off by default; the button takes a click.
        assert!(!look(cx).0, "{n}: off by default");
        let (filled, enabled, tip, at) = button(cx);
        assert_eq!(
            (filled, enabled, tip),
            (false, true, TIP_SIDE_BY_SIDE),
            "{n}"
        );
        cx.simulate_click(window, at.expect("drawn").center(), Modifiers::none());
        live_frame(cx, window);
        assert_eq!((look(cx).0, look(cx).1), (true, true), "{n}: on");
        assert_eq!(*events.borrow(), [true], "{n}: the workspace told");
        let pair = |cx: &mut VisualTestAppContext| {
            matches!(
                cx.read(|cx| tab.read(cx).row_for_shots(2)),
                Some(Row::Pair { .. })
            )
        };
        assert!(pair(cx), "{n}: paired rows");
        assert!(button(cx).0, "{n}: filled");
        // Too narrow: unified, the button faint and deaf.
        width(cx, 800.0);
        assert_eq!(
            (look(cx).0, look(cx).1),
            (true, false),
            "{n}: unified when narrow"
        );
        assert!(!pair(cx), "{n}: the rows unified");
        let (filled, enabled, tip, at) = button(cx);
        assert_eq!(
            (filled, enabled, tip),
            (false, false, TIP_SIDE_TOO_NARROW),
            "{n}"
        );
        cx.simulate_click(window, at.expect("drawn").center(), Modifiers::none());
        live_frame(cx, window);
        assert!(look(cx).0, "{n}: a press on the faint button does nothing");
        assert_eq!(events.borrow().len(), 1, "{n}: nothing told");
        // Wide again: back.
        width(cx, 1240.0);
        assert_eq!(
            (look(cx).0, look(cx).1),
            (true, true),
            "{n}: back when wide"
        );
        assert!(pair(cx), "{n}: paired again");
        // A drag in the right half, from line 13 to past line 19's end: Copy gives the new half.
        let g = cx.read(|cx| tab.read(cx).geom()).expect("drawn");
        let shift = g.side.expect("side by side");
        let top = |cx: &mut VisualTestAppContext, line: usize| {
            cx.read(|cx| tab.read(cx).line_top(line, cx))
                .expect("shown")
        };
        let at = |cx: &mut VisualTestAppContext, line: usize, dx: Pixels, cells: f32| {
            let g = cx.read(|cx| tab.read(cx).geom()).expect("drawn");
            point(
                g.bounds.left() + g.text_left + dx + g.cell_width * cells,
                g.bounds.top() + top(cx, line) - g.scroll.y + g.line_height / 2.0,
            )
        };
        let (from, to) = (at(cx, 13, shift, 0.1), at(cx, 19, shift, 40.0));
        cx.simulate_mouse_down(window, from, MouseButton::Left, Modifiers::none());
        cx.simulate_mouse_move(window, to, MouseButton::Left, Modifiers::none());
        cx.simulate_mouse_up(window, to, MouseButton::Left, Modifiers::none());
        live_frame(cx, window);
        cx.simulate_keystrokes(window, "cmd-c");
        let copied = cx.read_from_clipboard().and_then(|c| c.text());
        assert_eq!(
            copied.as_deref(),
            Some(
                "fn refresh(&mut self) {\n    let token = fresh(count);\n    let count = \
                 fresh_count();\n    self.count += 1;\n    self.keep(token);"
            ),
            "{n}: the new half copied"
        );
        let current = |cx: &mut VisualTestAppContext| {
            cx.read(|cx| {
                (
                    tab.read(cx).diff_current(),
                    tab.read(cx).side_by_side_for_shots().2,
                )
            })
        };
        assert_eq!(
            current(cx),
            (Some(19), Some(Half::New)),
            "{n}: current, right"
        );
        // A press on the left: line 13 current, on the left.
        let p = at(cx, 13, px(0.0), 2.0);
        cx.simulate_click(window, p, Modifiers::none());
        live_frame(cx, window);
        assert_eq!(
            current(cx),
            (Some(13), Some(Half::Old)),
            "{n}: current, left"
        );
        // A press on the empty left half facing line 18: the old side's line above it.
        let p = at(cx, 18, px(0.0), 2.0);
        cx.simulate_click(window, p, Modifiers::none());
        live_frame(cx, window);
        assert_eq!(
            current(cx),
            (Some(15), Some(Half::Old)),
            "{n}: the empty half's"
        );
        // Find: row by row, left before right (16 is level with 14, 15 with 17).
        cx.update_window(window, |_, window, cx| {
            tab.read(cx).focus_handle(cx).focus(window, cx)
        })
        .expect("focus");
        cx.simulate_keystrokes(window, "cmd-f");
        cx.simulate_input(window, "count");
        live_frame(cx, window);
        live_frame(cx, window);
        let mut steps = Vec::new();
        for _ in 0..6 {
            cx.simulate_keystrokes(window, "cmd-g");
            live_frame(cx, window);
            steps.push(current(cx));
        }
        let want = [
            (Some(16), Some(Half::New)),
            (Some(15), Some(Half::Old)),
            (Some(15), Some(Half::Old)),
            (Some(17), Some(Half::New)),
            (Some(17), Some(Half::New)),
            (Some(18), Some(Half::New)),
        ];
        assert!(
            (0..6).any(|k| (0..6).all(|i| steps[i] == want[(i + k) % 6])),
            "{n}: the steps {steps:?}, not in turn {want:?}"
        );
        println!("ui-shots: {n} ok");
    };
    let _ = render_with(&theme, dims, build, act);

    // Fix round 1: a change bar's press opens Diff side by side with its card's title at the
    // top, worked out for the layout drawn: (a) chosen (restored so) before any Diff drawing;
    // (b) the pane narrowed below the fit while in File mode (unified, then).
    let n = "file-diff-side-narrow-reveal";
    let (text, unified) = cards_fixture();
    let path = live_file("side-reveal.rs", &text);
    let slot: Slot = Rc::default();
    let build = {
        let slot = slot.clone();
        move |window: &mut Window, cx: &mut App| -> AnyView {
            cx.set_global(shot_appearance(theme, cx));
            cx.bind_keys(crate::key_bindings());
            let tab = cx.new(|cx| {
                let mut tab = FileTab::new(path, crate::layout_model::FileMode::Plain, window, cx);
                tab.frozen = true;
                tab
            });
            let host = cx.new(|_| SideHost {
                tab: tab.clone(),
                width: px(1240.0),
            });
            *slot.borrow_mut() = Some((tab, host.clone()));
            host.into()
        }
    };
    let act = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
        use crate::git_status::LineChange;
        use crate::layout_model::FileMode;
        let (tab, host) = slot.borrow().clone().expect("the tab");
        live_frame(cx, window);
        cx.update(|cx| {
            tab.update(cx, |t, cx| {
                t.pretend_in_repo_for_shots(cx);
                t.set_side_by_side_for_shots(true);
                t.set_diff_for_shots(&unified, cx);
                t.set_changes(vec![(72, LineChange::Added)], cx);
            })
        });
        live_frame(cx, window);
        // Card 1's added line 72 (0-based 71): its bar pressed in File mode.
        let press_bar = |cx: &mut VisualTestAppContext| {
            let top = cx
                .read(|cx| tab.read(cx).line_top(71, cx))
                .expect("line 72");
            let lh = cx
                .read(|cx| tab.read(cx).geom())
                .expect("drawn")
                .line_height;
            cx.update(|cx| {
                tab.update(cx, |t, cx| t.set_scroll(point(px(0.0), top - lh * 2.0), cx))
            });
            live_frame(cx, window);
            let g = cx.read(|cx| tab.read(cx).geom()).expect("drawn");
            let bar = point(
                g.bounds.left() + g.bar_left + px(1.5),
                g.bounds.top() + top - g.scroll.y + g.line_height / 2.0,
            );
            cx.simulate_click(window, bar, Modifiers::none());
            live_frame(cx, window);
            live_frame(cx, window);
            assert_eq!(cx.read(|cx| tab.read(cx).mode()), FileMode::Diff, "{n}");
            let title = cx.read(|cx| tab.read(cx).title_top(1, cx)).expect("card 1");
            let g = cx.read(|cx| tab.read(cx).geom()).expect("drawn");
            (title, g.scroll.y, g.max_scroll.y)
        };
        let (title, scroll, max) = press_bar(cx);
        assert!(title < max, "{n}: card 1's title can reach the top");
        assert!(
            cx.read(|cx| tab.read(cx).side_by_side_for_shots().1),
            "{n}: side by side"
        );
        assert_eq!(scroll, title, "{n}: (a) card 1's title at the top");
        // (b) Back to File, the pane too narrow, the bar again: unified, at its title.
        cx.update(|cx| tab.update(cx, |t, cx| t.choose_mode_for_shots(FileMode::Plain, cx)));
        cx.update(|cx| {
            host.update(cx, |h, cx| {
                h.width = px(800.0);
                cx.notify();
            })
        });
        live_frame(cx, window);
        live_frame(cx, window);
        let (title, scroll, max) = press_bar(cx);
        assert!(title < max, "{n}: card 1's title can reach the top");
        assert!(
            !cx.read(|cx| tab.read(cx).side_by_side_for_shots().1),
            "{n}: unified"
        );
        assert_eq!(scroll, title, "{n}: (b) card 1's title at the top");
        println!("ui-shots: {n} ok");
    };
    let _ = render_with(&theme, dims, build, act);
}
