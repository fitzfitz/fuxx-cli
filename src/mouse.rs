//! Mouse helpers: pixel to grid position, wheel accumulation and terminal mouse reports.
//!
//! Pure functions, no GPUI types, so they are unit-tested. The view feeds GPUI mouse events in.
//! Position mapping and report encodings (SGR and normal/UTF-8) follow Zed's
//! `crates/terminal/src/mappings/mouse.rs` (xterm conventions).

use alacritty_terminal::index::{Column, Line, Point, Side};
use alacritty_terminal::term::TermMode;

use crate::terminal::TermDims;

/// Map a pixel position (relative to the grid's top-left) to a grid point and the cell half hit.
pub fn grid_point(
    x: f32,
    y: f32,
    cell_width: f32,
    line_height: f32,
    dims: TermDims,
    display_offset: usize,
) -> (Point, Side) {
    let max_col = dims.cols.saturating_sub(1);
    let max_row = dims.rows.saturating_sub(1);

    let (col, side) = if x < 0.0 {
        (0, Side::Left)
    } else {
        let raw = (x / cell_width) as usize;
        if raw > max_col {
            (max_col, Side::Right)
        } else if x - raw as f32 * cell_width > cell_width / 2.0 {
            (raw, Side::Right)
        } else {
            (raw, Side::Left)
        }
    };

    let (row, side) = if y < 0.0 {
        (0, Side::Left)
    } else {
        let raw = (y / line_height) as usize;
        if raw > max_row {
            (max_row, Side::Right)
        } else {
            (raw, side)
        }
    };

    (
        Point::new(Line(row as i32 - display_offset as i32), Column(col)),
        side,
    )
}

/// Accumulates wheel pixel deltas so fractional lines are not lost between events.
#[derive(Default)]
pub struct ScrollAccumulator {
    total: f32,
}

impl ScrollAccumulator {
    pub fn reset(&mut self) {
        self.total = 0.0;
    }

