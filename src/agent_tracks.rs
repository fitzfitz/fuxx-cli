//! A busy step's file tracks in the agent chat (the agent activity chore, approach C): a
//! header, a checks track and a track per file, one bead per call in time order, inside the
//! tool group's bordered shell. Built from `agent_steps::Tracks` (cached on the row), so drawing
//! parses nothing. A submodule of `agent_tab`, like `agent_rows.rs`.

use std::ops::Range;

use super::*;
use crate::agent_phases::{Phase, bands, column_phases, column_tip};
use crate::agent_step_words::head_time;
use crate::agent_steps::{Bead, Glyph, MAX_TRACKS, Track, Tracks};
use crate::theme::mix;
use crate::ui_tokens::chat;

/// The summary column's width while a step shows every column (it scrolls sideways then).
const SUMMARY_W: f32 = 220.0;
/// A call's tile and the icon in it (the 2026-11-24 chores: icons, not shapes).
const TILE: f32 = 16.0;
const TILE_ICON: f32 = 11.0;
/// How much of a call's ink tints its tile, and a phase band's.
const TILE_TINT: f32 = 0.16;
const BAND_TINT: f32 = 0.14;
/// The lane snaps to steps of this many columns, so summaries line up across steps and move at
/// most a few times as a step grows.
const LANE_SNAP: usize = 6;

/// The bead lane's width for `cols` shown columns: snapped up to `LANE_SNAP` columns.
pub fn lane_width(cols: usize) -> f32 {
    (cols.div_ceil(LANE_SNAP) * LANE_SNAP) as f32 * chat::TRACK_COL_W
}

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

/// A call's icon in its tile (the 2026-11-24 chores); none for a running call (its arc).
pub fn glyph_icon(g: Glyph) -> Option<crate::icons::Icon> {
    use crate::icons::Icon;
    Some(match g {
        Glyph::Read => Icon::Eye,
        Glyph::Search => Icon::Search,
        Glyph::Edit => Icon::FilePen,
        Glyph::Script | Glyph::Command => Icon::Terminal,
        Glyph::Passed => Icon::Check,
        Glyph::Failed | Glyph::Planned | Glyph::Cleared => Icon::Stop,
        Glyph::Check => Icon::Circle,
        Glyph::Running => return None,
    })
}

/// A bead's glyph: its shape carries what it means (colour only backs it). The running arc is
/// the call's mark, not text.
pub fn glyph_text(g: Glyph) -> &'static str {
    match g {
        Glyph::Read | Glyph::Command => "●",
        Glyph::Check => "◌",
        Glyph::Search => "▲",
        Glyph::Edit => "■",
        Glyph::Script => "◇",
        Glyph::Passed => "✓",
        Glyph::Failed => "✖",
        Glyph::Cleared => "✕",
        Glyph::Planned => "○",
        Glyph::Running => "",
    }
}

/// Whether a band `cols` columns wide holds its `word` (at the bands' 10.5 pt, ~6 pt a
/// character, and its 4 pt padding each side).
pub fn fits_word(word: &str, cols: usize) -> bool {
    cols as f32 * chat::TRACK_COL_W - 2. >= word.chars().count() as f32 * 6.0 + 8.
}

/// A call's ink by its kind: reads grey, searches the accent, edits the edit ink, commands
/// the text's, passes green, failures red.
fn glyph_ink(g: Glyph, c: &Colors) -> Rgb {
    match g {
        Glyph::Read | Glyph::Check | Glyph::Running => c.panel_dim,
        Glyph::Search => c.track_look,
        Glyph::Edit => c.bead_edit,
        Glyph::Script | Glyph::Command => c.panel_text,
        Glyph::Passed => c.tool_ok,
        Glyph::Failed | Glyph::Planned | Glyph::Cleared => c.failed_panel,
    }
}

/// A phase band's ink: as its calls' (Looking the accent, Ran the text's, Changing the edit
/// ink, Checking green).
fn phase_ink(p: Phase, c: &Colors) -> Rgb {
    match p {
        Phase::Looking => c.track_look,
        Phase::Ran => c.panel_text,
        Phase::Changing => c.bead_edit,
        Phase::Checking => c.tool_ok,
    }
}

