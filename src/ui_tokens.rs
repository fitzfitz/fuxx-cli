//! Design tokens for the window chrome (M11): WCAG contrast, the contrast floors, and the sizes
//! and text sizes every UI piece is drawn with.
//!
//! Pure. `contrast` is the WCAG 2.x ratio (sRGB-linearised relative luminance). `ensure_contrast`
//! moves a colour toward black or white, keeping its hue, until it reads on a background;
//! `Theme::ui()` uses it so every built-in theme, dark or light, meets the floors below.

use alacritty_terminal::vte::ansi::Rgb;

/// Text on its background (WCAG AA for normal text).
pub const TEXT_MIN: f32 = 4.5;
/// Status marks and focus/accent lines on their background (WCAG non-text contrast).
pub const MARK_MIN: f32 = 3.0;
/// Plain 1 px separators (the sidebar edge, split dividers, panel borders). Deliberately below
/// `MARK_MIN`: the spec wants dividers "1 px and quiet", and a 3:1 hairline reads as heavy. Lines
/// that carry meaning (the accent bar, a divider while hovered or dragged) use `MARK_MIN`.
pub const QUIET_LINE_MIN: f32 = 1.4;
/// The pane tab bar's empty strip against the card (the active tab's colour): enough that the
/// two read as different surfaces. Stepped to from the canvas colour, within the floors.
pub const TAB_BAR_MIN: f32 = 1.3;
/// How far a light theme's tab bar may step from the canvas colour: further, and text, dim
/// and faint text (all held to 4.5:1 on it) press together.
pub const TAB_BAR_LIGHT_CAP: f32 = 1.15;
/// What the strip still reaches against the card where `TAB_BAR_MIN` can't be had (a light
/// theme at its cap, or a dark one whose bar is already black): checked by the unit test and
/// the `tab-bars` ui-shot.
#[cfg(any(test, feature = "ui-shots"))]
pub const TAB_BAR_LOW_MIN: f32 = 1.15;

/// Sizes in pixels, from the M11 plan's Global Constraints.
pub mod size {
    #![allow(dead_code)] // drawn with from M11 Task 2 on

    /// A sidebar row with its name line only, and one with a second line under it (where it
    /// is, what it waits on, a script's note, or "not live") (M11i: `sidebar::row_height`).
    pub const SIDEBAR_ROW: f32 = 22.0;
    pub const SIDEBAR_ROW_TWO: f32 = 32.0;
    pub const GROUP_HEADER: f32 = 24.0;
    /// A pane's tab bar (M11b; it replaced M11's 22 px pane header; 30 since M11i).
    pub const TAB_BAR: f32 = 30.0;
    /// The band at the top of the window that the traffic lights sit in (the drag area).
    pub const TOP_BAND: f32 = 38.0;
    /// Where the traffic lights go: (x, y) from the window's top left.
    pub const TRAFFIC_LIGHTS: (f32, f32) = (13.0, 12.0);
    /// Terminal padding: (sides, top and bottom); smaller in splits. Both are larger than
    /// `CARD_RADIUS`, so nothing the grid paints reaches into a card's rounded corner (M11i).
    pub const TERMINAL_PADDING: (f32, f32) = (16.0, 12.0);
    pub const SPLIT_TERMINAL_PADDING: (f32, f32) = (12.0, 10.0);
    /// M11i's raised cards (each pane, the right sidebar): the gap around and between them,
    /// their corner radius, and a focused pane's edge in a split.
    pub const CARD_GAP: f32 = 6.0;
    pub const CARD_RADIUS: f32 = 8.0;
    pub const FOCUS_RING: f32 = 1.0;
    /// The status bar along the window's bottom edge (M11i).
    pub const STATUS_BAR: f32 = 22.0;
    /// How far below the tab bar's top its tabs start (M11i).
    pub const TAB_TOP_GAP: f32 = 4.0;
    /// The accent bar at the selected sidebar row's left edge (M11i).
    pub const SELECTION_BAR: f32 = 2.0;
    pub const SIDEBAR_MIN_WIDTH: f32 = 180.0;
    pub const SIDEBAR_MAX_WIDTH: f32 = 400.0;
    pub const SIDEBAR_DEFAULT_WIDTH: f32 = 240.0;
    /// The right sidebar's (M11g).
    pub const RIGHT_SIDEBAR_MIN_WIDTH: f32 = 220.0;
    pub const RIGHT_SIDEBAR_MAX_WIDTH: f32 = 560.0;
    pub const RIGHT_SIDEBAR_DEFAULT_WIDTH: f32 = 300.0;
    /// The box a status mark is drawn in.
    pub const STATUS_MARK: f32 = 12.0;
    pub const DIVIDER: f32 = 1.0;
    /// A divider while hovered or dragged (in the accent colour).
    pub const DIVIDER_ACTIVE: f32 = 2.0;
    /// How much an unfocused pane is veiled with the terminal background (0-1).
    pub const UNFOCUSED_VEIL: f32 = 0.4;
}

/// Text sizes in pixels. Numbers and meta use tabular figures; group headers get letter spacing.
pub mod text {
    #![allow(dead_code)] // drawn with from M11 Task 2 on

