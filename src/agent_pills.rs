//! Files as pills in the agent tab's box (batch 3, chore 5): a path dragged from the Files tree,
//! dropped from Finder or added from the tree's menu sits in the text as the path the `@` list
//! would insert (a folder's ends in "/"), drawn as a pill that edits as one, and opens its file
//! tab when clicked. In the sent message it is a chip with the file icon. Pure parts first; the
//! tab's side after them.

use std::path::{Path, PathBuf};

use gpui::{App, AppContext, Context, IntoElement, ParentElement, Render, Styled, Window, div, px};

use crate::icons::{Icon, icon};
use crate::terminal_view::hsla;

/// The empty box's words while files are dragged over the tab.
pub(crate) const DROP_PLACEHOLDER: &str = "Drop to add to your message";

/// What the Files tree's rows carry while dragged (batch 3, chore 5): each path, and whether
/// it is a folder.
#[derive(Clone, Debug)]
pub(crate) struct DraggedPaths {
    pub paths: Vec<PillPath>,
}

/// Paths dropped on an agent tab: from the Files tree, or from Finder.
pub(crate) enum DropPaths {
    Tree(Vec<PillPath>),
    External(Vec<PathBuf>),
}

/// The chip that follows the pointer while tree rows are dragged: a pill (the tab chip's
/// family) 28 tall with round ends, at most 240 wide, the file's or folder's icon and its name.
pub(crate) struct PathsChip {
    pub name: gpui::SharedString,
    pub is_dir: bool,
}

impl PathsChip {
    /// The chip for `dragged`: its first path's name (one more: "and 1 more").
    pub(crate) fn of(dragged: &DraggedPaths) -> Self {
        let first = dragged.paths.first();
        let mut name = first
            .and_then(|p| p.path.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if dragged.paths.len() > 1 {
            name.push_str(&format!(" and {} more", dragged.paths.len() - 1));
        }
        Self {
            name: name.into(),
            is_dir: first.is_some_and(|p| p.is_dir),
        }
    }
}

/// A drag of paths just starting shows the copy cursor until it ends (set once GPUI has made
/// the drag, as `tab_bar::hold_closed_hand` does).
pub(crate) fn hold_drag_copy(window: &Window, cx: &mut App) {
    window.defer(cx, |window, cx| {
        cx.set_active_drag_cursor_style(gpui::CursorStyle::DragCopy, window);
    });
}

/// The chip's height, widest and icon.
const CHIP_H: f32 = 28.0;
const CHIP_MAX_W: f32 = 240.0;
const CHIP_ICON: f32 = 14.0;

impl Render for PathsChip {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ui = cx.global::<crate::appearance::Appearance>().theme.ui();
        div()
            .flex()
            .flex_row()
            .items_center()
            .max_w(px(CHIP_MAX_W))
            .h(px(CHIP_H))
            .px(px(10.0))
            .gap(px(6.0))
            .rounded(px(CHIP_H / 2.0))
            .opacity(0.95)
            .shadow_md()
            .bg(hsla(ui.panel))
            .border_1()
            .border_color(hsla(ui.panel_border))
            .text_size(px(crate::ui_tokens::text::UI))
            .text_color(hsla(ui.text))
            .child(icon(
                if self.is_dir {
                    Icon::Folder
                } else {
                    Icon::File
                },
                px(CHIP_ICON),
                hsla(ui.text_dim),
            ))
            .child(div().min_w(px(0.0)).truncate().child(self.name.clone()))
    }
}

/// A path pill's file: where it is and whether it is a folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PillPath {
    pub path: PathBuf,
    pub is_dir: bool,
}

/// What a path pill says in the box (and is sent as): the path as the `@` list inserts it
/// (`inserted_path`: from the agent's folder `cwd` when inside it, else whole), a folder's
/// ending in "/".
pub(crate) fn pill_text(cwd: &Path, path: &Path, is_dir: bool) -> String {
    let mut text = super::inserted_path(cwd, cwd, &path.to_string_lossy());
    if is_dir && !text.ends_with('/') {
        text.push('/');
    }
    text
}

