//! The Changes tab's GRAPH accordion (M11w, note 74): the commit history with coloured lanes
//! (`graph_layout`), branch pills, a faint author and a hover card (author, when, the whole
//! message, what it changed, the hash, Open on GitHub), under STAGED and CHANGES. Its header:
//! the scope (Auto → All → Current), ◎ go to HEAD, and Refresh. Read with the Changes panel's
//! refresh, only while it shows and GRAPH is open, and only when a ref moved (`stamp`).

use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use alacritty_terminal::vte::ansi::Rgb;
use gpui::{
    Anchor, AnyElement, ClickEvent, ClipboardItem, Context, FontWeight, MouseButton,
    MouseDownEvent, PathBuilder, SharedString, anchored, canvas, deferred, div, point, prelude::*,
    px,
};

use crate::activity::{civil_date, local_day};
use crate::git_branch::run_git;
use crate::git_graph::{self, Commit, Details, Ref, Scope};
use crate::graph_layout::{self, Row};
use crate::right_sidebar::{EDGE, PILL_INSET, PILL_PAD, PILL_RADIUS, ROW, SMALL, TEXT};
use crate::status::age_label;
use crate::tab_bar::{Glyph, glyph};
use crate::terminal_view::hsla;
use crate::theme::{Theme, UiColors, mix, readable};
use crate::ui_tokens::MARK_MIN;
use crate::workspace::Workspace;

/// Commits read at a time; "Load more" adds as many.
pub(crate) const PAGE: usize = 200;
/// A lane's column, the most columns drawn, a dot's radius, HEAD's ring, a line's width.
const LANE: f32 = 12.0;
const LANE_CAP: usize = 6;
const DOT: f32 = 3.5;
const HEAD_RING: f32 = 5.0;
const LINE: f32 = 1.75;
/// The hover card's width and how long the pointer rests first.
const CARD_WIDTH: f32 = 380.0;
const CARD_DELAY: Duration = Duration::from_millis(400);
/// The least room a subject keeps on a row with pills (the author gives way first).
const SUBJECT_MIN: f32 = 90.0;
/// The most pills on a row; the rest are `+N`.
const PILLS: usize = 2;

/// The GRAPH's state on the Changes panel.
pub(crate) struct GraphState {
    pub open: bool,
    pub scope: Scope,
    pub limit: usize,
    pub commits: Vec<Commit>,
    pub rows: Vec<Row>,
    /// The lane columns drawn (at least 1, at most `LANE_CAP`).
    pub columns: usize,
    /// What the commits were read for (the refs, the scope, the page); none: read again.
    pub stamp: Option<String>,
    /// Origin's URL (for Open on GitHub).
    pub origin: Option<String>,
    /// The row under the pointer, and whether its card shows yet.
    pub hover: Option<(usize, bool)>,
    /// The details read for a hash (none inside: git couldn't read it).
    pub details: Option<(String, Option<Details>)>,
}

impl Default for GraphState {
    fn default() -> Self {
        Self {
            // Closed until you open it (your call, M11x); its commits are read only then.
            open: false,
            scope: Scope::Auto,
            limit: PAGE,
            commits: Vec::new(),
            rows: Vec::new(),
            columns: 1,
            stamp: None,
            origin: None,
            hover: None,
            details: None,
        }
    }
}

impl GraphState {
    /// Take a read: the commits, laid out.
    pub(crate) fn apply(&mut self, stamp: String, commits: Vec<Commit>, origin: Option<String>) {
        let ids: Vec<(String, Vec<String>)> = commits
            .iter()
            .map(|c| (c.hash.clone(), c.parents.clone()))
            .collect();
        self.rows = graph_layout::layout(&ids, LANE_CAP);
        self.columns = columns_of(&self.rows);
        self.commits = commits;
        self.stamp = Some(stamp);
        self.origin = origin;
        self.hover = None;
    }

    /// Forget what was read (another root, or the scope or page changed).
    pub(crate) fn forget(&mut self) {
        self.stamp = None;
        self.commits.clear();
        self.rows.clear();
        self.columns = 1;
        self.hover = None;
        self.details = None;
    }
}

/// The lane columns the rows need: one past the rightmost lane any row draws in.
pub fn columns_of(rows: &[Row]) -> usize {
    rows.iter()
        .flat_map(|r| {
            std::iter::once(r.lane)
                .chain(r.into.iter().copied())
                .chain(r.out.iter().copied())
                .chain(r.through.iter().copied())
        })
        .max()
        .map_or(1, |l| (l + 1).min(LANE_CAP))
}

