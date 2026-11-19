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

/// M11ah (code view upgrade): every size the file tab's code view is drawn with, in points, in
/// one place so M11al can move them together. Row = the font's line height x `ROW_SCALE`.
pub mod code {
    pub const ROW_SCALE: f32 = 1.2;
    /// The room above the first line, and left of the numbers.
    pub const TOP_ROOM: f32 = 16.0;
    pub const GUTTER_LEFT: f32 = 16.0;
    /// Numbers to the change bar, and the bar to the text.
    pub const NUMBER_GAP: f32 = 10.0;
    pub const TEXT_GAP: f32 = 14.0;
    pub const BAR_WIDTH: f32 = 3.0;
    pub const BAR_RUN_GAP: f32 = 2.0;
    pub const FOLD_COLUMN: f32 = 14.0;
    /// An open block's ▾ (only while the pointer is over the gutter): the numbers' ink at this
    /// alpha.
    pub const FOLD_OPEN_ALPHA: f32 = 0.55;
    /// A folded line's pill ("⋯ 24 lines"): this far after the line's text, its words this far
    /// in from its ends, its ground `text` at this alpha, this far in from the row's top and
    /// bottom.
    pub const FOLD_PILL_GAP: f32 = 8.0;
    pub const FOLD_PILL_PAD: f32 = 6.0;
    pub const FOLD_PILL_ALPHA: f32 = 0.08;
    pub const FOLD_PILL_INSET: f32 = 2.0;
    /// The current line's band: `text` at this alpha.
    pub const CURRENT_ALPHA: f32 = 0.05;
    pub const GUIDE: f32 = 1.0;
    pub const GUIDE_ALPHA: f32 = 0.07;
    pub const GUIDE_ACTIVE_ALPHA: f32 = 0.14;
    pub const STICKY_RULE: f32 = 1.0;
    pub const BREADCRUMB: f32 = 26.0;
    pub const BREADCRUMB_TEXT: f32 = 12.0;
    pub const LANE: f32 = 4.0;
    pub const LANE_MARK_MIN: f32 = 2.0;
    pub const LANE_TICK: f32 = 2.0;
    pub const NAV_BUTTON: f32 = 24.0;
    pub const CARD_INSET: f32 = 12.0;
    pub const CARD_BORDER: f32 = 1.0;
    pub const CARD_GAP: f32 = 10.0;
    pub const CARD_TITLE: f32 = 30.0;
    pub const UNCHANGED_TEXT: f32 = 12.0;
    pub const WORD_ALPHA: f32 = 0.35;
    pub const WORD_RADIUS: f32 = 2.0;
    pub const SIDE_HAIRLINE: f32 = 1.0;
    pub const SIDE_EMPTY_ALPHA: f32 = 0.03;
    /// M11ag: a note row's bar (in the agent's colour), and the room kept clear before its
    /// "more" / "less".
    pub const NOTE_BAR: f32 = 2.0;
    pub const NOTE_MORE_GAP: f32 = 8.0;
}

