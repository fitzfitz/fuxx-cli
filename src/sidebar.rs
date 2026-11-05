//! The sidebar (M11): every session as a row that reads at a glance, under its group's header.
//!
//! `rows` is the pure view-model: what each row shows (number, name, status and how long it has
//! been in it, the waiting question, active / on screen), built from facts the Workspace hands
//! over. The rest of the file draws it.

use std::cell::Cell;
use std::f32::consts::TAU;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use alacritty_terminal::vte::ansi::Rgb;
use gpui::{
    Animation, AnimationExt, AnyElement, BoxShadow, ClickEvent, Context, DispatchPhase, Div,
    DragMoveEvent, EntityId, FontFeatures, FontWeight, Hsla, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, PathBuilder, ScrollHandle, SharedString, Task, Window, canvas,
    div, point, prelude::*, px, relative,
};

use crate::appearance::Appearance;
use crate::groups::{self, GroupKey, Row};
use crate::preferences;
use crate::presets::MAX_PRESETS;
use crate::reduce_motion::WindowVisible;
use crate::restore::SavedState;
use crate::status::{self, Status};
use crate::status_mark;
use crate::terminal_view::hsla;
use crate::theme::UiColors;
use crate::ui_tokens::size::{
    DIVIDER_ACTIVE, GROUP_HEADER, SIDEBAR_DEFAULT_WIDTH, SIDEBAR_MAX_WIDTH, SIDEBAR_MIN_WIDTH,
    SIDEBAR_ROW, STATUS_MARK, WAITING_ROW,
};
use crate::ui_tokens::text;
use crate::workspace::{POP_SHOWN, Session, Workspace};

/// What the sidebar needs to know of one session, in sidebar order.
pub struct SessionFacts<'a> {
    pub label: &'a str,
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
}

/// One row of the sidebar.
#[derive(Clone, Debug, PartialEq)]
pub enum SidebarRow {
    Header(HeaderRow),
    Session(SessionRow),
}

/// A group's header.
#[derive(Clone, Debug, PartialEq)]
pub struct HeaderRow {
    pub key: GroupKey,
    pub label: String,
    /// Its sessions: `first..first + count` in the session list.
    pub first: usize,
    pub count: usize,
    pub collapsed: bool,
    /// The group's most urgent status, shown only while it's collapsed (open, its rows say it).
    pub mark: Option<Status>,
}

/// A session's row.
#[derive(Clone, Debug, PartialEq)]
pub struct SessionRow {
    /// Its place in the session list; its number is one more.
    pub index: usize,
    pub label: String,
    pub status: Status,
    /// How long it has been in its status ("4m"); Idle shows none.
    pub age: Option<String>,
    /// The small line under the name, if any.
    pub sub: Option<Sub>,
    /// The focused session.
    pub active: bool,
    /// In the layout on screen, but not the focused pane.
    pub on_screen: bool,
    /// Under a group header.
    pub grouped: bool,
    /// The script-set progress bar's percent.
    pub progress: Option<u8>,
}

/// The line under a session's name.
#[derive(Clone, Debug, PartialEq)]
pub enum Sub {
    /// What a waiting session is asking.
    Question(String),
    /// Waiting, but no question was found on screen.
    NeedsInput,
    /// A script's note or progress label, or "not live".
    Note(String),
}

impl SessionRow {
    pub fn number(&self) -> usize {
        self.index + 1
    }
}

/// The sidebar's rows: leading ungrouped sessions, then each group under its header (a closed
/// group's sessions left out). `active` is the focused session; `shown`, the sessions in the
/// layout on screen.
pub fn rows(
    sessions: &[SessionFacts],
    collapsed: &[GroupKey],
    active: Option<usize>,
    shown: &[usize],
    now: Instant,
) -> Vec<SidebarRow> {
    let keys: Vec<Option<&GroupKey>> = sessions.iter().map(|s| s.group.as_ref()).collect();
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
                let is_active = active == Some(index);
                SidebarRow::Session(SessionRow {
                    index,
                    label: s.label.to_string(),
                    status: s.status,
                    age: (s.status != Status::Idle)
                        .then(|| status::age_label(now.saturating_duration_since(s.since))),
                    sub: sub(s),
                    active: is_active,
                    on_screen: !is_active && shown.contains(&index),
                    grouped,
                    progress: s.progress.map(|(p, _)| *p),
                })
            }
        })
        .collect()
}