/// The scope after `scope` (the header's button cycles them).
pub fn next_scope(scope: Scope) -> Scope {
    match scope {
        Scope::Auto => Scope::All,
        Scope::All => Scope::Current,
        Scope::Current => Scope::Auto,
    }
}

/// The scope button's word.
pub fn scope_label(scope: Scope) -> &'static str {
    match scope {
        Scope::Auto => "Auto",
        Scope::All => "All",
        Scope::Current => "Current",
    }
}

/// The pills a row shows: at most `PILLS`, the branch checked out first, and how many more.
pub fn shown_refs(refs: &[Ref]) -> (Vec<Ref>, usize) {
    let mut ordered: Vec<Ref> = refs
        .iter()
        .filter(|r| matches!(r, Ref::Head(_)))
        .cloned()
        .collect();
    ordered.extend(refs.iter().filter(|r| !matches!(r, Ref::Head(_))).cloned());
    let more = ordered.len().saturating_sub(PILLS);
    ordered.truncate(PILLS);
    (ordered, more)
}

/// The card's "when": `1h ago (November 12, 2026 at 10:52 AM)`, at `utc_offset` seconds east
/// of UTC.
pub fn card_when(time: i64, now: i64, utc_offset: i64) -> String {
    const MONTHS: [&str; 12] = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    let ago = (now - time).max(0) as u64;
    let rel = if ago < 60 {
        "just now".to_string()
    } else {
        format!("{} ago", age_label(Duration::from_secs(ago)))
    };
    let (year, month, day) = civil_date(local_day(time, utc_offset));
    let secs = (time + utc_offset).rem_euclid(86_400);
    let (hour, minute) = (secs / 3_600, secs % 3_600 / 60);
    let (h12, half) = match hour {
        0 => (12, "AM"),
        1..=11 => (hour, "AM"),
        12 => (12, "PM"),
        _ => (hour - 12, "PM"),
    };
    let month = MONTHS[(month as usize).saturating_sub(1).min(11)];
    format!("{rel} ({month} {day}, {year} at {h12}:{minute:02} {half})")
}

/// What a read is for: the refs (HEAD and every branch's commit), the scope and the page.
/// None when git fails. Blocking.
pub fn stamp(git: &Path, root: &Path, scope: Scope, limit: usize) -> Option<String> {
    let head = run_git(git, root, &["rev-parse", "-q", "--verify", "HEAD"]).unwrap_or_default();
    let refs = run_git(
        git,
        root,
        &[
            "for-each-ref",
            "--format=%(objectname)",
            "refs/heads",
            "refs/remotes",
        ],
    )?;
    Some(format!("{scope:?}/{limit}/{}/{refs}", head.trim()))
}

/// The commits again, when the refs, the scope or the page moved since `seen`: (the new stamp,
/// the commits, origin's URL). None: nothing moved, or git failed. Blocking.
pub fn read_if_changed(
    git: &Path,
    root: &Path,
    scope: Scope,
    limit: usize,
    seen: Option<&str>,
) -> Option<(String, Vec<Commit>, Option<String>)> {
    let now = stamp(git, root, scope, limit)?;
    if seen == Some(now.as_str()) {
        return None;
    }
    let commits = git_graph::read(git, root, scope, limit)?;
    let origin = run_git(git, root, &["remote", "get-url", "origin"]).map(|u| u.trim().to_string());
    Some((now, commits, origin))
}

/// The lanes' inks: the theme's ANSI colours (blue, magenta, green, yellow, cyan, red), each
/// pushed to `MARK_MIN` on the panel.
pub fn lane_inks(theme: &Theme) -> Vec<Rgb> {
    let ui = theme.ui();
    [4, 5, 2, 3, 6, 1]
        .into_iter()
        .map(|i| readable(theme.palette[i], &[ui.card, ui.card_hover], MARK_MIN))
        .collect()
}

fn ink(inks: &[Rgb], lane: usize) -> Rgb {
    inks[lane % inks.len()]
}

