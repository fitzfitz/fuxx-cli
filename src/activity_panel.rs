//! The right sidebar's Activity panel (M11h): the followed terminal's timeline, from its
//! `activity::ActivityLog` (kept in memory on its `Session`).
//!
//! A header in the Files root header's style: the terminal's name (`terminal_label`) and under
//! it the status bar's words for it (`Working · 4m`; `Needs input · 2m` in `ring_waiting`).
//! While it waits, a card filled `waiting_tint` under the header: `Needs input` and the whole
//! question, wrapped. Then the timeline, newest first, 22 pt rows: the local time, the
//! status's still mark (or a dot for a note or progress) and the words, cut at the end, whole in
//! the tooltip with a Waiting entry's question. Empty states: no terminal followed, nothing in
//! its log yet. It follows `right_sidebar.followed`, asks nothing of git or the disk, takes no
//! keys, and nothing in it moves.
//!
//! `rows` is pure and tested.

use std::time::{SystemTime, UNIX_EPOCH};

use gpui::{
    AnyElement, AnyView, Context, FontWeight, SharedString, div, prelude::*, px, uniform_list,
};

use crate::activity::{ActivityLog, Kind, clock_hm, entry_text};
use crate::appearance::Appearance;
use crate::cli;
use crate::right_sidebar::{
    PATH_LINE, ROOT_HEADER, ROOT_LINE, ROOT_PAD_LEFT, ROOT_PAD_RIGHT, ROW, note,
};
use crate::sidebar::tabular;
use crate::sidebar_toggle::Tip;
use crate::status::Status;
use crate::status_bar::StatusWord;
use crate::status_mark;
use crate::terminal_view::hsla;
use crate::theme::UiColors;
use crate::ui_tokens::text;
use crate::workspace::{Session, Workspace};

pub(crate) const NO_ACTIVITY: &str = "No activity yet.";
pub(crate) const NO_TERMINAL: &str = "No terminal to follow.";
const NEEDS_INPUT: &str = "Needs input";
/// A row: its room left and right, the time's column, the mark's box and the dot drawn in it
/// for a note or progress, and the room between the mark and the words.
const ROW_LEFT: f32 = 8.0;
const ROW_RIGHT: f32 = 8.0;
const TIME_COLUMN: f32 = 36.0;
const MARK: f32 = 10.0;
const DOT: f32 = 4.0;
const MARK_GAP: f32 = 6.0;
/// The waiting card: its inset from the panel's sides (and from the header above it), its
/// padding and its corners.
const CARD_INSET: f32 = 6.0;
const CARD_PAD: f32 = 8.0;
const CARD_CORNER: f32 = 6.0;

/// A row's mark: a status's still mark, or a dot for a note or progress.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowMark {
    Status(Status),
    Dot,
}

/// A timeline row as drawn: its local time, mark, words and tooltip.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActivityRow {
    pub time: String,
    pub mark: RowMark,
    pub text: String,
    pub tip: Option<String>,
}

/// The log's rows, newest first: each entry's local `HH:MM` (`offset`: seconds east of UTC
/// at that time), its mark and words. The tooltip is for what a row can cut: a note's or a
/// progress's words whole, and a Waiting entry's words with its question on a second line;
/// none for the other status words (short, never cut).
#[cfg(test)]
pub fn rows(log: &ActivityLog, offset: impl Fn(SystemTime) -> i64) -> Vec<ActivityRow> {
    rows_in(log, 0..log.len(), offset)
}

/// The rows `range` of `rows` (newest first), and only those built: a `uniform_list` asks
/// for the rows in sight.
pub fn rows_in(
    log: &ActivityLog,
    range: std::ops::Range<usize>,
    offset: impl Fn(SystemTime) -> i64,
) -> Vec<ActivityRow> {
    log.newest_first()
        .skip(range.start)
        .take(range.len())
        .map(|entry| {
            let secs = entry
                .at
                .duration_since(UNIX_EPOCH)
                .map_or(0, |d| d.as_secs() as i64);
            let text = entry_text(&entry.kind);
            let (mark, tip) = match &entry.kind {
                Kind::Status {
                    status, question, ..
                } => (
                    RowMark::Status(*status),
                    question.as_ref().map(|q| format!("{text}\n{q}")),
                ),
                Kind::Note(_) | Kind::Progress(..) => (RowMark::Dot, Some(text.clone())),
            };
            ActivityRow {
                time: clock_hm(secs, offset(entry.at)),
                mark,
                text,
                tip,
            }
        })
        .collect()
}

