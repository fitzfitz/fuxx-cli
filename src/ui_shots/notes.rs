//! M11ag Part 2: the agent's reason, under its edit. These scenes file notes (and light reasons)
//! in the app's `NoteStore` as the Workspace does, change a made-up `src/workspace.rs` on disk,
//! have the file tab read it again, and read where the tab placed each note (`placed_notes`).
//! The `notes-row`, `notes-open`, `notes-under-fold` and `notes-diff` scenes (M11ag Task 11) read
//! the note rows drawn (`note_rows_drawn`), where their parts were drawn and their pixels;
//! `notes-follow-scroll` reads where Follow scrolled to.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use gpui::{Bounds, Entity};

use super::branch::settle_until;
use super::*;
use crate::edit_ledger::{self, agent_mark_for};
use crate::edit_notes::{self, Light, Note};
use crate::file_tab::FileTab;
use crate::layout_model::FileMode;
use crate::row_map::Row;

/// This file, as `file!()` gives it (repo-relative under cargo): `registry::AREAS` pairs it with
/// `SCENES`, so `--changed` runs these scenes when this file changes.
pub(super) const FILE: &str = file!();

/// The files a scene here is made from.
const SOURCES: &[&str] = &[
    "src/file_tab.rs",
    "src/file_tab_notes.rs",
    "src/row_map.rs",
    "src/edit_notes.rs",
    "src/live_lines.rs",
    "src/edit_ledger.rs",
];

/// This area's scenes (`registry.rs` runs them in its `ORDER`).
pub(super) const SCENES: &[Scene] = &[
    Scene {
        name: "notes-follow-edit",
        run: notes_follow_edit,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: SOURCES,
    },
    Scene {
        name: "notes-light",
        run: notes_light,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: SOURCES,
    },
    Scene {
        name: "notes-late",
        run: notes_late,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: SOURCES,
    },
    Scene {
        name: "notes-row",
        run: notes_row,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: SOURCES,
    },
    Scene {
        name: "notes-open",
        run: notes_open,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: SOURCES,
    },
    Scene {
        name: "notes-under-fold",
        run: notes_under_fold,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: SOURCES,
    },
    Scene {
        name: "notes-diff",
        run: notes_diff,
        themes: Themes::Both,
        serial: false,
        kind: Kind::Visual,
        sources: SOURCES,
    },
    Scene {
        name: "notes-follow-scroll",
        run: notes_follow_scroll,
        themes: Themes::One,
        serial: false,
        kind: Kind::State,
        sources: SOURCES,
    },
];

/// The lines a note is about: lines 43–45 of the made-up file (0-based 42–44).
const BLOCK: [&str; 3] = [
    "    if let Some(tab) = pane.find(&path) {",
    "        return self.show(tab);",
    "    }",
];

/// `BLOCK` with its middle line changed (a later edit of the note's lines).
const CHANGED: [&str; 3] = [
    "    if let Some(tab) = pane.find(&path) {",
    "        return self.reveal(tab);",
    "    }",
];

/// A made-up `src/workspace.rs` under a scratch folder named `fuxx-cli`, one per scene (scenes
/// run side by side): its canonical path, as notes and the ledger are keyed.
fn scratch(scene: &str) -> PathBuf {
    let src = std::env::temp_dir()
        .join("fuxx-ui-shots-notes")
        .join(scene)
        .join("fuxx-cli")
        .join("src");
    std::fs::create_dir_all(&src).expect("the scratch folder");
    let src = std::fs::canonicalize(&src).unwrap_or(src);
    src.join("workspace.rs")
}

/// 100 numbered lines (`    let line_{n} = {n};`) with `block` over each 0-based line of `at`,
/// after `top` added lines.
fn text(top: usize, at: &[usize], block: &[&str]) -> String {
    text_of(100, top, at, block)
}

/// `text`, `total` lines long.
fn text_of(total: usize, top: usize, at: &[usize], block: &[&str]) -> String {
    let mut lines: Vec<String> = (1..=total)
        .map(|n| format!("    let line_{n} = {n};"))
        .collect();
    for &start in at {
        for (k, line) in block.iter().enumerate() {
            lines[start + k] = line.to_string();
        }
    }
    let mut out: String = (0..top).map(|n| format!("    // added {n}\n")).collect();
    for line in lines {
        out.push_str(&line);
        out.push('\n');
    }
    out
}

/// A note by `agent` about `anchor`, first seen at 0-based line `hint`.
fn note(agent: &str, reason: &str, anchor: &[&str], hint: usize) -> Note {
    Note {
        id: 0,
        agent: agent_mark_for(agent),
        clock: "10:42".into(),
        reason: reason.into(),
        reason_key: format!("a1:{reason}").into(),
        anchor: Arc::new(anchor.iter().map(|l| l.to_string()).collect()),
        hint,
    }
}

/// A light reason by `agent`, left now.
fn light(agent: &str, reason: &str) -> Light {
    Light {
        agent: agent_mark_for(agent),
        clock: "10:42".into(),
        reason: reason.into(),
        reason_key: format!("a1:{reason}").into(),
        at: Instant::now(),
    }
}

/// An edit by `agent` to `path` that started and finished ok now, in the app's edit ledger.
fn edited(cx: &mut VisualTestAppContext, call: &str, agent: &str, path: &Path) {
    let (call, agent, path) = (call.to_string(), agent_mark_for(agent), path.to_path_buf());
    cx.update(|cx| {
        edit_ledger::update(cx, |ledger, now| {
            ledger.edit_started(&call, agent, vec![path], now);
            ledger.edit_finished(&call, true, now);
        })
    });
}

type TabSlot = std::rc::Rc<std::cell::RefCell<Option<Entity<FileTab>>>>;

