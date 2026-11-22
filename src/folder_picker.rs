//! The agent box's folder picker (M11ak §2), its pure part: the Recent list fuxx keeps, the
//! folders fuxx knows (`KnownFolders`, a global the Workspace keeps up to date), and the rows the
//! picker shows for a search. The tab draws them (`agent_box.rs`).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use gpui::Global;

use crate::palette_match::fuzzy;
use crate::sidebar::home_relative;

/// The most folders Recent keeps.
pub const RECENT_MAX: usize = 8;

/// The folders agent tabs and workspaces were opened in, newest first, each once, up to
/// `RECENT_MAX` (saved with the sessions file).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RecentFolders {
    folders: Vec<PathBuf>,
}

impl RecentFolders {
    /// `folder` is the newest now (an older place of it goes).
    pub fn push(&mut self, folder: PathBuf) {
        self.folders.retain(|f| *f != folder);
        self.folders.insert(0, folder);
        self.folders.truncate(RECENT_MAX);
    }

    pub fn folders(&self) -> &[PathBuf] {
        &self.folders
    }

    /// The list as the sessions file kept it (newest first); twins and any past
    /// `RECENT_MAX` go.
    pub fn from_saved(v: Vec<PathBuf>) -> RecentFolders {
        let mut recent = RecentFolders::default();
        // Oldest first, so each newer place of a folder wins.
        for folder in v.into_iter().rev() {
            recent.push(folder);
        }
        recent
    }
}

/// What the picker offers besides an agent's own list: Recent, and the folders of the
/// workspaces and saved sessions. The Workspace sets it whenever either changes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct KnownFolders {
    pub recent: Vec<PathBuf>,
    pub projects: Vec<PathBuf>,
}

impl Global for KnownFolders {}

/// Which folders the open picker may list are there to start in, as checked off the UI thread
/// (R10: listing reads no disk on a keystroke). A folder not checked yet is not listed until
/// it is; one check runs at a time.
#[derive(Debug, Default)]
pub struct FolderChecks {
    there: HashMap<PathBuf, bool>,
    checking: bool,
}

impl FolderChecks {
    /// Whether `folder` was found there.
    pub fn exists(&self, folder: &Path) -> bool {
        self.there.get(folder) == Some(&true)
    }

    /// The folders of `folders` not checked yet (each once), to check now; None when there are
    /// none or a check runs already (its end asks again).
    pub fn begin(&mut self, folders: &[PathBuf]) -> Option<Vec<PathBuf>> {
        if self.checking {
            return None;
        }
        let mut new: Vec<PathBuf> = Vec::new();
        for folder in folders {
            if !self.there.contains_key(folder) && !new.contains(folder) {
                new.push(folder.clone());
            }
        }
        self.checking = !new.is_empty();
        self.checking.then_some(new)
    }

    /// Whether each of `folders` is a folder. Reads the disk: call it off the UI thread.
    pub fn check(folders: Vec<PathBuf>) -> Vec<(PathBuf, bool)> {
        folders
            .into_iter()
            .map(|f| {
                let there = f.is_dir();
                (f, there)
            })
            .collect()
    }

    /// A check's answers are in.
    pub fn done(&mut self, found: Vec<(PathBuf, bool)>) {
        self.there.extend(found);
        self.checking = false;
    }
}

