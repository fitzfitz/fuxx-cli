//! The terminal view: the cursor (M11f), a flood of output (M11n) and command blocks (M11l).

use super::*;

/// This file, as `file!()` gives it (repo-relative under cargo): `registry::AREAS` pairs it with
/// `SCENES`, so `--changed` runs these scenes when this file changes.
pub(super) const FILE: &str = file!();

/// This area's scenes (`registry.rs` runs them in its `ORDER`).
pub(super) const SCENES: &[Scene] = &[
    Scene {
        name: "cursor",
        run: cursor_scenes,
        themes: Themes::Both,
        // Serial: (c) the glide (90 ms), sparks (350 ms) and the breath's 600 ms hold, read from
        // Instant.
        serial: true,
        kind: Kind::Visual,
        sources: &[
            "src/terminal_view.rs",
            "src/terminal.rs",
            "src/grid.rs",
            "src/pty.rs",
            "src/cursor_look.rs",
            "src/appearance.rs",
            "src/status.rs",
        ],
    },
    Scene {
        name: "cursor-flood",
        run: cursor_flood_scene,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: &[
            "src/terminal_view.rs",
            "src/terminal.rs",
            "src/grid.rs",
            "src/pty.rs",
        ],
    },
    Scene {
        name: "command-blocks",
        run: command_blocks_scene,
        themes: Themes::Both,
        // Serial: (c) nothing animates over 200 ms of real time.
        serial: true,
        kind: Kind::Visual,
        sources: &[
            "src/terminal_view.rs",
            "src/terminal.rs",
            "src/grid.rs",
            "src/pty.rs",
            "src/blocks.rs",
            "src/block_view.rs",
            "src/shell_marks.rs",
            "src/reduce_motion.rs",
        ],
    },
    Scene {
        name: "shell-cards",
        run: shell_cards_scene,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/terminal_view.rs",
            "src/card_view.rs",
            "src/terminal.rs",
            "src/grid.rs",
            "src/blocks.rs",
        ],
    },
    Scene {
        name: "shell-cards-select",
        run: shell_cards_select_scene,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/terminal_view.rs",
            "src/card_view.rs",
            "src/terminal.rs",
            "src/grid.rs",
            "src/blocks.rs",
        ],
    },
    Scene {
        name: "shell-cards-running-full",
        run: shell_cards_running_full_scene,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/terminal_view.rs",
            "src/card_view.rs",
            "src/terminal.rs",
            "src/grid.rs",
            "src/blocks.rs",
        ],
    },
    Scene {
        name: "shell-cards-alt-screen",
        run: shell_cards_alt_screen_scene,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/terminal_view.rs",
            "src/card_view.rs",
            "src/terminal.rs",
            "src/grid.rs",
            "src/blocks.rs",
        ],
    },
    Scene {
        name: "shell-cards-off",
        run: shell_cards_off_scene,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/terminal_view.rs",
            "src/card_view.rs",
            "src/terminal.rs",
            "src/grid.rs",
            "src/blocks.rs",
        ],
    },
    Scene {
        name: "shell-cards-toggle",
        run: shell_cards_toggle_scene,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/terminal_view.rs",
            "src/card_view.rs",
            "src/terminal.rs",
            "src/grid.rs",
            "src/blocks.rs",
        ],
    },
    Scene {
        name: "command-blocks-alt-screen",
        run: command_blocks_alt_screen_scene,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/terminal_view.rs",
            "src/terminal.rs",
            "src/grid.rs",
            "src/pty.rs",
            "src/blocks.rs",
            "src/block_view.rs",
            "src/shell_marks.rs",
        ],
    },
    Scene {
        name: "command-blocks-narrow",
        run: command_blocks_narrow_scene,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/terminal_view.rs",
            "src/terminal.rs",
            "src/grid.rs",
            "src/pty.rs",
            "src/blocks.rs",
            "src/block_view.rs",
            "src/shell_marks.rs",
        ],
    },
    Scene {
        name: "command-blocks-rprompt",
        run: command_blocks_rprompt_scene,
        themes: Themes::One,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/terminal_view.rs",
            "src/terminal.rs",
            "src/grid.rs",
            "src/pty.rs",
            "src/blocks.rs",
            "src/block_view.rs",
            "src/shell_marks.rs",
        ],
    },
    Scene {
        name: "command-blocks-small-font",
        run: command_blocks_small_font_scene,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/terminal_view.rs",
            "src/terminal.rs",
            "src/grid.rs",
            "src/pty.rs",
            "src/blocks.rs",
            "src/block_view.rs",
            "src/shell_marks.rs",
        ],
    },
    Scene {
        name: "command-blocks-off",
        run: command_blocks_off_scene,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/terminal_view.rs",
            "src/terminal.rs",
            "src/grid.rs",
            "src/pty.rs",
            "src/blocks.rs",
            "src/block_view.rs",
            "src/shell_marks.rs",
            "src/appearance.rs",
        ],
    },
    Scene {
        name: "command-blocks-jump",
        run: command_blocks_jump_scene,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/terminal_view.rs",
            "src/terminal.rs",
            "src/grid.rs",
            "src/pty.rs",
            "src/blocks.rs",
            "src/block_view.rs",
            "src/shell_marks.rs",
        ],
    },
    Scene {
        name: "command-blocks-copy",
        run: command_blocks_copy_scene,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: &[
            "src/terminal_view.rs",
            "src/terminal.rs",
            "src/grid.rs",
            "src/pty.rs",
            "src/blocks.rs",
            "src/block_view.rs",
            "src/shell_marks.rs",
            "src/palette_items.rs",
        ],
    },
    Scene {
        name: "command-blocks-rerun",
        run: command_blocks_rerun_scene,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: &[
            "src/terminal_view.rs",
            "src/terminal.rs",
            "src/grid.rs",
            "src/pty.rs",
            "src/blocks.rs",
            "src/block_view.rs",
            "src/shell_marks.rs",
        ],
    },
];

/// One terminal in a hidden window for the `cursor` scene: a `cat` showing `text`, the
/// Appearance as `tweak` leaves it, motion off when `still`. `focus`: the terminal has the
/// focus; `key`: it counts its window as the key one (an off-screen window never is).
struct CursorRig {
    // Before `cx`: fields drop in order, and a handle must go before its app.
    view: gpui::Entity<crate::terminal_view::TerminalView>,
    cx: ShotApp,
    window: AnyWindowHandle,
    dims: Size<Pixels>,
}

impl CursorRig {
    fn new(
        theme: Theme,
        text: &str,
        focus: bool,
        key: bool,
        still: bool,
        tweak: impl FnOnce(&mut Appearance),
    ) -> Self {
        let dims = size(px(320.0), px(120.0));
        Self::sized(theme, text, dims, false, (focus, key, still), tweak)
    }

    /// `new` at `dims`, the pane laid out as one of a split when `split`; `(focus, key, still)`
    /// as `new` takes them.
    fn sized(
        theme: Theme,
        text: &str,
        dims: Size<Pixels>,
        split: bool,
        flags: (bool, bool, bool),
        tweak: impl FnOnce(&mut Appearance),
    ) -> Self {
        Self::spawned(
            theme,
            vec!["/bin/cat".into()],
            text,
            dims,
            split,
            flags,
            tweak,
        )
    }

    /// `sized`, running `program` instead of `cat`.
    fn spawned(
        theme: Theme,
        program: Vec<String>,
        text: &str,
        dims: Size<Pixels>,
        split: bool,
        (focus, key, still): (bool, bool, bool),
        tweak: impl FnOnce(&mut Appearance),
    ) -> Self {
        use crate::terminal_view::TerminalView;
        use gpui::Focusable;
        use std::cell::RefCell;
        use std::rc::Rc;

        let mut cx = ShotApp::new();
        let slot = Rc::new(RefCell::new(None));
        let text = text.to_string();
        let window = {
            let slot = slot.clone();
            cx.open_offscreen_window(dims, move |window, cx| {
                cx.set_reduce_motion(still);
                let mut appearance = shot_appearance(theme, cx);
                tweak(&mut appearance);
                cx.set_global(appearance);
                let quiet = crate::pty::ShellEnv {
                    base: None,
                    set: Vec::new(),
                    remove: Vec::new(),
                };
                let view = TerminalView::spawn(cx, Some(program), None, None, &quiet)
                    .expect("start the program");
                view.read(cx).show_text(&text);
                view.read(cx).set_split(split);
                if key {
                    view.read(cx).pretend_key_window();
                }
                if focus {
                    view.focus_handle(cx).focus(window, cx);
                }
                *slot.borrow_mut() = Some(view.clone());
                cx.new(|_| Root {
                    theme,
                    scene: view.into(),
                })
            })
            .expect("open an off-screen window")
        };
        cx.run_until_parked();
        let view = slot.borrow().clone().expect("the terminal");
        Self {
            view,
            cx,
            window: window.into(),
            dims,
        }
    }

    /// Draw a frame now and capture it.
    fn frame(&mut self) -> Shot {
        self.cx.run_until_parked();
        self.cx
            .update_window(self.window, |_, window, cx| window.draw(cx).clear())
            .expect("draw");
        let image = self
            .cx
            .capture_screenshot(self.window)
            .expect("capture the window");
        Shot {
            width: image.width(),
            height: image.height(),
            rgba: image.as_raw().to_vec(),
        }
    }

    /// Print `text` as the program would, and have the terminal redraw.
    fn write(&mut self, text: &str) {
        let view = self.view.clone();
        self.cx.update(|cx| {
            view.read(cx).show_text(text);
            view.update(cx, |_, cx| cx.notify());
        });
    }

    fn appearance(&mut self, f: impl FnOnce(&mut Appearance)) {
        self.cx
            .update(|cx| cx.update_global::<Appearance, _>(|a, _| f(a)));
    }

    /// Where cell (`col`, `row`) was last painted, in points.
    fn cell(&self, col: usize, row: usize) -> gpui::Bounds<Pixels> {
        self.cx
            .read(|cx| self.view.read(cx).cell_bounds(col, row))
            .expect("painted")
    }

    fn read<R>(&self, f: impl FnOnce(&crate::terminal_view::TerminalView) -> R) -> R {
        self.cx.read(|cx| f(self.view.read(cx)))
    }
}

/// Whether the pixel at (`x`, `y`) points is `c` (within 3 per channel).
fn is_colour(
    shot: &Shot,
    dims: Size<Pixels>,
    x: f32,
    y: f32,
    c: alacritty_terminal::vte::ansi::Rgb,
) -> bool {
    pixel_at(shot, dims, x, y)
        .iter()
        .zip([c.r, c.g, c.b])
        .all(|(a, b)| a.abs_diff(b) <= 3)
}

