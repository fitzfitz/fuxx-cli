//! The right sidebar's Activity panel (M11h): the followed terminal's timeline, from its
//! `activity::ActivityLog` (kept in memory on its `Session`).
//!
//! M11v part 4 ("Activity A · Timeline"): a 42 pt header, the terminal's name (`terminal_label`)
//! and the status bar's words for it as a coloured pill (`Done · 4m`: green done, red waiting,
//! yellow cultivating, grey otherwise; `pill_inks`), a hairline under it; then the events,
//! newest first, on a thin rail, grouped under a day heading (`TODAY`, else the date): an 8 pt
//! dot coloured by kind (the status colours; progress the accent; a note grey), the event's
//! words, a faint second line (`Worked 11 min`, `Note`), the time at the right; progress has a
//! 3 pt bar, and a waiting question shows whole in a tinted box. Events differ in height, so the
//! timeline is a GPUI `list` (only the events in sight are built). A title that is cut is whole
//! in its tooltip. Empty states: no terminal followed, nothing in its log yet. It follows
//! `right_sidebar.followed`, asks nothing of git or the disk, takes no keys, and nothing in it
//! moves.
//!
//! `rows_in`, `timeline` and `pill_inks` are pure and tested.

use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

use alacritty_terminal::vte::ansi::Rgb;
use gpui::{
    AnyElement, AnyView, App, Context, FontWeight, SharedString, div, list, prelude::*, px,
    relative,
};

use crate::activity::{
    ActivityLog, Kind, clock_hm, day_heading, event_detail, event_title, local_day,
};
use crate::cli;
use crate::right_sidebar::{EDGE, SMALL, TEXT, TITLE, header_row, note};
use crate::sidebar::tabular;
use crate::sidebar_toggle::Tip;
use crate::status::Status;
use crate::terminal_view::hsla;
use crate::theme::{UiColors, mix, readable};
use crate::ui_tokens::TEXT_MIN;
use crate::workspace::{Session, Workspace};

pub(crate) const NO_ACTIVITY: &str = "No activity yet.";
pub(crate) const NO_TERMINAL: &str = "No terminal to follow.";
/// An event (part 4): the rail's width and the room after it, the dot and its room above,
/// the stem's room under the dot, the title's and the second line's line heights, the room
/// under an event, the progress bar's height and its room above.
const RAIL: f32 = 14.0;
const RAIL_GAP: f32 = 10.0;
const DOT: f32 = 8.0;
const DOT_TOP: f32 = 6.0;
const STEM_GAP: f32 = 3.0;
const TITLE_LINE: f32 = 20.0;
const DETAIL_LINE: f32 = 16.0;
const EVENT_BELOW: f32 = 12.0;
const BAR: f32 = 3.0;
const BAR_TOP: f32 = 5.0;
/// The question's box: its room above, padding, corners and text.
const QUESTION_TOP: f32 = 6.0;
const QUESTION_PAD_X: f32 = 10.0;
const QUESTION_PAD_Y: f32 = 8.0;
const QUESTION_CORNER: f32 = 8.0;
const QUESTION_TEXT: f32 = 13.0;
/// A day's heading: its room above and below.
const DAY_TOP: f32 = 10.0;
const DAY_BELOW: f32 = 4.0;
/// The header's pill: its padding and how much of its colour tints its fill.
const PILL_PAD_X: f32 = 8.0;
const PILL_PAD_Y: f32 = 2.0;
const PILL_TINT: f32 = 0.16;
/// How far past the panel's edges the list lays events out (so a scroll doesn't pop).
pub(crate) const OVERDRAW: f32 = 200.0;

/// What an event's dot is coloured by: its status, or progress (the accent), or a note (grey).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowMark {
    Status(Status),
    Progress,
    Note,
}

/// An event as drawn: its local time, dot, words, second line, progress, question and
/// tooltip.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActivityRow {
    pub time: String,
    pub mark: RowMark,
    pub text: String,
    pub detail: Option<String>,
    pub progress: Option<u8>,
    pub question: Option<String>,
    pub tip: Option<String>,
}

/// A line of the timeline: a day's heading, or the event at that index (newest first).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Item {
    Day(String),
    Event(usize),
}

/// The log's rows, newest first (see `rows_in`).
#[cfg(test)]
pub fn rows(log: &ActivityLog, offset: impl Fn(SystemTime) -> i64) -> Vec<ActivityRow> {
    rows_in(log, 0..log.len(), offset)
}

