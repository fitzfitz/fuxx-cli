//! A busy step's file tracks in the agent chat (the agent activity chore, approach C): a
//! header, a checks track and a track per file, one bead per call in time order, inside the
//! tool group's bordered shell. Built from `agent_steps::Tracks` (cached on the row), so drawing
//! parses nothing. A submodule of `agent_tab`, like `agent_rows.rs`.

use std::ops::Range;

use super::*;
use crate::agent_step_words::header;
use crate::agent_steps::{Bead, Glyph, Track, Tracks};
use crate::ui_tokens::chat;

/// The summary column's width while a step shows every column (it scrolls sideways then).
const SUMMARY_W: f32 = 220.0;
/// A bead's glyph size: a little over the bead, so a text glyph fills it.
const GLYPH: f32 = chat::BEAD + 2.0;

/// Whether the chat column (the tab's width, up to `COLUMN_MAX`) holds tracks; under
/// `TRACKS_NARROW` a busy step falls back to rows.
pub fn fits_tracks(bar_width: f32) -> bool {
    bar_width.min(chat::COLUMN_MAX) >= chat::TRACKS_NARROW
}

/// Column `col`'s left edge in a step's body, when its shown columns start at `first_shown`.
pub fn column_x(col: usize, first_shown: usize) -> f32 {
    chat::TRACK_LABEL_W + col.saturating_sub(first_shown) as f32 * chat::TRACK_COL_W
}

/// The cursor's column after ←/→ (`dir` −1 / 1), kept within the `shown` columns; no cursor yet
/// starts at the last shown (the latest call).
pub fn cursor_moved(col: Option<usize>, dir: i8, shown: Range<usize>) -> usize {
    let last = shown.end.saturating_sub(1);
    match col {
        None => last,
        Some(c) => {
            let c = c.clamp(shown.start, last);
            if dir < 0 {
                c.saturating_sub(1).max(shown.start)
            } else {
                (c + 1).min(last)
            }
        }
    }
}

/// A bead's glyph: its shape carries what it means (colour only backs it). The running arc is
/// the call's mark, not text.
pub fn glyph_text(g: Glyph) -> &'static str {
    match g {
        Glyph::Read | Glyph::Check => "●",
        Glyph::Search => "▲",
        Glyph::Edit => "■",
        Glyph::Script => "◆",
        Glyph::Passed => "✓",
        Glyph::Failed => "✕",
        Glyph::Planned => "○",
        Glyph::Running => "",
    }
}

/// The keyboard's column in a step's tracks: the step (its first call's id) and the column.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrackCursor {
    pub lead: String,
    pub col: usize,
}

/// A hover tooltip with `words`.
fn tip(words: String) -> impl Fn(&mut Window, &mut App) -> gpui::AnyView + 'static {
    move |_, cx| {
        cx.new(|_| crate::sidebar_toggle::Tip(words.clone().into()))
            .into()
    }
}

