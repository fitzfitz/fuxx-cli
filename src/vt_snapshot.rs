//! M10a: a `Term` as a VT byte stream. `encode` writes bytes that, fed into a fresh `Term` of the
//! same size and history limit, rebuild its screen. The daemon sends them on attach, so the
//! GUI's replica starts out identical to the authoritative Term.
//!
//! The stream, in order: `ESC c`; every row of scrollback and screen with SGR and OSC 8 runs
//! (soft wraps written as overflow, never `\r\n`, so the rebuilt Term reflows the same way);
//! the kitty keyboard flags; the saved cursor (`ESC 7`); the cursor, its pen and a pending
//! wrap. If the alt screen is up, all of that is written for the primary screen first, then
//! `CSI ?1049h`, a clear with a default pen, and the alt screen's. Last come the modes, the
//! cursor shape, and insert mode.
//!
//! Reaching the primary screen under the alt screen means swapping screens on the source Term
//! (see `encode`): that also clears its selection, marks it fully damaged and replaces its
//! kitty keyboard mode bits with the stack top. Harmless for the daemon.
//!
//! Round-trips (tested): cells (char, colours, SGR flags, underline colour, wide and
//! zero-width chars, tabs, hyperlinks), `WRAPLINE`, history, the alt screen with the primary
//! screen and its history underneath, the cursor (position, pen, pending wrap), the saved
//! cursor (position and pen), and the modes: DECCKM, keypad, mouse modes and encodings, focus,
//! bracketed paste, origin, autowrap, cursor visibility, alternate scroll, urgency hints, LNM,
//! insert, and the cursor shape.
//!
//! Does not round-trip, because alacritty_terminal 0.26 keeps it private with no getter:
//! - the **title** (and title stack) — the daemon keeps the title as session metadata;
//! - the **scroll region** — the rebuilt Term has the full screen as its region until the
//!   program sets one again;
//! - **tab stops** (default every 8 columns) and **G0–G3 charsets** (cell contents are already
//!   translated, only later output would differ);
//! - the kitty keyboard mode **stack**: only its top entry is restored, pushed with `CSI > n u`
//!   (the study said `CSI = n u`, which sets the top entry; on a fresh Term, whose stack is
//!   empty, a push is what recreates it). In fuxx this is moot: `Config::default()` has
//!   `kitty_keyboard: false`, so the flags are never set.
//!
//! Also not restored: the saved cursor's pending-wrap flag and charsets; OSC 4/10/11 colour
//! changes; the scrolled-back view; a pending DEC 2026 synchronized update (that lives in the
//! `Processor`, and a fresh one has none); `WRAPLINE` on the bottom screen row (it cannot be
//! written without scrolling); and a tab in the last column of a wrapped row (written as a
//! space).

use std::io::Write;

use alacritty_terminal::event::EventListener;
use alacritty_terminal::grid::{Dimensions, Grid};
use alacritty_terminal::index::{Column, Line, Point};
use alacritty_terminal::term::cell::{Cell, Flags, Hyperlink};
use alacritty_terminal::term::{Term, TermMode};
use alacritty_terminal::vte::ansi::{Color, CursorShape, CursorStyle, NamedColor};

/// Cell flags that SGR sets (the rest are layout: wraps and wide-char spacers).
const SGR_FLAGS: Flags = Flags::BOLD
    .union(Flags::DIM)
    .union(Flags::ITALIC)
    .union(Flags::ALL_UNDERLINES)
    .union(Flags::INVERSE)
    .union(Flags::HIDDEN)
    .union(Flags::STRIKEOUT);

/// DEC private modes restored with `CSI ? n h/l`, and the ANSI LNM mode (`CSI 20 h/l`).
const MODES: [(TermMode, &str); 14] = [
    (TermMode::APP_CURSOR, "?1"),
    (TermMode::ORIGIN, "?6"),
    (TermMode::LINE_WRAP, "?7"),
    (TermMode::SHOW_CURSOR, "?25"),
    (TermMode::MOUSE_REPORT_CLICK, "?1000"),
    (TermMode::MOUSE_DRAG, "?1002"),
    (TermMode::MOUSE_MOTION, "?1003"),
    (TermMode::FOCUS_IN_OUT, "?1004"),
    (TermMode::UTF8_MOUSE, "?1005"),
    (TermMode::SGR_MOUSE, "?1006"),
    (TermMode::ALTERNATE_SCROLL, "?1007"),
    (TermMode::URGENCY_HINTS, "?1042"),
    (TermMode::BRACKETED_PASTE, "?2004"),
    (TermMode::LINE_FEED_NEW_LINE, "20"),
];

