//! A file tab (M11g): a pane tab that shows a file, read-only. Nothing here writes to the file;
//! the one process it starts is `/usr/bin/open -t <path>` (Open in Editor, a toolbar button:
//! the default text editor, never the file's own app, which could run it), or for a refused file
//! `/usr/bin/open -R <path>` (Show in Finder: revealed, never launched).
//!
//! The file is read, coloured (`highlight::Highlighter`, one shared GPUI global loaded once in
//! the background) and compared with HEAD (`git_status::line_changes`) on the background
//! executor; the UI thread only swaps the results in. The text shows as soon as it is read,
//! plain, and the colours follow.
//!
//! **Live reload.** While the tab is on screen (the workspace says so every frame: `set_shown`)
//! and the window can be seen, the disk is checked every second: its mtime and length, off the
//! UI thread, and a changed file is read again. The scroll stays. Each read is a new `version`;
//! a highlight or a git answer carries the version it was asked for: it is taken for the text
//! shown, when nothing is held yet, or when newer than the one held (old colours and bars are
//! carried over a reload, so a file that grows faster than it's coloured still gets fresher
//! colours), else dropped (`Doc::accept_spans`, `Doc::accept_changes`). One highlight and one git look-up run at a time
//! per tab (`OneAtATime`): asked again meanwhile, each runs once more when it ends, for the text
//! then shown. git runs at most once every 2 s per tab, its program and repo kept once found (a
//! failed look-up is tried again); a failed or slow answer leaves the bars as they are. Only
//! plain files are read (a FIFO or a device is refused), never past 20 MB. The UI screenshots
//! freeze it (`frozen`): no polling, no git.
//!
//! **Find** runs on the background executor too (one search at a time; only the latest search's
//! answer is taken) and keeps at most 10,000 matches ("10,000+"). It scrolls to a match for a new
//! query or a step (⌘G, ⇧⌘G, Return), never when a reload searches again.
//!
//! **Drawing.** One canvas draws the lines in sight (not an element per line): the changed
//! lines' tints, the selection, find's matches and the text (one shaped line per source line),
//! then the gutter's numbers and change bars. Tabs show as four spaces; long lines scroll
//! sideways. A long line (over 1,000 cells) keeps checkpoints, made off the UI thread with the
//! text: only its part in sight is sliced out, shaped and hit-tested, and only the matches over
//! that part (and in sight) are drawn.
//!
//! **Diff mode (M11h).** A tab shows its file as it is (`FileMode::Plain`) or its whole diff
//! against the last commit (`FileMode::Diff`): old and new lines in one column (`DiffDoc`, one
//! line per row, so find and copy work on it unchanged), removed rows tinted red with a `−`,
//! added rows green with a `+`, unchanged rows' colours dimmed, both line numbers in the gutter
//! (`columns`). The toolbar's File | Diff toggle (`shows_toggle`) and a press on a change bar
//! switch modes; each mode keeps its own scroll. In Diff mode the git look-up asks for the
//! diff instead of the bars, on the same schedule, off the UI thread (`look_up_diff`); its
//! answer carries the disk version, as the bars' does (`HeldDiff`). A refused file (over 20 MB
//! or binary) never asks git.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ffi::OsString;
use std::io::Read;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use alacritty_terminal::vte::ansi::Rgb;
use gpui::{
    AnyElement, AnyView, App, Bounds, ClickEvent, ClipboardItem, ContentMask, Context, CursorStyle,
    DispatchPhase, Entity, EventEmitter, FocusHandle, Focusable, Global, HitboxBehavior,
    ListAlignment, ListState, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels,
    Point, ScrollWheelEvent, SharedString, Subscription, TextAlign, TextRun, WeakEntity, Window,
    canvas, div, fill, list, point, prelude::*, px, size,
};

use crate::agent_markdown::Md;
use crate::appearance::Appearance;
use crate::diff_view::{
    BINARY, DiffRow, NO_CHANGES, NO_REPO, Outcome, Parsed, RowKind, context_for, diff_outcome,
    diff_spans, dim, first_change, reveal_row, sides,
};
use crate::git_branch::find_git;
use crate::git_status::{FileDiff, LineChange, file_diff, head_size, line_changes, repo_root_at};
use crate::highlight::{Highlighter, PLAIN_ABOVE, REFUSE_ABOVE, Span, Viewable, classify};
use crate::layout_model::{FileMode, PaneId, TabId, TabKind};
use crate::md_preview::{self, LinkTarget, is_markdown, link_target};
use crate::mermaid::{self, Asked, Key};
use crate::mermaid_render::{Drawn, Mermaids, Waiter};
use crate::name_input::{NameInput, NameInputEvent};
use crate::overlay_panel;
use crate::reduce_motion::WindowVisible;
use crate::sidebar_toggle::Tip;
use crate::tab_bar::{Glyph, glyph, glyph_with};
use crate::terminal_view::{CellMetrics, Copy, hsla};
use crate::theme::{Theme, UiColors, mix};
use crate::ui_tokens::size::CARD_RADIUS;
use crate::ui_tokens::{MARK_MIN, ensure_contrast, text};
use crate::workspace::Workspace;

// Bound in main.rs in the "FileTab" key context: ⌘F, ⌘G, ⇧⌘G, and Esc to close the find bar.
gpui::actions!(file_tab, [Find, FindNext, FindPrev, CloseFind]);

/// The notes the tab shows for a file it can't colour or show.
pub const MISSING: &str = "This file no longer exists";
pub const REFUSED: &str = "This file is too large or binary to show";
pub const TOO_LARGE: &str = "Too large to colour";
const OPEN_IN_EDITOR: &str = "Open in Editor";
/// The toolbar's File | Diff toggle (M11h): its words and tooltips.
const TOGGLE_FILE: &str = "File";
const TOGGLE_DIFF: &str = "Diff";
const WRAP: &str = "Wrap";
const TIP_WRAP: &str = "Wrap long lines at the tab\u{2019}s width";
const PREVIEW: &str = "Preview";
const TIP_PREVIEW: &str = "Show the Markdown rendered";
const TIP_PREVIEW_TOO_LARGE: &str = "Too large to preview";
const TIP_FILE: &str = "Show the file";
const TIP_DIFF: &str = "Show the changes since the last commit";
const SHOW_IN_FINDER: &str = "Show in Finder";
const FIND_PLACEHOLDER: &str = "Find in file";

/// How many spaces a tab shows as.
pub const TAB_WIDTH: usize = 4;
/// How often a tab on screen looks at the disk.
const POLL: Duration = Duration::from_secs(1);
/// Every this many looks, git is asked for the change bars again (a commit changes them
/// without touching the file).
const CHANGES_EVERY: u32 = 5;
/// The bytes `classify` looks at for a NUL.
const HEAD: usize = 8 * 1024;
/// A line longer than this (bytes) is not coloured: syntect crawls on minified lines. The file
/// shows as plain text, "Too large to colour".
const COLOUR_LINE_MAX: usize = 20_000;
/// A line wider than this (cells) is shaped only where it is in sight, and found from its
/// checkpoints (one every `CHECKPOINT` cells), never scanned from its start each frame.
const LONG_LINE: usize = 1_000;
const CHECKPOINT: usize = 256;

// The view's geometry, in points.
const TOOLBAR: f32 = crate::browser_tab::BAR;
const BAR_PAD: f32 = 6.0;
const BUTTON: f32 = 24.0;
const BUTTON_STROKE: f32 = 1.75;
const FIND_BOX: f32 = 260.0;
const GUTTER_LEFT: f32 = 12.0;
/// The room above the first line (M11i: as the gutter's at its left).
const TOP_ROOM: f32 = 12.0;
/// The room below the last line (M11i): as tall as the card's rounded corners, so no line's
/// tint, match or selection reaches them.
const BOTTOM_ROOM: f32 = CARD_RADIUS;
const NUMBER_GAP: f32 = 8.0;
const BAR_WIDTH: f32 = 3.0;
const TEXT_GAP: f32 = 10.0;
const RIGHT_PAD: f32 = 24.0;
/// A changed line's tint: its bar's colour at this opacity.
const TINT: f32 = 0.12;
/// Find's matches: the accent at these opacities (the current one stronger).
const MATCH_TINT: f32 = 0.25;
const CURRENT_TINT: f32 = 0.55;
/// The deleted-lines ▸, larger than the text so it reads at a glance.
const MARKER_SCALE: f32 = 1.3;
/// A diff opens (and a change bar's click shows its change) this many rows from the top.
const REVEAL_ABOVE: usize = 3;
/// A diff row's sign: removed (U+2212) and added.
const SIGN_REMOVED: &str = "\u{2212}";
const SIGN_ADDED: &str = "+";

/// A place in the file: a (0-based) line and a byte offset in it (the source text, tabs as tabs).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Pos {
    pub line: usize,
    pub col: usize,
}

/// A find match: its line and its bytes in that line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Match {
    pub line: usize,
    pub cols: Range<usize>,
}

/// The text's lines as byte ranges, each without its `\n` (or `\r\n`); a last `\n` ends the last
/// line (`"a\n"` is one line, as the highlighter and git count them); an empty text is one empty
/// line.
pub fn line_ranges(text: &str) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let mut at = 0;
    for piece in text.split_inclusive('\n') {
        let line = piece.strip_suffix('\n').unwrap_or(piece);
        let line = line.strip_suffix('\r').unwrap_or(line);
        out.push(at..at + line.len());
        at += piece.len();
    }
    if out.is_empty() {
        out.push(0..0);
    }
    out
}

/// A line as shown: each tab as four spaces.
pub fn expand_tabs(line: &str) -> String {
    line.replace('\t', &" ".repeat(TAB_WIDTH))
}

/// A line's tabs (their byte offsets) and its length, to map between its bytes and its shown
/// bytes.
struct Tabs {
    at: Vec<usize>,
    len: usize,
}

impl Tabs {
    fn of(line: &str) -> Tabs {
        Tabs {
            at: line
                .bytes()
                .enumerate()
                .filter(|&(_, b)| b == b'\t')
                .map(|(i, _)| i)
                .collect(),
            len: line.len(),
        }
    }

    /// Source byte → shown byte (past the end: the end).
    fn shown(&self, col: usize) -> usize {
        let col = col.min(self.len);
        col + self.at.partition_point(|&t| t < col) * (TAB_WIDTH - 1)
    }

    /// Shown byte → source byte (past the end: the end); inside a tab, the nearer of its edges.
    fn source(&self, shown: usize) -> usize {
        for (k, &t) in self.at.iter().enumerate() {
            let start = t + k * (TAB_WIDTH - 1);
            if shown < start {
                return shown - k * (TAB_WIDTH - 1);
            }
            if shown < start + TAB_WIDTH {
                return if shown - start < TAB_WIDTH / 2 {
                    t
                } else {
                    t + 1
                };
            }
        }
        (shown - self.at.len() * (TAB_WIDTH - 1)).min(self.len)
    }
}

/// Where source byte `col` of `line` is shown (tabs as four spaces). The view maps with `Tabs`.
#[cfg(test)]
pub fn display_col(line: &str, col: usize) -> usize {
    Tabs::of(line).shown(col)
}

/// The source byte of `line` shown at byte `display` (inside a tab: the nearer edge; past the
/// end: the end).
#[cfg(test)]
pub fn source_col(line: &str, display: usize) -> usize {
    Tabs::of(line).source(display)
}

/// `query`'s matches in `text`'s lines, in order, ignoring case; none across a line's end, none
/// overlapping, none for an empty query.
pub fn find_matches(
    text: &str,
    lines: &[Range<usize>],
    query: &str,
    cap: usize,
) -> (Vec<Match>, bool) {
    let mut found = find_upto(text, lines, query, cap + 1);
    let capped = found.len() > cap;
    found.truncate(cap);
    (found, capped)
}

/// At most `limit` of `query`'s matches.
fn find_upto(text: &str, lines: &[Range<usize>], query: &str, limit: usize) -> Vec<Match> {
    let mut out = Vec::new();
    if query.is_empty() {
        return out;
    }
    if query.is_ascii() {
        // Byte by byte: an ASCII byte never sits inside a multi-byte character, so every match
        // starts and ends on a character boundary.
        let q = query.as_bytes();
        for (i, r) in lines.iter().enumerate() {
            let l = &text.as_bytes()[r.clone()];
            let mut at = 0;
            while at + q.len() <= l.len() {
                if l[at..at + q.len()].eq_ignore_ascii_case(q) {
                    out.push(Match {
                        line: i,
                        cols: at..at + q.len(),
                    });
                    if out.len() >= limit {
                        return out;
                    }
                    at += q.len();
                } else {
                    at += 1;
                }
            }
        }
        return out;
    }
    let q: Vec<char> = query.chars().flat_map(char::to_lowercase).collect();
    for (i, r) in lines.iter().enumerate() {
        let l = &text[r.clone()];
        let mut at = 0;
        while at < l.len() {
            match lower_match(&l[at..], &q) {
                Some(len) => {
                    out.push(Match {
                        line: i,
                        cols: at..at + len,
                    });
                    if out.len() >= limit {
                        return out;
                    }
                    at += len;
                }
                None => at += l[at..].chars().next().map_or(1, char::len_utf8),
            }
        }
    }
    out
}

/// The bytes of `rest`'s start that lower-case to `q`, if they do (whole characters only).
fn lower_match(rest: &str, q: &[char]) -> Option<usize> {
    let mut k = 0;
    for (i, c) in rest.char_indices() {
        for lc in c.to_lowercase() {
            if q.get(k) != Some(&lc) {
                return None;
            }
            k += 1;
        }
        if k == q.len() {
            return Some(i + c.len_utf8());
        }
    }
    None
}

/// The next (or previous) of `count` matches after `current`, round the ends; with none current,
/// the first (or last).
pub fn step(current: Option<usize>, count: usize, forward: bool) -> Option<usize> {
    if count == 0 {
        return None;
    }
    Some(match (current, forward) {
        (None, true) => 0,
        (None, false) => count - 1,
        (Some(c), true) => (c + 1) % count,
        (Some(c), false) => (c + count - 1) % count,
    })
}

/// The first match on or after `line`, else the first of all.
pub fn first_from(matches: &[Match], line: usize) -> Option<usize> {
    matches
        .iter()
        .position(|m| m.line >= line)
        .or_else(|| (!matches.is_empty()).then_some(0))
}

/// The find bar's count: "2 of 7", "1 of 10,000+" (`capped`: there are more), "No results", or
/// nothing while the box is empty.
pub fn find_label(query: &str, current: Option<usize>, count: usize, capped: bool) -> String {
    if query.is_empty() {
        String::new()
    } else if count == 0 {
        "No results".to_string()
    } else {
        let more = if capped { "+" } else { "" };
        format!(
            "{} of {}{more}",
            thousands(current.map_or(0, |c| c + 1)),
            thousands(count)
        )
    }
}