/// Opens a frozen file tab on `path` (no polling: each read is the scene's) after `setup` has
/// had the app (a note filed before the tab opens), runs `act` on it once it has read the file.
fn with_tab(
    n: &str,
    path: PathBuf,
    setup: impl FnOnce(&mut App) + 'static,
    act: impl FnOnce(&mut VisualTestAppContext, AnyWindowHandle, &Entity<FileTab>),
) {
    let at = Look {
        theme: shot_themes()[0].1,
        dims: size(px(760.0), px(360.0)),
        mode: FileMode::Plain,
    };
    with_tab_in(n, path, at, setup, act);
}

/// Where a scene's tab is drawn: the theme's name, the window's size, the tab's mode.
#[derive(Clone, Copy)]
struct Look {
    theme: &'static str,
    dims: Size<Pixels>,
    mode: FileMode,
}

/// `with_tab` in `look`, with the app's key bindings.
fn with_tab_in(
    n: &str,
    path: PathBuf,
    look: Look,
    setup: impl FnOnce(&mut App) + 'static,
    act: impl FnOnce(&mut VisualTestAppContext, AnyWindowHandle, &Entity<FileTab>),
) {
    let theme = shot_theme(look.theme);
    let slot: TabSlot = Default::default();
    let keep = slot.clone();
    let mode = look.mode;
    let build = move |window: &mut Window, cx: &mut App| -> AnyView {
        cx.set_global(shot_appearance(theme, cx));
        cx.bind_keys(crate::key_bindings());
        setup(cx);
        let tab = cx.new(|cx| {
            let mut tab = FileTab::new(path, mode, window, cx);
            tab.frozen = true;
            tab
        });
        *keep.borrow_mut() = Some(tab.clone());
        tab.into()
    };
    let n = n.to_string();
    let run = move |cx: &mut VisualTestAppContext, window: AnyWindowHandle| {
        let tab = slot.borrow().clone().expect("the file tab");
        let read = settle_until(cx, window, 10, |cx| version(cx, &tab) == Some(1));
        assert!(read, "{n}: the file is read");
        act(cx, window, &tab);
        slot.borrow_mut().take();
    };
    render_with(&theme, look.dims, build, run);
}

fn version(cx: &mut VisualTestAppContext, tab: &Entity<FileTab>) -> Option<u64> {
    cx.read(|cx| tab.read(cx).doc().map(|d| d.version))
}

fn placed(cx: &mut VisualTestAppContext, tab: &Entity<FileTab>) -> Vec<(usize, String)> {
    cx.read(|cx| tab.read(cx).placed_notes())
}

/// Frames until the notes placed are `want` (each as its line and reason).
fn expect(
    cx: &mut VisualTestAppContext,
    window: AnyWindowHandle,
    tab: &Entity<FileTab>,
    n: &str,
    want: &[(usize, &str)],
) {
    let want: Vec<(usize, String)> = want.iter().map(|(l, r)| (*l, r.to_string())).collect();
    let done = settle_until(cx, window, 10, |cx| placed(cx, tab) == want);
    assert!(
        done,
        "{n}: notes placed {:?}, not {want:?}",
        placed(cx, tab)
    );
}

/// Writes `text` over the file and has the tab read it (frames until it has a new version).
fn rewrite(
    cx: &mut VisualTestAppContext,
    window: AnyWindowHandle,
    tab: &Entity<FileTab>,
    path: &Path,
    text: &str,
    n: &str,
) {
    let before = version(cx, tab);
    std::fs::write(path, text).expect("rewrite the file");
    cx.update(|cx| tab.update(cx, |t, cx| t.check_disk(cx)));
    let read = settle_until(cx, window, 10, |cx| version(cx, tab) > before);
    assert!(read, "{n}: read again");
}

/// `notes-follow-edit` (a note following a later edit): a note about lines 43–45 sits under
/// line 44 (0-based); 3 lines added at the top: under 47 (its text found nearest its first
/// place); its text changed: no note (kept in the store); changed back, an undo: under 47
/// again. Then changed and back once more, now with a second copy of the block 40 lines further
/// down: the note stays with the copy nearest where it was first seen.
fn notes_follow_edit() {
    let n = "notes-follow-edit";
    let path = scratch(n);
    std::fs::write(&path, text(0, &[42], &BLOCK)).expect("the file");
    let filed = path.clone();
    let setup = move |cx: &mut App| {
        edit_notes::update(cx, |store| {
            store.add(filed, note("claude", "reuse an open tab", &BLOCK, 42))
        });
    };
    with_tab(n, path.clone(), setup, |cx, window, tab| {
        let why = "reuse an open tab";
        expect(cx, window, tab, n, &[(44, why)]);
        rewrite(cx, window, tab, &path, &text(3, &[42], &BLOCK), n);
        expect(cx, window, tab, n, &[(47, why)]);
        rewrite(cx, window, tab, &path, &text(3, &[42], &CHANGED), n);
        expect(cx, window, tab, n, &[]);
        rewrite(cx, window, tab, &path, &text(3, &[42], &BLOCK), n);
        expect(cx, window, tab, n, &[(47, why)]);
        rewrite(cx, window, tab, &path, &text(3, &[42], &CHANGED), n);
        expect(cx, window, tab, n, &[]);
        rewrite(cx, window, tab, &path, &text(3, &[42, 82], &BLOCK), n);
        expect(cx, window, tab, n, &[(47, why)]);
    });
    std::fs::remove_file(&path).ok();
    println!("ui-shots: {n} ok");
}