/// The bytes that rebuild `term` on a fresh `Term` of the same size and history limit.
///
/// Takes `&mut` only to reach the primary screen while the alt screen is up (alacritty has no
/// getter for the inactive grid): it swaps to the primary screen and back, then puts the alt
/// grid back as it was. The Term's contents, cursors and modes are left as they were found; the
/// swap does clear any selection, mark the whole Term damaged, and reset the kitty keyboard
/// mode bits to the top of the alt screen's stack (what they already were). All harmless for
/// the daemon, which has no selection and does not render.
#[allow(dead_code)] // used by the daemon from M10a Task 3
pub fn encode<L: EventListener>(term: &mut Term<L>) -> Vec<u8> {
    let mut enc = Encoder::default();
    enc.out.extend_from_slice(b"\x1bc");
    if term.mode().contains(TermMode::ALT_SCREEN) {
        let alt = term.grid().clone();
        term.swap_alt();
        enc.screen(term.grid());
        enc.keyboard(*term.mode());
        enc.cursors(term.grid());
        term.swap_alt();
        *term.grid_mut() = alt;
        // `?1049h` hands the alt screen the primary cursor's pen, background included: clear
        // it with a default pen so the alt grid starts out as blank as the original's.
        enc.out
            .extend_from_slice(b"\x1b[?1049h\x1b[H\x1b[0m\x1b[2J");
        enc.pen = Pen::default();
    }
    let mode = *term.mode();
    enc.screen(term.grid());
    enc.keyboard(mode);
    enc.modes(mode);
    enc.cursor_style(term.cursor_style());
    enc.cursors(term.grid());
    if mode.contains(TermMode::INSERT) {
        enc.out.extend_from_slice(b"\x1b[4h");
    }
    enc.out
}

/// The rendition the rebuilding Term will write cells with (its cursor template).
#[derive(Clone, PartialEq)]
struct Pen {
    fg: Color,
    bg: Color,
    flags: Flags,
    underline: Option<Color>,
}

impl Default for Pen {
    fn default() -> Self {
        Pen {
            fg: Color::Named(NamedColor::Foreground),
            bg: Color::Named(NamedColor::Background),
            flags: Flags::empty(),
            underline: None,
        }
    }
}

impl Pen {
    fn of(cell: &Cell) -> Self {
        Pen {
            fg: cell.fg,
            bg: cell.bg,
            flags: cell.flags & SGR_FLAGS,
            underline: cell.underline_color(),
        }
    }
}

#[derive(Default)]
struct Encoder {
    out: Vec<u8>,
    pen: Pen,
    link: Option<Hyperlink>,
}

impl Encoder {
    /// Every row from the top of the scrollback to the bottom of the screen. Rows that the
    /// rebuilding Term pushes off the top land in its scrollback, as they did in the original.
    fn screen(&mut self, grid: &Grid<Cell>) {
        let cols = grid.columns();
        self.out.reserve(grid.total_lines() * cols);
        let bottom = grid.bottommost_line();
        let mut prev_wrapped = false;
        for line in grid.topmost_line().0..=bottom.0 {
            let row = &grid[Line(line)];
            let last_row = line == bottom.0;
            let wrapped = !last_row && row[Column(cols - 1)].flags.contains(Flags::WRAPLINE);
            let mut end = if wrapped {
                cols
            } else {
                (0..cols)
                    .rev()
                    .find(|&c| !is_blank(&row[Column(c)]))
                    .map_or(0, |c| c + 1)
            };
            // The row above only gets its WRAPLINE once a character overflows into this one.
            if prev_wrapped && end == 0 {
                end = 1;
            }
            for col in 0..end {
                let cell = &row[Column(col)];
                if cell
                    .flags
                    .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
                {
                    continue; // written by the wide char itself
                }
                if cell.c == '\t' && col + 1 < cols {
                    // HT stores '\t' in a blank cell and jumps to a tab stop; jump back after.
                    self.set_pen(cell);
                    let _ = write!(self.out, " \x1b[{}G\t\x1b[{}G", col + 1, col + 2);
                    self.zerowidth(cell);
                } else {
                    self.cell(cell);
                }
            }
            if wrapped {
                prev_wrapped = true;
                continue;
            }
            // Overflowing into a new bottom row scrolls with the pen's background, which may
            // have coloured the part of this row that was not written.
            if prev_wrapped && end < cols && row[Column(0)].bg != Pen::default().bg {
                self.set_pen(&Cell::default());
                self.out.extend_from_slice(b"\x1b[K");
            }
            if !last_row {
                // A linefeed that scrolls fills the new row with the pen's background.
                if self.pen.bg != Pen::default().bg {
                    self.set_pen(&Cell::default());
                }
                self.out.extend_from_slice(b"\r\n");
            }
            prev_wrapped = false;
        }
    }