/// A row's lanes, drawn in a canvas `columns` wide: lines passing through, the lines coming
/// into the dot and leaving it (curves when they change column), and the dot (a ring for a
/// merge, a larger hollow ring for HEAD).
fn lanes(row: &Row, columns: usize, head: bool, inks: Vec<Rgb>, fill: Rgb) -> impl IntoElement {
    let row = row.clone();
    div()
        .flex_none()
        .w(px(columns as f32 * LANE))
        .h(px(ROW))
        .child(
            canvas(
                |_, _, _| (),
                move |bounds, _, window, _| {
                    let o = bounds.origin;
                    let h = bounds.size.height;
                    let x = |l: usize| o.x + px(l as f32 * LANE + LANE / 2.0);
                    let (top, mid, bottom) = (o.y, o.y + h / 2.0, o.y + h);
                    let mut paint = |path: PathBuilder, c: Rgb| {
                        if let Ok(path) = path.build() {
                            window.paint_path(path, hsla(c));
                        }
                    };
                    for &j in &row.through {
                        let mut p = PathBuilder::stroke(px(LINE));
                        p.move_to(point(x(j), top));
                        p.line_to(point(x(j), bottom));
                        paint(p, ink(&inks, j));
                    }
                    for &j in &row.into {
                        let mut p = PathBuilder::stroke(px(LINE));
                        p.move_to(point(x(j), top));
                        if j == row.lane {
                            p.line_to(point(x(j), mid));
                        } else {
                            p.cubic_bezier_to(
                                point(x(row.lane), mid),
                                point(x(j), mid),
                                point(x(row.lane), top),
                            );
                        }
                        paint(p, ink(&inks, j));
                    }
                    for &k in &row.out {
                        let mut p = PathBuilder::stroke(px(LINE));
                        p.move_to(point(x(row.lane), mid));
                        if k == row.lane {
                            p.line_to(point(x(k), bottom));
                        } else {
                            p.cubic_bezier_to(
                                point(x(k), bottom),
                                point(x(row.lane), bottom),
                                point(x(k), mid),
                            );
                        }
                        paint(p, ink(&inks, k));
                    }
                    let c = ink(&inks, row.lane);
                    let circle = |r: f32, filled: bool, w: f32| {
                        let mut p = if filled {
                            PathBuilder::fill()
                        } else {
                            PathBuilder::stroke(px(w))
                        };
                        let (cx, cy) = (x(row.lane), mid);
                        let radii = point(px(r), px(r));
                        p.move_to(point(cx, cy - px(r)));
                        p.arc_to(radii, px(0.0), false, true, point(cx, cy + px(r)));
                        p.arc_to(radii, px(0.0), false, true, point(cx, cy - px(r)));
                        p.close();
                        p
                    };
                    if head {
                        paint(circle(HEAD_RING, true, 0.0), fill);
                        paint(circle(HEAD_RING, false, 2.0), c);
                    } else if row.merge {
                        paint(circle(DOT + 1.0, true, 0.0), fill);
                        paint(circle(DOT + 1.0, false, 1.75), c);
                    } else {
                        paint(circle(DOT, true, 0.0), c);
                    }
                },
            )
            .size_full(),
        )
}

