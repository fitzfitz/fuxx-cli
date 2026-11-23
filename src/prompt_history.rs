//! Prompt history (a chore after M11ar): ↑/↓ in an agent tab's box bring back what you sent.
//!
//! Per folder: the tab's own prompts first (newest first; a resumed conversation's too), then
//! the prompts sent from that folder before, without repeats. The folders' lists live in
//! `prompt-history.jsonl` (next to the sessions file; `-dev` for a test build), one line per
//! send, appended off the UI thread; it is read and compacted once at app start, off the UI
//! thread, into `PromptHistory` (a GPUI global). Until then only the tab's own prompts are
//! offered. Pure but for `PromptHistory` and its file.

use std::path::{Path, PathBuf};

use gpui::{App, Global};
use serde::{Deserialize, Serialize};

/// The most prompts kept for one folder, the newest.
pub const PER_FOLDER: usize = 100;
/// The most folders kept, the most recently used.
pub const FOLDERS: usize = 40;
/// A prompt longer than this (in characters) isn't kept.
pub const MAX_CHARS: usize = 8_000;

/// One line of the file: a prompt sent from `folder` at `at` (Unix seconds).
#[derive(Serialize, Deserialize)]
struct Line {
    folder: String,
    text: String,
    at: u64,
}

/// The folders' prompts: the most recently used folder first, each folder's prompts newest
/// first (with when each was sent).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Store {
    folders: Vec<(String, Vec<(String, u64)>)>,
}

impl Store {
    /// `text` sent from `folder` at `at`: first in its folder (a repeat moves there), the
    /// folder first of all; past the caps the oldest prompts and least recently used folders
    /// go. An empty or oversized prompt isn't kept (false).
    pub fn record(&mut self, folder: &str, text: &str, at: u64) -> bool {
        if text.is_empty() || text.chars().count() > MAX_CHARS {
            return false;
        }
        let mut prompts = match self.folders.iter().position(|(f, _)| f == folder) {
            Some(i) => self.folders.remove(i).1,
            None => Vec::new(),
        };
        prompts.retain(|(t, _)| t != text);
        prompts.insert(0, (text.to_string(), at));
        prompts.truncate(PER_FOLDER);
        self.folders.insert(0, (folder.to_string(), prompts));
        self.folders.truncate(FOLDERS);
        true
    }

    /// `folder`'s prompts, newest first.
    pub fn prompts(&self, folder: &str) -> Vec<&str> {
        self.folders
            .iter()
            .find(|(f, _)| f == folder)
            .map(|(_, p)| p.iter().map(|(t, _)| t.as_str()).collect())
            .unwrap_or_default()
    }

    /// How many prompts are kept in all.
    pub fn kept(&self) -> usize {
        self.folders.iter().map(|(_, p)| p.len()).sum()
    }

    /// The file's text replayed, line by line (a line that isn't one is skipped), and how many
    /// lines it had.
    pub fn parse(text: &str) -> (Store, usize) {
        let mut store = Store::default();
        let mut lines = 0;
        for raw in text.lines().filter(|l| !l.trim().is_empty()) {
            lines += 1;
            if let Ok(line) = serde_json::from_str::<Line>(raw) {
                store.record(&line.folder, &line.text, line.at);
            }
        }
        (store, lines)
    }

    /// What is kept as the file's lines, in an order that replays to the same store: the least
    /// recently used folder first, each folder's prompts oldest first.
    pub fn to_jsonl(&self) -> String {
        let mut out = String::new();
        for (folder, prompts) in self.folders.iter().rev() {
            for (text, at) in prompts.iter().rev() {
                out.push_str(&line(folder, text, *at));
                out.push('\n');
            }
        }
        out
    }
}

/// One line of the file (without its newline).
pub fn line(folder: &str, text: &str, at: u64) -> String {
    serde_json::to_string(&Line {
        folder: folder.to_string(),
        text: text.to_string(),
        at,
    })
    .unwrap_or_default()
}

/// Whether a file of `lines` lines holding `kept` prompts is to be written again with only
/// those: once it holds twice what is kept.
pub fn needs_compaction(lines: usize, kept: usize) -> bool {
    lines > 0 && lines >= 2 * kept
}

