//! Command blocks (M11l): where each command, its output and its end are, kept **inside the
//! grid** as a hidden hyperlink ("tag") on column 0 of the command row's logical-line start.
//!
//! `alacritty_terminal` carries a cell's hyperlink through scrolling, the scrollback cap (the
//! evicted row is reset), reflow (cells move whole, and a logical line's first cell stays in
//! column 0), `ESC [2J` (rows pushed into scrollback) and `vt_snapshot`, and drops it on
//! `ESC [3J`, `ESC c` and any write to the cell. So a tag needs no side table that could drift.
//!
//! The tag's counts are **logical lines**: a row and its soft-wrapped continuations
//! (`WRAPLINE` on the row above's last cell) count as one, so they stay right at any width.
//!
//! `advance` replaces a bare `Processor::advance`: it splits the bytes at the end of each OSC
//! 133 mark (`shell_marks`), so the screen is exactly where the mark was sent when it is
//! applied. Nothing is written on the alternate screen or while a synchronized update is
//! being buffered.

use std::fmt::Write as _;
use std::time::{SystemTime, UNIX_EPOCH};

use alacritty_terminal::event::EventListener;
use alacritty_terminal::grid::{Dimensions, Grid};
use alacritty_terminal::index::{Column, Line, Point};
use alacritty_terminal::term::cell::{Cell, Flags, Hyperlink};
use alacritty_terminal::term::{Term, TermMode};
use alacritty_terminal::vte::ansi::{Processor, StdSyncHandler};

use crate::grid::BlockView;
use crate::shell_marks::{Mark, MarkScanner, percent_decode};

/// Every tag's URI starts with this (`links.rs` ignores such hyperlinks).
pub const URI_PREFIX: &str = "fuxx-block:";
/// The longest command kept in a tag, in bytes of UTF-8; a longer one is left out (`t=1`).
pub const CMD_CAP: usize = 2048;
/// The most prompt lines above the command row a tag records.
pub const PROMPT_LINES_CAP: u8 = 8;
/// How far up from the cursor a `C` mark looks for its prompt's tag.
pub const C_SCAN_ROWS: usize = 500;

const ID_PREFIX: &str = "fuxx-block-";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockState {
    /// A prompt is shown; nothing has run from it yet.
    Prompt,
    Running,
    Done,
}

impl BlockState {
    fn word(self) -> &'static str {
        match self {
            BlockState::Prompt => "prompt",
            BlockState::Running => "running",
            BlockState::Done => "done",
        }
    }

    fn from_word(word: &str) -> Option<Self> {
        match word {
            "prompt" => Some(BlockState::Prompt),
            "running" => Some(BlockState::Running),
            "done" => Some(BlockState::Done),
            _ => None,
        }
    }
}

/// One command's record, as kept in its tag.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tag {
    pub id: String,
    pub state: BlockState,
    /// Logical lines of prompt above the command row (`p`, at most `PROMPT_LINES_CAP`).
    pub prompt_lines: u8,
    /// Logical lines from the command row to the first output row (`c`, set at C).
    pub cmd_lines: Option<u32>,
    /// Logical lines of output (`o`, set at D).
    pub out_lines: Option<u32>,
    /// The exit code (`x`, set at D; none when unknown).
    pub exit: Option<i32>,
    /// The command line (`cmd`), at most `CMD_CAP` bytes.
    pub cmd: Option<String>,
    /// The command was longer than `CMD_CAP` and left out (`t=1`).
    pub truncated: bool,
}

/// The tag as a hyperlink: `fuxx-block:?v=1&s=…&p=…[&c=…][&o=…][&x=…][&cmd=…][&t=1]`, bytes
/// 0x21–0x7E only and never `;`, so `vt_snapshot` writes it and `vte` reads it back unchanged.
pub fn encode(tag: &Tag) -> Hyperlink {
    let mut uri = format!(
        "{URI_PREFIX}?v=1&s={}&p={}",
        tag.state.word(),
        tag.prompt_lines.min(PROMPT_LINES_CAP)
    );
    if let Some(c) = tag.cmd_lines {
        let _ = write!(uri, "&c={c}");
    }
    if let Some(o) = tag.out_lines {
        let _ = write!(uri, "&o={o}");
    }
    if let Some(x) = tag.exit {
        let _ = write!(uri, "&x={x}");
    }
    if let Some(cmd) = &tag.cmd {
        uri.push_str("&cmd=");
        for b in cmd.bytes() {
            if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
                uri.push(b as char);
            } else {
                let _ = write!(uri, "%{b:02X}");
            }
        }
    }
    if tag.truncated {
        uri.push_str("&t=1");
    }
    Hyperlink::new(Some(&tag.id), uri)
}

/// The tag a hyperlink carries, if it is one of ours (any other link, another version, or a
/// malformed one gives none). A `cmd` that doesn't decode is read as absent.
pub fn decode(link: &Hyperlink) -> Option<Tag> {
    let query = link.uri().strip_prefix(URI_PREFIX)?.strip_prefix('?')?;
    let mut version = false;
    let mut tag = Tag {
        id: link.id().to_string(),
        state: BlockState::Prompt,
        prompt_lines: 0,
        cmd_lines: None,
        out_lines: None,
        exit: None,
        cmd: None,
        truncated: false,
    };
    let mut state = None;
    for pair in query.split('&') {
        let (key, value) = pair.split_once('=')?;
        match key {
            "v" => version = value == "1",
            "s" => state = Some(BlockState::from_word(value)?),
            "p" => {
                let p = value.parse::<u32>().ok()?;
                tag.prompt_lines = p.min(u32::from(PROMPT_LINES_CAP)) as u8;
            }
            "c" => tag.cmd_lines = Some(value.parse().ok()?),
            "o" => tag.out_lines = Some(value.parse().ok()?),
            "x" => tag.exit = Some(value.parse().ok()?),
            "cmd" => tag.cmd = percent_decode(value.as_bytes()),
            "t" => tag.truncated = value == "1",
            _ => {}
        }
    }
    if !version {
        return None;
    }
    tag.state = state?;
    Some(tag)
}