/// The line under a session's name: "not live" for a lost one; a waiting one's question; else
/// a script's progress label and note.
fn sub(s: &SessionFacts) -> Option<Sub> {
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

/// A width the sidebar can take: within 180–400 px (the default for a missing or odd one).
pub fn clamp_width(width: Option<f32>) -> f32 {
    match width {
        Some(w) if w.is_finite() => w.clamp(SIDEBAR_MIN_WIDTH, SIDEBAR_MAX_WIDTH),
        _ => SIDEBAR_DEFAULT_WIDTH,
    }
}

/// Where a session dragged from `from` and dropped on `target`'s upper (or with `below`, lower)
/// half lands, as `SessionList::move_item` takes it.
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
/// The session number's column, right-aligned, and the space after it.
const NUMBER_WIDTH: f32 = 16.0;
const NUMBER_GAP: f32 = 6.0;
/// The space between the mark and the number, and before the age.
const SLOT_GAP: f32 = 8.0;
/// A row's name line, and the small line under it.
const NAME_LINE: f32 = 18.0;
const SUB_LINE: f32 = 15.0;
/// A header's + (new session in the group), shown on hover.
const ADD_BUTTON: f32 = 18.0;
/// The footer under the rows; the resize strip along the right edge.
const FOOTER: f32 = 36.0;
const RESIZE_STRIP: f32 = 4.0;
/// The "+ New" menu's width.
const MENU_WIDTH: f32 = 230.0;
/// How long a header's chevron takes to turn.
const CHEVRON_TURN: Duration = Duration::from_millis(120);
/// The first-run hint card.
const HINT_TITLE: &str = "Three things to know";
const HINT_LINES: [&str; 3] = [
    "⌘T new session · ⇧⌘S SSH · ⇧⌘B browser",
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
    /// While a row is dragged: the row the pointer is over (by view), and whether it's in its
    /// lower half. Only where the drop would move it.
    pub drop_at: Option<(EntityId, bool)>,
    /// The row being dragged (by view), noted as its drag starts.
    pub drag_source: Rc<Cell<Option<EntityId>>>,
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

/// What a sidebar row carries while it's dragged: the session (by its view) and its group.
pub(crate) struct DraggedSession {
    view: EntityId,
    group: Option<GroupKey>,
}

/// The row that follows the pointer during a drag: the row itself, lifted.
struct DragPreview {
    number: usize,
    label: SharedString,
    status: Status,
    width: f32,
}

impl Render for DragPreview {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ui = cx.global::<Appearance>().theme.ui();
        div()
            .flex()
            .items_center()
            .w(px(self.width - 2.0 * ROW_INSET))
            .h(px(SIDEBAR_ROW))
            .px(px(ROW_PAD))
            .rounded(px(6.0))
            .opacity(0.95)
            .shadow_md()
            .bg(hsla(ui.row_active))
            .border_1()
            .border_color(hsla(ui.panel_border))
            .text_size(px(text::NAME))
            .text_color(hsla(ui.text))
            .child(mark_slot(status_mark::mark_on(
                self.status,
                &ui,
                0.0,
                None,
                px(mark_size(self.status)),
                ui.row_active,
            )))
            .child(number(self.number, ui))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .truncate()
                    .child(self.label.clone()),
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

/// A session's number in its fixed, right-aligned column.
fn number(n: usize, ui: UiColors) -> Div {
    div()
        .flex_none()
        .w(px(NUMBER_WIDTH))
        .ml(px(SLOT_GAP))
        .mr(px(NUMBER_GAP))
        .text_right()
        .text_size(px(text::META))
        .font_features(tabular())
        .text_color(hsla(ui.text_faint))
        .child(n.to_string())
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
    /// The sidebar's rows as they are now.
    fn sidebar_rows(&self, now: Instant) -> Vec<SidebarRow> {
        let items = self.sessions.items();
        let labels: Vec<SharedString> = items.iter().map(Session::label).collect();
        let facts: Vec<SessionFacts> = items
            .iter()
            .zip(&labels)
            .map(|(s, label)| SessionFacts {
                label,
                status: s.status,
                since: s.status_since,
                question: s.question.as_deref(),
                group: s.group(),
                progress: s.progress.as_ref(),
                note: s.note.as_deref(),
                lost: s.lost,
            })
            .collect();
        let shown: Vec<usize> = self
            .shown_layout()
            .map(|t| {
                t.leaves()
                    .into_iter()
                    .filter_map(|id| self.sessions.position(|s| s.view.entity_id() == id))
                    .collect()
            })
            .unwrap_or_default();
        rows(
            &facts,
            &self.collapsed,
            self.sessions.active_index(),
            &shown,
            now,
        )
    }

    /// The marks on screen: each row in the scrolled view (a closed group's header shows its
    /// sessions' most urgent; none with the sidebar hidden), and each pane header.
    pub(crate) fn visible_statuses(&self) -> Vec<Status> {
        let headers = self
            .header_sessions()
            .into_iter()
            .map(|i| self.sessions.items()[i].status);
        self.rows_in_view(Instant::now())
            .into_iter()
            .filter_map(|row| match row {
                SidebarRow::Header(h) => h.mark,
                SidebarRow::Session(s) => Some(s.status),
            })
            .chain(headers)
            .collect()
    }

    /// Whether session `index`'s row is on screen: the sidebar is shown, its group open, and
    /// the row in the scrolled view.
    pub(crate) fn row_on_screen(&self, index: usize) -> bool {
        self.rows_in_view(Instant::now())
            .iter()
            .any(|row| matches!(row, SidebarRow::Session(s) if s.index == index))
    }

    /// The sidebar's rows in the scrolled view; none with the sidebar hidden.
    fn rows_in_view(&self, now: Instant) -> Vec<SidebarRow> {
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
                    SidebarRow::Session(s) if s.age.is_some() => Some(s.index),
                    _ => None,
                });
            // Pane headers show an age too (every status but Idle).
            let headers = self
                .header_sessions()
                .into_iter()
                .filter(|&i| self.sessions.items()[i].status != Status::Idle);
            let sinces = rows
                .chain(headers)
                .map(|i| self.sessions.items()[i].status_since);
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
                    .position(|r| matches!(r, SidebarRow::Session(s) if s.active));
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
                SidebarRow::Session(s) => self.render_session_row(s, ui, cx),
            })
            .collect();
        let band = self.render_top_band("sidebar-band", window, cx);
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

    /// A group's header: chevron, name, then (closed) its most urgent mark, its session count,
    /// and on hover a + for a new session in it. Click opens or closes it.
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

    /// A session's row: mark, number, name (or the rename box), age and on-screen pip; under
    /// them a waiting session's question or a script's note; a script's progress along the
    /// bottom. Click selects, double-click renames; it can be dragged within its group.
    fn render_session_row(
        &self,
        row: &SessionRow,
        ui: UiColors,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let index = row.index;
        let session = &self.sessions.items()[index];
        let view = session.view.entity_id();
        let waiting = row.status == Status::Waiting;
        // The active row is filled; a waiting one is tinted. Hover deepens either a little.
        let fill = if row.active {
            Some(ui.row_active)
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
        let label_color = if row.active || row.on_screen || waiting {
            ui.text
        } else {
            ui.text_dim
        };
        let rename_box = self
            .renaming
            .as_ref()
            .filter(|(v, _)| *v == view)
            .map(|(_, input)| input.clone());
        let group = session.group();
        // While a row is dragged: it's dimmed; rows of another group refuse it.
        let dragged = self
            .sidebar
            .drag_source
            .get()
            .filter(|_| cx.has_active_drag());
        let lifted = dragged == Some(view);
        let refused = dragged.is_some_and(|d| {
            self.sessions
                .items()
                .iter()
                .find(|s| s.view.entity_id() == d)
                .is_some_and(|s| s.group() != group)
        });
        let insertion = self
            .sidebar
            .drop_at
            .filter(|(v, _)| dragged.is_some() && *v == view)
            .map(|(_, below)| below);

        let name: AnyElement = match rename_box {
            Some(input) => input.into_any_element(),
            None => div()
                .flex_1()
                .min_w(px(0.0))
                .truncate()
                .when(row.active, |name| name.font_weight(FontWeight::MEDIUM))
                .child(row.label.clone())
                .into_any_element(),
        };
        let name_line = div()
            .flex()
            .items_center()
            .h(px(NAME_LINE))
            .child(mark_slot(self.session_mark("row", session, ui, surface)))
            .child(number(row.number(), ui))
            .child(name)
            .when_some(row.age.clone(), |line, age| {
                line.child(
                    div()
                        .flex_none()
                        .ml(px(SLOT_GAP))
                        .text_size(px(text::META))
                        .font_features(tabular())
                        .text_color(hsla(ui.text_faint))
                        .child(age),
                )
            })
            .when(row.on_screen, |line| {
                line.child(
                    div()
                        .flex_none()
                        .ml(px(6.0))
                        .size(px(4.0))
                        .rounded(px(1.0))
                        .bg(hsla(ui.text_dim)),
                )
            });
        let sub_line = row.sub.as_ref().map(|sub| {
            let (line, color) = match sub {
                Sub::Question(q) => (q.clone(), ui.text_dim),
                Sub::NeedsInput => ("Needs your input".to_string(), ui.text_faint),
                Sub::Note(note) => (note.clone(), ui.text_faint),
            };
            div()
                .h(px(SUB_LINE))
                .pl(px(MARK_SLOT + SLOT_GAP + NUMBER_WIDTH + NUMBER_GAP))
                .text_size(px(text::META))
                .line_height(px(SUB_LINE))
                .text_color(hsla(color))
                .truncate()
                .child(line)
        });
        let height = if row.sub.is_some() {
            WAITING_ROW
        } else {
            SIDEBAR_ROW
        };
        let source = self.sidebar.drag_source.clone();
        let preview = DragPreview {
            number: row.number(),
            label: row.label.clone().into(),
            status: row.status,
            width: self.sidebar.width,
        };
        let accepts = group.clone();
        div()
            .id(("session", view))
            .flex_none()
            .relative()
            .flex()
            .flex_col()
            .justify_center()
            .h(px(height))
            .mx(px(ROW_INSET))
            .px(px(ROW_PAD))
            .rounded(px(6.0))
            .text_size(px(text::NAME))
            .text_color(hsla(label_color))
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
                this.select(index, window, cx);
                if event.click_count() == 2 {
                    this.start_rename(index, window, cx);
                }
            }))
            .on_drag(
                DraggedSession {
                    view,
                    group: group.clone(),
                },
                move |dragged, _, _, cx| {
                    source.set(Some(dragged.view));
                    cx.new(|_| DragPreview {
                        label: preview.label.clone(),
                        ..preview
                    })
                },
            )
            // Only rows of the dragged session's group accept it; the drop re-checks.
            .can_drop(move |dragged, _, _| {
                dragged
                    .downcast_ref::<DraggedSession>()
                    .is_some_and(|d| d.group == accepts)
            })
            .on_drag_move(
                cx.listener(move |this, event: &DragMoveEvent<DraggedSession>, _, cx| {
                    this.drag_over_row(view, event, cx)
                }),
            )
            .on_drop(cx.listener(move |this, dragged: &DraggedSession, _, cx| {
                this.drop_session(dragged.view, view, cx)
            }))
            // The active row's accent bar, on its left edge.
            .when(row.active, |row| {
                row.child(
                    div()
                        .absolute()
                        .left_0()
                        .top(px(6.0))
                        .bottom(px(6.0))
                        .w(px(DIVIDER_ACTIVE))
                        .rounded(px(1.0))
                        .bg(hsla(ui.accent)),
                )
            })
            .child(name_line)
            .children(sub_line)
            // The progress bar: along the bottom, as wide as the percent.
            .when_some(row.progress, |row, percent| {
                row.child(
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
                                .bg(hsla(ui.ring_working)),
                        ),
                )
            })
            // Where a dragged row would land: a line in the gap above or below this one.
            .when_some(insertion, |row, below| {
                let line = div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .h(px(DIVIDER_ACTIVE))
                    .rounded(px(1.0))
                    .bg(hsla(ui.accent));
                row.child(if below {
                    line.bottom(px(-ROW_GAP))
                } else {
                    line.top(px(-ROW_GAP))
                })
            })
            .into_any_element()
    }

    /// A session's mark on its surface's `fill` (a row, or a pane's header: `place` keeps their
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

    /// The pointer moved during a row drag: if it's over this row (`view`) and dropping there
    /// would move the dragged one, the insertion line goes above or below it.
    fn drag_over_row(
        &mut self,
        view: EntityId,
        event: &DragMoveEvent<DraggedSession>,
        cx: &mut Context<Self>,
    ) {
        let over = event.bounds.contains(&event.event.position);
        if !over && self.sidebar.drop_at.is_none_or(|(v, _)| v != view) {
            return; // not this row's business
        }
        let dragged = event.drag(cx);
        let (dragged, group) = (dragged.view, dragged.group.clone());
        let find = |id: EntityId| self.sessions.position(|s| s.view.entity_id() == id);
        let next = match (over, find(dragged), find(view)) {
            (true, Some(from), Some(to)) if self.sessions.items()[to].group() == group => {
                let below = event.event.position.y > event.bounds.center().y;
                (drop_index(from, to, below) != from).then_some((view, below))
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

    /// The "+ New" menu, over the window above the footer: a new session (here, or in a
    /// folder) and each preset. A click anywhere else closes it.
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
        let mut items = vec![
            item(("new-menu", 0), "New Session".into(), "⌘T".into())
                .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                    this.close_new_menu(cx);
                    this.new_session(window, cx)
                }))
                .into_any_element(),
            item(
                ("new-menu", 1),
                "New Session in Folder…".into(),
                "⌘O".into(),
            )
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

/// A header's element id: stable while its sessions are reordered or closed.
fn group_id(key: &GroupKey) -> u64 {
    let mut hasher = DefaultHasher::new();
    key.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn folder(p: &str) -> GroupKey {
        GroupKey::Folder(PathBuf::from(p))
    }

    fn facts(
        label: &str,
        status: Status,
        since: Instant,
        group: Option<GroupKey>,
    ) -> SessionFacts<'_> {
        SessionFacts {
            label,
            status,
            since,
            question: None,
            group,
            progress: None,
            note: None,
            lost: false,
        }
    }

    fn session(row: &SidebarRow) -> &SessionRow {
        match row {
            SidebarRow::Session(s) => s,
            SidebarRow::Header(h) => panic!("a header: {h:?}"),
        }
    }

    fn header(row: &SidebarRow) -> &HeaderRow {
        match row {
            SidebarRow::Header(h) => h,
            SidebarRow::Session(s) => panic!("a session: {s:?}"),
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
        let rows = rows(&sessions, &[], Some(1), &[1, 3], now);
        assert_eq!(rows.len(), 6, "{rows:?}");

        let shell = session(&rows[0]);
        assert_eq!((shell.number(), shell.label.as_str()), (1, "shell"));
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
        assert!(!codex.on_screen);

        let tests = session(&rows[4]);
        assert!(tests.on_screen && !tests.active);
        assert_eq!(tests.age.as_deref(), Some("2h"));

        // Waiting with no question found: says it needs you.
        let agent = session(&rows[5]);
        assert_eq!(agent.sub, Some(Sub::NeedsInput));
        assert_eq!(agent.number(), 5);
    }

    #[test]
    fn a_lost_session_says_not_live() {
        let now = Instant::now();
        let mut s = facts("api", Status::Waiting, now, None);
        s.question = Some("Proceed? (y/n)");
        s.lost = true;
        let rows = rows(&[s], &[], None, &[], now);
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
        let rows = rows(&sessions, &[web.clone()], None, &[], now);
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
}