    /// Returns the whole lines crossed by this delta. Positive scrolls up into history.
    pub fn push(&mut self, delta_px: f32, line_height: f32) -> i32 {
        let old = (self.total / line_height).trunc() as i32;
        self.total += delta_px;
        (self.total / line_height).trunc() as i32 - old
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    Left,
    Middle,
    Right,
    WheelUp,
    WheelDown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseAction {
    Press,
    Release,
    Drag,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mods {
    pub shift: bool,
    pub alt: bool,
    pub ctrl: bool,
}

/// Encode a mouse event for the running program, or `None` if it should not be reported.
/// `row` is the viewport row.
pub fn report(
    button: Button,
    action: MouseAction,
    mods: Mods,
    col: usize,
    row: usize,
    mode: TermMode,
) -> Option<Vec<u8>> {
    if !mode.intersects(TermMode::MOUSE_MODE) {
        return None;
    }
    let wheel = matches!(button, Button::WheelUp | Button::WheelDown);
    if wheel && action == MouseAction::Release {
        return None;
    }
    if action == MouseAction::Drag
        && !mode.intersects(TermMode::MOUSE_DRAG | TermMode::MOUSE_MOTION)
    {
        return None;
    }

    let mut code: u8 = match button {
        Button::Left => 0,
        Button::Middle => 1,
        Button::Right => 2,
        Button::WheelUp => 64,
        Button::WheelDown => 65,
    };
    if action == MouseAction::Drag {
        code += 32;
    }
    let mut mod_bits = 0;
    if mods.shift {
        mod_bits += 4;
    }
    if mods.alt {
        mod_bits += 8;
    }
    if mods.ctrl {
        mod_bits += 16;
    }

    if mode.contains(TermMode::SGR_MOUSE) {
        let end = if action == MouseAction::Release {
            'm'
        } else {
            'M'
        };
        return Some(
            format!("\x1b[<{};{};{}{}", code + mod_bits, col + 1, row + 1, end).into_bytes(),
        );
    }

    // Normal encoding: release has no button identity (code 3).
    let base = if action == MouseAction::Release {
        3
    } else {
        code
    };
    let mut out = vec![0x1b, b'[', b'M', 32 + base + mod_bits];
    let utf8 = mode.contains(TermMode::UTF8_MOUSE);
    for pos in [col, row] {
        if utf8 {
            if pos >= 2015 {
                return None;
            }
            let ch = char::from_u32(33 + pos as u32)?;
            let mut buf = [0u8; 4];
            out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
        } else {
            if pos >= 223 {
                return None;
            }
            out.push(33 + pos as u8);
        }
    }
    Some(out)
}

/// Wheel lines as application-cursor arrows (alternate-screen scrolling).
pub fn alt_scroll(lines: i32) -> Vec<u8> {
    let seq: &[u8] = if lines > 0 { b"\x1bOA" } else { b"\x1bOB" };
    seq.repeat(lines.unsigned_abs() as usize)
}

/// Whether the mouse goes to the program (mouse mode on and shift not held to bypass it).
pub fn wants_mouse(mode: TermMode, shift: bool) -> bool {
    mode.intersects(TermMode::MOUSE_MODE) && !shift
}

#[cfg(test)]
mod tests {
    use super::*;

    const D: TermDims = TermDims { cols: 80, rows: 24 };
    const SGR: TermMode = TermMode::MOUSE_REPORT_CLICK.union(TermMode::SGR_MOUSE);

    #[test]
    fn grid_point_clamps_and_picks_side() {
        assert_eq!(
            grid_point(25.0, 45.0, 10.0, 20.0, D, 0),
            (Point::new(Line(2), Column(2)), Side::Left)
        );
        assert_eq!(
            grid_point(29.0, 45.0, 10.0, 20.0, D, 0),
            (Point::new(Line(2), Column(2)), Side::Right)
        );
        assert_eq!(
            grid_point(5000.0, 9000.0, 10.0, 20.0, D, 0),
            (Point::new(Line(23), Column(79)), Side::Right)
        );
    }

    #[test]
    fn grid_point_accounts_for_scrollback() {
        assert_eq!(
            grid_point(0.0, 45.0, 10.0, 20.0, D, 3).0,
            Point::new(Line(-1), Column(0))
        );
    }

    #[test]
    fn scroll_accumulator_keeps_fractions() {
        let mut acc = ScrollAccumulator::default();
        assert_eq!(
            [
                acc.push(10.0, 20.0),
                acc.push(15.0, 20.0),
                acc.push(-30.0, 20.0)
            ],
            [0, 1, -1]
        );
    }

    #[test]
    fn sgr_press_release_drag_and_wheel() {
        let m = Mods::default();
        assert_eq!(
            report(Button::Left, MouseAction::Press, m, 0, 0, SGR).unwrap(),
            b"\x1b[<0;1;1M"
        );
        assert_eq!(
            report(Button::Left, MouseAction::Release, m, 0, 0, SGR).unwrap(),
            b"\x1b[<0;1;1m"
        );
        let drag = SGR.union(TermMode::MOUSE_DRAG);
        assert_eq!(
            report(Button::Left, MouseAction::Drag, m, 4, 2, drag).unwrap(),
            b"\x1b[<32;5;3M"
        );
        assert_eq!(
            report(Button::WheelUp, MouseAction::Press, m, 0, 0, SGR).unwrap(),
            b"\x1b[<64;1;1M"
        );
    }

    #[test]
    fn modifiers_add_bits() {
        let ctrl = Mods {
            ctrl: true,
            ..Mods::default()
        };
        assert_eq!(
            report(Button::Right, MouseAction::Press, ctrl, 0, 0, SGR).unwrap(),
            b"\x1b[<18;1;1M"
        );
    }

    #[test]
    fn normal_encoding_and_limit() {
        let normal = TermMode::MOUSE_REPORT_CLICK;
        let m = Mods::default();
        assert_eq!(
            report(Button::Left, MouseAction::Press, m, 0, 0, normal).unwrap(),
            vec![0x1b, b'[', b'M', 32, 33, 33]
        );
        assert_eq!(
            report(Button::Left, MouseAction::Release, m, 1, 2, normal).unwrap(),
            vec![0x1b, b'[', b'M', 35, 34, 35]
        );
        assert_eq!(
            report(Button::Left, MouseAction::Press, m, 223, 0, normal),
            None
        );
        let utf8 = normal.union(TermMode::UTF8_MOUSE);
        assert_eq!(
            report(Button::Left, MouseAction::Press, m, 223, 0, utf8).unwrap(),
            [b"\x1b[M ".as_slice(), "\u{100}".as_bytes(), b"!"].concat()
        );
    }

    #[test]
    fn no_report_without_mouse_mode_and_drag_rules() {
        let m = Mods::default();
        assert_eq!(
            report(Button::Left, MouseAction::Press, m, 0, 0, TermMode::empty()),
            None
        );
        assert_eq!(report(Button::Left, MouseAction::Drag, m, 0, 0, SGR), None); // click-only mode
        assert_eq!(
            report(Button::WheelUp, MouseAction::Release, m, 0, 0, SGR),
            None
        );
        assert!(wants_mouse(SGR, false) && !wants_mouse(SGR, true));
    }

    #[test]
    fn alt_scroll_sends_ss3_arrows() {
        assert_eq!(alt_scroll(2), b"\x1bOA\x1bOA".to_vec());
        assert_eq!(alt_scroll(-1), b"\x1bOB".to_vec());
    }
}
