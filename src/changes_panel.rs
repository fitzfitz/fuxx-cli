//! The right sidebar's Changes panel (M11h): the files that differ from the last commit, in
//! the followed terminal's repo, with their line counts.
//!
//! Its root is found as the Files tree's (`right_sidebar::find_root`: the repo's top folder).
//! While it shows (and only then: `Workspace::right_panel_works`), `git_status::changes` runs on
//! the background executor every `REFRESH` (2 s), again at once when the panel is shown or the
//! terminal changes; a failed or slow git keeps the last list and backs off (`back_off`, up to
//! 30 s), and an answer for a root the panel has left is dropped (`ChangesPanel::apply`).
//!
//! Above the rows, a header in the Files root header's style: the repo's name and `N files`,
//! and under them the totals `+A −R`. Each row (22 pt): the mark letter in its Files colour,
//! the file's name, its folder (faint, cut at its end; the whole path in the tooltip) and its
//! counts at the right (`+12 −3`, `binary`). A click opens the file's tab in the focused pane
//! in Diff mode (`Workspace::open_file_tab`); the row of the focused diff tab's file is drawn
//! selected. It takes no keys and has no focus of its own. Empty states: no changes, not a
//! repo, an SSH terminal, no folder.
//!
//! `counts_label`, `totals`, `file_count_label`, `split_path` and `count_inks` are pure and
//! tested, as is `ChangesPanel::apply`.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use alacritty_terminal::vte::ansi::Rgb;
use gpui::{
    AnyElement, AnyView, App, ClickEvent, Context, FontWeight, SharedString,
    UniformListScrollHandle, Window, div, prelude::*, px, uniform_list,
};

use crate::appearance::Appearance;
use crate::git_branch::find_git;
use crate::git_status::{self, Change, ChangeList, Counts, Mark};
use crate::layout_model::{FileMode, TabKind};
use crate::right_sidebar::{
    NO_FOLDER, PATH_LINE, Panel, REFRESH, ROOT_HEADER, ROOT_LINE, ROOT_PAD_LEFT, ROOT_PAD_RIGHT,
    ROW, back_off, due, find_root, mark_ink, note,
};
use crate::sidebar::tabular;
use crate::sidebar_toggle::Tip;
use crate::terminal_view::hsla;
use crate::theme::{Theme, UiColors, readable};
use crate::ui_tokens::{TEXT_MIN, text};
use crate::workspace::Workspace;

pub(crate) const NO_CHANGES: &str = "No changes since the last commit.";
pub(crate) const NO_REPO: &str = "Not a git repository.";
pub(crate) const SSH_NOTE: &str = "Changes aren't shown for SSH terminals.";
const BINARY: &str = "binary";
/// A row: its room left of the mark, the mark's column, the room after the mark and between
/// the name, the folder and the counts, and its room right of the counts.
const ROW_LEFT: f32 = 8.0;
const MARK_COLUMN: f32 = 12.0;
const ROW_GAP: f32 = 6.0;
const ROW_RIGHT: f32 = 8.0;
/// Between the added and the removed count.
const COUNTS_GAP: f32 = 4.0;

/// The Changes panel: the list last shown and what it was made from, its scroll, and the
/// background work under way.
pub(crate) struct ChangesPanel {
    /// The terminal folder the root was last looked up for.
    folder: Option<PathBuf>,
    /// The repo's top folder (else the folder itself, `in_repo` false); none: no folder.
    root: Option<PathBuf>,
    in_repo: bool,
    /// The root's changes as last answered; none: not answered yet.
    list: Option<ChangeList>,
    scroll: UniformListScrollHandle,
    /// A root lookup, and a refresh, under way.
    rooting: bool,
    refreshing: bool,
    /// When the last refresh started; none: refresh now.
    pub(crate) refreshed: Option<Instant>,
    /// How long after it the next one starts: `REFRESH`, longer while git fails or is slow.
    wait: Duration,
}