    /// Kitty keyboard flags, pushed as one stack entry.
    fn keyboard(&mut self, mode: TermMode) {
        let bits = [
            TermMode::DISAMBIGUATE_ESC_CODES,
            TermMode::REPORT_EVENT_TYPES,
            TermMode::REPORT_ALTERNATE_KEYS,
            TermMode::REPORT_ALL_KEYS_AS_ESC,
            TermMode::REPORT_ASSOCIATED_TEXT,
        ];
        let flags: u8 = bits
            .iter()
            .enumerate()
            .filter(|(_, m)| mode.contains(**m))
            .map(|(i, _)| 1 << i)
            .sum();
        if flags != 0 {
            let _ = write!(self.out, "\x1b[>{flags}u");
        }
    }

    /// Modes that differ from a freshly reset Term's, except insert mode (written last, since
    /// it would shift the cells written before it).
    fn modes(&mut self, mode: TermMode) {
        let default = TermMode::default();
        for (flag, code) in MODES {
            if mode.contains(flag) != default.contains(flag) {
                let set = if mode.contains(flag) { 'h' } else { 'l' };
                let _ = write!(self.out, "\x1b[{code}{set}");
            }
        }
        if mode.contains(TermMode::APP_KEYPAD) {
            self.out.extend_from_slice(b"\x1b=");
        }
    }

    /// DECSCUSR, unless the shape is the default (left to the rebuilding Term's own config).
    fn cursor_style(&mut self, style: CursorStyle) {
        if style == CursorStyle::default() {
            return;
        }
        let steady = match style.shape {
            CursorShape::Block => 2,
            CursorShape::Underline => 4,
            CursorShape::Beam => 6,
            CursorShape::HollowBlock | CursorShape::Hidden => return, // not settable by DECSCUSR
        };
        let _ = write!(self.out, "\x1b[{} q", steady - u8::from(style.blinking));
    }

    /// The saved cursor, then the cursor with its pen and pending wrap.
    fn cursors(&mut self, grid: &Grid<Cell>) {
        let saved = &grid.saved_cursor;
        self.set_pen(&saved.template);
        self.goto(saved.point);
        self.out.extend_from_slice(b"\x1b7");

        let cursor = &grid.cursor;
        self.goto(cursor.point);
        if cursor.input_needs_wrap && cursor.point.column == grid.last_column() {
            // Rewrite the last cell: writing into the last column is what sets a pending wrap.
            let row = &grid[cursor.point.line];
            let mut col = cursor.point.column;
            if row[col].flags.contains(Flags::WIDE_CHAR_SPACER) && col.0 > 0 {
                col -= 1;
                self.goto(Point::new(cursor.point.line, col));
            }
            let cell = &row[col];
            if cell.c == '\t' {
                self.cell(&Cell {
                    c: ' ',
                    ..cell.clone()
                });
            } else {
                self.cell(cell);
            }
        }
        self.set_pen(&cursor.template);
    }

    /// One cell: its pen, its character and its zero-width characters.
    fn cell(&mut self, cell: &Cell) {
        self.set_pen(cell);
        let mut buf = [0; 4];
        self.out
            .extend_from_slice(cell.c.encode_utf8(&mut buf).as_bytes());
        self.zerowidth(cell);
    }

