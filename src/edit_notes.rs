//! The agent's own reason for an edit (M11ag Part 2): the turn's last text before the edit, kept
//! as plain characters to show under the changed lines.

use std::collections::HashMap;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use gpui::{BorrowAppContext, Global};

use crate::edit_ledger::KEEP;
use crate::live_lines::AgentMark;
use crate::providers::EditText;

/// A reason is cut at this many characters.
pub const REASON_MAX: usize = 2000;

/// The text as a reason: whitespace runs become one space, the ends are trimmed, and it is cut
/// at `REASON_MAX` characters (on a character boundary). `None` if nothing is left. The
/// characters stay raw: no Markdown is read.
pub fn reason_text(raw: &str) -> Option<Arc<str>> {
    let mut out = String::new();
    for word in raw.split_whitespace() {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(word);
        if out.chars().count() > REASON_MAX {
            break;
        }
    }
    if let Some((at, _)) = out.char_indices().nth(REASON_MAX) {
        out.truncate(at);
        out.truncate(out.trim_end().len());
    }
    if out.is_empty() {
        None
    } else {
        Some(out.into())
    }
}

/// What an edit that finished OK carries to the Workspace: the agent's reason, the id of the
/// message it came from (one reason, several edits), and the new text of each change.
#[derive(Clone, Debug, PartialEq)]
pub struct EditNote {
    pub reason: Arc<str>,
    pub reason_id: u64,
    pub texts: Vec<EditText>,
}

/// At most this many notes are kept for one file; past it the oldest goes.
pub const PER_FILE: usize = 200;
/// At most this many files have notes; past it the file added to least recently goes.
pub const FILES: usize = 2000;

/// One reason filed under changed lines: who, when (a clock string made once), the reason, which
/// reason it is (`"a{slot}:{reason_id}"`, so one reason shows once per file), and where in the
/// file the lines it is about were: their text and the first one's 0-based line.
#[derive(Clone, Debug, PartialEq)]
pub struct Note {
    pub id: u64,
    pub agent: AgentMark,
    pub clock: Arc<str>,
    pub reason: Arc<str>,
    pub reason_key: Arc<str>,
    pub anchor: Arc<Vec<String>>,
    pub hint: usize,
}

/// Why a reason waits the light way, which says what change it may be filed under.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LightKind {
    /// The agent gives no edit text: the reason is about its next change, whatever it is.
    NoText,
    /// The edit only removed text: the reason is about a change that only removed lines.
    Removed,
}

/// A reason waiting for a file tab to say which lines it is about (the light way: a pure
/// deletion, or an agent that gives no text).
#[derive(Clone, Debug)]
pub struct Light {
    pub kind: LightKind,
    pub agent: AgentMark,
    pub clock: Arc<str>,
    pub reason: Arc<str>,
    pub reason_key: Arc<str>,
    pub at: Instant,
}

struct FileNotes {
    notes: Arc<Vec<Note>>,
    generation: u64,
    /// When (in adds, store-wide) this file was last added to.
    last_added: u64,
}

/// The session's notes, per file, and the light reasons waiting. Not saved.
#[derive(Default)]
pub struct NoteStore {
    files: HashMap<PathBuf, FileNotes>,
    lights: HashMap<PathBuf, Vec<Light>>,
    next_id: u64,
    adds: u64,
}

impl Global for NoteStore {}

impl NoteStore {
    /// Files `note` under `path` (its id is the store's); the oldest of that file goes over
    /// `PER_FILE`, the file added to least recently goes over `FILES`.
    pub fn add(&mut self, path: PathBuf, mut note: Note) {
        self.next_id += 1;
        self.adds += 1;
        note.id = self.next_id;
        let entry = self.files.entry(path).or_insert_with(|| FileNotes {
            notes: Arc::new(Vec::new()),
            generation: 0,
            last_added: 0,
        });
        let notes = Arc::make_mut(&mut entry.notes);
        notes.push(note);
        if notes.len() > PER_FILE {
            notes.remove(0);
        }
        entry.generation += 1;
        entry.last_added = self.adds;
        if self.files.len() > FILES
            && let Some(oldest) = self
                .files
                .iter()
                .min_by_key(|(_, f)| f.last_added)
                .map(|(p, _)| p.clone())
        {
            self.files.remove(&oldest);
        }
    }

    /// The notes of `path`, oldest first.
    pub fn notes(&self, path: &Path) -> Arc<Vec<Note>> {
        self.files
            .get(path)
            .map_or_else(|| Arc::new(Vec::new()), |f| f.notes.clone())
    }