impl ChangesPanel {
    pub(crate) fn new() -> Self {
        Self {
            folder: None,
            root: None,
            in_repo: false,
            list: None,
            scroll: UniformListScrollHandle::new(),
            rooting: false,
            refreshing: false,
            refreshed: None,
            wait: REFRESH,
        }
    }

    /// The UI screenshots: show `list` (none: not answered yet) for `root` (a repo's when
    /// `in_repo`); nothing is looked up there.
    #[cfg(feature = "ui-shots")]
    pub(crate) fn show_for_shots(
        &mut self,
        root: PathBuf,
        in_repo: bool,
        list: Option<ChangeList>,
    ) {
        self.folder = Some(root.clone());
        self.root = Some(root);
        self.in_repo = in_repo;
        self.list = list;
    }

    /// Re-root at `root` (a repo's when `in_repo`): the same root keeps its list; another
    /// starts empty, and work under way for the old one is dropped when it answers.
    fn rooted(&mut self, root: PathBuf, in_repo: bool) {
        self.in_repo = in_repo;
        if self.root.as_ref() == Some(&root) {
            return;
        }
        self.forget();
        self.in_repo = in_repo;
        self.root = Some(root);
    }

    /// No root (an SSH terminal's, or no folder): nothing shown, nothing under way holds the
    /// next root up.
    fn forget(&mut self) {
        self.root = None;
        self.in_repo = false;
        self.list = None;
        self.scroll = UniformListScrollHandle::new();
        self.refreshing = false;
        self.wait = REFRESH;
    }

    /// Whether `folder`'s root is looked up `now`: a folder other than the last one at once;
    /// the same one again only when its lookup had no answer (no root, nothing under way) and
    /// its wait is over.
    fn needs_root(&self, folder: &Path, now: Instant) -> bool {
        self.folder.as_deref() != Some(folder)
            || (self.root.is_none() && !self.rooting && due(self.refreshed, self.wait, now))
    }

    /// The root lookup had no answer at `now`: un-rooted (nothing shown; not "Not a git
    /// repository."), asked again after a longer wait (`back_off`).
    fn unanswered(&mut self, now: Instant) {
        let wait = back_off(self.wait, false);
        self.forget();
        self.rooting = false;
        self.refreshed = Some(now);
        self.wait = wait;
    }

    /// A refresh's answer for `root`: the list (none: git failed or was slow, so the list shown
    /// stays and the next refresh waits longer, `back_off`; its counts failed: shown, and the
    /// next refresh waits longer too). False, and nothing done, when the panel has moved on to
    /// another root.
    fn apply(&mut self, root: &Path, list: Option<ChangeList>) -> bool {
        if self.root.as_deref() != Some(root) {
            return false;
        }
        self.refreshing = false;
        let ok = list.as_ref().is_some_and(|l| !l.counts_failed);
        self.wait = back_off(self.wait, ok);
        if let Some(list) = list {
            self.list = Some(list);
        }
        true
    }
}

/// A row's counts as drawn: `+12` and `−3` (U+2212; a zero side left out), `binary`, or
/// nothing (no lines changed, or not counted).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CountsLabel {
    Lines {
        added: Option<String>,
        removed: Option<String>,
    },
    Binary,
    Nothing,
}

/// `+added` and `−removed`, each left out when zero.
fn sides(added: u64, removed: u64) -> (Option<String>, Option<String>) {
    (
        (added > 0).then(|| format!("+{added}")),
        (removed > 0).then(|| format!("\u{2212}{removed}")),
    )
}

/// How a change's counts read on its row.
pub fn counts_label(c: Counts) -> CountsLabel {
    match c {
        Counts::Lines {
            added: 0,
            removed: 0,
        }
        | Counts::Unknown => CountsLabel::Nothing,
        Counts::Lines { added, removed } => {
            let (added, removed) = sides(added.into(), removed.into());
            CountsLabel::Lines { added, removed }
        }
        Counts::Binary => CountsLabel::Binary,
    }
}