/// Row `i` (22 pt): 8 pt in, the time (11 pt, tabular, faint) in its 36 pt column, the still
/// mark (10 pt: the status's at `STILL_PHASE`, as Reduce motion draws it, no pop; Idle's
/// hollow ring) or a 4 pt faint dot, 6 pt, the words (13 pt, cut at the end); its tooltip, if
/// any: the words whole (and a question).
fn activity_row(i: usize, row: &ActivityRow, ui: &UiColors) -> AnyElement {
    let mark = match row.mark {
        RowMark::Status(Status::Idle) => status_mark::idle_mark(false, ui).into_any_element(),
        RowMark::Status(status) => status_mark::mark_on(
            status,
            ui,
            status_mark::STILL_PHASE,
            None,
            px(MARK),
            ui.card,
        )
        .into_any_element(),
        RowMark::Dot => div()
            .size(px(DOT))
            .rounded_full()
            .bg(hsla(ui.text_faint))
            .into_any_element(),
    };
    let tip: Option<SharedString> = row.tip.clone().map(Into::into);
    div()
        .id(("activity-row", i))
        .flex()
        .items_center()
        .h(px(ROW))
        .w_full()
        .pl(px(ROW_LEFT))
        .pr(px(ROW_RIGHT))
        .when_some(tip, |r, tip| {
            r.tooltip(move |_, cx| -> AnyView { cx.new(|_| Tip(tip.clone())).into() })
        })
        .child(
            div()
                .flex_none()
                .w(px(TIME_COLUMN))
                .text_size(px(text::META))
                .font_features(tabular())
                .text_color(hsla(ui.text_faint))
                .child(row.time.clone()),
        )
        .child(
            div()
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .size(px(MARK))
                .child(mark),
        )
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .ml(px(MARK_GAP))
                .truncate()
                .text_size(px(text::NAME))
                .text_color(hsla(ui.text))
                .child(row.text.clone()),
        )
        .into_any_element()
}

/// While it waits: a card under the header (6 pt in from the sides and below the header,
/// padding 8, corners 6, filled `waiting_tint`): `Needs input` (11 pt, `ring_waiting`), then
/// the whole question (13 pt, wrapped).
fn waiting_card(question: Option<String>, ui: &UiColors) -> AnyElement {
    div()
        .flex_none()
        .flex()
        .flex_col()
        .mx(px(CARD_INSET))
        .my(px(CARD_INSET))
        .p(px(CARD_PAD))
        .rounded(px(CARD_CORNER))
        .bg(hsla(ui.waiting_tint))
        .child(
            div()
                .text_size(px(text::META))
                .text_color(hsla(ui.ring_waiting))
                .child(NEEDS_INPUT),
        )
        .children(question.map(|q| {
            div()
                .w_full()
                .text_size(px(text::NAME))
                .text_color(hsla(ui.text))
                .child(q)
        }))
        .into_any_element()
}

/// The header (the Files root header's style, 40 pt): the name (13 pt semibold), under it the
/// status bar's words (11 pt, dim; `ring_waiting` while it waits).
fn activity_header_element(
    label: SharedString,
    words: String,
    waiting: bool,
    ui: &UiColors,
) -> AnyElement {
    let name = div().flex().items_center().h(px(ROOT_LINE)).w_full().child(
        div()
            .flex_1()
            .min_w(px(0.0))
            .truncate()
            .text_size(px(text::NAME))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(hsla(ui.text))
            .child(label),
    );
    let ink = if waiting {
        ui.ring_waiting
    } else {
        ui.text_dim
    };
    let status = div().flex().items_center().h(px(PATH_LINE)).w_full().child(
        div()
            .min_w(px(0.0))
            .truncate()
            .text_size(px(text::META))
            .text_color(hsla(ink))
            .child(words),
    );
    div()
        .flex_none()
        .flex()
        .flex_col()
        .justify_center()
        .h(px(ROOT_HEADER))
        .w_full()
        .pl(px(ROOT_PAD_LEFT))
        .pr(px(ROOT_PAD_RIGHT))
        .child(name)
        .child(status)
        .into_any_element()
}

impl Workspace {
    /// The followed terminal (`right_sidebar.followed`), if there is one: the panel's subject.
    fn activity_session(&self) -> Option<&Session> {
        let id = self.right_sidebar.followed?;
        self.sessions.items().iter().find(|s| s.id == id)
    }

    /// The header's words: the followed terminal's name (`terminal_label`), the status bar's
    /// words for it, and whether those say Needs input. None with no terminal followed.
    pub(crate) fn activity_header(&self) -> Option<(SharedString, String, bool)> {
        let session = self.activity_session()?;
        let facts = self.status_bar_facts()?;
        Some((
            self.terminal_label(session.id),
            facts.text,
            facts.word == StatusWord::NeedsInput,
        ))
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

    /// The timeline's rows `range` (newest first), only those built (`rows_in`).
    fn activity_rows_in(&self, range: std::ops::Range<usize>) -> Vec<ActivityRow> {
        self.activity_session().map_or_else(Vec::new, |s| {
            rows_in(&s.activity, range, |at| {
                let ms = at.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis());
                cli::local_utc_offset(ms as u64)
            })
        })
    }

