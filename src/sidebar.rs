//! The sidebar (M11; M11b: a row per workspace): every workspace as a row that reads at a
//! glance, under its group's header (its first terminal's folder or host); pinned workspaces
//! lead, above every header, with a 📌.
//!
//! `rows` is the pure view-model: what each row shows (its name in bold, its most urgent
//! terminal's status and how long it has been in it, `branch • ~/folder` from `git_branch`, the
//! waiting question with a 🔔 "Needs input" pill, focused, pinned, its colour), built from facts
//! the Workspace hands over. The rest of the file draws it; the selected row is filled with the
//! accent, everything on it in `on_accent`. Rows behave like tabs: a × on hover in the mark's
//! place, a middle-click closes, a right-click opens the row's menu (`tab_menu`, with Pin /
//! Unpin); on the right, the colour's dot and the ⌘1–⌘9 shortcut.

use std::cell::Cell;
use std::f32::consts::TAU;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use alacritty_terminal::vte::ansi::Rgb;
use gpui::{
    Animation, AnimationExt, AnyElement, BoxShadow, ClickEvent, Context, DispatchPhase, Div,
    DragMoveEvent, FontFeatures, FontWeight, Hsla, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, PathBuilder, ScrollHandle, SharedString, Task, TextStyle, TruncateFrom, Window,
    canvas, div, point, prelude::*, px, relative,
};

use crate::appearance::Appearance;
use crate::groups::{self, GroupKey, Row};
use crate::layout_model::{Model, TabKind, WorkspaceId};
use crate::preferences;
use crate::presets::MAX_PRESETS;
use crate::reduce_motion::WindowVisible;
use crate::restore::SavedState;
use crate::status::{self, Status};
use crate::status_mark;
use crate::tab_color::TabColor;
use crate::tab_menu::TabMenu;
use crate::terminal_view::hsla;
use crate::theme::UiColors;
use crate::ui_tokens::size::{
    DIVIDER_ACTIVE, GROUP_HEADER, SIDEBAR_DEFAULT_WIDTH, SIDEBAR_MAX_WIDTH, SIDEBAR_MIN_WIDTH,
    SIDEBAR_ROW, STATUS_MARK, WAITING_ROW,
};
use crate::ui_tokens::text;
use crate::workspace::{POP_SHOWN, Renaming, Session, Workspace};

/// What the sidebar needs to know of one workspace, in display order: its name, colour and
/// group, and the rest from the terminal its ring is (its most urgent: `subject`).
pub struct WorkspaceFacts<'a> {
    pub workspace: WorkspaceId,
    pub label: &'a str,
    /// The session (its place in the session list) the row's mark and age are about; none for
    /// a workspace with no terminal (idle, no age).
    pub subject: Option<usize>,
    pub status: Status,
    /// When `status` last changed.
    pub since: Instant,
    /// While Waiting: the screen line that asked.
    pub question: Option<&'a str>,
    pub group: Option<GroupKey>,
    pub progress: Option<&'a (u8, Option<String>)>,
    pub note: Option<&'a str>,
    /// The background service's connection was lost (M10a).
    pub lost: bool,
    pub color: Option<TabColor>,
    /// Pinned to the top of the sidebar (out of its group).
    pub pinned: bool,
    /// The git branch checked out where its first terminal is (only inside a repo).
    pub branch: Option<&'a str>,
    /// Where its first terminal is, as shown (`home_relative`; an SSH one's host).
    pub folder: Option<String>,
}

/// One row of the sidebar.
#[derive(Clone, Debug, PartialEq)]
pub enum SidebarRow {
    Header(HeaderRow),
    Workspace(WorkspaceRow),
}

/// A group's header.
#[derive(Clone, Debug, PartialEq)]
pub struct HeaderRow {
    pub key: GroupKey,
    pub label: String,
    /// Its workspaces: `first..first + count` in the sidebar's order.
    pub first: usize,
    pub count: usize,
    pub collapsed: bool,
    /// The group's most urgent status, shown only while it's collapsed (open, its rows say it).
    pub mark: Option<Status>,
}

/// A workspace's row.
#[derive(Clone, Debug, PartialEq)]
pub struct WorkspaceRow {
    pub workspace: WorkspaceId,
    /// Its place in the sidebar (⌘1–⌘9 select the first nine by it).
    pub index: usize,
    /// The session its mark and age are about (none: a workspace with no terminal).
    pub subject: Option<usize>,
    pub label: String,
    pub status: Status,
    /// How long it has been in its status ("4m"); Idle shows none.
    pub age: Option<String>,
    /// The small line under the name, if any.
    pub sub: Option<Sub>,
    /// The focused workspace.
    pub active: bool,
    /// On screen, but not the focused one (never, since one workspace shows at a time).
    pub on_screen: bool,
    /// Under a group header.
    pub grouped: bool,
    /// The script-set progress bar's percent.
    pub progress: Option<u8>,
    /// Its tab colour, drawn as a dot.
    pub color: Option<TabColor>,
    /// Pinned: at the top, out of its group, with a 📌.
    pub pinned: bool,
    /// The second line, `branch • folder` (the branch only inside a git repo).
    pub branch: Option<String>,
    pub folder: Option<String>,
}

/// The line under a workspace's name.
#[derive(Clone, Debug, PartialEq)]
pub enum Sub {
    /// What a waiting session is asking.
    Question(String),
    /// Waiting, but no question was found on screen (the "Needs input" pill alone).
    NeedsInput,
    /// A script's note or progress label, or "not live".
    Note(String),
}

/// The sidebar's rows: leading ungrouped workspaces, then each group under its header (a closed
/// group's workspaces left out). `active` is the focused workspace; `shown`, the others on
/// screen.
pub fn rows(
    sessions: &[WorkspaceFacts],
    collapsed: &[GroupKey],
    active: Option<WorkspaceId>,
    shown: &[WorkspaceId],
    now: Instant,
) -> Vec<SidebarRow> {
    // Pinned ones are out of their groups: they lead, above every header.
    let keys: Vec<Option<&GroupKey>> = sessions
        .iter()
        .map(|s| s.group.as_ref().filter(|_| !s.pinned))
        .collect();
    let all: Vec<&GroupKey> = keys.iter().flatten().copied().collect();
    groups::rows(&keys, collapsed)
        .into_iter()
        .map(|row| match row {
            Row::Header {
                key,
                first,
                len,
                collapsed,
            } => {
                let statuses = sessions[first..first + len].iter().map(|s| s.status);
                SidebarRow::Header(HeaderRow {
                    label: groups::group_label(&key, &all),
                    key,
                    first,
                    count: len,
                    collapsed,
                    mark: collapsed.then(|| groups::rolled_up(statuses)),
                })
            }
            Row::Session { index, grouped } => {
                let s = &sessions[index];
                let is_active = active == Some(s.workspace);
                SidebarRow::Workspace(WorkspaceRow {
                    workspace: s.workspace,
                    index,
                    subject: s.subject,
                    label: s.label.to_string(),
                    status: s.status,
                    age: (s.status != Status::Idle)
                        .then(|| status::age_label(now.saturating_duration_since(s.since))),
                    sub: sub(s),
                    active: is_active,
                    on_screen: !is_active && shown.contains(&s.workspace),
                    grouped,
                    progress: s.progress.map(|(p, _)| *p),
                    color: s.color,
                    pinned: s.pinned,
                    branch: s.branch.map(String::from),
                    folder: s.folder.clone(),
                })
            }
        })
        .collect()
}

/// The line under a workspace's name, from its most urgent terminal: "not live" for a lost
/// one; a waiting one's question; else a script's progress label and note.
fn sub(s: &WorkspaceFacts) -> Option<Sub> {
    if s.lost {
        return Some(Sub::Note("not live".into()));
    }
    if s.status == Status::Waiting {
        return Some(match s.question {
            Some(q) => Sub::Question(q.to_string()),
            None => Sub::NeedsInput,
        });
    }
    sub_line(s.progress, s.note).map(Sub::Note)
}

/// A script's line under the name: `label · note`, or whichever of them there is.
fn sub_line(progress: Option<&(u8, Option<String>)>, note: Option<&str>) -> Option<String> {
    let label = progress.and_then(|(_, label)| label.as_deref());
    match (label, note) {
        (Some(l), Some(n)) => Some(format!("{l} · {n}")),
        (Some(t), None) | (None, Some(t)) => Some(t.to_string()),
        (None, None) => None,
    }
}

/// `path` as a row shows it: under your home folder (`home`) it starts with `~`.
pub fn home_relative(path: &Path, home: Option<&Path>) -> String {
    match home.and_then(|home| path.strip_prefix(home).ok()) {
        Some(rest) if rest.as_os_str().is_empty() => "~".into(),
        Some(rest) => format!("~/{}", rest.display()),
        None => path.display().to_string(),
    }
}

/// The place line, `branch • folder`, fitted to its room (`fits` says whether a line does).
/// As it is when it fits; else the branch loses its end first (down to `BRANCH_KEEP`
/// characters), then the folder its start, down to its own name; past that the line is cut at
/// its end as drawn.
pub fn fit_place(
    branch: Option<&str>,
    folder: Option<&str>,
    fits: impl Fn(&str) -> bool,
) -> String {
    let line = |b: Option<&str>, f: Option<&str>| {
        [b, f]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(PLACE_SEP)
    };
    let fits = |b: Option<&str>, f: Option<&str>| fits(&line(b, f));
    if fits(branch, folder) {
        return line(branch, folder);
    }
    // The longest `cut(n)` (n from `least` up to `most`) that fits, if any: shorter ones are
    // narrower, so a binary search finds it.
    let longest = |least: usize, most: usize, ok: &dyn Fn(usize) -> bool| {
        let (mut lo, mut hi) = (least, most);
        if !ok(lo) {
            return None;
        }
        while lo < hi {
            let mid = (lo + hi).div_ceil(2);
            if ok(mid) { lo = mid } else { hi = mid - 1 }
        }
        Some(lo)
    };
    // The branch: its first n characters and an ellipsis.
    let branch_len = branch.map_or(0, |b| b.chars().count());
    let cut_branch = |n: usize| {
        branch.map(|b| match b.chars().count() > n {
            true => format!("{}…", b.chars().take(n).collect::<String>()),
            false => b.to_string(),
        })
    };
    if branch_len > BRANCH_KEEP {
        let ok = |n: usize| fits(cut_branch(n).as_deref(), folder);
        if let Some(n) = longest(BRANCH_KEEP, branch_len - 1, &ok) {
            return line(cut_branch(n).as_deref(), folder);
        }
    }
    let branch = cut_branch(BRANCH_KEEP);
    // The folder: an ellipsis and its last n characters, never fewer than its own name's.
    let Some(folder) = folder else {
        return line(branch.as_deref(), None);
    };
    let folder_len = folder.chars().count();
    let name_len = folder
        .rsplit('/')
        .next()
        .map_or(0, |n| n.chars().count())
        .max(1);
    let cut_folder = |n: usize| {
        let tail: String = folder.chars().skip(folder_len - n).collect();
        format!("…{tail}")
    };
    if folder_len <= name_len {
        return line(branch.as_deref(), Some(folder));
    }
    let ok = |n: usize| fits(branch.as_deref(), Some(&cut_folder(n)));
    let n = longest(name_len, folder_len - 1, &ok).unwrap_or(name_len);
    line(branch.as_deref(), Some(&cut_folder(n)))
}