    pub const NAME: f32 = 13.0;
    pub const META: f32 = 11.0;
    pub const GROUP_HEADER: f32 = 11.0;
}

const BLACK: Rgb = Rgb { r: 0, g: 0, b: 0 };
const WHITE: Rgb = Rgb {
    r: 255,
    g: 255,
    b: 255,
};

/// WCAG relative luminance (0 black, 1 white): each sRGB channel linearised, then weighted.
fn relative_luminance(c: Rgb) -> f32 {
    let lin = |v: u8| {
        let v = v as f32 / 255.0;
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * lin(c.r) + 0.7152 * lin(c.g) + 0.0722 * lin(c.b)
}

/// The WCAG 2.x contrast ratio of two colours, from 1 (same) to 21 (black on white).
pub fn contrast(a: Rgb, b: Rgb) -> f32 {
    let (la, lb) = (relative_luminance(a), relative_luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

/// `fg` if it already has `min` contrast on `bg`; otherwise `fg` mixed toward black or white (so
/// its hue stays the same) just far enough to reach `min`. When even black and white fall short,
/// whichever of them contrasts more.
pub fn ensure_contrast(fg: Rgb, bg: Rgb, min: f32) -> Rgb {
    if contrast(fg, bg) >= min {
        return fg;
    }
    // Away from the background first; if that end can't reach `min`, the other end may.
    let (away, back) = if relative_luminance(fg) >= relative_luminance(bg) {
        (WHITE, BLACK)
    } else {
        (BLACK, WHITE)
    };
    let target = if contrast(away, bg) >= min {
        away
    } else if contrast(back, bg) >= min {
        back
    } else if contrast(away, bg) >= contrast(back, bg) {
        return away;
    } else {
        return back;
    };
    // Mixing toward black or white scales every channel difference by the same factor, so the
    // hue is kept. Along the way the contrast only falls below `min` before some point and stays
    // above it after, so a binary search finds the smallest mix that reaches it.
    let (mut lo, mut hi) = (0.0, 1.0);
    for _ in 0..16 {
        let mid = (lo + hi) / 2.0;
        if contrast(crate::theme::mix(fg, target, mid), bg) >= min {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    crate::theme::mix(fg, target, hi)
}

/// Hue in degrees (HSV), for tests that check a colour kept its hue. Panics on greys.
#[cfg(test)]
pub fn hue(c: Rgb) -> f32 {
    let (r, g, b) = (c.r as f32, c.g as f32, c.b as f32);
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let d = max - min;
    assert!(d > 0.0, "{c:?} is grey");
    let h = if max == r {
        ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    h * 60.0
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn rgb(v: u32) -> Rgb {
        Rgb {
            r: (v >> 16) as u8,
            g: (v >> 8) as u8,
            b: v as u8,
        }
    }

    #[test]
    fn contrast_is_the_wcag_ratio() {
        assert!((contrast(rgb(0x000000), rgb(0xffffff)) - 21.0).abs() < 0.01);
        assert!((contrast(rgb(0xffffff), rgb(0x000000)) - 21.0).abs() < 0.01);
        assert!((contrast(rgb(0x777777), rgb(0x777777)) - 1.0).abs() < 0.001);
        // Known value: #767676 on white is the classic 4.54:1.
        assert!((contrast(rgb(0x767676), rgb(0xffffff)) - 4.54).abs() < 0.01);
    }

    #[test]
    fn ensure_contrast_keeps_hue() {
        let cases = [
            // Catppuccin Latte's amber on a light sidebar: darkens.
            (rgb(0xdf8e1d), rgb(0xe8ebef), MARK_MIN),
            // Gruvbox's red on its dark sidebar: lightens.
            (rgb(0xcc241d), rgb(0x212121), MARK_MIN),
            // A dull blue as text on a dark background.
            (rgb(0x3b4a7a), rgb(0x1e1e2e), TEXT_MIN),
        ];
        for (fg, bg, min) in cases {
            assert!(contrast(fg, bg) < min, "{fg:?} already passes");
            let out = ensure_contrast(fg, bg, min);
            assert!(contrast(out, bg) >= min, "{fg:?} → {out:?} on {bg:?}");
            // Just far enough: not pushed all the way to black or white.
            assert!(contrast(out, bg) < min + 0.5, "{fg:?} → {out:?} overshoots");
            let d = (hue(out) - hue(fg)).abs();
            assert!(d.min(360.0 - d) < 3.0, "{fg:?} → {out:?} changed hue");
        }
    }

    #[test]
    fn ensure_contrast_leaves_passing_colours_alone() {
        let fg = rgb(0xcdd6f4);
        assert_eq!(ensure_contrast(fg, rgb(0x1e1e2e), TEXT_MIN), fg);
    }

    #[test]
    fn ensure_contrast_turns_around_when_one_way_cannot_reach() {
        // Slightly lighter than a mid-grey background: white can't reach 4.5:1 there, black can.
        let bg = rgb(0x808080);
        let out = ensure_contrast(rgb(0x8a8590), bg, TEXT_MIN);
        assert!(contrast(out, bg) >= TEXT_MIN, "{out:?}");
        // Impossible floors give the better of black and white.
        assert_eq!(ensure_contrast(rgb(0x8a8590), bg, 30.0), rgb(0x000000));
    }
}