/// The terminal cursor (M11f), in both themes, one `cat` terminal per check, its cursor on
/// the blank cell (2, 1):
/// - `cursor-bar|underline|hollow|block`: each Style setting with no program style (glow off,
///   so the colour is exact): the bar is 2 px at the cell's left edge, the underline 2 px at
///   its bottom, the hollow a 1 px outline, the block a filled cell.
/// - `cursor-working`: a Working terminal (its status given by a `Workspace`) draws the cursor
///   in `ring_working`; back to Idle, the theme's cursor colour. A block in a status colour
///   writes its letter in the background colour.
/// - `cursor-glow`: the breathing glow around the bar, on a ~30 fps timer; after a program's
///   `ESC [ 2 q` (`cursor-steady-block`), a steady block with no glow and no frames.
/// - `cursor-unfocused`: an unfocused terminal (and a focused one in a window that isn't key):
///   a 1 px outline in the theme's cursor colour, whatever the status; no frames.
/// - 50 cursor moves before one frame leave one glide, from the old cell to the last.
/// - Sparks come from typed text only: not from output, arrows, ctrl-keys or Enter.
/// - `cursor-still`: with motion off, no glow, no glide (the bar is exactly on its new cell),
///   no sparks, and no frames.
fn cursor_scenes() {
    use crate::appearance::{CursorFun, CursorShapeSetting};
    use crate::cursor_look::Frames;

    let text = "\r\n$ ";
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let ui = theme.ui();
        let own = theme.cursor.unwrap_or(theme.foreground);
        let bg = theme.background;

        // Each shape, glow off so the colour is exact.
        for setting in CursorShapeSetting::ALL {
            let mut rig = CursorRig::new(theme, text, true, true, false, |a| {
                a.cursor_style = setting;
                a.cursor_glow = false;
            });
            let shot = rig.frame();
            let n = format!("cursor-{}-{slug}", setting.value());
            save_shot(&format!("cursor-{}", setting.value()), slug, &shot);
            let dims = rig.dims;
            let cell = rig.cell(2, 1);
            let scale = shot.width as f32 / f32::from(dims.width);
            let (left, right) = (f32::from(cell.left()), f32::from(cell.right()));
            let (top, bottom) = (f32::from(cell.top()), f32::from(cell.bottom()));
            let mid = (top + bottom) / 2.0;
            let across = |y: f32| count_along(&shot, dims, own, y, left - 4.0, right + 4.0);
            let two = (2.0 * scale).round() as usize;
            match setting {
                CursorShapeSetting::Bar => {
                    assert_eq!(across(mid), two, "{n}: the bar is 2 px wide");
                    assert!(
                        is_colour(&shot, dims, left + 0.5, mid, own),
                        "{n}: the bar is at the cell's left edge"
                    );
                    assert!(
                        is_colour(&shot, dims, left + 2.5, mid, bg),
                        "{n}: the bar stops after 2 px"
                    );
                }
                CursorShapeSetting::Underline => {
                    let wide = across(bottom - 1.0);
                    let cell_px = ((right - left) * scale).round() as usize;
                    assert!(
                        wide.abs_diff(cell_px) <= 2,
                        "{n}: the underline spans the cell ({wide} of {cell_px})"
                    );
                    assert_eq!(across(bottom - 3.0), 0, "{n}: the underline is 2 px tall");
                    assert_eq!(across(mid), 0, "{n}: nothing at the cell's middle");
                }
                CursorShapeSetting::Hollow => {
                    assert_eq!(across(mid), two, "{n}: a 1 px outline, left and right");
                    assert!(
                        is_colour(&shot, dims, (left + right) / 2.0, mid, bg),
                        "{n}: hollow inside"
                    );
                }
                CursorShapeSetting::Block => {
                    assert!(
                        is_colour(&shot, dims, (left + right) / 2.0, mid, own),
                        "{n}: the block fills the cell"
                    );
                }
            }
        }

        // The status colour, from a Workspace.
        {
            use std::cell::RefCell;
            use std::rc::Rc;
            let dims = size(px(640.0), px(400.0));
            let mut cx = ShotApp::new();
            let slot: Slot = Rc::new(RefCell::new(None));
            let window = {
                let slot = slot.clone();
                cx.open_offscreen_window(dims, move |window, cx| {
                    let mut appearance = shot_appearance(theme, cx);
                    appearance.cursor_glow = false;
                    cx.set_global(appearance);
                    let ws = cx.new(|cx| {
                        let mut ws = Workspace::for_shots(window, cx);
                        let project = std::path::Path::new("/Users/me/code/api");
                        ws.open_shot_session("api", project, text, None, window, cx);
                        ws.select(0, window, cx);
                        ws.focus_active(window, cx);
                        ws
                    });
                    *slot.borrow_mut() = Some(ws.clone());
                    cx.new(|_| Root {
                        theme,
                        scene: ws.into(),
                    })
                })
                .expect("open an off-screen window")
            };
            cx.run_until_parked();
            let ws = slot.borrow().clone().expect("the workspace");
            let view = cx.read(|cx| ws.read(cx).sessions.items()[0].view.clone());
            cx.read(|cx| view.read(cx).pretend_key_window());
            let frame = |cx: &mut VisualTestAppContext, status: Status| -> Shot {
                cx.update(|cx| ws.update(cx, |ws, cx| ws.set_status(0, status, false, cx)));
                cx.run_until_parked();
                cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear())
                    .expect("draw");
                let image = cx
                    .capture_screenshot(window.into())
                    .expect("capture the window");
                Shot {
                    width: image.width(),
                    height: image.height(),
                    rgba: image.as_raw().to_vec(),
                }
            };
            for (status, colour, label) in [
                (Status::Working, ui.ring_working, "working"),
                (Status::Idle, own, "idle"),
            ] {
                let shot = frame(&mut cx, status);
                if status == Status::Working {
                    save_shot("cursor-working", slug, &shot);
                }
                let cell = cx
                    .read(|cx| view.read(cx).cell_bounds(2, 1))
                    .expect("painted");
                let mid = f32::from(cell.top() + cell.bottom()) / 2.0;
                assert!(
                    is_colour(&shot, dims, f32::from(cell.left()) + 0.5, mid, colour),
                    "cursor-working-{slug}: a {label} terminal's bar is {colour:?}, saw {:?}",
                    pixel_at(&shot, dims, f32::from(cell.left()) + 0.5, mid)
                );
            }
        }

        // A block in a status colour: its letter in the background colour.
        {
            let mut rig = CursorRig::new(theme, "\r\n$ x\x1b[D", true, true, false, |a| {
                a.cursor_style = CursorShapeSetting::Block;
                a.cursor_glow = false;
            });
            let view = rig.view.clone();
            rig.cx
                .update(|cx| view.update(cx, |v, cx| v.set_status(Status::Working, cx)));
            let shot = rig.frame();
            let cell = rig.cell(2, 1);
            let (l, r) = (f32::from(cell.left()), f32::from(cell.right()));
            let (t, b) = (f32::from(cell.top()), f32::from(cell.bottom()));
            let n = format!("cursor-block-working-{slug}");
            let mut fill = 0;
            let mut letter = 0;
            let mut y = t + 0.5;
            while y < b {
                fill += count_along(&shot, rig.dims, ui.ring_working, y, l, r);
                letter += count_along(&shot, rig.dims, bg, y, l, r);
                y += 1.0;
            }
            assert!(fill > 0, "{n}: the block is ring_working");
            assert!(letter > 0, "{n}: the letter is the background colour");
        }

        // The glow, then a program's steady block.
        {
            let mut rig = CursorRig::new(theme, text, true, true, false, |_| ());
            let shot = rig.frame();
            save_shot("cursor-glow", slug, &shot);
            let cell = rig.cell(2, 1);
            let left = f32::from(cell.left());
            let mid = f32::from(cell.top() + cell.bottom()) / 2.0;
            let n = format!("cursor-glow-{slug}");
            assert!(
                !is_colour(&shot, rig.dims, left - 3.0, mid, bg),
                "{n}: a glow beside the bar"
            );
            assert_eq!(
                rig.read(|v| v.cursor_frames()),
                Frames::Breath,
                "{n}: breathing redraws on the ~30 fps timer"
            );

            rig.write("\x1b[2 q");
            let shot = rig.frame();
            save_shot("cursor-steady-block", slug, &shot);
            let n = format!("cursor-steady-block-{slug}");
            let centre = f32::from(cell.left() + cell.right()) / 2.0;
            assert!(is_colour(&shot, rig.dims, centre, mid, own), "{n}: a block");
            assert!(
                is_colour(&shot, rig.dims, left - 3.0, mid, bg),
                "{n}: no glow around a steady cursor"
            );
            assert_eq!(
                rig.read(|v| v.cursor_frames()),
                Frames::None,
                "{n}: no frames"
            );
        }

        // The breath at rest (M11n): five minutes untouched, solid at full strength with its
        // glow and no frames; a keystroke, a program's write or a refocus brings it back.
        // Glide off, so cat's echo moving the cursor leaves the breath's frames, not a glide's.
        {
            let mut rig = CursorRig::new(theme, text, true, true, false, |a| {
                a.cursor_glide = false;
            });
            rig.frame();
            let n = format!("cursor-breath-resumes-{slug}");
            let cell = rig.cell(2, 1);
            let left = f32::from(cell.left());
            let mid = f32::from(cell.top() + cell.bottom()) / 2.0;
            assert_eq!(rig.read(|v| v.cursor_frames()), Frames::Breath, "{n}");
            let rest = |rig: &mut CursorRig| {
                rig.read(|v| v.age_breath(std::time::Duration::from_millis(301_500)));
                let view = rig.view.clone();
                rig.cx.update(|cx| view.update(cx, |_, cx| cx.notify()));
                rig.frame()
            };
            let shot = rest(&mut rig);
            save_shot("cursor-breath-resumes", slug, &shot);
            assert_eq!(
                rig.read(|v| v.cursor_frames()),
                Frames::None,
                "{n}: at rest after five minutes, no frames"
            );
            assert!(
                is_colour(&shot, rig.dims, left + 0.5, mid, own),
                "{n}: at rest the bar is the full colour, saw {:?}",
                pixel_at(&shot, rig.dims, left + 0.5, mid)
            );
            assert!(
                !is_colour(&shot, rig.dims, left - 3.0, mid, bg),
                "{n}: at rest the glow stays"
            );
            // One paint now, before anything else runs, and the frames it asked for (read
            // before the capture, which paints again): what the very next frame after a change
            // asks for. (A cursor move ends the rest by itself; cat's echo reaches the `Term`
            // from the parser thread before its `Output` is heard.)
            let paint_now = |rig: &mut CursorRig| {
                rig.cx
                    .update_window(rig.window, |_, window, cx| window.draw(cx).clear())
                    .expect("draw");
                let frames = rig.read(|v| v.cursor_frames());
                let image = rig
                    .cx
                    .capture_screenshot(rig.window)
                    .expect("capture the window");
                let shot = Shot {
                    width: image.width(),
                    height: image.height(),
                    rgba: image.as_raw().to_vec(),
                };
                (frames, shot)
            };
            // A keystroke cat doesn't echo (an erase on its empty line): only the keystroke
            // itself can end the rest.
            let view = rig.view.clone();
            rig.cx
                .update(|cx| view.update(cx, |v, cx| v.type_bytes(b"\x7f", cx)));
            assert_eq!(
                paint_now(&mut rig).0,
                Frames::Breath,
                "{n}: a keystroke ends the rest"
            );
            // A typed letter.
            rest(&mut rig);
            assert_eq!(rig.read(|v| v.cursor_frames()), Frames::None, "{n}");
            rig.cx
                .update(|cx| view.update(cx, |v, cx| v.type_bytes(b"x", cx)));
            assert_eq!(
                paint_now(&mut rig).0,
                Frames::Breath,
                "{n}: typing ends the rest"
            );
            // Output that leaves the cursor where it was: cat echoes `a`, then its erase
            // (`\b \b`), so only the `Output` itself can end the rest.
            wait_for(&mut rig, |v| {
                v.text(crate::grid::TextScope::Screen).contains("$ x")
            });
            rig.write("\x1b[2;3H\x1b[K");
            rig.frame();
            rest(&mut rig);
            assert_eq!(rig.read(|v| v.cursor_frames()), Frames::None, "{n}");
            rig.read(|v| v.write_to_program(b"a\x7f"));
            wait_for(&mut rig, |v| v.cursor_frames() == Frames::Breath);
            assert_eq!(
                rig.read(|v| v.cursor_frames()),
                Frames::Breath,
                "{n}: output ends the rest"
            );
            // A program's write that moves the cursor.
            rest(&mut rig);
            assert_eq!(rig.read(|v| v.cursor_frames()), Frames::None, "{n}");
            rig.write("y");
            rig.frame();
            assert_eq!(
                rig.read(|v| v.cursor_frames()),
                Frames::Breath,
                "{n}: a cursor move ends the rest"
            );
            // Blur, then focus back: breathing again, solid at first, from the first paint.
            rest(&mut rig);
            assert_eq!(rig.read(|v| v.cursor_frames()), Frames::None, "{n}");
            rig.cx
                .update_window(rig.window, |_, window, _| window.blur())
                .expect("blur");
            rig.frame();
            assert_eq!(
                rig.read(|v| v.cursor_frames()),
                Frames::None,
                "{n}: blurred"
            );
            let window = rig.window;
            rig.cx
                .update_window(window, |_, window, cx| {
                    use gpui::Focusable;
                    view.focus_handle(cx).focus(window, cx)
                })
                .expect("focus");
            // The focus change paints as the update ends: that one paint must already ask for
            // the breath (nothing else would paint again in the app).
            assert_eq!(
                rig.read(|v| v.cursor_frames()),
                Frames::Breath,
                "{n}: a refocus ends the rest, from its first paint"
            );
            let (frames, shot) = paint_now(&mut rig);
            assert_eq!(frames, Frames::Breath, "{n}: a refocus ends the rest");
            // The bar sits after `$ y` (cat's echo of the `x` was erased by the line above).
            let at = rig.cell(3, 1);
            let x = f32::from(at.left()) + 0.5;
            assert!(
                is_colour(&shot, rig.dims, x, mid, own),
                "{n}: back in focus the bar is solid at first, saw {:?}",
                pixel_at(&shot, rig.dims, x, mid)
            );
            // A setting that stops the breath, turned back on at rest (the glow, Animations,
            // Reduce motion off): breathing again at once, solid at first.
            type Toggle = fn(&mut CursorRig, bool);
            let toggles: [(&str, Toggle); 3] = [
                ("the glow", |rig, on| rig.appearance(|a| a.cursor_glow = on)),
                ("Animations", |rig, on| {
                    rig.appearance(|a| a.animations = on)
                }),
                ("Reduce motion off", |rig, on| {
                    rig.cx.update(|cx| cx.set_reduce_motion(!on))
                }),
            ];
            for (label, set) in toggles {
                rest(&mut rig);
                assert_eq!(
                    rig.read(|v| v.cursor_frames()),
                    Frames::None,
                    "{n}: {label}"
                );
                set(&mut rig, false);
                rig.frame();
                set(&mut rig, true);
                let shot = rig.frame();
                assert_eq!(
                    rig.read(|v| v.cursor_frames()),
                    Frames::Breath,
                    "{n}: {label} back on ends the rest"
                );
                assert!(
                    is_colour(&shot, rig.dims, x, mid, own),
                    "{n}: {label} back on, the bar is solid at first, saw {:?}",
                    pixel_at(&shot, rig.dims, x, mid)
                );
            }
        }

        // Unfocused, and focused in a window that isn't key: a still 1 px outline.
        for (focus, key, label) in [(false, true, "unfocused"), (true, false, "not key")] {
            let mut rig = CursorRig::new(theme, text, focus, key, false, |_| ());
            let view = rig.view.clone();
            rig.cx
                .update(|cx| view.update(cx, |v, cx| v.set_status(Status::Working, cx)));
            let shot = rig.frame();
            if !focus {
                save_shot("cursor-unfocused", slug, &shot);
            }
            let scale = shot.width as f32 / f32::from(rig.dims.width);
            let cell = rig.cell(2, 1);
            let (l, r) = (f32::from(cell.left()), f32::from(cell.right()));
            let mid = f32::from(cell.top() + cell.bottom()) / 2.0;
            let n = format!("cursor-{label}-{slug}");
            assert_eq!(
                count_along(&shot, rig.dims, own, mid, l - 4.0, r + 4.0),
                (2.0 * scale).round() as usize,
                "{n}: a 1 px outline in the theme's cursor colour"
            );
            assert!(
                is_colour(&shot, rig.dims, (l + r) / 2.0, mid, bg),
                "{n}: hollow"
            );
            assert_eq!(
                rig.read(|v| v.cursor_frames()),
                Frames::None,
                "{n}: no frames"
            );
        }

        // 50 moves in one frame: one glide, from the old cell to the last.
        {
            let mut rig = CursorRig::new(theme, text, true, true, false, |_| ());
            rig.frame();
            let mut moves = String::new();
            for i in 0..49 {
                moves.push_str(&format!("\x1b[{};{}H", 2 + i % 3, 3 + i % 7));
            }
            moves.push_str("\x1b[2;13H");
            rig.write(&moves);
            rig.frame();
            let n = format!("cursor-glide-{slug}");
            let glide = rig.read(|v| v.cursor_glide()).expect("a glide");
            assert_eq!(glide.from, (2.0, 1.0), "{n}: from the old cell");
            assert_eq!(glide.to, (12, 1), "{n}: to the last");
            assert_eq!(rig.read(|v| v.cursor_frames()), Frames::EveryFrame, "{n}");
            // Where the first frame draws the bar depends on when the platform drew it (a few
            // ms in, it is already a fraction of a pixel along): the unit tests pin that.
        }

        // Sparks: typed text only.
        {
            let mut rig = CursorRig::new(theme, text, true, true, false, |a| {
                a.cursor_fun = CursorFun::Sparks;
            });
            rig.frame();
            let n = format!("cursor-sparks-{slug}");
            let window = rig.window;
            rig.cx.simulate_keystrokes(window, "a->a");
            assert_eq!(
                rig.read(|v| v.spark_bursts()),
                1,
                "{n}: a typed letter sparks"
            );
            assert_eq!(rig.read(|v| v.live_sparks()), 6, "{n}: six of them");
            rig.write("output");
            rig.cx.simulate_keystrokes(window, "left ctrl-c enter");
            assert_eq!(
                rig.read(|v| v.spark_bursts()),
                1,
                "{n}: no sparks from output, arrows, ctrl-keys or Enter"
            );
        }

        // Motion off: still and solid.
        {
            let mut rig = CursorRig::new(theme, text, true, true, true, |a| {
                a.cursor_fun = CursorFun::Sparks;
            });
            let shot = rig.frame();
            let cell = rig.cell(2, 1);
            let mid = f32::from(cell.top() + cell.bottom()) / 2.0;
            let n = format!("cursor-still-{slug}");
            assert!(
                is_colour(&shot, rig.dims, f32::from(cell.left()) - 3.0, mid, bg),
                "{n}: no glow"
            );
            assert_eq!(
                rig.read(|v| v.cursor_frames()),
                Frames::None,
                "{n}: no frames"
            );
            rig.write("\x1b[2;13H");
            let shot = rig.frame();
            save_shot("cursor-still", slug, &shot);
            let scale = shot.width as f32 / f32::from(rig.dims.width);
            let new = rig.cell(12, 1);
            let (l, r) = (f32::from(new.left()), f32::from(new.right()));
            assert!(rig.read(|v| v.cursor_glide()).is_none(), "{n}: no glide");
            assert!(
                is_colour(&shot, rig.dims, l + 0.5, mid, own),
                "{n}: the bar is on its new cell at once"
            );
            assert_eq!(
                count_along(&shot, rig.dims, own, mid, l - 4.0, r),
                (2.0 * scale).round() as usize,
                "{n}: exactly on the cell's left edge"
            );
            assert_eq!(
                count_along(
                    &shot,
                    rig.dims,
                    own,
                    mid,
                    f32::from(cell.left()) - 1.0,
                    f32::from(cell.right())
                ),
                0,
                "{n}: nothing left at the old cell"
            );
            let window = rig.window;
            rig.cx.simulate_keystrokes(window, "a->a");
            assert_eq!(rig.read(|v| v.spark_bursts()), 0, "{n}: no sparks");
            assert_eq!(rig.read(|v| v.live_sparks()), 0, "{n}: no sparks");
            rig.appearance(|a| a.cursor_fun = CursorFun::Trail);
            rig.write("\x1b[2;3H");
            rig.frame();
            assert!(
                rig.read(|v| v.cursor_glide()).is_none(),
                "{n}: no glide, so no trail"
            );
        }
    }
}