/// What the tagger knows of the shell between marks (note 63, Settled 4). `C` and `D` work
/// from scans of the grid, never from this alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ShellState {
    /// Nothing known yet (a new terminal, a reattach, after a command finished).
    Unknown,
    /// Between A and the next C: `lfs` counts the line feeds from A to B; `input_start` is
    /// the cursor at B (none until B).
    Prompt {
        lfs: usize,
        input_start: Option<Point>,
    },
    Running,
}

/// Writes the tags for one terminal; lives next to its `Term`.
pub struct Tagger {
    state: ShellState,
    /// The next id's number, seeded from the clock so two taggers' ids don't collide in
    /// practice (a window's replica and the background service's session).
    next_id: u64,
    /// The last D, with the tagger `Unknown`, found no running tag to finish. Another D before
    /// any A, B or C is skipped without scanning, so a storm of D marks (hostile output) costs
    /// nothing.
    d_missed: bool,
}

impl Tagger {
    pub fn new() -> Self {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);
        Tagger {
            state: ShellState::Unknown,
            next_id: seed,
            d_missed: false,
        }
    }

    /// A prompt is shown and the cursor at its end is known (B seen, no C since).
    #[allow(dead_code)] // wired in M11l task 5
    pub fn prompt_ready(&self) -> bool {
        self.input_start().is_some()
    }

    /// Where typing started at the shown prompt (the cursor at B).
    pub fn input_start(&self) -> Option<Point> {
        match self.state {
            ShellState::Prompt { input_start, .. } => input_start,
            _ => None,
        }
    }

    fn fresh_id(&mut self) -> String {
        let n = self.next_id;
        self.next_id = n.wrapping_add(1);
        format!("{ID_PREFIX}{n}")
    }

    /// Counts the line feeds of a prompt being drawn (between A and B).
    fn saw(&mut self, bytes: &[u8]) {
        if let ShellState::Prompt {
            lfs,
            input_start: None,
        } = &mut self.state
        {
            *lfs += bytes.iter().filter(|&&b| b == b'\n').count();
        }
    }

    fn apply<L>(&mut self, term: &mut Term<L>, mark: Mark) {
        let cursor = term.grid().cursor.point;
        if !matches!(mark, Mark::Done { .. }) {
            self.d_missed = false;
        }
        match mark {
            Mark::PromptStart => {
                // The prompt always starts a hard line. zsh's PROMPT_SP, after output without
                // a final newline, prints its EOL mark and `cols - 1` spaces, then `\r`: the
                // spaces soft-wrap the output's row, so without this the prompt and command
                // row would be a continuation of the output (B would tag the output's row and
                // the earlier block would swallow the prompt). With the cursor at column 0,
                // the row above's `WRAPLINE` is cleared. This is the only grid flag fuxx
                // writes.
                let grid = term.grid_mut();
                if cursor.column == Column(0) && cursor.line > grid.topmost_line() {
                    let last = grid.last_column();
                    grid[cursor.line - 1i32][last].flags.remove(Flags::WRAPLINE);
                }
                self.state = ShellState::Prompt {
                    lfs: 0,
                    input_start: None,
                };
            }
            Mark::CommandStart => {
                let lfs = match self.state {
                    ShellState::Prompt { lfs, .. } => lfs,
                    _ => 0,
                };
                let row = line_start(term.grid(), cursor.line);
                // A B repeated with no redraw in between (a second integration) finds this
                // prompt's tag and keeps its id. A redraw overwrites column 0, which drops the
                // old tag, so the redrawn prompt gets a fresh id.
                let id = match tag_at(term.grid(), row) {
                    Some(old) if old.state == BlockState::Prompt => old.id,
                    _ => self.fresh_id(),
                };
                let tag = Tag {
                    id,
                    state: BlockState::Prompt,
                    prompt_lines: lfs.min(usize::from(PROMPT_LINES_CAP)) as u8,
                    cmd_lines: None,
                    out_lines: None,
                    exit: None,
                    cmd: None,
                    truncated: false,
                };
                write_tag(term, row, &tag);
                self.state = ShellState::Prompt {
                    lfs,
                    input_start: Some(cursor),
                };
            }
            Mark::OutputStart { cmd, truncated } => {
                self.state = ShellState::Running;
                let grid = term.grid();
                let Some((row, mut tag)) = nearest_tag(grid, cursor.line, C_SCAN_ROWS) else {
                    return;
                };
                match tag.state {
                    BlockState::Prompt => {
                        tag.state = BlockState::Running;
                        tag.cmd_lines = Some(count_lines(grid, row, cursor.line));
                        tag.cmd = cmd;
                        tag.truncated = truncated;
                    }
                    // A second C (another integration) only fills a missing command.
                    BlockState::Running
                        if tag.cmd.is_none() && !tag.truncated && (cmd.is_some() || truncated) =>
                    {
                        tag.cmd = cmd;
                        tag.truncated = truncated;
                    }
                    _ => return,
                }
                write_tag(term, row, &tag);
            }
            Mark::Done { exit } => {
                if self.state == ShellState::Unknown && self.d_missed {
                    return;
                }
                self.state = ShellState::Unknown;
                let grid = term.grid();
                let rows = grid.history_size() + grid.screen_lines();
                let found = nearest_tag(grid, cursor.line, rows);
                let Some((row, mut tag)) = found.filter(|(_, t)| t.state == BlockState::Running)
                else {
                    self.d_missed = true;
                    return;
                };
                let out_start = lines_down(grid, row, tag.cmd_lines.unwrap_or(1));
                // Output ends above the cursor, or on its row when something is printed there
                // (output without a final newline).
                let end = cursor.line + i32::from(cursor.column > Column(0));
                tag.state = BlockState::Done;
                tag.out_lines = Some(count_lines(grid, out_start, end));
                tag.exit = exit;
                write_tag(term, row, &tag);
            }
        }
    }
}

/// Feeds a terminal's raw output to its `Term`, writing the tags its OSC 133 marks call for:
/// the processor is advanced exactly to the end of each mark before the mark is applied
/// (`vte` carries its state across calls, so the screen is the same as unsplit).
pub fn advance<L: EventListener>(
    processor: &mut Processor<StdSyncHandler>,
    term: &mut Term<L>,
    tagger: &mut Tagger,
    scanner: &mut MarkScanner,
    bytes: &[u8],
) {
    let mut from = 0;
    for (end, mark) in scanner.feed(bytes) {
        let part = &bytes[from..end];
        processor.advance(term, part);
        tagger.saw(part);
        // On the alternate screen, or inside a synchronized update (the cursor isn't where
        // the bytes say yet), the mark is dropped.
        if !term.mode().contains(TermMode::ALT_SCREEN) && processor.sync_bytes_count() == 0 {
            tagger.apply(term, mark);
        }
        from = end;
    }
    let rest = &bytes[from..];
    processor.advance(term, rest);
    tagger.saw(rest);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Ok,
    Failed(i32),
    /// Finished, exit code not reported.
    Unknown,
    Running,
}