/// The chips' words for `paths` (each with whether it is a folder): its name (a folder's with
/// "/"); two with the same name show their parent too.
pub(crate) fn pill_labels(paths: &[PillPath]) -> Vec<String> {
    let name = |p: &PillPath| {
        let mut n = p
            .path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| p.path.to_string_lossy().into_owned());
        if p.is_dir {
            n.push('/');
        }
        n
    };
    let names: Vec<String> = paths.iter().map(name).collect();
    paths
        .iter()
        .zip(&names)
        .map(|(p, n)| {
            let clash = names.iter().filter(|m| *m == n).count() > 1;
            let parent = p
                .path
                .parent()
                .and_then(Path::file_name)
                .map(|f| f.to_string_lossy().into_owned());
            match parent {
                Some(parent) if clash => format!("{parent}/{n}"),
                _ => n.clone(),
            }
        })
        .collect()
}

/// What goes into the box for `pills` with the character before the caret `before`: the pills
/// apart by spaces, a space after them, and one before them unless the caret starts a word.
pub(crate) fn spaced(pills: &[String], before: Option<char>) -> String {
    if pills.is_empty() {
        return String::new();
    }
    let lead = if before.is_some_and(|c| !c.is_whitespace()) {
        " "
    } else {
        ""
    };
    format!("{lead}{} ", pills.join(" "))
}

/// Which of `pills` (their texts) are in `text`, each once, in the order they first show (on
/// word edges, as the box finds them).
pub(crate) fn pills_in(text: &str, pills: &[String]) -> Vec<String> {
    let mut buf = crate::composer::EditBuffer::new(text);
    buf.set_atoms(pills.to_vec());
    let mut out: Vec<String> = Vec::new();
    for r in buf.atom_ranges() {
        let found = text[r].to_string();
        if pills.contains(&found) && !out.contains(&found) {
            out.push(found);
        }
    }
    out
}