    fn zerowidth(&mut self, cell: &Cell) {
        for c in cell.zerowidth().unwrap_or_default() {
            let mut buf = [0; 4];
            self.out
                .extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
        }
    }

    fn goto(&mut self, point: Point) {
        let _ = write!(
            self.out,
            "\x1b[{};{}H",
            point.line.0 + 1,
            point.column.0 + 1
        );
    }

    /// Switch SGR and the OSC 8 link to `cell`'s, writing only what changed.
    fn set_pen(&mut self, cell: &Cell) {
        let pen = Pen::of(cell);
        if pen != self.pen {
            self.sgr(&pen);
            self.pen = pen;
        }
        let link = cell.hyperlink();
        if link != self.link {
            match &link {
                Some(l) => {
                    let _ = write!(self.out, "\x1b]8;id={};{}\x1b\\", l.id(), l.uri());
                }
                None => self.out.extend_from_slice(b"\x1b]8;;\x1b\\"),
            }
            self.link = link;
        }
    }

    /// A full SGR from reset: `CSI 0 ; ... m`.
    fn sgr(&mut self, pen: &Pen) {
        const FLAGS: [(Flags, &[u8]); 11] = [
            (Flags::BOLD, b";1"),
            (Flags::DIM, b";2"),
            (Flags::ITALIC, b";3"),
            (Flags::UNDERLINE, b";4"),
            (Flags::DOUBLE_UNDERLINE, b";4:2"),
            (Flags::UNDERCURL, b";4:3"),
            (Flags::DOTTED_UNDERLINE, b";4:4"),
            (Flags::DASHED_UNDERLINE, b";4:5"),
            (Flags::INVERSE, b";7"),
            (Flags::HIDDEN, b";8"),
            (Flags::STRIKEOUT, b";9"),
        ];
        self.out.extend_from_slice(b"\x1b[0");
        for (flag, param) in FLAGS {
            if pen.flags.contains(flag) {
                self.out.extend_from_slice(param);
            }
        }
        self.colour(pen.fg, 30, 90, 38);
        self.colour(pen.bg, 40, 100, 48);
        if let Some(underline) = pen.underline {
            self.colour(underline, 0, 0, 58);
        }
        self.out.push(b'm');
    }

    /// `;` and the SGR parameter for a colour: `base + n` for the 8 named colours, `bright + n`
    /// for their bright forms, `ext;5;n` for indexed and `ext;2;r;g;b` for RGB. Default colours
    /// need none (SGR 0 already set them); the other named colours cannot come from SGR.
    fn colour(&mut self, color: Color, base: u8, bright: u8, ext: u8) {
        let _ = match color {
            Color::Spec(rgb) => write!(self.out, ";{ext};2;{};{};{}", rgb.r, rgb.g, rgb.b),
            Color::Indexed(n) => write!(self.out, ";{ext};5;{n}"),
            Color::Named(named) if base > 0 => match named as usize {
                n @ 0..=7 => write!(self.out, ";{}", base as usize + n),
                n @ 8..=15 => write!(self.out, ";{}", bright as usize + n - 8),
                _ => Ok(()),
            },
            Color::Named(_) => Ok(()),
        };
    }
}