/// A prompt as history keeps it: its pasted images' markers (`[Image #2]`) out, runs of spaces
/// made one, each line's ends trimmed, blank lines at either end dropped. None: nothing is
/// left (a prompt that was only an image).
pub fn tidy(text: &str) -> Option<String> {
    let mut rest = text;
    let mut stripped = String::with_capacity(text.len());
    while let Some(i) = rest.find("[Image #") {
        let after = &rest[i + "[Image #".len()..];
        let digits = after.chars().take_while(char::is_ascii_digit).count();
        if digits > 0 && after[digits..].starts_with(']') {
            stripped.push_str(&rest[..i]);
            rest = &after[digits + 1..];
        } else {
            stripped.push_str(&rest[..i + 1]);
            rest = &rest[i + 1..];
        }
    }
    stripped.push_str(rest);
    let tidied = stripped
        .lines()
        .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
        .collect::<Vec<_>>()
        .join("\n");
    let tidied = tidied.trim_matches('\n');
    (!tidied.is_empty()).then(|| tidied.to_string())
}

/// What ↑ walks: this tab's own prompts (`own`, oldest first as said), newest first, then the
/// folder's (`folder`, newest first), each tidied, without repeats.
pub fn entries(own: &[String], folder: &[&str]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let own = own.iter().rev().filter_map(|t| tidy(t));
    let theirs = folder.iter().map(|t| t.to_string());
    for text in own.chain(theirs) {
        if !out.contains(&text) {
            out.push(text);
        }
    }
    out
}

/// Browsing the history from the box: the entries as they were when browsing began
/// (`snapshot`, newest first), which one is shown (`at`), what the box held before (`draft`,
/// put back past the newest) and the text put in the box (`shown`: once the box holds anything
/// else, it was edited and browsing is over).
#[derive(Clone, Debug, PartialEq)]
pub struct Recall {
    snapshot: Vec<String>,
    at: usize,
    draft: String,
    shown: String,
}

/// What ↓ shows: a newer entry, or the draft again (browsing is over).
#[derive(Clone, Debug, PartialEq)]
pub enum Newer {
    Show(String),
    Draft(String),
}

impl Recall {
    /// ↑ from the box holding `draft`: the newest entry that differs from it. None: there is
    /// none to show.
    pub fn begin(snapshot: Vec<String>, draft: &str) -> Option<Recall> {
        let at = snapshot.iter().position(|t| t != draft)?;
        Some(Recall {
            shown: snapshot[at].clone(),
            snapshot,
            at,
            draft: draft.to_string(),
        })
    }

    /// The entry in the box.
    pub fn shown(&self) -> &str {
        &self.shown
    }

    /// Whether the box (holding `text`) still shows the recalled entry, unedited.
    pub fn browsing(&self, text: &str) -> bool {
        text == self.shown
    }

    /// ↑ again: the next older entry; None at the oldest (it stays).
    pub fn older(&mut self) -> Option<&str> {
        let at = (self.at + 1..self.snapshot.len()).find(|&i| self.snapshot[i] != self.draft)?;
        self.at = at;
        self.shown = self.snapshot[at].clone();
        Some(&self.shown)
    }

    /// ↓: the next newer entry, or past the newest the draft.
    pub fn newer(&mut self) -> Newer {
        match (0..self.at).rev().find(|&i| self.snapshot[i] != self.draft) {
            Some(at) => {
                self.at = at;
                self.shown = self.snapshot[at].clone();
                Newer::Show(self.shown.clone())
            }
            None => Newer::Draft(self.draft.clone()),
        }
    }
}

/// The file, under `$HOME`: the sessions file's folder, `-dev` for a test build (as
/// `restore::state_path_for`).
pub fn history_path_for(home: &Path, dev: bool) -> PathBuf {
    let name = if dev {
        "prompt-history-dev.jsonl"
    } else {
        "prompt-history.jsonl"
    };
    home.join("Library/Application Support/fuxx").join(name)
}