/// `n` with commas between thousands.
fn thousands(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// Byte `col` of `line`, at most its end, back to a character boundary.
fn clamp_col(line: &str, col: usize) -> usize {
    let mut col = col.min(line.len());
    while !line.is_char_boundary(col) {
        col -= 1;
    }
    col
}

/// The text between `a` and `b` (either way round): the first and last lines in part, `\n`
/// between lines (a CRLF file's `\r` is not copied).
pub fn selection_text(text: &str, lines: &[Range<usize>], a: Pos, b: Pos) -> String {
    let (s, e) = if a <= b { (a, b) } else { (b, a) };
    let mut out = String::new();
    let Some(last) = lines.len().checked_sub(1) else {
        return out;
    };
    for i in s.line..=e.line.min(last) {
        let l = &text[lines[i].clone()];
        let from = if i == s.line { clamp_col(l, s.col) } else { 0 };
        let to = if i == e.line {
            clamp_col(l, e.col)
        } else {
            l.len()
        };
        if from < to {
            out.push_str(&l[from..to]);
        }
        if i != e.line {
            out.push('\n');
        }
    }
    out
}

/// A change list (1-based new lines, from `line_changes`) as bars by 0-based line, and the
/// edges where lines were deleted: edge `c` is above 0-based line `c` (0 is the top).
pub fn bars(changes: &[(usize, LineChange)]) -> (HashMap<usize, LineChange>, Vec<usize>) {
    let mut by_line = HashMap::new();
    let mut deleted = Vec::new();
    for &(line, change) in changes {
        match change {
            LineChange::DeletedAfter => deleted.push(line),
            _ if line > 0 => {
                by_line.insert(line - 1, change);
            }
            _ => {}
        }
    }
    deleted.sort_unstable();
    deleted.dedup();
    (by_line, deleted)
}

/// Background work of which one runs at a time (a tab's highlight, its git look-up): asked again
/// while one runs, it runs once more when that one ends, with whatever is current then.
#[derive(Default, Debug)]
pub struct OneAtATime {
    busy: bool,
    again: bool,
}

impl OneAtATime {
    /// Whether to start now (else it's queued to run once the running one ends).
    pub fn start(&mut self) -> bool {
        if self.busy {
            self.again = true;
            return false;
        }
        self.busy = true;
        true
    }

    /// The running one ended: whether to run once more.
    pub fn finish(&mut self) -> bool {
        self.busy = false;
        std::mem::take(&mut self.again)
    }
}

/// The most matches find keeps; past that the count shows as "10,000+".
pub const FIND_CAP: usize = 10_000;

/// What the find bar found, and which search (by generation) it waits for.
#[derive(Default, Debug)]
pub struct Finds {
    pub query: String,
    pub matches: Arc<Vec<Match>>,
    /// More than `FIND_CAP` matched.
    pub capped: bool,
    pub current: Option<usize>,
    wanted: u64,
    /// The query changed since the last answer: its current match is the first from the top.
    from_top: bool,
}

impl Finds {
    /// A search asked as `generation`: for a new query, or (`None`) the same one on new text.
    pub fn ask(&mut self, generation: u64, query: Option<String>) {
        if let Some(query) = query
            && query != self.query
        {
            self.query = query;
            self.from_top = true;
        }
        self.wanted = generation;
    }

    /// Search `generation`'s answer: taken only if it's the latest asked (else `None`). A new
    /// query's current match is the first from line `top`, and it's to be scrolled to
    /// (`Some(true)`); a reload's keeps it by number, and the view stays put (`Some(false)`).
    pub fn accept(
        &mut self,
        generation: u64,
        found: Vec<Match>,
        capped: bool,
        top: usize,
    ) -> Option<bool> {
        if generation != self.wanted {
            return None;
        }
        let new_query = std::mem::take(&mut self.from_top);
        self.current = if new_query {
            first_from(&found, top)
        } else {
            self.current
                .filter(|_| !found.is_empty())
                .map(|c| c.min(found.len() - 1))
        };
        self.matches = Arc::new(found);
        self.capped = capped;
        Some(new_query)
    }
}

/// The least time between two git look-ups of a tab.
const LOOKUP_GAP: Duration = Duration::from_secs(2);

/// How long a git look-up waits: none when the last one started 2 s ago or more.
pub fn lookup_wait(last: Option<Instant>, now: Instant) -> Option<Duration> {
    last.and_then(|at| LOOKUP_GAP.checked_sub(now.saturating_duration_since(at)))
        .filter(|wait| !wait.is_zero())
}

/// The change bars of a loaded text.
#[derive(Default, PartialEq, Debug)]
pub struct Changes {
    pub by_line: HashMap<usize, LineChange>,
    pub deleted: Vec<usize>,
}

/// A loaded text: its lines, colours (once highlighted) and change bars (once git answered).
#[derive(Clone)]
pub struct Doc {
    pub text: Arc<str>,
    pub lines: Arc<Vec<Range<usize>>>,
    pub spans: Option<Arc<Vec<Vec<Span>>>>,
    /// The version the colours were made for (an older one's carried over a reload).
    pub spans_version: Option<u64>,
    pub changes: Arc<Changes>,
    /// The version the change bars were made for (an older one's carried over a reload).
    pub changes_version: Option<u64>,
    /// Which read of the file this is (each reload, one more).
    pub version: u64,
    /// The widest line as shown, in cells (characters, tabs as four).
    pub widest: usize,
    /// Each line's width in cells (M11x part 2: what Wrap cuts into rows).
    pub widths: Arc<Vec<usize>>,
    /// The long lines' checkpoints (`checkpoints`), by line: drawn and hit-tested from the
    /// nearest one, not from the line's start.
    long: Arc<HashMap<usize, Vec<(usize, usize)>>>,
    /// The longest line, in bytes.
    longest: usize,
}

impl Doc {
    pub fn new(text: String, version: u64) -> Doc {
        let lines = line_ranges(&text);
        let (mut widest, mut longest) = (0, 0);
        let mut long = HashMap::new();
        let mut widths = Vec::with_capacity(lines.len());
        for (i, r) in lines.iter().enumerate() {
            let l = &text[r.clone()];
            longest = longest.max(l.len());
            let cells = scan_cell(l, (0, 0), l.len());
            widths.push(cells);
            widest = widest.max(cells);
            if cells > LONG_LINE {
                long.insert(i, checkpoints(l));
            }
        }
        Doc {
            text: text.into(),
            lines: Arc::new(lines),
            spans: None,
            spans_version: None,
            changes: Arc::default(),
            changes_version: None,
            version,
            widest,
            widths: Arc::new(widths),
            long: Arc::new(long),
            longest,
        }
    }

    /// Whether line `i` is long (drawn only where it is in sight).
    fn is_long(&self, i: usize) -> bool {
        self.long.contains_key(&i)
    }

    /// The cell (characters, tabs four wide) where byte `col` of line `i` starts (past the end:
    /// the end).
    pub fn cell_of(&self, i: usize, col: usize) -> usize {
        let from = self.long.get(&i).map_or((0, 0), |marks| {
            marks[marks.partition_point(|&(b, _)| b <= col).max(1) - 1]
        });
        scan_cell(self.line(i), from, col)
    }

    /// The byte of line `i` whose character holds cell `cell` (inside a tab: the tab; past the
    /// end: the end), and the cell where that character starts.
    pub fn byte_at_cell(&self, i: usize, cell: usize) -> (usize, usize) {
        let from = self.long.get(&i).map_or((0, 0), |marks| {
            marks[marks.partition_point(|&(_, c)| c <= cell).max(1) - 1]
        });
        scan_byte(self.line(i), from, cell)
    }

    /// Line `i` (without its ending); empty past the end.
    pub fn line(&self, i: usize) -> &str {
        self.lines.get(i).map_or("", |r| &self.text[r.clone()])
    }

    /// Colours made for `version`: taken if that is the text shown, or newer than the colours
    /// held (an older version's, carried over: a file that grows faster than it's coloured
    /// still gets fresher colours).
    pub fn accept_spans(&mut self, version: u64, spans: Vec<Vec<Span>>) -> bool {
        if !fresher(version, self.version, self.spans_version) {
            return false;
        }
        self.spans = Some(Arc::new(spans));
        self.spans_version = Some(version);
        true
    }

    /// Change bars asked for `version`, taken as colours are (`accept_spans`). True when they
    /// changed.
    pub fn accept_changes(&mut self, version: u64, changes: &[(usize, LineChange)]) -> bool {
        if !fresher(version, self.version, self.changes_version) {
            return false;
        }
        self.changes_version = Some(version);
        let (by_line, deleted) = bars(changes);
        let changes = Changes { by_line, deleted };
        if *self.changes == changes {
            return false;
        }
        self.changes = Arc::new(changes);
        true
    }
}

/// Whether an answer for `version` may replace one for `held` while `shown` is the text shown:
/// never newer than the text; with nothing held, any (a file that changes faster than its first
/// colouring still gets colours); else newer than what's held.
fn fresher(version: u64, shown: u64, held: Option<u64>) -> bool {
    version <= shown && held.is_none_or(|held| version > held || version == shown)
}

/// A character's width in cells: a tab is four.
fn cell_width(c: char) -> usize {
    if c == '\t' { TAB_WIDTH } else { 1 }
}

/// From `from` (a byte of `line` and its cell), the cell where byte `col` starts.
fn scan_cell(line: &str, from: (usize, usize), col: usize) -> usize {
    let col = clamp_col(line, col).max(from.0);
    from.1 + line[from.0..col].chars().map(cell_width).sum::<usize>()
}

/// From `from`, the character of `line` holding cell `cell`: its byte and its first cell.
fn scan_byte(line: &str, from: (usize, usize), cell: usize) -> (usize, usize) {
    let (mut b, mut c) = from;
    for ch in line[b..].chars() {
        let w = cell_width(ch);
        if c + w > cell {
            break;
        }
        c += w;
        b += ch.len_utf8();
    }
    (b, c)
}

/// A long line's checkpoints: (byte, cell) at its start and then every `CHECKPOINT` cells or so,
/// each at a character's start.
fn checkpoints(line: &str) -> Vec<(usize, usize)> {
    let mut marks = vec![(0, 0)];
    let mut cell = 0;
    for (b, ch) in line.char_indices() {
        if cell >= marks[marks.len() - 1].1 + CHECKPOINT {
            marks.push((b, cell));
        }
        cell += cell_width(ch);
    }
    marks
}

/// What a file tab shows.
pub enum State {
    /// Being read for the first time.
    Loading,
    Text(Doc),
    /// Over 2 MB (or with a line too long to colour): plain text, "Too large to colour".
    PlainTooLarge(Doc),
    /// Over 20 MB or binary: "This file is too large or binary to show".
    Refuse,
    /// Gone: "This file no longer exists".
    Missing,
}

/// What a file tab in Diff mode shows (M11h).
#[derive(Default)]
pub enum DiffState {
    /// Not asked yet (or no answer yet).
    #[default]
    Waiting,
    Rows(DiffDoc),
    /// "No changes since the last commit."
    Same,
    /// "This file is binary: no diff to show." (git's answer, or binary on disk.)
    Binary,
    /// "Not a git repository."
    NoRepo,
    /// The file on disk is over 20 MB (or can't be read): git isn't asked (it would stream the
    /// whole old blob); or its old blob in HEAD, or git's diff, is over 20 MB. "This file is too
    /// large or binary to show". (Binary on disk is `Binary`: `refused_diff`.)
    Refuse,
}

/// A diff's rows as a text to draw, find in and copy (one line per row, no signs or numbers).
#[derive(Clone)]
pub struct DiffDoc {
    pub doc: Doc,
    pub rows: Arc<Vec<DiffRow>>,
    /// Its two number columns' widths, worked out once.
    pub gutter: Gutter,
    /// Whether it may be coloured: both its sides at most 2 MB (`PLAIN_ABOVE`) and no line too
    /// long to colour. Decided by the rows, not the file on disk (a deleted file has none).
    pub colourable: bool,
}

/// The diff held and the disk version it was asked for: an answer for an older version than
/// the one held is dropped, as change bars are (`Doc::accept_changes`).
#[derive(Default)]
pub struct HeldDiff {
    pub state: DiffState,
    pub version: Option<u64>,
}

impl HeldDiff {
    /// `state`, asked for disk version `version` while `shown` is the latest read: taken when
    /// fresher than the one held (`fresher`).
    pub fn accept(&mut self, version: u64, shown: u64, state: DiffState) -> bool {
        if !fresher(version, shown, self.version) {
            return false;
        }
        self.version = Some(version);
        self.state = state;
        true
    }
}

/// The diff's rows held, coloured: their text (the `Doc`'s) and rows. A look-up whose parse
/// gives the same isn't coloured again (`diff_state`): a diff that hasn't changed costs no
/// highlighting, and `accept_diff` keeps the rows and colours held.
#[derive(Clone)]
struct ColouredRows {
    text: Arc<str>,
    rows: Arc<Vec<DiffRow>>,
}

/// A diff's rows coloured: the old and the new side each highlighted whole as `path`, each row
/// taking its side's colours (blocking: the background executor's).
fn colour_rows(
    h: &Highlighter,
    path: &Path,
    rows: &[DiffRow],
    (old, new): &(String, String),
) -> Vec<Vec<Span>> {
    diff_spans(rows, &h.highlight(path, old), &h.highlight(path, new))
}

/// Whether a diff with these `sides` and its longest line `longest` (bytes) may be coloured:
/// each side at most `PLAIN_ABOVE` (as a file over 2 MB isn't), no line too long to colour.
fn colourable((old, new): &(String, String), longest: usize) -> bool {
    old.len() as u64 <= PLAIN_ABOVE && new.len() as u64 <= PLAIN_ABOVE && longest <= COLOUR_LINE_MAX
}

/// What git's answer shows (blocking: the background executor's), its rows a `Doc` of `version`
/// (one line per row: a last `\n` so a blank last row is a line), coloured with `colour` when
/// given (none: the syntaxes not loaded yet) and the rows are `colourable`, unless they are the
/// rows `held` coloured already (left uncoloured: `accept_diff` keeps the held ones).
fn diff_state(
    diff: FileDiff,
    disk_text: Option<&str>,
    version: u64,
    colour: Option<(&Highlighter, &Path)>,
    held: Option<&ColouredRows>,
) -> DiffState {
    match diff_outcome(diff, disk_text) {
        Outcome::Same => DiffState::Same,
        Outcome::Binary => DiffState::Binary,
        Outcome::Rows(parsed) => {
            let mut doc = Doc::new(format!("{}\n", parsed.text), version);
            let both = sides(&parsed);
            let colourable = colourable(&both, doc.longest);
            // The rows held, coloured, again: not highlighted again (`accept_diff` keeps them).
            let unchanged = held.is_some_and(|h| *h.text == *doc.text && *h.rows == parsed.rows);
            if let Some((h, path)) = colour
                && colourable
                && !unchanged
            {
                doc.spans = Some(Arc::new(colour_rows(h, path, &parsed.rows, &both)));
                doc.spans_version = Some(version);
            }
            DiffState::Rows(DiffDoc {
                doc,
                gutter: Gutter::diff(&parsed.rows),
                rows: Arc::new(parsed.rows),
                colourable,
            })
        }
    }
}

/// The diff to show for a file whose old blob is `old_size` bytes (none: not in HEAD): over
/// 20 MB (`REFUSE_ABOVE`) refused without asking git for the diff (it would stream the whole
/// blob); else git's answer (`diff`), refused too when that is over 20 MB. None: no answer
/// from git (the rows shown stay). Blocking.
fn diff_answer(
    old_size: Option<u64>,
    diff: impl FnOnce() -> Option<FileDiff>,
    disk_text: Option<&str>,
    version: u64,
    colour: Option<(&Highlighter, &Path)>,
    held: Option<&ColouredRows>,
) -> Option<DiffState> {
    if old_size.is_some_and(|n| n > REFUSE_ABOVE) {
        return Some(DiffState::Refuse);
    }
    let diff = diff()?;
    if let FileDiff::Unified(u) = &diff
        && u.len() as u64 > REFUSE_ABOVE
    {
        return Some(DiffState::Refuse);
    }
    Some(diff_state(diff, disk_text, version, colour, held))
}

/// A file's size and time, to tell whether it changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Stamp {
    modified: Option<SystemTime>,
    len: u64,
}

/// What a look at the disk found.
enum Loaded {
    /// The same size and time as the text shown.
    Unchanged,
    Missing,
    Refuse(Stamp),
    Text {
        stamp: Stamp,
        doc: Doc,
        plain: bool,
    },
}

/// Read `path` (blocking: the background executor's) unless its size and time are still
/// `known`.
fn read_file(path: &Path, known: Option<Stamp>) -> Loaded {
    let Ok(meta) = std::fs::metadata(path) else {
        return Loaded::Missing;
    };
    let stamp = Stamp {
        modified: meta.modified().ok(),
        len: meta.len(),
    };
    if known == Some(stamp) {
        return Loaded::Unchanged;
    }
    // Only a plain file: a FIFO or a device would block the read (or never end).
    if !meta.is_file() || classify(meta.len(), &[]) == Viewable::Refuse {
        return Loaded::Refuse(stamp);
    }
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Loaded::Missing,
        Err(_) => return Loaded::Refuse(stamp),
    };
    // One byte past the limit at most: a file that grew past it since the look is refused.
    let mut bytes = Vec::new();
    if file.take(REFUSE_ABOVE + 1).read_to_end(&mut bytes).is_err() {
        return Loaded::Refuse(stamp);
    }
    let head = &bytes[..bytes.len().min(HEAD)];
    let viewable = classify(bytes.len() as u64, head);
    if viewable == Viewable::Refuse {
        return Loaded::Refuse(stamp);
    }
    let text = String::from_utf8(bytes)
        .unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned());
    let doc = Doc::new(text, 0);
    let plain = viewable == Viewable::PlainTooLarge || doc.longest > COLOUR_LINE_MAX;
    Loaded::Text { stamp, doc, plain }
}

/// The shared highlighter (a GPUI global): its syntaxes loaded once, in the background, by the
/// first tab that needs them; recoloured (`with_theme`) when the theme changes.
#[derive(Default)]
pub struct Highlighters {
    ready: Option<(Arc<Highlighter>, Theme)>,
    loading: bool,
    /// Tabs waiting for the syntaxes, coloured once they are in.
    waiting: Vec<WeakEntity<FileTab>>,
}

impl Global for Highlighters {}

impl Highlighters {
    /// The highlighter in `theme`'s colours, once the syntaxes are loaded.
    fn get(&mut self, theme: &Theme) -> Option<Arc<Highlighter>> {
        let (h, t) = self.ready.as_mut()?;
        if t != theme {
            *h = Arc::new(h.with_theme(theme));
            *t = *theme;
        }
        Some(h.clone())
    }
}

/// The screen rows of lines `widths` cells wide (M11x part 2): each as (line, the cell it starts
/// at). With `wrap`, a line takes a row per `columns` cells (at least one row, an empty line
/// too); without it, one row each.
pub fn visual_rows(widths: &[usize], columns: usize, wrap: bool) -> Vec<(usize, usize)> {
    let columns = columns.max(1);
    let mut rows = Vec::with_capacity(widths.len());
    for (line, &width) in widths.iter().enumerate() {
        let count = if wrap {
            width.div_ceil(columns).max(1)
        } else {
            1
        };
        rows.extend((0..count).map(|k| (line, k * columns)));
    }
    rows
}

/// Each line's first row in `rows` (from `visual_rows`), then the row count: `lines + 1` long.
pub fn first_rows(rows: &[(usize, usize)], lines: usize) -> Vec<usize> {
    let mut first = vec![rows.len(); lines + 1];
    for (r, &(line, cell)) in rows.iter().enumerate().rev() {
        if cell == 0 && line < lines {
            first[line] = r;
        }
    }
    first
}

/// A wrapped row of line `i` that starts at cell `cell0`, `columns` wide: its bytes
/// `from..to` and the cell `from` starts at (`start`: a character crossing the row's start, a
/// tab, begins the row whole). Painting and clicking both use it, so they agree.
pub fn wrapped_row(doc: &Doc, i: usize, cell0: usize, columns: usize) -> (usize, usize, usize) {
    let (from, start) = doc.byte_at_cell(i, cell0);
    let (to, _) = doc.byte_at_cell(i, cell0 + columns.max(1));
    (from, to.max(from), start)
}

/// The byte a click `x_cells` cells into a wrapped row lands on (`row`: `wrapped_row`'s span):
/// the nearest character edge, kept in that row (right of a middle row's end: its end).
pub fn row_hit(doc: &Doc, i: usize, row: (usize, usize, usize), last: bool, x_cells: f32) -> usize {
    let (from, to, start) = row;
    let cell = start + x_cells.max(0.0).round() as usize;
    let (b, _) = doc.byte_at_cell(i, cell);
    let end = if last { doc.line(i).len() } else { to };
    b.clamp(from, end.max(from))
}

/// The top row after Wrap is switched (`from` → `to`, each wrapped or not): the row of the same
/// line, so the line at the top stays at the top.
pub fn rescroll_row(top: usize, from: Option<&Wrapped>, to: Option<&Wrapped>) -> usize {
    let line = from.map_or(top, |w| w.row(top).0);
    to.map_or(line, |w| w.first_row(line))
}

/// `bar_at` in wrapped rows: a press `y` on any of a changed line's rows is its bar; within half
/// a row of the edge where lines were deleted, that ▸.
pub fn bar_at_rows(
    y: f32,
    line_height: f32,
    w: &Wrapped,
    changes: &Changes,
) -> Option<(usize, LineChange)> {
    if y < 0.0 || line_height <= 0.0 {
        return None;
    }
    let row = (y / line_height).floor() as usize;
    if row < w.rows.len() {
        let line = w.row(row).0;
        if let Some(&change) = changes.by_line.get(&line) {
            return Some((line + 1, change));
        }
    }
    let edge_row = (y / line_height).round() as usize;
    let edge = w.first.binary_search(&edge_row).ok()?;
    changes
        .deleted
        .contains(&edge)
        .then_some((edge, LineChange::DeletedAfter))
}

/// The lines in sight: from `scroll_y` down `height`, `line_height` each, of `count`.
fn visible_rows(
    scroll_y: Pixels,
    height: Pixels,
    line_height: Pixels,
    count: usize,
) -> Range<usize> {
    let lh = f32::from(line_height).max(1.0);
    let first = (f32::from(scroll_y) / lh).floor().max(0.0) as usize;
    let last = ((f32::from(scroll_y + height) / lh).ceil().max(0.0) as usize).min(count);
    first.min(last)..last
}

/// The gutter's number columns: the file's line numbers, or a diff's old and new numbers (each
/// as many digits as its largest number, at least 2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gutter {
    Plain {
        digits: usize,
    },
    Diff {
        old_digits: usize,
        new_digits: usize,
    },
}

/// A number column's width: its largest number's digits, at least 2.
fn digits(n: usize) -> usize {
    n.to_string().len().max(2)
}

impl Gutter {
    /// The gutter for `count` lines of a file.
    fn plain(count: usize) -> Gutter {
        Gutter::Plain {
            digits: digits(count),
        }
    }

    /// The gutter for a diff's `rows` (made once, with its `DiffDoc`).
    fn diff(rows: &[DiffRow]) -> Gutter {
        let (old, new) = rows.iter().fold((0, 0), |(o, n), r| {
            (o.max(r.old.unwrap_or(0)), n.max(r.new.unwrap_or(0)))
        });
        Gutter::Diff {
            old_digits: digits(old),
            new_digits: digits(new),
        }
    }
}

/// Where the gutter's parts sit, in points from the body's left edge: the old numbers' right
/// edge (a diff's), the (new) numbers' right edge, the change bar's left edge (a diff's sign
/// column), the text's left edge.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Columns {
    pub old_right: Option<f32>,
    pub new_right: f32,
    pub bar_left: f32,
    pub text_left: f32,
}

/// The gutter laid out for cells `cell` wide: `GUTTER_LEFT`, the numbers (old, `NUMBER_GAP`,
/// new, for a diff), `NUMBER_GAP`, the bar (`BAR_WIDTH`; a diff's sign, one cell), `TEXT_GAP`.
pub fn columns(cell: f32, gutter: Gutter) -> Columns {
    let (old_right, new_right, mark) = match gutter {
        Gutter::Plain { digits } => (None, GUTTER_LEFT + cell * digits as f32, BAR_WIDTH),
        Gutter::Diff {
            old_digits,
            new_digits,
        } => {
            let old = GUTTER_LEFT + cell * old_digits as f32;
            (Some(old), old + NUMBER_GAP + cell * new_digits as f32, cell)
        }
    };
    let bar_left = new_right + NUMBER_GAP;
    Columns {
        old_right,
        new_right,
        bar_left,
        text_left: bar_left + mark + TEXT_GAP,
    }
}

/// The largest text Preview draws (M11ac review fix): bigger, the source stays.
pub const PREVIEW_MAX: usize = 1024 * 1024;

/// How far past the Preview's edges its list lays rows out (smooth scrolling).
const PREVIEW_OVERDRAW: f32 = 400.0;

/// Whether Preview can draw `state` (M11ac review fix): a coloured text of at most
/// `PREVIEW_MAX` bytes.
fn preview_fits(state: &State) -> bool {
    matches!(state, State::Text(doc) if doc.text.len() <= PREVIEW_MAX)
}

/// Whether the toolbar shows Preview (M11ac): for a Markdown file, in File mode only.
pub fn shows_preview(path: &Path, mode: FileMode) -> bool {
    is_markdown(path) && mode == FileMode::Plain
}

/// Whether the toolbar shows the File | Diff toggle: for a file in a repo, and always in Diff
/// mode (so it can go back).
pub fn shows_toggle(in_repo: bool, mode: FileMode) -> bool {
    in_repo || mode == FileMode::Diff
}

/// The change under a press `y` points down the text (scroll included) in the bar column: a
/// changed line's (1-based line, change), else the deleted-lines ▸ on the edge nearest to it.
pub fn bar_at(y: f32, line_height: f32, changes: &Changes) -> Option<(usize, LineChange)> {
    if y < 0.0 || line_height <= 0.0 {
        return None;
    }
    let line = (y / line_height).floor() as usize;
    if let Some(&change) = changes.by_line.get(&line) {
        return Some((line + 1, change));
    }
    let edge = (y / line_height).round() as usize;
    changes
        .deleted
        .contains(&edge)
        .then_some((edge, LineChange::DeletedAfter))
}

/// Where the text body's parts sit, as last drawn (for the mouse, the wheel and find).
#[derive(Clone, Copy, Debug)]
pub struct Geom {
    pub bounds: Bounds<Pixels>,
    pub line_height: Pixels,
    pub cell_width: Pixels,
    /// From the body's left edge: a diff's old numbers' right edge, the (new) line numbers'
    /// right edge, the change bar's left edge (a diff's sign column), the text's left edge
    /// (before the sideways scroll).
    pub old_numbers_right: Option<Pixels>,
    pub numbers_right: Pixels,
    pub bar_left: Pixels,
    pub text_left: Pixels,
    /// The scroll drawn (kept within `max_scroll`).
    pub scroll: Point<Pixels>,
    pub max_scroll: Point<Pixels>,
}

impl Geom {
    fn of(
        bounds: Bounds<Pixels>,
        m: &CellMetrics,
        lines: usize,
        gutter: Gutter,
        widest: usize,
        scroll: Point<Pixels>,
    ) -> Geom {
        let c = columns(f32::from(m.cell_width), gutter);
        let text_left = px(c.text_left);
        let max_scroll = point(
            (text_left + m.cell_width * widest as f32 + px(RIGHT_PAD) - bounds.size.width)
                .max(px(0.0)),
            (m.line_height * lines as f32 - bounds.size.height).max(px(0.0)),
        );
        Geom {
            bounds,
            line_height: m.line_height,
            cell_width: m.cell_width,
            old_numbers_right: c.old_right.map(px),
            numbers_right: px(c.new_right),
            bar_left: px(c.bar_left),
            text_left,
            scroll: Geom::clamp(scroll, max_scroll),
            max_scroll,
        }
    }

    fn clamp(p: Point<Pixels>, max: Point<Pixels>) -> Point<Pixels> {
        point(p.x.clamp(px(0.0), max.x), p.y.clamp(px(0.0), max.y))
    }
}

/// The colours of the change bars (and tints): the theme's green, blue and red, pushed to 3:1 on
/// the background as the tree's marks are.
#[derive(Clone, Copy)]
struct BarColours {
    added: Rgb,
    changed: Rgb,
    deleted: Rgb,
}

impl BarColours {
    fn of(theme: &Theme) -> BarColours {
        let fit = |c: Rgb| ensure_contrast(c, theme.background, MARK_MIN);
        BarColours {
            added: fit(theme.palette[2]),
            changed: fit(theme.palette[4]),
            deleted: fit(theme.palette[1]),
        }
    }

    fn of_change(&self, change: LineChange) -> Rgb {
        match change {
            LineChange::Added => self.added,
            LineChange::Changed => self.changed,
            LineChange::DeletedAfter => self.deleted,
        }
    }
}