/// M11ai (Markdown reader): every size the file tab's Markdown Preview is drawn with, in points,
/// in one place (agent chat keeps its own, `md_preview::Sizes::CHAT`). The Preview's side and top
/// padding is `size::TERMINAL_PADDING`.
pub mod reader {
    /// Body text, and its line height (a multiple of the size).
    pub const BODY: f32 = 16.0;
    pub const BODY_LINE: f32 = 1.65;
    /// Headings H1–H6 (H6 dim), and their line height.
    pub const HEADINGS: [f32; 6] = [30.0, 24.0, 20.0, 17.0, 16.0, 15.0];
    pub const HEADING_LINE: f32 = 1.3;
    /// Between blocks; above and under a heading; between a list's items.
    pub const BLOCK_GAP: f32 = 16.0;
    pub const HEADING_ABOVE: f32 = 32.0;
    pub const HEADING_BELOW: f32 = 12.0;
    pub const ITEM_GAP: f32 = 6.0;
    /// Code (blocks and inline), in the terminal's font, and a block's line height.
    pub const CODE: f32 = 14.0;
    pub const CODE_LINE: f32 = 1.5;
    /// H1 / H2's underline, this far under the text.
    pub const RULE_BELOW: f32 = 8.0;
    /// The prose column's widest: about 70 characters of the UI font at `BODY`. Tuned (Task 3):
    /// the plan's 576 (36 em) holds about 84; a 70-character sentence shapes to 480 pt (30 em),
    /// checked by the `file-tab-md-reader-column` scene.
    pub const MEASURE: f32 = 480.0;
    /// A code block's header strip and its words; the code's padding (sides, top and bottom)
    /// and corners.
    pub const CODE_HEADER: f32 = 28.0;
    pub const CODE_HEADER_TEXT: f32 = 12.0;
    pub const CODE_PAD: (f32, f32) = (12.0, 10.0);
    pub const CODE_RADIUS: f32 = 6.0;
    /// The header strip's fill: the code's, this much toward black (a shade darker).
    pub const CODE_HEADER_SHADE: f32 = 0.05;
    /// Between the Copy button's icon and its word.
    pub const COPY_GAP: f32 = 4.0;
    /// A callout's bar, padding, corners, and its wash (the kind's colour at this alpha).
    pub const CALLOUT_BAR: f32 = 3.0;
    pub const CALLOUT_PAD: f32 = 12.0;
    pub const CALLOUT_RADIUS: f32 = 6.0;
    pub const CALLOUT_WASH: f32 = 0.08;
    /// A table cell's padding (sides, top and bottom), a column's narrowest, and a striped
    /// row's tint (the text at this alpha).
    pub const TABLE_CELL: (f32, f32) = (13.0, 7.0);
    pub const TABLE_COL_MIN: f32 = 96.0;
    pub const STRIPE: f32 = 0.035;
    /// A table's corners (M11ac's 4; the front matter's too).
    pub const TABLE_RADIUS: f32 = 4.0;
    /// The front matter's text, and the most key/value rows it shows ("+ N more" for the rest).
    pub const FRONT_TEXT: f32 = 13.0;
    pub const FRONT_ROWS: usize = 20;
    /// A badge's pill: height, text, padding, and the gap between pills.
    pub const PILL_H: f32 = 20.0;
    pub const PILL_TEXT: f32 = 12.0;
    pub const PILL_PAD: f32 = 7.0;
    pub const PILL_GAP: f32 = 6.0;
    /// A pill's fill, and its value half's (a step stronger): the text at these alphas over the
    /// page (Task 6; the plan names no number).
    pub const PILL_FILL: f32 = 0.08;
    pub const PILL_VALUE_FILL: f32 = 0.16;
    /// The front matter's grid (Task 6): of `.1` columns, a key takes `.0`, its value the rest.
    pub const FRONT_GRID: (u16, u16) = (1, 3);
    /// "On this page": its width, the gap left of it, its text, a row, a level-3 entry's
    /// indent, the current entry's bar.
    pub const OUTLINE_W: f32 = 200.0;
    pub const OUTLINE_GAP: f32 = 48.0;
    pub const OUTLINE_TEXT: f32 = 13.0;
    pub const OUTLINE_ROW: f32 = 26.0;
    pub const OUTLINE_INDENT: f32 = 12.0;
    pub const OUTLINE_BAR: f32 = 2.0;
    /// An entry's text (and the caption), this far in from the column's left edge, where the
    /// current entry's bar is (Task 8; the plan names no number).
    pub const OUTLINE_PAD: f32 = 10.0;
    /// The narrow outline's floating panel (Task 9; the plan names no numbers): at most this
    /// many entries show at once (more scroll), and it hangs this far under its button (and
    /// keeps this far in from the window's edges).
    pub const OUTLINE_PANEL_ROWS: usize = 12;
    pub const OUTLINE_PANEL_GAP: f32 = 4.0;
    /// The reading-progress line's height.
    pub const PROGRESS: f32 = 2.0;
    /// How far under the Preview's top a jumped-to heading's text lands.
    pub const JUMP_ROOM: f32 = 16.0;
    /// A heading counts as reached once it is this share of the Preview's height from its top.
    pub const REACH: f32 = 0.25;
    /// How long "Copied" shows.
    pub const COPIED: std::time::Duration = std::time::Duration::from_millis(1_500);
    /// The `#` left of a hovered heading, this far from its text.
    pub const ANCHOR_GAP: f32 = 6.0;
    /// "Copied" beside a heading, once its `#` is clicked.
    pub const ANCHOR_TEXT: f32 = 12.0;
    /// Between a callout's icon and its title (Task 11: was a literal).
    pub const CALLOUT_GAP: f32 = 6.0;
    /// A reader code block's border (its header's corners are this much less round).
    pub const CODE_BORDER: f32 = 1.0;
    /// A list level's indent (and the marker's column), and a number's gap to its text.
    pub const LIST_INDENT: f32 = 20.0;
    pub const MARKER_GAP: f32 = 6.0;
    /// A plain quote's padding left of its text, and its bar.
    pub const QUOTE_PAD: f32 = 12.0;
    pub const QUOTE_BAR: f32 = 3.0;
    /// A rule (`---`): its margin above and below, and its line.
    pub const RULE_MARGIN: f32 = 4.0;
    pub const RULE_LINE: f32 = 1.0;
    /// A Mermaid diagram's corners; the note under a Mermaid block shown as code: its gap
    /// above, its text and its line height (a multiple of the text).
    pub const DIAGRAM_RADIUS: f32 = 6.0;
    pub const NOTE_GAP: f32 = 4.0;
    pub const NOTE_TEXT: f32 = 12.0;
    pub const NOTE_LINE: f32 = 1.4;
}