/// This build's file; none in unit tests (they never touch your history) or without `$HOME`.
fn history_path() -> Option<PathBuf> {
    if cfg!(test) {
        return None;
    }
    crate::workspace::home_dir().map(|h| history_path_for(&h, cfg!(debug_assertions)))
}

/// Appends and the start's compaction take turns on the file (both on background threads).
static FILE: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// The run's copy of the folders' prompts: none until the file is read (`load`).
#[derive(Default)]
pub struct PromptHistory {
    store: Option<Store>,
    /// Scenes: the store is theirs, and nothing is read or written.
    no_files: bool,
}

impl Global for PromptHistory {}

impl PromptHistory {
    /// At app start: the file read and, when it holds twice what is kept, written again with
    /// only that, on the background executor; then the store is the run's. Never blocks.
    pub fn load(cx: &mut App) {
        let Some(path) = history_path() else {
            return;
        };
        let read = cx
            .background_executor()
            .spawn(async move { read_and_compact(&path) });
        cx.spawn(async move |cx| {
            let store = read.await;
            cx.update(|cx| {
                let history = cx.default_global::<PromptHistory>();
                if !history.no_files && history.store.is_none() {
                    history.store = Some(store);
                }
            });
        })
        .detach();
    }

    /// `folder`'s stored prompts, newest first; none before the file is read.
    pub fn prompts(folder: &str, cx: &App) -> Vec<String> {
        cx.try_global::<PromptHistory>()
            .and_then(|h| h.store.as_ref())
            .map(|s| s.prompts(folder).into_iter().map(String::from).collect())
            .unwrap_or_default()
    }

    /// `text`, sent from `folder`, kept (tidied; not when nothing is left or it is too long)
    /// and appended to the file off the UI thread (`write`: not for a scene's tab).
    pub fn record(folder: &str, text: &str, write: bool, cx: &mut App) {
        let Some(text) = tidy(text) else {
            return;
        };
        let at = now_secs();
        let history = cx.default_global::<PromptHistory>();
        let kept = match history.store.as_mut() {
            Some(store) => store.record(folder, &text, at),
            None => text.chars().count() <= MAX_CHARS,
        };
        if !kept || !write || history.no_files {
            return;
        }
        let Some(path) = history_path() else {
            return;
        };
        let line = line(folder, &text, at);
        cx.background_executor()
            .spawn(async move { append(&path, &line) })
            .detach();
    }

    /// The prompts a scene's tabs recall from (never read from or written to a file).
    #[cfg(feature = "ui-shots")]
    pub fn set_for_shots(folders: &[(&str, &[&str])], cx: &mut App) {
        let mut store = Store::default();
        // Each folder's oldest first, so the first named is the newest.
        for (folder, prompts) in folders.iter().rev() {
            for text in prompts.iter().rev() {
                store.record(folder, text, 0);
            }
        }
        let history = cx.default_global::<PromptHistory>();
        history.store = Some(store);
        history.no_files = true;
    }
}

/// The file's store, compacted in place when it holds twice what is kept. Blocking.
fn read_and_compact(path: &Path) -> Store {
    let _turn = FILE.lock();
    let Ok(text) = std::fs::read_to_string(path) else {
        return Store::default();
    };
    let (store, lines) = Store::parse(&text);
    if needs_compaction(lines, store.kept())
        && let Err(e) = crate::settings::write_atomic(path, &store.to_jsonl())
    {
        eprintln!(
            "[fuxx] prompt history: couldn't compact {}: {e}",
            path.display()
        );
    }
    store
}

