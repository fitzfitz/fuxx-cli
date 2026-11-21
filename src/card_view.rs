//! M11ab's card view of a shell tab (note 88): finished commands as message cards painted from
//! their grid lines, the live terminal in a framed box at the bottom. Plain data here; the
//! terminal view paints and hit-tests with it.

use alacritty_terminal::event::EventListener;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line, Point, Side};
use alacritty_terminal::term::Term;
use alacritty_terminal::vte::ansi::CursorShape;
use gpui::{Pixels, Size};

use crate::activity::clock_hm;
use crate::blocks::{Block, Outcome, Segment, segments, tag_id_at};
use crate::command_name::{card_duration, ran_label};
use crate::grid::{CellView, CursorView, lines_snapshot};
use crate::terminal::TermDims;
use crate::theme::Theme;
use crate::ui_tokens::size as ui_size;

/// Below this pane width a tab keeps today's terminal (the card's panel needs room).
pub const CARD_MIN_WIDTH: f32 = 260.0;

/// Whether a tab shows cards: the setting on, its zsh started with fuxx's marks, not on the
/// alternate screen (a full-screen program takes the whole pane), and wide enough.
pub fn card_mode(shell_cards: bool, integrated: bool, alt_screen: bool, pane_width: f32) -> bool {
    shell_cards && integrated && !alt_screen && pane_width >= CARD_MIN_WIDTH
}

/// One item of the column, top to bottom. Rows are grid lines (negative in scrollback),
/// inclusive.
#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    /// Rows above the first tagged command (a login message, a session from an older fuxx).
    Earlier { top: Line, bottom: Line },
    /// A finished command.
    Card(Card),
    /// Rows after a card that belong to no command (a background job printing later).
    Loose { top: Line, bottom: Line },
    /// The live box: the prompt shown now, or the command running now.
    Live { top: Line, bottom: Line },
}

/// A finished command as its card shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct Card {
    pub id: String,
    pub header: Header,
    /// The command the shell reported; none when it was too long (then `cmd_rows` show it).
    pub cmd: Option<String>,
    /// The command's rows as printed.
    pub cmd_rows: (Line, Line),
    pub output: Option<(Line, Line)>,
    pub failed: bool,
    /// How it ended (M11aa's drag onto an agent tab carries it).
    pub outcome: Outcome,
}

/// A card's header words: when it started, how long it ran, how it ended.
#[derive(Clone, Debug, PartialEq)]
pub struct Header {
    pub start: Option<String>,
    pub took: Option<String>,
    pub exit: Option<i32>,
}

impl Header {
    /// `10:42 · 3.2s · exit 1`: the parts it has, the exit only when not 0.
    pub fn words(&self) -> String {
        let exit = self.exit.filter(|&x| x != 0).map(|x| format!("exit {x}"));
        [self.start.clone(), self.took.clone(), exit]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" · ")
    }
}

/// A segment's first and last rows.
fn extent(segment: &Segment) -> (Line, Line) {
    match segment {
        Segment::Prompt { top, bottom, .. } => (*top, *bottom),
        Segment::Command(b) => {
            let last = b.output.map_or(b.cmd_row, |(_, end)| end.max(b.cmd_row));
            (b.prompt_top, last)
        }
    }
}

fn card(b: &Block, utc_offset: i64) -> Card {
    let cmd_end = b
        .output
        .map_or(b.cmd_row, |(start, _)| (start - 1i32).max(b.cmd_row));
    Card {
        id: b.id.clone(),
        header: Header {
            start: b.t0.map(|t| clock_hm(t as i64, utc_offset)),
            took: b.ms.map(card_duration),
            exit: match b.outcome {
                Outcome::Ok => Some(0),
                Outcome::Failed(x) => Some(x),
                Outcome::Unknown | Outcome::Running => None,
            },
        },
        cmd: b.cmd.clone(),
        cmd_rows: (b.cmd_row, cmd_end),
        output: b.output,
        failed: matches!(b.outcome, Outcome::Failed(_)),
        outcome: b.outcome,
    }
}

/// The column for a grid whose first row is `first`, its cursor on `cursor`'s row and its last
/// non-blank row `last_text` (note 88 §1, Settled 2, 3 and 5): the shown prompt or running
/// command (the last segment) and everything after it is the live box; every command before it
/// is a card; prompts before it with no command are hidden; rows between segments are loose,
/// rows before the first are earlier. With no segment, every row is the live box.
pub fn column(
    segments: &[Segment],
    first: Line,
    cursor: Line,
    last_text: Line,
    utc_offset: i64,
) -> Vec<Item> {
    let end = cursor.max(last_text);
    let Some(last) = segments.last() else {
        return vec![Item::Live {
            top: first,
            bottom: end.max(first),
        }];
    };
    // The last segment is live unless it is a finished command (its prompt not shown yet).
    let live_last = !matches!(last, Segment::Command(b) if b.outcome != Outcome::Running);
    let (before, live_top) = if live_last {
        (&segments[..segments.len() - 1], extent(last).0)
    } else {
        (segments, extent(last).1 + 1i32)
    };
    let mut items = Vec::new();
    let mut next = first;
    for segment in before {
        let (top, bottom) = extent(segment);
        if top > next {
            let gap = (next, top - 1i32);
            items.push(if items.is_empty() {
                Item::Earlier {
                    top: gap.0,
                    bottom: gap.1,
                }
            } else {
                Item::Loose {
                    top: gap.0,
                    bottom: gap.1,
                }
            });
        }
        if let Segment::Command(b) = segment {
            items.push(Item::Card(card(b, utc_offset)));
        }
        next = next.max(bottom + 1i32);
    }
    if live_top > next {
        let gap = (next, live_top - 1i32);
        items.push(if items.is_empty() {
            Item::Earlier {
                top: gap.0,
                bottom: gap.1,
            }
        } else {
            Item::Loose {
                top: gap.0,
                bottom: gap.1,
            }
        });
    }
    // The box holds the screen's rows at most (the terminal is sized to its inside, Settled 4):
    // a running command's rows already in the scrollback show as plain rows above its chips.
    let screen_top = Line(0);
    let live_top = if live_top < screen_top {
        items.push(Item::Loose {
            top: live_top,
            bottom: screen_top - 1i32,
        });
        screen_top
    } else {
        live_top
    };
    items.push(Item::Live {
        top: live_top,
        bottom: end.max(live_top),
    });
    items
}