/// The shot terminal of the `command-blocks*` scenes: four prompts marked as fuxx's zsh
/// integration marks them, then a live one. Rows: 0 `$ echo hi` (exit 0), 1 its output,
/// 2 an older empty prompt, 3-4 a two-line prompt and `false` (exit 1, no output),
/// 5 `sleep 9` (running, never ended), 6-7 its output, 8 the live prompt.
fn blocks_text() -> String {
    blocks_text_with(0)
}

/// `blocks_text` with `filler` more lines of output after block 1's `hi`.
fn blocks_text_with(filler: usize) -> String {
    // (`redraw=0`: a scripted shell, it never redraws its prompt, so a resize must not empty it.)
    const A: &str = "\x1b]133;A;redraw=0\x07";
    const B: &str = "\x1b]133;B\x07";
    let c = |cmd: &str| format!("\x1b]133;C;cmdline_url={cmd}\x07");
    let d = |x: i32| format!("\x1b]133;D;{x}\x07");
    [
        format!(
            "{A}$ {B}echo hi\r\n{}hi\r\n{}{}",
            c("echo%20hi"),
            (1..=filler)
                .map(|n| format!("line {n}\r\n"))
                .collect::<String>(),
            d(0)
        ),
        format!("{A}$ {B}\r\n{}", d(0)),
        format!("{A}~/code\r\n$ {B}false\r\n{}{}", c("false"), d(1)),
        format!("{A}$ {B}sleep 9\r\n{}one\r\ntwo\r\n", c("sleep%209")),
        format!("{A}$ {B}"),
    ]
    .concat()
}

/// A `command-blocks*` rig: the blocks text in a `cat` terminal, unfocused (its cursor a still
/// outline), at `dims`, a split's pane when `split`.
fn blocks_rig(
    theme: Theme,
    dims: Size<Pixels>,
    split: bool,
    still: bool,
    tweak: impl FnOnce(&mut Appearance),
) -> CursorRig {
    CursorRig::sized(
        theme,
        &blocks_text(),
        dims,
        split,
        (false, false, still),
        tweak,
    )
}

/// What a `command-blocks*` check reads off a rig: where row `r`'s middle and top are, and
/// where the bars' centre is, in points.
struct BlockGeometry {
    bar_x: f32,
    origin_x: f32,
    sep_x: f32,
    rows: Vec<(f32, f32)>,
}

impl BlockGeometry {
    fn of(rig: &CursorRig, split: bool) -> Self {
        let first = rig.cell(0, 0);
        let origin_x = f32::from(first.left());
        let rows = (0..10)
            .map(|r| {
                let c = rig.cell(0, r);
                (f32::from(c.top()), f32::from(c.center().y))
            })
            .collect();
        Self {
            bar_x: origin_x - crate::block_view::gutter_x(split),
            origin_x,
            // Right of every row's text, inside the grid.
            sep_x: f32::from(rig.cell(20, 0).left()),
            rows,
        }
    }
    fn top(&self, r: usize) -> f32 {
        self.rows[r].0
    }
    fn mid(&self, r: usize) -> f32 {
        self.rows[r].1
    }
}

/// How wide, in points, the pixels along `y` from `x0` to `x1` that aren't `bg` are (a
/// partly covered edge pixel counts by how far it is from `bg`).
fn ink_width(
    shot: &Shot,
    dims: Size<Pixels>,
    bg: alacritty_terminal::vte::ansi::Rgb,
    y: f32,
    x0: f32,
    x1: f32,
) -> f32 {
    let scale = shot.width as f32 / f32::from(dims.width);
    let (a, b) = ((x0 * scale) as usize, (x1 * scale) as usize);
    let mut full = 0.0;
    let mut far = 0.0f32;
    for dx in a..b {
        let p = pixel_at(shot, dims, dx as f32 / scale, y);
        let d = p
            .iter()
            .zip([bg.r, bg.g, bg.b])
            .map(|(a, b)| a.abs_diff(b) as f32)
            .fold(0.0, f32::max);
        far = far.max(d);
        full += d;
    }
    // Each pixel's share: its distance from `bg` over the bar's own (the farthest seen).
    if far == 0.0 { 0.0 } else { full / far / scale }
}

/// The first device pixel (in points) that differs between shots `a` and `b` in the box from
/// (`x0`, `y0`) to (`x1`, `y1`), if any.
fn first_change(
    a: &Shot,
    b: &Shot,
    dims: Size<Pixels>,
    (x0, y0): (f32, f32),
    (x1, y1): (f32, f32),
) -> Option<(f32, f32, [u8; 3], [u8; 3])> {
    let scale = a.width as f32 / f32::from(dims.width);
    let (px0, px1) = ((x0 * scale) as usize, (x1 * scale).ceil() as usize);
    let (py0, py1) = ((y0 * scale) as usize, (y1 * scale).ceil() as usize);
    (py0..py1.max(py0 + 1)).find_map(|py| {
        (px0..px1).find_map(|px_| {
            let (x, y) = (px_ as f32 / scale, py as f32 / scale);
            let (p, q) = (pixel_at(a, dims, x, y), pixel_at(b, dims, x, y));
            (p != q).then_some((x, y, p, q))
        })
    })
}