/// Whether a waiting row `row_width` wide has room for its question beside the pill (else the
/// pill shows alone: the question is in ⌘P and the notification).
pub fn shows_question(row_width: f32) -> bool {
    row_width >= QUESTION_MIN_ROW
}

/// Whether `row` gets a group's gap above it: the first unpinned row after the pinned ones
/// (a header has its own).
pub fn gap_before(previous: Option<&SidebarRow>, row: &SidebarRow) -> bool {
    matches!(
        (previous, row),
        (Some(SidebarRow::Workspace(p)), SidebarRow::Workspace(r)) if p.pinned && !r.pinned
    )
}

/// An SSH workspace's place: its host, and the folder there once its shell has said (no git
/// branch: the folder is on another machine).
pub fn remote_place(host: &str, folder: Option<&Path>) -> String {
    match folder {
        Some(folder) => format!("{host}:{}", folder.display()),
        None => host.to_string(),
    }
}

/// A row's height: the name line, and a line each for where it is and for what it's waiting
/// on (or a script's note).
pub fn row_height(row: &WorkspaceRow) -> f32 {
    let placed = row.branch.is_some() || row.folder.is_some();
    let lines = usize::from(placed) + usize::from(row.sub.is_some());
    SIDEBAR_ROW + lines as f32 * (WAITING_ROW - SIDEBAR_ROW)
}

/// A workspace with no terminal (only browser tabs) is called this until you name it.
pub const BROWSER_NAME: &str = "Browser";

/// Each workspace in display order with the session its row is about: its ring, the most
/// urgent terminal (`Model::rollup`; `index_of` gives a terminal's place in the session list);
/// none for a workspace with no terminal, which still gets its row.
pub fn row_subjects(
    model: &Model,
    index_of: impl Fn(u64) -> Option<usize>,
    status: impl Fn(u64) -> Status,
) -> Vec<(WorkspaceId, Option<usize>)> {
    model
        .display_order()
        .into_iter()
        .map(|ws| {
            let subject = model.rollup(ws, &status).and_then(|(id, _)| index_of(id));
            (ws, subject)
        })
        .collect()
}

/// The shortcut that selects the workspace in sidebar place `index` (0-based): "⌘1" to "⌘9" for the first nine, none
/// after.
pub fn shortcut_label(index: usize) -> Option<String> {
    (index < 9).then(|| format!("⌘{}", index + 1))
}

/// Whether a row `row_width` wide has room for the age right of the name (else it's left out).
pub fn shows_age(row_width: f32) -> bool {
    row_width >= AGE_MIN_ROW
}

/// A width the sidebar can take: within 180–400 px (the default for a missing or odd one).
pub fn clamp_width(width: Option<f32>) -> f32 {
    match width {
        Some(w) if w.is_finite() => w.clamp(SIDEBAR_MIN_WIDTH, SIDEBAR_MAX_WIDTH),
        _ => SIDEBAR_DEFAULT_WIDTH,
    }
}

/// Where a row dragged from `from` and dropped on `target`'s upper (or with `below`, lower)
/// half lands, as `Model::move_workspace_display` takes it.
pub fn drop_index(from: usize, target: usize, below: bool) -> usize {
    // The place before which it goes, then one less if taking it out shifted that place.
    let before = target + usize::from(below);
    if from < before { before - 1 } else { before }
}

/// How long until an age label (`status::age_label`) of `age` next changes.
pub fn next_age_change(age: Duration) -> Duration {
    let unit = match age.as_secs() {
        0..60 => 1,
        60..3600 => 60,
        3600..86400 => 3600,
        _ => 86400,
    };
    let unit = Duration::from_secs(unit);
    let into = Duration::from_nanos((age.as_nanos() % unit.as_nanos()) as u64);
    unit - into
}

/// Whether a list item laid out at `top..bottom` (before scrolling) shows in a viewport at
/// `view_top..view_bottom`, scrolled by `offset` (0 or less, as GPUI keeps it).
pub fn in_view(top: f32, bottom: f32, view_top: f32, view_bottom: f32, offset: f32) -> bool {
    bottom + offset > view_top && top + offset < view_bottom
}

/// Layout, in pixels (row and header heights, text sizes and widths are in `ui_tokens`).
/// Rows' margin from the sidebar's sides, and their own padding each side.
const ROW_INSET: f32 = 6.0;
const ROW_PAD: f32 = 8.0;
/// Between rows; above every group header but the first row, more.
const ROW_GAP: f32 = 2.0;
const GROUP_GAP: f32 = 10.0;
/// The slot a status mark (or a header's chevron) sits in; a waiting mark fills it.
pub(crate) const MARK_SLOT: f32 = 14.0;
/// The space between the mark and the name, and before the age.
const SLOT_GAP: f32 = 8.0;
/// A row's name line, and the small line under it.
const NAME_LINE: f32 = 18.0;
const SUB_LINE: f32 = 15.0;
/// A header's + (a new workspace in the group), shown on hover.
const ADD_BUTTON: f32 = 18.0;
/// The footer under the rows; the resize strip along the right edge.
const FOOTER: f32 = 36.0;
const RESIZE_STRIP: f32 = 4.0;
/// A row narrower than this leaves the age out, so the name keeps its room.
const AGE_MIN_ROW: f32 = 220.0;
/// The tab colour dot, and the space before it and before the shortcut.
const COLOR_DOT: f32 = 8.0;
const META_GAP: f32 = 6.0;
/// Between a row's branch and folder.
const PLACE_SEP: &str = " • ";
/// A long branch keeps this many characters before the folder gives way.
const BRANCH_KEEP: usize = 8;
/// A waiting row narrower than this leaves the question out (the pill alone).
const QUESTION_MIN_ROW: f32 = 200.0;
/// A pinned row's mark, right of its name.
const PIN: &str = "📌";
/// The "Needs input" pill: its height and words.
const PILL: f32 = 15.0;
const NEEDS_INPUT: &str = "🔔 Needs input";
/// A row's × (a little bigger than the mark slot it sits on), and the rows' hover group.
const CLOSE_BUTTON: f32 = 16.0;
const ROW_GROUP: &str = "sidebar-row";
/// The "+ New" menu's width.
const MENU_WIDTH: f32 = 230.0;
/// The "+ New" menu's first items (the presets follow): label and shortcut.
const NEW_MENU: [(&str, &str); 2] = [("New Workspace", "⌘N"), ("New Workspace in Folder…", "⌘O")];
/// How long a header's chevron takes to turn.
const CHEVRON_TURN: Duration = Duration::from_millis(120);
/// The first-run hint card.
const HINT_TITLE: &str = "Three things to know";
const HINT_LINES: [&str; 3] = [
    "⌘N workspace · ⌘T tab · ⇧⌘S SSH · ⇧⌘B browser",
    "Rings: spinning = working, pulsing = needs you, ✓ = done",
    "⇧⌘U jumps to what needs you",
];

/// The sidebar's own state: what's saved (width, hidden, the hint closed) and what isn't.
pub(crate) struct SidebarState {
    pub width: f32,
    pub hidden: bool,
    pub hint_dismissed: bool,
    /// The rows' scroll position (and their last laid-out bounds).
    pub scroll: ScrollHandle,
    /// Scroll the active row into view on the next frame: it just changed.
    pub reveal_active: bool,
    /// The right edge is being dragged.
    pub resizing: bool,
    /// The footer's "+ New" menu is open.
    pub new_menu: bool,
    /// A row's tab menu is open (right-click).
    pub tab_menu: Option<TabMenu>,
    /// While a row is dragged: the row the pointer is over, and whether it's in its lower
    /// half. Only where the drop would move it.
    pub drop_at: Option<(WorkspaceId, bool)>,
    /// The row being dragged, noted as its drag starts.
    pub drag_source: Rc<Cell<Option<WorkspaceId>>>,
    /// The group last opened or closed, and that toggle's number: its chevron turns.
    pub turned: Option<(GroupKey, u64)>,
    turns: u64,
    /// Redraws the rows when an age on them next changes, while the marks' ticker is off.
    age_refresh: Option<Task<()>>,
}

impl SidebarState {
    pub fn new() -> Self {
        Self {
            width: SIDEBAR_DEFAULT_WIDTH,
            hidden: false,
            hint_dismissed: false,
            scroll: ScrollHandle::new(),
            reveal_active: false,
            resizing: false,
            new_menu: false,
            tab_menu: None,
            drop_at: None,
            drag_source: Rc::new(Cell::new(None)),
            turned: None,
            turns: 0,
            age_refresh: None,
        }
    }

    /// Take the width, hidden and hint state from the sessions file.
    pub fn load(&mut self, saved: &SavedState) {
        self.width = clamp_width(saved.sidebar_width);
        self.hidden = saved.sidebar_hidden;
        self.hint_dismissed = saved.hint_dismissed;
    }

    /// Whether row `i` was in the scrolled view when last laid out (yes before any layout).
    fn row_in_view(&self, i: usize) -> bool {
        let Some(row) = self.scroll.bounds_for_item(i) else {
            return true;
        };
        let view = self.scroll.bounds();
        in_view(
            f32::from(row.top()),
            f32::from(row.bottom()),
            f32::from(view.top()),
            f32::from(view.bottom()),
            f32::from(self.scroll.offset().y),
        )
    }
}

