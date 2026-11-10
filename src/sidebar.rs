//! The sidebar (M11; M11b: a row per workspace): every workspace as a row that reads at a
//! glance, under its group's header (its first terminal's folder or host); pinned workspaces
//! lead, above every header, with a 📌.
//!
//! `rows` is the pure view-model: what each row shows (its name in semibold, its most urgent
//! terminal's status and how long it has been in it, whether a program runs there, focused,
//! pinned, its colour, and one line under the name: `line_two`), built from facts the
//! Workspace hands over. The rest of the file draws it (M11i: compact, 22 pt with one line and
//! 32 pt with two); the selected row is a soft tint of the accent with a 2 pt accent bar at its
//! left, everything on it in its own colours. Rows behave like tabs: a × on hover in the mark's
//! place, a middle-click closes, a right-click opens the row's menu (`tab_menu`, with Pin /
//! Unpin); on the right, the colour's dot and, on hover, the ⌘1–⌘9 shortcut.

use std::cell::Cell;
use std::f32::consts::TAU;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use alacritty_terminal::vte::ansi::Rgb;
use gpui::{
    Animation, AnimationExt, AnyElement, AnyView, App, Bounds, BoxShadow, ClickEvent, Context,
    DispatchPhase, Div, DragMoveEvent, FontFeatures, FontWeight, Hsla, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, PathBuilder, Pixels, ScrollHandle, SharedString, Task, TextStyle,
    TruncateFrom, Window, canvas, div, point, prelude::*, px, relative,
};

use crate::appearance::Appearance;
use crate::drag_gap::{self, Slid};
use crate::groups::{self, GroupKey, Row};
use crate::layout_model::{Model, TabDrop, TabKind, WorkspaceId};
use crate::preferences;
use crate::presets::MAX_PRESETS;
use crate::procs::ProgramState;
use crate::reduce_motion::WindowVisible;
use crate::restore::SavedState;
use crate::sidebar_toggle::{Tip, open_button_at};
use crate::status::{self, Status};
use crate::status_mark;
use crate::tab_bar::{DraggedTab, Glyph, glyph, hold_closed_hand};
use crate::tab_color::TabColor;
use crate::tab_menu::TabMenu;
use crate::terminal_view::hsla;
use crate::theme::UiColors;
use crate::title_area::{
    BELL_GROUP, FOOTER_BUTTON, FOOTER_GAP, NOTIFICATIONS_TIP, PREFERENCES_GROUP, PREFERENCES_TIP,
    footer_icon, unseen_dot,
};
use crate::ui_tokens::size::{
    DIVIDER_ACTIVE, GROUP_HEADER, SIDEBAR_DEFAULT_WIDTH, SIDEBAR_MAX_WIDTH, SIDEBAR_MIN_WIDTH,
    SIDEBAR_ROW, SIDEBAR_ROW_THREE, SIDEBAR_ROW_TWO, STATUS_BAR, STATUS_MARK,
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
    /// A program other than the shell runs in `subject` (M11i: an idle row's filled dot).
    pub running: bool,
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
    /// Where it is: the branch (only inside a git repo) and the folder, each on its own line
    /// under the name (2026-11-10; it was one `branch • folder` line).
    pub branch: Option<String>,
    pub folder: Option<String>,
    /// A program other than the shell runs in its subject: idle, its mark is a filled dot, not
    /// a ring (M11i).
    pub running: bool,
}

/// The line under a workspace's name.
#[derive(Clone, Debug, PartialEq)]
pub enum Sub {
    /// What a waiting session is asking.
    Question(String),
    /// Waiting, but no question was found on screen (the "Needs input" pill alone).
    NeedsInput,
    /// A script's note or progress label.
    Note(String),
    /// The background service's connection was lost (M10a).
    NotLive,
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
                    running: s.running,
                })
            }
        })
        .collect()
}