/// One row of the picker.
#[derive(Clone, Debug, PartialEq)]
pub enum PickerRow {
    Heading(&'static str),
    /// A folder, as shown (`~/…`), ✓ when the agent is in it.
    Folder {
        path: PathBuf,
        shown: String,
        current: bool,
    },
    /// `This Computer ▸`: the folders fuxx and the agent know, beside the picker.
    ThisComputer,
    /// `New Project`: the folder panel.
    NewProject,
    /// `No Project`: your home folder.
    NoProject,
}

/// The rows' words.
pub const RECENT: &str = "Recent";
pub const THIS_COMPUTER: &str = "This Computer";
pub const NEW_PROJECT: &str = "New Project";
pub const NO_PROJECT: &str = "No Project";

/// `path` as a folder row.
pub fn folder_row(path: &Path, current: &Path, home: Option<&Path>) -> PickerRow {
    PickerRow::Folder {
        path: path.to_path_buf(),
        shown: home_relative(path, home),
        current: path == current,
    }
}

/// The picker's rows for `query`. With none: Recent (its folders that `exists`), then This
/// Computer ▸, New Project and No Project. With one, every section is filtered (fuzzy, on the
/// shown path or the row's words): Recent's folders, This Computer's (`computer`, those not in
/// Recent) under their own heading, New Project, No Project; a section left empty loses its
/// heading.
pub fn picker_rows(
    query: &str,
    recent: &[PathBuf],
    computer: &[PathBuf],
    current: &Path,
    home: Option<&Path>,
    exists: impl Fn(&Path) -> bool,
) -> Vec<PickerRow> {
    let query = query.trim();
    let found = |words: &str| query.is_empty() || fuzzy(query, words).is_some();
    let folders = |list: &[PathBuf], skip: &[PathBuf]| -> Vec<PickerRow> {
        list.iter()
            .filter(|f| !skip.contains(f) && exists(f))
            .map(|f| folder_row(f, current, home))
            .filter(|row| matches!(row, PickerRow::Folder { shown, .. } if found(shown)))
            .collect()
    };
    let mut rows = Vec::new();
    let mut section = |heading: &'static str, found: Vec<PickerRow>| {
        if !found.is_empty() {
            rows.push(PickerRow::Heading(heading));
            rows.extend(found);
        }
    };
    section(RECENT, folders(recent, &[]));
    if query.is_empty() {
        rows.push(PickerRow::ThisComputer);
    } else {
        section(THIS_COMPUTER, folders(computer, recent));
    }
    rows.extend(
        [
            (PickerRow::NewProject, NEW_PROJECT),
            (PickerRow::NoProject, NO_PROJECT),
        ]
        .into_iter()
        .filter(|(_, words)| found(words))
        .map(|(row, _)| row),
    );
    rows
}