impl AgentTab {
    /// A busy step's tracks (item `ix`, its first call): a header, the checks track and the
    /// file tracks (beads at their calls' columns, a summary each), "+N files", "+M earlier",
    /// and the step's open failure under the group. Hover a column for its call, click it for
    /// the call's detail; ←/→ and Enter once the group has the keyboard.
    pub(super) fn tracks(&self, ix: usize, tracks: &Tracks, look: &Look) -> AnyElement {
        let c = &look.c;
        let Some(lead) = tracks.columns.first().map(|col| col.id.clone()) else {
            return div().into_any_element();
        };
        let all_cols = self.tracks_all.contains(&lead);
        let all_files = self.files_all.contains(&lead);
        let shown = tracks.shown_columns(all_cols);
        let earlier_w = if tracks.earlier > 0 && !all_cols {
            chat::TRACK_EARLIER_W
        } else {
            0.0
        };
        let beads_w = earlier_w + shown.len() as f32 * chat::TRACK_COL_W;
        let cursor = self
            .track_cursor
            .as_ref()
            .filter(|t| t.lead == lead)
            .map(|t| t.col);
        let took = tracks
            .span
            .map(|(start, end)| end.unwrap_or(look.now).saturating_duration_since(start));
        let (files, more) = tracks.shown_files(all_files);
        let mut rows: Vec<AnyElement> = Vec::new();
        let place = Place {
            first: shown.start,
            shown: shown.clone(),
            earlier_w,
            beads_w,
        };
        if let Some(checks) = &tracks.checks {
            rows.push(self.track_row(0, checks, &place, tracks, look));
        }
        for (n, track) in files.iter().enumerate() {
            rows.push(self.track_row(n + 1, track, &place, tracks, look));
        }
        if more > 0 {
            let view = look.view.clone();
            let step = lead.clone();
            rows.push(
                self.marked(
                    "tracks-more-files",
                    div()
                        .id(("agent-tracks-more", ix))
                        .h(px(chat::TRACK_ROW))
                        .flex()
                        .items_center()
                        .text_color(hsla(c.panel_dim))
                        .cursor_pointer()
                        .hover(|s| s.text_color(hsla(c.panel_text)))
                        .child(SharedString::from(format!("+{more} files")))
                        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                            cx.stop_propagation();
                            let step = step.clone();
                            view.update(cx, |this, cx| this.toggle_all_files(step, cx))
                                .ok();
                        }),
                )
                .into_any_element(),
            );
        }
        let rows_h = rows.len() as f32 * chat::TRACK_ROW;
        // The columns: a strip each under the tracks (hover, click, the cursor's ring).
        let strips = tracks.columns[shown.clone()]
            .iter()
            .enumerate()
            .map(|(n, col)| {
                let at = shown.start + n;
                let view = look.view.clone();
                let (step, id) = (lead.clone(), col.id.clone());
                div()
                    .id(("agent-track-col", at))
                    .absolute()
                    .left(px(column_x(at, shown.start) + earlier_w))
                    .top(px(chat::TRACK_HEAD))
                    .w(px(chat::TRACK_COL_W))
                    .h(px(rows_h))
                    .rounded(px(3.))
                    .cursor_pointer()
                    .hover(|s| s.bg(hsla(c.row_hover)))
                    .when(cursor == Some(at), |d| {
                        d.border_1().border_color(hsla(c.track_cursor))
                    })
                    .tooltip(tip(col.tip.clone()))
                    .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                        cx.stop_propagation();
                        let (step, id) = (step.clone(), id.clone());
                        view.update(cx, |this, cx| this.track_pick(step, at, id, cx))
                            .ok();
                    })
            });
        let earlier = (earlier_w > 0.0).then(|| {
            let view = look.view.clone();
            let step = lead.clone();
            self.marked(
                "tracks-earlier",
                div()
                    .id(("agent-tracks-earlier", ix))
                    .absolute()
                    .left(px(chat::TRACK_LABEL_W))
                    .top(px(chat::TRACK_HEAD))
                    .w(px(earlier_w))
                    .h(px(chat::TRACK_ROW))
                    .flex()
                    .items_center()
                    .text_color(hsla(c.panel_dim))
                    .cursor_pointer()
                    .hover(|s| s.text_color(hsla(c.panel_text)))
                    .child(SharedString::from(format!("+{} earlier", tracks.earlier)))
                    .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                        cx.stop_propagation();
                        let step = step.clone();
                        view.update(cx, |this, cx| this.toggle_all_columns(step, cx))
                            .ok();
                    }),
            )
        });
        let head = {
            let view = look.view.clone();
            let step = lead.clone();
            div()
                .id(("agent-tracks-head", ix))
                .h(px(chat::TRACK_HEAD))
                .flex()
                .items_center()
                .text_color(hsla(c.panel_dim))
                .child(SharedString::from(header(&tracks.counts, took)))
                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                    cx.stop_propagation();
                    let step = step.clone();
                    view.update(cx, |this, cx| this.track_focus_step(step, cx))
                        .ok();
                })
        };
        let body = div()
            .relative()
            .flex()
            .flex_col()
            .when(all_cols, |d| {
                d.w(px(chat::TRACK_LABEL_W + beads_w + SUMMARY_W))
            })
            .when(!all_cols, |d| d.w_full())
            .children(strips)
            .child(head)
            .children(rows)
            .children(earlier);
        let body: AnyElement = if all_cols {
            div()
                .id(("agent-tracks-scroll", ix))
                .w_full()
                .overflow_x_scroll()
                .child(body)
                .into_any_element()
        } else {
            body.into_any_element()
        };
        let keyed = cursor.is_some();
        let group = div()
            .id(("agent-tracks", ix))
            .w_full()
            .px(px(chat::TOOL_PAD_X))
            .pb(px(4.))
            .bg(hsla(c.panel))
            .border_1()
            .border_color(hsla(c.panel_border))
            .rounded(px(chat::R_GROUP))
            .text_size(px(text::META))
            .font_features(crate::sidebar::tabular())
            .when(keyed, |d| {
                d.key_context("AgentTracks").track_focus(&self.tracks_focus)
            })
            .child(body);
        let first_error = tracks.first_error.clone().map(|line| {
            self.marked(
                "tracks-first-error",
                div()
                    .id(("agent-tracks-error", ix))
                    .pt(px(4.))
                    .pl(px(chat::TOOL_PAD_X))
                    .truncate()
                    .text_size(px(text::META))
                    .text_color(hsla(c.failed))
                    .child(SharedString::from(line.clone()))
                    .tooltip(tip(line)),
            )
        });
        div()
            .w_full()
            .flex()
            .flex_col()
            .child(self.marked("tracks-group", group))
            .children(first_error)
            .into_any_element()
    }

    /// One track: its label (the full path on hover), its beads with the rule between them,
    /// and its summary.
    fn track_row(
        &self,
        n: usize,
        track: &Track,
        place: &Place,
        tracks: &Tracks,
        look: &Look,
    ) -> AnyElement {
        let c = &look.c;
        let shown: Vec<&(usize, Bead)> = track
            .beads
            .iter()
            .filter(|(col, _)| place.shown.contains(col))
            .collect();
        let x = |col: usize| place.earlier_w + (col - place.first) as f32 * chat::TRACK_COL_W;
        let rule = match (shown.first(), shown.last()) {
            (Some((a, _)), Some((b, _))) if a != b => Some(
                div()
                    .absolute()
                    .left(px(x(*a) + chat::TRACK_COL_W / 2.))
                    .w(px(x(*b) - x(*a)))
                    .top(px(chat::TRACK_ROW / 2.))
                    .h(px(1.))
                    .bg(hsla(c.track_rule)),
            ),
            _ => None,
        };
        let beads = shown.iter().map(|&&(col, bead)| {
            let ink = if bead.dim {
                c.panel_dim
            } else {
                match bead.glyph {
                    Glyph::Edit => c.bead_edit,
                    Glyph::Passed => c.tool_ok,
                    Glyph::Failed | Glyph::Planned => c.failed_panel,
                    _ => c.bead,
                }
            };
            let mark: AnyElement = match bead.glyph {
                Glyph::Running => {
                    let id = &tracks.columns[col].id;
                    match self.tool_marks.get(id) {
                        Some(mark) => self
                            .marked("tracks-arc", div().child(mark.clone()))
                            .into_any_element(),
                        None => div().child("●").into_any_element(),
                    }
                }
                g => div().child(glyph_text(g)).into_any_element(),
            };
            div()
                .absolute()
                .left(px(x(col)))
                .top_0()
                .w(px(chat::TRACK_COL_W))
                .h_full()
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(GLYPH))
                .line_height(relative(1.))
                .text_color(hsla(ink))
                .child(mark)
                // A call that itself failed: a small red ✕ on its bead.
                .when(bead.failed, |d| {
                    d.child(
                        div()
                            .absolute()
                            .top(px(2.))
                            .right(px(0.))
                            .text_size(px(7.))
                            .text_color(hsla(c.failed_panel))
                            .child("✕"),
                    )
                })
        });
        let full = if track.path.is_empty() {
            track.label.clone()
        } else {
            track.path.clone()
        };
        div()
            .h(px(chat::TRACK_ROW))
            .flex()
            .items_center()
            .child(
                div()
                    .id(("agent-track-label", n))
                    .flex_none()
                    .w(px(chat::TRACK_LABEL_W))
                    .pr(px(8.))
                    .truncate()
                    .text_color(hsla(c.panel_text))
                    .child(SharedString::from(track.label.clone()))
                    .tooltip(tip(full)),
            )
            .child(
                div()
                    .relative()
                    .flex_none()
                    .w(px(place.beads_w))
                    .h_full()
                    .children(rule)
                    .children(beads),
            )
            .child(
                div()
                    .id(("agent-track-summary", n))
                    .flex_1()
                    .min_w(px(0.))
                    .pl(px(8.))
                    .truncate()
                    .text_color(hsla(c.panel_dim))
                    .child(SharedString::from(track.summary.clone()))
                    .tooltip(tip(track.summary.clone())),
            )
            .into_any_element()
    }

    /// The step whose first call is `lead`, as the shown view's rows hold it.
    fn step_tracks(&self, lead: &str) -> Option<Rc<Tracks>> {
        self.pane()
            .rows
            .iter()
            .filter_map(|r| r.tracks.clone())
            .find(|t| t.columns.first().is_some_and(|c| c.id == lead))
    }

    /// A click on a column: the cursor goes there and the call's detail opens.
    fn track_pick(&mut self, lead: String, col: usize, id: String, cx: &mut Context<Self>) {
        self.track_cursor = Some(TrackCursor { lead, col });
        self.open_tool(id, cx);
    }

    /// A click on a step's header: it takes the keyboard, the cursor on its latest call.
    fn track_focus_step(&mut self, lead: String, cx: &mut Context<Self>) {
        let Some(t) = self.step_tracks(&lead) else {
            return;
        };
        let all = self.tracks_all.contains(&lead);
        let col = cursor_moved(None, 1, t.shown_columns(all));
        self.track_cursor = Some(TrackCursor { lead, col });
        self.refocus = Some(Refocus::Tracks);
        cx.notify();
    }

    /// "+M earlier": the step shows every column (scrolling sideways), or the latest again.
    fn toggle_all_columns(&mut self, lead: String, cx: &mut Context<Self>) {
        if !self.tracks_all.remove(&lead) {
            self.tracks_all.insert(lead);
        }
        cx.notify();
    }

    /// "+N files": the step shows every file track, or the top ones again.
    fn toggle_all_files(&mut self, lead: String, cx: &mut Context<Self>) {
        if !self.files_all.remove(&lead) {
            self.files_all.insert(lead);
        }
        cx.notify();
    }

    /// ←/→ in a step's tracks.
    fn track_move(&mut self, dir: i8, cx: &mut Context<Self>) {
        let Some(cur) = self.track_cursor.clone() else {
            return;
        };
        let Some(t) = self.step_tracks(&cur.lead) else {
            return;
        };
        let all = self.tracks_all.contains(&cur.lead);
        let col = cursor_moved(Some(cur.col), dir, t.shown_columns(all));
        self.track_cursor = Some(TrackCursor { col, ..cur });
        cx.notify();
    }

    pub(super) fn track_prev(&mut self, _: &TrackPrev, _: &mut Window, cx: &mut Context<Self>) {
        self.track_move(-1, cx);
    }

    pub(super) fn track_next(&mut self, _: &TrackNext, _: &mut Window, cx: &mut Context<Self>) {
        self.track_move(1, cx);
    }

    /// Enter: the cursor's call's detail.
    pub(super) fn track_open(&mut self, _: &TrackOpen, _: &mut Window, cx: &mut Context<Self>) {
        let Some(cur) = self.track_cursor.clone() else {
            return;
        };
        let id = self
            .step_tracks(&cur.lead)
            .and_then(|t| t.columns.get(cur.col).map(|c| c.id.clone()));
        if let Some(id) = id {
            self.open_tool(id, cx);
        }
    }

    /// Esc: the tracks give the keyboard back to the box.
    pub(super) fn track_leave(&mut self, _: &TrackLeave, _: &mut Window, cx: &mut Context<Self>) {
        self.track_cursor = None;
        self.refocus = Some(Refocus::Box);
        cx.notify();
    }

    /// The shown view's step models, in order (the screenshot scenes read them).
    #[cfg(feature = "ui-shots")]
    pub fn shot_tracks(&self) -> Vec<Rc<Tracks>> {
        self.pane()
            .rows
            .iter()
            .filter_map(|r| r.tracks.clone())
            .collect()
    }
}