/// A path as a shell reads it when typed (a drop on a terminal, as macOS terminals do): every
/// character a shell would take for something else escaped with a backslash; one with a
/// control character (a newline) single-quoted instead.
pub(crate) fn shell_escaped(path: &str) -> String {
    if path.chars().any(char::is_control) {
        return format!("'{}'", path.replace('\'', "'\\''"));
    }
    let mut out = String::with_capacity(path.len());
    for c in path.chars() {
        let plain = c.is_alphanumeric()
            || matches!(c, '_' | '-' | '.' | '/' | ',' | ':' | '+' | '@' | '%' | '=');
        if !plain {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// What a drop of `paths` on a terminal types: each shell-escaped, apart by spaces, a space
/// after.
pub(crate) fn typed_paths(paths: &[PathBuf]) -> String {
    let words: Vec<String> = paths
        .iter()
        .map(|p| shell_escaped(&p.to_string_lossy()))
        .collect();
    if words.is_empty() {
        return String::new();
    }
    format!("{} ", words.join(" "))
}

impl super::AgentTab {
    /// What the box draws as pills that edit as one: the pasted images' markers and the path
    /// pills' texts.
    pub(super) fn box_atoms(&self) -> Vec<String> {
        let mut atoms = self.pills.clone();
        atoms.extend(self.file_pills.keys().cloned());
        atoms
    }

    /// `paths` as path pills in the box, at the caret when it has the keyboard (`at_caret`),
    /// else at the end, with a space after (and one before when the caret ends a word). An
    /// error tab (no agent) takes none.
    pub(crate) fn add_path_pills(
        &mut self,
        paths: Vec<PillPath>,
        at_caret: bool,
        cx: &mut Context<Self>,
    ) {
        if self.provider.is_none() || paths.is_empty() {
            return;
        }
        let texts: Vec<String> = paths
            .iter()
            .map(|p| pill_text(&self.start.cwd, &p.path, p.is_dir))
            .collect();
        for (text, pill) in texts.iter().zip(paths) {
            self.file_pills.insert(text.clone(), pill);
        }
        let atoms = self.box_atoms();
        self.composer.update(cx, |c, cx| {
            c.set_atoms(atoms, cx);
            let at = if at_caret { c.cursor() } else { c.text().len() };
            let before = c.text()[..at].chars().next_back();
            let words = spaced(&texts, before);
            if at_caret {
                c.insert(&words, cx);
            } else {
                c.replace(at..at, &words, cx);
            }
        });
        cx.notify();
    }

    /// Files dropped on the tab from Finder (`ExternalPaths`): images become image pills in
    /// the text (as pasted ones), anything else a path pill. Whether each is a folder is
    /// asked off the UI thread.
    pub(crate) fn drop_external(
        &mut self,
        paths: Vec<PathBuf>,
        at_caret: bool,
        cx: &mut Context<Self>,
    ) {
        if self.provider.is_none() {
            return;
        }
        let job = cx.background_spawn(async move {
            paths
                .into_iter()
                .map(|path| {
                    let is_dir = path.is_dir();
                    PillPath { path, is_dir }
                })
                .collect::<Vec<_>>()
        });
        cx.spawn(async move |this, cx| {
            let found = job.await;
            this.update(cx, |this, cx| {
                let (images, others): (Vec<PillPath>, Vec<PillPath>) = found
                    .into_iter()
                    .partition(|p| !p.is_dir && super::media_type_for(&p.path).is_some());
                this.add_path_pills(others, at_caret, cx);
                for image in images {
                    this.add_inline_image(image.path, cx);
                }
            })
            .ok();
        })
        .detach();
    }

    /// Paths dropped on the tab (its page, its box or its tab pill): pills at the caret when
    /// the box has the keyboard, else at the end.
    pub(crate) fn drop_on_tab(&mut self, drop: DropPaths, window: &Window, cx: &mut Context<Self>) {
        use gpui::Focusable;
        let at_caret = self.composer.read(cx).focus_handle(cx).is_focused(window);
        self.set_drop_over(false, cx);
        match drop {
            DropPaths::Tree(paths) => self.add_path_pills(paths, at_caret, cx),
            DropPaths::External(paths) => self.drop_external(paths, at_caret, cx),
        }
    }

    /// Paths dragged over the page, or not any more: its drop states (the page's tint, the
    /// box's ring, the empty box's words).
    pub(crate) fn set_drop_over(&mut self, over: bool, cx: &mut Context<Self>) {
        if self.drop_over != over {
            self.drop_over = over;
            self.update_placeholder(cx);
            cx.notify();
        }
    }

    /// Whether the drop states show: paths over the page, while a drag is on.
    pub(crate) fn dropping(&self, cx: &App) -> bool {
        self.drop_over && cx.has_active_drag()
    }

    /// A path pill (or its sent chip) clicked: its file opens in a tab, its folder shows in
    /// the Files tree.
    pub(super) fn open_pill(&mut self, pill: PillPath, cx: &mut Context<Self>) {
        cx.emit(if pill.is_dir {
            super::AgentTabEvent::RevealInFiles(pill.path)
        } else {
            super::AgentTabEvent::OpenFile(pill.path)
        });
    }

    /// At send: the path pills in `text` as its chips (`Attachment::PathPill`, each file kept
    /// by its words for a click), and the box's pills forgotten (the next message starts with
    /// none).
    pub(super) fn take_sent_pills(&mut self, text: &str) -> Vec<super::Attachment> {
        let keys: Vec<String> = self.file_pills.keys().cloned().collect();
        let found: Vec<PillPath> = pills_in(text, &keys)
            .into_iter()
            .filter_map(|k| self.file_pills.get(&k).cloned())
            .collect();
        self.file_pills.clear();
        let labels = pill_labels(&found);
        labels
            .into_iter()
            .zip(found)
            .map(|(label, pill)| {
                self.sent_pills.insert(label.clone(), pill.clone());
                super::Attachment::PathPill {
                    path: pill.path,
                    is_dir: pill.is_dir,
                    label,
                }
            })
            .collect()
    }

    /// The box's atoms again (after its path pills changed).
    pub(super) fn sync_box_atoms(&mut self, cx: &mut Context<Self>) {
        let atoms = self.box_atoms();
        self.composer.update(cx, |c, cx| c.set_atoms(atoms, cx));
    }

    /// A rewound message's path pills back in the box as pills (its text has them).
    pub(super) fn restore_pills(&mut self, attachments: &[super::Attachment]) {
        for a in attachments {
            if let super::Attachment::PathPill { path, is_dir, .. } = a {
                let text = pill_text(&self.start.cwd, path, *is_dir);
                self.file_pills.insert(
                    text,
                    PillPath {
                        path: path.clone(),
                        is_dir: *is_dir,
                    },
                );
            }
        }
    }

    /// The file a sent message's chip `label` stands for, if it was a path pill.
    pub(super) fn sent_pill(&self, label: &str) -> Option<&PillPath> {
        self.sent_pills.get(label)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pp(path: &str, is_dir: bool) -> PillPath {
        PillPath {
            path: PathBuf::from(path),
            is_dir,
        }
    }

    #[test]
    fn a_pill_says_the_path_as_the_at_list_would() {
        let cwd = Path::new("/home/me/code/my-app");
        assert_eq!(
            pill_text(cwd, Path::new("/home/me/code/my-app/src/main.rs"), false),
            "src/main.rs"
        );
        // A folder ends in "/"; one outside the agent's folder is whole.
        assert_eq!(
            pill_text(cwd, Path::new("/home/me/code/my-app/src"), true),
            "src/"
        );
        assert_eq!(
            pill_text(cwd, Path::new("/home/me/code/fuxx-cli/src/lib.rs"), false),
            "/home/me/code/fuxx-cli/src/lib.rs"
        );
    }

    #[test]
    fn chips_say_the_name_and_a_parent_where_two_names_clash() {
        let paths = [
            pp("/a/code/my-app/src/main.rs", false),
            pp("/a/code/my-app/cli/main.rs", false),
            pp("/a/code/my-app/docs", true),
            pp("/a/code/my-app/README.md", false),
        ];
        assert_eq!(
            pill_labels(&paths),
            ["src/main.rs", "cli/main.rs", "docs/", "README.md"]
        );
    }

    #[test]
    fn pills_go_in_with_spaces_around_them() {
        let pills = ["src/main.rs".to_string(), "docs/".to_string()];
        assert_eq!(spaced(&pills, None), "src/main.rs docs/ ");
        assert_eq!(spaced(&pills, Some(' ')), "src/main.rs docs/ ");
        assert_eq!(spaced(&pills, Some('\n')), "src/main.rs docs/ ");
        assert_eq!(spaced(&pills[..1], Some('k')), " src/main.rs ");
        assert_eq!(spaced(&[], Some('k')), "");
    }

    #[test]
    fn the_pills_in_a_text_are_found_on_word_edges_once_each() {
        let pills = ["src/main.rs".to_string(), "docs/".to_string()];
        assert_eq!(
            pills_in("see docs/ and src/main.rs, again src/main.rs", &pills),
            ["docs/", "src/main.rs"]
        );
        assert_eq!(
            pills_in("code/src/main.rs is another", &pills),
            Vec::<String>::new()
        );
    }

    #[test]
    fn a_dropped_path_is_typed_shell_escaped() {
        assert_eq!(shell_escaped("/tmp/notes.md"), "/tmp/notes.md");
        assert_eq!(
            shell_escaped("/tmp/My File (1).txt"),
            "/tmp/My\\ File\\ \\(1\\).txt"
        );
        assert_eq!(shell_escaped("/tmp/it's $HOME"), "/tmp/it\\'s\\ \\$HOME");
        assert_eq!(shell_escaped("/tmp/café"), "/tmp/café");
        assert_eq!(shell_escaped("/tmp/a\nb"), "'/tmp/a\nb'");
        assert_eq!(
            typed_paths(&[PathBuf::from("/tmp/a b"), PathBuf::from("/tmp/c")]),
            "/tmp/a\\ b /tmp/c "
        );
        assert_eq!(typed_paths(&[]), "");
    }
}
