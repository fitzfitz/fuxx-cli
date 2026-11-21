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
    /// The live box: the prompt shown now, or the command running now; `typing`: at a prompt,
    /// the column the typing starts at on its first row (the crop, rework).
    Live {
        top: Line,
        bottom: Line,
        typing: Option<usize>,
    },
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
            // To its output's end, or its command line's last row (a wrapped one too).
            let last = b.output.map_or(b.cmd_end, |(_, end)| end.max(b.cmd_end));
            (b.prompt_top, last)
        }
    }
}

fn card(b: &Block, utc_offset: i64) -> Card {
    let cmd_end = b.cmd_end.max(b.cmd_row);
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
        // The box keeps to the screen (Settled 4): rows above it are earlier rows.
        let screen_top = Line(0).max(first);
        let mut items = Vec::new();
        if first < screen_top {
            items.push(Item::Earlier {
                top: first,
                bottom: screen_top - 1i32,
            });
        }
        items.push(Item::Live {
            top: screen_top,
            bottom: end.max(screen_top),
            typing: None,
        });
        return items;
    };
    // The last segment is live unless it is a finished command (its prompt not shown yet).
    let live_last = !matches!(last, Segment::Command(b) if b.outcome != Outcome::Running);
    // The live box starts where the typing does (the shown prompt's B row), or at a running
    // command's output (rework: its prompt and command rows are left out, its command is in the
    // chip); the rows above that within the live segment are hidden, not loose.
    let (before, seg_top, live_top, typing) = if live_last {
        let (start, typing) = match last {
            // Where the typing starts (from the tag: a wrapped prompt's lower row), else the
            // tag's row.
            Segment::Prompt { row, input, .. } => (
                input.map_or(*row, |(line, _)| line),
                input.map(|(_, col)| col),
            ),
            Segment::Command(b) => (b.output.map_or(b.cmd_end + 1i32, |(s, _)| s), None),
        };
        (
            &segments[..segments.len() - 1],
            extent(last).0,
            start,
            typing,
        )
    } else {
        let after = extent(last).1 + 1i32;
        (segments, after, after, None)
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
    if seg_top > next {
        let gap = (next, seg_top - 1i32);
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
    let typing = typing.filter(|_| live_top >= screen_top);
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
        typing,
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
    /// Moves on every rebuild or shift (the column built from these segments is kept by it).
    pub generation: u64,
}

fn shift(segment: &Segment, by: i32) -> Segment {
    match segment {
        Segment::Prompt {
            id,
            row,
            top,
            bottom,
            input,
        } => Segment::Prompt {
            id: id.clone(),
            row: *row + by,
            top: *top + by,
            bottom: *bottom + by,
            input: input.map(|(line, col)| (line + by, col)),
        },
        Segment::Command(b) => Segment::Command(Block {
            prompt_top: b.prompt_top + by,
            cmd_row: b.cmd_row + by,
            cmd_end: b.cmd_end + by,
            output: b.output.map(|(s, e)| (s + by, e + by)),
            ..b.clone()
        }),
    }
}

impl SegmentCache {
    /// The grid's segments (rows as the grid numbers them now), read again only when `key`
    /// changed or the scrollback rotated; and whether it read them.
    pub fn segments<L>(&mut self, term: &Term<L>, key: CacheKey) -> (Vec<Segment>, bool) {
        let rebuilt = self.refresh(term, key);
        let history = term.grid().history_size() as i64;
        let now = self
            .from_top
            .iter()
            .map(|s| shift(s, -(history as i32)))
            .collect();
        (now, rebuilt)
    }

    /// Bring the cache up to date; true when it read the whole scrollback again. At the
    /// scrollback cap every row moves up as lines rotate out: the newest card's tag is looked
    /// for above its old row, and the cache shifts by as much (final review #7) instead.
    pub fn refresh<L>(&mut self, term: &Term<L>, key: CacheKey) -> bool {
        let history = term.grid().history_size() as i64;
        let at = |from_top: i64| Line((from_top - history) as i32);
        if self.key == Some(key) {
            let Some((id, from_top)) = self.check.clone() else {
                return false;
            };
            if tag_id_at(term, at(from_top)).as_deref() == Some(id.as_str()) {
                return false;
            }
            let rows = history + term.screen_lines() as i64;
            if let Some(k) = (1..=rows)
                .find(|&k| tag_id_at(term, at(from_top - k)).as_deref() == Some(id.as_str()))
            {
                // Every row moved up by `k`: shift, and drop the segments whose top rotated out
                // (their tags went with it; what's left of them reads as earlier rows, as a
                // full read would have it).
                let by = -(k as i32);
                self.from_top = self
                    .from_top
                    .iter()
                    .map(|s| shift(s, by))
                    .filter(|s| extent(s).0.0 >= 0)
                    .collect();
                self.check = Some((id, from_top - k));
                self.generation += 1;
                return false;
            }
        }
        let rebuilt = true;
        if rebuilt {
            let segments = segments(term);
            self.check = segments.iter().rev().find_map(|s| match s {
                Segment::Command(b) => Some((b.id.clone(), i64::from(b.cmd_row.0) + history)),
                Segment::Prompt { .. } => None,
            });
            self.from_top = segments.iter().map(|s| shift(s, history as i32)).collect();
            self.key = Some(key);
            self.generation += 1;
        }
        rebuilt
    }
}

/// The screen's last row with text on it (row 0 when it's blank).
fn last_text_row<L>(term: &Term<L>) -> Line {
    let grid = term.grid();
    (0..term.screen_lines() as i32)
        .rev()
        .map(Line)
        .find(|&row| {
            (0..term.columns()).any(|c| {
                let ch = grid[row][Column(c)].c;
                ch != ' ' && ch != '\0'
            })
        })
        .unwrap_or(Line(0))
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
    let last_text = last_text_row(term);
    column(
        &segments,
        grid.topmost_line(),
        cursor,
        last_text,
        utc_offset,
    )
}

/// The rework's sizes (note 88 "Rework"), points: a 1 pt edge; the card surface's padding and
/// the gap above its output; the composer: the context row, the inner box 5 pt in from the outer
/// box's sides and bottom, its padding.
pub const EDGE: f32 = 1.0;
pub const SURF_PAD_TOP: f32 = 8.0;
pub const SURF_PAD_X: f32 = 12.0;
pub const SURF_PAD_BOTTOM: f32 = 10.0;
pub const OUT_GAP: f32 = 4.0;
pub const CTX_ROW: f32 = 30.0;
pub const OUTER_INSET: f32 = 5.0;
pub const INNER_PAD_X: f32 = 14.0;
pub const INNER_PAD_Y: f32 = 12.0;
/// The composer's height beside its rows: the outer box's edges, the context row, the inner
/// box's edges and padding, and its inset at the bottom.
pub const COMPOSER_FRAME: f32 = EDGE + CTX_ROW + 2.0 * (EDGE + INNER_PAD_Y) + OUTER_INSET + EDGE;

/// A card's header line (UI text), points.
pub const HEADER_H: f32 = 18.0;
/// Between items, points.
pub const CARD_GAP: f32 = 14.0;

/// A card's height: its header, its command's lines, and its panel (the rows shown, the fold
/// row when the output is long enough to fold, its padding and edge).
pub fn card_height(card: &Card, unfolded: bool, line_h: f32) -> f32 {
    let cmd_lines = match &card.cmd {
        Some(cmd) => cmd.lines().count().max(1),
        None => (card.cmd_rows.1.0 - card.cmd_rows.0.0 + 1).max(1) as usize,
    };
    let mut height = SURF_PAD_TOP + HEADER_H + cmd_lines as f32 * line_h + SURF_PAD_BOTTOM;
    if let Some(output) = card.output {
        let lines = (output.1.0 - output.0.0 + 1).max(0) as usize;
        let s = shown(output, unfolded);
        let rows = (s.bottom.0 - s.top.0 + 1).max(0) as usize + usize::from(lines > FOLD_OVER);
        height += OUT_GAP + rows as f32 * line_h;
    }
    height
}

/// An item's height.
pub fn item_height(item: &Item, unfolded: bool, line_h: f32) -> f32 {
    let rows = |top: Line, bottom: Line| (bottom.0 - top.0 + 1).max(0) as f32 * line_h;
    match item {
        Item::Card(card) => card_height(card, unfolded, line_h),
        Item::Earlier { top, bottom } | Item::Loose { top, bottom } => rows(*top, *bottom),
        Item::Live { top, bottom, .. } => COMPOSER_FRAME + rows(*top, *bottom),
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

/// Where the first live row is drawn from (rework): `from` the column the typing starts at (0
/// when unknown), and `cut` where your prompt's right-hand part starts (a run after two blank
/// cells reaching the line's end, as zsh right-aligns it), when there is one past `from`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Crop {
    pub from: usize,
    pub cut: Option<usize>,
}

pub fn crop(line: &[CellView], input_col: Option<usize>, cursor_col: Option<usize>) -> Crop {
    let blank = |c: &CellView| (c.ch == ' ' || c.ch == '\0') && !c.spacer;
    let from = input_col.unwrap_or(0).min(line.len());
    let end = line.iter().rposition(|c| !blank(c));
    let at_edge = end.is_some_and(|e| e + 2 >= line.len());
    // Nothing you typed is cut (review #3): the cursor must be on this row (the typing hasn't
    // wrapped on), and the right part start past it.
    let cut = crate::block_view::right_run(line, blank)
        .filter(|&start| at_edge && start > from && cursor_col.is_some_and(|c| start > c));
    Crop { from, cut }
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
    /// The column last built and what it was built from (cache generation, history size,
    /// cursor row, last text row, UTC offset): rebuilt only when one moves (final review #7).
    pub items: Vec<Item>,
    pub items_key: Option<(u64, usize, i32, i32, i64)>,
    /// The column's view last frame (window points, `x, y, w, h`): hit areas are cut to it.
    pub view_rect: (f32, f32, f32, f32),
    /// The composer's top-left and width last frame (window points): its context row is
    /// placed there (rework).
    pub composer: Option<(f32, f32, f32)>,
    /// The composer's first row as painted last frame, as text (the scenes check what shows).
    #[cfg(feature = "ui-shots")]
    pub typed_row: String,
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

/// The scroll for a column that grew from `old_total` to `total` (final review #4): scrolled up,
/// it grows by as much, so what you read stays put; at the bottom it stays at the bottom.
pub fn keep_view(scroll: f32, old_total: f32, total: f32) -> f32 {
    if scroll <= 0.0 || old_total <= 0.0 {
        return scroll;
    }
    (scroll + total - old_total).max(0.0)
}

/// Shift+Page Up (`up`) / Down in card view (final review #6): a view's height, within
/// `0..=max`.
pub fn page(scroll: f32, view: f32, max: f32, up: bool) -> f32 {
    let step = if up { view } else { -view };
    (scroll + step).clamp(0.0, max.max(0.0))
}

/// A card's hit areas cut to the column's view `(x, y, w, h)` (final review #5): none when the
/// card is out of view; an area out of view shrinks to nothing (height 0).
pub fn clip_hit(hit: &CardHit, view: (f32, f32, f32, f32)) -> Option<CardHit> {
    let (top, bottom) = (view.1, view.1 + view.3);
    let cut = |(x, y, w, h): (f32, f32, f32, f32)| {
        let (a, b) = (y.max(top), (y + h).min(bottom));
        (x, a, w, (b - a).max(0.0))
    };
    let card = cut(hit.card);
    if card.3 <= 0.0 {
        return None;
    }
    Some(CardHit {
        card,
        cmd: cut(hit.cmd),
        fold: hit.fold.map(cut).filter(|f| f.3 > 0.0),
        ..hit.clone()
    })
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
        /// The composer's top (its outer box; the context row, then the inner box below it).
        y: f32,
        height: f32,
        /// The rows in view, with the cursor among them (rows relative to `rows.y`).
        rows: Rows,
        cursor: Option<CursorView>,
        /// At a prompt whose typing start is known: what of the first row to show (rework).
        crop: Option<Crop>,
    },
}

/// A card's output panel: its box, the fold row's words (none when not long enough to fold),
/// and the rows in view.
#[derive(Clone, Debug)]
pub struct Panel {
    pub y: f32,
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
    state.cache.refresh(term, key);
    let grid = term.grid();
    let (cursor, last_text) = (grid.cursor.point.line, last_text_row(term));
    let items_key = (
        state.cache.generation,
        grid.history_size(),
        cursor.0,
        last_text.0,
        utc_offset,
    );
    if state.items_key != Some(items_key) {
        state.items = items_for(term, &mut state.cache, key, utc_offset);
        state.items_key = Some(items_key);
    }
    let items = std::mem::take(&mut state.items);
    let unfolded = &state.unfolded;
    let (placed, total) = layout(&items, &|id| unfolded.contains(id), line_h);
    // Scrolled up, what you read stays put as the column grows (final review #4).
    state.scroll = keep_view(state.scroll, state.total, total);
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
                // Inside the card's surface (rework): its padding, the header, the command.
                let cmd_y = y + SURF_PAD_TOP + HEADER_H;
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
                    // The output under the command, in the same surface (no panel of its own).
                    let panel_y = cmd_y + cmd_lines as f32 * line_h + OUT_GAP;
                    let rows_y = panel_y + if fold.is_some() { line_h } else { 0.0 };
                    let shown_rows = count(s.top, s.bottom);
                    Panel {
                        y: panel_y,
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
            Item::Live {
                top,
                bottom,
                typing,
            } => {
                // In the composer's inner box: under the outer edge, the context row and the
                // inner box's edge and padding.
                let rows_y = y + EDGE + CTX_ROW + EDGE + INNER_PAD_Y;
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
                let on_first = (point.line == *top).then_some(point.column.0);
                let cut = typing
                    .filter(|_| rows.top == *top)
                    .and_then(|col| rows.lines.first().map(|l| crop(l, Some(col), on_first)));
                draws.push(Draw::Live {
                    y,
                    height: at.height,
                    rows,
                    cursor,
                    crop: cut,
                });
            }
        }
    }
    state.items = items;
    CardFrame { draws }
}

/// The terminal's size in card view (note 88 §4, Settled 4): the live box's inside at its
/// tallest, the pane less its padding (`terminal_view::inset`), the chips row, and the box's
/// edge and padding on each side.
pub fn card_dims(pane: Size<Pixels>, split: bool, cell_w: f32, line_h: f32) -> TermDims {
    let (pad_x, pad_y) = if split {
        ui_size::SPLIT_TERMINAL_PADDING
    } else {
        ui_size::TERMINAL_PADDING
    };
    let side = 2.0 * (EDGE + OUTER_INSET + EDGE + INNER_PAD_X);
    let w = f32::from(pane.width) - 2.0 * pad_x - side;
    let h = f32::from(pane.height) - 2.0 * pad_y - COMPOSER_FRAME;
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
            cmd_end: Line(top + 1),
            output: out.map(|(a, b)| (Line(a), Line(b))),
            cmd: Some(id.into()),
            t0: None,
            ms: None,
        })
    }

    fn prompt(id: &str, top: i32, bottom: i32) -> Segment {
        prompt_at(id, top, top, bottom)
    }

    /// A prompt from `top`, its typing on `row` (a two-line prompt: `row` below `top`).
    fn prompt_at(id: &str, top: i32, row: i32, bottom: i32) -> Segment {
        Segment::Prompt {
            id: id.into(),
            row: Line(row),
            top: Line(top),
            bottom: Line(bottom),
            input: None,
        }
    }

    fn ids(items: &[Item]) -> Vec<String> {
        items
            .iter()
            .map(|i| match i {
                Item::Earlier { top, bottom } => format!("earlier {}..{}", top.0, bottom.0),
                Item::Card(c) => format!("card {}{}", c.id, if c.failed { " failed" } else { "" }),
                Item::Loose { top, bottom } => format!("loose {}..{}", top.0, bottom.0),
                Item::Live { top, bottom, .. } => format!("live {}..{}", top.0, bottom.0),
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
        // Final review #3: rows above the screen are earlier rows, the box keeps to the screen.
        let items = column(&[], Line(-20), Line(3), Line(5), 0);
        assert_eq!(ids(&items), ["earlier -20..-1", "live 0..5"]);
        assert_eq!(
            ids(&column(&[], Line(0), Line(3), Line(5), 0)),
            ["live 0..5"]
        );
    }

    #[test]
    fn a_running_command_is_in_the_live_box() {
        let segments = [
            command("ls", 0, Some((2, 2)), Outcome::Ok),
            command("npm run dev", 3, Some((5, 40)), Outcome::Running),
        ];
        let items = column(&segments, Line(0), Line(40), Line(40), 0);
        // Rework: only its output (its command is in the chip above the box).
        assert_eq!(ids(&items), ["card ls", "live 5..40"]);
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
            ["card ls", "loose -47..-31", "loose -28..-1", "live 0..40"]
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
        // Rework: the inner box's inside: less the pane's padding, the outer box's edge and
        // inset and the inner box's edge and padding each side; the context row above.
        let pane = gpui::size(gpui::px(1000.0), gpui::px(700.0));
        let side = 2.0 * (EDGE + OUTER_INSET + EDGE + INNER_PAD_X);
        let tall = EDGE + CTX_ROW + 2.0 * (EDGE + INNER_PAD_Y) + OUTER_INSET + EDGE;
        let (w, h) = (1000.0 - 2.0 * 28.0 - side, 700.0 - 2.0 * 22.0 - tall);
        let dims = card_dims(pane, false, 8.0, 17.0);
        assert_eq!(
            (dims.cols, dims.rows),
            ((w / 8.0) as usize, (h / 17.0) as usize)
        );
        assert_eq!((dims.cols, dims.rows), (112, 34));
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
        // Final review #7: the rotation is found by looking up from the newest card's old row
        // (no full walk), and the cached segments are shifted by it.
        let (segments, rebuilt) = cache.segments(&sh.term, key(1, 0));
        assert!(!rebuilt, "a rotation shifts the cache, no full walk");
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
        // Rework: one surface, its padding around the header, the command and the output.
        let pad = SURF_PAD_TOP + SURF_PAD_BOTTOM;
        let frame = pad + OUT_GAP;
        let card = a_card(Some((2, 4)));
        assert_eq!(
            card_height(&card, false, lh),
            HEADER_H + lh + frame + 3.0 * lh
        );
        assert_eq!(card_height(&a_card(None), false, lh), HEADER_H + lh + pad);
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
                typing: None,
            },
        ];
        let (placed, total) = layout(&items, &|_| false, lh);
        assert_eq!(placed[0].top, 0.0);
        assert_eq!(placed[1].top, card_height(&card, false, lh) + CARD_GAP);
        assert_eq!(total, placed[1].top + COMPOSER_FRAME + 2.0 * lh);
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
    fn a_chosen_card_is_scrolled_into_view() {
        // A 2000 pt column in a 600 pt view: a card at 400..520 needs the view's top at 400,
        // so the column scrolls up (from its bottom) by 2000 - 600 - 400.
        assert_eq!(reveal(400.0, 120.0, 2000.0, 600.0, 0.0), 1000.0);
        // Already in view (the view shows 1400..2000 at scroll 0): unchanged.
        assert_eq!(reveal(1500.0, 100.0, 2000.0, 600.0, 0.0), 0.0);
        // Taller than the view: its top at the view's top.
        assert_eq!(reveal(100.0, 900.0, 2000.0, 600.0, 0.0), 1300.0);
    }

    // Final review fixes.

    /// #4: scrolled up, new output (a taller column) keeps what you read in place; at the
    /// bottom it stays at the bottom.
    #[test]
    fn scrolled_up_the_view_stays_put_as_the_column_grows() {
        assert_eq!(keep_view(120.0, 1000.0, 1040.0), 160.0);
        assert_eq!(
            keep_view(0.0, 1000.0, 1040.0),
            0.0,
            "at the bottom: stays there"
        );
        assert_eq!(
            keep_view(120.0, 1000.0, 900.0),
            20.0,
            "a shorter column (a clear)"
        );
        assert_eq!(keep_view(120.0, 0.0, 1040.0), 120.0, "no last frame yet");
    }

    /// #6: Shift+Page Up / Down move the column by a view's height, within its bounds.
    #[test]
    fn page_keys_scroll_the_column() {
        assert_eq!(page(0.0, 600.0, 2000.0, true), 600.0);
        assert_eq!(page(1800.0, 600.0, 2000.0, true), 2000.0);
        assert_eq!(page(500.0, 600.0, 2000.0, false), 0.0);
    }

    /// #5: a card's hit areas are cut to the column's view: the part scrolled above it takes no
    /// presses (it would lie over the pane's tab bar).
    #[test]
    fn hits_are_cut_to_the_view() {
        let hit = CardHit {
            id: "c".into(),
            card: (10.0, -50.0, 300.0, 200.0),
            cmd: (10.0, -32.0, 300.0, 17.0),
            fold: Some((21.0, 100.0, 278.0, 17.0)),
            command: None,
            outcome: Outcome::Ok,
        };
        let cut = clip_hit(&hit, (0.0, 0.0, 400.0, 600.0)).expect("partly in view");
        assert_eq!(cut.card, (10.0, 0.0, 300.0, 150.0));
        assert_eq!(
            cut.cmd.3, 0.0,
            "the command line is above the view: no handle"
        );
        assert_eq!(cut.fold, hit.fold, "the fold row is in view");
        assert!(
            clip_hit(&hit, (0.0, 200.0, 400.0, 600.0)).is_none(),
            "wholly out of view"
        );
    }

    // M11ab rework, Task 1.

    /// A wrapped command line's second row is its card's (it was left between cards).
    #[test]
    fn a_wrapped_command_row_stays_in_its_card() {
        let Segment::Command(mut b) = command("make", 0, None, Outcome::Ok) else {
            unreachable!()
        };
        b.cmd_end = Line(2);
        let segments = [Segment::Command(b), prompt("live", 3, 3)];
        let items = column(&segments, Line(0), Line(3), Line(3), 0);
        assert_eq!(ids(&items), ["card make", "live 3..3"], "no loose row 2");
    }

    /// The box starts where the typing does: a two-line prompt's upper row is left out.
    #[test]
    fn the_live_box_starts_at_the_typing() {
        let segments = [
            command("ls", 0, Some((2, 2)), Outcome::Ok),
            prompt_at("live", 3, 4, 4),
        ];
        let items = column(&segments, Line(0), Line(4), Line(4), 0);
        assert_eq!(ids(&items), ["card ls", "live 4..4"]);
    }

    /// A running command: only its output in the box (its prompt and command rows left out).
    #[test]
    fn a_running_command_shows_only_its_output() {
        let segments = [command("npm run dev", 0, Some((2, 9)), Outcome::Running)];
        let items = column(&segments, Line(0), Line(9), Line(9), 0);
        assert_eq!(ids(&items), ["live 2..9"]);
        // Nothing printed yet: the box is the cursor's row.
        let segments = [command("sleep 9", 0, None, Outcome::Running)];
        let items = column(&segments, Line(0), Line(2), Line(1), 0);
        assert_eq!(ids(&items), ["live 2..2"]);
    }

    fn cells(text: &str, cols: usize) -> Vec<CellView> {
        let theme = crate::theme::Theme::default();
        let mut line: Vec<CellView> = text
            .chars()
            .map(|ch| CellView {
                ch,
                ..CellView::blank(&theme)
            })
            .collect();
        line.resize(cols, CellView::blank(&theme));
        line
    }

    /// The prompt's left part (up to where the typing starts) and its right part (a run at the
    /// line's end after two blank cells) are left out; what you typed stays.
    #[test]
    fn the_prompt_and_its_right_side_are_left_out() {
        let row = format!("{:<34}{}", "~/code/api main ls", "\u{2714} 08:00");
        let line = cells(&row, 40);
        assert_eq!(
            crop(&line, Some(16), Some(18)),
            Crop {
                from: 16,
                cut: Some(34)
            }
        );
        // A plain `$ ` prompt: no right part.
        assert_eq!(
            crop(&cells("$ ls", 40), Some(2), Some(4)),
            Crop { from: 2, cut: None }
        );
        // A double space typed mid-line isn't a right prompt (the run doesn't reach the edge).
        assert_eq!(
            crop(&cells("$ git commit -m  x", 40), Some(2), Some(18)),
            Crop { from: 2, cut: None }
        );
        // Where the typing starts unknown (a reattached session): the whole row.
        assert_eq!(
            crop(&line, None, Some(18)),
            Crop {
                from: 0,
                cut: Some(34)
            }
        );
    }

    // Rework review fixes.

    /// #1/#2: the box starts at the row the typing is on (a wrapped prompt's second row), and
    /// carries the typing's column for the crop.
    #[test]
    fn a_wrapped_prompt_starts_the_box_at_the_typing() {
        let live = Segment::Prompt {
            id: "live".into(),
            row: Line(3),
            top: Line(3),
            bottom: Line(4),
            input: Some((Line(4), 5)),
        };
        let items = column(&[live], Line(0), Line(4), Line(4), 0);
        assert_eq!(
            items.last(),
            Some(&Item::Live {
                top: Line(4),
                bottom: Line(4),
                typing: Some(5),
            })
        );
    }

    /// #1: a prompt on the screen's last row, then lines printed under it and the cursor moved
    /// back up (a completion list): the box still starts at the typing, with its column.
    #[test]
    fn the_crop_survives_a_scroll() {
        let mut sh = Sh::new(100);
        sh.command("ls", "a\r\nb\r\nc\r\n").prompt();
        sh.feed(b"x\r\n\r\n\r\n\x1b[3A\x1b[3C");
        let items = items_for(&sh.term, &mut SegmentCache::default(), key(2, 0), 0);
        let Some(Item::Live { typing, top, .. }) = items.last() else {
            panic!("a live box")
        };
        assert_eq!(*typing, Some(2), "the typing's column, where B was");
        assert!(
            tag_id_at(&sh.term, *top).is_some(),
            "the box starts on the tag's row"
        );
    }

    /// #3: nothing you typed is cut: no cut at or left of the cursor, none when the cursor has
    /// moved past the row (the typing wraps on).
    #[test]
    fn typed_text_is_never_cut() {
        let row = format!("{:<34}{}", "$ git commit -m a  b", "\u{2714} 08:00");
        let line = cells(&row, 40);
        assert_eq!(
            crop(&line, Some(2), Some(20)).cut,
            Some(34),
            "a right prompt"
        );
        assert_eq!(
            crop(&line, Some(2), Some(36)).cut,
            None,
            "the cursor is past it"
        );
        assert_eq!(
            crop(&line, Some(2), None).cut,
            None,
            "the typing wrapped on"
        );
    }
}