/// A cell indistinguishable from an untouched one (layout flags aside).
fn is_blank(cell: &Cell) -> bool {
    cell.c == ' '
        && Pen::of(cell) == Pen::default()
        && cell.flags.difference(Flags::WRAPLINE).is_empty()
        && cell.zerowidth().is_none_or(<[char]>::is_empty)
        && cell.hyperlink().is_none()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::TermDims;
    use alacritty_terminal::event::VoidListener;
    use alacritty_terminal::grid::Dimensions;
    use alacritty_terminal::index::{Column, Line, Point};
    use alacritty_terminal::term::cell::{Cell, Flags};
    use alacritty_terminal::term::{Config, TermMode};
    use alacritty_terminal::vte::ansi::{Color, CursorStyle, Processor, StdSyncHandler};

    /// One cell, with everything `Cell` carries flattened (so `extra: None` and an empty
    /// `extra` compare equal).
    #[derive(Debug, PartialEq)]
    struct CellView {
        c: char,
        fg: Color,
        bg: Color,
        flags: Flags,
        zerowidth: Vec<char>,
        underline: Option<Color>,
        link: Option<(String, String)>,
    }

    fn view(cell: &Cell) -> CellView {
        CellView {
            c: cell.c,
            fg: cell.fg,
            bg: cell.bg,
            flags: cell.flags,
            zerowidth: cell.zerowidth().map(<[char]>::to_vec).unwrap_or_default(),
            underline: cell.underline_color(),
            link: cell
                .hyperlink()
                .map(|l| (l.id().to_owned(), l.uri().to_owned())),
        }
    }

    /// Everything a round trip must preserve that alacritty lets us read.
    #[derive(Debug, PartialEq)]
    struct State {
        size: (usize, usize),
        history: usize,
        lines: Vec<Vec<CellView>>,
        cursor: Point,
        pending_wrap: bool,
        pen: CellView,
        saved_cursor: Point,
        saved_pen: CellView,
        mode: TermMode,
        cursor_style: CursorStyle,
    }

    fn state(term: &Term<VoidListener>) -> State {
        let grid = term.grid();
        let lines = (grid.topmost_line().0..=grid.bottommost_line().0)
            .map(|l| {
                let row = &grid[Line(l)];
                (0..grid.columns()).map(|c| view(&row[Column(c)])).collect()
            })
            .collect();
        State {
            size: (grid.columns(), grid.screen_lines()),
            history: grid.history_size(),
            lines,
            cursor: grid.cursor.point,
            pending_wrap: grid.cursor.input_needs_wrap,
            pen: view(&grid.cursor.template),
            saved_cursor: grid.saved_cursor.point,
            saved_pen: view(&grid.saved_cursor.template),
            mode: *term.mode(),
            cursor_style: term.cursor_style(),
        }
    }

    /// Line-by-line first, so a failure names the line instead of dumping the whole grid.
    fn assert_same(a: &State, b: &State) {
        assert_eq!(a.size, b.size, "size");
        assert_eq!(a.history, b.history, "history size");
        for (i, (la, lb)) in a.lines.iter().zip(&b.lines).enumerate() {
            let line = i as i64 - a.history as i64;
            assert_eq!(la, lb, "line {line} differs");
        }
        assert_eq!(a, b);
    }

    fn term(cols: usize, rows: usize) -> Term<VoidListener> {
        Term::new(Config::default(), &TermDims { cols, rows }, VoidListener)
    }

    fn feed(term: &mut Term<VoidListener>, bytes: &[u8]) {
        let mut processor: Processor<StdSyncHandler> = Processor::new();
        processor.advance(term, bytes);
    }

    /// Feed `bytes` into Term A, encode A, feed that into a fresh Term B, and check A == B.
    /// Also checks that encoding left A unchanged.
    fn round_trip(
        bytes: &[u8],
        cols: usize,
        rows: usize,
    ) -> (Term<VoidListener>, Term<VoidListener>) {
        let mut a = term(cols, rows);
        feed(&mut a, bytes);
        let before = state(&a);
        let snapshot = encode(&mut a);
        assert_same(&before, &state(&a));
        let mut b = term(cols, rows);
        feed(&mut b, &snapshot);
        assert_same(&state(&a), &state(&b));
        (a, b)
    }

    #[test]
    fn plain_coloured_text() {
        let bytes = concat!(
            "plain \x1b[31mred\x1b[0m \x1b[1;4;38;5;208mbold-ul-208\x1b[0m\r\n",
            "a\tb \x1b[48;2;10;20;30mrgb bg\x1b[0m \x1b[92;101mbright\x1b[0m\r\n",
            "\x1b[3;9;2mitalic strike dim\x1b[7;8m inv-hid\x1b[0m\r\n",
            "\x1b[4:3;58;2;255;0;0mcurl\x1b[4:2;58;5;9m dbl\x1b[0m \x1b[38;5;1mindexed1\x1b[0m\r\n",
            "\x1b[44m\x1b[Kerased with blue\x1b[0m\r\n",
            "\x1b[35;1mthis line is exactly forty columns wide!", // pending wrap, pen left magenta+bold
        );
        let (a, _) = round_trip(bytes.as_bytes(), 40, 8);
        assert!(a.grid().cursor.input_needs_wrap);
    }

    #[test]
    fn wrapped_line_reflows_like_the_original() {
        // A wrapped line in the history, then a red line wrapped back on screen (no scroll in A;
        // rebuilding it scrolls B with a red pen, which must not tint the unwritten cells).
        let bytes = concat!(
            "0123456789abcdefghijKLMNO\r\nnext line\r\nmore\r\nlines\r\n",
            "\x1b[2;1H\x1b[41mred wraps around\x1b[0m!\x1b[K",
        );
        let (mut a, mut b) = round_trip(bytes.as_bytes(), 10, 4);
        assert!(
            a.grid()[Line(-3)][Column(9)]
                .flags
                .contains(Flags::WRAPLINE)
        );
        assert!(a.grid()[Line(1)][Column(9)].flags.contains(Flags::WRAPLINE));
        for cols in [6, 10, 4, 13] {
            a.resize(TermDims { cols, rows: 4 });
            b.resize(TermDims { cols, rows: 4 });
            assert_same(&state(&a), &state(&b));
        }
    }

    #[test]
    fn wide_and_combining_characters() {
        let bytes = concat!(
            "漢字テスト\r\n",
            "e\u{301}a\u{308} 👍🏽 ❤\u{fe0f}\r\n",
            "1234567漢wraps\r\n", // the wide char does not fit: leading spacer, then wrap
            "abcdef字",           // ends on a wide char at the last two columns: pending wrap
        );
        let (a, _) = round_trip(bytes.as_bytes(), 8, 6);
        let row = &a.grid()[Line(3)]; // the first line wraps too
        assert!(
            row[Column(7)]
                .flags
                .contains(Flags::LEADING_WIDE_CHAR_SPACER)
        );
        assert!(a.grid().cursor.input_needs_wrap);
    }

    #[test]
    fn alt_screen_with_primary_underneath() {
        let mut bytes = String::new();
        for i in 0..8 {
            bytes.push_str(&format!("\x1b[3{}mprimary {i}\x1b[0m\r\n", i % 8));
        }
        bytes.push_str("$ vim");
        bytes.push_str(
            "\x1b[?1049h\x1b[H\x1b[44malt screen\x1b[0m\r\n~\r\n~ \x1b[1mstatus\x1b[2;3H",
        );
        let (mut a, mut b) = round_trip(bytes.as_bytes(), 20, 4);
        assert!(a.mode().contains(TermMode::ALT_SCREEN));
        // Leaving the alt screen shows the primary screen and its history, in both.
        feed(&mut a, b"\x1b[?1049l");
        feed(&mut b, b"\x1b[?1049l");
        assert!(a.grid().history_size() > 0);
        assert_same(&state(&a), &state(&b));
    }

    #[test]
    fn scroll_region_set() {
        // A region over rows 2-4; text scrolls inside it. The cells and cursor round-trip; the
        // region itself is private in alacritty (see the module docs).
        let bytes =
            b"header\x1b[2;4r\x1b[2;1Hone\r\ntwo\r\nthree\r\nfour\r\nfive\x1b[6;1Hfooter\x1b[3;2H";
        round_trip(bytes, 20, 6);
    }

    #[test]
    fn bracketed_paste_app_cursor_and_mouse_modes() {
        let bytes = concat!(
            "\x1b[?2004h\x1b[?1h\x1b[?1002h\x1b[?1006h\x1b[?1004h\x1b=", // paste, DECCKM, mouse, focus, keypad
            "\x1b[?25l\x1b[5 q\x1b[20h",                                 // hidden beam cursor, LNM
            "\x1b[3;5H\x1b[32m\x1b7\x1b[0m\x1b[1;1H", // a saved cursor with a pen
            "prompt> \x1b[?7l\x1b[4h",                // no autowrap, insert mode
        );
        let (a, _) = round_trip(bytes.as_bytes(), 20, 5);
        let mode = *a.mode();
        assert!(mode.contains(TermMode::BRACKETED_PASTE | TermMode::APP_CURSOR));
        assert!(mode.contains(TermMode::MOUSE_DRAG | TermMode::SGR_MOUSE | TermMode::INSERT));
        assert!(!mode.contains(TermMode::LINE_WRAP | TermMode::SHOW_CURSOR));
    }

    #[test]
    fn an_osc8_link() {
        let bytes = concat!(
            "see \x1b]8;id=doc;https://example.com/a;b\x1b\\the docs\x1b]8;;\x1b\\ and ",
            "\x1b]8;;https://x.y/z\x1b\\an auto-id link\x1b]8;;\x1b\\ end\r\n",
            "\x1b]8;;https://still.open\x1b\\open", // left open: the pen carries it
        );
        let (a, _) = round_trip(bytes.as_bytes(), 30, 4);
        assert!(a.grid()[Line(0)][Column(4)].hyperlink().is_some());
    }

    #[test]
    fn a_long_history() {
        let mut bytes = String::new();
        for i in 0..500 {
            bytes.push_str(&format!(
                "line {i} \x1b[3{0}mcolour {i}\x1b[4{0}m!\x1b[0m\r\n",
                i % 8
            ));
        }
        let (a, _) = round_trip(bytes.as_bytes(), 30, 24);
        assert_eq!(a.grid().history_size(), 500 - 23);
    }

    #[test]
    fn a_claude_code_redraw() {
        // Synthesised from what Claude Code (Ink) writes: bracketed paste on, hidden cursor,
        // erase-line + cursor-up redraws of the spinner and input box, truecolor SGR.
        let bytes = concat!(
            "\x1b[?2004h\x1b[?25l",
            "\x1b[38;2;215;119;87m✻\x1b[39m \x1b[1mWelcome to Claude Code!\x1b[22m\r\n\r\n",
            "\x1b[38;2;153;153;153m> \x1b[39mfix the failing test\r\n\r\n",
            "\x1b[38;2;215;119;87m● \x1b[39mI'll look at the test first.\r\n",
            "\x1b[38;2;215;119;87m✢ \x1b[39mThinking… \x1b[2m(3s · esc to interrupt)\x1b[22m\r\n",
            "\x1b[38;2;102;102;102m╭──────────────────────────────╮\x1b[39m\r\n",
            "\x1b[38;2;102;102;102m│\x1b[39m > \x1b[7m \x1b[27m                          \x1b[38;2;102;102;102m│\x1b[39m\r\n",
            "\x1b[38;2;102;102;102m╰──────────────────────────────╯\x1b[39m\r\n",
            "  \x1b[2m? for shortcuts\x1b[22m",
            // Redraw: up four lines, erase each, write the new spinner frame and box.
            "\x1b[2K\x1b[1A\x1b[2K\x1b[1A\x1b[2K\x1b[1A\x1b[2K\x1b[1A\x1b[2K\x1b[G",
            "\x1b[38;2;215;119;87m✳ \x1b[39mThinking… \x1b[2m(4s · esc to interrupt)\x1b[22m\r\n",
            "\x1b[38;2;102;102;102m╭──────────────────────────────╮\x1b[39m\r\n",
            "\x1b[38;2;102;102;102m│\x1b[39m > \x1b[7m \x1b[27m                          \x1b[38;2;102;102;102m│\x1b[39m\r\n",
            "\x1b[38;2;102;102;102m╰──────────────────────────────╯\x1b[39m\r\n",
            "  \x1b[2m? for shortcuts\x1b[22m\x1b[3A\x1b[6G",
        );
        round_trip(bytes.as_bytes(), 40, 12);
    }

    #[test]
    fn the_alt_screen_does_not_inherit_the_primary_pen() {
        // `?1049h` copies the primary cursor's pen, and the alt grid is cleared with its
        // background; the snapshot must not leave the alt screen blue.
        round_trip(b"hello\x1b[44m\x1b[?1049h\x1b[0m\x1b[2J\x1b[Hvim", 20, 4);
    }

    #[test]
    fn a_wrap_into_an_emptied_row_keeps_its_wrapline() {
        // Row 0 wraps into row 1, whose only character is then erased.
        let (a, _) = round_trip(b"0123456789x\x1b[1D\x1b[K\x1b[3;1Hz", 10, 4);
        assert!(a.grid()[Line(0)][Column(9)].flags.contains(Flags::WRAPLINE));
    }
}