/// Sizes in pixels, from the M11 plan's Global Constraints.
pub mod size {
    #![allow(dead_code)] // drawn with from M11 Task 2 on

    /// A sidebar row with its name line only, and one with a second line under it (who and
    /// where, what it waits on, a script's note, or "not live") (M11i: `sidebar::row_height`).
    /// 56 each since M11al (§2.3): every row has line two since M11r.
    pub const SIDEBAR_ROW: f32 = 56.0;
    pub const SIDEBAR_ROW_TWO: f32 = 56.0;
    /// A group heading: its 12 px words with 16 above and 8 below (M11al §2.5).
    pub const GROUP_HEADER: f32 = 36.0;
    /// A pane's tab bar (M11b; it replaced M11's 22 px pane header; 30 since M11i; 52 since
    /// M11al, level with the right panel's header).
    pub const TAB_BAR: f32 = 52.0;
    /// The band at the top of the window that the traffic lights sit in (the drag area); 52
    /// since M11al (38 before), level with the tab bars.
    pub const TOP_BAND: f32 = 52.0;
    /// Where the traffic lights go: (x, y) from the window's top left (centred in `TOP_BAND`
    /// since M11al).
    pub const TRAFFIC_LIGHTS: (f32, f32) = (18.0, 20.0);
    /// Terminal padding: (sides, top and bottom); smaller in splits. Enough that nothing the
    /// grid paints reaches into the card's rounded corner (M11i): the grid's corner cell starts
    /// inside the `CARD_RADIUS` curve.
    pub const TERMINAL_PADDING: (f32, f32) = (28.0, 22.0);
    pub const SPLIT_TERMINAL_PADDING: (f32, f32) = (20.0, 14.0);
    /// The floating card's corner radius (M11al §3.1; M11i's cards' was 8).
    pub const CARD_RADIUS: f32 = 14.0;
    /// M11al's one floating card: the canvas inset around it, its shadow's drop and blur, and
    /// the 1 px lines.
    pub const CARD_INSET: f32 = 8.0;
    pub const CARD_SHADOW_Y: f32 = 8.0;
    pub const CARD_SHADOW_BLUR: f32 = 28.0;
    pub const HAIRLINE: f32 = 1.0;
    /// The status bar along the window's bottom edge (M11i).
    pub const STATUS_BAR: f32 = 26.0;
    /// How far in from its card's left edge the first tab starts, in a pane's tab bar and in
    /// the right sidebar's panel tabs alike (2026-11-10, your call: the pane's had none; 12
    /// since M11al).
    pub const TABS_LEFT: f32 = 12.0;
    pub const SIDEBAR_MIN_WIDTH: f32 = 180.0;
    pub const SIDEBAR_MAX_WIDTH: f32 = 400.0;
    /// 290 since M11al (240 before); a saved width is kept.
    pub const SIDEBAR_DEFAULT_WIDTH: f32 = 290.0;
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

/// Text sizes in pixels. Numbers and meta use tabular figures; group headings are in capitals.
pub mod text {
    #![allow(dead_code)] // drawn with from M11 Task 2 on

    /// Names 14, the line under them 12.5, group headings 12 (M11al §11).
    pub const NAME: f32 = 14.0;
    pub const META: f32 = 12.5;
    pub const GROUP_HEADER: f32 = 12.0;
    /// M11al: tabs, search, buttons, right-panel rows; and shortcut keys.
    pub const UI: f32 = 13.5;
    pub const KEYS: f32 = 11.5;
}

const BLACK: Rgb = Rgb { r: 0, g: 0, b: 0 };
const WHITE: Rgb = Rgb {
    r: 255,
    g: 255,
    b: 255,
};

/// WCAG relative luminance (0 black, 1 white): each sRGB channel linearised, then weighted.
pub fn relative_luminance(c: Rgb) -> f32 {
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
    fn the_sidebars_band_and_default_width_are_the_uplifts() {
        use super::size::*;
        // M11al 2.1: the band is 52 with the lights centred in it; a new window's sidebar 290,
        // inside its limits.
        assert_eq!(TOP_BAND, 52.0);
        assert_eq!(TRAFFIC_LIGHTS, (18.0, 20.0));
        assert_eq!(SIDEBAR_DEFAULT_WIDTH, 290.0);
        assert!((SIDEBAR_MIN_WIDTH..=SIDEBAR_MAX_WIDTH).contains(&SIDEBAR_DEFAULT_WIDTH));
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