/// A diff's signs' inks (removed, added): the change bars' red and green, each pushed to 3:1
/// on its own row's tint as painted (the colour at `TINT` over `background`).
fn sign_inks(bars: &BarColours, background: Rgb) -> (Rgb, Rgb) {
    let on_tint = |c: Rgb| ensure_contrast(c, mix(background, c, TINT), MARK_MIN);
    (on_tint(bars.deleted), on_tint(bars.added))
}

/// The colour runs (shown bytes, colour) of bytes `range` of a line as shown, covering it: the
/// line's spans (source bytes, sorted; the line's ending and anything outside `range` dropped,
/// tabs widened), the foreground between them.
fn line_runs(
    line: &str,
    spans: Option<&[Span]>,
    fg: Rgb,
    range: Range<usize>,
) -> Vec<(Range<usize>, Rgb)> {
    let slice = &line[range.clone()];
    let tabs = Tabs::of(slice);
    let shown_len = tabs.shown(slice.len());
    let spans = spans.unwrap_or(&[]);
    // Only the spans over `range` (a long line's part in sight).
    let first = spans.partition_point(|(r, _)| r.end <= range.start);
    let local = |b: usize| clamp_col(line, b).clamp(range.start, range.end) - range.start;
    let mut out = Vec::new();
    let mut at = 0;
    for (r, c) in spans[first..]
        .iter()
        .take_while(|(r, _)| r.start < range.end)
    {
        let (s, e) = (tabs.shown(local(r.start)).max(at), tabs.shown(local(r.end)));
        if s >= e {
            continue;
        }
        if s > at {
            out.push((at..s, fg));
        }
        out.push((s..e, *c));
        at = e;
    }
    if at < shown_len {
        out.push((at..shown_len, fg));
    }
    out
}

/// A line laid out to draw: its bytes `from..to` (all of a short line; a long line's part in
/// sight, found from its checkpoints) shaped, starting `x0` from the text's left edge.
struct Laid {
    shaped: Option<gpui::ShapedLine>,
    /// Each glyph's shown byte and x in the shaped part, in order (for `x`).
    xs: Vec<(usize, Pixels)>,
    width: Pixels,
    from: usize,
    to: usize,
    tabs: Tabs,
    x0: Pixels,
}

impl Laid {
    #[allow(clippy::too_many_arguments)]
    fn new(
        doc: &Doc,
        i: usize,
        spans: Option<&[Span]>,
        fg: Rgb,
        m: &CellMetrics,
        scroll_x: Pixels,
        width: Pixels,
        window: &mut Window,
    ) -> Laid {
        let line = doc.line(i);
        let (from, to, cell) = if doc.is_long(i) {
            let cw = f32::from(m.cell_width).max(1.0);
            let first = ((f32::from(scroll_x) / cw).floor() as usize).saturating_sub(8);
            let count = (f32::from(width) / cw).ceil() as usize + 16;
            let (from, cell) = doc.byte_at_cell(i, first);
            let (to, _) = doc.byte_at_cell(i, first + count);
            (from, to.max(from), cell)
        } else {
            (0, line.len(), 0)
        };
        Laid::part(doc, i, from, to, cell, spans, fg, m, window)
    }

    /// Bytes `from..to` of line `i` laid out (`cell`: where `from` starts), as `new` does: a
    /// wrapped row's part (M11x part 2).
    #[allow(clippy::too_many_arguments)]
    fn part(
        doc: &Doc,
        i: usize,
        from: usize,
        to: usize,
        cell: usize,
        spans: Option<&[Span]>,
        fg: Rgb,
        m: &CellMetrics,
        window: &mut Window,
    ) -> Laid {
        let line = doc.line(i);
        let slice = &line[from..to];
        let shown = expand_tabs(slice);
        let runs = line_runs(line, spans, fg, from..to);
        let text_runs: Vec<TextRun> = runs
            .iter()
            .map(|(r, c)| plain_run(r.len(), m, *c))
            .collect();
        let shaped = (!shown.is_empty()).then(|| {
            window.text_system().shape_line(
                SharedString::from(shown),
                m.font_size,
                &text_runs,
                None,
            )
        });
        let xs = shaped.as_ref().map_or_else(Vec::new, |s| {
            s.runs
                .iter()
                .flat_map(|run| &run.glyphs)
                .map(|g| (g.index, g.position.x))
                .collect()
        });
        Laid {
            width: shaped.as_ref().map_or(px(0.0), |s| s.width()),
            shaped,
            xs,
            from,
            to,
            tabs: Tabs::of(slice),
            x0: m.cell_width * cell as f32,
        }
    }

    /// The x of source byte `col` from the text's left edge (before the shaped part: its start;
    /// after it: its end, both out of sight on a long line).
    fn x(&self, col: usize) -> Pixels {
        if col <= self.from {
            return self.x0;
        }
        if col >= self.to {
            return self.x0 + self.width;
        }
        let b = self.tabs.shown(col - self.from);
        let k = self.xs.partition_point(|&(index, _)| index < b);
        self.x0 + self.xs.get(k).map_or(self.width, |&(_, x)| x)
    }
}

/// A one-colour run of `len` bytes in `m`'s font.
fn plain_run(len: usize, m: &CellMetrics, color: Rgb) -> TextRun {
    TextRun {
        len,
        font: m.font.clone(),
        color: hsla(color),
        background_color: None,
        underline: None,
        strikethrough: None,
    }
}

/// A text body wrapped at its width (M11x part 2): its screen rows and each line's first row,
/// for the doc, mode and column count they were made for.
#[derive(Clone)]
pub struct Wrapped {
    key: (FileMode, u64, usize),
    rows: Arc<Vec<(usize, usize)>>,
    first: Arc<Vec<usize>>,
    columns: usize,
}

impl Wrapped {
    /// `doc` wrapped at `columns` (each line's width from its cells).
    fn of(doc: &Doc, mode: FileMode, columns: usize) -> Wrapped {
        let count = doc.lines.len();
        let rows = visual_rows(&doc.widths, columns, true);
        let first = first_rows(&rows, count);
        Wrapped {
            key: (mode, doc.version, columns),
            rows: Arc::new(rows),
            first: Arc::new(first),
            columns: columns.max(1),
        }
    }

    /// Screen row `r` as (line, its first cell).
    fn row(&self, r: usize) -> (usize, usize) {
        self.rows.get(r).copied().unwrap_or((r, 0))
    }

    /// Line `i`'s first row (past the last line: the row count).
    fn first_row(&self, i: usize) -> usize {
        self.first
            .get(i)
            .copied()
            .unwrap_or_else(|| self.rows.len())
    }
}

/// Everything one frame of the text body draws, read in `render`.
struct Frame {
    doc: Doc,
    /// A diff's rows (Diff mode): tints, signs and two number columns, unchanged rows dimmed.
    diff: Option<Arc<Vec<DiffRow>>>,
    /// A diff's gutter (none: a file's, from its line count).
    gutter: Option<Gutter>,
    mode: FileMode,
    coloured: bool,
    scroll: Point<Pixels>,
    selection: Option<(Pos, Pos)>,
    matches: Arc<Vec<Match>>,
    current: Option<usize>,
    theme: Theme,
}

/// A number in the gutter, right-aligned at `right` (a point at its line's top).
fn paint_number(
    n: usize,
    right: Point<Pixels>,
    m: &CellMetrics,
    ink: Rgb,
    window: &mut Window,
    cx: &mut App,
) {
    let label = n.to_string();
    let run = plain_run(label.len(), m, ink);
    let number =
        window
            .text_system()
            .shape_line(SharedString::from(label), m.font_size, &[run], None);
    let at = point(right.x - number.width(), right.y);
    let _ = number.paint(at, m.line_height, TextAlign::Left, None, window, cx);
}

/// Paint the text body: `f`'s lines in sight, laid out as `g`. A file: its numbers, change bars
/// and tints, the deleted-lines ▸. A diff: each row's old and new numbers, its sign and tint
/// (removed red, added green), unchanged rows' colours dimmed (`diff_view::dim`).
fn paint_body(
    f: &Frame,
    g: &Geom,
    m: &CellMetrics,
    wrapped: Option<&Wrapped>,
    window: &mut Window,
    cx: &mut App,
) {
    let ui = f.theme.ui();
    let bars = BarColours::of(&f.theme);
    let bounds = g.bounds;
    let lh = g.line_height;
    // Wrapped (M11x part 2): screen rows, each a part of a line; else a row a line.
    let row_count = wrapped.map_or(f.doc.lines.len(), |w| w.rows.len());
    let rows = visible_rows(g.scroll.y, bounds.size.height, lh, row_count);
    let mask = ContentMask {
        bounds: Bounds::from_corners(
            point(
                bounds.left() + g.text_left - px(TEXT_GAP - 1.0),
                bounds.top(),
            ),
            bounds.bottom_right(),
        ),
    };
    let (seen_left, seen_right) = (mask.bounds.left(), mask.bounds.right());
    let spans = f.doc.spans.as_deref().filter(|_| f.coloured);
    let accent = hsla(ui.accent);
    let bg = f.theme.background;
    let sign = |s: &'static str, c: Rgb, window: &mut Window| {
        window.text_system().shape_line(
            SharedString::from(s),
            m.font_size,
            &[plain_run(s.len(), m, c)],
            None,
        )
    };
    let (removed_ink, added_ink) = sign_inks(&bars, bg);
    let (removed_sign, added_sign) = (
        sign(SIGN_REMOVED, removed_ink, window),
        sign(SIGN_ADDED, added_ink, window),
    );
    for r in rows.clone() {
        let (i, cell0) = wrapped.map_or((r, 0), |w| w.row(r));
        let y = bounds.top() + lh * r as f32 - g.scroll.y;
        let line = f.doc.line(i);
        // A wrapped row: its line's part, drawn from the text's left edge; the line's first row
        // has its number, bar and sign, its last row the selection's line end.
        let first_row = cell0 == 0;
        let part = wrapped.map(|w| {
            let (from, to, start) = wrapped_row(&f.doc, i, cell0, w.columns);
            let last = w.row(r + 1).0 != i || r + 1 >= w.rows.len();
            (from, to, start, last)
        });
        let text_x = match part {
            Some((_, _, start, _)) => bounds.left() + g.text_left - g.cell_width * start as f32,
            None => bounds.left() + g.text_left - g.scroll.x,
        };
        let row = f.diff.as_ref().and_then(|d| d.get(i)).copied();
        let change = match f.mode {
            FileMode::Plain => f.doc.changes.by_line.get(&i).copied(),
            FileMode::Diff => None,
        };
        let tint = match (row.map(|r| r.kind), change) {
            (Some(RowKind::Removed), _) => Some(bars.deleted),
            (Some(RowKind::Added), _) => Some(bars.added),
            (None, Some(change)) => Some(bars.of_change(change)),
            _ => None,
        };
        if let Some(tint) = tint {
            let band = Bounds::new(point(bounds.left(), y), size(bounds.size.width, lh));
            window.paint_quad(fill(band, hsla(tint).opacity(TINT)));
        }
        let line_spans = spans.and_then(|s| s.get(i)).map(Vec::as_slice);
        // An unchanged diff row: its colours mixed toward the background, still readable.
        let same = row.is_some_and(|r| r.kind == RowKind::Same);
        let dimmed: Option<Vec<Span>> = same.then(|| {
            line_spans
                .unwrap_or(&[])
                .iter()
                .map(|(r, c)| (r.clone(), dim(*c, bg)))
                .collect()
        });
        let (line_spans, fg) = match &dimmed {
            Some(d) => (Some(d.as_slice()), dim(f.theme.foreground, bg)),
            None => (line_spans, f.theme.foreground),
        };
        let laid = match part {
            Some((from, to, start, _)) => {
                Laid::part(&f.doc, i, from, to, start, line_spans, fg, m, window)
            }
            None => Laid::new(
                &f.doc,
                i,
                line_spans,
                fg,
                m,
                g.scroll.x,
                bounds.size.width,
                window,
            ),
        };
        let x_of = |col: usize| text_x + laid.x(col);
        window.with_content_mask(Some(mask), |window| {
            if let Some((s, e)) = f.selection
                && (s.line..=e.line).contains(&i)
                && s != e
            {
                let from = if i == s.line { s.col } else { 0 };
                let to = if i == e.line { e.col } else { line.len() };
                // A line's end inside the selection shows as one more cell (wrapped: on its
                // last row only).
                let ends_here = part.is_none_or(|(_, _, _, last)| last);
                let end = if i == e.line || !ends_here {
                    px(0.0)
                } else {
                    g.cell_width
                };
                let (x0, x1) = (x_of(from), x_of(to) + end);
                if x1 > x0 {
                    let b = Bounds::from_corners(point(x0, y), point(x1, y + lh));
                    window.paint_quad(fill(b, hsla(ui.selection)));
                }
            }
            // This line's matches over the part laid out, and of those only the ones in sight.
            let on_line = f.matches.partition_point(|m| m.line < i);
            let start = on_line
                + f.matches[on_line..].partition_point(|m| m.line == i && m.cols.end <= laid.from);
            for (k, hit) in f.matches[start..]
                .iter()
                .take_while(|m| m.line == i && m.cols.start < laid.to)
                .enumerate()
            {
                let (x0, x1) = (x_of(hit.cols.start), x_of(hit.cols.end));
                if x1 < seen_left || x0 > seen_right {
                    continue;
                }
                let tint = if f.current == Some(start + k) {
                    CURRENT_TINT
                } else {
                    MATCH_TINT
                };
                let b = Bounds::from_corners(point(x0, y), point(x1, y + lh));
                window.paint_quad(fill(b, accent.opacity(tint)));
            }
            if let Some(shaped) = &laid.shaped {
                let origin = point(text_x + laid.x0, y);
                let _ = shaped.paint(origin, lh, TextAlign::Left, None, window, cx);
            }
        });
        if !first_row {
            continue;
        }
        match row {
            None => {
                let right = point(bounds.left() + g.numbers_right, y);
                paint_number(i + 1, right, m, ui.text_faint, window, cx);
                if let Some(change) = change {
                    let bar = Bounds::new(
                        point(bounds.left() + g.bar_left, y),
                        size(px(BAR_WIDTH), lh),
                    );
                    window.paint_quad(fill(bar, hsla(bars.of_change(change))));
                }
            }
            Some(row) => {
                if let (Some(n), Some(right)) = (row.old, g.old_numbers_right) {
                    let right = point(bounds.left() + right, y);
                    paint_number(n, right, m, ui.text_faint, window, cx);
                }
                if let Some(n) = row.new {
                    let right = point(bounds.left() + g.numbers_right, y);
                    paint_number(n, right, m, ui.text_faint, window, cx);
                }
                let sign = match row.kind {
                    RowKind::Removed => Some(&removed_sign),
                    RowKind::Added => Some(&added_sign),
                    RowKind::Same | RowKind::Gap => None,
                };
                if let Some(sign) = sign {
                    // Centred in its one-cell column.
                    let x = bounds.left() + g.bar_left + (g.cell_width - sign.width()) / 2.0;
                    let _ = sign.paint(point(x, y), lh, TextAlign::Left, None, window, cx);
                }
            }
        }
    }
    if f.mode == FileMode::Diff {
        return;
    }
    // Deleted lines: a red ▸ on the edge between two lines, over the bar's place (at the top
    // edge, just under it, so it isn't cut in half).
    let glyph = "\u{25b8}";
    let run = plain_run(glyph.len(), m, bars.deleted);
    let marker = window.text_system().shape_line(
        SharedString::from(glyph),
        m.font_size * MARKER_SCALE,
        &[run],
        None,
    );
    for &edge in &f.doc.changes.deleted {
        let edge = wrapped.map_or(edge, |w| w.first_row(edge));
        if edge < rows.start || edge > rows.end {
            continue;
        }
        let y = bounds.top() + lh * edge as f32 - g.scroll.y;
        let top = if edge == 0 { y } else { y - lh / 2.0 };
        let x = bounds.left() + g.bar_left + px(BAR_WIDTH / 2.0) - marker.width() / 2.0;
        let _ = marker.paint(point(x, top), lh, TextAlign::Left, None, window, cx);
    }
}

/// What the tab asks of its workspace.
pub enum FileTabEvent {
    /// The Close button of a file that no longer exists.
    Close,
    /// The tab now shows its file in this mode (the toolbar's toggle, a change bar's click):
    /// the workspace saves it.
    Mode(FileMode),
    /// The toolbar's Preview turned on or off (M11ac): the workspace saves it.
    Preview(bool),
    /// A link in the Preview to a file (M11ac): the workspace opens it in a tab.
    OpenFile(PathBuf),
}

/// The find bar: its box and what it found.
struct FindBar {
    input: Entity<NameInput>,
    finds: Finds,
    _subscriptions: Vec<Subscription>,
}

/// The note a tab's body shows instead of text: a missing or refused file's; in Diff mode,
/// without rows, the diff's (a deleted file's rows still show: git has its old lines).
fn note_of(state: &State, mode: FileMode, diff: &DiffState) -> Option<&'static str> {
    match (state, mode, diff) {
        // Refused on disk for being binary (`refused_diff`).
        (State::Refuse, FileMode::Diff, DiffState::Binary) => Some(BINARY),
        (State::Refuse, ..) => Some(REFUSED),
        (_, FileMode::Diff, DiffState::Rows(_)) => None,
        (State::Missing, ..) => Some(MISSING),
        (_, FileMode::Plain, _) => None,
        (_, FileMode::Diff, DiffState::Refuse) => Some(REFUSED),
        (_, FileMode::Diff, DiffState::Same) => Some(NO_CHANGES),
        (_, FileMode::Diff, DiffState::Binary) => Some(BINARY),
        (_, FileMode::Diff, DiffState::NoRepo) => Some(NO_REPO),
        (_, FileMode::Diff, DiffState::Waiting) => None,
    }
}

/// The diff of a file refused on disk (`len` bytes when read): git isn't asked (it would read
/// the whole old blob). At most 20 MB (`REFUSE_ABOVE`) it was refused for being binary: "This
/// file is binary: no diff to show." Over: M11g's "too large or binary".
fn refused_diff(len: Option<u64>) -> DiffState {
    match len {
        Some(len) if len <= REFUSE_ABOVE => DiffState::Binary,
        _ => DiffState::Refuse,
    }
}

/// Where a tab's file sits for git: the git to run, its repo's top folder, and the file's path
/// in it. Looked up off the UI thread, and kept once found (`lookup_place`).
#[derive(Clone, Debug, PartialEq, Eq)]
struct GitPlace {
    git: PathBuf,
    root: PathBuf,
    rel: PathBuf,
}

/// `path`'s place for git (blocking): `Some(None)` outside a repo (git says so), without git,
/// or with no folder of it left; None when git gave no answer (it failed or took longer than
/// the cap: asked again later). A deleted file's is found from its nearest folder still there
/// (its own, or an ancestor after `rm -rf dir` or `git mv dir new`): git still has its old
/// lines, and Diff mode shows them.
fn git_place(path: &Path) -> Option<Option<GitPlace>> {
    let Some(git) = find_git() else {
        return Some(None);
    };
    let Some((real, dir)) = real_path(path) else {
        return Some(None);
    };
    let Some(root) = repo_root_at(&git, &dir)? else {
        return Some(None);
    };
    let root = std::fs::canonicalize(&root).unwrap_or(root);
    let rel = real.strip_prefix(&root).ok().map(Path::to_path_buf);
    Some(rel.map(|rel| GitPlace { git, root, rel }))
}

/// `path` as git prints it (real: `/private/var`, not `/var`) and the folder to ask git from:
/// its own folder, or for a file that's gone, its nearest folder still there (canonical, the
/// missing parts joined back on). None when no folder of it is left.
fn real_path(path: &Path) -> Option<(PathBuf, PathBuf)> {
    if let Ok(real) = std::fs::canonicalize(path) {
        let dir = real.parent()?.to_path_buf();
        return Some((real, dir));
    }
    path.ancestors().skip(1).find_map(|up| {
        let dir = std::fs::canonicalize(up).ok().filter(|d| d.is_dir())?;
        let rest = path.strip_prefix(up).ok()?;
        Some((dir.join(rest), dir))
    })
}

/// The tab's place for git: the one `known`, else looked up (`look`: `git_place`). Only a
/// place found is kept, so a look-up with no answer (git slow or failing) or no repo is tried
/// again next time.
fn lookup_place(
    known: Option<GitPlace>,
    look: impl FnOnce() -> Option<Option<GitPlace>>,
) -> Option<Option<GitPlace>> {
    match known {
        Some(place) => Some(Some(place)),
        None => look(),
    }
}

/// `/usr/bin/open`'s arguments for the file at `path` in `state`: shown (or being read), `-t`
/// (the default text editor, so a script or a program is never run); refused (binary or huge,
/// maybe a program), `-R` (shown in the Finder, never launched); missing, none.
fn open_argv(state: &State, path: &Path) -> Option<Vec<OsString>> {
    let flag = match state {
        State::Loading | State::Text(_) | State::PlainTooLarge(_) => "-t",
        State::Refuse => "-R",
        State::Missing => return None,
    };
    Some(vec![OsString::from(flag), path.as_os_str().to_owned()])
}

/// The button for `open_argv`: Open in Editor, or for a refused file Show in Finder.
fn open_label(state: &State) -> Option<&'static str> {
    match state {
        State::Loading | State::Text(_) | State::PlainTooLarge(_) => Some(OPEN_IN_EDITOR),
        State::Refuse => Some(SHOW_IN_FINDER),
        State::Missing => None,
    }
}