/// A flood reaches the screen (M11n): a real pane runs `seq 1 300000; echo END`, with
/// Animations off so nothing but its output asks for a repaint, and the window is drawn only
/// when the view asks (`cx.notify`), as the app's frame loop draws it. Once the output stops,
/// with no input at all, the last frame drawn holds `END`. This fails if the view stops
/// acknowledging `Output` (`output_seen`): the parser's gate then sends no further `Output`
/// and the screen stays on an early frame until a keystroke.
fn cursor_flood_scene() {
    use crate::grid::TextScope;
    use std::cell::Cell;
    use std::rc::Rc;
    use std::time::{Duration, Instant};

    /// A file removed when the scene ends, a failed check included.
    struct RemovedOnDrop(std::path::PathBuf);
    impl Drop for RemovedOnDrop {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    let (slug, name) = shot_themes()[0];
    let n = format!("cursor-flood-{slug}");
    // The flood waits for `go` to exist, so it starts only once the scene is watching. The
    // child stays alive after `END`, so no exit repaints the screen either.
    let go = RemovedOnDrop(
        std::env::temp_dir().join(format!("fuxx-ui-shots-{}-flood-go", std::process::id())),
    );
    let _ = std::fs::remove_file(&go.0);
    let program = vec![
        "/bin/sh".into(),
        "-c".into(),
        format!(
            "while [ ! -e '{}' ]; do sleep 0.01; done; seq 1 300000; echo END; exec sleep 60",
            go.0.display()
        ),
    ];
    let mut rig = CursorRig::spawned(
        shot_theme(name),
        program,
        "",
        size(px(320.0), px(120.0)),
        false,
        (true, true, false),
        |a| a.animations = false,
    );
    let asked = Rc::new(Cell::new(0usize));
    let _watch = {
        let asked = asked.clone();
        let view = rig.view.clone();
        rig.cx
            .update(|cx| cx.observe(&view, move |_, _| asked.set(asked.get() + 1)))
    };
    std::fs::write(&go.0, b"").expect("start the flood");
    let (mut drawn, mut frames) = (0, 0);
    let mut screen = String::new();
    let start = Instant::now();
    let mut ended: Option<Instant> = None;
    // Until the `Term` holds `END` and a further half second has passed with no input.
    while ended.is_none_or(|at| at.elapsed() < Duration::from_millis(500)) {
        assert!(
            start.elapsed() < Duration::from_secs(60),
            "{n}: the flood never finished"
        );
        prof::sleep(Duration::from_millis(5));
        rig.cx.run_until_parked();
        if asked.get() != drawn {
            drawn = asked.get();
            frames += 1;
            rig.cx
                .update_window(rig.window, |_, window, cx| window.draw(cx).clear())
                .expect("draw");
            screen = rig.read(|v| v.text(TextScope::Screen));
        }
        if ended.is_none() && rig.read(|v| v.text(TextScope::Screen).contains("END")) {
            ended = Some(Instant::now());
        }
    }
    assert!(
        screen.lines().any(|l| l.trim_end() == "END"),
        "{n}: once the output stops, the last frame drawn (of {frames}) holds END, saw {screen:?}"
    );
}

/// The view's chip, read for the checks: its label, buttons and painted bounds.
fn chip_of(rig: &CursorRig) -> Option<(String, [&'static str; 3], gpui::Bounds<Pixels>)> {
    rig.read(|v| v.chip_shot())
}

/// Draw frames until `done` holds (at most 3 s: the program's echo comes from its own thread).
fn wait_for(rig: &mut CursorRig, done: impl Fn(&crate::terminal_view::TerminalView) -> bool) {
    let start = std::time::Instant::now();
    while !rig.read(&done) && start.elapsed() < std::time::Duration::from_secs(3) {
        prof::sleep(std::time::Duration::from_millis(10));
        let _ = rig.frame();
    }
    let _ = rig.frame();
}

/// Move the pointer to `at` and let the rig settle.
fn point_at(rig: &mut CursorRig, at: gpui::Point<Pixels>) {
    let window = rig.window;
    rig.cx
        .simulate_mouse_move(window, at, None, gpui::Modifiers::none());
    rig.cx.run_until_parked();
}

/// Command blocks (M11l Task 4), in both themes, from a `cat` terminal shown the marked text
/// of `blocks_text`:
/// - `command-blocks`: each block's bar, 8 pt left of the grid, in `block_ok`, `block_failed`
///   (both rows of the two-line prompt) and `block_running`; none beside output or the older
///   empty prompt. No hairline above any prompt (removed 2026-11-10). Hovering block 2's gutter widens its bar to 3 pt and opens the chip (`exit 1 ·
///   false`, the three buttons) 4 pt right of the gutter, one row tall on the command row (the
///   pixel rows just above and below it untouched); the chip stays while the pointer is on it
///   (200 ms on), and goes 150 ms after it leaves both.
///   Nothing moves: two frames 200 ms apart with the chip open are the same pixels, with
///   motion on and with Reduce motion on.
fn command_blocks_scene() {
    use crate::block_view::{CHIP_GAP, CHIP_HIDE_MS};
    use std::time::Duration;
    let dims = size(px(480.0), px(240.0));
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let ui = theme.ui();
        let bg = theme.background;
        let n = format!("command-blocks-{slug}");
        for still in [false, true] {
            let mut rig = blocks_rig(theme, dims, false, still, |_| ());
            let shot = rig.frame();
            let g = BlockGeometry::of(&rig, false);
            let at = |shot: &Shot, x: f32, y: f32, c| is_colour(shot, dims, x, y, c);
            let seen = |shot: &Shot, x: f32, y: f32| pixel_at(shot, dims, x, y);
            if !still {
                save_shot("command-blocks-plain", slug, &shot);
                for (row, colour, what) in [
                    (0, ui.block_ok, "echo hi (exit 0)"),
                    (3, ui.block_failed, "false's first prompt row"),
                    (4, ui.block_failed, "false (exit 1)"),
                    (5, ui.block_running, "sleep 9 (running)"),
                ] {
                    assert!(
                        at(&shot, g.bar_x, g.mid(row), colour),
                        "{n}: row {row}, {what}: {colour:?}, saw {:?}",
                        seen(&shot, g.bar_x, g.mid(row))
                    );
                }
                for (row, what) in [(1, "output"), (2, "the empty prompt"), (6, "output")] {
                    assert!(
                        at(&shot, g.bar_x, g.mid(row), bg),
                        "{n}: no bar beside {what} (row {row}): {:?}",
                        seen(&shot, g.bar_x, g.mid(row))
                    );
                }
                let w = ink_width(&shot, dims, bg, g.mid(4), g.bar_x - 4.0, g.bar_x + 4.0);
                assert!((1.5..=2.5).contains(&w), "{n}: a 2 pt bar, saw {w} pt");
                // No hairline anywhere (removed 2026-11-10, your call): above a block's first
                // prompt row, the live prompt, the first line of scrollback or an empty prompt.
                for row in [0, 2, 3, 5, 8] {
                    assert!(
                        at(&shot, g.sep_x, g.top(row) + 0.5, bg),
                        "{n}: no hairline above row {row}: {:?}",
                        seen(&shot, g.sep_x, g.top(row) + 0.5)
                    );
                }
            }
            // Hover block 2's gutter.
            let ids = rig.read(|v| v.block_ids());
            assert_eq!(ids.len(), 3, "{n}: three blocks");
            point_at(&mut rig, point(px(g.bar_x), px(g.mid(4))));
            assert_eq!(
                rig.read(|v| v.hovered_block().map(str::to_string)),
                Some(ids[1].clone()),
                "{n}: block 2 hovered"
            );
            let shown = rig.frame();
            let (label, buttons, chip) = chip_of(&rig).expect("the chip is open");
            assert!(label.contains("exit 1 · false"), "{n}: chip says {label:?}");
            assert_eq!(buttons, ["Copy command", "Copy output", "Rerun"], "{n}");
            // (Layout snaps boxes to the pixel grid: within half a point.)
            let left = f32::from(chip.left());
            assert!(
                (left - (g.origin_x + CHIP_GAP)).abs() <= 0.5,
                "{n}: the chip's left edge {left} is 4 pt right of the gutter ({})",
                g.origin_x
            );
            assert!(
                (f32::from(chip.top()) - g.top(4)).abs() <= 0.5,
                "{n}: the chip opens at the command row's top"
            );
            let row = g.top(5) - g.top(4);
            let tall = f32::from(chip.size.height);
            assert!(
                (tall - row).abs() <= 0.5,
                "{n}: the chip is one row tall ({row} pt), saw {tall}"
            );
            if !still {
                save_shot("command-blocks", slug, &shown);
                // It covers its row only: the pixel rows just above and just below it (the
                // row under it starts with block 3's hairline) are as they were before it
                // opened, across the grid (the gutter's bar widens; it is left of this).
                let grid = rig.read(|v| v.grid_bounds()).expect("painted");
                let scale = shown.width as f32 / f32::from(dims.width);
                let (x0, x1) = (g.origin_x, f32::from(grid.right()));
                for (y, what) in [
                    (f32::from(chip.top()) - 1.0 / scale, "above"),
                    (f32::from(chip.bottom()), "below"),
                ] {
                    let change = first_change(&shot, &shown, dims, (x0, y), (x1, y));
                    assert!(
                        change.is_none(),
                        "{n}: the chip changed the pixel row {what} it: {change:?}"
                    );
                }
                // Hovered: 3 pt wide, still red.
                let w = ink_width(&shown, dims, bg, g.mid(4), g.bar_x - 4.0, g.bar_x + 4.0);
                assert!(
                    (2.5..=3.5).contains(&w),
                    "{n}: the hovered bar is 3 pt, saw {w}"
                );
                assert!(
                    at(&shown, g.bar_x, g.mid(4), ui.block_failed),
                    "{n}: still red"
                );
                let inside = (left + 3.0, f32::from(chip.center().y));
                assert!(
                    at(&shown, inside.0, inside.1, ui.panel),
                    "{n}: the chip is the panel colour: {:?}",
                    seen(&shown, inside.0, inside.1)
                );
            }
            if !still {
                use crate::grid::TextScope;
                // A program printing on the prompt's row (`cat` echoes what it is sent)
                // doesn't move the block: the chip stays open.
                rig.read(|v| v.write_to_program(b"z"));
                wait_for(&mut rig, |v| v.text(TextScope::Screen).contains("$ z"));
                assert!(
                    rig.read(|v| v.hovered_block().is_some()) && chip_of(&rig).is_some(),
                    "{n}: output that leaves the block in place keeps the chip"
                );
                // Scripted typing and pastes (the scripting API; Rerun's paste in Task 5)
                // keep the chip and the picked block.
                let view = rig.view.clone();
                let first = ids[0].clone();
                rig.cx.update(|cx| {
                    view.update(cx, |v, cx| {
                        v.set_chosen_block(Some(first), cx);
                        v.type_bytes(b"y", cx);
                        v.paste("w", cx);
                    })
                });
                wait_for(&mut rig, |v| v.text(TextScope::Screen).contains("$ zyw"));
                assert!(
                    chip_of(&rig).is_some(),
                    "{n}: a scripted paste keeps the chip"
                );
                assert_eq!(
                    rig.read(|v| v.chosen_block().map(str::to_string)),
                    Some(ids[0].clone()),
                    "{n}: and the picked block"
                );
                rig.cx
                    .update(|cx| view.update(cx, |v, cx| v.set_chosen_block(None, cx)));
            }
            let shown = rig.frame();
            // Nothing animates: 200 ms on (real time for what paints by the clock, the app's
            // clock for its timers), the same pixels.
            prof::sleep(Duration::from_millis(200));
            rig.cx.advance_clock(Duration::from_millis(200));
            let later = rig.frame();
            assert!(
                shown.rgba == later.rgba,
                "{n} (still {still}): the open chip's frames differ 200 ms apart"
            );
            // Onto the chip (the app's clock drives the hide timer): it stays.
            point_at(&mut rig, chip.center());
            rig.cx.advance_clock(Duration::from_millis(200));
            rig.cx.run_until_parked();
            assert!(
                chip_of(&rig).is_some(),
                "{n}: the chip stays while pointed at"
            );
            // Off both: still there at once and 140 ms on, gone at 150 ms.
            let away = rig.cell(30, 10).center();
            point_at(&mut rig, away);
            assert!(chip_of(&rig).is_some(), "{n}: the chip doesn't go at once");
            rig.cx
                .advance_clock(Duration::from_millis(CHIP_HIDE_MS - 10));
            rig.cx.run_until_parked();
            assert!(chip_of(&rig).is_some(), "{n}: the chip is there 140 ms on");
            rig.cx.advance_clock(Duration::from_millis(10));
            rig.cx.run_until_parked();
            assert!(rig.read(|v| v.hovered_block().is_none()), "{n}: unhovered");
            let gone = rig.frame();
            assert!(chip_of(&rig).is_none(), "{n}: the chip went 150 ms after");
            assert!(
                !at(&gone, left + 3.0, f32::from(chip.center().y), ui.panel),
                "{n}: no chip drawn"
            );
            let w = ink_width(&gone, dims, bg, g.mid(4), g.bar_x - 4.0, g.bar_x + 4.0);
            assert!(
                (1.5..=2.5).contains(&w),
                "{n}: the bar is back to 2 pt, saw {w}"
            );
            if !still {
                // The chosen block (picked in Task 5): a 3 pt accent bar down its whole
                // height, output rows too; typing drops it.
                let view = rig.view.clone();
                let third = ids[2].clone();
                rig.cx
                    .update(|cx| view.update(cx, |v, cx| v.set_chosen_block(Some(third), cx)));
                let chosen = rig.frame();
                save_shot("command-blocks-chosen", slug, &chosen);
                for row in [5, 6, 7] {
                    assert!(
                        at(&chosen, g.bar_x, g.mid(row), ui.accent),
                        "{n}: the chosen bar at row {row}: {:?}",
                        seen(&chosen, g.bar_x, g.mid(row))
                    );
                }
                let w = ink_width(&chosen, dims, bg, g.mid(6), g.bar_x - 4.0, g.bar_x + 4.0);
                assert!(
                    (2.5..=3.5).contains(&w),
                    "{n}: the chosen bar is 3 pt, saw {w}"
                );
                assert!(
                    at(&chosen, g.bar_x, g.mid(8), bg),
                    "{n}: not past the block"
                );
                // A press on a bar dragged into the grid selects nothing; the same drag
                // from the grid does.
                let (window, none) = (rig.window, gpui::Modifiers::none());
                let left = Some(gpui::MouseButton::Left);
                let to = rig.cell(6, 6).center();
                for (from, selects) in [
                    (point(px(g.bar_x), px(g.mid(4))), false),
                    (rig.cell(1, 4).center(), true),
                ] {
                    rig.cx
                        .simulate_mouse_down(window, from, gpui::MouseButton::Left, none);
                    rig.cx.simulate_mouse_move(window, to, left, none);
                    rig.cx
                        .simulate_mouse_up(window, to, gpui::MouseButton::Left, none);
                    rig.cx.run_until_parked();
                    let text = rig.read(|v| v.selection_text()).unwrap_or_default();
                    assert_eq!(!text.is_empty(), selects, "{n}: from {from:?}: {text:?}");
                }
                // An open chip closes on a press in the grid, on a typed key (which also
                // drops the picked block), and on output that scrolls its block.
                let open = |rig: &mut CursorRig, dy: f32| {
                    point_at(rig, point(px(g.bar_x), px(g.mid(4) + dy)));
                    assert!(chip_of(rig).is_some(), "{n}: the chip opens again");
                };
                open(&mut rig, 1.0);
                let grid_at = rig.cell(10, 2).center();
                rig.cx
                    .simulate_mouse_down(window, grid_at, gpui::MouseButton::Left, none);
                rig.cx
                    .simulate_mouse_up(window, grid_at, gpui::MouseButton::Left, none);
                rig.cx.run_until_parked();
                assert!(chip_of(&rig).is_none(), "{n}: a grid press closes the chip");
                open(&mut rig, -1.0);
                let third = ids[2].clone();
                rig.cx
                    .update(|cx| view.update(cx, |v, cx| v.set_chosen_block(Some(third), cx)));
                rig.cx
                    .update_window(window, |_, window, cx| {
                        use gpui::Focusable;
                        view.focus_handle(cx).focus(window, cx)
                    })
                    .expect("focus");
                rig.cx.simulate_keystrokes(window, "q");
                rig.cx.run_until_parked();
                assert!(chip_of(&rig).is_none(), "{n}: a typed key closes the chip");
                assert!(
                    rig.read(|v| v.chosen_block().is_none()),
                    "{n}: and drops the picked block"
                );
                open(&mut rig, 2.0);
                // Three Returns: `cat`'s echo and its copy of each line scroll the screen a
                // few rows, block 2 still on it.
                rig.read(|v| v.write_to_program(b"\r\r\r"));
                wait_for(&mut rig, |v| v.hovered_block().is_none());
                let screen = rig.read(|v| v.text(crate::grid::TextScope::Screen));
                assert!(
                    screen.contains("$ false") && !screen.starts_with("$ echo hi"),
                    "{n}: scrolled, block 2 still shown: {screen:?}"
                );
                assert!(
                    rig.read(|v| v.hovered_block().is_none()),
                    "{n}: output that moves the block closes the chip"
                );
                assert!(chip_of(&rig).is_none(), "{n}: no chip");
            }
        }
    }
}

/// `command-blocks-alt-screen`: with a full-screen program up (`ESC [?1049h`), no bar, no
/// hairline and no chip; back on the main screen (`ESC [?1049l`) the blocks are as they were.
fn command_blocks_alt_screen_scene() {
    let dims = size(px(480.0), px(240.0));
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let ui = theme.ui();
        let bg = theme.background;
        let n = format!("command-blocks-alt-screen-{slug}");
        let mut rig = blocks_rig(theme, dims, false, false, |_| ());
        let _ = rig.frame();
        let g = BlockGeometry::of(&rig, false);
        rig.write("\x1b[?1049hvim, full screen\r\n");
        let shot = rig.frame();
        save_shot("command-blocks-alt-screen", slug, &shot);
        let inks = [ui.block_ok, ui.block_failed, ui.block_running, ui.accent];
        for row in 0..10 {
            for c in inks {
                assert!(
                    !is_colour(&shot, dims, g.bar_x, g.mid(row), c),
                    "{n}: a bar on the alternate screen at row {row}"
                );
            }
            assert!(
                is_colour(&shot, dims, g.sep_x, g.top(row) + 0.5, bg),
                "{n}: a hairline on the alternate screen above row {row}"
            );
        }
        point_at(&mut rig, point(px(g.bar_x), px(g.mid(4))));
        assert!(rig.read(|v| v.hovered_block().is_none()), "{n}: no hover");
        assert!(chip_of(&rig).is_none(), "{n}: no chip");
        rig.write("\x1b[?1049l");
        let back = rig.frame();
        assert!(
            is_colour(&back, dims, g.bar_x, g.mid(4), ui.block_failed),
            "{n}: back on the main screen, the bars are back"
        );
    }
}

