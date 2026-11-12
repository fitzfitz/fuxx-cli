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

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use alacritty_terminal::vte::ansi::Rgb;
use gpui::{
    Anchor, AnyElement, AnyView, App, ClickEvent, Context, Div, ElementId, FontWeight, Hsla,
    MouseButton, MouseDownEvent, SharedString, Stateful, UniformListScrollHandle, Window, anchored,
    deferred, div, prelude::*, px, relative, uniform_list,
};

use crate::appearance::Appearance;
use crate::branch_git::FetchMode;
use crate::branch_list::Head;
use crate::branch_sync::{self, Action, Sync, Tone};
use crate::git_branch::find_git;
use crate::git_stage::{self, FileWork};
use crate::git_status::{self, Change, ChangeList, Counts, Mark};
use crate::layout_model::{FileMode, TabKind};
use crate::right_sidebar::{
    EDGE, HEADER_PAD_RIGHT, NO_FOLDER, PILL_INSET, PILL_PAD, PILL_RADIUS, Panel, REFRESH, ROW,
    SMALL, TEXT, TITLE, back_off, due, find_root, mark_ink, note,
};
use crate::sidebar::tabular;
use crate::sidebar_toggle::{Tip, tip_box};
use crate::tab_bar::{Glyph, footer_glyph, glyph};
use crate::terminal_view::hsla;
use crate::theme::{Theme, UiColors, readable};
use crate::ui_tokens::TEXT_MIN;
use crate::workspace::Workspace;

pub(crate) const NO_CHANGES: &str = "No changes since the last commit.";
pub(crate) const NO_REPO: &str = "Not a git repository.";
pub(crate) const SSH_NOTE: &str = "Changes aren't shown for SSH terminals.";
const BINARY: &str = "binary";
/// A row (part 4: a pill inset `PILL_INSET`, its content from the 12 pt edge): the mark's
/// column, and the room after the mark and between the name, the folder and the counts.
const MARK_COLUMN: f32 = 12.0;
const ROW_GAP: f32 = 6.0;
/// Between the added and the removed count.
const COUNTS_GAP: f32 = 4.0;
/// The header (M11v part 2: the circle, the name and `N of M staged`, the progress bar; part
/// 4: the panels' 42 pt) and its line.
pub(crate) const HEADER: f32 = crate::right_sidebar::HEADER;
const HEADER_LINE: f32 = 20.0;
/// Part 2: the header's progress bar, the check circles, the dialog's widest.
const PROGRESS: f32 = 3.0;
const CIRCLE: f32 = 16.0;
const CONFIRM_WIDTH: f32 = 280.0;
/// A group header's room under its words (part 6: the words at the row's foot).
const DIVIDER_FOOT: f32 = 3.0;
/// The branch bar (M11v): its padding, its buttons' size and corner, the gap between them,
/// the tooltip's gap above its button and how long the pointer rests before it shows.
const BAR_PAD_X: f32 = 12.0;
const BAR_PAD_Y: f32 = 8.0;
const BAR_BUTTON: f32 = 26.0;
const BAR_BUTTON_RADIUS: f32 = 6.0;
/// The bar's icons: 15 pt (part 6, as the mockup), their lines 1.2 pt.
const BAR_ICON: f32 = 15.0;
const BAR_ICON_STROKE: f32 = 1.2;
const BAR_GAP: f32 = 6.0;
const TIP_GAP: f32 = 6.0;
const TIP_DELAY: Duration = Duration::from_millis(400);
/// The branch button's side padding (its hover fill); the line under it is set in as much.
const BRANCH_PAD: f32 = 4.0;

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
    /// The branch bar (M11v): where the root's branch stands, as last read; none: not read yet.
    sync: Option<Sync>,
    /// The git writes under way, by root: a bar action or a file action (part 2), one at a
    /// time per repo; one keeps running when the panel moves to another repo, and its repo is
    /// busy again on coming back.
    running: HashMap<PathBuf, Work>,
    /// The revert dialog (part 2, R1), while it asks.
    confirm: Option<Confirm>,
    /// Which of STAGED and CHANGES are open (part 6).
    groups: Groups,
    /// The last action's failure, as the bar says it; gone with the next action, another
    /// branch or another root.
    error: Option<String>,
    /// The bar button under the pointer, and whether its tooltip shows yet (after `TIP_DELAY`).
    hover: Option<(BarButton, bool)>,
    /// The root the automatic fetch last ran for.
    auto_fetched: Option<PathBuf>,
}

/// A git write under way on a repo (M11v): the bar's, or the rows' (part 2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Work {
    Bar(Action),
    Files(FileWork),
}

/// The revert dialog's words and what Revert does (part 2, R1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Confirm {
    pub title: String,
    pub body: String,
    pub button: &'static str,
    pub work: FileWork,
}

/// A row of the Changes list (part 2): a group's divider (`true`: STAGED), an entry (its index
/// in the list), or `… N more`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RowItem {
    Divider(bool),
    Entry(usize),
    More,
}

/// Which groups are open (part 6: STAGED and CHANGES are accordions; both open at first).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Groups {
    pub staged: bool,
    pub changes: bool,
}

impl Default for Groups {
    fn default() -> Self {
        Self {
            staged: true,
            changes: true,
        }
    }
}

impl Groups {
    fn open(self, staged: bool) -> bool {
        if staged { self.staged } else { self.changes }
    }
}

/// The rows with every group open (see `rows_of_in`).
#[cfg(test)]
pub(crate) fn rows_of(list: &ChangeList) -> Vec<RowItem> {
    rows_of_in(list, Groups::default())
}

/// The rows: STAGED's divider and entries, then CHANGES' (a group with none has no divider),
/// then `… N more` (the list is sorted staged first). A closed group keeps its divider (its
/// header) and drops its entries; `… N more` goes with CHANGES.
pub(crate) fn rows_of_in(list: &ChangeList, groups: Groups) -> Vec<RowItem> {
    let mut rows = Vec::new();
    for staged in [true, false] {
        let mut group = list
            .entries
            .iter()
            .enumerate()
            .filter(|(_, c)| c.staged == staged)
            .map(|(i, _)| RowItem::Entry(i))
            .peekable();
        if group.peek().is_some() {
            rows.push(RowItem::Divider(staged));
            if groups.open(staged) {
                rows.extend(group);
            }
        }
    }
    if list.more > 0 && groups.changes {
        rows.push(RowItem::More);
    }
    rows
}