/// An unknown check's mark: a drawn hollow ring `size` across in `ink` (the font's ◌ draws as
/// faint dots at bead size).
pub(crate) fn hollow_ring(ink: Rgb, size: f32) -> gpui::Div {
    div()
        .size(px(size))
        .rounded_full()
        .border_1()
        .border_color(hsla(ink))
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
        let beads_w = if all_cols {
            shown.len() as f32 * chat::TRACK_COL_W
        } else {
            lane_width(shown.len())
        };
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
            beads_w,
        };
        if let Some(checks) = &tracks.checks {
            rows.push(self.track_row(0, checks, &place, tracks, look));
        }
        for (n, track) in files.iter().enumerate() {
            rows.push(self.track_row(n + 1, track, &place, tracks, look));
        }
        // "+N files" opens the rest; once open, "fewer" closes them again.
        let files_toggle = match (more, all_files && tracks.files.len() > MAX_TRACKS) {
            (0, false) => None,
            (0, true) => Some("fewer".to_string()),
            (n, _) => Some(format!("+{n} files")),
        };
        if let Some(words) = files_toggle {
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
                        .text_color(hsla(c.panel_text))
                        .cursor_pointer()
                        .child(SharedString::from(words))
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
        if let Some(commands) = &tracks.commands {
            let n = files.len() + 1;
            rows.push(self.track_row(n, commands, &place, tracks, look));
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
                    .left(px(column_x(at, shown.start)))
                    .top(px(chat::TRACK_HEAD))
                    .w(px(chat::TRACK_COL_W))
                    .h(px(chat::TRACK_BANDS + rows_h))
                    .rounded(px(3.))
                    .cursor_pointer()
                    .hover(|s| s.bg(hsla(c.row_hover)))
                    .when(cursor == Some(at), |d| {
                        d.border_1().border_color(hsla(c.track_cursor))
                    })
                    // Its number and what it did (your pick: "C1's hover").
                    .tooltip(tip(column_tip(at, &col.tip)))
                    .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                        cx.stop_propagation();
                        let (step, id) = (step.clone(), id.clone());
                        view.update(cx, |this, cx| this.track_pick(step, at, id, cx))
                            .ok();
                    })
            });
        // The header: the counts, "N failed" in the failure's ink, the time, and "+M earlier"
        // (or "fewer" once every column shows), which toggles the earlier columns.
        let head = {
            let view = look.view.clone();
            let step = lead.clone();
            let sep = crate::agent_ledger::SEP;
            let earlier = (tracks.earlier > 0).then(|| {
                let view = look.view.clone();
                let step = lead.clone();
                let words = if all_cols {
                    "fewer".to_string()
                } else {
                    format!("+{} earlier", tracks.earlier)
                };
                self.marked(
                    "tracks-earlier",
                    div()
                        .id(("agent-tracks-earlier", ix))
                        .text_color(hsla(c.panel_text))
                        .cursor_pointer()
                        .child(SharedString::from(words))
                        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                            cx.stop_propagation();
                            let step = step.clone();
                            view.update(cx, |this, cx| this.toggle_all_columns(step, cx))
                                .ok();
                        }),
                )
            });
            div()
                .id(("agent-tracks-head", ix))
                .h(px(chat::TRACK_HEAD))
                .flex()
                .items_center()
                .whitespace_nowrap()
                .text_color(hsla(c.panel_dim))
                .child(SharedString::from(tracks.counts.clone()))
                .when(tracks.failed > 0, |d| {
                    d.child(
                        div()
                            .text_color(hsla(c.failed_panel))
                            .child(SharedString::from(format!("{sep}{} failed", tracks.failed))),
                    )
                })
                .children(head_time(took).map(|t| SharedString::from(format!("{sep}{t}"))))
                .when_some(earlier, |d, earlier| d.child(sep).child(earlier))
                .child(div().flex_1())
                .children(self.phase_legend(tracks, &shown, c))
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
            .child(self.phase_bands(ix, tracks, &place, c))
            .children(rows);
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

    /// The phases' key at the header's right, when a band is too narrow for its word: a dot and
    /// the word for each phase the step shows, in order.
    fn phase_legend(
        &self,
        tracks: &Tracks,
        shown: &Range<usize>,
        c: &Colors,
    ) -> Option<AnyElement> {
        let all = bands(&column_phases(tracks));
        let visible: Vec<(Phase, usize)> = all
            .iter()
            .filter_map(|(p, run)| {
                let cols = run
                    .end
                    .min(shown.end)
                    .saturating_sub(run.start.max(shown.start));
                (cols > 0).then_some((*p, cols))
            })
            .collect();
        if visible.iter().all(|(p, cols)| fits_word(p.word(), *cols)) {
            return None;
        }
        let mut phases: Vec<Phase> = Vec::new();
        for (p, _) in &visible {
            if !phases.contains(p) {
                phases.push(*p);
            }
        }
        phases.sort_by_key(|p| *p as u8);
        let items = phases.into_iter().map(|p| {
            let ink = phase_ink(p, c);
            div()
                .flex()
                .items_center()
                .gap(px(4.))
                .child(div().size(px(7.)).rounded(px(2.)).bg(hsla(ink)))
                .child(SharedString::from(p.word()))
        });
        Some(
            self.marked(
                "tracks-legend",
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .text_size(px(11.))
                    .children(items),
            )
            .into_any_element(),
        )
    }

    /// The phase bands over the shown columns (your pick, "C2"): Looking around → Ran →
    /// Changing → Checking, one band per run of columns of one phase, its word cut to fit.
    fn phase_bands(&self, ix: usize, tracks: &Tracks, place: &Place, c: &Colors) -> AnyElement {
        let phases = column_phases(tracks);
        let shown = place.shown.clone();
        let bands = bands(&phases).into_iter().filter_map(|(phase, run)| {
            let start = run.start.max(shown.start);
            let end = run.end.min(shown.end);
            (start < end).then(|| {
                let ink = phase_ink(phase, c);
                div()
                    .absolute()
                    .left(px(column_x(start, shown.start) + 1.))
                    .top_0()
                    .w(px((end - start) as f32 * chat::TRACK_COL_W - 2.))
                    .h_full()
                    .rounded(px(4.))
                    .bg(hsla(mix(c.panel, ink, BAND_TINT)))
                    .px(px(4.))
                    .flex()
                    .items_center()
                    .overflow_hidden()
                    .text_size(px(10.5))
                    .text_color(hsla(ink))
                    // Its word where it fits; a one-call band is its colour alone (the
                    // column's hover names the call).
                    .when(fits_word(phase.word(), end - start), |d| {
                        d.child(div().truncate().child(SharedString::from(phase.word())))
                    })
            })
        });
        self.marked(
            "tracks-bands",
            div()
                .id(("agent-tracks-bands", ix))
                .relative()
                .w_full()
                .h(px(chat::TRACK_BANDS))
                .children(bands),
        )
        .into_any_element()
    }

    /// One track: its label (the full path on hover), its calls' icon tiles, and its summary.
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
        let x = |col: usize| (col - place.first) as f32 * chat::TRACK_COL_W;
        let beads = shown.iter().map(|&&(col, bead)| {
            // Each call an icon in a tile tinted by its kind (no rule between them: the
            // columns are the time). A call that itself failed: its icon on a red tile.
            let ink = if bead.failed {
                c.failed_panel
            } else {
                glyph_ink(bead.glyph, c)
            };
            let mark: AnyElement = match glyph_icon(bead.glyph) {
                None => {
                    let id = &tracks.columns[col].id;
                    match self.tool_marks.get(id) {
                        Some(mark) => self
                            .marked("tracks-arc", div().child(mark.clone()))
                            .into_any_element(),
                        None => div().child("●").into_any_element(),
                    }
                }
                Some(icon) => {
                    let tile = div()
                        .size(px(TILE))
                        .rounded(px(4.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(crate::icons::icon(icon, px(TILE_ICON), hsla(ink)));
                    // A planned failure (a test written first) is an outlined tile: expected.
                    let tile = if bead.glyph == Glyph::Planned {
                        tile.border_1().border_color(hsla(ink))
                    } else {
                        tile.bg(hsla(mix(c.panel, ink, TILE_TINT)))
                    };
                    let tile = if bead.glyph == Glyph::Check {
                        self.marked("tracks-unknown", tile)
                    } else {
                        tile
                    };
                    tile.into_any_element()
                }
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
                // Not run (denied, stopped), or a failure a later pass cleared: quieter.
                .when(bead.dim || bead.glyph == Glyph::Cleared, |d| d.opacity(0.5))
                .child(mark)
        });
        let full = if track.path.is_empty() {
            track.label.clone()
        } else {
            track.path.clone()
        };
        // A changed file's label (and the Checks and Commands labels) read; a read-only file's
        // is quiet. A summary that says a failure takes the failure's ink.
        let named = track.path.is_empty();
        let label_ink = if track.changed || named {
            c.panel_text
        } else {
            c.panel_dim
        };
        let summary_ink = if track.failing {
            c.failed_panel
        } else {
            c.panel_dim
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
                    .text_color(hsla(label_ink))
                    .when(named, |d| d.font_weight(FontWeight::MEDIUM))
                    .child(SharedString::from(track.label.clone()))
                    .tooltip(tip(full)),
            )
            .child(
                div()
                    .relative()
                    .flex_none()
                    .w(px(place.beads_w))
                    .h_full()
                    .children(beads),
            )
            .child(
                div()
                    .id(("agent-track-summary", n))
                    .flex_1()
                    .min_w(px(0.))
                    .pl(px(8.))
                    .truncate()
                    .text_color(hsla(summary_ink))
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

    /// ⌥↑ in the box: the shown view's latest busy step takes the keyboard, its cursor on its
    /// latest call (Esc gives it back). Nothing when no step draws as tracks.
    pub(super) fn focus_tracks(&mut self, _: &FocusTracks, _: &mut Window, cx: &mut Context<Self>) {
        let lead = self
            .pane()
            .rows
            .iter()
            .rev()
            .find_map(|r| Some(r.tracks.as_ref()?.columns.first()?.id.clone()));
        match lead {
            Some(lead) => self.track_focus_step(lead, cx),
            None => cx.propagate(),
        }
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

/// Where a step's columns sit: the first shown, the shown range and the bead lane's width.
struct Place {
    first: usize,
    shown: Range<usize>,
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
    fn the_lane_snaps_to_six_columns() {
        assert_eq!(lane_width(1), 6.0 * chat::TRACK_COL_W);
        assert_eq!(lane_width(6), 6.0 * chat::TRACK_COL_W);
        assert_eq!(lane_width(7), 12.0 * chat::TRACK_COL_W);
        assert_eq!(lane_width(24), 24.0 * chat::TRACK_COL_W);
    }

    #[test]
    fn the_cursor_moves_within_the_shown_columns() {
        assert_eq!(cursor_moved(None, 1, 36..60), 59);
        assert_eq!(cursor_moved(Some(36), -1, 36..60), 36);
        assert_eq!(cursor_moved(Some(59), 1, 36..60), 59);
        assert_eq!(cursor_moved(Some(40), 1, 36..60), 41);
    }

    /// Each call is a real icon (the 2026-11-24 chores: "change it into icon and make it easily
    /// readable"): an eye reads, a magnifier searches, a pen edits, a terminal runs, a check
    /// passed, an ✕ failed; the running arc is the call's own mark.
    #[test]
    fn a_band_shows_its_word_only_where_it_fits() {
        assert!(!fits_word("Ran", 1));
        assert!(fits_word("Ran", 2));
        assert!(!fits_word("Looking around", 4));
        assert!(fits_word("Looking around", 6));
    }

    #[test]
    fn every_glyph_but_the_arc_has_an_icon() {
        use crate::icons::Icon;
        assert_eq!(
            [
                Glyph::Read,
                Glyph::Search,
                Glyph::Edit,
                Glyph::Script,
                Glyph::Command,
                Glyph::Passed,
                Glyph::Failed,
                Glyph::Planned,
                Glyph::Cleared,
                Glyph::Check,
            ]
            .map(glyph_icon),
            [
                Some(Icon::Eye),
                Some(Icon::Search),
                Some(Icon::FilePen),
                Some(Icon::Terminal),
                Some(Icon::Terminal),
                Some(Icon::Check),
                Some(Icon::Stop),
                Some(Icon::Stop),
                Some(Icon::Stop),
                Some(Icon::Circle),
            ]
        );
        assert_eq!(glyph_icon(Glyph::Running), None);
    }
}