/// A card-view rig (M11ab): `text` in a `cat` that stands in for a zsh with fuxx's marks.
fn cards_rig(theme: Theme, text: &str, dims: Size<Pixels>) -> CursorRig {
    let mut rig = CursorRig::sized(theme, text, dims, false, (false, false, false), |_| ());
    let view = rig.view.clone();
    rig.cx.update(|cx| {
        view.update(cx, |v, cx| {
            v.shot_integrated();
            v.set_branch(Some("main".into()), cx);
            cx.notify();
        })
    });
    rig
}

/// The terminal's size for `dims` in today's view (the pane less its padding).
fn plain_dims(dims: Size<Pixels>, (cell_w, line_h): (f32, f32)) -> crate::terminal::TermDims {
    let (pad_x, pad_y) = crate::ui_tokens::size::TERMINAL_PADDING;
    crate::terminal::TermDims {
        cols: (((f32::from(dims.width) - 2.0 * pad_x) / cell_w).floor() as usize).max(2),
        rows: (((f32::from(dims.height) - 2.0 * pad_y) / line_h).floor() as usize).max(1),
    }
}

/// `shell-cards` (M11ab): `echo hi` with 25 lines of output (folded to its last 12), an empty
/// prompt (hidden), `false` (exit 1, in the failed colour), a stranded `sleep 9`, the branch
/// chip, and the live prompt in its box at the bottom; the terminal sized to the box.
fn shell_cards_scene() {
    let dims = size(px(640.0), px(520.0));
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let ui = theme.ui();
        let n = format!("shell-cards-{slug}");
        let mut rig = cards_rig(theme, &blocks_text_with(24), dims);
        let shot = rig.frame();
        save_shot("shell-cards", slug, &shot);
        let (cards, term, cell) = rig.read(|v| v.shot_cards());
        assert!(cards, "{n}: card view");
        assert_eq!(
            term,
            crate::card_view::card_dims(dims, false, cell.0, cell.1),
            "{n}: the terminal is the box's inside"
        );
        assert_eq!(
            rig.read(|v| v.shot_card_count()),
            3,
            "{n}: three cards in view"
        );
        // `exit 1` in the failed colour, and the branch chip's pill, somewhere in the frame.
        let w = f32::from(dims.width) as usize;
        let h = f32::from(dims.height) as usize;
        let any = |c| {
            (0..h).step_by(2).any(|y| {
                (0..w)
                    .step_by(2)
                    .any(|x| is_colour(&shot, dims, x as f32, y as f32, c))
            })
        };
        assert!(any(ui.block_failed), "{n}: exit 1 in the failed colour");
        assert!(any(ui.raised), "{n}: the chips' pills");
    }
}

/// `shell-cards-select` (final review #1): a drag across a card's output selects its rows (the
/// selection stays in that card), and a press-drag-release in the live box selects there too.
fn shell_cards_select_scene() {
    let dims = size(px(640.0), px(520.0));
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let n = format!("shell-cards-select-{slug}");
        let mut rig = cards_rig(theme, &blocks_text_with(24), dims);
        let _ = rig.frame();
        let (_, _, (cw, lh)) = rig.read(|v| v.shot_cards());
        let regions = rig.read(|v| v.shot_card_regions());
        let (window, none) = (rig.window, gpui::Modifiers::none());
        let left = Some(gpui::MouseButton::Left);
        let drag = |rig: &mut CursorRig, from: gpui::Point<Pixels>, to: gpui::Point<Pixels>| {
            rig.cx
                .simulate_mouse_down(window, from, gpui::MouseButton::Left, none);
            rig.cx.simulate_mouse_move(window, to, left, none);
            rig.cx
                .simulate_mouse_up(window, to, gpui::MouseButton::Left, none);
            rig.cx.run_until_parked();
            rig.read(|v| v.selection_text()).unwrap_or_default()
        };
        // The folded echo card's rows ("line 13" ..): from its first row to its third.
        let (_, x, y, _, _) = regions
            .iter()
            .find(|r| !r.4 && !r.0.starts_with("rows:"))
            .cloned()
            .expect("a card's panel in view");
        let from = point(px(x + 1.0), px(y + lh * 0.5));
        let to = point(px(x + cw * 6.5), px(y + lh * 2.5));
        let text = drag(&mut rig, from, to);
        assert!(
            text.contains("line"),
            "{n}: a drag selects the card's rows: {text:?}"
        );
        assert_eq!(text.lines().count(), 3, "{n}: three rows: {text:?}");
        let shot = rig.frame();
        save_shot("shell-cards-select", slug, &shot);
        // A program in the live box that turned the mouse on gets its press, and its release
        // even when the button comes up over the chips row (above the box).
        rig.write("\x1b[?1000h");
        let _ = rig.frame();
        let view = rig.view.clone();
        let take_sent =
            |rig: &mut CursorRig| rig.cx.update(|cx| view.update(cx, |v, _| v.take_sent()));
        let _ = take_sent(&mut rig);
        let regions = rig.read(|v| v.shot_card_regions());
        let (_, x, y, _, _) = regions.iter().find(|r| r.4).cloned().expect("the live box");
        let press = point(px(x + cw * 2.5), px(y + lh * 0.5));
        let above = point(px(x + cw * 2.5), px(y - 30.0));
        rig.cx
            .simulate_mouse_down(window, press, gpui::MouseButton::Left, none);
        rig.cx
            .simulate_mouse_up(window, above, gpui::MouseButton::Left, none);
        rig.cx.run_until_parked();
        let sent = take_sent(&mut rig);
        // X10 reports: ESC [ M, then a button byte: 32 (press), 35 (release).
        let reports: Vec<u8> = sent
            .windows(4)
            .filter(|w| w[..3] == *b"\x1b[M")
            .map(|w| w[3])
            .collect();
        assert_eq!(
            reports,
            [32, 35],
            "{n}: press and release reported: {sent:?}"
        );
        // Rerun on a card far up brings the column to its bottom, where it types (review).
        rig.write("\x1b[?1000l");
        let view = rig.view.clone();
        rig.cx
            .update(|cx| view.update(cx, |v, _| v.shot_scroll_cards(200.0)));
        let _ = rig.frame();
        assert!(rig.read(|v| v.shot_card_scroll()) > 0.0, "{n}: scrolled up");
        let id = rig
            .read(|v| v.block_ids())
            .first()
            .cloned()
            .expect("a card");
        rig.cx
            .update(|cx| view.update(cx, |v, cx| v.rerun_card(&id, cx)));
        assert_eq!(
            rig.read(|v| v.shot_card_scroll()),
            0.0,
            "{n}: Rerun snaps to the bottom"
        );
    }
}