/// What a sidebar row carries while it's dragged: the workspace and the run of rows it moves
/// within (`Workspace::sidebar_run`).
pub(crate) struct DraggedWorkspace {
    workspace: WorkspaceId,
    run: (bool, Option<GroupKey>),
}

/// The row that follows the pointer during a drag: the row itself, lifted.
struct DragPreview {
    label: SharedString,
    /// Its place line, if it has one (the preview is as tall as the row's two lines).
    place: Option<SharedString>,
    status: Status,
    width: f32,
}

impl Render for DragPreview {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ui = cx.global::<Appearance>().theme.ui();
        let name_line = div()
            .flex()
            .items_center()
            .h(px(NAME_LINE))
            .child(mark_slot(status_mark::mark_on(
                self.status,
                &ui,
                0.0,
                None,
                px(mark_size(self.status)),
                ui.row_active,
            )))
            .child(
                div()
                    .flex_1()
                    .ml(px(SLOT_GAP))
                    .min_w(px(0.0))
                    .truncate()
                    .font_weight(FontWeight::BOLD)
                    .child(self.label.clone()),
            );
        let place = self.place.clone().map(|place| {
            div()
                .h(px(SUB_LINE))
                .pl(px(MARK_SLOT + SLOT_GAP))
                .text_size(px(text::META))
                .line_height(px(SUB_LINE))
                .text_color(hsla(ui.text_faint))
                .truncate()
                .child(place)
        });
        let height = SIDEBAR_ROW
            + if place.is_some() {
                WAITING_ROW - SIDEBAR_ROW
            } else {
                0.0
            };
        div()
            .flex()
            .flex_col()
            .justify_center()
            .w(px(self.width - 2.0 * ROW_INSET))
            .h(px(height))
            .px(px(ROW_PAD))
            .rounded(px(6.0))
            .opacity(0.95)
            .shadow_md()
            .bg(hsla(ui.row_active))
            .border_1()
            .border_color(hsla(ui.panel_border))
            .text_size(px(text::NAME))
            .text_color(hsla(ui.text))
            .child(name_line)
            .children(place)
    }
}

/// Tabular figures, so numbers and times line up.
pub(crate) fn tabular() -> FontFeatures {
    FontFeatures(Arc::new(vec![("tnum".into(), 1)]))
}

/// Every letter as a small capital (the system font has them): headers read as headers.
pub(crate) fn small_caps() -> FontFeatures {
    FontFeatures(Arc::new(vec![("smcp".into(), 1), ("c2sc".into(), 1)]))
}

/// A waiting mark is drawn bigger than the others.
pub(crate) fn mark_size(status: Status) -> f32 {
    if status == Status::Waiting {
        MARK_SLOT
    } else {
        STATUS_MARK
    }
}

pub(crate) fn mark_slot(mark: impl IntoElement) -> Div {
    div()
        .flex_none()
        .size(px(MARK_SLOT))
        .flex()
        .items_center()
        .justify_center()
        .child(mark)
}

/// The colours a row's mark and × are drawn in: on the selected row (`active`, filled with the
/// accent) every mark and the × are `on_accent` (the mark's shape still says its status), and
/// the ×'s hover keeps the fill; elsewhere the theme's own.
fn on_fill(ui: UiColors, active: bool) -> UiColors {
    if !active {
        return ui;
    }
    let ink = ui.on_accent;
    UiColors {
        ring_idle: ink,
        ring_working: ink,
        ring_waiting: ink,
        ring_done: ink,
        text: ink,
        text_dim: ink,
        panel_hover: ui.accent,
        ..ui
    }
}

/// A waiting row's 🔔 "Needs input" pill: outlined in the waiting colour (in `on_accent` on the
/// selected row), its words in the row's text colour.
fn needs_input_pill(ui: UiColors, active: bool) -> Div {
    let (edge, ink) = if active {
        (ui.on_accent, ui.on_accent)
    } else {
        (ui.ring_waiting, ui.text)
    };
    div()
        .flex_none()
        .flex()
        .items_center()
        .h(px(PILL))
        .px(px(5.0))
        .rounded_full()
        .border_1()
        .border_color(hsla(edge))
        .text_color(hsla(ink))
        .font_weight(FontWeight::MEDIUM)
        .line_height(px(PILL - 2.0))
        .child(NEEDS_INPUT)
}

/// A tab colour's dot.
pub(crate) fn color_dot(color: TabColor, ui: UiColors) -> Div {
    div()
        .flex_none()
        .size(px(COLOR_DOT))
        .rounded_full()
        .bg(hsla(ui.tab(color)))
}

/// A header's chevron, `angle` radians clockwise from pointing right (a quarter turn: down).
fn chevron(angle: f32, color: Rgb) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            let c = bounds.center();
            let (sin, cos) = angle.sin_cos();
            let p =
                |x: f32, y: f32| point(c.x + px(x * cos - y * sin), c.y + px(x * sin + y * cos));
            let mut path = PathBuilder::fill();
            path.move_to(p(-1.6, -3.2));
            path.line_to(p(2.4, 0.0));
            path.line_to(p(-1.6, 3.2));
            path.close();
            if let Ok(path) = path.build() {
                window.paint_path(path, hsla(color));
            }
        },
    )
    .flex_none()
    .size(px(MARK_SLOT))
}

impl Workspace {
    /// The sidebar's rows as they are now: one per workspace, in display order.
    fn sidebar_rows(&self, now: Instant) -> Vec<SidebarRow> {
        let items = self.sessions.items();
        let status = |id: u64| {
            items
                .iter()
                .find(|s| s.id == id)
                .map_or(Status::Idle, |s| s.status)
        };
        let subjects = row_subjects(&self.model, |id| self.index_of(id), status);
        let labels: Vec<SharedString> = subjects
            .iter()
            .map(|(ws, _)| self.workspace_label(*ws))
            .collect();
        // Where each is: its first terminal's folder (as last looked up, else the one it started
        // in) and the branch there; an SSH one's host.
        let places: Vec<(Option<&str>, Option<String>)> = subjects
            .iter()
            .map(|&(ws, _)| {
                let first = self.model.terminals(ws).first().copied();
                let Some(s) = first.and_then(|id| items.iter().find(|s| s.id == id)) else {
                    return (None, None);
                };
                let found = self.places.folder(s.id);
                if let Some(ssh) = &s.ssh {
                    return (None, Some(remote_place(&ssh.host, found)));
                }
                let Some(folder) = found.or(s.project.as_deref()) else {
                    return (None, None);
                };
                let shown = home_relative(folder, self.places.home.as_deref());
                (self.places.branch(folder), Some(shown))
            })
            .collect();
        let facts: Vec<WorkspaceFacts> = subjects
            .iter()
            .zip(&labels)
            .zip(places)
            .map(|((&(ws, subject), label), (branch, folder))| {
                let s = subject.map(|i| &items[i]);
                WorkspaceFacts {
                    workspace: ws,
                    label,
                    subject,
                    status: s.map_or(Status::Idle, |s| s.status),
                    since: s.map_or(now, |s| s.status_since),
                    question: s.and_then(|s| s.question.as_deref()),
                    group: self.workspace_group(ws),
                    progress: s.and_then(|s| s.progress.as_ref()),
                    note: s.and_then(|s| s.note.as_deref()),
                    lost: s.is_some_and(|s| s.lost),
                    color: self.model.workspace(ws).and_then(|w| w.color),
                    pinned: self.model.workspace(ws).is_some_and(|w| w.pinned),
                    branch,
                    folder,
                }
            })
            .collect();
        rows(
            &facts,
            &self.collapsed,
            self.model.focused_workspace(),
            &[],
            now,
        )
    }

    /// The workspaces (by place in the sidebar) that have a row: all but those folded in a
    /// closed group (scrolled out of view or not).
    pub(crate) fn workspaces_with_rows(&self) -> Vec<usize> {
        self.sidebar_rows(Instant::now())
            .into_iter()
            .filter_map(|row| match row {
                SidebarRow::Workspace(s) => Some(s.index),
                SidebarRow::Header(_) => None,
            })
            .collect()
    }

    /// The marks on screen: each row in the scrolled view (a closed group's header shows its
    /// sessions' most urgent; none with the sidebar hidden), and each terminal tab in the panes'
    /// tab bars.
    pub(crate) fn visible_statuses(&self) -> Vec<Status> {
        let tabs = self
            .tab_sessions()
            .into_iter()
            .map(|i| self.sessions.items()[i].status);
        self.rows_in_view(Instant::now())
            .into_iter()
            .filter_map(|row| match row {
                SidebarRow::Header(h) => h.mark,
                SidebarRow::Workspace(s) => Some(s.status),
            })
            .chain(tabs)
            .collect()
    }

    /// The sessions whose tabs are on screen: every terminal tab of each pane of the focused
    /// workspace, in layout then bar order (sessions by place in the list).
    pub(crate) fn tab_sessions(&self) -> Vec<usize> {
        let Some(tree) = self.shown_layout() else {
            return Vec::new();
        };
        tree.leaves()
            .into_iter()
            .filter_map(|pane| self.model.pane(pane))
            .flat_map(|p| &p.tabs)
            .filter_map(|t| match t.kind {
                TabKind::Terminal(id) => self.index_of(id),
                TabKind::Browser { .. } => None,
            })
            .collect()
    }

    /// Whether session `index`'s workspace's row is on screen: the sidebar is shown, its
    /// group open, and the row in the scrolled view.
    pub(crate) fn row_on_screen(&self, index: usize) -> bool {
        let Some(ws) = self.workspace_of(index) else {
            return false;
        };
        self.rows_in_view(Instant::now())
            .iter()
            .any(|row| matches!(row, SidebarRow::Workspace(s) if s.workspace == ws))
    }

    /// The sidebar's rows in the scrolled view; none with the sidebar hidden.
    pub(crate) fn rows_in_view(&self, now: Instant) -> Vec<SidebarRow> {
        if self.sidebar.hidden {
            return Vec::new();
        }
        self.sidebar_rows(now)
            .into_iter()
            .enumerate()
            .filter(|(i, _)| self.sidebar.row_in_view(*i))
            .map(|(_, row)| row)
            .collect()
    }

