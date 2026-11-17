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
//! (`columns`). The bar's view switch (File | Diff | Preview, `view_segments`) and a press on a
//! change bar switch modes; each mode keeps its own scroll. In Diff mode the git look-up asks
//! for the diff instead of the bars, on the same schedule, off the UI thread (`look_up_diff`);
//! its answer carries the disk version, as the bars' does (`HeldDiff`). A refused file (over
//! 20 MB or binary) never asks git.
//!
//! **Live changes (M11af).** Each reload that gives new text is compared with the text shown before
//! (`live_lines::compare`, on the background executor with the read, given up after 50 ms; never
//! for a plain-too-large or refused file). The first read is a baseline: nothing is marked. A
//! `LineRecord` keeps, for each line, who last changed it this session (an agent, exact or guessed,
//! or you) and when, and follows its lines when others are added or removed above them. Who is
//! decided by the app-wide edit ledger (`edit_ledger`, fed by agent tabs and by terminals that run a
//! known agent); a late tool finish revises a mark to exact once, within 2 s (one-shot timer, no
//! other), and at that check a change decided while an edit was still open is settled without it
//! (a guess, or yours). A changed line glows in three steps over 60 s and shows its age at the end of the first
//! line of each warm block; levels are worked out on the existing 1 s poll and the tab redraws only
//! when a level or an age label changes. A mark column (a small circle with a letter; hollow when
//! guessed) sits between the line numbers and the change bars, only while a line is marked. File
//! mode only; Diff and Preview look as before. **Follow** (a pill in the bar, off by default,
//! never turns itself on) scrolls the agent's newest change into view, 3 lines from the top as a
//! diff opens; scrolling, a mode switch (Preview on or off too) or cleared marks turn it off.
//!
//! **Folding (M11ah).** In File mode a file with blocks (`code_outline`) has a fold column between
//! the numbers and the marks: ▸ on a folded line, ▾ on an open block's first line while the
//! pointer is over the gutter. A fold hides the block's lines after its first (`hidden_ranges`;
//! the row map leaves them out), and a pill after the folded line's text says how many. A fold
//! never hides a change: the folded line draws its hidden lines' bar (or ▸), warmest glow and
//! newest mark (`folded_summary`). ⌥⌘[ / ⌥⌘] fold and unfold where the current line is; the
//! palette folds or unfolds all. Folds follow their first lines over a reload (one whose first
//! line changed opens), open for a find step or Follow into them, and are not kept over a
//! restart. Folding is you moving the view: it ends Follow.
//!
//! **The sticky line and the breadcrumb (M11ah).** In File mode a file with blocks has a strip
//! between the bar (and the find bar) and the text: the file's name, then the blocks the top line
//! is in, outer to inner (`crumbs`; the outer ones fold into "…" when narrow, `fit_crumbs`). Once
//! a block's first line scrolls above the top, it is pinned over the text's first row (one line,
//! the innermost named block). Both are worked out when the view moves (`refresh_place`: a
//! scroll, a read, the colours landing, a fold or a mode change), never while drawing; a press on
//! either puts its line at the top, current, and ends Follow. Find, Follow and the change jumps
//! land `REVEAL_ABOVE` rows from the top, one more under a sticky line (`reveal_top`).

use std::cell::{Cell, RefCell};
use std::collections::{BTreeSet, HashMap};
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
    DispatchPhase, Entity, EventEmitter, FocusHandle, Focusable, FontWeight, Global,
    HighlightStyle, HitboxBehavior, ListAlignment, ListOffset, ListState, MouseButton,
    MouseDownEvent, MouseExitEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point, ScrollWheelEvent,
    SharedString, StyledText, Subscription, TextAlign, TextRun, WeakEntity, Window, anchored,
    canvas, deferred, div, fill, list, point, prelude::*, px, quad, size,
};

use crate::agent_markdown::Md;
use crate::appearance::Appearance;
use crate::code_outline::{self, Outline};
use crate::diff_view::{
    BINARY, DiffRow, NO_CHANGES, NO_REPO, Outcome, Parsed, RowKind, context_for, diff_outcome,
    diff_spans, dim, first_change, reveal_row, sides,
};
use crate::edit_ledger::{Ledger, SLACK};
use crate::git_branch::{GIT_CAP, find_git, run_capped_outcome, run_git};
use crate::git_status::{FileDiff, LineChange, file_diff, head_size, line_changes, repo_root_at};
use crate::highlight::{Highlighter, PLAIN_ABOVE, REFUSE_ABOVE, Span, Viewable, classify};
use crate::layout_model::{FileMode, PaneId, TabId, TabKind, WorkspaceId};
use crate::live_lines::{
    AgentMark, Compared, LIMIT, LineRecord, Mark, Sure, WARM, Who, age_words, compare, letter_of,
};
use crate::md_preview::{self, LinkTarget, is_markdown, link_target};
use crate::mermaid::{self, Asked, Key};
use crate::mermaid_render::{Drawn, Mermaids, Waiter};
use crate::name_input::{NameInput, NameInputEvent};
use crate::overlay_panel;
use crate::reduce_motion::WindowVisible;
use crate::row_map::{MapKey, Row, RowMap};
use crate::scrollbar::{self, Axis, Bar, Thumb};
use crate::sidebar_toggle::{Tip, tip_box};
use crate::tab_bar::{Glyph, glyph_with};
use crate::tab_color::TabColor;
use crate::terminal_view::{CellMetrics, Copy, hsla};
use crate::theme::{Theme, UiColors, mix};
use crate::ui_tokens::code::{self, BAR_WIDTH, GUTTER_LEFT, NUMBER_GAP, TEXT_GAP, TOP_ROOM};
use crate::ui_tokens::size::CARD_RADIUS;
use crate::ui_tokens::{MARK_MIN, TEXT_MIN, contrast, ensure_contrast, text};
use crate::workspace::Workspace;

// Bound in main.rs in the "FileTab" key context: ⌘F, ⌘G, ⇧⌘G, and Esc to close the find bar.
// M11ah: ⌥⌘[ / ⌥⌘] fold and unfold the block the current line is in; the palette folds or
// unfolds them all.
gpui::actions!(
    file_tab,
    [
        Find, FindNext, FindPrev, CloseFind, FoldHere, UnfoldHere, FoldAll, UnfoldAll
    ]
);

/// The notes the tab shows for a file it can't colour or show.
pub const MISSING: &str = "This file no longer exists";
pub const REFUSED: &str = "This file is too large or binary to show";
pub const TOO_LARGE: &str = "Too large to colour";
const OPEN_IN_EDITOR: &str = "Open in Editor";
/// The bar's view switch (M11h's File | Diff toggle; M11ae adds Preview) and Wrap: their words
/// and tooltips.
const TOGGLE_FILE: &str = "File";
const TOGGLE_DIFF: &str = "Diff";
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
/// The file bar's height (M11ae: its title on two lines); the find bar keeps `TOOLBAR`.
const FILE_TOOLBAR: f32 = 42.0;
const BAR_PAD: f32 = 6.0;
const BUTTON: f32 = 24.0;
const BUTTON_STROKE: f32 = 1.75;
const FIND_BOX: f32 = 260.0;
/// The room below the last line (M11i): as tall as the card's rounded corners, so no line's
/// tint, match or selection reaches them.
const BOTTOM_ROOM: f32 = CARD_RADIUS;
const RIGHT_PAD: f32 = 24.0;
/// M11af: the column of who-changed-what marks, between the numbers and the bar; a mark's
/// circle and its letter in it; the age words' size and their gap after the line's end (as the
/// mockup's).
const MARK_COLUMN: f32 = 16.0;
const MARK_DOT: f32 = 13.0;
const MARK_LETTER: f32 = 8.0;
const AGE_SIZE: f32 = 10.0;
const AGE_GAP: f32 = 10.0;
/// A filled mark's dark letter (the mockup's), when it reads better than white.
const ON_MARK_DARK: Rgb = Rgb {
    r: 0x1e,
    g: 0x1f,
    b: 0x27,
};
/// M11af: the Follow pill's fill when on: the warm accent at this opacity (the mockup's).
const FOLLOW_TINT: f32 = 0.15;
/// A changed line's tint: its bar's colour at this opacity.
const TINT: f32 = 0.12;
/// M11af: the glow's warm accent (the mockup's peach), and its opacity at each level (1, 2, 3;
/// note 82's starting values).
pub const WARM_RGB: Rgb = Rgb {
    r: 0xfa,
    g: 0xb3,
    b: 0x87,
};
pub const GLOW: [f32; 3] = [0.05, 0.11, 0.20];
/// Find's matches: the accent at these opacities (the current one stronger).
const MATCH_TINT: f32 = 0.25;
const CURRENT_TINT: f32 = 0.55;
/// The deleted-lines ▸, larger than the text so it reads at a glance.
const MARKER_SCALE: f32 = 1.3;
/// A diff opens (and a change bar's click shows its change) this many rows from the top; Follow
/// (M11af) puts the agent's change there too.
const REVEAL_ABOVE: usize = 3;
/// A diff row's sign: removed (U+2212) and added.
/// M11ah: between the breadcrumb's parts, and in place of the outer ones that do not fit (and
/// at the end of a sticky line cut by Wrap).
const CRUMB_SEPARATOR: &str = " \u{203a} ";
const ELLIPSIS: &str = "\u{2026}";
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

/// The header chip's text: `M · +3 −1` for a changed file, `A · +12` for a new one; None when
/// nothing changed or git has not answered.
pub fn chip_label(counts: Option<(usize, usize, bool)>) -> Option<String> {
    let (added, removed, new) = counts?;
    if added == 0 && removed == 0 {
        return None;
    }
    let mut out = String::from(if new { "A \u{b7}" } else { "M \u{b7}" });
    if added > 0 {
        out.push_str(&format!(" +{added}"));
    }
    if removed > 0 {
        out.push_str(&format!(" \u{2212}{removed}"));
    }
    Some(out)
}

/// Where the file lives, for the header: the folders from the repo's name down (`repo / a / b`),
/// else the folder shortened under `home` (`~/Downloads`), else the folder. `repo` is
/// `(root, rel)` as in `GitPlace`.
pub fn where_label(path: &Path, repo: Option<(&Path, &Path)>, home: Option<&Path>) -> String {
    if let Some((root, rel)) = repo {
        let mut parts: Vec<String> = root
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .into_iter()
            .collect();
        if let Some(dir) = rel.parent() {
            parts.extend(
                dir.components()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned()),
            );
        }
        return parts.join(" / ");
    }
    let dir = path.parent().unwrap_or(path);
    if let Some(home) = home
        && let Ok(under) = dir.strip_prefix(home)
    {
        return if under.as_os_str().is_empty() {
            "~".to_string()
        } else {
            format!("~/{}", under.display())
        };
    }
    dir.display().to_string()
}

/// `1 line`, `42 lines`, `12,345 lines`.
pub fn lines_label(n: usize) -> String {
    format!("{} line{}", thousands(n), if n == 1 { "" } else { "s" })
}

/// How long ago the file was modified: `edited just now`, `edited 5 min ago`, `1 h`, `1 d`,
/// `1 mo` (30 days), `1 y` (365 days). A time ahead of `now` is just now.
pub fn age_label(modified: SystemTime, now: SystemTime) -> String {
    let secs = now.duration_since(modified).map_or(0, |d| d.as_secs());
    match age_units(secs) {
        None => "edited just now".to_string(),
        Some((n, unit)) => format!("edited {n} {unit} ago"),
    }
}

/// An age of a minute or more in its largest unit: `(5, "min")`, `(1, "h")`, `(1, "d")`,
/// `(1, "mo")` (30 days), `(1, "y")` (365 days); none under a minute.
fn age_units(secs: u64) -> Option<(u64, &'static str)> {
    Some(match secs {
        0..60 => return None,
        60..3_600 => (secs / 60, "min"),
        3_600..86_400 => (secs / 3_600, "h"),
        86_400..2_592_000 => (secs / 86_400, "d"),
        2_592_000..31_536_000 => (secs / 2_592_000, "mo"),
        _ => (secs / 31_536_000, "y"),
    })
}

