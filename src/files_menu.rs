//! The Files tree's right-click menu (batch 3, chore 7): what its items do, the inline Rename
//! and New boxes, Move to Trash with its sheet, and ⌘Z putting the last one back.

use std::path::{Path, PathBuf};

use gpui::{
    AppContext, ClipboardItem, Context, Entity, Focusable, Pixels, Point, PromptLevel, Window,
};

use crate::context_menu::{self, MenuAction};
use crate::file_tree::Row;
use crate::layout_model::{FileMode, TabKind};
use crate::name_input::{NameInput, NameInputEvent};
use crate::restore::{Launch, SavedSession};
use crate::workspace::{At, Workspace};
use crate::workspace_menu::MenuFrom;

/// Where the Rename box's selection ends in `name`: before the last extension for a file
/// (`main` of `main.rs`, `archive.tar` of `archive.tar.gz`), the whole name for a folder or a
/// name starting with its only dot (`.gitignore`).
pub fn stem_end(name: &str, is_dir: bool) -> usize {
    if is_dir {
        return name.len();
    }
    match name.rfind('.') {
        Some(dot) if dot > 0 => dot,
        _ => name.len(),
    }
}

/// What is wrong with `name` for a new or renamed entry, said under the box; none: it can be
/// made. `taken`: the folder already has an entry by that name.
pub fn name_problem(name: &str, taken: bool) -> Option<String> {
    let name = name.trim();
    if name.is_empty() {
        Some("Type a name.".into())
    } else if name.contains('/') {
        Some("A name can\u{2019}t have \u{201c}/\u{201d} in it.".into())
    } else if name == "." || name == ".." {
        Some("That name is taken by the system.".into())
    } else if taken {
        Some(format!("\u{201c}{name}\u{201d} is already here."))
    } else {
        None
    }
}

/// Where the New box's row goes among `rows`: just under folder `parent`'s row (the top of the
/// list for the root).
pub fn new_row_at(rows: &[Row], parent: &Path) -> usize {
    if parent.as_os_str().is_empty() {
        return 0;
    }
    rows.iter()
        .position(|r| r.more.is_none() && r.rel == parent)
        .map_or(0, |at| at + 1)
}

/// Move `path` to the Trash and say where it went (for ⌘Z). Off the UI thread. `fake` (the
/// scenes, which never touch your Trash): moved into that folder instead.
pub fn trash_item(path: &Path, fake: Option<&Path>) -> Result<PathBuf, String> {
    let Some(trash) = fake else {
        return trash_natively(path);
    };
    let name = path.file_name().ok_or("It has no name.")?.to_string_lossy();
    let mut to = trash.join(name.as_ref());
    let mut n = 1;
    while to.symlink_metadata().is_ok() {
        n += 1;
        to = trash.join(format!("{name} {n}"));
    }
    std::fs::rename(path, &to).map_err(|e| e.to_string())?;
    Ok(to)
}

/// Finder's own Move to Trash (`NSFileManager`), which says where the item went.
fn trash_natively(path: &Path) -> Result<PathBuf, String> {
    use objc2_foundation::{NSFileManager, NSString, NSURL};
    objc2::rc::autoreleasepool(|_| {
        let url = NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));
        let mut went = None;
        NSFileManager::defaultManager()
            .trashItemAtURL_resultingItemURL_error(&url, Some(&mut went))
            .map_err(|e| e.localizedDescription().to_string())?;
        went.and_then(|u| u.path())
            .map(|p| PathBuf::from(p.to_string()))
            .ok_or_else(|| "The Trash didn\u{2019}t say where it went.".to_string())
    })
}

/// Put an item trashed to `from` back at `to` (⌘Z): never over something now at `to`.
pub fn put_back(from: &Path, to: &Path) -> Result<(), String> {
    if to.symlink_metadata().is_ok() {
        return Err(format!(
            "Something is at \u{201c}{}\u{201d} now.",
            to.file_name()
                .map_or_else(String::new, |n| n.to_string_lossy().into_owned())
        ));
    }
    std::fs::rename(from, to).map_err(|e| e.to_string())
}

/// The Move to Trash sheet's words for `name`.
pub fn trash_sheet_text(name: &str) -> (String, &'static str) {
    (
        format!("Move \u{201c}{name}\u{201d} to the Trash?"),
        "You can put it back with \u{2318}Z or from the Trash in Finder.",
    )
}

/// What an inline box in the tree makes: a new name for a row, or a new entry in a folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum EditKind {
    Rename { rel: PathBuf, is_dir: bool },
    New { parent: PathBuf, is_dir: bool },
}

