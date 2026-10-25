//! Resolve `alacritty_terminal` cell colors to concrete RGB.
//!
//! `Term::colors()` only holds colors a program overrode (OSC 4/10/11/12) and is otherwise all
//! `None`, so we supply the palette ourselves: Alacritty's defaults
//! (alacritty/src/config/color.rs). Pure, no GPUI types.

use alacritty_terminal::term::color::Colors;
use alacritty_terminal::vte::ansi::{Color, Rgb};

const fn hex(v: u32) -> Rgb {
    Rgb {
        r: (v >> 16) as u8,
        g: (v >> 8) as u8,
        b: v as u8,
    }
}

pub const FOREGROUND: Rgb = hex(0xd8d8d8);
pub const BACKGROUND: Rgb = hex(0x181818);

/// 0-7 normal, 8-15 bright.
const ANSI_16: [Rgb; 16] = [
    hex(0x181818),
    hex(0xac4242),
    hex(0x90a959),
    hex(0xf4bf75),
    hex(0x6a9fb5),
    hex(0xaa759f),
    hex(0x75b5aa),
    hex(0xd8d8d8),
    hex(0x6b6b6b),
    hex(0xc55555),
    hex(0xaac474),
    hex(0xfeca88),
    hex(0x82b8c8),
    hex(0xc28cb8),
    hex(0x93d3c3),
    hex(0xf8f8f8),
];

/// Alacritty dims colors to 66% brightness.
const DIM_FACTOR: f32 = 0.66;

pub fn dim(c: Rgb) -> Rgb {
    let f = |v: u8| (v as f32 * DIM_FACTOR) as u8;
    Rgb {
        r: f(c.r),
        g: f(c.g),
        b: f(c.b),
    }
}

/// The xterm 256-color table: 16 palette colors, a 6x6x6 cube, then a 24-step gray ramp.
pub fn indexed_rgb(i: u8) -> Rgb {
    match i {
        0..=15 => ANSI_16[i as usize],
        16..=231 => {
            let i = i - 16;
            let (r, g, b) = (i / 36, (i % 36) / 6, i % 6);
            let step = |v: u8| if v == 0 { 0 } else { v * 40 + 55 };
            Rgb {
                r: step(r),
                g: step(g),
                b: step(b),
            }
        }
        232..=255 => {
            let v = (i - 232) * 10 + 8;
            Rgb { r: v, g: v, b: v }
        }
    }
}

/// Default for any index in alacritty's 269-slot color table
/// (0-255 indexed, 256 fg, 257 bg, 258 cursor, 259-266 dim 0-7, 267 bright fg, 268 dim fg).
pub fn default_for_index(idx: usize) -> Rgb {
    match idx {
        0..=255 => indexed_rgb(idx as u8),
        256 | 258 | 267 => FOREGROUND,
        257 => BACKGROUND,
        259..=266 => dim(ANSI_16[idx - 259]),
        268 => dim(FOREGROUND),
        _ => FOREGROUND,
    }
}

/// A program's override wins; otherwise our palette.
pub fn resolve_index(idx: usize, overrides: &Colors) -> Rgb {
    overrides[idx].unwrap_or_else(|| default_for_index(idx))
}

pub fn resolve(color: Color, overrides: &Colors) -> Rgb {
    match color {
        Color::Spec(rgb) => rgb,
        Color::Indexed(i) => resolve_index(i as usize, overrides),
        Color::Named(n) => resolve_index(n as usize, overrides),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alacritty_terminal::vte::ansi::NamedColor;

    #[test]
    fn palette_and_cube_and_gray_ramp() {
        assert_eq!(indexed_rgb(1), hex(0xac4242));
        assert_eq!(indexed_rgb(196), Rgb { r: 255, g: 0, b: 0 });
        assert_eq!(
            indexed_rgb(231),
            Rgb {
                r: 255,
                g: 255,
                b: 255
            }
        );
        assert_eq!(indexed_rgb(232), Rgb { r: 8, g: 8, b: 8 });
        assert_eq!(
            indexed_rgb(255),
            Rgb {
                r: 238,
                g: 238,
                b: 238
            }
        );
    }

    #[test]
    fn named_defaults() {
        let none = Colors::default();
        assert_eq!(
            resolve(Color::Named(NamedColor::Foreground), &none),
            FOREGROUND
        );
        assert_eq!(
            resolve(Color::Named(NamedColor::Background), &none),
            BACKGROUND
        );
        assert_eq!(resolve(Color::Named(NamedColor::Red), &none), hex(0xac4242));
    }

    #[test]
    fn spec_colors_pass_through_and_overrides_win() {
        let mut colors = Colors::default();
        let pink = Rgb { r: 1, g: 2, b: 3 };
        assert_eq!(resolve(Color::Spec(pink), &colors), pink);
        colors[NamedColor::Red] = Some(pink);
        assert_eq!(resolve(Color::Named(NamedColor::Red), &colors), pink);
    }
}