/// This Computer's folders: the workspaces' and saved sessions' (`projects`) and the agent's own
/// (`agent_known`), each once, in the order of their paths (as shown: those under your home
/// sort together).
pub fn computer_folders(projects: &[PathBuf], agent_known: &[PathBuf]) -> Vec<PathBuf> {
    let mut all: Vec<PathBuf> = projects.iter().chain(agent_known).cloned().collect();
    all.sort_by(|a, b| a.as_os_str().cmp(b.as_os_str()));
    all.dedup();
    all
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    const HOME: &str = "/home/me";

    fn shown(rows: &[PickerRow]) -> Vec<String> {
        rows.iter()
            .map(|r| match r {
                PickerRow::Heading(h) => format!("# {h}"),
                PickerRow::Folder { shown, current, .. } => {
                    format!("{shown}{}", if *current { " \u{2713}" } else { "" })
                }
                PickerRow::ThisComputer => "This Computer \u{25b8}".into(),
                PickerRow::NewProject => NEW_PROJECT.into(),
                PickerRow::NoProject => NO_PROJECT.into(),
            })
            .collect()
    }

    #[test]
    fn recent_keeps_eight_newest_first_without_twins() {
        let mut r = RecentFolders::default();
        for i in 0..10 {
            r.push(p(&format!("/f{i}")));
        }
        assert_eq!(r.folders().len(), RECENT_MAX);
        assert_eq!(r.folders()[0], p("/f9"), "newest first");
        assert_eq!(r.folders()[7], p("/f2"), "the oldest went");
        r.push(p("/f5"));
        assert_eq!(r.folders()[0], p("/f5"), "pushed again: the newest");
        assert_eq!(
            r.folders().iter().filter(|f| **f == p("/f5")).count(),
            1,
            "once"
        );
        assert_eq!(r.folders().len(), RECENT_MAX);
        // As saved: newest first, twins and the overflow dropped (the newest place kept).
        let saved: Vec<PathBuf> = ["/a", "/b", "/a", "/c", "/d", "/e", "/f", "/g", "/h", "/i"]
            .map(p)
            .to_vec();
        let back = RecentFolders::from_saved(saved);
        assert_eq!(
            back.folders(),
            ["/a", "/b", "/c", "/d", "/e", "/f", "/g", "/h"].map(p)
        );
    }

    #[test]
    fn recent_skips_missing_folders() {
        let home = p(HOME);
        let recent = [p("/home/me/code/my-app"), p("/home/me/gone")];
        let rows = picker_rows(
            "",
            &recent,
            &[],
            Path::new("/elsewhere"),
            Some(&home),
            |f| f != Path::new("/home/me/gone"),
        );
        assert_eq!(
            shown(&rows),
            [
                "# Recent",
                "~/code/my-app",
                "This Computer \u{25b8}",
                "New Project",
                "No Project"
            ]
        );
        // None left: no Recent heading.
        let rows = picker_rows("", &recent, &[], Path::new("/x"), Some(&home), |_| false);
        assert_eq!(
            shown(&rows),
            ["This Computer \u{25b8}", "New Project", "No Project"]
        );
    }

    #[test]
    fn search_filters_every_section() {
        let home = p(HOME);
        let recent = [p("/home/me/code/my-app"), p("/home/me/code/scratch")];
        let computer = [
            p("/home/me/code/my-app"),
            p("/home/me/code/scratch"),
            p("/home/me/code/notes"),
            p("/srv/screens"),
        ];
        let rows = |q: &str| {
            shown(&picker_rows(
                q,
                &recent,
                &computer,
                Path::new("/x"),
                Some(&home),
                |_| true,
            ))
        };
        assert_eq!(
            rows("scr"),
            [
                "# Recent",
                "~/code/scratch",
                "# This Computer",
                "/srv/screens"
            ],
            "Recent, then This Computer's others; no project row matches"
        );
        assert_eq!(
            rows("notes"),
            ["# This Computer", "~/code/notes"],
            "an empty Recent loses its heading"
        );
        assert_eq!(rows("new"), ["New Project"]);
        // Fuzzy: letters in order, so "no" is in both project rows' words.
        assert_eq!(rows("no pro"), ["New Project", "No Project"]);
        assert!(rows("zzz").is_empty());
        // Blank is no search.
        assert_eq!(rows("  "), rows(""));
    }

    #[test]
    fn rows_mark_the_current_folder() {
        let home = p(HOME);
        let recent = [p("/home/me/code/my-app"), p("/home/me/code/scratch")];
        let rows = picker_rows(
            "",
            &recent,
            &[],
            Path::new("/home/me/code/scratch"),
            Some(&home),
            |_| true,
        );
        assert_eq!(
            rows[1],
            folder_row(&recent[0], Path::new("/x"), Some(&home))
        );
        assert_eq!(
            rows[2],
            PickerRow::Folder {
                path: recent[1].clone(),
                shown: "~/code/scratch".into(),
                current: true,
            }
        );
        assert_eq!(
            shown(&rows)[..3],
            ["# Recent", "~/code/my-app", "~/code/scratch \u{2713}"]
        );
    }

    #[test]
    fn a_folder_is_listed_once_checked_off_the_ui_thread() {
        let dir = crate::test_temp::dir("fuxx-test-there");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let gone = dir.join("gone");
        let mut checks = FolderChecks::default();
        assert!(!checks.exists(&dir), "not checked yet: not listed");
        let asked = checks.begin(&[dir.clone(), gone.clone(), dir.clone()]);
        assert_eq!(asked, Some(vec![dir.clone(), gone.clone()]), "each once");
        assert_eq!(
            checks.begin(std::slice::from_ref(&dir)),
            None,
            "one check at a time"
        );
        // The disk is read by `check` (a background thread's work), not by `exists`.
        checks.done(FolderChecks::check(asked.unwrap()));
        assert!(checks.exists(&dir));
        assert!(!checks.exists(&gone), "a folder that isn't there");
        assert_eq!(
            checks.begin(&[dir.clone(), gone.clone()]),
            None,
            "known: not checked again"
        );
        let new = dir.join("new");
        assert_eq!(checks.begin(&[dir.clone(), new.clone()]), Some(vec![new]));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn computer_folders_dedupe() {
        let projects = [p("/home/me/code/scratch"), p("/home/me/code/my-app")];
        let agent = [
            p("/home/me/code/my-app"),
            p("/home/me/code/notes"),
            p("/home/me/code/scratch"),
        ];
        assert_eq!(
            computer_folders(&projects, &agent),
            [
                "/home/me/code/my-app",
                "/home/me/code/notes",
                "/home/me/code/scratch"
            ]
            .map(p)
        );
        assert!(computer_folders(&[], &[]).is_empty());
    }
}