/// The lines added and removed over the rows listed (binaries and the uncounted add nothing).
pub fn totals(list: &ChangeList) -> (u64, u64) {
    list.entries
        .iter()
        .fold((0, 0), |(a, r), change| match change.counts {
            Counts::Lines { added, removed } => (a + u64::from(added), r + u64::from(removed)),
            _ => (a, r),
        })
}

/// `1 file`, `12 files`.
pub fn file_count_label(n: usize) -> String {
    if n == 1 {
        "1 file".to_string()
    } else {
        format!("{n} files")
    }
}

/// A changed file's name and its folder (from the repo's top; `""` for one at the top).
pub fn split_path(rel: &Path) -> (String, String) {
    let name = rel
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| rel.to_string_lossy().into_owned());
    let folder = rel
        .parent()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    (name, folder)
}

/// The counts' inks: the theme's ANSI green (added) and red (removed), pushed (hue kept) to
/// read as text (`TEXT_MIN`) on each fill a row has: the card, hovered, selected.
pub fn count_inks(theme: &Theme) -> (Rgb, Rgb) {
    let ui = theme.ui();
    let fills = [ui.card, ui.card_hover, ui.card_selected];
    (
        readable(theme.palette[2], &fills, TEXT_MIN),
        readable(theme.palette[1], &fills, TEXT_MIN),
    )
}

/// The colours the rows are drawn in, worked out once per frame (not per row).
struct ChangeInks {
    ui: UiColors,
    /// M, A, D.
    marks: [Rgb; 3],
    added: Rgb,
    removed: Rgb,
}

impl ChangeInks {
    fn of(theme: &Theme) -> Self {
        let ui = theme.ui();
        let (added, removed) = count_inks(theme);
        Self {
            marks: [Mark::Modified, Mark::Added, Mark::Deleted].map(|m| mark_ink(m, theme, &ui)),
            ui,
            added,
            removed,
        }
    }

    fn mark(&self, mark: Mark) -> Rgb {
        self.marks[match mark {
            Mark::Modified => 0,
            Mark::Added => 1,
            Mark::Deleted => 2,
        }]
    }
}

fn mark_letter(mark: Mark) -> &'static str {
    match mark {
        Mark::Modified => "M",
        Mark::Added => "A",
        Mark::Deleted => "D",
    }
}

/// `+A −R` (either left out at zero) at 11 pt, tabular, in the added and removed inks.
fn counts_element(added: Option<String>, removed: Option<String>, inks: &ChangeInks) -> AnyElement {
    div()
        .flex()
        .flex_none()
        .gap(px(COUNTS_GAP))
        .text_size(px(text::META))
        .font_features(tabular())
        .children(added.map(|a| div().text_color(hsla(inks.added)).child(a)))
        .children(removed.map(|r| div().text_color(hsla(inks.removed)).child(r)))
        .into_any_element()
}

/// Whether a click on `change`'s row opens its diff: not a folder (`?? vendor/x/`, a nested
/// repo, a submodule), which is listed but has no file to show.
pub fn opens_diff(change: &Change) -> bool {
    !change.folder
}