    /// While the marks' ticker isn't redrawing (nothing moves, or motion is off), redraw once
    /// when an age on screen next changes ("59s" to "1m"), so Done's time stays true. Nothing
    /// while the window can't be seen: showing it again redraws.
    pub(crate) fn refresh_ages(&mut self, ticking: bool, cx: &mut Context<Self>) {
        let seen = cx.try_global::<WindowVisible>().is_none_or(|w| w.visible);
        let now = Instant::now();
        let next = if ticking || !seen {
            None
        } else {
            let rows = self
                .rows_in_view(now)
                .into_iter()
                .filter_map(|row| match row {
                    SidebarRow::Workspace(s) if s.age.is_some() => s.subject,
                    _ => None,
                });
            // (A tab shows its age only in its tooltip, read when it opens.)
            let sinces = rows.map(|i| self.sessions.items()[i].status_since);
            // And the switcher's rows, while it's open.
            let switcher = self
                .switcher_shown(cx)
                .into_iter()
                .filter(|(status, _)| *status != Status::Idle)
                .map(|(_, since)| since);
            sinces
                .chain(switcher)
                .map(|since| next_age_change(now.saturating_duration_since(since)))
                .min()
        };
        self.sidebar.age_refresh = next.map(|wait| {
            cx.spawn(async move |this, cx| {
                // Just past the change, so the label has turned when it's drawn.
                let wait = wait + Duration::from_millis(20);
                cx.background_executor().timer(wait).await;
                this.update(cx, |_, cx| cx.notify()).ok();
            })
        });
    }