/// Outputs longer than this many lines are folded (note 88 §2) …
pub const FOLD_OVER: usize = 20;
/// … to their last this many.
pub const FOLD_KEEP: usize = 12;

/// The rows of an output a card paints, and how many above them it folds away.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shown {
    pub hidden: usize,
    pub top: Line,
    pub bottom: Line,
}

pub fn shown(output: (Line, Line), unfolded: bool) -> Shown {
    let (top, bottom) = output;
    let lines = (bottom.0 - top.0 + 1).max(0) as usize;
    if unfolded || lines <= FOLD_OVER {
        return Shown {
            hidden: 0,
            top,
            bottom,
        };
    }
    Shown {
        hidden: lines - FOLD_KEEP,
        top: bottom - (FOLD_KEEP as i32 - 1),
        bottom,
    }
}

/// What decides that the segments must be read again: a mark arrived, the screen was cleared,
/// the size or the screen changed. Plain scrolling doesn't (rows are kept from the scrollback's
/// top); the scrollback cap rotating does, found by the newest card's tag moving.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CacheKey {
    pub marks: u64,
    pub clears: u64,
    pub cols: usize,
    pub rows: usize,
    pub alt: bool,
}

/// The segments last read (note 88's constraint: no scrollback walk per frame).
#[derive(Default)]
pub struct SegmentCache {
    key: Option<CacheKey>,
    /// Rows counted from the scrollback's top (`line + history`), stable while it scrolls.
    from_top: Vec<Segment>,
    /// The newest command's id and its tag's row from the top: if the tag isn't there any
    /// more, the scrollback rotated.
    check: Option<(String, i64)>,
}

fn shift(segment: &Segment, by: i32) -> Segment {
    match segment {
        Segment::Prompt { id, top, bottom } => Segment::Prompt {
            id: id.clone(),
            top: *top + by,
            bottom: *bottom + by,
        },
        Segment::Command(b) => Segment::Command(Block {
            prompt_top: b.prompt_top + by,
            cmd_row: b.cmd_row + by,
            output: b.output.map(|(s, e)| (s + by, e + by)),
            ..b.clone()
        }),
    }
}

impl SegmentCache {
    /// The grid's segments (rows as the grid numbers them now), read again only when `key`
    /// changed or the scrollback rotated; and whether it read them.
    pub fn segments<L>(&mut self, term: &Term<L>, key: CacheKey) -> (Vec<Segment>, bool) {
        let history = term.grid().history_size() as i64;
        let still = self.key == Some(key)
            && self.check.as_ref().is_none_or(|(id, from_top)| {
                let row = Line((from_top - history) as i32);
                tag_id_at(term, row).as_deref() == Some(id.as_str())
            });
        let rebuilt = !still;
        if rebuilt {
            let segments = segments(term);
            self.check = segments.iter().rev().find_map(|s| match s {
                Segment::Command(b) => Some((b.id.clone(), i64::from(b.cmd_row.0) + history)),
                Segment::Prompt { .. } => None,
            });
            self.from_top = segments.iter().map(|s| shift(s, history as i32)).collect();
            self.key = Some(key);
        }
        let now = self
            .from_top
            .iter()
            .map(|s| shift(s, -(history as i32)))
            .collect();
        (now, rebuilt)
    }
}

/// The column for `term` now (the segments from `cache`, the cursor and the last row with text
/// on the screen).
pub fn items_for<L>(
    term: &Term<L>,
    cache: &mut SegmentCache,
    key: CacheKey,
    utc_offset: i64,
) -> Vec<Item> {
    let (segments, _) = cache.segments(term, key);
    let grid = term.grid();
    let cursor = grid.cursor.point.line;
    let last_text = (0..term.screen_lines() as i32)
        .rev()
        .map(Line)
        .find(|&row| {
            (0..term.columns()).any(|c| {
                let ch = grid[row][alacritty_terminal::index::Column(c)].c;
                ch != ' ' && ch != '\0'
            })
        })
        .unwrap_or(Line(0));
    column(
        &segments,
        grid.topmost_line(),
        cursor,
        last_text,
        utc_offset,
    )
}

/// A card's header line (UI text), points.
pub const HEADER_H: f32 = 18.0;
/// Between the command line and its output panel, points.
pub const PANEL_GAP: f32 = 4.0;
/// Between items, points.
pub const CARD_GAP: f32 = 14.0;