/// The line under a workspace's name, from its most urgent terminal: "not live" for a lost
/// one; a waiting one's question; else a script's progress label and note.
fn sub(s: &WorkspaceFacts) -> Option<Sub> {
    if s.lost {
        return Some(Sub::NotLive);
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

/// Whether a waiting row `row_width` wide has room for its question after "Needs input" (else
/// "Needs input" shows alone: the question is in ⌘P and the notification).
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

/// A row's second line (M11i), the first that applies.
#[derive(Clone, Debug, PartialEq)]
pub enum LineTwo {
    /// A lost background session.
    NotLive,
    /// Waiting: "Needs input", then the question when one was found.
    Waiting(Option<String>),
    /// A script's note or progress label.
    Note(String),
    /// Where it is: the branch and the folder, each on its own line (`lines_under`).
    Place,
}

/// What a row shows under its name (M11i), the first that applies: "not live" for a lost
/// background session; waiting, "Needs input" and its question; a script's note; else where it
/// is (its branch and folder). None when there's nothing to say.
pub fn line_two(row: &WorkspaceRow) -> Option<LineTwo> {
    match &row.sub {
        Some(Sub::NotLive) => Some(LineTwo::NotLive),
        Some(Sub::Question(q)) => Some(LineTwo::Waiting(Some(q.clone()))),
        Some(Sub::NeedsInput) => Some(LineTwo::Waiting(None)),
        Some(Sub::Note(note)) => Some(LineTwo::Note(note.clone())),
        None => (row.branch.is_some() || row.folder.is_some()).then_some(LineTwo::Place),
    }
}

/// How many lines a row has under its name: none, the one `line_two` says, or two when that is
/// where it is and it knows both its branch and its folder (the branch, then the folder). No
/// branch line outside a git repo.
pub fn lines_under(row: &WorkspaceRow) -> usize {
    match line_two(row) {
        None => 0,
        Some(LineTwo::Place) if row.branch.is_some() && row.folder.is_some() => 2,
        Some(_) => 1,
    }
}

/// What a dragged row shows under its name, as tall as the row itself: its branch and its
/// folder on their own lines when the row has both (`lines_under` is 2), else what it knows of
/// its place on one line (`branch • folder` under a waiting row's single line, as before).
pub fn place_lines(row: &WorkspaceRow) -> Vec<String> {
    let (branch, folder) = (row.branch.as_deref(), row.folder.as_deref());
    match lines_under(row) {
        2 => [branch, folder]
            .into_iter()
            .flatten()
            .map(String::from)
            .collect(),
        _ if branch.is_some() || folder.is_some() => vec![fit_place(branch, folder, |_| true)],
        _ => Vec::new(),
    }
}

/// A row's height: its name line alone, or with the lines under it (`lines_under`).
pub fn row_height(row: &WorkspaceRow) -> f32 {
    match lines_under(row) {
        0 => SIDEBAR_ROW,
        1 => SIDEBAR_ROW_TWO,
        _ => SIDEBAR_ROW_THREE,
    }
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

/// A row's tooltip (note 60 §2): its name, then the program's own title when it says more
/// (`extra_title`), then, while the row waits, its whole `question` on its own line (the row
/// shows it cut to one); none when there is nothing past the name.
pub fn row_tip(label: &str, title: Option<&str>, question: Option<&str>) -> Option<String> {
    let lines: Vec<&str> = [
        groups::extra_title(label, title),
        question.filter(|q| !q.trim().is_empty()),
    ]
    .into_iter()
    .flatten()
    .collect();
    (!lines.is_empty()).then(|| format!("{label}\n{}", lines.join("\n")))
}

/// Whether a row's idle mark is a dot (a program runs there) rather than a ring: for an idle
/// row, terminal `named`'s (the one it is named after), so its mark and its name describe the
/// same terminal; otherwise the rollup `subject`'s (its mark isn't the idle one then).
pub fn row_running(
    status: Status,
    subject: Option<u64>,
    named: Option<u64>,
    running: impl Fn(u64) -> bool,
) -> bool {
    let id = if status == Status::Idle {
        named
    } else {
        subject
    };
    id.is_some_and(running)
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

/// How far each item of the sidebar list is drawn from its place while the row at `own` is
/// dragged within its run (`run`: whether each item is a row of it): a gap as tall as the row,
/// `height`, and the gap after it, opens at slot `slot` among the run's other rows (none: at its
/// own place); rows outside the run (and headers) stay.
pub fn row_offsets(run: &[bool], own: usize, slot: Option<usize>, height: f32) -> Vec<f32> {
    let members: Vec<usize> = (0..run.len()).filter(|&i| run[i]).collect();
    let mut offsets = vec![0.0; run.len()];
    let Some(at) = members.iter().position(|&i| i == own) else {
        return offsets;
    };
    let gap = slot.unwrap_or(at);
    let moved = drag_gap::gap_offsets(members.len(), Some(at), Some(gap), height + ROW_GAP);
    for (i, offset) in members.into_iter().zip(moved) {
        offsets[i] = offset;
    }
    offsets
}

/// The slot a row dragged within its run lands in (the gap nearest its copy: `landing_slot`),
/// from its copy's `centre` and the run's rows' `(top, bottom)` as laid out (scrolled), its own at `own`, `size` the room it takes (its height
/// and the gap after it); none while its centre is off the run by more than half of it (a drop
/// there would put it back).
pub fn row_slot(centre: f32, spans: &[(f32, f32)], own: usize, size: f32) -> Option<usize> {
    let (first, last) = (spans.first()?, spans.last()?);
    let reach = size / 2.0;
    (first.0 - reach..=last.1 + reach)
        .contains(&centre)
        .then(|| drag_gap::landing_slot(centre, spans, Some(own), size))
}

/// How far each of the list's `count` items is drawn from its place while the row at `own` is
/// dragged into another run, to land before item `at` (`insert_point`): the items between
/// slide by `size` (its height and the gap after it) towards its own place, which closes up.
pub fn cross_offsets(count: usize, own: usize, at: usize, size: f32) -> Vec<f32> {
    (0..count)
        .map(|i| {
            if at <= own && (at..own).contains(&i) {
                size
            } else if at > own && (own + 1..at).contains(&i) {
                -size
            } else {
                0.0
            }
        })
        .collect()
}

/// Which part of its group's card a list item is (2026-11-10): none (pinned and ungrouped
/// rows, above every header), a header with rows under it, a header alone (a closed or empty
/// group), a row with more under it, or a group's last row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CardPart {
    Out,
    Top,
    Alone,
    Middle,
    Bottom,
}

/// Each list item's part of a card, from whether each is a header: a header opens a card that
/// its rows fill until the next header or the end.
pub fn card_parts(headers: &[bool]) -> Vec<CardPart> {
    let mut in_card = false;
    (0..headers.len())
        .map(|i| {
            let last = headers.get(i + 1).is_none_or(|&next| next);
            if headers[i] {
                in_card = true;
                if last { CardPart::Alone } else { CardPart::Top }
            } else if !in_card {
                CardPart::Out
            } else if last {
                CardPart::Bottom
            } else {
                CardPart::Middle
            }
        })
        .collect()
}

/// `card_parts` while the row at `own` is dragged to land before item `at`: the parts of the
/// list as it will be (that row taken out, a row put in before `at`); the dragged row's own
/// part is the gap's, where its card is drawn (2026-11-10: the gap is part of the card it opens
/// in, and nothing is drawn where the row was).
pub fn drag_parts(headers: &[bool], own: usize, at: usize) -> Vec<CardPart> {
    // The order as it will be: every item but `own`, the gap (`own`) before `at`.
    let mut order: Vec<usize> = (0..headers.len()).filter(|&i| i != own).collect();
    let place = order.iter().position(|&i| i >= at).unwrap_or(order.len());
    order.insert(place, own);
    let flags: Vec<bool> = order.iter().map(|&i| i != own && headers[i]).collect();
    let mut parts = vec![CardPart::Out; headers.len()];
    for (part, &i) in card_parts(&flags).into_iter().zip(&order) {
        parts[i] = part;
    }
    parts
}

/// How far from its place the dragged row (item `own`, laid out at `spans`) is drawn: at the
/// gap that opens before item `at`, `size` tall with the gap after it (`cross_offsets`).
pub fn gap_offset(spans: &[(f32, f32)], own: usize, at: usize, size: f32) -> f32 {
    let top = |i: usize| match spans.get(i) {
        Some(span) => span.0,
        None => spans.last().map_or(0.0, |last| last.1 + ROW_GAP),
    };
    let gap = if at <= own { top(at) } else { top(at) - size };
    gap - top(own)
}

/// The card behind list item `part`, `offset` from its place (the item's drag slide): joined
/// across the gap under it to the next item of its group, past the header's top and the last
/// row's bottom by `CARD_PAD`; none outside a card.
fn card_band(part: CardPart, offset: f32, ui: &UiColors) -> Option<Div> {
    let (top, bottom, opens, closes) = match part {
        CardPart::Out => return None,
        CardPart::Top => (-CARD_PAD, -ROW_GAP, true, false),
        CardPart::Alone => (-CARD_PAD, -CARD_PAD, true, true),
        CardPart::Middle => (0.0, -ROW_GAP, false, false),
        CardPart::Bottom => (0.0, -CARD_PAD, false, true),
    };
    Some(
        div()
            .absolute()
            .left_0()
            .right_0()
            .top(px(offset + top))
            .bottom(px(bottom - offset))
            .bg(hsla(ui.card))
            .border_color(hsla(ui.card_border))
            .border_l_1()
            .border_r_1()
            .when(opens, |d| d.border_t_1().rounded_t(px(CARD_RADIUS)))
            .when(closes, |d| d.border_b_1().rounded_b(px(CARD_RADIUS))),
    )
}

/// An item of the sidebar list as last drawn.
#[derive(Clone, Debug, PartialEq)]
pub enum Listed {
    Row(WorkspaceId),
    Header(GroupKey),
}

/// An item of the sidebar list, as a drag sees it.
#[derive(Clone, Debug, PartialEq)]
pub enum ListItem {
    /// A pinned row (unpinned rows never land among them).
    Pinned,
    Row,
    Header(GroupKey),
}

/// Where an unpinned row (item `own` of `items`, laid out at `spans`: top and bottom) dragged
/// with its copy's centre at `centre` lands (2026-11-10, your call): before item `at`, in the
/// group of the nearest header above that (none: out of every group, above the first header),
/// at slot `slot` among that group's other rows. None where it would stay as it is.
pub fn drop_place(
    items: &[ListItem],
    spans: &[(f32, f32)],
    own: usize,
    centre: f32,
) -> Option<(usize, Option<GroupKey>, usize)> {
    let first = items
        .iter()
        .position(|i| *i != ListItem::Pinned)
        .unwrap_or(items.len());
    let mid = |i: usize| (spans[i].0 + spans[i].1) / 2.0;
    let at = (first..items.len())
        .filter(|&i| i != own)
        .find(|&i| mid(i) > centre)
        .unwrap_or(items.len());
    // The group a place `p` is in, and its slot there among the rows other than `own`.
    let place = |p: usize| {
        let header = (first..p).rev().find_map(|i| match &items[i] {
            ListItem::Header(key) => Some((i, key.clone())),
            _ => None,
        });
        let (start, group) = header.map_or((first, None), |(i, key)| (i + 1, Some(key)));
        let slot = (start..p)
            .filter(|&i| i != own && items[i] == ListItem::Row)
            .count();
        (group, slot)
    };
    let (group, slot) = place(at);
    (place(own) != (group.clone(), slot)).then_some((at, group, slot))
}

/// Where a row dropped at `slot` of its run goes, as `Model::move_workspace_display` takes it:
/// `in_run` says whether each workspace in display order is of the run (one stretch of it).
pub fn run_drop_index(in_run: &[bool], slot: usize) -> Option<usize> {
    let start = in_run.iter().position(|&r| r)?;
    Some(start + slot)
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
pub(crate) const ROW_INSET: f32 = 6.0;
const ROW_PAD: f32 = 8.0;
/// Between rows; above every group header but the first row, more.
const ROW_GAP: f32 = 4.0;
const GROUP_GAP: f32 = 16.0;
/// Above the list, under the top band (the title isn't crowded by the first group).
const LIST_TOP: f32 = 10.0;
/// Above and below a row's text: `ui_tokens`' row heights are its lines plus twice this.
#[cfg(test)]
const ROW_PAD_Y: f32 = 7.0;
/// A row's (and a header's) corners, as the panes' cards.
const ROW_RADIUS: f32 = 8.0;
/// A group's card (2026-11-10, your call: "make it cool so I can tell the workspaces are
/// grouped"): the room inside it around its header and rows, and its corners.
const CARD_PAD: f32 = 4.0;
const CARD_RADIUS: f32 = 10.0;
/// The slot a status mark (or a header's chevron) sits in; a waiting mark fills it.
pub(crate) const MARK_SLOT: f32 = 14.0;
/// The space between the mark and the name, and before the age.
const SLOT_GAP: f32 = 8.0;
/// A row's name line, and the small line under it (M11i).
/// The branch and folder lines' glyphs (the 12 px box `tab_bar::glyph` draws in) and the space
/// after them (2026-11-10, your call: "add icon git and icon folder as well").
const PLACE_ICON: f32 = 12.0;
const PLACE_ICON_GAP: f32 = 4.0;
/// The branch button's pill: its side padding (it reaches that far left of the text column,
/// so the branch glyph stays in line with the folder's).
const BRANCH_CHIP_PAD: f32 = 4.0;
/// Between a row's name and the lines under it (2026-11-10, your call: "add more gaps between
/// title and git/folder").
const NAME_GAP: f32 = 4.0;
const NAME_LINE: f32 = 18.0;
const SUB_LINE: f32 = 16.0;
/// A header's + (a new workspace in the group), shown on hover.
const ADD_BUTTON: f32 = 18.0;
/// The footer under the rows; the resize strip along the right edge.
pub(crate) const FOOTER: f32 = 36.0;
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
/// A waiting row narrower than this leaves the question out ("Needs input" alone).
const QUESTION_MIN_ROW: f32 = 200.0;
/// A pinned row's mark, right of its name.
const PIN: &str = "📌";
/// A waiting row's line 2 starts with this (in the waiting colour), and a lost one's says
/// the other (M11i).
const NEEDS_INPUT: &str = "Needs input";
const NOT_LIVE: &str = "not live";
/// A row's × (a little bigger than the mark slot it sits on), and the rows' hover group.
const CLOSE_BUTTON: f32 = 16.0;
const ROW_GROUP: &str = "sidebar-row";
/// The + menu's width.
const MENU_WIDTH: f32 = 230.0;
/// The + menu's first items (the presets follow): label and shortcut.
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
    /// The footer's + menu is open.
    pub new_menu: bool,
    /// A row's tab menu is open (right-click).
    pub tab_menu: Option<TabMenu>,
    /// While a row is dragged: the slot of its run it would land in (`row_slot`). Only where
    /// the drop would move it; none: its gap is at its own place.
    pub drop_slot: Option<usize>,
    /// While an unpinned row is dragged where a drop would move it (`drop_place`): before
    /// which list item, into which group (none: out of every group), at which slot there.
    pub drop_at: Option<(usize, Option<GroupKey>, usize)>,
    /// The row being dragged and where it was taken, noted as its drag starts.
    pub drag_source: Rc<Cell<Option<RowGrab>>>,
    /// What each item of the list is, as last drawn: a workspace's row or a group's header.
    listed: Vec<Listed>,
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
            drop_slot: None,
            drop_at: None,
            drag_source: Rc::new(Cell::new(None)),
            listed: Vec::new(),
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

    /// Whether a drag's pointer is over a row drawn at `row`: inside it, and where the list
    /// shows it (a row partly scrolled under the band or the footer is cut there; its bounds
    /// aren't).
    fn pointer_on_row(&self, event: &MouseMoveEvent, row: Bounds<Pixels>) -> bool {
        row.contains(&event.position) && self.scroll.bounds().contains(&event.position)
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

/// A row's drag as it started: the workspace, and how far below the row's top the pointer took
/// it (GPUI's cursor offset).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct RowGrab {
    pub workspace: WorkspaceId,
    pub at: f32,
}

/// The row that follows the pointer during a drag: the row itself, lifted.
struct DragPreview {
    label: SharedString,
    /// Its place lines (`place_lines`): none, one, or its branch and its folder.
    place: Vec<SharedString>,
    status: Status,
    /// Idle, its mark is a dot (a program runs) or a ring.
    running: bool,
    width: f32,
    /// As tall as the row (`row_height`), so it fills the gap the row leaves and opens.
    height: f32,
}

impl Render for DragPreview {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ui = cx.global::<Appearance>().theme.ui();
        let mark = match self.status {
            Status::Idle => status_mark::idle_mark(self.running, &ui).into_any_element(),
            status => {
                let size = px(mark_size(status));
                status_mark::mark_on(status, &ui, 0.0, None, size, ui.row_active).into_any_element()
            }
        };
        let name_line = div()
            .flex()
            .items_center()
            .h(px(NAME_LINE))
            .child(mark_slot(mark))
            .child(
                div()
                    .flex_1()
                    .ml(px(SLOT_GAP))
                    .min_w(px(0.0))
                    .truncate()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(self.label.clone()),
            );
        let place = self.place.iter().cloned().map(|place| {
            div()
                .h(px(SUB_LINE))
                .pl(px(MARK_SLOT + SLOT_GAP))
                .text_size(px(text::META))
                .line_height(px(SUB_LINE))
                .text_color(hsla(ui.text_faint))
                .truncate()
                .child(place)
        });
        div()
            .flex()
            .flex_col()
            .justify_center()
            .w(px(self.width - 2.0 * ROW_INSET))
            .h(px(self.height))
            .px(px(ROW_PAD))
            .rounded(px(ROW_RADIUS))
            .opacity(0.95)
            .shadow_md()
            .bg(hsla(ui.row_active))
            .border_1()
            .border_color(hsla(ui.panel_border))
            .text_size(px(text::NAME))
            .text_color(hsla(ui.text))
            .child(name_line)
            .children(
                place
                    .enumerate()
                    .map(|(i, line)| line.when(i == 0, |l| l.mt(px(NAME_GAP)))),
            )
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
    /// The terminal a row's branch is read from: the workspace's first terminal (as
    /// `sidebar_rows` reads it), so the branch switcher opens for the one that was clicked.
    pub(crate) fn branch_terminal(&self, ws: WorkspaceId) -> Option<u64> {
        self.model.terminals(ws).first().copied()
    }

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
                    running: row_running(
                        s.map_or(Status::Idle, |s| s.status),
                        s.map(|s| s.id),
                        self.named_terminal(ws),
                        |id| matches!(self.programs.state(id), ProgramState::Running(_)),
                    ),
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
    /// sessions' most urgent; none with the sidebar hidden), each terminal tab in the panes'
    /// tab bars, and the status bar's terminal (the followed one, M11i).
    pub(crate) fn visible_statuses(&self) -> Vec<Status> {
        let followed = self
            .right_sidebar
            .followed
            .and_then(|id| self.sessions.items().iter().find(|s| s.id == id));
        let tabs = self
            .tab_sessions()
            .into_iter()
            .map(|i| self.sessions.items()[i].status)
            .chain(followed.map(|s| s.status));
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
                TabKind::Browser { .. } | TabKind::File { .. } => None,
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
            // (A tab shows its age only in its tooltip, read when it opens.) And the status
            // bar's terminal (M11i), unless idle (its word has no age then).
            let followed = self
                .right_sidebar
                .followed
                .and_then(|id| self.sessions.items().iter().find(|s| s.id == id))
                .filter(|s| s.status != Status::Idle)
                .map(|s| s.status_since);
            let sinces = rows
                .map(|i| self.sessions.items()[i].status_since)
                .chain(followed);
            // And the palette's workspace rows, while it's open.
            let palette = self
                .palette_shown(cx)
                .into_iter()
                .filter(|(status, _)| *status != Status::Idle)
                .map(|(_, since)| since);
            sinces
                .chain(palette)
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
        self.sidebar.listed = rows
            .iter()
            .map(|row| match row {
                SidebarRow::Workspace(s) => Listed::Row(s.workspace),
                SidebarRow::Header(h) => Listed::Header(h.key.clone()),
            })
            .collect();
        let targets = self.row_drag_offsets(&rows, cx);
        let headers: Vec<bool> = rows
            .iter()
            .map(|r| matches!(r, SidebarRow::Header(_)))
            .collect();
        // While a row is dragged to a gap: the cards as they will be after the drop.
        let dragging = self.dragged_row(cx).and_then(|ws| {
            rows.iter()
                .position(|r| matches!(r, SidebarRow::Workspace(s) if s.workspace == ws))
        });
        let parts = match (dragging, &self.sidebar.drop_at) {
            (Some(own), Some((at, _, _))) => drag_parts(&headers, own, *at),
            _ => card_parts(&headers),
        };
        let (now, animate) = (Instant::now(), !cx.reduce_motion());
        let elements: Vec<AnyElement> = rows
            .iter()
            .zip(targets)
            .enumerate()
            .map(|(i, (row, target))| match row {
                SidebarRow::Header(h) => {
                    let offset = self.drag_slides.borrow_mut().offset(
                        Slid::Header(group_id(&h.key)),
                        target,
                        now,
                        animate,
                    );
                    self.render_header(h, i == 0, offset, parts[i], ui, cx)
                }
                SidebarRow::Workspace(s) => {
                    let gap = gap_before(i.checked_sub(1).map(|p| &rows[p]), row);
                    let offset = self.drag_slides.borrow_mut().offset(
                        Slid::Row(s.workspace),
                        target,
                        now,
                        animate,
                    );
                    self.render_workspace_row(s, gap, offset, parts[i], ui, cx)
                }
            })
            .collect();
        // The band holds the app's title in its middle and the sidebar button at its right edge
        // (M11m): both ride the sidebar when it slides. In a window narrower than the sidebar
        // the button comes in to the window's edge (`open_button_at`), so it stays clickable.
        let label = div()
            .relative()
            .size_full()
            .children(self.render_app_title(ui, window))
            .child(self.render_sidebar_button(
                ui,
                open_button_at(self.sidebar.width, window.viewport_size().width.as_f32()),
                cx,
            ))
            .into_any_element();
        let band = self.render_top_band("sidebar-band", Some(label), window, cx);
        let list = div()
            .id("sidebar-rows")
            .flex()
            .flex_col()
            .flex_1()
            .min_h(px(0.0))
            .gap(px(ROW_GAP))
            .pt(px(LIST_TOP))
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
            // On the canvas, no line at its right (M11i): the gap to the cards does that job.
            .bg(hsla(ui.sidebar_background))
            // A click in the sidebar gives focus back to the terminal, which cancels a rename
            // (the rename box keeps its own clicks).
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _: &MouseDownEvent, window, cx| this.focus_active(window, cx)),
            )
            // A dragged row's gap follows it over the list, and a drop anywhere on the sidebar
            // (its title band too: the top of the list) puts it where the gap is (2026-11-10).
            .on_drag_move(
                cx.listener(|this, event: &DragMoveEvent<DraggedWorkspace>, _, cx| {
                    this.drag_row_over_list(event, cx)
                }),
            )
            .on_drop(cx.listener(|this, dragged: &DraggedWorkspace, _, cx| {
                this.drop_workspace(dragged.workspace, cx)
            }))
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
        offset: f32,
        part: CardPart,
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
            .mx(px(CARD_PAD))
            .px(px(ROW_PAD - CARD_PAD))
            .rounded(px(ROW_RADIUS))
            .cursor_pointer()
            .hover(|style| style.bg(hsla(ui.card_hover)))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.toggle_group(&key, cx)))
            .child(chevron)
            // What the card holds: a folder (open while the group is), or a host's globe.
            .child(div().flex_none().ml(px(PLACE_ICON_GAP)).child(glyph(
                match (&header.key, header.collapsed) {
                    (GroupKey::Host(_), _) => Glyph::Globe,
                    (GroupKey::Folder(_), true) => Glyph::Folder,
                    (GroupKey::Folder(_), false) => Glyph::FolderOpen,
                },
                ui.text_dim,
            )))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .ml(px(PLACE_ICON_GAP + 2.0))
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
            // Its card's top (and, closed, all of it); while a row is dragged past it into another
            // group, it slides with the rows.
            .child(
                div()
                    .relative()
                    .mx(px(ROW_INSET))
                    .top(px(offset))
                    .children(card_band(part, 0.0, &ui))
                    .child(header_row),
            )
            .into_any_element()
    }

    /// A workspace's row (M11i: compact): its most urgent terminal's mark (idle: a dot while a
    /// program runs, else a ring; a × in its place on hover), its name in semibold (or the
    /// rename box), age (when the row is wide enough), 📌 when pinned, on-screen pip, colour dot
    /// and, on hover, its ⌘N shortcut; under them one line (`line_two`): "not live", "Needs
    /// input · question", a script's note, or where it is (`branch • ~/folder`); a script's
    /// progress along the bottom. The selected row is a soft tint of the accent with a 2 pt
    /// accent bar at its left, its text and marks in their own colours. Click selects,
    /// double-click renames, the × or a middle-click closes, a right-click opens its menu; it
    /// can be dragged within its run (`sidebar_run`).
    fn render_workspace_row(
        &self,
        row: &WorkspaceRow,
        gap: bool,
        offset: f32,
        part: CardPart,
        ui: UiColors,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let ws = row.workspace;
        let session = row.subject.and_then(|i| self.sessions.items().get(i));
        let waiting = row.status == Status::Waiting;
        // The selected row is a soft tint of the accent (waiting or not); an unselected waiting
        // one keeps its waiting tint. Hover deepens an unselected one a little; the selected
        // one stays as it is.
        // In a group's card (2026-11-10): inset in it, on its colour, with the card's own hover
        // and selection.
        let in_card = part != CardPart::Out;
        let inset = if in_card { CARD_PAD } else { 0.0 };
        let fill = if row.active {
            Some(if in_card {
                ui.card_selected
            } else {
                ui.row_selected
            })
        } else if waiting {
            Some(ui.waiting_tint)
        } else {
            None
        };
        let hover = match fill {
            _ if row.active => None,
            Some(_) => Some(ui.waiting_hover),
            None => Some(if in_card { ui.card_hover } else { ui.row_hover }),
        };
        let surface = fill.unwrap_or(if in_card {
            ui.card
        } else {
            ui.sidebar_background
        });
        let rename_box = self
            .renaming
            .as_ref()
            .filter(|(r, _)| *r == Renaming::Workspace(ws))
            .map(|(_, input)| input.clone());
        let run = self.sidebar_run(ws);
        // While it's dragged, its place shows empty: its copy is it.
        let lifted = self.dragged_row(cx) == Some(ws);
        // A pane tab dragged over it: dropping moves the tab into this workspace.
        let tab_target =
            self.dragged_tab(cx).is_some() && self.tab_drop == Some(TabDrop::Workspace(ws));

        let name: AnyElement = match rename_box {
            Some(input) => input.into_any_element(),
            None => div()
                .flex_1()
                .min_w(px(0.0))
                .truncate()
                .font_weight(FontWeight::SEMIBOLD)
                .child(row.label.clone())
                .into_any_element(),
        };
        let meta = |child: AnyElement| {
            div()
                .flex_none()
                .ml(px(META_GAP))
                .text_size(px(text::META))
                .font_features(tabular())
                .text_color(hsla(ui.text_faint))
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
            .child(self.close_slot(ws, session, row.running, ui, surface, cx))
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
                line.child(color_dot(color, ui).ml(px(META_GAP)))
            })
            // The shortcut only while the row is hovered, to keep the rows quiet (its room is
            // kept, so the name doesn't shift); ⌘1–⌘9 work either way.
            .when_some(shortcut_label(row.index), |line, keys| {
                line.child(
                    meta(keys.into_any_element())
                        .invisible()
                        .group_hover(ROW_GROUP, |s| s.visible()),
                )
            });
        // Under the name, level with it: one line, the first that applies (`line_two`).
        let under = || {
            div()
                .flex()
                .items_center()
                .h(px(SUB_LINE))
                .pl(px(MARK_SLOT + SLOT_GAP))
                .text_size(px(text::META))
                .line_height(px(SUB_LINE))
        };
        let text = |line: String, color: Rgb| {
            div()
                .flex_1()
                .min_w(px(0.0))
                .truncate()
                .text_color(hsla(color))
                .child(line)
        };
        let row_width = self.sidebar.width - 2.0 * (ROW_INSET + inset);
        let second = line_two(row).map(|line| match line {
            LineTwo::NotLive => under().child(text(NOT_LIVE.into(), ui.text_faint)),
            // "Needs input" in the waiting colour, then the question cut at its end (left out
            // where the row is too narrow: it's in ⌘P and the notification).
            LineTwo::Waiting(question) => under()
                .child(
                    div()
                        .flex_none()
                        .text_color(hsla(ui.ring_waiting))
                        .child(NEEDS_INPUT),
                )
                .when_some(question.filter(|_| shows_question(row_width)), |line, q| {
                    line.child(text(
                        format!("{}{q}", crate::status_bar::SEPARATOR),
                        ui.text_dim,
                    ))
                }),
            LineTwo::Note(note) => under().child(text(note, ui.text_faint)),
            // The branch on its own line, cut at its end when long; the folder under it, short
            // of room losing its start (its own name stays): `fit_place`, measured as drawn.
            LineTwo::Place => {
                // The folder line's text starts after its glyph.
                let room =
                    row_width - 2.0 * ROW_PAD - MARK_SLOT - SLOT_GAP - PLACE_ICON - PLACE_ICON_GAP;
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
                // The branch reads as a button: the branch glyph (as in the status bar) and the
                // name in `text_dim` (the folder is `text_faint`); on hover a pill and `text`.
                // Only the glyph and the name take the click, not the rest of the line; a name
                // too long for the row is cut and then the button spans the line (2026-11-10).
                // Clicking it opens the branch switcher (not a row click or a drag).
                let branch = row.branch.clone().map(|b| {
                    under().child(
                        div()
                            .id(("row-branch", ws.0 as usize))
                            .flex()
                            .items_center()
                            .gap(px(PLACE_ICON_GAP))
                            .min_w(px(0.0))
                            .h(px(SUB_LINE))
                            .px(px(BRANCH_CHIP_PAD))
                            .ml(px(-BRANCH_CHIP_PAD))
                            .rounded(px(4.0))
                            .overflow_hidden()
                            .text_color(hsla(ui.text_dim))
                            .cursor_pointer()
                            .hover(move |s| s.bg(hsla(ui.row_active)).text_color(hsla(ui.text)))
                            .on_mouse_down(MouseButton::Left, |_: &MouseDownEvent, _, cx| {
                                cx.stop_propagation()
                            })
                            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                                cx.stop_propagation();
                                if let Some(terminal) = this.branch_terminal(ws) {
                                    this.open_branch_switcher(terminal, window, cx);
                                }
                            }))
                            .child(glyph(Glyph::Branch, ui.text_dim))
                            .child(div().min_w(px(0.0)).truncate().child(b)),
                    )
                });
                // The folder: a folder glyph, then the path (not a button).
                let folder = row.folder.as_deref().map(|f| {
                    under()
                        .gap(px(PLACE_ICON_GAP))
                        .child(glyph(Glyph::Folder, ui.text_faint))
                        .child(text(fit_place(None, Some(f), fits), ui.text_faint))
                });
                div().flex().flex_col().children(branch).children(folder)
            }
        });
        let (source, tab_source) = (self.sidebar.drag_source.clone(), self.tab_drag.clone());
        let preview = DragPreview {
            label: row.label.clone().into(),
            place: place_lines(row).into_iter().map(Into::into).collect(),
            status: row.status,
            running: row.running,
            width: self.sidebar.width,
            height: row_height(row),
        };
        // The program's own title, under the name, when it says more (M11i).
        let tip = self.workspace_tip(ws);
        let row_el = div()
            .id(("workspace", ws.0 as usize))
            .group(ROW_GROUP)
            .absolute()
            .top(px(offset))
            .left(px(inset))
            .right(px(inset))
            .h_full()
            .flex()
            .flex_col()
            .justify_center()
            .px(px(ROW_PAD))
            .rounded(px(ROW_RADIUS))
            .text_size(px(text::NAME))
            .text_color(hsla(ui.text))
            .when_some(fill, |row, fill| row.bg(hsla(fill)))
            .when_some(hover, |row, hover| {
                row.hover(move |style| style.bg(hsla(hover)))
            })
            .when(lifted, |row| row.opacity(0.0))
            .when_some(tip, |row, tip| {
                row.tooltip(move |_, cx| -> AnyView { cx.new(|_| Tip(tip.clone().into())).into() })
            })
            // During any drag the closed hand shows instead (`hold_closed_hand`).
            .cursor_pointer()
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
                move |dragged, at, window, cx| {
                    source.set(Some(RowGrab {
                        workspace: dragged.workspace,
                        at: f32::from(at.y),
                    }));
                    tab_source.set(None); // not a tab's drag any more
                    hold_closed_hand(window, cx);
                    cx.new(|_| DragPreview {
                        label: preview.label.clone(),
                        place: preview.place.clone(),
                        ..preview
                    })
                },
            )
            // Every row takes a pane tab (moved into its workspace, not a reorder); a dragged row
            // is the list's (`drag_row_over_list`).
            .on_drag_move(
                cx.listener(move |this, event: &DragMoveEvent<DraggedTab>, _, cx| {
                    this.drag_tab_over_row(ws, event, cx)
                }),
            )
            .on_drop(cx.listener(move |this, dragged: &DraggedTab, window, cx| {
                let here = |at| at == TabDrop::Workspace(ws);
                this.drop_tab(dragged.tab, here, window, cx);
            }))
            .child(name_line)
            .children(second.map(|lines| lines.mt(px(NAME_GAP))))
            // The progress bar: along the bottom edge (a compact row has 1 pt under its line 2),
            // as wide as the percent.
            .when_some(row.progress, |row_el, percent| {
                row_el.child(
                    div()
                        .absolute()
                        .bottom_0()
                        .left(px(ROW_PAD))
                        .right(px(ROW_PAD))
                        .h(px(2.0))
                        .child(
                            div()
                                .h_full()
                                .w(relative(f32::from(percent.min(100)) / 100.0))
                                .rounded(px(1.0))
                                .bg(hsla(ui.ring_working)),
                        ),
                )
            })
            // A pane tab over it: lit as on hover (GPUI draws no hover during a drag) and ringed
            // in the accent.
            .when(tab_target, |row_el| {
                let ring = ui.accent;
                row_el.when_some(hover, |r, hover| r.bg(hsla(hover))).child(
                    div()
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full()
                        .rounded(px(ROW_RADIUS))
                        .border(px(DIVIDER_ACTIVE))
                        .border_color(hsla(ring)),
                )
            });
        // Its place in the list: the room the list gives it, where the row is drawn from.
        div()
            .flex_none()
            .relative()
            .h(px(row_height(row)))
            // Under the pinned rows, a group's gap sets the rest apart.
            .when(gap, |place| place.mt(px(GROUP_GAP - ROW_GAP)))
            .mx(px(ROW_INSET))
            // Its group's card behind it, sliding with it.
            .children(card_band(part, offset, &ui))
            .child(row_el)
            .into_any_element()
    }

    /// While a row is dragged: how far each item of the list `rows` is drawn from its place
    /// (`row_offsets`), its gap at the slot it would land in, or at its own place.
    fn row_drag_offsets(&self, rows: &[SidebarRow], cx: &App) -> Vec<f32> {
        let dragged = self.dragged_row(cx).and_then(|ws| {
            rows.iter().enumerate().find_map(|(i, row)| match row {
                SidebarRow::Workspace(s) if s.workspace == ws => Some((i, s)),
                _ => None,
            })
        });
        let Some((own, dragged)) = dragged else {
            return vec![0.0; rows.len()];
        };
        // An unpinned row: everything between its place and where it lands slides, headers too.
        // The dragged row itself (unseen) and its card go to the gap (`gap_offset`).
        if let Some((at, _, _)) = &self.sidebar.drop_at {
            let size = row_height(dragged) + ROW_GAP;
            let mut offsets = cross_offsets(rows.len(), own, *at, size);
            let spans: Option<Vec<(f32, f32)>> = (0..rows.len())
                .map(|i| {
                    let b = self.sidebar.scroll.bounds_for_item(i)?;
                    Some((f32::from(b.top()), f32::from(b.bottom())))
                })
                .collect();
            if let Some(spans) = spans {
                offsets[own] = gap_offset(&spans, own, *at, size);
            }
            return offsets;
        }
        let run = self.sidebar_run(dragged.workspace);
        let in_run: Vec<bool> = rows
            .iter()
            .map(|row| {
                matches!(row, SidebarRow::Workspace(s) if self.sidebar_run(s.workspace) == run)
            })
            .collect();
        row_offsets(&in_run, own, self.sidebar.drop_slot, row_height(dragged))
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
        self.session_mark_sized(place, session, ui, fill, mark_size(session.status))
    }

    /// `session_mark` in a `size` box (the status bar's is smaller, M11i).
    pub(crate) fn session_mark_sized(
        &self,
        place: &'static str,
        session: &Session,
        ui: UiColors,
        fill: Rgb,
        size: f32,
    ) -> AnyElement {
        let size = px(size);
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

    /// A row's mark slot: its session's status mark (idle, or a workspace with no terminal:
    /// a dot while a program runs there, `running`, else a ring), or while the row is hovered
    /// a × in its place that closes the workspace (asking first when it has 2+ terminals).
    fn close_slot(
        &self,
        ws: WorkspaceId,
        session: Option<&Session>,
        running: bool,
        ui: UiColors,
        surface: Rgb,
        cx: &mut Context<Self>,
    ) -> Div {
        let mark = match session {
            Some(session) if session.status != Status::Idle => {
                self.session_mark("row", session, ui, surface)
            }
            _ => status_mark::idle_mark(running, &ui).into_any_element(),
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

    /// The pointer moved during a row drag: while it's over the list and the row's copy is over
    /// its run, it lands in the gap nearest the copy's centre among the run's other rows as laid
    /// out with it taken out (`row_slot`); elsewhere, its gap goes back to its own place.
    fn drag_row_over_list(
        &mut self,
        event: &DragMoveEvent<DraggedWorkspace>,
        cx: &mut Context<Self>,
    ) {
        let dragged = event.drag(cx);
        let (ws, run) = (dragged.workspace, dragged.run.clone());
        let grab = self.sidebar.drag_source.get().filter(|g| g.workspace == ws);
        let pointer = event.event.position;
        let bounds = self.sidebar.scroll.bounds();
        let over = bounds.contains(&pointer);
        // Above the list (the title band, over the sidebar): the very top of it.
        let above =
            pointer.y < bounds.top() && pointer.x >= bounds.left() && pointer.x <= bounds.right();
        let scroll = &self.sidebar.scroll;
        let dy = f32::from(scroll.offset().y);
        let span = |i: usize| {
            scroll
                .bounds_for_item(i)
                .map(|b| (f32::from(b.top()) + dy, f32::from(b.bottom()) + dy))
        };
        if !run.0 {
            // Unpinned: anywhere among the unpinned rows, into any group or out of every one
            // (2026-11-10, your call).
            let next = grab.filter(|_| over || above).and_then(|grab| {
                let (mut items, mut spans, mut own) = (Vec::new(), Vec::new(), None);
                for (i, item) in self.sidebar.listed.iter().enumerate() {
                    spans.push(span(i)?);
                    items.push(match item {
                        Listed::Header(key) => ListItem::Header(key.clone()),
                        Listed::Row(w) => {
                            if *w == ws {
                                own = Some(i);
                            }
                            if self.model.workspace(*w).is_some_and(|w| w.pinned) {
                                ListItem::Pinned
                            } else {
                                ListItem::Row
                            }
                        }
                    });
                }
                let own = own?;
                let height = spans[own].1 - spans[own].0;
                let centre = if above {
                    f32::MIN
                } else {
                    f32::from(pointer.y) - grab.at + height / 2.0
                };
                drop_place(&items, &spans, own, centre)
            });
            if self.sidebar.drop_at != next {
                self.sidebar.drop_at = next;
                cx.notify();
            }
            return;
        }
        // Pinned: within the pinned rows.
        let next = grab.filter(|_| over).and_then(|grab| {
            let (mut own, mut spans) = (None, Vec::new());
            for (i, item) in self.sidebar.listed.iter().enumerate() {
                let Listed::Row(w) = *item else { continue };
                if self.sidebar_run(w) != run {
                    continue;
                }
                if w == ws {
                    own = Some(spans.len());
                }
                spans.push(span(i)?);
            }
            let own = own?;
            let height = spans[own].1 - spans[own].0;
            let centre = f32::from(pointer.y) - grab.at + height / 2.0;
            let slot = row_slot(centre, &spans, own, height + ROW_GAP)?;
            (slot != own).then_some(slot)
        });
        if self.sidebar.drop_slot != next {
            self.sidebar.drop_slot = next;
            cx.notify();
        }
    }

    /// The pointer moved during a pane tab's drag: if it's over this row (`ws`) and dropping there
    /// would move the tab (`Model::drop_place`), the row is lit.
    fn drag_tab_over_row(
        &mut self,
        ws: WorkspaceId,
        event: &DragMoveEvent<DraggedTab>,
        cx: &mut Context<Self>,
    ) {
        let over = self.sidebar.pointer_on_row(&event.event, event.bounds);
        let at = TabDrop::Workspace(ws);
        if !over && self.tab_drop != Some(at) {
            return; // not this row's business
        }
        let next = Some(at)
            .filter(|_| over)
            .filter(|at| self.model.drop_place(event.drag(cx).tab, *at).is_some());
        if self.tab_drop != next {
            self.tab_drop = next;
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

    /// The footer: a + (a menu of ways to start a session), then, at the right, the 🔔 (this
    /// run's notifications, with a dot while some are unseen) beside a gear for Preferences.
    fn render_footer(&self, ui: UiColors, cx: &mut Context<Self>) -> AnyElement {
        let open = self.sidebar.new_menu;
        let popover = self.notifications_popover.is_some();
        let unseen = self.notifications.unseen() > 0;
        let button = |id: &'static str| {
            div()
                .id(id)
                .flex()
                .items_center()
                .justify_center()
                .w(px(FOOTER_BUTTON))
                .h(px(FOOTER_BUTTON))
                .rounded(px(6.0))
                .cursor_pointer()
                .hover(|style| style.bg(hsla(ui.row_hover)).text_color(hsla(ui.text)))
        };
        div()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(FOOTER_GAP))
            .h(px(FOOTER))
            .px(px(ROW_INSET))
            .border_t_1()
            .border_color(hsla(ui.sidebar_border))
            .child(
                button("sidebar-new")
                    .group("sidebar-new")
                    .when(open, |b| b.bg(hsla(ui.row_hover)))
                    .child(footer_icon(Glyph::Plus, ui, "sidebar-new", open))
                    .tooltip(|_, cx| -> AnyView { cx.new(|_| Tip("New\u{2026}".into())).into() })
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.sidebar.new_menu = !this.sidebar.new_menu;
                        cx.notify();
                    })),
            )
            .child(div().flex_1())
            .child(
                button("sidebar-notifications")
                    .group(BELL_GROUP)
                    .relative()
                    .when(popover, |b| b.bg(hsla(ui.row_hover)))
                    .child(footer_icon(Glyph::Bell, ui, BELL_GROUP, popover))
                    .when(unseen, |b| b.child(unseen_dot(ui)))
                    .tooltip(|_, cx| -> AnyView {
                        cx.new(|_| Tip(NOTIFICATIONS_TIP.into())).into()
                    })
                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                        this.open_notifications(window, cx)
                    })),
            )
            .child(
                button("sidebar-preferences")
                    .group(PREFERENCES_GROUP)
                    .child(footer_icon(Glyph::Gear, ui, PREFERENCES_GROUP, false))
                    .tooltip(|_, cx| -> AnyView { cx.new(|_| Tip(PREFERENCES_TIP.into())).into() })
                    .on_click(|_: &ClickEvent, _, cx| preferences::open_or_activate(cx)),
            )
            .into_any_element()
    }

    /// The + menu, over the window above the footer: a new workspace (here, or in a
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
            // Above the footer, which sits on the status bar (M11i).
            .bottom(px(STATUS_BAR + FOOTER + 4.0))
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

    /// Help → Show Tips Card: the tips card back and remembered (`show_hint`), and the sidebar
    /// it lives in shown if it was hidden (as ⌘\ would).
    pub(crate) fn show_welcome_tips(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.sidebar.hidden {
            self.toggle_sidebar(window, cx);
        }
        self.show_hint(cx);
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
    pub(crate) fn toggle_group(&mut self, key: &GroupKey, cx: &mut Context<Self>) {
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
            running: false,
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
        assert_eq!(session(&rows[0]).sub, Some(Sub::NotLive));
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
    fn while_dragging_the_cards_are_drawn_as_they_will_be_after_the_drop() {
        use CardPart::{Alone, Bottom, Middle, Out, Top};
        // 0 an ungrouped row, 1 header a, 2 a row, 3 the dragged row, 4 header b, 5 a row.
        let headers = [false, true, false, false, true, false];
        // Out to the top: a is left with one row; the gap (3) is out of every card.
        assert_eq!(
            drag_parts(&headers, 3, 0),
            [Out, Top, Bottom, Out, Top, Bottom]
        );
        // Into b, before its row: the gap is b's first row; a ends at its row.
        assert_eq!(
            drag_parts(&headers, 3, 5),
            [Out, Top, Bottom, Middle, Top, Bottom]
        );
        // To b's end: the gap is b's last row.
        assert_eq!(
            drag_parts(&headers, 3, 6),
            [Out, Top, Bottom, Bottom, Top, Middle]
        );
        // A group's only row dragged to the end: its header is alone; the gap ends the next.
        assert_eq!(
            drag_parts(&[true, false, true, false], 1, 4),
            [Alone, Bottom, Top, Middle]
        );
        // The only row of the only group, out to the top: the header alone, the gap out.
        assert_eq!(drag_parts(&[true, false], 1, 0), [Alone, Out]);
    }

    #[test]
    fn the_dragged_row_is_drawn_at_its_gap() {
        // Rows 40 tall with 4 between: tops at 0, 44, 88, 132.
        let spans = [(0.0, 40.0), (44.0, 84.0), (88.0, 128.0), (132.0, 172.0)];
        // Up, before item 0: where item 0 was.
        assert_eq!(gap_offset(&spans, 2, 0, 44.0), -88.0);
        // Down, before item 3: the items between closed up by its size.
        assert_eq!(gap_offset(&spans, 0, 3, 44.0), 88.0);
        // To the end: after the last one.
        assert_eq!(gap_offset(&spans, 0, 4, 44.0), 132.0);
        // Where it is: no move.
        assert_eq!(gap_offset(&spans, 1, 1, 44.0), 0.0);
    }

    #[test]
    fn a_group_s_card_runs_from_its_header_to_its_last_row() {
        use CardPart::{Alone, Bottom, Middle, Out, Top};
        // Two ungrouped rows, a group of three, a closed group, a group of one.
        let headers = [false, false, true, false, false, false, true, true, false];
        assert_eq!(
            card_parts(&headers),
            [Out, Out, Top, Middle, Middle, Bottom, Alone, Top, Bottom]
        );
        assert_eq!(card_parts(&[]), []);
        assert_eq!(card_parts(&[true]), [Alone]);
        assert_eq!(card_parts(&[false]), [Out]);
    }

    #[test]
    fn a_row_dragged_into_another_run_slides_everything_between() {
        // Items: 0 an ungrouped row, 1 a header, 2 a row, 3 the dragged row, 4 a row.
        // Up, before item 0: items 0–2 slide down by its size; it and item 4 stay.
        assert_eq!(cross_offsets(5, 3, 0, 40.0), [40.0, 40.0, 40.0, 0.0, 0.0]);
        // Up, before item 1 (after the ungrouped row): only the header and row 2.
        assert_eq!(cross_offsets(5, 3, 1, 40.0), [0.0, 40.0, 40.0, 0.0, 0.0]);
        // Down from item 0, before item 4: items 1–3 slide up; at the end, item 4 too.
        assert_eq!(
            cross_offsets(5, 0, 4, 40.0),
            [0.0, -40.0, -40.0, -40.0, 0.0]
        );
        assert_eq!(
            cross_offsets(5, 0, 5, 40.0),
            [0.0, -40.0, -40.0, -40.0, -40.0]
        );
    }

    #[test]
    fn a_row_lands_in_the_group_of_the_header_above_where_it_is_dropped() {
        use ListItem::{Header, Pinned, Row};
        let (a, b) = (folder("/w/a"), folder("/w/b"));
        // 0 pinned, 1 an ungrouped row, 2 header a, 3 a row, 4 the dragged row, 5 header b,
        // 6 a row; each 40 tall from the top (middles at 20, 60, 100…).
        let items = [
            Pinned,
            Row,
            Header(a.clone()),
            Row,
            Row,
            Header(b.clone()),
            Row,
        ];
        let spans: Vec<(f32, f32)> = (0..7)
            .map(|i| (i as f32 * 40.0, i as f32 * 40.0 + 40.0))
            .collect();
        let at = |centre| drop_place(&items, &spans, 4, centre);
        // Above everything (or the title band): out of every group, first; never among the
        // pinned rows.
        assert_eq!(at(f32::MIN), Some((1, None, 0)));
        assert_eq!(at(10.0), Some((1, None, 0)));
        // Above header a: out of every group, after the ungrouped row.
        assert_eq!(at(70.0), Some((2, None, 1)));
        // Under header a, before its row: first in a.
        assert_eq!(at(110.0), Some((3, Some(a.clone()), 0)));
        // Where it is (second in a): stays.
        assert_eq!(at(150.0), None);
        // Under header b: into another folder's group.
        assert_eq!(at(250.0), Some((6, Some(b.clone()), 0)));
        assert_eq!(at(1000.0), Some((7, Some(b), 1)));
    }

    #[test]
    fn a_dragged_row_opens_a_gap_in_its_run_only() {
        // A header, then a run of four rows (the second, 57 px tall, dragged), then another
        // header and a row of another run.
        let run = [false, true, true, true, true, false, false];
        let shift = 57.0 + ROW_GAP;
        // At its own place (or no slot yet): nothing moves.
        assert_eq!(row_offsets(&run, 2, Some(1), 57.0), [0.0; 7]);
        assert_eq!(row_offsets(&run, 2, None, 57.0), [0.0; 7]);
        // To the top of the run: the row above slides down by its height and the gap.
        assert_eq!(
            row_offsets(&run, 2, Some(0), 57.0),
            [0.0, shift, 0.0, 0.0, 0.0, 0.0, 0.0]
        );
        // To the end: the two below slide up; the header and the other run stay.
        assert_eq!(
            row_offsets(&run, 2, Some(3), 57.0),
            [0.0, 0.0, 0.0, -shift, -shift, 0.0, 0.0]
        );
        // A shorter row opens a shorter gap: its own height, not its neighbours'.
        let short = 36.0 + ROW_GAP;
        assert_eq!(
            row_offsets(&run, 2, Some(2), 36.0),
            [0.0, 0.0, 0.0, -short, 0.0, 0.0, 0.0]
        );
    }

    #[test]
    fn a_dragged_row_lands_in_the_gap_nearest_its_copy_within_its_run() {
        // Three 40 px rows 2 px apart from y = 100; the middle one (42 px with its gap) dragged:
        // closed up, the others' centres are 120 and (the third, moved up) 162, passed half a
        // gap later (141, 183).
        let rows = [(100.0, 140.0), (142.0, 182.0), (184.0, 224.0)];
        let size = 42.0;
        assert_eq!(row_slot(110.0, &rows, 1, size), Some(0));
        assert_eq!(row_slot(140.0, &rows, 1, size), Some(0));
        assert_eq!(row_slot(142.0, &rows, 1, size), Some(1));
        assert_eq!(row_slot(182.0, &rows, 1, size), Some(1));
        assert_eq!(row_slot(184.0, &rows, 1, size), Some(2));
        // A shaky start: a few px up or down of its own centre (162) is its own place.
        for y in [155.0, 159.0, 162.0, 165.0, 169.0] {
            assert_eq!(row_slot(y, &rows, 1, size), Some(1), "{y}");
        }
        // Half the row's room past the run's ends still counts; further off, nowhere.
        assert_eq!(row_slot(100.0 - 20.0, &rows, 1, size), Some(0));
        assert_eq!(row_slot(224.0 + 20.0, &rows, 1, size), Some(2));
        assert_eq!(row_slot(100.0 - 22.0, &rows, 1, size), None);
        assert_eq!(row_slot(224.0 + 22.0, &rows, 1, size), None);
        assert_eq!(row_slot(f32::NAN, &rows, 1, size), None);
        assert_eq!(row_slot(120.0, &[], 0, size), None);
    }

    #[test]
    fn a_tall_row_s_copy_over_any_gap_of_its_run_lands_there() {
        // 40 px rows around a 56 px one (58 px with its gap), dragged.
        let rows = [
            (100.0, 140.0),
            (142.0, 198.0),
            (200.0, 240.0),
            (242.0, 282.0),
        ];
        let size = 58.0;
        // Each gap as it opens (closed up, the gap at the slot): its centre lands there.
        for (slot, centre) in [(0, 128.0), (1, 170.0), (2, 212.0), (3, 254.0)] {
            assert_eq!(row_slot(centre, &rows, 1, size), Some(slot), "{centre}");
        }
    }

    #[test]
    fn a_dropped_row_goes_exactly_where_its_gap_was() {
        use crate::layout_model::TabKind;
        // Two pinned workspaces, then four unpinned: every unpinned one to every slot of its run.
        let mut m = Model::new();
        let ids: Vec<WorkspaceId> = (0..6)
            .map(|i| {
                m.new_workspace(i, TabKind::Terminal(i as u64 + 1))
                    .unwrap()
                    .workspace
            })
            .collect();
        for id in &ids[..2] {
            m.set_pinned(*id, true);
        }
        let order = m.display_order();
        let in_run: Vec<bool> = order.iter().map(|id| !ids[..2].contains(id)).collect();
        let run: Vec<WorkspaceId> = order[2..].to_vec();
        for (at, &dragged) in run.iter().enumerate() {
            for slot in 0..run.len() {
                let mut moved = m.clone();
                let to = run_drop_index(&in_run, slot).unwrap();
                assert!(moved.move_workspace_display(dragged, to) || slot == at);
                let after = moved.display_order();
                assert_eq!(after[..2], order[..2], "{at} to {slot}: the pinned stay");
                assert_eq!(after[2 + slot], dragged, "{at} to {slot}");
                let others = |o: &[WorkspaceId]| -> Vec<WorkspaceId> {
                    o.iter().copied().filter(|w| *w != dragged).collect()
                };
                assert_eq!(others(&after), others(&order), "{at} to {slot}");
            }
        }
        // The pinned run: from its start.
        let pinned: Vec<bool> = in_run.iter().map(|r| !r).collect();
        assert_eq!(run_drop_index(&pinned, 1), Some(1));
        assert_eq!(run_drop_index(&[false, false], 0), None);
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
    fn a_row_grows_to_two_lines_for_where_it_is_or_what_it_waits_on() {
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
        // Whatever is under the name, it's one line (the waiting one was 56 pt with both).
        assert_eq!(
            heights,
            [
                SIDEBAR_ROW,
                SIDEBAR_ROW_TWO,
                SIDEBAR_ROW_TWO,
                SIDEBAR_ROW_TWO
            ]
        );
    }

    #[test]
    fn a_row_in_a_repo_shows_its_branch_and_its_folder_on_their_own_lines() {
        assert_eq!(
            SIDEBAR_ROW_THREE, 68.0,
            "18, 4 under the name, 16 + 16, and 7 above and below"
        );
        let now = Instant::now();
        let under = |f: WorkspaceFacts| {
            let rows = rows(&numbered(vec![f]), &[], None, &[], now);
            let row = session(&rows[0]);
            (lines_under(row), row_height(row))
        };
        // In a repo: the branch, then the folder under it.
        let mut both = facts("api", Status::Idle, now, None);
        both.folder = Some("~/api".into());
        both.branch = Some("main");
        assert_eq!(under(both), (2, SIDEBAR_ROW_THREE));
        // Not in a repo: no branch line, the folder alone.
        let mut folder_only = facts("api", Status::Idle, now, None);
        folder_only.folder = Some("/tmp/scratch".into());
        assert_eq!(under(folder_only), (1, SIDEBAR_ROW_TWO));
        let mut branch_only = facts("api", Status::Idle, now, None);
        branch_only.branch = Some("main");
        assert_eq!(under(branch_only), (1, SIDEBAR_ROW_TWO));
        // Waiting, a note or lost: that one line, as before (the place is not shown).
        let mut waiting = facts("claude", Status::Waiting, now, None);
        waiting.folder = Some("~/api".into());
        waiting.branch = Some("main");
        assert_eq!(under(waiting), (1, SIDEBAR_ROW_TWO));
        assert_eq!(
            under(facts("shell", Status::Idle, now, None)),
            (0, SIDEBAR_ROW)
        );
    }

    #[test]
    fn a_dragged_row_shows_the_lines_the_row_has() {
        let now = Instant::now();
        let lines = |f: WorkspaceFacts| {
            let rows = rows(&numbered(vec![f]), &[], None, &[], now);
            place_lines(session(&rows[0]))
        };
        let mut both = facts("api", Status::Idle, now, None);
        both.folder = Some("~/api".into());
        both.branch = Some("main");
        assert_eq!(lines(both), ["main", "~/api"]);
        let mut folder_only = facts("api", Status::Idle, now, None);
        folder_only.folder = Some("/tmp/scratch".into());
        assert_eq!(lines(folder_only), ["/tmp/scratch"]);
        // A waiting row is two lines tall: its place on one, as before.
        let mut waiting = facts("claude", Status::Waiting, now, None);
        waiting.folder = Some("~/api".into());
        waiting.branch = Some("main");
        assert_eq!(lines(waiting), ["main • ~/api"]);
        assert!(lines(facts("shell", Status::Idle, now, None)).is_empty());
    }

    #[test]
    fn a_row_is_32_with_one_line_and_48_with_two_with_room_around_its_text() {
        assert_eq!((SIDEBAR_ROW, SIDEBAR_ROW_TWO), (32.0, 52.0));
        assert_eq!(NAME_GAP, 4.0);
        assert_eq!((NAME_LINE, SUB_LINE), (18.0, 16.0));
        // 7 above and 7 below the text, whatever the row holds.
        assert_eq!(SIDEBAR_ROW - NAME_LINE, 2.0 * ROW_PAD_Y);
        assert_eq!(
            SIDEBAR_ROW_TWO - NAME_LINE - NAME_GAP - SUB_LINE,
            2.0 * ROW_PAD_Y
        );
        assert_eq!(
            SIDEBAR_ROW_THREE - NAME_LINE - NAME_GAP - 2.0 * SUB_LINE,
            2.0 * ROW_PAD_Y
        );
        let now = Instant::now();
        let height = |f: WorkspaceFacts| {
            let rows = rows(&numbered(vec![f]), &[], None, &[], now);
            row_height(session(&rows[0]))
        };
        // No place, nothing under the name: one line.
        assert_eq!(height(facts("shell", Status::Idle, now, None)), 32.0);
        // Its place only.
        let mut placed = facts("api", Status::Idle, now, None);
        placed.folder = Some("~/api".into());
        assert_eq!(height(placed), 52.0);
        // Waiting, with a place and a question: still two lines.
        let mut waiting = facts("claude", Status::Waiting, now, None);
        waiting.folder = Some("~/api".into());
        waiting.branch = Some("main");
        waiting.question = Some("Proceed? (y/n)");
        assert_eq!(height(waiting), 52.0);
        // Lost.
        let mut lost = facts("api", Status::Idle, now, None);
        lost.lost = true;
        assert_eq!(height(lost), 52.0);
    }

    #[test]
    fn the_list_has_room_under_the_title_between_rows_and_above_groups() {
        assert_eq!(LIST_TOP, 10.0);
        assert_eq!(ROW_GAP, 4.0);
        assert_eq!(GROUP_GAP, 16.0);
        assert_eq!(GROUP_HEADER, 28.0);
        assert_eq!(ROW_RADIUS, 8.0);
    }

    #[test]
    fn line_two_prefers_not_live_then_waiting_then_note_then_place() {
        let now = Instant::now();
        let line = |f: WorkspaceFacts| {
            let rows = rows(&numbered(vec![f]), &[], None, &[], now);
            line_two(session(&rows[0]))
        };
        let base = |status| {
            let mut f = facts("api", status, now, None);
            f.folder = Some("~/api".into());
            f.branch = Some("main");
            f.note = Some("3 of 7");
            f.question = Some("Proceed? (y/n)");
            f
        };
        // Lost beats everything, waiting included.
        let mut lost = base(Status::Waiting);
        lost.lost = true;
        assert_eq!(line(lost), Some(LineTwo::NotLive));
        // Waiting beats the note and the place; with its question, or without one.
        assert_eq!(
            line(base(Status::Waiting)),
            Some(LineTwo::Waiting(Some("Proceed? (y/n)".into())))
        );
        let mut asking = base(Status::Waiting);
        asking.question = None;
        assert_eq!(line(asking), Some(LineTwo::Waiting(None)));
        // A script's note beats the place.
        assert_eq!(
            line(base(Status::Working)),
            Some(LineTwo::Note("3 of 7".into()))
        );
        // The place, else nothing.
        let mut placed = base(Status::Idle);
        placed.note = None;
        assert_eq!(line(placed), Some(LineTwo::Place));
        let mut branch_only = facts("api", Status::Idle, now, None);
        branch_only.branch = Some("main");
        assert_eq!(line(branch_only), Some(LineTwo::Place));
        assert_eq!(line(facts("shell", Status::Idle, now, None)), None);
    }

    #[test]
    fn an_idle_row_knows_whether_a_program_runs() {
        let now = Instant::now();
        let mut server = facts("server", Status::Idle, now, None);
        server.running = true;
        let shell = facts("shell", Status::Idle, now, None);
        let rows = rows(&numbered(vec![server, shell]), &[], None, &[], now);
        assert!(session(&rows[0]).running);
        assert!(!session(&rows[1]).running);
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

    /// Note 60 §2: a waiting row shows its question cut to one line; the whole question is in
    /// the row's tooltip, on its own line under the name (and the title, when it says more).
    #[test]
    fn a_waiting_rows_tooltip_has_its_whole_question() {
        let question = "Do you want to make this edit to src/auth/session_store.rs? (y/n)";
        assert_eq!(
            row_tip("api", None, Some(question)).as_deref(),
            Some(format!("api\n{question}").as_str())
        );
        assert_eq!(
            row_tip("api", Some("Editing auth"), Some(question)).as_deref(),
            Some(format!("api\nEditing auth\n{question}").as_str())
        );
        // Not waiting (no question): as before, the title only when it says more.
        assert_eq!(
            row_tip("fuxx-cli", Some("zsh \u{2014} fuxx-cli"), None).as_deref(),
            Some("fuxx-cli\nzsh \u{2014} fuxx-cli")
        );
        assert_eq!(row_tip("fuxx-cli", Some("fuxx-cli"), None), None);
        assert_eq!(
            row_tip("fuxx-cli", None, Some("  ")),
            None,
            "a blank question"
        );
    }

    /// An idle row's mark (dot or ring) and its name describe the same terminal: the one it is
    /// named after, not the rollup's (any idle one); a busier row's mark is the rollup's.
    #[test]
    fn an_idle_rows_dot_is_its_named_terminals() {
        // Terminal 1 (the rollup's pick, at its prompt) and 2 (focused, named after, runs a
        // server) are both idle.
        let running = |id: u64| id == 2;
        assert!(row_running(Status::Idle, Some(1), Some(2), running));
        assert!(!row_running(Status::Idle, Some(2), Some(1), running));
        // Not idle: the subject's (the mark there isn't the idle one).
        assert!(row_running(Status::Working, Some(2), Some(1), running));
        assert!(!row_running(Status::Waiting, Some(1), Some(2), running));
        assert!(
            !row_running(Status::Idle, None, None, running),
            "no terminal"
        );
    }
}