/// A file tab's view.
pub struct FileTab {
    pub path: PathBuf,
    /// When it was opened (an age for a workspace with no terminal, as a browser tab's).
    pub since: Instant,
    focus: FocusHandle,
    state: State,
    /// The file as it is, or its diff against the last commit (M11h).
    mode: FileMode,
    /// The diff, once asked (Diff mode asks for it instead of the change bars).
    diff: HeldDiff,
    /// Where Diff mode scrolls to when its rows are next drawn: their first change, or a
    /// change bar's change.
    reveal: Option<Reveal>,
    /// The size and time of the file as last read.
    stamp: Option<Stamp>,
    /// Reads so far (each one's `version`).
    versions: u64,
    /// A read in flight.
    reading: bool,
    /// The highlight, the git look-up and the search in flight (one of each at a time).
    colouring: OneAtATime,
    colouring_diff: OneAtATime,
    looking: OneAtATime,
    searching: OneAtATime,
    /// Searches asked so far (each one's generation).
    searches: u64,
    /// The file's place for git, once found (none: not yet, or the last look-up failed).
    git_place: Option<GitPlace>,
    /// When the last git look-up started, and whether one waits for its turn (`lookup_wait`).
    looked_at: Option<Instant>,
    lookup_waiting: bool,
    /// The mode and version last drawn: drawing a new one stores the scroll as it was kept in
    /// the text.
    drawn: Rc<Cell<(FileMode, u64)>>,
    /// The find bar closed by itself (the file went): the tab takes the keyboard back.
    refocus: bool,
    /// The theme the colours were asked in (another one: colour again).
    coloured_with: Option<Theme>,
    /// Each mode's scroll: the file's, the diff's.
    scroll: Point<Pixels>,
    diff_scroll: Point<Pixels>,
    geom: Rc<Cell<Option<Geom>>>,
    /// The text body wrapped at its width, as last drawn with Wrap on (M11x part 2); none
    /// with it off.
    wrapped: Rc<RefCell<Option<Wrapped>>>,
    /// The selection: where the mouse went down, and where it is.
    selection: Option<(Pos, Pos)>,
    selecting: bool,
    find: Option<FindBar>,
    /// On screen (the workspace sets it each frame), polling, polls so far.
    shown: bool,
    ticking: bool,
    polls: u32,
    /// The UI screenshots: no polling, no git.
    pub frozen: bool,
    /// The Close button's bounds (a missing file's), as last drawn.
    close_at: Rc<Cell<Option<Bounds<Pixels>>>>,
    /// The toggle's File and Diff segments, as last drawn.
    toggle_at: Rc<Cell<[Option<Bounds<Pixels>>; 2]>>,
    /// M11ac: a Markdown file shown rendered (`md_preview`; only in File mode), the reader's
    /// scroll (kept over a reload), the blocks parsed for the version shown, and the Preview
    /// button as last drawn.
    preview: bool,
    preview_list: ListState,
    parsed: Option<(u64, Rc<Vec<Md>>)>,
    preview_at: Rc<Cell<Option<Bounds<Pixels>>>>,
    /// M11ac Part 2: what each block of the parsed version does as a Mermaid diagram, for the
    /// version and light or dark it was planned for (`mermaid::plan`), and how they were shown
    /// last frame.
    mermaid_plan: Option<(u64, bool, MermaidPlan)>,
    mermaid_shown: Rc<Vec<Option<md_preview::Shown>>>,
    _appearance: Subscription,
}

/// What each block does as a Mermaid diagram (M11ac Part 2, `mermaid::plan`).
type MermaidPlan = Rc<Vec<Option<mermaid::Plan>>>;

/// Where a diff scrolls to when its rows are drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Reveal {
    /// Its first change (a diff opened, or its first rows).
    First,
    /// A change bar's change (`diff_view::reveal_row`; not found: the first change).
    Change((usize, LineChange)),
}

impl EventEmitter<FileTabEvent> for FileTab {}

impl Focusable for FileTab {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl FileTab {
    /// A tab showing `path`: read at once, off the UI thread.
    pub fn new(
        path: PathBuf,
        mode: FileMode,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        // A theme change colours the text again (bars and tints follow the theme each frame).
        // Drawn again at once for any change (the view is cached: it repaints when it says so).
        let appearance = cx.observe_global::<Appearance>(|this, cx| {
            let theme = cx.global::<Appearance>().theme;
            if this.coloured_with.is_some_and(|t| t != theme) {
                this.colour(cx);
            }
            // The Preview's rows may change height (another font or size).
            this.preview_list.remeasure();
            cx.notify();
        });
        let mut tab = Self {
            path,
            since: Instant::now(),
            focus: cx.focus_handle(),
            state: State::Loading,
            mode,
            diff: HeldDiff::default(),
            reveal: None,
            stamp: None,
            versions: 0,
            reading: false,
            colouring: OneAtATime::default(),
            colouring_diff: OneAtATime::default(),
            looking: OneAtATime::default(),
            searching: OneAtATime::default(),
            searches: 0,
            git_place: None,
            looked_at: None,
            lookup_waiting: false,
            drawn: Rc::default(),
            refocus: false,
            coloured_with: None,
            scroll: Point::default(),
            diff_scroll: Point::default(),
            geom: Rc::default(),
            wrapped: Rc::default(),
            selection: None,
            selecting: false,
            find: None,
            shown: false,
            ticking: false,
            polls: 0,
            frozen: false,
            close_at: Rc::default(),
            toggle_at: Rc::default(),
            preview: false,
            preview_list: ListState::new(0, ListAlignment::Top, px(PREVIEW_OVERDRAW)),
            parsed: None,
            preview_at: Rc::default(),
            mermaid_plan: None,
            mermaid_shown: Rc::default(),
            _appearance: appearance,
        };
        tab.check_disk(cx);
        tab
    }

    /// The text shown, plain or coloured.
    pub fn doc(&self) -> Option<&Doc> {
        match &self.state {
            State::Text(doc) | State::PlainTooLarge(doc) => Some(doc),
            _ => None,
        }
    }

    fn doc_mut(&mut self) -> Option<&mut Doc> {
        match &mut self.state {
            State::Text(doc) | State::PlainTooLarge(doc) => Some(doc),
            _ => None,
        }
    }

    /// The text the body shows: the file's (Plain), or the diff's rows (Diff, once it has
    /// them). Find, the selection and the scroll work on it.
    fn shown_doc(&self) -> Option<&Doc> {
        match (self.mode, &self.diff.state) {
            (FileMode::Plain, _) => self.doc(),
            (FileMode::Diff, DiffState::Rows(d)) => Some(&d.doc),
            (FileMode::Diff, _) => None,
        }
    }

    /// The scroll of the mode shown.
    fn mode_scroll(&self) -> Point<Pixels> {
        match self.mode {
            FileMode::Plain => self.scroll,
            FileMode::Diff => self.diff_scroll,
        }
    }

    fn mode_scroll_mut(&mut self, mode: FileMode) -> &mut Point<Pixels> {
        match mode {
            FileMode::Plain => &mut self.scroll,
            FileMode::Diff => &mut self.diff_scroll,
        }
    }

    /// The selected text (empty: none): a diff's without its signs or numbers.
    pub fn selected_text(&self) -> String {
        match (self.shown_doc(), self.selection) {
            (Some(doc), Some((a, b))) => selection_text(&doc.text, &doc.lines, a, b),
            _ => String::new(),
        }
    }

    /// The workspace's word, each frame: whether the tab is on screen. On screen (and the window
    /// seen), the disk is checked every second; coming back on screen, at once.
    pub(crate) fn set_shown(&mut self, shown: bool, cx: &mut Context<Self>) {
        let was = std::mem::replace(&mut self.shown, shown);
        if shown && !was && !self.frozen {
            self.check_disk(cx);
        }
        self.tick(cx);
    }

    /// Whether the disk should be watched: on screen, the window seen, not frozen.
    fn wanted(&self, cx: &App) -> bool {
        let seen = cx.try_global::<WindowVisible>().is_none_or(|w| w.visible);
        self.shown && seen && !self.frozen
    }