/// `notes-light`: an agy edit to the file finished ok (agy gives no text, so the store waits
/// for a light reason); the file changes on disk, two lines replaced: after the read the note
/// sits under the replaced block's last line with agy's reason. Then a pure deletion by Claude
/// (its edit's text `Removed`): the note sits under the line above the gap.
fn notes_light() {
    let n = "notes-light";
    let path = scratch(n);
    let base = text(0, &[], &[]);
    std::fs::write(&path, &base).expect("the file");
    with_tab(
        n,
        path.clone(),
        |_| {},
        |cx, window, tab| {
            edited(cx, "a1:e1", "agy", &path);
            let key = path.clone();
            cx.update(|cx| {
                edit_notes::update(cx, |store| {
                    store.expect_light(key, light("agy", "tidy the loop"))
                })
            });
            let mut lines: Vec<&str> = base.lines().collect();
            lines[20] = "    let first = 1;";
            lines[21] = "    let second = 2;";
            let replaced = lines.join("\n") + "\n";
            rewrite(cx, window, tab, &path, &replaced, n);
            expect(cx, window, tab, n, &[(21, "tidy the loop")]);

            edited(cx, "a2:e1", "claude", &path);
            let key = path.clone();
            cx.update(|cx| {
                edit_notes::update(cx, |store| {
                    store.expect_light(key, light("claude", "drop the dead lines"))
                })
            });
            // 0-based lines 50–52 go: the gap is before new line 50, the line above it is 49.
            let mut lines: Vec<&str> = replaced.lines().collect();
            lines.drain(50..53);
            let removed = lines.join("\n") + "\n";
            rewrite(cx, window, tab, &path, &removed, n);
            expect(
                cx,
                window,
                tab,
                n,
                &[(21, "tidy the loop"), (49, "drop the dead lines")],
            );
        },
    );
    std::fs::remove_file(&path).ok();
    println!("ui-shots: {n} ok");
}

/// `notes-late`: the file tab reads an edit's change before its note is filed (the Workspace
/// reads the file for the anchor off the UI thread); filing it afterwards moves the store's
/// generation, and the tab places it without reading the file again.
fn notes_late() {
    let n = "notes-late";
    let path = scratch(n);
    std::fs::write(&path, text(0, &[], &[])).expect("the file");
    with_tab(
        n,
        path.clone(),
        |_| {},
        |cx, window, tab| {
            rewrite(cx, window, tab, &path, &text(0, &[42], &BLOCK), n);
            expect(cx, window, tab, n, &[]);
            let read = version(cx, tab);
            let key = path.clone();
            cx.update(|cx| {
                edit_notes::update(cx, |store| {
                    store.add(key, note("claude", "reuse an open tab", &BLOCK, 42))
                })
            });
            expect(cx, window, tab, n, &[(44, "reuse an open tab")]);
            assert_eq!(version(cx, tab), read, "{n}: placed without another read");
        },
    );
    std::fs::remove_file(&path).ok();
    println!("ui-shots: {n} ok");
}

/// A reason about three lines long at the 760 pt tab's width, with Markdown in it that stays raw.
const LONG_WHY: &str = "Reuse an open tab instead of opening the same file twice: \
    **bold** stays as typed, since a note is plain text. You asked for no duplicate tabs, so \
    the lookup runs first and the new tab only opens when the pane has none for this path; the \
    rest of the function is unchanged and keeps its early return for the welcome pane.";

/// The note's rows as drawn, frames until there are some (a note is placed off the UI thread).
fn drawn_rows(
    cx: &mut VisualTestAppContext,
    window: AnyWindowHandle,
    tab: &Entity<FileTab>,
    n: &str,
) -> (u64, std::ops::Range<usize>) {
    let mut seen = Vec::new();
    let done = settle_until(cx, window, 10, |cx| {
        seen = cx.read(|cx| tab.read(cx).note_rows_drawn());
        !seen.is_empty()
    });
    assert!(done, "{n}: a note's rows are drawn");
    assert_eq!(seen.len(), 1, "{n}: one note {seen:?}");
    seen[0].clone()
}

fn rows_drawn(
    cx: &mut VisualTestAppContext,
    tab: &Entity<FileTab>,
) -> Vec<(u64, std::ops::Range<usize>)> {
    cx.read(|cx| tab.read(cx).note_rows_drawn())
}

fn geom(cx: &mut VisualTestAppContext, tab: &Entity<FileTab>) -> crate::file_tab::Geom {
    cx.read(|cx| tab.read(cx).geom()).expect("drawn")
}

/// Line `line`'s top down the text as the rows are now (no scroll).
fn line_top(cx: &mut VisualTestAppContext, tab: &Entity<FileTab>, line: usize) -> f32 {
    let top = cx.read(|cx| tab.read(cx).line_top(line, cx));
    f32::from(top.expect("the line is shown"))
}

/// The window point `col` cells into line `line`, half a row down it (the rows as now).
fn at_line(
    cx: &mut VisualTestAppContext,
    tab: &Entity<FileTab>,
    line: usize,
    col: usize,
) -> gpui::Point<Pixels> {
    let g = geom(cx, tab);
    let top = line_top(cx, tab, line);
    point(
        g.bounds.left() + g.text_left + g.cell_width * col as f32,
        g.bounds.top() + px(top) + g.line_height / 2.0 - g.scroll.y,
    )
}