/// A card's height: its header, its command's lines, and its panel (the rows shown, the fold
/// row when the output is long enough to fold, its padding and edge).
pub fn card_height(card: &Card, unfolded: bool, line_h: f32) -> f32 {
    let cmd_lines = match &card.cmd {
        Some(cmd) => cmd.lines().count().max(1),
        None => (card.cmd_rows.1.0 - card.cmd_rows.0.0 + 1).max(1) as usize,
    };
    let mut height = HEADER_H + cmd_lines as f32 * line_h;
    if let Some(output) = card.output {
        let lines = (output.1.0 - output.0.0 + 1).max(0) as usize;
        let s = shown(output, unfolded);
        let rows = (s.bottom.0 - s.top.0 + 1).max(0) as usize + usize::from(lines > FOLD_OVER);
        height += PANEL_GAP + 2.0 * (BOX_EDGE + BOX_PAD) + rows as f32 * line_h;
    }
    height
}

/// An item's height.
pub fn item_height(item: &Item, unfolded: bool, line_h: f32) -> f32 {
    let rows = |top: Line, bottom: Line| (bottom.0 - top.0 + 1).max(0) as f32 * line_h;
    match item {
        Item::Card(card) => card_height(card, unfolded, line_h),
        Item::Earlier { top, bottom } | Item::Loose { top, bottom } => rows(*top, *bottom),
        Item::Live { top, bottom } => CHIPS_ROW + 2.0 * (BOX_EDGE + BOX_PAD) + rows(*top, *bottom),
    }
}

/// An item placed in the column: its top from the column's top, and its height.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placed {
    pub top: f32,
    pub height: f32,
}

/// Every item placed top to bottom, `CARD_GAP` apart (`unfolded`: a card id is unfolded), and
/// the column's whole height.
pub fn layout(items: &[Item], unfolded: &dyn Fn(&str) -> bool, line_h: f32) -> (Vec<Placed>, f32) {
    let mut top = 0.0;
    let mut placed = Vec::with_capacity(items.len());
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            top += CARD_GAP;
        }
        let open = matches!(item, Item::Card(c) if unfolded(&c.id));
        let height = item_height(item, open, line_h);
        placed.push(Placed { top, height });
        top += height;
    }
    (placed, top)
}

/// Which of `rows` rows (the first at `top`, `line_h` apart) touch the `clip` span (`top`,
/// `bottom`): only those are read and painted.
pub fn visible_rows(
    top: f32,
    rows: usize,
    line_h: f32,
    clip: (f32, f32),
) -> std::ops::Range<usize> {
    let first = ((clip.0 - top) / line_h).floor().max(0.0) as usize;
    let end = ((clip.1 - top) / line_h).ceil().max(0.0) as usize;
    first.min(rows)..end.min(rows)
}

/// What a tab in card view keeps between frames.
#[derive(Default)]
pub struct CardsState {
    pub cache: SegmentCache,
    /// The cards unfolded (by id), while the tab is open.
    pub unfolded: std::collections::HashSet<String>,
    /// How far the column is scrolled up from its bottom, points (0: stuck to the bottom).
    pub scroll: f32,
    /// The most it can scroll (the column's height less the view's), from the last frame.
    pub max_scroll: f32,
    /// From the last frame: the rows painted (to point into, M11ab Task 6), each card's hit
    /// areas, every card's place in the column, the column's and the view's heights.
    pub regions: Vec<Region>,
    pub hits: Vec<CardHit>,
    pub order: Vec<(String, f32, f32)>,
    pub total: f32,
    pub view: f32,
    /// The pane's top-left corner last frame (window points): the overlays are placed from it.
    pub origin: (f32, f32),
}

/// Rows painted this frame that the pointer can select in (window points): the first row is
/// grid line `top`, its first cell at (`x`, `y`), the rows span `y..bottom`. `key` names it
/// across frames (`live`, a card's id, `rows:<top>`); a live region takes mouse reports.
#[derive(Clone, Debug, PartialEq)]
pub struct Region {
    pub key: String,
    pub x: f32,
    pub y: f32,
    pub bottom: f32,
    pub top: Line,
    pub live: bool,
}

/// A card's hit areas this frame (window points, `x, y, width, height`): the whole card, its
/// command line (the drag handle), its fold row; and what its drag carries.
#[derive(Clone, Debug, PartialEq)]
pub struct CardHit {
    pub id: String,
    pub card: (f32, f32, f32, f32),
    pub cmd: (f32, f32, f32, f32),
    pub fold: Option<(f32, f32, f32, f32)>,
    pub command: Option<String>,
    pub outcome: Outcome,
}

impl CardsState {
    /// Typing goes to the live box, so the column goes back to its bottom (note 88 §1).
    pub fn typed(&mut self) {
        self.scroll = 0.0;
    }
}

/// The grid point under `pos` in a region of rows whose first row is grid line `top`, with its
/// first cell at `origin` (window points; cells `cell` = width, height): the column clamped to
/// the grid's `cols`, the side of the cell it's on (M11ab: selecting and links in a card).
pub fn card_point(
    pos: (f32, f32),
    origin: (f32, f32),
    top: Line,
    (cell_w, line_h): (f32, f32),
    cols: usize,
) -> (Point, Side) {
    let x = (pos.0 - origin.0) / cell_w;
    let last = cols.max(1) - 1;
    let col = x.floor().clamp(0.0, last as f32) as usize;
    let side = if x < 0.0 {
        Side::Left
    } else if x >= cols as f32 || x - x.floor() >= 0.5 {
        Side::Right
    } else {
        Side::Left
    };
    let row = ((pos.1 - origin.1) / line_h).floor() as i32;
    (Point::new(top + row, Column(col)), side)
}