    /// Start the 1 s poll unless it runs or isn't wanted; it stops once it isn't, so a hidden tab
    /// or window is never woken. `set_shown` (each frame) starts it again.
    fn tick(&mut self, cx: &mut Context<Self>) {
        if self.ticking || !self.wanted(cx) {
            return;
        }
        self.ticking = true;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(POLL).await;
                let wanted = this
                    .update(cx, |this, cx| {
                        this.ticking = this.wanted(cx);
                        if this.ticking {
                            this.polls = this.polls.wrapping_add(1);
                            this.check_disk(cx);
                            if this.polls.is_multiple_of(CHANGES_EVERY) {
                                this.look_up_changes(cx);
                            }
                        }
                        this.ticking
                    })
                    .unwrap_or(false);
                if !wanted {
                    break;
                }
            }
        })
        .detach();
    }

    /// Look at the disk (off the UI thread): a changed file is read again.
    pub fn check_disk(&mut self, cx: &mut Context<Self>) {
        if self.reading {
            return;
        }
        self.reading = true;
        let (path, known) = (self.path.clone(), self.stamp);
        cx.spawn(async move |this, cx| {
            let loaded = cx
                .background_executor()
                .spawn(async move { read_file(&path, known) })
                .await;
            this.update(cx, |this, cx| {
                this.reading = false;
                this.loaded(loaded, cx);
            })
            .ok();
        })
        .detach();
    }

    /// Show what the disk gave: a new version of the text (the scroll kept; the old colours and
    /// bars until the new ones come), or a note.
    fn loaded(&mut self, loaded: Loaded, cx: &mut Context<Self>) {
        match loaded {
            Loaded::Unchanged => return,
            // Each look at the disk that finds something new is a version (a diff asked for an
            // older one is dropped).
            Loaded::Missing => {
                if matches!(self.state, State::Missing) {
                    return;
                }
                self.versions += 1;
                let was_refused = matches!(self.state, State::Refuse);
                self.state = State::Missing;
                self.stamp = None;
                self.unrefuse_diff(was_refused);
                // A deleted file's diff: its old lines, all removed (the find bar stays on
                // them).
                self.look_up_changes(cx);
                self.drop_find_without_text();
            }
            Loaded::Refuse(stamp) => {
                self.versions += 1;
                self.state = State::Refuse;
                self.stamp = Some(stamp);
                self.look_up_changes(cx);
                self.drop_find_without_text();
            }
            Loaded::Text {
                stamp,
                mut doc,
                plain,
            } => {
                self.versions += 1;
                doc.version = self.versions;
                if let Some(old) = self.doc() {
                    if !plain {
                        doc.spans = old.spans.clone();
                        doc.spans_version = old.spans_version;
                    }
                    doc.changes = old.changes.clone();
                    doc.changes_version = old.changes_version;
                }
                self.stamp = Some(stamp);
                let was_refused = matches!(self.state, State::Refuse);
                self.state = if plain {
                    State::PlainTooLarge(doc)
                } else {
                    State::Text(doc)
                };
                self.unrefuse_diff(was_refused);
                self.colour(cx);
                self.look_up_changes(cx);
            }
        }
        self.keep_selection();
        self.find_again(cx);
        cx.notify();
    }

    /// The file isn't refused any more: its diff (refused without asking git) is to be asked.
    /// A binary one too, when it came from the refusal (`was_refused`), not from git.
    fn unrefuse_diff(&mut self, was_refused: bool) {
        let from_disk = was_refused && matches!(self.diff.state, DiffState::Binary);
        if from_disk || matches!(self.diff.state, DiffState::Refuse) {
            self.diff.state = DiffState::Waiting;
        }
    }

    /// The find bar closes when no text is left to find in (the file went or was refused, and
    /// no diff rows show); the tab takes the keyboard back.
    fn drop_find_without_text(&mut self) {
        if self.shown_doc().is_none() && matches!(self.state, State::Missing | State::Refuse) {
            self.refocus |= self.find.take().is_some();
        }
    }

    /// The selection within the new text (none without one).
    fn keep_selection(&mut self) {
        let last = self.shown_doc().map(|d| d.lines.len().saturating_sub(1));
        self.selection = match (last, self.selection) {
            (Some(last), Some((a, b))) => {
                let fit = |p: Pos| {
                    if p.line > last {
                        Pos {
                            line: last,
                            col: usize::MAX,
                        }
                    } else {
                        p
                    }
                };
                Some((fit(a), fit(b)))
            }
            _ => None,
        };
        if self.selection.is_none() {
            self.selecting = false;
        }
    }

    /// The shared highlighter in the current theme, once its syntaxes are loaded. Not yet: this
    /// tab waits for them (coloured once they are in, `colour`), and the first to ask loads them
    /// in the background.
    fn shared_highlighter(&mut self, cx: &mut Context<Self>) -> Option<Arc<Highlighter>> {
        let theme = cx.global::<Appearance>().theme;
        let me = cx.entity().downgrade();
        let already_loading = {
            let all = cx.default_global::<Highlighters>();
            if let Some(h) = all.get(&theme) {
                return Some(h);
            }
            if !all.waiting.iter().any(|w| w.entity_id() == me.entity_id()) {
                all.waiting.push(me);
            }
            std::mem::replace(&mut all.loading, true)
        };
        if !already_loading {
            cx.spawn(async move |_, cx| {
                let loaded = cx
                    .background_executor()
                    .spawn(async move { Highlighter::load(&theme) })
                    .await;
                cx.update(|cx| {
                    let all = cx.default_global::<Highlighters>();
                    all.loading = false;
                    all.ready = Some((Arc::new(loaded), theme));
                    for tab in std::mem::take(&mut all.waiting) {
                        tab.update(cx, |tab, cx| tab.colour(cx)).ok();
                    }
                });
            })
            .detach();
        }
        None
    }

    /// Colour the text and the diff's rows (off the UI thread), in the current theme.
    fn colour(&mut self, cx: &mut Context<Self>) {
        self.colour_text(cx);
        self.colour_diff(cx);
    }

    /// Colour the file's text (off the UI thread), in the current theme.
    fn colour_text(&mut self, cx: &mut Context<Self>) {
        let State::Text(doc) = &self.state else {
            return;
        };
        let (version, text) = (doc.version, doc.text.clone());
        let Some(h) = self.shared_highlighter(cx) else {
            return;
        };
        if !self.colouring.start() {
            // It runs again, in the theme and on the text of then, when the running one ends.
            return;
        }
        self.coloured_with = Some(cx.global::<Appearance>().theme);
        let path = self.path.clone();
        cx.spawn(async move |this, cx| {
            let spans = cx
                .background_executor()
                .spawn(async move { h.highlight(&path, &text) })
                .await;
            this.update(cx, |this, cx| {
                if let State::Text(doc) = &mut this.state
                    && doc.accept_spans(version, spans)
                {
                    cx.notify();
                }
                if this.colouring.finish() {
                    this.colour_text(cx);
                }
            })
            .ok();
        })
        .detach();
    }

    /// Colour the diff's rows held (off the UI thread), in the current theme: rows that came
    /// before the syntaxes were loaded, or a new theme. Only rows that may be coloured (each
    /// side at most 2 MB, no line too long: `DiffDoc::colourable`).
    fn colour_diff(&mut self, cx: &mut Context<Self>) {
        let DiffState::Rows(d) = &self.diff.state else {
            return;
        };
        if !d.colourable {
            return;
        }
        let (version, text, rows) = (d.doc.version, d.doc.text.clone(), d.rows.clone());
        let Some(h) = self.shared_highlighter(cx) else {
            return;
        };
        if !self.colouring_diff.start() {
            return;
        }
        self.coloured_with = Some(cx.global::<Appearance>().theme);
        let path = self.path.clone();
        cx.spawn(async move |this, cx| {
            let keep = rows.clone();
            let spans = cx
                .background_executor()
                .spawn(async move {
                    // The rows' text without the `Doc`'s last `\n`.
                    let text = text.strip_suffix('\n').unwrap_or(&text).to_string();
                    let parsed = Parsed {
                        text,
                        rows: keep.to_vec(),
                    };
                    colour_rows(&h, &path, &parsed.rows, &sides(&parsed))
                })
                .await;
            this.update(cx, |this, cx| {
                // Only onto the rows it was made for.
                if let DiffState::Rows(d) = &mut this.diff.state
                    && Arc::ptr_eq(&d.rows, &rows)
                    && d.doc.accept_spans(version, spans)
                {
                    cx.notify();
                }
                if this.colouring_diff.finish() {
                    this.colour_diff(cx);
                }
            })
            .ok();
        })
        .detach();
    }

    /// Ask git about the text shown (off the UI thread): in Plain mode its change bars, in
    /// Diff mode its diff (`look_up_diff`). Not when frozen. At most one look-up every 2 s
    /// (`lookup_wait`): one asked sooner waits for its turn. The git to run and the repo are
    /// kept once found (`lookup_place`). No answer from git (slow or failing) leaves the bars
    /// (or the rows) shown.
    fn look_up_changes(&mut self, cx: &mut Context<Self>) {
        if self.frozen {
            return;
        }
        match (self.mode, &self.state) {
            (FileMode::Plain, State::Text(_) | State::PlainTooLarge(_)) => {}
            // No text, no bars: only its place, once (for the toolbar's toggle).
            (FileMode::Plain, State::Missing | State::Refuse) if self.git_place.is_none() => {}
            (FileMode::Plain, _) | (FileMode::Diff, State::Loading) => return,
            // Over 20 MB or binary: git isn't asked (it would read the whole old blob).
            (FileMode::Diff, State::Refuse) => {
                let version = self.versions;
                let state = refused_diff(self.stamp.map(|s| s.len));
                self.accept_diff(version, state, cx);
                return;
            }
            (FileMode::Diff, _) => {}
        }
        if let Some(wait) = lookup_wait(self.looked_at, Instant::now()) {
            if !self.lookup_waiting {
                self.lookup_waiting = true;
                cx.spawn(async move |this, cx| {
                    cx.background_executor().timer(wait).await;
                    this.update(cx, |this, cx| {
                        this.lookup_waiting = false;
                        this.look_up_changes(cx);
                    })
                    .ok();
                })
                .detach();
            }
            return;
        }
        if !self.looking.start() {
            return;
        }
        self.looked_at = Some(Instant::now());
        if self.mode == FileMode::Diff {
            self.look_up_diff(cx);
            return;
        }
        // Without a text (missing or refused), only the place is looked up.
        let version = self.doc().map(|d| d.version);
        let (path, known) = (self.path.clone(), self.git_place.clone());
        cx.spawn(async move |this, cx| {
            let (place, changes) = cx
                .background_executor()
                .spawn(async move {
                    let place = lookup_place(known, || git_place(&path)).flatten();
                    let changes = place
                        .as_ref()
                        .filter(|_| version.is_some())
                        .and_then(|p| line_changes(&p.git, &p.root, &p.rel));
                    (place, changes)
                })
                .await;
            this.update(cx, |this, cx| {
                let found = place.is_some() && this.git_place.is_none();
                this.git_place = place;
                // No answer (git slow or failing, or no repo): the bars shown stay.
                if let Some(changes) = changes
                    && let Some(version) = version
                    && let Some(doc) = this.doc_mut()
                    && doc.accept_changes(version, &changes)
                {
                    cx.notify();
                }
                // A place found shows the toggle.
                if found {
                    cx.notify();
                }
                this.looked_up(cx);
            })
            .ok();
        })
        .detach();
    }

    /// A look-up ended: the next, if one was asked meanwhile.
    fn looked_up(&mut self, cx: &mut Context<Self>) {
        if self.looking.finish() {
            self.look_up_changes(cx);
        }
    }

    /// Diff mode's look-up (its turn taken): the file's whole diff against the last commit,
    /// `git diff HEAD -U<lines on disk + 1>`, made into rows, coloured when the syntaxes are in
    /// and the file is at most 2 MB; all off the UI thread. No repo: "Not a git repository."
    /// The answer carries the disk version it was asked for (`HeldDiff::accept`).
    fn look_up_diff(&mut self, cx: &mut Context<Self>) {
        let version = self.versions;
        // Over 2 MB on disk, the new side can't be coloured: the syntaxes aren't asked for.
        let (disk, lines, colour) = match &self.state {
            State::Text(doc) => (Some(doc.text.clone()), doc.lines.len(), true),
            State::PlainTooLarge(doc) => (Some(doc.text.clone()), doc.lines.len(), false),
            State::Missing => (None, 0, true),
            State::Loading | State::Refuse => {
                self.looked_up(cx);
                return;
            }
        };
        let h = if colour {
            self.shared_highlighter(cx)
        } else {
            None
        };
        // The rows held, coloured: the same parse again isn't coloured again.
        let held = match &self.diff.state {
            DiffState::Rows(d) if d.doc.spans.is_some() => Some(ColouredRows {
                text: d.doc.text.clone(),
                rows: d.rows.clone(),
            }),
            _ => None,
        };
        let (path, known) = (self.path.clone(), self.git_place.clone());
        cx.spawn(async move |this, cx| {
            let (place, state) = cx
                .background_executor()
                .spawn(async move {
                    let place = lookup_place(known, || git_place(&path));
                    let state = match &place {
                        // No answer (git slow or failing): nothing changes, asked again later.
                        None => None,
                        Some(None) => Some(DiffState::NoRepo),
                        // The old blob's size first: over 20 MB, git isn't asked for the diff.
                        Some(Some(p)) => head_size(&p.git, &p.root, &p.rel).and_then(|size| {
                            let colour = h.as_deref().map(|h| (h, path.as_path()));
                            diff_answer(
                                size,
                                || file_diff(&p.git, &p.root, &p.rel, context_for(lines)),
                                disk.as_deref(),
                                version,
                                colour,
                                held.as_ref(),
                            )
                        }),
                    };
                    (place.flatten(), state)
                })
                .await;
            this.update(cx, |this, cx| {
                this.git_place = place;
                // No answer (git slow or failing): the rows (or the wait) shown stay.
                if let Some(state) = state {
                    this.accept_diff(version, state, cx);
                }
                this.looked_up(cx);
            })
            .ok();
        })
        .detach();
    }

    /// A diff asked for disk version `version`: taken unless older than the one held. Its
    /// first rows scroll to their first change; the same rows again keep theirs (and their
    /// colours) as they are.
    fn accept_diff(&mut self, version: u64, state: DiffState, cx: &mut Context<Self>) {
        let unchanged = match (&self.diff.state, &state) {
            (DiffState::Rows(held), DiffState::Rows(new)) => {
                held.doc.text == new.doc.text
                    && held.rows == new.rows
                    && (held.doc.spans.is_some() || new.doc.spans.is_none())
            }
            _ => false,
        };
        let first_rows = !matches!(self.diff.state, DiffState::Rows(_));
        if unchanged {
            if fresher(version, self.versions, self.diff.version) {
                self.diff.version = Some(version);
            }
            return;
        }
        if !self.diff.accept(version, self.versions, state) {
            return;
        }
        match &self.diff.state {
            DiffState::Rows(d) => {
                if first_rows && self.reveal.is_none() {
                    self.reveal = Some(Reveal::First);
                }
                if d.doc.spans.is_none() {
                    self.colour_diff(cx);
                }
            }
            // No rows to show a change bar's change in: it is dropped (new rows open at their
            // first change).
            _ => self.reveal = None,
        }
        if self.mode == FileMode::Diff {
            self.keep_selection();
            self.drop_find_without_text();
            self.find_again(cx);
        }
        cx.notify();
    }

    /// The file's mode now (M11h).
    pub fn mode(&self) -> FileMode {
        self.mode
    }

    /// Whether the Markdown is shown rendered now (M11ac): Preview on, a Markdown file, File
    /// mode.
    pub fn preview_shown(&self) -> bool {
        self.preview && shows_preview(&self.path, self.mode) && preview_fits(&self.state)
    }

    /// Preview on or off by hand (M11ac): the find bar goes with the source, and the workspace
    /// saves it.
    fn choose_preview(&mut self, on: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.preview == on || (on && !preview_fits(&self.state)) {
            return;
        }
        self.preview = on;
        if on && self.find.is_some() {
            self.close_find(window, cx);
        }
        self.selection = None;
        cx.emit(FileTabEvent::Preview(on));
        cx.notify();
    }

    /// A link clicked in the Preview (M11ac, `md_preview::link_target`): a web or mail link
    /// opens in the default browser, a file opens in a tab (if it is one); nothing else.
    fn follow_link(&mut self, url: &str, cx: &mut Context<Self>) {
        let base = self.path.parent().unwrap_or(Path::new("/"));
        match link_target(base, url) {
            LinkTarget::Web(url) => cx.open_url(&url),
            LinkTarget::File(path) if path.is_file() => cx.emit(FileTabEvent::OpenFile(path)),
            LinkTarget::File(_) | LinkTarget::Nowhere => {}
        }
    }

    /// The Preview's body (M11ac): the file's Markdown drawn as a reader, a GPUI `list` of
    /// its blocks (only those in sight are drawn). The text is parsed again only for a new
    /// version; the list then keeps its place by block.
    fn preview_body(&mut self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        let Some(doc) = self.doc() else {
            return div().flex_1().into_any_element();
        };
        let version = doc.version;
        let blocks = match &self.parsed {
            Some((v, blocks)) if *v == version => blocks.clone(),
            _ => {
                let blocks = Rc::new(crate::agent_markdown::parse(&doc.text));
                let top = self.preview_list.logical_scroll_top();
                self.preview_list.reset(blocks.len());
                self.preview_list.scroll_to(top);
                self.parsed = Some((version, blocks.clone()));
                blocks
            }
        };
        let mermaid = self.mermaid_shown(version, &blocks, theme.is_dark(), cx);
        let view = cx.entity().downgrade();
        let draw = md_preview::Draw {
            colours: md_preview::Colours::of(&theme),
            code_font: cx.global::<Appearance>().font.family.clone(),
            on_link: Rc::new(move |url, _, cx| {
                view.update(cx, |this, cx| this.follow_link(&url, cx)).ok();
            }),
            links: Cell::new(0),
            mermaid,
        };
        div()
            .id("file-md-preview")
            .flex_1()
            .min_h(px(0.0))
            .w_full()
            .child(
                list(self.preview_list.clone(), move |ix, _, _| {
                    md_preview::row(&blocks, ix, &draw)
                })
                .size_full(),
            )
            .into_any_element()
    }

    /// How each Mermaid block of `blocks` (version `version`) is shown this frame (M11ac Part
    /// 2): planned once per version and light or dark; each diagram read from the cache, or
    /// queued to render (its code shown meanwhile; when it is ready the rows are measured again
    /// and drawn). A file with none costs nothing past the plan.
    fn mermaid_shown(
        &mut self,
        version: u64,
        blocks: &Rc<Vec<Md>>,
        dark: bool,
        cx: &mut Context<Self>,
    ) -> Rc<Vec<Option<md_preview::Shown>>> {
        use md_preview::Shown;
        use mermaid::Plan;
        let plan = match &self.mermaid_plan {
            Some((v, d, plan)) if *v == version && *d == dark => plan.clone(),
            _ => {
                let plan = Rc::new(mermaid::plan(blocks, dark));
                self.mermaid_plan = Some((version, dark, plan.clone()));
                plan
            }
        };
        if plan.iter().all(Option::is_none) {
            self.mermaid_shown = Rc::default();
            return self.mermaid_shown.clone();
        }
        let view = cx.entity().downgrade();
        let who = cx.entity_id().as_u64();
        let shown: Vec<Option<Shown>> = plan
            .iter()
            .zip(blocks.iter())
            .map(|(plan, md)| {
                Some(match plan.as_ref()? {
                    Plan::TooMany => Shown::Code(Some(mermaid::note(mermaid::TOO_MANY))),
                    Plan::TooLarge => Shown::Code(Some(mermaid::note(mermaid::TOO_LARGE))),
                    Plan::Diagram(key) => {
                        let Md::Code { text, .. } = md else {
                            return None;
                        };
                        let (tell_view, wanted_view) = (view.clone(), view.clone());
                        let waiter = Waiter {
                            tell: Box::new(move |cx: &mut App| {
                                tell_view
                                    .update(cx, |this, cx| {
                                        this.preview_list.remeasure();
                                        cx.notify();
                                    })
                                    .ok();
                            }),
                            // Still wanted: the tab is open, shows its Preview, and its plan
                            // (this file version, the theme's light or dark) has the diagram.
                            wanted: Box::new(move |cx: &App, key: &Key| {
                                wanted_view
                                    .upgrade()
                                    .is_some_and(|tab| tab.read(cx).wants_mermaid(key, cx))
                            }),
                        };
                        match Mermaids::shown(*key, text, who, waiter, cx) {
                            Asked::Ready(Drawn { image, w, h }) => Shown::Diagram { image, w, h },
                            Asked::Failed(e) => Shown::Code(Some(mermaid::note(&e))),
                            Asked::TooMuch => Shown::Code(Some(mermaid::note(mermaid::TOO_MUCH))),
                            Asked::Waiting => Shown::Code(None),
                        }
                    }
                })
            })
            .collect();
        self.mermaid_shown = Rc::new(shown);
        self.mermaid_shown.clone()
    }

    /// Whether this tab still wants diagram `key` drawn (M11ac Part 2, asked before it
    /// renders): Preview shows, the plan for the file as read has it, and the theme is still
    /// light or dark as the key says.
    fn wants_mermaid(&self, key: &Key, cx: &App) -> bool {
        let dark = cx.global::<Appearance>().theme.is_dark();
        key.dark == dark
            && self.preview_shown()
            && self.mermaid_plan.as_ref().is_some_and(|(_, d, plan)| {
                *d == dark && plan.contains(&Some(mermaid::Plan::Diagram(*key)))
            })
    }

    /// Show the file as it is, or its diff (`reveal`: a change bar's change, shown when the
    /// rows are). Each mode keeps its own scroll; the selection goes, find searches the text
    /// now shown. A mode with nothing yet for the file as read asks git at once.
    pub(crate) fn set_mode(
        &mut self,
        mode: FileMode,
        reveal: Option<(usize, LineChange)>,
        cx: &mut Context<Self>,
    ) {
        if mode == FileMode::Diff
            && let Some(change) = reveal
        {
            self.reveal = Some(Reveal::Change(change));
        }
        if std::mem::replace(&mut self.mode, mode) == mode {
            cx.notify();
            return;
        }
        self.selection = None;
        self.selecting = false;
        // Find's current match: the first from the top of the text now shown.
        if let Some(find) = self.find.as_mut() {
            find.finds.from_top = true;
        }
        let current = match mode {
            FileMode::Plain => self
                .doc()
                .is_some_and(|d| d.changes_version == Some(d.version)),
            FileMode::Diff => self.diff.version == Some(self.versions),
        };
        if !current {
            self.looked_at = None;
            self.look_up_changes(cx);
        }
        self.drop_find_without_text();
        self.find_again(cx);
        cx.notify();
    }

    /// The toggle or a change bar: show `mode`, and tell the workspace (it saves the mode).
    fn choose_mode(
        &mut self,
        mode: FileMode,
        reveal: Option<(usize, LineChange)>,
        cx: &mut Context<Self>,
    ) {
        let was = self.mode();
        self.set_mode(mode, reveal, cx);
        if was != mode {
            cx.emit(FileTabEvent::Mode(mode));
        }
    }

    /// Scroll to `to`, kept within the text (as last drawn).
    pub fn set_scroll(&mut self, to: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(g) = self.geom.get() else {
            return;
        };
        let to = Geom::clamp(to, g.max_scroll);
        let mode = self.mode;
        let scroll = self.mode_scroll_mut(mode);
        if to != *scroll {
            *scroll = to;
            cx.notify();
        }
    }

    fn wheel(&mut self, e: &ScrollWheelEvent, cx: &mut Context<Self>) {
        let Some(g) = self.geom.get() else {
            return;
        };
        let d = e.delta.pixel_delta(g.line_height);
        self.set_scroll(point(g.scroll.x - d.x, g.scroll.y - d.y), cx);
    }

    /// The place under window point `p` (the nearest character edge; above or below the text,
    /// its first or last line).
    fn pos_at(&self, p: Point<Pixels>, window: &mut Window, cx: &App) -> Option<Pos> {
        let g = self.geom.get()?;
        let doc = self.shown_doc()?;
        let wrapped = self.wrapped.borrow().clone();
        let count = wrapped.as_ref().map_or(doc.lines.len(), |w| w.rows.len());
        let y = p.y - g.bounds.top() + g.scroll.y;
        let row = if y < px(0.0) {
            0
        } else {
            ((y / g.line_height).floor() as usize).min(count.saturating_sub(1))
        };
        // Wrapped (M11x part 2): in cells from the row's start, kept in the row (as it's drawn).
        if let Some(w) = &wrapped {
            let (line, cell0) = w.row(row);
            let span = wrapped_row(doc, line, cell0, w.columns);
            let last = w.row(row + 1).0 != line || row + 1 >= w.rows.len();
            let x = p.x - g.bounds.left() - g.text_left;
            let col = row_hit(doc, line, span, last, x / g.cell_width.max(px(1.0)));
            return Some(Pos { line, col });
        }
        let line = row;
        let x = p.x - g.bounds.left() - g.text_left + g.scroll.x;
        if x <= px(0.0) {
            return Some(Pos { line, col: 0 });
        }
        if doc.is_long(line) {
            // From its checkpoints, in cells (the font is monospaced): never the whole line.
            let cell = (x / g.cell_width).round() as usize;
            let (col, _) = doc.byte_at_cell(line, cell);
            return Some(Pos { line, col });
        }
        let l = doc.line(line);
        let shown = expand_tabs(l);
        if shown.is_empty() {
            return Some(Pos { line, col: 0 });
        }
        let m = CellMetrics::measure(cx.global::<Appearance>(), window);
        let run = plain_run(shown.len(), &m, Rgb { r: 0, g: 0, b: 0 });
        let at = window
            .text_system()
            .shape_line(SharedString::from(shown), m.font_size, &[run], None)
            .closest_index_for_x(x);
        Some(Pos {
            line,
            col: Tabs::of(l).source(at),
        })
    }

    fn mouse_down(&mut self, e: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.focus.focus(window, cx);
        // A change bar (or a deleted-lines ▸) in a file: its diff, at that change.
        if let Some(change) = self.bar_under(e.position) {
            self.choose_mode(FileMode::Diff, Some(change), cx);
            return;
        }
        if let Some(pos) = self.pos_at(e.position, window, cx) {
            self.selection = Some((pos, pos));
            self.selecting = true;
            cx.notify();
        }
    }

    /// The change whose bar (or ▸) is under window point `p`, in Plain mode: a press from 2 pt
    /// left of the bar column to 2 pt right of it.
    fn bar_under(&self, p: Point<Pixels>) -> Option<(usize, LineChange)> {
        if self.mode != FileMode::Plain {
            return None;
        }
        let (g, doc) = (self.geom.get()?, self.doc()?);
        let x = f32::from(p.x - g.bounds.left() - g.bar_left);
        if !(-2.0..=BAR_WIDTH + 2.0).contains(&x) {
            return None;
        }
        let y = f32::from(p.y - g.bounds.top() + g.scroll.y);
        if let Some(w) = self.wrapped.borrow().as_ref() {
            return bar_at_rows(y, f32::from(g.line_height), w, &doc.changes);
        }
        bar_at(y, f32::from(g.line_height), &doc.changes)
    }

    fn mouse_drag(&mut self, e: &MouseMoveEvent, window: &mut Window, cx: &mut Context<Self>) {
        if !self.selecting || e.pressed_button != Some(MouseButton::Left) {
            return;
        }
        let Some(head) = self.pos_at(e.position, window, cx) else {
            return;
        };
        if let Some((anchor, old)) = self.selection
            && old != head
        {
            self.selection = Some((anchor, head));
            cx.notify();
        }
    }

    fn copy(&mut self, cx: &mut Context<Self>) {
        let text = self.selected_text();
        if !text.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
    }

    /// ⌘F: the find bar, its box taking the keyboard (its text selected, when open already).
    /// Nothing without a text (a missing or refused file, or one still loading).
    fn open_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.shown_doc().is_none() {
            return;
        }
        // M11ac: find works on the source, so the Preview turns off first.
        if self.preview_shown() {
            self.choose_preview(false, window, cx);
        }
        if let Some(find) = &self.find {
            let input = find.input.clone();
            input.update(cx, |input, cx| {
                input.select_all();
                cx.notify();
            });
            input.read(cx).focus_handle(cx).focus(window, cx);
            return;
        }
        let input = cx.new(|cx| NameInput::new(String::new(), window, cx));
        let events = cx.subscribe_in(&input, window, |this, input, event, window, cx| {
            match event {
                NameInputEvent::Confirm(_) => this.find_step(true, window, cx),
                // Esc in the box (not the box losing the keyboard): the bar closes.
                NameInputEvent::Cancel => {
                    if input.read(cx).focus_handle(cx).is_focused(window) {
                        this.close_find(window, cx);
                    }
                }
            }
        });
        let typed = cx.observe(&input, |this, input, cx| {
            let query = input.read(cx).text().to_string();
            this.find_query(query, cx);
        });
        input.read(cx).focus_handle(cx).focus(window, cx);
        self.find = Some(FindBar {
            input,
            finds: Finds::default(),
            _subscriptions: vec![events, typed],
        });
        cx.notify();
    }

    /// The find box's text changed: search for it (off the UI thread).
    fn find_query(&mut self, query: String, cx: &mut Context<Self>) {
        if self.find.as_ref().is_none_or(|f| f.finds.query == query) {
            return;
        }
        self.searches += 1;
        let generation = self.searches;
        if let Some(find) = self.find.as_mut() {
            find.finds.ask(generation, Some(query));
        }
        self.search(cx);
    }

    /// The text changed under the find bar: search again (the current match kept by number).
    fn find_again(&mut self, cx: &mut Context<Self>) {
        if self.find.is_none() {
            return;
        }
        self.searches += 1;
        let generation = self.searches;
        if let Some(find) = self.find.as_mut() {
            find.finds.ask(generation, None);
        }
        self.search(cx);
    }

    /// Run the latest search asked (off the UI thread), one at a time: asked again meanwhile, it
    /// runs once more when this one ends. Only the latest search's answer is taken (`Finds`).
    fn search(&mut self, cx: &mut Context<Self>) {
        let Some((text, lines)) = self.shown_doc().map(|d| (d.text.clone(), d.lines.clone()))
        else {
            // Nothing shown (a diff not in yet): nothing found, until it is.
            if let Some(find) = self.find.as_mut() {
                find.finds.matches = Arc::default();
                find.finds.current = None;
                find.finds.capped = false;
            }
            return;
        };
        let Some(find) = self.find.as_ref() else {
            return;
        };
        let (generation, query) = (find.finds.wanted, find.finds.query.clone());
        if !self.searching.start() {
            return;
        }
        cx.spawn(async move |this, cx| {
            let (found, capped) = cx
                .background_executor()
                .spawn(async move { find_matches(&text, &lines, &query, FIND_CAP) })
                .await;
            this.update(cx, |this, cx| {
                let top = this
                    .geom
                    .get()
                    .map_or(0, |g| (g.scroll.y / g.line_height).floor() as usize);
                let top = this.wrapped.borrow().as_ref().map_or(top, |w| w.row(top).0);
                // Scrolled to its current match only for a new query (⌘G, ⇧⌘G and Return scroll
                // by themselves): a reload's search leaves the view where it is.
                let taken = this
                    .find
                    .as_mut()
                    .and_then(|find| find.finds.accept(generation, found, capped, top));
                if let Some(new_query) = taken {
                    if new_query {
                        this.reveal_current(cx);
                    }
                    cx.notify();
                }
                if this.searching.finish() {
                    this.search(cx);
                }
            })
            .ok();
        })
        .detach();
    }

    /// ⌘G / ⇧⌘G (and Return in the box): the next or previous match, round the ends. With the
    /// bar closed, it opens.
    fn find_step(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(find) = self.find.as_mut() else {
            self.open_find(window, cx);
            return;
        };
        let finds = &mut find.finds;
        finds.current = step(finds.current, finds.matches.len(), forward);
        self.reveal_current(cx);
        cx.notify();
    }

    /// Esc: the find bar closes and the tab takes the keyboard. With no bar, Esc goes on.
    fn close_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.find.take().is_none() {
            cx.propagate();
            return;
        }
        self.focus.focus(window, cx);
        cx.notify();
    }

    /// Scroll the current match into sight: its line to the middle when off screen, and
    /// sideways when it's out of the text's width.
    fn reveal_current(&mut self, cx: &mut Context<Self>) {
        let Some(hit) = self
            .find
            .as_ref()
            .and_then(|f| f.finds.matches.get(f.finds.current?).cloned())
        else {
            return;
        };
        let (Some(g), Some(doc)) = (self.geom.get(), self.shown_doc()) else {
            return;
        };
        let (lh, h, w) = (g.line_height, g.bounds.size.height, g.bounds.size.width);
        let mut to = g.scroll;
        // Wrapped (M11x part 2): the row the match starts on; no sideways scroll.
        let wrapped = self.wrapped.borrow().clone();
        if let Some(w) = wrapped.as_ref() {
            let cell = doc.cell_of(hit.line, hit.cols.start);
            let top = lh * (w.first_row(hit.line) + cell / w.columns) as f32;
            if top < to.y || top + lh > to.y + h {
                to.y = top - (h - lh) / 2.0;
            }
            self.set_scroll(to, cx);
            return;
        }
        let top = lh * hit.line as f32;
        if top < to.y || top + lh > to.y + h {
            to.y = top - (h - lh) / 2.0;
        }
        let x = |col: usize| g.cell_width * doc.cell_of(hit.line, col) as f32;
        let (x0, x1) = (x(hit.cols.start), x(hit.cols.end));
        let room = w - g.text_left - px(RIGHT_PAD);
        if x0 < to.x || x1 > to.x + room {
            to.x = (x0 + x1) / 2.0 - room / 2.0;
        }
        self.set_scroll(to, cx);
    }

    /// Open the file in the default text editor, or show a refused one in the Finder
    /// (`open_argv`; off the UI thread). Never with the file's own app: that could run it. The
    /// only process a file tab starts.
    fn open_in_editor(&self, cx: &mut Context<Self>) {
        let Some(args) = open_argv(&self.state, &self.path) else {
            return;
        };
        cx.background_executor()
            .spawn(async move {
                let _ = std::process::Command::new("/usr/bin/open")
                    .args(&args)
                    .status();
            })
            .detach();
    }

    fn icon_button(
        id: &'static str,
        kind: Glyph,
        tip: &'static str,
        ui: UiColors,
        on_click: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .id(id)
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .size(px(BUTTON))
            .rounded(px(5.0))
            .cursor_pointer()
            .hover(move |s| s.bg(hsla(ui.panel_hover)))
            .child(glyph_with(kind, ui.text, BUTTON_STROKE))
            .on_click(
                cx.listener(move |this, _: &ClickEvent, window, cx| on_click(this, window, cx)),
            )
            .tooltip(move |_, cx| -> AnyView { cx.new(|_| Tip(tip.into())).into() })
            .into_any_element()
    }

    fn text_button(
        id: &'static str,
        label: &'static str,
        ui: UiColors,
        on_click: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        div()
            .id(id)
            .relative()
            .flex()
            .flex_none()
            .items_center()
            .h(px(BUTTON))
            .px(px(10.0))
            .rounded(px(5.0))
            .border_1()
            .border_color(hsla(ui.panel_border))
            .bg(hsla(ui.panel))
            .text_size(px(text::NAME))
            .text_color(hsla(ui.text))
            .cursor_pointer()
            .hover(move |s| s.bg(hsla(ui.panel_hover)))
            .on_click(
                cx.listener(move |this, _: &ClickEvent, window, cx| on_click(this, window, cx)),
            )
            .child(label)
    }

    /// One of the toggle's two segments (`i`: 0 File, 1 Diff): the current one `card_selected`
    /// in `text`, the other `panel` in `text_dim` (`panel_hover` on hover); a click shows its
    /// mode.
    fn mode_segment(
        &self,
        i: usize,
        mode: FileMode,
        ui: UiColors,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let (id, label, tip) = match mode {
            FileMode::Plain => ("file-mode-file", TOGGLE_FILE, TIP_FILE),
            FileMode::Diff => ("file-mode-diff", TOGGLE_DIFF, TIP_DIFF),
        };
        let current = self.mode == mode;
        let at = self.toggle_at.clone();
        // Inside the 1 pt border: its 5 pt corners less the border.
        let inner = px(4.0);
        div()
            .id(id)
            .relative()
            .flex()
            .flex_none()
            .items_center()
            .h_full()
            .px(px(10.0))
            .map(|d| match i {
                0 => d.rounded_l(inner),
                _ => d
                    .rounded_r(inner)
                    .border_l_1()
                    .border_color(hsla(ui.panel_border)),
            })
            .bg(hsla(if current { ui.card_selected } else { ui.panel }))
            .text_color(hsla(if current { ui.text } else { ui.text_dim }))
            .when(!current, |d| {
                d.cursor_pointer()
                    .hover(move |s| s.bg(hsla(ui.panel_hover)))
            })
            .on_click(
                cx.listener(move |this, _: &ClickEvent, _, cx| this.choose_mode(mode, None, cx)),
            )
            .tooltip(move |_, cx| -> AnyView { cx.new(|_| Tip(tip.into())).into() })
            .child(label)
            // Where it is (the scenes click it).
            .child(
                canvas(
                    move |b, _, _| {
                        let mut both = at.get();
                        both[i] = Some(b);
                        at.set(both);
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
    }

    /// The File | Diff toggle: two joined segments, a 1 pt border, 5 pt corners.
    /// Wrap (M11x part 2): one button, filled when on, outlined when off; a click turns it the
    /// other way for every file tab (`file-wrap`, kept in the settings).
    fn wrap_button(&self, ui: UiColors, cx: &mut Context<Self>) -> AnyElement {
        let on = cx.global::<Appearance>().file_wrap;
        div()
            .id("file-wrap")
            .flex()
            .flex_none()
            .items_center()
            .h(px(BUTTON))
            .px(px(10.0))
            .rounded(px(5.0))
            .border_1()
            .border_color(hsla(ui.panel_border))
            .text_size(px(text::NAME))
            .when(on, |b| {
                b.bg(hsla(ui.card_selected)).text_color(hsla(ui.text))
            })
            .when(!on, |b| {
                b.text_color(hsla(ui.text_dim))
                    .hover(move |s| s.bg(hsla(ui.panel_hover)))
            })
            .cursor_pointer()
            .tooltip(move |_, cx| -> AnyView { cx.new(|_| Tip(TIP_WRAP.into())).into() })
            .child(WRAP)
            .on_click(cx.listener(|_, _: &ClickEvent, _, cx| {
                let on = !cx.global::<Appearance>().file_wrap;
                let values = if on { vec!["on".to_string()] } else { vec![] };
                crate::preferences::apply(cx, "file-wrap", &values);
            }))
            .into_any_element()
    }

    /// Preview (M11ac): beside Wrap and drawn as it, filled when on; a click shows a Markdown
    /// file rendered, or its source again.
    /// Over `PREVIEW_MAX` (or too large to colour) it is drawn faint and does nothing, its
    /// tooltip saying why.
    fn preview_button(&self, ui: UiColors, cx: &mut Context<Self>) -> AnyElement {
        let fits = preview_fits(&self.state);
        let on = self.preview && fits;
        let tip = if fits {
            TIP_PREVIEW
        } else {
            TIP_PREVIEW_TOO_LARGE
        };
        let at = self.preview_at.clone();
        div()
            .id("file-preview")
            .relative()
            .flex()
            .flex_none()
            .items_center()
            .h(px(BUTTON))
            .px(px(10.0))
            .rounded(px(5.0))
            .border_1()
            .border_color(hsla(ui.panel_border))
            .text_size(px(text::NAME))
            .when(on, |b| {
                b.bg(hsla(ui.card_selected)).text_color(hsla(ui.text))
            })
            .when(!on && fits, |b| {
                b.text_color(hsla(ui.text_dim))
                    .hover(move |s| s.bg(hsla(ui.panel_hover)))
                    .cursor_pointer()
            })
            .when(!fits, |b| b.text_color(hsla(ui.text_faint)))
            .when(on, |b| b.cursor_pointer())
            .tooltip(move |_, cx| -> AnyView { cx.new(|_| Tip(tip.into())).into() })
            .child(PREVIEW)
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                if preview_fits(&this.state) {
                    this.choose_preview(!this.preview, window, cx)
                }
            }))
            // Where it is (the scenes click it).
            .child(
                canvas(move |b, _, _| at.set(Some(b)), |_, _, _, _| {})
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full(),
            )
            .into_any_element()
    }

    fn toggle(&self, ui: UiColors, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex()
            .flex_none()
            .flex_row()
            .h(px(BUTTON))
            .rounded(px(5.0))
            .border_1()
            .border_color(hsla(ui.panel_border))
            .text_size(px(text::NAME))
            .child(self.mode_segment(0, FileMode::Plain, ui, cx))
            .child(self.mode_segment(1, FileMode::Diff, ui, cx))
            .into_any_element()
    }

    /// The toolbar: the file's path, a note, the File | Diff toggle (in a repo, or in Diff
    /// mode), Open in Editor.
    fn toolbar(&self, ui: UiColors, cx: &mut Context<Self>) -> AnyElement {
        let note = match self.state {
            State::PlainTooLarge(_) => Some(TOO_LARGE),
            _ => None,
        };
        let toggle = shows_toggle(self.git_place.is_some(), self.mode).then(|| self.toggle(ui, cx));
        // Wrap, left of File | Diff (M11x part 2), wherever there's text to wrap.
        let has_text = matches!(self.state, State::Text(_) | State::PlainTooLarge(_));
        let wrap = has_text.then(|| self.wrap_button(ui, cx));
        // Preview, right of Wrap (M11ac): a Markdown file in File mode.
        let preview =
            (has_text && shows_preview(&self.path, self.mode)).then(|| self.preview_button(ui, cx));
        // A refused file has its button in the middle; a missing one has none.
        let open = matches!(
            self.state,
            State::Loading | State::Text(_) | State::PlainTooLarge(_)
        )
        .then(|| {
            Self::text_button(
                "file-open-in-editor",
                OPEN_IN_EDITOR,
                ui,
                |this, _, cx| this.open_in_editor(cx),
                cx,
            )
        });
        div()
            .flex()
            .flex_none()
            .flex_row()
            .items_center()
            .w_full()
            .h(px(TOOLBAR))
            .px(px(BAR_PAD + 4.0))
            .gap(px(8.0))
            .bg(hsla(ui.panel))
            .border_b_1()
            .border_color(hsla(ui.panel_border))
            .child(glyph(Glyph::File, ui.text_dim))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .truncate()
                    .text_size(px(text::NAME))
                    .text_color(hsla(ui.text_dim))
                    .child(SharedString::from(self.path.display().to_string())),
            )
            .children(note.map(|note| {
                div()
                    .flex_none()
                    .text_size(px(text::META))
                    .text_color(hsla(ui.text_faint))
                    .child(note)
            }))
            .children(wrap)
            .children(preview)
            .children(toggle)
            .children(open)
            .into_any_element()
    }

    /// The find bar, when open: its box, its count, previous / next, close.
    fn find_bar(&self, ui: UiColors, cx: &mut Context<Self>) -> Option<AnyElement> {
        let find = self.find.as_ref()?;
        let input = find.input.clone();
        let empty = input.read(cx).text().is_empty();
        let f = &find.finds;
        let label = find_label(&f.query, f.current, f.matches.len(), f.capped);
        let focus_box = input.clone();
        let prev = Self::icon_button(
            "file-find-prev",
            Glyph::Back,
            "Previous match",
            ui,
            |this, window, cx| this.find_step(false, window, cx),
            cx,
        );
        let next = Self::icon_button(
            "file-find-next",
            Glyph::Forward,
            "Next match",
            ui,
            |this, window, cx| this.find_step(true, window, cx),
            cx,
        );
        let close = Self::icon_button(
            "file-find-close",
            Glyph::Stop,
            "Close the find bar",
            ui,
            |this, window, cx| this.close_find(window, cx),
            cx,
        );
        Some(
            div()
                .flex()
                .flex_none()
                .flex_row()
                .items_center()
                .w_full()
                .h(px(TOOLBAR))
                .px(px(BAR_PAD))
                .gap(px(2.0))
                .bg(hsla(ui.panel))
                .border_b_1()
                .border_color(hsla(ui.panel_border))
                .child(
                    div()
                        .flex_none()
                        .w(px(FIND_BOX))
                        // The box keeps its clicks from reaching what holds it: it takes the
                        // keyboard here, before they arrive.
                        .capture_any_mouse_down(move |_, window, cx| {
                            let focus = focus_box.read(cx).focus_handle(cx);
                            focus.focus(window, cx);
                        })
                        .child(overlay_panel::filter_box(
                            &ui,
                            &input,
                            empty,
                            FIND_PLACEHOLDER,
                        )),
                )
                .child(
                    div()
                        .flex_none()
                        .min_w(px(72.0))
                        .px(px(8.0))
                        .text_size(px(text::META))
                        .text_color(hsla(ui.text_dim))
                        .child(SharedString::from(label)),
                )
                .child(prev)
                .child(next)
                .child(div().flex_1())
                .child(close)
                .into_any_element(),
        )
    }

    /// The note the body shows instead of text (`note_of`).
    fn body_note(&self) -> Option<&'static str> {
        note_of(&self.state, self.mode, &self.diff.state)
    }

    /// A diff's note in the middle of the body (the toolbar keeps Open in Editor and the
    /// toggle).
    fn diff_note(words: &'static str, ui: UiColors) -> AnyElement {
        div()
            .flex()
            .flex_1()
            .min_h(px(0.0))
            .items_center()
            .justify_center()
            .px(px(24.0))
            .text_size(px(text::NAME))
            .text_color(hsla(ui.text))
            .child(words)
            .into_any_element()
    }

    /// A note in the middle of the body, with its button: a missing or refused file.
    fn notice(&self, ui: UiColors, cx: &mut Context<Self>) -> AnyElement {
        let missing = matches!(self.state, State::Missing);
        let words = if missing { MISSING } else { REFUSED };
        let button = if missing {
            let at = self.close_at.clone();
            Self::text_button(
                "file-close",
                "Close",
                ui,
                |_, _, cx| cx.emit(FileTabEvent::Close),
                cx,
            )
            // Where it is (the scenes click it).
            .child(
                canvas(move |b, _, _| at.set(Some(b)), |_, _, _, _| {})
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full(),
            )
        } else {
            Self::text_button(
                "file-open-in-editor-body",
                open_label(&self.state).unwrap_or(SHOW_IN_FINDER),
                ui,
                |this, _, cx| this.open_in_editor(cx),
                cx,
            )
        };
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h(px(0.0))
            .items_center()
            .justify_center()
            .gap(px(10.0))
            .px(px(24.0))
            .child(
                div()
                    .text_size(px(text::NAME))
                    .text_color(hsla(ui.text))
                    .child(words),
            )
            .when(missing, |d| {
                d.child(
                    div()
                        .max_w_full()
                        .truncate()
                        .text_size(px(text::META))
                        .text_color(hsla(ui.text_dim))
                        .child(SharedString::from(self.path.display().to_string())),
                )
            })
            .child(button)
            .into_any_element()
    }

    /// The text: a canvas drawing the lines in sight, the wheel scrolling it, the mouse
    /// selecting.
    fn body(&self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        let Some(doc) = self.shown_doc().cloned() else {
            return div().flex_1().into_any_element();
        };
        let (matches, current) = self.find.as_ref().map_or((Arc::default(), None), |f| {
            (f.finds.matches.clone(), f.finds.current)
        });
        let (diff, gutter) = match (self.mode, &self.diff.state) {
            (FileMode::Diff, DiffState::Rows(d)) => (Some(d.rows.clone()), Some(d.gutter)),
            _ => (None, None),
        };
        let frame = Rc::new(Frame {
            // A diff's colours are only made when it may have them.
            coloured: diff.is_some() || matches!(self.state, State::Text(_)),
            diff,
            gutter,
            mode: self.mode,
            doc,
            scroll: self.mode_scroll(),
            selection: self
                .selection
                .map(|(a, b)| if a <= b { (a, b) } else { (b, a) }),
            matches,
            current,
            theme,
        });
        let geom = self.geom.clone();
        let wrapped = self.wrapped.clone();
        let drawn = self.drawn.clone();
        let view = cx.entity();
        let keep = view.clone();
        let laid = frame.clone();
        // The text is clipped between the top and bottom rooms, so a line scrolled half out of
        // sight doesn't show in them (nor in the card's bottom corners); a press or the wheel in
        // a room still reaches the text.
        let text = div().size_full().overflow_hidden();
        div()
            .flex_1()
            .min_h(px(0.0))
            .w_full()
            .pt(px(TOP_ROOM))
            .pb(px(BOTTOM_ROOM))
            .on_scroll_wheel(cx.listener(|this, e: &ScrollWheelEvent, _, cx| this.wheel(e, cx)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, e: &MouseDownEvent, window, cx| this.mouse_down(e, window, cx)),
            )
            .child(
                text.child(
                    canvas(
                        move |bounds, window, cx| {
                            let m = CellMetrics::measure(cx.global::<Appearance>(), window);
                            let count = laid.doc.lines.len();
                            let gutter = laid.gutter.unwrap_or_else(|| Gutter::plain(count));
                            // Wrap on (M11x part 2): rows at the text's width, made again only
                            // for another version, mode or width; no sideways scroll.
                            let wrap = cx.global::<Appearance>().file_wrap.then(|| {
                                let cw = f32::from(m.cell_width).max(1.0);
                                let left = columns(cw, gutter).text_left;
                                let room = f32::from(bounds.size.width) - left - RIGHT_PAD;
                                let cols = (room / cw).floor().max(1.0) as usize;
                                let key = (laid.mode, laid.doc.version, cols);
                                let kept = wrapped.borrow().clone().filter(|w| w.key == key);
                                kept.unwrap_or_else(|| Wrapped::of(&laid.doc, laid.mode, cols))
                            });
                            // Wrap switched: the same line stays at the top (review fix).
                            let before = wrapped.borrow().clone();
                            let mut scroll = laid.scroll;
                            if before.is_some() != wrap.is_some() {
                                let lh = f32::from(m.line_height).max(1.0);
                                let top = (f32::from(scroll.y) / lh).floor().max(0.0) as usize;
                                let row = rescroll_row(top, before.as_ref(), wrap.as_ref());
                                scroll = point(px(0.0), m.line_height * row as f32);
                                let mode = laid.mode;
                                keep.update(cx, |this, _| *this.mode_scroll_mut(mode) = scroll);
                            }
                            *wrapped.borrow_mut() = wrap.clone();
                            let (rows, widest) = match &wrap {
                                Some(w) => (w.rows.len(), 0),
                                None => (count, laid.doc.widest),
                            };
                            let g = Geom::of(bounds, &m, rows, gutter, widest, scroll);
                            geom.set(Some(g));
                            // A new version (or mode) drawn: the scroll as kept in it is the
                            // scroll (after a shrink, the view doesn't jump back down when the
                            // file grows again).
                            let key = (laid.mode, laid.doc.version);
                            if drawn.replace(key) != key && g.scroll != laid.scroll {
                                let (kept, mode) = (g.scroll, laid.mode);
                                keep.update(cx, |this, _| *this.mode_scroll_mut(mode) = kept);
                            }
                            let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
                            (m, g, hitbox, wrap)
                        },
                        move |_, (m, g, hitbox, wrap), window, cx| {
                            window.set_cursor_style(CursorStyle::IBeam, &hitbox);
                            paint_body(&frame, &g, &m, wrap.as_ref(), window, cx);
                            // A drag keeps going outside the text; registered for this frame.
                            window.on_mouse_event({
                                let view = view.clone();
                                move |e: &MouseMoveEvent, phase, window, cx| {
                                    if phase == DispatchPhase::Bubble {
                                        view.update(cx, |this, cx| this.mouse_drag(e, window, cx));
                                    }
                                }
                            });
                            window.on_mouse_event(move |_: &MouseUpEvent, phase, _, cx| {
                                if phase == DispatchPhase::Bubble {
                                    view.update(cx, |this, _| this.selecting = false);
                                }
                            });
                        },
                    )
                    .size_full(),
                ),
            )
            .into_any_element()
    }
}

