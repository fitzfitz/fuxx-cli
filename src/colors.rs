//! Resolve `alacritty_terminal` cell colors to concrete RGB.
//!
//! `Term::colors()` only holds colors a program overrode (OSC 4/10/11/12) and is otherwise all
//! `None`, so we supply the palette ourselves, from the `Theme`. Pure, no GPUI types.

use alacritty_terminal::term::color::Colors;
use alacritty_terminal::vte::ansi::{Color, Rgb};

use crate::theme::Theme;

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

/// The xterm 256-color table: the theme's 16 palette colors, a 6x6x6 cube, then a 24-step gray ramp.
pub fn indexed_rgb(i: u8, theme: &Theme) -> Rgb {
    match i {
        0..=15 => theme.palette[i as usize],
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
pub fn default_for_index(idx: usize, theme: &Theme) -> Rgb {
    match idx {
        0..=255 => indexed_rgb(idx as u8, theme),
        256 | 267 => theme.foreground,
        257 => theme.background,
        258 => theme.cursor.unwrap_or(theme.foreground),
        259..=266 => dim(theme.palette[idx - 259]),
        268 => dim(theme.foreground),
        _ => theme.foreground,
    }
}

/// A program's override wins; otherwise our palette.
pub fn resolve_index(idx: usize, overrides: &Colors, theme: &Theme) -> Rgb {
    overrides[idx].unwrap_or_else(|| default_for_index(idx, theme))
}

pub fn resolve(color: Color, overrides: &Colors, theme: &Theme) -> Rgb {
    match color {
        Color::Spec(rgb) => rgb,
        Color::Indexed(i) => resolve_index(i as usize, overrides, theme),
        Color::Named(n) => resolve_index(n as usize, overrides, theme),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn hex(v: u32) -> Rgb {
        Rgb {
            r: (v >> 16) as u8,
            g: (v >> 8) as u8,
            b: v as u8,
        }
    }

    use crate::theme::Theme;
    use alacritty_terminal::vte::ansi::NamedColor;

    #[test]
    fn palette_and_cube_and_gray_ramp() {
        assert_eq!(indexed_rgb(1, &Theme::default()), hex(0xcc6666));
        assert_eq!(
            indexed_rgb(196, &Theme::default()),
            Rgb { r: 255, g: 0, b: 0 }
        );
        assert_eq!(
            indexed_rgb(231, &Theme::default()),
            Rgb {
                r: 255,
                g: 255,
                b: 255
            }
        );
        assert_eq!(
            indexed_rgb(232, &Theme::default()),
            Rgb { r: 8, g: 8, b: 8 }
        );
        assert_eq!(
            indexed_rgb(255, &Theme::default()),
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
        let t = Theme::default();
        assert_eq!(
            resolve(Color::Named(NamedColor::Foreground), &none, &t),
            t.foreground
        );
        assert_eq!(
            resolve(Color::Named(NamedColor::Background), &none, &t),
            t.background
        );
        assert_eq!(
            resolve(Color::Named(NamedColor::Red), &none, &t),
            hex(0xcc6666)
        );
    }

    #[test]
    fn spec_colors_pass_through_and_overrides_win() {
        let mut colors = Colors::default();
        let pink = Rgb { r: 1, g: 2, b: 3 };
        assert_eq!(resolve(Color::Spec(pink), &colors, &Theme::default()), pink);
        colors[NamedColor::Red] = Some(pink);
        assert_eq!(
            resolve(Color::Named(NamedColor::Red), &colors, &Theme::default()),
            pink
        );
    }

    #[test]
    fn named_colors_and_palette_follow_the_theme() {
        let mut t = Theme::default();
        t.background = hex(0x000000);
        t.palette[1] = hex(0xff0000);
        let none = Colors::default();
        assert_eq!(
            resolve(Color::Named(NamedColor::Background), &none, &t),
            hex(0x000000)
        );
        assert_eq!(indexed_rgb(1, &t), hex(0xff0000));
        assert_eq!(indexed_rgb(196, &t), hex(0xff0000));
        assert_eq!(indexed_rgb(232, &t), hex(0x080808));
    }
}