/// One command, read from its tag. Rows are the grid's lines (negative in scrollback).
#[derive(Clone, Debug, PartialEq)]
pub struct Block {
    pub id: String,
    pub outcome: Outcome,
    pub prompt_top: Line,
    pub cmd_row: Line,
    /// First and last output rows; none when there is no output.
    pub output: Option<(Line, Line)>,
    pub cmd: Option<String>,
}

/// Every running or finished command still in the grid, oldest first; none on the alternate
/// screen.
#[allow(dead_code)] // wired in M11l task 4
pub fn list<L>(term: &Term<L>) -> Vec<Block> {
    if term.mode().contains(TermMode::ALT_SCREEN) {
        return Vec::new();
    }
    let grid = term.grid();
    let cursor = grid.cursor.point.line;
    let (top, bottom) = (grid.topmost_line().0, grid.bottommost_line().0);
    (top..=bottom)
        .map(Line)
        .filter_map(|row| Some((row, tag_at(grid, row)?)))
        .filter(|(_, tag)| tag.state != BlockState::Prompt)
        .map(|(row, tag)| block(grid, row, tag, cursor))
        .collect()
}

/// The top row of the prompt shown now: the newest tag's, if it is a prompt's. Looks from the
/// bottom of the screen to `C_SCAN_ROWS` above the cursor (as far as a `C` looks for its
/// prompt), so it is cheap enough for every frame.
#[allow(dead_code)] // drawn in M11l task 4 (through `visible`)
pub fn live_prompt<L>(term: &Term<L>) -> Option<Line> {
    if term.mode().contains(TermMode::ALT_SCREEN) {
        return None;
    }
    let grid = term.grid();
    let bottom = grid.bottommost_line();
    let below_cursor = (bottom.0 - grid.cursor.point.line.0).max(0) as usize;
    let (row, tag) = nearest_tag(grid, bottom, below_cursor + C_SCAN_ROWS)?;
    (tag.state == BlockState::Prompt).then(|| lines_up(grid, row, tag.prompt_lines.into()))
}

/// The output of the block `id`, lines right-trimmed and trailing empty lines dropped (empty
/// when it has none); none when no such block is in the grid.
#[allow(dead_code)] // wired in M11l task 5
pub fn output_text<L>(term: &Term<L>, id: &str) -> Option<String> {
    let block = list(term).into_iter().find(|b| b.id == id)?;
    let Some((first, last)) = block.output else {
        return Some(String::new());
    };
    let start = Point::new(first, Column(0));
    let end = Point::new(last, term.grid().last_column());
    let raw = term.bounds_to_string(start, end);
    let mut lines: Vec<&str> = raw.split('\n').map(str::trim_end).collect();
    while lines.last() == Some(&"") {
        lines.pop();
    }
    Some(lines.join("\n"))
}

/// Rows under the viewport whose tags `visible` also reads: a block's prompt can be shown
/// while its tag (on the command row) is still just under the view.
const BELOW_VIEW: i32 = 8;

/// The blocks to draw, oldest first, and the live prompt's top, in viewport rows (see
/// `BlockView`). Reads column 0 of the viewport and `BELOW_VIEW` rows under it, plus, for the
/// `chosen` block only, the whole scrollback. Nothing on the alternate screen.
#[allow(dead_code)] // drawn in M11l task 4
pub fn visible<L>(term: &Term<L>, chosen: Option<&str>) -> (Vec<BlockView>, Option<i32>) {
    if term.mode().contains(TermMode::ALT_SCREEN) {
        return (Vec::new(), None);
    }
    let grid = term.grid();
    let offset = grid.display_offset() as i32;
    let top = -offset;
    let bottom = (top + grid.screen_lines() as i32 - 1 + BELOW_VIEW).min(grid.bottommost_line().0);
    let mut found: Vec<(Line, Tag)> = (top..=bottom)
        .map(Line)
        .filter_map(|row| Some((row, tag_at(grid, row)?)))
        .filter(|(_, tag)| tag.state != BlockState::Prompt)
        .collect();
    if let Some(id) = chosen
        && !found.iter().any(|(_, tag)| tag.id == id)
        && let Some(hit) = find_tag(grid, id).filter(|(_, tag)| tag.state != BlockState::Prompt)
    {
        found.push(hit);
        found.sort_by_key(|(row, _)| *row);
    }
    let cursor = grid.cursor.point.line;
    let blocks = found
        .into_iter()
        .map(|(row, tag)| {
            // The command's rows end just above the output's first.
            let cmd_end = (lines_down(grid, row, tag.cmd_lines.unwrap_or(1)) - 1i32)
                .min(grid.bottommost_line())
                .max(row);
            let is_chosen = chosen == Some(tag.id.as_str());
            let b = block(grid, row, tag, cursor);
            BlockView {
                id: b.id,
                outcome: b.outcome,
                prompt: (b.prompt_top.0 + offset, cmd_end.0 + offset),
                output: b
                    .output
                    .map(|(first, last)| (first.0 + offset, last.0 + offset)),
                chosen: is_chosen,
            }
        })
        .collect();
    (blocks, live_prompt(term).map(|line| line.0 + offset))
}

fn block(grid: &Grid<Cell>, row: Line, tag: Tag, cursor: Line) -> Block {
    let bottom = grid.bottommost_line();
    let out_start = lines_down(grid, row, tag.cmd_lines.unwrap_or(1));
    let (outcome, output) = match tag.state {
        // A running command's output so far ends on the cursor's row.
        BlockState::Running | BlockState::Prompt => (
            Outcome::Running,
            (out_start <= cursor).then_some((out_start, cursor)),
        ),
        BlockState::Done => {
            let outcome = match tag.exit {
                Some(0) => Outcome::Ok,
                Some(x) => Outcome::Failed(x),
                None => Outcome::Unknown,
            };
            let output = match tag.out_lines {
                Some(o) if o > 0 && out_start <= bottom => {
                    let last = lines_down(grid, out_start, o - 1).min(bottom);
                    Some((out_start, line_end(grid, last)))
                }
                _ => None,
            };
            (outcome, output)
        }
    };
    Block {
        id: tag.id,
        outcome,
        prompt_top: lines_up(grid, row, tag.prompt_lines.into()),
        cmd_row: row,
        output,
        cmd: tag.cmd,
    }
}