/// What the UI screenshots read of a tab, and the change list they give it.
#[cfg(feature = "ui-shots")]
impl FileTab {
    /// M11ac: the Preview button as last drawn, the reader's scroll, and the version parsed.
    pub fn preview_bounds(&self) -> Option<Bounds<Pixels>> {
        self.preview_at.get()
    }

    pub fn preview_list(&self) -> ListState {
        self.preview_list.clone()
    }

    /// M11ac Part 2: how many Mermaid blocks show as diagrams, and the notes under the others,
    /// as last drawn.
    pub fn mermaid_drawn(&self) -> (usize, Vec<String>) {
        let mut diagrams = 0;
        let mut notes = Vec::new();
        for shown in self.mermaid_shown.iter().flatten() {
            match shown {
                md_preview::Shown::Diagram { .. } => diagrams += 1,
                md_preview::Shown::Code(note) => notes.extend(note.clone()),
            }
        }
        (diagrams, notes)
    }

    pub fn parsed_version(&self) -> Option<u64> {
        self.parsed.as_ref().map(|(v, _)| *v)
    }

    /// The lines of the text shown.
    pub fn line_count(&self) -> usize {
        self.shown_doc().map_or(0, |d| d.lines.len())
    }

    /// The screen rows as last drawn with Wrap on (M11x part 2); none with it off.
    pub fn wrapped_rows(&self) -> Option<usize> {
        self.wrapped.borrow().as_ref().map(|w| w.rows.len())
    }

    /// "loading", "text", "plain", "refuse" or "missing".
    pub fn state_name(&self) -> &'static str {
        match self.state {
            State::Loading => "loading",
            State::Text(_) => "text",
            State::PlainTooLarge(_) => "plain",
            State::Refuse => "refuse",
            State::Missing => "missing",
        }
    }

    /// The note the tab shows instead of (or over) the text.
    pub fn note(&self) -> Option<&'static str> {
        match self.state {
            State::PlainTooLarge(_) => Some(TOO_LARGE),
            State::Refuse => Some(REFUSED),
            State::Missing => Some(MISSING),
            State::Loading | State::Text(_) => None,
        }
    }

    pub fn geom(&self) -> Option<Geom> {
        self.geom.get()
    }

    /// The scroll of the mode shown.
    pub fn scroll(&self) -> Point<Pixels> {
        self.mode_scroll()
    }

    /// The note the body shows instead of text (`MISSING`, `REFUSED`, a diff's), if any.
    pub fn shown_note(&self) -> Option<&'static str> {
        self.body_note()
    }

    /// The toggle's File and Diff segments, as last drawn.
    pub fn toggle_bounds(&self) -> [Option<Bounds<Pixels>>; 2] {
        self.toggle_at.get()
    }

    /// "waiting", "rows", "same", "binary", "norepo" or "refuse".
    pub fn diff_state_name(&self) -> &'static str {
        match self.diff.state {
            DiffState::Waiting => "waiting",
            DiffState::Rows(_) => "rows",
            DiffState::Same => "same",
            DiffState::Binary => "binary",
            DiffState::NoRepo => "norepo",
            DiffState::Refuse => "refuse",
        }
    }

    /// A frozen tab has no git place (no look-up), so Plain mode shows no toggle: give it a
    /// made-up one, so the scenes can click File and Diff both ways. git is never run on it.
    pub fn pretend_in_repo_for_shots(&mut self, cx: &mut Context<Self>) {
        let root = self
            .path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default();
        let rel = self.path.file_name().map(PathBuf::from).unwrap_or_default();
        self.git_place = Some(GitPlace {
            git: PathBuf::from("git"),
            root,
            rel,
        });
        cx.notify();
    }

    /// The diff's rows, once in.
    pub fn diff_doc(&self) -> Option<&DiffDoc> {
        match &self.diff.state {
            DiffState::Rows(d) => Some(d),
            _ => None,
        }
    }

    /// A diff as git would give it for the file as read (`unified`; a `Binary files` line and
    /// no hunk: binary), made into rows at once, coloured when the shared highlighter is
    /// loaded (and the file isn't over 2 MB).
    pub fn set_diff_for_shots(&mut self, unified: &str, cx: &mut Context<Self>) {
        let hunks = unified.lines().any(|l| l.starts_with("@@ "));
        let diff = if !hunks && unified.lines().any(|l| l.starts_with("Binary files ")) {
            FileDiff::Binary
        } else {
            FileDiff::Unified(unified.to_string())
        };
        let theme = cx.global::<Appearance>().theme;
        let h = cx.default_global::<Highlighters>().get(&theme);
        let disk = self.doc().map(|d| d.text.clone());
        let colour = h.as_deref().map(|h| (h, self.path.as_path()));
        let state = diff_state(diff, disk.as_deref(), self.versions, colour, None);
        let version = self.versions;
        self.accept_diff(version, state, cx);
    }

    pub fn close_bounds(&self) -> Option<Bounds<Pixels>> {
        self.close_at.get()
    }

    /// The find bar's count ("1 of 7"); empty with the bar closed.
    pub fn find_label(&self) -> String {
        self.find.as_ref().map_or_else(String::new, |f| {
            find_label(
                &f.finds.query,
                f.finds.current,
                f.finds.matches.len(),
                f.finds.capped,
            )
        })
    }

    /// Whether the find bar is open.
    pub fn find_open(&self) -> bool {
        self.find.is_some()
    }

    /// Change bars from a list (git's answer; a scene's given list).
    pub fn set_changes(&mut self, changes: Vec<(usize, LineChange)>, cx: &mut Context<Self>) {
        if let Some(doc) = self.doc_mut() {
            let version = doc.version;
            if doc.accept_changes(version, &changes) {
                cx.notify();
            }
        }
    }
}