impl Workspace {
    /// The GRAPH header row: a thin chevron, `GRAPH`, a hairline; the scope button, ◎ go to
    /// HEAD, Refresh. A click elsewhere on it opens or closes the accordion.
    pub(crate) fn graph_header_row(&self, ui: &UiColors, cx: &mut Context<Self>) -> AnyElement {
        let ui = *ui;
        let graph = &self.right_sidebar.changes.graph;
        let (open, scope) = (graph.open, graph.scope);
        let button = |id: &'static str| {
            div()
                .id(id)
                .flex_none()
                .flex()
                .items_center()
                .gap(px(4.0))
                .h(px(22.0))
                .px(px(5.0))
                .rounded(px(5.0))
                .cursor_pointer()
                .text_size(px(SMALL))
                .text_color(hsla(ui.text_dim))
                .hover(move |s| s.bg(hsla(ui.card_hover)))
                .on_mouse_down(MouseButton::Left, |_: &MouseDownEvent, _, cx| {
                    cx.stop_propagation()
                })
        };
        div()
            .id("graph-header")
            .flex()
            .items_end()
            .gap(px(6.0))
            .h(px(ROW))
            .w_full()
            .pl(px(EDGE))
            .pr(px(EDGE - 4.0))
            .pb(px(2.0))
            .cursor_pointer()
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                let graph = &mut this.right_sidebar.changes.graph;
                graph.open = !graph.open;
                if graph.open {
                    graph.stamp = None;
                    this.right_sidebar.changes.refreshed = None;
                    this.refresh_changes(cx);
                }
                cx.notify();
            }))
            .child(
                div()
                    .flex_none()
                    .h(px(20.0))
                    .flex()
                    .items_center()
                    .child(crate::right_sidebar::thin_chevron(open, ui.text_faint)),
            )
            .child(
                div()
                    .flex_none()
                    .h(px(20.0))
                    .flex()
                    .items_center()
                    .text_size(px(SMALL))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(hsla(ui.text_faint))
                    .child("GRAPH"),
            )
            .child(div().flex_1().h(px(1.0)).mb(px(9.0)).bg(hsla(ui.divider)))
            .when(open, |d| {
                d.child(
                    button("graph-scope")
                        .child(glyph(Glyph::Branch, ui.text_dim))
                        .child(scope_label(scope))
                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                            cx.stop_propagation();
                            let graph = &mut this.right_sidebar.changes.graph;
                            graph.scope = next_scope(graph.scope);
                            graph.limit = PAGE;
                            graph.forget();
                            this.right_sidebar.changes.refreshed = None;
                            this.refresh_changes(cx);
                            cx.notify();
                        })),
                )
                .child(button("graph-head").child("\u{25ce}").on_click(cx.listener(
                    |this, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        this.right_sidebar.changes.scroll_to_graph_head();
                        cx.notify();
                    },
                )))
                .child(
                    button("graph-refresh")
                        .child(glyph(Glyph::Reload, ui.text_dim))
                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                            cx.stop_propagation();
                            this.right_sidebar.changes.graph.stamp = None;
                            this.right_sidebar.changes.refreshed = None;
                            this.refresh_changes(cx);
                        })),
                )
            })
            .into_any_element()
    }

    /// Commit `i`'s row: its lanes, its subject (bold at HEAD), its pills, its author (faint);
    /// on hover, after a moment, its card to the left of the panel.
    pub(crate) fn graph_commit_row(
        &self,
        i: usize,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let graph = &self.right_sidebar.changes.graph;
        let commit = graph.commits.get(i)?;
        let row = graph.rows.get(i)?;
        let ui = theme.ui();
        let inks = lane_inks(theme);
        let (pills, more) = shown_refs(&commit.refs);
        // The author only on a row without pills (as Cursor shows it): no room for both.
        let author = pills.is_empty().then(|| commit.author.clone());
        let lane_ink = ink(&inks, row.lane);
        let shown = graph.hover == Some((i, true));
        let card = shown.then(|| self.graph_card(commit, theme, cx));
        let pill = |text: String, bg: Rgb, fg: Rgb| {
            div()
                .flex_shrink(1.0)
                .min_w(px(40.0))
                .max_w(px(110.0))
                .truncate()
                .px(px(7.0))
                .py(px(1.0))
                .rounded(px(9.0))
                .text_size(px(SMALL))
                .font_weight(FontWeight::SEMIBOLD)
                .bg(hsla(bg))
                .text_color(hsla(fg))
                .child(text)
        };
        let pills: Vec<_> = pills
            .into_iter()
            .map(|r| match r {
                Ref::Head(name) => pill(format!("\u{25ce} {name}"), lane_ink, ui.panel),
                Ref::Local(name) | Ref::Remote(name) => {
                    pill(name, mix(ui.card, lane_ink, 0.22), lane_ink)
                }
                Ref::Tag(name) => pill(
                    format!("tag {name}"),
                    mix(ui.card, ui.text_faint, 0.2),
                    ui.text_dim,
                ),
            })
            .collect();
        let inner = div()
            .id(("graph-row", i))
            .relative()
            .flex()
            .items_center()
            .gap(px(6.0))
            .h(px(ROW))
            .w_full()
            .min_w(px(0.0))
            .px(px(PILL_PAD))
            .rounded(px(PILL_RADIUS))
            .hover(move |s| s.bg(hsla(ui.card_hover)))
            .on_hover(
                cx.listener(move |this, hovered: &bool, _, cx| this.graph_hover(i, *hovered, cx)),
            )
            .child(lanes(
                row,
                graph.columns,
                commit.head,
                inks.clone(),
                ui.card,
            ))
            .child(
                div()
                    .flex_1()
                    .min_w(px(SUBJECT_MIN))
                    .truncate()
                    .text_size(px(TEXT))
                    .text_color(hsla(ui.text))
                    .when(commit.head, |d| d.font_weight(FontWeight::SEMIBOLD))
                    .child(commit.subject.clone()),
            )
            .children(pills)
            .when(more > 0, |d| {
                d.child(
                    div()
                        .flex_none()
                        .text_size(px(SMALL))
                        .text_color(hsla(ui.text_faint))
                        .child(format!("+{more}")),
                )
            })
            .children(author.map(|author| {
                div()
                    .flex_shrink(4.0)
                    .min_w(px(0.0))
                    .max_w(px(90.0))
                    .truncate()
                    .text_size(px(SMALL))
                    .text_color(hsla(ui.text_faint))
                    .child(author)
            }))
            .children(card.map(|card| {
                div()
                    .absolute()
                    .left(px(-PILL_INSET - 6.0))
                    .top(px(0.0))
                    .child(deferred(
                        anchored()
                            .anchor(Anchor::TopRight)
                            .snap_to_window_with_margin(px(8.0))
                            .child(card),
                    ))
            }));
        // As wide as the panel (inset for the pill), so the subject is cut, not the pills.
        Some(
            div()
                .w_full()
                .px(px(PILL_INSET))
                .child(inner)
                .into_any_element(),
        )
    }

    /// The pointer entered or left commit row `i`: its card after `CARD_DELAY`, its details
    /// read then (off the UI thread).
    fn graph_hover(&mut self, i: usize, hovered: bool, cx: &mut Context<Self>) {
        let root = self.right_sidebar.changes.repo_root();
        let graph = &mut self.right_sidebar.changes.graph;
        if !hovered {
            if graph.hover.is_some_and(|(h, _)| h == i) {
                graph.hover = None;
                cx.notify();
            }
            return;
        }
        graph.hover = Some((i, false));
        let Some(hash) = graph.commits.get(i).map(|c| c.hash.clone()) else {
            return;
        };
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(CARD_DELAY).await;
            let still = this
                .update(cx, |this, cx| {
                    let graph = &mut this.right_sidebar.changes.graph;
                    if graph.hover != Some((i, false)) {
                        return false;
                    }
                    graph.hover = Some((i, true));
                    cx.notify();
                    graph.details.as_ref().is_none_or(|(h, _)| *h != hash)
                })
                .unwrap_or(false);
            let Some(root) = root.filter(|_| still) else {
                return;
            };
            let at = hash.clone();
            let details = cx
                .background_executor()
                .spawn(async move {
                    crate::git_branch::find_git()
                        .and_then(|git| git_graph::details(&git, &root, &at))
                })
                .await;
            this.update(cx, |this, cx| {
                this.right_sidebar.changes.graph.details = Some((hash, details));
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// The hover card: who and when, the whole message, what it changed, the hash (a click
    /// copies it) and Open on GitHub.
    fn graph_card(&self, commit: &Commit, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let ui = theme.ui();
        let graph = &self.right_sidebar.changes.graph;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        let offset = crate::cli::local_utc_offset((now as u64).saturating_mul(1000));
        let details = graph
            .details
            .as_ref()
            .filter(|(h, _)| *h == commit.hash)
            .and_then(|(_, d)| d.clone());
        let body = details
            .as_ref()
            .map_or_else(|| commit.subject.clone(), |d| d.body.clone());
        let (green, red) = crate::changes_panel::count_inks(theme);
        let short: SharedString = commit.hash.chars().take(7).collect::<String>().into();
        let full = commit.hash.clone();
        let link = graph
            .origin
            .as_deref()
            .and_then(|o| git_graph::github_commit_url(o, &commit.hash));
        let line = || div().h(px(1.0)).w_full().bg(hsla(ui.divider));
        let plural = |n: u32, word: &str| format!("{n} {word}{}", if n == 1 { "" } else { "s" });
        div()
            .id("graph-card")
            .w(px(CARD_WIDTH))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .p(px(12.0))
            .rounded(px(10.0))
            .bg(hsla(ui.panel))
            .border_1()
            .border_color(hsla(ui.panel_border))
            .text_size(px(TEXT - 1.0))
            .text_color(hsla(ui.text))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(4.0))
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(format!("{},", commit.author)),
                    )
                    .child(div().text_color(hsla(ui.text_dim)).child(card_when(
                        commit.time,
                        now,
                        offset,
                    ))),
            )
            .child(div().child(body))
            .children(details.as_ref().map(|d| {
                div().flex().flex_col().gap(px(8.0)).child(line()).child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap(px(4.0))
                        .text_size(px(SMALL))
                        .child(format!("{} changed,", plural(d.files, "file")))
                        .child(
                            div()
                                .text_color(hsla(green))
                                .child(format!("{}(+),", plural(d.insertions, "insertion"))),
                        )
                        .child(
                            div()
                                .text_color(hsla(red))
                                .child(format!("{}(-)", plural(d.deletions, "deletion"))),
                        ),
                )
            }))
            .child(line())
            .child(
                div()
                    .flex()
                    .gap(px(14.0))
                    .text_size(px(SMALL))
                    .child(
                        div()
                            .id("graph-card-hash")
                            .cursor_pointer()
                            .text_color(hsla(ui.accent))
                            .child(short)
                            .on_click(move |_: &ClickEvent, _, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(full.clone()))
                            }),
                    )
                    .children(link.map(|url| {
                        div()
                            .id("graph-card-github")
                            .cursor_pointer()
                            .text_color(hsla(ui.accent))
                            .child("Open on GitHub")
                            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                                this.open_link_in_followed(&url, window, cx)
                            }))
                    })),
            )
            .into_any_element()
    }

    /// Open `url` in the followed terminal's workspace's browser tab (as the status bar's
    /// ports do).
    fn open_link_in_followed(
        &mut self,
        url: &str,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        let place = self
            .right_sidebar
            .followed
            .and_then(|id| self.model.terminal_tab(id))
            .and_then(|tab| self.model.locate(tab));
        if let Some((ws, pane)) = place {
            let _ = self.browser_in(ws, pane, url, true, window, cx);
        }
    }

    /// "Load more": the next page.
    pub(crate) fn graph_load_more(&mut self, cx: &mut Context<Self>) {
        let graph = &mut self.right_sidebar.changes.graph;
        graph.limit += PAGE;
        graph.stamp = None;
        self.right_sidebar.changes.refreshed = None;
        self.refresh_changes(cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Your call (2026-11-12, "make "Graph" by default collapsed"): GRAPH starts closed.
    #[test]
    fn the_graph_starts_closed() {
        assert!(!GraphState::default().open);
    }

    fn row(lane: usize, into: &[usize], out: &[usize], through: &[usize]) -> Row {
        Row {
            lane,
            into: into.to_vec(),
            out: out.to_vec(),
            through: through.to_vec(),
            merge: false,
        }
    }

    #[test]
    fn columns_cover_every_lane_drawn() {
        assert_eq!(columns_of(&[]), 1);
        assert_eq!(columns_of(&[row(0, &[], &[0], &[])]), 1);
        assert_eq!(
            columns_of(&[row(0, &[], &[0, 1], &[]), row(1, &[1], &[0], &[0, 2])]),
            3
        );
    }

    #[test]
    fn the_scope_button_cycles() {
        assert_eq!(next_scope(Scope::Auto), Scope::All);
        assert_eq!(next_scope(Scope::All), Scope::Current);
        assert_eq!(next_scope(Scope::Current), Scope::Auto);
        assert_eq!(
            [Scope::Auto, Scope::All, Scope::Current].map(scope_label),
            ["Auto", "All", "Current"]
        );
    }

    #[test]
    fn at_most_two_pills_the_checked_out_branch_first() {
        let refs = vec![
            Ref::Remote("origin/x".into()),
            Ref::Head("x".into()),
            Ref::Tag("v1".into()),
            Ref::Local("y".into()),
        ];
        assert_eq!(
            shown_refs(&refs),
            (
                vec![Ref::Head("x".into()), Ref::Remote("origin/x".into())],
                2
            )
        );
        assert_eq!(shown_refs(&[]), (vec![], 0));
    }

    #[test]
    fn the_cards_when_says_how_long_ago_and_the_date() {
        // 2026-11-12T03:52:00Z, seen at UTC+7: 10:52 AM.
        let at = 1_794_455_520;
        assert_eq!(
            card_when(at, at + 3_600, 7 * 3_600),
            "1h ago (November 12, 2026 at 10:52 AM)"
        );
        assert_eq!(
            card_when(at, at + 30, 0),
            "just now (November 12, 2026 at 3:52 AM)"
        );
        assert_eq!(
            card_when(at + 12 * 3_600, at + 12 * 3_600, 0),
            "just now (November 12, 2026 at 3:52 PM)"
        );
    }
}