    /// How many adds `path` has had (a change tells a view to look again).
    pub fn generation(&self, path: &Path) -> u64 {
        self.files.get(path).map_or(0, |f| f.generation)
    }

    /// Leaves a reason for `path` to be taken by a file tab that sees `light.agent` change it.
    pub fn expect_light(&mut self, path: PathBuf, light: Light) {
        let pending = self.lights.entry(path).or_default();
        let now = light.at;
        pending.retain(|l| fresh(l, now));
        pending.push(light);
    }

    /// The newest pending reason of `agent` for `path` not older than `KEEP` at `now` that a
    /// change may take: any `NoText` one, and a `Removed` one only when the change removed
    /// lines alone (`removal_only`). Every pending one of that path and agent it could have
    /// taken is gone with it; a `Removed` one not taken stays, to expire.
    pub fn take_light(
        &mut self,
        path: &Path,
        agent: &str,
        removal_only: bool,
        now: Instant,
    ) -> Option<Light> {
        let pending = self.lights.get_mut(path)?;
        let mut taken: Option<Light> = None;
        pending.retain(|l| {
            if &*l.agent.name != agent || (l.kind == LightKind::Removed && !removal_only) {
                return true;
            }
            if fresh(l, now) && taken.as_ref().is_none_or(|t| l.at >= t.at) {
                taken = Some(l.clone());
            }
            false
        });
        if pending.is_empty() {
            self.lights.remove(path);
        }
        taken
    }
}

/// Whether `light` is not older than `KEEP` at `now`.
fn fresh(light: &Light, now: Instant) -> bool {
    now.saturating_duration_since(light.at) <= KEEP
}

/// Runs `f` on the app's store. A scene that never set the global gets an empty one.
pub fn update(cx: &mut gpui::App, f: impl FnOnce(&mut NoteStore)) {
    if !cx.has_global::<NoteStore>() {
        cx.set_global(NoteStore::default());
    }
    cx.update_global::<NoteStore, _>(|store, _| f(store));
}

/// Whether any of `texts` is looked for in the file (so the file is read at the finish): a pure
/// removal, a call with no note and an agent that gives no text need no read.
pub fn needs_read(texts: &[EditText]) -> bool {
    texts
        .iter()
        .any(|t| matches!(t, EditText::Inserted(_) | EditText::WholeFile))
}

/// The light reason an edit leaves for a path, from its `texts` and whether one of them was
/// found there (`anchored`): an agent that gives no text leaves a `NoText` one; an edit whose
/// texts found no lines and that removed text leaves a `Removed` one; any other leaves none (a
/// text found is a note already; one not found, or a call with no note, gives nothing).
pub fn light_kind(texts: &[EditText], anchored: bool) -> Option<LightKind> {
    if texts.is_empty() {
        Some(LightKind::NoText)
    } else if !anchored && texts.contains(&EditText::Removed) {
        Some(LightKind::Removed)
    } else {
        None
    }
}

/// Where `edit`'s new text is in `text` (the file as read at the finish): `Inserted` gives the
/// whole lines its first occurrence spans (without `\r`) and the first one's 0-based line;
/// `WholeFile` the first line and 0 (None for an empty file); `Removed`, and a text that is not
/// found, None.
pub fn anchor_in(text: &str, edit: &EditText) -> Option<(Vec<String>, usize)> {
    let (first, last) = match edit {
        EditText::Removed | EditText::NoNote => return None,
        EditText::WholeFile => {
            let line = text.lines().next()?;
            return Some((vec![line.trim_end_matches('\r').to_string()], 0));
        }
        EditText::Inserted(new) => {
            if new.is_empty() {
                return None;
            }
            let at = text.find(new.as_str())?;
            let line_of = |byte: usize| {
                text.as_bytes()[..byte]
                    .iter()
                    .filter(|&&b| b == b'\n')
                    .count()
            };
            (line_of(at), line_of(at + new.len() - 1))
        }
    };
    let lines: Vec<String> = text
        .lines()
        .skip(first)
        .take(last - first + 1)
        .map(|l| l.trim_end_matches('\r').to_string())
        .collect();
    (!lines.is_empty()).then_some((lines, first))
}