/// `shell-cards-running-full`: a command printing 200 lines runs: the live box holds it and
/// fills the pane, no card in view.
fn shell_cards_running_full_scene() {
    let dims = size(px(640.0), px(420.0));
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let n = format!("shell-cards-running-full-{slug}");
        let text = format!(
            "{}\x1b]133;A;redraw=0\x07$ \x1b]133;B\x07make\r\n\x1b]133;C;cmdline_url=make\x07{}",
            blocks_text(),
            (1..=200)
                .map(|i| format!("building {i}\r\n"))
                .collect::<String>()
        );
        let mut rig = cards_rig(theme, &text, dims);
        let shot = rig.frame();
        save_shot("shell-cards-running-full", slug, &shot);
        assert!(rig.read(|v| v.shot_cards()).0, "{n}: card view");
        assert_eq!(rig.read(|v| v.shot_card_count()), 0, "{n}: no card in view");
    }
}

/// `shell-cards-alt-screen`: a full-screen program takes the whole pane in today's view (its
/// full width); leaving it brings the cards back.
fn shell_cards_alt_screen_scene() {
    let dims = size(px(640.0), px(420.0));
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let n = format!("shell-cards-alt-screen-{slug}");
        let mut rig = cards_rig(theme, &blocks_text(), dims);
        let _ = rig.frame();
        let cell = rig.read(|v| v.shot_cards()).2;
        rig.write("\x1b[?1049hvim, full screen\r\n");
        let shot = rig.frame();
        save_shot("shell-cards-alt-screen", slug, &shot);
        let (cards, term, _) = rig.read(|v| v.shot_cards());
        assert!(!cards, "{n}: today's view on the alternate screen");
        assert_eq!(term, plain_dims(dims, cell), "{n}: the pane's full size");
        rig.write("\x1b[?1049l");
        let _ = rig.frame();
        let (cards, term, _) = rig.read(|v| v.shot_cards());
        assert!(cards, "{n}: cards again after it");
        assert_eq!(
            term,
            crate::card_view::card_dims(dims, false, cell.0, cell.1)
        );
    }
}

/// `shell-cards-off`: `shell-cards = off` is today's terminal (its size, M11l's bars).
fn shell_cards_off_scene() {
    let dims = size(px(640.0), px(420.0));
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let n = format!("shell-cards-off-{slug}");
        let mut rig = cards_rig(theme, &blocks_text(), dims);
        rig.appearance(|a| a.shell_cards = false);
        let shot = rig.frame();
        save_shot("shell-cards-off", slug, &shot);
        let (cards, term, cell) = rig.read(|v| v.shot_cards());
        assert!(!cards, "{n}: today's view");
        assert_eq!(term, plain_dims(dims, cell), "{n}: today's size");
    }
}

/// `shell-cards-toggle`: with a command running, the setting off then on resizes the
/// terminal each way, and the running command stays in the live box.
fn shell_cards_toggle_scene() {
    let dims = size(px(640.0), px(420.0));
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let n = format!("shell-cards-toggle-{slug}");
        let mut rig = cards_rig(theme, &blocks_text(), dims);
        let _ = rig.frame();
        let (_, cards_dims, cell) = rig.read(|v| v.shot_cards());
        rig.appearance(|a| a.shell_cards = false);
        let _ = rig.frame();
        assert_eq!(
            rig.read(|v| v.shot_cards()).1,
            plain_dims(dims, cell),
            "{n}: off"
        );
        rig.appearance(|a| a.shell_cards = true);
        let shot = rig.frame();
        save_shot("shell-cards-toggle", slug, &shot);
        let (cards, term, _) = rig.read(|v| v.shot_cards());
        assert!(cards, "{n}: on again");
        assert_eq!(term, cards_dims, "{n}: back to the box's inside");
    }
}

/// `command-blocks-narrow`: a split's pane with a 200 pt grid: the bars 6 pt left of the grid,
/// the chip's buttons short (`Cmd`, `Output`, `Rerun`), the chip one row tall and inside the
/// pane.
fn command_blocks_narrow_scene() {
    // The split's padding each side (a token): a 200 pt grid area.
    let dims = size(
        px(200.0 + 2.0 * crate::ui_tokens::size::SPLIT_TERMINAL_PADDING.0),
        px(240.0),
    );
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let ui = theme.ui();
        let n = format!("command-blocks-narrow-{slug}");
        let mut rig = blocks_rig(theme, dims, true, false, |_| ());
        let _ = rig.frame();
        let g = BlockGeometry::of(&rig, true);
        point_at(&mut rig, point(px(g.bar_x), px(g.mid(4))));
        let shot = rig.frame();
        save_shot("command-blocks-narrow", slug, &shot);
        assert!(
            is_colour(&shot, dims, g.bar_x, g.mid(0), ui.block_ok),
            "{n}: the bar 6 pt left of the grid: {:?}",
            pixel_at(&shot, dims, g.bar_x, g.mid(0))
        );
        let (label, buttons, chip) = chip_of(&rig).expect("the chip is open");
        assert_eq!(buttons, ["Cmd", "Output", "Rerun"], "{n}: {label:?}");
        let grid = rig.read(|v| v.grid_bounds()).expect("painted");
        assert!(
            chip.right() <= grid.right(),
            "{n}: the chip's right edge {:?} is inside the grid ({:?})",
            chip.right(),
            grid.right()
        );
        assert!(
            f32::from(chip.left()) >= g.origin_x,
            "{n}: the chip starts right of the gutter"
        );
        let (row, tall) = (g.top(5) - g.top(4), f32::from(chip.size.height));
        assert!((tall - row).abs() <= 0.5, "{n}: one row tall, saw {tall}");
    }
}

/// `command-blocks-small-font`: a 9 pt terminal (12 pt rows): the chip is still one row tall,
/// its text smaller to fit and inked inside it, and the pixel rows just above and below it are
/// as they were before it opened.
fn command_blocks_small_font_scene() {
    let dims = size(px(480.0), px(240.0));
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let ui = theme.ui();
        let n = format!("command-blocks-small-font-{slug}");
        let mut rig = blocks_rig(theme, dims, false, false, |a| a.font_size = 9.0);
        let before = rig.frame();
        let g = BlockGeometry::of(&rig, false);
        point_at(&mut rig, point(px(g.bar_x), px(g.mid(4))));
        let shown = rig.frame();
        save_shot("command-blocks-small-font", slug, &shown);
        let (_, _, chip) = chip_of(&rig).expect("the chip is open");
        let (row, tall) = (g.top(5) - g.top(4), f32::from(chip.size.height));
        assert!(row < 13.0, "{n}: a short row, {row} pt");
        assert!((tall - row).abs() <= 0.5, "{n}: one row tall, saw {tall}");
        let grid = rig.read(|v| v.grid_bounds()).expect("painted");
        let scale = shown.width as f32 / f32::from(dims.width);
        let (x0, x1) = (g.origin_x, f32::from(grid.right()));
        for (y, what) in [
            (f32::from(chip.top()) - 1.0 / scale, "above"),
            (f32::from(chip.bottom()), "below"),
        ] {
            let change = first_change(&before, &shown, dims, (x0, y), (x1, y));
            assert!(
                change.is_none(),
                "{n}: the chip changed the pixel row {what} it: {change:?}"
            );
        }
        // Its words are drawn: pixels well off the panel's colour inside it.
        let inked = (0..(f32::from(chip.size.width) * scale) as usize)
            .flat_map(|dx| (0..(tall * scale) as usize).map(move |dy| (dx as f32, dy as f32)))
            .filter(|&(dx, dy)| {
                let p = pixel_at(
                    &shown,
                    dims,
                    f32::from(chip.left()) + (dx + 0.5) / scale,
                    f32::from(chip.top()) + (dy + 0.5) / scale,
                );
                p.iter()
                    .zip([ui.panel.r, ui.panel.g, ui.panel.b])
                    .any(|(a, b)| a.abs_diff(b) > 60)
            })
            .count();
        assert!(
            inked > 40,
            "{n}: the chip's text is drawn ({inked} inked pixels)"
        );
    }
}

/// `command-blocks-rprompt`: block 2's command row (`$ false`) given a right prompt, a run of
/// text after two or more blank cells ending a cell short of the right edge (as zsh draws an
/// RPROMPT, powerlevel10k's `✔ / 15:05:41` among them):
/// - `-room`: a short one leaves room for the chip with its long buttons: they stay long and
///   the chip ends before it;
/// - `-tight`: a longer one leaves less than the short chip would take whole (but more than
///   its narrowest): the buttons go short (`Cmd`, `Output`, `Rerun`), the command is cut, and
///   the chip ends exactly a cell before it;
/// - `-overlap`: one that leaves less than the chip's narrowest: the chip keeps that width,
///   its short buttons inside it, over the start of the run.
///
/// Where the chip ends before the run, the run's pixels are as they were before it opened.
fn command_blocks_rprompt_scene() {
    use crate::block_view::{CHIP_GAP, CHIP_MIN_WIDTH, CHIP_PAD};
    let dims = size(px(480.0), px(240.0));
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        for (case, run, long) in [
            ("room", "ok / 15:05:41", true),
            ("tight", "", false),
            (
                "overlap",
                "ok / 15:05:41 / ~/code/fuxx-cli / main / x",
                false,
            ),
        ] {
            let n = format!("command-blocks-rprompt-{case}-{slug}");
            let mut rig = blocks_rig(theme, dims, false, false, |_| ());
            let _ = rig.frame();
            let g = BlockGeometry::of(&rig, false);
            let grid = rig.read(|v| v.grid_bounds()).expect("painted");
            let cell = f32::from(rig.cell(0, 0).size.width);
            let cols = (f32::from(grid.size.width) / cell).round() as usize;
            // The run ends one cell short of the right edge (zsh's ZLE_RPROMPT_INDENT). The
            // tight one starts a cell past where the narrowest chip would end.
            let run = match case {
                "tight" => {
                    let start = ((CHIP_GAP + CHIP_MIN_WIDTH) / cell).ceil() as usize + 2;
                    let text = "ok / 15:05:41 / ~/code/fuxx-cli/src/terminal_view.rs";
                    let cut: String = text.chars().take(cols - 1 - start).collect();
                    cut.trim_end().to_string()
                }
                _ => run.to_string(),
            };
            let run = run.as_str();
            let len = run.chars().count();
            let start = cols - 1 - len;
            rig.write(&format!("\x1b7\x1b[5;{}H{run}\x1b8", start + 1));
            let before = rig.frame();
            assert!(
                rig.read(|v| v.text(crate::grid::TextScope::Screen))
                    .contains(&format!("$ false{}{run}", " ".repeat(start - 7))),
                "{n}: the run is on the command row"
            );
            point_at(&mut rig, point(px(g.bar_x), px(g.mid(4))));
            let shown = rig.frame();
            save_shot(&format!("command-blocks-rprompt-{case}"), slug, &shown);
            let (label, buttons, chip) = chip_of(&rig).expect("the chip is open");
            assert!(label.contains("exit 1"), "{n}: {label:?}");
            let (row, tall) = (g.top(5) - g.top(4), f32::from(chip.size.height));
            assert!((tall - row).abs() <= 0.5, "{n}: one row tall, saw {tall}");
            let want = if long {
                ["Copy command", "Copy output", "Rerun"]
            } else {
                ["Cmd", "Output", "Rerun"]
            };
            assert_eq!(buttons, want, "{n}");
            let run_left = f32::from(rig.cell(start, 4).left());
            let right = f32::from(chip.right());
            let (left, width) = (f32::from(chip.left()), f32::from(chip.size.width));
            assert!(
                (left - (g.origin_x + CHIP_GAP)).abs() <= 0.5,
                "{n}: the left anchor is the gutter's"
            );
            // Whatever its width, the buttons fit inside it.
            let (bounds, _, _) = rig.read(|v| v.chip_buttons_shot()).expect("the chip");
            for (i, b) in bounds.iter().enumerate() {
                let b = b.expect("laid out");
                assert!(
                    f32::from(b.right()) <= right - CHIP_PAD + 0.5,
                    "{n}: button {i} ({:?}) inside the chip ({right})",
                    b.right()
                );
            }
            if case == "overlap" {
                assert!(
                    (width - CHIP_MIN_WIDTH).abs() <= 0.5,
                    "{n}: at its narrowest ({CHIP_MIN_WIDTH}), saw {width}"
                );
                assert!(right > run_left, "{n}: over the run's start");
            } else {
                // GPUI rounds the chip's left and its max width to device pixels separately, so
                // its right edge lands within (strictly less than) one device pixel of
                // `run_left - cell`: 1 pt at 1x, 0.5 pt at 2x. A real overlap is a whole cell.
                let scale = shown.width as f32 / f32::from(dims.width);
                let dp = 1.0 / scale;
                assert!(
                    right < run_left - cell + dp,
                    "{n}: the chip ends at {right}, a cell before the run at {run_left}"
                );
                if case == "tight" {
                    assert!(
                        (right - (run_left - cell)).abs() < dp && width >= CHIP_MIN_WIDTH,
                        "{n}: cut to end a cell before the run ({run_left}), at {right}"
                    );
                }
                // From the first device pixel wholly right of the chip: its edge is rounded to
                // the pixel grid, so it may end up to (just under) a device pixel past
                // `run_left - cell`.
                let from = ((run_left - cell) * scale).ceil() / scale;
                let change = first_change(
                    &before,
                    &shown,
                    dims,
                    (from, g.top(4)),
                    (f32::from(grid.right()), g.top(5)),
                );
                assert!(
                    change.is_none(),
                    "{n}: the run's pixels changed: {change:?}"
                );
            }
        }
    }
}