/// The tag on column 0 of `row`, if any.
fn tag_at(grid: &Grid<Cell>, row: Line) -> Option<Tag> {
    decode(&grid[row][Column(0)].hyperlink()?)
}

fn write_tag<L>(term: &mut Term<L>, row: Line, tag: &Tag) {
    term.grid_mut()[row][Column(0)].set_hyperlink(Some(encode(tag)));
}

/// The tag whose id is `id`, anywhere in the grid (column 0 of every row; only the matching
/// one is decoded).
fn find_tag(grid: &Grid<Cell>, id: &str) -> Option<(Line, Tag)> {
    (grid.topmost_line().0..=grid.bottommost_line().0)
        .map(Line)
        .find_map(|row| {
            let link = grid[row][Column(0)].hyperlink()?;
            if link.id() != id {
                return None;
            }
            Some((row, decode(&link)?))
        })
}

/// The nearest tag at or above `from`, looking at most `rows` rows (and not past the top).
fn nearest_tag(grid: &Grid<Cell>, from: Line, rows: usize) -> Option<(Line, Tag)> {
    let lowest = (from.0 - rows as i32 + 1).max(grid.topmost_line().0);
    (lowest..=from.0)
        .rev()
        .map(Line)
        .find_map(|row| Some((row, tag_at(grid, row)?)))
}

/// `row` soft-wraps into the row below it.
fn wraps(grid: &Grid<Cell>, row: Line) -> bool {
    grid[row][grid.last_column()]
        .flags
        .contains(Flags::WRAPLINE)
}

/// The first row of the logical line `row` is in.
fn line_start(grid: &Grid<Cell>, mut row: Line) -> Line {
    while row > grid.topmost_line() && wraps(grid, row - 1) {
        row -= 1;
    }
    row
}

/// The last row of the logical line `row` is in.
fn line_end(grid: &Grid<Cell>, mut row: Line) -> Line {
    while row < grid.bottommost_line() && wraps(grid, row) {
        row += 1;
    }
    row
}

/// The first row `n` logical lines below the one at `row`; one past the last row when that
/// runs off the grid.
fn lines_down(grid: &Grid<Cell>, mut row: Line, n: u32) -> Line {
    for _ in 0..n {
        if row > grid.bottommost_line() {
            break;
        }
        row = line_end(grid, row) + 1;
    }
    row
}

/// The first row `n` logical lines above the one at `row`, stopping at the top.
fn lines_up(grid: &Grid<Cell>, mut row: Line, n: u32) -> Line {
    for _ in 0..n {
        if row <= grid.topmost_line() {
            break;
        }
        row = line_start(grid, row - 1);
    }
    row
}