/// `n` with commas between thousands.
pub(crate) fn thousands(n: usize) -> String {
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

/// Runs of neighbouring shown rows with the same change, in row order: (rows, change).
pub fn bar_runs(changes: &[(usize, LineChange)]) -> Vec<(Range<usize>, LineChange)> {
    let mut runs: Vec<(Range<usize>, LineChange)> = Vec::new();
    for &(row, change) in changes {
        match runs.last_mut() {
            Some((rows, kind)) if rows.end == row && *kind == change => rows.end = row + 1,
            _ => runs.push((row..row + 1, change)),
        }
    }
    runs
}

/// The pixel span of a run drawn at `top..bottom`: shortened by half of `code::BAR_RUN_GAP` at an
/// end that touches a run of another kind.
pub fn bar_span(top: f32, bottom: f32, gap_above: bool, gap_below: bool) -> (f32, f32) {
    let half = code::BAR_RUN_GAP / 2.0;
    (
        if gap_above { top + half } else { top },
        if gap_below { bottom - half } else { bottom },
    )
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
    /// The blocks (M11ah), made with the read, named once the colours land (it keeps the named
    /// lines, which a reload carries as the colours). None: gave up past `code_outline::LIMIT`,
    /// or not made (a diff's rows). Never for a refused file (no `Doc`).
    pub outline: Option<Arc<Outline>>,
    pub changes: Arc<Changes>,
    /// The version the change bars were made for (an older one's carried over a reload).
    pub changes_version: Option<u64>,
    /// (added, removed, new) lines against HEAD, from the same answer as the bars, or from
    /// Diff mode's rows (once git answered).
    pub counts: Option<(usize, usize, bool)>,
    /// The syntax's name, once the colours are made (`Highlighter::syntax_name`).
    pub syntax: Option<Arc<str>>,
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
            outline: None,
            changes: Arc::default(),
            changes_version: None,
            counts: None,
            syntax: None,
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
    pub fn accept_spans(&mut self, version: u64, spans: Vec<Vec<Span>>, named: &[usize]) -> bool {
        if !fresher(version, self.version, self.spans_version) {
            return false;
        }
        self.spans = Some(Arc::new(spans));
        self.spans_version = Some(version);
        if let Some(outline) = &mut self.outline {
            Arc::make_mut(outline).name(named);
        }
        true
    }

    /// Takes what `old` (the text shown before a reload) knew, until the new answers land: the
    /// colours and the type (not for a `plain` text), the change bars and their counts.
    fn carry_over(&mut self, old: &Doc, plain: bool) {
        if !plain {
            self.spans = old.spans.clone();
            self.spans_version = old.spans_version;
            if let (Some(new), Some(old)) = (&mut self.outline, &old.outline) {
                Arc::make_mut(new).name(old.named_lines());
            }
            self.syntax = old.syntax.clone();
        }
        self.changes = old.changes.clone();
        self.changes_version = old.changes_version;
        self.counts = old.counts;
    }

    /// Change bars and counts asked for `version`, taken as colours are (`accept_spans`). True
    /// when they changed.
    pub fn accept_changes(
        &mut self,
        version: u64,
        changes: &[(usize, LineChange)],
        counts: Option<(usize, usize, bool)>,
    ) -> bool {
        if !fresher(version, self.version, self.changes_version) {
            return false;
        }
        self.changes_version = Some(version);
        let (by_line, deleted) = bars(changes);
        let changes = Changes { by_line, deleted };
        let counts_changed = self.set_counts(counts);
        if *self.changes == changes {
            return counts_changed;
        }
        self.changes = Arc::new(changes);
        true
    }

    /// The header chip's counts (the bars' answer, or Diff mode's rows: `DiffDoc::counts`),
    /// taken by the caller's version check (`accept_changes`'s, or the held diff's). True when
    /// they changed.
    pub fn set_counts(&mut self, counts: Option<(usize, usize, bool)>) -> bool {
        std::mem::replace(&mut self.counts, counts) != counts
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
    /// Untracked, or no commits yet: git said the whole file is added (`FileDiff::AllAdded`).
    /// For the header chip's `A` (M11ae).
    pub new: bool,
}

impl DiffDoc {
    /// (added, removed, new) for the header chip (M11ae): the Added and Removed rows, exact as
    /// `git diff --numstat` counts them (not the hunk headers': with `-U<n>` they include the
    /// context).
    pub fn counts(&self) -> (usize, usize, bool) {
        let (added, removed) = self.rows.iter().fold((0, 0), |(a, r), row| match row.kind {
            RowKind::Added => (a + 1, r),
            RowKind::Removed => (a, r + 1),
            RowKind::Same | RowKind::Gap => (a, r),
        });
        (added, removed, self.new)
    }
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
/// taking its side's colours (blocking: the background executor's); and each side's named lines.
struct ColouredDiff {
    spans: Vec<Vec<Span>>,
    /// The old side's named lines (ascending); the cards' titles read them (Task 9).
    #[allow(dead_code)]
    old_named: Vec<usize>,
    /// The new side's named lines (ascending).
    #[allow(dead_code)]
    new_named: Vec<usize>,
}

fn colour_rows(
    h: &Highlighter,
    path: &Path,
    rows: &[DiffRow],
    (old, new): &(String, String),
) -> ColouredDiff {
    let (old, new) = (h.colour(path, old), h.colour(path, new));
    ColouredDiff {
        spans: diff_spans(rows, &old.spans, &new.spans),
        old_named: old.named,
        new_named: new.named,
    }
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
    let new = matches!(diff, FileDiff::AllAdded);
    match diff_outcome(diff, disk_text) {
        Outcome::Same => DiffState::Same,
        Outcome::Binary => DiffState::Binary,
        Outcome::Rows(parsed) => {
            let mut doc = Doc::new(format!("{}\n", parsed.text), version);
            // M11ah: the rows' own outline, for the guides (made here, off the UI thread).
            doc.outline = code_outline::outline(
                &line_slices(&doc.text, &doc.lines),
                false,
                code_outline::LIMIT,
            )
            .map(Arc::new);
            let both = sides(&parsed);
            let colourable = colourable(&both, doc.longest);
            // The rows held, coloured, again: not highlighted again (`accept_diff` keeps them).
            let unchanged = held.is_some_and(|h| *h.text == *doc.text && *h.rows == parsed.rows);
            if let Some((h, path)) = colour
                && colourable
                && !unchanged
            {
                doc.spans = Some(Arc::new(colour_rows(h, path, &parsed.rows, &both).spans));
                doc.spans_version = Some(version);
            }
            DiffState::Rows(DiffDoc {
                doc,
                gutter: Gutter::diff(&parsed.rows),
                rows: Arc::new(parsed.rows),
                colourable,
                new,
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

/// The text a file tab showed last, sent to the next read to be compared with (M11af).
#[derive(Clone)]
struct Prev {
    text: Arc<str>,
    lines: Arc<Vec<Range<usize>>>,
}

/// The text's lines without their endings, as `live_lines::compare` takes them: a CRLF text and
/// its LF twin, or a missing last newline, are no change.
fn line_slices<'a>(text: &'a str, lines: &[Range<usize>]) -> Vec<&'a str> {
    lines
        .iter()
        .map(|r| {
            text.get(r.clone())
                .unwrap_or("")
                .trim_end_matches(['\r', '\n'])
        })
        .collect()
}

/// The lines changed since `prev` (blocking: the background read's). None for the first read,
/// a text shown `plain` (too large to colour), or a compare that took longer than `limit`.
fn compare_with(prev: Option<&Prev>, doc: &Doc, plain: bool, limit: Duration) -> Option<Compared> {
    if plain {
        return None;
    }
    let prev = prev?;
    let old = line_slices(&prev.text, &prev.lines);
    let new = line_slices(&doc.text, &doc.lines);
    compare(&old, &new, limit)
}

/// When a change was made, as its marks say: when fuxx saw it, unless the file's time is earlier
/// (it changed while the tab was hidden).
fn change_time(seen: SystemTime, modified: Option<SystemTime>) -> SystemTime {
    modified.filter(|m| *m < seen).unwrap_or(seen)
}

/// The letter on your marks: git's `user.name` for the file's repo, else the account's full
/// name, else `Y`.
fn your_letter(git_name: Option<&str>, account_name: Option<&str>) -> char {
    [git_name, account_name]
        .into_iter()
        .flatten()
        .map(letter_of)
        .find(|c| *c != '?')
        .unwrap_or('Y')
}

/// Your letter (blocking): `git config user.name` with the tab's git and repo, else `id -F`.
fn find_your_letter(place: Option<GitPlace>) -> char {
    let git = place.and_then(|p| run_git(&p.git, &p.root, &["config", "user.name"]));
    let account = || {
        let mut id = std::process::Command::new("/usr/bin/id");
        id.arg("-F");
        run_capped_outcome(id, None, GIT_CAP).success()
    };
    let account = if git.as_deref().is_some_and(|n| letter_of(n) != '?') {
        None
    } else {
        account()
    };
    your_letter(git.as_deref(), account.as_deref())
}

/// Who changed the file at `path` since `since` (the edit ledger's rule). A scene without a
/// ledger: you.
fn who_changed(
    cx: &App,
    path: &Path,
    ws: Option<WorkspaceId>,
    since: Instant,
    now: Instant,
) -> (Who, Sure, bool) {
    cx.try_global::<Ledger>()
        .map_or((Who::You, Sure::Exact, false), |ledger| {
            ledger.decide(path, ws, since, now)
        })
}

/// What a look at the disk means for the live record (M11af).
enum Seen<'a> {
    Unchanged,
    Missing,
    Refused,
    Text {
        doc: &'a Doc,
        /// Against the last text (`compare_with`); None starts the record again.
        compared: Option<Compared>,
        when: SystemTime,
    },
}

/// Follow's scroll (M11af): the first row to show so `line` sits `REVEAL_ABOVE` rows from the
/// top (M11ah: one more under a sticky line, `reveal_top`), none when `line` is already in
/// `view` (rows; M11ah: not the row under a sticky line).
fn follow_scroll(
    view: Range<usize>,
    line: usize,
    sticky_at: impl Fn(usize) -> bool,
) -> Option<usize> {
    (!view.contains(&line)).then(|| reveal_top(line, sticky_at))
}

/// A file tab's live changes (M11af): who changed each line since the tab opened, and what the
/// next read needs to keep that record.
#[derive(Default)]
struct Live {
    /// Shared with the frame drawing it (a change copies it while a frame still holds it).
    record: Rc<LineRecord>,
    /// Whether any line is marked, kept with each change (`render` asks it every frame; the
    /// record would be scanned).
    marked: bool,
    /// The runs glowing as of the last poll (`refresh`), with their words: what is drawn.
    warm: Rc<Vec<Warm>>,
    /// The latest change's time: once it has cooled and nothing glows, the poll skips the
    /// glow (none: nothing was ever marked).
    latest: Option<SystemTime>,
    /// The last text read (kept while the file is missing): the next read compares with it.
    last: Option<Prev>,
    /// When the disk was last looked at: a change found next was made since then.
    read_at: Option<Instant>,
    /// The tab's workspace (the workspace says so each frame): its working agents are guessed.
    workspace: Option<WorkspaceId>,
    /// The letter on your marks, once found (`find_your_letter`, asked once per tab).
    your_letter: Option<char>,
    letter_asked: bool,
    /// Follow the agent's newest change (M11af part 4). Off until the pill is clicked.
    follow: bool,
    /// The newest agent mark's name, kept with each change: what the pill says (none: no
    /// agent has changed the file, no pill).
    agent: Option<Arc<str>>,
    /// The revision check runs (one at a time), and the look whose decision it may revise
    /// (or, decided while an edit was open, settle).
    revising: bool,
    pending: Option<Pending>,
    /// The file's canonical path, as the edit ledger keeps paths (found with each text read).
    real: Option<PathBuf>,
    /// The time the glow is worked out at: a scene's own (frozen tabs never poll).
    #[cfg(feature = "ui-shots")]
    now: Option<SystemTime>,
}

/// A look the revision check waits for (M11af): when it was, the look before it, and whether
/// an agent's edit to the file was still open when it was decided.
#[derive(Clone, Copy, Debug)]
struct Pending {
    at: Instant,
    since: Instant,
    open: bool,
}

impl Live {
    /// What the record holds, kept: whether any line is marked and the newest agent's name.
    /// Cleared marks turn Follow off.
    fn sync_marks(&mut self) {
        self.marked = self.record.any();
        self.agent = self.record.newest_agent().map(|a| a.name.clone());
        if !self.marked {
            self.follow = false;
        }
    }

    /// The pill's agent: the newest agent mark's name.
    fn agent_name(&self) -> Option<Arc<str>> {
        self.agent.clone()
    }

    /// With Follow on, the line to bring into view after a reload: the first line of the latest
    /// change's first block, when an agent made it (your changes never move the view).
    fn follow_line(&self) -> Option<usize> {
        if !self.follow {
            return None;
        }
        let line = self.record.last_blocks().first()?.start;
        matches!(self.record.mark(line)?.who, Who::Agent(_)).then_some(line)
    }

    /// The last text, for the next read to compare with.
    fn prev(&self) -> Option<Prev> {
        self.last.clone()
    }

    /// The glow at `now` (the poll's, each second): true when a run's level or words changed,
    /// so the tab is drawn again.
    fn refresh(&mut self, now: SystemTime) -> bool {
        let cold = self
            .latest
            .is_none_or(|t| now.duration_since(t).is_ok_and(|age| age >= WARM));
        if cold && self.warm.is_empty() {
            return false;
        }
        let warm = warm_runs(&self.record, now);
        if warm == *self.warm {
            return false;
        }
        self.warm = Rc::new(warm);
        true
    }

    /// A look at the disk at `at`. A new text's changed lines are marked as `decide` says (given
    /// the previous look's time); a missing file keeps the record and its last text; a refused
    /// file, or a text with nothing to compare (the first read, plain, or given up), starts the
    /// record again. True when the decision may be revised by a late finish: an edit still open,
    /// a guess, or yours.
    fn saw(
        &mut self,
        seen: Seen,
        at: Instant,
        decide: impl FnOnce(Instant) -> (Who, Sure, bool),
    ) -> bool {
        let since = self.read_at.replace(at).unwrap_or(at);
        let (doc, compared, when) = match seen {
            Seen::Unchanged | Seen::Missing => return false,
            Seen::Refused => {
                Rc::make_mut(&mut self.record).clear(0);
                self.sync_marks();
                self.last = None;
                self.pending = None;
                return false;
            }
            Seen::Text {
                doc,
                compared,
                when,
            } => (doc, compared, when),
        };
        self.last = Some(Prev {
            text: doc.text.clone(),
            lines: doc.lines.clone(),
        });
        let lines = doc.lines.len();
        let Some(c) = compared else {
            Rc::make_mut(&mut self.record).clear(lines);
            self.sync_marks();
            self.pending = None;
            return false;
        };
        let (who, sure, open) = if c.blocks.is_empty() {
            (Who::You, Sure::Exact, false)
        } else {
            decide(since)
        };
        let revisable = !c.blocks.is_empty() && (open || sure == Sure::Guess || who == Who::You);
        Rc::make_mut(&mut self.record).apply(&c, lines, Mark { who, sure, when });
        self.sync_marks();
        if !c.blocks.is_empty() {
            self.latest = Some(self.latest.map_or(when, |t| t.max(when)));
        }
        self.pending = revisable.then_some(Pending { at, since, open });
        revisable
    }

    /// A change decided while an agent's edit was open, settled (`Ledger::settle`): the latest
    /// change's lines become `who`, `sure`, which may be less sure than they were. True when a
    /// mark changed.
    fn settle(&mut self, (who, sure): (Who, Sure)) -> bool {
        let record = &self.record;
        let changed = record
            .last_blocks()
            .iter()
            .flat_map(Clone::clone)
            .any(|line| {
                record
                    .mark(line)
                    .is_some_and(|m| m.who != who || m.sure != sure)
            });
        if changed {
            Rc::make_mut(&mut self.record).revise_last(who, sure);
        }
        changed
    }

    /// A late finish (`Ledger::revise`): the latest change's lines become that agent's, exact.
    /// True when a mark changed.
    fn revise(&mut self, found: Option<AgentMark>) -> bool {
        found.is_some_and(|agent| self.settle((Who::Agent(agent), Sure::Exact)))
    }
}

/// A run of neighbouring changed lines glowing at one level (`LineRecord::warm_blocks`), and
/// the words said at its first line's end (`age_words`, from that line's change).
#[derive(Clone, Debug, PartialEq)]
struct Warm {
    lines: Range<usize>,
    level: u8,
    words: String,
}

/// The record's warm runs at `now`.
fn warm_runs(record: &LineRecord, now: SystemTime) -> Vec<Warm> {
    record
        .warm_blocks(now)
        .into_iter()
        .map(|(lines, level)| {
            let when = record.mark(lines.start).map_or(now, |m| m.when);
            let words = age_words(now.duration_since(when).unwrap_or_default());
            Warm {
                lines,
                level,
                words,
            }
        })
        .collect()
}

/// A mark's tooltip at `now`: "claude · 12 s ago", "probably claude · 12 s ago (it was
/// working)", "you · 3 min ago".
fn mark_tip_words(m: &Mark, now: SystemTime) -> String {
    let name = match &m.who {
        Who::Agent(a) => &*a.name,
        Who::You => "you",
    };
    let age = now.duration_since(m.when).unwrap_or_default();
    // Under a minute as the glow says it; then as the bar's "edited …" does.
    let ago = match age_units(age.as_secs()) {
        None => age_words(age),
        Some((n, unit)) => format!("{n} {unit} ago"),
    };
    match m.sure {
        Sure::Exact => format!("{name} \u{b7} {ago}"),
        Sure::Guess => format!("probably {name} \u{b7} {ago} (it was working)"),
    }
}

/// What a look at the disk found.
enum Loaded {
    /// The same size and time as the text shown.
    Unchanged,
    Missing,
    Refuse(Stamp),
    Text {
        stamp: Stamp,
        /// Boxed: the other answers are small.
        doc: Box<Doc>,
        plain: bool,
        /// M11af: the lines changed since the text shown before (`compare_with`).
        compared: Option<Compared>,
        /// The file's canonical path, as the edit ledger keeps paths.
        real: Option<PathBuf>,
    },
}

/// Read `path` (blocking: the background executor's) unless its size and time are still
/// `known`; a new text is compared with `prev`, the text shown before (M11af).
fn read_file(path: &Path, known: Option<Stamp>, prev: Option<Prev>) -> Loaded {
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
    let mut doc = Doc::new(text, 0);
    let plain = viewable == Viewable::PlainTooLarge || doc.longest > COLOUR_LINE_MAX;
    let compared = compare_with(prev.as_ref(), &doc, plain, LIMIT);
    // M11ah: the blocks, for every text (a plain-too-large one too: indentation only).
    doc.outline = code_outline::outline(
        &line_slices(&doc.text, &doc.lines),
        is_markdown(path),
        code_outline::LIMIT,
    )
    .map(Arc::new);
    let real = std::fs::canonicalize(path).ok();
    Loaded::Text {
        stamp,
        doc: Box::new(doc),
        plain,
        compared,
        real,
    }
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

/// The top row after the rows are made again (`from` → `to`; Wrap switched; `from` none: a row
/// a line): the row of the same line, so the line at the top stays at the top (a hidden line:
/// the shown line above it).
pub fn rescroll_row(top: usize, from: Option<&RowMap>, to: &RowMap) -> usize {
    let line = match from.and_then(|m| m.row(top)) {
        Some(Row::Line { line, .. }) => line,
        // Not a line's row (a Diff card's title): the row kept.
        Some(_) => return top.min(to.len().saturating_sub(1)),
        None => top,
    };
    (0..=line).rev().find_map(|l| to.first_row(l)).unwrap_or(0)
}

/// The change under a press `y` points down the text (scroll included) in the bar column, by
/// `map`'s rows: on any of a changed line's rows, (its 1-based line, its change); else within
/// half a row of an edge where lines were deleted, that ▸.
pub fn bar_at_rows(y: f32, map: &RowMap, changes: &Changes) -> Option<(usize, LineChange)> {
    if y < 0.0 || map.len() == 0 {
        return None;
    }
    let r = map.row_at(y);
    if y < map.height()
        && let Some(Row::Line { line, .. }) = map.row(r)
    {
        if let Some(&change) = changes.by_line.get(&line) {
            return Some((line + 1, change));
        }
        // M11ah: a folded line's bar (or ▸) is its hidden lines': the first of their changes.
        let hidden = map.hidden_after(line);
        let changed = hidden
            .clone()
            .find_map(|l| Some((l + 1, *changes.by_line.get(&l)?)));
        let at = changes.deleted.partition_point(|&e| e < hidden.start);
        let edge = changes
            .deleted
            .get(at)
            .filter(|&&e| hidden.contains(&e))
            .map(|&e| (e, LineChange::DeletedAfter));
        // The one higher up: a changed (1-based) line c sits below the edge above line c - 1,
        // so an edge e < c comes first.
        match (changed, edge) {
            (Some(c), Some(e)) => return Some(if e.0 < c.0 { e } else { c }),
            (Some(found), None) | (None, Some(found)) => return Some(found),
            (None, None) => {}
        }
    }
    // The edge nearest `y`: row `r`'s top or bottom (none further than half a row below the end).
    let (top, bottom) = (map.top(r), map.top(r + 1));
    let half = (bottom - top) / 2.0;
    let edge_row = if y < top + half {
        r
    } else if y < bottom + half {
        r + 1
    } else {
        return None;
    };
    let edge = changes
        .deleted
        .iter()
        .copied()
        .find(|&e| map.first_row(e) == Some(edge_row))?;
    Some((edge, LineChange::DeletedAfter))
}

/// The gutter's number columns: the file's line numbers, or a diff's old and new numbers (each
/// as many digits as its largest number, at least 2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gutter {
    /// A file's: its numbers, (M11ah) a fold column for a file with blocks, and (M11af) a
    /// column for who changed each line while any is marked.
    Plain {
        digits: usize,
        marks: bool,
        folds: bool,
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
    /// The gutter for `count` lines of a file, with the marks' and the fold column or not.
    fn plain(count: usize, marks: bool, folds: bool) -> Gutter {
        Gutter::Plain {
            digits: digits(count),
            marks,
            folds,
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

/// A row's height (M11ah): the font's line height times `code::ROW_SCALE`; the text is painted
/// with it, so GPUI centres the line in its row.
pub fn row_height(line_height: Pixels) -> Pixels {
    line_height * code::ROW_SCALE
}

/// A line number's colour: the current line's in `text`; the others quiet, `text_dim` held to
/// `MARK_MIN` on the background so they stay readable.
fn number_ink(ui: &UiColors, bg: Rgb, current: bool) -> Rgb {
    if current {
        ui.text
    } else {
        ensure_contrast(ui.text_dim, bg, MARK_MIN)
    }
}

/// Where the current line `line` is after a reload: its new place when the reload's line
/// comparison (`new_of_old`) says where it went, none when that line is gone; with no
/// comparison, kept when `same` says the line there is unchanged.
fn carry_line(
    new_of_old: Option<&[Option<usize>]>,
    line: usize,
    same: impl Fn(usize) -> bool,
) -> Option<usize> {
    match new_of_old {
        Some(map) => map.get(line).copied().flatten(),
        None => same(line).then_some(line),
    }
}

/// A folded line's pill (M11ah): `⋯ 24 lines`.
fn fold_pill_words(hidden: usize) -> String {
    format!("\u{22ef} {}", lines_label(hidden))
}

/// Where a folded line's pill sits (x from the body's left, its width): `code::FOLD_PILL_GAP`
/// after the line's `width` cells drawn from `text_x`, its words (`hidden` lines) in cells with
/// `code::FOLD_PILL_PAD` either side. Painting and pressing use it, so they agree.
fn fold_pill_span(text_x: Pixels, cell: Pixels, width: usize, hidden: usize) -> (Pixels, Pixels) {
    let words = fold_pill_words(hidden).chars().count();
    let x0 = text_x + cell * width as f32 + px(code::FOLD_PILL_GAP);
    (x0, cell * words as f32 + px(2.0 * code::FOLD_PILL_PAD))
}

/// Two changes on one bar: the same one, else Changed.
fn either_change(a: Option<LineChange>, b: Option<LineChange>) -> Option<LineChange> {
    match (a, b) {
        (Some(a), Some(b)) if a != b => Some(LineChange::Changed),
        (a, b) => a.or(b),
    }
}

/// The newest mark on `lines`, and its line.
fn newest_mark(record: &LineRecord, lines: Range<usize>) -> Option<(usize, &Mark)> {
    lines.filter_map(|l| Some((l, record.mark(l)?))).fold(
        None,
        |best: Option<(usize, &Mark)>, (l, m)| match best {
            Some((_, b)) if b.when >= m.when => best,
            _ => Some((l, m)),
        },
    )
}

/// What a folded line shows for the lines it hides (M11ah: a fold never hides a change).
#[derive(Clone, Debug, Default, PartialEq)]
struct FoldSummary {
    change: Option<LineChange>,
    glow: Option<u8>,
    mark: Option<Mark>,
}

/// What a folded line shows for its `hidden` lines: the strongest change (all Added: Added; all
/// deletions: DeletedAfter; else Changed), the warmest glow level, the newest mark.
fn folded_summary(
    hidden: Range<usize>,
    changes: &Changes,
    live: Option<&LiveFrame>,
) -> FoldSummary {
    // The hidden lines' changes: by line or by change, whichever list is shorter.
    let mut change = None;
    if hidden.len() <= changes.by_line.len() {
        for l in hidden.clone() {
            change = either_change(change, changes.by_line.get(&l).copied());
        }
    } else {
        for (l, c) in &changes.by_line {
            if hidden.contains(l) {
                change = either_change(change, Some(*c));
            }
        }
    }
    // Lines deleted between two of the fold's lines (the edge above a hidden line).
    let at = changes.deleted.partition_point(|&e| e < hidden.start);
    if changes.deleted.get(at).is_some_and(|e| hidden.contains(e)) {
        change = either_change(change, Some(LineChange::DeletedAfter));
    }
    let Some(live) = live else {
        return FoldSummary {
            change,
            ..FoldSummary::default()
        };
    };
    let from = live.warm.partition_point(|w| w.lines.end <= hidden.start);
    let glow = live.warm[from..]
        .iter()
        .take_while(|w| w.lines.start < hidden.end)
        .map(|w| w.level)
        .max();
    let mark = newest_mark(&live.record, hidden).map(|(_, m)| m.clone());
    FoldSummary { change, glow, mark }
}

/// The folds after a reload: each follows its first line as the current line does
/// (`carry_line`); one whose first line changed or is gone, or where the new outline has no
/// block, opens.
fn carry_folds(
    folded: &BTreeSet<usize>,
    new_of_old: Option<&[Option<usize>]>,
    same: impl Fn(usize) -> bool,
    outline: &Outline,
) -> BTreeSet<usize> {
    folded
        .iter()
        .filter_map(|&start| carry_line(new_of_old, start, &same))
        .filter(|&line| outline.starting_at(line).is_some())
        .collect()
}

/// The current line after a Find step: the line of the match stepped to; a step that found
/// no match (`stepped` none: no matches) keeps `kept`.
fn step_line(matches: &[Match], stepped: Option<usize>, kept: Option<usize>) -> Option<usize> {
    stepped
        .and_then(|k| matches.get(k))
        .map_or(kept, |m| Some(m.line))
}

/// M11ah: where the view is (File mode), worked out when it moves (a scroll, a read, the colours
/// landing, a fold or a mode change), never while drawing: the sticky line and the breadcrumb's
/// parts.
#[derive(Clone, Debug, Default, PartialEq)]
struct Place {
    sticky: Option<Sticky>,
    crumbs: Vec<Crumb>,
}

/// What is pinned at the top of the text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Sticky {
    /// A block's first line (File mode).
    Line(usize),
    /// A Diff card's title row (by card).
    #[allow(dead_code)] // pinned by Diff mode's cards (M11ah Task 9)
    Title(usize),
}

/// What the breadcrumb's parts were laid out for (its layout cache's key): the parts, the room,
/// the font and its size, and the inks (the parts', the last part's).
#[derive(Clone, Debug, PartialEq)]
struct CrumbFitKey {
    crumbs: Vec<Crumb>,
    room: f32,
    font: gpui::Font,
    font_size: Pixels,
    inks: (Rgb, Rgb),
}

/// What a piece of the breadcrumb is: a part (a press takes it), the "…" of the outer parts
/// folded, or a " › " between two.
#[derive(Clone, Debug)]
enum CrumbPiece {
    Part(Crumb),
    Folded,
    Separator,
}

/// The breadcrumb laid out: each piece's x from the strip's text edge, shaped.
type CrumbsLaid = Vec<(Pixels, gpui::ShapedLine, CrumbPiece)>;

/// The breadcrumb's parts and "…" as drawn (window points), each with its crumb (none: the "…",
/// which a press does not take).
type CrumbsDrawn = Vec<(Bounds<Pixels>, Option<Crumb>)>;

/// A breadcrumb part: its words, and the line a press goes to (none: the file's name, the top).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Crumb {
    pub words: String,
    pub line: Option<usize>,
}

/// The breadcrumb for `top` the line at the top of the text: the file's name, then each block
/// naming where `top` is, outer to inner (`Outline::path`), in their first lines' words
/// (`code_outline::part_words`). `line` gives a line's text (only the path's are read).
pub fn crumbs<'a>(
    file_name: &str,
    outline: &Outline,
    line: impl Fn(usize) -> &'a str,
    top: usize,
    markdown: bool,
) -> Vec<Crumb> {
    let file = Crumb {
        words: file_name.to_string(),
        line: None,
    };
    let parts = outline.path(top).into_iter().map(|b| {
        let start = outline.blocks[b].start;
        Crumb {
            words: code_outline::part_words(line(start), markdown),
            line: Some(start),
        }
    });
    std::iter::once(file).chain(parts).collect()
}

/// Which crumbs fit `room` points (widths measured): outer ones fold into one "…" first; the last
/// always stays. Each part shown in order (none: the "…").
pub fn fit_crumbs(widths: &[f32], room: f32, ellipsis: f32, separator: f32) -> Vec<Option<usize>> {
    let n = widths.len();
    // `k` outer parts folded into the "…" (none for k = 0).
    for k in 0..n {
        let shown = &widths[k..];
        let mut width: f32 = shown.iter().sum();
        let mut parts = shown.len();
        if k > 0 {
            width += ellipsis;
            parts += 1;
        }
        width += separator * parts.saturating_sub(1) as f32;
        if width <= room {
            let fold = (k > 0).then_some(None);
            return fold.into_iter().chain((k..n).map(Some)).collect();
        }
    }
    n.checked_sub(1).map(Some).into_iter().collect()
}

/// The first row to show so `target_row` sits `REVEAL_ABOVE` rows down, one more when a sticky
/// line shows there (`sticky_at`: with that row at the top).
fn reveal_top(target_row: usize, sticky_at: impl Fn(usize) -> bool) -> usize {
    let top = target_row.saturating_sub(REVEAL_ABOVE);
    if top > 0 && sticky_at(top) {
        top - 1
    } else {
        top
    }
}

/// The place shown with `top` the line at the text's top and `under` the line just under the
/// sticky line's row (the sticky line pins the innermost named block holding `under` whose
/// first line is above it: `Outline::sticky`; at that block's own first line, none).
fn place_of<'a>(
    file_name: &str,
    outline: &Outline,
    line: impl Fn(usize) -> &'a str,
    (top, under): (usize, usize),
    markdown: bool,
) -> Place {
    Place {
        sticky: outline
            .sticky(under)
            .map(|b| Sticky::Line(outline.blocks[b].start)),
        crumbs: crumbs(file_name, outline, line, top, markdown),
    }
}

/// Where the gutter's parts sit, in points from the body's left edge: the old numbers' right
/// edge (a diff's), the (new) numbers' right edge, the change bar's left edge (a diff's sign
/// column), the text's left edge.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Columns {
    pub old_right: Option<f32>,
    /// M11ah: the fold column's left edge (a file's with blocks, in File mode).
    pub fold_left: Option<f32>,
    /// The marks' column's left edge (M11af; a file's, while any line is marked).
    pub marks_left: Option<f32>,
    pub new_right: f32,
    pub bar_left: f32,
    pub text_left: f32,
}

/// The gutter laid out for cells `cell` wide: `GUTTER_LEFT`, the numbers (old, `NUMBER_GAP`,
/// new, for a diff), `NUMBER_GAP` (a file's fold column, `code::FOLD_COLUMN`, for a file with
/// blocks, then its marks' column, `MARK_COLUMN`, while any line is marked, opened in its
/// middle), the bar (`BAR_WIDTH`; a diff's sign, one cell), `TEXT_GAP`.
pub fn columns(cell: f32, gutter: Gutter) -> Columns {
    let (mut marks_left, mut fold_left) = (None, None);
    let (old_right, new_right, mark) = match gutter {
        Gutter::Plain {
            digits,
            marks,
            folds,
        } => {
            let right = GUTTER_LEFT + cell * digits as f32;
            // M11ah: the fold column, half a gap from the numbers; then (M11af) the marks'
            // column; then half a gap to the bar.
            fold_left = folds.then_some(right + NUMBER_GAP / 2.0);
            let fold = if folds { code::FOLD_COLUMN } else { 0.0 };
            marks_left = marks.then_some(right + NUMBER_GAP / 2.0 + fold);
            (None, right, BAR_WIDTH)
        }
        Gutter::Diff {
            old_digits,
            new_digits,
        } => {
            let old = GUTTER_LEFT + cell * old_digits as f32;
            (Some(old), old + NUMBER_GAP + cell * new_digits as f32, cell)
        }
    };
    let marks = if marks_left.is_some() {
        MARK_COLUMN
    } else {
        0.0
    };
    let folds = if fold_left.is_some() {
        code::FOLD_COLUMN
    } else {
        0.0
    };
    let bar_left = new_right + NUMBER_GAP + folds + marks;
    Columns {
        old_right,
        fold_left,
        marks_left,
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

/// Whether the view switch offers Diff: for a file in a repo, and always in Diff mode (so it can
/// go back).
pub fn shows_toggle(in_repo: bool, mode: FileMode) -> bool {
    in_repo || mode == FileMode::Diff
}

/// A segment of the bar's view switch (M11ae): the file, its diff, or the Markdown rendered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View {
    File,
    Diff,
    Preview,
}

impl View {
    /// Its place in `FileTab::toggle_bounds`.
    fn index(self) -> usize {
        self as usize
    }
}

/// The switch's segments, in order: File always, Diff where `shows_toggle`, Preview for a
/// `markdown` file (in either mode). The switch shows when it has two or more.
pub fn view_segments(in_repo: bool, mode: FileMode, markdown: bool) -> Vec<View> {
    let mut views = vec![View::File];
    if shows_toggle(in_repo, mode) {
        views.push(View::Diff);
    }
    if markdown {
        views.push(View::Preview);
    }
    views
}

/// The switch's current segment: Diff in Diff mode; Preview in File mode with Preview on and
/// fitting (`preview_on_and_fits`); else File.
pub fn current_view(mode: FileMode, preview_on_and_fits: bool) -> View {
    match mode {
        FileMode::Diff => View::Diff,
        FileMode::Plain if preview_on_and_fits => View::Preview,
        FileMode::Plain => View::File,
    }
}

/// Where the text body's parts sit, as last drawn (for the mouse, the wheel and find).
#[derive(Clone, Copy, Debug)]
pub struct Geom {
    pub bounds: Bounds<Pixels>,
    /// A row's height (`row_height`: the font's line height times `code::ROW_SCALE`).
    pub line_height: Pixels,
    pub cell_width: Pixels,
    /// From the body's left edge: a diff's old numbers' right edge, the (new) line numbers'
    /// right edge, the change bar's left edge (a diff's sign column), the text's left edge
    /// (before the sideways scroll).
    pub old_numbers_right: Option<Pixels>,
    pub numbers_right: Pixels,
    /// M11ah: the fold column's left edge (none: not a file's with blocks).
    pub fold_left: Option<Pixels>,
    /// M11af: the marks' column's left edge (none: no line is marked, or not a file's).
    pub marks_left: Option<Pixels>,
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
        row: Pixels,
        height: Pixels,
        gutter: Gutter,
        widest: usize,
        scroll: Point<Pixels>,
    ) -> Geom {
        let c = columns(f32::from(m.cell_width), gutter);
        let text_left = px(c.text_left);
        let max_scroll = point(
            (text_left + m.cell_width * widest as f32 + px(RIGHT_PAD) - bounds.size.width)
                .max(px(0.0)),
            (height - bounds.size.height).max(px(0.0)),
        );
        Geom {
            bounds,
            line_height: row,
            cell_width: m.cell_width,
            old_numbers_right: c.old_right.map(px),
            numbers_right: px(c.new_right),
            fold_left: c.fold_left.map(px),
            marks_left: c.marks_left.map(px),
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

/// A scrollbar's track in the text as last drawn (M11ae): its area (the text's height at its
/// right edge; the text's width right of the numbers, along its bottom), the view's and the
/// text's length along it, and the thumb.
#[derive(Clone, Copy, Debug)]
struct Track {
    axis: Axis,
    area: Bounds<Pixels>,
    viewport: f32,
    content: f32,
    thumb: Thumb,
}

impl Track {
    /// None when the text fits along `axis` (always sideways with Wrap on: no sideways scroll).
    fn of(g: &Geom, axis: Axis) -> Option<Track> {
        let (area, viewport, max, offset) = match axis {
            Axis::Vertical => (g.bounds, g.bounds.size.height, g.max_scroll.y, g.scroll.y),
            Axis::Horizontal => {
                let left = point(g.bounds.left() + g.text_left, g.bounds.top());
                let area = Bounds::from_corners(left, g.bounds.bottom_right());
                (area, area.size.width, g.max_scroll.x, g.scroll.x)
            }
        };
        // The text's length along it: its rows × the line height, or (Wrap off) its widest line
        // × the cell width + `RIGHT_PAD` (`Geom::of`'s `max_scroll` is that less the view).
        let (viewport, content) = (f32::from(viewport), f32::from(viewport + max));
        let thumb = scrollbar::thumb(viewport, content, f32::from(offset))?;
        Some(Track {
            axis,
            area,
            viewport,
            content,
            thumb,
        })
    }

    /// The Preview's: the list's `area` (its viewport), the reader's `content` height and its
    /// scroll `offset` down. None when it all fits.
    fn of_list(area: Bounds<Pixels>, content: f32, offset: f32) -> Option<Track> {
        let viewport = f32::from(area.size.height);
        let thumb = scrollbar::thumb(viewport, content, offset)?;
        Some(Track {
            axis: Axis::Vertical,
            area,
            viewport,
            content,
            thumb,
        })
    }

    /// Window point `p` along the track, from its start.
    fn along(&self, p: Point<Pixels>) -> f32 {
        f32::from(match self.axis {
            Axis::Vertical => p.y - self.area.top(),
            Axis::Horizontal => p.x - self.area.left(),
        })
    }

    /// Whether `p` is within `scrollbar::ZONE` of the text's right (or bottom) edge.
    fn zone_has(&self, p: Point<Pixels>) -> bool {
        self.area.contains(&p)
            && match self.axis {
                Axis::Vertical => p.x >= self.area.right() - px(scrollbar::ZONE),
                Axis::Horizontal => p.y >= self.area.bottom() - px(scrollbar::ZONE),
            }
    }

    /// Scroll `from` with its part along the track where the thumb starts at `start`.
    fn scroll_for(&self, start: f32, from: Point<Pixels>) -> Point<Pixels> {
        let to = px(scrollbar::offset_for(start, self.viewport, self.content));
        match self.axis {
            Axis::Vertical => point(from.x, to),
            Axis::Horizontal => point(to, from.y),
        }
    }
}

/// The track whose edge zone has window point `p` (the right edge's first), as last drawn.
fn zone_at(g: &Geom, p: Point<Pixels>) -> Option<Track> {
    [Axis::Vertical, Axis::Horizontal]
        .into_iter()
        .filter_map(|axis| Track::of(g, axis))
        .find(|t| t.zone_has(p))
}

/// M11ae: while selecting with the pointer out of the text, the text scrolls on its own this often.
const AUTOSCROLL_STEP: Duration = Duration::from_millis(16);

/// How far point `p` is out of the span `lo..hi`: after it, `p - hi` (positive); before it,
/// `p - lo` (negative); within it, 0.
pub fn outside(p: f32, lo: f32, hi: f32) -> f32 {
    if p > hi {
        p - hi
    } else if p < lo {
        p - lo
    } else {
        0.0
    }
}

/// How far the text scrolls in one auto-scroll step for a pointer `outside` points out of it
/// (M11ae): faster the further out, from 1 pt to 3 lines; nothing within it.
pub fn edge_speed(outside: f32, line_height: f32) -> f32 {
    if outside <= 0.0 {
        return 0.0;
    }
    (outside / 8.0).clamp(1.0, 3.0 * line_height)
}

/// The auto-scroll's step for window point `p` (M11ae): per axis, towards the side the pointer
/// is out of the text, at `edge_speed`. Down the text: its canvas (the scrollbar's edge zones in
/// it); across it, only with Wrap off (Wrap on has no sideways scroll): from where the text
/// starts, right of the numbers (over them is left of the text). Zero: the pointer is in the
/// text.
fn edge_push(p: Point<Pixels>, g: &Geom, wrap: bool) -> Point<Pixels> {
    let lh = f32::from(g.line_height);
    let push = |o: f32| px(o.signum() * edge_speed(o.abs(), lh));
    let b = g.bounds;
    let y = outside(f32::from(p.y), f32::from(b.top()), f32::from(b.bottom()));
    let x = if wrap {
        0.0
    } else {
        let left = b.left() + g.text_left;
        outside(f32::from(p.x), f32::from(left), f32::from(b.right()))
    };
    point(push(x), push(y))
}

/// Window point `p` held inside the text (M11ae): its edge row above or below it, and with Wrap
/// off its edge column left or right of it (the text's, right of the numbers), so a selection's
/// moving end never goes out of sight.
fn hold_inside(p: Point<Pixels>, g: &Geom, wrap: bool) -> Point<Pixels> {
    let b = g.bounds;
    let y = p.y.clamp(b.top(), (b.bottom() - px(1.0)).max(b.top()));
    let x = if wrap {
        p.x
    } else {
        let left = b.left() + g.text_left;
        p.x.clamp(left, (b.right() - px(1.0)).max(left))
    };
    point(x, y)
}

/// A Preview of up to this many blocks measures every row at its first layout (and again after a
/// reset, a remeasure or a width change), so its scrollbar is exact. Measured on a release build
/// (M11ae fix round 1): a full relayout is ~3 ms for 585 blocks (20 KB) but ~160 ms for 29,115
/// (1 MB), about 5.5 us a block; 2,000 blocks keeps it near 11 ms, under a frame.
pub const PREVIEW_MEASURE_ALL: usize = 2_000;

/// A block's height as the scrollbar of a longer Preview reckons it (the mean of a mixed 1 MB
/// file's blocks: 45 pt). GPUI's list drops its height hints at its first layout and at each
/// width change, and a list with unmeasured rows reports only the drawn rows' height, so for such
/// a Preview the track is counted in blocks (`preview_track`), not in the list's pixels.
const PREVIEW_BLOCK_HINT: f32 = 45.0;

/// The Preview's list for `count` blocks: all measured at once when few (`PREVIEW_MEASURE_ALL`),
/// else each row measured as it is drawn. Each of its own scrolls (the wheel) shows the
/// scrollbar: the handler runs from the wheel's event, never from drawing, and only notes it and
/// redraws.
fn preview_list_of(count: usize, view: WeakEntity<FileTab>) -> ListState {
    let state = ListState::new(count, ListAlignment::Top, px(PREVIEW_OVERDRAW));
    let state = if count <= PREVIEW_MEASURE_ALL {
        state.measure_all()
    } else {
        state
    };
    state.set_scroll_handler(move |_, _, cx| {
        view.update(cx, |this, cx| {
            this.scrolled(cx);
            cx.notify();
        })
        .ok();
    });
    state
}

/// The Preview's scrollbar track as the list was last laid out (M11ae), read from its state:
/// `scroll_px_offset_for_scrollbar` has a negative `y`, and the content is the list's own
/// `max_offset_for_scrollbar` (fixed during a thumb drag) plus the viewport.
///
/// A longer Preview (rows not all measured) is counted in blocks of `PREVIEW_BLOCK_HINT`: its
/// content is the block count, its offset the top block (and the share of it scrolled past).
fn preview_track(list: &ListState) -> Option<Track> {
    let area = list.viewport_bounds();
    let count = list.item_count();
    if count > PREVIEW_MEASURE_ALL {
        let top = list.logical_scroll_top();
        let into = (f32::from(top.offset_in_item) / PREVIEW_BLOCK_HINT).min(0.999);
        let offset = (top.item_ix as f32 + into) * PREVIEW_BLOCK_HINT;
        return Track::of_list(area, count as f32 * PREVIEW_BLOCK_HINT, offset);
    }
    let content = f32::from(list.max_offset_for_scrollbar().y + area.size.height);
    let offset = -f32::from(list.scroll_px_offset_for_scrollbar().y);
    Track::of_list(area, content, offset)
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

/// A diff row's look as the file tab draws it (M11aa: the agent tab's edit diffs share it): the
/// fill of a removed and an added row (its bar colour at `TINT` over `background`), and their
/// signs' inks on those fills.
#[derive(Clone, Copy, Debug)]
pub(crate) struct DiffRowColours {
    pub removed_fill: Rgb,
    pub added_fill: Rgb,
    pub removed_ink: Rgb,
    pub added_ink: Rgb,
}

pub(crate) fn diff_row_colours(theme: &Theme, background: Rgb) -> DiffRowColours {
    let bars = BarColours::of(theme);
    let (removed_ink, added_ink) = sign_inks(&bars, background);
    DiffRowColours {
        removed_fill: mix(background, bars.deleted, TINT),
        added_fill: mix(background, bars.added, TINT),
        removed_ink,
        added_ink,
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
    /// M11ah: the current line (a file's line, or a diff's row), drawn with a band.
    current_line: Option<usize>,
    /// M11ah: whether indent guides are drawn (not in a Markdown file: its outline is headings).
    guides: bool,
    theme: Theme,
    /// M11af: the live changes drawn (a file in File mode with a line marked).
    live: Option<LiveFrame>,
    /// M11ah: the folded blocks (File mode) and their generation (the row map's key), and
    /// whether the pointer is over the gutter (the open blocks' ▾ show).
    folded: BTreeSet<usize>,
    folds_gen: u64,
    gutter_hover: bool,
    /// M11ah: the line pinned at the top of the text (File mode), as the place says.
    sticky: Option<usize>,
}

/// What the text body draws of the live changes (M11af): the marks, and the runs glowing as
/// of the last poll with their words.
struct LiveFrame {
    record: Rc<LineRecord>,
    warm: Rc<Vec<Warm>>,
    your_letter: char,
}

impl LiveFrame {
    /// The warm run line `i` is in, if any.
    fn run_of(&self, i: usize) -> Option<&Warm> {
        let k = self.warm.partition_point(|w| w.lines.end <= i);
        self.warm.get(k).filter(|w| w.lines.contains(&i))
    }
}

/// A mark's look: its colour, whether it is filled (else drawn hollow, in that colour), and
/// its letter. An agent's in its provider's colour (filled when we know, hollow when we guess);
/// an agent without one grey and hollow; yours teal and filled. Colours stand out on the
/// background as marks do.
fn mark_look(m: &Mark, your_letter: char, ui: &UiColors, bg: Rgb) -> (Rgb, bool, char) {
    match &m.who {
        Who::You => (ui.tab(TabColor::Teal), true, your_letter),
        Who::Agent(a) => match a.rgb {
            Some(c) => {
                let c = Rgb {
                    r: (c >> 16) as u8,
                    g: (c >> 8) as u8,
                    b: c as u8,
                };
                let c = ensure_contrast(c, bg, MARK_MIN);
                (c, m.sure == Sure::Exact, a.letter)
            }
            None => (ui.tab(TabColor::Grey), false, a.letter),
        },
    }
}

/// A line's mark (the line's top at `top`) in the marks' column at `left`: a circle with its
/// letter, as `mark_look` says.
fn paint_mark(
    look: (Rgb, bool, char),
    left: Pixels,
    top: Pixels,
    lh: Pixels,
    window: &mut Window,
    cx: &mut App,
) {
    let (colour, filled, letter) = look;
    let dot = px(MARK_DOT);
    let centre = point(left + px(MARK_COLUMN / 2.0), top + lh / 2.0);
    let b = Bounds::new(
        point(centre.x - dot / 2.0, centre.y - dot / 2.0),
        size(dot, dot),
    );
    let ink = if filled {
        let white = Rgb {
            r: 255,
            g: 255,
            b: 255,
        };
        window.paint_quad(fill(b, hsla(colour)).corner_radii(dot / 2.0));
        if contrast(white, colour) >= contrast(ON_MARK_DARK, colour) {
            white
        } else {
            ON_MARK_DARK
        }
    } else {
        let ring = quad(
            b,
            dot / 2.0,
            gpui::transparent_black(),
            px(1.0),
            hsla(colour),
            gpui::BorderStyle::Solid,
        );
        window.paint_quad(ring);
        colour
    };
    let font = gpui::Font {
        weight: FontWeight::BOLD,
        ..window.text_style().font()
    };
    let label = SharedString::from(letter.to_string());
    let run = TextRun {
        len: label.len(),
        font,
        color: hsla(ink),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let shaped = window
        .text_system()
        .shape_line(label, px(MARK_LETTER), &[run], None);
    let at = point(centre.x - shaped.width() / 2.0, top);
    let _ = shaped.paint(at, lh, TextAlign::Left, None, window, cx);
}

/// A number in the gutter, right-aligned at `right` (a point at its line's top).
fn paint_number(
    n: usize,
    right: Point<Pixels>,
    m: &CellMetrics,
    row: Pixels,
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
    let _ = number.paint(at, row, TextAlign::Left, None, window, cx);
}

/// The folded lines in sight (M11ah): for each, how many lines it hides and what they hold.
type Folds = HashMap<usize, (usize, FoldSummary)>;

/// The change bars of the rows in sight (a file's; a diff has signs): runs of neighbouring rows
/// of one change are one bar, `BAR_WIDTH` wide with round ends, a `BAR_RUN_GAP` between runs of
/// different changes. A run that goes on past the view's edge keeps a square end there. With Wrap
/// a run spans every screen row of its lines.
fn paint_bars(
    f: &Frame,
    g: &Geom,
    map: &RowMap,
    rows: &Range<usize>,
    folds: &Folds,
    colours: &BarColours,
    window: &mut Window,
) {
    if f.mode != FileMode::Plain || f.doc.changes.by_line.is_empty() {
        return;
    }
    // One row either side of the view, to tell a cut run and a touching one from a free end.
    let (from, to) = (rows.start.saturating_sub(1), (rows.end + 1).min(map.len()));
    let shown: Vec<(usize, LineChange)> = (from..to)
        .filter_map(|r| match map.row(r)? {
            Row::Line { line, .. } => {
                // M11ah: a folded line's bar is its own and its hidden lines' (deletions alone
                // are its ▸).
                let own = f.doc.changes.by_line.get(&line).copied();
                let hidden = folds.get(&line).and_then(|(_, s)| s.change);
                let change = either_change(own, hidden).filter(|c| *c != LineChange::DeletedAfter);
                Some((r, change?))
            }
            _ => None,
        })
        .collect();
    let runs = bar_runs(&shown);
    let top = f32::from(g.bounds.top() - g.scroll.y);
    let x = g.bounds.left() + g.bar_left;
    for (k, (run, change)) in runs.iter().enumerate() {
        let touches = |other: Option<&(Range<usize>, LineChange)>, at: usize| {
            other.is_some_and(|(rows, _)| rows.contains(&at))
        };
        let gap_above =
            run.start > 0 && touches(k.checked_sub(1).and_then(|p| runs.get(p)), run.start - 1);
        let gap_below = touches(runs.get(k + 1), run.end);
        let (y0, y1) = bar_span(
            top + map.top(run.start),
            top + map.top(run.end),
            gap_above,
            gap_below,
        );
        // Cut by the view's edge (the run goes on to the extra row): square there.
        let radius = px(BAR_WIDTH / 2.0);
        let (cut_top, cut_bottom) = (run.start < rows.start, run.end > rows.end);
        let corners = gpui::Corners {
            top_left: if cut_top { px(0.0) } else { radius },
            top_right: if cut_top { px(0.0) } else { radius },
            bottom_right: if cut_bottom { px(0.0) } else { radius },
            bottom_left: if cut_bottom { px(0.0) } else { radius },
        };
        let bar = Bounds::new(point(x, px(y0)), size(px(BAR_WIDTH), px(y1 - y0)));
        window.paint_quad(fill(bar, hsla(colours.of_change(*change))).corner_radii(corners));
    }
}

/// Paint the text body: `f`'s lines in sight, laid out as `g`. A file: its numbers, change bars
/// and tints, the deleted-lines ▸. A diff: each row's old and new numbers, its sign and tint
/// (removed red, added green), unchanged rows' colours dimmed (`diff_view::dim`).
fn paint_body(
    f: &Frame,
    g: &Geom,
    m: &CellMetrics,
    map: &RowMap,
    window: &mut Window,
    cx: &mut App,
) {
    let ui = f.theme.ui();
    let bars = BarColours::of(&f.theme);
    let bounds = g.bounds;
    let lh = g.line_height;
    // The screen rows in sight (M11ah: the row map's), each a line or (Wrap) a part of one.
    let scroll_y = f32::from(g.scroll.y);
    let rows = map.rows_in(scroll_y..scroll_y + f32::from(bounds.size.height));
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
    let (quiet, bright) = (number_ink(&ui, bg, false), number_ink(&ui, bg, true));
    let ink_for = |i: usize| {
        if f.current_line == Some(i) {
            bright
        } else {
            quiet
        }
    };
    let (removed_ink, added_ink) = sign_inks(&bars, bg);
    // M11af: the age words in the warm accent, readable on the background.
    let age_ink = ensure_contrast(WARM_RGB, bg, TEXT_MIN);
    let (removed_sign, added_sign) = (
        sign(SIGN_REMOVED, removed_ink, window),
        sign(SIGN_ADDED, added_ink, window),
    );
    // M11ah: indent guides (a file with blocks), the current line's block drawn stronger.
    let outline = f
        .doc
        .outline
        .as_deref()
        .filter(|o| f.guides && o.has_structure());
    let active = outline.and_then(|o| {
        let block = *o.containing(f.current_line?).last()?;
        Some((
            o.blocks[block].start..o.blocks[block].end,
            o.blocks[block].indent / o.step.max(1),
        ))
    });
    let guide = hsla(ui.text).opacity(code::GUIDE_ALPHA);
    let guide_active = hsla(ui.text).opacity(code::GUIDE_ACTIVE_ALPHA);
    // M11ah: the folded lines in sight (one row either side, for the bars' ends), and the fold
    // column's ▸ (folded, always) and ▾ (open, faint, while the pointer is over the gutter).
    let mut folds = Folds::new();
    if f.mode == FileMode::Plain && !f.folded.is_empty() {
        let around = rows.start.saturating_sub(1)..(rows.end + 1).min(map.len());
        for r in around {
            let Some(Row::Line { line, .. }) = map.row(r) else {
                continue;
            };
            let hidden = map.hidden_after(line);
            if !hidden.is_empty() && !folds.contains_key(&line) {
                let summary = folded_summary(hidden.clone(), &f.doc.changes, f.live.as_ref());
                folds.insert(line, (hidden.len(), summary));
            }
        }
    }
    let fold_outline = f
        .doc
        .outline
        .as_deref()
        .filter(|o| g.fold_left.is_some() && o.has_structure());
    let (fold_shut, fold_open) = (
        sign("\u{25b8}", quiet, window),
        sign("\u{25be}", mix(bg, quiet, code::FOLD_OPEN_ALPHA), window),
    );
    paint_bars(f, g, map, &rows, &folds, &bars, window);
    for r in rows.clone() {
        let Some(Row::Line { line: i, cell0 }) = map.row(r) else {
            continue;
        };
        let y = bounds.top() + px(map.top(r)) - g.scroll.y;
        let line = f.doc.line(i);
        // A wrapped row: its line's part, drawn from the text's left edge; the line's first row
        // has its number, bar and sign, its last row the selection's line end.
        let first_row = cell0 == 0;
        let part = map.columns().map(|columns| {
            let (from, to, start) = wrapped_row(&f.doc, i, cell0, columns);
            let last = !matches!(map.row(r + 1), Some(Row::Line { line, .. }) if line == i);
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
        // The current line's band, first: under the tints, the glow, the selection and matches.
        if f.current_line == Some(i) {
            let band = Bounds::new(point(bounds.left(), y), size(bounds.size.width, lh));
            window.paint_quad(fill(band, hsla(ui.text).opacity(code::CURRENT_ALPHA)));
        }
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
        // M11af: a line changed on disk in the last minute glows, under the selection and find.
        let run = f.live.as_ref().and_then(|l| l.run_of(i));
        // M11ah: a folded line glows as its warmest hidden line, if warmer.
        let fold = folds.get(&i);
        let level = run.map(|w| w.level).max(fold.and_then(|(_, s)| s.glow));
        let glow = level.and_then(|l| l.checked_sub(1).and_then(|k| GLOW.get(usize::from(k))));
        if let Some(level) = glow {
            let band = Bounds::new(point(bounds.left(), y), size(bounds.size.width, lh));
            window.paint_quad(fill(band, hsla(WARM_RGB).opacity(*level)));
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
            // Indent guides, under the selection, the matches and the text: a 1 pt line at each
            // of the line's levels (the same cells on every row of a wrapped line).
            if let Some(o) = outline {
                let origin =
                    text_x + g.cell_width * part.map_or(0, |(_, _, start, _)| start) as f32;
                for k in 0..o.levels(i) {
                    let on = active
                        .as_ref()
                        .is_some_and(|(span, level)| *level == k && span.contains(&i));
                    let x = origin + g.cell_width * (k * o.step) as f32;
                    let line = Bounds::new(point(x, y), size(px(code::GUIDE), lh));
                    window.paint_quad(fill(line, if on { guide_active } else { guide }));
                }
            }
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
            let ends_here = part.is_none_or(|(_, _, _, last)| last);
            let width = f.doc.widths.get(i).copied().unwrap_or(0);
            let mut after_text = text_x + g.cell_width * width as f32;
            // M11ah: a folded line's pill, after its text (on the row it ends on).
            if let Some((hidden, _)) = fold.filter(|_| ends_here) {
                let (x0, w) = fold_pill_span(text_x - bounds.left(), g.cell_width, width, *hidden);
                let inset = px(code::FOLD_PILL_INSET);
                let pill = Bounds::new(
                    point(bounds.left() + x0, y + inset),
                    size(w, lh - inset * 2.0),
                );
                let ground = hsla(ui.text).opacity(code::FOLD_PILL_ALPHA);
                window.paint_quad(fill(pill, ground).corner_radii((lh - inset * 2.0) / 2.0));
                let label = fold_pill_words(*hidden);
                let words = window.text_system().shape_line(
                    SharedString::from(label.clone()),
                    m.font_size,
                    &[plain_run(label.len(), m, quiet)],
                    None,
                );
                let at = point(bounds.left() + x0 + px(code::FOLD_PILL_PAD), y);
                let _ = words.paint(at, lh, TextAlign::Left, None, window, cx);
                after_text = bounds.left() + x0 + w;
            }
            // M11af: a warm run's age, after its first line's end (on the row it ends on).
            if let Some(w) = run.filter(|w| w.lines.start == i && ends_here) {
                let x = after_text + px(AGE_GAP);
                let label = SharedString::from(w.words.clone());
                let run = TextRun {
                    len: label.len(),
                    font: window.text_style().font(),
                    color: hsla(age_ink),
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                };
                let words = window
                    .text_system()
                    .shape_line(label, px(AGE_SIZE), &[run], None);
                let _ = words.paint(point(x, y), lh, TextAlign::Left, None, window, cx);
            }
        });
        if !first_row {
            continue;
        }
        match row {
            None => {
                let right = point(bounds.left() + g.numbers_right, y);
                paint_number(i + 1, right, m, lh, ink_for(i), window, cx);
                // M11ah: ▸ on a folded line, always; ▾ on an open block's while the pointer is
                // over the gutter.
                if let (Some(o), Some(left)) = (fold_outline, g.fold_left)
                    && o.starting_at(i).is_some()
                {
                    let glyph = match fold {
                        Some(_) => Some(&fold_shut),
                        None => f.gutter_hover.then_some(&fold_open),
                    };
                    if let Some(glyph) = glyph {
                        let x =
                            bounds.left() + left + (px(code::FOLD_COLUMN) - glyph.width()) / 2.0;
                        let _ = glyph.paint(point(x, y), lh, TextAlign::Left, None, window, cx);
                    }
                }
                // M11af: its mark (M11ah: a folded line's, the newest of its hidden lines' too).
                let hidden_mark = fold.and_then(|(_, s)| s.mark.as_ref());
                if let (Some(l), Some(left)) = (&f.live, g.marks_left)
                    && let Some(mark) = match (l.record.mark(i), hidden_mark) {
                        (Some(own), Some(hid)) if hid.when > own.when => Some(hid),
                        (own, hid) => own.or(hid),
                    }
                {
                    let look = mark_look(mark, l.your_letter, &ui, bg);
                    paint_mark(look, bounds.left() + left, y, lh, window, cx);
                }
            }
            Some(row) => {
                if let (Some(n), Some(right)) = (row.old, g.old_numbers_right) {
                    let right = point(bounds.left() + right, y);
                    paint_number(n, right, m, lh, ink_for(i), window, cx);
                }
                if let Some(n) = row.new {
                    let right = point(bounds.left() + g.numbers_right, y);
                    paint_number(n, right, m, lh, ink_for(i), window, cx);
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
    // M11ah: lines deleted only inside a fold: the ▸ on the folded line's row, in its middle.
    for (&line, (_, s)) in &folds {
        let own = f.doc.changes.by_line.get(&line).copied();
        if either_change(own, s.change) != Some(LineChange::DeletedAfter) {
            continue;
        }
        let Some(row) = map.first_row(line).filter(|r| rows.contains(r)) else {
            continue;
        };
        let y = bounds.top() + px(map.top(row)) - g.scroll.y;
        let x = bounds.left() + g.bar_left + px(BAR_WIDTH / 2.0) - marker.width() / 2.0;
        let _ = marker.paint(point(x, y), lh, TextAlign::Left, None, window, cx);
    }
    for &edge in &f.doc.changes.deleted {
        // A hidden edge is its fold's (above).
        let Some(edge) = map.first_row(edge) else {
            continue;
        };
        if edge < rows.start || edge > rows.end {
            continue;
        }
        let y = bounds.top() + px(map.top(edge)) - g.scroll.y;
        let top = if edge == 0 { y } else { y - lh / 2.0 };
        let x = bounds.left() + g.bar_left + px(BAR_WIDTH / 2.0) - marker.width() / 2.0;
        let _ = marker.paint(point(x, top), lh, TextAlign::Left, None, window, cx);
    }
}

/// M11ah: the breadcrumb's parts laid out for `key`: each shaped, the ones that fit `key.room`
/// (`fit_crumbs`) placed left to right with " › " between them and "…" for the folded outer
/// ones. A part's x is from the strip's text edge; the last part may run past the room (cut
/// where it is drawn).
fn lay_out_crumbs(key: &CrumbFitKey, window: &mut Window) -> CrumbsLaid {
    let shape = |words: &str, ink: Rgb, window: &mut Window| {
        let run = TextRun {
            len: words.len(),
            font: key.font.clone(),
            color: hsla(ink),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        window.text_system().shape_line(
            SharedString::from(words.to_string()),
            key.font_size,
            &[run],
            None,
        )
    };
    let (dim, bright) = key.inks;
    let last = key.crumbs.len().saturating_sub(1);
    let parts: Vec<gpui::ShapedLine> = key
        .crumbs
        .iter()
        .enumerate()
        .map(|(i, c)| shape(&c.words, if i == last { bright } else { dim }, window))
        .collect();
    let separator = shape(CRUMB_SEPARATOR, dim, window);
    let dots = shape(ELLIPSIS, dim, window);
    let widths: Vec<f32> = parts.iter().map(|p| f32::from(p.width)).collect();
    let shown = fit_crumbs(
        &widths,
        key.room,
        f32::from(dots.width),
        f32::from(separator.width),
    );
    let mut x = px(0.0);
    let mut laid = Vec::new();
    for (k, part) in shown.into_iter().enumerate() {
        if k > 0 {
            laid.push((x, separator.clone(), CrumbPiece::Separator));
            x += separator.width;
        }
        let (shaped, piece) = match part {
            Some(i) => (parts[i].clone(), CrumbPiece::Part(key.crumbs[i].clone())),
            None => (dots.clone(), CrumbPiece::Folded),
        };
        let width = shaped.width;
        laid.push((x, shaped, piece));
        x += width;
    }
    laid
}

/// M11ah: the sticky line (File mode): block line `line` pinned over the text's first row, on
/// the background, with its number (as bright as the current line's when it is current) and a
/// `STICKY_RULE` `divider` line under it. Its text as the line's own row draws it, following
/// the sideways scroll; with Wrap, one row, cut with "…" when the line takes more.
fn paint_sticky(
    f: &Frame,
    g: &Geom,
    m: &CellMetrics,
    map: &RowMap,
    line: usize,
    window: &mut Window,
    cx: &mut App,
) {
    let ui = f.theme.ui();
    let bg = f.theme.background;
    let (bounds, lh) = (g.bounds, g.line_height);
    let y = bounds.top();
    let row = Bounds::new(point(bounds.left(), y), size(bounds.size.width, lh));
    window.paint_quad(fill(row, hsla(bg)));
    let rule = Bounds::new(
        point(bounds.left(), y + lh),
        size(bounds.size.width, px(code::STICKY_RULE)),
    );
    window.paint_quad(fill(rule, hsla(ui.divider)));
    let ink = number_ink(&ui, bg, f.current_line == Some(line));
    let right = point(bounds.left() + g.numbers_right, y);
    paint_number(line + 1, right, m, lh, ink, window, cx);
    let spans = f
        .doc
        .spans
        .as_deref()
        .filter(|_| f.coloured)
        .and_then(|s| s.get(line))
        .map(Vec::as_slice);
    let fg = f.theme.foreground;
    let (laid, text_x, cut) = match map.columns() {
        Some(columns) => {
            let width = f.doc.widths.get(line).copied().unwrap_or(0);
            let cut = width > columns;
            // Cut: one cell less, for the "…".
            let cells = if cut {
                columns.saturating_sub(1)
            } else {
                columns
            };
            let (from, to, start) = wrapped_row(&f.doc, line, 0, cells.max(1));
            let laid = Laid::part(&f.doc, line, from, to, start, spans, fg, m, window);
            (laid, bounds.left() + g.text_left, cut)
        }
        None => {
            let laid = Laid::new(
                &f.doc,
                line,
                spans,
                fg,
                m,
                g.scroll.x,
                bounds.size.width,
                window,
            );
            (laid, bounds.left() + g.text_left - g.scroll.x, false)
        }
    };
    let mask = ContentMask {
        bounds: Bounds::from_corners(
            point(bounds.left() + g.text_left - px(TEXT_GAP - 1.0), y),
            point(bounds.right(), y + lh),
        ),
    };
    window.with_content_mask(Some(mask), |window| {
        if let Some(shaped) = &laid.shaped {
            let _ = shaped.paint(
                point(text_x + laid.x0, y),
                lh,
                TextAlign::Left,
                None,
                window,
                cx,
            );
        }
        if cut {
            let dots = window.text_system().shape_line(
                SharedString::from(ELLIPSIS),
                m.font_size,
                &[plain_run(ELLIPSIS.len(), m, fg)],
                None,
            );
            let x = text_x + laid.x0 + laid.width;
            let _ = dots.paint(point(x, y), lh, TextAlign::Left, None, window, cx);
        }
    });
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
    /// M11ah: the text body's screen rows as last drawn (with Wrap on, cut at its width) and
    /// what they were made for; none before the first drawing.
    rows: Rc<RefCell<Option<(MapKey, RowMap)>>>,
    /// The selection: where the mouse went down, and where it is.
    selection: Option<(Pos, Pos)>,
    selecting: bool,
    /// M11ah: the current line: the last press's line, or where a drag ended, a find step's
    /// line (File mode: a line), and the same in Diff mode (a `DiffDoc` row). None when the
    /// tab opens; Esc does not clear it.
    current: Option<usize>,
    diff_current: Option<usize>,
    /// M11ah: the folded blocks (File mode), by their first line, and how many times they
    /// changed (the row map's key); whether the pointer is over the gutter (the open blocks'
    /// ▾ show). Not kept over a restart.
    folded: BTreeSet<usize>,
    folds_gen: u64,
    gutter_hover: bool,
    /// M11ah: the sticky line and the breadcrumb's parts (File mode), worked out when the view
    /// moves (`refresh_place`), never while drawing.
    place: Place,
    /// M11ah: the breadcrumb's parts as last drawn (a press finds its part), and how they were
    /// laid out, for the words and the room they were measured in.
    crumbs_at: Rc<RefCell<CrumbsDrawn>>,
    crumbs_fit: Rc<RefCell<Option<(CrumbFitKey, CrumbsLaid)>>>,
    /// M11ae: the pointer's last place while selecting, and whether the auto-scroll task runs
    /// (one at a time; only while the pointer is out of the text).
    drag_at: Option<Point<Pixels>>,
    autoscrolling: bool,
    /// M11ae: the overlay scrollbar (File, Diff and Preview: one shows at a time).
    scrollbar: Bar,
    find: Option<FindBar>,
    /// On screen (the workspace sets it each frame), polling, polls so far.
    shown: bool,
    ticking: bool,
    polls: u32,
    /// The UI screenshots: no polling, no git.
    pub frozen: bool,
    /// The Close button's bounds (a missing file's), as last drawn.
    close_at: Rc<Cell<Option<Bounds<Pixels>>>>,
    /// The view switch's File, Diff and Preview segments (`View::index`), and the bar's
    /// controls together, as last drawn (none: not drawn).
    toggle_at: Rc<Cell<[Option<Bounds<Pixels>>; 3]>>,
    controls_at: Rc<Cell<Option<Bounds<Pixels>>>>,
    /// M11af: the Follow pill, as drawn this frame (none: not drawn).
    follow_at: Rc<Cell<Option<Bounds<Pixels>>>>,
    /// The second line's Where and the parts after it, as last drawn (the scenes check Where
    /// is cut first).
    meta_at: Rc<Cell<[Option<Bounds<Pixels>>; 2]>>,
    /// M11ae: how long ago the file was modified (`age_label`), as the bar says it; worked out
    /// when a version is read and in each poll (`refresh_age`), never while drawing.
    age: Option<String>,
    /// M11ac: a Markdown file shown rendered (`md_preview`; only in File mode), the reader's
    /// scroll (kept over a reload), and the blocks parsed for the version shown.
    preview: bool,
    preview_list: ListState,
    parsed: Option<(u64, Rc<Vec<Md>>)>,
    /// M11ac Part 2: what each block of the parsed version does as a Mermaid diagram, for the
    /// version and light or dark it was planned for (`mermaid::plan`), and how they were shown
    /// last frame.
    mermaid_plan: Option<(u64, bool, MermaidPlan)>,
    mermaid_shown: Rc<Vec<Option<md_preview::Shown>>>,
    /// M11af: who changed each line since the tab opened.
    live: Live,
    /// M11af: the mark under the pointer: its tooltip's place (window points) and words.
    mark_tip: Option<(Point<Pixels>, SharedString)>,
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
            rows: Rc::default(),
            selection: None,
            selecting: false,
            current: None,
            diff_current: None,
            folded: BTreeSet::new(),
            folds_gen: 0,
            gutter_hover: false,
            place: Place::default(),
            crumbs_at: Rc::default(),
            crumbs_fit: Rc::default(),
            drag_at: None,
            autoscrolling: false,
            scrollbar: Bar::default(),
            find: None,
            shown: false,
            ticking: false,
            polls: 0,
            frozen: false,
            close_at: Rc::default(),
            toggle_at: Rc::default(),
            controls_at: Rc::default(),
            follow_at: Rc::default(),
            meta_at: Rc::default(),
            age: None,
            preview: false,
            preview_list: preview_list_of(0, cx.entity().downgrade()),
            parsed: None,
            mermaid_plan: None,
            mermaid_shown: Rc::default(),
            live: Live::default(),
            mark_tip: None,
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

    /// M11ah: the screen rows of `doc` shown in `mode`, cut at `columns` (Wrap) with rows
    /// `row_h` tall and (File mode) the `folded` blocks' lines hidden. The one place a map is
    /// made (the canvas, Follow, a diff's reveal, a fold), so they never disagree.
    fn row_map_of(
        doc: &Doc,
        mode: FileMode,
        columns: Option<usize>,
        row_h: f32,
        folded: &BTreeSet<usize>,
    ) -> RowMap {
        let hidden = match (mode, &doc.outline) {
            (FileMode::Plain, Some(outline)) if !folded.is_empty() => {
                code_outline::hidden_ranges(outline, folded)
            }
            _ => Vec::new(),
        };
        RowMap::file(&doc.widths, columns, &hidden, row_h)
    }

    /// The rows of the text shown now (its folds, the Wrap width and row height last drawn),
    /// made fresh: a fold or a reload changed them since the last drawing. None before it.
    fn fresh_map(&self, cx: &App) -> Option<RowMap> {
        let (g, doc) = (self.geom.get()?, self.shown_doc()?);
        let columns = self
            .wrap_columns()
            .filter(|_| cx.global::<Appearance>().file_wrap);
        Some(Self::row_map_of(
            doc,
            self.mode,
            columns,
            f32::from(g.line_height),
            &self.folded,
        ))
    }

    /// M11ah: whether blocks fold here: File mode (not Preview) on a file with blocks. The
    /// breadcrumb and the sticky line show by the same rule.
    fn folds_apply(&self) -> bool {
        self.mode == FileMode::Plain
            && !self.preview_shown()
            && self
                .doc()
                .and_then(|d| d.outline.as_deref())
                .is_some_and(Outline::has_structure)
    }

    /// M11ah: the blocks folded now `folded` (a fold, an unfold, Fold All...): the rows are made
    /// again, the line at the top stays there (one now hidden: the folded line above it), a
    /// current line now hidden moves up to its fold's line, and Follow ends (you moved the
    /// view). Drawn again only when the folds changed.
    fn set_folds(&mut self, folded: BTreeSet<usize>, cx: &mut Context<Self>) {
        if folded == self.folded {
            return;
        }
        let before = self.rows.borrow().as_ref().map(|(_, map)| map.clone());
        self.folded = folded;
        self.folds_gen += 1;
        self.live.follow = false;
        let Some(map) = self.fresh_map(cx) else {
            cx.notify();
            return;
        };
        if let Some(line) = self.current
            && map.first_row(line).is_none()
        {
            self.current = (0..line).rev().find(|&l| map.first_row(l).is_some());
        }
        if let Some(before) = before {
            let y = f32::from(self.scroll.y);
            let top = before.row_at(y);
            let row = rescroll_row(top, Some(&before), &map);
            // Still shown: as far into its line as before; else the folded line's top.
            let into = match before.row(top) {
                Some(Row::Line { line, .. }) if map.first_row(line) == Some(row) => {
                    y - before.top(before.first_row(line).unwrap_or(top))
                }
                _ => 0.0,
            };
            self.scroll.y = px(map.top(row) + into.max(0.0));
        }
        self.refresh_place(cx);
        cx.notify();
    }

    /// M11ah: ⌥⌘[ folds the innermost open block holding the current line (none: the line at
    /// the top); ⌥⌘] (`fold` false) unfolds the innermost folded one. File mode only.
    fn fold_here(&mut self, fold: bool, cx: &mut Context<Self>) {
        if !self.folds_apply() {
            return;
        }
        let Some(outline) = self.doc().and_then(|d| d.outline.clone()) else {
            return;
        };
        let Some(line) = self.current.or_else(|| self.top_line()) else {
            return;
        };
        let start = outline
            .containing(line)
            .into_iter()
            .rev()
            .map(|b| outline.blocks[b].start)
            .find(|start| self.folded.contains(start) != fold);
        let Some(start) = start else {
            return;
        };
        let mut folded = self.folded.clone();
        if fold {
            folded.insert(start);
        } else {
            folded.remove(&start);
        }
        self.set_folds(folded, cx);
    }

    /// M11ah: the palette's Fold All (every block) and Unfold All (`fold` false).
    fn fold_all(&mut self, fold: bool, cx: &mut Context<Self>) {
        if !self.folds_apply() {
            return;
        }
        let folded = match self.doc().and_then(|d| d.outline.as_deref()) {
            Some(outline) if fold => outline.blocks.iter().map(|b| b.start).collect(),
            _ => BTreeSet::new(),
        };
        self.set_folds(folded, cx);
    }

    /// M11ah: the folds that hide `line` open (a find step or Follow went there; not you
    /// moving the view, so Follow stays). True when one opened.
    fn open_folds_at(&mut self, line: usize) -> bool {
        let Some(outline) = self.doc().and_then(|d| d.outline.clone()) else {
            return false;
        };
        let before = self.folded.len();
        for b in outline.containing(line) {
            let start = outline.blocks[b].start;
            if start < line {
                self.folded.remove(&start);
            }
        }
        let opened = self.folded.len() != before;
        if opened {
            self.folds_gen += 1;
        }
        opened
    }

    /// M11ah: the rows of the text shown as they are now: the last drawing's when made for this
    /// mode, version, folds and Wrap; else made fresh (a read or a fold since then). None
    /// before the first drawing.
    fn map_now(&self, cx: &App) -> Option<RowMap> {
        let doc = self.shown_doc()?;
        let wrap = cx.global::<Appearance>().file_wrap;
        if let Some((key, map)) = self.rows.borrow().as_ref()
            && key.mode == self.mode
            && key.version == doc.version
            && key.folds == self.folds_gen
            && key.columns.is_some() == wrap
        {
            return Some(map.clone());
        }
        self.fresh_map(cx)
    }

    /// M11ah: the place shown now (File mode, a file with blocks, not Preview; else none): the
    /// breadcrumb for the line at the top, the sticky line for the line just under its row (a
    /// line exactly at the top has none pinned over it). Before the first drawing: the top.
    fn place_now(&self, cx: &App) -> Place {
        let (Some(doc), true) = (self.doc(), self.folds_apply()) else {
            return Place::default();
        };
        let Some(outline) = doc.outline.as_deref() else {
            return Place::default();
        };
        let (top, under) = match (self.map_now(cx), self.geom.get()) {
            (Some(map), Some(g)) => {
                // As the next drawing keeps the scroll (`Geom::clamp`).
                let max = (map.height() - f32::from(g.bounds.size.height)).max(0.0);
                let y = f32::from(self.scroll.y).clamp(0.0, max);
                let line_at = |y: f32| match map.row(map.row_at(y)) {
                    Some(Row::Line { line, .. }) => line,
                    _ => 0,
                };
                (line_at(y), line_at(y + map.row_h() - 1.0))
            }
            _ => (0, 0),
        };
        let name = self
            .path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let markdown = is_markdown(&self.path);
        place_of(&name, outline, |i| doc.line(i), (top, under), markdown)
    }

    /// M11ah: the place worked out again (a scroll, a read, the colours landing, a fold or a
    /// mode change); drawn again only when it changed.
    fn refresh_place(&mut self, cx: &mut Context<Self>) {
        let place = self.place_now(cx);
        if place != self.place {
            self.place = place;
            cx.notify();
        }
    }

    /// M11ah: a breadcrumb part or the sticky line pressed: its line becomes the first row you
    /// see (the top row; when a sticky line shows there, the row under it, as `reveal_top`
    /// leaves room) and current; the file's name (`None`) goes to the top. You moved the view:
    /// Follow ends.
    fn show_at_top(&mut self, line: Option<usize>, cx: &mut Context<Self>) {
        let y = match line {
            Some(line) => {
                let Some(map) = self.map_now(cx) else {
                    return;
                };
                // A line with no row (hidden): nowhere to go.
                let Some(row) = map.first_row(line) else {
                    return;
                };
                let top = if row > 0 && self.sticky_at_row(&map, row) {
                    row - 1
                } else {
                    row
                };
                map.top(top)
            }
            None => 0.0,
        };
        self.live.follow = false;
        self.set_scroll(point(self.scroll.x, px(y)), cx);
        if line.is_some() {
            self.set_current(line, cx);
        }
        self.refresh_place(cx);
        cx.notify();
    }

    /// M11ah: the sticky line's line, when window point `p` is on it (as last drawn).
    fn sticky_under(&self, p: Point<Pixels>) -> Option<usize> {
        let Some(Sticky::Line(line)) = self.place.sticky else {
            return None;
        };
        let g = self.geom.get()?;
        (g.bounds.contains(&p) && p.y < g.bounds.top() + g.line_height).then_some(line)
    }

    /// M11ah: whether a sticky line shows with row `r` of `map` at the top (File mode): what a
    /// reveal leaves one more row of room for (`reveal_top`).
    fn sticky_at_row(&self, map: &RowMap, r: usize) -> bool {
        if !self.folds_apply() {
            return false;
        }
        let outline = self.doc().and_then(|d| d.outline.as_deref());
        match (outline, map.row(r)) {
            (Some(o), Some(Row::Line { line, .. })) => o.sticky(line).is_some(),
            _ => false,
        }
    }

    /// M11ah: a press in the breadcrumb: the part under it (as last drawn) goes to the top.
    fn crumb_press(&mut self, p: Point<Pixels>, cx: &mut Context<Self>) {
        let hit = self
            .crumbs_at
            .borrow()
            .iter()
            .find(|(b, _)| b.contains(&p))
            .and_then(|(_, c)| c.clone());
        if let Some(crumb) = hit {
            self.show_at_top(crumb.line, cx);
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
        // M11ae: off screen, its canvas isn't painted, so its release and move listeners are
        // gone: a drag going on would never end (the auto-scroll stepping to the text's end,
        // each step redrawing the window). The selection's drag, a thumb's drag and the hover
        // end here; the auto-scroll task ends at its next step. These draw nothing.
        if was && !shown {
            self.selecting = false;
            self.drag_at = None;
            if self.scrollbar.hide() {
                self.preview_list.scrollbar_drag_ended();
            }
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
                            this.refresh_age(SystemTime::now(), cx);
                            this.refresh_live(cx);
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
        let (path, known, prev) = (self.path.clone(), self.stamp, self.live.prev());
        let at = Instant::now();
        cx.spawn(async move |this, cx| {
            let loaded = cx
                .background_executor()
                .spawn(async move { read_file(&path, known, prev) })
                .await;
            this.update(cx, |this, cx| {
                this.reading = false;
                this.loaded(loaded, at, cx);
            })
            .ok();
        })
        .detach();
    }

    /// Show what the disk gave (looked at `at`): a new version of the text (the scroll kept; the
    /// old colours and bars until the new ones come), or a note. The live record follows.
    fn loaded(&mut self, loaded: Loaded, at: Instant, cx: &mut Context<Self>) {
        match loaded {
            // The same file: only its age may read differently (a tab back on screen).
            Loaded::Unchanged => {
                self.live_saw(Seen::Unchanged, at, cx);
                self.refresh_age(SystemTime::now(), cx);
                return;
            }
            // Each look at the disk that finds something new is a version (a diff asked for an
            // older one is dropped).
            Loaded::Missing => {
                // The marks and the last text stay: a file back is compared with it.
                self.live_saw(Seen::Missing, at, cx);
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
                self.live_saw(Seen::Refused, at, cx);
                self.versions += 1;
                self.state = State::Refuse;
                self.stamp = Some(stamp);
                self.look_up_changes(cx);
                self.drop_find_without_text();
            }
            Loaded::Text {
                stamp,
                doc,
                plain,
                compared,
                real,
            } => {
                let mut doc = *doc;
                self.versions += 1;
                doc.version = self.versions;
                let mut folded = BTreeSet::new();
                if let Some(old) = self.doc() {
                    doc.carry_over(old, plain);
                    // M11ah: the current line follows its line (the reload's comparison), or
                    // without one stays when its text is unchanged; so does each fold (one whose
                    // first line changed or went opens).
                    let map = compared.as_ref().map(|c| c.new_of_old.as_slice());
                    let same = |l: usize| {
                        l < old.lines.len() && l < doc.lines.len() && old.line(l) == doc.line(l)
                    };
                    if let Some(outline) = &doc.outline {
                        folded = carry_folds(&self.folded, map, same, outline);
                    }
                    self.current = self.current.and_then(|line| carry_line(map, line, same));
                }
                if folded != self.folded {
                    self.folded = folded;
                    self.folds_gen += 1;
                }
                if real.is_some() {
                    self.live.real = real;
                }
                let when = change_time(SystemTime::now(), stamp.modified);
                let seen = Seen::Text {
                    doc: &doc,
                    compared,
                    when,
                };
                self.live_saw(seen, at, cx);
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
                self.follow_latest(cx);
            }
        }
        self.refresh_age(SystemTime::now(), cx);
        self.keep_selection();
        self.find_again(cx);
        // M11ah: the sticky line and the breadcrumb, for the new text (or none without one).
        self.refresh_place(cx);
        cx.notify();
    }

    /// M11af: what a look at the disk (at `at`) means for the live record. Who changed new
    /// lines is the edit ledger's answer; a decision a late finish may revise starts the
    /// revision check. Your letter is looked up once, at the first mark.
    fn live_saw(&mut self, seen: Seen, at: Instant, cx: &mut Context<Self>) {
        let path = self.live.real.clone().unwrap_or_else(|| self.path.clone());
        let ws = self.live.workspace;
        let revisable = self.live.saw(seen, at, |since| {
            who_changed(cx, &path, ws, since, Instant::now())
        });
        if revisable {
            self.revise_soon(cx);
        }
        if self.live.marked && !self.live.letter_asked && !self.frozen {
            self.ask_your_letter(cx);
        }
        self.refresh_live(cx);
    }

    /// M11af: the glow and its words worked out at now (each poll, each look at the disk),
    /// never while drawing: drawn again only when a run's level or words changed.
    fn refresh_live(&mut self, cx: &mut Context<Self>) {
        if self.live.refresh(self.live_now()) {
            cx.notify();
        }
    }

    /// M11af: the time the glow and the tooltips are worked out at: now (a scene's own).
    fn live_now(&self) -> SystemTime {
        #[cfg(feature = "ui-shots")]
        if let Some(now) = self.live.now {
            return now;
        }
        SystemTime::now()
    }

    /// The revision check (one at a time): 2 s after the look it waits for, a late finish may
    /// revise that look's change; then it ends (a later look meanwhile: it waits for that one).
    fn revise_soon(&mut self, cx: &mut Context<Self>) {
        if self.live.revising {
            return;
        }
        self.live.revising = true;
        cx.spawn(async move |this, cx| {
            let mut wait = SLACK;
            loop {
                cx.background_executor().timer(wait).await;
                match this.update(cx, |this, cx| this.revise_pending(cx)) {
                    Ok(Some(more)) => wait = more,
                    _ => break,
                }
            }
        })
        .detach();
    }

    /// The revision check's turn: the look waited for, once its 2 s have passed, is revised to
    /// exact if an agent's edit to the file finished ok in them; one decided while an edit was
    /// open is settled without it otherwise (a guess, or yours). Drawn again only if a mark
    /// changed; then the check ends. Some: that look's 2 s are still running, wait so long.
    fn revise_pending(&mut self, cx: &mut Context<Self>) -> Option<Duration> {
        let Some(look) = self.live.pending else {
            self.live.revising = false;
            return None;
        };
        let left = (look.at + SLACK).saturating_duration_since(Instant::now());
        if !left.is_zero() {
            return Some(left);
        }
        self.live.pending = None;
        self.live.revising = false;
        let path = self.live.real.as_deref().unwrap_or(&self.path);
        let ws = self.live.workspace;
        // A look decided while an edit was open is settled (ruling R10: it may be downgraded);
        // any other may only be revised up by a late finish.
        let changed = match cx.try_global::<Ledger>() {
            Some(ledger) if look.open => {
                let settled = ledger.settle(path, ws, look.since, look.at);
                self.live.settle(settled)
            }
            Some(ledger) => {
                let found = ledger.revise(path, look.at);
                self.live.revise(found)
            }
            None => false,
        };
        if changed {
            self.live.sync_marks();
            self.follow_latest(cx);
            cx.notify();
        }
        None
    }

    /// M11af part 4: with Follow on, after a change by an agent, bring its first line into view
    /// (File mode, as last drawn; never while Preview shows). Not through `set_scroll`: that is yours, and ends Follow.
    /// The rows aren't drawn yet, so the scroll is kept within them when drawn (as a diff's
    /// reveal is). By the row map (M11ah): the line's first screen row, the new text's rows made
    /// at the width last drawn; a fold holding the line opens.
    fn follow_latest(&mut self, cx: &mut Context<Self>) {
        let (Some(line), Some(g)) = (self.live.follow_line(), self.geom.get()) else {
            return;
        };
        if self.mode != FileMode::Plain || self.preview_shown() {
            return;
        }
        // M11ah: a change inside a fold opens it (Follow stays on: not your move).
        if self.open_folds_at(line) {
            cx.notify();
        }
        let Some(map) = self.fresh_map(cx) else {
            return;
        };
        let Some(row) = map.first_row(line) else {
            return;
        };
        let top = map.row_at(f32::from(g.scroll.y));
        let rows = (g.bounds.size.height / g.line_height).floor().max(1.0) as usize;
        // M11ah: a row under the sticky line is not in view; the change lands below it.
        let covered = usize::from(self.place.sticky.is_some());
        let view = (top + covered).min(top + rows)..top + rows;
        if let Some(first) = follow_scroll(view, row, |r| self.sticky_at_row(&map, r)) {
            self.scroll = point(g.scroll.x, px(map.top(first)));
            self.scrolled(cx);
            cx.notify();
        }
        self.refresh_place(cx);
    }

    /// Your letter, looked up once off the UI thread (`find_your_letter`).
    fn ask_your_letter(&mut self, cx: &mut Context<Self>) {
        self.live.letter_asked = true;
        let place = self.git_place.clone();
        cx.spawn(async move |this, cx| {
            let letter = cx
                .background_executor()
                .spawn(async move { find_your_letter(place) })
                .await;
            this.update(cx, |this, cx| {
                this.live.your_letter = Some(letter);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// The workspace's word, each frame: which workspace the tab is in (its working agents'
    /// changes are guessed theirs). Nothing is drawn differently.
    pub(crate) fn set_workspace(&mut self, ws: Option<WorkspaceId>) {
        self.live.workspace = ws;
    }

    /// M11af: who changed each line since the tab opened.
    pub fn live_marks(&self) -> &LineRecord {
        &self.live.record
    }

    /// The bar's "edited …" (M11ae) at `now`, from the file's time as last read (none without
    /// one); drawn again only when the words change.
    fn refresh_age(&mut self, now: SystemTime, cx: &mut Context<Self>) {
        let age = self
            .stamp
            .and_then(|s| s.modified)
            .map(|modified| age_label(modified, now));
        if age != self.age {
            self.age = age;
            cx.notify();
        }
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
            let (coloured, syntax) = cx
                .background_executor()
                .spawn(async move {
                    let first = text.split_inclusive('\n').next().unwrap_or("");
                    let syntax: Arc<str> = h.syntax_name(&path, first).into();
                    (h.colour(&path, &text), syntax)
                })
                .await;
            this.update(cx, |this, cx| {
                if let State::Text(doc) = &mut this.state
                    && doc.accept_spans(version, coloured.spans, &coloured.named)
                {
                    doc.syntax = Some(syntax);
                    // M11ah: the named lines came with them: the sticky line and the
                    // breadcrumb name their blocks now.
                    this.refresh_place(cx);
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
                    colour_rows(&h, &path, &parsed.rows, &sides(&parsed)).spans
                })
                .await;
            this.update(cx, |this, cx| {
                // Only onto the rows it was made for.
                if let DiffState::Rows(d) = &mut this.diff.state
                    && Arc::ptr_eq(&d.rows, &rows)
                    && d.doc.accept_spans(version, spans, &[])
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
                    && doc.accept_changes(
                        version,
                        &changes.lines,
                        Some((changes.added, changes.removed, changes.new)),
                    )
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
        // M11ae: the header chip's counts from the rows (none to show when unchanged), so Diff
        // mode's chip is there and current too; taken when the diff is (the held diff's version
        // check). Look-ups run one at a time, so the bars' answer and the diff's never cross.
        let counts = match &state {
            DiffState::Rows(d) => Some(d.counts()),
            DiffState::Same => Some((0, 0, false)),
            _ => None,
        };
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
                if self.take_diff_counts(counts) {
                    cx.notify();
                }
            }
            return;
        }
        if !self.diff.accept(version, self.versions, state) {
            return;
        }
        // Redrawn below. The rows changed: the current row is another one's.
        self.diff_current = None;
        self.take_diff_counts(counts);
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

    /// The counts a diff gave (none: it gives none, as binary or refused) for the file's text;
    /// whether the chip changed.
    fn take_diff_counts(&mut self, counts: Option<(usize, usize, bool)>) -> bool {
        match (counts, self.doc_mut()) {
            (Some(counts), Some(doc)) => doc.set_counts(Some(counts)),
            _ => false,
        }
    }

    /// The file's mode now (M11h).
    pub fn mode(&self) -> FileMode {
        self.mode
    }

    /// Whether the Markdown is shown rendered now (M11ac): Preview on, a Markdown file, File
    /// mode.
    pub fn preview_shown(&self) -> bool {
        self.mode == FileMode::Plain
            && self.preview
            && is_markdown(&self.path)
            && preview_fits(&self.state)
    }

    /// Preview on or off by hand (M11ac): the find bar goes with the source, and the workspace
    /// saves it.
    fn choose_preview(&mut self, on: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.preview == on || (on && !preview_fits(&self.state)) {
            return;
        }
        self.preview = on;
        // M11af: as a mode switch, Preview on or off ends Follow, and the marks' tooltip does
        // not wait for the way back.
        self.live.follow = false;
        self.mark_tip = None;
        if on && self.find.is_some() {
            self.close_find(window, cx);
        }
        // The selection ends (and with it a drag's auto-scroll, M11ae), as for a mode switch.
        self.selection = None;
        self.selecting = false;
        cx.emit(FileTabEvent::Preview(on));
        self.refresh_place(cx);
        cx.notify();
    }

    /// A click on the view switch (M11ae): File turns Preview off and shows the file; Diff
    /// shows the diff; Preview (when the file fits) shows the file rendered. What is saved per
    /// tab stays its mode and its Preview.
    fn choose_view(&mut self, view: View, window: &mut Window, cx: &mut Context<Self>) {
        match view {
            View::File => {
                self.choose_preview(false, window, cx);
                self.choose_mode(FileMode::Plain, None, cx);
            }
            View::Diff => self.choose_mode(FileMode::Diff, None, cx),
            View::Preview => {
                if preview_fits(&self.state) {
                    self.choose_mode(FileMode::Plain, None, cx);
                    self.choose_preview(true, window, cx);
                }
            }
        }
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

    /// A press at `p` in the Preview's scrollbar zone (M11ae): the thumb is grabbed, or jumps to
    /// centre on the press and is dragged on, as in the code view. Whether it was in the zone
    /// (then the press goes no further: the list's links and diagrams get the rest).
    fn preview_press(&mut self, p: Point<Pixels>, cx: &mut Context<Self>) -> bool {
        let Some(track) = preview_track(&self.preview_list).filter(|t| t.zone_has(p)) else {
            return false;
        };
        let (at, t) = (track.along(p), track.thumb);
        // Frozen content height for the drag, before the first move of the list.
        self.preview_list.scrollbar_drag_started();
        let grab = if (t.start..=t.start + t.len).contains(&at) {
            at - t.start
        } else {
            self.preview_scroll_to(&track, at - t.len / 2.0, cx);
            t.len / 2.0
        };
        self.scrollbar.start_drag(Axis::Vertical, grab);
        cx.notify();
        true
    }

    /// Scroll the list so the thumb starts at `start` along `track` (`set_offset_from_scrollbar`
    /// takes the offset with a negative `y`).
    fn preview_scroll_to(&mut self, track: &Track, start: f32, cx: &mut Context<Self>) {
        let to = scrollbar::offset_for(start, track.viewport, track.content);
        if to >= track.content - track.viewport - 1.0 {
            // The thumb at its end: the text's true end, whatever rows were not yet measured.
            self.preview_list.scroll_to_end();
        } else if self.preview_list.item_count() > PREVIEW_MEASURE_ALL {
            let item_ix = (to / PREVIEW_BLOCK_HINT) as usize;
            self.preview_list.scroll_to(ListOffset {
                item_ix,
                offset_in_item: px(0.0),
            });
        } else {
            self.preview_list
                .set_offset_from_scrollbar(point(px(0.0), px(-to)));
        }
        self.scrolled(cx);
        cx.notify();
    }

    /// The pointer moved anywhere in the window while Preview shows: a thumb drag follows it;
    /// near the list's right edge the scrollbar shows.
    fn preview_move(&mut self, e: &MouseMoveEvent, over: bool, cx: &mut Context<Self>) {
        if let Some((_, grab)) = self.scrollbar.drag() {
            if e.pressed_button != Some(MouseButton::Left) {
                self.preview_release(cx);
            } else if let Some(track) = preview_track(&self.preview_list) {
                self.preview_scroll_to(&track, track.along(e.position) - grab, cx);
            }
            return;
        }
        let hot = over && preview_track(&self.preview_list).is_some_and(|t| t.zone_has(e.position));
        if self.scrollbar.set_hot(hot) {
            self.scrolled(cx);
            cx.notify();
        }
    }

    /// M11ae: the pointer left the window (GPUI sends no last move): the edge zone isn't hot,
    /// so the thumb fades as after any activity (a pane at the window's edge is the usual case).
    fn pointer_left(&mut self, cx: &mut Context<Self>) {
        if self.scrollbar.set_hot(false) {
            self.scrolled(cx);
            cx.notify();
        }
        self.hover_mark(None, cx);
        self.hover_gutter(false, cx);
    }

    /// M11ah: whether the pointer is over the gutter (the ▾ show); drawn again only when it
    /// changed.
    fn hover_gutter(&mut self, on: bool, cx: &mut Context<Self>) {
        if std::mem::replace(&mut self.gutter_hover, on) != on {
            cx.notify();
        }
    }

    /// M11af: the pointer at `p` (none: not over the text): on a mark, its tooltip shows at
    /// once (its age as of now); off it, none. Drawn again only when that changes.
    fn hover_mark(&mut self, p: Option<Point<Pixels>>, cx: &mut Context<Self>) {
        let tip = p.and_then(|p| self.mark_under(p)).and_then(|(line, at)| {
            let m = self.live_marks().mark(line)?;
            Some((at, SharedString::from(mark_tip_words(m, self.live_now()))))
        });
        if tip != self.mark_tip {
            self.mark_tip = tip;
            cx.notify();
        }
    }

    /// M11af: the marked line whose mark is under window point `p` (File mode, as last drawn),
    /// and where its tooltip goes: under the mark's row, from the column's left.
    fn mark_under(&self, p: Point<Pixels>) -> Option<(usize, Point<Pixels>)> {
        if self.mode != FileMode::Plain || self.preview_shown() {
            return None;
        }
        let g = self.geom.get()?;
        let left = g.marks_left?;
        let x = p.x - g.bounds.left() - left;
        // M11ah: the sticky line covers the marks of the row under it.
        if x < px(0.0)
            || x > px(MARK_COLUMN)
            || !g.bounds.contains(&p)
            || self.sticky_under(p).is_some()
        {
            return None;
        }
        let y = f32::from(p.y - g.bounds.top() + g.scroll.y);
        let (row, bottom, hidden) = {
            let rows = self.rows.borrow();
            let (_, map) = rows.as_ref()?;
            if y >= map.height() {
                return None;
            }
            let row = map.row_at(y);
            let hidden = match map.row(row) {
                Some(Row::Line { line, .. }) => map.hidden_after(line),
                _ => 0..0,
            };
            (map.row(row), map.top(row + 1), hidden)
        };
        // A line's first row (its number's): a wrapped line's other rows have no mark.
        let Some(Row::Line { line, cell0: 0 }) = row else {
            return None;
        };
        // M11ah: a folded line's mark is the newest of its own and its hidden lines'.
        let (line, _) = newest_mark(self.live_marks(), line..hidden.end.max(line + 1))?;
        let below = px(bottom) - g.scroll.y + px(2.0);
        Some((line, point(g.bounds.left() + left, g.bounds.top() + below)))
    }

    /// The button released anywhere in the window: a thumb drag ends.
    fn preview_release(&mut self, cx: &mut Context<Self>) {
        if self.scrollbar.end_drag(Instant::now()) {
            self.preview_list.scrollbar_drag_ended();
            self.scrolled(cx);
            cx.notify();
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
                // A new list (its measuring is by the block count), at the place kept.
                let top = self.preview_list.logical_scroll_top();
                self.preview_list = preview_list_of(blocks.len(), cx.entity().downgrade());
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
        let (bar, ink, state) = (self.scrollbar, theme.ui().text, self.preview_list.clone());
        let view = cx.entity().downgrade();
        div()
            .id("file-md-preview")
            .relative()
            .flex_1()
            .min_h(px(0.0))
            .w_full()
            .child(
                list(self.preview_list.clone(), move |ix, _, _| {
                    md_preview::row(&blocks, ix, &draw)
                })
                .size_full(),
            )
            // M11ae: the scrollbar over the list: its thumb (the fade read from the clock; the
            // fade task redraws), and the 12 pt edge zone's presses, taken before the list's
            // own (bubbling goes last-registered first); presses elsewhere reach the list.
            .child(
                canvas(
                    |bounds, window, _| window.insert_hitbox(bounds, HitboxBehavior::Normal),
                    move |_, hitbox, window, _| {
                        let now = Instant::now();
                        if bar.is_shown(now)
                            && let Some(t) = preview_track(&state)
                        {
                            scrollbar::paint(
                                window,
                                t.area,
                                Axis::Vertical,
                                &t.thumb,
                                bar.alpha_at(now),
                                ink,
                            );
                        }
                        window.on_mouse_event({
                            let view = view.clone();
                            move |e: &MouseDownEvent, phase, _, cx| {
                                if phase == DispatchPhase::Bubble
                                    && e.button == MouseButton::Left
                                    && view
                                        .update(cx, |this, cx| this.preview_press(e.position, cx))
                                        .unwrap_or(false)
                                {
                                    cx.stop_propagation();
                                }
                            }
                        });
                        window.on_mouse_event({
                            let view = view.clone();
                            move |e: &MouseMoveEvent, phase, window, cx| {
                                if phase == DispatchPhase::Bubble {
                                    let over = hitbox.is_hovered(window);
                                    view.update(cx, |this, cx| this.preview_move(e, over, cx))
                                        .ok();
                                }
                            }
                        });
                        window.on_mouse_event({
                            let view = view.clone();
                            move |_: &MouseUpEvent, phase, _, cx| {
                                if phase == DispatchPhase::Bubble {
                                    view.update(cx, |this, cx| this.preview_release(cx)).ok();
                                }
                            }
                        });
                        // The pointer left the window (no last move comes): not hot.
                        window.on_mouse_event(move |_: &MouseExitEvent, phase, _, cx| {
                            if phase == DispatchPhase::Bubble {
                                view.update(cx, |this, cx| this.pointer_left(cx)).ok();
                            }
                        });
                    },
                )
                .absolute()
                .top_0()
                .left_0()
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
                            Asked::Crowded => Shown::Code(Some(mermaid::note(mermaid::CROWDED))),
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
        // M11af: a mode switch ends Follow, and the marks' tooltip does not wait for the
        // way back.
        self.live.follow = false;
        self.mark_tip = None;
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
        self.refresh_place(cx);
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

    /// Scroll to `to`, kept within the text (as last drawn). Every scroll of the text shown
    /// comes here (the wheel, find, a thumb, the auto-scroll), so the scrollbar shows.
    pub fn set_scroll(&mut self, to: Point<Pixels>, cx: &mut Context<Self>) {
        // M11af: any scroll of yours ends Follow (its own scroll is `follow_to`'s).
        self.live.follow = false;
        let Some(g) = self.geom.get() else {
            return;
        };
        let to = Geom::clamp(to, g.max_scroll);
        let mode = self.mode;
        let scroll = self.mode_scroll_mut(mode);
        if to != *scroll {
            *scroll = to;
            self.scrolled(cx);
            self.refresh_place(cx);
            cx.notify();
        }
    }

    /// The text scrolled, the pointer came to or left its edge, or a thumb drag ended: the
    /// scrollbar shows, and fades out on its own (M11ae). The fade task runs only after such
    /// activity, one at a time, and ends once faded out (or while hovered or dragged: held with
    /// no frames, the next poke starts another). The caller redraws.
    fn scrolled(&mut self, cx: &mut Context<Self>) {
        self.scrollbar.poke(Instant::now());
        if !self.scrollbar.begin_fade() {
            return;
        }
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(scrollbar::STEP).await;
                let go = this
                    .update(cx, |this, cx| {
                        let (redraw, go) = this.scrollbar.fade_step(Instant::now());
                        if redraw {
                            cx.notify();
                        }
                        go
                    })
                    .unwrap_or(false);
                if !go {
                    break;
                }
            }
        })
        .detach();
    }

    fn wheel(&mut self, e: &ScrollWheelEvent, cx: &mut Context<Self>) {
        let Some(g) = self.geom.get() else {
            return;
        };
        // The marks move under the pointer: the next move shows the tooltip again.
        self.hover_mark(None, cx);
        let d = e.delta.pixel_delta(g.line_height);
        self.set_scroll(point(g.scroll.x - d.x, g.scroll.y - d.y), cx);
    }

    /// Wrap's cells per row as last drawn (none: Wrap off, or not drawn yet).
    fn wrap_columns(&self) -> Option<usize> {
        self.rows
            .borrow()
            .as_ref()
            .and_then(|(_, map)| map.columns())
    }

    /// The line drawn at the top of the text, as last drawn (none: not drawn yet).
    fn top_line(&self) -> Option<usize> {
        let g = self.geom.get()?;
        let rows = self.rows.borrow();
        let (_, map) = rows.as_ref()?;
        match map.row(map.row_at(f32::from(g.scroll.y)))? {
            Row::Line { line, .. } => Some(line),
            _ => None,
        }
    }

    /// The place under window point `p` (the nearest character edge; above or below the text,
    /// its first or last line).
    fn pos_at(&self, p: Point<Pixels>, window: &mut Window, cx: &App) -> Option<Pos> {
        let g = self.geom.get()?;
        let doc = self.shown_doc()?;
        let (_, map) = self.rows.borrow().clone()?;
        // The row drawn there (M11ah: by the row map; above or below the text, the first or last).
        let row = map.row_at(f32::from(p.y - g.bounds.top() + g.scroll.y));
        let Some(Row::Line { line, cell0 }) = map.row(row) else {
            return None;
        };
        // A map a version behind (a reload not drawn yet) may know more lines than the text.
        let line = line.min(doc.lines.len().saturating_sub(1));
        // Wrapped (M11x part 2): in cells from the row's start, kept in the row (as it's drawn).
        if let Some(columns) = map.columns() {
            let span = wrapped_row(doc, line, cell0, columns);
            let last =
                !matches!(map.row(row + 1), Some(Row::Line { line: next, .. }) if next == line);
            let x = p.x - g.bounds.left() - g.text_left;
            let col = row_hit(doc, line, span, last, x / g.cell_width.max(px(1.0)));
            return Some(Pos { line, col });
        }
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
        // M11ae: a press in the scrollbar's edge zone works the thumb and never selects: on it,
        // a drag keeping the grab point under the pointer; off it, the thumb first jumps to
        // centre on the press (then drags on).
        if let Some(track) = self.geom.get().and_then(|g| zone_at(&g, e.position)) {
            let (at, t) = (track.along(e.position), track.thumb);
            let grab = if (t.start..=t.start + t.len).contains(&at) {
                at - t.start
            } else {
                let to = track.scroll_for(at - t.len / 2.0, self.mode_scroll());
                self.set_scroll(to, cx);
                t.len / 2.0
            };
            self.scrollbar.start_drag(track.axis, grab);
            cx.notify();
            return;
        }
        // M11ah: the sticky line scrolls back to its line (the top line, current); what it
        // covers takes no press.
        if let Some(line) = self.sticky_under(e.position) {
            self.show_at_top(Some(line), cx);
            return;
        }
        // A change bar (or a deleted-lines ▸) in a file: its diff, at that change.
        if let Some(change) = self.bar_under(e.position) {
            self.choose_mode(FileMode::Diff, Some(change), cx);
            return;
        }
        // M11ah: a block's ▾ folds it; a folded line's ▸ or its pill unfolds it.
        if let Some(start) = self.fold_under(e.position) {
            let mut folded = self.folded.clone();
            if !folded.remove(&start) {
                folded.insert(start);
            }
            self.set_folds(folded, cx);
            return;
        }
        if let Some(pos) = self.pos_at(e.position, window, cx) {
            self.selection = Some((pos, pos));
            self.selecting = true;
            // The last drag's pointer is not this one's (an old auto-scroll task ends).
            self.drag_at = None;
            cx.notify();
        }
    }

    /// M11ah: the block whose fold is under window point `p` (File mode, as last drawn): in the
    /// fold column, on the first row of a block's first line; or on a folded line's pill.
    fn fold_under(&self, p: Point<Pixels>) -> Option<usize> {
        if !self.folds_apply() {
            return None;
        }
        let g = self.geom.get()?;
        let left = g.fold_left?;
        let outline = self.doc()?.outline.clone()?;
        let rows = self.rows.borrow();
        let (_, map) = rows.as_ref()?;
        let y = f32::from(p.y - g.bounds.top() + g.scroll.y);
        if !g.bounds.contains(&p) || y >= map.height() {
            return None;
        }
        let r = map.row_at(y);
        let Some(Row::Line { line, cell0 }) = map.row(r) else {
            return None;
        };
        let x = p.x - g.bounds.left();
        let in_column = x >= left && x < left + px(code::FOLD_COLUMN);
        if in_column && cell0 == 0 && outline.starting_at(line).is_some() {
            return Some(line);
        }
        // The pill: after the text on the folded line's last row.
        let hidden = map.hidden_after(line);
        let last = !matches!(map.row(r + 1), Some(Row::Line { line: l, .. }) if l == line);
        if hidden.is_empty() || !last {
            return None;
        }
        let doc = self.doc()?;
        let width = doc.widths.get(line).copied().unwrap_or(0);
        let text_x = match map.columns() {
            Some(_) => g.text_left - g.cell_width * cell0 as f32,
            None => g.text_left - g.scroll.x,
        };
        let (x0, w) = fold_pill_span(text_x, g.cell_width, width, hidden.len());
        (x >= x0 && x < x0 + w).then_some(line)
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
        let rows = self.rows.borrow();
        bar_at_rows(y, &rows.as_ref()?.1, &doc.changes)
    }

    /// The pointer moved anywhere in the window (`over` the text, not covered): a thumb drag
    /// follows it; near the text's edge the scrollbar shows (M11ae); a selection grows.
    fn mouse_move(
        &mut self,
        e: &MouseMoveEvent,
        over: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some((axis, grab)) = self.scrollbar.drag() {
            if e.pressed_button != Some(MouseButton::Left) {
                self.mouse_up(cx);
            } else if let Some(track) = self.geom.get().and_then(|g| Track::of(&g, axis)) {
                let to = track.scroll_for(track.along(e.position) - grab, self.mode_scroll());
                self.set_scroll(to, cx);
            }
            return;
        }
        let hot = over
            && self
                .geom
                .get()
                .is_some_and(|g| zone_at(&g, e.position).is_some());
        if self.scrollbar.set_hot(hot) {
            self.scrolled(cx);
            cx.notify();
        }
        // M11af: a mark's tooltip, while the pointer rests on it (not while selecting).
        let resting = over && !self.selecting && e.pressed_button.is_none();
        self.hover_mark(resting.then_some(e.position), cx);
        // M11ah: over the gutter, the open blocks' ▾ show.
        let in_gutter = over
            && self
                .geom
                .get()
                .filter(|g| g.fold_left.is_some())
                .is_some_and(|g| e.position.x < g.bounds.left() + g.text_left);
        self.hover_gutter(in_gutter, cx);
        self.mouse_drag(e, window, cx);
    }

    /// The button released anywhere in the window: a selection or a thumb drag ends.
    fn mouse_up(&mut self, cx: &mut Context<Self>) {
        // M11ah: the selection's moving end (a press without a drag: the press) is the current
        // line.
        if self.selecting {
            let line = self.selection.map(|(_, head)| head.line);
            self.set_current(line, cx);
        }
        self.selecting = false;
        // No old auto-scroll task resumes on a quick press again.
        self.drag_at = None;
        if self.scrollbar.end_drag(Instant::now()) {
            // Idempotent: a Preview thumb's drag may end here too.
            self.preview_list.scrollbar_drag_ended();
            self.scrolled(cx);
            cx.notify();
        }
    }

    /// M11ah: the current line of the mode shown (a file's line, a diff's row) set to `line`;
    /// drawn again only when it changed.
    fn set_current(&mut self, line: Option<usize>, cx: &mut Context<Self>) {
        let slot = match self.mode {
            FileMode::Plain => &mut self.current,
            FileMode::Diff => &mut self.diff_current,
        };
        if *slot != line {
            *slot = line;
            cx.notify();
        }
    }

    /// The selection follows the pointer, held inside the text (M11ae: never into lines out of
    /// sight); out of the text, the auto-scroll starts (if not running). A move with the button
    /// up (its release missed) ends the selection, so the auto-scroll ends too.
    fn mouse_drag(&mut self, e: &MouseMoveEvent, window: &mut Window, cx: &mut Context<Self>) {
        if !self.selecting {
            return;
        }
        if e.pressed_button != Some(MouseButton::Left) {
            self.selecting = false;
            return;
        }
        self.drag_at = Some(e.position);
        self.drag_head(e.position, window, cx);
        let wrap = self.wrap_columns().is_some();
        let out = self
            .geom
            .get()
            .is_some_and(|g| edge_push(e.position, &g, wrap) != Point::default());
        if out && !self.autoscrolling {
            self.autoscroll(window, cx);
        }
    }

    /// The selection's moving end to window point `p` held inside the text (`hold_inside`), at
    /// the scroll as it is now (an auto-scroll step's is not drawn yet: `p` moved by the
    /// difference, which is how `pos_at` reads it).
    fn drag_head(&mut self, p: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(g) = self.geom.get() else {
            return;
        };
        let wrap = self.wrap_columns().is_some();
        let now = Geom::clamp(self.mode_scroll(), g.max_scroll);
        let ahead = point(
            if wrap { px(0.0) } else { now.x - g.scroll.x },
            now.y - g.scroll.y,
        );
        let held = hold_inside(p, &g, wrap);
        let Some(head) = self.pos_at(point(held.x + ahead.x, held.y + ahead.y), window, cx) else {
            return;
        };
        if let Some((anchor, old)) = self.selection
            && old != head
        {
            self.selection = Some((anchor, head));
            cx.notify();
        }
    }

    /// M11ae: selecting with the pointer out of the text, the text scrolls on its own every
    /// `AUTOSCROLL_STEP`, and the selection grows with it. One task at a time, which ends itself
    /// when the selection ends (a release, a mode switch), the pointer is back in the text, or
    /// the scroll stops (the end of the text): no timer runs otherwise.
    fn autoscroll(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.autoscrolling = true;
        cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor().timer(AUTOSCROLL_STEP).await;
                let go = cx.update(|window, cx| {
                    this.update(cx, |this, cx| this.autoscroll_step(window, cx))
                });
                match go {
                    Ok(Ok(true)) => {}
                    Ok(Ok(false)) | Ok(Err(_)) => break,
                    // The window went: the task ends (the flag with it, if the tab is still there).
                    Err(_) => {
                        this.update(cx, |this, _| this.autoscrolling = false).ok();
                        break;
                    }
                }
            }
        })
        .detach();
    }

    /// One auto-scroll step: the scroll pushed towards the pointer, the selection's moving end
    /// to the place under it held inside the text. False (the task ends): not selecting, the
    /// pointer back in the text, or the scroll didn't move.
    fn autoscroll_step(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let wrap = self.wrap_columns().is_some();
        let step = match (self.selecting, self.drag_at, self.geom.get()) {
            (true, Some(p), Some(g)) => Some((p, g)),
            _ => None,
        };
        let go = step.is_some_and(|(p, g)| {
            let push = edge_push(p, &g, wrap);
            if push == Point::default() {
                return false;
            }
            let from = Geom::clamp(self.mode_scroll(), g.max_scroll);
            self.set_scroll(point(from.x + push.x, from.y + push.y), cx);
            let moved = self.mode_scroll() != from;
            self.drag_head(p, window, cx);
            moved
        });
        if !go {
            self.autoscrolling = false;
        }
        go
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
                let top = this.top_line().unwrap_or(0);
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
        let kept = match self.mode {
            FileMode::Plain => self.current,
            FileMode::Diff => self.diff_current,
        };
        let line = step_line(&finds.matches, finds.current, kept);
        self.set_current(line, cx);
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

    /// Scroll the current match into sight: off screen (M11ah: or under the sticky line), its
    /// row `REVEAL_ABOVE` rows from the top, one more under a sticky line (`reveal_top`); and
    /// sideways when it's out of the text's width (M11ah: a fold holding it opens).
    fn reveal_current(&mut self, cx: &mut Context<Self>) {
        let Some(hit) = self
            .find
            .as_ref()
            .and_then(|f| f.finds.matches.get(f.finds.current?).cloned())
        else {
            return;
        };
        // M11ah: a match inside a fold opens it; the rows are then made again at once.
        let opened = self.mode == FileMode::Plain && self.open_folds_at(hit.line);
        let map = if opened {
            cx.notify();
            self.fresh_map(cx)
        } else {
            self.rows.borrow().as_ref().map(|(_, map)| map.clone())
        };
        let (Some(g), Some(doc), Some(map)) = (self.geom.get(), self.shown_doc(), map) else {
            return;
        };
        let (h, w) = (g.bounds.size.height, g.bounds.size.width);
        let Some(first) = map.first_row(hit.line) else {
            return;
        };
        let mut to = g.scroll;
        // The row the match starts on (Wrap: the part of its line it is in).
        let columns = map.columns();
        let row = first + columns.map_or(0, |c| doc.cell_of(hit.line, hit.cols.start) / c);
        let (top, bottom) = (px(map.top(row)), px(map.top(row + 1)));
        let covered = if self.place.sticky.is_some() {
            g.line_height
        } else {
            px(0.0)
        };
        if top < to.y + covered || bottom > to.y + h {
            to.y = px(map.top(reveal_top(row, |r| self.sticky_at_row(&map, r))));
        }
        // Wrapped (M11x part 2): no sideways scroll.
        if columns.is_some() {
            self.set_scroll(to, cx);
            self.refresh_place(cx);
            return;
        }
        let x = |col: usize| g.cell_width * doc.cell_of(hit.line, col) as f32;
        let (x0, x1) = (x(hit.cols.start), x(hit.cols.end));
        let room = w - g.text_left - px(RIGHT_PAD);
        if x0 < to.x || x1 > to.x + room {
            to.x = (x0 + x1) / 2.0 - room / 2.0;
        }
        self.set_scroll(to, cx);
        // M11ah: a fold opened with no scroll needed: the place still moves.
        self.refresh_place(cx);
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

    /// A 24 pt icon button with a tooltip; `on` (Wrap on) filled with `card_selected`, else
    /// `panel_hover` on hover.
    fn icon_button(
        id: &'static str,
        kind: Glyph,
        tip: &'static str,
        on: bool,
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
            .when(on, |b| b.bg(hsla(ui.card_selected)))
            .when(!on, |b| b.hover(move |s| s.bg(hsla(ui.panel_hover))))
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

    /// One of the view switch's segments (M11ae): the current one `card_selected` in `text`,
    /// the others `panel` in `text_dim` (`panel_hover` on hover); Preview over its limit faint,
    /// doing nothing, its tooltip saying why. A click shows its view (`choose_view`).
    fn view_segment(
        &self,
        view: View,
        (first, last): (bool, bool),
        current: bool,
        ui: UiColors,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let fits = preview_fits(&self.state);
        let (id, label, tip) = match view {
            View::File => ("file-view-file", TOGGLE_FILE, TIP_FILE),
            View::Diff => ("file-view-diff", TOGGLE_DIFF, TIP_DIFF),
            View::Preview if fits => ("file-view-preview", PREVIEW, TIP_PREVIEW),
            View::Preview => ("file-view-preview", PREVIEW, TIP_PREVIEW_TOO_LARGE),
        };
        let faint = view == View::Preview && !fits;
        let ink = match (current, faint) {
            (true, _) => ui.text,
            (false, true) => ui.text_faint,
            (false, false) => ui.text_dim,
        };
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
            .when(first, |d| d.rounded_l(inner))
            .when(last, |d| d.rounded_r(inner))
            .when(!first, |d| {
                d.border_l_1().border_color(hsla(ui.panel_border))
            })
            .bg(hsla(if current { ui.card_selected } else { ui.panel }))
            .text_color(hsla(ink))
            .when(!current && !faint, |d| {
                d.cursor_pointer()
                    .hover(move |s| s.bg(hsla(ui.panel_hover)))
            })
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                this.choose_view(view, window, cx)
            }))
            .tooltip(move |_, cx| -> AnyView { cx.new(|_| Tip(tip.into())).into() })
            .child(label)
            // Where it is (the scenes click it).
            .child(
                canvas(
                    move |b, _, _| {
                        let mut all = at.get();
                        all[view.index()] = Some(b);
                        at.set(all);
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
    }

    /// The view switch (M11ae): `views` as joined segments, a 1 pt border, 5 pt corners.
    fn view_switch(&self, views: &[View], ui: UiColors, cx: &mut Context<Self>) -> AnyElement {
        let current = current_view(self.mode, self.preview_shown());
        let last = views.len() - 1;
        let segments: Vec<_> = views
            .iter()
            .enumerate()
            .map(|(i, &view)| self.view_segment(view, (i == 0, i == last), view == current, ui, cx))
            .collect();
        div()
            .flex()
            .flex_none()
            .flex_row()
            .h(px(BUTTON))
            .rounded(px(5.0))
            .border_1()
            .border_color(hsla(ui.panel_border))
            .text_size(px(text::NAME))
            .children(segments)
            .into_any_element()
    }

    /// The bar's title (M11ae): the file's name, and its change chip once git answered
    /// (`chip_label`) with whether the file is new to git.
    fn title_words(&self) -> (String, Option<(String, bool)>) {
        let name = self.path.file_name().map_or_else(
            || self.path.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        );
        let counts = self.doc().and_then(|d| d.counts);
        let chip = chip_label(counts).zip(counts.map(|(_, _, new)| new));
        (name, chip)
    }

    /// The bar's second line (M11ae, note 80), part by part: Where; the Type, once coloured
    /// (`Doc::syntax`; none too large to colour); the Lines, with a text shown; Edited, with a
    /// time from the disk.
    fn meta_parts(&self) -> Vec<String> {
        let place = self
            .git_place
            .as_ref()
            .map(|p| (p.root.as_path(), p.rel.as_path()));
        let home = crate::workspace::home_dir();
        let doc = self.doc();
        let mut parts = vec![where_label(&self.path, place, home.as_deref())];
        parts.extend(doc.and_then(|d| d.syntax.as_deref()).map(str::to_string));
        parts.extend(doc.map(|d| lines_label(d.lines.len())));
        parts.extend(self.age.clone());
        parts
    }

    /// The change chip (M11ae): `M` in the change bars' blue or `A` in their green, the counts
    /// after it in `text_dim`; `text::META` on `panel`, a 1 pt `panel_border`, fully rounded.
    fn chip(words: String, new: bool, theme: &Theme, ui: UiColors) -> AnyElement {
        let change = if new {
            LineChange::Added
        } else {
            LineChange::Changed
        };
        let letter = HighlightStyle {
            color: Some(hsla(BarColours::of(theme).of_change(change))),
            ..Default::default()
        };
        let first = words.chars().next().map_or(0, char::len_utf8);
        div()
            .flex()
            .flex_none()
            .items_center()
            .px(px(6.0))
            .rounded_full()
            .border_1()
            .border_color(hsla(ui.panel_border))
            .bg(hsla(ui.panel))
            .text_size(px(text::META))
            .text_color(hsla(ui.text_dim))
            .whitespace_nowrap()
            .child(StyledText::new(words).with_highlights([(0..first, letter)]))
            .into_any_element()
    }

    /// M11af: the Follow pill, in File mode once an agent has changed the file: "Follow claude"
    /// (off) or "● Following claude" (on: the warm accent tinted), named from the newest agent
    /// mark. A click turns it the other way.
    fn follow_pill(&self, ui: UiColors, cx: &mut Context<Self>) -> Option<AnyElement> {
        self.follow_at.set(None);
        if self.mode != FileMode::Plain || self.preview_shown() {
            return None;
        }
        let name = self.live.agent_name()?;
        let on = self.live.follow;
        let words = if on {
            format!("\u{25cf} Following {name}")
        } else {
            format!("Follow {name}")
        };
        let ink = ensure_contrast(WARM_RGB, ui.panel, TEXT_MIN);
        let at = self.follow_at.clone();
        Some(
            div()
                .id("file-follow")
                .relative()
                .flex_none()
                .px(px(8.0))
                .py(px(2.0))
                .rounded(px(999.0))
                .text_size(px(text::META))
                .cursor_pointer()
                .whitespace_nowrap()
                .when(on, |p| {
                    p.bg(hsla(WARM_RGB).opacity(FOLLOW_TINT))
                        .text_color(hsla(ink))
                })
                .when(!on, |p| {
                    p.text_color(hsla(ui.text_dim))
                        .hover(move |s| s.bg(hsla(ui.panel_hover)))
                })
                .child(SharedString::from(words))
                .child(
                    canvas(move |b, _, _| at.set(Some(b)), |_, _, _, _| {})
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full(),
                )
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.live.follow = !this.live.follow;
                    cx.notify();
                }))
                .into_any_element(),
        )
    }

    /// The bar (M11ae, option B): on the left the title, the file's name (semibold) and its
    /// change chip over where it is and what it is (`meta_parts`, Where cut first when narrow);
    /// a note; on the right the view switch (two segments or more), Wrap and Open in Editor.
    fn toolbar(&self, theme: &Theme, ui: UiColors, cx: &mut Context<Self>) -> AnyElement {
        let note = match self.state {
            State::PlainTooLarge(_) => Some(TOO_LARGE),
            _ => None,
        };
        let has_text = matches!(self.state, State::Text(_) | State::PlainTooLarge(_));
        let markdown = has_text && is_markdown(&self.path);
        let views = view_segments(self.git_place.is_some(), self.mode, markdown);
        // Only segments drawn this frame have bounds.
        self.toggle_at.set([None; 3]);
        let switch = (views.len() >= 2).then(|| self.view_switch(&views, ui, cx));
        // Wrap (M11x part 2), wherever there's text to wrap: filled when on; a click turns it
        // the other way for every file tab (`file-wrap`, kept in the settings).
        let wrap = has_text.then(|| {
            let on = cx.global::<Appearance>().file_wrap;
            Self::icon_button(
                "file-wrap",
                Glyph::Wrap,
                TIP_WRAP,
                on,
                ui,
                |_, _, cx| {
                    let on = !cx.global::<Appearance>().file_wrap;
                    let values = if on { vec!["on".to_string()] } else { vec![] };
                    crate::preferences::apply(cx, "file-wrap", &values);
                },
                cx,
            )
        });
        // A refused file has its button in the middle; a missing one has none.
        let open = matches!(
            self.state,
            State::Loading | State::Text(_) | State::PlainTooLarge(_)
        )
        .then(|| {
            Self::icon_button(
                "file-open-in-editor",
                Glyph::OpenExternal,
                OPEN_IN_EDITOR,
                false,
                ui,
                |this, _, cx| this.open_in_editor(cx),
                cx,
            )
        });
        let follow = self.follow_pill(ui, cx);
        let (name, chip) = self.title_words();
        let mut meta = self.meta_parts().into_iter();
        let place = meta.next().unwrap_or_default();
        // The parts after Where, kept whole: one run, each after its ` · `.
        let rest: String = meta.map(|part| format!(" \u{b7} {part}")).collect();
        let at = self.controls_at.clone();
        let (where_at, rest_at) = (self.meta_at.clone(), self.meta_at.clone());
        self.meta_at.set([None; 2]);
        let title = div()
            .flex()
            .flex_row()
            .items_center()
            .min_w(px(0.0))
            .gap(px(6.0))
            .child(
                div()
                    .min_w(px(0.0))
                    .truncate()
                    .text_size(px(text::NAME))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(hsla(ui.text))
                    .child(SharedString::from(name)),
            )
            .children(chip.map(|(words, new)| Self::chip(words, new, theme, ui)));
        let details = div()
            .flex()
            .flex_row()
            .min_w(px(0.0))
            .text_size(px(text::META))
            .text_color(hsla(ui.text_dim))
            // Where is cut first, down to its `…` (never to nothing, so the ` · ` after it always
            // follows something): a floor of one em, wider than the ellipsis.
            .child(
                div()
                    .relative()
                    .flex_shrink(1.0)
                    .min_w(px(text::META))
                    .truncate()
                    .child(SharedString::from(place))
                    .child(
                        canvas(
                            move |b, _, _| {
                                let mut both = where_at.get();
                                both[0] = Some(b);
                                where_at.set(both);
                            },
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full(),
                    ),
            )
            .when(!rest.is_empty(), |d| {
                d.child(
                    div()
                        .relative()
                        .flex_none()
                        .whitespace_nowrap()
                        .child(SharedString::from(rest))
                        .child(
                            canvas(
                                move |b, _, _| {
                                    let mut both = rest_at.get();
                                    both[1] = Some(b);
                                    rest_at.set(both);
                                },
                                |_, _, _, _| {},
                            )
                            .absolute()
                            .top_0()
                            .left_0()
                            .size_full(),
                        ),
                )
            });
        div()
            .flex()
            .flex_none()
            .flex_row()
            .items_center()
            .w_full()
            .h(px(FILE_TOOLBAR))
            .px(px(BAR_PAD + 4.0))
            .gap(px(8.0))
            .bg(hsla(ui.panel))
            .border_b_1()
            .border_color(hsla(ui.panel_border))
            // Too narrow even with Where gone, the parts kept whole are cut at the controls.
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w(px(0.0))
                    .overflow_hidden()
                    .gap(px(1.0))
                    .child(title)
                    .child(details),
            )
            .children(note.map(|note| {
                div()
                    .flex_none()
                    .text_size(px(text::META))
                    .text_color(hsla(ui.text_faint))
                    .child(note)
            }))
            .child(
                div()
                    .relative()
                    .flex()
                    .flex_none()
                    .flex_row()
                    .items_center()
                    .gap(px(8.0))
                    .children(follow)
                    .children(switch)
                    .children(wrap)
                    .children(open)
                    // Where they are (the scenes check they stay in the pane).
                    .child(
                        canvas(move |b, _, _| at.set(Some(b)), |_, _, _, _| {})
                            .absolute()
                            .top_0()
                            .left_0()
                            .size_full(),
                    ),
            )
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
            false,
            ui,
            |this, window, cx| this.find_step(false, window, cx),
            cx,
        );
        let next = Self::icon_button(
            "file-find-next",
            Glyph::Forward,
            "Next match",
            false,
            ui,
            |this, window, cx| this.find_step(true, window, cx),
            cx,
        );
        let close = Self::icon_button(
            "file-find-close",
            Glyph::Stop,
            "Close the find bar",
            false,
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

    /// M11ah: the breadcrumb, a strip `code::BREADCRUMB` tall between the bar (and the find bar)
    /// and the text, whenever the file has blocks (File mode, not Preview: `folds_apply`), so the
    /// text never moves when its parts fill in: the file's name, then each block the top line is
    /// in, outer to inner, in `code::BREADCRUMB_TEXT` UI text, `text_dim`, the last part in
    /// `text`. Too narrow, the outer parts fold into "…" first (`fit_crumbs`, measured in the
    /// prepaint, a layout cache keyed by its inputs). A press on a part puts its line at the top.
    fn breadcrumb(&self, ui: UiColors, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.folds_apply() {
            // Not drawn: no part to press.
            self.crumbs_at.borrow_mut().clear();
            return None;
        }
        let crumbs = self.place.crumbs.clone();
        let (at, fit) = (self.crumbs_at.clone(), self.crumbs_fit.clone());
        Some(
            div()
                .flex_none()
                .w_full()
                .h(px(code::BREADCRUMB))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, e: &MouseDownEvent, _, cx| this.crumb_press(e.position, cx)),
                )
                .child(
                    canvas(
                        move |bounds, window, _| {
                            let key = CrumbFitKey {
                                crumbs,
                                room: f32::from(bounds.size.width) - 2.0 * GUTTER_LEFT,
                                font: window.text_style().font(),
                                font_size: px(code::BREADCRUMB_TEXT),
                                inks: (ui.text_dim, ui.text),
                            };
                            let laid = match fit.borrow().as_ref() {
                                Some((k, laid)) if *k == key => Some(laid.clone()),
                                _ => None,
                            };
                            let laid = laid.unwrap_or_else(|| {
                                let laid = lay_out_crumbs(&key, window);
                                *fit.borrow_mut() = Some((key, laid.clone()));
                                laid
                            });
                            let left = bounds.left() + px(GUTTER_LEFT);
                            let mut hits = Vec::new();
                            let mut boxes = Vec::new();
                            for (x, shaped, piece) in &laid {
                                let b = Bounds::new(
                                    point(left + *x, bounds.top()),
                                    size(shaped.width, bounds.size.height),
                                );
                                match piece {
                                    CrumbPiece::Part(crumb) => {
                                        let hit = HitboxBehavior::Normal;
                                        boxes.push(window.insert_hitbox(b, hit));
                                        hits.push((b, Some(crumb.clone())));
                                    }
                                    CrumbPiece::Folded => hits.push((b, None)),
                                    CrumbPiece::Separator => {}
                                }
                            }
                            *at.borrow_mut() = hits;
                            (laid, boxes)
                        },
                        move |bounds, (laid, boxes), window, cx| {
                            for b in &boxes {
                                window.set_cursor_style(CursorStyle::PointingHand, b);
                            }
                            let left = bounds.left() + px(GUTTER_LEFT);
                            let mask = ContentMask {
                                bounds: Bounds::from_corners(
                                    point(left, bounds.top()),
                                    point(bounds.right() - px(GUTTER_LEFT), bounds.bottom()),
                                ),
                            };
                            window.with_content_mask(Some(mask), |window| {
                                for (x, shaped, _) in &laid {
                                    let at = point(left + *x, bounds.top());
                                    let h = bounds.size.height;
                                    let _ = shaped.paint(at, h, TextAlign::Left, None, window, cx);
                                }
                            });
                        },
                    )
                    .size_full(),
                )
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
            current_line: match self.mode {
                FileMode::Plain => self.current,
                FileMode::Diff => self.diff_current,
            },
            guides: !is_markdown(&self.path),
            theme,
            live: (self.mode == FileMode::Plain && self.live.marked).then(|| LiveFrame {
                record: self.live.record.clone(),
                warm: self.live.warm.clone(),
                your_letter: self.live.your_letter.unwrap_or('Y'),
            }),
            folded: match self.mode {
                FileMode::Plain => self.folded.clone(),
                FileMode::Diff => BTreeSet::new(),
            },
            folds_gen: self.folds_gen,
            gutter_hover: self.gutter_hover,
            sticky: match self.place.sticky {
                Some(Sticky::Line(line)) if self.mode == FileMode::Plain => Some(line),
                _ => None,
            },
        });
        let geom = self.geom.clone();
        let row_map = self.rows.clone();
        let drawn = self.drawn.clone();
        let bar = self.scrollbar;
        let view = cx.entity();
        let keep = view.clone();
        let laid = frame.clone();
        // The text is clipped between the top and bottom rooms, so a line scrolled half out of
        // sight doesn't show in them (nor in the card's bottom corners); a press or the wheel in
        // a room still reaches the text.
        let text = div().size_full().overflow_hidden();
        // M11af: the hovered mark's tooltip, under it (as the Changes bar's, drawn in place).
        let tip = self.mark_tip.clone().filter(|_| frame.live.is_some());
        let ui = theme.ui();
        div()
            .flex_1()
            .min_h(px(0.0))
            .w_full()
            .pt(px(TOP_ROOM))
            .pb(px(BOTTOM_ROOM))
            .when_some(tip, |d, (at, words)| {
                d.child(deferred(
                    anchored()
                        .position(at)
                        .snap_to_window_with_margin(px(4.0))
                        .child(tip_box(words, &ui)),
                ))
            })
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
                            // M11ah: a file with blocks has a fold column (File mode).
                            let folds = laid.mode == FileMode::Plain
                                && laid
                                    .doc
                                    .outline
                                    .as_deref()
                                    .is_some_and(Outline::has_structure);
                            let gutter = laid.gutter.unwrap_or_else(|| {
                                Gutter::plain(count, laid.live.is_some(), folds)
                            });
                            // Wrap on (M11x part 2): rows cut at the text's width; no sideways
                            // scroll.
                            let wrap = cx.global::<Appearance>().file_wrap.then(|| {
                                let cw = f32::from(m.cell_width).max(1.0);
                                let left = columns(cw, gutter).text_left;
                                let room = f32::from(bounds.size.width) - left - RIGHT_PAD;
                                (room / cw).floor().max(1.0) as usize
                            });
                            // M11ah: the row map, a layout cache made again only for another
                            // key (mode, version, width...) or row height.
                            let row_h = row_height(m.line_height);
                            let key = MapKey {
                                mode: laid.mode,
                                version: laid.doc.version,
                                columns: wrap,
                                folds: laid.folds_gen,
                                layout: 0,
                                side: false,
                            };
                            let before = row_map.borrow().clone();
                            let map = match &before {
                                Some((k, map)) if *k == key && map.row_h() == f32::from(row_h) => {
                                    map.clone()
                                }
                                _ => FileTab::row_map_of(
                                    &laid.doc,
                                    laid.mode,
                                    wrap,
                                    f32::from(row_h),
                                    &laid.folded,
                                ),
                            };
                            // The rows reflowed (Wrap switched, review fix; M11ah fix round 1:
                            // another width with Wrap on, or another row height): the same line
                            // stays at the top, so the place worked out from it (the sticky line,
                            // the breadcrumb) stays right with no scroll. A map of the other mode
                            // says nothing of this one's rows.
                            let wrap_switched =
                                before.as_ref().and_then(|(_, b)| b.columns()).is_some()
                                    != wrap.is_some();
                            let reflowed = before.as_ref().is_some_and(|(k, b)| {
                                k.mode == laid.mode
                                    && (b.columns() != wrap || b.row_h() != f32::from(row_h))
                            });
                            let before = before.map(|(_, map)| map);
                            let mut scroll = laid.scroll;
                            if wrap_switched || reflowed {
                                let y = f32::from(scroll.y);
                                let top = before.as_ref().map_or_else(
                                    || (y / f32::from(row_h).max(1.0)).floor().max(0.0) as usize,
                                    |b| b.row_at(y),
                                );
                                let row = rescroll_row(top, before.as_ref(), &map);
                                let x = if wrap.is_some() { px(0.0) } else { scroll.x };
                                scroll = point(x, px(map.top(row)));
                                let mode = laid.mode;
                                keep.update(cx, |this, _| *this.mode_scroll_mut(mode) = scroll);
                            }
                            *row_map.borrow_mut() = Some((key, map.clone()));
                            let widest = if wrap.is_some() { 0 } else { laid.doc.widest };
                            let g = Geom::of(
                                bounds,
                                &m,
                                row_h,
                                px(map.height()),
                                gutter,
                                widest,
                                scroll,
                            );
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
                            // M11ah: the sticky line, over the text's first row: a press there
                            // scrolls back to its line.
                            let sticky = laid.sticky.map(|_| {
                                let row =
                                    Bounds::new(bounds.origin, size(bounds.size.width, row_h));
                                window.insert_hitbox(row, HitboxBehavior::Normal)
                            });
                            (m, g, hitbox, map, sticky)
                        },
                        move |_, (m, g, hitbox, map, sticky), window, cx| {
                            window.set_cursor_style(CursorStyle::IBeam, &hitbox);
                            paint_body(&frame, &g, &m, &map, window, cx);
                            if let (Some(line), Some(row)) = (frame.sticky, &sticky) {
                                window.set_cursor_style(CursorStyle::PointingHand, row);
                                paint_sticky(&frame, &g, &m, &map, line, window, cx);
                            }
                            // M11ae: the scrollbar's thumbs over the text (the fade read from
                            // the clock; its task redraws).
                            let now = Instant::now();
                            if bar.is_shown(now) {
                                let (alpha, ink) = (bar.alpha_at(now), frame.theme.ui().text);
                                for axis in [Axis::Vertical, Axis::Horizontal] {
                                    if let Some(t) = Track::of(&g, axis) {
                                        scrollbar::paint(
                                            window, t.area, axis, &t.thumb, alpha, ink,
                                        );
                                    }
                                }
                            }
                            // A drag (a selection's or a thumb's) keeps going outside the text,
                            // and the scrollbar shows near its edge; registered for this frame.
                            window.on_mouse_event({
                                let view = view.clone();
                                move |e: &MouseMoveEvent, phase, window, cx| {
                                    if phase == DispatchPhase::Bubble {
                                        let over = hitbox.is_hovered(window);
                                        view.update(cx, |this, cx| {
                                            this.mouse_move(e, over, window, cx)
                                        });
                                    }
                                }
                            });
                            window.on_mouse_event({
                                let view = view.clone();
                                move |_: &MouseUpEvent, phase, _, cx| {
                                    if phase == DispatchPhase::Bubble {
                                        view.update(cx, |this, cx| this.mouse_up(cx));
                                    }
                                }
                            });
                            // The pointer left the window (no last move comes): not hot.
                            window.on_mouse_event(move |_: &MouseExitEvent, phase, _, cx| {
                                if phase == DispatchPhase::Bubble {
                                    view.update(cx, |this, cx| this.pointer_left(cx));
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
    /// M11ac: the view switch's Preview segment as last drawn (M11ae), the reader's scroll, and
    /// the version parsed.
    pub fn preview_bounds(&self) -> Option<Bounds<Pixels>> {
        self.toggle_at.get()[View::Preview.index()]
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
        let rows = self.rows.borrow();
        let (_, map) = rows.as_ref()?;
        map.columns().map(|_| map.len())
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

    /// M11ae: whether the drag-select auto-scroll task runs.
    pub fn autoscrolling(&self) -> bool {
        self.autoscrolling
    }

    /// M11ae: the selection (anchor, moving end).
    pub fn selection(&self) -> Option<(Pos, Pos)> {
        self.selection
    }

    /// The scroll of the mode shown.
    pub fn scroll(&self) -> Point<Pixels> {
        self.mode_scroll()
    }

    /// M11ae: whether a scrollbar thumb is drawn now (the text overflows and the bar shows),
    /// whether its fade task runs, and the thumb along `axis` as last drawn (none: the text
    /// fits that way).
    pub fn scrollbar_shown(&self) -> bool {
        let overflows = self.thumb_bounds(Axis::Vertical).is_some()
            || self.thumb_bounds(Axis::Horizontal).is_some();
        overflows && self.scrollbar.is_shown(Instant::now())
    }

    pub fn scrollbar_animating(&self) -> bool {
        self.scrollbar.animating()
    }

    pub fn thumb_bounds(&self, axis: Axis) -> Option<Bounds<Pixels>> {
        if self.preview_shown() {
            let t = preview_track(&self.preview_list).filter(|t| t.axis == axis)?;
            return Some(scrollbar::thumb_bounds(t.area, axis, &t.thumb));
        }
        let t = Track::of(&self.geom.get()?, axis)?;
        Some(scrollbar::thumb_bounds(t.area, axis, &t.thumb))
    }

    /// The note the body shows instead of text (`MISSING`, `REFUSED`, a diff's), if any.
    pub fn shown_note(&self) -> Option<&'static str> {
        self.body_note()
    }

    /// The view switch's File, Diff and Preview segments, as last drawn (none: not drawn).
    pub fn toggle_bounds(&self) -> [Option<Bounds<Pixels>>; 3] {
        self.toggle_at.get()
    }

    /// M11ae: the bar's controls together (the switch, Wrap, Open in Editor), as last drawn.
    pub fn controls_bounds(&self) -> Option<Bounds<Pixels>> {
        self.controls_at.get()
    }

    /// M11ae: the second line's Where and the parts after it (none: no parts after it), as
    /// last drawn.
    pub fn meta_bounds(&self) -> [Option<Bounds<Pixels>>; 2] {
        self.meta_at.get()
    }

    /// M11ae: the bar's words: the name, the chip, and the second line (its parts joined by
    /// ` · `).
    pub fn header_words(&self) -> (String, Option<String>, String) {
        let (name, chip) = self.title_words();
        (
            name,
            chip.map(|(words, _)| words),
            self.meta_parts().join(" \u{b7} "),
        )
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

    /// The Preview segment's click handler, or File's for off (M11p's chrome scenes call it
    /// without the mouse).
    pub fn choose_preview_for_shots(
        &mut self,
        on: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.choose_view(if on { View::Preview } else { View::File }, window, cx);
    }

    /// The view switch's Diff handler, or a mode by hand (as above).
    pub fn choose_mode_for_shots(&mut self, mode: FileMode, cx: &mut Context<Self>) {
        self.choose_mode(mode, None, cx);
    }

    /// A frozen tab has no git place (no look-up), so Plain mode offers no Diff: give it a
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

    /// M11af: the age words drawn, each at its run's first line (as of the last poll).
    pub fn live_words(&self) -> Vec<(usize, String)> {
        self.live
            .warm
            .iter()
            .map(|w| (w.lines.start, w.words.clone()))
            .collect()
    }

    /// M11af: the hovered mark's tooltip, if one shows.
    pub fn mark_tip(&self) -> Option<String> {
        self.mark_tip.as_ref().map(|(_, words)| words.to_string())
    }

    /// M11af: a scene's live record, its glow worked out at `now`, and Follow on or off
    /// (none: as it is).
    pub fn set_live_for_shot(&mut self, record: LineRecord, now: SystemTime, follow: Option<bool>) {
        self.live.record = Rc::new(record);
        self.live.sync_marks();
        self.live.now = Some(now);
        // A scene's marks are all at or before its `now`.
        self.live.latest = Some(now);
        self.live.refresh(now);
        if let Some(follow) = follow {
            self.live.follow = follow;
        }
    }

    /// M11af: a reload by an agent, as far as Follow goes (what `loaded` does after a text).
    pub fn follow_for_shots(&mut self, cx: &mut Context<Self>) {
        self.follow_latest(cx);
    }

    /// M11af: the Follow pill as drawn (none: not shown).
    pub fn follow_pill_bounds(&self) -> Option<Bounds<Pixels>> {
        self.follow_at.get()
    }

    /// M11af: whether Follow is on.
    pub fn follow_on(&self) -> bool {
        self.live.follow
    }

    /// M11ah: the folded blocks (their first lines), the current line, and the line drawn on
    /// screen row `row` (none: not a line's, or not drawn yet).
    pub fn folded_lines(&self) -> Vec<usize> {
        self.folded.iter().copied().collect()
    }

    pub fn current_line(&self) -> Option<usize> {
        self.current
    }

    pub fn line_at_row(&self, row: usize) -> Option<usize> {
        match self.rows.borrow().as_ref()?.1.row(row)? {
            Row::Line { line, .. } => Some(line),
            _ => None,
        }
    }

    /// M11ah: the line pinned as the sticky line (none: nothing pinned), the breadcrumb's
    /// parts' words, and the pieces it drew: each part's (or the "…"'s) bounds and words.
    pub fn sticky_line(&self) -> Option<usize> {
        match self.place.sticky {
            Some(Sticky::Line(line)) => Some(line),
            _ => None,
        }
    }

    /// M11ah: the current line set (as a press there would).
    pub fn set_current_for_shots(&mut self, line: Option<usize>, cx: &mut Context<Self>) {
        self.set_current(line, cx);
    }

    /// M11ah: the line drawn at the top of the text (none: not drawn yet).
    pub fn top_line_for_shots(&self) -> Option<usize> {
        self.top_line()
    }

    pub fn crumb_words(&self) -> Vec<String> {
        self.place.crumbs.iter().map(|c| c.words.clone()).collect()
    }

    pub fn crumbs_drawn(&self) -> Vec<(Bounds<Pixels>, String)> {
        self.crumbs_at
            .borrow()
            .iter()
            .map(|(b, c)| {
                (
                    *b,
                    c.as_ref().map_or(ELLIPSIS.to_string(), |c| c.words.clone()),
                )
            })
            .collect()
    }

    /// M11ah: fold the blocks starting on `lines` (as a press on their ▾ would).
    pub fn fold_for_shots(&mut self, lines: &[usize], cx: &mut Context<Self>) {
        let mut folded = self.folded.clone();
        folded.extend(lines);
        self.set_folds(folded, cx);
    }

    /// Change bars from a list (git's answer; a scene's given list).
    pub fn set_changes(&mut self, changes: Vec<(usize, LineChange)>, cx: &mut Context<Self>) {
        if let Some(doc) = self.doc_mut() {
            let version = doc.version;
            if doc.accept_changes(version, &changes, None) {
                cx.notify();
            }
        }
    }
}

impl FileTab {
    /// Diff mode with rows and a place to show: scroll it `REVEAL_ABOVE` rows from the top
    /// (kept within the rows when drawn).
    fn reveal_diff(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
        let line = row.unwrap_or(0).saturating_sub(REVEAL_ABOVE);
        // By the row map (M11ah): that line's first screen row; Wrap on (M11x part 2): the diff
        // wrapped at the width last drawn (its own gutter may move it a column; the next frame
        // wraps it exactly).
        let columns = self
            .wrap_columns()
            .filter(|_| cx.global::<Appearance>().file_wrap);
        let map = Self::row_map_of(
            &d.doc,
            FileMode::Diff,
            columns,
            f32::from(row_height(m.line_height)),
            &self.folded,
        );
        let top = map.first_row(line).map_or(0.0, |r| map.top(r));
        self.diff_scroll = point(px(0.0), px(top));
        // M11ae: a reveal scrolls, so the scrollbar shows (this frame draws it). Not through
        // `set_scroll`: the rows aren't drawn yet, so the last drawing's limits are another
        // text's.
        self.scrolled(cx);
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
        let toolbar = self.toolbar(&theme, ui, cx);
        let find_bar = self.find_bar(ui, cx);
        let crumbs = self.breadcrumb(ui, cx);
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
            .on_action(cx.listener(|this, _: &FoldHere, _, cx| this.fold_here(true, cx)))
            .on_action(cx.listener(|this, _: &UnfoldHere, _, cx| this.fold_here(false, cx)))
            .on_action(cx.listener(|this, _: &FoldAll, _, cx| this.fold_all(true, cx)))
            .on_action(cx.listener(|this, _: &UnfoldAll, _, cx| this.fold_all(false, cx)))
            .on_action(cx.listener(|this, _: &Copy, _, cx| this.copy(cx)))
            .child(toolbar)
            .children(find_bar)
            .children(crumbs)
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
                TabKind::Terminal(_) | TabKind::Browser { .. } | TabKind::Agent { .. } => None,
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
                    // Switched File ↔ Diff by hand: kept, no longer a preview (M11t); its
                    // title in the (cached, M11p) tab bar is upright now.
                    this.pin_tab(tab, cx);
                    if this.model.set_file_mode(tab, *mode) {
                        this.save();
                    }
                }
                // M11ac: Preview turned on or off by hand: kept as well, and saved.
                FileTabEvent::Preview(on) => {
                    this.pin_tab(tab, cx);
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
            // M11af: its workspace, for guessing whose change it was.
            let ws = self.model.locate(*tab).map(|(ws, _)| ws);
            view.update(cx, |view, cx| {
                view.set_workspace(ws);
                view.set_shown(on, cx);
            });
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

    /// M11ae: the view switch offers File always, Diff in a repo (or in Diff mode), Preview for
    /// Markdown (in either mode); it shows with two segments or more.
    #[test]
    fn the_view_switch_offers_file_diff_and_preview_where_they_apply() {
        use View::{Diff, File, Preview};
        assert_eq!(
            view_segments(true, FileMode::Plain, true),
            [File, Diff, Preview]
        );
        assert_eq!(view_segments(false, FileMode::Plain, true), [File, Preview]);
        // A file outside a repo that isn't Markdown: one segment, no switch.
        assert_eq!(view_segments(false, FileMode::Plain, false), [File]);
        assert_eq!(view_segments(false, FileMode::Diff, false), [File, Diff]);
        assert_eq!(
            view_segments(false, FileMode::Diff, true),
            [File, Diff, Preview]
        );
    }

    /// M11ae: the current segment is Diff in Diff mode, Preview in File mode with Preview on
    /// (and fitting), else File.
    #[test]
    fn the_current_view_is_diff_then_preview_then_file() {
        assert_eq!(current_view(FileMode::Diff, true), View::Diff);
        assert_eq!(current_view(FileMode::Diff, false), View::Diff);
        assert_eq!(current_view(FileMode::Plain, true), View::Preview);
        assert_eq!(current_view(FileMode::Plain, false), View::File);
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
    #[allow(clippy::single_range_in_vec_init)] // `hidden`: one hidden run
    fn switching_wrap_keeps_the_top_line() {
        let doc = Doc::new("a\nbbbbbbbbbb\nc".to_string(), 0);
        let w = RowMap::file(&doc.widths, Some(4), &[], 10.0);
        let plain = RowMap::file(&doc.widths, None, &[], 10.0);
        // Rows: a | bbbb bbbb bb | c.
        assert_eq!(rescroll_row(4, Some(&w), &plain), 2);
        assert_eq!(rescroll_row(2, Some(&plain), &w), 4);
        assert_eq!(rescroll_row(2, None, &w), 4);
        // Inside line 1's rows: line 1's first row.
        assert_eq!(rescroll_row(2, Some(&w), &plain), 1);
        assert_eq!(rescroll_row(1, None, &w), 1);
        // M11ah: a line hidden in the rows made: the shown line above it.
        let hidden = RowMap::file(&doc.widths, Some(4), &[1..2], 10.0);
        assert_eq!(rescroll_row(1, Some(&plain), &hidden), 0);
        assert_eq!(rescroll_row(2, Some(&plain), &hidden), 1);
    }

    /// Review fix (M11x part 2): with Wrap on, a press finds a change bar by its rows, and a
    /// deleted-lines ▸ within half a row of its edge.
    #[test]
    fn bars_are_hit_by_their_rows_when_wrapped() {
        let doc = Doc::new("a\nbbbbbbbbbb\nc".to_string(), 0);
        let w = RowMap::file(&doc.widths, Some(4), &[], 10.0);
        let mut changes = Changes::default();
        changes.by_line.insert(1, LineChange::Changed);
        // Any of line 1's three rows (rows 1-3): line 2 (1-based), changed.
        for y in [12.0, 25.0, 39.0] {
            assert_eq!(bar_at_rows(y, &w, &changes), Some((2, LineChange::Changed)));
        }
        assert_eq!(bar_at_rows(5.0, &w, &changes), None);
        // Lines deleted after line 1 (the edge at row 4): within half a row of it, either side.
        let mut deleted = Changes::default();
        deleted.deleted.push(2);
        for y in [36.0, 44.0] {
            assert_eq!(
                bar_at_rows(y, &w, &deleted),
                Some((2, LineChange::DeletedAfter))
            );
        }
        assert_eq!(bar_at_rows(30.0, &w, &deleted), None);
        assert_eq!(bar_at_rows(-3.0, &w, &deleted), None);
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
    fn the_chip_says_modified_or_added_with_its_counts() {
        assert_eq!(
            chip_label(Some((3, 1, false))).as_deref(),
            Some("M \u{b7} +3 \u{2212}1")
        );
        assert_eq!(
            chip_label(Some((3, 0, false))).as_deref(),
            Some("M \u{b7} +3")
        );
        assert_eq!(
            chip_label(Some((0, 2, false))).as_deref(),
            Some("M \u{b7} \u{2212}2")
        );
        assert_eq!(
            chip_label(Some((12, 0, true))).as_deref(),
            Some("A \u{b7} +12")
        );
        assert_eq!(chip_label(Some((0, 0, false))), None);
        assert_eq!(chip_label(None), None);
    }

    #[test]
    fn where_starts_at_the_repo_or_home() {
        let (root, home) = (Path::new("/w/fuxx-cli"), Path::new("/Users/me"));
        assert_eq!(
            where_label(
                Path::new("/w/fuxx-cli/src/ui_shots/README.md"),
                Some((root, Path::new("src/ui_shots/README.md"))),
                Some(home)
            ),
            "fuxx-cli / src / ui_shots"
        );
        assert_eq!(
            where_label(
                Path::new("/w/fuxx-cli/a.rs"),
                Some((root, Path::new("a.rs"))),
                Some(home)
            ),
            "fuxx-cli"
        );
        assert_eq!(
            where_label(Path::new("/Users/me/Downloads/x.txt"), None, Some(home)),
            "~/Downloads"
        );
        assert_eq!(
            where_label(Path::new("/Users/me/x.txt"), None, Some(home)),
            "~"
        );
        assert_eq!(
            where_label(Path::new("/etc/hosts"), None, Some(home)),
            "/etc"
        );
    }

    #[test]
    fn lines_and_ages_read_as_note_80_says() {
        assert_eq!(lines_label(1), "1 line");
        assert_eq!(lines_label(42), "42 lines");
        assert_eq!(lines_label(12345), "12,345 lines");
        let t = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000_000);
        let ago = |s| age_label(t - Duration::from_secs(s), t);
        assert_eq!(ago(59), "edited just now");
        assert_eq!(ago(60), "edited 1 min ago");
        assert_eq!(ago(3_599), "edited 59 min ago");
        assert_eq!(ago(3_600), "edited 1 h ago");
        assert_eq!(ago(86_400), "edited 1 d ago");
        assert_eq!(ago(29 * 86_400), "edited 29 d ago");
        assert_eq!(ago(30 * 86_400), "edited 1 mo ago");
        assert_eq!(ago(365 * 86_400), "edited 1 y ago");
        assert_eq!(age_label(t + Duration::from_secs(5), t), "edited just now");
    }

    #[test]
    fn counts_for_an_older_version_are_dropped() {
        let lines = [(1, LineChange::Added)];
        // Version 4 is shown; counts asked for version 3 arrive after version 4's are taken.
        let mut doc = Doc::new("a\n".into(), 4);
        assert!(doc.accept_changes(4, &lines, Some((1, 0, false))));
        assert!(!doc.accept_changes(3, &lines, Some((9, 9, true))));
        assert_eq!(doc.counts, Some((1, 0, false)));
        // For the shown version, a new answer is taken and says it changed (counts only).
        assert!(doc.accept_changes(4, &lines, Some((2, 1, false))));
        assert_eq!(doc.counts, Some((2, 1, false)));
    }

    #[test]
    fn counts_from_the_diff_replace_the_held_ones_and_clear_the_chip_when_unchanged() {
        let mut doc = Doc::new("a\n".into(), 4);
        assert!(doc.set_counts(Some((3, 1, false))));
        assert!(!doc.set_counts(Some((3, 1, false))), "the same again");
        assert_eq!(
            chip_label(doc.counts).as_deref(),
            Some("M \u{b7} +3 \u{2212}1")
        );
        // The file reverted: the diff says unchanged, so no chip.
        assert!(doc.set_counts(Some((0, 0, false))));
        assert_eq!(chip_label(doc.counts), None);
        // Carried over a reload until the next answer.
        doc.set_counts(Some((1, 0, true)));
        let mut fresh = Doc::new("a\nb\n".into(), 5);
        fresh.carry_over(&doc, false);
        assert_eq!(chip_label(fresh.counts).as_deref(), Some("A \u{b7} +1"));
    }

    #[test]
    fn a_reload_keeps_the_counts_and_the_type_until_new_ones_land() {
        let mut old = Doc::new("a\n".into(), 1);
        old.accept_changes(1, &[(1, LineChange::Added)], Some((1, 0, false)));
        old.syntax = Some("Rust".into());
        let mut fresh = Doc::new("a\nb\n".into(), 2);
        fresh.carry_over(&old, false);
        assert_eq!(fresh.counts, Some((1, 0, false)));
        assert_eq!(fresh.syntax.as_deref(), Some("Rust"));
        // A text too large to colour has no type, but keeps its counts.
        let mut plain = Doc::new("a\nb\n".into(), 2);
        plain.carry_over(&old, true);
        assert_eq!(plain.counts, Some((1, 0, false)));
        assert_eq!(plain.syntax, None);
    }

    #[test]
    fn a_highlight_for_an_older_version_is_dropped() {
        let spans = || vec![vec![(0..10, Rgb { r: 1, g: 2, b: 3 })]];
        // Version 3 was asked to be coloured; the text changed on disk, version 4 is loaded and
        // coloured before version 3's colours arrive: those are dropped.
        let asked = Doc::new("fn a() {}\n".into(), 3).version;
        let mut doc = Doc::new("fn b() {}\n".into(), 4);
        assert!(doc.accept_spans(4, spans(), &[]));
        assert!(!doc.accept_spans(asked, vec![], &[]));
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
        assert!(doc5.accept_spans(3, spans(), &[]));
        assert_eq!(doc5.spans_version, Some(3));
        // Version 2's: older than those held now.
        assert!(!doc5.accept_spans(2, spans(), &[]));
        // Newer than the text shown can't happen, and is dropped.
        assert!(!doc5.accept_spans(6, spans(), &[]));
        assert!(doc5.accept_spans(5, spans(), &[]));
        assert_eq!(doc5.spans_version, Some(5));
        // Nothing held yet (it changed faster than its first colouring): any older version's
        // colours are taken, none newer than the text.
        let mut fresh = Doc::new("x\n".into(), 7);
        assert!(!fresh.accept_spans(8, spans(), &[]));
        assert!(fresh.accept_spans(2, spans(), &[]));
        assert_eq!(fresh.spans_version, Some(2));
        let mut bars = Doc::new("x\n".into(), 7);
        assert!(bars.accept_changes(2, &[(1, LineChange::Added)], None));
    }

    #[test]
    fn bar_runs_join_rows_of_one_kind() {
        let changes = [
            (2, LineChange::Added),
            (3, LineChange::Added),
            (4, LineChange::Added),
            (5, LineChange::Changed),
            (9, LineChange::Added),
        ];
        assert_eq!(
            bar_runs(&changes),
            vec![
                (2..5, LineChange::Added),
                (5..6, LineChange::Changed),
                (9..10, LineChange::Added),
            ]
        );
        assert!(bar_runs(&[]).is_empty());
    }

    #[test]
    fn bar_span_leaves_a_gap_only_between_kinds() {
        let gap = code::BAR_RUN_GAP;
        assert_eq!(bar_span(10.0, 40.0, false, false), (10.0, 40.0));
        // Two runs touching at y = 40: each gives up half the gap, so 2 pt shows between them.
        let (_, above_end) = bar_span(10.0, 40.0, false, true);
        let (below_start, _) = bar_span(40.0, 70.0, true, false);
        assert!((below_start - above_end - gap).abs() < f32::EPSILON);
        assert_eq!(
            bar_span(10.0, 40.0, true, true),
            (10.0 + gap / 2.0, 40.0 - gap / 2.0)
        );
    }

    #[test]
    fn the_read_makes_the_outline_and_the_colours_name_it() {
        let dir = std::env::temp_dir().join(format!("fuxx-outline-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let file = dir.join("session.rs");
        std::fs::write(&file, "impl S {\n    fn r() {\n        x();\n    }\n}\n").unwrap();
        let Loaded::Text { doc, .. } = read_file(&file, None, None) else {
            panic!("a text");
        };
        let mut doc = *doc;
        let outline = doc.outline.clone().expect("an outline");
        assert_eq!(outline.blocks.len(), 2);
        assert!(!outline.blocks[0].named, "named once the colours land");
        // The colours' named lines name the blocks that start on them.
        assert!(doc.accept_spans(doc.version, vec![], &[0, 1]));
        let named = doc.outline.as_ref().unwrap();
        assert!(named.blocks.iter().all(|b| b.named));
        assert!(!outline.blocks[0].named, "the Arc made before is untouched");
        // A file with no structure has an outline without blocks; a markdown one has headings.
        std::fs::write(&file, "one\ntwo\n").unwrap();
        let Loaded::Text { doc, .. } = read_file(&file, None, None) else {
            panic!("a text");
        };
        assert!(!doc.outline.as_ref().unwrap().has_structure());
        let md = dir.join("notes.md");
        std::fs::write(&md, "# A\n```\n# no\n```\n## B\n").unwrap();
        let Loaded::Text { doc, .. } = read_file(&md, None, None) else {
            panic!("a text");
        };
        assert_eq!(doc.outline.as_ref().unwrap().blocks.len(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_reload_carries_the_named_lines_onto_its_new_outline() {
        let lines = ["fn a() {", "    x();", "}"];
        let mut old = Doc::new("fn a() {\n    x();\n}\n".into(), 1);
        old.outline = code_outline::outline(&lines, false, code_outline::LIMIT).map(Arc::new);
        assert!(old.accept_spans(1, vec![vec![]], &[0]));
        let mut fresh = Doc::new("fn a() {\n    x();\n    y();\n}\n".into(), 2);
        let more = ["fn a() {", "    x();", "    y();", "}"];
        fresh.outline = code_outline::outline(&more, false, code_outline::LIMIT).map(Arc::new);
        fresh.carry_over(&old, false);
        assert_eq!(fresh.outline.as_ref().unwrap().named_lines(), [0]);
        assert!(fresh.outline.as_ref().unwrap().blocks[0].named);
        // A text too large to colour has no names.
        let mut plain = Doc::new("fn a() {\n    x();\n    y();\n}\n".into(), 2);
        plain.outline = code_outline::outline(&more, false, code_outline::LIMIT).map(Arc::new);
        plain.carry_over(&old, true);
        assert!(plain.outline.as_ref().unwrap().named_lines().is_empty());
        assert!(!plain.outline.as_ref().unwrap().blocks[0].named);
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
            let refused = matches!(read_file(&path, None, None), Loaded::Refuse(_));
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
        let plain = columns(
            8.0,
            Gutter::Plain {
                digits: 2,
                marks: false,
                folds: false,
            },
        );
        assert_eq!(plain.old_right, None);
        assert_eq!(plain.new_right, 32.0);
        assert_eq!(plain.bar_left, 42.0);
        assert_eq!(plain.text_left, 59.0);
        let diff = columns(
            8.0,
            Gutter::Diff {
                old_digits: 3,
                new_digits: 2,
            },
        );
        assert_eq!(diff.old_right, Some(40.0));
        assert_eq!(diff.new_right, 66.0);
        // The sign column's left, one cell wide, then the text.
        assert_eq!(diff.bar_left, 76.0);
        assert_eq!(diff.text_left, 98.0);
    }

    #[test]
    fn a_press_on_a_bar_or_a_mark_finds_its_change() {
        let (by_line, deleted) = bars(&[(3, LineChange::Changed), (6, LineChange::DeletedAfter)]);
        let changes = Changes { by_line, deleted };
        // A row a line (the identity map), 10 pt each.
        let map = RowMap::file(&[1; 10], None, &[], 10.0);
        // 0-based line 2 is (1-based) line 3.
        assert_eq!(
            bar_at_rows(25.0, &map, &changes),
            Some((3, LineChange::Changed))
        );
        assert_eq!(
            bar_at_rows(20.0, &map, &changes),
            Some((3, LineChange::Changed))
        );
        // The ▸ on the edge above 0-based line 6, from either side.
        assert_eq!(
            bar_at_rows(57.0, &map, &changes),
            Some((6, LineChange::DeletedAfter))
        );
        assert_eq!(
            bar_at_rows(63.0, &map, &changes),
            Some((6, LineChange::DeletedAfter))
        );
        // A line without a change, away from the mark.
        assert_eq!(bar_at_rows(5.0, &map, &changes), None);
        assert_eq!(bar_at_rows(45.0, &map, &changes), None);
        assert_eq!(bar_at_rows(-3.0, &map, &changes), None);
    }

    #[test]
    fn rows_are_the_line_height_times_1_2() {
        assert_eq!(row_height(px(15.0)), px(18.0));
    }

    #[test]
    fn columns_use_the_code_sizes() {
        let c = columns(
            8.0,
            Gutter::Plain {
                digits: 2,
                marks: false,
                folds: false,
            },
        );
        assert_eq!(c.new_right, 16.0 + 16.0);
        assert_eq!(c.bar_left, 32.0 + 10.0);
        assert_eq!(c.text_left, 42.0 + 3.0 + 14.0);
        let m = columns(
            8.0,
            Gutter::Plain {
                digits: 2,
                marks: true,
                folds: false,
            },
        );
        assert_eq!(
            m.text_left,
            c.text_left + 16.0,
            "the marks' column is 16 more"
        );
    }

    #[test]
    fn quiet_numbers_stay_readable() {
        for (name, theme) in crate::theme::every_theme() {
            let ui = theme.ui();
            let quiet = number_ink(&ui, theme.background, false);
            assert!(
                contrast(quiet, theme.background) >= MARK_MIN,
                "{name}: quiet numbers {:.2}",
                contrast(quiet, theme.background)
            );
            assert_ne!(quiet, ui.text, "{name}: quieter than the current number");
            assert_eq!(number_ink(&ui, theme.background, true), ui.text, "{name}");
        }
    }

    #[test]
    fn the_current_line_follows_its_line_or_clears() {
        let map = [Some(2), None, Some(3)];
        assert_eq!(carry_line(Some(&map), 0, |_| false), Some(2));
        assert_eq!(carry_line(Some(&map), 1, |_| true), None);
        assert_eq!(
            carry_line(Some(&map), 9, |_| true),
            None,
            "past the old text"
        );
        assert_eq!(carry_line(None, 4, |l| l == 4), Some(4));
        assert_eq!(carry_line(None, 4, |_| false), None);
    }

    #[test]
    fn a_find_step_sets_the_current_line_and_a_miss_keeps_it() {
        let hit = |line| Match { line, cols: 0..1 };
        let matches = [hit(3), hit(7)];
        assert_eq!(step_line(&matches, Some(1), Some(5)), Some(7));
        assert_eq!(step_line(&matches, None, Some(5)), Some(5), "no matches");
        assert_eq!(step_line(&[], None, None), None);
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
        assert_eq!(d.counts(), (1, 0, false), "one row added");
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
        assert_eq!(d.counts(), (2, 0, true), "every line added, new to git");
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
    fn diff_counts_are_the_added_and_removed_rows_not_the_hunk_headers() {
        // `-U<n>`: the header counts context lines (4 old, 5 new); the rows say +2 −1.
        let unified = "@@ -1,4 +1,5 @@\n a\n-b\n+bee\n c\n d\n+e\n";
        let state = diff_state(FileDiff::Unified(unified.into()), None, 1, None, None);
        let DiffState::Rows(d) = state else {
            panic!("rows");
        };
        assert_eq!(d.counts(), (2, 1, false));
        // Two hunks with a gap between them.
        let unified = "@@ -1,2 +1,2 @@\n-a\n+A\n b\n@@ -9,2 +9,1 @@\n i\n-j\n";
        let state = diff_state(FileDiff::Unified(unified.into()), None, 1, None, None);
        let DiffState::Rows(d) = state else {
            panic!("rows");
        };
        assert_eq!(d.counts(), (1, 2, false));
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

    /// M11ae: a drag past the text's edge scrolls faster the further out, from 1 pt a step to
    /// 3 lines; inside, not at all.
    #[test]
    fn the_auto_scroll_speeds_up_the_further_out() {
        assert_eq!(edge_speed(0.0, 18.0), 0.0);
        assert_eq!(edge_speed(-3.0, 18.0), 0.0);
        assert_eq!(edge_speed(4.0, 18.0), 1.0);
        assert_eq!(edge_speed(80.0, 18.0), 10.0);
        assert_eq!(edge_speed(2000.0, 18.0), 54.0);
    }

    /// M11ae: how far a point is out of a span: after it, positive; before it, negative.
    #[test]
    fn outside_is_signed_by_the_side() {
        assert_eq!(outside(50.0, 0.0, 100.0), 0.0);
        assert_eq!(outside(130.0, 0.0, 100.0), 30.0);
        assert_eq!(outside(-20.0, 0.0, 100.0), -20.0);
    }

    // M11af: the live record kept on each reload.

    const T0: SystemTime = SystemTime::UNIX_EPOCH;

    fn agent(name: &str) -> Who {
        Who::Agent(AgentMark {
            name: name.into(),
            letter: letter_of(name),
            rgb: None,
        })
    }

    fn doc(text: &str) -> Doc {
        Doc::new(text.to_string(), 0)
    }

    /// A read of `text` after `live`'s last one, compared on the way (as the background read does).
    fn text_read<'a>(live: &Live, doc: &'a Doc, when: SystemTime) -> Seen<'a> {
        Seen::Text {
            doc,
            compared: compare_with(live.prev().as_ref(), doc, false, LIMIT),
            when,
        }
    }

    /// The first text read marks nothing and is what the next read compares with; the next
    /// read's "since" is the look before it.
    #[test]
    fn first_read_is_a_baseline() {
        let t0 = Instant::now();
        let mut live = Live::default();
        let first = doc("a\nb\nc\n");
        assert!(compare_with(live.prev().as_ref(), &first, false, LIMIT).is_none());
        let seen = text_read(&live, &first, T0);
        let revise = live.saw(seen, t0, |_| unreachable!("nothing to decide"));
        assert!(!revise);
        assert!(!live.record.any());
        assert_eq!(live.prev().map(|p| p.text), Some(first.text.clone()));
        // A look that finds nothing new, then a change: decided over the time since that look.
        let t1 = t0 + Duration::from_secs(1);
        let t2 = t0 + Duration::from_secs(2);
        assert!(!live.saw(Seen::Unchanged, t1, |_| unreachable!()));
        let second = doc("a\nB\nc\n");
        let seen = text_read(&live, &second, T0);
        let revise = live.saw(seen, t2, |since| {
            assert_eq!(since, t1);
            (agent("claude"), Sure::Exact, false)
        });
        assert!(!revise, "an exact, finished edit is not revised");
        assert!(live.record.mark(0).is_none());
        assert_eq!(live.record.mark(1).map(|m| &m.who), Some(&agent("claude")));
        assert!(live.record.mark(2).is_none());
    }

    /// A file that goes missing keeps its marks and its last text: when it comes back, the
    /// new text is compared with that one.
    #[test]
    #[allow(clippy::single_range_in_vec_init)] // one block
    fn missing_then_back_compares_with_last_text() {
        let t0 = Instant::now();
        let mut live = Live::default();
        let one = doc("a\nb\n");
        live.saw(text_read(&live, &one, T0), t0, |_| unreachable!());
        let two = doc("a\nB\n");
        let revise = live.saw(text_read(&live, &two, T0), t0, |_| {
            (Who::You, Sure::Exact, false)
        });
        assert!(revise, "yours may be revised by a late finish");
        live.saw(Seen::Missing, t0, |_| unreachable!());
        assert_eq!(live.record.mark(1).map(|m| &m.who), Some(&Who::You));
        let back = doc("a\nB\nc\n");
        let seen = text_read(&live, &back, T0);
        let Seen::Text { compared, .. } = &seen else {
            unreachable!()
        };
        assert_eq!(
            compared.as_ref().map(|c| c.blocks.clone()),
            Some(vec![2..3])
        );
        live.saw(seen, t0, |_| (agent("agy"), Sure::Guess, false));
        assert_eq!(live.record.mark(1).map(|m| &m.who), Some(&Who::You));
        assert_eq!(live.record.mark(2).map(|m| &m.who), Some(&agent("agy")));
    }

    /// A text shown plain (too large to colour), a compare given up and a refused file all
    /// clear the marks; after a refusal the next text is a new baseline.
    #[test]
    fn plain_too_large_or_gave_up_clears_marks() {
        let t0 = Instant::now();
        let mut live = Live::default();
        let marked = |live: &mut Live| {
            let one = doc("a\nb\n");
            live.saw(text_read(live, &one, T0), t0, |_| unreachable!());
            let two = doc("a\nB\n");
            live.saw(text_read(live, &two, T0), t0, |_| {
                (Who::You, Sure::Exact, false)
            });
            assert!(live.record.any());
        };
        marked(&mut live);
        let plain = doc("a\nB\nc\n");
        assert!(compare_with(live.prev().as_ref(), &plain, true, LIMIT).is_none());
        live.saw(text_read(&live, &doc("x"), T0), t0, |_| {
            (Who::You, Sure::Exact, false)
        });
        let seen = Seen::Text {
            doc: &plain,
            compared: compare_with(live.prev().as_ref(), &plain, true, LIMIT),
            when: T0,
        };
        live.saw(seen, t0, |_| unreachable!("nothing to decide"));
        assert!(!live.record.any());
        // Given up (out of time): no answer, the marks cleared.
        let mut live = Live::default();
        marked(&mut live);
        let other = doc("q\nr\n");
        let gave_up = compare_with(live.prev().as_ref(), &other, false, Duration::ZERO);
        assert!(gave_up.is_none());
        let seen = Seen::Text {
            doc: &other,
            compared: gave_up,
            when: T0,
        };
        live.saw(seen, t0, |_| unreachable!("nothing to decide"));
        assert!(!live.record.any());
        // Refused: cleared, and the text after it is a baseline.
        let mut live = Live::default();
        marked(&mut live);
        live.saw(Seen::Refused, t0, |_| unreachable!());
        assert!(!live.record.any());
        assert!(live.prev().is_none());
    }

    /// A change is dated when fuxx saw it, unless the file's time is earlier (it changed while
    /// the tab was hidden): one seen 10 minutes after it was made is marked but not warm.
    #[test]
    fn when_is_the_mtime_if_earlier() {
        let seen = T0 + Duration::from_secs(10_000);
        let ten_min_ago = seen - Duration::from_secs(600);
        assert_eq!(change_time(seen, Some(ten_min_ago)), ten_min_ago);
        assert_eq!(change_time(seen, None), seen);
        // A file time in the future (another clock): when it was seen.
        assert_eq!(change_time(seen, Some(seen + Duration::from_secs(5))), seen);
        let t0 = Instant::now();
        let mut live = Live::default();
        let one = doc("a\nb\n");
        live.saw(text_read(&live, &one, T0), t0, |_| unreachable!());
        let two = doc("a\nB\n");
        let when = change_time(seen, Some(ten_min_ago));
        live.saw(text_read(&live, &two, when), t0, |_| {
            (Who::You, Sure::Exact, false)
        });
        assert_eq!(live.record.mark(1).map(|m| m.when), Some(ten_min_ago));
        assert!(live.record.warm_blocks(seen).is_empty());
    }

    /// Your letter: git's user.name, else the account's full name, else Y.
    #[test]
    fn your_letter_from_git_name_then_account_then_y() {
        assert_eq!(your_letter(Some("fitz gerald"), Some("Someone Else")), 'F');
        assert_eq!(your_letter(None, Some("Someone Else")), 'S');
        assert_eq!(your_letter(Some("  "), Some("Someone Else")), 'S');
        assert_eq!(your_letter(Some("--"), Some("someone")), 'S');
        assert_eq!(your_letter(None, None), 'Y');
        assert_eq!(your_letter(Some(""), Some("\n")), 'Y');
    }

    /// The tab's own lines go to the compare without their endings: a CRLF text and its LF
    /// twin, or a text with and without a last newline, have no changed lines.
    #[test]
    fn line_endings_alone_mark_nothing() {
        let prev = |d: &Doc| Prev {
            text: d.text.clone(),
            lines: d.lines.clone(),
        };
        let lf = doc("a\nb\nc\n");
        let crlf = doc("a\r\nb\r\nc\r\n");
        let c = compare_with(Some(&prev(&lf)), &crlf, false, LIMIT).unwrap();
        assert!(c.blocks.is_empty());
        assert_eq!(c.new_of_old, vec![Some(0), Some(1), Some(2)]);
        let without = doc("a\nb\nc");
        let c = compare_with(Some(&prev(&lf)), &without, false, LIMIT).unwrap();
        assert!(c.blocks.is_empty());
        let c = compare_with(Some(&prev(&without)), &crlf, false, LIMIT).unwrap();
        assert!(c.blocks.is_empty());
        assert_eq!(line_slices("a\r\nb", &[0..3, 3..4]), vec!["a", "b"]);
    }

    /// A guess, yours, or an edit still open may be revised; a late finish revises the latest
    /// change's lines to that agent, exact, and says whether anything changed.
    #[test]
    fn a_late_finish_revises_the_latest_change() {
        let t0 = Instant::now();
        let mut live = Live::default();
        let one = doc("a\nb\nc\n");
        live.saw(text_read(&live, &one, T0), t0, |_| unreachable!());
        let two = doc("A\nb\nc\n");
        assert!(live.saw(text_read(&live, &two, T0), t0, |_| (
            agent("claude"),
            Sure::Guess,
            false
        )));
        let three = doc("A\nb\nC\n");
        assert!(live.saw(text_read(&live, &three, T0), t0, |_| (
            Who::You,
            Sure::Exact,
            false
        )));
        assert!(!live.revise(None));
        let claude = || match agent("claude") {
            Who::Agent(a) => a,
            Who::You => unreachable!(),
        };
        assert!(live.revise(Some(claude())));
        let m = live.record.mark(2).unwrap();
        assert_eq!((&m.who, m.sure), (&agent("claude"), Sure::Exact));
        // The earlier change is not the latest: still a guess.
        assert_eq!(live.record.mark(0).unwrap().sure, Sure::Guess);
        // Already so: nothing changes.
        assert!(!live.revise(Some(claude())));
        // An edit still open may be; an exact, finished one isn't.
        let open = doc("A\nb\nC\nd\n");
        assert!(live.saw(text_read(&live, &open, T0), t0, |_| (
            agent("claude"),
            Sure::Exact,
            true
        )));
        let four = doc("A\nB\nC\nd\n");
        assert!(!live.saw(text_read(&live, &four, T0), t0, |_| (
            agent("claude"),
            Sure::Exact,
            false
        )));
    }

    /// A read with no changed line (a touch, a save of the same text) marks nothing, so it is
    /// not the latest change either.
    #[test]
    fn a_read_with_no_change_keeps_latest() {
        let t0 = Instant::now();
        let mut live = Live::default();
        live.saw(text_read(&live, &doc("a\nb\n"), T0), t0, |_| unreachable!());
        assert_eq!(live.latest, None);
        let later = T0 + Duration::from_secs(30);
        live.saw(text_read(&live, &doc("a\nb\n"), later), t0, |_| {
            unreachable!("nothing to decide")
        });
        assert_eq!(live.latest, None, "a touch is not a change");
        let changed = T0 + Duration::from_secs(40);
        live.saw(text_read(&live, &doc("a\nB\n"), changed), t0, |_| {
            (Who::You, Sure::Exact, false)
        });
        live.saw(
            text_read(&live, &doc("a\nB\n"), changed + Duration::from_secs(9)),
            t0,
            |_| unreachable!("nothing to decide"),
        );
        assert_eq!(live.latest, Some(changed));
    }

    /// Ruling R10: a change decided while an agent's edit was open is remembered so, with the
    /// look before it; at its 2 s check it is settled, and may be downgraded (the edit never
    /// finished ok: yours, or a guess).
    #[test]
    fn an_open_decision_is_settled_at_the_revision_turn() {
        let t0 = Instant::now();
        let t1 = t0 + Duration::from_secs(1);
        let mut live = Live::default();
        live.saw(text_read(&live, &doc("a\nb\n"), T0), t0, |_| unreachable!());
        assert!(live.saw(text_read(&live, &doc("a\nB\n"), T0), t1, |_| (
            agent("claude"),
            Sure::Exact,
            true
        )));
        let p = live.pending.expect("waits for its check");
        assert_eq!((p.at, p.since, p.open), (t1, t0, true));
        assert!(live.settle((Who::You, Sure::Exact)));
        let m = live.record.mark(1).unwrap();
        assert_eq!((&m.who, m.sure), (&Who::You, Sure::Exact));
        // Already so: nothing changes.
        assert!(!live.settle((Who::You, Sure::Exact)));
        // A guess, or yours, is not open.
        assert!(live.saw(text_read(&live, &doc("A\nB\n"), T0), t1, |_| (
            agent("agy"),
            Sure::Guess,
            false
        )));
        assert!(!live.pending.expect("waits").open);
    }

    // M11af Task 6: what the glow, the age words and the marks draw.

    /// A record of `lines` lines whose `block` changed at `when`, by `who` (`sure`).
    fn changed(
        record: &mut LineRecord,
        lines: usize,
        block: Range<usize>,
        who: Who,
        sure: Sure,
        when: SystemTime,
    ) {
        let c = Compared {
            new_of_old: (0..lines)
                .map(|i| (!block.contains(&i)).then_some(i))
                .collect(),
            blocks: vec![block],
        };
        record.apply(&c, lines, Mark { who, sure, when });
    }

    #[test]
    fn warm_runs_carry_their_first_lines_words() {
        let now = T0 + Duration::from_secs(1_000);
        let ago = |s: u64| now - Duration::from_secs(s);
        let mut r = LineRecord::baseline(14);
        changed(&mut r, 14, 1..3, Who::You, Sure::Exact, ago(2));
        changed(&mut r, 14, 5..6, Who::You, Sure::Exact, ago(20));
        changed(&mut r, 14, 8..10, agent("claude"), Sure::Guess, ago(40));
        changed(&mut r, 14, 12..13, Who::You, Sure::Exact, ago(70));
        let warm = |lines: Range<usize>, level: u8, words: &str| Warm {
            lines,
            level,
            words: words.to_string(),
        };
        assert_eq!(
            warm_runs(&r, now),
            vec![
                warm(1..3, 3, "just now"),
                warm(5..6, 2, "20 s ago"),
                warm(8..10, 1, "40 s ago"),
            ]
        );
        // A minute on, everything has cooled.
        assert!(warm_runs(&r, now + Duration::from_secs(60)).is_empty());
    }

    #[test]
    fn mark_tips_say_who_and_when() {
        let now = T0 + Duration::from_secs(10_000);
        let mark = |who: Who, sure: Sure, secs: u64| Mark {
            who,
            sure,
            when: now - Duration::from_secs(secs),
        };
        let tip = |m: Mark| mark_tip_words(&m, now);
        assert_eq!(
            tip(mark(agent("claude"), Sure::Exact, 12)),
            "claude \u{b7} 12 s ago"
        );
        assert_eq!(
            tip(mark(agent("claude"), Sure::Guess, 12)),
            "probably claude \u{b7} 12 s ago (it was working)"
        );
        assert_eq!(
            tip(mark(Who::You, Sure::Exact, 180)),
            "you \u{b7} 3 min ago"
        );
        assert_eq!(tip(mark(Who::You, Sure::Exact, 3)), "you \u{b7} just now");
        assert_eq!(
            tip(mark(agent("codex"), Sure::Guess, 7_200)),
            "probably codex \u{b7} 2 h ago (it was working)"
        );
        // A time ahead of now (a clock moved back) is just now.
        let ahead = Mark {
            who: Who::You,
            sure: Sure::Exact,
            when: now + Duration::from_secs(5),
        };
        assert_eq!(mark_tip_words(&ahead, now), "you \u{b7} just now");
    }

    /// The marks' column (16 pt) sits between the numbers and the bar, only while any line is
    /// marked; without it the gutter is as before M11af.
    #[test]
    fn the_marks_column_opens_only_while_marked() {
        let without = columns(8.0, Gutter::plain(9, false, false));
        let with = columns(8.0, Gutter::plain(9, true, false));
        assert_eq!(without.marks_left, None);
        assert_eq!(without.text_left, 59.0);
        assert_eq!(with.new_right, without.new_right);
        assert_eq!(with.marks_left, Some(37.0));
        assert_eq!(with.bar_left, without.bar_left + MARK_COLUMN);
        assert_eq!(with.text_left, without.text_left + MARK_COLUMN);
    }

    /// The poll's refresh says "draw again" only when a run's level or words changed.
    #[test]
    fn refresh_says_when_the_glow_changed() {
        let now = T0 + Duration::from_secs(1_000);
        let mut live = Live::default();
        let mut r = LineRecord::baseline(3);
        changed(&mut r, 3, 1..2, Who::You, Sure::Exact, now);
        live.record = Rc::new(r);
        assert!(
            !live.refresh(now),
            "nothing marked yet as far as it knows: skipped"
        );
        live.latest = Some(now);
        assert!(live.refresh(now), "the first glow");
        assert!(
            !live.refresh(now + Duration::from_millis(900)),
            "still just now"
        );
        assert!(live.refresh(now + Duration::from_secs(5)), "5 s ago");
        assert!(!live.refresh(now + Duration::from_millis(5_500)));
        assert!(live.refresh(now + Duration::from_secs(60)), "cooled");
        assert!(
            !live.refresh(now + Duration::from_secs(61)),
            "nothing warm: nothing to draw"
        );
    }

    /// `marked` (what each frame asks) follows the record through every look.
    #[test]
    fn marked_follows_the_record() {
        let t0 = Instant::now();
        let mut live = Live::default();
        let first = doc("a\nb\n");
        live.saw(text_read(&live, &first, T0), t0, |_| unreachable!());
        assert!(!live.marked);
        let second = doc("a\nB\n");
        live.saw(text_read(&live, &second, T0), t0, |_| {
            (Who::You, Sure::Exact, false)
        });
        assert!(live.marked && live.record.any());
        live.saw(Seen::Unchanged, t0, |_| unreachable!());
        assert!(live.marked, "a look that finds nothing new keeps the marks");
        live.saw(Seen::Refused, t0, |_| unreachable!());
        assert!(!live.marked && !live.record.any());
    }

    // M11af part 4: Follow.
    #[test]
    fn follow_scroll_is_none_when_in_view() {
        assert_eq!(follow_scroll(10..40, 10, |_| false), None);
        assert_eq!(follow_scroll(10..40, 39, |_| false), None);
    }

    #[test]
    fn follow_scroll_puts_line_at_top_rows() {
        assert_eq!(follow_scroll(10..40, 400, |_| false), Some(397));
        assert_eq!(
            follow_scroll(10..40, 40, |_| false),
            Some(37),
            "just below the view"
        );
        assert_eq!(
            follow_scroll(10..40, 2, |_| false),
            Some(0),
            "never above the first row"
        );
        assert_eq!(follow_scroll(10..40, 5, |_| false), Some(2));
        // M11ah: under a sticky line, one row more.
        assert_eq!(follow_scroll(10..40, 400, |_| true), Some(396));
    }

    #[test]
    fn follow_off_by_default() {
        let mut live = Live::default();
        assert!(!live.follow);
        // A change by an agent never turns it on.
        let t0 = Instant::now();
        live.saw(text_read(&live, &doc("a\nb\n"), T0), t0, |_| unreachable!());
        live.saw(text_read(&live, &doc("a\nB\n"), T0), t0, |_| {
            (agent("claude"), Sure::Exact, false)
        });
        assert!(!live.follow);
    }

    /// Follow acts on an agent's latest change only (yours never moves the view), at its first
    /// block's first line; clearing the marks turns it off; the pill is named from the newest
    /// agent mark.
    #[test]
    fn follow_line_and_name() {
        let t0 = Instant::now();
        let mut live = Live::default();
        live.saw(
            text_read(&live, &doc("a\nb\nc\n"), T0),
            t0,
            |_| unreachable!(),
        );
        live.saw(text_read(&live, &doc("a\nb\nC\n"), T0), t0, |_| {
            (Who::You, Sure::Exact, false)
        });
        live.follow = true;
        assert_eq!(live.follow_line(), None, "your change never moves the view");
        assert_eq!(live.agent_name(), None, "no agent mark: no pill");
        live.saw(text_read(&live, &doc("a\nB\nC\n"), T0), t0, |_| {
            (agent("codex"), Sure::Exact, false)
        });
        assert_eq!(live.follow_line(), Some(1));
        assert_eq!(live.agent_name().as_deref(), Some("codex"));
        live.follow = false;
        assert_eq!(live.follow_line(), None, "off: no scroll");
        live.follow = true;
        live.saw(Seen::Refused, t0, |_| unreachable!());
        assert!(!live.follow, "the marks cleared");
        assert_eq!(live.agent_name(), None);
    }

    // M11ah Task 5: folding in File mode.

    #[test]
    fn fold_pill_says_its_hidden_lines() {
        assert_eq!(fold_pill_words(24), "\u{22ef} 24 lines");
        assert_eq!(fold_pill_words(1204), "\u{22ef} 1,204 lines");
        assert_eq!(fold_pill_words(1), "\u{22ef} 1 line");
    }

    #[test]
    fn a_fold_never_hides_a_change() {
        use LineChange::{Added, Changed, DeletedAfter};
        // 0-based: line 2 added, line 3 changed, lines deleted above line 9.
        let (by_line, deleted) = bars(&[(3, Added), (4, Changed), (9, DeletedAfter)]);
        let changes = Changes { by_line, deleted };
        let change = |hidden: Range<usize>| folded_summary(hidden, &changes, None).change;
        assert_eq!(change(2..4), Some(Changed), "added and changed: changed");
        assert_eq!(change(2..3), Some(Added), "all added: added");
        assert_eq!(change(5..10), Some(DeletedAfter), "only deletions");
        assert_eq!(change(2..10), Some(Changed), "added and deleted: changed");
        assert_eq!(change(10..12), None);
        // The edge under the fold's last hidden line is the next line's: not hidden.
        assert_eq!(change(5..9), None);
        // M11af: the warmest glow and the newest mark of the hidden lines.
        let now = T0 + Duration::from_secs(1_000);
        let ago = |s: u64| now - Duration::from_secs(s);
        let mut r = LineRecord::baseline(14);
        changed(&mut r, 14, 5..6, agent("claude"), Sure::Exact, ago(40));
        changed(&mut r, 14, 8..9, Who::You, Sure::Exact, ago(2));
        changed(&mut r, 14, 11..12, agent("agy"), Sure::Guess, ago(30));
        let live = LiveFrame {
            warm: Rc::new(warm_runs(&r, now)),
            record: Rc::new(r.clone()),
            your_letter: 'Y',
        };
        let all = folded_summary(4..12, &Changes::default(), Some(&live));
        assert_eq!(all.glow, Some(3), "levels 1 and 3: 3");
        assert_eq!(all.mark.as_ref(), r.mark(8), "the newest mark");
        assert_eq!(all.change, None);
        let cool = folded_summary(4..7, &Changes::default(), Some(&live));
        assert_eq!((cool.glow, cool.mark.as_ref()), (Some(1), r.mark(5)));
        assert_eq!(
            folded_summary(12..14, &Changes::default(), Some(&live)),
            FoldSummary::default()
        );
    }

    /// Review Focus 2: a reload that removes a folded block's first line, or shrinks the file
    /// below a fold, opens it; the current line clears; nothing indexes past the end.
    #[test]
    fn reload_drops_folds_and_current_line_that_are_gone() {
        let new = ["// a", "// b", "fn a {", "    x", "}", "flat"];
        let outline = code_outline::outline(&new, false, LIMIT).expect("an outline");
        assert_eq!(outline.starting_at(2), Some(0));
        let set = |s: &[usize]| s.iter().copied().collect::<BTreeSet<usize>>();
        // Old line 0 moved down by 2 (followed); old 4 changed (opens); old 9 is past the
        // comparison's end (opens).
        let map = [Some(2), Some(3), Some(4), Some(5), None];
        assert_eq!(
            carry_folds(&set(&[0, 4, 9]), Some(&map), |_| true, &outline),
            set(&[2])
        );
        // A start that lands where the new outline has no block opens.
        assert_eq!(
            carry_folds(&set(&[1]), Some(&map), |_| true, &outline),
            set(&[])
        );
        // No comparison: kept where its line is the same, and never past the new end.
        let same = |l: usize| l < new.len() && l == 2;
        assert_eq!(carry_folds(&set(&[2, 40]), None, same, &outline), set(&[2]));
        // The current line: gone with its line, or past the new end.
        assert_eq!(carry_line(Some(&map), 4, |_| true), None);
        assert_eq!(carry_line(None, 40, same), None);
    }

    /// A fold at (0-based) line 11 hiding 12..36: the row after it is line 36, drawn `37`; in
    /// Diff mode nothing is folded.
    #[test]
    fn numbers_skip_hidden_lines() {
        let mut lines = vec!["a"; 11];
        lines.push("fn a {");
        lines.extend(["    x"; 23]);
        lines.push("}");
        lines.extend(["b"; 4]);
        let mut doc = doc(&lines.join("\n"));
        let slices: Vec<&str> = lines.clone();
        doc.outline = code_outline::outline(&slices, false, LIMIT).map(Arc::new);
        let folded: BTreeSet<usize> = [11].into();
        let map = FileTab::row_map_of(&doc, FileMode::Plain, None, 10.0, &folded);
        assert_eq!(map.row(11), Some(Row::Line { line: 11, cell0: 0 }));
        assert_eq!(map.row(12), Some(Row::Line { line: 36, cell0: 0 }));
        assert_eq!(map.hidden_after(11).len(), 24, "the pill's 24 lines");
        assert_eq!(map.len(), 40 - 24);
        // Wrap on: the same lines hidden.
        let wrapped = FileTab::row_map_of(&doc, FileMode::Plain, Some(80), 10.0, &folded);
        assert_eq!(wrapped.row(12), Some(Row::Line { line: 36, cell0: 0 }));
        // Not in Diff mode.
        let diff = FileTab::row_map_of(&doc, FileMode::Diff, None, 10.0, &folded);
        assert_eq!(diff.row(12), Some(Row::Line { line: 12, cell0: 0 }));
    }

    /// The fold column, 14 pt, between the numbers and the marks' column (numbers · fold ·
    /// marks · bar · text).
    #[test]
    fn the_fold_column_sits_between_the_numbers_and_the_marks() {
        let plain = |marks, folds| {
            columns(
                8.0,
                Gutter::Plain {
                    digits: 2,
                    marks,
                    folds,
                },
            )
        };
        let (none, fold, both) = (plain(false, false), plain(false, true), plain(true, true));
        assert_eq!(none.fold_left, None);
        assert_eq!(fold.fold_left, Some(32.0 + 5.0));
        assert_eq!(fold.bar_left, none.bar_left + 14.0);
        assert_eq!(fold.text_left, none.text_left + 14.0);
        assert_eq!(both.fold_left, Some(37.0));
        assert_eq!(both.marks_left, Some(37.0 + 14.0));
        assert_eq!(both.text_left, none.text_left + 14.0 + 16.0);
    }

    /// Ruling: a change or a deletion edge under a fold is hit on the folded line's row.
    #[test]
    #[allow(clippy::single_range_in_vec_init)] // one fold
    fn a_press_on_a_folded_line_s_bar_finds_its_hidden_change() {
        // Line 2 folded, hiding 3..7: rows 0 1 2 | 7 8 9.
        let map = RowMap::file(&[1; 10], None, &[3..7], 10.0);
        let deleted_only = Changes {
            by_line: HashMap::new(),
            deleted: vec![5],
        };
        assert_eq!(
            bar_at_rows(25.0, &map, &deleted_only),
            Some((5, LineChange::DeletedAfter))
        );
        let (by_line, deleted) = bars(&[(5, LineChange::Added), (8, LineChange::DeletedAfter)]);
        let mixed = Changes { by_line, deleted };
        // The first hidden change: (1-based) line 5, added.
        assert_eq!(
            bar_at_rows(25.0, &map, &mixed),
            Some((5, LineChange::Added))
        );
        // The edge above line 7 (the next shown row) is drawn there, as before.
        let edge = Changes {
            by_line: HashMap::new(),
            deleted: vec![7],
        };
        assert_eq!(
            bar_at_rows(29.0, &map, &edge),
            Some((7, LineChange::DeletedAfter))
        );
        assert_eq!(bar_at_rows(15.0, &map, &edge), None);
    }

    /// M11ah: a file with `impl Session` holding `fn refresh` (the outline is by indentation, so
    /// any words do) and `fn small` after it.
    const PLACE_FIXTURE: &str = "impl Session {
    fn refresh {
        let a = 1;
        if a > 0 {
            a;
        }
    }
}

fn small {
    let b = 2;
}
";

    fn place_outline(named: &[usize]) -> (Vec<&'static str>, Outline) {
        let lines: Vec<&str> = PLACE_FIXTURE.lines().collect();
        let mut outline =
            code_outline::outline(&lines, false, code_outline::LIMIT).expect("an outline");
        outline.name(named);
        (lines, outline)
    }

    fn words(crumbs: &[Crumb]) -> String {
        crumbs
            .iter()
            .map(|c| c.words.as_str())
            .collect::<Vec<_>>()
            .join(" \u{203a} ")
    }

    #[test]
    fn crumbs_go_outer_to_inner_from_the_file() {
        let (lines, outline) = place_outline(&[0, 1, 9]);
        // A top line inside `fn refresh` (its `let`; inside the `if`, the unnamed `if` is not
        // a part).
        for top in [2, 4] {
            let c = crumbs("session.rs", &outline, |i| lines[i], top, false);
            assert_eq!(
                words(&c),
                "session.rs \u{203a} impl Session \u{203a} fn refresh"
            );
            assert_eq!(
                c.iter().map(|c| c.line).collect::<Vec<_>>(),
                [None, Some(0), Some(1)],
                "the file's name goes to the top; each part to its first line"
            );
        }
        // Outside every block: the file's name alone.
        let c = crumbs("session.rs", &outline, |i| lines[i], 8, false);
        assert_eq!(words(&c), "session.rs");
        // No names yet (the colours not in): every block holding it.
        let (lines, outline) = place_outline(&[]);
        let c = crumbs("session.rs", &outline, |i| lines[i], 4, false);
        assert_eq!(
            words(&c),
            "session.rs \u{203a} impl Session \u{203a} fn refresh \u{203a} if a > 0"
        );
    }

    #[test]
    fn crumbs_follow_the_top_not_the_current_line() {
        // The breadcrumb is worked out from the line at the top (the place has no current
        // line among its inputs): two tops, two paths, whatever line is current.
        let (lines, outline) = place_outline(&[0, 1, 9]);
        let at = |top: usize| place_of("session.rs", &outline, |i| lines[i], (top, top), false);
        assert_eq!(
            words(&at(2).crumbs),
            "session.rs \u{203a} impl Session \u{203a} fn refresh"
        );
        assert_eq!(words(&at(10).crumbs), "session.rs \u{203a} fn small");
        // `top` and `under` differ (the sticky line's row): the breadcrumb takes `top`.
        let place = place_of("session.rs", &outline, |i| lines[i], (8, 10), false);
        assert_eq!(words(&place.crumbs), "session.rs");
        assert_eq!(place.sticky, Some(Sticky::Line(9)));
    }

    #[test]
    fn fit_crumbs_folds_outer_parts_first() {
        // Three parts, 60, 90 and 80 wide; " › " 20, "…" 15.
        let widths = [60.0, 90.0, 80.0];
        let fit = |room: f32| fit_crumbs(&widths, room, 15.0, 20.0);
        // All: 230 + 2 × 20 = 270.
        assert_eq!(fit(300.0), [Some(0), Some(1), Some(2)]);
        assert_eq!(fit(270.0), [Some(0), Some(1), Some(2)]);
        // The file's name into "…": 15 + 90 + 80 + 40 = 225.
        assert_eq!(fit(260.0), [None, Some(1), Some(2)]);
        // Narrow: "… › fn refresh", 15 + 20 + 80 = 115.
        assert_eq!(fit(120.0), [None, Some(2)]);
        // Very narrow: the last part still (cut where it is drawn).
        assert_eq!(fit(50.0), [Some(2)]);
        assert_eq!(fit(0.0), [Some(2)]);
        // One part, or none.
        assert_eq!(fit_crumbs(&[500.0], 10.0, 15.0, 20.0), [Some(0)]);
        assert!(fit_crumbs(&[], 10.0, 15.0, 20.0).is_empty());
    }

    #[test]
    fn reveal_top_leaves_room_for_the_sticky_line() {
        assert_eq!(reveal_top(40, |_| true), 36);
        assert_eq!(reveal_top(40, |_| false), 37);
        // Asked with the row it would put at the top.
        assert_eq!(reveal_top(40, |r| r == 37), 36);
        assert_eq!(reveal_top(40, |r| r != 37), 37);
        // Near the top: from the first row.
        assert_eq!(reveal_top(2, |_| true), 0);
        assert_eq!(reveal_top(3, |_| true), 0);
        assert_eq!(reveal_top(0, |_| false), 0);
    }

    #[test]
    fn the_sticky_line_is_none_at_its_own_first_line() {
        let (lines, outline) = place_outline(&[0, 1, 9]);
        let sticky = |under: usize| {
            place_of("session.rs", &outline, |i| lines[i], (under, under), false).sticky
        };
        // At `impl Session`'s own first line: nothing above to pin.
        assert_eq!(sticky(0), None);
        // At `fn refresh`'s first line: not itself, the block above holding it (impl).
        assert_eq!(sticky(1), Some(Sticky::Line(0)));
        // Inside `fn refresh` (the `if` is not named): fn refresh.
        assert_eq!(sticky(2), Some(Sticky::Line(1)));
        assert_eq!(sticky(4), Some(Sticky::Line(1)));
        // Between blocks, and at `fn small`'s first line: none.
        assert_eq!(sticky(8), None);
        assert_eq!(sticky(9), None);
        assert_eq!(sticky(10), Some(Sticky::Line(9)));
    }
}