/// `command-blocks-off`: with `command-blocks = off`, the same terminal draws no bar or
/// hairline, and hovering the gutter opens nothing.
fn command_blocks_off_scene() {
    let dims = size(px(480.0), px(240.0));
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let ui = theme.ui();
        let bg = theme.background;
        let n = format!("command-blocks-off-{slug}");
        let mut rig = blocks_rig(theme, dims, false, false, |a| a.command_blocks = false);
        let shot = rig.frame();
        save_shot("command-blocks-off", slug, &shot);
        let g = BlockGeometry::of(&rig, false);
        for row in 0..10 {
            assert!(
                is_colour(&shot, dims, g.bar_x, g.mid(row), bg),
                "{n}: nothing in the gutter at row {row}"
            );
            assert!(
                is_colour(&shot, dims, g.sep_x, g.top(row) + 0.5, bg),
                "{n}: no hairline above row {row}"
            );
        }
        assert!(
            !is_colour(&shot, dims, g.bar_x, g.mid(4), ui.block_failed),
            "{n}: no red bar"
        );
        point_at(&mut rig, point(px(g.bar_x), px(g.mid(4))));
        assert!(rig.read(|v| v.hovered_block().is_none()), "{n}: no hover");
        assert!(chip_of(&rig).is_none(), "{n}: no chip");
    }
}

/// Give a `command-blocks*` rig fuxx's key bindings and the focus, so keys reach its terminal.
fn keyed(rig: &mut CursorRig) {
    use gpui::Focusable;
    let view = rig.view.clone();
    rig.cx.update(|cx| cx.bind_keys(crate::key_bindings()));
    rig.cx
        .update_window(rig.window, |_, window, cx| {
            view.focus_handle(cx).focus(window, cx)
        })
        .expect("focus");
    rig.cx.run_until_parked();
}

fn press(rig: &mut CursorRig, keys: &str) {
    let window = rig.window;
    rig.cx.simulate_keystrokes(window, keys);
    let _ = rig.frame();
}

fn chosen(rig: &CursorRig) -> Option<String> {
    rig.read(|v| v.chosen_block().map(str::to_string))
}

fn clipboard(rig: &mut CursorRig) -> Option<String> {
    rig.cx
        .update(|cx| cx.read_from_clipboard().and_then(|c| c.text()))
}

fn set_clipboard(rig: &mut CursorRig, text: &str) {
    rig.cx
        .update(|cx| cx.write_to_clipboard(gpui::ClipboardItem::new_string(text.into())));
}

/// The clipboard once a copy has run: its text is read on the background executor, which is
/// the harness's (GPUI's test scheduler), so a frame (`run_until_parked`) runs it to the end.
fn copied(rig: &mut CursorRig) -> Option<String> {
    let _ = rig.frame();
    clipboard(rig)
}

/// Press chip button `i` (0 Copy command, 1 Copy output, 2 Rerun) of the open chip.
fn press_chip_button(rig: &mut CursorRig, i: usize) {
    let (buttons, _, _) = rig
        .read(|v| v.chip_buttons_shot())
        .expect("the chip is open");
    let at = buttons[i].expect("the button was laid out").center();
    let window = rig.window;
    rig.cx.simulate_click(window, at, gpui::Modifiers::none());
    let _ = rig.frame();
}

/// `command-blocks-jump` (M11l Task 5), in both themes: the blocks text with 40 lines of
/// output under `echo hi`, five rows tall (so `false`'s first prompt row is just above the
/// view). ⇧⌘↑ picks `sleep 9` (all in view: no scroll), again `false`, scrolled so its first
/// prompt row is the top row with its bar in the accent down both rows; ⇧⌘↓ twice comes back
/// to the bottom with nothing picked; ⌘↑ still scrolls a page; a typed key drops the pick.
fn command_blocks_jump_scene() {
    let dims = size(px(480.0), px(120.0));
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let ui = theme.ui();
        let n = format!("command-blocks-jump-{slug}");
        let mut rig = CursorRig::sized(
            theme,
            &blocks_text_with(40),
            dims,
            false,
            (false, false, false),
            |_| (),
        );
        keyed(&mut rig);
        let _ = rig.frame();
        let g = BlockGeometry::of(&rig, false);
        let line_height = f32::from(rig.cell(0, 1).top() - rig.cell(0, 0).top());
        let grid = rig.read(|v| v.grid_bounds()).expect("painted");
        let rows = (f32::from(grid.size.height) / line_height).round() as usize;
        let blocks = rig.read(|v| v.shot_blocks());
        let ids: Vec<String> = blocks.iter().map(|b| b.id.clone()).collect();
        assert_eq!(ids.len(), 3, "{n}: three blocks");
        assert!(
            blocks[1].prompt_top.0 < 0,
            "{n}: false's prompt starts above the view ({rows} rows): {blocks:?}"
        );
        let offset = |rig: &CursorRig| rig.read(|v| v.display_offset());
        assert_eq!(offset(&rig), 0);

        press(&mut rig, "cmd-shift-up");
        assert_eq!(chosen(&rig), Some(ids[2].clone()), "{n}: sleep 9 picked");
        assert_eq!(offset(&rig), 0, "{n}: all in view, no scroll");

        press(&mut rig, "cmd-shift-up");
        assert_eq!(chosen(&rig), Some(ids[1].clone()), "{n}: false picked");
        let top = blocks[1].prompt_top.0;
        assert_eq!(
            offset(&rig) as i32,
            -top,
            "{n}: its first prompt row is the top row"
        );
        let shot = rig.frame();
        save_shot("command-blocks-jump", slug, &shot);
        for row in [0, 1] {
            assert!(
                is_colour(&shot, dims, g.bar_x, g.mid(row), ui.accent),
                "{n}: the picked bar at row {row}: {:?}",
                pixel_at(&shot, dims, g.bar_x, g.mid(row))
            );
        }
        let w = ink_width(
            &shot,
            dims,
            theme.background,
            g.mid(1),
            g.bar_x - 4.0,
            g.bar_x + 4.0,
        );
        assert!((2.5..=3.5).contains(&w), "{n}: a 3 pt bar, saw {w}");

        press(&mut rig, "cmd-shift-down");
        assert_eq!(chosen(&rig), Some(ids[2].clone()), "{n}: back to sleep 9");
        press(&mut rig, "cmd-shift-down");
        assert_eq!(chosen(&rig), None, "{n}: past the newest, nothing picked");
        assert_eq!(offset(&rig), 0, "{n}: at the bottom");
        // Past the oldest, ⇧⌘↑ stays.
        for _ in 0..3 {
            press(&mut rig, "cmd-shift-up");
        }
        assert_eq!(chosen(&rig), Some(ids[0].clone()), "{n}: echo hi picked");
        let at_oldest = offset(&rig);
        press(&mut rig, "cmd-shift-up");
        assert_eq!(
            chosen(&rig),
            Some(ids[0].clone()),
            "{n}: stays on the oldest"
        );
        assert_eq!(offset(&rig), at_oldest, "{n}: and doesn't scroll");
        press(&mut rig, "cmd-shift-down");
        press(&mut rig, "cmd-shift-down");
        press(&mut rig, "cmd-shift-down");
        assert_eq!((chosen(&rig), offset(&rig)), (None, 0), "{n}: bottom again");

        // ⌘↑ still scrolls a page.
        press(&mut rig, "cmd-up");
        assert_eq!(offset(&rig), rows, "{n}: ⌘↑ scrolled {rows} rows");
        press(&mut rig, "cmd-down");
        assert_eq!(offset(&rig), 0, "{n}: ⌘↓ back down");

        // A typed key drops the pick.
        press(&mut rig, "cmd-shift-up");
        assert!(chosen(&rig).is_some());
        press(&mut rig, "q");
        assert_eq!(chosen(&rig), None, "{n}: typing drops the pick");

        // On the alternate screen the keys do nothing.
        rig.write("\x1b[?1049h");
        press(&mut rig, "cmd-shift-up");
        assert_eq!(chosen(&rig), None, "{n}: no pick on the alternate screen");
        rig.write("\x1b[?1049l");
        // With command blocks off, nothing either.
        rig.appearance(|a| a.command_blocks = false);
        press(&mut rig, "cmd-shift-up");
        assert_eq!(chosen(&rig), None, "{n}: no pick with command blocks off");
    }
}

