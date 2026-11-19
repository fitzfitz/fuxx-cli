//! The agent's own reason for an edit (M11ag Part 2): the turn's last text before the edit, kept
//! as plain characters to show under the changed lines.

use std::collections::HashMap;
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
#[allow(dead_code)] // read by the file tab (M11ag Tasks 9-10)
pub struct Note {
    pub id: u64,
    pub agent: AgentMark,
    pub clock: Arc<str>,
    pub reason: Arc<str>,
    pub reason_key: Arc<str>,
    pub anchor: Arc<Vec<String>>,
    pub hint: usize,
}

/// A reason waiting for a file tab to say which lines it is about (the light way: a pure
/// deletion, or an agent that gives no text).
#[derive(Clone, Debug)]
#[allow(dead_code)] // read by the file tab (M11ag Tasks 9-10)
pub struct Light {
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

    #[allow(dead_code)] // used by the file tab (M11ag Tasks 9-10)
    /// The notes of `path`, oldest first.
    pub fn notes(&self, path: &Path) -> Arc<Vec<Note>> {
        self.files
            .get(path)
            .map_or_else(|| Arc::new(Vec::new()), |f| f.notes.clone())
    }

    #[allow(dead_code)] // used by the file tab (M11ag Tasks 9-10)
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

    #[allow(dead_code)] // used by the file tab (M11ag Tasks 9-10)
    /// The newest pending reason of `agent` for `path` not older than `KEEP` at `now`; every
    /// pending one of that path and agent is gone with it.
    pub fn take_light(&mut self, path: &Path, agent: &str, now: Instant) -> Option<Light> {
        let pending = self.lights.get_mut(path)?;
        let mut taken: Option<Light> = None;
        pending.retain(|l| {
            if &*l.agent.name != agent {
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

/// Where `edit`'s new text is in `text` (the file as read at the finish): `Inserted` gives the
/// whole lines its first occurrence spans (without `\r`) and the first one's 0-based line;
/// `WholeFile` the first line and 0 (None for an empty file); `Removed`, and a text that is not
/// found, None.
pub fn anchor_in(text: &str, edit: &EditText) -> Option<(Vec<String>, usize)> {
    let (first, last) = match edit {
        EditText::Removed => return None,
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
        let got = store.take_light(&path, "claude", later).expect("pending");
        assert_eq!(&*got.reason, "new");
        assert!(store.take_light(&path, "claude", later).is_none());
        // Another agent's is untouched.
        assert!(store.take_light(&path, "agy", later).is_some());

        // One older than KEEP is not taken.
        store.expect_light(path.clone(), light("claude", "stale", t0));
        let past = t0 + KEEP + std::time::Duration::from_secs(1);
        assert!(store.take_light(&path, "claude", past).is_none());
        assert!(store.take_light(&path, "claude", t0).is_none());
    }
}