/// The running chip's words: `● npm test · 1m 15s` (whole seconds, M11q's `ran_label`).
pub fn running_words(short: &str, elapsed: std::time::Duration) -> String {
    format!("\u{25cf} {short} \u{b7} {}", ran_label(elapsed))
}

/// One chip above the live box.
#[derive(Clone, Debug, PartialEq)]
pub struct Chip {
    pub words: String,
    /// The running command's (drawn in the running colour).
    pub running: bool,
}

/// The chips above the live box (note 88 §3; ruling 1: folder and branch, no counts): the
/// running command first, then the folder, then the branch.
pub fn chips(folder: Option<&str>, branch: Option<&str>, running: Option<String>) -> Vec<Chip> {
    let running = running.map(|words| Chip {
        words,
        running: true,
    });
    let folder = folder.map(|f| Chip {
        words: f.to_string(),
        running: false,
    });
    let branch = branch.map(|b| Chip {
        words: format!("\u{2387} {b}"),
        running: false,
    });
    running.into_iter().chain(folder).chain(branch).collect()
}

/// The scroll (from the column's bottom) that shows a card at `top`, `height` tall, in a
/// column `total` tall seen through a view `view` tall, now scrolled `scroll`: unchanged when
/// it's in view, else its top at the view's top (as far as the column goes).
pub fn reveal(top: f32, height: f32, total: f32, view: f32, scroll: f32) -> f32 {
    let (shown_top, shown_bottom) = (total - view - scroll, total - scroll);
    if top >= shown_top && top + height <= shown_bottom {
        return scroll;
    }
    (total - view - top).clamp(0.0, (total - view).max(0.0))
}

/// Rows of cells to paint, the first at `y` (window points).
#[derive(Clone, Debug)]
pub struct Rows {
    pub y: f32,
    /// The grid line of `lines[0]`.
    pub top: Line,
    pub lines: Vec<Vec<CellView>>,
}

/// One item as the view paints it this frame (window points).
#[derive(Clone, Debug)]
pub enum Draw {
    /// Earlier or loose rows.
    Plain(Rows),
    Card {
        id: String,
        y: f32,
        height: f32,
        outcome: Outcome,
        header: Header,
        /// `❯ cmd` when the shell reported it; else the command's rows as printed.
        cmd: Result<String, Rows>,
        panel: Option<Panel>,
        failed: bool,
    },
    Live {
        /// The chips row's top; the box starts `CHIPS_ROW` below it.
        y: f32,
        height: f32,
        /// The rows in view, with the cursor among them (rows relative to `rows.y`).
        rows: Rows,
        cursor: Option<CursorView>,
    },
}

/// A card's output panel: its box, the fold row's words (none when not long enough to fold),
/// and the rows in view.
#[derive(Clone, Debug)]
pub struct Panel {
    pub y: f32,
    pub height: f32,
    pub fold: Option<String>,
    pub rows: Rows,
}

/// What a frame paints (the column's height is kept in `CardsState::total`).
pub struct CardFrame {
    pub draws: Vec<Draw>,
}