fn drag_to(
    cx: &mut VisualTestAppContext,
    window: AnyWindowHandle,
    from: gpui::Point<Pixels>,
    to: gpui::Point<Pixels>,
) {
    use gpui::{Modifiers, MouseButton};
    let (left, none) = (MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_down(window, from, left, none);
    cx.simulate_mouse_move(window, to, Some(left), none);
    cx.simulate_mouse_up(window, to, left, none);
    settle(cx, window);
}

fn scroll_rows(
    cx: &mut VisualTestAppContext,
    window: AnyWindowHandle,
    tab: &Entity<FileTab>,
    rows: f32,
) {
    let lh = geom(cx, tab).line_height;
    cx.update(|cx| tab.update(cx, |t, cx| t.set_scroll(point(px(0.0), lh * rows), cx)));
    settle(cx, window);
}

fn focus(cx: &mut VisualTestAppContext, window: AnyWindowHandle, tab: &Entity<FileTab>) {
    use gpui::Focusable;
    cx.update_window(window, |_, window, cx| {
        tab.read(cx).focus_handle(cx).focus(window, cx)
    })
    .expect("focus");
    settle(cx, window);
}

/// Note `id`'s text as shaped and its wrapped line count.
fn shaped(cx: &mut VisualTestAppContext, tab: &Entity<FileTab>, id: u64) -> (String, usize) {
    cx.read(|cx| tab.read(cx).note_shaped(id))
        .expect("the note is shaped")
}

/// The made-up file with the block at lines 43–45 and a note about them (under line 45),
/// filed before the tab opens.
fn noted(n: &str) -> (PathBuf, impl FnOnce(&mut App) + 'static) {
    let path = scratch(n);
    std::fs::write(&path, text(0, &[42], &BLOCK)).expect("the file");
    let filed = path.clone();
    let setup = move |cx: &mut App| {
        edit_notes::update(cx, |store| {
            store.add(filed, note("claude", LONG_WHY, &BLOCK, 42))
        });
    };
    (path, setup)
}

/// `notes-row` (both themes): a note about lines 43–45 sits under line 45: two rows after its row
/// (line 46's top two rows lower), no number in the gutter on them, its bar in claude's colour,
/// the prefix in the dim text colour and the reason in the text colour, shaped as its raw
/// characters (the `**` kept). Selection, Find and ⌥↓ step around it: a drag from line 44 to 47
/// copies those lines only; a drag ending on a note row ends at its line's end; Find for a word
/// only in the note finds nothing; ⌥↓ from line 40 stops at the change.
fn notes_row() {
    for &(slug, name) in shot_themes() {
        let n = format!("notes-row-{slug}");
        let theme = shot_theme(name);
        let (ui, bg) = (theme.ui(), theme.background);
        let (path, setup) = noted("notes-row");
        let look = Look {
            theme: name,
            dims: size(px(760.0), px(360.0)),
            mode: FileMode::Plain,
        };
        let n_in = n.clone();
        with_tab_in(&n, path.clone(), look, setup, move |cx, window, tab| {
            let n = n_in;
            let (id, rows) = drawn_rows(cx, window, tab, &n);
            assert_eq!(rows, 45..47, "{n}: two rows after line 45's");
            let (words, lines) = shaped(cx, tab, id);
            assert!(lines > 2, "{n}: the reason wraps past two rows ({lines})");
            assert!(
                words.starts_with("claude \u{b7} 10:42 \u{2014} ") && words.contains("**bold**"),
                "{n}: the prefix and the raw reason: {words}"
            );
            let lh = f32::from(geom(cx, tab).line_height);
            assert_eq!(line_top(cx, tab, 44), 44.0 * lh, "{n}: line 45 stays");
            assert_eq!(
                line_top(cx, tab, 45),
                47.0 * lh,
                "{n}: line 46 two rows down"
            );
            scroll_rows(cx, window, tab, 38.0);
            let g = geom(cx, tab);
            let shot = shown(cx, window);
            keep_shot(&n, &shot);
            let dims = size(px(760.0), px(360.0));
            let quiet =
                crate::ui_tokens::ensure_contrast(ui.text_dim, bg, crate::ui_tokens::MARK_MIN);
            let row_y = |r: usize| g.bounds.top() + g.line_height * r as f32 - g.scroll.y;
            // No number on the note's rows; line 45's is drawn.
            let gutter = |r: usize| {
                let y = f32::from(row_y(r));
                let x0 = f32::from(g.bounds.left()) + 2.0;
                let w = f32::from(g.numbers_right) - 2.0;
                ink_box(
                    &shot,
                    dims,
                    (x0, y + 2.0, w, f32::from(g.line_height) - 4.0),
                    [bg.r, bg.g, bg.b],
                    [quiet.r, quiet.g, quiet.b],
                )
            };
            assert!(gutter(44).is_some(), "{n}: line 45's number is drawn");
            assert!(
                gutter(45).is_none() && gutter(46).is_none(),
                "{n}: no number on a note row"
            );
            // The bar, in claude's colour (as a mark's).
            let bar = cx
                .read(|cx| tab.read(cx).note_bar_bounds(id))
                .expect("the bar");
            assert!(
                (bar.top() - row_y(45)).abs() < px(0.6)
                    && (bar.bottom() - row_y(47)).abs() < px(0.6),
                "{n}: the bar spans the note's rows {bar:?}"
            );
            assert!(
                (f32::from(bar.size.width) - crate::ui_tokens::code::NOTE_BAR).abs() < 0.01,
                "{n}: 2 pt"
            );
            let badge = edit_ledger::agent_mark_for("claude")
                .rgb
                .expect("claude's colour");
            let claude = crate::ui_tokens::ensure_contrast(
                alacritty_terminal::vte::ansi::Rgb {
                    r: (badge >> 16) as u8,
                    g: (badge >> 8) as u8,
                    b: badge as u8,
                },
                bg,
                crate::ui_tokens::MARK_MIN,
            );
            let p = pixel_at(
                &shot,
                dims,
                f32::from(bar.center().x),
                f32::from(bar.center().y),
            );
            assert!(
                near(p, claude),
                "{n}: the bar {p:?}, not claude's {claude:?}"
            );
            // The prefix dim, the reason in the text colour.
            let dim =
                crate::ui_tokens::ensure_contrast(ui.text_dim, bg, crate::ui_tokens::TEXT_MIN);
            let x0 = f32::from(g.bounds.left() + g.text_left);
            let row = |r: usize| {
                Bounds::new(
                    point(g.bounds.left(), row_y(r)),
                    size(g.bounds.size.width, g.line_height),
                )
            };
            let prefix = strongest(&shot, dims, bg, row(45), x0, x0 + 36.0).expect("the prefix");
            assert!(
                gap(prefix, dim) < gap(prefix, ui.text),
                "{n}: the prefix {prefix:?} is dim"
            );
            let reason = strongest(&shot, dims, bg, row(46), x0, x0 + 300.0).expect("the reason");
            assert!(
                gap(reason, ui.text) < gap(reason, dim),
                "{n}: the reason {reason:?} in text"
            );
            // A drag from line 44 to 47 copies lines 44–47 only.
            let line = |k: usize| format!("    let line_{k} = {k};");
            let (from, to) = (at_line(cx, tab, 43, 0), at_line(cx, tab, 46, 40));
            drag_to(cx, window, from, to);
            let copied = cx.read(|cx| tab.read(cx).selected_text());
            let lines44_47 = [BLOCK[1], BLOCK[2], &line(46), &line(47)].join("\n");
            assert_eq!(copied, lines44_47, "{n}: lines 44–47, no note words");
            // Ending on a note row: its line's end.
            let note_row = point(from.x + px(100.0), row_y(46) + g.line_height / 2.0);
            drag_to(cx, window, from, note_row);
            let copied = cx.read(|cx| tab.read(cx).selected_text());
            assert_eq!(
                copied,
                [BLOCK[1], BLOCK[2]].join("\n"),
                "{n}: to line 45's end"
            );
            // Find: a word only in the note finds nothing.
            focus(cx, window, tab);
            cx.simulate_keystrokes(window, "cmd-f");
            cx.simulate_input(window, "bold");
            settle(cx, window);
            assert_eq!(
                cx.read(|cx| tab.read(cx).find_label()),
                "No results",
                "{n}: Find"
            );
            cx.simulate_keystrokes(window, "escape");
            settle(cx, window);
            // ⌥↓ from line 40: the change at line 43 (1-based 43–45 changed).
            let changes = (43..=45)
                .map(|l| (l, crate::git_status::LineChange::Changed))
                .collect();
            cx.update(|cx| tab.update(cx, |t, cx| t.set_changes(changes, cx)));
            cx.update(|cx| tab.update(cx, |t, cx| t.set_current_for_shots(Some(39), cx)));
            focus(cx, window, tab);
            cx.simulate_keystrokes(window, "alt-down");
            settle(cx, window);
            assert_eq!(
                cx.read(|cx| tab.read(cx).current_line()),
                Some(42),
                "{n}: ⌥↓ to the change"
            );
        });
        std::fs::remove_file(&path).ok();
        println!("ui-shots: {n} ok");
    }
}

/// `notes-open` (both themes): line 40 at the top; the note's "more" opens it (all its rows
/// drawn), its "less" folds it to two; line 40 stays at the top. "more" is drawn dim at the end
/// of the note's last row shown. Fix round 1: a reason whose last line runs into the box "less"
/// takes (found by lengthening the reason a word at a time), opened, takes one row more: "less"
/// alone on it, the last line's words drawn where "less" would have been.
fn notes_open() {
    for &(slug, name) in shot_themes() {
        let n = format!("notes-open-{slug}");
        let theme = shot_theme(name);
        let (ui, bg) = (theme.ui(), theme.background);
        let (path, setup) = noted("notes-open");
        let dims = size(px(760.0), px(360.0));
        let look = Look {
            theme: name,
            dims,
            mode: FileMode::Plain,
        };
        let n_in = n.clone();
        let filed = path.clone();
        with_tab_in(&n, path.clone(), look, setup, move |cx, window, tab| {
            let n = n_in;
            let (id, rows) = drawn_rows(cx, window, tab, &n);
            let (_, lines) = shaped(cx, tab, id);
            assert_eq!(rows, 45..47, "{n}: folded");
            scroll_rows(cx, window, tab, 39.0);
            let top =
                |cx: &mut VisualTestAppContext| cx.read(|cx| tab.read(cx).top_line_for_shots());
            assert_eq!(top(cx), Some(39), "{n}: line 40 at the top");
            let more = cx
                .read(|cx| tab.read(cx).note_more_bounds(id))
                .expect("more");
            let g = geom(cx, tab);
            let last = g.bounds.top() + g.line_height * 46.0 - g.scroll.y;
            assert!(
                (more.top() - last).abs() < px(0.6),
                "{n}: on the last row shown {more:?}"
            );
            let right = g.bounds.right() - px(24.0);
            assert!(
                // The room is whole points (`ShapeKey::width`): within one of the edge.
                (more.right() - right).abs() < px(1.0),
                "{n}: at its right end {more:?}"
            );
            let shot = shown(cx, window);
            let dim =
                crate::ui_tokens::ensure_contrast(ui.text_dim, bg, crate::ui_tokens::TEXT_MIN);
            let p = strongest(
                &shot,
                dims,
                bg,
                more,
                f32::from(more.left()),
                f32::from(more.right()),
            )
            .expect("more's ink");
            assert!(gap(p, dim) < gap(p, ui.text), "{n}: more is dim {p:?}");
            cx.simulate_click(window, more.center(), gpui::Modifiers::none());
            settle(cx, window);
            assert_eq!(
                rows_drawn(cx, tab),
                [(id, 45..45 + lines)],
                "{n}: opened: all rows"
            );
            assert_eq!(top(cx), Some(39), "{n}: line 40 still at the top");
            keep_shot(&n, &shown(cx, window));
            let less = cx
                .read(|cx| tab.read(cx).note_more_bounds(id))
                .expect("less");
            let g = geom(cx, tab);
            let last = g.bounds.top() + g.line_height * (44 + lines) as f32 - g.scroll.y;
            assert!(
                (less.top() - last).abs() < px(0.6),
                "{n}: less on the last row {less:?}"
            );
            cx.simulate_click(window, less.center(), gpui::Modifiers::none());
            settle(cx, window);
            assert_eq!(rows_drawn(cx, tab), [(id, 45..47)], "{n}: folded again");
            assert_eq!(top(cx), Some(39), "{n}: line 40 still at the top");
            assert!(
                cx.read(|cx| tab.read(cx).selection())
                    .is_none_or(|(a, b)| a == b),
                "{n}: no selection"
            );
            under_less(cx, window, tab, &filed, less.size.width, &n);
        });
        std::fs::remove_file(&path).ok();
        println!("ui-shots: {n} ok");
    }
}

/// `notes-open`'s second part: newer notes about the same lines (each hides the last) with the
/// reason a word longer each time, until its last line ends inside the box "less" (`label`
/// wide, `NOTE_MORE_GAP` before it) takes at the room's end; opened, it has a row more than its
/// lines, "less" sits on that row with no word before it, and the last line's words reach into
/// the box's place.
fn under_less(
    cx: &mut VisualTestAppContext,
    window: AnyWindowHandle,
    tab: &Entity<FileTab>,
    path: &Path,
    label: Pixels,
    n: &str,
) {
    use crate::ui_tokens::code::NOTE_MORE_GAP;
    let mut found = None;
    for k in 1..=80 {
        let reason = format!("{LONG_WHY}{}", " ab".repeat(k));
        let key = path.to_path_buf();
        let why = reason.clone();
        cx.update(|cx| {
            edit_notes::update(cx, |store| store.add(key, note("claude", &why, &BLOCK, 42)))
        });
        let mut seen = None;
        let done = settle_until(cx, window, 10, |cx| {
            let t = tab.read_with(cx, |t, _| {
                let rows = t.note_rows_drawn();
                let id = rows.first()?.0;
                let (words, _) = t.note_shaped(id)?;
                words.ends_with(&reason).then(|| (id, t.note_last_line(id)))
            });
            seen = t;
            seen.is_some()
        });
        assert!(done, "{n}: the longer note is drawn");
        let (id, last) = seen.expect("seen");
        let (lines, end, room) = last.expect("its last line");
        if lines > 2 && end > room - f32::from(label) - NOTE_MORE_GAP {
            found = Some((id, lines));
            break;
        }
    }
    let (id, lines) = found.expect("a reason whose last line runs under less");
    let rows = rows_drawn(cx, tab);
    assert_eq!(rows, [(id, 45..47)], "{n}: folded, two rows as before");
    let more = cx
        .read(|cx| tab.read(cx).note_more_bounds(id))
        .expect("more");
    cx.simulate_click(window, more.center(), gpui::Modifiers::none());
    settle(cx, window);
    assert_eq!(
        rows_drawn(cx, tab),
        [(id, 45..46 + lines)],
        "{n}: opened, a row more than its {lines} lines"
    );
    let less = cx
        .read(|cx| tab.read(cx).note_more_bounds(id))
        .expect("less");
    let g = geom(cx, tab);
    let row_y = |r: usize| g.bounds.top() + g.line_height * r as f32 - g.scroll.y;
    let extra = row_y(45 + lines);
    assert!(
        (less.top() - extra).abs() < px(0.6),
        "{n}: less on the row of its own {less:?}"
    );
    let shot = shown(cx, window);
    keep_shot(&format!("{n}-under-less"), &shot);
    let theme = cx.read(|cx| cx.global::<crate::appearance::Appearance>().theme);
    let (bg, ink) = (theme.background, theme.ui().text);
    let (bg, ink) = ([bg.r, bg.g, bg.b], [ink.r, ink.g, ink.b]);
    let dims = size(px(760.0), px(360.0));
    let lh = f32::from(g.line_height);
    // The last line's words reach into where "less" would have been drawn over them.
    let last = f32::from(row_y(44 + lines));
    let x0 = f32::from(less.left()) - NOTE_MORE_GAP;
    let reach = (x0, last + 2.0, f32::from(less.right()) - x0, lh - 4.0);
    assert!(
        ink_box(&shot, dims, reach, bg, ink).is_some(),
        "{n}: the last line's words are drawn under less's place"
    );
    // "less"'s own row has no word before it.
    let text_left = f32::from(g.bounds.left() + g.text_left);
    let before = (
        text_left,
        f32::from(extra) + 2.0,
        f32::from(less.left()) - 2.0 - text_left,
        lh - 4.0,
    );
    assert!(
        ink_box(&shot, dims, before, bg, ink).is_none(),
        "{n}: nothing before less on its row"
    );
}

/// `notes-under-fold` (both themes): folding the block that holds line 45 (lines 43–45) hides
/// the note's rows with it (line 46 right under line 43); unfolding brings them back.
fn notes_under_fold() {
    for &(slug, name) in shot_themes() {
        let n = format!("notes-under-fold-{slug}");
        let (path, setup) = noted("notes-under-fold");
        let look = Look {
            theme: name,
            dims: size(px(760.0), px(360.0)),
            mode: FileMode::Plain,
        };
        let n_in = n.clone();
        with_tab_in(&n, path.clone(), look, setup, move |cx, window, tab| {
            let n = n_in;
            let (id, rows) = drawn_rows(cx, window, tab, &n);
            assert_eq!(rows, 45..47, "{n}");
            scroll_rows(cx, window, tab, 36.0);
            cx.update(|cx| tab.update(cx, |t, cx| t.fold_for_shots(&[42], cx)));
            settle(cx, window);
            assert_eq!(
                cx.read(|cx| tab.read(cx).folded_lines()),
                [42],
                "{n}: folded"
            );
            assert!(
                rows_drawn(cx, tab).is_empty(),
                "{n}: the note hides with the fold"
            );
            let lh = f32::from(geom(cx, tab).line_height);
            assert_eq!(
                line_top(cx, tab, 45),
                43.0 * lh,
                "{n}: line 46 under line 43"
            );
            keep_shot(&n, &shown(cx, window));
            cx.update(|cx| tab.update(cx, |t, cx| t.unfold_all_for_shots(cx)));
            settle(cx, window);
            assert_eq!(rows_drawn(cx, tab), [(id, 45..47)], "{n}: back");
        });
        std::fs::remove_file(&path).ok();
        println!("ui-shots: {n} ok");
    }
}

/// A reason long enough to wrap past two rows in a wide window.
fn longer_why() -> String {
    format!("{LONG_WHY} {LONG_WHY}")
}

/// The diff of `text(0, &[42], &BLOCK)` against the numbered lines: old lines 43–45 removed,
/// `BLOCK` added in their place (diff rows 42–44 removed, 45–47 added; new line 45 is row 47).
fn noted_diff() -> String {
    let mut u =
        String::from("--- a/src/workspace.rs\n+++ b/src/workspace.rs\n@@ -1,100 +1,100 @@\n");
    for k in 1..=100usize {
        if (43..=45).contains(&k) {
            u.push_str(&format!("-    let line_{k} = {k};\n"));
            if k == 45 {
                for line in BLOCK {
                    u.push_str(&format!("+{line}\n"));
                }
            }
        } else {
            u.push_str(&format!("     let line_{k} = {k};\n"));
        }
    }
    u
}

/// `notes-diff` (both themes): Diff mode. Unified: the note's rows follow the row of new line 45
/// (diff row 47). Side by side: they follow the pair holding it and the note's text runs across
/// the full width (ink right of the middle). Wrap on, in a narrower window: the note wraps at
/// the visible width (more lines than in the wide window; as many as with Wrap off there) and
/// its ink stays inside the card.
fn notes_diff() {
    for &(slug, name) in shot_themes() {
        let theme = shot_theme(name);
        let (ui, bg) = (theme.ui(), theme.background);
        let wide_lines = std::rc::Rc::new(std::cell::Cell::new(0));
        for (dims, part) in [
            (size(px(1900.0), px(600.0)), "wide"),
            (size(px(900.0), px(600.0)), "narrow"),
        ] {
            let n = format!("notes-diff-{part}-{slug}");
            let path = scratch(&format!("notes-diff-{part}"));
            std::fs::write(&path, text(0, &[42], &BLOCK)).expect("the file");
            let filed = path.clone();
            let why = longer_why();
            let setup = move |cx: &mut App| {
                edit_notes::update(cx, |store| {
                    store.add(filed, note("claude", &why, &BLOCK, 42))
                });
            };
            let look = Look {
                theme: name,
                dims,
                mode: FileMode::Diff,
            };
            let n_in = n.clone();
            let wide_lines = wide_lines.clone();
            with_tab_in(&n, path.clone(), look, setup, move |cx, window, tab| {
                let n = n_in;
                cx.update(|cx| tab.update(cx, |t, cx| t.set_diff_for_shots(&noted_diff(), cx)));
                let (id, rows) = drawn_rows(cx, window, tab, &n);
                let row = |cx: &mut VisualTestAppContext, r: usize| {
                    cx.read(|cx| tab.read(cx).row_for_shots(r))
                };
                assert_eq!(
                    row(cx, rows.start - 1),
                    Some(Row::Line { line: 47, cell0: 0 }),
                    "{n}: under new line 45's row"
                );
                assert_eq!(rows.len(), 2, "{n}: folded");
                let (_, lines) = shaped(cx, tab, id);
                if part == "wide" {
                    wide_lines.set(lines);
                    // Side by side: after the pair holding new line 45.
                    cx.update(|cx| {
                        tab.update(cx, |t, cx| {
                            t.set_side_by_side_for_shots(true);
                            cx.notify();
                        })
                    });
                    settle(cx, window);
                    assert!(
                        cx.read(|cx| tab.read(cx).side_by_side_for_shots().1),
                        "{n}: side by side"
                    );
                    let (_, rows) = drawn_rows(cx, window, tab, &n);
                    match row(cx, rows.start - 1) {
                        Some(Row::Pair {
                            new: Some((47, _)), ..
                        }) => {}
                        other => panic!("{n}: under the pair holding row 47, not {other:?}"),
                    }
                    let top = cx.read(|cx| tab.read(cx).line_top(47, cx)).expect("row 47");
                    cx.update(|cx| {
                        tab.update(cx, |t, cx| {
                            t.set_scroll(point(px(0.0), (top - px(120.0)).max(px(0.0))), cx)
                        })
                    });
                    settle(cx, window);
                    let g = geom(cx, tab);
                    let shot = shown(cx, window);
                    keep_shot(&n, &shot);
                    let middle = g.bounds.left() + g.middle().expect("a middle");
                    let y = g.bounds.top()
                        + px(cx
                            .read(|cx| tab.read(cx).line_top(47, cx))
                            .map_or(0.0, f32::from))
                        + g.line_height
                        - g.scroll.y;
                    let area = (
                        f32::from(middle) + 20.0,
                        f32::from(y) + 2.0,
                        300.0,
                        f32::from(g.line_height) - 4.0,
                    );
                    let ink = ink_box(
                        &shot,
                        dims,
                        area,
                        [bg.r, bg.g, bg.b],
                        [ui.text.r, ui.text.g, ui.text.b],
                    );
                    assert!(ink.is_some(), "{n}: the note runs past the middle");
                    let bar = cx
                        .read(|cx| tab.read(cx).note_bar_bounds(id))
                        .expect("the bar");
                    assert!(bar.left() < middle, "{n}: the bar on the left half");
                } else {
                    let wide = wide_lines.get();
                    assert!(
                        lines > wide,
                        "{n}: narrower, more lines ({lines} vs {wide})"
                    );
                    let wrap = |cx: &mut VisualTestAppContext, on: bool| {
                        cx.update(|cx| {
                            cx.update_global::<crate::appearance::Appearance, _>(|a, _| {
                                a.file_wrap = on
                            })
                        });
                        cx.update(|cx| tab.update(cx, |_, cx| cx.notify()));
                        settle(cx, window);
                        shaped(cx, tab, id).1
                    };
                    assert_eq!(wrap(cx, true), lines, "{n}: Wrap on: the same width rule");
                    // Opened: every line drawn, inside the card.
                    let top = cx.read(|cx| tab.read(cx).line_top(47, cx)).expect("row 47");
                    cx.update(|cx| {
                        tab.update(cx, |t, cx| {
                            t.set_scroll(point(px(0.0), (top - px(120.0)).max(px(0.0))), cx)
                        })
                    });
                    settle(cx, window);
                    let more = cx
                        .read(|cx| tab.read(cx).note_more_bounds(id))
                        .expect("more");
                    cx.simulate_click(window, more.center(), gpui::Modifiers::none());
                    settle(cx, window);
                    let (_, rows) = drawn_rows(cx, window, tab, &n);
                    assert_eq!(rows.len(), lines, "{n}: opened");
                    let g = geom(cx, tab);
                    let shot = shown(cx, window);
                    keep_shot(&n, &shot);
                    let card_right = g.bounds.right() - px(crate::ui_tokens::code::CARD_INSET);
                    let y = g.bounds.top()
                        + px(cx
                            .read(|cx| tab.read(cx).line_top(47, cx))
                            .map_or(0.0, f32::from))
                        + g.line_height
                        - g.scroll.y;
                    let x = f32::from(card_right) - 20.0;
                    let area = (
                        x,
                        f32::from(y),
                        18.0,
                        f32::from(g.line_height) * lines as f32,
                    );
                    let ink = ink_box(
                        &shot,
                        dims,
                        area,
                        [bg.r, bg.g, bg.b],
                        [ui.text.r, ui.text.g, ui.text.b],
                    );
                    assert!(
                        ink.is_none(),
                        "{n}: the note's words stay inside its room: {ink:?}"
                    );
                    assert_eq!(wrap(cx, false), lines, "{n}: Wrap off: as many");
                }
            });
            std::fs::remove_file(&path).ok();
            println!("ui-shots: {n} ok");
        }
    }
}

/// `notes-follow-scroll`: Follow on, an agent's change at lines 401–403 of 600 with a note under
/// its last line and another under line 397, above it: Follow scrolls the change's first line to
/// 3 rows from the top counting the rows as drawn (the note above it included), so a line, not
/// a note row, is at the top.
fn notes_follow_scroll() {
    use crate::live_lines::{Compared, LineRecord, Mark, Sure, Who};
    let n = "notes-follow-scroll";
    let path = scratch(n);
    let other = ["    let a = 1;", "        let b = 2;", "    let c = 3;"];
    let mut lines: Vec<String> = text_of(600, 0, &[400], &BLOCK)
        .lines()
        .map(String::from)
        .collect();
    for (k, l) in other.iter().enumerate() {
        lines[394 + k] = l.to_string();
    }
    std::fs::write(&path, lines.join("\n") + "\n").expect("the file");
    let filed = path.clone();
    let setup = move |cx: &mut App| {
        edit_notes::update(cx, |store| {
            store.add(filed.clone(), note("claude", "first", &other, 394));
            store.add(filed, note("claude", "second", &BLOCK, 400));
        });
    };
    with_tab(n, path.clone(), setup, |cx, window, tab| {
        let done = settle_until(cx, window, 10, |cx| rows_drawn(cx, tab).len() == 2);
        assert!(done, "{n}: two notes drawn: {:?}", rows_drawn(cx, tab));
        let now = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(2_000_000_000);
        let mut record = LineRecord::baseline(600);
        let c = Compared {
            new_of_old: (0..600)
                .map(|i| (!(400..403).contains(&i)).then_some(i))
                .collect(),
            blocks: std::iter::once(400..403).collect(),
        };
        let who = Who::Agent(edit_ledger::agent_mark_for("claude"));
        record.apply(
            &c,
            600,
            Mark {
                who,
                sure: Sure::Exact,
                when: now,
            },
        );
        cx.update(|cx| tab.update(cx, |t, _| t.set_live_for_shot(record, now, Some(true))));
        settle(cx, window);
        cx.update(|cx| tab.update(cx, |t, cx| t.follow_for_shots(cx)));
        settle(cx, window);
        let g = geom(cx, tab);
        let top_row = (g.scroll.y / g.line_height).round() as usize;
        // The first note's row (one line) under line 397 (0-based 396) pushes line 401 (0-based
        // 400) to row 401; three rows above it is row 398: line 398 (0-based 397). Counting no
        // note row, the top would be row 397: the note's.
        assert_eq!(top_row, 398, "{n}: the change three rows from the top");
        let note = cx.read(|cx| tab.read(cx).row_for_shots(397));
        assert!(
            matches!(note, Some(Row::Note { .. })),
            "{n}: the note's row above it: {note:?}"
        );
        let row = cx.read(|cx| tab.read(cx).row_for_shots(top_row));
        assert_eq!(
            row,
            Some(Row::Line {
                line: 397,
                cell0: 0
            }),
            "{n}: a line at the top"
        );
        assert!(
            cx.read(|cx| tab.read(cx).follow_on()),
            "{n}: Follow stays on"
        );
    });
    std::fs::remove_file(&path).ok();
    println!("ui-shots: {n} ok");
}