/// Logical lines in the rows `from..to`, the first counted wherever it starts.
fn count_lines(grid: &Grid<Cell>, from: Line, to: Line) -> u32 {
    if from >= to {
        return 0;
    }
    let starts = (from.0 + 1..to.0)
        .filter(|&l| !wraps(grid, Line(l - 1)))
        .count();
    1 + starts as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::{TermDims, term_config};
    use alacritty_terminal::event::VoidListener;
    use alacritty_terminal::grid::Dimensions;
    use alacritty_terminal::index::Column;
    use alacritty_terminal::term::{Config, TermMode};

    const A: &[u8] = b"\x1b]133;A\x07";
    const B: &[u8] = b"\x1b]133;B\x1b\\";

    fn c(cmd: Option<&str>) -> Vec<u8> {
        match cmd {
            Some(cmd) => format!("\x1b]133;C;cmdline_url={cmd}\x07").into_bytes(),
            None => b"\x1b]133;C\x07".to_vec(),
        }
    }

    fn d(exit: Option<i32>) -> Vec<u8> {
        match exit {
            Some(x) => format!("\x1b]133;D;{x}\x07").into_bytes(),
            None => b"\x1b]133;D\x07".to_vec(),
        }
    }

    /// A Term of 20 × 6 with 50 lines of history, fed through `advance`.
    struct Shell {
        term: Term<VoidListener>,
        processor: Processor<StdSyncHandler>,
        tagger: Tagger,
        scanner: MarkScanner,
    }

    impl Shell {
        fn new() -> Self {
            let config = Config {
                scrolling_history: 50,
                ..term_config()
            };
            Shell {
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
            self.feed(A).feed(b"$ ").feed(B)
        }

        /// A prompt, `cmd` typed and run, its output, and its end.
        fn command(&mut self, cmd: &str, output: &str, exit: Option<i32>) -> &mut Self {
            self.prompt()
                .feed(cmd.as_bytes())
                .feed(b"\r\n")
                .feed(&c(Some(cmd)))
                .feed(output.as_bytes())
                .feed(&d(exit))
        }

        fn blocks(&self) -> Vec<Block> {
            list(&self.term)
        }

        fn cursor(&self) -> Point {
            self.term.grid().cursor.point
        }
    }

    /// Every tag on column 0 of the active grid, top to bottom.
    fn tags(term: &Term<VoidListener>) -> Vec<(Line, Tag)> {
        let grid = term.grid();
        let top = grid.topmost_line().0;
        let bottom = grid.bottommost_line().0;
        (top..=bottom)
            .filter_map(|l| {
                let link = grid[Line(l)][Column(0)].hyperlink()?;
                Some((Line(l), decode(&link)?))
            })
            .collect()
    }

    fn tag(state: BlockState, cmd: Option<&str>) -> Tag {
        Tag {
            id: "fuxx-block-7".into(),
            state,
            prompt_lines: 3,
            cmd_lines: Some(2),
            out_lines: Some(40),
            exit: Some(-1),
            cmd: cmd.map(String::from),
            truncated: false,
        }
    }

    #[test]
    fn tag_round_trips() {
        for t in [
            tag(BlockState::Done, Some("ls -la")),
            tag(BlockState::Running, Some("a; b & c % é \n=?")),
            Tag {
                id: "fuxx-block-1".into(),
                state: BlockState::Prompt,
                prompt_lines: 0,
                cmd_lines: None,
                out_lines: None,
                exit: None,
                cmd: None,
                truncated: true,
            },
        ] {
            let link = encode(&t);
            let uri = link.uri();
            assert!(uri.starts_with(URI_PREFIX), "{uri}");
            assert!(
                uri.bytes().all(|b| (0x21..=0x7e).contains(&b) && b != b';'),
                "{uri}"
            );
            assert_eq!(link.id(), t.id);
            assert_eq!(decode(&link), Some(t));
        }
        // Not ours, or not this version.
        let other = Hyperlink::new(Some("x"), "https://example.com".into());
        assert_eq!(decode(&other), None);
        let v2 = Hyperlink::new(Some("x"), "fuxx-block:?v=2&s=done&p=0".into());
        assert_eq!(decode(&v2), None);
        let bad = Hyperlink::new(Some("x"), "fuxx-block:?v=1&s=gone&p=0".into());
        assert_eq!(decode(&bad), None);
        // A prompt count over the cap reads as the cap.
        let big = Hyperlink::new(Some("x"), "fuxx-block:?v=1&s=prompt&p=200".into());
        assert_eq!(decode(&big).map(|t| t.prompt_lines), Some(PROMPT_LINES_CAP));
    }

    #[test]
    fn one_command_is_one_block() {
        let mut sh = Shell::new();
        sh.command("ls", "a\r\nb\r\n", Some(0)).prompt();
        let blocks = sh.blocks();
        assert_eq!(blocks.len(), 1, "{blocks:?}");
        let b = &blocks[0];
        assert_eq!(b.outcome, Outcome::Ok);
        assert_eq!(b.cmd.as_deref(), Some("ls"));
        assert_eq!(b.prompt_top, b.cmd_row);
        assert_eq!(b.cmd_row, Line(0));
        assert_eq!(b.output, Some((Line(1), Line(2))));
        assert!(b.id.starts_with("fuxx-block-"), "{}", b.id);
        assert_eq!(output_text(&sh.term, &b.id).as_deref(), Some("a\nb"));
        assert_eq!(output_text(&sh.term, "fuxx-block-nope"), None);
        assert_eq!(live_prompt(&sh.term), Some(Line(3)));
        assert_eq!(live_prompt(&sh.term), Some(sh.cursor().line));
    }

    #[test]
    fn a_two_line_prompt_and_a_wrapped_command() {
        let mut sh = Shell::new();
        let cmd = "echo 0123456789abcdefghijklmno"; // 30 chars, wraps at 20 cols
        assert_eq!(cmd.len(), 30);
        sh.feed(A)
            .feed(b"dir\r\n$ ")
            .feed(B)
            .feed(cmd.as_bytes())
            .feed(b"\r\n")
            .feed(&c(Some("echo")))
            .feed(b"out\r\n")
            .feed(&d(Some(0)));
        let blocks = sh.blocks();
        assert_eq!(blocks.len(), 1, "{blocks:?}");
        let b = &blocks[0];
        assert_eq!(b.prompt_top, b.cmd_row - 1);
        assert_eq!(b.output.map(|o| o.0), Some(b.cmd_row + 2));
        assert_eq!(output_text(&sh.term, &b.id).as_deref(), Some("out"));
        let t = &tags(&sh.term)[0].1;
        assert_eq!(
            (t.prompt_lines, t.cmd_lines, t.out_lines),
            (1, Some(1), Some(1))
        );
        // The counts are logical lines: at any width the prompt is one line up and the output
        // starts just past the command's last row.
        for cols in [12, 50, 9] {
            sh.term.resize(TermDims { cols, rows: 6 });
            let b = &sh.blocks()[0];
            // "$ " + 30 chars = 32 cells of command.
            assert_eq!(b.prompt_top, b.cmd_row - 1, "{cols} cols");
            assert_eq!(
                b.output.map(|o| o.0),
                Some(b.cmd_row + (2 + cmd.len()).div_ceil(cols) as i32),
                "{cols} cols"
            );
            assert_eq!(
                output_text(&sh.term, &b.id).as_deref(),
                Some("out"),
                "{cols} cols"
            );
        }
    }

    #[test]
    fn failed_and_unknown_exits() {
        let mut sh = Shell::new();
        sh.command("false", "\r\n", Some(2))
            .command("x", "y\r\n", None)
            .prompt()
            .feed(b"sleep\r\n")
            .feed(&c(Some("sleep")))
            .feed(b"x\r\ny");
        let blocks = sh.blocks();
        let outcomes: Vec<_> = blocks.iter().map(|b| b.outcome).collect();
        assert_eq!(
            outcomes,
            vec![Outcome::Failed(2), Outcome::Unknown, Outcome::Running]
        );
        let running = &blocks[2];
        assert_eq!(
            running.output,
            Some((running.cmd_row + 1, sh.cursor().line))
        );
        assert_eq!(output_text(&sh.term, &running.id).as_deref(), Some("x\ny"));
        // No live prompt while a command runs.
        assert_eq!(live_prompt(&sh.term), None);
    }

    #[test]
    fn duplicate_marks_are_idempotent() {
        let mut sh = Shell::new();
        // Two integrations both marking the prompt, then a redraw of it.
        sh.feed(A).feed(A).feed(b"$ ").feed(B);
        let first = tags(&sh.term);
        sh.feed(B);
        assert_eq!(
            tags(&sh.term),
            first,
            "a second B re-tags the same row, same id"
        );
        assert_eq!(first.len(), 1);
        // A redraw overwrites column 0 (dropping the old tag) and re-marks it.
        sh.feed(b"\r").feed(A).feed(b"$ ").feed(B);
        let found = tags(&sh.term);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0, first[0].0);
        assert_eq!(found[0].1.state, BlockState::Prompt);
        let id = found[0].1.id.clone();
        // A second C only fills a missing command.
        sh.feed(b"ls\r\n").feed(&c(None));
        assert_eq!(tags(&sh.term)[0].1.cmd, None);
        sh.feed(&c(Some("ls")));
        assert_eq!(tags(&sh.term)[0].1.cmd.as_deref(), Some("ls"));
        sh.feed(&c(Some("other")));
        assert_eq!(tags(&sh.term)[0].1.cmd.as_deref(), Some("ls"));
        // A second D finds a done tag and is ignored.
        sh.feed(b"a\r\n").feed(&d(Some(0))).feed(&d(Some(1)));
        let blocks = sh.blocks();
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].id, id);
        assert_eq!(blocks[0].outcome, Outcome::Ok);
        assert_eq!(blocks[0].cmd.as_deref(), Some("ls"));
    }

    #[test]
    fn tags_survive_reflow() {
        let mut sh = Shell::new();
        let long = "0123456789abcdefghijklmnopqrst"; // 30 chars
        sh.command("ls", &format!("a\r\n{long}\r\n"), Some(0))
            .prompt();
        let before = sh.blocks();
        assert_eq!(before.len(), 1);
        let id = before[0].id.clone();
        let text = output_text(&sh.term, &id);
        assert_eq!(text, Some(format!("a\n{long}")));
        for cols in [10, 40, 7, 20] {
            sh.term.resize(TermDims { cols, rows: 6 });
            let after = sh.blocks();
            assert_eq!(after.len(), 1, "{cols} cols: {after:?}");
            assert_eq!(after[0].id, id, "{cols} cols");
            assert_eq!(after[0].cmd.as_deref(), Some("ls"), "{cols} cols");
            assert_eq!(after[0].outcome, Outcome::Ok, "{cols} cols");
            assert_eq!(output_text(&sh.term, &id), text, "{cols} cols");
            assert!(live_prompt(&sh.term).is_some(), "{cols} cols");
        }
    }

    #[test]
    fn an_evicted_tag_removes_only_its_block() {
        // 56 rows in all (50 of history + 6). Blocks 1 and 2 take rows 0–5 (tags on 0 and 3),
        // block 3's command row is row 6; with 53 lines of output and the next prompt, 61 rows
        // were written: rows 0–4 are evicted, block 2's last output line (row 5) is left as a
        // plain line at the top.
        let lines = |n: usize| (0..n).map(|i| format!("{i}\r\n")).collect::<String>();
        let mut sh = Shell::new();
        sh.command("one", "a\r\nb\r\n", Some(0))
            .command("two", "a\r\nb\r\n", Some(0))
            .command("seq", &lines(53), Some(0))
            .prompt();
        let blocks = sh.blocks();
        assert_eq!(blocks.len(), 1, "{blocks:?}");
        assert_eq!(blocks[0].cmd.as_deref(), Some("seq"));
        let top = sh.term.grid().topmost_line();
        assert_eq!(blocks[0].cmd_row, top + 1);
        assert_eq!(
            sh.term
                .bounds_to_string(Point::new(top, Column(0)), Point::new(top, Column(19))),
            "b"
        );
        let text = output_text(&sh.term, &blocks[0].id).unwrap();
        assert_eq!(text.lines().count(), 53);

        // 60 lines evict the third block's own tag too: nothing is left, nothing panics.
        let mut sh = Shell::new();
        sh.command("one", "a\r\nb\r\n", Some(0))
            .command("two", "a\r\nb\r\n", Some(0))
            .command("seq", &lines(60), Some(0))
            .prompt();
        assert!(sh.blocks().is_empty(), "{:?}", sh.blocks());
        assert!(live_prompt(&sh.term).is_some());
    }

    #[test]
    fn an_evicted_first_tag_keeps_the_later_blocks() {
        // As above, with 50 lines: 58 rows written, rows 0–1 evicted: block 1's tag (row 0)
        // goes, blocks 2 (row 3) and 3 (row 6) stay.
        let lines = |n: usize| (0..n).map(|i| format!("{i}\r\n")).collect::<String>();
        let mut sh = Shell::new();
        sh.command("one", "a\r\nb\r\n", Some(0))
            .command("two", "a\r\nb\r\n", Some(0))
            .command("seq", &lines(50), Some(0))
            .prompt();
        let blocks = sh.blocks();
        let cmds: Vec<_> = blocks.iter().map(|b| b.cmd.as_deref()).collect();
        assert_eq!(cmds, vec![Some("two"), Some("seq")], "{blocks:?}");
        assert_eq!(
            output_text(&sh.term, &blocks[0].id).as_deref(),
            Some("a\nb")
        );
        assert_eq!(blocks[0].cmd_row, sh.term.grid().topmost_line() + 1);
    }

    /// zsh's PROMPT_SP after output without a final newline: the EOL mark, `cols - 1` spaces
    /// (which soft-wrap the line), `\r`, a space, `\r`.
    fn prompt_sp(cols: usize) -> Vec<u8> {
        format!("%{}\r \r", " ".repeat(cols - 1)).into_bytes()
    }

    #[test]
    fn prompt_sp_does_not_join_the_next_prompt_to_the_output() {
        let mut sh = Shell::new();
        sh.prompt()
            .feed(b"printf abc\r\n")
            .feed(&c(Some("printf%20abc")))
            .feed(b"abc")
            .feed(&prompt_sp(20))
            .feed(&d(Some(0)))
            .prompt()
            .feed(b"ls\r\n")
            .feed(&c(Some("ls")))
            .feed(b"x\r\n")
            .feed(&prompt_sp(20))
            .feed(&d(Some(0)))
            .prompt();
        let check = |sh: &Shell, when: &str| {
            let blocks = sh.blocks();
            assert_eq!(blocks.len(), 2, "{when}: {blocks:?}");
            let (printf, ls) = (&blocks[0], &blocks[1]);
            assert_eq!(printf.cmd.as_deref(), Some("printf abc"), "{when}");
            assert_eq!(
                output_text(&sh.term, &printf.id).as_deref(),
                Some("abc%"),
                "{when}"
            );
            let (_, out_end) = printf.output.unwrap();
            assert!(out_end < ls.prompt_top, "{when}: {blocks:?}");
            // The ls command row is the prompt's row, below the printf output.
            assert_eq!(ls.cmd.as_deref(), Some("ls"), "{when}");
            assert_eq!(ls.prompt_top, ls.cmd_row, "{when}");
            assert_eq!(ls.cmd_row, out_end + 1, "{when}");
            let row = sh.term.bounds_to_string(
                Point::new(ls.cmd_row, Column(0)),
                Point::new(ls.cmd_row, sh.term.grid().last_column()),
            );
            assert_eq!(row.trim_end(), "$ ls", "{when}");
            assert_eq!(
                output_text(&sh.term, &ls.id).as_deref(),
                Some("x"),
                "{when}"
            );
            assert_eq!(live_prompt(&sh.term), Some(sh.cursor().line), "{when}");
        };
        check(&sh, "20 cols");
        // Reflow does not re-join them.
        for cols in [10, 40, 6, 20] {
            sh.term.resize(TermDims { cols, rows: 6 });
            check(&sh, &format!("{cols} cols"));
        }
    }

    #[test]
    fn a_c_over_the_mark_cap_still_starts_output() {
        let mut sh = Shell::new();
        let long = "a".repeat(9 * 1024);
        sh.prompt().feed(b"x\r\n");
        assert!(sh.tagger.prompt_ready());
        sh.feed(&c(Some(&long)));
        assert!(!sh.tagger.prompt_ready());
        let t = &tags(&sh.term)[0].1;
        assert_eq!(t.state, BlockState::Running);
        assert_eq!(t.cmd, None);
        assert!(t.truncated);
        assert!(encode(t).uri().contains("&t=1"));
        let blocks = sh.blocks();
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].outcome, Outcome::Running);
        assert_eq!(blocks[0].cmd, None);
    }

    #[test]
    fn a_storm_of_d_marks_after_a_miss_scans_nothing() {
        let mut sh = Shell::new();
        sh.command("ls", "a\r\n", Some(0));
        // The second D misses (the tag is done), and so do all after it.
        for _ in 0..1000 {
            sh.feed(&d(Some(1)));
        }
        assert_eq!(sh.blocks()[0].outcome, Outcome::Ok);
        // Proof the scan is skipped: a running tag written by hand above the cursor is not
        // finished by another D while the tagger stays Unknown after a miss…
        let (line, mut t) = tags(&sh.term)[0].clone();
        t.state = BlockState::Running;
        t.out_lines = None;
        t.exit = None;
        sh.term.grid_mut()[line][Column(0)].set_hyperlink(Some(encode(&t)));
        sh.feed(&d(Some(0)));
        assert_eq!(sh.blocks()[0].outcome, Outcome::Running);
        // …but a C (output of some command started) re-arms it.
        sh.feed(&c(None)).feed(&d(Some(0)));
        assert_eq!(sh.blocks()[0].outcome, Outcome::Ok);
        // A fresh tagger (a reattach) scans on its first D.
        let mut sh = Shell::new();
        sh.prompt().feed(b"ls\r\n").feed(&c(Some("ls")));
        sh.tagger = Tagger::new();
        sh.feed(&d(Some(0)));
        assert_eq!(sh.blocks()[0].outcome, Outcome::Ok);
    }

    #[test]
    fn clears_keep_or_drop_tags() {
        let mut sh = Shell::new();
        sh.command("ls", "a\r\nb\r\n", Some(0)).prompt();
        sh.feed(b"\x1b[H\x1b[2J");
        let blocks = sh.blocks();
        assert_eq!(blocks.len(), 1, "ESC[2J keeps tags in scrollback");
        assert!(blocks[0].cmd_row < Line(0));
        sh.feed(b"\x1b[3J");
        assert!(sh.blocks().is_empty(), "ESC[3J drops them");
        assert!(tags(&sh.term).is_empty());
        sh.command("ls", "a\r\n", Some(0));
        assert_eq!(sh.blocks().len(), 1);
        sh.feed(b"\x1bc");
        assert!(sh.blocks().is_empty(), "ESC c drops them");
        assert!(tags(&sh.term).is_empty());
    }

    #[test]
    fn counts_past_the_grid_are_clamped() {
        let mut sh = Shell::new();
        sh.command("ls", "a\r\nb\r\n", Some(0)).prompt();
        let (line, mut t) = tags(&sh.term)[0].clone();
        let last = Line(sh.term.screen_lines() as i32 - 1);
        t.out_lines = Some(999);
        sh.term.grid_mut()[line][Column(0)].set_hyperlink(Some(encode(&t)));
        let b = &sh.blocks()[0];
        assert_eq!(b.output, Some((Line(1), last)));
        assert!(output_text(&sh.term, &b.id).is_some());
        // Output that would start past the grid: none; a prompt taller than the scrollback
        // starts at its top.
        t.cmd_lines = Some(999);
        t.prompt_lines = PROMPT_LINES_CAP;
        sh.term.grid_mut()[line][Column(0)].set_hyperlink(Some(encode(&t)));
        let b = &sh.blocks()[0];
        assert_eq!(b.output, None);
        assert_eq!(b.prompt_top, sh.term.grid().topmost_line());
        assert_eq!(output_text(&sh.term, &b.id).as_deref(), Some(""));
    }

    #[test]
    fn marks_on_the_alt_screen_are_ignored() {
        let mut sh = Shell::new();
        sh.feed(b"\x1b[?1049h");
        sh.command("x", "y\r\n", Some(0)).prompt();
        assert!(sh.term.mode().contains(TermMode::ALT_SCREEN));
        assert!(tags(&sh.term).is_empty());
        assert!(!sh.tagger.prompt_ready());
        sh.feed(b"\x1b[?1049l");
        assert!(tags(&sh.term).is_empty());
        assert!(sh.blocks().is_empty());

        // A running block whose program uses the alt screen still finishes on D.
        sh.prompt()
            .feed(b"vim\r\n")
            .feed(&c(Some("vim")))
            .feed(b"\x1b[?1049h");
        assert!(
            sh.blocks().is_empty(),
            "nothing listed while the alt screen is up"
        );
        assert_eq!(live_prompt(&sh.term), None);
        sh.feed(b"hello\r\n").feed(b"\x1b[?1049l").feed(&d(Some(0)));
        let blocks = sh.blocks();
        assert_eq!(blocks.len(), 1, "{blocks:?}");
        assert_eq!(blocks[0].outcome, Outcome::Ok);
        assert_eq!(blocks[0].cmd.as_deref(), Some("vim"));
    }

    #[test]
    fn marks_inside_a_sync_update_are_ignored() {
        let mut sh = Shell::new();
        sh.feed(b"\x1b[?2026h").prompt();
        assert!(sh.processor.sync_bytes_count() > 0);
        assert!(tags(&sh.term).is_empty());
        sh.feed(b"\x1b[?2026l");
        assert_eq!(sh.processor.sync_bytes_count(), 0);
        assert!(tags(&sh.term).is_empty());
        assert!(!sh.tagger.prompt_ready());
        sh.feed(b"\r\n").prompt();
        assert_eq!(tags(&sh.term).len(), 1);
        assert!(sh.tagger.prompt_ready());
        // In one chunk: the sync part dropped, the rest tagged.
        let mut sh = Shell::new();
        let mut chunk = b"\x1b[?2026h".to_vec();
        chunk.extend_from_slice(A);
        chunk.extend_from_slice(b"$ ");
        chunk.extend_from_slice(B);
        chunk.extend_from_slice(b"\x1b[?2026l\r\n");
        chunk.extend_from_slice(A);
        chunk.extend_from_slice(b"$ ");
        chunk.extend_from_slice(B);
        sh.feed(&chunk);
        let found = tags(&sh.term);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0, Line(1));
    }

    #[test]
    fn an_overlong_command_is_kept_without_text() {
        let mut sh = Shell::new();
        let long = "a".repeat(3000);
        sh.prompt().feed(b"x\r\n").feed(&c(Some(&long)));
        let t = &tags(&sh.term)[0].1;
        assert_eq!(t.state, BlockState::Running);
        assert_eq!(t.cmd, None);
        assert!(t.truncated);
        sh.feed(&d(Some(0)));
        let blocks = sh.blocks();
        assert_eq!(blocks[0].cmd, None);
        // Exactly at the cap is kept.
        let mut sh = Shell::new();
        let at_cap = "b".repeat(CMD_CAP);
        sh.prompt().feed(b"x\r\n").feed(&c(Some(&at_cap)));
        let t = &tags(&sh.term)[0].1;
        assert_eq!(t.cmd.as_deref(), Some(at_cap.as_str()));
        assert!(!t.truncated);
    }

    #[test]
    fn empty_prompts_are_not_blocks() {
        let mut sh = Shell::new();
        sh.prompt().feed(b"\r\n").prompt();
        assert!(sh.blocks().is_empty());
        assert_eq!(tags(&sh.term).len(), 2);
        assert_eq!(live_prompt(&sh.term), Some(Line(1)));
    }

    #[test]
    fn the_tagger_knows_the_prompt() {
        let mut sh = Shell::new();
        assert!(!sh.tagger.prompt_ready());
        sh.feed(A).feed(b"$ ");
        assert!(!sh.tagger.prompt_ready());
        sh.feed(B);
        assert!(sh.tagger.prompt_ready());
        assert_eq!(sh.tagger.input_start(), Some(sh.cursor()));
        assert_eq!(
            sh.tagger.input_start(),
            Some(Point::new(Line(0), Column(2)))
        );
        sh.feed(b"ls\r\n").feed(&c(Some("ls")));
        assert!(!sh.tagger.prompt_ready());
        assert_eq!(sh.tagger.input_start(), None);
        sh.feed(&d(Some(0)));
        assert!(!sh.tagger.prompt_ready());
        // An Unknown tagger (e.g. after a reattach) still finishes a block it finds.
        let mut sh = Shell::new();
        sh.prompt()
            .feed(b"ls\r\n")
            .feed(&c(Some("ls")))
            .feed(b"a\r\n");
        sh.tagger = Tagger::new();
        sh.feed(&d(Some(0)));
        assert_eq!(sh.blocks()[0].outcome, Outcome::Ok);
    }

    #[test]
    fn visible_rows_and_the_chosen_block() {
        use alacritty_terminal::grid::Scroll;
        let mut sh = Shell::new();
        let ten: String = (1..=10).map(|i| format!("{i}\r\n")).collect();
        sh.command("one", &ten, Some(0))
            .command("two", "x\r\n", Some(1))
            .command("three", "y\r\n", None)
            .prompt();
        // 16 rows on a 6-row screen: block 1's command row is Line(-10), above the view.
        let ids: Vec<String> = sh.blocks().into_iter().map(|b| b.id).collect();
        assert_eq!(ids.len(), 3);
        let view = |id: &str, outcome, prompt, output, chosen| BlockView {
            id: id.to_string(),
            outcome,
            prompt,
            output,
            chosen,
        };
        let one = |chosen| view(&ids[0], Outcome::Ok, (-10, -10), Some((-9, 0)), chosen);
        let two = |shift: i32| {
            let o = Some((2 + shift, 2 + shift));
            view(
                &ids[1],
                Outcome::Failed(1),
                (1 + shift, 1 + shift),
                o,
                false,
            )
        };
        let three = |shift: i32| {
            let o = Some((4 + shift, 4 + shift));
            view(&ids[2], Outcome::Unknown, (3 + shift, 3 + shift), o, false)
        };
        assert_eq!(
            visible(&sh.term, None),
            (vec![two(0), three(0)], Some(5)),
            "block 1 is above the view"
        );
        assert_eq!(
            visible(&sh.term, Some(&ids[0])),
            (vec![one(true), two(0), three(0)], Some(5)),
            "the chosen block is found in scrollback"
        );
        // An unknown id changes nothing.
        assert_eq!(
            visible(&sh.term, Some("fuxx-block-0")).0,
            vec![two(0), three(0)]
        );
        // Scrolled back 4: rows move down 4; block 3's tag is under the view, within 8 rows.
        sh.term.scroll_display(Scroll::Delta(4));
        assert_eq!(visible(&sh.term, None), (vec![two(4), three(4)], Some(9)));
        // Scrolled back 10: block 1's tag is the top row; block 3's is 8 rows under the view.
        sh.term.scroll_display(Scroll::Delta(6));
        let (blocks, _) = visible(&sh.term, None);
        let shifted = |v: BlockView| BlockView {
            prompt: (v.prompt.0 + 10, v.prompt.1 + 10),
            output: v.output.map(|(a, b)| (a + 10, b + 10)),
            ..v
        };
        assert_eq!(blocks, vec![shifted(one(false)), two(10), three(10)]);
        // Nothing on the alternate screen.
        sh.term.scroll_display(Scroll::Bottom);
        sh.feed(b"\x1b[?1049h");
        assert_eq!(visible(&sh.term, None), (vec![], None));
        assert_eq!(visible(&sh.term, Some(&ids[0])), (vec![], None));
    }
}
