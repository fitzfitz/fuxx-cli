//! The one right-click menu every surface shares (batch 3, chore 4): the terminal, a file tab's
//! text (and its Preview), the Files tree (chore 7), and (wave 3) the agent chat and its box
//! build their items here, and the workspace opens and acts
//! on them (`workspace_menu`). Pure: the items, the light that ↑/↓ move, the menu's size and
//! where it opens.
//!
//! The rule: the standard edit items (Copy, Paste, Select All) show disabled when they don't
//! apply; the context ones (a link's, Add to Chat) are left out.

use crate::split_tree::Rect;

/// The panel's width, its padding, an item's height and a separator's (1 pt with 4 above and
/// below).
pub const WIDTH: f32 = 220.0;
pub const PAD: f32 = 4.0;
pub const ITEM: f32 = 26.0;
pub const SEPARATOR: f32 = 9.0;
/// The panel's 1 pt edge, both sides.
const EDGES: f32 = 2.0;
/// How far from the window's edges the menu keeps.
pub const EDGE_MARGIN: f32 = 8.0;

/// What an item does. The workspace acts on it for the surface the menu opened from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MenuAction {
    /// The link where "Open links in" says (`Workspace::open_link`).
    OpenLink(String),
    OpenInBrowser(String),
    CopyLink(String),
    Cut,
    Copy,
    Paste,
    SelectAll,
    /// The selection as a chip in the target agent tab's box.
    AddToChat,
    // The Files tree (chore 7).
    Open,
    OpenToSide,
    ShowChanges,
    Reveal,
    CopyPath,
    CopyRelativePath,
    Rename,
    NewFile,
    NewFolder,
    OpenInTerminal,
    Trash,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuItem {
    pub label: String,
    /// Its shortcut as the menu shows it (`⌘C`).
    pub keys: Option<&'static str>,
    pub enabled: bool,
    /// Drawn in `menu_danger` (Move to Trash).
    pub danger: bool,
    pub action: MenuAction,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Entry {
    Item(MenuItem),
    Separator,
}

fn item(label: &str, keys: Option<&'static str>, enabled: bool, action: MenuAction) -> Entry {
    Entry::Item(MenuItem {
        label: label.to_string(),
        keys,
        enabled,
        danger: false,
        action,
    })
}

/// Add to Chat names its agent: "Add to Claude Chat".
pub fn add_label(target: &str) -> String {
    format!("Add to {target} Chat")
}

/// A link's three items: where "Open links in" says, the default browser, the clipboard.
fn link_group(url: &str) -> Vec<Entry> {
    vec![
        item(
            "Open Link",
            None,
            true,
            MenuAction::OpenLink(url.to_string()),
        ),
        item(
            "Open in Default Browser",
            None,
            true,
            MenuAction::OpenInBrowser(url.to_string()),
        ),
        item(
            "Copy Link",
            None,
            true,
            MenuAction::CopyLink(url.to_string()),
        ),
        Entry::Separator,
    ]
}

/// Copy, then Add to Chat when there is a selection and an agent to add it to.
fn copy_group(selection: bool, add_target: Option<&str>) -> Vec<Entry> {
    let mut out = vec![item("Copy", Some("\u{2318}C"), selection, MenuAction::Copy)];
    if let Some(target) = add_target.filter(|_| selection) {
        out.push(item(&add_label(target), None, true, MenuAction::AddToChat));
    }
    out
}

/// A selection or a link in text (a file tab, its Preview): the link's items, a line,
/// then Copy and, with something selected and an agent to add it to, Add to Chat.
pub fn text_menu(selection: bool, link: Option<&str>, add_target: Option<&str>) -> Vec<Entry> {
    let mut out = link.map(link_group).unwrap_or_default();
    out.extend(copy_group(selection, add_target));
    out
}

/// The agent chat's (wave 3): as `text_menu`, Add to Chat going to its own tab (`target`, its
/// agent's label); with nothing selected, a line and Select All ⌘A.
pub fn chat_menu(selection: bool, link: Option<&str>, target: &str) -> Vec<Entry> {
    let mut out = text_menu(selection, link, Some(target));
    if !selection {
        out.push(Entry::Separator);
        out.push(item(
            "Select All",
            Some("\u{2318}A"),
            true,
            MenuAction::SelectAll,
        ));
    }
    out
}

/// A box you type in (the agent tab's message box, wave 3): Cut and Copy (with a selection),
/// Paste, a line, Select All.
pub fn editable_menu(has_sel: bool) -> Vec<Entry> {
    vec![
        item("Cut", Some("\u{2318}X"), has_sel, MenuAction::Cut),
        item("Copy", Some("\u{2318}C"), has_sel, MenuAction::Copy),
        item("Paste", Some("\u{2318}V"), true, MenuAction::Paste),
        Entry::Separator,
        item("Select All", Some("\u{2318}A"), true, MenuAction::SelectAll),
    ]
}

/// The terminal's: the link's items and a line, then Copy (Add to Chat with a selection and a
/// target) and Paste, a line, Select All.
pub fn terminal_menu(selection: bool, link: Option<&str>, add_target: Option<&str>) -> Vec<Entry> {
    let mut out = text_menu(selection, link, add_target);
    out.push(item("Paste", Some("\u{2318}V"), true, MenuAction::Paste));
    out.push(Entry::Separator);
    out.push(item("Select All", None, true, MenuAction::SelectAll));
    out
}

/// A Files row's (chore 7): a file's or a folder's. `add_target` is the agent Add to Chat
/// would go to (wave 3: a path pill; none, as on another host: the item is left out);
/// `marked`: the file has changes, so Show Changes is offered.
pub fn file_tree_menu(is_dir: bool, add_target: Option<&str>, marked: bool) -> Vec<Entry> {
    let add = add_target.map(|t| item(&add_label(t), None, true, MenuAction::AddToChat));
    let mut out = Vec::new();
    if is_dir {
        out.push(item("New File\u{2026}", None, true, MenuAction::NewFile));
        out.push(item(
            "New Folder\u{2026}",
            None,
            true,
            MenuAction::NewFolder,
        ));
        out.push(Entry::Separator);
        out.push(item(
            "Open in Terminal",
            None,
            true,
            MenuAction::OpenInTerminal,
        ));
        out.extend(add);
    } else {
        out.push(item("Open", Some("\u{21a9}"), true, MenuAction::Open));
        out.push(item("Open to the Side", None, true, MenuAction::OpenToSide));
        out.extend(add);
        if marked {
            out.push(item("Show Changes", None, true, MenuAction::ShowChanges));
        }
        out.push(Entry::Separator);
    }
    out.push(item("Reveal in Finder", None, true, MenuAction::Reveal));
    out.push(item(
        "Copy Path",
        Some("\u{2325}\u{2318}C"),
        true,
        MenuAction::CopyPath,
    ));
    out.push(item(
        "Copy Relative Path",
        Some("\u{2325}\u{21e7}\u{2318}C"),
        true,
        MenuAction::CopyRelativePath,
    ));
    out.push(Entry::Separator);
    out.push(item("Rename\u{2026}", None, true, MenuAction::Rename));
    out.push(Entry::Separator);
    out.push(Entry::Item(MenuItem {
        label: "Move to Trash".into(),
        keys: Some("\u{2318}\u{232b}"),
        enabled: true,
        danger: true,
        action: MenuAction::Trash,
    }));
    out
}

/// Whether entry `i` can be lit: an enabled item.
fn litable(entries: &[Entry], i: usize) -> bool {
    matches!(entries.get(i), Some(Entry::Item(item)) if item.enabled)
}

/// The next item ↑ (`down` false) or ↓ lights from `lit`, past separators and disabled items;
/// with nothing lit, ↓ lights the first and ↑ the last. It stops at the ends.
pub fn step(entries: &[Entry], lit: Option<usize>, down: bool) -> Option<usize> {
    let found = match (lit, down) {
        (None, true) => (0..entries.len()).find(|&i| litable(entries, i)),
        (None, false) => (0..entries.len()).rev().find(|&i| litable(entries, i)),
        (Some(at), true) => (at + 1..entries.len()).find(|&i| litable(entries, i)),
        (Some(at), false) => (0..at).rev().find(|&i| litable(entries, i)),
    };
    found.or(lit)
}

/// The menu's height: its items and separators, its padding and its edge.
pub fn height(entries: &[Entry]) -> f32 {
    let rows: f32 = entries
        .iter()
        .map(|e| match e {
            Entry::Item(_) => ITEM,
            Entry::Separator => SEPARATOR,
        })
        .sum();
    rows + 2.0 * PAD + EDGES
}

/// Where a `w` × `h` menu opened at the pointer `at` goes in `window` (its top-left corner):
/// down and right of the pointer, flipped up or left where it wouldn't fit `EDGE_MARGIN` from
/// the window's edge, then held inside those margins.
pub fn place(at: (f32, f32), w: f32, h: f32, window: Rect) -> (f32, f32) {
    let along = |p: f32, size: f32, start: f32, len: f32| {
        let (lo, hi) = (start + EDGE_MARGIN, start + len - EDGE_MARGIN);
        let p = if p + size <= hi {
            p
        } else if p - size >= lo {
            p - size
        } else {
            p
        };
        p.min(hi - size).max(lo)
    };
    (
        along(at.0, w, window.x, window.w),
        along(at.1, h, window.y, window.h),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(entries: &[Entry]) -> Vec<String> {
        entries
            .iter()
            .map(|e| match e {
                Entry::Item(i) => i.label.clone(),
                Entry::Separator => "-".into(),
            })
            .collect()
    }

    fn found<'a>(entries: &'a [Entry], label: &str) -> &'a MenuItem {
        entries
            .iter()
            .find_map(|e| match e {
                Entry::Item(i) if i.label == label => Some(i),
                _ => None,
            })
            .unwrap_or_else(|| panic!("no {label}"))
    }

    #[test]
    fn add_to_chat_names_its_agent() {
        assert_eq!(add_label("Claude"), "Add to Claude Chat");
    }

    #[test]
    fn text_with_a_link_and_a_selection_has_both_groups() {
        let m = text_menu(true, Some("https://a.dev"), Some("Claude"));
        assert_eq!(
            labels(&m),
            [
                "Open Link",
                "Open in Default Browser",
                "Copy Link",
                "-",
                "Copy",
                "Add to Claude Chat"
            ]
        );
        assert_eq!(
            found(&m, "Copy Link").action,
            MenuAction::CopyLink("https://a.dev".into())
        );
        assert_eq!(found(&m, "Copy").keys, Some("\u{2318}C"));
    }

    #[test]
    fn copy_shows_disabled_but_add_to_chat_and_links_go_when_they_dont_apply() {
        let m = text_menu(false, None, Some("Claude"));
        assert_eq!(labels(&m), ["Copy"]);
        assert!(!found(&m, "Copy").enabled);
        // A selection with nowhere to add it (an SSH workspace): no Add to Chat.
        let m = text_menu(true, None, None);
        assert_eq!(labels(&m), ["Copy"]);
        assert!(found(&m, "Copy").enabled);
    }

    #[test]
    fn the_chat_menu_adds_select_all_with_nothing_selected() {
        // Nothing selected, no link: Copy (disabled) and Select All.
        let m = chat_menu(false, None, "Claude");
        assert_eq!(labels(&m), ["Copy", "-", "Select All"]);
        assert!(!found(&m, "Copy").enabled);
        assert_eq!(found(&m, "Select All").keys, Some("\u{2318}A"));
        assert_eq!(found(&m, "Select All").action, MenuAction::SelectAll);
        // A selection: Copy and Add to Chat (its own tab), no Select All.
        let m = chat_menu(true, None, "Claude");
        assert_eq!(labels(&m), ["Copy", "Add to Claude Chat"]);
        // On a link with nothing selected: the link's group first.
        let m = chat_menu(false, Some("https://a.dev"), "Agy");
        assert_eq!(
            labels(&m),
            [
                "Open Link",
                "Open in Default Browser",
                "Copy Link",
                "-",
                "Copy",
                "-",
                "Select All"
            ]
        );
    }

    #[test]
    fn the_box_menu_is_cut_copy_paste_and_select_all() {
        let m = editable_menu(false);
        assert_eq!(labels(&m), ["Cut", "Copy", "Paste", "-", "Select All"]);
        assert!(!found(&m, "Cut").enabled && !found(&m, "Copy").enabled);
        assert!(found(&m, "Paste").enabled && found(&m, "Select All").enabled);
        assert_eq!(found(&m, "Cut").keys, Some("\u{2318}X"));
        assert_eq!(found(&m, "Cut").action, MenuAction::Cut);
        assert_eq!(found(&m, "Select All").keys, Some("\u{2318}A"));
        let m = editable_menu(true);
        assert!(found(&m, "Cut").enabled && found(&m, "Copy").enabled);
    }

    #[test]
    fn the_terminal_menu_is_copy_paste_and_select_all() {
        let m = terminal_menu(false, None, Some("Claude"));
        assert_eq!(labels(&m), ["Copy", "Paste", "-", "Select All"]);
        assert!(!found(&m, "Copy").enabled);
        assert!(found(&m, "Paste").enabled && found(&m, "Select All").enabled);
        assert_eq!(found(&m, "Paste").keys, Some("\u{2318}V"));
        let m = terminal_menu(true, Some("mailto:a@b.c"), Some("Agy"));
        assert_eq!(
            labels(&m),
            [
                "Open Link",
                "Open in Default Browser",
                "Copy Link",
                "-",
                "Copy",
                "Add to Agy Chat",
                "Paste",
                "-",
                "Select All"
            ]
        );
    }

    #[test]
    fn a_files_file_row_menu() {
        let m = file_tree_menu(false, None, true);
        assert_eq!(
            labels(&m),
            [
                "Open",
                "Open to the Side",
                "Show Changes",
                "-",
                "Reveal in Finder",
                "Copy Path",
                "Copy Relative Path",
                "-",
                "Rename\u{2026}",
                "-",
                "Move to Trash"
            ]
        );
        assert!(found(&m, "Move to Trash").danger);
        assert_eq!(found(&m, "Move to Trash").keys, Some("\u{2318}\u{232b}"));
        assert_eq!(found(&m, "Copy Path").keys, Some("\u{2325}\u{2318}C"));
        assert_eq!(
            found(&m, "Copy Relative Path").keys,
            Some("\u{2325}\u{21e7}\u{2318}C")
        );
        assert_eq!(found(&m, "Open").keys, Some("\u{21a9}"));
        // Unmarked: no Show Changes. With a target (wave 3's pills): Add to Chat after Open to
        // the Side.
        let m = file_tree_menu(false, Some("Claude"), false);
        assert_eq!(
            labels(&m)[..4],
            ["Open", "Open to the Side", "Add to Claude Chat", "-"]
        );
    }

    #[test]
    fn a_files_folder_row_menu() {
        let m = file_tree_menu(true, None, true);
        assert_eq!(
            labels(&m),
            [
                "New File\u{2026}",
                "New Folder\u{2026}",
                "-",
                "Open in Terminal",
                "Reveal in Finder",
                "Copy Path",
                "Copy Relative Path",
                "-",
                "Rename\u{2026}",
                "-",
                "Move to Trash"
            ]
        );
        let m = file_tree_menu(true, Some("Claude"), false);
        assert_eq!(
            found(&m, "Add to Claude Chat").action,
            MenuAction::AddToChat
        );
    }

    #[test]
    fn arrows_skip_separators_and_disabled_items_and_stop_at_the_ends() {
        // Copy (disabled), Paste, -, Select All.
        let m = terminal_menu(false, None, None);
        assert_eq!(step(&m, None, true), Some(1));
        assert_eq!(step(&m, Some(1), true), Some(3));
        assert_eq!(step(&m, Some(3), true), Some(3));
        assert_eq!(step(&m, Some(3), false), Some(1));
        // Copy is disabled: ↑ from Paste stays.
        assert_eq!(step(&m, Some(1), false), Some(1));
        assert_eq!(step(&m, None, false), Some(3));
        assert_eq!(step(&[], None, true), None);
    }

    #[test]
    fn its_height_counts_items_separators_padding_and_edge() {
        let m = terminal_menu(false, None, None);
        assert_eq!(height(&m), 3.0 * ITEM + SEPARATOR + 2.0 * PAD + 2.0);
    }

    #[test]
    fn it_opens_at_the_pointer_and_flips_where_it_wont_fit() {
        let window = Rect {
            x: 0.0,
            y: 0.0,
            w: 1000.0,
            h: 800.0,
        };
        // Room below and right: at the pointer.
        assert_eq!(place((100.0, 100.0), 220.0, 200.0, window), (100.0, 100.0));
        // Near the bottom right: flipped up and left, its corner at the pointer.
        assert_eq!(place((900.0, 700.0), 220.0, 200.0, window), (680.0, 500.0));
        // No room either way down (taller than both halves): held 8 from the top.
        assert_eq!(place((100.0, 300.0), 220.0, 790.0, window).1, 8.0);
        // Flipped left wouldn't fit either: held 8 from the right edge.
        let narrow = Rect { w: 300.0, ..window };
        assert_eq!(place((150.0, 100.0), 220.0, 100.0, narrow).0, 72.0);
    }
}