fn secs_of(at: SystemTime) -> i64 {
    at.duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

/// The rows `range` of the log's events (newest first), only those built (the list asks for
/// the ones in sight): each entry's local `HH:MM` (`offset`: seconds east of UTC at that
/// time), its dot, words and second line (`event_title`, `event_detail`), a progress's
/// percent, a status's question. The tooltip is for what a title can cut: a note's or a
/// progress's words whole, and a Waiting entry's words with its question on a second line;
/// none for the other status words (short, never cut).
pub fn rows_in(
    log: &ActivityLog,
    range: std::ops::Range<usize>,
    offset: impl Fn(SystemTime) -> i64,
) -> Vec<ActivityRow> {
    log.newest_first()
        .skip(range.start)
        .take(range.len())
        .map(|entry| {
            let text = event_title(&entry.kind);
            let (mark, tip, progress, question) = match &entry.kind {
                Kind::Status {
                    status, question, ..
                } => (
                    RowMark::Status(*status),
                    question.as_ref().map(|q| format!("{text}\n{q}")),
                    None,
                    question.clone(),
                ),
                Kind::Note(_) => (RowMark::Note, Some(text.clone()), None, None),
                Kind::Progress(p, _) => (RowMark::Progress, Some(text.clone()), Some(*p), None),
            };
            ActivityRow {
                time: clock_hm(secs_of(entry.at), offset(entry.at)),
                mark,
                detail: event_detail(&entry.kind),
                text,
                progress,
                question,
                tip,
            }
        })
        .collect()
}

/// The timeline's lines, newest first: a day's heading (`day_heading`, against `today`) before
/// the first event of each day (its local day, `offset` at its time), then its events.
pub fn timeline(log: &ActivityLog, offset: impl Fn(SystemTime) -> i64, today: i64) -> Vec<Item> {
    let mut items = Vec::with_capacity(log.len() + 2);
    let mut last = None;
    for (i, entry) in log.newest_first().enumerate() {
        let day = local_day(secs_of(entry.at), offset(entry.at));
        if last != Some(day) {
            items.push(Item::Day(day_heading(day, today)));
            last = Some(day);
        }
        items.push(Item::Event(i));
    }
    items
}

/// The colour a status stands for (the status marks'): Working yellow, Waiting red, Done
/// green, Idle grey.
fn status_ink(status: Status, ui: &UiColors) -> Rgb {
    match status {
        Status::Working => ui.ring_working,
        Status::Waiting => ui.ring_waiting,
        Status::Done => ui.ring_done,
        Status::Idle => ui.text_faint,
    }
}

/// The header pill's fill (a wash of the status's colour over the card) and its words' colour
/// (the status's, pushed, hue kept, to read on that fill).
pub fn pill_inks(status: Status, ui: &UiColors) -> (Rgb, Rgb) {
    let colour = status_ink(status, ui);
    let fill = mix(ui.card, colour, PILL_TINT);
    (fill, readable(colour, &[fill], TEXT_MIN))
}

/// An event's dot colour: its status's, the accent for progress, grey for a note.
fn dot_ink(mark: RowMark, ui: &UiColors) -> Rgb {
    match mark {
        RowMark::Status(status) => status_ink(status, ui),
        RowMark::Progress => ui.accent,
        RowMark::Note => ui.text_faint,
    }
}

/// A day's heading: the words in capitals (12 pt semibold, faint) on the 12 pt edge.
fn day_row(words: &str, ui: &UiColors) -> AnyElement {
    div()
        .w_full()
        .px(px(EDGE))
        .pt(px(DAY_TOP))
        .pb(px(DAY_BELOW))
        .text_size(px(SMALL))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(hsla(ui.text_faint))
        .child(words.to_uppercase())
        .into_any_element()
}

/// Event `i` on the rail: the dot (and, unless it is the day's last, the stem down to the
/// next), the words (14 pt, cut at the end, whole in the tooltip), the second line (12 pt,
/// faint), a progress bar or the question's tinted box; the time at the right (12 pt, faint,
/// tabular).
fn event_row(i: usize, row: &ActivityRow, stem: bool, ui: &UiColors) -> AnyElement {
    let rail = div()
        .flex_none()
        .flex()
        .flex_col()
        .items_center()
        .w(px(RAIL))
        .child(
            div()
                .flex_none()
                .mt(px(DOT_TOP))
                .size(px(DOT))
                .rounded_full()
                .bg(hsla(dot_ink(row.mark, ui))),
        )
        .when(stem, |r| {
            r.child(
                div()
                    .flex_1()
                    .w(px(1.0))
                    .mt(px(STEM_GAP))
                    .bg(hsla(ui.guide)),
            )
        });
    let tip: Option<SharedString> = row.tip.clone().map(Into::into);
    let title = div()
        .id(("activity-row", i))
        .min_w(px(0.0))
        .truncate()
        .line_height(px(TITLE_LINE))
        .text_size(px(TEXT))
        .text_color(hsla(ui.text))
        .when_some(tip, |r, tip| {
            r.tooltip(move |_, cx| -> AnyView { cx.new(|_| Tip(tip.clone())).into() })
        })
        .child(row.text.clone());
    let detail = row.detail.clone().map(|d| {
        div()
            .line_height(px(DETAIL_LINE))
            .text_size(px(SMALL))
            .text_color(hsla(ui.text_faint))
            .child(d)
    });
    let bar = row.progress.map(|p| {
        div()
            .mt(px(BAR_TOP))
            .h(px(BAR))
            .w_full()
            .rounded(px(BAR))
            .bg(hsla(ui.divider))
            .child(
                div()
                    .h_full()
                    .w(relative(f32::from(p.min(100)) / 100.0))
                    .rounded(px(BAR))
                    .bg(hsla(ui.accent)),
            )
    });
    let question = row.question.clone().map(|q| {
        div()
            .mt(px(QUESTION_TOP))
            .px(px(QUESTION_PAD_X))
            .py(px(QUESTION_PAD_Y))
            .rounded(px(QUESTION_CORNER))
            .bg(hsla(ui.waiting_tint))
            .text_size(px(QUESTION_TEXT))
            .text_color(hsla(ui.text))
            .child(q)
    });
    let body = div()
        .flex()
        .flex_col()
        .flex_1()
        .min_w(px(0.0))
        .pb(px(EVENT_BELOW))
        .child(title)
        .children(detail)
        .children(bar)
        .children(question);
    let time = div()
        .flex_none()
        .line_height(px(TITLE_LINE))
        .text_size(px(SMALL))
        .font_features(tabular())
        .text_color(hsla(ui.text_faint))
        .child(row.time.clone());
    div()
        .flex()
        .gap(px(RAIL_GAP))
        .w_full()
        .px(px(EDGE))
        .child(rail)
        .child(body)
        .child(time)
        .into_any_element()
}

/// The header (part 4, 42 pt): the name (14 pt semibold) and the status bar's words for the
/// terminal in a pill of its status's colour (`pill_inks`); a hairline under it.
fn activity_header_element(
    label: SharedString,
    words: String,
    status: Status,
    ui: &UiColors,
) -> AnyElement {
    let (fill, ink) = pill_inks(status, ui);
    let row = header_row()
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .truncate()
                .text_size(px(TITLE))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(hsla(ui.text))
                .child(label),
        )
        .child(
            div()
                .id("activity-status")
                .flex_none()
                .px(px(PILL_PAD_X))
                .py(px(PILL_PAD_Y))
                .rounded_full()
                .whitespace_nowrap()
                .bg(hsla(fill))
                .text_size(px(SMALL))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(hsla(ink))
                .child(words),
        );
    div()
        .flex_none()
        .flex()
        .flex_col()
        .w_full()
        .child(row)
        .child(div().mx(px(EDGE)).h(px(1.0)).bg(hsla(ui.divider)))
        .into_any_element()
}