/// The header circle: how it looks and what a click does, from one rule (review). Nothing
/// staged: empty, stages all; some: a dash, stages all; everything (no unstaged part left,
/// and nothing past the list's cap, which counts as unstaged): ✓, unstages all. None with
/// nothing changed.
pub(crate) fn header_state(list: &ChangeList) -> Option<(Circle, FileWork)> {
    if list.entries.is_empty() && list.more == 0 {
        return None;
    }
    let all = || vec![PathBuf::from(".")];
    let staged = list.entries.iter().any(|c| c.staged);
    let unstaged = list.more > 0 || list.entries.iter().any(|c| !c.staged);
    Some(match (staged, unstaged) {
        (true, false) => (Circle::On, FileWork::Unstage(all())),
        (true, true) => (Circle::Half, FileWork::Stage(all())),
        _ => (Circle::Off, FileWork::Stage(all())),
    })
}

/// Revert all: every unstaged file back, an untracked one deleted, with how many go back and
/// how many are deleted. Left alone: folder entries, and a `git add -N` file (Added but in
/// the index: `clean` would skip it and a restore would empty it; see `revertable`). None
/// with nothing to revert.
pub(crate) fn revert_all_work(list: &ChangeList) -> Option<(FileWork, usize, usize)> {
    let (mut tracked, mut untracked) = (Vec::new(), Vec::new());
    for c in list.entries.iter().filter(|c| revertable(c)) {
        if c.untracked {
            untracked.push(c.path.clone());
        } else {
            tracked.push(c.path.clone());
        }
    }
    if tracked.is_empty() && untracked.is_empty() {
        return None;
    }
    let (t, u) = (tracked.len(), untracked.len());
    Some((FileWork::Revert { tracked, untracked }, t, u))
}

/// Whether ↺ is offered for `c`: an unstaged part, not a folder entry, and not a `git add -N`
/// file (Added yet not untracked).
pub(crate) fn revertable(c: &Change) -> bool {
    !c.staged && !c.folder && (c.untracked || c.mark != Mark::Added)
}

/// The revert dialog for `path`'s unstaged part in `list` (by path, so a refresh between the
/// click and now can't swap the file): Delete for an untracked file. None when it isn't
/// listed (any more) or can't be reverted (`revertable`).
pub(crate) fn revert_confirm(list: &ChangeList, path: &Path) -> Option<Confirm> {
    let change = list
        .entries
        .iter()
        .find(|c| !c.staged && c.path == path)
        .filter(|c| revertable(c))?;
    let (name, _) = split_path(&change.path);
    let (title, body, button) = git_stage::revert_words(&name, change.counts, change.untracked);
    let paths = vec![change.path.clone()];
    let work = if change.untracked {
        FileWork::Revert {
            tracked: vec![],
            untracked: paths,
        }
    } else {
        FileWork::Revert {
            tracked: paths,
            untracked: vec![],
        }
    };
    Some(Confirm {
        title,
        body,
        button,
        work,
    })
}

/// `Stage failed: …` from git's refusal line (its own `error: ` / `fatal: ` dropped).
fn failed_line(verb: &str, err: &str) -> String {
    let err = err
        .strip_prefix("error: ")
        .or_else(|| err.strip_prefix("fatal: "))
        .unwrap_or(err);
    format!("{verb} failed: {err}")
}