    /// The sidebar: the top band (under the traffic lights), the rows (scrolling), the
    /// background service's notice, the first-run hint, and the footer; its right edge resizes it.
    pub(crate) fn render_sidebar(
        &mut self,
        ui: UiColors,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let rows = self.sidebar_rows(Instant::now());
        if self.sidebar.reveal_active && !rows.is_empty() {
            if self.sidebar.scroll.children_count() == 0 {
                // Not laid out yet, so a scroll asked for now would be lost: once it is.
                let this = cx.entity().downgrade();
                window.on_next_frame(move |_, cx| {
                    this.update(cx, |_, cx| cx.notify()).ok();
                });
            } else {
                self.sidebar.reveal_active = false;
                let active = rows
                    .iter()
                    .position(|r| matches!(r, SidebarRow::Workspace(s) if s.active));
                if let Some(i) = active {
                    self.sidebar.scroll.scroll_to_item(i);
                }
            }
        }
        let elements: Vec<AnyElement> = rows
            .iter()
            .enumerate()
            .map(|(i, row)| match row {
                SidebarRow::Header(h) => self.render_header(h, i == 0, ui, cx),
                SidebarRow::Workspace(s) => {
                    let gap = gap_before(i.checked_sub(1).map(|p| &rows[p]), row);
                    self.render_workspace_row(s, gap, ui, cx)
                }
            })
            .collect();
        let band = self.render_top_band("sidebar-band", None, window, cx);
        let list = div()
            .id("sidebar-rows")
            .flex()
            .flex_col()
            .flex_1()
            .min_h(px(0.0))
            .gap(px(ROW_GAP))
            .pb(px(8.0))
            .overflow_y_scroll()
            .track_scroll(&self.sidebar.scroll)
            // Scrolling changes which marks are on screen, so the ticker re-checks (`render`).
            .on_scroll_wheel(cx.listener(|_, _, _, cx| cx.notify()))
            .children(elements);
        let notice = self.notice.clone().map(|notice| {
            div()
                .flex_none()
                .mx(px(8.0))
                .mb(px(8.0))
                .px(px(8.0))
                .py(px(6.0))
                .rounded(px(6.0))
                .text_size(px(text::META))
                .bg(hsla(ui.row_active))
                .text_color(hsla(ui.text))
                .child(notice)
        });
        let hint = (!self.sidebar.hint_dismissed).then(|| self.render_hint(ui, cx));
        div()
            .relative()
            .flex()
            .flex_col()
            .flex_none()
            .w(px(self.sidebar.width))
            .h_full()
            .bg(hsla(ui.sidebar_background))
            .border_r_1()
            .border_color(hsla(ui.sidebar_border))
            // A click in the sidebar gives focus back to the terminal, which cancels a rename
            // (the rename box keeps its own clicks).
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _: &MouseDownEvent, window, cx| this.focus_active(window, cx)),
            )
            .children(band)
            .child(list)
            .children(notice)
            .children(hint)
            .child(self.render_footer(ui, cx))
            .child(self.render_resize_edge(ui, cx))
            .into_any_element()
    }

    /// A group's header: chevron, name, then (closed) its most urgent mark, its workspace count,
    /// and on hover a + for a new workspace in it. Click opens or closes it.
    fn render_header(
        &self,
        header: &HeaderRow,
        first: bool,
        ui: UiColors,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let key = header.key.clone();
        let add = header.key.clone();
        let (open, closed) = (TAU / 4.0, 0.0);
        let angle = if header.collapsed { closed } else { open };
        let color = ui.text_faint;
        let chevron = match &self.sidebar.turned {
            Some((turned, n)) if *turned == header.key => {
                let from = if header.collapsed { open } else { closed };
                div()
                    .flex_none()
                    .size(px(MARK_SLOT))
                    .with_animation(
                        ("chevron", *n),
                        Animation::new(CHEVRON_TURN),
                        move |slot, t| slot.child(chevron(from + (angle - from) * t, color)),
                    )
                    .into_any_element()
            }
            _ => chevron(angle, color).into_any_element(),
        };
        let mark = header.mark.map(|status| {
            div().flex_none().ml(px(6.0)).child(status_mark::mark(
                status,
                &ui,
                self.phase,
                None,
                px(STATUS_MARK),
            ))
        });
        let header_row = div()
            .id(("group", group_id(&header.key)))
            .group("sidebar-header")
            .flex()
            .items_center()
            .h(px(GROUP_HEADER))
            .mx(px(ROW_INSET))
            .px(px(ROW_PAD))
            .rounded(px(6.0))
            .cursor_pointer()
            .hover(|style| style.bg(hsla(ui.row_hover)))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.toggle_group(&key, cx)))
            .child(chevron)
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .ml(px(SLOT_GAP))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis_start()
                    .text_size(px(text::GROUP_HEADER))
                    .font_weight(FontWeight::SEMIBOLD)
                    .font_features(small_caps())
                    .text_color(hsla(ui.text_dim))
                    .child(header.label.clone()),
            )
            .children(mark)
            // The count, flush right like the rows' ages; on hover the + takes its place.
            .child(
                div()
                    .flex_none()
                    .relative()
                    .ml(px(6.0))
                    .min_w(px(ADD_BUTTON))
                    .h(px(ADD_BUTTON))
                    .flex()
                    .items_center()
                    .justify_end()
                    .child(
                        div()
                            .text_size(px(text::META))
                            .font_features(tabular())
                            .text_color(hsla(ui.text_faint))
                            .group_hover("sidebar-header", |style| style.invisible())
                            .child(header.count.to_string()),
                    )
                    .child(
                        div()
                            .id("group-add")
                            .absolute()
                            .top_0()
                            .right(px(-3.0))
                            .size(px(ADD_BUTTON))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(4.0))
                            .text_size(px(14.0))
                            .text_color(hsla(ui.text_dim))
                            .invisible()
                            .group_hover("sidebar-header", |style| style.visible())
                            .hover(|style| style.bg(hsla(ui.row_active)).text_color(hsla(ui.text)))
                            .child("+")
                            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                                cx.stop_propagation(); // not also a click on the header
                                this.add_to_group(&add, window, cx)
                            })),
                    ),
            );
        div()
            .flex_none()
            .when(!first, |gap| gap.pt(px(GROUP_GAP - ROW_GAP)))
            .child(header_row)
            .into_any_element()
    }

    /// A workspace's row: its most urgent terminal's mark (a × in its place on hover), its name
    /// in bold (or the rename box), age (when the row is wide enough), 📌 when pinned, on-screen
    /// pip, colour dot and ⌘N shortcut; under them where it is (`branch • ~/folder`), then while
    /// a terminal waits a 🔔 "Needs input" pill and its question (or a script's note); a
    /// script's progress along the bottom. The selected row is filled with the accent, its text
    /// and marks in `on_accent`. Click selects, double-click renames, the × or a middle-click
    /// closes, a right-click opens its menu; it can be dragged within its run (`sidebar_run`).
    fn render_workspace_row(
        &self,
        row: &WorkspaceRow,
        gap: bool,
        ui: UiColors,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let ws = row.workspace;
        let session = row.subject.and_then(|i| self.sessions.items().get(i));
        let waiting = row.status == Status::Waiting;
        // The selected row is filled with the accent; a waiting one is tinted. Hover deepens an
        // unselected one a little.
        let fill = if row.active {
            Some(ui.accent)
        } else if waiting {
            Some(ui.waiting_tint)
        } else {
            None
        };
        let hover = match fill {
            _ if row.active => None,
            Some(_) => Some(ui.waiting_hover),
            None => Some(ui.row_hover),
        };
        let surface = fill.unwrap_or(ui.sidebar_background);
        // Everything on the accent is drawn in the one colour that reads on it.
        let on = |c: Rgb| if row.active { ui.on_accent } else { c };
        let marks = on_fill(ui, row.active);
        let rename_box = self
            .renaming
            .as_ref()
            .filter(|(r, _)| *r == Renaming::Workspace(ws))
            .map(|(_, input)| input.clone());
        let run = self.sidebar_run(ws);
        // While a row is dragged: it's dimmed; rows of another run (group, or the pinned ones)
        // refuse it.
        let dragged = self
            .sidebar
            .drag_source
            .get()
            .filter(|_| cx.has_active_drag());
        let lifted = dragged == Some(ws);
        let refused = dragged.is_some_and(|d| self.sidebar_run(d) != run);
        let insertion = self
            .sidebar
            .drop_at
            .filter(|(w, _)| dragged.is_some() && *w == ws)
            .map(|(_, below)| below);

        let name: AnyElement = match rename_box {
            Some(input) => input.into_any_element(),
            None => div()
                .flex_1()
                .min_w(px(0.0))
                .truncate()
                .font_weight(FontWeight::BOLD)
                .child(row.label.clone())
                .into_any_element(),
        };
        let meta = |child: AnyElement| {
            div()
                .flex_none()
                .ml(px(META_GAP))
                .text_size(px(text::META))
                .font_features(tabular())
                .text_color(hsla(on(ui.text_faint)))
                .child(child)
        };
        // The age only where the name keeps its room.
        let age = row
            .age
            .clone()
            .filter(|_| shows_age(self.sidebar.width - 2.0 * ROW_INSET));
        let name_line = div()
            .flex()
            .items_center()
            .h(px(NAME_LINE))
            .child(self.close_slot(ws, session, marks, surface, cx))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .ml(px(SLOT_GAP))
                    .flex()
                    .child(name),
            )
            .when_some(age, |line, age| {
                line.child(meta(age.into_any_element()).ml(px(SLOT_GAP)))
            })
            .when(row.pinned, |line| line.child(meta(PIN.into_any_element())))
            .when(row.on_screen, |line| {
                line.child(
                    div()
                        .flex_none()
                        .ml(px(6.0))
                        .size(px(4.0))
                        .rounded(px(1.0))
                        .bg(hsla(ui.text_dim)),
                )
            })
            .when_some(row.color, |line, color| {
                // On the accent a dot of any colour keeps an edge that reads.
                let dot = color_dot(color, ui).ml(px(META_GAP));
                line.child(dot.when(row.active, |dot| {
                    dot.border_1().border_color(hsla(ui.on_accent))
                }))
            })
            .when_some(shortcut_label(row.index), |line, keys| {
                line.child(meta(keys.into_any_element()))
            });
        // Under the name, level with it: where it is, then what it waits on.
        let under = || {
            div()
                .flex()
                .items_center()
                .h(px(SUB_LINE))
                .pl(px(MARK_SLOT + SLOT_GAP))
                .text_size(px(text::META))
                .line_height(px(SUB_LINE))
        };
        // Short of room, the branch loses its end, then the folder its start (its own name
        // stays): `fit_place`, measured as drawn.
        let row_width = self.sidebar.width - 2.0 * ROW_INSET;
        let place = (row.branch.is_some() || row.folder.is_some()).then(|| {
            let room = row_width - 2.0 * ROW_PAD - MARK_SLOT - SLOT_GAP;
            let wrapper = std::cell::RefCell::new(
                cx.text_system()
                    .line_wrapper(TextStyle::default().font(), px(text::META)),
            );
            // As the drawn line decides whether to cut itself (`truncate`).
            let fits = |s: &str| {
                wrapper
                    .borrow_mut()
                    .should_truncate_line(s, px(room), "…", TruncateFrom::End)
                    .is_none()
            };
            let line = fit_place(row.branch.as_deref(), row.folder.as_deref(), fits);
            under()
                .text_color(hsla(on(ui.text_faint)))
                .child(div().flex_1().min_w(px(0.0)).truncate().child(line))
        });
        let status = row.sub.as_ref().map(|sub| {
            let text = |line: String, color: Rgb| {
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .truncate()
                    .text_color(hsla(on(color)))
                    .child(line)
            };
            match sub {
                Sub::Question(q) => under()
                    .child(needs_input_pill(ui, row.active))
                    .when(shows_question(row_width), |line| {
                        line.child(text(q.clone(), ui.text_dim).ml(px(6.0)))
                    }),
                Sub::NeedsInput => under().child(needs_input_pill(ui, row.active)),
                Sub::Note(note) => under().child(text(note.clone(), ui.text_faint)),
            }
        });
        let source = self.sidebar.drag_source.clone();
        let preview = DragPreview {
            label: row.label.clone().into(),
            place: (row.branch.is_some() || row.folder.is_some())
                .then(|| fit_place(row.branch.as_deref(), row.folder.as_deref(), |_| true).into()),
            status: row.status,
            width: self.sidebar.width,
        };
        let accepts = run.clone();
        div()
            .id(("workspace", ws.0 as usize))
            .group(ROW_GROUP)
            .flex_none()
            .relative()
            .flex()
            .flex_col()
            .justify_center()
            .h(px(row_height(row)))
            // Under the pinned rows, a group's gap sets the rest apart.
            .when(gap, |row| row.mt(px(GROUP_GAP - ROW_GAP)))
            .mx(px(ROW_INSET))
            .px(px(ROW_PAD))
            .rounded(px(6.0))
            .text_size(px(text::NAME))
            .text_color(hsla(on(ui.text)))
            .when_some(fill, |row, fill| row.bg(hsla(fill)))
            .when_some(hover, |row, hover| {
                row.hover(move |style| style.bg(hsla(hover)))
            })
            .when(lifted, |row| row.opacity(0.4))
            .map(|row| {
                if refused {
                    row.cursor_not_allowed()
                } else {
                    row.cursor_pointer()
                }
            })
            .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                this.select_workspace(ws, window, cx);
                if event.click_count() == 2 {
                    this.start_rename(ws, window, cx);
                }
            }))
            // A middle-click closes it, like its ×; it never selects or drags (both left only).
            .on_aux_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                if event.is_middle_click() {
                    this.close_workspace(ws, window, cx);
                }
            }))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    this.open_tab_menu(ws, event.position, window, cx)
                }),
            )
            .on_drag(
                DraggedWorkspace {
                    workspace: ws,
                    run: run.clone(),
                },
                move |dragged, _, _, cx| {
                    source.set(Some(dragged.workspace));
                    cx.new(|_| DragPreview {
                        label: preview.label.clone(),
                        place: preview.place.clone(),
                        ..preview
                    })
                },
            )
            // Only rows of the dragged workspace's run accept it; the drop re-checks.
            .can_drop(move |dragged, _, _| {
                dragged
                    .downcast_ref::<DraggedWorkspace>()
                    .is_some_and(|d| d.run == accepts)
            })
            .on_drag_move(cx.listener(
                move |this, event: &DragMoveEvent<DraggedWorkspace>, _, cx| {
                    this.drag_over_row(ws, event, cx)
                },
            ))
            .on_drop(cx.listener(move |this, dragged: &DraggedWorkspace, _, cx| {
                this.drop_workspace(dragged.workspace, ws, cx)
            }))
            .child(name_line)
            .children(place)
            .children(status)
            // The progress bar: along the bottom, as wide as the percent.
            .when_some(row.progress, |row_el, percent| {
                row_el.child(
                    div()
                        .absolute()
                        .bottom(px(1.0))
                        .left(px(ROW_PAD))
                        .right(px(ROW_PAD))
                        .h(px(2.0))
                        .child(
                            div()
                                .h_full()
                                .w(relative(f32::from(percent.min(100)) / 100.0))
                                .rounded(px(1.0))
                                .bg(hsla(on(ui.ring_working))),
                        ),
                )
            })
            // Where a dragged row would land: a line in the gap above or below this one (beside
            // the selected row, in the text colour: an accent line would merge with its fill).
            .when_some(insertion, |row_el, below| {
                let color = if row.active { ui.text } else { ui.accent };
                let line = div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .h(px(DIVIDER_ACTIVE))
                    .rounded(px(1.0))
                    .bg(hsla(color));
                row_el.child(if below {
                    line.bottom(px(-ROW_GAP))
                } else {
                    line.top(px(-ROW_GAP))
                })
            })
            .into_any_element()
    }

    /// A session's mark on its surface's `fill` (a row, or a pane tab: `place` keeps their
    /// animations apart); a Done one that just arrived pops (keyed by its pop, so it plays once;
    /// after it, the mark is drawn plain).
    pub(crate) fn session_mark(
        &self,
        place: &'static str,
        session: &Session,
        ui: UiColors,
        fill: Rgb,
    ) -> AnyElement {
        let size = px(mark_size(session.status));
        let popping = session
            .pop
            .filter(|(_, at)| session.status == Status::Done && at.elapsed() < POP_SHOWN);
        match popping {
            Some((pop, _)) => {
                let phase = self.phase;
                div()
                    .flex_none()
                    .size(size)
                    .with_animation(
                        (place, pop),
                        Animation::new(status_mark::POP),
                        move |slot, t| {
                            slot.child(status_mark::mark_on(
                                Status::Done,
                                &ui,
                                phase,
                                Some(t),
                                size,
                                fill,
                            ))
                        },
                    )
                    .into_any_element()
            }
            None => status_mark::mark_on(session.status, &ui, self.phase, None, size, fill)
                .into_any_element(),
        }
    }

    /// A row's mark slot: its session's status mark (an idle one for a workspace with no
    /// terminal), or while the row is hovered a × in its place that closes the workspace
    /// (asking first when it has 2+ terminals).
    fn close_slot(
        &self,
        ws: WorkspaceId,
        session: Option<&Session>,
        ui: UiColors,
        surface: Rgb,
        cx: &mut Context<Self>,
    ) -> Div {
        let mark = match session {
            Some(session) => self.session_mark("row", session, ui, surface),
            None => status_mark::mark_on(
                Status::Idle,
                &ui,
                self.phase,
                None,
                px(mark_size(Status::Idle)),
                surface,
            )
            .into_any_element(),
        };
        let mark = div()
            .absolute()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .child(mark)
            .group_hover(ROW_GROUP, |s| s.invisible());
        let close = div()
            .id(("row-close", ws.0 as usize))
            .absolute()
            .top(px(-(CLOSE_BUTTON - MARK_SLOT) / 2.0))
            .left(px(-(CLOSE_BUTTON - MARK_SLOT) / 2.0))
            .size(px(CLOSE_BUTTON))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(4.0))
            .text_size(px(14.0))
            .text_color(hsla(ui.text_dim))
            .hover(move |s| s.bg(hsla(ui.panel_hover)).text_color(hsla(ui.text)))
            .child("×")
            .invisible()
            .group_hover(ROW_GROUP, |s| s.visible())
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                cx.stop_propagation(); // not also a click on the row
                this.close_workspace(ws, window, cx)
            }));
        div()
            .flex_none()
            .relative()
            .size(px(MARK_SLOT))
            .child(mark)
            .child(close)
    }

    /// The pointer moved during a row drag: if it's over this row (`ws`) and dropping there
    /// would move the dragged one, the insertion line goes above or below it.
    fn drag_over_row(
        &mut self,
        ws: WorkspaceId,
        event: &DragMoveEvent<DraggedWorkspace>,
        cx: &mut Context<Self>,
    ) {
        let over = event.bounds.contains(&event.event.position);
        if !over && self.sidebar.drop_at.is_none_or(|(w, _)| w != ws) {
            return; // not this row's business
        }
        let dragged = event.drag(cx);
        let (dragged, run) = (dragged.workspace, dragged.run.clone());
        let order = self.model.display_order();
        let find = |id: WorkspaceId| order.iter().position(|&w| w == id);
        let next = match (over, find(dragged), find(ws)) {
            (true, Some(from), Some(to)) if self.sidebar_run(ws) == run => {
                let below = event.event.position.y > event.bounds.center().y;
                (drop_index(from, to, below) != from).then_some((ws, below))
            }
            _ => None,
        };
        if self.sidebar.drop_at != next {
            self.sidebar.drop_at = next;
            cx.notify();
        }
    }

    /// The first-run hint card, until its × closes it for good.
    fn render_hint(&self, ui: UiColors, cx: &mut Context<Self>) -> AnyElement {
        // A line of "·"-separated phrases wraps between phrases, never inside one.
        let lines = HINT_LINES.map(|line| {
            let phrases: Vec<&str> = line.split(" · ").collect();
            let last = phrases.len() - 1;
            div()
                .mt(px(4.0))
                .text_size(px(text::META))
                .line_height(px(15.0))
                .text_color(hsla(ui.text_dim))
                .when(last == 0, |plain| plain.child(line))
                .when(last > 0, |wrapped| {
                    wrapped
                        .flex()
                        .flex_wrap()
                        .children(phrases.into_iter().enumerate().map(|(i, phrase)| {
                            let phrase = if i < last {
                                format!("{phrase} ·\u{a0}")
                            } else {
                                phrase.to_string()
                            };
                            div().whitespace_nowrap().child(phrase)
                        }))
                })
        });
        div()
            .flex_none()
            .relative()
            .mx(px(8.0))
            .mb(px(8.0))
            .p(px(10.0))
            .rounded(px(8.0))
            .bg(hsla(ui.panel))
            .border_1()
            .border_color(hsla(ui.panel_border))
            .child(
                div()
                    .pr(px(16.0))
                    .text_size(px(12.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(hsla(ui.text))
                    .child(HINT_TITLE),
            )
            .children(lines)
            .child(
                div()
                    .id("hint-close")
                    .absolute()
                    .top(px(6.0))
                    .right(px(6.0))
                    .size(px(18.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(4.0))
                    .text_size(px(14.0))
                    .text_color(hsla(ui.text_faint))
                    .cursor_pointer()
                    .hover(|style| style.bg(hsla(ui.row_hover)).text_color(hsla(ui.text)))
                    .child("×")
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.dismiss_hint(cx))),
            )
            .into_any_element()
    }

    /// The footer: "+ New" (a menu of ways to start a session) and a gear for Preferences.
    fn render_footer(&self, ui: UiColors, cx: &mut Context<Self>) -> AnyElement {
        let open = self.sidebar.new_menu;
        let button = |id: &'static str| {
            div()
                .id(id)
                .flex()
                .items_center()
                .h(px(24.0))
                .rounded(px(6.0))
                .text_color(hsla(ui.text_dim))
                .cursor_pointer()
                .hover(|style| style.bg(hsla(ui.row_hover)).text_color(hsla(ui.text)))
        };
        div()
            .flex_none()
            .flex()
            .items_center()
            .h(px(FOOTER))
            .px(px(ROW_INSET))
            .border_t_1()
            .border_color(hsla(ui.sidebar_border))
            .child(
                button("sidebar-new")
                    .px(px(8.0))
                    .gap(px(6.0))
                    .text_size(px(12.0))
                    .when(open, |b| b.bg(hsla(ui.row_hover)).text_color(hsla(ui.text)))
                    .child(div().text_size(px(15.0)).child("+"))
                    .child("New")
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.sidebar.new_menu = !this.sidebar.new_menu;
                        cx.notify();
                    })),
            )
            .child(div().flex_1())
            .child(
                button("sidebar-preferences")
                    .w(px(24.0))
                    .justify_center()
                    .text_size(px(15.0))
                    .child("\u{2699}\u{fe0e}")
                    .on_click(|_: &ClickEvent, _, cx| preferences::open_or_activate(cx)),
            )
            .into_any_element()
    }

    /// The "+ New" menu, over the window above the footer: a new workspace (here, or in a
    /// folder) and each preset's. A click anywhere else closes it.
    pub(crate) fn render_new_menu(
        &self,
        ui: UiColors,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.sidebar.new_menu || self.sidebar.hidden {
            return None;
        }
        let hover = ui.panel_hover;
        let item = |id: (&'static str, usize), label: String, keys: String| {
            div()
                .id(id)
                .flex()
                .items_center()
                .h(px(26.0))
                .px(px(8.0))
                .rounded(px(5.0))
                .text_size(px(text::NAME))
                .text_color(hsla(ui.text))
                .cursor_pointer()
                .hover(move |style| style.bg(hsla(hover)))
                .child(div().flex_1().min_w(px(0.0)).truncate().child(label))
                .child(
                    div()
                        .flex_none()
                        .ml(px(12.0))
                        .text_size(px(text::META))
                        .text_color(hsla(ui.text_faint))
                        .child(keys),
                )
        };
        let [(new, new_keys), (in_folder, in_folder_keys)] = NEW_MENU;
        let mut items = vec![
            item(("new-menu", 0), new.into(), new_keys.into())
                .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                    this.close_new_menu(cx);
                    this.new_session(window, cx)
                }))
                .into_any_element(),
            item(("new-menu", 1), in_folder.into(), in_folder_keys.into())
                .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                    this.close_new_menu(cx);
                    this.new_session_in_folder(window, cx)
                }))
                .into_any_element(),
        ];
        let presets = &cx.global::<Appearance>().presets;
        if !presets.is_empty() {
            items.push(
                div()
                    .my(px(4.0))
                    .mx(px(8.0))
                    .h(px(1.0))
                    .bg(hsla(ui.panel_border))
                    .into_any_element(),
            );
        }
        let presets: Vec<String> = presets
            .iter()
            .take(MAX_PRESETS)
            .map(|p| p.name.clone())
            .collect();
        for (n, name) in presets.into_iter().enumerate() {
            items.push(
                item(("new-menu-preset", n), name, format!("⌃⌘{}", n + 1))
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        this.close_new_menu(cx);
                        this.launch_preset(n, window, cx)
                    }))
                    .into_any_element(),
            );
        }
        let menu = div()
            .absolute()
            .left(px(ROW_INSET))
            .bottom(px(FOOTER + 4.0))
            .w(px(MENU_WIDTH))
            .p(px(4.0))
            .rounded(px(8.0))
            .bg(hsla(ui.panel))
            .border_1()
            .border_color(hsla(ui.panel_border))
            .shadow(vec![BoxShadow {
                color: Hsla {
                    a: 0.25,
                    ..hsla(ui.shadow)
                },
                offset: point(px(0.0), px(4.0)),
                blur_radius: px(16.0),
                spread_radius: px(0.0),
                inset: false,
            }])
            // Its own clicks don't reach the backdrop under it.
            .on_mouse_down(MouseButton::Left, |_: &MouseDownEvent, _, cx| {
                cx.stop_propagation()
            })
            .children(items);
        Some(
            div()
                .id("new-menu-backdrop")
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .occlude()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _: &MouseDownEvent, _, cx| this.close_new_menu(cx)),
                )
                .child(menu)
                .into_any_element(),
        )
    }

    fn close_new_menu(&mut self, cx: &mut Context<Self>) {
        self.sidebar.new_menu = false;
        cx.notify();
    }

    /// The hint card's ×: gone, and remembered.
    fn dismiss_hint(&mut self, cx: &mut Context<Self>) {
        self.sidebar.hint_dismissed = true;
        self.save();
        cx.notify();
    }

    /// Preferences' "Show the tips card again": back, and remembered.
    pub(crate) fn show_hint(&mut self, cx: &mut Context<Self>) {
        self.sidebar.hint_dismissed = false;
        self.save();
        cx.notify();
    }

    /// The strip along the sidebar's right edge: drag it to resize the sidebar. A 2 px accent
    /// line shows on hover and while dragging. While dragging, the whole window follows the
    /// mouse until release.
    fn render_resize_edge(&self, ui: UiColors, cx: &mut Context<Self>) -> AnyElement {
        let resizing = self.sidebar.resizing;
        let this = cx.entity().downgrade();
        let follow = canvas(
            |_, _, _| (),
            move |_, _, window, _| {
                if !resizing {
                    return;
                }
                // Window-wide, not tied to the strip, so a fast move off it keeps resizing.
                let moved = this.clone();
                window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
                    if phase != DispatchPhase::Capture {
                        return;
                    }
                    moved
                        .update(cx, |this, cx| {
                            // Released where no mouse-up reached us (e.g. outside the window).
                            if event.dragging() {
                                this.resize_sidebar(f32::from(event.position.x), cx)
                            } else {
                                this.end_resize(cx)
                            }
                        })
                        .ok();
                });
                let released = this.clone();
                window.on_mouse_event(move |event: &MouseUpEvent, phase, _, cx| {
                    if phase == DispatchPhase::Capture && event.button == MouseButton::Left {
                        released.update(cx, |this, cx| this.end_resize(cx)).ok();
                    }
                });
            },
        )
        .absolute()
        .size_0();
        div()
            .id("sidebar-resize")
            .group("sidebar-edge")
            .absolute()
            .top_0()
            .bottom_0()
            .right(px(-1.0))
            .w(px(RESIZE_STRIP))
            .cursor_col_resize()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _: &MouseDownEvent, _, cx| {
                    cx.stop_propagation(); // not a click in the sidebar
                    this.sidebar.resizing = true;
                    cx.notify();
                }),
            )
            .child(
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .right(px(1.0))
                    .w(px(DIVIDER_ACTIVE))
                    .bg(hsla(ui.accent))
                    .when(!resizing, |line| {
                        line.invisible()
                            .group_hover("sidebar-edge", |style| style.visible())
                    }),
            )
            .child(follow)
            .into_any_element()
    }

    /// The sidebar's right edge follows the mouse (the sidebar starts at the window's left).
    fn resize_sidebar(&mut self, x: f32, cx: &mut Context<Self>) {
        let width = clamp_width(Some(x));
        if width != self.sidebar.width {
            self.sidebar.width = width;
            cx.notify();
        }
    }

    fn end_resize(&mut self, cx: &mut Context<Self>) {
        if std::mem::take(&mut self.sidebar.resizing) {
            self.save(); // once per drag, not on every move
            cx.notify();
        }
    }

    /// Open or close a group from its header; its chevron turns.
    fn toggle_group(&mut self, key: &GroupKey, cx: &mut Context<Self>) {
        match self.collapsed.iter().position(|c| c == key) {
            Some(i) => {
                self.collapsed.remove(i);
            }
            None => self.collapsed.push(key.clone()),
        }
        self.sidebar.turns += 1;
        self.sidebar.turned = Some((key.clone(), self.sidebar.turns));
        self.save();
        cx.notify();
    }
}