/// Where `anchor` is in `lines`: of every place its lines are found together, the one starting
/// nearest `hint` (a tie: the upper one). None when it isn't there (or is empty).
pub fn find_anchor(lines: &[&str], anchor: &[String], hint: usize) -> Option<Range<usize>> {
    if anchor.is_empty() || anchor.len() > lines.len() {
        return None;
    }
    let mut best: Option<usize> = None;
    for start in 0..=lines.len() - anchor.len() {
        let here = lines[start..start + anchor.len()]
            .iter()
            .zip(anchor)
            .all(|(line, want)| *line == want);
        if here && best.is_none_or(|b| start.abs_diff(hint) < b.abs_diff(hint)) {
            best = Some(start);
        }
    }
    best.map(|start| start..start + anchor.len())
}

/// A note placed in a text: the 0-based line it sits under (its anchor's last line).
#[derive(Clone, Debug, PartialEq)]
pub struct Placed {
    pub note: Note,
    pub under: usize,
}

/// Where `notes` (oldest first, as the store keeps them) sit in `lines`, sorted by line.
/// None when it took past `deadline`.
///
/// Newest first: a note whose anchor isn't found is left out (it stays in the store, and comes
/// back with its text); one whose lines overlap a newer placed note's is hidden by it; then of
/// each reason only the topmost note stays.
pub fn place(notes: &[Note], lines: &[&str], deadline: Instant) -> Option<Vec<Placed>> {
    let mut found: Vec<(Range<usize>, &Note)> = Vec::new();
    for note in notes.iter().rev() {
        if Instant::now() >= deadline {
            return None;
        }
        let Some(at) = find_anchor(lines, &note.anchor, note.hint) else {
            continue;
        };
        let hidden = found
            .iter()
            .any(|(newer, _)| newer.start < at.end && at.start < newer.end);
        if !hidden {
            found.push((at, note));
        }
    }
    found.sort_by_key(|(at, _)| at.start);
    let mut placed: Vec<Placed> = Vec::new();
    for (at, note) in found {
        if placed.iter().any(|p| p.note.reason_key == note.reason_key) {
            continue;
        }
        placed.push(Placed {
            note: note.clone(),
            under: at.end - 1,
        });
    }
    placed.sort_by_key(|p| p.under);
    Some(placed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reason_text_collapses_whitespace_and_caps_on_a_char_boundary() {
        let long = "é".repeat(3000);
        let cut = reason_text(&long).expect("text");
        assert_eq!(cut.chars().count(), 2000);
        assert!(cut.chars().all(|c| c == 'é'));
        assert_eq!(reason_text("a \n\n  b").as_deref(), Some("a b"));
        assert_eq!(reason_text("   "), None);
        assert_eq!(
            reason_text("**bold** `x`\n```rs\nfn f() {}\n```").as_deref(),
            Some("**bold** `x` ```rs fn f() {} ```")
        );
    }

    #[test]
    fn reason_text_cut_on_a_space_leaves_no_trailing_space() {
        // Words and spaces straddle 2,000: the 2,000th character is a space.
        let raw = format!("{} {}", "a".repeat(1999), "b".repeat(50));
        let cut = reason_text(&raw).expect("text");
        assert_eq!(cut.chars().count(), 1999);
        assert!(!cut.ends_with(' '));
        // The cut after a space that is followed by more words.
        let raw = format!("{} b c", "a".repeat(1998));
        let cut = reason_text(&raw).expect("text");
        assert_eq!(cut.chars().count(), 2000);
        assert!(cut.ends_with(" b"));
    }

    fn mark(name: &str) -> AgentMark {
        AgentMark {
            name: name.into(),
            letter: 'C',
            rgb: None,
        }
    }

    fn note(reason: &str) -> Note {
        Note {
            id: 0,
            agent: mark("claude"),
            clock: "10:42".into(),
            reason: reason.into(),
            reason_key: "a1:1".into(),
            anchor: Arc::new(vec!["x".into()]),
            hint: 0,
        }
    }

    #[test]
    fn store_caps_per_file_and_files_oldest_first() {
        let mut store = NoteStore::default();
        let one = PathBuf::from("src/one.rs");
        for n in 0..=PER_FILE {
            store.add(one.clone(), note(&format!("r{n}")));
        }
        let notes = store.notes(&one);
        assert_eq!(notes.len(), PER_FILE);
        assert_eq!(&*notes[0].reason, "r1");
        assert_eq!(&*notes[PER_FILE - 1].reason, "r200");
        assert!(notes.windows(2).all(|w| w[0].id < w[1].id));

        let mut store = NoteStore::default();
        for n in 0..=FILES {
            store.add(PathBuf::from(format!("f{n}.rs")), note("r"));
            if n == 5 {
                // Adding to file 0 again makes file 1 the least recently added to.
                store.add(PathBuf::from("f0.rs"), note("again"));
            }
        }
        assert_eq!(store.notes(Path::new("f1.rs")).len(), 0);
        assert_eq!(store.notes(Path::new("f0.rs")).len(), 2);
        assert_eq!(store.notes(Path::new("f2.rs")).len(), 1);
        assert_eq!(store.notes(Path::new("f2000.rs")).len(), 1);
    }

    #[test]
    fn generation_counts_adds_per_path() {
        let mut store = NoteStore::default();
        let (a, b) = (PathBuf::from("a.rs"), PathBuf::from("b.rs"));
        assert_eq!(store.generation(&a), 0);
        store.add(a.clone(), note("1"));
        store.add(a.clone(), note("2"));
        store.add(b.clone(), note("3"));
        assert_eq!((store.generation(&a), store.generation(&b)), (2, 1));
    }

    fn ins(s: &str) -> EditText {
        EditText::Inserted(s.into())
    }

    #[test]
    fn anchor_of_an_edit_is_its_whole_lines() {
        let mut text = String::new();
        for n in 1..=50 {
            match n {
                44 => text.push_str("        self.show(tab);\r\n"),
                45 => text.push_str("    }\r\n"),
                _ => text.push_str(&format!("line {n}\n")),
            }
        }
        let (lines, hint) = anchor_in(&text, &ins("self.show(tab);\r\n    }")).expect("found");
        assert_eq!(lines, vec!["        self.show(tab);", "    }"]);
        assert_eq!(hint, 43);
        // A new text ending in a newline spans only the lines it fills.
        let (lines, hint) = anchor_in("a\nb\nc\n", &ins("b\n")).expect("found");
        assert_eq!((lines, hint), (vec!["b".to_string()], 1));
        assert_eq!(anchor_in("a\nb\n", &ins("zzz")), None);
    }

    #[test]
    fn anchor_ends_on_a_multi_byte_character() {
        let (lines, hint) = anchor_in("a\nb\u{2192}\nc", &ins("b\u{2192}")).expect("found");
        assert_eq!((lines, hint), (vec!["b\u{2192}".to_string()], 1));
        let (lines, _) = anchor_in("\u{e9}\n", &ins("\u{e9}")).expect("found");
        assert_eq!(lines, vec!["\u{e9}".to_string()]);
    }

    #[test]
    fn anchor_of_a_write_is_line_one() {
        assert_eq!(
            anchor_in("first\nsecond\n", &EditText::WholeFile),
            Some((vec!["first".to_string()], 0))
        );
        assert_eq!(anchor_in("", &EditText::WholeFile), None);
    }

    #[test]
    fn replace_all_and_repeats_anchor_the_first_match() {
        let text = "x = 1;\nfoo();\ny = 2;\nfoo();\n";
        assert_eq!(
            anchor_in(text, &ins("foo();")),
            Some((vec!["foo();".to_string()], 1))
        );
    }

    #[test]
    fn removed_has_no_anchor() {
        assert_eq!(anchor_in("a\nb\n", &EditText::Removed), None);
    }

    fn light(agent: &str, reason: &str, at: Instant) -> Light {
        Light {
            kind: LightKind::NoText,
            agent: mark(agent),
            clock: "10:42".into(),
            reason: reason.into(),
            reason_key: "a1:1".into(),
            at,
        }
    }

    #[test]
    fn light_reasons_latest_wins_and_expire_after_keep() {
        let path = PathBuf::from("src/one.rs");
        let t0 = Instant::now();
        let later = t0 + std::time::Duration::from_secs(5);
        let mut store = NoteStore::default();
        store.expect_light(path.clone(), light("claude", "old", t0));
        store.expect_light(path.clone(), light("claude", "new", later));
        store.expect_light(path.clone(), light("agy", "other", later));
        let got = store
            .take_light(&path, "claude", false, later)
            .expect("pending");
        assert_eq!(&*got.reason, "new");
        assert!(store.take_light(&path, "claude", false, later).is_none());
        // Another agent's is untouched.
        assert!(store.take_light(&path, "agy", false, later).is_some());

        // One older than KEEP is not taken.
        store.expect_light(path.clone(), light("claude", "stale", t0));
        let past = t0 + KEEP + std::time::Duration::from_secs(1);
        assert!(store.take_light(&path, "claude", false, past).is_none());
        assert!(store.take_light(&path, "claude", false, t0).is_none());
    }

    #[test]
    fn a_removed_light_is_taken_only_by_a_removal_only_change() {
        let path = PathBuf::from("src/one.rs");
        let t0 = Instant::now();
        let removed = |reason: &str| Light {
            kind: LightKind::Removed,
            ..light("claude", reason, t0)
        };
        let mut store = NoteStore::default();
        store.expect_light(path.clone(), removed("drop the dead lines"));
        // A change with new lines is not what it is about: it stays, to be taken or to expire.
        assert!(store.take_light(&path, "claude", false, t0).is_none());
        let got = store.take_light(&path, "claude", true, t0).expect("kept");
        assert_eq!(&*got.reason, "drop the dead lines");
        assert_eq!(got.kind, LightKind::Removed);
        assert!(store.take_light(&path, "claude", true, t0).is_none());

        // A no-text reason is about any change; a removed one beside it is left by a change
        // with new lines, and the newest of both goes to one that only removed lines.
        let later = t0 + std::time::Duration::from_secs(5);
        store.expect_light(path.clone(), light("agy", "tidy the loop", t0));
        store.expect_light(
            path.clone(),
            Light {
                agent: mark("agy"),
                at: later,
                ..removed("cut the old loop")
            },
        );
        let got = store.take_light(&path, "agy", false, later).expect("any");
        assert_eq!(&*got.reason, "tidy the loop");
        let got = store.take_light(&path, "agy", true, later).expect("left");
        assert_eq!(&*got.reason, "cut the old loop");
        store.expect_light(path.clone(), light("agy", "older", t0));
        store.expect_light(
            path.clone(),
            Light {
                agent: mark("agy"),
                at: later,
                ..removed("newer")
            },
        );
        let got = store.take_light(&path, "agy", true, later).expect("both");
        assert_eq!(&*got.reason, "newer");
        assert!(store.take_light(&path, "agy", true, later).is_none());
    }

    #[test]
    fn the_light_an_edit_leaves_is_decided_by_its_texts() {
        // An agent that gives no text: its reason waits for its next change, with no read.
        assert_eq!(light_kind(&[], false), Some(LightKind::NoText));
        assert!(!needs_read(&[]));
        // A pure removal: a reason about a removal-only change, with no read.
        let removed = [EditText::Removed];
        assert_eq!(light_kind(&removed, false), Some(LightKind::Removed));
        assert!(!needs_read(&removed));
        assert!(!needs_read(&[EditText::Removed, EditText::Removed]));
        // The provider says "no note": nothing waits, nothing is read.
        let none = [EditText::NoNote];
        assert_eq!(light_kind(&none, false), None);
        assert!(!needs_read(&none));
        assert_eq!(
            light_kind(&[EditText::NoNote, EditText::Removed], false),
            Some(LightKind::Removed)
        );
        // A text to find is read for; found, it is the note; not found beside a removal, the
        // removal's light; not found alone, nothing.
        let both = [ins("b"), EditText::Removed];
        assert!(needs_read(&both));
        assert_eq!(light_kind(&both, true), None);
        assert_eq!(light_kind(&both, false), Some(LightKind::Removed));
        assert_eq!(light_kind(&[ins("b")], false), None);
        assert!(needs_read(&[EditText::WholeFile]));
        assert_eq!(light_kind(&[EditText::WholeFile], false), None);
    }

    fn lines_with(at: &[usize], block: &[&str], len: usize) -> Vec<String> {
        let mut lines: Vec<String> = (0..len).map(|n| format!("line {n}")).collect();
        for &start in at {
            for (k, l) in block.iter().enumerate() {
                lines[start + k] = l.to_string();
            }
        }
        lines
    }

    fn slices(lines: &[String]) -> Vec<&str> {
        lines.iter().map(String::as_str).collect()
    }

    fn anchored(reason: &str, key: &str, anchor: &[&str], hint: usize) -> Note {
        Note {
            reason_key: key.into(),
            anchor: Arc::new(anchor.iter().map(|l| l.to_string()).collect()),
            hint,
            ..note(reason)
        }
    }

    fn far() -> Instant {
        Instant::now() + std::time::Duration::from_secs(60)
    }

    /// Placed as (under, reason).
    fn unders(placed: &[Placed]) -> Vec<(usize, &str)> {
        placed.iter().map(|p| (p.under, &*p.note.reason)).collect()
    }

    #[test]
    fn the_anchor_nearest_the_hint_wins() {
        let block = ["    if found {", "        return;"];
        let lines = lines_with(&[10, 50], &block, 80);
        let anchor: Vec<String> = block.iter().map(|l| l.to_string()).collect();
        let lines = slices(&lines);
        assert_eq!(find_anchor(&lines, &anchor, 47), Some(50..52));
        assert_eq!(find_anchor(&lines, &anchor, 12), Some(10..12));
        // 30 is as far from 10 as from 50: the upper one.
        assert_eq!(find_anchor(&lines, &anchor, 30), Some(10..12));
        assert_eq!(find_anchor(&lines, &anchor, 31), Some(50..52));
        // Past the end, at the very end, and not there.
        assert_eq!(find_anchor(&lines, &anchor, 500), Some(50..52));
        let tail = lines_with(&[78], &block, 80);
        assert_eq!(find_anchor(&slices(&tail), &anchor, 0), Some(78..80));
        assert_eq!(find_anchor(&lines, &["nope".to_string()], 0), None);
        assert_eq!(find_anchor(&lines, &[], 0), None);
        assert_eq!(find_anchor(&lines[..1], &anchor, 0), None);
    }

    #[test]
    fn a_gone_anchor_is_skipped_and_comes_back() {
        let notes = vec![anchored("why", "a1:1", &["fn a() {", "}"], 4)];
        let with = lines_with(&[4], &["fn a() {", "}"], 10);
        let without = lines_with(&[], &[], 10);
        let placed = place(&notes, &slices(&with), far()).expect("in time");
        assert_eq!(unders(&placed), vec![(5, "why")]);
        assert_eq!(place(&notes, &slices(&without), far()), Some(Vec::new()));
        // The text back (an undo): the same note again.
        let placed = place(&notes, &slices(&with), far()).expect("in time");
        assert_eq!(placed[0].note, notes[0]);
        assert_eq!(placed[0].under, 5);
    }

    #[test]
    fn a_newer_overlapping_note_hides_the_older() {
        let lines = lines_with(&[3], &["a", "b", "c", "d"], 20);
        let lines = slices(&lines);
        let notes = vec![
            // Oldest first: lines 3-4, then 10 (apart), then 4-5 (the newest, over the first).
            anchored("old", "a1:1", &["a", "b"], 3),
            anchored("apart", "a1:2", &["line 10"], 10),
            anchored("new", "a1:3", &["b", "c"], 4),
        ];
        let placed = place(&notes, &lines, far()).expect("in time");
        assert_eq!(unders(&placed), vec![(5, "new"), (10, "apart")]);
        // Touching is not overlapping.
        let notes = vec![
            anchored("old", "a1:1", &["a", "b"], 3),
            anchored("new", "a1:3", &["c", "d"], 5),
        ];
        let placed = place(&notes, &lines, far()).expect("in time");
        assert_eq!(unders(&placed), vec![(4, "old"), (6, "new")]);
    }

    #[test]
    fn one_note_per_reason_under_the_topmost_block() {
        let mut lines = lines_with(&[5], &["x();"], 40);
        lines[20] = "y();".into();
        lines[21] = "z();".into();
        let lines = slices(&lines);
        let notes = vec![
            // One reason, two blocks (the lower one filed first), and another reason.
            anchored("why", "a1:7", &["y();", "z();"], 20),
            anchored("why", "a1:7", &["x();"], 5),
            anchored("other", "a2:1", &["line 30"], 30),
        ];
        let placed = place(&notes, &lines, far()).expect("in time");
        assert_eq!(unders(&placed), vec![(5, "why"), (30, "other")]);
        // The top block gone: the reason shows under the one left.
        let gone: Vec<&str> = lines
            .iter()
            .map(|l| if *l == "x();" { "w();" } else { l })
            .collect();
        let placed = place(&notes, &gone, far()).expect("in time");
        assert_eq!(unders(&placed), vec![(21, "why"), (30, "other")]);
    }

    #[test]
    fn place_gives_up_past_the_deadline() {
        let lines = lines_with(&[], &[], 10);
        let notes = vec![anchored("why", "a1:1", &["line 3"], 3)];
        let past = Instant::now()
            .checked_sub(std::time::Duration::from_millis(1))
            .expect("an instant before now");
        assert_eq!(place(&notes, &slices(&lines), past), None);
        assert!(place(&notes, &slices(&lines), far()).is_some());
    }
}