/// A bar button: Fetch, or the one action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BarButton {
    Fetch,
    Action,
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
            sync: None,
            running: HashMap::new(),
            confirm: None,
            groups: Groups::default(),
            error: None,
            hover: None,
            auto_fetched: None,
        }
    }

    /// A read of where `root`'s branch stands (none: git failed; the last stays). False, and
    /// nothing done, for another root or the same answer. Another branch clears the error.
    fn apply_sync(&mut self, root: &Path, sync: Option<Sync>) -> bool {
        if self.root.as_deref() != Some(root) {
            return false;
        }
        let Some(sync) = sync else {
            return false;
        };
        if self.sync.as_ref() == Some(&sync) {
            return false;
        }
        if self.sync.as_ref().is_some_and(|old| old.head != sync.head) {
            self.error = None;
        }
        self.sync = Some(sync);
        true
    }

    /// `action` at `root` ended with `result`: the bar is free again and says why it failed,
    /// if it did. False, and nothing done, for another root.
    fn action_done(
        &mut self,
        root: &Path,
        action: Action,
        result: Result<(), String>,
        auto: bool,
    ) -> bool {
        self.running.remove(root);
        if self.root.as_deref() != Some(root) {
            return false;
        }
        // An automatic fetch that failed says nothing: nobody asked for it.
        self.error = result
            .err()
            .filter(|_| !auto)
            .map(|e| branch_sync::failed_text(action, &e));
        true
    }

    /// Open or close STAGED (`staged`) or CHANGES (part 6).
    fn toggle_group(&mut self, staged: bool) {
        if staged {
            self.groups.staged = !self.groups.staged;
        } else {
            self.groups.changes = !self.groups.changes;
        }
    }

    /// The git write under way on the root shown, if any.
    fn work(&self) -> Option<&Work> {
        self.root.as_ref().and_then(|r| self.running.get(r))
    }

    /// The bar's action under way on the root shown, if any.
    #[cfg(test)]
    fn busy(&self) -> Option<Action> {
        match self.work() {
            Some(Work::Bar(action)) => Some(*action),
            _ => None,
        }
    }

    /// A file action at `root` ended with `result`: the repo is free again, and the bar's line
    /// says why it failed (`verb failed: …`). False, and nothing shown, for another root.
    fn work_done(&mut self, root: &Path, result: Result<(), String>, verb: &str) -> bool {
        self.running.remove(root);
        if self.root.as_deref() != Some(root) {
            return false;
        }
        self.error = result.err().map(|e| failed_line(verb, &e));
        true
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

    /// The UI screenshots: the revert dialog asks to revert entry `i` (as ↺ would).
    #[cfg(feature = "ui-shots")]
    /// The UI screenshots: whether the dialog shows, and whether a git write runs or ran (its
    /// work, or the error a failed one left).
    #[cfg(feature = "ui-shots")]
    pub(crate) fn dialog_and_work_for_shots(&self) -> (bool, bool) {
        (
            self.confirm.is_some(),
            self.work().is_some() || self.error.is_some(),
        )
    }

    /// The UI screenshots: the revert dialog for entry `i`'s file (as its ↺ would ask).
    #[cfg(feature = "ui-shots")]
    pub(crate) fn ask_revert_for_shots(&mut self, i: usize) {
        self.confirm = self
            .list
            .as_ref()
            .and_then(|l| revert_confirm(l, &l.entries.get(i)?.path));
    }

    /// The UI screenshots: the bar shows `sync`, `busy` and `error`, with the action's tooltip
    /// when `tip`; no automatic fetch.
    #[cfg(feature = "ui-shots")]
    pub(crate) fn show_bar_for_shots(
        &mut self,
        sync: Sync,
        busy: Option<Action>,
        error: Option<String>,
        tip: bool,
    ) {
        self.sync = Some(sync);
        if let (Some(root), Some(action)) = (self.root.clone(), busy) {
            self.running.insert(root, Work::Bar(action));
        }
        self.error = error;
        self.hover = tip.then_some((BarButton::Action, true));
        self.auto_fetched = self.root.clone();
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
        self.sync = None;
        self.error = None;
        self.confirm = None;
        self.hover = None;
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
#[cfg(any(test, feature = "ui-shots"))]
pub fn totals(list: &ChangeList) -> (u64, u64) {
    list.entries
        .iter()
        .fold((0, 0), |(a, r), change| match change.counts {
            Counts::Lines { added, removed } => (a + u64::from(added), r + u64::from(removed)),
            _ => (a, r),
        })
}

/// `1 file`, `12 files`.
#[cfg(test)]
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

/// The branch bar's line inks (M11v): good (green), warn (yellow) and bad (red), the theme's
/// ANSI colours pushed (hue kept) to read as text on the bar's fill.
pub fn bar_inks(theme: &Theme) -> (Rgb, Rgb, Rgb) {
    let fill = [theme.ui().tab_bar];
    let ink = |i: usize| readable(theme.palette[i], &fill, TEXT_MIN);
    (ink(2), ink(3), ink(1))
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

/// A check circle (part 2): empty, a dash (some), or ✓ on green (all / staged).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Circle {
    Off,
    Half,
    On,
}

/// A `CIRCLE` pt circle in `state`: its edge faint (Off) or green, a green dash (Half), or
/// filled green with a ✓ in the panel's colour (On).
fn circle(state: Circle, green: Rgb, ui: &UiColors) -> Div {
    let d = div()
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .size(px(CIRCLE))
        .rounded_full()
        .border_1();
    match state {
        Circle::Off => d.border_color(hsla(ui.text_faint)),
        Circle::Half => d
            .border_color(hsla(green))
            .child(div().w(px(7.0)).h(px(1.5)).bg(hsla(green))),
        Circle::On => d
            .bg(hsla(green))
            .border_color(hsla(green))
            .text_size(px(10.0))
            .font_weight(FontWeight::BOLD)
            .text_color(hsla(ui.panel))
            .child("\u{2713}"),
    }
}

/// A group's header row (parts 2 and 6): a thin chevron (› closed, ⌄ open), `STAGED` /
/// `CHANGES` in small caps and a hairline; a click opens or closes the group. Its words sit at
/// the row's foot, which leaves room above it, between the groups (part 6: "more gap"; the
/// list's rows are one height). CHANGES' shows ↺ "Revert all…" on hover (`revert_all`).
fn divider_row(
    staged: bool,
    open: bool,
    revert_all: bool,
    ui: &UiColors,
    cx: &mut Context<Workspace>,
) -> AnyElement {
    let ui = *ui;
    div()
        .id(("change-divider", usize::from(staged)))
        .group("change-divider")
        .flex()
        .items_end()
        .gap(px(6.0))
        .h(px(ROW))
        .w_full()
        .pl(px(EDGE))
        .pr(px(EDGE))
        .pb(px(DIVIDER_FOOT))
        .cursor_pointer()
        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
            this.right_sidebar.changes.toggle_group(staged);
            cx.notify();
        }))
        .child(
            div()
                .flex_none()
                .h(px(16.0))
                .flex()
                .items_center()
                .child(crate::right_sidebar::thin_chevron(open, ui.text_faint)),
        )
        .child(
            div()
                .flex_none()
                .text_size(px(SMALL))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(hsla(ui.text_faint))
                .child(if staged { "STAGED" } else { "CHANGES" }),
        )
        .child(div().flex_1().h(px(1.0)).mb(px(7.0)).bg(hsla(ui.divider)))
        .when(revert_all, |d| {
            d.relative().child(
                div()
                    .absolute()
                    .right(px(EDGE - 4.0))
                    .bottom(px(DIVIDER_FOOT - 2.0))
                    .child(
                        revert_button(
                            "changes-revert-all",
                            "Revert all\u{2026}",
                            "change-divider",
                            &ui,
                        )
                        .group_hover("change-divider", move |s| s.visible().bg(hsla(ui.card)))
                        .on_click(cx.listener(
                            |this, _: &ClickEvent, _, cx| {
                                cx.stop_propagation();
                                this.ask_revert_all(cx)
                            },
                        )),
                    ),
            )
        })
        .into_any_element()
}

/// The ↺ button (part 2): shown while `group` (its row, or the divider) is hovered.
fn revert_button(
    id: impl Into<ElementId>,
    tip: &'static str,
    group: &'static str,
    ui: &UiColors,
) -> Stateful<Div> {
    let ui = *ui;
    div()
        .id(id)
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .size(px(20.0))
        .rounded(px(5.0))
        .cursor_pointer()
        // One group per element (GPUI keeps the last `group_hover`): its row's, or the
        // divider's.
        .invisible()
        .group_hover(group, |s| s.visible())
        .hover(move |s| s.bg(hsla(ui.card_hover)))
        .tooltip(move |_, cx| -> AnyView { cx.new(|_| Tip(tip.into())).into() })
        .on_mouse_down(MouseButton::Left, |_: &MouseDownEvent, _, cx| {
            cx.stop_propagation()
        })
        .child(glyph(Glyph::Revert, ui.text_dim))
}

