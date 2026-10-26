//! A render-ready copy of the terminal screen, plus the batching that turns a line of cells
//! into a few background rectangles and text runs.
//!
//! Everything here is plain data (no GPUI), so the tricky parts of rendering are unit-tested.
//! The GPUI view only paints what these functions return.

use alacritty_terminal::event::EventListener;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line, Point};
use alacritty_terminal::term::Term;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::vte::ansi::{CursorShape, Rgb};

use crate::colors;
use crate::theme::Theme;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CellView {
    pub ch: char,
    pub fg: Rgb,
    pub bg: Rgb,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    /// Right half of a wide (CJK/emoji) glyph: draws nothing itself.
    pub spacer: bool,
}

impl CellView {
    pub fn blank(theme: &Theme) -> CellView {
        CellView {
            ch: ' ',
            fg: theme.foreground,
            bg: theme.background,
            bold: false,
            italic: false,
            underline: false,
            spacer: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CursorView {
    pub row: usize,
    pub col: usize,
    pub shape: CursorShape,
}

#[derive(Clone, Debug)]
pub struct GridSnapshot {
    pub cols: usize,
    pub rows: usize,
    /// `lines[row][col]`, viewport coordinates (row 0 = top of the visible screen).
    pub lines: Vec<Vec<CellView>>,
    /// `None` when the program hid the cursor.
    pub cursor: Option<CursorView>,
}

/// The last `n` non-blank lines of the *live* screen (top to bottom), trailing spaces trimmed.
/// Used by the status heuristics to look for a question near the bottom of the screen. Reads
/// `grid()[Line(r)]` directly, which is always the live screen whatever the display offset, so
/// scrolling back through history doesn't change the agent's status.
pub fn screen_tail_text<L: EventListener>(term: &Term<L>, n: usize) -> String {
    let mut tail: Vec<String> = (0..term.screen_lines())
        .rev()
        .map(|r| {
            (0..term.columns())
                .map(|c| term.grid()[Line(r as i32)][Column(c)].c)
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .filter(|text| !text.is_empty())
        .take(n)
        .collect();
    tail.reverse();
    tail.join("\n")
}

/// Copy the visible screen out of the `Term` (call while holding its lock, then release it).
pub fn snapshot<L: EventListener>(term: &Term<L>, theme: &Theme) -> GridSnapshot {
    let rows = term.screen_lines();
    let cols = term.columns();
    let content = term.renderable_content();
    let overrides = content.colors;
    let selection = content.selection;
    // Grid lines are negative when scrolled back; viewport row = line + display_offset.
    let offset = content.display_offset as i32;
    let mut lines = vec![vec![CellView::blank(theme); cols]; rows];

    for indexed in content.display_iter {
        let row = indexed.point.line.0 + offset;
        let col = indexed.point.column.0;
        if row < 0 || row as usize >= rows || col >= cols {
            continue;
        }
        let cell = indexed.cell;
        let flags = cell.flags;
        let mut fg = colors::resolve(cell.fg, overrides, theme);
        let mut bg = colors::resolve(cell.bg, overrides, theme);
        if flags.contains(Flags::DIM) {
            fg = colors::dim(fg);
        }
        if flags.contains(Flags::INVERSE) {
            std::mem::swap(&mut fg, &mut bg);
        }
        // After the inverse swap, so a selection always shows.
        if selection.is_some_and(|range| range.contains(indexed.point)) {
            match theme.selection_background {
                Some(selected_bg) => {
                    bg = selected_bg;
                    if let Some(selected_fg) = theme.selection_foreground {
                        fg = selected_fg;
                    }
                }
                // Unset: swap the window's colors (as Ghostty does), so gray text stays readable.
                None => {
                    bg = theme.foreground;
                    fg = theme.selection_foreground.unwrap_or(theme.background);
                }
            }
        }
        lines[row as usize][col] = CellView {
            ch: if flags.contains(Flags::HIDDEN) {
                ' '
            } else {
                cell.c
            },
            fg,
            bg,
            bold: flags.contains(Flags::BOLD),
            italic: flags.contains(Flags::ITALIC),
            underline: flags.intersects(Flags::ALL_UNDERLINES),
            spacer: flags.intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER),
        };
    }

    let c = content.cursor;
    let cursor_row = c.point.line.0 + offset;
    let cursor = (c.shape != CursorShape::Hidden
        && cursor_row >= 0
        && (cursor_row as usize) < rows)
        .then(|| CursorView {
            row: cursor_row as usize,
            col: c.point.column.0,
            shape: c.shape,
        });

    GridSnapshot {
        cols,
        rows,
        lines,
        cursor,
    }
}

/// A horizontal span of cells sharing a non-default background color.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BgRect {
    pub col: usize,
    pub cells: usize,
    pub color: Rgb,
}

/// A horizontal span of cells drawn with one text style (shaped as one GPUI line).
#[derive(Clone, Debug, PartialEq)]
pub struct TextSpan {
    pub col: usize,
    pub cells: usize,
    pub text: String,
    pub fg: Rgb,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
}

/// Merge adjacent same-colored backgrounds; the default background is painted once for the
/// whole view, so it's skipped here.
pub fn bg_rects(line: &[CellView], default_bg: Rgb) -> Vec<BgRect> {
    let mut out: Vec<BgRect> = Vec::new();
    for (col, cell) in line.iter().enumerate() {
        if cell.bg == default_bg {
            continue;
        }
        match out.last_mut() {
            Some(r) if r.color == cell.bg && r.col + r.cells == col => r.cells += 1,
            _ => out.push(BgRect {
                col,
                cells: 1,
                color: cell.bg,
            }),
        }
    }
    out
}

/// Group contiguous same-style cells into spans. Spaces extend a span but never start one,
/// so blank areas cost nothing; wide-glyph spacers are skipped (and so end the span).
pub fn text_spans(line: &[CellView]) -> Vec<TextSpan> {
    let mut out: Vec<TextSpan> = Vec::new();
    for (col, cell) in line.iter().enumerate() {
        if cell.spacer {
            continue;
        }
        let continues = |s: &TextSpan| {
            s.col + s.cells == col
                && s.fg == cell.fg
                && s.bold == cell.bold
                && s.italic == cell.italic
                && s.underline == cell.underline
        };
        match out.last_mut() {
            Some(s) if continues(s) => {
                s.text.push(cell.ch);
                s.cells += 1;
            }
            _ if cell.ch == ' ' && !cell.underline => {}
            _ => out.push(TextSpan {
                col,
                cells: 1,
                text: cell.ch.to_string(),
                fg: cell.fg,
                bold: cell.bold,
                italic: cell.italic,
                underline: cell.underline,
            }),
        }
    }
    // Trailing spaces draw nothing (unless underlined): drop them.
    for s in &mut out {
        if !s.underline {
            let trimmed = s.text.trim_end_matches(' ').len();
            s.cells -= s.text.len() - trimmed;
            s.text.truncate(trimmed);
        }
    }
    out
}

/// Underline every visible cell from `start` to `end` inclusive, wrapping across lines.
pub fn underline_cells(snap: &mut GridSnapshot, start: Point, end: Point, display_offset: usize) {
    let offset = display_offset as i32;
    for line in start.line.0..=end.line.0 {
        let row = line + offset;
        if row < 0 || row as usize >= snap.rows {
            continue;
        }
        let from = if line == start.line.0 {
            start.column.0
        } else {
            0
        };
        let to = if line == end.line.0 {
            end.column.0
        } else {
            snap.cols.saturating_sub(1)
        };
        for cell in snap.lines[row as usize].iter_mut().take(to + 1).skip(from) {
            cell.underline = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::TermDims;
    use alacritty_terminal::event::VoidListener;
    use alacritty_terminal::grid::Scroll;
    use alacritty_terminal::index::Side;
    use alacritty_terminal::selection::{Selection, SelectionType};
    use alacritty_terminal::term::Config;
    use alacritty_terminal::vte::ansi::{Processor, StdSyncHandler};

    fn term_with(bytes: &[u8]) -> Term<VoidListener> {
        let mut term = Term::new(
            Config::default(),
            &TermDims { cols: 10, rows: 3 },
            VoidListener,
        );
        let mut processor: Processor<StdSyncHandler> = Processor::new();
        processor.advance(&mut term, bytes);
        term
    }

    fn cell(ch: char, fg: Rgb, bg: Rgb) -> CellView {
        CellView {
            ch,
            fg,
            bg,
            ..CellView::blank(&Theme::default())
        }
    }

    const RED: Rgb = Rgb {
        r: 0xcc,
        g: 0x66,
        b: 0x66,
    };

    #[test]
    fn snapshot_copies_text_colors_and_cursor() {
        let snap = snapshot(&term_with(b"hi\x1b[31mR"), &Theme::default());
        assert_eq!((snap.cols, snap.rows), (10, 3));
        assert_eq!(snap.lines[0][0].ch, 'h');
        assert_eq!(snap.lines[0][1].ch, 'i');
        assert_eq!(snap.lines[0][2].ch, 'R');
        assert_eq!(snap.lines[0][2].fg, RED);
        assert_eq!(snap.lines[0][0].fg, Theme::default().foreground);
        assert_eq!(snap.cursor.map(|c| (c.row, c.col)), Some((0, 3)));
    }

    #[test]
    fn snapshot_applies_inverse_and_hidden_cursor() {
        let snap = snapshot(&term_with(b"\x1b[7mX\x1b[?25l"), &Theme::default());
        assert_eq!(snap.lines[0][0].fg, Theme::default().background);
        assert_eq!(snap.lines[0][0].bg, Theme::default().foreground);
        assert_eq!(snap.cursor, None);
    }

    #[test]
    fn bg_rects_merge_adjacent_same_color() {
        let d = Theme::default().background;
        let line = [
            cell('a', RED, RED),
            cell('b', RED, RED),
            cell('c', RED, d),
            cell('d', RED, RED),
        ];
        assert_eq!(
            bg_rects(&line, d),
            vec![
                BgRect {
                    col: 0,
                    cells: 2,
                    color: RED
                },
                BgRect {
                    col: 3,
                    cells: 1,
                    color: RED
                }
            ]
        );
    }

    #[test]
    fn text_spans_split_on_style_and_skip_leading_blanks() {
        let d = Theme::default().background;
        let f = Theme::default().foreground;
        let line = [
            cell(' ', f, d),
            cell('l', f, d),
            cell(' ', f, d),
            cell('s', f, d),
            cell(' ', f, d),
            cell('x', RED, d),
        ];
        let spans = text_spans(&line);
        assert_eq!(spans.len(), 2);
        // inner space kept, trailing space trimmed
        assert_eq!(
            (spans[0].col, spans[0].text.as_str(), spans[0].cells),
            (1, "l s", 3)
        );
        assert_eq!(
            (spans[1].col, spans[1].text.as_str(), spans[1].fg),
            (5, "x", RED)
        );
    }

    #[test]
    fn blank_line_has_no_spans_or_rects() {
        let line = vec![CellView::blank(&Theme::default()); 10];
        assert!(text_spans(&line).is_empty());
        assert!(bg_rects(&line, Theme::default().background).is_empty());
    }

    #[test]
    fn tail_text_returns_last_non_blank_lines() {
        let term = term_with(b"one\r\n\r\ntwo  ");
        assert_eq!(screen_tail_text(&term, 8), "one\ntwo");
        assert_eq!(screen_tail_text(&term, 1), "two");
    }

    #[test]
    fn screen_tail_ignores_scrollback() {
        let text: String = (1..=30).map(|i| format!("l{i}\r\n")).collect();
        let mut term = Term::new(
            Config::default(),
            &TermDims { cols: 10, rows: 5 },
            VoidListener,
        );
        let mut p: Processor<StdSyncHandler> = Processor::new();
        p.advance(&mut term, text.as_bytes());
        term.scroll_display(Scroll::Delta(3));
        // The view shows history (l24 at the top)...
        assert_eq!(
            snapshot(&term, &Theme::default()).lines[0]
                .iter()
                .map(|c| c.ch)
                .collect::<String>()
                .trim_end(),
            "l24"
        );
        // ...but the status tail still reads the live bottom.
        assert_eq!(screen_tail_text(&term, 1), "l30");
    }

    #[test]
    fn wide_glyph_spacer_is_skipped() {
        let snap = snapshot(&term_with("界a".as_bytes()), &Theme::default());
        assert_eq!(snap.lines[0][0].ch, '界');
        assert!(snap.lines[0][1].spacer);
        let spans = text_spans(&snap.lines[0]);
        assert_eq!(spans[0].text, "界");
        assert_eq!((spans[1].col, spans[1].text.as_str()), (2, "a"));
    }

    #[test]
    fn selected_cells_get_the_selection_background() {
        let mut term = term_with(b"hello");
        let mut sel = Selection::new(
            SelectionType::Simple,
            Point::new(Line(0), Column(0)),
            Side::Left,
        );
        sel.update(Point::new(Line(0), Column(1)), Side::Right);
        term.selection = Some(sel);
        let snap = snapshot(&term, &Theme::default());
        let t = Theme::default();
        // No selection color set: the window's colors swap, so gray text stays readable.
        assert_eq!(snap.lines[0][0].bg, t.foreground);
        assert_eq!(snap.lines[0][0].fg, t.background);
        assert_eq!(snap.lines[0][1].bg, t.foreground);
        assert_eq!(snap.lines[0][2].bg, Theme::default().background);
    }

    #[test]
    fn explicit_selection_colors_apply() {
        let selected = |theme: &Theme| {
            let mut term = term_with(b"hello");
            let mut sel = Selection::new(
                SelectionType::Simple,
                Point::new(Line(0), Column(0)),
                Side::Left,
            );
            sel.update(Point::new(Line(0), Column(1)), Side::Right);
            term.selection = Some(sel);
            let cell = snapshot(&term, theme).lines[0][0].clone();
            (cell.bg, cell.fg)
        };
        let x = Rgb { r: 1, g: 2, b: 3 };
        let y = Rgb { r: 4, g: 5, b: 6 };
        let mut t = Theme::default();
        t.selection_background = Some(x);
        t.selection_foreground = Some(y);
        assert_eq!(selected(&t), (x, y));
        t.selection_background = None;
        assert_eq!(selected(&t), (t.foreground, y));
    }

    #[test]
    fn underline_cells_marks_the_link() {
        let mut snap = snapshot(&term_with(b"ab https"), &Theme::default());
        underline_cells(
            &mut snap,
            Point::new(Line(0), Column(3)),
            Point::new(Line(0), Column(7)),
            0,
        );
        assert!(
            !snap.lines[0][2].underline && snap.lines[0][3].underline && snap.lines[0][7].underline
        );
    }
}