impl FileTab {
    /// Diff mode with rows and a place to show: scroll it `REVEAL_ABOVE` rows from the top
    /// (kept within the rows when drawn).
    fn reveal_diff(&mut self, window: &mut Window, cx: &App) {
        if self.mode != FileMode::Diff {
            return;
        }
        let DiffState::Rows(d) = &self.diff.state else {
            return;
        };
        let Some(reveal) = self.reveal.take() else {
            return;
        };
        let row = match reveal {
            Reveal::First => first_change(&d.rows),
            Reveal::Change(change) => reveal_row(&d.rows, change).or_else(|| first_change(&d.rows)),
        };
        let m = CellMetrics::measure(cx.global::<Appearance>(), window);
        let mut rows_up = row.unwrap_or(0).saturating_sub(REVEAL_ABOVE);
        // Wrap on (M11x part 2): that line's first screen row, the diff wrapped at the width
        // last drawn (its own gutter may move it a column; the next frame wraps it exactly).
        let columns = self.wrapped.borrow().as_ref().map(|w| w.columns);
        if let (Some(columns), true) = (columns, cx.global::<Appearance>().file_wrap) {
            rows_up = Wrapped::of(&d.doc, FileMode::Diff, columns).first_row(rows_up);
        }
        self.diff_scroll = point(px(0.0), m.line_height * rows_up as f32);
    }
}

impl Render for FileTab {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // The find bar went with the file: the keyboard back to the tab.
        if std::mem::take(&mut self.refocus) {
            self.focus.focus(window, cx);
        }
        let theme = cx.global::<Appearance>().theme;
        let ui = theme.ui();
        self.reveal_diff(window, cx);
        let toolbar = self.toolbar(ui, cx);
        let find_bar = self.find_bar(ui, cx);
        let body = match self.body_note() {
            Some(words) if words == MISSING || words == REFUSED => self.notice(ui, cx),
            Some(words) => Self::diff_note(words, ui),
            None if self.preview_shown() => self.preview_body(theme, cx),
            None => self.body(theme, cx),
        };
        div()
            .flex()
            .flex_col()
            .size_full()
            .track_focus(&self.focus)
            .key_context("FileTab")
            // Its bottom corners rounded with its card's (M11i).
            .bg(hsla(theme.background))
            .rounded_b(px(CARD_RADIUS))
            .text_color(hsla(ui.text))
            .on_action(cx.listener(|this, _: &Find, window, cx| this.open_find(window, cx)))
            .on_action(
                cx.listener(|this, _: &FindNext, window, cx| this.find_step(true, window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &FindPrev, window, cx| this.find_step(false, window, cx)),
            )
            .on_action(cx.listener(|this, _: &CloseFind, window, cx| this.close_find(window, cx)))
            .on_action(cx.listener(|this, _: &Copy, _, cx| this.copy(cx)))
            .child(toolbar)
            .children(find_bar)
            .child(body)
    }
}

impl Workspace {
    /// Open file `path` in the focused pane in `mode` (Files: Plain; Changes: Diff): the pane's
    /// tab that already shows it (in either mode, now in `mode`), else a new tab at the end of
    /// its bar. It is shown and takes the keyboard.
    pub(crate) fn open_file_tab(
        &mut self,
        path: PathBuf,
        mode: FileMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(pane) = self.model.focused_pane() else {
            return;
        };
        let Some(opened) = self.model.open_file(pane, path.clone(), mode) else {
            return;
        };
        self.sync();
        match self.file_tabs.get(&opened.tab) {
            Some(view) => view.update(cx, |view, cx| view.set_mode(mode, None, cx)),
            None => self.make_file_tab(opened.tab, path, mode, window, cx),
        }
        self.save();
        self.activate(window, cx);
        cx.notify();
    }

    /// Preview file `path` in the focused pane in `mode` (M11t: a single click in Files or
    /// Changes): the pane's tab that shows it, else its preview tab now showing it (a new view
    /// for the new file), else a new preview tab. It is shown and takes the keyboard.
    pub(crate) fn preview_file_tab(
        &mut self,
        path: PathBuf,
        mode: FileMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(pane) = self.model.focused_pane() else {
            return;
        };
        let Some(opened) = self.model.preview_file(pane, path.clone(), mode) else {
            return;
        };
        self.sync();
        match self.file_tabs.get(&opened.tab) {
            Some(view) if view.read(cx).path == path => {
                view.update(cx, |view, cx| view.set_mode(mode, None, cx))
            }
            _ => self.make_file_tab(opened.tab, path, mode, window, cx),
        }
        self.save();
        self.activate(window, cx);
        cx.notify();
    }

    /// Keep tab `tab`: no longer its pane's preview (M11t: a double click on its title).
    pub(crate) fn pin_tab(&mut self, tab: TabId, cx: &mut Context<Self>) {
        if self.model.pin(tab) {
            cx.notify();
        }
    }

    /// Close file tab `tab` (its ×, ⌘W, its menu): its view goes (`sync`), and the focused tab
    /// takes the keyboard as after any close; with nothing left, the window.
    pub(crate) fn close_file_tab(
        &mut self,
        tab: TabId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.model.close_tab(tab).is_none() {
            return;
        }
        if let Some(menu) = &self.pane_tab_menu
            && menu.tab == tab
        {
            self.pane_tab_menu = None;
        }
        self.sync();
        self.save();
        if self.all_gone() {
            let conns = self.subscribers.clone();
            self.quit_after_flush(conns, None, window, cx);
            return;
        }
        self.activate(window, cx);
    }