/// Row `i`: the mark letter in its colour, the name, the folder (faint, cut at its end), the
/// counts at the right; filled while `selected`, else `card_hover` on hover; the whole path in
/// its tooltip. A click opens the file's diff (`changes_click`). A folder entry's row
/// (`opens_diff`) has no hover fill and opens nothing.
fn change_row(
    i: usize,
    change: &Change,
    selected: bool,
    inks: &ChangeInks,
    cx: &mut Context<Workspace>,
) -> AnyElement {
    let ui = inks.ui;
    let (name, folder) = split_path(&change.path);
    let ink = inks.mark(change.mark);
    let counts = match counts_label(change.counts) {
        CountsLabel::Lines { added, removed } => Some(counts_element(added, removed, inks)),
        CountsLabel::Binary => Some(
            div()
                .flex_none()
                .text_size(px(text::META))
                .text_color(hsla(ui.text_faint))
                .child(BINARY)
                .into_any_element(),
        ),
        CountsLabel::Nothing => None,
    };
    let full: SharedString = change.path.display().to_string().into();
    let rel = change.path.clone();
    let hover = ui.card_hover;
    let opens = opens_diff(change);
    div()
        .id(("change-row", i))
        .flex()
        .items_center()
        .h(px(ROW))
        .w_full()
        .pl(px(ROW_LEFT))
        .pr(px(ROW_RIGHT))
        .gap(px(ROW_GAP))
        .text_size(px(text::NAME))
        .when(opens, |r| r.cursor_pointer())
        .when(selected, |r| r.bg(hsla(ui.card_selected)))
        .when(!selected && opens, |r| r.hover(move |s| s.bg(hsla(hover))))
        .tooltip(move |_, cx| -> AnyView { cx.new(|_| Tip(full.clone())).into() })
        .child(
            div()
                .flex_none()
                .w(px(MARK_COLUMN))
                .text_size(px(text::META))
                .text_color(hsla(ink))
                .child(mark_letter(change.mark)),
        )
        // The name keeps its room; the folder gets what's left (and is cut first).
        .child(
            div()
                .min_w(px(0.0))
                .truncate()
                .text_color(hsla(ui.text))
                .child(name),
        )
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .truncate()
                .text_size(px(text::META))
                .text_color(hsla(ui.text_faint))
                .child(folder),
        )
        .children(counts)
        .when(opens, |r| {
            // A single click previews its diff, a double click opens it for good (M11t).
            r.on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                this.changes_click(&rel, event.click_count() < 2, window, cx)
            }))
        })
        .into_any_element()
}

/// The `… N more` row: its words under the names.
fn more_row(more: usize, ui: &UiColors) -> AnyElement {
    div()
        .id("change-more")
        .flex()
        .items_center()
        .h(px(ROW))
        .w_full()
        .pl(px(ROW_LEFT + MARK_COLUMN + ROW_GAP))
        .text_size(px(text::NAME))
        .text_color(hsla(ui.text_faint))
        .child(format!("\u{2026} {more} more"))
        .into_any_element()
}