impl EditKind {
    /// The folder the name goes in (from the tree's root).
    fn folder(&self) -> PathBuf {
        match self {
            EditKind::Rename { rel, .. } => rel.parent().map(Path::to_path_buf).unwrap_or_default(),
            EditKind::New { parent, .. } => parent.clone(),
        }
    }
}

/// The inline Rename or New box in the tree, and what is wrong with its name (under it).
pub(crate) struct TreeEdit {
    pub kind: EditKind,
    pub input: Entity<NameInput>,
    pub problem: Option<String>,
    /// Its change is being made on disk (off the UI thread): Esc and a lost focus wait.
    busy: bool,
}

impl Workspace {
    /// A right-click (or ⌃-click) on a Files row, or ⇧F10 on the selected one: the row is
    /// selected and its menu opens at `at`.
    pub(crate) fn files_row_menu(
        &mut self,
        rel: PathBuf,
        is_dir: bool,
        at: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let files = &mut self.right_sidebar.files;
        let Some(tree) = files.tree.as_mut() else {
            return;
        };
        tree.select(&rel);
        // The tree takes the keyboard, as a click in it does: the menu gives it back to it.
        if let Some(focus) = files.focus.clone() {
            focus.focus(window, cx);
        }
        let files = &self.right_sidebar.files;
        let marked = !is_dir && files.rows.iter().any(|r| r.rel == rel && r.mark.is_some());
        let entries = context_menu::file_tree_menu(is_dir, self.files_add_target(), marked);
        self.open_context_menu(MenuFrom::FileTree { rel, is_dir }, entries, at, window, cx);
    }