/// The column for `term` this frame, placed in the view from `view_top` (window points,
/// `view_height` tall): bottom-anchored, scrolled up by `state.scroll` (clamped here), and only
/// the rows in view read.
#[allow(clippy::too_many_arguments)]
pub fn frame<L: EventListener>(
    term: &Term<L>,
    state: &mut CardsState,
    key: CacheKey,
    theme: &Theme,
    line_h: f32,
    (view_top, view_height): (f32, f32),
    utc_offset: i64,
    cursor_shape: Option<(CursorShape, bool)>,
) -> CardFrame {
    let items = items_for(term, &mut state.cache, key, utc_offset);
    let unfolded = &state.unfolded;
    let (placed, total) = layout(&items, &|id| unfolded.contains(id), line_h);
    state.max_scroll = (total - view_height).max(0.0);
    state.order = items
        .iter()
        .zip(&placed)
        .filter_map(|(item, at)| match item {
            Item::Card(c) => Some((c.id.clone(), at.top, at.height)),
            _ => None,
        })
        .collect();
    state.total = total;
    state.view = view_height;
    state.scroll = state.scroll.clamp(0.0, state.max_scroll);
    let base = view_top + view_height - total + state.scroll;
    let clip = (view_top, view_top + view_height);
    let read = |top: Line, rows_y: f32, count: usize| -> Rows {
        let r = visible_rows(rows_y, count, line_h, clip);
        let lines = if r.is_empty() {
            Vec::new()
        } else {
            lines_snapshot(term, theme, top + r.start as i32, top + (r.end as i32 - 1))
        };
        Rows {
            y: rows_y + r.start as f32 * line_h,
            top: top + r.start as i32,
            lines,
        }
    };
    let inner = BOX_EDGE + BOX_PAD;
    let mut draws = Vec::new();
    for (item, at) in items.iter().zip(&placed) {
        let y = base + at.top;
        if y >= clip.1 || y + at.height <= clip.0 {
            continue;
        }
        let count = |top: Line, bottom: Line| (bottom.0 - top.0 + 1).max(0) as usize;
        match item {
            Item::Earlier { top, bottom } | Item::Loose { top, bottom } => {
                draws.push(Draw::Plain(read(*top, y, count(*top, *bottom))));
            }
            Item::Card(card) => {
                let open = unfolded.contains(&card.id);
                let cmd_y = y + HEADER_H;
                let (cmd, cmd_lines) = match &card.cmd {
                    Some(c) => (Ok(c.clone()), c.lines().count().max(1)),
                    None => {
                        let (a, b) = card.cmd_rows;
                        (Err(read(a, cmd_y, count(a, b))), count(a, b).max(1))
                    }
                };
                let panel = card.output.map(|output| {
                    let lines = count(output.0, output.1);
                    let s = shown(output, open);
                    let fold = (lines > FOLD_OVER).then(|| match s.hidden {
                        0 => "▾ Show less".to_string(),
                        n => format!("▸ {n} earlier lines"),
                    });
                    let panel_y = cmd_y + cmd_lines as f32 * line_h + PANEL_GAP;
                    let rows_y = panel_y + inner + if fold.is_some() { line_h } else { 0.0 };
                    let shown_rows = count(s.top, s.bottom);
                    Panel {
                        y: panel_y,
                        height: 2.0 * inner
                            + (shown_rows + usize::from(fold.is_some())) as f32 * line_h,
                        fold,
                        rows: read(s.top, rows_y, shown_rows),
                    }
                });
                draws.push(Draw::Card {
                    id: card.id.clone(),
                    y,
                    height: at.height,
                    outcome: card.outcome,
                    header: card.header.clone(),
                    cmd,
                    panel,
                    failed: card.failed,
                });
            }
            Item::Live { top, bottom } => {
                let rows_y = y + CHIPS_ROW + inner;
                let rows = read(*top, rows_y, count(*top, *bottom));
                let first = *top + ((rows.y - rows_y) / line_h).round() as i32;
                let point = term.grid().cursor.point;
                let row = point.line.0 - first.0;
                let cursor = cursor_shape
                    .filter(|_| row >= 0 && (row as usize) < rows.lines.len())
                    .map(|(shape, blinking)| CursorView {
                        row: row as usize,
                        col: point.column.0,
                        shape,
                        blinking,
                    });
                draws.push(Draw::Live {
                    y,
                    height: at.height,
                    rows,
                    cursor,
                });
            }
        }
    }
    CardFrame { draws }
}

/// The chips row above the live box (folder, branch, the running command), points.
pub const CHIPS_ROW: f32 = 28.0;
/// The live box's (and a card panel's) inner padding and edge, points.
pub const BOX_PAD: f32 = 10.0;
pub const BOX_EDGE: f32 = 1.0;