impl Workspace {
    /// Bring the Changes panel up to date (only while it shows: `refresh_right_sidebar`): a new
    /// terminal folder looks its root up (`find_root`, off the UI thread); otherwise, in a repo,
    /// every `wait` (at once when asked: shown, another terminal), `git_status::changes` runs
    /// on the background executor. An SSH terminal's changes aren't looked at; with no folder
    /// there is nothing to show.
    pub(crate) fn refresh_changes(&mut self, cx: &mut Context<Self>) {
        let folder = self.right_sidebar_folder(cx);
        let changes = &mut self.right_sidebar.changes;
        let Some((folder, false)) = folder else {
            if changes.root.is_some() || changes.folder.is_some() {
                changes.forget();
                changes.folder = None;
                changes.rooting = false;
                cx.notify();
            }
            return;
        };
        let now = Instant::now();
        if changes.needs_root(&folder, now) {
            changes.folder = Some(folder.clone());
            changes.rooting = true;
            cx.spawn(async move |this, cx| {
                let asked = folder.clone();
                let found = cx
                    .background_executor()
                    .spawn(async move { find_root(&asked) })
                    .await;
                this.update(cx, |this, cx| {
                    let changes = &mut this.right_sidebar.changes;
                    // The terminal has moved on since: its own lookup is under way.
                    if changes.folder.as_ref() != Some(&folder) {
                        return;
                    }
                    // No answer from git: un-rooted (not "Not a git repository."), asked
                    // again on a later tick.
                    let Some((root, in_repo, _)) = found else {
                        changes.unanswered(Instant::now());
                        cx.notify();
                        return;
                    };
                    changes.rooting = false;
                    changes.rooted(root, in_repo);
                    changes.refreshed = None;
                    // Not while another panel shows: it refreshes when shown again.
                    if this.right_panel_works(Panel::Changes, cx) {
                        this.refresh_changes(cx);
                    }
                    cx.notify();
                })
                .ok();
            })
            .detach();
            return;
        }
        let due = due(changes.refreshed, changes.wait, now);
        if changes.rooting || changes.refreshing || !due || !changes.in_repo {
            return;
        }
        let Some(root) = changes.root.clone() else {
            return;
        };
        changes.refreshing = true;
        changes.refreshed = Some(now);
        cx.spawn(async move |this, cx| {
            let at = root.clone();
            let list = cx
                .background_executor()
                .spawn(async move { find_git().and_then(|git| git_status::changes(&git, &at)) })
                .await;
            this.update(cx, |this, cx| {
                let changes = &mut this.right_sidebar.changes;
                // Drawn again only when the list shown changes (not every 2 s).
                let new = list
                    .as_ref()
                    .is_some_and(|l| changes.list.as_ref() != Some(l));
                if changes.apply(&root, list) && new {
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    /// A click on a Changes row: the file's tab in the focused pane, in Diff mode (that pane's
    /// tab for it selected if it has one); the tab takes the keyboard.
    fn changes_click(
        &mut self,
        rel: &Path,
        preview: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(root) = self.right_sidebar.changes.root.clone() else {
            return;
        };
        if preview {
            self.preview_file_tab(root.join(rel), FileMode::Diff, window, cx);
        } else {
            self.open_file_tab(root.join(rel), FileMode::Diff, window, cx);
        }
        cx.notify();
    }

    /// The file (from the Changes root) whose diff tab is the focused one, if it is.
    fn changes_selected(&self) -> Option<PathBuf> {
        let root = self.right_sidebar.changes.root.as_ref()?;
        let tab = self.model.tab(self.model.focused_tab()?)?;
        match &tab.kind {
            TabKind::File {
                path,
                mode: FileMode::Diff,
            } => path.strip_prefix(root).ok().map(Path::to_path_buf),
            _ => None,
        }
    }

    /// The header (the Files root header's style): the repo's name (13 pt semibold) and, at
    /// the right, `N files` (11 pt, faint); under them the totals `+A −R` (11 pt). With no
    /// changes (or none answered yet), the name alone.
    fn render_changes_header(
        &self,
        root: &Path,
        list: Option<&ChangeList>,
        inks: &ChangeInks,
    ) -> AnyElement {
        let ui = inks.ui;
        let name = root
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| root.display().to_string());
        let list = list.filter(|l| !l.entries.is_empty() || l.more > 0);
        let count = list.map(|l| file_count_label(l.entries.len() + l.more));
        let (added, removed) = list.map_or((None, None), |l| {
            let (a, r) = totals(l);
            sides(a, r)
        });
        let line = div()
            .flex()
            .items_center()
            .gap(px(ROW_GAP))
            .h(px(ROOT_LINE))
            .w_full()
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .truncate()
                    .text_size(px(text::NAME))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(hsla(ui.text))
                    .child(name),
            )
            .children(count.map(|c| {
                div()
                    .flex_none()
                    .text_size(px(text::META))
                    .text_color(hsla(ui.text_faint))
                    .child(c)
            }));
        let totals_line = div()
            .flex()
            .items_center()
            .h(px(PATH_LINE))
            .w_full()
            .child(counts_element(added, removed, inks));
        div()
            .flex_none()
            .flex()
            .flex_col()
            .justify_center()
            .h(px(ROOT_HEADER))
            .w_full()
            .pl(px(ROOT_PAD_LEFT))
            .pr(px(ROOT_PAD_RIGHT))
            .child(line)
            .child(totals_line)
            .into_any_element()
    }

    /// What the Changes panel shows in place of its rows, if anything: an SSH terminal's note,
    /// no folder, not a repo, no changes. None: its rows, or nothing yet (on their way).
    pub(crate) fn changes_note(&self, cx: &App) -> Option<&'static str> {
        let changes = &self.right_sidebar.changes;
        if self.followed_ssh() {
            return Some(SSH_NOTE);
        }
        if changes.root.is_none() {
            // Nothing to look at; else its root is on its way.
            return self.right_sidebar_folder(cx).is_none().then_some(NO_FOLDER);
        }
        if !changes.in_repo {
            return Some(NO_REPO);
        }
        changes
            .list
            .as_ref()
            .filter(|l| l.entries.is_empty() && l.more == 0)
            .map(|_| NO_CHANGES)
    }

    /// The Changes panel under the strip: the header (in a repo) and the rows (at most
    /// `CHANGES_CAP`, then `… N more`), or a note in their place (`changes_note`).
    /// (`_width`: as the Files panel's; nothing here is cut to a measured width.)
    pub(crate) fn render_changes_panel(
        &self,
        ui: UiColors,
        _width: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let changes = &self.right_sidebar.changes;
        let inks = ChangeInks::of(&cx.global::<Appearance>().theme);
        let shown = self.changes_note(cx);
        let root = changes
            .root
            .as_ref()
            .filter(|_| changes.in_repo && !self.followed_ssh());
        let header =
            root.map(|root| self.render_changes_header(root, changes.list.as_ref(), &inks));
        let body = match (shown, &changes.list) {
            (Some(text), _) => Some(note(&ui, text)),
            (None, Some(l)) if root.is_some() => {
                let count = l.entries.len() + usize::from(l.more > 0);
                let list = uniform_list(
                    "change-rows",
                    count,
                    cx.processor(|this, range: std::ops::Range<usize>, _window, cx| {
                        let inks = ChangeInks::of(&cx.global::<Appearance>().theme);
                        let selected = this.changes_selected();
                        let Some(list) = this.right_sidebar.changes.list.as_ref() else {
                            return Vec::new();
                        };
                        range
                            .filter_map(|i| match list.entries.get(i) {
                                Some(change) => {
                                    let chosen = selected.as_deref() == Some(&change.path);
                                    Some(change_row(i, change, chosen, &inks, cx))
                                }
                                None if i == list.entries.len() && list.more > 0 => {
                                    Some(more_row(list.more, &inks.ui))
                                }
                                None => None,
                            })
                            .collect::<Vec<_>>()
                    }),
                )
                .track_scroll(&changes.scroll)
                .size_full();
                Some(list.into_any_element())
            }
            _ => None,
        };
        div()
            .id("changes-panel")
            .flex()
            .flex_col()
            .flex_1()
            .min_h(px(0.0))
            .w_full()
            .children(header)
            .child(div().flex_1().min_h(px(0.0)).w_full().children(body))
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui_tokens::contrast;

    fn change(path: &str, mark: Mark, counts: Counts) -> Change {
        Change {
            path: PathBuf::from(path),
            mark,
            counts,
            folder: false,
        }
    }

    #[test]
    fn a_folder_row_opens_nothing() {
        let file = change("a.rs", Mark::Modified, Counts::Unknown);
        let folder = Change {
            folder: true,
            ..change("vendor/x/", Mark::Added, Counts::Unknown)
        };
        assert!(opens_diff(&file));
        assert!(!opens_diff(&folder), "no Diff tab on a folder");
    }

    fn list_of(entries: Vec<Change>, more: usize) -> ChangeList {
        ChangeList {
            entries,
            more,
            counts_failed: false,
        }
    }

    #[test]
    fn a_list_whose_counts_failed_is_shown_but_backs_off() {
        let root = PathBuf::from("/repo");
        let mut panel = ChangesPanel::new();
        panel.rooted(root.clone(), true);
        let failed = ChangeList {
            counts_failed: true,
            ..list_of(vec![change("a.rs", Mark::Modified, Counts::Unknown)], 0)
        };
        panel.refreshing = true;
        assert!(panel.apply(&root, Some(failed.clone())));
        // The marks are git's answer (shown); its counts aren't: waits longer, as a failed
        // status does (2 → 4 → 8 s).
        assert_eq!(panel.list.as_ref(), Some(&failed));
        assert_eq!(panel.wait, Duration::from_secs(4));
        assert!(panel.apply(&root, Some(failed.clone())));
        assert_eq!(panel.wait, Duration::from_secs(8));
        // Counted again: 2 s.
        assert!(panel.apply(&root, Some(list_of(Vec::new(), 0))));
        assert_eq!(panel.wait, REFRESH);
    }

    #[test]
    fn a_changes_answer_for_another_root_is_dropped_and_a_failure_keeps_the_list() {
        let root = PathBuf::from("/repo");
        let mut panel = ChangesPanel::new();
        panel.rooted(root.clone(), true);
        let first = list_of(
            vec![change(
                "a.rs",
                Mark::Modified,
                Counts::Lines {
                    added: 1,
                    removed: 0,
                },
            )],
            0,
        );
        panel.refreshing = true;
        assert!(panel.apply(&root, Some(first.clone())));
        assert_eq!(panel.list.as_ref(), Some(&first));
        assert!(!panel.refreshing);
        assert_eq!(panel.wait, REFRESH);
        // Another root's answer (the terminal moved on while git ran): dropped, nothing done.
        panel.refreshing = true;
        let other = list_of(vec![change("b.rs", Mark::Added, Counts::Unknown)], 0);
        assert!(!panel.apply(Path::new("/other"), Some(other)));
        assert_eq!(panel.list.as_ref(), Some(&first), "this root's list kept");
        assert!(panel.refreshing, "not that answer's refresh to end");
        // A failed or slow git: the list stays, the next refresh waits longer (2 → 4 s).
        assert!(panel.apply(&root, None));
        assert_eq!(panel.list.as_ref(), Some(&first), "the list kept");
        assert_eq!(panel.wait, Duration::from_secs(4), "backed off");
        assert!(!panel.refreshing);
        // A good answer again: shown, and the wait is 2 s again.
        let next = list_of(Vec::new(), 0);
        assert!(panel.apply(&root, Some(next.clone())));
        assert_eq!(panel.list.as_ref(), Some(&next));
        assert_eq!(panel.wait, REFRESH);
        // Moved to another root: that one starts empty, and the old root's answer is dropped.
        panel.rooted(PathBuf::from("/other"), true);
        assert_eq!(panel.list, None);
        assert!(!panel.apply(&root, Some(first.clone())));
        assert_eq!(panel.list, None);
        // The same root again keeps its list.
        assert!(panel.apply(Path::new("/other"), Some(first.clone())));
        panel.rooted(PathBuf::from("/other"), true);
        assert_eq!(panel.list.as_ref(), Some(&first));
    }

    /// A root lookup with no answer (git failed or was slow) doesn't say "Not a git
    /// repository.": the folder stays un-rooted and is asked again, backing off.
    #[test]
    fn changes_asks_for_an_unanswered_root_again_backing_off() {
        let folder = Path::new("/w/app/src");
        let s = Duration::from_secs;
        let now = Instant::now();
        let mut panel = ChangesPanel::new();
        assert!(panel.needs_root(folder, now), "a new folder at once");
        panel.folder = Some(folder.to_path_buf());
        panel.rooting = true;
        assert!(!panel.needs_root(folder, now), "under way");
        panel.unanswered(now);
        assert!(!panel.rooting);
        assert_eq!(panel.root, None, "un-rooted");
        assert!(!panel.in_repo);
        assert!(!panel.needs_root(folder, now + s(3)));
        assert!(
            panel.needs_root(folder, now + s(4)),
            "asked again after 4 s"
        );
        panel.rooting = true;
        panel.unanswered(now + s(4));
        assert!(!panel.needs_root(folder, now + s(11)));
        assert!(panel.needs_root(folder, now + s(12)), "then after 8 s");
        assert!(panel.needs_root(Path::new("/w/other"), now + s(2)));
        // Answered: rooted, not asked again for this folder.
        panel.rooted(PathBuf::from("/w/app"), true);
        assert!(!panel.needs_root(folder, now + s(60)));
    }

    #[test]
    fn counts_files_and_paths_read_as_the_rows_show_them() {
        let lines = |added, removed| counts_label(Counts::Lines { added, removed });
        assert_eq!(
            lines(12, 3),
            CountsLabel::Lines {
                added: Some("+12".into()),
                removed: Some("\u{2212}3".into())
            }
        );
        assert_eq!(
            lines(4, 0),
            CountsLabel::Lines {
                added: Some("+4".into()),
                removed: None
            }
        );
        assert_eq!(
            lines(0, 7),
            CountsLabel::Lines {
                added: None,
                removed: Some("\u{2212}7".into())
            }
        );
        assert_eq!(lines(0, 0), CountsLabel::Nothing);
        assert_eq!(counts_label(Counts::Binary), CountsLabel::Binary);
        assert_eq!(counts_label(Counts::Unknown), CountsLabel::Nothing);
        assert_eq!(file_count_label(1), "1 file");
        assert_eq!(file_count_label(12), "12 files");
        assert_eq!(file_count_label(0), "0 files");
        assert_eq!(
            split_path(Path::new("src/ui/a.rs")),
            ("a.rs".to_string(), "src/ui".to_string())
        );
        assert_eq!(
            split_path(Path::new("a.rs")),
            ("a.rs".to_string(), String::new())
        );
        // Only line counts add up: binaries and the uncounted add nothing.
        let list = list_of(
            vec![
                change(
                    "a.rs",
                    Mark::Modified,
                    Counts::Lines {
                        added: 12,
                        removed: 3,
                    },
                ),
                change("b.png", Mark::Added, Counts::Binary),
                change(
                    "c.rs",
                    Mark::Added,
                    Counts::Lines {
                        added: 4,
                        removed: 0,
                    },
                ),
                change("d.bin", Mark::Added, Counts::Unknown),
                change(
                    "e.rs",
                    Mark::Deleted,
                    Counts::Lines {
                        added: 0,
                        removed: u32::MAX,
                    },
                ),
            ],
            5,
        );
        assert_eq!(totals(&list), (16, 3 + u64::from(u32::MAX)));
        assert_eq!(totals(&ChangeList::default()), (0, 0));
    }

    /// The counts are text: both inks reach 4.5:1 on each fill a Changes row has (the card,
    /// hovered, selected), in every built-in theme, keeping the ANSI green and red's hue.
    #[test]
    fn count_inks_read_on_every_files_row_fill() {
        let mut failures = vec![];
        for (name, theme) in crate::theme::every_theme() {
            let ui = theme.ui();
            let (added, removed) = count_inks(&theme);
            for (what, ink) in [("added", added), ("removed", removed)] {
                for (fill_name, fill) in [
                    ("card", ui.card),
                    ("card_hover", ui.card_hover),
                    ("card_selected", ui.card_selected),
                ] {
                    let ratio = contrast(ink, fill);
                    if ratio < TEXT_MIN {
                        failures.push(format!("{name}: {what} on {fill_name} is {ratio:.2}"));
                    }
                }
            }
        }
        assert!(failures.is_empty(), "\n{}", failures.join("\n"));
    }
}