/// `line` added to the file's end (the folder made if needed). Blocking.
fn append(path: &Path, line: &str) {
    use std::io::Write;
    let _turn = FILE.lock();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let written = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .and_then(|mut f| writeln!(f, "{line}"));
    if let Err(e) = written {
        eprintln!(
            "[fuxx] prompt history: couldn't save {}: {e}",
            path.display()
        );
    }
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn owned(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn recall_older_newer_and_draft_restored() {
        let snapshot = owned(&["third", "second", "first"]);
        let mut r = Recall::begin(snapshot, "half typed").expect("something to recall");
        assert_eq!(r.shown(), "third", "↑ shows the newest");
        assert_eq!(r.older(), Some("second"));
        assert_eq!(r.older(), Some("first"));
        assert_eq!(r.newer(), Newer::Show("second".into()));
        assert_eq!(r.newer(), Newer::Show("third".into()));
        assert_eq!(
            r.newer(),
            Newer::Draft("half typed".into()),
            "past the newest, the draft"
        );
        assert_eq!(Recall::begin(Vec::new(), ""), None, "nothing to recall");
    }

    /// ↑ always changes the box: an entry equal to what it holds is passed over.
    #[test]
    fn recall_skips_an_entry_equal_to_the_box() {
        let snapshot = owned(&["fix the build", "add a test", "fix the build again"]);
        let mut r = Recall::begin(snapshot, "fix the build").unwrap();
        assert_eq!(r.shown(), "add a test");
        assert_eq!(r.older(), Some("fix the build again"));
        assert_eq!(r.newer(), Newer::Show("add a test".into()));
        assert_eq!(r.newer(), Newer::Draft("fix the build".into()));
        assert_eq!(Recall::begin(owned(&["same"]), "same"), None);
    }

    #[test]
    fn editing_a_recalled_prompt_ends_browsing() {
        let r = Recall::begin(owned(&["run the tests"]), "").unwrap();
        assert!(r.browsing("run the tests"));
        assert!(!r.browsing("run the tests again"), "edited");
        assert!(!r.browsing(""), "emptied");
    }

    #[test]
    fn end_of_history_stays() {
        let mut r = Recall::begin(owned(&["newer", "oldest"]), "").unwrap();
        assert_eq!(r.older(), Some("oldest"));
        assert_eq!(r.older(), None, "nothing older");
        assert_eq!(r.shown(), "oldest", "still shown");
        assert_eq!(r.newer(), Newer::Show("newer".into()));
    }

    #[test]
    fn record_moves_a_repeat_to_front_and_caps() {
        let mut s = Store::default();
        assert!(s.record("code/my-app", "one", 1));
        assert!(s.record("code/my-app", "two", 2));
        assert!(s.record("code/my-app", "one", 3));
        assert_eq!(
            s.prompts("code/my-app"),
            ["one", "two"],
            "a repeat moves first"
        );
        for i in 0..PER_FOLDER + 5 {
            s.record("code/my-app", &format!("p{i}"), 10 + i as u64);
        }
        let kept = s.prompts("code/my-app");
        assert_eq!(kept.len(), PER_FOLDER);
        assert_eq!(kept[0], format!("p{}", PER_FOLDER + 4), "the newest first");
        // Folders: the least recently used go past the cap.
        for i in 0..FOLDERS {
            s.record(&format!("code/app-{i}"), "hi", 500);
        }
        assert!(
            s.prompts("code/my-app").is_empty(),
            "the oldest folder went"
        );
        assert_eq!(s.prompts("code/app-0"), ["hi"]);
        s.record("code/app-0", "again", 600);
        s.record("fuxx-cli/src", "new", 700);
        assert_eq!(s.prompts("code/app-0"), ["again", "hi"], "used again: kept");
        assert!(s.prompts("code/app-1").is_empty(), "now the oldest went");
    }

    #[test]
    fn image_markers_stripped_and_image_only_skipped() {
        assert_eq!(
            tidy("look at [Image #1]  and   [Image #12] here ").as_deref(),
            Some("look at and here")
        );
        assert_eq!(tidy("[Image #3]"), None);
        assert_eq!(tidy("  [Image #1] [Image #2]\n "), None);
        assert_eq!(tidy("   "), None);
        // Lines stay; their spaces are tidied.
        assert_eq!(
            tidy("first  line\n\n  second\n").as_deref(),
            Some("first line\n\nsecond")
        );
        // Brackets that aren't a marker stay.
        assert_eq!(
            tidy("see [Image #x] and [Image #]").as_deref(),
            Some("see [Image #x] and [Image #]")
        );
    }

    #[test]
    fn oversized_not_kept() {
        let mut s = Store::default();
        let big = "x".repeat(MAX_CHARS + 1);
        assert!(!s.record("code/my-app", &big, 1));
        assert!(s.prompts("code/my-app").is_empty());
        let fits = "é".repeat(MAX_CHARS);
        assert!(s.record("code/my-app", &fits, 2), "counted in characters");
    }

    #[test]
    fn own_tab_first_then_folder_deduped() {
        let own = owned(&["fix the build", "look at [Image #1] this", "run it"]);
        let folder = ["older idea", "run it", "look at this"];
        assert_eq!(
            entries(&own, &folder),
            ["run it", "look at this", "fix the build", "older idea"]
        );
        assert_eq!(
            entries(&[], &folder),
            ["older idea", "run it", "look at this"]
        );
        assert_eq!(entries(&owned(&["[Image #1]"]), &[]), Vec::<String>::new());
    }

    #[test]
    fn jsonl_round_trip_and_garbage_ignored() {
        let mut s = Store::default();
        s.record("code/my-app", "one", 1);
        s.record("fuxx-cli/src", "two\nlines", 2);
        s.record("code/my-app", "three \"quoted\"", 3);
        let text = s.to_jsonl();
        assert_eq!(Store::parse(&text), (s.clone(), 3));
        let noisy = format!(
            "not json\n{{\"folder\":\"code/my-app\"}}\n\n{text}{}",
            "{\"folder\":\"code/my-app\",\"text\":\"four\",\"at\":4,\"extra\":true}\n"
        );
        let (read, lines) = Store::parse(&noisy);
        assert_eq!(lines, 6, "blank lines aren't counted");
        assert_eq!(
            read.prompts("code/my-app"),
            ["four", "three \"quoted\"", "one"]
        );
        assert_eq!(read.prompts("fuxx-cli/src"), ["two\nlines"]);
        assert_eq!(
            line("code/my-app", "hi", 7),
            r#"{"folder":"code/my-app","text":"hi","at":7}"#
        );
    }

    #[test]
    fn compaction_keeps_newest_per_folder_and_lru_folders() {
        // A file of every send: more than the caps keep.
        let mut text = String::new();
        for i in 0..PER_FOLDER + 20 {
            text.push_str(&line("code/my-app", &format!("p{i}"), i as u64));
            text.push('\n');
        }
        for f in 0..FOLDERS {
            text.push_str(&line(&format!("code/app-{f}"), "hi", 1000));
            text.push('\n');
        }
        let (store, lines) = Store::parse(&text);
        assert_eq!(lines, PER_FOLDER + 20 + FOLDERS);
        assert_eq!(store.kept(), FOLDERS, "my-app was the least recently used");
        assert!(!needs_compaction(lines, store.kept() + lines));
        assert!(needs_compaction(lines, store.kept()));
        assert!(!needs_compaction(0, 0), "no file, nothing to do");
        // Written again, the file holds only what is kept, and reads the same.
        let compacted = store.to_jsonl();
        let (again, again_lines) = Store::parse(&compacted);
        assert_eq!(again, store);
        assert_eq!(again_lines, FOLDERS);
        assert!(!needs_compaction(again_lines, again.kept()));
        // Within a folder, the newest prompts are the ones kept.
        let mut one = String::new();
        for i in 0..PER_FOLDER * 2 {
            one.push_str(&line("fuxx-cli/src", &format!("p{i}"), i as u64));
            one.push('\n');
        }
        let (store, lines) = Store::parse(&one);
        assert!(needs_compaction(lines, store.kept()));
        let kept = store.prompts("fuxx-cli/src");
        assert_eq!(
            kept.first().copied(),
            Some(format!("p{}", PER_FOLDER * 2 - 1).as_str())
        );
        assert_eq!(
            kept.last().copied(),
            Some(format!("p{PER_FOLDER}").as_str())
        );
    }

    #[test]
    fn the_file_follows_the_dev_and_release_split() {
        let home = Path::new("/Users/me");
        assert_eq!(
            history_path_for(home, false),
            Path::new("/Users/me/Library/Application Support/fuxx/prompt-history.jsonl")
        );
        assert_eq!(
            history_path_for(home, true),
            Path::new("/Users/me/Library/Application Support/fuxx/prompt-history-dev.jsonl")
        );
    }
}
