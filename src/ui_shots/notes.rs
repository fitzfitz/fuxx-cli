//! M11ag Part 2: the agent's reason, under its edit. These scenes file notes (and light reasons)
//! in the app's `NoteStore` as the Workspace does, change a made-up `src/workspace.rs` on disk,
//! have the file tab read it again, and read where the tab placed each note (`placed_notes`):
//! state, not pixels (note rows are drawn from M11ag Task 11).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use gpui::Entity;

use super::branch::settle_until;
use super::*;
use crate::edit_ledger::{self, agent_mark_for};
use crate::edit_notes::{self, Light, Note};
use crate::file_tab::FileTab;
use crate::layout_model::FileMode;

/// This file, as `file!()` gives it (repo-relative under cargo): `registry::AREAS` pairs it with
/// `SCENES`, so `--changed` runs these scenes when this file changes.
pub(super) const FILE: &str = file!();

/// The files a scene here is made from.
const SOURCES: &[&str] = &[
    "src/file_tab.rs",
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
    let mut lines: Vec<String> = (1..=100)
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
    let theme = shot_theme(shot_themes()[0].1);
    let slot: TabSlot = Default::default();
    let keep = slot.clone();
    let build = move |window: &mut Window, cx: &mut App| -> AnyView {
        cx.set_global(shot_appearance(theme, cx));
        setup(cx);
        let tab = cx.new(|cx| {
            let mut tab = FileTab::new(path, FileMode::Plain, window, cx);
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
    render_with(&theme, size(px(760.0), px(360.0)), build, run);
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