/// `command-blocks-copy` (M11l Task 5), in Mocha (both themes with `--all-themes`), the blocks
/// text: with nothing picked
/// ⇧⌘C copies the newest finished block's output (`false`: none, so the clipboard is left
/// as it was); a click on
/// `echo hi`'s bar picks it (no scroll) and ⇧⌘C copies `hi`; its chip's Copy command copies
/// `echo hi` and Copy output `hi`; the palette's Copy Command and Output copies
/// `$ echo hi\nhi`; a click on `sleep 9`'s bar picks it, still no scroll, and ⇧⌘C copies its
/// output so far. With command blocks off, ⇧⌘C copies nothing.
fn command_blocks_copy_scene() {
    use crate::terminal_view::{CopyCommand, CopyCommandAndOutput};
    let dims = size(px(480.0), px(240.0));
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let n = format!("command-blocks-copy-{slug}");
        let mut rig = blocks_rig(theme, dims, false, false, |_| ());
        let _keep = ClipboardKeeper::new(&rig);
        keyed(&mut rig);
        let _ = rig.frame();
        let g = BlockGeometry::of(&rig, false);
        let ids = rig.read(|v| v.block_ids());
        let window = rig.window;
        let none = gpui::Modifiers::none();

        set_clipboard(&mut rig, "before");
        press(&mut rig, "cmd-shift-c");
        // The copy has run (on the harness's executor) and written nothing.
        assert_eq!(
            copied(&mut rig).as_deref(),
            Some("before"),
            "{n}: false's output is empty: the clipboard keeps what it had"
        );
        assert_eq!(chosen(&rig), None, "{n}: ⇧⌘C picks nothing");

        // A click on echo hi's bar picks it.
        rig.cx
            .simulate_click(window, point(px(g.bar_x), px(g.mid(0))), none);
        let _ = rig.frame();
        assert_eq!(chosen(&rig), Some(ids[0].clone()), "{n}: echo hi picked");
        set_clipboard(&mut rig, "before");
        press(&mut rig, "cmd-shift-c");
        assert_eq!(copied(&mut rig).as_deref(), Some("hi"), "{n}");

        // Its chip: Copy command, then Copy output.
        point_at(&mut rig, point(px(g.bar_x), px(g.mid(0))));
        let _ = rig.frame();
        assert_eq!(
            rig.read(|v| v.hovered_block().map(str::to_string)),
            Some(ids[0].clone())
        );
        set_clipboard(&mut rig, "before");
        press_chip_button(&mut rig, 0);
        assert_eq!(clipboard(&mut rig).as_deref(), Some("echo hi"), "{n}");
        set_clipboard(&mut rig, "before");
        press_chip_button(&mut rig, 1);
        assert_eq!(copied(&mut rig).as_deref(), Some("hi"), "{n}");
        assert!(chip_of(&rig).is_some(), "{n}: the chip stays");
        assert_eq!(chosen(&rig), Some(ids[0].clone()), "{n}: the pick is kept");

        // The palette's Copy Command and Output (dispatched as the palette does).
        set_clipboard(&mut rig, "before");
        rig.cx
            .update_window(window, |_, window, cx| {
                window.dispatch_action(Box::new(CopyCommandAndOutput), cx)
            })
            .expect("dispatch");
        assert_eq!(copied(&mut rig).as_deref(), Some("$ echo hi\nhi"), "{n}");
        set_clipboard(&mut rig, "before");
        rig.cx
            .update_window(window, |_, window, cx| {
                window.dispatch_action(Box::new(CopyCommand), cx)
            })
            .expect("dispatch");
        let _ = rig.frame();
        assert_eq!(clipboard(&mut rig).as_deref(), Some("echo hi"), "{n}");

        // A click on sleep 9's bar picks it, and nothing scrolls.
        rig.cx
            .simulate_click(window, point(px(g.bar_x), px(g.mid(5))), none);
        let shot = rig.frame();
        save_shot("command-blocks-copy", slug, &shot);
        assert_eq!(chosen(&rig), Some(ids[2].clone()), "{n}: sleep 9 picked");
        assert_eq!(rig.read(|v| v.display_offset()), 0, "{n}: no scroll");
        set_clipboard(&mut rig, "before");
        press(&mut rig, "cmd-shift-c");
        assert_eq!(
            copied(&mut rig).as_deref(),
            Some("one\ntwo"),
            "{n}: a running block's output so far"
        );

        // Off: nothing is copied.
        rig.appearance(|a| a.command_blocks = false);
        set_clipboard(&mut rig, "before");
        press(&mut rig, "cmd-shift-c");
        assert_eq!(copied(&mut rig).as_deref(), Some("before"), "{n}: off");
    }
}

/// `command-blocks-rerun` (M11l Task 5), in both themes, the blocks text in `cat` at its live
/// prompt (the tagger knows where typing starts): `echo hi`'s chip offers Rerun; pressed, the
/// terminal shows `echo hi` after the live prompt (the tty's echo of what was sent) and the
/// cursor stays on that row (no Return). Then the cursor has left the input start: Rerun is
/// dimmed, its tooltip says why, and pressing it changes nothing.
fn command_blocks_rerun_scene() {
    use crate::block_view::Refusal;
    use crate::grid::TextScope;
    let dims = size(px(480.0), px(240.0));
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let ui = theme.ui();
        let n = format!("command-blocks-rerun-{slug}");
        let mut rig = blocks_rig(theme, dims, false, false, |_| ());
        keyed(&mut rig);
        let _ = rig.frame();
        let g = BlockGeometry::of(&rig, false);
        let ids = rig.read(|v| v.block_ids());
        let screen = |rig: &CursorRig| rig.read(|v| v.text(TextScope::Screen));
        let before = screen(&rig);

        point_at(&mut rig, point(px(g.bar_x), px(g.mid(0))));
        let _ = rig.frame();
        let (_, _, rerun) = rig.read(|v| v.chip_buttons_shot()).expect("the chip");
        assert_eq!(rerun, Ok(()), "{n}: Rerun offered at the empty prompt");
        let shown = rig.frame();
        save_shot("command-blocks-rerun", slug, &shown);
        press_chip_button(&mut rig, 2);
        wait_for(&mut rig, |v| {
            v.text(TextScope::Screen).lines().nth(8) == Some("$ echo hi")
        });
        let after = screen(&rig);
        assert_eq!(
            after.lines().nth(8),
            Some("$ echo hi"),
            "{n}: typed after the live prompt: {after:?}"
        );
        // No Return: `cat` never gets a line to copy back.
        prof::sleep(std::time::Duration::from_millis(200));
        let _ = rig.frame();
        let settled = screen(&rig);
        assert_eq!(settled, after, "{n}: nothing more came");
        assert_eq!(
            settled.lines().count(),
            before.lines().count(),
            "{n}: the cursor stayed on the prompt's row"
        );
        assert!(chip_of(&rig).is_some(), "{n}: the chip stays");

        // Typed now (the cursor left the input start): Rerun is disabled.
        let (buttons, enabled_now, rerun) = rig.read(|v| v.chip_buttons_shot()).expect("the chip");
        assert_eq!(enabled_now, [true, true, false], "{n}");
        assert_eq!(rerun, Err(Refusal::Typed), "{n}");
        let dim = rig.frame();
        save_shot("command-blocks-rerun-disabled", slug, &dim);
        // Its label in the disabled ink: no pixel of it is as strong as `text_dim`.
        let b = buttons[2].expect("Rerun laid out");
        let strongest = (0..(f32::from(b.size.width) as usize))
            .map(|dx| {
                pixel_at(
                    &dim,
                    dims,
                    f32::from(b.left()) + dx as f32,
                    f32::from(b.center().y),
                )
            })
            .map(|p| {
                crate::ui_tokens::contrast(
                    alacritty_terminal::vte::ansi::Rgb {
                        r: p[0],
                        g: p[1],
                        b: p[2],
                    },
                    ui.panel,
                )
            })
            .fold(0.0f32, f32::max);
        let enabled = crate::ui_tokens::contrast(ui.text_dim, ui.panel);
        assert!(
            strongest > 1.05 && strongest < enabled - 0.3,
            "{n}: Rerun's label {strongest:.2} vs enabled {enabled:.2}"
        );
        press_chip_button(&mut rig, 2);
        prof::sleep(std::time::Duration::from_millis(200));
        let _ = rig.frame();
        assert_eq!(screen(&rig), settled, "{n}: a disabled Rerun types nothing");
        let _ = ids;

        rerun_odd_blocks(theme, &n);
    }
}

/// The `command-blocks-rerun` checks on a second terminal: a `t=1` block (its command was too
/// long to record; no output either) and a two-line command, at a live prompt whose shell
/// asked for bracketed paste. The `t=1` block's chip has all three buttons disabled; pressing
/// them keeps the chip open and sends and copies nothing. The two-line command's Rerun sends exactly
/// `ESC [200~echo a\necho b ESC [201~`: wrapped, with no `\r`.
fn rerun_odd_blocks(theme: Theme, n: &str) {
    use crate::block_view::Refusal;
    // (`redraw=0`: a scripted shell, it never redraws its prompt, so a resize must not empty it.)
    const A: &str = "\x1b]133;A;redraw=0\x07";
    const B: &str = "\x1b]133;B\x07";
    let c = |cmd: &str| format!("\x1b]133;C;cmdline_url={cmd}\x07");
    let d0 = "\x1b]133;D;0\x07";
    let text = [
        // Row 0: a command longer than CMD_CAP (recorded as t=1, no command).
        format!("{A}$ {B}long\r\n{}{d0}", c(&"x".repeat(2100))),
        // Rows 1-2: a two-line command; rows 3-4 its output.
        format!(
            "{A}$ {B}echo a\r\n> echo b\r\n{}a\r\nb\r\n{d0}",
            c("echo%20a%0Aecho%20b")
        ),
        // Row 5: the live prompt, the shell asking for bracketed paste (as zsh does).
        format!("{A}$ {B}\x1b[?2004h"),
    ]
    .concat();
    let dims = size(px(480.0), px(240.0));
    let mut rig = CursorRig::sized(theme, &text, dims, false, (false, false, false), |_| ());
    let _keep = ClipboardKeeper::new(&rig);
    keyed(&mut rig);
    let _ = rig.frame();
    let g = BlockGeometry::of(&rig, false);
    let blocks = rig.read(|v| v.shot_blocks());
    assert_eq!(blocks.len(), 2, "{n}: {blocks:?}");
    assert_eq!(blocks[0].cmd, None, "{n}: t=1, no command");
    let view = rig.view.clone();
    let take_sent = |rig: &mut CursorRig| rig.cx.update(|cx| view.update(cx, |v, _| v.take_sent()));
    let _ = take_sent(&mut rig);

    // The t=1 block (no output either): every button disabled.
    point_at(&mut rig, point(px(g.bar_x), px(g.mid(0))));
    let _ = rig.frame();
    let (_, enabled, rerun) = rig.read(|v| v.chip_buttons_shot()).expect("the chip");
    assert_eq!(enabled, [false, false, false], "{n}: t=1 block's buttons");
    assert_eq!(rerun, Err(Refusal::NoCommand), "{n}");
    set_clipboard(&mut rig, "before");
    for i in [0, 1, 2] {
        // A copy would run on the harness's executor, which the press's frame runs; a rerun
        // would be in `take_sent` at once.
        press_chip_button(&mut rig, i);
        assert!(
            chip_of(&rig).is_some(),
            "{n}: a disabled button keeps the chip"
        );
        assert_eq!(
            rig.read(|v| v.hovered_block().map(str::to_string)),
            Some(blocks[0].id.clone()),
            "{n}: still the t=1 block's chip"
        );
        assert_eq!(
            clipboard(&mut rig).as_deref(),
            Some("before"),
            "{n}: copied nothing"
        );
        assert!(take_sent(&mut rig).is_empty(), "{n}: sent nothing");
        assert_eq!(chosen(&rig), None, "{n}: picked nothing");
    }

    // The two-line command: Rerun sends it wrapped, no `\r`.
    point_at(&mut rig, point(px(g.bar_x), px(g.mid(1))));
    let _ = rig.frame();
    assert_eq!(
        rig.read(|v| v.hovered_block().map(str::to_string)),
        Some(blocks[1].id.clone()),
        "{n}: the two-line command's chip"
    );
    let (_, enabled, rerun) = rig.read(|v| v.chip_buttons_shot()).expect("the chip");
    assert_eq!((enabled, rerun), ([true, true, true], Ok(())), "{n}");
    press_chip_button(&mut rig, 2);
    let sent = take_sent(&mut rig);
    assert_eq!(
        sent,
        b"\x1b[200~echo a\necho b\x1b[201~",
        "{n}: {:?}",
        String::from_utf8_lossy(&sent)
    );
    assert!(!sent.contains(&b'\r'), "{n}: no carriage return");
}

/// Keeps the rig's clipboard as it was before a scene that copies: saved when made, put back
/// when dropped, a failing scene's unwinding included. Made after its rig, so dropped before it.
/// That clipboard is the rig's app's own, not the Mac's: GPUI's `VisualTestPlatform` keeps one
/// per app (`visual_test.rs`), so scenes that copy never touch yours or each other's.
struct ClipboardKeeper {
    app: std::rc::Rc<gpui::AppCell>,
    saved: Option<gpui::ClipboardItem>,
}

impl ClipboardKeeper {
    fn new(rig: &CursorRig) -> Self {
        let app = rig.cx.app.clone();
        let saved = app.borrow().read_from_clipboard();
        Self { app, saved }
    }
}

impl Drop for ClipboardKeeper {
    fn drop(&mut self) {
        // Not while the app is borrowed (a panic inside an update): never a second panic.
        if let Ok(app) = self.app.try_borrow_mut() {
            let item = self
                .saved
                .take()
                .unwrap_or_else(|| gpui::ClipboardItem::new_string(String::new()));
            app.write_to_clipboard(item);
        }
    }
}