    /// The Activity panel under the strip: the header and, while it waits, the question's
    /// card; then the timeline (newest first, a `uniform_list` of 22 pt rows), or a note in its
    /// place (`activity_note`). Nothing in it moves, and it asks nothing of git or the disk.
    /// (`_width`: as the Files panel's; nothing here is cut to a measured width.)
    pub(crate) fn render_activity_panel(
        &self,
        ui: UiColors,
        _width: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let header = self
            .activity_header()
            .map(|(label, words, waiting)| activity_header_element(label, words, waiting, &ui));
        let card = self
            .activity_session()
            .filter(|s| s.status == Status::Waiting)
            .map(|s| waiting_card(s.question.clone(), &ui));
        let body = match self.activity_note() {
            Some(text) => note(&ui, text),
            None => {
                let count = self.activity_session().map_or(0, |s| s.activity.len());
                uniform_list(
                    "activity-rows",
                    count,
                    cx.processor(|this, range: std::ops::Range<usize>, _window, cx| {
                        let ui = cx.global::<Appearance>().theme.ui();
                        let first = range.start;
                        this.activity_rows_in(range)
                            .iter()
                            .enumerate()
                            .map(|(k, row)| activity_row(first + k, row, &ui))
                            .collect::<Vec<_>>()
                    }),
                )
                .track_scroll(&self.right_sidebar.activity_scroll)
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
            .children(card)
            .child(div().flex_1().min_h(px(0.0)).w_full().child(body))
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui_tokens::{MARK_MIN, TEXT_MIN, contrast};
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
    fn rows_are_newest_first_with_local_times_and_tips() {
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
        let row = |time: &str, mark, text: &str, tip: Option<&str>| ActivityRow {
            time: time.into(),
            mark,
            text: text.into(),
            tip: tip.map(String::from),
        };
        let at = |min: u64| clock_hm((start + min * 60) as i64, 0);
        assert_eq!(
            rows(&log, |_| 0),
            vec![
                row(
                    &at(18),
                    RowMark::Status(Status::Done),
                    "Done \u{2014} 11 min",
                    None
                ),
                row(&at(9), RowMark::Dot, "migrating", Some("migrating")),
                row(
                    &at(7),
                    RowMark::Status(Status::Waiting),
                    "Needs input",
                    Some("Needs input\nProceed? (y/n)")
                ),
                row(&at(0), RowMark::Status(Status::Working), "Working", None),
            ]
        );
        assert_eq!(at(18), "14:20");
        assert_eq!(at(7), "14:09");
        // The offset is the one at each entry's time (east of UTC).
        let ahead = rows(&log, |at| if at == t(18) { 3600 } else { 0 });
        assert_eq!(ahead[0].time, "15:20");
        assert_eq!(ahead[1].time, "14:11");
        // Progress: a dot, its words in the tooltip (a label can be cut).
        log.progress(Some((42, Some("build".into()))), t(20));
        assert_eq!(
            rows(&log, |_| 0)[0],
            row(
                &at(20),
                RowMark::Dot,
                "Progress 42% \u{b7} build",
                Some("Progress 42% \u{b7} build")
            )
        );
        assert!(rows(&ActivityLog::default(), |_| 0).is_empty());
    }

    /// The panel's words read (4.5:1) and its marks show (3:1) on what they are drawn on, in
    /// every built-in theme: the card (header, rows), and the waiting card's tint.
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
                ("the name", ui.text),
                ("the status words", ui.text_dim),
                ("Needs input in the header", ui.ring_waiting),
                ("a time", ui.text_faint),
            ] {
                check(&format!("{what} on the card"), ink, ui.card, TEXT_MIN);
            }
            for (what, ink) in [("Needs input", ui.ring_waiting), ("the question", ui.text)] {
                check(
                    &format!("{what} on the tint"),
                    ink,
                    ui.waiting_tint,
                    TEXT_MIN,
                );
            }
            for (what, ink) in [
                ("Working's mark", ui.ring_working),
                ("Waiting's mark", ui.ring_waiting),
                ("Done's mark", ui.ring_done),
                ("Idle's ring, a dot", ui.text_faint),
            ] {
                check(&format!("{what} on the card"), ink, ui.card, MARK_MIN);
            }
        }
        assert!(failures.is_empty(), "\n{}", failures.join("\n"));
    }
}