#[cfg(feature = "ui-shots")]
impl AgentTab {
    /// How many steps show every column and every file, and the keyboard's column (the
    /// scenes read it).
    pub fn shot_track_state(&self) -> (usize, usize, Option<usize>) {
        (
            self.tracks_all.len(),
            self.files_all.len(),
            self.track_cursor.as_ref().map(|t| t.col),
        )
    }
}

/// Where a step's columns sit: the first shown, the shown range, the "+M earlier" cell's width
/// and the bead area's.
struct Place {
    first: usize,
    shown: Range<usize>,
    earlier_w: f32,
    beads_w: f32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_steps::Glyph;
    use crate::ui_tokens::chat;

    #[test]
    fn tracks_fall_back_to_rows_under_560() {
        assert!(!fits_tracks(559.0));
        assert!(fits_tracks(560.0));
        assert!(fits_tracks(1400.0));
    }

    #[test]
    fn columns_sit_after_the_label_column() {
        assert_eq!(column_x(36, 36), chat::TRACK_LABEL_W);
        assert_eq!(column_x(37, 36), chat::TRACK_LABEL_W + chat::TRACK_COL_W);
    }

    #[test]
    fn the_cursor_moves_within_the_shown_columns() {
        assert_eq!(cursor_moved(None, 1, 36..60), 59);
        assert_eq!(cursor_moved(Some(36), -1, 36..60), 36);
        assert_eq!(cursor_moved(Some(59), 1, 36..60), 59);
        assert_eq!(cursor_moved(Some(40), 1, 36..60), 41);
    }

    #[test]
    fn every_glyph_but_the_arc_has_its_shape() {
        assert_eq!(
            [
                Glyph::Read,
                Glyph::Search,
                Glyph::Edit,
                Glyph::Script,
                Glyph::Passed,
                Glyph::Failed,
                Glyph::Planned
            ]
            .map(glyph_text),
            ["●", "▲", "■", "◆", "✓", "✕", "○"]
        );
        assert_eq!(glyph_text(Glyph::Running), "");
    }
}