    /// Every file tab of the model has its view (a restored one, one dragged in). A new view
    /// for the focused tab takes the keyboard.
    pub(crate) fn ensure_file_tabs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let missing: Vec<(TabId, PathBuf, FileMode)> = self
            .model
            .workspaces()
            .iter()
            .flat_map(|w| &w.panes)
            .flat_map(|p| &p.tabs)
            .filter(|t| !self.file_tabs.contains_key(&t.id))
            .filter_map(|t| match &t.kind {
                TabKind::File { path, mode } => Some((t.id, path.clone(), *mode)),
                TabKind::Terminal(_) | TabKind::Browser { .. } => None,
            })
            .collect();
        let focused = self.model.focused_tab();
        let mut focus_it = false;
        for (tab, path, mode) in missing {
            self.make_file_tab(tab, path, mode, window, cx);
            focus_it |= Some(tab) == focused;
        }
        if focus_it {
            self.focus_active(window, cx);
        }
    }

    /// The view for file tab `tab`, showing `path` in `mode`.
    fn make_file_tab(
        &mut self,
        tab: TabId,
        path: PathBuf,
        mode: FileMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let frozen = self.places.frozen;
        let preview = self.model.md_preview(tab);
        let view = cx.new(|cx| {
            let mut view = FileTab::new(path, mode, window, cx);
            view.frozen = frozen;
            view.preview = preview;
            view
        });
        // A missing file's Close button closes its tab; a new mode is the model's, saved.
        cx.subscribe_in(
            &view,
            window,
            move |this, _, event, window, cx| match event {
                FileTabEvent::Close => this.close_file_tab(tab, window, cx),
                FileTabEvent::Mode(mode) => {
                    // Switched File ↔ Diff by hand: kept, no longer a preview (M11t).
                    this.model.pin(tab);
                    if this.model.set_file_mode(tab, *mode) {
                        this.save();
                    }
                }
                // M11ac: Preview turned on or off by hand: kept as well, and saved.
                FileTabEvent::Preview(on) => {
                    this.model.pin(tab);
                    if this.model.set_md_preview(tab, *on) {
                        this.save();
                    }
                }
                FileTabEvent::OpenFile(path) => {
                    this.open_file_tab(path.clone(), FileMode::Plain, window, cx)
                }
            },
        )
        .detach();
        self.file_tabs.insert(tab, view);
    }

    /// Tell each file tab whether it's on screen (the shown tab of a pane of the focused
    /// workspace): only those watch their file. Each frame (`render`), so showing the window
    /// again starts their polls.
    pub(crate) fn show_file_tabs(&self, cx: &mut Context<Self>) {
        let shown: Vec<TabId> = self
            .shown_layout()
            .map(|tree| tree.leaves())
            .unwrap_or_default()
            .into_iter()
            .filter_map(|pane| Some(self.model.pane(pane)?.active))
            .collect();
        for (tab, view) in &self.file_tabs {
            let on = shown.contains(tab);
            view.update(cx, |view, cx| view.set_shown(on, cx));
        }
    }

    /// The file tab the focused pane shows, if it shows one.
    pub(crate) fn focused_file_tab(&self) -> Option<&Entity<FileTab>> {
        self.file_tabs.get(&self.model.focused_tab()?)
    }

    /// The file tab pane `pane` shows, if it shows one.
    pub(crate) fn pane_file_tab(&self, pane: PaneId) -> Option<Entity<FileTab>> {
        self.file_tabs.get(&self.model.pane(pane)?.active).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// M11ac: Preview is offered for a Markdown file in File mode only.
    #[test]
    fn preview_shows_for_markdown_in_file_mode_only() {
        assert!(shows_preview(Path::new("/w/README.md"), FileMode::Plain));
        assert!(shows_preview(Path::new("/w/a.MARKDOWN"), FileMode::Plain));
        assert!(!shows_preview(Path::new("/w/README.md"), FileMode::Diff));
        assert!(!shows_preview(Path::new("/w/main.rs"), FileMode::Plain));
    }

    /// M11ac review fix: Preview draws only a coloured text up to 1 MB; a bigger one (or one
    /// too large to colour, 2–20 MB) keeps its source, its button off ("Too large to preview").
    #[test]
    fn preview_fits_a_text_up_to_a_megabyte() {
        let text = |len: usize| State::Text(Doc::new("a".repeat(len), 0));
        assert!(preview_fits(&text(10)));
        assert!(preview_fits(&text(PREVIEW_MAX)));
        assert!(!preview_fits(&text(PREVIEW_MAX + 1)));
        let big = State::PlainTooLarge(Doc::new("a".repeat(10), 0));
        assert!(!preview_fits(&big));
        assert!(!preview_fits(&State::Loading));
        assert!(!preview_fits(&State::Missing));
    }

    /// M11x part 2: each line's width in cells is kept from reading the file (a rewrap on a
    /// resize then costs a pass over the lines, not over the text).
    #[test]
    fn a_doc_keeps_each_line_s_width() {
        let doc = Doc::new("ab\n\tx\n\nhé".to_string(), 0);
        assert_eq!(*doc.widths, [2, 5, 0, 2]);
    }

    /// M11x part 2: each screen row as (line, its first cell); wrapped at `columns`, else one
    /// row a line.
    #[test]
    fn wrapped_lines_take_as_many_rows_as_they_need() {
        assert_eq!(
            visual_rows(&[3, 10, 0], 4, true),
            [(0, 0), (1, 0), (1, 4), (1, 8), (2, 0)]
        );
        assert_eq!(visual_rows(&[3, 10], 4, false), [(0, 0), (1, 0)]);
        // Exactly full: no empty row after it.
        assert_eq!(visual_rows(&[8], 4, true), [(0, 0), (0, 4)]);
        // No room at all is one column.
        assert_eq!(visual_rows(&[2], 0, true), [(0, 0), (0, 1)]);
        // Each line's first row, and after the last line the row count.
        let rows = visual_rows(&[3, 10, 0], 4, true);
        assert_eq!(first_rows(&rows, 3), [0, 1, 4, 5]);
        assert_eq!(first_rows(&[(0, 0), (1, 0)], 2), [0, 1, 2]);
    }

    /// Review fixes (M11x part 2): a row's span is where its first character starts (a tab that
    /// crosses the row's end moves whole to the next row), and a click lands in its own row.
    #[test]
    fn a_wrapped_row_is_drawn_and_hit_from_the_same_start() {
        let doc = Doc::new("abc\tdef".to_string(), 0);
        // Cells: a b c, the tab 3-6, d e f 7-9; five columns.
        assert_eq!(wrapped_row(&doc, 0, 0, 5), (0, 3, 0));
        assert_eq!(wrapped_row(&doc, 0, 5, 5), (3, 7, 3));
        // On the second row, x is from the tab's cell: its left edge is the tab, 4 cells on is d.
        assert_eq!(row_hit(&doc, 0, (3, 7, 3), true, 0.2), 3);
        assert_eq!(row_hit(&doc, 0, (3, 7, 3), true, 4.4), 4);
        // Right of a middle row's last character: that row's end, not into the next row.
        let long = Doc::new("abcdefghij".to_string(), 0);
        assert_eq!(row_hit(&long, 0, (0, 4, 0), false, 9.0), 4);
        assert_eq!(row_hit(&long, 0, (8, 10, 8), true, 9.0), 10);
    }

    /// Review fix (M11x part 2): switching Wrap keeps the same line at the top.
    #[test]
    fn switching_wrap_keeps_the_top_line() {
        let doc = Doc::new("a\nbbbbbbbbbb\nc".to_string(), 0);
        let w = Wrapped::of(&doc, FileMode::Plain, 4);
        // Rows: a | bbbb bbbb bb | c.
        assert_eq!(rescroll_row(4, Some(&w), None), 2);
        assert_eq!(rescroll_row(2, None, Some(&w)), 4);
        // Inside line 1's rows: line 1's first row.
        assert_eq!(rescroll_row(2, Some(&w), None), 1);
        assert_eq!(rescroll_row(1, None, Some(&w)), 1);
    }

    /// Review fix (M11x part 2): with Wrap on, a press finds a change bar by its rows, and a
    /// deleted-lines ▸ within half a row of its edge.
    #[test]
    fn bars_are_hit_by_their_rows_when_wrapped() {
        let doc = Doc::new("a\nbbbbbbbbbb\nc".to_string(), 0);
        let w = Wrapped::of(&doc, FileMode::Plain, 4);
        let lh = 10.0;
        let mut changes = Changes::default();
        changes.by_line.insert(1, LineChange::Changed);
        // Any of line 1's three rows (rows 1-3): line 2 (1-based), changed.
        for y in [12.0, 25.0, 39.0] {
            assert_eq!(
                bar_at_rows(y, lh, &w, &changes),
                Some((2, LineChange::Changed))
            );
        }
        assert_eq!(bar_at_rows(5.0, lh, &w, &changes), None);
        // Lines deleted after line 1 (the edge at row 4): within half a row of it, either side.
        let mut deleted = Changes::default();
        deleted.deleted.push(2);
        for y in [36.0, 44.0] {
            assert_eq!(
                bar_at_rows(y, lh, &w, &deleted),
                Some((2, LineChange::DeletedAfter))
            );
        }
        assert_eq!(bar_at_rows(30.0, lh, &w, &deleted), None);
        assert_eq!(bar_at_rows(-3.0, lh, &w, &deleted), None);
    }
    use alacritty_terminal::vte::ansi::Rgb;

    #[test]
    fn open_in_editor_never_runs_the_file() {
        let path = Path::new("/repo/gradlew");
        let editor = Some(vec![OsString::from("-t"), OsString::from("/repo/gradlew")]);
        let doc = || Doc::new("x\n".into(), 1);
        // Shown (or being read): the default text editor, never the file's own app.
        assert_eq!(open_argv(&State::Loading, path), editor);
        assert_eq!(open_argv(&State::Text(doc()), path), editor);
        assert_eq!(open_argv(&State::PlainTooLarge(doc()), path), editor);
        assert_eq!(open_label(&State::Text(doc())), Some(OPEN_IN_EDITOR));
        // Refused (binary or huge, maybe a program): only shown in the Finder.
        let finder = Some(vec![OsString::from("-R"), OsString::from("/repo/gradlew")]);
        assert_eq!(open_argv(&State::Refuse, path), finder);
        assert_eq!(open_label(&State::Refuse), Some(SHOW_IN_FINDER));
        assert_eq!(SHOW_IN_FINDER, "Show in Finder");
        // Gone: nothing to open.
        assert_eq!(open_argv(&State::Missing, path), None);
        assert_eq!(open_label(&State::Missing), None);
    }

    #[test]
    fn a_failed_git_place_lookup_is_tried_again() {
        let place = GitPlace {
            git: "/usr/bin/git".into(),
            root: "/repo".into(),
            rel: "a.rs".into(),
        };
        let asked = Cell::new(0);
        // No answer (git slow or failing): nothing kept, asked again next time.
        let failed = lookup_place(None, || {
            asked.set(asked.get() + 1);
            None
        });
        assert_eq!(failed, None);
        // Not a repo (git said so): an answer, nothing kept either.
        let no_repo = lookup_place(None, || {
            asked.set(asked.get() + 1);
            Some(None)
        });
        assert_eq!(no_repo, Some(None));
        let found = lookup_place(None, || {
            asked.set(asked.get() + 1);
            Some(Some(place.clone()))
        });
        assert_eq!(found, Some(Some(place.clone())));
        assert_eq!(asked.get(), 3);
        // Found once: kept for the tab's life, git isn't asked where it is again.
        let kept = lookup_place(found.flatten(), || unreachable!("asked again"));
        assert_eq!(kept, Some(Some(place)));
    }

    #[test]
    fn lines_drop_their_endings_and_a_last_newline_ends_the_last_line() {
        let text = "a\nbb\r\n\nc";
        let lines = line_ranges(text);
        let got: Vec<&str> = lines.iter().map(|r| &text[r.clone()]).collect();
        assert_eq!(got, ["a", "bb", "", "c"]);
        assert_eq!(line_ranges("a\n").len(), 1);
        let empty = line_ranges("");
        assert_eq!((empty.len(), empty[0].clone()), (1, 0..0));
    }

    #[test]
    fn a_tab_shows_as_four_spaces_and_columns_map_both_ways() {
        assert_eq!(expand_tabs("\tx\ty"), "    x    y");
        assert_eq!(expand_tabs("plain"), "plain");
        let line = "\tx\ty";
        // Source byte → shown byte: each tab before it adds three.
        assert_eq!(display_col(line, 0), 0);
        assert_eq!(display_col(line, 1), 4);
        assert_eq!(display_col(line, 2), 5);
        assert_eq!(display_col(line, 3), 9);
        assert_eq!(display_col(line, 4), 10);
        // Shown byte → source byte: inside a tab, the nearer of its two edges.
        assert_eq!(source_col(line, 0), 0);
        assert_eq!(source_col(line, 1), 0);
        assert_eq!(source_col(line, 2), 1);
        assert_eq!(source_col(line, 3), 1);
        assert_eq!(source_col(line, 4), 1);
        assert_eq!(source_col(line, 5), 2);
        assert_eq!(source_col(line, 9), 3);
        assert_eq!(source_col(line, 10), 4);
        assert_eq!(source_col(line, 99), 4);
        assert_eq!(source_col("ab", 1), 1);
    }

    #[test]
    fn a_column_past_the_end_of_a_tabbed_line_maps_to_its_end() {
        // A selection kept over a reload can point past a line's end.
        assert_eq!(display_col("\tx", usize::MAX), 5);
        assert_eq!(display_col("", usize::MAX), 0);
    }

    #[test]
    fn find_matches_every_line_ignoring_case_and_steps_around() {
        let text = "fn main() {\n    FN x; fn y\n}\nnothing\n";
        let lines = line_ranges(text);
        let (found, _) = find_matches(text, &lines, "fn", FIND_CAP);
        assert_eq!(
            found,
            [
                Match {
                    line: 0,
                    cols: 0..2
                },
                Match {
                    line: 1,
                    cols: 4..6
                },
                Match {
                    line: 1,
                    cols: 10..12
                },
            ]
        );
        assert!(find_matches(text, &lines, "", FIND_CAP).0.is_empty());
        assert!(find_matches(text, &lines, "zzz", FIND_CAP).0.is_empty());
        // Not across a line's end.
        assert!(find_matches(text, &lines, "{\n", FIND_CAP).0.is_empty());
        // A non-ASCII query, any case.
        let text = "Ärger ärger\n";
        let lines = line_ranges(text);
        assert_eq!(find_matches(text, &lines, "äR", FIND_CAP).0.len(), 2);
        // Next and previous go round.
        assert_eq!(step(None, 3, true), Some(0));
        assert_eq!(step(None, 3, false), Some(2));
        assert_eq!(step(Some(2), 3, true), Some(0));
        assert_eq!(step(Some(0), 3, false), Some(2));
        assert_eq!(step(Some(1), 3, true), Some(2));
        assert_eq!(step(Some(0), 0, true), None);
        // The first at or after a line, else the first of all.
        let text = "fn a\nb\nfn c\n";
        let lines = line_ranges(text);
        let (found, _) = find_matches(text, &lines, "fn", FIND_CAP);
        assert_eq!(first_from(&found, 1), Some(1));
        assert_eq!(first_from(&found, 3), Some(0));
        assert_eq!(first_from(&[], 0), None);
        assert_eq!(find_label("fn", Some(0), 3, false), "1 of 3");
        assert_eq!(find_label("fn", None, 0, false), "No results");
        assert_eq!(find_label("", None, 0, false), "");
    }

    #[test]
    fn a_selection_over_lines_copies_the_ends_partly() {
        let text = "alpha\nbeta\ngamma\n";
        let lines = line_ranges(text);
        let a = Pos { line: 0, col: 2 };
        let b = Pos { line: 2, col: 3 };
        assert_eq!(selection_text(text, &lines, a, b), "pha\nbeta\ngam");
        // Either way round; on one line; empty.
        assert_eq!(selection_text(text, &lines, b, a), "pha\nbeta\ngam");
        let c = Pos { line: 1, col: 1 };
        let d = Pos { line: 1, col: 3 };
        assert_eq!(selection_text(text, &lines, c, d), "et");
        assert_eq!(selection_text(text, &lines, c, c), "");
        // Past a line's end: the line's end (CRLF endings are not copied).
        let text = "ab\r\ncd\r\n";
        let lines = line_ranges(text);
        let e = Pos { line: 0, col: 99 };
        let f = Pos { line: 1, col: 1 };
        assert_eq!(selection_text(text, &lines, e, f), "\nc");
    }

    #[test]
    fn a_highlight_for_an_older_version_is_dropped() {
        let spans = || vec![vec![(0..10, Rgb { r: 1, g: 2, b: 3 })]];
        // Version 3 was asked to be coloured; the text changed on disk, version 4 is loaded and
        // coloured before version 3's colours arrive: those are dropped.
        let asked = Doc::new("fn a() {}\n".into(), 3).version;
        let mut doc = Doc::new("fn b() {}\n".into(), 4);
        assert!(doc.accept_spans(4, spans()));
        assert!(!doc.accept_spans(asked, vec![]));
        assert_eq!(doc.spans_version, Some(4));
        assert_eq!(doc.spans.as_deref().map(Vec::len), Some(1));
    }

    #[test]
    fn asks_while_one_runs_become_one_more_run() {
        let mut work = OneAtATime::default();
        assert!(work.start(), "nothing running: go");
        // Three reloads while it runs: queued, not started.
        assert!(!work.start());
        assert!(!work.start());
        assert!(!work.start());
        // It ends: one more run (for the latest), which starts as usual.
        assert!(work.finish());
        assert!(work.start());
        // That one ends with nothing asked meanwhile: done, and idle again.
        assert!(!work.finish());
        assert!(work.start());
    }

    #[test]
    fn find_stops_at_ten_thousand_and_says_so() {
        let text = "e".repeat(FIND_CAP + 500);
        let lines = line_ranges(&text);
        let (found, capped) = find_matches(&text, &lines, "e", FIND_CAP);
        assert_eq!(found.len(), FIND_CAP);
        assert!(capped);
        assert_eq!(
            find_label("e", Some(0), found.len(), capped),
            "1 of 10,000+"
        );
        let (found, capped) = find_matches("e e e\n", &line_ranges("e e e\n"), "e", FIND_CAP);
        assert_eq!((found.len(), capped), (3, false));
        // Exactly the cap is not "more".
        let text = "e".repeat(FIND_CAP);
        let (found, capped) = find_matches(&text, &line_ranges(&text), "e", FIND_CAP);
        assert_eq!((found.len(), capped), (FIND_CAP, false));
        assert_eq!(find_label("e", Some(9), 1234, false), "10 of 1,234");
    }

    #[test]
    fn a_search_for_an_older_query_is_dropped() {
        let text = "fn a\nfx b\nfn c\n";
        let lines = line_ranges(text);
        let mut finds = Finds::default();
        finds.ask(1, Some("fn".into()));
        // Typed on before "fn"'s search came back.
        finds.ask(2, Some("fx".into()));
        let (old, _) = find_matches(text, &lines, "fn", FIND_CAP);
        assert_eq!(finds.accept(1, old, false, 0), None);
        assert!(finds.matches.is_empty());
        let (new, _) = find_matches(text, &lines, "fx", FIND_CAP);
        // A new query's answer: shown (scrolled to its current match).
        assert_eq!(finds.accept(2, new, false, 0), Some(true));
        assert_eq!(finds.matches.len(), 1);
        assert_eq!(finds.current, Some(0));
        // The text reloads (same query): the current match is kept by number.
        finds.ask(3, None);
        let (again, _) = find_matches("fx\nfx\n", &line_ranges("fx\nfx\n"), "fx", FIND_CAP);
        finds.current = Some(0);
        // A reload's: taken, but not scrolled to.
        assert_eq!(finds.accept(3, again, false, 1), Some(false));
        assert_eq!(finds.current, Some(0));
    }

    #[test]
    fn a_long_line_s_cells_map_like_a_full_scan() {
        // Tabs, two-byte and three-byte characters, long enough for checkpoints.
        let unit = "ab\tcé€d";
        let line = unit.repeat(400);
        let doc = Doc::new(format!("short\n{line}\n"), 1);
        let full = |col: usize| -> usize {
            line[..col]
                .chars()
                .map(|c| if c == '\t' { TAB_WIDTH } else { 1 })
                .sum()
        };
        let cells = full(line.len());
        assert!(cells > LONG_LINE);
        for (col, _) in line.char_indices().chain([(line.len(), ' ')]) {
            assert_eq!(doc.cell_of(1, col), full(col), "byte {col}");
        }
        for cell in 0..=cells + 3 {
            let (b, at) = doc.byte_at_cell(1, cell);
            assert!(line.is_char_boundary(b), "cell {cell}");
            assert_eq!(at, full(b), "cell {cell}");
            assert!(at <= cell.min(cells), "cell {cell}");
            // The next character would start past it.
            let next = line[b..]
                .chars()
                .next()
                .map_or(usize::MAX, |c| at + if c == '\t' { TAB_WIDTH } else { 1 });
            assert!(next > cell || b == line.len(), "cell {cell}");
        }
        // A short line maps the same way, without checkpoints.
        assert_eq!(doc.cell_of(0, 3), 3);
        assert_eq!(doc.byte_at_cell(0, 99), (5, 5));
    }

    #[test]
    fn colours_newer_than_the_ones_held_are_taken() {
        let spans = || vec![vec![(0..1, Rgb { r: 1, g: 2, b: 3 })]];
        // Version 5 is shown with version 1's colours carried over (a fast-growing file).
        let mut doc5 = Doc::new("x\n".into(), 5);
        doc5.spans = Some(Arc::new(spans()));
        doc5.spans_version = Some(1);
        // Version 3's colours: newer than those held, so taken.
        assert!(doc5.accept_spans(3, spans()));
        assert_eq!(doc5.spans_version, Some(3));
        // Version 2's: older than those held now.
        assert!(!doc5.accept_spans(2, spans()));
        // Newer than the text shown can't happen, and is dropped.
        assert!(!doc5.accept_spans(6, spans()));
        assert!(doc5.accept_spans(5, spans()));
        assert_eq!(doc5.spans_version, Some(5));
        // Nothing held yet (it changed faster than its first colouring): any older version's
        // colours are taken, none newer than the text.
        let mut fresh = Doc::new("x\n".into(), 7);
        assert!(!fresh.accept_spans(8, spans()));
        assert!(fresh.accept_spans(2, spans()));
        assert_eq!(fresh.spans_version, Some(2));
        let mut bars = Doc::new("x\n".into(), 7);
        assert!(bars.accept_changes(2, &[(1, LineChange::Added)]));
    }

    #[test]
    fn a_fifo_is_refused_without_blocking() {
        let dir = std::env::temp_dir().join(format!("fuxx-fifo-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let fifo = dir.join("pipe");
        let _ = std::fs::remove_file(&fifo);
        let made = std::process::Command::new("mkfifo").arg(&fifo).status();
        if !made.is_ok_and(|s| s.success()) {
            eprintln!("mkfifo unavailable: skipped");
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let path = fifo.clone();
        std::thread::spawn(move || {
            let refused = matches!(read_file(&path, None), Loaded::Refuse(_));
            let _ = tx.send(refused);
        });
        let got = rx.recv_timeout(Duration::from_secs(3));
        let _ = std::fs::remove_file(&fifo);
        assert_eq!(got, Ok(true), "the FIFO is refused at once");
    }

    #[test]
    fn git_look_ups_are_at_most_one_per_two_seconds() {
        let now = Instant::now();
        assert_eq!(lookup_wait(None, now), None);
        assert_eq!(lookup_wait(Some(now - Duration::from_secs(3)), now), None);
        assert_eq!(lookup_wait(Some(now - Duration::from_secs(2)), now), None);
        assert_eq!(
            lookup_wait(Some(now - Duration::from_millis(500)), now),
            Some(Duration::from_millis(1500))
        );
    }

    #[test]
    fn change_lists_become_bars_by_line_and_deletion_marks_between_lines() {
        let (by_line, deleted) = bars(&[
            (1, LineChange::Added),
            (3, LineChange::Changed),
            (5, LineChange::DeletedAfter),
            (0, LineChange::DeletedAfter),
        ]);
        // 0-based lines.
        assert_eq!(by_line.get(&0), Some(&LineChange::Added));
        assert_eq!(by_line.get(&2), Some(&LineChange::Changed));
        assert_eq!(by_line.len(), 2);
        // Deleted after (1-based) line 5: the edge above 0-based line 5; after 0: the top.
        assert_eq!(deleted, [0, 5]);
    }

    #[test]
    fn columns_lay_out_one_or_two_number_columns() {
        let plain = columns(8.0, Gutter::Plain { digits: 2 });
        assert_eq!(plain.old_right, None);
        assert_eq!(plain.new_right, 28.0);
        assert_eq!(plain.bar_left, 36.0);
        assert_eq!(plain.text_left, 49.0);
        let diff = columns(
            8.0,
            Gutter::Diff {
                old_digits: 3,
                new_digits: 2,
            },
        );
        assert_eq!(diff.old_right, Some(36.0));
        assert_eq!(diff.new_right, 60.0);
        // The sign column's left, one cell wide, then the text.
        assert_eq!(diff.bar_left, 68.0);
        assert_eq!(diff.text_left, 86.0);
    }

    #[test]
    fn the_toggle_shows_in_a_repo_or_in_diff_mode() {
        assert!(!shows_toggle(false, FileMode::Plain));
        assert!(shows_toggle(true, FileMode::Plain));
        assert!(shows_toggle(false, FileMode::Diff));
        assert!(shows_toggle(true, FileMode::Diff));
    }

    #[test]
    fn a_press_on_a_bar_or_a_mark_finds_its_change() {
        let (by_line, deleted) = bars(&[(3, LineChange::Changed), (6, LineChange::DeletedAfter)]);
        let changes = Changes { by_line, deleted };
        let lh = 10.0;
        // 0-based line 2 is (1-based) line 3.
        assert_eq!(bar_at(25.0, lh, &changes), Some((3, LineChange::Changed)));
        assert_eq!(bar_at(20.0, lh, &changes), Some((3, LineChange::Changed)));
        // The ▸ on the edge above 0-based line 6, from either side.
        assert_eq!(
            bar_at(57.0, lh, &changes),
            Some((6, LineChange::DeletedAfter))
        );
        assert_eq!(
            bar_at(63.0, lh, &changes),
            Some((6, LineChange::DeletedAfter))
        );
        // A line without a change, away from the mark.
        assert_eq!(bar_at(5.0, lh, &changes), None);
        assert_eq!(bar_at(45.0, lh, &changes), None);
        assert_eq!(bar_at(-3.0, lh, &changes), None);
    }

    #[test]
    fn a_diff_doc_has_a_line_per_row() {
        // A blank last row is still a line of its own.
        let state = diff_state(
            FileDiff::Unified("@@ -1,2 +1,3 @@\n a\n+b\n \n".into()),
            None,
            4,
            None,
            None,
        );
        let DiffState::Rows(d) = state else {
            panic!("rows");
        };
        assert_eq!(d.rows.len(), 3);
        assert_eq!(d.doc.lines.len(), 3);
        assert_eq!(d.doc.line(2), "");
        assert_eq!(d.doc.version, 4);
        assert!(d.doc.spans.is_none(), "no highlighter: uncoloured");
        // Its gutter, worked out once: the largest old number is 2, the largest new 3, so each
        // column is the 2-digit minimum.
        assert_eq!(
            d.gutter,
            Gutter::Diff {
                old_digits: 2,
                new_digits: 2
            }
        );
        let all = diff_state(FileDiff::AllAdded, Some("x\n\n"), 1, None, None);
        let DiffState::Rows(d) = all else {
            panic!("rows");
        };
        assert_eq!((d.rows.len(), d.doc.lines.len()), (2, 2));
        assert!(matches!(
            diff_state(FileDiff::Unified(String::new()), None, 1, None, None),
            DiffState::Same
        ));
        assert!(matches!(
            diff_state(FileDiff::Binary, Some("x"), 1, None, None),
            DiffState::Binary
        ));
    }

    #[test]
    fn a_diff_answer_for_an_older_disk_version_is_dropped() {
        let rows = |v: u64| diff_state(FileDiff::AllAdded, Some("x\n"), v, None, None);
        let mut held = HeldDiff::default();
        assert!(matches!(held.state, DiffState::Waiting));
        // Asked at version 2; version 3 was read and answered first: 2's answer is dropped.
        assert!(held.accept(3, 3, rows(3)));
        assert!(!held.accept(2, 3, rows(2)));
        assert_eq!(held.version, Some(3));
        // The same version again (a refresh after a commit): taken.
        assert!(held.accept(3, 3, DiffState::Same));
        assert!(matches!(held.state, DiffState::Same));
        // Newer than the text read can't be, and is dropped.
        assert!(!held.accept(4, 3, rows(4)));
    }

    #[test]
    fn a_deleted_file_still_has_a_git_place() {
        let Some(git) = find_git() else {
            return eprintln!("skipped: no git");
        };
        let dir = std::env::temp_dir().join(format!("fuxx-git-place-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src")).unwrap();
        let ok = std::process::Command::new(&git)
            .arg("-C")
            .arg(&dir)
            .args(["init", "-q"])
            .status()
            .is_ok_and(|s| s.success());
        assert!(ok, "git init");
        let file = dir.join("src/gone.rs");
        std::fs::write(&file, "x\n").unwrap();
        let found = git_place(&file)
            .expect("an answer")
            .expect("a file in a repo");
        assert_eq!(found.rel, PathBuf::from("src/gone.rs"));
        // Deleted (git still has its old lines): the same place, so its diff can show them.
        std::fs::remove_file(&file).unwrap();
        assert_eq!(git_place(&file), Some(Some(found.clone())));
        // Its folder gone too (`rm -rf src`): still its place, found from the nearest folder
        // left.
        std::fs::remove_dir_all(dir.join("src")).unwrap();
        assert_eq!(git_place(&file), Some(Some(found)));
        let deep = git_place(&dir.join("nowhere/deeper/x.rs"))
            .expect("an answer")
            .expect("a gone folder in a repo");
        assert_eq!(deep.rel, PathBuf::from("nowhere/deeper/x.rs"));
        // Outside any repo: git says so, for a gone folder too.
        let plain = std::env::temp_dir().join(format!("fuxx-no-repo-{}", std::process::id()));
        std::fs::create_dir_all(&plain).unwrap();
        assert_eq!(git_place(&plain.join("x.rs")), Some(None));
        assert_eq!(git_place(&plain.join("gone/x.rs")), Some(None));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&plain);
    }

    /// `git` in `root` with a throwaway identity; true when it succeeded.
    fn git_ok(git: &Path, root: &Path, args: &[&str]) -> bool {
        std::process::Command::new(git)
            .arg("-C")
            .arg(root)
            .args(["-c", "user.name=t", "-c", "user.email=t@t"])
            .args(["-c", "commit.gpgsign=false"])
            .args(args)
            .status()
            .is_ok_and(|s| s.success())
    }

    #[test]
    fn a_moved_folders_old_files_keep_their_place_and_old_lines() {
        let Some(git) = find_git() else {
            return eprintln!("skipped: no git");
        };
        let dir = std::env::temp_dir().join(format!("fuxx-git-mv-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("old/inner")).unwrap();
        assert!(git_ok(&git, &dir, &["init", "-q"]));
        std::fs::write(dir.join("old/a.rs"), "one\ntwo\n").unwrap();
        std::fs::write(dir.join("old/inner/b.rs"), "three\n").unwrap();
        assert!(git_ok(&git, &dir, &["add", "-A"]));
        assert!(git_ok(&git, &dir, &["commit", "-q", "-m", "first"]));
        assert!(git_ok(&git, &dir, &["mv", "old", "new"]));
        assert!(!dir.join("old").exists());
        for (rel, lines) in [("old/a.rs", 2), ("old/inner/b.rs", 1)] {
            let place = git_place(&dir.join(rel))
                .expect("an answer")
                .expect("a moved file keeps its place");
            assert_eq!(place.rel, PathBuf::from(rel));
            let Some(FileDiff::Unified(u)) =
                file_diff(&place.git, &place.root, &place.rel, context_for(0))
            else {
                panic!("a unified diff for {rel}");
            };
            let rows = crate::diff_view::parse_diff(&u).rows;
            assert_eq!(rows.len(), lines, "{rel}: {u}");
            assert!(rows.iter().all(|r| r.kind == RowKind::Removed), "{rel}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_diff_is_coloured_only_when_both_sides_are_at_most_2_mb() {
        let h = Highlighter::load(&Theme::default());
        let path = Path::new("gone.rs");
        let deleted = |lines: usize| {
            let mut u = format!("@@ -1,{lines} +0,0 @@\n");
            for i in 0..lines {
                u.push_str(&format!("-let value_{i:06} = \"{}\";\n", "x".repeat(40)));
            }
            FileDiff::Unified(u)
        };
        // A small deleted file (no disk text): coloured.
        let DiffState::Rows(small) = diff_state(deleted(10), None, 1, Some((&h, path)), None)
        else {
            panic!("rows");
        };
        assert!(
            small.doc.spans.is_some(),
            "a small deleted file is coloured"
        );
        // Its old side over 2 MB: uncoloured, though there's no disk text to say so.
        let big = deleted(40_000);
        let DiffState::Rows(d) = diff_state(big, None, 1, Some((&h, path)), None) else {
            panic!("rows");
        };
        assert!(
            d.doc.text.len() as u64 > PLAIN_ABOVE,
            "{}",
            d.doc.text.len()
        );
        assert!(
            d.doc.spans.is_none(),
            "an old side over 2 MB is not coloured"
        );
        assert!(!d.colourable);
        assert!(small.colourable);
    }

    #[test]
    fn diff_signs_numbers_and_text_read_on_their_tints() {
        for (name, theme) in crate::theme::every_theme() {
            let bg = theme.background;
            let bars = BarColours::of(&theme);
            let (removed, added) = sign_inks(&bars, bg);
            let faint = theme.ui().text_faint;
            use crate::ui_tokens::TEXT_MIN;
            for (what, bar, sign) in [
                ("removed", bars.deleted, removed),
                ("added", bars.added, added),
            ] {
                // As painted: the bar colour at `TINT` over the background.
                let tint = mix(bg, bar, TINT);
                let c = |ink| crate::ui_tokens::contrast(ink, tint);
                assert!(
                    c(sign) >= MARK_MIN,
                    "{name}: the {what} sign {:.2}",
                    c(sign)
                );
                // The numbers and the text are text: 4.5:1.
                assert!(
                    c(faint) >= TEXT_MIN,
                    "{name}: numbers on {what} {:.2}",
                    c(faint)
                );
                assert!(
                    c(theme.foreground) >= TEXT_MIN,
                    "{name}: text on {what} {:.2}",
                    c(theme.foreground)
                );
            }
        }
    }

    #[test]
    fn a_binary_file_on_disk_shows_the_binary_diff_note() {
        // Refused on disk at most 20 MB: binary (a NUL in its head). Over: too large.
        assert!(matches!(refused_diff(Some(12)), DiffState::Binary));
        assert!(matches!(
            refused_diff(Some(REFUSE_ABOVE)),
            DiffState::Binary
        ));
        assert!(matches!(
            refused_diff(Some(REFUSE_ABOVE + 1)),
            DiffState::Refuse
        ));
        assert!(matches!(refused_diff(None), DiffState::Refuse));
        // In Diff mode a refused file's binary diff says so; in Plain mode it's M11g's note.
        let binary = DiffState::Binary;
        let refused = DiffState::Refuse;
        assert_eq!(
            note_of(&State::Refuse, FileMode::Diff, &binary),
            Some(BINARY)
        );
        assert_eq!(
            note_of(&State::Refuse, FileMode::Diff, &refused),
            Some(REFUSED)
        );
        assert_eq!(
            note_of(&State::Refuse, FileMode::Plain, &binary),
            Some(REFUSED)
        );
    }

    #[test]
    fn an_unchanged_diff_is_not_coloured_again() {
        let h = Highlighter::load(&Theme::default());
        let path = Path::new("a.rs");
        let diff =
            || FileDiff::Unified("@@ -1,2 +1,2 @@\n let a = 1;\n-let b = 2;\n+let b = 3;\n".into());
        let colour = Some((&h, path));
        let DiffState::Rows(first) = diff_state(diff(), None, 1, colour, None) else {
            panic!("rows");
        };
        assert!(first.doc.spans.is_some(), "first rows are coloured");
        let held = ColouredRows {
            text: first.doc.text.clone(),
            rows: first.rows.clone(),
        };
        // The same text and rows again (a static tab's 5 s look-up): no highlighting, no spans.
        let DiffState::Rows(again) = diff_state(diff(), None, 2, colour, Some(&held)) else {
            panic!("rows");
        };
        assert!(
            again.doc.spans.is_none(),
            "an unchanged parse isn't coloured"
        );
        assert_eq!(*again.doc.text, *first.doc.text);
        assert_eq!(*again.rows, *first.rows);
        // Through `diff_answer` too.
        let Some(DiffState::Rows(answer)) =
            diff_answer(None, || Some(diff()), None, 2, colour, Some(&held))
        else {
            panic!("rows");
        };
        assert!(answer.doc.spans.is_none());
        // A changed parse is coloured.
        let changed =
            FileDiff::Unified("@@ -1,2 +1,2 @@\n let a = 1;\n-let b = 2;\n+let b = 4;\n".into());
        let DiffState::Rows(new) = diff_state(changed, None, 3, colour, Some(&held)) else {
            panic!("rows");
        };
        assert!(new.doc.spans.is_some(), "a changed parse is coloured");
    }

    #[test]
    fn an_old_blob_or_a_diff_over_20_mb_is_refused() {
        let rows = || Some(FileDiff::Unified("@@ -1 +1 @@\n-a\n+b\n".into()));
        // The old blob over the limit: refused, git's diff never asked.
        let over = Some(REFUSE_ABOVE + 1);
        let refused = diff_answer(over, || unreachable!("no diff asked"), None, 1, None, None);
        assert!(matches!(refused, Some(DiffState::Refuse)));
        // At the limit, or not in HEAD: the diff.
        assert!(matches!(
            diff_answer(Some(REFUSE_ABOVE), rows, None, 1, None, None),
            Some(DiffState::Rows(_))
        ));
        assert!(matches!(
            diff_answer(None, rows, None, 1, None, None),
            Some(DiffState::Rows(_))
        ));
        // git's answer itself over the limit (the backstop): refused.
        let huge = || {
            let mut u = String::from("@@ -1 +1 @@\n-");
            u.push_str(&"a".repeat(REFUSE_ABOVE as usize));
            Some(FileDiff::Unified(u))
        };
        assert!(matches!(
            diff_answer(None, huge, None, 1, None, None),
            Some(DiffState::Refuse)
        ));
        // No answer from git: none (the rows shown stay).
        assert!(diff_answer(None, || None, None, 1, None, None).is_none());
    }
}