/// A header's element id: stable while its workspaces are reordered or closed.
fn group_id(key: &GroupKey) -> u64 {
    let mut hasher = DefaultHasher::new();
    key.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn the_hint_and_the_new_menu_say_n_makes_a_workspace_and_t_a_tab() {
        assert_eq!(
            HINT_LINES[0],
            "⌘N workspace · ⌘T tab · ⇧⌘S SSH · ⇧⌘B browser"
        );
        assert_eq!(
            NEW_MENU,
            [("New Workspace", "⌘N"), ("New Workspace in Folder…", "⌘O")]
        );
    }

    fn folder(p: &str) -> GroupKey {
        GroupKey::Folder(PathBuf::from(p))
    }

    fn facts(
        label: &str,
        status: Status,
        since: Instant,
        group: Option<GroupKey>,
    ) -> WorkspaceFacts<'_> {
        WorkspaceFacts {
            workspace: WorkspaceId(0),
            label,
            subject: Some(0),
            status,
            since,
            question: None,
            group,
            progress: None,
            note: None,
            lost: false,
            color: None,
            pinned: false,
            branch: None,
            folder: None,
        }
    }

    /// Each row's workspace is its place (as `rows` takes them).
    fn numbered(mut rows: Vec<WorkspaceFacts<'_>>) -> Vec<WorkspaceFacts<'_>> {
        for (i, f) in rows.iter_mut().enumerate() {
            f.workspace = WorkspaceId(i as u64);
        }
        rows
    }
    fn w(i: u64) -> WorkspaceId {
        WorkspaceId(i)
    }

    fn session(row: &SidebarRow) -> &WorkspaceRow {
        match row {
            SidebarRow::Workspace(s) => s,
            SidebarRow::Header(h) => panic!("a header: {h:?}"),
        }
    }

    fn header(row: &SidebarRow) -> &HeaderRow {
        match row {
            SidebarRow::Header(h) => h,
            SidebarRow::Workspace(s) => panic!("a session: {s:?}"),
        }
    }

    #[test]
    fn rows_build_from_sessions() {
        let now = Instant::now();
        let ago = |s: u64| now - Duration::from_secs(s);
        let api = folder("/w/api");
        let bar = (40, Some("tests".to_string()));
        let mut sessions = vec![
            facts("shell", Status::Idle, ago(900), None),
            facts("claude", Status::Waiting, ago(240), Some(api.clone())),
            facts("codex", Status::Working, ago(32), Some(api.clone())),
            facts("tests", Status::Done, ago(7200), Some(api.clone())),
            facts("agent", Status::Waiting, ago(5), Some(api.clone())),
        ];
        sessions[1].question = Some("Do you want to make this edit to auth.rs?");
        // A note and a bar: the waiting question wins over the note; others show it.
        sessions[1].note = Some("ignored while waiting");
        sessions[2].progress = Some(&bar);
        sessions[2].note = Some("3 of 7");
        sessions[2].color = Some(TabColor::Teal);
        let sessions = numbered(sessions);
        let rows = rows(&sessions, &[], Some(w(1)), &[w(1), w(3)], now);
        assert_eq!(rows.len(), 6, "{rows:?}");

        let shell = session(&rows[0]);
        assert_eq!((shell.index, shell.label.as_str()), (0, "shell"));
        assert!(!shell.grouped && !shell.active && !shell.on_screen);
        // Idle shows no age; the others how long they've been in their status.
        assert_eq!(shell.age, None);
        assert_eq!(shell.sub, None);

        assert_eq!(header(&rows[1]).label, "api");

        let claude = session(&rows[2]);
        assert!(claude.grouped && claude.active);
        // The active session isn't also marked as on screen.
        assert!(!claude.on_screen);
        assert_eq!(claude.age.as_deref(), Some("4m"));
        assert_eq!(
            claude.sub,
            Some(Sub::Question(
                "Do you want to make this edit to auth.rs?".into()
            ))
        );

        let codex = session(&rows[3]);
        assert_eq!(codex.age.as_deref(), Some("32s"));
        assert_eq!(codex.sub, Some(Sub::Note("tests · 3 of 7".into())));
        assert_eq!(codex.progress, Some(40));
        assert_eq!(codex.color, Some(TabColor::Teal));
        assert!(!codex.on_screen);

        let tests = session(&rows[4]);
        assert!(tests.on_screen && !tests.active);
        assert_eq!(tests.age.as_deref(), Some("2h"));

        // Waiting with no question found: says it needs you.
        let agent = session(&rows[5]);
        assert_eq!(agent.sub, Some(Sub::NeedsInput));
        assert_eq!(agent.index, 4);
        assert_eq!(shell.color, None);
    }

    #[test]
    fn a_lost_session_says_not_live() {
        let now = Instant::now();
        let mut s = facts("api", Status::Waiting, now, None);
        s.question = Some("Proceed? (y/n)");
        s.lost = true;
        let rows = rows(&numbered(vec![s]), &[], None, &[], now);
        assert_eq!(session(&rows[0]).sub, Some(Sub::Note("not live".into())));
    }

    #[test]
    fn headers_count_and_roll_up_only_when_collapsed() {
        let now = Instant::now();
        let (api, web) = (folder("/w/api"), folder("/w/web"));
        let sessions = [
            facts("a", Status::Working, now, Some(api.clone())),
            facts("b", Status::Waiting, now, Some(api.clone())),
            facts("c", Status::Done, now, Some(api.clone())),
            facts("d", Status::Done, now, Some(web.clone())),
            facts("e", Status::Idle, now, Some(web.clone())),
        ];
        let rows = rows(&numbered(sessions.into()), &[web.clone()], None, &[], now);
        // api open (header + 3 rows), web closed (its header only).
        assert_eq!(rows.len(), 5, "{rows:?}");
        let open = header(&rows[0]);
        assert_eq!((open.count, open.first, open.collapsed), (3, 0, false));
        assert_eq!(open.mark, None);
        let closed = header(&rows[4]);
        assert_eq!((closed.count, closed.first, closed.collapsed), (2, 3, true));
        assert_eq!(closed.label, "web");
        assert_eq!(closed.mark, Some(Status::Done));
    }

    #[test]
    fn the_line_under_the_name_joins_label_and_note() {
        let bar = |l: Option<&str>| (40, l.map(String::from));
        assert_eq!(sub_line(None, None), None);
        assert_eq!(sub_line(Some(&bar(None)), None), None);
        assert_eq!(sub_line(None, Some("n")), Some("n".into()));
        assert_eq!(sub_line(Some(&bar(Some("l"))), None), Some("l".into()));
        assert_eq!(
            sub_line(Some(&bar(Some("l"))), Some("n")),
            Some("l · n".into())
        );
    }

    #[test]
    fn the_first_nine_rows_show_their_shortcut() {
        assert_eq!(shortcut_label(0).as_deref(), Some("⌘1"));
        assert_eq!(shortcut_label(4).as_deref(), Some("⌘5"));
        assert_eq!(shortcut_label(8).as_deref(), Some("⌘9"));
        assert_eq!(shortcut_label(9), None);
        assert_eq!(shortcut_label(23), None);
    }

    #[test]
    fn the_age_shows_only_on_rows_220_px_or_wider() {
        assert!(shows_age(220.0));
        assert!(shows_age(388.0));
        assert!(!shows_age(219.0));
        assert!(!shows_age(168.0)); // the narrowest sidebar's row
    }

    #[test]
    fn width_clamps() {
        assert_eq!(clamp_width(None), SIDEBAR_DEFAULT_WIDTH);
        assert_eq!(clamp_width(Some(250.0)), 250.0);
        assert_eq!(clamp_width(Some(10.0)), SIDEBAR_MIN_WIDTH);
        assert_eq!(clamp_width(Some(9000.0)), SIDEBAR_MAX_WIDTH);
        assert_eq!(clamp_width(Some(180.0)), 180.0);
        assert_eq!(clamp_width(Some(400.0)), 400.0);
        // A hand-edited file can hold anything.
        assert_eq!(clamp_width(Some(f32::NAN)), SIDEBAR_DEFAULT_WIDTH);
        assert_eq!(clamp_width(Some(f32::INFINITY)), SIDEBAR_DEFAULT_WIDTH);
    }

    #[test]
    fn drop_lands_where_the_line_is() {
        // Rows 0..5. Dragging 1 down: onto 3's upper half lands between 2 and 3.
        assert_eq!(drop_index(1, 3, false), 2);
        assert_eq!(drop_index(1, 3, true), 3);
        // Dragging 3 up: onto 1's upper half lands before 1; its lower half after it.
        assert_eq!(drop_index(3, 1, false), 1);
        assert_eq!(drop_index(3, 1, true), 2);
        // Onto itself, or the line right next to it: stays.
        assert_eq!(drop_index(2, 2, false), 2);
        assert_eq!(drop_index(2, 2, true), 2);
        assert_eq!(drop_index(2, 1, true), 2);
        assert_eq!(drop_index(2, 3, false), 2);
    }

    #[test]
    fn ages_change_at_their_next_label() {
        let s = Duration::from_secs;
        let ms = Duration::from_millis;
        assert_eq!(next_age_change(ms(12_300)), ms(700));
        assert_eq!(next_age_change(s(12)), s(1));
        assert_eq!(next_age_change(s(59)), s(1));
        assert_eq!(next_age_change(s(4 * 60 + 10)), s(50));
        assert_eq!(next_age_change(s(2 * 3600 + 60)), s(3540));
        assert_eq!(next_age_change(s(3 * 86400)), s(86400));
        // The labels say what `age_label` says just after.
        for age in [ms(12_300), s(59), s(4 * 60 + 10), s(2 * 3600 + 60)] {
            let next = age + next_age_change(age);
            assert_ne!(status::age_label(age), status::age_label(next), "{age:?}");
            assert_eq!(
                status::age_label(age),
                status::age_label(next - ms(1)),
                "{age:?}"
            );
        }
    }

    #[test]
    fn rows_scrolled_out_of_view_are_not_in_view() {
        // A 100 px viewport at y 50, rows 28 px tall from y 50.
        let row = |i: f32| (50.0 + 28.0 * i, 50.0 + 28.0 * (i + 1.0));
        let shows = |i: f32, offset: f32| {
            let (top, bottom) = row(i);
            in_view(top, bottom, 50.0, 150.0, offset)
        };
        assert!(shows(0.0, 0.0) && shows(3.0, 0.0));
        assert!(!shows(4.0, 0.0));
        // Scrolled down by 3 rows: the first three are gone, rows 3-6 show.
        assert!(!shows(2.0, -84.0));
        assert!(shows(3.0, -84.0) && shows(6.0, -84.0));
        assert!(!shows(7.0, -84.0));
    }

    #[test]
    fn a_workspace_without_a_terminal_keeps_its_row_and_the_rows_below_keep_theirs() {
        use crate::layout_model::{Model, TabKind};
        let mut m = Model::new();
        let a = m.new_workspace(0, TabKind::Terminal(1)).unwrap();
        let web = TabKind::Browser {
            url: Some("http://localhost:3000".into()),
        };
        let b = m.new_workspace(1, web).unwrap();
        let c = m.new_workspace(2, TabKind::Terminal(2)).unwrap();
        m.focus_workspace(c.workspace);
        // Sessions s1 and s2 are at places 0 and 1 of the session list.
        let index_of = |id: u64| Some(id as usize - 1);
        let status = |id: u64| {
            if id == 2 {
                Status::Waiting
            } else {
                Status::Idle
            }
        };
        let subjects = row_subjects(&m, index_of, status);
        assert_eq!(
            subjects,
            [
                (a.workspace, Some(0)),
                (b.workspace, None),
                (c.workspace, Some(1))
            ]
        );
        // Its row: idle, named after the workspace (else "Browser"); c's row is still c's.
        let now = Instant::now();
        let mut browser = facts(BROWSER_NAME, Status::Idle, now, None);
        browser.subject = None;
        let mut list = vec![
            facts("api", Status::Idle, now, None),
            browser,
            facts("agent", Status::Waiting, now, None),
        ];
        for (f, (ws, subject)) in list.iter_mut().zip(&subjects) {
            f.workspace = *ws;
            f.subject = *subject;
        }
        let rows = rows(&list, &[], m.focused_workspace(), &[], now);
        let picked: Vec<(WorkspaceId, usize, Option<usize>, bool)> = rows
            .iter()
            .map(session)
            .map(|r| (r.workspace, r.index, r.subject, r.active))
            .collect();
        assert_eq!(
            picked,
            [
                (a.workspace, 0, Some(0), false),
                (b.workspace, 1, None, false),
                (c.workspace, 2, Some(1), true)
            ]
        );
        assert_eq!(session(&rows[1]).label, "Browser");
        assert_eq!(session(&rows[1]).status, Status::Idle);
    }

    #[test]
    fn a_row_says_where_its_workspace_is() {
        let now = Instant::now();
        let mut api = facts("api", Status::Idle, now, Some(folder("/w/api")));
        api.branch = Some("main");
        api.folder = Some("~/w/api".into());
        // Not in a repo: the folder alone.
        let mut notes = facts("notes", Status::Idle, now, None);
        notes.folder = Some("~/notes".into());
        // Nowhere known (a browser-only workspace).
        let bare = facts("Browser", Status::Idle, now, None);
        let rows = rows(&numbered(vec![notes, bare, api]), &[], None, &[], now);
        let picked: Vec<(Option<&str>, Option<&str>)> = rows
            .iter()
            .filter_map(|r| match r {
                SidebarRow::Workspace(w) => Some((w.branch.as_deref(), w.folder.as_deref())),
                SidebarRow::Header(_) => None,
            })
            .collect();
        assert_eq!(
            picked,
            [
                (None, Some("~/notes")),
                (None, None),
                (Some("main"), Some("~/w/api"))
            ]
        );
    }

    #[test]
    fn pinned_rows_come_first_out_of_their_groups_with_their_pin() {
        let now = Instant::now();
        let (api, web) = (folder("/w/api"), folder("/w/web"));
        // Display order (the model's): pinned ones first, whatever their groups.
        let mut list = vec![
            facts("deploy", Status::Idle, now, Some(web.clone())),
            facts("review", Status::Waiting, now, Some(api.clone())),
            facts("shell", Status::Idle, now, None),
            facts("claude", Status::Working, now, Some(api.clone())),
            facts("site", Status::Idle, now, Some(web.clone())),
        ];
        list[0].pinned = true;
        list[1].pinned = true;
        let rows = rows(&numbered(list), &[web.clone()], Some(w(1)), &[], now);
        let shape: Vec<String> = rows
            .iter()
            .map(|r| match r {
                SidebarRow::Header(h) => format!("[{}]", h.label),
                SidebarRow::Workspace(w) => {
                    format!(
                        "{}{}{}",
                        w.label,
                        ["", " pin"][w.pinned as usize],
                        ["", " g"][w.grouped as usize]
                    )
                }
            })
            .collect();
        // web is closed, but its pinned workspace still shows (it's not in the group now).
        assert_eq!(
            shape,
            [
                "deploy pin",
                "review pin",
                "shell",
                "[api]",
                "claude g",
                "[web]"
            ]
        );
        // ⌘1 and ⌘2 are the pinned ones; the header counts only the unpinned.
        assert_eq!(session(&rows[1]).index, 1);
        assert!(session(&rows[1]).active);
        assert_eq!((header(&rows[3]).first, header(&rows[3]).count), (3, 1));
        assert_eq!((header(&rows[5]).first, header(&rows[5]).count), (4, 1));
    }

    #[test]
    fn an_ssh_workspace_shows_its_host_and_a_folder_only_once_reported() {
        assert_eq!(remote_place("devbox", None), "devbox");
        assert_eq!(
            remote_place("devbox", Some(Path::new("/home/me/api"))),
            "devbox:/home/me/api"
        );
    }

    #[test]
    fn under_home_a_folder_starts_with_a_tilde() {
        let home = Some(Path::new("/Users/me"));
        assert_eq!(home_relative(Path::new("/Users/me"), home), "~");
        assert_eq!(
            home_relative(Path::new("/Users/me/code/api"), home),
            "~/code/api"
        );
        // Only whole folder names: /Users/meow isn't under /Users/me.
        assert_eq!(
            home_relative(Path::new("/Users/meow/x"), home),
            "/Users/meow/x"
        );
        assert_eq!(home_relative(Path::new("/tmp"), home), "/tmp");
        assert_eq!(home_relative(Path::new("/Users/me/x"), None), "/Users/me/x");
    }

    #[test]
    fn a_row_grows_a_line_for_where_it_is_and_one_for_what_it_waits_on() {
        let now = Instant::now();
        let mut placed = facts("api", Status::Idle, now, None);
        placed.folder = Some("~/api".into());
        let mut waiting = facts("claude", Status::Waiting, now, None);
        waiting.folder = Some("~/api".into());
        waiting.branch = Some("main");
        let plain_waiting = facts("agent", Status::Waiting, now, None);
        let plain = facts("Browser", Status::Idle, now, None);
        let rows = rows(
            &numbered(vec![plain, placed, plain_waiting, waiting]),
            &[],
            None,
            &[],
            now,
        );
        let heights: Vec<f32> = rows.iter().map(|r| row_height(session(r))).collect();
        assert_eq!(heights, [SIDEBAR_ROW, WAITING_ROW, WAITING_ROW, 56.0]);
    }

    #[test]
    fn the_place_line_fits_by_cutting_the_branch_then_the_folder_start() {
        // One unit per character.
        let fit = |b: Option<&str>, f: Option<&str>, w: f32| {
            fit_place(b, f, |s: &str| s.chars().count() as f32 <= w)
        };
        let (branch, folder) = (Some("feature/payments-retry"), Some("~/code/shop/payments"));
        // Room for all of it ("feature/payments-retry • ~/code/shop/payments" is 45).
        assert_eq!(
            fit(branch, folder, 45.0),
            "feature/payments-retry • ~/code/shop/payments"
        );
        // The branch gives way first, down to 8 characters and an ellipsis.
        assert_eq!(
            fit(branch, folder, 35.0),
            "feature/pay… • ~/code/shop/payments"
        );
        assert_eq!(
            fit(branch, folder, 32.0),
            "feature/… • ~/code/shop/payments"
        );
        // Then the folder loses its start, keeping its own name.
        assert_eq!(fit(branch, folder, 26.0), "feature/… • …shop/payments");
        assert_eq!(fit(branch, folder, 21.0), "feature/… • …payments");
        // Past that, as short as it goes (the drawn line cuts its end).
        assert_eq!(fit(branch, folder, 10.0), "feature/… • …payments");
        // A short branch is never cut; a folder alone, a branch alone.
        assert_eq!(fit(Some("main"), folder, 16.0), "main • …payments");
        assert_eq!(fit(None, Some("/tmp/scratch"), 30.0), "/tmp/scratch");
        assert_eq!(
            fit(None, Some("~/code/shop/payments"), 12.0),
            "…op/payments"
        );
        assert_eq!(fit(Some("main"), None, 2.0), "main");
        assert_eq!(fit(None, None, 50.0), "");
        // Characters, not bytes.
        assert_eq!(
            fit(Some("café-branché-x"), Some("~/é"), 15.0),
            "café-bra… • ~/é"
        );
    }

    #[test]
    fn a_narrow_waiting_row_shows_the_pill_alone() {
        assert!(shows_question(228.0));
        assert!(shows_question(200.0));
        assert!(!shows_question(199.0));
        assert!(!shows_question(168.0)); // the narrowest sidebar's row
    }

    #[test]
    fn the_first_unpinned_row_after_the_pinned_ones_has_a_gap() {
        let now = Instant::now();
        let mut list = vec![
            facts("deploy", Status::Idle, now, None),
            facts("review", Status::Idle, now, None),
            facts("shell", Status::Idle, now, None),
            facts("claude", Status::Idle, now, Some(folder("/w/api"))),
        ];
        list[0].pinned = true;
        list[1].pinned = true;
        let rows = rows(&numbered(list), &[], None, &[], now);
        let gaps: Vec<bool> = rows
            .iter()
            .enumerate()
            .map(|(i, r)| gap_before(i.checked_sub(1).map(|p| &rows[p]), r))
            .collect();
        // deploy, review | shell (gap), [api] (its own), claude.
        assert_eq!(gaps, [false, false, true, false, false]);
    }
}