/// The terminal's size in card view (note 88 §4, Settled 4): the live box's inside at its
/// tallest, the pane less its padding (`terminal_view::inset`), the chips row, and the box's
/// edge and padding on each side.
pub fn card_dims(pane: Size<Pixels>, split: bool, cell_w: f32, line_h: f32) -> TermDims {
    let (pad_x, pad_y) = if split {
        ui_size::SPLIT_TERMINAL_PADDING
    } else {
        ui_size::TERMINAL_PADDING
    };
    let frame = 2.0 * (BOX_EDGE + BOX_PAD);
    let w = f32::from(pane.width) - 2.0 * pad_x - frame;
    let h = f32::from(pane.height) - 2.0 * pad_y - CHIPS_ROW - frame;
    TermDims {
        cols: ((w / cell_w).floor().max(0.0) as usize).max(2),
        rows: ((h / line_h).floor().max(0.0) as usize).max(1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn card_mode_needs_all_four() {
        assert!(card_mode(true, true, false, 800.0));
        assert!(!card_mode(false, true, false, 800.0), "the setting off");
        assert!(
            !card_mode(true, false, false, 800.0),
            "a shell without the marks"
        );
        assert!(!card_mode(true, true, true, 800.0), "the alternate screen");
        assert!(!card_mode(true, true, false, 259.0), "too narrow");
        assert!(card_mode(true, true, false, CARD_MIN_WIDTH));
    }

    use crate::blocks::{Block, Outcome, Segment};
    use alacritty_terminal::index::Line;

    /// A command's segment: its prompt on `top`, its command row the next, output rows `out`.
    fn command(id: &str, top: i32, out: Option<(i32, i32)>, outcome: Outcome) -> Segment {
        Segment::Command(Block {
            id: id.into(),
            outcome,
            prompt_top: Line(top),
            cmd_row: Line(top + 1),
            output: out.map(|(a, b)| (Line(a), Line(b))),
            cmd: Some(id.into()),
            t0: None,
            ms: None,
        })
    }

    fn prompt(id: &str, top: i32, bottom: i32) -> Segment {
        Segment::Prompt {
            id: id.into(),
            top: Line(top),
            bottom: Line(bottom),
        }
    }

    fn ids(items: &[Item]) -> Vec<String> {
        items
            .iter()
            .map(|i| match i {
                Item::Earlier { top, bottom } => format!("earlier {}..{}", top.0, bottom.0),
                Item::Card(c) => format!("card {}{}", c.id, if c.failed { " failed" } else { "" }),
                Item::Loose { top, bottom } => format!("loose {}..{}", top.0, bottom.0),
                Item::Live { top, bottom } => format!("live {}..{}", top.0, bottom.0),
            })
            .collect()
    }

    #[test]
    fn cards_for_done_commands_and_empty_prompts_hidden() {
        let segments = [
            command("ls", 0, Some((2, 3)), Outcome::Ok),
            prompt("empty", 4, 5),
            command("npm test", 6, Some((8, 9)), Outcome::Failed(1)),
            prompt("live", 10, 10),
        ];
        let items = column(&segments, Line(0), Line(10), Line(10), 0);
        assert_eq!(
            ids(&items),
            ["card ls", "card npm test failed", "live 10..10"]
        );
    }

    #[test]
    fn no_tags_is_one_live_box() {
        let items = column(&[], Line(-20), Line(3), Line(5), 0);
        assert_eq!(ids(&items), ["live -20..5"]);
    }

    #[test]
    fn a_running_command_is_in_the_live_box() {
        let segments = [
            command("ls", 0, Some((2, 2)), Outcome::Ok),
            command("npm run dev", 3, Some((5, 40)), Outcome::Running),
        ];
        let items = column(&segments, Line(0), Line(40), Line(40), 0);
        assert_eq!(ids(&items), ["card ls", "live 3..40"]);
    }

    /// Settled 4: the box's frame and chips stay on screen, so a running command's rows that
    /// scrolled off the screen (above line 0) show as plain rows above the chips.
    #[test]
    fn a_long_running_command_keeps_the_box_on_screen() {
        let segments = [
            command("ls", -50, Some((-48, -48)), Outcome::Ok),
            command("make", -30, Some((-28, 40)), Outcome::Running),
        ];
        let items = column(&segments, Line(-50), Line(40), Line(40), 0);
        assert_eq!(
            ids(&items),
            ["card ls", "loose -47..-31", "loose -30..-1", "live 0..40"]
        );
    }

    #[test]
    fn rows_after_a_card_are_loose() {
        let segments = [
            command("ls", 0, Some((2, 2)), Outcome::Ok),
            prompt("live", 6, 6),
        ];
        let items = column(&segments, Line(0), Line(6), Line(6), 0);
        assert_eq!(ids(&items), ["card ls", "loose 3..5", "live 6..6"]);
    }

    #[test]
    fn rows_before_the_first_tag_are_earlier() {
        let segments = [prompt("live", 4, 4)];
        let items = column(&segments, Line(-2), Line(4), Line(4), 0);
        assert_eq!(ids(&items), ["earlier -2..3", "live 4..4"]);
    }

    #[test]
    fn the_live_box_ends_at_the_cursor_or_the_last_text() {
        let segments = [prompt("live", 4, 23)];
        // A menu under the prompt: text below the cursor.
        assert_eq!(
            ids(&column(&segments, Line(4), Line(4), Line(7), 0)),
            ["live 4..7"]
        );
        // Nothing under the cursor: it ends on the cursor's row.
        assert_eq!(
            ids(&column(&segments, Line(4), Line(5), Line(4), 0)),
            ["live 4..5"]
        );
    }

    #[test]
    fn header_words() {
        let full = Header {
            start: Some("10:42".into()),
            took: Some("3.2s".into()),
            exit: Some(1),
        };
        assert_eq!(full.words(), "10:42 · 3.2s · exit 1");
        let ok = Header {
            exit: Some(0),
            ..full.clone()
        };
        assert_eq!(ok.words(), "10:42 · 3.2s");
        let none = Header {
            start: None,
            took: None,
            exit: None,
        };
        assert_eq!(none.words(), "");
    }

    #[test]
    fn a_card_takes_its_times_from_its_tag() {
        let Segment::Command(mut b) = command("make", 0, Some((2, 2)), Outcome::Ok) else {
            unreachable!()
        };
        // 10:42:05 UTC, shown at UTC+7.
        b.t0 = Some(1_795_000_000 - 1_795_000_000 % 86_400 + 3 * 3600 + 42 * 60 + 5);
        b.ms = Some(3_200);
        let items = column(&[Segment::Command(b)], Line(0), Line(3), Line(3), 7 * 3600);
        let Item::Card(card) = &items[0] else {
            panic!("a card")
        };
        assert_eq!(card.header.words(), "10:42 · 3.2s");
    }

    #[test]
    fn folding_keeps_the_last_twelve() {
        let folded = shown((Line(0), Line(24)), false);
        assert_eq!(
            (folded.hidden, folded.top, folded.bottom),
            (13, Line(13), Line(24))
        );
        let short = shown((Line(0), Line(19)), false);
        assert_eq!((short.hidden, short.top), (0, Line(0)));
        let open = shown((Line(0), Line(24)), true);
        assert_eq!((open.hidden, open.top), (0, Line(0)));
    }

    #[test]
    fn card_dims_are_the_box_inside() {
        let pane = gpui::size(gpui::px(1000.0), gpui::px(700.0));
        let side = 2.0 * (BOX_EDGE + BOX_PAD);
        let (w, h) = (
            1000.0 - 2.0 * 28.0 - side,
            700.0 - 2.0 * 22.0 - CHIPS_ROW - side,
        );
        let dims = card_dims(pane, false, 8.0, 17.0);
        assert_eq!(
            (dims.cols, dims.rows),
            ((w / 8.0) as usize, (h / 17.0) as usize)
        );
        assert_eq!((dims.cols, dims.rows), (115, 35));
    }

    // M11ab Task 5: the cache, the layout, the rows a paint reads.

    use crate::blocks::{Tagger, advance, tag_id_at};
    use crate::shell_marks::MarkScanner;
    use alacritty_terminal::event::VoidListener;
    use alacritty_terminal::term::{Config, Term};
    use alacritty_terminal::vte::ansi::{Processor, StdSyncHandler};

    /// A 20 x 6 terminal with `history` lines of scrollback, fed through `advance`.
    struct Sh {
        term: Term<VoidListener>,
        processor: Processor<StdSyncHandler>,
        tagger: Tagger,
        scanner: MarkScanner,
    }

    impl Sh {
        fn new(history: usize) -> Self {
            let config = Config {
                scrolling_history: history,
                ..crate::terminal::term_config()
            };
            Sh {
                term: Term::new(config, &TermDims { cols: 20, rows: 6 }, VoidListener),
                processor: Processor::new(),
                tagger: Tagger::new(),
                scanner: MarkScanner::new(),
            }
        }

        fn feed(&mut self, bytes: &[u8]) -> &mut Self {
            advance(
                &mut self.processor,
                &mut self.term,
                &mut self.tagger,
                &mut self.scanner,
                bytes,
            );
            self
        }

        fn prompt(&mut self) -> &mut Self {
            self.feed(b"\x1b]133;A\x07$ \x1b]133;B\x1b\\")
        }

        fn run(&mut self, cmd: &str) -> &mut Self {
            self.prompt()
                .feed(cmd.as_bytes())
                .feed(b"\r\n")
                .feed(format!("\x1b]133;C;cmdline_url={cmd}\x07").as_bytes())
        }

        fn command(&mut self, cmd: &str, output: &str) -> &mut Self {
            self.run(cmd)
                .feed(output.as_bytes())
                .feed(b"\x1b]133;D;0\x07")
        }
    }

    fn key(marks: u64, clears: u64) -> CacheKey {
        CacheKey {
            marks,
            clears,
            cols: 20,
            rows: 6,
            alt: false,
        }
    }

    fn card_ids(items: &[Item]) -> Vec<String> {
        items
            .iter()
            .filter_map(|i| match i {
                Item::Card(c) => Some(c.id.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn cards_rebuild_after_a_clear() {
        let mut sh = Sh::new(100);
        sh.command("ls", "a\r\n").command("pwd", "/\r\n").prompt();
        let mut cache = SegmentCache::default();
        assert_eq!(
            card_ids(&items_for(&sh.term, &mut cache, key(1, 0), 0)).len(),
            2
        );
        // `clear`: home, erase the screen, erase the scrollback; then the next prompt.
        sh.feed(b"\x1b[H\x1b[2J\x1b[3J").prompt();
        let items = items_for(&sh.term, &mut cache, key(2, 1), 0);
        assert!(
            matches!(items.as_slice(), [Item::Live { .. }]),
            "only the live box: {items:?}"
        );
    }

    #[test]
    fn the_cache_keeps_cards_as_output_scrolls() {
        let mut sh = Sh::new(100);
        sh.command("ls", "a\r\n")
            .command("pwd", "/\r\n")
            .run("make");
        let mut cache = SegmentCache::default();
        let (_, rebuilt) = cache.segments(&sh.term, key(1, 0));
        assert!(rebuilt);
        for i in 0..30 {
            sh.feed(format!("out {i}\r\n").as_bytes());
        }
        // No new mark: the same key, and the cards are still where their tags are.
        let (segments, rebuilt) = cache.segments(&sh.term, key(1, 0));
        assert!(!rebuilt, "scrolling alone doesn't rebuild");
        for segment in &segments {
            if let Segment::Command(b) = segment {
                assert_eq!(
                    tag_id_at(&sh.term, b.cmd_row).as_deref(),
                    Some(b.id.as_str())
                );
            }
        }
    }

    #[test]
    fn the_cache_rebuilds_when_the_scrollback_rotates() {
        let mut sh = Sh::new(10);
        sh.command("ls", "a\r\n")
            .command("pwd", "/\r\n")
            .run("make");
        let mut cache = SegmentCache::default();
        let _ = cache.segments(&sh.term, key(1, 0));
        // Past the 10-line cap (8 rows so far, 16 fit): 11 more drop the 3 oldest rows and move
        // every row up, `pwd`'s card still in the grid.
        for i in 0..11 {
            sh.feed(format!("out {i}\r\n").as_bytes());
        }
        assert_eq!(sh.term.grid().history_size(), 10, "at the cap");
        let (segments, rebuilt) = cache.segments(&sh.term, key(1, 0));
        assert!(rebuilt, "rotation rebuilds");
        assert!(
            segments
                .iter()
                .any(|s| matches!(s, Segment::Command(b) if b.cmd.as_deref() == Some("pwd"))),
            "pwd's card is still read: {segments:?}"
        );
        for segment in &segments {
            if let Segment::Command(b) = segment {
                assert_eq!(
                    tag_id_at(&sh.term, b.cmd_row).as_deref(),
                    Some(b.id.as_str())
                );
            }
        }
    }

    fn a_card(output: Option<(i32, i32)>) -> Card {
        Card {
            id: "c".into(),
            header: Header {
                start: None,
                took: None,
                exit: Some(0),
            },
            cmd: Some("ls".into()),
            cmd_rows: (Line(1), Line(1)),
            output: output.map(|(a, b)| (Line(a), Line(b))),
            failed: false,
            outcome: Outcome::Ok,
        }
    }

    #[test]
    fn heights_add_up() {
        let lh = 17.0;
        let frame = PANEL_GAP + 2.0 * (BOX_EDGE + BOX_PAD);
        let card = a_card(Some((2, 4)));
        assert_eq!(
            card_height(&card, false, lh),
            HEADER_H + lh + frame + 3.0 * lh
        );
        assert_eq!(card_height(&a_card(None), false, lh), HEADER_H + lh);
        // 25 lines folded: the last 12 and the fold row; unfolded: all 25 and "Show less".
        let long = a_card(Some((0, 24)));
        assert_eq!(
            card_height(&long, false, lh),
            HEADER_H + lh + frame + 13.0 * lh
        );
        assert_eq!(
            card_height(&long, true, lh),
            HEADER_H + lh + frame + 26.0 * lh
        );
        let items = [
            Item::Card(card.clone()),
            Item::Live {
                top: Line(5),
                bottom: Line(6),
            },
        ];
        let (placed, total) = layout(&items, &|_| false, lh);
        assert_eq!(placed[0].top, 0.0);
        assert_eq!(placed[1].top, card_height(&card, false, lh) + CARD_GAP);
        assert_eq!(
            total,
            placed[1].top + CHIPS_ROW + 2.0 * (BOX_EDGE + BOX_PAD) + 2.0 * lh
        );
    }

    #[test]
    fn only_visible_rows_are_read() {
        let lh = 17.0;
        let rows = visible_rows(-100_000.0, 10_000, lh, (0.0, 600.0));
        assert!(rows.len() <= (600.0 / lh) as usize + 2, "{rows:?}");
        assert!(!rows.is_empty());
        assert_eq!(visible_rows(0.0, 5, lh, (0.0, 600.0)), 0..5);
        assert_eq!(visible_rows(700.0, 5, lh, (0.0, 600.0)), 0..0);
        assert_eq!(visible_rows(-17.0, 5, lh, (0.0, 600.0)), 1..5);
    }

    // M11ab Task 6: pointing, typing, chips, choosing.

    #[test]
    fn card_point_maps_a_panel_pixel_to_its_grid_line() {
        // Row 0 of the region is line -40, at (100, 200); cells 8 x 17.
        let at = |x: f32, y: f32| card_point((x, y), (100.0, 200.0), Line(-40), (8.0, 17.0), 80);
        let (point, side) = at(100.0 + 5.0 * 8.0 + 2.0, 200.0 + 3.0 * 17.0 + 5.0);
        assert_eq!(
            (point.line, point.column, side),
            (Line(-37), Column(5), Side::Left)
        );
        let (point, side) = at(100.0 + 5.0 * 8.0 + 6.0, 200.0);
        assert_eq!(
            (point.line, point.column, side),
            (Line(-40), Column(5), Side::Right)
        );
        // Past the edges: the first and last columns.
        assert_eq!(at(10.0, 200.0).0.column, Column(0));
        assert_eq!(at(10_000.0, 200.0).0.column, Column(79));
    }

    #[test]
    fn typing_snaps_the_column_to_the_bottom() {
        let mut state = CardsState {
            scroll: 340.0,
            max_scroll: 900.0,
            ..CardsState::default()
        };
        state.typed();
        assert_eq!(state.scroll, 0.0);
    }

    #[test]
    fn elapsed_words() {
        let words = running_words("npm test", std::time::Duration::from_secs(75));
        assert_eq!(words, "\u{25cf} npm test \u{b7} 1m 15s");
        assert_eq!(
            running_words("make", std::time::Duration::from_millis(12_400)),
            "\u{25cf} make \u{b7} 12s"
        );
    }

    #[test]
    fn branch_chip_comes_from_the_workspace() {
        let words = |c: Vec<Chip>| c.into_iter().map(|c| c.words).collect::<Vec<_>>();
        assert_eq!(
            words(chips(Some("~/code/api"), Some("main"), None)),
            ["~/code/api", "\u{2387} main"]
        );
        assert_eq!(words(chips(None, None, None)), Vec::<String>::new());
        let running = chips(Some("~"), None, Some("\u{25cf} make \u{b7} 3s".into()));
        assert!(
            running[0].running && running[0].words.contains("make"),
            "running first"
        );
        assert_eq!(running[1].words, "~");
    }

    #[test]
    fn a_chosen_card_is_scrolled_into_view() {
        // A 2000 pt column in a 600 pt view: a card at 400..520 needs the view's top at 400,
        // so the column scrolls up (from its bottom) by 2000 - 600 - 400.
        assert_eq!(reveal(400.0, 120.0, 2000.0, 600.0, 0.0), 1000.0);
        // Already in view (the view shows 1400..2000 at scroll 0): unchanged.
        assert_eq!(reveal(1500.0, 100.0, 2000.0, 600.0, 0.0), 0.0);
        // Taller than the view: its top at the view's top.
        assert_eq!(reveal(100.0, 900.0, 2000.0, 600.0, 0.0), 1300.0);
    }
}