/// Seconds east of UTC at `at`, on this Mac.
fn offset_at(at: SystemTime) -> i64 {
    let ms = at.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis());
    cli::local_utc_offset(ms as u64)
}

impl Workspace {
    /// The followed terminal (`right_sidebar.followed`), if there is one: the panel's subject.
    fn activity_session(&self) -> Option<&Session> {
        let id = self.right_sidebar.followed?;
        self.sessions.items().iter().find(|s| s.id == id)
    }

    /// The header's words: the followed terminal's name (`terminal_label`), the status bar's
    /// words for it, and its status (the pill's colour). None with no terminal followed.
    pub(crate) fn activity_header(&self) -> Option<(SharedString, String, Status)> {
        let session = self.activity_session()?;
        let facts = self.status_bar_facts()?;
        Some((self.terminal_label(session.id), facts.text, facts.status))
    }

    /// What the panel shows in place of its timeline, if anything: no terminal followed, or
    /// one with nothing in its log yet.
    pub(crate) fn activity_note(&self) -> Option<&'static str> {
        match self.activity_session() {
            None => Some(NO_TERMINAL),
            Some(s) if s.activity.is_empty() => Some(NO_ACTIVITY),
            Some(_) => None,
        }
    }

    /// The followed terminal's timeline rows, newest first, in this Mac's local time.
    #[cfg(feature = "ui-shots")]
    pub(crate) fn activity_rows(&self) -> Vec<ActivityRow> {
        let all = self.activity_session().map_or(0, |s| s.activity.len());
        self.activity_rows_in(0..all)
    }

    /// The timeline's lines (`timeline`), today being this Mac's.
    pub(crate) fn activity_timeline(&self) -> Vec<Item> {
        let now = SystemTime::now();
        let today = local_day(secs_of(now), offset_at(now));
        self.activity_session()
            .map_or_else(Vec::new, |s| timeline(&s.activity, offset_at, today))
    }

    /// The timeline's rows `range` (newest first), only those built (`rows_in`).
    fn activity_rows_in(&self, range: std::ops::Range<usize>) -> Vec<ActivityRow> {
        self.activity_session()
            .map_or_else(Vec::new, |s| rows_in(&s.activity, range, offset_at))
    }

    /// The Activity panel under the strip: the header, then the timeline (a GPUI `list`: the
    /// events differ in height), or a note in its place (`activity_note`). The list starts
    /// again at the top (the newest) when its lines change, when it is shown again or follows
    /// another terminal. Nothing in it moves, and it asks nothing of git or the disk.
    /// (`_width`: as the Files panel's; nothing here is cut to a measured width.)
    pub(crate) fn render_activity_panel(
        &self,
        ui: UiColors,
        _width: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let header = self
            .activity_header()
            .map(|(label, words, status)| activity_header_element(label, words, status, &ui));
        let body = match self.activity_note() {
            Some(text) => note(&ui, text),
            None => {
                let items = Rc::new(self.activity_timeline());
                let session = self.activity_session();
                let key = session.map(|s| {
                    let newest = s.activity.newest_first().next().map(|e| e.at);
                    (s.id, items.len(), newest)
                });
                let state = self.right_sidebar.activity_list.clone();
                if self.right_sidebar.activity_key.get() != key {
                    state.reset(items.len());
                    self.right_sidebar.activity_key.set(key);
                }
                let this = cx.entity();
                list(state, move |i, _window, cx: &mut App| {
                    let ui = cx.global::<crate::appearance::Appearance>().theme.ui();
                    match items.get(i) {
                        Some(Item::Day(words)) => day_row(words, &ui),
                        Some(Item::Event(at)) => {
                            let stem = matches!(items.get(i + 1), Some(Item::Event(_)));
                            let row = this.read(cx).activity_rows_in(*at..*at + 1);
                            match row.first() {
                                Some(row) => event_row(*at, row, stem, &ui),
                                None => div().into_any_element(),
                            }
                        }
                        None => div().into_any_element(),
                    }
                })
                .size_full()
                .into_any_element()
            }
        };
        div()
            .id("activity-panel")
            .flex()
            .flex_col()
            .flex_1()
            .min_h(px(0.0))
            .w_full()
            .children(header)
            .child(div().flex_1().min_h(px(0.0)).w_full().child(body))
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui_tokens::{MARK_MIN, contrast};
    use std::time::Duration;

    #[test]
    fn only_the_rows_asked_for_are_built() {
        let t = |s: u64| UNIX_EPOCH + Duration::from_secs(1_000_000 + s * 60);
        let mut log = ActivityLog::default();
        for i in 0..6 {
            log.note(Some(&format!("note {i}")), t(i));
        }
        let built = std::cell::Cell::new(0);
        let some = rows_in(&log, 2..4, |_| {
            built.set(built.get() + 1);
            0
        });
        assert_eq!(built.get(), 2, "only the two rows in sight");
        assert_eq!(some, rows(&log, |_| 0)[2..4].to_vec());
        assert_eq!(some[0].text, "note 3", "newest first");
        // Past the end: what there is.
        assert_eq!(rows_in(&log, 5..9, |_| 0).len(), 1);
    }

    #[test]
    fn rows_are_newest_first_with_local_times_details_and_tips() {
        // 14:02 UTC on some day.
        let start = 19_700 * 86_400 + 14 * 3600 + 2 * 60;
        let t = |min: u64| UNIX_EPOCH + Duration::from_secs(start + min * 60);
        let mins = |n: u64| Duration::from_secs(n * 60);
        let mut log = ActivityLog::default();
        log.status(Status::Working, mins(3), None, t(0));
        log.status(
            Status::Waiting,
            mins(7),
            Some("Proceed? (y/n)".into()),
            t(7),
        );
        log.note(Some("migrating"), t(9));
        log.status(Status::Done, mins(11), None, t(18));
        let row = |time: &str, mark, text: &str, detail: Option<&str>| ActivityRow {
            time: time.into(),
            mark,
            text: text.into(),
            detail: detail.map(String::from),
            progress: None,
            question: None,
            tip: None,
        };
        let at = |min: u64| clock_hm((start + min * 60) as i64, 0);
        assert_eq!(
            rows(&log, |_| 0),
            vec![
                row(
                    &at(18),
                    RowMark::Status(Status::Done),
                    "Done",
                    Some("Worked 11 min")
                ),
                ActivityRow {
                    tip: Some("migrating".into()),
                    ..row(&at(9), RowMark::Note, "migrating", Some("Note"))
                },
                ActivityRow {
                    question: Some("Proceed? (y/n)".into()),
                    tip: Some("Needs input\nProceed? (y/n)".into()),
                    ..row(
                        &at(7),
                        RowMark::Status(Status::Waiting),
                        "Needs input",
                        None
                    )
                },
                row(
                    &at(0),
                    RowMark::Status(Status::Working),
                    "Cultivating",
                    None
                ),
            ]
        );
        assert_eq!(at(18), "14:20");
        assert_eq!(at(7), "14:09");
        // The offset is the one at each entry's time (east of UTC).
        let ahead = rows(&log, |at| if at == t(18) { 3600 } else { 0 });
        assert_eq!(ahead[0].time, "15:20");
        assert_eq!(ahead[1].time, "14:11");
        // Progress: the accent's dot, its percent (the bar), its words in the tooltip.
        log.progress(Some((42, Some("build".into()))), t(20));
        assert_eq!(
            rows(&log, |_| 0)[0],
            ActivityRow {
                progress: Some(42),
                tip: Some("Progress 42% \u{b7} build".into()),
                ..row(
                    &at(20),
                    RowMark::Progress,
                    "Progress 42% \u{b7} build",
                    None
                )
            }
        );
        assert!(rows(&ActivityLog::default(), |_| 0).is_empty());
    }

    /// Part 4: a day's heading before each day's events, newest first; the day is the local one
    /// at each event's time.
    #[test]
    fn the_timeline_groups_events_under_their_day() {
        let day = 20_768_u64; // 2026-11-11
        let at = |d: u64, h: u64| UNIX_EPOCH + Duration::from_secs(d * 86_400 + h * 3600);
        let mut log = ActivityLog::default();
        log.note(Some("old"), at(day - 1, 22));
        log.note(Some("a"), at(day, 9));
        log.note(Some("b"), at(day, 10));
        let today = day as i64;
        assert_eq!(
            timeline(&log, |_| 0, today),
            vec![
                Item::Day("Today".into()),
                Item::Event(0),
                Item::Event(1),
                Item::Day("10 Nov".into()),
                Item::Event(2),
            ]
        );
        // Three hours east of UTC, 22:00 on the 10th is already the 11th.
        assert_eq!(
            timeline(&log, |_| 3 * 3600, today),
            vec![
                Item::Day("Today".into()),
                Item::Event(0),
                Item::Event(1),
                Item::Event(2),
            ]
        );
        assert!(timeline(&ActivityLog::default(), |_| 0, today).is_empty());
    }

    /// The panel's words read (4.5:1) and its marks show (3:1) on what they are drawn on, in
    /// every built-in theme: the card (header, events), the pill's fill (each status), the
    /// question's tint.
    #[test]
    fn the_panel_reads_on_every_theme() {
        let mut failures = vec![];
        for (name, theme) in crate::theme::every_theme() {
            let ui = theme.ui();
            let mut check = |what: &str, ink, fill, floor: f32| {
                let ratio = contrast(ink, fill);
                if ratio < floor {
                    failures.push(format!("{name}: {what} is {ratio:.2}"));
                }
            };
            for (what, ink) in [
                ("the name, an event", ui.text),
                ("a second line, a time, a day", ui.text_faint),
            ] {
                check(&format!("{what} on the card"), ink, ui.card, TEXT_MIN);
            }
            for status in [Status::Idle, Status::Working, Status::Waiting, Status::Done] {
                let (fill, ink) = pill_inks(status, &ui);
                check(&format!("{status:?}'s pill"), ink, fill, TEXT_MIN);
            }
            check(
                "the question on its tint",
                ui.text,
                ui.waiting_tint,
                TEXT_MIN,
            );
            for mark in [
                RowMark::Status(Status::Working),
                RowMark::Status(Status::Waiting),
                RowMark::Status(Status::Done),
                RowMark::Status(Status::Idle),
                RowMark::Progress,
                RowMark::Note,
            ] {
                check(
                    &format!("{mark:?}'s dot on the card"),
                    dot_ink(mark, &ui),
                    ui.card,
                    MARK_MIN,
                );
            }
            check("the progress bar", ui.accent, ui.card, MARK_MIN);
        }
        assert!(failures.is_empty(), "\n{}", failures.join("\n"));
    }
}