/// `+A −R` (either left out at zero) at 11 pt, tabular, in the added and removed inks.
fn counts_element(added: Option<String>, removed: Option<String>, inks: &ChangeInks) -> AnyElement {
    div()
        .flex()
        .flex_none()
        .gap(px(COUNTS_GAP))
        .text_size(px(SMALL))
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
    enabled: bool,
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
                .text_size(px(SMALL))
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
    // Part 2: its circle stages or unstages it; under Changes, ↺ (on hover) asks to revert it.
    let staged = change.staged;
    let stage_work = if staged {
        FileWork::Unstage(vec![change.path.clone()])
    } else {
        FileWork::Stage(vec![change.path.clone()])
    };
    let green = inks.added;
    let can_act = enabled && !change.folder;
    let tick = circle(if staged { Circle::On } else { Circle::Off }, green, &ui)
        .id(("change-circle", i))
        .when(can_act, |c| {
            let tip: SharedString = if staged { "Unstage" } else { "Stage" }.into();
            c.cursor_pointer()
                .hover(move |s| s.border_color(hsla(green)))
                .tooltip(move |_, cx| -> AnyView { cx.new(|_| Tip(tip.clone())).into() })
                .on_mouse_down(MouseButton::Left, |_: &MouseDownEvent, _, cx| {
                    cx.stop_propagation()
                })
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    cx.stop_propagation();
                    this.run_file_work(stage_work.clone(), cx)
                }))
        })
        .when(!can_act, |c| c.opacity(0.4));
    let offers_revert = can_act && revertable(change);
    let counts = counts.map(|c| {
        div()
            .flex_none()
            .when(offers_revert, |d| {
                d.group_hover("change-row", |s| s.invisible())
            })
            .child(c)
    });
    let revert_path = change.path.clone();
    let revert = offers_revert.then(|| {
        div()
            .absolute()
            .right(px(PILL_PAD - 4.0))
            .top(px((ROW - 20.0) / 2.0))
            .child(
                revert_button(
                    ("change-revert", i),
                    "Revert changes\u{2026}",
                    "change-row",
                    &ui,
                )
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    cx.stop_propagation();
                    this.ask_revert(&revert_path, cx)
                })),
            )
    });
    let pill = div()
        .id(("change-row", i))
        .group("change-row")
        .relative()
        .flex()
        .items_center()
        .h_full()
        .w_full()
        .pl(px(PILL_PAD))
        .pr(px(PILL_PAD))
        .rounded(px(PILL_RADIUS))
        .gap(px(ROW_GAP))
        .text_size(px(TEXT))
        .when(opens, |r| r.cursor_pointer())
        .when(selected, |r| r.bg(hsla(ui.card_selected)))
        .when(!selected && opens, |r| r.hover(move |s| s.bg(hsla(hover))))
        .tooltip(move |_, cx| -> AnyView { cx.new(|_| Tip(full.clone())).into() })
        .child(tick)
        .child(
            div()
                .flex_none()
                .w(px(MARK_COLUMN))
                .text_size(px(SMALL))
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
                .text_size(px(SMALL))
                .text_color(hsla(ui.text_faint))
                .child(folder),
        )
        .children(counts)
        .children(revert)
        .when(opens, |r| {
            // A single click previews its diff, a double click opens it for good (M11t).
            r.on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                this.changes_click(&rel, event.click_count() < 2, window, cx)
            }))
        });
    // The pill in its row: inset from the card's sides (part 4).
    div()
        .h(px(ROW))
        .w_full()
        .px(px(PILL_INSET))
        .child(pill)
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
        .pl(px(EDGE + CIRCLE + MARK_COLUMN + 2.0 * ROW_GAP))
        .text_size(px(TEXT))
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
            // The list and where the branch stands (M11v), read together: local git only.
            let (list, sync) = cx
                .background_executor()
                .spawn(async move {
                    match find_git() {
                        Some(git) => (git_status::changes(&git, &at), branch_sync::read(&git, &at)),
                        None => (None, None),
                    }
                })
                .await;
            this.update(cx, |this, cx| {
                let changes = &mut this.right_sidebar.changes;
                // Drawn again only when the list shown or the bar changes (not every 2 s).
                let new = list
                    .as_ref()
                    .is_some_and(|l| changes.list.as_ref() != Some(l));
                let listed = changes.apply(&root, list) && new;
                let synced = changes.apply_sync(&root, sync);
                if listed || synced {
                    cx.notify();
                }
                this.auto_fetch_changes(cx);
            })
            .ok();
        })
        .detach();
    }

    /// Once per root shown (M11v): fetch origin if it was last fetched more than
    /// `AUTO_FETCH_AFTER` ago, or never, without asking for a login (`FetchMode::Auto`).
    fn auto_fetch_changes(&mut self, cx: &mut Context<Self>) {
        let changes = &mut self.right_sidebar.changes;
        let (Some(root), Some(sync)) = (changes.root.clone(), changes.sync.as_ref()) else {
            return;
        };
        if changes.work().is_some() || changes.auto_fetched.as_ref() == Some(&root) {
            return;
        }
        let due = sync.has_origin && branch_sync::auto_fetch_due(sync.fetched, SystemTime::now());
        changes.auto_fetched = Some(root);
        if due {
            self.run_bar_action(Action::Fetch, FetchMode::Auto, cx);
        }
    }

    /// Run `action` on the Changes root in the background (one at a time): the bar says what
    /// it's doing, then reads the branch again (and says why, if git refused).
    pub(crate) fn run_bar_action(
        &mut self,
        action: Action,
        mode: FetchMode,
        cx: &mut Context<Self>,
    ) {
        let changes = &mut self.right_sidebar.changes;
        if changes.work().is_some() || !changes.in_repo {
            return;
        }
        let Some(root) = changes.root.clone() else {
            return;
        };
        changes.running.insert(root.clone(), Work::Bar(action));
        changes.error = None;
        changes.hover = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let at = root.clone();
            let result = cx
                .background_executor()
                .spawn(async move {
                    match find_git() {
                        Some(git) => branch_sync::run(&git, &at, action, mode),
                        None => Err("git not found".to_string()),
                    }
                })
                .await;
            this.update(cx, |this, cx| {
                let auto = mode == FetchMode::Auto;
                if this
                    .right_sidebar
                    .changes
                    .action_done(&root, action, result, auto)
                {
                    this.right_sidebar.changes.refreshed = None;
                    this.refresh_changes(cx);
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    /// The pointer entered or left a bar button: its tooltip shows after `TIP_DELAY`.
    fn bar_hover(&mut self, which: BarButton, hovered: bool, cx: &mut Context<Self>) {
        let hover = &mut self.right_sidebar.changes.hover;
        if !hovered {
            if hover.is_some_and(|(b, _)| b == which) {
                *hover = None;
                cx.notify();
            }
            return;
        }
        *hover = Some((which, false));
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(TIP_DELAY).await;
            this.update(cx, |this, cx| {
                let hover = &mut this.right_sidebar.changes.hover;
                if *hover == Some((which, false)) {
                    *hover = Some((which, true));
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

    /// The header (part 2, layout C): the stage-everything circle (empty: nothing staged; a
    /// dash: some; ✓: all), the repo's name (13 pt semibold) and `N of M staged` at the right
    /// (N green, 11 pt), and under them a 3 pt bar, the staged share in green. With no changes,
    /// the name alone.
    fn render_changes_header(
        &self,
        root: &Path,
        list: Option<&ChangeList>,
        inks: &ChangeInks,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let ui = inks.ui;
        let green = inks.added;
        let name = root
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| root.display().to_string());
        let list = list.filter(|l| !l.entries.is_empty());
        let summary = list.map(git_stage::staged_summary);
        let enabled = self.right_sidebar.changes.work().is_none();
        let all = list.and_then(header_state).map(|(state, work)| {
            let work = Some(work);
            let tip: SharedString = if state == Circle::On {
                "Unstage all".into()
            } else {
                "Stage all".into()
            };
            circle(state, green, &ui)
                .id("changes-stage-all")
                .when(enabled, |c| {
                    c.cursor_pointer()
                        .hover(move |s| s.border_color(hsla(green)))
                        .tooltip(move |_, cx| -> AnyView { cx.new(|_| Tip(tip.clone())).into() })
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            if let Some(work) = work.clone() {
                                this.run_file_work(work, cx);
                            }
                        }))
                })
                .when(!enabled, |c| c.opacity(0.4))
        });
        let line = div()
            .flex()
            .items_center()
            .gap(px(ROW_GAP + 2.0))
            .h(px(HEADER_LINE))
            .w_full()
            .children(all)
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .truncate()
                    .text_size(px(TITLE))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(hsla(ui.text))
                    .child(name),
            )
            .children(summary.map(|(staged, total)| {
                div()
                    .flex_none()
                    .flex()
                    .text_size(px(SMALL))
                    .text_color(hsla(ui.text_faint))
                    .child(div().text_color(hsla(green)).child(staged.to_string()))
                    .child(format!("\u{a0}of {total} staged"))
            }));
        let progress = summary.map(|(staged, total)| {
            let share = if total == 0 {
                0.0
            } else {
                staged as f32 / total as f32
            };
            div()
                .mt(px(5.0))
                .h(px(PROGRESS))
                .w_full()
                .rounded(px(PROGRESS))
                .bg(hsla(ui.divider))
                .child(
                    div()
                        .h_full()
                        .w(relative(share))
                        .rounded(px(PROGRESS))
                        .bg(hsla(green)),
                )
        });
        div()
            .flex_none()
            .flex()
            .flex_col()
            .justify_center()
            .h(px(HEADER))
            .w_full()
            .pl(px(EDGE))
            .pr(px(HEADER_PAD_RIGHT))
            .child(line)
            .children(progress)
            .into_any_element()
    }

    /// Run `work` on the Changes root in the background (one git write per repo): the bar
    /// says what it's doing, then the list is read again (and the bar says why, if git refused).
    pub(crate) fn run_file_work(&mut self, work: FileWork, cx: &mut Context<Self>) {
        let changes = &mut self.right_sidebar.changes;
        if changes.work().is_some() || !changes.in_repo {
            return;
        }
        let Some(root) = changes.root.clone() else {
            return;
        };
        changes
            .running
            .insert(root.clone(), Work::Files(work.clone()));
        changes.error = None;
        changes.confirm = None;
        cx.notify();
        let verb = git_stage::verb(&work);
        cx.spawn(async move |this, cx| {
            let at = root.clone();
            let result = cx
                .background_executor()
                .spawn(async move {
                    match find_git() {
                        Some(git) => git_stage::run(&git, &at, &work),
                        None => Err("git not found".to_string()),
                    }
                })
                .await;
            this.update(cx, |this, cx| {
                if this.right_sidebar.changes.work_done(&root, result, verb) {
                    this.right_sidebar.changes.refreshed = None;
                    this.refresh_changes(cx);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Ask before reverting `path`'s unstaged part (R1): its words and what Revert does.
    fn ask_revert(&mut self, path: &Path, cx: &mut Context<Self>) {
        let changes = &mut self.right_sidebar.changes;
        let Some(confirm) = changes.list.as_ref().and_then(|l| revert_confirm(l, path)) else {
            return;
        };
        changes.confirm = Some(confirm);
        cx.notify();
    }

    /// Ask before reverting every unstaged file (R1).
    fn ask_revert_all(&mut self, cx: &mut Context<Self>) {
        let changes = &mut self.right_sidebar.changes;
        let Some((work, tracked, untracked)) = changes.list.as_ref().and_then(revert_all_work)
        else {
            return;
        };
        let (title, body, button) = git_stage::revert_all_words(tracked, untracked);
        changes.confirm = Some(Confirm {
            title,
            body,
            button,
            work,
        });
        cx.notify();
    }

    /// The revert dialog over the Changes tab (R1): what is lost, Cancel and the red button; a
    /// click outside it cancels.
    fn render_confirm(
        &self,
        confirm: &Confirm,
        inks: &ChangeInks,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let ui = inks.ui;
        let (_, _, bad) = bar_inks(&cx.global::<Appearance>().theme);
        let work = confirm.work.clone();
        let button = |id: &'static str, label: &'static str| {
            div()
                .id(id)
                .px(px(12.0))
                .py(px(5.0))
                .rounded(px(7.0))
                .text_size(px(SMALL))
                .cursor_pointer()
                .child(label)
        };
        let dialog = div()
            .id("changes-confirm-box")
            .w_full()
            .max_w(px(CONFIRM_WIDTH))
            .p(px(14.0))
            .rounded(px(12.0))
            .bg(hsla(ui.panel))
            .border_1()
            .border_color(hsla(ui.panel_border))
            .on_mouse_down(MouseButton::Left, |_: &MouseDownEvent, _, cx| {
                cx.stop_propagation()
            })
            .child(
                div()
                    .text_size(px(TEXT))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(hsla(ui.text))
                    .child(confirm.title.clone()),
            )
            .child(
                div()
                    .mt(px(6.0))
                    .text_size(px(SMALL))
                    .text_color(hsla(ui.text_dim))
                    .child(confirm.body.clone()),
            )
            .child(
                div()
                    .mt(px(14.0))
                    .flex()
                    .justify_end()
                    .gap(px(8.0))
                    .child(
                        button("changes-confirm-cancel", "Cancel")
                            .border_1()
                            .border_color(hsla(ui.panel_border))
                            .text_color(hsla(ui.text))
                            .hover(move |s| s.bg(hsla(ui.card_hover)))
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                this.right_sidebar.changes.confirm = None;
                                cx.notify();
                            })),
                    )
                    .child(
                        button("changes-confirm-go", confirm.button)
                            .bg(hsla(bad))
                            .text_color(hsla(ui.panel))
                            .font_weight(FontWeight::SEMIBOLD)
                            .hover(|s| s.opacity(0.85))
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                this.run_file_work(work.clone(), cx)
                            })),
                    ),
            );
        div()
            .id("changes-confirm")
            .absolute()
            .inset_0()
            // Nothing under the dim layer is hovered or clicked while the dialog asks (review).
            .occlude()
            .flex()
            .items_center()
            .justify_center()
            .p(px(12.0))
            .bg(Hsla {
                a: 0.45,
                ..hsla(ui.shadow)
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _: &MouseDownEvent, _, cx| {
                    this.right_sidebar.changes.confirm = None;
                    cx.notify();
                }),
            )
            .child(dialog)
            .into_any_element()
    }

    /// The branch bar (M11v, note 73) under the rows: the branch (a click opens the branch
    /// switcher for the followed terminal) and where it stands, then Fetch and the one action
    /// as icon buttons whose tooltips show above them.
    fn render_changes_bar(
        &self,
        sync: &Sync,
        inks: &ChangeInks,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let ui = inks.ui;
        let changes = &self.right_sidebar.changes;
        let working = changes.work().map(|w| match w {
            Work::Bar(action) => branch_sync::working_text(*action),
            Work::Files(work) => git_stage::working_text(work).to_string(),
        });
        let branch = match &sync.head {
            Head::Branch(name) | Head::Unborn(name) => name.clone(),
            Head::Detached(sha) => sha.clone(),
        };
        let (words, tone) = match (working, &changes.error) {
            (Some(text), _) => (text, Tone::Faint),
            (None, Some(error)) => (error.clone(), Tone::Bad),
            (None, None) => branch_sync::line(sync, SystemTime::now()),
        };
        let (good, warn, bad) = bar_inks(&cx.global::<Appearance>().theme);
        let ink = match tone {
            Tone::Faint => ui.text_faint,
            Tone::Good => good,
            Tone::Warn => warn,
            Tone::Bad => bad,
        };
        let followed = self.right_sidebar.followed;
        let name = div()
            .id("changes-branch")
            // As wide as the branch, not the column (your call: "don't make it 100% width").
            .self_start()
            .flex()
            .items_center()
            .gap(px(4.0))
            .min_w(px(0.0))
            .max_w_full()
            .px(px(BRANCH_PAD))
            .rounded(px(4.0))
            .cursor_pointer()
            .hover(move |s| s.bg(hsla(ui.card_hover)))
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                if let Some(terminal) = followed {
                    this.open_branch_switcher(terminal, window, cx);
                }
            }))
            .child(glyph(Glyph::Branch, ui.text_dim))
            .child(
                div()
                    .min_w(px(0.0))
                    .truncate()
                    .text_size(px(TEXT))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(hsla(ui.text))
                    .child(branch.clone()),
            );
        let left = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w(px(0.0))
            .child(name)
            .child(
                div()
                    .pl(px(BRANCH_PAD))
                    .min_w(px(0.0))
                    .truncate()
                    .text_size(px(SMALL))
                    .text_color(hsla(ink))
                    .child(words),
            );
        let action = branch_sync::action(sync);
        let fetch = branch_sync::shows_fetch(sync)
            .then(|| self.bar_button(BarButton::Fetch, Action::Fetch, &branch, &ui, cx));
        let primary = action.map(|a| self.bar_button(BarButton::Action, a, &branch, &ui, cx));
        div()
            .id("changes-bar")
            .flex_none()
            .flex()
            .items_center()
            .gap(px(BAR_GAP))
            .w_full()
            .px(px(BAR_PAD_X))
            .py(px(BAR_PAD_Y))
            .border_t_1()
            .border_color(hsla(ui.divider))
            .bg(hsla(ui.tab_bar))
            .child(left)
            .children(fetch)
            .children(primary)
            .into_any_element()
    }

    /// One bar button: `action`'s glyph, the accent fill for the one action, dimmed and dead
    /// while an action runs; its tooltip above it once the pointer has rested, lined up with
    /// its right edge (so the rightmost one never runs past the panel).
    fn bar_button(
        &self,
        which: BarButton,
        action: Action,
        branch: &str,
        ui: &UiColors,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let ui = *ui;
        let changes = &self.right_sidebar.changes;
        let enabled = changes.work().is_none();
        let primary = which == BarButton::Action;
        let kind = match action {
            Action::Fetch => Glyph::Reload,
            Action::Pull(_) => Glyph::Pull,
            Action::Push(_) => Glyph::Push,
            Action::Publish => Glyph::Publish,
        };
        // Just the icon (your call: "make it just icon there waiting to be clicked"): the
        // action's in the accent, Fetch's dim; a fill only under the pointer.
        let ink = if primary { ui.accent } else { ui.text_dim };
        let shown = enabled && changes.hover == Some((which, true));
        let tip_text: SharedString = branch_sync::tip(action, branch).into();
        let id = match which {
            BarButton::Fetch => "changes-bar-fetch",
            BarButton::Action => "changes-bar-action",
        };
        div()
            .id(id)
            .relative()
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .size(px(BAR_BUTTON))
            .rounded(px(BAR_BUTTON_RADIUS))
            .when(!enabled, |d| d.opacity(0.4))
            .when(enabled, |d| {
                d.cursor_pointer()
                    .hover(move |s| s.bg(hsla(ui.card_hover)))
                    .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                        this.bar_hover(which, *hovered, cx)
                    }))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.run_bar_action(action, FetchMode::Button, cx)
                    }))
            })
            .child(footer_glyph(kind, ink, BAR_ICON, BAR_ICON_STROKE))
            .when(shown, |d| {
                // A zero-size point at the button's top right (`TIP_GAP` above it, a little
                // right of it); the tooltip hangs from it by its bottom right corner.
                d.child(
                    div()
                        .absolute()
                        .top(px(-TIP_GAP))
                        .right(px(-4.0))
                        .child(deferred(
                            anchored()
                                .anchor(Anchor::BottomRight)
                                .snap_to_window_with_margin(px(4.0))
                                .child(tip_box(tip_text, &ui)),
                        )),
                )
            })
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
            root.map(|root| self.render_changes_header(root, changes.list.as_ref(), &inks, cx));
        let confirm = changes
            .confirm
            .clone()
            .filter(|_| root.is_some())
            .map(|c| self.render_confirm(&c, &inks, cx));
        let bar = root
            .and(changes.sync.clone())
            .map(|sync| self.render_changes_bar(&sync, &inks, cx));
        let body = match (shown, &changes.list) {
            (Some(text), _) => Some(note(&ui, text)),
            (None, Some(l)) if root.is_some() => {
                // Part 2: STAGED's divider and rows, then CHANGES', then `… N more`.
                let count = rows_of_in(l, changes.groups).len();
                let list = uniform_list(
                    "change-rows",
                    count,
                    cx.processor(|this, range: std::ops::Range<usize>, _window, cx| {
                        let inks = ChangeInks::of(&cx.global::<Appearance>().theme);
                        let selected = this.changes_selected();
                        let changes = &this.right_sidebar.changes;
                        let enabled = changes.work().is_none();
                        let Some(list) = changes.list.clone() else {
                            return Vec::new();
                        };
                        let rows = rows_of_in(&list, changes.groups);
                        let groups = changes.groups;
                        let revert_all = enabled && revert_all_work(&list).is_some();
                        range
                            .filter_map(|r| match *rows.get(r)? {
                                RowItem::Divider(staged) => Some(divider_row(
                                    staged,
                                    groups.open(staged),
                                    !staged && revert_all,
                                    &inks.ui,
                                    cx,
                                )),
                                RowItem::Entry(i) => {
                                    let change = list.entries.get(i)?;
                                    let chosen = selected.as_deref() == Some(&change.path);
                                    Some(change_row(i, change, chosen, enabled, &inks, cx))
                                }
                                RowItem::More => Some(more_row(list.more, &inks.ui)),
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
            .relative()
            .children(header)
            .child(div().flex_1().min_h(px(0.0)).w_full().children(body))
            .children(bar)
            .children(confirm)
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::branch_sync::Upstream;
    use crate::ui_tokens::contrast;

    fn change(path: &str, mark: Mark, counts: Counts) -> Change {
        Change {
            path: PathBuf::from(path),
            mark,
            counts,
            folder: false,
            staged: false,
            untracked: false,
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

    fn sync_on(branch: &str) -> Sync {
        Sync {
            head: Head::Branch(branch.into()),
            upstream: Upstream::Track {
                ahead: 0,
                behind: 2,
            },
            has_origin: true,
            fetched: None,
        }
    }

    #[test]
    fn a_sync_answer_for_another_root_is_dropped() {
        let mut panel = ChangesPanel::new();
        panel.rooted(PathBuf::from("/a"), true);
        assert!(!panel.apply_sync(Path::new("/b"), Some(sync_on("main"))));
        assert_eq!(panel.sync, None);
        assert!(panel.apply_sync(Path::new("/a"), Some(sync_on("main"))));
        assert!(
            !panel.apply_sync(Path::new("/a"), Some(sync_on("main"))),
            "the same answer changes nothing (no redraw)"
        );
        assert!(
            !panel.apply_sync(Path::new("/a"), None),
            "no answer keeps the last"
        );
        assert_eq!(panel.sync, Some(sync_on("main")));
    }

    #[test]
    fn an_action_answer_for_another_root_is_dropped() {
        let mut panel = ChangesPanel::new();
        panel.rooted(PathBuf::from("/a"), true);
        panel
            .running
            .insert(PathBuf::from("/a"), Work::Bar(Action::Pull(2)));
        assert!(!panel.action_done(Path::new("/b"), Action::Pull(2), Err("x".into()), false));
        assert_eq!(panel.busy(), Some(Action::Pull(2)));
        assert_eq!(panel.error, None);
    }

    #[test]
    fn a_failed_action_says_why_until_the_branch_changes() {
        let a = Path::new("/a");
        let mut panel = ChangesPanel::new();
        panel.rooted(a.to_path_buf(), true);
        panel.apply_sync(a, Some(sync_on("main")));
        panel
            .running
            .insert(a.to_path_buf(), Work::Bar(Action::Pull(2)));
        assert!(panel.action_done(a, Action::Pull(2), Err("Not possible".into()), false));
        assert_eq!(panel.busy(), None);
        assert_eq!(panel.error.as_deref(), Some("Pull failed: Not possible"));
        panel.apply_sync(a, Some(sync_on("main")));
        assert!(panel.error.is_some(), "kept while on the same branch");
        panel.apply_sync(a, Some(sync_on("feat")));
        assert_eq!(panel.error, None, "another branch: gone");
        panel.error = Some("Pull failed: x".into());
        panel
            .running
            .insert(a.to_path_buf(), Work::Bar(Action::Fetch));
        assert!(panel.action_done(a, Action::Fetch, Ok(()), false));
        assert_eq!(panel.error, None, "a later success clears it");
    }

    /// Review: a pull still running on A when you come back to A from B keeps A's bar busy
    /// (no second git command on A), and its answer frees it.
    #[test]
    fn returning_to_a_root_whose_action_still_runs_keeps_it_busy() {
        let a = Path::new("/a");
        let mut panel = ChangesPanel::new();
        panel.rooted(a.to_path_buf(), true);
        panel
            .running
            .insert(a.to_path_buf(), Work::Bar(Action::Pull(2)));
        panel.rooted(PathBuf::from("/b"), true);
        assert_eq!(panel.busy(), None, "B has nothing running");
        panel.rooted(a.to_path_buf(), true);
        assert_eq!(panel.busy(), Some(Action::Pull(2)), "A's pull still runs");
        assert!(panel.action_done(a, Action::Pull(2), Ok(()), false));
        assert_eq!(panel.busy(), None);
    }

    /// Review: an automatic fetch that fails (offline, a login it may not ask for) says
    /// nothing in red; you didn't ask for it.
    #[test]
    fn an_automatic_fetch_that_fails_says_nothing() {
        let a = Path::new("/a");
        let mut panel = ChangesPanel::new();
        panel.rooted(a.to_path_buf(), true);
        panel
            .running
            .insert(a.to_path_buf(), Work::Bar(Action::Fetch));
        assert!(panel.action_done(a, Action::Fetch, Err("offline".into()), true));
        assert_eq!(panel.error, None);
        assert_eq!(panel.busy(), None);
    }

    #[test]
    fn rerooting_forgets_the_bar() {
        let a = Path::new("/a");
        let mut panel = ChangesPanel::new();
        panel.rooted(a.to_path_buf(), true);
        panel.apply_sync(a, Some(sync_on("main")));
        panel
            .running
            .insert(a.to_path_buf(), Work::Bar(Action::Fetch));
        panel.error = Some("x".into());
        panel.rooted(PathBuf::from("/b"), true);
        assert_eq!(panel.sync, None);
        assert_eq!(panel.busy(), None);
        assert_eq!(panel.error, None);
    }

    fn staged_list() -> ChangeList {
        let entry = |path: &str, mark, staged| Change {
            path: PathBuf::from(path),
            mark,
            counts: Counts::Unknown,
            folder: false,
            staged,
            untracked: mark == Mark::Added && !staged,
        };
        list_of(
            vec![
                entry("a.txt", Mark::Modified, true),
                entry("a.txt", Mark::Modified, false),
                entry("new.txt", Mark::Added, false),
                entry("old.txt", Mark::Deleted, false),
            ],
            2,
        )
    }

    /// Part 2: the rows are STAGED's divider and its entries, then CHANGES' and its, then
    /// `… N more`; a group with nothing in it has no divider.
    #[test]
    fn the_rows_are_staged_then_changes_under_dividers() {
        assert_eq!(
            rows_of(&staged_list()),
            vec![
                RowItem::Divider(true),
                RowItem::Entry(0),
                RowItem::Divider(false),
                RowItem::Entry(1),
                RowItem::Entry(2),
                RowItem::Entry(3),
                RowItem::More,
            ]
        );
        let nothing_staged = list_of(staged_list().entries[1..].to_vec(), 0);
        assert_eq!(rows_of(&nothing_staged)[0], RowItem::Divider(false));
        assert_eq!(rows_of(&list_of(vec![], 0)), vec![]);
    }

    /// Part 6: STAGED and CHANGES are accordions: a closed one keeps its header, its entries
    /// go; `… N more` follows CHANGES, so it goes with it.
    #[test]
    fn a_closed_group_keeps_its_header_only() {
        let open = Groups::default();
        assert_eq!(rows_of_in(&staged_list(), open), rows_of(&staged_list()));
        let staged_closed = Groups {
            staged: false,
            ..open
        };
        assert_eq!(
            rows_of_in(&staged_list(), staged_closed),
            vec![
                RowItem::Divider(true),
                RowItem::Divider(false),
                RowItem::Entry(1),
                RowItem::Entry(2),
                RowItem::Entry(3),
                RowItem::More,
            ]
        );
        let changes_closed = Groups {
            changes: false,
            ..open
        };
        assert_eq!(
            rows_of_in(&staged_list(), changes_closed),
            vec![
                RowItem::Divider(true),
                RowItem::Entry(0),
                RowItem::Divider(false)
            ]
        );
        let mut panel = ChangesPanel::new();
        panel.toggle_group(true);
        assert!(!panel.groups.staged && panel.groups.changes);
        panel.toggle_group(true);
        assert!(panel.groups.staged);
    }

    /// Part 2: the header's circle stages everything, or unstages everything once all of it
    /// is staged; revert all reverts the changed files and deletes the new ones.
    #[test]
    fn the_header_circle_and_revert_all_do_what_they_say() {
        let dot = || vec![PathBuf::from(".")];
        let stage = Some((Circle::Half, FileWork::Stage(dot())));
        assert_eq!(header_state(&staged_list()), stage);
        let all_staged = list_of(staged_list().entries[..1].to_vec(), 0);
        assert_eq!(
            header_state(&all_staged),
            Some((Circle::On, FileWork::Unstage(dot())))
        );
        let none_staged = list_of(staged_list().entries[1..].to_vec(), 0);
        assert_eq!(
            header_state(&none_staged),
            Some((Circle::Off, FileWork::Stage(dot())))
        );
        assert_eq!(header_state(&list_of(vec![], 0)), None);
        // Review: one rule for the look and the click. A file staged and edited again still
        // has an unstaged part: a dash, and a click stages.
        let partial = list_of(staged_list().entries[..2].to_vec(), 0);
        assert_eq!(header_state(&partial), stage);
        // Past the cap the rest counts as unstaged: never "unstage all" when it isn't all.
        let capped = list_of(staged_list().entries[..1].to_vec(), 5);
        assert_eq!(header_state(&capped), stage);
        assert_eq!(
            revert_all_work(&staged_list()),
            Some((
                FileWork::Revert {
                    tracked: vec![PathBuf::from("a.txt"), PathBuf::from("old.txt")],
                    untracked: vec![PathBuf::from("new.txt")],
                },
                2,
                1
            ))
        );
        assert_eq!(revert_all_work(&all_staged), None, "nothing unstaged");
        // Review: a `git add -N` file (Added, in the index, not untracked) is neither deleted
        // (clean would skip it) nor restored (it would come back empty): left out.
        let mut ita = staged_list();
        ita.entries[2].untracked = false;
        assert_eq!(
            revert_all_work(&ita),
            Some((
                FileWork::Revert {
                    tracked: vec![PathBuf::from("a.txt"), PathBuf::from("old.txt")],
                    untracked: vec![],
                },
                2,
                0
            ))
        );
    }

    /// Part 2: one git write per repo: while files are staged the bar's buttons wait, and
    /// while the bar pulls the circles wait.
    #[test]
    fn file_work_and_bar_actions_never_overlap() {
        let a = Path::new("/a");
        let mut panel = ChangesPanel::new();
        panel.rooted(a.to_path_buf(), true);
        panel
            .running
            .insert(a.to_path_buf(), Work::Files(FileWork::Stage(vec![])));
        assert!(panel.work().is_some());
        assert_eq!(panel.busy(), None, "not a bar action");
        assert!(panel.work_done(a, Err("x".into()), "Stage"));
        assert_eq!(panel.error.as_deref(), Some("Stage failed: x"));
        assert!(panel.work().is_none());
    }

    /// Review: ↺ asks about the file by its path (never by its place in a list a refresh may
    /// have changed): the unstaged part of that file, Delete for an untracked one.
    #[test]
    fn the_revert_dialog_is_for_the_file_clicked() {
        let list = staged_list();
        let a = revert_confirm(&list, Path::new("a.txt")).expect("a.txt's unstaged part");
        assert_eq!(a.title, "Revert a.txt?");
        assert_eq!(
            a.work,
            FileWork::Revert {
                tracked: vec![PathBuf::from("a.txt")],
                untracked: vec![],
            }
        );
        let new = revert_confirm(&list, Path::new("new.txt")).expect("new.txt");
        assert_eq!(
            (new.title.as_str(), new.button),
            ("Delete new.txt?", "Delete")
        );
        assert_eq!(
            new.work,
            FileWork::Revert {
                tracked: vec![],
                untracked: vec![PathBuf::from("new.txt")],
            }
        );
        assert_eq!(
            revert_confirm(&list, Path::new("gone.txt")),
            None,
            "not listed any more"
        );
    }

    #[test]
    fn rerooting_closes_the_dialog() {
        let mut panel = ChangesPanel::new();
        panel.rooted(PathBuf::from("/a"), true);
        panel.confirm = Some(Confirm {
            title: "Revert a?".into(),
            body: "x".into(),
            button: "Revert",
            work: FileWork::Revert {
                tracked: vec![PathBuf::from("a")],
                untracked: vec![],
            },
        });
        panel.rooted(PathBuf::from("/b"), true);
        assert!(panel.confirm.is_none());
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

    /// The branch bar's line (M11v): green, yellow and red from the theme's ANSI colours, each
    /// readable on the bar's fill, in every theme; the warning is a yellow (red and green over
    /// blue), not the waiting colour (pink in Mocha).
    #[test]
    fn bar_inks_read_on_the_bar_and_warn_in_yellow() {
        let mut failures = vec![];
        for (name, theme) in crate::theme::every_theme() {
            let ui = theme.ui();
            let (good, warn, bad) = bar_inks(&theme);
            for (what, ink) in [("good", good), ("warn", warn), ("bad", bad)] {
                let ratio = contrast(ink, ui.tab_bar);
                if ratio < TEXT_MIN {
                    failures.push(format!("{name}: {what} on the bar is {ratio:.2}"));
                }
            }
            if !(warn.r > warn.b && warn.g > warn.b) {
                failures.push(format!("{name}: warn {warn:?} isn't a yellow"));
            }
        }
        assert!(failures.is_empty(), "\n{}", failures.join("\n"));
    }
}