    /// Who the Files menu's Add to Chat would add to. None until path pills come (batch 3, wave
    /// 3: a pill in `ask_pick`'s agent tab), which leaves the item out.
    fn files_add_target(&self) -> Option<&'static str> {
        None
    }

    /// ⇧F10 in the tree: the selected row's menu, at its row as last drawn.
    pub(crate) fn files_show_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let files = &self.right_sidebar.files;
        let Some(rel) = files
            .tree
            .as_ref()
            .and_then(|t| t.selected())
            .map(Path::to_path_buf)
        else {
            return;
        };
        let Some(i) = files
            .rows
            .iter()
            .position(|r| r.more.is_none() && r.rel == rel)
        else {
            return;
        };
        let is_dir = files.rows[i].is_dir;
        let at = files
            .row_place(i)
            .unwrap_or_else(|| crate::workspace_menu::keyboard_at(None));
        self.files_row_menu(rel, is_dir, at, window, cx);
    }

    /// The tree's root.
    fn files_root(&self) -> Option<PathBuf> {
        Some(self.right_sidebar.files.tree.as_ref()?.root().to_path_buf())
    }

    /// The selected row and whether it is a folder (the tree's own keys act on it).
    fn files_selected(&self) -> Option<(PathBuf, bool)> {
        let files = &self.right_sidebar.files;
        let rel = files.tree.as_ref()?.selected()?.to_path_buf();
        let is_dir = files
            .rows
            .iter()
            .find(|r| r.more.is_none() && r.rel == rel)?
            .is_dir;
        Some((rel, is_dir))
    }

    /// A tree key that acts on the selected row as its menu item would (⌥⌘C, ⌥⇧⌘C, ⌘⌫).
    pub(crate) fn files_key_action(
        &mut self,
        action: MenuAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.right_sidebar.files.edit.is_some() {
            return;
        }
        if let Some((rel, is_dir)) = self.files_selected() {
            self.files_menu_action(&rel, is_dir, action, window, cx);
        }
    }

    /// A choice from a Files row's menu, for the row at `rel` (from the tree's root).
    pub(crate) fn files_menu_action(
        &mut self,
        rel: &Path,
        is_dir: bool,
        action: MenuAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(root) = self.files_root() else {
            return;
        };
        let path = root.join(rel);
        match action {
            MenuAction::Open => self.files_click(rel, false, false, window, cx),
            MenuAction::OpenToSide => self.open_file_beside(path, window, cx),
            MenuAction::ShowChanges => self.open_file_tab(path, FileMode::Diff, window, cx),
            MenuAction::Reveal => self.reveal_in_finder(path, cx),
            MenuAction::CopyPath => {
                cx.write_to_clipboard(ClipboardItem::new_string(path.display().to_string()))
            }
            MenuAction::CopyRelativePath => {
                cx.write_to_clipboard(ClipboardItem::new_string(rel.display().to_string()))
            }
            MenuAction::Rename => self.start_tree_edit(
                EditKind::Rename {
                    rel: rel.to_path_buf(),
                    is_dir,
                },
                window,
                cx,
            ),
            MenuAction::NewFile | MenuAction::NewFolder => {
                let open = self
                    .right_sidebar
                    .files
                    .tree
                    .as_ref()
                    .is_some_and(|t| t.is_open(rel));
                if !open {
                    self.files_click(rel, true, false, window, cx);
                }
                let kind = EditKind::New {
                    parent: rel.to_path_buf(),
                    is_dir: action == MenuAction::NewFolder,
                };
                self.start_tree_edit(kind, window, cx);
            }
            MenuAction::OpenInTerminal => self.terminal_in_folder(path, window, cx),
            MenuAction::Trash => self.confirm_trash(rel.to_path_buf(), window, cx),
            // Wave 3 (path pills): the item isn't offered yet.
            _ => {}
        }
    }

    /// Open to the Side: file `path` in the focused pane's right neighbour (its tab of it, else
    /// a new one), else in a new pane split right of it; shown and focused.
    pub(crate) fn open_file_beside(
        &mut self,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(pane) = self.model.focused_pane() else {
            return;
        };
        let right = self
            .shown_layout()
            .and_then(|tree| tree.neighbor(pane, crate::split_tree::Dir::Right));
        let opened = match right {
            Some(right) => self.model.open_file(right, path.clone(), FileMode::Plain),
            None => self.model.split(
                pane,
                crate::split_tree::Axis::Row,
                TabKind::File {
                    path: path.clone(),
                    mode: FileMode::Plain,
                },
            ),
        };
        if opened.is_none() {
            return;
        }
        self.sync();
        self.ensure_file_tabs(window, cx);
        self.save();
        self.activate(window, cx);
        cx.notify();
    }

    /// Reveal in Finder (`open -R`), started off the UI thread.
    fn reveal_in_finder(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        #[cfg(feature = "ui-shots")]
        if self.places.frozen {
            self.right_sidebar.files.shot_revealed.push(path);
            return;
        }
        cx.background_spawn(async move {
            let _ = std::process::Command::new("/usr/bin/open")
                .arg("-R")
                .arg(&path)
                .status();
        })
        .detach();
    }

    /// Open in Terminal: a terminal tab in folder `path`, at the end of the focused pane's bar.
    fn terminal_in_folder(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let Some(pane) = self.model.focused_pane() else {
            return;
        };
        let launch = Launch {
            saved: SavedSession {
                project: self.active_project(),
                cwd: Some(path),
                ..Default::default()
            },
            run_preset: false,
        };
        self.open_session_then(launch, At::Tab(pane), None, window, cx);
    }

    /// Rename… or New File… / New Folder…: the inline box, Rename's with the name's stem
    /// selected, taking the keyboard.
    pub(crate) fn start_tree_edit(
        &mut self,
        kind: EditKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let text = match &kind {
            EditKind::Rename { rel, .. } => rel
                .file_name()
                .map_or_else(String::new, |n| n.to_string_lossy().into_owned()),
            EditKind::New { .. } => String::new(),
        };
        let stem = match &kind {
            EditKind::Rename { is_dir, .. } => stem_end(&text, *is_dir),
            EditKind::New { .. } => 0,
        };
        let input = cx.new(|cx| {
            let mut input = NameInput::new(text, window, cx);
            input.select_range(0..stem);
            input
        });
        cx.subscribe_in(&input, window, Self::on_tree_edit_event)
            .detach();
        input.focus_handle(cx).focus(window, cx);
        self.right_sidebar.files.edit = Some(TreeEdit {
            kind,
            input,
            problem: None,
            busy: false,
        });
        self.right_sidebar.files.reveal = Some(gpui::ScrollStrategy::Nearest);
        cx.notify();
    }

    fn on_tree_edit_event(
        &mut self,
        input: &Entity<NameInput>,
        event: &NameInputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let files = &mut self.right_sidebar.files;
        let Some(edit) = files.edit.as_mut().filter(|e| e.input == *input) else {
            return;
        };
        if edit.busy {
            return;
        }
        match event {
            NameInputEvent::Cancel => {
                // An error under the box keeps it up until Esc: only Esc (focused) ends it then.
                if edit.problem.is_some() && !input.focus_handle(cx).is_focused(window) {
                    return;
                }
                self.end_tree_edit(window, cx);
            }
            NameInputEvent::Confirm(name) => {
                let name = name.trim().to_string();
                let kind = edit.kind.clone();
                let folder = kind.folder();
                let unchanged = matches!(&kind, EditKind::Rename { rel, .. } if rel.file_name().is_some_and(|n| n.to_string_lossy() == name));
                if unchanged {
                    self.end_tree_edit(window, cx);
                    return;
                }
                let taken = files
                    .tree
                    .as_ref()
                    .is_some_and(|t| t.has_entry(&folder, &name));
                if let Some(problem) = name_problem(&name, taken) {
                    edit.problem = Some(problem);
                    cx.notify();
                    return;
                }
                edit.busy = true;
                self.make_tree_edit(kind, folder, name, window, cx);
            }
        }
    }

    /// The box's name made on disk off the UI thread: the row renamed, the file or folder made.
    /// Done, the folder is listed again and the new row selected; failed, the reason shows
    /// under the box.
    fn make_tree_edit(
        &mut self,
        kind: EditKind,
        folder: PathBuf,
        name: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(root) = self.files_root() else {
            return;
        };
        let target = root.join(&folder).join(&name);
        let job = {
            let (kind, target, root) = (kind.clone(), target.clone(), root.clone());
            cx.background_spawn(async move {
                if target.symlink_metadata().is_ok() {
                    return Err(format!("\u{201c}{name}\u{201d} is already here."));
                }
                let made = match kind {
                    EditKind::Rename { rel, .. } => std::fs::rename(root.join(rel), &target),
                    EditKind::New { is_dir: true, .. } => std::fs::create_dir(&target),
                    EditKind::New { is_dir: false, .. } => std::fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(&target)
                        .map(|_| ()),
                };
                made.map_err(|e| e.to_string())
            })
        };
        let rel = folder.join(target.file_name().unwrap_or_default());
        cx.spawn_in(window, async move |this, cx| {
            let done = job.await;
            this.update_in(cx, |this, window, cx| {
                let files = &mut this.right_sidebar.files;
                let Some(edit) = files.edit.as_mut().filter(|e| e.kind == kind) else {
                    return;
                };
                edit.busy = false;
                match done {
                    Ok(()) => {
                        files.edit = None;
                        if let Some(tree) = files.tree.as_mut() {
                            tree.select(&rel);
                        }
                        files.reveal = Some(gpui::ScrollStrategy::Nearest);
                        this.files_list(vec![folder], cx);
                        if let Some(focus) = this.right_sidebar.files.focus.clone() {
                            focus.focus(window, cx);
                        }
                    }
                    Err(why) => edit.problem = Some(why),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// The box goes; the tree takes the keyboard back.
    fn end_tree_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.right_sidebar.files.edit.take().is_some() {
            if let Some(focus) = self.right_sidebar.files.focus.clone() {
                focus.focus(window, cx);
            }
            cx.notify();
        }
    }

    /// Move to Trash: asked in a native sheet first; Move to Trash moves it (`trash_now`).
    fn confirm_trash(&mut self, rel: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let name = rel
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        let (title, detail) = trash_sheet_text(&name);
        let answer = window.prompt(
            PromptLevel::Warning,
            &title,
            Some(detail),
            &["Move to Trash", "Cancel"],
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            if answer.await != Ok(0) {
                return;
            }
            this.update(cx, |this, cx| this.trash_now(rel, cx)).ok();
        })
        .detach();
    }

    /// Move the row at `rel` to the Trash off the UI thread, remembered for ⌘Z; its folder is
    /// listed again.
    pub(crate) fn trash_now(&mut self, rel: PathBuf, cx: &mut Context<Self>) {
        let Some(root) = self.files_root() else {
            return;
        };
        let path = root.join(&rel);
        #[cfg(feature = "ui-shots")]
        let fake = self.right_sidebar.files.fake_trash.clone();
        #[cfg(not(feature = "ui-shots"))]
        let fake: Option<PathBuf> = None;
        let job = {
            let path = path.clone();
            cx.background_spawn(async move { trash_item(&path, fake.as_deref()) })
        };
        let folder = rel.parent().map(Path::to_path_buf).unwrap_or_default();
        cx.spawn(async move |this, cx| {
            let went = job.await;
            this.update(cx, |this, cx| match went {
                Ok(went) => {
                    this.right_sidebar.files.trashed.push((path, went));
                    this.files_list(vec![folder], cx);
                    cx.notify();
                }
                Err(why) => eprintln!("[fuxx] files: couldn't move to the Trash: {why}"),
            })
            .ok();
        })
        .detach();
    }

    /// ⌘Z in the tree: the last item trashed this session goes back where it was (off the UI
    /// thread), selected; never over something now there.
    pub(crate) fn put_back_trashed(&mut self, cx: &mut Context<Self>) {
        let Some((to, from)) = self.right_sidebar.files.trashed.pop() else {
            return;
        };
        let job = {
            let (from, to) = (from.clone(), to.clone());
            cx.background_spawn(async move { put_back(&from, &to) })
        };
        cx.spawn(async move |this, cx| {
            let done = job.await;
            this.update(cx, |this, cx| {
                if let Err(why) = done {
                    eprintln!("[fuxx] files: couldn't put it back: {why}");
                    this.right_sidebar.files.trashed.push((to, from));
                    return;
                }
                let Some(root) = this.files_root() else {
                    return;
                };
                let Ok(rel) = to.strip_prefix(&root).map(Path::to_path_buf) else {
                    return;
                };
                let folder = rel.parent().map(Path::to_path_buf).unwrap_or_default();
                let files = &mut this.right_sidebar.files;
                if let Some(tree) = files.tree.as_mut() {
                    tree.select(&rel);
                }
                files.reveal = Some(gpui::ScrollStrategy::Nearest);
                this.files_list(vec![folder], cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_trash_sheet_names_the_item_and_how_to_put_it_back() {
        assert_eq!(
            trash_sheet_text("main.rs"),
            (
                "Move \u{201c}main.rs\u{201d} to the Trash?".to_string(),
                "You can put it back with \u{2318}Z or from the Trash in Finder."
            )
        );
    }

    #[test]
    fn rename_selects_the_name_before_its_extension() {
        assert_eq!(stem_end("main.rs", false), 4);
        assert_eq!(stem_end("archive.tar.gz", false), 11);
        assert_eq!(stem_end(".gitignore", false), 10);
        assert_eq!(stem_end("Makefile", false), 8);
        assert_eq!(stem_end("my.folder", true), 9);
        assert_eq!(stem_end("é.rs", false), "é".len());
    }

    #[test]
    fn a_name_must_be_new_whole_and_without_a_slash() {
        assert_eq!(name_problem("notes.md", false), None);
        assert_eq!(
            name_problem("notes.md", true).as_deref(),
            Some("\u{201c}notes.md\u{201d} is already here.")
        );
        assert_eq!(name_problem("  ", false).as_deref(), Some("Type a name."));
        assert_eq!(
            name_problem("a/b", false).as_deref(),
            Some("A name can\u{2019}t have \u{201c}/\u{201d} in it.")
        );
        for dots in [".", ".."] {
            assert_eq!(
                name_problem(dots, false).as_deref(),
                Some("That name is taken by the system."),
                "{dots}"
            );
        }
    }

    #[test]
    fn a_trashed_item_goes_and_comes_back_but_never_over_a_new_one() {
        let dir = crate::test_temp::dir("files-menu-trash");
        let (folder, trash) = (dir.join("w"), dir.join("trash"));
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::create_dir_all(&trash).unwrap();
        let file = folder.join("main.rs");
        std::fs::write(&file, "fn main() {}").unwrap();
        let went = trash_item(&file, Some(&trash)).unwrap();
        assert!(!file.exists() && went.starts_with(&trash) && went.exists());
        // Another of the same name trashed: kept apart.
        std::fs::write(&file, "again").unwrap();
        let second = trash_item(&file, Some(&trash)).unwrap();
        assert_ne!(second, went);
        // Back: not over the one now there.
        std::fs::write(&file, "new").unwrap();
        assert!(put_back(&went, &file).is_err());
        std::fs::remove_file(&file).unwrap();
        put_back(&went, &file).unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "fn main() {}");
        assert!(!went.exists());
    }

    fn row(rel: &str, depth: usize, is_dir: bool) -> Row {
        Row {
            rel: PathBuf::from(rel),
            depth,
            is_dir,
            expanded: is_dir,
            mark: None,
            more: None,
        }
    }

    #[test]
    fn the_new_box_sits_under_its_folder() {
        let rows = [
            row("src", 0, true),
            row("src/main.rs", 1, false),
            row("README.md", 0, false),
        ];
        assert_eq!(new_row_at(&rows, Path::new("")), 0);
        assert_eq!(new_row_at(&rows, Path::new("src")), 1);
        // A folder not shown (filtered away): the top.
        assert_eq!(new_row_at(&rows, Path::new("docs")), 0);
    }
}
